export const meta = {
  name: 'wire2-listthemes',
  description: 'TUI-CLI wiring 2 (finale) — fill the list_themes interactive preview picker (Preview + event loop + zf.rank + TextInput) in rustty-cli: design -> adversary -> fold -> port -> verify',
  phases: [
    { title: 'Design', detail: 'faithful-port design for the Preview picker + event loop + zf ranking + test strategy' },
    { title: 'Adversary', detail: 'attack the event-loop + updateFiltered + draw fidelity' },
    { title: 'Fold', detail: 'fold fixes' },
    { title: 'Port', detail: 'add zf dep + fill the preview seam' },
    { title: 'Verify', detail: 'gates + R1 fidelity + Miri(pure parts)' },
  ],
}

const R = '/Users/gael/projects/rustty'
const G = '/Users/gael/projects/ghostty'
const DESIGN = `${R}/.port/phase-2/WIRE2_DESIGN.md`

const rules = `
Full-fidelity Zig->Rust port (rustty). TUI-CLI wiring 2 (the finale): un-defer the list_themes interactive fuzzy-finder preview picker in rustty-cli, now that rustty-vaxis (the full core: Vaxis/Window/Tty/Loop/Style/Color/Key/Mouse + widgets::TextInput) AND rustty-zf (zf::rank) are landed. Reference (read-only): ${G}/src/cli/list_themes.zig (the Event union + Preview struct + its init/deinit/run/update/updateFiltered/draw methods + the color helper + lorem_ipsum + the preview entry fn). Target: crates/rustty-cli/src/list_themes.rs — the preview picker is CURRENTLY R5 unimplemented!() seams with the verbatim Zig bodies preserved as comments (DEFERRALS cli C5). Fill them from those preserved bodies using rustty-vaxis + rustty-zf. The --plain theme-list path already works and STAYS.
ADD DEP: rustty-zf to crates/rustty-cli/Cargo.toml (rustty-vaxis already added in WIRE1; both leaves, acyclic). The picker uses: vaxis::Loop (the read-thread event loop), Vaxis::init/window/render/queryTerminal/enterAltScreen/setTitle/setMouseShape/setMouseMode/copyToSystemClipboard/deinit, Tty::init/getWinsize, Window::print (styled theme rows + preview swatches + lorem_ipsum sample), widgets::TextInput (search box), zf::rank (fuzzy filter on the theme names), Style/Color/Key/Mouse. Event handling: up/down/page/home/end/enter/escape/typing/mouse.
R1: EVERY line/comment of the Preview/preview bodies ported faithfully; byte-slices -> &[u8], never lossy String (theme names, paths, lorem_ipsum). R4 acyclic. TEST STRATEGY: the interactive Loop path needs a real tty -> #[ignore]-on-device; but factor updateFiltered (zf ranking — PURE, Vec of themes + a filter string -> ranked/filtered indices) and draw (render-to-Vec<u8>) so they get pure unit tests (zf.rank ordering + a draw smoke). Gates: build/test/clippy -D/fmt/check_acyclic.sh/check_test_counts.py/check_comment_coverage.py + Miri.`

const D_SCHEMA = { type:'object', additionalProperties:false, required:['disposition'], properties:{
  doc_path:{type:'string'}, disposition:{type:'string',description:"'ready' or 'blocked: <reason>'"}, summary:{type:'string'}, key_decisions:{type:'array',items:{type:'string'}}, risks:{type:'array',items:{type:'string'}} } }

phase('Design')
const design = await agent(
  `${rules}\n\nDESIGN PHASE. Write ${DESIGN}. Read ${G}/src/cli/list_themes.zig (the Event union + Preview struct + all methods + color + lorem_ipsum + preview) + the current crates/rustty-cli/src/list_themes.rs seams (the preserved verbatim bodies) + the rustty-vaxis Loop/Vaxis/Window/TextInput API + rustty-zf::rank signature. Map the fill: the Preview struct (heap-pinned — it stores tty/vx/text_input + the vaxis Loop holds a raw self, the construct-then-spawn discipline again), init/deinit, run (the Loop event loop + key/mouse dispatch), update (state transitions per key: up/down/page/home/end/enter/escape/typing), updateFiltered (zf::rank over the theme names -> filtered+ranked indices — PURE, unit-testable), draw (the theme rows + the preview pane color swatches + lorem_ipsum sample + the TextInput search box), the ThemeListElement.toUri. Pin the Loop-holds-Preview aliasing (raw-self, like C3b — needs_mirihar if non-trivial, but the Loop aliasing was already proven in C3b so reuse that discipline). Test strategy: interactive #[ignore]-on-device; updateFiltered + draw pure unit tests. If a genuine blocker, disposition 'blocked' + STOP. Else 'ready'. Return the object.`,
  { label:'design:wire2', phase:'Design', agentType:'general-purpose', schema:D_SCHEMA })

if (!design || (design.disposition||'').toLowerCase().startsWith('blocked')) {
  log(`WIRE2 DESIGN BLOCKED: ${design ? design.disposition : 'design failed'}`)
  return { blocked:true, design }
}

phase('Adversary')
const ADV_SCHEMA = { type:'object', additionalProperties:false, required:['verdict','fixes','blocking_issues'], properties:{
  verdict:{enum:['SOUND','SOUND_WITH_FIXES','UNSOUND']}, fixes:{type:'array',items:{type:'string'}}, blocking_issues:{type:'array',items:{type:'string'}} } }
const adversary = await agent(
  `${rules}\n\nADVERSARY PHASE. Design at ${DESIGN}. BREAK it by re-reading list_themes.zig: (1) the update() event dispatch — EVERY key arm (up/down/kp_*/page_up/page_down/home/end/enter/kp_enter/escape/f + typing into TextInput + the selection clamp/scroll math) faithful? a dropped arm silently breaks navigation. (2) updateFiltered — the zf::rank call (tokens from the search string, the rank threshold/null handling, the filtered-index rebuild, the selected-index reclamp) exact? (3) draw — the theme-row styling (selected vs not), the preview pane (color swatches from the theme palette + the lorem_ipsum styled sample), the TextInput placement faithful? (4) the Preview heap-pinning + the Loop-holds-raw-self aliasing sound (reuse the C3b-proven discipline; no &mut *self while the read thread is live)? (5) toUri + copyToSystemClipboard on enter faithful? (6) the interactive path correctly #[ignore]-on-device + updateFiltered/draw unit-tested? (7) no lossy String? List every fix + blocker. verdict SOUND only if the event loop + filtering are provably faithful.`,
  { label:'adversary:wire2', phase:'Adversary', agentType:'Explore', schema:ADV_SCHEMA })

const needsFold = adversary && ((adversary.fixes && adversary.fixes.length) || (adversary.blocking_issues && adversary.blocking_issues.length))
if (needsFold) {
  phase('Fold')
  await agent(
    `${rules}\n\nFOLD PHASE. Adversary issues on ${DESIGN}. Fixes: ${JSON.stringify(adversary.fixes)}. Blocking: ${JSON.stringify(adversary.blocking_issues)}. Update ${DESIGN} IN PLACE to fix every one (re-read list_themes.zig as needed). If a blocker is genuine add a top 'DISPOSITION: blocked' line. Return a short summary.`,
    { label:'fold:wire2', phase:'Fold', agentType:'general-purpose' })
}

phase('Port')
const P_SCHEMA = { type:'object', additionalProperties:false, required:['disposition','files_changed','build_ok','notes'], properties:{
  disposition:{type:'string'}, files_changed:{type:'array',items:{type:'string'}}, build_ok:{type:'boolean'}, notes:{type:'string'} } }
const port = await agent(
  `${rules}\n\nPORT PHASE. Implement per ${DESIGN} — if 'DISPOSITION: blocked', STOP. Add rustty-zf to crates/rustty-cli/Cargo.toml. Fill the list_themes preview picker (Preview struct + all methods + Event + color + lorem_ipsum + preview + toUri) from the preserved verbatim Zig bodies using rustty-vaxis + rustty-zf, preserving every comment (R1). The Preview is heap-pinned; the Loop-holds-raw-self follows the C3b-proven discipline. Remove the unimplemented!() + DEFERRALS markers; update DEFERRALS.md (cli C5 RESOLVED). Add the updateFiltered (zf rank ordering) + draw (Vec<u8> smoke) unit tests; interactive path #[ignore]-on-device. Build \`cargo build -p rustty-cli\`, \`cargo test -p rustty-cli\`, \`cargo +nightly miri test -p rustty-cli\` (pure parts). Report files_changed, build_ok, notes (incl. the aliasing approach + what's #[ignore]-on-device + the unit coverage). Do NOT git commit.`,
  { label:'port:wire2', phase:'Port', agentType:'general-purpose', schema:P_SCHEMA })

if (!port || (port.disposition||'').toLowerCase().startsWith('stopped')) {
  log(`WIRE2 PORT STOPPED`)
  return { design, adversary, port, verify:null }
}

phase('Verify')
const V_SCHEMA = { type:'object', additionalProperties:false, required:['dimension','verdict','findings'], properties:{
  dimension:{type:'string'}, verdict:{enum:['PASS','FAIL']}, findings:{type:'array',items:{type:'string'}} } }
const verify = await parallel([
  () => agent(`${rules}\n\nVERIFY (GATES). Independently re-run + confirm GREEN: cargo build --workspace; cargo test -p rustty-cli; cargo +nightly miri test -p rustty-cli; cargo clippy -p rustty-cli --all-targets -- -D warnings; cargo fmt --check; bash .port/tools/gates/check_acyclic.sh; python3 .port/tools/gates/check_test_counts.py; python3 .port/tools/gates/check_comment_coverage.py. Report each. verdict PASS only if ALL green.`,
    { label:'verify:gates', phase:'Verify', agentType:'Explore', schema:V_SCHEMA }),
  () => agent(`${rules}\n\nVERIFY (R1 FIDELITY). Diff the filled list_themes preview vs ${G}/src/cli/list_themes.zig: (1) every comment verbatim, no unimplemented!() left? (2) update() event dispatch faithful (every key arm + selection/scroll math)? (3) updateFiltered zf::rank filtering exact (unit-tested ordering)? (4) draw (theme rows + preview swatches + lorem_ipsum + TextInput) faithful? (5) the Loop-holds-raw-self aliasing sound (C3b discipline)? (6) toUri + clipboard-on-enter faithful? (7) interactive #[ignore]-on-device, --plain path unchanged? (8) no lossy String? R4 acyclic (rustty-zf added)? Report findings; verdict PASS only if the event loop + filtering are faithful.`,
    { label:'verify:fidelity', phase:'Verify', agentType:'Explore', schema:V_SCHEMA }),
])

return { design, adversary, port, verify }
