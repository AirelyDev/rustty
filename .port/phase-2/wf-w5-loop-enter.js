export const meta = {
  name: 'w5-loop-enter',
  description: 'Sub-project #1 chunk W5 (fill Metal::loop_enter container_of display-callback trampoline; un-ignore the live-frame finale): design -> adversary -> fold -> port -> [mirihar] -> verify',
  phases: [
    { title: 'Design', detail: 'write W5_DESIGN.md — the loop_enter trampoline + display-callback threading/aliasing model' },
    { title: 'Adversary', detail: 'stress the display-callback aliasing/threading soundness' },
    { title: 'Fold', detail: 'fold fixes' },
    { title: 'Port', detail: 'fill Metal::loop_enter + headless trampoline KAT + un-ignore the finale where possible' },
    { title: 'Mirihar', detail: 'if the design deems the aliasing non-trivial, prove it on the harness' },
    { title: 'Verify', detail: 'gates + R1 fidelity + aliasing panel' },
  ],
}

const R = '/Users/gael/projects/rustty'
const G = '/Users/gael/projects/ghostty'
const DESIGN = `${R}/.port/phase-2/W5_DESIGN.md`
const MIRIHAR = `${R}/.port/phase-2/mirihar-chunk3b`

const rules = `
Full-fidelity Zig->Rust port (rustty). Reference (read-only): ghostty at ${G}. Target: ${R}.
W0-W4 DONE: on-screen present (IOSurfaceLayer), CVDisplayLink, the core render Thread loop, Surface::draw, blur/display_id C-ABI. The ONLY remaining seam for a live windowed terminal is renderer::metal::metal_impl::Metal::loop_enter (metal_impl.rs:476, unimplemented!("windowing-host")), which aborts any live surface's render thread.
R1 (every comment/test/assertion ported; deferred=unimplemented!()+verbatim body; NEVER lossy String). R4 (rustty-renderer MUST NOT dep rustty-apprt-api). R6 (FFI->ASan; non-trivial raw-ptr aliasing PROVEN on ${MIRIHAR} under SB+TB). Gates: build/test/clippy -D/fmt/check_acyclic.sh/check_test_counts.py/check_comment_coverage.py.
THE SEAM (reference Metal.zig:164-178, verbatim in metal_impl.rs:461-475):
  pub fn loopEnter(self: *Metal) void {
    const renderer: *align(1) Renderer = @fieldParentPtr("api", self);   // container_of: Metal is Renderer.api
    self.layer.setDisplayCallback(@ptrCast(&displayCallback), @ptrCast(renderer));
  }
  fn displayCallback(renderer: *Renderer) align(8) void {
    renderer.drawFrame(true) catch |err| log.warn("Error drawing frame in display callback, err={}", .{err});
  }
Port shape: loop_enter recovers *mut Renderer from &Metal via core::mem::offset_of!(Renderer, api) (byte_sub), then self.layer's IOSurfaceLayer::set_display_callback(Some(display_callback), renderer as *mut c_void) (W0 ported set_display_callback + DisplayCallback = Option<unsafe extern "C-unwind" fn(*mut c_void)>). display_callback casts ctx->*mut Renderer and calls draw_frame(true).
THE CRUX (design MUST resolve): display_callback calls (*renderer).draw_frame(true) — draw_frame takes &mut self, so this is a whole-Renderer &mut reborrow through a raw ptr. Is the IOSurfaceLayer display method EVER invoked concurrently with the render thread's own draw_frame? Investigate the reference: when/what thread fires the CALayer display (vs the CVDisplayLink draw_now->render-thread path)? If serialized/same-thread => sound as-is (faithful). If potentially concurrent => the &mut reborrow is UNSOUND and needs the drawFrame-internal draw_mutex to gate it (does drawFrame already take a mutex making the raw-*mut call sound-by-construction?), or a raw-self draw path. Resolve this precisely with reference evidence; if non-trivial, add a mirihar candidate.`

const DESIGN_SCHEMA = { type:'object', additionalProperties:false, required:['doc_path','disposition','summary','aliasing_model','needs_mirihar','key_decisions','risks'], properties:{
  doc_path:{type:'string'}, disposition:{type:'string',description:"'ready' or 'blocked: <reason>'"},
  summary:{type:'string'}, aliasing_model:{type:'string',description:'the resolved display-callback threading/aliasing model + why the raw-*mut draw_frame call is sound'},
  needs_mirihar:{type:'boolean'}, key_decisions:{type:'array',items:{type:'string'}}, risks:{type:'array',items:{type:'string'}} } }

phase('Design')
const design = await agent(
  `${rules}\n\nDESIGN PHASE. Write ${DESIGN} — the exact faithful port of Metal::loop_enter + display_callback. Read FIRST: metal_impl.rs:455-495 (the seam + the Renderer/Metal field layout — confirm the field name is 'api' and Renderer's definition), crates/rustty-renderer/src/generic.rs (Renderer struct + draw_frame signature + draw_mutex if any + how the render thread calls loop_enter), crates/rustty-renderer/src/metal/iosurface_layer.rs (set_display_callback + DisplayCallback), and the reference Metal.zig loopEnter/displayCallback + generic.zig drawFrame (does it take a draw_mutex?) + how the layer display is triggered. RESOLVE THE CRUX (fill aliasing_model): prove the raw-*mut Renderer draw_frame call is sound (serialized with the render thread, or gated by drawFrame's own mutex). Give the exact loop_enter/display_callback code, the offset_of! container_of, a HEADLESS KAT proving the trampoline (build a windowed Renderer, loop_enter, fire the layer's display method, assert draw_frame ran — no live threads/window server needed), and whether to un-ignore any W4-4 finale test. Set needs_mirihar if the aliasing is non-trivial. If a genuine blocker, disposition 'blocked' + STOP. Else 'ready'. Return the object.`,
  { label:'design:W5', phase:'Design', agentType:'general-purpose', schema:DESIGN_SCHEMA })

if (!design || (design.disposition||'').toLowerCase().startsWith('blocked')) {
  log(`W5 DESIGN BLOCKED: ${design ? design.disposition : 'design agent failed'}`)
  return { blocked:true, design }
}

phase('Adversary')
const ADV_SCHEMA = { type:'object', additionalProperties:false, required:['verdict','fixes','blocking_issues'], properties:{
  verdict:{enum:['SOUND','SOUND_WITH_FIXES','UNSOUND']}, fixes:{type:'array',items:{type:'string'}}, blocking_issues:{type:'array',items:{type:'string'}} } }
const adversary = await agent(
  `${rules}\n\nADVERSARY PHASE. Design at ${DESIGN}; its claimed aliasing_model: "${design.aliasing_model}". BREAK it: (1) is the aliasing argument ACTUALLY sound? Is the CALayer display method truly never concurrent with the render thread's draw_frame, OR does drawFrame's mutex genuinely make the raw-*mut &mut-reborrow sound (a Rust &mut reborrow is UB under aliasing EVEN IF an internal mutex serializes the effects — so the argument must be that the two draw_frame calls never overlap in TIME, not merely that a mutex serializes them)? Find the counterexample thread interleaving or confirm none exists with reference evidence. (2) is offset_of!(Renderer, api) the correct container_of (field name/type)? (3) R1 — every comment ported, the log.warn preserved? (4) does the headless KAT actually fire the trampoline and observe draw_frame (not trivial)? (5) DisplayCallback ABI (extern "C-unwind") match W0? (6) if the design says needs_mirihar=false but the aliasing is non-trivial, DEMAND a mirihar proof. List every fix + blocker. verdict SOUND only if the aliasing is provably sound AND zero other issues.`,
  { label:'adversary:W5', phase:'Adversary', agentType:'Explore', schema:ADV_SCHEMA })

const needsFold = adversary && ((adversary.fixes && adversary.fixes.length) || (adversary.blocking_issues && adversary.blocking_issues.length))
if (needsFold) {
  phase('Fold')
  await agent(
    `${rules}\n\nFOLD PHASE. Adversary issues on ${DESIGN}. Fixes: ${JSON.stringify(adversary.fixes)}. Blocking: ${JSON.stringify(adversary.blocking_issues)}. Update ${DESIGN} IN PLACE to resolve every one (re-read reference/current as needed); if the aliasing cannot be shown sound, either switch the design to a sound raw-self draw path or add a top 'DISPOSITION: blocked' line. Set needs_mirihar=true in the doc if the adversary demanded it. Return a short summary.`,
    { label:'fold:W5', phase:'Fold', agentType:'general-purpose' })
}

phase('Port')
const PORT_SCHEMA = { type:'object', additionalProperties:false, required:['disposition','files_changed','build_ok','kats','gate_results','notes'], properties:{
  disposition:{type:'string',description:"'done' or 'stopped: <reason>'"},
  files_changed:{type:'array',items:{type:'string'}}, build_ok:{type:'boolean'},
  kats:{type:'array',items:{type:'object',additionalProperties:false,required:['name','passed'],properties:{name:{type:'string'},passed:{type:'boolean'}}}},
  gate_results:{type:'array',items:{type:'object',additionalProperties:false,required:['gate','ok'],properties:{gate:{type:'string'},ok:{type:'boolean'}}}},
  notes:{type:'string'} } }
const port = await agent(
  `${rules}\n\nPORT PHASE. Implement W5 STRICTLY per ${DESIGN} — if its first lines say 'DISPOSITION: blocked', STOP. Fill crates/rustty-renderer/src/metal/metal_impl.rs Metal::loop_enter + the display_callback trampoline (offset_of! container_of; set_display_callback; the sound draw_frame path from the design's aliasing_model), preserving every reference comment (R1). Add the headless trampoline KAT. If the design un-ignores a W4-4 finale test, do so. Build \`cargo build -p rustty-renderer\` + run the KAT (\`cargo test -p rustty-renderer loop_enter/display_callback\`); run gates (clippy -p rustty-renderer -- -D warnings; fmt; check_acyclic.sh; check_comment_coverage.py). Report files_changed, build_ok, each KAT + gate, notes. Do NOT git commit.`,
  { label:'port:W5', phase:'Port', agentType:'general-purpose', schema:PORT_SCHEMA })

if (!port || (port.disposition||'').toLowerCase().startsWith('stopped')) {
  log(`W5 PORT STOPPED: ${port ? port.disposition : 'port agent failed'}`)
  return { design, adversary, port, mirihar:null, verify: null }
}

let mirihar = null
if (design.needs_mirihar) {
  phase('Mirihar')
  const MIRI_SCHEMA = { type:'object', additionalProperties:false, required:['naive_is_ub','shipped_is_clean','sb_ran','tb_ran','evidence'], properties:{
    naive_is_ub:{type:'boolean'}, shipped_is_clean:{type:'boolean'}, sb_ran:{type:'boolean'}, tb_ran:{type:'boolean'}, evidence:{type:'string'} } }
  mirihar = await agent(
    `${rules}\n\nMIRIHAR PHASE. Per ${DESIGN}, add a displaycb_naive (UB) + displaycb_shipped (CLEAN) candidate pair to ${MIRIHAR}/src/main.rs modelling the loop_enter display-callback aliasing (a Renderer-analogue with an api sub-field; a callback recovering the parent via offset + calling draw through a raw ptr; model whether it overlaps the render-thread draw). Run BOTH under SB (default) AND -Zmiri-tree-borrows; confirm the shipped shape is CLEAN under both. Update ${MIRIHAR}/README.md + .port/phase-2/MIRI_EXCEPTIONS.md. Report naive_is_ub/shipped_is_clean/sb_ran/tb_ran + evidence (actual miri output).`,
    { label:'mirihar:W5', phase:'Mirihar', agentType:'general-purpose', schema:MIRI_SCHEMA })
}

phase('Verify')
const VER_SCHEMA = { type:'object', additionalProperties:false, required:['dimension','verdict','findings'], properties:{
  dimension:{type:'string'}, verdict:{enum:['PASS','FAIL']}, findings:{type:'array',items:{type:'string'}} } }
const verify = await parallel([
  () => agent(`${rules}\n\nVERIFY (GATES). Independently re-run + confirm GREEN: cargo build --workspace; cargo test -p rustty-renderer; cargo clippy -p rustty-renderer --all-targets -- -D warnings; cargo fmt --check; bash .port/tools/gates/check_acyclic.sh; python3 .port/tools/gates/check_test_counts.py; python3 .port/tools/gates/check_comment_coverage.py. Report each. verdict PASS only if ALL green.`,
    { label:'verify:gates', phase:'Verify', agentType:'Explore', schema:VER_SCHEMA }),
  () => agent(`${rules}\n\nVERIFY (R1 + ALIASING). Diff W5 vs reference Metal.zig loopEnter/displayCallback: (1) faithful (container_of via offset_of!('api'), set_display_callback, draw_frame(true), the log.warn preserved)? (2) is the raw-*mut Renderer draw_frame call PROVABLY sound (the two draw_frame invocations never overlap in time, per the design's aliasing_model + reference evidence; if a mirihar proof was required, confirm it passed under SB+TB)? (3) headless KAT genuinely fires the trampoline + observes draw_frame? (4) Metal::loop_enter no longer unimplemented!(); does a live surface's render thread now proceed past loop_enter? (5) no lossy String; R4 held? Report findings; verdict PASS only if R1 + aliasing sound.`,
    { label:'verify:fidelity', phase:'Verify', agentType:'Explore', schema:VER_SCHEMA }),
])

return { design, adversary, port, mirihar, verify }
