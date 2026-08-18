# W6 termio write-pool aliasing Miri harness

Isolated, **non-FFI** reproduction of the exact borrow structure of the W6
(upstream `e0ef934f7`) `SegmentedPool` → `MemoryPool` termio write-path rewrite
(`crates/rustty-termio/src/exec.rs` `queueWrite`/`ttyWrite` over
`crates/rustty-xev/src/stream.rs` `queue_write`/trampoline and
`crates/rustty-zigstd/src/memory_pool.rs`), used to PROVE Stacked-Borrows +
Tree-Borrows soundness of the pooled-`Write`-node discipline that the real
(pty/event-loop-gated) tests cannot check under Miri.

What it proves:

* The `&mut (*exec).write_pool` autorefs in `create`/`destroy` cover only the
  pool HEADER inside `ThreadData` — never the arena-heap node bytes — so the
  addresses of in-flight `Write` nodes stay valid across completion.
* Writing the logically-uninitialized checked-out slot via `addr_of_mut!` raw
  places (`w.td`, `w.buf`) and re-initializing the undefined `req` by
  whole-value assignment (no drop glue → pure write) is clean.
* Destroy-inside-the-completion-callback (via the `w.td` BACKREF) while OTHER
  nodes are still queued, LIFO slot reuse (free-list `Node` punning), and the
  CRLF-expansion buffer path are all clean.
* The FOIL (`ub_threaddata_mut`) shows the banned shape
  (`rust-mut-args-strongly-protected`): one `&mut ThreadData` re-borrow between
  queue and completion invalidates the engine's raw `&write_queue` — Miri
  flags the trampoline's very first queue access.

Each candidate is one executed path (Miri only checks executed code), selected
by argv.

Run (from this dir):

```sh
cargo +nightly miri run -q -- ported                              # Stacked Borrows: PASS
MIRIFLAGS=-Zmiri-tree-borrows cargo +nightly miri run -q -- ported  # Tree Borrows: PASS
cargo +nightly miri run -q -- ub_threaddata_mut                   # documented UB: Miri flags it
```

Proven 2026-08-18 (miri 0.1.0 6f72b5dd5f 2026-07-22): `ported` clean under
both SB and TB; `ub_threaddata_mut` flagged at the trampoline's
`(*q_inner).head` read, invalidated by the `&mut td` re-borrow — exactly the
doctrine's prediction.
