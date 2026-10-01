//! PORTS: ghostty src/renderer/generic.zig — the `Renderer` lifecycle and setter
//! cluster (`init`, `deinit`, `threadEnter`/`threadExit`, `loopEnter`/`loopExit`,
//! `setMacOSDisplayID`, `hasVsync`, `setFocus`, `setVisible`, `releaseGpuResources`,
//! `syncDisplayLink`, `setFontGrid`, `setScreenSize`, `changeConfig`,
//! `prepBackgroundImage`).
//!
//! This file is part of the macOS `Renderer(Metal)` arm (`generic::macos_impl`).

use super::*;

impl Renderer {
    // DEFERRED SEAM (the draw path — a later chunk). `Renderer::init` needs
    // `GraphicsAPI::init(options)` over a LIVE surface (the apprt/windowing-host
    // seam — `Metal::init` is likewise deferred, `metal_impl.rs`), reads
    // `options.config.custom_shaders` (a deferred `RepeatablePath` placeholder,
    // §B.1), constructs `font.Shaper.init`, and calls the deferred CPU methods
    // (`initShaders`/`updateFontGridUniforms`/`updateScreenSizeUniforms`/
    // `updateBgImageBuffer`/`prepBackgroundImage`). The reference body is
    // preserved verbatim (R1/R5); it lands — un-commented — with those methods.
    //
    // pub fn init(alloc: Allocator, options: renderer.Options) !Self {
    //     // Initialize our graphics API wrapper, this will prepare the
    //     // surface provided by the apprt and set up any API-specific
    //     // GPU resources.
    //     var api = try GraphicsAPI.init(alloc, options);
    //     errdefer api.deinit();
    //
    //     const has_custom_shaders = options.config.custom_shaders.value.items.len > 0;
    //
    //     // Prepare our swap chain
    //     var swap_chain = try SwapChain.init(
    //         api,
    //         has_custom_shaders,
    //     );
    //     errdefer swap_chain.deinit();
    //
    //     // Create the font shaper.
    //     var font_shaper = try font.Shaper.init(alloc, .{
    //         .features = options.config.font_features.items,
    //     });
    //     errdefer font_shaper.deinit();
    //
    //     // Initialize all the data that requires a critical font section.
    //     const font_critical: struct {
    //         metrics: font.Metrics,
    //     } = font_critical: {
    //         const grid: *font.SharedGrid = options.font_grid;
    //         grid.lock.lockShared();
    //         defer grid.lock.unlockShared();
    //         break :font_critical .{
    //             .metrics = grid.metrics,
    //         };
    //     };
    //
    //     const display_link: ?DisplayLink = switch (builtin.os.tag) {
    //         .macos => if (options.config.vsync)
    //             try macos.video.DisplayLink.createWithActiveCGDisplays()
    //         else
    //             null,
    //         else => null,
    //     };
    //     errdefer if (display_link) |v| v.release();
    //
    //     var result: Self = .{
    //         .alloc = alloc,
    //         .config = options.config,
    //         .surface_mailbox = options.surface_mailbox,
    //         .grid_metrics = font_critical.metrics,
    //         .size = options.size,
    //         .focused = true,
    //         .scrollbar = .zero,
    //         .scrollbar_dirty = false,
    //         .last_bottom_node = null,
    //         .last_bottom_y = 0,
    //         .search_matches = null,
    //         .search_selected_match = null,
    //         .search_matches_dirty = false,
    //
    //         // Render state
    //         .cells = .{},
    //         .uniforms = .{
    //             .projection_matrix = undefined,
    //             .cell_size = undefined,
    //             .grid_size = undefined,
    //             .grid_padding = undefined,
    //             .screen_size = undefined,
    //             .padding_extend = .{},
    //             .min_contrast = options.config.min_contrast,
    //             .cursor_pos = .{ std.math.maxInt(u16), std.math.maxInt(u16) },
    //             .cursor_color = undefined,
    //             .bg_color = .{
    //                 options.config.background.r,
    //                 options.config.background.g,
    //                 options.config.background.b,
    //                 // Note that if we're on macOS with glass effects
    //                 // we'll disable background opacity but we handle
    //                 // that in updateFrame.
    //                 @intFromFloat(@round(options.config.background_opacity * 255.0)),
    //             },
    //             .bools = .{
    //                 .cursor_wide = false,
    //                 .use_display_p3 = options.config.colorspace == .@"display-p3",
    //                 .use_linear_blending = options.config.blending.isLinear(),
    //                 .use_linear_correction = options.config.blending == .@"linear-corrected",
    //             },
    //         },
    //         .custom_shader_uniforms = .{
    //             .resolution = .{ 0, 0, 1 },
    //             .time = 0,
    //             .time_delta = 0,
    //             .frame_rate = 60, // not currently updated
    //             .frame = 0,
    //             .channel_time = @splat(@splat(0)), // not currently updated
    //             .channel_resolution = @splat(@splat(0)),
    //             .mouse = @splat(0), // not currently updated
    //             .date = @splat(0), // not currently updated
    //             .sample_rate = 0, // N/A, we don't have any audio
    //             .current_cursor = @splat(0),
    //             .previous_cursor = @splat(0),
    //             .current_cursor_color = @splat(0),
    //             .previous_cursor_color = @splat(0),
    //             .current_cursor_style = 0,
    //             .previous_cursor_style = 0,
    //             .cursor_visible = 0,
    //             .cursor_change_time = 0,
    //             .time_focus = 0,
    //             .focus = 1, // assume focused initially
    //             .palette = @splat(@splat(0)),
    //             .background_color = @splat(0),
    //             .foreground_color = @splat(0),
    //             .cursor_color = @splat(0),
    //             .cursor_text = @splat(0),
    //             .selection_background_color = @splat(0),
    //             .selection_foreground_color = @splat(0),
    //         },
    //         .bg_image_buffer = undefined,
    //
    //         // Fonts
    //         .font_grid = options.font_grid,
    //         .font_shaper = font_shaper,
    //         .font_shaper_cache = font.ShaperCache.init(),
    //
    //         // Shaders (initialized below)
    //         .shaders = undefined,
    //
    //         // Graphics API stuff
    //         .api = api,
    //         .swap_chain = swap_chain,
    //         .display_link = display_link,
    //     };
    //
    //     try result.initShaders();
    //
    //     // Ensure our undefined values above are correctly initialized.
    //     result.updateFontGridUniforms();
    //     result.updateScreenSizeUniforms();
    //     result.updateBgImageBuffer();
    //     try result.prepBackgroundImage();
    //
    //     return result;
    // }
    pub fn init(alloc: Global, options: Options) -> Result<Self, Error> {
        // Initialize our graphics API wrapper, this will prepare the
        // surface provided by the apprt and set up any API-specific
        // GPU resources.
        //
        // var api = try GraphicsAPI.init(alloc, options);   (generic.zig:662)
        // errdefer api.deinit();
        //
        // W1-7 DIVERGENCE (R4/apprt-free, DEFERRALS.md #3): `Options` carries no
        // `rt_surface` (it stays in `renderer_seam::Options<A>` in core so
        // rustty-renderer names no apprt type — R4), so `api` is built via the
        // offscreen-portable device ctor (`init_offscreen`, layer `None`) instead
        // of the live-surface `GraphicsAPI.init`. The on-screen layer attach is
        // supplied by the windowing host via `Metal::init` (W1-2) at the
        // renderer_seam wiring (W3) — behaviorally identical to ghostty across the
        // `init` -> host-attach sequence. This is the "reuse `init_offscreen`'s
        // shared body, do NOT duplicate" directive at the `Renderer` level.
        let api = Metal::init_offscreen(
            options.config.blending,
            options.size.screen.width,
            options.size.screen.height,
        )?;

        // const has_custom_shaders = options.config.custom_shaders.value.items.len > 0;
        let has_custom_shaders = !options.config.custom_shaders.value.is_empty();

        // P7-W7 T2 (`2f0b65346`): the swap chain and the shaders are no longer
        // built here; the first `drawFrame` on the render thread builds both
        // (`.swap_chain = null`, `.shaders = .uninit`,
        // `.reinitialize_shaders = true`).

        // Create the font shaper.
        //
        // Zig: `var font_shaper = try font.Shaper.init(alloc, .{
        //          .features = options.config.font_features.items });`
        // The `[:0]const u8` items -> the shaper's `Vec<Vec<u8>>` features
        // (the RAW bytes, matching the Zig `.len`-based content; the
        // `change_config` precedent, generic.zig).
        let font_shaper = Shaper::init(shape::Options {
            features: options.config.font_features.clone(),
        })?; // errdefer -> Drop

        // Initialize all the data that requires a critical font section.
        //
        // const font_critical: struct { metrics: font.Metrics } = font_critical: {
        //     const grid: *font.SharedGrid = options.font_grid;
        //     grid.lock.lockShared();
        //     defer grid.lock.unlockShared();
        //     break :font_critical .{ .metrics = grid.metrics };
        // };
        //
        // The shared-read lock is LOAD-BEARING and MUST be mirrored (R1): the
        // reference takes `grid.lock.lockShared()` before reading `grid.metrics`.
        // `SharedGrid` exposes `pub lock: RwLock<()>` with `.read()` (every
        // `lockShared` site in the port is `self.lock.read().unwrap()`).
        let grid_metrics = {
            // const grid: *font.SharedGrid = options.font_grid;
            let g: *mut SharedGrid = options.font_grid;
            // grid.lock.lockShared(); defer grid.lock.unlockShared();
            //
            // SAFETY: `g` is the external `*mut SharedGrid` handle; `(*g).lock` /
            // `(*g).metrics` are shared reads through a raw pointer with no `&mut`
            // alias — the `set_font_grid` (§4.4) precedent, no SB hazard.
            let _read = unsafe { (*g).lock.read().unwrap() };
            unsafe { (*g).metrics }
        };

        // PORT NOTE (`a177ba90a`): `init` no longer creates the display link.
        // The reference DELETED its eager `createWithActiveCGDisplays()` +
        // `errdefer ... release()` pair here, because a locked macOS session can
        // temporarily have no active displays and that error path aborted renderer
        // init (reported as OutOfMemory, no less). Creation is now LAZY, inside
        // `sync_display_link` below, which tolerates failure by falling back to
        // event-driven rendering without vsync and retrying on the next display
        // update. The field starts `None`; the owning-`DisplayLinkErrdefer` RAII
        // guard this replaced is gone with the fallible call it guarded.

        // generic.zig:718-743 uniforms literal. The reference sets
        // projection_matrix/cell_size/grid_size/grid_padding/screen_size (and
        // cursor_color) to `undefined` (fixed by the `update_*` calls below /
        // `update_frame`), and every REMAINING field to a concrete value. Rust has
        // no `undefined`: start from zeroed POD (the `undefined` fields) then set
        // each field the reference sets. NB (R1, corrects a design-doc note):
        // `update_font_grid_uniforms`/`update_screen_size_uniforms` do NOT
        // re-apply `min_contrast`/`cursor_pos`/`bg_color`/`bools`, so these MUST be
        // set here to match the reference first-frame state.
        let uniforms = {
            let mut u: Uniforms = unsafe { core::mem::zeroed() };
            // .padding_extend = .{},
            u.padding_extend = UniformsPaddingExtend::default();
            // .min_contrast = options.config.min_contrast,
            u.min_contrast = options.config.min_contrast;
            // .cursor_pos = .{ std.math.maxInt(u16), std.math.maxInt(u16) },
            u.cursor_pos = [u16::MAX, u16::MAX];
            // .bg_color = .{ bg.r, bg.g, bg.b, @intFromFloat(@round(bg_opacity * 255.0)) },
            // Note that if we're on macOS with glass effects we'll disable
            // background opacity but we handle that in updateFrame.
            u.bg_color = [
                options.config.background.r,
                options.config.background.g,
                options.config.background.b,
                (options.config.background_opacity * 255.0).round() as u8,
            ];
            // .bools = .{ ... },
            u.bools = UniformsBools {
                cursor_wide: false,
                use_display_p3: options.config.colorspace == WindowColorspace::DisplayP3,
                use_linear_blending: options.config.blending.is_linear(),
                use_linear_correction: options.config.blending == AlphaBlending::LinearCorrected,
            };
            u
        };

        // generic.zig:744-772 custom_shader_uniforms literal: resolution = {0,0,1},
        // frame_rate = 60, focus = 1 (assume focused initially); every other field
        // is 0 / @splat(0). Zeroed POD + the three non-zero fields.
        let custom_shader_uniforms = {
            let mut c: shadertoy::Uniforms = unsafe { core::mem::zeroed() };
            c.resolution = [0.0, 0.0, 1.0];
            c.frame_rate = 60.0; // not currently updated
            c.focus = 1; // assume focused initially
            c
        };

        // .cells = .{},  (generic.zig:717) — an EMPTY Contents. The first
        // `update_frame`/`rebuild_cells` resizes it to the terminal grid size
        // (generic.rs `rebuild_cells`: `if grid_size_diff { self.cells.resize(..) }`).
        let cells = Contents::default();

        let mut result = Self {
            // ---- generic.zig:701-787 (spelled out in the reference literal) ----
            alloc,
            config: options.config,
            // .surface_mailbox = options.surface_mailbox,  (704)
            surface_mailbox: options.surface_mailbox,
            grid_metrics, // font_critical.metrics
            size: options.size,
            focused: true,
            // .visible = true,  (a177ba90a)
            visible: true,
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
            // .bg_image_buffer = undefined,  (773) — fixed by update_bg_image_buffer below.
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
            // ---- Zig struct-decl defaults (OMITTED by 701-787; per golden_renderer) ----
            // PORT NOTE (`a177ba90a`): the reference DROPPED `.display_link =
            // display_link` from the 701-787 literal when creation moved into
            // `sync_display_link`, so the field now takes its struct-decl default
            // (generic.zig:205 `display_link: ?DisplayLink = null`) and joins this
            // section.
            display_link: None,
            latest_frame: Arc::new(LatestFrame::new()),
            presentation_health: AtomicHealth::new(crate::Health::Healthy),
            display_realized: true,
            kitty_animation_clock: None,
            kitty_animation_next_ms: None,
            draw_mutex: DrawMutex::new(),
            custom_shader_focused_changed: false,
            cells_rebuilt: AtomicBool::new(false),
            first_frame_time: None,
            last_frame_time: None,
            images: image::State::empty(),
            bg_image: None,
            bg_image_changed: false,
            bg_image_buffer_modified: 0,
            health: Arc::new(AtomicHealth::new(crate::Health::Healthy)),
            health_report: Arc::new(HealthReport::new()),
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
        result.prep_background_image()?; // (795)

        // return result;
        Ok(result)
    }

    // pub fn deinit(self: *Self) void {
    //     // This only deinitializes and frees CPU-side state
    //     // and does not free GPU resources like the swap chain and
    //     // shaders. Those are freed with `releaseGpuResources`.
    //
    //     if (self.overlay) |*overlay| overlay.deinit(self.alloc);
    //     self.terminal_state.deinit(self.alloc);
    //     if (self.search_selected_match) |*m| m.arena.deinit();
    //     if (self.search_matches) |*m| m.arena.deinit();
    //
    //     self.latest_frame.deinit(global.io());
    //
    //     if (DisplayLink != void) {
    //         if (self.display_link) |display_link| {
    //             display_link.stop() catch {};
    //             display_link.release();
    //         }
    //     }
    //
    //     self.cells.deinit(self.alloc);
    //
    //     self.font_shaper.deinit();
    //     self.font_shaper_cache.deinit(self.alloc);
    //
    //     self.config.deinit();
    //     self.api.deinit();
    //
    //     self.* = undefined;
    // }   (generic.zig:822-850 at `12752b2ac`, `2f0b65346`)
    //
    // The swap chain, shaders, images and background image were released by
    // `thread_exit` on the render thread (generic.zig:888-916).
    pub fn deinit(&mut self) {
        // This only deinitializes and frees CPU-side state
        // and does not free GPU resources like the swap chain and
        // shaders. Those are freed with `releaseGpuResources`.

        // if (self.overlay) |*overlay| overlay.deinit(self.alloc);
        // Port: `Overlay::deinit(&mut self)` takes NO allocator arg (overlay.rs:176).
        if let Some(overlay) = &mut self.overlay {
            overlay.deinit();
        }
        // (UD363: the reused overlay the port keeps beside it.)
        if let Some(overlay) = &mut self.overlay_spare {
            overlay.deinit();
        }
        // self.terminal_state.deinit(self.alloc);
        self.terminal_state.deinit(self.alloc.into());
        // if (self.search_selected_match) |*m| m.arena.deinit();
        // if (self.search_matches) |*m| m.arena.deinit();
        // Port: `SearchMatch`/`SearchMatches` DROPPED the reference `arena`
        // field (message.rs — "same arena collapse"); the owned `Flattened` /
        // `Vec<Flattened>` free via Rust `Drop` when `self.search_*` drops. No
        // explicit `arena.deinit()` exists to call (faithful — the arena teardown
        // is the automatic Option drop).

        // self.latest_frame.deinit(global.io());
        self.latest_frame.deinit();

        // if (DisplayLink != void) {
        //     if (self.display_link) |display_link| {
        //         display_link.stop() catch {};
        //         display_link.release();
        //     }
        // }
        // macOS-only crate ⇒ `DisplayLink != void` is always true (no comptime
        // guard). `stop() catch {}` = ignore the stop error (link may never have
        // started / its view is gone); then `release()` the +1 ref from `init`.
        if let Some(display_link) = self.display_link.as_ref() {
            let _ = display_link.stop(); // catch {}
            display_link.release();
        }

        // self.cells.deinit(self.alloc);
        // Port: `Contents::deinit(&mut self)` takes NO allocator arg (cell.rs:131).
        self.cells.deinit();

        // self.font_shaper.deinit();
        self.font_shaper.deinit();
        // self.font_shaper_cache.deinit(self.alloc);
        // Port: `ShaperCache::deinit(&mut self)` takes NO allocator arg (cache.rs:112).
        self.font_shaper_cache.deinit();

        // self.config.deinit();
        self.config.deinit();

        // self.api.deinit();
        //
        // PORT NOTE (P7-W7 T5b fix round 2): `Metal::deinit` clears the layer's
        // display callback and frees its context, as the reference's
        // `layer.release()` does; the ARC releases of queue/device/layer are the
        // `api` field's drop, which the owner runs right after this `deinit`
        // (core `RendererBox::dealloc_after_deinit` drops the whole renderer).
        self.api.deinit();

        // self.* = undefined; -> no-op (the owner drops the value next).
    }

    // P7-W7 T2 (`2f0b65346`): `surfaceInit` and `finalizeSurfaceInit` are gone
    // from `generic.zig` (neither had a Metal arm).

    /// W-ATTACH (macos-swift-app tier): attach the (offscreen-built) graphics
    /// API to a live host NSView, so frames present to the window instead of a
    /// dropped offscreen `IOSurface`. Core calls this from `Surface::init`
    /// (right after `finalizeSurfaceInit`, before the render thread spawns)
    /// with the raw view pointer it reads from `rt_surface.host_view()` — the
    /// renderer never names an apprt/app-kit type (R4). No-op on a null view
    /// (headless / non-hosted backends).
    #[cfg(target_os = "macos")]
    pub fn attach_host_view(
        &mut self,
        view: *mut core::ffi::c_void,
        scale: f64,
    ) -> Result<(), Error> {
        if view.is_null() {
            return Ok(());
        }
        // SAFETY: `view` is the live NSView the apprt surface was created with
        // (host contract); reinterpret the type-erased pointer as `&AnyObject`
        // for the objc2 layer-hosting messages inside `Metal::attach_view`.
        let obj = unsafe { &*(view as *const objc2::runtime::AnyObject) };
        self.api.attach_view(obj, scale)?;
        Ok(())
    }

    /// Non-macOS: no windowed Metal host-attach (offscreen/other backends).
    #[cfg(not(target_os = "macos"))]
    pub fn attach_host_view(
        &mut self,
        _view: *mut core::ffi::c_void,
        _scale: f64,
    ) -> Result<(), Error> {
        Ok(())
    }

    // generic.zig:880-886 threadEnter (`2f0b65346`: `self: *Self`).
    // `@hasDecl(Metal,"threadEnter")` is FALSE (Metal declares only loopEnter) ⇒
    // no api call; `surface` is the apprt surface the hook is generic over
    // (never named, R4).
    //   pub fn threadEnter(self: *Self, surface: *apprt.Surface) !void {
    //       // If our API has to do things on thread enter, let it.
    //       if (@hasDecl(GraphicsAPI, "threadEnter")) try self.api.threadEnter(surface);
    //   }
    /// Callback called by renderer.Thread when it begins.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn thread_enter<S>(&mut self, rt_surface: *mut S) -> Result<(), Error> {
        let _ = rt_surface;
        Ok(())
    }

    // generic.zig:888-916 threadExit (`2f0b65346`).
    //   pub fn threadExit(self: *Self) void {
    //       {
    //           self.draw_mutex.lockUncancelable(global.io());
    //           defer self.draw_mutex.unlock(global.io());
    //
    //           // Release swap chain and shaders.
    //           self.releaseGpuResources();
    //           ...
    //       }
    //
    //       // If our API has to do things on thread exit, let it.
    //       if (@hasDecl(GraphicsAPI, "threadExit")) {
    //           self.api.threadExit();
    //       }
    //   }
    /// Callback called by renderer.Thread when it exits. Called on the
    /// render thread. Releases all GPU resources before the API tears down
    /// its context, since after this the GL context will be gone.
    ///
    /// Locks `self.draw_mutex` around [`Self::thread_exit_locked`], for the
    /// single-threaded callers (the tests); the render thread uses
    /// [`Self::thread_exit_raw`] with its own handle (N437).
    pub fn thread_exit(&mut self) {
        let mutex = self.draw_mutex.clone();
        mutex.lock();
        self.thread_exit_locked();
        mutex.unlock();
        // `@hasDecl(Metal, "threadExit")` is FALSE ⇒ no api call.
    }

    /// The `draw_mutex` block of [`Self::thread_exit`]. The caller holds
    /// `draw_mutex`.
    pub fn thread_exit_locked(&mut self) {
        // Release swap chain and shaders.
        self.release_gpu_resources();

        // We don't release images in `releaseGpuResources`
        // since it can be called whenever the terminal is
        // occluded or unrealized, and we don't want to
        // reupload images every time that happens.
        // self.images.deinit(self.alloc); self.images = .empty;
        self.images.deinit(self.alloc);
        self.images = image::State::empty();

        // if (self.bg_image) |img| { img.deinit(self.alloc); self.bg_image = null; }
        // Port: `image::Image::deinit(self, alloc)` consumes `self` by value, so
        // move the `Image` out of the `Option` with `.take()` (image.rs:1283).
        if let Some(img) = self.bg_image.take() {
            img.deinit(self.alloc);
        }
    }

    // generic.zig:918-936 loopEnter. Adapted for R4: the reference takes
    // `thr: *renderer.Thread` and reads `&thr.draw_now`; we cannot name
    // `renderer.Thread` (R4), so the caller passes the `draw_now: xev::Async`
    // field directly (Copy+Send handle — NOT an apprt type).
    //   pub fn loopEnter(self, thr: *renderer.Thread) !void {
    //       // If our API has to do things on loop enter, let it.
    //       if (@hasDecl(GraphicsAPI, "loopEnter")) self.api.loopEnter();
    //       // If we don't support a display link we have no work to do.
    //       if (comptime DisplayLink == void) return;      // (macOS: never)
    //       self.syncDisplayLink(null, &thr.draw_now);
    //   }
    /// Called by renderer.Thread when it starts the main loop.
    ///
    /// Single-threaded form (the tests); the render thread uses
    /// [`Self::loop_enter_raw`] (N437).
    pub fn loop_enter(&mut self, draw_now: Async) -> Result<(), Error> {
        let mutex = self.draw_mutex.clone();
        let self_ptr: *mut Renderer = self;
        unsafe { Self::loop_enter_raw(self_ptr, &mutex, draw_now) }
    }

    // generic.zig:938-951 loopExit. `@hasDecl(Metal,"loopExit")` FALSE ⇒ no api
    // call ⇒ FULLY runnable (unlike loopEnter). Stop the link; error ignored.
    //   pub fn loopExit(self) void {
    //       // If our API has to do things on loop exit, let it.
    //       if (@hasDecl(GraphicsAPI, "loopExit")) self.api.loopExit();
    //       // If we don't support a display link we have no work to do.
    //       if (comptime DisplayLink == void) return;
    //       const display_link = self.display_link orelse return;
    //       display_link.stop() catch {};
    //   }
    /// Called by renderer.Thread when it exits the main loop.
    pub fn loop_exit(&self) {
        // UD362: disarm the health-report wakeup.
        self.health_report.arm(None);
        // Stop our display link. If this fails its okay it just means
        // that we either never started it or the view its attached to
        // is gone which is fine.
        if let Some(display_link) = self.display_link.as_ref() {
            let _ = display_link.stop(); // catch {}
        }
    }

    // generic.zig:986-994 displayLinkCallback. The body is INLINED into the
    // `cvdisplaylink::output_callback` trampoline (W2-T1) because a CoreVideo
    // `extern "C"` callback cannot be a `Self` method and must run without a
    // `Renderer` receiver. Kept as an R1 doc-anchor here; the live copy is in
    // cvdisplaylink.rs.
    //   fn displayLinkCallback(_: *macos.video.DisplayLink, ud: ?*xev.Async) void {
    //       const draw_now = ud orelse return;
    //       draw_now.notify() catch |err| log.err("error notifying draw_now err={}", .{err});
    //   }

    // generic.zig:1060-1068 setMacOSDisplayID.
    //   pub fn setMacOSDisplayID(
    //       self: *Self,
    //       id: u32,
    //       draw_now: *xev.Async,
    //   ) !void {
    //       if (comptime DisplayLink == void) return;
    //       self.syncDisplayLink(id, draw_now);
    //   }
    //
    // PORT NOTE (`a177ba90a`): the new `draw_now` parameter: a display change
    // is allowed to CREATE the link (that is the "when a display becomes
    // available it re-adds itself" half of the commit), which needs the
    // output-callback userdata. The R4 adaptation of `*xev.Async` matches
    // `loop_enter`'s: the Copy+Send handle by value.
    /// Called when we get an updated display ID for our display link.
    ///
    /// Single-threaded form; the render thread uses
    /// [`Self::set_macos_display_id_raw`] (N437).
    pub fn set_macos_display_id(&mut self, id: u32, draw_now: Async) -> Result<(), Error> {
        let mutex = self.draw_mutex.clone();
        let self_ptr: *mut Renderer = self;
        unsafe { Self::set_macos_display_id_raw(self_ptr, &mutex, id, draw_now) }
    }

    // generic.zig:1136-1144 hasVsync.
    //   pub fn hasVsync(self) bool {
    //       if (comptime DisplayLink == void) return false;
    //       const display_link = self.display_link orelse return false;
    //       return display_link.isRunning();
    //   }
    /// True if our renderer is using vsync. If true, the renderer or apprt
    /// is responsible for triggering draw_now calls to the render thread.
    /// That is the only way to trigger a drawFrame.
    pub fn has_vsync(&self) -> bool {
        match self.display_link.as_ref() {
            Some(display_link) => display_link.is_running(),
            None => false,
        }
    }

    // generic.zig:1146-1158 setFocus.
    //   pub fn setFocus(self, focus: bool) !void {
    //       assert(self.focused != focus);
    //       self.focused = focus;
    //
    //       // Flag that we need to update our custom shaders
    //       self.custom_shader_focused_changed = true;
    //
    //       self.syncDisplayLink(null, null);
    //   }
    /// Callback when the focus changes for the terminal this is rendering.
    ///
    /// Must be called on the render thread.
    ///
    /// Single-threaded form; the render thread uses [`Self::set_focus_raw`].
    pub fn set_focus(&mut self, focus: bool) -> Result<(), Error> {
        let mutex = self.draw_mutex.clone();
        let self_ptr: *mut Renderer = self;
        unsafe { Self::set_focus_raw(self_ptr, &mutex, focus) }
    }

    /// The locked half of `setFocus`: the two field writes.
    pub(super) fn set_focus_locked(&mut self, focus: bool) {
        assert!(self.focused != focus);
        self.focused = focus;

        // Flag that we need to update our custom shaders
        self.custom_shader_focused_changed = true;
    }

    /// Create or update the display link and match it to the current
    /// surface state.
    ///
    /// Must be called on the render thread and must NOT be called
    /// while holding `draw_mutex`. Stopping a CVDisplayLink is a
    /// blocking join on CoreVideo's IO thread, and the apprt calls
    /// `drawFrame` (which takes `draw_mutex`) from the CoreAnimation
    /// layer display path on the main thread.
    // generic.zig:1216-1276 syncDisplayLink at `12752b2ac` (NEW in `a177ba90a`;
    // the doc paragraph above is from `72cf50855`).
    //   fn syncDisplayLink(
    //       self: *Self,
    //       display_id: ?u32,
    //       draw_now: ?*xev.Async,
    //   ) void {
    //       if (comptime DisplayLink == void) return;
    //
    //       const display_link = self.display_link orelse display_link: {
    //           if (!self.config.vsync) return;
    //           const callback = draw_now orelse return;
    //           const result = macos.video.DisplayLink.createWithActiveCGDisplays() catch |err| {
    //               // A locked macOS session can temporarily have no active
    //               // displays. Rendering can continue without vsync and a
    //               // later display update will retry this method.
    //               log.warn("error creating display link; using fallback rendering err={}", .{err});
    //               return;
    //           };
    //           result.setOutputCallback(
    //               xev.Async,
    //               &displayLinkCallback,
    //               callback,
    //           ) catch |err| {
    //               log.warn("error configuring display link err={}", .{err});
    //               result.release();
    //               return;
    //           };
    //
    //           self.display_link = result;
    //           log.info("created display link", .{});
    //           break :display_link result;
    //       };
    //
    //       if (display_id) |id| {
    //           log.info("updating display link display id={}", .{id});
    //           display_link.setCurrentCGDisplay(id) catch |err| {
    //               log.warn("error setting display link display id err={}", .{err});
    //           };
    //       }
    //
    //       const should_run =
    //           // Non-visible windows never vsync
    //           self.visible and
    //           // Only vsync if we have cell changes or animation
    //           (self.cells_rebuilt or self.animationWake() != null);
    //
    //       if (should_run) {
    //           if (!display_link.isRunning()) {
    //               display_link.start() catch {};
    //           }
    //       } else {
    //           display_link.stop() catch {};
    //       }
    //   }
    //
    // PORT NOTE (`97f57edcc` + `6688aa072`, phase-6): the start/stop predicate
    // is no longer `visible and focused`. `97f57edcc` (vsync unfocused
    // surfaces while dirty) dropped `focused` — an unfocused-but-visible
    // surface with pending cell changes must still be presented on vsync —
    // and `6688aa072` (park DisplayLink while idle) gates the link on
    // `cells_rebuilt or animationWake() != null`, so an idle surface parks
    // the link between frames; `updateFrame`'s tail and `drawFrame`'s
    // no-redraw arm resync it. `set_focus` still calls this (the reference
    // kept the call) — it now only matters through `animationWake`'s
    // focus-gated custom-shader cadence.
    //
    // PORT NOTE: the reference's `orelse ... break :display_link result` labelled
    // block yields a `DisplayLink` VALUE (a `*DisplayLink` pointer in Zig, so the
    // copy and `self.display_link` denote the same CoreVideo object). Rust's
    // `DisplayLink` is a NonNull wrapper that owns the boxed callback userdata and
    // is NOT `Copy`, so the port keeps the link in the field throughout and
    // re-borrows it after the creation arm — same object, no aliasing copy. The
    // creation arm therefore assigns `self.display_link` and falls through rather
    // than yielding a value.
    //
    // `if (comptime DisplayLink == void) return;` — macOS: DisplayLink is never
    // void; no early return.
    //
    // PORT NOTE: reference defect (R13), UD352. The tail of the reference body
    // (read `visible`/`cells_rebuilt`/`animationWake()`, then start or stop the
    // link) is split into `display_link_sync` (the reads, producing a
    // `DisplayLinkSync`) and `DisplayLinkSync::apply` (the CoreVideo calls, on
    // a non-owning `DisplayLinkHandle` that touches no renderer bytes).
    // `draw_frame_raw` and `update_frame_raw` take the reads while they still
    // hold `draw_mutex` and apply after releasing it: in the reference those
    // reads run unlocked and race the main thread's `drawFrame(sync)`.
    //
    // PORT NOTE (N437): this form is single-threaded (the tests); every live
    // caller is [`Self::sync_display_link_raw`], which does the same with every
    // read and write of the renderer under `draw_mutex` and every CoreVideo call
    // outside it.
    #[cfg(test)]
    pub(super) fn sync_display_link(&mut self, display_id: Option<u32>, draw_now: Option<Async>) {
        let mutex = self.draw_mutex.clone();
        let self_ptr: *mut Renderer = self;
        unsafe { Self::sync_display_link_raw(self_ptr, &mutex, display_id, draw_now) }
    }

    /// The reads of the `syncDisplayLink` tail (UD352): whether the link
    /// should run, and a non-owning handle to it (`None` without a link, in
    /// which case the reference has already returned).
    ///
    /// Callers that race the main thread's draw take this under
    /// `draw_mutex`; see `sync_display_link`.
    pub(super) fn display_link_sync(&self) -> DisplayLinkSync {
        let should_run =
            // Non-visible windows never vsync
            self.visible &&
            // Only vsync if we have cell changes or animation
            (self.cells_rebuilt.load(Ordering::Relaxed) || self.animation_wake().is_some());
        DisplayLinkSync {
            link: self.display_link.as_ref().map(DisplayLink::handle),
            should_run,
        }
    }

    /// Callback when the window is visible or occluded.
    ///
    /// Must be called on the render thread.
    // generic.zig:1174-1188 setVisible (`2f0b65346`).
    //   pub fn setVisible(self: *Self, visible: bool) void {
    //       self.visible = visible;
    //       self.syncDisplayLink(null, null);
    //
    //       // When we're hidden, release our GPU resources.
    //       if (!visible) {
    //           self.draw_mutex.lockUncancelable(global.io());
    //           defer self.draw_mutex.unlock(global.io());
    //           self.releaseGpuResources();
    //       }
    //   }
    ///
    /// Single-threaded form; the render thread uses [`Self::set_visible_raw`].
    pub fn set_visible(&mut self, visible: bool) {
        let mutex = self.draw_mutex.clone();
        let self_ptr: *mut Renderer = self;
        unsafe { Self::set_visible_raw(self_ptr, &mutex, visible) }
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
    // pub fn releaseGpuResources(self: *Self) void {
    //     if (self.swap_chain) |*sc| {
    //         // Waits for any in-flight frames to complete, then
    //         // frees all GPU resources.
    //         sc.deinit();
    //         self.swap_chain = null;
    //     }
    //
    //     // Release the shaders as well if we're unrealized.
    //     if (!self.display_realized) {
    //         self.shaders.deinit(self.alloc);
    //     }
    // }   (generic.zig:1190-1214, `2f0b65346`)
    //
    // SAFETY (assessment D hazard #3): `SwapChain::deinit` WAITS on
    // `frame_sema` for every in-flight frame before freeing, and
    // `frame_completed` (the only cross-thread reader of `swap_chain`)
    // posts that semaphore as its LAST touch of the chain — so the
    // `.unwrap()` deref there can never observe the teardown. The
    // draw_mutex (held by the caller) serializes this against `drawFrame`.
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
    // Zig: pub inline fn markDirty(self: *Self) void {
    //          self.terminal_state.dirty = .full;
    //      }
    pub fn mark_dirty(&mut self) {
        self.terminal_state.dirty = Dirty::Full;
    }

    /// Set the new font grid.
    ///
    /// Must be called on the render thread.
    //
    // Zig: pub fn setFontGrid(self: *Self, grid: *font.SharedGrid) void
    // The reference takes a safe `*font.SharedGrid` arg it dereferences; keep the
    // faithful safe `pub fn` signature (the raw deref is the external shared-grid
    // handle, §4.4 — not the `rust-mut-args-strongly-protected` pattern).
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn set_font_grid(&mut self, grid: *mut SharedGrid) {
        let mutex = self.draw_mutex.clone();
        mutex.lock();
        self.set_font_grid_locked(grid);
        mutex.unlock();
    }

    /// The body of [`Self::set_font_grid`] between its `lock()` and the
    /// `defer unlock()`; the caller holds `draw_mutex` (N437: the render
    /// thread takes it through its own handle before forming this `&mut self`).
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn set_font_grid_locked(&mut self, grid: *mut SharedGrid) {
        // Update our grid
        self.font_grid = grid;

        // Update all our textures so that they sync on the next frame.
        // We can modify this without a lock because the GPU does not
        // touch this data. A released swap chain is rebuilt with
        // fresh frames that sync all textures on first use.
        // if (self.swap_chain) |*sc| for (&sc.frames) |*frame| { ... };
        if let Some(sc) = &mut self.swap_chain {
            for frame in &mut sc.frames {
                frame.grayscale_modified = 0;
                frame.color_modified = 0;
            }
        }

        // Get our metrics from the grid. This doesn't require a lock because
        // the metrics are never recalculated.
        //
        // SAFETY: `grid` is the external `*mut SharedGrid` handle (not reborrowed
        // from `&mut self`); `(*grid).metrics` is a shared read through a raw
        // pointer with no `&mut` alias to `*grid` — not the
        // `rust-mut-args-strongly-protected` pattern, no SB hazard (§4.4).
        let metrics = unsafe { (*grid).metrics };
        self.grid_metrics = metrics;

        // Reset our shaper cache. If our font changed (not just the size) then
        // the data in the shaper cache may be invalid and cannot be used, so we
        // always clear the cache just in case.
        //
        // Zig: `self.font_shaper_cache.deinit(self.alloc)` — the ported
        // `ShaperCache::deinit(&mut self)` takes NO allocator arg (cache.rs:112).
        let font_shaper_cache = ShaperCache::init();
        self.font_shaper_cache.deinit();
        self.font_shaper_cache = font_shaper_cache;

        // Update cell size.
        self.size.cell = CellSize {
            width: metrics.cell_width,
            height: metrics.cell_height,
        };

        // Update relevant uniforms
        self.update_font_grid_uniforms();

        // Force a full rebuild, because cached rows may still reference
        // an outdated atlas from the old grid and this can cause garbage
        // to be rendered.
        self.mark_dirty();
    }

    /// Resize the screen.
    //
    // generic.zig:2210-2226 `setScreenSize` (`2f0b65346`).
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
    /// (N437).
    pub fn set_screen_size_locked(&mut self, size: Size) {
        self.size = size;
        self.update_screen_size_uniforms();

        // Some graphics APIs need to manually update their viewport,
        // like OpenGL. Do so here.
        // (`@hasDecl(Metal, "setViewport")` is FALSE.)

        // log.debug("screen size size={}", .{size});
        log::debug!(target: LOG_SCOPE, "screen size size={size:?}");
    }

    /// PORT NOTE (rustty-only glue, additive): no reference counterpart — see
    /// `Metal::set_offscreen_size`, which this forwards to. Ghostty's renderer
    /// never sizes its own surface: `Metal.surfaceSize` reads the attached
    /// layer's bounds, so the windowing system resizes it. rustty's OFFSCREEN
    /// slice has no layer (`init_offscreen`, layer `None`), so an embedder that
    /// composites the finished IOSurface itself needs this to make its surface
    /// follow the pane. Purely additive: nothing in the reference lane calls it,
    /// and on-screen (`layer.is_some()`) it has no effect at all.
    ///
    /// Separate from [`Self::set_screen_size`] on purpose, and *not* folded into
    /// it: that one is the reference's `setScreenSize`, which stores the size
    /// core computed (plus the uniforms derived from it), while the surface
    /// `draw_frame` sizes the swap-chain target to comes from
    /// `api.surface_size()`. So the two compose: this call changes what the
    /// *next* `draw_frame` will see.
    ///
    /// Takes `draw_mutex`, as `set_screen_size` does, so the write cannot land
    /// midway through a concurrent `draw_frame`'s size handling.
    pub fn set_offscreen_size(&mut self, size: ScreenSize) {
        self.draw_mutex.lock();
        // `defer self.draw_mutex.unlock()`: INFALLIBLE body (no `?`) => the end
        // unlock is reached on every non-panic path (as in `set_screen_size`).

        self.api.set_offscreen_size(size.width, size.height);

        log::debug!(target: LOG_SCOPE, "offscreen size size={size:?}");

        self.draw_mutex.unlock();
    }

    // pub fn changeConfig(self: *Self, config: *DerivedConfig) !void
    //
    // SIGNATURE PORT NOTE (§4.3): the reference does a shallow `self.config = config.*`
    // (a Zig move-by-copy) and relies on the caller's later `Message.deinit`. In
    // Rust `DerivedConfig` is non-`Copy` (owns `ArenaAllocator`/`Vec`/`link::Set`),
    // so this is ported as by-value ownership transfer; the `Message::ChangeConfig`
    // payload holds `Box<DerivedConfig>`, so the caller passes ownership (the
    // reference `v.impl.deinit()` collapses into the old-config drop).
    //
    // `defer self.draw_mutex.unlock()` FIDELITY: the reference releases the mutex
    // on EVERY exit — including the `try font.Shaper.init(...)` error return. The
    // ported `Mutex` is bare lock/unlock with NO RAII guard; a scope-guard holding
    // `&self.draw_mutex` would conflict with the body's `&mut self` calls
    // (`update_bg_image_buffer`/`mark_dirty`/field writes) once those reborrow
    // `self` mutably. So `defer` is replicated by explicit unlock on every early
    // return (unlock-before-`?`) plus the unlock at the natural end.
    /// Update the configuration.
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
        // We always redo the font shaper in case font features changed. We
        // could check to see if there was an actual config change but this is
        // easier and rare enough to not cause performance issues.
        {
            // Zig: `var font_shaper = try font.Shaper.init(self.alloc, .{
            //          .features = config.font_features.items });`
            // The `[:0]const u8` items -> the shaper's `Vec<Vec<u8>>` features
            // (the RAW bytes, matching the Zig `.len`-based content).
            let font_shaper = Shaper::init(shape::Options {
                features: config.font_features.clone(),
            })?;
            // errdefer font_shaper.deinit();
            self.font_shaper.deinit();
            self.font_shaper = font_shaper;
        }

        // We also need to reset the shaper cache so shaper info
        // from the previous font isn't reused for the new font.
        //
        // Zig: `self.font_shaper_cache.deinit(self.alloc)` — NO arg (cache.rs:112).
        let font_shaper_cache = ShaperCache::init();
        self.font_shaper_cache.deinit();
        self.font_shaper_cache = font_shaper_cache;

        // Set our new minimum contrast
        self.uniforms.min_contrast = config.min_contrast;

        // Set our new color space and blending
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
        // `Path` derives `PartialEq`, so `old != new` == `!old.equal(new)`. Now
        // that `bg_image` is a real `Option<Path>` this genuinely change-detects
        // (None->Some, Some->different, Some->same) and gates `prepBackgroundImage`.
        let bg_image_changed = match (&self.config.bg_image, &config.bg_image) {
            (Some(old), Some(new)) => old != new,
            (Some(_), None) => true,
            (None, Some(_)) => true,
            (None, None) => false,
        };

        let old_blending = self.config.blending;
        // Zig: `!self.config.custom_shaders.equal(config.custom_shaders)`.
        // The derived `PartialEq` on `RepeatablePath` (`Vec<Path>` — length +
        // per-element variant + `CString` bytes) reproduces the reference `equal`
        // exactly, so `!=` is `!equal(...)`.
        let custom_shaders_changed = self.config.custom_shaders != config.custom_shaders;

        // Zig: `self.config.deinit(); self.config = config.*;` -> replace the
        // config (moving the new one in), then `deinit` + drop the old one.
        let mut old = mem::replace(&mut self.config, config);
        old.deinit();

        // If our background image path changed, prepare the new bg image.
        // if (bg_image_changed) try self.prepBackgroundImage();
        //
        // `prep_background_image` decodes the new path (wuffs PNG/JPEG) into a
        // pending `bg_image`; only a DECODE error propagates (open/read failures
        // warn + no-op — §3.3). unlock-before-`?` (the `Mutex` is guard-less, so
        // `defer draw_mutex.unlock()` is replicated on every early return).
        if bg_image_changed {
            self.prep_background_image()?;
        }

        // If our background image config changed, update the vertex buffer.
        if bg_image_config_changed {
            self.update_bg_image_buffer();
        }

        // Reset our viewport to force a rebuild, in case of a font change.
        self.mark_dirty();

        // FIDELITY (§4.3): the reference reads `config.blending` here (the local
        // param), which — after the move into `self.config` above — equals
        // `self.config.blending`; the Rust `mem::replace` moved `config`, so the
        // trailing reads use `self.config` (== the new config), the same values.
        let blending_changed = old_blending != self.config.blending;

        if blending_changed {
            // We update our API's blending mode.
            self.api.blending = self.config.blending;
            // And indicate that we need to reinitialize our shaders.
            self.reinitialize_shaders = true;
            // And indicate that our swap chain targets need to
            // be re-created to account for the new blending mode.
            // Zig `+%=` -> `wrapping_add`.
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
    // fn prepBackgroundImage(self: *Self) !void {  (ref generic.zig:1747)
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
        // ADVERSARY F6 — borrow discipline: `p` borrows `self.config` immutably,
        // but the load block below mutates `self.bg_image` (`mark_for_replace`/the
        // `self.bg_image = ...` set). Copy the null-terminated path bytes into an
        // OWNED `CString` here and drop the `&self.config` borrow before touching
        // `self.bg_image`. Purely mechanical; no fidelity impact — the reference's
        // `path` is a plain slice into config memory, but Rust's aliasing rules
        // require the early copy.
        let path: CString = p.slice().to_owned();

        // `self.alloc` is a Copy `Global` ZST; capture it so the `'load_background`
        // block reads it without holding a `&self` borrow across the later
        // `self.bg_image` mutation.
        let alloc = self.alloc;

        // 'load_background: { ... break :load_background; } — a labeled block whose
        // early exits (open-fail / read-fail / unknown-type) `break` with `None`
        // ("no change"); a DECODE error `?`-propagates OUT of the whole method
        // (the reference `try`), NOT a `break` (§3.3 asymmetry).
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
            // `{s}` on the reference's `[]const u8` path prints the RAW bytes; the
            // closest Rust rendering is `String::from_utf8_lossy` over the same
            // bytes. The lossy conversion is confined to the log message — the path
            // the port actually opens stays the byte-faithful `OsStr`.
            let path_bytes = path.to_bytes();
            let path_display = String::from_utf8_lossy(path_bytes);
            // var file = std.fs.openFileAbsolute(path, .{}) catch |err| { log.warn(...); break; };
            let mut file = match std::fs::File::open(OsStr::from_bytes(path_bytes)) {
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
            // const contents = file.readToEndAlloc(self.alloc, std.math.maxInt(u32)) catch |err| { log.warn(...); break; };
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
            // align 1, so `image.rs::free_data`'s `Layout::from_size_align(len, 1)`
            // frees it later without a layout mismatch — §3.2/R-2). `contents`
            // (the encoded bytes) IS freed here, at the block's end (`Vec` drop ==
            // the reference `defer self.alloc.free(contents)`).
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
            let image = image::Image::Pending(pend);
            // if (self.bg_image) |*img| { img.markForReplace(self.alloc, image); }
            // else { self.bg_image = image; }
            if let Some(img) = self.bg_image.as_mut() {
                img.mark_for_replace(self.alloc, image);
            } else {
                self.bg_image = Some(image);
            }
        }

        Ok(())
    }
}
