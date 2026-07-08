export const meta = {
  name: 'gpu-rd-chunk3c-uniforms',
  description: 'GPU R-d Chunk 3c: port generic.zig updateFrame (the frame orchestrator: live State{mutex,*mut Terminal} critical section + terminal_state.update + scroll-to-bottom + kitty images + it calls the already-ported rebuild_cells/rebuild_overlay) plus the uniforms cluster (updateFontGridUniforms/updateScreenSizeUniforms/updateCustomShaderUniformsFromState/updateCustomShaderUniformsForFrame/updateBgImageBuffer). Design (blank-slate, guarded) -> independent adversary -> fold fixes into the doc -> port (stop-if-no-design) -> verify panel (R1-comment fidelity + correctness/aliasing + gates).',
  phases: [
    { title: 'Design', detail: 'scope a coherent, testable 3c sub-unit (updateFrame + uniforms; decide setScreenSize/changeConfig/setFontGrid/DerivedConfig::init<C> in-or-deferred); WRITE GPU_RD_CHUNK3C_DESIGN.md (guarded)' },
    { title: 'Adversary-design', detail: 'independently re-derive from generic.zig + attack (scope, the live-State/Terminal aliasing, kitty-image seam, uniforms math fidelity, dead-code, dropped test{})' },
    { title: 'Fold-fixes', detail: 'fold adversary findings INTO the doc (single source of truth); skipped when SOUND' },
    { title: 'Port', detail: 'implement exactly as the hardened doc specifies; STOP if the design doc is absent' },
    { title: 'Verify', detail: 'panel: R1-comment fidelity (manual), correctness vs the updateFrame/uniforms walk, gates' },
  ],
}

const REPO = '/Users/gael/projects/rustty'
const REF = '/Users/gael/projects/ghostty'
const DESIGN_DOC = REPO + '/.port/phase-2/GPU_RD_CHUNK3C_DESIGN.md'

const DESIGN_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['wrote_doc', 'scope_methods', 'deferred_methods', 'state_type_decision', 'compile_boundary', 'summary'],
  properties: {
    wrote_doc: { type: 'boolean' },
    scope_methods: { type: 'array', items: { type: 'string' } },
    deferred_methods: { type: 'array', items: { type: 'string' } },
    state_type_decision: { type: 'string' },
    compile_boundary: { type: 'string' },
    summary: { type: 'string' },
  },
}
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
const FOLD_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['wrote_doc', 'folded', 'summary'],
  properties: {
    wrote_doc: { type: 'boolean' },
    folded: { type: 'array', items: { type: 'string' } },
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
  'REPO=' + REPO + '. REFERENCE (read-only)=' + REF + ' (Zig 0.15.2, src/renderer/generic.zig). The Chunk-3c design doc is ' + DESIGN_DOC + '.',
  'STATE: GPU R-d Chunks 0/1/2/3a/3b are DONE + committed. Chunk 3b just landed the CPU cell-fill in crates/rustty-renderer/src/generic.rs (mod macos_impl): rebuild_cells/rebuild_row/add_*/rebuild_overlay + PreeditRange + row.rs::never_extend_bg + a REAL TerminalColor enum. The Renderer struct (3a) already has the fields updateFrame/uniforms touch: uniforms: shaders::Uniforms, custom_shader_uniforms: shadertoy::Uniforms, scrollbar/scrollbar_dirty/last_bottom_node/last_bottom_y, terminal_state, size, config: DerivedConfig, images, bg_image*, swap_chain, first_frame_time/last_frame_time. DerivedConfig::init<C> is an unimplemented!() seam; the golden/test build DerivedConfig via a fixed-color fixture.',
  'Reference 3c method line ranges (generic.zig, verified): setFontGrid 1070; updateFontGridUniforms 1115; updateFrame 1123-1428; changeConfig 1841; setScreenSize 1919; updateScreenSizeUniforms 1938; updateBgImageBuffer 1979; updateCustomShaderUniformsFromState 2010; updateCustomShaderUniformsForFrame 2102. drawFrame (1430) + the GPU submit path stay CHUNK 4. updateFrame ends by calling self.updateCustomShaderUniformsFromState (:1418) then self.font_shaper.endFrame (:1423), and it is the SOLE caller of the already-ported rebuild_cells (:1364) / rebuild_overlay (:1349).',
  'updateFrame CRITICAL SECTION (the live-State surface): it builds a `Critical` struct under `state.mutex.lock()` — reads state.terminal.modes (synchronized_output early-return), state.terminal.screens.active.pages.getBottomRight (scroll-to-bottom via self.last_bottom_node/y + scrollViewport), runs self.terminal_state.update(state.terminal), drives self.images kitty (kitty_requires_update/kitty_update) + overlay_features under self.draw_mutex, snapshots scrollbar. `state` is the renderer thread State { mutex, terminal: *Terminal, ... } (NOT the ported state::State which is Preedit). Check whether that State type exists in rustty (crates/rustty-renderer/src/state.rs / thread.rs / message.rs) or must be ported/placeholder-filled.',
  'Rules: ' + REPO + '/.port/PORT_RULES.md — R1 (100% fidelity: every line/comment/test{} ported; byte-slices -> &[u8]/Vec<u8>/CString; memory rust-mut-args-strongly-protected), R4 (ACYCLIC; renderer canNOT dep rustty-config), R5 (deferred = unimplemented!() seam + verbatim body). MONOMORPHIC port (concrete Renderer over Metal).',
  'ALIASING LESSON (memory shaper-hook-sb-clean-interleave, from Chunk 3b — CRITICAL): a long-lived raw alias into a struct field interleaved with `&mut` access to that field is UB under BOTH Stacked and Tree Borrows. For updateFrame: `state.terminal: *mut Terminal` is read under the mutex and passed to terminal_state.update(...) — model the live-Terminal access with the raw-ptr discipline (no `&mut` that outlives an interleaved deref); Copy-hoist scalars where the reference re-reads across sub-calls. updateFrame does NOT re-drive the shaper run loop (it calls the already-fixed rebuild_cells), so the §C.3 shaper-hook remedy is inherited, not re-introduced. If a NEW raw-alias-crossed-by-&mut pattern appears, PROVE the fix on the isolated non-FFI Miri harness at .port/phase-2/mirihar-chunk3b/ (extend it), do NOT hand-reason Stacked Borrows.',
  'CRITICAL GATE-COVERAGE NOTE: G3b comment-coverage SKIPS rustty-renderer (its lib.rs says "PORT STATUS: in progress"), so generic.rs R1 comment fidelity is NOT gate-enforced — the adversary/verify are the ONLY R1 backstop. Check EVERY ported line+comment against the reference by hand.',
].join('\n')

phase('Design')
const design = await agent(
  'ARCHITECT for GPU R-d Chunk 3c — the FRAME ORCHESTRATOR + UNIFORMS. You MUST write ' + DESIGN_DOC + ' with the Write tool (guarded — return wrote_doc=true only if you actually wrote it). Design ONLY; touch NO crates/. Read the reference methods in ' + REF + '/src/renderer/generic.zig (the line ranges below) + the ported crates/rustty-renderer/src/{generic,state,shaders,shadertoy}.rs + the Renderer struct + rustty-vt render::RenderState::update + terminal Screen/PageList getBottomRight/scrollViewport + the Scrollbar type.\n' + COMMON + '\n\n' +
  'DECIDE + specify (concrete Rust signature blocks + a scope table + a RenderState/Terminal-walk map + OPEN RISKS):\n' +
  '(1) SCOPE BOUNDARY: which methods land in 3c vs deferred. Recommended 3c core = updateFrame + updateFontGridUniforms + updateScreenSizeUniforms + updateCustomShaderUniformsFromState + updateCustomShaderUniformsForFrame + updateBgImageBuffer (frame orchestration + all uniforms math). Recommended DEFER (to a 3c-tail or Chunk 4/5, justify): setScreenSize / changeConfig / setFontGrid (resize + config-swap plumbing) and DerivedConfig::init<C> — UNLESS updateFrame transitively needs them for a testable path. Give a coherent gate-green boundary (compiles under -D warnings + unit-testable: construct a live State{mutex, &mut Terminal}+RenderState, call update_frame, assert Contents + uniforms + scrollbar — NO drawFrame, NO GPU submit).\n' +
  '(2) THE LIVE State TYPE: does the renderer thread `State { mutex, terminal: *Terminal, ... }` that updateFrame takes already exist (state.rs/thread.rs/message.rs), or must it be ported / inhabited-placeholder-filled now? Specify its exact shape + how the 3c test constructs it. This is distinct from the ported state::State (Preedit).\n' +
  '(3) THE updateFrame CRITICAL-SECTION ALIASING: map every state.terminal / self.* access in the `Critical` block; flag any raw-alias-crossed-by-&mut hazard (memory rust-mut-args-strongly-protected + shaper-hook-sb-clean-interleave). state.terminal is a *mut Terminal read under the mutex — specify the raw-ptr discipline. Note updateFrame inherits the already-fixed rebuild_cells (no new shaper loop).\n' +
  '(4) THE UNIFORMS MATH: map updateScreenSizeUniforms / updateFontGridUniforms / updateCustomShaderUniforms{FromState,ForFrame} / updateBgImageBuffer to the ported shaders::Uniforms / shadertoy::Uniforms fields + self.size (render_types::Size) + grid_metrics. Flag the custom_shader_y_is_down / custom_shader_target consts (the monomorphic Metal values). Confirm each field exists.\n' +
  '(5) DEAD-CODE (-D warnings) + the 3c UNIT TEST: specify how updateFrame + the uniforms methods stay reachable (pub-widen the 3a way if needed) and the exact test (construct State+Terminal+RenderState, call update_frame, assert Contents filled + uniforms.{screen_size,grid_size,...} + scrollbar). macOS/Metal-gated + Metal-skip like the 3b test.\n' +
  'Return the schema: wrote_doc, scope_methods (exact 3c methods), deferred_methods, state_type_decision, compile_boundary, summary.',
  { label: 'design:chunk3c', agentType: 'general-purpose', schema: DESIGN_SCHEMA })

if (!design.wrote_doc) {
  log('DESIGN GUARD TRIPPED: design agent did NOT write GPU_RD_CHUNK3C_DESIGN.md. Aborting before port (no self-design).')
  return { aborted: 'design-doc-not-written', design }
}
log('3c design written. scope=' + (design.scope_methods || []).join(',').slice(0, 160))

phase('Adversary-design')
const critique = await agent(
  'ADVERSARIAL reviewer: BREAK the Chunk-3c design at ' + DESIGN_DOC + '. Re-derive INDEPENDENTLY from source — do not trust the doc.\n' + COMMON + '\n\n' +
  'Read the design + generic.zig updateFrame/updateFontGridUniforms/updateScreenSizeUniforms/updateCustomShaderUniforms{FromState,ForFrame}/updateBgImageBuffer + the ported generic.rs/state.rs/shaders.rs/shadertoy.rs + rustty-vt RenderState::update/Screen/PageList. CHECK, breaking each: (1) SCOPE coherence — does the 3c boundary compile + is it unit-testable with NO dangling reference to deferred (drawFrame/GPU-submit) methods? Does updateFrame transitively need any deferred method? (2) The live State type — does the proposed State{mutex,*Terminal} shape actually exist or is the port/placeholder plan sound + acyclic (no renderer->config/->apprt cycle)? Can the 3c test REALLY construct it + a live Terminal? (3) The critical-section aliasing — is every state.terminal/self.* access mapped to a real ported API; any raw-alias-crossed-by-&mut hazard (rust-mut-args-strongly-protected / shaper-hook-sb-clean-interleave) missed? Is the state.terminal *mut discipline sound? (4) UNIFORMS fidelity — does every uniforms field the math writes EXIST in shaders::Uniforms/shadertoy::Uniforms with the cited shape (grep to confirm); are the Metal consts (custom_shader_y_is_down/target) right? (5) any reference test{} in the ported ranges dropped (grep for `test`)? (6) R1 — any method proposed half-ported/summarized (illegal)? Return the schema: verdict, blocking_issues, concrete fixes, summary.',
  { label: 'adversary:3c-design', agentType: 'general-purpose', schema: CRITIQUE_SCHEMA })

log('3c adversary-design: ' + critique.verdict + ' | blocking=' + critique.blocking_issues.length + ' fixes=' + critique.fixes.length)

phase('Fold-fixes')
const needsFold = critique.verdict !== 'SOUND' && ((critique.fixes && critique.fixes.length) || (critique.blocking_issues && critique.blocking_issues.length))
let fold = null
if (needsFold) {
  fold = await agent(
    'DESIGN EDITOR: the adversary reviewed the Chunk-3c design (' + DESIGN_DOC + ') and returned verdict=' + critique.verdict + '. FOLD its findings INTO the doc so the porting engineer has a SINGLE authoritative source.\n' + COMMON + '\n\n' +
    'ADVERSARY BLOCKING ISSUES:\n- ' + (critique.blocking_issues || []).join('\n- ') + '\nADVERSARY FIXES:\n- ' + (critique.fixes || []).join('\n- ') + '\n\n' +
    'For EACH fix/blocking issue: verify it against the reference source yourself (the adversary can be wrong — do NOT fold a fix the source contradicts; note any rejected + why in `folded`). Then Edit ' + DESIGN_DOC + ' to incorporate the valid ones. Preserve the doc structure. Touch NO crates/. You MUST end with the doc written. Return: wrote_doc, folded (per-fix applied/rejected+why), summary.',
    { label: 'fold:adversary-fixes', agentType: 'general-purpose', schema: FOLD_SCHEMA })
  if (!fold.wrote_doc) {
    log('FOLD GUARD TRIPPED: fold agent reports the design doc is not present after folding. Aborting before port.')
    return { aborted: 'design-doc-missing-after-fold', design, critique, fold }
  }
  log('3c fold-fixes: ' + (fold.folded || []).length + ' items :: ' + String(fold.summary).slice(0, 120))
} else {
  log('3c design SOUND — no fold needed; porting directly from the doc.')
}

phase('Port')
const portReport = await agent(
  'PORTING engineer: implement Chunk 3c EXACTLY as specified in the (adversary-hardened) design doc ' + DESIGN_DOC + '. FIRST Read the doc and verify it EXISTS and is substantive — if absent or a stub, STOP immediately and report "design doc missing" (do NOT self-design or improvise). The doc is the single source of truth.\n' + COMMON + '\n\n' +
  'Port the scoped updateFrame + uniforms methods into crates/rustty-renderer/src/generic.rs (mod macos_impl), port/fill the live State type per the design, and wire the RenderState/Terminal walk + uniforms math faithfully. Every line+comment ported (R1 — G3b does NOT cover this crate, preserve EVERY reference comment verbatim by hand); the live-Terminal raw-ptr discipline per the design (rust-mut-args-strongly-protected / shaper-hook-sb-clean-interleave — if a NEW raw-alias-crossed-by-&mut pattern arises, PROVE it on .port/phase-2/mirihar-chunk3b/, do not hand-reason SB); byte-faithful types. Add the design-specified unit test (construct State+Terminal+RenderState, call update_frame, assert Contents + uniforms + scrollbar; macOS/Metal-gated + Metal-skip). Deferred methods (drawFrame etc.) stay unimplemented!() seams with verbatim bodies.\n' +
  'GATES (run; ALL green, never weaken): cargo build -p rustty-renderer, cargo build --workspace, cargo fmt -p rustty-renderer -- --check, cargo clippy -p rustty-renderer --all-targets -- -D warnings, cargo test -p rustty-renderer, bash .port/tools/gates/check_acyclic.sh, python3 .port/tools/gates/check_test_counts.py. If a gate can not go green HONESTLY, STOP + report the precise blocker (do NOT weaken/flail). Do NOT git commit. Report: methods ported (with generic.rs line ranges), the State-type handling, the uniforms math, the unit test + result, each gate PASS/FAIL with output tail, and any reference comment/line you could not place.',
  { label: 'port:updateFrame-uniforms', agentType: 'general-purpose' })

log('3c port done. Head: ' + String(portReport).slice(0, 200))

phase('Verify')
const LENSES = [
  'R1 COMMENT + LINE FIDELITY (the PRIMARY lens — G3b does NOT gate this crate): diff EVERY ported line AND comment of updateFrame + updateFontGridUniforms + updateScreenSizeUniforms + updateCustomShaderUniforms{FromState,ForFrame} + updateBgImageBuffer (+ any State-type port) against the reference generic.zig. Report any dropped/truncated/summarized comment (a dropped comment is a BLOCKER) and any dropped line/branch/assertion. Confirm any reference test{} in range is ported.',
  'CORRECTNESS vs the updateFrame/uniforms walk + ALIASING: trace the ported updateFrame critical section against generic.zig — does it lock/read state.terminal faithfully (synchronized_output early-return, scroll-to-bottom pin compare + scrollViewport, terminal_state.update, kitty images, scrollbar snapshot) and compute the uniforms math correctly (screen_size/grid_size/cell_size/projection + custom-shader uniforms + bg image)? Any raw-alias-crossed-by-&mut hazard (rust-mut-args-strongly-protected / shaper-hook-sb-clean-interleave)? If a new raw-alias pattern was introduced, RE-RUN / EXTEND the Miri harness .port/phase-2/mirihar-chunk3b/ under -Zmiri-stacked-borrows AND -Zmiri-tree-borrows to prove it clean. Run the 3c unit test.',
  'GATES + acyclicity + no-regression: RUN cargo build --workspace, cargo fmt -p rustty-renderer -- --check, cargo clippy -p rustty-renderer --all-targets -- -D warnings, cargo test -p rustty-renderer, bash .port/tools/gates/check_acyclic.sh, python3 .port/tools/gates/check_test_counts.py — report REAL results (tails). Confirm no renderer->config cycle and no workspace regression.',
]
const verdicts = await parallel(LENSES.map((lens, i) => () =>
  agent(
    'ADVERSARIAL verifier for GPU R-d Chunk 3c (updateFrame + uniforms, UNCOMMITTED working tree). LENS:\n' + lens + '\n' + COMMON + '\nFind the changed files (git diff/status), compare vs ' + REF + '/src/renderer/generic.zig, and RUN the gates your lens names. Report the schema: lens, verdict PASS/CONCERNS/FAIL, findings (severity, file, issue). Only REAL evidenced issues; if clean, empty findings + PASS + name the two things you attacked hardest in `lens`.',
    { label: 'verify:' + i, phase: 'Verify', agentType: 'general-purpose', schema: VERDICT_SCHEMA })))

const allFindings = verdicts.filter(Boolean).flatMap(v => (v.findings || []).map(f => ({ ...f, lens: v.lens })))
const blockers = allFindings.filter(f => f.severity === 'blocker')
log('3c verify: ' + verdicts.filter(Boolean).map(v => v.verdict).join(', ') + ' | ' + blockers.length + ' blockers, ' + allFindings.length + ' findings')
return { design, design_critique: critique, fold, port_report: portReport, verify: verdicts, findings: allFindings, blockers }
