//! PORTS: ghostty src/renderer/generic.zig — Chunk 3c, the frame orchestrator and
//! uniforms math (`updateFrame`, `updateFontGridUniforms`, `updateScreenSizeUniforms`,
//! `updateBgImageBuffer`, `updateCustomShaderUniformsFromState`,
//! `updateCustomShaderUniformsForFrame`).
//!
//! This file is part of the `Renderer(OpenGL)` arm (`generic::linux_impl`).

use super::*;

impl Renderer {
    /// Update uniforms that are based on the font grid.
    // PORTS: generic.zig `updateFontGridUniforms`; macos_impl generic.rs:4811-4816.
    pub fn update_font_grid_uniforms(&mut self) {
        self.uniforms.cell_size = [
            self.grid_metrics.cell_width as f32,
            self.grid_metrics.cell_height as f32,
        ];
    }

    /// Update uniforms that are based on the screen size.
    // PORTS: generic.zig `updateScreenSizeUniforms`; macos_impl generic.rs:4827-4864.
    // PARITY: the reference multiplies left/top padding by `-1`; the `-1 *` form is
    // kept verbatim rather than the `-(x)` clippy rewrite.
    #[allow(clippy::neg_multiply)]
    pub fn update_screen_size_uniforms(&mut self) {
        let terminal_size: ScreenSize = self.size.terminal();

        // Blank space around the grid.
        let blank: Padding = self
            .size
            .screen
            .blank_padding(
                self.size.padding,
                GridSize {
                    columns: self.cells.size.columns,
                    rows: self.cells.size.rows,
                },
                CellSize {
                    width: self.grid_metrics.cell_width,
                    height: self.grid_metrics.cell_height,
                },
            )
            .add(self.size.padding);

        self.uniforms.projection_matrix = math::ortho2d(
            -1.0 * self.size.padding.left as f32,
            (terminal_size.width + self.size.padding.right) as f32,
            (terminal_size.height + self.size.padding.bottom) as f32,
            -1.0 * self.size.padding.top as f32,
        );
        self.uniforms.grid_padding = [
            blank.top as f32,
            blank.right as f32,
            blank.bottom as f32,
            blank.left as f32,
        ];
        self.uniforms.screen_size = [
            self.size.screen.width as f32,
            self.size.screen.height as f32,
        ];
    }

    /// Update the background image vertex buffer (CPU-side).
    // PORTS: generic.zig `updateBgImageBuffer`; macos_impl generic.rs:4874-4902.
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
        self.bg_image_buffer_modified = self.bg_image_buffer_modified.wrapping_add(1);
    }
}

impl Renderer {
    /// Update the frame data.
    //
    // PORTS: generic.zig:1123-… `updateFrame`; macos_impl generic.rs:4424-4803.
    // Zig `pub fn updateFrame(self, state: *renderer.State, cursor_blink_visible:
    // bool) Allocator.Error!void`; `state: *renderer.State` → `&state::State` (never
    // mutates a `State` field — only locks `state.mutex` + mutates THROUGH the stored
    // `*mut Terminal`). The sole propagated `?` is `terminal_state.update` →
    // `VtOutOfMemory`.
    //
    // HISTORY (both L4 scope cuts are now RESOLVED; nothing here is elided):
    //   1. RESOLVED (w2 F-005) — the Kitty-graphics image state
    //      (`self.images.kitty_requires_update`/`kitty_update`) is LIVE below;
    //      `crate::image` is a twin pair now, so `self.images` is a real field.
    //   2. RESOLVED (w2 F-020) — the inspector debug-overlay chain (`state.inspector`
    //      → `overlay_features` → `rebuild_overlay`, macos_impl generic.rs:4856-4873,
    //      :4959) and the overlay image upload (`self.images.overlay_update`,
    //      :5026-5028) are LIVE below.
    //   3. NOT A CUT — the macOS-only glass-background alpha zero-out (macos_impl
    //      generic.rs:5007-5021) is guarded by `if (comptime builtin.os.tag ==
    //      .macos)` in generic.zig; on Linux that branch is COMPILED OUT, so its
    //      absence here IS the faithful port.
    // Every other line is copy-adapted verbatim from `macos_impl`.
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

        // We fully deinit and reset the terminal state every so often so that a
        // particularly large terminal state doesn't cause the renderer to hold on to
        // retained memory. Frame count is ~12 minutes at 120Hz.
        let max_terminal_state_frame_count = 100_000;
        if *terminal_state_frame_count >= max_terminal_state_frame_count {
            terminal_state.deinit(alloc.into());
            *terminal_state = RenderState::empty();
            *terminal_state_frame_count = 0;
        }
        *terminal_state_frame_count += 1;

        // Data we extract out of the critical area.
        struct Critical {
            links: CellSet,
            // macos_impl §C.5: written but never READ after the block (post-block code
            // reads `state.mouse.point/.mods` directly). Mirrored (R1) write-only.
            #[allow(dead_code)]
            mouse: state::Mouse,
            preedit: Option<state::Preedit>,
            scrollbar: Scrollbar,
            overlay_features: Vec<crate::overlay::Feature>,
            /// `self.size`, copied under the draw mutex for the overlay rebuild (UD363).
            size: Size,
        }

        // The renderer-thread state carries its terminal behind a `*mut Terminal` (a
        // separate allocation, disjoint from `self`). Copy the stored raw pointer and
        // re-deref it FRESH at every use, matching the Zig `state.terminal.…` re-walk.
        let t = state.terminal; // *mut Terminal

        // Hand-rolled `defer state.unlockDemand()` (macos_impl §C.4/RISK-5): the
        // guard-less `Mutex` has no RAII drop, and the lock is held across THREE exits
        // — the sync-output early `return`, the `terminal_state.update(...)?`
        // propagation, and normal fall-through. This `Drop` fires on all three.
        //
        // (D4, oracle d34b54e9b: the guard now borrows the whole `State` because the
        // release path is `unlock_demand`, not a bare `mutex.unlock` — it must also
        // bump the handoff generation and wake parked yielders. A shared `&State` is
        // disjoint from the `&mut self` uses inside the block, exactly as the bare
        // `&Mutex` was.)
        struct Unlock<'a>(&'a state::State);
        impl Drop for Unlock<'_> {
            fn drop(&mut self) {
                // SAFETY: same as the `lock_demand` below — `state.mutex` is the live
                // shared render mutex, held by this frame.
                unsafe { self.0.unlock_demand() };
            }
        }

        // Update all our data as tightly as possible within the mutex.
        let mut critical: Critical = {
            // Lock while signaling demand so the IO parse thread can't starve us.
            // See renderer::State::lock_demand.
            //
            // SAFETY: `state.mutex` points at the live shared render-state mutex and
            // this frame does not already hold it.
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
            // changed by comparing the bottom-right pin. Update this BEFORE we update
            // our render state so we can draw the new scrolled data immediately.
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

            // Update our terminal state
            terminal_state.update(alloc.into(), unsafe { &mut *t })?;

            // If our terminal state is dirty at all we need to redo the viewport search.
            if terminal_state.dirty != Dirty::False {
                unsafe {
                    (*t).flags.search_viewport_dirty = true;
                }
            }

            // Get our scrollbar out of the terminal (re-read `active` FRESH after the
            // `&mut *t` update above).
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
            // (the macos twin carries the verbatim Zig quote — API-free code,
            // copied verbatim.)
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
            //
            // PORTS: generic.zig:1229-1248; macos_impl generic.rs:4813-4833.
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
            let links: CellSet = match state.mouse.point {
                None => CellSet::new(),
                Some(vp) => {
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

            // PORTS: generic.zig:1269-1283; macos_impl generic.rs:4856-4873.
            //   const insp = state.inspector orelse break :overlay &.{};
            //   const renderer_info = insp.rendererInfo();
            //   break :overlay renderer_info.overlayFeatures(arena_alloc) catch &.{};
            //
            // R2 ACCESSOR-TRAIT SEAM: `state.inspector` is the renderer-owned
            // `*mut dyn InspectorOverlay` fat pointer (state.rs) — the acyclic
            // accessor trait (INV-7). The renderer reads the enabled overlay features
            // THROUGH the trait, never naming a `rustty-inspector` type. The
            // reference's `insp.rendererInfo()` navigation happens at STORE time
            // (core coerces the `Info` into the fat pointer), so here the one-method
            // trait call subsumes the whole chain. INV-1: this deref is inside the
            // `state.mutex`-guarded critical section; the raw pointer never escapes
            // the guard.
            let overlay_features: Vec<crate::overlay::Feature> = match state.inspector {
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

        // Outside the critical area we can update our links to contain our regex
        // results.
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

            // NOTE: The order below matters. Highlights added earlier take priority.
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
        let (old_overlay, overlay_off) = unsafe {
            Self::with_locked(self_ptr, lock, move |r| {
                // PORT NOTE: reference defect (R13), UD363. The overlay was built
                // into `next_overlay` above, without the lock (as the reference
                // rebuilds it); only the swap into `self.overlay` happens here.
                let old_overlay = core::mem::replace(&mut r.overlay, next_overlay);

                // Build our GPU cells. `critical.preedit` is MOVED into `rebuild_cells`,
                // but `critical.preedit.is_some()` is read FIRST for `cursor::style`.
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
                //
                // LOG PORT NOTE (task-14c-5, N9 — Linux-side residual of the WAIVED row
                // S115-F4): `rebuild_cells` is INFALLIBLE in rustty (it returns `()`;
                // its sole OOM source aborts via the `Global` allocator), so the
                // reference's "continue with frozen contents" catch path is unreachable
                // here and this `log.warn` is NOT restorable — emitting it
                // unconditionally would put a diagnostic on the SUCCESS path. Same
                // ruling as the Metal twin (task-14c-1, generic.rs:5412-5427). See
                // ADJUDICATION.md#s115-f4.

                // The scrollbar is only emitted during draws so we also check the
                // scrollbar cache here and update if needed.
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

                // NOT A CUT (macOS glass background): macos_impl generic.rs:5007-5021 is
                // guarded by `if (comptime builtin.os.tag == .macos)` in generic.zig;
                // that branch is COMPILED OUT on Linux, so it is absent here by fidelity.

                // Prepare our overlay image for upload (or unload). This has to use our
                // general allocator since it modifies state that survives frames.
                // self.images.overlayUpdate(self.alloc, self.overlay) catch |err|
                //     log.warn("error updating overlay images err={}", .{err});
                //
                // PORTS: generic.zig:1416-1424; macos_impl generic.rs:5026-5028.
                // (`Option<&Overlay>` — the borrowed-ownership deviation documented on
                //  `image::linux_impl::State::overlay_update`.)
                if let Err(err) = r.images.overlay_update(r.alloc, r.overlay.as_ref()) {
                    log::warn!(target: LOG_SCOPE, "error updating overlay images err={err:?}");
                }

                // Update custom shader uniforms that depend on terminal state.
                // self.updateCustomShaderUniformsFromState();
                r.update_custom_shader_uniforms_from_state();
                (old_overlay, r.overlay.is_none())
            })
        };
        // UD363: keep the replaced overlay for the next rebuild to reuse, or,
        // with no overlay configured, release it as the reference does.
        // (`overlay_off` was read under the lock with the swap.)
        match old_overlay {
            Some(mut old) if overlay_off => old.deinit(),
            old => unsafe { (*self_ptr).overlay_spare = old },
        }

        // Reset our dirty state after updating (the reference `defer
        // self.terminal_state.dirty = .false`, fired at fn scope exit — §C.4).
        unsafe { (*self_ptr).terminal_state.dirty = Dirty::False };

        Ok(())
    }

    /// Update custom shader uniforms that depend on terminal state.
    ///
    /// This should be called in `updateFrame` when terminal state changes.
    //
    // PORTS: generic.zig `updateCustomShaderUniformsFromState`; copy-adapted verbatim
    // from macos_impl generic.rs:4926-5041 (widened to `pub` there for direct unit
    // tests; kept `pub` here for the same reason).
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
        // `.color.r/g/b` on a `?TerminalColor` whose other tags,
        // `cell-foreground` and `cell-background`, are documented values of these
        // options; with any `custom-shader` it panics in ReleaseSafe. A
        // cell-relative value names no single colour for a whole-surface uniform,
        // so the port writes each uniform only for a `.color` value and otherwise
        // leaves it unchanged, as the reference does when the option is unset.
        // The full rationale is at the twin site in `generic.rs`
        // (`update_custom_shader_uniforms_from_state`).
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
    // PORTS: generic.zig `updateCustomShaderUniformsForFrame`; copy-adapted verbatim
    // from macos_impl generic.rs:5052-5169 (the sole `try` — `std.time.Instant.now()`
    // — is infallible in Rust ⇒ `()`). `CUSTOM_SHADER_Y_IS_DOWN` is `false` here
    // (`OpenGL.zig:27-28`; Metal's is `true`), so the flipped-Y branches are LIVE.
    pub fn update_custom_shader_uniforms_for_frame(&mut self) {
        // We only need to do this if we have custom shaders.
        if !self.has_custom_shaders {
            return;
        }

        let now = Instant::now();
        // defer self.last_frame_time = now; (set as the LAST statement — §E.6)
        let first_frame_time = match self.first_frame_time {
            Some(t) => t,
            None => {
                self.first_frame_time = Some(now);
                now
            }
        };
        let last_frame_time = self.last_frame_time.unwrap_or(now);

        let u: &mut shadertoy::Uniforms = &mut self.custom_shader_uniforms;

        let since_ns: f32 = now.duration_since(first_frame_time).as_nanos() as f32;
        u.time = since_ns / 1_000_000_000.0;

        let delta_ns: f32 = now.duration_since(last_frame_time).as_nanos() as f32;
        u.time_delta = delta_ns / 1_000_000_000.0;

        u.frame += 1;

        let screen = self.size.screen;
        let padding = self.size.padding;
        let cell = self.size.cell;

        u.resolution = [screen.width as f32, screen.height as f32, 1.0];
        u.channel_resolution[0] = [screen.width as f32, screen.height as f32, 1.0, 0.0];

        if let Some(cursor) = self.cells.get_cursor_glyph() {
            let cursor_width: f32 = cursor.glyph_size[0] as f32;
            let cursor_height: f32 = cursor.glyph_size[1] as f32;

            // Left edge of the cell the cursor is in.
            //
            // PORT NOTE (macos_impl §E.6): `cursor.grid_pos: [u16; 2]` but
            // `cell.{width,height}` / `padding.{left,top}` are `u32` — the reference
            // `grid_pos[0] * cell.width + padding.left` relies on Zig's implicit
            // widening. Rust has no implicit widening, so widen `grid_pos` to `u32`
            // BEFORE the `as f32`.
            //
            // PORT NOTE: reference defect (R13), UD76 (task R13-C2): saturating,
            // as in `generic.rs` (macOS).
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
        u.focus = self.focused as i32;

        // If we need to update the time our focus state changed
        // then update it to our current frame time. This may not be
        // exactly correct since it is frame time, not exact focus
        // time, but focus time on its own isn't exactly correct anyways
        // since it comes async from a message.
        if self.custom_shader_focused_changed && self.focused {
            u.time_focus = u.time;
            self.custom_shader_focused_changed = false;
        }

        // defer self.last_frame_time = now; (Zig fn-scope-exit defer — §E.6)
        self.last_frame_time = Some(now);
    }
}
