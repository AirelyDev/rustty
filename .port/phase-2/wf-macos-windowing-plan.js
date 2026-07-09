export const meta = {
  name: 'macos-windowing-plan',
  description: 'Author the detailed W0-W4 implementation-plan task specs for Sub-project #1 (macOS on-screen windowing) by deep-reading each chunk\'s reference bodies + current seams + objc2/xev APIs',
  phases: [
    { title: 'ChunkSpecs', detail: 'one deep-read agent per chunk W0-W4 emits exact task specs' },
    { title: 'Consistency', detail: 'cross-chunk interface + reference-coverage critic' },
  ],
}

const G = '/Users/gael/projects/ghostty'
const R = '/Users/gael/projects/rustty'
const SPEC = `${R}/docs/superpowers/specs/2026-07-08-rustty-completion-and-macos-windowing-design.md`

const common = `
REFERENCE (read-only): ghostty Zig 1.3.2-dev at ${G}. Port target: rustty at ${R}.
The approved design spec is ${SPEC} (read your chunk's section under §5.2/§5.3).
Governing rules: R1 (100% fidelity — every comment/test/assertion ported, Zig byte-slices -> &[u8]/Vec<u8>/CString never lossy String), R4 (acyclic — check_acyclic.sh), R5 (deferred = unimplemented!() + verbatim body), R6 (Miri pure / ASan FFI).
rustty-xev public API (the real event loop, Milestone A/B DONE): Loop::init()->Result<Loop,Error> / run(&mut self, RunMode)->Result<(),Error> / deinit(&mut self); Async::init()->Result<Async,Error> / notify(&self)->Result<(),NotifyError> / deinit; Timer::init()->Result<Timer,Error> / deinit; RunMode{NoWait,Once,UntilDone}; CallbackAction{Rearm,Disarm}; Completion. The proven live-loop harness pattern to imitate for tests: crates/rustty-termio/tests/live_shell.rs (bounded run(NoWait) pump + heap-pinned struct + raw-ptr aliasing discipline).
objc2 FFI path (R3-consistent, verified in GPU_RD_BLUEPRINT.md §2): objc2 0.6.4, objc2-foundation, objc2-metal 0.3.2, objc2-io-surface, objc2-core-graphics, objc2-core-foundation, block2, dispatch2 are in-lock; objc2-quartz-core (CAMetalLayer/CALayer) must be ADDED. objc2 renames selectors to snake_case with _ per colon.
YOUR JOB: produce EXACT implementation-plan task specs for your chunk. For EACH task: exact file paths (create/modify with line ranges), the precise ghostty reference source path+lines to port, the interfaces it PRODUCES (exact Rust signatures later tasks depend on) and CONSUMES, the objc2/xev call mapping for FFI/loop tasks, the actual Rust test KAT code (real, runnable — imitate live_shell.rs; macOS-gated #[cfg(target_os="macos")]; reuse the offscreen golden gate where pixels are involved), key SB/TB aliasing + fidelity port-notes, and the gate commands. NO placeholders, NO 'similar to' — real signatures and real test code. Read the actual current seam files and the actual reference bodies before writing; cite real line numbers.
Return ONLY the structured object.`

const TASK = {
  type: 'object', additionalProperties: false,
  required: ['id','name','files_create','files_modify','files_test','consumes','produces','reference','objc2_or_xev_mapping','test_kat','port_notes','gates','risk'],
  properties: {
    id: { type: 'string' }, name: { type: 'string' },
    files_create: { type: 'array', items: { type: 'string' } },
    files_modify: { type: 'array', items: { type: 'object', additionalProperties: false, required: ['path','lines'], properties: { path: {type:'string'}, lines: {type:'string'} } } },
    files_test: { type: 'array', items: { type: 'string' } },
    consumes: { type: 'array', items: { type: 'string' }, description: 'exact signatures/types this task uses from earlier tasks' },
    produces: { type: 'array', items: { type: 'string' }, description: 'exact Rust signatures/types later tasks rely on' },
    reference: { type: 'array', items: { type: 'object', additionalProperties: false, required: ['ghostty_path','lines','what'], properties: { ghostty_path:{type:'string'}, lines:{type:'string'}, what:{type:'string'} } } },
    objc2_or_xev_mapping: { type: 'array', items: { type: 'object', additionalProperties: false, required: ['ghostty_call','rust_call'], properties: { ghostty_call:{type:'string'}, rust_call:{type:'string'} } } },
    test_kat: { type: 'string', description: 'the actual runnable Rust test code for this task' },
    port_notes: { type: 'string', description: 'key SB/TB aliasing, ownership, byte-fidelity, acyclicity notes' },
    gates: { type: 'array', items: { type: 'string' }, description: 'exact gate commands to run' },
    risk: { type: 'string' },
  },
}
const CHUNK_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['chunk','goal','tasks','chunk_culminating_test','open_risks'],
  properties: {
    chunk: { type: 'string' }, goal: { type: 'string' },
    tasks: { type: 'array', items: TASK },
    chunk_culminating_test: { type: 'string', description: 'the KAT that proves the whole chunk deliverable' },
    open_risks: { type: 'array', items: { type: 'string' } },
  },
}

const CHUNKS = [
  { key: 'W0', focus: 'W0 — deps + cf_release_thread rewire + IOSurfaceLayer port. Tasks: (1) add objc2-quartz-core to crates/rustty-renderer/Cargo.toml + workspace; (2) rewire crates/rustty-os/src/cf_release_thread.rs — replace the LOCAL placeholder xev mod (the unimplemented!() Loop/Async at ~lines 330-373) with the REAL rustty-xev (reference: ghostty src/os/cf_release_thread.zig — the CF-object release background thread with a mach-port Async wakeup + xev Loop); (3) port ghostty src/renderer/metal/IOSurfaceLayer.zig (183 LOC — CALayer subclass via objc2 allocateClassPair, init/release/setSurface[main-thread dispatch]/setSurfaceSync/setDisplayCallback) to crates/rustty-renderer/src/metal/iosurface_layer.rs. Read: crates/rustty-os/src/cf_release_thread.zig equivalent + current cf_release_thread.rs, crates/rustty-xev API, ghostty IOSurfaceLayer.zig in full, existing crates/rustty-renderer/src/metal/*.rs for objc2 idioms already in use.' },
  { key: 'W1', focus: 'W1 — Metal on-screen delta + generic R5 present = FIRST PIXELS. Tasks around: crates/rustty-renderer/src/metal/metal_impl.rs (:334 init NSView attach wantsLayer/addSublayer/clipsToBounds + layer field; :384 loop_enter; surfaceSize from layer.bounds; present/presentLastTarget), crates/rustty-renderer/src/generic.rs (:1795 and :1841 R5 present seams), crates/rustty-renderer/src/metal/frame.rs (:198 async addCompletedHandler present). Reference: ghostty src/renderer/Metal.zig (init/present/presentLastTarget/surfaceSize/loopEnter) + src/renderer/generic.zig (drawFrame present path ~:1590) + src/renderer/metal/Frame.zig (:126 bufferCompleted->present). The present primitive is IOSurfaceLayer.setSurface(iosurface) from W0. Culminating KAT: Metal::init binds a CAMetalLayer to a test NSView and generic presents ONE real frame on-screen (reuse the offscreen golden state). Read the actual metal_impl.rs/generic.rs/frame.rs seams + reference bodies + W0 IOSurfaceLayer produces.' },
  { key: 'W2', focus: 'W2 — CVDisplayLink block in the RENDERER ONLY (apprt-free). CRITICAL R4 CONSTRAINT (verified): rustty-renderer does NOT and MUST NOT depend on rustty-apprt-api ("only apprt/config/core depend on renderer"). The render Thread is apprt-generic (surface:*apprt.Surface, app_mailbox:App.Mailbox) so it CANNOT live in rustty-renderer — it ALREADY lives fully-defined in core (crate::renderer_seam::Thread<A>) and is filled in W3, NOT here. DO NOT create/port any Thread type in rustty-renderer; DO NOT touch crates/rustty-renderer/src/thread.rs beyond DerivedConfig. YOUR SCOPE: port ONLY the apprt-free CVDisplayLink machinery into crates/rustty-renderer/src/generic.rs — the display_link field, hasVsync, setMacOSDisplayID, displayLinkCallback, loopEnter/loopExit, and the Metal api threadEnter/threadExit hooks (offscreen no-op; on-screen start/stop the display link). The displayLinkCallback notifies a draw_now xev::Async HANDLE that core passes IN (the renderer never names the Thread). Reference: the CVDisplayLink block in ghostty src/renderer/generic.zig + src/renderer/Metal.zig loopEnter/threadEnter/threadExit + src/renderer/metal/CVDisplayLink usage. Culminating KAT: a macOS test builds a CVDisplayLink bound to the main display and its callback notifies a real xev::Async (drain observed). Read generic.rs current display-link placeholder, ghostty generic.zig CVDisplayLink block + Metal.zig loopEnter, CoreVideo CVDisplayLink objc2/extern-C FFI availability, rustty-xev Async.' },
  { key: 'W3', focus: 'W3 — swap renderer_seam::Renderer->real AND FILL renderer_seam::Thread<A> bodies IN CORE. Part E1: replace the crates/rustty-core/src/renderer_seam.rs `Renderer` placeholder (unit struct; surfaceInit/init/finalizeSurfaceInit/threadEnter/drawFrame/deinit are unimplemented!() at ~lines 60-170) with the REAL monomorphic rustty_renderer::Renderer (the offscreen+on-screen renderer, on-screen present added in W1); wire crates/rustty-core/src/surface.rs Surface::draw -> real drawFrame. Part E2: FILL the unimplemented!() bodies of crate::renderer_seam::Thread<A: apprt::App>::{init, threadMain, deinit} + private callbacks (armLoop/wakeupCallback/drawCallback/renderCallback/cursorTimerCallback/drainMailbox/syncDrawTimer) over rustty-xev, calling (*self.renderer).drawFrame(sync). THE THREAD STRUCT + ALL FIELDS ALREADY EXIST at renderer_seam.rs:229-337 (loop_/wakeup/wakeup_c/stop/stop_c/render_h/render_c/draw_h/draw_c/draw_active/draw_now/draw_now_c/cursor_h/cursor_c/cursor_c_cancel/surface/renderer/state/mailbox/app_mailbox/config:thread::DerivedConfig/flags) — DO NOT redefine or move it. CANONICAL signatures (already wired in surface.rs, MUST match exactly): `Thread::<A>::init(alloc, config, rt_surface: *mut A::Surface, renderer: *mut Renderer, state: *mut State, app_mailbox: crate::app::Mailbox<A>) -> Result<Thread<A>, xev::Error>` (surface.rs:1857); `deinit(&mut self)` NON-consuming (stop.notify()+join in Surface::deinit); `pub unsafe fn threadMain(self_: *mut Thread<A>)` raw-ptr — the construct-then-spawn is ALREADY wired via RendererThreadArg(*mut Thread<A>) at surface.rs:1481-1489 spawning threadMain, with ptr::write(&raw mut (*self_).renderer_thread, render_thread) at :1965. The draw_now Async handle is what core hands to the renderer display-link (W2). Ownership proven on the mirihar harness (add renderthread_naive=UB / renderthread_shipped=CLEAN candidates). Reference: ghostty src/renderer/Thread.zig in full (threadMain loop + every callback) mapped onto the core-resident struct; src/Surface.zig draw. Culminating KAT: core Surface::draw drives a live drawFrame; a bounded-pump test ticks renderer_seam::Thread threadMain and observes a present; build + check_acyclic green. Read renderer_seam.rs:60-380, surface.rs:1480-1975 + the Surface::draw + Surface::deinit paths, ghostty renderer/Thread.zig, rustty-xev Loop/Async/Timer.' },
  { key: 'W4', focus: 'W4 — apprt-c macOS host handshake + blur/display_id/scale. Fill the macOS #[cfg(target_os="macos")] leaves in crates/rustty-apprt-c/src/lib.rs (~:2186, :2244) + crates/rustty-apprt/src/embedded.rs macOS leaves (~:1813-2176) needed to HOST a window: the NSView/CAMetalLayer host handshake (the embedded Surface receives the host layer via the C-ABI), window blur (CGSSetWindowBackgroundBlurRadius — private CGS), display_id, content-scale. Reference: ghostty src/apprt/embedded.zig (the macOS surface functions: newSurface layer handshake, setContentScale, the CGS blur, displayID) + src/apprt/surface.zig. quicklook/secure-input/inspector leaves STAY deferred (do not fill). Culminating KAT: a Rust integration harness opens an NSWindow, hosts the layer, and shows a live terminal frame (no Swift app). Read embedded.rs + apprt-c/lib.rs macOS leaves + ghostty embedded.zig macOS functions + objc2-app-kit availability for NSWindow/NSView in tests.' },
]

phase('ChunkSpecs')
const chunkSpecs = await parallel(CHUNKS.map((c) => () =>
  agent(`${common}\n\nYOUR CHUNK: ${c.focus}`, { label: `plan:${c.key}`, phase: 'ChunkSpecs', agentType: 'Explore', schema: CHUNK_SCHEMA })
    .then((r) => r || { chunk: c.key, goal: 'AGENT FAILED', tasks: [], chunk_culminating_test: '', open_risks: ['<agent-failed>'] })
))

phase('Consistency')
const blob = JSON.stringify(chunkSpecs.map((c) => ({ chunk: c.chunk, tasks: (c.tasks||[]).map((t) => ({ id: t.id, produces: t.produces, consumes: t.consumes, files: [...(t.files_create||[]), ...(t.files_modify||[]).map((m)=>m.path)] })) })))
const CONS = {
  type: 'object', additionalProperties: false, required: ['verdict','findings'],
  properties: {
    verdict: { enum: ['clean','issues'] },
    findings: { type: 'array', items: { type: 'object', additionalProperties: false, required: ['kind','detail','severity'], properties: {
      kind: { enum: ['interface-mismatch','coverage-gap','ordering','duplicate-file','placeholder'] }, detail: { type: 'string' }, severity: { enum: ['blocker','notable','minor'] } } } },
  },
}
const consistency = await agent(
  `Cross-chunk consistency + coverage critic for the macOS-windowing implementation plan. The per-chunk task interfaces:\n${blob}\n\nCheck: (1) every 'consumes' signature in a later task is 'produced' by an earlier task with the EXACT same name/type (interface-mismatch); (2) the plan covers every scope unit A-G of the spec §5.2 (coverage-gap): A IOSurfaceLayer+dep, B Metal on-screen delta, C generic R5+CVDisplayLink+loopEnter/Exit, D renderer Thread loop, E renderer_seam->real Renderer in core, F apprt-c macOS host leaves, G cf_release_thread rewire; (3) task ordering respects dependencies (ordering); (4) no two tasks create the same file (duplicate-file); (5) any task lacking real test code or with vague steps (placeholder). Read the spec ${SPEC} §5 to check coverage. Report findings; verdict 'clean' only if zero blocker/notable.`,
  { label: 'plan:consistency', phase: 'Consistency', agentType: 'Explore', schema: CONS },
)

return { chunkSpecs, consistency }
