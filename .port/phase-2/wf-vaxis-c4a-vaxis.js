export const meta = {
  name: 'vaxis-c4a-vaxis',
  description: 'vaxis chunk 4a — port Vaxis.zig (the main object + render diff-and-emit, 1411 LOC), filling the C3b inhabited placeholder: design -> adversary -> fold -> port -> verify',
  phases: [
    { title: 'Design', detail: 'faithful-port design; fill the loop_ Vaxis placeholder; render diff core; zigimg R5 seams' },
    { title: 'Adversary', detail: 'attack the render diff-and-emit fidelity (Vaxis has 0 unit tests)' },
    { title: 'Fold', detail: 'fold fixes' },
    { title: 'Port', detail: 'port Vaxis + fill the placeholder' },
    { title: 'Verify', detail: 'gates + R1 fidelity (render escapes) + Miri' },
  ],
}

const R = '/Users/gael/projects/rustty'
const VX = '/Users/gael/.cache/zig/p/vaxis-0.5.1-BWNV_LosCQAGmCCNOLljCIw6j6-yt53tji6n6rwJ2BhS'
const DESIGN = `${R}/.port/phase-2/VAXIS_C4A_DESIGN.md`

const rules = `
Full-fidelity Zig->Rust port (rustty). vaxis chunk 4a: Vaxis.zig — the main object + the render diff-and-emit engine. Source (read-only): vaxis 0.5.1 ${VX}/src/Vaxis.zig (1411 LOC, 0 test{}). Target: crates/rustty-vaxis (chunks 1a/1b/2/3a/3b landed). Deps ONLY intra-crate: cell/key/mouse/screen/internal_screen/window/tty/ctlseqs/gwidth/unicode/lib(Winsize) + zigimg (for Image render — DEFERRED). Chunk 3b left an INHABITED Vaxis placeholder in loop_.rs (caps/state/screen/query_futex/queries_done/opts fields + unimplemented!() translate_mouse/device_status_report leaves); C4a REPLACES that placeholder with the REAL Vaxis, preserving its field/method surface so handle_event_generic still compiles (intrusive-ptr-and-placeholder-fill discipline).
SCOPE: the Vaxis struct (caps/Capabilities, screen: Screen, screen_last: InternalScreen, state, unicode, opts, the query futex/queries_done, mouse), init/deinit, enterAltScreen/exitAltScreen, queryTerminal/deviceStatusReport (the capability-detection query sequence), RENDER (the core: diff screen vs screen_last cell-by-cell, emit the minimal cursor-move + SGR + text escape sequences to the tty writer via ctlseqs — this is the fidelity heart), window() (returns the root Window over screen), resize, setTitle/setMouseShape/setMouseMode/copyToSystemClipboard/subscribeToColorSchemeUpdates, prettyPrint helpers. list_themes uses exactly: window/render/queryTerminal/enterAltScreen/deinit/caps/resize/setTitle/setMouseShape/setMouseMode/copyToSystemClipboard/subscribeToColorSchemeUpdates.
zigimg/Image render path = R5 unimplemented!() seams + verbatim body (list_themes/boo use no images).
R1: EVERY line/comment ported (Vaxis.zig has 0 test{} — fidelity is by faithful translation + the adversary's render-diff re-check + a feasible ADDED render-to-Vec<u8>-buffer smoke test asserting emitted escapes). Byte-slices -> &[u8]/Vec<u8>, never lossy String. R4 acyclic. R6: the render logic is pure (writes to a generic writer) -> Miri where possible; tty writes -> native. Gates: build/test/clippy -D/fmt/check_acyclic.sh/check_test_counts.py/check_comment_coverage.py + Miri.`

const D_SCHEMA = { type:'object', additionalProperties:false, required:['disposition','placeholder_fill_ok'], properties:{
  doc_path:{type:'string'}, disposition:{type:'string',description:"'ready' or 'blocked: <reason>'"}, placeholder_fill_ok:{type:'boolean',description:'true if the real Vaxis preserves the C3b loop_ placeholder field/method surface so handle_event_generic still compiles'},
  summary:{type:'string'}, key_decisions:{type:'array',items:{type:'string'}}, risks:{type:'array',items:{type:'string'}} } }

phase('Design')
const design = await agent(
  `${rules}\n\nDESIGN PHASE. Write ${DESIGN}. Read ${VX}/src/Vaxis.zig in full + the current crates/rustty-vaxis/src/loop_.rs Vaxis placeholder (its exact field/method surface) + the Screen/InternalScreen/Window/Tty/ctlseqs APIs. Map Vaxis.zig -> vaxis.rs; pin the RENDER diff-and-emit engine (the cell-by-cell diff vs screen_last, cursor-move/SGR/text emission, the hyperlink/image/cursor handling — the fidelity heart), the capability-query sequences (queryTerminal/deviceStatusReport), the placeholder REPLACEMENT (confirm the real Vaxis's fields/methods are a superset of loop_'s placeholder so handle_event_generic compiles unchanged — set placeholder_fill_ok), and the zigimg/Image R5 seams. Specify a feasible ADDED render-to-Vec<u8> smoke test (render a small known Screen state, assert the emitted escape bytes) since Vaxis.zig has no unit tests. If a genuine blocker, disposition 'blocked' + STOP. Else 'ready'. Return the object.`,
  { label:'design:vaxis-c4a', phase:'Design', agentType:'general-purpose', schema:D_SCHEMA })

if (!design || (design.disposition||'').toLowerCase().startsWith('blocked')) {
  log(`VAXIS-C4a DESIGN BLOCKED: ${design ? design.disposition : 'design failed'}`)
  return { blocked:true, design }
}

phase('Adversary')
const ADV_SCHEMA = { type:'object', additionalProperties:false, required:['verdict','fixes','blocking_issues'], properties:{
  verdict:{enum:['SOUND','SOUND_WITH_FIXES','UNSOUND']}, fixes:{type:'array',items:{type:'string'}}, blocking_issues:{type:'array',items:{type:'string'}} } }
const adversary = await agent(
  `${rules}\n\nADVERSARY PHASE. Design at ${DESIGN}. Vaxis.zig has ZERO unit tests, so BREAK the design by re-reading Vaxis.zig against the plan: (1) the RENDER diff engine — does the design reproduce EVERY branch (the cursor-position tracking, the SGR-state minimization/only-emit-on-change, the default-vs-set cell diff, wide-char/skip handling, hyperlink OSC-8, the cursor show/hide + shape, the mouse-shape emission, the link_state)? a dropped branch silently corrupts output with no test to catch it. (2) queryTerminal/deviceStatusReport capability query byte sequences exact? (3) does the real Vaxis preserve the loop_ placeholder's EXACT field/method surface (so handle_event_generic + winsize_callback compile unchanged)? (4) enterAltScreen/exitAltScreen/deinit escape sequences + teardown order faithful? (5) are the zigimg/Image seams correctly R5 (not silently dropping non-image render logic)? (6) is the ADDED render smoke test meaningful (asserts real emitted escapes, not trivial)? (7) no lossy String? List every fix + blocker. verdict SOUND only if the render engine is provably faithful.`,
  { label:'adversary:vaxis-c4a', phase:'Adversary', agentType:'Explore', schema:ADV_SCHEMA })

const needsFold = adversary && ((adversary.fixes && adversary.fixes.length) || (adversary.blocking_issues && adversary.blocking_issues.length))
if (needsFold) {
  phase('Fold')
  await agent(
    `${rules}\n\nFOLD PHASE. Adversary issues on ${DESIGN}. Fixes: ${JSON.stringify(adversary.fixes)}. Blocking: ${JSON.stringify(adversary.blocking_issues)}. Update ${DESIGN} IN PLACE to fix every one (re-read Vaxis.zig as needed). If a blocker is genuine add a top 'DISPOSITION: blocked' line. Return a short summary.`,
    { label:'fold:vaxis-c4a', phase:'Fold', agentType:'general-purpose' })
}

phase('Port')
const P_SCHEMA = { type:'object', additionalProperties:false, required:['disposition','files_changed','build_ok','miri_clean','notes'], properties:{
  disposition:{type:'string'}, files_changed:{type:'array',items:{type:'string'}}, build_ok:{type:'boolean'}, miri_clean:{type:'boolean'}, notes:{type:'string'} } }
const port = await agent(
  `${rules}\n\nPORT PHASE. Implement per ${DESIGN} — if 'DISPOSITION: blocked', STOP. Port Vaxis.zig -> crates/rustty-vaxis/src/vaxis.rs, REPLACING the loop_.rs Vaxis placeholder with the real Vaxis (preserve its field/method surface so handle_event_generic + winsize_callback still compile; move the placeholder's leaves into the real methods). Preserve every comment (R1). The render diff engine is the core — port it branch-for-branch. zigimg/Image render = R5 seams. Add the render-to-Vec<u8> smoke test. Update lib.rs PORT STATUS + the loop_ Loop test (may un-ignore if Vaxis now supports it, else keep #[ignore]). Build \`cargo build -p rustty-vaxis\`, \`cargo test -p rustty-vaxis\`, \`cargo +nightly miri test -p rustty-vaxis\`. Report files_changed, build_ok, miri_clean, notes (incl. the render engine fidelity + which methods are R5 seams + whether the Loop test un-ignored). Do NOT git commit.`,
  { label:'port:vaxis-c4a', phase:'Port', agentType:'general-purpose', schema:P_SCHEMA })

if (!port || (port.disposition||'').toLowerCase().startsWith('stopped')) {
  log(`VAXIS-C4a PORT STOPPED`)
  return { design, adversary, port, verify:null }
}

phase('Verify')
const V_SCHEMA = { type:'object', additionalProperties:false, required:['dimension','verdict','findings'], properties:{
  dimension:{type:'string'}, verdict:{enum:['PASS','FAIL']}, findings:{type:'array',items:{type:'string'}} } }
const verify = await parallel([
  () => agent(`${rules}\n\nVERIFY (GATES+MIRI). Independently re-run + confirm GREEN: cargo build --workspace; cargo test -p rustty-vaxis; cargo +nightly miri test -p rustty-vaxis; cargo clippy -p rustty-vaxis --all-targets -- -D warnings; cargo fmt --check; bash .port/tools/gates/check_acyclic.sh; python3 .port/tools/gates/check_test_counts.py; python3 .port/tools/gates/check_comment_coverage.py. Report each. verdict PASS only if ALL green.`,
    { label:'verify:gates', phase:'Verify', agentType:'Explore', schema:V_SCHEMA }),
  () => agent(`${rules}\n\nVERIFY (R1 + RENDER FIDELITY). Diff rustty-vaxis vaxis.rs vs ${VX}/src/Vaxis.zig: (1) every comment verbatim? (2) the RENDER diff-and-emit engine faithful branch-for-branch (cursor tracking, SGR minimization, cell diff, wide-char, hyperlink OSC-8, cursor/mouse shape) — spot-check the emitted escapes for a small state via the added smoke test? (3) queryTerminal/deviceStatusReport + enterAltScreen/deinit sequences exact? (4) the real Vaxis preserves the loop_ placeholder surface (handle_event_generic compiles unchanged)? (5) zigimg/Image seams R5 (not dropping non-image logic)? (6) no lossy String? R4 acyclic? Report findings; verdict PASS only if the render engine is faithful.`,
    { label:'verify:fidelity', phase:'Verify', agentType:'Explore', schema:V_SCHEMA }),
])

return { design, adversary, port, verify }
