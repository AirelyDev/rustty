# Phase-0 Performance Baseline — Methodology

**Date:** 2026-06-21
**Machine:** Apple M4 Pro (`Mac16,7`), arm64, 14 logical cores, 48 GB RAM, macOS 26.5.1 (build 25F80), Darwin 25.5.0
**Reference oracle:** `/Users/gael/projects/ghostty` (READ-ONLY), pinned `4749c4e93731067049bfbf2e4572061cef2bdd17`, Zig `0.15.2`
**Isolation:** captured with no other heavy work running concurrently (per task). Benchmarks were never run in parallel with each other (per `src/benchmark/AGENTS.md`).

All structured numbers live in `BASELINE.json`. This file documents *how* they were obtained and *why* the methodology is what it is.

---

## 1. What the build stage actually produced (and what it did not)

- The build stage did **NOT** produce separate `ghostty-bench-*` binaries. `zig build -Demit-bench=true` emits a single multiplexed runner **`ghostty-bench`** (+ `ghostty-gen`). `ghostty-bench` dispatches by a `+<action>` subcommand.
- **Action names require a leading `+`** (CLI uses `cli/action.zig::detectIter`, which only treats `+`-prefixed args as actions; a bare `utf8`/`terminal-stream` yields `error: NoAction`).
- Available bench actions: `terminal-parser`, `terminal-stream`, `codepoint-width`, `grapheme-break`, `is-symbol`, `osc-parser`, `screen-clone`.
- The main **GUI `ghostty` binary was NOT built** (`zig-out/bin/ghostty` absent). This makes `ghostty +version` startup and all GUI/frame metrics unmeasurable here — recorded as TODOs, not fabricated.

## 2. How the benchmark binary reports results (it doesn't)

Reading `src/benchmark/{Benchmark.zig,cli.zig}` and each action:

- `cli.zig::mainActionImpl` calls `b.run(.once)` — every benchmark step executes **exactly once** (`RunMode.once`).
- The returned `RunResult { iterations, duration }` is **discarded**. The binary **prints nothing** on success (only `std.log` warnings to stderr on bad input).

Therefore the only signal available externally is **end-to-end process wall-clock**. That is what we measure.

`screen-clone` is special: its step body loops the clone **1000 times** internally (because a single clone is too fast to measure), so its meaningful metric is **clones/sec = 1000 / median_seconds**, and it is **data-independent** (the corpus only seeds initial screen state; setup already fills every row).

## 3. Timing harness — and a discarded first attempt

**Discarded attempt (DO NOT reuse):** a bash harness that captured the start and end timestamps with two *separate* `python3 -c 'time.monotonic()'` processes. This produced sub-millisecond and **negative** durations because `time.monotonic()` epochs are **per-process and not comparable across processes**. That entire result set was thrown away.

**Final harness:** `run_benchmarks.py` — all timing done inside a single Python process: `time.monotonic()` deltas around `subprocess.run(...)`. For each config:
- 1 warmup run (discarded), then **N timed reps** (ReleaseFast N=7, Debug N=5).
- `stdout`/`stderr` → `DEVNULL` (terminal stream/parser emit per-byte `warn` logs on control/garbage bytes; we don't want terminal logging I/O to dominate the measured work).
- Records every rep, plus **median** (headline), min, max.
- Throughput = `corpus_bytes / median_s` (data benches); `1000 / median_s` (screen-clone).

`hyperfine` (preferred by AGENTS.md) is **not installed** on this machine; this harness substitutes for it.

## 4. Corpora (generated once, reused)

Per `AGENTS.md`, generation is **not** piped into benchmarking (that would mix generation cost into the measurement). `ghostty-gen` streams **forever**, so each corpus was generated to a file in the background, the process killed once the file exceeded the target size, then truncated to an exact size with `head -c`.

| corpus | generator command | size | used by |
|---|---|---|---|
| `corpora/utf8.bin` | `ghostty-gen +utf8 --seed=42` | 64 MiB (67108864 B) | terminal-parser, codepoint-width, grapheme-break, is-symbol |
| `corpora/ascii.bin` | `ghostty-gen +ascii` | 64 MiB | terminal-stream (clean printable → represents print throughput, avoids pathological control-code warning flood seen with random utf8) |
| `corpora/osc-parser.bin` | `ghostty-gen +osc --style=parser --p-valid=0.5` | 64 MiB | osc-parser (length-prefixed records the parser bench expects) |
| `corpora/ascii-small.bin` | `head -c 262144 ascii.bin` | 256 KiB | screen-clone (keeps the untimed setup/replay cheap vs the 1000-clone loop; a 64 MiB corpus made screen-clone setup dominate at ~123 s/run) |

`utf8` uses a fixed `--seed=42` for determinism. `ascii`/`osc` generators take no CLI seed (osc uses a time seed); byte distribution is stable and only byte-throughput matters for those.

## 5. Optimize mode — Debug (as-built) vs ReleaseFast (headline)

The build stage built bench binaries at the **default optimize = Debug** (`zig build -Demit-bench=true`, no `-Doptimize`; `standardOptimizeOption` defaults to Debug). `src/benchmark/AGENTS.md` explicitly states Debug benchmarks are *"very slow and not representative"* and mandates `-Doptimize=ReleaseFast`.

So we report **both**:
- **ReleaseFast (HEADLINE, N=7):** built isolated under `phase-0/baseline/bench-releasefast/` with `zig build -Demit-bench=true -Demit-macos-app=false -Doptimize=ReleaseFast` using a dedicated `--prefix` and `--cache-dir` (the ghostty source tree was verified untouched: `git status --short` empty, HEAD still `4749c4e`). **Use these for any parity/perf comparison.**
- **Debug (as-built, N=5):** the literal build-stage binary at `ghostty/zig-out/bin/ghostty-bench`. Recorded for completeness only.

The contrast is large for compute-bound benches (e.g. terminal-stream 118.8 vs 2.1 MiB/s; screen-clone.clone 20,264 vs 182 clones/s), confirming why ReleaseFast must be the reference.

## 6. Startup time

The GUI `ghostty` binary was not built, so `ghostty +version` cannot be timed → **TODO** in `BASELINE.json`. The only available proxy is the no-argument `ghostty-bench` invocation (exits immediately with `error: NoAction`): it measures binary load + Zig runtime init + CLI arg parse, **dominated by macOS process-spawn overhead** (~5–9 ms median, 15 runs after 3 warmups, for both Debug and ReleaseFast — effectively identical because it's an immediate error-exit). **This is not a substitute for terminal GUI startup.**

## 7. Binary sizes

`stat -f '%N %z'` on each artifact. The unit-test executables live in `.zig-cache/o/<hash>/` (content-addressed, may move on rebuild), not in `zig-out`; their paths/sizes are recorded with that caveat. `libghostty-vt.dylib` / `.0.dylib` are symlinks to `libghostty-vt.0.1.0.dylib`. See `binary_sizes_bytes` in `BASELINE.json`.

## 8. Metrics deliberately NOT measured (documented TODOs, never fabricated)

- **GUI frame timing / render FPS** — needs a display + the Metal GUI renderer; GUI binary not built; headless environment.
- **Peak RSS / memory high-water mark** — the bench binary has no RSS instrumentation and runs each step once. Addable later by wrapping each invocation in `/usr/bin/time -l` and parsing *maximum resident set size*.
- **`ghostty +version` GUI startup** — GUI binary not built.

## 9. Files

- `BASELINE.json` — structured baseline (machine, methodology, binary sizes, per-benchmark medians + raw reps, startup, TODOs, blockers, reproduce commands).
- `run_benchmarks.py` — the timing harness (single-process monotonic timing).
- `corpora/` — generated input files (utf8.bin, ascii.bin, osc-parser.bin, ascii-small.bin; plus osc-stream.bin generated but unused).
- `raw/results-releasefast.jsonl`, `raw/results-debug.jsonl` — raw per-config records (all reps).
- `raw/startup.json` — startup proxy raw reps.
- `raw/run-releasefast.log`, `raw/run-debug.log` — harness stderr (per-rep timings).
- `bench-releasefast/bin/` — the ReleaseFast bench/gen binaries built for this baseline.
- `build-bench-releasefast.log` — ReleaseFast build log (exit 0; binaries present and verified runnable).
