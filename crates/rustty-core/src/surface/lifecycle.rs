//! PORTS: ghostty src/Surface.zig — the lifecycle cluster: `init`, `deinit`, `close`,
//! `surfaceMailbox`, `queueIo`, `draw`, `activateInspector`, `deactivateInspector`,
//! `needsConfirmQuit` (Surface.zig:457-957).

#![allow(non_snake_case)]

use super::*;

impl<A> Surface<A>
where
    A: apprt::App + 'static,
    // C0 DECISION (CORE_BLUEPRINT §1, option 1 — the app.rs impl-block
    // precedent): the self-referential associated-type bound; Surface's
    // method tier needs it to call into `App<A>`'s constrained methods
    // (maybeHandleBinding's `performAllChainedAction`, C10).
    A::Surface: ApprtSurface<App = A, Core = Surface<A>>,
{
    /// Create a new surface. This must be called from the main thread. The
    /// pointer to the memory for the surface must be provided and must be
    /// stable due to interfacing with various callbacks.
    ///
    /// # Safety
    ///
    /// `self_` must point at valid (possibly uninitialized) `Surface<A>`
    /// memory that stays STABLE for the surface's lifetime (the reference doc
    /// contract above — the renderer/io threads and mailboxes hold pointers
    /// into it); `app`/`rt_app`/`rt_surface` must be valid for the call. On
    /// `Err` the pointee is (still or again) uninitialized and must be freed
    /// without dropping.
    //
    // pub fn init(
    //     self: *Surface,
    //     alloc: Allocator,
    //     config_original: *const configpkg.Config,
    //     app: *App,
    //     rt_app: *apprt.runtime.App,
    //     rt_surface: *apprt.runtime.Surface,
    // ) !void {
    //
    // PORT NOTE: the reference out-pointer init stays an out-pointer (unlike
    // the vt `Terminal::init` constructor-return precedent) because the body
    // is SELF-REFERENTIAL: `&self.renderer_thread`/`&self.renderer`/
    // `&self.renderer_state`/`&self.io.terminal` and the `self` mailbox
    // pointer are handed out mid-init. The inferred `!void` error set is the
    // typed [`InitError`] union.
    pub unsafe fn init(
        self_: *mut Surface<A>,
        alloc: Allocator,
        config_original: &configpkg::Config,
        app: *mut App<A>,
        rt_app: *mut A,
        rt_surface: *mut A::Surface,
    ) -> Result<(), InitError> {
        // Apply our conditional state. If we fail to apply the conditional state
        // then we log and attempt to move forward with the old config.
        // var config_: ?configpkg.Config = config_original.changeConditionalState(
        //     app.config_conditional_state,
        // ) catch |err| err: {
        //     log.warn("failed to apply conditional state to config err={}", .{err});
        //     break :err null;
        // };
        //
        // PARITY: not `.unwrap_or_default()` — the arm IS the reference's
        // labeled catch block with its log site (the app.rs updateConfig
        // precedent).
        #[allow(clippy::manual_unwrap_or_default)]
        let mut config_: Option<configpkg::Config> = match config_original
            .change_conditional_state(unsafe { (*app).config_conditional_state })
        {
            Ok(v) => v,
            Err(err) => {
                // log.warn("failed to apply conditional state to config err={}", .{err});
                log::warn!(
                    target: LOG_SCOPE,
                    "failed to apply conditional state to config err={err:?}"
                );
                None
            }
        };
        // defer if (config_) |*c| c.deinit();
        //
        // PORT NOTE: fired explicitly on the success path at the end of this
        // fn; every early-return path maps to `config_`'s Drop (the app.rs
        // updateConfig precedent). `config_` is declared first, so its Drop
        // runs LAST on error paths — the reference defer/errdefer order.

        // We want a config pointer for everything so we get that either
        // based on our conditional state or the original config.
        // const config: *const configpkg.Config = if (config_) |*c| config: { ... } else config_original;
        let config: &configpkg::Config = match config_.as_mut() {
            Some(c) => {
                // We want to preserve our original working directory. We
                // don't need to dupe memory here because termio will derive
                // it. We preserve this so directory inheritance works.
                // c.@"working-directory" = config_original.@"working-directory";
                //
                // PORT NOTE: the reference is a shallow slice copy into the
                // original config's memory; the owned Rustty value clones the
                // same bytes.
                c.working_directory = config_original.working_directory.clone();
                // break :config c;
                c
            }
            None => config_original,
        };

        // Get our configuration
        // var derived_config = try DerivedConfig.init(alloc, config);
        let mut derived_config = DerivedConfig::init(alloc, config)?;
        // errdefer derived_config.deinit();
        //
        // PORT NOTE: fired EXPLICITLY at every pre-`self.*`-write failure
        // site below (oni `Regex` frees only via explicit deinit — no Drop;
        // the C5 `DerivedConfig` arena-collapse note); after the big write it
        // is realized by [`Surface::initErrdefers`].

        // P7-W7 T2 (`2f0b65346`): `try Renderer.surfaceInit(rt_surface);` is gone
        // (the renderer no longer prepares a GL context on the main thread).

        // Determine our DPI configurations so we can properly configure
        // font points to pixels and handle other high-DPI scaling factors.
        // const content_scale = try rt_surface.getContentScale();
        let content_scale = match unsafe { &*rt_surface }.get_content_scale() {
            Ok(v) => v,
            Err(err) => {
                // (errdefer derived_config.deinit() — Surface.zig:490)
                derived_config.deinit();
                return Err(err.into());
            }
        };
        // const x_dpi = content_scale.x * font.face.default_dpi;
        // const y_dpi = content_scale.y * font.face.default_dpi;
        let x_dpi = content_scale.x * f32::from(face::DEFAULT_DPI);
        let y_dpi = content_scale.y * f32::from(face::DEFAULT_DPI);
        // log.debug("xscale={} yscale={} xdpi={} ydpi={}", .{
        //     content_scale.x,
        //     content_scale.y,
        //     x_dpi,
        //     y_dpi,
        // });
        log::debug!(
            target: LOG_SCOPE,
            "xscale={} yscale={} xdpi={x_dpi} ydpi={y_dpi}",
            content_scale.x,
            content_scale.y
        );

        // The font size we desire along with the DPI determined for the surface
        // const font_size: font.face.DesiredSize = .{
        //     .points = config.@"font-size",
        //     .xdpi = @intFromFloat(@round(x_dpi)),
        //     .ydpi = @intFromFloat(@round(y_dpi)),
        // };
        // (`2f0b65346` rounds: a fractional GTK scale gives a fractional DPI,
        // which `@intFromFloat` alone truncated.)
        let font_size: DesiredSize = DesiredSize {
            points: config.font_size,
            xdpi: x_dpi.round() as u16,
            ydpi: y_dpi.round() as u16,
        };

        // Setup our font group. This will reuse an existing font group if
        // it was already loaded.
        // const font_grid_key, const font_grid = try app.font_grid_set.ref(
        //     &derived_config.font,
        //     font_size,
        // );
        let (font_grid_key, font_grid) =
            match unsafe { (*app).font_grid_set.r#ref(&derived_config.font, font_size) } {
                Ok(v) => v,
                Err(err) => {
                    // (errdefer derived_config.deinit() — Surface.zig:490)
                    derived_config.deinit();
                    return Err(err.into());
                }
            };
        // PORT NOTE: reference defect (R13), UPSTREAM-DEFECTS.md UD115. The
        // reference arms no `errdefer app.font_grid_set.deref(font_grid_key)`
        // after this `ref` (Surface.zig:523), so any later `init` failure keeps
        // the grid's reference forever: the grid leaks, and the app's teardown
        // `assert(self.font_grid_set.count() == 0)` (App.zig:141) traps in
        // safety-checked builds. The port releases it on every error path below
        // (in that errdefer's LIFO slot: after the renderer teardown, before
        // `derived_config.deinit()`), as `Surface.deinit` does
        // (Surface.zig:840) and as `setFontSize`'s own `ref` does with its
        // errdefer (Surface.zig:2446).

        // Build our size struct which has all the sizes we need.
        // const size: rendererpkg.Size = size: { ... };
        let size: rendererpkg::size::Size = {
            // var size: rendererpkg.Size = .{
            let mut size = rendererpkg::size::Size {
                // .screen = screen: {
                //     const surface_size = try rt_surface.getSize();
                //     break :screen .{
                //         .width = surface_size.width,
                //         .height = surface_size.height,
                //     };
                // },
                screen: {
                    let surface_size = match unsafe { &*rt_surface }.get_size() {
                        Ok(v) => v,
                        Err(err) => {
                            // (errdefer app.font_grid_set.deref — UD115)
                            unsafe { (*app).font_grid_set.deref(font_grid_key) };
                            // (errdefer derived_config.deinit() — Surface.zig:490)
                            derived_config.deinit();
                            return Err(err.into());
                        }
                    };
                    rendererpkg::size::ScreenSize {
                        width: surface_size.width,
                        height: surface_size.height,
                    }
                },

                // .cell = font_grid.cellSize(),
                cell: unsafe { &*font_grid }.cell_size(),
                // .padding = .{},
                padding: rendererpkg::size::Padding::default(),
            };

            // const explicit: rendererpkg.Padding = derived_config.scaledPadding(
            //     x_dpi,
            //     y_dpi,
            // );
            let explicit: rendererpkg::size::Padding = derived_config.scaledPadding(x_dpi, y_dpi);
            // if (derived_config.window_padding_balance != .false) {
            if derived_config.window_padding_balance
                != configpkg::value_types::WindowPaddingBalance::False
            {
                // size.balancePadding(explicit, derived_config.window_padding_balance);
                size.balance_padding(explicit, derived_config.window_padding_balance);
            } else {
                // size.padding = explicit;
                size.padding = explicit;
            }

            // break :size size;
            size
        };

        // Create our terminal grid with the initial size
        // const app_mailbox: App.Mailbox = .{ .rt_app = rt_app, .mailbox = &app.mailbox };
        let app_mailbox: crate::app::Mailbox<A> = crate::app::Mailbox {
            rt_app,
            mailbox: unsafe { &raw mut (*app).mailbox },
        };
        // var renderer_impl = try Renderer.init(alloc, .{
        //     .config = try .init(alloc, config),
        //     .font_grid = font_grid,
        //     .size = size,
        //     .surface_mailbox = .{ .surface = self, .app = app_mailbox },
        //     .rt_surface = rt_surface,
        //     .thread = &self.renderer_thread,
        // });
        let renderer_config = match rendererpkg::DerivedConfig::init(alloc, config) {
            Ok(v) => v,
            Err(err) => {
                // (errdefer app.font_grid_set.deref — UD115)
                unsafe { (*app).font_grid_set.deref(font_grid_key) };
                // (errdefer derived_config.deinit() — Surface.zig:490)
                derived_config.deinit();
                return Err(err.into());
            }
        };
        // PORT NOTE (N444, P7-W7 T5c): `&self.renderer_thread` (the renderer
        // options just below) and `&self.renderer_state` (`Thread.init`) are
        // handed out before either block exists. Each lives in its own
        // allocation (`ThreadSharedBox`), so both are allocated UNINITIALIZED
        // here and written in place at the big write; every error path until
        // then frees them without glue (`dealloc`).
        let renderer_thread_slot = ThreadSharedBox::<crate::renderer_seam::Thread<A>>::new_uninit();
        let renderer_state_slot = ThreadSharedBox::<rendererpkg::state::State>::new_uninit();
        let renderer_impl = match Renderer::init(
            alloc,
            crate::renderer_seam::Options {
                config: renderer_config,
                font_grid,
                size,
                surface_mailbox: SurfaceMailbox {
                    surface: self_,
                    app: app_mailbox,
                },
                rt_surface,
                // (N444: the out-of-line block's address.)
                thread: renderer_thread_slot.as_ptr(),
            },
        ) {
            Ok(v) => v,
            Err(err) => {
                // (N444: the two never-written slots.)
                unsafe {
                    renderer_state_slot.dealloc();
                    renderer_thread_slot.dealloc();
                }
                // (errdefer app.font_grid_set.deref — UD115)
                unsafe { (*app).font_grid_set.deref(font_grid_key) };
                // (errdefer derived_config.deinit() — Surface.zig:490; the
                // renderer DerivedConfig moved into the failed init's options)
                derived_config.deinit();
                return Err(err.into());
            }
        };
        // errdefer renderer_impl.deinit();
        // (fired explicitly at the failure sites below / by initErrdefers.)
        //
        // PORT NOTE (N441): moved into its own allocation before the render
        // thread gets its pointer (`RendererBox`); `renderer_ptr` is the raw
        // root the thread and `self.renderer` share.
        let renderer_impl = crate::renderer_seam::RendererBox::new(renderer_impl);
        let renderer_ptr: *mut Renderer = renderer_impl.as_ptr();

        // The mutex used to protect our renderer state.
        // const mutex = try alloc.create(std.Thread.Mutex);
        // mutex.* = .{};
        // errdefer alloc.destroy(mutex);
        //
        // PORT NOTE (CORE_BLUEPRINT §3): `std.Thread.Mutex` -> the guard-less
        // `rustty_zigstd::thread::Mutex` (the renderer `State.mutex` retype);
        // `alloc.create` -> `Box::into_raw` (infallible Rust allocation
        // collapses the `try`; the errdefer destroy is `Box::from_raw` at the
        // failure sites below / in initErrdefers).
        let mutex: *mut rustty_zigstd::thread::Mutex =
            Box::into_raw(Box::new(rustty_zigstd::thread::Mutex::new()));

        // Create the renderer thread
        // var render_thread = try rendererpkg.Thread.init(
        //     alloc,
        //     config,
        //     rt_surface,
        //     &self.renderer,
        //     &self.renderer_state,
        // );
        // (`2f0b65346` dropped the `app_mailbox` argument. PORT-ONLY, N437: the
        // thread also gets its own handle to the renderer's draw mutex, taken from
        // the local `renderer_impl` because `self.renderer` is not written yet.
        // N441: `&self.renderer` is the out-of-line renderer's raw root.)
        let render_thread = match crate::renderer_seam::Thread::<A>::init(
            alloc,
            config,
            rt_surface,
            renderer_ptr,
            // (N444: the out-of-line block's address.)
            renderer_state_slot.as_ptr(),
            unsafe { (*renderer_ptr).1.draw_mutex.clone() },
        ) {
            Ok(v) => v,
            Err(err) => {
                // (the armed errdefers, LIFO — Surface.zig:564/559/490)
                // errdefer alloc.destroy(mutex);
                unsafe { drop(Box::from_raw(mutex)) };
                // errdefer renderer_impl.deinit();
                // (N441: then the renderer is dropped and its allocation freed.)
                unsafe {
                    (*renderer_ptr).deinit();
                    renderer_impl.dealloc_after_deinit();
                }
                // (N444: the two never-written slots, after the renderer.)
                unsafe {
                    renderer_state_slot.dealloc();
                    renderer_thread_slot.dealloc();
                }
                // (errdefer app.font_grid_set.deref — UD115)
                unsafe { (*app).font_grid_set.deref(font_grid_key) };
                // errdefer derived_config.deinit();
                derived_config.deinit();
                return Err(err.into());
            }
        };
        // errdefer render_thread.deinit();
        // (fired at the io_thread failure site below / by initErrdefers.)

        // PORT NOTE: the io block's `.renderer_wakeup = render_thread.wakeup`
        // / `.renderer_mailbox = render_thread.mailbox` reads happen through
        // the Zig LOCAL after it was copied into `self`; Rust MOVES the local
        // at the big write below, so the `Copy` handles are read out first —
        // same values (xev handle unification, CORE_BLUEPRINT §5).
        // Windows (task W-2, N342): on IOCP `xev.Async` is a value, not a Copy
        // handle, and `.clone()` is Zig's by-value copy (the reference copies it
        // here, N343: record-only, faithful value semantics).
        #[allow(clippy::clone_on_copy)]
        let renderer_wakeup = render_thread.wakeup.clone();
        let renderer_mailbox = render_thread.mailbox;

        // Create the IO thread
        // var io_thread = try termio.Thread.init(alloc);
        //
        // (the ported `termio::Thread::init` collapses the `alloc` param —
        // its PORT NOTE.)
        //
        // Test-only (UD115, task R13-H2): an armed fault fails this init, the
        // last early return before the big write, so the test sees the grid
        // released here. Not compiled into production builds.
        #[cfg(feature = "init-fault-inject")]
        let io_thread_result = if crate::init_fault::inject(crate::init_fault::Fault::IoThreadInit)
        {
            Err(rustty_xev::Error)
        } else {
            termio::Thread::init()
        };
        #[cfg(not(feature = "init-fault-inject"))]
        let io_thread_result = termio::Thread::init();
        let io_thread = match io_thread_result {
            Ok(v) => v,
            Err(err) => {
                // (the armed errdefers, LIFO — Surface.zig:575/564/559/490)
                // errdefer render_thread.deinit();
                let mut render_thread = render_thread;
                render_thread.deinit();
                // errdefer alloc.destroy(mutex);
                unsafe { drop(Box::from_raw(mutex)) };
                // errdefer renderer_impl.deinit();
                // (N441: then the renderer is dropped and its allocation freed.)
                unsafe {
                    (*renderer_ptr).deinit();
                    renderer_impl.dealloc_after_deinit();
                }
                // (N444: the two never-written slots, after the renderer.)
                unsafe {
                    renderer_state_slot.dealloc();
                    renderer_thread_slot.dealloc();
                }
                // (errdefer app.font_grid_set.deref — UD115)
                unsafe { (*app).font_grid_set.deref(font_grid_key) };
                // errdefer derived_config.deinit();
                derived_config.deinit();
                return Err(err.into());
            }
        };
        // errdefer io_thread.deinit();
        // (fired by initErrdefers after the big write below.)

        // self.* = .{ ... };
        //
        // PORT NOTE: Zig writes the whole struct at once over the caller's
        // undefined memory; Rust has no placement struct-write with a
        // not-yet-constructible field (`.io = undefined`), so the write is
        // realized field-by-field with `ptr::write` (no drop of the
        // uninitialized old values), in the reference literal order. The
        // fields the literal omits take their declaration defaults
        // (pressed_key=null, last_binding_trigger=0, inspector=null,
        // child_exited=false, focused=true, visible=true, selection_scroll_active=false,
        // readonly=false, command_timer=null, search=null,
        // last_bell_time=null) — written at the end of the block.
        unsafe {
            // .id = id: {
            //     while (true) {
            //         const candidate = std.crypto.random.int(u64);
            //         if (candidate == 0) continue;
            //         break :id candidate;
            //     }
            // },
            //
            // PORT NOTE: `std.crypto.random.int(u64)` -> the rustty-os
            // `std.crypto.random.bytes` shim over 8 native-endian bytes.
            std::ptr::write(&raw mut (*self_).id, {
                loop {
                    let mut candidate_bytes = [0u8; 8];
                    internal_os::file::crypto_random_bytes(&mut candidate_bytes);
                    let candidate = u64::from_ne_bytes(candidate_bytes);
                    if candidate == 0 {
                        continue;
                    }
                    break candidate;
                }
            });
            // .alloc = alloc,
            std::ptr::write(&raw mut (*self_).alloc, alloc);
            // .app = app,
            std::ptr::write(&raw mut (*self_).app, app);
            // .rt_app = rt_app,
            std::ptr::write(&raw mut (*self_).rt_app, rt_app);
            // .rt_surface = rt_surface,
            std::ptr::write(&raw mut (*self_).rt_surface, rt_surface);
            // .font_grid_key = font_grid_key,
            std::ptr::write(&raw mut (*self_).font_grid_key, font_grid_key);
            // .font_size = font_size,
            std::ptr::write(&raw mut (*self_).font_size, font_size);
            // .font_size_adjusted = false,
            std::ptr::write(&raw mut (*self_).font_size_adjusted, false);
            // .font_metrics = font_grid.metrics,
            std::ptr::write(&raw mut (*self_).font_metrics, (*font_grid).metrics);
            // .renderer = renderer_impl,
            std::ptr::write(&raw mut (*self_).renderer, renderer_impl);
            // .renderer_thread = render_thread,
            //
            // (N444: into its out-of-line slot, then the owner into the field.)
            renderer_thread_slot.as_ptr().write(render_thread);
            std::ptr::write(&raw mut (*self_).renderer_thread, renderer_thread_slot);
            // (N444: `io` is out of line too, allocated uninitialized here so
            // that `renderer_state.terminal` below can take `&self.io.terminal`
            // while io is still undefined, as the reference does; `Termio.init`
            // writes it in place in the io block below.)
            std::ptr::write(
                &raw mut (*self_).io,
                ThreadSharedBox::<termio::Termio<SurfaceMailbox<A>>>::new_uninit(),
            );
            // .renderer_state = .{
            //     .mutex = mutex,
            //     .terminal = &self.io.terminal,
            // },
            //
            // (the `&self.io.terminal` address is taken while `io` is still
            // undefined — an address-only read, exactly the reference; the
            // State declaration defaults: inspector=null, preedit=null,
            // mouse=.{}.)
            //
            // (N444: into its out-of-line slot, then the owner into the field.)
            renderer_state_slot
                .as_ptr()
                .write(rendererpkg::state::State {
                    mutex,
                    terminal: &raw mut (*(*self_).io.as_ptr()).terminal,
                    inspector: None,
                    inspector_ptr: None,
                    preedit: None,
                    mouse: rendererpkg::state::Mouse::default(),
                    // D4 handoff (oracle d34b54e9b) - reference field defaults are `.init(0)`.
                    demand: Default::default(),
                    handoff_gen: Default::default(),
                    handoff_park: Default::default(),
                });
            std::ptr::write(&raw mut (*self_).renderer_state, renderer_state_slot);
            // .renderer_thr = undefined,
            // (§8.5 Option shape: None until the spawn below.)
            std::ptr::write(&raw mut (*self_).renderer_thr, None);
            // .mouse = .{},
            std::ptr::write(&raw mut (*self_).mouse, Mouse::default());
            // .keyboard = .{},
            std::ptr::write(&raw mut (*self_).keyboard, Keyboard::default());
            // .io = undefined,
            // (placement-written by the io block below — no Rust `undefined`;
            // N444: its out-of-line slot was written above.)
            // .io_thread = io_thread,
            //
            // (N444: in its own allocation.)
            std::ptr::write(&raw mut (*self_).io_thread, ThreadSharedBox::new(io_thread));
            // .io_thr = undefined,
            // (§8.5 Option shape: None until the spawn below.)
            std::ptr::write(&raw mut (*self_).io_thr, None);
            // .size = size,
            std::ptr::write(&raw mut (*self_).size, size);
            // .config = derived_config,
            std::ptr::write(&raw mut (*self_).config, derived_config);

            // Our conditional state is initialized to the app state. This
            // lets us get the most likely correct color theme and so on.
            // .config_conditional_state = app.config_conditional_state,
            std::ptr::write(
                &raw mut (*self_).config_conditional_state,
                (*app).config_conditional_state,
            );

            // (the declaration-default fields the reference literal omits)
            std::ptr::write(&raw mut (*self_).pressed_key, None);
            std::ptr::write(&raw mut (*self_).last_binding_trigger, 0);
            std::ptr::write(&raw mut (*self_).inspector, None);
            std::ptr::write(&raw mut (*self_).child_exited, false);
            std::ptr::write(&raw mut (*self_).focused, true);
            std::ptr::write(&raw mut (*self_).visible, true);
            std::ptr::write(&raw mut (*self_).selection_scroll_active, false);
            std::ptr::write(&raw mut (*self_).readonly, false);
            std::ptr::write(&raw mut (*self_).command_timer, None);
            std::ptr::write(&raw mut (*self_).search, None);
            std::ptr::write(&raw mut (*self_).last_bell_time, None);
        }

        // The command we're going to execute
        // const command: ?configpkg.Command = command: {
        //     if (app.first) {
        //         if (config.@"initial-command") |command| {
        //             break :command command;
        //         }
        //     }
        //     break :command config.command;
        // };
        //
        // PORT NOTE: the Zig union copy is shallow (slices into config
        // memory); the owned Rustty `Command` clones the bytes (the config
        // command.rs `Clone` PORT NOTE).
        let command: Option<configpkg::command::Command> = 'command: {
            if unsafe { (*app).first } {
                if let Some(command) = &config.initial_command {
                    break 'command Some(command.clone());
                }
            }
            config.command.clone()
        };

        // Start our IO implementation
        // This separate block ({}) is important because our errdefers must
        // be scoped here to be valid.
        {
            // var env = rt_surface.defaultTermioEnv() catch |err| env: {
            //     // If an error occurs, we don't want to block surface startup.
            //     log.warn("error getting env map for surface err={}", .{err});
            //     break :env internal_os.getEnvMap(alloc) catch
            //         std.process.EnvMap.init(alloc);
            // };
            //
            // PORT NOTE: the ported rustty-os `getEnvMap` is infallible (Rust
            // `std::env::vars`), so the second fallback
            // (`std.process.EnvMap.init`) collapses.
            let mut env = match unsafe { &*rt_surface }.default_termio_env() {
                Ok(env) => env,
                Err(err) => {
                    // If an error occurs, we don't want to block surface startup.
                    // log.warn("error getting env map for surface err={}", .{err});
                    log::warn!(target: LOG_SCOPE, "error getting env map for surface err={err:?}");
                    // PORT NOTE (P3-W7 T4): `internal_os.getEnvMap(alloc)` ->
                    // `global.environMap()` (`src/os/env.zig` was DELETED by
                    // `e8525c0fd`). This is the site area E §3.1 identified as
                    // the live non-UTF-8 PANIC; it is byte-typed now.
                    rustty_global::environ_map()
                }
            };
            // errdefer env.deinit();
            // (ownership drop covers — the map moves into the Exec config.)

            // don't leak GHOSTTY_LOG to any subprocesses
            // env.remove("GHOSTTY_LOG");
            //
            // PORT NOTE (R2 identity): the reference's `GHOSTTY_*` env names carry
            // the product identity — `<ENV_PREFIX>_<suffix>` (the termio
            // shell_integration `env_name` precedent).
            env.remove(format!("{}_LOG", rustty_identity::ENV_PREFIX).as_bytes());

            // var buf: [18]u8 = undefined;
            // try env.put(
            //     "GHOSTTY_SURFACE_ID",
            //     std.fmt.bufPrint(&buf, "0x{x:0>16}", .{self.id}) catch unreachable,
            // );
            //
            // ("0x" + 16 zero-padded hex digits fills the [18]u8 exactly; the
            // `catch unreachable` collapses — `format!` is infallible.)
            env.insert(
                format!("{}_SURFACE_ID", rustty_identity::ENV_PREFIX).into_bytes(),
                format!("0x{:016x}", unsafe { (*self_).id }).into_bytes(),
            );

            // Initialize our IO backend
            // var io_exec = try termio.Exec.init(alloc, .{
            //     .command = command,
            //     .env = env,
            //     .env_override = config.env,
            //     .shell_integration = config.@"shell-integration",
            //     .shell_integration_features = config.@"shell-integration-features",
            //     .cursor_blink = config.@"cursor-style-blink",
            //     .working_directory = if (config.@"working-directory") |wd| wd.value() else null,
            //     .resources_dir = global_state.resources_dir.host(),
            //     .term = config.term,
            //     .rt_pre_exec_info = .init(config),
            //     .rt_post_fork_info = .init(config),
            // });
            let mut io_exec = match termio::Exec::init(
                alloc,
                termio::exec::Config {
                    command: command.clone(),
                    env,
                    env_override: config.env.clone(),
                    shell_integration: config.shell_integration,
                    shell_integration_features: config.shell_integration_features,
                    cursor_blink: config.cursor_style_blink,
                    working_directory: config
                        .working_directory
                        .as_ref()
                        .and_then(|wd| wd.value())
                        .map(|v| v.to_vec()),
                    resources_dir: crate::global_state()
                        .resources_dir
                        .host()
                        .map(<[u8]>::to_vec),
                    term: config.term.clone(),
                    rt_pre_exec_info: termio::command::RtPreExecInfo::init(config),
                    rt_post_fork_info: termio::command::RtPostForkInfo::init(config),
                },
            ) {
                Ok(v) => v,
                Err(err) => {
                    // (the armed errdefer chain — initErrdefers)
                    unsafe { Self::initErrdefers(self_, false) };
                    return Err(err.into());
                }
            };
            // errdefer io_exec.deinit();

            // Initialize our IO mailbox
            // var io_mailbox = try termio.Mailbox.initSPSC(alloc);
            //
            // (the ported `initSPSC` collapses the `alloc` param — its PORT
            // NOTE.)
            let io_mailbox = match termio::Mailbox::initSPSC() {
                Ok(v) => v,
                Err(err) => {
                    // errdefer io_exec.deinit(); (Surface.zig:662)
                    io_exec.deinit();
                    // (+ the armed errdefer chain — initErrdefers)
                    unsafe { Self::initErrdefers(self_, false) };
                    return Err(err.into());
                }
            };
            // errdefer io_mailbox.deinit(alloc);
            // (moves into the Termio options below; on ITS failure the moved
            // options drop covers the ownership when the deferred
            // `Termio.init` lands — D6.)

            // try termio.Termio.init(&self.io, alloc, .{
            //     .size = size,
            //     .full_config = config,
            //     .config = try termio.Termio.DerivedConfig.init(alloc, config),
            //     .backend = .{ .exec = io_exec },
            //     .mailbox = io_mailbox,
            //     .renderer_state = &self.renderer_state,
            //     .renderer_wakeup = render_thread.wakeup,
            //     .renderer_mailbox = render_thread.mailbox,
            //     .surface_mailbox = .{ .surface = self, .app = app_mailbox },
            // });
            //
            // PORT NOTE ([ADV-FIX F1]): the reference `try termio.Termio.init(
            // &self.io, alloc, .{...})` is an IN-PLACE out-param initializer, and the
            // port matches it (`unsafe fn init(self_: *mut Termio<M>, ...)`). This
            // is MANDATORY: `StreamHandler` holds raw self-pointers back into the
            // owning Termio (`terminal`/`size`/`termio_mailbox`); a by-value return
            // + a `ptr::write` here would copy those pointer *values* but leave them
            // aimed at the old return slot → dangling. `init` writes every field at
            // the FINAL `(*self_).io` address. (`termio::DerivedConfig::init` is
            // infallible — the reference `try` collapses, its PORT NOTE.)
            match unsafe {
                termio::Termio::init(
                    // (N444: the out-of-line io block.)
                    (*self_).io.as_ptr(),
                    alloc,
                    termio::Options {
                        size,
                        full_config: config,
                        config: termio::DerivedConfig::init(config),
                        backend: termio::Backend::Exec(io_exec),
                        mailbox: io_mailbox,
                        renderer_state: (*self_).renderer_state.as_ptr(),
                        renderer_wakeup,
                        renderer_mailbox,
                        surface_mailbox: SurfaceMailbox {
                            surface: self_,
                            app: app_mailbox,
                        },
                    },
                )
            } {
                Ok(()) => {}
                Err(err) => {
                    // (errdefer io_mailbox.deinit(alloc) + errdefer
                    // io_exec.deinit() — both moved into the consumed options;
                    // + the armed errdefer chain — initErrdefers)
                    unsafe { Self::initErrdefers(self_, false) };
                    return Err(err.into());
                }
            }
        }
        // Outside the block, IO has now taken ownership of our temporary state
        // so we can just defer this and not the subcomponents.
        // errdefer self.io.deinit();
        // (io_live=true in the initErrdefers calls from here on.)

        // Report initial cell size on surface creation
        // _ = try rt_app.performAction(
        //     .{ .surface = self },
        //     .cell_size,
        //     .{ .width = size.cell.width, .height = size.cell.height },
        // );
        let _ = unsafe {
            Self::initTry(
                self_,
                true,
                (*rt_app).perform_action(
                    apprt::Target::surface_ptr(self_),
                    apprt::Action::CellSize(CellSize {
                        width: size.cell.width,
                        height: size.cell.height,
                    }),
                ),
            )
        }?;

        // _ = try rt_app.performAction(
        //     .{ .surface = self },
        //     .size_limit,
        //     .{
        //         .min_width = size.cell.width * min_window_width_cells,
        //         .min_height = size.cell.height * min_window_height_cells,
        //         // No max:
        //         .max_width = 0,
        //         .max_height = 0,
        //     },
        // );
        //
        // PORT NOTE: reference defect (R13, UPSTREAM-DEFECTS.md UD59). Both
        // products are `u32`. `adjust-cell-width` / `adjust-cell-height` add an
        // `i32` to the cell size (saturating), so a cell wider than
        // `u32::MAX / 10` (or taller than `u32::MAX / 4`) makes the product
        // overflow: ReleaseSafe panics, ReleaseFast reports a wrapped minimum.
        // The arithmetic is config-reachable; whether surface init survives such
        // a cell up to this line was not measured. The reference offers no error
        // path here, so the port skips the `size_limit` action when a product
        // overflows (the most conservative choice: no minimum is imposed) —
        // `size_limit_min`.
        if let Some((min_width, min_height)) = size_limit_min(size.cell.width, size.cell.height) {
            let _ = unsafe {
                Self::initTry(
                    self_,
                    true,
                    (*rt_app).perform_action(
                        apprt::Target::surface_ptr(self_),
                        apprt::Action::SizeLimit(SizeLimit {
                            min_width,
                            min_height,
                            // No max:
                            max_width: 0,
                            max_height: 0,
                        }),
                    ),
                )
            }?;
        }

        // Call our size callback which handles all our retina setup
        // Note: this shouldn't be necessary and when we clean up the surface
        // init stuff we should get rid of this. But this is required because
        // sizeCallback does retina-aware stuff we don't do here and don't want
        // to duplicate.
        // try self.resize(self.size.screen);
        {
            let screen = unsafe { (*self_).size.screen };
            unsafe { Self::initTry(self_, true, (*self_).resize(screen)) }?;
        }

        // P7-W7 T2 (`2f0b65346`): `try renderer_impl.finalizeSurfaceInit(rt_surface);`
        // ("Give the renderer one more opportunity to finalize any surface setup
        // on the main thread prior to spinning up the rendering thread.") is gone.

        // W-ATTACH (macos-swift-app tier): route the host NSView into the
        // renderer's windowed layer-attach so frames present to the window (the
        // renderer was built offscreen via `init_offscreen`; without this its
        // `present` drops every frame into a detached `IOSurface` → a blank
        // window). MUST run here — on the main thread, BEFORE the render thread
        // spawn below — so `loop_enter` installs the
        // display callback against `Some(layer)`. `host_view` is null for
        // non-view-hosted backends, where `attachHostView` is a no-op.
        unsafe {
            let view = (*rt_surface).host_view();
            let scale = (*rt_surface).host_content_scale();
            Self::initTry(
                self_,
                true,
                (*(*self_).renderer.as_ptr()).attachHostView(view, scale),
            )
        }?;

        // Start our renderer thread
        // self.renderer_thr = try std.Thread.spawn(
        //     .{},
        //     rendererpkg.Thread.threadMain,
        //     .{&self.renderer_thread},
        // );
        // self.renderer_thr.setName("renderer") catch {};
        //
        // PORT NOTE ([ADV-FIX 6b]): the spawn argument is the documented
        // Send-wrapper newtype ([`RendererThreadArg`]); the thread name is
        // set via the pre-spawn `Builder` (Rust has no post-spawn setName),
        // whose naming cannot fail — the reference's ignored `setName`
        // failure (`catch {}`) collapses.
        // (N444: the out-of-line block's raw root.)
        let arg = RendererThreadArg::<A>(unsafe { (*self_).renderer_thread.as_ptr() });
        // Test-only (task R13-H2): observes the thread itself, independently
        // of the logged join (see `init_fault::ThreadProbe`). Not compiled
        // into production builds.
        #[cfg(feature = "init-fault-inject")]
        let probe =
            crate::init_fault::ThreadProbe::start(crate::init_fault::ProbedThread::Renderer);
        let spawn_result = rustty_zigstd::thread::spawn_config()
            .name("renderer".to_string())
            .spawn(move || {
                // (the probe is dropped when this closure ends)
                #[cfg(feature = "init-fault-inject")]
                let _probe = probe;
                // (capture the whole Send wrapper, not the raw-ptr field —
                // edition-2021 disjoint captures would otherwise capture the
                // `!Send` pointer directly.)
                let arg = arg;
                // [ADV-FIX F1/F2] (E2b): `Thread::threadMain` is an associated fn
                // taking RAW `*mut Thread<A>` (NOT `&mut`) — the child holds it for
                // its whole life and the xev loop re-hands it into every callback;
                // a `&mut` retag would alias those + the main thread's pre-join
                // `stop.notify()`. The io-thread precedent below.
                unsafe { crate::renderer_seam::Thread::<A>::threadMain(arg.0) }
            });
        let renderer_thr = unsafe { Self::initTry(self_, true, spawn_result) }?;
        unsafe { (*self_).renderer_thr = Some(renderer_thr) };
        #[cfg(feature = "init-fault-inject")]
        crate::init_fault::record(crate::init_fault::Event::RendererSpawned);

        // Start our IO thread
        // self.io_thr = try std.Thread.spawn(
        //     .{},
        //     termio.Thread.threadMain,
        //     .{ &self.io_thread, &self.io },
        // );
        // self.io_thr.setName("io") catch {};
        //
        // (same [ADV-FIX 6b] newtype + Builder-name mapping as above.)
        // (N444: the out-of-line blocks' raw roots.)
        let args = IoThreadArgs::<A> {
            thread: unsafe { (*self_).io_thread.as_ptr() },
            io: unsafe { (*self_).io.as_ptr() },
        };
        let io_main = move || {
            // (whole-wrapper capture — same note as the renderer spawn
            // above.)
            let args = args;
            // [ADV-FIX F2]: `Thread::threadMain` is an associated fn taking RAW
            // `*mut Thread`/`*mut Termio<M>` (NOT `&mut`) — a `&mut` arg would be
            // a protected `Unique` retag for the whole `loop.run` (shell life),
            // aliasing the concurrent ReadThread + the parent's pre-join
            // `io_thread.stop.notify()`. Pass the raw pointers straight through.
            unsafe { termio::Thread::threadMain(args.thread, args.io) }
        };
        let io_builder = rustty_zigstd::thread::spawn_config().name("io".to_string());
        // Test-only (UD104, task R13-D2): an armed fault fails the spawn
        // without starting the thread. Not compiled into production builds.
        #[cfg(feature = "init-fault-inject")]
        let spawn_result = if crate::init_fault::inject(crate::init_fault::Fault::IoSpawn) {
            Err(std::io::Error::other(
                "init-fault-inject: the io thread spawn failed",
            ))
        } else {
            // (the R13-H2 thread probe, as for the renderer above)
            let probe = crate::init_fault::ThreadProbe::start(crate::init_fault::ProbedThread::Io);
            io_builder.spawn(move || {
                // (the probe is dropped when this closure ends)
                let _probe = probe;
                io_main()
            })
        };
        #[cfg(not(feature = "init-fault-inject"))]
        let spawn_result = io_builder.spawn(io_main);
        let io_thr = unsafe { Self::initTry(self_, true, spawn_result) }?;
        unsafe { (*self_).io_thr = Some(io_thr) };
        #[cfg(feature = "init-fault-inject")]
        crate::init_fault::record(crate::init_fault::Event::IoSpawned);

        // Determine our initial window size if configured. We need to do this
        // quite late in the process because our height/width are in grid dimensions,
        // so we need to know our cell sizes first.
        //
        // Note: it is important to do this after the renderer is setup above.
        // This allows the apprt to fully initialize the surface before we
        // start messing with the window.
        // self.recomputeInitialSize() catch |err| { ... };
        if let Err(err) = unsafe { (*self_).recomputeInitialSize() } {
            // We don't treat this as a fatal error because not setting
            // an initial size shouldn't stop our terminal from working.
            // log.warn("unable to set initial window size: {}", .{err});
            log::warn!(target: LOG_SCOPE, "unable to set initial window size: {err:?}");
        }

        // Test-only (UD104, task R13-D2): an armed fault fails here, where the
        // first `set_title` `try` below would, with both threads running. Not
        // compiled into production builds.
        #[cfg(feature = "init-fault-inject")]
        if crate::init_fault::inject(crate::init_fault::Fault::AfterIoSpawn) {
            unsafe {
                Self::initTry(
                    self_,
                    true,
                    Err::<(), _>(InitError::Spawn(std::io::Error::other(
                        "init-fault-inject: a step after the io thread spawn failed",
                    ))),
                )
            }?;
        }

        // if (config.title) |title| {
        if let Some(title) = &config.title {
            // _ = try rt_app.performAction(
            //     .{ .surface = self },
            //     .set_title,
            //     .{ .title = title },
            // );
            let _ = unsafe {
                Self::initTry(
                    self_,
                    true,
                    (*rt_app).perform_action(
                        apprt::Target::surface_ptr(self_),
                        apprt::Action::SetTitle(SetTitle { title }),
                    ),
                )
            }?;
        // } else if ((comptime builtin.os.tag == .linux) and
        //     config.@"_xdg-terminal-exec")
        // xdg: {
        } else if cfg!(target_os = "linux") && config._xdg_terminal_exec {
            'xdg: {
                // For xdg-terminal-exec execution we special-case and set the window
                // title to the command being executed. This allows window managers
                // to set custom styling based on the command being executed.
                // const v = command orelse break :xdg;
                let v = match &command {
                    Some(v) => v,
                    None => break 'xdg,
                };
                // const title = v.string(alloc) catch |err| {
                //     log.warn(
                //         "error copying command for title, title will not be set err={}",
                //         .{err},
                //     );
                //     break :xdg;
                // };
                //
                // PORT NOTE: the ported `Command::string` is infallible (the
                // reference error is OOM-only) — the catch arm collapses.
                let title = v.string();
                // defer alloc.free(title);
                // (ownership drop at scope end.)
                // _ = try rt_app.performAction(
                //     .{ .surface = self },
                //     .set_title,
                //     .{ .title = title },
                // );
                let _ = unsafe {
                    Self::initTry(
                        self_,
                        true,
                        (*rt_app).perform_action(
                            apprt::Target::surface_ptr(self_),
                            apprt::Action::SetTitle(SetTitle { title: &title }),
                        ),
                    )
                }?;
            }
        // } else if (command) |cmd| switch (cmd) {
        } else if let Some(cmd) = &command {
            match cmd {
                // If a user specifies a command it is appropriate to set the title as argv[0]
                // we know in the case of a direct command it has been supplied by the user
                // .direct => |cmd_str| if (cmd_str.len != 0) { ... },
                configpkg::command::Command::Direct(cmd_str) => {
                    if !cmd_str.is_empty() {
                        // _ = try rt_app.performAction(
                        //     .{ .surface = self },
                        //     .set_title,
                        //     .{ .title = cmd_str[0] },
                        // );
                        let _ = unsafe {
                            Self::initTry(
                                self_,
                                true,
                                (*rt_app).perform_action(
                                    apprt::Target::surface_ptr(self_),
                                    apprt::Action::SetTitle(SetTitle { title: &cmd_str[0] }),
                                ),
                            )
                        }?;
                    }
                }

                // We won't set the title in the case the shell expands the command
                // as that should typically be used to launch a shell which should
                // set its own titles
                // .shell => {},
                configpkg::command::Command::Shell(_) => {}
            }
        }

        // We are no longer the first surface
        // app.first = false;
        unsafe { (*app).first = false };

        // (the reference `defer if (config_) |*c| c.deinit();` — success
        // path; see the PORT NOTE at the defer site above.)
        if let Some(c) = config_.as_mut() {
            c.deinit();
        }

        Ok(())
    }

    /// PORT NOTE (Rust-only glue): the error-path unwinding for failures
    /// AFTER the reference's `self.* = .{...}` write (Surface.zig:581). Zig's
    /// armed errdefers unwind through the LOCALS (`derived_config.deinit()`
    /// :490, `renderer_impl.deinit()` :559, `alloc.destroy(mutex)` :564,
    /// `render_thread.deinit()` :575, `io_thread.deinit()` :579, and — once
    /// the io block completes — `self.io.deinit()` :682), whose Zig struct
    /// copies alias the state just moved into `self`; Rust single ownership
    /// reads the fields back OUT of `self_` instead, in the same LIFO order.
    /// The abandoned `self_` memory is then the caller's to free without
    /// dropping (the failed-`init` contract).
    ///
    /// # Safety
    ///
    /// `self_` must point at a Surface whose big-write fields are initialized
    /// (io additionally initialized iff `io_live`); afterwards the pointee is
    /// logically uninitialized.
    unsafe fn initErrdefers(self_: *mut Surface<A>, io_live: bool) {
        unsafe {
            // PORT NOTE: reference defect (R13), UPSTREAM-DEFECTS.md UD104. The
            // reference's errdefers (`render_thread.deinit()` :582,
            // `io_thread.deinit()` :586, `self.io.deinit()`) free the thread
            // loops and the terminal while the renderer and io threads spawned
            // at :725/:733 are still running, when a later `try` fails (the io
            // thread's own spawn, or `performAction(.set_title)`). Every
            // spawned thread is therefore stopped and joined FIRST, in
            // `deinit`'s order (renderer, then io), before any teardown below:
            // the renderer thread reads the terminal that `self.io.deinit()`
            // frees, so joining it only after that deinit (as this function
            // used to) left the same race in the port. Task R13-D moved the
            // renderer join here and added the io join. Test (task R13-D2):
            // `init_error_path_joins_both_threads_before_freeing_their_state`
            // (rustty-apprt-c `tests/macos_window_host.rs`).
            //
            // P7-W7 T2: this note used to say that, unlike `deinit`, this path
            // does not re-enter the renderer (`threadEnter`, Surface.zig:811 at
            // the pin) before `renderer.deinit()`, and that this was harmless
            // because OpenGL's `threadEnter` did nothing. Upstream deleted that
            // re-entry from `deinit` (`2f0b65346`): the render thread's own
            // `threadExit` frees the GPU objects while its context is current,
            // and `renderer.deinit()` frees CPU state only. Both paths now agree.
            //
            // Stop the renderer via a NARROW Copy handle (no whole-struct read
            // while it may still be live — §2.2 rule 4; Async::notify(&self)
            // touches only the mach port). Join the spawned handle if init
            // still holds it on this path; else the never-started child is
            // simply absent.
            //
            // (The `init_fault` lines are the test-only probe, task R13-D2: they
            // log each stop and join, and at each free end the process first if
            // a spawned thread is still unjoined. Not compiled into production
            // builds.)
            if let Some(handle) = (*self_).renderer_thr.take() {
                let _ = (*(*self_).renderer_thread.as_ptr()).stop.notify();
                #[cfg(feature = "init-fault-inject")]
                crate::init_fault::record(crate::init_fault::Event::RendererStopped);
                let _ = handle.join();
                #[cfg(feature = "init-fault-inject")]
                crate::init_fault::record(crate::init_fault::Event::RendererJoined);
            }
            // The io thread, if it was spawned (`io_thr` is stored straight
            // after its spawn succeeds), the same way `deinit` stops it.
            if let Some(handle) = (*self_).io_thr.take() {
                let _ = (*(*self_).io_thread.as_ptr()).stop.notify();
                #[cfg(feature = "init-fault-inject")]
                crate::init_fault::record(crate::init_fault::Event::IoStopped);
                let _ = handle.join();
                #[cfg(feature = "init-fault-inject")]
                crate::init_fault::record(crate::init_fault::Event::IoJoined);
            }

            // errdefer self.io.deinit(); (Surface.zig:682 — armed only after
            // the io block; consuming receiver, read back out.)
            if io_live {
                #[cfg(feature = "init-fault-inject")]
                crate::init_fault::before_free("io");
                std::ptr::read((*self_).io.as_ptr()).deinit();
            }
            // (N444: the io block's own allocation, initialized or not; the
            // threads are joined above.)
            (*self_).io.dealloc();
            // errdefer io_thread.deinit(); (Surface.zig:579 — consuming
            // receiver, read back out.)
            #[cfg(feature = "init-fault-inject")]
            crate::init_fault::before_free("io_thread");
            std::ptr::read((*self_).io_thread.as_ptr()).deinit();
            // (N444: then its own allocation.)
            (*self_).io_thread.dealloc();
            // errdefer render_thread.deinit(); (Surface.zig:575)
            //
            // The renderer child was stopped and joined at the top of this
            // function (UD104), so read the field out by-value (no whole
            // `&mut renderer_thread` — E2's aliasing discipline) and deinit,
            // matching the io half above.
            {
                #[cfg(feature = "init-fault-inject")]
                crate::init_fault::before_free("renderer_thread");
                let mut rt = std::ptr::read((*self_).renderer_thread.as_ptr());
                rt.deinit();
            }
            // (N444: then its own allocation; `rt` above was read out of it.)
            (*self_).renderer_thread.dealloc();
            // errdefer alloc.destroy(mutex); (Surface.zig:564)
            #[cfg(feature = "init-fault-inject")]
            crate::init_fault::before_free("renderer_state.mutex");
            drop(Box::from_raw((*(*self_).renderer_state.as_ptr()).mutex));
            // errdefer renderer_impl.deinit(); (Surface.zig:559)
            (*(*self_).renderer.as_ptr()).deinit();
            // (N441: then the renderer is dropped and its own allocation freed;
            // the caller frees the surface slot itself without glue.)
            (*self_).renderer.dealloc_after_deinit();
            // (N444: then the state's own allocation, after the renderer, so
            // its bytes outlive every teardown above as the inline field did.)
            (*self_).renderer_state.dealloc();
            // PORT NOTE: reference defect (R13), UPSTREAM-DEFECTS.md UD115: the
            // missing `errdefer app.font_grid_set.deref(font_grid_key)` (see the
            // `ref` site). The by-value Key is read out of `self_`, as
            // `deinit` does, which also releases the port's owned copy of it.
            (*(*self_).app)
                .font_grid_set
                .deref(std::ptr::read(&raw const (*self_).font_grid_key));
            // errdefer derived_config.deinit(); (Surface.zig:490 — the
            // explicit regex teardown + the collapsed-arena ownership drop.)
            let mut derived_config = std::ptr::read(&raw const (*self_).config);
            derived_config.deinit();
            drop(derived_config);
        }
    }

    /// PORT NOTE (Rust-only glue): Zig `try` under the armed post-write
    /// errdefer chain — on `Err` fires [`Surface::initErrdefers`] and
    /// propagates (typed into [`InitError`]).
    ///
    /// # Safety
    ///
    /// Same contract as [`Surface::initErrdefers`] (which runs on `Err`).
    unsafe fn initTry<T, E: Into<InitError>>(
        self_: *mut Surface<A>,
        io_live: bool,
        result: Result<T, E>,
    ) -> Result<T, InitError> {
        match result {
            Ok(v) => Ok(v),
            Err(err) => {
                unsafe { Self::initErrdefers(self_, io_live) };
                Err(err.into())
            }
        }
    }

    // pub fn deinit(self: *Surface) void {
    //
    /// PORT NOTE: the reference invalidates the pointee in place and the
    /// apprt then frees the memory WITHOUT re-running field teardown (Zig
    /// `alloc.destroy`). The consuming sub-deinits (`Search`,
    /// `termio.Thread`, `Termio`, the grid-set key) are `ptr::read` out of
    /// `self`, and the owned containers are dropped explicitly (the collapsed
    /// Zig-allocator frees).
    ///
    /// # Safety
    ///
    /// The surface must be fully initialized (a successful [`Surface::init`]).
    /// After `deinit` returns, `*self` must not be used or dropped; the owner
    /// must free the allocation without dropping (the reference
    /// `alloc.destroy` contract).
    pub unsafe fn deinit(&mut self) {
        // Stop search thread
        // if (self.search) |*s| s.deinit();
        //
        // (Option take — the C5 `Search::deinit` by-value receiver note.)
        if let Some(s) = self.search.take() {
            s.deinit();
        }

        // ORDERING INVARIANT (WS7 Phase C review, M5 — prospective, not a
        // current bug): search is torn down BEFORE the renderer thread is
        // stopped/joined below, so nothing here holds `renderer::State`'s
        // `mutex`/`opts.mutex` across that join. A future change to this
        // teardown order (or a new consumer that locks `mutex` around a
        // join) should re-check this invariant against the D4 mutex-handoff
        // machinery (`crates/rustty-renderer/src/state.rs`) to avoid a
        // deadlock.

        // Stop rendering thread
        {
            // self.renderer_thread.stop.notify() catch |err|
            //     log.err("error notifying renderer thread to stop, may stall err={}", .{err});
            //
            // (N444: the out-of-line block, by place; `notify` is `&self`.)
            if let Err(err) = unsafe { (*self.renderer_thread.as_ptr()).stop.notify() } {
                // log.err("error notifying renderer thread to stop, may stall err={}", .{err});
                log::error!(
                    target: LOG_SCOPE,
                    "error notifying renderer thread to stop, may stall err={err:?}"
                );
            }
            // self.renderer_thr.join();
            //
            // (§8.5 Option shape: Some after init's spawn; join consumes.)
            self.renderer_thr
                .take()
                .expect("renderer_thr is set by init")
                .join()
                .unwrap();

            // (`2f0b65346` deleted "We need to become the active rendering thread
            // again / self.renderer.threadEnter(self.rt_surface) catch
            // unreachable;": the renderer owns its context, and its thread's
            // `threadExit` already released the GPU resources.)
        }

        // Stop our IO thread
        {
            // self.io_thread.stop.notify() catch |err|
            //     log.err("error notifying io thread to stop, may stall err={}", .{err});
            //
            // (N444: the out-of-line block, by place; `notify` is `&self`.)
            if let Err(err) = unsafe { (*self.io_thread.as_ptr()).stop.notify() } {
                // log.err("error notifying io thread to stop, may stall err={}", .{err});
                log::error!(
                    target: LOG_SCOPE,
                    "error notifying io thread to stop, may stall err={err:?}"
                );
            }
            // self.io_thr.join();
            self.io_thr
                .take()
                .expect("io_thr is set by init")
                .join()
                .unwrap();
        }

        // We need to deinit AFTER everything is stopped, since there are
        // shared values between the two threads.
        // self.renderer_thread.deinit();
        //
        // (N444: the render thread is joined above; then the block's own
        // allocation is freed without glue, as the inline field was never
        // dropped.)
        unsafe {
            (*self.renderer_thread.as_ptr()).deinit();
            self.renderer_thread.dealloc();
        }
        // self.renderer.deinit();
        //
        // (N441: the render thread is joined above, so the exclusive borrow of
        // the out-of-line renderer overlaps nothing.)
        unsafe {
            (*self.renderer.as_ptr()).deinit();
            // (N441: then the renderer is dropped and its own allocation freed;
            // the apprt frees the surface itself without glue.)
            self.renderer.dealloc_after_deinit();
        }
        // self.io_thread.deinit();
        //
        // (consuming receiver — read out of the invalidated-in-place self.)
        //
        // (N444: read out of its own allocation, which is then freed.)
        unsafe {
            std::ptr::read(self.io_thread.as_ptr()).deinit();
            self.io_thread.dealloc();
        }
        // self.mouse.selection_gesture.deinit(&self.io.terminal);
        self.mouse
            .selection_gesture
            .deinit(unsafe { &mut (*self.io.as_ptr()).terminal });
        // self.io.deinit();
        //
        // (consuming receiver — read out of the invalidated-in-place self;
        // N444: out of its own allocation, which is then freed.)
        unsafe {
            std::ptr::read(self.io.as_ptr()).deinit();
            self.io.dealloc();
        }

        // if (self.inspector) |v| {
        //     v.deinit(self.alloc);
        //     self.alloc.destroy(v);
        // }
        //
        // PORT NOTE (Task 6): `activateInspector` now constructs the inspector, so
        // a Surface torn down with an active inspector must free it here (the
        // deactivate path nulls `self.inspector`, so this fires only when a live
        // inspector outlives the Surface). `alloc.destroy` -> `Box::from_raw` drop.
        if let Some(v) = self.inspector {
            unsafe { (*v).deinit(self.alloc) };
            unsafe { drop(Box::from_raw(v)) };
        }

        // Clean up our keyboard state
        // for (self.keyboard.sequence_queued.items) |req| req.deinit();
        //
        // (`WriteReq.deinit` — the alloc-variant free — is Drop in Rustty;
        // the datastruct MessageData precedent.)
        for req in self.keyboard.sequence_queued.drain(..) {
            drop(req);
        }
        // self.keyboard.sequence_queued.deinit(self.alloc);
        drop(std::mem::take(&mut self.keyboard.sequence_queued));
        // self.keyboard.table_stack.deinit(self.alloc);
        drop(std::mem::take(&mut self.keyboard.table_stack));

        // Clean up our font grid
        // self.app.font_grid_set.deref(self.font_grid_key);
        //
        // (the by-value Key is read out of the invalidated-in-place self.)
        unsafe {
            (*self.app)
                .font_grid_set
                .deref(std::ptr::read(&raw const self.font_grid_key));
        }

        // Clean up our render state
        // if (self.renderer_state.preedit) |p| self.alloc.free(p.codepoints);
        //
        // (N444: the out-of-line state, by place; every thread is joined.)
        if let Some(p) = unsafe { (*self.renderer_state.as_ptr()).preedit.take() } {
            drop(p.codepoints);
        }
        // self.alloc.destroy(self.renderer_state.mutex);
        //
        // (N444: then the state's own allocation, without glue, as the inline
        // field was never dropped.)
        unsafe {
            drop(Box::from_raw((*self.renderer_state.as_ptr()).mutex));
            self.renderer_state.dealloc();
        }
        // self.config.deinit();
        //
        // (explicit regex teardown + the collapsed-arena ownership drop —
        // read out of the invalidated-in-place self.)
        let mut config = unsafe { std::ptr::read(&raw const self.config) };
        config.deinit();
        drop(config);

        // log.info("surface closed id={x}", .{self.id});
        //
        // CORRECTED (task 14d-1): the comment carried `addr={x}` /
        // `@intFromPtr(self)`; at pin 20abdb50a `Surface.zig:847` reads
        // `id={x}` / `self.id`. The reference wins.
        log::info!(target: LOG_SCOPE, "surface closed id={:x}", self.id);
    }

    /// Close this surface. This will trigger the runtime to start the
    /// close process, which should ultimately deinitialize this surface.
    // pub fn close(self: *Surface) void {
    //
    // PORT NOTE (N465, P7-W12 T1): raw-self. The runtime's close callback
    // re-enters this surface synchronously: on macOS the close notification's
    // handler removes the view from the split tree, which cancels a pending
    // clipboard confirmation → `rustty_surface_deny_clipboard_request` →
    // `denyClipboardRequest` (a `&mut` embedded surface, which holds this one
    // inline); on GTK `close-request` → `SplitTree.surfaceCloseRequest` →
    // `needsConfirmQuit` on this surface. So no protected `&mut Surface`
    // spans `rt_surface.close`: `needsConfirmQuit`'s reborrow ends first, and
    // the runtime surface is reached by place (N442's shape).
    ///
    /// # Safety
    /// `self_` is the stable Surface allocation (init's stability contract); no
    /// protected `&mut Surface` may span the call, and no reference to it may
    /// be used after the call returns without being taken afresh.
    pub unsafe fn close(self_: *mut Self) {
        // self.rt_surface.close(self.needsConfirmQuit());
        let process_alive = unsafe { &mut *self_ }.needsConfirmQuit();
        let rt_surface = unsafe { (*self_).rt_surface };
        unsafe { <A::Surface as apprt::Surface>::close(rt_surface, process_alive) };
    }

    /// Returns a mailbox that can be used to send messages to this surface.
    // inline fn surfaceMailbox(self: *Surface) Mailbox {
    //
    // PORT NOTE: reference `inline fn` -> `#[inline]`. The `&mut self` form is
    // preserved verbatim (R1) but currently dead — its sole reached caller,
    // `searchCallback_`, now uses the raw `surface_mailbox_raw` variant (§3.3a,
    // to avoid a whole `&mut Surface` reborrow on the search thread; since N444
    // fix round 1 it is built on the main thread at search spawn and copied into
    // `SearchCallbackCtx`). Future main-thread callers of other ported arms use
    // this `&mut self` form.
    #[allow(dead_code)]
    #[inline]
    fn surfaceMailbox(&mut self) -> Mailbox<A> {
        // return .{
        //     .surface = self,
        //     .app = .{ .rt_app = self.rt_app, .mailbox = &self.app.mailbox },
        // };
        SurfaceMailbox {
            surface: self,
            app: crate::app::Mailbox {
                rt_app: self.rt_app,
                mailbox: unsafe { &raw mut (*self.app).mailbox },
            },
        }
    }

    /// Raw-`*mut Surface` variant of [`surfaceMailbox`](Self::surfaceMailbox).
    //
    // PORT NOTE (SEARCH_BINDING_ARM_DESIGN §3.3a): the search-thread callback
    // (`searchCallback_`) must NOT form a whole `&mut Surface` — that reborrow,
    // fired ON THE SEARCH THREAD while the main thread holds `&mut self` (every
    // `performBindingAction` arm), is Stacked-Borrows UB (mirihar `callback_naive`).
    // `surfaceMailbox` takes `&mut self` only to coerce `self` into the
    // `SurfaceMailbox { surface: *mut Surface, .. }` literal, whose fields are all
    // raw projections — so this variant builds the SAME literal directly from a
    // raw `*mut Surface` with NO `&mut Surface`/`&Surface` (mirihar `callback_shipped`,
    // SB + TB clean).
    #[inline]
    pub(super) unsafe fn surface_mailbox_raw(self_: *mut Surface<A>) -> Mailbox<A> {
        SurfaceMailbox {
            surface: self_,
            app: crate::app::Mailbox {
                rt_app: unsafe { (*self_).rt_app },
                mailbox: unsafe { &raw mut (*(*self_).app).mailbox },
            },
        }
    }

    /// Queue a message for the IO thread, taking ownership of `msg`.
    ///
    /// We centralize all our logic into this spot so we can intercept
    /// messages for example in readonly mode.
    // fn queueIo(
    //     self: *Surface,
    //     msg: termio.Message,
    //     mutex: termio.Termio.MutexState,
    // ) void {
    //
    // PORT NOTE: `Message<'static>` — the queued instantiation (the termio
    // cross-thread-queue precedent).
    pub(super) fn queueIo(
        &mut self,
        msg: termio::Message<'static>,
        mutex: termio::termio_impl::MutexState,
    ) {
        // In readonly mode, we don't allow any writes through to the pty.
        // if (self.readonly) {
        if self.readonly {
            // switch (msg) {
            match msg {
                // .write_small,
                // .write_stable,
                // .write_alloc,
                // => {
                //     msg.deinit();
                //     return;
                // },
                //
                // (`msg.deinit()` — `2f7fbadb0`, freeing discarded messages —
                // is Drop in Rustty: the moved `msg` (incl. a `write_alloc`
                // payload, `957ed21d5`) drops on this return.)
                termio::Message::WriteSmall(_)
                | termio::Message::WriteStable(_)
                | termio::Message::WriteAlloc(_) => return,

                // else => {},
                _ => {}
            }
        }

        // self.io.queueMessage(msg, mutex);
        //
        // (N444: raw-self, through the io block's raw root: no `&mut Termio`
        // while the io threads run.)
        unsafe { termio::Termio::queueMessage(self.io.as_ptr(), msg, mutex) };
    }

    /// Forces the surface to render. This is useful for when the surface
    /// is in the middle of animation (such as a resize, etc.) or when
    /// the render timer is managed manually by the apprt.
    // pub fn draw(self: *Surface) !void {
    //
    // (the inferred `!void` is `drawFrame`'s error — the placeholder
    // `RendererError`, Phase-3 R-d.)
    //
    // W5 (`W5_DESIGN §2.5` caller 2 / §3.5): RAW-self, per the Zig `self: *Surface`
    // receiver. An `&mut self` receiver would protect `self.renderer` (embedded by
    // value) against the concurrent render thread's `renderer.0` writes (the render
    // thread aliases the same Surface allocation). Raw-self finishes the raw draw
    // chain so no protected reference over the `Renderer` bytes spans this present.
    //
    /// # Safety
    /// `self_` is the stable Surface allocation (init's stability contract); no
    /// protected `&mut Surface` may span the call.
    pub unsafe fn draw(self_: *mut Surface<A>) -> Result<(), crate::renderer_seam::RendererError> {
        // Renderers are required to support `drawFrame` being called from
        // the main thread, so that they can update contents during resize.
        // try self.renderer.drawFrame(true);
        unsafe { crate::renderer_seam::Renderer::drawFrame((*self_).renderer.as_ptr(), true)? };
        Ok(())
    }

    /// Activate the inspector. This will begin collecting inspection data.
    /// This will not affect the GUI. The GUI must use performAction to
    /// show/hide the inspector UI.
    //
    // DEFERRED (CORE_BLUEPRINT §4 — the ONLY verbatim-comment deferral family
    // in core, with deactivateInspector below; 0 tests, R1-safe): the body
    // `alloc.create`s and `init`s the UNINHABITED Phase-3
    // `inspectorpkg::Inspector` placeholder (unconstructible until the ImGui
    // inspector tier lands). Preserved verbatim:
    //
    // pub fn activateInspector(self: *Surface) !void {
    //     if (self.inspector != null) return;
    //
    //     // Setup the inspector
    //     const ptr = try self.alloc.create(inspectorpkg.Inspector);
    //     errdefer self.alloc.destroy(ptr);
    //     ptr.* = try inspectorpkg.Inspector.init(self.alloc);
    //     errdefer ptr.deinit(self.alloc);
    //     self.inspector = ptr;
    //     errdefer self.inspector = null;
    //
    //     // Put the inspector onto the render state
    //     {
    //         self.renderer_state.mutex.lock();
    //         defer self.renderer_state.mutex.unlock();
    //         assert(self.renderer_state.inspector == null);
    //         self.renderer_state.inspector = self.inspector;
    //     }
    //
    //     // Notify our components we have an inspector active
    //     _ = self.renderer_thread.mailbox.push(.{ .inspector = true }, .{ .forever = {} });
    //     self.queueIo(.{ .inspector = true }, .unlocked);
    // }
    pub fn activateInspector(&mut self) -> Result<(), Error> {
        // if (self.inspector != null) return;
        if self.inspector.is_some() {
            return Ok(());
        }

        // Setup the inspector
        // const ptr = try self.alloc.create(inspectorpkg.Inspector);
        // errdefer self.alloc.destroy(ptr);
        // ptr.* = try inspectorpkg.Inspector.init(self.alloc);
        // errdefer ptr.deinit(self.alloc);
        // self.inspector = ptr;
        // errdefer self.inspector = null;
        //
        // (`alloc.create` + `init` -> infallible `Box::into_raw(Box::new(...))`
        // under the global allocator — the mutex-create precedent at `init`. The
        // errdefer destroy/deinit/null chains collapse: the allocation and
        // `Inspector::init` are both infallible (a real OOM aborts — panic=abort
        // parity), so there is no fallible step after the field is set.)
        let ptr: *mut inspectorpkg::Inspector =
            Box::into_raw(Box::new(inspectorpkg::Inspector::init(self.alloc)));
        self.inspector = Some(ptr);

        // Put the inspector onto the render state
        {
            // self.renderer_state.mutex.lock();
            // defer self.renderer_state.mutex.unlock();
            unsafe { (*(*self.renderer_state.as_ptr()).mutex).lock() };
            // assert(self.renderer_state.inspector == null);
            assert!(unsafe { (*self.renderer_state.as_ptr()).inspector.is_none() });
            // self.renderer_state.inspector = self.inspector;
            //
            // R2 ACYCLIC OVERLAY EDGE (renderer `state.rs` Task-5 PORT NOTE): the
            // renderer field is `Option<*mut dyn InspectorOverlay>`. Core unsize-
            // coerces the inspector's `renderer_info` sub-object (which impls
            // `InspectorOverlay`) into the fat pointer — the `inspector -> renderer`
            // edge, never the reverse. This is the Rust shape of the reference's
            // `= self.inspector`, whose whole-`Inspector` pointer the renderer read
            // through `rendererInfo().overlayFeatures()`. `&mut` is materialized only
            // instantaneously (immediately decayed to raw + stored); core never holds
            // a protected `&mut Inspector` spanning the renderer's raw reads.
            let info_ref: &mut dyn rendererpkg::state::InspectorOverlay =
                unsafe { &mut (*ptr).gui.renderer_info };
            let info_ptr: *mut rendererpkg::state::Inspector = info_ref;
            unsafe { (*self.renderer_state.as_ptr()).inspector = Some(info_ptr) };
            // The SAME `ptr`, but type-erased (`rendererpkg::state::State::
            // inspector_ptr` PORT NOTE): termio's `recordPtyRead` (INV-8)
            // needs the WHOLE `Inspector`, unreachable through the narrow
            // `renderer_info` sub-object fat pointer above.
            assert!(unsafe { (*self.renderer_state.as_ptr()).inspector_ptr.is_none() });
            unsafe { (*self.renderer_state.as_ptr()).inspector_ptr = Some(ptr as *mut ()) };
            unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
        }

        // Notify our components we have an inspector active
        // _ = self.renderer_thread.mailbox.push(.{ .inspector = true }, .{ .forever = {} });
        let _ = unsafe { &*(*self.renderer_thread.as_ptr()).mailbox }
            .push(rendererpkg::Message::Inspector(true), Timeout::Forever);
        // self.queueIo(.{ .inspector = true }, .unlocked);
        self.queueIo(
            termio::Message::Inspector(true),
            termio::termio_impl::MutexState::Unlocked,
        );

        Ok(())
    }

    /// Report to the renderer how well the apprt can present the frames
    /// the renderer exports. See `renderer.Health` and the renderer's
    /// `presentation_health` state for the semantics.
    //
    // pub fn reportPresentationHealth(self: *Surface, health: rendererpkg.Health) void {
    //     _ = self.renderer_thread.mailbox.push(
    //         global.io(),
    //         .{ .presentation_health = health },
    //         .forever,
    //     );
    //     self.renderer_thread.wakeup.notify() catch |err|
    //         log.warn("failed to wake up renderer err={}", .{err});
    // }   (`2f0b65346`, Surface.zig:924-935)
    //
    // Raw-self (the `draw` precedent): the GTK `RenderSurface`'s snapshot handler
    // calls this on the main thread while the render thread runs, and it touches
    // only the mailbox and the wakeup handle.
    ///
    /// # Safety
    /// `self_` is the stable Surface allocation; no protected `&mut Surface` may
    /// span the call.
    pub unsafe fn reportPresentationHealth(self_: *mut Surface<A>, health: rendererpkg::Health) {
        let _ = unsafe { &*(*(*self_).renderer_thread.as_ptr()).mailbox }.push(
            rendererpkg::Message::PresentationHealth(health),
            Timeout::Forever,
        );
        if let Err(err) = unsafe { (*(*self_).renderer_thread.as_ptr()).wakeup.notify() } {
            // log.warn("failed to wake up renderer err={}", .{err});
            log::warn!(target: LOG_SCOPE, "failed to wake up renderer err={err:?}");
        }
    }

    /// Deactivate the inspector and stop collecting any information.
    //
    // DEFERRED (CORE_BLUEPRINT §4, with activateInspector above; 0 tests,
    // R1-safe): the body tears down the UNINHABITED Phase-3
    // `inspectorpkg::Inspector` placeholder (`insp` can never exist until the
    // Phase-3 fill). Preserved verbatim:
    //
    // pub fn deactivateInspector(self: *Surface) void {
    //     const insp = self.inspector orelse return;
    //
    //     // Remove the inspector from the render state
    //     {
    //         self.renderer_state.mutex.lock();
    //         defer self.renderer_state.mutex.unlock();
    //         assert(self.renderer_state.inspector != null);
    //         self.renderer_state.inspector = null;
    //     }
    //
    //     // Notify our components we have deactivated inspector
    //     _ = self.renderer_thread.mailbox.push(.{ .inspector = false }, .{ .forever = {} });
    //     self.queueIo(.{ .inspector = false }, .unlocked);
    //
    //     // Deinit the inspector
    //     insp.deinit(self.alloc);
    //     self.alloc.destroy(insp);
    //     self.inspector = null;
    // }
    pub fn deactivateInspector(&mut self) {
        // const insp = self.inspector orelse return;
        let insp = match self.inspector {
            Some(v) => v,
            None => return,
        };

        // Remove the inspector from the render state
        {
            // self.renderer_state.mutex.lock();
            // defer self.renderer_state.mutex.unlock();
            unsafe { (*(*self.renderer_state.as_ptr()).mutex).lock() };
            // assert(self.renderer_state.inspector != null);
            assert!(unsafe { (*self.renderer_state.as_ptr()).inspector.is_some() });
            // self.renderer_state.inspector = null;
            unsafe { (*self.renderer_state.as_ptr()).inspector = None };
            // Clear the type-erased alias in lock-step (see activateInspector's
            // `inspector_ptr` PORT NOTE).
            assert!(unsafe { (*self.renderer_state.as_ptr()).inspector_ptr.is_some() });
            unsafe { (*self.renderer_state.as_ptr()).inspector_ptr = None };
            unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
        }

        // Notify our components we have deactivated inspector
        // _ = self.renderer_thread.mailbox.push(.{ .inspector = false }, .{ .forever = {} });
        let _ = unsafe { &*(*self.renderer_thread.as_ptr()).mailbox }
            .push(rendererpkg::Message::Inspector(false), Timeout::Forever);
        // self.queueIo(.{ .inspector = false }, .unlocked);
        self.queueIo(
            termio::Message::Inspector(false),
            termio::termio_impl::MutexState::Unlocked,
        );

        // Deinit the inspector
        // insp.deinit(self.alloc);
        // self.alloc.destroy(insp);
        //
        // (`insp.deinit` runs first for the reference call-shape; `alloc.destroy`
        // -> `Box::from_raw` drop frees the heap allocation — the streams/map free
        // on that Box drop.)
        unsafe { (*insp).deinit(self.alloc) };
        unsafe { drop(Box::from_raw(insp)) };
        // self.inspector = null;
        self.inspector = None;
    }

    /// True if the surface requires confirmation to quit. This should be called
    /// by apprt to determine if the surface should confirm before quitting.
    // pub fn needsConfirmQuit(self: *Surface) bool {
    //
    // PORT NOTE: `&mut self` — the reference receiver is non-const and the
    // `.true` arm locks the renderer mutex and queries the terminal
    // (`cursorIsAtPrompt` takes a mutable terminal).
    pub fn needsConfirmQuit(&mut self) -> bool {
        // If the surface is in read-only mode, always require confirmation
        // if (self.readonly) return true;
        if self.readonly {
            return true;
        }

        // If the child has exited, then our process is certainly not alive.
        // We check this first to avoid the locking overhead below.
        // if (self.child_exited) return false;
        if self.child_exited {
            return false;
        }

        // Check the configuration for confirming close behavior.
        // return switch (self.config.confirm_close_surface) {
        match self.config.confirm_close_surface {
            // .always => true,
            configpkg::value_types::ConfirmCloseSurface::Always => true,
            // .false => false,
            configpkg::value_types::ConfirmCloseSurface::False => false,
            // .true => true: {
            configpkg::value_types::ConfirmCloseSurface::True => {
                // self.renderer_state.mutex.lock();
                // defer self.renderer_state.mutex.unlock();
                //
                // (guard-less zigstd Mutex — CORE_BLUEPRINT §3; the defer is
                // the explicit unlock after the query.)
                unsafe { (*(*self.renderer_state.as_ptr()).mutex).lock() };
                // break :true !self.io.terminal.cursorIsAtPrompt();
                let result = !unsafe { (*self.io.as_ptr()).terminal.cursor_is_at_prompt() };
                unsafe { (*(*self.renderer_state.as_ptr()).mutex).unlock() };
                result
            }
        }
    }
}
