# rustty Port Realignment & Validation Hardening Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Large multi-agent tasks are executed as `.port/phase-2/wf-*.js` Workflow scripts per project doctrine (memory: workflow-scripts-in-port); their task entries specify the script's contract, not per-agent steps.

**Goal:** Make the port's verification layer binding (it currently is not), fix the confirmed soundness/semantics defects, realign the drifted ledgers, adopt the ratified Bun-port practices (LIFETIMES.tsv, detectors D1–D9, unsafe accounting, upstream parity), and codify the pre-port process for the remaining tiers.

**Architecture:** Five sequenced phases. Phase 0 makes the gates honest (nothing else can be trusted until then); Phase 1 fixes confirmed code defects; Phase 2 realigns ledgers/status headers and flips completed crates to `done` so the contract gates finally grade them; Phase 3 lands the Bun adoptions; Phase 4 is validation sweep #2 aimed at sweep #1's declared blind spots; Phase 5 codifies the setup for code not yet ported (gtk, swift-app glue, inspector widgets).

**Tech Stack:** Rust workspace (39 crates), Python gate scripts (`.port/tools/gates/`), bash `ci.sh`, Miri/ASan, the ghostty oracle checkout at `/Users/gael/projects/ghostty` @ `4749c4e9`, and `.port/phase-2/wf-*.js` Workflow orchestration.

## Evidence base (read these before executing any task)

- `.port/phase-2/VALIDATION_REPORT.md` — sweep #1: findings B1–B2, H1–H7, M1–M8, §3 ledger, §4 blind spots, §5 refuted appendix (do NOT re-raise).
- `.port/phase-2/BUN_PORT_LEARNINGS.md` — learnings catalog, detector spec D1–D9 (§4), PROPOSED R7–R12 (§5).
- `/Users/gael/.claude/jobs/22556231/tmp/ci-run-2026-07-10.log` — live ci.sh run; termio test-binary fork deadlock evidence.

## Global Constraints

- **R1** 100% fidelity: every line/comment/test/assertion ported; contracts 3234 test blocks / 11975 assertions / 42069 comment lines / 677 parity ids (`.port/PORT_RULES.md`).
- **R3** ghostty checkout is READ-ONLY, used only as external oracle; nothing in-repo may reference its path at build/run time.
- **R5** gates only ever tighten. Every gate fix in this plan is a matcher/wiring correction or an addition — if a task appears to loosen a gate, STOP and surface it.
- **R6** pure-Rust → Miri, FFI → ASan; Miri exceptions only per `MIRI_EXCEPTIONS.md` 4-criterion rule.
- Every SB/TB fix must be PROVEN on an isolated Miri harness (mirihar pattern), never hand-reasoned (memory: shaper-hook-sb-clean-interleave).
- Workflow scripts live at `.port/phase-2/wf-*.js`; agents in them follow the git discipline in R11 (no reset/checkout/stash; commit explicit paths; orchestrator pushes).
- Commit after every green task. Do not batch phases into one commit.

## Decision gates (user forks — resolve before dependent tasks)

| #   | Fork                                                                                                                                                                                                                                | Recommendation                                                                                                                        | Blocks      |
| --- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------- | ----------- |
| DG1 | Ratify PROPOSED R7–R12 into PORT_RULES.md (as-is / amended / dropped per rule)                                                                                                                                                      | Ratify all six; they only tighten                                                                                                     | Tasks 14–22 |
| DG2 | Release profile semantics: ghostty's tagged artifacts are ReleaseFast (overflow = UB), dev builds ReleaseSafe (trap). Options: (a) `overflow-checks=true` in release + a `release-fast` bench profile; (b) status quo (silent wrap) | (a) — strictly safer than the mirrored artifact, one line, restores trap-parity for Kitty multiply etc. Record the fork in PORT_RULES | Task 10     |
| DG3 | G1 `--strict` finale semantics: exact `ported == contract` is unsatisfiable (sanctioned extra tests). Options: (a) `ported >= contract` AND empty MISSING; (b) tag rustty-only tests `// rustty-only` and subtract                  | (a) now, (b) later if EXTRA noise ever hides a real miss                                                                              | Task 4      |
| DG4 | ReleaseSafe-active Zig asserts ported as `debug_assert!` (109 vt sites): (a) add a `release-safe` profile (`debug-assertions = true`) used by the differential harness + CI test lane; (b) flip the 109 sites to `assert!`          | (a) — zero code churn, matches the oracle's build mode at the harness boundary                                                        | Task 10     |

---

## Phase 0 — Make the gates honest (do first; everything downstream depends on it)

### Task 1: bench `ref_all_decls` contract test (fixes B2-genuine + H3)

**Files:**

- Modify: `crates/rustty-bench/src/lib.rs` (header lines ~14–26 + new test)
- Modify: `DEFERRALS.md:3288` (the documented-drop entry)

**Interfaces:**

- Produces: `rustty-bench` test count 8/8 vs contract; G1 bench row GREEN.

- [ ] **Step 1: Read the workspace convention** — `grep -rn "fn ref_all_decls" crates/rustty-crash/src crates/rustty-zf/src | head -3` and open one; the port already has 30 of these (VALIDATION_REPORT H3).
- [ ] **Step 2: Add the test to `crates/rustty-bench/src/lib.rs`** following that exact convention, porting `benchmark/main.zig:12`'s `test { @import("std").testing.refAllDecls(@This()); }`:

```rust
// test { @import("std").testing.refAllDecls(@This()); }
#[cfg(test)]
mod tests {
    /// Ports main.zig's anonymous refAllDecls block: compile-touch every pub
    /// decl of this hub module (workspace ref_all_decls convention).
    #[test]
    fn ref_all_decls() {
        use crate as root;
        let _ = core::any::type_name::<root::benchmark::Benchmark>();
        let _ = core::any::type_name::<root::capi::CApi>();
        // ... one line per pub mod's principal type: cli, terminal_stream,
        // codepoint_width, grapheme_break, screen_clone, terminal_parser,
        // options, main-equivalents — mirror the names actually exported;
        // copy the pattern used in rustty-crash's ref_all_decls.
    }
}
```

- [ ] **Step 3: Rewrite the header rationale** — replace the `is DROPPED — a Zig reflection idiom with no Rust analogue` block (lib.rs lines ~16–23) with: `main.zig's refAllDecls test{} ports as tests::ref_all_decls (workspace convention). Test contract: 8 Zig test{} blocks -> 8 #[test] fns.` Delete the `rust_test_count = zig_test_count - 1` sentence.
- [ ] **Step 4: Update `DEFERRALS.md:3288`** — mark the drop entry RESOLVED with today's date and the new test path.
- [ ] **Step 5: Verify** — `cargo test -p rustty-bench 2>&1 | grep "test result"` → 8 passed. Then `python3 .port/tools/gates/check_tests_ported.py --crate bench` → 8/8 GREEN.
- [ ] **Step 6: Commit** — `git add crates/rustty-bench/src/lib.rs DEFERRALS.md && git commit -m "rustty-bench: port main.zig refAllDecls as ref_all_decls #[test] (G1 8/8, R1)"`

### Task 2: G1/G9 name matcher — generate the port's slug convention (fixes H2 + H4)

**Files:**

- Modify: `.port/tools/gates/_gatelib.py:325-355` (`name_candidates`)
- Modify: `.port/tools/gates/check_parity_surface.py` (`id_variants` — same flaw)
- Modify: `crates/rustty-extra/src/...` (rename `get_description_test` → `get_description`, per H4)

**Interfaces:**

- Produces: `name_candidates(name)` additionally yields the composed port slug (non-alnum→split, camel-split, lowercase, `_`-join). All other candidate forms unchanged (tightening-safe: counts still enforced; this only fixes false MISSING).

- [ ] **Step 1: Write failing check** — `python3 -c "import sys; sys.path.insert(0,'.port/tools/gates'); from _gatelib import name_candidates; assert 'dynamic_palette_change_default_preserves_changes' in name_candidates('DynamicPalette: changeDefault preserves changes'), 'composed slug missing'"` → AssertionError (current behavior).
- [ ] **Step 2: Add the composed form** at the end of `name_candidates` (before the final return):

```python
    # Composed port-slug form (the port's actual uniform convention):
    # split on every non-alphanumeric run, camel-split each token, lowercase,
    # join with '_'.  'DynamicPalette: changeDefault preserves changes'
    #   -> dynamic_palette_change_default_preserves_changes
    # 'bash: ENV' -> bash_env ; 'parseIntoField: bool' -> parse_into_field_bool
    words = []
    for tok in re.split(r"[^A-Za-z0-9]+", name):
        if not tok:
            continue
        words.extend(
            w for w in re.split(r"(?<=[a-z0-9])(?=[A-Z])|(?<=[A-Z])(?=[A-Z][a-z])|(?<=[A-Za-z])(?=[0-9])", tok) if w
        )
    if words:
        out.add("_".join(w.lower() for w in words))
```

- [ ] **Step 3: Re-run Step 1's check** → passes. Also assert `'bash_env' in name_candidates('bash: ENV')` and `'parse_into_field_bool' in name_candidates('parseIntoField: bool')`.
- [ ] **Step 4: Apply the identical transform to `check_parity_surface.py`'s `id_variants`** (H4 names it as having the same flaw) — import from `_gatelib` rather than duplicating if feasible.
- [ ] **Step 5: Rename the misnamed test** — in rustty-extra, `get_description_test` → `get_description` (find with `grep -rn "fn get_description_test" crates/rustty-extra/src`).
- [ ] **Step 6: Measure the effect** — `for c in args extra termio vt font config input; do python3 .port/tools/gates/check_tests_ported.py --crate $c; done`. Expected: args/extra/termio flip GREEN; vt's simulated done-flip false-MISSING count drops from ~1,170 to ~0 (any residue is a REAL R1 gap — file it, don't suppress it).
- [ ] **Step 7: Run the gate self-test** — `python3 .port/tools/gates/check_tests_ported.py --self-test` must still flip RED on injection (R5 self-test contract).
- [ ] **Step 8: Commit** — `git commit -m "gates: name_candidates learns the composed port slug; id_variants aligned; G1 false-MISSING eliminated (H2/H4)"`

### Task 3: G2 assertion gate — real per-test comparison, undiluted counting (fixes H6 + M7)

**Files:**

- Modify: `.port/tools/gates/check_assertions.py:124` (per-test resolution)
- Modify: `.port/tools/gates/_gatelib.py:358-373` (`ASSERTION_PATTERNS`)

**Interfaces:**

- Consumes: Task 2's fixed `name_candidates`.
- Produces: per-test WEAKENED check resolves ref string-name → ported fn via `name_candidates` (today only 43/3,065 compare); Rust assertion counting drops `.expect(`, `.unwrap()`, `?` from the parity count (keeps `unwrap_err`, `should_panic`, plus a per-test baseline equal to the Zig body's non-testing `try` count from `tests.jsonl`, per M7's alternative if pure removal floors any crate below contract).

- [ ] **Step 1: Baseline** — `python3 .port/tools/gates/check_assertions.py` and record current totals.
- [ ] **Step 2: Fix per-test resolution** — at check_assertions.py:124 replace raw-name equality with candidate-set intersection: build `cand = name_candidates(ref_name)` and match any ported fn whose name is in `cand`. Report the number of compared pairs — must jump from 43 to >2,900.
- [ ] **Step 3: Tighten the Rust-side count** — remove `\.expect\s*\(` and `\.unwrap\s*\(\s*\)` and the `QMARK_RE` contribution from the parity count; add `\.expect_err\s*\(`. Re-run. If any crate's total now undershoots its floor, implement M7's subtraction variant instead (baseline = Zig body's non-`testing.` `try` count, available in `tests.jsonl`) — do NOT lower any floor (R5).
- [ ] **Step 4: Self-test** — `--self-test` still flips RED on an injected assertion deletion; add a new injection case: delete one assertion from one test with the crate total held constant (pad another test) — the per-test check must now catch it (this was H6's escape).
- [ ] **Step 5: Commit** — `git commit -m "gates: G2 per-test comparison binds via name_candidates; assertion count no longer inflated by unwrap/expect/? (H6/M7)"`

### Task 4: Done-detection unification + `--strict` semantics (fixes H5, DG3)

**Files:**

- Modify: `.port/tools/gates/check_leaks.sh:71-75`, `.port/tools/gates/check_comment_coverage.py:142-145`
- Modify: `.port/tools/gates/check_tests_ported.py` (strict mode per DG3)
- Modify: `.port/phase-0/plan/DEFINITION_OF_PARITY.md` (record DG3), `.port/tools/gates/README` (drift, §3a)

- [ ] **Step 1: Route both scripts through `_gatelib.is_claimed_done`** — check_comment_coverage.py: one-line import per H5; check_leaks.sh: call a tiny `python3 -c` shim against `_gatelib` instead of `grep 'done'` (statuses done|complete|parity|ported|final all count, matching `_gatelib`).
- [ ] **Step 2: Route rustty-wuffs to ASan** — it wraps C FFI but is absent from `FFI_CRATES` (H5): add it; also make the enumerator visit `crates-sys/*/Cargo.toml` or delete the unreachable sys entries with a comment saying why.
- [ ] **Step 3: Implement DG3(a)** — `--strict` = `ported >= contract AND MISSING == ∅` (EXTRA reported, not fatal). Update DoP §G1 text in the same commit. Run `--self-test`.
- [ ] **Step 4: Fix gate README drift** (§3a: dead check_leaks dry-mode text, stale 3225-vs-3227 contract note, stale crate/edge counts) and the two low-level nits if trivial (PORT_STATUS_RE multi-word truncation; 8KB header read cap).
- [ ] **Step 5: Fix the LIVE G6 RED (new finding, ci-run-2026-07-10.log:2160)** — `rustty-os`'s integration test binary `--test cf_release_live` (`cf_release_thread_releases_on_real_xev_loop`) calls real `kqueue`, which Miri cannot execute → G6 currently RED on a claimed-done crate. Fix the ROUTING, not the test: G6's Miri lane runs `cargo miri test -p rustty-os --lib` (unit surface) and routes live-FFI test binaries to the ASan lane; record the split in `MIRI_EXCEPTIONS.md` with the 4-criterion justification (the test itself keeps running under plain `cargo test` + ASan — R5: nothing skipped, only routed to the right tool per R6).
- [ ] **Step 6: Verify** — `bash .port/tools/gates/check_leaks.sh --list` (or its enumeration mode) now lists args/termio/wuffs; `python3 .port/tools/gates/check_comment_coverage.py` visits them; `bash .port/tools/gates/check_leaks.sh` (G6) exits 0.
- [ ] **Step 7: Commit** — `git commit -m "gates: unified is_claimed_done across G3b/G6, wuffs->ASan, os live-kqueue test routed Miri->ASan, strict = >=contract+empty-MISSING (H5, DG3)"`

### Task 5: Arm G4 — the real byte-exact differential gate (fixes B1; the port's single biggest un-run check)

**Files:**

- Modify: `.port/tools/gates/run_differential.sh:57-110`
- Modify: `.port/phase-0/harness/build_port_driver.sh` (wire librustty_vt_c staticlib → `harness/bin/vt_driver_port`)
- Modify: `.port/tools/ci.sh:45-46`, `.port/tools/CI.md` (truthful G4 description)

**Interfaces:**

- Produces: `run_differential.sh` auto-arms when `harness/bin/vt_driver_port` (or `$RUSTTY_VT_DRIVER`) exists; drives `comparator.py` with its REAL CLI (`comparator.py <input> --ref <golden> --port <port-output> ...` — read the argparse in `harness/comparator.py` first and use exactly that); a present-but-diverging port is RED (exit 1), never WARNING; `gate_empty_divergence.sh` runs only on the truly-empty branch.

- [ ] **Step 1: Build the port driver** — `bash .port/phase-0/harness/build_port_driver.sh` (target/debug/librustty_vt_c.a already exports the 162 `rustty_*` symbols it needs, per B1). If the script needs the release lib: `cargo build -p rustty-vt-c --release` first. Deliverable: executable `harness/bin/vt_driver_port`.
- [ ] **Step 2: Read `harness/comparator.py`'s argparse** and rewrite the run_differential.sh real-arm loop (lines 88–106): key on `PORT_DRIVER="${RUSTTY_VT_DRIVER:-$HARNESS/bin/vt_driver_port}"`, generate port output per fixture, invoke comparator with its real flags, do NOT mask stderr (`2>&1 | sed 's/^/    /'` on failure), exit 1 on any diff.
- [ ] **Step 3: Move `gate_empty_divergence.sh`** to the `[ ! -x "$PORT_DRIVER" ]` branch only (its assert-100%-divergence contract is only meaningful against an empty port).
- [ ] **Step 4: Extend the corpus loop** to the 21 c-vt golden cases (`$CORPUS/*.golden`) AND `harness/fixtures/*.vt` if the fixture set is distinct (B1 fix text names both).
- [ ] **Step 5: Add a flip self-test** (R5): `--self-test` gains an injection mode — corrupt one byte of a copied port output, assert the gate exits 1; restore, assert exit 0.
- [ ] **Step 6: Run it for real** — `bash .port/tools/gates/run_differential.sh`. **Expected: unknown — this is the first honest run.** Divergences are REAL FINDINGS: file each as a fidelity bug (do not "fix" the gate); a clean run is the first true byte-parity evidence for the VT core.
- [ ] **Step 7: Truth in docs** — fix ci.sh:45-46 + CI.md ("auto-flips to real byte-exact gate" is now true; describe the arming key). Commit: `git commit -m "gates: arm G4 for real — vt_driver_port drives comparator over the corpus; diverging port is RED (B1)"`

### Task 6: CI workflow can actually pass (fixes H7)

**Files:**

- Modify: `.github/workflows/ci.yml:44-50`
- Modify: `.port/tools/gates/check_leaks.sh:29-31` (require_miri exit code)

- [ ] **Step 1:** Install `nightly` + `miri` component and the pinned zig (0.15.2) in ci.yml on both legs.
- [ ] **Step 2:** Make `require_miri` exit 3 (BLOCKED) matching CI.md's documented semantics — or install miri unconditionally and keep exit 2; pick ONE and align script+doc.
- [ ] **Step 3:** Decide the vt_driver_ref policy honestly per R3 (oracle stays off CI): document G4 as **local-only** in CI.md and make ci.yml skip it with an explicit `G4: local-only (R3 — oracle not on CI)` log line, not a fake HARD claim. Remove the phantom "exit 78" comment.
- [ ] **Step 4:** Push to a branch, confirm the workflow goes green end-to-end (with G1 now green from Tasks 1–2). Commit + merge.

### Task 7: Register `check_test_counts.py` in ci.sh (§3a — the only fidelity net for 11 non-contract crates)

**Files:**

- Modify: `.port/tools/ci.sh` (new gate line), `.port/tools/CI.md`

- [ ] **Step 1:** Add it after G1, gated on an oracle-checkout probe: absent (CI) → exit 3 BLOCKED per R3; present (local) → binding.
- [ ] **Step 2:** `bash .port/tools/ci.sh 2>&1 | grep -A2 "test_counts"` → GREEN locally. Commit.

### Task 8: termio test-binary fork deadlock (live finding from ci-run-2026-07-10.log)

**Files:**

- Investigate: `crates/rustty-termio/src/command.rs` (`command_rt_pre_exec_1`), `src/exec.rs` (`subprocess_start_spawns_and_reaps`, `threadenter_queuewrite_writes_to_pty`)

Evidence: under system load, the test binary hung >29 min with a forked child at 0.00s CPU (PID snapshot in the CI log analysis) — the fork-in-threaded-process class: child forked while another test thread held a lock; only async-signal-safe work is legal between `fork` and `exec`. The same tests pass in isolation (3661-pass run earlier the same day). The hang had to be killed, which is why ci-run-2026-07-10.log shows `cargo test --workspace` HARD-FAIL exit 101 (`error: test failed ... -p rustty-termio --lib`) — i.e. this flake is CI-breaking, not cosmetic. Same _class_ as memory `parallel-cargo-test-exposes-c-init-races` (crash site ≠ cause; parallel test harness exposes it).

- [ ] **Step 1: REQUIRED SUB-SKILL `superpowers:systematic-debugging`.** Reproduce: `while cargo test -p rustty-termio 2>&1 | tail -1; do :; done` under parallel load (run `cargo build --workspace` concurrently). Capture the hung child's stack (`sample <pid>` on macOS).
- [ ] **Step 2:** Identify what the child touches pre-exec (allocator? env lock? `std::process` internals vs a hand-rolled fork?). Compare with ghostty's `Command.zig` — Zig's posix spawn path does only async-signal-safe work in the child; if the Rust port's `pre_exec` closure allocates or locks, that is a fidelity-relevant divergence, not just a test flake.
- [ ] **Step 3:** Fix per root cause (likely: make the pre-exec path async-signal-safe to match the reference; if the hazard is purely test-harness parallelism, serialize the fork-heavy tests with the workspace's existing serialization convention — check how vt-c handles onig via OnceLock precedent). NEVER `#[ignore]` (R5).
- [ ] **Step 4:** Acceptance: 20 consecutive `cargo test -p rustty-termio` runs while a `cargo build` loop churns in parallel, zero hangs. Document root cause in the commit message. Commit.

### Task 9: Re-run ci.sh end-to-end, capture the first honest green log

- [ ] **Step 1:** `bash .port/tools/ci.sh 2>&1 | tee .port/phase-2/ci-green-$(date +%F).log` → every HARD gate green, progress gates green-or-honestly-blocked. This log is the Phase-0 exit artifact; commit it.

---

## Phase 1 — Confirmed code defects (soundness & semantics)

### Task 10: Release-profile semantics (M6 + DG2 + DG4 + §3d Kitty multiply)

**Files:**

- Modify: root `Cargo.toml` (currently has NO `[profile.*]`)
- Modify: `.port/PORT_RULES.md` (record the fork)

- [ ] **Step 1:** Add to root Cargo.toml:

```toml
# DG2: ghostty dev builds are ReleaseSafe (overflow/bounds TRAP); its tagged
# artifacts are ReleaseFast (overflow UB). rustty release keeps trap-parity
# with ReleaseSafe — strictly safer than the mirrored artifact (PORT_RULES §DG2).
[profile.release]
overflow-checks = true

# Parity/bench profile mirroring the tagged ReleaseFast artifact (Goal-2 only).
[profile.release-fast]
inherits = "release"
overflow-checks = false

# DG4: differential harness + CI test lane run the oracle's ReleaseSafe
# assert semantics (std.debug.assert active) against ported debug_assert!.
[profile.release-safe]
inherits = "release"
debug-assertions = true
```

- [ ] **Step 2:** Point `build_port_driver.sh` and the G4 harness at `--profile release-safe`. `cargo build --workspace --release` still green.
- [ ] **Step 3:** Write the capacity regression test the Kitty multiply needs (§3d): in `crates/rustty-vt` kitty graphics tests, feed `w=0xFFFF_FFFF, h=2, bpp=4` and assert the expected-size validation ERRORS (release: now panics-on-overflow → must be caught as the Zig trap-equivalent or checked-mul per the Zig source — read `graphics_image.zig`'s exact behavior first and mirror it).
- [ ] **Step 4:** Record DG2+DG4 resolution in PORT_RULES.md. Commit.

### Task 11: vaxis `Loop::stop()` SB/TB protector UB (H1) — mirihar-proven

**Files:**

- Modify: `crates/rustty-vaxis/src/loop_.rs:206-211`
- Extend: `.port/phase-2/mirihar-chunk3b` (new overlap model)

- [ ] **Step 1:** Extend the mirihar harness FIRST with a model of the actual overlap: reader thread blocked in `Tty::read(&self)` (call-protected shared borrow) while the main thread forms `&mut Tty` via `(*tty).writer()` and `&Vaxis` via `device_status_report(&self)` against concurrent plain writes to `caps.*`. `cargo +nightly miri test` in the harness → must FAIL (UB reproduced). This is the failing-test step; do not touch loop_.rs until Miri shows the violation.
- [ ] **Step 2:** Fix per H1's sketch: project the writer without a whole-Tty retag — `let w = unsafe { addr_of_mut!((*tty).tty_writer) };` and call through it (the crate already did exactly this for the same class at loop_.rs:27-38 — copy that shipped pattern); for the DSR wake, either route via the Vaxis raw-self helpers or replace with a self-pipe/quit-fd wake (read what upstream libvaxis 0.5.1 does before choosing; prefer the faithful shape).
- [ ] **Step 3:** Mirror the fix in the mirihar model → Miri clean. Then `cargo +nightly miri test -p rustty-vaxis` for the tractable suite.
- [ ] **Step 4:** Correct `MIRI_EXCEPTIONS.md:708`'s falsified "zero unsafe path unverified" claim to cite the new harness. `cargo test -p rustty-vaxis` + `rustty list-themes` interactive smoke (enter/exit). Commit.

### Task 12: Soundness mediums batch (§3c) — one wf script, one commit per item

**Files:** Create: `.port/phase-2/wf-soundness-mediums.js` (implement→adversarial-verify→surgical-fix pipeline per R11 shape)

Items (each = one pipeline element, file:line from VALIDATION_REPORT §3c):

1. `page_impl.rs:1278-1281` `Page::get_row` — replace `&mut Row`→`*mut` materialization with pure pointer arithmetic (match siblings `get_row_and_cell`/`get_cells`).
2. `vt-c/sys.rs:176` `decode_png_wrapper` — free the C-callback-allocated buffer (leak per kitty PNG decode); add an ASan-visible test.
3. `metal_impl.rs:829` — `extern "C-unwind"` display path: `catch_unwind` + abort (Zig reference aborts); never unwind into ObjC frames.
4. `apprt-c` 271 exported fns — record the abort-vs-error-code panic policy in PORT_RULES + apply `catch_unwind` wrappers per policy (placeholder leaves may keep panic=abort semantics if that IS the policy — decide, write it down, apply mechanically).
5. `tty.rs:249-258` SIGWINCH — deregistration on Drop (or a documented `// SAFETY` note + debug-liveness guard if upstream-faithful shape must stay; cite MIRI_EXCEPTIONS:693-695).
6. `MIRI_EXCEPTIONS.md` rows 8-13 — add a Miri-tractable prune-branch test; re-pin the two dangling evidence hashes (58a5faa, c46a6a2 → current equivalents).
7. `bitmap_allocator.rs:201` — `@divExact` ported as `/`: add `div_exact` helper (debug_assert exact + trap parity under DG2 profile).
8. `terminal.rs:3619` `erase_chars` — clamp in usize BEFORE u16 truncate (reachable via pub API/FFI).
9. §3e error-path integrity: `screen.rs:2828,2960`, `pagelist/erase.rs:91,278,535` — route `?`-early-returns through inner fns so `assert_integrity` runs on error paths like Zig's `defer`.
10. `iosurface_layer` unsynchronized `Cell` ivars → `AtomicPtr` (free fix per report).

- [ ] **Step 1:** Author the wf script: pipeline(items, implement, verify, fix); every fix carries its own test or mirihar/ASan proof where the item is UB-class; verifier default-not-ok; each item lands as its own commit with `(VALIDATION_REPORT §3c.N)` in the message.
- [ ] **Step 2:** Run; then `cargo test --workspace` + targeted `miri`/ASan for items 1/5/6/10. Commit any script fixes.

### Task 13: Semantics mediums batch (§3d/§3e residue)

Same wf-script shape (`.port/phase-2/wf-semantics-mediums.js`): tmux DCS dormant-body asserts restored + note fixed (dcs.rs:767-776); `@intFromFloat`→`as` selection-gesture fork documented or helper'd; `quirks::inlineAssert` policy picked + applied (~20 files); `assert!` vs `debug_assert!` inconsistency on the same op surface unified per DG4; config `format_entry_field` `_` arm + NaN "nan" + `print_repeat` u16 narrowing; xev empty-body test → visible `#[ignore = "deferred: ThreadPool body"]`... **wait — R5 forbids adding ignores**: instead port the real body or mark via the DEFERRALS ledger + a `compile_error!`-guarded cfg; decide in-task with the verifier. Font Atlas wasm 4-assert test → DEFERRALS per-test ledger entry. M4: un-ignore the two windowed-present KATs (`apprt-c/tests/macos_window_host.rs:313,377`) and RUN on this Metal host — a failure is the suppressed signal, treat as new finding.

- [ ] **Step 1:** Author + run the script; one commit per item; full `cargo test --workspace` green after.

---

## Phase 2 — Ledger & status realignment (after Task 2; enables contract grading)

### Task 14: The great status/ledger sweep (M1–M5 + §3b)

**Files:** Create: `.port/phase-2/wf-ledger-realign.js`

Scope (all from VALIDATION_REPORT — every claim pre-verified):

- Flip PORT STATUS to accurate values: vt (M1, after Task 2), config (M2), vt-c (M3 + stale comments terminal.rs:823/:1647), core, apprt, termio, font-shaper, renderer DEFERRED block, cli. Add missing `PORT STATUS:` markers: identity, terminfo, xev (normalize 'CHUNK STATUS'), z2d.
- DEFERRALS.md: add W0–W5 + vaxis 4a/4b + synthetic + wuffs sections; resolve R-d :505/:3134, W2 loop_enter note, synthetic :3266, cli C5 :2618; net effect: zero phantom open seams (~25 today), every live `unimplemented!()`/`todo!()` (69) maps to a live entry.
- GAP_ANALYSIS.md: stamp `superseded-by` header pointing at this plan + regenerate the workstream table.
- vt12-terminal-missing-tests.txt: mark resolved (all 179 ported, 2 renames).
- stream.rs SIMD-deferred header, rustty-input/vt input-slice ownership doc (10 files not 5).

- [ ] **Step 1:** Author + run the wf script (read-only inventory pass → edit pass → adversarial verify pass re-checking each header against test counts/seams).
- [ ] **Step 2:** THE PAYOFF — with headers now honest and matchers fixed, run the progress gates in binding mode for the newly-done crates: `python3 .port/tools/gates/check_tests_ported.py && python3 .port/tools/gates/check_assertions.py && python3 .port/tools/gates/check_comment_coverage.py && python3 .port/tools/gates/check_parity_surface.py`. Every residual RED is a genuine R1 finding — file and fix each (no header rollback to silence a gate; R5).
- [ ] **Step 3:** Commit per crate cluster; final commit updates `.port/phase-2/WORKFLOWS.md` provenance.

---

## Phase 3 — Bun adoptions (after DG1 ratification)

### Task 15: Ratify + land PORT_RULES R7–R12

**Files:** Modify: `.port/PORT_RULES.md` (append §5 text from BUN_PORT_LEARNINGS.md with any DG1 amendments)

- [ ] **Step 1:** Copy the ratified rule text (R7 replacement-primitive semantic fidelity; R8 comptime-phase/build-switch preservation; R9 unsafe accounting + unsound-safe ledger; R10 marker/perf-intent discipline; R11 adversarial-verification protocol + LIFETIMES.tsv; R12 upstream parity tracking) into PORT_RULES.md, marked BINDING with today's date. Cross-link each to its BUN_PORT_LEARNINGS §4/§5 anchor. Commit.

### Task 16: GHOSTTY_LIFETIMES.tsv — retroactive build + audit (locked decision, memory: adopt-lifetimes-tsv)

**Files:**

- Create: `.port/audits/GHOSTTY_LIFETIMES.tsv`
- Create: `.port/phase-2/wf-lifetime-classify.js` (modeled on Bun's `.claude/workflows/lifetime-classify.workflow.js` @ ed1a70f81708 — fetch it as the template)

**Interfaces:**

- Produces: TSV schema `file<TAB>struct<TAB>field<TAB>zig_type<TAB>class<TAB>rust_type<TAB>evidence`; classes {OWNED, SHARED, BORROW_PARAM, BORROW_FIELD, STATIC, BACKREF, INTRUSIVE, FFI, ARENA, GHOSTTY_C, UNKNOWN}; evidence = "file:line — what you saw"; UNKNOWN ⇒ `Option<NonNull<T>>` + `TODO(port)`.

- [ ] **Step 1:** Author the wf script: (a) enumerate every pointer-typed struct field in the RETROACTIVE scope — ghostty sources for rustty-vt (Page/PageList/Screen/Terminal), vt-c wrappers, xev intrusive nodes, vaxis (Tty/Vaxis/Loop — direct H1 relevance), font/font-shaper hooks, termio; (b) classify with deinit-as-ownership-oracle (§3.1) + evidence citation; (c) 3-vote default-refute verification on every UNKNOWN + a 12% sample of confident rows; (d) publish inter-classifier agreement in the TSV header comment.
- [ ] **Step 2:** Run; commit the TSV.
- [ ] **Step 3:** AUDIT pass (second wf phase): for each row, check the ported Rust field's actual type/discipline against the `rust_type` column; mismatches become findings (feed any UB-class ones through the mirihar proof loop). This is the retroactive-bug-hunt amplifier.
- [ ] **Step 4:** Add the PROSPECTIVE rule to the pre-port checklist (Task 22): no pointer-heavy tier ports without its LIFETIMES rows landing first (now binding via R11).

### Task 17: Unsafe census + UNSOUND_SAFE ledger (D7, R9)

**Files:**

- Create: `.port/audits/count-unsafe.sh` (the exact Bun rg command, vendor-excluded, per BUN_PORT_LEARNINGS §4-D7)
- Create: `.port/audits/UNSOUND_SAFE.md`
- Create: `.port/phase-2/wf-safety-tags.js` (retro-tag `// SAFETY(origin/dest):` on existing unsafe blocks)

- [ ] **Step 1:** Commit count-unsafe.sh + first snapshot JSON (per-crate density per kLOC split by R6 tier; block line-span distribution; categories reconcile exactly to total).
- [ ] **Step 2:** Author + run wf-safety-tags.js over the top-density crates first (vt: ~5,129 sites — chunked): origins {zig-port-idiom, ffi-boundary, raw-page-aliasing, callback-ctx, lifetime-erasure, uninit-buffer, marker-trait, perf}; adversarial verify a sample.
- [ ] **Step 3:** Seed UNSOUND_SAFE.md from known candidates: every safe fn wrapping a Page/Row raw-ptr deref, `&self`→`&mut` accessors, lifetime-less pointer-holding structs (the LIFETIMES audit in Task 16 feeds this directly); triage reachable-from-VT-stream / internal / theoretical.
- [ ] **Step 4:** Wire into ci.sh as advisory per-commit, blocking at sub-project finales (monotone-density rule). Add workspace lints `unsafe_op_in_unsafe_fn = "deny"`, `clippy::undocumented_unsafe_blocks = "deny"` once tagging completes (NOT before — it would be red). Commit.

### Task 18: Semantic-mismatch detectors D1–D6, D8, D9

**Files:** Create: `.port/tools/gates/check_slice_reinterpret.sh` (D1), `.port/audits/slice-reinterpret.tsv`, capacity-saturation test inventory (D2), `.port/tools/gates/check_constants_parity.py` (D3), comptime-inventory checklist + ST-integrity test helper in rustty-vt (D4), `.port/phase-2/VT_INVARIANTS.md` (D5, seeded with the 4 known pairs), sanitizer canaries (D6), clone-audit checklist (D8), stale-comment rule (D9 → completeness-critic checklist)

- [ ] **Step 1 (D1):** Land the grep gate + allowlist exactly per BUN_PORT_LEARNINGS §4-D1; populate the allowlist by running the grep and adjudicating each hit against its Zig origin (truncate|error|exact-len-proven + named odd-length test). Wire into ci.sh static-checks.
- [ ] **Step 2 (D2):** One-time inventory of every `[T; N]` + monotonic-counter pair in rustty-vt (CSI param buffers, page capacities, kitty quotas per §4-D2); write `*_capacity_saturation` tests driving each to its ceiling; gate = inventory entries each name their test.
- [ ] **Step 3 (D3):** Constants-parity script diffing numeric consts vs the Zig twin (derived consts must be the same EXPRESSION); zero placeholder-marker gate per sub-batch (scoped, per per-subbatch-scoped-g3b).
- [ ] **Step 4 (D4):** ST-integrity assertion helper (every OSC/DCS/APC opener → correct terminator) run over the reply corpus + golden byte tests on all emitted escape output; comptime-param inventory checklist item. **Highest-likelihood Bun class for this codebase — prioritize within this task.**
- [ ] **Step 5 (D5):** Seed VT_INVARIANTS.md: dirty-tracking↔renderer damage; style-id refcounts Page↔style set; kitty placements↔atlas; mode state↔key encoder — each with producer assert + consumer degrade + one integration test through the seam.
- [ ] **Step 6 (D6):** Sanitizer canaries (intentional leak + heap overflow must FAIL the ASan/LSan lane, then filtered from roll-up); cfg-mapping audit item (every `builtin.mode`/build-option switch → `cfg`, never folded — D6b) into the completeness-critic checklist.
- [ ] **Step 7 (D8+D9):** Add to the review-gate checklist: new `.clone()/.to_vec()/.to_owned()` diff-audit vs Zig copy behavior; stale-comment contract rule. Commit each detector separately.

### Task 19: Main-parity workflow — upstream ghostty commits (R12)

**Files:** Create: `.port/phase-2/wf-main-parity.js`

- [ ] **Step 1:** `git -C /Users/gael/projects/ghostty log --oneline 4749c4e9..origin/main -- src/ | wc -l` — enumerate upstream commits touching ported files since the pin (fetch first; the checkout stays read-only — `git fetch` only).
- [ ] **Step 2:** Author the wf script per BUN_PORT_LEARNINGS §3.3: per commit → read diff → map to .rs → verdict ported/missing/partial/not-applicable with file:line evidence → 2-vote adversarial verify → port missing ones WITH the commit's test hunks. Doctrine line in the script prompt verbatim: "a merge does NOT port the fix; only an explicit .rs change does."
- [ ] **Step 3:** Run for the current backlog; schedule as a standing per-sub-project ritual (add to PORT_RULES R12 text). Commit results ledger `.port/phase-2/MAIN_PARITY.md`.

### Task 20: Marker taxonomy migration (R10)

**Files:** Create: `.port/phase-2/wf-marker-migrate.js`

- [ ] **Step 1:** Census existing markers (`rg -c "TODO|FIXME|unimplemented!|todo!" crates/`); migrate free-form ones to `TODO(port):` / `PERF(port): <zig idiom>` / `PORT NOTE:` forms; large ported files gain the PORT STATUS confidence trailer.
- [ ] **Step 2:** Add the greppable-marker check to the sub-batch checklist; the endgame leverage-ranked backlog (`unsafe_removed × affected_count / effort`) becomes the C-F audit's output format. Commit.

---

## Phase 4 — Validation sweep #2 (targets = sweep #1's declared blind spots, §4)

### Task 21: Dynamic + blind-spot sweep

**Files:** Create: `.port/phase-2/wf-port-validation-2.js` (reuse wf-port-validation.js skeleton; swap auditor prompts)

Auditor set (from VALIDATION_REPORT §4 "What NO auditor examined"):

1. **Dynamic Miri** — the full per-crate Miri suite vs MIRI_EXCEPTIONS (now with the Task 4 enumerator); every exception's compensating coverage actually executed.
2. **Dynamic ASan** — G6 --all-done across the widened FFI set (termio's 233 unsafe occurrences, wuffs 27).
3. **Armed G4 runs** — full corpus + fixtures, release-safe profile (this is now just "run the gate", Task 5 armed it).
4. **Terminal.print/printCell walk** — the un-walked hot path (grapheme-attach-on-overwrite, wide chars at margins) — line-level vs Zig.
5. **Selection functions + dumpString/testWriteString walk.**
6. **Bulk unsafe bodies** — per-block review of terminal.rs (894), screen.rs (529), formatter.rs (247) guided by the Task 16 LIFETIMES audit + Task 17 SAFETY tags.
7. **Assertion-content diffs at scale** — now partially mechanical via fixed G2 (Task 3); an agent pass value-diffs the ~1,150 citation-matched tests G2 can't fully compare.
8. **Comment-fidelity content census** — the 42,069-line R1 contract, sampled at content level per crate (G3/G3b aggregates are counts only).
9. **Unicode generated tables** — the one-time generator-diff job (§3g): dump ghostty's generated arrays via a build-enabled harness, diff numerically, land a checksum test.
10. **apprt-c remaining ~60% bodies + vt-c render/selection full bodies + inspector widget bodies.**

- [ ] **Step 1:** Author + run after Phases 0–2 land (the gates it exercises must be fixed first). Same verify-then-synthesize shape; report to `.port/phase-2/VALIDATION_REPORT_2.md`; consult §5 refuted appendix so nothing is re-raised.

---

## Phase 5 — Setup for code not yet ported

### Task 22: Pre-port protocol for the remaining tiers (gtk ~23k LOC, swift-app glue, inspector widgets, platform-linux/windows)

**Files:** Modify: `.port/PORT_RULES.md` (pre-port checklist section), `docs/superpowers/specs/2026-07-08-rustty-completion-and-macos-windowing-design.md` (cross-reference)

- [ ] **Step 1:** Codify the per-tier PRE-PORT GATE (no porting workflow may start without):
  1. LIFETIMES.tsv rows for every pointer-typed field in the tier's Zig sources (Task 16 process, prospective prong — 2-reviewer consensus).
  2. Paired-invariant catalog entries for cross-crate predicates the tier introduces (D5).
  3. Comptime-parameter inventory for the tier (D4a) with per-instance rendering decisions logged.
  4. Constants-parity extraction for the tier's Zig files (D3).
  5. Design doc + adversary pass (existing doctrine), now REQUIRED to cite LIFETIMES rows for any raw-pointer seam.
  6. Slice-reinterpret allowlist extension for any new reinterpret sites (D1).
- [ ] **Step 2:** Wire into the wf-script template: the implement→verify→fix pipeline prompt block gains "consult .port/audits/GHOSTTY_LIFETIMES.tsv; trust it over local guessing; UNKNOWN ⇒ Option<NonNull<T>> + TODO(port)".
- [ ] **Step 3:** The swift-app milestone remains gated on the symbol-identity fork (open decision #1 in the completion plan — user's call, out of scope here). Note it; do not resolve it.
- [ ] **Step 4:** Commit; update memory `rustty-completion-plan` pointer to this plan.

---

## Sequencing DAG

```
Phase 0: T1 → T2 → T3 → T4 → T5 → T6/T7 (parallel) → T8 → T9 (exit artifact)
Phase 1: T10 → T11..T13 (T12/T13 parallel after T10)     [may start after T2]
Phase 2: T14 (requires T2; ideally T1–T9)
Phase 3: T15 (DG1) → T16/T17/T18/T19/T20 (parallel)      [T16 Step 3 after T14]
Phase 4: T21 (requires Phases 0–2 complete)
Phase 5: T22 (any time after T15/T16)
```

## Self-review notes

- Every VALIDATION_REPORT confirmed finding (B1,B2,H1–H7,M1–M8) maps to a task: B1→T5, B2→T1+T2, H1→T11, H2/H4→T2, H3→T1, H5→T4, H6→T3(+M7), H7→T6, M1–M3/M5→T14, M4→T13, M6→T10, M8→T4+T21. §3a→T4/T5/T7 + G8/G5 arming noted in T21's gate exercise (G8 candidate emitter = rustty-bench BASELINE-shaped JSON; G5 fuzz wiring — both are additions, park in T21 follow-ups if sweep #2 confirms priority). §3c→T12, §3d→T10/T13, §3e→T12.9/T13, §3f→(fontconfig-sys/gettext-sys/libpng duplicates are pre-Linux-milestone work; recorded in T22's tier checklist), §3g→T21.9.
- Every BUN TL;DR adoption maps: #1→T16, #2→T18, #3→T17, #4→T20, #5→T19, #6→T5/T21 (.diag files: adopt inside T5's comparator loop — write per-case diag output on diff), #7/#8→T15 (R7/R8 tables), #9→wf-script template updates in T22, #10→T15 + the same-day-rule now lives in PORT_RULES.
- LIFETIMES.tsv locked decision (memory adopt-lifetimes-tsv): T16 retroactive + T22 prospective. ✔

---

## New findings (2026-07-10 execution)

### Task 23: Wire windowed host-attach in core renderer_seam (M4 root cause; DEFERRALS W-ATTACH)

**Finding (T13/M4 re-run on a Metal host):** the M4 KAT
`hosts_view_layer_and_presents_live_frame` fails deterministically at
`"host view is not layer-hosting"` (macos_window_host.rs, first assertion) —
NOT the old (false) `Metal::loop_enter` ignore reason (that leaf landed in W5).
The TRUE gap: core `renderer_seam::Renderer::init`
(`crates/rustty-core/src/renderer_seam.rs:240`) drops `rt_surface`
(`let _ = (surface_mailbox, rt_surface, thread);`) and the generic renderer
builds its Metal via the offscreen ctor
(`crates/rustty-renderer/src/generic.rs:1840` `Metal::init_offscreen`, the W1-7
apprt-free divergence), so the on-screen attach ctor `Metal::init(view)`
(`crates/rustty-renderer/src/metal/metal_impl.rs:379`) has no caller on the
surface-creation path and no IOSurfaceLayer is ever attached to the host view.
Full triage: `.port/phase-2/DEFERRALS.md` "W-ATTACH". Interim (honest, R5): the
KAT is `#[ignore]` with the TRUE reason pointing at W-ATTACH; its sibling
`set_display_id_pushes_to_render_thread_panic_free` passes on a Metal host and
is live (un-ignored); the TIS `app_new_serialized` fix (parallel
`rustty_app_new` SIGABRT in HIToolbox TIS init) is kept.

**Files:** Modify: `crates/rustty-core/src/renderer_seam.rs` (route the host
view from `Options<A>.rt_surface` into renderer construction instead of
dropping it), `crates/rustty-renderer/src/generic.rs` (accept the optional
on-screen target and construct via `Metal::init(view)` when present —
preserving the R4 no-apprt-type rule via the same seam discipline as the
RendererConfig accessor trait), `crates/rustty-renderer/src/metal/metal_impl.rs`
(as needed for the view handoff), `crates/rustty-apprt-c/tests/macos_window_host.rs`
(un-ignore). Reference: ghostty `src/renderer/Metal.zig:64` `init` reads
`opts.rt_surface.platform.macos.nsview` and makes the view layer-hosting
(`layer` then `wantsLayer`, Metal.zig:110-126); generic.zig:662
`GraphicsAPI.init(alloc, options)` is the on-screen construction site.

- [ ] **Step 1:** Port the attach wiring (R1 line-level against Metal.zig
      init); keep `init_offscreen` for the headless/golden lanes.
- [ ] **Step 2 (acceptance):** remove the `#[ignore]` from
      `hosts_view_layer_and_presents_live_frame` — acceptance IS both
      windowed-present KATs un-ignored and green on a Metal host
      (`cargo test -p rustty-apprt-c --test macos_window_host`), plus
      `cargo test --workspace` green. Close DEFERRALS W-ATTACH in the same commit.
