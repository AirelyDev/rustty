# rustty parity gates (`.port/tools/gates/`)

Executable implementations of the **G1–G9** parity gates from
`.port/phase-0/plan/DEFINITION_OF_PARITY.md` (DoP), governed by
`.port/PORT_RULES.md` (R1 fidelity, R2 identity-rename, R3 self-contained,
R4 acyclic crate graph, R5 gates-invariant).

**Contract:** each gate is a runnable script — `exit 0 = GREEN`, `non-zero +
diagnostic = RED` (or `exit 3 = BLOCKED` when a required contract artifact /
tool is absent — never a silent green). Every gate runs on the current
(mostly-empty / stub) workspace.

## Two modes (R5: never weaken a gate)

- **progress mode** (default for the content gates): counts what is ported so far
  vs the frozen Phase-0 contract and reports `ported/contract`. On the
  empty/stub workspace this reports `0/<contract>` **without erroring** (exit 0).
  It goes **RED** only when a *claimed-done* crate is short/weakened/drops
  fidelity. A crate is **claimed-done** when its `lib.rs` (or `main.rs`) no
  longer says `//! PORT STATUS: stub` (e.g. it says `done`/`complete`/`parity`).
- **`--strict` mode**: the full DoP gate. RED unless the global contract is met
  (count == contract / assertions >= floor / comments >= floor / surface fully
  covered). Used for the final/locked workspace. The content gates (G1/G2/G3/G9)
  all flip **RED in `--strict` on the empty workspace** and only GREEN at real
  parity — proving they are not stuck-green.

The "claimed-done" signal is what makes the gates *flippable today*: marking any
crate `done` while its ported tests/comments/assertions are missing turns the
relevant gate RED on the real tree (demonstrated in self-tests and verifiable by
flipping a single `PORT STATUS:` line).

## Gate map

| Script | DoP id | Reads (contract) | GREEN / pass condition | Self-test |
|---|---|---|---|---|
| `check_tests_ported.py` | **G1** 100% test blocks ported [R1] | `inventory/tests-summary.json` (`crate_map` = PORT_PLAN file→crate map + per-file `test_blocks`; sums to 3234), `inventory/tests.jsonl` (per-test names) | progress: `0/3225` workspace (extra/xtask = 9 non-DAG) without erroring; RED if a claimed-done crate has `ported<contract` or a `MISSING` name. `--strict`: global ported == workspace contract AND contract+non-DAG == 3234 global floor AND every per-crate count == contract; name-set delta empty. | `--self-test` (synthetic crate: stub-0 GREEN, done-0 RED, done-N GREEN, strict GREEN) |
| `check_assertions.py` | **G2** assertion-for-assertion [R1] | `tests-summary.json` (11975 floor, per-file counts), `tests.jsonl` (per-test assertion counts + names) | per-test `rust >= ref`; global `sum(rust) >= sum(ref)`. RED if a claimed-done crate `WEAKENED`s a test (rust<ref) or lowers a crate total. `--strict`: global rust >= 11975 floor AND none weakened. | `--self-test` (stub-0 GREEN, done-2-vs-3 RED `WEAKENED`, done-3 GREEN, done-4 GREEN) |
| `check_comments_fidelity.py` | **G3** zero comment-inventory delta (fidelity critic) [R1] | `inventory/comments-census.json` (per-file comment-line census by class), `tests-summary.json` (file→crate), `plan/identity-rename.map` (optional ghostty→rustty token map) | per-crate `rust_comment_lines - ref >= 0` (no DROP) for every claimed-done crate. RED `DROPPED:` on any drop for a done crate. `--strict`: also global `rust >= ref` floor (flips RED on empty). | `--self-test` (stub-under GREEN, done-under RED `DROPPED`, done-over GREEN) |
| `run_differential.sh` | **G4** differential VT clean | `phase-0/harness/` (comparator + ref driver), `phase-0/corpora/cvt/*.golden` (21 c-vt cases) | real gate (when `target/release/rustty-c-vt-runner` exists): zero canonical-output diffs across the corpus (rustty built `--features identity-ghostty`); any `DIFF` ⇒ RED. progress (port empty): runs the harness self-test `gate_empty_divergence.sh` (proves the comparator detects total divergence) ⇒ `HARNESS-WIRED` (exit 0, distinct from a parity green). BLOCKED if harness/corpus absent. | `--self-test` (asserts comparator runnable + 21 golden cases + flip-proof gate present) |
| `run_fuzz_smoke.sh` | **G5** differential fuzz clean | `crates/rustty-vt/fuzz/` (cargo-fuzz `vt_diff`), `harness/FUZZ-INVENTORY.md`, `rustty-synthetic` (corpus gen) | real smoke (when `cargo fuzz` + target exist): zero new `fuzz/artifacts/`, exit 0 ⇒ GREEN; new artifact/non-zero ⇒ RED. progress (port empty): verifies fuzz inventory + corpus wiring ⇒ `FUZZ-WIRED` (exit 0). Budgets via `FUZZ_SECS`/`FUZZ_RUNS`. | `--self-test` (asserts FUZZ-INVENTORY + synthetic crate + cargo present) |
| `check_leaks.sh` | **G6** leak-free | — (drives `cargo test --features leak-track`, `cargo miri`, ASan) | real (`--run`/`--run-miri`): all sub-gates exit 0 with zero leak/UB/ASan ⇒ GREEN; any leak ⇒ RED. dry/progress (default, no cargo): reports wiring and surfaces documented BLOCKERs loudly — `cargo miri` not installed ⇒ **BLOCKED, not waived** (DoP §G6); `leak-track` feature not declared ⇒ unrunnable. exit 0 = `WIRED`; `--strict` dry ⇒ BLOCKED (exit 3) if a sub-gate is unrunnable. | `--self-test` (wiring discovery: cargo / leak-track feature / miri presence) |
| `check_acyclic.sh` (+ `cargo_cycle_check.py`) | **G7** crate graph acyclic [R4] | the **actual** `crates/*/Cargo.toml` `[dependencies]` / `[build-dependencies]` (dev-deps excluded; `--include-dev` to add) | parses the real rustty Cargo graph, runs Tarjan SCC; **0 cross-crate SCCs ⇒ GREEN** (must be GREEN on the empty/stub workspace — it is: 29 crates, 93 edges, 0 cycles). Any cross-crate cycle ⇒ RED with the `CYCLE:` members + edges. `--with-reference` additionally prints `.port/tools/cycle_check.py`'s runtime SCC line (informational: models the *un-broken* reference, per the DoP exit-code caveat). | `--self-test` (synthetic 2-crate tree: acyclic GREEN, then injects a cycle ⇒ RED) |
| `check_baseline_regression.py` | **G8** no perf/mem/size/startup regression [R5] | `phase-0/baseline/BASELINE.json` (frozen metrics) + a candidate `--metrics PATH` (same shape) | every shared metric within tolerance: size 0% (growth ⇒ regress), throughput −7%, time/RSS +7%, startup +10% (overridable). RED `REGRESS <metric>: baseline=… now=… (+z%)`. progress (no `--metrics`): prints the baseline it *would* compare and exits 0 (nothing to regress) — never fabricates a green. `--strict`: also RED if candidate drops a baseline metric. | `--self-test` (synthetic baseline+candidate: no-metrics GREEN, equal GREEN, −50% throughput RED, +1B size RED, −2% within-band GREEN) |
| `check_parity_surface.py` | **G9** full surface coverage (config/keybind/action/terminfo) [R2] | `inventory/parity-surface.json` (config options, keybind/cli actions, terminfo caps, modes/sgr/osc — each a stable id) | every surface id present in its owning crate's ported Rust AND (`--require-tests`/`--strict`) referenced by a ported test; uncovered set empty ⇒ GREEN. RED `UNCOVERED <kind>: <id>` when a claimed-done owner crate has an uncovered id (progress) or any id uncovered (`--strict`). terminfo also diffs `infocmp rustty` cap set vs surface (R2-terminfo-cap-parity) when the entry is installed. | `--self-test` (synthetic config surface: stub GREEN, done-1/2 RED `UNCOVERED`, done-2/2 GREEN, strict-stub RED) |

Helpers (not gates themselves):
- `_gatelib.py` — shared library: contract loaders, the file→crate map (from
  `tests-summary.json`'s `crate_map`), crate-src discovery, `PORT STATUS`
  done-detection, and string/comment-aware Rust scanners for `#[test]` fns,
  in-test assertions (`assert*!`, `.expect`/`.unwrap`, `?`-propagation,
  `#[should_panic]`), and comment-line census by class.
- `cargo_cycle_check.py` — the real-rustty-Cargo-graph SCC checker that backs G7
  (dependency-free TOML scan + Tarjan; `--fail-on-scc` default on).

## Per-crate gates (P-T / P-A / P-C / P-SURF)

The content gates accept `--crate <name>` to scope to a single crate (the DoP
per-crate template). Example:

```
python3 check_tests_ported.py   --crate vt        # P-T for rustty-vt
python3 check_assertions.py     --crate vt        # P-A
python3 check_comments_fidelity.py --crate vt     # P-C
python3 check_parity_surface.py --kind config     # P-SURF (config slice)
```

## Running everything

```
cd .port/tools/gates
for g in check_tests_ported check_assertions check_comments_fidelity \
         check_baseline_regression check_parity_surface; do
  python3 $g.py            # progress mode
done
bash check_acyclic.sh                              # G7 (binding: real Cargo graph)
bash run_differential.sh ; bash run_fuzz_smoke.sh ; bash check_leaks.sh
```

Every gate also accepts `--self-test` (Python) / `--self-test` (shell), which
runs a synthetic flip-proof in a temp dir and prints `SELF-TEST: PASS|FAIL`.

## Contract artifacts consumed (frozen Phase-0, immutable — R5)

| Artifact | Gates |
|---|---|
| `phase-0/inventory/tests-summary.json` | G1, G2, G3, G9 (file→crate map) |
| `phase-0/inventory/tests.jsonl` | G1 (names), G2 (per-test assertions) |
| `phase-0/inventory/comments-census.json` | G3 |
| `phase-0/inventory/parity-surface.json` | G9 |
| `phase-0/baseline/BASELINE.json` | G8 |
| `phase-0/harness/` + `phase-0/corpora/cvt/` | G4 |
| `crates/rustty-vt/fuzz/` + `harness/FUZZ-INVENTORY.md` | G5 |
| `crates/*/Cargo.toml` | G7 |
| `plan/identity-rename.map` (optional) | G3 (token-rename ALTERED detection) |

**Note (contract counts):** the global floor is **3234** test blocks /
**11975** assertions / **42069** comment lines / **677** surface ids. Of the
3234 tests, **9** live in non-DAG assets (`extra` 2, `xtask` 7) outside
`crates/`; the in-`crates/` workspace contract is therefore **3225**. G1 prints
this decomposition explicitly so 3225-vs-3234 is never read as a discrepancy.

**Invariant (R5):** no gate may be weakened, skipped, xfail'd, or its threshold
loosened to pass. A red gate that cannot be made honestly green ⇒ STOP and
surface a precise blocker. `cargo miri` is not installed on this machine ⇒ the
G6 Miri sub-gate is BLOCKED (loudly), never waived.
