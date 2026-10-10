# Research dossier: performance, Rustty vs Ghostty (Zig)

Area: perf. Scope: Rustty (Rust port) against Ghostty's own benchmark binary
(`ghostty-bench`, Zig ReleaseFast). All runs on one host: Apple M4 Pro
(`Mac16,7`), 14 logical cores, macOS 26.5.1.

Provenance tags:
- **MEASURED**: computed in this research pass from recorded raw sweep files
  (`.port/phase-2/perf*/**.jsonl`) with `tmp/build_perf_data.py`. No
  benchmark was re-run.
- **DOCUMENTED**: read from a repo doc (path given). The raw data is not
  in the repo, or the number is prose.
- **ESTIMATED**: derived by reasoning.

All `.jsonl` medians I recomputed match the figures the docs quote, to 0.1 ms
(for example the r5 table: 263.8/92.5, 50.6/20.9, 551.5/343.3).

---

## 1. Headline numbers

### 1.1 The July campaign: 14/14 against the original pinned oracle `4749c4e93`

Final r5-SHIP (`a3a2b23`, merged in `349e5d1` on 2026-07-16 10:41) against
Ghostty ReleaseFast at `4749c4e93`, N=7 medians, same-session Zig control.
Data: `data/perf-ratio-r5-vs-zig-4749c4e93.csv` (MEASURED from
`.port/phase-2/perf-v5/{zig,r5ship}.jsonl`).

| bench | Zig ms | Rust ms | speedup |
|---|--:|--:|--:|
| terminal-parser | 263.8 | 92.5 | **2.85x** (−64.9%) |
| screen-clone.clone | 50.6 | 20.9 | **2.42x** (−58.7%) |
| terminal-stream | 551.5 | 343.3 | 1.61x (−37.7%) |
| grapheme-break.table | 256.4 | 211.4 | 1.21x |
| is-symbol.table | 238.6 | 200.5 | 1.19x |
| osc-parser | 57.1 | 49.4 | 1.16x |
| codepoint-width.table | 234.3 | 207.9 | 1.13x |
| is-symbol.uucode | 229.9 | 206.1 | 1.12x |
| screen-clone.render | 15.4 | 13.8 | 1.11x |
| grapheme-break.noop | 231.0 | 211.8 | 1.09x |
| codepoint-width.wcwidth | 258.4 | 237.6 | 1.09x |
| codepoint-width.simd | 501.1 | 466.4 | 1.07x |
| codepoint-width.noop* | 3.7 | 3.0 | (1.22x) |
| screen-clone.noop* | 5.5 | 4.6 | (1.20x) |

*Process-spawn-floor configs (under 6 ms). Their percentages are not
meaningful (see §3).

**14 wins / 0 ties / 0 losses. Suite sum: Zig 2897 ms, Rust 2269 ms (−21.7%).**
(MEASURED; matches `.port/phase-2/PERF_LOG.md` "Rounds 4-5 OFFICIAL".)
Intra-session max/min spread is ≤1.07 on every large config, so every
large-config margin (thinnest −6.9%) is several times the noise.

### 1.2 Where it started: 0 wins out of 14 (2026-07-15)

The first head-to-head (`10cc614`, `.port/phase-2/PERF_COMPARISON.md`, raw
`.port/phase-2/perf/{zig,rust}.jsonl`) was built with `--release` (overflow
checks on). Rust was **slower on all 14**. Suite: Zig 3106 ms, Rust 4411 ms,
**+42.0%** (MEASURED). The worst gaps were osc-parser 3.3x (204.2 vs 62.7 ms),
terminal-parser 2.9x (833.7 vs 286.4), screen-clone.render 2.5x and
screen-clone.clone 2.0x. terminal-stream was the only near-tie (+1.3%).

So **terminal-parser went from 2.9x slower to 2.85x faster**, a ~9x
improvement against the same oracle. Caveat: the 833.7 ms starting figure
comes from a different session than the round-1 frozen binary of the same
code (667.1 ms `release`, 660.1 ms `release-fast`). A conservative
same-session figure is 660 → 92.5 ms (**7.1x**).

### 1.3 The scoreboard by round (all 2026-07-16, ~10.5 hours of wall time)

`data/perf-scoreboard-by-stage.csv` (MEASURED; W/T/L counts the docs quote are
in brackets).

| stage | Rustty code | W/T/L | suite vs Zig |
|---|---|---|--:|
| first head-to-head (07-15) | `10cc614`, release | 0/1/13 | +42.0% |
| pre-fix, release-fast profile only | `10cc614` | — | +13.6% |
| round 1 ship | `9e1837b` | 10/1/3 | −5.8% |
| round 2 ship | `9e386ab` | 12/0/2 | −10.0% |
| round 3 ship | `10f2091` | [13/1/0]¹ | −11.1% |
| rounds 4+5 ship | `a3a2b23` | 14/0/0 | **−21.7%** |

¹ screen-clone.render was +2.0%. A dedicated N=21 interleaved run gave +1.2%
with fully overlapping ranges, so the docs call it statistical parity
(PERF_LOG.md "Rounds 2-3 OFFICIAL", footnote 1).

Commit timestamps (MEASURED, `git log`): the first perf fix, `9e028ec`, landed
at 01:18 and the final merge, `349e5d1`, at 10:41 on 2026-07-16. 29
`rustty: perf —` / `perf:` commits landed that day (36 in the whole history).

### 1.4 At the advanced pin: 14/14 idle, suite −27.9% (2026-10-07)

After the oracle pin advanced (Phase 6/7), P7-W23 rebuilt `ghostty-bench` at
pin `12752b2ac` (ReleaseFast). It ran on an idle host (load 1.65–1.85, Linux
VM stopped), Rustty head `ad5c0040e`. Data: `data/perf-ratio-pin-12752b2ac-idle.csv`
(DOCUMENTED, `.port/phase-2/VERIFICATION_DEBT.md` V17; raw not kept).

- **14/14 wins in two independent runs. Sum of medians: Ghostty 3,281 ms,
  Rustty 2,364 ms, −27.9%** (G8b run −27.6%).
- Widest: terminal-parser 0.221x, osc-parser 0.255x, screen-clone.clone 0.378x.
- Thinnest: **terminal-stream 0.984x** (−1.6% / −3.1%). Upstream sped this
  config up 2.8x with `quirks_memset.zig`, which Rustty deliberately does not
  port (§6.3).
- Workloads changed with the pin (Ghostty's own terminal-parser went from
  266 ms to 591 ms; screen-clone.render now runs 50,000 iterations). So these
  ratios are **not comparable** to the July table. Compare ratios within
  their own series only.

### 1.5 Breadth: 33 configs against Ghostty main `4540d499a` (2026-08-28)

This was the first run of the complete matrix: the 14 originals plus 19
newer benchmarks (apc-parser, hyperlink-map, terminal-resize,
terminal-snapshot, terminal-formatter, scrollback/page-compression). Data:
`data/perf-full33-4540d499a.csv` (MEASURED from
`.port/phase-2/perf-new7/full-{baseline,final}-{zig,rust}.jsonl`).

- Baseline: Rust wins 22/33, suite Zig 9930 ms vs Rust 8202 ms.
- After "wave 2": **Rust wins 23/33. Suite Zig 8968 ms vs Rust 7607 ms
  (−15.2%). Geomean Rust/Zig 0.750, i.e. 1.33x faster** (MEASURED).
- Caveat recorded in RESULTS.md: Ghostty main at that time was much slower
  than its own pinned-era self on terminal-parser (689 vs ~286 ms) and wcwidth
  (815 vs ~273 ms), likely from the Zig 0.16 / std.Io / uucode migration.
  Those Rust wins partly reflect the oracle regressing.
- The 10 remaining losses at that point were all ≤ +19% (resize cols/both,
  format-vt, snapshot encode, stream).

---

## 2. Methodology

Sources: `.port/phase-2/PERF_COMPARISON_PLAN.md`, `.port/phase-2/BENCHMARK_DESIGN.md`,
`.port/tools/perf/README.md`, `.port/tools/perf/ws10_suite.py` (all DOCUMENTED).

- **Same benchmark, both languages.** `crates/rustty-bench` is a 1:1 port of
  Ghostty's `src/benchmark/` CLI (`+terminal-parser`, `+screen-clone --mode=…`,
  etc., 22 source files). Both binaries read **byte-identical corpora**:
  `utf8.bin`, `ascii.bin` and `osc-parser.bin` (64 MiB each) and
  `ascii-small.bin` (256 KiB). `rustty-synthetic +utf8 --seed=42` is
  byte-identical to `ghostty-gen +utf8 --seed=42` (8 MiB prefix `cmp`).
- **Zig build:** `zig build -Demit-bench -Doptimize=ReleaseFast`. A Debug
  zigbench is ~6x slower on the alloc bench and ~57x slower on
  terminal-stream (`.port/tools/perf/README.md`). Building without
  `-Doptimize=ReleaseFast` gave misleading numbers once (per the project's
  perf notes). At pin `12752b2ac`, `GhosttyBench.zig` forces ReleaseFast for
  the bench regardless.
- **Rust build:** `cargo build -p rustty-bench --profile release-fast`. This
  profile turns overflow checks off (the Zig ReleaseFast arithmetic tier) and
  nothing else. The product `release` profile keeps overflow checks
  (ReleaseSafe trap parity). See `Cargo.toml` `[profile.release-fast]`.
- **Timing:** single-process `time.monotonic()` wall clock with stdout and
  stderr to /dev/null, N=7 (later N=9) timed reps plus 1 discarded warmup,
  medians. Runs are strictly sequential: no parallel runs and no concurrent
  builds. Zig is re-measured **in the same session** as every Rust variant.
- **Idle gate:** `ws10_suite.py` reads the 1-minute load. Above `IDLE_MAX = 2.0`
  it still prints verdicts but stamps the run `provisional` and **exits 2**
  ("unusable measurement", distinct from pass and fail).
- **Two fences:** Fence 1 is Rust vs oracle, interleaved in one process
  (STRICT 0% tolerance on the 10 large configs, BANDED +15% on 4 spawn-floor
  configs). Fence 2 is the candidate vs a **stored entry binary** (+5% / +15%).
  The binary is stored, not a JSON of numbers, because the same unchanged
  oracle binary measured terminal-parser at 256.2 / 263.8 / 265.5 / 286.4 ms
  across four sessions (an 11.8% span). "A stored number is a cross-session
  comparison in disguise." A two-strike rule re-runs any failing config once
  at 2N.
- **Oracle-relative fences can't catch self-regression** (WS10, 2026-07-30,
  PERF_LOG.md "Gate findings"). Rustty's margin acts as headroom.
  screen-clone.clone could get **110.6%** slower, terminal-parser 60.4% and
  terminal-stream 40.3% before the oracle-relative gate (G8) fired. A metrics
  file with terminal-stream throughput cut by 30% still exited 0. That is why
  Fence 2 exists.
- **G8 keying bug** (fixed WS10 T2). `BASELINE.json` held 28 rows (14 ids x
  {ReleaseFast, Debug}), and `bench_index()` keyed on id alone, so the Debug
  rows overwrote the ReleaseFast ones. terminal-stream was fenced 57x too
  loose and screen-clone.clone 111x too loose.

---

## 3. Why short-bench wall clock lies

DOCUMENTED: `.port/phase-2/perf-new7/DIAGNOSIS.md` "Cross-check" and
`.port/phase-2/perf-new7/RESULTS.md`.

- Every wall-clock figure includes process spawn and untimed setup. On
  2026-08-28 the hyperlink agent found that **Rustty's process floor is ~3 ms
  lower than Ghostty's**. On a 5–15 ms benchmark that difference dominates.
  `hyperlink-map.lookup` "won" by −15% on wall clock while being **7.5x
  slower per operation**. `hyperlink-map.churn` was 6.1x slower per op once
  floor-corrected.
- The floor flipped direction during the port. On 2026-07-15
  (PERF_COMPARISON.md) Rust's floor was ~2 ms *higher* (codepoint-width.noop
  4.83 vs 6.92 ms), which inflated Rust's short-bench losses.
- The fix is a **loops-sweep linear fit**: run with `--loops=N` for several N
  and take the slope `(t(N) − t(1)) / (N − 1)` as ns/op. Examples:
  - The WS7 D8 codec numbers: compress slope 48.7 ms/pass = 1.02 GB/s, with
    11-loop and 41-loop slopes agreeing to 0.9%.
  - screen-clone render at ~9.0 µs per full-rebuild update.
  - V18's hyperlink-map: 37.7 vs 47.5 µs/loop.
- V18 (2026-10-07) went further: with load above the idle gate, verdicts came
  from **instructions and cycles** (`/usr/bin/time -l`), which don't depend on
  load. Example: apc-parser at 943M vs 241M instructions above the empty-file
  floor.
- Corpus content limits what a config can show. `ascii.bin` is 64 MiB of
  printable ASCII with **zero ESC bytes**
  (`d.count(b'\x1b') == 0`, PERF_LOG.md WS10 T10). So terminal-stream/ascii
  measures only the print path. Three upstream CSI fast-path commits
  (`1a88f3622`, `253e4f9c3`, `300f42c7a`) are "unmeasurable against this
  repo's corpora", and no win was claimed for them.
- **A vacuous-test trap.** The first CSI bulk-vs-per-byte differential
  compared the per-byte path with itself, because `next_slice` falls back to
  per-byte when the `simd` feature is off, and it is off in
  `cargo test -p rustty-vt`. A deliberate off-by-one left all 2163 tests
  green. Mutation testing caught it.

---

## 4. Optimization history: which commit moved which benchmark

### 4.1 Waterfalls for the key benches

Data: `data/perf-waterfall-key-benches.csv` (each step is an official N=7 sweep
of the cumulative stage binary; raw jsonl named per row; same-session Zig
column included).

**terminal-parser** (Zig ~263 ms):
660.1 (pre-fix, release-fast) → **322.7** r1: inline idioms `f2b7dcd`, UTF-8
DFA bounds `79dce2e`, parser destage `2bb5c42`, profile `9e1837b` → **216.6** r2
`e1f4829` → 210.6 r3 (noise) → **92.5** r5 `be02801`.
- r2 `e1f4829`: an xctrace histogram showed ~53% of the parse loop was drop
  machinery plus materializing the discarded per-byte `[Option<Action>;3]`
  (3x64 B). The `OscDispatch(osc::Command)` variant has a destructor, which
  forces a `drop_in_place` pointer escape of the whole alloca. The fix boxed
  the dispatch payloads (`Action` shrinks to 16 B and LLVM SROAs the array
  into registers), used `InlineVec` for CSI/ESC/DCS params, and hoisted
  `self.state` (another 20.5%). Allocations per escape went down (1 box vs 2
  `to_vec`). The upper bound from severing OscDispatch entirely is 0.14 s
  (2.3x faster than Zig), not shippable.
- r5 `be02801`: a state-equal fast path makes the discarded action array
  statically drop-free on ~89% of bytes.

**screen-clone.clone** (Zig ~51 ms):
79.4 → 79.8 r1 (noise) → **56.9** r2 `ab20827` (packed `Style::eql` + coalesced
cell copy) → **53.7** r3 `1abae33` (outline cold grapheme/hyperlink arms; their
live ranges had pushed loop invariants off the register file) → 50.5 r3 ship →
**20.9** r5 `745b3c9`.
- r5 `745b3c9` batches same-style runs: the k−1 run-mates collapse to one
  `use_multiple(id, k−1)`, but only when a predicate proves each per-cell
  call would take the pure ref-bump path. Set adds go by pointer, which
  removes the 28 B by-value `Style` marshal. Zig does not do this. On an
  adversarial corpus that alternates style every cell, it is −9% vs pre but
  still behind Zig (167.6 vs 155.2 ms).

**screen-clone.render** (Zig ~15.5 ms):
30.4 → **24.7** r1 `9789788` (MultiArrayList SoA + raw-column memcpy) →
**17.0** r2 `b6f5e4f` (stop re-zeroing SoA columns) → **15.5** r3 `e6f27dc`
(hoisted column borrows, which removes per-cell bounds checks and base
reloads) → **13.8** r4 `a26a4b1` (hoist style-set base + zip walk). After r4
the styled path is 16 instructions with zero per-cell base loads, vs Zig's
17 + 2 reloads (disassembly-verified). Zig does not do this hoist.

**osc-parser** (Zig ~57 ms): 146.4 → **57.5** r1 (inline idioms `f2b7dcd`
130→77 on the LTO chain; zero-alloc fixed-writer capture `11ca425` 79→49) →
**49.2** r2 (`ab20827` shared RefCountedSet path, `9e386ab` fall-through store)
→ 49.4 final. Round 5's lane C was declared **NONE-SAFE**: the loop sits on an
M4 store-forward floor, and four variants all measured worse.

**terminal-stream** (Zig ~551 ms): flat 443–461 through r1–r3 (used as the
canary) → **343.3** r5. Two commits:
- `6ca1015` outlined the cold print arms.
- `52116f3` is a **fairness fix**: `ghostty-bench` is built with `simd=true`,
  but `rustty-bench` was running the scalar fallback. The feature is now
  enabled in the bench crate only.

The WS10 flip-proof (2026-07-30) later measured the simd effect at +8.2%, not
the ~24% recorded at r5. PERF_LOG.md says not to keep quoting the 24%.

### 4.2 Round-1 per-fix attribution

Data: `data/perf-r1-stage-chain-lto-profile.csv`, all 14 benches across 12
cumulative stage binaries (MEASURED from `perf-v2/stage_*_C.jsonl`). Caveat:
these stages were built with the LTO "C" profile that was later rejected, so
the absolute values differ from the shipped profile. The table lists the
accept/revert decisions (PERF_LOG.md):

| fix | commit | effect (own target) | verdict |
|---|---|---|---|
| inline-idioms | `f2b7dcd` | osc 130→77; t-parser 685→620 | accept |
| utf8-dfa | `79dce2e` | decode family −5…−7% | accept |
| osc-fixed-capture | `11ca425` | osc 79→49 | accept |
| lut-bounds | `82defe5` | gb.table 233→216 | accept |
| parser-destage | `2bb5c42` | t-parser 651→522 | accept |
| render-soa | `9789788` | sc.render 30.9→23.9 | accept |
| color-list-prealloc | `67962f1` | 0 osc gain; −3.3% t-stream canary | **reverted** `408ddaa` |
| bench-assert-tier, pin-keys, remap-hasher | `eb32359` `08299e0` `cdb9a26` | noise | accept (fidelity) |

### 4.3 August 2026: comptime flattening (`1d50073f6`, 24 files, +1178/−171)

Data: `data/perf-new7-comptime-fixes.csv` and
`data/perf-comptime-flattening-steps.csv` (DOCUMENTED `perf-new7/RESULTS.md`,
`DIAGNOSIS.md`).

- terminal-snapshot.encode: +295% → +85% (const-generic encoded width) →
  +28% (hoist the 4 KiB row buffer out of the loop) → **+16%** (packed
  4-value store).
- hyperlink-map.churn (per-op, floor-corrected): 6.1x → 3.9x (`#[inline]`
  Wyhash) → 1.5x (one-shot hash with a const key length) → 1.26x (inlined
  page accessors). On wall clock it went from +38% to −41%.
- terminal-resize.rows: +33.6% → +25.3% (OnceLock `std_size`) → **−1.3%**
  (`#[inline(always)]` on `Page::layout`, a 254-instruction function that is a
  hard `inline fn` in Zig; plain `#[inline]` lost to LLVM's cost model).
- terminal-formatter.format-vt: +85% → +22% → +16%.
- In wave 2, Rustty's own suite time fell from 8202 to 7607 ms (−7.3%) on
  identical corpora.

### 4.4 October 2026: N511 (APC scan, hyperlink map)

Data: `data/perf-n511-apc-hyperlink.csv` (DOCUMENTED V18/N511). The first
paired numbers at pin `12752b2ac` showed real losses: apc-parser **1.81x**,
hyperlink churn 1.26x, lookup 1.17x. Root cause: the APC scan was a scalar
port of a vector scan. `5a7f2d1c8` (P7-W20) spells the group test as the
reference's NEON loop and adds the missing hyperlink inlining. The idle
re-measure gives apc-parser **1.11x**, churn ~1.0x and lookup 1.10x. These
are still Rustty-slower on two configs; they are not part of the 14-config
suite.

---

## 5. Perf-gap classes

Data: `data/perf-gap-classes.csv`. The 2026-08-28 diagnosis found that **every**
major gap was the port flattening Zig comptime machinery into runtime code. In
each case the Rust spelling lost something the Zig construct gave for free; no
upstream optimization was missing (DIAGNOSIS.md "Fixes applied").

1. **comptime parameter → runtime enum.** This kills fixed-size stores (a
   dynamic-length `copy_from_slice` becomes a libc memcpy *per cell*), stops
   inlining and blocks dead-arm pruning. Fix: a const-generic parameter, with
   a runtime `match` at the site of the reference's `inline else`.
2. **Missing cross-crate `#[inline]`.** LTO is banned and Zig is one
   compilation unit, so every hot `pub fn` needs `#[inline]`. Hash lengths
   must be constants (`size_of::<K>()`) or the ≤16-byte branch won't fold.
3. **comptime constant → recomputed function.** Fix: `OnceLock`.
4. **`undefined` buffer → `[0; N]` inside a hot loop.** A 4 KiB memset per row
   was 36% of snapshot encode. Fix: hoist the buffer.
5. **`#[inline]` is a hint, Zig `inline` is not.** Use `#[inline(always)]`.
6. **Scalar early-exit scans** don't vectorize. Rewrite them to the
   reference's branchless bitmask + `trailing_zeros` form.
7. **`black_box` is a mis-port of `doNotOptimizeAway`.** It forces a stack
   spill and a memory clobber, while Zig's is a register constraint. Use an asm
   register sink.
8. **Enum drop glue on a per-byte return value.** Zig's `union(enum)` has no
   destructor. A droppy variant blocks SROA, so box it.
9. **Large arguments passed by value.** Zig passes big parameters by const
   reference. Rust marshals 24–28 B to the stack, so add internal raw-pointer
   entry points.
10. **Bench build-configuration mismatch** (the simd feature).

A rule that also came out of this work: changing a hot `assert!` to
`debug_assert!` made things **worse**. A Zig ReleaseFast `assert` is
`if (!ok) unreachable`, which keeps the optimizer hint. A release-mode
`debug_assert!` gives neither the check nor the hint, and some asserts were
load-bearing for bounds-check elimination.

Process change: before this campaign there were **zero** `PERF(port)` markers
in the workspace (PERF_FIX_PLAN.md T0-C, in `1e9660360`). Today there are
**153 markers in 42 files** (MEASURED: `grep -rn "PERF(port)" crates | wc -l`).
Every reshape is labelled at its site.

---

## 6. Build-profile findings

### 6.1 LTO is banned

Data: `data/perf-lto-profile-experiment.csv` (MEASURED from `perf-v2/FINAL_A`,
`EXP_Bfinal`, `FINAL_C`, `EXP_thinlto`, `EXP_demote`).

| profile @ `d211a84` | t-parser | t-stream | osc | sc.clone |
|---|--:|--:|--:|--:|
| release (overflow checks on) | 328.8 | 483.8 | 57.4 | 80.0 |
| **overflow-checks off only (shipped)** | **322.0** | 449.7 | 57.8 | 84.2 |
| + fat LTO + cgu=1 + panic=abort | 531.1 | 484.5 | 48.4 | 70.4 |
| + thin LTO | 500.6 | 469.6 | 48.4 | 70.4 |
| + fat LTO, `do_action` demoted | 535.3 | 464.3 | 49.0 | 70.2 |

LTO helps osc and clone by ~16% but makes terminal-parser **~65% slower**.
The working hypothesis (unproven) is whole-program inlining of the
`#[inline(always)]` per-byte parser path, which causes register pressure.
Demoting `do_action` did not recover it. The ban is written into
`Cargo.toml` above `[profile.release-fast]` so that nobody re-adds LTO
without re-measuring.

Because LTO is banned, missing cross-crate `#[inline]` attributes became a
recurring source of perf gaps (class 2).

### 6.2 panic strategy on the bench profile (P7-W22, N512)

`panic = "abort"` on release-fast costs terminal-parser +2.2% instructions and
saves osc-parser 4.0%. The bench profile keeps `unwind`
(Cargo.toml comment, DOCUMENTED).

### 6.3 Not porting `quirks_memset.zig`

Zig 0.16 globally disabled LLVM loop auto-vectorization, which made
compiler_rt's byte-loop `memset` ~24x slower. In terminal-stream that memset
was 63% of executed instructions. Upstream hand-vectorized it in
`src/quirks_memset.zig` (`20c3eae04`; ~2.8x on terminal-stream). Rustty
deliberately does not port it:
- `rustc` already lowers `@llvm.memset` to the platform libc `memset`.
- Exporting a strong `memset` from a Rust dylib risks ODR and
  symbol-interposition breakage across the staticlib + dylib + Swift link
  graph.

This is the reason terminal-stream is the thinnest margin at the new pin
(0.984x). Sources: `.port/phase-3/REVIEW_CLASSES.md` §5 and the READ FIRST
note in `PERF_COMPARISON.md`.

---

## 7. Memory and allocation

Data: `data/perf-alloc-memory.csv` (DOCUMENTED PERF_LOG.md "Memory-allocation
benchmarks — OFFICIAL", `.port/phase-2/ALLOC_BENCH_V2.md`).

The workload is an equal-scope standalone PageList run: init 80x24, grow to
≥2000 rows, scroll, same-pool clone, reflow 200/40/80, reset.

- **Page allocations: 6 vs 6 (parity).** An earlier report of "7 vs 3" was
  a scope error: Rust's full workload was compared against an isolated
  single-arena Zig replay. A real seam fix (in-place shrink via
  `TaggedPageAllocator::resize`) removed one realloc on reset.
- **Peak outstanding page bytes: 5.27 MB vs 7.50 MB (−30%).** This is a
  geometry difference, not a bug. The port carries upstream `b953bb346`
  (BitmapAllocator layout fix), which the pinned oracle predates. Std page
  size is 409,600 B vs 589,824 B (grapheme_alloc 8,256 vs 131,136 B,
  string_alloc 2,056 vs 65,544 B).
- **Latency: 768.6 vs 809 µs (−5%).**
- **Peak RSS on the alloc bench: +31% for Rust.** The comparison is
  asymmetric because the Rust binary carries a TrackingAllocator
  instrumentation seam. The earlier "+47%" figure was taken inside
  `cargo test --release`, which carries the libtest baseline and flips
  `cfg!(test)` page geometry.
- **Per-bench suite RSS (`/usr/bin/time -l`, r2-SHIP): Rust lower in 7 of 8
  configs.** terminal-parser 6.29 vs 7.06 MB, sc.clone 139.9 vs 157.2 MB,
  sc.render 8.04 vs 9.18 MB. osc-parser is the exception (7.50 vs 7.01 MB).
- Later correction (N88, 2026-09-22): the style-set item geometry differed
  (Rust `Style` item 28 B vs Zig 36 B). This *was* observable: it changed rows
  per page, `maxCols` and scrollback kept. The port now uses the reference's
  item geometry (styles 4000 = 4000). Perf work surfaced a fidelity bug here too.

**Binary size** (`data/perf-binary-size.csv`, DOCUMENTED BASELINE.json, pin
`12752b2ac`):

| artifact | Ghostty | Rustty | ratio |
|---|--:|--:|--:|
| bench binary | 4,029,184 | 2,957,840 | 0.73x |
| gen tool | 464,408 | 505,296 | 1.09x |
| libghostty-vt dylib vs librustty_vt_c | 1,924,784 | 2,071,760 | 1.08x (was 1.30x before N512) |

The size gap is itemized in BASELINE.json `gap_attribution`: overflow-checks
55 KB, std panic/backtrace runtime, unwind tables, and longer mangled symbols.

---

## 8. The parity bug that profiling found

`10f2091` (2026-07-16 07:23), DOCUMENTED in the commit message and
PERF_LOG.md.

During round-3 instruction-level profiling of `clone_partial_row_from`, the
diff against the oracle exposed a control-flow mismatch. The reference's
`break :hyperlink` (`page.zig:926`) exits only the labeled hyperlink block,
so the style and kitty-placeholder arms still run for that cell. The port
used `continue` on the same-page fast path, which skipped them. **A cell
carrying both a hyperlink and a style lost its style on a same-page clone**:
style_id left DEFAULT, refcount not incremented, row styled flag unset.

- Regression test: `page_cloneRowFrom_partial_hyperlink_and_style_in_same_page`,
  verified to fail before the fix and pass after.
- It was found by a perf agent that was told to diff both binaries at
  instruction level and to declare NONE-SAFE when a residual was
  parity-locked. It flagged the bug rather than fixing it inside the perf
  round.
- Point for the article: Zig's labeled-block `break` has no direct Rust
  equivalent, and a mechanical translation that uses `continue` compiles and
  passes the existing tests.

Two more measurement-adjacent findings:
- The WS10 T12 `--loops` port made Rustty's screen-clone.render do **50x** the
  pinned oracle's work (`446f80f4e` re-scaled `stepRender` from 1,000 to
  50,000 iterations). The config was put in an `INCOMPARABLE` set, not
  waived. Normalized, it is 7.69 vs 17.21 ms (−55%).
- In S-4b (N207, 2026-09-24) the retired oracle turned out to be faster
  partly because it skipped `global.init` (0 vs 127,375 log lines). Rustty's
  bench now runs the reference's global init, including `ensureLocale`, the
  same way the oracle at the pin does (N173).

---

## 9. Negative results

Data: `data/perf-negative-results.csv`. 11 falsified optimizations are
recorded with their measured effects. Notable ones:

- LTO: +65% on terminal-parser.
- Style value-memoization: +42%. SROA splits the cached 21 B Style into ~7
  GPRs, so each cache hit costs 15 scalar stores.
- Plain-run memcpy for clone: +28%.
- Branchless UTF-8 DFA: −94% on the noop lane (dead-code-eliminated) but
  +12–22% on every real lane.
- Transposed DFA table: +5–13%.
- Packed grapheme `u8`: +7%.
- `debug_assert!` swap: reproducibly worse on resize.rows.
- Four OSC loop variants: all worse.

PERF_LOG.md: "A perf series with no recorded negative results is an
under-reported one." Three changes that looked right on paper regressed when
measured, which is why the rule became "measure, don't model".

---

## 10. Proposed visualizations

1. **Per-benchmark speedup bar chart (July, 14 configs).** Horizontal bars of
   `speedup_x` (Zig/Rust), sorted, with a reference line at 1.0x. The 2
   spawn-floor configs are hatched or greyed with a footnote. Data:
   `data/perf-ratio-r5-vs-zig-4749c4e93.csv`. Why: the headline result with
   honest caveats in the encoding. The spread is wide, from 1.07x to 2.85x.
2. **Before/after dumbbell (0/14 → 14/14).** For each bench, a dot at the
   07-15 ratio and a dot at the r5 ratio on a log scale, with a line at 1.0.
   Data: `data/perf-history-by-stage.csv` (stages `0-baseline` and `5-r5-ship`).
   Why: every bench crossed the parity line within one day of work.
3. **Scoreboard step chart by round.** Stacked W/T/L bars per round plus a line
   for suite delta % (+42 → +13.6 → −5.8 → −10 → −11.1 → −21.7). Data:
   `data/perf-scoreboard-by-stage.csv`. Why: shows the campaign's shape and
   that the profile change alone did not close the gap.
4. **Optimization waterfalls (small multiples, 5 benches).** Each panel shows
   step bars of rust_ms with the commit hash labels, plus a dashed horizontal
   line for same-session Zig. Data: `data/perf-waterfall-key-benches.csv`.
   Why: links specific code changes (boxed Action, style-run batching, SoA)
   to measured effects, which suits a technical reader.
5. **LTO grouped bars.** 4 benches x 5 profiles. Data:
   `data/perf-lto-profile-experiment.csv`. Why: a counter-intuitive result
   (LTO makes the hottest loop 65% slower), and the reason missing `#[inline]`
   later became the main gap class.
6. **Comptime-flattening slope charts.** For snapshot encode, hyperlink churn,
   resize.rows and format-vt, show the gap after each fix. Data:
   `data/perf-comptime-flattening-steps.csv`. Why: illustrates the "Zig
   comptime → Rust runtime" hazard class with one fix per step.
7. **33-config heat strip or scatter.** x = baseline ratio, y = final ratio,
   log axes, with the diagonal and the 1.0 lines drawn. Data:
   `data/perf-full33-4540d499a.csv`. Why: breadth beyond the original 14
   benchmarks; wins and losses are both visible (23/33).
8. **Pin-era ratio bars (Oct).** Same form as chart 1 using
   `data/perf-ratio-pin-12752b2ac-idle.csv`, with an annotation on
   terminal-stream 0.984x explaining `quirks_memset`. Why: shows that the
   result held after three upstream resyncs, against a rebuilt oracle on an
   idle host.
9. **Memory panel.** Paired bars: page allocs 6/6, peak outstanding bytes,
   latency, and per-bench RSS for 4 configs. Data: `data/perf-alloc-memory.csv`
   and `data/perf-binary-size.csv`. Why: performance is not only time, and
   parity on allocation count is a fidelity result.
10. **Gap-class table or diagram.** Zig construct → Rust spelling that lost →
    fix. Data: `data/perf-gap-classes.csv`. Why: the most reusable lesson for
    anyone porting from Zig (or C++ templates) to Rust.
11. **Timeline strip.** Perf milestones with W/total and suite delta. Data:
    `data/perf-timeline.csv`.
12. **"Measurement traps" sidebar** (text, or a two-point illustration): a
    3 ms floor difference flips the verdict on a 5 ms bench (hyperlink lookup
    −15% wall vs 7.5x per-op), the 50x-work INCOMPARABLE config, and the
    zero-ESC corpus.

---

## Files

- Dossier: `/Users/gael/projects/rustty/.port/journey-article/research/perf.md`
- Generators: `tmp/build_perf_data.py` (MEASURED from raw jsonl),
  `tmp/build_perf_docdata.py` (DOCUMENTED transcriptions)
- Data: `data/perf-*.csv` (16 files)

---

## Fact-check

Independent re-derivation, 2026-10-08. Raw JSONL medians were recomputed with a fresh script rather than `build_perf_data.py`; git facts were checked with `git log`, and documented facts against the source docs.

| # | Fact | Verdict | Evidence / note |
|--:|---|---|---|
| 1 | Final July campaign: 14/0/0 wins, suite 2897 vs 2269 ms, -21.7% | CONFIRMED | `perf-v5/{zig,r5ship}.jsonl`: 14/14 Rust medians lower, sums 2897 / 2269, -21.7%, geomean 0.758. `a3a2b23` (merge D, round 5) and `349e5d1` ("rounds 4-5 official: 14/14 WINS ... -21.7%") exist. |
| 2 | terminal-parser 263.8 vs 92.5 ms | CONFIRMED | 2.85x. `e1f4829` (boxed dispatch payloads) and `be02801` (state-equal fast path) are both 2026-07-16. |
| 3 | screen-clone.clone 50.6 vs 20.9 ms | CONFIRMED | 2.42x. `ab20827`, `1abae33`, `745b3c9` (same-style runs) match. |
| 4 | First head-to-head: 0/14, suite +42.0%, osc 3.3x, t-parser 2.9x slower | CONFIRMED | `perf/{zig,rust}.jsonl`: 0 wins, 3106 vs 4411 ms (+42.0%). osc 204.2/62.7 = 3.26x, t-parser 833.7/286.4 = 2.91x. `10cc614` dated 2026-07-15. |
| 5 | Campaign in one day: 01:18 to 10:41 on 2026-07-16, 29 perf commits | CONFIRMED | `9e028ec0f` at 01:18:25 -0700 (first fix, fix-profile-lto) and `349e5d101` at 10:41:04 -0700. Exactly 29 `rustty: perf` subjects that day, all before 10:42, both on `349e5d1` ancestry and `--all`. Note: `tmp/perf-subject-commits.txt` has 36 lines, so it is a wider set than these 29. |
| 6 | Idle pin re-measure: 14/14 twice, suite -27.9%, terminal-stream 0.984x | CONFIRMED | VERIFICATION_DEBT V17 table: sum 3,281 vs 2,364 ms (-27.9%; G8b -27.6%), "14/14 wins in both runs", terminal-stream 0.984 (G8b 0.969). Commit `b61e273e6` (P7-W23). Caveat: V17 does not itself blame `quirks_memset` for the thin margin. That link comes from VERIFICATION_DEBT ~L1222, where `quirks_memset` makes the oracle about 2.8x faster on terminal-stream and "must NOT be ported". `src/quirks_memset.zig` exists in `../ghostty` at `12752b2ac`. Present it as an explanation, not as something measured. |
| 7 | 33-config matrix: 23/33, geomean 0.750, suite -15.2% | CONFIRMED | `perf-new7/full-final-{zig,rust}.jsonl`: 33 shared ids, 23 wins, 8968 vs 7607 ms (-15.2%), geomean 0.750 (1.33x). Both commits are dated 2026-08-28. |
| 8 | LTO: t-parser 322 to 531 ms, osc/clone about -16%, banned | CONFIRMED | `EXP_Bfinal` t-parser 322.0 vs `FINAL_C` 531.1 (+65%; `EXP_thinlto` 500.6, `EXP_demote` 535.3). osc 57.8 to 48.4 (-16.3%), clone 84.2 to 70.4 (-16.4%). The `[profile.release-fast]` comment records the revert, but it says "~60%" where the data gives +65%. `9e1837b` is "revert LTO/cgu/panic". |
| 9 | Aug gaps were comptime flattening: encode +295% to +16%, churn 6.1x to 1.26x, resize.rows +33.6% to -1.3% | CONFIRMED (with nuance) | DIAGNOSIS "Fixes applied": "every gap was the port flattening Zig COMPTIME machinery". Its headers give encode +295% to +16% and churn 6.1x to 1.26x. RESULTS.md L122 gives rows +33.6%; L225 gives +26.8% to -1.3% "now a WIN". full-final rows: 1588.0 vs 1608.2 ms = -1.3%. The DIAGNOSIS header still shows an interim "~+6-10%", so cite RESULTS for the final figure. The "4 sub-patterns (+6 more classes)" count is the dossier's own grouping, not a count stated in the source. |
| 10 | Short-bench trap: Rustty floor ~3 ms lower; lookup -15% wall vs 7.5x per op | CONFIRMED | DIAGNOSIS L156-159 and the "lookup 7.5x" header. The -15.0% wall figure is also in RESULTS L118. |
| 11 | Parity bug: `continue` vs `break :hyperlink`, fixed in `10f2091` with a fails-before/passes-after test | CONFIRMED | `git show 10f2091` (2026-07-16 07:23) cites page.zig:926. It says the bug was found in round-3 profiling and that test `page_cloneRowFrom_partial_hyperlink_and_style_in_same_page` was "verified both ways". |
| 12 | Alloc parity 6=6, peak outstanding -30%, latency -5%, RSS lower in 7/8 | CONFIRMED as documented, CAVEAT | PERF_LOG L343-357 gives every figure (809 vs 768.6 us). Caveat: ALLOC_BENCH_V2 L69 (correction 2026-09-22, N88) says the 589,824-byte oracle page figure "was an older pin's". From `20abdb50a` on, the reference std page is 409,600 B, equal to the port's. So the -30% is a geometry delta against the old pin only, not a lasting port advantage. Present it that way, or leave it out. |
| 13 | Fence headroom: clone 110.6%, G8 keying bug 57x loose, motivated self fence | CONFIRMED | PERF_LOG L940 ("57x loose", clone "111x loose") and L950 (110.6%). tools/perf/README L128-154: Fence 2 SELF stores a binary because "Fence 1 and G8 both miss self-regression". |
| 14 | PERF(port) markers 0 to 153 across 42 files | CONFIRMED | `grep -rn` gives 153 lines in 42 files today. `git grep` at `9e028ec^` gives 0 files. |
| 15 | N511: apc-parser 1.81x to 1.11x; churn 1.26x to ~1.0x | CONFIRMED | V18 table: apc 1.81 (943M vs 241M instructions), churn 1.26. After the fix (`5a7f2d1c8`), the idle re-measure gives apc 1.11x and churn 0.99-1.03x. The commit's provisional figure was 0.94x; use the idle one. |
| 16 | Binary size: bench 0.73x, gen 1.09x, dylib 1.08x (was 1.30x) | CONFIRMED | BASELINE.json `binary_sizes_bytes`: 2,957,840/4,029,184 = 0.734; 505,296/464,408 = 1.088; 2,071,760/1,924,784 = 1.076 (+7.6%). The 1.30 (2,498,112 B) comes from the P7-W19 table in VERIFICATION_DEBT ~L1388. Caveat: that same table puts bench at 0.78 (3,138,576 B) before the P7-W22 re-base, so 0.73x is the re-base value. |

No fact needed correcting. There are 4 caveats for the writer: #6 (the `quirks_memset` link is an inference), #8 (the Cargo comment says ~60%, the data +65%), #12 (the -30% depends on the old pin's geometry) and #16 (0.73x is the re-base value).
