//! Represents a render target.
//!
//! In this case, an OpenGL renderbuffer-backed framebuffer.
//!
//! PORTS: ghostty `src/renderer/opengl/Target.zig` (60 loc) — the renderer-facing
//! render target layered over the `gl` package's [`Framebuffer`](super::gl::Framebuffer)
//! and [`Renderbuffer`](super::gl::Renderbuffer) leaves (Task 4). This is the
//! OpenGL twin of `metal/target.rs`.
//!
//! PORT NOTE (Copy handles, no ARC): the `gl` leaves are `Copy` `GLuint` handles
//! with explicit `create`/`destroy`, so `deinit` is a real by-value method (not a
//! `Drop` impl), exactly as `Target.zig:57-60` has it. This wrapper is deliberately
//! NOT `Copy` (a copy + double `deinit` would double-delete the shared handles) and
//! NOT `Drop` (the reference manages the handles explicitly) — see `opengl::texture`
//! for the same rationale.

use gl::types::GLsizei;

use super::gl::framebuffer::{Attachment, Framebuffer, Target as FbTarget};
use super::gl::renderbuffer::Renderbuffer;
use super::gl::texture::InternalFormat;

// const log = std.log.scoped(.opengl);
// PORT NOTE: `Target.zig` declares the shared `.opengl` scoped logger but has no
// `log` call sites; the declaration is preserved as this comment for inventory
// parity (see `opengl::gl`'s package-root logger note).

/// PORTS: `Target.zig:34-55`'s error union. `gl.Renderbuffer.create`/`Framebuffer.create`
/// have EMPTY inferred error sets, and `storage`/`renderbuffer` collect
/// `errors.Error` — so `init`'s inferred set is exactly `errors.Error`. Ported as a
/// distinct one-variant type so [`Target::init`]'s signature reads at its call
/// sites; the wrapped [`errors::Error`](super::gl::errors::Error) carries the
/// specific GL failure.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Error {
    /// An OpenGL API call failed.
    Gl(super::gl::errors::Error),
}

impl From<super::gl::errors::Error> for Error {
    fn from(err: super::gl::errors::Error) -> Self {
        Self::Gl(err)
    }
}

/// Options for initializing a Target
//
// PORTS: `Target.zig:12-21`.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// Desired width
    pub width: usize,
    /// Desired height
    pub height: usize,

    /// Internal format for the renderbuffer.
    pub internal_format: InternalFormat,
}

/// PORTS: `Target.zig:23-32` — the top-level `framebuffer`/`renderbuffer`/`width`/
/// `height` fields ARE the struct.
pub struct Target {
    /// The underlying `gl.Framebuffer` instance.
    pub framebuffer: Framebuffer,

    /// The underlying `gl.Renderbuffer` instance.
    pub renderbuffer: Renderbuffer,

    /// Current width of this target.
    pub width: usize,
    /// Current height of this target.
    pub height: usize,
}

impl Target {
    /// PORTS: `Target.zig:34-55`.
    ///
    /// ```zig
    /// pub fn init(opts: Options) !Self {
    ///     const rbo = try gl.Renderbuffer.create();
    ///     const bound_rbo = try rbo.bind();
    ///     defer bound_rbo.unbind();
    ///     try bound_rbo.storage(opts.internal_format, @intCast(opts.width), @intCast(opts.height));
    ///
    ///     const fbo = try gl.Framebuffer.create();
    ///     const bound_fbo = try fbo.bind(.framebuffer);
    ///     defer bound_fbo.unbind();
    ///     try bound_fbo.renderbuffer(.color0, rbo);
    ///
    ///     return .{ .framebuffer = fbo, .renderbuffer = rbo, .width = …, .height = … };
    /// }
    /// ```
    ///
    /// PORT NOTE (`@intCast(opts.width)`): `usize` -> `GLsizei` (`c_int`), trapping
    /// on overflow in Zig; `try_from`'s `expect` panics identically. A render
    /// target dimension exceeding `GLsizei` is not a case the reference handles.
    pub fn init(opts: Options) -> Result<Self, Error> {
        let width = GLsizei::try_from(opts.width).expect("Target width exceeds GLsizei");
        let height = GLsizei::try_from(opts.height).expect("Target height exceeds GLsizei");

        let rbo = Renderbuffer::create()?;
        let bound_rbo = rbo.bind()?;
        // defer bound_rbo.unbind()
        let rbo_result = bound_rbo.storage(opts.internal_format, width, height);
        bound_rbo.unbind();
        rbo_result?;

        let fbo = Framebuffer::create()?;
        let bound_fbo = fbo.bind(FbTarget::FRAMEBUFFER)?;
        // defer bound_fbo.unbind()
        let fbo_result = bound_fbo.renderbuffer(Attachment::COLOR0, rbo);
        bound_fbo.unbind();
        fbo_result?;

        Ok(Self {
            framebuffer: fbo,
            renderbuffer: rbo,
            width: opts.width,
            height: opts.height,
        })
    }

    /// PORTS: `Target.zig:57-60`.
    ///
    /// ```zig
    /// pub fn deinit(self: *Self) void {
    ///     self.framebuffer.destroy();
    ///     self.renderbuffer.destroy();
    /// }
    /// ```
    ///
    /// PORT NOTE: takes `self` by value here (not `&mut self`): the `gl` handles it
    /// destroys are `Copy`, and consuming `self` prevents a use-after-`deinit` at
    /// the type level, which the reference's `*Self` cannot. The reference's callers
    /// drop the `Target` right after `deinit`, so no call site relies on it staying
    /// usable.
    pub fn deinit(self) {
        self.framebuffer.destroy();
        self.renderbuffer.destroy();
    }

    /// Copy the target's `width*height*4` bytes into a tight `Vec<u8>` in RGBA
    /// order, by binding its framebuffer for reading and `glReadPixels`-ing it.
    ///
    /// RUSTTY-ONLY (no reference `test{}` drives this — it exists for the port's own
    /// golden gate): the offscreen readback the milestone's Task 11 golden test
    /// needs. Documented exactly like `metal/target.rs::read_bgra()`. Call AFTER the
    /// frame has completed (the `Frame::complete` / `RenderPass::complete` flush or
    /// `gl.finish()` has landed the pixels).
    pub fn read_rgba(&self) -> Vec<u8> {
        self.read_pixels(gl::RGBA)
    }

    /// Copy the target's `width*height*4` bytes into a tight `Vec<u8>` in BGRA
    /// order (the golden runner's native byte order — matches
    /// `metal/target.rs::read_bgra`). RUSTTY-ONLY; see [`Target::read_rgba`].
    pub fn read_bgra(&self) -> Vec<u8> {
        self.read_pixels(gl::BGRA)
    }

    /// The shared body of [`read_rgba`](Self::read_rgba)/[`read_bgra`](Self::read_bgra):
    /// bind the target's framebuffer to `GL_READ_FRAMEBUFFER`, read `width*height`
    /// RGBA/BGRA texels into a tight buffer, restore the previous read binding.
    //
    // No `pkg/opengl` wrapper exists for `glReadPixels` (it has no reference call
    // site), so this rustty-only helper calls it raw. The row stride is exactly
    // `width*4`, always a multiple of 4, so the default `GL_PACK_ALIGNMENT` of 4 is
    // already satisfied — no `glPixelStorei` needed.
    fn read_pixels(&self, format: gl::types::GLenum) -> Vec<u8> {
        let mut out = vec![0u8; self.width * self.height * 4];
        let width = GLsizei::try_from(self.width).expect("Target width exceeds GLsizei");
        let height = GLsizei::try_from(self.height).expect("Target height exceeds GLsizei");

        // Bind for reading; the `Binding` restores the previous framebuffer on
        // `unbind` (see `framebuffer::Framebuffer::bind`). If the bind itself fails
        // the buffer is returned zeroed — the caller compares it against an
        // expected image, so a zeroed miss surfaces as a failed assertion, never a
        // panic or a fabricated result.
        let Ok(binding) = self.framebuffer.bind(FbTarget::READ) else {
            return out;
        };
        // SAFETY: `out` is `width*height*4` bytes; `glReadPixels` writes exactly
        // that for a `GL_UNSIGNED_BYTE` RGBA/BGRA read of a `width x height` region
        // at pack alignment 4 (satisfied — see above). The bound read framebuffer
        // is this target's, which is complete after `init`.
        unsafe {
            gl::ReadPixels(
                0,
                0,
                width,
                height,
                format,
                gl::UNSIGNED_BYTE,
                out.as_mut_ptr().cast::<std::ffi::c_void>(),
            );
        }
        binding.unbind();
        out
    }
}
