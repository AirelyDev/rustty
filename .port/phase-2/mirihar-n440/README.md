# N440 — the Metal completion handler vs the render thread's whole-renderer `&mut`

Isolated, **non-FFI** reproduction of the borrow shape behind NEW-FINDINGS **N440**
(P7-W7 T2 fix round 1; fixed by P7-W7 T5a), used to PROVE the fix (doctrine: memories
`shaper-hook-sb-clean-interleave`, `rust-mut-args-strongly-protected`; precedent
`../mirihar-n437`).

## The bug

A Metal command buffer drawn with `sync = false` completes on a Metal-owned thread,
which runs the completion handler `metal_buffer_completed` (the port of
`metal/Frame.zig` `bufferCompleted`) WITHOUT the draw mutex. The handler recovered the
renderer from the context's type-erased pointer and reached its bytes:
`(*renderer).api.present(&*ctx.target, ..)` (a `&mut` reborrow of `api`, and a `&Target`
into `swap_chain.frames[i]`), then `frame_completed_raw` (places on `health`,
`surface_mailbox` and `swap_chain`). Meanwhile the render thread may hold a
whole-renderer `&mut` under the lock (`Renderer::with_locked`: `set_font_grid_locked`,
the locked tail of `update_frame_impl`, the search-match writes), whose retag and
accesses cover the same bytes. The reference's `*Renderer` carries no aliasing rule.

## The fix

The completion context (`crate::metal::frame::CompletionContext`) carries owned clones
of exactly what the handler needs, each in an allocation of its own, taken under the
draw mutex when the frame begins (`draw_frame_locked`):

- `health: Arc<AtomicHealth>` (the renderer's `health` field is now that `Arc`);
- `frame_sema: Arc<Semaphore>` (the swap chain's `frame_sema` is now that `Arc`);
- `surface_mailbox`: a `SurfaceMailbox` clone (its sink is already an `Arc`);
- `layer: Arc<LayerCell>`: Metal's presenter cell (`Metal::layer`), which is all
  `present` reads besides the target;
- `surface`: a retain of the target's IOSurface, the only part of the `Target`
  `present` reads.

- `health_retry: Arc<HealthReportRetry>`: the UD362 retry (a redraw flag and the render
  thread's `draw_now` wakeup).

The handler (`metal_buffer_completed` → `LayerCell::present` + `frame_completed_on`)
forms no reference to any renderer byte. Fix round 1 (ruling 39): it keeps the
reference's order (store, push, release) with an `.instant` push; a refused report is
withdrawn (compare-exchange back) and a redraw requested (UD362). The model does the
same, with a one-slot app mailbox so the refusal path runs, and it tears the renderer
down (wait for every release, take the chain, free the box) BEFORE joining the
completion thread, since after its release a completion may still be dropping its
clones.

## The model

`src/main.rs` includes the REAL `rustty_zigstd::thread::{Mutex, Semaphore}` and the
REAL `rustty_renderer::latest_frame::DrawMutex` by `#[path]`. Three threads: the
render thread (per round, a `with_locked` whole-`&mut` entry, then an async draw that,
under the lock, waits on `frame_sema` in `next_frame` and hands the frame to Metal),
the Metal completion thread (runs each committed frame's handler, unlocked), and, in
`fixed_async_and_sync_draw`, the main thread's CoreAnimation sync draw (narrow, running
the same handler inline under the lock). Two swap-chain frames and four rounds, so
`next_frame` really waits for completions. The main thread's sync draw also drains the
app mailbox between its draws. The scheduler is explored over 16 seeds
(`-Zmiri-many-seeds=0..16`).

## Candidates & results

| candidate | models | SB | TB |
|---|---|---|---|
| `shipped_async_handler` | the pre-fix handler (renderer pointer, `&mut api`, `&Target` into the swap chain, `frame_completed_raw` places) vs the render thread's locked whole `&mut` | **UB** | **UB** |
| `fixed_async_handler` | the fixed handler (context clones only) vs the same render thread | **CLEAN** | **CLEAN** |
| `fixed_async_and_sync_draw` | the fixed handler on the completion thread AND inline in a main-thread sync draw, vs the same render thread | **CLEAN** | **CLEAN** |

The UB is an aliasing-model violation: SB "not granting access to tag … because that
would remove [Unique for …] which is strongly protected" (the render thread's
`with_locked` retag against the handler's protected `&mut self` of `present`, or the
reverse); TB "Data race detected between (1) retag read on thread … and (2) non-atomic
write …" / "(1) atomic store … and (2) retag read of type `Shipped`".

Measured 2026-09-29, `miri 0.1.0 (6f72b5dd5f 2026-07-22)`, macOS host: the shipped candidate was run
before the port change, the fixed candidates model the change as landed; re-run after fix round 1
(new order, refusal path, teardown before the join) with the same results.

## Run

```sh
bash run-all.sh                                        # 3 candidates x SB/TB x 16 seeds
cargo +nightly miri run -q -- shipped_async_handler    # one candidate, SB
```
