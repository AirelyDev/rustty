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
//! (this placeholder `Thread<A>` points at both).
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
use rustty_xev as xev;

use crate::surface::SurfaceMailbox;

/// The error type of the placeholder [`Renderer`] entry points.
//
// PORT NOTE: the reference error sets are INFERRED error unions (`!void` /
// `!Self`, from the concrete GraphicsAPI + shader/font failures). Opaque
// marker until Phase-3 R-d fills the renderer — the apprt-api `ApprtError`
// growth-point precedent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RendererError;

impl core::fmt::Display for RendererError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "renderer placeholder (Phase-3 R-d)")
    }
}

impl std::error::Error for RendererError {}

/// The configuration for this renderer that is derived from the main
/// configuration. This must be exported so that we don't need to
/// pass around Config pointers which makes memory management a pain.
//
// PORT NOTE (DEPENDENCY PLACEHOLDER): `renderer.Renderer.DerivedConfig`
// (generic.zig:536) — the arena + the ~35 config/font/link-derived fields
// type against the unported GPU/link/shader surface and land with Phase-3
// R-d. Inhabited field-less struct with `unimplemented!()` methods
// ([ADV-FIX 2] doctrine). NOT to be confused with the renderer THREAD's own
// `rustty_renderer::thread::DerivedConfig` (`custom_shader_animation`), which
// `Thread<A>.config` below embeds.
pub struct DerivedConfig {
    // (generic.zig:536 fields — arena, font_thicken.., cursor_.., links,
    // custom_shaders, bg_image.., vsync/colorspace/blending.. — deferred WITH
    // the Phase-3 R-d renderer they configure.)
}

impl DerivedConfig {
    // PORT NOTE: reference `pub fn init(alloc_gpa: Allocator, config: *const
    // configpkg.Config) !DerivedConfig` (generic.zig:574). Core CAN name
    // `rustty_config::Config` directly (no G7 cycle from up here), so the
    // placeholder keeps the concrete signature. Callsite: Surface.zig:552
    // `.config = try .init(alloc, config)`.
    pub fn init(
        alloc_gpa: Allocator,
        config: &rustty_config::Config,
    ) -> Result<DerivedConfig, RendererError> {
        let _ = (alloc_gpa, config);
        unimplemented!("renderer.Renderer.DerivedConfig placeholder (Phase-3 R-d)")
    }

    // PORT NOTE: `pub fn deinit(self: *DerivedConfig) void` (generic.zig:651).
    pub fn deinit(&mut self) {
        unimplemented!("renderer.Renderer.DerivedConfig placeholder (Phase-3 R-d)")
    }
}

/// The renderer implementation.
//
// PORT NOTE (DEPENDENCY PLACEHOLDER, CORE_BLUEPRINT §5): the reference is
// comptime-chosen — `pub const Renderer = switch (build_config.renderer) {
// .metal => GenericRenderer(Metal), .opengl => GenericRenderer(OpenGL), ... }`
// (renderer.zig:38). Phase-3 R-d. Inhabited unit struct; the methods below
// carry the §5 signatures with `unimplemented!()` bodies — dependency
// placeholders, no reference logic stubbed. Surface.zig embeds it by value
// (`renderer: renderer.Renderer`).
pub struct Renderer;

impl Renderer {
    /// This is called early right after surface creation.
    //
    // PORT NOTE: `pub fn surfaceInit(surface: *apprt.Surface) !void`
    // (generic.zig:864, a type-level fn — no self); callsite Surface.zig:493
    // `try Renderer.surfaceInit(rt_surface);`. Generic `<S>` per the
    // blueprint §5 signature: the placeholder holds the pointer without
    // needing an `App` bound.
    pub fn surfaceInit<S>(rt_surface: *mut S) -> Result<(), RendererError> {
        let _ = rt_surface;
        unimplemented!("renderer.Renderer placeholder (Phase-3 R-d)")
    }

    // PORT NOTE ([ADV-FIX 3]): the reference init takes `(alloc, options)` —
    // `pub fn init(alloc: Allocator, options: renderer.Options) !Self`
    // (generic.zig:658) — so the alloc STAYS on the placeholder. Callsite
    // Surface.zig:551.
    pub fn init<A: apprt::App + 'static>(
        alloc: Allocator,
        options: Options<A>,
    ) -> Result<Renderer, RendererError> {
        let _ = (alloc, options);
        unimplemented!("renderer.Renderer placeholder (Phase-3 R-d)")
    }

    /// This is called just prior to spinning up the renderer thread for
    /// final main thread setup requirements.
    //
    // PORT NOTE: `pub fn finalizeSurfaceInit(self: *Self, surface:
    // *apprt.Surface) !void` (generic.zig:873); callsite Surface.zig:712.
    pub fn finalizeSurfaceInit<S>(&mut self, rt_surface: *mut S) -> Result<(), RendererError> {
        let _ = rt_surface;
        unimplemented!("renderer.Renderer placeholder (Phase-3 R-d)")
    }

    /// Callback called by renderer.Thread when it begins.
    //
    // PORT NOTE: `pub fn threadEnter(self: *const Self, surface:
    // *apprt.Surface) !void` (generic.zig:881); core reaches it on the
    // surface deinit path (Surface.zig:801
    // `self.renderer.threadEnter(self.rt_surface) catch unreachable;`).
    pub fn threadEnter<S>(&self, rt_surface: *mut S) -> Result<(), RendererError> {
        let _ = rt_surface;
        unimplemented!("renderer.Renderer placeholder (Phase-3 R-d)")
    }

    /// Draw the frame to the screen.
    ///
    /// If `sync` is true, this will synchronously block until
    /// the frame is finished drawing and has been presented.
    //
    // PORT NOTE: `pub fn drawFrame(self: *Self, sync: bool) !void`
    // (generic.zig:1430); callsite Surface.zig:884 (`draw` — "Renderers are
    // required to support `drawFrame` being called from the main thread").
    pub fn drawFrame(&mut self, sync: bool) -> Result<(), RendererError> {
        let _ = sync;
        unimplemented!("renderer.Renderer placeholder (Phase-3 R-d)")
    }

    // PORT NOTE: `pub fn deinit(self: *Self) void` (generic.zig:800).
    pub fn deinit(&mut self) {
        unimplemented!("renderer.Renderer placeholder (Phase-3 R-d)")
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
    // PORT NOTE: `config: renderer.Renderer.DerivedConfig` — the placeholder
    // `DerivedConfig` above.
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

/// PORT: the reference's inline `flags: packed struct { ... } = .{}`
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
        let _ = (alloc, config, surface, renderer_impl, state, app_mailbox);
        unimplemented!("renderer.Thread placeholder (Phase-3 R-d + D6 xev)")
    }

    /// Clean up the thread. This is only safe to call once the thread
    /// completes executing; the caller must join prior to this.
    //
    // PORT NOTE: `pub fn deinit(self: *Thread) void` -> `&mut self` (NOT
    // consuming): Surface.zig calls it on the still-owned field
    // (`self.renderer_thread.deinit()`, Surface.zig:813 + the :575 errdefer).
    pub fn deinit(&mut self) {
        unimplemented!("renderer.Thread placeholder (Phase-3 R-d + D6 xev)")
    }

    /// The main entrypoint for the thread.
    //
    // PORT NOTE: `pub fn threadMain(self: *Thread) void`
    // (renderer/Thread.zig:199). The run-loop body (`threadMain_` and the
    // callbacks) lands WITH the Phase-3/D6 port — see the module doc.
    pub fn threadMain(&mut self) {
        unimplemented!("renderer.Thread placeholder (Phase-3 R-d + D6 xev)")
    }
}
