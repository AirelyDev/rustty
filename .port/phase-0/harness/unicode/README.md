# rustty-unicode differential gate (ORACLE #2)

**Gate id:** `G4-uni` (G4-class differential, wired into `.port/tools/ci.sh`).
**Status:** GREEN — 0 diffs full-range.

## Why this exists

ghostty's `src/unicode/props_uucode.zig` and `src/unicode/symbols_uucode.zig`
each end with a test:

```zig
test "unicode props: tables match uucode"   // props_uucode.zig
test "unicode symbols: tables match uucode"  // symbols_uucode.zig
```

These tests compare ghostty's *generated* lookup tables against the **live
`uucode` oracle** — an INDEPENDENT source (`uucode.get(.width, cp)`,
`uucode.get(.is_symbol, cp)`, …) over the code-point range. That independence is
the whole point: the table is proven equal to a second, externally-sourced
computation.

rustty embeds those tables bit-for-bit (R1) but **cannot depend on `uucode` as a
Cargo dep (R3)**. Porting the reference test naively makes the only available
"oracle" rustty's own table — a tautology (`table == table`) that verifies
nothing. The honest reproduction is to keep the *independent* oracle, but as a
**harness differential gate** in a separate process. That is this directory.

This gate is the faithful, full-coverage reproduction; the in-crate
`*_tables_match_uucode` tests (`props_uucode.rs` / `symbols_uucode.rs`) are
genuine, independently-sourced **curated spot-checks** kept for the test-count
contract (5 in-crate tests). Total coverage = curated spot-check **+** this
exhaustive differential ≥ the reference. This is NOT a weakening (R5); see
`.port/phase-0/plan/DEFINITION_OF_PARITY.md` (Methodology note).

## Design

```
        uucode 0.2.0 @ 9d555245  (live, independent oracle — ORACLE #2)
        + ghostty's src/build/uucode_config.zig  (copied verbatim, pinned)
                 |
        uucode_oracle.zig  (zig build -> uucode-oracle)
                 |  dumps, for every cp 0..=0x10FFFF:
                 |    width.bin       width (u8)
                 |    symbol.bin      is_symbol (0/1)
                 |    props_flags.bin width|wcwidth_zero_in_grapheme|is_emoji_vs_base
                 |    props_gb.bin    grapheme_break_no_control (enum int)
                 |    grapheme.bin    8192-entry precompute (result|state<<1)
                 v
        oracle/*.bin  <----- diff_unicode.py -----> rust/*.bin
                 ^                                       ^
                 |                                       |
   cargo run --example dump_tables (rustty-unicode) dumps rustty's
   EMBEDDED tables over the identical range, identical layout.

        diff_unicode.py asserts 0 diffs for every field. Exit 0 = GREEN.
```

The grapheme precompute is regenerated INDEPENDENTLY in the oracle via
`uucode.grapheme.computeGraphemeBreakNoControl` over every `(state, gb1, gb2)`
key — exactly the comptime logic ghostty's `src/unicode/grapheme.zig` embeds.
The rust side re-encodes rustty's *runtime* `Precompute::DATA` (via the
`harness_precompute_bytes()` accessor in `grapheme.rs`), so the table actually
used at runtime is what gets diffed.

## Reproduce

```sh
bash .port/phase-0/harness/unicode/run_unicode_diff.sh   # exit 0 = GREEN (0 diffs)
```

Or step by step:

```sh
cd .port/phase-0/harness/unicode
zig build                                   # builds uucode-oracle (needs zig 0.16.0)
./zig-out/bin/uucode-oracle /tmp/oracle     # dump live uucode, full range
cd /Users/gael/projects/rustty
cargo run --example dump_tables -p rustty-unicode -- /tmp/rust
python3 .port/phase-0/harness/unicode/diff_unicode.py /tmp/oracle /tmp/rust
```

## Files (all committed; reproducible — does NOT rely on /tmp)

| File | Role |
|---|---|
| `uucode_oracle.zig` | the independent oracle generator (queries live uucode) |
| `uucode_config.zig` | ghostty's `src/build/uucode_config.zig`, copied verbatim (pinned @ 12752b2ac) |
| `build.zig` / `build.zig.zon` | builds `uucode-oracle` against uucode 0.2.0 @ 9d555245 (the reference pin's revision) |
| `diff_unicode.py` | byte-diffs oracle vs rust dumps; per-field 0-diff assertion |
| `run_unicode_diff.sh` | the gate runner (build → oracle → rust dumper → diff) |
| `../../../../crates/rustty-unicode/examples/dump_tables.rs` | rust-side dumper |

## Exit codes (match ci.sh)

* `0` — GREEN, 0 diffs.
* `3` — BLOCKED (zig toolchain absent) — surfaced as a warning, never silent green.
* other non-zero — RED, a real diff or build failure — a BLOCKER (R5).

## R3 note

The Zig generator and its `uucode` dependency are **harness-only tooling**. No
rustty Cargo crate depends on `uucode` or the ghostty tree. The
`harness_precompute_bytes()` accessor and `dump_tables` example are pure-Rust
glue inside `rustty-unicode` that add nothing-lost (R1) and read only the
already-embedded tables.
```
