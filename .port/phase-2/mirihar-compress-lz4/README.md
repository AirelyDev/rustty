# WS7 LZ4 block codec — isolated Miri harness

Miri evidence for `crates/rustty-vt/src/compress/lz4.rs` (WS7 Phase A, task A6).
See `../MIRI_EXCEPTIONS.md` ("WS7 Phase A — LZ4 block codec") for why the in-crate
lane (`miri-run.sh rustty-vt lz4_`) is routed here: building the 2073-test
`rustty-vt` test binary under Miri exceeded 70 minutes on this host without
completing, so the codec's Miri evidence was hostage to an unrelated build.

The codec is a dependency-free leaf module, so this harness includes the **real
source file** via `#[path]`. It is not a copy — change `lz4.rs` and this harness
interprets the change.

Run (from this directory):

```sh
cargo +nightly miri run --quiet                                  # Stacked Borrows
MIRIFLAGS=-Zmiri-tree-borrows cargo +nightly miri run --quiet    # Tree Borrows
```

## Results

| model | result | time |
|---|---|---|
| Stacked Borrows | **CLEAN** — no UB, no leak | 41 s |
| Tree Borrows | **CLEAN** — no UB, no leak | 180 s |

## What is actually being proven

The module contains **zero `unsafe`**, so there are no raw pointers whose
provenance could be violated — a clean SB/TB result is expected, and the load-bearing
findings are the rest of what Miri checks:

- no out-of-bounds slice access while decoding **arbitrary bytes** (15 truncations,
  64 single-byte corruptions, 64 fully random blocks) — the oracle's `AGENTS.md`
  requirement that "decoders must be memory-safe for arbitrary input bytes";
- no uninitialised reads through the codec's bounded wide-copy paths;
- no integer UB;
- no leaks (Miri's leak checker runs at exit).

Encoder paths executed: structured page-shaped input (the production shape),
all-zeros (maximum ratio / longest matches), text, incompressible (literal-only),
period-1..8 runs (the overlapping-match branches of `copy_match`, the codec's
sharpest edge), and 17 boundary sizes around the token/extension transitions.
Decoder contract: the exact-size requirement is asserted in **both** directions
(buffer one byte too small and one byte too large must both be rejected).

Scale is deliberately small — Miri is ~100-400x native, and the full-scale
workloads are covered by the native lanes (`cargo test -p rustty-vt --lib lz4_`
in release and debug, plus `RUSTTY_LZ4_SLOW=1` exhaustive in both profiles).
