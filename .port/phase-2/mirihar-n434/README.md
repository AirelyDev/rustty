# N434 — timer-heap links invalidated by a `&mut (*c).op` retag

Isolated, **non-FFI** reproduction of the borrow shape behind NEW-FINDINGS **N434**
(the epoll, kqueue, iocp and wasi_poll timer heaps), used to PROVE the
`timer_ptr(c)` fix (doctrine: memory `shaper-hook-sb-clean-interleave`; precedent
`../mirihar-n431`). kqueue, iocp and wasi_poll cannot run under Miri (foreign
`kqueue`/`kevent64`, IOCP and `poll_oneoff` calls); epoll can, from the Mac, with
`--target aarch64-unknown-linux-gnu`, and is the real-code twin (below).

## The bug

Each backend's `start` inserted into the timer heap a `*mut Timer` derived from a
`&mut (*c).op` reborrow (kqueue/iocp/wasi_poll: the binding of a whole-op
`match &mut (*c).op`; epoll: an inner `match &mut (*completion).op`). While the timer
is linked, `timer_reset`'s Active arm and `stop_completion` each form a fresh
`&mut (*c).op`. That Unique retag, granted by the completion's own tag, pops the
heap's pointer under Stacked Borrows; `heap::remove` -> `delete_min` then reads
the node through the popped root link. Under Tree Borrows, once the heap has
written the node's links through that pointer (a second timer melded under it)
the pointer is Unique, the new retag's read freezes it, and `delete_min`'s write
of `root.heap = .{}` is forbidden.

## The fix

`unsafe fn timer_ptr(c: *mut Completion) -> *mut Timer`: a shared match on
`&(*c).op` finds the payload address, returned with `c`'s own provenance
(`c.cast::<u8>().with_addr(addr).cast()`). Every heap-path Timer access (the
`start` insert and `.c` write, `timer_reset`'s Adding and Active arms,
`stop_completion`'s remove and `reset`/`next` reads and writes) goes through it,
and `stop_completion` matches `&(*c).op`. Same writes in the same order (R1).

## The model

The heap is the REAL `crates/rustty-xev/src/heap.rs`, included by `#[path]`.
`Completion { op: Operation, userdata, callback, state, result }` with a
multi-variant `Operation` (Noop / Read / Cancel / Timer), so the Timer payload
sits at a non-zero offset as in the real enums. The loop is raw-self (the N431
shape) with `submissions`, kqueue/iocp `cancellations` and `completions`, and the
tick's expired-timer loop (`peek`, `Timer::less`, `delete_min`, `(*t).c`,
callback). The scenario is the renderer Thread's: loop and completions inline in
a Box, every pointer `&raw mut (*th).field` (`renderer_seam.rs` `render_h.reset`,
`cursor_h.reset`, `cursor_h.cancel`). Three timers (10, 20, 30) are started; one
is reset to 25 or cancelled; then everything fires. Each candidate asserts the
exact callback order, `active == 0`, an empty heap and every completion dead.

Candidates `<backend>_<shape>_<path>_<target>`:

- backend `kqueue` / `iocp` / `wasi` / `epoll`: that backend's `start`,
  `timer_reset` Active/Adding and `stop_completion` borrow shapes, statement for
  statement in the timer arms.
- shape `shipped` (pre-N434) / `fixed` (`timer_ptr`).
- path `reset` (`timer_reset` on an Active timer -> cancel -> `stop_completion` ->
  remove + rearm) / `cancel` (`stop_completion` -> remove + Cancel callback).
- target `root` (the acted-on timer is the heap root) / `inner` (a child).

## Run

```sh
bash run-all.sh                                                 # 32 candidates, SB + TB (~12 s)
cargo +nightly miri test applied                                # the fixed shape, SB
MIRIFLAGS=-Zmiri-tree-borrows cargo +nightly miri test applied  # the fixed shape, TB
```

## Results (2026-09-28, miri 0.1.0 nightly 2026-07-22)

| candidate | SB | TB |
|---|---|---|
| `{kqueue,iocp,wasi,epoll}_shipped_reset_root` | **UB** | **UB** |
| `{kqueue,iocp,wasi,epoll}_shipped_cancel_root` | **UB** | **UB** |
| `{kqueue,iocp,wasi,epoll}_shipped_reset_inner` | CLEAN | CLEAN |
| `{kqueue,iocp,wasi,epoll}_shipped_cancel_inner` | CLEAN | CLEAN |
| **`{kqueue,iocp,wasi,epoll}_fixed_{reset,cancel}_{root,inner}`** (16) | **CLEAN** | **CLEAN** |

Diagnostics: SB "attempting a read access using <tag> .. but that tag does not
exist in the borrow stack" at `heap.rs:39` (`get_child` from `delete_min`); TB
"write access through <tag> .. is forbidden .. has state Frozen" at `heap.rs:146`
(`delete_min`'s `root.heap = .{}`), the tag created by the `Operation::Timer(t)`
binding in `start`.

`cargo +nightly miri test applied` (the 16 fixed candidates): ok under SB and TB.

Notes:

1. The `inner` rows are clean in both shapes: removing a non-root node compares
   the parent's link to it by address only and never dereferences the stale
   pointer. The bug needs a dereference of a link to the retagged node, which the
   root link always gets (`remove` -> `delete_min`, and every tick's `peek` +
   `less`).
2. The real epoll tests (`epoll_timer_reset`, `epoll_timer_cancellation`) hold a
   single timer, so the heap never writes that node's links before the retag:
   they are UB under SB only. With two or more timers linked at once (the renderer
   thread can have `render_c` and `cursor_c` armed together), TB also rejects the
   shipped shape.

## Real-code twin (epoll)

`cargo +nightly miri test -p rustty-xev --target aarch64-unknown-linux-gnu --lib --
--exact epoll::tests::<name>` for the 14 Miri-runnable epoll loop/timer/async/cancel
tests: before the fix `epoll_timer_reset` and `epoll_timer_cancellation` are UB
under SB (CLEAN under TB), the other 12 CLEAN/CLEAN; after the fix all 14 are
CLEAN under SB and TB.
