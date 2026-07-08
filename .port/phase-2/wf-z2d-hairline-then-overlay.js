export const meta = {
  name: 'z2d-hairline-then-overlay',
  description: 'Complete the last z2d-for-ghostty subset gap — hairline stroking (the polyline_plotter + hairline rasterizer that painter::stroke fast-paths to) — then unblock + fill the 4 rustty-renderer overlay seams that depend on it. The overlay port already proved it compiles + is faithful but panics at z2d hairline stroking (unported). Design (guarded: SCOPES the z2d hairline port + STOPS if it is a large/algorithmically-heavy multi-module effort disproportionate to the overlay-only payoff) -> adversary -> fold -> port (stop-if-no-design) -> verify.',
  phases: [
    { title: 'Design', detail: 'locate + scope the z2d hairline stroking reference (pkg/z2d — the polyline_plotter + hairline rasterizer / the painter stroke hairline fast-path); decide if it is a BOUNDED faithful port or a large algorithm; then the 4 overlay seams. WRITE the design doc (guarded). STOP-report scope=large if the z2d hairline is a big multi-module rasterizer (disproportionate to the overlay-only payoff).' },
    { title: 'Adversary-design', detail: 'attack: is the z2d hairline scope real+bounded; the rasterizer fidelity; does it break the ported non-hairline stroke / fills; the overlay seams; dropped test{}' },
    { title: 'Fold-fixes', detail: 'if the adversary returned any fixes OR blocking issues (regardless of verdict), fold them INTO the doc' },
    { title: 'Port', detail: 'implement exactly as the hardened doc specifies; STOP if the design doc is absent' },
    { title: 'Verify', detail: 'panel: R1-comment fidelity (manual), correctness (the hairline rasterizer + the overlay draw), gates + tests' },
  ],
}

const REPO = '/Users/gael/projects/rustty'
const REF = '/Users/gael/projects/ghostty'
const DESIGN_DOC = REPO + '/.port/phase-2/Z2D_HAIRLINE_THEN_OVERLAY_DESIGN.md'

const DESIGN_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['wrote_doc', 'hairline_scope', 'scope_methods', 'z2d_ref_files', 'test_plan', 'compile_boundary', 'summary'],
  properties: {
    wrote_doc: { type: 'boolean' },
    hairline_scope: { type: 'string' },
    scope_methods: { type: 'array', items: { type: 'string' } },
    z2d_ref_files: { type: 'array', items: { type: 'string' } },
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
  'REPO=' + REPO + '. REFERENCE (read-only)=' + REF + ' (Zig 0.15.2). z2d is a VENDORED dep at ' + REF + '/pkg/z2d (R3 vendored port). The design doc is ' + DESIGN_DOC + '.',
  'STATE: the rustty-renderer offscreen surface is comprehensively DONE + committed. The 4 renderer overlay seams (crates/rustty-renderer/src/overlay.rs highlight_grid_rect/highlight_pixel_rect/highlight_hyperlinks/highlight_semantic_prompts) were ported + proved faithful + COMPILE, but they PANIC at runtime because the ported z2d `painter::stroke` hairline fast-path is `unimplemented!("hairline stroking is not part of the z2d sprite subset")` (crates/rustty-z2d/src/painter.rs:225-228; internal/raster/mod.rs:8 marks `hairline` a blocker). A prior workflow ported the overlay seams then correctly STOPPED + reverted at this z2d panic. The RENDERER-side overlay port is a KNOWN-GOOD design (see .port/phase-2/RENDERER_OVERLAY_RENDER_DESIGN.md) — reuse it once z2d hairline lands.',
  'THE z2d HAIRLINE GAP (scope FIRST + the STOP gate): the ported z2d painter supports fills (multisample) + non-hairline stroke, but NOT hairline stroking (the 1px thin-line fast path the overlay uses via `ctx.hairline = true; ctx.stroke()`). Locate the reference in ' + REF + '/pkg/z2d (grep for hairline / polyline_plotter / the painter stroke hairline branch — it may be a `polyline_plotter.zig` + `hairline.zig` pair, or an inline fast-path in painter.zig / a stroke module). DETERMINE the true scope: is it a BOUNDED faithful port (a small fast-path + a Bresenham/Wu-style thin-line plotter) or a LARGE algorithm (a full anti-aliased polyline rasterizer)? If LARGE/multi-module + algorithmically heavy (disproportionate to the overlay-only payoff), STOP: record hairline_scope=large + do NOT port; recommend leaving the overlay deferred on z2d-hairline. If bounded, land it.',
  'Rules: ' + REPO + '/.port/PORT_RULES.md — R1 (100% fidelity: every line/comment/test{}; byte-faithful; the ported z2d modules must match the reference z2d algorithm exactly), R4 (ACYCLIC — z2d is a leaf; renderer already deps z2d), R5 (deferred = unimplemented!() seam + verbatim body). rustty-z2d has its OWN test suite — the hairline port must ADD its reference tests + keep the existing z2d tests green. G3b comment-coverage COVERS rustty-z2d (verify PORT STATUS) — keep it green.',
  'TESTABILITY: z2d hairline stroking is host-only (CPU rasterization) — port the reference z2d hairline stroke tests (if any) + assert a 1px line/rect stroke draws the expected coverage pixels. Then the overlay test (host-only, from the overlay design): build Terminal->RenderState, poke hyperlink metadata, Overlay::init + apply_features, assert the drawn overlay rect pixels (which now no longer panic).',
].join('\n')

phase('Design')
const design = await agent(
  'ARCHITECT: complete the z2d hairline stroking subset, then unblock the renderer overlay. You MUST write ' + DESIGN_DOC + ' with the Write tool (guarded — wrote_doc=true only if written). Design ONLY; touch NO crates/. FIRST locate + scope the z2d hairline reference in ' + REF + '/pkg/z2d (see COMMON) — record hairline_scope + z2d_ref_files. If the hairline rasterizer is LARGE/algorithmically-heavy (disproportionate to the overlay-only payoff), record hairline_scope=large, recommend deferring, and return WITHOUT designing the full port.\n' + COMMON + '\n\n' +
  'If bounded, DECIDE + specify (concrete Rust signature blocks mirroring the reference z2d hairline algorithm + the overlay unblock + OPEN RISKS): (1) the z2d hairline stroking port (the painter stroke hairline branch + the polyline/hairline plotter) faithful to the reference; (2) how it slots into the ported painter/raster module structure without breaking the existing non-hairline stroke/fills; (3) the 4 overlay seams (reuse RENDERER_OVERLAY_RENDER_DESIGN.md); (4) the z2d hairline tests + the overlay test; (5) dead-code + no-regression (existing z2d + 55 renderer tests stay green). Return the schema: wrote_doc, hairline_scope, scope_methods, z2d_ref_files, test_plan, compile_boundary, summary.',
  { label: 'design:z2d-hairline', agentType: 'general-purpose', schema: DESIGN_SCHEMA })

if (!design.wrote_doc) {
  log('DESIGN GUARD TRIPPED: design agent did NOT write the design doc. Aborting before port.')
  return { aborted: 'design-doc-not-written', design }
}
const _hs = String(design.hairline_scope).trim().toLowerCase()
if (_hs.startsWith('large') || _hs.startsWith('blocked') || _hs.startsWith('abort') || _hs.includes('disproportionate')) {
  log('Z2D HAIRLINE SCOPE=LARGE: ' + design.hairline_scope + ' — the z2d hairline rasterizer is disproportionate to the overlay-only payoff; DEFERRING. No port attempted. The renderer surface is complete except this z2d-gated debug overlay.')
  return { aborted: 'z2d-hairline-scope-large', design }
}
log('z2d-hairline design written (scope=bounded). scope=' + (design.scope_methods || []).join(',').slice(0, 120))

phase('Adversary-design')
const critique = await agent(
  'ADVERSARIAL reviewer: BREAK the z2d-hairline-then-overlay design at ' + DESIGN_DOC + '. Re-derive INDEPENDENTLY from the reference z2d.\n' + COMMON + '\n\n' +
  'Read the design + the reference z2d hairline stroking source (pkg/z2d) + the ported rustty-z2d painter/raster + the overlay seams. CHECK, breaking each: (1) SCOPE HONESTY — is the hairline port REALLY bounded, or did the design under-scope a large anti-aliased rasterizer? (2) FIDELITY — does the ported hairline algorithm match the reference z2d EXACTLY (coverage/plotting/endpoints)? Does it break the existing non-hairline stroke or the fill rasterizer? (3) the overlay unblock is faithful; (4) do the ported painter/raster APIs the design names exist? (5) TESTABILITY — are the z2d + overlay tests non-vacuous? (6) any reference test{} dropped (z2d tests + Overlay.zig)? Return the schema: verdict, blocking_issues, concrete fixes, summary.',
  { label: 'adversary:z2d-hairline-design', agentType: 'general-purpose', schema: CRITIQUE_SCHEMA })

log('z2d-hairline adversary: ' + critique.verdict + ' | blocking=' + critique.blocking_issues.length + ' fixes=' + critique.fixes.length)

phase('Fold-fixes')
const needsFold = (critique.fixes && critique.fixes.length) || (critique.blocking_issues && critique.blocking_issues.length)
let fold = null
if (needsFold) {
  fold = await agent(
    'DESIGN EDITOR: the adversary reviewed the z2d-hairline-then-overlay design (' + DESIGN_DOC + ') and returned verdict=' + critique.verdict + ' with fixes/issues to FOLD IN.\n' + COMMON + '\n\n' +
    'ADVERSARY BLOCKING ISSUES:\n- ' + (critique.blocking_issues || []).join('\n- ') + '\nADVERSARY FIXES:\n- ' + (critique.fixes || []).join('\n- ') + '\n\n' +
    'For EACH: verify against the reference z2d source yourself (reject any the source contradicts; note rejected + why in `folded`). Edit ' + DESIGN_DOC + ' to incorporate the valid ones. IF the adversary shows the hairline scope is actually LARGE, record that prominently so the port STOPS. Preserve structure. Touch NO crates/. You MUST end with the doc written. Return: wrote_doc, folded, summary.',
    { label: 'fold:adversary-fixes', agentType: 'general-purpose', schema: FOLD_SCHEMA })
  if (!fold.wrote_doc) {
    log('FOLD GUARD TRIPPED: doc not present after folding. Aborting before port.')
    return { aborted: 'design-doc-missing-after-fold', design, critique, fold }
  }
  log('z2d-hairline fold-fixes: ' + (fold.folded || []).length + ' items :: ' + String(fold.summary).slice(0, 120))
} else {
  log('z2d-hairline design fully clean — no fold needed; porting directly from the doc.')
}

phase('Port')
const portReport = await agent(
  'PORTING engineer: implement the z2d hairline stroking subset + the renderer overlay unblock EXACTLY as specified in the (adversary-hardened) design doc ' + DESIGN_DOC + '. FIRST Read the doc + verify it EXISTS + is substantive — if absent or a stub, STOP + report "design doc missing". If the doc concludes the hairline scope is LARGE/deferred, STOP + report that (do NOT self-improvise a large rasterizer).\n' + COMMON + '\n\n' +
  'Port the z2d hairline stroking (faithful to the reference z2d algorithm) into rustty-z2d, then fill the 4 overlay seams (reuse RENDERER_OVERLAY_RENDER_DESIGN.md). Every line+comment ported (R1 — the z2d algorithm byte-faithful; preserve EVERY reference comment; keep rustty-z2d G3b green); add the z2d hairline reference tests + the overlay test. If any gate can not go green HONESTLY (e.g. the hairline coverage does not match the reference), STOP + report the precise blocker (do NOT fake coverage). Anything still deferred stays an unimplemented!() seam with verbatim body.\n' +
  'GATES (run; ALL green, never weaken): cargo build --workspace, cargo fmt --check, cargo clippy --workspace --all-targets -- -D warnings, cargo test -p rustty-z2d -p rustty-renderer, bash .port/tools/gates/check_acyclic.sh, python3 .port/tools/gates/check_test_counts.py, python3 .port/tools/gates/check_comment_coverage.py. Do NOT git commit. Report: the z2d hairline port (files + line ranges), the overlay seams, the tests + results, each gate PASS/FAIL with tail, any reference comment/line you could not place.',
  { label: 'port:z2d-hairline-overlay', agentType: 'general-purpose' })

log('z2d-hairline port done. Head: ' + String(portReport).slice(0, 200))

phase('Verify')
const LENSES = [
  'R1 COMMENT + LINE FIDELITY: diff EVERY ported line AND comment of the z2d hairline stroking (rustty-z2d) against the reference pkg/z2d, AND the 4 overlay seams (overlay.rs) against Overlay.zig. Report any dropped/truncated/summarized comment (BLOCKER; keep rustty-z2d G3b green) or dropped branch/line. Confirm the z2d hairline reference test{}s + any Overlay.zig test{} are ported.',
  'CORRECTNESS: (a) the ported z2d hairline stroking matches the reference algorithm (coverage/plotting/endpoints) + does NOT break the existing non-hairline stroke or the fill rasterizer (run the z2d test suite); (b) the overlay draw is faithful + no longer panics (run the overlay test); (c) any aliasing. Confirm the tests are non-vacuous (real hairline coverage pixels + real overlay pixels).',
  'GATES + no-regression: RUN cargo build --workspace, cargo fmt --check, cargo clippy --workspace --all-targets -- -D warnings, cargo test -p rustty-z2d -p rustty-renderer, bash .port/tools/gates/check_acyclic.sh, python3 .port/tools/gates/check_test_counts.py, python3 .port/tools/gates/check_comment_coverage.py — report REAL results (tails). The existing z2d + renderer 55 tests must still pass.',
]
const verdicts = await parallel(LENSES.map((lens, i) => () =>
  agent(
    'ADVERSARIAL verifier for the z2d hairline stroking + renderer overlay (UNCOMMITTED working tree). LENS:\n' + lens + '\n' + COMMON + '\nFind the changed files (git diff/status), compare vs the reference, and RUN the gates your lens names. Report the schema: lens, verdict PASS/CONCERNS/FAIL, findings (severity, file, issue). Only REAL evidenced issues; if clean, empty findings + PASS + name the two things you attacked hardest in `lens`.',
    { label: 'verify:' + i, phase: 'Verify', agentType: 'general-purpose', schema: VERDICT_SCHEMA })))

const allFindings = verdicts.filter(Boolean).flatMap(v => (v.findings || []).map(f => ({ ...f, lens: v.lens })))
const blockers = allFindings.filter(f => f.severity === 'blocker')
log('z2d-hairline verify: ' + verdicts.filter(Boolean).map(v => v.verdict).join(', ') + ' | ' + blockers.length + ' blockers, ' + allFindings.length + ' findings')
return { design, design_critique: critique, fold, port_report: portReport, verify: verdicts, findings: allFindings, blockers }
