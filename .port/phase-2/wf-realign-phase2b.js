export const meta = {
  name: 'realign-phase2b',
  description: 'Close out Phases 1+2: commit T13\'s finished items with M4 honestly ledgered, finish the T14 ledger remainder, teach G1/G2 the five verified name-families, triage the 136 WEAKENED rows, closing ci.sh run',
  whenToUse: 'Follow-up to wf-realign-phase12 (T13 blocked-on-M4, T14 impl API-failure, payoff surfaced matcher families)',
  phases: [
    { title: 'Commit T13', detail: 'verify + commit the 7 done items; M4 → honest ignore-reason + DEFERRALS + plan addendum' },
    { title: 'T14 remainder', detail: 'DEFERRALS reconciliation, GAP_ANALYSIS stamp, residual §3b items' },
    { title: 'Matcher families', detail: 'name_candidates F2-F5 + G2 WEAKENED triage — binding gates to green honestly' },
    { title: 'Close', detail: 'full ci.sh, commit logs' },
  ],
}

const PLAN = 'docs/superpowers/plans/2026-07-10-port-realignment-and-validation-hardening.md'
const REPORT = '.port/phase-2/VALIDATION_REPORT.md'

const IMPL = {
  type: 'object',
  required: ['task', 'status', 'changed_files', 'evidence'],
  properties: {
    task: { type: 'string' },
    status: { type: 'string', enum: ['done', 'blocked'] },
    changed_files: { type: 'array', items: { type: 'string' } },
    evidence: { type: 'string' },
    blocker: { type: 'string' },
    notes: { type: 'string' },
    committed: { type: 'boolean' },
    commit_refs: { type: 'array', items: { type: 'string' } },
  },
}

const COMMON = `
REPO: /Users/gael/projects/rustty. Plan: ${PLAN}. Evidence: ${REPORT}.
HARD RULES:
- NEVER modify anything under /Users/gael/projects/ghostty (read-only oracle, R3) — but DO read it to verify fidelity claims (R1).
- R5: never weaken/skip/loosen any gate or test; a red that cannot go green honestly → status=blocked with a precise blocker. Matcher changes may only ADD candidate forms for names whose ported test you have VERIFIED exists (quote file:line for at least 3 samples per family); gate self-tests (--self-test) must still flip RED-on-injection afterwards.
- cargo may block on the target-dir lock while siblings run — wait it out.
- Git: explicit paths only; index.lock → sleep 3, retry ≤10x; never reset/checkout/stash/pull; no push. You own your commits.
Return via StructuredOutput with decisive command output as evidence.`

phase('Commit T13')
const t13c = await agent(`${COMMON}
TASK: land T13's finished-but-uncommitted work (the previous T13 agent completed 7 of 8 items then blocked on M4; its edits are LIVE in the working tree, uncommitted).
The uncommitted T13 files: crates/rustty-vt/src/{dcs.rs,terminal.rs,stream.rs,stream_terminal.rs}, crates/rustty-termio/src/stream_handler.rs, crates/rustty-core/src/surface.rs, crates/rustty-util/src/quirks.rs, crates/rustty-config/src/config_struct.rs, crates/rustty-cfg-fmt/src/{formatter.rs,lib.rs}, crates/rustty-xev/src/kqueue.rs, crates/rustty-font/src/atlas.rs, crates/rustty-apprt-c/tests/macos_window_host.rs, .port/phase-2/DEFERRALS.md.
1. VERIFY each item's diff against its ${PLAN} Task 13 spec + ${REPORT} §3d/§3e lines (read the Zig reference for each behavioral change, R1). Run the touching crates' tests (cargo test -p <crate>) and cargo fmt --check + clippy on them.
2. M4 handling (the KAT that FAILS when un-ignored: "host view is not layer-hosting" at macos_window_host.rs:329 — a REAL windowing gap: core renderer_seam Renderer::init constructs Metal via init_offscreen and never attaches the host view layer): committing an always-failing test is not acceptable, and neither is the old FALSE ignore reason. Honest interim: restore #[ignore] on the two KATs with the TRUE reason string naming the real gap (e.g. "deferred: windowed host-attach unwired — Renderer::init uses init_offscreen; see DEFERRALS W-ATTACH"), add a DEFERRALS.md W-ATTACH entry with the full triage (failure verbatim, root cause file:line, resolution = the windowed-attach porting chunk whose acceptance test IS un-ignoring these KATs), and append an addendum section "New findings (2026-07-10 execution)" to ${PLAN} defining Task 23: wire windowed host-attach in core renderer_seam (files, acceptance = both KATs un-ignored and green on a Metal host). Keep the TIS-serialization fix from the T13 work (it fixed a real SIGABRT) regardless.
3. Commit per item (or small coherent groups), messages per the plan's T13 item list + "(T13, VALIDATION_REPORT §3d/§3e)"; the M4 commit message must say the ignore-reason is now honest and points at W-ATTACH.
4. Full-tree check after: cargo test --workspace green; git status shows none of the T13 files modified.`,
  { label: 'commit:T13', phase: 'Commit T13', schema: IMPL })
log(`T13 close: ${t13c ? t13c.status : 'failed'}`)

phase('T14 remainder')
const t14r = await agent(`${COMMON}
TASK: finish ${PLAN} Task 14 — the parts the payoff commit (ae40aa2) did NOT do. ae40aa2 already flipped the PORT STATUS headers (vt/config/vt-c/core/cli/identity/terminfo/xev/z2d and matcher files). Remaining per ${REPORT} §3b (each pre-verified with file:line):
- DEFERRALS.md reconciliation: add the missing W0-W5, vaxis-4a/4b, synthetic, wuffs sections; resolve R-d :505/:3134, the W2 "loop_enter PANICS until W3" note, synthetic :3266, cli C5 :2618; note the T13 agent may have already added some (check the working tree/git log first — do not duplicate).
- rustty-renderer lib.rs DEFERRED block: prune the ~90% stale entries (W0-W5, config, Kitty, overlay landed; keep inspector overlayFeatures + GL/WebGL/GTK).
- rustty-apprt "chunk E0 — scaffold" header → accurate (E1-E11 landed, 13 tracked leaves).
- rustty-termio + rustty-font-shaper stale header sentences (§3b).
- GAP_ANALYSIS.md: superseded-by stamp pointing at ${PLAN} + corrected workstream table.
- vt12-terminal-missing-tests.txt: mark resolved (all 179 ported, 2 renames).
- stream.rs SIMD-deferred header + input-slice ownership doc (10 files not 5) — check whether T13's stream.rs commit already fixed the header.
FINISH with the proof the plan demands: enumerate every live unimplemented!()/todo!() in crates/ and show each maps to a live DEFERRALS entry (paste the mapping table into evidence). Commit per cluster with the plan's T14 message + "(T14, VALIDATION_REPORT M1-M5 §3b)".`,
  { label: 'T14:remainder', phase: 'T14 remainder', schema: IMPL })
log(`T14 remainder: ${t14r ? t14r.status : 'failed'}`)

phase('Matcher families')
const fam = await agent(`${COMMON}
TASK: make the binding-mode G1/G2 verdicts honest-green. The payoff run (see .port/phase-2/ci-phase2-payoff.log + commit ae40aa2, classifier notes in .port/phase-2/tmp-payoff-classify.py) left: G1 RED with ~379 MISSING names on done crates decomposing into name-convention families — F2 canon-fn (mixed-case snake joins + _test collision suffix, e.g. 'Capacity maxCols basic'→capacity_maxCols_basic page_tests.rs:434, 'vt_write'→vt_write_test source.rs, 'formatTimestamp'→format_timestamp_test ssh_cache.rs:744), F3 squash (CamelCase compounds joined: 'PageList …'→pagelist_* pagelist/tests.rs:4165, 'execveZ'→execvez), F4 container-prefix (~57, e.g. 'parseCLI tab…' carrying the impl container), F5 (see the classifier) — and G2 RED with 136 WEAKENED rows (vt-c 110, vt 19, config 4, cli 2), most likely mis-bound comparisons rather than real weakenings.
1. For EACH family: sample ≥5 names, verify the ported test exists (file:line), then extend _gatelib.name_candidates (and check_assertions' layered resolver if needed) with a CONSERVATIVE additional candidate form for that family. NEVER add a form that could bind a name to a non-matching fn (collision check: after each family lands, assert the total EXTRA+MISSING accounting still reconciles and no candidate form maps two different ref names to the same fn without a multiplicity note).
2. Names that remain MISSING after honest matcher forms = REAL gaps: port each missing test assertion-for-assertion from its Zig source (with comments), or rename a nonconforming port fn to the DoP slug ONLY when the plan's convention clearly prescribes it (like T9 did for args) — never delete/relabel a contract entry.
3. G2 WEAKENED triage: for all 136 rows, determine bound-to-wrong-fn (fix binding) vs genuinely fewer assertions (port the missing assertions from the Zig test). Zero rows may be waved through.
4. Self-tests: check_tests_ported --self-test and check_assertions --self-test still flip RED on injection.
5. Acceptance: python3 .port/tools/gates/check_tests_ported.py exit 0 AND check_assertions.py exit 0, in binding mode with the honest headers, with the workspace floor (3234/11975) intact. Commit in ≤3 coherent commits ("gates: name families F2-F5 …", "rustty-*: residual contract tests ported …", "gates: G2 bindings …").
If any family CANNOT be matched without a risky form, leave those names RED and return status=blocked with the precise list — honesty over green.`,
  { label: 'families:G1G2', phase: 'Matcher families', schema: IMPL })
log(`Families: ${fam ? fam.status : 'failed'}`)

phase('Close')
const close = await agent(`${COMMON}
TASK: closing run. bash .port/tools/ci.sh 2>&1 | tee .port/phase-2/ci-phase12-close.log — read the CI SUMMARY. Expected: all HARD green, binding G1/G2/G3/G3b/G9 green (or precisely-explained residuals from the prior agents' blocked lists). Diagnose any surprise red: surgical Phase-0/1/2 fallout you can prove → fix + one re-run; anything else → report as blocker, never bend (R5).
Then commit: the log, .port/phase-2/ci-phase2-payoff.log (untracked from the payoff), and the edited .port/phase-2/wf-realign-phase12.js + the new wf-realign-phase2b.js (message "ci: phases 1+2 close — binding gates state + orchestration scripts"). Leave the tmp-*.py analysis scripts untracked. Report the final gate table verbatim in evidence.`,
  { label: 'close:ci', phase: 'Close', schema: IMPL })

return {
  t13: t13c, t14: t14r, families: fam, close,
  summary: `T13 ${t13c ? t13c.status : 'fail'}; T14r ${t14r ? t14r.status : 'fail'}; families ${fam ? fam.status : 'fail'}; close ${close ? close.status : 'fail'} — ${(close && (close.notes || close.evidence) || '').slice(0, 400)}`,
}
