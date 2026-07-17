//! Renderer seam — core-local dependency placeholders for the GPU renderer
//! and the renderer thread (CORE_BLUEPRINT §5).
//!
//! PORTS (PLACEHOLDER SHAPES ONLY — NOT the port): ghostty
//! src/renderer/generic.zig (`renderer.Renderer`, the comptime
//! `GenericRenderer(Metal|OpenGL)`), src/renderer/Options.zig
//! (`renderer.Options`) and src/renderer/Thread.zig (the `renderer.Thread`
//! field shape). The REAL ports are Phase-3 R-d (the GPU
//! Renderer/Metal/OpenGL tier) + D6 (the xev run loop) work; they cannot land
//! in rustty-renderer today because renderer/Thread.zig imports `configpkg`
//! AND core's `App.Mailbox`, neither of which rustty-renderer may dep
//! (rustty-config already depends on rustty-renderer — the G7 cycle — and a
//! renderer -> core edge is upside down). So core carries the SHAPES it
//! embeds (Surface.zig fields `renderer: renderer.Renderer` /
//! `renderer_thread: renderer.Thread`; Surface.zig:551 `Renderer.init(alloc,
//! .{...})`), exactly the termio crate-local `xev_seam` precedent: inhabited
//! structs, faithful field/signature shapes, `unimplemented!()` method bodies
//! — dependency placeholders, with NO reference logic stubbed or weakened
//! behind them. Runtime panics are confined to these placeholder leaves;
//! everything that calls them compiles and is reviewable.
//!
//! What is NOT here (lands WITH the real Phase-3/D6 ports, nothing of it is
//! stubbed): generic.zig's GraphicsAPI/SwapChain/FrameState/CustomShaderState
//! machinery and the rest of its method surface, and renderer/Thread.zig's
//! run loop (`threadMain_`, `drainMailbox`, the render/draw/cursor timer
//! callbacks, `DRAW_INTERVAL`/`CURSOR_BLINK_INTERVAL`,
//! `must_draw_from_app_thread`). Already REAL elsewhere:
//! `rustty_renderer::state::State` and `rustty_renderer::thread::Mailbox`
//! (this placeholder `Thread<A>` points at both), and now the renderer's
//! `DerivedConfig` — the config plane is landed, so this module re-exports the
//! real `rustty_renderer::DerivedConfig` (crate-root re-export) for the
//! `Options<A>.config` field below; only the windowing `Renderer`/xev `Thread`
//! shapes remain placeholders here.
//!
//! PORT STATUS: placeholder (core chunk C3; filled by Phase-3 R-d + D6).

// PARITY: the reference's identifiers are camelCase (`surfaceInit`,
// `finalizeSurfaceInit`, `threadEnter`, `drawFrame`, `threadMain`); preserved
// verbatim per R1/R2 (termio thread.rs precedent).
#![allow(non_snake_case)]

// PORT NOTE: `alloc: Allocator` params mirror the termio/vt convention
// (`allocator_api2::alloc::Global as Allocator`).
use allocator_api2::alloc::Global as Allocator;
use rustty_apprt_api as apprt;
use rustty_font_shaper::SharedGrid;
use rustty_renderer::size::Size;
use rustty_renderer::state::State;
use rustty_renderer::thread;
// PORT NOTE: the real cross-platform `renderer.Renderer.DerivedConfig`
// (`rustty_renderer::DerivedConfig`, crate-root re-export of `generic.rs`) —
// swapped in for the former core-local placeholder (the config plane is landed;
// `Options<A>.config` below carries it). `init<C: RendererConfig>` runs headless
// with `C = rustty_config::Config`, the type core binds that the renderer crate
// cannot.
use rustty_renderer::message::Message;
use rustty_renderer::DerivedConfig;
use rustty_xev as xev;

// PORT NOTE (W3 E2): `const crash = @import("../crash/main.zig");` — the renderer
// thread sets its crash metadata (`crash.sentry.thread_state`) in `threadMain_`.
use rustty_crash as crash;
// PORT NOTE (W3 E2): `const internal_os = @import("../os/main.zig");` — used by
// `threadMain_` (pthread_setname_np) and `setQosClass`.
use rustty_os as internal_os;

use crate::surface::SurfaceMailbox;

// Thread.zig:19-20
const DRAW_INTERVAL: u64 = 8; // 120 FPS
const CURSOR_BLINK_INTERVAL: u64 = 600;

// PORT NOTE (Thread.zig:22-30):
//   const must_draw_from_app_thread =
//       if (@hasDecl(apprt.App, "must_draw_from_app_thread"))
//           apprt.App.must_draw_from_app_thread
//       else
//           false;
// `@hasDecl(apprt.App, "must_draw_from_app_thread")` — the embedded/macOS App
// (this milestone) does NOT declare it (only GTK does, which is deferred), so
// the probe resolves to `false`. When GTK lands it flips this to an
// `apprt::App::MUST_DRAW_FROM_APP_THREAD` (a new default-false trait const).
// Both `drawFrame` arms (the `drawFrame` helper below) stay live regardless.
const MUST_DRAW_FROM_APP_THREAD: bool = false;

/// PORT NOTE (Rust-only glue): the reference `threadMain_` returns an INFERRED
/// error union combining the xev handle errors (`loop.run`, `wakeup.notify`)
/// with the renderer's (`loopEnter`, `threadEnter`). Rust has no inferred
/// unions, so the three concrete error types are unified in this enum;
/// `threadMain` only logs+discards it (the reference `catch |err| log.warn`),
/// so the discriminant is never inspected at runtime.
// PORT NOTE: the payloads are the values the reference `threadMain`'s
// `log.warn("error in renderer err={}", .{err})` would format; that log is
// ported as a comment (the module's log convention), so the payloads are held
// but unread until logging is wired — hence `allow(dead_code)`.
#[derive(Debug)]
#[allow(dead_code)]
enum ThreadMainError {
    /// A renderer entry point failed (`loopEnter`/`threadEnter`).
    Renderer(RendererError),
    /// `wakeup.notify()` failed.
    Notify(xev::NotifyError),
    /// `loop.run(.until_done)` failed.
    Xev(xev::Error),
}

impl From<RendererError> for ThreadMainError {
    fn from(err: RendererError) -> ThreadMainError {
        ThreadMainError::Renderer(err)
    }
}
impl From<xev::NotifyError> for ThreadMainError {
    fn from(err: xev::NotifyError) -> ThreadMainError {
        ThreadMainError::Notify(err)
    }
}
impl From<xev::Error> for ThreadMainError {
    fn from(err: xev::Error) -> ThreadMainError {
        ThreadMainError::Xev(err)
    }
}

/// PORT NOTE ([`ThreadStateGuard`] family, Rust-only glue): the reference
/// `defer crash.sentry.thread_state = null;` in `threadMain_` (Thread.zig:222)
/// — the `defer` clears the crash thread-local on EVERY exit path, declared
/// right after the set so it drops in the reference's LIFO defer position.
struct RendererThreadStateGuard;
impl Drop for RendererThreadStateGuard {
    fn drop(&mut self) {
        // defer crash.sentry.thread_state = null;
        crash::sentry::clearThreadState();
    }
}

/// PORT NOTE (Rust-only glue): the reference
/// `defer if (has_loop) self.renderer.loopExit();` (Thread.zig:230). Holds the
/// raw `*mut Renderer` (the child no longer races the main thread once the
/// guard drops — `loop.run` has returned). `@hasDecl(Renderer, "loopExit")` is
/// TRUE for Metal, so this fires unconditionally.
struct LoopExitGuard(*mut Renderer);
impl Drop for LoopExitGuard {
    fn drop(&mut self) {
        // self.renderer.loopExit();
        unsafe { (*self.0).0.loop_exit() };
    }
}

/// PORT NOTE (Rust-only glue): the reference `defer self.renderer.threadExit();`
/// (Thread.zig:236).
struct ThreadExitGuard(*mut Renderer);
impl Drop for ThreadExitGuard {
    fn drop(&mut self) {
        // self.renderer.threadExit();
        unsafe { (*self.0).0.thread_exit() };
    }
}

/// The error type of the [`Renderer`] entry points.
//
// PORT NOTE: the reference error sets are INFERRED error unions (`!void` /
// `!Self`, from the concrete GraphicsAPI + shader/font failures). E1 (W3)
// aliases the REAL renderer error (`rustty_renderer::generic::Error` — the
// aggregate Metal-leaf + OOM enum, generic.zig's inferred `drawFrame`/`init`
// error set), so the real variant (`MetalFailed`/`OutOfMemory`/`Defunct`/
// `ImageDecode`) propagates out of `drawFrame` faithfully rather than an opaque
// marker. Its `Display`/`std::error::Error` impls live in
// `rustty_renderer::generic` (the `write!("{self:?}")` bump); `Surface::draw`'s
// `Result<(), RendererError>` and `InitError::Renderer`'s `From`/`Display`
// delegation are unchanged.
pub type RendererError = rustty_renderer::generic::Error;

// PORT NOTE (SWAP LANDED): the `renderer.Renderer.DerivedConfig`
// (generic.zig:536, `pub fn init` generic.zig:574, `pub fn deinit`
// generic.zig:651) is no longer a core-local placeholder — it is the REAL
// cross-platform `rustty_renderer::DerivedConfig` imported above (its
// `init<C>`/`deinit` bodies live in `rustty_renderer::generic`). Core
// constructs it directly at `Surface::init` (surface.rs) with
// `C = rustty_config::Config`. NOT to be confused with the renderer THREAD's
// own `rustty_renderer::thread::DerivedConfig` (`custom_shader_animation`),
// which `Thread<A>.config` below embeds.

/// The renderer implementation — the monomorphic `rustty_renderer::Renderer`,
/// wrapped so core keeps the camelCase seam names + the `Options<A>` ctor whose
/// apprt-typed fields the real `generic::Options` cannot hold (surface-mailbox
/// seam).
//
// PORT NOTE (CORE_BLUEPRINT §5, E1/W3 SWAP LANDED): the reference is
// comptime-chosen — `pub const Renderer = switch (build_config.renderer) {
// .metal => GenericRenderer(Metal), .opengl => GenericRenderer(OpenGL), ... }`
// (renderer.zig:38) — here the macOS/Metal monomorph. Newtype forwarder (NOT a
// bare `pub use`): core's callsites use the camelCase seam names
// (`surfaceInit`/`finalizeSurfaceInit`/`threadEnter`/`drawFrame`) and core's
// `Renderer::init` takes `Options<A>` (3 apprt-typed fields the real
// `generic::Options` refuses — naming them in rustty-renderer would cycle). `.0`
// is `pub` so the same-crate `Thread<A>` callbacks reach the full real method
// surface directly through `(*self.renderer).0`; out-of-module callers only
// touch the 6 forwarders below. Surface.zig embeds it by value
// (`renderer: renderer.Renderer`).
pub struct Renderer(pub rustty_renderer::Renderer);

impl Renderer {
    /// This is called early right after surface creation.
    //
    // PORT NOTE: `pub fn surfaceInit(surface: *apprt.Surface) !void`
    // (generic.zig:864, a type-level fn — no self); callsite Surface.zig:493
    // `try Renderer.surfaceInit(rt_surface);`. Generic `<S>` per the
    // blueprint §5 signature: the forwarder passes the pointer without
    // needing an `App` bound.
    pub fn surfaceInit<S>(rt_surface: *mut S) -> Result<(), RendererError> {
        // (A) `Error == RendererError`, so the real error flows out directly.
        rustty_renderer::Renderer::surface_init(rt_surface)
    }

    // PORT NOTE ([ADV-FIX 3]): the reference init takes `(alloc, options)` —
    // `pub fn init(alloc: Allocator, options: renderer.Options) !Self`
    // (generic.zig:658) — so the alloc STAYS on the forwarder. Callsite
    // Surface.zig:551. Maps `Options<A>` -> the real `generic::Options`
    // (`config`/`font_grid`/`size`); the apprt-typed `surface_mailbox`/
    // `rt_surface`/`thread` stay in core (surface-mailbox seam: the real
    // renderer does not store them).
    //
    // ERRDEFER PARITY (discharged by the real init): `options.config` is the
    // REAL `rustty_renderer::DerivedConfig` (moved in). The reference frees a
    // partially-built config via its own errdefer on the `init` error path;
    // that errdefer lives INSIDE the real `Renderer::init` (it owns the moved
    // `DerivedConfig`), so the forwarder does NOT re-run `config.deinit()` on
    // error — doing so would be a double-free.
    pub fn init<A: apprt::App + 'static>(
        alloc: Allocator,
        options: Options<A>,
    ) -> Result<Renderer, RendererError> {
        let Options {
            config,
            font_grid,
            size,
            surface_mailbox,
            rt_surface,
            thread,
        } = options;
        let _ = (surface_mailbox, rt_surface, thread); // held-in-core seam bits
        let inner = rustty_renderer::Renderer::init(
            alloc,
            rustty_renderer::generic::Options {
                config,
                font_grid,
                size,
            },
        )?;
        Ok(Renderer(inner))
    }

    /// This is called just prior to spinning up the renderer thread for
    /// final main thread setup requirements.
    //
    // PORT NOTE: `pub fn finalizeSurfaceInit(self: *Self, surface:
    // *apprt.Surface) !void` (generic.zig:873); callsite Surface.zig:712.
    pub fn finalizeSurfaceInit<S>(&mut self, rt_surface: *mut S) -> Result<(), RendererError> {
        self.0.finalize_surface_init(rt_surface)
    }

    /// W-ATTACH: route the host NSView into the renderer's windowed layer-attach.
    /// Forwards the raw view pointer + scale (core reads them from the apprt
    /// surface's `host_view()`/`host_content_scale()`); the renderer never names an
    /// apprt type (R4). Called from `Surface::init` right after
    /// `finalizeSurfaceInit`, before the render thread spawns.
    pub fn attachHostView(
        &mut self,
        view: *mut core::ffi::c_void,
        scale: f64,
    ) -> Result<(), RendererError> {
        self.0.attach_host_view(view, scale)
    }

    /// Callback called by renderer.Thread when it begins.
    //
    // PORT NOTE: `pub fn threadEnter(self: *const Self, surface:
    // *apprt.Surface) !void` (generic.zig:881); core reaches it on the
    // surface deinit path (Surface.zig:801
    // `self.renderer.threadEnter(self.rt_surface) catch unreachable;`).
    pub fn threadEnter<S>(&self, rt_surface: *mut S) -> Result<(), RendererError> {
        self.0.thread_enter(rt_surface)
    }

    /// Draw the frame to the screen.
    ///
    /// If `sync` is true, this will synchronously block until
    /// the frame is finished drawing and has been presented.
    //
    // PORT NOTE: `pub fn drawFrame(self: *Self, sync: bool) !void`
    // (generic.zig:1430); callsite Surface.zig:884 (`draw` — "Renderers are
    // required to support `drawFrame` being called from the main thread").
    //
    // W5 (`W5_DESIGN §2.5` caller 2 / §3.4b): RAW-self, per the Zig `self: *Self`
    // receiver. A `&mut self` receiver would be a protected `&mut` over the same
    // bytes as `rustty_renderer::Renderer` (newtype at offset 0), UB-concurrent
    // with the render thread's `draw_frame_raw`. The port's prior `&mut self` was
    // the divergence; raw-self is the faithful shape.
    //
    /// # Safety
    /// `self_` must be a valid, stable `*mut Renderer` (the Surface-embedded renderer,
    /// aliased raw by the render thread). No protected `&mut` may span the call.
    pub unsafe fn drawFrame(self_: *mut Renderer, sync: bool) -> Result<(), RendererError> {
        // self.0.draw_frame(sync)  →  raw-self draw (no protector spans the call)
        unsafe { rustty_renderer::Renderer::draw_frame_raw(&raw mut (*self_).0, sync) }
    }

    // PORT NOTE: `pub fn deinit(self: *Self) void` (generic.zig:800).
    pub fn deinit(&mut self) {
        self.0.deinit();
    }
}

/// The options that are used to configure a renderer.
//
// PORT NOTE: renderer/Options.zig (the file IS the struct -> named `Options`;
// termio options.rs precedent). Generic over the apprt backend `A`
// (CORE_BLUEPRINT §5): `surface_mailbox`/`rt_surface`/`thread` name
// backend-parameterized types. The blueprint's §5 sketch wrote
// `Options<'_, A>`, but the reference file has NO borrowed field (`config` is
// by value, the rest are raw pointers), so a lifetime parameter would be
// unused (E0392) and is dropped — faithful to Options.zig.
pub struct Options<A: apprt::App + 'static> {
    /// The derived configuration for this renderer implementation.
    //
    // PORT NOTE: `config: renderer.Renderer.DerivedConfig` — now the real
    // `rustty_renderer::DerivedConfig` (crate-root re-export, imported above);
    // the seam `Options<A>` still carries it because the real
    // `rustty_renderer::Options` cannot hold the 3 apprt-typed fields below
    // (`rustty-apprt-api -> rustty-renderer` would cycle).
    pub config: DerivedConfig,

    /// The font grid that should be used along with the key for deref-ing.
    //
    // PORT NOTE: `font_grid: *font.SharedGrid` — rustty-font-shaper's
    // `SharedGrid` (the D1 3-tier shaper crate ABOVE vt+font;
    // `SharedGridSet::ref` returns exactly this `*mut SharedGrid`).
    pub font_grid: *mut SharedGrid,

    /// The size of everything.
    pub size: Size,

    /// The mailbox for sending the surface messages. This is only valid
    /// once the thread has started and should not be used outside of the thread.
    //
    // PORT NOTE: `surface_mailbox: apprt.surface.Mailbox` — core's
    // `SurfaceMailbox<A>` (CORE_BLUEPRINT §1; declared in `crate::surface`,
    // C3 skeleton).
    pub surface_mailbox: SurfaceMailbox<A>,

    /// The apprt surface.
    pub rt_surface: *mut A::Surface,

    /// The renderer thread.
    pub thread: *mut Thread<A>,
}

/// Represents the renderer thread logic. The renderer thread is able to
/// be woken up to render.
//
// PORT NOTE (DEPENDENCY PLACEHOLDER, CORE_BLUEPRINT §5): renderer/Thread.zig
// — the FAITHFUL field shape (what Surface.zig embeds and wires:
// `renderer_thread: renderer.Thread`, the `.renderer_wakeup =
// render_thread.wakeup` handle copy at :675, `renderer_thread.mailbox` pushes,
// `renderer_thread.stop.notify()` at :796), with `init`/`threadMain`/`deinit`
// as `unimplemented!()` placeholders. Generic over the apprt backend `A`
// (reference fields `surface: *apprt.Surface`, `app_mailbox: App.Mailbox`).
// The reference's first field `alloc: std.mem.Allocator`
// ("Allocator used for some state") collapses — Rust `Box` ownership carries
// the mailbox cleanup (termio thread.rs precedent). Field defaults
// (`= .{}` / `= false`) are documented at each field for the Phase-3 fill.
pub struct Thread<A: apprt::App + 'static> {
    /// The main event loop for the application. The user data of this loop
    /// is always the allocator used to create the loop. This is a convenience
    /// so that users of the loop always have an allocator.
    //
    // PORT NOTE: reference field name `loop` is a Rust keyword -> `loop_`
    // (termio thread.rs precedent).
    pub loop_: xev::Loop,

    /// This can be used to wake up the renderer and force a render safely from
    /// any thread.
    pub wakeup: xev::Async,
    // (reference default: `= .{}`)
    pub wakeup_c: xev::Completion,

    /// This can be used to stop the renderer on the next loop iteration.
    pub stop: xev::Async,
    // (reference default: `= .{}`)
    pub stop_c: xev::Completion,

    /// The timer used for rendering
    pub render_h: xev::Timer,
    // (reference default: `= .{}`)
    pub render_c: xev::Completion,

    /// The timer used for draw calls. Draw calls don't update from the
    /// terminal state so they're much cheaper. They're used for animation
    /// and are paused when the terminal is not focused.
    pub draw_h: xev::Timer,
    // (reference default: `= .{}`)
    pub draw_c: xev::Completion,
    // (reference default: `= false`)
    pub draw_active: bool,

    /// This async is used to force a draw immediately. This does not
    /// coalesce like the wakeup does.
    pub draw_now: xev::Async,
    // (reference default: `= .{}`)
    pub draw_now_c: xev::Completion,

    /// The timer used for cursor blinking
    pub cursor_h: xev::Timer,
    // (reference default: `= .{}`)
    pub cursor_c: xev::Completion,
    // (reference default: `= .{}`)
    pub cursor_c_cancel: xev::Completion,

    /// The surface we're rendering to.
    //
    // PORT NOTE: `surface: *apprt.Surface` -> the backend surface pointer.
    pub surface: *mut A::Surface,

    /// The underlying renderer implementation.
    //
    // PORT NOTE: `renderer: *rendererpkg.Renderer` -> the placeholder above.
    pub renderer: *mut Renderer,

    /// Pointer to the shared state that is used to generate the final render.
    //
    // PORT NOTE: `state: *rendererpkg.State` — REAL, already landed
    // (`rustty_renderer::state::State`).
    pub state: *mut State,

    /// The mailbox that can be used to send this thread messages. Note
    /// this is a blocking queue so if it is full you will get errors (or block).
    //
    // PORT NOTE: `mailbox: *Mailbox` — REAL, already landed
    // (`rustty_renderer::thread::Mailbox` =
    // `BlockingQueue<renderer::Message, 64>`).
    pub mailbox: *mut thread::Mailbox,

    /// Mailbox to send messages to the app thread
    pub app_mailbox: crate::app::Mailbox<A>,

    /// Configuration we need derived from the main config.
    //
    // PORT NOTE: `config: DerivedConfig` — renderer/Thread.zig's OWN
    // `DerivedConfig` (`custom_shader_animation`), the [ADV-FIX 2] INHABITED
    // placeholder in `rustty_renderer::thread` — NOT this module's
    // `renderer.Renderer.DerivedConfig` placeholder above.
    pub config: thread::DerivedConfig,

    // (reference default: `= .{}`)
    pub flags: Flags,
}

/// PORT NOTE: the reference's inline `flags: packed struct { ... } = .{}`
/// (renderer/Thread.zig:94).
pub struct Flags {
    /// This is true when a blinking cursor should be visible and false
    /// when it should not be visible. This is toggled on a timer by the
    /// thread automatically.
    // (reference default: `= false`)
    pub cursor_blink_visible: bool,

    /// This is true when the inspector is active.
    // (reference default: `= false`)
    pub has_inspector: bool,

    /// This is true when the view is visible. This is used to determine
    /// if we should be rendering or not.
    // (reference default: `= true`)
    pub visible: bool,

    /// This is true when the view is focused. This defaults to true
    /// and it is up to the apprt to set the correct value.
    // (reference default: `= true`)
    pub focused: bool,
}

// PORT NOTE: MANUAL `Default` — the reference defaults are not all-false
// (`visible = true`, `focused = true`), so `#[derive(Default)]` would be
// unfaithful.
impl Default for Flags {
    fn default() -> Flags {
        Flags {
            cursor_blink_visible: false,
            has_inspector: false,
            visible: true,
            focused: true,
        }
    }
}

impl<A: apprt::App + 'static> Thread<A> {
    /// Initialize the thread. This does not START the thread. This only sets
    /// up all the internal state necessary prior to starting the thread. It
    /// is up to the caller to start the thread with the threadMain entrypoint.
    //
    // PORT NOTE: `pub fn init(alloc: Allocator, config: *const
    // configpkg.Config, surface: *apprt.Surface, renderer_impl:
    // *rendererpkg.Renderer, state: *rendererpkg.State, app_mailbox:
    // App.Mailbox) !Thread` (renderer/Thread.zig:125); callsite
    // Surface.zig:567. Error type: the xev handle-init failures (termio
    // `Thread::init` precedent; `Mailbox.create`'s alloc failure collapses
    // into Rust's infallible `Box`). The `alloc` param is kept on the
    // placeholder ([ADV-FIX 3] doctrine).
    pub fn init(
        alloc: Allocator,
        config: &rustty_config::Config,
        surface: *mut A::Surface,
        renderer_impl: *mut Renderer,
        state: *mut State,
        app_mailbox: crate::app::Mailbox<A>,
    ) -> Result<Thread<A>, xev::Error> {
        // PORT NOTE ([ADV-FIX 3]): the `alloc` param is kept on the placeholder
        // signature, but the reference `Mailbox.create(alloc)` collapses to
        // Rust `Box` ownership, so `alloc` is unused here.
        let _ = alloc;

        // PORT NOTE (errdefer parity, CONFIRMED): rustty-xev has NO `impl Drop`
        // for `Loop`/`Timer`/`Async` — every teardown is an explicit `deinit()`
        // syscall. So the reference `errdefer h.deinit()` ordering
        // (Thread.zig:135-163) does NOT happen for free on the `?` early return;
        // it is reproduced explicitly: on each `Err`, `deinit()` the
        // earlier-initialized handles in LIFO order, then return the error (the
        // surface.rs errdefer-chain precedent). Pre-spawn + single-threaded, so
        // no aliasing concern.

        // Create our event loop.
        // var loop = try xev.Loop.init(.{});
        // errdefer loop.deinit();
        let mut loop_ = xev::Loop::init(xev::Options::default())?;

        // This async handle is used to "wake up" the renderer and force a render.
        // var wakeup_h = try xev.Async.init();
        // errdefer wakeup_h.deinit();
        let mut wakeup_h = match xev::Async::init() {
            Ok(v) => v,
            Err(e) => {
                loop_.deinit();
                return Err(e);
            }
        };

        // This async handle is used to stop the loop and force the thread to end.
        // var stop_h = try xev.Async.init();
        // errdefer stop_h.deinit();
        let mut stop_h = match xev::Async::init() {
            Ok(v) => v,
            Err(e) => {
                wakeup_h.deinit();
                loop_.deinit();
                return Err(e);
            }
        };

        // The primary timer for rendering.
        // var render_h = try xev.Timer.init();
        // errdefer render_h.deinit();
        let render_h = match xev::Timer::init() {
            Ok(v) => v,
            Err(e) => {
                stop_h.deinit();
                wakeup_h.deinit();
                loop_.deinit();
                return Err(e);
            }
        };

        // Draw timer, see comments.
        // var draw_h = try xev.Timer.init();
        // errdefer draw_h.deinit();
        let draw_h = match xev::Timer::init() {
            Ok(v) => v,
            Err(e) => {
                render_h.deinit();
                stop_h.deinit();
                wakeup_h.deinit();
                loop_.deinit();
                return Err(e);
            }
        };

        // Draw now async, see comments.
        // var draw_now = try xev.Async.init();
        // errdefer draw_now.deinit();
        let mut draw_now = match xev::Async::init() {
            Ok(v) => v,
            Err(e) => {
                draw_h.deinit();
                render_h.deinit();
                stop_h.deinit();
                wakeup_h.deinit();
                loop_.deinit();
                return Err(e);
            }
        };

        // Setup a timer for blinking the cursor
        // var cursor_timer = try xev.Timer.init();
        // errdefer cursor_timer.deinit();
        let cursor_timer = match xev::Timer::init() {
            Ok(v) => v,
            Err(e) => {
                draw_now.deinit();
                draw_h.deinit();
                render_h.deinit();
                stop_h.deinit();
                wakeup_h.deinit();
                loop_.deinit();
                return Err(e);
            }
        };

        // The mailbox for messaging this thread
        // var mailbox = try Mailbox.create(alloc);
        // errdefer mailbox.destroy(alloc);
        //
        // (PORT NOTE: `Mailbox.create(alloc)` -> the REAL infallible
        // `BlockingQueue::create()` returning `Box<Self>`; `Box::into_raw` for
        // the `*mut Mailbox` field. No `try`/errdefer — infallible.)
        let mailbox: *mut thread::Mailbox = Box::into_raw(thread::Mailbox::create());

        // return .{ ... };
        Ok(Thread {
            // .alloc = alloc,  (collapsed — Box ownership)
            // .config = .init(config),
            //
            // (PORT NOTE: `rustty_config::Config` impls the renderer-defined
            // `RendererConfig` accessor, so the thread `DerivedConfig::init` runs
            // with `C = rustty_config::Config`.)
            config: thread::DerivedConfig::init(config),
            // .loop = loop,
            loop_,
            // .wakeup = wakeup_h,
            wakeup: wakeup_h,
            wakeup_c: xev::Completion::default(),
            // .stop = stop_h,
            stop: stop_h,
            stop_c: xev::Completion::default(),
            // .render_h = render_h,
            render_h,
            render_c: xev::Completion::default(),
            // .draw_h = draw_h,
            draw_h,
            draw_c: xev::Completion::default(),
            draw_active: false,
            // .draw_now = draw_now,
            draw_now,
            draw_now_c: xev::Completion::default(),
            // .cursor_h = cursor_timer,
            cursor_h: cursor_timer,
            cursor_c: xev::Completion::default(),
            cursor_c_cancel: xev::Completion::default(),
            // .surface = surface,
            surface,
            // .renderer = renderer_impl,
            renderer: renderer_impl,
            // .state = state,
            state,
            // .mailbox = mailbox,
            mailbox,
            // .app_mailbox = app_mailbox,
            app_mailbox,
            flags: Flags::default(),
        })
    }

    /// Clean up the thread. This is only safe to call once the thread
    /// completes executing; the caller must join prior to this.
    //
    // PORT NOTE: `pub fn deinit(self: *Thread) void` -> `&mut self` (NOT
    // consuming): Surface.zig calls it on the still-owned field
    // (`self.renderer_thread.deinit()`, Surface.zig:813 + the :575 errdefer).
    pub fn deinit(&mut self) {
        // self.stop.deinit();
        self.stop.deinit();
        // self.wakeup.deinit();
        self.wakeup.deinit();
        // self.render_h.deinit();
        self.render_h.deinit();
        // self.draw_h.deinit();
        self.draw_h.deinit();
        // self.draw_now.deinit();
        self.draw_now.deinit();
        // self.cursor_h.deinit();
        self.cursor_h.deinit();
        // self.loop.deinit();
        self.loop_.deinit();

        // Nothing can possibly access the mailbox anymore, destroy it.
        // self.mailbox.destroy(self.alloc);
        //
        // (PORT NOTE: `Mailbox.destroy(alloc)` -> `BlockingQueue::destroy(Box<Self>)`;
        // reconstitute the owning `Box` from the raw pointer. `deinit` runs
        // post-join — the child is dead, no aliasing.)
        unsafe { thread::Mailbox::destroy(Box::from_raw(self.mailbox)) };
    }
}

// PORT NOTE (W3 E2b, THE ALIASING DISCIPLINE): after the spawn
// (surface.rs) the child thread holds `*mut Thread<A>` for its whole life; the
// xev loop hands THAT SAME raw pointer back into every callback as `userdata:
// *mut Thread<A>`. So the faithful port is raw-self throughout — `threadMain`
// takes `self_: *mut Thread<A>` (NOT `&mut self`); there is NO
// `let t = &mut *self_;` whole-struct reborrow anywhere. Every callback is a
// SAFE fn (raw-ptr derefs in explicit `unsafe` blocks — an `unsafe fn` item
// does NOT impl `Fn`, so xev's ZST-callback bound forces safe fns; the termio
// io_thread precedent) taking raw `self_` with only narrow per-field
// projections `(*self_).<field>`. xev registration args stay raw: the loop
// re-enters the completion `c` across ticks, so a `&mut` re-read would be UB.
// This shape is proven on the mirihar-chunk3b `renderthread_shipped` (CLEAN
// under SB + TB) vs `renderthread_naive` (UB) harness.
//
// The `where A::Surface: apprt::Surface<Core = Surface<A>>` bound gives
// `threadMain_` the typed `rt_surface.core()` edge (the crash-metadata target);
// every callsite (Surface::init's spawn, in surface.rs) is inside a Surface
// impl block carrying the same bound.
impl<A: apprt::App + 'static> Thread<A>
where
    A::Surface: apprt::Surface<App = A, Core = crate::surface::Surface<A>>,
{
    /// The main entrypoint for the thread.
    //
    // PORT NOTE (E2b: RAW self, was `&mut self`): `pub fn threadMain(self:
    // *Thread) void` (Thread.zig:199). NO whole reborrow — see the impl-block
    // aliasing note above.
    ///
    /// # Safety
    ///
    /// `self_` must point at a fully-initialized, frame-pinned `Thread<A>` that
    /// the child thread is the sole user of until the `deinit` join (the
    /// `RendererThreadArg` contract, surface.rs).
    pub unsafe fn threadMain(self_: *mut Thread<A>) {
        // Call child function so we can use errors...
        // self.threadMain_() catch |err| {
        //     log.warn("error in renderer err={}", .{err});
        // };
        if let Err(_err) = unsafe { Self::threadMain_(self_) } {
            let _ = &_err;
            // log.warn("error in renderer err={}", .{err});
        }
    }

    // fn threadMain_(self: *Thread) !void {
    unsafe fn threadMain_(self_: *mut Thread<A>) -> Result<(), ThreadMainError> {
        // defer log.debug("renderer thread exited", .{});

        // Right now, on Darwin, `std.Thread.setName` can only name the current
        // thread, and we have no way to get the current thread from within it,
        // so instead we use this code to name the thread instead.
        // if (builtin.os.tag.isDarwin()) {
        //     internal_os.macos.pthread_setname_np(&"renderer".*);
        // }
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        unsafe {
            internal_os::macos::pthread_setname_np(c"renderer".as_ptr());
        }

        // Setup our crash metadata
        // crash.sentry.thread_state = .{
        //     .type = .renderer,
        //     .surface = self.renderer.surface_mailbox.surface,
        // };
        // defer crash.sentry.thread_state = null;
        //
        // (PORT NOTE: the real Renderer dropped `surface_mailbox` (the surface-mailbox
        // seam), so the `ThreadStateSurface` target — the core `Surface<A>` — is
        // obtained from the Thread's apprt surface via `.core()`. Core's
        // `Surface<A>` impls `crash::sentry::ThreadStateSurface`.)
        let core_surface: *mut crate::surface::Surface<A> =
            unsafe { <A::Surface as apprt::Surface>::core(&*(*self_).surface) };
        crash::sentry::setThreadState(crash::sentry::ThreadState {
            type_: crash::sentry::Type::Renderer,
            surface: core_surface as *const crate::surface::Surface<A>,
        });
        let _thread_state = RendererThreadStateGuard;

        // Setup our thread QoS
        // self.setQosClass();
        Self::setQosClass(self_);

        // Run our loop start/end callbacks if the renderer cares.
        // const has_loop = @hasDecl(rendererpkg.Renderer, "loopEnter");
        // if (has_loop) try self.renderer.loopEnter(self);
        // defer if (has_loop) self.renderer.loopExit();
        //
        // (`@hasDecl(Renderer, "loopEnter")` is TRUE for Metal ⇒ the real
        // `loop_enter`. R4: the reference passes `self` and reads `&thr.draw_now`;
        // the real signature takes the Copy `draw_now: Async` directly.)
        unsafe { (*(*self_).renderer).0.loop_enter((*self_).draw_now) }?;
        let _loop_exit = LoopExitGuard(unsafe { (*self_).renderer });

        // Run our thread start/end callbacks. This is important because some
        // renderers have to do per-thread setup.
        // try self.renderer.threadEnter(self.surface);
        // defer self.renderer.threadExit();
        unsafe { (*(*self_).renderer).0.thread_enter((*self_).surface) }?;
        let _thread_exit = ThreadExitGuard(unsafe { (*self_).renderer });

        // Start the async handlers
        // self.wakeup.wait(&self.loop, &self.wakeup_c, Thread, self, wakeupCallback);
        // self.stop.wait(&self.loop, &self.stop_c, Thread, self, stopCallback);
        // self.draw_now.wait(&self.loop, &self.draw_now_c, Thread, self, drawNowCallback);
        unsafe {
            (*self_).wakeup.wait(
                &raw mut (*self_).loop_,
                &raw mut (*self_).wakeup_c,
                self_,
                Self::wakeupCallback,
            );
            (*self_).stop.wait(
                &raw mut (*self_).loop_,
                &raw mut (*self_).stop_c,
                self_,
                Self::stopCallback,
            );
            (*self_).draw_now.wait(
                &raw mut (*self_).loop_,
                &raw mut (*self_).draw_now_c,
                self_,
                Self::drawNowCallback,
            );
        }

        // Send an initial wakeup message so that we render right away.
        // try self.wakeup.notify();
        unsafe { (*self_).wakeup.notify() }?;

        // Start blinking the cursor.
        // self.cursor_h.run(&self.loop, &self.cursor_c, cursorBlinkInterval(), Thread, self, cursorTimerCallback);
        unsafe {
            (*self_).cursor_h.run(
                &raw mut (*self_).loop_,
                &raw mut (*self_).cursor_c,
                cursorBlinkInterval(),
                self_,
                Self::cursorTimerCallback,
            );
        }

        // Start the draw timer
        // self.syncDrawTimer();
        Self::syncDrawTimer(self_);

        // Run
        // log.debug("starting renderer thread", .{});
        // defer log.debug("starting renderer thread shutdown", .{});
        // _ = try self.loop.run(.until_done);
        unsafe { (*self_).loop_.run(xev::RunMode::UntilDone) }?;
        Ok(())
    }

    // fn setQosClass(self: *const Thread) void {
    fn setQosClass(self_: *mut Thread<A>) {
        // Thread QoS classes are only relevant on macOS.
        // if (comptime !builtin.target.os.tag.isDarwin()) return;
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        {
            let class: internal_os::macos::QosClass = {
                // If we aren't visible (our view is fully occluded) then we
                // always drop our rendering priority down because it's just
                // mostly wasted work.
                // if (!self.flags.visible) break :class .utility;
                if !unsafe { (*self_).flags.visible } {
                    internal_os::macos::QosClass::utility
                }
                // If we're not focused, but we're visible, then we set a higher
                // than default priority because framerates still matter.
                // if (!self.flags.focused) break :class .user_initiated;
                else if !unsafe { (*self_).flags.focused } {
                    internal_os::macos::QosClass::user_initiated
                }
                // We are focused and visible, we are the definition of user interactive.
                // break :class .user_interactive;
                else {
                    internal_os::macos::QosClass::user_interactive
                }
            };

            // if (internal_os.macos.setQosClass(class)) { log.debug } else |err| { log.warn }
            match internal_os::macos::setQosClass(class) {
                Ok(()) => { /* log.debug("thread QoS class set class={}", .{class}); */ }
                Err(_err) => { /* log.warn("error setting QoS class err={}", .{err}); */ }
            }
        }
        #[cfg(not(any(target_os = "macos", target_os = "ios")))]
        {
            let _ = self_;
        }
    }

    // fn syncDrawTimer(self: *Thread) void {
    fn syncDrawTimer(self_: *mut Thread<A>) {
        // skip: {
        'skip: {
            // If our renderer supports animations and has them, then we can apply
            // draw timer based on custom shader animation configuration.
            // if (@hasDecl(rendererpkg.Renderer, "hasAnimations") and
            //     self.renderer.hasAnimations())  (@hasDecl TRUE for Metal)
            if unsafe { (*(*self_).renderer).0.has_animations() } {
                // switch (self.config.custom_shader_animation) {
                match unsafe { (*self_).config.custom_shader_animation } {
                    // .always => break :skip,
                    thread::CustomShaderAnimation::Always => break 'skip,
                    // .true => if (self.flags.focused) break :skip,
                    thread::CustomShaderAnimation::True => {
                        if unsafe { (*self_).flags.focused } {
                            break 'skip;
                        }
                    }
                    // .false => {},
                    thread::CustomShaderAnimation::False => {}
                }
            }

            // We're skipping the draw timer. Stop it on the next iteration.
            // self.draw_active = false;
            // return;
            unsafe { (*self_).draw_active = false };
            return;
        }

        // Set our active state so it knows we're running. We set this before
        // even checking the active state in case we have a pending shutdown.
        // self.draw_active = true;
        unsafe { (*self_).draw_active = true };

        // If our draw timer is already active, then we don't have to do anything.
        // if (self.draw_c.state() == .active) return;
        if xev::Completion::state_of(unsafe { &(*self_).draw_c }) == xev::CompletionState::Active {
            return;
        }

        // Start the timer which loops
        // self.draw_h.run(&self.loop, &self.draw_c, DRAW_INTERVAL, Thread, self, drawCallback);
        unsafe {
            (*self_).draw_h.run(
                &raw mut (*self_).loop_,
                &raw mut (*self_).draw_c,
                DRAW_INTERVAL,
                self_,
                Self::drawCallback,
            );
        }
    }

    // /// Drain the mailbox.
    // fn drainMailbox(self: *Thread) !void {
    fn drainMailbox(self_: *mut Thread<A>) -> Result<(), RendererError> {
        // There's probably a more elegant way to do this...
        //
        // This is effectively an @autoreleasepool{} block, which we need in
        // order to ensure that autoreleased objects are properly released.
        // const pool = if (builtin.os.tag.isDarwin())
        //     @import("objc").AutoreleasePool.init()
        // else
        //     void;
        // defer if (builtin.os.tag.isDarwin()) pool.deinit();
        //
        // DEFERRED (W3 renderer-Thread leaf; .port/phase-2/DEFERRALS.md): rustty-core
        // does NOT dep objc2 (only rustty-renderer/os do), so this Darwin
        // memory-hygiene autoreleasepool (not semantics) is ported as a no-op
        // scope carrying the verbatim reference block above. Re-arm when core
        // gains an objc2 edge (or via a `rustty_os` autoreleasepool shim).

        // while (self.mailbox.pop()) |message| {
        while let Some(message) = unsafe { (*(*self_).mailbox).pop() } {
            // log.debug("mailbox message={}", .{message});
            // switch (message) {
            match message {
                // .crash => @panic("crash request, crashing intentionally"),
                Message::Crash => panic!("crash request, crashing intentionally"),

                // .visible => |v| visible: {
                Message::Visible(v) => 'visible: {
                    // If our state didn't change we do nothing.
                    // if (self.flags.visible == v) break :visible;
                    if unsafe { (*self_).flags.visible } == v {
                        break 'visible;
                    }

                    // Set our visible state
                    // self.flags.visible = v;
                    unsafe { (*self_).flags.visible = v };

                    // Visibility affects our QoS class
                    // self.setQosClass();
                    Self::setQosClass(self_);

                    // If we became visible then we immediately rebuild cells
                    // (renderCallback skips updateFrame while invisible) and draw.
                    // if (v) {
                    if v {
                        // self.renderer.updateFrame(self.state, self.flags.cursor_blink_visible)
                        //     catch |err| log.warn("error rendering on visibility regain err={}", .{err});
                        //
                        // (PORT NOTE: `update_frame` takes a SHARED `&State` — it mutates
                        // only through the stored `*mut Terminal` — so the `*mut
                        // State` is narrow-shared-deref'd; the error is the
                        // `VtOutOfMemory` type, logged not propagated.)
                        if let Err(_err) = unsafe {
                            (*(*self_).renderer)
                                .0
                                .update_frame(&*(*self_).state, (*self_).flags.cursor_blink_visible)
                        } {
                            // log.warn("error rendering on visibility regain err={}", .{err});
                        }
                        // self.drawFrame(false);
                        Self::drawFrame(self_, false);
                    }

                    // Notify the renderer so it can update any state.
                    // self.renderer.setVisible(v);
                    unsafe { (*(*self_).renderer).0.set_visible(v) };

                    // Note that we're explicitly today not stopping any cursor
                    // timers, draw timers, etc. ... check the visible state
                    // themselves to control their behavior.
                }

                // .focus => |v| focus: {
                Message::Focus(v) => 'focus: {
                    // If our state didn't change we do nothing.
                    // if (self.flags.focused == v) break :focus;
                    if unsafe { (*self_).flags.focused } == v {
                        break 'focus;
                    }

                    // Set our state
                    // self.flags.focused = v;
                    unsafe { (*self_).flags.focused = v };

                    // Focus affects our QoS class
                    // self.setQosClass();
                    Self::setQosClass(self_);

                    // Set it on the renderer
                    // try self.renderer.setFocus(v);
                    unsafe { (*(*self_).renderer).0.set_focus(v) }?;

                    // We always resync our draw timer (may disable it)
                    // self.syncDrawTimer();
                    Self::syncDrawTimer(self_);

                    // if (!v) {
                    if !v {
                        // If we're not focused, then we stop the cursor blink
                        // if (self.cursor_c.state() == .active and
                        //     self.cursor_c_cancel.state() == .dead)
                        if xev::Completion::state_of(unsafe { &(*self_).cursor_c })
                            == xev::CompletionState::Active
                            && xev::Completion::state_of(unsafe { &(*self_).cursor_c_cancel })
                                == xev::CompletionState::Dead
                        {
                            // self.cursor_h.cancel(&self.loop, &self.cursor_c,
                            //     &self.cursor_c_cancel, void, null, cursorCancelCallback);
                            unsafe {
                                (*self_).cursor_h.cancel(
                                    &raw mut (*self_).loop_,
                                    &raw mut (*self_).cursor_c,
                                    &raw mut (*self_).cursor_c_cancel,
                                    core::ptr::null_mut::<()>(),
                                    Self::cursorCancelCallback,
                                );
                            }
                        }
                    } else {
                        // If we're focused, we immediately show the cursor again
                        // and then restart the timer.
                        // if (self.cursor_c.state() != .active) {
                        if xev::Completion::state_of(unsafe { &(*self_).cursor_c })
                            != xev::CompletionState::Active
                        {
                            // self.flags.cursor_blink_visible = true;
                            unsafe { (*self_).flags.cursor_blink_visible = true };
                            // self.cursor_h.run(&self.loop, &self.cursor_c,
                            //     cursorBlinkInterval(), Thread, self, cursorTimerCallback);
                            unsafe {
                                (*self_).cursor_h.run(
                                    &raw mut (*self_).loop_,
                                    &raw mut (*self_).cursor_c,
                                    cursorBlinkInterval(),
                                    self_,
                                    Self::cursorTimerCallback,
                                );
                            }
                        }
                    }
                }

                // .reset_cursor_blink => {
                Message::ResetCursorBlink => {
                    // self.flags.cursor_blink_visible = true;
                    unsafe { (*self_).flags.cursor_blink_visible = true };
                    // if (self.cursor_c.state() == .active) {
                    if xev::Completion::state_of(unsafe { &(*self_).cursor_c })
                        == xev::CompletionState::Active
                    {
                        // self.cursor_h.reset(&self.loop, &self.cursor_c,
                        //     &self.cursor_c_cancel, cursorBlinkInterval(), Thread, self,
                        //     cursorTimerCallback);
                        unsafe {
                            (*self_).cursor_h.reset(
                                &raw mut (*self_).loop_,
                                &raw mut (*self_).cursor_c,
                                &raw mut (*self_).cursor_c_cancel,
                                cursorBlinkInterval(),
                                self_,
                                Self::cursorTimerCallback,
                            );
                        }
                    }
                }

                // .font_grid => |grid| {
                Message::FontGrid {
                    grid,
                    set,
                    new_key,
                    old_key,
                } => {
                    // self.renderer.setFontGrid(grid.grid);
                    unsafe { (*(*self_).renderer).0.set_font_grid(grid) };
                    // grid.set.deref(grid.old_key);
                    unsafe { (*set).deref(old_key) };
                    // (PORT NOTE: `new_key` is an OWNED `Key` copy the reference discards
                    // here; drop it to free its descriptors and avoid a leak — the
                    // surface.rs `font_grid_key` owned-copy precedent.)
                    drop(new_key);
                }

                // .resize => |v| self.renderer.setScreenSize(v),
                Message::Resize(v) => unsafe { (*(*self_).renderer).0.set_screen_size(v) },

                // .change_config => |config| {
                Message::ChangeConfig { thread, r#impl } => {
                    // defer config.alloc.destroy(config.thread);
                    // defer config.alloc.destroy(config.impl);
                    // (PORT NOTE: both `alloc.destroy`s collapse into the `Box` drops at
                    // the end of this arm — the `Message::ChangeConfig` owns them.)
                    // try self.changeConfig(config.thread);
                    Self::changeConfig(self_, *thread);
                    // try self.renderer.changeConfig(config.impl);
                    unsafe { (*(*self_).renderer).0.change_config(*r#impl) }?;

                    // Stop and start the draw timer to capture the new
                    // hasAnimations value.
                    // self.syncDrawTimer();
                    Self::syncDrawTimer(self_);
                }

                // .search_viewport_matches => |v| {
                Message::SearchViewportMatches(v) => {
                    // Note we don't free the new value because we expect our
                    // allocators to match.
                    // if (self.renderer.search_matches) |*m| m.arena.deinit();
                    // (PORT NOTE: the `arena.deinit()` collapses into the drop of the old
                    // `Option`'s owned `Vec` when the field is reassigned below.)
                    // self.renderer.search_matches = v;
                    unsafe { (*(*self_).renderer).0.search_matches = Some(v) };
                    // self.renderer.search_matches_dirty = true;
                    unsafe { (*(*self_).renderer).0.search_matches_dirty = true };
                }

                // .search_selected_match => |v| {
                Message::SearchSelectedMatch(v) => {
                    // if (self.renderer.search_selected_match) |*m| m.arena.deinit();
                    // (same arena collapse as above.)
                    // self.renderer.search_selected_match = v;
                    unsafe { (*(*self_).renderer).0.search_selected_match = v };
                    // self.renderer.search_matches_dirty = true;
                    unsafe { (*(*self_).renderer).0.search_matches_dirty = true };
                }

                // .inspector => |v| self.flags.has_inspector = v,
                Message::Inspector(v) => unsafe { (*self_).flags.has_inspector = v },

                // .macos_display_id => |v| {
                //     if (@hasDecl(rendererpkg.Renderer, "setMacOSDisplayID")) {
                //         try self.renderer.setMacOSDisplayID(v);
                //     }
                // },  (@hasDecl TRUE for Metal)
                Message::MacosDisplayId(v) => {
                    unsafe { (*(*self_).renderer).0.set_macos_display_id(v) }?;
                }
            }
        }

        Ok(())
    }

    // fn changeConfig(self: *Thread, config: *const DerivedConfig) !void {
    //
    // (PORT NOTE: the reference takes a `*const DerivedConfig` and copies `config.*`;
    // the port takes the value BY MOVE — the caller owns the `Box<DerivedConfig>`
    // and unboxes it. The `!void` is infallible.)
    fn changeConfig(self_: *mut Thread<A>, config: thread::DerivedConfig) {
        // self.config = config.*;
        unsafe { (*self_).config = config };
    }

    /// Trigger a draw. This will not update frame data or anything, it will
    /// just trigger a draw/paint.
    // fn drawFrame(self: *Thread, now: bool) void {
    fn drawFrame(self_: *mut Thread<A>, now: bool) {
        // If we're invisible, we do not draw.
        // if (!self.flags.visible) return;
        if !unsafe { (*self_).flags.visible } {
            return;
        }

        // If the renderer is managing a vsync on its own, we only draw
        // when we're forced to via `now`.
        // if (!now and self.renderer.hasVsync()) return;
        if !now && unsafe { (*(*self_).renderer).0.has_vsync() } {
            return;
        }

        // if (must_draw_from_app_thread) {
        if MUST_DRAW_FROM_APP_THREAD {
            // _ = self.app_mailbox.push(.{ .redraw_surface = self.surface }, .{ .instant = {} });
            let _ = unsafe {
                (*self_).app_mailbox.push(
                    crate::app::Message::RedrawSurface((*self_).surface),
                    rustty_datastruct::blocking_queue::Timeout::Instant,
                )
            };
        } else {
            // self.renderer.drawFrame(false) catch |err| log.warn("error drawing err={}", .{err});
            // W5 (`W5_DESIGN §2.5` caller 1): RAW-self draw — the render thread must
            // NOT establish a protected whole-`Renderer` `&mut` (it would race the
            // main-thread display callback's `draw_frame_raw` under `draw_mutex`; the
            // mutex cannot rescue a protector established before the lock).
            // `&raw mut (*(*self_).renderer).0` is a raw place projection off the
            // already-raw `renderer: *mut Renderer` — no reborrow, no protector.
            let rp: *mut rustty_renderer::Renderer = unsafe { &raw mut (*(*self_).renderer).0 };
            if let Err(_err) = unsafe { rustty_renderer::Renderer::draw_frame_raw(rp, false) } {
                // log.warn("error drawing err={}", .{err});
            }
        }
    }

    // fn wakeupCallback(self_: ?*Thread, _: *xev.Loop, _: *xev.Completion,
    //     r: xev.Async.WaitError!void) xev.CallbackAction {
    fn wakeupCallback(
        self_: *mut Thread<A>,
        _l: *mut xev::Loop,
        _c: *mut xev::Completion,
        r: Result<(), xev::WaitError>,
    ) -> xev::CallbackAction {
        // _ = r catch |err| { log.err("error in wakeup err={}", .{err}); return .rearm; };
        if r.is_err() {
            // log.err("error in wakeup err={}", .{err});
            return xev::CallbackAction::Rearm;
        }

        // const t = self_.?;  (non-null by construction)

        // When we wake up, we check the mailbox.
        // t.drainMailbox() catch |err| log.err("error draining mailbox err={}", .{err});
        if let Err(_err) = Self::drainMailbox(self_) {
            // log.err("error draining mailbox err={}", .{err});
        }

        // Render immediately
        // _ = renderCallback(t, undefined, undefined, {});
        let _ = Self::renderCallback(self_, core::ptr::null_mut(), core::ptr::null_mut(), Ok(()));

        // The below is not used anymore but if we ever want to introduce a
        // configuration to introduce a delay to coalesce renders, we can use this.
        //
        // // If the timer is already active then we don't have to do anything.
        // if (t.render_c.state() == .active) return .rearm;
        //
        // // Timer is not active, let's start it
        // t.render_h.run(&t.loop, &t.render_c, 10, Thread, t, renderCallback);

        // return .rearm;
        xev::CallbackAction::Rearm
    }

    // fn drawNowCallback(self_: ?*Thread, _: *xev.Loop, _: *xev.Completion,
    //     r: xev.Async.WaitError!void) xev.CallbackAction {
    fn drawNowCallback(
        self_: *mut Thread<A>,
        _l: *mut xev::Loop,
        _c: *mut xev::Completion,
        r: Result<(), xev::WaitError>,
    ) -> xev::CallbackAction {
        // _ = r catch |err| { log.err("error in draw now err={}", .{err}); return .rearm; };
        if r.is_err() {
            // log.err("error in draw now err={}", .{err});
            return xev::CallbackAction::Rearm;
        }

        // Draw immediately
        // const t = self_.?;
        // t.drawFrame(true);
        Self::drawFrame(self_, true);

        // return .rearm;
        xev::CallbackAction::Rearm
    }

    // fn drawCallback(self_: ?*Thread, _: *xev.Loop, _: *xev.Completion,
    //     r: xev.Timer.RunError!void) xev.CallbackAction {
    fn drawCallback(
        self_: *mut Thread<A>,
        _l: *mut xev::Loop,
        _c: *mut xev::Completion,
        r: Result<(), xev::TimerRunError>,
    ) -> xev::CallbackAction {
        // _ = r catch unreachable;
        if r.is_err() {
            unreachable!()
        }
        // const t: *Thread = self_ orelse { log.warn(...); return .disarm; };
        // (non-null by construction)

        // Draw
        // t.drawFrame(false);
        Self::drawFrame(self_, false);

        // Only continue if we're still active
        // if (t.draw_active) {
        //     t.draw_h.run(&t.loop, &t.draw_c, DRAW_INTERVAL, Thread, t, drawCallback);
        // }
        if unsafe { (*self_).draw_active } {
            unsafe {
                (*self_).draw_h.run(
                    &raw mut (*self_).loop_,
                    &raw mut (*self_).draw_c,
                    DRAW_INTERVAL,
                    self_,
                    Self::drawCallback,
                );
            }
        }

        // return .disarm;
        xev::CallbackAction::Disarm
    }

    // fn renderCallback(self_: ?*Thread, _: *xev.Loop, _: *xev.Completion,
    //     r: xev.Timer.RunError!void) xev.CallbackAction {
    fn renderCallback(
        self_: *mut Thread<A>,
        _l: *mut xev::Loop,
        _c: *mut xev::Completion,
        r: Result<(), xev::TimerRunError>,
    ) -> xev::CallbackAction {
        // _ = r catch unreachable;
        if r.is_err() {
            unreachable!()
        }
        // const t: *Thread = self_ orelse { log.warn(...); return .disarm; };

        // If we're not visible there's no point spending CPU rebuilding cells.
        // if (!t.flags.visible) return .disarm;
        if !unsafe { (*self_).flags.visible } {
            return xev::CallbackAction::Disarm;
        }

        // Update our frame data
        // t.renderer.updateFrame(t.state, t.flags.cursor_blink_visible)
        //     catch |err| log.warn("error rendering err={}", .{err});
        //
        // (PORT NOTE: `update_frame` takes a SHARED `&State`; the error is
        // `VtOutOfMemory`, logged not propagated.)
        if let Err(_err) = unsafe {
            (*(*self_).renderer)
                .0
                .update_frame(&*(*self_).state, (*self_).flags.cursor_blink_visible)
        } {
            // log.warn("error rendering err={}", .{err});
        }

        // Draw
        // t.drawFrame(false);
        Self::drawFrame(self_, false);

        // return .disarm;
        xev::CallbackAction::Disarm
    }

    // fn cursorTimerCallback(self_: ?*Thread, _: *xev.Loop, _: *xev.Completion,
    //     r: xev.Timer.RunError!void) xev.CallbackAction {
    fn cursorTimerCallback(
        self_: *mut Thread<A>,
        _l: *mut xev::Loop,
        _c: *mut xev::Completion,
        r: Result<(), xev::TimerRunError>,
    ) -> xev::CallbackAction {
        // _ = r catch |err| switch (err) {
        //     error.Canceled => return .disarm,
        //     else => { log.warn(...); unreachable; },
        // };
        if let Err(err) = r {
            match err {
                // This is sent when our timer is canceled. That's fine.
                xev::TimerRunError::Canceled => return xev::CallbackAction::Disarm,
                _ => {
                    // log.warn("error in cursor timer callback err={}", .{err});
                    unreachable!()
                }
            }
        }

        // const t: *Thread = self_ orelse { log.warn(...); return .disarm; };

        // t.flags.cursor_blink_visible = !t.flags.cursor_blink_visible;
        unsafe { (*self_).flags.cursor_blink_visible = !(*self_).flags.cursor_blink_visible };
        // t.wakeup.notify() catch {};
        let _ = unsafe { (*self_).wakeup.notify() };

        // t.cursor_h.run(&t.loop, &t.cursor_c, cursorBlinkInterval(), Thread, t, cursorTimerCallback);
        unsafe {
            (*self_).cursor_h.run(
                &raw mut (*self_).loop_,
                &raw mut (*self_).cursor_c,
                cursorBlinkInterval(),
                self_,
                Self::cursorTimerCallback,
            );
        }
        // return .disarm;
        xev::CallbackAction::Disarm
    }

    // fn cursorCancelCallback(_: ?*void, _: *xev.Loop, _: *xev.Completion,
    //     r: xev.Timer.CancelError!void) xev.CallbackAction {
    //
    // (PORT NOTE: the userdata is `?*void` — a `*mut ()` here.)
    fn cursorCancelCallback(
        _ud: *mut (),
        _l: *mut xev::Loop,
        _c: *mut xev::Completion,
        r: Result<(), xev::CancelError>,
    ) -> xev::CallbackAction {
        // This makes it easier to work across platforms where different platforms
        // support different sets of errors, so we just unify it.
        // const CancelError = xev.Timer.CancelError || error{ Canceled, NotFound, Unexpected };
        // _ = r catch |err| switch (@as(CancelError, @errorCast(err))) {
        //     error.Canceled => {}, // success
        //     error.NotFound => {}, // completed before it could cancel
        //     else => { log.warn(...); unreachable; },
        // };
        //
        // (PORT NOTE: Rustty's `CancelError` set is `error{Canceled}` — its sole
        // variant is the success case; the NotFound/unreachable arms above have
        // no counterpart yet and are preserved verbatim in the comment.)
        if let Err(_err) = r {
            // error.Canceled => {} (success)
        }

        // return .disarm;
        xev::CallbackAction::Disarm
    }

    // fn stopCallback(self_: ?*Thread, _: *xev.Loop, _: *xev.Completion,
    //     r: xev.Async.WaitError!void) xev.CallbackAction {
    fn stopCallback(
        self_: *mut Thread<A>,
        _l: *mut xev::Loop,
        _c: *mut xev::Completion,
        r: Result<(), xev::WaitError>,
    ) -> xev::CallbackAction {
        // _ = r catch unreachable;
        if r.is_err() {
            unreachable!()
        }
        // self_.?.loop.stop();
        unsafe { (*self_).loop_.stop() };
        // return .disarm;
        xev::CallbackAction::Disarm
    }
}

// Returns the interval for the blinking cursor in milliseconds.
// fn cursorBlinkInterval() u64 {
fn cursorBlinkInterval() -> u64 {
    // if (std.valgrind.runningOnValgrind() > 0) {
    //     // If we're running under Valgrind, the cursor blink adds enough
    //     // churn that it makes some stalls annoying unless you're on a
    //     // super powerful computer, so we delay it.
    //     //
    //     // This is a hack, we should change some of our cursor timer
    //     // logic to be more efficient:
    //     // https://github.com/ghostty-org/ghostty/issues/8003
    //     return CURSOR_BLINK_INTERVAL * 5;
    // }
    //
    // PORT NOTE: Rustty has no `std.valgrind.runningOnValgrind` shim, so the
    // valgrind `* 5` branch above is inert (never taken) and kept as a verbatim
    // comment; the fn returns the plain constant. Re-arm the branch if a
    // `rustty_os` valgrind probe is added.

    // return CURSOR_BLINK_INTERVAL;
    CURSOR_BLINK_INTERVAL
}
