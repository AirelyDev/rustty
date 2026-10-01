//! PORTS: ghostty src/renderer/generic.zig — Chunk 4, the draw path (`drawFrame`,
//! `drawFrameLocked`, `frameCompleted`, `initShaders`/`deinitShaders`,
//! `syncAtlasTexture`, `uploadBackgroundImage`, `animationWake`), plus the OpenGL
//! `Frame.zig` completion tail and the port-only `DeferredSurfacePushes`.
//!
//! This file is part of the `Renderer(OpenGL)` arm (`generic::linux_impl`).

use super::*;

/// The `Frame` completion tail (`opengl::frame::CompletionFn`) — the OpenGL twin of
/// `macos_impl::metal_buffer_completed` (generic.rs). `Frame::complete` computes
/// `health` from `gl.errors.getError()` and hands it in, so — unlike Metal, which
/// inspects an `MTLCommandBuffer` here — this runs the reference's tail from the
/// present on (`opengl/Frame.zig:62-89` at `12752b2ac`, `2f0b65346`):
///
/// ```zig
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
//
// The log site's oracle is `renderer/opengl/Frame.zig`, whose scope is `.opengl`
// (Frame.zig:15), NOT `generic.zig`'s `.generic_renderer` — so it targets
// `crate::opengl::opengl_impl::LOG_SCOPE` even though the code lives here, exactly
// as the Metal twin targets `metal::frame::LOG_SCOPE`. It lives here at all because
// the port inlines `Frame.zig`'s completion tail into this trampoline.
//
// PORT NOTE (raw-ptr re-entry, `rust-mut-args-strongly-protected`): the renderer is
// recovered as a RAW `*mut Renderer` (never `&mut`) and deref'd HERE — inside
// `Frame::complete`, which runs inside `draw_frame_raw` while its `self_ptr` raw root
// is live. `present(&self)`, `push_frame(&self)`, the mailbox push and
// `frame_completed(&self)` each reborrow only `&` (narrow). The whole-renderer
// `&self` they form is sound because OpenGL frames complete synchronously, here on
// the render thread with `draw_mutex` held, so no other thread touches the
// renderer meanwhile (N437: every other entry locks first).
//
// # Safety
// `renderer` must be a live, whole-`Renderer`-provenance `*mut ()` (the `self_ptr`
// `begin_frame` was handed); `target` a live `*mut Target` into that renderer's frame;
// called on the render thread inside `draw_frame_raw`, with `draw_mutex` held.
unsafe fn opengl_frame_completed(renderer: *mut (), target: *mut Target, health: crate::Health) {
    let renderer = renderer as *mut Renderer;

    // If the frame is healthy, draw it to an ExportedFrame
    // Then sync before exporting and pushing to the present queue.
    // The apprt pulls from this queue in its snapshot handler.
    let presented_frame: Option<ExportedFrame> = if health == crate::Health::Healthy {
        let presentation_health = unsafe { (*renderer).presentation_health() };
        match unsafe { (*renderer).api.present(&*target, presentation_health) } {
            Ok(frame) => Some(frame),
            Err(err) => {
                // log.warn("failed to present render target: err={}", .{err});
                log::warn!(
                    target: crate::opengl::opengl_impl::LOG_SCOPE,
                    "failed to present render target: err={err:?}"
                );
                None
            }
        }
    } else {
        None
    };

    // Sync after frame draw AND frame present GL calls.
    // gl.finish();
    crate::opengl::gl::finish();

    // At this point the ExportedFrame is finished and can be shared
    if let Some(frame) = presented_frame {
        // self.renderer.pushFrame(frame);
        unsafe { (*renderer).push_frame(frame) };

        // Notify the surface that it should redraw
        // _ = self.renderer.surface_mailbox.push(.redraw, .{ .forever = {} });
        //
        // PORT NOTE: reference defect (R13), UD362. This runs with the draw
        // mutex held; the `.forever` push is deferred until `draw_frame_raw`
        // releases it (`deferred_surface_pushes`).
        unsafe { (*renderer).defer_forever_push(SurfaceMessage::Redraw) };
    }

    // Report the health to the renderer.
    // self.renderer.frameCompleted(health);
    unsafe { (*renderer).frame_completed(health) };
}

/// `.forever` surface-mailbox pushes taken under the draw mutex, sent after it
/// is released (UD362).
pub struct DeferredSurfacePushes {
    mailbox: SurfaceMailbox,
    msgs: Vec<SurfaceMessage>,
}

impl DeferredSurfacePushes {
    /// Send every queued message with `.forever`, in order.
    pub fn push_all(self) {
        for msg in self.msgs {
            let _ = self.mailbox.push(msg, MailboxTimeout::Forever);
        }
    }
}

impl Renderer {
    /// The soonest animation wake this renderer needs, if any:
    /// custom shader animation wants continuous draw-only wakes
    /// at draw_interval_ms while active, and a running Kitty
    /// graphics animation wants an update wake when its next
    /// frame is due. The renderer thread drives its animation
    /// timer off this, re-querying after every wake.
    ///
    /// Must be called on the render thread.
    //
    // PORTS: `animationWake` (`aee7bf347`, REPLACES `hasAnimations`); the macos
    // twin carries the verbatim Zig quote — API-free code, copied verbatim.
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
}

impl Renderer {
    // PORTS: generic.zig:836-... `initShaders`; macos_impl generic.rs:5231-5272.
    pub(super) fn init_shaders(&mut self) -> Result<(), Error> {
        // Load our custom shaders. The reference arena is subsumed by owned
        // `Vec<CString>`; the `catch |err| break :err &.{}` (any load error ⇒ no
        // custom shaders, never block the renderer) maps to `unwrap_or_else`.
        let custom_shaders: Vec<CString> =
            shadertoy::load_from_files(&self.config.custom_shaders, OpenGL::CUSTOM_SHADER_TARGET)
                .unwrap_or_else(|err| {
                    // log.warn("error loading custom shaders err={}", .{err});
                    log::warn!(target: LOG_SCOPE, "error loading custom shaders err={err:?}");
                    Vec::new()
                });

        let custom_refs: Vec<&CStr> = custom_shaders.iter().map(|c| c.as_c_str()).collect();
        let has_custom_shaders = !custom_refs.is_empty();

        // var shaders = try self.api.initShaders(...); self.shaders = shaders;
        // The old `Shaders` value is replaced; its GL pipelines are freed
        // EXPLICITLY (manual GL delete — `Shaders` has no `Drop`) before the
        // reassignment, matching the reference's `errdefer shaders.deinit()` +
        // replace model.
        let new_shaders = self.api.init_shaders(&custom_refs)?;
        let mut old = mem::replace(&mut self.shaders, new_shaders);
        old.deinit();
        self.has_custom_shaders = has_custom_shaders;
        Ok(())
    }

    // PORTS: generic.zig:832-834 `deinitShaders`; macos_impl generic.rs:5216-5218.
    pub fn deinit_shaders(&mut self) {
        self.shaders.deinit();
    }
}

impl Renderer {
    /// Sync the atlas data to the given texture. This copies the bytes associated with
    /// the atlas to the given texture. If the atlas no longer fits into the texture,
    /// the texture will be resized.
    //
    // PORTS: generic.zig:1234-1257 `syncAtlasTexture`; macos_impl generic.rs:5291-5317.
    //
    // PORT NOTE (raw-ptr — the same MIRIHAR-proven shape as the Metal twin): the
    // reference is `syncAtlasTexture(self: *const Self, atlas: *const font.Atlas,
    // texture: *Texture)` — `self` and `texture` are RAW pointers. A `&self`/`&mut
    // texture` form is unsound because `texture` is `&mut frame.grayscale`, which
    // aliases INTO `self.swap_chain`. Ported as raw self + raw texture: it reborrows
    // only `self.api` (narrow, disjoint from `swap_chain`) and writes `*texture`.
    // `atlas` stays a real `&FontAtlas` — it lives in the `*mut SharedGrid` (a SEPARATE
    // allocation).
    //
    // DIVERGENCE from `macos_impl` (Metal `Texture` is ARC-`Retained`, dropped on
    // reassign): the OpenGL `Texture` has a MANUAL `deinit(self)` and no `Drop`, so the
    // old texture is freed EXPLICITLY before the reassignment (Zig `texture.*.deinit()`,
    // generic.zig:1240) rather than by an implicit ARC release.
    //
    // # Safety
    // `self_raw`/`texture` must be valid, well-aligned, whole-provenance pointers that
    // outlive the call; `texture` must not alias `atlas`.
    unsafe fn sync_atlas_texture(
        self_raw: *const Renderer,
        atlas: &FontAtlas,
        texture: *mut Texture,
    ) -> Result<(), Error> {
        // if (atlas.size > texture.width) {
        if atlas.size() as usize > unsafe { (*texture).width } {
            // Free our old texture / Reallocate.
            // texture.*.deinit();  texture.* = try self.api.initAtlasTexture(atlas);
            let new_texture = unsafe { (*self_raw).api.init_atlas_texture(atlas)? };
            // `mem::replace` + explicit `deinit` of the old value (manual GL delete —
            // the OpenGL leaf has no `Drop`, unlike Metal's ARC release).
            let old = unsafe { mem::replace(&mut *texture, new_texture) };
            old.deinit();
        }

        // try texture.replaceRegion(0, 0, atlas.size, atlas.size, atlas.data);
        unsafe {
            (*texture).replace_region(
                0,
                0,
                atlas.size() as usize,
                atlas.size() as usize,
                atlas.data(),
            )?;
        }
        Ok(())
    }

    // PORTS: generic.zig:1832-1846 `uploadBackgroundImage`; macos_impl
    // generic.rs:5561-5581.
    //
    // GL: `Image::deinit`/`Image::upload` touch real GL objects
    // (`glDeleteTextures`/`glTexImage2D`), so this must run with the owned context
    // current — it does: the sole caller is `draw_frame_raw`.
    fn upload_background_image(&mut self) -> Result<(), Error> {
        // Make sure our bg image is uploaded if it needs to be.
        // if (self.bg_image) |*bg| {
        if let Some(bg) = self.bg_image.as_mut() {
            // if (bg.isUnloading()) { bg.deinit(self.alloc); self.bg_image = null; return; }
            if bg.is_unloading() {
                // `Image::deinit` consumes `self` by value (the reference
                // `bg.deinit(self.alloc)`), so take the image OUT of the Option to
                // move it into `deinit`; then the Option is left `None`.
                let img = self.bg_image.take().unwrap();
                img.deinit(self.alloc);
                return Ok(());
            }
            // if (bg.isPending()) try bg.upload(self.alloc, &self.api);
            //   (`bg` borrows the `self.bg_image` field; `self.api` is a DISJOINT
            //    field, so the shared borrow is fine — same shape as the Metal twin.)
            if bg.is_pending() {
                bg.upload(self.alloc, &self.api)
                    .map_err(|_| Error::GlFailed)?;
            }
        }
        Ok(())
    }

    /// Callback from the graphics API when a frame is completed.
    //
    // PORTS: generic.zig:1259-1276 `frameCompleted`; macos_impl generic.rs:5351-5372.
    // `&self` narrowing (faithful): every op is `&self`-safe — `health.store`
    // (`AtomicHealth`), `surface_mailbox.push` (`&self` — the sink is behind an
    // `Arc`), and `swap_chain.release_frame`
    // (posts the semaphore).
    /// Queue a `.forever` surface-mailbox push until the draw mutex is
    /// released (UD362).
    pub fn defer_forever_push(&self, msg: SurfaceMessage) {
        self.deferred_surface_pushes.borrow_mut().push(msg);
    }

    /// Take the queued `.forever` pushes (under the draw mutex) together with a
    /// handle to the mailbox, so the caller can send them after the unlock
    /// without touching the renderer (UD362, N437).
    pub fn take_deferred_surface_pushes(&self) -> DeferredSurfacePushes {
        DeferredSurfacePushes {
            mailbox: self.surface_mailbox.clone(),
            msgs: core::mem::take(&mut *self.deferred_surface_pushes.borrow_mut()),
        }
    }

    pub fn frame_completed(&self, health: crate::Health) {
        // If our health value hasn't changed, then we do nothing.
        // if (self.health.load(.seq_cst) != health) {
        if self.health.load() != health {
            // self.health.store(health, .seq_cst);
            self.health.store(health);

            // Notify the surface so it can react (REAL — seam §B.2 resolved: core's
            // sink re-wraps this into `apprt::surface::Message::RendererHealth`; a
            // DETACHED offscreen mailbox still returns 0).
            // _ = self.surface_mailbox.push(.{ .renderer_health = health }, .{ .forever = {} });
            //
            // PORT NOTE: reference defect (R13), UD362. On OpenGL this runs
            // inside `drawFrameLocked` (the frame completes synchronously), so
            // the `.forever` push is deferred until the draw mutex is released.
            self.defer_forever_push(SurfaceMessage::RendererHealth(health));
        }

        // Always release our semaphore. The swap chain is
        // guaranteed to exist here: it is only torn down after
        // waiting for all in-flight frames to complete, and this
        // callback is what signals that completion.
        // self.swap_chain.?.releaseFrame();
        self.swap_chain.as_ref().unwrap().release_frame();
    }

    /// Draw the frame to the screen.
    ///
    /// If `sync` is true, this will synchronously block until the frame is finished
    /// drawing and has been presented. (For OpenGL `Frame::complete` ALWAYS blocks via
    /// `gl.finish()` regardless of `sync` — `opengl/Frame.zig:50-57`.)
    //
    // PORTS: generic.zig:1278-1466 `drawFrame`; macos_impl generic.rs:5408-6022.
    //
    // NO SCOPE CUTS REMAIN in this body (the L4 plan's Global-Constraints elisions are
    // all resolved). The encode path is the reference's, in the reference's order:
    // begin_frame → one render_pass { bg_image-or-bg_color, kitty_below_bg, cell_bg,
    // kitty_below_text, cell_text, kitty_above_text, overlay } → complete.
    //   * KITTY-IMAGE bands + the image upload sweep: w2 F-005 (`crate::image` is a
    //     twin pair now, so `images.upload`/`images.draw` run for real).
    //   * BACKGROUND IMAGE: w2 F-012 — `upload_background_image` runs before the pass
    //     and the `if (self.bg_image) |img| switch (img) { .ready => … } else { … }`
    //     arm replaces the unconditional `bg_color` step (generic.zig:1611-1628).
    //   * DEBUG OVERLAY: w2 F-020 — the guarded `images.draw(…, .overlay)` band
    //     (generic.zig:1681-1688) runs whenever `update_frame` built an overlay.
    //   * CUSTOM SHADERS (post-process): F-013 — when `has_custom_shaders` is set the
    //     base pass retargets to the custom-shader state's `back_texture` and the
    //     `shaders.post_pipelines` ping-pong chain runs afterwards, exactly as
    //     generic.zig:1691-1719.
    //
    // PORT NOTE (raw-ptr discipline, identical to the Metal twin / MIRIHAR-proven): the
    // whole body drives `self` through the SINGLE raw root `self_ptr` with NARROW field
    // reborrows, and re-derives the `frame` raw root FRESH via
    // `addr_of_mut!((*self_ptr).swap_chain.frames[idx])` after every whole-`&mut self`
    // call, because a whole `&mut *self_ptr` reborrow POPS a held `frame` tag.
    //
    // P7-W7 T2: `drawFrame` is split into its locked body, `drawFrameLocked`
    // (`72cf50855`, P7-W0 T1 on the Metal twin), so `syncDisplayLink` runs after
    // the mutex is released. Off macOS `syncDisplayLink` returns at once
    // (`DisplayLink == void`), so the split only changes where the lock ends.
    //
    // N437: `lock` is the caller's own `draw_mutex` handle (`crate::latest_frame`),
    // taken before the render thread started; the body forms references to the
    // renderer only while it is held.
    //
    /// # Safety
    /// `self_ptr` must be a valid, well-aligned, whole-`Renderer`-provenance pointer
    /// that outlives the call, and `lock` a handle to its `draw_mutex`.
    pub unsafe fn draw_frame_raw(
        self_ptr: *mut Renderer,
        lock: &DrawMutex,
        sync: bool,
    ) -> Result<(), Error> {
        // Everything that touches draw state happens under the draw
        // mutex. The display link is synced only after the mutex is
        // released; see `syncDisplayLink` for why it must never be
        // called with the draw mutex held.
        // const sync_display_link = locked: {
        //     self.draw_mutex.lockUncancelable(global.io());
        //     defer self.draw_mutex.unlock(global.io());
        //     break :locked try self.drawFrameLocked(sync);
        // };
        let (sync_display_link, deferred) = {
            lock.lock();
            struct UnlockGuard<'a>(&'a DrawMutex);
            impl Drop for UnlockGuard<'_> {
                fn drop(&mut self) {
                    self.0.unlock();
                }
            }
            let _unlock = UnlockGuard(lock);
            let result = unsafe { Self::draw_frame_locked(self_ptr, sync) };
            // UD362: take the frame completion's `.forever` pushes while the
            // lock is held; they are sent below, after the unlock.
            (result, unsafe {
                (*self_ptr).take_deferred_surface_pushes()
            })
        };

        // PORT NOTE: reference defect (R13), UD362. The `.redraw` and
        // `.renderer_health` pushes the reference makes inside the locked
        // frame completion go out here, with the draw mutex released, so a
        // full app mailbox cannot block the render thread while it holds the
        // mutex the apprt main thread (the mailbox's consumer) needs.
        deferred.push_all();
        let sync_display_link = sync_display_link?;

        // if (sync_display_link) self.syncDisplayLink(null, null);
        // (`if (comptime DisplayLink == void) return;` — nothing to sync off macOS.)
        let _ = sync_display_link;
        Ok(())
    }

    /// The body of `drawFrame`. Must be called with `draw_mutex` held.
    ///
    /// Returns true if the display link should be resynced once the
    /// draw mutex is released. This is only ever true on the no-redraw
    /// path, which a sync draw never takes, so the main thread's sync
    /// draws never touch the display link and `syncDisplayLink` stays
    /// on the render thread.
    //
    // generic.zig:1695-2003 `drawFrameLocked` (`72cf50855`, `2f0b65346`).
    //
    /// # Safety
    /// As [`draw_frame_raw`](Self::draw_frame_raw), and the caller holds
    /// `draw_mutex` for the whole call.
    // PORT NOTE: `&(&(*self_ptr).shaders.pipelines).x` spells out the reference the
    // pipelines `Undefined` derefs through (the rustc lint `dangerous_implicit_autorefs` rejects
    // the implicit one on a raw-pointer place); clippy calls that borrow needless.
    #[allow(clippy::needless_borrow)]
    unsafe fn draw_frame_locked(self_ptr: *mut Renderer, sync: bool) -> Result<bool, Error> {
        // Hand-rolled `defer`s as RAII guards (each holds the raw root, cleans up via a
        // NARROW field reborrow), dropping LIFO in the reference order:
        // draw_frame_end → scrollbar.
        struct ScrollbarGuard(*mut Renderer);
        impl Drop for ScrollbarGuard {
            fn drop(&mut self) {
                // defer if (self.scrollbar_dirty) { if (self.surface_mailbox.push(...,
                //     .instant) > 0) self.scrollbar_dirty = false; };
                // (REAL — seam §B.2 resolved: core's sink enqueues
                //  `apprt::surface::Message::Scrollbar`. A DETACHED mailbox — the
                //  offscreen golden/unit renderers — still returns 0, so the branch
                //  is never taken offscreen.)
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
        // errdefer-only guard: releases the swap-chain semaphore ONLY on an early error
        // return after `next_frame`. Disarmed on the success path (the frame's
        // `frameCompleted` releases instead — else a double-release).
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

        // self.api.drawFrameStart(); defer self.api.drawFrameEnd();
        unsafe { (*self_ptr).api.draw_frame_start() };
        let _draw_end = DrawEndGuard(self_ptr);

        // const surface_size = try self.api.surfaceSize();
        let surface_size = unsafe { (*self_ptr).api.surface_size() }?;

        // If either dimension is zero, drawing is absurd — return.
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
        // (raw-ptr discipline: instead of the reference's held `*SwapChain`
        // local, the chain is re-derived FRESH from `self_ptr` at every use
        // below — the same rule as `frame`; the macos twin carries the verbatim
        // Zig quote.)
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

        // Conditions under which we need to draw the frame.
        //
        // While any animation is in progress (a pending animation wake)
        // every draw must actually render.
        let needs_redraw = size_changed
            || swap_chain_rebuilt
            || unsafe { (*self_ptr).cells_rebuilt }
            || unsafe { (*self_ptr).animation_wake().is_some() }
            || sync;

        if !needs_redraw {
            // Ask our caller to resync the display link once the draw
            // mutex is released, because we can probably pause the
            // display link at this point.
            //
            // (`2f0b65346` deleted the `try self.api.presentLastTarget()` that
            // preceded this: the apprt keeps displaying the last exported frame.)
            return Ok(true);
        }
        // self.cells_rebuilt = false;
        unsafe { (*self_ptr).cells_rebuilt = false };

        // const frame = swap_chain.nextFrame(); errdefer swap_chain.releaseFrame();
        // (called for its side effects; the returned `&mut FrameState` is dropped
        // immediately and `frame` is re-derived fresh from `self_ptr` at each use.
        // The chain is Some here — established/rebuilt above; teardown is
        // excluded by the held draw_mutex.)
        let _ = unsafe { (*self_ptr).swap_chain.as_mut().unwrap().next_frame() };
        let mut release_guard = ReleaseGuard {
            r: self_ptr,
            armed: true,
        };
        let idx = unsafe { (*self_ptr).swap_chain.as_ref().unwrap().frame_index as usize };

        // If we need to reinitialize our shaders, do so.
        // if (self.reinitialize_shaders) { self.reinitialize_shaders = false;
        //     self.shaders.deinit(); try self.initShaders(); }
        if unsafe { (*self_ptr).reinitialize_shaders } {
            unsafe {
                (*self_ptr).reinitialize_shaders = false;
                // `drawFrame` deinits `self.shaders` DIRECTLY (generic.zig:1500), NOT via
                // `self.deinit_shaders()`.
                (*self_ptr).shaders.deinit();
                (*self_ptr).init_shaders()?; // whole-&mut self
            }
        }

        // Our shaders should not be defunct at this point.
        // assert(!self.shaders.defunct);
        assert!(!unsafe { (*self_ptr).shaders.defunct });

        // If we have custom shaders, make sure we have the
        // custom shader state in our frame state, otherwise
        // if we have a state but don't need it we remove it.
        // PORTS: generic.zig:1305-1324; macos_impl generic.rs:5588-5621.
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
            //
            // DIVERGENCE from `macos_impl` (which relies on field `Drop`): the OpenGL
            // leaf resources have a manual `deinit(self)` and no `Drop`, so the state
            // is `take`n and deinited EXPLICITLY (a bare `= None` would leak the GL
            // objects), matching `FrameState::deinit`'s model.
            else if let Some(state) = unsafe { (*frame).custom_shader_state.take() } {
                state.deinit();
            }
        }

        // If our stored size doesn't match the surface size we need to update it.
        // (Golden: size_changed=false ⇒ skipped. `update_screen_size_uniforms` is a
        //  whole-&mut self call, so no `frame` raw root is held across it.)
        if size_changed {
            unsafe {
                (*self_ptr).size.screen = ScreenSize {
                    width: surface_size.width,
                    height: surface_size.height,
                };
                (*self_ptr).update_screen_size_uniforms();
            }
        }

        // If this frame's target isn't the correct size / config, replace it.
        // if (frame.target.width != self.size.screen.width or … or
        //     frame.target_config_modified != self.target_config_modified) { … }
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
        // _ = self.images.upload(self.alloc, &self.api);   (generic.zig:1548-1549)
        //   (narrow: &mut self.images + &self.api — disjoint fields.)
        //   macos_impl generic.rs:5899-5906.
        //
        // GL: this is where pending Kitty images become real `glTexImage2D` uploads
        // AND where unloaded ones are evicted (`glDeleteTextures`). Both need the
        // context current — it is, we are inside `draw_frame_raw`.
        let _ = unsafe {
            (*self_ptr)
                .images
                .upload((*self_ptr).alloc, &(*self_ptr).api)
        };

        // Make sure our background image is uploaded if it needs to be.
        // try self.uploadBackgroundImage();   (generic.zig:1551-1552)
        //   (whole-`&mut self` — macos_impl generic.rs:5911.)
        //
        // GL: this is where a pending bg image becomes a real `glTexImage2D` upload
        // and where an unloaded one is freed (`glDeleteTextures`); both need the
        // context current — it is, we are inside `draw_frame_raw`.
        unsafe { (*self_ptr).upload_background_image()? };

        // Update per-frame custom shader uniforms.
        // try self.updateCustomShaderUniformsForFrame();   (whole-&mut self)
        unsafe { (*self_ptr).update_custom_shader_uniforms_for_frame() };

        // Re-derive the frame raw root FRESH: the two whole-`&mut self` calls above
        // (`upload_background_image`, `update_custom_shader_uniforms_for_frame`)
        // popped any earlier frame tag.
        // (No whole-`&mut self` reborrows occur from here to `complete()`, so this
        // root survives to the end.)
        let frame: *mut FrameState = unsafe {
            // Narrow projection through the `Option` (a fresh,
            // immediately-discarded reborrow of ONLY the `swap_chain`
            // field; the chain is Some — established above, teardown
            // excluded by the held draw_mutex).
            addr_of_mut!((*self_ptr).swap_chain.as_mut().unwrap().frames[idx])
        };

        // Setup our frame data (the serialization boundary: ergonomic `crate::shaders`
        // structs → their OpenGL-ABI POD images via `opengl::gpu_abi`'s `From`).
        // try frame.uniforms.sync(&.{self.uniforms});
        unsafe {
            (*frame)
                .uniforms
                .sync(&[UniformsGpu::from(&(*self_ptr).uniforms)])?;
        }
        // try frame.cells_bg.sync(self.cells.bg_cells);  (CellBg is already ABI = [u8;4])
        unsafe {
            (*frame).cells_bg.sync(&(*self_ptr).cells.bg_cells)?;
        }
        // const fg_count = try frame.cells.syncFromArrayLists(self.cells.fg_rows);
        //
        // PORT NOTE (accepted deviation, same as the Metal twin): the reference's
        // `syncFromArrayLists` is a zero-copy memcpy of the `extern struct`s. Rust's
        // ergonomic `CellText` is NOT the packed-`u8` ABI (byte 29), so we FUSE the row
        // concatenation (order-preserving) with the `CellText → CellTextGpu` ABI
        // serialization into one transient `Vec`, then `sync` it (still reuses
        // `frame.cells`).
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

        // If our background image buffer has changed, sync it. (Golden: modified stays 0
        // ⇒ skipped — the `bg_image_buffer` BUFFER is kept for parity even though the
        // bg-image DRAW is cut.)
        // if (frame.bg_image_buffer_modified != self.bg_image_buffer_modified) { … }
        if unsafe { (*frame).bg_image_buffer_modified != (*self_ptr).bg_image_buffer_modified } {
            unsafe {
                (*frame)
                    .bg_image_buffer
                    .sync(&[BgImageGpu::from(&(*self_ptr).bg_image_buffer)])?;
                (*frame).bg_image_buffer_modified = (*self_ptr).bg_image_buffer_modified;
            }
        }

        // If our font atlas changed, sync the texture data. Load-bearing: without it the
        // `cell_text` shader samples an empty texture → no glyphs. The `*mut SharedGrid`
        // is a SEPARATE allocation; the shared read lock is the reference
        // `lock.lockShared()`.
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
        //   — `self` is the type-erased `*mut ()` renderer root; the target is the raw
        //     `addr_of_mut!((*frame).target)`; the completion fn is the trampoline.
        //     (OpenGL `beginFrame` is infallible — `opengl/Frame.zig` has an empty error
        //     set — so no `?` here, unlike the Metal twin.)
        let frame_ctx: Frame = unsafe {
            (*self_ptr).api.begin_frame(
                self_ptr as *mut (),
                addr_of_mut!((*frame).target),
                opengl_frame_completed,
            )
        };

        {
            // var pass = frame_ctx.renderPass(&.{.{
            //     .target = if (frame.custom_shader_state) |state|
            //         .{ .texture = state.back_texture } else .{ .target = frame.target },
            //     .clear_color = .{ 0.0, 0.0, 0.0, 0.0 },
            // }});
            // defer pass.complete();
            //
            // PORT NOTE: OpenGL `RenderPass::step` takes `&mut self` (it bumps
            // `step_number`), unlike Metal's `&self`, so `pass` is `mut`. The
            // `attachments` slice is bound to a `let` so it outlives `pass` (the
            // `RenderPass` borrows it — E0716).
            let attach_target = match unsafe { (*frame).custom_shader_state.as_ref() } {
                Some(state) => AttachmentTarget::Texture(&state.back_texture),
                None => AttachmentTarget::Target(unsafe { &(*frame).target }),
            };
            let attachments = [Attachment {
                target: attach_target,
                clear_color: Some([0.0, 0.0, 0.0, 0.0]),
            }];
            let mut pass = frame_ctx.render_pass(&attachments);

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
            //
            // PORTS: generic.zig:1611-1628; macos_impl generic.rs:6069-6100.
            // if (self.bg_image) |img| switch (img) {
            //     .ready => |texture| pass.step(.{ .pipeline = bg_image, .uniforms,
            //         .buffers = &.{frame.bg_image_buffer.buffer},
            //         .textures = &.{texture}, .draw = {triangle, 3} }),
            //     else => {},
            // } else { pass.step(.{ .pipeline = bg_color, .uniforms, .buffers =
            //     &.{ null, frame.cells_bg.buffer }, .draw = {triangle, 3} }); }
            match unsafe { (*self_ptr).bg_image.as_ref() } {
                Some(image::Image::Ready(texture)) => pass.step(Step {
                    pipeline: unsafe { &(&(*self_ptr).shaders.pipelines).bg_image },
                    uniforms: Some(unsafe { (*frame).uniforms.buffer }),
                    buffers: &[Some(unsafe { (*frame).bg_image_buffer.buffer })],
                    textures: &[Some(texture)],
                    samplers: &[],
                    draw: Draw {
                        kind: Primitive::Triangle,
                        vertex_count: 3,
                        instance_count: 1,
                    },
                }),
                Some(_) => {}
                None => {
                    pass.step(Step {
                        pipeline: unsafe { &(&(*self_ptr).shaders.pipelines).bg_color },
                        uniforms: Some(unsafe { (*frame).uniforms.buffer }),
                        buffers: &[None, Some(unsafe { (*frame).cells_bg.buffer })],
                        textures: &[],
                        samplers: &[],
                        draw: Draw {
                            kind: Primitive::Triangle,
                            vertex_count: 3,
                            instance_count: 1,
                        },
                    });
                }
            }

            // Then we draw any kitty images that need
            // to be behind text AND cell backgrounds.
            // self.images.draw(&self.api, self.shaders.pipelines.image, &pass, .kitty_below_bg);
            //
            // PORTS: generic.zig:1628-1635; macos_impl generic.rs:6105-6113.
            // GL: `pass` is `&mut` (step bumps `step_number`) — see the module docs.
            unsafe {
                (*self_ptr).images.draw(
                    &(*self_ptr).api,
                    &(&(*self_ptr).shaders.pipelines).image,
                    &mut pass,
                    DrawPlacements::KittyBelowBg,
                );
            }

            // Opaque cell backgrounds.
            // pass.step(.{ .pipeline = cell_bg, .uniforms, .buffers = &.{ null,
            //     frame.cells_bg.buffer }, .draw = {triangle, 3} });
            pass.step(Step {
                pipeline: unsafe { &(&(*self_ptr).shaders.pipelines).cell_bg },
                uniforms: Some(unsafe { (*frame).uniforms.buffer }),
                buffers: &[None, Some(unsafe { (*frame).cells_bg.buffer })],
                textures: &[],
                samplers: &[],
                draw: Draw {
                    kind: Primitive::Triangle,
                    vertex_count: 3,
                    instance_count: 1,
                },
            });

            // Kitty images between cell backgrounds and text.
            // self.images.draw(..., .kitty_below_text);
            //
            // PORTS: generic.zig:1645-1651; macos_impl generic.rs:6131-6139.
            unsafe {
                (*self_ptr).images.draw(
                    &(*self_ptr).api,
                    &(&(*self_ptr).shaders.pipelines).image,
                    &mut pass,
                    DrawPlacements::KittyBelowText,
                );
            }

            // Text.
            // pass.step(.{ .pipeline = cell_text, .uniforms, .buffers = &.{
            //     frame.cells.buffer, frame.cells_bg.buffer }, .textures = &.{
            //     frame.grayscale, frame.color }, .draw = {triangle_strip, 4, fg_count} });
            pass.step(Step {
                pipeline: unsafe { &(&(*self_ptr).shaders.pipelines).cell_text },
                uniforms: Some(unsafe { (*frame).uniforms.buffer }),
                buffers: &[
                    Some(unsafe { (*frame).cells.buffer }),
                    Some(unsafe { (*frame).cells_bg.buffer }),
                ],
                textures: &[
                    Some(unsafe { &(*frame).grayscale }),
                    Some(unsafe { &(*frame).color }),
                ],
                samplers: &[],
                draw: Draw {
                    kind: Primitive::TriangleStrip,
                    vertex_count: 4,
                    instance_count: fg_count,
                },
            });

            // Kitty images in front of text.
            // self.images.draw(..., .kitty_above_text);
            //
            // PORTS: generic.zig:1671-1677; macos_impl generic.rs:6166-6174.
            unsafe {
                (*self_ptr).images.draw(
                    &(*self_ptr).api,
                    &(&(*self_ptr).shaders.pipelines).image,
                    &mut pass,
                    DrawPlacements::KittyAboveText,
                );
            }

            // Debug overlay. We do this before any custom shader state
            // because our debug overlay is aligned with the grid.
            // if (self.overlay != null) self.images.draw(..., .overlay);
            //
            // PORTS: generic.zig:1681-1688; macos_impl generic.rs:6176-6188.
            if unsafe { (*self_ptr).overlay.is_some() } {
                unsafe {
                    (*self_ptr).images.draw(
                        &(*self_ptr).api,
                        &(&(*self_ptr).shaders.pipelines).image,
                        &mut pass,
                        DrawPlacements::Overlay,
                    );
                }
            }

            // defer pass.complete();
            pass.complete();
        }

        // If we have custom shaders, then we render them.
        // if (frame.custom_shader_state) |*state| { … }
        // PORTS: generic.zig:1691-1719; macos_impl generic.rs:5963-6023.
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
            let post_pipelines: &[crate::opengl::pipeline::Pipeline] =
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
                let attachments = [Attachment {
                    target,
                    clear_color: Some([0.0, 0.0, 0.0, 0.0]),
                }];
                let mut pass = frame_ctx.render_pass(&attachments);

                // pass.step(.{ .pipeline = pipeline, .uniforms = state.uniforms.buffer,
                //     .textures = &.{state.back_texture}, .samplers = &.{state.sampler},
                //     .draw = .{ .type = .triangle, .vertex_count = 3 } });
                pass.step(Step {
                    pipeline,
                    uniforms: Some(unsafe { (*state).uniforms.buffer }),
                    buffers: &[],
                    textures: &[Some(unsafe { &(*state).back_texture })],
                    samplers: &[Some(unsafe { &(*state).sampler })],
                    draw: Draw {
                        kind: Primitive::Triangle,
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
        // trampoline calls `frame_completed` → `release_frame`. Leaving it armed would
        // double-release.
        release_guard.armed = false;

        // defer frame_ctx.complete(sync); — the reference's LAST-registered defer; for
        // OpenGL it `gl.finish()`es (always blocking) then invokes `opengl_frame_completed`
        // directly. `frame` is NOT used after this.
        frame_ctx.complete(sync);

        Ok(false)
        // The two RAII guards drop here in the reference LIFO order:
        // draw_frame_end → scrollbar; the caller's unlock follows.
    }

    /// `&mut self` convenience wrapper over [`draw_frame_raw`], locking through
    /// the renderer's own handle. Sound for the exclusive single-threaded
    /// offscreen callers (the golden unit tests). Mirrors `macos_impl::draw_frame`.
    pub fn draw_frame(&mut self, sync: bool) -> Result<(), Error> {
        let mutex = self.draw_mutex.clone();
        let self_ptr: *mut Renderer = self;
        unsafe { Self::draw_frame_raw(self_ptr, &mutex, sync) }
    }
}
