# N441 / N442: main-thread whole-core-`Surface` references

This is an isolated, **non-FFI** reproduction of the borrow shapes behind NEW-FINDINGS
**N441** and **N442**, fixed in P7-W7 T5b. It is used to PROVE the fixes. It follows the
doctrine in memories `rust-mut-args-strongly-protected` and `shaper-hook-sb-clean-interleave`,
and the precedent in `../mirihar-n437`.

## The bugs

- **N441.** The core `Surface` embedded the renderer by value. The render thread writes it under
  the draw mutex, through the `*mut Renderer` it gets at `Thread.init`. The main thread calls the
  apprt's core entries through a whole `&mut Surface` or `&Surface`:
  - GTK `render_surface_resize` → `contentScaleCallback` / `sizeCallback`;
  - key, mouse, focus and clipboard callbacks;
  - the embedded C API.

  Every such retag covers every byte of the surface, including the renderer. That makes it a data
  race with the render thread's locked writes, under Stacked and Tree Borrows. The draw mutex does
  not help, because the retag happens without it.
- **N442.** `performBindingAction(&mut self)` and its callers `keyCallback(&mut self)` and
  `maybeHandleBinding(&mut self)` held a protected Unique across `rt_app.perform_action`. The macOS
  host's `new_split` / `new_tab` / `new_window` handlers call `rustty_surface_inherited_config`
  inside that callback, which reads `core_surface.font_size` and the pwd through the parent
  pointer. That is a foreign read of a protected Unique.

  `App::newWindow(&mut self)` had the same shape: the host also reads `core_app.alloc`.

## The fixes

- **N441.** The renderer moves into its own allocation, `renderer_seam::RendererBox`, created with
  `Box::into_raw` before `Thread::init`. The render thread and the surface share that one raw root,
  so a whole-surface retag now covers only the pointer. Main-thread renderer entries go through
  `RendererBox::as_ptr()` and lock first, per N437.
- **N442.** `performBindingAction`, `keyCallback` and `maybeHandleBinding` are raw-self
  (`self_: *mut Surface`).
  - Each one takes an **unprotected** local reborrow, `this`. That reborrow is dead before any call
    that can run the host callback, and it is taken afresh after.
  - The re-entrant arms touch the surface only by place through `self_`, plus the shared
    `Target::Surface(&*self_)`. A read-only re-entry is sound under a shared protector.
  - `App::newWindow` takes `&self`.
  - The C routes (`rustty_surface_key`, `_binding_action`, `_split`, `_split_focus`,
    `_request_close`) enter through `&raw mut (*ptr).core_surface`.

## The model

`src/main.rs` includes the REAL `rustty_zigstd::thread::Mutex` and the REAL
`rustty_renderer::latest_frame::DrawMutex` by `#[path]`. The N441 candidates run two threads. The
N442 and N443 candidates are single-threaded and model the C routes, the embedded surface (with
`core_surface` inline), the `Target` enum holding a shared reference, and the host callback that
re-enters. Each run explores 16 scheduler seeds (`-Zmiri-many-seeds=0..16`).

## Candidates and results

| candidate | what it models | SB | TB |
|---|---|---|---|
| `n441_inline_renderer` | whole-`&mut Surface` main callbacks vs render-thread locked writes to the INLINE renderer | **UB** | **UB** |
| `n441_boxed_renderer` | the fix: the renderer out of line (`RendererBox`); main-thread locked narrow renderer entry included | **CLEAN** | **CLEAN** |
| `n441_residual_terminal_inline` | the same class beyond the renderer (N444): the inline terminal vs io-thread locked writes | **UB** | **UB** |
| `n441_residual_terminal_boxed` | the same fix shape applied to the terminal | **CLEAN** | **CLEAN** |
| `n442_shipped_key_route` | `rustty_surface_key` → `keyCallback(&mut)` → `maybeHandleBinding(&mut)` → `performBindingAction(&mut)` → host → `inherited_config` | **UB** | **UB** |
| `n442_shipped_binding_new_split` | `rustty_surface_split` / `_binding_action` → `performBindingAction(&mut)` `new_split` | **UB** | CLEAN |
| `n442_shipped_binding_new_tab` | as above, `new_tab` | **UB** | CLEAN |
| `n442_shipped_binding_new_window` | as above, `new_window` (with `App::newWindow(&mut)`) | **UB** | CLEAN |
| `n442_new_window_app_mut` | raw-self surface, but `App::newWindow(&mut self)` while the host reads `core_app.alloc` | **UB** | CLEAN |
| `n442_raw_this_spans` | NEGATIVE: raw-self, but the local `this` (written before) is used after the callback | **UB** | **UB** |
| `n442_raw_key_route` | the fix: the key route, all four actions | **CLEAN** | **CLEAN** |
| `n442_raw_binding_all` | the fix: the binding/menu routes, all four actions | **CLEAN** | **CLEAN** |
| `n443_quit_app_mut` | N443 (OPEN): `App::performAction(&mut self)` `.quit` while the host reads `needsConfirmQuit` | **UB** | CLEAN |
| `n443_quit_app_ref` | the N443 fix direction (`&App`) | **CLEAN** | **CLEAN** |

The N441 UB rows are data races between a main-thread retag and the other thread's access. Under
SB Miri reports "retag write … retag write of type `Renderer`"; under TB it reports "retag read …
non-atomic write". The N442 UB rows under SB say "not granting access to tag … because that would
remove [Unique …] which is strongly protected".

Several rows are **UB** under SB and **CLEAN** under TB. In those candidates the protected Unique
has not been written before the callback, and a Tree Borrows `Reserved` tag tolerates a foreign
read. The key route writes first (`keyCallback` / `maybeHandleBinding` reset `pressed_key`,
`last_trigger` and `sequence_set`), so it is UB under both models. The Stacked Borrows verdict
alone is enough to make it a defect.

The negative `n442_raw_this_spans` shows that raw-self alone is not enough. The local reborrow must
be dead across the callback. That is why `maybeHandleBinding` re-takes it lazily after the dispatch,
once the closing check has run.

Measured 2026-09-29 with `miri 0.1.0 (6f72b5dd5f 2026-07-22)` on a macOS host. One SB run of
`n442_shipped_binding_new_tab` printed a `Bus error: 10` from a many-seeds child after its UB
report. On rerun it reports the same UB with a clean exit code of 1.

## Run

```sh
bash run-all.sh                                        # 14 candidates x SB/TB x 16 seeds
cargo +nightly miri run -q -- n442_raw_key_route       # one candidate, SB
```
