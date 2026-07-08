export const meta = {
  name: 'gpu-rd-chunk3b-cell-fill',
  description: 'GPU R-d Chunk 3b: port generic.zig CPU cell-fill (rebuildCells/rebuildRow/add*/rebuildOverlay + row.zig::neverExtendBg) filling the ported Contents from the rustty-vt RenderState, and promote the golden-relevant DerivedConfig/TerminalColor placeholders to real. The Chunk-3b DESIGN doc is already committed (v1, pre-adversary); this run VALIDATES+REFINES it (not a blank-slate rewrite) -> independent adversary attack -> fold fixes INTO the doc -> port (stop-if-no-design) -> verify (incl. an R1-comment lens, since G3b skips the in-progress renderer crate)',
  phases: [
    { title: 'Design', detail: 'validate+refine the COMMITTED GPU_RD_CHUNK3B_DESIGN.md in place (independent re-verify of load-bearing claims vs generic.zig; preserve validated structure/decisions)' },
    { title: 'Adversary-design', detail: 'independently re-derive from source + attack the design (scope holes, cycle, RenderState-walk fidelity, the raw-ptr triad, dead-code, dropped test{})' },
    { title: 'Fold-fixes', detail: 'if the adversary found issues, a design agent folds them INTO the doc so the port has a single source of truth; skipped when SOUND' },
    { title: 'Port', detail: 'implement exactly as the hardened doc specifies; STOP if the design doc is absent' },
    { title: 'Verify', detail: 'panel: R1-comment fidelity (manual), correctness vs the RenderState walk, gates' },
  ],
}

const REPO = '/Users/gael/projects/rustty'
const REF = '/Users/gael/projects/ghostty'
const DESIGN_DOC = REPO + '/.port/phase-2/GPU_RD_CHUNK3B_DESIGN.md'

const DESIGN_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['wrote_doc', 'refined', 'claims_checked', 'corrections', 'summary'],
  properties: {
    wrote_doc: { type: 'boolean' },
    refined: { type: 'boolean' },
    claims_checked: { type: 'array', items: { type: 'string' } },
    corrections: { type: 'array', items: { type: 'string' } },
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
  'REPO=' + REPO + '. REFERENCE (read-only)=' + REF + ' (Zig 0.15.2). The Chunk-3b design doc is ' + DESIGN_DOC + '.',
  'rustty-renderer has, committed: the whole metal/* backend + Metal impl (Chunks 0/1a/1b/2); image.rs/overlay.rs (P1); and the Chunk-3a renderer SKELETON in crates/rustty-renderer/src/generic.rs (the monomorphic Renderer struct in `mod macos_impl` + SwapChain/FrameState/CustomShaderState + DerivedConfig struct with init<C>/deinit as unimplemented!() seams + inhabited placeholders `pub struct TerminalColor;`/`Path`/`RepeatablePath`). The R-a leaves are ported: cell.rs (Contents = bg_cells + fg_rows, add/bg_cell/set_cursor/resize/clear/clear(y)/reset, Key, is_covering/is_symbol/constraint_width/no_min_contrast), shaders.rs (Uniforms/CellText/CellBg/CellTextBools/Atlas), cursor.rs (Style/style()), state.rs (State/Preedit/Codepoint), link.rs, shadertoy.rs. row.zig::neverExtendBg is NOT yet ported (a 3b prereq -> new crates/rustty-renderer/src/row.rs).',
  'Reference fn line ranges in ' + REF + '/src/renderer/generic.zig (verified): updateFrame 1123; updateCustomShaderUniformsFromState 2010; rebuildOverlay 2226; PreeditRange 2294; rebuildCells 2307; rebuildRow 2610; addUnderline 3059; addOverline 3100; addStrikethrough 3131; addGlyph 3162; addCursor 3224; addPreeditCell 3313. row.zig::neverExtendBg is at src/renderer/row.zig:8.',
  'Rules: ' + REPO + '/.port/PORT_RULES.md — R1 (100% fidelity: every line/comment/test{} ported; byte-slices -> &[u8]/Vec<u8>/CString, never lossy String; memory rust-mut-args-strongly-protected: &mut Page/Row/[Cell] re-read across a call is SB UB, use raw-ptr args), R4 (ACYCLIC; renderer canNOT dep rustty-config), R5 (deferred = unimplemented!() seam + verbatim body). MONOMORPHIC port (concrete Renderer over Metal).',
  'CRITICAL GATE-COVERAGE NOTE: G3b comment-coverage SKIPS rustty-renderer (its lib.rs says "PORT STATUS: in progress"), so generic.rs R1 comment fidelity is NOT gate-enforced — the adversary/verify are the ONLY R1 backstop. Check EVERY ported line+comment against the reference by hand.',
].join('\n')

phase('Design')
const design = await agent(
  'ARCHITECT for GPU R-d Chunk 3b — the CPU CELL-FILL. A v1 design doc is ALREADY COMMITTED at ' + DESIGN_DOC + ' but was written WITHOUT an adversary review. Your job is NOT to rewrite it from scratch — it is to VALIDATE and REFINE it IN PLACE.\n' + COMMON + '\n\n' +
  'STEP 1 — read the committed doc fully. STEP 2 — independently re-verify its LOAD-BEARING claims against the reference source (Read/Grep ' + REF + '/src/renderer/generic.zig + row.zig, and the ported crates/rustty-renderer/src/{cell,shaders,cursor,state,generic}.rs + rustty-vt render API). The load-bearing claims are:\n' +
  '  (a) SCOPE: 3b = rebuildCells/rebuildRow/add{Underline,Overline,Strikethrough,Glyph,Cursor,PreeditCell}/rebuildOverlay + PreeditRange + row.zig::neverExtendBg + real TerminalColor; updateFrame + the uniforms cluster deferred to 3c. Is that boundary coherent + gate-green (compiles under -D warnings, unit-testable by filling Contents from a constructed Terminal/RenderState with NO GPU output)? Does updateFrame really depend on a 3c-only method (updateCustomShaderUniformsFromState) that justifies deferring it?\n' +
  '  (b) The raw-pointer triad (the terminal_state alias; rebuild_row cells/highlights ptr args; the shaper-cell buffer ptr) — are the SAFETY arguments correct (no write through the alias; Copy-out semantics; the shaper buffer valid-until-next-shape window; put() clones not frees)?\n' +
  '  (c) The RenderState-walk API mappings (§C table) — does every cited rustty-vt symbol (render::{RenderState,Row,Cell,Highlight,Cursor,Viewport,Colors,CellSet}, page::{Cell,Row,Wide}, style::{Style,Fg,BoldColor}, color::{RGB,Palette}, sgr::Underline) actually exist with the cited shape? Grep to confirm the risky ones (page::Row semantic_prompt accessor, page::Wide variants, Style::{bg,fg,underline_color}, render_glyph/render_codepoint signatures, get_constraint, SPRITE_INDEX).\n' +
  '  (d) TerminalColor promotion is cycle-safe (renderer-local enum over vt::color::RGB; NO rustty-config or rustty-render-types edge), and the DerivedConfig::init<C>-deferred / golden-constructs-directly decision holds against the Chunk-4 golden (which renders a real red cursor).\n' +
  '  (e) Dead-code (-D warnings): the pub-widening strategy for rebuild_cells/rebuild_overlay + reachability for the private helpers.\n' +
  '  (f) The 3b unit-test fixture (offscreen Renderer field-by-field, Metal-skip) and the DerivedConfig test fixture field ctors are named against types that actually exist.\n' +
  'STEP 3 — REFINE the doc in place with the Edit tool: fix any claim the reference disproves, tighten any under-specified section, correct any wrong line-range/ctor/variant name. PRESERVE the validated structure and decisions — do NOT restructure or re-decide unless the reference proves a decision WRONG (if so, restructure and say why in `corrections`). Keep it a design doc (touch NO crates/). You MUST end with the doc written (Edit/Write). Return the schema: wrote_doc (true iff the doc is written+present after your edits), refined (true iff you changed anything), claims_checked (the load-bearing claims you re-verified), corrections (concrete changes you made, empty if the v1 was already correct), summary.',
  { label: 'design:refine-3b', agentType: 'general-purpose', schema: DESIGN_SCHEMA })

if (!design.wrote_doc) {
  log('DESIGN GUARD TRIPPED: refine agent reports the design doc is NOT present. Aborting before port (no self-design).')
  return { aborted: 'design-doc-missing', design }
}
log('3b design validated+refined. refined=' + design.refined + ' corrections=' + (design.corrections || []).length + ' :: ' + String(design.summary).slice(0, 120))

phase('Adversary-design')
const critique = await agent(
  'ADVERSARIAL reviewer: BREAK the Chunk-3b design at ' + DESIGN_DOC + '. Re-derive INDEPENDENTLY from source — do not trust the doc.\n' + COMMON + '\n\n' +
  'Read the design + generic.zig rebuildCells/rebuildRow/add*/rebuildOverlay/PreeditRange + row.zig + the ported cell.rs/shaders.rs/cursor.rs/state.rs. CHECK, breaking each: (1) SCOPE coherence — does the 3b boundary compile + is it unit-testable with NO dangling reference to deferred (uniforms/updateFrame/drawFrame) methods? Is deferring updateFrame to 3c actually justified, or does something in 3b transitively need it? (2) DerivedConfig/TerminalColor — is making TerminalColor a renderer-local enum over vt::color::RGB genuinely cycle-safe (no render-types/config back-edge; check the acyclic gate)? With init<C> deferred, can the golden + the 3b test REALLY construct a DerivedConfig literal with fixed real colors (do all the fixture field ctors/variants exist)? (3) RenderState-walk fidelity — does EVERY terminal access map to a real ported rustty-vt API (grep to confirm; flag any invented symbol/shape)? Is the raw-pointer TRIAD (§C.3) actually sound — especially the shaper-cell buffer pointer held across add_glyph/put/next (does put clone-not-free? is the pointer nulled before the next shape?)? Any rust-mut-args-strongly-protected aliasing hazard missed? (4) any reference test{} in the ported method range dropped (grep the ranges for `test`)? (5) R1 — is any method proposed to be half-ported/summarized (illegal)? Return the schema: verdict, blocking_issues, concrete fixes (each precise + actionable), summary.',
  { label: 'adversary:3b-design', agentType: 'general-purpose', schema: CRITIQUE_SCHEMA })

log('3b adversary-design: ' + critique.verdict + ' | blocking=' + critique.blocking_issues.length + ' fixes=' + critique.fixes.length)

phase('Fold-fixes')
const needsFold = critique.verdict !== 'SOUND' && ((critique.fixes && critique.fixes.length) || (critique.blocking_issues && critique.blocking_issues.length))
let fold = null
if (needsFold) {
  fold = await agent(
    'DESIGN EDITOR: the adversary reviewed the committed Chunk-3b design (' + DESIGN_DOC + ') and returned verdict=' + critique.verdict + '. FOLD its findings INTO the doc so the porting engineer has a SINGLE authoritative source (never asks the port agent to reconcile the doc against a separate fix list).\n' + COMMON + '\n\n' +
    'ADVERSARY BLOCKING ISSUES:\n- ' + (critique.blocking_issues || []).join('\n- ') + '\nADVERSARY FIXES:\n- ' + (critique.fixes || []).join('\n- ') + '\n\n' +
    'For EACH fix/blocking issue: verify it against the reference source yourself (the adversary can be wrong — do NOT fold a fix that the source contradicts; note any you reject + why in `folded`). Then Edit ' + DESIGN_DOC + ' to incorporate the valid ones — correct the wrong claim, add the missing spec, update the OPEN RISKS. Preserve the doc structure. Touch NO crates/. You MUST end with the doc written. Return: wrote_doc, folded (per-fix: applied/rejected + why), summary.',
    { label: 'fold:adversary-fixes', agentType: 'general-purpose', schema: FOLD_SCHEMA })
  if (!fold.wrote_doc) {
    log('FOLD GUARD TRIPPED: fold agent reports the design doc is not present after folding. Aborting before port.')
    return { aborted: 'design-doc-missing-after-fold', design, critique, fold }
  }
  log('3b fold-fixes: ' + (fold.folded || []).length + ' items :: ' + String(fold.summary).slice(0, 120))
} else {
  log('3b design SOUND — no fold needed; porting directly from the doc.')
}

phase('Port')
const portReport = await agent(
  'PORTING engineer: implement Chunk 3b EXACTLY as specified in the (adversary-hardened) design doc ' + DESIGN_DOC + '. FIRST Read the doc and verify it EXISTS and is substantive — if it is absent or a stub, STOP immediately and report "design doc missing" (do NOT self-design or improvise). The doc is the single source of truth; it has already had the adversary fixes folded in.\n' + COMMON + '\n\n' +
  'Port the scoped cell-fill methods (rebuildCells/rebuildRow/addUnderline/addOverline/addStrikethrough/addGlyph/addCursor/addPreeditCell/rebuildOverlay + the private PreeditRange) into crates/rustty-renderer/src/generic.rs (inside `mod macos_impl`, as `impl Renderer` methods), filling the ported Contents from the rustty-vt RenderState. Add the new crates/rustty-renderer/src/row.rs (never_extend_bg, `pub mod row;` in lib.rs). Promote `TerminalColor` to the real renderer-local enum + `Color` per §E; DerivedConfig::init<C> stays the unimplemented!() seam. Every line+comment ported (R1 — remember G3b does NOT cover this crate, so preserve EVERY reference comment verbatim by hand, including the row.zig `// TODO: Test neverExtendBg function`); the raw-pointer triad (§C.3) with a `// SAFETY:` invariant comment at each unsafe site; byte-faithful types. Add the design-specified unit tests (rebuild_cells_fills_bg_fg_cursor + rebuild_overlay_builds_and_clears), macOS-gated + Metal-skip. updateFrame + the uniforms cluster stay OUT of 3b (3c).\n' +
  'GATES (run; ALL green, never weaken): cargo build -p rustty-renderer, cargo build --workspace (no regression), cargo fmt -p rustty-renderer -- --check, cargo clippy -p rustty-renderer --all-targets -- -D warnings, cargo test -p rustty-renderer, bash .port/tools/gates/check_acyclic.sh, python3 .port/tools/gates/check_test_counts.py. If a gate can not go green HONESTLY, STOP + report the precise blocker (do NOT weaken a gate or flail). Do NOT git commit. Report: methods ported (with generic.rs line ranges), the DerivedConfig/TerminalColor handling, row.rs, the unit tests + their result (or Metal-skip), each gate PASS/FAIL with output tail, and any reference comment/line you could not place.',
  { label: 'port:cell-fill', agentType: 'general-purpose' })

log('3b port done. Head: ' + String(portReport).slice(0, 200))

phase('Verify')
const LENSES = [
  'R1 COMMENT + LINE FIDELITY (the PRIMARY lens — G3b does NOT gate this crate): diff EVERY ported line AND comment of rebuildCells/rebuildRow/addUnderline/addOverline/addStrikethrough/addGlyph/addCursor/addPreeditCell/rebuildOverlay/PreeditRange (generic.zig) and neverExtendBg (row.zig) against the reference. Report any dropped/truncated/summarized comment (a dropped comment is a BLOCKER) and any dropped line/assertion/branch. Confirm any reference test{} in range is ported (grep the ranges for `test`).',
  'CORRECTNESS vs the RenderState walk + ALIASING: trace the ported cell fill against generic.zig — does it read RenderState/rows/cells faithfully and write the right Contents entries (bg_cells/fg_rows/cursor) with the right coordinates/colors/flags across add_glyph/add_cursor/add_preedit_cell/add_underline/add_overline/add_strikethrough? Is the raw-pointer triad (§C.3) sound (no write through the terminal_state alias; the shaper-cell buffer window; put clones)? Any rust-mut-args-strongly-protected hazard? Run the 3b unit tests (or confirm the Metal-skip is clean).',
  'GATES + acyclicity + no-regression + cycle-safety: TerminalColor-real is cycle-safe (renderer-local enum over vt::color::RGB, no config/render-types edge); DerivedConfig handling sound; init<C> still an unimplemented!() seam. RUN: cargo build --workspace, cargo fmt -p rustty-renderer -- --check, cargo clippy -p rustty-renderer --all-targets -- -D warnings, cargo test -p rustty-renderer, bash .port/tools/gates/check_acyclic.sh, python3 .port/tools/gates/check_test_counts.py — report REAL results (tails).',
]
const verdicts = await parallel(LENSES.map((lens, i) => () =>
  agent(
    'ADVERSARIAL verifier for GPU R-d Chunk 3b (cell fill, UNCOMMITTED working tree). LENS:\n' + lens + '\n' + COMMON + '\nFind the changed files (git diff/status), compare vs ' + REF + '/src/renderer/generic.zig + row.zig, and RUN the gates your lens names. Report the schema: lens, verdict PASS/CONCERNS/FAIL, findings (severity, file, issue). Only REAL evidenced issues; if clean, empty findings + PASS + name the two things you attacked hardest in `lens`.',
    { label: 'verify:' + i, phase: 'Verify', agentType: 'general-purpose', schema: VERDICT_SCHEMA })))

const allFindings = verdicts.filter(Boolean).flatMap(v => (v.findings || []).map(f => ({ ...f, lens: v.lens })))
const blockers = allFindings.filter(f => f.severity === 'blocker')
log('3b verify: ' + verdicts.filter(Boolean).map(v => v.verdict).join(', ') + ' | ' + blockers.length + ' blockers, ' + allFindings.length + ' findings')
return { design, design_critique: critique, fold, port_report: portReport, verify: verdicts, findings: allFindings, blockers }
