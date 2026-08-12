export const meta = {
  name: 'codexvet-vet',
  description: 'Independently vet one tier of Codex-reported parity findings against primary sources',
  whenToUse: 'Pass args={tier, groups:[{gid,members,severity,title}]} built from .port/codex-vet/STATE.json',
  phases: [{ title: 'Vet' }],
}

const SCHEMA = {
  type: 'object',
  required: ['gid', 'verdict', 'known_deferred', 'fixable_now', 'fix_category', 'evidence'],
  properties: {
    gid: { type: 'string' },
    verdict: { enum: ['confirmed', 'not_a_gap', 'already_fixed', 'needs_deeper_evidence'] },
    severity_assessment: { type: 'string', description: 'agree/disagree with reported severity, one line' },
    known_deferred: { type: 'boolean', description: 'documented deliberate deferral of the macOS-first plan' },
    deferral_ref: { type: 'string', description: 'doc + section proving the deferral, empty if none' },
    fixable_now: { type: 'boolean' },
    fix_category: { enum: ['code-fix', 'test-add', 'build-config', 'docs', 'deferred-platform', 'infra', 'none'] },
    evidence: { type: 'string', description: '<=1500 chars, file:line cites, what YOU verified' },
    fix_plan: { type: 'string', description: 'if fixable_now: files, exact behavioral delta, proving test' },
    files: { type: 'array', items: { type: 'string' }, description: 'rustty files a fix would touch' },
    notes: { type: 'string', description: 'member-row nuances, corrections to the report' },
  },
}

const RUSTTY = '/Users/gael/projects/rustty'
const GHOSTTY = '/Users/gael/projects/ghostty'

function vetPrompt(g) {
  return `You are one agent in a fleet independently auditing reported parity gaps in Rustty — a full-fidelity Rust port of Ghostty (Zig). A less-capable Codex agent audited RUSTTY=${RUSTTY} against the read-only oracle GHOSTTY=${GHOSTTY} (pin 6ad1fe7d) and reported findings. Your job: independently CONFIRM or REFUTE one finding group from primary sources. Do NOT trust the report's claims — reproduce them yourself.

FINDING GROUP ${g.gid} (reported severity ${g.severity}): ${g.title}
Member registry rows: ${g.members.join(', ')}${g.dups && g.dups.length ? `; duplicate rows folded in: ${g.dups.join(', ')}` : ''}

Steps:
1. Read the registry rows: grep -n -- "${g.gid}" ${RUSTTY}/.port/codex/line-audit/waves/INDEX.md (and each member id) for status/replay context.
2. Find the detailed report(s): grep -rln -- "${g.gid}" ${RUSTTY}/.port/codex/ and read the relevant sections (they carry exact file:line citations, probes, commands). Reports are large — grep for the id, read surrounding context only.
3. Independently verify against CURRENT sources: read the cited Rustty code on current main and the Ghostty oracle code. The audit pinned Rustty at dc054bb; main has moved — the gap may already be fixed.
4. Prefer close reading of both implementations; where cheap and decisive, run a focused test (cargo test -p <crate> <filter> inside ${RUSTTY}). NEVER run full workspace builds or workspace-wide test sweeps.
5. Classify deferral: Rustty is a macOS-first port by locked plan (GTK/Linux-GUI/Windows/FreeBSD subsystems sequenced last). Check ${RUSTTY}/.port/phase-2/DEFERRALS.md, MAIN_PARITY.md, INTEGRATION_PARITY.md, GAP_ANALYSIS.md, and blueprint docs for whether this exact gap is a documented deliberate deferral; cite the doc in deferral_ref.
6. Decide fixable_now: true when a surgical, plan-consistent remediation exists (shared/VT/config/font/C-ABI behavior fixes, missing tests, small parity corrections, build-config toggles). false for whole-subsystem work (GTK rendering pipeline, Windows OS layer, FreeBSD event backends) — those are fix_category deferred-platform.
7. If fixable_now, write a concrete fix_plan: files to change, the exact behavioral delta vs the Ghostty oracle, and what test will prove it (tests may be added even where Ghostty has none).

Verdicts: confirmed = you reproduced the divergence yourself on current main. not_a_gap = the report is wrong or stale (say exactly why). already_fixed = current main no longer diverges. needs_deeper_evidence = you could not decide (say what is missing).
Be adversarial in BOTH directions: do not rubber-stamp the report, and do not rubber-stamp the port.
Do not modify any files anywhere — this pass is strictly read-only (running tests is fine).
Return ONLY the StructuredOutput; evidence <= 1500 chars with file:line cites.`
}

const A = typeof args === 'string' ? JSON.parse(args) : args
const groups = A.groups
log(`vetting ${groups.length} groups (tier ${A.tier})`)
const results = await parallel(
  groups.map((g) => () =>
    agent(vetPrompt(g), { label: `vet:${g.gid}`, phase: 'Vet', schema: SCHEMA })
  )
)
const ok = results.filter(Boolean)
log(`vet complete: ${ok.length}/${groups.length} verdicts returned`)
return { tier: A.tier, results: ok }
