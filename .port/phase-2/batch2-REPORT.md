# Phase 2 — Batch 2 Integration Report

**Verdict: GREEN**
**CI RESULT: PASS** (`bash .port/tools/ci.sh` exit 0)
**Commit:** `7a0a3b4` on branch `phase-1-parity-gates`
**Git tree:** clean (all batch-2 work committed)
**Date:** 2026-06-21

Crates integrated this batch: **rustty-global** and **rustty-unicode** (both `PORT STATUS: done`).

---

## CI (single entry point: `.port/tools/ci.sh`)

`CI RESULT: PASS` with exit code 0. Aggregated result:

- PASSED (6 hard-fail): `cargo fmt --check`, `cargo clippy -D warnings`, `cargo build --workspace`, `cargo test --workspace`, G7 (acyclic), G4 (differential control).
- PROGRESS (8, all green): G1, G2, G3, G3b, G9, G8, G5, G6 (Miri).
- WARNINGS (blocked): 0
- HARD FAILURES: 0

`cargo build`/`test`/`clippy -D`/`fmt` all green. G7 acyclic (29 crates, 95 edges, 0 cross-crate SCCs). G6 Miri ran for every `done` crate (build-config, datastruct, global, lib, unicode, util) — zero `Undefined Behavior` / `memory leaked` strings across the whole CI log.

---

## Per-crate results

### rustty-global (BREAK #1 — leaf process-state holder)

| Gate | Result |
|---|---|
| cargo test | GREEN — 0 tests (contract = 0), exit 0 |
| clippy -D | GREEN — exit 0 |
| fmt | GREEN — exit 0 |
| G1 tests ported | GREEN — 0/0, name-sets match |
| G2 assertions | GREEN — rust=0 >= ref=0 |
| G3 comments fidelity | GREEN — rust=327, ref=77, delta=+250 |
| G3b comment coverage | GREEN — distinctive=58, missing=0, review=0 |
| G7 acyclic | GREEN — rustty-global remains a leaf |
| G6 Miri | GREEN — 0 UB, 0 leaks, 0 tests |

Contract (`tests-summary.json`) confirms `global.zig` has **0 `test{}` blocks**, so 0 tests ported is correct (R1 satisfied). Upward deps (apprt/cli/renderer/crash/font/os/xev) recorded as DEFERRED SEAMS for rustty-core, not pulled in (R4). Only Cargo dep is the leaf `rustty-build-config`.

**Review verdict: PARITY.** Documented structural divergences (all R4 BREAK #1): `pub var state ... = undefined` -> `OnceLock<GlobalState>`; `Logging` packed struct -> plain struct (packed-parse consumer is a deferred seam in the `cli` upward crate, defaults bit-faithful, 0 tests exercise packing here); generic `GlobalState<Action, ResourcesDir>` standing in for upward field types. No silently weakened behavior, no unsafe, no dropped comment/test/assertion.

### rustty-unicode (8 files, 5 test blocks)

| Gate | Result |
|---|---|
| cargo test | GREEN — 5 passed, exit 0 |
| clippy -D | GREEN — exit 0 (3 justified PARITY `#[allow]`) |
| fmt | GREEN — exit 0 |
| G1 tests ported | GREEN — 5/5, name-sets match (+1 extra `ref_all_decls`) |
| G2 assertions | GREEN — rust=26 >= ref=18 |
| G3 comments fidelity | GREEN — rust=279, ref=107, delta=+172 |
| G3b comment coverage | GREEN — distinctive=64, missing=0, review=0 |
| G7 acyclic | GREEN — 0 cross-crate cycles |
| G6 Miri | GREEN — 5 tests pass under Miri in 857.11s, 0 UB, 0 leaks |

**Tests ported (5/5), assertion-for-assertion:**
grapheme.zig(2): `grapheme_break_emoji_modifier`, `long_emoji_zwj_sequences`; main.zig(1): `ref_all_decls`; props_uucode.zig(1): `unicode_props_tables_match_uucode`; symbols_uucode.zig(1): `unicode_symbols_tables_match_uucode`.

**Tables bit-identical: VERIFIED.** The reviewer independently rebuilt the uucode 0.2.0 + ghostty `src/build/uucode_config.zig` oracle (not reusing porter intermediates) and diffed against the embedded rustty tables: width diffs=0, is_symbol diffs=0, props all-fields diffs=0 across the full range `0..=0x10FFFF`; grapheme precompute diffs=0 across all 8192 entries; grapheme sweep zero-diff. Enum discriminants and Key/Value bit-packing match the reference packed-struct layouts.

**Review verdict: PARITY.**

---

## Resolved blockers (from porter's submission)

1. **Miri (G6) unconfirmed** — RESOLVED. `cargo +nightly miri test -p rustty-unicode` ran to completion in this CI (857.11s, 5 passed, 0 failed, zero UB/leak strings). The full-range tests were correctly NOT shrunk (R5).
2. **`committed: false` / dirty tree** — RESOLVED. Batch-2 unicode work (lib.rs + 8 ported source files + 3 generated tables + Cargo.lock) committed as `7a0a3b4`. `git status` is now clean.

## Flagged note (NOT a parity break — no action required for GREEN)

The two in-crate "tables match uucode" tests are **tautological in-crate**: R3 forbids the uucode Cargo dep, so the oracle was relocated to the external parity harness and the in-crate comparison reduces to table-vs-itself. Assertion counts are fully preserved (`assert!(false)` kept verbatim; gate rust=26 >= ref=18; no assertion dropped). **Data parity is not weakened** — the tables are independently proven bit-identical against the rebuilt uucode oracle. The weakening is confined to those two tests' self-checking power. Recommendation for a later phase: add a committed external-oracle differential test as the discriminating check. This does not block batch-2 GREEN.

---

## R3 (no ghostty dependency)

Verified: no Cargo manifest declares a dependency on the ghostty tree; all references to "ghostty" in Cargo.toml files are R3 compliance comments only. rustty-unicode depends on no ghostty/uucode crate (oracle is external-harness only). rustty-global depends only on the leaf rustty-build-config.

## Totals

**Tests ported: 98 / 3234** (global contract floor; workspace contract 3225 + 9 non-DAG asset tests). Batch-2 done crates: build-config, datastruct, lib, util, global (0), unicode (5). Progress is GREEN — no claimed-done crate is short of its contract.
