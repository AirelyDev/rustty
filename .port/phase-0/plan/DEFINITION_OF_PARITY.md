# DEFINITION OF PARITY — machine-checkable parity gates

**Date:** 2026-06-21
**Status:** Phase-0 deliverable. Authoritative gate definition for the rustty port.
**Reference oracle:** `/Users/gael/projects/ghostty` @ `4749c4e` (Zig 0.15.2), READ-ONLY.
**Governs:** every porting phase/workflow. Amended by `.port/PORT_RULES.md` (R1–R5);
where this file and the Parity Constitution conflict, `PORT_RULES.md` wins.

This document defines **what "done" means** as a set of gates that a machine can
check, each with: (a) the exact command/script that runs it, (b) the file/artifact
it reads, and (c) the precise pass condition. A gate is GREEN only when its pass
condition is met by real output — never by editing the gate, the threshold, the
contract, or by `xfail`/`skip` (R5). A gate that cannot be made green honestly is
a **STOP-and-surface-a-blocker** condition, not a reason to weaken the gate.

---

## 0. Contract artifacts (the source-of-truth files)

Gates compare the rustty tree against frozen Phase-0 artifacts. These are the
*contracts*; they are written once in Phase-0 and are immutable thereafter
(changing one is a reviewed event, never a silent edit to pass a gate).

| Artifact | Produced by (Phase-0 agent) | Consumed by gates | Status now |
|---|---|---|---|
| `.port/phase-0/inventory/inventory.json` | inventory agent | G1, G2, G3, P-T, P-C, P-A | pending (dir empty) |
| `.port/phase-0/inventory/parity-surface.json` | inventory agent | G9, P-SURF | pending |
| `.port/phase-0/baseline/BASELINE.json` | baseline agent | G8 | pending (dir empty) |
| `.port/phase-0/harness/` (diff runner + golden corpora) | harness agent | G4, G5 | pending (dir empty) |
| `.port/phase-0/corpora/{cvt,config,terminfo}/` | corpora agent | G4, G5, G9 | scaffolded (empty) |
| `.port/tools/cycle_check.py`, `reach_scc.py` | (exist) | G7 | present, runs, exit 0 |
| `.port/phase-0/plan/PORT_PLAN.md` | plan agent | file↔crate map for per-crate gates | pending |

**Frozen reference counts already established** (`build-summary.md`, the R1 contract
source): total `test{}` blocks under `src/` = **3234** (3113 string-named + 45
identifier-named + 76 anonymous; 3158 named total), **11975 assertions**, across
**279** files; `src/terminal/` = **2171**; `src/terminal/c/` = **330**. The full per-module
test/comment/assertion breakdown lives in `inventory.json` once the inventory agent
emits it; the totals above are the global floor.

> **Schema note for the inventory agent.** Gates G1–G3 and the per-crate gates
> require `inventory.json` to be keyed by source file (relative to `src/`) and to
> carry, per file: `test_blocks` (count + ordered list of test names incl. the
> anonymous ones), `comment_units` (count, by class: doc / inline / TODO / license
> / ascii-diagram / citation), and `assertions` (count, where "assertion" = any
> `try std.testing.*`, `try expect*`, `assert(`, `std.debug.assert`, `comptime
> assert`, `@compileError`, and explicit `error.X` returns inside test blocks).
> `parity-surface.json` must enumerate every config option, keybind trigger+action,
> apprt action, and terminfo capability with a stable id. If these files are absent
> or lack a field a gate needs, that gate is **BLOCKED (not green, not skipped)**.

---

## 1. GLOBAL gates

Each gate below applies to the whole workspace. `$RUSTTY` = `/Users/gael/projects/rustty`.
`$REF` = `/Users/gael/projects/ghostty` (oracle only; never built as a Cargo dep, R3).
Gate scripts live under `.port/tools/gates/` (Phase-1 creates them from the specs here).

### G1 — 100% test blocks ported (count == contract) [R1]

Every reference `test{}` block has a corresponding Rust test; the count must
**equal** (not merely meet) the contract, and named tests must match name-for-name.

- **Check:**
  `python3 .port/tools/gates/check_tests_ported.py --inventory .port/phase-0/inventory/inventory.json --rust-root $RUSTTY/crates`
  The script (Phase-1): for each source file, read the contract `test_blocks` list;
  locate the ported Rust test module(s) for that file via `PORT_PLAN.md`'s file→crate
  map; collect `#[test]` fns (and the anonymous-block equivalents, named by a fixed
  convention e.g. `test_anon_<n>`); diff the sets.
- **Pass condition:** ported test count **== 3234** globally AND per-file
  `ported == contract` for every file AND the per-file name-set delta is empty.
  Exit 0 on equality; non-zero + a `MISSING:`/`EXTRA:` list otherwise.
- **Cross-checks (independent of the script):** `cargo test --workspace -- --list | grep -c ': test$'` must be ≥ 3234 (Rust may add glue tests, never fewer ported);
  `cargo test --workspace` exit 0 (the PASS/FAIL oracle).
- **Contract:** 3234 total (3113 string-named + 45 identifier-named + 76
  anonymous; terminal 2171, vt-c 330). A missing test or a dropped anonymous or
  identifier-named block is a **blocker**, full stop.

### G2 — Assertion-for-assertion [R1]

Inside each ported test, every reference assertion/error-case is reproduced; no
assertion is dropped, merged, or weakened.

- **Check:**
  `python3 .port/tools/gates/check_assertions.py --inventory .port/phase-0/inventory/inventory.json --rust-root $RUSTTY/crates`
  Per ported test: count Rust assertions (`assert!`, `assert_eq!`, `assert_ne!`,
  `expect(`, `.unwrap()` in test bodies used as assertions, `?`-propagated
  `Result` checks, `#[should_panic]`) and compare to the contract `assertions`
  count for the corresponding reference test.
- **Pass condition:** per-test `rust_assertions >= ref_assertions` AND
  global `sum(rust_assertions) >= sum(ref_assertions)`. (`>=` because idiomatic
  Rust sometimes needs an extra check to express one Zig `try`; never `<`.) A test
  with **fewer** assertions than its contract is a blocker; the script exits
  non-zero and lists `WEAKENED: <file>::<test> ref=<n> rust=<m>`.

### G3 — Zero comment-inventory delta (fidelity critic) [R1]

Every reference comment is preserved (retargeted to rustty names only where it
named a ghostty-specific identifier, per R2). Comments are never dropped or
summarized.

- **Check:**
  `python3 .port/tools/gates/fidelity_critic.py --inventory .port/phase-0/inventory/inventory.json --rust-root $RUSTTY/crates --identity-map .port/phase-0/plan/identity-rename.map`
  Per source file: recompute `comment_units` over the ported Rust file(s),
  normalize whitespace and apply the identity-rename map (ghostty→rustty token
  substitutions are *expected*, not deltas), then diff against the contract.
- **Pass condition:** per-file `comment_unit_delta == 0` for **every** file;
  global delta == 0. Exit 0 only at delta 0. The script lists
  `DROPPED:`/`ALTERED:` comments with file:line on any non-zero delta. Any
  non-zero delta is a blocker (R1: "Any delta is a blocker").

### G3b — Content-aware reference-comment coverage [R1]

G3 compares **aggregate** per-crate comment-line *counts*, so a genuinely
dropped reference comment can be masked by added PORT NOTEs (rust_total >=
ref_total even though a specific reference comment vanished). G3b closes that
hole **deterministically**: every *distinctive* reference comment (a
phrase/sentence — not a separator or ASCII diagram) must have a fuzzy match
somewhere in the ported crate's comments, after applying the ghostty→rustty
identity rename (R2). This is what `build-config` needed to surface the dropped
`config`/re-export block and `datastruct` to surface the dropped `deepEqual`
`.@"struct"` field-by-field fallback comment.

- **Check:**
  `python3 .port/tools/gates/check_comment_coverage.py --crate <name>`
  (bare, no `--crate`, runs per `PORT STATUS: done` crate). Distinctive =
  >= 5 word-tokens after normalization. Covered = normalized ref comment is a
  substring of the port comment blob, OR a port comment matches it with difflib
  ratio >= 0.60. A ref comment below the MISS threshold (0.45) is a real drop
  (RED); between MISS and 0.60 it is reported as REVIEW (soft, not RED).
- **Pass condition:** **0 MISSING** for every done crate; exit 0. The script
  lists `MISSING ref comment:` with the dropped text on any drop. Any MISSING is
  a blocker (R1). Run per done-crate; wired into `.port/tools/ci.sh` as a
  PROGRESS gate (a stub crate is green; a claimed-done crate that drops a comment
  goes RED, exactly R5).

### G4 — Differential VT clean (primary behavioral gate)

Identical input fed to the reference oracle (built with the `identity-ghostty`
feature OR replayed from frozen golden corpora) and to rustty produces
byte-identical canonical VT output (Screen/PageList dump, OSC/DCS responses,
formatter output, key/mouse/focus encodings).

- **Check:**
  `python3 .port/phase-0/harness/diff_vt.py --corpus .port/phase-0/corpora/cvt --rust-bin $RUSTTY/target/release/rustty-c-vt-runner --ref-golden .port/phase-0/corpora/cvt/golden`
  Drives each corpus input through both sides and diffs canonical output. Where a
  live oracle is needed it builds the **rustty** side with `--features identity-ghostty`
  so identity bytes (XTVERSION, DA1/DA2, TERM) match the reference with **no
  canonicalization hacks** (R2). The reference side is the pre-built `c-vt-*`
  binaries (`.port/phase-0/examples-out/*/zig-out/bin/*`) or frozen goldens.
- **Pass condition:** zero diffs across the entire corpus; exit 0. Any single
  byte diff (outside the documented identity-token positions, which the harness
  neutralizes by building rustty in `identity-ghostty` mode) is a blocker. Each
  diff is reported as `DIFF <case>: ref=<hex> rust=<hex> @offset`.
- **Corpus floor:** the 21 `c-vt-*` consumer programs enumerated in
  `build-summary.md` plus vttest/esctest conformance fixtures staged under
  `corpora/cvt/`.

### G5 — Differential fuzz clean

Continuous differential fuzzing (stood up early) finds **no** input on which
rustty's canonical VT output diverges from the reference, and no panic/abort/leak.

- **Check:**
  `cargo +nightly fuzz run vt_diff -- -max_total_time=$FUZZ_SECS -runs=$FUZZ_RUNS`
  (fuzz target lives in `crates/rustty-vt/fuzz/`; harness shared with G4). A
  reproducible nightly/CI invocation uses a fixed corpus + fixed seconds; the
  release gate uses the larger budget defined in CI config.
- **Pass condition:** exit 0, **zero** new entries in `fuzz/artifacts/` (no
  crash/divergence repro) for the configured budget. Any artifact (a divergence
  or crash repro) is a blocker; the artifact is the precise reproducing input.
  Regression corpus (`fuzz/corpus/vt_diff/`) must replay clean on every run.

### G6 — Memory safety (UB + leaks)

No memory leaks AND no undefined behavior in the test suite. This supersedes the
reference's `std.testing.allocator` leak check with a strictly stronger tool: Miri
catches leaks *and* UB (use-after-free, OOB, uninitialized reads, invalid
aliasing). **Right tool per job** (see PORT_RULES R6):

- **Pure-Rust crates → Miri (PRIMARY):**
  `cargo +nightly miri test -p <crate>` — catches UB + leaks. Applies to
  rustty-util, build-config, identity, datastruct, unicode, lib, global,
  render-types, cfg-fmt, args, terminfo, vt, vt-c, input, core, apprt-api, cli,
  termio, bench, synthetic (everything not in the FFI set).
- **Rust↔C FFI crates → AddressSanitizer:**
  `RUSTFLAGS="-Zsanitizer=address" cargo +nightly test -p <ffi-crate> --target <host>`
  for crates that link C (rustty-simd, font, renderer, config, crash, inspector).
  Miri cannot execute foreign C, so ASan covers UB across the boundary.
- **FFI leak detection → Linux CI:** LSan/valgrind do not run on arm64 macOS, so
  leak checks for the FFI crates are performed on the Linux CI box (LSan/valgrind)
  — documented, not skipped.
- **Runner:** `.port/tools/gates/check_leaks.sh --crate <name>` picks the right
  tool automatically; `--all-done` runs it for every `PORT STATUS: done` crate;
  `--self-test` proves Miri catches a leak and a UB.
- **Pass condition:** all selected runs exit 0 with zero leak / zero UB / zero
  ASan reports. Any leak or UB is a blocker.
- **Status:** Miri **installed and validated** (`miri 0.1.0`); the tracking-allocator
  approach has been **removed** (Miri is the correct, stronger tool). Miri runs are
  slower (interpreted), so they gate per-crate completion + CI, while fast native
  `cargo test` gates assertion parity on every iteration.

### G7 — Crate graph acyclic (`cycle_check.py` exit 0)

The Cargo workspace graph contains no unexpected cross-crate cycle.

- **Check:**
  `python3 .port/tools/cycle_check.py` and `python3 .port/tools/reach_scc.py`.
- **Pass condition (two parts):**
  1. **Cargo is authoritative once crates exist:** `cargo metadata --format-version 1`
     resolves with no cyclic *normal* dependency (Cargo refuses to build a normal-dep
     cycle; a clean `cargo build --workspace` IS the hard acyclicity proof). Dev-dep
     cycles are permitted (test-only edges) and are excluded by design.
  2. **Structural pre-check:** `cycle_check.py`'s `cross-crate SCCs (size>1)` line
     must read **`0`** in the "RUNTIME ONLY (excl. test-block imports)" section, and
     `reach_scc.py`'s vt leak-check must print **no `LEAK:` lines** (the vt crown
     jewel must not import font/renderer/config/apprt/inspector/termio/cli/crash/global).
- **Exit-code caveat (precise, do not misread):** as written today
  `cycle_check.py` **always exits 0** — it *reports* SCCs but does not fail on
  them; running it against the current reference mapping prints **24 runtime
  mutual pairs** (e.g. `core <-> vt`, `font <-> renderer`) because it is mapping the
  *un-broken* reference, pre-R4-breaks. The gate's pass condition is therefore the
  **`cross-crate SCCs (size>1): 0`** line, NOT the process exit code. Phase-1 MUST
  add an `--fail-on-scc` flag (exit non-zero when the runtime-only SCC count > 0)
  so the literal "exit 0" wording of the task becomes load-bearing; until that
  flag exists, CI asserts on the reported count via `grep`. Per-PR, the runtime
  SCC count must be **monotonically non-increasing** as R4 breaks land (it goes
  from the 167-file upper-half SCC down to 0); it must never increase.

### G8 — Perf / memory / size / startup not worse than BASELINE.json

No regression on the four non-functional axes vs the frozen Phase-0 baseline.

- **Check:**
  `python3 .port/tools/gates/check_baseline.py --baseline .port/phase-0/baseline/BASELINE.json --measure now`
  Re-runs the same measurements the baseline agent captured and compares.
  - **perf:** `ghostty-bench`/`rustty-bench` actions (`codepoint-width`,
    `grapheme-break`, `screen-clone`, `terminal-parser`, `terminal-stream`,
    `is-symbol`, `osc-parser`) — throughput / ns-per-op.
  - **memory:** peak RSS over a fixed corpus replay (OS-level `/usr/bin/time -l`
    max RSS on macOS) + optional heap high-water via a profiling allocator
    (dhat/jemalloc-stats) when a finer breakdown is needed. (Distinct from the G6
    correctness gate, which is Miri/ASan, not a perf measurement.)
  - **size:** `librustty-vt.{a,dylib}` and `rustty` binary byte sizes vs the
    reference `libghostty-vt.a` (12,065,920 B) / `.0.1.0.dylib` (5,742,200 B)
    captured in `build-summary.md`.
  - **startup:** cold-start wall time of `rustty --version`-class entry.
- **Pass condition:** for every metric, `rustty <= baseline * (1 + tol)` where
  `tol` is the per-metric tolerance recorded in `BASELINE.json` (default 0% for
  size, a small documented noise band for time/RSS). Exit 0 only if no metric
  regresses beyond tolerance. **Parity is the invariant; performance is the
  gradient (Goal 2 only after Goal 1 is locked, R5)** — so during Goal 1 this
  gate is a *no-regression* fence, not an optimization target. A regression is
  reported as `REGRESS <metric>: baseline=<x> now=<y> (+z%)`.

### G9 — Full surface coverage (config / keybind / action / terminfo)

Every config option, keybind trigger+action, apprt action, and terminfo
capability present in the reference is covered (ported + parity-tested) in rustty.

- **Check:**
  `python3 .port/tools/gates/check_surface.py --surface .port/phase-0/inventory/parity-surface.json --rust-root $RUSTTY/crates --terminfo $RUSTTY/target/.../rustty.terminfo`
  Cross-references each id in `parity-surface.json` against: the rustty config
  struct fields, the keybind Action enum + parser, the apprt Action union, and
  the compiled terminfo (`infocmp rustty` vs the reference cap set with names
  retargeted to `rustty` per R2).
- **Pass condition:** every surface id is `present AND has ≥1 parity test`; the
  uncovered set is empty; exit 0. Missing option/keybind/action/cap → blocker,
  listed as `UNCOVERED <kind>: <id>`.
- **Reference magnitudes (illustrative, from probe; authoritative counts come
  from `parity-surface.json`):** config options on the order of ~180–230 fields
  in `config/Config.zig`; ~48 keybind Action variants in `input/Binding.zig`;
  apprt actions in `apprt/action.zig`; ~270 terminfo entries in
  `terminfo/ghostty.zig`. These are sanity magnitudes, NOT the contract — the
  gate keys on `parity-surface.json` to avoid fabricated numbers.

---

## 1b. METHODOLOGY — reference tests that compare against a LIVE external oracle (R3)

Some reference `test{}` blocks compare a generated artifact against a **live
external oracle** that R3 forbids as an in-crate Cargo dependency. The canonical
example is `uucode`:

```zig
// src/unicode/props_uucode.zig
test "unicode props: tables match uucode" { ... table.get(cp) vs uucode.get(.width, cp) ... }
// src/unicode/symbols_uucode.zig
test "unicode symbols: tables match uucode" { ... table.get(cp) vs uucode.get(.is_symbol, cp) ... }
```

The reference's *independence* is the whole point: the embedded table is proven
equal to a **second, externally-sourced** computation over the full range. A
naive in-crate port has no second source (R3: no `uucode` dep), so the only
available "oracle" becomes the table itself — a **tautology** (`table == table`)
that verifies nothing. That is a silent weakening (R5) even though the test still
"passes".

**Rule (binding).** When a reference test compares against a live external oracle
unavailable in-crate under R3, it is reproduced as a **harness DIFFERENTIAL gate
(oracle #2)**: the external oracle is built in a separate process (it MAY depend
on the forbidden library, because the harness is tooling, not a rustty crate),
dumps its values over the full reference range, and a diff asserts **0 diffs**
against the rustty artifact dumped over the same range. This differential gate is
the **faithful, full-coverage reproduction** of the reference test.

A **genuine in-crate spot-check** is ALSO kept (it must assert the
function-under-test against *independently-sourced* ground truth — the standard /
known values — NEVER read from the rustty artifact itself), so the test-count
contract (G1) is preserved and a fast `cargo test` still exercises the path.

**This is NOT a weakening (R5):** total coverage = exhaustive harness differential
(every point) **+** independent in-crate spot-check **≥** the reference's single
full-range oracle test. A tautological in-crate "oracle" is *less* than the
reference; this decomposition is *more*.

**Implemented instance — rustty-unicode (`G4-uni`):**
`.port/phase-0/harness/unicode/` builds `uucode-oracle` (uucode 0.2.0 +
ghostty's `src/build/uucode_config.zig`, copied verbatim/pinned) and dumps
`width`, `is_symbol`, all `Properties` fields, `grapheme_break_no_control`, and
the 8192-entry grapheme precompute for every cp `0..=0x10FFFF`;
`cargo run --example dump_tables -p rustty-unicode` dumps rustty's embedded
tables over the same range; `diff_unicode.py` asserts 0 diffs (result: **0 diffs**
across all fields). Wired into `.port/tools/ci.sh` as the HARD gate `G4-uni`. The
in-crate `props_uucode.rs` / `symbols_uucode.rs` `*_tables_match_uucode` tests are
the curated spot-checks (ASCII/CJK/combining/ZWJ/emoji/fullwidth/control, the
prepend & emoji-modifier width special-cases, Hangul & Indic break classes, the
symbol block set + non-symbols, and the `cp > MAX_CODE_POINT` out-of-range rule),
each value hand-verified against the Unicode standard / `uucode_config.zig` rule.
See `.port/phase-0/harness/unicode/README.md`.

---

## 2. PER-CRATE gate template

Applied to **every** `rustty-*` crate as it is carved (leaves first, per the
topological order in the design spec §5). A crate is "parity-complete" only when
**all** of the following are green for that crate. `<crate>` = the crate name;
`<files>` = its source-file set from `PORT_PLAN.md`'s file→crate map.

| Gate id | What it checks | Command (scoped to the crate) | Pass condition |
|---|---|---|---|
| **P-T** (tests ported) | every `test{}` in `<files>` → a Rust test, name-for-name | `check_tests_ported.py --crate <crate>` + `cargo test -p rustty-<crate> -- --list` | per-file ported == contract; count == sum of `test_blocks` over `<files>`; name-set delta empty |
| **P-A** (assertions) | assertion-for-assertion within each ported test | `check_assertions.py --crate <crate>` | per-test `rust >= ref`; none weakened |
| **P-C** (comments) | zero comment delta over `<files>` | `fidelity_critic.py --crate <crate>` | per-file delta == 0 |
| **P-PASS** (green) | the ported tests actually pass + memory-safe | `cargo test -p rustty-<crate>` (fast, correctness) + `check_leaks.sh --crate rustty-<crate>` (Miri pure-Rust / ASan FFI) | exit 0; zero leaks/UB |
| **P-DIFF** (differential) | crate's behavior matches oracle on its corpus slice | crate-scoped harness run (vt/config/input/font/render/terminfo slices) | zero diffs |
| **P-FUZZ** (where input-driven) | differential fuzz target for the crate clean | `cargo fuzz run <crate>_diff` | zero artifacts (only for vt, config-parse, input-encode, osc, etc.) |
| **P-DAG** (acyclic) | the crate adds no new cross-crate runtime cycle | `cycle_check.py` after the crate lands; `cargo build -p rustty-<crate>` | runtime SCC count not increased; Cargo resolves |
| **P-SURF** (surface) | surface ids owned by the crate are covered | `check_surface.py --crate <crate>` | crate's uncovered set empty |
| **P-SIZE/PERF** (no regress) | crate-attributable size/perf vs baseline | `check_baseline.py --crate <crate>` | within tolerance |

**Per-crate gate provenance.** Which gates are *binding* per crate depends on the
crate's surface (recorded in `PORT_PLAN.md`):

- `rustty-vt` / `rustty-vt-c` (crown jewel): ALL gates binding; P-DIFF + P-FUZZ are
  the headline. vt-c additionally: the C ABI exports `rustty_*` symbols and emits
  `include/rustty.h` + `rustty/vt/*` (R2) — verified by `nm`/header-presence checks
  (see §4), NOT by ABI-equivalence to `ghostty.h`.
- Leaf crates with no I/O surface (`rustty-util`, `-datastruct`, `-unicode`,
  `-simd`, `-lib`, `-global`, `-build-config`, `-identity`, `-render-types`):
  P-T/P-A/P-C/P-PASS/P-DAG binding; P-DIFF/P-FUZZ/P-SURF N/A (documented as such).
- `rustty-config`/`-input`/`-terminfo`: P-SURF binding (they own surface ids).
- `rustty-renderer`/`-font`: P-DIFF uses perceptual/raster diff (documented
  tolerance) rather than byte-exact, recorded in the crate's gate manifest.

A crate's "N/A" for a gate must be **explicitly recorded** in `PORT_PLAN.md` with a
reason; an unrecorded skip is treated as BLOCKED, never as green.

---

## 3. Identity-rename caveat (R2): dropped gates and their replacements

R2 renames all *published* artifacts to `rustty` (clean standalone, not a ghostty
drop-in). This makes three **identity-EQUIVALENCE** gates from the Parity
Constitution impossible-by-design; they are **dropped** and each is replaced by an
equivalent *behavioral/structural* gate. All non-identity behavioral gates remain
fully in force.

| Dropped constitution gate | Why it cannot hold under R2 | Replacement gate (binding) | Check |
|---|---|---|---|
| **ghostty.h ABI byte-match** (rustty header == `ghostty.h`) | header is `rustty.h`, symbols are `rustty_*`, tree is `rustty/vt/*` | **R2-ABI-shape:** rustty exports the *same set of functions with the same signatures*, renamed `ghostty_*`→`rustty_*`; the cbindgen-emitted `include/rustty.h` + `rustty/vt/*` tree exists and is symbol-complete vs the reference header set | `python3 .port/tools/gates/check_abi_shape.py --ref-headers $REF/zig-out/include/ghostty --rust-headers $RUSTTY/target/include/rustty --rename ghostty_=rustty_` → every ref symbol has a renamed counterpart; `nm -gU librustty-vt.dylib` exposes the full renamed set; **zero** residual `ghostty_*` symbols. Behavioral equivalence of each exported fn is proven by **G4 differential** with rustty built `--features identity-ghostty`. |
| **Swift-app-links-unchanged** (reference macOS app links our lib unmodified) | rustty ships its own identity; it is NOT a drop-in for the ghostty Swift app | **R2-xcframework-consumable:** `rustty-vt.xcframework` builds with all reference slices (`macos-arm64_x86_64`, `ios-arm64`, `ios-arm64-simulator`) and a sample Swift consumer linking the **rustty** framework compiles+links+runs | `xtask xcframework` produces the framework; `swiftc` test consumer links `rustty-vt.xcframework` and runs a smoke test → exit 0. (Equivalence to ghostty's own app is explicitly NOT required.) |
| **terminfo == ghostty** (entry name/caps byte-equal to `ghostty` entry) | entry is named `rustty`; `TERM=xterm-rustty` (R2) | **R2-terminfo-cap-parity:** the rustty terminfo entry has the **same capability set/values** as the reference, with only the entry name + identity strings renamed `ghostty`→`rustty` | `infocmp -1 rustty` vs `infocmp -1 ghostty` (from `$REF`), diffed after applying the `ghostty`→`rustty` rename to names only → **zero** capability-value deltas. This is part of G9/P-SURF. |

**What is retained unchanged:** the dual-profile `identity-ghostty` build feature
makes the *behavioral* gates (G4 differential, G5 fuzz, conformance) compare
byte-for-byte against the reference with **no canonicalization hacks** — when the
parity harness builds rustty with `identity-ghostty`, every identity byte
(XTVERSION, Primary/Secondary DA, `TERM`/`TERM_PROGRAM`, version strings, config
dir) flips to the reference's value, so a byte diff in G4 is always a *real*
behavioral divergence, never an identity artifact. Default (published) builds carry
the rustty identity; a dedicated **identity-default test** asserts the default build
reports `rustty`/`xterm-rustty`/`TERM_PROGRAM=rustty`/`~/.config/rustty` (i.e.
the rename actually took effect), so R2 is itself gated.

---

## 4. Auxiliary structural checks (fold into the relevant gates)

- **R2 symbol hygiene:** `nm -gU` on all rustty libs → zero `ghostty_*` /
  `_ghostty_*` symbols; zero literal `ghostty`/`Ghostty` in published headers,
  `--help`, man pages, or default config-dir resolution (grep gate). Part of G9.
- **Comptime artifacts bit-for-bit (R1):** generated parse tables, unicode tables,
  terminfo blob reproduced byte-identically vs the reference-generated artifact
  (`sha256` compare of the generated table files). Part of G3/G9.
- **No ghostty build/run dependency (R3):** `cargo metadata` shows no path/registry
  dep resolving under `/Users/gael/projects/ghostty`; a build with the ghostty tree
  moved away still succeeds. Standalone gate `check_no_ghostty_dep.py`.

---

## 5. Gate ledger (summary, machine-checkable)

| Gate | Reads | Command (canonical) | GREEN when |
|---|---|---|---|
| G1 tests ported | inventory.json | `check_tests_ported.py` + `cargo test --workspace` | count==3234, per-file ==contract, names match, exit 0 |
| G2 assertions | inventory.json | `check_assertions.py` | per-test rust>=ref, none weakened |
| G3 comments | inventory.json + identity map | `fidelity_critic.py` | per-file delta==0 |
| G3b comment coverage | ref `.zig` comments + identity map | `check_comment_coverage.py` | 0 MISSING (every distinctive ref comment matched) |
| G4 differential VT | corpora/cvt + harness | `diff_vt.py` (rustty `--features identity-ghostty`) | zero diffs |
| G5 differential fuzz | fuzz corpus | `cargo fuzz run vt_diff` | zero new artifacts |
| G6 memory-safe | — | `check_leaks.sh --all-done` (Miri pure-Rust / ASan FFI) | zero leaks/UB |
| G7 acyclic | tools/cycle_check.py | `cycle_check.py` (SCC line) + `cargo build` | runtime SCCs==0; Cargo resolves |
| G8 no-regress | BASELINE.json | `check_baseline.py` | every metric <= baseline*(1+tol) |
| G9 surface | parity-surface.json | `check_surface.py` + `infocmp` | uncovered set empty |
| Per-crate | PORT_PLAN map + above | P-T/P-A/P-C/P-PASS/P-DIFF/P-FUZZ/P-DAG/P-SURF/P-SIZE | all binding sub-gates green |
| R2-ABI-shape | ref+rust headers | `check_abi_shape.py` | full renamed symbol set, zero residual ghostty_* |
| R2-xcframework | xcframework | `xtask xcframework` + swift smoke | builds+links+runs |
| R2-terminfo | infocmp | `infocmp rustty` vs `ghostty` | zero cap-value delta after name rename |
| identity-default | default build | identity-default test | reports rustty/xterm-rustty |

**Invariant (R5):** no gate may be weakened, skipped, xfail'd, or its threshold
loosened to pass. A red gate that cannot be made honestly green → STOP and surface
a precise blocker (which gate, which file:line/case, ref vs rust value). Pending
contracts (`inventory.json`, `parity-surface.json`, `BASELINE.json`, harness) and
missing tooling (`cargo miri`) are **BLOCKED**, never GREEN-by-default.
