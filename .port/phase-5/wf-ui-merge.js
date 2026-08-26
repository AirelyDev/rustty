export const meta = {
  name: 'p5-ui-merge',
  description: 'Phase-5: merge the W7 UI patch onto the landed W6 C-API end state, oracle as arbiter',
  phases: [{ title: 'Merge', detail: 'one agent, 3-way + oracle arbitration + full battery incl. xcodebuild' }],
}

const REPO = '/Users/gael/projects/rustty'
const REF = '/Users/gael/projects/ghostty'
const PATCHDIR = '/Users/gael/.claude/jobs/b740b914/tmp'

phase('Merge')
const r = await agent(`
Phase-5 merge specialist. ISOLATED worktree of ${REPO}; FIRST: git log --oneline -1 — if not
a7ddf8f0, git reset --hard main. Oracle AT pin 683d8db64: ${REF} — the ARBITER for every
conflict. Binding: .port/PORT_RULES.md.

SITUATION: the W6 C-API bundle (embed clipboard ABI E2, reply-based effects, paste C API)
is LANDED on your HEAD. The W7 UI patch (${PATCHDIR}/p5-ui.patch — macOS kitty-clipboard
halves + core Surface mailbox handlers + renderer GPU-release/animation-clock/relative-
placement + KeyboardLayout wave + gtk arms) was built INDEPENDENTLY against the pre-W6 base
and conflicts in 16 files / 54 regions — BOTH agents ported overlapping E2 halves
(apprt-api structs/seam, embedded.rs [20 regions], apprt-c KATs/smoke, core surface.rs,
rustty-apprt.h).

YOUR JOB:
1. git apply --3way ${PATCHDIR}/p5-ui.patch ; resolve EVERY conflict against the oracle end
   state (ghostty src/apprt/embedded.zig, include/ghostty.h, src/Surface.zig, macos/ at the
   pin). Where both sides rendered the same oracle region, converge on the more faithful
   one; W6's landed gate-pinned artifacts (rustty-embed.h, A3/A4 KATs, cbindgen pins) are
   AUTHORITATIVE unless the oracle contradicts them — if the UI side carries something the
   W6 header lacks, the oracle decides and the gates re-pin IN THIS PATCH.
2. The UI-only mass (macos/ Swift, renderer, KeyboardLayout tests, gtk, core mailbox
   handlers) applies clean — keep it all; zero W7 tests lost (inventory cross-check
   both sides' test lists).
3. VERIFY the union: cargo build --workspace; cargo test --workspace (0 failed; renderer
   88+10, core lanes, apprt-c ABI/smoke incl. smoke.c compile+link+run); gates
   check_rustty_h_abi + check_embed_h_abi + check_rustty_vt_headers + check_closed_enum_abi
   ALL GREEN; xcodebuild build then xcodebuild test -project macos/Rustty.xcodeproj (report
   counts; menu-shortcut-parity harness green). Note the Linux hand-verified arms for the
   finale.
HARVEST: git add -A && git diff --cached > ${PATCHDIR}/p5-ui-merged.patch (non-empty).
Report: summary, resolution_notes (oracle-cited per region family), test_evidence, caveats.`,
  { label: 'ui-merge', phase: 'Merge', schema: {
    type: 'object',
    required: ['patch_path', 'summary', 'resolution_notes', 'test_evidence', 'caveats'],
    properties: {
      patch_path: { type: 'string' }, summary: { type: 'string' },
      resolution_notes: { type: 'string' }, test_evidence: { type: 'string' },
      caveats: { type: 'string' },
    } }, isolation: 'worktree' })

return { merge: r }
