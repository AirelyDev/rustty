export const meta = {
  name: 'p4-gap-close',
  description: 'Phase-4 finale: close the last two code gaps (inspector ad96613a8, kitty f719af00c) + stream gating check',
  phases: [{ title: 'Close', detail: 'two agents, surgical' }],
}

const REPO = '/Users/gael/projects/rustty'
const REF = '/Users/gael/projects/ghostty'
const PATCHDIR = '/Users/gael/.claude/jobs/b740b914/tmp'

const COMMON = `
You are a Phase-4 gap-closure porter for the rustty port. ISOLATED worktree of ${REPO};
FIRST: git log --oneline -1 — if not 445239ad or newer, git reset --hard main.
Oracle AT the pin 12967b68f: ${REF}. Binding: ${REPO}/.port/PORT_RULES.md (R1 end-state
fidelity, R2 renames, R7 arithmetic, R9 SAFETY, SB raw-ptr discipline); the plan's Global
Constraints. VERIFY with cargo build + the touched crates' test lanes (0 failed).
HARVEST: git add -A && git diff --cached > ${PATCHDIR}/<PATCHNAME>.patch (non-empty).
`

const SCHEMA = {
  type: 'object',
  required: ['patch_path', 'summary', 'test_evidence', 'tests_carried', 'caveats'],
  properties: {
    patch_path: { type: 'string' },
    summary: { type: 'string' },
    tests_carried: { type: 'array', items: { type: 'string' } },
    test_evidence: { type: 'string' },
    caveats: { type: 'string' },
  },
}

phase('Close')
const results = await parallel([
  () => agent(COMMON + `
YOUR TASK (PATCHNAME gap-inspector): port ghostty ad96613a8 — inspector terminal-IO
copy/export — the LAST unported code in the Phase-4 range. Oracle:
${REF}/src/inspector/widgets/termio.zig lines ~95-400 (Copy/Export buttons in the control
row + copyEvents ~:350 + exportEvents ~:371 + formatEvents ~:396). Port into
crates/rustty-inspector/src/widgets/termio.rs (draw's control row currently ends at the
Clear button). The apprt-api action side ALREADY EXISTS (see
crates/rustty-apprt-api/src/action_tests.rs:406 and the export_terminal_io Action landed in
par-4/W5-E) — wire the widget to it the way the oracle wires its apprt action. Restore the
4 reference doc comments verbatim WITH the code (the G3b inspector row must go missing=0:
run python3 .port/tools/gates/check_comment_coverage.py --crate inspector at the end and
report the line). Carry every test hunk of ad96613a8. cargo test -p rustty-inspector green.`,
    { label: 'gap-inspector', phase: 'Close', schema: SCHEMA, isolation: 'worktree' }),

  () => agent(COMMON + `
YOUR TASK (PATCHNAME gap-kv): port ghostty f719af00c + 5ce1fe1ff — the kitty graphics
command KV DENSE TABLE that fell between two par-2 agents (each thought the other owned
it). Oracle: ${REF}/src/terminal/kitty/graphics_command.zig (the Kv struct ~line 31: dense
[52]u32 value table + presence bitmap replacing the HashMap<u8,u32>, arena removal, and
5ce1fe1ff's doc). rustty: crates/rustty-vt/src/kitty/graphics_command.rs still has the
HashMap Kv (verify first). Port the END STATE + the test "ignore unknown keys (non-letter)"
+ every amended test body + 5ce1fe1ff's doc comment. R7 for any index arithmetic. Also ONE
small check the comment sweep flagged: crates/rustty-vt/src/stream.rs next_slice_untracked
gates its batched path on debug||!SIMD while the PIN gates on debug alone (non-SIMD builds
take the batched path via scalar fallbacks) — read the pin's stream.zig gating and align the
port's cfg gate to it (the restored comment at that site describes the pin behavior; make
the code match, minimal change). cargo test -p rustty-vt --lib kitty:: and stream:: filters
+ full vt lane green.`,
    { label: 'gap-kv', phase: 'Close', schema: SCHEMA, isolation: 'worktree' }),
])

return { tasks: results.filter(Boolean) }
