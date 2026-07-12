# xev MPSC completion-queue aliasing Miri harness

Isolated, **non-FFI** reproduction of the latent Stacked-/Tree-Borrows UB in the
`rustty-xev` kqueue thread-pool completion queue (LIFETIMES-audit findings **M1 +
M2**; `.port/audits/LIFETIMES_AUDIT_2026-07-10.md`,
`.port/phase-2/DEFERRALS.md` "LIFETIMES-AUDIT findings"). Used to PROVE the fix
before applying it to `crates/rustty-xev/src/kqueue.rs`, per doctrine
(`shaper-hook-sb-clean-interleave`; precedent `.port/phase-2/mirihar-chunk3b`).

## The bug

`Loop.thread_pool_completions` is a Vyukov intrusive MPSC queue
(`queue_mpsc.rs`) that is **self-referential**: its `head`/`tail` point at the
**embedded `stub`** — an address *inside* the queue. `init` writes those stub
self-pointers ONCE (first tick); `pop`/`push` re-deref them on EVERY later tick.
But the queue lived **inline in `Loop`**, and each `tick(&mut self)` /
`run(&mut self)` reborrows the whole `Loop`. Under SB/TB that reborrow invalidates
the provenance the stub self-pointers were minted with, so the later deref of the
stored `tail`/`stub.next` is use-after-invalidation UB. The branch is **dead
today** (`thread_pool` is always `None`), so no test reaches it and a normal Miri
run won't flag it — hence this synthetic init→tick→pop reproduction (+ the
cross-thread `thread_perform` push).

Each candidate is one executed path (Miri only checks executed code), selected by
argv[1].

## Run

```sh
for c in plain_naive plain_selfptr cell_selfptr inline_cached plain_rawtick boxq \
         xthread_naive xthread_shipped xthread_boxq; do
  echo "== $c =="
  cargo +nightly miri run --quiet -- "$c"                            # Stacked Borrows
  MIRIFLAGS=-Zmiri-tree-borrows cargo +nightly miri run -q -- "$c"   # Tree Borrows
done
```

## Results (the load-bearing evidence)

| candidate | models | SB | TB |
|---|---|---|---|
| `plain_naive` | current `kqueue.rs`: `&mut self.thread_pool_completions` coercions, `tick(&mut self)` | **UB** | **UB** |
| `plain_selfptr` | the DEFERRALS one-line `addr_of_mut!((*self_ptr)…)` prescription (tick still `&mut self`) | **UB** | **UB** |
| `cell_selfptr` | `UnsafeCell` head/tail/stub fields (the audit's "UnsafeCell-class fields" hypothesis) | **UB** | **UB** |
| `inline_cached` | inline queue + a cached raw `qptr = addr_of_mut!((*sp).q)` (control: caching alone) | **UB** | **UB** |
| `plain_rawtick` | Zig `tick(self: *Loop)` raw receiver, ONE stable raw root across all ticks | **CLEAN** | **CLEAN** |
| **`boxq`** | **APPLIED FIX**: heap-rooted queue (`Box`) + cached raw handle established at init; tick stays `&mut self` | **CLEAN** | **CLEAN** |
| `xthread_naive` | `thread_perform` forms `&mut (*task_loop).q` on the pool thread | **UB** | **UB** |
| `xthread_shipped` | pool-thread push via raw `addr_of_mut!` through the shared raw | **CLEAN** | **CLEAN** |
| **`xthread_boxq`** | **APPLIED FIX cross-thread**: pool thread reads the loop's cached raw handle + pushes through it; loop thread pops via the same cache | **CLEAN** | **CLEAN** |

## What the harness SETTLED (the audit's open questions)

The DEFERRALS entry left the fix shape open ("the harness must decide whether the
tick receiver needs a raw seam or head/tail need `UnsafeCell`-class fields"). The
runs settle it decisively:

1. **The one-line `addr_of_mut!` prescription is NOT enough** (`plain_selfptr` UB).
   The stub self-pointers minted on the init tick are children of that tick's
   `&mut self`; the NEXT tick's `&mut self` *function-entry* Unique retag pops the
   whole lineage before pop re-derefs them.
2. **`UnsafeCell` fields are NOT enough** (`cell_selfptr` UB). The invalidated tag
   is the whole-`Loop` raw coercion (`let sp = self`), whose root is popped by the
   next tick's whole-`Loop` Unique retag — the `UnsafeCell`-ness of the *leaf*
   fields is irrelevant because the *root* of the pointer chain is popped.
3. **Caching a raw pointer is NOT enough on its own** (`inline_cached` UB) — a
   cached raw that still points *into the `Loop` allocation* is popped by the same
   whole-`Loop` reborrow. The **separate allocation** is the load-bearing
   ingredient, not the cache.
4. **A stable raw root across ticks IS enough** — either the Zig `tick(self:
   *Loop)` raw receiver (`plain_rawtick`), or, keeping the public `tick(&mut self)`
   API, **giving the queue its own heap allocation** so `Loop`'s reborrows never
   touch the queue's allocation (`boxq`). The queue's `*Self` method contract is
   preserved verbatim in both.

`boxq` is the applied fix: it fixes the exact defect (a self-referential queue in a
reborrowed allocation) with **zero** change to the public `tick`/`run` API or any
driver/test — the raw-receiver route would have required converting the public
event-loop API and every caller (the live driver reborrows `loop_.run(NoWait)` up
to 2000×), a redesign of live, tested code for a dead branch. `boxq` mirrors why
the OTHER (non-self-referential) intrusive queues are already Miri-clean: their
links point at EXTERNAL allocations, immune to `Loop` reborrows.

## Applied fix (`crates/rustty-xev/src/kqueue.rs`)

* `thread_pool_completions: TaskCompletionQueue` → `Box<TaskCompletionQueue>`.
* new field `thread_pool_completions_ptr: *mut TaskCompletionQueue` — the stable
  heap-rooted handle, cached at first-tick `init` (`null` until then).
* the three access sites (`init` @ tick_inner, `pop` @ tick_inner drain,
  `push` @ `thread_perform`) route through the cached raw handle, never a fresh
  `&mut …thread_pool_completions`.
* `queue_mpsc.rs` is UNCHANGED — its methods already take `*mut Self` (faithful to
  Zig `*Self`); only the queue's storage is heap-rooted.
