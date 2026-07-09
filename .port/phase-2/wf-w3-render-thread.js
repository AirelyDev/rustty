export const meta = {
  name: 'w3-render-thread',
  description: 'Sub-project #1 chunk W3 (swap renderer_seam::Renderer->real + fill core-resident Thread<A> render event loop + wire Surface::draw): design -> adversary -> fold -> port(E1,E2) -> mirihar -> verify',
  phases: [
    { title: 'Design', detail: 'write W3_DESIGN.md — E1 real-Renderer swap + E2 Thread event loop + mirihar candidates' },
    { title: 'Adversary', detail: 'independently re-grep to break it' },
    { title: 'Fold', detail: 'fold fixes into W3_DESIGN.md' },
    { title: 'Port', detail: 'A: real Renderer swap + Surface::draw; B: fill Thread<A> loop (sequential)' },
    { title: 'Mirihar', detail: 'add renderthread candidates; prove naive=UB / shipped=CLEAN under SB+TB' },
    { title: 'Verify', detail: 'gates + R1 fidelity + aliasing-soundness panel' },
  ],
}

const R = '/Users/gael/projects/rustty'
const G = '/Users/gael/projects/ghostty'
const PLAN = `${R}/docs/superpowers/plans/2026-07-08-macos-windowing-sub1.md`
const DESIGN = `${R}/.port/phase-2/W3_DESIGN.md`
const MIRIHAR = `${R}/.port/phase-2/mirihar-chunk3b`

const rules = `
Full-fidelity Zig->Rust port (rustty). Reference (read-only): ghostty at ${G}. Target: ${R}.
Governing plan: ${PLAN} — chunk W3 (E1, E2a, E2b), plus the plan's Canonical Interfaces + Global Constraints (AUTHORITATIVE). W0/W1/W2 DONE: rustty_renderer::Renderer has the real windowed init/deinit/draw_frame/surface_init/present + CVDisplayLink loop_enter(draw_now: xev::Async).
R1 (every comment/test/assertion ported; Zig slices -> &[u8]/Vec<u8>/CString, NEVER lossy String; deferred=unimplemented!()+verbatim body). R4: rustty-core already deps rustty-renderer (renderer_seam is core-local) so E adds NO new edge; VERIFY check_acyclic.sh stays green. R6: pure->Miri; the render-thread raw-ptr aliasing is PROVEN on the isolated non-FFI ${MIRIHAR} harness (SB default + -Zmiri-tree-borrows). Gates: build/test/clippy -D/fmt/check_acyclic.sh/check_test_counts.py/check_comment_coverage.py.
W3 scope (in rustty-core):
- E1: replace the crates/rustty-core/src/renderer_seam.rs \`Renderer\` PLACEHOLDER (unit struct; surfaceInit/init/finalizeSurfaceInit/threadEnter/drawFrame/deinit are unimplemented!()) with the REAL rustty_renderer::Renderer (monomorphic). Because core's Options<A> holds 3 apprt-typed fields the real Options can't, use the newtype/forwarder approach: core constructs rustty_renderer::Options (config/font_grid/size) from Options<A>, calls the real methods, keeps the apprt-typed bits (surface_mailbox/rt_surface/thread) in core. Alias RendererError. Wire crates/rustty-core/src/surface.rs Surface::draw -> real drawFrame.
- E2a/E2b: FILL the unimplemented!() bodies of crate::renderer_seam::Thread<A: apprt::App>::{init, threadMain(self_: *mut Thread<A>), deinit(&mut self)} + private callbacks (armLoop/wakeupCallback/drawCallback/renderCallback/cursorTimerCallback/drainMailbox/syncDrawTimer/startDrawTimer/stopDrawTimer) over rustty-xev, calling (*self.renderer).draw_frame(sync) on the real Renderer. The Thread struct + ALL fields ALREADY EXIST at renderer_seam.rs:229-337 (DO NOT redefine/move it). CANONICAL init already wired at surface.rs:1857 (init(alloc, config, rt_surface, renderer, state, app_mailbox)); the construct-then-spawn raw-ptr threadMain is ALREADY wired via RendererThreadArg(*mut Thread<A>) at surface.rs:1481-1489,1965.
ALIASING DISCIPLINE (the crux): after spawn the child thread holds *mut Thread<A> (threadMain) for its life; the main thread must access ONLY the Copy handles (wakeup/draw_now Async, captured pre-spawn at surface.rs:1885) + *const mailbox — NEVER &mut renderer_thread. threadMain/callbacks use raw-self (*mut Thread<A>) with narrow field access, NO &mut *self whole-struct reborrow (the [ADV-FIX F1/F2] + search/io_thread precedent; see rust-mut-args-strongly-protected). This MUST be proven on the mirihar harness (renderthread_naive=UB / renderthread_shipped=CLEAN), modelling the aliasing shape (xev calls stubbed as no-ops — the harness is non-FFI).
Reference: ghostty src/renderer/Thread.zig in full (threadMain + every callback + startDrawTimer/stopDrawTimer) mapped onto the core-resident struct; src/Surface.zig draw.`

const DESIGN_SCHEMA = { type:'object', additionalProperties:false, required:['doc_path','disposition','summary','key_decisions','risks'], properties:{
  doc_path:{type:'string'}, disposition:{type:'string',description:"'ready' or 'blocked: <reason>'"},
  summary:{type:'string'}, key_decisions:{type:'array',items:{type:'string'}}, risks:{type:'array',items:{type:'string'}} } }

phase('Design')
const design = await agent(
  `${rules}\n\nDESIGN PHASE. Write ${DESIGN} — the EXACT faithful port design for W3 E1 + E2a + E2b. Read FIRST: crates/rustty-core/src/renderer_seam.rs (the Renderer placeholder + the Thread<A> struct 229-337 + Options<A>), crates/rustty-core/src/surface.rs (the Surface::draw path + the spawn wiring 1481-1975 + Surface::deinit stop/join), crates/rustty-renderer/src/generic.rs (the real Renderer public surface: init/deinit/draw_frame/surface_init/present/loop_enter), the reference ghostty src/renderer/Thread.zig (threadMain + all callbacks) + src/Surface.zig draw. Design: (E1) the Options<A>->rustty_renderer::Options mapping + the Renderer swap + Surface::draw wiring; (E2) the faithful Thread event-loop fill over rustty-xev with the raw-self aliasing discipline; (mirihar) the exact renderthread_naive (UB) + renderthread_shipped (CLEAN) candidates to add to ${MIRIHAR}/src/main.rs modelling the construct-then-spawn shape (xev stubbed). Give precise edits, real line numbers, the bounded-pump Surface::draw KAT. If a genuine blocker, disposition 'blocked: <reason>' + STOP. Else 'ready'. Return the structured object.`,
  { label:'design:W3', phase:'Design', agentType:'general-purpose', schema:DESIGN_SCHEMA })

if (!design || (design.disposition||'').toLowerCase().startsWith('blocked')) {
  log(`W3 DESIGN BLOCKED: ${design ? design.disposition : 'design agent failed'}`)
  return { blocked:true, design }
}

phase('Adversary')
const ADV_SCHEMA = { type:'object', additionalProperties:false, required:['verdict','fixes','blocking_issues'], properties:{
  verdict:{enum:['SOUND','SOUND_WITH_FIXES','UNSOUND']}, fixes:{type:'array',items:{type:'string'}}, blocking_issues:{type:'array',items:{type:'string'}} } }
const adversary = await agent(
  `${rules}\n\nADVERSARY PHASE. Design at ${DESIGN}. BREAK it: (1) R1 — threadMain + EVERY callback faithful to Thread.zig (every comment, timer arithmetic, the min-contrast/cursor-blink logic, startDrawTimer/stopDrawTimer)? any dropped assertion? (2) ALIASING — does any threadMain/callback do &mut *self whole-struct reborrow while the child holds *mut Thread? does the main thread take &mut renderer_thread after spawn (surface.rs)? is deinit's stop.notify()+join ordering sound (no access after join / no double-deinit)? are the mirihar renderthread_naive/shipped candidates faithful to the REAL shape (would naive actually be UB and shipped actually CLEAN under BOTH SB and TB)? (3) E1 — does the Options<A>->Options mapping drop any field? Surface::draw wiring faithful to Surface.zig? RendererError alias correct? (4) R4 — check_acyclic stays green (no new edge)? (5) does draw_frame get called with the right sync arg + under the right lock (renderer mutex)? (6) does the bounded-pump KAT actually drive threadMain + observe a draw? List every fix + blocker. verdict SOUND only if zero.`,
  { label:'adversary:W3', phase:'Adversary', agentType:'Explore', schema:ADV_SCHEMA })

const needsFold = adversary && ((adversary.fixes && adversary.fixes.length) || (adversary.blocking_issues && adversary.blocking_issues.length))
if (needsFold) {
  phase('Fold')
  await agent(
    `${rules}\n\nFOLD PHASE. Adversary issues on ${DESIGN}. Fixes: ${JSON.stringify(adversary.fixes)}. Blocking: ${JSON.stringify(adversary.blocking_issues)}. Update ${DESIGN} IN PLACE to fix every one (re-read reference/current as needed). If a blocker is unresolvable add a top 'DISPOSITION: blocked: <reason>' line. Return a short summary.`,
    { label:'fold:W3', phase:'Fold', agentType:'general-purpose' })
}

// ---- Port: 2 SEQUENTIAL sub-agents (E1 then E2; both touch renderer_seam.rs/surface.rs) ----
phase('Port')
const PORT_SCHEMA = { type:'object', additionalProperties:false, required:['disposition','files_changed','build_ok','notes'], properties:{
  disposition:{type:'string',description:"'done' or 'stopped: <reason>'"},
  files_changed:{type:'array',items:{type:'string'}}, build_ok:{type:'boolean'}, notes:{type:'string'} } }

const portA = await agent(
  `${rules}\n\nPORT PHASE A (E1: real Renderer swap + Surface::draw). Implement STRICTLY per ${DESIGN} — if its first lines say 'DISPOSITION: blocked', STOP (no edits). Replace the renderer_seam.rs Renderer placeholder with the real rustty_renderer::Renderer (Options<A>->Options mapping; RendererError alias; keep the apprt-typed Options<A> fields in core), and wire surface.rs Surface::draw -> real draw_frame. Preserve every comment (R1). Build \`cargo build -p rustty-core\` + fix incrementally. Report files_changed, build_ok, notes. Do NOT git commit.`,
  { label:'port:W3-A', phase:'Port', agentType:'general-purpose', schema:PORT_SCHEMA })

let portB = null
if (portA && !(portA.disposition||'').toLowerCase().startsWith('stopped')) {
  portB = await agent(
    `${rules}\n\nPORT PHASE B (E2: fill the core-resident Thread<A> render event loop). Port A done. FILL the unimplemented!() bodies of crate::renderer_seam::Thread<A>::{init, threadMain(self_:*mut Thread<A>), deinit(&mut self)} + private callbacks (armLoop/wakeupCallback/drawCallback/renderCallback/cursorTimerCallback/drainMailbox/syncDrawTimer/startDrawTimer/stopDrawTimer) over rustty-xev per ${DESIGN}, calling (*self.renderer).draw_frame(sync). Use the raw-self aliasing discipline (NO &mut *self whole-struct reborrow; the child holds *mut Thread<A>). Do NOT redefine/move the Thread struct. Add the bounded-pump Surface::draw KAT. Build \`cargo build -p rustty-core\` + run \`cargo test -p rustty-core\` (the render-thread KAT). Report files_changed, build_ok, notes. Do NOT git commit.`,
    { label:'port:W3-B', phase:'Port', agentType:'general-purpose', schema:PORT_SCHEMA })
}

const stopped = (p) => !p || (p.disposition||'').toLowerCase().startsWith('stopped')
if (stopped(portA) || stopped(portB)) {
  log(`W3 PORT INCOMPLETE: A=${portA&&portA.disposition} B=${portB&&portB.disposition}`)
  return { design, adversary, portA, portB, mirihar: null, verify: null }
}

// ---- Mirihar: prove the render-thread aliasing on the isolated non-FFI harness ----
phase('Mirihar')
const MIRI_SCHEMA = { type:'object', additionalProperties:false, required:['naive_is_ub','shipped_is_clean','sb_ran','tb_ran','evidence'], properties:{
  naive_is_ub:{type:'boolean'}, shipped_is_clean:{type:'boolean'}, sb_ran:{type:'boolean'}, tb_ran:{type:'boolean'}, evidence:{type:'string'} } }
const mirihar = await agent(
  `${rules}\n\nMIRIHAR PHASE. Add renderthread_naive (UB) + renderthread_shipped (CLEAN) candidates to ${MIRIHAR}/src/main.rs per ${DESIGN}, modelling the render-thread construct-then-spawn aliasing (a heap-pinned struct; a std::thread spawned with a raw *mut to it running a threadMain-analogue loop; the main thread accessing ONLY Copy handles + *const after spawn; xev stubbed as no-ops). renderthread_naive must do the FORBIDDEN &mut *self whole-struct reborrow (or main-thread &mut after spawn) so it triggers UB; renderthread_shipped uses the raw-self discipline. Run BOTH under Stacked Borrows (default) AND Tree Borrows: \`cd ${MIRIHAR} && cargo +nightly miri run -- renderthread_naive\` (expect UB) and \`... renderthread_shipped\` (expect CLEAN), then \`MIRIFLAGS=-Zmiri-tree-borrows cargo +nightly miri run -- renderthread_shipped\` (expect CLEAN). Confirm naive=UB, shipped=CLEAN under BOTH. Update ${MIRIHAR}/README.md + .port/phase-2/MIRI_EXCEPTIONS.md. Report naive_is_ub/shipped_is_clean/sb_ran/tb_ran + the evidence (the actual miri output lines).`,
  { label:'mirihar:W3', phase:'Mirihar', agentType:'general-purpose', schema:MIRI_SCHEMA })

phase('Verify')
const VER_SCHEMA = { type:'object', additionalProperties:false, required:['dimension','verdict','findings'], properties:{
  dimension:{type:'string'}, verdict:{enum:['PASS','FAIL']}, findings:{type:'array',items:{type:'string'}} } }
const verify = await parallel([
  () => agent(`${rules}\n\nVERIFY (GATES). Independently re-run + confirm GREEN: cargo build --workspace; cargo test -p rustty-core; cargo clippy -p rustty-core --all-targets -- -D warnings; cargo fmt --check; bash .port/tools/gates/check_acyclic.sh; python3 .port/tools/gates/check_test_counts.py; python3 .port/tools/gates/check_comment_coverage.py. Report each. verdict PASS only if ALL green.`,
    { label:'verify:gates', phase:'Verify', agentType:'Explore', schema:VER_SCHEMA }),
  () => agent(`${rules}\n\nVERIFY (R1 FIDELITY + ALIASING). Diff W3 vs reference: (1) Thread::threadMain + every callback faithful to renderer/Thread.zig (comments, timer math, cursor-blink, startDrawTimer/stopDrawTimer)? (2) does ANY threadMain/callback do &mut *self whole-struct reborrow, or does surface.rs take &mut renderer_thread after spawn? (grep + reason) (3) E1 Options mapping drops no field; Surface::draw faithful; RendererError alias correct? (4) deinit stop.notify()+join sound (no post-join access, no double-deinit)? (5) draw_frame called under the renderer mutex with the right sync? (6) no lossy String? Report findings; verdict PASS only if R1 + aliasing sound.`,
    { label:'verify:fidelity', phase:'Verify', agentType:'Explore', schema:VER_SCHEMA }),
])

return { design, adversary, portA, portB, mirihar, verify }
