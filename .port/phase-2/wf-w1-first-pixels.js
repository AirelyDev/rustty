export const meta = {
  name: 'w1-first-pixels',
  description: 'Sub-project #1 chunk W1 (Metal on-screen present + Renderer windowed lifecycle = first pixels): design -> adversary -> fold -> port(3 sequential) -> verify',
  phases: [
    { title: 'Design', detail: 'write W1_DESIGN.md — the exact port design for W1-1..W1-7' },
    { title: 'Adversary', detail: 'independently re-grep references to break the design' },
    { title: 'Fold', detail: 'fold fixes into W1_DESIGN.md' },
    { title: 'Port', detail: 'A: Metal on-screen delta; B: Renderer windowed lifecycle; C: first-pixels KAT (sequential)' },
    { title: 'Verify', detail: 'gates + R1 fidelity panel' },
  ],
}

const R = '/Users/gael/projects/rustty'
const G = '/Users/gael/projects/ghostty'
const PLAN = `${R}/docs/superpowers/plans/2026-07-08-macos-windowing-sub1.md`
const DESIGN = `${R}/.port/phase-2/W1_DESIGN.md`

const rules = `
Full-fidelity Zig->Rust port (rustty). Reference (read-only): ghostty at ${G}. Target: ${R}.
Governing plan: ${PLAN} — chunk W1, tasks W1-1..W1-7, plus the plan's "Canonical Interfaces" + "Global Constraints" (AUTHORITATIVE). W0 is DONE/committed (IOSurfaceLayer at crates/rustty-renderer/src/metal/iosurface_layer.rs with init/set_surface->Result/set_surface_sync/set_display_callback/as_ca_layer; objc2-quartz-core CALayer/CAMetalLayer in-lock; cf_release_thread on real xev).
R1 (100% fidelity: every comment/test/assertion ported; Zig byte-slices -> &[u8]/Vec<u8>/CString, NEVER lossy String; deferred = unimplemented!() + verbatim body). R4 (rustty-renderer MUST NOT dep rustty-apprt-api). R6 (pure->Miri, FFI->ASan). Gates: cargo build --workspace / cargo test / clippy -D warnings / fmt / check_acyclic.sh / check_test_counts.py / check_comment_coverage.py.
W1 scope (all in rustty-renderer, apprt-free): W1-1 add layer:Option<IOSurfaceLayer> field to Metal (metal_impl.rs) + set None in init_offscreen; W1-2 Metal::init on-screen constructor (M-4 view attach: wantsLayer/addSublayer/clipsToBounds — but headless-test-friendly: build a CAMetalLayer/IOSurfaceLayer off-screen, no window server required); W1-3 surface_size (from layer.bounds), present/presentLastTarget (via IOSurfaceLayer::set_surface), loop_enter; W1-4 Frame.complete async addCompletedHandler present half (metal/frame.rs:198); W1-5 generic.rs present-failure error log seam (generic.rs metal_buffer_completed); W1-7 Renderer windowed lifecycle — FILL Renderer::init (generic.rs:1793) + deinit (generic.rs:1840, verbatim body preserved) + surface_init<S>/finalize_surface_init<S>, generalizing the SHARED body with the existing init_offscreen (do NOT duplicate); W1-6 CULMINATING first-pixels KAT.
CRITICAL: "first pixels" is proven HEADLESS-FRIENDLY — render one real frame into an IOSurface-backed Target (the existing golden path) then present it via IOSurfaceLayer::set_surface onto an off-screen CAMetalLayer, and assert the IOSurface readback golden-matches (reuse the offscreen golden gate). The actual visible NSWindow is W4, NOT here. Skip gracefully if no Metal device (MetalError::NoMetalDevice), like the W0 KAT.`

const DESIGN_SCHEMA = { type:'object', additionalProperties:false, required:['doc_path','disposition','summary','key_decisions','risks'], properties:{
  doc_path:{type:'string'}, disposition:{type:'string',description:"'ready' or 'blocked: <reason>'"},
  summary:{type:'string'}, key_decisions:{type:'array',items:{type:'string'}}, risks:{type:'array',items:{type:'string'}} } }

phase('Design')
const design = await agent(
  `${rules}\n\nDESIGN PHASE. Write ${DESIGN} — the EXACT faithful port design for W1-1..W1-7. Read FIRST: crates/rustty-renderer/src/metal/metal_impl.rs (init_offscreen, the :334/:384 seams, Target), crates/rustty-renderer/src/generic.rs (init_offscreen body + the :1793 init / :1840 deinit seams + Options + metal_buffer_completed present error seam), crates/rustty-renderer/src/metal/frame.rs (:198), the W0 iosurface_layer.rs, and the reference ghostty src/renderer/Metal.zig (init/present/presentLastTarget/surfaceSize/loopEnter) + src/renderer/generic.zig (init/deinit/surfaceInit/finalizeSurfaceInit) + src/renderer/metal/Frame.zig. For each task give the precise edits, the objc2 mapping (objc2-app-kit NSView attach — headless-safe; objc2-metal present), how the windowed init reuses init_offscreen's shared body (factor a common helper), the surface handshake generic-over-S (apprt-free), and the runnable KATs incl. the headless first-pixels golden-readback KAT. Cite real line numbers. If a genuine blocker exists set disposition 'blocked: <reason>' and STOP. Else 'ready'. Return the structured object.`,
  { label:'design:W1', phase:'Design', agentType:'general-purpose', schema:DESIGN_SCHEMA })

if (!design || (design.disposition||'').toLowerCase().startsWith('blocked')) {
  log(`W1 DESIGN BLOCKED: ${design ? design.disposition : 'design agent failed'}`)
  return { blocked:true, design }
}

phase('Adversary')
const ADV_SCHEMA = { type:'object', additionalProperties:false, required:['verdict','fixes','blocking_issues'], properties:{
  verdict:{enum:['SOUND','SOUND_WITH_FIXES','UNSOUND']}, fixes:{type:'array',items:{type:'string'}}, blocking_issues:{type:'array',items:{type:'string'}} } }
const adversary = await agent(
  `${rules}\n\nADVERSARY PHASE. Design at ${DESIGN}. BREAK it by independent re-grep: (1) R1 — does the windowed Renderer::init preserve EVERY comment + the exact reference init body (re-read generic.zig init)? is the deinit verbatim body preserved (generic.rs:1840)? any dropped assertion? (2) does factoring the shared init/init_offscreen helper drop or alter any reference logic? (3) Metal::init/present/surfaceSize/loopEnter faithful to Metal.zig? does present actually route drawFrame->Frame.complete->bufferCompleted->present->IOSurfaceLayer.set_surface (generic.zig:~1590/Frame.zig:126)? (4) R4 — any apprt type named in rustty-renderer? surface_init<S> truly generic? (5) is the first-pixels KAT genuinely headless (no window-server dependency) and does its golden readback actually assert real pixels? (6) do the KATs compile against the Canonical Interfaces + W0 IOSurfaceLayer signatures? (7) Options extension — only platform-neutral fields (no cycle)? List every fix + true blocker. verdict SOUND only if zero.`,
  { label:'adversary:W1', phase:'Adversary', agentType:'Explore', schema:ADV_SCHEMA })

const needsFold = adversary && ((adversary.fixes && adversary.fixes.length) || (adversary.blocking_issues && adversary.blocking_issues.length))
if (needsFold) {
  phase('Fold')
  await agent(
    `${rules}\n\nFOLD PHASE. Adversary issues on ${DESIGN}. Fixes: ${JSON.stringify(adversary.fixes)}. Blocking: ${JSON.stringify(adversary.blocking_issues)}. Update ${DESIGN} IN PLACE to incorporate every fix + resolve every blocker (re-read reference/current files as needed). If a blocker is genuinely unresolvable, add a top 'DISPOSITION: blocked: <reason>' line so the port STOPS. Return a short summary of changes.`,
    { label:'fold:W1', phase:'Fold', agentType:'general-purpose' })
}

// ---- Port: 3 SEQUENTIAL sub-agents (share metal_impl.rs/generic.rs; must not run concurrently) ----
phase('Port')
const PORT_SCHEMA = { type:'object', additionalProperties:false, required:['disposition','files_changed','build_ok','notes'], properties:{
  disposition:{type:'string',description:"'done' or 'stopped: <reason>'"},
  files_changed:{type:'array',items:{type:'string'}}, build_ok:{type:'boolean'}, notes:{type:'string'} } }

const portA = await agent(
  `${rules}\n\nPORT PHASE A (Metal on-screen present machinery: W1-1,W1-2,W1-3,W1-4,W1-5). Implement STRICTLY per ${DESIGN} — if its first lines say 'DISPOSITION: blocked', STOP (disposition 'stopped: <reason>', no edits). Edit crates/rustty-renderer/src/metal/metal_impl.rs (layer field + init on-screen + surface_size + present/presentLastTarget + loop_enter), crates/rustty-renderer/src/metal/frame.rs (:198 async present), crates/rustty-renderer/src/generic.rs (metal_buffer_completed present-error-log seam ONLY — do NOT touch init/deinit, that is Port B). Preserve every comment (R1). Build \`cargo build -p rustty-renderer\` and fix incrementally. Report files_changed, build_ok, notes. Do NOT git commit.`,
  { label:'port:W1-A', phase:'Port', agentType:'general-purpose', schema:PORT_SCHEMA })

let portB = null, portC = null
if (portA && !(portA.disposition||'').toLowerCase().startsWith('stopped')) {
  portB = await agent(
    `${rules}\n\nPORT PHASE B (Renderer windowed lifecycle: W1-7). Port A is done (Metal present machinery landed). Implement per ${DESIGN}: FILL crates/rustty-renderer/src/generic.rs Renderer::init (:1793) + deinit (:1840, preserve the verbatim reference body) + add surface_init<S>/finalize_surface_init<S> + extend Options with the platform-neutral windowed fields ONLY (no apprt types — R4). Factor the SHARED body with init_offscreen into a common helper (no duplication, full R1 fidelity). Build \`cargo build -p rustty-renderer\` + run \`cargo test -p rustty-renderer windowed_lifecycle\`. Report files_changed, build_ok, notes. Do NOT git commit.`,
    { label:'port:W1-B', phase:'Port', agentType:'general-purpose', schema:PORT_SCHEMA })
  if (portB && !(portB.disposition||'').toLowerCase().startsWith('stopped')) {
    portC = await agent(
      `${rules}\n\nPORT PHASE C (first-pixels culminating KAT: W1-6). Ports A+B are done. Implement per ${DESIGN} the HEADLESS first-pixels KAT: render one real frame into an IOSurface-backed Target via the real windowed present path (Renderer draw -> Frame.complete -> present -> IOSurfaceLayer::set_surface on an off-screen CAMetalLayer), then assert the IOSurface readback golden-matches (reuse the offscreen golden gate). Skip gracefully on MetalError::NoMetalDevice. Build + run the KAT (\`cargo test -p rustty-renderer\` first_pixels or the KAT name). Then run the gates: clippy -p rustty-renderer -- -D warnings; cargo fmt; check_acyclic.sh; check_comment_coverage.py. Report files_changed, build_ok, notes (incl. each gate + KAT result). Do NOT git commit.`,
      { label:'port:W1-C', phase:'Port', agentType:'general-purpose', schema:PORT_SCHEMA })
  }
}

const portStopped = (p) => !p || (p.disposition||'').toLowerCase().startsWith('stopped')
if (portStopped(portA) || portStopped(portB) || portStopped(portC)) {
  log(`W1 PORT INCOMPLETE: A=${portA&&portA.disposition} B=${portB&&portB.disposition} C=${portC&&portC.disposition}`)
  return { design, adversary, portA, portB, portC, verify: null }
}

phase('Verify')
const VER_SCHEMA = { type:'object', additionalProperties:false, required:['dimension','verdict','findings'], properties:{
  dimension:{type:'string'}, verdict:{enum:['PASS','FAIL']}, findings:{type:'array',items:{type:'string'}} } }
const verify = await parallel([
  () => agent(`${rules}\n\nVERIFY (GATES). Independently re-run + confirm GREEN: cargo build --workspace; cargo test -p rustty-renderer; cargo clippy -p rustty-renderer --all-targets -- -D warnings; cargo fmt --check; bash .port/tools/gates/check_acyclic.sh; python3 .port/tools/gates/check_test_counts.py; python3 .port/tools/gates/check_comment_coverage.py. Report each as a finding. verdict PASS only if ALL green.`,
    { label:'verify:gates', phase:'Verify', agentType:'Explore', schema:VER_SCHEMA }),
  () => agent(`${rules}\n\nVERIFY (R1 FIDELITY). Diff the ported W1 code vs reference: (1) Renderer::init/deinit (generic.rs) faithful to generic.zig — every comment, the verbatim deinit body, no logic dropped in the init_offscreen shared-helper factoring? (2) Metal::init/present/surfaceSize/loopEnter faithful to Metal.zig? present routes to IOSurfaceLayer::set_surface? (3) R4: no apprt type in rustty-renderer; surface_init<S> generic; Options only platform-neutral? (4) the first-pixels KAT genuinely asserts real golden pixels (not a trivial assert)? (5) no lossy String? Report findings; verdict PASS only if R1 intact.`,
    { label:'verify:fidelity', phase:'Verify', agentType:'Explore', schema:VER_SCHEMA }),
])

return { design, adversary, portA, portB, portC, verify }
