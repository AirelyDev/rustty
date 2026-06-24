# PageList Allocator Benchmark — OLD rustty vs NEW rustty vs Zig reference

Phase-2 allocator re-architecture benchmark. Compares the hand-rolled
`PagePool` (OLD) against the faithful `DynamicMemoryPoolAligned<PageAllocator>`
arena-over-lazy-zero-mmap port (NEW), with the Ghostty Zig `PageList` as the
reference target.

- OLD rustty: commit `f7acaec` (parent of the PagePool refactor `fdfc97f`).
  `struct PagePool: Vec<NonNull<u8>>` arena + free_list; standard pages drawn
  via `std::alloc::alloc_zeroed` + `handle_alloc_error` (eager-zero, infallible).
- NEW rustty: HEAD (`1cfa7a3`, parent `0f8ce3c`).
  `DynamicMemoryPoolAligned<PageAllocator>` over a faithful lazy-zero `mmap`
  `pageAllocator`, fallible OOM.
- Zig reference: Ghostty `src/terminal/PageList.zig`, ReleaseFast, Zig 0.15.2,
  built standalone outside the read-only reference tree
  (`.port/phase-1/bench-zig-pagelist/`, commit `7f7ba77`).

Platform: Apple Silicon arm64, macOS Darwin 25.5.0.

---

## 1. Methodology

### Workload (identical across all three variants)

`PageList::init(80, 24, max_size)` → grow to ≥2000 rows → `scroll(Top + DeltaRow,
Active)` across the viewport → full-screen `clone` (active + scrollback) → `resize`
cols with reflow (80→{120/200}→40→80) → `reset()` (pool retained) → `deinit` (both
the clone and the original). This exercises the page-allocation path through every
stage: preheat, bulk grow, scroll, clone (separate pool), reflow churn, and reset.

Minor per-variant differences forced by API surface, all driving the *same*
page-alloc path:
- NEW used `grow_rows(2000)` (a `#[test]` inside rustty-vt, since `grow_rows` is
  `pub(crate)`); `max_size = 64 MiB`.
- OLD used the public `grow()` ×2000 from an example binary; `max_size = 256 MiB`.
- Zig used `grow()` to ≥2000 rows; `max_size = 1 GiB`.

### Builds
RELEASE for all RSS + latency runs (`cargo build --release` / Zig `ReleaseFast`).
Alloc-count for NEW was captured from a `cargo test -p rustty-vt --release` run
because the tracking seam is `#[cfg(test)]`-only.

### Per-metric method

| Metric | Method |
|---|---|
| **Latency** | Median of 9 full-workload runs (`std::time::Instant` / Zig nanoTimestamp), 1 warmup excluded. **MEASURED.** |
| **Peak RSS** | `getrusage(RUSAGE_SELF).ru_maxrss`, read after all runs. On macOS `ru_maxrss` is in **BYTES** (no `/1024`). Process high-water mark; each variant in its own freshly-spawned process. Zig cross-checked with `/usr/bin/time -l`. **MEASURED.** |
| **Page allocs / bytes — NEW** | `rustty-zigstd::tracking_allocator::TrackingAllocator` on the PagePool, via the `page_tracking` thread-local seam (`pagelist/mod.rs` ~221-345). `total_allocs()` delta + peak `outstanding_bytes()` over one workload. Counts **arena backing-block** allocations (geometric doublings) + non-standard pages. Build-mode independent. **MEASURED.** |
| **Page allocs / bytes — OLD** | A `#[global_allocator]` wrapper counting allocations with `align ≥ 4096` (the `PAGE_SIZE_MIN`-aligned standard page buffers) during the workload, warmup gated off. Counts per-page `alloc_zeroed` calls directly. **MEASURED.** |
| **Page allocs / bytes — Zig** | Underlying `mmap` count via `--arena-sim` (a real `std.heap.ArenaAllocator` over the real `std.heap.page_allocator` with a counting wrapper — same mechanics as PagePool, isolated). Per-stage logical page-pool allocs via the public `pl.page_size` field (`--accounting`). **MEASURED.** |
| **Binary size** | Production release `librustty_vt.rlib` for rustty (tracking/bench are dev-only and absent); Zig the standalone bench exe. **MEASURED.** |

### MEASURED vs DERIVED
- **All five reported numbers for OLD and NEW rustty are MEASURED** (not derived).
- **Zig: latency, peak RSS, page geometry, per-stage logical allocs, and the
  underlying mmap count are MEASURED.** The mmap count came from a faithful
  `--arena-sim` replay because direct `dtruss`/`fs_usage` syscall tracing was
  infeasible (requires root under SIP, non-interactive sudo unavailable).
- **DERIVED-from-code (Zig):** Darwin `pageAllocator = mach.taggedPageAllocator`
  (mmap `MAP_ANON`, lazy zero-fill-on-demand); std page = 589824 B = exactly
  36×16 KiB hardware pages; MemoryPool preheats 4 std pages; `reset()` uses
  `retain_with_limit` (no `munmap`).

### Caveats (be explicit)
- The OLD tree at `f7acaec` is **not** a naive per-page-malloc design — it is
  already arena/free-list-backed; it just draws each standard page buffer via a
  separate `alloc_zeroed`. So "OLD vs NEW" measures **eager-zero per-page
  `alloc_zeroed`** vs **lazy-zero arena-doubling mmap**, not "no arena" vs "arena".
- **Peak RSS is noisy** (process high-water mark; OLD observed 10–18 MB across
  re-runs). Treat RSS deltas of a few MB as within noise; latency and
  page-alloc counts are the stable comparison metrics.
- Each variant's `max_size` differs (64 MiB / 256 MiB / 1 GiB) but none was hit
  (no prune), so it does not affect the alloc path for this workload.

---

## 2. Results

Per-iteration full-workload figures, MEASURED unless noted. Δ columns are
NEW relative to OLD, and NEW relative to the Zig reference.

| Metric | OLD rustty | NEW rustty | Zig ref | Δ NEW vs OLD | Δ NEW vs Zig |
|---|---:|---:|---:|---|---|
| Latency (median, ms) | 0.4241 | 0.6371 | 0.737 | **+50.2% (slower)** | −13.6% (faster) |
| Peak RSS (bytes) | 10,059,776 | 11,370,496 | 7,733,248 | +13.0% (≈noise) | +47.0% (higher) |
| Page allocs | 14 | **7** | 3 | **−50% (fewer)** | +4 (more) |
| Page bytes (peak live) | 8,200,192 | 8,374,728 | 7,502,136 | +2.1% | +11.6% |
| Binary size — rlib (bytes) | 2,034,968 | 2,032,880 | 341,592¹ | −0.1% (flat) | — |

¹ Zig binary size is a standalone *exe*, not directly comparable to the rustty
`.rlib`; shown for completeness only. The OLD example-bin figure (529,840 B) is
likewise not comparable to a library and is omitted from the headline row.

Notes on the page-alloc semantics (they are not apples-to-apples *units*, which
is itself the finding):
- OLD **14** = count of per-page `alloc_zeroed` calls (one syscall-backed
  zeroed buffer per ~150-row standard page).
- NEW **7** = count of arena **backing-block** allocations (geometric
  doublings) that *serve* ~2000 rows of page items — the faithful "fewer,
  larger allocations" arena behavior.
- Zig **3** = underlying `mmap`s to back the workload's 9-std-page live peak
  (the 4-page preheat alone needs 2 mmaps); arena_resize always fails so each
  growth is a fresh mmap node, O(log items).

---

## 3. Audit-claim validation

### Claim #2 — lazy-zero mmap → lower RSS **and** lower latency
**REFUTED (for this workload).**
- Latency: NEW **0.6371 ms** vs OLD **0.4241 ms** — NEW is **50% slower**, not
  faster.
- Peak RSS: NEW **11.37 MB** vs OLD **10.06 MB** — NEW is **higher** (though
  within RSS noise of 10–18 MB; call this INCONCLUSIVE-but-not-lower for RSS).

The predicted lazy-zero latency/RSS win did **not** materialize at the
rustty level. Lazy zero-fill avoids eagerly touching pages, but for this
working set the pages are written anyway (rows are populated), so the
zero-fill is not actually saved; meanwhile the `DynamicMemoryPoolAligned`
+ mmap path adds per-page bookkeeping/indirection that costs ~0.2 ms.
The lazy-zero benefit is real for *sparse/over-reserved* page pools, which
this dense grow→reflow workload is not.

### Claim #3 — arena → fewer / larger allocations
**CONFIRMED.**
- Page allocs: NEW **7** vs OLD **14** — a clean **2× reduction**, exactly the
  arena-doubling "fewer, larger" signature. OLD makes one allocation per
  standard page; NEW amortizes ~2000 page items across 7 geometric backing
  blocks. Page bytes are essentially flat (8.20 MB → 8.37 MB, +2.1%), so the
  win is purely in *allocation count*, as predicted.

### Overall — new-rustty approaches the Zig reference
**INCONCLUSIVE / MIXED.**
- Latency: NEW **0.637 ms** is **closer to** Zig **0.737 ms** than OLD's
  0.424 ms is — but it approaches from *below* and overshoots: NEW is actually
  **faster** than the Zig reference here, while OLD was faster still. So
  "approaches Zig" is true in the sense of matching the same allocator
  architecture, but rustty is not converging *up* toward Zig latency — both
  rustty variants are quicker than the Zig bench (different geometry: rustty
  std page ≈586 KB / Zig 589,824 B with 16 KiB min vs 4 KiB, and the Zig bench
  carries `terminal`-module overhead).
- RSS: Zig **7.73 MB** is the lowest of the three; NEW **11.37 MB** is the
  highest. NEW did **not** approach Zig RSS — it moved away from it.
- Alloc count: NEW **7** sits between OLD **14** and Zig **3**, i.e. NEW moved
  in the right direction (halved) but is still 2.3× the Zig mmap count — the
  remaining gap is Zig's 16 KiB-page geometry and `mach.taggedPageAllocator`
  preheat, not an architectural defect in NEW.

---

## 4. Surprises / honesty notes

1. **The headline regression is latency.** The faithful re-architecture made
   the *common* page-allocation path ~50% slower (0.42 → 0.64 ms) on this
   dense grow/reflow workload. The arena+mmap indirection is not free, and the
   lazy-zero "win" is a no-op when every page is written anyway. This is a real
   regression, reported plainly — the audit predicted a latency *win* and the
   measurement shows a latency *loss*.

2. **RSS went up, not down.** Despite lazy zero-fill, NEW's peak RSS is higher
   than OLD's. Within the (large) RSS noise band, but it is certainly not the
   predicted reduction.

3. **The one clean, unambiguous win is allocation count** (14 → 7, exactly 2×),
   which is the arena claim and it held perfectly.

4. **rustty is faster than the Zig bench** at this workload — a surprise given
   the framing of "approaching" Zig from below. This is mostly a geometry/
   harness artifact (Zig's 16 KiB min page + larger std page + `terminal`-module
   overhead in the standalone exe), not evidence that rustty's allocator is
   superior; it means the latency comparison to Zig should be read with care.

5. **"Page allocs" is not the same unit across columns** (per-page `alloc_zeroed`
   for OLD vs arena backing-blocks for NEW vs mmaps for Zig). The numbers are
   each individually MEASURED and meaningful, but the 14 vs 7 vs 3 comparison is
   directional, not a like-for-like ratio. Stated here so the table is not
   over-read.

---

### Source captures
- NEW: `/tmp/bench_new.txt`; harness `crates/rustty-vt/src/pagelist/bench.rs`
  (committed `1cfa7a3`, trailer present).
- OLD: `/tmp/bench_old.txt`; ephemeral worktree harness preserved at
  `/tmp/bench_old_harness.rs` (worktree removed, HEAD undisturbed).
- Zig: `/tmp/bench_zig.txt`; committed `.port/phase-1/bench-zig-pagelist/RESULTS.txt`
  (commit `7f7ba77`, trailer present).
