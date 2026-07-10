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
4. `cargo test --workspace`
5. the parity gates (hard-fail gates first, then progress)

CI exits **non-zero iff any *non-progress* check is red.**

`CI_SKIP_CARGO=1 bash .port/tools/ci.sh` runs the gates only (no cargo) for fast
local iteration.

---

## The canonical gate suite (`.port/tools/gates/`)

| DoP id | Script ci.sh runs | Helper | Class |
|---|---|---|---|
| **G7** | `check_acyclic.sh` | `cargo_cycle_check.py` | **HARD-FAIL** |
| **G4** | `run_differential.sh` | harness `gate_empty_divergence.sh`, `comparator.py` | **HARD-FAIL** |
| **G1** | `check_tests_ported.py` | `_gatelib.py` | PROGRESS* |
| **G1b** | `check_test_counts.py` (oracle-probed by ci.sh + self-guarded: missing zig ref ⇒ exit 3, never a vacuous green) | ghostty oracle checkout (R3, local only) | PROGRESS* |
| **G2** | `check_assertions.py` | `_gatelib.py` | PROGRESS* |
| **G3** | `check_comments_fidelity.py` | `_gatelib.py` | PROGRESS* |
| **G9** | `check_parity_surface.py` | `_gatelib.py` | PROGRESS* |
| **G8** | `check_baseline_regression.py` | `_gatelib.py` | PROGRESS* |
| **G5** | `run_fuzz_smoke.sh` | harness `FUZZ-INVENTORY.md`, `comparator.py` | PROGRESS* |
| **G6** | `check_leaks.sh` | Miri (pure) / ASan (FFI + live-syscall tests), `_gatelib` done-detection; missing miri ⇒ exit 3 BLOCKED | PROGRESS* |

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
| `3` | BLOCKED — a contract/tooling input is missing | **WARNING** (surfaced, never a silent green) |
| other non-zero | RED | HARD gate ⇒ fails CI; PROGRESS gate ⇒ a *claimed-done crate regressed* ⇒ fails CI (R5) |

---

## HARD-FAIL checks (red ⇒ CI fails, from day one)

These must be green right now and at every commit; they don't depend on any
crate being ported.

| Check | What it enforces | Why it's binding today |
|---|---|---|
| `cargo fmt --check` | formatting | scaffold + every future file must be `rustfmt`-clean |
| `cargo clippy --workspace -D warnings` | lint cleanliness | warnings are errors; the scaffold compiles clean and must stay so |
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
the ghostty oracle checkout is absent on CI ⇒ probe exits 3 BLOCKED.)

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
| `check_tests_ported.py` | G1 | **3234** test blocks, count == contract, names match | per-crate `0/contract`, workspace `0/3225` ported | a crate is `done` (then per-crate ported == contract, names match) |
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
