//! Wrapper for handling render passes.
//!
//! PORTS: ghostty `src/renderer/metal/Frame.zig`. (The `//! Wrapper for handling
//! render passes.` module doc is a copy-paste from `RenderPass.zig` in the
//! reference; preserved verbatim per R1.)
//!
//! PORT NOTE (R3/objc2, HARD SPOT 2 [F6]): ghostty stashes an `objc.Block`
//! (`CompletionBlock`) over a `Context { renderer: *Renderer, target: *Target,
//! sync: bool }` and, on completion, derefs the stashed `*Renderer` to call
//! `renderer.api.present(target, sync)` + `renderer.frameCompleted(health)`. The
//! port's completion body is a plain-Rust `CompletionFn` fn-pointer that
//! `drawFrame` installs (the faithful analogue of Zig's type-erased
//! `CompletionBlock`; R1 "no-loss, not no-addition"), and its context carries
//! owned clones of what that body needs instead of the renderer pointer (N440, see
//! `CompletionContext`): the async body runs on a Metal-owned thread without the
//! draw mutex, so it must not touch the renderer's bytes at all.
//!
//! The **sync path (the golden path) needs NO ObjC block**: `complete(true)` does
//! commit → waitUntilCompleted → set `sync=true` → call the fn-pointer directly
//! (Zig's `CompletionBlock.invoke`). The **async path**
//! (`addCompletedHandler:` via a `block2::RcBlock`) installs the completion block
//! over its own clone of the `CompletionContext` (W1-4).

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{MTLCommandBuffer, MTLCommandQueue};

use std::sync::Arc;

use objc2_core_foundation::CFRetained;
#[cfg(test)]
use objc2_core_foundation::Type;
use objc2_io_surface::IOSurfaceRef;
use rustty_zigstd::thread::Semaphore;

use crate::generic::{AtomicHealth, HealthReport, SurfaceMailbox};
use crate::metal::metal_impl::LayerCell;
use crate::metal::render_pass::{Attachment, Options as RpOptions, RenderPass};

// const log = std.log.scoped(.metal);
// PORT NOTE (task-14c-2): `Frame.zig`'s ONE diagnostic — `log.err("Failed to
// present render target: err={}", …)` (`Frame.zig:86`) — is NOT emitted from this
// file, because the port does not have `bufferCompleted` here: its body is the
// generic renderer's completion trampoline `generic.rs::metal_buffer_completed`
// (this module stores only the `CompletionFn`). The live emission is therefore in
// `generic.rs`, and it carries THIS file's scope via the constant below rather than
// `generic.rs`'s own `generic_renderer` — the reference tags it `.metal`.
/// `Frame.zig:16`'s `const log = std.log.scoped(.metal);`.
pub(crate) const LOG_SCOPE: &str = "metal";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Error {
    /// A Metal API call failed.
    MetalFailed,
}

/// The completion context ghostty stashes as `CompletionBlock.Context`.
//
// PORT NOTE (N440, P7-W7 T5a): the reference's context is `{ renderer: *Renderer,
// target: *Target, sync: bool }`, and `bufferCompleted` reaches `renderer.api`,
// `renderer.health`, `renderer.surface_mailbox` and `renderer.swap_chain` through
// it. On the async path that runs on a Metal-owned thread WITHOUT the draw mutex,
// while the render thread may hold a whole-renderer `&mut` under it: an aliasing
// violation in Rust (a data race with that retag). The port's context therefore
// carries its own clones of exactly what the handler needs, each in an allocation
// of its own, taken under the draw mutex when the frame begins, so the handler
// forms no reference to any renderer byte (`.port/phase-2/mirihar-n440`):
// - `health`: the renderer's `health` cell (`Arc<AtomicHealth>`);
// - `frame_sema`: the swap chain's frame semaphore (`releaseFrame`);
// - `surface_mailbox`: a clone of the renderer's surface mailbox handle;
// - `health_report`: the renderer's health-report state (UD362);
// - `layer`: Metal's presenter cell (`api.present` reads only the layer);
// - `surface`: the target's IOSurface (`present` reads only `target.surface`; the
//   frame slot is held by `frame_sema` until this completion, so it is the surface
//   the reference copies out of `block.target.*`).
// `Clone` (not `Copy`): the async block owns its own clone (ghostty's
// `addCompletedHandler:` copies the block's context), so it never borrows
// `self.block`, which would dangle after `complete` returns.
#[derive(Clone)]
pub struct CompletionContext {
    pub health: Arc<AtomicHealth>,
    pub frame_sema: Arc<Semaphore>,
    pub surface_mailbox: SurfaceMailbox,
    pub health_report: Arc<HealthReport>,
    pub layer: Arc<LayerCell>,
    pub surface: CFRetained<IOSurfaceRef>,
    pub sync: bool,
}

/// The completion body (= Zig's `&bufferCompleted`): the generic renderer's
/// `metal_buffer_completed`, which presents `ctx.surface` on a healthy frame and
/// runs `frameCompleted(health)` over the context. A safe fn: the context holds
/// no raw pointer (N440).
pub type CompletionFn = fn(ctx: &CompletionContext, buffer: &ProtocolObject<dyn MTLCommandBuffer>);

#[cfg(test)]
impl CompletionContext {
    /// A context tied to no renderer (a fresh health cell and semaphore, a
    /// detached mailbox, no layer), for the frame tests that complete through a
    /// no-op `CompletionFn`.
    pub(crate) fn detached(surface: &IOSurfaceRef) -> CompletionContext {
        CompletionContext {
            health: Arc::new(AtomicHealth::new(crate::Health::Healthy)),
            frame_sema: Arc::new(Semaphore::new()),
            surface_mailbox: SurfaceMailbox::DETACHED,
            health_report: Arc::new(HealthReport::new()),
            layer: Arc::new(LayerCell::offscreen()),
            surface: surface.retain(),
            sync: false,
        }
    }
}

/// The `MTLCommandBufferHandler` closure shape `addCompletedHandler:` expects
/// (`block2::RcBlock<CompletionHandler>` ⇒ `*const DynBlock<dyn Fn(NonNull<…>)>`).
/// A named alias keeps the async-present block type readable (clippy
/// `type_complexity`).
type CompletionHandler = dyn Fn(core::ptr::NonNull<ProtocolObject<dyn MTLCommandBuffer>>);

/// Options for beginning a frame.
pub struct Options<'a> {
    /// MTLCommandQueue
    pub queue: &'a ProtocolObject<dyn MTLCommandQueue>,
}

pub struct Frame {
    /// MTLCommandBuffer
    pub buffer: Retained<ProtocolObject<dyn MTLCommandBuffer>>,

    /// The stashed completion context (Frame.zig `block: CompletionBlock.Context`).
    pub block: CompletionContext,

    /// The completion body (installed by the caller; = Zig's `&bufferCompleted`).
    pub on_completed: CompletionFn,
}

impl Frame {
    /// Begin encoding a frame.
    ///
    /// Once the frame has been completed, `on_completed` (the generic renderer's
    /// `frameCompleted` path) is called with the health status of the frame. The
    /// target (`completion.surface`) is presented through the context's layer when
    /// completed.
    //
    // PORT NOTE (N440): the reference takes `renderer: *Renderer, target: *Target`;
    // the port takes the context of clones the caller built under the draw mutex
    // (see `CompletionContext`).
    pub fn begin(
        opts: Options<'_>,
        completion: CompletionContext,
        on_completed: CompletionFn,
    ) -> Result<Self, Error> {
        let buffer = opts.queue.commandBuffer().ok_or(Error::MetalFailed)?;

        // Create our block to register for completion updates.
        // The block is deallocated by the objC runtime on success.
        //
        // Frame.zig stashes `CompletionBlock.init(.{ renderer, target, sync = false
        // }, &bufferCompleted)`; here the context (with `sync = false`) plus the
        // `on_completed` fn-pointer (the type-erased analogue).
        Ok(Self {
            buffer,
            block: CompletionContext {
                sync: false,
                ..completion
            },
            on_completed,
        })
    }

    // /// This is the block type used for the addCompletedHandler callback.
    // const CompletionBlock = objc.Block(struct {
    //     renderer: *Renderer,
    //     target: *Target,
    //     sync: bool,
    // }, .{
    //     objc.c.id, // MTLCommandBuffer
    // }, void);
    //
    // PORT NOTE (HARD SPOT 2): the completion body reproduces `Frame.zig:67
    // bufferCompleted`; it lives in the generic renderer (`metal_buffer_completed`),
    // and this module only stores + calls `on_completed: CompletionFn` (see
    // `complete`).
    //
    //     fn bufferCompleted(
    //         block: *const CompletionBlock.Context,
    //         buffer_id: objc.c.id,
    //     ) callconv(.c) void {
    //         const buffer = objc.Object.fromId(buffer_id);
    //
    //         // Get our command buffer status to pass back to the generic renderer.
    //         const status = buffer.getProperty(mtl.MTLCommandBufferStatus, "status");
    //         const health: Health = switch (status) {
    //             .@"error" => .unhealthy,
    //             else => .healthy,
    //         };
    //
    //         // If the frame is healthy, present it.
    //         if (health == .healthy) {
    //             block.renderer.api.present(
    //                 block.target.*,
    //                 block.sync,
    //             ) catch |err| {
    //                 log.err("Failed to present render target: err={}", .{err});
    //             };
    //         }
    //
    //         block.renderer.frameCompleted(health);
    //     }
    //
    // The Rust shape is `generic.rs` `metal_buffer_completed`, over the
    // `CompletionContext` clones (N440): present `ctx.surface` through `ctx.layer`,
    // then `frameCompleted` over `ctx.health`, `ctx.frame_sema` and
    // `ctx.surface_mailbox`.

    /// Add a render pass to this frame with the provided attachments.
    /// Returns a RenderPass which allows render steps to be added.
    pub fn render_pass(&self, attachments: &[Attachment<'_>]) -> RenderPass {
        RenderPass::begin(RpOptions {
            attachments,
            command_buffer: &self.buffer,
        })
    }

    /// Complete this frame and present the target.
    ///
    /// If `sync` is true, this will block until the frame is presented.
    pub fn complete(&mut self, sync: bool) {
        // If we don't need to complete synchronously,
        // we add our block as a completion handler.
        //
        // It will be copied when we add the handler, and then the
        // copy will be deallocated by the objc runtime on success.
        if !sync {
            // The async present half installs a completion block over the exact
            // `MTLCommandBufferHandler` closure type (`*mut block2::DynBlock<dyn
            // Fn(NonNull<ProtocolObject<dyn MTLCommandBuffer>>)>`).
            //
            // Clone the context + copy the fn-ptr into the block (matches Zig's
            // by-value Context copy — stays valid until Metal fires the handler). The
            // block is copied when `addCompletedHandler:` retains it and dealloc'd by
            // the objc runtime on completion (Frame.zig:106-108), dropping the clones.
            let ctx = self.block.clone();
            let on_completed = self.on_completed;
            let handler: block2::RcBlock<CompletionHandler> = block2::RcBlock::new(
                move |buf: core::ptr::NonNull<ProtocolObject<dyn MTLCommandBuffer>>| {
                    // SAFETY: buf is a valid MTLCommandBuffer for the callback's
                    // duration. The context holds only owned clones (N440).
                    on_completed(&ctx, unsafe { buf.as_ref() });
                },
            );
            // SAFETY: `handler` matches `MTLCommandBufferHandler`; `addCompletedHandler:`
            // retains the block (copied on install) and the objc runtime dealloc's it
            // once fired.
            unsafe {
                self.buffer
                    .addCompletedHandler(block2::RcBlock::as_ptr(&handler));
            }
        }

        self.buffer.commit();

        // If we need to complete synchronously, we wait until
        // the buffer is completed and invoke the block directly.
        if sync {
            self.buffer.waitUntilCompleted();
            self.block.sync = true;
            // Zig `CompletionBlock.invoke(&self.block, .{self.buffer.value})` — a
            // direct call. Here the Rust fn-pointer IS the block body; no ObjC
            // roundtrip is needed on the sync path.
            (self.on_completed)(&self.block, &self.buffer);
        }
    }
}
