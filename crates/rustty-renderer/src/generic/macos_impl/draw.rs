//! PORTS: ghostty src/renderer/generic.zig — Chunk 4, the draw path (`drawFrame`,
//! `drawFrameLocked`, `frameCompleted`, `initShaders`/`deinitShaders`,
//! `syncAtlasTexture`, `uploadBackgroundImage`, `animationWake`), plus the Metal
//! `Frame.zig` `bufferCompleted` trampoline and the port-only `HealthReport`.
//!
//! This file is part of the macOS `Renderer(Metal)` arm (`generic::macos_impl`).

use super::*;

// =======================================================================
// Chunk 4 — the offscreen draw path (GPU_RD_CHUNK4_DESIGN)
// =======================================================================

/// `= Frame.zig` `bufferCompleted`. Matches `crate::metal::frame::CompletionFn`.
//
// PORT NOTE (N440, P7-W7 T5a): the reference reaches `block.renderer.api` and
// `block.renderer.frameCompleted(..)` through the context's `*Renderer`. On the
// async path this runs on a Metal-owned thread WITHOUT the draw mutex while the
// render thread may hold a whole-renderer `&mut` under it (`Renderer::with_locked`),
// which in Rust is an aliasing violation (a data race with that retag). The port's
// context carries clones of exactly what this body needs (the layer cell, the
// target's IOSurface, the health cell, the frame semaphore, the surface
// mailbox and the health-report state), taken under the draw mutex when the
// frame began, so it forms no
// reference to any renderer byte. `.port/phase-2/mirihar-n440`:
// `shipped_async_handler` UB, `fixed_async_handler` and
// `fixed_async_and_sync_draw` CLEAN, under SB and TB.
fn metal_buffer_completed(ctx: &CompletionContext, buffer: &ProtocolObject<dyn MTLCommandBuffer>) {
    // const buffer = objc.Object.fromId(buffer_id);
    // Get our command buffer status to pass back to the generic renderer.
    // const status = buffer.getProperty(mtl.MTLCommandBufferStatus, "status");
    let status = buffer.status();
    // const health: Health = switch (status) { .@"error" => .unhealthy, else => .healthy };
    let health = match status {
        MTLCommandBufferStatus::Error => crate::Health::Unhealthy,
        _ => crate::Health::Healthy,
    };
    // If the frame is healthy, present it.
    if health == crate::Health::Healthy {
        // block.renderer.api.present(block.target.*, block.sync) catch |err|
        //     log.err("Failed to present render target: err={}", .{err});   (Frame.zig:82-88)
        // RESTORED (task-14c-2, S118-F4). This site's oracle is
        // `renderer/metal/Frame.zig:82-88`, whose scope is `.metal`, NOT
        // `generic.zig`'s `.generic_renderer` — so it targets
        // `crate::metal::frame::LOG_SCOPE` even though the code lives here. It lives
        // here at all because the port inlines `Frame.zig`'s `bufferCompleted` into
        // this completion trampoline. We consume the now-fallible Result faithfully
        // (the layer path can fail; W1-3). N440: `Metal.present`'s body over the
        // context's layer cell and IOSurface (`LayerCell::present`).
        if let Err(e) = ctx.layer.present(&ctx.surface, ctx.sync) {
            // log.err("Failed to present render target: err={}", .{err});
            log::error!(
                target: crate::metal::frame::LOG_SCOPE,
                "Failed to present render target: err={e:?}"
            );
        }
    }
    // block.renderer.frameCompleted(health);
    // (N440: over the context's clones; see `frame_completed_on`.)
    frame_completed_on(
        &ctx.health,
        &ctx.frame_sema,
        &ctx.surface_mailbox,
        &ctx.health_report,
        health,
    );
}

/// The renderer's health reports to the surface: the health the surface was
/// last told, whether a report is still owed, and how to retry one soon.
//
// rustty-only (R13, UPSTREAM-DEFECTS.md UD362). `frame_completed_on` reports
// with `.instant` where the reference blocks in `.forever`, so a full app
// mailbox can refuse a report. Instead of withdrawing the health cell (fix
// round 1; round 2 found that a withdrawal could restore a value that was
// itself never delivered), the surface's view is tracked on its own:
// - `reported`: the last health actually delivered (or, with a detached
//   mailbox, settled). Every completion compares its health with it and
//   reports when they differ, under this mutex, so `reported` always equals
//   what the surface last heard and a report never goes back in time.
// - `pending`: a report is owed (the last attempt was refused). While set,
//   `animation_wake` asks for draw-only wakes at `DRAW_INTERVAL_MS`, so every
//   draw renders a frame whose completion retries, and a render-thread draw
//   resyncs the display link, which then runs (vsync-paced) until the report
//   lands and a draw finds nothing to redraw.
// - `wake`: the render thread's `draw_now` wakeup, to start that retry at once.
//   It is sent only when a report BECOMES owed (not on every refusal), and is
//   skipped while the display link is running (its vsync tick already draws).
//   The draw it causes (`drawNowCallback`) arms the render thread's animation
//   timer while the report is still owed (fix round 3), and that Draw timer
//   re-arms itself every `DRAW_INTERVAL_MS` until `animation_wake` has nothing
//   left, drawing only while no display link runs; so the retries are paced
//   by vsync or by the timer, never by the completions. The same post-draw
//   check also catches a report refused while a no-redraw draw was parking
//   the link (the wake saw it running). Armed by `loop_enter_raw` / on link
//   creation, disarmed by `loop_exit_raw` (render thread) before `draw_now` is
//   deinitialized and the link released; a completion uses it only under the
//   same mutex.
// In its own allocation (an `Arc` the renderer and each in-flight frame's
// `CompletionContext` share, N440).
pub struct HealthReport {
    reported: std::sync::Mutex<crate::Health>,
    pending: AtomicBool,
    wake: std::sync::Mutex<HealthReportWake>,
}

/// `HealthReport`'s wakeup state.
#[derive(Default)]
struct HealthReportWake {
    draw_now: Option<Async>,
    link: Option<DisplayLinkHandle>,
}

// SAFETY: `link` is a CVDisplayLink handle, only queried (`isRunning`) while
// armed: it is set on the render thread when the link is created and cleared
// in `loop_exit_raw`, before `Renderer::deinit` releases the link, under the
// mutex every reader holds. CoreVideo's display-link calls are thread-safe.
// `draw_now` is a mach-port name (Copy, Send).
unsafe impl Send for HealthReportWake {}

impl Default for HealthReport {
    fn default() -> Self {
        Self::new()
    }
}

impl HealthReport {
    pub fn new() -> HealthReport {
        HealthReport {
            // The surface starts out assuming a healthy renderer (the
            // reference's `health` starts `.healthy` and reports changes only).
            reported: std::sync::Mutex::new(crate::Health::Healthy),
            pending: AtomicBool::new(false),
            wake: std::sync::Mutex::new(HealthReportWake::default()),
        }
    }

    fn wake_state(&self) -> std::sync::MutexGuard<'_, HealthReportWake> {
        self.wake.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The health the surface was last told.
    pub fn reported(&self) -> crate::Health {
        *self.reported.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Whether a report is owed (the last attempt was refused).
    pub fn pending(&self) -> bool {
        self.pending.load(Ordering::SeqCst)
    }

    /// Arms (`Some`, `loopEnter`) or disarms (`None`, `loopExit`) the
    /// `draw_now` wakeup; disarming also forgets the display link.
    pub fn arm(&self, draw_now: Option<Async>) {
        let mut w = self.wake_state();
        if draw_now.is_none() {
            w.link = None;
        }
        w.draw_now = draw_now;
    }

    /// The render thread created its display link (the wakeup skips
    /// `draw_now` while it runs).
    pub(crate) fn set_link(&self, link: Option<DisplayLinkHandle>) {
        self.wake_state().link = link;
    }

    /// Wakes the render thread to retry a report that just became owed: not
    /// while the display link runs (its tick draws). Returns whether
    /// `draw_now` was notified.
    pub fn wake(&self) -> bool {
        let w = self.wake_state();
        let Some(draw_now) = w.draw_now else {
            return false;
        };
        if let Some(link) = w.link {
            // SAFETY: see `HealthReportWake`'s `Send` impl: the link is live
            // while armed.
            if unsafe { link.is_running() } {
                return false;
            }
        }
        if let Err(err) = draw_now.notify() {
            log::warn!(target: LOG_SCOPE, "error notifying draw_now err={err:?}");
        }
        true
    }

    /// Test-only: mark a report as owed.
    #[cfg(test)]
    pub(crate) fn set_pending_for_test(&self) {
        self.pending.store(true, Ordering::SeqCst);
    }
}

/// `frameCompleted`'s body over the cells it touches: the renderer's `health`,
/// its swap chain's `frame_sema`, its surface mailbox and its health reports.
//
// PORT NOTE (N440, P7-W7 T5a): the reference's `frameCompleted(self: *Self, ..)`
// reaches these through the renderer; the completion handler passes its
// context's clones instead (see `metal_buffer_completed`), and the test-only
// `Renderer::frame_completed` passes the renderer's own.
pub(crate) fn frame_completed_on(
    health_cell: &AtomicHealth,
    frame_sema: &Semaphore,
    surface_mailbox: &SurfaceMailbox,
    report: &HealthReport,
    health: crate::Health,
) {
    // If our health value hasn't changed, then we do nothing. We don't
    // do a cmpxchg here because strict atomicity isn't important.
    // if (self.health.load(.seq_cst) != health) {
    //     self.health.store(health, .seq_cst);
    if health_cell.load() != health {
        health_cell.store(health);
    }

    // Our health value changed, so we notify the surface so that it
    // can do something about it.
    // _ = self.surface_mailbox.push(.{ .renderer_health = health }, .{ .forever = {} });
    //   — REAL (seam §B.2 resolved): core's sink re-wraps this into
    //     `apprt::surface::Message::RendererHealth` and enqueues it.
    //     Detached (offscreen) mailboxes still return 0.
    //
    // PORT NOTE: reference defect (R13), UD362 (Metal). The reference pushes
    // with `.forever`, which blocks while the app mailbox is full: (a) on the
    // Metal completion thread, before `releaseFrame`, while the main thread's
    // sync draw holds `draw_mutex` and waits in `nextFrame` for exactly that
    // release (the mailbox's only drainer waits on the frame, the completion
    // thread on the mailbox); (b) on the main thread itself, whose sync draw
    // completes its frame inline, into the mailbox only it drains. The port
    // pushes `.instant`, so a completion never blocks, and keeps the
    // reference's order (store, push, release), so `SwapChain::deinit`, which
    // waits for every frame's release, still waits for every push. The report
    // is decided against what the surface was last told (`HealthReport`), not
    // against the previous frame, so a refused report stays owed: it is
    // retried by the next completion (a paced redraw is requested for that).
    {
        let mut reported = report.reported.lock().unwrap_or_else(|e| e.into_inner());
        if *reported == health {
            report.pending.store(false, Ordering::SeqCst);
        } else if !surface_mailbox.is_attached()
            || surface_mailbox.push(
                SurfaceMessage::RendererHealth(health),
                MailboxTimeout::Instant,
            ) > 0
        {
            *reported = health;
            report.pending.store(false, Ordering::SeqCst);
        } else {
            // Wake the render thread only when the report becomes owed; while
            // it stays owed the render thread's animation timer (or the display
            // link) paces the retries.
            let was_owed = report.pending.swap(true, Ordering::SeqCst);
            drop(reported);
            if !was_owed {
                report.wake();
            }
        }
    }

    // Always release our semaphore. The swap chain is
    // guaranteed to exist here: it is only torn down after
    // waiting for all in-flight frames to complete, and this
    // callback is what signals that completion.
    // self.swap_chain.?.releaseFrame();
    frame_sema.post();
}

impl Renderer {
    // fn deinitShaders(self: *Self) void { self.shaders.deinit(self.alloc); }
    //
    // PORT NOTE (dead-code / `-D warnings`, GPU_RD_CHUNK4_DESIGN §6.4 following the
    // GPU_RD_CHUNK3A_DESIGN §B.4 "make pub, ZERO #[allow(dead_code)]" precedent):
    // `deinit_shaders`'s SOLE reference is `Renderer::deinit` (ref generic.zig
    // :825), which stays the `unimplemented!()` seam (§1). `draw_frame`'s reinit
    // branch calls `self.shaders.deinit()` DIRECTLY (ref :1500), NOT
    // `self.deinit_shaders()` — so it has no live caller in Chunk 4 and a private
    // `fn` would trip `-D warnings`. Widened to `pub fn` (a pub method on the
    // re-exported `Renderer` is reachable ⇒ dead-code-exempt) until
    // `Renderer::deinit` un-defers. Zig `alloc` param dissolves (`Shaders::deinit`
    // is objc2/Vec-managed).
    pub fn deinit_shaders(&mut self) {
        self.shaders.deinit();
    }

    /// The soonest animation wake this renderer needs, if any:
    /// custom shader animation wants continuous draw-only wakes
    /// at draw_interval_ms while active, and a running Kitty
    /// graphics animation wants an update wake when its next
    /// frame is due. The renderer thread drives its animation
    /// timer off this, re-querying after every wake.
    ///
    /// Must be called on the render thread.
    //
    // PORT NOTE (`aee7bf347`): REPLACES `hasAnimations` (deleted upstream);
    // `pub` — the renderer thread's `armAnimationTimer` (core,
    // `renderer_seam::Thread`) reaches it cross-crate.
    //
    // pub fn animationWake(self: *const Self) ?AnimationWake {
    //     // Custom shaders animate by redrawing on a fixed cadence,
    //     // gated by configuration and focus.
    //     const shader_delay: ?u64 = shader: {
    //         if (!self.has_custom_shaders) break :shader null;
    //         break :shader switch (self.config.custom_shader_animation) {
    //             .false => null,
    //             .always => draw_interval_ms,
    //             .true => if (self.focused) draw_interval_ms else null,
    //         };
    //     };
    //
    //     // Kitty animations tick during updateFrame; between
    //     // updates the deadline is absolute on the animation
    //     // clock, so a stream of draw wakes recomputing this
    //     // cannot starve it into the future.
    //     const kitty_delay: ?u64 = kitty: {
    //         const next = self.kitty_animation_next_ms orelse break :kitty null;
    //         const base = self.kitty_animation_clock orelse break :kitty null;
    //         const now: std.Io.Timestamp = .now(global.io(), .awake);
    //         const now_ms: u64 = @intCast(@divTrunc(
    //             base.durationTo(now).nanoseconds,
    //             std.time.ns_per_ms,
    //         ));
    //         // Never wake faster than the draw interval; an
    //         // overdue frame is picked up on the next wake.
    //         break :kitty @max(next -| now_ms, draw_interval_ms);
    //     };
    //
    //     // An update wake includes a draw, so it wins ties.
    //     if (kitty_delay) |k| {
    //         if (shader_delay == null or k <= shader_delay.?) {
    //             return .{ .delay_ms = k, .kind = .update };
    //         }
    //     }
    //
    //     if (shader_delay) |s| return .{ .delay_ms = s, .kind = .draw };
    //
    //     return null;
    // }
    pub fn animation_wake(&self) -> Option<AnimationWake> {
        // Custom shaders animate by redrawing on a fixed cadence,
        // gated by configuration and focus.
        let shader_delay: Option<u64> = 'shader: {
            if !self.has_custom_shaders {
                break 'shader None;
            }
            break 'shader match self.config.custom_shader_animation {
                crate::thread::CustomShaderAnimation::False => None,
                crate::thread::CustomShaderAnimation::Always => Some(DRAW_INTERVAL_MS),
                crate::thread::CustomShaderAnimation::True => {
                    if self.focused {
                        Some(DRAW_INTERVAL_MS)
                    } else {
                        None
                    }
                }
            };
        };

        // PORT NOTE: reference defect (R13), UD362. An owed health report
        // (`HealthReport`) wants draw-only wakes at the draw interval, so
        // every draw renders a frame whose completion retries it.
        let shader_delay = if self.health_report.pending() {
            Some(shader_delay.map_or(DRAW_INTERVAL_MS, |d| d.min(DRAW_INTERVAL_MS)))
        } else {
            shader_delay
        };

        // Kitty animations tick during updateFrame; between
        // updates the deadline is absolute on the animation
        // clock, so a stream of draw wakes recomputing this
        // cannot starve it into the future.
        let kitty_delay: Option<u64> = 'kitty: {
            let next = match self.kitty_animation_next_ms {
                Some(next) => next,
                None => break 'kitty None,
            };
            let base = match self.kitty_animation_clock {
                Some(base) => base,
                None => break 'kitty None,
            };
            let now = Instant::now();
            // (`base.durationTo(now).nanoseconds / ns_per_ms` — a monotonic
            // clock, so the duration is never negative.)
            let now_ms: u64 = now.duration_since(base).as_millis() as u64;
            // Never wake faster than the draw interval; an
            // overdue frame is picked up on the next wake.
            break 'kitty Some(core::cmp::max(
                next.saturating_sub(now_ms),
                DRAW_INTERVAL_MS,
            ));
        };

        // An update wake includes a draw, so it wins ties.
        if let Some(k) = kitty_delay {
            if shader_delay.is_none() || k <= shader_delay.unwrap() {
                return Some(AnimationWake {
                    delay_ms: k,
                    kind: AnimationWakeKind::Update,
                });
            }
        }

        if let Some(s) = shader_delay {
            return Some(AnimationWake {
                delay_ms: s,
                kind: AnimationWakeKind::Draw,
            });
        }

        None
    }

    /// `init_shaders` (the reference's `initShaders`) through a raw root, with narrow field access only.
    ///
    /// # Safety
    /// `self_ptr` is a valid, whole-`Renderer`-provenance pointer, and the
    /// caller holds the draw mutex (or is the only thread using the renderer).
    //
    // N437 / ruling 32 (P7-W7 T2 fix round 1): the body is `init_shaders_raw`, which
    // reaches the renderer only through narrow field places of a raw root, so
    // the main thread's sync draw (`display_callback` → `draw_frame_raw`) never
    // retags the whole renderer while the render thread works on its own fields
    // unlocked inside `update_frame_raw`.
    pub(super) unsafe fn init_shaders_raw(self_ptr: *mut Renderer) -> Result<(), Error> {
        unsafe {
            // var arena = ArenaAllocator.init((*self_ptr).alloc);
            // defer arena.deinit();
            // const arena_alloc = arena.allocator();
            //
            // Load our custom shaders
            // const custom_shaders: []const [:0]const u8 = shadertoy.loadFromFiles(
            //     arena_alloc,
            //     (*self_ptr).config.custom_shaders,
            //     GraphicsAPI.custom_shader_target,
            // ) catch |err| err: {
            //     log.warn("error loading custom shaders err={}", .{err});
            //     break :err &.{};
            // };
            //
            // The reference arena is subsumed by owned `Vec<CString>` (dropped after
            // `init_shaders` returns — `init_post_pipeline` copies the sources into an
            // `NSString`). The `catch |err| break :err &.{}` (any load error ⇒ no
            // custom shaders, never block the renderer) maps to `unwrap_or_else`.
            let custom_shaders: Vec<CString> = shadertoy::load_from_files(
                &(*self_ptr).config.custom_shaders,
                Metal::CUSTOM_SHADER_TARGET,
            )
            .unwrap_or_else(|err| {
                // log.warn("error loading custom shaders err={}", .{err});
                log::warn!(target: LOG_SCOPE, "error loading custom shaders err={err:?}");
                Vec::new()
            });

            // GraphicsAPI.initShaders takes `[]const [:0]const u8`; build the borrow slice.
            let custom_refs: Vec<&CStr> = custom_shaders.iter().map(|c| c.as_c_str()).collect();

            // const has_custom_shaders = custom_shaders.len > 0;
            let has_custom_shaders = !custom_refs.is_empty();

            // var shaders = try (*self_ptr).api.initShaders((*self_ptr).alloc, custom_shaders);
            // errdefer shaders.deinit((*self_ptr).alloc);  → the `?` early-return `Drop`.
            // (*self_ptr).shaders = shaders;
            (*self_ptr).shaders = (*self_ptr).api.init_shaders(&custom_refs)?;
            // (*self_ptr).has_custom_shaders = has_custom_shaders;
            (*self_ptr).has_custom_shaders = has_custom_shaders;
            Ok(())
        }
    }

    /// Sync the atlas data to the given texture. This copies the bytes
    /// associated with the atlas to the given texture. If the atlas no
    /// longer fits into the texture, the texture will be resized.
    //
    // PORT NOTE (raw-ptr — MIRIHAR-PROVEN deviation from GPU_RD_CHUNK4_DESIGN
    // §6.3's literal signature): the reference is `syncAtlasTexture(self: *const
    // Self, atlas: *const font.Atlas, texture: *Texture)` — `self` and `texture`
    // are RAW pointers. The design §6.3's Rust signature `fn sync_atlas_texture(
    // &self, atlas: &Atlas, texture: &mut Texture)` is UNSOUND: `texture` is `&mut
    // frame.grayscale`, which aliases INTO `self.swap_chain`, so a `&self` receiver
    // FREEZES the texture bytes and the subsequent `*texture` write pops that frozen
    // tag — UB under BOTH Stacked Borrows AND Tree Borrows (PROVEN on
    // `.port/phase-2/mirihar-chunk3b`'s `atlas_self_isolated` candidate; the raw-self
    // form is proven clean by `drawframe_shipped`). Ported as raw self + raw texture
    // (the faithful Zig shape): it reborrows only `self.api` (narrow, disjoint from
    // `swap_chain`) and writes `*texture`. `atlas` stays a real `&FontAtlas` — it
    // lives in the `*mut SharedGrid` (a SEPARATE allocation, no aliasing with self).
    unsafe fn sync_atlas_texture(
        self_raw: *const Renderer,
        atlas: &FontAtlas,
        texture: *mut Texture,
    ) -> Result<(), Error> {
        // if (atlas.size > texture.width) {
        if atlas.size() as usize > unsafe { (*texture).width } {
            // Free our old texture / Reallocate. The reassignment drops the old
            // `Texture` (Retained release) — the reference `texture.*.deinit()`.
            // texture.* = try self.api.initAtlasTexture(atlas);
            unsafe {
                *texture = (*self_raw).api.init_atlas_texture(atlas)?;
            }
        }

        // try texture.replaceRegion(0, 0, atlas.size, atlas.size, atlas.data);
        unsafe {
            (*texture).replace_region(
                0,
                0,
                atlas.size() as usize,
                atlas.size() as usize,
                atlas.data(),
            );
        }
        Ok(())
    }

    // fn uploadBackgroundImage(self: *Self) !void
    /// `upload_background_image` (the reference's `uploadBackgroundImage`) through a raw root, with narrow field access only.
    ///
    /// # Safety
    /// `self_ptr` is a valid, whole-`Renderer`-provenance pointer, and the
    /// caller holds the draw mutex (or is the only thread using the renderer).
    //
    // N437 / ruling 32 (P7-W7 T2 fix round 1): the body is `upload_background_image_raw`, which
    // reaches the renderer only through narrow field places of a raw root, so
    // the main thread's sync draw (`display_callback` → `draw_frame_raw`) never
    // retags the whole renderer while the render thread works on its own fields
    // unlocked inside `update_frame_raw`.
    unsafe fn upload_background_image_raw(self_ptr: *mut Renderer) -> Result<(), Error> {
        unsafe {
            // Make sure our bg image is uploaded if it needs to be.
            // if ((*self_ptr).bg_image) |*bg| {
            if let Some(bg) = (*self_ptr).bg_image.as_mut() {
                // if (bg.isUnloading()) { bg.deinit((*self_ptr).alloc); (*self_ptr).bg_image = null; return; }
                if bg.is_unloading() {
                    // `Image::deinit` consumes `self` by value (the reference
                    // `bg.deinit((*self_ptr).alloc)`), so take the image OUT of the Option to
                    // move it into `deinit`; then the Option is left `None`.
                    let img = (*self_ptr).bg_image.take().unwrap();
                    img.deinit((*self_ptr).alloc);
                    return Ok(());
                }
                // if (bg.isPending()) try bg.upload((*self_ptr).alloc, &(*self_ptr).api);
                if bg.is_pending() {
                    bg.upload((*self_ptr).alloc, &(*self_ptr).api)
                        .map_err(|_| Error::MetalFailed)?;
                }
            }
            Ok(())
        }
    }

    /// Callback from the graphics API when a frame is completed.
    //
    // PORT NOTE (N440, P7-W7 T5a): the reference's `frameCompleted(self: *Self,
    // health)`, over the renderer's own `health`, swap-chain semaphore and
    // surface mailbox, as an asynchronously completed frame reports. The
    // production completion handler (`metal_buffer_completed`) does not call
    // this: it runs `frame_completed_on` over its context's clones, since it
    // must not touch the renderer's bytes. Only the tests call this, with
    // exclusive access to the renderer.
    #[cfg(test)]
    pub(crate) fn frame_completed(&self, health: crate::Health) {
        // The swap chain is guaranteed to exist here (see `frame_completed_on`).
        let swap_chain = self.swap_chain.as_ref().unwrap();
        frame_completed_on(
            &self.health,
            &swap_chain.frame_sema,
            &self.surface_mailbox,
            &self.health_report,
            health,
        );
    }

    /// Draw the frame to the screen.
    ///
    /// If `sync` is true, this will synchronously block until
    /// the frame is finished drawing and has been presented.
    //
    // PORT NOTE (raw-ptr discipline, GPU_RD_CHUNK4_DESIGN §5.2 / MIRIHAR-PROVEN):
    // the whole body drives `self` through the SINGLE raw root `self_ptr = self`
    // with NARROW field reborrows (`(*self_ptr).field`), and re-derives the frame
    // raw root `frame` FRESH via `addr_of_mut!((*self_ptr).swap_chain.frames[idx])`
    // AFTER every whole-`&mut self` call (`upload_background_image`,
    // `update_custom_shader_uniforms_for_frame`, `update_screen_size_uniforms`,
    // `init_shaders`) — because a whole `&mut *self_ptr` reborrow POPS a held
    // `frame` tag (H1: SB UB, PROVEN by the `drawframe_naive` mirihar candidate).
    // The trampoline (`Frame::complete`) re-enters through the type-erased
    // `self_ptr` (H2). The full structure is clean under SB AND TB
    // (`drawframe_shipped`). `swap_chain.frames` is a fixed ARRAY (place-projected
    // `[idx]`, no `Index` autoref).
    //
    // W5 (`W5_DESIGN §2.3`): the raw-self ENTRY. Faithful to Zig
    // `drawFrame(self: *Renderer)` (a raw-pointer receiver — no protector). BOTH
    // concurrent production callers invoke THIS through a raw `*mut Renderer`:
    // (a) the render thread's `renderer.Thread` owner (above rustty-renderer; it
    // holds `renderer: *mut Renderer`, faithful to Zig's raw `*Renderer` field),
    // and (b) the main-thread display callback (metal_impl `display_callback`). No
    // whole-`Renderer` `&mut` protector ever spans the call, so the two concurrent
    // calls never alias a protected reference; the `draw_mutex` (locked below)
    // serialises the field accesses. Proven on mirihar-chunk3b
    // (`displaycb_concurrent`); the `displaycb_naive` UB control shows the split is
    // load-bearing (a protected `&mut self` receiver racing this raw draw is UB).
    //
    // N437: `lock` is the caller's own `draw_mutex` handle (`crate::latest_frame`),
    // taken before the render thread started (the main thread's display callback
    // carries one in its context, `metal_impl::DisplayCallbackCtx`); the body
    // forms references to the renderer only while it is held.
    //
    /// # Safety
    /// `self_ptr` must be a valid, well-aligned, whole-`Renderer`-provenance
    /// pointer, and outlive the call; `lock` is a handle to its `draw_mutex`.
    /// Concurrent calls are permitted (serialised by `draw_mutex`).
    pub unsafe fn draw_frame_raw(
        self_ptr: *mut Renderer,
        lock: &DrawMutex,
        sync: bool,
    ) -> Result<(), Error> {
        // const start = std.time.Instant.now() catch unreachable;
        // const start_micro = std.time.microTimestamp();
        // defer {
        //     const end = std.time.Instant.now() catch unreachable;
        //     log.warn(
        //         "[drawFrame time] start_micro={} duration={}ns",
        //         .{ start_micro, end.since(start) / std.time.ns_per_us },
        //     );
        // }

        // Everything that touches draw state happens under the draw
        // mutex. The display link is synced only after the mutex is
        // released; see `syncDisplayLink` for why it must never be
        // called with the draw mutex held.
        // const sync_display_link = locked: {
        //     self.draw_mutex.lockUncancelable(global.io());
        //     defer self.draw_mutex.unlock(global.io());
        //     break :locked try self.drawFrameLocked(sync);
        // };
        // (`self_ptr` is now the raw-self receiver; W5_DESIGN §2.3.)
        let sync_display_link = {
            lock.lock();
            // `defer self.draw_mutex.unlock()` as an RAII guard over the caller's
            // handle; it also unlocks on the `try` early return, as the
            // reference's `defer`.
            struct UnlockGuard<'a>(&'a DrawMutex);
            impl Drop for UnlockGuard<'_> {
                fn drop(&mut self) {
                    self.0.unlock();
                }
            }
            let _unlock = UnlockGuard(lock);
            let sync_display_link = unsafe { Self::draw_frame_locked(self_ptr, sync) }?;
            // PORT NOTE: reference defect (R13), UD352. The resync's reads of
            // draw state are taken HERE, still under the mutex, and only the
            // CoreVideo calls run after the unlock. Upstream reads them
            // unlocked, racing the main thread's `drawFrame(sync)`. On this
            // path the outcome is the same either way: a no-redraw draw saw
            // `cells_rebuilt == false` and no animation, and only the render
            // thread (this one) can change them back.
            // (`display_link_sync(&self)` is a transient shared reborrow,
            // excluded from the other drawer by the held mutex.)
            sync_display_link.then(|| unsafe { (*self_ptr).display_link_sync() })
        };

        #[cfg(test)]
        tests::between_draw_unlock_and_sync(self_ptr);

        // if (sync_display_link) self.syncDisplayLink(null, null);
        // (`draw_now`/`display_id` are null here, so no link is created or
        // retargeted: only the start/stop tail remains, and it touches no
        // renderer bytes.)
        if let Some(link_sync) = sync_display_link {
            link_sync.apply();
        }
        Ok(())
    }

    /// The body of `drawFrame`. Must be called with `draw_mutex` held.
    ///
    /// Returns true if the display link should be resynced once the
    /// draw mutex is released. This is only ever true on the no-redraw
    /// path, which a sync draw never takes, so the main thread's sync
    /// draws never touch the display link and `syncDisplayLink` stays
    /// on the render thread. (Rustty-only, UD362: also on a non-sync draw
    /// while a health report is owed.)
    // generic.zig `drawFrameLocked` (`72cf50855`): `fn drawFrameLocked(self:
    // *Self, sync: bool) !bool`. Same raw-ptr discipline as `draw_frame_raw`.
    //
    /// # Safety
    /// As [`draw_frame_raw`](Self::draw_frame_raw), and the caller holds
    /// `draw_mutex` for the whole call.
    // PORT NOTE: `&(&(*self_ptr).shaders.pipelines).x` spells out the reference
    // the pipelines `Undefined` derefs through (the rustc lint `dangerous_implicit_autorefs`
    // rejects the implicit one on a raw-pointer place); clippy calls that borrow
    // needless.
    #[allow(clippy::needless_borrow)]
    unsafe fn draw_frame_locked(self_ptr: *mut Renderer, sync: bool) -> Result<bool, Error> {
        // Hand-rolled `defer`s as RAII guards, each holding the raw root `self_ptr`
        // and performing its cleanup via a NARROW field reborrow on `drop` (so no
        // guard holds a live `&`/`&mut` across the body). Declared so they drop
        // LIFO in the reference order: `draw_frame_end` → scrollbar (Rust drops
        // locals in reverse declaration order); the caller's unlock follows.
        // `frame_ctx.complete(sync)` is the reference's LAST-registered `defer`
        // (fires FIRST); it is called explicitly at the end of the body, before
        // these guards drop.
        struct ScrollbarGuard(*mut Renderer);
        impl Drop for ScrollbarGuard {
            fn drop(&mut self) {
                // After the graphics API is complete (so we defer) we want to
                // update our scrollbar state.
                // defer if (self.scrollbar_dirty) {
                //     // Fail instantly if the surface mailbox if full, we'll just
                //     // get it on the next frame.
                //     if (self.surface_mailbox.push(.{ .scrollbar = self.scrollbar },
                //         .instant) > 0) self.scrollbar_dirty = false;
                // };
                // (REAL, seam §B.2 resolved: core's sink enqueues
                //  `apprt::surface::Message::Scrollbar`. A DETACHED mailbox —
                //  the offscreen golden/unit renderers, which have no surface —
                //  still returns 0, so the golden observes no mailbox.)
                unsafe {
                    if (*self.0).scrollbar_dirty
                        && (*self.0).surface_mailbox.push(
                            SurfaceMessage::Scrollbar((*self.0).scrollbar),
                            MailboxTimeout::Instant,
                        ) > 0
                    {
                        (*self.0).scrollbar_dirty = false;
                    }
                }
            }
        }
        struct DrawEndGuard(*mut Renderer);
        impl Drop for DrawEndGuard {
            fn drop(&mut self) {
                // defer self.api.drawFrameEnd();
                unsafe { (*self.0).api.draw_frame_end() };
            }
        }
        // errdefer-only guard: releases the swap-chain semaphore ONLY on an early
        // error return after `next_frame`. On the success path it is disarmed (the
        // frame's `frameCompleted` releases instead — else a double-release).
        struct ReleaseGuard {
            r: *mut Renderer,
            armed: bool,
        }
        impl Drop for ReleaseGuard {
            fn drop(&mut self) {
                // errdefer swap_chain.releaseFrame();
                // (the chain exists whenever the guard is armed — armed only
                // after `next_frame` on the established-Some chain.)
                if self.armed {
                    unsafe { (*self.r).swap_chain.as_ref().unwrap().release_frame() };
                }
            }
        }

        let _scrollbar = ScrollbarGuard(self_ptr);

        // Let our graphics API do any bookkeeping, etc.
        // that it needs to do before / after `drawFrame`.
        // self.api.drawFrameStart();
        // defer self.api.drawFrameEnd();
        unsafe { (*self_ptr).api.draw_frame_start() };
        let _draw_end = DrawEndGuard(self_ptr);

        // Retrieve the most up-to-date surface size from the Graphics API
        // const surface_size = try self.api.surfaceSize();
        let surface_size = unsafe { (*self_ptr).api.surface_size() }?;

        // If either of our surface dimensions is zero
        // then drawing is absurd, so we just return.
        if surface_size.width == 0 || surface_size.height == 0 {
            return Ok(false);
        }

        // If we have no graphics context we can't draw. This is
        // only the case while unrealized (GTK); displayRealized
        // rebuilds the swap chain.
        // if (!self.display_realized) return false;
        if !unsafe { (*self_ptr).display_realized } {
            return Ok(false);
        }

        // Get our swap chain, rebuilding it if it was released
        // while we were hidden. Rebuilding is deferred to draw
        // time because resource creation must happen somewhere
        // our graphics API allows it (OpenGL requires a current
        // context, which drawFrame guarantees).
        //
        // const swap_chain: *SwapChain, const swap_chain_rebuilt: bool =
        //     if (self.swap_chain) |*sc| .{ sc, false } else rebuild: {
        //         self.swap_chain = try SwapChain.init(
        //             self.api,
        //             self.has_custom_shaders,
        //         );
        //         break :rebuild .{ &self.swap_chain.?, true };
        //     };
        //
        // (raw-ptr discipline: instead of the reference's held `*SwapChain`
        // local, the chain is re-derived FRESH from `self_ptr` at every use
        // below — the same rule as `frame`; §5.2.)
        let swap_chain_rebuilt: bool = if unsafe { (*self_ptr).swap_chain.is_some() } {
            false
        } else {
            let sc = SwapChain::init(unsafe { &(*self_ptr).api }, unsafe {
                (*self_ptr).has_custom_shaders
            })?;
            unsafe { (*self_ptr).swap_chain = Some(sc) };
            true
        };

        let size_changed = unsafe {
            (*self_ptr).size.screen.width != surface_size.width
                || (*self_ptr).size.screen.height != surface_size.height
        };

        // Conditions under which we need to draw the frame, otherwise we
        // don't need to since the previous frame should be identical.
        //
        // While any animation is in progress (a pending animation wake)
        // every draw must actually render.
        // (UD362: an owed health report is such an animation wake, see
        // `animation_wake` and `HealthReport`.)
        let needs_redraw = size_changed
            || swap_chain_rebuilt
            || unsafe { (*self_ptr).cells_rebuilt.load(Ordering::Relaxed) }
            || sync
            || unsafe { (*self_ptr).animation_wake().is_some() };

        if !needs_redraw {
            // (`2f0b65346` deleted the `try self.api.presentLastTarget()` that
            // preceded this; it was a no-op for Metal.)
            //
            // Ask our caller to resync the display link once the draw
            // mutex is released, because we can probably pause the
            // display link at this point.
            return Ok(true);
        }
        // self.cells_rebuilt = false;
        // (UD352: atomic; this store runs under `draw_mutex`.)
        unsafe { (*self_ptr).cells_rebuilt.store(false, Ordering::Relaxed) };

        // Wait for a frame to be available.
        // const frame = swap_chain.nextFrame();
        // errdefer swap_chain.releaseFrame();
        //
        // `next_frame` is called for its side effects (semaphore wait + index
        // bump); the returned `&mut FrameState` is dropped
        // immediately and `frame` is re-derived FRESH from `self_ptr` at each use
        // (§5.2 — a held frame raw root is popped by any whole-`&mut self`
        // reborrow). `frame_index` now holds the chosen index. The chain is
        // Some here (established/rebuilt above; teardown is excluded by the
        // held draw_mutex).
        let _ = unsafe { (*self_ptr).swap_chain.as_mut().unwrap().next_frame() };
        let mut release_guard = ReleaseGuard {
            r: self_ptr,
            armed: true,
        };
        let idx = unsafe { (*self_ptr).swap_chain.as_ref().unwrap().frame_index as usize };
        // N440: the frame's completion releases it through its own clone of the
        // semaphore (`CompletionContext`), taken here, before the frame root.
        let frame_sema = unsafe { (*self_ptr).swap_chain.as_ref().unwrap().frame_sema.clone() };
        // log.debug("drawing frame index={}", .{swap_chain.frame_index});

        // If we need to reinitialize our shaders, do so.
        // if (self.reinitialize_shaders) {
        //     self.reinitialize_shaders = false;
        //     self.shaders.deinit(self.alloc);
        //     try self.initShaders();
        // }
        if unsafe { (*self_ptr).reinitialize_shaders } {
            unsafe {
                (*self_ptr).reinitialize_shaders = false;
                // `drawFrame` deinits `self.shaders` DIRECTLY (ref :1500), NOT via
                // `self.deinit_shaders()` — keeping `deinit_shaders` referenced only
                // by the deferred `Renderer::deinit` (§6.4).
                (*self_ptr).shaders.deinit();
                Self::init_shaders_raw(self_ptr)?; // narrow (ruling 32)
            }
        }

        // Our shaders should not be defunct at this point.
        // assert(!self.shaders.defunct);
        assert!(!unsafe { (*self_ptr).shaders.defunct });

        // If we have custom shaders, make sure we have the
        // custom shader state in our frame state, otherwise
        // if we have a state but don't need it we remove it.
        // (Golden: has_custom_shaders=false + custom_shader_state=None ⇒ no-op.)
        {
            let frame: *mut FrameState = unsafe {
                // Narrow projection through the `Option` (a fresh,
                // immediately-discarded reborrow of ONLY the `swap_chain`
                // field; the chain is Some — established above, teardown
                // excluded by the held draw_mutex).
                addr_of_mut!((*self_ptr).swap_chain.as_mut().unwrap().frames[idx])
            };
            // if (self.has_custom_shaders) {
            if unsafe { (*self_ptr).has_custom_shaders } {
                // if (frame.custom_shader_state == null) {
                //     frame.custom_shader_state = try .init(self.api);
                //     try frame.custom_shader_state.?.resize(self.api, w, h);
                // }
                if unsafe { (*frame).custom_shader_state.is_none() } {
                    unsafe {
                        (*frame).custom_shader_state =
                            Some(CustomShaderState::init(&(*self_ptr).api)?);
                        (*frame).custom_shader_state.as_mut().unwrap().resize(
                            &(*self_ptr).api,
                            surface_size.width as usize,
                            surface_size.height as usize,
                        )?;
                    }
                }
            }
            // } else if (frame.custom_shader_state) |*state| {
            //     state.deinit();
            //     frame.custom_shader_state = null;
            // }
            else if unsafe { (*frame).custom_shader_state.is_some() } {
                // `state.deinit()` is the `CustomShaderState` field `Drop`; nulling
                // the Option drops it.
                unsafe { (*frame).custom_shader_state = None };
            }
        }

        // If our stored size doesn't match the surface size we need to update it.
        // if (size_changed) {
        //     self.size.screen = .{ .width = w, .height = h };
        //     self.updateScreenSizeUniforms();
        // }
        // (Golden: size_changed=false ⇒ skipped. `update_screen_size_uniforms` is a
        //  whole-&mut self call, so no `frame` raw root is held across this.)
        if size_changed {
            unsafe {
                (*self_ptr).size.screen = ScreenSize {
                    width: surface_size.width,
                    height: surface_size.height,
                };
                Self::update_screen_size_uniforms_raw(self_ptr);
            }
        }

        // If this frame's target isn't the correct size, or the target
        // config has changed (such as when the blending mode changes),
        // remove it and replace it with a new one with the right values.
        // if (frame.target.width != self.size.screen.width or
        //     frame.target.height != self.size.screen.height or
        //     frame.target_config_modified != self.target_config_modified) {
        //     try frame.resize(self.api, self.size.screen.width, self.size.screen.height);
        //     frame.target_config_modified = self.target_config_modified;
        // }
        {
            let frame: *mut FrameState = unsafe {
                // Narrow projection through the `Option` (a fresh,
                // immediately-discarded reborrow of ONLY the `swap_chain`
                // field; the chain is Some — established above, teardown
                // excluded by the held draw_mutex).
                addr_of_mut!((*self_ptr).swap_chain.as_mut().unwrap().frames[idx])
            };
            let need_resize = unsafe {
                (*frame).target.width != (*self_ptr).size.screen.width as usize
                    || (*frame).target.height != (*self_ptr).size.screen.height as usize
                    || (*frame).target_config_modified != (*self_ptr).target_config_modified
            };
            if need_resize {
                unsafe {
                    (*frame).resize(
                        &(*self_ptr).api,
                        (*self_ptr).size.screen.width as usize,
                        (*self_ptr).size.screen.height as usize,
                    )?;
                    (*frame).target_config_modified = (*self_ptr).target_config_modified;
                }
            }
        }

        // Upload images to the GPU as necessary.
        // _ = self.images.upload(self.alloc, &self.api);
        //   (narrow: &mut self.images + &self.api — disjoint fields.)
        let _ = unsafe {
            (*self_ptr)
                .images
                .upload((*self_ptr).alloc, &(*self_ptr).api)
        };

        // Upload the background image to the GPU as necessary.
        // try self.uploadBackgroundImage();      (whole-&mut self)
        unsafe { Self::upload_background_image_raw(self_ptr)? };

        // Update per-frame custom shader uniforms.
        // try self.updateCustomShaderUniformsForFrame();   (whole-&mut self)
        unsafe { Self::update_custom_shader_uniforms_for_frame_raw(self_ptr) };

        // Re-derive the frame raw root FRESH: the two whole-`&mut self` calls above
        // (`upload_background_image`, `update_custom_shader_uniforms_for_frame`)
        // popped any earlier frame tag. From here to `complete()` there are NO
        // whole-`&mut self` reborrows (only narrow field reborrows + raw
        // `self_ptr`/`frame`), so this root survives to the end.
        let frame: *mut FrameState = unsafe {
            // Narrow projection through the `Option` (a fresh,
            // immediately-discarded reborrow of ONLY the `swap_chain`
            // field; the chain is Some — established above, teardown
            // excluded by the held draw_mutex).
            addr_of_mut!((*self_ptr).swap_chain.as_mut().unwrap().frames[idx])
        };

        // Setup our frame data. The serialization boundary (§2): the ergonomic
        // `crate::shaders` structs are converted to their Metal-ABI POD images.
        // try frame.uniforms.sync(&.{self.uniforms});
        unsafe {
            (*frame)
                .uniforms
                .sync(&[UniformsGpu::from(&(*self_ptr).uniforms)])?;
        }
        // try frame.cells_bg.sync(self.cells.bg_cells);  (CellBg is already ABI)
        unsafe {
            (*frame).cells_bg.sync(&(*self_ptr).cells.bg_cells)?;
        }
        // const fg_count = try frame.cells.syncFromArrayLists(self.cells.fg_rows);
        //
        // PORT NOTE (§6.2, accepted deviation — OPEN RISK #4): the reference's
        // `syncFromArrayLists` is a zero-copy memcpy of the `extern struct`s. Rust's
        // ergonomic `CellText` in-memory layout is NOT the packed-`u8` ABI (byte 29),
        // so we FUSE the row concatenation (order-preserving) with the `CellText ->
        // CellTextGpu` ABI serialization into one transient `Vec` (NOT a GPU-buffer
        // realloc — `sync` still reuses `frame.cells`). The one transient `Vec` per
        // frame deviates from the reference's "keep this around so we don't
        // reallocate" intent; forced by the layout mismatch, fine for the offscreen
        // golden. Documented; the repr(C)-pack-in-place alternative (§2.0) would
        // remove it at the cost of 3b/3c churn.
        let flat: Vec<CellTextGpu> = unsafe {
            (*self_ptr)
                .cells
                .fg_rows
                .iter()
                .flat_map(|row| row.iter().map(CellTextGpu::from))
                .collect()
        };
        unsafe { (*frame).cells.sync(&flat)? };
        let fg_count = flat.len();

        // If our background image buffer has changed, sync it. (Golden: modified
        // stays 0 ⇒ skipped.)
        // if (frame.bg_image_buffer_modified != self.bg_image_buffer_modified) {
        //     try frame.bg_image_buffer.sync(&.{self.bg_image_buffer});
        //     frame.bg_image_buffer_modified = self.bg_image_buffer_modified;
        // }
        if unsafe { (*frame).bg_image_buffer_modified != (*self_ptr).bg_image_buffer_modified } {
            unsafe {
                (*frame)
                    .bg_image_buffer
                    .sync(&[BgImageGpu::from(&(*self_ptr).bg_image_buffer)])?;
                (*frame).bg_image_buffer_modified = (*self_ptr).bg_image_buffer_modified;
            }
        }

        // If our font atlas changed, sync the texture data. Load-bearing: without
        // it the `cell_text` shader samples an empty texture → no glyphs. The
        // `*mut SharedGrid` is a SEPARATE allocation (`&*self.font_grid`); the
        // shared read lock is the reference `lock.lockShared()`. `sync_atlas_texture`
        // takes raw self + raw texture (see its PORT NOTE — the texture aliases into
        // `self.swap_chain`).
        // texture: {
        //     const modified = self.font_grid.atlas_grayscale.modified.load(.monotonic);
        //     if (modified <= frame.grayscale_modified) break :texture;
        //     self.font_grid.lock.lockShared();
        //     defer self.font_grid.lock.unlockShared();
        //     frame.grayscale_modified = self.font_grid.atlas_grayscale.modified.load(.monotonic);
        //     try self.syncAtlasTexture(&self.font_grid.atlas_grayscale, &frame.grayscale);
        // }
        {
            let grid = unsafe { &*(*self_ptr).font_grid };
            let modified = grid.atlas_grayscale().modified();
            if modified > unsafe { (*frame).grayscale_modified } {
                let _guard = grid.lock.read().unwrap();
                unsafe {
                    (*frame).grayscale_modified = grid.atlas_grayscale().modified();
                    Renderer::sync_atlas_texture(
                        self_ptr,
                        grid.atlas_grayscale(),
                        addr_of_mut!((*frame).grayscale),
                    )?;
                }
            }
        }
        // texture: {
        //     const modified = self.font_grid.atlas_color.modified.load(.monotonic);
        //     if (modified <= frame.color_modified) break :texture;
        //     self.font_grid.lock.lockShared();
        //     defer self.font_grid.lock.unlockShared();
        //     frame.color_modified = self.font_grid.atlas_color.modified.load(.monotonic);
        //     try self.syncAtlasTexture(&self.font_grid.atlas_color, &frame.color);
        // }
        {
            let grid = unsafe { &*(*self_ptr).font_grid };
            let modified = grid.atlas_color().modified();
            if modified > unsafe { (*frame).color_modified } {
                let _guard = grid.lock.read().unwrap();
                unsafe {
                    (*frame).color_modified = grid.atlas_color().modified();
                    Renderer::sync_atlas_texture(
                        self_ptr,
                        grid.atlas_color(),
                        addr_of_mut!((*frame).color),
                    )?;
                }
            }
        }

        // Get a frame context from the graphics API.
        // var frame_ctx = try self.api.beginFrame(self, &frame.target);
        // defer frame_ctx.complete(sync);
        //   — N440: in place of `self` and `&frame.target`, the completion context
        //     carries clones of what the completion handler needs (the health
        //     cell, the frame semaphore, the surface mailbox, Metal's layer cell,
        //     the target's IOSurface), taken here under the draw mutex through
        //     narrow places; the completion fn is the trampoline. `begin_frame`
        //     is `&self.api` (narrow).
        let completion = unsafe {
            CompletionContext {
                health: (*self_ptr).health.clone(),
                frame_sema,
                surface_mailbox: (*self_ptr).surface_mailbox.clone(),
                health_report: (*self_ptr).health_report.clone(),
                layer: (*self_ptr).api.presenter(),
                surface: (*frame).target.surface.clone(),
                sync: false,
            }
        };
        let mut frame_ctx: Frame = unsafe {
            (*self_ptr)
                .api
                .begin_frame(completion, metal_buffer_completed)
        }?;

        {
            // var pass = frame_ctx.renderPass(&.{.{
            //     .target = if (frame.custom_shader_state) |state|
            //         .{ .texture = state.back_texture } else .{ .target = frame.target },
            //     .clear_color = .{ 0.0, 0.0, 0.0, 0.0 },
            // }});
            // defer pass.complete();
            let attach_target = match unsafe { (*frame).custom_shader_state.as_ref() } {
                Some(state) => AttachmentTarget::Texture(&state.back_texture),
                None => AttachmentTarget::Target(unsafe { &(*frame).target }),
            };
            let pass = frame_ctx.render_pass(&[Attachment {
                target: attach_target,
                clear_color: Some([0.0, 0.0, 0.0, 0.0]),
            }]);

            // First we draw our background image, if we have one.
            // The bg image shader also draws the main bg color.
            //
            // Otherwise, if we don't have a background image, we
            // draw the background color by itself in its own step.
            //
            // NOTE: We don't use the clear_color for this because that
            //       would require us to do color space conversion on the
            //       CPU-side. In the future when we have utilities for
            //       that we should remove this step and use clear_color.
            // if (self.bg_image) |img| switch (img) {
            //     .ready => |texture| pass.step(.{ .pipeline = bg_image, ... }),
            //     else => {},
            // } else { pass.step(.{ .pipeline = bg_color, ... }); }
            match unsafe { (*self_ptr).bg_image.as_ref() } {
                Some(image::Image::Ready(texture)) => pass.step(Step {
                    pipeline: unsafe { &(&(*self_ptr).shaders.pipelines).bg_image },
                    uniforms: Some(unsafe { (*frame).uniforms.raw() }),
                    buffers: &[Some(unsafe { (*frame).bg_image_buffer.raw() })],
                    textures: &[Some(texture)],
                    samplers: &[],
                    draw: Draw {
                        kind: MTLPrimitiveType::Triangle,
                        vertex_count: 3,
                        instance_count: 1,
                    },
                }),
                Some(_) => {}
                None => {
                    pass.step(Step {
                        pipeline: unsafe { &(&(*self_ptr).shaders.pipelines).bg_color },
                        uniforms: Some(unsafe { (*frame).uniforms.raw() }),
                        buffers: &[None, Some(unsafe { (*frame).cells_bg.raw() })],
                        textures: &[],
                        samplers: &[],
                        draw: Draw {
                            kind: MTLPrimitiveType::Triangle,
                            vertex_count: 3,
                            instance_count: 1,
                        },
                    });
                }
            }

            // Then we draw any kitty images that need
            // to be behind text AND cell backgrounds.
            // self.images.draw(&self.api, self.shaders.pipelines.image, &pass, .kitty_below_bg);
            unsafe {
                (*self_ptr).images.draw(
                    &(*self_ptr).api,
                    &(&(*self_ptr).shaders.pipelines).image,
                    &pass,
                    DrawPlacements::KittyBelowBg,
                );
            }

            // Then we draw any opaque cell backgrounds.
            // pass.step(.{ .pipeline = cell_bg, .uniforms, .buffers = &.{ null, frame.cells_bg.buffer }, .draw = {triangle, 3} });
            pass.step(Step {
                pipeline: unsafe { &(&(*self_ptr).shaders.pipelines).cell_bg },
                uniforms: Some(unsafe { (*frame).uniforms.raw() }),
                buffers: &[None, Some(unsafe { (*frame).cells_bg.raw() })],
                textures: &[],
                samplers: &[],
                draw: Draw {
                    kind: MTLPrimitiveType::Triangle,
                    vertex_count: 3,
                    instance_count: 1,
                },
            });

            // Kitty images between cell backgrounds and text.
            // self.images.draw(..., .kitty_below_text);
            unsafe {
                (*self_ptr).images.draw(
                    &(*self_ptr).api,
                    &(&(*self_ptr).shaders.pipelines).image,
                    &pass,
                    DrawPlacements::KittyBelowText,
                );
            }

            // Text.
            // pass.step(.{ .pipeline = cell_text, .uniforms,
            //     .buffers = &.{ frame.cells.buffer, frame.cells_bg.buffer },
            //     .textures = &.{ frame.grayscale, frame.color },
            //     .draw = .{ .type = .triangle_strip, .vertex_count = 4, .instance_count = fg_count } });
            pass.step(Step {
                pipeline: unsafe { &(&(*self_ptr).shaders.pipelines).cell_text },
                uniforms: Some(unsafe { (*frame).uniforms.raw() }),
                buffers: &[
                    Some(unsafe { (*frame).cells.raw() }),
                    Some(unsafe { (*frame).cells_bg.raw() }),
                ],
                textures: &[
                    Some(unsafe { &(*frame).grayscale }),
                    Some(unsafe { &(*frame).color }),
                ],
                samplers: &[],
                draw: Draw {
                    kind: MTLPrimitiveType::TriangleStrip,
                    vertex_count: 4,
                    instance_count: fg_count,
                },
            });

            // Kitty images in front of text.
            // self.images.draw(..., .kitty_above_text);
            unsafe {
                (*self_ptr).images.draw(
                    &(*self_ptr).api,
                    &(&(*self_ptr).shaders.pipelines).image,
                    &pass,
                    DrawPlacements::KittyAboveText,
                );
            }

            // Debug overlay. We do this before any custom shader state
            // because our debug overlay is aligned with the grid.
            // if (self.overlay != null) self.images.draw(..., .overlay);
            if unsafe { (*self_ptr).overlay.is_some() } {
                unsafe {
                    (*self_ptr).images.draw(
                        &(*self_ptr).api,
                        &(&(*self_ptr).shaders.pipelines).image,
                        &pass,
                        DrawPlacements::Overlay,
                    );
                }
            }

            // defer pass.complete();
            pass.complete();
        }

        // If we have custom shaders, then we render them. (Golden:
        // custom_shader_state=None ⇒ skipped.)
        // if (frame.custom_shader_state) |*state| {
        if unsafe { (*frame).custom_shader_state.is_some() } {
            let state: *mut CustomShaderState =
                unsafe { (*frame).custom_shader_state.as_mut().unwrap() };
            // Sync our uniforms.
            // try state.uniforms.sync(&.{self.custom_shader_uniforms});
            //   (CustomShaderState.uniforms stays `Buffer<shadertoy::Uniforms>` — no
            //    ABI serializer; the ergonomic shadertoy uniforms sync directly.)
            unsafe {
                (*state)
                    .uniforms
                    .sync(&[(*self_ptr).custom_shader_uniforms])?;
            }

            // for (self.shaders.post_pipelines, 0..) |pipeline, i| {
            //   (the slice ref is hoisted so `[i]` does not autoref through the raw
            //    `self_ptr` — a narrow shared reborrow of `self.shaders.post_pipelines`,
            //    disjoint from the `state`/`frame` the loop touches.)
            let post_pipelines: &[crate::metal::pipeline::Pipeline] =
                unsafe { &(*self_ptr).shaders.post_pipelines };
            let n = post_pipelines.len();
            for (i, pipeline) in post_pipelines.iter().enumerate() {
                // var pass = frame_ctx.renderPass(&.{.{
                //     .target = if (i < post_pipelines.len - 1)
                //         .{ .texture = state.front_texture } else .{ .target = frame.target },
                //     .clear_color = .{ 0.0, 0.0, 0.0, 0.0 },
                // }});
                // defer pass.complete();
                let target = if i < n - 1 {
                    AttachmentTarget::Texture(unsafe { &(*state).front_texture })
                } else {
                    AttachmentTarget::Target(unsafe { &(*frame).target })
                };
                let pass = frame_ctx.render_pass(&[Attachment {
                    target,
                    clear_color: Some([0.0, 0.0, 0.0, 0.0]),
                }]);

                // pass.step(.{ .pipeline = pipeline, .uniforms = state.uniforms.buffer,
                //     .textures = &.{state.back_texture}, .samplers = &.{state.sampler},
                //     .draw = .{ .type = .triangle, .vertex_count = 3 } });
                pass.step(Step {
                    pipeline,
                    uniforms: Some(unsafe { (*state).uniforms.raw() }),
                    buffers: &[],
                    textures: &[Some(unsafe { &(*state).back_texture })],
                    samplers: &[Some(unsafe { &(*state).sampler })],
                    draw: Draw {
                        kind: MTLPrimitiveType::Triangle,
                        vertex_count: 3,
                        instance_count: 1,
                    },
                });
                // defer pass.complete();
                pass.complete();
                // defer state.swap();
                unsafe { (*state).swap() };
            }
        }

        // Disarm the errdefer: from here we always reach `frame_ctx.complete`, whose
        // trampoline calls `frame_completed` → `release_frame` (releasing the
        // semaphore). Leaving the guard armed would double-release.
        release_guard.armed = false;

        // defer frame_ctx.complete(sync);  — the reference's LAST-registered defer;
        // on the sync path it commits → waitUntilCompleted → invokes
        // `metal_buffer_completed` directly (H2). `frame` is NOT used after this.
        frame_ctx.complete(sync);

        // PORT NOTE: reference defect (R13), UD362. While a health report is
        // owed (`HealthReport`), a render-thread draw asks for the display-link
        // resync too, so the link runs (its `should_run` sees the owed report as
        // an animation wake) and paces the retries at vsync; the no-redraw draw
        // after the report lands parks it again. Never on a sync draw, which
        // must not touch the display link.
        Ok(!sync && unsafe { (*self_ptr).health_report.pending() })
        // The two RAII guards drop here in the reference LIFO order:
        // draw_frame_end → scrollbar; the caller then unlocks.
    }

    /// `&mut self` convenience wrapper over [`draw_frame_raw`]. Sound **only** for
    /// EXCLUSIVE single-threaded callers — i.e. the offscreen unit tests, where no
    /// display link / render thread / CA `display` can fire concurrently. Every
    /// concurrent production path uses `draw_frame_raw`; no `&mut self` draw entry
    /// is reachable from the render thread or the main-thread present path
    /// (`W5_DESIGN §2.5`). Adding a concurrent caller of THIS wrapper reintroduces
    /// the protected-`&mut` UB the split exists to prevent — the `displaycb_naive`
    /// mirihar control fails for exactly that shape.
    pub fn draw_frame(&mut self, sync: bool) -> Result<(), Error> {
        let mutex = self.draw_mutex.clone();
        let self_ptr: *mut Renderer = self;
        unsafe { Self::draw_frame_raw(self_ptr, &mutex, sync) }
    }
}
