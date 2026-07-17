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
//! [`primitives`]/[`draw`]. The remaining leaves (`Framebuffer`/`Program`/
//! `Renderbuffer`/`Sampler`/`Shader`/`Texture`/`extensions`) are Tasks 3-4.
//!
//! PORT NOTE (`pub const c` / `pub const glad`, `main.zig:14-15`): no analogue.
//! Both name ghostty's own vendored glad artifacts; the `gl` crate IS that layer
//! here and is reachable as `::gl` from anywhere in the crate, so re-exporting it
//! under a second name would add a spelling the reference does not have.

pub mod buffer;
pub mod draw;
pub mod errors;
pub mod primitives;
pub mod sys;
pub mod vertex_array;

// RUSTTY-ONLY: the `#[cfg(test)]` GL-context scaffolding shared by this module's
// leaf tests and `sys`'s. Not part of `main.zig`'s surface.
#[cfg(test)]
pub(crate) mod test_support;

// pub const Buffer = @import("Buffer.zig");
pub use buffer::Buffer;
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
// pub const pixelStore = draw.pixelStore;
// pub const viewport = draw.viewport;
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
    blend_func, clear, clear_color, disable, draw_arrays, draw_arrays_instanced, draw_elements,
    draw_elements_instanced, enable, finish, flush, front_face, pixel_store, viewport,
};

#[cfg(test)]
mod tests {
    use super::buffer::{Buffer, Target, Usage};
    use super::errors::get_error;
    use super::test_support;
    use super::vertex_array::VertexArray;

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
}
