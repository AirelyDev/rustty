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
//! PORT NOTE (`sync` is ignored — GL always blocks, `Frame.zig:50-57`): unlike
//! Metal's command-buffer completion block, OpenGL's `complete` ALWAYS blocks via
//! `gl.finish()` regardless of `sync`. So there is no async/block path here at all
//! (the whole HARD-SPOT-2 `RcBlock` machinery `metal/frame.rs` needs is absent): the
//! health is computed from `gl.errors.getError()` right after the blocking finish,
//! and the completion fn is invoked directly. Ported verbatim.

use crate::Health;

use super::target::Target;

// const log = std.log.scoped(.opengl);
// PORT NOTE (updated task-14c-1): `Frame.zig`'s
// `log.err("Failed to present render target: err={}", …)` call site lives in the
// completion body (`generic/linux_impl.rs`), preserved as a comment there. It is
// NOT RESTORED YET — the OpenGL 14c stage owns it; the `log` facade itself IS wired
// for this crate now. Scheduled work, not a missing dependency.

/// The present + report tail of `Frame.complete` (`Frame.zig:62-70`), type-erased
/// so `Frame` compiles before `GenericRenderer(OpenGL)` exists. The renderer task
/// supplies a concrete fn that casts `renderer` back to `*mut GenericRenderer` and
/// runs, on a healthy frame, `renderer.api.present(*target)` (logging on failure),
/// then `renderer.frameCompleted(health)` unconditionally. Raw `*mut` per the
/// module note; `Health` is already computed by [`Frame::complete`] and handed in.
///
/// The reference body this stands in for:
/// ```zig
/// if (health == .healthy) {
///     self.renderer.api.present(self.target.*) catch |err| {
///         log.err("Failed to present render target: err={}", .{err});
///     };
/// }
/// self.renderer.frameCompleted(health);
/// ```
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
    /// PORTS: `Frame.zig:23-38`.
    ///
    /// ```zig
    /// pub fn begin(opts: Options, renderer: *Renderer, target: *Target) !Self {
    ///     _ = opts;
    ///     return .{ .renderer = renderer, .target = target };
    /// }
    /// ```
    ///
    /// `renderer` is the type-erased `*mut GenericRenderer` (renderer task); once
    /// the frame completes, its `frameCompleted` is called with the frame's health,
    /// and `target` is presented via the renderer's API. `on_completed` is the
    /// concrete tail the renderer task supplies. Raw `*mut` per the module note.
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

    /// PORTS: `Frame.zig:50-70`.
    ///
    /// ```zig
    /// pub fn complete(self: *const Self, sync: bool) void {
    ///     _ = sync;
    ///     gl.finish();
    ///     const health: Health = if (gl.errors.getError()) .healthy else |_| .unhealthy;
    ///     if (health == .healthy) { self.renderer.api.present(self.target.*) catch … }
    ///     self.renderer.frameCompleted(health);
    /// }
    /// ```
    ///
    /// If `sync` is true, this will block until the frame is presented — NOTE (the
    /// reference's own, `Frame.zig:54`): for OpenGL, `sync` is ignored and we always
    /// block. The present + `frameCompleted` tail is [`on_completed`](Self::on_completed).
    pub fn complete(&self, _sync: bool) {
        super::gl::finish();

        // If there are any GL errors, consider the frame unhealthy.
        // `if (gl.errors.getError()) .healthy else |_| .unhealthy` — Ok => healthy.
        let health = if super::gl::errors::get_error().is_ok() {
            Health::Healthy
        } else {
            Health::Unhealthy
        };

        // The present-if-healthy + `frameCompleted(health)` tail (the renderer task
        // authors the body — see [`CompletionFn`]).
        //
        // SAFETY: the renderer pointer is live for the duration of `drawFrame`;
        // single-threaded on this path (we just blocked on `gl.finish()`); the
        // pointers are raw `*mut`, never aliased as `&mut` (the module note).
        unsafe {
            (self.on_completed)(self.renderer, self.target, health);
        }
    }
}
