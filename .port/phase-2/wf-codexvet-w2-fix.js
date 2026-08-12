export const meta = {
  name: 'codexvet-w2-fix',
  description: 'Execute the wave-2 platform remediations (Windows OS layer, FreeBSD, Linux GL images) from their hardened design docs',
  whenToUse: 'args={wave, lanes:[{lane, batches:[{bid, design, section, findings, files, note}]}]}',
  phases: [{ title: 'Implement' }, { title: 'Verify' }],
}

const RUSTTY = '/Users/gael/projects/rustty'
const GHOSTTY = '/Users/gael/projects/ghostty'
const DESIGNS = `${RUSTTY}/.port/codex-vet/designs`

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
          unverifiable_residue: { type: 'string', description: 'what could not be proven here and why (e.g. needs a real Windows host)' },
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

const CONTEXT = `Rustty is a full-fidelity Rust port of Ghostty (Zig). RUSTTY=${RUSTTY} (work in place on main), read-only oracle GHOSTTY=${GHOSTTY} (pin 6ad1fe7d — NEVER modify it).
WAVE 2 CONTEXT: these are the platform gaps the macOS-first plan had deliberately deferred (Windows OS layer, FreeBSD enablement, Linux GPU image rendering). The user has explicitly directed that they be fixed NOW, accepting that some cannot be fully verified on this host — implement them faithfully and state the unverifiable residue honestly rather than skipping the work.
Verification available here: macOS host (cargo test); \`cargo check --target x86_64-pc-windows-msvc\` and \`--target x86_64-unknown-freebsd\` (rust-std installed; check-only, no system libs); and a Fedora 44 aarch64 Linux VM at \`ssh rvm\` (GTK 4.22.4, cargo) — rsync the tree to rvm:~/rustty-vmwork, see .port/tools/gates/ws19_portal_openuri_gate.sh for the established pattern. NEVER git commit or push.
Findings evidence lives in ${RUSTTY}/.port/codex-vet/STATE.json (\`.findings[] | select(.id=="<gid>") | .vet\`).
Repo doctrine: .port/PORT_RULES.md. Zig byte-slices map to Vec<u8>/&[u8], never lossy String. Other lanes edit other files concurrently — touch only your batch's files; re-read shared ledger docs immediately before editing them and keep hunks narrow.
The repo's HARD gate is \`cargo clippy --workspace --all-targets -- -D warnings\` — do not introduce lints (no blanket allows; if a lint is wrong for parity-faithful code, scope the allow narrowly and justify it in a comment).`

function fixPrompt(b) {
  const list = b.findings.map((f) => `- ${f.gid}: ${f.title}`).join('\n')
  return `${CONTEXT}

BATCH ${b.bid} — implement ${b.section} of the design doc ${DESIGNS}/${b.design}.

Findings this closes:
${list}
${b.note ? `\nBATCH NOTE: ${b.note}\n` : ''}
Steps:
1. READ ${DESIGNS}/${b.design} — at minimum the section(s) named above plus the doc's preamble/verification sections. The doc was written against the oracle and then adversarially reviewed and corrected; it names exact files, oracle cites at the pin, cfg strategy, porting order, and per-item check commands. FOLLOW IT.
2. The doc's line numbers were captured before ~130 other fixes landed. RE-DERIVE anchors by content (grep for the named symbol/comment), never by trusting a line number. If the doc's premise no longer matches the tree, say so and adapt with the oracle as arbiter.
3. Read the cited Ghostty oracle code yourself before writing each piece. Fidelity to the oracle's behavior is the acceptance bar.
4. Add proving tests wherever the platform allows one to run here (host-runnable logic tests are strongly preferred over cfg-gated tests nobody can execute). Where the proof genuinely requires the target OS, run the closest available check (cross \`cargo check --target ...\`, or the Linux VM) and record precisely what remains unproven in \`unverifiable_residue\`.
5. Run the doc's stated per-item check commands and report their real output. Never run whole-workspace test sweeps; scoped \`cargo test -p <crate>\` / \`cargo check --target <triple>\` only.
6. If an item cannot be completed safely, mark it blocked with the reason and revert your partial edits for it — never leave the tree broken.

Return the StructuredOutput, one outcome per gid.`
}

function verifyPrompt(b, fixResult) {
  const list = (b.findings || []).map((f) => `- ${f.gid}: ${f.title}`).join('\n')
  return `${CONTEXT}

You are an INDEPENDENT ADVERSARIAL VERIFIER for wave-2 batch ${b.bid} (${b.section} of ${DESIGNS}/${b.design}):
${list}
Fix agent's summary: ${String(fixResult.summary || '').slice(0, 900)}
Its test claims: ${String(fixResult.tests_run || '').slice(0, 600)}
Its declared unverifiable residue: ${(fixResult.outcomes || []).map((o) => `${o.gid}: ${o.unverifiable_residue || '(none declared)'}`).join(' | ').slice(0, 600)}

Verify skeptically, for each gid:
1. Read the design section and the finding's \`.vet\` in STATE.json, then the ACTUAL diff (\`git diff -- <files>\`, \`git status\` for new files).
2. FIDELITY: read the cited oracle code YOURSELF at pin 6ad1fe7d. Does the new code match its behavior — error kinds, ordering, boundary values, cfg gating, ABI/layout? Platform code is easy to get subtly wrong (struct field widths, constant values, syscall flags): check the actual platform headers/std source where the design cites them, don't take the doc's word.
3. RUN the checks: the cross-target \`cargo check\` for the platform in question, the crate's scoped tests on macOS, and the Linux VM (\`ssh rvm\`) where the code is Linux-live. Report real output.
4. HONESTY AUDIT — this matters most in wave 2: is the fix agent's claimed verification real, and is its declared \`unverifiable_residue\` complete? A fix that type-checks but cannot possibly work at runtime, presented as done, is a FAIL. Equally, do not fail an item merely because it cannot be executed on this host — fail it if the claim overstates what was actually proven.
5. Confirm the repo's clippy gate is not reddened by these files, and no unrelated damage (including shared ledger docs — a doc edit must not delete or orphan a neighbouring section).

verified=true only when fidelity holds, the checks that CAN run pass, and the residue is stated accurately. Otherwise verified=false with concrete, actionable problems. Do NOT fix anything yourself. Return the StructuredOutput.`
}

const A = typeof args === 'string' ? JSON.parse(args) : args
const lanes = A.lanes
log(`wave ${A.wave}: ${lanes.length} lanes, ${lanes.reduce((n, l) => n + l.batches.length, 0)} batches`)

const results = await parallel(
  lanes.map((lane) => async () => {
    const out = []
    for (const b of lane.batches) {
      const fixResult = await agent(fixPrompt(b), { label: `w2fix:${b.bid}`, phase: 'Implement', schema: FIX_SCHEMA })
      if (!fixResult) { out.push({ bid: b.bid, fix: null, verdicts: [], summary: 'fix agent died' }); continue }
      const anyFixed = (fixResult.outcomes || []).some((o) => o.status === 'fixed' || o.status === 'partial')
      if (!anyFixed) { out.push({ bid: b.bid, fix: fixResult, verdicts: [], summary: 'nothing changed; verify skipped' }); continue }
      const v = await agent(verifyPrompt(b, fixResult), { label: `w2ver:${b.bid}`, phase: 'Verify', schema: VERIFY_SCHEMA })
      out.push({ ...(v || { verdicts: [], summary: 'verifier died' }), bid: b.bid, fix: fixResult })
    }
    return { lane: lane.lane, batches: out }
  })
)

log(`wave ${A.wave} complete`)
return { wave: A.wave, lanes: results.filter(Boolean) }
