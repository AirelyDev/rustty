export const meta = {
  name: 'p5-par3-clipboard-dnd',
  description: 'Phase-5 tier 3: OSC 5522 clipboard vt-side ∥ OSC 72 DnD vt-side → unified stream/Surface/termio dispatch',
  phases: [{ title: 'Port', detail: 'clipvt ∥ dnd' }, { title: 'Dispatch', detail: 'single stream_terminal owner' }],
}

const REPO = '/Users/gael/projects/rustty'
const REF = '/Users/gael/projects/ghostty'
const PATCHDIR = '/Users/gael/.claude/jobs/b740b914/tmp'

const COMMON = `
Phase-5 porter for the rustty port. ISOLATED worktree of ${REPO}; FIRST: git log --oneline -1
— if not 4e7dce65 or newer, git reset --hard main. Oracle AT pin 683d8db64: ${REF}; range
12967b68f..683d8db64.

READ FIRST: assessments A (clusters B5/B6/B7) + B (cluster B-1) + E (bundle KC) in
${REPO}/.port/phase-5/assessments/; the plan (traps: 5350d4a5f truncation + 4 tests REVERTED
by 70f006575 — reject-not-truncate end state, limit plumbing survives; 7a940ec02 drop-chunk
REVERTED by bc2f7d7d2 — abort, 3 tests renamed dropped->reported; hazard #5 WriteState owns
its simd Streaming decoder state). On main: W1 (simd base64 Streaming, paste core with
'W3 seam' markers, sys random_secure + io_failing/io_native stand-in, config
clipboard_write_limit) and tier-2 (kitty graphics complete, OSC 99).

BINDING: R1 end-state; R2; R7; R9; DG5; SB raw-ptr discipline. VERIFY: crate lanes green.
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
const [clip, dnd] = await parallel([
  () => agent(COMMON + `
YOUR TASK (PATCHNAME p5-clipvt): the OSC 5522 kitty clipboard VT SIDE — assessment A B5 (10
commits, 61 named end-state tests) + B7's PasteEvent slice + B B-1's vt-file rows: NEW
${REF}/src/terminal/kitty/clipboard{,_command,_grants,_response,_write}.zig ->
crates/rustty-vt/src/kitty/clipboard*.rs, the osc clipboard-parser one-line touch
(5350d4a5f survivor), grants/generateOtp over the W1 sys::random_secure seam (the sys.rs
'Io' stand-in is the DECIDED mapping — read its NON_PORTS re-read note), WriteState over
the W1 simd Streaming decoder (owned state, per-call &mut output, EOF-at-padding restart
tests), and FILL the 4 'W3 seam' markers in crates/rustty-vt/src/input/paste.rs (grep
'W3 seam': KittyClipboard enum inhabitants, kitty_paste_events mode conjunct with its
cfg-not-folded build predicate, paste_kitty_event body, the deferred test). Wire the
kitty/mod re-exports. DO NOT touch stream_terminal.rs / Surface / termio (the Dispatch
stage owns them — leave your handler/effect types pub and PORT-NOTE the dispatch seam).
Full vt lane green (expect large kitty test growth toward the 334 oracle count).`,
    { label: 'p5-clipvt', phase: 'Port', schema: SCHEMA, isolation: 'worktree' }),

  () => agent(COMMON + `
YOUR TASK (PATCHNAME p5-dnd): the OSC 72 kitty drag-and-drop VT SIDE — assessment A B6
(4 commits + doc supersede, 43 tests incl. the dnd_test kitten-0.47 replay): NEW
${REF}/src/terminal/kitty/dnd{,_command,_drop,_response}.zig + dnd_test.zig ->
crates/rustty-vt/src/kitty/dnd*.rs, ATOMIC WITH gutting
crates/rustty-vt/src/osc/parsers/kitty_dnd_protocol.rs to the 138-line oracle end state
(6 tests migrate; the pin-era in-parser state machine must NOT coexist with the new one —
50f69b883). DO NOT touch stream_terminal.rs / Surface / termio (Dispatch stage owns them;
leave handler/effect types pub + PORT-NOTE seams) and DO NOT touch clipboard* files
(sibling owns them). Full vt lane green.`,
    { label: 'p5-dnd', phase: 'Port', schema: SCHEMA, isolation: 'worktree' }),
])

phase('Dispatch')
const dispatch = (clip && dnd) ? await agent(COMMON + `
YOUR TASK (PATCHNAME p5-dispatch): the unified dispatch stage — you are the SINGLE OWNER of
stream_terminal.rs/Surface/termio for this tier. FIRST apply both sibling patches:
git apply --3way ${clip.patch_path} && git apply --3way ${dnd.patch_path}
(same base; resolve trivial overlaps — they own disjoint files).
Then port the END STATE of: (1) stream_terminal.rs clipboard + DnD dispatch (the G1b
stream_terminal drop=38 — the assessment attributes the bulk to 4f49dc2b8/af8d28a94/
bc2f7d7d2/e8d8945b5/928c7f0e7/5350d4a5f-survivors/70f006575 — and parser drop=1), incl.
the e03475c0c osc terminator capture the osc99 wave PORT-NOTE'd ('W3 seam' in stream.rs
tests); (2) Surface.zig arms (bc2f7d7d2, 928c7f0e7); (3) termio stream_handler halves +
assessment E bundle KC's termio rows (the config half landed in W1). The C-API effects and
macOS/apprt halves are W6/W7 — leave PORT-NOTE seams compiling (apprt-api arms with notes
if an exhaustive match forces it, per the Phase-4 precedent). VERIFY: full vt lane + termio
+ core lanes green; run python3 .port/tools/gates/check_test_counts.py and report the
kitty/stream_terminal/parser rows (targets: all green or name exactly what remains for
W6/W7). Miri on the new dispatch seams if any raw aliasing was added.`,
  { label: 'p5-dispatch', phase: 'Dispatch', schema: SCHEMA, isolation: 'worktree' }) : null

return { tasks: [clip, dnd, dispatch].filter(Boolean) }
