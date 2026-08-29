# Perf Comparison — the 7 benchmarks added to ghostty in the last 2 months

Covers `apc-parser`, `hyperlink-map`, `terminal-resize`, `terminal-snapshot`,
`terminal-formatter`, `scrollback-compression`, `page-compression` — none of
these have a prior comparison (the original 14/14 rustty-vs-zig comparison in
`../PERF_COMPARISON.md` predates all of them). See the parent session for
scope discussion.

**Machine.** Same host as the original comparison (Apple Silicon, macOS).
**Date.** 2026-08-28.
**Oracle.** Ghostty `origin/main` @ `4540d499a` (fresh build — the old pinned
oracle `4749c4e93` predates 3 of these 7 benchmarks entirely and can't be used).
Built with Zig 0.16.0 (now installed; wasn't available for the original
comparison, which is why that one was pinned to an old commit).
**Methodology.** `run.py`, same shape as `.port/phase-0/baseline/run_benchmarks.py`:
single-process wall-clock, N=7 timed reps + 1 discarded warmup, median reported,
stdout/stderr discarded, byte-identical `--data` corpora fed to both binaries.
**Build profiles.** Zig: `-Demit-bench -Doptimize=ReleaseFast -Demit-macos-app=false`.
Rust: `cargo build --profile release-fast` (the established parity profile —
`overflow-checks=false`, no LTO; see `../PERF_LOG.md` for why LTO is banned).
**Corpora.** Generated once via `rustty-synthetic` into `corpora/`: `ascii-2m.vt`
(2 MiB, terminal-snapshot), `ascii-16m.vt` (16 MiB, scrollback/page-compression),
`styled-1m.vt` (1.25 MiB, `--style-rate=0.3`, terminal-formatter), `kitty-4m.vt`
(4 MiB, apc-parser). terminal-resize and hyperlink-map are fully synthetic
(no `--data`), using each benchmark's default fill parameters.

**Note on generator determinism.** `ghostty-gen +ascii`/`+kitty` on current
`origin/main` are NOT reproducible across runs even with the same flags — the
shared RNG seed in `src/synthetic/cli.zig` is hardcoded to wall-clock time
(`// TODO: Make this a command line option.`), a regression introduced by the
Zig 0.16 `std.Io` rewrite. This doesn't affect these results: the corpora are
fixed files on disk, generated once, and fed identically to both binaries —
it only means `ghostty-gen` can no longer regenerate byte-identical corpora
on demand the way the original comparison's methodology note assumed.

`delta_pct = (rust_median − zig_median) / zig_median × 100`; positive = Rust slower.
Three full runs were taken back-to-back (same binaries, same corpora, no
rebuilds between them) to check reproducibility; all three are reported.

## Per-config table

| id | run1 % | run2 % | run3 % | range across runs | zig throughput (run3) | rust throughput (run3) |
|---|--:|--:|--:|--:|--:|--:|
| `apc-parser` | -19.2% | -37.3% | -34.7% | -37.3% to -19.2% | 433.4 MiB/s | 663.6 MiB/s |
| `hyperlink-map.lookup` | -21.9% | -16.8% | -13.1% | -21.9% to -13.1% | — | — |
| `hyperlink-map.churn` | +84.3% | +37.0% | +39.2% | +37.0% to +84.3% | — | — |
| `terminal-resize.cols` | +27.5% | +28.9% | +26.9% | +26.9% to +28.9% | — | — |
| `terminal-resize.cols-no-reflow` | +6.8% | +6.2% | +4.7% | +4.7% to +6.8% | — | — |
| `terminal-resize.rows` | +33.9% | +33.9% | +33.2% | +33.2% to +33.9% | — | — |
| `terminal-resize.both` | +26.0% | +28.9% | +28.6% | +26.0% to +28.9% | — | — |
| `terminal-snapshot.encode` | +295.4% | +276.7% | +297.9% | **+276.7% to +297.9%** | 69.4 MiB/s | 17.4 MiB/s |
| `terminal-snapshot.decode` | +49.8% | +52.1% | +50.8% | +49.8% to +52.1% | 35.9 MiB/s | 23.8 MiB/s |
| `terminal-snapshot.decode-ready` | +14.6% | +12.5% | +9.1% | +9.1% to +14.6% | 155.6 MiB/s | 142.6 MiB/s |
| `terminal-formatter.noop` | -24.6% | -21.2% | -19.3% | -24.6% to -19.3% | 94.7 MiB/s | 117.3 MiB/s |
| `terminal-formatter.format-vt` | +83.5% | +85.5% | +85.1% | +83.5% to +85.5% | 6.0 MiB/s | 3.2 MiB/s |
| `terminal-formatter.format-plain` | +21.1% | +22.6% | +22.1% | +21.1% to +22.6% | 7.6 MiB/s | 6.2 MiB/s |
| `scrollback-compression.compress` | +3.4% | +12.3% | +2.6% | +2.6% to +12.3% | 528.1 MiB/s | 514.8 MiB/s |
| `scrollback-compression.incremental` | +1.4% | +8.9% | +6.2% | +1.4% to +8.9% | 540.3 MiB/s | 508.8 MiB/s |
| `scrollback-compression.restore` | +5.7% | +5.4% | +11.4% | +5.4% to +11.4% | 509.9 MiB/s | 457.8 MiB/s |
| `page-compression.compress` | -36.7% | -34.6% | -37.4% | -37.4% to -34.6% | 1091.3 MiB/s | 1743.7 MiB/s |
| `page-compression.store` | -35.5% | -34.8% | -36.8% | -36.8% to -34.8% | 1124.0 MiB/s | 1777.3 MiB/s |
| `page-compression.decompress` | -38.5% | -33.0% | -33.1% | -38.5% to -33.0% | 933.6 MiB/s | 1395.8 MiB/s |

**Rust faster on 6/19 configs, Zig faster on 13/19 — identical split on all
three runs.** Spread (max/min rep) was ≤1.28 on every config within a run —
no wild outliers within a run. Run2→run3 drift was ≤3pp on almost every
config; run1 was the noisier outlier overall (likely cold-cache effects from
being the very first suite execution), most visible on `hyperlink-map.churn`
(+84.3% in run1 vs +37-39% in runs 2-3) and `apc-parser` (-19.2% in run1 vs
-35 to -37% in runs 2-3) — both short (~6-18ms) benchmarks where process-spawn
floor noise matters most. `terminal-snapshot.encode`, the largest gap, held
rock-solid at +277% to +298% across all three runs — that finding is not
noise.

## Summary

- **Rust clearly wins:** `page-compression` (all 3 modes, −35% to −39%, high-throughput
  LZ4 codec path, 1–1.7 GiB/s) and `apc-parser`/`hyperlink-map.lookup` (~−20%).
  `terminal-formatter.noop` (pure floor/setup overhead) also favors Rust,
  reversing the old comparison's finding that Rust's process-startup floor was
  higher — worth a spot-check but not something to read into a 10-14ms measurement.
- **Near parity:** `scrollback-compression` (all 3 modes, +1% to +6%) and
  `terminal-resize.cols-no-reflow` (+6.8%).
- **Zig clearly wins, and by a lot:**
  - `terminal-snapshot.encode` is the standout gap at **+295% (3.95×)** — Rust
    encodes at 17.3 MiB/s vs Zig's 68.3 MiB/s. `decode` is +50%. This is the
    single highest-priority optimization target in this batch — it lines up
    with the SIMD/vectorization work upstream did on the encoder this window
    (`vectorize grid row/cell encoding`, `batch grapheme suffix codec`,
    `slicing-by-16 CRC32C`) that rustty's port may not have carried the same
    vectorization strategy for.
  - `terminal-formatter.format-vt` (+83.5%) and `hyperlink-map.churn` (+84.3%)
    are the next tier — both point at a specific hot loop (VT-emission
    formatting; hash-map remove/reinsert) rather than a broad structural gap,
    since their sibling configs (`format-plain`, `.lookup`) are much closer.
  - `terminal-resize` (cols/rows/both, +26% to +34%) is a moderate, consistent
    gap across every reflow-triggering mode — the reflow path itself, not a
    specific mode.

Raw per-rep records: `zig.jsonl`, `rust.jsonl`. Corpora: `corpora/`.

---

# Post-fix results (2026-08-28, same session — after the gap-closing work)

Diagnosis in `DIAGNOSIS.md` (4 agent reports + CPU profiles): no upstream
optimization was missing; every gap was the port flattening Zig comptime
machinery into runtime code (runtime enums for comptime params, un-inlined
cross-crate hashes, a comptime constant recomputed per call, zeroed stack
buffers where the reference uses `undefined`). Fixes + per-step measurements
in `DIAGNOSIS.md` "Fixes applied".

Same harness, same corpora, N=7 + warmup, fresh full-suite run:

| id | zig_ms | rust_ms | delta now | delta before |
|---|--:|--:|--:|--:|
| `apc-parser` | 10.09 | 6.94 | -31.2% | -36.0% |
| `hyperlink-map.lookup` | 9.02 | 4.91 | -45.6% | -15.0% |
| `hyperlink-map.churn` | 10.63 | 6.29 | **-40.8%** | +38.1% |
| `terminal-resize.cols` | 342.60 | 412.55 | +20.4% | +27.9% |
| `terminal-resize.cols-no-reflow` | 2336.27 | 2434.34 | +4.2% | +5.5% |
| `terminal-resize.rows` | 1617.29 | 2025.66 | +25.3% | +33.6% |
| `terminal-resize.both` | 332.40 | 418.57 | +25.9% | +28.8% |
| `terminal-snapshot.encode` | 28.90 | 33.49 | **+15.9%** | +287.3% |
| `terminal-snapshot.decode` | 56.52 | 63.01 | **+11.5%** | +51.5% |
| `terminal-snapshot.decode-ready` | 15.00 | 10.97 | **-26.9%** | +10.8% |
| `terminal-formatter.noop` | 13.86 | 10.43 | -24.7% | -20.3% |
| `terminal-formatter.format-vt` | 208.15 | 253.30 | **+21.7%** | +85.3% |
| `terminal-formatter.format-plain` | 164.48 | 176.22 | +7.1% | +22.4% |
| `scrollback-compression.compress` | 30.08 | 32.16 | +6.9% | +7.5% |
| `scrollback-compression.incremental` | 30.23 | 31.41 | +3.9% | +7.6% |
| `scrollback-compression.restore` | 32.51 | 35.27 | +8.5% | +8.4% |
| `page-compression.compress` | 16.25 | 9.71 | -40.3% | -36.0% |
| `page-compression.store` | 15.08 | 9.38 | -37.8% | -35.8% |
| `page-compression.decompress` | 17.69 | 12.51 | -29.3% | -33.1% |

**Rust faster on 8/19 (was 6/19).** All wall-clock; remember the floor
caveat for the short benches (see DIAGNOSIS.md — rustty's process floor is
~3 ms lower, flattering short-bench deltas in both directions).

Remaining above-noise Zig wins, with the known next steps:
- `terminal-resize` rows/cols/both (+20-26%): reflow_row inner-loop
  bounds-check elimination (resize agent report, candidates unproven);
  rows-mode interleaved spot-measurements ran as low as +6-10%, so part of
  this residual is serial-run thermal ordering.
- `terminal-formatter.format-vt` (+22%): remaining per-unit cost in the
  (now-monomorphized) write_cell_run; next lever is converting the
  `&mut dyn io::Write` seam to a generic writer through the emission chain.
- `terminal-snapshot` encode/decode (+12-16%): remaining scalar-vs-@Vector
  delta in the grid codec; a rustty-simd version of the 4-cell step is the
  next lever.
- `scrollback-compression` (+4-9%): unchanged; small and stable, not yet
  diagnosed.

Verification: full workspace test suite green post-change (rustty-vt 2968+
tests, 0 failures), formatter roundtrip byte-equal, snapshot codec exercised
by decode benches on encoded output, clippy -D clean, fmt clean, G3b/G7 and
test-count gates GREEN. The historical WS10 oracle fence is BLOCKED on this
checkout (its pinned oracle binary + corpora were cleaned; corpora now
regenerated) — substituted with a same-suite self-fence: NEW vs PRE-CHANGE
rustty-bench on 11 of the original configs, worst delta +1.5% (noise), i.e.
no regression on the old suite from these changes.

---

# Full-suite baseline (2026-08-28, post-fix session 2) — all 33 configs

First-ever run of the COMPLETE bench matrix (original 14 + perf-new7's 19)
against one oracle (current main 4540d499a, ReleaseFast). Harness:
`full_suite.py`, N=7+warmup, medians. Corpora regenerated via
rustty-synthetic (byte-parity generator). Raw: `full-baseline-{zig,rust}.jsonl`.

**Rust wins 22/33. Suite total: zig 9930ms, rust 8202ms → rust −17.4%.**

Sorted by absolute ms lost (positive = rust slower — the optimization targets):

| id | zig_ms | rust_ms | delta | abs_ms |
|---|--:|--:|--:|--:|
| terminal-resize.rows | 1615.7 | 2048.5 | +26.8% | +432.8 |
| terminal-resize.both | 332.8 | 410.7 | +23.4% | +77.9 |
| terminal-resize.cols | 341.7 | 413.0 | +20.9% | +71.3 |
| terminal-formatter.format-vt | 207.1 | 253.0 | +22.1% | +45.9 |
| terminal-stream/ascii | 55.9 | 72.4 | +29.5% | +16.5 |
| terminal-formatter.format-plain | 163.4 | 174.5 | +6.8% | +11.1 |
| scrollback-compression.restore | 31.7 | 36.3 | +14.5% | +4.6 |
| terminal-snapshot.encode | 30.5 | 34.8 | +14.0% | +4.3 |
| terminal-snapshot.decode | 58.9 | 63.0 | +7.1% | +4.2 |
| scrollback-compression.incremental | 31.8 | 33.5 | +5.4% | +1.7 |
| scrollback-compression.compress | 30.6 | 32.0 | +4.9% | +1.5 |
| (22 rust-winning configs) | | | −8% to −87% | −2405 total |

Biggest rust wins: terminal-parser −86.6% (92 vs 689ms), wcwidth −70.2%,
osc-parser −73.9%, screen-clone.clone −61.9%, cols-no-reflow −23.6% (−762ms,
largest absolute win), hyperlink-map.lookup −59.9%.

**Oracle-regression note (validated, not a measurement error):** current-main
ghostty is far slower than its own pinned-era self on terminal-parser
(~689ms vs ~286ms era) and codepoint-width.wcwidth (~815 vs ~273ms era) —
reps are tight (max/min ≤1.04), stderr shows no log spam, same corpus bytes
feed both binaries. Presumably the Zig 0.16/std.Io/uucode migration. Those
rust wins are real against today's main but partly reflect the oracle
regressing, not only rustty improving.

**terminal-stream +29.5% is a genuine rustty regression-relative-to-parity**
(July: +1.3%): upstream gained quirks_memset (deliberately not ported —
ODR risk) and the config got ~8x faster on both sides via the new bench
shape; the remaining delta is under diagnosis.

---

# Optimization wave 2 — final full-suite results (2026-08-28, same session)

Diagnosis: 4 more read-only agents (stream, formatter residual, resize
residual, R10 win-extension), all disassembly-level; fixes measured
individually. Full log to append to DIAGNOSIS.md. Raw:
`full-final-{zig,rust}.jsonl` (same harness/corpora as full-baseline).

**Rust wins 23/33 (was 22). Suite total: rust 7607ms vs zig 8968ms (−15.2%).
Rustty's own suite time improved −7.3% in this wave (8202→7607ms on identical
corpora — the clean cross-run metric; the zig side's own run-to-run swing on
cols-no-reflow muddies suite-total comparisons). Geomean across all 33
configs: rustty 1.33x faster (median 1.18x).**

Key per-config moves (rust_before → rust_now, same harness):
- terminal-resize.rows: 2048.5 → 1588.0ms (**+26.8% → −1.3%, now a WIN**) —
  `#[inline(always)]` on `Page::layout` (reference is a HARD `pub inline fn`;
  plain `#[inline]` lost to the cost model at 254 instructions) + helper
  chain + `Screen::resize` inline.
- terminal-stream: 72.4 → 60.0ms (+29.5% → +10.4%) — three scalar early-exit
  scan loops rewritten to the reference's branchless vector-reduce idiom
  (bitmask accumulate + trailing_zeros; whole-group compare with scalar-tail
  pinpointing), plus `page_assume_resident` at the print hot site.
- terminal-formatter.format-vt: 253.0 → 241.7ms (+22.1% → +15.9%) and
  format-plain 174.5 → 169.3 (+4.2%) — SGR chain `#[inline(always)]` with
  fixed-array cursors, style-scratch hoist, `BufOut` buffered emission
  (~230k dyn write calls → ~bytes/4096), `len` by value through
  `encode_codepoint`.
- scrollback-compression: all three modes now ≤ +0.3% (restore was +14.5%) —
  entirely the stream fix (the "compression" configs are dominated by the
  untimed-in-name-only VT parse of the 16MiB corpus in-process).
- resize cols/both: 413/411 → 391/395ms (+18-19% remaining) — `as_chunks`
  group scan + branchless group ops + reverse-iterator trailing-blank trim
  landed; residual is the write_cell/style-set half of reflow_row (next
  lever documented in the resize agent report).
- unicode family extended its wins (gb.table −21%, cw.table −15.7%,
  is.table −15.6%, cw.simd −12.4%) — `do_not_optimize_away` fidelity fix
  (rustc `black_box` spills to stack per call where Zig's
  `doNotOptimizeAway` is a pure register constraint — a mis-port), 256KiB
  `BufReader` on `DataFile` (the reference reads buffered; 16384 → 256
  syscalls per 64MiB corpus), `#[inline]` + ASCII early-out on
  `codepointWidth` (guarded by a new exhaustive 0..=0x10FFFF KAT).

Falsified and reverted (measured, kept for the record):
- Transposed 16x256 DFA transition table (shorter loop-carried chain on
  paper): regressed every decode config by 5-13% — the 512B→4KB table
  footprint beat the chain win. Reverted.
- Grapheme `Value` packed back to the reference's u8 bitfield (8KB table,
  one load): gb.table +7% — the two-byte struct's aligned pair-load beats
  the unpack ALU on this core. Reverted. (Both experiments echo the
  debug_assert lesson: measure, don't model.)

Remaining above-noise Zig wins, all ≤ +19%: resize.both/cols (+18-19%),
format-vt (+16%), snapshot.encode (+16%), stream (+10%), cols-no-reflow
(+5.8%, high inter-run variance — was a rust win in the baseline run),
format-plain/decode (+4%). Next levers are documented in the four wave-2
agent reports (write_cell/style-set half of reflow_row; spare-capacity
`io::Writer` for snapshot; formatter per-boundary chain; NEON
`CodepointWidth` port for cw.simd; 3-way interleaved CRC32C).

Verification: full workspace test suite green post-wave, formatter
roundtrip byte-equal, exhaustive codepointWidth KAT green, snapshot codec
tests green, clippy/fmt clean, G3b/G7/test-count gates GREEN.

On the "50% faster" target: by geomean rustty is 1.33x faster (25% less
time on the typical config) with 23/33 outright wins and every former
multi-x gap eliminated; by suite total it is −15 to −17% (dominated by the
two long resize configs). Reaching a 50% suite-total advantage would
require rustty to be ~2x Zig on the resize family specifically — the
per-call fixed costs there are now at or below Zig's (rows is a win), and
the remaining reflow inner-loop work is bounded by the same memory
bandwidth both engines share, so treat 50%-by-suite-total as out of reach
without algorithmic divergence; 50%-by-geomean is plausible with the
documented next levers plus the parser/osc/wcwidth-class wins already in
hand.
