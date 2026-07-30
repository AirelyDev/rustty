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
  fidelity. A crate is **claimed-done** when its `lib.rs` (or `main.rs`)
  `PORT STATUS:` is one of `done`/`complete`/`parity`/`ported`/`final`
  (`_gatelib.DONE_STATUSES` — the ONE definition every gate shares: G1/G2/G3b
  and G6/check_leaks.sh all bind on it, H5 fix).
- **`--strict` mode**: the full DoP gate. RED unless the global contract is met
  (G1: ported >= contract AND empty MISSING name-set, per DG3(a) — EXTRA is
  reported, never fatal / assertions >= floor / comments >= floor / surface
  fully covered). Used for the final/locked workspace. The content gates
  (G1/G2/G3/G9) all flip **RED in `--strict` on the empty workspace** and only
  GREEN at real parity — proving they are not stuck-green.

The "claimed-done" signal is what makes the gates *flippable today*: marking any
crate `done` while its ported tests/comments/assertions are missing turns the
relevant gate RED on the real tree (demonstrated in self-tests and verifiable by
flipping a single `PORT STATUS:` line).

## Gate map

| Script | DoP id | Reads (contract) | GREEN / pass condition | Self-test |
|---|---|---|---|---|
| `check_tests_ported.py` | **G1** 100% test blocks ported [R1] | `inventory/tests-summary.json` (`crate_map` = PORT_PLAN file→crate map + per-file `test_blocks`; sums to 3234), `inventory/tests.jsonl` (per-test names) | progress: `0/3227` workspace (xtask = 7 non-DAG) without erroring; RED if a claimed-done crate has `ported<contract` or a `MISSING` name. `--strict` (DG3(a)): global ported >= workspace contract AND contract+non-DAG == 3234 global floor AND every per-crate `ported >= contract` AND MISSING name-set empty (EXTRA reported, not fatal). | `--self-test` (synthetic crate: stub-0 GREEN, done-0 RED, done-N GREEN, strict-extra GREEN, strict-short RED) |
| `check_assertions.py` | **G2** assertion-for-assertion [R1] | `tests-summary.json` (11975 floor, per-file counts), `tests.jsonl` (per-test assertion counts + names) | per-test `rust >= ref`; global `sum(rust) >= sum(ref)`. RED if a claimed-done crate `WEAKENED`s a test (rust<ref) or lowers a crate total. `--strict`: global rust >= 11975 floor AND none weakened. | `--self-test` (stub-0 GREEN, done-2-vs-3 RED `WEAKENED`, done-3 GREEN, done-4 GREEN) |
| `check_comments_fidelity.py` | **G3** zero comment-inventory delta (fidelity critic) [R1] | `inventory/comments-census.json` (per-file comment-line census by class), `tests-summary.json` (file→crate), `plan/identity-rename.map` (optional ghostty→rustty token map) | per-crate `rust_comment_lines - ref >= 0` (no DROP) for every claimed-done crate. RED `DROPPED:` on any drop for a done crate. `--strict`: also global `rust >= ref` floor (flips RED on empty). | `--self-test` (stub-under GREEN, done-under RED `DROPPED`, done-over GREEN) |
| `run_differential.sh` | **G4** differential VT clean | `phase-0/harness/` (comparator + `bin/vt_driver_ref` + `bin/vt_driver_port` and `bin/cvt/*` from `build_port_driver.sh`), `phase-0/corpora/cvt/*.golden` (21 c-vt cases) | ARMS itself the moment `harness/bin/vt_driver_port` (or `$RUSTTY_VT_DRIVER`) exists. Armed: **leg A** — every `harness/fixtures/*.vt` through `comparator.py <input> --ref vt_driver_ref --port vt_driver_port` (byte-exact canonical output); **leg B** — all 21 port-built c-vt example binaries (`harness/bin/cvt/<case>`) vs the stored goldens (combined stdout+stderr byte-diff + exit-code match; a missing binary is RED `unbuilt`, never a skip). ANY diff ⇒ RED (exit 1), never a warning; each failure writes `harness/results/diag/<case>.diag` (class crash/hang/diverge/unbuilt + first diverging byte w/ hex context). progress (driver absent or `--progress`): runs `gate_empty_divergence.sh` pinned to an empty stub (proves the comparator detects 100% divergence) ⇒ `HARNESS-WIRED` (exit 0, distinct from a parity green). BLOCKED if harness/corpus absent. | `--self-test` (injection flip proof: ref-vs-ref GREEN control; 1 corrupted byte ⇒ RED exit 1; restore ⇒ GREEN; golden-leg corrupt detected + restore clean) |
| `run_fuzz_smoke.sh` | **G5** differential fuzz clean | `crates/rustty-vt/fuzz/` (cargo-fuzz `vt_diff`), `harness/FUZZ-INVENTORY.md`, `rustty-synthetic` (corpus gen) | real smoke (when `cargo fuzz` + target exist): zero new `fuzz/artifacts/`, exit 0 ⇒ GREEN; new artifact/non-zero ⇒ RED. progress (port empty): verifies fuzz inventory + corpus wiring ⇒ `FUZZ-WIRED` (exit 0). Budgets via `FUZZ_SECS`/`FUZZ_RUNS`. | `--self-test` (asserts FUZZ-INVENTORY + synthetic crate + cargo present) |
| `check_leaks.sh` | **G6** memory-safe (UB + leaks) | `crates/*/Cargo.toml` + `crates-sys/*/Cargo.toml` (enumeration) + `_gatelib` done-detection (statuses done\|complete\|parity\|ported\|final, shared with G1/G2/G3b) | default / `--all-done`: for every claimed-done crate run the right tool per R6 — Miri (pure Rust: UB + leaks) or ASan (FFI crates + all `*-sys`: Miri can't run C); live-syscall test binaries (e.g. rustty-os `--test cf_release_live`, real kqueue) are ROUTED Miri→ASan per `MIRI_EXCEPTIONS.md` §"Live-syscall test binaries", never skipped. All clean ⇒ GREEN (exit 0); any UB/leak ⇒ RED. `--crate <name>` scopes to one crate; `--list` prints crate/status/gated/lane without running. FFI *leak* checks deferred to Linux CI (LSan/valgrind absent on arm64 macOS, DoP §G6). | `--self-test` (Miri flip proof: an injected leak and an injected OOB-read UB must both be DETECTED) |
| `check_acyclic.sh` (+ `cargo_cycle_check.py`) | **G7** crate graph acyclic [R4] | the **actual** `crates/*/Cargo.toml` `[dependencies]` / `[build-dependencies]` (dev-deps excluded; `--include-dev` to add) | parses the real rustty Cargo graph, runs Tarjan SCC; **0 cross-crate SCCs ⇒ GREEN** (must be GREEN on the empty/stub workspace — it is: 40 crates, 181 edges, 0 cycles). Any cross-crate cycle ⇒ RED with the `CYCLE:` members + edges. `--with-reference` additionally prints `.port/tools/cycle_check.py`'s runtime SCC line (informational: models the *un-broken* reference, per the DoP exit-code caveat). | `--self-test` (synthetic 2-crate tree: acyclic GREEN, then injects a cycle ⇒ RED) |
| `check_baseline_regression.py` | **G8** no perf/mem/size/startup regression [R5] | `phase-0/baseline/BASELINE.json` (frozen metrics) + a candidate `--metrics PATH` (same shape) | every shared metric within tolerance: size 0% (growth ⇒ regress), throughput −7%, time/RSS +7%, startup +10% (overridable). RED `REGRESS <metric>: baseline=… now=… (+z%)`. progress (no `--metrics`): prints the baseline it *would* compare and exits 0 (nothing to regress) — never fabricates a green. `--strict`: also RED if candidate drops a baseline metric. | `--self-test` (synthetic baseline+candidate: no-metrics GREEN, equal GREEN, −50% throughput RED, +1B size RED, −2% within-band GREEN) |
| `check_parity_surface.py` | **G9** full surface coverage (config/keybind/action/terminfo) [R2] | `inventory/parity-surface.json` (config options, keybind/cli actions, terminfo caps, modes/sgr/osc — each a stable id) | every surface id present in its owning crate's ported Rust AND (`--require-tests`/`--strict`) referenced by a ported test; uncovered set empty ⇒ GREEN. RED `UNCOVERED <kind>: <id>` when a claimed-done owner crate has an uncovered id (progress) or any id uncovered (`--strict`). terminfo also diffs `infocmp rustty` cap set vs surface (R2-terminfo-cap-parity) when the entry is installed. | `--self-test` (synthetic config surface: stub GREEN, done-1/2 RED `UNCOVERED`, done-2/2 GREEN, strict-stub RED) |
| `check_release_lane.sh` | **G-REL** `cargo test --release` green [R5] (WS18) | the real workspace: `cargo build --release --workspace` then `cargo test --release --workspace --no-fail-fast` | zero failing tests in the RELEASE profile ⇒ GREEN. `ci.sh` steps 3/4 only ever exercise debug, so nothing caught release rotting to 22 failures / 7 targets before WS18. Tests that drive a seam the port deliberately compiles out without `debug_assertions` (tripwire fail points; the slow-runtime-safety integrity gate / `pin_is_valid`) carry `#[cfg_attr(not(debug_assertions), ignore)]` and are counted as *ignored*, never failed — they still run and pass in `cargo test`. The build step is required, not incidental: `rustty-apprt-c`'s C-ABI smoke asserts the published cdylib/staticlib exist in `target/<profile>/`, which only `cargo build` uplifts. The whole run is watchdogged (`RELEASE_LANE_TIMEOUT`, default 2700s) so a HANG is RED, never an infinite wait. `--crate <name>` scopes it for cheap local iteration. | `--self-test` (drives `verdict()` with canned cargo output: clean ⇒ 0, failing ⇒ 1) |

Helpers (not gates themselves):
- `_gatelib.py` — shared library: contract loaders, the file→crate map (from
  `tests-summary.json`'s `crate_map`), crate-src discovery, `PORT STATUS`
  done-detection, and string/comment-aware Rust scanners for `#[test]` fns,
  in-test assertions (`assert*!`, `.expect`/`.unwrap`, `?`-propagation,
  `#[should_panic]`), and comment-line census by class.
- `cargo_cycle_check.py` — the real-rustty-Cargo-graph SCC checker that backs G7
  (dependency-free TOML scan + Tarjan; `--fail-on-scc` default on).

## G3b (`check_comment_coverage.py`) — beyond-pin comment deltas are EXPECTED (WS19 T2 verdict)

**Read this before "fixing" a G3b `missing` count that went UP after a port lane.**

`check_comment_coverage.py:34` resolves its reference side from `GHOSTTY_SRC`, defaulting
to `/Users/gael/projects/ghostty/src` — the oracle **working tree, which is pinned at
`4749c4e93`**. Consequence: when a lane ports a **beyond-pin** upstream state in which
upstream **DELETED** a comment, that sentence still exists on the reference side at the
pin, so the gate reports it `missing`. WS7 D2's config lane went `5 → 9` for exactly this
reason. **The rise is an artifact of the pin, not a fidelity regression.**

**VERDICT [WS19 T2, 2026-07-29 — audit row N6].** **Advancing the pin in Phase 3 resolves
this WHOLESALE, and is the only correct fix.** Once `GHOSTTY_SRC` is the resynced tree,
the deleted sentences are absent from the reference side and the counts drop *by
construction* — no per-file exception list, no gate change, no R5 exposure. Phase 3
carries this as re-vet assertion **P3-RV2** (`.port/phase-3/README.md`).

**FORBIDDEN: a per-file / per-lane `GHOSTTY_SRC` override.** That is precisely the spoof
hole `.port/tools/ci.sh:114-129` deliberately killed for G1b — a reference path
redirected away from what the gate actually reads yields a **vacuous green** (R5). Do not
re-open it here.

**Until the pin advances:** each lane records its own beyond-pin delta in its own report
(which file, which sentence, and the upstream commit that deleted it), so the residual is
attributable rather than merely tolerated. A `missing` rise with **no** such attribution
is a real regression and must be treated as one.

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
3234 tests, **7** live in non-DAG assets (`xtask`) outside `crates/`; the
in-`crates/` workspace contract is therefore **3227** (the former `extra` 2
now live in-workspace). G1 prints this decomposition explicitly so
3227-vs-3234 is never read as a discrepancy.

**Invariant (R5):** no gate may be weakened, skipped, xfail'd, or its threshold
loosened to pass. A red gate that cannot be made honestly green ⇒ STOP and
surface a precise blocker. If `cargo miri` is ever missing, the G6 Miri lane
reports that loudly and exits non-zero — BLOCKED, never waived. Tests Miri
cannot execute (live-syscall binaries) are ROUTED to ASan per the 4-criterion
rule in `.port/phase-2/MIRI_EXCEPTIONS.md`, never skipped.

- `check_rustty_vt_headers.sh` — ABI-fidelity gate for the curated public C header family `include/rustty/vt.h` + `vt/*.h`: (a) compiles clean, (b) declared symbols are real exports (native ∪ wasm) cross-checked vs the cbindgen `rustty.h` ground-truth, (c) struct sizes/offsets match `rustty_type_json()`. Run after any C-ABI or header change.
