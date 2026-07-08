export const meta = {
  name: 'renderer-custom-shaders',
  description: 'Fill the deferred rustty-renderer custom-shader (shadertoy) path: un-defer the config custom_shaders RepeatablePath (renderer-local + the RendererConfig::custom_shaders accessor + the rustty-config impl), port shadertoy::load_from_files/load_from_file (read each GLSL file + transpile via the ALREADY-ported glsl->spirv->msl chain), wire init_shaders to load them + build the post_pipelines + set has_custom_shaders. The draw_frame post-process passes are ALREADY wired (Chunk 4, dead under has_custom_shaders=false) — activated. Design (guarded, STOPS if blocked) -> adversary -> fold -> port (stop-if-no-design) -> verify.',
  phases: [
    { title: 'Design', detail: 'resolve the acyclic custom_shaders RepeatablePath (renderer-local list-of-Path + the RendererConfig accessor); scope load_from_files (transpile orchestration) + the init_shaders wiring; confirm the draw_frame post-process passes are wired; design the offscreen test; WRITE the design doc (guarded). STOP-report if blocked.' },
    { title: 'Adversary-design', detail: 'attack: is the transpile chain reused faithfully? the RepeatablePath acyclic? load_from_files fidelity, the init_shaders/post_pipeline wiring, the (untested Chunk-4) post-process render, aliasing, dropped test{}' },
    { title: 'Fold-fixes', detail: 'fold adversary findings INTO the doc; skipped when SOUND' },
    { title: 'Port', detail: 'implement exactly as the hardened doc specifies; STOP if the design doc is absent' },
    { title: 'Verify', detail: 'panel: R1-comment fidelity (manual), correctness (load+transpile + post-process render), gates + the offscreen custom-shader test' },
  ],
}

const REPO = '/Users/gael/projects/rustty'
const REF = '/Users/gael/projects/ghostty'
const DESIGN_DOC = REPO + '/.port/phase-2/RENDERER_CUSTOM_SHADERS_DESIGN.md'

const DESIGN_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['wrote_doc', 'feasible', 'scope_methods', 'acyclic_note', 'render_wired_note', 'test_plan', 'compile_boundary', 'summary'],
  properties: {
    wrote_doc: { type: 'boolean' },
    feasible: { type: 'string' },
    scope_methods: { type: 'array', items: { type: 'string' } },
    acyclic_note: { type: 'string' },
    render_wired_note: { type: 'string' },
    test_plan: { type: 'string' },
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
  'REPO=' + REPO + '. REFERENCE (read-only)=' + REF + ' (Zig 0.15.2, src/renderer/shadertoy.zig + generic.zig). The design doc is ' + DESIGN_DOC + '.',
  'STATE: GPU R-d + the renderer config/resize surface + Kitty + background-image are DONE + committed. This milestone fills the DEFERRED custom-shader (shadertoy) path. ALREADY PORTED: the transpile chain crate::shadertoy::{glsl_from_shader, spirv_from_glsl, msl_from_spv, glsl_from_spv} (4 tests). DEFERRED SEAMS: shadertoy::load_from_files / load_from_file (shadertoy.rs ~:132/:163, commented-out verbatim reference bodies) + init_shaders custom-shader load (generic.rs, currently passes no custom shaders). The config custom_shaders field is the renderer-local `RepeatablePath` PLACEHOLDER (generic.rs:251). The bg-image milestone JUST established the exact pattern: un-defer a config path type + add a RendererConfig accessor + impl in rustty-config (acyclic).',
  'KEY: the draw_frame CUSTOM-SHADER POST-PROCESS PASSES are ALREADY WIRED (Chunk 4, generic.rs draw_frame: the has_custom_shaders branch + CustomShaderState::init/resize + the post_pipelines ping-pong) — they are DEAD only because has_custom_shaders==false + custom_shaders==RepeatablePath placeholder. Confirm this. So activating custom shaders = un-defer RepeatablePath + load_from_files + wire init_shaders to build the post_pipelines + set has_custom_shaders. NOTE: those Chunk-4 passes are UNTESTED (the golden uses no custom shaders) — the new test is their first exercise; flag any latent issue.',
  'THE ACYCLIC RepeatablePath (R4/G7): mirror the bg-image Path solution — a renderer-local repeatable path type (a list of the renderer-local `Path` enum, byte-faithful CString), RendererConfig::custom_shaders() added to the trait, impl-d IN rustty-config (config already deps renderer; NO renderer->config edge; check_acyclic green). Reuse the just-landed renderer-local `Path`.',
  'LOAD + TRANSPILE: load_from_files (shadertoy.zig loadFromFiles) reads each GLSL file + wraps (glsl_from_shader) + compiles (spirv_from_glsl) + transpiles to the target (msl_from_spv for Metal / glsl_from_spv otherwise) via the ALREADY-ported chain, returning the shader sources ([]const [:0]const u8 -> Vec<CString>). CUSTOM_SHADER_TARGET is the Metal const (.msl). Map faithfully; byte-faithful sources (CString/Vec<u8>, never lossy String).',
  'Rules: ' + REPO + '/.port/PORT_RULES.md — R1 (100% fidelity: every line/comment/test{}; byte-slices -> &[u8]/Vec<u8>/CString, NEVER lossy String — shader sources + paths), R4 (ACYCLIC — renderer gains NO config edge; run check_acyclic.sh), R5 (anything still deferred = unimplemented!() seam + verbatim body). G3b comment-coverage SKIPS rustty-renderer, so the adversary/verify are the ONLY R1 backstop.',
  'TESTABILITY: write a simple pass-through custom shader GLSL (fragColor = texture(iChannel0, ...)) to a temp file, point MockConfig::custom_shaders at it (renderer canNOT dep rustty-config), init/init_shaders (load+transpile+build post_pipelines) -> has_custom_shaders=true, draw_frame, and assert the post-process ran (the CustomShaderState textures created + the post_pipelines non-empty + the output still renders the cells — a pass-through shader leaves the image recognizable). macOS/Metal-gated + Metal-skip. A host-only test can assert load_from_files transpiles a GLSL file to non-empty MSL.',
].join('\n')

phase('Design')
const design = await agent(
  'ARCHITECT: fill the deferred rustty-renderer custom-shader path. You MUST write ' + DESIGN_DOC + ' with the Write tool (guarded — wrote_doc=true only if written). Design ONLY; touch NO crates/. FIRST confirm the transpile chain is reusable + the draw_frame post-process passes are wired + the RepeatablePath can be made acyclic (see COMMON) — if any is a hard blocker, still WRITE the doc recording feasible=blocked + the blocker + a fallback, and return.\n' + COMMON + '\n\n' +
  'If feasible, DECIDE + specify (concrete Rust signature blocks + the load/transpile map + the init_shaders wiring + OPEN RISKS): (1) the renderer-local RepeatablePath (list of Path) + RendererConfig::custom_shaders + the rustty-config impl + un-deferring DerivedConfig.custom_shaders; (2) shadertoy::load_from_files/load_from_file faithful to the reference (read file, glsl_from_shader, spirv_from_glsl, msl_from_spv per CUSTOM_SHADER_TARGET); (3) the init_shaders wiring (load -> post_pipelines -> has_custom_shaders) + confirm the draw_frame post-process passes activate; (4) the offscreen + host tests; (5) dead-code + no-regression (the golden + existing 49 tests still pass). Return the schema: wrote_doc, feasible, scope_methods, acyclic_note, render_wired_note, test_plan, compile_boundary, summary.',
  { label: 'design:customshaders', agentType: 'general-purpose', schema: DESIGN_SCHEMA })

if (!design.wrote_doc) {
  log('DESIGN GUARD TRIPPED: design agent did NOT write the design doc. Aborting before port.')
  return { aborted: 'design-doc-not-written', design }
}
if (String(design.feasible).toLowerCase().includes('blocked')) {
  log('CUSTOM-SHADERS BLOCKED: ' + design.feasible + ' — aborting this milestone (pick a different next surface). No port attempted.')
  return { aborted: 'custom-shaders-blocked', design }
}
log('custom-shaders design written. scope=' + (design.scope_methods || []).join(',').slice(0, 140))

phase('Adversary-design')
const critique = await agent(
  'ADVERSARIAL reviewer: BREAK the custom-shader design at ' + DESIGN_DOC + '. Re-derive INDEPENDENTLY from source.\n' + COMMON + '\n\n' +
  'Read the design + shadertoy.zig loadFromFiles/loadFromFile + generic.zig initShaders/drawFrame custom-shader passes + the ported crate::shadertoy + the RendererConfig trait + rustty-config. CHECK, breaking each: (1) TRANSPILE REUSE — is load_from_files a faithful port orchestrating the ALREADY-ported glsl_from_shader/spirv_from_glsl/msl_from_spv (no re-implementation, no summarization)? (2) ACYCLICITY — does the renderer-local RepeatablePath + RendererConfig::custom_shaders leak NO rustty_config type; no config edge (check_acyclic)? Byte-faithful (CString/Vec<u8>, not String)? (3) RENDER WIRING — are the draw_frame post-process passes REALLY wired + correctly activated by has_custom_shaders (the ping-pong front/back, the post_pipelines, the final-to-screen)? Any latent bug in the untested Chunk-4 passes the new test would hit? (4) init_shaders wiring fidelity; (5) any aliasing; (6) TESTABILITY — will the pass-through-shader test really build post_pipelines + exercise the passes; is it non-vacuous? (7) any reference test{} dropped? Return the schema: verdict, blocking_issues, concrete fixes, summary.',
  { label: 'adversary:customshaders-design', agentType: 'general-purpose', schema: CRITIQUE_SCHEMA })

log('custom-shaders adversary: ' + critique.verdict + ' | blocking=' + critique.blocking_issues.length + ' fixes=' + critique.fixes.length)

phase('Fold-fixes')
const needsFold = critique.verdict !== 'SOUND' && ((critique.fixes && critique.fixes.length) || (critique.blocking_issues && critique.blocking_issues.length))
let fold = null
if (needsFold) {
  fold = await agent(
    'DESIGN EDITOR: the adversary reviewed the custom-shader design (' + DESIGN_DOC + ') and returned verdict=' + critique.verdict + '. FOLD its findings INTO the doc (single authoritative source).\n' + COMMON + '\n\n' +
    'ADVERSARY BLOCKING ISSUES:\n- ' + (critique.blocking_issues || []).join('\n- ') + '\nADVERSARY FIXES:\n- ' + (critique.fixes || []).join('\n- ') + '\n\n' +
    'For EACH: verify against the reference/rustty source yourself (reject any the source contradicts; note rejected + why in `folded`). Edit ' + DESIGN_DOC + ' to incorporate the valid ones. Preserve structure. Touch NO crates/. You MUST end with the doc written. Return: wrote_doc, folded, summary.',
    { label: 'fold:adversary-fixes', agentType: 'general-purpose', schema: FOLD_SCHEMA })
  if (!fold.wrote_doc) {
    log('FOLD GUARD TRIPPED: doc not present after folding. Aborting before port.')
    return { aborted: 'design-doc-missing-after-fold', design, critique, fold }
  }
  log('custom-shaders fold-fixes: ' + (fold.folded || []).length + ' items :: ' + String(fold.summary).slice(0, 120))
} else {
  log('custom-shaders design SOUND — no fold needed; porting directly from the doc.')
}

phase('Port')
const portReport = await agent(
  'PORTING engineer: implement the custom-shader path EXACTLY as specified in the (adversary-hardened) design doc ' + DESIGN_DOC + '. FIRST Read the doc + verify it EXISTS + is substantive — if absent or a stub, STOP + report "design doc missing".\n' + COMMON + '\n\n' +
  'Implement the renderer-local RepeatablePath + RendererConfig::custom_shaders (+ the rustty-config impl) + shadertoy::load_from_files/load_from_file (orchestrating the already-ported transpile chain) + the init_shaders wiring, per the design. Every line+comment ported (R1 — preserve EVERY reference comment verbatim; shader sources + paths byte-faithful CString/Vec<u8>, NEVER lossy String); raw-ptr discipline for any aliasing. Add the design-specified tests. Anything still deferred stays an unimplemented!() seam with verbatim body.\n' +
  'GATES (run; ALL green, never weaken): cargo build --workspace, cargo fmt --check, cargo clippy --workspace --all-targets -- -D warnings, cargo test -p rustty-renderer (+ rustty-config if touched), bash .port/tools/gates/check_acyclic.sh (CRITICAL — no config edge), python3 .port/tools/gates/check_test_counts.py, python3 .port/tools/gates/check_comment_coverage.py. If a gate can not go green HONESTLY, STOP + report the precise blocker (a broken custom-shader render is a REAL failure to diagnose — the Chunk-4 passes were untested). Do NOT git commit. Report: methods ported (line ranges), the transpile orchestration, the RepeatablePath acyclicity, the init_shaders/render wiring, the tests + results, each gate PASS/FAIL with tail, any reference comment/line you could not place.',
  { label: 'port:customshaders', agentType: 'general-purpose' })

log('custom-shaders port done. Head: ' + String(portReport).slice(0, 200))

phase('Verify')
const LENSES = [
  'R1 COMMENT + LINE FIDELITY (the PRIMARY lens — G3b does NOT gate rustty-renderer): diff EVERY ported line AND comment of shadertoy::load_from_files/load_from_file + the init_shaders custom-shader wiring + the RepeatablePath/RendererConfig::custom_shaders additions against shadertoy.zig / generic.zig. Report any dropped/truncated/summarized comment (BLOCKER) or dropped branch/line. Confirm sources/paths byte-faithful (CString/Vec<u8>, NOT String). Confirm any reference test{} in range is ported.',
  'CORRECTNESS + ACYCLICITY: (a) load_from_files faithfully orchestrates the ported glsl->spirv->msl chain (a real GLSL transpiles to non-empty MSL — run the test); (b) the renderer-local RepeatablePath leaks NO rustty_config type — run bash .port/tools/gates/check_acyclic.sh (no config edge); (c) the init_shaders wiring + the draw_frame post-process passes actually run under has_custom_shaders (any latent bug in the untested Chunk-4 passes?); (d) any aliasing hazard. Run the offscreen custom-shader test; confirm non-vacuous (post_pipelines built + the passes exercised).',
  'GATES + no-regression: RUN cargo build --workspace, cargo fmt --check, cargo clippy --workspace --all-targets -- -D warnings, cargo test -p rustty-renderer, bash .port/tools/gates/check_acyclic.sh, python3 .port/tools/gates/check_test_counts.py, python3 .port/tools/gates/check_comment_coverage.py — report REAL results (tails). The golden + existing 49 tests must still pass.',
]
const verdicts = await parallel(LENSES.map((lens, i) => () =>
  agent(
    'ADVERSARIAL verifier for the renderer custom-shader path (UNCOMMITTED working tree). LENS:\n' + lens + '\n' + COMMON + '\nFind the changed files (git diff/status), compare vs the reference, and RUN the gates your lens names. Report the schema: lens, verdict PASS/CONCERNS/FAIL, findings (severity, file, issue). Only REAL evidenced issues; if clean, empty findings + PASS + name the two things you attacked hardest in `lens`.',
    { label: 'verify:' + i, phase: 'Verify', agentType: 'general-purpose', schema: VERDICT_SCHEMA })))

const allFindings = verdicts.filter(Boolean).flatMap(v => (v.findings || []).map(f => ({ ...f, lens: v.lens })))
const blockers = allFindings.filter(f => f.severity === 'blocker')
log('custom-shaders verify: ' + verdicts.filter(Boolean).map(v => v.verdict).join(', ') + ' | ' + blockers.length + ' blockers, ' + allFindings.length + ' findings')
return { design, design_critique: critique, fold, port_report: portReport, verify: verdicts, findings: allFindings, blockers }
