export const meta = {
  name: 'gpu-rd-design',
  description: 'Design blueprint for renderer R-d (Metal GPU draw path): generic.zig + Metal.zig + metal/* + shaders, via objc2-metal, with an OFFSCREEN-render golden-image gate. Architect + adversarial verification.',
  phases: [ { title: 'Design' }, { title: 'Verify' } ],
}

const CONTEXT = [
  'Project: rustty, a faithful Zig->Rust port of ghostty. Reference READ-ONLY at /Users/gael/projects/ghostty (Zig 0.15.2); port at /Users/gael/projects/rustty. Rules: /Users/gael/projects/rustty/.port/PORT_RULES.md (R1 test-count contract; R3 byte-faithful every line and comment; R5 never weaken). Gates: check_acyclic.sh, check_test_counts.py, scoped-G3b.',
  '',
  'MILESTONE STATE: the ENTIRE terminal + app + libghostty-C-ABI + CLI is ported and gate-green, AND the port now RUNS A LIVE SHELL (D6 xev event loop real, termio wired, a live /bin/sh round-trips into the terminal Screen). renderer R-a/R-b/R-c are done (headless: cell/cursor/state/shaders/shadertoy/link/message/thread in crates/rustty-renderer/src). rustty-xev is the real kqueue loop. The ONLY thing between "runs a shell" and "shows the shell in a window" is R-d: the GPU draw path.',
  '',
  'THIS IS R-d (the Metal GPU renderer). It is Phase-3 / golden-image-gated by explicit plan (heavy-tier-strategy: "GPU/GTK accept no unit-gate; compile + golden-image / live-session diff"). Reference scope (all 0 reference test{}): src/renderer/generic.zig (3374 loc, the BACKEND-AGNOSTIC renderer core: Renderer, drawFrame, updateFrame, the cell/bg/image vertex upload, the render passes), src/renderer/Metal.zig (458, the Metal backend selection), src/renderer/metal/{Frame,IOSurfaceLayer,Pipeline,RenderPass,Sampler,Target,Texture,api,buffer,shaders}.zig (the Metal API layer), src/renderer/shaders/shaders.metal (the cell/bg/image/cursor MSL shaders) + shadertoy glsl. macOS Metal only (OpenGL.zig = Linux, defer).',
  '',
  'FFI PATH (established, R3-consistent): the crates.io objc2-* framework crates REPLACE ghostty pkg/macos (like the CoreText font path already does — rustty-font-shaper deps objc2-core-foundation/core-graphics/core-text 0.3). R-d uses objc2-metal (MTLDevice/CommandQueue/RenderPipelineState/Buffer/Texture/RenderPassDescriptor) + objc2-quartz-core (CAMetalLayer/IOSurface) + objc2-foundation. The shader pipeline can compile the .metal shaders at build time (metal/metallib) OR reuse the R-b glslang->spirv-cross->MSL path (glslang-sys/spirv-cross-sys are vendored).',
].join('\n')

phase('Design')
const design = await agent(CONTEXT + '\n\n' + [
  'You are the ARCHITECT. Produce a CONCISE blueprint (write to /Users/gael/projects/rustty/.port/phase-2/GPU_RD_BLUEPRINT.md; also return as final text) for porting the Metal GPU draw path into rustty-renderer. Ground EVERY claim by READING generic.zig, Metal.zig, the metal/*.zig files, shaders.metal, the already-ported rustty-renderer (cell.rs/state.rs/thread.rs — the CPU-side vertex structs from R-a), and the objc2-metal crate API (check what MTL types/methods exist). Cover:',
  '1. ACHIEVABLE-NOW vs deferred split (the framing decision). On-screen PRESENTATION needs a CAMetalLayer attached to a live NSView/window = the apprt embedded/GTK host (not yet a running window). But OFFSCREEN rendering (render a terminal frame to an MTLTexture with no window, read back the pixels) IS achievable now and PROVES the renderer produces correct pixels. Recommend: land R-d as "the Metal draw path + an offscreen-render golden-image gate", with on-screen CAMetalLayer present deferred to the windowing-host milestone. Give the honest split (which metal/* files + generic.zig paths are offscreen-portable now vs need a live layer).',
  '2. The Metal FFI mapping: for each metal/*.zig (Frame/Pipeline/RenderPass/Sampler/Target/Texture/api/buffer/shaders), the objc2-metal (+ quartz-core) types/methods it maps to. Flag any ghostty macos-binding call with no objc2 equivalent (a gap). The MTLDevice/CommandQueue/PipelineState/Buffer/Texture/RenderPassDescriptor surface generic.zig drives.',
  '3. The shader pipeline: how shaders.metal (the cell_text/cell_bg/image/cursor MSL) gets compiled + loaded. Options: (a) build.rs invokes the `metal`/`xcrun metal` compiler to a .metallib embedded at build time (closest to ghostty), (b) the R-b glslang->spirv-cross->MSL runtime path, (c) MTLDevice.newLibraryWithSource at runtime. Recommend one; note the shadertoy custom-shader path already uses (b).',
  '4. generic.zig port: what is BACKEND-AGNOSTIC + already-CPU-ported (the Contents/CellText/CellBg vertex assembly from R-a) vs the GPU-specific draw calls (buffer upload, pipeline bind, draw, present). The drawFrame/updateFrame/rebuildCells flow.',
  '5. The renderer.Thread event loop: it was xev-AND-GPU blocked (renderer_seam.rs:381); with xev real, R-d can wire it (like termio Thread, but drives drawFrame + present). Does it land in R-d or defer the present half? The offscreen golden-image can drive drawFrame directly without the thread loop.',
  '6. The GOLDEN-IMAGE GATE (the R-d verification, since 0 unit tests). Precedent: the font sprite tier does pixel-exact PNG known-answer compares (Face render-all-sprites vs res/*.png). Design: render a known terminal state (a few cells of text + bg + cursor) to an offscreen MTLTexture, read back RGBA, compare to a committed reference PNG (generated once, visually verified). How to make it deterministic (fixed font/atlas, fixed colors, fixed size). Where the reference PNG lives + how the gate runs (a macOS-gated #[test] or a check script). This is the R-d contract, replacing the unit-test count.',
  '7. Crate placement + deps + acyclicity: objc2-metal/objc2-quartz-core added to rustty-renderer (leaf-safe? renderer is below apprt/core). The shader-compile build.rs. Confirm no cycle.',
  '8. Chunked execution plan + risks (the offscreen-vs-onscreen honesty; objc2-metal API coverage; shader compile determinism; the golden-image reference generation + review; Metal device availability in CI/headless — does offscreen Metal work without a display? use a headless MTLCreateSystemDefaultDevice).',
  'Be decisive. This is the largest remaining tier and golden-image-gated (different verification model) — the blueprint IS the quality gate.',
].join('\n'), { label: 'architect', phase: 'Design' })

phase('Verify')
const verdict = await agent(CONTEXT + '\n\n' + [
  'You are the ADVERSARIAL REVIEWER. The blueprint is at /Users/gael/projects/rustty/.port/phase-2/GPU_RD_BLUEPRINT.md (read it). Break it against the real reference + objc2-metal + the repo:',
  '1. objc2-metal API coverage: for the metal/*.zig calls the blueprint maps, does objc2-metal ACTUALLY expose them (spot-check MTLDevice/CommandQueue/RenderPipelineState/Buffer/Texture/RenderPassDescriptor/RenderCommandEncoder + newLibraryWithSource/makeBuffer/etc.)? Any mapped call that does not exist in objc2-metal = a gap. Is objc2-quartz-core needed only for the (deferred) CAMetalLayer, or also for offscreen?',
  '2. Offscreen-render feasibility: does an offscreen MTLTexture render + readback ACTUALLY work headless (no window, no CAMetalLayer) on macOS? Does MTLCreateSystemDefaultDevice work in a CI/ssh/headless session, or does it need a GPU/display? If it can fail headless, the golden-image gate is unreliable — flag it and check the blueprint has a mitigation (skip-if-no-device? a software/offscreen path?).',
  '3. Achievable-now honesty: is the offscreen/onscreen split real, or does some "offscreen-portable" path actually pull CAMetalLayer/IOSurface/the present half? Does generic.zig drawFrame reach a present that needs a live layer?',
  '4. Shader compile determinism: is the chosen shader path (metallib build.rs / spirv-cross / newLibraryWithSource) deterministic + does xcrun metal exist on the box? Any non-reproducibility?',
  '5. Golden-image determinism: will the rendered pixels be BYTE-stable across runs/machines (font atlas, antialiasing, GPU rounding)? Pixel-exact compare may be too brittle for GPU output — should it be a tolerance/perceptual compare? Is the reference-PNG generation + review process sound (not a self-fulfilling snapshot)?',
  '6. Cycles + test-count (R-d is 0 zig test{}; the golden-image is rustty-only margin) + anything else that sinks it.',
  'Return numbered findings (CONFIRMED-ISSUE with evidence / FALSE-ALARM / RECOMMENDATION) + verdict: ACCEPT / ACCEPT-WITH-FIXES (list fixes) / REDESIGN. Do NOT edit the blueprint.',
].join('\n'), { label: 'adversary', phase: 'Verify' })

return { design_len: (design || '').length, verdict }
