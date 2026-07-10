export const meta = {
  name: 'realign-phase0',
  description: 'Phase 0 of the realignment plan: make the gates honest — 8 plan tasks as implement→adversarial-verify→fix pipelines, then the first honest full ci.sh run',
  whenToUse: 'Executes docs/superpowers/plans/2026-07-10-port-realignment-and-validation-hardening.md Phase 0',
  phases: [
    { title: 'Round 1', detail: 'T1 bench test, T2 name matcher, T4 done-detection+G6 routing, T5 arm G4, T7 register test-counts, T8 termio fork deadlock — parallel pipelines' },
    { title: 'Round 2', detail: 'T3 G2 binding (needs T2), T6 CI workflow (needs T1/T2)' },
    { title: 'Round 3', detail: 'T9 full ci.sh run — the Phase-0 exit artifact' },
  ],
}

const PLAN = 'docs/superpowers/plans/2026-07-10-port-realignment-and-validation-hardening.md'

const IMPL = {
  type: 'object',
  required: ['task', 'status', 'changed_files', 'evidence'],
  properties: {
    task: { type: 'string' },
    status: { type: 'string', enum: ['done', 'blocked'] },
    changed_files: { type: 'array', items: { type: 'string' } },
    evidence: { type: 'string', description: 'exact commands run + their decisive output lines proving each plan step' },
    blocker: { type: 'string', description: 'if blocked: the precise honest blocker per R5' },
    notes: { type: 'string' },
  },
}

const VERDICT = {
  type: 'object',
  required: ['ok', 'issues'],
  properties: {
    ok: { type: 'boolean' },
    issues: { type: 'array', items: { type: 'string' }, description: 'each: file:line + what is wrong + what the plan/rules require' },
    r5_concerns: { type: 'array', items: { type: 'string' }, description: 'anything that WEAKENS a gate/test — automatic not-ok' },
  },
}

const FIX = {
  type: 'object',
  required: ['task', 'committed', 'summary'],
  properties: {
    task: { type: 'string' },
    committed: { type: 'boolean' },
    commit_ref: { type: 'string' },
    summary: { type: 'string' },
    residual: { type: 'string', description: 'anything left un-fixed and why (honest, R5)' },
  },
}

const COMMON = `
REPO: /Users/gael/projects/rustty. You are executing ONE task of the realignment plan.
MANDATORY FIRST READS: (1) ${PLAN} — your task's section ONLY plus the Global Constraints + Decision gates table (DG1-DG4 are resolved as RECOMMENDED); (2) the evidence docs it cites for your task (.port/phase-2/VALIDATION_REPORT.md sections named in your task; BUN_PORT_LEARNINGS.md only if cited).
HARD RULES:
- NEVER modify anything under /Users/gael/projects/ghostty (read-only oracle; R3).
- R5: never weaken, skip, xfail, #[ignore], or loosen any test or gate. If a step cannot go green honestly, return status=blocked with a precise blocker instead of bending anything.
- Gate self-tests (--self-test) must still flip RED-on-injection after your change; run them when your task touches a gate.
- cargo commands may block on the target-dir lock while sibling tasks run — that is normal, wait it out (do not kill builds).
- Git: commit ONLY when your task prompt says so, ONLY explicit paths (git add <paths> && git commit -m "..."). If index.lock contention occurs, sleep 3 and retry (up to 10x). NEVER git reset/checkout/stash/rebase/pull. NO push.
- Follow the plan's steps in order, including writing failing checks first where a step says so.
Return via StructuredOutput with real evidence (paste the decisive output lines).`

function taskPipeline(t) {
  return async () => {
    const impl = await agent(`${COMMON}
YOUR TASK: ${t.id} — ${t.title}. Execute every step of "${t.heading}" in ${PLAN}. ${t.extra || ''}
Do NOT commit — a fixer commits after adversarial review. Leave the working tree changes in place.`,
      { label: `impl:${t.id}`, phase: t.phase, schema: IMPL })

    if (!impl || impl.status === 'blocked') {
      log(`${t.id} BLOCKED: ${impl ? impl.blocker : 'agent failed'}`)
      return { task: t.id, committed: false, summary: `BLOCKED: ${impl ? impl.blocker : 'agent failed'}` }
    }

    const verdict = await agent(`${COMMON}
You are the ADVERSARIAL VERIFIER for task ${t.id} (${t.title}). Another agent claims it executed "${t.heading}" of ${PLAN}. Its report:
${JSON.stringify(impl, null, 1)}
Default ok=false. Check with your own eyes (git diff -- <its changed files>; re-run its verification commands yourself): (a) every plan step actually done as specified; (b) NOTHING weakened — diff every touched gate/test for R5 violations (a "fix" that silences a gate instead of correcting the matcher/wiring is the #1 failure mode to catch); (c) self-tests still flip RED on injection where applicable; (d) claimed command output reproduces. List every concrete issue with file:line.`,
      { label: `verify:${t.id}`, phase: t.phase, schema: VERDICT })

    return agent(`${COMMON}
You are the SURGICAL FIXER for task ${t.id} (${t.title}). Implementation report:
${JSON.stringify(impl, null, 1)}
Adversarial review verdict:
${JSON.stringify(verdict, null, 1)}
If issues exist: fix each surgically (if an issue is wrong — verifier hallucinated — skip it and note that in residual). Re-run the task's verification commands until green. THEN commit exactly the task's files with the plan's commit message for ${t.id} (index.lock: sleep 3, retry up to 10x). If the verdict was ok with no issues, just run the final verification once more and commit. Never commit unrelated working-tree changes — explicit paths only.`,
      { label: `fix:${t.id}`, phase: t.phase, schema: FIX })
  }
}

phase('Round 1')
const round1 = [
  {
    id: 'T1', phase: 'Round 1', title: 'bench ref_all_decls contract test',
    heading: 'Task 1: bench `ref_all_decls` contract test',
    extra: 'Copy the exact ref_all_decls convention from an existing crate (grep -rn "fn ref_all_decls" crates/rustty-crash/src crates/rustty-zf/src). Acceptance: cargo test -p rustty-bench shows 8 passed AND python3 .port/tools/gates/check_tests_ported.py output shows bench 8/8 GREEN.',
  },
  {
    id: 'T2', phase: 'Round 1', title: 'G1/G9 name matcher composed slug',
    heading: 'Task 2: G1/G9 name matcher — generate the port\'s slug convention',
    extra: 'The plan contains the exact code to add. After landing it, paste the before/after MISSING counts for args, extra, termio, and the vt done-flip simulation into evidence. Any REMAINING missing after the fix is a REAL R1 gap: list it in notes, do not suppress it.',
  },
  {
    id: 'T4', phase: 'Round 1', title: 'done-detection unification + G6 routing + strict semantics',
    heading: 'Task 4: Done-detection unification + `--strict` semantics',
    extra: 'Includes the LIVE G6 RED fix (Step 5): rustty-os --test cf_release_live calls real kqueue which Miri cannot run — route that ONE test binary to the ASan lane (cargo miri test -p rustty-os --lib for the Miri lane), document in MIRI_EXCEPTIONS.md with the 4-criterion justification. The test itself keeps running under cargo test + ASan; nothing is skipped. Acceptance: bash .port/tools/gates/check_leaks.sh exits 0.',
  },
  {
    id: 'T5', phase: 'Round 1', title: 'arm G4 — real byte-exact differential',
    heading: 'Task 5: Arm G4 — the real byte-exact differential gate',
    extra: 'CRITICAL: read harness/comparator.py argparse FIRST and use its real CLI. Step 6 is the first honest run — if it reports divergences, those are REAL FINDINGS: record each (case name + first diverging bytes) in notes as fidelity bugs, leave the gate honestly RED, and return status=done with the divergence list (a red gate with real findings is a SUCCESSFUL arming; do NOT bend the gate to green, R5). Also adopt Bun-style .diag output per failing case (write <case>.diag with crash/hang/diverge + the byte context) inside the comparator loop.',
  },
  {
    id: 'T7', phase: 'Round 1', title: 'register check_test_counts.py in ci.sh',
    heading: 'Task 7: Register `check_test_counts.py` in ci.sh',
  },
  {
    id: 'T8', phase: 'Round 1', title: 'termio fork deadlock (systematic debugging)',
    heading: 'Task 8: termio test-binary fork deadlock',
    extra: `This is a DEBUGGING task — follow superpowers:systematic-debugging discipline: reproduce first, form hypotheses, instrument, prove root cause BEFORE fixing. Evidence so far: under parallel load, rustty-termio's test binary hung >29min; a forked child sat at 0.00s CPU (classic fork-while-threaded lock inheritance; only async-signal-safe work is legal between fork and exec). Suspect tests: command::tests::command_rt_pre_exec_1, exec::tests::subprocess_start_spawns_and_reaps, exec::tests::threadenter_queuewrite_writes_to_pty. Compare the child-side path against ghostty's src/os/Command.zig (or termio equivalent) — if the Rust pre_exec closure allocates/locks where Zig is async-signal-safe, that is a fidelity divergence to fix in the code, not the tests. Reproduction may take many iterations under load (run a cargo build loop concurrently); use 'sample <pid>' on a hung child. NEVER #[ignore]. Acceptance: 20 consecutive cargo test -p rustty-termio runs with concurrent build churn, zero hangs; root cause stated in the commit message. If the root cause is genuinely upstream-faithful behavior that only the test harness provokes, serialize ONLY the fork-heavy tests using an existing workspace convention and document why that is not a weakening (they still run).`,
  },
]

const r1 = await parallel(round1.map(t => taskPipeline(t)))
const r1Blocked = r1.filter(Boolean).filter(r => !r.committed)
log(`Round 1: ${r1.filter(Boolean).filter(r => r.committed).length}/${round1.length} committed${r1Blocked.length ? '; blocked/uncommitted: ' + r1Blocked.map(r => r.task).join(', ') : ''}`)

phase('Round 2')
const round2 = [
  {
    id: 'T3', phase: 'Round 2', title: 'G2 per-test binding + undiluted counting',
    heading: 'Task 3: G2 assertion gate — real per-test comparison, undiluted counting',
    extra: 'T2 (name_candidates composed slug) is already landed — build on it. Paste before/after compared-pair counts (must jump from 43 to >2,900) and the new injection self-test result into evidence.',
  },
  {
    id: 'T6', phase: 'Round 2', title: 'CI workflow can pass',
    heading: 'Task 6: CI workflow can actually pass',
    extra: 'Do NOT push (Step 4 is deferred to the user). Instead validate locally: python3 -c "import yaml,sys; yaml.safe_load(open(\'.github/workflows/ci.yml\'))" parses, and cross-check every exit-code the yml branches on against what the gate scripts actually emit. Note in residual that the branch-push verification is pending.',
  },
]
const r2 = await parallel(round2.map(t => taskPipeline(t)))
log(`Round 2: ${r2.filter(Boolean).filter(r => r.committed).length}/${round2.length} committed`)

phase('Round 3')
const t9 = await agent(`${COMMON}
YOUR TASK: T9 — the Phase-0 exit artifact. Run: bash .port/tools/ci.sh 2>&1 | tee .port/phase-2/ci-green-2026-07-10.log
This is the FIRST run after the gate-honesty fixes. Read the CI SUMMARY block.
- If everything HARD is green: commit the log (git add .port/phase-2/ci-green-2026-07-10.log && git commit -m "ci: first honest full-gate run after Phase 0 realignment (T9)").
- If anything is red: diagnose WHICH task's fix is incomplete vs which is a genuinely new finding. Fix incomplete Phase-0 work yourself if surgical (then re-run ci.sh once); report genuinely new findings as blockers — do NOT weaken anything (R5). Commit the log either way, renaming to ci-run-2026-07-10-phase0.log if red, and say exactly what is red and why in evidence.
Note: the earlier pre-fix run is at /Users/gael/.claude/jobs/22556231/tmp/ci-run-2026-07-10.log for comparison (its RED set: cargo-test/G1/G6).`,
  { label: 'T9:ci-run', phase: 'Round 3', schema: IMPL })

return {
  round1: r1,
  round2: r2,
  t9,
  summary: `Phase 0: R1 ${r1.filter(Boolean).filter(r => r.committed).length}/6, R2 ${r2.filter(Boolean).filter(r => r.committed).length}/2 committed; T9 ${t9 ? t9.status : 'failed'} — ${t9 ? (t9.notes || t9.evidence || '').slice(0, 400) : ''}`,
}
