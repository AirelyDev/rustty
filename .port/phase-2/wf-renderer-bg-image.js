export const meta = {
  name: 'renderer-bg-image',
  description: 'Fill the deferred rustty-renderer background-image pipeline: un-defer the config bg_image Path (a renderer-local path type + the RendererConfig::bg_image accessor + the rustty-config impl), prep_background_image (open + wuffs-sys PNG/JPEG decode), upload_background_image (GPU texture), and the bg_image render (draw_frame). Reuses the already-landed Image::convert + Texture upload. Design (guarded, verifies wuffs-sys decode API + STOPS if blocked) -> adversary -> fold -> port (stop-if-no-design) -> verify.',
  phases: [
    { title: 'Design', detail: 'verify the wuffs-sys PNG/JPEG decode API is usable (rustty-font already deps it) + resolve the renderer-local bg_image Path type (acyclic, via the RendererConfig trait); scope prep/upload/render; design the offscreen test; WRITE the design doc (guarded). STOP-report if decode/Path is genuinely blocked.' },
    { title: 'Adversary-design', detail: 'attack: is the wuffs-sys decode real + the Path acyclic? the decode->convert->upload fidelity, the bg_image render, any aliasing, dead-code, dropped test{}' },
    { title: 'Fold-fixes', detail: 'fold adversary findings INTO the doc; skipped when SOUND' },
    { title: 'Port', detail: 'implement exactly as the hardened doc specifies; STOP if the design doc is absent' },
    { title: 'Verify', detail: 'panel: R1-comment fidelity (manual), correctness (decode + upload + render + acyclicity), gates + the offscreen bg-image test' },
  ],
}

const REPO = '/Users/gael/projects/rustty'
const REF = '/Users/gael/projects/ghostty'
const DESIGN_DOC = REPO + '/.port/phase-2/RENDERER_BG_IMAGE_DESIGN.md'

const DESIGN_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['wrote_doc', 'decode_and_path_feasible', 'scope_methods', 'acyclic_note', 'test_plan', 'compile_boundary', 'summary'],
  properties: {
    wrote_doc: { type: 'boolean' },
    decode_and_path_feasible: { type: 'string' },
    scope_methods: { type: 'array', items: { type: 'string' } },
    acyclic_note: { type: 'string' },
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
  'REPO=' + REPO + '. REFERENCE (read-only)=' + REF + ' (Zig 0.15.2). The design doc is ' + DESIGN_DOC + '.',
  'STATE: GPU R-d + the renderer config/resize surface + the Kitty image path are DONE + committed. This milestone fills the DEFERRED background-image pipeline. The seams (crates/rustty-renderer/src/generic.rs): prep_background_image (ref generic.zig:1747 — open the config bg_image Path, wuffs-decode PNG/JPEG), upload_background_image (ref :1828 — GPU texture). The config bg_image field is currently a DEFERRED renderer-local `Path` placeholder (inhabited, no real path). The already-landed Image::convert (kitty milestone) + Texture upload are REUSABLE.',
  'DECODE (verify FIRST): the reference uses `wuffs.png.decode(alloc, contents)` / `wuffs.jpeg.decode(...)` + `FileType.detect`/`guessFromExtension`. `crates-sys/wuffs-sys` is a workspace FFI crate ALREADY used by rustty-font (crates/rustty-font/Cargo.toml). CONFIRM wuffs-sys exposes (or can expose) a usable PNG/JPEG decode entry (the opaque decoder struct + sizeof + decode calls per its lib.rs) BEFORE designing — if the decode API is a stub/absent, STOP and report decode_and_path_feasible=blocked with the missing symbol + a fallback (a different next milestone).',
  'THE bg_image Path (acyclic, R4/G7): the renderer canNOT name a rustty_config type. The RendererConfig trait (crate::config, landed in the config/resize milestone) already provides `bg_image()`. Currently it/the extraction yields the placeholder. Resolve the renderer-local `Path` type (mirroring the reference config `Path = union(enum) { required, optional: [:0]const u8 }` -> a renderer-local enum over `CString`/`Vec<u8>`, renderer-nameable) + make RendererConfig::bg_image return it + rustty-config impls it (acyclic — config already deps renderer). Un-defer the DerivedConfig.bg_image field from the placeholder to the real renderer-local Path. Keep it BYTE-FAITHFUL (path bytes -> CString/&[u8], never lossy String).',
  'RENDER: check whether the bg_image render pass in draw_frame is already wired (Chunk 4) or a seam; if wired but untested (bg_image==None), the milestone activates it; if a seam, port it. The bg-image is drawn as a full-screen textured quad before the cells (the bg_image pipeline). Map to the ported metal pipeline/step machinery + the BgImage vertex + the gpu_abi BgImageGpu (byte-faithful, landed in Chunk 4).',
  'Rules: ' + REPO + '/.port/PORT_RULES.md — R1 (100% fidelity: every line/comment/test{}; byte-slices -> &[u8]/Vec<u8>/CString, NEVER lossy String — paths especially), R4 (ACYCLIC — renderer gains NO config edge; run check_acyclic.sh), R5 (anything still deferred = unimplemented!() seam + verbatim body). G3b comment-coverage SKIPS rustty-renderer, so the adversary/verify are the ONLY R1 backstop.',
  'TESTABILITY: write a small PNG/JPEG to a temp file (or embed bytes), point the config bg_image Path at it via the MockConfig (renderer canNOT dep rustty-config), init/change_config to trigger prep_background_image (decode) + upload_background_image (GPU), draw_frame, and assert the bg image rendered (the bg-image texture created + the bg pixels in the IOSurface readback, distinct from the bg_color). macOS/Metal-gated + Metal-skip.',
].join('\n')

phase('Design')
const design = await agent(
  'ARCHITECT: fill the deferred rustty-renderer background-image pipeline. You MUST write ' + DESIGN_DOC + ' with the Write tool (guarded — wrote_doc=true only if written). Design ONLY; touch NO crates/. FIRST verify wuffs-sys PNG/JPEG decode is usable + resolve the acyclic bg_image Path (see COMMON) — if the decode API is a stub/absent or the Path cannot be made acyclic, still WRITE the doc recording decode_and_path_feasible=blocked + the missing piece + a fallback next-milestone, and return.\n' + COMMON + '\n\n' +
  'If feasible, DECIDE + specify (concrete Rust signature blocks + the decode/convert/upload/render map + OPEN RISKS): (1) the renderer-local Path type + RendererConfig::bg_image + the rustty-config impl + un-deferring DerivedConfig.bg_image; (2) prep_background_image (FileType detect + wuffs-sys decode) faithful to the reference; (3) upload_background_image + the bg_image render (reusing Image::convert + Texture + the BgImage/gpu_abi vertex); (4) the offscreen test (a real small PNG/JPEG); (5) dead-code + no-regression (the golden + existing 45 tests still pass). Return the schema: wrote_doc, decode_and_path_feasible, scope_methods, acyclic_note, test_plan, compile_boundary, summary.',
  { label: 'design:bgimage', agentType: 'general-purpose', schema: DESIGN_SCHEMA })

if (!design.wrote_doc) {
  log('DESIGN GUARD TRIPPED: design agent did NOT write the design doc. Aborting before port.')
  return { aborted: 'design-doc-not-written', design }
}
if (String(design.decode_and_path_feasible).toLowerCase().includes('blocked')) {
  log('BG-IMAGE BLOCKED: ' + design.decode_and_path_feasible + ' — aborting this milestone (pick a different next surface). No port attempted.')
  return { aborted: 'bg-image-blocked', design }
}
log('bg-image design written. scope=' + (design.scope_methods || []).join(',').slice(0, 140))

phase('Adversary-design')
const critique = await agent(
  'ADVERSARIAL reviewer: BREAK the background-image design at ' + DESIGN_DOC + '. Re-derive INDEPENDENTLY from source.\n' + COMMON + '\n\n' +
  'Read the design + generic.zig prepBackgroundImage/uploadBackgroundImage + the wuffs-sys lib.rs + the RendererConfig trait + rustty-config + the metal bg_image pipeline. CHECK, breaking each: (1) DECODE — does wuffs-sys REALLY expose a usable PNG/JPEG decode (grep its lib.rs/build.rs — is WUFFS_IMPLEMENTATION built? are the decoder symbols linked)? Any invented API? (2) ACYCLICITY — does the renderer-local Path + the RendererConfig::bg_image accessor leak NO rustty_config type; does un-deferring add NO config edge (check_acyclic)? Byte-faithful path (CString/&[u8], not String)? (3) FIDELITY — are prep/upload/render faithful to the verbatim reference (FileType.detect, the wuffs decode, the texture upload, the full-screen quad)? (4) any aliasing hazard? (5) TESTABILITY — can the test really decode a real image via wuffs-sys without a rustty-config dep; is it non-vacuous (bg pixels distinct from bg_color)? (6) any reference test{} dropped? Return the schema: verdict, blocking_issues, concrete fixes, summary.',
  { label: 'adversary:bgimage-design', agentType: 'general-purpose', schema: CRITIQUE_SCHEMA })

log('bg-image adversary: ' + critique.verdict + ' | blocking=' + critique.blocking_issues.length + ' fixes=' + critique.fixes.length)

phase('Fold-fixes')
const needsFold = critique.verdict !== 'SOUND' && ((critique.fixes && critique.fixes.length) || (critique.blocking_issues && critique.blocking_issues.length))
let fold = null
if (needsFold) {
  fold = await agent(
    'DESIGN EDITOR: the adversary reviewed the background-image design (' + DESIGN_DOC + ') and returned verdict=' + critique.verdict + '. FOLD its findings INTO the doc (single authoritative source).\n' + COMMON + '\n\n' +
    'ADVERSARY BLOCKING ISSUES:\n- ' + (critique.blocking_issues || []).join('\n- ') + '\nADVERSARY FIXES:\n- ' + (critique.fixes || []).join('\n- ') + '\n\n' +
    'For EACH: verify against the reference/wuffs-sys/rustty-config source yourself (reject any the source contradicts; note rejected + why in `folded`). Edit ' + DESIGN_DOC + ' to incorporate the valid ones. Preserve structure. Touch NO crates/. You MUST end with the doc written. Return: wrote_doc, folded, summary.',
    { label: 'fold:adversary-fixes', agentType: 'general-purpose', schema: FOLD_SCHEMA })
  if (!fold.wrote_doc) {
    log('FOLD GUARD TRIPPED: doc not present after folding. Aborting before port.')
    return { aborted: 'design-doc-missing-after-fold', design, critique, fold }
  }
  log('bg-image fold-fixes: ' + (fold.folded || []).length + ' items :: ' + String(fold.summary).slice(0, 120))
} else {
  log('bg-image design SOUND — no fold needed; porting directly from the doc.')
}

phase('Port')
const portReport = await agent(
  'PORTING engineer: implement the background-image pipeline EXACTLY as specified in the (adversary-hardened) design doc ' + DESIGN_DOC + '. FIRST Read the doc + verify it EXISTS + is substantive — if absent or a stub, STOP + report "design doc missing".\n' + COMMON + '\n\n' +
  'Implement the renderer-local Path + RendererConfig::bg_image (+ the rustty-config impl) + prep_background_image (wuffs-sys decode) + upload_background_image + the bg_image render, per the design. Every line+comment ported (R1 — preserve EVERY reference comment verbatim; paths byte-faithful CString/&[u8], NEVER lossy String); byte-faithful GPU structs; raw-ptr discipline for any aliasing. Add the design-specified offscreen bg-image test. Anything still deferred stays an unimplemented!() seam with verbatim body.\n' +
  'GATES (run; ALL green, never weaken): cargo build --workspace, cargo fmt --check, cargo clippy --workspace --all-targets -- -D warnings, cargo test -p rustty-renderer (+ rustty-config if touched), bash .port/tools/gates/check_acyclic.sh (CRITICAL — no config edge), python3 .port/tools/gates/check_test_counts.py, python3 .port/tools/gates/check_comment_coverage.py. If a gate can not go green HONESTLY, STOP + report the precise blocker. Do NOT git commit. Report: methods ported (line ranges), the wuffs-sys decode wiring, the Path acyclicity, the render, the offscreen test + result, each gate PASS/FAIL with tail, any reference comment/line you could not place.',
  { label: 'port:bgimage', agentType: 'general-purpose' })

log('bg-image port done. Head: ' + String(portReport).slice(0, 200))

phase('Verify')
const LENSES = [
  'R1 COMMENT + LINE FIDELITY (the PRIMARY lens — G3b does NOT gate rustty-renderer): diff EVERY ported line AND comment of prep_background_image + upload_background_image + the bg_image render + the Path/RendererConfig::bg_image additions against generic.zig / the reference. Report any dropped/truncated/summarized comment (BLOCKER) or dropped branch/line. Confirm paths are byte-faithful (CString/&[u8], NOT String). Confirm any reference test{} in range is ported.',
  'CORRECTNESS + ACYCLICITY + DECODE: (a) the wuffs-sys PNG/JPEG decode actually works (run the test — a real image decodes to the right pixels); (b) prep/upload/render are faithful (FileType detect, decode, texture, full-screen quad); (c) the renderer-local Path leaks NO rustty_config type — run bash .port/tools/gates/check_acyclic.sh (no config edge); (d) any aliasing hazard. Run the offscreen bg-image test; confirm non-vacuous (bg pixels distinct from bg_color).',
  'GATES + no-regression: RUN cargo build --workspace, cargo fmt --check, cargo clippy --workspace --all-targets -- -D warnings, cargo test -p rustty-renderer, bash .port/tools/gates/check_acyclic.sh, python3 .port/tools/gates/check_test_counts.py, python3 .port/tools/gates/check_comment_coverage.py — report REAL results (tails). The golden + existing 45 tests must still pass.',
]
const verdicts = await parallel(LENSES.map((lens, i) => () =>
  agent(
    'ADVERSARIAL verifier for the renderer background-image pipeline (UNCOMMITTED working tree). LENS:\n' + lens + '\n' + COMMON + '\nFind the changed files (git diff/status), compare vs the reference, and RUN the gates your lens names. Report the schema: lens, verdict PASS/CONCERNS/FAIL, findings (severity, file, issue). Only REAL evidenced issues; if clean, empty findings + PASS + name the two things you attacked hardest in `lens`.',
    { label: 'verify:' + i, phase: 'Verify', agentType: 'general-purpose', schema: VERDICT_SCHEMA })))

const allFindings = verdicts.filter(Boolean).flatMap(v => (v.findings || []).map(f => ({ ...f, lens: v.lens })))
const blockers = allFindings.filter(f => f.severity === 'blocker')
log('bg-image verify: ' + verdicts.filter(Boolean).map(v => v.verdict).join(', ') + ' | ' + blockers.length + ' blockers, ' + allFindings.length + ' findings')
return { design, design_critique: critique, fold, port_report: portReport, verify: verdicts, findings: allFindings, blockers }
