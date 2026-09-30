//! PORTS: ghostty's `pkg/opengl` package — the low-level OpenGL wrapper ghostty
//! hand-wrote over glad (`pkg/opengl/main.zig`'s module docs: "purpose-built for
//! usage within this program … not meant to have 100% API coverage"). It is the GL
//! analogue of what `objc2-metal` is to the Metal lane, except that ghostty
//! hand-rolled it, so it is hand-ported here rather than taken off crates.io.
//!
//! PORT NOTE: this module is `crate::opengl::gl`; the raw `gl` CRATE (glad's
//! analogue, i.e. `pkg/opengl/c.zig` + `glad.zig`) is `::gl`.
//!
//! This file is the port of `pkg/opengl/main.zig` itself (45 loc): the package's
//! flattened public surface. The re-exports below are one-for-one with its
//! `pub const` list, so every reference call site (`gl.Buffer.create()`,
//! `gl.errors.getError()`, `gl.clearColor(…)`, `gl.Primitive`, …) ports with its
//! spelling intact.
//!
//! Task 1 landed [`sys`]. Task 2 lands [`buffer`]/[`vertex_array`]/[`errors`]/
//! [`primitives`]/[`draw`]. Task 3 lands [`shader`]/[`program`]. Task 4 lands
//! [`texture`]/[`sampler`]/[`framebuffer`]/[`renderbuffer`]/[`extensions`], which
//! COMPLETES the package: every file of the ~1266-loc `pkg/opengl` oracle is now
//! ported, and the renderer-facing leaves (Task 5+) build on top of it.
//!
//! PORT NOTE (`pub const c` / `pub const glad`, `main.zig:14-15`): no analogue.
//! Both name ghostty's own vendored glad artifacts; the `gl` crate IS that layer
//! here and is reachable as `::gl` from anywhere in the crate, so re-exporting it
//! under a second name would add a spelling the reference does not have.

pub mod buffer;
pub mod draw;
pub mod errors;
pub mod extensions;
pub mod framebuffer;
pub mod glad;
pub mod primitives;
pub mod program;
pub mod renderbuffer;
pub mod sampler;
pub mod shader;
pub mod sys;
pub mod texture;
pub mod vertex_array;

// PORT NOTE (`const log = std.log.scoped(.opengl);`): EVERY file in `pkg/opengl`
// that logs declares that same line and shares the ONE `.opengl` scope
// (`Shader.zig:5`, `Program.zig:5`, `OpenGL.zig:34`). These macros reproduce that
// scoped logger by emitting `std.log`'s own wire format (`error(opengl): …`), so
// every reference call site ports as a one-for-one textual match.
//
// CORRECTED (task-14c-1): the original note claimed "this crate has no log facade
// and R3 permits no new dependency for one". Both halves are now false — the `log`
// facade IS a dependency of this crate (`Cargo.toml`'s `log = { workspace = true }`,
// DECISION-logging.md Option A, ratified), so these `eprintln!` bodies are exactly
// the `eprintln!`-stand-in class that stage 14c is converting elsewhere. Rewriting
// them to `log::error!(target: "opengl", …)` etc. is the OpenGL 14c stage's work,
// not this one's; the macro NAMES and call sites do not have to change when it
// happens, only the bodies here.
//
// DONE (task-14c-3, the OpenGL 14c stage): the bodies below now route through the
// `log` facade with `target: LOG_SCOPE` (`"opengl"`), so every one of the FIFTEEN
// call sites — 8 in `gl/sys.rs`, 3 in `gl/shader.rs`, 4 in `gl/program.rs` —
// reaches whichever `log::Log` backend the running binary installed, exactly like
// the `metal`-scope sites 14c-2 converted. (14 of the 15 port a reference call:
// 7 from `OpenGL.zig`, 3 from `Shader.zig`, 4 from `Program.zig`. The 15th,
// `sys.rs`'s `initialized EGL …`, is RUSTTY-ONLY offscreen scaffolding with no
// oracle. Count corrected in the 14c-3 fix round — it read "eleven".)
// The macro NAMES and the call sites are unchanged, as that note predicted. No row
// in `LEDGER.tsv` claims these — they were never DEAD, only mis-routed — but the
// note above scheduled them here explicitly, so they are done here.
//
// CONSEQUENCE, stated so it is findable: a binary that installs NO `log::Log`
// backend now prints NOTHING here, where the `eprintln!` bodies printed to stderr
// unconditionally. That is correct parity — `std.log.scoped(.opengl)` likewise
// goes nowhere without a `logFn` — and all seven ported message texts are
// byte-identical to before; but it IS a user-visible change to a Linux build's
// output. `rustty-vt-c::sys::Logger` / `rustty-core::logging::AppLogger` install
// one for the real binaries; `cargo test -p rustty-renderer` installs
// `log_capture`'s.
//
// They live HERE, at the package root, rather than in each leaf, because the
// reference's logger is likewise one shared thing rather than a per-file one.
// Task 1 introduced them privately inside `sys` (which was then the only file that
// logged); Task 3 adds two more files that log (`Shader.zig`/`Program.zig`), so
// they are hoisted rather than copied. `sys` now imports them from here.

/// `const log = std.log.scoped(.opengl);` — shared by EVERY file in `pkg/opengl`
/// (`Shader.zig:5`, `Program.zig:5`) and by `src/renderer/OpenGL.zig:34`.
pub(crate) const LOG_SCOPE: &str = "opengl";

macro_rules! log_err {
    ($($arg:tt)*) => { log::error!(target: $crate::opengl::gl::LOG_SCOPE, $($arg)*) };
}
macro_rules! log_warn {
    ($($arg:tt)*) => { log::warn!(target: $crate::opengl::gl::LOG_SCOPE, $($arg)*) };
}
macro_rules! log_info {
    ($($arg:tt)*) => { log::info!(target: $crate::opengl::gl::LOG_SCOPE, $($arg)*) };
}
macro_rules! log_debug {
    ($($arg:tt)*) => { log::debug!(target: $crate::opengl::gl::LOG_SCOPE, $($arg)*) };
}

// `macro_rules!` is textually scoped; these re-exports make the macros addressable
// as `super::log_err` etc. from the leaf modules regardless of declaration order.
pub(crate) use {log_debug, log_err, log_info, log_warn};

// RUSTTY-ONLY: the `#[cfg(test)]` GL-context scaffolding shared by this module's
// leaf tests and `sys`'s. Not part of `main.zig`'s surface.
#[cfg(test)]
pub(crate) mod test_support;

// pub const ext = @import("extensions.zig");
//
// PORT NOTE: the reference re-exports the MODULE under a shortened name — its
// call sites would read `gl.ext.iterator()`, not `gl.extensions.iterator()`. The
// module itself is `extensions` (after the file, per this tree's convention and
// the brief's named path), and this alias restores the reference's spelling, so
// both resolve.
pub use extensions as ext;

// pub const Buffer = @import("Buffer.zig");
pub use buffer::Buffer;
// pub const Framebuffer = @import("Framebuffer.zig");
pub use framebuffer::Framebuffer;
// pub const Renderbuffer = @import("Renderbuffer.zig");
pub use renderbuffer::Renderbuffer;
// pub const Program = @import("Program.zig");
pub use program::Program;
// pub const Sampler = @import("Sampler.zig");
pub use sampler::Sampler;
// pub const Shader = @import("Shader.zig");
pub use shader::Shader;
// pub const Texture = @import("Texture.zig");
pub use texture::Texture;
// pub const VertexArray = @import("VertexArray.zig");
pub use vertex_array::VertexArray;

// pub const Primitive = @import("primitives.zig").Primitive;
pub use primitives::Primitive;

// const draw = @import("draw.zig");
//
// pub const blendFunc = draw.blendFunc;
// pub const clear = draw.clear;
// pub const clearColor = draw.clearColor;
// pub const drawArrays = draw.drawArrays;
// pub const drawArraysInstanced = draw.drawArraysInstanced;
// pub const drawElements = draw.drawElements;
// pub const drawElementsInstanced = draw.drawElementsInstanced;
// pub const enable = draw.enable;
// pub const disable = draw.disable;
// pub const frontFace = draw.frontFace;
// pub const readPixels = draw.readPixels;
// pub const pixelStore = draw.pixelStore;
// pub const viewport = draw.viewport;
// pub const blitFramebuffer = draw.blitFramebuffer;
// pub const flush = draw.flush;
// pub const finish = draw.finish;
//
// PORT NOTE: `main.zig` re-exports `draw`'s fns individually and keeps the
// `draw` module itself PRIVATE (`const draw =`, not `pub const draw =`). Rust
// cannot re-export a fn out of a private module into the public API without the
// `private_interfaces` problem, and the brief names `opengl::gl::draw::{…}` as
// this task's produced path, so `draw` is `pub` and re-exported from. Both
// spellings — `gl::clear_color(…)` (the reference's) and `gl::draw::clear_color(…)`
// — therefore resolve.
pub use draw::{
    blend_func, blit_framebuffer, clear, clear_color, disable, draw_arrays, draw_arrays_instanced,
    draw_elements, draw_elements_instanced, enable, finish, flush, front_face, pixel_store,
    read_pixels, viewport,
};

#[cfg(test)]
mod tests {
    use std::ffi::CStr;

    use super::buffer::{Buffer, Target, Usage};
    use super::errors::get_error;
    use super::extensions;
    use super::framebuffer::{self, Framebuffer};
    use super::program::{self, Program};
    use super::renderbuffer::Renderbuffer;
    use super::sampler::Sampler;
    use super::shader::{self, Shader};
    use super::test_support;
    use super::texture::{self, Texture};
    use super::vertex_array::VertexArray;

    /// A minimal, valid `#version 430 core` vertex shader.
    ///
    /// NOT one of the real GLSL assets — those are reused verbatim from the
    /// reference in Task 7. This pair exists only to drive `Shader`/`Program`
    /// through a real GLSL compile+link, so it is kept as small as a legal
    /// core-profile shader can be.
    const VERTEX_SRC: &CStr = c"#version 430 core\n\
        void main() {\n\
        \x20   gl_Position = vec4(0.0, 0.0, 0.0, 1.0);\n\
        }\n";

    /// A minimal, valid `#version 430 core` fragment shader. Core profile has no
    /// `gl_FragColor`, so it declares its own output.
    const FRAGMENT_SRC: &CStr = c"#version 430 core\n\
        out vec4 frag_color;\n\
        void main() {\n\
        \x20   frag_color = vec4(1.0, 0.0, 0.0, 1.0);\n\
        }\n";

    /// RUSTTY-ONLY smoke test — as with [`gl_buffer_vertex_array_smoke`], the
    /// reference has NO `test{}` block over `Shader.zig`/`Program.zig`, so there
    /// is no parity oracle to match. It proves the ported wrappers drive a REAL
    /// GLSL compiler and linker through `createVF`'s whole sequence
    /// (`Program.zig:29-44`) and then the `use`/`unbind` round-trip that
    /// `Pipeline.zig:48-49` performs, without panicking and without provoking a
    /// GL error.
    ///
    /// Skips cleanly (never panics) when the host has no OpenGL 4.3 core context
    /// — see [`test_support::offscreen_gl`].
    #[test]
    fn gl_program_compiles_trivial_shader_pair() {
        let Some(_gl) = test_support::offscreen_gl("gl_program_compiles_trivial_shader_pair")
        else {
            return;
        };

        assert!(get_error().is_ok(), "context started in a GL error state");

        // `Program.createVF` (`Program.zig:29-44`) — the ONE call site the
        // reference has for all of `Shader.zig` (`Pipeline.zig:42`). Compiles
        // both stages, creates the program, attaches both, links.
        let program = Program::create_vf(VERTEX_SRC, FRAGMENT_SRC).expect("Program::create_vf");
        assert!(get_error().is_ok(), "after Program::create_vf");
        assert_ne!(program.id, 0, "create_vf handed back the zero program name");

        // `const pbind = try program.use(); defer pbind.unbind();` —
        // `Pipeline.zig:48-49` verbatim.
        let binding = program.use_().expect("Program::use_");
        assert!(get_error().is_ok(), "after Program::use_");

        binding.unbind();
        assert!(get_error().is_ok(), "after Program::Binding::unbind");

        // `errdefer program.destroy()` (`Pipeline.zig:46`).
        program.destroy();
        assert!(get_error().is_ok(), "after Program::destroy");
    }

    /// RUSTTY-ONLY. The FAILURE half of `setSourceAndCompile`
    /// (`Shader.zig:32-37`) and of `link` (`Program.zig:63-72`): both check a
    /// status flag that is NOT a GL error, so nothing in
    /// [`gl_program_compiles_trivial_shader_pair`] — which only ever asserts
    /// `get_error()` — can observe them. A port that dropped the status check
    /// entirely, or inverted it, would pass that test and hand back a
    /// silently-broken program.
    ///
    /// Skips cleanly when the host has no OpenGL 4.3 core context.
    #[test]
    fn gl_shader_compile_failure_is_reported() {
        let Some(_gl) = test_support::offscreen_gl("gl_shader_compile_failure_is_reported") else {
            return;
        };

        assert!(get_error().is_ok(), "context started in a GL error state");

        // Not GLSL at all — guaranteed to fail GL_COMPILE_STATUS on any driver.
        let shader = Shader::create(gl::VERTEX_SHADER).expect("Shader::create");
        let err = shader
            .set_source_and_compile(c"this is not glsl")
            .expect_err("a non-GLSL source must fail to compile");
        assert_eq!(err, shader::Error::CompileFailed);

        // `getInfoLog` (`Shader.zig:46-50`) is what the reference logs on that
        // failure; a driver that rejected the source must have said why.
        let log = shader.get_info_log();
        assert!(
            log.iter().any(|&b| b != 0),
            "a failed compile produced an empty info log"
        );

        shader.destroy();
        assert!(get_error().is_ok(), "after Shader::destroy");

        // A compile failure is reported via GL_COMPILE_STATUS, NOT via
        // glGetError — so the context must still be clean. This is also what
        // lets `create_vf`'s error path stay distinguishable from a GL error.
        assert!(get_error().is_ok(), "a compile failure raised a GL error");

        // The same failure through `createVF`, which is how the reference
        // actually reaches it (`Program.zig:31`).
        assert_eq!(
            Program::create_vf(c"this is not glsl", FRAGMENT_SRC).unwrap_err(),
            program::Error::CompileFailed,
        );
        assert!(get_error().is_ok(), "after a failed Program::create_vf");
    }

    /// RUSTTY-ONLY smoke test — the reference has NO `test{}` block over
    /// `Buffer.zig`/`VertexArray.zig` (nor over any of `pkg/opengl`), so there is
    /// no parity oracle to match here. Same framing as `metal.rs`'s
    /// `creates_buffer_texture_sampler_without_panic`: prove the ported wrappers
    /// drive a REAL driver through their whole lifecycle without panicking and
    /// without provoking a GL error, which is the only thing a hand-rolled FFI
    /// wrapper can get silently wrong (a bad enum value, a wrong pointer, a
    /// swapped argument) and still compile.
    ///
    /// `get_error()` is asserted clean after every step rather than once at the
    /// end, so a failure names the call that caused it. Note that GL errors are
    /// STICKY-until-read, so an un-asserted step would otherwise smear its error
    /// onto a later assert.
    ///
    /// Skips cleanly (never panics) when the host has no OpenGL 4.3 core context
    /// — see [`test_support::offscreen_gl`].
    #[test]
    fn gl_buffer_vertex_array_smoke() {
        let Some(_gl) = test_support::offscreen_gl("gl_buffer_vertex_array_smoke") else {
            return;
        };

        // A fresh context must not already be in an error state; if it is, every
        // assert below would be reporting someone else's error.
        assert!(get_error().is_ok(), "context started in a GL error state");

        // --- Buffer.zig: create -> bind -> setData -> unbind -> destroy --------
        let buf = Buffer::create().expect("Buffer::create");
        assert!(get_error().is_ok(), "after Buffer::create");

        let binding = buf.bind(Target::ARRAY).expect("Buffer::bind");
        assert!(get_error().is_ok(), "after Buffer::bind");

        // The `.slice` arm of `dataInfo` (`Buffer.zig:113-137`) — the only arm the
        // reference actually calls (`renderer/opengl/buffer.zig:57`).
        binding
            .set_data(&[1.0f32, 2.0, 3.0, 4.0], Usage::STATIC_DRAW)
            .expect("Binding::set_data");
        assert!(get_error().is_ok(), "after Binding::set_data");

        // `setSubData` over the just-sized store, and `setDataNullManual` — the
        // two other data paths `renderer/opengl/buffer.zig` (Task 5) drives.
        binding
            .set_sub_data(0, &[5.0f32, 6.0])
            .expect("Binding::set_sub_data");
        assert!(get_error().is_ok(), "after Binding::set_sub_data");

        binding
            .set_data_null_manual(64, Usage::DYNAMIC_DRAW)
            .expect("Binding::set_data_null_manual");
        assert!(get_error().is_ok(), "after Binding::set_data_null_manual");

        binding.unbind();
        assert!(get_error().is_ok(), "after Binding::unbind");

        // `bindBase` (`Buffer.zig:27-34`) — the uniform/storage-block path
        // `renderer/opengl/RenderPass.zig:95,120` drives. `.uniform` at index 1 is
        // literally `RenderPass.zig:95`'s call.
        buf.bind_base(Target::UNIFORM, 1)
            .expect("bind_base uniform");
        assert!(get_error().is_ok(), "after Buffer::bind_base");

        buf.destroy();
        assert!(get_error().is_ok(), "after Buffer::destroy");

        // --- VertexArray.zig: create -> bind -> enableAttribArray -> … --------
        let vao = VertexArray::create().expect("VertexArray::create");
        assert!(get_error().is_ok(), "after VertexArray::create");

        let vao_binding = vao.bind().expect("VertexArray::bind");
        assert!(get_error().is_ok(), "after VertexArray::bind");

        vao_binding
            .enable_attrib_array(0)
            .expect("Binding::enable_attrib_array");
        assert!(get_error().is_ok(), "after Binding::enable_attrib_array");

        // The rest of `VertexArray.Binding` — GL 4.3's separate attribute
        // format/binding API. The brief's test only names `enable_attrib_array`,
        // but these six are the LIVE vertex-attribute path (every one of them is
        // driven by `renderer/opengl/Pipeline.zig`, Task 6) and each is a
        // hand-transcribed FFI signature where a swapped or wrong-typed argument
        // compiles silently and shows up only as a GL error. Nothing else covers
        // them until Task 6, so drive them here where a failure names the wrapper.
        let attrs = Buffer::create().expect("attr Buffer::create");
        let attrs_binding = attrs.bind(Target::ARRAY).expect("attr Buffer::bind");
        attrs_binding
            .set_data(&[0.0f32; 8], Usage::STATIC_DRAW)
            .expect("attr Binding::set_data");
        assert!(get_error().is_ok(), "after attr set_data");

        vao_binding
            .attribute_format(0, 4, gl::FLOAT, false, 0)
            .expect("Binding::attribute_format");
        assert!(get_error().is_ok(), "after Binding::attribute_format");

        vao_binding
            .attribute_i_format(1, 1, gl::UNSIGNED_INT, 16)
            .expect("Binding::attribute_i_format");
        assert!(get_error().is_ok(), "after Binding::attribute_i_format");

        // Hard-codes GL_DOUBLE internally — hence no `typ` argument.
        vao_binding
            .attribute_l_format(2, 1, 24)
            .expect("Binding::attribute_l_format");
        assert!(get_error().is_ok(), "after Binding::attribute_l_format");

        vao_binding
            .attribute_binding(0, 0)
            .expect("Binding::attribute_binding");
        assert!(get_error().is_ok(), "after Binding::attribute_binding");

        // Per-instance stepping on the BINDING point (not the attribute) — the
        // 4.3 counterpart of `Buffer.Binding`'s `glVertexAttribDivisor`.
        vao_binding
            .binding_divisor(0, 1)
            .expect("Binding::binding_divisor");
        assert!(get_error().is_ok(), "after Binding::binding_divisor");

        // Takes a RAW buffer name, matching the reference's `buf.buffer.id` call
        // sites — so this is also the assertion that `Buffer::create` handed back
        // a name GL actually recognizes.
        vao_binding
            .bind_vertex_buffer(0, attrs.id, 0, 32)
            .expect("Binding::bind_vertex_buffer");
        assert!(get_error().is_ok(), "after Binding::bind_vertex_buffer");

        attrs_binding.unbind();
        attrs.destroy();
        assert!(get_error().is_ok(), "after attr Buffer::destroy");

        vao_binding.unbind();
        assert!(get_error().is_ok(), "after VertexArray::Binding::unbind");

        vao.destroy();
        assert!(get_error().is_ok(), "after VertexArray::destroy");
    }

    /// RUSTTY-ONLY smoke test — same framing as [`gl_buffer_vertex_array_smoke`]:
    /// the reference has NO `test{}` block over `Texture.zig`/`Sampler.zig`/
    /// `Framebuffer.zig`/`Renderbuffer.zig`/`extensions.zig`, so there is no
    /// parity oracle to match. It drives the ported wrappers through a REAL
    /// driver, which is the only way to catch what a hand-transcribed FFI shim
    /// gets silently wrong and still compiles: a wrong enum value, a swapped
    /// argument, a bad pointer.
    ///
    /// The shape mirrors `metal.rs`'s texture smoke test (a 2x2 RGBA texture with
    /// data), then adds the offscreen-target trio the reference builds in
    /// `renderer/opengl/Target.zig`: a renderbuffer + framebuffer, attached at
    /// `COLOR_ATTACHMENT0`, asserted `complete`.
    ///
    /// `get_error()` is asserted clean after every step rather than once at the
    /// end, so a failure names the call that caused it — GL errors are
    /// STICKY-until-read, so an un-asserted step would smear its error onto a
    /// later assert.
    ///
    /// Skips cleanly (never panics) when the host has no OpenGL 4.3 core context
    /// — see [`test_support::offscreen_gl`].
    #[test]
    fn gl_texture_framebuffer_renderbuffer_smoke() {
        let Some(_gl) = test_support::offscreen_gl("gl_texture_framebuffer_renderbuffer_smoke")
        else {
            return;
        };

        assert!(get_error().is_ok(), "context started in a GL error state");

        // --- Texture.zig: active -> create -> bind -> parameter -> image2D ----

        // `Texture.active(0)` — `renderer/opengl/RenderPass.zig:139`'s call.
        Texture::active(0).expect("Texture::active");
        assert!(get_error().is_ok(), "after Texture::active");

        let tex = Texture::create().expect("Texture::create");
        assert!(get_error().is_ok(), "after Texture::create");

        let tbind = tex.bind(texture::Target::Texture2D).expect("Texture::bind");
        assert!(get_error().is_ok(), "after Texture::bind");

        // The four `parameter` calls `renderer/opengl/Texture.zig` makes on every
        // texture it creates, each with its typed value (`adf161473`'s
        // `value: name.Type()`).
        tbind
            .parameter(texture::Parameter::WrapS, texture::Wrap::ClampToEdge)
            .expect("Binding::parameter WrapS");
        tbind
            .parameter(texture::Parameter::WrapT, texture::Wrap::ClampToEdge)
            .expect("Binding::parameter WrapT");
        tbind
            .parameter(texture::Parameter::MinFilter, texture::MinFilter::Linear)
            .expect("Binding::parameter MinFilter");
        tbind
            .parameter(texture::Parameter::MagFilter, texture::MagFilter::Linear)
            .expect("Binding::parameter MagFilter");
        assert!(get_error().is_ok(), "after Binding::parameter");

        // A 2x2 RGBA texture with data — `metal.rs`'s texture smoke test shape.
        // Opaque red / green / blue / white, one texel each.
        #[rustfmt::skip]
        let pixels: [u8; 16] = [
            0xff, 0x00, 0x00, 0xff,   0x00, 0xff, 0x00, 0xff,
            0x00, 0x00, 0xff, 0xff,   0xff, 0xff, 0xff, 0xff,
        ];

        // SAFETY: `pixels` is 16 live bytes = 2 * 2 texels * 4 bytes for
        // (RGBA, UNSIGNED_BYTE), which is exactly what this call makes GL read at
        // the default unpack alignment. See `image_2d`'s safety contract.
        unsafe {
            tbind.image_2d(
                0,
                texture::InternalFormat::RGBA,
                2,
                2,
                texture::Format::RGBA,
                texture::DataType::UNSIGNED_BYTE,
                pixels.as_ptr().cast(),
            )
        }
        .expect("Binding::image_2d");
        assert!(get_error().is_ok(), "after Binding::image_2d");

        // `subImage2D` over the just-sized image — the incremental-upload path
        // `renderer/opengl/Texture.zig` drives. One texel at (1, 1).
        // SAFETY: a 1x1 (RGBA, UNSIGNED_BYTE) upload reads 4 bytes; `pixels` is 16.
        unsafe {
            tbind.sub_image_2d(
                0,
                1,
                1,
                1,
                1,
                texture::Format::RGBA,
                texture::DataType::UNSIGNED_BYTE,
                pixels.as_ptr().cast(),
            )
        }
        .expect("Binding::sub_image_2d");
        assert!(get_error().is_ok(), "after Binding::sub_image_2d");

        tbind.generate_mipmap();
        assert!(get_error().is_ok(), "after Binding::generate_mipmap");

        // --- Sampler.zig: create -> bind -> parameter -> destroy --------------
        let sampler = Sampler::create().expect("Sampler::create");
        assert!(get_error().is_ok(), "after Sampler::create");

        sampler.bind(0).expect("Sampler::bind");
        assert!(get_error().is_ok(), "after Sampler::bind");

        // `Sampler.parameter` takes `Texture.Parameter` — the reference shares the
        // one enum across both objects (`Sampler.zig:27`).
        sampler
            .parameter(texture::Parameter::MinFilter, texture::MinFilter::Linear)
            .expect("Sampler::parameter");
        assert!(get_error().is_ok(), "after Sampler::parameter");

        sampler.destroy();
        assert!(get_error().is_ok(), "after Sampler::destroy");

        // --- Renderbuffer.zig: create -> bind -> storage ----------------------
        let rbo = Renderbuffer::create().expect("Renderbuffer::create");
        assert!(get_error().is_ok(), "after Renderbuffer::create");

        let rbind = rbo.bind().expect("Renderbuffer::bind");
        assert!(get_error().is_ok(), "after Renderbuffer::bind");

        rbind
            .storage(texture::InternalFormat::RGBA, 2, 2)
            .expect("Renderbuffer::Binding::storage");
        assert!(get_error().is_ok(), "after Renderbuffer::Binding::storage");

        // --- Framebuffer.zig: create -> bind -> renderbuffer -> checkStatus ---
        let fbo = Framebuffer::create().expect("Framebuffer::create");
        assert!(get_error().is_ok(), "after Framebuffer::create");

        let fbind = fbo
            .bind(framebuffer::Target::FRAMEBUFFER)
            .expect("Framebuffer::bind");
        assert!(get_error().is_ok(), "after Framebuffer::bind");

        fbind
            .renderbuffer(framebuffer::Attachment::COLOR0, rbo)
            .expect("Framebuffer::Binding::renderbuffer");
        assert!(
            get_error().is_ok(),
            "after Framebuffer::Binding::renderbuffer"
        );

        // THE assertion of this test: a renderbuffer-backed framebuffer that GL
        // itself certifies as renderable.
        assert_eq!(
            fbind.check_status(),
            framebuffer::Status::COMPLETE,
            "renderbuffer-backed framebuffer is not complete",
        );

        // `drawBuffers` (`Framebuffer.zig:104-111`) over that attachment.
        fbind
            .draw_buffers(&[framebuffer::Attachment::COLOR0])
            .expect("Framebuffer::Binding::draw_buffers");
        assert!(
            get_error().is_ok(),
            "after Framebuffer::Binding::draw_buffers"
        );

        // `copySubImage2D` (`Texture.zig:199-220`) reads from the READ framebuffer
        // — which is now the complete, renderbuffer-backed one above, NOT the
        // texture, so there is no feedback loop. This is the only point in the
        // test where the call is legal, hence its placement here rather than with
        // the other `Texture.Binding` methods.
        tbind
            .copy_sub_image_2d(0, 0, 0, 0, 0, 1, 1)
            .expect("Binding::copy_sub_image_2d");
        assert!(get_error().is_ok(), "after Binding::copy_sub_image_2d");

        // `texture2D` (`Framebuffer.zig:73-88`) — the OTHER attachment path, which
        // is what `renderer/opengl/Target.zig` actually uses. Replaces the
        // renderbuffer at the same attachment point; must also be complete.
        fbind
            .texture_2d(
                framebuffer::Attachment::COLOR0,
                texture::Target::Texture2D,
                tex,
                0,
            )
            .expect("Framebuffer::Binding::texture_2d");
        assert!(
            get_error().is_ok(),
            "after Framebuffer::Binding::texture_2d"
        );

        assert_eq!(
            fbind.check_status(),
            framebuffer::Status::COMPLETE,
            "texture-backed framebuffer is not complete",
        );

        // --- extensions.zig: len -> iterator -> next --------------------------
        let n = extensions::len().expect("extensions::len");
        assert!(get_error().is_ok(), "after extensions::len");

        let mut it = extensions::iterator().expect("extensions::iterator");
        let mut seen = 0u32;
        while let Some(ext) = it.next().expect("Iterator::next") {
            // Every GL extension name is a non-empty ASCII token.
            assert!(!ext.is_empty(), "extension {seen} has an empty name");
            seen += 1;
        }
        assert_eq!(seen, n, "iterator yielded a different count than len()");
        assert!(get_error().is_ok(), "after exhausting extensions::Iterator");

        // A 4.3 core context always advertises at least one extension.
        assert!(n > 0, "a live GL 4.3 core context advertised no extensions");

        // --- teardown ---------------------------------------------------------
        fbind.unbind();
        assert!(get_error().is_ok(), "after Framebuffer::Binding::unbind");
        fbo.destroy();
        assert!(get_error().is_ok(), "after Framebuffer::destroy");

        rbind.unbind();
        assert!(get_error().is_ok(), "after Renderbuffer::Binding::unbind");
        rbo.destroy();
        assert!(get_error().is_ok(), "after Renderbuffer::destroy");

        tbind.unbind();
        assert!(get_error().is_ok(), "after Texture::Binding::unbind");
        tex.destroy();
        assert!(get_error().is_ok(), "after Texture::destroy");
    }
}
