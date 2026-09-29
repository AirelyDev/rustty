//! Wrapper for handling textures.
//!
//! PORTS: ghostty `src/renderer/opengl/Texture.zig` (104 loc) — the renderer-facing
//! texture wrapper layered over the `gl` package's [`gl::Texture`](super::gl::Texture)
//! leaf (Task 4). This is the OpenGL twin of `metal/texture.rs`.
//!
//! PORT NOTE (`gl.Texture` field, no ARC): the `gl` leaf is a `Copy` `GLuint`
//! handle with explicit `create`/`destroy`, so `deinit` is a real by-value method
//! (not a `Drop` impl), exactly as `Texture.zig:77-79` has it. This wrapper is
//! deliberately NOT `Copy` (a copy + double `deinit` would double-delete the shared
//! handle) and NOT `Drop` (the reference manages the handle explicitly; an RAII
//! `Drop` would be a deviation).
//!
//! PORT NOTE (`Error{OpenGLFailed}`, distinct from `gl::errors::Error`): unlike
//! `opengl::buffer` — which propagates the `gl` package's own `errors.Error` — the
//! reference's `Texture.zig` COLLAPSES every `gl` call's error into a single
//! `error.OpenGLFailed` via `catch return error.OpenGLFailed` (`Texture.zig:36-39`).
//! That mapping is reproduced here as `.map_err(|_| Error::OpenGLFailed)`.

use std::ffi::c_void;

use gl::types::{GLint, GLsizei};

use super::gl::texture::{
    DataType, Format, InternalFormat, MagFilter, MinFilter, Parameter, Target,
    Texture as GlTexture, Wrap,
};

/// Options for initializing a texture.
//
// PORTS: `Texture.zig:13-21`. See `opengl::buffer::Options` for why this keeps the
// reference's per-file `Options` spelling rather than a flattened `TextureOptions`.
//
// PORT NOTE (no `Default`): unlike `buffer.Options`, the reference gives these
// fields NO defaults — every one is required at every call site — so no `Default`
// impl is synthesized.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub format: Format,
    pub internal_format: InternalFormat,
    pub target: Target,
    pub min_filter: MinFilter,
    pub mag_filter: MagFilter,
    pub wrap_s: Wrap,
    pub wrap_t: Wrap,
}

/// PORTS: `Texture.zig:1-34` — a file-as-struct: the top-level
/// `texture`/`width`/`height`/`format`/`target` fields ARE the struct.
pub struct Texture {
    /// The underlying `gl.Texture`.
    pub texture: GlTexture,

    /// The width of this texture.
    pub width: usize,
    /// The height of this texture.
    pub height: usize,

    /// Format for this texture.
    pub format: Format,

    /// Target for this texture.
    pub target: Target,
}

/// PORTS: `Texture.zig:36-39` — `pub const Error = error{ OpenGLFailed };`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Error {
    /// An OpenGL API call failed.
    OpenGLFailed,
}

impl Texture {
    /// Initialize a texture
    //
    // PORTS: `Texture.zig:41-75`.
    //
    // ```zig
    // pub fn init(opts, width, height, data: ?[]const u8) Error!Self {
    //     const tex = gl.Texture.create() catch return error.OpenGLFailed;
    //     errdefer tex.destroy();
    //     {
    //         const texbind = tex.bind(opts.target) catch return error.OpenGLFailed;
    //         defer texbind.unbind();
    //         texbind.parameter(.wrap_s, opts.wrap_s) catch return error.OpenGLFailed;
    //         texbind.parameter(.wrap_t, opts.wrap_t) catch return error.OpenGLFailed;
    //         texbind.parameter(.min_filter, opts.min_filter) catch return error.OpenGLFailed;
    //         texbind.parameter(.mag_filter, opts.mag_filter) catch return error.OpenGLFailed;
    //         texbind.image2D(0, opts.internal_format, @intCast(width), @intCast(height),
    //             opts.format, .unsigned_byte, if (data) |d| @ptrCast(d.ptr) else null) catch return error.OpenGLFailed;
    //     }
    //     return .{ .texture = tex, .width = width, .height = height,
    //               .format = opts.format, .target = opts.target };
    // }
    // ```
    //
    // PORT NOTE (`errdefer tex.destroy()` + the inner `{ … defer texbind.unbind() }`
    // block): the block-scoped `defer` unbinds at the end of the parameter/image
    // sequence — on success AND on error — while the function-scoped `errdefer`
    // destroys the texture only on an error return, AFTER the unbind. The closure
    // below reproduces both scopes and that ordering (unbind before destroy). A
    // `bind` failure runs only the `errdefer destroy`.
    //
    // PORT NOTE (`adf161473`): the reference passes the typed `Wrap`/`MinFilter`/
    // `MagFilter` straight through (`value: name.Type()`), and so does this;
    // `gl::texture::Binding::parameter` does the `@intFromEnum`.
    pub fn init(
        opts: Options,
        width: usize,
        height: usize,
        data: Option<&[u8]>,
    ) -> Result<Self, Error> {
        let tex = GlTexture::create().map_err(|_| Error::OpenGLFailed)?;

        // The reference's inner `{ … }` block: bind, configure, `defer unbind`.
        let configured = (|| -> Result<(), Error> {
            let texbind = tex.bind(opts.target).map_err(|_| Error::OpenGLFailed)?;

            let inner = (|| -> Result<(), Error> {
                texbind
                    .parameter(Parameter::WrapS, opts.wrap_s)
                    .map_err(|_| Error::OpenGLFailed)?;
                texbind
                    .parameter(Parameter::WrapT, opts.wrap_t)
                    .map_err(|_| Error::OpenGLFailed)?;
                texbind
                    .parameter(Parameter::MinFilter, opts.min_filter)
                    .map_err(|_| Error::OpenGLFailed)?;
                texbind
                    .parameter(Parameter::MagFilter, opts.mag_filter)
                    .map_err(|_| Error::OpenGLFailed)?;

                // `if (data) |d| @ptrCast(d.ptr) else null`.
                let ptr: *const c_void = match data {
                    Some(d) => d.as_ptr().cast::<c_void>(),
                    None => std::ptr::null(),
                };

                // SAFETY: `image_2d` reads `width*height*<texel bytes of (format,
                // UnsignedByte)>` from `ptr` (or nothing, when null). For the sole
                // `Some` call site (`renderer/opengl/Texture.zig`) `data` is the
                // caller's texel buffer; the reference likewise does NOT bounds-
                // check it. A null `ptr` is explicitly legal (allocate-only).
                unsafe {
                    texbind.image_2d(
                        0,
                        opts.internal_format,
                        to_glsizei(width),
                        to_glsizei(height),
                        opts.format,
                        DataType::UNSIGNED_BYTE,
                        ptr,
                    )
                }
                .map_err(|_| Error::OpenGLFailed)?;

                Ok(())
            })();

            texbind.unbind(); // defer
            inner
        })();

        if let Err(e) = configured {
            tex.destroy(); // errdefer
            return Err(e);
        }

        Ok(Self {
            texture: tex,
            width,
            height,
            format: opts.format,
            target: opts.target,
        })
    }

    /// PORTS: `Texture.zig:77-79`.
    ///
    /// ```zig
    /// pub fn deinit(self: Self) void {
    ///     self.texture.destroy();
    /// }
    /// ```
    ///
    /// PORT NOTE: consumes `self` (by value) — NOT a `Drop` impl. See the module
    /// note.
    pub fn deinit(self) {
        self.texture.destroy();
    }

    /// Replace a region of the texture with the provided data.
    ///
    /// Does NOT check the dimensions of the data to ensure correctness.
    //
    // PORTS: `Texture.zig:84-104`.
    //
    // ```zig
    // pub fn replaceRegion(self, x, y, width, height, data: []const u8) Error!void {
    //     const texbind = self.texture.bind(self.target) catch return error.OpenGLFailed;
    //     defer texbind.unbind();
    //     texbind.subImage2D(0, @intCast(x), @intCast(y), @intCast(width), @intCast(height),
    //         self.format, .unsigned_byte, data.ptr) catch return error.OpenGLFailed;
    // }
    // ```
    //
    // PORT NOTE (`&self`): the reference takes `self: Self` by value, but this fn
    // only READS `self.texture`/`self.target`, so `&self` is behaviourally
    // identical and lets the caller keep the `Texture` (as `init`'s own
    // `this.replaceRegion(…)` does — though that path is not exercised here since
    // `init` inlines the `image2D` upload).
    pub fn replace_region(
        &self,
        x: usize,
        y: usize,
        width: usize,
        height: usize,
        data: &[u8],
    ) -> Result<(), Error> {
        let texbind = self
            .texture
            .bind(self.target)
            .map_err(|_| Error::OpenGLFailed)?;

        // SAFETY: `sub_image_2d` reads `width*height*<texel bytes>` from `data`'s
        // pointer; the doc note (the reference's own) states it does NOT bounds-
        // check `data` against the region — that contract is preserved.
        let res = unsafe {
            texbind.sub_image_2d(
                0,
                to_glint(x),
                to_glint(y),
                to_glsizei(width),
                to_glsizei(height),
                self.format,
                DataType::UNSIGNED_BYTE,
                data.as_ptr().cast::<c_void>(),
            )
        };
        texbind.unbind(); // defer
        res.map_err(|_| Error::OpenGLFailed)
    }
}

/// `@intCast(v)`: usize -> GLsizei. Zig `@intCast` traps on overflow; `try_from`
/// panics identically, matching the tree's established handling (see the `gl` leaf
/// `buffer::Binding::set_sub_data`). Texture dimensions within `GLsizei::MAX` are
/// the same precondition the reference already assumes.
fn to_glsizei(v: usize) -> GLsizei {
    GLsizei::try_from(v).expect("texture dimension exceeds GLsizei")
}

/// `@intCast(v)`: usize -> GLint. See [`to_glsizei`].
fn to_glint(v: usize) -> GLint {
    GLint::try_from(v).expect("texture offset exceeds GLint")
}
