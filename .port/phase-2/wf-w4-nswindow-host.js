export const meta = {
  name: 'w4-nswindow-host',
  description: 'Sub-project #1 chunk W4 (apprt-c macOS host handshake + window blur/display_id + NSWindow live-frame finale): design -> adversary -> fold -> port -> verify',
  phases: [
    { title: 'Design', detail: 'write W4_DESIGN.md — the macOS C-ABI host leaves + the live-frame KAT' },
    { title: 'Adversary', detail: 'independently re-grep to break it' },
    { title: 'Fold', detail: 'fold fixes into W4_DESIGN.md' },
    { title: 'Port', detail: 'macOS host exports + blur/display_id + the NSWindow finale KAT' },
    { title: 'Verify', detail: 'gates + R1 fidelity panel' },
  ],
}

const R = '/Users/gael/projects/rustty'
const G = '/Users/gael/projects/ghostty'
const PLAN = `${R}/docs/superpowers/plans/2026-07-08-macos-windowing-sub1.md`
const DESIGN = `${R}/.port/phase-2/W4_DESIGN.md`

const rules = `
Full-fidelity Zig->Rust port (rustty). Reference (read-only): ghostty at ${G}. Target: ${R}.
Governing plan: ${PLAN} — chunk W4 (tasks W4-1..W4-4), plus the plan's Canonical Interfaces + Global Constraints (AUTHORITATIVE). W0/W1/W2/W3 DONE: the renderer presents on-screen (IOSurfaceLayer), CVDisplayLink works, the core render Thread event loop drives the real Renderer::draw_frame and Surface::draw is live. What remains for a LIVE windowed macOS terminal is the apprt-c host handshake.
R1 (every comment/test/assertion ported; Zig slices -> &[u8]/Vec<u8>/CString, NEVER lossy String; deferred=unimplemented!()+verbatim body). R4 (acyclic). R6 (FFI->ASan). Gates: build/test/clippy -D/fmt/check_acyclic.sh/check_test_counts.py/check_comment_coverage.py.
W4 scope (rustty-apprt-c + rustty-apprt embedded.rs macOS leaves — the two remaining macOS #[cfg(target_os="macos")] C-ABI leaves + the host handshake):
- W4-1: add the objc2 windowing deps (objc2-app-kit for NSWindow/NSView; objc2-quartz-core already; objc2-core-graphics for CGS) to rustty-apprt-c; expose the config BackgroundBlur radius accessor publicly.
- W4-2: fill rustty_set_window_background_blur (crates/rustty-apprt-c/src/lib.rs ~:2164-2192) — objc2 view->window->windowNumber + the PRIVATE CGS SPI CGSSetWindowBackgroundBlurRadius (extern "C" to the SkyLight/CoreGraphics private symbol, faithful to ghostty's apprt/embedded.zig setInitialWindowPosition/blur or os/ private CGS decl).
- W4-3: fill rustty_surface_set_display_id (lib.rs ~:2234-2248) — push a renderer-thread mailbox Message + wakeup the render loop (set_macos_display_id path), faithful to embedded.zig.
- W4-4: the culminating macOS windowing integration harness: open a real NSWindow hosting an NSView, hand its layer to the embedded Surface via the C-ABI (the host handshake), drive one live Surface::draw, and assert (headless-friendly: golden IOSurface readback of the hosted layer's contents; the actual visible window is #[ignore] on-device). Reuse the offscreen golden gate; skip on MetalError::NoMetalDevice.
Reference: ghostty src/apprt/embedded.zig (the macOS surface functions: newSurface/the CAMetalLayer/NSView host handshake, setContentScale, the CGS blur, displayID) + src/apprt/surface.zig. quicklook/secure-input/inspector leaves STAY deferred (do NOT fill).
NOTE (headless): NSWindow/NSView creation may need +[NSApplication sharedApplication] first and can fail without a window server; the live-window KAT is #[ignore] on-device, but the host-handshake + present path must be proven headless via an off-screen layer + golden readback.`

const DESIGN_SCHEMA = { type:'object', additionalProperties:false, required:['doc_path','disposition','summary','key_decisions','risks'], properties:{
  doc_path:{type:'string'}, disposition:{type:'string',description:"'ready' or 'blocked: <reason>'"},
  summary:{type:'string'}, key_decisions:{type:'array',items:{type:'string'}}, risks:{type:'array',items:{type:'string'}} } }

phase('Design')
const design = await agent(
  `${rules}\n\nDESIGN PHASE. Write ${DESIGN} — the EXACT faithful port design for W4-1..W4-4. Read FIRST: crates/rustty-apprt-c/src/lib.rs (the macOS blur ~:2164-2192 + display_id ~:2234-2248 leaves + the surface export patterns) + crates/rustty-apprt/src/embedded.rs (the macOS Surface leaves + the host handshake + core() accessor), the reference ghostty src/apprt/embedded.zig (setInitialWindowPosition/the CGS blur/displayID/newSurface layer handshake) + src/apprt/surface.zig, and how W3's Surface::draw + the renderer_seam::Thread mailbox/wakeup are reached. Decide: the objc2-app-kit/core-graphics deps + the private CGS SPI extern "C" decl (CGSSetWindowBackgroundBlurRadius / the SkyLight symbol), the display_id mailbox+wakeup path, and the headless-friendly NSWindow host KAT (off-screen golden readback + an #[ignore] on-device visible variant). Give precise edits, real line numbers, the runnable KAT. If a genuine blocker, disposition 'blocked: <reason>' + STOP. Else 'ready'. Return the structured object.`,
  { label:'design:W4', phase:'Design', agentType:'general-purpose', schema:DESIGN_SCHEMA })

if (!design || (design.disposition||'').toLowerCase().startsWith('blocked')) {
  log(`W4 DESIGN BLOCKED: ${design ? design.disposition : 'design agent failed'}`)
  return { blocked:true, design }
}

phase('Adversary')
const ADV_SCHEMA = { type:'object', additionalProperties:false, required:['verdict','fixes','blocking_issues'], properties:{
  verdict:{enum:['SOUND','SOUND_WITH_FIXES','UNSOUND']}, fixes:{type:'array',items:{type:'string'}}, blocking_issues:{type:'array',items:{type:'string'}} } }
const adversary = await agent(
  `${rules}\n\nADVERSARY PHASE. Design at ${DESIGN}. BREAK it: (1) R1 — blur/display_id faithful to embedded.zig (every comment, the exact CGS SPI symbol name + signature, the windowNumber path)? (2) the private CGS symbol — is CGSSetWindowBackgroundBlurRadius the correct linkage (SkyLight framework? CoreGraphics? the connection = CGSMainConnectionID())? faithful to the reference decl? (3) FFI — objc2-app-kit NSWindow/NSView API real? the layer host handshake sound (main-thread affinity)? (4) display_id — does the mailbox push + wakeup reach the W3 render Thread correctly (Message variant exists)? (5) does the finale KAT actually prove the host handshake + a live frame (golden readback), or is it trivial? is the #[ignore] on-device split correct + headless-safe? (6) R4 acyclic held? (7) any quicklook/secure-input/inspector leaf accidentally filled? List every fix + blocker. verdict SOUND only if zero.`,
  { label:'adversary:W4', phase:'Adversary', agentType:'Explore', schema:ADV_SCHEMA })

const needsFold = adversary && ((adversary.fixes && adversary.fixes.length) || (adversary.blocking_issues && adversary.blocking_issues.length))
if (needsFold) {
  phase('Fold')
  await agent(
    `${rules}\n\nFOLD PHASE. Adversary issues on ${DESIGN}. Fixes: ${JSON.stringify(adversary.fixes)}. Blocking: ${JSON.stringify(adversary.blocking_issues)}. Update ${DESIGN} IN PLACE to fix every one (re-read reference/current as needed). If a blocker is unresolvable add a top 'DISPOSITION: blocked: <reason>' line. Return a short summary.`,
    { label:'fold:W4', phase:'Fold', agentType:'general-purpose' })
}

phase('Port')
const PORT_SCHEMA = { type:'object', additionalProperties:false, required:['disposition','files_changed','build_ok','kats','gate_results','notes'], properties:{
  disposition:{type:'string',description:"'done' or 'stopped: <reason>'"},
  files_changed:{type:'array',items:{type:'string'}}, build_ok:{type:'boolean'},
  kats:{type:'array',items:{type:'object',additionalProperties:false,required:['name','passed'],properties:{name:{type:'string'},passed:{type:'boolean'}}}},
  gate_results:{type:'array',items:{type:'object',additionalProperties:false,required:['gate','ok'],properties:{gate:{type:'string'},ok:{type:'boolean'}}}},
  notes:{type:'string'} } }
const port = await agent(
  `${rules}\n\nPORT PHASE. Implement W4 STRICTLY per ${DESIGN} — if its first lines say 'DISPOSITION: blocked', STOP (no edits). In order: W4-1 (deps + config blur accessor), W4-2 (rustty_set_window_background_blur via objc2 + private CGS SPI), W4-3 (rustty_surface_set_display_id mailbox+wakeup), W4-4 (the NSWindow host-handshake finale KAT — headless golden readback + #[ignore] on-device visible variant). Do NOT fill quicklook/secure-input/inspector leaves. Preserve every comment (R1). Build \`cargo build -p rustty-apprt-c -p rustty-apprt\` incrementally; run the KAT; run gates (clippy -- -D warnings; fmt; check_acyclic.sh; check_comment_coverage.py). Report files_changed, build_ok, each KAT + gate result, notes (incl. any design deviation). Do NOT git commit.`,
  { label:'port:W4', phase:'Port', agentType:'general-purpose', schema:PORT_SCHEMA })

if (!port || (port.disposition||'').toLowerCase().startsWith('stopped')) {
  log(`W4 PORT STOPPED: ${port ? port.disposition : 'port agent failed'}`)
  return { design, adversary, port, verify: null }
}

phase('Verify')
const VER_SCHEMA = { type:'object', additionalProperties:false, required:['dimension','verdict','findings'], properties:{
  dimension:{type:'string'}, verdict:{enum:['PASS','FAIL']}, findings:{type:'array',items:{type:'string'}} } }
const verify = await parallel([
  () => agent(`${rules}\n\nVERIFY (GATES). Independently re-run + confirm GREEN: cargo build --workspace; cargo test -p rustty-apprt-c -p rustty-apprt; cargo clippy -p rustty-apprt-c -p rustty-apprt --all-targets -- -D warnings; cargo fmt --check; bash .port/tools/gates/check_acyclic.sh; python3 .port/tools/gates/check_test_counts.py; python3 .port/tools/gates/check_comment_coverage.py. Report each. verdict PASS only if ALL green.`,
    { label:'verify:gates', phase:'Verify', agentType:'Explore', schema:VER_SCHEMA }),
  () => agent(`${rules}\n\nVERIFY (R1 FIDELITY). Diff W4 vs reference: (1) blur (the CGS SPI symbol + signature + windowNumber path) + display_id faithful to embedded.zig? every comment preserved? (2) the private CGS extern decl correct (symbol/framework/connection)? (3) the host handshake main-thread-safe + faithful? (4) the finale KAT genuinely proves the handshake + a live golden frame (not trivial); the #[ignore] split headless-correct? (5) no quicklook/secure-input/inspector leaf filled (still deferred)? (6) R4 acyclic? (7) no lossy String? Report findings; verdict PASS only if R1 intact.`,
    { label:'verify:fidelity', phase:'Verify', agentType:'Explore', schema:VER_SCHEMA }),
])

return { design, adversary, port, verify }
