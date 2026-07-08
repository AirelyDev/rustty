export const meta = {
  name: 'core-renderer-config-integration',
  description: 'Wire the REAL rustty_renderer into rustty-core as far as the non-windowing surface allows — replace the temporal renderer_seam placeholders (DerivedConfig, RendererError, and any now-real Renderer/Thread method that does NOT need a live NSView) with the real rustty_renderer types, using the landed DerivedConfig::init<C: RendererConfig> + Message::initChangeConfig<C>. Keep Renderer::init/present/Thread-run-loop as documented windowing/xev seams. Design (guarded: STOPS if the whole seam is windowing-blocked with no non-windowing slice) -> adversary -> fold -> port (stop-if-no-design) -> verify.',
  phases: [
    { title: 'Design', detail: 'audit renderer_seam.rs (DerivedConfig/RendererError/Renderer/Thread<A>/Options<A>) vs the real rustty_renderer; determine which placeholders can be swapped to real NOW (non-windowing: DerivedConfig via DerivedConfig::init<C: RendererConfig>, RendererError, the config-change path) vs which stay windowing/xev seams (Renderer::init needs Metal::init(NSView); Thread run loop needs xev). WRITE the design doc (guarded). STOP-report if NO non-windowing slice exists.' },
    { title: 'Adversary-design', detail: 'attack: is each swap acyclic + genuinely non-windowing; does core actually USE the swapped type meaningfully (not a cosmetic swap); the config-change path fidelity; the seam boundary; dropped test{}' },
    { title: 'Fold-fixes', detail: 'if the adversary returned any fixes OR blocking issues (regardless of verdict), fold them INTO the doc' },
    { title: 'Port', detail: 'implement exactly as the hardened doc specifies; STOP if the design doc is absent' },
    { title: 'Verify', detail: 'panel: R1-comment fidelity (manual), correctness (the config derivation/change wiring), gates + tests' },
  ],
}

const REPO = '/Users/gael/projects/rustty'
const REF = '/Users/gael/projects/ghostty'
const DESIGN_DOC = REPO + '/.port/phase-2/CORE_RENDERER_CONFIG_INTEGRATION_DESIGN.md'

const DESIGN_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['wrote_doc', 'non_windowing_slice', 'scope_swaps', 'staying_seams', 'test_plan', 'compile_boundary', 'summary'],
  properties: {
    wrote_doc: { type: 'boolean' },
    non_windowing_slice: { type: 'string' },
    scope_swaps: { type: 'array', items: { type: 'string' } },
    staying_seams: { type: 'array', items: { type: 'string' } },
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
  'REPO=' + REPO + '. REFERENCE (read-only)=' + REF + ' (Zig 0.15.2, src/Surface.zig + src/renderer.zig). The design doc is ' + DESIGN_DOC + '.',
  'STATE: rustty-renderer is fully render-complete OFFSCREEN (golden-gated) + its config surface is done: DerivedConfig::init<C: RendererConfig> is REAL, Message::initChangeConfig<C> is REAL, the RendererConfig accessor trait is defined in rustty-renderer::config + impl-d for Config IN rustty-config. rustty-core ALREADY DEPS rustty-renderer (Cargo.toml:27) — the renderer_seam is a TEMPORAL placeholder (crates/rustty-core/src/renderer_seam.rs), NOT an acyclicity issue.',
  'renderer_seam.rs placeholders: RendererError (struct), DerivedConfig (init/deinit unimplemented!()), Renderer (surfaceInit/init/finalizeSurfaceInit/threadEnter/drawFrame unimplemented!()), Thread<A> (run-loop unimplemented!()). Core Surface::init (surface.rs ~:1735) calls renderer_seam::DerivedConfig::init(alloc, config) then renderer_seam::Renderer.init(options{config, font_grid, size, surface_mailbox, rt_surface, thread}).',
  'THE WINDOWING BOUNDARY (the STOP gate): Renderer::init needs Metal::init(rt_surface) — a LIVE NSView/IOSurfaceLayer (the M-2 windowing seam) — so full Renderer construction is windowing-blocked + NOT headless-testable. The Thread run loop is D6/xev-related. But the CONFIG plane is NOT windowing: DerivedConfig::init<C: RendererConfig>(alloc, &rustty_config::Config) works headlessly (core passes a real Config which impls RendererConfig). DETERMINE the maximal NON-WINDOWING slice: which renderer_seam placeholders (DerivedConfig, RendererError, the config-change message path via Message::initChangeConfig) can be swapped to the REAL rustty_renderer types NOW, and which (Renderer::init/present, Thread run loop) stay documented windowing/xev seams. If NO meaningful non-windowing slice exists (everything routes through Renderer::init), STOP: record non_windowing_slice=none + report.',
  'Rules: ' + REPO + '/.port/PORT_RULES.md — R1 (100% fidelity: every line/comment/test{}; byte-faithful), R4 (ACYCLIC — core already deps renderer; changing renderer_seam::X to rustty_renderer::X must NOT add a cycle; run check_acyclic.sh), R5 (windowing/xev parts stay unimplemented!() seam + verbatim body). A swap must be MEANINGFUL (core actually uses the real type, not a cosmetic rename) OR justified. G3b comment-coverage — keep green (core/renderer PORT STATUS).',
  'TESTABILITY: core canNOT dep rustty-config?? — NO: core ALREADY deps BOTH rustty-config (for Config) AND rustty-renderer, and binds the concrete C = rustty_config::Config (that is where the RendererConfig bound resolves — the whole point). So core CAN construct a real DerivedConfig from a real rustty_config::Config in a unit test (unlike the renderer crate, which cannot). Assert core derives a real DerivedConfig with the extracted fields, and (if wired) that Surface::updateConfig builds a real Message::ChangeConfig.',
].join('\n')

phase('Design')
const design = await agent(
  'ARCHITECT: wire the real rustty_renderer into rustty-core as far as the non-windowing surface allows. You MUST write ' + DESIGN_DOC + ' with the Write tool (guarded — wrote_doc=true only if written). Design ONLY; touch NO crates/. FIRST audit renderer_seam.rs vs the real rustty_renderer + trace how core USES each placeholder (surface.rs/app.rs) — determine the maximal NON-WINDOWING swap slice (see COMMON). If NO meaningful non-windowing slice exists, record non_windowing_slice=none + return.\n' + COMMON + '\n\n' +
  'If a slice exists, DECIDE + specify (concrete Rust signature blocks + the swap map + the seam boundary + OPEN RISKS): (1) which renderer_seam placeholders swap to the REAL rustty_renderer types NOW (recommend: DerivedConfig -> rustty_renderer::DerivedConfig via init<C: RendererConfig>, RendererError -> the real error, the config-change path via Message::initChangeConfig<C>) — and HOW core uses them meaningfully; (2) which stay documented windowing/xev seams (Renderer::init/present -> Metal::init(NSView); Thread run loop -> xev) with the verbatim body preserved; (3) the exact boundary in Surface::init (real DerivedConfig derived + passed to the Renderer::init seam); (4) the core unit tests (core CAN use a real Config -> real DerivedConfig); (5) dead-code + no-regression (existing core + renderer tests stay green). Return the schema: wrote_doc, non_windowing_slice, scope_swaps, staying_seams, test_plan, compile_boundary, summary.',
  { label: 'design:core-renderer', agentType: 'general-purpose', schema: DESIGN_SCHEMA })

if (!design.wrote_doc) {
  log('DESIGN GUARD TRIPPED: design agent did NOT write the design doc. Aborting before port.')
  return { aborted: 'design-doc-not-written', design }
}
const _nws = String(design.non_windowing_slice).trim().toLowerCase()
if (_nws.startsWith('none') || _nws.startsWith('blocked') || _nws.startsWith('no ') || _nws.startsWith('no-')) {
  log('NO NON-WINDOWING SLICE: ' + design.non_windowing_slice + ' — the renderer_seam is entirely windowing-blocked; DEFERRING. No port attempted. The renderer<->core render integration needs a live macOS surface.')
  return { aborted: 'core-renderer-windowing-blocked', design }
}
log('core-renderer design written (slice exists). swaps=' + (design.scope_swaps || []).join(',').slice(0, 120))

phase('Adversary-design')
const critique = await agent(
  'ADVERSARIAL reviewer: BREAK the core-renderer-config-integration design at ' + DESIGN_DOC + '. Re-derive INDEPENDENTLY from source.\n' + COMMON + '\n\n' +
  'Read the design + renderer_seam.rs + how core uses it (surface.rs/app.rs) + the real rustty_renderer DerivedConfig/Message + Surface.zig. CHECK, breaking each: (1) is each proposed swap ACYCLIC (no new cycle; run/inspect check_acyclic) + genuinely NON-WINDOWING (does it truly avoid Metal::init(NSView))? (2) MEANINGFUL — does core actually USE the swapped real type, or is it a cosmetic rename of a placeholder that is still only fed to the Renderer::init seam (if cosmetic, is it still worth landing / is the value overstated)? (3) FIDELITY — is the DerivedConfig derivation + the config-change path faithful to Surface.zig; is the seam boundary at the right place? (4) is the staying-seam set correct (nothing that is actually now-real left as a seam; nothing windowing wrongly landed)? (5) can core REALLY unit-test a real DerivedConfig from a real Config? (6) any reference test{} dropped? Return the schema: verdict, blocking_issues, concrete fixes, summary.',
  { label: 'adversary:core-renderer-design', agentType: 'general-purpose', schema: CRITIQUE_SCHEMA })

log('core-renderer adversary: ' + critique.verdict + ' | blocking=' + critique.blocking_issues.length + ' fixes=' + critique.fixes.length)

phase('Fold-fixes')
const needsFold = (critique.fixes && critique.fixes.length) || (critique.blocking_issues && critique.blocking_issues.length)
let fold = null
if (needsFold) {
  fold = await agent(
    'DESIGN EDITOR: the adversary reviewed the core-renderer-config-integration design (' + DESIGN_DOC + ') and returned verdict=' + critique.verdict + ' with fixes/issues to FOLD IN.\n' + COMMON + '\n\n' +
    'ADVERSARY BLOCKING ISSUES:\n- ' + (critique.blocking_issues || []).join('\n- ') + '\nADVERSARY FIXES:\n- ' + (critique.fixes || []).join('\n- ') + '\n\n' +
    'For EACH: verify against the reference/rustty source yourself (reject any the source contradicts; note rejected + why in `folded`). IF the adversary shows the slice is actually cosmetic/not-worth-it or windowing-blocked, record that prominently so the port can STOP. Edit ' + DESIGN_DOC + '. Preserve structure. Touch NO crates/. You MUST end with the doc written. Return: wrote_doc, folded, summary.',
    { label: 'fold:adversary-fixes', agentType: 'general-purpose', schema: FOLD_SCHEMA })
  if (!fold.wrote_doc) {
    log('FOLD GUARD TRIPPED: doc not present after folding. Aborting before port.')
    return { aborted: 'design-doc-missing-after-fold', design, critique, fold }
  }
  log('core-renderer fold-fixes: ' + (fold.folded || []).length + ' items :: ' + String(fold.summary).slice(0, 120))
} else {
  log('core-renderer design fully clean — no fold needed; porting directly from the doc.')
}

phase('Port')
const portReport = await agent(
  'PORTING engineer: implement the core-renderer-config integration EXACTLY as specified in the (adversary-hardened) design doc ' + DESIGN_DOC + '. FIRST Read the doc + verify it EXISTS + is substantive — if absent or a stub, STOP + report "design doc missing". If the doc concludes there is no worthwhile non-windowing slice, STOP + report that.\n' + COMMON + '\n\n' +
  'Swap the specified renderer_seam placeholders (DerivedConfig/RendererError/the config-change path) to the REAL rustty_renderer types in rustty-core, wiring the config derivation faithfully to Surface.zig. Keep the windowing/xev parts (Renderer::init/present, Thread run loop) as documented unimplemented!() seams with verbatim bodies. Every line+comment ported (R1 — preserve EVERY reference comment; keep G3b green); byte-faithful. Add the design-specified core unit tests (core uses a real Config -> real DerivedConfig).\n' +
  'GATES (run; ALL green, never weaken): cargo build --workspace, cargo fmt --check, cargo clippy --workspace --all-targets -- -D warnings, cargo test -p rustty-core (+ any crate touched), bash .port/tools/gates/check_acyclic.sh (CRITICAL — no new cycle), python3 .port/tools/gates/check_test_counts.py, python3 .port/tools/gates/check_comment_coverage.py. If a gate can not go green HONESTLY, STOP + report the precise blocker. Do NOT git commit. Report: the swaps landed (line ranges), what stays a seam + why, the core tests + results, each gate PASS/FAIL with tail, any reference comment/line you could not place.',
  { label: 'port:core-renderer', agentType: 'general-purpose' })

log('core-renderer port done. Head: ' + String(portReport).slice(0, 200))

phase('Verify')
const LENSES = [
  'R1 COMMENT + LINE FIDELITY: diff EVERY ported/changed line AND comment in rustty-core (renderer_seam.rs + surface.rs/app.rs where the swaps land) against Surface.zig / renderer.zig. Report any dropped/truncated/summarized comment (BLOCKER; keep G3b green) or dropped branch/line. Confirm the windowing/xev seams keep their verbatim bodies + any reference test{} is ported.',
  'CORRECTNESS + ACYCLICITY: (a) the DerivedConfig derivation + config-change path are faithful; (b) the swaps are acyclic — run bash .port/tools/gates/check_acyclic.sh (no new cycle); (c) core genuinely USES the real types (not cosmetic) + the seam boundary is right (nothing now-real left a seam; nothing windowing wrongly landed). Run the core tests; confirm core builds a real DerivedConfig from a real Config non-vacuously.',
  'GATES + no-regression: RUN cargo build --workspace, cargo fmt --check, cargo clippy --workspace --all-targets -- -D warnings, cargo test -p rustty-core, bash .port/tools/gates/check_acyclic.sh, python3 .port/tools/gates/check_test_counts.py, python3 .port/tools/gates/check_comment_coverage.py — report REAL results (tails). The existing core + renderer tests must still pass.',
]
const verdicts = await parallel(LENSES.map((lens, i) => () =>
  agent(
    'ADVERSARIAL verifier for the core-renderer-config integration (UNCOMMITTED working tree). LENS:\n' + lens + '\n' + COMMON + '\nFind the changed files (git diff/status), compare vs the reference, and RUN the gates your lens names. Report the schema: lens, verdict PASS/CONCERNS/FAIL, findings (severity, file, issue). Only REAL evidenced issues; if clean, empty findings + PASS + name the two things you attacked hardest in `lens`.',
    { label: 'verify:' + i, phase: 'Verify', agentType: 'general-purpose', schema: VERDICT_SCHEMA })))

const allFindings = verdicts.filter(Boolean).flatMap(v => (v.findings || []).map(f => ({ ...f, lens: v.lens })))
const blockers = allFindings.filter(f => f.severity === 'blocker')
log('core-renderer verify: ' + verdicts.filter(Boolean).map(v => v.verdict).join(', ') + ' | ' + blockers.length + ' blockers, ' + allFindings.length + ' findings')
return { design, design_critique: critique, fold, port_report: portReport, verify: verdicts, findings: allFindings, blockers }
