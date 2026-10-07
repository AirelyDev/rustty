//! Represents a render target.
//!
//! In this case, a texture-backed framebuffer. The color attachment is a
//! `GL_TEXTURE_2D` texture instead of a renderbuffer so that we can create
//! an EGLImage from it and export the rendered frame as a dma-buf for
//! presentation by the apprt.
//!
//! We use two textures:
//!
//!   - `texture`: `GL_SRGB8_ALPHA8`. We render to this. With
//!     `GL_FRAMEBUFFER_SRGB` enabled, the GPU automatically converts linear
//!     shader output to sRGB on write. This is required for the
//!     linear-blending color pipeline to produce correct output.
//!
//!   - `export_texture`: `GL_RGBA8`. This is the texture we actually export
//!     as a dma-buf. Mesa cannot export `GL_SRGB8_ALPHA8` textures to
//!     dma-buf, so we blit the rendered sRGB texture into this plain RGBA8
//!     texture (the blit copies the already-sRGB-encoded pixel values
//!     verbatim) and export that instead.
//!
//! PORTS: ghostty `src/renderer/opengl/Target.zig` at `12752b2ac` (P7-W7 T1) — the
//! renderbuffer-backed target of the pin, rewritten upstream for the GtkGLArea →
//! EGL + DMA-BUF redesign (`2f0b65346`, `adf161473`, `8619becb2`, `b9e07f98f`).
//! This is the OpenGL twin of `metal/target.rs`.
//!
//! R13 FIX (DECISIONS M5, `.port/parity-closure/UPSTREAM-DEFECTS.md` UD354): the
//! render texture is `GL_SRGB8_ALPHA8` only for the linear `alpha-blending`
//! modes; with `native` it is `GL_RGBA8`, as the pin's renderbuffer was. See
//! [`Options::internal_format`].
//!
//! PORT NOTE (Copy handles, no ARC): the `gl` leaves are `Copy` `GLuint` handles
//! with explicit `create`/`destroy`, so `deinit` is a real by-value method (not a
//! `Drop` impl), exactly as `Target.zig` has it. This wrapper is deliberately NOT
//! `Copy` (a copy + double `deinit` would double-delete the shared handles) and NOT
//! `Drop` (the reference manages the handles explicitly) — see `opengl::texture`
//! for the same rationale.

use gl::types::{GLint, GLsizei};

use super::gl::draw;
use super::gl::errors;
use super::gl::framebuffer::{Attachment, Framebuffer, Status, Target as FbTarget};
use super::gl::glad;
use super::gl::texture::{
    self as gltex, DataType, Format, InternalFormat, MagFilter, MinFilter, Parameter, Texture, Wrap,
};

// const log = std.log.scoped(.opengl);
//
// The same `.opengl` scope as every other file in the package (`gl.rs`'s logger
// note).
use super::gl::LOG_SCOPE;

/// The errors [`Target::init`] can return: `Target.zig`'s inferred error set.
///
/// PORTS: every `try` in `init` collects `errors.Error` (a GL call failed), and the
/// two completeness checks return `error.FramebufferIncomplete`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Error {
    /// An OpenGL API call failed.
    Gl(errors::Error),
    /// `return error.FramebufferIncomplete;` (`Target.zig:88`/`:126`).
    FramebufferIncomplete,
}

impl From<errors::Error> for Error {
    fn from(err: errors::Error) -> Self {
        Self::Gl(err)
    }
}

/// Options for initializing a Target
//
// PORTS: `Target.zig:30-36`.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// Desired width
    pub width: usize,
    /// Desired height
    pub height: usize,

    /// Internal format for the render texture.
    ///
    /// R13 FIX (DECISIONS M5, UD354): the reference dropped this field (the pin
    /// had it, `Target.zig:19-20` at `20abdb50a`, for the renderbuffer) and always
    /// renders into `.srgba`. With `GL_FRAMEBUFFER_SRGB` enabled an sRGB target
    /// ENCODES what the shaders write, which is right for the linear
    /// `alpha-blending` modes, whose shaders write linear values. Under `native`
    /// the shaders already write sRGB-encoded values (`unlinearize` in
    /// `cell_text.f.glsl`, `image.f.glsl`, `bg_image.f.glsl`, …), so every color
    /// is encoded twice: measured on the offscreen golden, the background image
    /// `#3060C0` renders as `#78A5E1` and the cursor golden's `#141414` as
    /// `#4F4F4F`. `OpenGL::init_target` passes `.srgba` for the linear modes and
    /// `.rgba` for `native`, the pin's choice. The export texture stays `RGBA8`
    /// either way, and the export blit copies the stored bytes verbatim.
    pub internal_format: InternalFormat,
}

/// PORTS: `Target.zig:38-53`.
pub struct Target {
    /// The framebuffer we render to.
    pub framebuffer: Framebuffer,

    /// The sRGB color attachment texture we render to.
    ///
    /// (`RGBA8` under `alpha-blending = native`: UD354, [`Options::internal_format`].)
    pub texture: Texture,

    /// A plain RGBA8 texture + framebuffer that we blit `texture` into for
    /// dma-buf export. Mesa can't export sRGB textures, so we blit the
    /// already-sRGB-encoded pixels into this non-sRGB texture and export it.
    pub export_texture: Texture,
    pub export_framebuffer: Framebuffer,

    /// Current width of this target.
    pub width: usize,
    /// Current height of this target.
    pub height: usize,
}

/// Which of [`Target::init`]'s two texture + framebuffer pairs [`init_pair`]
/// builds; it picks the log line.
#[derive(Clone, Copy)]
enum Pair {
    Render,
    Export,
}

/// One texture + framebuffer pair of [`Target::init`]: the reference's two
/// identical blocks (`Target.zig:56-91` and `:93-129`), which differ only in the
/// texture's internal format and the log line.
///
/// PORT NOTE (`errdefer texture.destroy()` / `errdefer fbo.destroy()`): each is
/// reproduced by destroying what was created before returning the error; the
/// pair's objects are destroyed by the caller if a LATER pair fails.
fn init_pair(
    width: GLsizei,
    height: GLsizei,
    internal_format: InternalFormat,
    pair: Pair,
) -> Result<(Texture, Framebuffer), Error> {
    let texture = Texture::create()?;
    let configured = (|| -> Result<(), Error> {
        let bound_tex = texture.bind(gltex::Target::Texture2D)?;
        // defer bound_tex.unbind();
        let res = (|| -> Result<(), Error> {
            bound_tex.parameter(Parameter::MinFilter, MinFilter::Nearest)?;
            bound_tex.parameter(Parameter::MagFilter, MagFilter::Nearest)?;
            bound_tex.parameter(Parameter::WrapS, Wrap::ClampToEdge)?;
            bound_tex.parameter(Parameter::WrapT, Wrap::ClampToEdge)?;
            // SAFETY: a null `data` allocates uninitialized storage and reads
            // nothing (`image_2d`'s contract).
            unsafe {
                bound_tex.image_2d(
                    0,
                    internal_format,
                    width,
                    height,
                    Format::RGBA,
                    DataType::UNSIGNED_BYTE,
                    std::ptr::null(),
                )
            }?;
            bound_tex.parameter(Parameter::BaseLevel, 0 as GLint)?;
            bound_tex.parameter(Parameter::MaxLevel, 0 as GLint)?;
            Ok(())
        })();
        bound_tex.unbind();
        res
    })();
    if let Err(err) = configured {
        texture.destroy(); // errdefer texture.destroy();
        return Err(err);
    }

    let fbo = match Framebuffer::create() {
        Ok(fbo) => fbo,
        Err(err) => {
            texture.destroy();
            return Err(err.into());
        }
    };
    let attached = (|| -> Result<(), Error> {
        let bound_fbo = fbo.bind(FbTarget::FRAMEBUFFER)?;
        // defer bound_fbo.unbind();
        let res = (|| -> Result<(), Error> {
            bound_fbo.texture_2d(Attachment::COLOR0, gltex::Target::Texture2D, texture, 0)?;
            match bound_fbo.check_status() {
                Status::COMPLETE => Ok(()),
                // PORT NOTE (`status={}`): `Status` is a newtype over the raw
                // `GLenum` (the reference's non-exhaustive enum), so the value
                // prints as its number.
                status => {
                    match pair {
                        Pair::Render => {
                            // log.warn("render framebuffer incomplete status={}", .{status});
                            log::warn!(
                                target: LOG_SCOPE,
                                "render framebuffer incomplete status={}",
                                status.0
                            );
                        }
                        Pair::Export => {
                            // log.warn("export framebuffer incomplete status={}", .{status});
                            log::warn!(
                                target: LOG_SCOPE,
                                "export framebuffer incomplete status={}",
                                status.0
                            );
                        }
                    }
                    Err(Error::FramebufferIncomplete)
                }
            }
        })();
        bound_fbo.unbind();
        res
    })();
    if let Err(err) = attached {
        fbo.destroy(); // errdefer fbo.destroy();
        texture.destroy(); // errdefer texture.destroy();
        return Err(err);
    }

    Ok((texture, fbo))
}

impl Target {
    /// PORTS: `Target.zig:55-139`.
    ///
    /// ```zig
    /// pub fn init(opts: Options) !Self {
    ///     const texture = try gl.Texture.create();
    ///     errdefer texture.destroy();
    ///     {
    ///         const bound_tex = try texture.bind(.@"2d");
    ///         defer bound_tex.unbind();
    ///         try bound_tex.parameter(.min_filter, .nearest);
    ///         try bound_tex.parameter(.mag_filter, .nearest);
    ///         try bound_tex.parameter(.wrap_s, .clamp_to_edge);
    ///         try bound_tex.parameter(.wrap_t, .clamp_to_edge);
    ///         try bound_tex.image2D(
    ///             0,
    ///             .srgba,
    ///             @intCast(opts.width),
    ///             @intCast(opts.height),
    ///             .rgba,
    ///             .unsigned_byte,
    ///             null,
    ///         );
    ///         try bound_tex.parameter(.base_level, 0);
    ///         try bound_tex.parameter(.max_level, 0);
    ///     }
    ///
    ///     const fbo = try gl.Framebuffer.create();
    ///     errdefer fbo.destroy();
    ///     {
    ///         const bound_fbo = try fbo.bind(.framebuffer);
    ///         defer bound_fbo.unbind();
    ///         try bound_fbo.texture2D(.color0, .@"2d", texture, 0);
    ///         switch (bound_fbo.checkStatus()) {
    ///             .complete => {},
    ///             else => |status| {
    ///                 log.warn("render framebuffer incomplete status={}", .{status});
    ///                 return error.FramebufferIncomplete;
    ///             },
    ///         }
    ///     }
    ///
    ///     // --- Export texture (plain RGBA8, for dma-buf export) ---
    ///     (the same two blocks with `.rgba`, `export_texture`, `export_fbo` and
    ///      "export framebuffer incomplete status={}")
    ///
    ///     return .{
    ///         .framebuffer = fbo,
    ///         .texture = texture,
    ///         .export_framebuffer = export_fbo,
    ///         .export_texture = export_texture,
    ///         .width = opts.width,
    ///         .height = opts.height,
    ///     };
    /// }
    /// ```
    ///
    /// PORT NOTE (`@intCast(opts.width)`): `usize` -> `GLsizei` (`c_int`), trapping
    /// on overflow in Zig; `try_from`'s `expect` panics identically. A render
    /// target dimension exceeding `GLsizei` is not a case the reference handles.
    pub fn init(opts: Options) -> Result<Self, Error> {
        let width = GLsizei::try_from(opts.width).expect("Target width exceeds GLsizei");
        let height = GLsizei::try_from(opts.height).expect("Target height exceeds GLsizei");

        // `.srgba` in the reference; `opts.internal_format` under UD354.
        let (texture, fbo) = init_pair(width, height, opts.internal_format, Pair::Render)?;

        // --- Export texture (plain RGBA8, for dma-buf export) ---
        let (export_texture, export_fbo) =
            match init_pair(width, height, InternalFormat::RGBA, Pair::Export) {
                Ok(pair) => pair,
                Err(err) => {
                    // errdefer fbo.destroy(); errdefer texture.destroy();
                    fbo.destroy();
                    texture.destroy();
                    return Err(err);
                }
            };

        Ok(Self {
            framebuffer: fbo,
            texture,
            export_framebuffer: export_fbo,
            export_texture,
            width: opts.width,
            height: opts.height,
        })
    }

    /// PORTS: `Target.zig:141-146`.
    ///
    /// ```zig
    /// pub fn deinit(self: *Self) void {
    ///     self.framebuffer.destroy();
    ///     self.texture.destroy();
    ///     self.export_framebuffer.destroy();
    ///     self.export_texture.destroy();
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
        self.texture.destroy();
        self.export_framebuffer.destroy();
        self.export_texture.destroy();
    }

    /// Read the current contents of the framebuffer into CPU memory.
    ///
    /// This is used by the CPU readback presentation fallback. The
    /// returned data is tightly-packed RGBA8 with premultiplied alpha,
    /// i.e. `width * 4` bytes per row, containing sRGB-encoded values:
    /// `GL_FRAMEBUFFER_SRGB` has no effect on `ReadPixels`, so the stored
    /// values are returned verbatim.
    ///
    /// PORTS: `Target.zig:217-242`.
    ///
    /// ```zig
    /// pub fn readPixelsAlloc(self: *const Self, alloc: std.mem.Allocator) ![]u8 {
    ///     const bind = try self.framebuffer.bind(.read);
    ///     defer bind.unbind();
    ///
    ///     const pixels = try alloc.alloc(u8, self.width * self.height * 4);
    ///     errdefer alloc.free(pixels);
    ///
    ///     try gl.readPixels(
    ///         0,
    ///         0,
    ///         @intCast(self.width),
    ///         @intCast(self.height),
    ///         .rgba,
    ///         .unsigned_byte,
    ///         pixels.ptr,
    ///     );
    ///
    ///     return pixels;
    /// }
    /// ```
    ///
    /// PORT NOTE (`alloc`): the Rust port does not thread an allocator (the module
    /// docs of `opengl_impl`); the pixels are a `Vec<u8>`, freed on drop.
    ///
    /// PORT NOTE (`bind(.read)`): `Framebuffer.bind` saves `GL_FRAMEBUFFER_BINDING`
    /// (the DRAW binding) whatever the target, and `unbind` restores it onto READ
    /// (UD250). Faithful; it has no effect while both bindings start at the same
    /// framebuffer.
    pub fn read_pixels_alloc(&self) -> Result<Vec<u8>, errors::Error> {
        let width = GLsizei::try_from(self.width).expect("Target width exceeds GLsizei");
        let height = GLsizei::try_from(self.height).expect("Target height exceeds GLsizei");

        let bind = self.framebuffer.bind(FbTarget::READ)?;
        // defer bind.unbind();
        let mut pixels = vec![0u8; self.width * self.height * 4];
        // The rows are `width * 4` bytes, a multiple of the default
        // `GL_PACK_ALIGNMENT` of 4, so the read is exactly `pixels.len()` bytes.
        //
        // SAFETY: `pixels` holds `width * height * 4` writable bytes, which is
        // what an RGBA/UNSIGNED_BYTE read of `width x height` writes at pack
        // alignment 4 (`read_pixels`'s contract).
        let res = unsafe {
            draw::read_pixels(
                0,
                0,
                width,
                height,
                Format::RGBA,
                DataType::UNSIGNED_BYTE,
                pixels.as_mut_ptr().cast(),
            )
        };
        bind.unbind();
        res?;
        Ok(pixels)
    }

    /// Copy the target's `width*height*4` bytes into a tight `Vec<u8>` in RGBA
    /// order, by binding its framebuffer for reading and `glReadPixels`-ing it.
    ///
    /// RUSTTY-ONLY (no reference `test{}` drives this — it exists for the port's own
    /// golden gate): the offscreen readback the milestone's Task 11 golden test
    /// needs. Documented exactly like `metal/target.rs::read_bgra()`. Call AFTER the
    /// frame has completed (the `Frame::complete` / `RenderPass::complete` flush or
    /// `gl.finish()` has landed the pixels). Unlike [`Self::read_pixels_alloc`] it
    /// never fails: a failed read returns the (zeroed) buffer, which the golden
    /// assertions then reject.
    pub fn read_rgba(&self) -> Vec<u8> {
        self.read_pixels(Format::RGBA)
    }

    /// Copy the target's `width*height*4` bytes into a tight `Vec<u8>` in BGRA
    /// order (the golden runner's native byte order — matches
    /// `metal/target.rs::read_bgra`). RUSTTY-ONLY; see [`Target::read_rgba`].
    pub fn read_bgra(&self) -> Vec<u8> {
        self.read_pixels(Format::BGRA)
    }

    /// The shared body of [`read_rgba`](Self::read_rgba)/[`read_bgra`](Self::read_bgra):
    /// bind the target's framebuffer to `GL_READ_FRAMEBUFFER`, read `width*height`
    /// RGBA/BGRA texels into a tight buffer, restore the previous read binding.
    fn read_pixels(&self, format: Format) -> Vec<u8> {
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
        // No `errors.getError()` here, unlike `draw::read_pixels`: this helper
        // predates that wrapper and deliberately leaves any pending GL error for
        // the caller's own `get_error` checks to see.
        //
        // SAFETY: `out` is `width*height*4` bytes; `glReadPixels` writes exactly
        // that for a `GL_UNSIGNED_BYTE` RGBA/BGRA read of a `width x height` region
        // at pack alignment 4 (the row stride `width*4` is a multiple of 4). The
        // bound read framebuffer is this target's, which is complete after `init`.
        glad::context(|gl| unsafe {
            gl.ReadPixels(
                0,
                0,
                width,
                height,
                format.0,
                DataType::UNSIGNED_BYTE.0,
                out.as_mut_ptr().cast(),
            );
        });
        binding.unbind();
        out
    }
}

// The DMA-BUF export is Unix-only: `crate::dmabuf` holds Linux fds. Within the
// `.opengl` targets (Linux, FreeBSD, Windows) `unix` is exactly Linux + FreeBSD;
// Windows builds the OpenGL renderer but never exports a frame.
#[cfg(unix)]
mod export {
    use gl::types::GLint;

    use super::super::gl::draw::{self, BlitMask};
    use super::super::gl::framebuffer::Target as FbTarget;
    use super::super::gl::sys::{EglError, Image, ImageTarget, OffscreenContext};
    use super::super::gl::texture::MagFilter;
    use super::super::gl::{errors, LOG_SCOPE};
    use super::Target;
    use crate::dmabuf::{self, Dmabuf, RawPlanes};

    /// The errors [`Target::export_dmabuf`] can return: `exportDmabuf`'s inferred
    /// error set.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub enum ExportError {
        /// A GL call failed (`gl.disable`, the binds, `gl.blitFramebuffer`).
        Gl(errors::Error),
        /// An EGL call failed (`Image.create`, the two MESA exports).
        Egl(EglError),
        /// `return error.DmabufTooManyPlanes;` (`Target.zig:193`).
        DmabufTooManyPlanes,
        /// `error.BadDmabuf` from `planes.validate()` (`Target.zig:203`).
        BadDmabuf,
    }

    impl From<errors::Error> for ExportError {
        fn from(err: errors::Error) -> Self {
            Self::Gl(err)
        }
    }

    impl From<EglError> for ExportError {
        fn from(err: EglError) -> Self {
            Self::Egl(err)
        }
    }

    impl From<dmabuf::BadDmabuf> for ExportError {
        fn from(_: dmabuf::BadDmabuf) -> Self {
            Self::BadDmabuf
        }
    }

    impl Target {
        /// PORTS: `Target.zig:148-215`.
        ///
        /// ```zig
        /// pub fn exportDmabuf(
        ///     self: *const Self,
        ///     display: *egl.Display,
        ///     context: *egl.Context,
        /// ) !Dmabuf {
        ///     // We disable GL_FRAMEBUFFER_SRGB while doing this blit, otherwise
        ///     // the values may be linearized as they're copied, but even though
        ///     // the draw framebuffer has a linear internal format, the values in
        ///     // it should be sRGB, not linear!
        ///     try gl.disable(gl.c.GL_FRAMEBUFFER_SRGB);
        ///     defer gl.enable(gl.c.GL_FRAMEBUFFER_SRGB) catch |err| {
        ///         log.err("Error re-enabling GL_FRAMEBUFFER_SRGB, err={}", .{err});
        ///     };
        ///
        ///     const read_bind = try self.framebuffer.bind(.read);
        ///     defer read_bind.unbind();
        ///
        ///     const draw_bind = try self.export_framebuffer.bind(.draw);
        ///     defer draw_bind.unbind();
        ///
        ///     try gl.blitFramebuffer(
        ///         0, 0, @intCast(self.width), @intCast(self.height),
        ///         0, 0, @intCast(self.width), @intCast(self.height),
        ///         .{ .color_buffer_bit = true },
        ///         .nearest,
        ///     );
        ///
        ///     const image: *egl.Image = try .create(
        ///         display,
        ///         context,
        ///         .{ .texture_2d = self.export_texture.id },
        ///         &.{},
        ///     );
        ///     defer image.destroy(display) catch {};
        ///
        ///     const query = try image.exportDmabufQuery(display);
        ///
        ///     if (query.num_planes < 1 or query.num_planes > Dmabuf.max_planes) {
        ///         log.err("DMABUF has too many planes={}", .{query.num_planes});
        ///         return error.DmabufTooManyPlanes;
        ///     }
        ///
        ///     var planes: Dmabuf.Planes = .{ .count = @intCast(query.num_planes) };
        ///     try image.exportDmabuf(
        ///         display,
        ///         &planes.fds,
        ///         &planes.strides,
        ///         &planes.offsets,
        ///     );
        ///     try planes.validate();
        ///
        ///     return .{
        ///         .width = @intCast(self.width),
        ///         .height = @intCast(self.height),
        ///         // bitCast instead of intCast since the numerical value of fourccs
        ///         // is rather meaningless, and we only care about the bit pattern
        ///         .fourcc = @bitCast(query.fourcc),
        ///         .modifier = query.modifier,
        ///         .premultiplied = true,
        ///         .planes = planes,
        ///     };
        /// }
        /// ```
        ///
        /// PORT NOTE (`display, context`): the [`OffscreenContext`] that owns both
        /// (see its docs; stage S2 moves them into `OpenGL`).
        ///
        /// PORT NOTE (the `defer` order): Zig runs `defer`s in reverse, so the
        /// image is destroyed, then the draw and read bindings are restored, then
        /// `GL_FRAMEBUFFER_SRGB` is re-enabled, on every exit path. The body below
        /// is split so that order holds on the error paths too.
        pub fn export_dmabuf(&self, ctx: &OffscreenContext) -> Result<Dmabuf, ExportError> {
            draw::disable(gl::FRAMEBUFFER_SRGB)?;
            let res = self.export_dmabuf_srgb_disabled(ctx);
            // defer gl.enable(gl.c.GL_FRAMEBUFFER_SRGB) catch |err| { log.err(…) };
            if let Err(err) = draw::enable(gl::FRAMEBUFFER_SRGB) {
                log::error!(
                    target: LOG_SCOPE,
                    "Error re-enabling GL_FRAMEBUFFER_SRGB, err={err:?}"
                );
            }
            res
        }

        fn export_dmabuf_srgb_disabled(
            &self,
            ctx: &OffscreenContext,
        ) -> Result<Dmabuf, ExportError> {
            let read_bind = self.framebuffer.bind(FbTarget::READ)?;
            let res = (|| {
                let draw_bind = self.export_framebuffer.bind(FbTarget::DRAW)?;
                let res = self.export_dmabuf_bound(ctx);
                draw_bind.unbind(); // defer draw_bind.unbind();
                res
            })();
            read_bind.unbind(); // defer read_bind.unbind();
            res
        }

        fn export_dmabuf_bound(&self, ctx: &OffscreenContext) -> Result<Dmabuf, ExportError> {
            let width = GLint::try_from(self.width).expect("Target width exceeds GLint");
            let height = GLint::try_from(self.height).expect("Target height exceeds GLint");
            draw::blit_framebuffer(
                0,
                0,
                width,
                height,
                0,
                0,
                width,
                height,
                BlitMask {
                    color_buffer_bit: true,
                    ..BlitMask::default()
                },
                MagFilter::Nearest,
            )?;

            let image = Image::create(
                ctx,
                ImageTarget::Texture2d(self.export_texture.id),
                &[khronos_egl::ATTRIB_NONE],
            )?;
            let res = self.export_dmabuf_image(ctx, &image);
            // defer image.destroy(display) catch {};
            let _ = image.destroy(ctx);
            res
        }

        fn export_dmabuf_image(
            &self,
            ctx: &OffscreenContext,
            image: &Image,
        ) -> Result<Dmabuf, ExportError> {
            let query = image.export_dmabuf_query(ctx)?;

            if query.num_planes < 1 || query.num_planes > dmabuf::MAX_PLANES as i32 {
                // log.err("DMABUF has too many planes={}", .{query.num_planes});
                log::error!(
                    target: LOG_SCOPE,
                    "DMABUF has too many planes={}",
                    query.num_planes
                );
                return Err(ExportError::DmabufTooManyPlanes);
            }

            // `@intCast(query.num_planes)`: 1..=4, checked just above.
            let mut planes = RawPlanes::new(query.num_planes as u8);
            image.export_dmabuf(
                ctx,
                &mut planes.fds,
                &mut planes.strides,
                &mut planes.offsets,
            )?;
            // SAFETY: the driver just exported these fds to us; nothing else owns
            // them, and `validate` either takes ownership or closes them.
            let planes = unsafe { planes.validate() }?;

            Ok(Dmabuf {
                width: u32::try_from(self.width).expect("Target width exceeds u32"),
                height: u32::try_from(self.height).expect("Target height exceeds u32"),
                // bitCast instead of intCast since the numerical value of fourccs
                // is rather meaningless, and we only care about the bit pattern
                fourcc: query.fourcc as u32,
                modifier: query.modifier,
                premultiplied: true,
                planes,
            })
        }
    }
}

#[cfg(unix)]
pub use export::ExportError;

#[cfg(test)]
pub(crate) mod tests {
    use rustty_render_types::AlphaBlending;

    use super::super::gl::framebuffer::Target as FbTarget;
    use super::super::gl::test_support;
    use super::super::gl::{draw, errors, glad};
    use super::Target;

    /// Clears `target`'s render framebuffer to `rgba`, then clears the
    /// `scissor` rectangle `(x, y, w, h)` to `inner`.
    pub(crate) fn clear_pattern(
        target: &Target,
        rgba: [f32; 4],
        scissor: [i32; 4],
        inner: [f32; 4],
    ) {
        let bind = target
            .framebuffer
            .bind(FbTarget::FRAMEBUFFER)
            .expect("bind the render framebuffer");
        draw::clear_color(rgba[0], rgba[1], rgba[2], rgba[3]);
        draw::clear(gl::COLOR_BUFFER_BIT);
        draw::enable(gl::SCISSOR_TEST).expect("enable GL_SCISSOR_TEST");
        // SAFETY: four ints to a GL setter.
        glad::context(|gl| unsafe { gl.Scissor(scissor[0], scissor[1], scissor[2], scissor[3]) });
        draw::clear_color(inner[0], inner[1], inner[2], inner[3]);
        draw::clear(gl::COLOR_BUFFER_BIT);
        draw::disable(gl::SCISSOR_TEST).expect("disable GL_SCISSOR_TEST");
        bind.unbind();
        assert_eq!(
            errors::get_error(),
            Ok(()),
            "clear pattern raised a GL error"
        );
    }

    /// `readPixelsAlloc` KAT (P7-W7 T1): tightly packed RGBA8, `width * 4` bytes a
    /// row, rows bottom-up (GL's origin), stored values returned verbatim. A 3x2
    /// target cleared red with its bottom-left texel green; 0.0 and 1.0 are
    /// fixed points of the sRGB encode, so the bytes are exact on either target
    /// format.
    #[test]
    fn target_read_pixels_alloc_returns_tight_rgba_rows() {
        let _guard = test_support::gl_lock();
        let Some(api) = test_support::offscreen_api(
            "target_read_pixels_alloc_returns_tight_rgba_rows",
            AlphaBlending::LinearCorrected,
        ) else {
            return;
        };

        let target = api.init_target(3, 2).expect("init_target");
        clear_pattern(
            &target,
            [1.0, 0.0, 0.0, 1.0],
            [0, 0, 1, 1],
            [0.0, 1.0, 0.0, 1.0],
        );

        let pixels = target.read_pixels_alloc().expect("read_pixels_alloc");
        // llvmpipe's pixels, declared defined for memcheck (a no-op natively).
        // Under G-VALGRIND the comparison below was reported as a conditional
        // jump on uninitialised bytes whose origin is a stack slot of llvmpipe's
        // JIT-compiled code, although every byte holds its expected value
        // (NEW-FINDINGS N521; the C program
        // `.port/tools/valgrind-llvmpipe-probe/readback-clear.c` gets the same
        // report for the same scissored clears and read-back, with no Rustty
        // code). Only these bytes are exempted, as in the golden tests (N412).
        rustty_zigstd::valgrind::memcheck::make_mem_defined(&pixels);
        const R: [u8; 4] = [255, 0, 0, 255];
        const G: [u8; 4] = [0, 255, 0, 255];
        #[rustfmt::skip]
        let expected: Vec<u8> = [
            G, R, R, // row 0 (bottom)
            R, R, R, // row 1
        ]
        .concat();
        assert_eq!(pixels, expected);
        assert_eq!(errors::get_error(), Ok(()));

        target.deinit();
        api.deinit();
    }

    /// `exportDmabuf` KAT (P7-W7 T1), behind the `RUSTTY_REQUIRE_DMABUF` pivot: a
    /// host whose surfaceless EGL display lacks `EGL_MESA_image_dma_buf_export`
    /// skips (or FAILS under the pivot). Checks the exported frame's metadata and
    /// that the export restored the GL state its `defer`s restore.
    #[cfg(unix)]
    #[test]
    fn target_export_dmabuf_exports_the_rendered_frame() {
        const TEST: &str = "target_export_dmabuf_exports_the_rendered_frame";
        // `DRM_FORMAT_ABGR8888` (`fourcc_code('A', 'B', '2', '4')`): the DRM layout
        // of a `GL_RGBA8` texture's bytes (R, G, B, A in memory).
        const DRM_FORMAT_ABGR8888: u32 = u32::from_le_bytes(*b"AB24");

        let _guard = test_support::gl_lock();
        let Some(api) = test_support::offscreen_api(TEST, AlphaBlending::LinearCorrected) else {
            return;
        };
        let ctx = api.egl();
        if !ctx.has_dmabuf_export() {
            let why = "the surfaceless EGL display has no EGL_MESA_image_dma_buf_export";
            test_support::require_dmabuf_or_skip(TEST, &why);
            eprintln!("{why}; skipping {TEST}");
            api.deinit();
            return;
        }

        let target = api.init_target(8, 4).expect("init_target");
        clear_pattern(
            &target,
            [0.0, 0.0, 1.0, 1.0],
            [0, 0, 1, 1],
            [1.0, 1.0, 1.0, 1.0],
        );

        let frame = target.export_dmabuf(ctx).expect("export_dmabuf");
        assert_eq!((frame.width, frame.height), (8, 4));
        assert!(frame.premultiplied);
        assert_eq!(
            frame.fourcc, DRM_FORMAT_ABGR8888,
            "fourcc {:#x}",
            frame.fourcc
        );
        let count = usize::from(frame.planes.count);
        assert!(
            (1..=crate::dmabuf::MAX_PLANES).contains(&count),
            "planes={count}"
        );
        for (i, fd) in frame.planes.fds.iter().enumerate() {
            assert_eq!(fd.is_some(), i < count, "plane {i} ownership");
        }
        assert!(
            frame.planes.strides[0] >= 8 * 4,
            "stride {} below one RGBA8 row",
            frame.planes.strides[0]
        );

        // The `defer`s: GL_FRAMEBUFFER_SRGB re-enabled, both bindings restored.
        // SAFETY: plain queries on the current context; each writes one GLint.
        let (srgb, read, drawn) = glad::context(|gl| unsafe {
            let (mut read, mut drawn) = (-1, -1);
            gl.GetIntegerv(gl::READ_FRAMEBUFFER_BINDING, &mut read);
            gl.GetIntegerv(gl::DRAW_FRAMEBUFFER_BINDING, &mut drawn);
            (gl.IsEnabled(gl::FRAMEBUFFER_SRGB), read, drawn)
        });
        assert_eq!(srgb, gl::TRUE, "GL_FRAMEBUFFER_SRGB not re-enabled");
        assert_eq!((read, drawn), (0, 0), "framebuffer bindings not restored");
        assert_eq!(errors::get_error(), Ok(()));

        frame.deinit();
        target.deinit();
        api.deinit();
    }
}
