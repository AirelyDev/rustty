# N456: host action callbacks that write the `Target` surface

This is an isolated, **non-FFI** reproduction of the borrow shape behind NEW-FINDINGS **N456**,
fixed in P7-W6 T3 (ruling 55). It is used to PROVE the fix. It follows the doctrine in memories
`rust-mut-args-strongly-protected` and `shaper-hook-sb-clean-interleave`.

## The bug

`rustty_apprt_api::Target<'s, C>` was `enum { App, Surface(&'s C) }`, passed BY VALUE to
`rt_app.perform_action(target, action)`. A reference field of a by-value argument is retagged at
function entry with a **protector** over every non-`UnsafeCell` byte of the core `Surface`, live for
the whole synchronous host action callback.

The macOS host writes that surface inside the callback:

- `reset_window_size` (**live**): `Rustty.App.swift` posts a notification; the observers run
  synchronously, `onResetWindowSize` -> `returnToDefaultSize` -> `apply(to:)` ->
  `window.setFrame(rect, display: true)` (`TerminalController.swift:1762`). The frame change lays the
  view out: `rustty_surface_set_size` -> embedded `updateSize(&mut self)` -> core
  `sizeCallback(&mut self)`.
- `resize_window` (latent until P7-W9 T2): `TerminalController.resizeWindow` calls
  `window.setFrame(frame, display: true)` inside the callback, the same write.

Zig's `*CoreSurface` carries no protector: port soundness only, no UD.

## The fix

`Target::Surface(SurfacePtr<'s, C>)`: a `NonNull<C>` plus `PhantomData<&'s C>`, no protector.
`Target::surface_ref(&C)` builds it from a reference (the reference is dead at return), and
`unsafe Target::surface_ptr(*const C)` from a raw root (the raw-self call sites). Consumers read
through it only transiently (`SurfacePtr::rt_surface_ptr`: `cval`, the embedded `set_title`
pre-action, GTK `widget()`). The C projection `rustty_target_s` is unchanged.

## The model

`src/main.rs` models the core `Surface`, the embedded `Surface` (with `core_surface` inline), the
embedded `App::perform_action` (pre-action read through the target, then the C projection and the
host callback), the macOS host callback (re-enters `rustty_surface_set_size` on the target), and the
three routes: `rustty_surface_key` -> `keyCallback` (raw-self, re-reads the surface after the
dispatch) -> `performBindingAction` (raw-self, local reborrow `this`), `rustty_surface_binding_action`
-> `performBindingAction`, and `rustty_app_tick` -> `handleMessage` -> `resizeWindow` (raw-self).
`Mode` selects the target shape.

## Results (`bash run-all.sh`, 16 seeds each, foreground, 2026-09-29)

| candidate | SB | TB |
|---|---|---|
| `n456_shipped_key_reset_window_size` | UB | UB |
| `n456_shipped_binding_reset_window_size` | UB | UB |
| `n456_shipped_resize_window` | UB | UB |
| `n456_fixed_ref_held_reset_window_size` (negative: raw target, `&C` held across the callback) | UB | UB |
| `n456_fixed_ref_held_resize_window` (negative, as above) | UB | UB |
| `n456_fixed_key_reset_window_size` | CLEAN | CLEAN |
| `n456_fixed_binding_reset_window_size` | CLEAN | CLEAN |
| `n456_fixed_resize_window` | CLEAN | CLEAN |
| `n456_fixed_all` (set_title pre-action, both resets, resize, a later binding) | CLEAN | CLEAN |

### Fix round 1 (ruling 57): the colour-scheme soft-reload chain

`colorSchemeCallback` (embedded and core) → `notifyConfigConditionalState` performs a soft
`reload_config` on the surface. The host's handler writes the surface synchronously: macOS
`rustty_surface_update_config` → `updateConfig`, GTK `reload_config_inner` → `updateConfig`. At the
T3 commit the target was already raw, but the three `&mut self` receivers were still protected
(`Mode::T3Receivers`). The fix makes the chain raw-self. Rerun 2026-09-30, 16 seeds:

| candidate | SB | TB |
|---|---|---|
| `n456_t3_macos_color_scheme` (`rustty_surface_set_color_scheme` entry) | UB | UB |
| `n456_t3_gtk_color_scheme` (GTK `color_scheme_thunk` over two surfaces) | UB | UB |
| `n456_fixed_macos_color_scheme` | CLEAN | CLEAN |
| `n456_fixed_gtk_color_scheme` | CLEAN | CLEAN |

The nine T3 rows above were rerun unchanged, and `n456_fixed_all` now also runs the macOS
colour-scheme path.

- Shipped, SB: `not granting access to tag .. because that would remove [SharedReadOnly for ..] which
  is strongly protected`, at `update_size(&mut self)`. TB: `write access .. is forbidden` at
  `size_callback`'s write.
- The negative rows show the consumer rule: materialising the whole `&C` from the pointer and keeping
  it across the host call is UB again.

The live route is exercised by
`crates/rustty-apprt-c/tests/macos_window_host.rs::reset_window_size_host_callback_resizes_the_target_surface`
(Miri cannot see across that FFI).
