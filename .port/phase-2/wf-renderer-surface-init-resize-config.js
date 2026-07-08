export const meta = {
  name: 'renderer-surface-init-resize-config',
  description: 'Fill the deferred rustty-renderer surface: DerivedConfig::init<C> (the config extraction — the renderer<->config acyclic crux), changeConfig<C> (config-swap), setScreenSize/resize (offscreen Target + FrameState + uniforms resize), setFontGrid (grid swap), and as much of Renderer::init<C> as is acyclic + offscreen-testable. Design (guarded) -> independent adversary -> fold -> port (stop-if-no-design) -> verify panel. UNCOMMITTED working tree.',
  phases: [
    { title: 'Design', detail: 'resolve the renderer<->config acyclic mechanism for DerivedConfig::init<C> (a renderer-defined accessor trait C impls, mapping each config field to a renderer-nameable type); scope init/resize/config-swap + what stays a live-surface seam; WRITE the design doc (guarded)' },
    { title: 'Adversary-design', detail: 'attack the acyclic mechanism (does any accessor return a config-only type? does the trait cycle?), the resize aliasing, the mock-C testability, dead-code, dropped test{}' },
    { title: 'Fold-fixes', detail: 'fold adversary findings INTO the doc; skipped when SOUND' },
    { title: 'Port', detail: 'implement exactly as the hardened doc specifies; STOP if the design doc is absent' },
    { title: 'Verify', detail: 'panel: R1-comment fidelity (manual), correctness (config extraction + resize + config-swap), acyclicity + gates' },
  ],
}

const REPO = '/Users/gael/projects/rustty'
const REF = '/Users/gael/projects/ghostty'
const DESIGN_DOC = REPO + '/.port/phase-2/RENDERER_SURFACE_INIT_RESIZE_CONFIG_DESIGN.md'

const DESIGN_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['wrote_doc', 'acyclic_mechanism', 'scope_methods', 'deferred_methods', 'testability', 'compile_boundary', 'summary'],
  properties: {
    wrote_doc: { type: 'boolean' },
    acyclic_mechanism: { type: 'string' },
    scope_methods: { type: 'array', items: { type: 'string' } },
    deferred_methods: { type: 'array', items: { type: 'string' } },
    testability: { type: 'string' },
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
  'REPO=' + REPO + '. REFERENCE (read-only)=' + REF + ' (Zig 0.15.2). The design doc is ' + DESIGN_DOC + '.',
  'STATE: GPU R-d is DONE + committed — the monomorphic Renderer<Metal> renders offscreen (golden-gated). This milestone fills the DEFERRED renderer surface: init / resize / config-swap. The relevant unimplemented!() seams (crates/rustty-renderer/src/generic.rs): DerivedConfig::init<C> (:574 ref, the config extraction — its verbatim ~30-field body is preserved as a comment above the seam), Renderer::init (:658 ref), changeConfig (:1841 ref), setScreenSize (:1919 ref), setFontGrid (:1070 ref). Also Message::initChangeConfig<C> (message.rs) + thread::init<C> (thread.rs) are related config-coupled seams.',
  'THE ACYCLIC CRUX (R4/G7 — the #1 design decision): rustty-config DEPENDS ON rustty-renderer (Cargo.toml:48 — the ChangeConfig back-edge), so the renderer CANNOT name ANY rustty_config type. The config-coupled seams are already GENERIC over `C` (`pub fn init<C>(config: &C)`, bare bound), with the concrete `C = rustty_config::Config` bound HIGHER UP (in core, which deps both). But there is NO accessor trait yet. DerivedConfig::init reads ~30 config fields (cursor-color/cursor-text/background/foreground/bold-color/faint-opacity/minimum-contrast/window-padding-color/selection-*/search-*/background-image-*/window-vsync/window-colorspace/alpha-blending/background-blur/scroll-to-bottom/font-thicken*/font-features/font-styles/font-shaping-break). DESIGN the mechanism: a renderer-defined `trait RendererConfig` (or similar) with an accessor per field, whose RETURN TYPES are all renderer-nameable (hoisted rustty-render-types value-types — AlphaBlending + the 6 hoisted; renderer-local TerminalColor/Color; or std types), impl-able by rustty_config::Config IN rustty-config (acyclic — config already deps renderer). Map EVERY field to a renderer-nameable accessor return type; any field whose only type is a config-crate type must be hoisted to rustty-render-types FIRST (the size/Health/AlphaBlending hoist precedent) or handled. This is the make-or-break of the milestone.',
  'RESIZE: setScreenSize (:1919) resizes for a new screen size — it updates self.size, resizes the offscreen Target + each FrameState (check FrameState::resize / Target resize are ported), and calls the already-ported update_screen_size_uniforms + update_font_grid_uniforms. setFontGrid (:1070) swaps self.font_grid + recomputes grid metrics + uniforms. Map the resize walk; flag any raw-ptr/aliasing hazard (memory rust-mut-args-strongly-protected / shaper-hook-sb-clean-interleave — if a new raw-alias-crossed-by-&mut arises, PROVE on .port/phase-2/mirihar-chunk3b/).',
  'TESTABILITY: the renderer canNOT dep rustty-config even as a DEV-dependency (that is the cycle). So test DerivedConfig::init<C>/changeConfig<C> with a MOCK `C` — a #[cfg(test)] struct in the renderer impl-ing the RendererConfig trait, returning fixed values — and assert the extracted DerivedConfig fields. Test setScreenSize/resize by resizing the offscreen golden Renderer + asserting self.size / the Target dims / the uniforms. Reuse the golden/test_renderer recipe.',
  'Renderer::init (:658): needs Metal::init(alloc, options) which needs a LIVE NSView/IOSurfaceLayer (windowing-host seam) + reads options.config (-> DerivedConfig::init<C>). DECIDE how much lands: the surface-INDEPENDENT setup (DerivedConfig::init, field init, init_shaders, SwapChain::init) can land; the live-NSView Metal::init attach likely STAYS a seam (no NSView in an offscreen test). Consider landing an offscreen-constructible init path or splitting init into core + the surface-attach seam. Maximize the acyclic + testable landing without faking the windowed surface.',
  'Rules: ' + REPO + '/.port/PORT_RULES.md — R1 (100% fidelity: every line/comment/test{} ported; byte-slices -> &[u8]/Vec<u8>/CString; the DerivedConfig::init verbatim body is preserved as a comment — port it faithfully), R4 (ACYCLIC — renderer must NOT gain a rustty-config edge; run check_acyclic.sh), R5 (anything still deferred = unimplemented!() seam + verbatim body). G3b comment-coverage SKIPS rustty-renderer (PORT STATUS: in progress), so the adversary/verify are the ONLY R1 backstop — check EVERY ported line+comment by hand.',
].join('\n')

phase('Design')
const design = await agent(
  'ARCHITECT: fill the deferred rustty-renderer surface (init / resize / config-swap). You MUST write ' + DESIGN_DOC + ' with the Write tool (guarded — wrote_doc=true only if written). Design ONLY; touch NO crates/. Read the reference (generic.zig DerivedConfig.init :574 / Renderer.init :658 / changeConfig :1841 / setScreenSize :1919 / setFontGrid :1070 / updateScreenSizeUniforms :1938 / updateFontGridUniforms :1115) + the ported crates/rustty-renderer/src/{generic,message,thread}.rs + the DerivedConfig struct + the hoisted rustty-render-types value-types + rustty-config Config (READ-ONLY, to enumerate the ~30 fields + their types).\n' + COMMON + '\n\n' +
  'DECIDE + specify (concrete Rust signature/trait blocks + a field-by-field config-extraction map + a resize walk + OPEN RISKS):\n' +
  '(1) THE ACYCLIC MECHANISM (the crux): the exact `trait RendererConfig` (name it) with one accessor per config field DerivedConfig::init reads, EACH return type renderer-nameable. Provide the FULL field->accessor->return-type table. Identify any field needing a NEW rustty-render-types hoist (list it + justify) vs already-hoisted/renderer-local/std. Specify DerivedConfig::init<C: RendererConfig> reading via the accessors (faithful to the verbatim reference body), and how rustty-config will impl RendererConfig for Config (acyclic — in rustty-config).\n' +
  '(2) SCOPE: which of DerivedConfig::init<C> / changeConfig<C> / setScreenSize/resize / setFontGrid / Renderer::init land now (maximize acyclic + offscreen-testable), and what stays a live-surface seam (Metal::init(NSView), the real windowed present). Give a coherent gate-green boundary.\n' +
  '(3) RESIZE WALK: map setScreenSize + setFontGrid to the ported Target/FrameState/uniforms APIs (confirm FrameState::resize / Target resize exist or must land); flag aliasing hazards.\n' +
  '(4) TESTABILITY: the exact mock-C test struct + the DerivedConfig-extraction asserts + the resize test (offscreen). Renderer canNOT dep rustty-config (even dev).\n' +
  '(5) DEAD-CODE + no-regression: how the new methods stay reachable; confirm the golden + existing 40 tests still pass.\n' +
  'Return the schema: wrote_doc, acyclic_mechanism, scope_methods, deferred_methods, testability, compile_boundary, summary.',
  { label: 'design:surface', agentType: 'general-purpose', schema: DESIGN_SCHEMA })

if (!design.wrote_doc) {
  log('DESIGN GUARD TRIPPED: design agent did NOT write the design doc. Aborting before port.')
  return { aborted: 'design-doc-not-written', design }
}
log('surface design written. acyclic=' + String(design.acyclic_mechanism).slice(0, 100) + ' | scope=' + (design.scope_methods || []).join(',').slice(0, 120))

phase('Adversary-design')
const critique = await agent(
  'ADVERSARIAL reviewer: BREAK the renderer-surface design at ' + DESIGN_DOC + '. Re-derive INDEPENDENTLY from source.\n' + COMMON + '\n\n' +
  'Read the design + generic.zig DerivedConfig.init/Renderer.init/changeConfig/setScreenSize/setFontGrid + the ported generic/message/thread.rs + rustty-config Config + rustty-render-types. CHECK, breaking each: (1) THE ACYCLIC MECHANISM — does EVERY RendererConfig accessor return a renderer-nameable type (NO rustty_config type leaks into the renderer, even transitively)? Does the trait or any needed hoist create a NEW cycle (run/inspect check_acyclic.sh mentally: renderer must gain NO config edge; render-types must gain NO vt/config edge)? Does the field->type map cover ALL ~30 fields faithfully (grep the reference verbatim body)? Any field silently dropped or mistyped? (2) FIDELITY — is DerivedConfig::init<C> a faithful port of the verbatim ~30-field body (every field, every toTerminalRGB/@max/@min/@intFromFloat/@floatCast/@ceil transform), or summarized/half-ported (illegal)? (3) RESIZE — does setScreenSize/setFontGrid map to real ported APIs; any raw-alias-crossed-by-&mut hazard? (4) TESTABILITY — can the mock-C test REALLY be written without a rustty-config dep; is it non-vacuous? (5) any reference test{} in range dropped? Return the schema: verdict, blocking_issues, concrete fixes, summary.',
  { label: 'adversary:surface-design', agentType: 'general-purpose', schema: CRITIQUE_SCHEMA })

log('surface adversary: ' + critique.verdict + ' | blocking=' + critique.blocking_issues.length + ' fixes=' + critique.fixes.length)

phase('Fold-fixes')
const needsFold = critique.verdict !== 'SOUND' && ((critique.fixes && critique.fixes.length) || (critique.blocking_issues && critique.blocking_issues.length))
let fold = null
if (needsFold) {
  fold = await agent(
    'DESIGN EDITOR: the adversary reviewed the renderer-surface design (' + DESIGN_DOC + ') and returned verdict=' + critique.verdict + '. FOLD its findings INTO the doc (single authoritative source).\n' + COMMON + '\n\n' +
    'ADVERSARY BLOCKING ISSUES:\n- ' + (critique.blocking_issues || []).join('\n- ') + '\nADVERSARY FIXES:\n- ' + (critique.fixes || []).join('\n- ') + '\n\n' +
    'For EACH: verify against the reference/rustty-config source yourself (reject any the source contradicts; note rejected + why in `folded`). Edit ' + DESIGN_DOC + ' to incorporate the valid ones. Preserve structure. Touch NO crates/. You MUST end with the doc written. Return: wrote_doc, folded, summary.',
    { label: 'fold:adversary-fixes', agentType: 'general-purpose', schema: FOLD_SCHEMA })
  if (!fold.wrote_doc) {
    log('FOLD GUARD TRIPPED: doc not present after folding. Aborting before port.')
    return { aborted: 'design-doc-missing-after-fold', design, critique, fold }
  }
  log('surface fold-fixes: ' + (fold.folded || []).length + ' items :: ' + String(fold.summary).slice(0, 120))
} else {
  log('surface design SOUND — no fold needed; porting directly from the doc.')
}

phase('Port')
const portReport = await agent(
  'PORTING engineer: implement the renderer surface (init/resize/config-swap) EXACTLY as specified in the (adversary-hardened) design doc ' + DESIGN_DOC + '. FIRST Read the doc + verify it EXISTS + is substantive — if absent or a stub, STOP + report "design doc missing".\n' + COMMON + '\n\n' +
  'Implement the RendererConfig accessor trait + any sanctioned rustty-render-types hoist + DerivedConfig::init<C>/changeConfig<C>/setScreenSize/setFontGrid/(the scoped part of) Renderer::init per the design. If the design has rustty-config impl RendererConfig for Config, make that edit in rustty-config (acyclic — config already deps renderer). Every line+comment ported (R1 — preserve the DerivedConfig::init verbatim body faithfully, EVERY field + transform); byte-faithful types; raw-ptr discipline for any resize aliasing (PROVE new patterns on .port/phase-2/mirihar-chunk3b/). Add the design-specified mock-C + resize tests. Anything still deferred stays an unimplemented!() seam with verbatim body.\n' +
  'GATES (run; ALL green, never weaken): cargo build --workspace, cargo fmt --check, cargo clippy --workspace --all-targets -- -D warnings, cargo test -p rustty-renderer (+ any crate you touched), bash .port/tools/gates/check_acyclic.sh (CRITICAL — renderer must gain NO config edge), python3 .port/tools/gates/check_test_counts.py, python3 .port/tools/gates/check_comment_coverage.py. If a gate can not go green HONESTLY, STOP + report the precise blocker. Do NOT git commit. Report: methods ported (line ranges), the RendererConfig trait + any hoist, the config-extraction fidelity, the resize handling, the mock-C + resize tests + results, each gate PASS/FAIL with tail, any reference comment/line you could not place.',
  { label: 'port:surface', agentType: 'general-purpose' })

log('surface port done. Head: ' + String(portReport).slice(0, 200))

phase('Verify')
const LENSES = [
  'R1 COMMENT + LINE FIDELITY (the PRIMARY lens — G3b does NOT gate rustty-renderer): diff EVERY ported line AND comment of DerivedConfig::init<C> (vs the verbatim ~30-field reference body) + changeConfig + setScreenSize + setFontGrid + (the ported part of) Renderer::init against generic.zig. Report any dropped/truncated/summarized comment (BLOCKER) or dropped field/line/branch/transform. Confirm any reference test{} in range is ported.',
  'CORRECTNESS + ACYCLICITY: (a) the config extraction — does DerivedConfig::init<C> read every field faithfully via the accessors + apply the exact transforms (toTerminalRGB/@max-min-clamp/@intFromFloat-ceil/@floatCast)? (b) the RendererConfig trait leaks NO rustty_config type into the renderer; run bash .port/tools/gates/check_acyclic.sh and confirm the renderer gained NO config edge + render-types gained no vt/config edge. (c) resize correctness (Target/FrameState/uniforms) + any raw-alias-crossed-by-&mut hazard (re-run/extend the mirihar harness if a new pattern). Run the mock-C + resize tests.',
  'GATES + no-regression: RUN cargo build --workspace, cargo fmt --check, cargo clippy --workspace --all-targets -- -D warnings, cargo test -p rustty-renderer (the golden + existing 40 must still pass), bash .port/tools/gates/check_acyclic.sh, python3 .port/tools/gates/check_test_counts.py, python3 .port/tools/gates/check_comment_coverage.py — report REAL results (tails). Confirm no workspace regression from the trait/hoist/config-impl.',
]
const verdicts = await parallel(LENSES.map((lens, i) => () =>
  agent(
    'ADVERSARIAL verifier for the renderer surface fill (init/resize/config-swap, UNCOMMITTED working tree). LENS:\n' + lens + '\n' + COMMON + '\nFind the changed files (git diff/status), compare vs ' + REF + '/src/renderer/generic.zig, and RUN the gates your lens names. Report the schema: lens, verdict PASS/CONCERNS/FAIL, findings (severity, file, issue). Only REAL evidenced issues; if clean, empty findings + PASS + name the two things you attacked hardest in `lens`.',
    { label: 'verify:' + i, phase: 'Verify', agentType: 'general-purpose', schema: VERDICT_SCHEMA })))

const allFindings = verdicts.filter(Boolean).flatMap(v => (v.findings || []).map(f => ({ ...f, lens: v.lens })))
const blockers = allFindings.filter(f => f.severity === 'blocker')
log('surface verify: ' + verdicts.filter(Boolean).map(v => v.verdict).join(', ') + ' | ' + blockers.length + ' blockers, ' + allFindings.length + ' findings')
return { design, design_critique: critique, fold, port_report: portReport, verify: verdicts, findings: allFindings, blockers }
