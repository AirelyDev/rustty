export const meta = {
  name: 'gpu-rd-chunk3a-renderer-skeleton',
  description: 'GPU R-d Chunk 3a: resolve the DerivedConfig config-type cycle + port the monomorphic Renderer struct + SwapChain/FrameState, via design -> adversary -> port -> verify',
  phases: [
    { title: 'Design', detail: 'DerivedConfig cycle (hoist-vs-defer per config type) + Renderer struct + SwapChain compile-boundary -> GPU_RD_CHUNK3A_DESIGN.md' },
    { title: 'Adversary-design', detail: 'break it: cycle/acyclicity, parity gates, dead-code under -D warnings, struct completeness' },
    { title: 'Port', detail: 'config-type hoists then the Renderer struct + SwapChain/FrameState + DerivedConfig, gate-green' },
    { title: 'Verify', detail: 'panel: R1 fidelity, R4-acyclicity+gates, struct/SwapChain/Semaphore correctness' },
  ],
}

const REPO = '/Users/gael/projects/rustty'
const REF = '/Users/gael/projects/ghostty'

const CRITIQUE_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['verdict', 'blocking_issues', 'fixes', 'summary'],
  properties: {
    verdict: { type: 'string', enum: ['SOUND', 'SOUND-WITH-FIXES', 'BROKEN'] },
    blocking_issues: { type: 'array', items: { type: 'string' } },
    fixes: { type: 'array', items: { type: 'string' } },
    summary: { type: 'string' },
  },
}
const VERDICT_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['lens', 'verdict', 'findings'],
  properties: {
    lens: { type: 'string' },
    verdict: { type: 'string', enum: ['PASS', 'CONCERNS', 'FAIL'] },
    findings: {
      type: 'array',
      items: {
        type: 'object', additionalProperties: false,
        required: ['severity', 'file', 'issue'],
        properties: { severity: { type: 'string', enum: ['blocker', 'major', 'minor'] }, file: { type: 'string' }, issue: { type: 'string' } },
      },
    },
  },
}

const COMMON = [
  'REPO=' + REPO + '. REFERENCE (read-only)=' + REF + ' (Zig 0.15.2).',
  'rustty-renderer already has: the whole metal/* backend (api + buffer/texture/sampler/pipeline/render_pass/target/frame/shaders + Metal impl, Chunks 0/1a/1b/2); the R-a leaves (cell.rs = Contents/CellBg/fg_rows, shaders.rs = Uniforms/CellText/CellBg/Image/BgImage, cursor.rs, state.rs = State/mutex, link.rs, shadertoy.rs); image.rs + overlay.rs (P1). All committed.',
  'Rules: ' + REPO + '/.port/PORT_RULES.md — R1 (100% fidelity: every line/comment/test{} ported), R4 (crate graph ACYCLIC — gate check_acyclic.sh; renderer canNOT dep rustty-config, the Message::ChangeConfig back-edge cycles), R5 (never weaken).',
  'Decision committed: generic.zig ports MONOMORPHICALLY (concrete Renderer over concrete Metal types, NO trait hierarchy) — GPU_RD_TRAIT_DESIGN.md sec D gives the GenericRenderer struct field list. Precedent for the config cycle: rustty-renderer/src/thread.rs already has DerivedConfig::init<C>(config: ref C) generic over C solely because renderer->config cycles; the concrete C=rustty_config::Config binds higher up in core. AlphaBlending was ALREADY hoisted to rustty-render-types (Chunk 2A, commit 5b9eb65) via the PaddingBalance precedent (config-local free-fn parsing + re-export) — repeat that pattern for the remaining config types.',
].join('\n')

phase('Design')
const designSummary = await agent(
  'ARCHITECT for GPU R-d Chunk 3a — the RENDERER SKELETON. Write ' + REPO + '/.port/phase-2/GPU_RD_CHUNK3A_DESIGN.md (Write tool). Design ONLY; do not touch crates/.\n' + COMMON + '\n\n' +
  'Scope Chunk 3a = the pieces of generic.zig Renderer that are STRUCTURE (not the CPU-fill/draw METHOD bodies, which are Chunk 3b/c/4): (A) resolve the DerivedConfig config-type cycle, (B) the GenericRenderer struct decl (monomorphic, GPU_RD_TRAIT_DESIGN.md sec D) + its nested SwapChain/FrameState/CustomShaderState, (C) construct/teardown (init/deinit) enough that the struct COMPILES clean under clippy -D warnings (no dead_code).\n\n' +
  'PART A — the DerivedConfig cycle. generic.zig Renderer.DerivedConfig (' + REF + '/src/renderer/generic.zig:536-...) is a ~37-field struct; ~9 fields name configpkg.Config.* types the renderer CANNOT name (cycle): TerminalColor (x7 uses), WindowPaddingColor, RepeatablePath, Path, BackgroundImagePosition, BackgroundImageFit, WindowColorspace, BackgroundBlur, FontShapingBreak. (AlphaBlending already hoisted.) Also uses already-ported types: terminal.color.RGB + terminal.Style.BoldColor (rustty-vt), font.* (rustty-font), link.Set (renderer link.rs), ArenaAllocator (rustty-zigstd). DECIDE per config type: HOIST to rustty-render-types now (the AlphaBlending/PaddingBalance precedent — simple enums like WindowColorspace/BackgroundImagePosition/BackgroundImageFit/BackgroundBlur/FontShapingBreak are cheap) vs DEFER (the complex Path/RepeatablePath back golden-UNUSED fields custom_shaders/bg_image — can the field use a deferred placeholder type or must it hoist?). Read each type def in rustty-config (grep value_types.rs / config_struct.rs). For every HOIST: give the exact enum + methods + the config-local free-fn parsing refactor (bare_enum_names!) + the DeepEqual orphan move + the re-export + the call-site edits, EXACTLY as Chunk 2A did AlphaBlending (commit 5b9eb65). Confirm each keeps check_acyclic + check_test_counts + check_comment_coverage GREEN (the size/Health/PaddingBalance/AlphaBlending precedent for parity-gate attribution). Give DerivedConfig Rust struct + init<C>(config: ref C) signature; for Chunk 3a, init MAY stay the deferred-seam placeholder that thread.rs already uses IF the full extraction needs config accessors not yet abstracted — decide + justify.\n\n' +
  'PART B — the Renderer struct. Port generic.zig Renderer struct decl (GPU_RD_TRAIT_DESIGN.md sec D field list: alloc, draw_mutex: rustty_zigstd::thread::Mutex, config: DerivedConfig, surface_mailbox, grid_metrics, size, focused, cells: crate::cell::Contents, uniforms: crate::shaders::Uniforms, custom_shader_uniforms, font_grid/font_shaper/font_shaper_cache, images: crate::image::State, bg_image, bg_image_buffer, overlay: Option<crate::Overlay>, api: Metal, health: AtomicHealth, swap_chain, shaders: metal::shaders::Shaders, terminal_state, ...) + the nested SwapChain { frames: [FrameState; SWAP_CHAIN_COUNT], frame_index, frame_sema: Semaphore::with_permits(N), defunct } + FrameState { uniforms/cells/cells_bg: metal::Buffer<..>, grayscale/color: metal::Texture, target: metal::Target, bg_image_buffer, custom_shader_state } + CustomShaderState. Map every field type to the landed types. The crate-root alias (pub type Renderer = ...) replaces the RendererDerivedConfig{} placeholder in lib.rs.\n\n' +
  'PART C — compile boundary. Chunk 3a gate = cargo build -p rustty-renderer + clippy -D warnings (NO dead_code). Since a struct with unread fields warns, specify which init/deinit + trivial methods (SwapChain init/deinit/nextFrame/releaseFrame using frame_sema.wait()/post(); FrameState init/deinit) MUST land in 3a to exercise the fields, vs which big methods (updateFrame/rebuildCells/drawFrame/uniforms math) are DEFERRED to 3b/c/4 (unimplemented!() seams or not-yet-declared — decide which avoids dead_code cleanest). Port every SwapChain/FrameState line+comment (R1).\n\n' +
  'Deliver: concrete rust blocks for A (each hoist + DerivedConfig) + B (the structs) + C (the 3a method set), a per-config-type HOIST/DEFER table, the compile boundary, OPEN RISKS. Return a ~12-line summary: which config types hoist vs defer, the DerivedConfig approach, the 3a struct+method boundary, how dead_code is avoided.',
  { label: 'design:chunk3a', agentType: 'general-purpose' })

log('3a design written. Head: ' + String(designSummary).slice(0, 200))

phase('Adversary-design')
const critique = await agent(
  'ADVERSARIAL reviewer: BREAK the Chunk-3a design at ' + REPO + '/.port/phase-2/GPU_RD_CHUNK3A_DESIGN.md. Re-derive from source.\n' + COMMON + '\n\n' +
  'Read the design, generic.zig Renderer + DerivedConfig + SwapChain/FrameState (' + REF + '/src/renderer/generic.zig:~85-470, 536-660), the rustty-config defs of each config type named for hoist, rustty-render-types/src/lib.rs (the hoist target + the AlphaBlending precedent), and the parity gates. CHECK: (1) Does each proposed config-type HOIST actually break the cycle + keep check_acyclic GREEN (render-types must stay a leaf; no new edge to config/renderer)? (2) Does each hoist parsing/DeepEqual/re-export refactor COMPILE (orphan rule) + keep config tests + check_test_counts + check_comment_coverage GREEN (does moving each type def+comment misattribute a parity count)? (3) For DEFERRED config types — is the deferred placeholder type SAFE (does the golden DerivedConfig construction ever need the real type)? (4) Does the Renderer/SwapChain/FrameState struct name any type that does not exist among the landed leaves? Is the frame_sema wired as Semaphore::with_permits(SWAP_CHAIN_COUNT)? Is [FrameState; SWAP_CHAIN_COUNT] expressible (const array, monomorphic N=3)? (5) The DEAD-CODE hazard: will the 3a struct+method set actually compile clean under clippy -D warnings, or will unread fields/unused methods warn? Name any field/method that would warn. (6) Is any generic.zig Renderer/SwapChain/FrameState line or comment left unported (R1)? Any nested test{} in that range?\n' +
  'Return the schema: verdict, blocking_issues, concrete fixes (file+change), summary.',
  { label: 'adversary:3a-design', agentType: 'general-purpose', schema: CRITIQUE_SCHEMA })

log('3a adversary-design: ' + critique.verdict + ' | blocking=' + critique.blocking_issues.length + ' fixes=' + critique.fixes.length)
const fixesBlock = (critique.fixes && critique.fixes.length)
  ? '\n\nADVERSARY-DESIGN (' + critique.verdict + ') — HONOR these fixes:\n- ' + critique.fixes.join('\n- ') + '\nAddress the blocking issues:\n- ' + (critique.blocking_issues || []).join('\n- ')
  : ''

phase('Port')
const hoistReport = await agent(
  'PORTING engineer: implement Chunk 3a PART A ONLY — the DerivedConfig config-type hoists, exactly as designed in ' + REPO + '/.port/phase-2/GPU_RD_CHUNK3A_DESIGN.md Part A.' + fixesBlock + '\n' + COMMON + '\n\n' +
  'For EACH config type the design says HOIST: move it (enum + methods + DeepEqual) to rustty-render-types, re-export from rustty-config, refactor its bare_enum_names! parsing to config-local free fns, flip the call-sites — EXACTLY as commit 5b9eb65 did AlphaBlending. Behavior-preserving; every comment preserved (R1); parity-gate attribution per the design.\n' +
  'GATES (run; ALL green, never weaken): cargo build --workspace, cargo test --workspace (config tests MUST stay green), cargo fmt --check, cargo clippy --workspace --all-targets -- -D warnings, bash .port/tools/gates/check_acyclic.sh, python3 .port/tools/gates/check_test_counts.py, python3 .port/tools/gates/check_comment_coverage.py. STOP+report a precise blocker if any can not go green honestly. Do NOT git commit. Report: each type hoisted, the refactor pattern, each gate PASS/FAIL with tail.',
  { label: 'port:derivedconfig-hoists', agentType: 'general-purpose' })

log('3a hoists ported. Head: ' + String(hoistReport).slice(0, 160))

const structReport = await agent(
  'PORTING engineer: implement Chunk 3a PARTS B+C — the monomorphic Renderer struct + SwapChain/FrameState/CustomShaderState + DerivedConfig struct + the 3a init/deinit method set, exactly as designed in ' + REPO + '/.port/phase-2/GPU_RD_CHUNK3A_DESIGN.md. Part A (the config hoists) is ALREADY DONE in the working tree — build on the hoisted rustty-render-types types.' + fixesBlock + '\n' + COMMON + '\n\n' +
  'Port generic.zig Renderer struct decl + the nested SwapChain/FrameState/CustomShaderState + DerivedConfig (struct + init<C>/deinit) + ONLY the 3a method set the design specifies (SwapChain init/deinit/nextFrame/releaseFrame using frame_sema.wait()/post(); FrameState init/deinit; Renderer init/deinit skeleton) so it compiles clean under -D warnings with NO dead_code. The big CPU methods (updateFrame/rebuildCells/drawFrame/uniforms) are DEFERRED per the design (unimplemented!() seams or not-yet-declared). Every struct/SwapChain/FrameState line+comment ported (R1); frame_sema = rustty_zigstd::thread::Semaphore::with_permits(SWAP_CHAIN_COUNT); draw_mutex = rustty_zigstd::thread::Mutex; the crate-root pub-type-Renderer alias replaces the lib.rs RendererDerivedConfig{} placeholder. Wire generic.rs in lib.rs.\n' +
  'GATES (run; ALL green): cargo build -p rustty-renderer, cargo build --workspace (no regression), cargo fmt -p rustty-renderer -- --check, cargo clippy -p rustty-renderer --all-targets -- -D warnings (NO dead_code), cargo test -p rustty-renderer, bash .port/tools/gates/check_acyclic.sh. STOP+report a precise blocker if any can not go green honestly (do NOT weaken/flail — if avoiding dead_code needs a small method or a documented allow with an R1 justification, state it). Do NOT git commit. Report: the struct + method set, how dead_code was avoided, the SwapChain semaphore wiring, each gate PASS/FAIL with tail, any comment unplaced.',
  { label: 'port:renderer-struct', agentType: 'general-purpose' })

log('3a struct ported. Head: ' + String(structReport).slice(0, 160))

phase('Verify')
const LENSES = [
  'R1 fidelity: every generic.zig Renderer/SwapChain/FrameState/CustomShaderState/DerivedConfig struct-decl line+comment ported (diff vs the reference); the config-type hoists behavior-preserving (each moved enum/methods unchanged, parsing intact); deferred methods R5-seamed not silently dropped; any nested test{} in the ported range ported. Run python3 .port/tools/gates/check_test_counts.py + check_comment_coverage.py and report real results.',
  'R4 acyclicity + gates + no-regression: each config-type hoist keeps rustty-render-types a leaf (no edge to config/renderer) + check_acyclic GREEN; rustty-config tests + C-API + parity gates stay green; the whole workspace build/test/clippy(-D warnings, NO dead_code)/fmt green. Actually RUN: cargo build --workspace, cargo test -p rustty-config, cargo clippy --workspace --all-targets -- -D warnings, bash .port/tools/gates/check_acyclic.sh, python3 .port/tools/gates/check_test_counts.py — report real results.',
  'Struct/SwapChain/Semaphore correctness: the Renderer struct field types all resolve to the landed leaves; SwapChain frame_sema = Semaphore::with_permits(SWAP_CHAIN_COUNT=3) and nextFrame/releaseFrame use wait()/post() matching the Zig triple-buffer backpressure; [FrameState;3] + FrameState::init/deinit faithful; DerivedConfig init<C> matches the generic-over-C precedent. Look for a wrong permit count, a missing wait/post, a field-type mismatch, or a DerivedConfig that secretly names a config type (cycle).',
]
const verdicts = await parallel(LENSES.map((lens, i) => () =>
  agent(
    'ADVERSARIAL verifier for GPU R-d Chunk 3a (DerivedConfig hoists + Renderer skeleton, uncommitted). LENS:\n' + lens + '\n' + COMMON + '\nFind the changed files (git status/diff), compare vs ' + REF + '/src/renderer/generic.zig, RUN any gate your lens names. Report the schema: lens, verdict PASS/CONCERNS/FAIL, findings (severity, file, issue). Only REAL evidenced issues; if clean, empty findings + PASS + the two things you tried hardest to break.',
    { label: 'verify:' + i, phase: 'Verify', agentType: 'general-purpose', schema: VERDICT_SCHEMA })))

const allFindings = verdicts.filter(Boolean).flatMap(v => (v.findings || []).map(f => ({ ...f, lens: v.lens })))
const blockers = allFindings.filter(f => f.severity === 'blocker')
log('3a verify: ' + verdicts.filter(Boolean).map(v => v.verdict).join(', ') + ' | ' + blockers.length + ' blockers, ' + allFindings.length + ' findings')
return { design_summary: designSummary, design_critique: critique, hoist_report: hoistReport, struct_report: structReport, verify: verdicts, blockers }
