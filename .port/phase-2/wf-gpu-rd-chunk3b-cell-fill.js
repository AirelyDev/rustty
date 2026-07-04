export const meta = {
  name: 'gpu-rd-chunk3b-cell-fill',
  description: 'GPU R-d Chunk 3b: port generic.zig CPU cell-fill (updateFrame + rebuildCells/rebuildRow/add*) + make the golden-relevant DerivedConfig placeholders real, via design (guarded) -> adversary -> port (stop-if-no-design) -> verify (incl. an R1-comment lens, since G3b skips the in-progress renderer crate)',
  phases: [
    { title: 'Design', detail: 'scope the coherent cell-fill sub-unit + resolve DerivedConfig::init<C>/TerminalColor-real; WRITE GPU_RD_CHUNK3B_DESIGN.md (guarded)' },
    { title: 'Adversary-design', detail: 'attack the design (scope holes, cycle, RenderState-walk fidelity, dead-code)' },
    { title: 'Port', detail: 'implement exactly as designed; STOP if the design doc is absent' },
    { title: 'Verify', detail: 'panel: R1-comment fidelity (manual), correctness vs the RenderState walk, gates' },
  ],
}

const REPO = '/Users/gael/projects/rustty'
const REF = '/Users/gael/projects/ghostty'

const DESIGN_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['wrote_doc', 'scope_methods', 'derivedconfig_approach', 'compile_boundary', 'summary'],
  properties: {
    wrote_doc: { type: 'boolean' },
    scope_methods: { type: 'array', items: { type: 'string' } },
    derivedconfig_approach: { type: 'string' },
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
  'rustty-renderer has, committed: the whole metal/* backend + Metal impl (Chunks 0/1a/1b/2); image.rs/overlay.rs (P1); and the Chunk-3a renderer SKELETON in crates/rustty-renderer/src/generic.rs (the monomorphic Renderer struct + SwapChain/FrameState/CustomShaderState + DerivedConfig struct with init<C>/deinit as unimplemented!() seams + inhabited placeholders TerminalColor/Path/RepeatablePath). The R-a leaves are ported: cell.rs (Contents = bg_cells + fg_rows, add/bg_cell/set_cursor/resize/clear), shaders.rs (Uniforms/CellText/CellBg), cursor.rs (Style/style()), state.rs (State/Preedit), link.rs, shadertoy.rs.',
  'Rules: ' + REPO + '/.port/PORT_RULES.md — R1 (100% fidelity: every line/comment/test{} ported; byte-slices -> &[u8]/Vec<u8>/CString, never lossy String; memory rust-mut-args-strongly-protected: &mut Page/Row/[Cell] re-read across assert_integrity is SB UB, use raw-ptr args), R4 (ACYCLIC; renderer canNOT dep rustty-config), R5 (deferred = unimplemented!() seam + verbatim body). MONOMORPHIC port (concrete Renderer over Metal).',
  'CRITICAL GATE-COVERAGE NOTE: G3b comment-coverage SKIPS rustty-renderer (its lib.rs says "PORT STATUS: in progress"), so generic.rs R1 comment fidelity is NOT gate-enforced — the adversary/verify are the ONLY R1 backstop. Check EVERY ported line+comment against the reference by hand.',
].join('\n')

phase('Design')
const design = await agent(
  'ARCHITECT for GPU R-d Chunk 3b — the CPU CELL-FILL. You MUST write ' + REPO + '/.port/phase-2/GPU_RD_CHUNK3B_DESIGN.md with the Write tool (this is guarded — return wrote_doc=true only if you actually wrote it). Design ONLY; do not touch crates/.\n' + COMMON + '\n\n' +
  'Scope: port generic.zig CPU-fill methods that fill the already-ported `Contents` (cell.rs) from the terminal RenderState — updateFrame (the critical-section extract + terminal.RenderState walk), rebuildCells -> rebuildRow -> addGlyph/addCursor/addPreeditCell/addUnderline/addOverline/addStrikethrough, rebuildOverlay. READ these in ' + REF + '/src/renderer/generic.zig (grep the fn names for line ranges). DECIDE + specify:\n' +
  '(1) SCOPE BOUNDARY: which methods land in 3b (the cell fill) vs deferred to 3c (uniforms math: updateScreenSizeUniforms/updateFontGridUniforms/updateCustomShaderUniforms*) vs Chunk 4 (drawFrame/GPU). Give a coherent gate-green boundary (compiles + unit-testable by filling Contents from a constructed Terminal/RenderState, NO GPU output).\n' +
  '(2) DerivedConfig / TerminalColor: the cell fill reads cursor/selection/search colors (DerivedConfig TerminalColor fields, currently inhabited placeholders). DECIDE: make TerminalColor a REAL renderer-local enum now (mirroring config Color/CellForeground/CellBackground, Color holding rustty_vt::color::RGB which the renderer CAN name) so the fill + the Chunk-4 golden can construct/read real colors; and whether DerivedConfig::init<C> (the config extraction, generic over C per thread.rs precedent) lands in 3b OR stays a seam with the golden constructing DerivedConfig directly with fixed values. Justify against the golden gate (Chunk 4 renders a cursor -> needs real cursor_color). Path/RepeatablePath stay placeholders (golden uses no bg_image/custom_shaders).\n' +
  '(3) The RenderState walk: how generic.zig reads the terminal (rustty-vt RenderState/Screen/PageList) — map each access to the ported rustty-vt API; flag any raw-ptr/aliasing (rust-mut-args-strongly-protected) hazard.\n' +
  '(4) Dead-code (-D warnings): the new fill methods must be reachable (they will be, from a unit test + eventually drawFrame) — specify the 3b unit test that exercises them (fill Contents from a small constructed terminal, assert cell/bg/cursor entries).\n' +
  'Deliver concrete rust signature blocks + a method scope table + OPEN RISKS. Return the schema: wrote_doc (true iff written), scope_methods (the exact methods 3b ports), derivedconfig_approach, compile_boundary, summary.',
  { label: 'design:chunk3b', agentType: 'general-purpose', schema: DESIGN_SCHEMA })

if (!design.wrote_doc) {
  log('DESIGN GUARD TRIPPED: design agent did NOT write GPU_RD_CHUNK3B_DESIGN.md. Aborting before port (no self-design). Re-run with a fixed design phase.')
  return { aborted: 'design-doc-not-written', design }
}
log('3b design written. scope=' + (design.scope_methods || []).join(',').slice(0, 140))

phase('Adversary-design')
const critique = await agent(
  'ADVERSARIAL reviewer: BREAK the Chunk-3b design at ' + REPO + '/.port/phase-2/GPU_RD_CHUNK3B_DESIGN.md. Re-derive from source.\n' + COMMON + '\n\n' +
  'Read the design + generic.zig updateFrame/rebuildCells/rebuildRow/add* + the ported cell.rs/shaders.rs/cursor.rs. CHECK: (1) scope coherence — does the 3b boundary compile + is it unit-testable with NO dangling reference to deferred (uniforms/drawFrame) methods? (2) DerivedConfig/TerminalColor — is making TerminalColor real cycle-safe (renderer-local enum over vt::color::RGB, no render-types/config edge)? If DerivedConfig::init<C> lands, is the generic-C config-read sound + acyclic; if deferred, can the golden actually construct DerivedConfig with fixed real colors? (3) RenderState-walk fidelity — does every terminal access map to a real ported rustty-vt API; any rust-mut-args-strongly-protected aliasing hazard in the row/cell walk? (4) any reference test{} in the ported method range dropped? (5) R1 — is any method proposed to be half-ported (illegal)? Return the schema: verdict, blocking_issues, concrete fixes, summary.',
  { label: 'adversary:3b-design', agentType: 'general-purpose', schema: CRITIQUE_SCHEMA })

log('3b adversary-design: ' + critique.verdict + ' | blocking=' + critique.blocking_issues.length)
const fixesBlock = (critique.fixes && critique.fixes.length)
  ? '\n\nADVERSARY-DESIGN (' + critique.verdict + ') — HONOR these fixes:\n- ' + critique.fixes.join('\n- ') + '\nAddress blocking issues:\n- ' + (critique.blocking_issues || []).join('\n- ')
  : ''

phase('Port')
const portReport = await agent(
  'PORTING engineer: implement Chunk 3b exactly as designed in ' + REPO + '/.port/phase-2/GPU_RD_CHUNK3B_DESIGN.md. FIRST verify that design doc EXISTS and is substantive — if it is absent or a stub, STOP immediately and report "design doc missing" (do NOT self-design or improvise).' + fixesBlock + '\n' + COMMON + '\n\n' +
  'Port the scoped cell-fill methods (updateFrame/rebuildCells/rebuildRow/add*/rebuildOverlay per the design) into crates/rustty-renderer/src/generic.rs, filling the ported Contents from the rustty-vt RenderState. Make TerminalColor real + handle DerivedConfig per the design. Every line+comment ported (R1 — remember G3b does NOT cover this crate, so preserve EVERY reference comment verbatim by hand); raw-ptr args per rust-mut-args-strongly-protected; byte-faithful types. Add the design-specified unit test. Deferred methods stay unimplemented!() seams with verbatim bodies.\n' +
  'GATES (run; ALL green, never weaken): cargo build -p rustty-renderer, cargo build --workspace (no regression), cargo fmt -p rustty-renderer -- --check, cargo clippy -p rustty-renderer --all-targets -- -D warnings, cargo test -p rustty-renderer, bash .port/tools/gates/check_acyclic.sh, python3 .port/tools/gates/check_test_counts.py. STOP+report a precise blocker if any can not go green honestly (do NOT weaken/flail). Do NOT git commit. Report: methods ported, the DerivedConfig/TerminalColor handling, the unit test + result, each gate PASS/FAIL with tail, any comment you could not place.',
  { label: 'port:cell-fill', agentType: 'general-purpose' })

log('3b port. Head: ' + String(portReport).slice(0, 160))

phase('Verify')
const LENSES = [
  'R1 COMMENT + LINE FIDELITY (the primary lens — G3b does NOT gate this crate): diff EVERY ported generic.zig updateFrame/rebuildCells/rebuildRow/add*/rebuildOverlay line AND comment against the reference. Report any dropped/truncated/summarized comment (a dropped comment is a blocker) and any dropped line/assertion. Also confirm any reference test{} in range is ported.',
  'CORRECTNESS vs the RenderState walk: trace the ported cell fill against generic.zig — does it read the terminal RenderState/Screen/rows/cells faithfully, write the right Contents entries (bg_cells/fg_rows/cursor), handle the add* helpers (glyph/cursor/preedit/underline/overline/strikethrough) with the right coordinates/colors/flags? Any rust-mut-args-strongly-protected aliasing hazard (raw-ptr vs &mut)? Run the 3b unit test.',
  'GATES + acyclicity + no-regression + DerivedConfig/TerminalColor: TerminalColor-real is cycle-safe (renderer-local, no config/render-types edge); DerivedConfig handling sound. RUN: cargo build --workspace, cargo clippy -p rustty-renderer --all-targets -- -D warnings, cargo test -p rustty-renderer, bash .port/tools/gates/check_acyclic.sh, python3 .port/tools/gates/check_test_counts.py — report real results.',
]
const verdicts = await parallel(LENSES.map((lens, i) => () =>
  agent(
    'ADVERSARIAL verifier for GPU R-d Chunk 3b (cell fill, uncommitted). LENS:\n' + lens + '\n' + COMMON + '\nFind the changed files (git diff), compare vs ' + REF + '/src/renderer/generic.zig, RUN the gates your lens names. Report the schema: lens, verdict PASS/CONCERNS/FAIL, findings (severity, file, issue). Only REAL evidenced issues; if clean, empty findings + PASS + the two things you attacked hardest.',
    { label: 'verify:' + i, phase: 'Verify', agentType: 'general-purpose', schema: VERDICT_SCHEMA })))

const allFindings = verdicts.filter(Boolean).flatMap(v => (v.findings || []).map(f => ({ ...f, lens: v.lens })))
const blockers = allFindings.filter(f => f.severity === 'blocker')
log('3b verify: ' + verdicts.filter(Boolean).map(v => v.verdict).join(', ') + ' | ' + blockers.length + ' blockers, ' + allFindings.length + ' findings')
return { design, design_critique: critique, port_report: portReport, verify: verdicts, blockers }
