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
//! `self: OpenGL` (an `inline` copy — OpenGL holds no GPU handles, only an
//! allocator + blending + last_target). In Rust every read-only method takes
//! `&self`. `*const OpenGL` → `&self`, `*OpenGL` → `&mut self` (matching Metal's
//! precedent).
//!
//! DEFERRED (windowing-host seams, offscreen scope):
//! - the `alloc: std.mem.Allocator` field (OpenGL.zig:40) is DROPPED — the Rust
//!   port does not thread an allocator (the leaves are `Vec`-managed), exactly as
//!   `metal_impl` dropped it.
//! - `surfaceInit`/`finalizeSurfaceInit`/`threadEnter`/`threadExit`/
//!   `displayRealized` (OpenGL.zig:161-250) — the GTK-runtime `prepareContext(null)`
//!   arm — need the `epoxy` loader decision L5 owns; NOT ported this tier. The
//!   offscreen entry point is `init_offscreen`, which stands up its own headless
//!   `OffscreenContext` (Task 1) and runs `prepare_context` over it.
//! - `present`/`present_last_target` ("GL-ATTACH" deferred item): the real
//!   `glBlitFramebuffer`-to-default-framebuffer body IS ported (below) but never
//!   exercised by the offscreen milestone — the golden reads `Target::read_rgba()`
//!   directly. Parallel to the Metal "W-ATTACH" entry (resolved for Metal, freshly
//!   opened for OpenGL).

use std::ffi::CStr;
use std::fmt;

use rustty_render_types::AlphaBlending; // ← Part A hoist (no config edge)

use super::gl::buffer as glbuf;
use super::gl::sys::{self, OffscreenContext};
use super::gl::texture as gltex;
use super::gl::{draw, errors, framebuffer};

use super::buffer;
use super::frame::{CompletionFn, Frame};
use super::sampler;
use super::shaders::{self, Shaders};
use super::target::{self, Target};
use super::texture::{self, Texture};

/// Names the anonymous `struct { width: u32, height: u32 }` that `surfaceSize`
/// returns (OpenGL.zig:279). Mirrors `metal_impl::SurfaceSize`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SurfaceSize {
    pub width: u32,
    pub height: u32,
}

/// OpenGL-local error.
///
/// PORT NOTE: `OpenGL.zig`'s own `init` is infallible (`error{}!OpenGL`,
/// OpenGL.zig:51 — the fallible signature exists only "for parity with Metal").
/// The RUSTTY-ONLY offscreen constructor `init_offscreen`, however, stands up a
/// real headless `OffscreenContext`, which CAN fail (no EGL, no 4.3-core driver);
/// that failure folds to [`Error::Context`]. The `present` blit's `gl` calls fold
/// to [`Error::Gl`]. Both mirror how `metal_impl::Error` gathers the leaf failures
/// its own methods can surface.
#[derive(Debug)]
pub enum Error {
    /// The headless GL context could not be created, or did not reach the
    /// [`sys::MIN_VERSION_MAJOR`].[`sys::MIN_VERSION_MINOR`] core floor. Carries
    /// the underlying [`sys::Error`] so a caller (or a test) can distinguish a
    /// clean skip (absent EGL / absent driver / version under the floor) from a
    /// real port break (a missing entry point on a context EGL promised was >=
    /// 4.3).
    Context(sys::Error),
    /// A raw `gl` call in `present` reported an error (OpenGL.zig:298-336's
    /// `disable`/`enable`/`bind` fallible calls).
    Gl(errors::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Context(err) => write!(f, "OpenGL context setup failed: {err}"),
            Self::Gl(err) => write!(f, "OpenGL error: {err}"),
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

/// Graphics API wrapper for OpenGL. Ports `OpenGL.zig`'s `OpenGL = @This()` struct
/// (fields OpenGL.zig:40-46).
pub struct OpenGL {
    // OpenGL.zig:40 `alloc: std.mem.Allocator` — DROPPED (see the module DEFERRED
    // note): the Rust leaves are Vec-managed, no allocator threading.
    /// Alpha blending mode.
    // OpenGL.zig:43 `blending: configpkg.Config.AlphaBlending`. Now the Part-A
    // hoisted `rustty_render_types::AlphaBlending` (no config edge). Drives sRGB
    // internal-format selection in `init_target`/`texture_options`/`init_shaders`
    // via `blending.is_linear()`.
    pub blending: AlphaBlending,

    /// The most recently presented target, in case we need to present it again.
    // OpenGL.zig:45-46 `last_target: ?Target = null`.
    pub last_target: Option<Target>,

    /// RUSTTY-ONLY: the owned headless EGL/GL context (Task 1). `Some` on the
    /// offscreen path (`init_offscreen`), where this type stands up + owns the
    /// context ghostty otherwise gets from GDK; `None` on the future windowed
    /// (L5) path, where GDK owns context lifetime. Kept alive here so the context
    /// stays current for the target/shader/frame calls, and terminated on drop.
    context: Option<OffscreenContext>,

    /// OFFSCREEN stub for `surface_size`: the fixed surface size (replaces the
    /// `GL_VIEWPORT` query in `surfaceSize`, OpenGL.zig:281-286). Set by
    /// `init_offscreen`; returned by `surface_size` while `context.is_some()`.
    /// Mirrors `metal_impl`'s `offscreen_size` seam.
    offscreen_size: SurfaceSize,
}

impl OpenGL {
    // OpenGL.zig:26 `pub const custom_shader_target: shadertoy.Target = .glsl;`
    pub const CUSTOM_SHADER_TARGET: crate::shadertoy::Target = crate::shadertoy::Target::Glsl;
    // OpenGL.zig:27-28 — the fragCoord for OpenGL shaders is +Y = up.
    // `pub const custom_shader_y_is_down = false;`
    pub const CUSTOM_SHADER_Y_IS_DOWN: bool = false;
    /// Because OpenGL's frame completion is always sync, we have no need for
    /// multi-buffering.
    // OpenGL.zig:30-32 `pub const swap_chain_count = 1;`
    pub const SWAP_CHAIN_COUNT: usize = 1;

    /// OFFSCREEN constructor (RUSTTY-ONLY, no ghostty analogue). Mirrors
    /// `metal_impl::Metal::init_offscreen`'s precedent: the reference `init`
    /// (OpenGL.zig:51-56) needs no windowing state itself, but the GL CONTEXT it
    /// runs against is GDK's — created out-of-band via `surfaceInit`/`prepareContext`
    /// (OpenGL.zig:162-170), which is unavailable headless. So this stands up Task
    /// 1's `OffscreenContext` (EGL surfaceless, core 4.3), runs `prepare_context`
    /// over it (the load + version-gate + debug-output + `GL_FRAMEBUFFER_SRGB`
    /// setup, OpenGL.zig:132-159, via [`sys::load_gl`]), and returns an `OpenGL`
    /// owning that context. Takes `blending` + the fixed offscreen size directly,
    /// so the milestone needs neither `renderer::Options` nor `apprt`.
    ///
    /// # Errors
    /// [`Error::Context`] if the headless context cannot be created or does not
    /// reach the required GL version (a clean-skip signal for the golden gate).
    pub fn init_offscreen(
        blending: AlphaBlending,
        width: u32,
        height: u32,
    ) -> Result<OpenGL, Error> {
        // Stand up the headless context and make it current (Task 1).
        let context = OffscreenContext::init()?;
        // Load the GL entry points into it + run the reference's version gate,
        // debug-output registration, and GL_FRAMEBUFFER_SRGB enable
        // (OpenGL.zig:132-159 `prepareContext`, reached via `load_gl`).
        sys::load_gl(&context)?;

        // RUSTTY-ONLY (offscreen viewport — no ghostty analogue): ghostty NEVER calls
        // `glViewport` in the renderer — the windowing host (GtkGLArea / GLFW) sets it
        // to the drawable size on resize, and `surfaceSize` merely READS it back
        // (OpenGL.zig:280-286). A surfaceless offscreen context has no host and starts
        // with a 0×0 (undefined) viewport, so primitives would rasterize into nothing
        // (`glClear` ignores the viewport — a clear "works" while no draw shows). We set
        // it once to `offscreen_size`, the offscreen stand-in for the host's resize
        // callback, keeping it consistent with what `surface_size` returns (the fixed
        // `offscreen_size`) and the target the generic renderer resizes to match. First
        // exercised by the Task-11 offscreen golden.
        draw::viewport(
            0,
            0,
            width as gl::types::GLsizei,
            height as gl::types::GLsizei,
        )?;

        Ok(OpenGL {
            blending,
            last_target: None,
            context: Some(context),
            offscreen_size: SurfaceSize { width, height },
        })
    }

    /// WINDOWED constructor (GL-ATTACH) — the FAITHFUL port of the reference
    /// `OpenGL.init` (`OpenGL.zig:51-56`), which creates NO GL context: ghostty's
    /// OpenGL graphics API never owns a context — GDK's GLArea owns it, and the
    /// GL loader is bound out-of-band by `surfaceInit`/`displayRealized`
    /// (`prepareContext(null)`, reached here via [`sys::prepare_context_windowed`]).
    /// So this merely stores `blending`; `context` is `None` (GDK owns lifetime),
    /// and `surface_size` reads the live `GL_VIEWPORT` GDK set (the `None` arm).
    ///
    /// Unlike [`Self::init_offscreen`] it does NOT stand up an `OffscreenContext`,
    /// does NOT load GL (the windowing host already did via
    /// `sys::prepare_context_windowed`), and does NOT set a viewport (GDK sets it
    /// on the GLArea `resize`/`render`). The caller MUST have made a GL context
    /// current and run `prepare_context_windowed` before any GL-touching method
    /// (`init_shaders`/`init_target`/`present_to_default`) is reached.
    pub fn init(blending: AlphaBlending) -> OpenGL {
        OpenGL {
            blending,
            last_target: None,
            // GDK owns the context; the renderer does NOT eglTerminate anything on
            // drop — this is the fix for the GL-ATTACH teardown double-free.
            context: None,
            // Unused on the windowed path (`surface_size` reads GL_VIEWPORT while
            // `context` is `None`); zeroed for parity with the offscreen stub.
            offscreen_size: SurfaceSize {
                width: 0,
                height: 0,
            },
        }
    }

    /// GL-ATTACH: whether this `OpenGL` OWNS its GL context (the offscreen golden
    /// path, [`Self::init_offscreen`]) or borrows the currently-current one (the
    /// windowed path, [`Self::init`], where GDK owns it). The renderer's frame
    /// completion uses this to decide whether to `present` to the default
    /// framebuffer (windowed only — offscreen has no complete FBO 0), and it is the
    /// same discriminant `surface_size` already switches on.
    pub fn owns_context(&self) -> bool {
        self.context.is_some()
    }

    /// PORTS: `OpenGL.zig:58-60`.
    ///
    /// ```zig
    /// pub fn deinit(self: *OpenGL) void {
    ///     self.* = undefined;
    /// }
    /// ```
    ///
    /// The reference merely invalidates `self` — the GL context is GDK's, not
    /// owned. In the RUSTTY offscreen path the `OffscreenContext` IS owned, so
    /// consuming `self` here drops it, which terminates EGL
    /// (`OffscreenContext::drop`). Consuming (`self`), mirroring the reference
    /// taking `*OpenGL` and leaving it `undefined`.
    pub fn deinit(self) {
        // `self` drops here: the owned `context` (offscreen path) terminates the
        // EGL display; `last_target`, if any, drops without releasing GL objects
        // (`Target` has a manual `deinit`, matching the reference's manual model —
        // the caller deinits targets explicitly, exactly as ghostty does).
    }

    /// Actions taken before doing anything in `drawFrame`.
    ///
    /// Right now there's nothing we need to do for OpenGL (GL has no per-frame
    /// autorelease-pool equivalent — contrast Metal's pool push).
    // pub fn drawFrameStart(self: *OpenGL) void { _ = self; }  (OpenGL.zig:255-257)
    pub fn draw_frame_start(&mut self) {}

    /// Actions taken after `drawFrame` is done.
    ///
    /// Right now there's nothing we need to do for OpenGL.
    // pub fn drawFrameEnd(self: *OpenGL) void { _ = self; }    (OpenGL.zig:262-264)
    pub fn draw_frame_end(&mut self) {}

    /// Initialize our shader set.
    // pub fn initShaders(self, alloc, custom_shaders: []const [:0]const u8) !shaders.Shaders
    //   (OpenGL.zig:266-276)
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

    /// Get the current size of the runtime surface.
    // pub fn surfaceSize(self: *const OpenGL) !struct { width: u32, height: u32 }
    //   (OpenGL.zig:278-287)
    //
    // OFFSCREEN stub: the reference reads `GL_VIEWPORT` (which reflects the live
    // windowing surface). Offscreen there is no viewport bound to a real surface,
    // so while a `context` is owned we return the fixed `offscreen_size` set at
    // construction. The real `GL_VIEWPORT` body is ported too (the `None` arm), for
    // when L5 wires a live viewport; the offscreen ctor path never reaches it.
    // Verbatim Zig body (OpenGL.zig:280-286):
    //     var viewport: [4]gl.c.GLint = undefined;
    //     gl.glad.context.GetIntegerv.?(gl.c.GL_VIEWPORT, &viewport);
    //     return .{ .width = @intCast(viewport[2]), .height = @intCast(viewport[3]) };
    pub fn surface_size(&self) -> Result<SurfaceSize, Error> {
        match &self.context {
            // Offscreen: no real viewport; return the construction-time size.
            Some(_) => Ok(self.offscreen_size),
            // Windowed (L5): query the live GL viewport (real body).
            None => {
                let mut viewport: [gl::types::GLint; 4] = [0; 4];
                // SAFETY: `GetIntegerv(GL_VIEWPORT, ...)` writes 4 GLints; the array
                // is exactly 4 wide. Core GL 1.0 — force-unwrapped by the reference
                // (`.?`); the `gl` crate panics on an unloaded slot, matching that
                // force-unwrap on a context that has none (unreachable here).
                unsafe { gl::GetIntegerv(gl::VIEWPORT, viewport.as_mut_ptr()) };
                Ok(SurfaceSize {
                    // @intCast → `as u32` (viewport width/height are non-negative).
                    width: viewport[2] as u32,
                    height: viewport[3] as u32,
                })
            }
        }
    }

    /// Initialize a new render target which can be presented by this API.
    // pub fn initTarget(self: *const OpenGL, width: usize, height: usize) !Target
    //   (OpenGL.zig:289-296)
    pub fn init_target(&self, width: usize, height: usize) -> Result<Target, target::Error> {
        Target::init(target::Options {
            // Using an sRGB internal format makes GL gamma-encode the pixels
            // written to it *after* blending, i.e. linear (correct) alpha blending
            // rather than gamma-incorrect blending.
            internal_format: if self.blending.is_linear() {
                gltex::InternalFormat::SRGBA
            } else {
                gltex::InternalFormat::RGBA
            },
            width,
            height,
        })
    }

    /// Present the provided target.
    // pub fn present(self: *OpenGL, target: Target) !void  (OpenGL.zig:298-331)
    //
    // GL-ATTACH deferred item: ported VERBATIM but the offscreen milestone never
    // calls it (the golden reads back via `Target::read_rgba`). It blits the
    // target's FBO into the default framebuffer — which only exists under a live
    // windowing host (L5). Consuming `target` by value (Zig `target: Target` is a
    // value copy; the Rust `Target` is move-only with a manual `deinit`, so the
    // faithful spelling takes ownership and stores it in `last_target`).
    //
    // # Errors
    // [`Error::Gl`] if the `disable`/`bind`/re-`enable` GL calls fail.
    pub fn present(&mut self, target: Target) -> Result<(), Error> {
        // In order to present a target we blit it to the default framebuffer.

        // We disable GL_FRAMEBUFFER_SRGB while doing this blit, otherwise the
        // values may be linearized as they're copied, but even though the draw
        // framebuffer has a linear internal format, the values in it should be
        // sRGB, not linear!
        draw::disable(gl::FRAMEBUFFER_SRGB)?;

        // The reference `defer`s re-enabling GL_FRAMEBUFFER_SRGB so it runs on
        // EVERY exit path (including the blit-bind error path). We reproduce the
        // `defer` by capturing the blit result, ALWAYS re-enabling, then
        // propagating (OpenGL.zig:307-309: the defer logs on its own failure and
        // swallows it, so we mirror that — a re-enable failure does not mask the
        // blit result).
        let blit = (|| -> Result<(), errors::Error> {
            // Bind the target for reading.
            let fbobind = target.framebuffer.bind(framebuffer::Target::READ)?;
            // Blit (OpenGL.zig:316-327).
            // SAFETY: a valid READ framebuffer is bound (`fbobind`); the src/dst
            // rects are the target's own extent; core GL 3.0. Widths/heights fit
            // GLint (target dims are render-sized).
            unsafe {
                gl::BlitFramebuffer(
                    0,
                    0,
                    target.width as gl::types::GLint,
                    target.height as gl::types::GLint,
                    0,
                    0,
                    target.width as gl::types::GLint,
                    target.height as gl::types::GLint,
                    gl::COLOR_BUFFER_BIT,
                    gl::NEAREST,
                );
            }
            fbobind.unbind();
            Ok(())
        })();

        // defer gl.enable(GL_FRAMEBUFFER_SRGB) catch |err| log.err(...);
        if draw::enable(gl::FRAMEBUFFER_SRGB).is_err() {
            // log.err("Error re-enabling GL_FRAMEBUFFER_SRGB, err={}", .{err});
            // (not restored yet — the OpenGL 14c stage; the facade itself IS wired as
            // of task-14c-1. Swallow, as the reference `defer` does after logging.)
        }

        blit?;

        // Keep track of this target in case we need to repeat it.
        self.last_target = Some(target);
        Ok(())
    }

    /// Present the last presented target again.
    // pub fn presentLastTarget(self: *OpenGL) !void {
    //     if (self.last_target) |target| try self.present(target);
    // }  (OpenGL.zig:333-336)
    //
    // GL-ATTACH deferred item (never called by the offscreen milestone). The Rust
    // `Target` is move-only, so we `take` the stored target and re-present it
    // (which stores it back). Behaviour-equivalent to the reference re-presenting
    // the value-copied `last_target`.
    pub fn present_last_target(&mut self) -> Result<(), Error> {
        if let Some(target) = self.last_target.take() {
            self.present(target)
        } else {
            Ok(())
        }
    }

    /// Called when the renderer released its GPU resources; the last
    /// presented target is deinited with them so we must drop our copy.
    //
    // pub fn gpuResourcesReleased(self: *OpenGL) void {
    //     self.last_target = null;
    // }
    // (`c4e16970a`. The Rust `Target` has no `Drop` — GL objects are freed by
    // the swap-chain teardown that precedes this call — so nulling the Option
    // drops only the handle copy, exactly like the Zig assignment.)
    pub fn gpu_resources_released(&mut self) {
        self.last_target = None;
    }

    /// GL-ATTACH — the WINDOWED present: blit `target`'s FBO into the CURRENTLY
    /// BOUND draw framebuffer, which under a live GTK GLArea IS the GLArea's
    /// framebuffer (GDK binds it — a non-zero FBO — before emitting `render`). This
    /// IS the reference `present` body (`OpenGL.zig:298-331`): it binds `target` as
    /// the READ framebuffer and `glBlitFramebuffer`s into the current DRAW binding,
    /// which the reference calls "the default framebuffer" (from the renderer's
    /// view — whatever the windowing host bound). It NEVER binds a draw framebuffer
    /// itself, so the pixels land wherever the host directed them — that is what
    /// makes the terminal VISIBLE on the window. It differs from [`Self::present`]
    /// only in taking `target` by SHARED REFERENCE rather than by value: on the
    /// windowed path the frame's `Target` is still owned by the swap chain (the
    /// renderer's completion trampoline holds a raw `*mut Target` into it), so there
    /// is nothing to move into `last_target` — the blit only READS the target's FBO.
    /// `Framebuffer` is a `Copy` GL handle, so binding it for read needs no ownership.
    ///
    /// # Errors
    /// [`Error::Gl`] if the `disable`/`bind`/re-`enable` GL calls fail (mirrors the
    /// reference `present`'s `try` sites).
    pub fn present_to_default(&self, target: &Target) -> Result<(), Error> {
        // We disable GL_FRAMEBUFFER_SRGB while doing this blit (see `present`).
        draw::disable(gl::FRAMEBUFFER_SRGB)?;

        // Mirror the reference `defer`: always re-enable GL_FRAMEBUFFER_SRGB, even
        // on the blit-bind error path, and never let a re-enable failure mask the
        // blit result (OpenGL.zig:307-309).
        let blit = (|| -> Result<(), errors::Error> {
            // Bind the target for reading; `bind(.read)` SAVES the current
            // GL_FRAMEBUFFER_BINDING (the host's draw framebuffer) and restores it on
            // `unbind`, so the blit's DRAW target stays the host framebuffer.
            let fbobind = target.framebuffer.bind(framebuffer::Target::READ)?;
            // SAFETY: a valid READ framebuffer is bound (`fbobind`); the default
            // (draw) framebuffer is FBO 0; core GL 3.0. Dims fit GLint.
            unsafe {
                gl::BlitFramebuffer(
                    0,
                    0,
                    target.width as gl::types::GLint,
                    target.height as gl::types::GLint,
                    0,
                    0,
                    target.width as gl::types::GLint,
                    target.height as gl::types::GLint,
                    gl::COLOR_BUFFER_BIT,
                    gl::NEAREST,
                );
            }
            fbobind.unbind();
            Ok(())
        })();

        if draw::enable(gl::FRAMEBUFFER_SRGB).is_err() {
            // (not restored yet — the OpenGL 14c stage; the facade itself IS wired as
            // of task-14c-1. Swallow, as the reference `defer` does.)
        }

        blit?;
        Ok(())
    }

    /// GL-ATTACH proof helper (RUSTTY-ONLY, no ghostty analogue): read the CURRENTLY
    /// BOUND draw framebuffer — the WINDOWING HOST's framebuffer — back into a tight
    /// `width*height*4` RGBA buffer. Under a GTK GLArea this is the GLArea's own
    /// (NON-zero) framebuffer, which GDK binds before emitting `render` (or which
    /// `gtk_gl_area_attach_buffers` binds), and which [`Self::present_to_default`]
    /// blits the frame into. The visible-render KAT calls this to assert the terminal
    /// frame landed in the WINDOW's framebuffer (NOT the renderer's offscreen target
    /// FBO). It reads the current `GL_FRAMEBUFFER_BINDING` (the draw alias), NOT the
    /// literal FBO 0, precisely because GtkGLArea's default framebuffer is a non-zero
    /// FBO. MUST be called with a GL context current and the host framebuffer bound.
    pub fn read_bound_framebuffer_rgba(&self, width: usize, height: usize) -> Vec<u8> {
        let mut out = vec![0u8; width * height * 4];
        let w = width as gl::types::GLsizei;
        let h = height as gl::types::GLsizei;

        // Read whatever framebuffer is currently bound as the draw target (the host's
        // GLArea framebuffer), NOT FBO 0 — GtkGLArea's default framebuffer is a
        // non-zero FBO.
        // SAFETY: `GL_FRAMEBUFFER_BINDING` is single-valued; the read is `w*h*4` bytes
        // at the default pack alignment of 4 (row stride `w*4` is a multiple of 4).
        unsafe {
            let mut bound: gl::types::GLint = 0;
            gl::GetIntegerv(gl::FRAMEBUFFER_BINDING, &mut bound);
            gl::BindFramebuffer(gl::READ_FRAMEBUFFER, bound as gl::types::GLuint);
            gl::ReadPixels(
                0,
                0,
                w,
                h,
                gl::RGBA,
                gl::UNSIGNED_BYTE,
                out.as_mut_ptr().cast::<std::ffi::c_void>(),
            );
        }
        out
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
            // TODO (ghostty): Generate mipmaps for image textures and use
            //   linear_mipmap_linear filtering so they look good scaled way down.
            min_filter: gltex::MinFilter::Linear,
            mag_filter: gltex::MagFilter::Linear,
            // TODO (ghostty): Separate out background image options, use repeating
            //   coordinate modes so we don't have to do the modulus in the shader.
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
// `metal_impl`'s `metal_offscreen_construct_and_begin_frame`: it drives the
// offscreen `OpenGL` (not the raw leaves) through `init_offscreen`
// (OffscreenContext + prepare_context) → field asserts → `init_shaders` (all five
// pipelines) → `init_target` (an offscreen renderbuffer FBO) → clean teardown. It
// asserts no panic + the expected field/surface-size values. Skips cleanly on
// hosts with no OpenGL 4.3 core context (the Task-1 OPEN RISK path), mirroring the
// Metal "no device → skip" precedent.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::opengl::gl::test_support;

    #[test]
    fn opengl_init_offscreen_then_deinit_round_trips() {
        // Hold the process-wide GL lock for the whole context lifetime: this test
        // stands up its own `OffscreenContext` (via `init_offscreen`) and writes
        // the `gl::load_with` globals, both of which the shared lock serializes
        // against every other GL test (`test_support::gl_lock` docs). `_guard` is
        // declared FIRST so it drops LAST — after `ogl.deinit()` terminates the
        // context — matching `GlTestContext`'s field-order contract.
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
            // an `EGL_BAD_MATCH` refusing the 4.3-core request (the default virgl
            // driver on this host, per the plan's EMPIRICAL HOST FINDING), or a
            // reported version under the floor. All are a CLEAN SKIP, mirroring
            // `test_support::offscreen_gl` (every context-creation error skips).
            // W8-03 strict mode: under RUSTTY_REQUIRE_GL=1 the clean skip
            // becomes a hard failure (see test_support docs).
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

        // Fields (OpenGL.zig:40-46 analogues).
        assert_eq!(ogl.blending, AlphaBlending::Native);
        assert!(ogl.last_target.is_none());
        // surface_size returns the offscreen construction size (the OFFSCREEN stub).
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

        // initTarget → an offscreen renderbuffer-backed FBO of the requested size.
        let target = ogl.init_target(10, 10).expect("init_target");
        assert_eq!(target.width, 10);
        assert_eq!(target.height, 10);

        // Clean teardown, in order: leaf GL objects first (while the context is
        // still current), then the context (via `deinit`, which drops it →
        // eglTerminate). `_guard` releases after.
        target.deinit();
        shaders.deinit();
        ogl.deinit();
    }
}
