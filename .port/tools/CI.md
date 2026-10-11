# rustty CI — what's hard-fail vs progress (while porting)

**Single entry:** `.port/tools/ci.sh` (the kickoff CI runs exactly this).
**Mirror:** `.github/workflows/ci.yml` invokes `ci.sh` so CI is *real*, not a
paper gate. The check sequence lives in `ci.sh` only; the workflow just sets up
the toolchain and calls it. Keep them in lockstep.

Governed by `.port/PORT_RULES.md` (R1 fidelity, R2 identity, R3 self-contained /
no ghostty Cargo dep, R4 acyclic, R5 gates-invariant) and
`.port/phase-0/plan/DEFINITION_OF_PARITY.md` (the authoritative gate definitions
G1–G9). `ci.sh` **runs the canonical Phase-0 gate suite already present in
`.port/tools/gates/`** — it does not reimplement the gates.

---

## Run order (ci.sh collects ALL failures, never stops at the first)

1. `cargo fmt --check`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo build --workspace`
4. `cargo check -p rustty-wasm --target wasm32-unknown-unknown` (N526; BLOCKED, not skipped, when the rustup target is missing)
5. `cargo test --workspace`
6. the parity gates (hard-fail gates first, then progress)

CI exits **non-zero iff any *non-progress* check is red.**

`CI_SKIP_CARGO=1 bash .port/tools/ci.sh` runs the gates only (no cargo) for fast
local iteration.

Gates run side by side by default (`--jobs auto`); `--jobs 1` is the serial
run. See "Parallel, change-scoped and pruned runs" below for the scheduler,
`--changed-since` fix verification and the policy on when to run the full
suite.

---

## The canonical gate suite (`.port/tools/gates/`)

| DoP id | Script ci.sh runs | Helper | Class |
|---|---|---|---|
| **G7** | `check_acyclic.sh` | `cargo_cycle_check.py` | **HARD-FAIL** |
| **G4** | `run_differential.sh` | harness `gate_empty_divergence.sh`, `comparator.py` | **HARD-FAIL** |
| **G1-SELF** | `check_tests_ported.py --self-test` | — | **HARD-FAIL** |
| **G1** | `check_tests_ported.py` | `_gatelib.py`, `.port/phase-3/CONTRACT_DELETIONS.md` | PROGRESS* |
| **G1b** | `check_test_counts.py` (oracle-probed by ci.sh + self-guarded: missing zig ref ⇒ exit 3, never a vacuous green) | ghostty oracle checkout (R3, local only) | PROGRESS* |
| **G2** | `check_assertions.py` | `_gatelib.py` | PROGRESS* |
| **G3** | `check_comments_fidelity.py` | `_gatelib.py` | PROGRESS* |
| **G9** | `check_parity_surface.py` | `_gatelib.py` | PROGRESS* |
| **G8** | `check_baseline_regression.py` | `_gatelib.py` | PROGRESS* |
| **G5** | `run_fuzz_smoke.sh` | harness `FUZZ-INVENTORY.md`, `comparator.py` | PROGRESS* |
| **G6** | `check_leaks.sh` | Miri (pure) / ASan (FFI + live-syscall tests), `_gatelib` done-detection; Miri runs on `cargo miri nextest` (one process per test, P7-W15; `--libtest` keeps the old runner for one release) plus a libtest `--doc` arm; missing miri or cargo-nextest ⇒ exit 3 BLOCKED | PROGRESS* |
| **G6c** | `check_leaks.sh --debug-assertions-off` | Miri with `Page::assert_integrity` off — runs the vt page-level compression tests G6 can only TIMEOUT on; the annotation-derived names resolve to exact nextest paths, each of which must PASS, so a lost flag is RED (WS19 T6, P7-W15) | PROGRESS |
| **G6t** | `check_leaks.sh --tree-borrows` | Miri Tree Borrows over the pointer-heavy scope (`G6T_SCOPE`), with G6/G6c's split on debug assertions; `--self-test` checks every scope entry matches tests (P7-W15) | PROGRESS |
| **G-WASI** | `check_wasi_xev.sh` | wasmtime + the `wasm32-wasip1` std: runs `cargo test -p rustty-xev --target wasm32-wasip1` with wasmtime as the runner (V8), requires the 17 loop-driving `wasi_poll`/`wasi_watcher` tests to be named `ok`, and requires `wasi_file` to fail on its fixture without the `--dir=.` preopen. wasmtime only: wazero and iwasm reject the loop's absolute-time clock subscriptions and Node's WASI hangs on them; missing wasmtime or std ⇒ exit 3 BLOCKED | **HARD-FAIL** |
| **D1** | `check_slice_reinterpret.sh` | `.port/audits/slice-reinterpret.tsv` | PROGRESS |
| **D2** | `check_capacity_saturation.sh` | `.port/audits/capacity-saturation.tsv` | PROGRESS |
| **D3** | `check_constants_parity.py` | port markers + ghostty oracle | PROGRESS |
| **D4** | `check_comptime_phase.sh` | `check_comptime_phase.md`, `crates/rustty-vt/tests/d4_golden_vt_bytes.rs` | PROGRESS |
| **D5** | `check_paired_invariants.sh` | `.port/phase-2/VT_INVARIANTS.md`, `crates/rustty-vt/tests/d5_paired_invariants.rs` | PROGRESS |
| **D6** | `check_sanitizer_canary.sh` | `.port/phase-2/d6-canary/`, `d6_cfg_mapping_audit.py` | PROGRESS |
| **D8** | `check_clone_injection.sh` | (advisory review list) | PROGRESS |
| **D9** | `check_stale_comments.sh` | `.port/audits/stale-comments-D9.tsv`, `D9_STALE_COMMENT_CHECKLIST.md` | PROGRESS |

> **G-REFGEOM** (`check_ref_item_geometry.py`, tasks R13-L2 and R13-L3, NEW-FINDINGS
> N88/N91) re-measures the reference's set-item geometry (the style and hyperlink
> `RefCountedSet.Item` and their parts) with `fixtures/ref_item_geometry_probe.zig`
> for aarch64-macos, aarch64-linux-gnu and wasm32-wasi, against
> `fixtures/ref_item_geometry_golden.txt`, which
> `crates/rustty-vt/src/ref_geometry_golden_tests.rs` also checks the port against.
> Exit 0 GREEN / 1 RED / 3 BLOCKED.
>
> It is a HARD gate in ci.sh, registered like G-LOGFILE and G-EOL, which also need
> the local oracle: without Zig or the oracle at `ORACLE_PIN` it is BLOCKED (a
> WARNING), as on CI. It runs on every local ci.sh, not only when the pin moves,
> because the golden is an input: a shadow edited together with the golden keeps the
> Rust test green, and only a re-measurement of the reference catches that. It finds
> the oracle through `_gatelib.oracle_root`, reads the pin from `ORACLE_PIN`, and
> reads the oracle only through `_gatelib.oracle_git`. `G-REFGEOM-SELF`
> (`--self-test`) runs a copy of the gate against fixture oracles with a stand-in
> `zig`: BLOCKED for an absent, off-pin, dirty or non-repository oracle, a missing
> git, and a missing or malformed pin file; a `GIT_DIR` decoy moves nothing; a
> worktree finds the main checkout's sibling; a measured difference is RED. It needs
> neither Zig nor the oracle, so it binds on CI too.

> **G-ESCTEST** (`check_esctest.sh`, P8-W8 T1, DECISIONS P3; upstream 438d584f0 and
> edcaf6670) runs esctest2 against librustty-vt through the port's `test/esctest`
> runner (a separate Cargo package; esctest2 fetched by `test/esctest/fetch.sh`,
> pinned and SHA-256 checked) and against libghostty-vt through the reference's own
> `test/esctest` runner, built with `zig build` in a temporary `git archive` of the
> commit in `ORACLE_PIN` (never in the oracle or the Rustty tree, R3). It parses
> each log into one outcome per test (pass, fail, known-bug, skip) and the test's
> log block, with the two install paths normalized, and is RED when an outcome
> differs in either direction, when a block differs, when the logs name different
> tests, or when the Rustty log has no tally. A difference is accepted only by a
> row of `esctest_known_diffs.tsv` naming its UD or N record, and a row whose
> difference no longer occurs is RED (stale). Upstream's `test-esctest` job is
> report-only because most of the suite fails there; the differential is the
> parity value. Exit 0 GREEN / 1 RED / 3 BLOCKED (no oracle with the pinned commit,
> no Zig, python3, cargo or curl, no network for esctest2 or the reference's Zig
> packages, or an oracle log without a tally). BLOCKED on CI, where `ci.yml`'s
> report-only `test-esctest` job runs the port's side alone. `G-ESCTEST-SELF`
> (`--self-test`) checks the comparison on fixture logs: a matching pair (also
> under different install paths) is GREEN; an outcome difference either way, a
> detail difference, a missing test, a Rustty log without a tally, two empty logs,
> a stale ledger row, a ledger row with other outcomes and one without a record id
> are RED; an oracle log without a tally is BLOCKED. Its target-directory arms run a
> copy of the gate with stand-in tools: with `CARGO_TARGET_DIR` set, the fetch, the
> build and the run all use that directory, and a build that leaves no runner is RED
> without a stale runner ever running.
>
> Disk: a local G-ESCTEST run leaves the port's Debug build of `test/esctest`, about
> 1.2 GB, in `test/esctest/target` (or `CARGO_TARGET_DIR`), outside the root
> `target/`. It goes when the worktree is removed (memory `cleanup-as-you-go`); delete
> it by hand on a long-lived checkout. The oracle's copy and build are in a temporary
> directory the gate deletes.

> **G5 (differential fuzz)** and **G6 (leak-free)** are wired progress-mode
> runners now. G5: if `cargo fuzz` + the `vt_diff` target in
> `crates/rustty-vt/fuzz/` exist it runs a short smoke budget and fails on any
> new artifact; otherwise it verifies the fuzz inventory/corpus wiring and
> reports FUZZ-WIRED (exit 0). G6 (`check_leaks.sh --all-done`): for every
> *claimed-done* crate it runs the right tool per R6 — Miri
> (`cargo +nightly miri test`) for pure-Rust crates, ASan
> (`-Zsanitizer=address`) for FFI crates, with live-syscall test binaries
> routed Miri→ASan per `MIRI_EXCEPTIONS.md`; any UB/leak/unrouted binary is
> RED. `cargo miri` not installed ⇒ **exit 3 BLOCKED-not-waived** per DoP §G6
> (the script and this doc agree; CI installs nightly+miri unconditionally so
> the gate actually runs there). The per-crate gate template
> (DoP §2: P-T/P-A/P-C/P-PASS/P-DIFF/P-FUZZ/P-DAG/P-SURF/P-SIZE) applies to each
> crate as it is carved.

---

## Gate exit-code contract (how ci.sh interprets each gate)

| exit | meaning | ci.sh effect |
|---|---|---|
| `0` | PASS, or (PROGRESS gate) progress-green while crates are stubs | not a failure |
| `3` | BLOCKED — a contract/tooling input is missing | **WARNING** (surfaced, never a silent green); the summary line carries the gate's own `BLOCKED` reason line(s) |
| other non-zero | RED | HARD gate ⇒ fails CI; PROGRESS gate ⇒ a *claimed-done crate regressed* ⇒ fails CI (R5) |

In the `WARNINGS` summary each BLOCKED gate is followed by up to three
`reason:` lines, copied from the `BLOCKED` lines the gate printed. A gate that
is an expected warning on a lane (G4, G-EOL, D10, G-LOGFILE, G-REFGEOM, G-ESCTEST on CI) can
therefore not hide a new, different reason behind the same gate id: read the
reason, not just the id.

For the oracle gates, exit 3 means the oracle is **absent, off the pin
(`.port/tools/gates/ORACLE_PIN`), or dirty**, or the pin file itself is missing
or malformed: G-EOL arm 3, G4's staleness guard, G-LOGFILE and G-REFGEOM need the
oracle's HEAD to equal `ORACLE_PIN` (G-REFGEOM is also BLOCKED without Zig); D10 needs the oracle to have that commit (it reads
`src/stb` at the pin, whatever HEAD is), and so does G-ESCTEST (it archives the
pinned commit). See `.port/ORACLE_SYNC.md`.

---

## HARD-FAIL checks (red ⇒ CI fails, from day one)

These must be green right now and at every commit; they don't depend on any
crate being ported.

| Check | What it enforces | Why it's binding today |
|---|---|---|
| `cargo fmt --check` | formatting | scaffold + every future file must be `rustfmt`-clean |
| `cargo clippy --workspace -D warnings` | lint cleanliness | warnings are errors; the scaffold compiles clean and must stay so |
| `cargo check -p rustty-wasm --target wasm32-unknown-unknown` | the freestanding wasm build compiles (N526) | the `--webgl` lane is opt-in, so a wasm-only break (ca2ef434b) went unseen; BLOCKED when the target is not installed |
| `cargo build --workspace` | the workspace compiles | also the **live R4/G7 proof** — Cargo refuses a normal-dependency cycle, so a clean build *is* hard acyclicity |
| `cargo test --workspace` | every present test passes | PASS/FAIL oracle for all ported tests (R1/G1). Zero ported tests now ⇒ trivially passes; each ported test must pass as it lands |
| **G7** `check_acyclic.sh` | crate graph acyclic (R4) | parses the **real rustty Cargo graph** (`crates/*/Cargo.toml`): 29 crates, 93 inter-crate edges, **0 cross-crate SCCs** today (the workspace is wired in leaves-first topological order). `--self-test` proves it flips RED on an injected cycle. `--with-reference` additionally prints the reference structural pre-check (informational; `.port/tools/cycle_check.py` models the *un-broken* ghostty mapping and prints non-zero SCCs by design — the binding G7 result is the rustty Cargo graph) |
| **G4** `run_differential.sh` | differential VT clean + harness soundness | **ARMED** — keys on the port driver `harness/bin/vt_driver_port` (override `$RUSTTY_VT_DRIVER`; build with `.port/phase-0/harness/build_port_driver.sh`). **Driver present:** runs the real byte-exact differential — leg A: every `harness/fixtures/*.vt` through `vt_driver_ref` vs `vt_driver_port` via `comparator.py` (identical input + options, canonical output byte-diffed); leg B: every `corpora/cvt/*.golden` c-vt case against its port-built example binary (combined stdout+stderr + exit code must match the stored golden). Any diff ⇒ **RED (exit 1)**, never a warning; each failing case writes `harness/results/diag/<case>.diag` (crash/hang/diverge/unbuilt + first-diverging-byte hex context). `--self-test` proves the RED flip by corrupting one byte of a byte-exact stand-in port. **Driver absent:** progress mode — the harness self-test `gate_empty_divergence.sh` asserts the **CONTROL** (ref vs ref ⇒ `0.0000%` divergence) and the **empty-divergence** proof (empty port ⇒ 100% divergence on every fixture); exit 0 = "HARNESS-WIRED (port empty)", explicitly not a parity green |

### G4 arming (no edit required)

`run_differential.sh` watches for the port driver
`.port/phase-0/harness/bin/vt_driver_port` (override: `$RUSTTY_VT_DRIVER`);
`build_port_driver.sh` produces it by compiling the SAME `vt_driver.c` the
reference uses against `librustty_vt_c.a`, mapping the canonical `rustty_*`
ABI to the `ghostty_*` names the shared C sources expect via a generated ld64
alias list (the locked `ghostty_*`-aliases fork, derived from `nm` of the
staticlib on every build). It also builds the 21 port-side c-vt example
binaries into `harness/bin/cvt/` for leg B.

- **No port driver (port empty):** harness self-test mode — control 0% **and**
  empty port 100% divergence on every fixture. Exit 0 = "HARNESS-WIRED (port
  empty)", which is explicitly *not* a parity green; it proves the harness is
  correct.
- **Port driver present:** the byte-exact differential is the binding check —
  zero diffs across both legs (fixtures + cvt goldens) or **RED (exit 1)**. A
  missing/unlinkable port example binary is RED (`unbuilt`), never a skip.
  Failing cases write `harness/results/diag/<case>.diag`
  (crash/hang/diverge/unbuilt taxonomy + byte context).

**Oracle-arch note (R3) — G4 is LOCAL-ONLY:** the reference driver
`.port/phase-0/harness/bin/vt_driver_ref` is a Mach-O **arm64** binary linked
against `libghostty-vt.a` — the *external oracle*, never a Cargo dep, and never
shipped to CI (`.port/phase-0/harness/bin/` is gitignored, so no CI leg has
it). On GitHub CI `run_differential.sh` therefore reports **BLOCKED (exit 3) ⇒
WARNING** on every leg — an honest "did not run", never a fabricated green and
never a fake HARD claim; `ci.yml` prints an explicit
`G4: local-only (R3 — oracle not on CI)` note before invoking ci.sh. The
binding byte-exact differential is the LOCAL run, where `build_ref_driver.sh` /
`build_port_driver.sh` produce both drivers and any diff is RED (exit 1) and
fails ci.sh. (G1b `check_test_counts.py` is local-only for the same R3 reason:
the ghostty oracle checkout is absent on CI ⇒ probe exits 3 BLOCKED.) Locally,
G4's driver-source staleness guard also BLOCKS (exit 3) when the oracle is
absent, is not at `ORACLE_PIN`, lacks a built `zig-out/include`, or there is no
C compiler, and when `ORACLE_PIN` is missing or malformed. G-EOL, D10,
G-LOGFILE, G-REFGEOM and G-ESCTEST are BLOCKED on CI for the same no-oracle reason; `ci.yml`'s policy
note names all of them.

---

## PROGRESS\* checks (print progress, never fail CI *while crates are unported*)

Each keeps its full `DEFINITION_OF_PARITY.md` pass condition — **none is
weakened (R5)**. The canonical checkers implement progress natively via crate
**done-status detection** (`_gatelib.py` reads `PORT STATUS:` in each crate's
`lib.rs`):

- A **stub** crate (still `PORT STATUS: stub`) with 0 ported tests/comments/etc.
  is **GREEN/progress** (exit 0) — it is not graded against the contract yet.
- A crate that **claims done** (status `done`/`complete`/`parity`/`ported`/
  `final`) yet is short — fewer tests than contract, a weakened assertion, a
  dropped comment, an uncovered surface id, a regressed metric — is **RED**
  (exit non-zero), and ci.sh **fails**.

So while crates are unported every PROGRESS gate exits 0 and prints a progress
line; the instant a completed crate falls short the gate self-flips to binding.
That self-flip is exactly R5-correct: *progress now, binding on completion* — no
threshold is ever loosened. (Verified: flipping `rustty-util` to `PORT STATUS:
done` with 0 ported tests turns G1/G2/G3 RED and ci.sh exits 1; reverting to
`stub` restores progress-green.)

| Gate | DoP id | Contract (frozen) | Progress today | Goes binding when |
|---|---|---|---|---|
| `check_tests_ported.py` | G1 | **3234** test blocks, count == contract, names match — less only the contract entries upstream DELETED or RENAMED, each re-verified live against `.port/phase-3/CONTRACT_DELETIONS.md` (a rename is excused only when the port carries the successor; every subtraction is printed, a bad or unused row is RED) | per-crate `0/contract`, workspace `0/3225` ported | a crate is `done` (then per-crate ported == contract, names match) |
| `check_test_counts.py` | G1b | per mapped module: rust `#[test]` `>=` zig `test{}` minus documented apc-deferred — the only fidelity net for the non-`src/` crates outside the 3234 contract (vaxis/zf/xev/wuffs/z2d/…) | binding NOW for every mapped module (no done-status grace) | **already binding** wherever the ghostty oracle checkout exists. It greps the oracle directly (R3: external, never on CI), so an absent oracle can never read green — two non-env-overridable defenses: ci.sh probes the **same hard-coded root the script greps** (one source of truth; absent ⇒ **BLOCKED (exit 3) ⇒ WARNING**), and the script **self-guards** — any explicitly listed zig ref file missing (absent/partial oracle or zig-cache dep checkout) ⇒ exit 3 BLOCKED from inside the gate, so no wrapper/probe mismatch can convert zig=0 into a vacuous green; oracle present ⇒ any dropped test is RED and fails CI |
| `check_assertions.py` | G2 | assertion-for-assertion, per-test `rust >= ref` (floor **11975**) | per-crate `rust=0 / ref`, workspace `rust=0` | a `done` crate (then no weakened assertion) |
| `check_comments_fidelity.py` | G3 | per-file comment delta `>= 0` after identity-rename (census **42069**) | per-crate `rust / ref` comment lines | a `done` crate (then delta `>= 0`, no DROPPED) |
| `check_parity_surface.py` | G9 | every config/keybind/action/terminfo id covered + test-referenced; zero residual `ghostty_*` (R2) | `0/677` surface ids covered | an owner crate is `done` (then no UNCOVERED) |
| `check_baseline_regression.py` | G8 | every perf/mem/size/startup metric `<= baseline*(1+tol)` | no candidate metrics ⇒ "nothing can regress" | the port emits a candidate metrics JSON (then no REGRESS) |
| `check_perf_fence.sh` | G8b | Fence 1: rustty beats the pinned oracle on all 14 configs; Fence 2: no self-regression (+5% / +15%) against the stored entry binary in the MAIN checkout's `.perf-bins/`, whose manifest (commit, sha256, date) is printed | no entry binary (or no manifest) ⇒ Fence 1 still runs and writes G8's metrics, but the verdict is **BLOCKED (exit 3)** with a reason line; an entry whose sha256 is not its manifest's is RED; any RED wins | an entry is stored on purpose with `--store-entry` at a phase close-out after a GREEN perf verdict (never automatically) |

A **BLOCKED** result from any of these (e.g. `tests-summary.json` lacks the
`crate_map`, `parity-surface.json` absent, `BASELINE.json` unreadable) exits 3
and is surfaced by ci.sh as a **WARNING** — pending contracts/tooling are
BLOCKED, never GREEN-by-default (DoP §0/§5).

---

## Bun semantic-mismatch detectors (D1–D9)

These are **additive** gates (they weaken no existing G-gate, R5) ported from the
Bun post-mortem catalog in `.port/phase-2/BUN_PORT_LEARNINGS.md` §4. Each kills
one class of *invisible* Zig→Rust divergence — a mismatch that produced no
compile error, no Miri finding, and no failing test, so it slipped every one of
Bun's gates. D7 is intentionally absent (no detector was carved for it).

| DoP id | Kills (Bun class / issue) | What it checks | Kind |
|---|---|---|---|
| **D1** | Class 1 — slice reinterpret truncate-vs-panic (`oven-sh/bun#31188`) | every `bytemuck::cast_slice`/`align_to`/widening `from_raw_parts` hit is allowlisted in `slice-reinterpret.tsv` with a decision + guard test; unlisted/stale/unguarded ⇒ RED | PROGRESS |
| **D2** | Class 2a — capacity off-by-one silent OOB write (`#31503`) | every fixed `[T; N]` + write-then-increment counter pair is inventoried and names a `_capacity_saturation` test that drives it to the ceiling | PROGRESS |
| **D3** | Class 2b — constants-by-expression parity | derived Zig constants port as the *same expression*, not a folded literal; port markers clean | PROGRESS |
| **D4** | Class 3 — comptime→runtime VT-string corruption (`#30693`) | byte-exact golden VT bytes + OSC/ST-integrity tests (`d4_golden_vt_bytes.rs`) | PROGRESS |
| **D5** | Class 4 — dropped paired invariant (`#30678`) | the cross-site producer/consumer invariant catalog (`VT_INVARIANTS.md`) is complete and every anchor + seam test present | PROGRESS |
| **D6** | Class 5 — sanitizer blinded by flattened build switch (`#30875`) | an ASan-visibility **canary** proves the sanitizer lane can still SEE the port's heap, + cfg-mapping/suppression audit | PROGRESS |
| **D8** | clone-injection (perf/alloc) | advisory review list of unexplained copies; confirmed by the G8 bench lane at tier finales (always exit 0) | PROGRESS |
| **D9** | stale-comment contract drift | candidate stale/semantic-mismatch comments printed for review; sealed via `stale-comments-D9.tsv` (always exit 0) | PROGRESS |

**Why PROGRESS, not HARD** (even for the fail-closed ones D1/D2/D4/D6): they are
brand-new and fail-closed against **hand-maintained audit ledgers**. A legitimate
new hit that lands one commit *before* its ledger row is added would flip a HARD
gate RED and make CI transiently **un-passable** — exactly what R5 forbids.
PROGRESS lets them bed in. They are all **GREEN on today's tree**, and each
fail-closed detector ships a `--self-test` proving it flips RED on an injected
violation and GREEN once allowlisted. Note the ci.sh `run_gate` contract still
treats a PROGRESS gate's RED (exit non-0/non-3) as a `HARD_FAILURE`, so a genuine
regression is **never silently swallowed** — PROGRESS only changes the exit-0
bucket (progress line vs PASSED), it does not hide a real finding. Promote a
detector to HARD once it has ridden the port clean. D8/D9 are advisory by
construction (exit 0 + a review list); their binding enforcement is the review
step and, for D8, the G8 bench lane.

---

## Opt-in lanes (NOT part of the default run)

### `--webgl` — the WS14 wasm / WebGL2 browser lane

```sh
bash .port/tools/ci.sh --webgl
```

which runs, in order:

```sh
.port/tools/build-main-wasm.sh --release            # ~9 s cold, ~5 s warm
.port/tools/build-web-canvas-wasm.sh                # DEBUG, for the next two
node .port/tools/wasm-log-emission.mjs
node .port/tools/wasm-web-canvas-font.mjs
node .port/tools/wasm-vt-roundtrip.mjs              # example/wasm-vt's flow in Node
.port/tools/build-web-canvas-wasm.sh --release
node .port/tools/web-canvas-instantiate.mjs         # example/wasm-web-canvas, mock Canvas
node .port/tools/gates/wasm_examples_browser_gate.mjs   # example pages vs Ghostty's
node .port/tools/wasm-parity.mjs                    # rustty-vt.wasm vs ghostty-vt.wasm
.port/tools/gates/ws14_webgl_browser_gate.sh        # ~6 s (headless Chrome)
```

**The example pages against Ghostty's (2026-10-10).**
`wasm_examples_browser_gate.mjs` loads `example/wasm-vt`, `wasm-sgr` and
`wasm-key-encode` in headless Chrome twice: from this tree with `rustty-vt.wasm`,
and from the oracle at the pin with `ghostty-vt.wasm`, which it builds there the
way Ghostty's CI does (`zig build -Demit-lib-vt -Dtarget=wasm32-freestanding`;
only the gitignored `zig-out/` and `.zig-cache/` change). It drives the same
inputs into both (8 VT runs, 11 SGR sequences, 20 keys under 2 Kitty flag
settings and 3 actions: 139 outputs) and requires the `#output` text to match
after Ghostty's names map to Rustty's. Its manifest arm checks every constant
table the pages hard-code (SGR tags, 175 key codes, key actions) against the
module's own `type_json`; that arm found UD380, a wrong SGR tag table in
Ghostty's `wasm-sgr` page, so the oracle side is served with the same
correction. Controls: different VT inputs and different keys must give
different outputs (NC1, NC2), and the uncorrected table must fail the manifest
arm (NC3). It needs the oracle checkout and `zig`; without either it exits 3.

**What it proves.** The freestanding `rustty_wasm.wasm` cdylib
(`wasm32-unknown-unknown`, zero wasi imports, every import in `env`) is loaded in
a real headless Chrome, its `Atlas.Wasm` texture is uploaded as an `R8` NEAREST
WebGL2 texture, `readPixels` counts ink per 32 px cell column, and those counts
must **equal** an expectation computed at runtime on the CPU **from the same atlas
bytes the module holds** (read back through `rustty_wasm_atlas_data` — never a
hardcoded number, never the JS copy of what was written).

**Why it cannot go vacuously green.** Every invocation runs three arms and passes
only if the positive arm matches **and both negative controls fail**:

| arm | mutation | must |
| --- | --- | --- |
| positive | none | MATCH the CPU expectation |
| NC1 | atlas UVs offset by +3 px | FAIL |
| NC2 | fragment shader outputs black | FAIL |

plus NC3 (structural): the third, deliberately undrawn cell column must be
**exactly 0** ink; and five product-sensitivity assertions:

| # | asserts |
| --- | --- |
| P1 | each glyph actually left ink in the atlas (a no-op `set` cannot pass with `expect == [0,0]`) |
| P2 | the two reserved regions are disjoint (`reserve` did not hand out the same rect twice) |
| P3 | whole-atlas ink equals the sum of the per-region inks — `Atlas::set` wrote **nothing** outside the region it was given (wrong destination offset or row stride) |
| P4 | the two distinct bitmaps produce distinct counts (a `set` that ignores its region is caught) |
| P5 | `atlas_debug_canvas` — the **reference's own** readback path — agrees with the WebGL one: its canvas carries exactly `totalInk` alpha-channel ink (the grayscale arm writes A8 into `buf[(i*4)+3]`) |

P1–P5 are what make the gate sensitive to the **ported product code** rather than
only to its own injected mutations. Three product mutations were each demonstrated
RED, and GREEN again on revert:

| mutation (product code) | gate |
| --- | --- |
| `Atlas::set` `tex_offset` + 1 column | RED — P4 |
| `Atlas::set` `tex_offset` + 2 rows | RED — P3 |
| `atlas_debug_canvas_` grayscale writes `buf[(i*4)+2]` instead of `+3` | RED — P5 |

**Why it is opt-in, not default.** It needs a second full target's artifacts plus a
browser process, and two things a CI box may not have. Missing either is **exit 3
(BLOCKED)**, never a silent green:

- the `wasm32-unknown-unknown` rustup target → `rustup target add wasm32-unknown-unknown`
- a Chrome/Chromium binary (the driver probes the usual macOS/Linux paths)
- for the example-page and parity arms, the oracle checkout and `zig`

**Headless is not a weaker proxy.** `--headless=new --enable-unsafe-swiftshader`
was observed to produce **byte-identical** ink counts to a live, GPU-backed Chrome
on the same host (WS14 T9 design probe and WS14 T10's live run), so wiring the lane
headlessly in CI asserts the same pixels a human sees at
`example/wasm-webgl/index.html`.

The driver is Node-only with **zero npm dependencies** (node's built-in `WebSocket`
+ `fetch` speak CDP directly) and serves over loopback only
(`python3 -m http.server --bind 127.0.0.1`).

---

## Parallel, change-scoped and pruned runs

A full `ci.sh` run took one and a half to two days on the Mac, gate after gate.
These options cut the wall clock without weakening a gate (R5). The mechanics
are in `.port/tools/ci_sched.sh`, and their self-test is the G-CI-SELF gate
(`bash .port/tools/ci.sh --self-test`).

```sh
bash .port/tools/ci.sh                          # parallel (--jobs auto), every gate
bash .port/tools/ci.sh --jobs 1                 # the serial run, in registry order
bash .port/tools/ci.sh --changed-since <rev>    # fix verification (exit 5)
bash .port/tools/ci.sh --changed-since <rev> --print-scope   # the scope only
bash .port/tools/ci.sh --tier product           # only the product gates (exit 4)
bash .port/tools/ci.sh --tier parity            # only the parity gates (exit 4)
```

| flag | effect |
|---|---|
| `--jobs N\|auto` | At most N gates run at once. `auto` is half the cores, between 2 and 8, and 1 when `GITHUB_ACTIONS` is set (GitHub-hosted runs stay serial). `--jobs 1` is the old serial loop: gates run in registry order and their output streams. |
| `--heavy-cap N` | At most N heavy gates run at once (default 2, never more than `--jobs`). A heavy gate also waits while another one runs and the checkout's volume has less than `CI_MIN_FREE_GB` (15) free. |
| `--changed-since <rev>` | The long gates run only the crates changed since `<rev>` and their reverse dependents. Every other gate runs in full. |
| `--print-scope` | With `--changed-since`: print the scope and each long gate's decision, then exit 6. It runs no gate, so it is never a verdict. |
| `--prune-targets` | Deletes each per-gate target dir under `target/ci-targets/` once the last gate using it ends, if every gate that used it ended GREEN. Never `target/debug`. |
| `--tier product\|parity\|all` | Runs only that tier's gates (default `all`). See "Parity and product tiers" below. |
| `--sccache` | Sets `RUSTC_WRAPPER=sccache` when sccache is on PATH. Without sccache the run is unchanged. |

**What every run writes.** Each cargo step and gate prints its wall-clock
seconds. The summary lists the slowest. `target/ci-run/ci-timings.tsv` has one
row per cargo step, gate and Valgrind shard: gate id, kind, rc (or `skipped` /
`scoped-out`), seconds. Gate logs go to `target/ci-run/logs/<id>.log`. The next
run overwrites both.

### What stays the same (R5)

- A full run (no `--fast`, no `--changed-since`) runs every gate. Exit codes
  are read exactly as before: 0 PASS, 3 BLOCKED (WARNING), anything else RED.
  The summary lists PASSED, PROGRESS, WARNINGS and HARD FAILURES in registry
  order, whatever order the gates finished in.
- In a parallel run each gate's output goes to its own log and is printed whole
  when the gate ends, under the same `### gate` header, followed by its
  duration. Output from two gates never interleaves.
- Exit codes:

  | code | meaning |
  |---|---|
  | 0 | PASS. Only a full run can exit 0. |
  | 1 | FAIL |
  | 2 | usage error |
  | 4 | `--fast` or `--tier` subset green |
  | 5 | narrowed green |
  | 6 | `--print-scope` (ran no gate) |

  A run counts as narrowed when `--changed-since` is given (even if every
  crate was selected), or when a narrowing variable is exported into the run.
  The narrowing variables are `G6_ONLY`, `SAN_ONLY`, `G6T_SCOPE`, the
  `G6C_*` overrides, `SAN_MSAN_DIAG`, `SAN_MANIFEST_PATH`, `SAN_CLAIMS_SRC`,
  `SAN_EXPECTED_ABSENT` and `TGTDIR_ARMS`.

  The exit code follows the mode, not how many gates were classified as
  scoped. A narrowed gate that answers a plain BLOCKED still leaves the run
  at exit 5.

  Exits 4 and 5 print a NOT-A-RELEASE-GATE banner instead of
  `CI RESULT: PASS`, and name what was skipped or narrowed and why.
  `--fast` and `--changed-since` do not combine. `--tier` combines with
  either, and the run then exits 4.

### How gates are scheduled

Each registry entry declares what it needs:
`"<id>|<HARD|PROGRESS>|<light|heavy>|<traits>|<runner>"`. Nothing is inferred.
The traits always include exactly one `tier=parity` or `tier=product` (see
"Parity and product tiers" below).

- **heavy**: Miri, the sanitizers, Valgrind, release / clean-clone /
  feature-matrix builds, G4, G5 and G8. `--heavy-cap` bounds them.
- **`tgt`**: a lock held by every gate that builds in the default target dir,
  reads artifacts cargo uplifts there (`target/{debug,release}/librustty_vt_c.*`,
  xtask, examples), or writes untracked files into the tree (`dist/`,
  `zig-out/`, `target/fuzz-*`). Cargo's own lock keeps concurrent builds
  correct, but not a `librustty_vt_c` that another gate rebuilds with other
  features while this one loads it (G-VTCLIB's publish rule). So the `tgt`
  gates run one at a time, in registry order, beside everything else.
- **`dir=<name>`**: in a parallel run the gate gets its own
  `CARGO_TARGET_DIR=target/ci-targets/<name>`. G6, G6c and G6t each have one,
  so the three Miri lanes can run side by side (G6c and G6t's debug-assertions-off
  half build with other RUSTFLAGS). G-VALGRIND has one so its hours of
  memcheck never wait for the `tgt` lock. Gates naming the same dir never
  overlap. In `--jobs 1` runs every gate uses the default dirs, as before.
- **`after=<id>`**: the gate starts only once `<id>` has finished.
- **`exclusive`**: the gate starts only once every other gate has finished,
  and nothing starts beside it. G8b (PERF-FENCE) and then G8 (ARMED) run this
  way, last and alone, so the perf fence measures a quiet machine.
  - Every registry kind is dispatched, in the order HARD, PROGRESS,
    PERF-FENCE, ARMED. An unknown kind is a usage error.
  - G8b BLOCKs (exit 3) through its dedicated pre-measurement load
    check: a bounded cooldown (`G8B_COOLDOWN_S`, default 600 s), run before
    its builds and again before measuring. It also BLOCKs, after measuring
    Fence 1 and writing the metrics, when Fence 2 has no entry binary (or
    none with a manifest) to compare against.
  - A harness crash, a usage error, a provisional measurement or a fence RED
    stay RED. G8b-SELF proves this.
  - G-REL-LAT (HARD) runs this way too, before G8b. It is
    `check_release_lane.sh --exclusive`: the `EXCLUSIVE_TESTS` rows that
    G-REL's main pass `--skip`s. Each row is a test whose verdict is a
    wall-clock budget, with its reason. Today the one row is rustty-termio's
    `readthread_pipeline_keeps_interactive_latency`, a 300 µs median in
    release only. In Phase 8 it went RED at 10 ms under full parallel load
    (N549) and passed 10/10 on a quiet machine.
  - G-REL-LAT follows G8b's load check. It builds first, then waits for the
    1-minute load to fall to `ws10_suite.py`'s `IDLE_MAX` within
    `REL_LAT_COOLDOWN_S` (default 600 s). If it does not, the gate exits 3
    BLOCKED, a WARNING with its reason, having run nothing.
  - On a quiet machine each row must run exactly one test and pass. A
    failure is RED, and so is a row that matches no test (stale). Off macOS
    the gate is BLOCKED, because the test is macOS-only.
  - G-REL's self-test proves the skip, the load-check BLOCKED, the stale
    row and the reasonless row, with controls. G-CI-SELF arm X proves that
    the real entry runs beside nothing.
- **`shard`**: in a parallel run G-VALGRIND runs as `CI_VALGRIND_SHARDS` (4)
  `check_valgrind.sh --shard K/N` units. Each unit is heavy, so the heavy cap
  bounds them, and each passes `--jobs $CI_VALGRIND_JOBS`. That defaults to
  the core count divided by the heavy cap (at least 1), because one memcheck
  process is one thread; set it to override. If
  any shard is RED the gate is RED. If any shard is BLOCKED the gate is
  BLOCKED. Only all-green shards make a green gate.
- **`scope=<how>`**: what `--changed-since` does to the gate (below).

### Parity and product tiers

Every registry entry carries exactly one tier trait:

- **`tier=parity`**: the gate exists because Rustty tracks the Ghostty
  oracle. It needs the oracle checkout, the pin or the oracle's artifacts,
  or it checks the port's records and coverage. Examples: G1 to G3b, G9,
  G-PARITY, G-FILES, G-SYMS, G-TESTS, G-CENSUS, G-UDDOCS, G-LOGFILE, G4,
  G4-uni, G-ESCTEST, G-REFGEOM, G-EOL, G8b and G8, and the D1 to D10 port
  audits.
- **`tier=product`**: the CI the project would have without the port. That
  covers fmt, clippy, the workspace build and tests, the release lane, the
  sanitizers, Miri, Valgrind, fuzz smoke, cross-target and feature builds,
  the linters, the examples, and the C-ABI and artifact checks.

A `-SELF` entry takes the tier of the gate it tests. The cargo steps are all
product.

ci.sh checks the registry before the cargo steps. An entry with no tier, two
tiers or an unknown value is a usage error (exit 2), and nothing runs.

`--tier product` or `--tier parity` runs only that tier. A tier run is a
subset, like `--fast`:

- every gate and cargo step of the other tier is listed as `SKIPPED by
  --tier`;
- the run exits 4 with a TIER-SUBSET banner, never 0, and never prints
  `CI RESULT: PASS`.

G-CI-SELF's T arms prove all of this, each with a negative control:

- every real entry has exactly one valid tier;
- a missing, doubled or unknown tier is refused;
- the real registry's plan under each tier holds only that tier;
- fixture runs under `--tier product` and `--tier parity` start no gate of the
  other tier and exit 4.

### `--changed-since <rev>`: fix verification

The changed paths are `git diff --no-renames --name-only <rev>` plus untracked
files. The diff is against the working tree, so uncommitted edits count. A
rename lists both its source and its destination, and deletions are listed. A path under `crates/<c>/`
or `xtask/` selects that crate, whatever its file type. The selected crates are
closed over reverse dependencies of every kind (normal, build, dev), using
`cargo metadata`. When paths are not crate paths, the rules are conservative:

- `.port/tools/**`, `.port/phase-2/**`, `crates-sys/**`, any `build.rs`,
  `Cargo.lock`, the root `Cargo.toml`, `rust-toolchain*`, `.cargo/**` and
  `valgrind.supp` select **every** crate. That is a full run (exit 0).
- Other `.port/` paths select the crates whose files name them. A `git grep`
  of `crates/` and `xtask/` finds the names. For example, rustty-vt's
  `fuzz_corpus_replay` reads `.port/phase-0/corpora/cvt`, so a seed change
  selects rustty-vt and its dependents.
  - A `.port/` record (`.md`, `.txt` or `.tsv`) counts only when a crate names
    it on a code line. Comments and `ignore = "..."` reasons only cite records.
  - Other `.port/` data that no crate names selects every crate.
- Docs and records select **nothing**: `docs/`, `.superpowers/`, the `.port/`
  records above, and `*.md` and `*.txt` elsewhere outside crate dirs. Every
  long gate is scoped out.
- Any other path selects every crate.

The long gates take the scope through their own selectors:

| gate | selector |
|---|---|
| G6, G6t | `check_leaks.sh` `G6_ONLY="<crates>"` |
| G6c | runs in full when `rustty-vt` is in scope, else scoped out |
| G-ASAN, G-LSAN, G-MSAN, G-HWASAN, G-TSAN | `sanitizers.sh` `SAN_ONLY="<crates>"` |
| G-LSAN-GTK | runs in full when `rustty-apprt` is in scope, else scoped out |
| G-VALGRIND | `check_valgrind.sh <crate entries>`: its `--list-scope` rows whose crate is in scope |

A narrowed gate never reports GREEN. `check_leaks.sh` and `sanitizers.sh`
answer exit 3 with a `SCOPED-GREEN` line when what they ran is clean. ci.sh
lists a narrowed gate that passed under "SCOPED", never under PASSED. A
narrowed gate that is RED fails the run, and a plain BLOCKED is still a
WARNING.

### Gates and a non-default `CARGO_TARGET_DIR`

Gates must not assume `<repo>/target`. A gate that reads a built artifact
sources `.port/tools/gates/_target_dir.sh` and calls `rustty_target_dir
"$ROOT"`, which returns `cargo metadata`'s `target_directory`, so
`CARGO_TARGET_DIR` and any `build.target-dir` config are honoured. D11r, A5,
G-ABISCHEMA, G-VTFEAT and the `example/*/build.sh` scripts read their
library this way. xtask resolves the archives it folds in under
`<root>/target`, so G-XCF and G-EXAMPLES' xcframework step run xtask under
`env -u CARGO_TARGET_DIR`, as G8b already does.

G-LINT skips only real cargo target roots, each anchored at the checkout:
`target/`, `target-*/`, any dir holding cargo's `CACHEDIR.TAG`, and an in-tree
`CARGO_TARGET_DIR`. A source dir that is merely named `target` is still
linted.

G-TGTDIR (`check_target_dir.sh`) holds these rules in place:

- It statically scans every registered runner and example script for literal
  `target/<profile>` artifact reads. Rows in its `ALLOWED` list are exempt,
  each with a reason, and a stale row is RED.
- It runs G-LINT in a fixture checkout, with bad files placed in target roots.
- It runs D11r, A5, G-ABISCHEMA and `example/c-vt` with `CARGO_TARGET_DIR`
  set somewhere else.

Each arm has a negative control.

### Policy

- **Phase end: one full run** (`bash .port/tools/ci.sh`, no `--fast`, no
  `--changed-since`). Only its exit 0 is a release verdict.
- **Fix verification: `--changed-since <rev>`**, where `<rev>` is the commit
  the fix started from. It runs every short gate and scopes the long ones to
  the fix. Do not re-run the whole suite because one crate changed.
- **Working lanes: `--fast`**, as before.
- **Low disk**: `--prune-targets`, and `--heavy-cap 1` when two sanitizer
  passes (tens of GB each) do not fit side by side.

### `--prune-targets`: what it may delete

`ci.sh` deletes only `target/ci-targets/<name>` dirs. `<name>` must be a plain
word, the target root must be `<checkout>/target/ci-targets`, and the dir must
hold the `.created-by-ci-sh` marker that ci.sh writes when it creates the dir.
A dir without the marker is refused by name. This is the only deletion
`ci.sh` makes.

A dir is deleted when the last gate that uses it ends, and only if every gate
that used it ended GREEN, meaning rc 0 on all of its units. A gate that ended
RED, was killed (rc 128 or more) or ended BLOCKED keeps its dir. ci.sh prints
`kept <dir>: <gate> ended RED (exit N)`, `was killed` or `ended BLOCKED`. That
gate is the one about to be rerun, and the rerun reuses its build. In Phase 8,
killing G-VALGRIND's shards ended the gate and pruned `ci-targets/valgrind`,
so the rerun rebuilt it from nothing. G-CI-SELF arm H proves the rule with a
RED, a self-killed (SIGTERM, 143), a BLOCKED and a RED-plus-GREEN shared dir.
Its control is a prune that ignores the verdict.

**`target/debug` is never pruned.** The developer's IDE shares it
(rust-analyzer's flycheck builds there), so deleting it would cost every
editor session a cold rebuild. `--prune-targets` touches only
`target/ci-targets/`.

**`target/san-<san>`** is deleted by `sanitizers.sh` itself on every way out
of a pass:

- the end of the crate loop, whether the crates were GREEN or RED;
- the disk-floor BLOCKED stop;
- an INT, TERM or HUP. A trap deletes the dir being built in, the shared
  `san-<san>` or the current `san-<san>-<crate>`, then exits 128 plus the
  signal number.

Before CI hygiene B an interrupted pass left its dir behind. A SIGKILL cannot
be trapped. In that case the next pass's opening `drop_target` clears the
dir. G-SAN-SELF arm D2 covers all four paths with a stub `run_crate`. Its
control installs no trap, and the SIGTERM then leaves the dir behind.

---

## Why this is honest (no fabricated greens)

- Hard-fail gates that pass today (`fmt`, `clippy`, `build`, `test`, G7, G4
  control) pass on **real output**, not by relaxing a check.
- PROGRESS gates do **not** print a fake GREEN. They print a progress count;
  the summary lists them under "PROGRESS — not failing CI", distinct from PASSED
  and from HARD FAILURES. They go RED (and fail CI) the moment a *claimed-done*
  crate is short.
- A BLOCKED gate is surfaced as a WARNING, never swallowed.
- Nothing here links the ghostty tree as a Cargo dep (R3); only the differential
  **harness** links the reference `libghostty-vt.a` as an external oracle, and
  only G4 consumes it.

---

## Adding / wiring a gate

The gate suite is registered in `ci.sh`'s `GATE_REGISTRY` array as
`"<DoP_id>|<HARD|PROGRESS>|<light|heavy>|<traits>|<runner command>"`. To add a
gate, put its script in `.port/tools/gates/` and make it obey the 0 / 3 /
non-zero exit contract above. Then add one registry line that declares its
resource and traits (see "How gates are scheduled"): `heavy` if it is long or
fills the machine, `tgt` if it builds in, or reads from, the default target
dir or writes untracked files into the tree, and always its `tier=parity` or
`tier=product`. Read built
artifacts through `_target_dir.sh`, never through a literal `target/` path
(G-TGTDIR enforces this). ci.sh dispatches HARD gates first, then PROGRESS
gates, and aggregates all results before deciding the exit code.
