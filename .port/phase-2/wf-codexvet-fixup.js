export const meta = {
  name: 'codexvet-fixup',
  description: 'Close out wave-1 rejections, orphaned verifications, and Linux-VM-executed proofs',
  whenToUse: 'args={wave, lanes:[{lane, batches:[{bid, mode, findings, files, note}]}]}; mode = fix | verify | vmverify',
  phases: [{ title: 'Fixup' }, { title: 'Recheck' }],
}

const RUSTTY = '/Users/gael/projects/rustty'
const GHOSTTY = '/Users/gael/projects/ghostty'
const VM = 'ssh rvm  # Fedora 44 aarch64, GTK 4.22.4, cargo; rsync the tree to rvm:~/rustty-vmwork'

const FIX_SCHEMA = {
  type: 'object',
  required: ['bid', 'outcomes', 'tests_run', 'summary'],
  properties: {
    bid: { type: 'string' },
    outcomes: {
      type: 'array',
      items: {
        type: 'object',
        required: ['gid', 'status'],
        properties: {
          gid: { type: 'string' },
          status: { enum: ['fixed', 'partial', 'blocked', 'no_change_needed'] },
          detail: { type: 'string' },
          files_touched: { type: 'array', items: { type: 'string' } },
          tests_added: { type: 'array', items: { type: 'string' } },
        },
      },
    },
    tests_run: { type: 'string' },
    summary: { type: 'string' },
  },
}

const VERIFY_SCHEMA = {
  type: 'object',
  required: ['bid', 'verdicts', 'summary'],
  properties: {
    bid: { type: 'string' },
    verdicts: {
      type: 'array',
      items: {
        type: 'object',
        required: ['gid', 'verified'],
        properties: {
          gid: { type: 'string' },
          verified: { type: 'boolean' },
          problems: { type: 'string' },
        },
      },
    },
    summary: { type: 'string' },
  },
}

const CONTEXT = `Rustty is a full-fidelity Rust port of Ghostty (Zig). RUSTTY=${RUSTTY} (work in place on main), read-only oracle GHOSTTY=${GHOSTTY} (pin 6ad1fe7d — NEVER modify).
A remediation wave already landed ~120 parity fixes. ${RUSTTY}/.port/codex-vet/STATE.json is the ledger: for any finding id, \`.findings[] | select(.id=="<gid>")\` gives \`.vet\` (the confirming audit: evidence + fix_plan) and \`.fix\` (what the fix agent did: .detail, .files_touched, .tests_added, and — critically — .verify_problems, the independent verifier's rejection reasons).
Repo doctrine: .port/PORT_RULES.md. Match the oracle's behavior exactly; add proving tests even where Ghostty has none; keep edits surgical; never git commit/push; never edit .port/codex/** or STATE.json.
Other lanes are editing other files concurrently — touch only your batch's files. If a shared ledger doc (DEFERRALS.md, NON_PORTS.md, MAIN_PARITY.md) needs an edit, re-read it immediately before editing and keep the hunk narrow.`

function fixPrompt(b) {
  const list = b.findings.map((f) => `- ${f.gid}: ${f.title}`).join('\n')
  return `${CONTEXT}

BATCH ${b.bid} — close out these findings:
${list}
${b.note ? `\nBATCH NOTE: ${b.note}\n` : ''}
For EACH gid:
1. Read \`.vet\` AND \`.fix\` for it in STATE.json. \`.fix.verify_problems\` is an independent verifier's concrete rejection — it is usually correct and specific (it cites oracle lines and commands it ran). Treat it as the spec for what to fix, but confirm it against the oracle yourself before acting; if the verifier is wrong, say so with evidence rather than making a change you cannot defend.
2. If \`.fix.status\` is 'fixed'/'partial', the previous attempt's code is ALREADY IN THE TREE — you are amending it, not starting over. Read the current state of the cited files first (\`git diff -- <file>\`).
3. Implement the remaining delta with full oracle fidelity, and make sure the proving test actually exercises it.
4. Verify: run the scoped tests for every crate you touch (\`cargo test -p <crate> <filter>\`). Never run workspace-wide sweeps. If the finding's proof requires Linux, note 'linux-ci-verification-pending' unless your batch note says otherwise.
5. If a fix is genuinely not safe to complete, report status=blocked with why, and revert your partial edits for that gid so the tree stays green.

Return the StructuredOutput, one outcome per gid.`
}

function verifyPrompt(b, fixResult) {
  const list = (b.findings || []).map((f) => `- ${f.gid}: ${f.title}`).join('\n')
  return `${CONTEXT}

You are an INDEPENDENT ADVERSARIAL VERIFIER for batch ${b.bid}:
${list}
${b.note ? `\nBATCH NOTE: ${b.note}\n` : ''}${fixResult ? `\nThe fix agent reported: ${String(fixResult.summary || '').slice(0, 900)}\nIts test claims: ${String(fixResult.tests_run || '').slice(0, 500)}\n` : '\nThese fixes landed in an earlier run (see .fix in STATE.json); their verification never completed, so you are the first verifier to look at them.\n'}
For EACH gid, skeptically verify:
1. Read \`.vet\` (what the gap was) and \`.fix\` (what was done) in STATE.json, then read the ACTUAL diff (\`git diff -- <files>\`, plus \`git status\` for new files).
2. FIDELITY: read the cited Ghostty oracle code YOURSELF at pin 6ad1fe7d and confirm the new Rust behavior matches exactly — including error kinds, ordering, boundary values, and cfg gating. Watch for behavior changed outside the finding's scope.
3. PROOF: would the new test actually fail without the change? Prove it if cheap (revert the hunk in a scratch copy, or reason precisely from the code). A test that only asserts what the new code does — rather than what the oracle does — is not a proof.
4. Re-run the scoped tests yourself. Also check the change does not redden a gate the repo already enforces (clippy -D warnings where the crate's gate runs it, Miri for crates listed in .port/tools/gates/check_leaks.sh, header/count gates).
5. Confirm no unrelated damage in the touched files (only intended hunks?), including shared ledger docs (a doc edit must not delete or orphan a neighbouring section).

verified=true only when fidelity, proof, tests and gates all hold. Otherwise verified=false with concrete, actionable problems. Do NOT fix anything yourself. Return the StructuredOutput.`
}

function vmVerifyPrompt(b) {
  const list = (b.findings || []).map((f) => `- ${f.gid}: ${f.title}`).join('\n')
  return `${CONTEXT}

You are verifying batch ${b.bid} — Linux-only code whose proof has NEVER BEEN EXECUTED:
${list}
${b.note ? `\nBATCH NOTE: ${b.note}\n` : ''}
A Linux VM is available: ${VM}
Established pattern for VM work (read it first): ${RUSTTY}/.port/tools/gates/ws19_portal_openuri_gate.sh documents how earlier milestones synced and ran live GTK/GL gates on this VM.

Steps:
1. Read \`.vet\` and \`.fix\` for each gid in STATE.json, plus the current diff of the cited files.
2. Sync the working tree to the VM (rsync to rvm:~/rustty-vmwork; exclude target/ and .git/ — check what the existing gate scripts do, and do NOT git commit anything).
3. EXECUTE the proof on the VM: build and run the tests the fix added (the exact cargo commands the finding's proof depends on), plus any gate the crate is subject to (e.g. the GTK feature lane's clippy -D warnings). Report the real command output.
4. Prove the test is non-vacuous: revert the production hunk in the VM copy ONLY (never in ${RUSTTY}), re-run, and confirm the test fails; then restore. If that is impractical, say so plainly instead of claiming proof.
5. Read the oracle at pin 6ad1fe7d and confirm fidelity of what actually ran.

Local repo ${RUSTTY} must be left untouched — this batch is verification only. verified=true only if the proof truly executed and passed on Linux. Return the StructuredOutput.`
}

const A = typeof args === 'string' ? JSON.parse(args) : args
const lanes = A.lanes
log(`fixup wave ${A.wave}: ${lanes.length} lanes, ${lanes.reduce((n, l) => n + l.batches.length, 0)} batches`)

const results = await parallel(
  lanes.map((lane) => async () => {
    const out = []
    for (const b of lane.batches) {
      if (b.mode === 'verify') {
        const v = await agent(verifyPrompt(b, null), { label: `verify:${b.bid}`, phase: 'Recheck', schema: VERIFY_SCHEMA })
        out.push({ ...(v || { verdicts: [], summary: 'verifier died' }), bid: b.bid, fix: null })
        continue
      }
      if (b.mode === 'vmverify') {
        const v = await agent(vmVerifyPrompt(b), { label: `vmverify:${b.bid}`, phase: 'Recheck', schema: VERIFY_SCHEMA })
        out.push({ ...(v || { verdicts: [], summary: 'vm verifier died' }), bid: b.bid, fix: null })
        continue
      }
      const fixResult = await agent(fixPrompt(b), { label: `fixup:${b.bid}`, phase: 'Fixup', schema: FIX_SCHEMA })
      if (!fixResult) { out.push({ bid: b.bid, fix: null, verdicts: [], summary: 'fix agent died' }); continue }
      const anyFixed = (fixResult.outcomes || []).some((o) => o.status === 'fixed' || o.status === 'partial')
      if (!anyFixed) { out.push({ bid: b.bid, fix: fixResult, verdicts: [], summary: 'nothing changed; recheck skipped' }); continue }
      const v = await agent(verifyPrompt(b, fixResult), { label: `verify:${b.bid}`, phase: 'Recheck', schema: VERIFY_SCHEMA })
      out.push({ ...(v || { verdicts: [], summary: 'verifier died' }), bid: b.bid, fix: fixResult })
    }
    return { lane: lane.lane, batches: out }
  })
)

log(`fixup wave ${A.wave} complete`)
return { wave: A.wave, lanes: results.filter(Boolean) }
