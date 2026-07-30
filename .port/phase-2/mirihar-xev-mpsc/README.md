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
         xthread_naive xthread_shipped xthread_boxq \
         wakeup_naive wakeup_copied wakeup_completion_cached \
         taskloop_lineage_naive taskloop_lineage_rooted; do
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

### Second pass — the worker-thread `&mut Loop`/`wakeup` + `task_loop`-lineage follow-up (WS19 T10)

The queue candidates above sequence the two threads with a channel handshake, so
the loop thread is **not** inside `tick(&mut self)` while the worker runs. The
candidates below remove that concession: the loop thread deliberately HOLDS its
function-entry (**protected**) `&mut Loop` across the worker's `thread_perform`,
which is what actually happens (`tick_inner(&mut self)` submits to the pool and
keeps ticking). Two spin flags make the interleaving deterministic.

| candidate | models | SB | TB |
|---|---|---|---|
| `wakeup_naive` | shipped `kqueue.rs`: `(*(*c).task_loop).wakeup()` — autorefs `&mut Loop` on the worker | **UB** | **UB** |
| `wakeup_copied` | shipped `epoll.rs` "fix": read the `Copy` wakeup handle out BY VALUE through `task_loop` | **UB** | CLEAN |
| **`wakeup_completion_cached`** | **APPLIED FIX**: the handle is copied into the (caller-owned) `Completion` at schedule time; the worker touches `Loop` not at all | **CLEAN** | **CLEAN** |
| `taskloop_lineage_naive` | shipped `(*c).task_loop = self` inside `&mut self`, deref'd after a later tick | **UB** | CLEAN |
| `taskloop_lineage_rooted` | `task_loop` from one stable raw root; handles cached in the completion | **CLEAN** | **CLEAN** |

**What this pass settled (and it contradicts the 2026-07-19 follow-up's own
prescription).** "Mirror epoll's copied-fd notify" is **NOT sufficient**:
`wakeup_copied` is still SB-UB. Forming no *reference* is not enough, because a
by-value read is still a foreign **access** on the `Loop` allocation, and the loop
thread's `&mut Loop` is *protected* (a function-entry argument) — protected tags may
not be invalidated at all. By the same argument the ALREADY-APPLIED M1 fix's
`(*(*c).task_loop).thread_pool_completions_ptr` read was also unsound; the
`xthread_boxq` "CLEAN" verdict held only because that candidate had sequenced the
concurrency away.

The cure is therefore structural, and it is the same one for both handles: the
worker must not touch the `Loop` allocation **at all**. Both are copied into the
`Completion` — an allocation the CALLER owns, never inside `Loop`, hence immune to
`Loop`'s per-tick reborrows — on the loop thread at schedule time. This also
discharges the `task_loop` lineage hazard for free: `task_loop` is no longer
dereferenced on the worker, so the fact that it is minted from one tick's `&mut
self` (`taskloop_lineage_naive`: SB UB) stops mattering.

**Fidelity note:** the reference's `Completion` HAS a `task_completions:
*TaskCompletionQueue` field, which the Task-2 port had elided in favour of reaching
the queue via `task_loop`. Restoring it is a fidelity GAIN; only `task_wakeup` is
port-added.

### Applied fix, second pass (`kqueue.rs` + `epoll.rs`)

* `Completion` gains `task_completions: *mut TaskCompletionQueue` (the reference's
  own field, restored) and `task_wakeup: Async` / `Eventfd` (port-added, `Copy`).
* `Loop::start`'s threadpool arm / `thread_schedule` writes both from the loop
  thread, before `schedule`.
* `thread_perform` reads BOTH out of `(*c)` and never derefs `(*c).task_loop`.
* `COMPLETION_SIZE` tripwires re-pinned: kqueue 248 → 264, epoll 208 → 224.

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
