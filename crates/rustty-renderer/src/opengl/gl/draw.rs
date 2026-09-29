//! PORTS: ghostty `pkg/opengl/draw.zig` (155 loc at `12752b2ac`), in full.
//!
//! PORT NOTE (`glad.context.<Fn>.?(…)`): every call below force-unwraps glad's
//! function-pointer slot in the reference, i.e. panics if the entry point is
//! absent. `glad::context` lends the calling thread's table, and an unloaded
//! entry point in it panics identically, so calling straight through is the
//! faithful port. See `errors::get_error`'s note.
//!
//! PORT NOTE (`void` returns): `clearColor`/`clear`/`finish`/`flush` return
//! `void` in the reference and nothing here. `viewport` used to be the one
//! `!void` that never called `errors.getError()`; `adf161473` made it check,
//! so it now can fail (P7-W7 T1).

use std::ffi::c_void;

use gl::types::{GLbitfield, GLenum, GLint, GLsizei};

use super::errors;
use super::glad;
use super::primitives::Primitive;
use super::texture;

/// PORTS: `draw.zig:6-8`.
///
/// ```zig
/// pub fn clearColor(r: f32, g: f32, b: f32, a: f32) void {
///     glad.context.ClearColor.?(r, g, b, a);
/// }
/// ```
pub fn clear_color(r: f32, g: f32, b: f32, a: f32) {
    // SAFETY: four floats to a no-op-on-bad-input GL setter; valid on any
    // current context.
    glad::context(|gl| unsafe { gl.ClearColor(r, g, b, a) });
}

/// PORTS: `draw.zig:10-12`.
///
/// ```zig
/// pub fn clear(mask: c.GLbitfield) void {
///     glad.context.Clear.?(mask);
/// }
/// ```
pub fn clear(mask: GLbitfield) {
    // SAFETY: a bitfield to a GL setter. An unrecognized bit yields
    // GL_INVALID_VALUE (readable via `errors::get_error`), never UB.
    glad::context(|gl| unsafe { gl.Clear(mask) });
}

/// PORTS: `draw.zig:14-17`.
///
/// ```zig
/// pub fn drawArrays(mode: c.GLenum, first: c.GLint, count: c.GLsizei) !void {
///     glad.context.DrawArrays.?(mode, first, count);
///     try errors.getError();
/// }
/// ```
///
/// PORT NOTE: unused by the reference's renderer (only `drawArraysInstanced` is
/// called, `renderer/opengl/RenderPass.zig:131`); carried over per the plan's
/// "port whole files" convention. Note it takes a raw `GLenum` where
/// [`draw_arrays_instanced`] takes a [`Primitive`] — that asymmetry is the
/// reference's, and is preserved.
pub fn draw_arrays(mode: GLenum, first: GLint, count: GLsizei) -> Result<(), errors::Error> {
    // SAFETY: the draw reads the currently-bound VAO/buffers. Every failure mode
    // (bad mode, negative count, incomplete VAO) is a defined GL error, which is
    // what `get_error` immediately collects — this is the reference's own
    // contract for the whole package.
    glad::context(|gl| unsafe { gl.DrawArrays(mode, first, count) });
    errors::get_error()
}

/// PORTS: `draw.zig:19-32`.
///
/// ```zig
/// pub fn drawArraysInstanced(
///     mode: Primitive,
///     first: c.GLint,
///     count: c.GLsizei,
///     primcount: c.GLsizei,
/// ) !void {
///     glad.context.DrawArraysInstanced.?(
///         @intCast(@intFromEnum(mode)),
///         first,
///         count,
///         primcount,
///     );
///     try errors.getError();
/// }
/// ```
pub fn draw_arrays_instanced(
    mode: Primitive,
    first: GLint,
    count: GLsizei,
    primcount: GLsizei,
) -> Result<(), errors::Error> {
    // `@intCast(@intFromEnum(mode))`: the `c_int`-backed enum's tag widened to
    // the `GLenum` (unsigned) the C entry point takes. All five tags are small
    // positive GL constants, so Zig's `@intCast` can never trap and the Rust
    // `as` can never wrap.
    //
    // SAFETY: see `draw_arrays`.
    glad::context(|gl| unsafe { gl.DrawArraysInstanced(mode as GLenum, first, count, primcount) });
    errors::get_error()
}

/// PORTS: `draw.zig:34-38`.
///
/// ```zig
/// pub fn drawElements(mode: c.GLenum, count: c.GLsizei, typ: c.GLenum, offset: usize) !void {
///     const offsetPtr = if (offset == 0) null else @as(*const anyopaque, @ptrFromInt(offset));
///     glad.context.DrawElements.?(mode, count, typ, offsetPtr);
///     try errors.getError();
/// }
/// ```
///
/// PORT NOTE: unused by the reference's renderer; carried over per "port whole
/// files".
pub fn draw_elements(
    mode: GLenum,
    count: GLsizei,
    typ: GLenum,
    offset: usize,
) -> Result<(), errors::Error> {
    // `@ptrFromInt(offset)`: with an ELEMENT_ARRAY_BUFFER bound, GL reads this
    // "pointer" as a byte OFFSET into that buffer, not as an address — hence the
    // int-to-pointer cast, which is the API's own convention. The `offset == 0`
    // branch exists so a zero offset is spelled `null` rather than an
    // (equivalent) zero pointer; preserved as-is.
    let offset_ptr: *const c_void = if offset == 0 {
        std::ptr::null()
    } else {
        offset as *const c_void
    };

    // SAFETY: see `draw_arrays`.
    glad::context(|gl| unsafe { gl.DrawElements(mode, count, typ, offset_ptr) });
    errors::get_error()
}

/// PORTS: `draw.zig:40-54`.
///
/// ```zig
/// pub fn drawElementsInstanced(
///     mode: c.GLenum,
///     count: c.GLsizei,
///     typ: c.GLenum,
///     primcount: c.GLsizei,
/// ) !void {
///     glad.context.DrawElementsInstanced.?(
///         mode,
///         count,
///         typ,
///         null,
///         primcount,
///     );
///     try errors.getError();
/// }
/// ```
///
/// PORT NOTE: unused by the reference's renderer; carried over per "port whole
/// files". Unlike [`draw_elements`] it takes no offset and hard-codes `null`
/// (offset 0) — the reference's asymmetry, preserved.
pub fn draw_elements_instanced(
    mode: GLenum,
    count: GLsizei,
    typ: GLenum,
    primcount: GLsizei,
) -> Result<(), errors::Error> {
    // SAFETY: see `draw_arrays`.
    glad::context(|gl| unsafe {
        gl.DrawElementsInstanced(mode, count, typ, std::ptr::null(), primcount)
    });
    errors::get_error()
}

/// PORTS: `draw.zig:56-59`.
///
/// ```zig
/// pub fn enable(cap: c.GLenum) !void {
///     glad.context.Enable.?(cap);
///     try errors.getError();
/// }
/// ```
pub fn enable(cap: GLenum) -> Result<(), errors::Error> {
    // SAFETY: a capability enum to a GL setter; an unknown one is
    // GL_INVALID_ENUM, collected immediately below.
    glad::context(|gl| unsafe { gl.Enable(cap) });
    errors::get_error()
}

/// PORTS: `draw.zig:61-64`.
///
/// ```zig
/// pub fn disable(cap: c.GLenum) !void {
///     glad.context.Disable.?(cap);
///     try errors.getError();
/// }
/// ```
pub fn disable(cap: GLenum) -> Result<(), errors::Error> {
    // SAFETY: see `enable`.
    glad::context(|gl| unsafe { gl.Disable(cap) });
    errors::get_error()
}

/// PORTS: `draw.zig:66-69`.
///
/// ```zig
/// pub fn frontFace(mode: c.GLenum) !void {
///     glad.context.FrontFace.?(mode);
///     try errors.getError();
/// }
/// ```
///
/// PORT NOTE: unused by the reference's renderer; carried over per "port whole
/// files".
pub fn front_face(mode: GLenum) -> Result<(), errors::Error> {
    // SAFETY: see `enable`.
    glad::context(|gl| unsafe { gl.FrontFace(mode) });
    errors::get_error()
}

/// PORTS: `draw.zig:71-74`.
///
/// ```zig
/// pub fn blendFunc(sfactor: c.GLenum, dfactor: c.GLenum) !void {
///     glad.context.BlendFunc.?(sfactor, dfactor);
///     try errors.getError();
/// }
/// ```
///
/// Load-bearing for pixel correctness: `renderer/opengl/RenderPass.zig:126`
/// drives it as `gl.blendFunc(gl.c.GL_ONE, gl.c.GL_ONE_MINUS_SRC_ALPHA)`
/// (Task 6).
pub fn blend_func(sfactor: GLenum, dfactor: GLenum) -> Result<(), errors::Error> {
    // SAFETY: see `enable`.
    glad::context(|gl| unsafe { gl.BlendFunc(sfactor, dfactor) });
    errors::get_error()
}

/// PORTS: `draw.zig:78-81` (`adf161473`).
///
/// ```zig
/// pub fn viewport(x: c.GLint, y: c.GLint, width: c.GLsizei, height: c.GLsizei) !void {
///     glad.context.Viewport.?(x, y, width, height);
///     try errors.getError();
/// }
/// ```
///
/// The reference's renderer now calls it from `OpenGL.setViewport`
/// (`OpenGL.zig:252`, stage S2); the port's offscreen constructor calls it too.
pub fn viewport(x: GLint, y: GLint, width: GLsizei, height: GLsizei) -> Result<(), errors::Error> {
    // SAFETY: four ints to a GL setter; a negative width/height is
    // GL_INVALID_VALUE, never UB, and is collected below.
    glad::context(|gl| unsafe { gl.Viewport(x, y, width, height) });
    errors::get_error()
}

/// PORTS: `draw.zig:83-102` (`adf161473`).
///
/// ```zig
/// pub fn readPixels(
///     x: c.GLint,
///     y: c.GLint,
///     width: c.GLsizei,
///     height: c.GLsizei,
///     format: Texture.Format,
///     typ: Texture.DataType,
///     data: ?*anyopaque,
/// ) !void {
///     glad.context.ReadPixels.?(
///         x,
///         y,
///         width,
///         height,
///         @intFromEnum(format),
///         @intFromEnum(typ),
///         data,
///     );
///     try errors.getError();
/// }
/// ```
///
/// # Safety
///
/// PORT NOTE (why `unsafe`, when the reference's fn is not): the DRIVER writes
/// through `data`, and how many bytes it writes follows from `width`/`height`/
/// `format`/`typ` and the current `GL_PACK_*` state, none of which the pointer
/// carries — the same obligation as `texture::Binding::image_2d`. Callers must
/// guarantee `data` is null or points to at least that many writable bytes.
pub unsafe fn read_pixels(
    x: GLint,
    y: GLint,
    width: GLsizei,
    height: GLsizei,
    format: texture::Format,
    typ: texture::DataType,
    data: *mut c_void,
) -> Result<(), errors::Error> {
    // SAFETY: `data`'s size is the caller's obligation, per the doc above. A bad
    // format/type pairing or an incomplete read framebuffer is a defined GL error,
    // collected below.
    glad::context(|gl| unsafe { gl.ReadPixels(x, y, width, height, format.0, typ.0, data) });
    errors::get_error()
}

/// PORTS: `draw.zig:104-129` (`adf161473`).
///
/// ```zig
/// pub fn blitFramebuffer(
///     src_x0: c.GLint,
///     src_y0: c.GLint,
///     src_x1: c.GLint,
///     src_y1: c.GLint,
///     dst_x0: c.GLint,
///     dst_y0: c.GLint,
///     dst_x1: c.GLint,
///     dst_y1: c.GLint,
///     mask: BlitMask,
///     filter: Texture.MagFilter,
/// ) !void {
///     glad.context.BlitFramebuffer.?(
///         src_x0,
///         src_y0,
///         src_x1,
///         src_y1,
///         dst_x0,
///         dst_y0,
///         dst_x1,
///         dst_y1,
///         @bitCast(mask),
///         @intCast(@intFromEnum(filter)),
///     );
///     try errors.getError();
/// }
/// ```
#[allow(clippy::too_many_arguments)]
pub fn blit_framebuffer(
    src_x0: GLint,
    src_y0: GLint,
    src_x1: GLint,
    src_y1: GLint,
    dst_x0: GLint,
    dst_y0: GLint,
    dst_x1: GLint,
    dst_y1: GLint,
    mask: BlitMask,
    filter: texture::MagFilter,
) -> Result<(), errors::Error> {
    // `@intCast(@intFromEnum(filter))`: the `c_int`-backed filter's tag to the
    // `GLenum` parameter; both tags are small positive GL constants.
    //
    // SAFETY: copies between the bound READ and DRAW framebuffers. Every failure
    // mode (an incomplete framebuffer, a format mismatch, a bad filter) is a
    // defined GL error, collected below.
    glad::context(|gl| unsafe {
        gl.BlitFramebuffer(
            src_x0,
            src_y0,
            src_x1,
            src_y1,
            dst_x0,
            dst_y0,
            dst_x1,
            dst_y1,
            mask.bits(),
            filter as GLenum,
        );
    });
    errors::get_error()
}

/// PORTS: `draw.zig:131-137`.
///
/// ```zig
/// pub fn pixelStore(mode: c.GLenum, value: anytype) !void {
///     switch (@typeInfo(@TypeOf(value))) {
///         .comptime_int, .int => glad.context.PixelStorei.?(mode, value),
///         else => unreachable,
///     }
///     try errors.getError();
/// }
/// ```
///
/// PORT NOTE (`value: anytype` -> `value: GLint`): the reference's `switch`
/// accepts only integer `value`s and declares everything else `unreachable`.
/// Rust has no `anytype`, and does not need one here: naming the parameter
/// `GLint` — the type `glPixelStorei` actually takes — makes the `unreachable`
/// arm a COMPILE error at the call site instead of a runtime trap, which is
/// strictly the same contract, enforced earlier.
///
/// (`adf161473` renamed the pre-0.14 `.ComptimeInt`/`.Int` tags, which UD40
/// recorded, to `.comptime_int`/`.int`.) Unused by the reference's renderer;
/// carried over per "port whole files".
pub fn pixel_store(mode: GLenum, value: GLint) -> Result<(), errors::Error> {
    // SAFETY: see `enable`.
    glad::context(|gl| unsafe { gl.PixelStorei(mode, value) });
    errors::get_error()
}

/// PORTS: `draw.zig:88-90`.
///
/// ```zig
/// pub fn finish() void {
///     glad.context.Finish.?();
/// }
/// ```
pub fn finish() {
    // SAFETY: no arguments; blocks until the GL pipeline drains.
    glad::context(|gl| unsafe { gl.Finish() });
}

/// PORTS: `draw.zig:92-94`.
///
/// ```zig
/// pub fn flush() void {
///     glad.context.Flush.?();
/// }
/// ```
pub fn flush() {
    // SAFETY: no arguments.
    glad::context(|gl| unsafe { gl.Flush() });
}

/// PORTS: `draw.zig:147-155` (`adf161473`).
///
/// ```zig
/// pub const BlitMask = packed struct(c.GLbitfield) {
///     _pad1: u8 = 0,
///     depth_buffer_bit: bool = false,
///     _pad2: u1 = 0,
///     stencil_buffer_bit: bool = false,
///     _pad3: u3 = 0,
///     color_buffer_bit: bool = false,
///     _pad4: std.meta.Int(.unsigned, @bitSizeOf(c.GLbitfield) - 15) = 0,
/// };
/// ```
///
/// PORT NOTE: Rust has no bit-packed struct, so the three flags are plain
/// `bool`s and [`BlitMask::bits`] is the `@bitCast`: bit 8
/// (`GL_DEPTH_BUFFER_BIT`, 0x100), bit 10 (`GL_STENCIL_BUFFER_BIT`, 0x400), bit
/// 14 (`GL_COLOR_BUFFER_BIT`, 0x4000), the positions the packed layout puts them
/// at. `Default` is the reference's all-`false` field defaults.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BlitMask {
    pub depth_buffer_bit: bool,
    pub stencil_buffer_bit: bool,
    pub color_buffer_bit: bool,
}

impl BlitMask {
    /// `@bitCast(mask)` to the `GLbitfield`.
    pub const fn bits(self) -> GLbitfield {
        ((self.depth_buffer_bit as GLbitfield) << 8)
            | ((self.stencil_buffer_bit as GLbitfield) << 10)
            | ((self.color_buffer_bit as GLbitfield) << 14)
    }
}

#[cfg(test)]
mod tests {
    use super::BlitMask;

    /// The packed layout's bit positions ARE the GL constants (the reference gets
    /// this from the compiler; the port restates it in `bits`).
    #[test]
    fn blit_mask_bits_are_the_gl_constants() {
        assert_eq!(BlitMask::default().bits(), 0);
        let only = |d, s, c| BlitMask {
            depth_buffer_bit: d,
            stencil_buffer_bit: s,
            color_buffer_bit: c,
        };
        assert_eq!(only(true, false, false).bits(), gl::DEPTH_BUFFER_BIT);
        assert_eq!(only(false, true, false).bits(), gl::STENCIL_BUFFER_BIT);
        assert_eq!(only(false, false, true).bits(), gl::COLOR_BUFFER_BIT);
        assert_eq!(
            only(true, true, true).bits(),
            gl::DEPTH_BUFFER_BIT | gl::STENCIL_BUFFER_BIT | gl::COLOR_BUFFER_BIT
        );
    }
}
