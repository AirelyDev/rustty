# Phase-1 GATE proof — CI clean green + gate self-test

**Date:** 2026-06-21
**Run from:** `/Users/gael/projects/rustty`
**Governs:** `.port/PORT_RULES.md` (R1–R5), `DEFINITION_OF_PARITY.md` (G1–G9),
the crate-structure design spec, and `PORT_PLAN.md`.
**Oracle:** `/Users/gael/projects/ghostty` (READ-ONLY; never modified; not a Cargo
dep — R3).

This document records the Phase-1 gate proof: (1) CI clean is GREEN on the stub
workspace, (2) every divergence gate is proven to CATCH its injection (flip RED,
then GREEN on revert), (3) the R3 C-vendoring pattern links, (4) cbindgen emits
`rustty.h`. Results below are exact, copied from real runs — no fabricated greens.

Toolchain: rustc 1.93.1, cargo 1.93.1, cbindgen 0.29.4, python3 (homebrew),
clang (Apple, `/usr/bin/clang`). Note: `cc` is shell-aliased to `claude` in this
environment, so direct C compiles below use `/usr/bin/clang`; the `cc` *crate* in
`build.rs` resolves the real compiler itself and is unaffected.

---

## (1) CI clean is GREEN

`bash .port/tools/ci.sh` — single entry point (mirrored by
`.github/workflows/ci.yml`). Result: **CI RESULT: PASS**, zero hard failures.

| Step / gate | Class | Result |
|---|---|---|
| `cargo fmt --check` | HARD | PASS |
| `cargo clippy --workspace --all-targets -D warnings` | HARD | PASS |
| `cargo build --workspace` (also the live R4/G7 acyclicity proof) | HARD | PASS |
| `cargo test --workspace` | HARD | PASS |
| G7 `check_acyclic.sh` (crate graph acyclic) | HARD | PASS (exit 0) |
| G4 `run_differential.sh` (harness wired + empty-divergence) | HARD | PASS (exit 0) |
| G1 tests-ported / G2 assertions / G3 comments / G9 surface / G8 baseline / G5 fuzz / G6 leaks | PROGRESS | all green (exit 0) |

### G7 acyclic — GREEN on the stub workspace
```
rustty workspace crates parsed: 29
inter-crate runtime edges: 93
cross-crate SCCs (size>1): 0
GREEN: rustty crate graph is acyclic (G7)        exit 0
```

### G4 harness — reports the expected ~100% empty divergence
The differential harness self-test proves the comparator detects total
divergence between the reference oracle and the (empty) port, and zero divergence
for the control (reference vs reference):
```
CONTROL mean divergence = 0.0%   (bad cases: 0)        (ref vs ref)
MEASURED EMPTY-DIVERGENCE = 100.0%   (over 11 fixtures) (ref vs empty port)
cases below 100%: 0    threshold_pct 99.9    pass: true
GATE PASS: harness correctly detects total divergence (100.0%).
```
This is the expected result: the harness is wired and self-proving; it does NOT
fabricate a parity green while the port is empty (it reports "HARNESS-WIRED (port
empty)", distinct from a parity pass).

---

## (2) GATE SELF-TEST — each gate CATCHES its injection

Method: inject a real divergence into the working tree (or a candidate metrics
file), run the gate, observe RED (non-zero exit) with a precise diagnostic, then
revert and observe GREEN (exit 0). A gate that did NOT flip RED would be a
BLOCKER. **All four flipped correctly.** Each gate also ships a built-in
`--self-test` flip-proof; those all PASS too (recorded at the end).

| # | Gate | Injection | Clean exit | Injected exit | Reverted exit | Caught? |
|---|---|---|---|---|---|---|
| a | **G7** `check_acyclic.sh` | add `rustty-vt-c` dep to `crates/rustty-vt/Cargo.toml` (vt↔vt-c mutual cycle) | 0 (GREEN) | **1 (RED)** | 0 (GREEN) | YES |
| b | **G1** `check_tests_ported.py` | mark `rustty-datastruct` `PORT STATUS: done` with 0 of 76 contract tests | 0 (GREEN) | **1 (RED)** | 0 (GREEN) | YES |
| c1 | **G3** `check_comments_fidelity.py` | mark `rustty-util` `done` (8 ported comment-lines vs 218 ref → 210 dropped) | 0 (GREEN) | **1 (RED)** | 0 (GREEN) | YES |
| c2 | **G2** `check_assertions.py` | mark `rustty-args` `done`, add `fn LineIterator()` with 2 assertions (ref=8) | 0 (GREEN) | **1 (RED)** | 0 (GREEN) | YES |
| d | **G8** `check_baseline_regression.py` | candidate metrics: `terminal-parser/utf8` throughput −51.6%, `libghostty-vt.a` size +1 byte | 0 (GREEN) | **1 (RED)** | 0 (GREEN) | YES |

### (a) G7 — fake cross-crate cycle
Injection: `rustty-vt-c = { workspace = true }` added under `[dependencies]` of
`crates/rustty-vt/Cargo.toml` (vt-c already depends on vt → mutual cycle).
```
cross-crate SCCs (size>1): 1
  CYCLE: {rustty-vt, rustty-vt-c}
    rustty-vt -> rustty-vt-c
    rustty-vt-c -> rustty-vt
RED: cross-crate dependency cycle in the rustty Cargo graph     exit 1
```
Cargo independently refuses the cycle as well (the hard acyclicity proof):
`cargo metadata` exits **101** with `error: cyclic package dependency: package
rustty-vt ... depends on itself`. Reverted → `cross-crate SCCs (size>1): 0`,
GREEN, exit 0.

### (b) G1 — dropped tests (claimed-done crate short of contract)
Injection: `crates/rustty-datastruct/src/lib.rs` header flipped from
`PORT STATUS: stub` to `PORT STATUS: done` while the crate has 0 of its 76
contract `test{}` blocks.
```
datastruct      done            0        76  RED
    MISSING: datastruct::CacheTable
    MISSING: datastruct::CircBuf append
    ... (+54 more)
RED: a claimed-done crate is missing ported tests/names     exit 1   (also exit 1 under --strict)
```
Reverted → `datastruct stub 0 76 progress` GREEN, exit 0.

### (c1) G3 — dropped comments (claimed-done crate drops comment lines)
Injection: `crates/rustty-util/src/lib.rs` flipped to `PORT STATUS: done`; the
stub carries 8 comment lines vs the 218-line reference census for the util crate.
```
util            done             8     218    -210  RED
    DROPPED: util ref_total=218 rust_total=8 (210 comment lines lost)
RED: a claimed-done crate dropped comments     exit 1
```
Reverted → `util stub 8 218 -210 progress` GREEN, exit 0 (a stub is progress-only,
never graded — only a *claimed-done* drop is RED).

### (c2) G2 — dropped assertion (weakened ported test)
Injection: `crates/rustty-args/src/lib.rs` flipped to `PORT STATUS: done` plus a
ported test `fn LineIterator()` carrying 2 assertions; the reference test
`cli/args.zig::LineIterator` carries 8 (from `tests.jsonl`).
```
args            done             2     174  RED
    WEAKENED: args::LineIterator ref=8 rust=2
    WEAKENED: args::<crate-total> ref=174 rust=2
RED: a claimed-done crate weakened an assertion     exit 1
```
Reverted → `args stub 0 174 progress` GREEN, exit 0.

### (d) G8 — worse metric vs BASELINE
Injection: a candidate metrics file (`/tmp/rustty_regressed_metrics.json`, outside
the tree) with `terminal-parser/utf8` throughput 124.9 MiB/s (baseline 258.16,
−51.6%, beyond the −7% band) and `libghostty-vt.a` size 12,065,921 B
(baseline 12,065,920 B, +1 byte, beyond the 0% size tolerance).
```
compared 2 metrics; regressions 2
    REGRESS bench terminal-parser/utf8 (throughput_MiB_per_s): baseline=258.16 now=124.9 (-51.62%)
    REGRESS size libghostty-vt.a (static): baseline=12065920.0 now=12065921.0 (+0.00%)
RED: metric(s) regressed beyond tolerance     exit 1
```
Reverted (candidate removed) → progress mode GREEN, exit 0. The candidate file
lived in `/tmp`, so nothing in the tree required reverting.

### Built-in flip-proof self-tests (corroboration)
Each gate's own `--self-test` (synthetic crate/contract in a temp dir, exercising
stub-progress / done-short / done-equal / done-over arms) also passes:
`G7 SELF-TEST: PASS`, `G1 SELF-TEST: PASS`, `G2 SELF-TEST: PASS`,
`G3 SELF-TEST: PASS`, `G8 SELF-TEST: PASS` (all exit 0).

**No gate failed to flip. No BLOCKER.**

---

## (3) R3 C-vendoring links — `cargo test -p zlib-sys`

Built the `zlib-sys` sys-crate from the vendor pattern (R3): the upstream
**zlib 1.3.1** Adler-32 checksum translation unit is vendored verbatim under
`crates-sys/zlib-sys/vendor/{adler32.c,zlib.h,zconf.h}` (copied from the SAME
library the reference links; version `ZLIB_VERSION "1.3.1"` confirmed against the
read-only oracle's resolved `zlib.h`), compiled standalone via the `cc` crate in
`build.rs` (`links = "z"`), and linked through a hand-written thin FFI in
`src/lib.rs`. No ghostty dependency.

```
   Compiling zlib-sys v0.1.0 (/Users/gael/projects/rustty/crates-sys/zlib-sys)
running 4 tests
test tests::adler32_empty_is_one ... ok
test tests::adler32_ffi_null_seed ... ok
test tests::adler32_single_byte ... ok
test tests::adler32_wikipedia_known_answer ... ok
test result: ok. 4 passed; 0 failed     exit 0
```
The known-answer test asserts `adler32("Wikipedia") == 0x11E60398` (zlib's
canonical worked example), proving the vendored C compiled, linked, and runs
bit-exactly. `cargo clippy -p zlib-sys -D warnings` and `cargo fmt -p zlib-sys
--check` both clean. This is the Phase-1 proof slice; the remaining zlib
translation units append to `SOURCES` in `build.rs` with no change to the pattern.

---

## (4) cbindgen emits `include/rustty.h`

`cargo run -p xtask -- header-gen` (which shells out to `cbindgen --config
crates/rustty-vt-c/cbindgen.toml --crate rustty-vt-c --output include/rustty.h`):
```
wrote /Users/gael/projects/rustty/include/rustty.h     exit 0
```
The emitted header (R2):
- include guard `RUSTTY_H`, cbindgen autogen warning, doxygen docs;
- exports `int rustty_vt_abi_version(void);` — the `rustty_` prefix (R2);
- **zero** `ghostty_*` / `_ghostty` symbols (the only literal "ghostty" is the
  prose docstring "Zig->Rust port of libghostty-vt", not a symbol);
- compiles cleanly as **C** and as **C++** (`cpp_compat`):
  `clang -Iinclude -c` and `clang -Iinclude -xc++ -c` both succeed;
- regeneration is deterministic (re-running produces no diff).

The real `terminal/c/` ABI surface + the `rustty/vt/*` header family replace the
proof stub when break #4 lands; the cbindgen pipeline is proven now.

---

## (5) Commit & reverted-clean

Before committing, the tree was verified clean of all injections:
- no `PORT STATUS: done` markers remain in `crates/`;
- no `rustty-vt-c` dependency in `crates/rustty-vt/Cargo.toml`.

The gate infrastructure was committed as
`rustty: phase-1 parity-gate infrastructure + self-test`
(Co-Authored-By: Claude Opus 4.8 (1M context)). The commit contains the gate
suite (`.port/tools/gates/`, `ci.sh`, `CI.md`), the CI workflow
(`.github/workflows/ci.yml`), the differential-harness additions, the cbindgen
config + emitted `include/rustty.h` + xtask `header-gen`, the identity crate R2
fill-in, the zlib-sys vendor-proof slice, and this report — with NO injection
artifacts.

---

## Result summary

- CI clean: **GREEN** (all hard checks + acyclic G7 + harness G4 green; ~100%
  empty divergence reported as expected).
- All four divergence gates **CATCH** their injection (RED on inject, GREEN on
  revert): G7 cycle, G1 dropped test, G3 dropped comment, G2 dropped assertion,
  G8 worse metric. (G2 and G3 together satisfy the "dropped comment / dropped
  assertion" requirement.)
- R3 vendor pattern **LINKS**: `cargo test -p zlib-sys` → 4/4 pass.
- cbindgen **EMITS** `include/rustty.h` with `rustty_*` symbols, zero `ghostty_*`,
  compiles as C/C++.
- **No BLOCKER.** No gate failed to flip.
