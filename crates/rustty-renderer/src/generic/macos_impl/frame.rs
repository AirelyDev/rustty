//! PORTS: ghostty src/renderer/generic.zig — Chunk 3c, the frame orchestrator and
//! uniforms math (`updateFrame`, `updateFontGridUniforms`, `updateScreenSizeUniforms`,
//! `updateBgImageBuffer`, `updateCustomShaderUniformsFromState`,
//! `updateCustomShaderUniformsForFrame`).
//!
//! This file is part of the macOS `Renderer(Metal)` arm (`generic::macos_impl`).

use super::*;

// =======================================================================
// Chunk 3c — the frame orchestrator + uniforms math (GPU_RD_CHUNK3C_DESIGN)
// =======================================================================

impl Renderer {
    /// Update the frame data.
    //
    // Zig: pub fn updateFrame(self, state: *renderer.State, cursor_blink_visible: bool)
    //      Allocator.Error!void
    // WIDENED to `pub` (dead-code-exempt entry point; also the crate's frame driver).
    // It is the SOLE caller of the already-ported `rebuild_cells` (:1364) /
    // `rebuild_overlay` (:1349), retiring their 3b "real caller lands in 3c"
    // placeholders. `state: *renderer.State` -> `&state::State` (Chunk 3c §B.2):
    // `update_frame` never mutates a `State` *field* — it only locks `state.mutex`
    // (`lock(&self)`) and mutates THROUGH the stored `*mut Terminal` (a disjoint
    // allocation), so a shared `&State` suffices. The ONLY propagated `?` is
    // `terminal_state.update`, whose error is `VtOutOfMemory`
    // (`rustty_vt::pagelist::OutOfMemory`, aliased — see the import PORT NOTE); Zig
    // `Allocator.Error!void` -> `Result<(), VtOutOfMemory>`.
    pub fn update_frame(
        &mut self,
        state: &state::State,
        cursor_blink_visible: bool,
    ) -> Result<(), VtOutOfMemory> {
        let mutex = self.draw_mutex.clone();
        let self_ptr: *mut Renderer = self;
        unsafe { Self::update_frame_raw(self_ptr, &mutex, state, cursor_blink_visible) }
    }

    /// The body of [`Self::update_frame_raw`], which runs
    /// `font_shaper.end_frame()` after it on every exit.
    ///
    /// # Safety
    /// `self_ptr` is a valid, whole-`Renderer`-provenance pointer and `lock`
    /// a handle to its draw mutex, which the caller does NOT hold; only the
    /// render thread calls this.
    pub(super) unsafe fn update_frame_impl(
        self_ptr: *mut Renderer,
        lock: &DrawMutex,
        state: &state::State,
        cursor_blink_visible: bool,
        link_sync: &mut Option<DisplayLinkSync>,
    ) -> Result<(), VtOutOfMemory> {
        // N437 / ruling 32 (P7-W7 T2 fix round 1): the reference runs this part of
        // `updateFrame` without the draw mutex; only the render thread uses these
        // fields. The port reaches each through a narrow place of the raw root, never
        // a whole-renderer reference, so the main thread's locked accesses (its sync
        // draw on macOS, `displayRealized`/`displayUnrealized` on GTK, all narrow
        // too) touch no byte these borrows cover. Proven on
        // `.port/phase-2/mirihar-n437` (`split_*`).
        let alloc = unsafe { (*self_ptr).alloc };
        let scroll_to_bottom_on_output = unsafe { (*self_ptr).config.scroll_to_bottom_on_output };
        // (`render_cell_map` updates the link matchers' caches, so this one is `&mut`.)
        let config_links = unsafe { &mut (*self_ptr).config.links };
        let terminal_state: &mut RenderState = unsafe { &mut (*self_ptr).terminal_state };
        let terminal_state_frame_count = unsafe { &mut (*self_ptr).terminal_state_frame_count };
        let last_bottom_node = unsafe { &mut (*self_ptr).last_bottom_node };
        let last_bottom_y = unsafe { &mut (*self_ptr).last_bottom_y };
        let kitty_animation_next_ms = unsafe { &mut (*self_ptr).kitty_animation_next_ms };
        let kitty_animation_clock = unsafe { &mut (*self_ptr).kitty_animation_clock };
        let search_matches_dirty = unsafe { &mut (*self_ptr).search_matches_dirty };
        let search_selected_match = unsafe { &(*self_ptr).search_selected_match };
        let search_matches = unsafe { &(*self_ptr).search_matches };

        /// Unlocks the draw mutex on every exit of the Kitty block.
        struct DrawUnlock<'a>(&'a DrawMutex);
        impl Drop for DrawUnlock<'_> {
            fn drop(&mut self) {
                self.0.unlock();
            }
        }

        // const start = std.time.Instant.now() catch unreachable;
        // const start_micro = std.time.microTimestamp();
        // defer {
        //     const end = std.time.Instant.now() catch unreachable;
        //     log.warn(
        //         "[updateFrame time] start_micro={} duration={}ns",
        //         .{ start_micro, end.since(start) / std.time.ns_per_us },
        //     );
        // }

        // We fully deinit and reset the terminal state every so often
        // so that a particularly large terminal state doesn't cause
        // the renderer to hold on to retained memory.
        //
        // Frame count is ~12 minutes at 120Hz.
        let max_terminal_state_frame_count = 100_000;
        if *terminal_state_frame_count >= max_terminal_state_frame_count {
            terminal_state.deinit(alloc.into());
            *terminal_state = RenderState::empty();
            *terminal_state_frame_count = 0;
        }
        *terminal_state_frame_count += 1;

        // Create an arena for all our temporary allocations while rebuilding
        //
        // PORT NOTE (Chunk 3c §D.6): the reference opens `var arena =
        // ArenaAllocator.init(self.alloc)` and threads `arena_alloc` into
        // `preedit.clone`, `link_cells`, and `render_cell_map`. In rustty every one
        // of those owns its result as a `Global` `Vec`/`CellSet` freed by `Drop`
        // (the arena-collapse precedent — message.rs `SearchMatches`), so the
        // explicit `ArenaAllocator` is omitted and `Global` is passed where the
        // reference passes `arena_alloc` (nothing leaks — the former arena tenants
        // are now `Drop`-freed).
        // var arena = ArenaAllocator.init(self.alloc);
        // defer arena.deinit();
        // const arena_alloc = arena.allocator();

        // Data we extract out of the critical area.
        struct Critical {
            links: CellSet,
            // §C.5: written (`:1275 .mouse = state.mouse`) but never READ after the
            // block in the reference (post-block code reads `state.mouse.point/.mods`
            // directly, :1289/1290). Mirrored (R1) as a faithful write-only field.
            #[allow(dead_code)]
            mouse: state::Mouse,
            preedit: Option<state::Preedit>,
            scrollbar: Scrollbar,
            overlay_features: Vec<crate::overlay::Feature>,
            /// `self.size`, copied under the draw mutex for the overlay rebuild (UD363).
            size: Size,
        }

        // The renderer-thread state carries its terminal behind a `*mut Terminal`
        // (a separate allocation, disjoint from `self`). Copy the stored raw
        // pointer and re-deref it FRESH at every use (never cache a reference
        // across an interleaving `&mut`), matching the Zig `state.terminal.…`
        // re-walk (Chunk 3c §C.2).
        let t = state.terminal; // *mut Terminal

        // Hand-rolled `defer state.unlockDemand()` (Chunk 3c §C.4/RISK-5): the
        // guard-less `rustty_zigstd::thread::Mutex` has no RAII drop, and the lock
        // is held across THREE exits — the sync-output early `return`, the
        // `terminal_state.update(...)?` propagation (the sole propagating `?`,
        // INSIDE the lock), and normal fall-through. This `Drop` fires on all
        // three. Sound: it borrows `state` SHAREDLY (and through it the mutex
        // allocation), disjoint from `self`, so it does not clash with the
        // `&mut self.terminal_state` / `&mut *t` uses inside the block.
        //
        // (D4, oracle d34b54e9b: the guard borrows the whole `&State` rather than
        // just `&Mutex` because the release path is now `unlock_demand`, which
        // must also bump the handoff generation and wake parked yielders.)
        struct Unlock<'a>(&'a state::State);
        impl Drop for Unlock<'_> {
            fn drop(&mut self) {
                // SAFETY: as for the `lock_demand` below — `state.mutex` is the
                // live shared render mutex, held by this frame.
                unsafe { self.0.unlock_demand() };
            }
        }

        // Update all our data as tightly as possible within the mutex.
        let mut critical: Critical = {
            // NOTE: This code needs be updated to 0.16.0 before you
            // un-comment it ;)
            //
            // const start = try std.time.Instant.now();
            // const start_micro = std.time.microTimestamp();
            // defer {
            //     const end = std.time.Instant.now() catch unreachable;
            //     std.log.err("[updateFrame critical time] start={}\tduration={} us", .{ start_micro, end.since(start) / std.time.ns_per_us });
            // }

            // Lock while signaling demand so the IO parse thread can't starve
            // us. See renderer::State::lock_demand.
            //
            // SAFETY: `state.mutex` points at the live shared render-state mutex
            // and this frame does not already hold it.
            unsafe {
                state.lock_demand();
            }
            let _mutex_guard = Unlock(state);

            // If we're in a synchronized output state, we pause all rendering.
            if unsafe { (*t).modes.get(Mode::SynchronizedOutput) } {
                // log.debug("synchronized output started, skipping render", .{});
                log::debug!(target: LOG_SCOPE, "synchronized output started, skipping render");
                return Ok(());
            }

            // If scroll-to-bottom on output is enabled, check if the final line
            // changed by comparing the bottom-right pin. If the node pointer or
            // y offset changed, new content was added to the screen.
            // Update this BEFORE we update our render state so we can
            // draw the new scrolled data immediately.
            if scroll_to_bottom_on_output {
                'scroll: {
                    let active = unsafe { (*t).screens.active };
                    let br = match unsafe { (*active).pages.get_bottom_right(point::Tag::Screen) } {
                        Some(br) => br,
                        None => break 'scroll,
                    };

                    // If the pin hasn't changed, then don't scroll.
                    if *last_bottom_node == Some(br.node as usize) && *last_bottom_y == br.y {
                        break 'scroll;
                    }

                    // Update tracked pin state for next frame
                    *last_bottom_node = Some(br.node as usize);
                    *last_bottom_y = br.y;

                    // Scroll
                    unsafe {
                        (*t).scroll_viewport(ScrollViewport::Bottom);
                    }
                }
            }

            // Begin the update of our terminal state. Work that
            // doesn't require terminal access (e.g. style
            // denormalization) is deferred to the end_update call
            // outside of this critical section, keeping our lock
            // hold time as short as possible.
            terminal_state.begin_update(alloc.into(), unsafe { &mut *t })?;

            // If our terminal state is dirty at all we need to redo
            // the viewport search.
            if terminal_state.dirty != Dirty::False {
                unsafe {
                    (*t).flags.search_viewport_dirty = true;
                }
            }

            // Get our scrollbar out of the terminal. We synchronize
            // the scrollbar read with frame data updates because this
            // naturally limits the number of calls to this method (it
            // can be expensive) and also makes it so we don't need another
            // cross-thread mailbox message within the IO path.
            //
            // (§C.1-#9: re-read `active` FRESH — do NOT reuse the scroll block's
            // `active` across the `&mut *t` update above.)
            let active = unsafe { (*t).screens.active };
            let scrollbar = unsafe { (*active).pages.scrollbar() };

            // Get our preedit state
            let preedit: Option<state::Preedit> = state.preedit.as_ref().map(|p| p.clone());

            // Advance any running Kitty graphics animations to the
            // frame due now, and remember when the next frame is
            // due (as an absolute deadline, see animationWake) so
            // the renderer thread can schedule a wakeup for it.
            // This must happen before the dirty check below:
            // advancing a frame marks the image state dirty.
            //
            // self.kitty_animation_next_ms = next: {
            //     // Likely case: we have no kitty images, so do nothing.
            //     const storage = &state.terminal.screens.active.kitty_images;
            //     if (storage.images.count() == 0) break :next null;
            //
            //     const now: std.Io.Timestamp = .now(global.io(), .awake);
            //     const base = self.kitty_animation_clock orelse base: {
            //         self.kitty_animation_clock = now;
            //         break :base now;
            //     };
            //     const now_ms: u64 = @intCast(@divTrunc(
            //         base.durationTo(now).nanoseconds,
            //         std.time.ns_per_ms,
            //     ));
            //     const delay = storage.animationTick(
            //         global.io(),
            //         now_ms,
            //     ) orelse break :next null;
            //     break :next now_ms + delay;
            // };
            //
            // SAFETY (assessment D hazard, `animationTick` threading): the
            // tick MUTATES kitty storage (frame advance + dirty stamp) from
            // the render thread while the terminal is LOCKED (this whole
            // block runs under the renderer/terminal mutex, like the
            // `begin_update` above) — the IO thread cannot interleave. The
            // `&mut` is re-derived from the raw screen ptr for exactly this
            // statement (no borrow held across).
            *kitty_animation_next_ms = 'next: {
                // Likely case: we have no kitty images, so do nothing.
                let active = unsafe { (*t).screens.active };
                if unsafe { &(*active).kitty_images }.images.is_empty() {
                    break 'next None;
                }

                let now = Instant::now();
                let base = match *kitty_animation_clock {
                    Some(base) => base,
                    None => {
                        *kitty_animation_clock = Some(now);
                        now
                    }
                };
                let now_ms: u64 = now.duration_since(base).as_millis() as u64;
                let delay = match unsafe { &mut (*active).kitty_images }.animation_tick(now_ms) {
                    Some(delay) => delay,
                    None => break 'next None,
                };
                break 'next Some(now_ms + delay);
            };

            // If we have Kitty graphics data, we enter a SLOW SLOW SLOW path.
            // We only do this if the Kitty image state is dirty meaning only if
            // it changes.
            //
            // If we have any virtual references, we must also rebuild our
            // kitty state on every frame because any cell change can move
            // an image.
            // PORT NOTE (N437 / ruling 32, Rust aliasing, not a reference defect):
            // the reference reads `kittyRequiresUpdate` without the lock; the
            // fields it reads are ones the draw never writes. In the port the
            // check forms a reference to the whole `images` state, which the main
            // thread's draw mutates under the lock (`images.upload`), so the lock is
            // taken around the check as well as the update. The same lock hold
            // copies `size`, which the draw writes on a resize (`self.size.screen`),
            // for the overlay rebuild below.
            let size_snapshot: Size = {
                lock.lock();
                let _unlock = DrawUnlock(lock);
                let size_snapshot = unsafe { (*self_ptr).size };
                if unsafe { (*self_ptr).images.kitty_requires_update(&*t) } {
                    // We need to grab the draw mutex since this updates
                    // our image state that drawFrame uses.
                    unsafe {
                        (*self_ptr).images.kitty_update(
                            alloc,
                            &*t,
                            CellSize {
                                width: (*self_ptr).grid_metrics.cell_width,
                                height: (*self_ptr).grid_metrics.cell_height,
                            },
                        )
                    };
                }
                size_snapshot
            };

            // Get our OSC8 links we're hovering if we have a mouse.
            // This requires terminal state because of URLs.
            let links: CellSet = match state.mouse.point {
                // If our mouse isn't hovering, we have no links.
                None => CellSet::new(),
                Some(vp) => {
                    // If the right mods aren't pressed, then we can't match.
                    if !state.mouse.mods.equal(key::ctrl_or_super(Mods::default())) {
                        CellSet::new()
                    } else {
                        terminal_state
                            .link_cells(Global.into(), vp)
                            .unwrap_or_else(|err| {
                                // log.warn("error searching for OSC8 links err={}", .{err});
                                log::warn!(
                                    target: LOG_SCOPE,
                                    "error searching for OSC8 links err={err:?}"
                                );
                                CellSet::new()
                            })
                    }
                }
            };

            let overlay_features: Vec<crate::overlay::Feature> = match state.inspector {
                //   const insp = state.inspector orelse break :overlay &.{};
                //   const renderer_info = insp.rendererInfo();
                //   break :overlay renderer_info.overlayFeatures(arena_alloc) catch &.{};
                //
                // R2 ACCESSOR-TRAIT SEAM (was DEFERRED, Task 5): `state.inspector`
                // is now the renderer-owned `*mut dyn InspectorOverlay` fat pointer
                // (state.rs) — the acyclic accessor-trait (INV-7), the exact shape
                // `RendererConfig` uses for the config edge. The renderer reads the
                // enabled overlay features THROUGH the trait, never naming a
                // `rustty-inspector` type. The reference's `insp.rendererInfo()`
                // navigation happens at STORE time (core coerces the `Info` into the
                // fat pointer), so here the one-method trait call subsumes the whole
                // chain. INV-1: this deref is inside the `state.mutex`-guarded
                // critical section; the raw pointer never escapes the guard.
                None => Vec::new(),
                Some(insp) => unsafe { (*insp).overlay_features() },
            };

            Critical {
                links,
                mouse: state.mouse,
                preedit,
                scrollbar,
                overlay_features,
                size: size_snapshot,
            }
        };

        // Outside the critical area, complete the update we began
        // within it. This must be done before anything reads the
        // render state (e.g. rebuild_cells).
        terminal_state.end_update();

        // Outside the critical area we can update our links to contain
        // our regex results.
        // catch |err| log.warn("error searching for regex links err={}", .{err});
        if let Err(err) = config_links.render_cell_map(
            Global.into(),
            &mut critical.links,
            &*terminal_state,
            state.mouse.point,
            state.mouse.mods,
        ) {
            log::warn!(target: LOG_SCOPE, "error searching for regex links err={err:?}");
        }

        // Clear our highlight state and update.
        if *search_matches_dirty || terminal_state.dirty != Dirty::False {
            *search_matches_dirty = false;

            // Clear the prior highlights
            let mut any_dirty: bool = false;
            for row in terminal_state.row_data.iter_mut() {
                if !row.highlights.is_empty() {
                    row.highlights.clear();
                    row.dirty = true;
                    any_dirty = true;
                }
            }
            if any_dirty && terminal_state.dirty == Dirty::False {
                terminal_state.dirty = Dirty::Partial;
            }

            // NOTE: The order below matters. Highlights added earlier
            // will take priority.

            if let Some(m) = search_selected_match {
                // catch |err| {
                //     // Not a critical error, we just won't show highlights.
                //     log.warn("error updating search selected highlight err={}", .{err});
                // }
                if let Err(err) = terminal_state.update_highlights_flattened(
                    alloc.into(),
                    HighlightTag::SearchMatchSelected as u8,
                    core::slice::from_ref(&m.r#match),
                ) {
                    // Not a critical error, we just won't show highlights.
                    log::warn!(
                        target: LOG_SCOPE,
                        "error updating search selected highlight err={err:?}"
                    );
                }
            }

            if let Some(m) = search_matches {
                // catch |err| {
                //     // Not a critical error, we just won't show highlights.
                //     log.warn("error updating search highlights err={}", .{err});
                // }
                if let Err(err) = terminal_state.update_highlights_flattened(
                    alloc.into(),
                    HighlightTag::SearchMatch as u8,
                    &m.matches,
                ) {
                    // Not a critical error, we just won't show highlights.
                    log::warn!(target: LOG_SCOPE, "error updating search highlights err={err:?}");
                }
            }
        }

        // From this point forward no more errors.
        // errdefer comptime unreachable;

        // Reset our dirty state after updating.
        // defer self.terminal_state.dirty = .false;
        //
        // PORT NOTE (§C.4/RISK-3): the reference `defer` fires at function scope
        // exit — i.e. AFTER `updateCustomShaderUniformsFromState` (which GATES on
        // `dirty != .false`). So the reset is the LAST statement of this body,
        // NOT here; the post-region is infallible so a plain final assignment is
        // faithful. CORRECTION (parity S115-F1): this used to say the reset runs
        // after `endFrame`; the `endFrame` defer is registered FIRST, so it runs
        // LAST, after this reset (in `update_frame`, after this body returns).

        // Rebuild the overlay image if we have one. We can do this
        // outside of any critical areas.
        // self.rebuildOverlay(critical.overlay_features) catch |err|
        //     log.warn("error rebuilding overlay surface err={}", .{err});
        //
        // PORT NOTE: reference defect (R13), UD363. The reference rebuilds
        // `self.overlay` in place here, without the lock, while the macOS
        // main thread's sync draw reads `self.overlay != null` under it. The
        // port builds the next overlay into a local (reusing last frame's
        // replaced one, `overlay_spare`) and swaps it in under the lock below.
        let mut next_overlay: Option<Overlay> = unsafe { (*self_ptr).overlay_spare.take() };
        if let Err(err) = Self::rebuild_overlay_into(
            &mut next_overlay,
            &critical.size,
            terminal_state,
            &critical.overlay_features,
        ) {
            log::warn!(target: LOG_SCOPE, "error rebuilding overlay surface err={err:?}");
        }

        // Acquire the draw mutex for all remaining state updates.
        // (N437 / ruling 32: the whole `&mut` exists only here, under the
        // lock, after the last use of the unlocked view above.)
        let (sync, old_overlay, overlay_off) = unsafe {
            Self::with_locked(self_ptr, lock, move |r| {
                // PORT NOTE: reference defect (R13), UD363. The overlay was built
                // into `next_overlay` above, without the lock (as the reference
                // rebuilds it); only the swap into `self.overlay` happens here.
                let old_overlay = core::mem::replace(&mut r.overlay, next_overlay);

                // Build our GPU cells.
                //
                // `critical.preedit` is MOVED into `rebuild_cells`, but
                // `critical.preedit != null` is read FIRST for `cursor::style` — so
                // compute the cursor style (reading `is_some()`) before the move.
                let cs = cursor::style(
                    &r.terminal_state,
                    cursor::StyleOptions {
                        preedit: critical.preedit.is_some(),
                        focused: r.focused,
                        blink_visible: cursor_blink_visible,
                    },
                );
                r.rebuild_cells(critical.preedit, cs, &critical.links);
                // catch |err| {
                //     // This means we weren't able to allocate our buffer
                //     // to update the cells. In this case, we continue with
                //     // our old buffer (frozen contents) and log it.
                //     comptime assert(@TypeOf(err) == error{OutOfMemory});
                //     log.warn("error rebuilding GPU cells err={}", .{err});
                // }
                // (rebuild_cells is infallible in rustty — its sole OOM source aborts
                // via the Global allocator, so the reference's "continue with frozen
                // contents" catch path above is unreachable here; §3b §C.4.)
                //
                // LOG PORT NOTE (task-14c-1, row S115-F4): consequently the
                // `log.warn` above is NOT restorable — there is no `catch` arm to
                // hang it on, and emitting it unconditionally would put a
                // diagnostic on the SUCCESS path. S115-F4 is therefore WAIVED, not
                // CLOSED; every other anchor it cites is live.

                // The scrollbar is only emitted during draws so we also
                // check the scrollbar cache here and update if needed.
                // This is pretty fast.
                if !r.scrollbar.eql(critical.scrollbar) {
                    r.scrollbar = critical.scrollbar;
                    r.scrollbar_dirty = true;
                }

                // Update our background color
                r.uniforms.bg_color = [
                    r.terminal_state.colors.background.r,
                    r.terminal_state.colors.background.g,
                    r.terminal_state.colors.background.b,
                    (r.config.background_opacity * 255.0).round() as u8,
                ];

                // If we're on macOS and have glass styles, we remove
                // the background opacity because the glass effect handles
                // it.
                //
                // PORT NOTE (§C.1-#24): the reference guards this with `if (comptime
                // builtin.os.tag == .macos)`; the whole `macos_impl` module is already
                // `#[cfg(target_os = "macos")]`-gated, so the comptime guard collapses
                // to the bare `switch`.
                match r.config.background_blur {
                    BackgroundBlur::MacosGlassRegular | BackgroundBlur::MacosGlassClear => {
                        r.uniforms.bg_color[3] = 0
                    }
                    _ => {}
                }

                // Prepare our overlay image for upload (or unload). This
                // has to use our general allocator since it modifies
                // state that survives frames.
                // catch |err| log.warn("error updating overlay images err={}", .{err});
                if let Err(err) = r.images.overlay_update(r.alloc, r.overlay.as_ref()) {
                    log::warn!(target: LOG_SCOPE, "error updating overlay images err={err:?}");
                }

                // Update custom shader uniforms that depend on terminal state.
                r.update_custom_shader_uniforms_from_state();

                // PORT NOTE: reference defect (R13), UD352. The reads of the
                // resync below are taken here, before the unlock; the reference
                // takes them after it, racing the main thread's `drawFrame(sync)`
                // clearing `cells_rebuilt`. Reading here is the outcome where
                // that draw comes after the resync.
                (r.display_link_sync(), old_overlay, r.overlay.is_none())
            })
        };
        // UD363: keep the replaced overlay for the next rebuild to reuse, or,
        // with no overlay configured, release it as the reference does.
        // (`overlay_off` was read under the lock with the swap.)
        match old_overlay {
            Some(mut old) if overlay_off => old.deinit(),
            old => unsafe { (*self_ptr).overlay_spare = old },
        }

        // Start the display link now that the rebuilt frame is ready.
        // (`6688aa072`: the link is parked while idle, so the rebuild that
        // set `cells_rebuilt` is what un-parks it.)
        // self.syncDisplayLink(null, null);
        // (`draw_now`/`display_id` are null: only the start/stop tail, which
        // `update_frame_raw` applies once the caller's lock is released.)
        *link_sync = Some(sync);

        // Reset our dirty state after updating (the reference `defer
        // self.terminal_state.dirty = .false`, fired at fn scope exit — §C.4).
        unsafe { (*self_ptr).terminal_state.dirty = Dirty::False };

        Ok(())
    }

    /// Update uniforms that are based on the font grid.
    ///
    /// Caller must hold the draw mutex.
    //
    // Zig: fn updateFontGridUniforms(self) void — widened to `pub` for dead-code
    // exemption (no 3c caller; its caller `setFontGrid`/`Renderer::init` land later).
    pub fn update_font_grid_uniforms(&mut self) {
        self.uniforms.cell_size = [
            self.grid_metrics.cell_width as f32,
            self.grid_metrics.cell_height as f32,
        ];
    }

    /// Update uniforms that are based on the screen size.
    ///
    /// Caller must hold the draw mutex.
    //
    // Zig: fn updateScreenSizeUniforms(self) void — `pub` (no 3c caller).
    // PARITY: the reference multiplies the left/top padding by `-1`
    // (`-1 * @as(f32, @floatFromInt(self.size.padding.left))`); the `-1 *` form is
    // kept verbatim rather than the `-(x)` clippy rewrite.
    #[allow(clippy::neg_multiply)]
    pub fn update_screen_size_uniforms(&mut self) {
        // SAFETY: `self` is a unique reference, so its raw root is valid.
        unsafe { Self::update_screen_size_uniforms_raw(self) }
    }

    /// [`Self::update_screen_size_uniforms`] through a raw root, with narrow field access only.
    ///
    /// # Safety
    /// `self_ptr` is a valid, whole-`Renderer`-provenance pointer, and the
    /// caller holds the draw mutex (or is the only thread using the renderer).
    //
    // N437 / ruling 32 (P7-W7 T2 fix round 1): the body is `update_screen_size_uniforms_raw`, which
    // reaches the renderer only through narrow field places of a raw root, so
    // the main thread's sync draw (`display_callback` → `draw_frame_raw`) never
    // retags the whole renderer while the render thread works on its own fields
    // unlocked inside `update_frame_raw`.
    #[allow(clippy::neg_multiply)]
    pub unsafe fn update_screen_size_uniforms_raw(self_ptr: *mut Renderer) {
        unsafe {
            let terminal_size: ScreenSize = (*self_ptr).size.terminal();

            // Blank space around the grid.
            let blank: Padding = (*self_ptr)
                .size
                .screen
                .blank_padding(
                    (*self_ptr).size.padding,
                    GridSize {
                        columns: (*self_ptr).cells.size.columns,
                        rows: (*self_ptr).cells.size.rows,
                    },
                    CellSize {
                        width: (*self_ptr).grid_metrics.cell_width,
                        height: (*self_ptr).grid_metrics.cell_height,
                    },
                )
                .add((*self_ptr).size.padding);

            // Setup our uniforms
            (*self_ptr).uniforms.projection_matrix = math::ortho2d(
                -1.0 * (*self_ptr).size.padding.left as f32,
                (terminal_size.width + (*self_ptr).size.padding.right) as f32,
                (terminal_size.height + (*self_ptr).size.padding.bottom) as f32,
                -1.0 * (*self_ptr).size.padding.top as f32,
            );
            (*self_ptr).uniforms.grid_padding = [
                blank.top as f32,
                blank.right as f32,
                blank.bottom as f32,
                blank.left as f32,
            ];
            (*self_ptr).uniforms.screen_size = [
                (*self_ptr).size.screen.width as f32,
                (*self_ptr).size.screen.height as f32,
            ];
        }
    }

    /// Update the background image vertex buffer (CPU-side).
    ///
    /// This should be called if and when configs change that
    /// could affect the background image.
    ///
    /// Caller must hold the draw mutex.
    //
    // Zig: fn updateBgImageBuffer(self) void — `pub` (no 3c caller).
    pub fn update_bg_image_buffer(&mut self) {
        self.bg_image_buffer = BgImage {
            opacity: self.config.bg_image_opacity,
            info: BgImageInfo {
                position: match self.config.bg_image_position {
                    BackgroundImagePosition::TopLeft => BgImageInfoPosition::Tl,
                    BackgroundImagePosition::TopCenter => BgImageInfoPosition::Tc,
                    BackgroundImagePosition::TopRight => BgImageInfoPosition::Tr,
                    BackgroundImagePosition::CenterLeft => BgImageInfoPosition::Ml,
                    BackgroundImagePosition::CenterCenter | BackgroundImagePosition::Center => {
                        BgImageInfoPosition::Mc
                    }
                    BackgroundImagePosition::CenterRight => BgImageInfoPosition::Mr,
                    BackgroundImagePosition::BottomLeft => BgImageInfoPosition::Bl,
                    BackgroundImagePosition::BottomCenter => BgImageInfoPosition::Bc,
                    BackgroundImagePosition::BottomRight => BgImageInfoPosition::Br,
                },
                fit: match self.config.bg_image_fit {
                    BackgroundImageFit::Contain => BgImageInfoFit::Contain,
                    BackgroundImageFit::Cover => BgImageInfoFit::Cover,
                    BackgroundImageFit::Stretch => BgImageInfoFit::Stretch,
                    BackgroundImageFit::None => BgImageInfoFit::None,
                },
                repeat: self.config.bg_image_repeat,
            },
        };
        // Signal that the buffer was modified.
        self.bg_image_buffer_modified = self.bg_image_buffer_modified.wrapping_add(1);
    }

    /// Update custom shader uniforms that depend on terminal state.
    ///
    /// This should be called in `updateFrame` when terminal state changes.
    //
    // Zig: fn updateCustomShaderUniformsFromState(self) void — `pub` (called by
    // `update_frame` + directly unit-tested).
    pub fn update_custom_shader_uniforms_from_state(&mut self) {
        // We only need to do this if we have custom shaders.
        if !self.has_custom_shaders {
            return;
        }

        // Only update when terminal state is dirty.
        if self.terminal_state.dirty == Dirty::False {
            return;
        }

        let uniforms: &mut shadertoy::Uniforms = &mut self.custom_shader_uniforms;
        let colors: &render::Colors = &self.terminal_state.colors;

        // 256-color palette
        for (i, color) in colors.palette.iter().enumerate() {
            // S117-F1: `palette` is `[shadertoy::Vec4; 256]` (std140-forcing
            // newtype — see shadertoy.rs), hence the explicit wrap.
            uniforms.palette[i] = shadertoy::Vec4([
                color.r as f32 / 255.0,
                color.g as f32 / 255.0,
                color.b as f32 / 255.0,
                1.0,
            ]);
        }

        // Background color
        uniforms.background_color = [
            colors.background.r as f32 / 255.0,
            colors.background.g as f32 / 255.0,
            colors.background.b as f32 / 255.0,
            1.0,
        ];

        // Foreground color
        uniforms.foreground_color = [
            colors.foreground.r as f32 / 255.0,
            colors.foreground.g as f32 / 255.0,
            colors.foreground.b as f32 / 255.0,
            1.0,
        ];

        // Cursor color
        if let Some(cursor_color) = colors.cursor {
            uniforms.cursor_color = [
                cursor_color.r as f32 / 255.0,
                cursor_color.g as f32 / 255.0,
                cursor_color.b as f32 / 255.0,
                1.0,
            ];
        }

        // NOTE: the following could be optimized to follow a change in
        // config for a slight optimization however this is only 12 bytes
        // each being updated and likely isn't a cause for concern

        // Cursor text color
        //
        // PORT NOTE: reference defect (R13, UPSTREAM-DEFECTS.md UD57), for this
        // uniform and the two selection uniforms below. The reference reads
        // `.color.r/g/b` on a `?TerminalColor`, a union whose other tags are
        // `cell-foreground` and `cell-background`. Those are documented values
        // of `cursor-text`, `selection-foreground` and `selection-background`,
        // and the `cursor-invert-fg-bg` / `selection-invert-fg-bg`
        // compatibility keys set them. With any `custom-shader`, the access
        // panics in ReleaseSafe and is undefined behaviour in ReleaseFast. A
        // cell-relative value names no single colour for a whole-surface
        // uniform (the reference's own `TerminalColor.toTerminalRGB` returns
        // null for it), and the cell-path resolution needs a cell. So the port
        // writes each uniform only for a `.color` value and otherwise leaves
        // it unchanged, which is what the reference does when the option is
        // unset. The platform twin is `generic/linux_impl.rs`.
        if let Some(TerminalColor::Color(c)) = self.config.cursor_text {
            uniforms.cursor_text = [
                c.r as f32 / 255.0,
                c.g as f32 / 255.0,
                c.b as f32 / 255.0,
                1.0,
            ];
        }

        // Selection background color
        if let Some(TerminalColor::Color(c)) = self.config.selection_background {
            uniforms.selection_background_color = [
                c.r as f32 / 255.0,
                c.g as f32 / 255.0,
                c.b as f32 / 255.0,
                1.0,
            ];
        }

        // Selection foreground color
        if let Some(TerminalColor::Color(c)) = self.config.selection_foreground {
            uniforms.selection_foreground_color = [
                c.r as f32 / 255.0,
                c.g as f32 / 255.0,
                c.b as f32 / 255.0,
                1.0,
            ];
        }

        // Cursor visibility
        uniforms.cursor_visible = self.terminal_state.cursor.visible as i32;

        // Cursor style
        let cursor_style: cursor::Style =
            cursor::Style::from_terminal(self.terminal_state.cursor.visual_style);
        uniforms.previous_cursor_style = uniforms.current_cursor_style;
        uniforms.current_cursor_style = cursor_style as i32;
    }

    /// Update per-frame custom shader uniforms.
    ///
    /// This should be called exactly once per frame, inside `drawFrame`.
    //
    // Zig: fn updateCustomShaderUniformsForFrame(self) !void — the sole `try`
    // (`std.time.Instant.now()`) is infallible in Rust ⇒ `()`. `pub` (no 3c caller —
    // its caller is `drawFrame`/Chunk 4; directly unit-tested here). It issues ZERO
    // GPU calls (self-contained uniform math over `self.size` +
    // `self.cells.get_cursor_glyph()` + timing), so it lands cleanly in 3c.
    pub fn update_custom_shader_uniforms_for_frame(&mut self) {
        // SAFETY: `self` is a unique reference, so its raw root is valid.
        unsafe { Self::update_custom_shader_uniforms_for_frame_raw(self) }
    }

    /// [`Self::update_custom_shader_uniforms_for_frame`] through a raw root, with narrow field access only.
    ///
    /// # Safety
    /// `self_ptr` is a valid, whole-`Renderer`-provenance pointer, and the
    /// caller holds the draw mutex (or is the only thread using the renderer).
    //
    // N437 / ruling 32 (P7-W7 T2 fix round 1): the body is `update_custom_shader_uniforms_for_frame_raw`, which
    // reaches the renderer only through narrow field places of a raw root, so
    // the main thread's sync draw (`display_callback` → `draw_frame_raw`) never
    // retags the whole renderer while the render thread works on its own fields
    // unlocked inside `update_frame_raw`.
    pub unsafe fn update_custom_shader_uniforms_for_frame_raw(self_ptr: *mut Renderer) {
        unsafe {
            // We only need to do this if we have custom shaders.
            if !(*self_ptr).has_custom_shaders {
                return;
            }

            let now = Instant::now();
            // defer (*self_ptr).last_frame_time = now; (set as the LAST statement — §E.6)
            let first_frame_time = match (*self_ptr).first_frame_time {
                Some(t) => t,
                None => {
                    (*self_ptr).first_frame_time = Some(now);
                    now
                }
            };
            let last_frame_time = (*self_ptr).last_frame_time.unwrap_or(now);

            let u: &mut shadertoy::Uniforms = &mut (*self_ptr).custom_shader_uniforms;

            let since_ns: f32 = now.duration_since(first_frame_time).as_nanos() as f32;
            u.time = since_ns / 1_000_000_000.0;

            let delta_ns: f32 = now.duration_since(last_frame_time).as_nanos() as f32;
            u.time_delta = delta_ns / 1_000_000_000.0;

            u.frame += 1;

            let screen = (*self_ptr).size.screen;
            let padding = (*self_ptr).size.padding;
            let cell = (*self_ptr).size.cell;

            u.resolution = [screen.width as f32, screen.height as f32, 1.0];
            u.channel_resolution[0] = [screen.width as f32, screen.height as f32, 1.0, 0.0];

            if let Some(cursor) = (*self_ptr).cells.get_cursor_glyph() {
                let cursor_width: f32 = cursor.glyph_size[0] as f32;
                let cursor_height: f32 = cursor.glyph_size[1] as f32;

                // Left edge of the cell the cursor is in.
                //
                // PORT NOTE (§E.6 IMPLEMENTER FLAG): `cursor.grid_pos: [u16; 2]` but
                // `cell.{width,height}` / `padding.{left,top}` are `u32` — the reference
                // `grid_pos[0] * cell.width + padding.left` relies on Zig's implicit
                // widening. Rust has no implicit widening, so widen `grid_pos` to `u32`
                // BEFORE the `as f32`.
                //
                // PORT NOTE: reference defect (R13), UD76 (task R13-C2): the
                // product and the padding sum saturate. A saturated
                // `padding.left`/`top` (UD71) overflows the reference's
                // unchecked `u32` sum for any cursor off the first column or
                // row until the terminal is resized to fit it.
                let mut pixel_x: f32 = (cursor.grid_pos[0] as u32)
                    .saturating_mul(cell.width)
                    .saturating_add(padding.left) as f32;
                // Top edge, relative to the top of the
                // screen, of the cell the cursor is in.
                let mut pixel_y: f32 = (cursor.grid_pos[1] as u32)
                    .saturating_mul(cell.height)
                    .saturating_add(padding.top) as f32;

                // If +Y is up in our shaders, we need to flip the coordinate
                // so that it's instead the top edge of the cell relative to
                // the *bottom* of the screen.
                if !CUSTOM_SHADER_Y_IS_DOWN {
                    pixel_y = screen.height as f32 - pixel_y;
                }

                // Add the X bearing to get the -X (left) edge of the cursor.
                pixel_x += cursor.bearings[0] as f32;

                // How we deal with the Y bearing depends on which direction
                // is "up", since we want our final `pixel_y` value to be the
                // +Y edge of the cursor.
                if CUSTOM_SHADER_Y_IS_DOWN {
                    // As a reminder, the Y bearing is the distance from the
                    // bottom of the cell to the top of the glyph, so to get
                    // the +Y edge we need to add the cell height, subtract
                    // the Y bearing, and add the glyph height to get the +Y
                    // (bottom) edge of the cursor.
                    pixel_y += cell.height as f32;
                    pixel_y -= cursor.bearings[1] as f32;
                    pixel_y += cursor.glyph_size[1] as f32;
                } else {
                    // If the Y direction is reversed though, we instead want
                    // the *top* edge of the cursor, which means we just need
                    // to subtract the cell height and add the Y bearing.
                    pixel_y -= cell.height as f32;
                    pixel_y += cursor.bearings[1] as f32;
                }

                // S117-F1: `current_cursor`/`previous_cursor` are `shadertoy::Vec4`
                // (the std140-forcing newtype — see shadertoy.rs), so this local is
                // typed the same for the comparisons/assignments below to compile.
                let new_cursor: shadertoy::Vec4 =
                    shadertoy::Vec4([pixel_x, pixel_y, cursor_width, cursor_height]);
                let cursor_color: [f32; 4] = [
                    cursor.color[0] as f32 / 255.0,
                    cursor.color[1] as f32 / 255.0,
                    cursor.color[2] as f32 / 255.0,
                    cursor.color[3] as f32 / 255.0,
                ];

                let cursor_changed: bool =
                    new_cursor != u.current_cursor || cursor_color != u.current_cursor_color;

                if cursor_changed {
                    u.previous_cursor = u.current_cursor;
                    u.previous_cursor_color = u.current_cursor_color;
                    u.current_cursor = new_cursor;
                    u.current_cursor_color = cursor_color;
                    u.cursor_change_time = u.time;
                }
            }

            // Update focus uniforms
            u.focus = (*self_ptr).focused as i32;

            // If we need to update the time our focus state changed
            // then update it to our current frame time. This may not be
            // exactly correct since it is frame time, not exact focus
            // time, but focus time on its own isn't exactly correct anyways
            // since it comes async from a message.
            if (*self_ptr).custom_shader_focused_changed && (*self_ptr).focused {
                u.time_focus = u.time;
                (*self_ptr).custom_shader_focused_changed = false;
            }

            // defer (*self_ptr).last_frame_time = now; (Zig fn-scope-exit defer — §E.6)
            (*self_ptr).last_frame_time = Some(now);
        }
    }
}
