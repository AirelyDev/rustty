export const meta = {
  name: 'codexvet-fix',
  description: 'Remediate confirmed parity gaps in file-disjoint batches: fix + proving tests, then independent verify',
  whenToUse: 'Pass args={wave, batches:[{bid, findings:[{gid,title,fix_category}], files:[...]}]} built from .port/codex-vet/STATE.json',
  phases: [{ title: 'Fix' }, { title: 'Verify' }],
}

const RUSTTY = '/Users/gael/projects/rustty'
const GHOSTTY = '/Users/gael/projects/ghostty'

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
          detail: { type: 'string', description: '<=400 chars: what changed, or why blocked' },
          files_touched: { type: 'array', items: { type: 'string' } },
          tests_added: { type: 'array', items: { type: 'string' } },
        },
      },
    },
    tests_run: { type: 'string', description: 'exact cargo test commands run and their pass/fail counts' },
    summary: { type: 'string', description: '<=600 chars overall' },
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
          problems: { type: 'string', description: 'empty if clean; else what is wrong (fidelity drift, missing test, test does not prove the delta, regression)' },
        },
      },
    },
    summary: { type: 'string' },
  },
}

function fixPrompt(b) {
  const list = b.findings
    .map((f) => `- ${f.gid} [${f.fix_category}]: ${f.title}`)
    .join('\n')
  return `You are one fix agent in a remediation fleet for Rustty — a full-fidelity Rust port of Ghostty (Zig). RUSTTY=${RUSTTY} (work here, on main, in-place), read-only oracle GHOSTTY=${GHOSTTY} (pin 6ad1fe7d — NEVER modify it).

Your batch ${b.bid} remediates these independently CONFIRMED parity findings:
${list}

For EACH finding gid:
1. Read its full vet verdict in ${RUSTTY}/.port/codex-vet/STATE.json (jq or python: .findings[] | select(.id=="<gid>") — the .vet object holds verdict/evidence/fix_plan/files). The fix_plan was written by the confirming auditor with exact file:line cites — follow it unless you find it wrong against the sources, in which case implement what the Ghostty oracle actually does and say so in your outcome detail.
2. Consult the original Codex report if helpful: grep -rln -- "<gid>" ${RUSTTY}/.port/codex/.
3. Implement the fix with FULL fidelity to the Ghostty oracle's behavior (match its semantics exactly; byte-level behavior where the oracle is byte-exact). Follow repo conventions: read neighboring code first; PORT NOTE comments where the port already uses them; Zig byte-slices map to Vec<u8>/&[u8], never lossy String; &mut-arg raw-pointer doctrine for Page/Row/Cell args (see .port/PORT_RULES.md).
4. Add PROVING tests — a test that fails without your change and passes with it. Tests may be added even where Ghostty has none. Follow the existing test idioms of the file you extend (tripwire/failpoint patterns exist for OOM arms).
5. Verify: run scoped tests (cargo test -p <crate> <filter>) for every crate you touched, plus the specific new tests. If you touched cross-crate seams, cargo check -p on the dependents. Do NOT run whole-workspace test sweeps (other agents are working in parallel; target-dir lock contention). If a fix cannot be completed safely, report status=blocked with why — do NOT leave the tree broken or half-edited: revert your partial edits for that gid.

Constraints:
- Touch ONLY files needed for your batch (other agents own other files; batches are file-disjoint by construction).
- Do NOT git commit, do NOT git checkout/restore other files, never git push.
- Do NOT modify ${RUSTTY}/.port/codex/** (the audit record is frozen). You MAY update .port/phase-2/DEFERRALS.md / MAIN_PARITY.md rows and stale PORT NOTE comments when your fix makes them outdated — several fix_plans call for exactly that.
- Keep each fix minimal and surgical; no drive-by refactors.
- Do NOT edit or regenerate include/*.h public headers unless a finding in YOUR batch explicitly targets that header — if your code fix would require a header regen, note it in the outcome detail instead (a dedicated codegen lane owns headers).
- Shared ledger docs (DEFERRALS.md, MAIN_PARITY.md, PACKAGING.md) may be edited concurrently by other lanes: re-read the file immediately before editing and keep your edit narrowly scoped to your rows.
- Never edit .port/codex-vet/STATE.json (the orchestrator owns it).
- Linux-only (cfg target_os="linux") code cannot be compile-checked on this macOS host. Still make the edit per plan, get as much verification as possible (e.g. shared-code tests, careful cross-reading against the oracle), and note "linux-ci-verification-pending" in the outcome detail.

Return the StructuredOutput with one outcome per gid.`
}

function verifyPrompt(b, fixResult) {
  const fixed = (fixResult.outcomes || []).filter((o) => o.status === 'fixed' || o.status === 'partial')
  const list = fixed.map((o) => `- ${o.gid}: ${o.detail || ''} (files: ${(o.files_touched || []).join(', ')})`).join('\n')
  return `You are an independent adversarial verifier for Rustty (Rust port of Ghostty; RUSTTY=${RUSTTY}, read-only oracle GHOSTTY=${GHOSTTY} pin 6ad1fe7d). A fix agent just remediated batch ${b.bid}:
${list}
Fix agent's test report: ${String(fixResult.tests_run || '').slice(0, 800)}

For EACH gid above, verify skeptically:
1. Read the vet verdict + fix_plan in ${RUSTTY}/.port/codex-vet/STATE.json (.findings[] id==gid, .vet) and the actual diff (git diff -- <files>, plus git status for new files).
2. Check FIDELITY: does the new Rust behavior match the Ghostty oracle exactly (read the cited oracle code yourself)? Watch for: subtle off-by-one vs the oracle, error-type mismatches, cfg-gating mistakes, behavior changed for inputs outside the finding's scope.
3. Check the PROVING test really proves the delta: would it fail without the change? Is it asserting the oracle-matching behavior rather than just the new code's behavior?
4. Re-run the relevant scoped tests yourself (cargo test -p <crate> <filter>). No workspace-wide sweeps.
5. Check nothing unrelated was damaged in the touched files (git diff scan: only intended hunks?).

verified=true only when fidelity, proof, and tests all hold. Otherwise verified=false with concrete problems. Do NOT fix anything yourself; do not modify files. Return the StructuredOutput.`
}

const A = typeof args === 'string' ? JSON.parse(args) : args
const lanes = A.lanes
log(`fix wave ${A.wave}: ${lanes.length} parallel lanes, ${lanes.reduce((n, l) => n + l.batches.length, 0)} sub-batches`)

// Lanes run in parallel; sub-batches inside a lane run strictly sequentially
// (they share files). Each sub-batch: fix agent, then independent verifier.
const results = await parallel(
  lanes.map((lane) => async () => {
    const out = []
    for (const b of lane.batches) {
      // verifyOnly: the fix already landed in an earlier (interrupted) run and
      // is on disk; only the independent verification pass is outstanding.
      if (b.verifyOnly) {
        const synth = {
          bid: b.bid,
          outcomes: (b.findings || []).map((f) => ({ gid: f.gid, status: 'fixed', detail: f.landedDetail || 'landed in an earlier run; see STATE.json .fix', files_touched: b.files || [] })),
          tests_run: b.landedTests || '(from the earlier run; re-run them yourself)',
          summary: 'fix landed in an earlier interrupted run; verification outstanding',
        }
        const v = await agent(verifyPrompt(b, synth), { label: `verify:${b.bid}`, phase: 'Verify', schema: VERIFY_SCHEMA })
        out.push({ ...(v || { verdicts: [], summary: 'verifier died' }), bid: b.bid, fix: synth })
        continue
      }
      const fixResult = await agent(fixPrompt(b), { label: `fix:${b.bid}`, phase: 'Fix', schema: FIX_SCHEMA })
      if (!fixResult) { out.push({ bid: b.bid, fix: null, verdicts: [], summary: 'fix agent died/skipped' }); continue }
      const anyFixed = (fixResult.outcomes || []).some((o) => o.status === 'fixed' || o.status === 'partial')
      if (!anyFixed) { out.push({ bid: b.bid, fix: fixResult, verdicts: [], summary: 'nothing fixed; verify skipped' }); continue }
      const v = await agent(verifyPrompt(b, fixResult), { label: `verify:${b.bid}`, phase: 'Verify', schema: VERIFY_SCHEMA })
      out.push({ ...(v || { verdicts: [], summary: 'verifier died' }), bid: b.bid, fix: fixResult })
    }
    return { lane: lane.lane, batches: out }
  })
)

const ok = results.filter(Boolean)
log(`fix wave ${A.wave} complete: ${ok.length}/${lanes.length} lanes reported`)
return { wave: A.wave, lanes: ok }
