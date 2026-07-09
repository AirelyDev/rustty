export const meta = {
  name: 'w2-cvdisplaylink',
  description: 'Sub-project #1 chunk W2 (CVDisplayLink block in the renderer, apprt-free; un-defers loop_enter + deinit display-link arm): design -> adversary -> fold -> port -> verify',
  phases: [
    { title: 'Design', detail: 'write W2_DESIGN.md — CVDisplayLink FFI + the generic.rs display-link block' },
    { title: 'Adversary', detail: 'independently re-grep to break it' },
    { title: 'Fold', detail: 'fold fixes into W2_DESIGN.md' },
    { title: 'Port', detail: 'CVDisplayLink primitive + block/callbacks + vsync KAT (sequential)' },
    { title: 'Verify', detail: 'gates + R1 fidelity panel' },
  ],
}

const R = '/Users/gael/projects/rustty'
const G = '/Users/gael/projects/ghostty'
const PLAN = `${R}/docs/superpowers/plans/2026-07-08-macos-windowing-sub1.md`
const DESIGN = `${R}/.port/phase-2/W2_DESIGN.md`

const rules = `
Full-fidelity Zig->Rust port (rustty). Reference (read-only): ghostty at ${G}. Target: ${R}.
Governing plan: ${PLAN} — chunk W2 (tasks W2-T1/T2/T3), plus the plan's Canonical Interfaces + Global Constraints (AUTHORITATIVE). W0 + W1 are DONE/committed: Metal on-screen present works (metal_impl.rs Metal::init/present->IOSurfaceLayer::set_surface; loop_enter is currently unimplemented!() with the verbatim Zig loopEnter+displayCallback body preserved; Renderer::init/deinit filled but deinit's DisplayLink teardown arm is guarded/deferred; generic.rs has display_link=None).
R1 (every comment/test/assertion ported; Zig slices -> &[u8]/Vec<u8>/CString, NEVER lossy String; deferred=unimplemented!()+verbatim body). R4: rustty-renderer MUST NOT dep rustty-apprt-api. R6 (pure->Miri, FFI->ASan). Gates: build/test/clippy -D/fmt/check_acyclic.sh/check_test_counts.py/check_comment_coverage.py.
W2 scope (ALL in apprt-free rustty-renderer): W2-T1 port the CoreVideo CVDisplayLink primitive (reference pkg/macos/video/DisplayLink.zig or src/renderer usage — CVDisplayLinkCreateWithActiveCGDisplays/SetOutputCallback/SetCurrentCGDisplay(FromMetalDevice)/Start/Stop/Release) -> crates/rustty-renderer/src/metal/cvdisplaylink.rs via extern "C" CoreVideo bindings (link framework=CoreVideo) or objc2-core-video if it covers the API; W2-T2 the generic.rs Renderer CVDisplayLink block: display_link field (Some), has_vsync, set_macos_display_id, displayLinkCallback, loop_enter/loop_exit, thread_enter/thread_exit, set_visible/set_focus + UN-DEFER the Renderer::deinit DisplayLink teardown arm (display_link.stop()+release()); W2-T3 the vsync KAT.
KEY apprt-free seam: the displayLinkCallback must NOT name the render Thread (which lives in core, R4). loop_enter takes the draw_now handle as an xev::Async (Copy, NOT an apprt type) passed by the caller; the callback notifies that Async. If loop_enter genuinely cannot be made apprt-free in W2, keep it unimplemented!() with a precise seam note and defer to W3 — but PREFER filling it via the xev::Async handle param. Verify CVDisplayLink API availability (objc2-core-video / extern-C + CoreVideo framework link) before locking.`

const DESIGN_SCHEMA = { type:'object', additionalProperties:false, required:['doc_path','disposition','summary','key_decisions','risks'], properties:{
  doc_path:{type:'string'}, disposition:{type:'string',description:"'ready' or 'blocked: <reason>'"},
  summary:{type:'string'}, key_decisions:{type:'array',items:{type:'string'}}, risks:{type:'array',items:{type:'string'}} } }

phase('Design')
const design = await agent(
  `${rules}\n\nDESIGN PHASE. Write ${DESIGN} — the EXACT faithful port design for W2-T1/T2/T3. Read FIRST: the reference CVDisplayLink wrapper (ghostty pkg/macos/video/DisplayLink.zig + its use in src/renderer/generic.zig displayLinkCallback/loopEnter and src/renderer/Metal.zig loopEnter), the current crates/rustty-renderer/src/generic.rs (display_link field, the deferred deinit DisplayLink arm, loop_enter callers) + metal_impl.rs (the unimplemented!() loop_enter with the verbatim body). Decide the CVDisplayLink FFI approach (objc2-core-video vs extern-C + CoreVideo framework link — VERIFY availability) and the apprt-free callback seam (loop_enter(draw_now: xev::Async) notifies via the Copy handle; NO apprt/Thread type named). Give precise edits, real line numbers, the runnable vsync KAT (a CVDisplayLink bound to the main/active display whose callback notifies a real xev::Async that a bounded loop drains). If a genuine blocker, disposition 'blocked: <reason>' + STOP. Else 'ready'. Return the structured object.`,
  { label:'design:W2', phase:'Design', agentType:'general-purpose', schema:DESIGN_SCHEMA })

if (!design || (design.disposition||'').toLowerCase().startsWith('blocked')) {
  log(`W2 DESIGN BLOCKED: ${design ? design.disposition : 'design agent failed'}`)
  return { blocked:true, design }
}

phase('Adversary')
const ADV_SCHEMA = { type:'object', additionalProperties:false, required:['verdict','fixes','blocking_issues'], properties:{
  verdict:{enum:['SOUND','SOUND_WITH_FIXES','UNSOUND']}, fixes:{type:'array',items:{type:'string'}}, blocking_issues:{type:'array',items:{type:'string'}} } }
const adversary = await agent(
  `${rules}\n\nADVERSARY PHASE. Design at ${DESIGN}. BREAK it: (1) R1 — CVDisplayLink primitive faithful to DisplayLink.zig (every comment/fn/error)? the generic.zig displayLinkCallback + loopEnter body preserved verbatim where deferred, ported faithfully where filled? (2) FFI — does the chosen CoreVideo binding (objc2-core-video/extern-C) actually expose CVDisplayLinkCreateWithActiveCGDisplays/SetOutputCallback/SetCurrentCGDisplayFromMetalDevice/Start/Stop/Release with the right signatures? is CoreVideo framework linked? (3) R4 — does the callback/loop_enter name any apprt type or the core Thread? is the xev::Async handle seam genuinely apprt-free? (4) the output-callback ABI + the raw context pointer aliasing — SB/TB sound (no &mut alias across the callback)? is the callback's real-time-thread context safe? (5) does the deinit display_link teardown arm now un-defer correctly (stop()+release(), no double-free)? (6) does the vsync KAT actually observe a notify (or is it racy/trivial)? (7) build: any missing feature/link? List every fix + blocker. verdict SOUND only if zero.`,
  { label:'adversary:W2', phase:'Adversary', agentType:'Explore', schema:ADV_SCHEMA })

const needsFold = adversary && ((adversary.fixes && adversary.fixes.length) || (adversary.blocking_issues && adversary.blocking_issues.length))
if (needsFold) {
  phase('Fold')
  await agent(
    `${rules}\n\nFOLD PHASE. Adversary issues on ${DESIGN}. Fixes: ${JSON.stringify(adversary.fixes)}. Blocking: ${JSON.stringify(adversary.blocking_issues)}. Update ${DESIGN} IN PLACE to fix every one (re-read reference/current as needed). If a blocker is unresolvable add a top 'DISPOSITION: blocked: <reason>' line. Return a short summary.`,
    { label:'fold:W2', phase:'Fold', agentType:'general-purpose' })
}

phase('Port')
const PORT_SCHEMA = { type:'object', additionalProperties:false, required:['disposition','files_changed','build_ok','kats','gate_results','notes'], properties:{
  disposition:{type:'string',description:"'done' or 'stopped: <reason>'"},
  files_changed:{type:'array',items:{type:'string'}}, build_ok:{type:'boolean'},
  kats:{type:'array',items:{type:'object',additionalProperties:false,required:['name','passed'],properties:{name:{type:'string'},passed:{type:'boolean'}}}},
  gate_results:{type:'array',items:{type:'object',additionalProperties:false,required:['gate','ok'],properties:{gate:{type:'string'},ok:{type:'boolean'}}}},
  notes:{type:'string'} } }
const port = await agent(
  `${rules}\n\nPORT PHASE. Implement W2 STRICTLY per ${DESIGN} — if its first lines say 'DISPOSITION: blocked', STOP (disposition 'stopped', no edits). Else, in order: W2-T1 (crates/rustty-renderer/src/metal/cvdisplaylink.rs + Cargo.toml CoreVideo link/dep + metal.rs mod), then W2-T2 (generic.rs display-link block + loop_enter/loop_exit/thread_enter/thread_exit/has_vsync/set_macos_display_id/set_visible/set_focus + un-defer the deinit DisplayLink arm; and un-defer metal_impl.rs loop_enter if the design fills it), then W2-T3 (the vsync KAT). Preserve every comment (R1). Build \`cargo build -p rustty-renderer\` incrementally; run the KAT (\`cargo test -p rustty-renderer\` cvdisplay/vsync); run gates (clippy -p rustty-renderer -- -D warnings; fmt; check_acyclic.sh; check_comment_coverage.py). Report files_changed, build_ok, each KAT + gate result, notes (incl. any design deviation + whether loop_enter was filled or stayed deferred). Do NOT git commit.`,
  { label:'port:W2', phase:'Port', agentType:'general-purpose', schema:PORT_SCHEMA })

if (!port || (port.disposition||'').toLowerCase().startsWith('stopped')) {
  log(`W2 PORT STOPPED: ${port ? port.disposition : 'port agent failed'}`)
  return { design, adversary, port, verify: null }
}

phase('Verify')
const VER_SCHEMA = { type:'object', additionalProperties:false, required:['dimension','verdict','findings'], properties:{
  dimension:{type:'string'}, verdict:{enum:['PASS','FAIL']}, findings:{type:'array',items:{type:'string'}} } }
const verify = await parallel([
  () => agent(`${rules}\n\nVERIFY (GATES). Independently re-run + confirm GREEN: cargo build --workspace; cargo test -p rustty-renderer; cargo clippy -p rustty-renderer --all-targets -- -D warnings; cargo fmt --check; bash .port/tools/gates/check_acyclic.sh; python3 .port/tools/gates/check_test_counts.py; python3 .port/tools/gates/check_comment_coverage.py. Report each. verdict PASS only if ALL green.`,
    { label:'verify:gates', phase:'Verify', agentType:'Explore', schema:VER_SCHEMA }),
  () => agent(`${rules}\n\nVERIFY (R1 FIDELITY). Diff W2 vs reference: (1) cvdisplaylink.rs faithful to the CoreVideo DisplayLink wrapper (all comments/fns/errors)? (2) generic.rs display-link block (displayLinkCallback/loop_enter/loop_exit/has_vsync/set_macos_display_id) faithful to generic.zig? (3) R4: no apprt/Thread type in rustty-renderer; the callback seam uses only xev::Async? (4) the deinit display_link teardown un-deferred soundly (no double-free)? (5) the output-callback raw-context aliasing SB-sound? (6) no lossy String? (7) is the loop_enter now filled or a documented R5 seam? Report findings; verdict PASS only if R1 intact.`,
    { label:'verify:fidelity', phase:'Verify', agentType:'Explore', schema:VER_SCHEMA }),
])

return { design, adversary, port, verify }
