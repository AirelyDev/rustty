//! PORTS: ghostty src/renderer/generic.zig — the `Renderer` lifecycle and setter
//! cluster (`init`, `deinit`, `threadEnter`/`threadExit`, `loopEnter`/`loopExit`,
//! `setMacOSDisplayID`, `hasVsync`, `setFocus`, `setVisible`, `releaseGpuResources`,
//! `displayRealized`/`displayUnrealized`, `setFontGrid`, `setScreenSize`,
//! `changeConfig`, `prepBackgroundImage`).
//!
//! This file is part of the `Renderer(OpenGL)` arm (`generic::linux_impl`).

use super::*;

impl Renderer {
    // PORTS: generic.zig:658-819 `Renderer.init`; macos_impl generic.rs.
    //
    // The macOS `display_link`/vsync branch (generic.zig `else => null`) is the only
    // omission — see the module docs.
    //
    // P7-W7 T2 (`2f0b65346`): `api` is the reference's `OpenGL.init`, which owns a
    // surfaceless EGL context and releases it before returning, so this touches no
    // GL at all: the swap chain and the shaders are built lazily by the first
    // `drawFrame` on the render thread (`.swap_chain = null`, `.shaders = .uninit`,
    // `.reinitialize_shaders = true`). The port's two constructors (`init` over an
    // offscreen context the tests owned, `init_windowed` over GDK's borrowed GLArea
    // context) are one again.
    pub fn init(alloc: Global, options: Options) -> Result<Self, Error> {
        // Initialize our graphics API wrapper, this will prepare the
        // surface provided by the apprt and set up any API- and surface-
        // specific GPU resources.
        // var api = try GraphicsAPI.init(
        //     alloc,
        //     options.device,
        //     options,
        // );   (generic.zig:701-705, `40d5b860d`)
        // errdefer api.deinit();
        //
        // R7: `Options.device` must point to a live `Device` (see the field's contract);
        // this safe fn dereferences it below.
        assert!(!options.device.is_null());
        // SAFETY: `options.device` is the app-scoped render device, which outlives
        // every renderer (the core `App` frees it after all surfaces; `Options`).
        let api = unsafe { OpenGL::init(options.device, options.config.blending) }?;

        // const has_custom_shaders = options.config.custom_shaders.value.items.len > 0;
        let has_custom_shaders = !options.config.custom_shaders.value.is_empty();

        // Create the font shaper.
        // Zig: var font_shaper = try font.Shaper.init(alloc, .{ .features = ... });
        let font_shaper = Shaper::init(shape::Options {
            features: options.config.font_features.clone(),
        })?;

        // Initialize all the data that requires a critical font section. The
        // shared-read lock is LOAD-BEARING and MUST be mirrored (R1):
        // `grid.lock.lockShared()` before reading `grid.metrics`.
        let grid_metrics = {
            let g: *mut SharedGrid = options.font_grid;
            // SAFETY: `g` is the external `*mut SharedGrid` handle; `(*g).lock` /
            // `(*g).metrics` are shared reads through a raw pointer with no `&mut`
            // alias (the `set_font_grid` precedent, no SB hazard).
            let _read = unsafe { (*g).lock.read().unwrap() };
            unsafe { (*g).metrics }
        };

        // generic.zig:692-698: `const display_link = switch (…) { .macos => … else
        // => null }`. Off macOS the `else => null` arm applies — there is NO display
        // link, so the field + its errdefer are omitted entirely.

        // generic.zig:718-743 uniforms literal. Reference sets projection_matrix/
        // cell_size/grid_size/grid_padding/screen_size + cursor_color to `undefined`
        // (fixed by the `update_*` calls below / `update_frame`); every REMAINING
        // field to a concrete value. Rust has no `undefined`: start from zeroed POD
        // then set each field the reference sets. (macos_impl generic.rs:1959-1985.)
        let uniforms = {
            let mut u: Uniforms = unsafe { core::mem::zeroed() };
            u.padding_extend = UniformsPaddingExtend::default();
            u.min_contrast = options.config.min_contrast;
            u.cursor_pos = [u16::MAX, u16::MAX];
            u.bg_color = [
                options.config.background.r,
                options.config.background.g,
                options.config.background.b,
                (options.config.background_opacity * 255.0).round() as u8,
            ];
            u.bools = UniformsBools {
                cursor_wide: false,
                use_display_p3: options.config.colorspace == WindowColorspace::DisplayP3,
                use_linear_blending: options.config.blending.is_linear(),
                use_linear_correction: options.config.blending == AlphaBlending::LinearCorrected,
            };
            u
        };

        // generic.zig:744-772 custom_shader_uniforms literal: resolution = {0,0,1},
        // frame_rate = 60, focus = 1; every other field 0. (macos_impl generic.rs:1990-1996.)
        let custom_shader_uniforms = {
            let mut c: shadertoy::Uniforms = unsafe { core::mem::zeroed() };
            c.resolution = [0.0, 0.0, 1.0];
            c.frame_rate = 60.0;
            c.focus = 1;
            c
        };

        // .cells = .{} (generic.zig:717) — an EMPTY Contents, sized by the first
        // `update_frame`/`rebuild_cells`.
        let cells = Contents::default();

        let mut result = Self {
            alloc,
            config: options.config,
            // .surface_mailbox = options.surface_mailbox, (generic.zig:704)
            surface_mailbox: options.surface_mailbox,
            deferred_surface_pushes: Default::default(),
            grid_metrics,
            size: options.size,
            focused: true,
            scrollbar: Scrollbar::ZERO,
            scrollbar_dirty: false,
            last_bottom_node: None,
            last_bottom_y: 0,
            search_matches: None,
            search_selected_match: None,
            search_matches_dirty: false,
            cells,
            uniforms,
            custom_shader_uniforms,
            // .bg_image_buffer = undefined (773) — fixed by update_bg_image_buffer below.
            bg_image_buffer: unsafe { core::mem::zeroed() },
            font_grid: options.font_grid,
            font_shaper,
            font_shaper_cache: ShaperCache::init(),
            // Graphics API stuff
            api,
            swap_chain: None,
            has_custom_shaders,
            reinitialize_shaders: true,
            // Shaders are initialized lazily on the render thread.
            shaders: Shaders::uninit(),
            // ---- Zig struct-decl defaults (OMITTED by the 701-787 literal) ----
            latest_frame: Arc::new(LatestFrame::new()),
            visible: true,
            presentation_health: AtomicHealth::new(crate::Health::Healthy),
            display_realized: true,
            kitty_animation_clock: None,
            kitty_animation_next_ms: None,
            draw_mutex: DrawMutex::new(),
            custom_shader_focused_changed: false,
            cells_rebuilt: false,
            first_frame_time: None,
            last_frame_time: None,
            // macos_impl generic.rs:2274-2276. NOTE (R1): `generic.zig:189` declares
            // `bg_image_changed: bool = true`, but that field is WRITE-ONLY in the
            // oracle — the only other mention is a SHADOWING local in `changeConfig`
            // (:1881) — because `init` calls `prepBackgroundImage()` directly (:799)
            // rather than consulting it. The macOS twin therefore initializes it
            // `false`; follow the twin so the two arms cannot drift.
            images: image::State::empty(),
            bg_image: None,
            bg_image_changed: false,
            bg_image_buffer_modified: 0,
            health: AtomicHealth::new(crate::Health::Healthy),
            target_config_modified: 0,
            terminal_state: RenderState::empty(),
            terminal_state_frame_count: 0,
            overlay: None,
            overlay_spare: None,
        };

        // Ensure our undefined values above are correctly initialized.
        result.update_font_grid_uniforms(); // (792)
        result.update_screen_size_uniforms(); // (793)
        result.update_bg_image_buffer(); // (794)
        result.prep_background_image()?; // (795) — macos_impl generic.rs:2297.

        Ok(result)
    }

    // PORTS: generic.zig:822-850 `Renderer.deinit` (`2f0b65346`); macos_impl generic.rs.
    //
    // This only deinitializes and frees CPU-side state
    // and does not free GPU resources like the swap chain and
    // shaders. Those are freed with `releaseGpuResources`.
    //
    // Off-macOS: the `if (DisplayLink != void)` teardown collapses to nothing (that
    // field does not exist here — see the module docs). The GPU objects (swap chain,
    // shaders, images, background image) were freed by `thread_exit` on the render
    // thread, whose context was current; the images' CPU state goes with them there
    // (`generic.zig:903-912`).
    pub fn deinit(&mut self) {
        // if (self.overlay) |*overlay| overlay.deinit(self.alloc);
        // Port: `Overlay::deinit(&mut self)` takes NO allocator arg.
        if let Some(overlay) = &mut self.overlay {
            overlay.deinit();
        }
        // (UD363: the reused overlay the port keeps beside it.)
        if let Some(overlay) = &mut self.overlay_spare {
            overlay.deinit();
        }
        // self.terminal_state.deinit(self.alloc);
        self.terminal_state.deinit(self.alloc.into());
        // `search_selected_match`/`search_matches` arena teardown: the arena field
        // was collapsed (message.rs), so the owned `Option`s free via Rust `Drop`;
        // no explicit call.

        // self.latest_frame.deinit(global.io());
        self.latest_frame.deinit();

        // DisplayLink teardown — omitted (void off macOS).

        // self.cells.deinit(self.alloc); — takes NO allocator arg.
        self.cells.deinit();

        // self.font_shaper.deinit();
        self.font_shaper.deinit();
        // self.font_shaper_cache.deinit(self.alloc); — takes NO allocator arg.
        self.font_shaper_cache.deinit();

        // self.config.deinit();
        self.config.deinit();

        // self.api.deinit(); — the owned `OpenGL` (its `OffscreenContext`) releases
        // and destroys its EGL context on `Drop`: the reference's `OpenGL.deinit`
        // body, which has no ordering constraint against the teardown above. The
        // owner drops the whole renderer on the main thread right after this
        // `deinit` (core `RendererBox::dealloc_after_deinit`, P7-W7 T5b fix round
        // 2), where the reference's `deinit` also runs. A `&mut self` `deinit`
        // cannot move `api` out (`OpenGL::deinit` consumes `self`).
        //
        // self.* = undefined; — no-op (the owner drops the value next).
    }
}

impl Renderer {
    // =======================================================================
    // L4 SWAP (first light): the renderer-thread lifecycle + mailbox-driven
    // setter surface that core's `renderer_seam::Thread<A>` forwards to through
    // `(*self.renderer).0`. Copy-adapted from `macos_impl` (generic.rs) with the
    // Metal leaf swapped for the OpenGL leaf and `DisplayLink` collapsed: on Linux
    // `generic.zig:39-42`'s `DisplayLink` is `void` (macOS-only CVDisplayLink), so
    // every `if (comptime DisplayLink != void)` block degenerates (GTK drives the
    // frame clock, not a CVDisplayLink). Each `@hasDecl(GraphicsAPI, …)` probe is
    // resolved against the concrete `OpenGL` leaf (OpenGL.zig), cited per method.
    // =======================================================================

    // P7-W7 T2 (`2f0b65346`): `surfaceInit` and `finalizeSurfaceInit` are gone
    // from `generic.zig` (and `OpenGL.zig`'s GTK `prepareContext(null)` arm with
    // them): the renderer owns its context and nothing prepares a GL context on the
    // main thread any more.

    /// Callback called by renderer.Thread when it begins.
    //
    // PORTS: generic.zig:880-886 `threadEnter` (`2f0b65346`: `self: *Self`).
    //
    //     pub fn threadEnter(self: *Self, surface: *apprt.Surface) !void {
    //         // If our API has to do things on thread enter, let it.
    //         if (@hasDecl(GraphicsAPI, "threadEnter")) {
    //             try self.api.threadEnter(surface);
    //         }
    //     }
    //
    // `OpenGL` declares `threadEnter` (OpenGL.zig:218-227): make the context
    // current on the render thread and load this thread's GL function table.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn thread_enter<S>(&mut self, rt_surface: *mut S) -> Result<(), Error> {
        let _ = rt_surface;
        // If our API has to do things on thread enter, let it.
        self.api.thread_enter()?;
        Ok(())
    }

    /// Callback called by renderer.Thread when it exits. Called on the
    /// render thread. Releases all GPU resources before the API tears down
    /// its context, since after this the GL context will be gone.
    //
    // PORTS: generic.zig:888-916 `threadExit` (`2f0b65346`).
    //
    //     pub fn threadExit(self: *Self) void {
    //         {
    //             self.draw_mutex.lockUncancelable(global.io());
    //             defer self.draw_mutex.unlock(global.io());
    //             ...
    //         }
    //         // If our API has to do things on thread exit, let it.
    //         if (@hasDecl(GraphicsAPI, "threadExit")) {
    //             self.api.threadExit();
    //         }
    //     }
    //
    // The locked block is [`Self::thread_exit_locked`]; this entry locks
    // `self.draw_mutex` around it for the single-threaded callers (the tests).
    // The render thread calls the two halves through `renderer_seam` with its own
    // lock handle (N437).
    pub fn thread_exit(&mut self) {
        let mutex = self.draw_mutex.clone();
        mutex.lock();
        self.thread_exit_locked();
        mutex.unlock();

        self.thread_exit_api();
    }

    /// The `draw_mutex` block of [`Self::thread_exit`]. The caller holds
    /// `draw_mutex`.
    pub fn thread_exit_locked(&mut self) {
        // Release swap chain and shaders. Shaders are only freed
        // while unrealized and not every apprt unrealizes before
        // destroying a surface (macOS never does), so mark it
        // unrealized. This also stops any later draw.
        self.display_realized = false;
        self.release_gpu_resources();

        // We don't release images in `releaseGpuResources`
        // since it can be called whenever the terminal is
        // occluded or unrealized, and we don't want to
        // reupload images every time that happens.
        // self.images.deinit(self.alloc); self.images = .empty;
        //
        // ORDER IS LOAD-BEARING (GL): this DELETES every Kitty/overlay image's GL
        // texture, so it must run here, while the render thread's context is still
        // current — the `api.threadExit()` below releases it.
        self.images.deinit(self.alloc);
        self.images = image::State::empty();

        // if (self.bg_image) |img| { img.deinit(self.alloc); self.bg_image = null; }
        if let Some(img) = self.bg_image.take() {
            img.deinit(self.alloc);
        }
    }

    /// The tail of [`Self::thread_exit`], after the `draw_mutex` block.
    pub fn thread_exit_api(&mut self) {
        // If our API has to do things on thread exit, let it.
        // (`OpenGL` declares `threadExit`, OpenGL.zig:229-235: release the context
        // and drop this thread's GL table.)
        self.api.thread_exit();
    }

    /// Run our loop start callback if the renderer cares.
    //
    // PORTS: generic.zig:897-916 `loopEnter`; macos_impl generic.rs:2274-2291.
    // `@hasDecl(OpenGL, "loopEnter")` is FALSE (OpenGL declares no `loopEnter`), so
    // there is no api call; and `if (comptime DisplayLink == void) return;` fires
    // immediately on Linux. The `draw_now: Async` arg (the reference `thr.draw_now`
    // fed to the CVDisplayLink output callback) is unused with no display link.
    pub fn loop_enter(&mut self, draw_now: rustty_xev::Async) -> Result<(), Error> {
        let _ = draw_now;
        // `if (comptime DisplayLink == void) return;` — Linux: DisplayLink is void.
        Ok(())
    }

    /// Run our loop end callback if the renderer cares.
    //
    // PORTS: generic.zig:919-933 `loopExit`; macos_impl generic.rs:2301-2305.
    // `@hasDecl(OpenGL, "loopExit")` is FALSE ⇒ no api call; `DisplayLink == void` ⇒
    // no link to stop. Faithful no-op.
    pub fn loop_exit(&self) {}

    /// PORTS: generic.zig:1001-1009 `setMacOSDisplayID`; macos_impl
    /// generic.rs:2325-2334. `if (comptime DisplayLink == void) return;` fires
    /// immediately on Linux — there is no display link to retarget. Kept because
    /// core's `Thread<A>` drains the `.macos_display_id` mailbox message
    /// unconditionally (the reference's own shape); a no-op here.
    ///
    /// PORT NOTE (`a177ba90a`): the signature tracks the macOS arm's — `&mut
    /// self` and the `draw_now` handle — even though neither is used here. The
    /// mailbox drain in `rustty-core`'s `renderer_seam.rs` is SHARED across
    /// platforms and calls this unconditionally, so the two arms must agree.
    /// They had diverged: the macOS arm took `(id, draw_now)` while this one
    /// still took `(id)`, which broke the Linux build outright.
    pub fn set_macos_display_id(
        &mut self,
        id: u32,
        draw_now: rustty_xev::Async,
    ) -> Result<(), Error> {
        let _ = (id, draw_now);
        Ok(())
    }

    /// PORTS: generic.zig:1017-1024 `hasVsync`; macos_impl generic.rs:2342-2347.
    /// `if (comptime DisplayLink == void) return false;` — Linux has no
    /// CVDisplayLink-managed vsync, so the renderer never self-manages the frame
    /// clock; core's `drawFrame` therefore always draws when asked.
    pub fn has_vsync(&self) -> bool {
        false
    }

    /// PORTS: generic.zig:1029-1048 `setFocus`; macos_impl generic.rs:2360-2372.
    /// Keeps the reference `assert(self.focused != focus)` + the custom-shader flag.
    /// The `if (comptime DisplayLink != void)` link start/stop block is elided on
    /// Linux (DisplayLink is void).
    pub fn set_focus(&mut self, focus: bool) -> Result<(), Error> {
        assert!(self.focused != focus);
        self.focused = focus;
        self.custom_shader_focused_changed = true;
        // `if (comptime DisplayLink != void) link: { … }` — Linux: no display link.
        Ok(())
    }

    /// Callback when the window is visible or occluded.
    ///
    /// Must be called on the render thread.
    //
    // PORTS: generic.zig:1174-1188 `setVisible` (`2f0b65346`).
    //
    //     pub fn setVisible(self: *Self, visible: bool) void {
    //         self.visible = visible;
    //         self.syncDisplayLink(null, null);
    //
    //         // When we're hidden, release our GPU resources.
    //         if (!visible) {
    //             self.draw_mutex.lockUncancelable(global.io());
    //             defer self.draw_mutex.unlock(global.io());
    //             self.releaseGpuResources();
    //         }
    //     }
    //
    // `syncDisplayLink` returns at once off macOS (`DisplayLink == void`). The
    // pin's `comptime !apprt.must_draw_from_app_thread` gate, which folded the
    // release away for GTK, is gone: the render thread owns the context now.
    //
    // N437: the render thread holds `draw_mutex` for the whole call (it takes it
    // through its own handle before forming this `&mut self`), so this is the
    // locked form: the reference's own lock around the release is the caller's.
    pub fn set_visible_locked(&mut self, visible: bool) -> DisplayLinkSync {
        self.visible = visible;

        // When we're hidden, release our GPU resources.
        if !visible {
            self.release_gpu_resources();
        }

        DisplayLinkSync
    }

    /// Release the GPU resources we hold while the surface is not
    /// visible. Today this is the swap chain (render targets, font
    /// atlas texture copies, cell buffers, custom shader textures),
    /// which makes up nearly all of a surface's GPU memory usage.
    /// The swap chain is rebuilt on the next `drawFrame`.
    ///
    /// Note that images are NOT released here since we don't want
    /// to reupload images every time the terminal is brought back
    /// from being occluded or unrealized.
    ///
    /// Caller must lock the draw mutex before calling this function.
    /// Resources that are already released are skipped.
    //
    // PORTS: generic.zig:1190-1214 `releaseGpuResources` (`2f0b65346`).
    //
    //     pub fn releaseGpuResources(self: *Self) void {
    //         if (self.swap_chain) |*sc| {
    //             // Waits for any in-flight frames to complete, then
    //             // frees all GPU resources.
    //             sc.deinit();
    //             self.swap_chain = null;
    //         }
    //
    //         // Release the shaders as well if we're unrealized.
    //         if (!self.display_realized) {
    //             self.shaders.deinit(self.alloc);
    //         }
    //     }
    //
    // Runs on the render thread with its context current (`thread_exit`,
    // `set_visible`, `renderCallback`'s unrealized arm).
    pub fn release_gpu_resources(&mut self) {
        if let Some(sc) = &mut self.swap_chain {
            // Waits for any in-flight frames to complete, then
            // frees all GPU resources.
            sc.deinit();
            self.swap_chain = None;
        }

        // Release the shaders as well if we're unrealized.
        if !self.display_realized {
            self.shaders.deinit();
        }
    }

    /// Mark the full screen as dirty so that we redraw everything.
    //
    // Zig: `pub inline fn markDirty(self: *Self) void { self.terminal_state.dirty
    // = .full; }`; macos_impl generic.rs:2397-2399.
    pub fn mark_dirty(&mut self) {
        self.terminal_state.dirty = Dirty::Full;
    }

    /// Set the new font grid. Must be called on the render thread.
    //
    // PORTS: generic.zig `setFontGrid`; macos_impl generic.rs:2410-2463. Fully
    // graphics-API-agnostic (no DisplayLink / Metal), so this is a straight
    // copy-adapt. `defer self.draw_mutex.unlock()`: the body is INFALLIBLE (no `?`),
    // so the single unlock at the natural end is reached on every non-panic path.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn set_font_grid(&mut self, grid: *mut SharedGrid) {
        let mutex = self.draw_mutex.clone();
        mutex.lock();
        self.set_font_grid_locked(grid);
        mutex.unlock();
    }

    /// The body of [`Self::set_font_grid`] between its `lock()` and the
    /// `defer unlock()`; the caller holds `draw_mutex` (N437: the render thread
    /// takes it through its own handle before forming this `&mut self`).
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn set_font_grid_locked(&mut self, grid: *mut SharedGrid) {
        // Update our grid.
        self.font_grid = grid;

        // Update all our textures so that they sync on the next frame. We can
        // modify this without a lock because the GPU does not touch this data.
        // A released swap chain is rebuilt with fresh frames that sync all
        // textures on first use.
        // if (self.swap_chain) |*sc| for (&sc.frames) |*frame| { ... };
        if let Some(sc) = &mut self.swap_chain {
            for frame in &mut sc.frames {
                frame.grayscale_modified = 0;
                frame.color_modified = 0;
            }
        }

        // Get our metrics from the grid. This doesn't require a lock because the
        // metrics are never recalculated.
        //
        // SAFETY: `grid` is the external `*mut SharedGrid` handle (not reborrowed
        // from `&mut self`); `(*grid).metrics` is a shared read through a raw pointer
        // with no `&mut` alias to `*grid` (§4.4 — the shared-grid handle).
        let metrics = unsafe { (*grid).metrics };
        self.grid_metrics = metrics;

        // Reset our shaper cache. If our font changed (not just the size) then the
        // cached data may be invalid, so we always clear the cache just in case.
        let font_shaper_cache = ShaperCache::init();
        self.font_shaper_cache.deinit();
        self.font_shaper_cache = font_shaper_cache;

        // Update cell size.
        self.size.cell = CellSize {
            width: metrics.cell_width,
            height: metrics.cell_height,
        };

        // Update relevant uniforms.
        self.update_font_grid_uniforms();

        // Force a full rebuild, because cached rows may still reference an outdated
        // atlas from the old grid and this can cause garbage to be rendered.
        self.mark_dirty();
    }

    /// Resize the screen.
    //
    // PORTS: generic.zig:2210-2226 `setScreenSize` (`2f0b65346`); macos_impl
    // generic.rs.
    //
    //     pub fn setScreenSize(self: *Self, size: renderer.Size) void {
    //         self.draw_mutex.lockUncancelable(global.io());
    //         defer self.draw_mutex.unlock(global.io());
    //
    //         self.size = size;
    //         self.updateScreenSizeUniforms();
    //
    //         // Some graphics APIs need to manually update their viewport,
    //         // like OpenGL. Do so here.
    //         if (@hasDecl(GraphicsAPI, "setViewport")) {
    //             self.api.setViewport(self.size.screen.width, self.size.screen.height);
    //         }
    //
    //         log.debug("screen size size={}", .{size});
    //     }
    pub fn set_screen_size(&mut self, size: Size) {
        let mutex = self.draw_mutex.clone();
        mutex.lock();
        self.set_screen_size_locked(size);
        mutex.unlock();
    }

    /// The body of [`Self::set_screen_size`]; the caller holds `draw_mutex`
    /// (N437). Runs on the render thread: `setViewport` is a GL call on its
    /// context.
    pub fn set_screen_size_locked(&mut self, size: Size) {
        self.size = size;
        self.update_screen_size_uniforms();

        // Some graphics APIs need to manually update their viewport,
        // like OpenGL. Do so here.
        self.api
            .set_viewport(self.size.screen.width, self.size.screen.height);

        // log.debug("screen size size={}", .{size});
        log::debug!(target: LOG_SCOPE, "screen size size={size:?}");
    }

    /// Update the configuration.
    //
    // PORTS: generic.zig:... `changeConfig`; macos_impl generic.rs:2501-2622.
    // By-value ownership transfer (`DerivedConfig` is non-`Copy`): the caller's
    // `Message::ChangeConfig` payload holds a `Box<DerivedConfig>`, so ownership is
    // moved in (the reference `v.impl.deinit()` collapses into the old-config drop).
    //
    // (RESOLVED, w2 F-012) The reference `bg_image_changed` branch — which calls
    // `prepBackgroundImage()` to decode a new bg-image path via `crate::image` — used
    // to be OMITTED here because `crate::image` was macOS-only. It is a twin pair
    // now, so the decode branch is LIVE below, exactly as in `macos_impl`.
    //
    // `defer self.draw_mutex.unlock()` FIDELITY: the ported `Mutex` is bare
    // lock/unlock with NO RAII guard, so `defer` is replicated by explicit unlock on
    // every early return (unlock-before-`?`) plus the unlock at the natural end.
    pub fn change_config(&mut self, config: DerivedConfig) -> Result<(), Error> {
        let mutex = self.draw_mutex.clone();
        mutex.lock();
        let result = self.change_config_locked(config);
        mutex.unlock();
        result
    }

    /// The body of [`Self::change_config`]; the caller holds `draw_mutex`
    /// (N437), so the reference's `defer self.draw_mutex.unlock()` is the
    /// caller's on every exit.
    pub fn change_config_locked(&mut self, config: DerivedConfig) -> Result<(), Error> {
        // We always redo the font shaper in case font features changed. Easier and
        // rare enough to not cause performance issues.
        {
            let font_shaper = Shaper::init(shape::Options {
                features: config.font_features.clone(),
            })?;
            self.font_shaper.deinit();
            self.font_shaper = font_shaper;
        }

        // Reset the shaper cache so info from the previous font isn't reused.
        let font_shaper_cache = ShaperCache::init();
        self.font_shaper_cache.deinit();
        self.font_shaper_cache = font_shaper_cache;

        // Set our new minimum contrast.
        self.uniforms.min_contrast = config.min_contrast;

        // Set our new color space and blending.
        self.uniforms.bools.use_display_p3 = config.colorspace == WindowColorspace::DisplayP3;
        self.uniforms.bools.use_linear_blending = config.blending.is_linear();
        self.uniforms.bools.use_linear_correction =
            config.blending == AlphaBlending::LinearCorrected;

        let bg_image_config_changed = self.config.bg_image_fit != config.bg_image_fit
            || self.config.bg_image_position != config.bg_image_position
            || self.config.bg_image_repeat != config.bg_image_repeat
            || self.config.bg_image_opacity != config.bg_image_opacity;

        // Zig: `if (self.config.bg_image) |old| if (config.bg_image) |new|
        //          !old.equal(new) else true else config.bg_image != null`.
        // `Path` derives `PartialEq`, so `old != new` == `!old.equal(new)`.
        // macos_impl generic.rs:2779-2784.
        let bg_image_changed = match (&self.config.bg_image, &config.bg_image) {
            (Some(old), Some(new)) => old != new,
            (Some(_), None) => true,
            (None, Some(_)) => true,
            (None, None) => false,
        };

        let old_blending = self.config.blending;
        let custom_shaders_changed = self.config.custom_shaders != config.custom_shaders;

        // `self.config.deinit(); self.config = config.*;` — replace the config
        // (moving the new one in), then `deinit` + drop the old one.
        let mut old = mem::replace(&mut self.config, config);
        old.deinit();

        // If our background image path changed, prepare the new bg image.
        // if (bg_image_changed) try self.prepBackgroundImage();
        //
        // `prep_background_image` decodes the new path (wuffs PNG/JPEG) into a
        // pending `bg_image`; only a DECODE error propagates (open/read failures
        // warn + no-op). unlock-before-`?` (the `Mutex` is guard-less, so
        // `defer draw_mutex.unlock()` is replicated on every early return).
        // macos_impl generic.rs:2805-2810.
        if bg_image_changed {
            self.prep_background_image()?;
        }

        // If our background image config changed, update the vertex buffer.
        if bg_image_config_changed {
            self.update_bg_image_buffer();
        }

        // Reset our viewport to force a rebuild, in case of a font change.
        self.mark_dirty();

        let blending_changed = old_blending != self.config.blending;

        if blending_changed {
            // We update our API's blending mode.
            self.api.blending = self.config.blending;
            // And indicate that we need to reinitialize our shaders.
            self.reinitialize_shaders = true;
            // And that our swap-chain targets need re-creation for the new blending
            // mode. Zig `+%=` -> `wrapping_add`.
            self.target_config_modified = self.target_config_modified.wrapping_add(1);
        }

        if custom_shaders_changed {
            self.reinitialize_shaders = true;
        }

        Ok(())
    }

    /// Call this any time the background image path changes.
    ///
    /// Caller must hold the draw mutex.
    //
    // PORTS: generic.zig:1746-1830 `prepBackgroundImage`; macos_impl
    // generic.rs:2844-2971. FULLY graphics-API-agnostic (file read + wuffs decode +
    // an `image::Pending` hand-off) — copy-adapted VERBATIM from the Metal twin; the
    // only reason it was cut here before is that `crate::image` did not compile on
    // Linux.
    pub(super) fn prep_background_image(&mut self) -> Result<(), Error> {
        // Then we try to load the background image if we have a path.
        // if (self.config.bg_image) |p| load_background: { ... } else { ... }
        let Some(p) = self.config.bg_image.as_ref() else {
            // If we don't have a background image path, mark our
            // background image for unload if we currently have one.
            // if (self.bg_image) |*img| img.markForUnload();
            if let Some(img) = self.bg_image.as_mut() {
                img.mark_for_unload();
            }
            return Ok(());
        };

        // const path = switch (p) { .required, .optional => |slice| slice };
        //
        // Borrow discipline (same as the Metal twin): `p` borrows `self.config`
        // immutably, but the load block below mutates `self.bg_image`. Copy the
        // null-terminated path bytes into an OWNED `CString` here and drop the
        // `&self.config` borrow before touching `self.bg_image`. Purely mechanical;
        // no fidelity impact.
        let path: CString = p.slice().to_owned();

        // `self.alloc` is a Copy `Global` ZST; capture it so the `'load_background`
        // block reads it without holding a `&self` borrow across the later
        // `self.bg_image` mutation.
        let alloc = self.alloc;

        // 'load_background: { ... break :load_background; } — a labeled block whose
        // early exits (open-fail / read-fail / unknown-type) `break` with `None`
        // ("no change"); a DECODE error `?`-propagates OUT of the whole method
        // (the reference `try`), NOT a `break`.
        let pending: Option<image::Pending> = 'load_background: {
            // Open the file / Read it.
            //
            // The reference `openFileAbsolute` + `readToEndAlloc(self.alloc,
            // maxInt(u32))` (max size 4 GiB) are kept as TWO distinct steps here
            // (they were previously collapsed into one `std.fs.read`) because the
            // reference logs a DIFFERENT message for each — "error opening ..." vs
            // "error reading ..." — and R1 requires the message text verbatim. Both
            // failures map to `break :load_background` (a warn + no-op). The 4 GiB
            // cap is applied AFTER the read (an oversize file is treated as a read
            // failure, the reference's `error.StreamTooLong`), byte-faithfully over
            // the `CString` path bytes (never a lossy `String`).
            //
            // (task-14c-5: split to match `macos_impl`'s `generic.rs` twin, which
            // task-14c-1 split for the same R1 reason. Same two `break`s as before,
            // so decode and caller behaviour are unchanged.)
            //
            // `{s}` on the reference's `[]const u8` path prints the RAW bytes; the
            // closest Rust rendering is `String::from_utf8_lossy` over the same
            // bytes. The lossy conversion is confined to the log message — the path
            // the port actually opens stays the byte-faithful `OsStr`.
            let path_bytes = path.to_bytes();
            let path_display = String::from_utf8_lossy(path_bytes);
            // var file = std.fs.openFileAbsolute(path, .{}) catch |err| { log.warn(...); break; };
            #[cfg(not(windows))]
            let opened = std::fs::File::open(OsStr::from_bytes(path_bytes));
            // Windows: a path that is not WTF-8 is Zig's `error.InvalidWtf8` from
            // `openFileAbsolute`, which takes this same warn-and-break arm.
            #[cfg(windows)]
            let opened = match rustty_os::path::os_string_from_bytes(path_bytes) {
                Some(p) => std::fs::File::open(p),
                None => Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "InvalidWtf8",
                )),
            };
            let mut file = match opened {
                Ok(f) => f,
                Err(err) => {
                    // log.warn("error opening background image file \"{s}\": {}", .{ path, err });
                    log::warn!(
                        target: LOG_SCOPE,
                        "error opening background image file \"{path_display}\": {err:?}"
                    );
                    break 'load_background None;
                }
            };
            // Read it
            // const contents = file.readToEndAlloc(self.alloc, std.math.maxInt(u32))
            //     catch |err| { log.warn(...); break; };
            //   (`std.math.maxInt(u32), // Max size of 4 GiB, for now.`)
            // defer self.alloc.free(contents);
            let contents: Vec<u8> = {
                let mut buf: Vec<u8> = Vec::new();
                match std::io::Read::read_to_end(&mut file, &mut buf) {
                    Ok(_) if buf.len() <= u32::MAX as usize => buf,
                    // Over the reference's `maxInt(u32)` cap: the reference's
                    // `readToEndAlloc` fails with `error.StreamTooLong`. Zig's `{}`
                    // on an error value prints `error.` + the name, so the oracle's
                    // rendering of this arm is `...: error.StreamTooLong` — the
                    // prefix is part of the message text, not decoration.
                    Ok(_) => {
                        log::warn!(
                            target: LOG_SCOPE,
                            "error reading background image file \"{path_display}\": error.StreamTooLong"
                        );
                        break 'load_background None;
                    }
                    Err(err) => {
                        log::warn!(
                            target: LOG_SCOPE,
                            "error reading background image file \"{path_display}\": {err:?}"
                        );
                        break 'load_background None;
                    }
                }
            };

            // Figure out what type it probably is.
            // const file_type = switch (FileType.detect(contents)) {
            //     .unknown => FileType.guessFromExtension(std.fs.path.extension(path)),
            //     else => |t| t,
            // };
            let file_type = match FileType::detect(&contents) {
                FileType::Unknown => FileType::guessFromExtension(path_extension(path.to_bytes())),
                t => t,
            };

            // Decode it if we know how.
            // const image_data = switch (file_type) {
            //     .png => try wuffs.png.decode(self.alloc, contents),
            //     .jpeg => try wuffs.jpeg.decode(self.alloc, contents),
            //     .unknown => { log.warn("Cannot determine file type ...", .{path}); break; },
            //     else => |f| { log.warn("Unsupported file type {} ...", .{ f, path }); break; },
            // };
            let image_data = match file_type {
                FileType::Png => rustty_wuffs::png::decode(alloc, &contents)?,
                FileType::Jpeg => rustty_wuffs::jpeg::decode(alloc, &contents)?,
                FileType::Unknown => {
                    // log.warn("Cannot determine file type for background image file \"{s}\"!", .{path});
                    log::warn!(
                        target: LOG_SCOPE,
                        "Cannot determine file type for background image file \"{path_display}\"!"
                    );
                    break 'load_background None;
                }
                f => {
                    // log.warn("Unsupported file type {} for background image file \"{s}\"!", .{ f, path });
                    log::warn!(
                        target: LOG_SCOPE,
                        "Unsupported file type {f:?} for background image file \"{path_display}\"!"
                    );
                    break 'load_background None;
                }
            };

            // const image: imagepkg.Image = .{ .pending = .{ .width, .height,
            //     .pixel_format = .rgba, .data = image_data.data.ptr } };
            //
            // Transfer OWNERSHIP of the decoded RGBA buffer to `Pending` (do NOT
            // free it here — `rustty-wuffs` allocated it over the SAME `Global`,
            // align 1, so `image::linux_impl`'s `free_data` `Layout::from_size_align(
            // len, 1)` frees it later without a layout mismatch). `contents` (the
            // encoded bytes) IS freed here, at the block's end (`Vec` drop == the
            // reference `defer self.alloc.free(contents)`).
            let (width, height, buf, _len) = image_data.into_raw();
            Some(image::Pending {
                width,
                height,
                pixel_format: image::PixelFormat::Rgba,
                data: buf.as_ptr(), // *mut u8 — freed later by Image::deinit / free_data
            })
        };

        // If we have an existing background image, replace it.
        // Otherwise, set this as our background image directly. A `None` pending
        // is the `break :load_background` case — leave `self.bg_image` unchanged.
        if let Some(pend) = pending {
            let img = image::Image::Pending(pend);
            // if (self.bg_image) |*img| { img.markForReplace(self.alloc, image); }
            // else { self.bg_image = image; }
            if let Some(existing) = self.bg_image.as_mut() {
                existing.mark_for_replace(self.alloc, img);
            } else {
                self.bg_image = Some(img);
            }
        }

        Ok(())
    }
}

impl Renderer {
    /// This is called by the GTK apprt after the surface is
    /// reinitialized (e.g. after the widget is re-realized following
    /// a display change or reparenting).
    //
    // PORTS: generic.zig:947-968 `displayRealized` (`2f0b65346`).
    //
    // ```zig
    // pub fn displayRealized(self: *Self) !void {
    //     // If our API has to do things on realize, let it.
    //     if (@hasDecl(GraphicsAPI, "displayRealized")) {
    //         self.api.displayRealized();
    //     }
    //
    //     // Lock the draw mutex so that we can safely update state.
    //     self.draw_mutex.lockUncancelable(global.io());
    //     defer self.draw_mutex.unlock(global.io());
    //
    //     // Mark the display as realized. ...
    //     self.display_realized = true;
    //     self.reinitialize_shaders = true;
    //     self.target_config_modified = 1;
    // }
    // ```
    //
    // `OpenGL` no longer declares `displayRealized` (its GTK `prepareContext(null)`
    // arm went with the GLArea), so the `@hasDecl` arm is statically false. The
    // lock is the caller's: the main thread calls this through `renderer_seam` with
    // its own `draw_mutex` handle (N437).
    //
    // PORT NOTE: the inferred `!void` has an empty error set now; the `Result`
    // stays so the core seam's `try` reads as the reference's.
    pub fn display_realized_locked(&mut self) -> Result<(), Error> {
        // Mark the display as realized. The render thread will lazily
        // rebuild the swap chain and shaders on the next `drawFrame`,
        // which is the right place for GL resource creation (it
        // guarantees a current context on the render thread).
        self.display_realized = true;
        self.reinitialize_shaders = true;
        self.target_config_modified = 1;
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
    // PORTS: generic.zig:970-985 `displayUnrealized` (`2f0b65346`).
    //
    // ```zig
    // pub fn displayUnrealized(self: *Self) void {
    //     // Lock the draw mutex so that we can safely update state.
    //     self.draw_mutex.lockUncancelable(global.io());
    //     defer self.draw_mutex.unlock(global.io());
    //
    //     // Clearing `display_realized` ensures drawFrame doesn't attempt
    //     // to rebuild the swap chain or make any graphics API calls.
    //     // The actual GPU resource release is done by the render thread.
    //     self.display_realized = false;
    // }
    // ```
    //
    // The lock is the caller's (N437), as for `display_realized_locked`.
    pub fn display_unrealized_locked(&mut self) {
        // Clearing `display_realized` ensures drawFrame doesn't attempt
        // to rebuild the swap chain or make any graphics API calls.
        // The actual GPU resource release is done by the render thread.
        self.display_realized = false;
    }
}

impl Renderer {
    /// GL-ATTACH (windowed present — DEFERRED STUB). Parallel to `macos_impl`'s
    /// `attach_host_view` (generic.rs:2218-2233) but no-op: the windowed
    /// `GtkGLArea` context hookup is L5's job (the "GL-ATTACH" deferral, matching
    /// Task 8's `present` deferral). Offscreen backends never attach a live view.
    #[allow(dead_code)]
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn attach_host_view(
        &mut self,
        _view: *mut core::ffi::c_void,
        _scale: f64,
    ) -> Result<(), Error> {
        Ok(())
    }
}
