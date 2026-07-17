# L4 — renderer OpenGL backend (offscreen milestone) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port ghostty's OpenGL graphics-API backend so a second concrete, monomorphic
`Renderer` — parallel to the already-landed `GenericRenderer(Metal)` — renders one terminal
`Screen` (background + text cells + block cursor) OFFSCREEN to a GL framebuffer object and the
pixels are read back and structurally pixel-compared, mirroring the existing Metal offscreen
golden (`generic::macos_impl::tests::golden_renders_cursor_fg_bg`). Headless GL is stood up via
EGL surfaceless (no X11/Wayland/GTK dependency) so the KAT runs in CI. Windowed present through a
real `GtkGLArea` context is explicitly OUT of scope — that is L5's job; L4 only has to prove the
GL draw path produces correct pixels into an FBO it owns.

**Architecture:**

- **Reference oracle:** `src/renderer/OpenGL.zig` (461 loc, the `OpenGL` graphics-API impl,
  parallel to the already-ported `Metal.zig`/`src/renderer/metal_impl.rs`), `src/renderer/opengl/
  *.zig` (~1090 loc: `buffer.zig`/`Frame.zig`/`Pipeline.zig`/`RenderPass.zig`/`Sampler.zig`/
  `shaders.zig`/`Target.zig`/`Texture.zig` — parallel to `src/renderer/metal/*.zig`), the 10 GLSL
  shaders `src/renderer/shaders/glsl/*.glsl` (reused VERBATIM — no MSL/GLSL divergence to write,
  ghostty already emits the same GLSL family for both custom-shader transpile targets), and
  ghostty's own low-level GL wrapper package `pkg/opengl/*.zig` (~1266 loc: `Buffer.zig`/
  `Framebuffer.zig`/`Program.zig`/`Renderbuffer.zig`/`Sampler.zig`/`Shader.zig`/`Texture.zig`/
  `VertexArray.zig`/`draw.zig`/`errors.zig`/`extensions.zig`/`primitives.zig`/`glad.zig`/`c.zig`)
  that `src/renderer/opengl/*.zig` is itself built on (the GL analogue of `objc2-metal` for
  Metal — except ghostty hand-wrote it, so we hand-port it instead of reusing a crates.io FFI
  binding crate). Total ~2800 loc of Zig oracle, plus the shared, graphics-API-agnostic
  `src/renderer/generic.zig` (`Renderer(comptime GraphicsAPI: type)`, 3374 loc) which is the TRUE
  oracle for the CPU cell-fill / uniforms-math / `drawFrame` step sequence — it is the SAME
  function ghostty instantiates for BOTH `Metal` and `OpenGL`, so it is cited throughout instead
  of re-deriving logic from the already-Metal-ported `generic.rs::macos_impl`.

- **Rust-side precedent (mirror, don't touch):** `crates/rustty-renderer/src/metal.rs` +
  `metal/{api,buffer,frame,gpu_abi,pipeline,render_pass,sampler,shaders,target,texture,
  metal_impl}.rs` are the already-landed, golden-gated Metal backend. `generic.rs`'s
  `#[cfg(target_os = "macos")] mod macos_impl { … }` (generic.rs:829-7551) is the already-landed,
  MONOMORPHIC port of `generic.zig`'s `Renderer(Metal)` instantiation (per the ratified
  `.port/phase-2/GPU_RD_TRAIT_DESIGN.md` decision: "port MONOMORPHICALLY … a set of one — no
  `GraphicsAPI` trait hierarchy … OpenGL is Linux-deferred"). That doc explicitly flags the
  monomorphic choice as **reversible** ("extract-trait … lands cleanly when OpenGL actually
  exists to validate the abstraction") — but this plan does **NOT** take that refactor. Per the
  task's R5 constraint (don't touch Metal) we add a **second, sibling, `#[cfg(target_os =
  "linux")]` module** (`linux_impl`, new file `generic/linux_impl.rs`) that is the SAME
  `generic.zig` `Renderer(GraphicsAPI)` instantiated a second time, this time over `OpenGL`
  — copy-adapted from `macos_impl`'s already-verified Rust shape (import swap +
  graphics-API-specific glue swap; the CPU-only logic bodies are textually near-identical because
  `generic.zig` is graphics-API-agnostic there) rather than re-derived from scratch. `macos_impl`
  is read-only reference material for this plan; not one byte of it changes. A future
  `GraphicsApi` trait extraction (unifying `macos_impl`/`linux_impl`) is explicitly OUT OF SCOPE
  — noted as a follow-up in Deferred, consistent with `GPU_RD_TRAIT_DESIGN.md`'s own framing.

- **Milestone scope cut (deliberate, mirrors how the Metal lane itself staged its Chunks 0-4
  before the later "P1 fill" backlog):** the golden only needs background-color cells, foreground
  glyph cells, and a block cursor — i.e. the `bg_color`/`cell_bg`/`cell_text` pipelines and
  `drawFrame`'s `bg_color → cell_bg → cell_text` step sequence (`generic.zig:1592-1673`, no
  `bg_image`, no kitty `images.draw`, no custom-shader post-pipelines, no debug overlay). All 5
  pipelines are still BUILT (`Shaders::init` is ported whole, faithfully — `opengl/shaders.zig:
  103-141`), only 3 are EXERCISED by the milestone's `draw_frame`. Background image / kitty
  graphics / custom shaders / windowed present / GLArea attach are DEFERRED (see bottom).

**Tech Stack / GL loader decision (D-D, resolved):**

- **`gl` crate (raw FFI bindings, `gl_generator`-based, loaded via an explicit
  `gl::load_with(|s| …)` closure)** — NOT `glow`, NOT a bare `epoxy-sys` link. Rationale: ghostty's
  own `pkg/opengl` is a **hand-rolled** thin wrapper (create/bind/parameter/image2D/subImage2D/
  storage/…) over a **raw** glad function-pointer context (`gl.c.GL_*` constants, `gl.c.gl*` raw
  calls) — it is not itself an abstraction library. The highest-fidelity Rust port keeps that same
  two-layer shape: a raw FFI binding layer (`gl` crate ≈ Rust's `glad`) UNDER a hand-ported
  `pkg/opengl` twin (this plan's `opengl::gl::*` module), so every `renderer/opengl/*.zig` leaf
  (`Target.zig`/`Pipeline.zig`/`RenderPass.zig`/…) ports **method-for-method**, not re-authored
  against a foreign vocabulary (`glow`'s `Context` trait has different method names/shapes; using
  it would mean re-inventing the RenderPass/Pipeline logic instead of porting it — violates the
  "never reinvent" constraint). `epoxy` (libepoxy, the GNOME/GTK per-call auto-dispatch loader
  ghostty's own `apprt.gtk => prepareContext(null)` arm implicitly relies on via glad's
  `gladLoaderLoadGLContext` auto-detect path) is the right loader for the **windowed** L5 GLArea
  case (GDK owns context creation/binding; there is no single stable proc-address function to
  hand `gl::load_with` across context switches) — but that is an L5 concern. This plan's `gl::
  load_with` closure resolves proc addresses via **raw EGL** (`eglGetProcAddress`, see below),
  which is sufficient and simpler for an offscreen, self-owned context. Record the two-loader
  split as an explicit ADR here so L5 does not have to re-litigate it: **offscreen (L4) → raw EGL
  proc-address closure; windowed (L5) → `epoxy`.**
- **`khronos-egl` crate, `dynamic` feature (dlopen, no build-time link/pkg-config dependency)**
  for the headless context: `eglGetPlatformDisplay`/`eglGetDisplay` → `eglInitialize` →
  `eglChooseConfig` (config-less / `EGL_SURFACE_TYPE = EGL_PBUFFER_BIT` or `EGL_KHR_surfaceless_
  context` if available) → `eglBindAPI(EGL_OPENGL_API)` → `eglCreateContext` (requesting
  `EGL_CONTEXT_MAJOR_VERSION=4, EGL_CONTEXT_MINOR_VERSION=3, EGL_CONTEXT_OPENGL_PROFILE_MASK=
  EGL_CONTEXT_OPENGL_CORE_PROFILE_BIT`) → `eglMakeCurrent(EGL_NO_SURFACE)` (surfaceless — the
  offscreen FBO/renderbuffer target IS the "surface"). `dynamic` avoids adding a new pkg-config
  `.pc` requirement (this host has `libEGL.so.1`/`libGL.so.1` present but no `.pc` files — verify
  at Task 1). Confirm exact crate versions at implementation time (repo precedent:
  `objc2-quartz-core`'s Cargo.toml comment "CONFIRM via the T3 spike").
- **OPEN RISK, flag prominently:** `glxinfo` on this dev host reports the DEFAULT (legacy/compat)
  context as `OpenGL version string: 2.1 Mesa 26.0.3` over a `virgl` (virtio-gpu → host ANGLE →
  Metal) renderer reporting an underlying `OpenGL 4.1 Metal` capability. ghostty's own
  `MIN_VERSION_MAJOR/MINOR = 4.3` (`OpenGL.zig:36-38`) may not be reachable through this
  particular virtualized/translated GL stack even with an explicit core-profile context request
  (4.1 < 4.3). Task 1's own EGL-context-creation step is the FIRST empirical answer to this; if
  4.3 core is unreachable in this dev/CI sandbox, the golden test must SKIP CLEANLY (mirroring the
  existing "no Metal device → skip" precedent in `metal.rs`/`generic.rs`'s golden tests) rather
  than fail the suite, and the risk is noted as a sandbox/virtualization limitation, not a design
  defect — real (non-virtualized) Linux hosts with Mesa/NVIDIA/AMD drivers routinely expose
  4.5/4.6.

## Global Constraints

- **R1 fidelity:** every leaf wrapper (`opengl::gl::*`, `opengl::{buffer,target,frame,render_pass,
  pipeline,sampler,texture,shaders,gpu_abi}`, `opengl::opengl_impl::OpenGL`) is a faithful,
  cited port of its named `.zig` file — method names, option structs, and behavior match; no
  reinvented abstraction (see the GL-loader ADR above for why `gl` was chosen over `glow`).
  `generic/linux_impl.rs`'s CPU logic (`rebuild_cells`/`add_cell`/`add_cursor`/`update_frame`/
  `draw_frame`) is copy-adapted from the ALREADY-VERIFIED `macos_impl` port (itself R1-checked
  against `generic.zig`) with graphics-API-specific glue swapped — cite BOTH the Rust source
  (`generic.rs::macos_impl` line range) and the ultimate Zig oracle (`generic.zig` line range) per
  step so a reviewer can diff either direction.
- **R3 self-contained:** the new Linux-only deps (`gl`, `khronos-egl`) live under
  `[target.'cfg(target_os = "linux")'.dependencies]` in `rustty-renderer/Cargo.toml`, mirroring
  the existing `[target.'cfg(target_os = "macos")'.dependencies]` block for objc2/Metal. No edge
  to the ghostty tree (reference is read-only oracle only, per repo-wide rule).
  `khronos-egl`'s `dynamic` feature keeps the crate acyclic/link-free (`check_acyclic.sh`,
  `check_leaks.sh` FFI_CRATES list gains `rustty-renderer`'s Linux arm — it is already an FFI
  crate via oniguruma/glslang/wuffs, so this is not a new category, just a new target-arm).
- **R5 gates invariant — Metal is untouched.** Every new file is additive; every touched existing
  file (`lib.rs`, `generic.rs`, `Cargo.toml`) gets an ADDITIVE `#[cfg(target_os = "linux")]` arm
  placed beside (never replacing) the existing `#[cfg(target_os = "macos")]` arm. `cargo build
  -p rustty-renderer` and `cargo test -p rustty-renderer` on macOS must be byte-behavior-identical
  before and after every task (verify via the existing macOS golden suite where a macOS runner is
  available; where it is not, `cargo check --target aarch64-apple-darwin` / careful `#[cfg]` review
  is the fallback gate — document which was used per task).
- **R6 Miri:** the new code is FFI-heavy (raw GL calls, EGL, `unsafe` pointer casts for the
  `Renderer`/`SwapChain`/`FrameState` raw-root pattern inherited from `macos_impl`'s
  `draw_frame_raw(self_ptr: *mut Renderer, …)` shape) — Miri-excluded like the Metal lane; add to
  `.port/phase-2/MIRI_EXCEPTIONS.md` under a new `linux_impl`/`opengl` heading, mirroring the
  existing Metal `mirihar-chunk3b` entry.
- **`opengl::gpu_abi` is a FRESH port, not a copy of `metal::gpu_abi`.** The two graphics APIs'
  `Uniforms`/`CellText`/`CellBg`/`Image`/`BgImage` `extern struct` byte layouts DIFFER: Metal's
  `Uniforms.bools` is an `extern struct` of individually byte-aligned bools (`metal/shaders.zig:
  232-254`) while OpenGL's is a bit-packed `packed struct(u32)` (`opengl/shaders.zig:200-222`);
  Metal's `padding_extend` is `align(1)` with no explicit padding field, OpenGL's is `packed
  struct(u32)` with an explicit `_padding: u28`. Port `opengl::gpu_abi` byte-offset-by-byte from
  `opengl/shaders.zig:163-300` directly; do NOT assume/reuse Metal's field order.
- **Milestone-scope cut is enforced, not aspirational:** tasks port ONLY what `bg_color`/
  `cell_bg`/`cell_text` + a plain (non-custom-shader, non-bg-image, non-kitty, non-overlay) offscreen
  frame needs. Anything else in `OpenGL.zig`/`opengl/*.zig`/`linux_impl`'s CPU-fill surface that
  isn't reached by that path is EITHER ported as an inert stub (mirroring `macos_impl`'s existing
  `unimplemented!()`-for-inspector-overlay precedent, generic.rs comment at ~43-46) OR left absent
  with a DEFERRALS.md entry — never half-implemented silently.

## File structure

| File | Responsibility |
|---|---|
| `crates/rustty-renderer/src/opengl/gl/sys.rs` (**create**) | Raw `gl` crate loader glue (`gl::load_with` + the EGL `eglGetProcAddress` closure); the GL-analogue of `objc2`'s raw layer. Port of `pkg/opengl/glad.zig` + `c.zig`'s intent (not literal — ghostty's own thin glad wrapper vs. crates.io `gl`), with the offscreen-EGL context-creation helper (`OffscreenContext::init()` — RUSTTY-ONLY, no ghostty analogue, needed because ghostty never stands up its own headless context; it always runs inside GLFW/GTK/embedded). |
| `crates/rustty-renderer/src/opengl/gl/{errors,primitives,draw,extensions}.rs` (**create**) | Port of `pkg/opengl/{errors,primitives,draw,extensions}.zig` — `glGetError` checking, `gl.Primitive` enum, `drawArraysInstanced`/`clear`/`clearColor`/`enable`/`disable`/`finish`/`flush` free functions, extension iteration. |
| `crates/rustty-renderer/src/opengl/gl/{buffer,vertex_array,shader,program,texture,sampler,framebuffer,renderbuffer}.rs` (**create**) | Port of `pkg/opengl/{Buffer,VertexArray,Shader,Program,Texture,Sampler,Framebuffer,Renderbuffer}.zig` — the low-level create/bind/destroy GL-object wrapper leaves (the GL analogue of `metal/api.rs`'s objc2-metal re-exports, except hand-ported since ghostty hand-wrote them too). |
| `crates/rustty-renderer/src/opengl/{buffer,texture,sampler}.rs` (**create**) | Port of `src/renderer/opengl/{buffer,Texture,Sampler}.zig` — the renderer-facing `Options`-driven leaves layered on the `gl/` low-level wrappers (parallel to `metal/{buffer,texture,sampler}.rs`). |
| `crates/rustty-renderer/src/opengl/{target,pipeline,render_pass,frame}.rs` (**create**) | Port of `src/renderer/opengl/{Target,Pipeline,RenderPass,Frame}.zig` — the draw-plumbing leaves (renderbuffer-backed FBO target, program+VAO pipeline, step/draw render pass, frame begin/complete=sync `gl.finish()`). Parallel to `metal/{target,pipeline,render_pass,frame}.rs`. `target.rs` additionally gets a RUSTTY-ONLY `read_rgba`/`read_bgra` `glReadPixels` golden-support method (no ghostty analogue — mirrors the `metal/target.rs::read_bgra()` precedent, itself already flagged as rustty-only). |
| `crates/rustty-renderer/src/opengl/gpu_abi.rs` (**create**) | Fresh, byte-faithful port of `opengl/shaders.zig:163-300`'s `Uniforms`/`CellText`/`CellBg`/`Image`/`BgImage` extern structs. NOT copied from `metal/gpu_abi.rs` (see Global Constraints). |
| `crates/rustty-renderer/src/opengl/shaders.rs` (**create**) | Port of `opengl/shaders.zig` (376 loc) — the `pipeline_descs` table (5 pipelines) → `Shaders::{init,deinit}`, `initPostPipelines`/`initPostPipeline` (built, milestone-inert). GLSL `#include` resolution happens at BUILD time (see `build.rs` row) rather than Zig's comptime `processIncludes`, functionally equivalent (resolved once into a static string, embedded). |
| `crates/rustty-renderer/src/shaders/glsl/*.glsl` (**create, verbatim copy**) | The 10 GLSL shaders copied byte-for-byte from `src/renderer/shaders/glsl/*.glsl` (bg_color.f, bg_image.{f,v}, cell_bg.f, cell_text.{f,v}, common.glsl, full_screen.v, image.{f,v}). |
| `crates/rustty-renderer/build.rs` (**modify**) | ADD a Linux-only step (additive, existing macOS metallib step untouched): resolve each `.glsl` file's `#include "…"` directives (mirrors `opengl/shaders.zig:341-376 processIncludes`, ported to build-time Rust) and write the resolved sources to `$OUT_DIR/*.glsl.resolved` for `include_str!`. |
| `crates/rustty-renderer/src/opengl/opengl_impl.rs` (**create**) | Port of `OpenGL.zig` (461 loc) — the `OpenGL` graphics-API impl: fields, `init`/`init_offscreen`(RUSTTY-ONLY, mirrors `metal_impl.rs::init_offscreen` precedent)/`deinit`, `prepare_context`+`gl_debug_message_callback`+the 4.3 version gate, `init_target`/`present`(deferred stub)/`present_last_target`(deferred stub), `buffer_options`/`texture_options`/`sampler_options`/`image_texture_options`, `init_atlas_texture`, `begin_frame`, `draw_frame_start`/`draw_frame_end`(no-ops), `surface_size`(offscreen-stub). |
| `crates/rustty-renderer/src/opengl.rs` (**create**) | `pub mod gl; pub mod buffer; … pub use opengl_impl::OpenGL;` — mirrors `metal.rs`'s module-index shape. `#[cfg(target_os = "linux")]`-gated in `lib.rs`. |
| `crates/rustty-renderer/src/generic/linux_impl.rs` (**create**) | The SECOND `generic.zig` `Renderer(GraphicsAPI)` instantiation, over `OpenGL`. Struct decls (`Renderer`/`SwapChain`/`FrameState`/`CustomShaderState`) + `init`(offscreen-only path)/`deinit` + the milestone-scoped `rebuild_cells`/`add_cell`/`add_cursor`/`update_frame`/`draw_frame`(bg_color→cell_bg→cell_text only)/`frame_completed`/`sync_atlas_texture`/`init_shaders`/`deinit_shaders`, copy-adapted from `generic.rs::macos_impl` (READ-ONLY reference) against the `generic.zig` oracle. Ends with the offscreen golden test module (`golden_renderer`/`golden_renders_cursor_fg_bg` OpenGL twins). |
| `crates/rustty-renderer/src/generic.rs` (**modify, additive only**) | ADD `#[cfg(target_os = "linux")] mod linux_impl;` beside the existing `#[cfg(target_os = "macos")] mod macos_impl { … }` (generic.rs:829); ADD `#[cfg(target_os = "linux")] pub use linux_impl::*;` beside the existing `#[cfg(target_os = "macos")] pub use macos_impl::*;` (generic.rs:7554-7555). Zero bytes of `macos_impl` change. |
| `crates/rustty-renderer/src/lib.rs` (**modify, additive only**) | ADD `#[cfg(target_os = "linux")] pub mod opengl;` (beside `pub mod metal;`); ADD `#[cfg(target_os = "linux")] pub use generic::Renderer;` (beside the macOS arm at lib.rs:134-135). |
| `crates/rustty-renderer/Cargo.toml` (**modify, additive only**) | ADD `[target.'cfg(target_os = "linux")'.dependencies]`: `gl`, `khronos-egl` (dynamic feature). |
| `.port/phase-2/GPU_RD_GL_CHUNK{0-4}_DESIGN.md` or a single `GPU_RD_GL_DESIGN.md` (**create, optional but recommended**) | Mirrors the `GPU_RD_CHUNK*_DESIGN.md` precedent for the Metal lane — records the milestone-scope cut, the loader ADR, and the `gpu_abi` divergence note in the repo's existing design-doc convention. |
| `.port/phase-2/DEFERRALS.md` (**modify**) | Record: windowed present / GLArea attach (the "GL-ATTACH" entry, mirroring the historical "W-ATTACH" entry now RESOLVED for Metal); bg-image/kitty-images/custom-shaders/debug-overlay for OpenGL (mirrors the Metal "P1 fill" backlog, now applied a second time); the `GraphicsApi` trait-extraction non-decision (cite `GPU_RD_TRAIT_DESIGN.md`, note it remains open). |
| `.port/phase-2/MIRI_EXCEPTIONS.md` (**modify**) | Add the `linux_impl`/`opengl` FFI-exclusion entry. |

---

### Task 1: Headless EGL context + raw `gl` loader — prove GL 4.3 core stands up offscreen

**Files:**
- Create: `crates/rustty-renderer/src/opengl/gl/sys.rs`
- Modify: `crates/rustty-renderer/Cargo.toml` (Linux target deps: `gl`, `khronos-egl` dynamic)

**Interfaces:**
- Consumes: `khronos-egl` (dynamic-loaded `libEGL.so.1`), the `gl` crate's `gl::load_with`.
- Produces: `opengl::gl::sys::OffscreenContext` — RUSTTY-ONLY (no ghostty analogue): `init() ->
  Result<OffscreenContext, Error>` (EGL display/init/config/context/`MakeCurrent(NO_SURFACE)` per
  the Architecture section's sequence, requesting core 4.3), `Drop` (destroy context, terminate
  display). A module-level `pub fn load_gl(egl: &OffscreenContext)` calling `gl::load_with(|s|
  egl.get_proc_address(s))`, mirroring `OpenGL.zig::prepareContext`'s `gl.glad.load(getProcAddress)`
  (`OpenGL.zig:133-159`) — including the version check (`MIN_VERSION_MAJOR/MINOR`, `OpenGL.zig:
  36-38,141-149`) and `GL_DEBUG_OUTPUT` + `glDebugMessageCallback` registration
  (`OpenGL.zig:72-130,151-155`) ported verbatim into a free fn `prepare_context` reused by Task 8.

- [ ] **Step 1: Write the failing test** — `#[cfg(target_os = "linux")] #[test] fn
      offscreen_egl_context_reaches_gl43_core()`: create an `OffscreenContext`, call `load_gl`,
      assert `glGetString(GL_VERSION)` parses to `>= (4, 3)`. Per the Architecture "OPEN RISK", if
      context creation or the version check fails, the test must **skip cleanly** (print + early
      `return`, not `panic!`) exactly like `metal.rs`'s "no Metal device" skip — write the skip
      branch INTO this first test, not as a later fixup.
- [ ] **Step 2: Run to verify it fails** — `cargo test -p rustty-renderer --target
      aarch64-unknown-linux-gnu offscreen_egl_context_reaches_gl43_core` → FAIL (module doesn't
      exist yet).
- [ ] **Step 3: Implement** — `OffscreenContext::init` (raw `khronos_egl` calls per the sequence
      above); `load_gl`/`prepare_context` ported from `OpenGL.zig:132-159` (the debug callback body
      `OpenGL.zig:72-130` ports verbatim: same severity→log-level mapping, same source/type string
      tables — Rust `log`/`tracing` in place of `std.log.scoped`).
- [ ] **Step 4: Run to verify it passes (or skips cleanly)** — same command → PASS or a clean
      "skipping: …" stderr line + early return, never a panic. Record the observed GL
      version/renderer string in the task's commit message (empirical answer to the OPEN RISK).
- [ ] **Step 5: Commit** — `git commit -m "rustty-renderer: headless EGL context + gl loader (L4 T1)"`

---

### Task 2: `opengl::gl` low-level leaves batch 1 — `Buffer`/`VertexArray`/errors/primitives/draw

**Files:**
- Create: `crates/rustty-renderer/src/opengl/gl/{buffer,vertex_array,errors,primitives,draw}.rs`

**Interfaces:**
- Consumes: Task 1's loaded `gl::*` raw calls, `OffscreenContext` (test-only, via a shared
  `#[cfg(test)] mod test_support` helper that stands up one context per test — mirrors
  `metal.rs`'s `MTLCreateSystemDefaultDevice()`-per-test pattern, not a shared global, to keep
  tests independent).
- Produces: `opengl::gl::errors::{Error, get_error}` (port `pkg/opengl/errors.zig`, 33 loc —
  `glGetError` → `Result`), `opengl::gl::primitives::Primitive` (port `primitives.zig`, 18 loc —
  `triangle`/`triangle_strip`/… enum → `GL_TRIANGLES`/…), `opengl::gl::draw::{clear, clear_color,
  enable, disable, finish, flush, draw_arrays_instanced}` (port `draw.zig`, 94 loc), `opengl::gl::
  buffer::Buffer` (port `Buffer.zig`, 245 loc: `create`/`destroy`/`bind`/`Binding::{unbind,
  set_data, set_data_null_manual, set_sub_data, bind_base}`), `opengl::gl::vertex_array::
  VertexArray` (port `VertexArray.zig`, 116 loc: `create`/`destroy`/`bind`/`Binding::{unbind,
  enable_attrib_array, attribute_binding, binding_divisor, attribute_i_format, attribute_format,
  attribute_l_format, bind_vertex_buffer}`).

- [ ] **Step 1: Write the failing test** — a `gl_buffer_vertex_array_smoke` test (RUSTTY-ONLY, no
      ghostty `test{}` block over these — same "smoke, not parity" framing as `metal.rs`'s
      `creates_buffer_texture_sampler_without_panic`): create an `OffscreenContext`, create+bind+
      `set_data`+destroy a `Buffer`, create+bind+`enable_attrib_array`+destroy a `VertexArray`, no
      panics, `get_error()` clean after each step. Skip cleanly if Task 1's context fails.
- [ ] **Step 2: Run to verify it fails** — FAIL (types don't exist).
- [ ] **Step 3: Implement** — port the 5 files, preserving method names/error shapes (`Result<_,
      gl::errors::Error>` in place of Zig's `!T`).
- [ ] **Step 4: Run to verify it passes** — PASS/skips cleanly.
- [ ] **Step 5: Commit** — `git commit -m "rustty-renderer: opengl::gl buffer/vertex_array/errors/draw leaves (L4 T2)"`

---

### Task 3: `opengl::gl` low-level leaves batch 2 — `Shader`/`Program`

**Files:**
- Create: `crates/rustty-renderer/src/opengl/gl/{shader,program}.rs`

**Interfaces:**
- Consumes: Task 2's `gl::errors`.
- Produces: `opengl::gl::shader::Shader` (port `Shader.zig`, 56 loc — `create(kind)`/`compile`/
  `destroy`/compile-log-on-failure), `opengl::gl::program::Program` (port `Program.zig`, 137 loc —
  `create_vf(vertex_src, fragment_src)` [compiles+attaches+links both stages, link-log-on-failure,
  detaches+deletes the shader objects after linking per GL best practice IF ghostty does so —
  verify against `Program.zig` and follow it exactly], `use_()`/`Binding::unbind`, `destroy`).

- [ ] **Step 1: Write the failing test** — `gl_program_compiles_trivial_shader_pair`: compile a
      minimal inline `#version 430 core` vertex+fragment pair (NOT yet the real GLSL assets — those
      land in Task 7) via `Program::create_vf`, assert success + `use_()`/`unbind()` round-trip.
      Skip cleanly per the established pattern.
- [ ] **Step 2: Run to verify it fails** — FAIL.
- [ ] **Step 3: Implement** — port `Shader.zig`/`Program.zig` verbatim (shader-compile/link
      error-log extraction via `glGetShaderInfoLog`/`glGetProgramInfoLog`, matching ghostty's log
      call sites).
- [ ] **Step 4: Run to verify it passes** — PASS/skips.
- [ ] **Step 5: Commit** — `git commit -m "rustty-renderer: opengl::gl shader/program leaves (L4 T3)"`

---

### Task 4: `opengl::gl` low-level leaves batch 3 — `Texture`/`Sampler`/`Framebuffer`/`Renderbuffer`/`extensions`

**Files:**
- Create: `crates/rustty-renderer/src/opengl/gl/{texture,sampler,framebuffer,renderbuffer,extensions}.rs`

**Interfaces:**
- Produces: `opengl::gl::texture::Texture` (port `Texture.zig`, 221 loc: `create`/`bind`/
  `Binding::{unbind, parameter, image2D, subImage2D, active(unit)}`; `Format`/`InternalFormat`/
  `Target`/`MinFilter`/`MagFilter`/`Wrap` enums), `opengl::gl::sampler::Sampler` (port
  `Sampler.zig`, 43 loc), `opengl::gl::framebuffer::Framebuffer` (port `Framebuffer.zig`, 116 loc:
  `create`/`bind(.framebuffer)`/`Binding::{unbind, renderbuffer, texture2D}`), `opengl::gl::
  renderbuffer::Renderbuffer` (port `Renderbuffer.zig`, 56 loc: `create`/`bind`/`Binding::{unbind,
  storage}`), `opengl::gl::extensions` (port `extensions.zig`, 32 loc — extension-string iterator;
  built but unused by the milestone, kept for fidelity/future use per the "port whole files"
  convention already used for e.g. Metal's `presentLastTarget`).

- [ ] **Step 1: Write the failing test** — `gl_texture_framebuffer_renderbuffer_smoke`: create a
      2x2 RGBA texture with data (mirrors `metal.rs`'s texture smoke test shape), a renderbuffer +
      framebuffer, attach renderbuffer to `COLOR_ATTACHMENT0`, assert `glCheckFramebufferStatus ==
      COMPLETE`. Skip cleanly per pattern.
- [ ] **Step 2: Run to verify it fails** — FAIL.
- [ ] **Step 3: Implement** — port the 5 files verbatim.
- [ ] **Step 4: Run to verify it passes** — PASS/skips. This closes out the low-level `opengl::gl`
      wrapper package (the GL analogue of `metal/api.rs`) — ~1266 loc oracle fully ported.
- [ ] **Step 5: Commit** — `git commit -m "rustty-renderer: opengl::gl texture/sampler/framebuffer/renderbuffer leaves (L4 T4)"`

---

### Task 5: Renderer-facing leaves — `opengl::{buffer,texture,sampler}`

**Files:**
- Create: `crates/rustty-renderer/src/opengl/{buffer,texture,sampler}.rs`

**Interfaces:**
- Consumes: Tasks 2-4's `opengl::gl::*`.
- Produces (parallel to `metal/{buffer,texture,sampler}.rs`): `opengl::buffer::{Options, Buffer<T>}`
  (port `src/renderer/opengl/buffer.zig`, 126 loc: `init`/`init_fill`/`deinit`/`sync`/
  `sync_from_array_lists` — the reallocate-on-grow semantics MUST match exactly, this is the same
  buffer-growth contract `rebuild_cells`/`update_frame` (Task 10) depend on, already proven correct
  for Metal's twin), `opengl::texture::{Options, Texture}` (port `src/renderer/opengl/Texture.zig`,
  104 loc: `init`/`deinit`/`replace_region`), `opengl::sampler::{Options, Sampler}` (port
  `src/renderer/opengl/Sampler.zig`, 45 loc).

- [ ] **Step 1: Write the failing test** — `opengl_buffer_grow_and_sync` (mirrors ghostty's
      `Buffer.sync` grow-path — no direct `test{}` in the reference for this leaf either, same
      "rustty smoke, not reference parity" framing): `Buffer::<u32>::init(4 elems)`, `sync(&[6
      elems])` → asserts the doubled-capacity reallocation happened (same shape as the Metal
      `buf.sync` grow-path smoke test in `metal.rs`); `opengl_texture_replace_region_smoke`.
- [ ] **Step 2: Run to verify it fails** — FAIL.
- [ ] **Step 3: Implement** — port the 3 files, layering `Options` over Tasks 2-4's raw leaves.
- [ ] **Step 4: Run to verify it passes** — PASS/skips.
- [ ] **Step 5: Commit** — `git commit -m "rustty-renderer: opengl buffer/texture/sampler leaves (L4 T5)"`

---

### Task 6: Draw-plumbing leaves — `opengl::{target,pipeline,render_pass,frame}`

**Files:**
- Create: `crates/rustty-renderer/src/opengl/{target,pipeline,render_pass,frame}.rs`

**Interfaces:**
- Consumes: Task 5's leaves.
- Produces (parallel to `metal/{target,pipeline,render_pass,frame}.rs`): `opengl::target::{Options,
  Target}` (port `Target.zig`, 60 loc: renderbuffer-backed FBO `init`/`deinit`) **+ a RUSTTY-ONLY
  `read_rgba(&self) -> Vec<u8>` / `read_bgra(&self) -> Vec<u8>`** (`glBindFramebuffer(READ,
  fbo)` + `glReadPixels` into a tight `w*h*4` buffer — the golden-support readback the milestone's
  Task 11 test needs; document it exactly like `metal/target.rs::read_bgra()` was documented: "no
  reference `test{}` drives this — it exists for the port's own golden gate"), `opengl::pipeline::
  {Options, Pipeline}` (port `Pipeline.zig`, 164 loc: `init(VertexAttributes, opts)` incl. the
  comptime `autoAttribute` reflection (`Pipeline.zig:76-164`) — ported as a Rust fn taking an
  explicit `&[AttribDesc]` table per vertex-struct type, since Rust has no field-reflection;
  hand-write the `AttribDesc` tables for `CellText`/`Image`/`BgImage` from `opengl/shaders.zig`'s
  struct field lists, mirroring how Metal's `pipeline.rs` already hand-wrote its own vertex
  descriptor tables from the same struct shapes — cite that file as the sibling precedent for
  "how a comptime reflection became a hand-written table" if it exists, else derive fresh from
  `opengl/shaders.zig:234-268` field order), `opengl::render_pass::{Options, Step, Draw, RenderPass}`
  (port `RenderPass.zig`, 144 loc: `begin`/`step`/`complete` — the uniform-buffer-at-binding-1
  convention (`RenderPass.zig:93-96`, "align with Metal") and the blend-func
  `GL_ONE, GL_ONE_MINUS_SRC_ALPHA` (`RenderPass.zig:124-129`) are load-bearing pixel-correctness
  details, port verbatim), `opengl::frame::{Options, Frame}` (port `Frame.zig`, 71 loc: `begin`/
  `render_pass`/`complete(sync)` — GL's `complete` ALWAYS blocks via `gl.finish()` regardless of
  the `sync` param, per `Frame.zig:54-57`'s comment; the health check via `gl.errors.getError()`
  then `renderer.frame_completed(health)`).

- [ ] **Step 1: Write the failing test** — `opengl_draw_plumbing_smoke` (mirrors `metal.rs`'s
      `draw_plumbing_smoke` shape exactly): `Target::init(4x4)`, a trivial `Pipeline` compiled from
      an inline clear-only shader pair, a `RenderPass` with a clear-color attachment + `step()`
      that draws nothing (`instance_count: 0` short-circuits per `RenderPass.zig:63`, so this can
      literally test the clear-only path first), `complete()`, then `read_rgba()` and assert the
      `4*4*4`-byte buffer equals the clear color. Skip cleanly per pattern.
- [ ] **Step 2: Run to verify it fails** — FAIL.
- [ ] **Step 3: Implement** — port the 4 files + the `read_rgba`/`read_bgra` addition.
- [ ] **Step 4: Run to verify it passes** — PASS/skips. This closes the draw-plumbing leaf set —
      everything needed to issue ONE clear+draw call and read pixels back now exists.
- [ ] **Step 5: Commit** — `git commit -m "rustty-renderer: opengl target/pipeline/render_pass/frame + golden readback (L4 T6)"`

---

### Task 7: GLSL shader assets + `opengl::gpu_abi` + `opengl::shaders::Shaders`

**Files:**
- Create: `crates/rustty-renderer/src/shaders/glsl/{bg_color.f,bg_image.f,bg_image.v,cell_bg.f,
  cell_text.f,cell_text.v,common,full_screen.v,image.f,image.v}.glsl` (verbatim copy)
- Modify: `crates/rustty-renderer/build.rs` (Linux `#include` pre-resolution step)
- Create: `crates/rustty-renderer/src/opengl/gpu_abi.rs`, `crates/rustty-renderer/src/opengl/shaders.rs`

**Interfaces:**
- Consumes: Task 6's `opengl::pipeline::Pipeline`.
- Produces: the 10 `.glsl` files byte-identical to the reference (`diff -q` in the test, not just
  "copied by hand"); `build.rs`'s new step ports `processIncludes` (`opengl/shaders.zig:352-376`)
  at build time — walk each `.glsl` file, inline `#include "…"` (only `common.glsl` is included,
  by `cell_text.f.glsl`/`cell_text.v.glsl` etc. — confirm exact include graph by grep), write
  resolved output to `$OUT_DIR/glsl/*.glsl`, embedded via `include_str!(concat!(env!("OUT_DIR"),
  "/glsl/….glsl"))` in `shaders.rs` (replaces Zig's comptime `@embedFile`+`processIncludes`);
  `opengl::gpu_abi::{Uniforms, CellText, CellBg, Image, BgImage}` (fresh port per the Global
  Constraints note, `#[repr(C)]` extern-struct-equivalent with explicit padding fields matching
  `opengl/shaders.zig:163-300` bit-for-bit — a `size_of` assert test per struct mirrors the
  reference's `test { try std.testing.expectEqual(32, @sizeOf(CellText)); }`,
  `opengl/shaders.zig:252-256`); `opengl::shaders::Shaders` (port `opengl/shaders.zig:64-160`: the
  `pipeline_descs` table → 5 named `Pipeline` fields built via `PipelineCollection`'s Rust
  equivalent — a plain struct with 5 named fields, since Rust has no comptime struct-type
  generation either; `Shaders::init`/`deinit`, `init_post_pipelines`/`init_post_pipeline` ported
  whole but milestone-inert (called with an empty custom-shader list)).

- [ ] **Step 1: Write the failing tests** — `glsl_assets_match_reference_byte_for_byte` (a
      scratch-relative `diff`/hash check against the pinned ghostty tree's `src/renderer/shaders/
      glsl/*.glsl` — same spirit as any other "verbatim reuse" R1 check elsewhere in this repo);
      `gpu_abi_struct_sizes` (the 5 `size_of` asserts, e.g. `CellText == 32` bytes); `opengl_shaders_
      init_builds_five_pipelines`: `Shaders::init(&[])` succeeds, all 5 named pipeline fields
      populated, `post_pipelines.is_empty()`. Skip cleanly on GL-context failure.
- [ ] **Step 2: Run to verify they fail** — FAIL.
- [ ] **Step 3: Implement** — copy the 10 files; write the build.rs include-resolver; port
      `gpu_abi.rs`/`shaders.rs`.
- [ ] **Step 4: Run to verify they pass** — PASS/skips (the byte-diff test never skips — it needs
      no GL context, only the pinned reference tree, so it MUST run everywhere and is the strongest
      per-task R1 gate in this plan).
- [ ] **Step 5: Commit** — `git commit -m "rustty-renderer: verbatim GLSL assets + opengl gpu_abi/shaders (L4 T7)"`

---

### Task 8: `opengl::opengl_impl::OpenGL` — the graphics-API impl

**Files:**
- Create: `crates/rustty-renderer/src/opengl/opengl_impl.rs`, `crates/rustty-renderer/src/opengl.rs`
- Modify: `crates/rustty-renderer/src/lib.rs` (`#[cfg(target_os = "linux")] pub mod opengl;`)

**Interfaces:**
- Consumes: Tasks 1-7.
- Produces (parallel to `metal/metal_impl.rs`, port of `OpenGL.zig`): `OpenGL::{alloc, blending,
  last_target}` fields (`OpenGL.zig:40-46`); `OpenGL::init_offscreen(blending, width, height) ->
  Result<OpenGL, Error>` — RUSTTY-ONLY (mirrors `metal_impl.rs::init_offscreen`'s precedent: the
  reference `init` needs a live `apprt.Surface`/view which doesn't exist headless, so a
  test-and-milestone-only constructor stands up Task 1's `OffscreenContext` + calls `prepare_
  context` + returns `OpenGL` with no windowing state); `deinit` (`OpenGL.zig:58-60`); `init_target`
  (`OpenGL.zig:289-296`, `Target::init` with `internal_format` chosen by `blending.is_linear()`);
  `present`/`present_last_target` — **DEFERRED STUB** for this milestone (offscreen has no default
  framebuffer to blit into; `present`'s real body — `OpenGL.zig:298-336`, the
  `GL_FRAMEBUFFER_SRGB`-disable + `glBlitFramebuffer` dance — is ported VERBATIM but the milestone
  test path never calls it, since the golden reads `Target::read_rgba()` directly; document this as
  the "GL-ATTACH" deferred item, parallel to the historical Metal "W-ATTACH" entry, now resolved
  for Metal and freshly opened for OpenGL); `buffer_options`/`texture_options`/`sampler_options`/
  `image_texture_options` (`OpenGL.zig:338-419`, verbatim factory bodies); `init_atlas_texture`
  (`OpenGL.zig:421-448`); `begin_frame` (`OpenGL.zig:450-461`); `draw_frame_start`/`draw_frame_end`
  (`OpenGL.zig:252-264`, no-ops — GL has no per-frame autorelease-pool equivalent); `surface_size`
  — **OFFSCREEN STUB**: returns the `Target`'s own `(width, height)` rather than querying
  `GL_VIEWPORT` (`OpenGL.zig:278-287`'s real body is ported too, for when L5 wires a live viewport,
  but the offscreen ctor path doesn't reach it).

- [ ] **Step 1: Write the failing test** — `opengl_init_offscreen_then_deinit_round_trips` (mirrors
      the Metal `test_renderer`/`windowed_lifecycle` skip-cleanly precedent): `OpenGL::init_
      offscreen(AlphaBlending::Native, 256, 256)`, assert fields, `init_shaders(&[])`, `init_target
      (10, 10)`, `deinit()` — no panic/leak. Skip cleanly on GL-context failure.
- [ ] **Step 2: Run to verify it fails** — FAIL.
- [ ] **Step 3: Implement** — port `OpenGL.zig` per the Interfaces list; wire `opengl.rs`'s module
      index; add the `lib.rs` Linux arm.
- [ ] **Step 4: Run to verify it passes** — PASS/skips; `cargo build -p rustty-renderer --target
      aarch64-unknown-linux-gnu` compiles the whole `opengl` module tree end to end for the first
      time.
- [ ] **Step 5: Commit** — `git commit -m "rustty-renderer: OpenGL graphics-API impl (L4 T8)"`

---

### Task 9: `generic/linux_impl` skeleton — `Renderer`/`SwapChain`/`FrameState`/`CustomShaderState` + `init`/`deinit`

**Files:**
- Create: `crates/rustty-renderer/src/generic/linux_impl.rs`
- Modify: `crates/rustty-renderer/src/generic.rs` (additive `mod`/`pub use` per the File-structure
  table; **do not touch `macos_impl`**)

**Interfaces:**
- Consumes: Task 8's `crate::opengl::OpenGL`; the graphics-API-agnostic module-level items already
  in `generic.rs` OUTSIDE `macos_impl` (`Color`, `TerminalColor`, `Path`, `RepeatablePath`,
  `DerivedConfig`, `Options`, `AtomicHealth` — these are NOT duplicated, `linux_impl` imports them
  via `use super::{…}` exactly like `macos_impl` does at generic.rs:878).
- Produces (copy-adapted from `generic.rs::macos_impl`'s struct decls, oracle = `generic.zig`):
  `linux_impl::Renderer { api: OpenGL, swap_chain: SwapChain, shaders: opengl::shaders::Shaders, …
  }` (same field SET as `macos_impl::Renderer`, `crate::metal::*` types swapped for
  `crate::opengl::*`); `linux_impl::SwapChain` (`swap_chain_count = OpenGL::swap_chain_count == 1`,
  per `OpenGL.zig:30-32`'s doc comment "OpenGL's frame completion is always sync … no need for
  multi-buffering" — this means `SwapChain::init`/`next_frame`/`release_frame`'s semaphore-
  backpressure logic degenerates to the trivial 1-buffer case; port it anyway rather than special-
  casing, so the code stays parallel to Metal's 3-buffer version and any later buffer-count change
  is a one-constant edit); `linux_impl::FrameState`/`CustomShaderState` (struct decls only this
  task; `resize`/`swap` bodies land with the fields they touch in Tasks 10-11); `Renderer::{init,
  deinit}` — port ONLY the offscreen constructor path (mirrors `macos_impl::Renderer::init`'s
  shape but `attach_view`/windowing hookup is the "GL-ATTACH" deferred stub, matching Task 8's
  `present` deferral).

- [ ] **Step 1: Write the failing test** — `linux_opengl_renderer_init_then_deinit_round_trips`
      (mirrors Task 8's test one layer up: build a real font grid — reuse the SAME `test_font_grid_
      and_size` helper `macos_impl`'s tests already define, either by moving it to a
      graphics-API-agnostic location both modules can `use`, OR duplicating the small helper per the
      established test-helper-duplication precedent (`opts(cols, rows)` is already duplicated
      cell.rs/cursor.rs/generic.rs per generic.rs:6064-6066's own comment) — prefer duplication to
      avoid coupling `linux_impl` to `macos_impl`'s test internals), `Renderer::init(Global, opts)`,
      `deinit()`. Skip cleanly on GL-context failure.
- [ ] **Step 2: Run to verify it fails** — FAIL (module doesn't exist).
- [ ] **Step 3: Implement** — port the struct decls + `init`/`deinit` from `macos_impl`'s
      corresponding span (generic.rs, locate via `struct Renderer`/`impl Renderer` grep inside
      `macos_impl`) against `generic.zig`'s real `Renderer`/`init`/`deinit` (the struct-literal
      field list, `generic.zig` — locate via the same names) as the ultimate oracle; wire
      `generic.rs`'s additive `mod`/`pub use`.
- [ ] **Step 4: Run to verify it passes** — PASS/skips.
- [ ] **Step 5: Commit** — `git commit -m "rustty-renderer: linux_impl Renderer/SwapChain skeleton + init/deinit (L4 T9)"`

---

### Task 10: CPU cell-fill + uniforms — `rebuild_cells`/`add_cell`/`add_cursor`/`update_frame` (milestone-scoped)

**Files:**
- Modify: `crates/rustty-renderer/src/generic/linux_impl.rs`

**Interfaces:**
- Consumes: Task 9's `Renderer`/`FrameState`; `crate::shaders::{CellText, CellBg, Uniforms, …}`
  (the graphics-API-agnostic ergonomic CPU structs, ALREADY SHARED — not duplicated, both backends
  convert these to their own `gpu_abi` types at the sync boundary, exactly as Metal does).
- Produces: `Renderer::rebuild_cells`/`add_cell`/`add_cursor` scoped to bg+text+cursor cells only
  (no image/kitty placement calls — `generic.zig:2307-…`'s image-placement branches are ELIDED,
  documented inert per the milestone-scope-cut constraint, not silently dropped);
  `Renderer::update_frame`'s uniforms math (`generic.zig:1123-…`) writing `opengl::gpu_abi::
  UniformsGpu` (Task 7) instead of Metal's `UniformsGpu`. Copy-adapt from `macos_impl`'s already-
  ported `rebuild_cells`/`update_frame` (generic.rs, locate via `fn rebuild_cells`/`fn
  update_frame` grep — noted at generic.rs:2835/4424 in the pre-task audit) — the bodies are
  expected to be near-textually-identical modulo the `crate::metal::*` → `crate::opengl::*` import
  swap, since neither function touches `self.api` (verified during research: `self.api.` call
  sites in `macos_impl` are limited to `init_shaders`/`attach_view`/`blending`-field-write/
  `init_atlas_texture`/frame-start-end/`surface_size`/`present_last_target`/`begin_frame` — NONE of
  which are inside `rebuild_cells`/`update_frame`).

- [ ] **Step 1: Write the failing test** — `linux_opengl_rebuild_cells_places_ab_and_cursor`
      (mirrors whatever CPU-only assertion `macos_impl`'s "3b test" used before the golden existed —
      locate and mirror its shape: feed "Ab" into a `Terminal`, call `rebuild_cells`, assert the
      resulting `frame.cells`/`frame.cells_bg` CPU-side buffers have the expected non-zero entries
      at the expected grid positions, no GPU readback yet). Skip cleanly on GL-context failure (the
      test still needs `Renderer::init`, hence a live offscreen GL context, even though this
      particular assertion is CPU-only).
- [ ] **Step 2: Run to verify it fails** — FAIL.
- [ ] **Step 3: Implement** — copy-adapt the two functions per the Interfaces note.
- [ ] **Step 4: Run to verify it passes** — PASS/skips.
- [ ] **Step 5: Commit** — `git commit -m "rustty-renderer: linux_impl rebuild_cells + update_frame uniforms (L4 T10)"`

---

### Task 11: `draw_frame` (bg_color→cell_bg→cell_text) + the offscreen golden — MILESTONE GATE

**Files:**
- Modify: `crates/rustty-renderer/src/generic/linux_impl.rs`

**Interfaces:**
- Consumes: Tasks 1-10, everything.
- Produces: `Renderer::draw_frame`/`frame_completed`/`sync_atlas_texture`/`init_shaders`/
  `deinit_shaders` — the milestone-scoped encode path: `self.api.begin_frame(self, &frame.target)`
  → one `render_pass` with the `bg_color` step (no `bg_image`, `generic.zig:1621-1628`'s `else`
  arm) → the `cell_bg` step (`generic.zig:1640-1645`) → the `cell_text` step
  (`generic.zig:1656-1672`) → `pass.complete()` → `frame_ctx.complete(sync)`. Kitty-image /
  overlay / custom-shader branches (`generic.zig:1630-1637,1647-1653,1674-1720`) are compiled out
  with a `// MILESTONE-SCOPE-CUT` comment citing this plan, not silently deleted — matches the
  existing `unimplemented!()`-for-inspector-overlay precedent's honesty bar.
  The golden test module (new, at the bottom of `linux_impl.rs`, mirroring `macos_impl::tests`):
  `golden_renderer(cols, rows) -> Option<Renderer>` (calls `OpenGL::init_offscreen` first as the
  skip-probe, THEN `Renderer::init`, mirroring `macos_impl`'s `golden_renderer` shape exactly —
  same `derived_config_for_test()` literal, duplicated per the established test-helper-duplication
  precedent); `golden_renders_cursor_fg_bg`: feed "Ab" into a 10x3 `Terminal` (SAME input as the
  Metal golden), `draw_frame(true)`, `target.read_rgba()`, and assert the SAME kind of structural
  pixel facts the Metal golden asserts (background color region, foreground glyph presence, block
  cursor at grid (2,0)) via `px_at`-equivalent sampling — reuse the Metal golden's THRESHOLD
  PHILOSOPHY (structural, not exact-byte, per generic.rs:6478-6490's own comment on why: AA/
  rounding differ across GPU backends) but do NOT literally cross-compare GL bytes against Metal
  bytes — they are different rendering pipelines and are not expected to produce identical bytes,
  only the same STRUCTURAL result. State this explicitly in the test's doc comment to avoid a
  future reviewer assuming byte-parity was attempted.

- [ ] **Step 1: Write the failing test** — `golden_renders_cursor_fg_bg` per the Interfaces
      description above (the actual milestone deliverable). Skip cleanly on GL-context/version
      failure (the Task 1 OPEN RISK path).
- [ ] **Step 2: Run to verify it fails** — `cargo test -p rustty-renderer --target
      aarch64-unknown-linux-gnu golden_renders_cursor_fg_bg` → FAIL (`draw_frame` doesn't exist).
- [ ] **Step 3: Implement** — port `draw_frame`'s milestone-scoped step sequence + the golden
      harness, copy-adapting `macos_impl`'s `draw_frame_raw`/`frame_completed`/`sync_atlas_texture`/
      `init_shaders`/`deinit_shaders` (generic.rs:5408 area) for the `crate::opengl::*` types.
- [ ] **Step 4: Run to verify it passes** — same command → PASS (or a clearly-logged clean skip if
      the Task-1 GL-4.3 risk materializes on this host — either outcome is an acceptable Task 11
      completion AS LONG AS the skip path is exercised and correct, since the milestone's contract
      is "the port is correct when a capable host runs it," mirroring how the Metal golden has
      always tolerated a no-Metal-device CI leg). Also run `cargo test -p rustty-renderer` (full
      suite) and, where a macOS runner is available, confirm the existing Metal golden is still
      green (R5 check).
- [ ] **Step 5: Commit** — `git commit -m "rustty-renderer: linux_impl draw_frame + OpenGL offscreen golden (L4 T11)"`

**Gate after Task 11 (MILESTONE):** the monomorphic `Renderer` over `OpenGL` renders one real
terminal frame (background + "Ab" glyphs + block cursor) into a GL FBO headlessly via EGL
surfaceless, and the pixels are read back and structurally verified — the stated L4 first
milestone. `cargo build -p rustty-renderer --target aarch64-unknown-linux-gnu` is cfg-clean;
Metal's macOS golden is unaffected (R5); `check_acyclic.sh`/`check_leaks.sh` green with the new
Linux FFI arm recorded.

---

## Deferred (out of L4 scope, documented in DEFERRALS.md)

- **Windowed present / GLArea attach ("GL-ATTACH"):** `OpenGL::present`'s real
  `glBlitFramebuffer`-to-default-framebuffer body is ported (Task 8) but never exercised — needs a
  real windowing host (`GtkGLArea`'s context), which is L5's job. `surfaceInit`/`displayRealized`
  (`OpenGL.zig:161-250`, the GTK-runtime `prepareContext(null)`-via-epoxy arm) are NOT ported this
  tier — they need the `epoxy` loader decision this plan explicitly punts to L5.
- **Background image, kitty graphics placement, custom (post-process) shaders, debug overlay** for
  the OpenGL backend — mirrors the Metal lane's own staged "P1 fill" backlog (`GPU_RD_P1_DESIGN.md`),
  now reopened a second time for OpenGL. The `bg_image`/`image`/post-pipeline `Pipeline`s ARE built
  by `Shaders::init` (Task 7) so this is a "wire it up" follow-up, not a "port it" follow-up.
  `opengl::shaders::Shaders::init_post_pipelines`/`init_post_pipeline` bodies are ALREADY ported
  (Task 7) since they were cheap to include whole; only the `draw_frame` call sites that would
  invoke them are deferred.
- **`GraphicsApi` trait extraction (unifying `macos_impl`/`linux_impl`):** `GPU_RD_TRAIT_DESIGN.md`
  explicitly flagged the original monomorphic Metal decision as reversible "when OpenGL actually
  exists to validate the abstraction." OpenGL now exists (post-L4), so this refactor becomes
  possible — but it is a cross-cutting change touching the ALREADY-GATED-GREEN `macos_impl`, so per
  this plan's R5 constraint it is explicitly NOT taken here. Left as an open architectural option
  for a future dedicated task, not a silent TODO.
- **X11/GLX path:** L6 territory per the roadmap (`AD-6`); this plan is EGL-only (both the offscreen
  KAT and the anticipated L5 GLArea/Wayland path use EGL, not GLX).
- **`opengl::gl::extensions` consumers:** the module is ported (Task 4) but nothing calls it yet —
  ghostty itself only uses it for a commented-out debug log (`OpenGL.zig:179-186`); left equally
  inert here, faithfully.

## Self-review notes (spec coverage)

- `OpenGL.zig` (461 loc) → Tasks 1 (context/version-gate/debug-callback halves) + 8 (the rest). ✓
- `opengl/{buffer,Frame,Pipeline,RenderPass,Sampler,shaders,Target,Texture}.zig` (~1090 loc) →
  Tasks 5-7 (leaves) + 6 (draw-plumbing) + 7 (shaders/gpu_abi). ✓
- `pkg/opengl/*.zig` (~1266 loc, the low-level GL wrapper ghostty itself hand-wrote) → Tasks 2-4. ✓
- 10 GLSL shaders reused verbatim → Task 7, with a byte-diff test as the strongest per-task R1
  gate in the plan (needs no GL context, runs everywhere). ✓
- The stated milestone ("OpenGL::init + generic GL draw path renders one terminal frame to an FBO,
  pixel-compared to the existing golden") → Task 11, with an explicit note that "pixel-compared"
  means the SAME structural-assertion philosophy the Metal golden uses, not cross-API byte
  equality (documented explicitly to prevent a future misreading). ✓
- Headless GL via EGL surfaceless → Task 1, with the GL-4.3-availability OPEN RISK surfaced
  empirically at the earliest possible task rather than discovered late. ✓
- GL loader decision (glow vs gl vs epoxy-sys) → resolved in Tech Stack with rationale: raw `gl`
  crate (fidelity to ghostty's own raw-glad-plus-hand-wrapper shape) + EGL now / `epoxy` deferred
  to L5, recorded as an explicit ADR so L5 doesn't re-litigate it. ✓
- R5 (don't touch Metal) → every generic.rs/lib.rs/Cargo.toml edit in the File-structure table is
  called out as additive-only; `macos_impl` is read-only reference material throughout, never a
  modification target. ✓
- `gpu_abi` Metal/OpenGL byte-layout divergence → caught during research (Metal's `bools` is an
  `extern struct` of individual bools, OpenGL's is a bit-packed `packed struct(u32)`) and called
  out as a Global Constraint + a fresh-port requirement in Task 7, not assumed-identical. ✓
