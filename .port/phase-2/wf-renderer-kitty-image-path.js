export const meta = {
  name: 'renderer-kitty-image-path',
  description: 'Fill the deferred rustty-renderer Kitty graphics image pixel path (image.rs): kitty_update + prep_kitty_placement + prep_kitty_virtual_placement + prep_kitty_image + Image::convert — reading the vt kitty image storage (t.screens.active.kitty_images), computing placements, and uploading image data to GPU textures. draw/upload/overlay_update/kitty_requires_update are already ported. Design (guarded, verifies the vt-kitty API is reachable + STOPS if blocked) -> adversary -> fold -> port (stop-if-no-design) -> verify.',
  phases: [
    { title: 'Design', detail: 'verify the vt kitty storage API (kitty_images/imageById/placements/pins) is reachable from the renderer; scope the 5 seams + Image::convert; design the offscreen APC-driven test; WRITE the design doc (guarded). STOP-report if the vt-kitty API is not exposed.' },
    { title: 'Adversary-design', detail: 'attack: is every vt-kitty access real+ported? the placement/viewport-containment math fidelity, the GPU upload, any raw-ptr/aliasing hazard (t is *const), dead-code, dropped test{}' },
    { title: 'Fold-fixes', detail: 'fold adversary findings INTO the doc; skipped when SOUND' },
    { title: 'Port', detail: 'implement exactly as the hardened doc specifies; STOP if the design doc is absent' },
    { title: 'Verify', detail: 'panel: R1-comment fidelity (manual), correctness (placement math + GPU upload), gates + the offscreen kitty render test' },
  ],
}

const REPO = '/Users/gael/projects/rustty'
const REF = '/Users/gael/projects/ghostty'
const DESIGN_DOC = REPO + '/.port/phase-2/RENDERER_KITTY_IMAGE_PATH_DESIGN.md'

const DESIGN_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['wrote_doc', 'vt_kitty_api_reachable', 'scope_methods', 'gpu_upload_plan', 'test_plan', 'compile_boundary', 'summary'],
  properties: {
    wrote_doc: { type: 'boolean' },
    vt_kitty_api_reachable: { type: 'string' },
    scope_methods: { type: 'array', items: { type: 'string' } },
    gpu_upload_plan: { type: 'string' },
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
  'REPO=' + REPO + '. REFERENCE (read-only)=' + REF + ' (Zig 0.15.2, src/renderer/Image.zig — the kitty image state). The design doc is ' + DESIGN_DOC + '.',
  'STATE: GPU R-d + the renderer config/resize surface are DONE + committed. This milestone fills the DEFERRED Kitty graphics image PIXEL PATH in crates/rustty-renderer/src/image.rs. Already ported (NOT seams): State::{empty,deinit,upload,draw,overlay_update,kitty_requires_update}, Pending/Image helpers. The R5 unimplemented!() SEAMS to fill (each has its verbatim reference body preserved as a leading comment block, R1): kitty_update (image.rs ~:459, ref Image.zig kittyUpdate), prep_kitty_placement (~:532), prep_kitty_virtual_placement (~:603), prep_kitty_image (~:731), Image::convert (~:1059).',
  'THE VT-KITTY DEPENDENCY (verify FIRST): the path reads `t.screens.active.kitty_images` (the storage) — `storage.imageById(id)`, `storage.placements` (iterator), `storage.dirty`, and the viewport pins `pages.getTopLeft(.viewport)`/`getBottomRight(.viewport)`/`pointFromPin(.screen, pin)`. rustty-vt ported the Kitty graphics support (APC "G" -> the image storage). CONFIRM these exact APIs exist + are accessible from the renderer (the renderer already deps rustty-vt) BEFORE designing — if any is missing/unexposed, STOP and report vt_kitty_api_reachable=blocked with the exact missing symbol (do NOT design against a nonexistent API).',
  'GPU UPLOAD: prep_kitty_image converts the image data (Image::convert — rgb->rgba etc.) + uploads to a GPU texture via the ported metal Texture / api.imageTextureOptions / init_atlas-style path (the already-ported State::upload sends Pending images to the GPU). Map the convert + upload faithfully to the ported metal Texture API + the Pending/Image types.',
  'Rules: ' + REPO + '/.port/PORT_RULES.md — R1 (100% fidelity: every line/comment/test{} ported; byte-slices -> &[u8]/Vec<u8>; the seam bodies have the verbatim reference preserved — port it faithfully), R4 (ACYCLIC), R5 (anything still deferred = unimplemented!() seam + verbatim body). memory rust-mut-args-strongly-protected / shaper-hook-sb-clean-interleave: `t: *const Terminal` is a READ-only alias — flag any raw-alias-crossed-by-&mut; if a new pattern arises PROVE on .port/phase-2/mirihar-chunk3b/. G3b comment-coverage SKIPS rustty-renderer, so the adversary/verify are the ONLY R1 backstop.',
  'TESTABILITY: drive a Terminal with a Kitty graphics APC sequence (upload a small image via `\\x1b_Ga=T,f=24,s=<w>,v=<h>,...;<base64 rgb>\\x1b\\\\` + a placement) using the ported vt stream, `update_frame` (which calls kitty_update when kitty_requires_update is true), `draw_frame`, and assert the image rendered (the kitty placement pixels in the IOSurface readback, or the image State: a GPU texture created + a placement computed). macOS/Metal-gated + Metal-skip like the golden.',
].join('\n')

phase('Design')
const design = await agent(
  'ARCHITECT: fill the deferred rustty-renderer Kitty image pixel path. You MUST write ' + DESIGN_DOC + ' with the Write tool (guarded — wrote_doc=true only if written). Design ONLY; touch NO crates/. FIRST verify the vt-kitty API is reachable (see COMMON) — grep rustty-vt for the kitty storage/imageById/placements/pin APIs; if blocked, still WRITE the doc recording vt_kitty_api_reachable=blocked + the missing symbol + a fallback (a different next milestone), and return.\n' + COMMON + '\n\n' +
  'If reachable, DECIDE + specify (concrete Rust signature blocks + the vt-kitty access map + the placement/containment math + the GPU-upload map + OPEN RISKS): (1) the 5 seams (kitty_update/prep_kitty_placement/prep_kitty_virtual_placement/prep_kitty_image/Image::convert) mapped to the ported vt-kitty + metal Texture APIs, faithful to the verbatim reference bodies; (2) the viewport-containment placement math (top/bot pins, offsets, cell-size scaling); (3) Image::convert (the pixel-format conversions) + the GPU texture upload; (4) the offscreen APC-driven test; (5) dead-code + no-regression (the golden + existing 44 tests still pass; kitty_update was called from update_frame under a guard — filling it means the guard can now be exercised). Return the schema: wrote_doc, vt_kitty_api_reachable, scope_methods, gpu_upload_plan, test_plan, compile_boundary, summary.',
  { label: 'design:kitty', agentType: 'general-purpose', schema: DESIGN_SCHEMA })

if (!design.wrote_doc) {
  log('DESIGN GUARD TRIPPED: design agent did NOT write the design doc. Aborting before port.')
  return { aborted: 'design-doc-not-written', design }
}
if (String(design.vt_kitty_api_reachable).toLowerCase().includes('blocked')) {
  log('VT-KITTY API BLOCKED: ' + design.vt_kitty_api_reachable + ' — aborting this milestone (pick a different next surface). No port attempted.')
  return { aborted: 'vt-kitty-api-blocked', design }
}
log('kitty design written. scope=' + (design.scope_methods || []).join(',').slice(0, 140))

phase('Adversary-design')
const critique = await agent(
  'ADVERSARIAL reviewer: BREAK the Kitty-image-path design at ' + DESIGN_DOC + '. Re-derive INDEPENDENTLY from source.\n' + COMMON + '\n\n' +
  'Read the design + Image.zig kittyUpdate/prepKittyPlacement/prepKittyVirtualPlacement/prepKittyImage/convert + the ported image.rs seam bodies + the rustty-vt kitty storage API. CHECK, breaking each: (1) VT-KITTY ACCESS — does EVERY cited vt symbol (kitty_images/imageById/placements/dirty/getTopLeft/getBottomRight/pointFromPin) actually exist + is it accessible (grep to confirm)? Any invented API? (2) FIDELITY — is each seam a faithful port of the verbatim reference body (every branch: pin vs virtual placement, the imageById-missing warn+continue, the markForUnload GC sweep, the containment/offset math), or summarized/half-ported? (3) GPU UPLOAD — does Image::convert + the texture upload map to real ported metal APIs; byte-faithful pixel formats? (4) ALIASING — t is *const Terminal (read-only); any raw-alias-crossed-by-&mut hazard? (5) TESTABILITY — will the APC-driven test actually populate the vt kitty storage + trigger kitty_requires_update; is it non-vacuous? (6) any reference test{} dropped? Return the schema: verdict, blocking_issues, concrete fixes, summary.',
  { label: 'adversary:kitty-design', agentType: 'general-purpose', schema: CRITIQUE_SCHEMA })

log('kitty adversary: ' + critique.verdict + ' | blocking=' + critique.blocking_issues.length + ' fixes=' + critique.fixes.length)

phase('Fold-fixes')
const needsFold = critique.verdict !== 'SOUND' && ((critique.fixes && critique.fixes.length) || (critique.blocking_issues && critique.blocking_issues.length))
let fold = null
if (needsFold) {
  fold = await agent(
    'DESIGN EDITOR: the adversary reviewed the Kitty-image-path design (' + DESIGN_DOC + ') and returned verdict=' + critique.verdict + '. FOLD its findings INTO the doc (single authoritative source).\n' + COMMON + '\n\n' +
    'ADVERSARY BLOCKING ISSUES:\n- ' + (critique.blocking_issues || []).join('\n- ') + '\nADVERSARY FIXES:\n- ' + (critique.fixes || []).join('\n- ') + '\n\n' +
    'For EACH: verify against the reference/rustty-vt source yourself (reject any the source contradicts; note rejected + why in `folded`). Edit ' + DESIGN_DOC + ' to incorporate the valid ones. Preserve structure. Touch NO crates/. You MUST end with the doc written. Return: wrote_doc, folded, summary.',
    { label: 'fold:adversary-fixes', agentType: 'general-purpose', schema: FOLD_SCHEMA })
  if (!fold.wrote_doc) {
    log('FOLD GUARD TRIPPED: doc not present after folding. Aborting before port.')
    return { aborted: 'design-doc-missing-after-fold', design, critique, fold }
  }
  log('kitty fold-fixes: ' + (fold.folded || []).length + ' items :: ' + String(fold.summary).slice(0, 120))
} else {
  log('kitty design SOUND — no fold needed; porting directly from the doc.')
}

phase('Port')
const portReport = await agent(
  'PORTING engineer: implement the Kitty image pixel path EXACTLY as specified in the (adversary-hardened) design doc ' + DESIGN_DOC + '. FIRST Read the doc + verify it EXISTS + is substantive — if absent or a stub, STOP + report "design doc missing".\n' + COMMON + '\n\n' +
  'Fill the 5 seams (kitty_update/prep_kitty_placement/prep_kitty_virtual_placement/prep_kitty_image/Image::convert) in crates/rustty-renderer/src/image.rs per the design, reading the vt-kitty storage + computing placements + uploading to GPU textures. Every line+comment ported (R1 — preserve EVERY reference comment verbatim by hand); byte-faithful pixel formats; raw-ptr discipline for the *const Terminal read (PROVE any new pattern on .port/phase-2/mirihar-chunk3b/). Add the design-specified offscreen APC-driven test. Anything still deferred stays an unimplemented!() seam with verbatim body.\n' +
  'GATES (run; ALL green, never weaken): cargo build --workspace, cargo fmt --check, cargo clippy -p rustty-renderer --all-targets -- -D warnings, cargo test -p rustty-renderer, bash .port/tools/gates/check_acyclic.sh, python3 .port/tools/gates/check_test_counts.py, python3 .port/tools/gates/check_comment_coverage.py. If a gate can not go green HONESTLY, STOP + report the precise blocker. Do NOT git commit. Report: seams filled (line ranges), the vt-kitty access, the placement math + GPU upload, the offscreen test + result, each gate PASS/FAIL with tail, any reference comment/line you could not place.',
  { label: 'port:kitty', agentType: 'general-purpose' })

log('kitty port done. Head: ' + String(portReport).slice(0, 200))

phase('Verify')
const LENSES = [
  'R1 COMMENT + LINE FIDELITY (the PRIMARY lens — G3b does NOT gate rustty-renderer): diff EVERY ported line AND comment of kitty_update/prep_kitty_placement/prep_kitty_virtual_placement/prep_kitty_image/Image::convert against Image.zig. Report any dropped/truncated/summarized comment (BLOCKER) or dropped branch/line. Confirm any reference test{} in range is ported.',
  'CORRECTNESS: (a) the vt-kitty access is faithful (imageById/placements/pins) + the placement containment/offset/scaling math matches the reference; (b) Image::convert pixel formats + the GPU texture upload are byte-faithful; (c) t:*const Terminal read-only — no aliasing hazard (re-run/extend mirihar if a new pattern). Run the offscreen kitty render/state test; confirm it is non-vacuous (a kitty image actually populates the storage + a texture/placement results).',
  'GATES + no-regression: RUN cargo build --workspace, cargo fmt --check, cargo clippy -p rustty-renderer --all-targets -- -D warnings, cargo test -p rustty-renderer (the golden + existing 44 must still pass), bash .port/tools/gates/check_acyclic.sh, python3 .port/tools/gates/check_test_counts.py, python3 .port/tools/gates/check_comment_coverage.py — report REAL results (tails).',
]
const verdicts = await parallel(LENSES.map((lens, i) => () =>
  agent(
    'ADVERSARIAL verifier for the renderer Kitty image path (UNCOMMITTED working tree). LENS:\n' + lens + '\n' + COMMON + '\nFind the changed files (git diff/status), compare vs ' + REF + '/src/renderer/Image.zig, and RUN the gates your lens names. Report the schema: lens, verdict PASS/CONCERNS/FAIL, findings (severity, file, issue). Only REAL evidenced issues; if clean, empty findings + PASS + name the two things you attacked hardest in `lens`.',
    { label: 'verify:' + i, phase: 'Verify', agentType: 'general-purpose', schema: VERDICT_SCHEMA })))

const allFindings = verdicts.filter(Boolean).flatMap(v => (v.findings || []).map(f => ({ ...f, lens: v.lens })))
const blockers = allFindings.filter(f => f.severity === 'blocker')
log('kitty verify: ' + verdicts.filter(Boolean).map(v => v.verdict).join(', ') + ' | ' + blockers.length + ' blockers, ' + allFindings.length + ' findings')
return { design, design_critique: critique, fold, port_report: portReport, verify: verdicts, findings: allFindings, blockers }
