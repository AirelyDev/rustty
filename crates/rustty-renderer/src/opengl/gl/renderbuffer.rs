//! PORTS: ghostty `pkg/opengl/Renderbuffer.zig` (56 loc), in full.
//!
//! PORT NOTE (`glad.context.<Fn>.?(…)`): see `draw.rs`'s module note.
//!
//! PORT NOTE (by-value `self`): see `buffer.rs`'s module note.
//!
//! PORT NOTE (`const Texture = @import("Texture.zig");`, `Renderbuffer.zig:8`):
//! the import is NOT unused — `Binding.storage` takes a `Texture.InternalFormat`.
//! Ported as the [`texture`] import below.

use gl::types::{GLenum, GLint, GLsizei, GLuint};

use super::errors;
use super::glad;
use super::texture;

/// PORTS: `Renderbuffer.zig:1-10` — `const Renderbuffer = @This(); … id: c.GLuint`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Renderbuffer {
    pub id: GLuint,
}

impl Renderbuffer {
    /// Create a single buffer.
    //
    // PORTS: `Renderbuffer.zig:12-17`.
    //
    // ```zig
    // pub fn create() !Renderbuffer {
    //     var rbo: c.GLuint = undefined;
    //     glad.context.GenRenderbuffers.?(1, &rbo);
    //     return .{ .id = rbo };
    // }
    // ```
    //
    // (The "Create a single buffer" doc comment — on a RENDERbuffer — is the
    // reference's own copy/paste from `Framebuffer.zig:12`. Carried over.)
    //
    // PORT NOTE (`!Renderbuffer` with no `getError`): the inferred error set is
    // EMPTY — this cannot fail. The `Result` is kept so the reference's `try`
    // call sites port with their `?` intact. Same handling as
    // `buffer::Buffer::create`, and unlike `texture::Texture::create`, which does
    // collect.
    pub fn create() -> Result<Self, errors::Error> {
        let mut rbo: GLuint = 0;
        // SAFETY: `glGenRenderbuffers` writes exactly `n` (= 1) GLuints through
        // the pointer; `&mut rbo` is one live, aligned GLuint.
        glad::context(|gl| unsafe { gl.GenRenderbuffers(1, &mut rbo) });
        Ok(Renderbuffer { id: rbo })
    }

    /// PORTS: `Renderbuffer.zig:19-21`.
    ///
    /// ```zig
    /// pub fn destroy(v: Renderbuffer) void {
    ///     glad.context.DeleteRenderbuffers.?(1, &v.id);
    /// }
    /// ```
    ///
    /// PORT NOTE: by-value `self`, NOT a `Drop` impl — see
    /// `buffer::Buffer::destroy`'s note.
    pub fn destroy(self) {
        // SAFETY: `glDeleteRenderbuffers` reads exactly `n` (= 1) GLuints through
        // the pointer; `&self.id` is one live, aligned GLuint. Deleting an
        // already-deleted or unknown name is silently ignored by GL, not UB.
        glad::context(|gl| unsafe { gl.DeleteRenderbuffers(1, &self.id) });
    }

    /// PORTS: `Renderbuffer.zig:23-29`.
    ///
    /// ```zig
    /// pub fn bind(v: Renderbuffer) !Binding {
    ///     // Keep track of the previous binding so we can restore it in unbind.
    ///     var current: c.GLint = undefined;
    ///     glad.context.GetIntegerv.?(c.GL_RENDERBUFFER_BINDING, &current);
    ///     glad.context.BindRenderbuffer.?(c.GL_RENDERBUFFER, v.id);
    ///     return .{ .previous = @intCast(current) };
    /// }
    /// ```
    ///
    /// PORT NOTE (no `target` parameter): `GL_RENDERBUFFER` is the only legal
    /// renderbuffer target, so the reference hard-codes it — hence, unlike
    /// `texture::Texture::bind`, there is nothing to pass and nothing for the
    /// [`Binding`] to remember beyond `previous`.
    ///
    /// PORT NOTE (`previous`): this `bind` SAVES AND RESTORES rather than
    /// unbinding to 0, which is why [`Binding::unbind`] is not the no-op
    /// `BindRenderbuffer(…, 0)` that `buffer::Binding::unbind` is. Same rationale
    /// as `framebuffer::Framebuffer::bind`.
    pub fn bind(self) -> Result<Binding, errors::Error> {
        // Keep track of the previous binding so we can restore it in unbind.
        let mut current: GLint = 0;
        // SAFETY: `GL_RENDERBUFFER_BINDING` is a single-valued query, so
        // `glGetIntegerv` writes exactly one GLint through the pointer; `&mut
        // current` is one live, aligned GLint.
        glad::context(|gl| unsafe { gl.GetIntegerv(gl::RENDERBUFFER_BINDING, &mut current) });
        // SAFETY: binds a name this `Renderbuffer` owns to the only legal target.
        glad::context(|gl| unsafe { gl.BindRenderbuffer(gl::RENDERBUFFER, self.id) });
        // `@intCast(current)`: GLint -> GLuint, trapping on a negative in Zig.
        // `try_from` panics identically. A GL object name is never negative.
        let previous =
            GLuint::try_from(current).expect("GL_RENDERBUFFER_BINDING returned a negative name");
        Ok(Binding { previous })
    }
}

/// PORTS: `Renderbuffer.zig:31-56` — `pub const Binding = struct { previous: c.GLuint, … }`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Binding {
    pub previous: GLuint,
}

impl Binding {
    /// PORTS: `Renderbuffer.zig:34-39`.
    ///
    /// ```zig
    /// pub fn unbind(self: Binding) void {
    ///     glad.context.BindRenderbuffer.?(
    ///         c.GL_RENDERBUFFER,
    ///         self.previous,
    ///     );
    /// }
    /// ```
    ///
    /// RESTORES the previous binding rather than binding 0 — see
    /// [`Renderbuffer::bind`]'s note.
    pub fn unbind(self) {
        // SAFETY: rebinds whatever name was bound before. If that name has since
        // been deleted, GL raises GL_INVALID_OPERATION (which the reference does
        // not collect here) — never UB.
        glad::context(|gl| unsafe { gl.BindRenderbuffer(gl::RENDERBUFFER, self.previous) });
    }

    /// PORTS: `Renderbuffer.zig:41-55`.
    ///
    /// ```zig
    /// pub fn storage(
    ///     self: Binding,
    ///     format: Texture.InternalFormat,
    ///     width: c.GLsizei,
    ///     height: c.GLsizei,
    /// ) !void {
    ///     _ = self;
    ///     glad.context.RenderbufferStorage.?(
    ///         c.GL_RENDERBUFFER,
    ///         @intCast(@intFromEnum(format)),
    ///         width,
    ///         height,
    ///     );
    ///     try errors.getError();
    /// }
    /// ```
    ///
    /// PORT NOTE (`_ = self`): the reference DISCARDS the receiver — the call acts
    /// on whatever is bound to `GL_RENDERBUFFER`, and the `Binding` is a bind-state
    /// token only. Rust grants method-call syntax only to a receiver literally
    /// named `self`, so it is named and simply goes unused — the same shape
    /// `buffer::Binding::attribute_divisor` uses.
    ///
    /// PORT NOTE (`@intCast(@intFromEnum(format))`): the cast is REAL, not
    /// incidental — `InternalFormat` is an `enum(c_int)` but `glRenderbufferStorage`
    /// takes a `GLenum` (`c_uint`), so the reference must widen the sign. Ported as
    /// `format.0 as GLenum`. Zig's `@intCast` traps on a negative; every named
    /// `InternalFormat` constant is a positive GL enum, so the two agree.
    pub fn storage(
        self,
        format: texture::InternalFormat,
        width: GLsizei,
        height: GLsizei,
    ) -> Result<(), errors::Error> {
        // SAFETY: allocates storage for the bound renderbuffer. An unsupported
        // format or an out-of-range size is a defined GL error, collected below.
        glad::context(|gl| unsafe {
            gl.RenderbufferStorage(gl::RENDERBUFFER, format.0 as GLenum, width, height)
        });
        errors::get_error()
    }
}
