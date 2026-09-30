# N441 / N442 / N443 / UD366: main-thread references to the core `Surface` and `App`

This is an isolated, **non-FFI** reproduction of the borrow shapes behind NEW-FINDINGS **N441**,
**N442**, **N443** and upstream defect **UD366** (first recorded as N448), fixed in P7-W7 T5b (round 0, then fix rounds 1 and 2 under rulings 43 and 46). It
is used to PROVE the fixes. It follows the doctrine in memories `rust-mut-args-strongly-protected`
and `shaper-hook-sb-clean-interleave`, and the precedent in `../mirihar-n437`.

## The bugs

- **N441.** The core `Surface` embedded the renderer by value. The render thread writes it under
  the draw mutex through its `*mut Renderer`. Every main-thread `&mut Surface` / `&Surface` the
  apprt forms (`sizeCallback`, `contentScaleCallback`, key, mouse, focus, clipboard, ...) retags
  every byte of the surface, so it races with those writes.
  - **Fix round 1.** Round 0's out-of-line box was freed in a `Drop` that never runs, because every
    owner frees the surface without drop glue. Every closed surface leaked its renderer allocation.
  - **Fix round 2.** Round 1 freed the box without drop glue, and `Renderer::deinit` left the backend
    to `Metal::drop`. So the layer's display callback was never cleared: the view-retained layer
    then ran it on the freed renderer at the next display pass. The fields only `Drop` frees (the
    callback context, queue, device, layer, EGL context, search matches, `Arc`s) also leaked.
- **N442.** `performBindingAction(&mut self)` and its callers `keyCallback` / `maybeHandleBinding`
  held a protected Unique across `rt_app.perform_action`. The macOS host's `new_split` / `new_tab` /
  `new_window` handlers re-enter the surface: `rustty_surface_inherited_config`, then
  `rustty_surface_new`.
- **N443.** The core `App` entries (`performAction`, `keyEvent`, `tick`, `newWindow`,
  `colorSchemeEvent`, ...) held `&mut self` or `&self` across host callbacks that write the app:
  - `reload_config` → `rustty_app_update_config` → `updateConfig(&mut self)`;
  - `new_*` → `rustty_surface_new` → `addSurface`;
  - `quit` → `needsConfirmQuit`.

  The embedded app had the same shape: `keyEvent(&self)` stayed protected across the whole key
  event, and `newSurface(&mut self)` retagged the app under `perform_action(&self)`.
- **UD366** (was N448). `performAllAction` iterated the live surface list while each surface's host callback can
  append to it. An append that reallocates the list frees the buffer under the loop.

## The fixes

- **N441.** `renderer_seam::RendererBox`: the renderer in its own allocation, shared with the render
  thread through one raw root. There is no `Drop`. `Renderer::deinit` calls `Metal::deinit`, which
  clears the layer's display callback and frees its context, as the reference's `layer.release()`
  does. `dealloc_after_deinit` then drops the renderer normally (round 2), right after `deinit`.
- **N442.** The three surface entries are raw-self. Each uses an unprotected local reborrow that is
  dead across any call that can reach the host, and the re-entrant arms work by place. The C routes
  enter through `&raw mut (*ptr).core_surface`.
- **N443.**
  - The core `App` entries are raw-self (`self_: *mut App`), reaching the app only by place in short
    reborrows.
  - The embedded `keyEvent` / `newSurface` are raw-self.
  - `rt_app: &A` stays a shared reference, because `perform_action` is a `&self` trait method.
    Nothing writes the embedded app's non-`UnsafeCell` bytes during a callback.
- **UD366.** The surface arm iterates a snapshot of the list and skips a surface that is gone.

## The model

`src/main.rs` includes the REAL `rustty_zigstd::thread::Mutex` and the REAL
`rustty_renderer::latest_frame::DrawMutex` by `#[path]`.

- The N441 race candidates run two threads.
- The leak arms rely on Miri's leak check.
- The N442/N443/UD366 candidates are single-threaded. They model the C routes, the embedded app and
  surface (with `core_surface` inline), the `Target` enum holding a shared reference, and the macOS
  host callback.
  - The callback re-enters with the host's own app pointer, as `Rustty.App.app` does:
    `inherited_config` then `rustty_surface_new` for `new_*`, `rustty_app_update_config` for
    `reload_config`, and `rustty_app_needs_confirm_quit` for `quit`.
  - A thread-local `Mode` selects the code: `Shipped` (before T5b), `Round0` (T5b round 0) or
    `Fixed` (fix round 1).

Each run explores 16 scheduler seeds (`-Zmiri-many-seeds=0..16`).

## Candidates and results (fix round 2, 2026-09-29)

| candidate | what it models | SB | TB |
|---|---|---|---|
| `n441_inline_renderer` | whole-`&mut Surface` main callbacks vs render-thread locked writes to the INLINE renderer | **UB** | **UB** |
| `n441_boxed_renderer` | the fix: the renderer out of line; main-thread locked narrow renderer entry included | **CLEAN** | **CLEAN** |
| `n441_residual_terminal_inline` | N444 (CLOSED in W7.T5c; full proof `../mirihar-n444`): the inline terminal vs io-thread locked writes | **UB** | **UB** |
| `n441_residual_terminal_boxed` | the same fix shape applied to the terminal | **CLEAN** | **CLEAN** |
| `n441_box_drop_leak` | round 0: the box freed in `Drop`, the surface freed through `ManuallyDrop` | **LEAK** | **LEAK** |
| `n441_box_dealloc_after_deinit` | fix round 1: `deinit`, then `dealloc_after_deinit` | **CLEAN** | **CLEAN** |
| `n441_r1_drop_only_fields_leak` | round 1: no backend `deinit`, the renderer freed as `ManuallyDrop`: the callback context and `Drop`-only fields | **LEAK** | **LEAK** |
| `n441_r1_display_after_free` | round 1, then a display pass on the view-retained layer | **UB** | **UB** |
| `n441_r2_deinit_then_drop` | round 2: `Metal::deinit` in `Renderer::deinit`, then a normal drop, then a display pass | **CLEAN** | **CLEAN** |
| `n442_shipped_key_route` | before T5b: `rustty_surface_key` → `keyCallback(&mut)` → … → host | **UB** | **UB** |
| `n442_shipped_binding_new_split` | before T5b: `performBindingAction(&mut)` `new_split` | **UB** | CLEAN |
| `n442_shipped_binding_new_tab` | as above, `new_tab` | **UB** | CLEAN |
| `n442_shipped_binding_new_window` | as above, `new_window` (with `App::newWindow(&mut)`) | **UB** | **UB** |
| `n442_raw_this_spans` | NEGATIVE: raw-self, but the local `this` (written before) is used after the callback | **UB** | **UB** |
| `r0_binding_new_window` | round 0: `App::newWindow(&self)` while `rustty_surface_new` appends to `surfaces` | **UB** | **UB** |
| `r0_key_route_new_split` | round 0: embedded `keyEvent(&self)` + `newSurface(&mut self)` | **UB** | CLEAN |
| `r0_binding_reload_config` | round 0: `App::performAction(&mut self)` `reload_config` → `updateConfig` | **UB** | **UB** |
| `r0_binding_quit` | round 0: `App::performAction(&mut self)` `quit` → `needsConfirmQuit` | **UB** | CLEAN |
| `r0_app_tick_reload_config` | round 0: `rustty_app_tick` → `performAction(&mut)` `reload_config` | **UB** | **UB** |
| `r0_app_key_global_reload_config` | round 0: `rustty_app_key` → `App::keyEvent(&mut)` global `reload_config` | **UB** | **UB** |
| `r0_all_new_split_live_iter` | UD366: two surfaces, a full list, `all:new_split` over the live list | **UB** | **UB** |
| `fixed_key_route_all` | the fix: the key route, every action | **CLEAN** | **CLEAN** |
| `fixed_binding_all` | the fix: the binding/menu routes, every action | **CLEAN** | **CLEAN** |
| `fixed_app_tick_all` | the fix: `rustty_app_tick` → `reload_config`, `quit`, `new_window` | **CLEAN** | **CLEAN** |
| `fixed_app_key_global_all` | the fix: global keybinds `new_split`/`new_tab`/`new_window`/`reload_config`/`quit` | **CLEAN** | **CLEAN** |
| `fixed_all_new_split_snapshot` | the UD366 fix on the same full-list world | **CLEAN** | **CLEAN** |

- **Messages behind each verdict.**
  - N441 races: "Data race detected … retag".
  - Protector rows under SB: "not granting access … strongly protected"; under TB: "write access
    through … is forbidden".
  - UD366 and the round-2 display pass: "alloc has been freed, so this pointer is dangling".
  - LEAK: Miri's "memory leaked" report.
- **Why some rows are UB under SB but CLEAN under TB.** In those rows, the re-entry only reads, or
  only retags `&mut` without writing. A TB `Reserved`/`Frozen` tag tolerates that. The SB verdict
  alone makes each one a defect.
- **Harness history.** The host model uses the host's own app pointer, not the callback's `app`
  argument (the host only hands that to `rustty_app_userdata`). An earlier version of the model used
  the callback's argument, which derives from `perform_action`'s `&self` and gave a spurious SB
  report.
- **Flaky exit.** Some runs print a many-seeds child `Bus error: 10` after their UB report. A rerun
  reports the same UB with exit code 1.

Measured with `miri 0.1.0 (6f72b5dd5f 2026-07-22)` on a macOS host.

## Run

```sh
bash run-all.sh                                   # every candidate x SB/TB x 16 seeds
cargo +nightly miri run -q -- fixed_binding_all   # one candidate, SB
```
