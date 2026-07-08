export const meta = {
  name: 'renderer-overlay-render',
  description: 'Fill the deferred rustty-renderer debug-overlay render path (overlay.rs, the LAST non-windowing renderer render seam): expose the RenderState hyperlink/semantic-prompt row/cell metadata in rustty-vt (bounded), then port the 4 z2d overlay seams — highlight_grid_rect / highlight_pixel_rect (the z2d rect draw helpers; the ported Context already has fill/stroke/paths + anti_aliasing_mode/hairline FIELDS) + highlight_hyperlinks / highlight_semantic_prompts (walk the RenderState metadata -> the rect helpers). Design (guarded, STOPS if the vt-metadata exposure is a big blocker) -> adversary -> fold -> port (stop-if-no-design) -> verify.',
  phases: [
    { title: 'Design', detail: 'scope the rustty-vt RenderState metadata the overlay needs (hyperlink + semantic-prompt row/cell) + whether it is a bounded additive exposure or blocked; map the 4 z2d seams to the ported Context (fields/fill/stroke); design the offscreen overlay test; WRITE the design doc (guarded). STOP-report if the vt-metadata is a large/blocked change.' },
    { title: 'Adversary-design', detail: 'attack: is the vt-metadata exposure minimal+additive (no vt behavior change)? the z2d draw fidelity, the AA/hairline field access, any aliasing, dead-code, dropped test{}' },
    { title: 'Fold-fixes', detail: 'if the adversary returned any fixes OR blocking issues (regardless of verdict), fold them INTO the doc; skipped only when truly clean' },
    { title: 'Port', detail: 'implement exactly as the hardened doc specifies; STOP if the design doc is absent' },
    { title: 'Verify', detail: 'panel: R1-comment fidelity (manual), correctness (z2d draw + the vt-metadata walk), gates + the offscreen overlay test' },
  ],
}

const REPO = '/Users/gael/projects/rustty'
const REF = '/Users/gael/projects/ghostty'
const DESIGN_DOC = REPO + '/.port/phase-2/RENDERER_OVERLAY_RENDER_DESIGN.md'

const DESIGN_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['wrote_doc', 'vt_metadata_feasible', 'scope_methods', 'z2d_note', 'test_plan', 'compile_boundary', 'summary'],
  properties: {
    wrote_doc: { type: 'boolean' },
    vt_metadata_feasible: { type: 'string' },
    scope_methods: { type: 'array', items: { type: 'string' } },
    z2d_note: { type: 'string' },
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
  'REPO=' + REPO + '. REFERENCE (read-only)=' + REF + ' (Zig 0.15.2, src/renderer/Overlay.zig — the CPU/z2d debug overlay). The design doc is ' + DESIGN_DOC + '.',
  'STATE: the rustty-renderer offscreen surface is comprehensively DONE + committed. This is the LAST non-windowing renderer render seam: the z2d debug overlay (crates/rustty-renderer/src/overlay.rs). The 4 R5 unimplemented!() seams: highlight_grid_rect (~:435), highlight_pixel_rect (~:499), highlight_hyperlinks (~:269), highlight_semantic_prompts (~:362). Each has its verbatim reference body preserved as a comment.',
  'THE z2d Context IS PORTED enough: crates/rustty-z2d/src/context.rs has move_to/line_to/close_path/set_source_to_pixel/set_line_width/fill/stroke, and `anti_aliasing_mode`/`hairline` as PUBLIC FIELDS (the reference `ctx.setAntiAliasingMode(.none)`/`ctx.setHairline(true)` -> field assignment `ctx.anti_aliasing_mode = AntiAliasMode::None; ctx.hairline = true;`). So highlight_grid_rect/highlight_pixel_rect (the rect draw helpers) are portable NOW. Confirm the exact field/method names.',
  'THE VT-METADATA DEPENDENCY (verify FIRST + the STOP gate): highlight_hyperlinks / highlight_semantic_prompts walk the terminal RenderState row/cell metadata — the reference reads hyperlink ranges + semantic-prompt info per row/cell. The ported rustty-vt render::RenderState/Row/Cell may NOT yet expose this. CONFIRM exactly what metadata is needed + whether exposing it in rustty-vt is a BOUNDED, ADDITIVE change (new accessors/fields on render::Row/Cell, no behavior change) or a large/blocked change. If large/blocked, STOP: land ONLY highlight_grid_rect/highlight_pixel_rect + keep the two higher-level seams deferred, OR abort — record vt_metadata_feasible + the disposition. Do NOT do a large speculative vt refactor.',
  'Rules: ' + REPO + '/.port/PORT_RULES.md — R1 (100% fidelity: every line/comment/test{}; byte-faithful), R4 (ACYCLIC), R5 (anything still deferred = unimplemented!() seam + verbatim body). Any rustty-vt change must be minimal+additive (no vt behavior/test change). G3b comment-coverage SKIPS rustty-renderer (but COVERS rustty-vt — any vt comment change must keep G3b green). The adversary/verify are the R1 backstop for the renderer.',
  'TESTABILITY: the overlay renders on the CPU to a z2d Surface (no Metal/GPU). So the overlay test is HOST-ONLY (no Metal-skip needed): build an Overlay, apply_features with a real Feature + a constructed RenderState carrying a hyperlink/semantic-prompt, and assert the z2d Surface pixels were drawn (a rect of the expected color at the expected cell). The existing rebuild_overlay_builds_and_clears (3b) test currently catch_unwind-wraps the seam — filling the seams lets it (or a new test) assert real overlay pixels.',
].join('\n')

phase('Design')
const design = await agent(
  'ARCHITECT: fill the deferred rustty-renderer z2d debug-overlay render path. You MUST write ' + DESIGN_DOC + ' with the Write tool (guarded — wrote_doc=true only if written). Design ONLY; touch NO crates/. FIRST determine the rustty-vt RenderState metadata the two higher-level seams need + whether exposing it is BOUNDED+ADDITIVE or blocked (see COMMON) — record vt_metadata_feasible + the disposition (land all 4, or land only grid_rect/pixel_rect, or abort).\n' + COMMON + '\n\n' +
  'DECIDE + specify (concrete Rust signature blocks + the z2d draw map + the vt-metadata walk + OPEN RISKS): (1) the exact rustty-vt additive exposure (accessors on render::Row/Cell for hyperlink ranges + semantic-prompt), minimal + no behavior change; (2) highlight_grid_rect / highlight_pixel_rect mapped to the ported z2d Context (the AA/hairline field assignments + move_to/line_to/close_path/fill/stroke), faithful to the verbatim reference; (3) highlight_hyperlinks / highlight_semantic_prompts (the RenderState walk -> the rect helpers); (4) the host-only offscreen overlay test (assert z2d Surface pixels); (5) dead-code + no-regression (the 55 renderer tests + the vt tests stay green). Return the schema: wrote_doc, vt_metadata_feasible, scope_methods, z2d_note, test_plan, compile_boundary, summary.',
  { label: 'design:overlay', agentType: 'general-purpose', schema: DESIGN_SCHEMA })

if (!design.wrote_doc) {
  log('DESIGN GUARD TRIPPED: design agent did NOT write the design doc. Aborting before port.')
  return { aborted: 'design-doc-not-written', design }
}
// STOP only on an EXPLICIT blocked disposition at the START of the field (the
// design was told to record `vt_metadata_feasible=blocked...`/`abort...` if blocked).
// A naive `.includes('abort')` false-matches phrases like "no abort" (which mean PROCEED).
const _vtf = String(design.vt_metadata_feasible).trim().toLowerCase()
if (_vtf.startsWith('blocked') || _vtf.startsWith('abort')) {
  log('OVERLAY BLOCKED: ' + design.vt_metadata_feasible + ' — the vt-metadata exposure is too large/blocked; aborting. No port attempted.')
  return { aborted: 'overlay-vt-metadata-blocked', design }
}
log('overlay design written. vt_metadata=' + String(design.vt_metadata_feasible).slice(0, 80) + ' | scope=' + (design.scope_methods || []).join(',').slice(0, 100))

phase('Adversary-design')
const critique = await agent(
  'ADVERSARIAL reviewer: BREAK the overlay-render design at ' + DESIGN_DOC + '. Re-derive INDEPENDENTLY from source.\n' + COMMON + '\n\n' +
  'Read the design + Overlay.zig highlightGridRect/highlightPixelRect/highlightHyperlinks/highlightSemanticPrompts + the ported overlay.rs seams + the rustty-z2d Context + the rustty-vt render::RenderState/Row/Cell. CHECK, breaking each: (1) VT-METADATA — is the proposed exposure REALLY minimal+additive (no vt behavior/test change; does the metadata the overlay needs actually exist in the terminal to expose)? Any vt-comment change that breaks G3b? (2) z2d FIDELITY — are grid_rect/pixel_rect faithful ports (the AA/hairline fields, the path build, fill+stroke order, the line width)? Do the ported Context methods/fields exist as claimed? (3) the hyperlink/semantic-prompt walk fidelity; (4) any aliasing; (5) TESTABILITY — will the host-only test really draw + assert overlay pixels non-vacuously? (6) any reference test{} dropped? Return the schema: verdict, blocking_issues, concrete fixes, summary.',
  { label: 'adversary:overlay-design', agentType: 'general-purpose', schema: CRITIQUE_SCHEMA })

log('overlay adversary: ' + critique.verdict + ' | blocking=' + critique.blocking_issues.length + ' fixes=' + critique.fixes.length)

phase('Fold-fixes')
const needsFold = (critique.fixes && critique.fixes.length) || (critique.blocking_issues && critique.blocking_issues.length)
let fold = null
if (needsFold) {
  fold = await agent(
    'DESIGN EDITOR: the adversary reviewed the overlay-render design (' + DESIGN_DOC + ') and returned verdict=' + critique.verdict + ' with fixes/issues to fold IN so the porting engineer has a SINGLE authoritative source.\n' + COMMON + '\n\n' +
    'ADVERSARY BLOCKING ISSUES:\n- ' + (critique.blocking_issues || []).join('\n- ') + '\nADVERSARY FIXES:\n- ' + (critique.fixes || []).join('\n- ') + '\n\n' +
    'For EACH: verify against the reference/rustty source yourself (reject any the source contradicts; note rejected + why in `folded`). Edit ' + DESIGN_DOC + ' to incorporate the valid ones. Preserve structure. Touch NO crates/. You MUST end with the doc written. Return: wrote_doc, folded, summary.',
    { label: 'fold:adversary-fixes', agentType: 'general-purpose', schema: FOLD_SCHEMA })
  if (!fold.wrote_doc) {
    log('FOLD GUARD TRIPPED: doc not present after folding. Aborting before port.')
    return { aborted: 'design-doc-missing-after-fold', design, critique, fold }
  }
  log('overlay fold-fixes: ' + (fold.folded || []).length + ' items :: ' + String(fold.summary).slice(0, 120))
} else {
  log('overlay design fully clean — no fold needed; porting directly from the doc.')
}

phase('Port')
const portReport = await agent(
  'PORTING engineer: implement the z2d debug-overlay render path EXACTLY as specified in the (adversary-hardened) design doc ' + DESIGN_DOC + '. FIRST Read the doc + verify it EXISTS + is substantive — if absent or a stub, STOP + report "design doc missing".\n' + COMMON + '\n\n' +
  'Implement the minimal+additive rustty-vt RenderState metadata exposure + the 4 overlay seams (grid_rect/pixel_rect/hyperlinks/semantic_prompts) per the design (or the reduced scope if the design deferred the higher-level two). Every line+comment ported (R1 — preserve EVERY reference comment verbatim; any vt-comment change keeps G3b green); byte-faithful; raw-ptr discipline for any aliasing. Add the design-specified host-only overlay test. Anything still deferred stays an unimplemented!() seam with verbatim body.\n' +
  'GATES (run; ALL green, never weaken): cargo build --workspace, cargo fmt --check, cargo clippy --workspace --all-targets -- -D warnings, cargo test -p rustty-renderer -p rustty-vt -p rustty-z2d (whatever you touched), bash .port/tools/gates/check_acyclic.sh, python3 .port/tools/gates/check_test_counts.py, python3 .port/tools/gates/check_comment_coverage.py (COVERS rustty-vt — must stay green). If a gate can not go green HONESTLY, STOP + report the precise blocker. Do NOT git commit. Report: seams filled (line ranges), the vt-metadata exposure, the z2d draw, the overlay test + result, each gate PASS/FAIL with tail, any reference comment/line you could not place.',
  { label: 'port:overlay', agentType: 'general-purpose' })

log('overlay port done. Head: ' + String(portReport).slice(0, 200))

phase('Verify')
const LENSES = [
  'R1 COMMENT + LINE FIDELITY: diff EVERY ported line AND comment of the 4 overlay seams (overlay.rs) + any rustty-vt metadata exposure against Overlay.zig / the vt reference. Report any dropped/truncated/summarized comment (BLOCKER for renderer; and any vt-comment change must keep G3b green) or dropped branch/line. Confirm any reference test{} in range is ported.',
  'CORRECTNESS: (a) the vt-metadata exposure is minimal+additive (no vt behavior/test change); (b) the z2d draw is faithful (AA/hairline fields, path build, fill+stroke, line width) + the ported Context methods/fields exist; (c) the hyperlink/semantic-prompt walk matches the reference; (d) any aliasing. Run the host-only overlay test; confirm non-vacuous (real overlay pixels drawn).',
  'GATES + no-regression: RUN cargo build --workspace, cargo fmt --check, cargo clippy --workspace --all-targets -- -D warnings, cargo test -p rustty-renderer -p rustty-vt -p rustty-z2d, bash .port/tools/gates/check_acyclic.sh, python3 .port/tools/gates/check_test_counts.py, python3 .port/tools/gates/check_comment_coverage.py — report REAL results (tails). The existing renderer 55 + vt/z2d tests must still pass.',
]
const verdicts = await parallel(LENSES.map((lens, i) => () =>
  agent(
    'ADVERSARIAL verifier for the renderer z2d overlay render path (UNCOMMITTED working tree). LENS:\n' + lens + '\n' + COMMON + '\nFind the changed files (git diff/status), compare vs the reference, and RUN the gates your lens names. Report the schema: lens, verdict PASS/CONCERNS/FAIL, findings (severity, file, issue). Only REAL evidenced issues; if clean, empty findings + PASS + name the two things you attacked hardest in `lens`.',
    { label: 'verify:' + i, phase: 'Verify', agentType: 'general-purpose', schema: VERDICT_SCHEMA })))

const allFindings = verdicts.filter(Boolean).flatMap(v => (v.findings || []).map(f => ({ ...f, lens: v.lens })))
const blockers = allFindings.filter(f => f.severity === 'blocker')
log('overlay verify: ' + verdicts.filter(Boolean).map(v => v.verdict).join(', ') + ' | ' + blockers.length + ' blockers, ' + allFindings.length + ' findings')
return { design, design_critique: critique, fold, port_report: portReport, verify: verdicts, findings: allFindings, blockers }
