# Perf Comparison — Rust `rustty-bench` vs Zig `ghostty-bench`

**Machine.** Apple M4 Pro (`Mac16,7`), arm64, 14 logical cores, macOS 26.5.1
(build 25F80).
**Date.** 2026-07-15.
**Methodology.** See `.port/phase-2/PERF_COMPARISON_PLAN.md`. Both binaries were
driven by the **same** bench-generic harness
`.port/phase-0/baseline/run_benchmarks.py` with **N=7** timed reps + 1 discarded
warmup, single-process `time.monotonic()` wall-clock, stdout+stderr → `DEVNULL`,
strictly serial (no parallel runs, no concurrent builds). The Zig and Rust
halves ran back-to-back on the same idle machine.

- Zig binary: `.port/phase-0/baseline/bench-releasefast/bin/ghostty-bench`
  (Zig ReleaseFast).
- Rust binary: `target/release/rustty-bench` (rebuilt release before timing).
- Corpora (**identical files feed both binaries**, produced by `ghostty-gen`):
  `utf8.bin` 64 MiB (`+utf8 --seed=42`), `ascii.bin` 64 MiB (`+ascii`),
  `osc-parser.bin` 64 MiB (`+osc --style=parser --p-valid=0.5`),
  `ascii-small.bin` 256 KiB.
- Raw per-rep records: `.port/phase-2/perf/zig.jsonl`, `.port/phase-2/perf/rust.jsonl`.
- All 14 Rust configs exited cleanly (exit 0) — no stub/`unimplemented!()` aborts.
- **Generator parity (independent check):** `rustty-synthetic +utf8 --seed=42`
  is **byte-identical** to `ghostty-gen +utf8 --seed=42` over an 8 MiB prefix
  (`cmp`, IDENTICAL). The bench corpora on disk are the single shared source of
  truth, so no generator drift can confound these numbers.

`delta_pct = (rust_median − zig_median) / zig_median × 100`; **positive = Rust
slower**. Throughput/clones columns: **higher is better** (opposite sign).

## Per-config table

| id | corpus | zig_median_ms | rust_median_ms | delta_pct | zig | rust | metric |
|---|---|--:|--:|--:|--:|--:|---|
| `terminal-parser/utf8` | utf8.bin | 286.36 | 833.69 | +191.1 | 223.5 | 76.8 | MiB/s |
| `terminal-stream/ascii` | ascii.bin | 570.41 | 577.84 | +1.3 | 112.2 | 110.8 | MiB/s |
| `codepoint-width.noop/utf8` | utf8.bin | 4.83 | 6.92 | +43.4 | 13258.4 | 9244.0 | MiB/s |
| `codepoint-width.table/utf8` | utf8.bin | 245.87 | 302.02 | +22.8 | 260.3 | 211.9 | MiB/s |
| `codepoint-width.simd/utf8` | utf8.bin | 539.54 | 637.68 | +18.2 | 118.6 | 100.4 | MiB/s |
| `codepoint-width.wcwidth/utf8` | utf8.bin | 273.46 | 347.22 | +27.0 | 234.0 | 184.3 | MiB/s |
| `grapheme-break.noop/utf8` | utf8.bin | 244.62 | 320.55 | +31.0 | 261.6 | 199.7 | MiB/s |
| `grapheme-break.table/utf8` | utf8.bin | 280.99 | 366.50 | +30.4 | 227.8 | 174.6 | MiB/s |
| `is-symbol.table/utf8` | utf8.bin | 258.63 | 321.22 | +24.2 | 247.5 | 199.2 | MiB/s |
| `is-symbol.uucode/utf8` | utf8.bin | 252.44 | 309.47 | +22.6 | 253.5 | 206.8 | MiB/s |
| `osc-parser/osc-parser` | osc-parser.bin | 62.67 | 204.19 | +225.8 | 1021.3 | 313.4 | MiB/s |
| `screen-clone.noop/small` | ascii-small.bin | 7.88 | 15.82 | +100.6 | 126834 | 63230 | clones/s |
| `screen-clone.clone/small` | ascii-small.bin | 57.93 | 117.88 | +103.5 | 17263 | 8483 | clones/s |
| `screen-clone.render/small` | ascii-small.bin | 19.98 | 49.89 | +149.7 | 50054 | 20045 | clones/s |

(`zig`/`rust` columns are throughput MiB/s for data benches, clones/s for
`screen-clone`.)

## Summary (honest)

The Rust port is **slower than the Zig reference on every one of the 14
configs** — there is no bench where Rust wins. The gap ranges from
near-parity to ~3.3×:

- **Near parity — 1 bench.** `terminal-stream/ascii` is effectively tied
  (+1.3%, 110.8 vs 112.2 MiB/s), the one config within noise.
- **Moderate gap (~+18% to +31%) — the codepoint-width / grapheme-break /
  is-symbol Unicode-property families.** These are steady per-codepoint table
  and SIMD lookups where Rust runs ~20–30% behind Zig ReleaseFast on the same
  64 MiB utf8 corpus. Tight spreads (max/min ≤ 1.13) — these deltas are real,
  not noise.
- **Large gap (~+150% to +226%) — `terminal-parser`, `osc-parser`, and all
  three `screen-clone` modes.** These are the heaviest algorithmic paths:
  the full VT parser (833.7 vs 286.4 ms, 2.9×), the OSC parser (204.2 vs
  62.7 ms, 3.3×), and screen cloning/rendering (2.0×–2.5×). `terminal-parser`
  and `osc-parser` have tight spreads (≤1.30), so their large deltas are
  genuine algorithmic differences, not measurement artifacts — the ported
  parser/state-machine and page-clone paths are the clearest optimization
  targets.

### Process-spawn / setup-floor caveat

Every wall-clock includes process spawn + untimed setup, not just step work.
The `codepoint-width.noop` floor is **4.83 ms (Zig) vs 6.92 ms (Rust)** — Rust's
runtime-startup floor is ~2 ms higher, which inflates the *percentage* delta of
short benches. This matters most for:

- `codepoint-width.noop` (+43.4%): almost entirely the ~2 ms startup-floor
  difference on a ~5 ms measurement, **not** algorithmic.
- `screen-clone.*` (256 KiB corpus, few-ms floor): the `screen-clone.noop`
  Rust reps were **noisy** (8.3–24.5 ms, max/min = 2.9× vs Zig's tight
  7.2–8.3 ms). Its median (+100.6%) is spawn-floor-dominated and jittery; read
  it as directional. `screen-clone.clone` and `.render` do enough real work
  (1000 clones/step) that their ~2×–2.5× gaps clear the floor and are
  meaningful.

For the large data benches (`terminal-parser`, `osc-parser`, the 240–640 ms
utf8 family) the sub-10 ms floor is negligible against the medians, so those
deltas reflect real compute cost.

### Notes

- Corpora are the **exact same `ghostty-gen` bytes** on disk for both binaries
  (§0 of the plan); the utf8 generator was additionally proven byte-identical,
  so generator behavior cannot skew the comparison.
- `codepoint-width.noop` is the smallest utf8-data median for **both** binaries,
  satisfying the do-nothing-floor validity check (§5.3).
