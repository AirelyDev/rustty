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
| `run_differential.sh` | **G4** differential VT clean | `phase-0/harness/` (comparator + `bin/vt_driver_ref` + `bin/vt_driver_port` and `bin/cvt/*` from `build_port_driver.sh`), `phase-0/corpora/cvt/*.golden` (21 c-vt cases) | ARMS itself the moment `harness/bin/vt_driver_port` (or `$RUSTTY_VT_DRIVER`) exists. Armed: **leg A** — every `harness/fixtures/*.vt` through `comparator.py <input> --ref vt_driver_ref --port vt_driver_port` (byte-exact canonical output); **leg B** — all 21 port-built c-vt example binaries (`harness/bin/cvt/<case>`) vs the stored goldens (combined stdout+stderr byte-diff + exit-code match; a missing binary is RED `unbuilt`, never a skip). ANY diff ⇒ RED (exit 1), never a warning; each failure writes `harness/results/diag/<case>.diag` (class crash/hang/diverge/unbuilt + first diverging byte w/ hex context). progress (driver absent or `--progress`): runs `gate_empty_divergence.sh` pinned to an empty stub (proves the comparator detects 100% divergence) ⇒ `HARNESS-WIRED` (exit 0, distinct from a parity green). BLOCKED if harness/corpus absent, and (driver-source staleness guard) when the oracle is absent or not at `ORACLE_PIN`, lacks `zig-out/include`, there is no C compiler, or `ORACLE_PIN` is missing/malformed; the BLOCKED line names the reason. | `--self-test` (injection flip proof: ref-vs-ref GREEN control; 1 corrupted byte ⇒ RED exit 1; restore ⇒ GREEN; golden-leg corrupt detected + restore clean) and `--self-test-oracle` (R13-GF3/GF4: absent/off-pin oracle ⇒ 3; GIT_DIR decoy ignored; the checked-in pin is the one enforced; a COPY of G4 run end to end in a fixture checkout ⇒ 3 for an off-pin, missing, empty, 39-hex, upper-case or CRLF `ORACLE_PIN`) |
| `run_fuzz_smoke.sh` | **G5** differential fuzz clean | `crates/rustty-vt/fuzz/` (cargo-fuzz `vt_diff`), `harness/FUZZ-INVENTORY.md`, `rustty-synthetic` (corpus gen) | real smoke (when `cargo fuzz` + target exist): zero new `fuzz/artifacts/`, exit 0 ⇒ GREEN; new artifact/non-zero ⇒ RED. progress (port empty): verifies fuzz inventory + corpus wiring ⇒ `FUZZ-WIRED` (exit 0). Budgets via `FUZZ_SECS`/`FUZZ_RUNS`. | `--self-test` (asserts FUZZ-INVENTORY + synthetic crate + cargo present) |
| `check_leaks.sh` | **G6** memory-safe (UB + leaks) | `crates/*/Cargo.toml` + `crates-sys/*/Cargo.toml` (enumeration) + `_gatelib` done-detection (statuses done\|complete\|parity\|ported\|final, shared with G1/G2/G3b) | default / `--all-done`: for every claimed-done crate run the right tool per R6 — Miri (pure Rust: UB + leaks) or ASan (FFI crates + all `*-sys`: Miri can't run C); live-syscall test binaries (e.g. rustty-os `--test cf_release_live`, real kqueue) are ROUTED Miri→ASan per `MIRI_EXCEPTIONS.md` §"Live-syscall test binaries", never skipped. All clean ⇒ GREEN (exit 0); any UB/leak ⇒ RED. `--crate <name>` scopes to one crate; `--list` prints crate/status/gated/lane without running. FFI *leak* checks deferred to Linux CI (LSan/valgrind absent on arm64 macOS, DoP §G6). | `--self-test` (Miri flip proof: an injected leak and an injected OOB-read UB must both be DETECTED) |
| `check_acyclic.sh` (+ `cargo_cycle_check.py`) | **G7** crate graph acyclic [R4] | the **actual** `crates/*/Cargo.toml` `[dependencies]` / `[build-dependencies]` (dev-deps excluded; `--include-dev` to add) | parses the real rustty Cargo graph, runs Tarjan SCC; **0 cross-crate SCCs ⇒ GREEN** (must be GREEN on the empty/stub workspace — it is: 40 crates, 181 edges, 0 cycles). Any cross-crate cycle ⇒ RED with the `CYCLE:` members + edges. `--with-reference` additionally prints `.port/tools/cycle_check.py`'s runtime SCC line (informational: models the *un-broken* reference, per the DoP exit-code caveat). | `--self-test` (synthetic 2-crate tree: acyclic GREEN, then injects a cycle ⇒ RED) |
| `check_baseline_regression.py` | **G8** no perf/mem/size/startup regression [R5] | `phase-0/baseline/BASELINE.json` (frozen metrics) + a candidate `--metrics PATH` (same shape) | every shared metric within tolerance: size 0% (growth ⇒ regress), throughput −7%, time/RSS +7%, startup +10% (overridable). RED `REGRESS <metric>: baseline=… now=… (+z%)`. progress (no `--metrics`): prints the baseline it *would* compare and exits 0 (nothing to regress) — never fabricates a green. `--strict`: also RED if candidate drops a baseline metric. | `--self-test` (synthetic baseline+candidate: no-metrics GREEN, equal GREEN, −50% throughput RED, +1B size RED, −2% within-band GREEN) |
| `check_parity_surface.py` | **G9** full surface coverage (config/keybind/action/terminfo) [R2] | `inventory/parity-surface.json` (config options, keybind/cli actions, terminfo caps, modes/sgr/osc — each a stable id) | every surface id present in its owning crate's ported Rust AND (`--require-tests`/`--strict`) referenced by a ported test; uncovered set empty ⇒ GREEN. RED `UNCOVERED <kind>: <id>` when a claimed-done owner crate has an uncovered id (progress) or any id uncovered (`--strict`). terminfo also diffs `infocmp rustty` cap set vs surface (R2-terminfo-cap-parity) when the entry is installed. | `--self-test` (synthetic config surface: stub GREEN, done-1/2 RED `UNCOVERED`, done-2/2 GREEN, strict-stub RED) |
| `check_release_lane.sh` | **G-REL** `cargo test --release` green [R5] (WS18) | the real workspace: `cargo build --release --workspace` then `cargo test --release --workspace --no-fail-fast` | zero failing tests in the RELEASE profile ⇒ GREEN. `ci.sh` steps 3/4 only ever exercise debug, so nothing caught release rotting to 22 failures / 7 targets before WS18. Tests that drive a seam the port deliberately compiles out without `debug_assertions` (the slow-runtime-safety integrity gate / `pin_is_valid`) carry `#[cfg_attr(not(debug_assertions), ignore)]` and are counted as *ignored*, never failed — they still run and pass in `cargo test`. Tripwire fail-point tests are NO LONGER in that set for `rustty-util`/`rustty-vt` (W57-B03 / W59-B04): `tripwire::Module::enabled()` is now `cfg!(any(test, debug_assertions, feature = "tripwire-test"))`, and `rustty-vt` turns the off-by-default `rustty-util/tripwire-test` feature on as a **dev-dependency** — so those tests RUN in the release lane, matching Zig's `builtin.is_test` (true at any optimize mode), while `cargo build --release` (no dev-deps) still compiles `check` away. Fail-point tests in crates that have not yet opted in (`rustty-font`, `rustty-font-shaper`) keep the attribute. The build step is required, not incidental: `rustty-apprt-c`'s C-ABI smoke asserts the published cdylib/staticlib exist in `target/<profile>/`, which only `cargo build` uplifts. The whole run is watchdogged (`RELEASE_LANE_TIMEOUT`, default 2700s) so a HANG is RED, never an infinite wait. `--crate <name>` scopes it for cheap local iteration. | `--self-test` (drives `verdict()` with canned cargo output: clean ⇒ 0, failing ⇒ 1) |
| `check_stb_resize_parity.sh` | **D10** vendored `stb_image_resize` output == ghostty's own build of it [R1/R3] (WS11) | `crates/rustty-font-shaper/vendor/{stb_image_resize.h,stb_impl.c}` (ours) and, read-only via `rustty_oracle_git -C <oracle> show <PIN>:src/stb/{stb.c,stb_image.h,stb_image_resize.h}` (NEVER a checkout; the oracle is `ghostty` beside the main checkout, found by `_oracle_root.sh` with no environment override; `<PIN>` is the checked-in `ORACLE_PIN`, not the oracle's HEAD; `GIT_*` stripped), the oracle's copies + its `SharedDeps.zig` build recipe | ARM 1: our vendored header built with the host `cc` at -O0/-O1/-O2/-O3/-Os must agree within the same measured ±1-on-≤3 % codegen envelope (byte-identical on Apple clang; Fedora gcc 16's `-O0`/`-O1` form a third ±1 variant vs its own `-O2`/`-O3`/`-Os`, which is why the arm is an envelope and not a hard byte-equality). ARM 2: ghostty's own recipe (`-I src/stb` + `stb.c`, no extra flags on non-MSVC) at all four Zig optimize modes vs the consensus — `Debug`/`ReleaseSafe`/`ReleaseSmall` must be **byte-exact**, `ReleaseFast` within **max delta 1 on ≤3 % of bytes** (a measured property of its loop vectorizer, not a fudge factor; the largest rate observed anywhere is 2.21 %, and a hard 2 % would make the gate falsely RED on any gcc host — see the permanent-divergence entry in `.port/phase-2/DEFERRALS.md`). Then `negative_control.py` requires nearest-neighbour and bilinear resamplings of the same input to fall FAR outside that envelope (measured 55–98 % of bytes at max delta 14–192, a ~14× margin in delta), so the envelope provably cannot admit a wrong resampler. `exit 3` BLOCKED when `zig` is absent, or the oracle repo is absent or lacks the pin commit (consensus arm + negative control still run and can still turn it RED). The pipeline half of the seam is covered in-crate by `face::freetype::tests::stb_resize_kat_*`. | `--self-test` (two reachable RED arms: rebuilding our header with `-ffast-math` makes `run.sh` RED — which is also why no flags are pinned in `build.rs`; and `negative_control.py --self-test` proves its own RED arm fires on a +1-on-1 % near miss); run end to end by G-ORACLE-SH-SELF: `run.sh --print-oracle-reads` reads the pinned fixture oracle under a `GIT_DIR` decoy, and is BLOCKED without the pin commit |
| `check_eol_fidelity.sh` | **G-EOL** byte fidelity of the corpora, fixtures and vendored trees [R1] (2026-08-26) | `.gitattributes` and the index (`git check-attr`, `git ls-files --eol`) for the byte-exact families (`.port/phase-0/corpora/cvt`, `corpora/terminfo/compiled`, `harness/fixtures`, `crates-sys/*/vendor/**`, `dist/windows`); the oracle's `test/fuzz-libghostty/corpus` (the oracle is `ghostty` beside the main checkout, found by `_oracle_root.sh`, no environment override) | Arm 1: every family carries `binary`/`-text`. Arm 2: no index/worktree EOL divergence in those families (an empty file list is RED). Arm 3: every shared fuzz seed is byte-identical to the oracle's, and the oracle must be AT THE PIN (`ORACLE_PIN`) with a clean corpus directory. Oracle absent, off the pin, dirty, or sharing no seed => `exit 3` BLOCKED (ci.sh: a WARNING, never GREEN); a RED in arm 1 or 2 still wins (`exit 1`). Arms 1-2 read the checked repository's own index, so by design they honour `GIT_INDEX_FILE` in a pre-commit hook. | End to end in G-ORACLE-SH-SELF: GREEN at the pin; BLOCKED when absent, off the pin, or with a dirty corpus; RED wins over BLOCKED; an override re-read anywhere in the gate is caught |
| `_oracle_root.sh --self-test` | **G-ORACLE-SH-SELF** the shell oracle resolver and every oracle reader [R5] (R13-GF2, extended in R13-GF3 and R13-H2) | A throwaway main checkout with a linked worktree, a pinned fixture oracle beside it, a decoy repository and a decoy oracle; the real readers (G-EOL, G4, D10's `run.sh`, the menu-shortcut parity harness, G-BENCH, G-LZ4, A5 and its Node checker) and `ORACLE_PIN` | The oracle resolves to the main checkout's sibling from a worktree and agrees with `_gatelib.oracle_root`. Neither `GIT_DIR`/`GIT_WORK_TREE`, a failing `git`, nor a retired override (`GHOSTTY_ORACLE`, `GHOSTTY`, `GHOSTTY_ROOT`, `ORACLE_TREE`) moves it. The ONE pin file is read identically by `rustty_oracle_pin` and `_gatelib.oracle_pin`, no other TRACKED `.port/tools` file spells the pin in full or as a 7-hex piece, and no other tracked gates file holds a 40-hex constant (untracked and ignored files, such as a stale `.pyc`, are skipped; a fixture repository proves a tracked copy is caught, an untracked or ignored one is not, and a tree `git ls-files` cannot list is BLOCKED). D10's oracle READ returns the fixture's files at the pin under a `GIT_DIR` decoy. G-EOL runs end to end (row above). A structural scan (a lint) checks that each reader assigns its oracle variable once, never runs plain `git` with it on one line, and keeps fixed per-file counts of every `git` word in any spelling (counted after deleting `"`, `'` and `\`, so `gi''t`, `gi\t` and `eval "g""it ..."` count) and of every bare mention of the variable (`printf -v`, `read`, `declare`, ...); that no reader holds an escape spelling a letter or digit (`$'\x67it'`, `\147`, `g`); that the Node checker spawns git (either quote style) only in `oracleGit` and forms no `git` by joining literals (`'g'+'it'`); and that Python runs `git -C <oracle>` only in `_gatelib.oracle_git` and forms no `git` by joining literals or with a letter escape. A failing count names the spec entry to update and says the read must go through `rustty_oracle_git`. Arm 10s puts each of those bypasses into a copy of a real reader and requires RED. Finally it runs the `--self-test-oracle` of G4, G-BENCH, G-LZ4 and A5. An arm that cannot check (no `python3`, no `node`) makes it `exit 3` BLOCKED, never GREEN. | It is the self-test; each arm's negative control is recorded in the R13-GF2 and R13-GF3 reports |
| `check_ref_item_geometry.py` | **G-REFGEOM** the reference's set-item geometry == the golden the port's shadows are pinned to [R1/R5] (R13-L2, brought under the oracle-gate rules by R13-L3; NEW-FINDINGS N88/N91) | `fixtures/ref_item_geometry_probe.zig`, appended to a scratch copy of the oracle's `src/terminal/page.zig` (the oracle is `ghostty` beside the main checkout, found by `_gatelib.oracle_root`, no environment override; every oracle git read through `_gatelib.oracle_git`); `fixtures/ref_item_geometry_golden.txt` | `zig build test-lib-vt-build -Dtarget=<t>` reports `@sizeOf`/`@alignOf` of `Style`, its parts, `PageEntry.Id`, `Offset(u8).Slice` and both set items for aarch64-macos, aarch64-linux-gnu and wasm32-wasi; the lines must equal the golden (RED on any difference, or when the probe no longer compiles). The oracle must be the top level of its own repository, at HEAD == `ORACLE_PIN` and with no modified tracked file. Oracle absent, off the pin, dirty or not a repository, no git, no Zig, or `ORACLE_PIN` missing or malformed => `exit 3` BLOCKED (ci.sh: a WARNING, never GREEN). `ref_geometry_golden_tests` checks the port's side against the same golden, and the wasm32 build's `const` asserts pin the shadows to its wasm32 lines. | `--self-test` (G-REFGEOM-SELF): a copy of the gate, run end to end in a fixture main checkout and linked worktree with a fixture oracle and a stand-in `zig`: GREEN at the pin (also from a worktree with no stopgap symlink, and under a dirty, off-pin `GIT_DIR` + `GIT_WORK_TREE` decoy with a decoy cwd); BLOCKED for an absent, off-pin, dirty or non-repository oracle, a missing git, and a missing, empty, 39-hex, upper-case or two-line `ORACLE_PIN`; a clean decoy cannot mask a dirty oracle; a measured difference is RED |
| `check_log_reconcile.py` | **G-LOGFILE** diagnostics reconcile per reference FILE [R1/R5] (NEW-FINDINGS N12/N14/N15/N20-N22/N81) | the oracle's live `log.*` sites (oracle beside the main checkout via `_gatelib.oracle_root`, at `ORACLE_PIN`); `.port/parity-closure/LOG-PORTMAP.tsv` (reference file -> port arms, allowance, bypass cap, `#!exclusion-cap`), `LOG-TEXT-DIVERGENCES.tsv` (`#!divergence-cap`/`#!unpairable-cap`), `LOG-DEAD-SITES.tsv` (`#!dead-site-cap`), `LOG-PORT-ONLY.tsv` (`#!port-only-cap`), `LEDGER.tsv` LOG rows | for every log-bearing reference file: reference sites minus declared-dead sites, against the thinnest platform arm's live port emissions minus the declared PORT-ONLY emissions (port diagnostics with no reference counterpart, keyed by enclosing fn + call text, each required to select exactly one live emission); the deficit must equal the reasoned allowance (a larger allowance is RED, a ratchet). Also RED: an undeclared log-bearing file, a facade bypass over its cap, a message that does not match its preserved reference comment, a port emission at a declared-dead site, a stale/ambiguous/doubled port-only row, and any of the five caps off by one in either direction. Oracle absent or off-pin, an empty portmap, a missing cap directive or a malformed declaration file => `exit 3` BLOCKED. `--report` prints the per-file deficit table. | `--self-test`: synthetic arms for every RED/GREEN branch (including the N81 arms: a declared port-only line is subtracted, and a lost reference site goes RED behind it), a live-tree scanner-vs-grep arm, a literal-shape arm, and the oracle-resolution arm |
| `w2f001_windows_check_gate.sh` | **G-WINCHK** windows-msvc type-check parity [R1/R3] (W2, codex-vet F-001) | the real workspace: `cargo check -p rustty-os -p rustty-vt -p rustty-vt-c --target x86_64-pc-windows-msvc` | all three crates TYPE-CHECK for `x86_64-pc-windows-msvc` (the surface the oracle CI-tests as `build-libghostty-vt-windows`) AND the host lane stays green with `RUSTTY_SYS_CHECK_ONLY` UNSET. `rustty-os` had never compiled for windows at all (25 errors), which cut vt/vt-c/termio off entirely. `RUSTTY_SYS_CHECK_ONLY=1` skips the vendored-C `-sys` `cc` builds for step 1 only — `cargo check` never links, and this host has no MSVC sysroot; the switch is opt-in so a real build can never silently lose the native code. **Limit, stated in the script header: type-checking ONLY — no windows host or CI lane exists, so the ntdll/ConPTY/VirtualAlloc arms are never linked or executed.** `exit 3` BLOCKED (never a pass) when the windows rust-std is absent. | step 2 IS the control: it re-runs the same three crates on the host with the env unset, so a `-sys` skip that swallowed the native build shows up as a host failure |
| `check_default_linux_winproto.sh` | **G-WINPROTO-DEFAULT** the DEFAULT GUI dependency graph, pinned per target [R1/R5] (F-007; FreeBSD arm added by W2D, codex-vet F-002) | the real manifests, resolved without building: `cargo tree -p rustty --target {x86_64-unknown-linux-gnu, x86_64-unknown-freebsd, aarch64-apple-darwin}` | the Linux AND FreeBSD default graphs contain `gtk4`/`libadwaita` and BOTH winproto arms (`gdk4-wayland`/`gdk4-x11`/`wayland-client`), and the macOS default graph contains NONE of them. Ghostty gives FreeBSD the same `.gtk` runtime it gives Linux (`apprt/runtime.zig:14-24`), and the gtk4-rs `-sys` crates probe pkg-config FOR THE TARGET, so a gtk-featured `cargo check --target x86_64-unknown-freebsd` is impossible off a FreeBSD host — this graph identity is the machine-checkable half. **Limit: dependency RESOLUTION only — nothing is compiled, linked or run for freebsd.** | the macOS arm is the control (a target gate that stopped gating fails it), and each arm first requires `rustty-apprt` in its graph so an empty/errored `cargo tree` can never pass |

Helpers (not gates themselves):
- `../stb-resize-oracle/` — D10's harness: `oracle.c` (deterministic input → raw
  resized bytes, links no rustty code so the C build is the only variable),
  `run.sh` (the toolchain matrix), `bindiff.py` (the one measurement primitive:
  `<n>/<total> differ, maxdelta=<d>`), `negative_control.py` (proves the envelope
  discriminates).
- `_gatelib.py` — shared library: contract loaders, the file→crate map (from
  `tests-summary.json`'s `crate_map`), crate-src discovery, `PORT STATUS`
  done-detection, and string/comment-aware Rust scanners for `#[test]` fns,
  in-test assertions (`assert*!`, `.expect`/`.unwrap`, `?`-propagation,
  `#[should_panic]`), and comment-line census by class.
- `cargo_cycle_check.py` — the real-rustty-Cargo-graph SCC checker that backs G7
  (dependency-free TOML scan + Tarjan; `--fail-on-scc` default on).

## G-P2 (`check_phase2_closure.py`) — the Phase 2 closure invariant (WS19 T19)

**What it enforces.** Phase 2 is a COMPLETION roadmap, so "deferred to a later phase" is
not an acceptable disposition. Every open item across `.port/phase-2/DEFERRALS.md`,
`VERIFICATION_DEBT.md`, `MIRI_EXCEPTIONS.md` and `INTEGRATION_PARITY.md` must end as
exactly one of: struck/fixed; a permanent **Justified R1 Divergence**; **verification
debt with a runnable recipe**; or OPEN **with an owner that is an OPEN workstream**
(or `Phase 3`). The gate FAILS on the last case when the owner is missing or names a
workstream that has already merged.

**Why it exists.** The WS19 closure audit found that **21 of 38 genuinely-open items were
ORPHANED** — filed into a closed workstream, or never owned at all. Nobody was ever going
to pick them up, and nothing detected it. This gate makes that class structurally
impossible to repeat silently.

**The owner set is DERIVED, never hardcoded.** `OPEN_WORKSTREAMS` comes from the
plan-authoring checklist in
`docs/superpowers/plans/2026-07-23-rustty-completion-ROADMAP.md` — the same text a human
reads. A hardcoded list would rot exactly when it matters: the day a workstream merges,
every row still filed against it becomes an orphan, and a stale list would keep passing
them. Inspect the derivation with `--check-derivation`.

**Per-file contracts.** The four ledgers have different shapes, so the gate has three
readers rather than one over-general one: `DEFERRALS.md` (entry + open-marker model),
`VERIFICATION_DEBT.md` (must name an owner **and** carry a fenced-code-block recipe —
that file's own header promises one), and `INTEGRATION_PARITY.md` (`### N.N` sections +
the fix list; `## N. Sanctioned Divergences` is excluded, being that file's analogue of
`# Justified R1 Divergences`).

**Anti-vacuity.** A non-empty ledger in which ZERO items are found is a LOUD failure, not
a pass — a parser that silently matches nothing reports every file as clean, which is the
failure mode this program has already hit twice. An empty derivation is `exit 3` BLOCKED.
A missing ledger file is `exit 3` BLOCKED, never a silent green.

**Exit codes:** `0` clean · `1` unowned or closed-WS-owned item(s) · `3` BLOCKED.

**Reproduce all three arms (each was observed and recorded at landing):**

```sh
# FAIL — items filed to a merged WS, and an item with no owner at all. Must be 1.
python3 .port/tools/gates/check_phase2_closure.py \
  --fixture .port/tools/gates/fixtures/phase2-closure-orphan.md; echo "must be 1: rc=$?"

# PASS — the positive control: same shape, owner is an OPEN workstream. Must be 0.
python3 .port/tools/gates/check_phase2_closure.py \
  --fixture .port/tools/gates/fixtures/phase2-closure-owned.md;  echo "must be 0: rc=$?"

# BLOCKED — absence is never a silent green. Must be 3.
python3 .port/tools/gates/check_phase2_closure.py \
  --fixture /nonexistent.md; echo "must be 3: rc=$?"

# DERIVATION IS LIVE — mark an open WS as merged in the ROADMAP and the PASS fixture
# must FLIP to FAIL. (Verified with WS14: rc 0 -> 1, then 1 -> 0 on revert.)
python3 .port/tools/gates/check_phase2_closure.py --check-derivation
```

**Do NOT downgrade it to PROGRESS to get a green CI.** It is registered HARD. If it is
RED, Phase 2 is not closed — the output IS the remaining worklist, and relaxing the gate
to hide that is precisely the R5 weakening the gate exists to prevent.

## G3b (`check_comment_coverage.py`) — beyond-pin comment deltas are EXPECTED (WS19 T2 verdict)

**Read this before "fixing" a G3b `missing` count that went UP after a port lane.**

`check_comment_coverage.py` resolves its reference side from the hard-coded
`/Users/gael/projects/ghostty/src` — the oracle **working tree** (the `GHOSTTY_SRC` env
override was **deleted** by `P3-W0 T2 Step 6`; see the FORBIDDEN note below). Consequence:
when a lane ports a **beyond-pin** upstream state in which
upstream **DELETED** a comment, that sentence still exists on the reference side at the
pin, so the gate reports it `missing`. WS7 D2's config lane went `5 → 9` for exactly this
reason. **The rise is an artifact of the pin, not a fidelity regression.**

**VERDICT [WS19 T2, 2026-07-29 — audit row N6].** **Advancing the pin in Phase 3 resolves
this WHOLESALE, and is the only correct fix.** Once the working tree is the resynced tree,
the deleted sentences are absent from the reference side and those counts drop *by
construction* — no per-file exception list, no gate change, no R5 exposure. Phase 3
carries this as re-vet assertion **P3-RV2** (`.port/phase-3/README.md`).

**CORRECTION [`P3-W10 T3`, 2026-07-31 — MEASURED].** The verdict above is right about the
*mechanism* and wrong about the *net effect*, and the difference matters when you read a
G3b number after an advance. An advance is **BIDIRECTIONAL**: the gate resolves its file
list from the FIXED phase-0 `crate_map` and re-reads those same files at the newer SHA, so
it removes the text upstream deleted **and adds the text upstream wrote** — all of which is
`missing` unless a lane ported it. Advancing `4749c4e93 -> 6ad1fe7d8` moved G3b's total
**218 -> 334**: `config 30->4`, `os 35->12`, `termio 23->14`, `bench 8->5` fell as predicted,
while `vt 116->218`, `global 0->31`, `unicode 0->20`, `cli 2->11`, `vt-c 4->10`, `lib 0->5`,
`simd 0->4` appeared. **Do not expect an advance to zero a row, and do not treat the rise as
an artifact** — it is the resync's real comment-coverage debt. Full before/after, per-crate
delta and per-file attribution of all 334: `.port/phase-3/w10/`.

**FORBIDDEN: a per-file / per-lane `GHOSTTY_SRC` override.** That is precisely the spoof
hole `.port/tools/ci.sh:114-129` deliberately killed for G1b — a reference path
redirected away from what the gate actually reads yields a **vacuous green** (R5). Do not
re-open it here.

**Between advances:** each lane records its own beyond-pin delta in its own report
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
