export const meta = {
  name: 'p5-g3b-sweep',
  description: 'Phase-5 finale: restore the remaining missing reference comments (G3b R1 tail, vt 174 + termio 1)',
  phases: [{ title: 'Sweep', detail: 'one agent, comments only unless a sentence reveals unported code' }],
}

const REPO = '/Users/gael/projects/rustty'
const REF = '/Users/gael/projects/ghostty'
const PATCHDIR = '/Users/gael/.claude/jobs/b740b914/tmp'

phase('Sweep')
const r = await agent(`
Phase-5 comment-fidelity sweeper. ISOLATED worktree of ${REPO}; FIRST: git log --oneline -1
— if not 4e9d5e10, git reset --hard main. Oracle AT pin 683d8db64: ${REF}.

G3b reports 175 missing reference comments (vt 174, termio 1 — R1 violations). The gate
truncates its listing at 25 per crate; ${PATCHDIR}/g3b-p5-missing.txt has the first page.
Recover the FULL enumeration the Phase-4 way: a read-only scratch reimplementation of
${REPO}/.port/tools/gates/check_comment_coverage.py's matching logic (write it under /tmp,
NEVER modify the gate or its data) with oracle file:line attribution.

Per missing sentence: find it in ${REF}/src (full comment block + context), find the ported
code, restore the comment FAITHFULLY (R2 renames; R7 comment rule where the primitive
diverges; verbatim by default). If a sentence documents genuinely UNPORTED CODE, do not
fake a match — report it under unresolvable with the owning upstream commit (the Phase-4
precedent: 4 such sentences exposed a real gap). Diff must be comment-only (verify: every
changed line is a comment line) UNLESS you hit unresolvable-code cases, which you report
rather than port. Rerun the gate per crate at the end — target missing=0 for both, or
missing=<n> with every n itemized as unresolvable.
cargo build -p rustty-vt -p rustty-termio must stay green.
HARVEST: git add -A && git diff --cached > ${PATCHDIR}/p5-g3b-sweep.patch (non-empty).`,
  { label: 'g3b-sweep', phase: 'Sweep', schema: {
    type: 'object',
    required: ['patch_path', 'summary', 'gate_lines', 'unresolvable', 'caveats'],
    properties: {
      patch_path: { type: 'string' }, summary: { type: 'string' },
      gate_lines: { type: 'array', items: { type: 'string' } },
      unresolvable: { type: 'array', items: { type: 'string' } },
      caveats: { type: 'string' },
    } }, isolation: 'worktree' })

return { sweep: r }
