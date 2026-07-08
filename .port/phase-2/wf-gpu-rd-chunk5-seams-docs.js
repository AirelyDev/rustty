export const meta = {
  name: 'gpu-rd-chunk5-seams-docs',
  description: 'GPU R-d Chunk 5 (finale): audit every remaining unimplemented!() seam in rustty-renderer (confirm each is legitimately deferred to a named Goal-2 tier, or flag any that should be reachable), make the HONEST PORT STATUS / G3b-comment-coverage decision (the crate still has large deferred surface — Renderer::init/deinit, resize, config-swap, real windowed present, custom-shader load, OpenGL/GTK/display-link — so do NOT falsely flip to "done"), and update the R-d docs to record the offscreen arc completion. Design (guarded) -> adversary -> fold -> port (stop-if-no-design) -> verify.',
  phases: [
    { title: 'Design', detail: 'enumerate + classify the 27 renderer unimplemented!() seams; decide the PORT STATUS / G3b-activation question honestly; scope the docs update; WRITE GPU_RD_CHUNK5_DESIGN.md (guarded)' },
    { title: 'Adversary-design', detail: 'attack the seam classification (any seam mis-labeled deferred that is actually reachable?) + the PORT STATUS honesty (is the crate really NOT done? would flipping G3b be premature/dishonest?) + docs accuracy' },
    { title: 'Fold-fixes', detail: 'fold adversary findings INTO the doc; skipped when SOUND' },
    { title: 'Port', detail: 'implement the seam-audit outcome (doc/comment updates only — no behavior change unless a seam is genuinely mis-deferred) + the PORT STATUS note + the R-d docs, EXACTLY per the hardened doc; STOP if the design doc is absent' },
    { title: 'Verify', detail: 'panel: seam-audit correctness (no reachable seam left as unimplemented!()), PORT STATUS/G3b honesty, gates (incl. whatever G3b decision was made)' },
  ],
}

const REPO = '/Users/gael/projects/rustty'
const REF = '/Users/gael/projects/ghostty'
const DESIGN_DOC = REPO + '/.port/phase-2/GPU_RD_CHUNK5_DESIGN.md'

const DESIGN_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['wrote_doc', 'seam_count', 'seam_classification', 'port_status_decision', 'g3b_decision', 'docs_plan', 'summary'],
  properties: {
    wrote_doc: { type: 'boolean' },
    seam_count: { type: 'number' },
    seam_classification: { type: 'string' },
    port_status_decision: { type: 'string' },
    g3b_decision: { type: 'string' },
    docs_plan: { type: 'string' },
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
  'REPO=' + REPO + '. REFERENCE (read-only)=' + REF + ' (Zig 0.15.2). The Chunk-5 design doc is ' + DESIGN_DOC + '.',
  'STATE: GPU R-d Chunks 0/1/2/3a/3b/3c/4 are DONE + committed. The renderer now RENDERS: the offscreen golden test (generic::macos_impl::tests::golden_renders_cursor_fg_bg) renders "Ab"+cursor via draw_frame and asserts real IOSurface pixels. Chunk 5 is the FINALE: seam audit + docs + the honest PORT STATUS / G3b decision. NO new heavy porting.',
  'THE SEAMS: `grep -rn "unimplemented!" crates/rustty-renderer/src/**/*.rs` (~27 sites — 7 in generic.rs, ~20 in metal/*). For EACH, classify: (a) legitimately DEFERRED to a NAMED Goal-2 tier (Renderer::init/deinit needing a live NSView/apprt surface; changeConfig/setScreenSize/setFontGrid resize+config-swap; the real windowed layer-present path; prepBackgroundImage/uploadBackgroundImage file I/O; shadertoy::load_from_files custom-shader load; the OpenGL/Linux backend; GTK; display-link; kitty pixel path) with a clear reason it is not needed for the OFFSCREEN R-d slice; OR (b) a seam that is ACTUALLY REACHABLE on a ported path and must be filled (a BUG). Any (b) is a blocker to surface.',
  'THE PORT STATUS / G3b DECISION (the crux — be HONEST): crates/rustty-renderer/src/lib.rs says "PORT STATUS: in progress"; check_comment_coverage.py (G3b) SKIPS a crate UNTIL a file says "PORT STATUS: done" (grep the gate: it matches `PORT STATUS:\\s*done`). The renderer still has substantial DEFERRED surface (the ~27 seams above), so it is NOT wholly done — flipping to "PORT STATUS: done" would be DISHONEST and would prematurely activate G3b comment-coverage on the whole crate. DECIDE + justify: keep an honest "in progress" (with an updated, accurate status of the R-d offscreen slice = done, the surface/resize/config/OpenGL/GTK = deferred), OR a scoped mechanism, OR (only if genuinely defensible) activate G3b. Do NOT weaken honesty for gate-green.',
  'Rules: ' + REPO + '/.port/PORT_RULES.md — R1 (fidelity), R4 (acyclic), R5 (deferred = unimplemented!() seam + verbatim body). This chunk is mostly doc/comment/status — any code change must be behavior-preserving unless fixing a genuinely mis-deferred (reachable) seam.',
  'CRITICAL GATE-COVERAGE NOTE: G3b currently SKIPS rustty-renderer. If the design keeps it skipped, the adversary/verify remain the ONLY R1 backstop. If the design activates G3b, the port must make the crate comment-coverage-CLEAN (run check_comment_coverage.py and fix misses) — a real effort, only undertake if the honesty decision supports it.',
].join('\n')

phase('Design')
const design = await agent(
  'ARCHITECT for GPU R-d Chunk 5 — the FINALE (seams + docs + the PORT STATUS/G3b decision). You MUST write ' + DESIGN_DOC + ' with the Write tool (guarded — wrote_doc=true only if you actually wrote it). Design ONLY; touch NO crates/.\n' + COMMON + '\n\n' +
  'DO: (1) `grep -rn "unimplemented!" crates/rustty-renderer/src` and read each site + its surrounding method + the reference method it stands for. Produce a SEAM AUDIT TABLE: seam -> reference method -> classification (DEFERRED to <named tier> with the reason it is not on the offscreen R-d path, OR REACHABLE-BUG). Trace whether ANY deferred seam is actually reachable from a ported/tested path (draw_frame/update_frame/rebuild_cells/the golden test) — if so it is a bug, flag it.\n' +
  '(2) Make the honest PORT STATUS / G3b decision (see COMMON). Recommend + justify. If keeping "in progress", specify the exact updated status text (R-d offscreen slice = done; enumerate the deferred surface). If activating G3b, specify the plan to make the crate comment-coverage-clean + the risk.\n' +
  '(3) Scope the DOCS update: mark the GPU R-d offscreen arc complete in the relevant .port/phase-2 blueprint/design docs + a concise completion summary (what renders, what is deferred, the mirihar-proven aliasing, the golden gate). List the exact files/sections to touch.\n' +
  'Return the schema: wrote_doc, seam_count, seam_classification (summary of the table), port_status_decision, g3b_decision, docs_plan, summary.',
  { label: 'design:chunk5', agentType: 'general-purpose', schema: DESIGN_SCHEMA })

if (!design.wrote_doc) {
  log('DESIGN GUARD TRIPPED: design agent did NOT write GPU_RD_CHUNK5_DESIGN.md. Aborting before port.')
  return { aborted: 'design-doc-not-written', design }
}
log('5 design written. seams=' + design.seam_count + ' port_status=' + String(design.port_status_decision).slice(0, 80))

phase('Adversary-design')
const critique = await agent(
  'ADVERSARIAL reviewer: BREAK the Chunk-5 finale design at ' + DESIGN_DOC + '. Re-derive INDEPENDENTLY.\n' + COMMON + '\n\n' +
  'Read the design + independently `grep -rn "unimplemented!" crates/rustty-renderer/src` + trace reachability. CHECK: (1) SEAM AUDIT — is EVERY seam classified DEFERRED actually unreachable from a ported/tested path (draw_frame/update_frame/rebuild_cells/golden)? Try to find a seam the design calls "deferred" that a passing test or draw_frame actually hits at runtime (that would be a latent panic = a REAL bug the finale must NOT paper over). Is any tier label wrong? (2) PORT STATUS HONESTY — is the decision honest? If it keeps "in progress", is the updated status accurate (does it over-claim)? If it flips to "done"/activates G3b, is that DEFENSIBLE given the deferred surface, or is it dishonest gate-gaming? (3) DOCS — do the claimed completions match reality (does the golden really assert what the docs say; is the deferred list complete)? (4) any behavior change smuggled into a "docs-only" chunk? Return the schema: verdict, blocking_issues, concrete fixes, summary.',
  { label: 'adversary:5-design', agentType: 'general-purpose', schema: CRITIQUE_SCHEMA })

log('5 adversary-design: ' + critique.verdict + ' | blocking=' + critique.blocking_issues.length + ' fixes=' + critique.fixes.length)

phase('Fold-fixes')
const needsFold = critique.verdict !== 'SOUND' && ((critique.fixes && critique.fixes.length) || (critique.blocking_issues && critique.blocking_issues.length))
let fold = null
if (needsFold) {
  fold = await agent(
    'DESIGN EDITOR: the adversary reviewed the Chunk-5 design (' + DESIGN_DOC + ') and returned verdict=' + critique.verdict + '. FOLD its findings INTO the doc (single authoritative source).\n' + COMMON + '\n\n' +
    'ADVERSARY BLOCKING ISSUES:\n- ' + (critique.blocking_issues || []).join('\n- ') + '\nADVERSARY FIXES:\n- ' + (critique.fixes || []).join('\n- ') + '\n\n' +
    'For EACH: verify against the source yourself (reject any the source contradicts; note rejected + why in `folded`). Edit ' + DESIGN_DOC + ' to incorporate the valid ones. Preserve structure. Touch NO crates/. You MUST end with the doc written. Return: wrote_doc, folded, summary.',
    { label: 'fold:adversary-fixes', agentType: 'general-purpose', schema: FOLD_SCHEMA })
  if (!fold.wrote_doc) {
    log('FOLD GUARD TRIPPED: doc not present after folding. Aborting before port.')
    return { aborted: 'design-doc-missing-after-fold', design, critique, fold }
  }
  log('5 fold-fixes: ' + (fold.folded || []).length + ' items :: ' + String(fold.summary).slice(0, 120))
} else {
  log('5 design SOUND — no fold needed; porting directly from the doc.')
}

phase('Port')
const portReport = await agent(
  'PORTING engineer: implement Chunk 5 (the finale) EXACTLY as specified in the (adversary-hardened) design doc ' + DESIGN_DOC + '. FIRST Read the doc and verify it EXISTS + is substantive — if absent or a stub, STOP immediately and report "design doc missing".\n' + COMMON + '\n\n' +
  'Implement the seam-audit outcome + the PORT STATUS note + the R-d docs per the design. This is a FINALIZATION chunk: prefer doc/comment/status edits; do NOT change behavior EXCEPT to fill a seam the design proved genuinely reachable (a real bug). If the design activates G3b, run python3 .port/tools/gates/check_comment_coverage.py and fix every miss it reports on rustty-renderer (preserve reference comments verbatim). If it keeps "in progress", update the status text exactly as designed. Update the docs the design lists.\n' +
  'GATES (run; ALL green, never weaken): cargo build --workspace, cargo fmt --check, cargo clippy -p rustty-renderer --all-targets -- -D warnings, cargo test -p rustty-renderer, bash .port/tools/gates/check_acyclic.sh, python3 .port/tools/gates/check_test_counts.py, python3 .port/tools/gates/check_comment_coverage.py (report whether it now covers rustty-renderer or still skips it, per the design decision). If a gate can not go green HONESTLY, STOP + report the precise blocker. Do NOT git commit. Report: the seam-audit result (each seam + disposition), the PORT STATUS/G3b outcome, the docs touched, each gate PASS/FAIL with tail, any reachable seam you had to fill.',
  { label: 'port:seams-docs', agentType: 'general-purpose' })

log('5 port done. Head: ' + String(portReport).slice(0, 200))

phase('Verify')
const LENSES = [
  'SEAM-AUDIT CORRECTNESS: independently `grep -rn "unimplemented!" crates/rustty-renderer/src` and, for each, trace whether it is reachable from a ported/tested path (draw_frame/update_frame/rebuild_cells/the golden + uniforms tests). A seam that a PASSING test or the golden path actually reaches at runtime (latent panic) is a BLOCKER. Confirm the design/port classification matches reality; confirm no behavior was changed except a genuinely-reachable seam fill.',
  'PORT STATUS / G3b HONESTY + DOCS ACCURACY: is the shipped PORT STATUS note honest (not over-claiming "done" while surface is deferred; not gate-gaming)? If G3b was activated, is the crate genuinely comment-coverage-clean (run python3 .port/tools/gates/check_comment_coverage.py) AND is the "done" claim defensible? If kept "in progress", does the status accurately list what is done (R-d offscreen slice) vs deferred? Do the updated docs match reality (the golden asserts what they claim)?',
  'GATES + no-regression: RUN cargo build --workspace, cargo fmt --check, cargo clippy -p rustty-renderer --all-targets -- -D warnings, cargo test -p rustty-renderer, bash .port/tools/gates/check_acyclic.sh, python3 .port/tools/gates/check_test_counts.py, python3 .port/tools/gates/check_comment_coverage.py — report REAL results (tails). Confirm no workspace regression.',
]
const verdicts = await parallel(LENSES.map((lens, i) => () =>
  agent(
    'ADVERSARIAL verifier for GPU R-d Chunk 5 (the finale, UNCOMMITTED working tree). LENS:\n' + lens + '\n' + COMMON + '\nFind the changed files (git diff/status), and RUN the gates your lens names. Report the schema: lens, verdict PASS/CONCERNS/FAIL, findings (severity, file, issue). Only REAL evidenced issues; if clean, empty findings + PASS + name the two things you attacked hardest in `lens`.',
    { label: 'verify:' + i, phase: 'Verify', agentType: 'general-purpose', schema: VERDICT_SCHEMA })))

const allFindings = verdicts.filter(Boolean).flatMap(v => (v.findings || []).map(f => ({ ...f, lens: v.lens })))
const blockers = allFindings.filter(f => f.severity === 'blocker')
log('5 verify: ' + verdicts.filter(Boolean).map(v => v.verdict).join(', ') + ' | ' + blockers.length + ' blockers, ' + allFindings.length + ' findings')
return { design, design_critique: critique, fold, port_report: portReport, verify: verdicts, findings: allFindings, blockers }
