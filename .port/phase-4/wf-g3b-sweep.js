export const meta = {
  name: 'p4-g3b-sweep',
  description: 'Phase-4 finale: restore the remaining missing reference comments (G3b R1 tail)',
  phases: [{ title: 'Sweep', detail: 'two agents, comments only' }],
}

const REPO = '/Users/gael/projects/rustty'
const REF = '/Users/gael/projects/ghostty'
const PATCHDIR = '/Users/gael/.claude/jobs/b740b914/tmp'

const COMMON = `
You are a Phase-4 comment-fidelity sweeper for the rustty port. You run in an ISOLATED git
worktree of ${REPO}; FIRST: git log --oneline -1 — if not 47b2dbbb or newer, git reset --hard main.

G3b (${REPO}/.port/tools/gates/check_comment_coverage.py) reports MISSING reference comments:
oracle sentences (from ${REF}/src, the working tree AT the pin) with no match in the port —
an R1 violation (comments are NEVER dropped or summarized away). The enumeration is in
${PATCHDIR}/g3b-missing.txt (per-crate blocks; each MISSING line is a normalized sentence
fragment).

METHOD, per missing sentence in YOUR crates:
1. Find the sentence in the oracle: grep -rn -i "<distinctive fragment>" ${REF}/src (it is a
   comment line there; read its full surrounding comment block).
2. Find the corresponding ported code in your crate (the fn/type the oracle comment documents).
3. Restore the comment FAITHFULLY at that location (R2-rename tokens; a comment may only be
   updated where it names a ghostty-specific identifier; meaning never changes). If the ported
   code deliberately diverges such that the comment describes the OLD primitive, follow R7's
   comment rule: rewrite to the Rust primitive's contract AND note it — but the default is
   verbatim restoration.
4. If a sentence is genuinely inapplicable (describes Zig machinery with a settled N-A port
   fork), do NOT fake a match: record it in your report — the orchestrator decides. Do not
   touch the gate or its data files. COMMENTS ONLY — zero behavior changes (the diff must be
   comment-only; verify with git diff -w --stat sanity + cargo build of touched crates).
5. Rerun: python3 .port/tools/gates/check_comment_coverage.py --crate <c> for each of your
   crates — target missing=0; report the final per-crate lines.

HARVEST: git add -A && git diff --cached > ${PATCHDIR}/<PATCHNAME>.patch ; confirm non-empty.
`

const SCHEMA = {
  type: 'object',
  required: ['patch_path', 'summary', 'gate_lines', 'unresolvable', 'caveats'],
  properties: {
    patch_path: { type: 'string' },
    summary: { type: 'string' },
    gate_lines: { type: 'array', items: { type: 'string' } },
    unresolvable: { type: 'array', items: { type: 'string' } },
    caveats: { type: 'string' },
  },
}

phase('Sweep')
const results = await parallel([
  () => agent(COMMON + `\nYOUR CRATES (PATCHNAME g3b-vt): vt (54 missing) and vt-c (27 missing) —
crates/rustty-vt and crates/rustty-vt-c. The bulk will be in the Phase-4-ported regions
(snapshot/, kitty, formatter, stream, render C surfaces).`,
    { label: 'g3b-vt', phase: 'Sweep', schema: SCHEMA, isolation: 'worktree' }),
  () => agent(COMMON + `\nYOUR CRATES (PATCHNAME g3b-rest): inspector (4), os (3), terminfo (2),
termio (2), args (1) — crates/rustty-{inspector,os,terminfo,termio,args}.`,
    { label: 'g3b-rest', phase: 'Sweep', schema: SCHEMA, isolation: 'worktree' }),
])

return { tasks: results.filter(Boolean) }
