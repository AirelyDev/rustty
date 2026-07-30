//! PORT STATUS (L4, Linux-only): the OpenGL graphics-API backend — the second
//! concrete graphics API alongside `crate::metal`.
//!
//! PORTS: ghostty `src/renderer/OpenGL.zig` + `src/renderer/opengl/*.zig`, layered
//! over [`gl`] — this crate's hand-port of ghostty's own low-level `pkg/opengl`
//! wrapper package. That two-layer shape is deliberate and mirrors the reference:
//! `pkg/opengl` is a thin hand-rolled wrapper over a RAW glad function-pointer
//! context, so the Rust port keeps a hand-ported `pkg/opengl` twin over the raw
//! `gl` crate (glad's analogue). See the plan's "Tech Stack / GL loader decision".
//!
//! Task 1 lands only [`gl::sys`] — the headless-EGL context plus the
//! `OpenGL.zig::prepareContext` version/debug-output gate. The rest of
//! `OpenGL.zig`, the `pkg/opengl` leaves, the `renderer/opengl/*` leaves and the
//! monomorphic `GenericRenderer(OpenGL)` are later tasks.
//! (docs/superpowers/plans/2026-07-17-L4-opengl-renderer.md)

pub mod gl;

// PORT STATUS (L4 Task 5): the renderer-facing resource leaves, layered over the
// `gl` package (Tasks 2-4). These are the OpenGL twins of `metal/{buffer,texture,
// sampler}.rs`: `Options`-carrying wrappers that drive object creation + upload.
//   * `buffer`  — ports `src/renderer/opengl/buffer.zig`  (the `Buffer(T)` growable store)
//   * `texture` — ports `src/renderer/opengl/Texture.zig` (init + replaceRegion)
//   * `sampler` — ports `src/renderer/opengl/Sampler.zig`
pub mod buffer;
pub mod sampler;
pub mod texture;

// PORT STATUS (L4 Task 6): the draw-plumbing leaves, layered over the resource
// leaves (Task 5) + the `gl` package. These are the OpenGL twins of
// `metal/{target,pipeline,render_pass,frame}.rs`: everything needed to stand up a
// render target, compile a pipeline, and issue ONE clear+draw call.
//   * `target`      — ports `src/renderer/opengl/Target.zig`     (renderbuffer-backed FBO)
//   * `pipeline`    — ports `src/renderer/opengl/Pipeline.zig`   (program + vao + autoAttribute)
//   * `render_pass` — ports `src/renderer/opengl/RenderPass.zig` (begin/step/complete)
//   * `frame`       — ports `src/renderer/opengl/Frame.zig`      (begin/render_pass/complete)
pub mod frame;
pub mod pipeline;
pub mod render_pass;
pub mod target;

// PORT STATUS (L4 Task 7): the GLSL shader assets + the OpenGL-ABI POD structs +
// the `Shaders` container that compiles/links the reused-verbatim GLSL into the 5
// render pipelines. These are the OpenGL twins of `metal/{gpu_abi,shaders}.rs`.
//   * `gpu_abi` — ports `src/renderer/opengl/shaders.zig:163-300` (the FRESH
//                 OpenGL byte layouts; NOT copied from `metal::gpu_abi`)
//   * `shaders` — ports `src/renderer/opengl/shaders.zig:1-160,302-376` (the
//                 pipeline collection over the verbatim `src/shaders/glsl/*.glsl`)
pub mod gpu_abi;
pub mod shaders;

// PORT STATUS (L4 Task 8): the graphics-API impl tying the leaves together — the
// OpenGL twin of `metal::metal_impl::Metal`, ported from `src/renderer/OpenGL.zig`.
// `OpenGL::{init_offscreen, deinit, init_shaders, init_target, present,
// present_last_target, buffer/texture/sampler/image_texture options,
// init_atlas_texture, begin_frame, draw_frame_start/end, surface_size}`. Consumed
// by the `generic::linux_impl` monomorphic `Renderer(OpenGL)` in Task 9.
pub mod opengl_impl;
pub use opengl_impl::OpenGL;

// RUSTTY-ONLY smoke tests for the Task-5 leaves — same framing as `metal.rs`'s
// `creates_buffer_texture_sampler_without_panic`: the reference has NO `test{}`
// block over `renderer/opengl/{buffer,Texture,Sampler}.zig`, so there is no parity
// oracle to match. They drive the ported wrappers through a REAL driver's whole
// buffer-grow / texture-upload / sampler-create lifecycle without panicking and
// without provoking a GL error. Skips cleanly (never panics) when the host has no
// OpenGL 4.3 core context — see [`gl::test_support::offscreen_gl`].
#[cfg(test)]
mod tests {
    use super::gl::buffer::{Target, Usage};
    use super::gl::errors::get_error;
    use super::gl::test_support;
    use super::gl::texture as gltex;
    use super::{buffer, sampler, texture};

    /// Mirrors ghostty's `Buffer.sync` grow-path (`renderer/opengl/buffer.zig:76-92`)
    /// and the Metal `buf.sync` grow-path smoke test in `metal.rs`:
    /// `Buffer::<u32>::init(4)`, then `sync(&[6 elems])` must take the
    /// doubled-capacity reallocation path (`self.len = data.len * 2` == 12). Also
    /// exercises `init_fill` (whose `len` is the reference's byte-valued quirk),
    /// the in-capacity `sync` no-realloc path, and `sync_from_array_lists`.
    #[test]
    fn opengl_buffer_grow_and_sync() {
        let Some(_gl) = test_support::offscreen_gl("opengl_buffer_grow_and_sync") else {
            return;
        };
        assert!(get_error().is_ok(), "context started in a GL error state");

        // `Buffer.init` with 4 `u32`s pre-allocated (`buffer.zig:33-47`).
        let mut buf = buffer::Buffer::<u32>::init(
            buffer::Options {
                target: Target::ARRAY,
                usage: Usage::DYNAMIC_DRAW,
            },
            4,
        )
        .expect("Buffer::init");
        assert_eq!(buf.len, 4);
        assert!(get_error().is_ok(), "after Buffer::init");

        // `Buffer.initFill` (`buffer.zig:50-64`). FAITHFULNESS QUIRK: its `len` is
        // `data.len * @sizeOf(T)` (BYTES), unlike `init`'s element count — pinned.
        // RE-AFFIRMED (P3-W0 T5): checked against oracle range 4749c4e93..6ad1fe7d8 — src/renderer/opengl/buffer.zig is UNTOUCHED there, so this quirk is still faithful. Ledger: .port/tools/gates/stale-comments-D9.tsv.
        let filled = buffer::Buffer::<u32>::init_fill(buffer::Options::default(), &[1u32, 2, 3])
            .expect("Buffer::init_fill");
        assert_eq!(filled.len, 3 * std::mem::size_of::<u32>());
        assert!(get_error().is_ok(), "after Buffer::init_fill");

        // THE grow path: 6 elems > 4 -> reallocate to `2 * 6 == 12` (`buffer.zig:81-88`).
        buf.sync(&[10u32, 20, 30, 40, 50, 60])
            .expect("Buffer::sync");
        assert_eq!(buf.len, 12, "grow path sets len to 2*data.len");
        assert!(get_error().is_ok(), "after Buffer::sync grow");

        // In-capacity sync (3 <= 12): must NOT reallocate, `len` untouched.
        buf.sync(&[1u32, 2, 3]).expect("Buffer::sync fit");
        assert_eq!(buf.len, 12, "in-capacity sync leaves len untouched");
        assert!(get_error().is_ok(), "after Buffer::sync fit");

        // `syncFromArrayLists` (`buffer.zig:96-124`) — returns the item total.
        let lists = [vec![7u32, 8], vec![9u32]];
        let n = buf
            .sync_from_array_lists(&lists)
            .expect("Buffer::sync_from_array_lists");
        assert_eq!(n, 3);
        assert!(get_error().is_ok(), "after Buffer::sync_from_array_lists");

        buf.deinit();
        filled.deinit();
        assert!(get_error().is_ok(), "after Buffer::deinit");
    }

    /// Ports the shape of `Texture.init` + `Texture.replaceRegion`
    /// (`renderer/opengl/Texture.zig`): a 2x2 RGBA texture created WITH data (the
    /// `parameter`x4 + `image2D` path), a `replace_region` sub-upload, a second
    /// `init` with `None` data (the `image2D(null)` allocate-only path), and a
    /// `Sampler::init` (the parallel leaf).
    #[test]
    fn opengl_texture_replace_region_smoke() {
        let Some(_gl) = test_support::offscreen_gl("opengl_texture_replace_region_smoke") else {
            return;
        };
        assert!(get_error().is_ok(), "context started in a GL error state");

        // Opaque red / green / blue / white, one texel each.
        #[rustfmt::skip]
        let pixels: [u8; 16] = [
            0xff, 0x00, 0x00, 0xff,   0x00, 0xff, 0x00, 0xff,
            0x00, 0x00, 0xff, 0xff,   0xff, 0xff, 0xff, 0xff,
        ];

        let tex = texture::Texture::init(
            texture::Options {
                format: gltex::Format::RGBA,
                internal_format: gltex::InternalFormat::RGBA,
                target: gltex::Target::Texture2D,
                min_filter: gltex::MinFilter::Linear,
                mag_filter: gltex::MagFilter::Linear,
                wrap_s: gltex::Wrap::ClampToEdge,
                wrap_t: gltex::Wrap::ClampToEdge,
            },
            2,
            2,
            Some(&pixels),
        )
        .expect("Texture::init");
        assert_eq!(tex.width, 2);
        assert_eq!(tex.height, 2);
        assert!(get_error().is_ok(), "after Texture::init");

        // `replaceRegion` — overwrite the (1,1) texel (`Texture.zig:84-104`).
        tex.replace_region(1, 1, 1, 1, &[0x12, 0x34, 0x56, 0x78])
            .expect("Texture::replace_region");
        assert!(get_error().is_ok(), "after Texture::replace_region");

        // `init` with `null` data — the allocate-only `image2D(…, null)` path.
        let empty = texture::Texture::init(
            texture::Options {
                format: gltex::Format::RGBA,
                internal_format: gltex::InternalFormat::RGBA,
                target: gltex::Target::Texture2D,
                min_filter: gltex::MinFilter::Nearest,
                mag_filter: gltex::MagFilter::Nearest,
                wrap_s: gltex::Wrap::ClampToEdge,
                wrap_t: gltex::Wrap::ClampToEdge,
            },
            4,
            4,
            None,
        )
        .expect("Texture::init(None)");
        assert!(get_error().is_ok(), "after Texture::init(None)");

        // `Sampler::init` (`renderer/opengl/Sampler.zig:28-41`).
        let smp = sampler::Sampler::init(sampler::Options {
            min_filter: gltex::MinFilter::Linear,
            mag_filter: gltex::MagFilter::Linear,
            wrap_s: gltex::Wrap::ClampToEdge,
            wrap_t: gltex::Wrap::ClampToEdge,
        })
        .expect("Sampler::init");
        assert!(get_error().is_ok(), "after Sampler::init");

        smp.deinit();
        empty.deinit();
        tex.deinit();
        assert!(get_error().is_ok(), "after deinit");
    }

    /// A minimal, valid `#version 430 core` vertex shader with NO vertex input —
    /// the clear-only pipeline's stage (`VertexAttributes = None`). NOT one of the
    /// real GLSL assets (those are reused verbatim in a later task); this pair
    /// exists only to give [`pipeline::Pipeline::init`] a real program to compile +
    /// link, exactly as `gl.rs`'s own smoke test does.
    const CLEAR_VERTEX_SRC: &std::ffi::CStr = c"#version 430 core\n\
        void main() {\n\
        \x20   gl_Position = vec4(0.0, 0.0, 0.0, 1.0);\n\
        }\n";

    /// The matching `#version 430 core` fragment shader.
    const CLEAR_FRAGMENT_SRC: &std::ffi::CStr = c"#version 430 core\n\
        out vec4 frag_color;\n\
        void main() {\n\
        \x20   frag_color = vec4(1.0, 0.0, 0.0, 1.0);\n\
        }\n";

    /// RUSTTY-ONLY smoke test (NOT a ghostty parity test): the OpenGL twin of
    /// `metal.rs`'s `draw_plumbing_smoke`. It drives the four draw-plumbing leaves
    /// through ONE clear-only render pass against a real GL context: a
    /// `Target::init` OFFSCREEN (a renderbuffer-backed FBO — exercises the
    /// `Renderbuffer.storage` + `Framebuffer.renderbuffer` attach path), a trivial
    /// `Pipeline` compiled + linked from the inline clear-only shader pair
    /// (exercises `Program.createVF` through `Pipeline::init`'s `defer`/`errdefer`
    /// fan-out), a `RenderPass` whose one `step()` clears the target to a known
    /// color and draws NOTHING, and `read_rgba()` — the golden-support readback.
    /// Asserts the `4*4*4` bytes come back as the clear color.
    ///
    /// PORT NOTE (the step's `instance_count`): `RenderPass.zig:63` short-circuits a
    /// step whose `instance_count == 0` at its FIRST line — BEFORE the clear at
    /// `:88`. So to actually exercise (and verify) the clear-only path, the step
    /// uses `instance_count: 1` with `vertex_count: 0`: the step runs the clear,
    /// then `glDrawArraysInstanced(…, 0, 1)` draws zero vertices — nothing is
    /// rendered, the clear color survives, and `read_rgba` reads it back. (A literal
    /// `instance_count: 0` would skip the clear too, leaving the readback undefined
    /// and the assertion meaningless.)
    ///
    /// Skips cleanly (never panics) when the host has no OpenGL 4.3 core context —
    /// see [`gl::test_support::offscreen_gl`].
    #[test]
    fn opengl_draw_plumbing_smoke() {
        use super::gl::errors::get_error;
        use super::gl::primitives::Primitive;
        use super::gl::test_support;
        use super::gl::texture::InternalFormat;
        use super::pipeline::{Options as PipelineOptions, Pipeline, StepFunction};
        use super::render_pass::{
            Attachment, AttachmentTarget, Draw, Options as PassOptions, RenderPass, Step,
        };
        use super::target::{Options as TargetOptions, Target};

        let Some(_gl) = test_support::offscreen_gl("opengl_draw_plumbing_smoke") else {
            return;
        };
        assert!(get_error().is_ok(), "context started in a GL error state");

        let w = 4usize;
        let h = 4usize;

        // Target::init OFFSCREEN — a renderbuffer-backed FBO in RGBA8.
        let target = Target::init(TargetOptions {
            width: w,
            height: h,
            internal_format: InternalFormat::RGBA,
        })
        .expect("Target::init");
        assert_eq!(target.width, w);
        assert_eq!(target.height, h);
        assert!(get_error().is_ok(), "after Target::init");

        // A trivial clear-only Pipeline: compile + link the inline shader pair, no
        // vertex input (`None`).
        let pipeline = Pipeline::init(
            None,
            PipelineOptions {
                vertex_fn: CLEAR_VERTEX_SRC,
                fragment_fn: CLEAR_FRAGMENT_SRC,
                step_fn: StepFunction::PerVertex,
                blending_enabled: true,
            },
        )
        .expect("Pipeline::init");
        assert!(get_error().is_ok(), "after Pipeline::init");

        // Blue, chosen for exact 8-bit round-trips: 0.0 -> 0, 1.0 -> 255.
        let clear = [0.0f32, 0.0, 1.0, 1.0];

        // One render pass, one step: clear to `clear`, draw nothing (see the note).
        {
            let attachments = [Attachment {
                target: AttachmentTarget::Target(&target),
                clear_color: Some(clear),
            }];
            let mut pass = RenderPass::begin(PassOptions {
                attachments: &attachments,
            });
            pass.step(Step {
                pipeline: &pipeline,
                uniforms: None,
                buffers: &[],
                textures: &[],
                samplers: &[],
                draw: Draw {
                    kind: Primitive::Triangle,
                    vertex_count: 0,
                    instance_count: 1,
                },
            });
            pass.complete();
        }
        assert!(get_error().is_ok(), "after render pass");

        // Golden-support readback: `w*h*4` RGBA bytes out of the offscreen target.
        let pixels = target.read_rgba();
        assert_eq!(pixels.len(), w * h * 4);
        assert!(get_error().is_ok(), "after read_rgba");

        // Every texel is the clear color: 0.0->0, 0.0->0, 1.0->255, 1.0->255.
        let expected = [0u8, 0, 255, 255];
        for (i, texel) in pixels.chunks_exact(4).enumerate() {
            assert_eq!(texel, expected, "texel {i} is not the clear color");
        }

        pipeline.deinit();
        target.deinit();
        assert!(get_error().is_ok(), "after deinit");
    }
}
