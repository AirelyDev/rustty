# N385 — IOCP `Async::notify` / `Loop::async_notify` aliasing Miri harness

Isolated, **non-FFI** reproduction of the borrow structure behind NEW-FINDINGS
**N385**, used to PROVE the fix applied to `crates/rustty-xev/src/iocp.rs` and
`iocp_watcher.rs` (doctrine: memory `shaper-hook-sb-clean-interleave`; precedent
`../mirihar-xev-mpsc` `wakeup_*`, WS19 T10). The real code cannot run under Miri:
IOCP is Win32 FFI.

## The bug

`AsyncIOCP.notify` runs on another thread and, like the reference
(`watcher/async.zig:597`), called `w.loop.async_notify(w.c)`, i.e.
`(*w.loop_).async_notify(w.c)` with `async_notify(&self, ..)`. Meanwhile the loop
thread is inside `Loop::run(&mut self)` -> `tick(&mut self)` -> `tick_inner(&mut
self)`: a function-entry, PROTECTED `&mut Loop` for the whole run. Forming a
`&Loop` on the notifier (or even reading `iocp_handle` by value through the raw
`*Loop`, the N385 fix sketch's option A) is a foreign access to that protected
unique borrow.

A second, related window on the same path: from `wait` on, a notifier may run
while the loop thread's `start_completion` holds `match &mut (*completion).op`
for the just-submitted async completion; the notifier reads `op` (discriminant +
the `wakeup` atomic), which pops that `&mut` under Stacked Borrows.

## The model

`Loop { iocp_handle, active, submissions, asyncs, other }`, `Completion { op,
next, state, .. }`, `Operation::{Noop, Timer, AsyncWait { wakeup: AtomicBool }}`,
the watcher `Async { guard: Mutex<{ wakeup, waiter }> }`. The kernel port is an
atomic counter in its own allocation (`PostQueuedCompletionStatus` and
`GetQueuedCompletionStatusEx` are FFI on a kernel object; only the HANDLE value is
in `Loop`). The loop is reached through ONE stable raw root, as termio does
(`(*self_).loop_.run(..)`). Handshake flags force the two notifications into the
two windows: (1) inside `start_completion` with its `op` borrow live, (2) while
the loop "blocks in GetQueuedCompletionStatusEx" holding `&mut self`. After both,
the loop thread keeps using its `&mut self`.

Candidates, `<notify>_<start>`:

- notify `loopref`: shipped, `(*w.loop_).async_notify(w.c)` (`&self`).
- notify `rawread`: fix-sketch option A, `addr_of!((*w.loop_).iocp_handle).read()`.
- notify `cached`: APPLIED, the HANDLE copied into the waiter at `wait` time (loop
  thread); `notify` calls `Loop::async_notify_handle(w.iocp_handle, w.c)`.
- start `mutop`: shipped, `match &mut (*completion).op`.
- start `sharedop`: APPLIED, the `async_wait` arm decided through `&(*completion).op`.

## Run

```sh
bash run-all.sh                                                  # every candidate, SB + TB
cargo +nightly miri test                                         # the applied shape, SB
MIRIFLAGS=-Zmiri-tree-borrows cargo +nightly miri test           # the applied shape, TB
MIRIFLAGS="-Zmiri-many-seeds=0..8" cargo +nightly miri run -q -- cached_sharedop
MIRIFLAGS="-Zmiri-tree-borrows -Zmiri-many-seeds=0..8" cargo +nightly miri run -q -- cached_sharedop
```

## Results (2026-09-28, miri 0.1.0 nightly 2026-07-22)

| candidate | SB | TB | SB diagnostic |
|---|---|---|---|
| `loopref_mutop` (shipped) | **UB** | **UB** | "would remove [Unique ..] which is strongly protected" at the notifier's `&Loop` |
| `loopref_sharedop` | **UB** | **UB** | same |
| `rawread_mutop` | **UB** | CLEAN | "strongly protected" at the notifier's by-value handle read |
| `rawread_sharedop` | **UB** | CLEAN | same |
| `cached_mutop` | **UB** | CLEAN | loop thread's `match op` after the notifier read `op`: tag "does not exist in the borrow stack" |
| **`cached_sharedop`** (applied) | **CLEAN** | **CLEAN** | — |

TB's `loopref` diagnostic: "reborrow through <tag> is forbidden ... foreign to the
protected tag". `cargo +nightly miri test` (1 test, the applied shape): ok under
SB and TB; `-Zmiri-many-seeds=0..8`: clean under both models.

So both parts of the fix are needed under Stacked Borrows: the notifier must not
access the `Loop` allocation at all (a raw by-value read is still UB, as
`mirihar-xev-mpsc` `wakeup_copied` found for kqueue), and the loop thread must not
hold a `&mut` to the async completion's `op` while a notifier may read it.
