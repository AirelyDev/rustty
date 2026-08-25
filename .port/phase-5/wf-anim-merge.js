export const meta = {
  name: 'p5-anim-merge',
  description: 'Phase-5: merge the B3+B4 animation/ScrollMargins patch onto the landed B1+B2 end state, oracle as arbiter',
  phases: [{ title: 'Merge', detail: 'one agent, 3-way + oracle arbitration + full battery' }],
}

const REPO = '/Users/gael/projects/rustty'
const REF = '/Users/gael/projects/ghostty'
const PATCHDIR = '/Users/gael/.claude/jobs/b740b914/tmp'

phase('Merge')
const r = await agent(`
You are a Phase-5 merge specialist for the rustty port. ISOLATED worktree of ${REPO}; FIRST:
git log --oneline -1 — if not b0685c0d, git reset --hard main. Oracle AT the pin 683d8db64:
${REF} — it is the ARBITER for every conflict.

SITUATION: two agents independently ported overlapping kitty end states against the SAME
oracle. The B1+B2 wave (spec-conformance + relative placements) is LANDED on your HEAD.
The B3+B4 patch (${PATCHDIR}/p5-kitty-anim.patch — ScrollMargins two-phase scroll guard +
graphics_animation/graphics_pixel + exec animation end state + terminal.rs scroll call
sites + the mirihar-scroll-margins harness) 3-way-conflicts with it in
graphics_{command,exec,image,storage}.rs (43 regions).

YOUR JOB:
1. git apply --3way ${PATCHDIR}/p5-kitty-anim.patch (expect the 4 UU files; everything else
   applies clean).
2. Resolve EVERY conflict by reading the ORACLE end state of the corresponding code
   (${REF}/src/terminal/kitty/*.zig — the working tree IS the target): where both sides
   ported the same oracle region, they should converge — pick the more faithful rendering
   (prefer the side whose comments/tests are more complete; where they genuinely differ,
   the oracle text decides). The anim side owns animation-specific structs/arms; the spec
   side owns Delete/lenient-parsing/relative-placement machinery. The exec file: spec left
   'unimplemented action' arms for animation — anim's real arms replace them.
3. Verify the UNION battery: cargo build --workspace; cargo test -p rustty-vt --lib FULL
   lane (expect >= 2751 + anim's additions, 0 failed) — spec's 197 kitty tests AND anim's
   B3 storage tests + animation/pixel tests must ALL pass (if a test from either side is
   missing, you dropped work — recover it); wuffs feature lane for pixel
   (cargo test -p rustty-vt --features wuffs <animation/pixel filters>); the
   mirihar-scroll-margins harness runs under cargo +nightly miri run (ported candidate
   clean, foil flagged); Miri on kitty::graphics_storage + graphics_exec filters.
4. .port/PORT_RULES.md binding (R1/R2/R7/R9, raw-ptr discipline).
HARVEST: git add -A && git diff --cached > ${PATCHDIR}/p5-anim-merged.patch (non-empty).
Report: summary, conflict-resolution notes (which side won where and why, oracle-cited),
test evidence (all lanes), tests_carried, caveats.`,
  { label: 'anim-merge', phase: 'Merge', schema: {
    type: 'object',
    required: ['patch_path', 'summary', 'resolution_notes', 'test_evidence', 'caveats'],
    properties: {
      patch_path: { type: 'string' }, summary: { type: 'string' },
      resolution_notes: { type: 'string' }, test_evidence: { type: 'string' },
      caveats: { type: 'string' },
    } }, isolation: 'worktree' })

return { merge: r }
