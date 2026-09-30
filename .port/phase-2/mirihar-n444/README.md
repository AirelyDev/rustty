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

## N449 (CLOSED in P7-W7 T5d, not the Surface)

The io writer thread called `&mut self` `Termio` methods (`focusGained`, `resize`, `changeConfig`,
`sizeReport`, `queueWrite`, ..., and `threadExit` after its loop). Each was a protected Unique over
the whole `Termio`, terminal included, while the io read thread writes the terminal under the mutex
(`processOutput`, raw-self). This is independent of the surface layout. The same methods reached the
backend through `&mut Backend` -> `&mut Exec` -> `&mut Subprocess` -> `&mut Pty` (`resize` ->
`setSize`, `threadExit` -> `stop`), while the main thread read the backend: `getForegroundPid`,
`getTtyName` (which fills the pty's tty-name cache) and the close-confirm read of
`subprocess.args` (a whole `&Backend`).

The fix (T5d), modelled by `run_n449`:

- every io writer-thread `Termio` method is raw-self (`self_: *mut Termio`) and reaches only the
  fields the reference touches, by place; `changeConfig` borrows the terminal only after the stream
  handler has written it through its own pointer;
- `Backend` is `#[repr(transparent)]`, and `Backend::exec_ptr` is the `.exec => |*exec|` projection
  as a pointer cast; the cross-thread `Backend` / `Exec` / `Subprocess` methods are raw-self;
- the pty's `setSize` / `getForegroundPid` / `getTtyName` take `&self`, and its tty-name cache is
  interior-mutable (`UnsafeCell` buffer, `Cell` length), so a shared pty reference covers none of
  the bytes the main thread writes;
- the close-confirm read borrows `subprocess.args` only.

The partial arms each keep one shipped piece, to show every piece is needed:
`n449_main_backend_mut` (the main thread's T5c `&mut Backend` chain), `n449_pty_set_size_mut`
(`setSize(&mut self)`), `n449_close_confirm_backend_ref` (a whole `&Backend`), and
`n449_change_config_terminal_early` (the terminal borrowed before the handler writes it).

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
| `n449_shipped` (T5d: the shipped writer methods, backend chain and close-confirm read) | UB | UB |
| `n449_main_backend_mut` (T5d partial: main thread's `&mut Backend` chain) | UB | UB |
| `n449_pty_set_size_mut` (T5d partial: `Pty::setSize(&mut self)`) | UB | UB |
| `n449_close_confirm_backend_ref` (T5d partial: a whole `&Backend`) | UB | UB |
| `n449_change_config_terminal_early` (T5d negative: terminal borrowed before the handler write) | UB | UB |
| `n449_fixed` (T5d: the fix) | CLEAN | CLEAN |
| `ud367_start_unordered` (T5d fix round 1: `start` writes the pty, main reads it) | UB | UB |
| `ud367_ready_in_subprocess` (the flag inside `Subprocess`, under `start`'s `&mut`) | UB | CLEAN |
| `ud367_ready_in_exec` (the fix: `Exec.pty_ready`, Release / Acquire) | CLEAN | CLEAN |
| `ud368_size_before_lock` (T5d fix round 1: `resize` stores `size` before the mutex) | UB | UB |
| `ud368_size_under_lock` (the fix: the store inside the mutex section) | CLEAN | CLEAN |

UD367 and UD368 (fix round 1, ruling 52) are upstream data races, not aliasing artefacts: the
unordered arms race on plain reads and writes. `run_ud367` models `Exec::threadEnter`'s
`subprocess.start` against a main-thread `getTtyName` / `getForegroundPid`; `run_ud368` models
`Termio::resize`'s `size` store against the read thread's `.enable_mode_3` read through the stream
handler's `size` pointer, under the renderer mutex. `ud367_ready_in_subprocess` under SB: "Data race
detected between (1) atomic load on thread `main` and (2) retag write of type `Ud367Subprocess`",
which is why the flag lives in `Exec`.

- The inline and `termio_mut` verdicts are data races between the main thread's retag and the
  other thread's access (SB: "retag write" vs a non-atomic read; TB: "retag read" vs a non-atomic
  write).
- `send_mut` under SB: "not granting access to tag … because that would remove [Unique …] which is
  strongly protected". TB tolerates it: the protected `&mut Mailbox` is never written.
- The N449 verdicts are the same data race, between the writer thread's `&mut Termio` retag and the
  read thread's terminal write.
- T5d: `n449_shipped` under SB: "not granting access to tag … because that would remove
  [SharedReadOnly …] which is strongly protected"; under TB a data race between the main thread's
  terminal write and the writer's `N449Termio` retag. The three partial arms are data races between a
  retag of `N449Backend` / `Option<N449Pty>` on one thread and a non-atomic access on the other.
  `n449_change_config_terminal_early` is single-thread aliasing: the handler's write through its own
  pointer invalidates the earlier terminal borrow (SB: "tag does not exist in the borrow stack"; TB:
  "write access through … is forbidden").
- `n444_search_cb_reads_surface` under SB: "Data race detected between (1) retag write on thread
  `main` and (2) retag read of type `ThreadSharedBox<RenderThread>` on thread `unnamed-1`".
- One SB run of `n444_inline_search_state` printed a many-seeds child `Bus error: 10`; its log
  carries the UB report, and the rerun gives the same verdict.
