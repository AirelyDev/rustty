# PERF_LOG — perf-opt branch: per-fix attribution & accept/revert decisions

Session: 2026-07-16, Apple M4 Pro, macOS 26.5.1. All numbers are N=7 medians
(1 warmup discarded), sequential runs, identical `ghostty-gen` corpora, machine
gated on cordo-session quiet (sustained load ≈ 6–8 incl. 4 stray `yes` procs on
4 of 14 cores; single-threaded benches unaffected — fresh Zig ran *faster* than
the prior committed baseline, worst-case −9% on data benches).

Zig reference: `ghostty-bench` ReleaseFast (phase-0 binary), measured fresh in
the SAME session as every Rust variant (all comparisons same-session).

## Variants

| variant | code | profile |
|---|---|---|
| frozen A/B/C | pre-fix `10cc614` | A=release (overflow-checks ON), B=release-fast (checks off), C=B+fat LTO+cgu1+abort |
| stage_\<fix\>_C | cumulative chain commit | C profile |
| FINAL_A/C | post-merge `d211a84` | A / C |
| EXP-thinlto / EXP-Bfinal / EXP-demote / EXP-nocolor | `d211a84` (± noted delta) | isolation experiments |
| **SHIP-RF** | **`9e1837b` (shipped)** | **release-fast = overflow-checks off only** |

## Headline (SHIP-RF vs Zig, same session)

| bench | zig ms | rust ms | Δ | pre-fix A | code improvement |
|---|--:|--:|--:|--:|--:|
| terminal-parser | 262.9 | 322.7 | **+22.7%** | 667.1 | −51.6% |
| terminal-stream | 562.6 | 458.4 | **−18.5%** | 447.3 | +2.5% |
| codepoint-width.noop | 3.9 | 3.1 | −21.8% | 3.0 | ≈ |
| codepoint-width.table | 233.2 | 218.4 | **−6.3%** | 237.9 | −8.2% |
| codepoint-width.simd | 502.8 | 474.0 | **−5.7%** | 482.9 | −1.9% |
| codepoint-width.wcwidth | 258.6 | 242.2 | **−6.3%** | 260.4 | −7.0% |
| grapheme-break.noop | 231.3 | 219.1 | **−5.3%** | 260.1 | −15.8% |
| grapheme-break.table | 255.4 | 215.6 | **−15.6%** | 267.2 | −19.3% |
| is-symbol.table | 239.0 | 205.6 | **−14.0%** | 225.7 | −8.9% |
| is-symbol.uucode | 230.3 | 214.1 | **−7.0%** | 230.8 | −7.2% |
| osc-parser | 57.3 | 57.5 | +0.3% (tie) | 143.2 | **−59.9%** |
| screen-clone.noop | 5.6 | 5.2 | −6.7% | 5.3 | ≈ |
| screen-clone.clone | 49.7 | 79.8 | **+60.4%** | 79.0 | ≈ |
| screen-clone.render | 15.6 | 24.7 | **+57.6%** | 31.4 | −21.4% |

**10 wins / 1 tie / 3 losses. Suite total: Zig 2908 ms vs Rust 2740 ms (−5.8%).**
Before this branch: 0 wins / 1 tie / 13 losses.

## Per-fix attribution (stage deltas, C profile; values = the fix's own targets)

| fix | commit | verdict | evidence |
|---|---|---|---|
| fix-profile-lto | 9e028ec | superseded by 9e1837b | see LTO finding below |
| fix-inline-idioms | f2b7dcd | **ACCEPT** | osc 130→77 (−40%); t-parser 685→620 (−9.5%) |
| fix-utf8-dfa | 79dce2e | **ACCEPT** | whole decode family −5…−7% (cw.table 229→216, gb.table 250→236, is.table 223→209 …) |
| fix-bench-assert-tier | eb32359 | **ACCEPT** (≈noise here; correct tier regardless) | debug_assert = Zig std.debug.assert tier |
| fix-pin-keys-collect | 08299e0 | **ACCEPT** (noise-level; restores exact ref idiom) | — |
| fix-remap-hasher | cdb9a26 | **ACCEPT** (noise-level here; wins on many-pin screens) | — |
| fix-osc-fixed-capture | 11ca425 | **ACCEPT** | osc 79→49 (−38%) — zero-alloc capture restored |
| fix-color-list-prealloc | 67962f1 | **REVERTED** (408ddaa) | zero osc gain (48.9 vs 49.2); t-stream canary −3.3% when removed |
| fix-lut-bounds | 82defe5 | **ACCEPT** | gb.table 233→216 (−7%) |
| fix-parser-destage | 2bb5c42 | **ACCEPT** | t-parser 651→522 (−20%) on C; enables the A/RF-profile 322 result |
| fix-render-soa | 9789788 (merge d211a84) | **ACCEPT** | sc.render 30.9→23.9 (−23%) |

## The LTO finding (why release-fast is overflow-checks-only)

Fat LTO + cgu=1 + panic=abort (and thin-LTO, and fat-LTO-with-do_action-demoted)
all pessimize `terminal-parser` by ~60% (322→531/500/535 ms) while helping only
`osc-parser` (−16%) and `screen-clone.clone` (−16%). Isolation:

| profile @ FINAL code | t-parser | t-stream | osc | sc.clone |
|---|--:|--:|--:|--:|
| release (checks ON, no LTO) | 328.8 | 483.8 | 57.4 | 80.0 |
| checks off only (**shipped**) | 322.0 | 449.7 | 57.8 | 84.2 |
| checks off + fat LTO + cgu1 + abort | 531.1 | 484.5 | 48.4 | 70.4 |
| … + thin instead of fat | 500.6 | 469.6 | 48.4 | 70.4 |

Hypothesis (unproven): whole-program inlining of the now-`#[inline(always)]`
per-byte parser path into the bench loop explodes register pressure. Demoting
`do_action` alone did not recover it (535). Recorded as an open item; the
Cargo.toml comment documents the measurement so nobody "helpfully" re-adds LTO.

## Remaining losses & next candidates (not attempted, by protocol)

1. **screen-clone.clone +60%** — untouched by this branch (pin-keys/hasher were
   noise). Candidates: PERF_FIX_PLAN fix 9 (`fix-cell-copy-coalesce` — masked
   u64 store instead of 3 RMW bitfield setters per cell; the overflow-checked
   Range increment is already gone in release-fast), plus the LTO clone win
   (−16%) suggests cross-function inlining in the clone chain matters — a
   targeted `#[inline]` pass on the clone path may capture it without LTO.
2. **screen-clone.render +58%** — SoA landed (−23%) but Zig's carried comment
   promises ~3×. The remaining gap is likely the per-cell managed-row loop
   (style/grapheme handling). Re-profile post-SoA before more surgery.
3. **terminal-parser +22.7%** — residual vs Zig. Candidate: PERF_FIX_PLAN t3
   `t3-action-arrayvec` (drop-glue-free `[Option<Action>;3]`: Vec payloads →
   inline ArrayVec, removing per-byte drop-glue walk of the discarded return).

## Protocol compliance

- Sequential benches only; no parallel runs. Machine gated on cordo-session
  quiet (user instruction) — primary signal session-exit OR 15 min sustained
  quiet (fired at 03:06).
- Canary (`terminal-stream`): −18.5% vs Zig at ship (no regression; the
  color-prealloc canary hit was caught and reverted).
- Functional parity: full workspace test suite green at `d211a84` (3796 passed
  / 0 failed, debug lane); the 14 release-profile failures are the PRE-EXISTING
  tripwire `cfg(debug_assertions)` design (verified untouched by this branch);
  comment-coverage delta vs merge-base = 0; acyclic gate green.
- Raw data: `~/.claude/jobs/22556231/tmp/perfrun/*.jsonl` (17 sweeps + 4
  experiments + 2 ship confirmations), copied to `.port/phase-2/perf-v2/`.
