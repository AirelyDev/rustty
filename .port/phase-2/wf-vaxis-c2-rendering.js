export const meta = {
  name: 'vaxis-c2-rendering',
  description: 'vaxis chunk 2 — port the rendering layer (Screen/InternalScreen/Window, ~1095 LOC, 9 test{}) into rustty-vaxis: design -> port -> verify(+Miri)',
  phases: [
    { title: 'Design', detail: 'faithful-port design for Screen/InternalScreen/Window + STOP-guard' },
    { title: 'Port', detail: 'port the rendering layer + 9 tests' },
    { title: 'Verify', detail: 'gates + R1 fidelity + Miri' },
  ],
}

const R = '/Users/gael/projects/rustty'
const VX = '/Users/gael/.cache/zig/p/vaxis-0.5.1-BWNV_LosCQAGmCCNOLljCIw6j6-yt53tji6n6rwJ2BhS'
const DESIGN = `${R}/.port/phase-2/VAXIS_C2_DESIGN.md`

const rules = `
Full-fidelity Zig->Rust port (rustty). vaxis chunk 2: the RENDERING layer. Source (read-only): vaxis 0.5.1 ${VX}/src/{Screen,InternalScreen,Window}.zig. Target: crates/rustty-vaxis (chunks 1a value types + 1b uucode/gwidth/unicode already landed, PORT STATUS done). Chunk 2 deps ONLY intra-crate modules: cell (Cell/Style/Color/Segment/Character), mouse, gwidth, unicode, image (shape), lib (Winsize) — all present.
SCOPE: Screen.zig (73 LOC, 1 test — the flat cell buffer: init/deinit/writeCell/readCell + width/height), InternalScreen.zig (133 — the allocating/back buffer for diffing), Window.zig (889, 8 test — the window view: child/initChild, writeCell/readCell, print/printSegment[gwidth-aware grapheme layout], fill, clear, showCursor, gwidth methods, hyperlink). ~9 test{} — the fidelity contract (Window's print/layout tests exercise the c1b gwidth/unicode).
R1: EVERY line/comment/test{} ported assertion-for-assertion; byte-slices -> &[u8]/Vec<u8>, never lossy String (cell grapheme bytes). Any method needing a LATER chunk (Vaxis/Tty/Loop/render/Image draw) = R5 unimplemented!() seam + verbatim body. R4 acyclic (rustty-vaxis stays a leaf). R6: PURE -> Miri primary. Gates: build/test/clippy -D/fmt/check_acyclic.sh/check_test_counts.py/check_comment_coverage.py + Miri.`

const D_SCHEMA = { type:'object', additionalProperties:false, required:['doc_path','disposition','summary','key_decisions','risks'], properties:{
  doc_path:{type:'string'}, disposition:{type:'string',description:"'ready' or 'blocked: <reason>'"}, summary:{type:'string'}, key_decisions:{type:'array',items:{type:'string'}}, risks:{type:'array',items:{type:'string'}} } }

phase('Design')
const design = await agent(
  `${rules}\n\nDESIGN PHASE. Write ${DESIGN}. Read ${VX}/src/{Screen,InternalScreen,Window}.zig in full + confirm the chunk-1a/1b module APIs they consume (cell::Cell/Style/Character, gwidth::gwidth, unicode grapheme iter, mouse, Winsize). Map each -> a rustty module (screen.rs/internal_screen.rs/window.rs); pin the cell-buffer ownership (Zig ArrayList/[]Cell -> Vec<Cell>/&mut [Cell]), the Window's screen back-reference (Zig *Screen -> a borrow or raw ptr — prefer a lifetime/&mut where the borrow structure allows, raw-ptr with the documented discipline only if child-window aliasing forces it), the print/gwidth grapheme-segmentation layout, and which methods are R5 seams (render/Tty/Vaxis-coupled). Port the 9 test{}. If a genuine blocker, disposition 'blocked' + STOP. Else 'ready'. Return the object.`,
  { label:'design:vaxis-c2', phase:'Design', agentType:'general-purpose', schema:D_SCHEMA })

if (!design || (design.disposition||'').toLowerCase().startsWith('blocked')) {
  log(`VAXIS-C2 DESIGN BLOCKED: ${design ? design.disposition : 'design failed'}`)
  return { blocked:true, design }
}

phase('Port')
const P_SCHEMA = { type:'object', additionalProperties:false, required:['disposition','files_changed','build_ok','tests_ported','miri_clean','notes'], properties:{
  disposition:{type:'string'}, files_changed:{type:'array',items:{type:'string'}}, build_ok:{type:'boolean'}, tests_ported:{type:'number'}, miri_clean:{type:'boolean'}, notes:{type:'string'} } }
const port = await agent(
  `${rules}\n\nPORT PHASE. Implement per ${DESIGN} — if blocked, STOP. Port Screen/InternalScreen/Window into crates/rustty-vaxis/src/{screen,internal_screen,window}.rs (+ mod wiring in lib.rs), preserving every comment (R1) and porting all 9 test{} assertion-for-assertion (esp. Window's print/gwidth layout tests). Methods needing later chunks = R5 seams. Update lib.rs PORT STATUS. Build \`cargo build -p rustty-vaxis\`, \`cargo test -p rustty-vaxis\`, \`cargo +nightly miri test -p rustty-vaxis\`. Report files_changed, build_ok, tests_ported, miri_clean, notes (incl. the Window->Screen borrow model + which methods are R5 seams). Do NOT git commit.`,
  { label:'port:vaxis-c2', phase:'Port', agentType:'general-purpose', schema:P_SCHEMA })

if (!port || (port.disposition||'').toLowerCase().startsWith('stopped')) {
  log(`VAXIS-C2 PORT STOPPED`)
  return { design, port, verify:null }
}

phase('Verify')
const V_SCHEMA = { type:'object', additionalProperties:false, required:['dimension','verdict','findings'], properties:{
  dimension:{type:'string'}, verdict:{enum:['PASS','FAIL']}, findings:{type:'array',items:{type:'string'}} } }
const verify = await parallel([
  () => agent(`${rules}\n\nVERIFY (GATES+MIRI). Independently re-run + confirm GREEN: cargo build --workspace; cargo test -p rustty-vaxis; cargo +nightly miri test -p rustty-vaxis; cargo clippy -p rustty-vaxis --all-targets -- -D warnings; cargo fmt --check; bash .port/tools/gates/check_acyclic.sh; python3 .port/tools/gates/check_test_counts.py; python3 .port/tools/gates/check_comment_coverage.py. Report each. verdict PASS only if ALL green.`,
    { label:'verify:gates', phase:'Verify', agentType:'Explore', schema:V_SCHEMA }),
  () => agent(`${rules}\n\nVERIFY (R1 FIDELITY). Diff rustty-vaxis chunk-2 vs ${VX}/src/{Screen,InternalScreen,Window}.zig: (1) every comment verbatim? (2) all 9 test{} ported assertion-for-assertion? (3) Window print/printSegment gwidth-aware grapheme layout faithful (grapheme segmentation + width advance + wrapping — exercises c1b, spot-check a wide-char/emoji print)? (4) Screen/InternalScreen cell buffer + readCell/writeCell bounds faithful? (5) the Window->Screen borrow/raw-ptr model sound (child-window aliasing)? (6) R5 seams (render/Tty-coupled) are unimplemented!()+verbatim, not fake? (7) no lossy String? R4 acyclic? Report findings; verdict PASS only if R1 intact.`,
    { label:'verify:fidelity', phase:'Verify', agentType:'Explore', schema:V_SCHEMA }),
])

return { design, port, verify }
