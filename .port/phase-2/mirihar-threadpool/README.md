# ThreadPool lock-free-primitives aliasing Miri harness

Isolated, **non-FFI** reproduction of the exact raw-pointer borrow structure of
libxev `ThreadPool.zig`'s two lock-free intrusive containers — the Vyukov-style
swap-based unbounded MPSC `Node.Queue` and the bounded single-producer /
multi-consumer work-stealing `Node.Buffer` — used to PROVE Stacked-Borrows +
Tree-Borrows soundness that the real port
(`crates/rustty-xev/src/thread_pool.rs`) cannot check under Miri (it spawns/
detaches OS threads and, in the epoll File task, routes blocking
pread/pwrite/close syscalls through the pool — none of which Miri executes).

This is the Task-1 SB-class gate (`.superpowers/sdd/task-1-brief.md` Step 2;
mirihar doctrine `.port/PORT_RULES.md:209`; memory
`shaper-hook-sb-clean-interleave`). Only once this is green under BOTH borrow
models were the same primitives ported verbatim into `thread_pool.rs`.

Run (from this dir):

```sh
cargo +nightly miri test                                         # Stacked Borrows (default)
MIRIFLAGS=-Zmiri-tree-borrows cargo +nightly miri test           # Tree Borrows
# harden across scheduler interleavings:
MIRIFLAGS="-Zmiri-many-seeds=0..8" cargo +nightly miri test
MIRIFLAGS="-Zmiri-tree-borrows -Zmiri-many-seeds=0..8" cargo +nightly miri test
# or drive the same hammer via main():
cargo +nightly miri run
```

## Results (the load-bearing evidence)

| container / op | models | SB | TB |
|---|---|---|---|
| `Queue::{push, try_acquire_consumer, release_consumer, pop}` | the tagged-pointer swap MPSC queue (`stack: AtomicUsize` with `HAS_CACHE`/`IS_CONSUMING` low bits, single-consumer `cache`) | **CLEAN** | **CLEAN** |
| `Buffer::{push (incl. overflow/migrate), pop, consume, steal}` | the bounded SP/MC work-stealing ring (`head`/`tail` `AtomicU32`, `[AtomicPtr<Node>; CAP]`) | **CLEAN** | **CLEAN** |
| whole `hammer_push_pop_steal` (K=3 workers × PER≥9 nodes, seeded via `push`-overflow → queue, drained via the faithful `Thread.pop` interleave: local buffer pop → local queue consume → global queue consume → peer queue consume → peer buffer steal) | the full concurrent aliasing structure | **CLEAN** | **CLEAN** |

Verified: 2/2 tests pass under Stacked Borrows and under `-Zmiri-tree-borrows`,
including `-Zmiri-many-seeds=0..8` (8 scheduler interleavings each) — no UB, no
data race. The only Miri diagnostic is the expected `integer-to-pointer cast`
*warning* on `node_from_addr` / `with_exposed_provenance_mut`: the Vyukov queue
genuinely stores tagged node addresses in a `usize` atomic, so int↔ptr is
intrinsic to the algorithm — not a defect. (`-Zmiri-permissive-provenance`
silences it; the harness deliberately uses the exposed-provenance API so a
`-Zmiri-strict-provenance` audit could still reason about it.)

## The SB-clean interleave (the rules the run proved)

1. **`Node.next` is touched ONLY through a raw `*mut Node`** (`(*node).next`),
   never a `&Node`/`&mut Node`. Producers write it (`push` / the `migrate`
   overflow chain) and one consumer reads it (`pop`); the atomic
   `stack`/`head`/`tail` carry the Acquire/Release happens-before, so there is no
   data race and no reference tag for a concurrent access to pop. A single
   `&mut Node` anywhere in this linkage would make every concurrent stealer's
   read UB.
2. **Every container method takes `&self`** (shared, interior-mutable) — the
   faithful Rust translation of Zig's `*Queue` / `*Buffer`. All mutable state is
   atomics, except the queue's `cache: ?*Node`, which lives in an
   `UnsafeCell<Option<*mut Node>>`. `cache` is written only by the single
   consumer holding `IS_CONSUMING`, so the cell is race-free; `&self` is what
   lets the container be shared across stealing threads without a `&mut`
   uniqueness claim that concurrent stealers would violate.
3. **The `Node`s live in their OWN heap allocations** (`Box::into_raw`), so a
   shared `&Shared` (or `&Buffer`/`&Queue`) never freezes their bytes — the raw
   `(*node).next` writes are writes to a *sibling* allocation, not interior
   mutation of a frozen `&`. (Storing the nodes inline in a `&`-shared `Vec`
   would freeze them and make the linkage writes UB — the classic trap this
   harness rules out.)
4. **`.raw` / `.unordered` map to `Relaxed`**: Zig's non-atomic `.raw` field
   reads ("we're the only thread that can change this") and `.unordered` array
   stores/loads become `Relaxed` atomic ops — always at least as strong, never
   UB, and race-free because the array cells are `AtomicPtr`.

`CAPACITY` is modelled at **8** (the real port uses 256): a smaller power of two
drives the identical push-overflow/migrate and steal-half code with far fewer
nodes so Miri can finish. This is a borrow-STRUCTURE proof, not a
throughput/capacity proof — the aliasing shape is capacity-independent.
