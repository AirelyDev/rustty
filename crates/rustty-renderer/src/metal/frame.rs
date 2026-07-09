//! Wrapper for handling render passes.
//!
//! PORTS: ghostty `src/renderer/metal/Frame.zig`. (The `//! Wrapper for handling
//! render passes.` module doc is a copy-paste from `RenderPass.zig` in the
//! reference; preserved verbatim per R1.)
//!
//! PORT NOTE (R3/objc2, HARD SPOT 2 [F6], `rust-mut-args-strongly-protected`):
//! ghostty stashes an `objc.Block` (`CompletionBlock`) over a `Context {
//! renderer: *Renderer, target: *Target, sync: bool }` and, on completion, derefs
//! the stashed `*Renderer` to call `renderer.api.present(target, sync)` +
//! `renderer.frameCompleted(health)`. Neither `GenericRenderer` nor
//! `frameCompleted`/`present` exists until Chunk 2/3, so Frame is made
//! 1b-compilable by a **forward-declared raw-pointer callback contract**: the
//! renderer pointer is type-erased to `*mut ()` and the completion body is a
//! plain-Rust `CompletionFn` fn-pointer that Chunk-3 `drawFrame` installs (the
//! faithful analogue of Zig's type-erased `CompletionBlock`; R1 "no-loss, not
//! no-addition"). The pointers are raw `*mut` (NEVER `&mut`) per
//! `rust-mut-args-strongly-protected`: the completion trampoline re-enters the
//! renderer while it is live inside `drawFrame` (the `@fieldParentPtr`/container_of
//! precedent), so a `&mut` here would be UB.
//!
//! The **sync path (the golden path) needs NO ObjC block**: `complete(true)` does
//! commit → waitUntilCompleted → set `sync=true` → call the fn-pointer directly
//! (Zig's `CompletionBlock.invoke`). The **async path**
//! (`addCompletedHandler:` via a `block2::RcBlock`) installs the completion block
//! over the by-value-`Copy` `CompletionContext` (W1-4).

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{MTLCommandBuffer, MTLCommandQueue};

use crate::metal::render_pass::{Attachment, Options as RpOptions, RenderPass};
use crate::metal::target::Target;

// const log = std.log.scoped(.metal);
// PORT NOTE: no logging facade is wired in the port yet (see `buffer.rs`); the
// `log.err("Failed to present render target: err={}", ...)` call site lives in the
// deferred Chunk-3 completion body below, preserved as a comment.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Error {
    /// A Metal API call failed.
    MetalFailed,
}

/// The completion context ghostty stashes as `CompletionBlock.Context`. Raw `*mut`
/// per `rust-mut-args-strongly-protected` (see the module PORT NOTE). `renderer` is
/// type-erased (`*mut ()`) so Frame compiles in 1b — Chunk-3 casts it back to
/// `*mut GenericRenderer` inside the installed callback.
// `Clone, Copy`: all three fields (`*mut ()`, `*mut Target`, `bool`) are `Copy`. The
// async `RcBlock` must own a BY-VALUE copy of this context (mirrors ghostty's
// `CompletionBlock.init(.{…})` copying the `Context` INTO the block), so the async
// body does not borrow `self.block` — which would dangle after `complete` returns.
#[derive(Clone, Copy)]
pub struct CompletionContext {
    pub renderer: *mut (),
    pub target: *mut Target,
    pub sync: bool,
}

/// The completion body. Chunk-3 supplies a concrete fn that casts `ctx.renderer`
/// back to `*mut GenericRenderer` and runs `renderer.api.present(*ctx.target,
/// ctx.sync)` (on healthy) + `renderer.frameCompleted(health)`. In 1b this is only
/// stored/typed, never authored (= Zig's `&bufferCompleted`).
pub type CompletionFn =
    unsafe fn(ctx: &CompletionContext, buffer: &ProtocolObject<dyn MTLCommandBuffer>);

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
    /// `renderer` is the type-erased `*mut GenericRenderer` (Chunk-3); once the
    /// frame has been completed, the `frameCompleted` method on the renderer is
    /// called with the health status of the frame. `target` is presented via the
    /// provided renderer's API when completed. `on_completed` is the concrete
    /// completion fn (Chunk-3 supplies it). Raw `*mut` per the aliasing model.
    pub fn begin(
        opts: Options<'_>,
        renderer: *mut (),
        target: *mut Target,
        on_completed: CompletionFn,
    ) -> Result<Self, Error> {
        let buffer = opts.queue.commandBuffer().ok_or(Error::MetalFailed)?;

        // Create our block to register for completion updates.
        // The block is deallocated by the objC runtime on success.
        //
        // Frame.zig stashes `CompletionBlock.init(.{ renderer, target, sync = false
        // }, &bufferCompleted)`; here the context is `{ renderer, target, sync =
        // false }` plus the `on_completed` fn-pointer (the type-erased analogue).
        Ok(Self {
            buffer,
            block: CompletionContext {
                renderer,
                target,
                sync: false,
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
    // PORT NOTE (HARD SPOT 2, deferred to Chunk 3): the completion body reproduces
    // `Frame.zig:67 bufferCompleted` verbatim. It is authored in Chunk 3 (it needs
    // `GenericRenderer`/`frameCompleted`/`api.present`, which do not exist in 1b);
    // in 1b `on_completed: CompletionFn` is only stored + called (see `complete`).
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
    // The Chunk-3 Rust shape (in the Metal/Renderer module):
    //     unsafe fn metal_buffer_completed(ctx: &CompletionContext, buffer: &ProtocolObject<dyn MTLCommandBuffer>) {
    //         let status = buffer.status();
    //         let health = match status {
    //             MTLCommandBufferStatus::Error => crate::Health::Unhealthy,
    //             _ => crate::Health::Healthy,
    //         };
    //         let renderer = ctx.renderer as *mut GenericRenderer;  // container_of / @fieldParentPtr
    //         if health == crate::Health::Healthy {
    //             let _ = (*renderer).api.present(&*ctx.target, ctx.sync);
    //         }
    //         (*renderer).frame_completed(health);
    //     }

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
            // Copy the context by value + the fn-ptr by move (matches Zig's by-value
            // Context copy — stays valid until Metal fires the handler). The block is
            // copied when `addCompletedHandler:` retains it and dealloc'd by the objc
            // runtime on completion (Frame.zig:106-108).
            let ctx = self.block; // Copy
            let on_completed = self.on_completed;
            let handler: block2::RcBlock<CompletionHandler> = block2::RcBlock::new(
                move |buf: core::ptr::NonNull<ProtocolObject<dyn MTLCommandBuffer>>| {
                    // SAFETY: buf is a valid MTLCommandBuffer for the callback's
                    // duration; ctx's raw ptrs are derefed NEVER as &mut (same
                    // container_of discipline as the sync path /
                    // metal_buffer_completed).
                    unsafe {
                        on_completed(&ctx, buf.as_ref());
                    }
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
            //
            // SAFETY: the renderer pointer is live for the duration of `drawFrame`;
            // single-threaded on the sync path (we just waited on the buffer); the
            // context is a raw `*mut`, never aliased as `&mut`.
            unsafe {
                (self.on_completed)(&self.block, &self.buffer);
            }
        }
    }
}
