//! PORTS: ghostty src/renderer/generic.zig — `pushFrame`, `takeFrame`,
//! `setPresentationHealth` and `presentationHealth`, plus the port-only N437
//! cross-thread entries (each takes the caller's `draw_mutex` handle before it forms a
//! reference to the renderer).
//!
//! This file is part of the macOS `Renderer(Metal)` arm (`generic::macos_impl`).

use super::*;

// =======================================================================
// N437: the cross-thread entries.
//
// Every renderer call the render thread makes (`renderer_seam::Thread`), and
// every one the main thread makes while the render thread runs (the
// CoreAnimation display callback's `drawFrame(true)`, core's `Surface::draw`),
// goes through one of these. Each takes the caller's own `draw_mutex` handle
// and forms a reference to the renderer only while holding it, so a
// whole-renderer `&mut` never overlaps another thread's access (Stacked and
// Tree Borrows; `.port/phase-2/mirihar-n437/`). Where the reference runs an
// entry unlocked (`setFocus`, `setVisible`'s field write, `animationWake`,
// `hasVsync`, `loopEnter`, `setMacOSDisplayID`, the search-match writes) it
// races the main thread's draw; the port takes the lock rather than keep a
// data race, and still makes every CoreVideo call after the unlock (UD352,
// `72cf50855`: stopping a CVDisplayLink is a blocking join on CoreVideo's IO
// thread, and the main thread's draw takes `draw_mutex`).
// =======================================================================
impl Renderer {
    /// Runs `f` on the renderer with `lock` held.
    ///
    /// # Safety
    /// `self_ptr` is the live renderer and `lock` a handle to its
    /// `draw_mutex`; no other thread may form a reference to the renderer, or
    /// touch its bytes, without holding that mutex.
    pub unsafe fn with_locked<T>(
        self_ptr: *mut Renderer,
        lock: &DrawMutex,
        f: impl FnOnce(&mut Renderer) -> T,
    ) -> T {
        lock.lock();
        struct UnlockGuard<'a>(&'a DrawMutex);
        impl Drop for UnlockGuard<'_> {
            fn drop(&mut self) {
                self.0.unlock();
            }
        }
        let _unlock = UnlockGuard(lock);
        // SAFETY: the caller's contract; the mutex excludes every other access.
        f(unsafe { &mut *self_ptr })
    }

    /// `syncDisplayLink` with every renderer access under `lock` and every
    /// CoreVideo call outside it. See `sync_display_link` for the reference.
    ///
    /// # Safety
    /// As [`Self::with_locked`]; called on the render thread, which owns and
    /// releases the link.
    pub unsafe fn sync_display_link_raw(
        self_ptr: *mut Renderer,
        lock: &DrawMutex,
        display_id: Option<u32>,
        draw_now: Option<Async>,
    ) {
        // const display_link = self.display_link orelse display_link: { ... };
        let (has_link, vsync) = unsafe {
            Self::with_locked(self_ptr, lock, |r| {
                (r.display_link.is_some(), r.config.vsync)
            })
        };
        if !has_link {
            // if (!self.config.vsync) return;
            if !vsync {
                return;
            }
            // const callback = draw_now orelse return;
            let Some(callback) = draw_now else {
                return;
            };
            // A locked macOS session can temporarily have no active
            // displays. Rendering can continue without vsync and a
            // later display update will retry this method.
            let mut result = match DisplayLink::create_with_active_cg_displays() {
                Ok(r) => r,
                Err(err) => {
                    // log.warn("error creating display link; using fallback rendering err={}", err);
                    log::warn!(
                        target: LOG_SCOPE,
                        "error creating display link; using fallback rendering err={err:?}"
                    );
                    return;
                }
            };
            if let Err(err) = result.set_output_callback(callback) {
                // log.warn("error configuring display link err={}", err);
                log::warn!(target: LOG_SCOPE, "error configuring display link err={err:?}");
                result.release();
                return;
            }

            // self.display_link = result;
            unsafe {
                Self::with_locked(self_ptr, lock, |r| {
                    // UD362: the health-report wakeup skips `draw_now` while the
                    // link runs (disarmed with it in `loop_exit_raw`).
                    r.health_report.set_link(Some(result.handle()));
                    r.display_link = Some(result)
                })
            };
            // log.info("created display link", .{});
            log::info!(target: LOG_SCOPE, "created display link");
        }

        // if (display_id) |id| { ... }
        if let Some(id) = display_id {
            let link = unsafe {
                Self::with_locked(self_ptr, lock, |r| {
                    r.display_link.as_ref().map(DisplayLink::handle)
                })
            };
            // log.info("updating display link display id={}", id);
            log::info!(target: LOG_SCOPE, "updating display link display id={id}");
            if let Some(link) = link {
                if let Err(err) = unsafe { link.set_current_cg_display(id) } {
                    // log.warn("error setting display link display id err={}", err);
                    log::warn!(
                        target: LOG_SCOPE,
                        "error setting display link display id err={err:?}"
                    );
                }
            }
        }

        let sync = unsafe { Self::with_locked(self_ptr, lock, |r| r.display_link_sync()) };
        sync.apply();
    }

    /// `threadEnter`.
    ///
    /// # Safety
    /// As [`Self::with_locked`].
    pub unsafe fn thread_enter_raw<S>(
        self_ptr: *mut Renderer,
        lock: &DrawMutex,
        rt_surface: *mut S,
    ) -> Result<(), Error> {
        unsafe { Self::with_locked(self_ptr, lock, |r| r.thread_enter(rt_surface)) }
    }

    /// `threadExit`: its `draw_mutex` block (Metal declares no api
    /// `threadExit`).
    ///
    /// # Safety
    /// As [`Self::with_locked`].
    pub unsafe fn thread_exit_raw(self_ptr: *mut Renderer, lock: &DrawMutex) {
        unsafe { Self::with_locked(self_ptr, lock, |r| r.thread_exit_locked()) };
    }

    /// `loopEnter`.
    ///
    /// # Safety
    /// As [`Self::with_locked`].
    pub unsafe fn loop_enter_raw(
        self_ptr: *mut Renderer,
        lock: &DrawMutex,
        draw_now: Async,
    ) -> Result<(), Error> {
        // If our API has to do things on loop enter, let it.
        // if (@hasDecl(GraphicsAPI, "loopEnter")) self.api.loopEnter();
        //
        // Metal DECLARES loopEnter ⇒ this hook fires. It installs the
        // IOSurfaceLayer display callback (W5). container_of needs
        // WHOLE-Renderer provenance (`draw_frame_raw` derefs all of Renderer),
        // so the leaf gets a raw `*mut Metal` projected from the RAW root
        // `self_ptr` — never from a `&mut` reborrow, whose tag the next
        // reborrow from the root would invalidate. It also gets a `draw_mutex`
        // handle for the callback (N437). See `W5_DESIGN §2.4`.
        lock.lock();
        let api_ptr: *mut Metal = unsafe { &raw mut (*self_ptr).api };
        unsafe { Metal::loop_enter(api_ptr, lock.clone()) };
        // UD362: arm the health-report wakeup (disarmed in
        // `loop_exit_raw`, before the render thread releases `draw_now`).
        unsafe { (*self_ptr).health_report.arm(Some(draw_now)) };
        lock.unlock();

        // `if (comptime DisplayLink == void) return;` — macOS: DisplayLink is
        // never void; no early return.
        //
        // `self.syncDisplayLink(null, &thr.draw_now)` (a177ba90a): this is the
        // one call site that can CREATE the link, because it is the only one
        // holding the `draw_now` the output callback needs.
        unsafe { Self::sync_display_link_raw(self_ptr, lock, None, Some(draw_now)) };
        Ok(())
    }

    /// `loopExit`: stop the link, after the unlock.
    ///
    /// # Safety
    /// As [`Self::with_locked`].
    pub unsafe fn loop_exit_raw(self_ptr: *mut Renderer, lock: &DrawMutex) {
        let link = unsafe {
            Self::with_locked(self_ptr, lock, |r| {
                // UD362: disarm the health-report wakeup.
                r.health_report.arm(None);
                r.display_link.as_ref().map(DisplayLink::handle)
            })
        };
        // Stop our display link. If this fails its okay it just means
        // that we either never started it or the view its attached to
        // is gone which is fine.
        if let Some(link) = link {
            let _ = unsafe { link.stop() }; // catch {}
        }
    }

    /// `setMacOSDisplayID`.
    ///
    /// # Safety
    /// As [`Self::with_locked`].
    pub unsafe fn set_macos_display_id_raw(
        self_ptr: *mut Renderer,
        lock: &DrawMutex,
        id: u32,
        draw_now: Async,
    ) -> Result<(), Error> {
        unsafe { Self::sync_display_link_raw(self_ptr, lock, Some(id), Some(draw_now)) };
        Ok(())
    }

    /// `hasVsync`: the link's `isRunning`, after the unlock.
    ///
    /// # Safety
    /// As [`Self::with_locked`].
    pub unsafe fn has_vsync_raw(self_ptr: *mut Renderer, lock: &DrawMutex) -> bool {
        let link = unsafe {
            Self::with_locked(self_ptr, lock, |r| {
                r.display_link.as_ref().map(DisplayLink::handle)
            })
        };
        match link {
            Some(link) => unsafe { link.is_running() },
            None => false,
        }
    }

    /// `setFocus`.
    ///
    /// # Safety
    /// As [`Self::with_locked`].
    pub unsafe fn set_focus_raw(
        self_ptr: *mut Renderer,
        lock: &DrawMutex,
        focus: bool,
    ) -> Result<(), Error> {
        unsafe { Self::with_locked(self_ptr, lock, |r| r.set_focus_locked(focus)) };

        // PORT NOTE (`a177ba90a`): the inlined `if (focus) start() else stop()`
        // moved into `sync_display_link`, which gates on visibility TOO —
        // focusing an occluded surface no longer spins vsync back up.
        // self.syncDisplayLink(null, null);
        unsafe { Self::sync_display_link_raw(self_ptr, lock, None, None) };
        Ok(())
    }

    /// `setVisible`.
    ///
    /// # Safety
    /// As [`Self::with_locked`].
    pub unsafe fn set_visible_raw(self_ptr: *mut Renderer, lock: &DrawMutex, visible: bool) {
        // self.visible = visible;
        unsafe { Self::with_locked(self_ptr, lock, |r| r.visible = visible) };
        // self.syncDisplayLink(null, null);
        unsafe { Self::sync_display_link_raw(self_ptr, lock, None, None) };

        // When we're hidden, release our GPU resources.
        if !visible {
            unsafe { Self::with_locked(self_ptr, lock, |r| r.release_gpu_resources()) };
        }
    }

    /// Whether a health report is owed (UD362, `HealthReport`), through the
    /// caller's `draw_mutex` handle.
    ///
    /// The render thread's `drawNowCallback` asks this after its draw (and
    /// the display-link resync) and arms its animation timer when it is true,
    /// so an owed report keeps being retried even when the draw that
    /// `HealthReport::wake` caused, or a no-redraw draw that parked the link
    /// while the report was being refused, would otherwise be the last one.
    ///
    /// # Safety
    /// As [`Self::with_locked`].
    pub unsafe fn health_report_owed_raw(self_ptr: *mut Renderer, lock: &DrawMutex) -> bool {
        unsafe { Self::with_locked(self_ptr, lock, |r| r.health_report.pending()) }
    }

    /// `animationWake`.
    ///
    /// # Safety
    /// As [`Self::with_locked`].
    pub unsafe fn animation_wake_raw(
        self_ptr: *mut Renderer,
        lock: &DrawMutex,
    ) -> Option<AnimationWake> {
        unsafe { Self::with_locked(self_ptr, lock, |r| r.animation_wake()) }
    }

    /// `updateFrame`, with the reference's lock scopes: the draw mutex is
    /// taken around the Kitty image update and the GPU-cell rebuild only
    /// (N437 / ruling 32, see `update_frame_impl`), and the display-link
    /// resync is applied after the unlock (UD352).
    ///
    /// # Safety
    /// As [`Self::with_locked`].
    pub unsafe fn update_frame_raw(
        self_ptr: *mut Renderer,
        lock: &DrawMutex,
        state: &state::State,
        cursor_blink_visible: bool,
    ) -> Result<(), VtOutOfMemory> {
        let mut link_sync = None;
        let result = unsafe {
            Self::update_frame_impl(self_ptr, lock, state, cursor_blink_visible, &mut link_sync)
        };
        // Start the display link now that the rebuilt frame is ready (the
        // resync's reads were taken under the lock, UD352).
        if let Some(sync) = link_sync {
            sync.apply();
        }
        // CoreText shaping accumulates objects for deferred release over
        // the course of a frame. Always flush those objects, including
        // when rebuilding the frame fails due to memory pressure.
        // defer self.font_shaper.endFrame();
        //
        // PORT NOTE (parity S115-F1): the reference's first statement is this
        // `defer`, so `endFrame` runs on every exit: the synchronized-output early
        // `return`, a failed `beginUpdate`, and the normal end, after the
        // later-registered `defer self.terminal_state.dirty = .false` (the impl's
        // last statement). Running `end_frame` after the impl returns is that
        // defer. (N437 / ruling 32: unlocked, as in the reference, through a
        // narrow place; the draw never touches the shaper.)
        unsafe { (*self_ptr).font_shaper.end_frame() };
        result
    }

    /// This is called by the GTK apprt after the surface is
    /// reinitialized (e.g. after the widget is re-realized following
    /// a display change or reparenting).
    //
    // generic.zig:947-968 `displayRealized` (`2f0b65346`). Metal declares no
    // `displayRealized`; only the flags change. No macOS caller exists (the
    // reference's only caller is the GTK apprt); ported so both twins carry
    // the whole of `generic.zig`.
    ///
    /// # Safety
    /// As [`Self::with_locked`].
    pub unsafe fn display_realized_raw(
        self_ptr: *mut Renderer,
        lock: &DrawMutex,
    ) -> Result<(), Error> {
        // Lock the draw mutex so that we can safely update state.
        lock.lock();
        // N437 / ruling 32: the main thread writes the three flags through narrow
        // places of the raw root (never a whole-renderer reference), because the
        // render thread may be working on its own fields unlocked in `update_frame_raw`.
        unsafe {
            // Mark the display as realized. The render thread will lazily
            // rebuild the swap chain and shaders on the next `drawFrame`,
            // which is the right place for GL resource creation (it
            // guarantees a current context on the render thread).
            (*self_ptr).display_realized = true;
            (*self_ptr).reinitialize_shaders = true;
            (*self_ptr).target_config_modified = 1;
        }
        lock.unlock();
        Ok(())
    }

    /// This is called when the surface is being unrealized.
    /// This can happen because the surface is being closed but
    /// also when moving the window between displays or splitting.
    ///
    /// This runs on the main thread and only updates CPU-side state
    /// here; resource cleanup happens on the render thread via
    /// `releaseGpuResources`.
    //
    // generic.zig:970-985 `displayUnrealized` (`2f0b65346`).
    ///
    /// # Safety
    /// As [`Self::with_locked`].
    pub unsafe fn display_unrealized_raw(self_ptr: *mut Renderer, lock: &DrawMutex) {
        // Lock the draw mutex so that we can safely update state.
        lock.lock();
        // (N437 / ruling 32: a narrow place, as in `display_realized_raw`.)
        // Clearing `display_realized` ensures drawFrame doesn't attempt
        // to rebuild the swap chain or make any graphics API calls.
        // The actual GPU resource release is done by the render thread.
        unsafe { (*self_ptr).display_realized = false };
        lock.unlock();
    }

    /// `renderCallback`'s unrealized arm (`Thread.zig:556-565` at `12752b2ac`);
    /// see the OpenGL twin. `display_realized` is read under the lock.
    ///
    /// # Safety
    /// As [`Self::with_locked`].
    pub unsafe fn release_if_unrealized_raw(self_ptr: *mut Renderer, lock: &DrawMutex) -> bool {
        unsafe {
            Self::with_locked(self_ptr, lock, |r| {
                if r.display_realized {
                    return false;
                }
                r.release_gpu_resources();
                true
            })
        }
    }

    /// Push the latest completed frame, replacing if one previously
    /// existed. Called on the render thread.
    ///
    /// Has no effect for renderers that do not export frames
    /// (i.e. `ExportedFrame == void`).
    // pub fn pushFrame(self: *Self, frame: ExportedFrame) void (generic.zig:1028-1034)
    pub fn push_frame(&self, frame: ()) {
        self.latest_frame.push(frame);
    }

    /// Take the latest completed frame for the apprt to composite.
    /// Returns null if no frame is available. The caller takes
    /// ownership of the returned frame. Called on the main thread.
    ///
    /// Has no effect for renderers that do not export frames
    /// (i.e. `ExportedFrame == void`).
    // pub fn takeFrame(self: *Self) ?ExportedFrame (generic.zig:1036-1044)
    pub fn take_frame(&self) -> Option<()> {
        self.latest_frame.take()
    }

    /// A handle to the `latest_frame` slot for the apprt's main thread (N437).
    pub fn latest_frame_handle(&self) -> Arc<LatestFrame<()>> {
        self.latest_frame.clone()
    }

    /// Called when the apprt reports a change in how well
    /// frames can be presented.
    // pub fn setPresentationHealth(self: *Self, health: Health) void (generic.zig:1163-1167)
    pub fn set_presentation_health(&self, health: crate::Health) {
        self.presentation_health.store(health);
    }

    /// Returns how well frames can be presented.
    // pub fn presentationHealth(self: *Self) Health (generic.zig:1169-1172)
    pub fn presentation_health(&self) -> crate::Health {
        self.presentation_health.load()
    }
}
