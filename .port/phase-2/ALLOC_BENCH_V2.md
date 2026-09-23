# PageList Allocation Benchmark V2 — equal-scope re-measure (T0-B corrections + fix 12)

Supersedes the alloc rows of `.port/phase-2/ALLOC_BENCH.md` (the "7 vs 3" and
"+47% RSS" numbers), which PERF_FIX_PLAN.md §T0-B diagnosed as measurement
artifacts: unequal workload scope, a `cargo test` harness (libtest baseline +
`cfg!(test)` shrinking `grapheme_bytes` 8192→512, i.e. NOT production page
geometry), and an isolated single-arena Zig replay compared against a
full-workload rustty count.

Date: 2026-07-16. Host: Apple Silicon arm64, macOS Darwin 25.5.0
(**machine NOT idle** — another session running; all latency/RSS figures are
**NON-OFFICIAL**; alloc counts and byte totals are deterministic and exact).
Worktree: `worktree-perf-alloc`. Zig: 0.15.2 ReleaseFast; Rust: `release-fast`
profile.

---

## 1. Methodology — EQUAL SCOPE this time

### One workload, both sides (identical stage-for-stage)

The Rust bench is `crates/rustty-vt/examples/alloc_bench.rs` (feature
`alloc-bench`); the Zig bench is the pre-existing standalone harness
`.port/phase-1/bench-zig-pagelist` (rebuilt fresh and re-run this session from
a /tmp copy; the read-only reference tree untouched):

`init(80, 24, max = 1 GiB)` → `grow()` until `total_rows >= 2000` →
`scroll(row=r)` for every r then `scroll(active)` → full-screen `clone`
(deinit'd immediately) → `resize` cols 200 → 40 → 80 (reflow) → `reset()` →
`deinit`.

Same `max_size` (1 GiB), same grow loop, same scroll walk, same clone
options, same resize sequence — the divergences the old bench carried
(64 MiB vs 1 GiB max, `grow_rows(2000)` vs `grow()` loop, 120/40 vs
200/40/80 resizes, clone held live vs deinit'd) are all gone.

### Standalone, production geometry (T0-B item 2)

The tracking seam moved off `#[cfg(test)]` onto the off-by-default
`alloc-bench` cargo feature, so the Rust bench is a plain **example binary**:
no libtest, no cargo-test threading, and `cfg!(test)` is FALSE — the library
is built with the production `std_capacity` (grapheme_bytes 8192).
Run: `cargo run -p rustty-vt --example alloc_bench --features alloc-bench --profile release-fast`.

### Per-metric method

| Metric | Method |
|---|---|
| page allocs | Rust: `TrackingAllocator` child of the PagePool (`page_tracking` seam), cumulative `total_allocs()` delta over ONE workload; counts arena backing-node allocations + non-standard pages, for the FULL workload (main pool + clone pool + reset). Zig: same-mechanics decomposition — `--arena-sim` (real `std.heap.ArenaAllocator` over the real page allocator) for the main pool + clone pool item sequences, plus the reset mechanics read from `arena_allocator.zig` (rawResize on the page allocator SUCCEEDS on shrink). **DETERMINISTIC/MEASURED.** |
| peak outstanding bytes | Rust: high-water mark of tracker `outstanding_bytes()` sampled after each stage (clone sampled while its pool is live). Zig: `--arena-sim` underlying bytes at the 9-item live peak. **DETERMINISTIC/MEASURED.** |
| peak RSS | `getrusage(RUSAGE_SELF).ru_maxrss` (BYTES on macOS). Zig cross-checked with `/usr/bin/time -l`, single iteration. **MEASURED, noisy.** |
| latency | median of 9 runs, 1 warmup, both sides, same busy machine. **NON-OFFICIAL.** |

---

## 2. GEOMETRY PRECONDITION — a real finding (std_size gate, T0-B item 3)

The new integration gate `crates/rustty-vt/tests/std_size_golden.rs` found the
port's PRODUCTION std page size is **409600 B (25×16 KiB)**, not the oracle's
**589824 B (36×16 KiB)**. Section-by-section (everything else byte-identical:
rows 1720, cells 369800, grapheme_map 10768, hyperlink_set 152, hyperlink_map 464):

| section | Zig oracle | rustty | cause |
|---|---:|---:|---|
| styles | 4000 | 3168 | Zig `Style` is 28 B/align 4 → RefCountedSet Item 36 B; Rust `Style` is 21 B/align 1 → Item 28 B. Repr-level, not observable through the style API. CORRECTION (2026-09-22, NEW-FINDINGS N88): it WAS observable — the styles section feeds `availableBitsForGrid`, so rows per page, `maxCols`, the Linux std page size, scrollback kept under `scrollback-limit` and snapshot page boundaries differed. The port now lays the set out with the reference's item geometry: styles 4000, identical. |
| grapheme_alloc | 131136 | 8256 | `BitmapAllocator.layout`: the port carries upstream ghostty fix `b953bb346` (chunks region = `aligned_chunk_count*chunk_size`); the PINNED oracle predates it and over-reserves `aligned_cap*chunk_size` (~120 KiB dead, unreachable space per std page). |
| string_alloc | 65544 | 2056 | Same `b953bb346` fix (~62 KiB dead space in the oracle). |

Geometry is not observable behavior (CORRECTION 2026-09-22, NEW-FINDINGS N88: false for the styles row above, now fixed; and the 589,824-byte oracle figure was an older pin's — at `20abdb50a`, which has `b953bb346`, the reference's std page is 409,600 bytes on macOS, equal to the port's), and 180 KiB per std page of the delta is
dead (bitmap-unaddressable) space in the OLD oracle that upstream itself
removed. Per instructions the geometry was NOT changed; the golden test pins
the port's actual sections/total so future drift is caught, and documents the
oracle delta in its module doc. Consequence for comparison: rustty std pages
are smaller, so equal ROW workloads produce equal PAGE counts (verified below)
but smaller byte totals.

## 3. Results

### Per-stage accounting — IDENTICAL logical shape

Both sides, one workload (std-page equivalents = `pl.page_size / item_size`):

| stage | Zig pages | rustty pages | Zig cum-allocs (decomposed) | rustty cum-allocs (measured) |
|---|---:|---:|---:|---:|
| init (preheat 4) | 1 | 1 | 2 | 2 |
| grow →2000 rows (1976 grows, 3 new nodes) | 4 | 4 | 2 | 2 |
| scroll | 4 | 4 | 2 | 2 |
| clone (own pool, preheat=page_count) | 4 | 4 | +2 | +2 |
| resize→200 (peak) | 9 | 9 | +1 | +1 |
| resize→40 | 2 | 2 | 5 | 5 |
| resize→80 | 4 | 4 | 5 | 5 |
| reset (retain; arena rawResize + 1 node for re-init page) | 1 | 1 | 6 | 6 |

Zig grow accounting from the fresh `--accounting` re-run matches RESULTS.txt
exactly (grow_calls=1976, 3 new-node grows, 1/4/4/4/9/2/4/1). The Zig
cum-alloc column is decomposed from `--arena-sim` (9 items → 3 allocs for the
main pool; 4 items → 2 for the clone pool preheat) + reset mechanics
(rawResize succeeds on the page allocator; the post-reset `initPages` page
does not fit the retained node behind the 16-B BufNode header, so +1 node —
the same +1 rustty measures).

### Headline table

| Metric | Zig ref (fresh re-run) | rustty V2 | Δ | old (flawed) rows |
|---|---:|---:|---|---|
| page allocs (full workload) | 6 (decomposed) | **6** (measured) | **parity** | "7 vs 3" (+4) |
| peak outstanding page bytes | 7,502,136 (9×589824 item peak, arena-sim) | 5,271,864 | −30% (geometry: 409600 vs 589824 items) | 8,374,728 vs 7,502,136 (+11.6%) |
| peak RSS (bytes) | 7,733,248 | 10,321,920 | +33% NON-OFFICIAL, noisy | +47% |
| latency median (9 runs, busy machine) | 821 µs | 779 µs | −5% NON-OFFICIAL | +… (unequal scope) |

RSS note: the rustty binary carries the Rust std runtime + the whole
rustty-vt rlib (the Zig exe is a 340 KB lib_vt-only build); the ~2.6 MB
process-baseline difference dominates the page-pool working set. The
page-level metrics (allocs, outstanding bytes) are the meaningful comparison
and are at parity / better.

### Fix 12 (`arena-resize-seam`) — before/after

Measured by compiling out the `PageAllocator::resize_in_place` override
(`--cfg rustty_disable_resize_seam` experiment, not committed):

| | page allocs | reset stage delta |
|---|---:|---:|
| before (manual realloc always) | 7 | +2 (fresh node alloc + old node free) |
| after (in-place shrink via `TaggedPageAllocator::resize`) | **6** | **+1** (only the Zig-matching post-reset page node) |

Exactly the plan's predicted "+1 alloc (the reset realloc)" recovered, and the
reset mechanics now match Zig's step-for-step (`rawResize` succeeds → node
retained in place → retained memory zeroed by the `zero_retained_page_arena`
memset, which now zeroes genuinely-retained memory instead of a fresh mmap).

Seam wiring: `ResizeInPlace` trait (rustty-zigstd `arena_allocator.rs`,
default false, `Global` false) → attempted by `ArenaAllocator::reset` before
the manual-realloc fallback → implemented by `PageAllocator` (rustty-os)
delegating to the previously-dead `TaggedPageAllocator::resize` (in-place tail
munmap on shrink) → forwarded through `TrackingAllocator` with an
`on_resize` byte-ledger adjustment so the R6 leak gates see no phantom leak.

---

## 4. Verification

* `cargo test -p rustty-vt` (2036 pass) and `cargo test -p rustty-vt
  --features alloc-bench` (2036 pass) — including every pagelist tracking /
  leak-gate test, now exercising the resize path under test.
* `cargo test -p rustty-zigstd` (49) / `--features tracking`; `cargo test -p
  rustty-os` (62, incl. 2 new mmap resize tests); `cargo build --workspace`.
* New tests: arena `reset_uses_child_resize_in_place` (proves no fresh alloc
  on reset with a resize-capable child), tracking
  `resize_seam_tracks_byte_delta`, page_allocator
  `resize_in_place_shrinks_and_refuses_growth` +
  `resize_in_place_same_page_count_succeeds`, and the
  `std_size_golden` integration gate.
* The example's own leak gate: outstanding allocs/bytes == 0 after deinit
  (asserted every run).

Raw captures: Zig `--accounting`/`--arena-sim`/`--iters=9 --warmup=1`/`time -l`
re-run 2026-07-16 from a /tmp copy of `.port/phase-1/bench-zig-pagelist`
(RESULTS.txt values reproduced exactly for the deterministic metrics).
