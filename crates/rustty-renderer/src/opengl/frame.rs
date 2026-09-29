//! Wrapper for handling render passes.
//!
//! PORTS: ghostty `src/renderer/opengl/Frame.zig` (71 loc). (The `//! Wrapper for
//! handling render passes.` module doc is a copy-paste from `RenderPass.zig` in the
//! reference; preserved verbatim per R1 — the same copy-paste `metal/frame.rs`
//! carries.) This is the OpenGL twin of `metal/frame.rs`.
//!
//! PORT NOTE (type-erased renderer, deferred to the renderer task): `Frame.zig`
//! stores `renderer: *Renderer` (`= Renderer(OpenGL)`) and `complete` calls
//! `renderer.api.present(target.*)` + `renderer.frameCompleted(health)`. Neither
//! `GenericRenderer(OpenGL)` nor its `api.present`/`frameCompleted` exist until the
//! monomorphic-renderer task, so — exactly as `metal/frame.rs` does for the same
//! reason — `renderer` is type-erased to `*mut ()` and the present + report tail is
//! a plain-Rust [`CompletionFn`] fn-pointer the renderer task installs. The pointers
//! are raw `*mut` (NEVER `&mut`): the completion trampoline re-enters the renderer
//! while it is live inside `drawFrame` (`rust-mut-args-strongly-protected`), so a
//! `&mut` here would be UB.
//!
//! PORT NOTE (`sync` is ignored, `Frame.zig:51-56` at `12752b2ac`): "For OpenGL,
//! `sync` is ignored and we never block, instead pushing the newly presented and
//! exported frame to the frame queue." So there is no async/block path here at
//! all (the whole HARD-SPOT-2 `RcBlock` machinery `metal/frame.rs` needs is
//! absent): the health is computed from `gl.errors.getError()` and the completion
//! tail is invoked directly.

use crate::Health;

use super::target::Target;

// const log = std.log.scoped(.opengl);
// PORT NOTE (updated task-14c-1): `Frame.zig`'s
// `log.err("Failed to present render target: err={}", …)` call site lives in the
// completion body (`generic/linux_impl.rs`), preserved as a comment there. It is
// RESTORED (task-14c-3) at that site, targeting `opengl_impl::LOG_SCOPE` (`"opengl"`,
// Frame.zig:15's scope) rather than the `generic_renderer` scope of the file it ended
// up in — the scope belongs to the file whose call is being ported.

/// The present + push + report tail of `Frame.complete` (`Frame.zig:65-89` at
/// `12752b2ac`), type-erased so `Frame` compiles without naming
/// `GenericRenderer(OpenGL)`. The renderer supplies a concrete fn that casts
/// `renderer` back to `*mut GenericRenderer` and runs the reference's tail from
/// the present on, `health` being computed by [`Frame::complete`] and handed in.
/// Raw `*mut` per the module note.
///
/// The reference body this stands in for:
/// ```zig
/// // If the frame is healthy, draw it to an ExportedFrame
/// // Then sync before exporting and pushing to the present queue.
/// // The apprt pulls from this queue in its snapshot handler.
/// const presented_frame: ?OpenGL.ExportedFrame = if (health == .healthy) frame: {
///     const frame = self.renderer.api.present(
///         self.target.*,
///         self.renderer.presentationHealth(),
///     ) catch |err| {
///         log.warn("failed to present render target: err={}", .{err});
///         break :frame null;
///     };
///     break :frame frame;
/// } else null;
///
/// // Sync after frame draw AND frame present GL calls.
/// gl.finish();
///
/// // At this point the ExportedFrame is finished and can be shared
/// if (presented_frame) |frame| {
///     self.renderer.pushFrame(frame);
///
///     // Notify the surface that it should redraw
///     _ = self.renderer.surface_mailbox.push(.redraw, .{ .forever = {} });
/// }
///
/// // Report the health to the renderer.
/// self.renderer.frameCompleted(health);
/// ```
///
/// PORT NOTE: the `gl.finish()` therefore runs inside the tail, between the
/// present and the push, where the reference puts it.
pub type CompletionFn = unsafe fn(renderer: *mut (), target: *mut Target, health: Health);

/// Options for beginning a frame.
//
// PORTS: `Frame.zig:17-18` — `pub const Options = struct {};` (empty).
pub struct Options;

/// PORTS: `Frame.zig:20-21` — the top-level `renderer`/`target` fields ARE the
/// struct, plus the type-erased `on_completed` fn-pointer (the module note).
pub struct Frame {
    /// Type-erased `*Renderer` (the renderer task casts it back). Raw `*mut`.
    pub renderer: *mut (),
    /// The target presented via the renderer's API on completion. Raw `*mut`.
    pub target: *mut Target,
    /// The present + report tail (installed by the renderer task; `Frame.zig:62-70`).
    pub on_completed: CompletionFn,
}

impl Frame {
    /// Begin encoding a frame.
    ///
    /// PORTS: `Frame.zig:23-38`.
    ///
    /// ```zig
    /// pub fn begin(opts: Options, renderer: *Renderer, target: *Target) !Self {
    ///     _ = opts;
    ///     return .{ .renderer = renderer, .target = target };
    /// }
    /// ```
    ///
    /// `renderer`: Once the frame has been completed, the `frameCompleted` method
    /// on the renderer is called with the health status of the frame.
    ///
    /// `target`: The target is presented via the provided renderer's API when completed.
    ///
    /// PORT NOTE: `renderer` is the type-erased `*mut GenericRenderer` (renderer
    /// task); `on_completed` is the concrete tail the renderer task supplies. Raw
    /// `*mut` per the module note.
    ///
    /// PORT NOTE (`!Self`): the reference's error set is EMPTY (`begin` has no
    /// fallible call), so unlike `metal/frame.rs::begin` — which can fail creating a
    /// command buffer — this returns `Self` directly, not a `Result`.
    pub fn begin(
        _opts: Options,
        renderer: *mut (),
        target: *mut Target,
        on_completed: CompletionFn,
    ) -> Self {
        Self {
            renderer,
            target,
            on_completed,
        }
    }

    /// Add a render pass to this frame with the provided attachments.
    /// Returns a RenderPass which allows render steps to be added.
    ///
    /// PORTS: `Frame.zig:40-48`.
    ///
    /// ```zig
    /// pub inline fn renderPass(self: *const Self, attachments: []const RenderPass.Options.Attachment) RenderPass {
    ///     _ = self;
    ///     return RenderPass.begin(.{ .attachments = attachments });
    /// }
    /// ```
    pub fn render_pass<'a>(
        &self,
        attachments: &'a [super::render_pass::Attachment<'a>],
    ) -> super::render_pass::RenderPass<'a> {
        super::render_pass::RenderPass::begin(super::render_pass::Options { attachments })
    }

    /// PORTS: `Frame.zig:51-90` (`2f0b65346`).
    ///
    /// ```zig
    /// pub fn complete(self: *const Self, sync: bool) void {
    ///     _ = sync;
    ///
    ///     // If there are any GL errors, consider the frame unhealthy.
    ///     const health: Health = if (gl.errors.getError()) .healthy else |_| .unhealthy;
    ///     ... // the tail: see `CompletionFn`
    /// }
    /// ```
    ///
    /// Complete this frame and present the target.
    ///
    /// If `sync` is true, this will block until the frame is presented.
    ///
    /// NOTE: For OpenGL, `sync` is ignored and we never block, instead
    /// pushing the newly presented and exported frame to the frame queue.
    ///
    /// The present + finish + push + `frameCompleted` tail is
    /// [`on_completed`](Self::on_completed).
    pub fn complete(&self, _sync: bool) {
        // If there are any GL errors, consider the frame unhealthy.
        // `if (gl.errors.getError()) .healthy else |_| .unhealthy` — Ok => healthy.
        let health = if super::gl::errors::get_error().is_ok() {
            Health::Healthy
        } else {
            Health::Unhealthy
        };

        // The present / `gl.finish()` / push / `frameCompleted` tail (the renderer
        // authors the body — see [`CompletionFn`]).
        //
        // SAFETY: the renderer pointer is live for the duration of `drawFrame`,
        // which is running on this (the render) thread; the pointers are raw
        // `*mut`, never aliased as `&mut` (the module note).
        unsafe {
            (self.on_completed)(self.renderer, self.target, health);
        }
    }
}
