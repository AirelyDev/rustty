# WS10 perf fences (G8 / G8b)

**Every number produced by these tools is comparable to oracle revision
`4749c4e93` — binary sha256 `0fb47d26…` — and to NO OTHER ORACLE REVISION.**
That is not a stylistic caution; see "The oracle is irreplaceable" below.

## The measurement substrate is UNTRACKED

Neither the oracle bench binary nor the corpora are in git (`.gitignore`:
"Reference-oracle BUILD outputs under `baseline/` (regenerable; large)"). They
exist only in the **shared checkout**, never in a worktree:

| what | path | size |
|---|---|---|
| oracle bench binary | `.port/phase-0/baseline/bench-releasefast/bin/ghostty-bench` | 1 553 824 B |
| corpus (utf8) | `.port/phase-0/baseline/corpora/utf8.bin` | 67 108 864 B |
| corpus (ascii) | `.port/phase-0/baseline/corpora/ascii.bin` | 67 108 864 B |
| corpus (osc) | `.port/phase-0/baseline/corpora/osc-parser.bin` | 67 108 864 B |
| corpus (ascii-small) | `.port/phase-0/baseline/corpora/ascii-small.bin` | 262 144 B |

`ws10_suite.py` references them by absolute path in the shared checkout, so it
works from a worktree. If any is absent the gate **BLOCKS (exit 3)** — a missing
substrate is not a green pass.

## The oracle is irreplaceable — do not delete or rebuild it

The pinned oracle **no longer builds on this host.** `4749c4e93` requires Zig
0.15.2 (`src/build/zig.zig` has a hard `@compileError`, plus real 0.15→0.16 std
breakage at `build.zig:27`), and the installed Zig is **0.16.0**. So:

- **Never** delete, overwrite, or rebuild
  `.port/phase-0/baseline/bench-releasefast/bin/ghostty-bench`.
- A verified-identical backup exists at
  `~/rustty-oracle-artifacts/ghostty-bench-4749c4e93`.
- If its sha256 ever differs from `0fb47d268c7b9a7f9f93ed14834ca221383ba189404b69d15f750ff76abf5cdb`,
  **STOP**: every historical number becomes incomparable. That is a
  report-and-stop condition, not something to work around.

`origin/main` *does* build (it wants 0.16.0), but it is **not** a substitute:
upstream added `src/quirks_memset.zig`, which makes the oracle ~2.8× faster on
`+terminal-stream`. Comparing rustty against it reads as a phantom rustty
regression. Filed as verification debt **V6**.

## The two fences

### Fence 1 — ORACLE ("hold the 14/14 wins")

rustty vs the pinned oracle, both measured **alternately in one process**.

- **STRICT** (10 large-corpus configs): `rust_median < zig_median`, 0% tolerance.
  Justified by measured intra-session `max/min` spread ≤1.05 on all ten; the
  thinnest measured margin is `grapheme-break.noop` at −7.5%, ~5× that spread.
- **BANDED** (4 short configs, 3.4–25 ms): `rust_median < zig_median × 1.15`.
  These are process-spawn/setup-floor dominated; measured spreads 1.53 / 1.20 /
  1.22 / 1.13.

### Fence 2 — SELF ("no self-regression") — **this is the real gate**

Candidate vs a **stored entry binary** (`.perf-bins/`), again measured
alternately in one process. Bands: **+5%** strict configs, **+15%** short ones.

Fence 2 exists because **Fence 1 and G8 both miss self-regression.** rustty is
7–60% faster than the oracle, and an oracle-relative fence spends that whole
margin as headroom. Measured 2026-07-30, G8 would not fire until rustty got:

| config | slower-than-today before G8 fires |
|---|--:|
| screen-clone.clone | **110.6%** |
| terminal-parser | **60.4%** |
| terminal-stream | **40.3%** |
| grapheme-break.noop (tightest) | 10.1% |

So G8 is an **oracle-parity fence**, not a regression fence — even after WS10 T2
fixed its `(id, optimize)` indexing bug. G8 stays armed because it also guards
binary sizes at 0% tolerance, which G8b does not measure.

### Why Fence 2 stores a BINARY, not a JSON

The *same unchanged* oracle binary measured terminal-parser at 256.2 / 263.8 /
265.5 / 286.4 ms across four sessions — an **11.8% span**. Any frozen-JSON
comparison therefore needs a ≥12% band to avoid false REDs, which is far too
loose to catch the 5–25% regressions this work can introduce. Re-measuring the
stored binary in the *same session* removes session variance entirely, leaving
only the ≤1.05 intra-session spread — which is what makes +5% meaningful.

A stored *number* is a cross-session comparison in disguise. Store the binary.

## The idle gate

`ws10_suite.py` reads `os.getloadavg()[0]` before starting. Above
`IDLE_MAX = 2.0` it still runs and still prints verdicts, but stamps
`"provisional": true` and **exits 2** — a distinct "unusable measurement" code,
neither pass nor fail. This is what stops a contended run becoming a baseline.

Exit 2 takes precedence over the fence verdict: a RED measured under load is
reported as unusable rather than as an authoritative regression. It is still
non-zero, so CI still blocks.

## Two-strike rule

Any config that fails is automatically re-run once at `2N` before being reported
RED. Both results appear in the output, so a flake is visible rather than hidden.

## ACCEPTED_TRADEOFFS

An intentional pessimization that is a *net* win (e.g. trading a slower single
`print` for a much faster bulk path) is **not** silently allowed. The task adds
its config to `ACCEPTED_TRADEOFFS` in `ws10_suite.py` with measured before/after
and a one-line justification. The table is printed on every run — no silent
waivers. Empty by design as of T3.

## Running

```bash
# Fence 1 only, and emit a metrics file that G8 consumes:
python3 .port/tools/perf/ws10_suite.py \
    --rust target/release-fast/rustty-bench -n 9 \
    --emit-metrics /tmp/ws10-metrics.json

# Both fences (the ci.sh-facing wrapper picks up .perf-bins/ automatically):
.port/tools/gates/check_perf_fence.sh

# Verdict logic self-test (no subprocesses, no substrate needed):
python3 .port/tools/perf/ws10_suite.py --self-test

# How much headroom does G8 actually have?
python3 .port/tools/perf/headroom_probe.py /tmp/ws10-metrics.json
```

Rebuilding the oracle binary is **only** possible at the pin and **only** with
Zig 0.15.2 (not installed): `zig build -Doptimize=ReleaseFast`. Note the bench
binary genuinely needs `-Doptimize=ReleaseFast`; a Debug build measures ~57×
slower on `terminal-stream` and is what BASELINE.json's second 14 rows contain.
