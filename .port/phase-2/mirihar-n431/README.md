# N431 / N430 — loop re-entry from callbacks while `Loop::run(&mut self)` is live

Isolated, **non-FFI** reproduction of the borrow structure behind NEW-FINDINGS
**N431** (every rustty-xev backend) and **N430** (WASI `async_notify`), used to
PROVE the raw-self fix (doctrine: memory `shaper-hook-sb-clean-interleave`;
precedents `../mirihar-n385`, `../mirihar-xev-mpsc`). The real backends cannot be
Miri-checked end to end: kqueue/epoll/io_uring/IOCP/`poll_oneoff` are syscalls.

## The bug

Every backend ported libxev's `run(self: *Loop)` / `tick` / `tick_inner` as
`&mut self`. That `&mut Loop` is a function-entry (PROTECTED) borrow for the
whole run, and every callback runs inside it. Callbacks re-enter the loop:

- **through the stored root** — termio's normal write path: `thread.rs`
  `(*self_).loop_.run(..)`, the wakeup callback drains the mailbox, `exec.rs`
  `td.loop_` (= `&raw mut (*thread).loop_`, `termio_impl.rs`) → `queue_write` →
  `stream.rs` `(*loop_).add(..)`; the stop callback `(*self_).loop_.stop()`.
  The exec.rs tests do the same with a stack `loop_` + `loop_ptr`.
- **through the callback's `l`** after `tick_inner` wrote a Loop field through
  `self` (e.g. `self.active -= 1` right before `cb(..)`).
- **WASI** (N430): a callback notifies an `Async` whose waiter holds the root;
  `async_notify(&self)` WRITES `Loop.wakeup`, which `tick_inner` then reads.
- **synchronous callbacks from `start`** (epoll/wasi `start`, iocp
  `start_completion`), reached from the tick's submission/rearm path.

## The model

`Loop { active, submissions, completions, asyncs, timers, wakeup: AtomicBool,
cached_now, flags }`, `Completion { op, next, state, result, callback, userdata }`
with the intrusive queue writing `next` through `addr_of_mut!` (queue.rs). The
loop body mirrors the real `tick_inner`s line by line in the parts that matter:
submissions → `start` (with the synchronous-callback arm), expired timers
(`active -= 1` then `cb(ud, self_ptr, ..)`, Rearm → `start`), WASI asyncs
(`wakeup` load/store, take the queue, per-completion `wakeup`), the completion
queue (kqueue `c_active` + Disarm/Rearm bookkeeping). termio's `Thread` holds the
loop inline, `ThreadData.loop_ = &raw mut (*thread).loop_`.
Timers here are an intrusive `next` queue, not a heap node embedded in the
completion's `op`, so this harness cannot see the timer-heap link aliasing: that
is N434 (`../mirihar-n434`). Its CLEAN rows do not cover the timer heap.

Candidates `<shape>_<entry>_<reentry>`:

- shape `shipped`: run/tick/tick_inner/start all `&mut self` (the shipped code).
- shape `raw`: the FIX — `Loop::run_raw(this: *mut Loop)` → `tick_raw` →
  `tick_inner_raw` → `start_raw`, narrow `(*this).field` access, short-lived
  `(*this).helper()` reborrows only for helpers that invoke no callback. Entered
  through `run_raw`.
- shape `wrapper`: the fix's thin `run(&mut self) { Loop::run_raw(self, mode) }`
  entered as a method (`(*self_).loop_.run(..)` / `loop_.run(..)`).
- shape `rawmutstart`: raw tick path, but `start` kept `&mut self`.
- entry `thread`: termio Thread (loop inline in a Box, stored root = field ptr).
- entry `local`: exec.rs tests (stack loop, `loop_ptr = addr_of_mut!(loop_)`,
  bounded `NoWait` pump).
- reentry `root` / `l` / `wasi` / `sync`: see above.

## Run

```sh
bash run-all.sh                                         # every candidate, SB + TB (~10 s)
cargo +nightly miri test                                # the applied shape, SB
MIRIFLAGS=-Zmiri-tree-borrows cargo +nightly miri test  # the applied shape, TB
```

## Results (2026-09-28, miri 0.1.0 nightly 2026-07-22)

| candidate | SB | TB | diagnostic |
|---|---|---|---|
| `shipped_thread_root` (termio) | **UB** | **UB** | SB "would remove [Unique ..] which is strongly protected"; TB "reborrow through .. is forbidden" |
| `shipped_local_root` (exec tests) | **UB** | **UB** | same |
| `shipped_thread_l` | **UB** | CLEAN | SB "retag .. tag does not exist in the borrow stack" (the `l` tag was popped by `self.active -= 1`) |
| `shipped_local_l` | **UB** | CLEAN | same |
| `shipped_thread_wasi` (N430) | **UB** | **UB** | strongly protected / reborrow forbidden |
| `shipped_local_wasi` (N430) | **UB** | **UB** | same |
| `shipped_thread_sync` | **UB** | **UB** | strongly protected / reborrow forbidden |
| `wrapper_thread_root` | **UB** | **UB** | the thin `run(&mut self)`'s own protector |
| `wrapper_local_root` | **UB** | **UB** | same |
| `wrapper_thread_l` | CLEAN | CLEAN | |
| `wrapper_local_l` | CLEAN | CLEAN | |
| `rawmutstart_thread_sync` | **UB** | **UB** | `start(&mut self)`'s protector across the synchronous callback |
| **`raw_thread_root`** (applied, termio) | **CLEAN** | **CLEAN** | |
| **`raw_local_root`** (applied, exec tests) | **CLEAN** | **CLEAN** | |
| **`raw_thread_l`** | **CLEAN** | **CLEAN** | |
| **`raw_local_l`** | **CLEAN** | **CLEAN** | |
| **`raw_thread_wasi`** (N430 fixed) | **CLEAN** | **CLEAN** | |
| **`raw_local_wasi`** | **CLEAN** | **CLEAN** | |
| **`raw_thread_sync`** | **CLEAN** | **CLEAN** | |
| **`raw_local_sync`** | **CLEAN** | **CLEAN** | |

`cargo +nightly miri test` (the ten applied-shape candidates): ok under SB and TB.

Conclusions that shape the fix:

1. The tick path must hold no `&mut Loop` across a callback: `run`/`tick`/
   `tick_inner` AND every callback-invoking helper (`start`, `stop_completion`)
   are raw-self (`rawmutstart` is UB).
2. A caller that re-enters through a stored root must ENTER through
   `Loop::run_raw(root, ..)`: the thin `run(&mut self)` still carries its own
   function-entry protector (`wrapper_*_root` is UB). termio's `Thread`, the
   renderer `Thread`, and the exec.rs tests switch to `run_raw`.
3. `run(&mut self)` remains sound for callers whose callbacks reach the loop only
   through `l` (`wrapper_*_l` is clean) — every rustty-xev unit test.
4. N430 needs no separate fix: with the raw tick path, the WASI notify's `&Loop`
   + `wakeup` store through the root is clean under both models.
