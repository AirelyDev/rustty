export const meta = {
  name: 'p5-par4-capi-ui',
  description: 'Phase-5 final porting wave: C-API bundles (P/C/E2/KG/sys) ∥ UI tiers (macOS, renderer, apprt)',
  phases: [{ title: 'Port', detail: 'w6-capi ∥ w7-ui' }],
}

const REPO = '/Users/gael/projects/rustty'
const REF = '/Users/gael/projects/ghostty'
const PATCHDIR = '/Users/gael/.claude/jobs/b740b914/tmp'

const COMMON = `
Phase-5 porter for the rustty port. ISOLATED worktree of ${REPO}; FIRST: git log --oneline -1
— if not 211ff7ef or newer, git reset --hard main. Oracle AT pin 683d8db64: ${REF}; range
12967b68f..683d8db64.

READ FIRST: your assessment (C-capi-lib.md / D-ui-tiers.md) in
${REPO}/.port/phase-5/assessments/; the plan
${REPO}/docs/superpowers/plans/2026-08-25-phase5-upstream-resync.md; .port/PORT_RULES.md.
On main: the COMPLETE Phase-5 vt mass (kitty graphics+animation+clipboard+DnD, OSC 99,
paste, dispatch — G1b GREEN) with W6/W7 PORT-NOTE seams that FAIL CLOSED (grep 'PORT NOTE'
+ W6/W7 in the files you touch; the dispatch commit 211ff7ef's message lists them).

BINDING: R1 end-state; R2; R7; R9 SAFETY; DG5 extern "C"; closed-enum c_int narrowing
(check_closed_enum_abi gates stay green); SB raw-ptr discipline for decoder/iterator
objects. Positional C-ABI enums: the embed-abi-byvalue-sret memory — header + KAT + gate
re-pin land in ONE motion. VERIFY: crate lanes + the header gates you touch.
HARVEST: git add -A && git diff --cached > ${PATCHDIR}/<PATCHNAME>.patch (non-empty).
Report per schema; every upstream test carried by name.
`

const SCHEMA = {
  type: 'object',
  required: ['patch_path', 'summary', 'test_evidence', 'tests_carried', 'caveats'],
  properties: {
    patch_path: { type: 'string' }, summary: { type: 'string' },
    files_created: { type: 'array', items: { type: 'string' } },
    files_modified: { type: 'array', items: { type: 'string' } },
    tests_carried: { type: 'array', items: { type: 'string' } },
    test_evidence: { type: 'string' }, caveats: { type: 'string' },
  },
}

phase('Port')
const results = await parallel([
  () => agent(COMMON + `
YOUR TASK (PATCHNAME p5-capi): ALL of assessment C — bundles P (terminal_paste streaming C
API + MimeReader + exports), C (kitty clipboard read/write C effects — the reply-based
ClipboardWrite REWORK; unlock the dispatch stage's fail-closed effect seams: clipboard_read
+ drag_and_drop effects wired live in rustty-vt-c), E2 (embed clipboard ABI: ghostty.h
content_s +len LAYOUT change + PRIMARY selection tag — rustty-embed.h advance + apprt-api/
apprt-c arms + positional-ordinals/A4 byte-parity gates re-pinned in the SAME patch), KG
(kitty graphics C reads: sourceRect helper, relative-placement resolve — unlock the vt-c
'reports not-visible' seam from tier-2a), sys.h + c/sys.zig (random_secure C option), and
the include/rustty/vt header additions for clipboard/dnd/paste. A5
check_rustty_vt_headers.sh: run BEFORE (expect arm (d) RED with exactly this range's new
exports) and AFTER (all arms GREEN; floors/registries re-floated to MEASURED values with
justification; types registry rows for every new C type). vt-c + vt + apprt-api + apprt-c
lanes green; Miri on new vt-c seams.`,
    { label: 'p5-capi', phase: 'Port', schema: SCHEMA, isolation: 'worktree' }),

  () => agent(COMMON + `
YOUR TASK (PATCHNAME p5-ui): ALL of assessment D — (1) macOS kitty-clipboard halves (D-B1,
11 commits: pasteboard integration, OTP prompts, the Surface completeClipboardRequest .kitty
arms incl. 928c7f0e7 readPromptExempt — the core seam is noted at surface.rs:12824; the
core Surface currently DROPS KittyClipboardRead/Write mailbox messages — wire them through
rustty-core to the apprt surface per the oracle); (2) renderer wave (D-B2 incl. the head
commit 683d8db64 GPU-resource release for hidden surfaces + the kitty animation clock —
drive rustty-vt's animation_tick from the renderer per the oracle's generic.zig:1388 shape,
both metal + linux paths); (3) placement-geometry renderer half (D-B3 — unlock tier-2a's
'skips relative placements' seam in rustty-renderer: resolve chains via the vt API); (4)
macOS menu-shortcut/KeyboardLayout wave (D-B5, 7 commits incl. the f6990690f revert — END
STATE; carry ScriptKeyEventCommandTests.swift + KeyboardLayoutTests.swift; R2 renames);
(5) apprt tail rows. VERIFY: cargo lanes for core/renderer/apprt*; xcodebuild build then
xcodebuild test -project macos/Rustty.xcodeproj (report pass counts; the menu-shortcut-
parity harness must stay green). GTK arms: hand-verify + note for the Linux finale.`,
    { label: 'p5-ui', phase: 'Port', schema: SCHEMA, isolation: 'worktree' }),
])

return { tasks: results.filter(Boolean) }
