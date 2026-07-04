export const meta = {
  name: 'gpu-rd-chunk2-metal',
  description: 'GPU R-d Chunk 2: hoist AlphaBlending (break the config->renderer cycle) + port Metal.zig impl (offscreen), via design -> adversary -> port -> verify',
  phases: [
    { title: 'Design', detail: 'AlphaBlending hoist + Metal.zig offscreen port plan -> GPU_RD_CHUNK2_DESIGN.md' },
    { title: 'Adversary-design', detail: 'independently break the plan (cycle/config-API/parity-gates/Metal-surface/UB)' },
    { title: 'Port', detail: 'apply the hoist (gate-green) then port Metal.zig (gate-green)' },
    { title: 'Verify', detail: 'adversarial panel: UB/FFI, R1 fidelity, R4-acyclicity+gates' },
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
          file: { type: 'string' },
          issue: { type: 'string' },
        },
      },
    },
  },
}

const COMMON = `
REPO=${REPO} (crate rustty-renderer already has the whole metal/* leaf tier: api/buffer/texture/sampler/pipeline/render_pass/target/frame/shaders — Chunks 0/1a/1b, committed). REFERENCE (read-only)=${REF} (Zig 0.15.2).
Rules: ${REPO}/.port/PORT_RULES.md — R1 (100% fidelity: every line/comment/test{} ported; reproduce behavior+quirks; comments only updated to rustty names), R3 (objc2 replaces pkg/objc+pkg/macos; same frameworks), R4 (crate graph ACYCLIC, gate check_acyclic.sh), R5 (never weaken a gate).
Decision already made+committed: port generic.zig/Metal MONOMORPHICALLY (concrete types, NO trait hierarchy) — see ${REPO}/.port/phase-2/GPU_RD_TRAIT_DESIGN.md.
`

phase('Design')
const designSummary = await agent(`You are the ARCHITECT for GPU R-d CHUNK 2. Produce an implementable design doc at ${REPO}/.port/phase-2/GPU_RD_CHUNK2_DESIGN.md (write it with the Write tool). Design ONLY; do not touch crates/.
${COMMON}
Chunk 2 has TWO parts:

PART A — break the config->renderer cycle for AlphaBlending (a prerequisite).
- \`AlphaBlending\` is a clean enum { Native, Linear, LinearCorrected } + \`is_linear()\` at ${REPO}/crates/rustty-config/src/value_types.rs:597. Metal.zig reads \`config.blending\` and branches on \`blending.isLinear()\` to pick bgra8unorm_srgb vs bgra8unorm (initShaders/initTarget/textureOptions). rustty-config DEPENDS ON rustty-renderer (Cargo.toml:48) => the renderer cannot name any rustty-config type (cycle).
- DECIDE the faithful break (recommend: HOIST the AlphaBlending enum + is_linear() to rustty-render-types — a leaf both config and renderer already depend on; the size/Health/render-types A1-hoist precedent). config then RE-EXPORTS it (\`pub use rustty_render_types::AlphaBlending;\`) so config's public path + all use-sites keep working.
- The SUBTLETY you must resolve exactly: \`AlphaBlending::string_to_enum\`/\`to_name\` are macro-generated INHERENT methods via \`bare_enum_names!\` (value_types.rs:175). You CANNOT add inherent methods to a foreign (render-types) type. Give the exact refactor so config keeps parsing AlphaBlending (used at config_struct.rs:9137 \`field_enum!(alpha_blending, AlphaBlending::string_to_enum)\`, plus TypeId at config_struct.rs:597, c_get.rs:246 \`g_enum!\`, deep_equal.rs:100, the Key enum, config_struct.rs:4669 defaults). Options to weigh: (i) move the enum+is_linear to render-types and keep string_to_enum/to_name as config free-fns/trait impl'd for the hoisted type (a config-local trait CAN be impl'd for a foreign type); (ii) move the parsing too (but string-parsing is a config concern, wrong layer for render-types). Pick one; give the exact code shape + every call-site edit.
- PARITY-GATE implication: AlphaBlending's def+comment moving out of rustty-config's value_types.rs (which maps to a ghostty config file in check_test_counts.py/check_comment_coverage.py). CHECK how the existing size/Health hoist handled the gate mapping (grep .port/tools/gates + the render-types lib.rs for how hoisted-type comments/tests are attributed) and follow that precedent so G1/G3/G3b stay green. State exactly what comment/pointer stays in value_types.rs.

PART B — port Metal.zig (${REF}/src/renderer/metal/Metal.zig, 458 loc; I have read it) MONOMORPHICALLY into rustty-renderer (crates/rustty-renderer/src/metal/metal_impl.rs or similar — pick a name; wire in metal.rs), OFFSCREEN scope.
- Offscreen-portable (port fully): \`chooseDevice\` (macOS: MTLCopyAllDevices() iteration skipping isHeadless, preferring isRemovable/isLowPower — [ADV-FIX F1]: objc2-metal exposes bare MTLCopyAllDevices directly, NO declare_extern; returns Retained<NSArray<ProtocolObject<dyn MTLDevice>>>), newCommandQueue, default_storage_mode (hasUnifiedMemory ? Shared : Managed), queryMaxTextureSize (supportsFamily apple10/apple3 -> 32768/16384/8192), the option factories (bufferOptions + the 6 aliases instanceBufferOptions/uniformBufferOptions/fgBufferOptions/bgBufferOptions/imageBufferOptions/bgImageBufferOptions — Rust can't alias methods by \`pub const\`, so 6 one-line forwarders; textureOptions/samplerOptions/imageTextureOptions), ImageTextureFormat enum + toPixelFormat, initAtlasTexture, initTarget, initShaders (uses blending.is_linear() -> srgb pixel format), beginFrame, drawFrameStart/drawFrameEnd (AutoreleasePool via objc2's autoreleasepool / AutoreleasePool — find the objc2 API), presentLastTarget (noop).
- DEFERRED as R5-preserved \`unimplemented!("windowing-host")\` seams (state each; preserve the verbatim Zig body as a comment): the \`layer: IOSurfaceLayer\` field + the whole \`init()\` NSView-attach half (wantsLayer/addSublayer/clipsToBounds/contentsScale/needsDisplayOnBoundsChange), \`surfaceSize\` (reads layer.bounds -> for offscreen use a FIXED size passed in / a field, NOT the layer), \`present\` (layer.setSurfaceSync -> offscreen the IOSurface readback in Target::read_bgra replaces it; present is a noop/stub offscreen), \`loopEnter\`/\`displayCallback\`.
- KEY STRUCTURE decision: Metal's real \`init(alloc, opts)\` needs \`opts.rt_surface.platform.nsview\` (a live view) => cannot run offscreen. Design how the offscreen golden constructs a Metal: e.g. a separate \`Metal::init_offscreen(device_or_default, blending, storage...)\` OR make init() port faithfully with the view-attach as the seam + an offscreen constructor that does chooseDevice+newCommandQueue+metadata WITHOUT the layer. Metal must expose device/queue/default_storage_mode/max_texture_size + the init* methods so Chunk 3/4 (generic.zig SwapChain + golden) can drive it. Metal.blending: a field of the hoisted render-types AlphaBlending.
- Metal.zig imports \`rendererpkg.Options\` (renderer.Options) + \`apprt\` — for offscreen, minimize: does Chunk 2 need renderer::Options ported now, or can the offscreen constructor take the few needed values directly (blending, and nothing view-related)? Decide + state the compile boundary (Chunk 2 must \`cargo build -p rustty-renderer\` + a smoke test that: chooseDevice/default device -> newCommandQueue -> initShaders(load pipelines) -> initTarget -> beginFrame, no panic).

Deliver the doc with concrete \`\`\`rust signature blocks for Part A (the hoist + the string_to_enum refactor + every call-site edit) and Part B (the Metal struct + all methods + the seams), a "COMPILE BOUNDARY" section, and "OPEN RISKS". Return a ~12-line executive summary: the AlphaBlending break chosen + the string_to_enum resolution + the parity-gate handling + the offscreen-Metal construction + the compile boundary.`, { label: 'design:chunk2', agentType: 'general-purpose' })

log(`Design written. Summary head: ${String(designSummary).slice(0, 200)}`)

phase('Adversary-design')
const critique = await agent(`You are an ADVERSARIAL reviewer. Independently try to BREAK the Chunk-2 design at ${REPO}/.port/phase-2/GPU_RD_CHUNK2_DESIGN.md. Re-derive from sources; do not trust the doc.
${COMMON}
Read the design doc, ${REF}/src/renderer/metal/Metal.zig, ${REPO}/crates/rustty-config/src/value_types.rs (AlphaBlending + bare_enum_names!) + its use-sites (config_struct.rs:597/4669/9137, c_get.rs:246, deep_equal.rs:100), ${REPO}/crates/rustty-render-types/ (the hoist target — confirm it stays a leaf, grep its Cargo.toml deps), and the parity gates ${REPO}/.port/tools/gates/{check_acyclic.sh,check_test_counts.py,check_comment_coverage.py}.
CHECK (concrete evidence for each): (1) Does the AlphaBlending hoist actually break the cycle AND keep check_acyclic green (render-types must not gain any edge that reaches config/renderer)? (2) Does the string_to_enum/to_name refactor compile (orphan rule: can config impl its parsing for the foreign type; do ALL use-sites still resolve)? (3) Do the parity gates stay green — will moving AlphaBlending's def+comment out of value_types.rs make G1/G3/G3b RED, and does the plan handle it like the size/Health precedent? (4) Is Metal.zig's FULL public surface covered (every method generic.zig/image.zig call — chooseDevice, newCommandQueue, all 6 *BufferOptions names, textureOptions/samplerOptions/imageTextureOptions, initAtlasTexture/initTarget/initShaders/beginFrame, drawFrameStart/End, presentLastTarget, the consts swap_chain_count/custom_shader_target/custom_shader_y_is_down)? (5) Is the offscreen-construction sound (can the golden build a Metal without a live NSView; are present/surfaceSize/layer correctly seamed not lost)? (6) [F1] Is MTLCopyAllDevices truly exposed by objc2-metal (grep the source) and the iteration faithful (isHeadless skip, isRemovable/isLowPower break)? (7) Any objc2 AutoreleasePool API gap? Any UB in the device iteration?
Return the schema: verdict SOUND / SOUND-WITH-FIXES / BROKEN, the blocking_issues, the concrete fixes the port must apply, and a summary. Be specific; a fix must name the file+change.`, { label: 'adversary:design', agentType: 'general-purpose', schema: CRITIQUE_SCHEMA })

log(`Adversary-design: ${critique.verdict} | blocking=${critique.blocking_issues.length} fixes=${critique.fixes.length}`)

const fixesBlock = (critique.fixes && critique.fixes.length)
  ? `\n\nADVERSARY-DESIGN found ${critique.verdict}. You MUST honor these fixes:\n- ${critique.fixes.join('\n- ')}\nAnd address these blocking issues:\n- ${(critique.blocking_issues||[]).join('\n- ')}`
  : ''

phase('Port')
const hoistReport = await agent(`You are a PORTING engineer. Implement CHUNK 2 PART A ONLY: the AlphaBlending cycle-break, exactly as designed in ${REPO}/.port/phase-2/GPU_RD_CHUNK2_DESIGN.md Part A.${fixesBlock}
${COMMON}
Hoist AlphaBlending (enum + is_linear) from rustty-config to rustty-render-types per the design; make config re-export it; resolve string_to_enum/to_name per the design; update every use-site; preserve every comment (R1) and handle the parity-gate attribution per the design's size/Health-precedent section.
This is a behavior-preserving R4 structural change ONLY — no logic altered.
GATES (run; ALL green, never weaken): \`cargo build --workspace\`, \`cargo test --workspace\` (config's tests MUST stay green), \`cargo fmt --check\`, \`cargo clippy --workspace --all-targets -- -D warnings\`, \`bash .port/tools/gates/check_acyclic.sh\`, \`python3 .port/tools/gates/check_test_counts.py\`, \`python3 .port/tools/gates/check_comment_coverage.py\`. If any can't go green honestly, STOP and report the precise blocker. Do NOT git commit. Report: files changed, the string_to_enum resolution used, each gate PASS/FAIL with tail, any comment you couldn't place.`, { label: 'port:alphablending-hoist', agentType: 'general-purpose' })

log(`Hoist ported. Head: ${String(hoistReport).slice(0, 160)}`)

const metalReport = await agent(`You are a PORTING engineer. Implement CHUNK 2 PART B: port Metal.zig offscreen, exactly as designed in ${REPO}/.port/phase-2/GPU_RD_CHUNK2_DESIGN.md Part B. Part A (AlphaBlending hoist) is ALREADY DONE in the working tree — build on it (blending is the hoisted rustty_render_types::AlphaBlending).${fixesBlock}
${COMMON}
Port ${REF}/src/renderer/metal/Metal.zig into rustty-renderer per the design (device/queue/chooseDevice[F1 MTLCopyAllDevices]/metadata/option-factories/initShaders/initTarget/initAtlasTexture/beginFrame/drawFrameStart+End AutoreleasePool/ImageTextureFormat + the offscreen construction), with the layer/init-NSView-attach/surfaceSize-from-layer/present/loopEnter/displayCallback as R5-preserved \`unimplemented!("windowing-host")\` seams (verbatim Zig body as a comment). Build on the landed metal/* leaves (Target/Frame/Shaders/etc.). Reproduce every line/comment (R1); ARC->Drop, Option->Error per the Chunk-1a/1b precedent. For objc2 method names read the generated source (~/.cargo/registry/.../objc2-metal-0.3.2/src/generated/); objc2 keeps camelCase with _ per colon.
Add a \`#[cfg(target_os="macos")] #[test]\` smoke test (rustty-only): default device -> newCommandQueue -> initShaders (all 5 pipelines) -> initTarget(offscreen) -> beginFrame, assert no panic. UN-defer nothing new here.
GATES (run; ALL green): \`cargo build -p rustty-renderer\`, \`cargo build --workspace\` (no regression), \`cargo fmt -p rustty-renderer -- --check\`, \`cargo clippy -p rustty-renderer --all-targets -- -D warnings\`, \`cargo test -p rustty-renderer\` (smoke test passes on this Metal host). STOP+report a precise blocker if any can't go green honestly (do NOT weaken/flail). Do NOT git commit. Report: per-method summary + objc2 symbols, the seams, the offscreen constructor shape, the smoke-test body+result, each gate PASS/FAIL with tail, any comment unplaced / faithfulness compromise.`, { label: 'port:metal', agentType: 'general-purpose' })

log(`Metal ported. Head: ${String(metalReport).slice(0, 160)}`)

phase('Verify')
const LENSES = [
  'UB / aliasing / objc2-FFI soundness: the chooseDevice MTLCopyAllDevices NSArray iteration (retain/Retained semantics, the isHeadless/isRemovable/isLowPower getters), the AutoreleasePool push/pop pairing in drawFrameStart/End, any raw pointer / device-metadata call. Look for use-after-free, wrong Retained ownership, a leaked or double-released device, an unbalanced autorelease pool.',
  'R1 fidelity: every Metal.zig line + comment ported (diff the reference method-by-method); the AlphaBlending hoist is behavior-preserving (is_linear unchanged, config parsing intact); the deferred layer/present/surfaceSize/loopEnter parts are R5-preserved as unimplemented!("windowing-host") with the verbatim Zig body as a comment, NOT dropped; the 6 *BufferOptions names all present.',
  'R4 acyclicity + gates + no-regression: the AlphaBlending hoist keeps rustty-render-types a leaf (no new edge to config/renderer) and check_acyclic green; rustty-config tests + C-API + comment-coverage/test-count gates stay green; the full workspace build/test/clippy(-D warnings)/fmt are green. Actually RUN: cargo build --workspace, cargo test -p rustty-config, cargo clippy --workspace --all-targets -- -D warnings, bash .port/tools/gates/check_acyclic.sh, python3 .port/tools/gates/check_test_counts.py — and report the real results.',
]
const verdicts = await parallel(LENSES.map((lens, i) => () =>
  agent(`You are an ADVERSARIAL verifier for GPU R-d Chunk 2 (AlphaBlending hoist + Metal.zig port, uncommitted in the working tree). Your LENS:\n${lens}\n${COMMON}\nRead the changed files (git diff --stat + git status to find them; the Metal port is in crates/rustty-renderer/src/metal/, the hoist touches rustty-config + rustty-render-types), compare against ${REF}/src/renderer/metal/Metal.zig where relevant, and RUN any gate commands your lens calls for. Report the schema: your lens, verdict PASS/CONCERNS/FAIL, and findings (severity blocker/major/minor, file, issue). Only report REAL, evidenced issues; if clean, return an empty findings array with PASS and name the two things you tried hardest to break.`,
    { label: `verify:${i}`, phase: 'Verify', agentType: 'general-purpose', schema: VERDICT_SCHEMA })))

const allFindings = verdicts.filter(Boolean).flatMap(v => (v.findings || []).map(f => ({ ...f, lens: v.lens })))
const blockers = allFindings.filter(f => f.severity === 'blocker')
log(`Verify done: ${verdicts.filter(Boolean).map(v => v.verdict).join(', ')} | ${blockers.length} blockers, ${allFindings.length} total findings`)

return {
  design_summary: designSummary,
  design_critique: critique,
  hoist_report: hoistReport,
  metal_report: metalReport,
  verify: verdicts,
  blockers,
}