# WS10 perf fences (G8 / G8b)

**Fence 1 (G8b's oracle half) compares against Ghostty's bench built at THE
ORACLE PIN** (`.port/tools/gates/ORACLE_PIN`), and every number it produces is
comparable to that binary only. Before task S-4 (2026-09-23) the oracle was the
older `4749c4e93` binary (sha256 `0fb47d26…`); the series recorded against it
(PERF_LOG.md, BASELINE.json's `benchmarks_retired_4749c4e93`) is a different
baseline, and a ratio between the two series means nothing. P7-W19 (2026-10-07)
found G8 RED on that account: `codepoint-width.wcwidth` against the retired
oracle's 0.254 s, which ran without `ensureLocale` (N173), while Fence 1 holds it
against the pin. P7-W23 (V17, 2026-10-07) re-based BASELINE.json's `benchmarks`
to the pinned oracle on an idle host (`run_benchmarks.py <oracle> <out> 7`, load
1.85; see `benchmarks_rule` there), so G8 and Fence 1 now read the same oracle.
P8-W10 PERF (2026-10-10) re-ran that recipe at the current pin (load 1.44), with
the size labels re-based beside it.

## The measurement substrate is UNTRACKED

Neither the oracle bench binary nor the corpora are in git. The corpora exist
only in the **shared checkout**; the oracle binary lives in the host's oracle
artifact directory:

| what | path | size |
|---|---|---|
| oracle bench binary (the pin) | `~/rustty-oracle-artifacts/ghostty-bench-<first 9 hex of ORACLE_PIN>` | 4 029 184 B |
| corpus (utf8) | `.port/phase-0/baseline/corpora/utf8.bin` | 67 108 864 B |
| corpus (ascii) | `.port/phase-0/baseline/corpora/ascii.bin` | 67 108 864 B |
| corpus (osc) | `.port/phase-0/baseline/corpora/osc-parser.bin` | 67 108 864 B |
| corpus (ascii-small) | `.port/phase-0/baseline/corpora/ascii-small.bin` | 262 144 B |

`ws10_suite.py` references them by absolute path (the corpora in the shared
checkout), so it works from a worktree. If any is absent the gate **BLOCKS
(exit 3)**, and it also BLOCKS when the oracle binary's sha256 is not
`ORACLE_SHA256` in `ws10_suite.py` — a missing or different substrate is not a
green pass.

## The oracle binary: provenance and rebuild

The stored binary (sha256 `79a642786cc3f9e6be52a9104b01c684387364a8c99d871b200a4decb8fb0d79`,
`otool -X -t` sha256 `301f3e9b…`; `ghostty-gen` beside it as `ghostty-gen-<9 hex>`,
sha256 `47b6257a…`) was rebuilt at the current pin by P7-W19 (2026-10-07) with
Zig 0.16.0 from a `git archive` of the pin, so the oracle checkout was not
touched. The previous pin's binary (task S-4, sha256 `46b5adfb…`) stays beside it
as `ghostty-bench-20abdb50a`. At this pin `src/build/GhosttyBench.zig` builds
`ghostty-bench` and `ghostty-gen` ReleaseFast whatever `-Doptimize` says, so there
is no Debug oracle bench any more. The recipe:

```sh
PIN=$(cat .port/tools/gates/ORACLE_PIN)
ORACLE=$(python3 -c 'import sys; sys.path.insert(0, ".port/tools/gates"); import _gatelib; print(_gatelib.oracle_root())')
W=$(mktemp -d) && mkdir "$W/src"
env -u GIT_DIR -u GIT_WORK_TREE git -C "$ORACLE" archive "$PIN" | tar -x -C "$W/src"
cp -Rc "$ORACLE/zig-pkg" "$W/src/"          # the fetched dependencies (ignored dir)
(cd "$W/src" && zig build -Demit-bench=true -Demit-macos-app=false \
    -Doptimize=ReleaseFast --prefix "$W/out" --cache-dir "$W/cache")
cp -p "$W/out/bin/ghostty-bench" ~/rustty-oracle-artifacts/ghostty-bench-${PIN:0:9}
shasum -a 256 ~/rustty-oracle-artifacts/ghostty-bench-${PIN:0:9}
rm -rf "$W"
```

A rebuild is functionally identical but NOT byte-identical: a second build from
a fresh archive and cache produced the same `__TEXT,__text`, `__TEXT,__const`
and `__DATA,__data` (`otool -X -t` sha256 `bda2d764…`), but a different LC_UUID
and debug map (they record the build directory), so a different file sha256. So
keep the stored file; a rebuilt one needs `ORACLE_SHA256` updated in the same
commit, with the section check above as its evidence. The bench needs
`-Doptimize=ReleaseFast`; a Debug build measures ~57x slower on
`terminal-stream`. The binary runs the reference's global init (`.tool`),
including `ensureLocale`, exactly as `rustty-bench` does (N173).

When the pin advances, rebuild at the new pin and update `ORACLE_SHA256` and
`ORACLE_SHA256_PIN_DIGEST` (the first 16 hex of the new pin's sha256) in the same
commit. Until both are updated every oracle run BLOCKS: the file name follows the
pin, and the digest ties the recorded sha256 to the pin it was built at, so
neither an un-rebuilt oracle nor the old binary copied to the new name can pass.
`--self-test` arm [f] covers the identity check (sha mismatch, missing pin,
digest mismatch, and that the live digest matches ORACLE_PIN).

The retired `4749c4e93` binary stays at
`.port/phase-0/baseline/bench-releasefast/bin/ghostty-bench` (shared checkout)
and `~/rustty-oracle-artifacts/ghostty-bench-4749c4e93`, sha256
`0fb47d268c7b9a7f9f93ed14834ca221383ba189404b69d15f750ff76abf5cdb`. Do not
delete or overwrite it: it is the anchor of the historical series and of G8's
BASELINE.json, and that revision needs Zig 0.15.2, which this host no longer
has.

## The size axis (G8, V16)

BASELINE.json's `binary_sizes_bytes` are three oracle artifacts built
ReleaseFast at the pin (P7-W22; the library with `zig build -Demit-lib-vt=true
-Doptimize=ReleaseFast`, kept in `~/rustty-oracle-artifacts/` as
`libghostty-vt.0.1.0.dylib-<9 hex>`, sha256 in BASELINE.json). Each names its
Rustty counterpart (`rustty_counterpart`, built by `rustty_build`) and its own
`tolerance`, set as the measured gap plus 2 points; the gap is itemized in
`gap_attribution`. The static `libghostty-vt.a` is in `binary_sizes_dropped`:
a Rust staticlib archives std and every dependency unpruned, so the dylib is
the size fence for that code. `ws10_suite.py --emit-metrics` measures the
counterparts (the bench label: the `--rust` candidate). `check_perf_fence.sh
--build` (what `ci.sh` runs as G8b) builds them first, once the substrate is
present, from the labels' `rustty_build` (`ws10_suite.py --print-size-builds`):

```bash
cargo build -p rustty-bench --profile release-fast --bin rustty-bench
cargo build -p rustty-synthetic --profile release-fast --bin rustty-gen
cargo build -p rustty-vt-c --release
```

An unbuilt counterpart is left out of the metrics, and G8 (`--strict`) reports
it NOT-MEASURED. Sizes are deterministic, so they need no idle host. When the
pin advances, rebuild the oracle artifacts and re-base the bytes, the
counterparts' `rustty_bytes_at_rebase` and the tolerances by the same rule.

## The two fences

### Fence 1 — ORACLE ("hold the 14/14 wins")

rustty vs the pinned oracle, both measured **alternately in one process**.
(The spreads and margins quoted below were measured against the retired
`4749c4e93` binary; against the pinned binary the thinnest margins are
`terminal-stream` and `screen-clone.render`, about −2% each in task S-4's
provisional run.)

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

Rebuilding the oracle binary: see "The oracle binary: provenance and rebuild"
above. The retired series' 14 Debug rows are in BASELINE.json's
`benchmarks_dropped_debug` (P7-W23): the pin builds no Debug bench, so a
re-base cannot reproduce them. When the pin advances, re-run the recipe in
`benchmarks_rule` on an idle host and update the two values
`check_baseline_regression.py` pins (`REBASE_PIN_TS`, `REBASE_PIN_SC`).
