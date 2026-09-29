# N437 — whole-`Renderer` references across the render and main threads

Isolated, **non-FFI** reproduction of the borrow shape behind NEW-FINDINGS **N437**
(P7-W7 T2), used to PROVE the fix (doctrine: memories `shaper-hook-sb-clean-interleave`,
`rust-mut-args-strongly-protected`; precedents `../mirihar-n431`, `../mirihar-n434`).

## The bug

The render thread (`rustty-core` `renderer_seam::Thread`) called the renderer through
whole-`Renderer` receivers: `update_frame(&mut self)`, `animation_wake(&self)` unlocked,
`set_visible`, `set_focus`, `loop_enter`, `set_macos_display_id`, `set_font_grid`,
`set_screen_size`, `change_config`, `thread_enter`/`thread_exit`. On macOS the main
thread draws at the same time (`draw_frame_raw(sync = true)` from the CoreAnimation
display callback), through a raw pointer, under the inline `draw_mutex`, and reborrows
the whole renderer inside it. A function-entry `&mut self`/`&self` is protected for the
whole call, so any concurrent main-thread access to the renderer's bytes — the inline
mutex's own bytes included — removes a protected tag (Stacked Borrows) or races the
protected retag (Tree Borrows). `draw_mutex` cannot help: the protector exists before
the lock is taken, and the waiter's `&Mutex` is itself over renderer bytes. After the
GtkGLArea redesign (`2f0b65346`) the same holds on Linux: the GTK main thread's
`displayRealized`/`displayUnrealized` and the `RenderSurface`'s `takeFrame` run while
the render thread draws.

## The fix

- `draw_mutex` lives in its own allocation (`rustty_renderer::latest_frame::DrawMutex`,
  an `Arc<Mutex>` handle). Core takes one handle for the render thread
  (`Thread::draw_mutex`) and one for the main thread (`renderer_seam::RendererHandles`,
  beside the renderer, not in it) before the render thread starts.
- Every cross-thread entry locks through the caller's handle BEFORE it forms any
  reference to the renderer (`Renderer::with_locked` and the `*_raw` entries in both
  `generic` twins); whole-renderer `&mut` exists only under the lock. Entries the
  reference runs unlocked (`setFocus`, `setVisible`'s field write, `animationWake`,
  `hasVsync`, `loopEnter`, `setMacOSDisplayID`, the search-match writes) take the lock
  too; the display link's CoreVideo calls still run after the unlock (UD352,
  `72cf50855`), through a `DisplayLinkHandle` copied out under the lock.
- The latest-frame slot (`LatestFrame`) is behind an `Arc`; the main thread takes
  frames through its own handle, touching no renderer byte.
- The macOS display callback's context carries the renderer pointer AND a
  `DrawMutex` handle (`metal_impl::DisplayCallbackCtx`), projected from the raw
  root, not a reborrow.

## The model

`src/main.rs` includes the REAL `rustty_zigstd::thread::Mutex` and the REAL
`rustty_renderer::latest_frame` (`DrawMutex`, `LatestFrame`) by `#[path]`. `Shipped`
is the pre-fix renderer (inline mutex, whole-renderer receivers, the raw sync draw
with whole-`&mut` helpers under the lock). `Fixed` is the fixed shape: every
render-thread entry of `renderCallback`/`drainMailbox`/`armAnimationTimer` through
`with_locked` with the thread's handle, the draw pushing frames into the `Arc`ed
slot; the main side is either the macOS sync draw or the GTK main thread
(unrealize/realize flags + `takeFrame`). Each run does 3 rounds per thread; the
scheduler is explored over 16 seeds (`-Zmiri-many-seeds=0..16`).

## Candidates & results

| candidate | models | SB | TB |
|---|---|---|---|
| `shipped_update_frame` | render `update_frame(&mut self)` vs main raw sync draw | **UB** | **UB** |
| `shipped_animation_wake` | render unlocked `animation_wake(&self)` vs main raw sync draw | **UB** | **UB** |
| `shipped_set_focus` | render `set_focus(&mut self)` vs main raw sync draw | **UB** | **UB** |
| `inline_lock_whole_mut` | NEGATIVE: lock-first through a narrow `&Mutex`, but the mutex INLINE | **UB** | **UB** |
| `latest_frame_inline` | NEGATIVE: external lock, but the frame slot INLINE (`takeFrame` through the renderer) | **UB** | **UB** |
| `fixed_vs_sync_draw` | the fix vs the macOS main-thread sync draw | **CLEAN** | **CLEAN** |
| `fixed_vs_gtk_main` | the fix vs the GTK main thread (realize flags + `takeFrame`) | **CLEAN** | **CLEAN** |

Every UB row is an aliasing-model violation (SB: "not granting access to tag … which
is strongly protected"; TB: a data race between a retag and the other thread's
access). The two negatives show each half of the fix is load-bearing: locking first
is not enough while the mutex is inline, and the external lock does not cover an
inline frame slot.

Measured 2026-09-28, `miri 0.1.0 (6f72b5dd5f 2026-07-22)`, macOS host.

## Run

```sh
bash run-all.sh                                   # 7 candidates x SB/TB x 16 seeds
cargo +nightly miri run -q -- fixed_vs_gtk_main   # one candidate, SB
```
