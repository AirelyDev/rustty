//! PORTS: ghostty `pkg/opengl/Framebuffer.zig` (116 loc), in full.
//!
//! PORT NOTE (`glad.context.<Fn>.?(…)`): see `draw.rs`'s module note.
//!
//! PORT NOTE (by-value `self`): see `buffer.rs`'s module note.

use gl::types::{GLenum, GLint, GLsizei, GLuint};

use super::errors;
use super::renderbuffer::Renderbuffer;
use super::texture::{self, Texture};

/// PORTS: `Framebuffer.zig:1-10` — `const Framebuffer = @This(); … id: c.GLuint`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Framebuffer {
    pub id: GLuint,
}

impl Framebuffer {
    /// Create a single buffer.
    //
    // PORTS: `Framebuffer.zig:12-17`.
    //
    // ```zig
    // pub fn create() !Framebuffer {
    //     var fbo: c.GLuint = undefined;
    //     glad.context.GenFramebuffers.?(1, &fbo);
    //     return .{ .id = fbo };
    // }
    // ```
    //
    // PORT NOTE (`!Framebuffer` with no `getError`): an EMPTY inferred error set —
    // this cannot fail. The `Result` is kept so the reference's `try` call sites
    // port with their `?` intact. Same handling as `buffer::Buffer::create`.
    pub fn create() -> Result<Self, errors::Error> {
        let mut fbo: GLuint = 0;
        // SAFETY: `glGenFramebuffers` writes exactly `n` (= 1) GLuints through the
        // pointer; `&mut fbo` is one live, aligned GLuint.
        unsafe { gl::GenFramebuffers(1, &mut fbo) };
        Ok(Framebuffer { id: fbo })
    }

    /// PORTS: `Framebuffer.zig:19-21`.
    ///
    /// ```zig
    /// pub fn destroy(v: Framebuffer) void {
    ///     glad.context.DeleteFramebuffers.?(1, &v.id);
    /// }
    /// ```
    ///
    /// PORT NOTE: by-value `self`, NOT a `Drop` impl — see
    /// `buffer::Buffer::destroy`'s note.
    pub fn destroy(self) {
        // SAFETY: `glDeleteFramebuffers` reads exactly `n` (= 1) GLuints through
        // the pointer; `&self.id` is one live, aligned GLuint. Deleting an
        // already-deleted or unknown name is silently ignored by GL, not UB.
        unsafe { gl::DeleteFramebuffers(1, &self.id) };
    }

    /// PORTS: `Framebuffer.zig:23-31`.
    ///
    /// ```zig
    /// pub fn bind(v: Framebuffer, target: Target) !Binding {
    ///     // The default framebuffer is documented as being zero but
    ///     // on multiple OpenGL drivers its not zero, so we grab it
    ///     // at runtime.
    ///     var current: c.GLint = undefined;
    ///     glad.context.GetIntegerv.?(c.GL_FRAMEBUFFER_BINDING, &current);
    ///     glad.context.BindFramebuffer.?(@intFromEnum(target), v.id);
    ///     return .{ .target = target, .previous = @intCast(current) };
    /// }
    /// ```
    ///
    /// PORT NOTE (`previous`): the comment above is the reference's own and is the
    /// whole reason [`Binding::unbind`] restores a saved name instead of binding 0
    /// the way `buffer::Binding::unbind` does. Ported verbatim — "bind 0 to
    /// unbind" is exactly the assumption it exists to refute.
    ///
    /// PORT NOTE (`GL_FRAMEBUFFER_BINDING` regardless of `target`): the query is
    /// hard-coded and does NOT vary with the target, even though GL has distinct
    /// `GL_DRAW_FRAMEBUFFER_BINDING`/`GL_READ_FRAMEBUFFER_BINDING` queries (and
    /// `GL_FRAMEBUFFER_BINDING` is an alias of the DRAW one). So binding with
    /// `.read` and then unbinding restores the saved DRAW name onto the READ
    /// target. Every call site in the reference binds `.framebuffer`
    /// (`renderer/opengl/Target.zig`), where the two coincide, so the latent
    /// mismatch never fires. Carried over as-is rather than "corrected".
    pub fn bind(self, target: Target) -> Result<Binding, errors::Error> {
        // The default framebuffer is documented as being zero but
        // on multiple OpenGL drivers its not zero, so we grab it
        // at runtime.
        let mut current: GLint = 0;
        // SAFETY: `GL_FRAMEBUFFER_BINDING` is a single-valued query, so
        // `glGetIntegerv` writes exactly one GLint through the pointer; `&mut
        // current` is one live, aligned GLint.
        unsafe { gl::GetIntegerv(gl::FRAMEBUFFER_BINDING, &mut current) };
        // SAFETY: binds a name this `Framebuffer` owns to a GL target. A bad
        // target is GL_INVALID_ENUM (which the reference does not collect here).
        unsafe { gl::BindFramebuffer(target.0, self.id) };
        // `@intCast(current)`: GLint -> GLuint, trapping on a negative in Zig.
        // `try_from` panics identically. A GL object name is never negative.
        let previous =
            GLuint::try_from(current).expect("GL_FRAMEBUFFER_BINDING returned a negative name");
        Ok(Binding { target, previous })
    }
}

/// Enum for possible binding targets.
//
// PORTS: `Framebuffer.zig:33-39`.
//
// ```zig
// pub const Target = enum(c_uint) {
//     framebuffer = c.GL_FRAMEBUFFER,
//     draw = c.GL_DRAW_FRAMEBUFFER,
//     read = c.GL_READ_FRAMEBUFFER,
//     _,
// };
// ```
//
// PORT NOTE (the trailing `_`): a Zig NON-EXHAUSTIVE enum -> a newtype over the
// tag type with the named values as associated constants. See `buffer::Target`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Target(pub GLenum);

impl Target {
    pub const FRAMEBUFFER: Self = Self(gl::FRAMEBUFFER);
    pub const DRAW: Self = Self(gl::DRAW_FRAMEBUFFER);
    pub const READ: Self = Self(gl::READ_FRAMEBUFFER);
}

/// PORTS: `Framebuffer.zig:41-47`.
//
// ```zig
// pub const Attachment = enum(c_uint) {
//     color0 = c.GL_COLOR_ATTACHMENT0,
//     depth = c.GL_DEPTH_ATTACHMENT,
//     stencil = c.GL_STENCIL_ATTACHMENT,
//     depth_stencil = c.GL_DEPTH_STENCIL_ATTACHMENT,
//     _,
// };
// ```
//
// PORT NOTE (the trailing `_`): NON-EXHAUSTIVE -> a newtype. It carries real
// weight for this enum specifically: `GL_COLOR_ATTACHMENT1`, `…2`, … are exactly
// the values the open tail is for, and `Attachment(gl::COLOR_ATTACHMENT0 + n)` is
// how a caller reaches them — the same thing `@enumFromInt` does there.
//
// PORT NOTE (`#[repr(transparent)]`): REQUIRED, not cosmetic —
// [`Binding::draw_buffers`] hands `&[Attachment]` to `glDrawBuffers` as a
// `*const GLenum`, which is sound only if `Attachment` and `GLenum` have
// identical layout. The reference gets that for free from `enum(c_uint)`'s
// guaranteed representation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(transparent)]
pub struct Attachment(pub GLenum);

impl Attachment {
    pub const COLOR0: Self = Self(gl::COLOR_ATTACHMENT0);
    pub const DEPTH: Self = Self(gl::DEPTH_ATTACHMENT);
    pub const STENCIL: Self = Self(gl::STENCIL_ATTACHMENT);
    pub const DEPTH_STENCIL: Self = Self(gl::DEPTH_STENCIL_ATTACHMENT);
}

/// PORTS: `Framebuffer.zig:49-60`.
//
// ```zig
// pub const Status = enum(c_uint) {
//     complete = c.GL_FRAMEBUFFER_COMPLETE,
//     undefined = c.GL_FRAMEBUFFER_UNDEFINED,
//     incomplete_attachment = c.GL_FRAMEBUFFER_INCOMPLETE_ATTACHMENT,
//     incomplete_missing_attachment = c.GL_FRAMEBUFFER_INCOMPLETE_MISSING_ATTACHMENT,
//     incomplete_draw_buffer = c.GL_FRAMEBUFFER_INCOMPLETE_DRAW_BUFFER,
//     incomplete_read_buffer = c.GL_FRAMEBUFFER_INCOMPLETE_READ_BUFFER,
//     unsupported = c.GL_FRAMEBUFFER_UNSUPPORTED,
//     incomplete_multisample = c.GL_FRAMEBUFFER_INCOMPLETE_MULTISAMPLE,
//     incomplete_layer_targets = c.GL_FRAMEBUFFER_INCOMPLETE_LAYER_TARGETS,
//     _,
// };
// ```
//
// PORT NOTE (the trailing `_`): NON-EXHAUSTIVE -> a newtype, and here that is what
// keeps [`Binding::check_status`] SOUND. `checkStatus` does a bare
// `@enumFromInt` of whatever the driver returns; on a Rust `enum` an unlisted
// value would be instant UB, and `glCheckFramebufferStatus` returns 0 on error —
// a value NOT in the list above. The newtype models the open tail natively, so
// the reference's unchecked conversion ports without either a UB hazard or an
// invented error path.
//
// PORT NOTE (`undefined` -> `UNDEFINED`): `undefined` is a Zig keyword, which is
// why the reference must write `.undefined` at use sites. In Rust it is an
// ordinary identifier and needs no escape.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Status(pub GLenum);

impl Status {
    pub const COMPLETE: Self = Self(gl::FRAMEBUFFER_COMPLETE);
    pub const UNDEFINED: Self = Self(gl::FRAMEBUFFER_UNDEFINED);
    pub const INCOMPLETE_ATTACHMENT: Self = Self(gl::FRAMEBUFFER_INCOMPLETE_ATTACHMENT);
    pub const INCOMPLETE_MISSING_ATTACHMENT: Self =
        Self(gl::FRAMEBUFFER_INCOMPLETE_MISSING_ATTACHMENT);
    pub const INCOMPLETE_DRAW_BUFFER: Self = Self(gl::FRAMEBUFFER_INCOMPLETE_DRAW_BUFFER);
    pub const INCOMPLETE_READ_BUFFER: Self = Self(gl::FRAMEBUFFER_INCOMPLETE_READ_BUFFER);
    pub const UNSUPPORTED: Self = Self(gl::FRAMEBUFFER_UNSUPPORTED);
    pub const INCOMPLETE_MULTISAMPLE: Self = Self(gl::FRAMEBUFFER_INCOMPLETE_MULTISAMPLE);
    pub const INCOMPLETE_LAYER_TARGETS: Self = Self(gl::FRAMEBUFFER_INCOMPLETE_LAYER_TARGETS);
}

/// PORTS: `Framebuffer.zig:62-116` — `pub const Binding = struct { target: Target,
/// previous: c.GLuint, … }`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Binding {
    pub target: Target,
    pub previous: GLuint,
}

impl Binding {
    /// PORTS: `Framebuffer.zig:66-71`.
    ///
    /// ```zig
    /// pub fn unbind(self: Binding) void {
    ///     glad.context.BindFramebuffer.?(
    ///         @intFromEnum(self.target),
    ///         self.previous,
    ///     );
    /// }
    /// ```
    ///
    /// RESTORES the previously-bound framebuffer rather than binding 0 — see
    /// [`Framebuffer::bind`]'s note.
    pub fn unbind(self) {
        // SAFETY: rebinds whatever name was bound before. If that name has since
        // been deleted, GL raises GL_INVALID_OPERATION (which the reference does
        // not collect here) — never UB.
        unsafe { gl::BindFramebuffer(self.target.0, self.previous) };
    }

    /// PORTS: `Framebuffer.zig:73-88`.
    ///
    /// ```zig
    /// pub fn texture2D(
    ///     self: Binding,
    ///     attachment: Attachment,
    ///     textarget: Texture.Target,
    ///     texture: Texture,
    ///     level: c.GLint,
    /// ) !void {
    ///     glad.context.FramebufferTexture2D.?(
    ///         @intFromEnum(self.target),
    ///         @intFromEnum(attachment),
    ///         @intFromEnum(textarget),
    ///         texture.id,
    ///         level,
    ///     );
    ///     try errors.getError();
    /// }
    /// ```
    ///
    /// The attachment path `renderer/opengl/Target.zig` uses.
    pub fn texture_2d(
        self,
        attachment: Attachment,
        textarget: texture::Target,
        texture: Texture,
        level: GLint,
    ) -> Result<(), errors::Error> {
        // SAFETY: attaches a name the caller's `Texture` owns to the bound
        // framebuffer. A deleted name, a bad level, or a mismatched target is a
        // defined GL error, collected below.
        unsafe {
            gl::FramebufferTexture2D(
                self.target.0,
                attachment.0,
                textarget as GLenum,
                texture.id,
                level,
            );
        }
        errors::get_error()
    }

    /// PORTS: `Framebuffer.zig:90-102`.
    ///
    /// ```zig
    /// pub fn renderbuffer(
    ///     self: Binding,
    ///     attachment: Attachment,
    ///     buffer: Renderbuffer,
    /// ) !void {
    ///     glad.context.FramebufferRenderbuffer.?(
    ///         @intFromEnum(self.target),
    ///         @intFromEnum(attachment),
    ///         c.GL_RENDERBUFFER,
    ///         buffer.id,
    ///     );
    ///     try errors.getError();
    /// }
    /// ```
    ///
    /// PORT NOTE (`c.GL_RENDERBUFFER` hard-coded): `renderbuffertarget` has no
    /// other legal value, so the reference passes it inline rather than taking a
    /// parameter — mirroring `renderbuffer::Renderbuffer::bind`'s missing target.
    pub fn renderbuffer(
        self,
        attachment: Attachment,
        buffer: Renderbuffer,
    ) -> Result<(), errors::Error> {
        // SAFETY: attaches a name the caller's `Renderbuffer` owns to the bound
        // framebuffer. Every failure mode is a defined GL error, collected below.
        unsafe {
            gl::FramebufferRenderbuffer(self.target.0, attachment.0, gl::RENDERBUFFER, buffer.id);
        }
        errors::get_error()
    }

    /// PORTS: `Framebuffer.zig:104-111`.
    ///
    /// ```zig
    /// pub fn drawBuffers(
    ///     self: Binding,
    ///     bufs: []Attachment,
    /// ) !void {
    ///     _ = self;
    ///     glad.context.DrawBuffers.?(@intCast(bufs.len), bufs.ptr);
    ///     try errors.getError();
    /// }
    /// ```
    ///
    /// PORT NOTE (`_ = self`): the reference DISCARDS the receiver — `glDrawBuffers`
    /// acts on the currently-bound draw framebuffer, and the `Binding` is a
    /// bind-state token only. See `renderbuffer::Binding::storage`'s note.
    ///
    /// PORT NOTE (`bufs: []Attachment` -> `bufs: &[Attachment]`): the reference's
    /// slice is MUTABLE, but the body only reads `.len`/`.ptr` and `glDrawBuffers`
    /// takes a `const GLenum *`. A shared slice is therefore the honest signature,
    /// and it does not narrow any call site.
    ///
    /// PORT NOTE (`bufs.ptr` as a `*const GLenum`): sound because [`Attachment`] is
    /// `#[repr(transparent)]` over `GLenum` — see its note.
    pub fn draw_buffers(self, bufs: &[Attachment]) -> Result<(), errors::Error> {
        // `@intCast(bufs.len)`: usize -> GLsizei, trapping on overflow in Zig;
        // `try_from` panics identically.
        let n = GLsizei::try_from(bufs.len())
            .expect("Framebuffer.Binding::draw_buffers length exceeds GLsizei");
        // SAFETY: `bufs` is a live slice of `n` `Attachment`s, each layout-identical
        // to a `GLenum`, so the pointer is valid for `n` GLenum reads. GL copies the
        // list before returning — it retains nothing.
        unsafe { gl::DrawBuffers(n, bufs.as_ptr().cast::<GLenum>()) };
        errors::get_error()
    }

    /// PORTS: `Framebuffer.zig:113-115`.
    ///
    /// ```zig
    /// pub fn checkStatus(self: Binding) Status {
    ///     return @enumFromInt(glad.context.CheckFramebufferStatus.?(@intFromEnum(self.target)));
    /// }
    /// ```
    ///
    /// PORT NOTE: returns a bare [`Status`], NOT a `Result` — `glCheckFramebufferStatus`
    /// reports through its return value, and the reference collects no `getError`
    /// here. Callers compare against [`Status::COMPLETE`], as
    /// `renderer/opengl/Target.zig` does.
    ///
    /// PORT NOTE (`@enumFromInt` of a driver value): unchecked there, and sound
    /// here only because `Status` is a newtype — see its note.
    pub fn check_status(self) -> Status {
        // SAFETY: queries the completeness of the bound framebuffer. A bad target
        // makes it return 0, which is a `Status` the newtype represents fine.
        Status(unsafe { gl::CheckFramebufferStatus(self.target.0) })
    }
}

#[cfg(test)]
mod tests {
    use super::{Attachment, Status, Target};

    /// RUSTTY-ONLY. `Target`/`Attachment`/`Status` are newtypes rather than the
    /// reference's `enum(c_uint)`, so the Zig compiler is no longer the thing
    /// guaranteeing each constant IS its `c.GL_*` value. Assert it. No GL context
    /// needed. Same framing as `buffer.rs`'s
    /// `target_and_usage_constants_are_the_gl_constants`.
    #[test]
    fn framebuffer_constants_are_the_gl_constants() {
        assert_eq!(Target::FRAMEBUFFER.0, gl::FRAMEBUFFER);
        assert_eq!(Target::DRAW.0, gl::DRAW_FRAMEBUFFER);
        assert_eq!(Target::READ.0, gl::READ_FRAMEBUFFER);

        assert_eq!(Attachment::COLOR0.0, gl::COLOR_ATTACHMENT0);
        assert_eq!(Attachment::DEPTH.0, gl::DEPTH_ATTACHMENT);
        assert_eq!(Attachment::STENCIL.0, gl::STENCIL_ATTACHMENT);
        assert_eq!(Attachment::DEPTH_STENCIL.0, gl::DEPTH_STENCIL_ATTACHMENT);

        assert_eq!(Status::COMPLETE.0, gl::FRAMEBUFFER_COMPLETE);
        assert_eq!(Status::UNDEFINED.0, gl::FRAMEBUFFER_UNDEFINED);
        assert_eq!(
            Status::INCOMPLETE_ATTACHMENT.0,
            gl::FRAMEBUFFER_INCOMPLETE_ATTACHMENT
        );
        assert_eq!(
            Status::INCOMPLETE_MISSING_ATTACHMENT.0,
            gl::FRAMEBUFFER_INCOMPLETE_MISSING_ATTACHMENT
        );
        assert_eq!(
            Status::INCOMPLETE_DRAW_BUFFER.0,
            gl::FRAMEBUFFER_INCOMPLETE_DRAW_BUFFER
        );
        assert_eq!(
            Status::INCOMPLETE_READ_BUFFER.0,
            gl::FRAMEBUFFER_INCOMPLETE_READ_BUFFER
        );
        assert_eq!(Status::UNSUPPORTED.0, gl::FRAMEBUFFER_UNSUPPORTED);
        assert_eq!(
            Status::INCOMPLETE_MULTISAMPLE.0,
            gl::FRAMEBUFFER_INCOMPLETE_MULTISAMPLE
        );
        assert_eq!(
            Status::INCOMPLETE_LAYER_TARGETS.0,
            gl::FRAMEBUFFER_INCOMPLETE_LAYER_TARGETS
        );
    }

    /// RUSTTY-ONLY. [`Binding::draw_buffers`] casts `&[Attachment]` straight to a
    /// `*const GLenum`, which is sound ONLY while `Attachment` stays
    /// `#[repr(transparent)]`. Dropping that attribute still compiles and still
    /// passes every other test — this is the tripwire that would not.
    #[test]
    fn attachment_is_layout_identical_to_glenum() {
        assert_eq!(
            size_of::<Attachment>(),
            size_of::<gl::types::GLenum>(),
            "Attachment must be layout-identical to GLenum for draw_buffers",
        );
        assert_eq!(
            align_of::<Attachment>(),
            align_of::<gl::types::GLenum>(),
            "Attachment must be layout-identical to GLenum for draw_buffers",
        );

        // The cast `draw_buffers` performs, spelled out: element N of the slice
        // must read back as the GLenum it was built from.
        let bufs = [Attachment::COLOR0, Attachment(gl::COLOR_ATTACHMENT0 + 1)];
        let as_enums = bufs.as_ptr().cast::<gl::types::GLenum>();
        // SAFETY: the assertions above establish the layout this cast relies on.
        unsafe {
            assert_eq!(*as_enums, gl::COLOR_ATTACHMENT0);
            assert_eq!(*as_enums.add(1), gl::COLOR_ATTACHMENT0 + 1);
        }
    }
}
