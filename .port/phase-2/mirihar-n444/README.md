# N444 (and N449): the core `Surface`'s other cross-thread state

This is an isolated, **non-FFI** reproduction of the borrow shapes behind NEW-FINDINGS **N444**,
fixed in P7-W7 T5c, and **N449**, filed OPEN by the same task. It is used to PROVE the fix. It
follows the doctrine in memories `rust-mut-args-strongly-protected` and
`shaper-hook-sb-clean-interleave`, and the precedent in `../mirihar-n441` (N441, the renderer).

## The bug (N444)

N441 moved the renderer out of the core `Surface`. Five more blocks stayed inline, and another
thread writes each of them through a raw pointer into the surface:

| block | written by |
|---|---|
| `io: Termio` | the io read thread (`processOutput`: `io.terminal` under `renderer_state.mutex`) |
| `io_thread: termio::Thread` | the io thread (its loop state) |
| `renderer_thread: renderer::Thread` | the render thread (its loop state) |
| `renderer_state: renderer::State` | the io and render threads (the D4 handoff atomics `demand`, `handoff_gen`) |
| `search.state: search::Thread` | the search thread (its loop state) |

Every main-thread `&mut Surface` / `&Surface` the apprt forms (`sizeCallback`,
`contentScaleCallback`, key, mouse, focus, clipboard, ...) retags every byte of the surface, so it
races with those writes. Zig's `*Surface` carries no aliasing rule: port soundness only, no UD.

Two narrower shapes on the main thread's own io entry (`queueIo`) had the same problem:

- `self.io.queueMessage(..)` took `&mut Termio`: a protected Unique over the whole `Termio`,
  terminal included, while the read thread writes the terminal.
- `Mailbox::send(&mut self)` / `notify(&mut self)`: under SB, the main thread's `&mut Mailbox` pops
  the io thread's drain tag (`match &(*io).mailbox { .. queue.pop() }`), which is protected.

## The fix

- Each block lives in its own allocation, `rustty_core::thread_shared::ThreadSharedBox<T>`, an
  owned raw pointer shared with the other thread through one raw root. A whole-surface retag now
  covers the pointer's 8 bytes only. Main-thread accesses go through `as_ptr()` by place, locked
  where the reference locks.
- `Termio::queueMessage(self_: *mut Termio, ..)` is raw-self (the reference's `self: *Termio`), and
  `Mailbox::send` / `notify` take `&self` (`BlockingQueue::push` and `xev::Async::notify` are
  already `&self`).
- `ThreadSharedBox` has no `Drop`: every owner frees the surface without drop glue (embedded
  `closeSurface` through `ManuallyDrop`, GTK through `dealloc`, the init-error callers through the
  `MaybeUninit` slot). Each teardown site tears the block down exactly as it did inline, then
  `ThreadSharedBox::dealloc` frees the allocation, also without glue.

## Fix round 1 (ruling 48): the search thread's callback

`searchCallback_` runs on the search thread with the raw `event_userdata`, which was the surface. It
read the surface's own bytes: the `renderer_thread` box pointer (for the renderer mailbox and
wakeup) and `rt_app` / `app` (through `surface_mailbox_raw`). Those are non-atomic reads of `Surface`
bytes while the main thread forms `&mut Surface` (key, mouse, focus, `deinit` before
`search.take()`). Under SB the main thread's Unique retag is a write, so this is a data race; under TB
the retag is a read, so it is clean there. Each writer thread in the round-0 model read only its own
box, so the model did not cover this.

The fix: a small boxed `SearchCallbackCtx` captured on the main thread at spawn (the renderer
mailbox pointer, the renderer wakeup pointer, a copy of the surface mailbox) is the userdata. The
callback never dereferences the surface. `Search::deinit` frees it once after the join, and the
`Thread::init` and spawn failure paths free it when no thread ran.

## N449 (OPEN, not the Surface)

The io writer thread calls `&mut self` `Termio` methods (`focusGained`, `resize`, `changeConfig`,
`sizeReport`, ...). Each is a protected Unique over the whole `Termio`, terminal included, while the
io read thread writes the terminal under the mutex (`processOutput`, raw-self). This is independent
of the surface layout. Fix direction: the `processOutput` shape (raw-self, narrow access).

## The model

`src/main.rs` includes the REAL `rustty_zigstd::thread::Mutex` by `#[path]`. Each writer thread
touches only its own block, through its raw root, as the port's `threadMain`s do (all raw-self).
The inline candidates run one writer at a time, so each verdict names one block. `n444_boxed_all`
runs every writer at once against the main thread's realistic entries: whole-surface callbacks,
`queueIo`, a locked terminal write by place, a locked `preedit` write, and a `queueRender` notify.

The teardown arms rely on Miri's leak check. `n444_teardown_all_paths` runs close → `deinit` under
each owner (ManuallyDrop, dealloc, MaybeUninit) and every init-error stage (`Renderer::init`,
`renderer::Thread::init` and `termio::Thread::init` failing with the slots unwritten, then
`initErrdefers` with io dead and with io live).

## Results (`bash run-all.sh`, 16 seeds each, foreground)

| candidate | SB | TB |
|---|---|---|
| `n444_inline_io_terminal` | UB | UB |
| `n444_inline_io_thread` | UB | UB |
| `n444_inline_renderer_thread_state` | UB | UB |
| `n444_inline_renderer_state_atomics` | UB | UB |
| `n444_inline_search_state` | UB | UB |
| `n444_boxed_all` (the fix) | CLEAN | CLEAN |
| `n444_boxed_queue_io_termio_mut` | UB | UB |
| `n444_boxed_queue_io_send_mut` | UB | CLEAN |
| `n444_drop_box_leak` (negative) | LEAK | LEAK |
| `n444_init_error_io_slot_leak` (negative) | LEAK | LEAK |
| `n444_teardown_all_paths` (the fix) | CLEAN | CLEAN |
| `n444_search_cb_reads_surface` (fix round 1: round-0 callback) | UB | CLEAN |
| `n444_search_cb_ctx` (fix round 1: the spawn-time context) | CLEAN | CLEAN |
| `n449_writer_termio_mut` | UB | UB |
| `n449_writer_termio_raw` | CLEAN | CLEAN |

- The inline and `termio_mut` verdicts are data races between the main thread's retag and the
  other thread's access (SB: "retag write" vs a non-atomic read; TB: "retag read" vs a non-atomic
  write).
- `send_mut` under SB: "not granting access to tag … because that would remove [Unique …] which is
  strongly protected". TB tolerates it: the protected `&mut Mailbox` is never written.
- The N449 verdicts are the same data race, between the writer thread's `&mut Termio` retag and the
  read thread's terminal write.
- `n444_search_cb_reads_surface` under SB: "Data race detected between (1) retag write on thread
  `main` and (2) retag read of type `ThreadSharedBox<RenderThread>` on thread `unnamed-1`".
- One SB run of `n444_inline_search_state` printed a many-seeds child `Bus error: 10`; its log
  carries the UB report, and the rerun gives the same verdict.
