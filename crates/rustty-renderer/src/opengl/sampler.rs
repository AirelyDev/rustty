//! Wrapper for handling samplers.
//!
//! PORTS: ghostty `src/renderer/opengl/Sampler.zig` (45 loc) — the renderer-facing
//! sampler wrapper layered over the `gl` package's [`gl::Sampler`](super::gl::Sampler)
//! leaf (Task 4). This is the OpenGL twin of `metal/sampler.rs`.
//!
//! PORT NOTE (`gl.Sampler` field, no ARC): the `gl` leaf is a `Copy` `GLuint`
//! handle with explicit `create`/`destroy`, so `deinit` is a real by-value method
//! (not a `Drop` impl), exactly as `Sampler.zig:43-45` has it. This wrapper is
//! deliberately NOT `Copy` and NOT `Drop` — see `opengl::texture` for the same
//! rationale.
//!
//! PORT NOTE (`Error{OpenGLFailed}`): as in `opengl::texture`, the reference
//! collapses every `gl` call's error into a single `error.OpenGLFailed`
//! (`Sampler.zig:33-36`), reproduced as `.map_err(|_| Error::OpenGLFailed)`.
//!
//! PORT NOTE (no `Binding`, `parameter` on the sampler): the `gl::Sampler` leaf has
//! no `Binding` type — `glSamplerParameteri` takes the sampler name directly — so
//! `parameter` is a method on the sampler and needs no prior bind. The reference's
//! `Sampler.zig` init drives exactly that (`sampler.parameter(.wrap_s, …)`), and it
//! is ported as-is.

use super::gl::sampler::Sampler as GlSampler;
use super::gl::texture::{MagFilter, MinFilter, Parameter, Wrap};

/// Options for initializing a sampler.
//
// PORTS: `Sampler.zig:13-18`. See `opengl::buffer::Options` for why this keeps the
// reference's per-file `Options` spelling.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub min_filter: MinFilter,
    pub mag_filter: MagFilter,
    pub wrap_s: Wrap,
    pub wrap_t: Wrap,
}

/// PORTS: `Sampler.zig:1-20` — a file-as-struct: the top-level `sampler` field IS
/// the struct.
pub struct Sampler {
    /// The underlying `gl.Sampler`.
    pub sampler: GlSampler,
}

/// PORTS: `Sampler.zig:22-25` — `pub const Error = error{ OpenGLFailed };`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Error {
    /// An OpenGL API call failed.
    OpenGLFailed,
}

impl Sampler {
    /// Initialize a sampler
    //
    // PORTS: `Sampler.zig:28-41`.
    //
    // ```zig
    // pub fn init(opts: Options) Error!Self {
    //     const sampler = gl.Sampler.create() catch return error.OpenGLFailed;
    //     errdefer sampler.destroy();
    //     sampler.parameter(.wrap_s, opts.wrap_s) catch return error.OpenGLFailed;
    //     sampler.parameter(.wrap_t, opts.wrap_t) catch return error.OpenGLFailed;
    //     sampler.parameter(.min_filter, opts.min_filter) catch return error.OpenGLFailed;
    //     sampler.parameter(.mag_filter, opts.mag_filter) catch return error.OpenGLFailed;
    //     return .{ .sampler = sampler };
    // }
    // ```
    //
    // PORT NOTE (`errdefer sampler.destroy()`): on any `parameter` failure the
    // reference destroys the sampler before returning the error. The closure below
    // reproduces that: run the four `parameter` calls, and on the first error
    // destroy the sampler and propagate.
    //
    // PORT NOTE (`adf161473`): the reference passes the typed enums straight
    // through (`value: name.Type()`), and so does this; `gl::Sampler::parameter`
    // does the `@intFromEnum`.
    pub fn init(opts: Options) -> Result<Self, Error> {
        let sampler = GlSampler::create().map_err(|_| Error::OpenGLFailed)?;

        let configured = (|| -> Result<(), Error> {
            sampler
                .parameter(Parameter::WrapS, opts.wrap_s)
                .map_err(|_| Error::OpenGLFailed)?;
            sampler
                .parameter(Parameter::WrapT, opts.wrap_t)
                .map_err(|_| Error::OpenGLFailed)?;
            sampler
                .parameter(Parameter::MinFilter, opts.min_filter)
                .map_err(|_| Error::OpenGLFailed)?;
            sampler
                .parameter(Parameter::MagFilter, opts.mag_filter)
                .map_err(|_| Error::OpenGLFailed)?;
            Ok(())
        })();

        if let Err(e) = configured {
            sampler.destroy(); // errdefer
            return Err(e);
        }

        Ok(Self { sampler })
    }

    /// PORTS: `Sampler.zig:43-45`.
    ///
    /// ```zig
    /// pub fn deinit(self: Self) void {
    ///     self.sampler.destroy();
    /// }
    /// ```
    ///
    /// PORT NOTE: consumes `self` (by value) — NOT a `Drop` impl. See the module
    /// note.
    pub fn deinit(self) {
        self.sampler.destroy();
    }
}
