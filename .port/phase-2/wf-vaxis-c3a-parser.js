export const meta = {
  name: 'vaxis-c3a-parser',
  description: 'vaxis chunk 3a — port the ANSI input Parser (+ ctlseqs + event, ~1466 LOC, 34 test{}) into rustty-vaxis: design -> port -> verify(+Miri)',
  phases: [
    { title: 'Design', detail: 'faithful-port design for Parser/ctlseqs/event + STOP-guard' },
    { title: 'Port', detail: 'port the parser + 34 tests' },
    { title: 'Verify', detail: 'gates + R1 fidelity (34 parser tests exact) + Miri' },
  ],
}

const R = '/Users/gael/projects/rustty'
const VX = '/Users/gael/.cache/zig/p/vaxis-0.5.1-BWNV_LosCQAGmCCNOLljCIw6j6-yt53tji6n6rwJ2BhS'
const DESIGN = `${R}/.port/phase-2/VAXIS_C3A_DESIGN.md`

const rules = `
Full-fidelity Zig->Rust port (rustty). vaxis chunk 3a: the ANSI INPUT PARSER (the pure half of the backend). Source (read-only): vaxis 0.5.1 ${VX}/src/{Parser,ctlseqs,event}.zig. Target: crates/rustty-vaxis (chunks 1a/1b/2 landed, PORT STATUS done). Deps ONLY intra-crate: cell, key (Key + constants), mouse, uucode (the c1b grapheme/utf8 — Parser uses it for grapheme text), + lib. NO tty/Loop/queue (those are chunk 3b).
SCOPE: Parser.zig (1285 LOC, 34 test — parses a byte stream into Key/Mouse/paste/etc. events: the ground/escape/csi/osc/dcs/ss3 state machine, the Kitty keyboard protocol, SGR mouse, bracketed paste, the key/modifier decoding). ctlseqs.zig (151 — the control-sequence string constants incl. unicode_set/unicode_reset that list_colors/keybinds write). event.zig (30 — the Event enum/union). ~34 test{} — the fidelity contract (each pins an exact parsed event from an input byte sequence).
R1: EVERY line/comment/test{} ported assertion-for-assertion; the input bytes + parsed-event assertions are the contract; byte-slices -> &[u8]/Vec<u8>, never lossy String (event text/paste are bytes). Any method needing chunk-3b types (Tty/Loop) = R5 seam. R4 acyclic. R6: PURE -> Miri primary. Gates: build/test/clippy -D/fmt/check_acyclic.sh/check_test_counts.py/check_comment_coverage.py + Miri.`

const D_SCHEMA = { type:'object', additionalProperties:false, required:['doc_path','disposition','summary','key_decisions','risks'], properties:{
  doc_path:{type:'string'}, disposition:{type:'string',description:"'ready' or 'blocked: <reason>'"}, summary:{type:'string'}, key_decisions:{type:'array',items:{type:'string'}}, risks:{type:'array',items:{type:'string'}} } }

phase('Design')
const design = await agent(
  `${rules}\n\nDESIGN PHASE. Write ${DESIGN}. Read ${VX}/src/{Parser,ctlseqs,event}.zig in full. Map -> parser.rs/ctlseqs.rs/event.rs; pin the parser state machine (the Result{event,n} return, the ground/CSI/OSC/DCS/SS3 arms, the Kitty keyboard u21 codepoint + modifier decode, SGR mouse button/coord parsing, bracketed-paste buffering, the grapheme-text path via the c1b uucode), the byte-slice ownership (event payloads borrow the input buffer vs owned), and any chunk-3b-coupled method (R5 seam). The 34 test{} each assert an exact event from input bytes — port them assertion-for-assertion. If a genuine blocker, disposition 'blocked' + STOP. Else 'ready'. Return the object.`,
  { label:'design:vaxis-c3a', phase:'Design', agentType:'general-purpose', schema:D_SCHEMA })

if (!design || (design.disposition||'').toLowerCase().startsWith('blocked')) {
  log(`VAXIS-C3a DESIGN BLOCKED: ${design ? design.disposition : 'design failed'}`)
  return { blocked:true, design }
}

phase('Port')
const P_SCHEMA = { type:'object', additionalProperties:false, required:['disposition','files_changed','build_ok','tests_ported','miri_clean','notes'], properties:{
  disposition:{type:'string'}, files_changed:{type:'array',items:{type:'string'}}, build_ok:{type:'boolean'}, tests_ported:{type:'number'}, miri_clean:{type:'boolean'}, notes:{type:'string'} } }
const port = await agent(
  `${rules}\n\nPORT PHASE. Implement per ${DESIGN} — if blocked, STOP. Port Parser/ctlseqs/event into crates/rustty-vaxis/src/{parser,ctlseqs,event}.rs (+ mod wiring), preserving every comment (R1) and porting all 34 Parser test{} assertion-for-assertion (exact input bytes -> exact parsed events). Update lib.rs PORT STATUS. Build \`cargo build -p rustty-vaxis\`, \`cargo test -p rustty-vaxis\`, \`cargo +nightly miri test -p rustty-vaxis\`. Report files_changed, build_ok, tests_ported, miri_clean, notes (incl. the event byte-slice ownership + any R5 seam). Do NOT git commit.`,
  { label:'port:vaxis-c3a', phase:'Port', agentType:'general-purpose', schema:P_SCHEMA })

if (!port || (port.disposition||'').toLowerCase().startsWith('stopped')) {
  log(`VAXIS-C3a PORT STOPPED`)
  return { design, port, verify:null }
}

phase('Verify')
const V_SCHEMA = { type:'object', additionalProperties:false, required:['dimension','verdict','findings'], properties:{
  dimension:{type:'string'}, verdict:{enum:['PASS','FAIL']}, findings:{type:'array',items:{type:'string'}} } }
const verify = await parallel([
  () => agent(`${rules}\n\nVERIFY (GATES+MIRI). Independently re-run + confirm GREEN: cargo build --workspace; cargo test -p rustty-vaxis; cargo +nightly miri test -p rustty-vaxis; cargo clippy -p rustty-vaxis --all-targets -- -D warnings; cargo fmt --check; bash .port/tools/gates/check_acyclic.sh; python3 .port/tools/gates/check_test_counts.py; python3 .port/tools/gates/check_comment_coverage.py. Report each. verdict PASS only if ALL green (esp. the 34 parser tests).`,
    { label:'verify:gates', phase:'Verify', agentType:'Explore', schema:V_SCHEMA }),
  () => agent(`${rules}\n\nVERIFY (R1 FIDELITY). Diff rustty-vaxis chunk-3a vs ${VX}/src/{Parser,ctlseqs,event}.zig: (1) every comment verbatim? (2) all 34 Parser test{} ported assertion-for-assertion (exact input->event; spot-check a Kitty-keyboard, an SGR-mouse, a bracketed-paste, a grapheme-text case)? (3) the state machine faithful (CSI/OSC/DCS/SS3 arms, modifier/codepoint decode, the n-consumed byte counts)? (4) ctlseqs constants byte-exact (unicode_set/reset)? (5) event payloads no lossy String (bytes)? R4 acyclic? Report findings; verdict PASS only if R1 intact + the 34 tests are faithful.`,
    { label:'verify:fidelity', phase:'Verify', agentType:'Explore', schema:V_SCHEMA }),
])

return { design, port, verify }
