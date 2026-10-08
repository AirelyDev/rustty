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
is an expected warning on a lane (G4, G-EOL, D10, G-LOGFILE, G-REFGEOM on CI) can
therefore not hide a new, different reason behind the same gate id: read the
reason, not just the id.

For the oracle gates, exit 3 means the oracle is **absent, off the pin
(`.port/tools/gates/ORACLE_PIN`), or dirty**, or the pin file itself is missing
or malformed: G-EOL arm 3, G4's staleness guard, G-LOGFILE and G-REFGEOM need the
oracle's HEAD to equal `ORACLE_PIN` (G-REFGEOM is also BLOCKED without Zig); D10 needs the oracle to have that commit (it reads
`src/stb` at the pin, whatever HEAD is). See `.port/ORACLE_SYNC.md`.

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
G-LOGFILE and G-REFGEOM are BLOCKED on CI for the same no-oracle reason; `ci.yml`'s policy
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

which is exactly these two commands, in order:

```sh
.port/tools/build-main-wasm.sh --release            # ~9 s cold, ~5 s warm
.port/tools/gates/ws14_webgl_browser_gate.sh        # ~6 s (headless Chrome)
```

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

**Headless is not a weaker proxy.** `--headless=new --enable-unsafe-swiftshader`
was observed to produce **byte-identical** ink counts to a live, GPU-backed Chrome
on the same host (WS14 T9 design probe and WS14 T10's live run), so wiring the lane
headlessly in CI asserts the same pixels a human sees at
`example/wasm-webgl/index.html`.

The driver is Node-only with **zero npm dependencies** (node's built-in `WebSocket`
+ `fetch` speak CDP directly) and serves over loopback only
(`python3 -m http.server --bind 127.0.0.1`).

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
`"<DoP_id>|<HARD|PROGRESS>|<runner command>"`. To add a gate, drop its script in
`.port/tools/gates/`, make it obey the 0 / 3 / non-zero exit contract above, and
add one registry line. ci.sh runs HARD gates first, then PROGRESS gates, and
aggregates all results before deciding the exit code.
