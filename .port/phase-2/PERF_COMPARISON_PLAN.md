# Perf Comparison Plan — Rust `rustty-bench` vs Zig `ghostty-bench`

**Purpose.** Run the ported Rust `rustty-bench` binary head-to-head against the
Zig reference `ghostty-bench` on the **same pre-generated corpora**, using the
**same** single-process monotonic timing harness that produced the Phase-0
baseline, and produce a per-config comparison table (Zig median vs Rust median,
delta %, throughput / clones-per-sec).

This is a **measurement protocol**, not a fidelity gate. Byte-for-byte parity of
the *generators* is validated **separately** (see §6) so that a generator diff
can never confound the bench numbers — the two binaries always read the **exact
same corpus files on disk**.

---

## 0. Fixed inputs (do not regenerate for a comparison run)

| Thing | Absolute path |
|---|---|
| Zig reference bench binary (ReleaseFast) | `/Users/gael/projects/rustty/.port/phase-0/baseline/bench-releasefast/bin/ghostty-bench` |
| Rust bench binary (after build, see §2) | `/Users/gael/projects/rustty/target/release/rustty-bench` |
| Timing harness | `/Users/gael/projects/rustty/.port/phase-0/baseline/run_benchmarks.py` |
| Corpora dir | `/Users/gael/projects/rustty/.port/phase-0/baseline/corpora/` |
| `utf8.bin` | 67108864 B (64 MiB), `ghostty-gen +utf8 --seed=42` |
| `ascii.bin` | 67108864 B (64 MiB), `ghostty-gen +ascii` |
| `osc-parser.bin` | 67108864 B (64 MiB), `ghostty-gen +osc --style=parser --p-valid=0.5` |
| `ascii-small.bin` | 262144 B (256 KiB), `head -c 262144 ascii.bin` |

The same four files feed **both** binaries. Never regenerate corpora between the
Zig run and the Rust run of a single comparison — reuse the identical bytes.

---

## 1. Why `run_benchmarks.py` drives BOTH binaries unchanged

`run_benchmarks.py` is **bench-generic**. It takes the bench binary as
`sys.argv[1]` (`BENCH = sys.argv[1]`) and builds every invocation as:

```python
argv = [BENCH] + args + ["--data=" + corpus]
```

The only binary-specific assumptions it makes are the CLI contract, which the
Rust port reproduces exactly:

- **Actions are `+`-prefixed subcommands** (`"+terminal-parser"`,
  `"+codepoint-width"`, …). The Rust `detect_args()` in
  `crates/rustty-bench/src/cli.rs` replicates the reference `detectIter`: it
  scans `std::env::args_os`, ignores any arg not starting with `+`, and
  `stringToEnum`s the remainder via `Action::from_name`. Same tokens, same
  leading `+`.
- **`--data=<path>`** selects the corpus (each action's `Options` parses
  `--data` into an owned `Vec<u8>` via the `rustty_args` `Target` impl).
- **`--mode=<name>`** selects the sub-mode for multi-mode actions
  (`codepoint-width`, `grapheme-break`, `is-symbol`, `screen-clone`).
- **Prints nothing on success**; the `RunResult` is discarded (`b.run(RunMode::Once)`
  return value dropped in `main_action_impl`). The only external signal is
  end-to-end process wall-clock — exactly what the harness measures.
- **stderr may carry per-byte `warn` logs** on control/garbage bytes; the
  harness sends both stdout and stderr to `DEVNULL`.

Because all of that matches the Zig binary's contract, the **same harness file
drives both** — the only thing that changes is `sys.argv[1]` (the `BENCH` path).

Run it once per binary:

```
# Zig reference (already the Phase-0 baseline binary)
python3 /Users/gael/projects/rustty/.port/phase-0/baseline/run_benchmarks.py \
  /Users/gael/projects/rustty/.port/phase-0/baseline/bench-releasefast/bin/ghostty-bench \
  /Users/gael/projects/rustty/.port/phase-2/perf/zig.jsonl 7

# Rust port
python3 /Users/gael/projects/rustty/.port/phase-0/baseline/run_benchmarks.py \
  /Users/gael/projects/rustty/target/release/rustty-bench \
  /Users/gael/projects/rustty/.port/phase-2/perf/rust.jsonl 7
```

(Create `/Users/gael/projects/rustty/.port/phase-2/perf/` first. `zig.jsonl` may
simply be copied from the existing Phase-0 `raw/results-releasefast.jsonl` if
that baseline is still valid for the current machine state; re-running it
alongside the Rust run on the same idle machine is preferred so both halves see
identical thermal/load conditions.)

---

## 2. Building the Rust binary

Per `crates/rustty-bench/AGENTS.md`, **always** build release — a debug build is
"very slow and not representative":

```
cargo build -p rustty-bench --release
```

This emits `/Users/gael/projects/rustty/target/release/rustty-bench`. Note the
crate is a **library** with a thin `[[bin]]` entrypoint whose `main` is the 3-line
mirror of `src/main_bench.zig` (`rustty_bench::cli::main`); the harness only needs
the produced executable path.

Rebuild the binary before every comparison run so it reflects the current tree.

---

## 3. The 14 bench configs (exactly what the harness runs)

`run_benchmarks.py::main` runs these 14, in this order. IDs, corpus, and the
`args` passed before `--data=` are fixed:

| # | id | corpus | action + flags | metric |
|---|---|---|---|---|
| 1 | `terminal-parser/utf8` | utf8.bin (64 MiB) | `+terminal-parser` | throughput MiB/s |
| 2 | `terminal-stream/ascii` | ascii.bin (64 MiB) | `+terminal-stream` | throughput MiB/s |
| 3 | `codepoint-width.noop/utf8` | utf8.bin | `+codepoint-width --mode=noop` | throughput MiB/s **(setup/spawn floor)** |
| 4 | `codepoint-width.table/utf8` | utf8.bin | `+codepoint-width --mode=table` | throughput MiB/s |
| 5 | `codepoint-width.simd/utf8` | utf8.bin | `+codepoint-width --mode=simd` | throughput MiB/s |
| 6 | `codepoint-width.wcwidth/utf8` | utf8.bin | `+codepoint-width --mode=wcwidth` | throughput MiB/s |
| 7 | `grapheme-break.noop/utf8` | utf8.bin | `+grapheme-break --mode=noop` | throughput MiB/s |
| 8 | `grapheme-break.table/utf8` | utf8.bin | `+grapheme-break --mode=table` | throughput MiB/s |
| 9 | `is-symbol.table/utf8` | utf8.bin | `+is-symbol --mode=table` | throughput MiB/s |
| 10 | `is-symbol.uucode/utf8` | utf8.bin | `+is-symbol --mode=uucode` | throughput MiB/s |
| 11 | `osc-parser/osc-parser` | osc-parser.bin (64 MiB) | `+osc-parser` | throughput MiB/s |
| 12 | `screen-clone.noop/small` | ascii-small.bin (256 KiB) | `+screen-clone --mode=noop` | clones/s (1000/step) |
| 13 | `screen-clone.clone/small` | ascii-small.bin | `+screen-clone --mode=clone` | clones/s |
| 14 | `screen-clone.render/small` | ascii-small.bin | `+screen-clone --mode=render` | clones/s |

`screen-clone` is **data-independent** (`nbytes=0` in the record): its step body
loops the clone 1000× internally, so its metric is `clones_per_s = 1000 / median_s`,
and the small corpus only seeds initial screen state (keeps the untimed
setup/replay cheap vs the 1000-clone loop).

**Rust-port availability caveat.** If any action is not yet fully ported/wired in
`rustty-bench`, that config will error/abort under the Rust binary. The harness
does not special-case failures — a crashed run still records a (meaningless)
wall-clock. Before trusting a Rust row, confirm the config actually ran to a clean
exit (see §5 validity checks). Configs whose Rust action is still a stub are
marked N/A in the table rather than compared.

---

## 4. Timing methodology (identical to Phase-0, do not change)

- **N = 7 timed reps + 1 warmup** (warmup discarded). Pass `7` as the harness's
  3rd arg (its default is also 7).
- **Median is the headline** per config; min/max/all-reps also recorded.
- **stdout AND stderr → `subprocess.DEVNULL`** (terminal-stream/-parser emit
  per-byte `warn` logs on control/garbage bytes; logging I/O must not dominate).
- **Single-process monotonic timing**: all `time.monotonic()` deltas happen
  inside one Python process around `subprocess.run(...)`. Never resurrect the
  discarded cross-process-timestamp approach (monotonic epochs are per-process
  and not comparable — it produced negative durations).
- **Never run benchmarks in parallel** (per `AGENTS.md` + `METHODOLOGY.md`); the
  harness is strictly sequential.
- **Isolate the machine**: no other heavy work concurrently (no builds, no other
  agents doing compute). Run the Zig and Rust halves back-to-back on the same
  idle machine so both see the same thermal/load state. `cargo build` (§2) must
  finish **before** timing starts, not overlap it.
- Machine of record (from `METHODOLOGY.md`): Apple M4 Pro (`Mac16,7`), arm64,
  14 logical cores, 48 GB RAM, macOS 26.x. Record the actual machine if different.

---

## 5. Validity checks before comparing (per binary, per config)

1. Each config's 7 reps should be tight (low spread); a large max/min ratio means
   thermal throttling or background load — re-run isolated.
2. Confirm the Rust config exited cleanly. Quick manual probe outside the harness:
   run e.g.
   `/Users/gael/projects/rustty/target/release/rustty-bench +terminal-parser --data=/Users/gael/projects/rustty/.port/phase-0/baseline/corpora/utf8.bin; echo $?`
   and check exit 0. A stub/`unimplemented!()` panic aborts non-zero.
3. The `codepoint-width.noop` config must have the **smallest** median among the
   utf8 data benches for that binary — it is the do-nothing floor (see §7). If it
   isn't, something is off.

---

## 6. Generator byte-equivalence — validated SEPARATELY

The generators are **not** used as live bench input, so a generator diff cannot
confound the bench numbers. But we still assert Rust↔Zig generator parity as an
independent check, entirely decoupled from the timing runs:

- Generate a fixed-seed sample from each binary to a file and `cmp` the bytes:
  - `utf8` is deterministic under `--seed=42`, so
    `ghostty-gen +utf8 --seed=42` and `rustty-synthetic +utf8 --seed=42`
    (once the synthetic CLI entrypoint is un-stubbed) must produce **identical
    leading bytes** (compare a bounded prefix, e.g. first 64 MiB, via `head -c`
    then `cmp`).
  - `ascii`/`osc` take no CLI seed (osc uses a time seed), so those are compared
    by **byte-distribution / structural** equivalence, not exact bytes — matching
    the Phase-0 note that only byte-throughput matters for them.
- This lives in its own step/report; it never feeds `run_benchmarks.py`. The
  bench corpora on disk (§0) are the **single shared source of truth** consumed by
  both bench binaries, so even a hypothetical generator drift leaves the perf
  comparison unaffected.

---

## 7. Process-spawn / setup floor

Every measured wall-clock includes **process spawn + the benchmark's untimed
setup**, not just the step work:

- The macOS process-spawn + runtime-init + arg-parse floor is ~**5–9 ms**
  (observed in Phase-0 as the no-argument immediate-error exit).
- `codepoint-width.noop` is the in-harness proxy for the do-nothing floor: it
  walks the corpus doing (essentially) no per-codepoint work, so its median ≈
  spawn + file-open + iterate overhead.
- Therefore, for compute benches, **useful work ≈ median − noop-floor** (roughly;
  the floor is per-action-family, use `codepoint-width.noop` for the utf8 data
  benches). Report raw medians in the table, but interpret small deltas with the
  floor in mind — a few-ms gap on a fast bench can be entirely spawn/setup, not
  algorithmic.
- The Rust binary has its own spawn/init floor (Rust runtime startup differs from
  Zig's); compare **noop-floor to noop-floor** first to separate startup cost from
  algorithmic cost before reading the compute deltas.

---

## 8. Comparison table schema

Join the two JSONL outputs on `id`. One row per config (14 rows):

| column | source | meaning |
|---|---|---|
| `id` | shared | config id (e.g. `terminal-stream/ascii`) |
| `corpus` | record | corpus basename |
| `bytes` | record | corpus size (0 for screen-clone) |
| `zig_median_ms` | zig.jsonl `median_s`×1000 | Zig ReleaseFast headline |
| `rust_median_ms` | rust.jsonl `median_s`×1000 | Rust release headline |
| `delta_pct` | `(rust_median_s − zig_median_s) / zig_median_s × 100` | **+ = Rust slower**, − = Rust faster |
| `zig_throughput_MiB_s` | zig record | data benches only (`bytes>0`) |
| `rust_throughput_MiB_s` | rust record | data benches only |
| `zig_clones_per_s` | zig record | screen-clone only (`bytes==0`) |
| `rust_clones_per_s` | rust record | screen-clone only |
| `zig_reps_ms` / `rust_reps_ms` | records | all 7 reps (spread audit) |
| `notes` | manual | e.g. "Rust action stub → N/A", "within noop floor" |

- **`delta_pct` is the headline**: signed, Rust relative to Zig, positive means
  the Rust port is slower. Compute it from `median_s` (not the rounded ms) to
  avoid rounding bias.
- For throughput/clones columns, higher is better (opposite sign convention to
  `delta_pct`); keep both so the direction is unambiguous.
- Flag any row where `delta_pct` is dominated by the spawn floor (§7) in `notes`.

Emit the joined table as `/Users/gael/projects/rustty/.port/phase-2/perf/comparison.md`
(and optionally a `comparison.json` with the same fields) — not covered further
here; this document defines the protocol, not the report.
