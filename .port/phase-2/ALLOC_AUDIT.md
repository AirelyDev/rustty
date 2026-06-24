# Allocation-Stack Audit — rustty (Ghostty Zig→Rust port)

Phase 2 · audited READ-ONLY against Ghostty (pinned) + Zig 0.15.2 stdlib.
Scope: `rustty-zigstd` (ArenaAllocator, MemoryPool) and `rustty-vt/pagelist` (PagePool,
NodePool, PinPool, non-standard pages, reset/deinit, the AutoArrayHashMap PinSet, and the
Stacked-Borrows page-pointer idioms in grow/resize/erase).

---

## 1. Executive summary

The faithful core of the allocator stack is **solid**. `rustty-zigstd`'s `ArenaAllocator`
and `MemoryPool` are byte-for-byte ports of the Zig stdlib: BufNode layout (16B/align-8),
the 1.5× geometric node growth, LIFO free-list reuse, preheat, growable gating, and
`reset(retain_with_limit)` collapse-to-one-node semantics all match the reference exactly
and are Miri-clean. The `NodePool` and `PinPool` ride this faithful engine. All the
Stacked-Borrows hazards in the page-pointer code (grow/compact/resize-no-reflow/reflow)
are real latent-UB sites that the port has **already defused correctly** via `addr_of_mut!`
re-derivation after list splices — verified against the reference and Miri.

The divergences cluster on **one component and one decision**:

### The single most important call: PagePool is a hand-rolled per-page `std::alloc` pool, not an arena — and it aborts on OOM.

The page-buffer pool (`mod.rs:212-353`) is the **one place** that departs from the faithful
model, and it does so on three axes at once, two of which are real defects:

1. **OOM → process abort (CRITICAL, SOUNDNESS).** `alloc_std` (mod.rs:265-266) and
   `alloc_non_standard` (mod.rs:1415-1416) call `alloc_zeroed` + `handle_alloc_error`,
   which `SIGABRT`s the process. The reference threads `Allocator.Error!` and callers
   (`Screen.zig`) degrade gracefully and even *retry* under memory pressure. A scenario
   Ghostty survives crashes rustty. The `Result<_, OutOfMemory>` signature on
   `create_page`/`grow` is **misleading**: the buffer path can never return `Err`. **This is
   the top fix.**

2. **Eager ~360 KiB memset per fresh page (HIGH, MEMORY+PERF).** `alloc_zeroed` with
   align=4096 > MIN_ALIGN(16) forces Rust std down the `alloc + write_bytes(0)` path
   (no calloc fast-path), so every cache-miss page allocation eagerly memsets the full
   ~360 KiB buffer **and commits all ~90 physical 4 KiB pages** to RSS. The reference's
   mmap is lazily OS-zeroed and only `@memset`s under `runtime_safety` (debug). On the
   grow/scroll hot path and across many lightly-used scrollback pages this is a real
   latency + RSS regression. The fix already exists in-tree (`page_alloc::alloc`).

3. **O(N) backing allocations vs reference O(log N) arena blocks (GOAL-2, optimality).**
   One `alloc_zeroed`/`dealloc` per page vs the reference's `MemoryPoolAligned`-over-
   `ArenaAllocator` geometric batching. **Caveat (corrected during audit): this is NOT a
   peak-RSS regression — the port carries zero geometric slack and is equal-or-better on
   committed bytes; the cost is syscall traffic + loss of slab contiguity/locality.** The
   reset path likewise retains N discrete buffers vs the reference's single collapsed node
   (FIDELITY/medium topology divergence, but byte budget and zeroing match).

A faithful `MemoryPoolAligned`-over-`ArenaAllocator` already exists in `rustty-zigstd` and
could replace the hand-rolled `PagePool`, fixing (1), (2), and (3) at once — but it must be
**parity-gated** for the reset/zeroing path, which the existing PORT NOTE (mod.rs:187-211)
documents was deliberately hand-rolled because zigstd's alias doesn't expose
`pages.arena.child_allocator` / BufNode-zeroing.

Everything else is low-severity: faithful-by-necessity fidelity notes (no in-place arena
grow, `remap` absent), accepted global-design tradeoffs (abort-on-OOM is the project-wide
`std::alloc`-direct policy; SipHash vs Wyhash index), a couple of doc/`expect()` hygiene
items, and **one genuine latent bug**: `PAGE_SIZE_MIN` is hardcoded to 4096 but
aarch64-macos (the primary dev target) uses 16384, diverging per-page `total_size` for
non-standard pages.

---

## 2. Prioritized CONFIRMED findings

| # | Component | Kind | Sev | Reference vs Port | Recommendation |
|---|-----------|------|-----|-------------------|----------------|
| 1 | PagePool / non-std alloc (mod.rs:265-266, 1415-1416) | SOUNDNESS | **CRITICAL** | ref: `try`→`Allocator.Error!`, callers retry/degrade · port: `alloc_zeroed`+`handle_alloc_error` → abort | Make buffer alloc fallible; thread `OutOfMemory` through `create_page`/`grow`. Top priority. |
| 2 | PagePool fresh alloc (mod.rs:262-269, 1413-1417) | MEMORY+PERF | **HIGH** | ref: lazily-zeroed mmap, `@memset` debug-only · port: eager ~360 KiB memset + full RSS commit per fresh page | Use the existing `page_alloc::alloc` (page.rs:190-209) lazy path; keep reset/destroy zeroing. |
| 3 | PagePool backing (mod.rs:262-279, 340-352) | GOAL-2 | high→med | ref: O(log N) geometric arena blocks, contiguous slabs · port: O(N) `alloc_zeroed`/`dealloc`, scattered | Route through `rustty-zigstd MemoryPoolAligned`-over-arena. Parity-gate. Not a peak-RSS regression. |
| 4 | `PAGE_SIZE_MIN` (page.rs:163) | FIDELITY | **medium** | ref: `std.heap.page_size_min`=16384 on aarch64-macos · port: hardcoded 4096 → divergent non-std `total_size` (up to 12 KiB/page) | Derive from target; restore parity. Std-page case coincides and masks it. |
| 5 | PagePool reset (mod.rs:309-337) | FIDELITY | medium→low | ref: collapse to ONE arena node, byte budget `min(limit,cap)` · port: retain N discrete buffers `floor(limit/item_size)` | Document topology divergence or fold into #3. Byte budget & zeroing match; benign. |
| 6 | ReflowCursor (resize.rs:1153, 1929-2001) | SOUNDNESS | medium | ref: `self.* = .init(node)` after splice · port: same, full re-init before any stale deref | No change — sound. Optional `addr_of_mut!` hardening. |
| 7 | increase_capacity / compact / resize-no-reflow (grow.rs:548, 634; resize.rs:747) | SOUNDNESS | high | ref: `&node.data` held (Zig has no SB) · port: `addr_of_mut!` re-derive after splice → SB-clean | No change — fix present, Miri-verified (commit c46a6a2). |
| 8 | Arena `alloc()` in-place grow (arena_allocator.rs:232-239) | FIDELITY | low | ref: `rawResize` head node first, re-loop · port: always fresh geometric node | Faithful-by-necessity (no stable in-place-grow primitive); realloc would be UB. Document. |
| 9 | `node_layout().expect()` (arena_allocator.rs:111-112) | FIDELITY | low | ref: `rawAlloc ... orelse return null` · port: `.expect()` → panic at >isize::MAX | PARITY-SAFE: `.ok()?`, thread `?` through `raw_alloc_node` + reset. |
| 10 | Arena reset realloc-move (arena_allocator.rs:349-362) | FIDELITY | low | ref: rawResize-else-alloc+free · port: `realloc` (may move), rewrites BufNode | Sound — contract dead-lists prior ptrs. Optional debug generation counter. |
| 11 | PinSet index = SipHash std::HashMap (array_hash_map.rs:64-69) | FIDELITY | low | ref: robin-hood power-of-two IndexHeader, 60% LF, Wyhash · port: SwissTable ~7/8 LF, SipHash13 | Keep — entries Vec holds insertion order; index unused by public API. Doc note. |
| 12 | PinSet OOM / put_no_clobber (array_hash_map.rs:234) | FIDELITY | low | ref: `try putNoClobber`→`Allocator.Error!` · port: `()` → Vec/HashMap abort | Accepted global `std::alloc` policy; init failpoint preserved. Doc. |
| 13 | `remap` vtable entry absent (arena_allocator.rs) | FIDELITY | low | ref: `remap = if resize ... ptr else null` · port: no `remap`, callers use `resize` | Optional additive `remap()` for API surface parity. |
| 14 | ITEM_ALIGNMENT over-align (memory_pool.rs:76-82) | FIDELITY | low | ref: under-aligns when explicit A<align_of(T) · port: folds align_of::<T>() → safer | Latent (only ALIGN≠0 branch, currently dead). debug_assert/doc. |
| 15 | MemoryPoolAligned alias (memory_pool.rs:210) | GOAL-2 | low | test-only; real pages use hand-rolled PagePool | `cfg(test)`-gate or doc as test-only. |
| 16 | MemoryPool field decl order / PORT NOTE (mod.rs:380-382, 431-434) | PARITY-SAFE-FIX | low | comment claims pages-first drop; actual decl `nodes,pages,pins` | Pools self-contained → no UB. Fix comment; optionally reorder to `pages,nodes,pins`. |
| 17 | OOM tripwire test coverage (mod.rs:942, 958) | GOAL-2 | low | tripwire never drives real buffer allocator to failure (ref same) | Hardening nice-to-have; not a fidelity gap (ref never failed real allocator either). |

**Faithful / no-divergence confirmations (no action):** BufNode layout + growth constants
+ free/resize bump (arena_allocator.rs); MemoryPool LIFO create/destroy/preheat/growable +
arena-backed batch growth + reset (memory_pool.rs); non-standard page alloc/free lifecycle
(mod.rs:1413-1456); PagePool create() zero-faithfulness (fresh=alloc_zeroed,
reuse=zero-on-destroy, retain=zero-on-reset); node vs page reset asymmetry (ref is itself
asymmetric by design); reflow_row `src_page` read-only hold; prev-page grow-cols fill;
split/prune/less-cols/erase immediate-use page pointers.

---

## 3. Recommended changes

### 3a. PARITY-SAFE fixes (do now — restore reference behavior, no parity risk)

- **#1 Make page-buffer allocation fallible (CRITICAL).** Replace `handle_alloc_error` in
  `alloc_std` (mod.rs:266) and `alloc_non_standard` (mod.rs:1416) with `NonNull::new(ptr)`
  returning `Option`/`Result`; propagate `OutOfMemory` through `create()` →
  `create_page_ext` (which already has the signature) → `grow`. Restores Ghostty's
  survive-and-retry behavior and makes the existing `Result` signatures honest.
- **#4 Derive `PAGE_SIZE_MIN` from the target (medium FIDELITY bug).** Replace the
  hardcoded `4096` (page.rs:163) with a target-derived value (16384 on aarch64-macos) to
  match `std.heap.page_size_min`. Fixes divergent non-standard-page `total_size`/footprint
  on Apple Silicon. Correct the false PORT NOTE at page.rs:158-162. Gate behind a parity
  test on `total_size` for a reduced-capacity page.
- **#9 `node_layout` graceful OOM.** `Layout::from_size_align(...).ok()?` + thread `?`
  through `raw_alloc_node`/reset to mirror Zig's `orelse return null`.
- **#16 Fix misleading deinit-order PORT NOTE** (mod.rs:431-434); optionally reorder fields
  to `pages, nodes, pins` so the comment is literally true. No functional change.
- **#14 / #15 Hygiene:** debug_assert on the under-align branch; `cfg(test)`-gate or doc the
  test-only `MemoryPoolAligned` alias.
- **#13 (optional) additive `remap()`** on the arena for vtable-surface parity.

### 3b. GOAL-2 optimizations (parity-gated — improve, but gate behind a parity test)

- **#2 Stop eager page zeroing (HIGH MEMORY+PERF).** Route fresh page buffers through the
  in-tree lazy `page_alloc::alloc` (page.rs:190-209) instead of `alloc_zeroed`, so RSS
  commits lazily on first write-fault as in the reference. Keep the faithful zero-on-destroy
  / zero-on-reset paths. Gate behind a test asserting handed-out buffers are observably
  zero.
- **#3 Replace hand-rolled PagePool with `MemoryPoolAligned`-over-`ArenaAllocator`.**
  Restores O(log N) backing allocs + slab contiguity and resolves #1/#2/#5 in one move.
  Requires either extending zigstd's alias to expose the arena child-allocator / BufNode
  zeroing the PORT NOTE (mod.rs:187-211) called out, or accepting the per-buffer zero on
  the retain path. **Do not assume a peak-RSS win** — the arena adds 1.19–1.51× geometric
  slack; the win is syscall count + locality. Parity-gate reset/zeroing.
- **#5 PagePool reset topology.** If #3 lands, the single-node collapse comes for free.
  Otherwise document the N-buffer retention as an intentional internal-layout divergence.
- **#17 Test seam** to drive the real buffer allocator to failure once #1 makes it fallible.

### 3c. OPEN ARCHITECTURAL QUESTIONS (for the human)

1. **Memory model: `std::alloc`-direct vs a faithful threaded `Allocator`/arena.** The
   project-wide decision to back everything with `std::alloc` + abort-on-OOM is the root of
   findings #1, #12, and the `remap`/fallibility gaps. Revisiting it (a threaded
   `Allocator` trait carrying `OutOfMemory`) would buy *exact* OOM fidelity but is a large
   surface change. **Decision needed: is abort-on-OOM acceptable as the global contract
   (then #1 is just "make the signature honest by aborting deliberately"), or must
   terminal-buffer OOM be recoverable (then #1 needs real error threading)?** Recommendation:
   recoverable for the *page-buffer* path specifically (it's the largest, most pressure-
   sensitive allocation and the reference explicitly retries), abort acceptable elsewhere.
2. **Single shared arena for page buffers vs per-pool isolation.** Adopting
   `MemoryPoolAligned`-over-arena (#3) reintroduces the reference's geometric slack
   (1.2–1.5× over-commit) in exchange for syscall/locality wins. Is the locality/syscall
   win worth the over-commit on memory-constrained targets? This is fundamentally a
   measurement question (§4).
3. **PinSet index data structure.** Keep the `std::HashMap` simplification (observationally
   equivalent, PARITY-SAFE) or port Zig's robin-hood `IndexHeader` for index-memory/probe
   parity? Only matters if a future micro-benchmark compares index internals.

---

## 4. Measurements needed (follow-up phase — Zig vs Rust)

Settle the perf/memory claims with data rather than reasoning:

1. **Page-alloc latency on the hot path (validates #2).** Microbench fresh-page allocation
   (cache-miss, free-list empty): measure the ~360 KiB memset cost per `alloc_std` vs
   reference lazy mmap. Expect a clear win from removing eager zeroing.
2. **Peak + steady-state RSS over a realistic scrollback workload (validates #2, #3, #5).**
   Drive identical input through Ghostty and rustty (grow scrollback to N pages, scroll,
   reset). Compare RSS_peak, RSS_steady, and committed-vs-reserved. Tests the
   port-carries-no-slack (better) vs arena-over-commit (reference) prediction and the
   N-buffer-vs-one-node reset topology.
3. **Backing allocation count / syscall trace (validates #3).** `strace`/`dtruss` mmap+brk
   counts for allocating 64 net-new pages: expect port O(N) vs reference O(log N). Confirms
   the syscall-traffic delta and its real-world magnitude after free-list amortization.
4. **Arena node-count under real Ghostty arena workloads (validates #8).** Instrument
   `node_count()` to quantify how often the missing in-place grow actually costs an extra
   node (expected small — GPA/page_allocator in-place grow frequently fails anyway).
5. **OOM behavior under induced memory pressure (validates #1).** Inject allocation failure
   (cgroup/ulimit) and confirm rustty aborts where Ghostty survives/retries — quantifies the
   availability gap and is the acceptance test for the #1 fix.
6. **Steady-state fragmentation / locality (validates #3, #5).** Page-fault and cache-miss
   counts after many grow/reset cycles: does the scattered per-page allocation
   re-fragment vs the reference's contiguous slabs?
7. **Binary size + startup (preheat path).** Compare `init_preheated` P-allocation cost and
   resulting binary size for the two pool implementations.
