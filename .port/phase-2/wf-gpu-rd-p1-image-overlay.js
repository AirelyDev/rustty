export const meta = {
  name: 'gpu-rd-p1-image-overlay',
  description: 'GPU R-d P1: port renderer/image.zig + renderer/Overlay.zig (Chunk-3 prereqs) into rustty-renderer, scope decided by golden-path usage, via design -> adversary -> port -> verify',
  phases: [
    { title: 'Design', detail: 'scope (full vs doctrine-seam per method) + image.rs/overlay.rs port plan -> GPU_RD_P1_DESIGN.md' },
    { title: 'Adversary-design', detail: 'break the scope/plan (surface coverage, seam safety, deps, acyclicity)' },
    { title: 'Port', detail: 'overlay.rs then image.rs, gate-green each' },
    { title: 'Verify', detail: 'panel: R1 fidelity, deps+acyclicity+gates, golden-path avoids the seams' },
  ],
}

const REPO = '/Users/gael/projects/rustty'
const REF = '/Users/gael/projects/ghostty'

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
        properties: {
          severity: { type: 'string', enum: ['blocker', 'major', 'minor'] },
          file: { type: 'string' }, issue: { type: 'string' },
        },
      },
    },
  },
}

const COMMON = `
REPO=${REPO} (rustty-renderer already has the whole metal/* backend: api + buffer/texture/sampler/pipeline/render_pass/target/frame/shaders + the Metal impl — Chunks 0/1a/1b/2, committed; the R-a leaves cell.rs/shaders.rs/cursor.rs/state.rs/link.rs/shadertoy.rs are ported). REFERENCE (read-only)=${REF} (Zig 0.15.2).
Rules: ${REPO}/.port/PORT_RULES.md — R1 (100% fidelity: every line/comment/test{} ported; reproduce behavior+quirks), R3, R4 (crate graph ACYCLIC — gate check_acyclic.sh), R5 (never weaken; deferred bodies are R5-preserved as unimplemented!("...") seams with the verbatim Zig body kept as a comment — the placeholder-leaf-doctrine).
Decision committed: generic.zig/Metal port is MONOMORPHIC (concrete types). image.zig+Overlay.zig are Chunk-3 prereqs (GenericRenderer fields: images: image::State, bg_image: Option<image::Image>, bg_image_buffer: crate::shaders::BgImage, overlay: Option<Overlay>). See ${REPO}/.port/phase-2/GPU_RD_TRAIT_DESIGN.md §D + GPU_RD_BLUEPRINT.md [F10/F11] (permits the offscreen golden path's minimal subset).
`

phase('Design')
const designSummary = await agent(`You are the ARCHITECT for GPU R-d P1. Write an implementable design doc to ${REPO}/.port/phase-2/GPU_RD_P1_DESIGN.md (Write tool). Design ONLY; do not touch crates/.
${COMMON}
Port ${REF}/src/renderer/image.zig (958 loc, Kitty-graphics image render) and ${REF}/src/renderer/Overlay.zig (415 loc, z2d-based overlay) into rustty-renderer as image.rs + overlay.rs (or a module). READ both fully.
KEY DECISION — SCOPE per method (full-port vs R5 unimplemented!() seam), driven by what generic.zig + the offscreen golden actually EXERCISE:
- generic.zig's image/overlay surface (grep-confirmed): self.images.{deinit, draw(x4 call sites: the kitty-below/below-text/above render steps), upload, overlayUpdate}; self.overlay.{reset/applyFeatures/pendingImage...}; rebuildOverlay (a generic.zig METHOD, Chunk 3, not here); self.bg_image / bg_image_buffer / bg_image_buffer_modified. Read generic.zig lines ~1349-1412 (updateFrame image/overlay), ~1550-1684 (drawFrame image/bg_image steps), ~1816-2004 (bg_image), ~2226-2285 (overlay build/draw), ~801-823 (deinit).
- The offscreen golden (GPU_RD_BLUEPRINT §6) renders MONOCHROME TEXT on solid bg — NO Kitty images, NO overlay content. So drawFrame's images.draw is CALLED but iterates an EMPTY image map (no-op); rebuildOverlay runs with no overlay. DECIDE precisely which methods MUST port fully (the container State + init/deinit + draw-dispatch iterate-empty path + the lifecycle generic.zig calls) vs which heavy leaves can be R5 unimplemented!() seams (e.g. Image decode/upload of actual Kitty pixel data, overlay feature RENDER via z2d — never hit by a no-image/no-overlay golden). A whole method is either ported-fully or a seam (never half). Prefer FULL port where feasible (R1); seam ONLY the genuinely-heavy leaves the golden never runs, each with the verbatim Zig body preserved.
- Give the EXACT Rust shapes: image::State (fields incl. the ImageMap = AutoHashMapUnmanaged(Id, ...), Placement, Id union, Image union) + method signatures; Overlay (Color/Feature/InitError/init/deinit/pendingImage/reset/applyFeatures) — mapping z2d (rustty-z2d, ported) + font types.
- CONFIRM the crate deps + acyclicity: does rustty-renderer already dep rustty-z2d (Overlay needs it) and rustty-vt (image.zig needs terminal image storage)? grep both Cargo.tomls; if an edge is missing, state it must be ADDED and confirm it's acyclic (check_acyclic). image.zig also uses the renderer api (Metal) for texture upload/draw — route to the landed crate::metal leaves.
- State the COMPILE BOUNDARY (image.rs/overlay.rs compile against the landed leaves + crate::shaders, WITHOUT the not-yet-existing GenericRenderer — the draw methods take the api/pass as params, like the leaves do) + the smoke-test gate + any un-deferred reference test{}.
Deliver concrete rust signature blocks + a per-method SCOPE TABLE (full/seam + why) + OPEN RISKS. Return a ~12-line summary: the scope decision, the dep edges needed, the method split, the compile boundary.`, { label: 'design:p1', agentType: 'general-purpose' })

log(`P1 design written. Head: ${String(designSummary).slice(0, 200)}`)

phase('Adversary-design')
const critique = await agent(`ADVERSARIAL reviewer: try to BREAK the P1 design at ${REPO}/.port/phase-2/GPU_RD_P1_DESIGN.md. Re-derive from source.
${COMMON}
Read the design doc, ${REF}/src/renderer/image.zig + Overlay.zig fully, and generic.zig's image/overlay call-sites (grep self.images/self.overlay/bg_image/rebuildOverlay). CHECK: (1) Does the scope cover generic.zig's ENTIRE image/overlay surface — is any method generic.zig calls left neither ported nor seamed (a hole)? (2) Are the deferred seams SAFE for the offscreen golden — will the monochrome-text/no-image/no-overlay drawFrame path ever CALL a seamed method and panic (esp. images.draw with an empty map, rebuildOverlay, updateFrame's image/overlay branches)? Trace the golden's drawFrame/updateFrame path. (3) Deps+acyclicity: are the rustty-z2d / rustty-vt edges present-or-addable without a cycle (grep the Cargo.tomls + check_acyclic)? Any type the design names that doesn't exist in the ported z2d/vt/font? (4) Any reference test{} in image.zig/Overlay.zig dropped (check_test_counts)? (5) R1: is a whole method ever split half-ported (illegal)? Is every seam's Zig body preserved? (6) Do the Image/Placement/Id union mappings match the Zig (union(enum) -> Rust enum) faithfully?
Return the schema: verdict, blocking_issues, concrete fixes (name file+change), summary.`, { label: 'adversary:p1-design', agentType: 'general-purpose', schema: CRITIQUE_SCHEMA })

log(`P1 adversary-design: ${critique.verdict} | blocking=${critique.blocking_issues.length} fixes=${critique.fixes.length}`)

const fixesBlock = (critique.fixes && critique.fixes.length)
  ? `\n\nADVERSARY-DESIGN (${critique.verdict}) — HONOR these fixes:\n- ${critique.fixes.join('\n- ')}\nAddress these blocking issues:\n- ${(critique.blocking_issues||[]).join('\n- ')}`
  : ''

phase('Port')
const overlayReport = await agent(`PORTING engineer: implement P1 PART 1 — port ${REF}/src/renderer/Overlay.zig into rustty-renderer (overlay.rs) exactly as designed in ${REPO}/.port/phase-2/GPU_RD_P1_DESIGN.md.${fixesBlock}
${COMMON}
Port Overlay.zig (Color/Feature/InitError/Overlay + init/deinit/pendingImage/reset/applyFeatures + any helpers), mapping z2d (rustty-z2d) + font types per the design. Full-port vs R5-seam per the design's scope table. Preserve every comment/line/test{} (R1). Add whatever Cargo dep edge the design specifies (acyclic). Wire the module in lib.rs.
GATES (run; ALL green, never weaken): cargo build -p rustty-renderer, cargo build --workspace (no regression), cargo fmt -p rustty-renderer -- --check, cargo clippy -p rustty-renderer --all-targets -- -D warnings, cargo test -p rustty-renderer, bash .port/tools/gates/check_acyclic.sh. STOP+report a precise blocker if any can't go green honestly. Do NOT git commit. Report: per-item summary, the z2d/font types used, the scope (full/seam) per method, each gate PASS/FAIL with tail, any comment unplaced.`, { label: 'port:overlay', agentType: 'general-purpose' })

log(`Overlay ported. Head: ${String(overlayReport).slice(0, 160)}`)

const imageReport = await agent(`PORTING engineer: implement P1 PART 2 — port ${REF}/src/renderer/image.zig into rustty-renderer (image.rs) exactly as designed in ${REPO}/.port/phase-2/GPU_RD_P1_DESIGN.md. Part 1 (overlay.rs) is ALREADY DONE in the working tree; build on it.${fixesBlock}
${COMMON}
Port image.zig (State + init/deinit/upload/draw/overlayUpdate; Placement; Id union; ImageMap; Image union + deinit/upload), full-port vs R5-seam per the design's scope table (seam ONLY the heavy Kitty-decode/upload leaves the golden never runs, with the verbatim Zig body preserved). The draw/upload dispatch route to the landed crate::metal leaves (Buffer<Image>/Texture/RenderPass) + take the api/pass as params (no GenericRenderer dependency). Preserve every comment/line/test{} (R1). ARC->Drop, Option->Error per the metal-leaf precedent. Add whatever Cargo dep edge the design specifies (acyclic). Wire the module in lib.rs.
GATES (run; ALL green): cargo build -p rustty-renderer, cargo build --workspace (no regression), cargo fmt -p rustty-renderer -- --check, cargo clippy -p rustty-renderer --all-targets -- -D warnings, cargo test -p rustty-renderer, bash .port/tools/gates/check_acyclic.sh, python3 .port/tools/gates/check_test_counts.py. STOP+report a precise blocker if any can't go green honestly (do NOT weaken/flail). Do NOT git commit. Report: per-item summary + objc2/leaf symbols, the scope (full/seam) per method with the seamed ones listed, whether any reference test{} was ported, each gate PASS/FAIL with tail, any comment unplaced / faithfulness compromise.`, { label: 'port:image', agentType: 'general-purpose' })

log(`Image ported. Head: ${String(imageReport).slice(0, 160)}`)

phase('Verify')
const LENSES = [
  'R1 fidelity: every image.zig + Overlay.zig line/comment/test{} ported or R5-seamed (diff method-by-method vs the reference); no method split half-ported; every seam preserves the verbatim Zig body; the Image/Placement/Id union(enum) mappings faithful. Run: python3 .port/tools/gates/check_test_counts.py and python3 .port/tools/gates/check_comment_coverage.py and report the real results.',
  'Golden-path safety: trace the offscreen golden drawFrame/updateFrame (monochrome text, NO images, NO overlay) against the ported image.rs/overlay.rs — will it ever CALL a seamed (unimplemented!) method and panic? Check images.draw over an empty ImageMap, rebuildOverlay/overlay reset, updateFrame image/overlay branches. Any seam reachable by the golden is a BLOCKER.',
  'Deps + acyclicity + gates + no-regression: the added rustty-z2d/rustty-vt (or other) dep edges keep the crate graph acyclic; nothing regresses. Actually RUN: cargo build --workspace, cargo clippy --workspace --all-targets -- -D warnings, cargo test -p rustty-renderer, bash .port/tools/gates/check_acyclic.sh, cargo fmt --check — and report the real results.',
]
const verdicts = await parallel(LENSES.map((lens, i) => () =>
  agent(`ADVERSARIAL verifier for GPU R-d P1 (image.rs + overlay.rs, uncommitted). LENS:\n${lens}\n${COMMON}\nFind the changed files (git status/diff --stat; new files in crates/rustty-renderer/src/), compare vs ${REF}/src/renderer/{image,Overlay}.zig, RUN any gate your lens names. Report the schema: lens, verdict PASS/CONCERNS/FAIL, findings (severity blocker/major/minor, file, issue). Only REAL evidenced issues; if clean, empty findings + PASS + name the two things you tried hardest to break.`,
    { label: `verify:${i}`, phase: 'Verify', agentType: 'general-purpose', schema: VERDICT_SCHEMA })))

const allFindings = verdicts.filter(Boolean).flatMap(v => (v.findings || []).map(f => ({ ...f, lens: v.lens })))
const blockers = allFindings.filter(f => f.severity === 'blocker')
log(`P1 verify: ${verdicts.filter(Boolean).map(v => v.verdict).join(', ')} | ${blockers.length} blockers, ${allFindings.length} findings`)

return { design_summary: designSummary, design_critique: critique, overlay_report: overlayReport, image_report: imageReport, verify: verdicts, blockers }