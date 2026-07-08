export const meta = {
  name: 'gpu-rd-chunk4-golden',
  description: 'GPU R-d Chunk 4: the offscreen GOLDEN-IMAGE gate — the R-d contract. Byte-faithfulize the packed-bool GPU structs (CellTextBools/Info/Uniforms bools -> packed u8/u32 the Metal shaders read), port drawFrame + frameCompleted (the raw *mut Renderer completion trampoline) + syncAtlasTexture + initShaders/deinitShaders, and wire a Renderer that renders a deterministic terminal state OFFSCREEN (Metal -> IOSurface), reads back pixels, and asserts the render (red block cursor + fg glyph + bg color). Design (blank-slate, guarded) -> independent adversary -> fold -> port (stop-if-no-design) -> verify panel.',
  phases: [
    { title: 'Design', detail: 'scope drawFrame+frameCompleted+syncAtlasTexture+initShaders + the packed-bool byte-faithfulization + the golden harness (offscreen render -> IOSurface readback -> pixel asserts); decide Renderer::init vs golden-constructs-directly; WRITE GPU_RD_CHUNK4_DESIGN.md (guarded)' },
    { title: 'Adversary-design', detail: 'independently re-derive + attack (packed-bool bit-order fidelity, the completion-trampoline raw-ptr aliasing, the golden determinism, dead-code, dropped test{})' },
    { title: 'Fold-fixes', detail: 'fold adversary findings INTO the doc; skipped when SOUND' },
    { title: 'Port', detail: 'implement exactly as the hardened doc specifies; STOP if the design doc is absent' },
    { title: 'Verify', detail: 'panel: R1-comment fidelity (manual), correctness (packed bytes + drawFrame + golden), gates' },
  ],
}

const REPO = '/Users/gael/projects/rustty'
const REF = '/Users/gael/projects/ghostty'
const DESIGN_DOC = REPO + '/.port/phase-2/GPU_RD_CHUNK4_DESIGN.md'

const DESIGN_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['wrote_doc', 'scope_methods', 'deferred_methods', 'packed_bool_plan', 'golden_gate_plan', 'renderer_init_decision', 'compile_boundary', 'summary'],
  properties: {
    wrote_doc: { type: 'boolean' },
    scope_methods: { type: 'array', items: { type: 'string' } },
    deferred_methods: { type: 'array', items: { type: 'string' } },
    packed_bool_plan: { type: 'string' },
    golden_gate_plan: { type: 'string' },
    renderer_init_decision: { type: 'string' },
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
  'REPO=' + REPO + '. REFERENCE (read-only)=' + REF + ' (Zig 0.15.2). The Chunk-4 design doc is ' + DESIGN_DOC + '.',
  'STATE: GPU R-d Chunks 0/1/2/3a/3b/3c are DONE + committed. The CPU side is complete: the Renderer struct (3a) + the whole metal/* backend (Chunks 0/1/2: Metal + buffer/texture/sampler/pipeline/render_pass/target(IOSurface)/frame(completion)/shaders + init_offscreen + init_shaders) + the cell-fill (3b: rebuild_cells/rebuild_row/add_* filling Contents) + updateFrame + the uniforms math (3c). What REMAINS for the R-d contract: drawFrame (the GPU encode+submit), the completion path, atlas->GPU sync, shader pipeline init, and a Renderer that actually RENDERS offscreen so pixels can be asserted.',
  'Reference line ranges (generic.zig, verified): Renderer::init :658; deinit :800; deinitShaders :832; initShaders :836; drawFrame :1430-1723; frameCompleted :1724; prepBackgroundImage :1747; uploadBackgroundImage :1828; syncAtlasTexture :3358. metal/shaders.zig: CellText.bools = packed struct(u8){no_min_contrast,is_cursor_glyph,_padding:u6} (:272); Info = packed struct(u8) (:306); PaddingExtend = packed struct(u8) (:255). The rustty side (crates/rustty-renderer/src/shaders.rs) currently models these as NAMED-BOOL structs (CellTextBools{no_min_contrast,is_cursor_glyph}, etc.) with PORT NOTEs that the packed byte/u32 layout is DEFERRED — Chunk 4 must make the backend serialize them to the exact packed byte the .metal shaders read (bit order matters).',
  'THE COMPLETION TRAMPOLINE ([FIX-2], memory shaper-hook-sb-clean-interleave + vtc-effects-trampoline-seam-d3): frameCompleted / Frame.complete stashes a RAW *mut Renderer in the Metal completion block and derefs it later (the container_of / @fieldParentPtr pattern). It MUST be a raw *mut (NOT &mut) — a &mut held across the completion callback while the renderer is mid-drawFrame is UB. The golden gate calls Frame.complete(sync=true) which invokes the block directly. If a NEW raw-alias-crossed-by-&mut pattern arises, PROVE it on the isolated non-FFI Miri harness .port/phase-2/mirihar-chunk3b/ (extend it), do NOT hand-reason Stacked Borrows.',
  'Rules: ' + REPO + '/.port/PORT_RULES.md — R1 (100% fidelity: every line/comment/test{} ported; byte-slices -> &[u8]/Vec<u8>/CString; memory rust-mut-args-strongly-protected), R4 (ACYCLIC; renderer canNOT dep rustty-config), R5 (deferred = unimplemented!() seam + verbatim body). MONOMORPHIC port (concrete Renderer over Metal). Byte-faithfulness (R1) is CRITICAL here: the packed GPU structs must match the .metal shader ABI bit-for-bit.',
  'CRITICAL GATE-COVERAGE NOTE: G3b comment-coverage SKIPS rustty-renderer, so generic.rs/shaders.rs R1 comment fidelity is NOT gate-enforced — the adversary/verify are the ONLY R1 backstop. The golden test is the FIRST real GPU-output assertion (the whole R-d contract), macOS/Metal-gated with a clean Metal-skip.',
].join('\n')

phase('Design')
const design = await agent(
  'ARCHITECT for GPU R-d Chunk 4 — the OFFSCREEN GOLDEN-IMAGE GATE (the R-d contract). You MUST write ' + DESIGN_DOC + ' with the Write tool (guarded — wrote_doc=true only if you actually wrote it). Design ONLY; touch NO crates/. Read the reference (generic.zig drawFrame/frameCompleted/initShaders/syncAtlasTexture/Renderer.init + metal/shaders.zig packed structs + metal/Frame.zig/RenderPass.zig/Target.zig) + the ported crates/rustty-renderer/src/{generic,shaders,metal/*}.rs (esp. metal/metal_impl.rs init_offscreen/init_shaders + frame/target/render_pass) + the .metal shader source (build.rs / *.metal) to read the exact GPU struct ABI.\n' + COMMON + '\n\n' +
  'DECIDE + specify (concrete Rust signature blocks + a scope table + OPEN RISKS):\n' +
  '(1) SCOPE: which methods land in 4 (recommended: drawFrame + frameCompleted + syncAtlasTexture + initShaders/deinitShaders + whatever Renderer construction the golden needs) vs deferred (prepBackgroundImage/uploadBackgroundImage if the golden uses no bg image; the surface/display-link/apprt methods). Give a coherent gate-green boundary (compiles under -D warnings + the golden test renders + asserts, Metal-skip clean).\n' +
  '(2) PACKED-BOOL BYTE-FAITHFULNESS: specify EXACTLY how the named-bool structs (CellTextBools, Info, the Uniforms u32 bools/PaddingExtend) serialize to the packed byte/u32 the .metal shaders read — bit order, offsets, _padding. Confirm against the .metal source (or the Zig packed struct(u8)/(u32) definitions). This is what makes the cursor render right; a wrong bit = wrong pixels. Decide: a serialize-on-buffer-sync approach, or make the Rust structs themselves #[repr(C)] packed with manual bit-packing. Justify byte-for-byte.\n' +
  '(3) THE GOLDEN GATE: how to render deterministically + assert. The golden must render a known terminal state (e.g. "Ab" + a red block cursor, as the 3b test set up) offscreen via drawFrame -> Metal -> IOSurface, then read back the pixels. DECIDE the assertion strategy: exact-golden-PNG (fragile across font/GPU) vs STRUCTURAL pixel asserts (the cursor cell region is the config red; the bg is the config bg; a fg glyph is present in row 0) — recommend the robust structural approach + justify. Specify the IOSurface/texture pixel readback (the ported Target/Metal API) + the exact asserts.\n' +
  '(4) Renderer::init vs golden-constructs-directly: does Renderer::init<A>(alloc, options) land now (it needs a live surface + apprt), or does the golden construct the Renderer field-by-field like the 3b/3c test_renderer (offscreen Metal + embedded SharedGrid + Shaper + the DerivedConfig fixture)? Recommend + justify against acyclicity + the surface dependency.\n' +
  '(5) THE COMPLETION TRAMPOLINE: map frameCompleted / Frame.complete(sync) + the raw *mut Renderer stash; specify the raw-ptr discipline (NOT &mut — [FIX-2]). Flag if a mirihar candidate is needed.\n' +
  '(6) DEAD-CODE + the golden TEST: how drawFrame etc. stay reachable; the exact macOS/Metal-gated golden test (construct Renderer, drive a Terminal, update_frame, draw_frame, read pixels, assert; Metal-skip).\n' +
  'Return the schema: wrote_doc, scope_methods, deferred_methods, packed_bool_plan, golden_gate_plan, renderer_init_decision, compile_boundary, summary.',
  { label: 'design:chunk4', agentType: 'general-purpose', schema: DESIGN_SCHEMA })

if (!design.wrote_doc) {
  log('DESIGN GUARD TRIPPED: design agent did NOT write GPU_RD_CHUNK4_DESIGN.md. Aborting before port.')
  return { aborted: 'design-doc-not-written', design }
}
log('4 design written. scope=' + (design.scope_methods || []).join(',').slice(0, 160))

phase('Adversary-design')
const critique = await agent(
  'ADVERSARIAL reviewer: BREAK the Chunk-4 design at ' + DESIGN_DOC + '. Re-derive INDEPENDENTLY from source.\n' + COMMON + '\n\n' +
  'Read the design + generic.zig drawFrame/frameCompleted/initShaders/syncAtlasTexture + metal/shaders.zig packed structs + the .metal shader source + the ported metal/*.rs. CHECK, breaking each: (1) PACKED-BOOL BIT FIDELITY — does the proposed serialization produce the EXACT byte/u32 the .metal shaders read (bit order, offsets, _padding, endianness)? Grep the .metal source + the Zig packed struct(u8)/(u32) and verify field-by-field; a single wrong bit = wrong cursor/pixels. (2) drawFrame FIDELITY — is every encode/step/attachment/present ported; does the scope compile + is drawFrame reachable? (3) THE GOLDEN GATE — is the render deterministic + the assertion strategy sound (does structural-assert actually prove the cursor/bg/fg, or is it vacuous)? Can the pixel readback actually work offscreen (IOSurface/Target API present)? (4) THE COMPLETION TRAMPOLINE — is the raw *mut Renderer discipline sound (NOT &mut); any aliasing hazard needing a mirihar candidate? (5) Renderer::init vs construct-directly — acyclic + surface-dep sound? (6) any reference test{} dropped? (7) R1 — any method half-ported/summarized? Return the schema: verdict, blocking_issues, concrete fixes, summary.',
  { label: 'adversary:4-design', agentType: 'general-purpose', schema: CRITIQUE_SCHEMA })

log('4 adversary-design: ' + critique.verdict + ' | blocking=' + critique.blocking_issues.length + ' fixes=' + critique.fixes.length)

phase('Fold-fixes')
const needsFold = critique.verdict !== 'SOUND' && ((critique.fixes && critique.fixes.length) || (critique.blocking_issues && critique.blocking_issues.length))
let fold = null
if (needsFold) {
  fold = await agent(
    'DESIGN EDITOR: the adversary reviewed the Chunk-4 design (' + DESIGN_DOC + ') and returned verdict=' + critique.verdict + '. FOLD its findings INTO the doc (single authoritative source).\n' + COMMON + '\n\n' +
    'ADVERSARY BLOCKING ISSUES:\n- ' + (critique.blocking_issues || []).join('\n- ') + '\nADVERSARY FIXES:\n- ' + (critique.fixes || []).join('\n- ') + '\n\n' +
    'For EACH: verify against the reference/.metal source yourself (the adversary can be wrong — do NOT fold a fix the source contradicts; note rejected + why in `folded`). Edit ' + DESIGN_DOC + ' to incorporate the valid ones. Preserve the doc structure. Touch NO crates/. You MUST end with the doc written. Return: wrote_doc, folded, summary.',
    { label: 'fold:adversary-fixes', agentType: 'general-purpose', schema: FOLD_SCHEMA })
  if (!fold.wrote_doc) {
    log('FOLD GUARD TRIPPED: doc not present after folding. Aborting before port.')
    return { aborted: 'design-doc-missing-after-fold', design, critique, fold }
  }
  log('4 fold-fixes: ' + (fold.folded || []).length + ' items :: ' + String(fold.summary).slice(0, 120))
} else {
  log('4 design SOUND — no fold needed; porting directly from the doc.')
}

phase('Port')
const portReport = await agent(
  'PORTING engineer: implement Chunk 4 EXACTLY as specified in the (adversary-hardened) design doc ' + DESIGN_DOC + '. FIRST Read the doc and verify it EXISTS + is substantive — if absent or a stub, STOP immediately and report "design doc missing" (do NOT self-design or improvise).\n' + COMMON + '\n\n' +
  'Port the scoped drawFrame + frameCompleted + syncAtlasTexture + initShaders/deinitShaders + the packed-bool byte-faithful serialization + the golden Renderer/harness per the design. Every line+comment ported (R1 — G3b does NOT cover this crate, preserve EVERY reference comment verbatim by hand); the packed GPU structs must serialize to the EXACT shader ABI byte (byte-faithfulness is the whole point); the completion trampoline uses a raw *mut Renderer (NOT &mut — [FIX-2]; if a new raw-alias pattern arises, PROVE it on .port/phase-2/mirihar-chunk3b/). Add the design-specified GOLDEN test (render offscreen, read IOSurface pixels, assert cursor/bg/fg; macOS/Metal-gated + Metal-skip). Deferred methods stay unimplemented!() seams with verbatim bodies.\n' +
  'GATES (run; ALL green, never weaken): cargo build -p rustty-renderer, cargo build --workspace, cargo fmt -p rustty-renderer -- --check, cargo clippy -p rustty-renderer --all-targets -- -D warnings, cargo test -p rustty-renderer, bash .port/tools/gates/check_acyclic.sh, python3 .port/tools/gates/check_test_counts.py. If a gate can not go green HONESTLY, STOP + report the precise blocker (do NOT weaken/flail — a golden mismatch is a REAL failure to diagnose, not to paper over). Do NOT git commit. Report: methods ported (line ranges), the packed-bool serialization + how you verified the bytes, the golden test + its result (the actual asserted pixels), each gate PASS/FAIL with output tail, any reference comment/line you could not place.',
  { label: 'port:drawFrame-golden', agentType: 'general-purpose' })

log('4 port done. Head: ' + String(portReport).slice(0, 200))

phase('Verify')
const LENSES = [
  'R1 COMMENT + LINE FIDELITY (the PRIMARY lens — G3b does NOT gate this crate): diff EVERY ported line AND comment of drawFrame + frameCompleted + syncAtlasTexture + initShaders/deinitShaders (+ the packed-bool struct changes in shaders.rs) against the reference generic.zig / metal/shaders.zig. Report any dropped/truncated/summarized comment (BLOCKER) and any dropped line/branch/assertion. Confirm any reference test{} in range is ported.',
  'CORRECTNESS: (a) PACKED-BOOL BYTES — independently verify the serialized CellTextBools/Info/Uniforms-bools byte matches the .metal shader ABI bit-for-bit (read the .metal source + the Zig packed struct, compute the expected byte, check the Rust serialization); a wrong bit is a BLOCKER. (b) drawFrame encode/step/present fidelity vs the reference. (c) THE GOLDEN — run the golden test; confirm it renders + the pixel asserts are non-vacuous and PASS (the cursor cell is the config red, bg is config bg, a fg glyph is present). (d) the completion trampoline raw-ptr discipline (raw *mut, not &mut); if a new raw-alias pattern, re-run/extend the mirihar harness under SB+TB.',
  'GATES + acyclicity + no-regression: RUN cargo build --workspace, cargo fmt -p rustty-renderer -- --check, cargo clippy -p rustty-renderer --all-targets -- -D warnings, cargo test -p rustty-renderer (the golden test must pass or cleanly Metal-skip), bash .port/tools/gates/check_acyclic.sh, python3 .port/tools/gates/check_test_counts.py — report REAL results (tails).',
]
const verdicts = await parallel(LENSES.map((lens, i) => () =>
  agent(
    'ADVERSARIAL verifier for GPU R-d Chunk 4 (drawFrame + golden gate, UNCOMMITTED working tree). LENS:\n' + lens + '\n' + COMMON + '\nFind the changed files (git diff/status), compare vs the reference, and RUN the gates/tests your lens names. Report the schema: lens, verdict PASS/CONCERNS/FAIL, findings (severity, file, issue). Only REAL evidenced issues; if clean, empty findings + PASS + name the two things you attacked hardest in `lens`.',
    { label: 'verify:' + i, phase: 'Verify', agentType: 'general-purpose', schema: VERDICT_SCHEMA })))

const allFindings = verdicts.filter(Boolean).flatMap(v => (v.findings || []).map(f => ({ ...f, lens: v.lens })))
const blockers = allFindings.filter(f => f.severity === 'blocker')
log('4 verify: ' + verdicts.filter(Boolean).map(v => v.verdict).join(', ') + ' | ' + blockers.length + ' blockers, ' + allFindings.length + ' findings')
return { design, design_critique: critique, fold, port_report: portReport, verify: verdicts, findings: allFindings, blockers }
