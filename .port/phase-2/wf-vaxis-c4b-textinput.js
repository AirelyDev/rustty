export const meta = {
  name: 'vaxis-c4b-textinput',
  description: 'vaxis chunk 4b — port the TextInput widget (widgets/TextInput.zig, 460 LOC, 3 test{}) into rustty-vaxis: design -> port -> verify(+Miri)',
  phases: [
    { title: 'Design', detail: 'faithful-port design for TextInput + STOP-guard' },
    { title: 'Port', detail: 'port TextInput + 3 tests' },
    { title: 'Verify', detail: 'gates + R1 fidelity + Miri' },
  ],
}

const R = '/Users/gael/projects/rustty'
const VX = '/Users/gael/.cache/zig/p/vaxis-0.5.1-BWNV_LosCQAGmCCNOLljCIw6j6-yt53tji6n6rwJ2BhS'
const DESIGN = `${R}/.port/phase-2/VAXIS_C4B_DESIGN.md`

const rules = `
Full-fidelity Zig->Rust port (rustty). vaxis chunk 4b: the TextInput widget (list_themes' search box). Source (read-only): vaxis 0.5.1 ${VX}/src/widgets/TextInput.zig (460 LOC, 3 test). Target: crates/rustty-vaxis/src/widgets/text_input.rs (create the widgets/ module dir + mod). Deps ONLY intra-crate: cell (Cell/Style), key (Key + constants), event (Event), window (Window), gwidth, unicode (grapheme iter), grapheme_cache — all landed.
SCOPE: TextInput.zig — the text-input widget: the internal grapheme-buffer (ArrayList of grapheme byte-slices / a gap-buffer), cursor (grapheme index), update(Event) [insert/delete/left/right/home/end/word-motion via Key], draw(Window) [gwidth-aware rendering + cursor], toOwnedSlice/clearAndFree/sliceToCursor. 3 test{} — the fidelity contract (they assert buffer/cursor state after key sequences).
R1: EVERY line/comment/test{} ported assertion-for-assertion; byte-slices -> &[u8]/Vec<u8>, never lossy String (grapheme bytes). Any Vaxis/Loop-coupled method = R5 seam (unlikely — TextInput is self-contained over Window). R4 acyclic. R6: PURE -> Miri primary. Gates: build/test/clippy -D/fmt/check_acyclic.sh/check_test_counts.py/check_comment_coverage.py + Miri.`

const D_SCHEMA = { type:'object', additionalProperties:false, required:['disposition'], properties:{
  doc_path:{type:'string'}, disposition:{type:'string',description:"'ready' or 'blocked: <reason>'"}, summary:{type:'string'}, key_decisions:{type:'array',items:{type:'string'}}, risks:{type:'array',items:{type:'string'}} } }

phase('Design')
const design = await agent(
  `${rules}\n\nDESIGN PHASE. Write ${DESIGN}. Read ${VX}/src/widgets/TextInput.zig in full + confirm the Window/Key/gwidth/unicode APIs it consumes. Map -> src/widgets/text_input.rs (+ src/widgets.rs or widgets/mod.rs + lib.rs mod wiring). Pin the grapheme-buffer representation (Zig ArrayList([]const u8) grapheme slices -> Vec<Vec<u8>> or a gap buffer — faithful to the Zig ownership), the cursor/update(Event) key handling (insert/delete/motion), the gwidth-aware draw, and the 3 test{}. If a genuine blocker, disposition 'blocked' + STOP. Else 'ready'. Return the object.`,
  { label:'design:vaxis-c4b', phase:'Design', agentType:'general-purpose', schema:D_SCHEMA })

if (!design || (design.disposition||'').toLowerCase().startsWith('blocked')) {
  log(`VAXIS-C4b DESIGN BLOCKED: ${design ? design.disposition : 'design failed'}`)
  return { blocked:true, design }
}

phase('Port')
const P_SCHEMA = { type:'object', additionalProperties:false, required:['disposition','files_changed','build_ok','tests_ported','miri_clean','notes'], properties:{
  disposition:{type:'string'}, files_changed:{type:'array',items:{type:'string'}}, build_ok:{type:'boolean'}, tests_ported:{type:'number'}, miri_clean:{type:'boolean'}, notes:{type:'string'} } }
const port = await agent(
  `${rules}\n\nPORT PHASE. Implement per ${DESIGN} — if blocked, STOP. Port TextInput into crates/rustty-vaxis/src/widgets/text_input.rs (+ the widgets module wiring in lib.rs), preserving every comment (R1) and porting all 3 test{} assertion-for-assertion. Update lib.rs PORT STATUS. Build \`cargo build -p rustty-vaxis\`, \`cargo test -p rustty-vaxis\`, \`cargo +nightly miri test -p rustty-vaxis\`. Report files_changed, build_ok, tests_ported, miri_clean, notes (incl. the grapheme-buffer ownership + any R5 seam). Do NOT git commit.`,
  { label:'port:vaxis-c4b', phase:'Port', agentType:'general-purpose', schema:P_SCHEMA })

if (!port || (port.disposition||'').toLowerCase().startsWith('stopped')) {
  log(`VAXIS-C4b PORT STOPPED`)
  return { design, port, verify:null }
}

phase('Verify')
const V_SCHEMA = { type:'object', additionalProperties:false, required:['dimension','verdict','findings'], properties:{
  dimension:{type:'string'}, verdict:{enum:['PASS','FAIL']}, findings:{type:'array',items:{type:'string'}} } }
const verify = await parallel([
  () => agent(`${rules}\n\nVERIFY (GATES+MIRI). Independently re-run + confirm GREEN: cargo build --workspace; cargo test -p rustty-vaxis; cargo +nightly miri test -p rustty-vaxis; cargo clippy -p rustty-vaxis --all-targets -- -D warnings; cargo fmt --check; bash .port/tools/gates/check_acyclic.sh; python3 .port/tools/gates/check_test_counts.py; python3 .port/tools/gates/check_comment_coverage.py. Report each. verdict PASS only if ALL green.`,
    { label:'verify:gates', phase:'Verify', agentType:'Explore', schema:V_SCHEMA }),
  () => agent(`${rules}\n\nVERIFY (R1 FIDELITY). Diff rustty-vaxis text_input.rs vs ${VX}/src/widgets/TextInput.zig: (1) every comment verbatim? (2) all 3 test{} ported assertion-for-assertion (buffer/cursor state after key sequences)? (3) the grapheme-buffer + cursor motion + insert/delete faithful (grapheme-aware, not byte-aware)? (4) the gwidth-aware draw faithful? (5) no lossy String (grapheme bytes)? R4 acyclic? Report findings; verdict PASS only if R1 intact.`,
    { label:'verify:fidelity', phase:'Verify', agentType:'Explore', schema:V_SCHEMA }),
])

return { design, port, verify }
