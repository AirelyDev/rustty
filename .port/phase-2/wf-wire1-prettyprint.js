export const meta = {
  name: 'wire1-prettyprint',
  description: 'TUI-CLI wiring 1 — fill the list_colors + list_keybinds prettyPrint seams in rustty-cli using rustty-vaxis: design -> port -> verify',
  phases: [
    { title: 'Design', detail: 'faithful-port design for the two prettyPrint paths + test strategy + STOP-guard' },
    { title: 'Port', detail: 'add vaxis dep + fill the two prettyPrint seams' },
    { title: 'Verify', detail: 'gates + R1 fidelity + (render smoke where feasible)' },
  ],
}

const R = '/Users/gael/projects/rustty'
const G = '/Users/gael/projects/ghostty'
const DESIGN = `${R}/.port/phase-2/WIRE1_DESIGN.md`

const rules = `
Full-fidelity Zig->Rust port (rustty). TUI-CLI wiring 1: un-defer the list_colors + list_keybinds interactive prettyPrint paths in rustty-cli, now that rustty-vaxis (the full vaxis core: Vaxis/Window/Tty/ctlseqs/Style/Color/prettyPrint) is landed. Reference (read-only): ${G}/src/cli/{list_colors,list_keybinds}.zig (the prettyPrint fns + list_keybinds' ChordBinding/TriggerNode/iterateBindings helpers). Target: crates/rustty-cli/src/{list_colors,list_keybinds}.rs — the prettyPrint bodies are CURRENTLY R5 unimplemented!() seams with the verbatim Zig body preserved as comments (DEFERRALS cli C5). Fill them from those preserved bodies using rustty-vaxis.
ADD DEP: rustty-vaxis (+ nothing else) to crates/rustty-cli/Cargo.toml (a leaf, acyclic — rustty-cli is a top consumer). The prettyPrint paths use: vaxis::Tty::init/getWinsize, vaxis::init (Vaxis), Window (print styled Segments), vaxis::ctlseqs::unicode_set/reset, Vaxis::pretty_print(writer), Style/Color. The --plain paths already work and STAY.
R1: EVERY line/comment/test{} of the prettyPrint bodies ported faithfully; byte-slices -> &[u8], never lossy String. R4 acyclic. TEST STRATEGY: prettyPrint opens a real Tty (winsize) -> the interactive path is #[ignore]-on-device (no /dev/tty in sandbox), BUT factor the render so a TestTty/Vec<u8>-writer smoke test can assert emitted bytes where feasible (mirror the vaxis C4a render smoke test). Gates: build/test/clippy -D/fmt/check_acyclic.sh/check_test_counts.py/check_comment_coverage.py + Miri.`

const D_SCHEMA = { type:'object', additionalProperties:false, required:['disposition'], properties:{
  doc_path:{type:'string'}, disposition:{type:'string',description:"'ready' or 'blocked: <reason>'"}, summary:{type:'string'}, key_decisions:{type:'array',items:{type:'string'}}, risks:{type:'array',items:{type:'string'}} } }

phase('Design')
const design = await agent(
  `${rules}\n\nDESIGN PHASE. Write ${DESIGN}. Read ${G}/src/cli/{list_colors,list_keybinds}.zig prettyPrint (+ list_keybinds ChordBinding/TriggerNode/iterateBindings) + the current crates/rustty-cli/src/{list_colors,list_keybinds}.rs seams (the preserved verbatim bodies) + the rustty-vaxis public API (Vaxis::init/window/pretty_print, Tty::init/getWinsize, Window::print, ctlseqs, Style/Color). Map the fill: how each prettyPrint builds its Window (color swatches / keybind table), the ChordBinding/TriggerNode iterateBindings port, the Tty+winsize acquisition, the pretty_print(writer) call. Decide the test strategy (interactive #[ignore]-on-device; a render-to-Vec<u8> smoke where the styled-Window build can be exercised without a real tty). If a genuine blocker (a needed vaxis API missing), disposition 'blocked' + STOP. Else 'ready'. Return the object.`,
  { label:'design:wire1', phase:'Design', agentType:'general-purpose', schema:D_SCHEMA })

if (!design || (design.disposition||'').toLowerCase().startsWith('blocked')) {
  log(`WIRE1 DESIGN BLOCKED: ${design ? design.disposition : 'design failed'}`)
  return { blocked:true, design }
}

phase('Port')
const P_SCHEMA = { type:'object', additionalProperties:false, required:['disposition','files_changed','build_ok','notes'], properties:{
  disposition:{type:'string'}, files_changed:{type:'array',items:{type:'string'}}, build_ok:{type:'boolean'}, notes:{type:'string'} } }
const port = await agent(
  `${rules}\n\nPORT PHASE. Implement per ${DESIGN} — if blocked, STOP. Add rustty-vaxis to crates/rustty-cli/Cargo.toml. Fill the list_colors::pretty_print + list_keybinds::pretty_print (+ ChordBinding/TriggerNode/iterateBindings) seams from their preserved verbatim Zig bodies using rustty-vaxis, preserving every comment (R1). Remove the unimplemented!() + DEFERRALS marker; update DEFERRALS.md (cli C5 partial). Add the smoke test(s) per the design; interactive paths #[ignore]-on-device. Build \`cargo build -p rustty-cli\`, \`cargo test -p rustty-cli\`, \`cargo +nightly miri test -p rustty-cli\` (if feasible; the tty FFI parts gate out). Report files_changed, build_ok, notes (incl. what's #[ignore]-on-device + the smoke coverage). Do NOT git commit.`,
  { label:'port:wire1', phase:'Port', agentType:'general-purpose', schema:P_SCHEMA })

if (!port || (port.disposition||'').toLowerCase().startsWith('stopped')) {
  log(`WIRE1 PORT STOPPED`)
  return { design, port, verify:null }
}

phase('Verify')
const V_SCHEMA = { type:'object', additionalProperties:false, required:['dimension','verdict','findings'], properties:{
  dimension:{type:'string'}, verdict:{enum:['PASS','FAIL']}, findings:{type:'array',items:{type:'string'}} } }
const verify = await parallel([
  () => agent(`${rules}\n\nVERIFY (GATES). Independently re-run + confirm GREEN: cargo build --workspace; cargo test -p rustty-cli; cargo clippy -p rustty-cli --all-targets -- -D warnings; cargo fmt --check; bash .port/tools/gates/check_acyclic.sh; python3 .port/tools/gates/check_test_counts.py; python3 .port/tools/gates/check_comment_coverage.py. Report each. verdict PASS only if ALL green.`,
    { label:'verify:gates', phase:'Verify', agentType:'Explore', schema:V_SCHEMA }),
  () => agent(`${rules}\n\nVERIFY (R1 FIDELITY). Diff the filled prettyPrint paths vs ${G}/src/cli/{list_colors,list_keybinds}.zig: (1) every comment verbatim (the preserved bodies fully ported, no unimplemented!() left)? (2) the color-swatch / keybind-table Window build faithful (Style/Color, the print Segments, layout)? (3) list_keybinds ChordBinding/TriggerNode/iterateBindings faithful? (4) the Tty+winsize + pretty_print(writer) usage correct? (5) interactive paths #[ignore]-on-device (justified), --plain paths unchanged? (6) no lossy String? R4 acyclic (only rustty-vaxis added)? Report findings; verdict PASS only if R1 intact.`,
    { label:'verify:fidelity', phase:'Verify', agentType:'Explore', schema:V_SCHEMA }),
])

return { design, port, verify }
