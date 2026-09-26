# Benchmarking

The benchmark tools are split into two roles:

- `rustty-gen` (the `rustty-synthetic` crate) generates synthetic input data.
- `rustty-bench` consumes existing input data and runs a benchmark.

## Workflow

- For timing comparisons, generate data first and benchmark it later.
- Do not pipe `rustty-gen` directly into `rustty-bench` when comparing
  performance. That mixes generation cost into the measurement and makes
  branch-to-branch comparisons noisy.
- Reuse the exact same generated files when comparing revisions.
- Prefer deterministic generation inputs such as fixed seeds when the
  generator supports them.
- Keep large generated benchmark corpora outside the repository unless the
  change explicitly requires checked-in test data.

## Running Benchmarks

- Prefer `hyperfine` to compare benchmark timings.
- Benchmark the `rustty-bench` command line, not the generator.
- Use `rustty-bench ... --data <path>` with pre-generated files.
- Run multiple warmups and repeated measurements so branch comparisons are
  based on medians instead of single runs.
- When comparing branches, keep all benchmark inputs and CLI flags the same,
  including terminal dimensions.
- Never run multiple benchmarks in parallel on the same machine, as they will
  interfere with each other and produce unreliable results.

## Building

- Build benchmark tools with `cargo build -p rustty-bench --release` (and
  `cargo build -p rustty-synthetic --release` for the generator).
- During development you can also run them directly with
  `cargo run -p rustty-bench --release -- ...` /
  `cargo run -p rustty-synthetic --release -- ...`.
- Make sure you specify `--release` when building benchmarks, otherwise the
  debug build will be very slow and not representative of real performance.

## Comparing Branches

- When comparing branches, switch to that branch, build the binary, then
  rename it e.g. `target/release/rustty-bench` to
  `target/release/rustty-bench-branch1`. Replace branch1 with something
  better.
- Then switch to the other branch, build it, and rename it to
  `target/release/rustty-bench-branch2`. Replace branch2 with something
  better.
- Then run all the benchmarks with `hyperfine` comparing the N binaries
  we want to.
