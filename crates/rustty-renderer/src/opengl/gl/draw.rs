//! PORTS: ghostty `pkg/opengl/draw.zig` (94 loc), in full.
//!
//! PORT NOTE (`glad.context.<Fn>.?(…)`): every call below force-unwraps glad's
//! function-pointer slot in the reference, i.e. panics if the entry point is
//! absent. The `gl` crate's unloaded entry points panic identically, so calling
//! them straight through is the faithful port. See `errors::get_error`'s note.
//!
//! PORT NOTE (`!void` with no `getError`): `clearColor`/`clear`/`finish`/`flush`
//! return `void` in the reference, and `viewport` returns `!void` while never
//! calling `errors.getError()` — so its inferred error set is EMPTY and `try
//! viewport(…)` can never actually fail. Both shapes are preserved exactly:
//! the `void` ones return nothing, and `viewport` returns a `Result` that is
//! always `Ok`. Collapsing `viewport` to `()` would silently change the
//! signature a later task ports its call sites against.

use std::ffi::c_void;

use gl::types::{GLbitfield, GLenum, GLint, GLsizei};

use super::errors;
use super::primitives::Primitive;

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
    unsafe { gl::ClearColor(r, g, b, a) };
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
    unsafe { gl::Clear(mask) };
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
    unsafe { gl::DrawArrays(mode, first, count) };
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
    unsafe { gl::DrawArraysInstanced(mode as GLenum, first, count, primcount) };
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
    unsafe { gl::DrawElements(mode, count, typ, offset_ptr) };
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
    unsafe { gl::DrawElementsInstanced(mode, count, typ, std::ptr::null(), primcount) };
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
    unsafe { gl::Enable(cap) };
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
    unsafe { gl::Disable(cap) };
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
    unsafe { gl::FrontFace(mode) };
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
    unsafe { gl::BlendFunc(sfactor, dfactor) };
    errors::get_error()
}

/// PORTS: `draw.zig:76-78`.
///
/// ```zig
/// pub fn viewport(x: c.GLint, y: c.GLint, width: c.GLsizei, height: c.GLsizei) !void {
///     glad.context.Viewport.?(x, y, width, height);
/// }
/// ```
///
/// PORT NOTE: the ONLY `!void` in this file with no `try errors.getError()`, so
/// its Zig error set is empty and it cannot fail. The `Result` is kept anyway —
/// see the module docs. Unused by the reference's renderer, which reads
/// `GL_VIEWPORT` rather than setting it (`OpenGL.zig:278-287`); carried over per
/// "port whole files".
pub fn viewport(x: GLint, y: GLint, width: GLsizei, height: GLsizei) -> Result<(), errors::Error> {
    // SAFETY: four ints to a GL setter; a negative width/height is
    // GL_INVALID_VALUE, never UB. (The reference does not collect it — see above.)
    unsafe { gl::Viewport(x, y, width, height) };
    Ok(())
}

/// PORTS: `draw.zig:80-86`.
///
/// ```zig
/// pub fn pixelStore(mode: c.GLenum, value: anytype) !void {
///     switch (@typeInfo(@TypeOf(value))) {
///         .ComptimeInt, .Int => glad.context.PixelStorei.?(mode, value),
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
/// PORT NOTE (stale in the reference): `.ComptimeInt`/`.Int` are the PRE-0.14
/// `@typeInfo` tag spellings (today's are `.comptime_int`/`.int`, as
/// `Buffer.zig:117-131`'s `.pointer`/`.slice` show). Zig only type-checks a
/// generic body when it is instantiated, so this fn has had no call site since
/// the rename and would not compile if called. It is ported to its evident
/// intent. Unused by the reference's renderer; carried over per "port whole
/// files".
pub fn pixel_store(mode: GLenum, value: GLint) -> Result<(), errors::Error> {
    // SAFETY: see `enable`.
    unsafe { gl::PixelStorei(mode, value) };
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
    unsafe { gl::Finish() };
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
    unsafe { gl::Flush() };
}
