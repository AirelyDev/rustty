//! PORTS: ghostty src/renderer/generic.zig — `pushFrame`, `takeFrame`,
//! `setPresentationHealth` and `presentationHealth`, plus the port-only N437
//! cross-thread entries (each takes the caller's `draw_mutex` handle before it forms a
//! reference to the renderer).
//!
//! This file is part of the `Renderer(OpenGL)` arm (`generic::linux_impl`).

use super::*;

impl Renderer {
    /// Push the latest completed frame, replacing if one previously
    /// existed. Called on the render thread.
    ///
    /// Has no effect for renderers that do not export frames
    /// (i.e. `ExportedFrame == void`).
    // pub fn pushFrame(self: *Self, frame: ExportedFrame) void {
    //     return self.latest_frame.push(global.io(), frame);
    // }   (generic.zig:1028-1034)
    pub fn push_frame(&self, frame: ExportedFrame) {
        self.latest_frame.push(frame);
    }

    /// Take the latest completed frame for the apprt to composite.
    /// Returns null if no frame is available. The caller takes
    /// ownership of the returned frame. Called on the main thread.
    ///
    /// Has no effect for renderers that do not export frames
    /// (i.e. `ExportedFrame == void`).
    // pub fn takeFrame(self: *Self) ?ExportedFrame {
    //     return self.latest_frame.take(global.io());
    // }   (generic.zig:1036-1044)
    //
    // PORT NOTE (N437): the main thread takes frames through
    // [`Self::latest_frame_handle`], which reaches the slot without touching the
    // renderer; this method is that handle's `take` for callers that own the
    // renderer outright (the tests).
    pub fn take_frame(&self) -> Option<ExportedFrame> {
        self.latest_frame.take()
    }

    /// A handle to the `latest_frame` slot for the apprt's main thread (N437).
    /// Taken once, before the render thread starts.
    pub fn latest_frame_handle(&self) -> Arc<LatestFrame<ExportedFrame>> {
        self.latest_frame.clone()
    }

    /// Called when the apprt reports a change in how well
    /// frames can be presented.
    // pub fn setPresentationHealth(self: *Self, health: Health) void {
    //     self.presentation_health.store(health, .seq_cst);
    // }   (generic.zig:1163-1167)
    pub fn set_presentation_health(&self, health: crate::Health) {
        self.presentation_health.store(health);
    }

    /// Returns how well frames can be presented.
    // pub fn presentationHealth(self: *Self) Health {
    //     return self.presentation_health.load(.seq_cst);
    // }   (generic.zig:1169-1172)
    pub fn presentation_health(&self) -> crate::Health {
        self.presentation_health.load()
    }
}

// ===========================================================================
// N437: the cross-thread entries.
//
// Every renderer call the render thread makes (`renderer_seam::Thread`), and
// every one the apprt's main thread makes while the render thread runs
// (`displayRealized`/`displayUnrealized`), goes through one of these. Each takes
// the caller's own `draw_mutex` handle and forms a reference to the renderer only
// while holding it, so a whole-renderer `&mut` never overlaps another thread's
// access (Stacked and Tree Borrows; `.port/phase-2/mirihar-n437/`). Where the
// reference runs an entry unlocked (`setFocus`, `setVisible`'s field write,
// `animationWake`, `hasVsync`, `loopEnter`, `setMacOSDisplayID`, the search-match
// writes), it only ever races the main thread's draw on macOS; the port takes the
// lock rather than keep a data race, and runs the display link's CoreVideo calls
// after the unlock exactly as `drawFrame` does (UD352, `72cf50855`).
// ===========================================================================
impl Renderer {
    /// Runs `f` on the renderer with `lock` held.
    ///
    /// # Safety
    /// `self_ptr` is the live renderer and `lock` a handle to its `draw_mutex`;
    /// no other thread may form a reference to the renderer, or touch its bytes,
    /// without holding that mutex.
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

    /// `threadEnter` under the lock.
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

    /// `threadExit`: its `draw_mutex` block, then the api's `threadExit`.
    ///
    /// # Safety
    /// As [`Self::with_locked`].
    pub unsafe fn thread_exit_raw(self_ptr: *mut Renderer, lock: &DrawMutex) {
        unsafe { Self::with_locked(self_ptr, lock, |r| r.thread_exit_locked()) };
        unsafe { Self::with_locked(self_ptr, lock, |r| r.thread_exit_api()) };
    }

    /// `loopEnter`.
    ///
    /// # Safety
    /// As [`Self::with_locked`].
    pub unsafe fn loop_enter_raw(
        self_ptr: *mut Renderer,
        lock: &DrawMutex,
        draw_now: rustty_xev::Async,
    ) -> Result<(), Error> {
        unsafe { Self::with_locked(self_ptr, lock, |r| r.loop_enter(draw_now)) }
    }

    /// `loopExit`.
    ///
    /// # Safety
    /// As [`Self::with_locked`].
    pub unsafe fn loop_exit_raw(self_ptr: *mut Renderer, lock: &DrawMutex) {
        unsafe { Self::with_locked(self_ptr, lock, |r| r.loop_exit()) }
    }

    /// `setMacOSDisplayID`.
    ///
    /// # Safety
    /// As [`Self::with_locked`].
    pub unsafe fn set_macos_display_id_raw(
        self_ptr: *mut Renderer,
        lock: &DrawMutex,
        id: u32,
        draw_now: rustty_xev::Async,
    ) -> Result<(), Error> {
        unsafe { Self::with_locked(self_ptr, lock, |r| r.set_macos_display_id(id, draw_now)) }
    }

    /// `hasVsync`.
    ///
    /// # Safety
    /// As [`Self::with_locked`].
    pub unsafe fn has_vsync_raw(self_ptr: *mut Renderer, lock: &DrawMutex) -> bool {
        unsafe { Self::with_locked(self_ptr, lock, |r| r.has_vsync()) }
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
        unsafe { Self::with_locked(self_ptr, lock, |r| r.set_focus(focus)) }
    }

    /// `setVisible`: the locked body, then the display-link sync after the unlock.
    ///
    /// # Safety
    /// As [`Self::with_locked`].
    pub unsafe fn set_visible_raw(self_ptr: *mut Renderer, lock: &DrawMutex, visible: bool) {
        let sync = unsafe { Self::with_locked(self_ptr, lock, |r| r.set_visible_locked(visible)) };
        sync.apply();
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

    /// `updateFrame`, with the reference's lock scopes: the draw mutex is taken
    /// around the Kitty image update and the GPU-cell rebuild only (N437 /
    /// ruling 32, see `update_frame_impl`).
    ///
    /// # Safety
    /// As [`Self::with_locked`].
    pub unsafe fn update_frame_raw(
        self_ptr: *mut Renderer,
        lock: &DrawMutex,
        state: &state::State,
        cursor_blink_visible: bool,
    ) -> Result<(), VtOutOfMemory> {
        let result =
            unsafe { Self::update_frame_impl(self_ptr, lock, state, cursor_blink_visible) };
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

    /// `displayRealized`, from the apprt's main thread.
    ///
    /// # Safety
    /// As [`Self::with_locked`].
    pub unsafe fn display_realized_raw(
        self_ptr: *mut Renderer,
        lock: &DrawMutex,
    ) -> Result<(), Error> {
        // If our API has to do things on realize, let it. (`OpenGL` no longer
        // declares `displayRealized`; see `display_realized_locked`.)
        //
        // Lock the draw mutex so that we can safely update state.
        lock.lock();
        // N437 / ruling 32: the main thread writes the three flags through narrow
        // places of the raw root (never a whole-renderer reference), because the
        // render thread may be working on its own fields unlocked in `update_frame_raw`.
        // The writes are `display_realized_locked`'s body.
        unsafe {
            (*self_ptr).display_realized = true;
            (*self_ptr).reinitialize_shaders = true;
            (*self_ptr).target_config_modified = 1;
        }
        lock.unlock();
        Ok(())
    }

    /// `displayUnrealized`, from the apprt's main thread.
    ///
    /// # Safety
    /// As [`Self::with_locked`].
    pub unsafe fn display_unrealized_raw(self_ptr: *mut Renderer, lock: &DrawMutex) {
        // Lock the draw mutex so that we can safely update state.
        lock.lock();
        // (N437 / ruling 32: a narrow place, as in `display_realized_raw`;
        // `display_unrealized_locked`'s body.)
        unsafe { (*self_ptr).display_realized = false };
        lock.unlock();
    }

    /// `renderCallback`'s unrealized arm (`Thread.zig:556-565` at `12752b2ac`):
    /// if the display is now unrealized, release the GPU resources on the render
    /// thread and report `true` so the callback disarms.
    //
    //     if (!t.renderer.display_realized) {
    //         t.renderer.draw_mutex.lockUncancelable(global.io());
    //         defer t.renderer.draw_mutex.unlock(global.io());
    //
    //         t.renderer.releaseGpuResources();
    //         return .disarm;
    //     }
    //
    // PORT NOTE (N437): the reference reads `display_realized` before it locks,
    // racing the main thread's `displayRealized`/`displayUnrealized` write; the
    // port reads it under the lock it takes for the release anyway.
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
}
