//! PORTS: ghostty `pkg/opengl/VertexArray.zig` (116 loc), in full.
//!
//! PORT NOTE (`glad.context.<Fn>.?(…)`): see `draw.rs`'s module note.
//!
//! PORT NOTE (by-value `self`): see `buffer.rs`'s module note — the reference's
//! methods take `v: VertexArray` / `_: Binding` by value.
//!
//! This is the LIVE vertex-attribute path (GL 4.3's separate attribute
//! format/binding API: `glVertexAttribFormat` + `glVertexAttribBinding` +
//! `glBindVertexBuffer`), as opposed to `Buffer.Binding`'s legacy
//! `glVertexAttribPointer` helpers. `renderer/opengl/Pipeline.zig` (Task 6) drives
//! every method below.

use gl::types::{GLenum, GLint, GLintptr, GLsizei, GLuint};

use super::errors;
use super::glad;

/// PORTS: `VertexArray.zig:1-7` — `const VertexArray = @This(); … id: c.GLuint`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct VertexArray {
    pub id: GLuint,
}

impl VertexArray {
    /// Create a single vertex array object.
    //
    // PORTS: `VertexArray.zig:9-14`.
    //
    // ```zig
    // pub fn create() !VertexArray {
    //     var vao: c.GLuint = undefined;
    //     glad.context.GenVertexArrays.?(1, &vao);
    //     return VertexArray{ .id = vao };
    // }
    // ```
    //
    // PORT NOTE: `!VertexArray` with no `getError` — an EMPTY error set, kept as a
    // `Result` so `try gl.VertexArray.create()` ports with its `?` intact. Same
    // handling (and same `undefined` -> `0` note) as `buffer::Buffer::create`.
    pub fn create() -> Result<Self, errors::Error> {
        let mut vao: GLuint = 0;
        // SAFETY: `glGenVertexArrays` writes exactly `n` (= 1) GLuints through
        // the pointer; `&mut vao` is one live, aligned GLuint.
        glad::context(|gl| unsafe { gl.GenVertexArrays(1, &mut vao) });
        Ok(VertexArray { id: vao })
    }

    /// glBindVertexArray
    //
    // PORTS: `VertexArray.zig:16-21`.
    //
    // ```zig
    // pub fn bind(v: VertexArray) !Binding {
    //     glad.context.BindVertexArray.?(v.id);
    //     try errors.getError();
    //     return .{};
    // }
    // ```
    //
    // PORT NOTE: unlike `Buffer.bind`, this one DOES collect the error — so its
    // `Result` is real, not an empty-set formality. The reference's asymmetry,
    // preserved.
    pub fn bind(self) -> Result<Binding, errors::Error> {
        // SAFETY: binds a name this `VertexArray` owns. A name that was never
        // generated is GL_INVALID_OPERATION, collected immediately below.
        glad::context(|gl| unsafe { gl.BindVertexArray(self.id) });
        errors::get_error()?;
        Ok(Binding)
    }

    /// PORTS: `VertexArray.zig:23-25`.
    ///
    /// ```zig
    /// pub fn destroy(v: VertexArray) void {
    ///     glad.context.DeleteVertexArrays.?(1, &v.id);
    /// }
    /// ```
    ///
    /// PORT NOTE: by-value `self`, NOT a `Drop` impl — see
    /// `buffer::Buffer::destroy`'s note.
    pub fn destroy(self) {
        // SAFETY: `glDeleteVertexArrays` reads exactly `n` (= 1) GLuints through
        // the pointer; `&self.id` is one live, aligned GLuint.
        glad::context(|gl| unsafe { gl.DeleteVertexArrays(1, &self.id) });
    }
}

/// PORTS: `VertexArray.zig:27-116` — `pub const Binding = struct { … }`.
///
/// PORT NOTE: a FIELD-LESS struct in the reference (`return .{};`,
/// `VertexArray.zig:20`) — it carries no VAO id, because every method below acts
/// on whatever VAO is currently bound. Ported as a unit struct, which is
/// constructible from anywhere exactly as the reference's `.{}` is. It is a
/// "you have bound something" token, not a handle.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Binding;

impl Binding {
    /// PORTS: `VertexArray.zig:28-31`.
    ///
    /// ```zig
    /// pub fn unbind(self: Binding) void {
    ///     _ = self;
    ///     glad.context.BindVertexArray.?(0);
    /// }
    /// ```
    pub fn unbind(self) {
        // SAFETY: binding VAO name 0 always succeeds.
        glad::context(|gl| unsafe { gl.BindVertexArray(0) });
    }

    /// PORTS: `VertexArray.zig:33-36`.
    ///
    /// ```zig
    /// pub fn enableAttribArray(_: Binding, idx: c.GLuint) !void {
    ///     glad.context.EnableVertexAttribArray.?(idx);
    ///     try errors.getError();
    /// }
    /// ```
    ///
    /// PORT NOTE (`_: Binding`): the reference DISCARDS the receiver — the call
    /// acts on the currently-bound VAO, not on anything the `Binding` carries
    /// (it carries nothing). Rust only grants method-call syntax to a receiver
    /// literally named `self`, so it is named and simply goes unused — which is
    /// exactly what the reference itself does one method up, in
    /// `unbind(self: Binding) { _ = self; … }` (`VertexArray.zig:28-31`). Same
    /// for every method below.
    pub fn enable_attrib_array(self, idx: GLuint) -> Result<(), errors::Error> {
        // SAFETY: acts on the currently-bound VAO. Every failure mode (no VAO
        // bound, index >= GL_MAX_VERTEX_ATTRIBS) is a defined GL error, collected
        // below.
        glad::context(|gl| unsafe { gl.EnableVertexAttribArray(idx) });
        errors::get_error()
    }

    /// PORTS: `VertexArray.zig:38-41`.
    ///
    /// ```zig
    /// pub fn bindingDivisor(_: Binding, idx: c.GLuint, divisor: c.GLuint) !void {
    ///     glad.context.VertexBindingDivisor.?(idx, divisor);
    ///     try errors.getError();
    /// }
    /// ```
    ///
    /// The per-instance step rate for a vertex BINDING point (`glVertexBinding-
    /// Divisor`) — the 4.3 separate-format counterpart of `Buffer.Binding`'s
    /// per-ATTRIBUTE `glVertexAttribDivisor`.
    pub fn binding_divisor(self, idx: GLuint, divisor: GLuint) -> Result<(), errors::Error> {
        // SAFETY: see `enable_attrib_array`.
        glad::context(|gl| unsafe { gl.VertexBindingDivisor(idx, divisor) });
        errors::get_error()
    }

    /// PORTS: `VertexArray.zig:43-50`.
    ///
    /// ```zig
    /// pub fn attributeBinding(
    ///     _: Binding,
    ///     attrib_idx: c.GLuint,
    ///     binding_idx: c.GLuint,
    /// ) !void {
    ///     glad.context.VertexAttribBinding.?(attrib_idx, binding_idx);
    ///     try errors.getError();
    /// }
    /// ```
    pub fn attribute_binding(
        self,
        attrib_idx: GLuint,
        binding_idx: GLuint,
    ) -> Result<(), errors::Error> {
        // SAFETY: see `enable_attrib_array`.
        glad::context(|gl| unsafe { gl.VertexAttribBinding(attrib_idx, binding_idx) });
        errors::get_error()
    }

    /// PORTS: `VertexArray.zig:52-68`.
    ///
    /// ```zig
    /// pub fn attributeFormat(
    ///     _: Binding,
    ///     idx: c.GLuint,
    ///     size: c.GLint,
    ///     typ: c.GLenum,
    ///     normalized: bool,
    ///     offset: c.GLuint,
    /// ) !void {
    ///     glad.context.VertexAttribFormat.?(
    ///         idx,
    ///         size,
    ///         typ,
    ///         @intCast(@intFromBool(normalized)),
    ///         offset,
    ///     );
    ///     try errors.getError();
    /// }
    /// ```
    pub fn attribute_format(
        self,
        idx: GLuint,
        size: GLint,
        typ: GLenum,
        normalized: bool,
        offset: GLuint,
    ) -> Result<(), errors::Error> {
        // `@intCast(@intFromBool(normalized))`: bool -> GLboolean (u8).
        let normalized_c: gl::types::GLboolean = if normalized { gl::TRUE } else { gl::FALSE };
        // SAFETY: see `enable_attrib_array`.
        glad::context(|gl| unsafe { gl.VertexAttribFormat(idx, size, typ, normalized_c, offset) });
        errors::get_error()
    }

    /// PORTS: `VertexArray.zig:70-84`.
    ///
    /// ```zig
    /// pub fn attributeIFormat(
    ///     _: Binding,
    ///     idx: c.GLuint,
    ///     size: c.GLint,
    ///     typ: c.GLenum,
    ///     offset: c.GLuint,
    /// ) !void {
    ///     glad.context.VertexAttribIFormat.?(
    ///         idx,
    ///         size,
    ///         typ,
    ///         offset,
    ///     );
    ///     try errors.getError();
    /// }
    /// ```
    ///
    /// The INTEGER attribute format (no normalize/convert-to-float step) — hence
    /// no `normalized` parameter.
    pub fn attribute_i_format(
        self,
        idx: GLuint,
        size: GLint,
        typ: GLenum,
        offset: GLuint,
    ) -> Result<(), errors::Error> {
        // SAFETY: see `enable_attrib_array`.
        glad::context(|gl| unsafe { gl.VertexAttribIFormat(idx, size, typ, offset) });
        errors::get_error()
    }

    /// PORTS: `VertexArray.zig:86-99`.
    ///
    /// ```zig
    /// pub fn attributeLFormat(
    ///     _: Binding,
    ///     idx: c.GLuint,
    ///     size: c.GLint,
    ///     offset: c.GLuint,
    /// ) !void {
    ///     glad.context.VertexAttribLFormat.?(
    ///         idx,
    ///         size,
    ///         c.GL_DOUBLE,
    ///         offset,
    ///     );
    ///     try errors.getError();
    /// }
    /// ```
    ///
    /// The `L` (long/double) attribute format. Takes no `typ`: the reference
    /// hard-codes `GL_DOUBLE`, which is the only type `glVertexAttribLFormat`
    /// accepts. Preserved exactly.
    pub fn attribute_l_format(
        self,
        idx: GLuint,
        size: GLint,
        offset: GLuint,
    ) -> Result<(), errors::Error> {
        // SAFETY: see `enable_attrib_array`.
        glad::context(|gl| unsafe { gl.VertexAttribLFormat(idx, size, gl::DOUBLE, offset) });
        errors::get_error()
    }

    /// PORTS: `VertexArray.zig:101-115`.
    ///
    /// ```zig
    /// pub fn bindVertexBuffer(
    ///     _: Binding,
    ///     idx: c.GLuint,
    ///     buffer: c.GLuint,
    ///     offset: c.GLintptr,
    ///     stride: c.GLsizei,
    /// ) !void {
    ///     glad.context.BindVertexBuffer.?(
    ///         idx,
    ///         buffer,
    ///         offset,
    ///         stride,
    ///     );
    ///     try errors.getError();
    /// }
    /// ```
    ///
    /// PORT NOTE (`buffer: c.GLuint`): a RAW buffer name, not a
    /// [`super::buffer::Buffer`] — the reference's call sites pass
    /// `buf.buffer.id`. Kept as `GLuint` so they port unchanged.
    pub fn bind_vertex_buffer(
        self,
        idx: GLuint,
        buffer: GLuint,
        offset: GLintptr,
        stride: GLsizei,
    ) -> Result<(), errors::Error> {
        // SAFETY: see `enable_attrib_array`.
        glad::context(|gl| unsafe { gl.BindVertexBuffer(idx, buffer, offset, stride) });
        errors::get_error()
    }
}
