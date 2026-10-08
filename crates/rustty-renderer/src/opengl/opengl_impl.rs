//! Graphics API wrapper for OpenGL.
//!
//! PORTS: ghostty `src/renderer/OpenGL.zig` — the `OpenGL = @This()` graphics-API
//! wrapper that `GenericRenderer(OpenGL)` is monomorphized over. Per the GPU R-d
//! sub-blueprint (`GPU_RD_TRAIT_DESIGN.md`) the port is MONOMORPHIC: the concrete
//! `OpenGL` leaf, no `GraphicsApi` trait hierarchy (this is the Linux sibling of
//! the already-landed `crate::metal::metal_impl::Metal`).
//!
//! PORT NOTE (R1 ledger, OpenGL.zig:13-24 — the `Renderer` alias + leaf-type
//! re-exports): ghostty declares
//!     const Renderer = rendererpkg.GenericRenderer(OpenGL);   // (line 13)
//!     pub const GraphicsAPI = OpenGL;                         // (15)
//!     pub const Target = @import("opengl/Target.zig");        // (16)
//!     pub const Frame = @import("opengl/Frame.zig");          // (17)
//!     pub const RenderPass = @import("opengl/RenderPass.zig");// (18)
//!     pub const Pipeline = @import("opengl/Pipeline.zig");    // (19)
//!     const bufferpkg = @import("opengl/buffer.zig");         // (20)
//!     pub const Buffer = bufferpkg.Buffer;                    // (21)
//!     pub const Sampler = @import("opengl/Sampler.zig");      // (22)
//!     pub const Texture = @import("opengl/Texture.zig");      // (23)
//!     pub const shaders = @import("opengl/shaders.zig");      // (24)
//! In the MONOMORPHIC port `GraphicsAPI = OpenGL` is the identity self-alias; the
//! leaf re-exports route to the already-landed leaf modules
//! `super::{target::Target, frame::Frame, render_pass::RenderPass,
//! pipeline::Pipeline, buffer::Buffer, sampler::Sampler, texture::Texture,
//! shaders}` (Tasks 5-7), reached via the `use super::…` imports below rather than
//! re-declared here. This comment is the R1 every-line ledger for lines 13-24.
//!
//! PORT NOTE (self: OpenGL by-value → &self): OpenGL.zig's option factories take
//! `self: OpenGL` (an `inline` copy). In Rust every read-only method takes
//! `&self`. `*const OpenGL` → `&self`, `*OpenGL` → `&mut self` (matching Metal's
//! precedent).
//!
//! P7-W7 T2 (`2f0b65346`, pin `12752b2ac`): the GtkGLArea is gone. `init` now
//! creates and OWNS a surfaceless EGL display + OpenGL 4.3 core context on the main
//! thread and releases it; the render thread makes it current in `threadEnter` and
//! loads its own GL function table, and `present` exports each frame for the apprt
//! (a DMA-BUF, or CPU memory when the apprt cannot import one). The pin's
//! `surfaceInit`/`finalizeSurfaceInit`/`displayRealized`/`presentLastTarget`/
//! `gpuResourcesReleased` are deleted, and so are the port's two constructors
//! (`init_offscreen` for the tests, `init` for GDK's borrowed context): there is
//! one context model now, the reference's.
//!
//! P8-W6 T2 (`40d5b860d`): the EGL display and config are the app-scoped
//! `opengl::device::Device` (`opengl/Device.zig`), which the core `App` creates
//! once and every renderer borrows (`device: *const Device`). `init` only binds
//! the API and creates this surface's context.
//!
//! DROPPED: the `alloc: std.mem.Allocator` field (OpenGL.zig:40) — the Rust port
//! does not thread an allocator (the leaves are `Vec`-managed), exactly as
//! `metal_impl` dropped it.

use std::ffi::CStr;
use std::fmt;

use rustty_render_types::AlphaBlending; // ← Part A hoist (no config edge)

use super::gl::buffer as glbuf;
use super::gl::glad;
use super::gl::sys::{self, OffscreenContext};
// `pub const Device = @import("opengl/Device.zig");` (OpenGL.zig:15, `40d5b860d`).
pub use super::device::Device;
use super::gl::texture as gltex;
use super::gl::{draw, errors};

use super::buffer;
use super::frame::{CompletionFn, Frame};
use super::sampler;
use super::shaders::{self, Shaders};
use super::target::{self, Target};
use super::texture::{self, Texture};

// const log = std.log.scoped(.opengl); (OpenGL.zig:34)
//
// The `log` facade's `target` for every call site ported out of `OpenGL.zig`. It is
// the SAME `.opengl` scope every file in `pkg/opengl` and `src/renderer/opengl`
// declares (`gl.rs`'s macro header spells that out), so `opengl/shaders.rs` and the
// `Frame.zig` tail inlined into `generic/linux_impl.rs` both target this constant
// rather than declaring their own copy.
pub(crate) const LOG_SCOPE: &str = "opengl";

/// Names the anonymous `struct { width: u32, height: u32 }` that `surfaceSize`
/// returns (OpenGL.zig:239). Mirrors `metal_impl::SurfaceSize`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SurfaceSize {
    pub width: u32,
    pub height: u32,
}

/// OpenGL-local error.
///
/// PORT NOTE: `OpenGL.init` returns Zig's inferred set of the `egl.load`/
/// `initPlatform`/`bindApi`/`Config.choose`/`Context.create`/`makeCurrent`
/// failures; the port's EGL half ([`OffscreenContext`] and, app-scoped since
/// `40d5b860d`, [`Device`]) folds them into
/// [`Error::Context`]. `threadEnter`'s `makeCurrent` is [`Error::Egl`] and its
/// `prepareContext` [`Error::Context`]; `present`'s readback is [`Error::Gl`].
#[derive(Debug)]
pub enum Error {
    /// The EGL display or context could not be created, or the context did not
    /// reach the [`sys::MIN_VERSION_MAJOR`].[`sys::MIN_VERSION_MINOR`] core floor.
    /// Carries the underlying [`sys::Error`] so a caller (or a test) can
    /// distinguish a clean skip (absent EGL / absent driver / version under the
    /// floor) from a real port break (a missing entry point on a context EGL
    /// promised was >= 4.3).
    Context(sys::Error),
    /// A `gl` call failed (`present`'s `readPixelsAlloc`).
    Gl(errors::Error),
    /// An EGL call failed: `threadEnter`'s `makeCurrent` (`OpenGL.zig:224`).
    Egl(sys::EglError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Context(err) => write!(f, "OpenGL context setup failed: {err}"),
            Self::Gl(err) => write!(f, "OpenGL error: {err}"),
            Self::Egl(err) => write!(f, "EGL error: {err}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<sys::Error> for Error {
    fn from(err: sys::Error) -> Self {
        Self::Context(err)
    }
}

impl From<errors::Error> for Error {
    fn from(err: errors::Error) -> Self {
        Self::Gl(err)
    }
}

impl From<sys::EglError> for Error {
    fn from(err: sys::EglError) -> Self {
        Self::Egl(err)
    }
}

/// A finished frame exported for presentation by the apprt.
///
/// PORTS: `OpenGL.zig:322-345` — `pub const ExportedFrame = union(enum)`.
///
/// ```zig
/// pub const ExportedFrame = union(enum) {
///     dmabuf: Dmabuf,
///     memory: Memory,
///     ...
///     pub fn deinit(self: ExportedFrame) void {
///         switch (self) {
///             .dmabuf => |v| v.deinit(),
///             .memory => |v| v.deinit(),
///         }
///     }
/// };
/// ```
///
/// PORT NOTE: `deinit` is the frame's `Drop` (the DMA-BUF owns its fds, the
/// memory frame its pixels); [`ExportedFrame::deinit`] is kept so the reference's
/// call sites read the same. The `dmabuf` arm is Unix-only, as `crate::dmabuf` is:
/// Windows builds the OpenGL renderer but has no DMA-BUF, so there every export
/// takes `present`'s CPU fallback.
#[derive(Debug)]
pub enum ExportedFrame {
    #[cfg(unix)]
    Dmabuf(crate::dmabuf::Dmabuf),
    Memory(Memory),
}

/// RGBA8 pixel data with premultiplied alpha, tightly packed
/// (`width * 4` bytes per row), in CPU memory.
//
// PORTS: `ExportedFrame.Memory` (`OpenGL.zig:327-338`). The `alloc` field and
// `deinit` (`self.alloc.free(self.pixels)`) are the `Vec`'s own.
#[derive(Debug)]
pub struct Memory {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl ExportedFrame {
    /// PORTS: `ExportedFrame.deinit`; see the type's PORT NOTE.
    pub fn deinit(self) {}
}

impl crate::latest_frame::ExportedFrameType for ExportedFrame {
    const IS_VOID: bool = false;
}

/// Graphics API wrapper for OpenGL. Ports `OpenGL.zig`'s `OpenGL = @This()` struct
/// (fields OpenGL.zig:40-47).
pub struct OpenGL {
    // OpenGL.zig:40 `alloc: std.mem.Allocator` — DROPPED (see the module note).
    /// Alpha blending mode
    // OpenGL.zig:43 `blending: configpkg.Config.AlphaBlending`. Now the Part-A
    // hoisted `rustty_render_types::AlphaBlending` (no config edge). Drives sRGB
    // internal-format selection in `init_target` (UD354).
    pub blending: AlphaBlending,

    /// The shared render device.
    //
    // `device: *const Device` (OpenGL.zig:47-48, `40d5b860d`): the app-scoped EGL
    // display and config (and the dlopen'd libEGL), owned by the core `App`
    // (`DeviceBox`, N522), which frees it after every surface. Read through
    // [`OpenGL::device`].
    device: *const Device,

    // egl_context: *gl.egl.Context,   (OpenGL.zig:50)
    //
    // PORT NOTE: the port's `OffscreenContext`, the surface-scoped context on the
    // device's display. Its `Drop` is `deinit`'s release + destroy; it never
    // terminates the display (DECISIONS B10).
    egl_context: OffscreenContext,
}

impl OpenGL {
    // OpenGL.zig:26 `pub const custom_shader_target: shadertoy.Target = .glsl;`
    pub const CUSTOM_SHADER_TARGET: crate::shadertoy::Target = crate::shadertoy::Target::Glsl;
    // OpenGL.zig:27-28 — the fragCoord for OpenGL shaders is +Y = up.
    // `pub const custom_shader_y_is_down = false;`
    pub const CUSTOM_SHADER_Y_IS_DOWN: bool = false;
    /// Triple-buffering gives the GPU room to pipeline renders without
    /// having to wait on the apprt consuming previous frames.
    // OpenGL.zig:31-33 `pub const swap_chain_count = 3;`
    pub const SWAP_CHAIN_COUNT: usize = 3;

    /// PORTS: `OpenGL.zig:52-90` — `init`.
    ///
    /// ```zig
    /// pub fn init(
    ///     alloc: Allocator,
    ///     device: *const Device,
    ///     opts: rendererpkg.Options,
    /// ) !OpenGL {
    ///     const display = device.display;
    ///
    ///     // Bind to OpenGL API explicitly before we create the context,
    ///     // as otherwise it might fallback to OpenGL ES
    ///     try egl.bindApi(egl.c.EGL_OPENGL_API);
    ///
    ///     // Create our context. Contexts are surface-scoped because a
    ///     // context can only be current on one thread at a time.
    ///     const context = egl.Context.create(display, device.config, null, &.{ ... }) catch |err| { ... };
    ///     errdefer context.destroy(display) catch {};
    ///
    ///     display.makeCurrent(null, null, context) catch |err| { ... };
    ///
    ///     // Release current so that the main thread
    ///     // doesn't hold onto the GL context forever.
    ///     defer display.releaseCurrent();
    ///
    ///     return .{ ... };
    /// }
    /// ```
    ///
    /// The bind/context/current half, with its log lines, is
    /// [`OffscreenContext::init`]. PORT NOTE: the reference takes the whole
    /// `rendererpkg.Options` and reads only `opts.config.blending`; the port takes
    /// that one value (the renderer crate's `Options` is the caller's).
    ///
    /// # Safety
    /// `device` points at a live, initialized [`Device`] that outlives the
    /// returned `OpenGL` (`Options.device`; the core `App` frees it after every
    /// surface, `App.zig` `deinit`).
    ///
    /// # Errors
    /// [`Error::Context`] when the API cannot be bound or a 4.3 core context
    /// cannot be created or made current on the device's display.
    pub unsafe fn init(device: *const Device, blending: AlphaBlending) -> Result<OpenGL, Error> {
        // SAFETY: the caller's contract.
        let egl_context = unsafe { OffscreenContext::init(device) }?;

        // Release current so that the main thread
        // doesn't hold onto the GL context forever.
        egl_context.release_current();

        Ok(OpenGL {
            blending,
            device,
            egl_context,
        })
    }

    /// TEST-ONLY (RUSTTY): [`Self::init`] on the process's test device
    /// (`gl::test_support::device`), then [`Self::thread_enter`] on the calling
    /// thread and [`Self::set_viewport`] to `width x height` — what the render
    /// thread does before its first draw (`threadEnter`, then the first `.resize`
    /// message). The renderer's own KATs run on the test thread, with no render
    /// thread, so they stand the context up here.
    #[cfg(test)]
    pub(crate) fn init_offscreen(
        blending: AlphaBlending,
        width: u32,
        height: u32,
    ) -> Result<OpenGL, Error> {
        let device = super::gl::test_support::device().map_err(|e| {
            // The test device's init error, re-wrapped (it is not `Clone`).
            Error::Context(sys::Error::EglUnavailable(e.to_string()))
        })?;
        // SAFETY: the test device is process-lived.
        let api = unsafe { Self::init(device, blending) }?;
        api.thread_enter()?;
        api.set_viewport(width, height);
        Ok(api)
    }

    /// The shared render device (`self.device.*`).
    #[inline]
    pub fn device(&self) -> &Device {
        // SAFETY: `init`'s contract: the device outlives this `OpenGL`. It is only
        // read, on any thread (see `opengl::device`'s sharing note).
        unsafe { &*self.device }
    }

    /// The surface's EGL context, which the DMA-BUF export takes where the
    /// reference takes `self.device.display`/`self.egl_context`
    /// (`OpenGL.zig:285`).
    pub fn egl_context(&self) -> &OffscreenContext {
        &self.egl_context
    }

    /// PORTS: `OpenGL.zig:92-96` — `deinit`.
    ///
    /// ```zig
    /// pub fn deinit(self: *OpenGL) void {
    ///     self.device.display.releaseCurrent();
    ///     self.egl_context.destroy(self.device.display) catch {};
    ///     self.* = undefined;
    /// }
    /// ```
    ///
    /// Consuming: `OffscreenContext`'s `Drop` is that body (release, destroy).
    /// The display is the device's and is never terminated (`Device.zig:91-96`).
    pub fn deinit(self) {}

    /// Callback called by renderer.Thread when it begins. Called on the render
    /// thread. The EGL context was created at `init` time on the main thread;
    /// here we (re)bind it to this thread and load the thread-local glad
    /// function pointers so all subsequent GL work on this thread is valid.
    ///
    /// PORTS: `OpenGL.zig:218-227`.
    ///
    /// ```zig
    /// pub fn threadEnter(self: *OpenGL, surface: *apprt.Surface) !void {
    ///     _ = surface;
    ///     try self.device.display.makeCurrent(null, null, self.egl_context);
    ///     // Load our function pointers for this thread's threadlocal.
    ///     try prepareContext(&gl.egl.getProcAddress);
    /// }
    /// ```
    pub fn thread_enter(&self) -> Result<(), Error> {
        self.egl_context.make_current()?;
        // Load our function pointers for this thread's threadlocal.
        sys::load_gl(&self.egl_context)?;
        Ok(())
    }

    /// Callback called by renderer.Thread when it exits. Called on the render
    /// thread; unbinds the context from this thread so it can be destroyed on
    /// the main thread.
    ///
    /// PORTS: `OpenGL.zig:229-235`.
    ///
    /// ```zig
    /// pub fn threadExit(self: *OpenGL) void {
    ///     self.device.display.releaseCurrent();
    ///     gl.glad.unload();
    /// }
    /// ```
    pub fn thread_exit(&self) {
        self.egl_context.release_current();
        glad::unload();
    }

    /// Get the current size of the runtime surface.
    ///
    /// PORTS: `OpenGL.zig:237-246`.
    ///
    /// ```zig
    /// pub fn surfaceSize(self: *const OpenGL) !struct { width: u32, height: u32 } {
    ///     _ = self;
    ///     var viewport: [4]gl.c.GLint = undefined;
    ///     gl.glad.context.GetIntegerv.?(gl.c.GL_VIEWPORT, &viewport);
    ///     return .{
    ///         .width = @intCast(viewport[2]),
    ///         .height = @intCast(viewport[3]),
    ///     };
    /// }
    /// ```
    ///
    /// The viewport is the one [`Self::set_viewport`] last set (`setScreenSize`);
    /// a surfaceless context starts with a 0x0 viewport, so nothing draws before
    /// the first resize.
    pub fn surface_size(&self) -> Result<SurfaceSize, Error> {
        let mut viewport: [gl::types::GLint; 4] = [0; 4];
        // SAFETY: `GetIntegerv(GL_VIEWPORT, ...)` writes 4 GLints; the array is
        // exactly 4 wide. Core GL 1.0 — force-unwrapped by the reference (`.?`);
        // `glad::context` panics on an unloaded table, matching that
        // force-unwrap on a thread that never entered.
        glad::context(|gl| unsafe { gl.GetIntegerv(gl::VIEWPORT, viewport.as_mut_ptr()) });
        Ok(SurfaceSize {
            // @intCast → `as u32` (viewport width/height are non-negative).
            width: viewport[2] as u32,
            height: viewport[3] as u32,
        })
    }

    /// Set the GL viewport to cover the given size in device pixels.
    ///
    /// This used to be automatically called by the GtkGLArea upon resizing,
    /// but now we need to do this manually.
    ///
    /// PORTS: `OpenGL.zig:248-257`.
    ///
    /// ```zig
    /// pub fn setViewport(self: *const OpenGL, width: u32, height: u32) void {
    ///     _ = self;
    ///     gl.viewport(0, 0, @intCast(width), @intCast(height)) catch |err| {
    ///         log.warn("failed to set OpenGL viewport err={}", .{err});
    ///     };
    /// }
    /// ```
    ///
    /// PORT NOTE (R7): `@intCast` of a `u32` to `GLsizei` traps above
    /// `i32::MAX`; `i32::try_from(..).expect(..)` is the same assert.
    pub fn set_viewport(&self, width: u32, height: u32) {
        let width = gl::types::GLsizei::try_from(width).expect("viewport width exceeds GLsizei");
        let height = gl::types::GLsizei::try_from(height).expect("viewport height exceeds GLsizei");
        if let Err(err) = draw::viewport(0, 0, width, height) {
            log::warn!(target: LOG_SCOPE, "failed to set OpenGL viewport err={err:?}");
        }
    }

    /// Actions taken before doing anything in `drawFrame`.
    ///
    /// Right now there's nothing we need to do for OpenGL (GL has no per-frame
    /// autorelease-pool equivalent — contrast Metal's pool push).
    // pub fn drawFrameStart(self: *OpenGL) void { _ = self; }  (OpenGL.zig:259-263)
    pub fn draw_frame_start(&mut self) {}

    /// Actions taken after `drawFrame` is done.
    ///
    /// Right now there's nothing we need to do for OpenGL.
    // pub fn drawFrameEnd(self: *OpenGL) void { _ = self; }    (OpenGL.zig:265-269)
    pub fn draw_frame_end(&mut self) {}

    /// Initialize our shader set.
    // pub fn initShaders(self, alloc, custom_shaders: []const [:0]const u8) !shaders.Shaders
    //   (OpenGL.zig:271-283)
    //
    // PORT NOTE: the reference `alloc` param is ELIDED — `Shaders::init`
    // (`opengl/shaders.rs`) does not thread an allocator (Vec-managed), matching
    // how the leaf was ported in Task 7. Unlike Metal, OpenGL's `initShaders` does
    // NOT pass a pixel format (GL selects the framebuffer's sRGB-ness via the
    // target's internal format + GL_FRAMEBUFFER_SRGB, not via the pipeline). The
    // reference discards `self` too. `custom_shaders: []const [:0]const u8` →
    // `&[&CStr]`.
    pub fn init_shaders(&self, custom_shaders: &[&CStr]) -> Result<Shaders, shaders::Error> {
        Shaders::init(custom_shaders)
    }

    /// Initialize a new render target which can be presented by this API.
    //
    // PORTS: `OpenGL.zig:285-292` (P7-W7 T1).
    //
    // pub fn initTarget(self: *const OpenGL, width: usize, height: usize) !Target {
    //     _ = self;
    //     return Target.init(.{
    //         .width = width,
    //         .height = height,
    //     });
    // }
    //
    // R13 FIX (DECISIONS M5, UD354): the reference passes only the size and its
    // `Target` always renders into `SRGB8_ALPHA8`, which encodes the already
    // sRGB-encoded output of the `native` shaders a second time. The port keeps
    // the pin's format choice (`OpenGL.zig:289-296` at `20abdb50a`); see
    // `target::Options::internal_format`.
    pub fn init_target(&self, width: usize, height: usize) -> Result<Target, target::Error> {
        Target::init(target::Options {
            width,
            height,
            // Using an sRGB internal format makes GL gamma-encode the pixels
            // written to it *after* blending, i.e. linear (correct) alpha blending
            // rather than gamma-incorrect blending.
            internal_format: if self.blending.is_linear() {
                gltex::InternalFormat::SRGBA
            } else {
                gltex::InternalFormat::RGBA
            },
        })
    }

    /// Export a rendered target. Caller takes ownership
    /// of the frame and is responsible for freeing it.
    ///
    /// This runs on the render thread.
    ///
    /// PORTS: `OpenGL.zig:294-320`.
    ///
    /// ```zig
    /// pub fn present(
    ///     self: *OpenGL,
    ///     target: Target,
    ///     presentation_health: rendererpkg.Health,
    /// ) !ExportedFrame {
    ///     // We only export DMABUFs when the apprt can present them.
    ///     // Otherwise, use CPU buffers.
    ///     if (presentation_health == .healthy) {
    ///         if (target.exportDmabuf(self.device.display, self.egl_context)) |dmabuf| {
    ///             return .{ .dmabuf = dmabuf };
    ///         } else |_| {
    ///             log.warn("failed to export DMABUF, falling back to CPU presentation", .{});
    ///         }
    ///     }
    ///
    ///     return .{ .memory = .{
    ///         .width = @intCast(target.width),
    ///         .height = @intCast(target.height),
    ///         .pixels = try target.readPixelsAlloc(self.alloc),
    ///         .alloc = self.alloc,
    ///     } };
    /// }
    /// ```
    ///
    /// PORT NOTE: `target: Target` is a by-value copy of the frame's target in
    /// Zig; the Rust `Target` is move-only and still owned by the swap chain, so
    /// the port borrows it. `&self`: nothing here writes the api.
    pub fn present(
        &self,
        target: &Target,
        presentation_health: crate::Health,
    ) -> Result<ExportedFrame, Error> {
        // We only export DMABUFs when the apprt can present them.
        // Otherwise, use CPU buffers.
        //
        // PORT NOTE (Windows, port-only): DMA-BUFs exist only on unix, so off
        // unix the export attempt and its warn are compiled out and every frame
        // goes straight to CPU memory. The reference builds no Windows library
        // (UD359), so there is no reference behaviour to follow there.
        #[cfg(unix)]
        if presentation_health == crate::Health::Healthy {
            if let Ok(dmabuf) = target.export_dmabuf(&self.egl_context) {
                return Ok(ExportedFrame::Dmabuf(dmabuf));
            } else {
                log::warn!(
                    target: LOG_SCOPE,
                    "failed to export DMABUF, falling back to CPU presentation"
                );
            }
        }
        #[cfg(not(unix))]
        let _ = presentation_health;

        // PORT NOTE (R7): `@intCast(target.width)` traps above `u32::MAX`.
        Ok(ExportedFrame::Memory(Memory {
            width: u32::try_from(target.width).expect("Target width exceeds u32"),
            height: u32::try_from(target.height).expect("Target height exceeds u32"),
            pixels: target.read_pixels_alloc()?,
        }))
    }

    /// Returns the options to use when constructing buffers.
    // pub inline fn bufferOptions(self: OpenGL) bufferpkg.Options  (OpenGL.zig:339-345)
    fn buffer_options(&self) -> buffer::Options {
        buffer::Options {
            target: glbuf::Target::ARRAY,
            usage: glbuf::Usage::DYNAMIC_DRAW,
        }
    }

    // The six Zig `pub const … = bufferOptions;` aliases (OpenGL.zig:347-352) → six
    // forwarders (Rust cannot alias methods by `pub const`, same as Metal). Names
    // MUST match what `generic.zig`/`image.zig` call.
    pub fn instance_buffer_options(&self) -> buffer::Options {
        self.buffer_options()
    }
    pub fn uniform_buffer_options(&self) -> buffer::Options {
        self.buffer_options()
    }
    pub fn fg_buffer_options(&self) -> buffer::Options {
        self.buffer_options()
    }
    pub fn bg_buffer_options(&self) -> buffer::Options {
        self.buffer_options()
    }
    pub fn image_buffer_options(&self) -> buffer::Options {
        self.buffer_options()
    }
    pub fn bg_image_buffer_options(&self) -> buffer::Options {
        self.buffer_options()
    }

    /// Returns the options to use when constructing textures.
    // pub inline fn textureOptions(self: OpenGL) Texture.Options  (OpenGL.zig:354-366)
    pub fn texture_options(&self) -> texture::Options {
        texture::Options {
            format: gltex::Format::RGBA,
            internal_format: gltex::InternalFormat::SRGBA,
            target: gltex::Target::Texture2D,
            min_filter: gltex::MinFilter::Linear,
            mag_filter: gltex::MagFilter::Linear,
            wrap_s: gltex::Wrap::ClampToEdge,
            wrap_t: gltex::Wrap::ClampToEdge,
        }
    }

    /// Returns the options to use when constructing samplers.
    // pub inline fn samplerOptions(self: OpenGL) Sampler.Options  (OpenGL.zig:369-377)
    pub fn sampler_options(&self) -> sampler::Options {
        sampler::Options {
            // These parameters match Shadertoy behaviors.
            min_filter: gltex::MinFilter::Linear,
            mag_filter: gltex::MagFilter::Linear,
            wrap_s: gltex::Wrap::ClampToEdge,
            wrap_t: gltex::Wrap::ClampToEdge,
        }
    }

    /// Returns the options to use when constructing textures for images.
    // pub inline fn imageTextureOptions(self, format, srgb) Texture.Options
    //   (OpenGL.zig:398-419)
    pub fn image_texture_options(
        &self,
        format: ImageTextureFormat,
        srgb: bool,
    ) -> texture::Options {
        texture::Options {
            format: format.to_pixel_format(),
            internal_format: if srgb {
                gltex::InternalFormat::SRGBA
            } else {
                gltex::InternalFormat::RGBA
            },
            target: gltex::Target::Texture2D,
            // TODO: Generate mipmaps for image textures and use
            //       linear_mipmap_linear filtering so that they
            //       look good even when scaled way down.
            min_filter: gltex::MinFilter::Linear,
            mag_filter: gltex::MagFilter::Linear,
            // TODO: Separate out background image options, use
            //       repeating coordinate modes so we don't have
            //       to do the modulus in the shader.
            wrap_s: gltex::Wrap::ClampToEdge,
            wrap_t: gltex::Wrap::ClampToEdge,
        }
    }

    /// Initializes a Texture suitable for the provided font atlas.
    // pub fn initAtlasTexture(self: *const OpenGL, atlas: *const font.Atlas) Texture.Error!Texture
    //   (OpenGL.zig:421-448)
    //
    // PORT NOTE: `rustty_font::atlas::Atlas` exposes `format`/`size` as accessors
    // (`atlas.format()`/`atlas.size()`), same as Metal's port. `atlas.size()` is
    // `u32`; `Texture::init` takes `usize`, widened with `as usize` (Zig widens
    // implicitly).
    pub fn init_atlas_texture(
        &self,
        atlas: &rustty_font::atlas::Atlas,
    ) -> Result<Texture, texture::Error> {
        // const format, internal_format = switch (atlas.format) {
        //     .grayscale => .{ .red, .red },
        //     .bgra => .{ .bgra, .srgba },
        //     else => @panic("unsupported atlas format for OpenGL texture"),
        // };
        let (format, internal_format) = match atlas.format() {
            rustty_font::atlas::Format::Grayscale => {
                (gltex::Format::RED, gltex::InternalFormat::RED)
            }
            rustty_font::atlas::Format::Bgra => (gltex::Format::BGRA, gltex::InternalFormat::SRGBA),
            // Zig `else => @panic(...)` — catches `.bgr` (Format::Bgr).
            _ => panic!("unsupported atlas format for OpenGL texture"),
        };

        Texture::init(
            texture::Options {
                format,
                internal_format,
                target: gltex::Target::Rectangle,
                min_filter: gltex::MinFilter::Nearest,
                mag_filter: gltex::MagFilter::Nearest,
                wrap_s: gltex::Wrap::ClampToEdge,
                wrap_t: gltex::Wrap::ClampToEdge,
            },
            atlas.size() as usize,
            atlas.size() as usize,
            None,
        )
    }

    /// Begin a frame.
    // pub inline fn beginFrame(self: *const OpenGL, renderer: *Renderer, target: *Target) !Frame
    //   (OpenGL.zig:451-461)
    //
    // PORT NOTE (`!Frame` vs `Frame`): the reference `beginFrame` is fallible for
    // parity with Metal, but `opengl/Frame.zig`'s `begin` has an EMPTY error set —
    // so `frame::Frame::begin` (Task 6) is infallible and this returns `Frame`
    // directly (not `Result`). Contrast `metal_impl::begin_frame`, whose leaf CAN
    // fail. The reference `renderer: *Renderer` + the `frameCompleted`-on-completion
    // contract map to the type-erased `renderer: *mut ()` + the `on_completed`
    // fn-pointer the renderer task (Tasks 9-11) installs — same shape as Metal.
    //
    // Reference doc-comments on the params (R1):
    //   renderer — Once the frame has been completed, the `frameCompleted` method
    //              on the renderer is called with the health status of the frame.
    //   target   — The target is presented via the provided renderer's API when
    //              completed.
    pub fn begin_frame(
        &self,
        renderer: *mut (),
        target: *mut Target,
        on_completed: CompletionFn,
    ) -> Frame {
        Frame::begin(super::frame::Options, renderer, target, on_completed)
    }
}

/// Pixel format for image texture options.
// pub const ImageTextureFormat = enum { gray, rgba, bgra };  (OpenGL.zig:380-395)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageTextureFormat {
    /// 1 byte per pixel grayscale.
    Gray,
    /// 4 bytes per pixel RGBA.
    Rgba,
    /// 4 bytes per pixel BGRA.
    Bgra,
}

impl ImageTextureFormat {
    // fn toPixelFormat(self: ImageTextureFormat) gl.Texture.Format  (OpenGL.zig:388-394)
    fn to_pixel_format(self) -> gltex::Format {
        match self {
            ImageTextureFormat::Gray => gltex::Format::RED,
            ImageTextureFormat::Rgba => gltex::Format::RGBA,
            ImageTextureFormat::Bgra => gltex::Format::BGRA,
        }
    }
}

// PORT NOTE (rustty-only smoke test, NOT a ghostty parity test): there is no
// reference `test{}` over `OpenGL.zig`. This is the OpenGL twin of
// `metal_impl`'s `metal_offscreen_construct_and_begin_frame`: it drives `OpenGL`
// (not the raw leaves) through `init` + `thread_enter` + `set_viewport`
// (`init_offscreen`) → field asserts → `init_shaders` (all five pipelines) →
// `init_target` → clean teardown. Skips cleanly on hosts with no OpenGL 4.3 core
// context (the Task-1 OPEN RISK path), mirroring the Metal "no device → skip"
// precedent.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::opengl::gl::test_support;

    #[test]
    fn opengl_init_offscreen_then_deinit_round_trips() {
        // Hold the process-wide GL lock for the whole context lifetime: this test
        // stands up its own context, which the shared lock serializes against
        // every other GL test (`test_support::gl_lock` docs). `_guard` is declared
        // FIRST so it drops LAST — after `ogl.deinit()` destroys the context.
        let _guard = test_support::gl_lock();

        let ogl = match OpenGL::init_offscreen(AlphaBlending::Native, 256, 256) {
            Ok(o) => o,
            // PORT break — must FAIL, not skip: a GL entry point missing, or a GL
            // error, on a context EGL already promised is >= 4.3 core. These come
            // only from `prepare_context` (`load_gl`), i.e. AFTER context creation
            // succeeded, so they indicate a broken loader/port rather than a host
            // that merely lacks 4.3. Exactly the two errors
            // `test_support::offscreen_gl` panics on.
            Err(e @ Error::Context(sys::Error::MissingGlEntryPoint(_) | sys::Error::Gl(_))) => {
                panic!("init_offscreen failed on a live GL context: {e}");
            }
            // Everything else is the host being UNABLE to stand up a 4.3-core
            // context — absent EGL, no surfaceless platform, no desktop-GL config,
            // an `EGL_BAD_MATCH` refusing the 4.3-core request, or a reported
            // version under the floor. All are a CLEAN SKIP, mirroring
            // `test_support::offscreen_gl`. W8-03 strict mode: under
            // RUSTTY_REQUIRE_GL=1 the clean skip becomes a hard failure.
            Err(e) => {
                test_support::require_gl_or_skip(
                    "opengl_init_offscreen_then_deinit_round_trips",
                    &e,
                );
                eprintln!(
                    "no headless OpenGL {}.{} core context ({e}); skipping \
                     opengl_init_offscreen_then_deinit_round_trips",
                    sys::MIN_VERSION_MAJOR,
                    sys::MIN_VERSION_MINOR,
                );
                return;
            }
        };

        // Fields (OpenGL.zig:40-47 analogues).
        assert_eq!(ogl.blending, AlphaBlending::Native);
        // surface_size reads the GL viewport `set_viewport` set.
        assert_eq!(
            ogl.surface_size().unwrap(),
            SurfaceSize {
                width: 256,
                height: 256
            }
        );

        // initShaders → all five pipelines built, no post pipelines (empty custom
        // list).
        let mut shaders = ogl.init_shaders(&[]).expect("init_shaders");
        assert!(shaders.post_pipelines.is_empty());

        // initTarget → an offscreen texture-backed FBO of the requested size.
        let target = ogl.init_target(10, 10).expect("init_target");
        assert_eq!(target.width, 10);
        assert_eq!(target.height, 10);

        // Clean teardown, in order: leaf GL objects first (while the context is
        // still current), then the context (via `deinit`, which drops it).
        target.deinit();
        shaders.deinit();
        ogl.thread_exit();
        ogl.deinit();
    }

    /// `init` leaves the context current on NO thread (`defer
    /// display.releaseCurrent()`, `OpenGL.zig:100-102`) and loads no GL table:
    /// the render thread's `threadEnter` does both. A GL call on the creating
    /// thread before `thread_enter` therefore has no table to go through.
    #[test]
    fn init_releases_the_context_and_loads_no_table() {
        let _guard = test_support::gl_lock();
        glad::unload();
        let device = match test_support::device() {
            Ok(d) => d,
            Err(e) => {
                test_support::require_gl_or_skip("init_releases_the_context_and_loads_no_table", e);
                eprintln!("no EGL ({e}); skipping init_releases_the_context_and_loads_no_table");
                return;
            }
        };
        // SAFETY: the test device is process-lived.
        let api = match unsafe { OpenGL::init(device, AlphaBlending::LinearCorrected) } {
            Ok(api) => api,
            Err(e) => {
                test_support::require_gl_or_skip(
                    "init_releases_the_context_and_loads_no_table",
                    &e,
                );
                eprintln!("no EGL ({e}); skipping init_releases_the_context_and_loads_no_table");
                return;
            }
        };
        assert!(!glad::is_loaded(), "init loads no GL table");
        api.thread_enter().expect("thread_enter");
        assert!(glad::is_loaded(), "thread_enter loads this thread's table");
        // A surfaceless context starts with an empty viewport: nothing draws until
        // the first `setScreenSize` → `setViewport`.
        assert_eq!(
            api.surface_size().unwrap(),
            SurfaceSize {
                width: 0,
                height: 0
            }
        );
        api.set_viewport(12, 7);
        assert_eq!(
            api.surface_size().unwrap(),
            SurfaceSize {
                width: 12,
                height: 7
            }
        );
        api.thread_exit();
        assert!(!glad::is_loaded());
        api.deinit();
    }

    /// `present` with an UNHEALTHY presentation (the apprt could not import a
    /// DMA-BUF) never tries the export and hands back the target's pixels in CPU
    /// memory, tightly packed RGBA8 (`OpenGL.zig:303-319`).
    #[test]
    fn present_reads_back_to_memory_when_presentation_is_unhealthy() {
        let _guard = test_support::gl_lock();
        let Some(api) = test_support::offscreen_api(
            "present_reads_back_to_memory_when_presentation_is_unhealthy",
            AlphaBlending::LinearCorrected,
        ) else {
            return;
        };
        let target = api.init_target(2, 1).expect("init_target");
        crate::opengl::target::tests::clear_pattern(
            &target,
            [0.0, 1.0, 0.0, 1.0],
            [0, 0, 1, 1],
            [1.0, 0.0, 0.0, 1.0],
        );

        let capture = crate::log_capture::start();
        let frame = api
            .present(&target, crate::Health::Unhealthy)
            .expect("present");
        let records = capture.messages();
        drop(capture);
        assert_eq!(records, Vec::new(), "no export was attempted");

        match frame {
            ExportedFrame::Memory(m) => {
                assert_eq!((m.width, m.height), (2, 1));
                assert_eq!(m.pixels, [255, 0, 0, 255, 0, 255, 0, 255]);
            }
            #[cfg(unix)]
            ExportedFrame::Dmabuf(_) => panic!("an unhealthy presentation exported a DMA-BUF"),
        }
        target.deinit();
        api.thread_exit();
        api.deinit();
    }

    /// `present` with a HEALTHY presentation exports a DMA-BUF when the display
    /// can (`RUSTTY_REQUIRE_DMABUF` pivot), and otherwise warns and falls back to
    /// the CPU readback (`OpenGL.zig:300-311`).
    #[cfg(unix)]
    #[test]
    fn present_exports_a_dmabuf_when_presentation_is_healthy() {
        const TEST: &str = "present_exports_a_dmabuf_when_presentation_is_healthy";
        let _guard = test_support::gl_lock();
        let Some(api) = test_support::offscreen_api(TEST, AlphaBlending::LinearCorrected) else {
            return;
        };
        let target = api.init_target(8, 4).expect("init_target");
        let capture = crate::log_capture::start();
        let frame = api
            .present(&target, crate::Health::Healthy)
            .expect("present");
        let records = capture.messages();
        drop(capture);

        if api.device().has_dmabuf_export() {
            match frame {
                ExportedFrame::Dmabuf(d) => assert_eq!((d.width, d.height), (8, 4)),
                ExportedFrame::Memory(_) => panic!("a healthy presentation fell back to memory"),
            }
            assert_eq!(records, Vec::new());
        } else {
            test_support::require_dmabuf_or_skip(
                TEST,
                &"the surfaceless EGL display has no EGL_MESA_image_dma_buf_export",
            );
            assert!(matches!(frame, ExportedFrame::Memory(_)));
            assert_eq!(
                records,
                vec![(
                    log::Level::Warn,
                    LOG_SCOPE.to_string(),
                    "failed to export DMABUF, falling back to CPU presentation".to_string(),
                )]
            );
        }
        target.deinit();
        api.thread_exit();
        api.deinit();
    }

    /// M5 / UD354 fail-first test. With `alpha-blending = native` the shaders
    /// write sRGB-ENCODED values (they `unlinearize` before output), so the render
    /// target must store them verbatim; with a linear mode they write LINEAR
    /// values and the target must encode them. A clear to 0.2 therefore has to
    /// read back as `0.2 * 255 = 51` under `native` and as `srgb(0.2) = 124` under
    /// `linear-corrected`. The reference's always-`SRGB8_ALPHA8` target encodes
    /// both, so `native` reads back 124: every color is encoded twice.
    #[test]
    fn native_blending_target_stores_shader_output_verbatim() {
        let _guard = test_support::gl_lock();
        for (blending, expected) in [
            (AlphaBlending::Native, 51u8),
            (AlphaBlending::Linear, 124),
            (AlphaBlending::LinearCorrected, 124),
        ] {
            let Some(api) = test_support::offscreen_api(
                "native_blending_target_stores_shader_output_verbatim",
                blending,
            ) else {
                return;
            };
            let target = api.init_target(2, 2).expect("init_target");
            crate::opengl::target::tests::clear_pattern(
                &target,
                [0.2, 0.2, 0.2, 1.0],
                [0, 0, 0, 0],
                [0.2, 0.2, 0.2, 1.0],
            );
            let pixels = target.read_pixels_alloc().expect("read_pixels_alloc");
            assert_eq!(
                &pixels[..4],
                &[expected, expected, expected, 255],
                "{blending:?}: a 0.2 clear read back as {:?}",
                &pixels[..4]
            );
            target.deinit();
            api.thread_exit();
            api.deinit();
        }
    }

    /// B9: the GL function table belongs to the thread that entered. The api is
    /// built and entered on the test thread; `thread_exit` releases the context
    /// and drops that thread's table; a second thread's `thread_enter` makes the
    /// context current THERE and loads ITS table, and renders through it;
    /// `thread_exit` drops it again. This is ghostty's `threadEnter`/`threadExit`
    /// pair (`OpenGL.zig:222-235`) around a render thread.
    #[test]
    fn thread_enter_and_exit_move_the_gl_table_to_the_render_thread() {
        let _guard = test_support::gl_lock();
        let Some(api) = test_support::offscreen_api(
            "thread_enter_and_exit_move_the_gl_table_to_the_render_thread",
            AlphaBlending::LinearCorrected,
        ) else {
            return;
        };
        assert!(
            glad::is_loaded(),
            "init_offscreen enters the creating thread"
        );

        api.thread_exit();
        assert!(!glad::is_loaded(), "thread_exit drops this thread's table");

        /// The renderer crosses to its thread by value in the app (behind
        /// `rustty-core`'s own `Send` wrapper); this is the test's.
        struct SendApi(OpenGL);
        // SAFETY: the EGL handles are process-wide; the context is current on no
        // thread between `thread_exit` above and `thread_enter` below.
        unsafe impl Send for SendApi {}

        let moved = SendApi(api);
        let back = std::thread::spawn(move || {
            let moved = moved;
            assert!(!glad::is_loaded(), "a fresh thread starts without a table");
            moved.0.thread_enter().expect("thread_enter");
            assert!(glad::is_loaded(), "thread_enter loads this thread's table");

            let target = moved
                .0
                .init_target(2, 1)
                .expect("init_target on the render thread");
            crate::opengl::target::tests::clear_pattern(
                &target,
                [0.0, 1.0, 0.0, 1.0],
                [0, 0, 1, 1],
                [1.0, 0.0, 0.0, 1.0],
            );
            let pixels = target.read_pixels_alloc().expect("read_pixels_alloc");
            assert_eq!(pixels, [255, 0, 0, 255, 0, 255, 0, 255]);
            target.deinit();

            moved.0.thread_exit();
            assert!(
                !glad::is_loaded(),
                "thread_exit drops the render thread's table"
            );
            moved
        })
        .join()
        .expect("render thread");

        // `deinit` on the creating thread, as the reference's `OpenGL.deinit`
        // runs on the main thread.
        back.0.deinit();
    }
}
