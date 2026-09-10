//! App is the primary GUI application for Rustty. This builds the window,
//! sets up the renderer, etc. The primary run loop is started by calling
//! the "run" function.
//!
//! PORTS: ghostty src/App.zig (chunk C4 of CORE_BLUEPRINT §6 — the full file:
//! `App<A>`, `Message<A>`/`NewWindow<A>`, `Queue<A>`, `Mailbox<A>`,
//! create..performAllAction, the Wasm block) + the rustty-only
//! `#[cfg(test)] mod test_runtime` seam tests (§7 — documented test glue,
//! NOT a port).
//!
//! PORT STATUS: done (every line/comment, R1; App.zig has 0 `test {}` blocks
//! — the tests below are rustty-only extras, `core_app` MAP row).

// PARITY: the reference's identifiers are camelCase (`updateConfig`,
// `drainMailbox`, `performAllChainedAction`, ...); preserved verbatim per
// R1/R2 (renderer_seam.rs / termio thread.rs precedent).
#![allow(non_snake_case)]
// PARITY: the reference passes raw `*Surface` / `*apprt.Surface` across the
// app-thread API (mailbox payloads, add/delete/focus). The validity contract
// is the reference's own ("you must always call hasSurface to validate it";
// see the `Send` note at [`Message`]), so the public fns deref raw-pointer
// args exactly as the Zig does — silenced at module scope (vt page.rs
// precedent).
#![allow(clippy::not_unsafe_ptr_arg_deref)]

// const App = @This();
//
// const std = @import("std");
// const builtin = @import("builtin");
// const assert = @import("quirks.zig").inlineAssert;
// PORT NOTE: `assert` -> `debug_assert!` (the established quirks.inlineAssert
// mapping, binding.rs precedent).
// const Allocator = std.mem.Allocator;
use allocator_api2::alloc::Global as Allocator;
// const apprt = @import("apprt.zig");
use rustty_apprt_api as apprt;
// PORT NOTE (C0 DECISION): the apprt `Surface` trait must be in scope for the
// typed `core()`/`rt_app()` method edges; aliased because core's own struct
// below is also named `Surface`.
use rustty_apprt_api::action::{ConfigChange, QuitTimer, ReloadConfig};
use rustty_apprt_api::Surface as ApprtSurface;
// const Surface = @import("Surface.zig");
use crate::surface::Surface;
// const input = @import("input.zig");
use rustty_input::binding::{Action as BindingAction, GenericLeaf, Scope, ScopedApp, Value};
use rustty_vt::input::key;
use rustty_vt::input::key::KeyEvent;
// const configpkg = @import("config.zig");
// const Config = configpkg.Config;
use rustty_config::conditional;
use rustty_config::Config;
// const BlockingQueue = @import("datastruct/main.zig").BlockingQueue;
use rustty_datastruct::blocking_queue::{Size, Timeout};
use rustty_datastruct::BlockingQueue;
// const renderer = @import("renderer.zig");
// PORT NOTE: used by `create`'s warmup probe (`renderer.Renderer.API.warmup`,
// the P4 warmup chain) — referenced by full path there (`rustty_renderer::`).
// const font = @import("font/main.zig");
use rustty_font_shaper::SharedGridSet;

// const log = std.log.scoped(.app);
// PORT NOTE: the reference `log.debug/info/warn` sites are preserved verbatim
// as comments at each site, with the LIVE emission added directly beneath each
// one (task 14d-1, `.port/parity-closure/DECISION-logging.md` §6 call shape 2 —
// `rustty-core` is below `rustty-vt-c` in R4's layering, so restored sites go
// through the `log` facade and reach whichever backend the running binary
// installed: `rustty-core::logging::AppLogger` for the app/libghostty
// artifacts, `rustty-vt-c::sys::Logger` for the standalone library).
// `LOG_SCOPE` is the `@tagName` of the reference's `std.log.scoped(.app)`.
const LOG_SCOPE: &str = "app";

// const SurfaceList = std.ArrayListUnmanaged(*apprt.Surface);
//
// PORT NOTE: generic over the backend because `*apprt.Surface` is the
// comptime-selected runtime surface pointer (`*mut A::Surface`).
type SurfaceList<A> = Vec<*mut <A as apprt::App>::Surface>;

/// PORT NOTE (GROWTH POINT, Rust-only glue): the reference App methods return
/// `!void` — error unions whose error SETS are *inferred*, merging the apprt
/// `performAction` error (the opaque [`apprt::ApprtError`]) with the core
/// Surface method error sets ([`crate::surface::Error`], itself a growth
/// marker). Typed as the union of the two sources so Zig `try` maps to `?`;
/// the variants gain the real error sets as the Surface chunks (C5+) land.
//
// (C8 dropped the C4 `Clone`/`Copy`/`Eq` derives with the surface `Error`
// growth — its new `FontGrid` payload is Debug-only, the InitError
// precedent.)
#[derive(Debug)]
pub enum Error {
    Apprt(apprt::ApprtError),
    Surface(crate::surface::Error),
}

impl From<apprt::ApprtError> for Error {
    fn from(err: apprt::ApprtError) -> Error {
        Error::Apprt(err)
    }
}

impl From<crate::surface::Error> for Error {
    fn from(err: crate::surface::Error) -> Error {
        Error::Surface(err)
    }
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::Apprt(err) => err.fmt(f),
            Error::Surface(err) => err.fmt(f),
        }
    }
}

impl std::error::Error for Error {}

/// App is the primary GUI application for Rustty (see the module doc; the
/// reference file IS this struct via `const App = @This()`).
pub struct App<A: apprt::App + 'static> {
    /// General purpose allocator
    pub alloc: Allocator,

    /// The list of surfaces that are currently active.
    pub surfaces: SurfaceList<A>,

    /// This is true if the app that Rustty is in is focused. This may
    /// mean that no surfaces (terminals) are focused but the app is still
    /// focused, i.e. may an about window. On macOS, this concept is known
    /// as the "active" app while focused windows are known as the
    /// "main" window.
    ///
    /// This is used to determine if keyboard shortcuts that are non-global
    /// should be processed. If the app is not focused, then we don't want
    /// to process keyboard shortcuts that are not global.
    ///
    /// This defaults to true since we assume that the app is focused when
    /// Rustty is initialized but a well behaved apprt should call
    /// focusEvent to set this to the correct value right away.
    // (reference field default: `= true`)
    pub focused: bool,

    /// The last focused surface. This surface may not be valid;
    /// you must always call hasSurface to validate it.
    // (reference field default: `= null`)
    pub focused_surface: Option<*mut Surface<A>>,

    /// The mailbox that can be used to send this thread messages. Note
    /// this is a blocking queue so if it is full you will get errors (or block).
    //
    // PORT NOTE: `mailbox: Mailbox.Queue` — the queue BY VALUE (the
    // [`Mailbox`] handle other threads hold points at it).
    pub mailbox: Queue<A>,

    /// The set of font GroupCache instances shared by surfaces with the
    /// same font configuration.
    pub font_grid_set: SharedGridSet,

    // Used to rate limit desktop notifications. Some platforms (notably macOS) will
    // run out of resources if desktop notifications are sent too fast and the OS
    // will kill Rustty.
    // (reference field defaults: `= null`, `= 0`)
    pub last_notification_time: Option<std::time::Instant>,
    pub last_notification_digest: u64,

    /// The conditional state of the configuration. See the equivalent field
    /// in the Surface struct for more information. In this case, this applies
    /// to the app-level config and as a default for new surfaces.
    pub config_conditional_state: conditional::State,

    /// Set to false once we've created at least one surface. This
    /// never goes true again. This can be used by surfaces to determine
    /// if they are the first surface.
    // (reference field default: `= true`)
    pub first: bool,
}

// pub const CreateError = Allocator.Error || font.SharedGridSet.InitError;
//
// PORT NOTE: `Allocator.Error` collapses (infallible Rust allocation — the
// termio/renderer_seam convention), leaving the `SharedGridSet` init error
// set.
pub type CreateError = rustty_font_shaper::shared_grid_set::InitError;

impl<A> App<A>
where
    A: apprt::App + 'static,
    // C0 DECISION (CORE_BLUEPRINT §1, option 1): the self-referential
    // associated-type bound giving core the TYPED `rt_surface.core()` /
    // `rt_surface.rtApp()` edges (App.zig sites 140/193/231/285/477/502/511
    // [ADV-FIX 4]).
    A::Surface: ApprtSurface<App = A, Core = Surface<A>>,
{
    /// Create a new app instance. This returns a stable pointer to the app
    /// instance which is required for callbacks.
    //
    // PORT NOTE: `CreateError!*App`; `alloc.create(App)` + `errdefer
    // alloc.destroy(app)` -> `Box` (the heap pointer is stable; the errdefer
    // free is Box drop on the error path).
    pub fn create(alloc: Allocator) -> Result<Box<App<A>>, CreateError> {
        // var app = try alloc.create(App);
        // errdefer alloc.destroy(app);
        // try app.init(alloc);
        let app = Box::new(App::init(alloc)?);

        // If font discovery supports warmup, then we call it. Some font
        // mechanisms (e.g. CoreText) have a multi-millisecond one-time cost
        // on startup.
        // if (comptime @hasDecl(font.Discover, "warmup")) {
        //     if (std.Thread.spawn(
        //         .{},
        //         font.Discover.warmup,
        //         .{},
        //     )) |thr| thr.detach() else |err| {
        //         log.warn("font warmup thread spawn failed err={}", .{err});
        //     }
        // }
        //
        // PORT NOTE (`c454a3bf4`..`931a538a3` warmup chain, END STATE — the
        // intermediate `threadedWarmup` shape was dissolved and is NOT
        // ported): the `@hasDecl` probe is a comptime backend fold — of the
        // monomorphic backends only CoreText declares `warmup`
        // (discovery.zig; fontconfig does not), and `font.Discover` IS
        // CoreText exactly on macOS (`discovery::Discover`), so the probe
        // folds to `cfg(target_os = "macos")` (the renderer_seam `@hasDecl`
        // two-case-fold precedent). Detach == dropping the `JoinHandle`.
        #[cfg(target_os = "macos")]
        {
            if let Err(err) = std::thread::Builder::new()
                .spawn(rustty_font_shaper::discovery::Discover::warmup)
                .map(drop)
            {
                // log.warn("font warmup thread spawn failed err={}", .{err});
                log::warn!(target: LOG_SCOPE, "font warmup thread spawn failed err={err:?}");
            }
        }

        // Same for the renderer's graphics API (e.g. Metal), which pays
        // one-time framework initialization costs on first use.
        // if (comptime @hasDecl(renderer.Renderer.API, "warmup")) {
        //     if (std.Thread.spawn(
        //         .{},
        //         renderer.Renderer.API.warmup,
        //         .{},
        //     )) |thr| thr.detach() else |err| {
        //         log.warn("renderer warmup thread spawn failed err={}", .{err});
        //     }
        // }
        //
        // PORT NOTE: same `@hasDecl` fold — `renderer.Renderer.API` is the
        // comptime graphics API (`Metal` on macOS, which declares `warmup`;
        // OpenGL/WebGL do not), so the probe folds to the same macOS cfg.
        #[cfg(target_os = "macos")]
        {
            if let Err(err) = std::thread::Builder::new()
                .spawn(rustty_renderer::metal::Metal::warmup)
                .map(drop)
            {
                // log.warn("renderer warmup thread spawn failed err={}", .{err});
                log::warn!(target: LOG_SCOPE, "renderer warmup thread spawn failed err={err:?}");
            }
        }

        Ok(app)
    }

    /// Initialize the main app instance. This creates the main window, sets
    /// up the renderer state, compiles the shaders, etc. This is the primary
    /// "startup" logic.
    ///
    /// After calling this function, well behaved apprts should then call
    /// `focusEvent` to set the initial focus state of the app.
    //
    // PORT NOTE: the reference is placement-init (`init(self: *App, alloc)`
    // writing `self.* = .{...}`); Rust has no placement-init, so `init`
    // returns the value and [`App::create`] is the stable-pointer path.
    pub fn init(alloc: Allocator) -> Result<App<A>, CreateError> {
        // var font_grid_set = try font.SharedGridSet.init(alloc);
        let font_grid_set = SharedGridSet::init(alloc)?;
        // errdefer font_grid_set.deinit();
        // PORT NOTE: nothing below can fail — ownership drops cover the
        // reference errdefer.

        // self.* = .{ ... } — the omitted fields take their declaration
        // defaults (focused=true, focused_surface=null,
        // last_notification_time=null, last_notification_digest=0,
        // first=true).
        Ok(App {
            alloc,
            surfaces: Vec::new(),
            focused: true,
            focused_surface: None,
            mailbox: Queue::<A>::default(),
            font_grid_set,
            last_notification_time: None,
            last_notification_digest: 0,
            config_conditional_state: conditional::State::default(),
            first: true,
        })
    }

    pub fn deinit(&mut self) {
        // Clean up all our surfaces
        // for (self.surfaces.items) |surface| surface.deinit();
        //
        // PORT NOTE: index loop, not a slice borrow — the backend `deinit`
        // (embedded.zig:597) re-enters `deleteSurface`, which mutates this
        // very list; a held `&` borrow across that call would be UB where the
        // Zig slice iteration is merely hazardous.
        let mut i: usize = 0;
        while i < self.surfaces.len() {
            let surface = self.surfaces[i];
            unsafe { (*surface).deinit() };
            i += 1;
        }
        // self.surfaces.deinit(self.alloc);
        // PORT NOTE: `Vec` ownership frees the list storage when `App` drops.

        // Clean up our font group cache
        // We should have zero items in the grid set at this point because
        // destroy only gets called when the app is shutting down and this
        // should gracefully close all surfaces.
        debug_assert!(self.font_grid_set.count() == 0);
        self.font_grid_set.deinit();
    }

    pub fn destroy(mut self: Box<Self>) {
        // Deinitialize the app
        self.deinit();

        // Free the app memory
        // self.alloc.destroy(self);
        drop(self);
    }

    /// Tick ticks the app loop. This will drain our mailbox and process those
    /// events. This should be called by the application runtime on every loop
    /// tick.
    //
    // PORT NOTE: `rt_app: *apprt.App` -> `&A` on the app-thread entry points
    // (the whole trait surface is `&self`); the cross-thread [`Mailbox`]
    // handle keeps the raw `*mut A`.
    pub fn tick(&mut self, rt_app: &A) -> Result<(), Error> {
        // Drain our mailbox
        self.drainMailbox(rt_app)
    }

    /// Update the configuration associated with the app. This can only be
    /// called from the main thread. The caller owns the config memory. The
    /// memory can be freed immediately when this returns.
    pub fn updateConfig(&mut self, rt_app: &A, config: &Config) -> Result<(), Error> {
        // Go through and update all of the surface configurations.
        for &surface in self.surfaces.iter() {
            // try surface.core().handleMessage(.{ .change_config = config });
            let core = unsafe { &*surface }.core();
            unsafe { &mut *core }.handleMessage(apprt::surface::Message::ChangeConfig(config))?;
        }

        // Apply our conditional state. If we fail to apply the conditional state
        // then we log and attempt to move forward with the old config.
        // We only apply this to the app-level config because the surface
        // config applies its own conditional state.
        // PARITY: not `.unwrap_or_default()` — the arm IS the reference's
        // labeled catch block (`catch |err| err: {...}`) with its log site.
        #[allow(clippy::manual_unwrap_or_default)]
        let mut applied_: Option<Config> =
            match config.change_conditional_state(self.config_conditional_state) {
                Ok(applied) => applied,
                // catch |err| err: { ... break :err null; }
                Err(err) => {
                    // log.warn("failed to apply conditional state to config err={}", .{err});
                    log::warn!(
                        target: LOG_SCOPE,
                        "failed to apply conditional state to config err={err:?}"
                    );
                    None
                }
            };
        // defer if (applied_) |*c| c.deinit();
        // PORT NOTE: on the early-`?` path the defer maps to `applied_`'s
        // Drop; the success-path deinit is explicit below.
        // const applied: *const configpkg.Config = if (applied_) |*c| c else config;
        let applied: &Config = match applied_.as_ref() {
            Some(c) => c,
            None => config,
        };

        // Notify the apprt that the app has changed configuration.
        let _ = rt_app.perform_action(
            apprt::Target::<Surface<A>>::App,
            apprt::Action::ConfigChange(ConfigChange { config: applied }),
        )?;

        // (the reference defer, on the success path)
        if let Some(c) = applied_.as_mut() {
            c.deinit();
        }

        Ok(())
    }

    /// Add an initialized surface. This is really only for the runtime
    /// implementations to call and should NOT be called by general app users.
    /// The surface must be from the pool.
    //
    // PORT NOTE: `Allocator.Error!void` -> infallible (Rust `Vec` allocation;
    // the CreateError convention above).
    pub fn addSurface(&mut self, rt_surface: *mut A::Surface) {
        // try self.surfaces.append(self.alloc, rt_surface);
        self.surfaces.push(rt_surface);

        // Since we have non-zero surfaces, we can cancel the quit timer.
        // It is up to the apprt if there is a quit timer at all and if it
        // should be canceled.
        //
        // _ = rt_surface.rtApp().performAction(.app, .quit_timer, .stop) catch ...
        let rt_app = unsafe { &*rt_surface }.rt_app();
        if let Err(err) = unsafe { &*rt_app }.perform_action(
            apprt::Target::<Surface<A>>::App,
            apprt::Action::QuitTimer(QuitTimer::Stop),
        ) {
            // log.warn("error stopping quit timer err={}", .{err});
            log::warn!(target: LOG_SCOPE, "error stopping quit timer err={err:?}");
        }
    }

    /// Delete the surface from the known surface list. This will NOT call the
    /// destructor or free the memory.
    pub fn deleteSurface(&mut self, rt_surface: *mut A::Surface) {
        // If this surface is the focused surface then we need to clear it.
        // There was a bug where we relied on hasSurface to return false and
        // just let focused surface be but the allocator was reusing addresses
        // after free and giving false positives, so we must clear it.
        if let Some(focused) = self.focused_surface {
            if focused == unsafe { &*rt_surface }.core() {
                self.focused_surface = None;
            }
        }

        let mut i: usize = 0;
        while i < self.surfaces.len() {
            if self.surfaces[i] == rt_surface {
                let _ = self.surfaces.swap_remove(i);
                continue;
            }

            i += 1;
        }

        // If we have no surfaces, we can start the quit timer. It is up to the
        // apprt to determine if this is necessary.
        //
        // if (self.surfaces.items.len == 0) _ = rt_surface.rtApp().performAction(
        //     .app, .quit_timer, .start) catch ...
        if self.surfaces.is_empty() {
            let rt_app = unsafe { &*rt_surface }.rt_app();
            if let Err(err) = unsafe { &*rt_app }.perform_action(
                apprt::Target::<Surface<A>>::App,
                apprt::Action::QuitTimer(QuitTimer::Start),
            ) {
                // log.warn("error starting quit timer err={}", .{err});
                log::warn!(target: LOG_SCOPE, "error starting quit timer err={err:?}");
            }
        }
    }

    /// The last focused surface. This is only valid while on the main thread
    /// before tick is called.
    pub fn focusedSurface(&self) -> Option<*mut Surface<A>> {
        // const surface = self.focused_surface orelse return null;
        let surface = self.focused_surface?;
        if !self.hasSurface(surface.cast_const()) {
            return None;
        }
        Some(surface)
    }

    /// Returns true if confirmation is needed to quit the app. It is up to
    /// the apprt to call this.
    pub fn needsConfirmQuit(&self) -> bool {
        for &v in self.surfaces.iter() {
            // if (v.core().needsConfirmQuit()) return true;
            //
            // PORT NOTE (C6): `&mut` — the reference receiver is the non-const
            // `self: *Surface` and the body locks the renderer mutex and
            // queries the terminal (`cursorIsAtPrompt` mutates prompt-scan
            // state), so the core method takes `&mut self`.
            let core = unsafe { &*v }.core();
            if unsafe { &mut *core }.needsConfirmQuit() {
                return true;
            }
        }

        false
    }

    /// Drain the mailbox.
    fn drainMailbox(&mut self, rt_app: &A) -> Result<(), Error> {
        while let Some(message) = self.mailbox.pop() {
            // if (comptime std.log.logEnabled(.debug, .app)) {
            //     switch (message) {
            //         // these tend to be way too verbose for normal debugging
            //         .redraw_surface => {},
            //         else => log.debug("mailbox message={t}", .{message}),
            //     }
            // }
            //
            // PORT NOTE (task 14d-1): `std.log.logEnabled(.debug, .app)` is a
            // COMPTIME query of the build's `std_options.log_level` in the
            // reference; `log`'s nearest analog is the runtime
            // `log_enabled!`, which reads the same max-level the backend
            // installed (`install_app_log_backend`: `Debug` on a debug build,
            // `Info` otherwise — the direct port of `main_ghostty.zig`'s
            // `.Debug => .debug, else => .info`). The gate exists to avoid
            // paying for `@tagName`/formatting when debug logging is off, and
            // `log_enabled!` preserves exactly that property.
            if log::log_enabled!(target: LOG_SCOPE, log::Level::Debug) {
                match &message {
                    // these tend to be way too verbose for normal debugging
                    Message::RedrawSurface(_) => {}
                    _ => log::debug!(
                        target: LOG_SCOPE,
                        "mailbox message={}",
                        message.tag_name()
                    ),
                }
            }
            match message {
                // .open_config => |v| try self.performAction(
                //     rt_app,
                //     .{
                //         .open_config = switch (v) {
                //             .os_open => .os_open,
                //             .new_window => .new_window,
                //         },
                //     },
                // ),
                //
                // (`0a183c923`: the `App.Message.OpenConfig` payload maps
                // arm-for-arm onto the binding `OpenConfig` enum.)
                Message::OpenConfig(v) => self.performAction(
                    rt_app,
                    ScopedApp::OpenConfig(match v {
                        OpenConfig::OsOpen => rustty_input::binding::OpenConfig::OsOpen,
                        OpenConfig::NewWindow => rustty_input::binding::OpenConfig::NewWindow,
                    }),
                )?,
                Message::NewWindow(msg) => self.newWindow(rt_app, msg)?,
                Message::Close(surface) => self.closeSurface(surface),
                Message::SurfaceMessage(msg) => self.surfaceMessage(msg.surface, msg.message)?,
                Message::RedrawSurface(surface) => self.redrawSurface(rt_app, surface)?,

                // If we're quitting, then we set the quit flag and stop
                // draining the mailbox immediately. This lets us defer
                // mailbox processing to the next tick so that the apprt
                // can try to quit as quickly as possible.
                Message::Quit => {
                    // log.info("quit message received, short circuiting mailbox drain", .{});
                    log::info!(
                        target: LOG_SCOPE,
                        "quit message received, short circuiting mailbox drain"
                    );
                    self.performAction(rt_app, ScopedApp::Quit)?;
                    return Ok(());
                }
            }
        }
        Ok(())
    }

    pub fn closeSurface(&mut self, surface: *mut Surface<A>) {
        if !self.hasSurface(surface.cast_const()) {
            return;
        }
        unsafe { &mut *surface }.close();
    }

    pub fn focusSurface(&mut self, surface: *mut Surface<A>) {
        if !self.hasSurface(surface.cast_const()) {
            return;
        }
        self.focused_surface = Some(surface);
    }

    fn redrawSurface(&mut self, rt_app: &A, surface: *mut A::Surface) -> Result<(), Error> {
        if !self.hasRtSurface(surface) {
            return Ok(());
        }

        // _ = try rt_app.performAction(.{ .surface = surface.core() }, .render, {});
        let core = unsafe { &*surface }.core();
        let _ = rt_app.perform_action(
            apprt::Target::Surface(unsafe { &*core }),
            apprt::Action::Render,
        )?;
        Ok(())
    }

    /// Create a new window
    pub fn newWindow(&mut self, rt_app: &A, msg: NewWindow<A>) -> Result<(), Error> {
        // const target: apprt.Target = target: { ... };
        let target: apprt::Target<'_, Surface<A>> = 'target: {
            // const parent = msg.parent orelse break :target .app;
            let parent = match msg.parent {
                Some(parent) => parent,
                None => break 'target apprt::Target::App,
            };
            // if (self.hasSurface(parent)) break :target .{ .surface = parent };
            if self.hasSurface(parent.cast_const()) {
                break 'target apprt::Target::Surface(unsafe { &*parent });
            }
            // break :target .app;
            apprt::Target::App
        };

        // _ = try rt_app.performAction(target, .new_window, {});
        let _ = rt_app.perform_action(target, apprt::Action::NewWindow)?;
        Ok(())
    }

    /// Handle an app-level focus event. This should be called whenever
    /// the focus state of the entire app containing Rustty changes.
    /// This is separate from surface focus events. See the `focused`
    /// field for more information.
    pub fn focusEvent(&mut self, focused: bool) {
        // Prevent redundant focus events
        if self.focused == focused {
            return;
        }

        // log.debug("focus event focused={}", .{focused});
        log::debug!(target: LOG_SCOPE, "focus event focused={focused}");
        self.focused = focused;
    }

    /// Handle a key event at the app-scope. If this key event is used,
    /// this will return true and the caller shouldn't continue processing
    /// the event. If the event is not used, this will return false.
    ///
    /// If the app currently has focus then all key events are processed.
    /// If the app does not have focus then only global key events are
    /// processed.
    pub fn keyEvent(&mut self, rt_app: &A, event: KeyEvent) -> bool {
        match event.action {
            // We don't care about key release events.
            key::Action::Release => return false,

            // Continue processing key press events.
            key::Action::Press | key::Action::Repeat => {}
        }

        // Get the keybind entry for this event. We don't support key sequences
        // so we can look directly in the top-level set.
        //
        // PORT NOTE: the ported `Set.getEvent` returns the entry INDEX;
        // `value_at(entry)` is the `entry.value_ptr.*` read.
        let set = &rt_app.config().keybind.set;
        let entry = match set.get_event(&event) {
            Some(entry) => entry,
            None => return false,
        };
        let leaf: GenericLeaf = match set.value_at(entry) {
            // Sequences aren't supported. Our configuration parser verifies
            // this for global keybinds but we may still get an entry for
            // a non-global keybind.
            Value::Leader(_) => return false,

            // Leaf entries are good
            // (Zig `inline .leaf, .leaf_chained => |leaf| leaf.generic()`)
            Value::Leaf(leaf) => leaf.generic(),
            Value::LeafChained(leaf) => leaf.generic(),
        };
        let actions: &[BindingAction] = leaf.actions_slice();
        debug_assert!(!actions.is_empty());

        // If we aren't focused, then we only process global keybinds.
        if !self.focused && !leaf.flags.global {
            return false;
        }

        // Global keybinds are done using performAll so that they
        // can target all surfaces too.
        if leaf.flags.global {
            self.performAllChainedAction(rt_app, actions);
            return true;
        }

        // Must be focused to process non-global keybinds
        debug_assert!(self.focused);
        debug_assert!(!leaf.flags.global);

        // If we are focused, then we process keybinds only if they are
        // app-scoped. Otherwise, we do nothing. Surface-scoped should
        // be processed by Surface.keyEvent. For chained actions, all
        // actions must be app-scoped.
        for action in actions {
            if action.scoped_app().is_none() {
                return false;
            }
        }
        for action in actions {
            // self.performAction(rt_app, action.scoped(.app).?) catch |err| {...};
            if let Err(err) = self.performAction(rt_app, action.scoped_app().unwrap()) {
                // log.warn("error performing app keybind action action={s} err={}", .{
                //     @tagName(action),
                //     err,
                // });
                log::warn!(
                    target: LOG_SCOPE,
                    "error performing app keybind action action={} err={err:?}",
                    action.tag_name()
                );
            }
        }

        true
    }

    /// Call to notify Rustty that the color scheme for the app has changed.
    /// "Color scheme" in this case refers to system themes such as "light/dark".
    pub fn colorSchemeEvent(
        &mut self,
        rt_app: &A,
        scheme: apprt::ColorScheme,
    ) -> Result<(), Error> {
        let new_scheme: conditional::Theme = match scheme {
            apprt::ColorScheme::Light => conditional::Theme::Light,
            apprt::ColorScheme::Dark => conditional::Theme::Dark,
        };

        // If our scheme didn't change, then we don't do anything.
        if self.config_conditional_state.theme == new_scheme {
            return Ok(());
        }

        // Setup our conditional state which has the current color theme.
        self.config_conditional_state.theme = new_scheme;

        // Request our configuration be reloaded because the new scheme may
        // impact the colors of the app.
        //
        // _ = try rt_app.performAction(.app, .reload_config, .{ .soft = true });
        let _ = rt_app.perform_action(
            apprt::Target::<Surface<A>>::App,
            apprt::Action::ReloadConfig(ReloadConfig { soft: true }),
        )?;

        Ok(())
    }

    /// Perform a binding action. This only accepts actions that are scoped
    /// to the app. Callers can use performAllAction to perform any action
    /// and any non-app-scoped actions will be performed on all surfaces.
    //
    // PORT NOTE: `action: input.Binding.Action.Scoped(.app)` — the
    // hand-written `Scoped(.app)` instantiation (`ScopedApp`, binding.rs).
    pub fn performAction(&mut self, rt_app: &A, action: ScopedApp) -> Result<(), Error> {
        match action {
            ScopedApp::Unbind => unreachable!(),
            ScopedApp::Ignore => {}
            ScopedApp::Quit => {
                let _ =
                    rt_app.perform_action(apprt::Target::<Surface<A>>::App, apprt::Action::Quit)?;
            }
            ScopedApp::NewWindow => {
                // _ = try self.newWindow(rt_app, .{ .parent = null });
                self.newWindow(rt_app, NewWindow { parent: None })?;
            }
            // .open_config => |v| _ = try rt_app.performAction(
            //     .app,
            //     .open_config,
            //     switch (v) {
            //         .os_open => .os_open,
            //         .new_window => .new_window,
            //     },
            // ),
            ScopedApp::OpenConfig(v) => {
                let _ = rt_app.perform_action(
                    apprt::Target::<Surface<A>>::App,
                    apprt::Action::OpenConfig(match v {
                        rustty_input::binding::OpenConfig::OsOpen => {
                            apprt::action::OpenConfig::OsOpen
                        }
                        rustty_input::binding::OpenConfig::NewWindow => {
                            apprt::action::OpenConfig::NewWindow
                        }
                    }),
                )?;
            }
            ScopedApp::ReloadConfig => {
                // _ = try rt_app.performAction(.app, .reload_config, .{});
                let _ = rt_app.perform_action(
                    apprt::Target::<Surface<A>>::App,
                    apprt::Action::ReloadConfig(ReloadConfig::default()),
                )?;
            }
            ScopedApp::CloseAllWindows => {
                let _ = rt_app.perform_action(
                    apprt::Target::<Surface<A>>::App,
                    apprt::Action::CloseAllWindows,
                )?;
            }
            ScopedApp::ToggleQuickTerminal => {
                let _ = rt_app.perform_action(
                    apprt::Target::<Surface<A>>::App,
                    apprt::Action::ToggleQuickTerminal,
                )?;
            }
            ScopedApp::ToggleVisibility => {
                let _ = rt_app.perform_action(
                    apprt::Target::<Surface<A>>::App,
                    apprt::Action::ToggleVisibility,
                )?;
            }
            ScopedApp::CheckForUpdates => {
                let _ = rt_app.perform_action(
                    apprt::Target::<Surface<A>>::App,
                    apprt::Action::CheckForUpdates,
                )?;
            }
            ScopedApp::ShowGtkInspector => {
                let _ = rt_app.perform_action(
                    apprt::Target::<Surface<A>>::App,
                    apprt::Action::ShowGtkInspector,
                )?;
            }
            ScopedApp::Undo => {
                let _ =
                    rt_app.perform_action(apprt::Target::<Surface<A>>::App, apprt::Action::Undo)?;
            }

            ScopedApp::Redo => {
                let _ =
                    rt_app.perform_action(apprt::Target::<Surface<A>>::App, apprt::Action::Redo)?;
            }
        }
        Ok(())
    }

    /// Performs a chained action. We will continue executing each action
    /// even if there is a failure in a prior action.
    pub fn performAllChainedAction(&mut self, rt_app: &A, actions: &[BindingAction]) {
        for action in actions {
            // self.performAllAction(rt_app, action) catch |err| {...};
            // PORT NOTE: Zig copies the action by value per call -> `clone()`.
            if let Err(err) = self.performAllAction(rt_app, action.clone()) {
                // log.warn("error performing chained action action={s} err={}", .{
                //     @tagName(action),
                //     err,
                // });
                log::warn!(
                    target: LOG_SCOPE,
                    "error performing chained action action={} err={err:?}",
                    action.tag_name()
                );
            }
        }
    }

    /// Perform an app-wide binding action. If the action is surface-specific
    /// then it will be performed on all surfaces. To perform only app-scoped
    /// actions, use performAction.
    pub fn performAllAction(&mut self, rt_app: &A, action: BindingAction) -> Result<(), Error> {
        match action.scope() {
            // App-scoped actions are handled by the app so that they aren't
            // repeated for each surface (since each surface forwards
            // app-scoped actions back up).
            Scope::App => self.performAction(
                rt_app,
                action.scoped_app().unwrap(), // asserted through the scope match
            )?,

            // Surface-scoped actions are performed on all surfaces. Errors
            // are logged but processing continues.
            Scope::Surface => {
                for &surface in self.surfaces.iter() {
                    // _ = surface.core().performBindingAction(action) catch |err| {...};
                    let core = unsafe { &*surface }.core();
                    // PORT NOTE (task 14d-1): the surface id is READ BEFORE the
                    // call, not inside the error arm as the reference does.
                    // Raw-pointer discipline (`PROCEDURE.md`): taking `&*core`
                    // inside the `if let` body while the scrutinee's
                    // `&mut *core` is still live is Stacked-Borrows UB here,
                    // and `id` is immutable for the lifetime of the surface, so
                    // the value logged is identical either way.
                    let id = unsafe { &*core }.id;
                    if let Err(err) = unsafe { &mut *core }.performBindingAction(action.clone()) {
                        // log.warn("error performing binding action on surface id={x} err={}", .{
                        //     surface.core().id,
                        //     err,
                        // });
                        log::warn!(
                            target: LOG_SCOPE,
                            "error performing binding action on surface id={id:x} err={err:?}"
                        );
                    }
                }
            }
        }
        Ok(())
    }

    /// Handle a window message
    fn surfaceMessage(
        &mut self,
        surface: *mut Surface<A>,
        msg: apprt::surface::Message<'_>,
    ) -> Result<(), Error> {
        // We want to ensure our window is still active. Window messages
        // are quite rare and we normally don't have many windows so we do
        // a simple linear search here.
        if self.hasSurface(surface.cast_const()) {
            unsafe { &mut *surface }.handleMessage(msg)?;
        }

        // Window was not found, it probably quit before we handled the message.
        // Not a problem.
        Ok(())
    }

    fn hasSurface(&self, surface: *const Surface<A>) -> bool {
        for &v in self.surfaces.iter() {
            // if (v.core() == surface) return true;
            if unsafe { &*v }.core().cast_const() == surface {
                return true;
            }
        }

        false
    }

    /// Search for a surface by a 64 bit unique ID.
    pub fn findSurfaceByID(&self, id: u64) -> Option<*mut Surface<A>> {
        for &v in self.surfaces.iter() {
            // const surface: *Surface = v.core();
            let surface: *mut Surface<A> = unsafe { &*v }.core();
            if unsafe { (*surface).id } == id {
                return Some(surface);
            }
        }

        None
    }

    fn hasRtSurface(&self, surface: *mut A::Surface) -> bool {
        for &v in self.surfaces.iter() {
            if v == surface {
                return true;
            }
        }

        false
    }
}

/// Returns a new config for a surface for the given app that should be
/// used for any new surfaces. The resulting config should be deinitialized
/// after the surface is initialized.
//
// PORT NOTE (core C-T): the reference free function `apprt.surface.newConfig`
// (src/apprt/surface.zig:175) — RESOLVED here per CORE_BLUEPRINT §6 because it
// reads core's `App`/`Surface` (`app.focusedSurface()` + `Surface.pwd()`), so
// it cannot live in the apprt-api interface crate (its verbatim body is
// retained there for provenance). The `Allocator.Error!Config` return collapses
// to `Config` (infallible Rust allocation — the App/DerivedConfig.init precedent);
// `copy._arena.?.allocator()` collapses (the pwd `Vec<u8>` allocates via Global,
// `app.alloc` carried for signature parity).
//
// pub fn newConfig(
//     app: *const App,
//     config: *const Config,
//     context: NewSurfaceContext,
// ) Allocator.Error!Config {
pub fn new_config<A>(
    app: &App<A>,
    config: &Config,
    context: apprt::surface::NewSurfaceContext,
) -> Config
where
    A: apprt::App + 'static,
    // The self-referential C0 bound (§1) that `App::focusedSurface` carries.
    A::Surface: ApprtSurface<App = A, Core = Surface<A>>,
{
    // Create a shallow clone
    // var copy = config.shallowClone(app.alloc);
    let mut copy = config.shallow_clone();

    // Our allocator is our config's arena
    // const alloc = copy._arena.?.allocator();
    // (PORT NOTE: collapses — `pwd` returns an owned `Vec<u8>` via the global
    // allocator.)

    // Get our previously focused surface for some inherited values.
    // const prev = app.focusedSurface();
    let prev = app.focusedSurface();
    // if (prev) |p| {
    if let Some(p) = prev {
        // if (shouldInheritWorkingDirectory(context, config)) {
        if apprt::surface::should_inherit_working_directory(context, config) {
            // if (try p.pwd(alloc)) |pwd| {
            //
            // SAFETY: `focusedSurface` returned a pointer it just validated via
            // `hasSurface`; it is a live core `Surface<A>` for this call.
            if let Some(pwd) = unsafe { &*p }.pwd(app.alloc) {
                // copy.@"working-directory" = .{ .path = pwd };
                copy.working_directory =
                    Some(rustty_config::value_types::WorkingDirectory::Path(pwd));
            }
        }
    }

    // return copy;
    copy
}

/// The message types that can be sent to the app thread.
//
// PARITY: the reference is a Zig `union(enum)` stored BY VALUE in the
// fixed-capacity `BlockingQueue(Message, 64)` slots — every slot is
// max-variant-sized there too. Boxing the large variant would change that
// ownership/layout shape, so `large_enum_variant` is silenced (renderer
// message.rs precedent).
#[allow(clippy::large_enum_variant)]
pub enum Message<A: apprt::App + 'static> {
    // Open the configuration file
    // open_config: OpenConfig,
    OpenConfig(OpenConfig),

    /// Create a new terminal window.
    NewWindow(NewWindow<A>),

    /// Close a surface. This notifies the runtime that a surface
    /// should close.
    Close(*mut Surface<A>),

    /// Quit
    Quit,

    /// A message for a specific surface.
    //
    // PORT NOTE: the reference's anonymous payload `struct { surface:
    // *Surface, message: apprt.surface.Message }` is the apprt-api
    // [`apprt::SurfaceMessageEnvelope`] DTO (fields `surface`/`message`),
    // instantiated at core's surface pointer and the queue-storable
    // `'static` message (the `AppMailbox` note: the one borrowed variant is
    // never queued).
    SurfaceMessage(apprt::SurfaceMessageEnvelope<'static, *mut Surface<A>>),

    /// Redraw a surface. This only has an effect for runtimes that
    /// use single-threaded draws. To redraw a surface for all runtimes,
    /// wake up the renderer thread. The renderer thread will send this
    /// message if it needs to.
    RedrawSurface(*mut A::Surface),
}

impl<A: apprt::App + 'static> Message<A> {
    /// PORT NOTE (task 14d-1): the image of Zig's `{t}` / `@tagName` on this
    /// `union(enum)`, needed by `drainMailbox`'s restored
    /// `log.debug("mailbox message={t}", ...)` site (`App.zig:271`). The
    /// strings are the reference's own field names, not the Rust variant
    /// spellings.
    fn tag_name(&self) -> &'static str {
        match self {
            Message::OpenConfig(_) => "open_config",
            Message::NewWindow(_) => "new_window",
            Message::Close(_) => "close",
            Message::Quit => "quit",
            Message::SurfaceMessage(_) => "surface_message",
            Message::RedrawSurface(_) => "redraw_surface",
        }
    }
}

// Cross-thread queue of raw pointers — the reference threading contract as a
// documented invariant (CORE_BLUEPRINT §1/§8.3): the surface pointers are
// stable for the surface's lifetime (apprt-owned heap allocations), and the
// app thread VALIDATES every received pointer against its known-surface list
// before dereferencing (`hasSurface`/`hasRtSurface` — "We want to ensure our
// window is still active"; "This surface may not be valid; you must always
// call hasSurface to validate it"). No `&`/`&mut` crosses threads here.
unsafe impl<A: apprt::App> Send for Message<A> {}

// const NewWindow = struct { ... };
//
// PORT NOTE: hoisted from the reference's nested `Message.NewWindow` (Rust
// enum variants have no type namespace) — CORE_BLUEPRINT §1 `NewWindow<A>`.
pub struct NewWindow<A: apprt::App + 'static> {
    /// The parent surface
    // (reference field default: `= null`)
    pub parent: Option<*mut Surface<A>>,
}

// PORT NOTE ([ADV-FIX 6] family): MANUAL `Default` — a derive would wrongly
// bound `A: Default`.
impl<A: apprt::App + 'static> Default for NewWindow<A> {
    fn default() -> Self {
        NewWindow { parent: None }
    }
}

// pub const OpenConfig = enum { ... };
//
// PORT NOTE (`0a183c923`): hoisted from the reference's nested
// `Message.OpenConfig` (Rust enum variants have no type namespace — the
// `NewWindow` precedent above).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenConfig {
    /// Open the config in the OS default editor.
    OsOpen,
    /// Open the config in a new window using $EDITOR or $VISUAL
    NewWindow,
}

/// The type used for sending messages to the app thread.
//
// PORT NOTE: reference `pub const Queue = BlockingQueue(Message, 64);` —
// declared INSIDE `Mailbox` in the reference (App.zig:562), hoisted to the
// module per the CORE_BLUEPRINT §1 signature (`pub type Queue<A>`).
// `Queue.Timeout`/`Queue.Size` resolve to the module-level
// `rustty_datastruct::blocking_queue::{Timeout, Size}` (the C0 `AppMailbox`
// concretization).
pub type Queue<A> = BlockingQueue<Message<A>, 64>;

/// Mailbox is the way that other threads send the app thread messages.
//
// PORT NOTE: App.zig:560 `pub const Mailbox = struct { rt_app: *apprt.App,
// mailbox: *Queue, ... }` — raw pointers held across threads (the reference
// threading contract; see the `Send` note at [`Message`]).
pub struct Mailbox<A: apprt::App + 'static> {
    pub rt_app: *mut A,
    pub mailbox: *mut Queue<A>,
}

impl<A: apprt::App + 'static> Mailbox<A> {
    /// Send a message to the surface.
    //
    // PORT NOTE: the reference receiver is by-value `self: Mailbox`
    // (`Mailbox` is a two-pointer `Copy` handle) -> `&self` per the
    // CORE_BLUEPRINT §1 signature.
    pub fn push(&self, msg: Message<A>, timeout: Timeout) -> Size {
        // On a failed push the message is dropped (the reference `push` returns
        // the Size and never hands the value back). Delegate to the
        // retain-on-failure primitive so `SurfaceMailbox` can retry.
        self.push_or_return(msg, timeout).unwrap_or(0)
    }

    /// Like [`push`](Self::push) but returns the message BACK when the queue
    /// push fails, so a caller can retry with a blocking timeout.
    //
    // PORT NOTE (core C-T): the reference `apprt.surface.Mailbox.push` retries
    // via the app queue — `StreamHandler.surfaceMessageWriter` re-pushes its
    // by-value message copy after a failed instant push. Rust `push` moves, so
    // the surface writer needs the message handed back on failure; this
    // primitive threads `BlockingQueue::push_or_return` up through the
    // `AppMailbox` seam. The reference `wakeup()` fires on BOTH the storing and
    // the failed push (App.Mailbox.push always wakes the app loop), preserved
    // here (the wakeup is unconditional, exactly as the reference).
    //
    // The `Err` carries a full `Message<A>` (large); handing it back is the point.
    #[allow(clippy::result_large_err)]
    pub fn push_or_return(&self, msg: Message<A>, timeout: Timeout) -> Result<Size, Message<A>> {
        // const result = self.mailbox.push(msg, timeout);
        let result = unsafe { &*self.mailbox }.push_or_return(msg, timeout);

        // Wake up our app loop
        unsafe { &*self.rt_app }.wakeup();

        result
    }
}

// PORT NOTE ([ADV-FIX 6a]): the reference copies `App.Mailbox` BY VALUE (Zig
// struct assignment — Surface.zig:550 `const app_mailbox: App.Mailbox =
// .{...}` then `.app = app_mailbox` / `rendererpkg.Thread.init(...,
// app_mailbox)`). A `#[derive(Clone, Copy)]` on a `Mailbox<A>` would wrongly
// demand `A: Copy`, so the impls are MANUAL with no bound on `A` (the fields
// are two raw pointers, always bitwise-copyable).
impl<A: apprt::App + 'static> Clone for Mailbox<A> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<A: apprt::App + 'static> Copy for Mailbox<A> {}

// Wasm API.
// PORT: `pub const Wasm = if (!builtin.target.isWasm()) struct {} else struct { … };`
//
// PORT STATUS: ported — COMPLETE, and deliberately EMPTY. On non-wasm targets the
// reference yields `struct {}`; on wasm it yields a struct whose ONLY live members
// are two private aliases (`wasm`, `alloc`) and whose three exports (`app_new`,
// `app_new_`, `app_free`) are COMMENTED OUT IN THE REFERENCE. Retaining it emits
// zero symbols and pulls no App implementation into the wasm module — which is why
// `crates/rustty-wasm` does NOT need a `rustty-core` dependency edge to be a
// faithful `main_wasm.zig` (WS14 T7); a `rustty-core` edge would over-retain an
// entire application tier and drag rustty-inspector → dcimgui-sys → freetype-sys →
// zlib-sys into a freestanding wasm link. The reference's commented-out bodies are
// preserved verbatim below so a future un-comment is a diff, not archaeology.
#[cfg(target_arch = "wasm32")]
pub mod wasm {
    //     const wasm = @import("os/wasm.zig");
    //     const alloc = wasm.alloc;
    #[allow(unused_imports)]
    use rustty_os::wasm as _wasm;

    // (The three reference exports, commented out IN THE REFERENCE:)
    //
    //     // export fn app_new(config: *Config) ?*App {
    //     //     return app_new_(config) catch |err| { log.err("error initializing app err={}", .{err});
    //     //         return null;
    //     //     };
    //     // }
    //     //
    //     // fn app_new_(config: *Config) !*App {
    //     //     const app = try App.create(alloc, config);
    //     //     errdefer app.destroy();
    //     //
    //     //     const result = try alloc.create(App);
    //     //     result.* = app;
    //     //     return result;
    //     // }
    //     //
    //     // export fn app_free(ptr: ?*App) void {
    //     //     if (ptr) |v| {
    //     //         v.destroy();
    //     //         alloc.destroy(v);
    //     //     }
    //     // }
    // };
}

/// rustty-only test glue (CORE_BLUEPRINT §7) — NOT a port.
///
/// apprt/none.zig cannot faithfully back these tests (it is a 19-loc IPC-only
/// backend: `App` has ONLY `performIpc` and `Surface = struct {}`; Zig's
/// comptime laziness never forces the rest — implementing the apprt-api
/// traits with it would mean FABRICATING `perform_action`/`wakeup`/... so it
/// stays with the apprt/embedded milestone). Instead: a core-local fake
/// runtime (`TestApp`/`TestSurface`) that records every `perform_action` in a
/// `Mutex<Vec<_>>` and counts `wakeup`s, driving the App.zig seams end to
/// end. App.zig itself has 0 `test {}` blocks — every test below is a
/// rustty-only extra (`core_app` MAP row).
//
// (pub(crate): surface.rs's ported `queueIo` readonly test borrows `TestApp`
// as its `Surface<A>` type parameter — P4 W6-tail, `957ed21d5`.)
#[cfg(test)]
pub(crate) mod test_runtime {
    use super::*;
    use rustty_apprt_api::action::{ActionKey, TargetKey};
    use rustty_input::binding::{Trigger, TriggerKey};
    use rustty_vt::input::key::Key;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    /// What the fake runtime records for one `perform_action` call: enough
    /// payload to assert the seam-sensitive cases (`quit_timer` start/stop,
    /// `reload_config{soft}`), the bare `ActionKey` otherwise.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum RecordedAction {
        Simple(ActionKey),
        QuitTimer(QuitTimer),
        ReloadConfig { soft: bool },
    }

    /// The fake apprt runtime (backend `A`).
    pub(crate) struct TestApp {
        /// The backend-owned Config (`embedded.zig:125 config: Config`) the
        /// `App::config()` seam hands out — real default keybinds, plus
        /// whatever a test `put`s.
        config: Config,
        /// Number of `wakeup()` calls (the `Mailbox.push` contract).
        wakeups: AtomicUsize,
        /// Every `perform_action`, in call order, as (target key, payload).
        actions: Mutex<Vec<(TargetKey, RecordedAction)>>,
        /// If set, `perform_action` fails (AFTER recording) for this key —
        /// drives the performAllChainedAction continue-past-failures test.
        fail_key: Option<ActionKey>,
    }

    impl TestApp {
        fn new() -> TestApp {
            TestApp {
                config: Config::default_config(),
                wakeups: AtomicUsize::new(0),
                actions: Mutex::new(Vec::new()),
                fail_key: None,
            }
        }

        fn recorded(&self) -> Vec<(TargetKey, RecordedAction)> {
            self.actions.lock().unwrap().clone()
        }
    }

    impl apprt::App for TestApp {
        type Surface = TestSurface;

        fn perform_action<C: apprt::CoreSurfaceHandle>(
            &self,
            target: apprt::Target<'_, C>,
            action: apprt::Action<'_>,
        ) -> Result<bool, apprt::ApprtError> {
            let key = action.key();
            let rec = match &action {
                apprt::Action::QuitTimer(v) => RecordedAction::QuitTimer(*v),
                apprt::Action::ReloadConfig(v) => RecordedAction::ReloadConfig { soft: v.soft },
                _ => RecordedAction::Simple(key),
            };
            self.actions.lock().unwrap().push((target.key(), rec));
            if self.fail_key == Some(key) {
                return Err(apprt::ApprtError);
            }
            Ok(true)
        }

        fn perform_ipc(
            &self,
            _target: apprt::ipc::Target<'_>,
            _action: apprt::ipc::Action<'_>,
        ) -> Result<bool, apprt::ipc::Errors> {
            unimplemented!("test runtime: not driven by the C4 seam tests")
        }

        fn wakeup(&self) {
            self.wakeups.fetch_add(1, Ordering::SeqCst);
        }

        fn config(&self) -> &Config {
            &self.config
        }

        fn keyboard_layout(&self) -> rustty_input::keyboard::Layout {
            rustty_input::keyboard::Layout::Unknown
        }
    }

    /// The fake runtime surface (backend `A::Surface`), wired the way the
    /// C0 prototype wires it: `core`/`app` back-pointers into test locals.
    pub(crate) struct TestSurface {
        core: *mut Surface<TestApp>,
        app: *mut TestApp,
    }

    /// rustty-only test glue (C5 follow-up): the full `Surface<A>` field set
    /// landed with chunk C5, and a real `Surface` is untestable-by-
    /// construction (CORE_BLUEPRINT §7 — init spawns pty/renderer/io
    /// threads). The reference App is handed POINTERS to Surfaces whose
    /// memory `Surface.init` populates in place (`self: *Surface` on
    /// `undefined` memory); these seam tests need only the two fields App
    /// reads (`id`, `rt_surface`), so they fabricate exactly that — a
    /// `MaybeUninit<Surface>` with only those fields written (sound: field-
    /// projected raw-pointer writes; the other fields are never read, and
    /// `MaybeUninit` never drops).
    fn fabricateSurface(
        mu: &mut std::mem::MaybeUninit<Surface<TestApp>>,
        id: u64,
    ) -> *mut Surface<TestApp> {
        let ptr = mu.as_mut_ptr();
        unsafe {
            std::ptr::addr_of_mut!((*ptr).id).write(id);
            std::ptr::addr_of_mut!((*ptr).rt_surface).write(std::ptr::null_mut());
        }
        ptr
    }

    impl apprt::Surface for TestSurface {
        type App = TestApp;
        type Core = Surface<TestApp>;

        fn core(&self) -> *mut Surface<TestApp> {
            self.core
        }

        fn rt_app(&self) -> *mut TestApp {
            self.app
        }

        fn close(&self, _process_alive: bool) {}

        fn deinit(&mut self) {}

        fn get_content_scale(&self) -> Result<apprt::ContentScale, apprt::ApprtError> {
            unimplemented!("test runtime: not driven by the C4 seam tests")
        }

        fn get_size(&self) -> Result<apprt::SurfaceSize, apprt::ApprtError> {
            unimplemented!("test runtime: not driven by the C4 seam tests")
        }

        fn get_title(&self) -> Option<&std::ffi::CStr> {
            None
        }

        fn supports_clipboard(&self, _clipboard_type: apprt::Clipboard) -> bool {
            unimplemented!("test runtime: not driven by the C4 seam tests")
        }

        fn clipboard_request(
            &self,
            _clipboard_type: apprt::Clipboard,
            _state: apprt::ClipboardRequest,
        ) -> Result<apprt::ClipboardReadResult, apprt::ApprtError> {
            unimplemented!("test runtime: not driven by the C4 seam tests")
        }

        fn set_clipboard(
            &self,
            _clipboard_type: apprt::Clipboard,
            _contents: &[apprt::ClipboardContent<'_>],
            _confirm: bool,
        ) -> Result<(), apprt::ApprtError> {
            unimplemented!("test runtime: not driven by the C4 seam tests")
        }

        fn get_cursor_pos(&self) -> Result<apprt::CursorPos, apprt::ApprtError> {
            unimplemented!("test runtime: not driven by the C4 seam tests")
        }

        fn default_termio_env(&self) -> Result<rustty_global::EnvMap, apprt::ApprtError> {
            unimplemented!("test runtime: not driven by the C4 seam tests")
        }
    }

    // rustty-only: App.create/deinit round trip on an empty app (the
    // reference create/init/deinit/destroy path incl. the grid-set-empty
    // assert and the field declaration defaults).
    #[test]
    // `App::create` (unlike `App::init`, which the sibling tests use and which
    // runs green under Miri) spawns the macOS CoreText font-discovery warmup
    // thread; that reaches `CFStringCreateWithBytes`, a foreign function Miri
    // cannot call. Same class as this crate's already-argued oniguruma
    // exception — see .port/phase-2/MIRI_EXCEPTIONS.md, "rustty-core
    // renderer_config_tests" — whose R6 verifier is the ASan lane. The
    // create/destroy round trip runs in ci.sh's HARD `cargo test --workspace`
    // step; every field assertion below is also covered from `init` by
    // `add_and_delete_surface_drive_the_quit_timer`, which stays Miri-checked.
    #[cfg_attr(
        miri,
        ignore = "App::create spawns the CoreText warmup thread (CFStringCreateWithBytes)"
    )]
    fn app_create_and_destroy_empty() {
        let app = App::<TestApp>::create(Allocator).unwrap();
        assert!(app.surfaces.is_empty());
        assert!(app.focused);
        assert!(app.focused_surface.is_none());
        assert!(app.last_notification_time.is_none());
        assert_eq!(app.last_notification_digest, 0);
        assert!(app.first);
        app.destroy();
    }

    // rustty-only: Mailbox.push stores the message, returns the queue Size,
    // and wakes the rt_app ("Wake up our app loop") — including through an
    // [ADV-FIX 6a] by-value copy of the handle.
    #[test]
    fn mailbox_push_wakes_rt_app_and_returns_size() {
        let mut rt_app = TestApp::new();
        let mut app = App::<TestApp>::init(Allocator).unwrap();

        let mailbox = Mailbox::<TestApp> {
            rt_app: &mut rt_app,
            mailbox: &mut app.mailbox,
        };
        // The reference passes App.Mailbox around by value — exercise the
        // no-`A: Copy`-bound copy.
        let copy = mailbox;

        assert_eq!(
            copy.push(Message::OpenConfig(OpenConfig::OsOpen), Timeout::Instant),
            1
        );
        assert_eq!(rt_app.wakeups.load(Ordering::SeqCst), 1);
        assert_eq!(mailbox.push(Message::Quit, Timeout::Instant), 2);
        assert_eq!(rt_app.wakeups.load(Ordering::SeqCst), 2);

        assert!(matches!(app.mailbox.pop(), Some(Message::OpenConfig(_))));
        assert!(matches!(app.mailbox.pop(), Some(Message::Quit)));
        assert!(app.mailbox.pop().is_none());

        app.deinit();
    }

    // rustty-only: tick/drainMailbox dispatches queued messages, and `.quit`
    // short-circuits the drain ("stop draining the mailbox immediately"),
    // leaving later messages queued for the next tick.
    #[test]
    fn drain_mailbox_dispatches_and_quit_short_circuits() {
        let rt_app = TestApp::new();
        let mut app = App::<TestApp>::init(Allocator).unwrap();

        // Plain dispatch: open_config reaches the apprt.
        app.mailbox
            .push(Message::OpenConfig(OpenConfig::OsOpen), Timeout::Instant);
        app.tick(&rt_app).unwrap();
        assert_eq!(
            rt_app.recorded(),
            vec![(
                TargetKey::App,
                RecordedAction::Simple(ActionKey::OpenConfig)
            )]
        );
        rt_app.actions.lock().unwrap().clear();

        // Short circuit: quit first, another message queued behind it.
        app.mailbox.push(Message::Quit, Timeout::Instant);
        app.mailbox
            .push(Message::OpenConfig(OpenConfig::OsOpen), Timeout::Instant);
        app.tick(&rt_app).unwrap();
        assert_eq!(
            rt_app.recorded(),
            vec![(TargetKey::App, RecordedAction::Simple(ActionKey::Quit))]
        );
        // The open_config message is still in the queue for the next tick.
        assert!(matches!(app.mailbox.pop(), Some(Message::OpenConfig(_))));

        app.deinit();
    }

    // rustty-only: focusEvent flips the app focus state and dedupes
    // ("Prevent redundant focus events").
    #[test]
    fn focus_event_sets_state_and_dedupes() {
        let mut app = App::<TestApp>::init(Allocator).unwrap();

        // Defaults to true; a redundant `true` early-returns.
        assert!(app.focused);
        app.focusEvent(true);
        assert!(app.focused);

        app.focusEvent(false);
        assert!(!app.focused);
        app.focusEvent(false);
        assert!(!app.focused);

        app.focusEvent(true);
        assert!(app.focused);

        app.deinit();
    }

    // rustty-only: colorSchemeEvent sets the conditional state and requests
    // `reload_config` with `soft: true`.
    #[test]
    fn color_scheme_event_sets_conditional_state_and_soft_reloads() {
        let rt_app = TestApp::new();
        let mut app = App::<TestApp>::init(Allocator).unwrap();
        assert_eq!(
            app.config_conditional_state.theme,
            conditional::Theme::Light
        );

        app.colorSchemeEvent(&rt_app, apprt::ColorScheme::Dark)
            .unwrap();
        assert_eq!(app.config_conditional_state.theme, conditional::Theme::Dark);
        assert_eq!(
            rt_app.recorded(),
            vec![(TargetKey::App, RecordedAction::ReloadConfig { soft: true })]
        );

        app.deinit();
    }

    // rustty-only: colorSchemeEvent is a no-op when the scheme didn't change
    // ("If our scheme didn't change, then we don't do anything.").
    #[test]
    fn color_scheme_event_same_scheme_is_noop() {
        let rt_app = TestApp::new();
        let mut app = App::<TestApp>::init(Allocator).unwrap();

        // The default conditional theme is light.
        app.colorSchemeEvent(&rt_app, apprt::ColorScheme::Light)
            .unwrap();
        assert_eq!(
            app.config_conditional_state.theme,
            conditional::Theme::Light
        );
        assert!(rt_app.recorded().is_empty());

        app.deinit();
    }

    // rustty-only: addSurface cancels the quit timer; deleting the last
    // surface starts it; findSurfaceByID resolves the unique ID while the
    // surface is registered.
    #[test]
    fn add_and_delete_surface_drive_the_quit_timer() {
        let mut rt_app = TestApp::new();
        let mut app = App::<TestApp>::init(Allocator).unwrap();

        let mut core_mu = std::mem::MaybeUninit::<Surface<TestApp>>::uninit();
        let core = fabricateSurface(&mut core_mu, 1);
        let mut rt_surface = TestSurface {
            core,
            app: &mut rt_app,
        };
        unsafe { std::ptr::addr_of_mut!((*core).rt_surface).write(&mut rt_surface) };

        app.addSurface(&mut rt_surface);
        assert_eq!(app.surfaces.len(), 1);
        assert_eq!(
            rt_app.recorded(),
            vec![(TargetKey::App, RecordedAction::QuitTimer(QuitTimer::Stop))]
        );

        // While registered, the surface resolves by its unique ID.
        assert_eq!(app.findSurfaceByID(1), Some(core));
        assert!(app.findSurfaceByID(2).is_none());

        // SB: clear through the SAME raw alias `deleteSurface` will write
        // through (`rt_surface.app`) — a fresh `&mut`-derived access via the
        // owner `rt_app` here would pop the alias tag stored at construction
        // and make the `perform_action` recording below Stacked-Borrows UB
        // (Miri-caught; the shaper-hook interleave rule: route mid-test
        // mutations via the alias's own pointer, never the owner).
        unsafe { (*rt_surface.app).actions.lock().unwrap().clear() };
        app.deleteSurface(&mut rt_surface);
        assert!(app.surfaces.is_empty());
        assert_eq!(
            rt_app.recorded(),
            vec![(TargetKey::App, RecordedAction::QuitTimer(QuitTimer::Start))]
        );

        app.deinit();
    }

    // rustty-only: deleteSurface clears a matching focused_surface (the
    // address-reuse false-positive bug comment) — and focusSurface/
    // focusedSurface validate through hasSurface.
    #[test]
    fn delete_surface_clears_matching_focused_surface() {
        let mut rt_app = TestApp::new();
        let mut app = App::<TestApp>::init(Allocator).unwrap();

        let mut core_mu = std::mem::MaybeUninit::<Surface<TestApp>>::uninit();
        let core = fabricateSurface(&mut core_mu, 7);
        let mut rt_surface = TestSurface {
            core,
            app: &mut rt_app,
        };
        unsafe { std::ptr::addr_of_mut!((*core).rt_surface).write(&mut rt_surface) };

        // Unknown surfaces don't focus (hasSurface validation).
        app.focusSurface(core);
        assert!(app.focused_surface.is_none());

        app.addSurface(&mut rt_surface);
        app.focusSurface(core);
        assert_eq!(app.focused_surface, Some(core));
        assert_eq!(app.focusedSurface(), Some(core));

        app.deleteSurface(&mut rt_surface);
        assert!(app.focused_surface.is_none());
        assert!(app.focusedSurface().is_none());

        app.deinit();
    }

    // rustty-only: needsConfirmQuit is false with zero surfaces (the per-
    // surface query is never reached).
    #[test]
    fn needs_confirm_quit_false_with_zero_surfaces() {
        let mut app = App::<TestApp>::init(Allocator).unwrap();
        assert!(!app.needsConfirmQuit());
        app.deinit();
    }

    // rustty-only: keyEvent ignores key release events.
    #[test]
    fn key_event_release_returns_false() {
        let rt_app = TestApp::new();
        let mut app = App::<TestApp>::init(Allocator).unwrap();

        let ev = KeyEvent {
            action: key::Action::Release,
            key: Key::KeyA,
            ..Default::default()
        };
        assert!(!app.keyEvent(&rt_app, ev));
        assert!(rt_app.recorded().is_empty());

        app.deinit();
    }

    // rustty-only: an unfocused app processes only GLOBAL keybinds — a
    // matching non-global binding returns false untouched.
    #[test]
    fn key_event_unfocused_non_global_returns_false() {
        let mut rt_app = TestApp::new();
        // Real Config keybinds (default set) + the binding under test:
        // `a -> open_config` (app-scoped, non-global).
        rt_app.config.keybind.set.put(
            Trigger {
                mods: Default::default(),
                key: TriggerKey::Physical(Key::KeyA),
            },
            BindingAction::OpenConfig(rustty_input::binding::OpenConfig::DEFAULT),
        );
        let mut app = App::<TestApp>::init(Allocator).unwrap();
        app.focusEvent(false);

        let ev = KeyEvent {
            key: Key::KeyA,
            ..Default::default()
        };
        assert!(!app.keyEvent(&rt_app, ev));
        assert!(rt_app.recorded().is_empty());

        app.deinit();
    }

    // rustty-only: a focused app performs an app-scoped leaf binding through
    // performAction and reports the event as used.
    #[test]
    fn key_event_app_scoped_leaf_performs_action() {
        let mut rt_app = TestApp::new();
        rt_app.config.keybind.set.put(
            Trigger {
                mods: Default::default(),
                key: TriggerKey::Physical(Key::KeyA),
            },
            BindingAction::OpenConfig(rustty_input::binding::OpenConfig::DEFAULT),
        );
        let mut app = App::<TestApp>::init(Allocator).unwrap();
        assert!(app.focused);

        let ev = KeyEvent {
            key: Key::KeyA,
            ..Default::default()
        };
        assert!(app.keyEvent(&rt_app, ev));
        assert_eq!(
            rt_app.recorded(),
            vec![(
                TargetKey::App,
                RecordedAction::Simple(ActionKey::OpenConfig)
            )]
        );

        // A surface-scoped binding is NOT app business — keyEvent leaves it
        // for Surface.keyEvent.
        rt_app.actions.lock().unwrap().clear();
        rt_app.config.keybind.set.put(
            Trigger {
                mods: Default::default(),
                key: TriggerKey::Physical(Key::KeyB),
            },
            BindingAction::ClearScreen,
        );
        let ev = KeyEvent {
            key: Key::KeyB,
            ..Default::default()
        };
        assert!(!app.keyEvent(&rt_app, ev));
        assert!(rt_app.recorded().is_empty());

        app.deinit();
    }

    // rustty-only: performAllChainedAction continues past failures ("We will
    // continue executing each action even if there is a failure in a prior
    // action") — and the surface-scoped performAllAction arm no-ops over an
    // empty surface list.
    #[test]
    fn perform_all_chained_action_continues_past_failures() {
        let mut rt_app = TestApp::new();
        rt_app.fail_key = Some(ActionKey::Quit);
        let mut app = App::<TestApp>::init(Allocator).unwrap();

        app.performAllChainedAction(
            &rt_app,
            &[
                BindingAction::Quit,
                BindingAction::OpenConfig(rustty_input::binding::OpenConfig::DEFAULT),
            ],
        );
        assert_eq!(
            rt_app.recorded(),
            vec![
                (TargetKey::App, RecordedAction::Simple(ActionKey::Quit)),
                (
                    TargetKey::App,
                    RecordedAction::Simple(ActionKey::OpenConfig)
                ),
            ]
        );

        // Surface-scoped fan-out over zero surfaces is a quiet no-op.
        app.performAllAction(&rt_app, BindingAction::ClearScreen)
            .unwrap();

        app.deinit();
    }

    // ===================================================================
    // task 14d-1 (S39-F6): the restored `App.zig` diagnostics.
    //
    // Three tests, chosen for shapes that break INDEPENDENTLY of each
    // other rather than one per site: a plain interpolated argument, the
    // `logEnabled` gate + `{t}`/`@tagName` rendering + the `redraw_surface`
    // exclusion, and a failure-arm site carrying both `@tagName(action)`
    // and the error payload. Every assertion is a full-vector `assert_eq!`
    // on the captured records, so a wrong level, a wrong scope, an extra
    // record or a dropped/renamed payload all fail.
    // ===================================================================

    /// `App.zig:349` — `log.debug("focus event focused={}", .{focused})`.
    /// Also pins the redundant-focus-event early return staying SILENT:
    /// the reference logs only when the state actually changes, and a
    /// diagnostic above that `if` would fire on every no-op focus event
    /// (the success-path-diagnostic bug class this plan exists to catch).
    #[test]
    fn focus_event_emits_the_debug_diagnostic_only_on_a_real_change() {
        let mut app = App::<TestApp>::init(Allocator).unwrap();
        // `init` leaves `focused = true`.
        let cap = crate::log_capture::start();

        // Redundant: same value, must not log.
        app.focusEvent(true);
        // A real change: must log.
        app.focusEvent(false);

        let messages = cap.messages();
        drop(cap);
        app.deinit();

        assert_eq!(
            vec![(
                log::Level::Debug,
                "app".to_string(),
                "focus event focused=false".to_string()
            )],
            messages
        );
    }

    /// `App.zig:271` + `App.zig:294` — the `logEnabled(.debug, .app)`
    /// switch (which skips `.redraw_surface` as "way too verbose") and the
    /// quit short-circuit `log.info`. Asserts the `{t}` rendering uses the
    /// REFERENCE tag names (`quit`, not `Quit`).
    #[test]
    fn drain_mailbox_logs_message_tags_but_skips_redraw_surface() {
        let mut rt_app = TestApp::new();
        let mut app = App::<TestApp>::init(Allocator).unwrap();

        // An unregistered rt-surface pointer: `redrawSurface` rejects it
        // through `hasRtSurface` before any dereference, so this is only
        // ever compared, never read.
        let unknown: *mut TestSurface = std::ptr::NonNull::dangling().as_ptr();
        {
            let mailbox = Mailbox::<TestApp> {
                rt_app: &mut rt_app,
                mailbox: &mut app.mailbox,
            };
            mailbox.push(Message::RedrawSurface(unknown), Timeout::Instant);
            mailbox.push(Message::Quit, Timeout::Instant);
        }

        let cap = crate::log_capture::start();
        app.drainMailbox(&rt_app).unwrap();
        let messages = cap.messages();
        drop(cap);
        app.deinit();

        assert_eq!(
            vec![
                (
                    log::Level::Debug,
                    "app".to_string(),
                    "mailbox message=quit".to_string()
                ),
                (
                    log::Level::Info,
                    "app".to_string(),
                    "quit message received, short circuiting mailbox drain".to_string()
                ),
            ],
            messages
        );
    }

    /// `App.zig:491` — `log.warn("error performing chained action
    /// action={s} err={}", .{ @tagName(action), err })`. The failing action
    /// is first, so the test also pins that the diagnostic sits on the
    /// FAILURE arm only: the second (succeeding) action emits nothing.
    #[test]
    fn perform_all_chained_action_warns_on_the_failing_action_only() {
        let mut rt_app = TestApp::new();
        rt_app.fail_key = Some(ActionKey::Quit);
        let mut app = App::<TestApp>::init(Allocator).unwrap();

        let cap = crate::log_capture::start();
        app.performAllChainedAction(
            &rt_app,
            &[
                BindingAction::Quit,
                BindingAction::OpenConfig(rustty_input::binding::OpenConfig::DEFAULT),
            ],
        );
        let messages = cap.messages();
        drop(cap);
        app.deinit();

        assert_eq!(
            vec![(
                log::Level::Warn,
                "app".to_string(),
                "error performing chained action action=quit err=Apprt(ApprtError)".to_string()
            )],
            messages
        );
    }
}
