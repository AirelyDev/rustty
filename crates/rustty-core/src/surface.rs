//! Surface represents a single terminal "surface". A terminal surface is
//! a minimal "widget" where the terminal is drawn and responds to events
//! such as keyboard and mouse. Each surface also creates and owns its pty
//! session.
//!
//! The word "surface" is used because it is left to the higher level
//! application runtime to determine if the surface is a window, a tab,
//! a split, a preview pane in a larger window, etc. This struct doesn't care:
//! it just draws and responds to events. The events come from the application
//! runtime so the runtime can determine when and how those are delivered
//! (i.e. with focus, without focus, and so on).
//!
//! PORTS: ghostty src/Surface.zig (+ the `apprt.surface.Mailbox` struct from
//! src/apprt/surface.zig:135, which CORE_BLUEPRINT §1 pins HERE — it names
//! the core `Surface`, so it could not live in rustty-apprt-api; apprt-api
//! holds only the `AppMailbox` trait it implements).
//!
//! PORT STATUS: in progress (core chunk C5 landed Surface.zig:1-456 — the
//! full `Surface<A>` field set, `InputEffect`/`Search`/`Mouse`/`Keyboard`,
//! `DerivedConfig` (+`scaledPadding`) under the field-parity audit, and the
//! `SurfaceMailbox<A>` push/`AppMailbox` impl; chunk C6 landed Surface.zig:
//! 457-957 — `init`/`deinit`/`close`/`surfaceMailbox`/`queueIo`/`draw`/
//! `needsConfirmQuit` LIVE against the placeholder leaves (GPU `Renderer`,
//! xev handles, `Termio.init` — runtime panics confined there), with
//! `activateInspector`/`deactivateInspector` the ONLY verbatim-comment
//! deferrals (CORE_BLUEPRINT §4 — they construct the uninhabited Phase-3
//! `Inspector`); chunk C7 landed Surface.zig:958-1523 — `handleMessage` (all
//! 22 Message arms, checked exhaustively against the reference)/
//! `selectionScrollTick`/`childExited`(+`Abnormally`)/`passwordInput`/
//! `searchCallback`(`_`) LIVE (the search-thread callbacks are fully real —
//! blueprint §5; the 3 `wakeup.notify` sites are the xev leaves) plus
//! `queueRender` (:2461, ahead of its C8 row — one-line xev leaf this range
//! calls); chunk C8 landed Surface.zig:1524-1866 + 2396-2604 —
//! `modsChanged`/`mouseRefreshLinks`/`updateRendererHealth`/`updateScrollbar`/
//! `notifyConfigConditionalState`/`updateConfig`/`recomputeInitialSize`
//! (+`InitialSizeError`) and `setCellSize`/`setFontSize`/`sizeCallback`/
//! `resize`/`balancePaddingIfNeeded`/`preeditCallback` LIVE, plus the crash
//! thread-state seam AHEAD of its C10 row (`crashThreadState` :6014 +
//! `impl ThreadStateSurface` — sizeCallback/preeditCallback are the first
//! `crash.sentry.thread_state` sites, [ADV 6.1]); chunk C9 landed
//! Surface.zig:1867-2395 — `Text`(+`TextViewport`)/`dumpText`(`Locked`)/
//! `hasSelection`/`selectionString`/`pwd`/`resolvePathForOpening`/`imePoint`/
//! `clipboardWrite`/`copySelectionToClipboards`/`setSelection`(`AndCopy`)
//! LIVE — `imePoint`'s no-defer lock/unlock paired verbatim with the
//! reference lines; the std.base64/std.fs.path analogues are the file-scope
//! PORT helpers at the bottom; chunk C10 landed the key tier, Surface.zig:
//! 2605-3281 + 6014-end — `keyEventIsBinding`/`keyCallback`/
//! `maybeHandleBinding`/`deactivateAllKeyTables`/`catchAllIsIgnore`/
//! `endKeySequence`/`encodeKey`(`Opts`) LIVE (keyCallback's inspector defer
//! block is the `InspEvGuard` Drop — CORE_BLUEPRINT §4: None-arm live,
//! Some-arm unreachable until Phase 3; its `crash.sentry.thread_state` site
//! is the third [ADV 6.1] guard site) plus the 6014-end tail
//! (`crashThreadState`/`ThreadStateSurface` had landed with C8;
//! `presentSurface` LIVE; `getProcessInfo` split into
//! `getForegroundPid`/`getTtyName` — the rustty-os pty.rs comptime-return
//! precedent) and `closingAction` ported ahead of its C14 row; chunk C11
//! landed the callback tier, Surface.zig:3282-3639 — `textCallback`/
//! `occlusionCallback`/`focusCallback`/`refreshCallback`/`scrollCallback`/
//! `contentScaleCallback` LIVE (the file-scope `ScrollAmount` hoists to the
//! trailing cluster; focusCallback's comptime modifier-release loop is a
//! local-macro unroll; six more [ADV 6.1] guard sites) with STUBS for
//! `isMouseReporting`/`mouseReport` (C12) and `completeClipboardPaste`
//! (C15) — their first callsites are this tier;
//! CORE_BLUEPRINT §6 rows C12..C15 fill the remaining function bodies IN
//! PLACE — the declared method stubs below are owned by C12/C13/C14/C15).

// PARITY: the reference's identifiers are camelCase (`needsConfirmQuit`,
// `handleMessage`, `performBindingAction`, `scaledPadding`,
// `activeLeftClickPin`); preserved verbatim per R1/R2 (renderer_seam.rs /
// termio thread.rs precedent).
#![allow(non_snake_case)]

// const Surface = @This();
//
// const apprt = @import("apprt.zig");
use rustty_apprt_api as apprt;
// pub const Mailbox = apprt.surface.Mailbox;
//
// PORT NOTE: `apprt.surface.Mailbox` is RELOCATED into this module as
// [`SurfaceMailbox`] (see its PORT NOTE below); the reference re-export is
// this alias.
pub type Mailbox<A> = SurfaceMailbox<A>;
// pub const Message = apprt.surface.Message;
pub use rustty_apprt_api::surface::Message;

// const std = @import("std");
// const builtin = @import("builtin");
// const assert = @import("quirks.zig").inlineAssert;
// PORT NOTE: `assert` -> `debug_assert!` (the established quirks.inlineAssert
// mapping, binding.rs / app.rs precedent).
// const Allocator = std.mem.Allocator;
use allocator_api2::alloc::Global as Allocator;
// const ArenaAllocator = std.heap.ArenaAllocator;
// PORT NOTE: arena-backed clones map to owned fields that free on drop (the
// font-shaper shared_grid_set precedent); see the `DerivedConfig` arena note.
// const global_state = &@import("global.zig").state;
// PORT NOTE (BREAK #1 wiring, chunk C6): `crate::global_state()` — the
// core-wired `GlobalState`/`STATE` glue in lib.rs (init's resources-dir
// site; the crash sites follow with C10).
// const oni = @import("oniguruma");
//
// PORT NOTE: the rustty-oniguruma library artifact is named `oni` (its
// Cargo.toml `[lib] name`), so `oni::*` paths bind exactly as the reference
// module name — no `use` needed (the rustty-input link.rs precedent).
// const crash = @import("crash/main.zig");
//
// PORT NOTE: first reached by C8 (`crash.sentry.thread_state` in
// sizeCallback/preeditCallback + `crashThreadState`, ahead of its C10 row —
// [ADV 6.1]); the C10 key tier added its keyCallback site; the C11 callback
// tier added its six sites; the remaining sites land with C12-C13.
use rustty_crash as crash;
// const unicode = @import("unicode/main.zig");
//
// PORT NOTE: first reached by C8 (preeditCallback's
// `unicode.table.get(cp).width`, Surface.zig:2572).
use rustty_unicode as unicode;
// const rendererpkg = @import("renderer.zig");
//
// PORT NOTE (CORE_BLUEPRINT §5): the renderer package splits — the REAL
// `State`/`Size`/`Padding` live in rustty-renderer (+ rustty-render-types via
// `rustty_renderer::size`); the GPU `Renderer`/`Thread` are the core-local
// `crate::renderer_seam` dependency placeholders.
use rustty_renderer as rendererpkg;
// const termio = @import("termio.zig");
use rustty_termio as termio;
// const font = @import("font/main.zig");
//
// PORT NOTE (D1 3-tier split): the reference `font` hub splits — `Metrics`
// lives in rustty-font (pure, BELOW vt); `SharedGridSet`/`face.DesiredSize`/
// `face.default_dpi` live in rustty-font-shaper (the shaper tier ABOVE
// vt+font).
use rustty_font::metrics::Metrics;
use rustty_font_shaper::face;
use rustty_font_shaper::face::DesiredSize;
use rustty_font_shaper::shared_grid_set;
// const Command = @import("Command.zig");
// PORT NOTE: `Command` is RELOCATED to rustty-termio (break #13b —
// Command-in-core would cycle); first reached by C15 getProcessInfo as
// `rustty_termio::command::Command`; preserved as this comment.
// const terminal = @import("terminal/main.zig");
use rustty_vt as terminal;
// const configpkg = @import("config.zig");
use rustty_config as configpkg;
// const Duration = configpkg.Config.Duration;
use rustty_config::value_types::Duration;
// const input = @import("input.zig");
//
// PORT NOTE: the `input` hub splits — the VT input slice (`KeyEvent`, `Mods`,
// mouse `Button`/`ButtonState`/`PressureStage`) lives in rustty-vt's input
// modules; `Binding`/`Link`/`OptionAsAlt`/`KeyRemapSet` live in rustty-input
// (the surface_mouse.rs precedent).
use rustty_input::binding;
use rustty_input::config::OptionAsAlt;
use rustty_input::key_mods::RemapSet;
use rustty_input::link;
use rustty_vt::input::key;
use rustty_vt::input::key::u21;
use rustty_vt::input::key::Key;
use rustty_vt::input::key::KeyEvent;
// PORT NOTE (D1 vt slice): `input.key_encode` lives in rustty-vt (the key
// encoder is VT-protocol logic) — `encodeKey`/`encodeKeyOpts` (C10).
use rustty_vt::input::key_encode;
use rustty_vt::input::key_mods::Mods;
use rustty_vt::input::mouse as input_mouse;
// const App = @import("App.zig");
use crate::app::App;
// const internal_os = @import("os/main.zig");
use rustty_os as internal_os;
// const inspectorpkg = @import("inspector/main.zig");
use rustty_inspector as inspectorpkg;
// const SurfaceMouse = @import("surface_mouse.zig");
use crate::surface_mouse::SurfaceMouse;
// const ProcessInfo = @import("pty.zig").ProcessInfo;
// PORT NOTE: `ProcessInfo` — first reached by C15 getProcessInfo; preserved
// as this comment.

// AppMailbox trait + the concrete queue Timeout/Size for the
// `SurfaceMailbox` impl below (CORE_BLUEPRINT §3).
use rustty_apprt_api::surface::AppMailbox;
use rustty_datastruct::blocking_queue::{Size as QueueSize, Timeout};

// The apprt action payloads init/handleMessage send (`performAction` —
// cell_size / size_limit / set_title / color_change / pwd / secure_input /
// show_child_exited / command_finished / search_total / search_selected) plus
// the C8 tier's (config_change / initial_size / mouse_over_link /
// reload_config — the [ADV-FIX 4] dispatch-arm list), and the `Surface` seam
// trait for the typed `rt_surface.*` method edges (aliased per the C0
// DECISION note — core's own struct is also named `Surface`).
use rustty_apprt_api::action::{
    CellSize, ColorChange, ColorKind, CommandFinished, ConfigChange, InitialSize, KeySequence,
    KeyTable, MouseOverLink, Pwd, ReloadConfig, SearchSelected, SearchTotal, SecureInput, SetTitle,
    SizeLimit,
};
use rustty_apprt_api::Surface as ApprtSurface;

// The apprt WriteReq payload arms (`Message.clipboard_write`/`pwd_change` are
// `MessageData(u8, 255)` unions) + the termio `write_alloc` payload
// (handleMessage's report_title arm) — C7.
use rustty_datastruct::message_data::Alloc;
use rustty_datastruct::message_data::Small;
use rustty_datastruct::MessageData;

// The vt search-thread event type (`terminal.search.Thread.Event`) —
// searchCallback(_)'s dispatch (C7) — and the vt `Terminal` the reference
// names as `terminal.Terminal` under the renderer mutex (the renderer
// state.rs / termio_impl.rs import precedent).
use rustty_vt::search::thread::Event as SearchThreadEvent;
use rustty_vt::terminal::Terminal;

// const log = std.log.scoped(.surface);
// PORT NOTE: the reference `log.debug/info/warn/err` sites are preserved
// verbatim as comments at each site (termio/app.rs precedent — no Rust
// logging shim).

// The renderer implementation to use.
// const Renderer = rendererpkg.Renderer;
//
// PORT NOTE (CORE_BLUEPRINT §5): the comptime-chosen GPU renderer is the
// core-local dependency placeholder.
use crate::renderer_seam::Renderer;

/// Minimum window size in cells. This is used to prevent the window from
/// being resized to a size that is too small to be useful. These defaults
/// are chosen to match the default size of Mac's Terminal.app, but is
/// otherwise somewhat arbitrary.
// pub const min_window_width_cells: u32 = 10;
// pub const min_window_height_cells: u32 = 4;
pub const MIN_WINDOW_WIDTH_CELLS: u32 = 10;
pub const MIN_WINDOW_HEIGHT_CELLS: u32 = 4;

/// The maximum number of key tables that can be active at any
/// given time. `activate_key_table` calls after this are ignored.
// const max_active_key_tables = 8;
//
// PORT NOTE: the C14a `performBindingAction` arm (Surface.zig:5495) is the
// only consumer; the allow is removed when it lands.
#[allow(dead_code)]
const MAX_ACTIVE_KEY_TABLES: usize = 8;

/// Surface represents a single terminal "surface" (see the module doc; the
/// reference file IS this struct via `const Surface = @This()`).
//
// PORT NOTE (CORE_BLUEPRINT §1): generic over the apprt backend `A` exactly
// as Zig comptime-selects ONE apprt implementation; `A: 'static` is required
// by crash's dyn erasure (§1 explosion audit — benign, backends own no
// borrows). Landed under the C5 field-parity audit: 31 reference fields ==
// 31 Rust fields.
pub struct Surface<A: apprt::App + 'static> {
    /// Unique ID used to identify this surface for IPC purposes. It is
    /// exposed to the commands running in surfaces as the environment variable
    /// RUSTTY_SURFACE_ID. It must not be zero as zero is used to incicate a null
    /// value when communicating an ID over DBus as DBus does not allow null/maybe
    /// values.
    // id: u64,
    pub id: u64,

    /// Allocator
    // alloc: Allocator,
    pub alloc: Allocator,

    /// The app that this surface is attached to.
    // app: *App,
    pub app: *mut App<A>,

    /// The windowing system surface and app.
    // rt_app: *apprt.runtime.App,
    // rt_surface: *apprt.runtime.Surface,
    pub rt_app: *mut A,
    pub rt_surface: *mut A::Surface,

    /// The font structures
    // font_grid_key: font.SharedGridSet.Key,
    // font_size: font.face.DesiredSize,
    // font_metrics: font.Metrics,
    pub font_grid_key: shared_grid_set::Key,
    pub font_size: DesiredSize,
    pub font_metrics: Metrics,

    /// This keeps track of if the font size was ever modified. If it wasn't,
    /// then config reloading will change the font. If it was manually adjusted,
    /// we don't change it on config reload since we assume the user wants
    /// a specific size.
    // font_size_adjusted: bool,
    pub font_size_adjusted: bool,

    /// The renderer for this surface.
    // renderer: Renderer,
    pub renderer: Renderer,

    /// The render state
    // renderer_state: rendererpkg.State,
    pub renderer_state: rendererpkg::state::State,

    /// The renderer thread manager
    // renderer_thread: rendererpkg.Thread,
    pub renderer_thread: crate::renderer_seam::Thread<A>,

    /// The actual thread
    // renderer_thr: std.Thread,
    //
    // PORT NOTE (CORE_BLUEPRINT §8.5): Zig `std.Thread` is `undefined` until
    // init spawns it — Rust has no `undefined`, so the handle is an `Option`
    // (`None` until spawn); `JoinHandle::join` also consumes, which the
    // `take()` at the join sites needs anyway. Documented shape deviation.
    pub renderer_thr: Option<std::thread::JoinHandle<()>>,

    /// Mouse state.
    // mouse: Mouse,
    pub mouse: Mouse,

    /// Keyboard input state.
    // keyboard: Keyboard,
    pub keyboard: Keyboard,

    /// A currently pressed key. This is used so that we can send a keyboard
    /// release event when the surface is unfocused. Note that when the surface
    /// is refocused, a key press event may not be sent again -- this depends
    /// on the apprt (UI framework) in use, but we want to consistently send
    /// a release.
    ///
    /// This is only sent when a keypress event results in a key event being
    /// sent to the pty. If it is consumed by a keybinding or other action,
    /// this is not set.
    ///
    /// Also note the utf8 value is not valid for this event so some unfocused
    /// release events may not send exactly the right data within Kitty keyboard
    /// events. This seems unspecified in the spec so for now I'm okay with
    /// this. Plus, its only for release events where the key text is far
    /// less important.
    // pressed_key: ?input.KeyEvent = null,
    pub pressed_key: Option<KeyEvent>,

    /// The hash value of the last keybinding trigger that we performed. This
    /// is only set if the last key input matched a keybinding, consumed it,
    /// and performed it. This is used to prevent sending release/repeat events
    /// for handled bindings.
    // last_binding_trigger: u64 = 0,
    pub last_binding_trigger: u64,

    /// The terminal IO handler.
    // io: termio.Termio,
    // io_thread: termio.Thread,
    // io_thr: std.Thread,
    //
    // PORT NOTE: `Termio` is generic over its app mailbox (the termio M seam);
    // core instantiates it at the concrete [`SurfaceMailbox<A>`] below
    // (CORE_BLUEPRINT §1). `io_thr` follows the `renderer_thr` Option shape
    // deviation (undefined-until-spawn `std.Thread`).
    pub io: termio::Termio<SurfaceMailbox<A>>,
    pub io_thread: termio::Thread,
    pub io_thr: Option<std::thread::JoinHandle<()>>,

    /// Terminal inspector
    // inspector: ?*inspectorpkg.Inspector = null,
    //
    // PORT NOTE (CORE_BLUEPRINT §4): `Inspector` is the deferred uninhabited
    // placeholder (`enum {}`); the pointer is always `None` until Phase 3.
    pub inspector: Option<*mut inspectorpkg::Inspector>,

    /// All our sizing information.
    // size: rendererpkg.Size,
    pub size: rendererpkg::size::Size,

    /// The configuration derived from the main config. We "derive" it so that
    /// we don't have a shared pointer hanging around that we need to worry about
    /// the lifetime of. This makes updating config at runtime easier.
    // config: DerivedConfig,
    pub config: DerivedConfig,

    /// The conditional state of the configuration. This can affect
    /// how certain configurations take effect such as light/dark mode.
    /// This is managed completely by Ghostty core but an apprt action
    /// is sent whenever this changes.
    // config_conditional_state: configpkg.ConditionalState,
    pub config_conditional_state: configpkg::conditional::State,

    /// This is set to true if our IO thread notifies us our child exited.
    /// This is used to determine if we need to confirm, hold open, etc.
    // child_exited: bool = false,
    pub child_exited: bool,

    /// We maintain our focus state and assume we're focused by default.
    /// If we're not initially focused then apprts can call focusCallback
    /// to let us know.
    // focused: bool = true,
    pub focused: bool,

    /// Used to determine whether to continuously scroll.
    // selection_scroll_active: bool = false,
    pub selection_scroll_active: bool,

    /// True if the surface is in read-only mode. When read-only, no input
    /// is sent to the PTY but terminal-level operations like selections,
    /// (native) scrolling, and copy keybinds still work. Warn before quit is
    /// always enabled in this state.
    // readonly: bool = false,
    pub readonly: bool,

    /// Used to send notifications that long running commands have finished.
    /// Requires that shell integration be active. Should represent a nanosecond
    /// precision timestamp. It does not necessarily need to correspond to the
    /// actual time, but we must be able to compare two subsequent timestamps to get
    /// the wall clock time that has elapsed between timestamps.
    // command_timer: ?std.time.Instant = null,
    //
    // PORT NOTE: Zig `std.time.Instant` -> `std::time::Instant` (monotonic,
    // comparable — exactly the stated contract).
    pub command_timer: Option<std::time::Instant>,

    /// Search state
    // search: ?Search = null,
    pub search: Option<Search>,

    /// Used to rate limit BEL handling.
    // last_bell_time: ?std.time.Instant = null,
    pub last_bell_time: Option<std::time::Instant>,
}

/// The effect of an input event. This can be used by callers to take
/// the appropriate action after an input event. For example, key
/// input can be forwarded to the OS for further processing if it
/// wasn't handled in any way by Ghostty.
// pub const InputEffect = enum { ... };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputEffect {
    /// The input was not handled in any way by Ghostty and should be
    /// forwarded to other subsystems (i.e. the OS) for further
    /// processing.
    // ignored,
    Ignored,

    /// The input was handled and consumed by Ghostty.
    // consumed,
    Consumed,

    /// The input resulted in a close event for this surface so
    /// the surface, runtime surface, etc. pointers may all be
    /// unsafe to use so exit immediately.
    // closed,
    Closed,
}

/// The search state for the surface.
// const Search = struct { ... };
//
// PORT NOTE: Zig-private type widened to `pub` — it types the pub
// `Surface.search` field (the termio "Zig-private widens" rule).
pub struct Search {
    // state: terminal.search.Thread,
    pub state: terminal::search::thread::Thread,
    // thread: std.Thread,
    //
    // PORT NOTE: unlike `renderer_thr`/`io_thr`, this handle is never
    // `undefined` (a `Search` is only constructed whole after the spawn), so
    // it stays a bare `JoinHandle`; the join-consumes constraint moves to the
    // `deinit` receiver below.
    pub thread: std::thread::JoinHandle<()>,
}

impl Search {
    // pub fn deinit(self: *Search) void {
    //
    // PORT NOTE: by-value receiver — `JoinHandle::join` consumes the handle
    // (callers hold `Option<Search>` and `take()` it, the same drop point as
    // the reference's `if (self.search) |*search| search.deinit()`).
    pub fn deinit(mut self) {
        // Notify the thread to stop
        // self.state.stop.notify() catch |err| log.err(
        //     "error notifying search thread to stop, may stall err={}",
        //     .{err},
        // );
        if let Err(_err) = self.state.stop.notify() {
            // log.err("error notifying search thread to stop, may stall err={}", .{err});
        }

        // Wait for the OS thread to quit
        // self.thread.join();
        self.thread.join().unwrap();

        // Now it is safe to deinit the state
        self.state.deinit();
    }
}

/// Mouse state for the surface.
// const Mouse = struct { ... };
//
// PORT NOTE: Zig-private type widened to `pub` — it types the pub
// `Surface.mouse` field.
pub struct Mouse {
    /// The last tracked mouse button state by button.
    // click_state: [input.MouseButton.max]input.MouseButtonState = @splat(.release),
    pub click_state: [input_mouse::ButtonState; input_mouse::Button::MAX as usize],

    /// The last mods state when the last mouse button (whatever it was) was
    /// pressed or release.
    // mods: input.Mods = .{},
    pub mods: Mods,

    /// Gesture state for text selection.
    // selection_gesture: terminal.SelectionGesture = .init,
    pub selection_gesture: terminal::selection_gesture::SelectionGesture,

    /// The last x/y sent for mouse reports.
    // event_point: ?terminal.point.Coordinate = null,
    pub event_point: Option<terminal::point::Coordinate>,

    /// The pressure stage for the mouse. This should always be none if
    /// the mouse is not pressed.
    // pressure_stage: input.MousePressureStage = .none,
    pub pressure_stage: input_mouse::PressureStage,

    /// Pending scroll amounts for high-precision scrolls
    // pending_scroll_x: f64 = 0,
    // pending_scroll_y: f64 = 0,
    pub pending_scroll_x: f64,
    pub pending_scroll_y: f64,

    /// True if the mouse is hidden
    // hidden: bool = false,
    pub hidden: bool,

    /// True if the mouse position is currently over a link.
    // over_link: bool = false,
    pub over_link: bool,

    /// The last x/y in the cursor position for links. We use this to
    /// only process link hover events when the mouse actually moves cells.
    // link_point: ?terminal.point.Coordinate = null,
    pub link_point: Option<terminal::point::Coordinate>,
}

// PORT NOTE: the reference field defaults (`= .release`-splat, `= .{}`,
// `= .init`, nulls, zeros, falses) — realized as `Default` so `.mouse = .{}`
// construction sites map to `Mouse::default()`.
impl Default for Mouse {
    fn default() -> Mouse {
        Mouse {
            // click_state: ... = @splat(.release),
            click_state: [input_mouse::ButtonState::Release; input_mouse::Button::MAX as usize],
            // mods: input.Mods = .{},
            mods: Mods::default(),
            // selection_gesture: terminal.SelectionGesture = .init,
            selection_gesture: terminal::selection_gesture::SelectionGesture::INIT,
            // event_point: ?terminal.point.Coordinate = null,
            event_point: None,
            // pressure_stage: input.MousePressureStage = .none,
            pressure_stage: input_mouse::PressureStage::None,
            // pending_scroll_x: f64 = 0,
            // pending_scroll_y: f64 = 0,
            pending_scroll_x: 0.0,
            pending_scroll_y: 0.0,
            // hidden: bool = false,
            hidden: false,
            // over_link: bool = false,
            over_link: false,
            // link_point: ?terminal.point.Coordinate = null,
            link_point: None,
        }
    }
}

impl Mouse {
    /// Return the left-click pin only if it still belongs to the active screen.
    // fn activeLeftClickPin(self: *const Mouse, screens: *const terminal.ScreenSet) ?*terminal.Pin {
    //
    // (the C8 mouseRefreshLinks :1594 callsite is live; the C13+ :4650
    // callsites land later.)
    fn activeLeftClickPin(
        &self,
        screens: &terminal::screen_set::ScreenSet,
    ) -> Option<*mut terminal::pagelist::Pin> {
        // return self.selection_gesture.validatedLeftClickPin(screens);
        self.selection_gesture.validated_left_click_pin(screens)
    }
}

/// Keyboard state for the surface.
// pub const Keyboard = struct { ... };
//
// PORT NOTE: the reference field defaults (`= null`, `= .empty`) are all the
// Rust defaults, so `derive(Default)` realizes `.keyboard = .{}` sites
// exactly.
#[derive(Default)]
pub struct Keyboard {
    /// The currently active key sequence for the surface. If this is null
    /// then we're not currently in a key sequence.
    // sequence_set: ?*const input.Binding.Set = null,
    pub sequence_set: Option<*const binding::Set>,

    /// The queued keys when we're in the middle of a sequenced binding.
    /// These are flushed when the sequence is completed and unconsumed or
    /// invalid.
    ///
    /// This is naturally bounded due to the configuration maximum
    /// length of a sequence.
    // sequence_queued: std.ArrayListUnmanaged(termio.Message.WriteReq) = .empty,
    //
    // PORT NOTE: `WriteReq` at the `'static` instantiation — the queued
    // requests are owned (`Alloc`/`Small`), the termio cross-thread-queue
    // precedent.
    pub sequence_queued: Vec<termio::message::WriteReq<'static>>,

    /// The stack of tables that is currently active. The first value
    /// in this is the first activated table (NOT the default keybinding set).
    ///
    /// This is bounded by `max_active_key_tables`.
    // table_stack: std.ArrayListUnmanaged(struct {
    //     set: *const input.Binding.Set,
    //     once: bool,
    // }) = .empty,
    //
    // PORT NOTE: the reference's anonymous element struct is the named
    // sibling [`TableStackEntry`] (Rust has no anonymous structs; the
    // established nested-type hoist).
    pub table_stack: Vec<TableStackEntry>,

    /// The last handled binding. This is used to prevent encoding release
    /// events for handled bindings. We only need to keep track of one because
    /// at least at the time of writing this, its impossible for two keys of
    /// a combination to be handled by different bindings before the release
    /// of the prior (namely since you can't bind modifier-only).
    // last_trigger: ?u64 = null,
    pub last_trigger: Option<u64>,
}

/// PORT: the reference's anonymous `table_stack` element struct (see the
/// [`Keyboard::table_stack`] PORT NOTE). `Copy` — the reference reads
/// elements out BY VALUE (`const table = table_items[rev_i];`,
/// maybeHandleBinding Surface.zig:2903).
#[derive(Clone, Copy)]
pub struct TableStackEntry {
    // set: *const input.Binding.Set,
    pub set: *const binding::Set,
    // once: bool,
    pub once: bool,
}

/// The configuration that a surface has, this is copied from the main
/// Config struct usually to prevent sharing a single value.
// const DerivedConfig = struct { ... };
//
// PORT NOTE: Zig-private type widened to `pub` — it types the pub
// `Surface.config` field. Landed under the C5 field-parity audit: 46
// reference fields == 45 Rust fields + the collapsed `arena` (owned fields
// free on drop — the font-shaper shared_grid_set precedent; `deinit` keeps
// the explicit non-memory teardown).
pub struct DerivedConfig {
    // arena: ArenaAllocator,
    //
    // PORT NOTE (collapsed): every arena-cloned field below is an owned value.

    // For docs for these, see the associated config they are derived from.
    // original_font_size: f32,
    pub original_font_size: f32,
    // keybind: configpkg.Keybinds,
    pub keybind: configpkg::value_types::Keybinds,
    // abnormal_command_exit_runtime_ms: u32,
    pub abnormal_command_exit_runtime_ms: u32,
    // clipboard_read: configpkg.ClipboardAccess,
    pub clipboard_read: configpkg::value_types::ClipboardAccess,
    // clipboard_write: configpkg.ClipboardAccess,
    pub clipboard_write: configpkg::value_types::ClipboardAccess,
    // clipboard_trim_trailing_spaces: bool,
    pub clipboard_trim_trailing_spaces: bool,
    // clipboard_paste_protection: bool,
    pub clipboard_paste_protection: bool,
    // clipboard_paste_bracketed_safe: bool,
    pub clipboard_paste_bracketed_safe: bool,
    // clipboard_codepoint_map: configpkg.Config.RepeatableClipboardCodepointMap,
    pub clipboard_codepoint_map: configpkg::value_types::RepeatableClipboardCodepointMap,
    // copy_on_select: configpkg.CopyOnSelect,
    pub copy_on_select: configpkg::value_types::CopyOnSelect,
    // right_click_action: configpkg.RightClickAction,
    pub right_click_action: configpkg::value_types::RightClickAction,
    // middle_click_action: configpkg.MiddleClickAction,
    pub middle_click_action: configpkg::value_types::MiddleClickAction,
    // confirm_close_surface: configpkg.ConfirmCloseSurface,
    pub confirm_close_surface: configpkg::value_types::ConfirmCloseSurface,
    // cursor_click_to_move: bool,
    pub cursor_click_to_move: bool,
    // desktop_notifications: bool,
    pub desktop_notifications: bool,
    // font: font.SharedGridSet.DerivedConfig,
    pub font: shared_grid_set::DerivedConfig,
    // mouse_interval: u64,
    pub mouse_interval: u64,
    // mouse_hide_while_typing: bool,
    pub mouse_hide_while_typing: bool,
    // mouse_reporting: bool,
    pub mouse_reporting: bool,
    // mouse_scroll_multiplier: configpkg.MouseScrollMultiplier,
    pub mouse_scroll_multiplier: configpkg::value_types::MouseScrollMultiplier,
    // mouse_shift_capture: configpkg.MouseShiftCapture,
    pub mouse_shift_capture: configpkg::value_types::MouseShiftCapture,
    // fullscreen: configpkg.Fullscreen,
    pub fullscreen: configpkg::value_types::Fullscreen,
    // macos_non_native_fullscreen: configpkg.NonNativeFullscreen,
    pub macos_non_native_fullscreen: configpkg::value_types::NonNativeFullscreen,
    // macos_option_as_alt: ?input.OptionAsAlt,
    pub macos_option_as_alt: Option<OptionAsAlt>,
    // selection_clear_on_copy: bool,
    pub selection_clear_on_copy: bool,
    // selection_clear_on_typing: bool,
    pub selection_clear_on_typing: bool,
    // selection_word_chars: []const u21,
    //
    // PORT NOTE: owned `Vec<u21>` (the reference slice lives in the collapsed
    // arena; `u21` is the vt key.rs alias = `u32`).
    pub selection_word_chars: Vec<u21>,
    // vt_kam_allowed: bool,
    pub vt_kam_allowed: bool,
    // wait_after_command: bool,
    pub wait_after_command: bool,
    // window_padding_top: u32,
    pub window_padding_top: u32,
    // window_padding_bottom: u32,
    pub window_padding_bottom: u32,
    // window_padding_left: u32,
    pub window_padding_left: u32,
    // window_padding_right: u32,
    pub window_padding_right: u32,
    // window_padding_balance: configpkg.Config.WindowPaddingBalance,
    pub window_padding_balance: configpkg::value_types::WindowPaddingBalance,
    // window_height: u32,
    pub window_height: u32,
    // window_width: u32,
    pub window_width: u32,
    // title: ?[:0]const u8,
    //
    // PORT NOTE: the reference copies the config's slice (pointer into the
    // config's memory); the owned clone here carries the same bytes without
    // the aliased lifetime (rustty-config stores `Option<Vec<u8>>`).
    pub title: Option<Vec<u8>>,
    // title_report: bool,
    pub title_report: bool,
    // links: []DerivedConfig.Link,
    pub links: Vec<DerivedConfigLink>,
    // link_previews: configpkg.LinkPreviews,
    pub link_previews: configpkg::value_types::LinkPreviews,
    // scroll_to_bottom: configpkg.Config.ScrollToBottom,
    pub scroll_to_bottom: configpkg::value_types::ScrollToBottom,
    // notify_on_command_finish: configpkg.Config.NotifyOnCommandFinish,
    pub notify_on_command_finish: configpkg::value_types::NotifyOnCommandFinish,
    // notify_on_command_finish_action: configpkg.Config.NotifyOnCommandFinishAction,
    pub notify_on_command_finish_action: configpkg::value_types::NotifyOnCommandFinishAction,
    // notify_on_command_finish_after: Duration,
    pub notify_on_command_finish_after: Duration,
    // key_remaps: input.KeyRemapSet,
    pub key_remaps: RemapSet,
}

// const Link = struct {
//     regex: oni.Regex,
//     action: input.Link.Action,
//     highlight: input.Link.Highlight,
// };
//
// PORT NOTE: the reference nests this as `DerivedConfig.Link` (and refers to
// it by that qualified name — `links: []DerivedConfig.Link`); Rust has no
// nested type declarations, so the sibling carries the qualified name. (The
// module-level name `Link` is reserved for Surface.zig:4271's file-scope
// `const Link`, the C13 mouse-link type.)
pub struct DerivedConfigLink {
    pub regex: oni::Regex,
    pub action: link::Action,
    pub highlight: link::Highlight,
}

impl DerivedConfig {
    // pub fn init(alloc_gpa: Allocator, config: *const configpkg.Config) !DerivedConfig {
    //
    // PORT NOTE: the inferred error set is the `link.oniRegex()` failures ∪
    // `Allocator.Error`; the latter collapses (infallible Rust allocation —
    // the termio/app.rs convention), leaving `oni::Error`.
    pub fn init(
        alloc_gpa: Allocator,
        config: &configpkg::Config,
    ) -> Result<DerivedConfig, oni::Error> {
        // var arena = ArenaAllocator.init(alloc_gpa);
        // errdefer arena.deinit();
        // const alloc = arena.allocator();
        //
        // PORT NOTE (collapsed arena): the clones below are owned values that
        // free on drop; the errdefer chain is covered by ownership drops on
        // the error path. `alloc_gpa` is kept for signature parity.
        let _ = alloc_gpa;

        // Build all of our links
        let links = {
            // var links: std.ArrayList(DerivedConfig.Link) = .empty;
            // defer links.deinit(alloc);
            let mut links: Vec<DerivedConfigLink> = Vec::new();
            // for (config.link.links.items) |link| {
            for link in &config.link.links {
                // var regex = try link.oniRegex();
                // errdefer regex.deinit();
                //
                // PORT NOTE: the errdefer guards the (infallible-in-Rust)
                // `links.append` below. On a FAILED `oniRegex` the reference
                // frees only the list storage (`defer links.deinit`) — the
                // previously-compiled regexes are NOT deinited on that path —
                // and the `Vec` drop on the `?` unwind reproduces that
                // exactly (`DerivedConfigLink` has no `Drop`; onig handles
                // free only via the explicit deinit chain).
                let regex = link.oni_regex()?;
                // try links.append(alloc, .{
                //     .regex = regex,
                //     .action = link.action,
                //     .highlight = link.highlight,
                // });
                links.push(DerivedConfigLink {
                    regex,
                    action: link.action,
                    highlight: link.highlight,
                });
            }

            // break :links try links.toOwnedSlice(alloc);
            links
        };
        // errdefer {
        //     for (links) |*link| link.regex.deinit();
        //     alloc.free(links);
        // }
        //
        // PORT NOTE: nothing after this point can fail in the port (the
        // remaining `try` sites are Allocator.Error-only clones), so this
        // errdefer has no live unwind path.

        // return .{
        Ok(DerivedConfig {
            // .original_font_size = config.@"font-size",
            original_font_size: config.font_size,
            // .keybind = try config.keybind.clone(alloc),
            keybind: config.keybind.clone_in(),
            // .abnormal_command_exit_runtime_ms = config.@"abnormal-command-exit-runtime",
            abnormal_command_exit_runtime_ms: config.abnormal_command_exit_runtime,
            // .clipboard_read = config.@"clipboard-read",
            clipboard_read: config.clipboard_read,
            // .clipboard_write = config.@"clipboard-write",
            clipboard_write: config.clipboard_write,
            // .clipboard_trim_trailing_spaces = config.@"clipboard-trim-trailing-spaces",
            clipboard_trim_trailing_spaces: config.clipboard_trim_trailing_spaces,
            // .clipboard_paste_protection = config.@"clipboard-paste-protection",
            clipboard_paste_protection: config.clipboard_paste_protection,
            // .clipboard_paste_bracketed_safe = config.@"clipboard-paste-bracketed-safe",
            clipboard_paste_bracketed_safe: config.clipboard_paste_bracketed_safe,
            // .clipboard_codepoint_map = try config.@"clipboard-codepoint-map".clone(alloc),
            clipboard_codepoint_map: config.clipboard_codepoint_map.clone_in(),
            // .copy_on_select = config.@"copy-on-select",
            copy_on_select: config.copy_on_select,
            // .right_click_action = config.@"right-click-action",
            right_click_action: config.right_click_action,
            // .middle_click_action = config.@"middle-click-action",
            middle_click_action: config.middle_click_action,
            // .confirm_close_surface = config.@"confirm-close-surface",
            confirm_close_surface: config.confirm_close_surface,
            // .cursor_click_to_move = config.@"cursor-click-to-move",
            cursor_click_to_move: config.cursor_click_to_move,
            // .desktop_notifications = config.@"desktop-notifications",
            desktop_notifications: config.desktop_notifications,
            // .font = try font.SharedGridSet.DerivedConfig.init(alloc, config),
            //
            // PORT NOTE (break #9 DI realization): the reference init is
            // duck-typed (`config: anytype`) over the GLOBAL config, but
            // rustty-font-shaper's `DerivedConfig::init` takes the
            // shaper-local subset type (that tier must not dep rustty-config
            // — G7 cycle). Core is the first crate that sees both sides, so
            // the field-by-field injection is the module-level bridge below
            // ([`font_shared_grid_set_derived_config`]).
            font: font_shared_grid_set_derived_config(config),
            // .mouse_interval = config.@"click-repeat-interval" * 1_000_000, // 500ms
            mouse_interval: u64::from(config.click_repeat_interval) * 1_000_000, // 500ms
            // .mouse_hide_while_typing = config.@"mouse-hide-while-typing",
            mouse_hide_while_typing: config.mouse_hide_while_typing,
            // .mouse_reporting = config.@"mouse-reporting",
            mouse_reporting: config.mouse_reporting,
            // .mouse_scroll_multiplier = config.@"mouse-scroll-multiplier",
            mouse_scroll_multiplier: config.mouse_scroll_multiplier,
            // .mouse_shift_capture = config.@"mouse-shift-capture",
            mouse_shift_capture: config.mouse_shift_capture,
            // .fullscreen = config.fullscreen,
            fullscreen: config.fullscreen,
            // .macos_non_native_fullscreen = config.@"macos-non-native-fullscreen",
            macos_non_native_fullscreen: config.macos_non_native_fullscreen,
            // .macos_option_as_alt = config.@"macos-option-as-alt",
            macos_option_as_alt: config.macos_option_as_alt,
            // .selection_clear_on_copy = config.@"selection-clear-on-copy",
            selection_clear_on_copy: config.selection_clear_on_copy,
            // .selection_clear_on_typing = config.@"selection-clear-on-typing",
            selection_clear_on_typing: config.selection_clear_on_typing,
            // .selection_word_chars = try alloc.dupe(u21, config.@"selection-word-chars".codepoints),
            selection_word_chars: config.selection_word_chars.codepoints.clone(),
            // .vt_kam_allowed = config.@"vt-kam-allowed",
            vt_kam_allowed: config.vt_kam_allowed,
            // .wait_after_command = config.@"wait-after-command",
            wait_after_command: config.wait_after_command,
            // .window_padding_top = config.@"window-padding-y".top_left,
            window_padding_top: config.window_padding_y.top_left,
            // .window_padding_bottom = config.@"window-padding-y".bottom_right,
            window_padding_bottom: config.window_padding_y.bottom_right,
            // .window_padding_left = config.@"window-padding-x".top_left,
            window_padding_left: config.window_padding_x.top_left,
            // .window_padding_right = config.@"window-padding-x".bottom_right,
            window_padding_right: config.window_padding_x.bottom_right,
            // .window_padding_balance = config.@"window-padding-balance",
            window_padding_balance: config.window_padding_balance,
            // .window_height = config.@"window-height",
            window_height: config.window_height,
            // .window_width = config.@"window-width",
            window_width: config.window_width,
            // .title = config.title,
            //
            // PORT NOTE: owned clone of the reference's shallow slice copy
            // (see the field's PORT NOTE).
            title: config.title.clone(),
            // .title_report = config.@"title-report",
            title_report: config.title_report,
            // .links = links,
            links,
            // .link_previews = config.@"link-previews",
            link_previews: config.link_previews,
            // .scroll_to_bottom = config.@"scroll-to-bottom",
            scroll_to_bottom: config.scroll_to_bottom,
            // .notify_on_command_finish = config.@"notify-on-command-finish",
            notify_on_command_finish: config.notify_on_command_finish,
            // .notify_on_command_finish_action = config.@"notify-on-command-finish-action",
            notify_on_command_finish_action: config.notify_on_command_finish_action,
            // .notify_on_command_finish_after = config.@"notify-on-command-finish-after",
            notify_on_command_finish_after: config.notify_on_command_finish_after,
            // .key_remaps = try config.@"key-remap".clone(alloc),
            key_remaps: config.key_remap.clone(),
            // Assignments happen sequentially so we have to do this last
            // so that the memory is captured from allocs above.
            // .arena = arena,
            //
            // PORT NOTE: collapsed (see the struct's arena note); the
            // ordering constraint it documents is moot in rustty.
        })
    }

    // pub fn deinit(self: *DerivedConfig) void {
    pub fn deinit(&mut self) {
        // for (self.links) |*link| link.regex.deinit();
        for link in &mut self.links {
            link.regex.deinit();
        }
        // self.arena.deinit();
        // PORT NOTE: the owned fields free on drop (the collapsed arena).
    }

    // fn scaledPadding(self: *const DerivedConfig, x_dpi: f32, y_dpi: f32) rendererpkg.Padding {
    //
    // (first callsite: `Surface::init`, chunk C6 — Surface.zig:536; more land
    // C8 :2514 / C11 :3630.)
    fn scaledPadding(&self, x_dpi: f32, y_dpi: f32) -> rendererpkg::size::Padding {
        // const padding_top: u32 = padding_top: {
        //     const padding_top: f32 = @floatFromInt(self.window_padding_top);
        //     break :padding_top @intFromFloat(@floor(padding_top * y_dpi / 72));
        // };
        let padding_top: u32 = {
            let padding_top: f32 = self.window_padding_top as f32;
            (padding_top * y_dpi / 72.0).floor() as u32
        };
        // const padding_bottom: u32 = padding_bottom: {
        //     const padding_bottom: f32 = @floatFromInt(self.window_padding_bottom);
        //     break :padding_bottom @intFromFloat(@floor(padding_bottom * y_dpi / 72));
        // };
        let padding_bottom: u32 = {
            let padding_bottom: f32 = self.window_padding_bottom as f32;
            (padding_bottom * y_dpi / 72.0).floor() as u32
        };
        // const padding_left: u32 = padding_left: {
        //     const padding_left: f32 = @floatFromInt(self.window_padding_left);
        //     break :padding_left @intFromFloat(@floor(padding_left * x_dpi / 72));
        // };
        let padding_left: u32 = {
            let padding_left: f32 = self.window_padding_left as f32;
            (padding_left * x_dpi / 72.0).floor() as u32
        };
        // const padding_right: u32 = padding_right: {
        //     const padding_right: f32 = @floatFromInt(self.window_padding_right);
        //     break :padding_right @intFromFloat(@floor(padding_right * x_dpi / 72));
        // };
        let padding_right: u32 = {
            let padding_right: f32 = self.window_padding_right as f32;
            (padding_right * x_dpi / 72.0).floor() as u32
        };

        // return .{
        //     .top = padding_top,
        //     .bottom = padding_bottom,
        //     .left = padding_left,
        //     .right = padding_right,
        // };
        rendererpkg::size::Padding {
            top: padding_top,
            bottom: padding_bottom,
            left: padding_left,
            right: padding_right,
        }
    }
}

/// PORT NOTE (Rust-only glue, break #9 DI realization — see the
/// `DerivedConfig::init` `.font` assignment): builds the rustty-font-shaper
/// `SharedGridSet.DerivedConfig` from the global config. The field list
/// mirrors the reference `SharedGridSet.zig DerivedConfig.init` assignments
/// one-for-one (== rustty-font-shaper shared_grid_set.rs `DerivedConfig::
/// init`, whose duck-typed `config: anytype` source is the global config at
/// this callsite). The per-type conversions are exact: both sides' font
/// subset types model the SAME reference config types (`RepeatableString`,
/// `FontStyle`, `RepeatableFontVariation`, `RepeatableCodepointMap`,
/// `FontSyntheticStyle`), duplicated across the tier boundary by break #9;
/// `Modifier` and `CodepointMap` are literally the same rustty-font types on
/// both sides.
fn font_shared_grid_set_derived_config(
    config: &configpkg::Config,
) -> shared_grid_set::DerivedConfig {
    use rustty_font_shaper::config as font_config;

    // config.RepeatableString -> shaper RepeatableString: same `list` payload
    // (`overwrite_next` is a CLI-parse artifact the shaper subset never had).
    fn repeatable_string(
        v: &configpkg::value_types::RepeatableString,
    ) -> font_config::RepeatableString {
        font_config::RepeatableString {
            list: v.list.clone(),
        }
    }

    // config.FontStyle -> shaper FontStyle: identical variants.
    fn font_style(v: &configpkg::value_types::FontStyle) -> font_config::FontStyle {
        match v {
            configpkg::value_types::FontStyle::Default => font_config::FontStyle::Default,
            configpkg::value_types::FontStyle::False => font_config::FontStyle::False,
            configpkg::value_types::FontStyle::Name(name) => {
                font_config::FontStyle::Name(name.clone())
            }
        }
    }

    // config.RepeatableFontVariation -> shaper RepeatableFontVariation: the
    // config side models `Variation.Id` as the packed `u32`
    // (`a<<24 | b<<16 | c<<8 | d` — value_types.rs `FaceVariation`); the
    // shaper side keeps the reference's field-wise `Id`. Same value, unpacked.
    fn font_variation(
        v: &configpkg::value_types::RepeatableFontVariation,
    ) -> font_config::RepeatableFontVariation {
        font_config::RepeatableFontVariation {
            list: v
                .list
                .iter()
                .map(|variation| rustty_font_shaper::face::Variation {
                    id: rustty_font_shaper::face::Id {
                        a: (variation.id >> 24) as u8,
                        b: (variation.id >> 16) as u8,
                        c: (variation.id >> 8) as u8,
                        d: variation.id as u8,
                    },
                    value: variation.value,
                })
                .collect(),
        }
    }

    shared_grid_set::DerivedConfig {
        // .@"font-family" = try config.@"font-family".clone(alloc), (×4 styles)
        font_family: repeatable_string(&config.font_family),
        font_family_bold: repeatable_string(&config.font_family_bold),
        font_family_italic: repeatable_string(&config.font_family_italic),
        font_family_bold_italic: repeatable_string(&config.font_family_bold_italic),
        // .@"font-style" = try config.@"font-style".clone(alloc), (×4 styles)
        font_style: font_style(&config.font_style),
        font_style_bold: font_style(&config.font_style_bold),
        font_style_italic: font_style(&config.font_style_italic),
        font_style_bold_italic: font_style(&config.font_style_bold_italic),
        // .@"font-variation" = try config.@"font-variation".clone(alloc), (×4 styles)
        font_variation: font_variation(&config.font_variation),
        font_variation_bold: font_variation(&config.font_variation_bold),
        font_variation_italic: font_variation(&config.font_variation_italic),
        font_variation_bold_italic: font_variation(&config.font_variation_bold_italic),
        // .@"font-codepoint-map" = try config.@"font-codepoint-map".clone(alloc),
        //
        // PORT NOTE: `CodepointMap` is the same rustty-font type on both
        // sides; deep-cloned exactly as the config crate's own `clone_in`
        // (Global aborts on OOM — the reference returns the error).
        font_codepoint_map: font_config::RepeatableCodepointMap {
            map: config
                .font_codepoint_map
                .map
                .clone(Allocator)
                .expect("CodepointMap.clone: Global allocator aborts on OOM"),
        },
        // .@"font-synthetic-style" = config.@"font-synthetic-style",
        font_synthetic_style: rustty_font_shaper::collection::FontSyntheticStyle {
            bold: config.font_synthetic_style.bold,
            italic: config.font_synthetic_style.italic,
            bold_italic: config.font_synthetic_style.bold_italic,
        },
        // .@"adjust-cell-width" = config.@"adjust-cell-width", (×13 metrics —
        // `Modifier` is the same rustty-font type on both sides, `Copy`)
        adjust_cell_width: config.adjust_cell_width,
        adjust_cell_height: config.adjust_cell_height,
        adjust_font_baseline: config.adjust_font_baseline,
        adjust_underline_position: config.adjust_underline_position,
        adjust_underline_thickness: config.adjust_underline_thickness,
        adjust_strikethrough_position: config.adjust_strikethrough_position,
        adjust_strikethrough_thickness: config.adjust_strikethrough_thickness,
        adjust_overline_position: config.adjust_overline_position,
        adjust_overline_thickness: config.adjust_overline_thickness,
        adjust_cursor_thickness: config.adjust_cursor_thickness,
        adjust_cursor_height: config.adjust_cursor_height,
        adjust_box_thickness: config.adjust_box_thickness,
        adjust_icon_height: config.adjust_icon_height,
        // .@"freetype-load-flags" = if (FreetypeLoadFlags != void) config.@"freetype-load-flags" else {},
        //
        // PORT NOTE: on macOS the shaper's `FreetypeLoadFlags` is `()` (the
        // reference `void` arm — exact). On freetype targets the shaper
        // currently carries its `_private` scaffold (face/mod.rs), so the
        // config value flows when that DI lands with the Linux freetype
        // backend; the default is the placeholder until then.
        freetype_load_flags: rustty_font_shaper::face::FREETYPE_LOAD_FLAGS_DEFAULT,
    }
}

/// PORT NOTE (GROWTH POINT, Rust-only glue): the shared *inferred*-error
/// union of the Surface method family — the reference methods App reaches
/// (`handleMessage`/`performBindingAction`) and the chunk-owned helpers they
/// call return `!void`/`!bool` with error sets inferred from the bodies.
/// The C7 fill grew the former opaque unit marker into the union of the
/// concrete sources its bodies reach (the app.rs `Error` union precedent);
/// the remaining chunk-owned bodies (C8/C9/C14/C15) keep growing it at each
/// growth point.
// (C8 dropped the C7 `Clone`/`Copy`/`Eq` derives: the new `FontGrid` payload
// — `shared_grid_set::Error` — is Debug-only, the InitError precedent.)
#[derive(Debug)]
pub enum Error {
    /// The apprt seam edges (`rt_app.performAction`, `rt_surface.
    /// getCursorPos`) — the opaque apprt-api growth-point marker.
    Apprt(apprt::ApprtError),
    /// The xev `Async.notify` leaves (`searchCallback_`'s renderer wakeups,
    /// `queueRender`) — the D6 placeholder seam.
    Xev(rustty_xev::NotifyError),
    /// vt terminal/highlight storage failures (`searchCallback_`'s
    /// `Flattened.clone`, `mouseRefreshLinks`'s `selectionString`) — Zig
    /// `Allocator.Error` surfaced as vt `OutOfMemory` (the vt convention).
    OutOfMemory(terminal::pagelist::OutOfMemory),
    /// `font.SharedGridSet.ref` (`setFontSize` — C8).
    FontGrid(shared_grid_set::Error),
    /// `rendererpkg.Message.initChangeConfig` (`updateConfig` — C8) — the
    /// renderer message.rs growth-point marker (config-coupled placeholder).
    InitChangeConfig(rendererpkg::message::InitChangeConfigError),
    /// `std.unicode.Utf8View.init`'s `error.InvalidUtf8`
    /// (`preeditCallback` — C8).
    Utf8(std::str::Utf8Error),
}

impl From<apprt::ApprtError> for Error {
    fn from(err: apprt::ApprtError) -> Error {
        Error::Apprt(err)
    }
}

impl From<rustty_xev::NotifyError> for Error {
    fn from(err: rustty_xev::NotifyError) -> Error {
        Error::Xev(err)
    }
}

impl From<terminal::pagelist::OutOfMemory> for Error {
    fn from(err: terminal::pagelist::OutOfMemory) -> Error {
        Error::OutOfMemory(err)
    }
}

impl From<shared_grid_set::Error> for Error {
    fn from(err: shared_grid_set::Error) -> Error {
        Error::FontGrid(err)
    }
}

impl From<rendererpkg::message::InitChangeConfigError> for Error {
    fn from(err: rendererpkg::message::InitChangeConfigError) -> Error {
        Error::InitChangeConfig(err)
    }
}

impl From<std::str::Utf8Error> for Error {
    fn from(err: std::str::Utf8Error) -> Error {
        Error::Utf8(err)
    }
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::Apprt(err) => err.fmt(f),
            Error::Xev(err) => write!(f, "core surface operation failed: {err:?}"),
            Error::OutOfMemory(err) => write!(f, "core surface operation failed: {err:?}"),
            Error::FontGrid(err) => write!(f, "core surface operation failed: {err:?}"),
            Error::InitChangeConfig(err) => write!(f, "core surface operation failed: {err:?}"),
            Error::Utf8(err) => err.fmt(f),
        }
    }
}

impl std::error::Error for Error {}

/// Errors from [`Surface::recomputeInitialSize`].
// const InitialSizeError = error{
//     ContentScaleUnavailable,
//     AppActionFailed,
// };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitialSizeError {
    // ContentScaleUnavailable,
    ContentScaleUnavailable,
    // AppActionFailed,
    AppActionFailed,
}

/// PORT NOTE (Rust-only glue): [`Surface::init`]'s reference return is `!void`
/// with an *inferred* error union — the merge of every `try` source in the
/// body. Typed as the union of the concrete sources so Zig `try` maps to
/// `?`/[`Surface::initTry`] (the app.rs `Error` union precedent).
#[derive(Debug)]
pub enum InitError {
    /// `DerivedConfig.init` — the config link `oniRegex()` compile failures.
    Oni(oni::Error),
    /// The placeholder GPU renderer entry points (`Renderer.surfaceInit`/
    /// `init`/`finalizeSurfaceInit`, renderer `DerivedConfig.init`, `draw`'s
    /// `drawFrame`) — Phase-3 R-d (CORE_BLUEPRINT §5).
    Renderer(crate::renderer_seam::RendererError),
    /// The apprt seam (`getContentScale`/`getSize`/`performAction`) — the
    /// opaque apprt-api growth marker.
    Apprt(apprt::ApprtError),
    /// `font.SharedGridSet.ref`.
    FontGrid(shared_grid_set::Error),
    /// The xev handle inits (renderer `Thread.init`, `termio.Thread.init`,
    /// `termio.Mailbox.initSPSC`) — the D6 placeholder seam.
    Xev(rustty_xev::Error),
    /// `termio.Exec.init`.
    TermioExec(termio::exec::InitError),
    /// `termio.Termio.init` — the termio growth-point marker.
    Termio(termio::Error),
    /// `std.Thread.spawn` (the renderer/io thread spawns).
    Spawn(std::io::Error),
    /// The C7/C8-owned Surface methods init reaches (`resize`) — the opaque
    /// [`Error`] growth marker.
    Surface(Error),
}

impl From<oni::Error> for InitError {
    fn from(err: oni::Error) -> InitError {
        InitError::Oni(err)
    }
}

impl From<crate::renderer_seam::RendererError> for InitError {
    fn from(err: crate::renderer_seam::RendererError) -> InitError {
        InitError::Renderer(err)
    }
}

impl From<apprt::ApprtError> for InitError {
    fn from(err: apprt::ApprtError) -> InitError {
        InitError::Apprt(err)
    }
}

impl From<shared_grid_set::Error> for InitError {
    fn from(err: shared_grid_set::Error) -> InitError {
        InitError::FontGrid(err)
    }
}

impl From<rustty_xev::Error> for InitError {
    fn from(err: rustty_xev::Error) -> InitError {
        InitError::Xev(err)
    }
}

impl From<termio::exec::InitError> for InitError {
    fn from(err: termio::exec::InitError) -> InitError {
        InitError::TermioExec(err)
    }
}

impl From<termio::Error> for InitError {
    fn from(err: termio::Error) -> InitError {
        InitError::Termio(err)
    }
}

impl From<std::io::Error> for InitError {
    fn from(err: std::io::Error) -> InitError {
        InitError::Spawn(err)
    }
}

impl From<Error> for InitError {
    fn from(err: Error) -> InitError {
        InitError::Surface(err)
    }
}

impl core::fmt::Display for InitError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            InitError::Oni(err) => write!(f, "surface init: {err:?}"),
            InitError::Renderer(err) => err.fmt(f),
            InitError::Apprt(err) => err.fmt(f),
            InitError::FontGrid(err) => write!(f, "surface init: {err:?}"),
            InitError::Xev(err) => write!(f, "surface init: {err:?}"),
            InitError::TermioExec(err) => write!(f, "surface init: {err:?}"),
            InitError::Termio(err) => err.fmt(f),
            InitError::Spawn(err) => err.fmt(f),
            InitError::Surface(err) => err.fmt(f),
        }
    }
}

impl std::error::Error for InitError {}

/// PORT NOTE ([ADV-FIX 6b], Rust-only glue): `Surface.init`'s
/// `std.Thread.spawn(.{}, rendererpkg.Thread.threadMain,
/// .{&self.renderer_thread})` (Surface.zig:715) captures a raw
/// `*mut renderer_seam::Thread<A>`, which is `!Send`. This documented
/// Send-wrapper newtype carries the reference threading contract: the pointer
/// is into the stable Surface allocation (init's "must be stable" doc
/// contract), and the renderer thread is the SOLE user of `*thread` between
/// the spawn and the `deinit` join (`renderer_thread.stop.notify()` +
/// `renderer_thr.join()`, Surface.zig:796-798).
struct RendererThreadArg<A: apprt::App + 'static>(*mut crate::renderer_seam::Thread<A>);

// SAFETY: see the newtype doc — stable allocation, sole-user-until-join.
unsafe impl<A: apprt::App + 'static> Send for RendererThreadArg<A> {}

/// PORT NOTE ([ADV-FIX 6b], Rust-only glue): the io-thread spawn tuple
/// `.{ &self.io_thread, &self.io }` (Surface.zig:726) under the same
/// stable-allocation/sole-user contract (the io thread owns both pointees
/// between the spawn and the `deinit` join — `io_thread.stop.notify()` +
/// `io_thr.join()`, Surface.zig:806-808).
struct IoThreadArgs<A: apprt::App + 'static> {
    thread: *mut termio::Thread,
    io: *mut termio::Termio<SurfaceMailbox<A>>,
}

// SAFETY: see the newtype doc — stable allocation, sole-user-until-join.
unsafe impl<A: apprt::App + 'static> Send for IoThreadArgs<A> {}

/// PORT NOTE ([ADV 6.1], Rust-only glue): the reference's paired
/// `crash.sentry.thread_state = self.crashThreadState();
/// defer crash.sentry.thread_state = null;` sites (13×, Surface.zig:2469..
/// 4708 — sizeCallback/preeditCallback are C8's two) map the Zig `defer` onto
/// `Drop` — the guard clears the crash thread-local on EVERY exit path (`?`
/// propagation included), declared right after the set so it drops LAST at
/// each site (the reference's LIFO defer position).
struct ThreadStateGuard;

impl Drop for ThreadStateGuard {
    fn drop(&mut self) {
        // defer crash.sentry.thread_state = null;
        crash::sentry::clearThreadState();
    }
}

/// PORT NOTE ([`ThreadStateGuard`] family, Rust-only glue): `keyCallback`'s
/// inspector defer block (`defer if (insp_ev) |ev| ev: {...}`, Surface.zig:
/// 2674-2689) must run on EVERY exit path (`?` propagation included), so it
/// maps onto `Drop`. The guard owns the reference's `insp_ev` local (the
/// `if (insp_ev) |*ev| ev else null` argument sites are `.ev.as_mut()`
/// reborrows) and holds a raw surface pointer for the drop-time inspector
/// recheck. Reachability contract (CORE_BLUEPRINT §4): `ev` is `Some` ONLY
/// when an inspector existed at keyCallback entry, which is impossible until
/// Phase 3 (`Inspector` is uninhabited) — so the raw pointer is never
/// dereferenced today; when it is (Phase 3), the deref happens strictly
/// AFTER the method body's last `&mut self` use (the guard drops at return —
/// the rust-mut-args raw-ptr discipline).
struct InspEvGuard<A: apprt::App + 'static> {
    surface: *mut Surface<A>,
    ev: Option<inspectorpkg::KeyEvent>,
}

impl<A: apprt::App + 'static> Drop for InspEvGuard<A> {
    fn drop(&mut self) {
        // defer if (insp_ev) |ev| ev: {
        if let Some(mut ev) = self.ev.take() {
            // We have to check for the inspector again because our keybinding
            // might close it.
            // const insp = self.inspector orelse {
            //     ev.deinit(self.alloc);
            //     break :ev;
            // };
            let _insp = match unsafe { (*self.surface).inspector } {
                Some(v) => v,
                None => {
                    ev.deinit(unsafe { (*self.surface).alloc });
                    return;
                }
            };

            // if (insp.recordKeyEvent(self.alloc, ev)) {
            //     self.queueRender() catch {};
            // } else |err| {
            //     log.warn("error adding key event to inspector err={}", .{err});
            // }
            //
            // PORT NOTE (CORE_BLUEPRINT §4): the Some-arm needs the Phase-3
            // `Inspector.recordKeyEvent` (+ the queueRender kick) — the
            // pointee is uninhabited, so this arm is unreachable until the
            // Phase-3 fill (the deinit/activateInspector arm precedent).
            unreachable!("inspector is unconstructible until Phase 3 (recordKeyEvent)");
        }
    }
}

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
            Err(_err) => {
                // log.warn("failed to apply conditional state to config err={}", .{err});
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
                // original config's memory; the owned rustty value clones the
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

        // Initialize our renderer with our initialized surface.
        // try Renderer.surfaceInit(rt_surface);
        if let Err(err) = Renderer::surfaceInit(rt_surface) {
            // (errdefer derived_config.deinit() — Surface.zig:490)
            derived_config.deinit();
            return Err(err.into());
        }

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

        // The font size we desire along with the DPI determined for the surface
        // const font_size: font.face.DesiredSize = .{
        //     .points = config.@"font-size",
        //     .xdpi = @intFromFloat(x_dpi),
        //     .ydpi = @intFromFloat(y_dpi),
        // };
        let font_size: DesiredSize = DesiredSize {
            points: config.font_size,
            xdpi: x_dpi as u16,
            ydpi: y_dpi as u16,
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
        let renderer_config = match crate::renderer_seam::DerivedConfig::init(alloc, config) {
            Ok(v) => v,
            Err(err) => {
                // (errdefer derived_config.deinit() — Surface.zig:490)
                derived_config.deinit();
                return Err(err.into());
            }
        };
        let mut renderer_impl = match Renderer::init(
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
                thread: unsafe { &raw mut (*self_).renderer_thread },
            },
        ) {
            Ok(v) => v,
            Err(err) => {
                // (errdefer derived_config.deinit() — Surface.zig:490; the
                // renderer DerivedConfig moved into the failed init's options)
                derived_config.deinit();
                return Err(err.into());
            }
        };
        // errdefer renderer_impl.deinit();
        // (fired explicitly at the failure sites below / by initErrdefers.)

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
        //     app_mailbox,
        // );
        let render_thread = match crate::renderer_seam::Thread::<A>::init(
            alloc,
            config,
            rt_surface,
            unsafe { &raw mut (*self_).renderer },
            unsafe { &raw mut (*self_).renderer_state },
            app_mailbox,
        ) {
            Ok(v) => v,
            Err(err) => {
                // (the armed errdefers, LIFO — Surface.zig:564/559/490)
                // errdefer alloc.destroy(mutex);
                unsafe { drop(Box::from_raw(mutex)) };
                // errdefer renderer_impl.deinit();
                renderer_impl.deinit();
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
        let renderer_wakeup = render_thread.wakeup;
        let renderer_mailbox = render_thread.mailbox;

        // Create the IO thread
        // var io_thread = try termio.Thread.init(alloc);
        //
        // (the ported `termio::Thread::init` collapses the `alloc` param —
        // its PORT NOTE.)
        let io_thread = match termio::Thread::init() {
            Ok(v) => v,
            Err(err) => {
                // (the armed errdefers, LIFO — Surface.zig:575/564/559/490)
                // errdefer render_thread.deinit();
                let mut render_thread = render_thread;
                render_thread.deinit();
                // errdefer alloc.destroy(mutex);
                unsafe { drop(Box::from_raw(mutex)) };
                // errdefer renderer_impl.deinit();
                renderer_impl.deinit();
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
        // child_exited=false, focused=true, selection_scroll_active=false,
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
            std::ptr::write(&raw mut (*self_).renderer_thread, render_thread);
            // .renderer_state = .{
            //     .mutex = mutex,
            //     .terminal = &self.io.terminal,
            // },
            //
            // (the `&self.io.terminal` address is taken while `io` is still
            // undefined — an address-only read, exactly the reference; the
            // State declaration defaults: inspector=null, preedit=null,
            // mouse=.{}.)
            std::ptr::write(
                &raw mut (*self_).renderer_state,
                rendererpkg::state::State {
                    mutex,
                    terminal: &raw mut (*self_).io.terminal,
                    inspector: None,
                    preedit: None,
                    mouse: rendererpkg::state::Mouse::default(),
                },
            );
            // .renderer_thr = undefined,
            // (§8.5 Option shape: None until the spawn below.)
            std::ptr::write(&raw mut (*self_).renderer_thr, None);
            // .mouse = .{},
            std::ptr::write(&raw mut (*self_).mouse, Mouse::default());
            // .keyboard = .{},
            std::ptr::write(&raw mut (*self_).keyboard, Keyboard::default());
            // .io = undefined,
            // (placement-written by the io block below — no Rust `undefined`.)
            // .io_thread = io_thread,
            std::ptr::write(&raw mut (*self_).io_thread, io_thread);
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
        // memory); the owned rustty `Command` clones the bytes (the config
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
                Err(_err) => {
                    // If an error occurs, we don't want to block surface startup.
                    // log.warn("error getting env map for surface err={}", .{err});
                    internal_os::env::getEnvMap()
                }
            };
            // errdefer env.deinit();
            // (ownership drop covers — the map moves into the Exec config.)

            // don't leak GHOSTTY_LOG to any subprocesses
            // env.remove("GHOSTTY_LOG");
            //
            // PORT (R2 identity): the reference's `GHOSTTY_*` env names carry
            // the product identity — `<APP_NAME upper>_<suffix>` (the termio
            // shell_integration `env_name` precedent).
            env.remove(&format!("{}_LOG", rustty_identity::APP_NAME.to_uppercase()));

            // var buf: [18]u8 = undefined;
            // try env.put(
            //     "GHOSTTY_SURFACE_ID",
            //     std.fmt.bufPrint(&buf, "0x{x:0>16}", .{self.id}) catch unreachable,
            // );
            //
            // ("0x" + 16 zero-padded hex digits fills the [18]u8 exactly; the
            // `catch unreachable` collapses — `format!` is infallible.)
            env.insert(
                format!("{}_SURFACE_ID", rustty_identity::APP_NAME.to_uppercase()),
                format!("0x{:016x}", unsafe { (*self_).id }),
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
                        .map(|v| v.as_bytes().to_vec()),
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
            // PORT NOTE: the reference out-pointer init is the Rust
            // constructor return (the termio_impl PORT NOTE); the returned
            // value is placement-written into the uninitialized `self.io`
            // field. (`termio::DerivedConfig::init` is infallible — the
            // reference `try` collapses, its PORT NOTE.)
            let io = match termio::Termio::init(
                alloc,
                termio::Options {
                    size,
                    full_config: config,
                    config: termio::DerivedConfig::init(config),
                    backend: termio::Backend::Exec(io_exec),
                    mailbox: io_mailbox,
                    renderer_state: unsafe { &raw mut (*self_).renderer_state },
                    renderer_wakeup,
                    renderer_mailbox,
                    surface_mailbox: SurfaceMailbox {
                        surface: self_,
                        app: app_mailbox,
                    },
                },
            ) {
                Ok(v) => v,
                Err(err) => {
                    // (errdefer io_mailbox.deinit(alloc) + errdefer
                    // io_exec.deinit() — both moved into the consumed options;
                    // + the armed errdefer chain — initErrdefers)
                    unsafe { Self::initErrdefers(self_, false) };
                    return Err(err.into());
                }
            };
            unsafe { std::ptr::write(&raw mut (*self_).io, io) };
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
                    apprt::Target::Surface(&*self_),
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
        let _ = unsafe {
            Self::initTry(
                self_,
                true,
                (*rt_app).perform_action(
                    apprt::Target::Surface(&*self_),
                    apprt::Action::SizeLimit(SizeLimit {
                        min_width: size.cell.width * MIN_WINDOW_WIDTH_CELLS,
                        min_height: size.cell.height * MIN_WINDOW_HEIGHT_CELLS,
                        // No max:
                        max_width: 0,
                        max_height: 0,
                    }),
                ),
            )
        }?;

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

        // Give the renderer one more opportunity to finalize any surface
        // setup on the main thread prior to spinning up the rendering thread.
        // try renderer_impl.finalizeSurfaceInit(rt_surface);
        //
        // PORT NOTE: the reference calls through its LOCAL renderer copy
        // after the `self.*` write (Zig value-copy; both copies share the
        // underlying GPU handles) — Rust single ownership calls through the
        // moved-into field.
        unsafe {
            Self::initTry(
                self_,
                true,
                (*self_).renderer.finalizeSurfaceInit(rt_surface),
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
        let arg = RendererThreadArg::<A>(unsafe { &raw mut (*self_).renderer_thread });
        let spawn_result = std::thread::Builder::new()
            .name("renderer".to_string())
            .spawn(move || {
                // (capture the whole Send wrapper, not the raw-ptr field —
                // edition-2021 disjoint captures would otherwise capture the
                // `!Send` pointer directly.)
                let arg = arg;
                unsafe { (*arg.0).threadMain() }
            });
        let renderer_thr = unsafe { Self::initTry(self_, true, spawn_result) }?;
        unsafe { (*self_).renderer_thr = Some(renderer_thr) };

        // Start our IO thread
        // self.io_thr = try std.Thread.spawn(
        //     .{},
        //     termio.Thread.threadMain,
        //     .{ &self.io_thread, &self.io },
        // );
        // self.io_thr.setName("io") catch {};
        //
        // (same [ADV-FIX 6b] newtype + Builder-name mapping as above.)
        let args = IoThreadArgs::<A> {
            thread: unsafe { &raw mut (*self_).io_thread },
            io: unsafe { &raw mut (*self_).io },
        };
        let spawn_result = std::thread::Builder::new()
            .name("io".to_string())
            .spawn(move || {
                // (whole-wrapper capture — same note as the renderer spawn
                // above.)
                let args = args;
                unsafe { (*args.thread).threadMain(&mut *args.io) }
            });
        let io_thr = unsafe { Self::initTry(self_, true, spawn_result) }?;
        unsafe { (*self_).io_thr = Some(io_thr) };

        // Determine our initial window size if configured. We need to do this
        // quite late in the process because our height/width are in grid dimensions,
        // so we need to know our cell sizes first.
        //
        // Note: it is important to do this after the renderer is setup above.
        // This allows the apprt to fully initialize the surface before we
        // start messing with the window.
        // self.recomputeInitialSize() catch |err| { ... };
        if let Err(_err) = unsafe { (*self_).recomputeInitialSize() } {
            // We don't treat this as a fatal error because not setting
            // an initial size shouldn't stop our terminal from working.
            // log.warn("unable to set initial window size: {}", .{err});
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
                        apprt::Target::Surface(&*self_),
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
                            apprt::Target::Surface(&*self_),
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
                                    apprt::Target::Surface(&*self_),
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
            // errdefer self.io.deinit(); (Surface.zig:682 — armed only after
            // the io block; consuming receiver, read back out.)
            if io_live {
                std::ptr::read(&raw const (*self_).io).deinit();
            }
            // errdefer io_thread.deinit(); (Surface.zig:579 — consuming
            // receiver, read back out.)
            std::ptr::read(&raw const (*self_).io_thread).deinit();
            // errdefer render_thread.deinit(); (Surface.zig:575)
            (*self_).renderer_thread.deinit();
            // errdefer alloc.destroy(mutex); (Surface.zig:564)
            drop(Box::from_raw((*self_).renderer_state.mutex));
            // errdefer renderer_impl.deinit(); (Surface.zig:559)
            (*self_).renderer.deinit();
            // errdefer derived_config.deinit(); (Surface.zig:490 — the
            // explicit regex teardown + the collapsed-arena ownership drop.)
            let mut derived_config = std::ptr::read(&raw const (*self_).config);
            derived_config.deinit();
            drop(derived_config);
            // PORT NOTE (Rust-only ownership): the rustty `SharedGridSet.ref`
            // Key is OWNED (the reference key's memory belongs to the set's
            // map — the shared_grid_set PORT NOTE), so the moved-in copy is
            // dropped to avoid a Rust-side leak the reference doesn't have.
            drop(std::ptr::read(&raw const (*self_).font_grid_key));
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

        // Stop rendering thread
        {
            // self.renderer_thread.stop.notify() catch |err|
            //     log.err("error notifying renderer thread to stop, may stall err={}", .{err});
            if let Err(_err) = self.renderer_thread.stop.notify() {
                // log.err("error notifying renderer thread to stop, may stall err={}", .{err});
            }
            // self.renderer_thr.join();
            //
            // (§8.5 Option shape: Some after init's spawn; join consumes.)
            self.renderer_thr
                .take()
                .expect("renderer_thr is set by init")
                .join()
                .unwrap();

            // We need to become the active rendering thread again
            // self.renderer.threadEnter(self.rt_surface) catch unreachable;
            self.renderer
                .threadEnter(self.rt_surface)
                .unwrap_or_else(|_| unreachable!());
        }

        // Stop our IO thread
        {
            // self.io_thread.stop.notify() catch |err|
            //     log.err("error notifying io thread to stop, may stall err={}", .{err});
            if let Err(_err) = self.io_thread.stop.notify() {
                // log.err("error notifying io thread to stop, may stall err={}", .{err});
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
        self.renderer_thread.deinit();
        // self.renderer.deinit();
        self.renderer.deinit();
        // self.io_thread.deinit();
        //
        // (consuming receiver — read out of the invalidated-in-place self.)
        unsafe { std::ptr::read(&raw const self.io_thread) }.deinit();
        // self.mouse.selection_gesture.deinit(&self.io.terminal);
        self.mouse.selection_gesture.deinit(&mut self.io.terminal);
        // self.io.deinit();
        //
        // (consuming receiver — read out of the invalidated-in-place self.)
        unsafe { std::ptr::read(&raw const self.io) }.deinit();

        // if (self.inspector) |v| {
        //     v.deinit(self.alloc);
        //     self.alloc.destroy(v);
        // }
        //
        // PORT NOTE (CORE_BLUEPRINT §4): `Inspector` is the deferred
        // uninhabited placeholder — the pointer is always `None` until
        // Phase 3 (`activateInspector`, the only constructor, defers verbatim
        // below), so the Some-arm (preserved verbatim above) is
        // unreachable-by-construction until the Phase-3 fill.
        if self.inspector.is_some() {
            unreachable!("inspector is unconstructible until Phase 3 (activateInspector deferred)");
        }

        // Clean up our keyboard state
        // for (self.keyboard.sequence_queued.items) |req| req.deinit();
        //
        // (`WriteReq.deinit` — the alloc-variant free — is Drop in rustty;
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
        if let Some(p) = self.renderer_state.preedit.take() {
            drop(p.codepoints);
        }
        // self.alloc.destroy(self.renderer_state.mutex);
        unsafe { drop(Box::from_raw(self.renderer_state.mutex)) };
        // self.config.deinit();
        //
        // (explicit regex teardown + the collapsed-arena ownership drop —
        // read out of the invalidated-in-place self.)
        let mut config = unsafe { std::ptr::read(&raw const self.config) };
        config.deinit();
        drop(config);

        // log.info("surface closed addr={x}", .{@intFromPtr(self)});
    }

    /// Close this surface. This will trigger the runtime to start the
    /// close process, which should ultimately deinitialize this surface.
    // pub fn close(self: *Surface) void {
    pub fn close(&mut self) {
        // self.rt_surface.close(self.needsConfirmQuit());
        let process_alive = self.needsConfirmQuit();
        unsafe { &*self.rt_surface }.close(process_alive);
    }

    /// Returns a mailbox that can be used to send messages to this surface.
    // inline fn surfaceMailbox(self: *Surface) Mailbox {
    //
    // PORT NOTE: reference `inline fn` -> `#[inline]`.
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

    /// Queue a message for the IO thread.
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
    fn queueIo(&mut self, msg: termio::Message<'static>, mutex: termio::termio_impl::MutexState) {
        // In readonly mode, we don't allow any writes through to the pty.
        // if (self.readonly) {
        if self.readonly {
            // switch (msg) {
            match msg {
                // .write_small,
                // .write_stable,
                // .write_alloc,
                // => return,
                termio::Message::WriteSmall(_)
                | termio::Message::WriteStable(_)
                | termio::Message::WriteAlloc(_) => return,

                // else => {},
                _ => {}
            }
        }

        // self.io.queueMessage(msg, mutex);
        self.io.queueMessage(msg, mutex);
    }

    /// Forces the surface to render. This is useful for when the surface
    /// is in the middle of animation (such as a resize, etc.) or when
    /// the render timer is managed manually by the apprt.
    // pub fn draw(self: *Surface) !void {
    //
    // (the inferred `!void` is `drawFrame`'s error — the placeholder
    // `RendererError`, Phase-3 R-d.)
    pub fn draw(&mut self) -> Result<(), crate::renderer_seam::RendererError> {
        // Renderers are required to support `drawFrame` being called from
        // the main thread, so that they can update contents during resize.
        // try self.renderer.drawFrame(true);
        self.renderer.drawFrame(true)?;
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
        unimplemented!(
            "Surface.activateInspector deferred (Phase-3 inspector, CORE_BLUEPRINT §4); \
             verbatim body above"
        )
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
        unimplemented!(
            "Surface.deactivateInspector deferred (Phase-3 inspector, CORE_BLUEPRINT §4); \
             verbatim body above"
        )
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
                unsafe { (*self.renderer_state.mutex).lock() };
                // break :true !self.io.terminal.cursorIsAtPrompt();
                let result = !self.io.terminal.cursor_is_at_prompt();
                unsafe { (*self.renderer_state.mutex).unlock() };
                result
            }
        }
    }

    /// Called from the app thread to handle mailbox messages to our specific
    /// surface.
    //
    // pub fn handleMessage(self: *Surface, msg: Message) !void {
    //
    // PORT NOTE: `Message<'_>` (not `'static`): the one borrowed variant,
    // `ChangeConfig(&Config)`, is DIRECT-dispatched by `App.updateConfig`,
    // never queued (apprt-api `AppMailbox` note). The inferred `!void` is the
    // [`Error`] union.
    pub fn handleMessage(&mut self, msg: Message<'_>) -> Result<(), Error> {
        // switch (msg) {
        match msg {
            // .change_config => |config| try self.updateConfig(config),
            Message::ChangeConfig(config) => self.updateConfig(config)?,

            // .set_title => |*v| {
            Message::SetTitle(v) => {
                // We ignore the message in case the title was set via config.
                // if (self.config.title != null) {
                if self.config.title.is_some() {
                    // log.debug("ignoring title change request since static title is set via config", .{});
                    return Ok(());
                }

                // The ptrCast just gets sliceTo to return the proper type.
                // We know that our title should end in 0.
                // const slice = std.mem.sliceTo(@as([*:0]const u8, @ptrCast(v)), 0);
                let slice = &v[..v.iter().position(|&b| b == 0).unwrap_or(v.len())];
                // log.debug("changing title \"{s}\"", .{slice});
                // _ = try self.rt_app.performAction(
                //     .{ .surface = self },
                //     .set_title,
                //     .{ .title = slice },
                // );
                let _ = unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::Surface(&*self),
                    apprt::Action::SetTitle(SetTitle { title: slice }),
                )?;
            }

            // .report_title => |style| report_title: {
            Message::ReportTitle(style) => 'report_title: {
                // if (!self.config.title_report) {
                if !self.config.title_report {
                    // log.info("report_title requested, but disabled via config", .{});
                    break 'report_title;
                }

                // const title: ?[:0]const u8 = self.rt_surface.getTitle();
                let title: Option<&std::ffi::CStr> = unsafe { &*self.rt_surface }.get_title();
                // const data = switch (style) {
                //     .csi_21_t => try std.fmt.allocPrint(
                //         self.alloc,
                //         "\x1b]l{s}\x1b\\",
                //         .{title orelse ""},
                //     ),
                // };
                //
                // PORT NOTE: `allocPrint` over the raw title bytes -> an owned
                // byte Vec (the title is a raw C string — never a lossy
                // String); the OOM-only `try` collapses (infallible Rust
                // allocation).
                let data: Vec<u8> = match style {
                    apprt::ReportTitleStyle::Csi21t => {
                        let mut data = Vec::new();
                        data.extend_from_slice(b"\x1b]l");
                        // (`title orelse ""`)
                        data.extend_from_slice(match title {
                            Some(v) => v.to_bytes(),
                            None => &[],
                        });
                        data.extend_from_slice(b"\x1b\\");
                        data
                    }
                };

                // We always use an allocating message because we don't know
                // the length of the title and this isn't a performance critical
                // path.
                // self.queueIo(.{
                //     .write_alloc = .{
                //         .alloc = self.alloc,
                //         .data = data,
                //     },
                // }, .unlocked);
                //
                // (the `.alloc` field collapses into the `Alloc` payload's Box
                // ownership — the datastruct message_data PORT NOTE.)
                self.queueIo(
                    termio::Message::WriteAlloc(Alloc {
                        data: data.into_boxed_slice(),
                    }),
                    termio::termio_impl::MutexState::Unlocked,
                );
            }

            // .color_change => |change| color_change: {
            Message::ColorChange(change) => 'color_change: {
                // Notify our apprt, but don't send a mode 2031 DSR report
                // because VT sequences were used to change the color.
                // _ = try self.rt_app.performAction(
                //     .{ .surface = self },
                //     .color_change,
                //     .{ ... },
                // );
                let _ = unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::Surface(&*self),
                    apprt::Action::ColorChange(ColorChange {
                        // .kind = switch (change.target) {
                        kind: match change.target {
                            // .palette => |v| @enumFromInt(v),
                            terminal::osc::color::Target::Palette(v) => ColorKind(i32::from(v)),
                            // .dynamic => |dyn| switch (dyn) {
                            terminal::osc::color::Target::Dynamic(dynamic) => match dynamic {
                                // .foreground => .foreground,
                                terminal::color::Dynamic::Foreground => ColorKind::FOREGROUND,
                                // .background => .background,
                                terminal::color::Dynamic::Background => ColorKind::BACKGROUND,
                                // .cursor => .cursor,
                                terminal::color::Dynamic::Cursor => ColorKind::CURSOR,
                                // Unsupported dynamic color change notification type
                                // else => break :color_change,
                                _ => break 'color_change,
                            },
                            // Special colors aren't supported for change notification
                            // .special => break :color_change,
                            terminal::osc::color::Target::Special(_) => break 'color_change,
                        },
                        // .r = change.color.r,
                        r: change.color.r,
                        // .g = change.color.g,
                        g: change.color.g,
                        // .b = change.color.b,
                        b: change.color.b,
                    }),
                )?;
            }

            // .set_mouse_shape => |shape| {
            Message::SetMouseShape(shape) => {
                // log.debug("changing mouse shape: {}", .{shape});
                // _ = try self.rt_app.performAction(
                //     .{ .surface = self },
                //     .mouse_shape,
                //     shape,
                // );
                let _ = unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::Surface(&*self),
                    apprt::Action::MouseShape(shape),
                )?;
            }

            // .clipboard_read => |clipboard| {
            Message::ClipboardRead(clipboard) => {
                // if (self.config.clipboard_read == .deny) {
                if self.config.clipboard_read == configpkg::value_types::ClipboardAccess::Deny {
                    // log.info("application attempted to read clipboard, but 'clipboard-read' is set to deny", .{});
                    return Ok(());
                }

                // _ = try self.startClipboardRequest(.standard, .{ .osc_52_read = clipboard });
                let _ = self.startClipboardRequest(
                    apprt::Clipboard::Standard,
                    apprt::ClipboardRequest::Osc52Read(clipboard),
                )?;
            }

            // .clipboard_write => |w| switch (w.req) {
            Message::ClipboardWrite {
                clipboard_type,
                req,
            } => match req {
                // .small => |v| try self.clipboardWrite(v.data[0..v.len], w.clipboard_type),
                MessageData::Small(v) => self.clipboardWrite(&v.data[0..v.len], clipboard_type)?,
                // .stable => |v| try self.clipboardWrite(v, w.clipboard_type),
                MessageData::Stable(v) => self.clipboardWrite(v, clipboard_type)?,
                // .alloc => |v| {
                MessageData::Alloc(v) => {
                    // defer v.alloc.free(v.data);
                    // (ownership drop at arm scope end — the collapsed
                    // `Alloc.alloc` Box ownership.)
                    // try self.clipboardWrite(v.data, w.clipboard_type);
                    self.clipboardWrite(&v.data, clipboard_type)?;
                }
            },

            // .pwd_change => |w| {
            Message::PwdChange(w) => {
                // defer w.deinit();
                // (`WriteReq.deinit` — the alloc-variant free — is Drop in
                // rustty; runs at arm scope end, the datastruct precedent.)

                // We always allocate for this because we need to null-terminate.
                // const str = try self.alloc.dupeZ(u8, w.slice());
                // defer self.alloc.free(str);
                //
                // PORT NOTE: the apprt-api `Pwd` payload carries raw bytes
                // (`&[u8]`); the reference's NUL-termination (`dupeZ`) is part
                // of the deferred C-ABI pinning (the action.rs `PwdC` PORT
                // NOTE). The owned dupe + scope-end drop realize the
                // alloc/free pair (Zig defer LIFO == Rust reverse-declaration
                // drop: `str` before `w`).
                let str_ = w.slice().to_vec();

                // _ = try self.rt_app.performAction(
                //     .{ .surface = self },
                //     .pwd,
                //     .{ .pwd = str },
                // );
                let _ = unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::Surface(&*self),
                    apprt::Action::Pwd(Pwd { pwd: &str_ }),
                )?;
            }

            // .close => self.close(),
            Message::Close => self.close(),

            // .child_exited => |v| self.childExited(v),
            Message::ChildExited(v) => self.childExited(v),

            // .desktop_notification => |notification| {
            Message::DesktopNotification { title, body } => {
                // if (!self.config.desktop_notifications) {
                if !self.config.desktop_notifications {
                    // log.info("application attempted to display a desktop notification, but 'desktop-notifications' is disabled", .{});
                    return Ok(());
                }

                // const title = std.mem.sliceTo(&notification.title, 0);
                let title = &title[..title.iter().position(|&b| b == 0).unwrap_or(title.len())];
                // const body = std.mem.sliceTo(&notification.body, 0);
                let body = &body[..body.iter().position(|&b| b == 0).unwrap_or(body.len())];
                // try self.showDesktopNotification(title, body);
                self.showDesktopNotification(title, body)?;
            }

            // .renderer_health => |health| self.updateRendererHealth(health),
            Message::RendererHealth(health) => self.updateRendererHealth(health),

            // .scrollbar => |scrollbar| self.updateScrollbar(scrollbar),
            Message::Scrollbar(scrollbar) => self.updateScrollbar(scrollbar),

            // .present_surface => try self.presentSurface(),
            Message::PresentSurface => self.presentSurface()?,

            // .password_input => |v| try self.passwordInput(v),
            Message::PasswordInput(v) => self.passwordInput(v)?,

            // .ring_bell => bell: {
            Message::RingBell => 'bell: {
                // const now = std.time.Instant.now() catch unreachable;
                //
                // (Rust `Instant::now` is infallible — the `catch unreachable`
                // collapses.)
                let now = std::time::Instant::now();
                // if (self.last_bell_time) |last| {
                if let Some(last) = self.last_bell_time {
                    // if (now.since(last) < 100 * std.time.ns_per_ms) break :bell;
                    //
                    // (`Instant.since` -> `duration_since` nanoseconds; the
                    // config_struct.rs NS_PER_MS precedent.)
                    const NS_PER_MS: u128 = 1_000_000;
                    if now.duration_since(last).as_nanos() < 100 * NS_PER_MS {
                        break 'bell;
                    }
                }
                // self.last_bell_time = now;
                self.last_bell_time = Some(now);
                // _ = self.rt_app.performAction(
                //     .{ .surface = self },
                //     .ring_bell,
                //     {},
                // ) catch |err| {
                //     log.warn("apprt failed to ring bell={}", .{err});
                // };
                if let Err(_err) = unsafe { &*self.rt_app }
                    .perform_action(apprt::Target::Surface(&*self), apprt::Action::RingBell)
                {
                    // log.warn("apprt failed to ring bell={}", .{err});
                }
            }

            // .progress_report => |v| {
            Message::ProgressReport(v) => {
                // _ = self.rt_app.performAction(
                //     .{ .surface = self },
                //     .progress_report,
                //     v,
                // ) catch |err| {
                //     log.warn("apprt failed to report progress err={}", .{err});
                // };
                if let Err(_err) = unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::Surface(&*self),
                    apprt::Action::ProgressReport(v),
                ) {
                    // log.warn("apprt failed to report progress err={}", .{err});
                }
            }

            // .selection_scroll_tick => |active| {
            Message::SelectionScrollTick(active) => {
                // self.selection_scroll_active = active;
                self.selection_scroll_active = active;
                // try self.selectionScrollTick();
                self.selectionScrollTick()?;
            }

            // .start_command => {
            Message::StartCommand => {
                // self.command_timer = try .now();
                //
                // (Rust `Instant::now` is infallible — the `try` collapses.)
                self.command_timer = Some(std::time::Instant::now());
            }

            // .stop_command => |v| timer: {
            Message::StopCommand(v) => 'timer: {
                // const end: std.time.Instant = try .now();
                let end: std::time::Instant = std::time::Instant::now();
                // const start = self.command_timer orelse break :timer;
                let start = match self.command_timer {
                    Some(start) => start,
                    None => break 'timer,
                };
                // self.command_timer = null;
                self.command_timer = None;

                // const duration: Duration = .{ .duration = end.since(start) };
                let duration: Duration = Duration {
                    duration: end.duration_since(start).as_nanos() as u64,
                };
                // log.debug("command took {f}", .{duration});

                // _ = self.rt_app.performAction(
                //     .{ .surface = self },
                //     .command_finished,
                //     .{
                //         .exit_code = v,
                //         .duration = duration,
                //     },
                // ) catch |err| {
                //     log.warn("apprt failed to notify command finish={}", .{err});
                // };
                if let Err(_err) = unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::Surface(&*self),
                    apprt::Action::CommandFinished(CommandFinished {
                        exit_code: v,
                        duration,
                    }),
                ) {
                    // log.warn("apprt failed to notify command finish={}", .{err});
                }
            }

            // .search_total => |v| {
            Message::SearchTotal(v) => {
                // _ = try self.rt_app.performAction(
                //     .{ .surface = self },
                //     .search_total,
                //     .{ .total = v },
                // );
                let _ = unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::Surface(&*self),
                    apprt::Action::SearchTotal(SearchTotal { total: v }),
                )?;
            }

            // .search_selected => |v| {
            Message::SearchSelected(v) => {
                // _ = try self.rt_app.performAction(
                //     .{ .surface = self },
                //     .search_selected,
                //     .{ .selected = v },
                // );
                let _ = unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::Surface(&*self),
                    apprt::Action::SearchSelected(SearchSelected { selected: v }),
                )?;
            }
        }

        Ok(())
    }

    // fn selectionScrollTick(self: *Surface) !void {
    fn selectionScrollTick(&mut self) -> Result<(), Error> {
        // If we're no longer active then we don't do anything.
        // if (!self.selection_scroll_active) return;
        if !self.selection_scroll_active {
            return Ok(());
        }

        // If our gesture doesn't want autoscrolling then disable it.
        // const was_autoscrolling = self.mouse.selection_gesture.left_drag_autoscroll != .none;
        let was_autoscrolling = self.mouse.selection_gesture.left_drag_autoscroll
            != terminal::selection_gesture::Autoscroll::None;
        // if (!was_autoscrolling) {
        if !was_autoscrolling {
            // self.queueIo(
            //     .{ .selection_scroll = false },
            //     .unlocked,
            // );
            self.queueIo(
                termio::Message::SelectionScroll(false),
                termio::termio_impl::MutexState::Unlocked,
            );
            return Ok(());
        }

        // const pos = try self.rt_surface.getCursorPos();
        let pos = unsafe { &*self.rt_surface }.get_cursor_pos()?;
        // const pos_vp = self.posToViewport(pos.x, pos.y);
        //
        // (Zig widens the f32 cursor coordinates to the f64 params.)
        let pos_vp = self.posToViewport(f64::from(pos.x), f64::from(pos.y));

        // We need our locked state for the remainder
        // self.renderer_state.mutex.lock();
        // defer self.renderer_state.mutex.unlock();
        //
        // (guard-less zigstd Mutex — CORE_BLUEPRINT §3; the defer is the
        // explicit unlock at every exit below.)
        unsafe { (*self.renderer_state.mutex).lock() };
        // const t: *terminal.Terminal = self.renderer_state.terminal;
        let t: *mut Terminal = self.renderer_state.terminal;

        // const selection = self.mouse.selection_gesture.autoscrollTick(t, .{
        //     .viewport = pos_vp,
        //     .xpos = pos.x,
        //     .ypos = pos.y,
        //     .rectangle = SurfaceMouse.isRectangleSelectState(self.mouse.mods),
        //     .word_boundary_codepoints = self.config.selection_word_chars,
        //     .geometry = .{
        //         .columns = @intCast(self.size.grid().columns),
        //         .cell_width = self.size.cell.width,
        //         .padding_left = self.size.padding.left,
        //         .screen_height = self.size.screen.height,
        //     },
        // });
        let selection = self.mouse.selection_gesture.autoscroll_tick(
            unsafe { &mut *t },
            terminal::selection_gesture::AutoscrollTick {
                viewport: pos_vp,
                xpos: f64::from(pos.x),
                ypos: f64::from(pos.y),
                rectangle: crate::surface_mouse::SurfaceMouse::is_rectangle_select_state(
                    self.mouse.mods,
                ),
                word_boundary_codepoints: &self.config.selection_word_chars,
                geometry: terminal::selection_gesture::Geometry {
                    columns: u32::from(self.size.grid().columns),
                    cell_width: self.size.cell.width,
                    padding_left: self.size.padding.left,
                    screen_height: self.size.screen.height,
                },
            },
        );

        // If we're no longer autoscrolling for whatever reason, disable it.
        // if (self.mouse.selection_gesture.left_drag_autoscroll == .none) {
        if self.mouse.selection_gesture.left_drag_autoscroll
            == terminal::selection_gesture::Autoscroll::None
        {
            // self.queueIo(
            //     .{ .selection_scroll = false },
            //     .locked,
            // );
            self.queueIo(
                termio::Message::SelectionScroll(false),
                termio::termio_impl::MutexState::Locked,
            );
        }

        // If our left click was invalidated, ignore the result. This isn't
        // strictly necessary but its a nice to have.
        // if (self.mouse.selection_gesture.left_click_count == 0) return;
        if self.mouse.selection_gesture.left_click_count == 0 {
            // (the deferred mutex unlock)
            unsafe { (*self.renderer_state.mutex).unlock() };
            return Ok(());
        }

        // We modified our viewport and selection so we need to queue
        // a render.
        // try self.setSelection(selection);
        if let Err(err) = self.setSelection(selection) {
            // (the deferred mutex unlock — the error exit)
            unsafe { (*self.renderer_state.mutex).unlock() };
            return Err(err);
        }
        // try self.queueRender();
        if let Err(err) = self.queueRender() {
            // (the deferred mutex unlock — the error exit)
            unsafe { (*self.renderer_state.mutex).unlock() };
            return Err(err.into());
        }

        // (the deferred mutex unlock — the normal exit)
        unsafe { (*self.renderer_state.mutex).unlock() };
        Ok(())
    }

    // fn childExited(self: *Surface, info: apprt.surface.Message.ChildExited) void {
    //
    // PARITY: not `.unwrap_or(false)` on the two performAction results —
    // each Err arm IS the reference's labeled `catch |err| gui:` block with
    // its log site (the init conditional-state precedent).
    #[allow(clippy::manual_unwrap_or, clippy::manual_unwrap_or_default)]
    fn childExited(&mut self, info: apprt::surface::ChildExited) {
        // Mark our flag that we exited immediately
        // self.child_exited = true;
        self.child_exited = true;

        // If our runtime was below some threshold then we assume that this
        // was an abnormal exit and we show an error message.
        // if (info.runtime_ms <= self.config.abnormal_command_exit_runtime_ms) runtime: {
        if info.runtime_ms <= u64::from(self.config.abnormal_command_exit_runtime_ms) {
            'runtime: {
                // On macOS, our exit code detection doesn't work, possibly
                // because of our `login` wrapper. More investigation required.
                // if (comptime !builtin.target.os.tag.isDarwin()) {
                if !cfg!(target_vendor = "apple") {
                    // If the exit code is 0 then it was a good exit.
                    // if (info.exit_code == 0) break :runtime;
                    if info.exit_code == 0 {
                        break 'runtime;
                    }
                }

                // log.warn("abnormal process exit detected, showing error message", .{});

                // Try and show a GUI message. If it returns true, don't do anything else.
                // if (self.rt_app.performAction(
                //     .{ .surface = self },
                //     .show_child_exited,
                //     info,
                // ) catch |err| gui: {
                //     log.err("error trying to show native child exited GUI err={}", .{err});
                //     break :gui false;
                // }) return;
                if match unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::Surface(&*self),
                    apprt::Action::ShowChildExited(info),
                ) {
                    Ok(shown) => shown,
                    Err(_err) => {
                        // log.err("error trying to show native child exited GUI err={}", .{err});
                        // break :gui false;
                        false
                    }
                } {
                    return;
                }

                // If a native GUI notification was not shown, update our terminal to
                // note the abnormal exit.
                // self.childExitedAbnormally(info) catch |err| {
                //     log.err("error handling abnormal child exit err={}", .{err});
                //     return;
                // };
                if let Err(_err) = self.childExitedAbnormally(info) {
                    // log.err("error handling abnormal child exit err={}", .{err});
                    return;
                }

                return;
            }
        }

        // We output a message so that the user knows what's going on and
        // doesn't think their terminal just froze. We show this unconditionally
        // on close even if `wait_after_command` is false and the surface closes
        // immediately because if a user does an `undo` to restore a closed
        // surface then they will see this message and know the process has
        // completed.
        // terminal: {
        'terminal: {
            // First try and show a native GUI message.
            // if (self.rt_app.performAction(
            //     .{ .surface = self },
            //     .show_child_exited,
            //     info,
            // ) catch |err| gui: {
            //     log.err("error trying to show native child exited GUI err={}", .{err});
            //     break :gui false;
            // }) break :terminal;
            if match unsafe { &*self.rt_app }.perform_action(
                apprt::Target::Surface(&*self),
                apprt::Action::ShowChildExited(info),
            ) {
                Ok(shown) => shown,
                Err(_err) => {
                    // log.err("error trying to show native child exited GUI err={}", .{err});
                    // break :gui false;
                    false
                }
            } {
                break 'terminal;
            }

            // If the native GUI can't be shown, display a text message in the
            // terminal.
            // self.renderer_state.mutex.lock();
            // defer self.renderer_state.mutex.unlock();
            //
            // (guard-less zigstd Mutex — the defer is the explicit unlock at
            // every exit of this block.)
            unsafe { (*self.renderer_state.mutex).lock() };
            // const t: *terminal.Terminal = self.renderer_state.terminal;
            let t: *mut Terminal = self.renderer_state.terminal;
            // t.carriageReturn();
            unsafe { (*t).carriage_return() };
            // t.linefeed() catch break :terminal;
            if unsafe { (*t).linefeed() }.is_err() {
                unsafe { (*self.renderer_state.mutex).unlock() };
                break 'terminal;
            }
            // t.printString("Process exited. Press any key to close the terminal.") catch
            //     break :terminal;
            if unsafe { (*t).print_string(b"Process exited. Press any key to close the terminal.") }
                .is_err()
            {
                unsafe { (*self.renderer_state.mutex).unlock() };
                break 'terminal;
            }
            // t.modes.set(.cursor_visible, false);
            unsafe { (*t).modes.set(terminal::modes::Mode::CursorVisible, false) };

            // We also want to ensure that normal keyboard encoding is on
            // so that we can close the terminal. We close the terminal on
            // any key press that encodes a character.
            // t.modes.set(.disable_keyboard, false);
            unsafe {
                (*t).modes
                    .set(terminal::modes::Mode::DisableKeyboard, false)
            };
            // t.screens.active.kitty_keyboard.set(.set, .disabled);
            unsafe {
                (*(*t).screens.active).kitty_keyboard.set(
                    terminal::kitty::key::SetMode::Set,
                    terminal::kitty::KeyFlags::DISABLED,
                )
            };

            // (the deferred mutex unlock — the normal exit of the block)
            unsafe { (*self.renderer_state.mutex).unlock() };
        }

        // Waiting after command we stop here. The terminal is updated, our
        // state is updated, and now its up to the user to decide what to do.
        // if (self.config.wait_after_command) return;
        if self.config.wait_after_command {
            return;
        }

        // If we aren't waiting after the command, then we exit immediately
        // with no confirmation.
        // self.close();
        self.close();
    }

    /// Called when the child process exited abnormally.
    //
    // fn childExitedAbnormally(
    //     self: *Surface,
    //     info: apprt.surface.Message.ChildExited,
    // ) !void {
    //
    // PORT NOTE: the inferred `!void` error set is the vt terminal storage
    // failures (`setAttribute`/`plainString`/`linefeed`/`print`/`printString`
    // — Zig `Allocator.Error` surfaced as vt `OutOfMemory`); the
    // join/allocPrint OOMs collapse (infallible Rust allocation).
    #[allow(clippy::redundant_closure_call)]
    fn childExitedAbnormally(
        &mut self,
        info: apprt::surface::ChildExited,
    ) -> Result<(), terminal::pagelist::OutOfMemory> {
        // var arena = ArenaAllocator.init(self.alloc);
        // defer arena.deinit();
        // const alloc = arena.allocator();
        //
        // PORT NOTE (collapsed arena): the allocations below are owned values
        // that free on drop (the C5 DerivedConfig arena-collapse precedent);
        // the arena's allocator maps to the surface allocator.
        let alloc = self.alloc;

        // Build up our command for the error message
        // const command = try std.mem.join(alloc, " ", switch (self.io.backend) {
        //     .exec => |*exec| exec.subprocess.args,
        // });
        let command: Vec<u8> = match &self.io.backend {
            termio::Backend::Exec(exec) => exec
                .subprocess
                .args
                .iter()
                .map(|arg| arg.to_bytes().to_vec())
                .collect::<Vec<Vec<u8>>>()
                .join(&b' '),
        };
        // const runtime_str = try std.fmt.allocPrint(alloc, "{d} ms", .{info.runtime_ms});
        let runtime_str = format!("{} ms", info.runtime_ms);

        // self.renderer_state.mutex.lock();
        // defer self.renderer_state.mutex.unlock();
        //
        // (guard-less zigstd Mutex — the deferred unlock is realized ONCE at
        // the single exit below; the `?`-propagating body is scoped in an
        // immediately-invoked closure so the unlock also runs on the error
        // paths — the vt search/screen.rs error-scoping precedent.)
        unsafe { (*self.renderer_state.mutex).lock() };
        // const t: *terminal.Terminal = self.renderer_state.terminal;
        let t: *mut Terminal = self.renderer_state.terminal;

        let result = (|| -> Result<(), terminal::pagelist::OutOfMemory> {
            unsafe {
                // No matter what move the cursor back to the column 0.
                // t.carriageReturn();
                (*t).carriage_return();

                // Reset styles
                // try t.setAttribute(.{ .unset = {} });
                (*t).set_attribute(terminal::sgr::Attribute::Unset)?;

                // If there is data in the viewport, we want to scroll down
                // a little bit and write a horizontal rule before writing
                // our message. This lets the use see the error message the
                // command may have output.
                // const viewport_str = try t.plainString(alloc);
                let viewport_str = (*t).plain_string(alloc)?;
                // if (viewport_str.len > 0) {
                if !viewport_str.is_empty() {
                    // try t.linefeed();
                    (*t).linefeed()?;
                    // for (0..t.cols) |_| try t.print(0x2501);
                    for _ in 0..(*t).cols {
                        (*t).print(0x2501)?;
                    }
                    // t.carriageReturn();
                    (*t).carriage_return();
                    // try t.linefeed();
                    (*t).linefeed()?;
                    // try t.linefeed();
                    (*t).linefeed()?;
                }

                // Output our error message
                // try t.setAttribute(.{ .@"8_fg" = .bright_red });
                (*t).set_attribute(terminal::sgr::Attribute::Color8Fg(
                    terminal::color::Name::BrightRed,
                ))?;
                // try t.setAttribute(.{ .bold = {} });
                (*t).set_attribute(terminal::sgr::Attribute::Bold)?;
                // try t.printString("Ghostty failed to launch the requested command:");
                //
                // PORT (R2 identity): the product name in the user-facing
                // message — `rustty_identity::APP_NAME`.
                (*t).print_string(
                    format!(
                        "{} failed to launch the requested command:",
                        rustty_identity::APP_NAME
                    )
                    .as_bytes(),
                )?;
                // try t.setAttribute(.{ .unset = {} });
                (*t).set_attribute(terminal::sgr::Attribute::Unset)?;

                // t.carriageReturn();
                (*t).carriage_return();
                // try t.linefeed();
                (*t).linefeed()?;
                // try t.linefeed();
                (*t).linefeed()?;
                // try t.printString(command);
                (*t).print_string(&command)?;
                // try t.setAttribute(.{ .unset = {} });
                (*t).set_attribute(terminal::sgr::Attribute::Unset)?;

                // t.carriageReturn();
                (*t).carriage_return();
                // try t.linefeed();
                (*t).linefeed()?;
                // try t.linefeed();
                (*t).linefeed()?;
                // try t.printString("Runtime: ");
                (*t).print_string(b"Runtime: ")?;
                // try t.setAttribute(.{ .@"8_fg" = .red });
                (*t).set_attribute(terminal::sgr::Attribute::Color8Fg(
                    terminal::color::Name::Red,
                ))?;
                // try t.printString(runtime_str);
                (*t).print_string(runtime_str.as_bytes())?;
                // try t.setAttribute(.{ .unset = {} });
                (*t).set_attribute(terminal::sgr::Attribute::Unset)?;

                // We don't print this on macOS because the exit code is always 0
                // due to the way we launch the process.
                // if (comptime !builtin.target.os.tag.isDarwin()) {
                if !cfg!(target_vendor = "apple") {
                    // const exit_code_str = try std.fmt.allocPrint(alloc, "{d}", .{info.exit_code});
                    let exit_code_str = format!("{}", info.exit_code);
                    // t.carriageReturn();
                    (*t).carriage_return();
                    // try t.linefeed();
                    (*t).linefeed()?;
                    // try t.printString("Exit Code: ");
                    (*t).print_string(b"Exit Code: ")?;
                    // try t.setAttribute(.{ .@"8_fg" = .red });
                    (*t).set_attribute(terminal::sgr::Attribute::Color8Fg(
                        terminal::color::Name::Red,
                    ))?;
                    // try t.printString(exit_code_str);
                    (*t).print_string(exit_code_str.as_bytes())?;
                    // try t.setAttribute(.{ .unset = {} });
                    (*t).set_attribute(terminal::sgr::Attribute::Unset)?;
                }

                // t.carriageReturn();
                (*t).carriage_return();
                // try t.linefeed();
                (*t).linefeed()?;
                // try t.linefeed();
                (*t).linefeed()?;
                // try t.printString("Press any key to close the window.");
                (*t).print_string(b"Press any key to close the window.")?;

                // Hide the cursor
                // t.modes.set(.cursor_visible, false);
                (*t).modes.set(terminal::modes::Mode::CursorVisible, false);
            }

            Ok(())
        })();

        // (the deferred mutex unlock — runs on both the Ok and Err paths)
        unsafe { (*self.renderer_state.mutex).unlock() };
        result
    }

    /// Called when the terminal detects there is a password input prompt.
    // fn passwordInput(self: *Surface, v: bool) !void {
    fn passwordInput(&mut self, v: bool) -> Result<(), Error> {
        {
            // self.renderer_state.mutex.lock();
            // defer self.renderer_state.mutex.unlock();
            //
            // (guard-less zigstd Mutex — the defer is the explicit unlock at
            // both exits of this block.)
            unsafe { (*self.renderer_state.mutex).lock() };

            // If our password input state is unchanged then we don't
            // waste time doing anything more.
            // const old = self.io.terminal.flags.password_input;
            let old = self.io.terminal.flags.password_input;
            // if (old == v) return;
            if old == v {
                unsafe { (*self.renderer_state.mutex).unlock() };
                return Ok(());
            }

            // self.io.terminal.flags.password_input = v;
            self.io.terminal.flags.password_input = v;

            unsafe { (*self.renderer_state.mutex).unlock() };
        }

        // Notify our apprt so it can do whatever it wants.
        // _ = self.rt_app.performAction(
        //     .{ .surface = self },
        //     .secure_input,
        //     if (v) .on else .off,
        // ) catch |err| {
        //     // We ignore this error because we don't want to fail this
        //     // entire operation just because the apprt failed to set
        //     // the secure input state.
        //     log.warn("apprt failed to set secure input state err={}", .{err});
        // };
        if let Err(_err) = unsafe { &*self.rt_app }.perform_action(
            apprt::Target::Surface(&*self),
            apprt::Action::SecureInput(if v { SecureInput::On } else { SecureInput::Off }),
        ) {
            // We ignore this error because we don't want to fail this
            // entire operation just because the apprt failed to set
            // the secure input state.
            // log.warn("apprt failed to set secure input state err={}", .{err});
        }

        // try self.queueRender();
        self.queueRender()?;
        Ok(())
    }

    // fn searchCallback(event: terminal.search.Thread.Event, ud: ?*anyopaque) void {
    //
    // PORT NOTE: the C14 `performBindingAction` search arm registers this as
    // the vt `search::thread::Options.event_cb` fn pointer (`?*anyopaque` ->
    // the seam's bare `*mut c_void`) — the allow is removed when that
    // callsite lands.
    #[allow(dead_code)]
    fn searchCallback(event: SearchThreadEvent<'_>, ud: *mut core::ffi::c_void) {
        // IMPORTANT: This function is run on the SEARCH THREAD! It is NOT SAFE
        // to access anything other than values that never change on the surface.
        // The surface is guaranteed to be valid for the lifetime of the search
        // thread.
        // const self: *Surface = @ptrCast(@alignCast(ud.?));
        //
        // (`ud.?` — the vt seam models the userdata as a bare pointer, so the
        // unwrap collapses into the non-null registration contract.)
        let self_: *mut Surface<A> = ud.cast();
        // self.searchCallback_(event) catch |err| {
        //     log.warn("error in search callback err={}", .{err});
        // };
        if let Err(_err) = unsafe { &mut *self_ }.searchCallback_(event) {
            // log.warn("error in search callback err={}", .{err});
        }
    }

    // fn searchCallback_(
    //     self: *Surface,
    //     event: terminal.search.Thread.Event,
    // ) !void {
    //
    // (reached only via `searchCallback` above until the C14 registration —
    // same allow.)
    #[allow(dead_code)]
    fn searchCallback_(&mut self, event: SearchThreadEvent<'_>) -> Result<(), Error> {
        // NOTE: This runs on the search thread.

        // switch (event) {
        match event {
            // .viewport_matches => |matches_unowned| {
            SearchThreadEvent::ViewportMatches(matches_unowned) => {
                // var arena: ArenaAllocator = .init(self.alloc);
                // errdefer arena.deinit();
                // const alloc = arena.allocator();
                //
                // PORT NOTE (collapsed arena): the renderer `SearchMatches`
                // payload owns its Vec (the renderer message.rs arena-collapse
                // note); the errdefer is the Vec drop on the `?` unwind.

                // const matches = try alloc.dupe(terminal.highlight.Flattened, matches_unowned);
                // for (matches) |*m| m.* = try m.clone(alloc);
                //
                // (the shallow dupe + in-place deep clone collapse into one
                // clone-into-Vec pass — same elements, same failure points.)
                let mut matches: Vec<terminal::highlight::Flattened> =
                    Vec::with_capacity(matches_unowned.len());
                for m in matches_unowned {
                    matches.push(m.clone()?);
                }

                // _ = self.renderer_thread.mailbox.push(
                //     .{ .search_viewport_matches = .{
                //         .arena = arena,
                //         .matches = matches,
                //     } },
                //     .forever,
                // );
                let _ = unsafe { &*self.renderer_thread.mailbox }.push(
                    rendererpkg::Message::SearchViewportMatches(
                        rendererpkg::message::SearchMatches { matches },
                    ),
                    Timeout::Forever,
                );
                // try self.renderer_thread.wakeup.notify();
                self.renderer_thread.wakeup.notify()?;
            }

            // .selected_match => |selected_| {
            SearchThreadEvent::SelectedMatch(selected_) => {
                // if (selected_) |sel| {
                if let Some(sel) = selected_ {
                    // Copy the flattened match.
                    // var arena: ArenaAllocator = .init(self.alloc);
                    // errdefer arena.deinit();
                    // const alloc = arena.allocator();
                    // const match = try sel.highlight.clone(alloc);
                    //
                    // (same collapsed arena as above; `match` is a Rust
                    // keyword — the renderer message.rs `r#match` note.)
                    let match_ = sel.highlight.clone()?;

                    // _ = self.renderer_thread.mailbox.push(
                    //     .{ .search_selected_match = .{
                    //         .arena = arena,
                    //         .match = match,
                    //     } },
                    //     .forever,
                    // );
                    let _ = unsafe { &*self.renderer_thread.mailbox }.push(
                        rendererpkg::Message::SearchSelectedMatch(Some(
                            rendererpkg::message::SearchMatch { r#match: match_ },
                        )),
                        Timeout::Forever,
                    );

                    // Send the selected index to the surface mailbox
                    // _ = self.surfaceMailbox().push(
                    //     .{ .search_selected = sel.idx },
                    //     .forever,
                    // );
                    let _ = self
                        .surfaceMailbox()
                        .push(Message::SearchSelected(Some(sel.idx)), Timeout::Forever);
                } else {
                    // Reset our selected match
                    // _ = self.renderer_thread.mailbox.push(
                    //     .{ .search_selected_match = null },
                    //     .forever,
                    // );
                    let _ = unsafe { &*self.renderer_thread.mailbox }.push(
                        rendererpkg::Message::SearchSelectedMatch(None),
                        Timeout::Forever,
                    );

                    // Reset the selected index
                    // _ = self.surfaceMailbox().push(
                    //     .{ .search_selected = null },
                    //     .forever,
                    // );
                    let _ = self
                        .surfaceMailbox()
                        .push(Message::SearchSelected(None), Timeout::Forever);
                }

                // try self.renderer_thread.wakeup.notify();
                self.renderer_thread.wakeup.notify()?;
            }

            // .total_matches => |total| {
            SearchThreadEvent::TotalMatches(total) => {
                // _ = self.surfaceMailbox().push(
                //     .{ .search_total = total },
                //     .forever,
                // );
                //
                // (Zig coerces the `usize` into the `?usize` payload.)
                let _ = self
                    .surfaceMailbox()
                    .push(Message::SearchTotal(Some(total)), Timeout::Forever);
            }

            // When we quit, tell our renderer to reset any search state.
            // .quit => {
            SearchThreadEvent::Quit => {
                // _ = self.renderer_thread.mailbox.push(
                //     .{ .search_selected_match = null },
                //     .forever,
                // );
                let _ = unsafe { &*self.renderer_thread.mailbox }.push(
                    rendererpkg::Message::SearchSelectedMatch(None),
                    Timeout::Forever,
                );
                // _ = self.renderer_thread.mailbox.push(
                //     .{ .search_viewport_matches = .{
                //         .arena = .init(self.alloc),
                //         .matches = &.{},
                //     } },
                //     .forever,
                // );
                let _ = unsafe { &*self.renderer_thread.mailbox }.push(
                    rendererpkg::Message::SearchViewportMatches(
                        rendererpkg::message::SearchMatches {
                            matches: Vec::new(),
                        },
                    ),
                    Timeout::Forever,
                );
                // try self.renderer_thread.wakeup.notify();
                self.renderer_thread.wakeup.notify()?;

                // Reset search totals in the surface
                // _ = self.surfaceMailbox().push(
                //     .{ .search_total = null },
                //     .forever,
                // );
                let _ = self
                    .surfaceMailbox()
                    .push(Message::SearchTotal(None), Timeout::Forever);
                // _ = self.surfaceMailbox().push(
                //     .{ .search_selected = null },
                //     .forever,
                // );
                let _ = self
                    .surfaceMailbox()
                    .push(Message::SearchSelected(None), Timeout::Forever);
            }

            // Unhandled, so far.
            // .complete => {},
            SearchThreadEvent::Complete => {}
        }

        Ok(())
    }

    /// Call this when modifiers change. This is safe to call even if modifiers
    /// match the previous state.
    ///
    /// This is not publicly exported because modifier changes happen implicitly
    /// on mouse callbacks, key callbacks, etc.
    ///
    /// The renderer state mutex MUST NOT be held.
    // fn modsChanged(self: *Surface, mods: input.Mods) void {
    //
    // PORT NOTE: the C10 keyCallback (:2720) callsite is live; the C12
    // mouseButtonCallback (:3774) / C13 cursorPosCallback (:4558) callsites
    // land later.
    fn modsChanged(&mut self, mods: Mods) {
        // The only place we keep track of mods currently is on the mouse.
        // if (!self.mouse.mods.equal(mods)) {
        if !self.mouse.mods.equal(mods) {
            // The mouse mods only contain binding modifiers since we don't
            // want caps/num lock or sided modifiers to affect the mouse.
            // self.mouse.mods = mods.binding();
            self.mouse.mods = mods.binding();

            // We also need to update the renderer so it knows if it should
            // highlight links. Additionally, mark the screen as dirty so
            // that the highlight state of all links is properly updated.
            {
                // self.renderer_state.mutex.lock();
                // defer self.renderer_state.mutex.unlock();
                //
                // (guard-less zigstd Mutex — CORE_BLUEPRINT §3; the defer is
                // the explicit unlock at the block end.)
                unsafe { (*self.renderer_state.mutex).lock() };
                // self.renderer_state.mouse.mods = self.mouseModsWithCapture(self.mouse.mods);
                self.renderer_state.mouse.mods = self.mouseModsWithCapture(self.mouse.mods);

                // We use the clear screen dirty flag to force a rebuild of all
                // rows because changing mouse mods can affect the highlight state
                // of a link. If there is no link this seems very wasteful but
                // its really only one frame so it's not so bad.
                // self.renderer_state.terminal.flags.dirty.clear = true;
                unsafe { (*self.renderer_state.terminal).flags.dirty.clear = true };

                // (the deferred mutex unlock — block end)
                unsafe { (*self.renderer_state.mutex).unlock() };
            }

            // self.queueRender() catch |err| {
            //     // Not a big deal if this fails.
            //     log.warn("failed to notify renderer of mods change err={}", .{err});
            // };
            if let Err(_err) = self.queueRender() {
                // Not a big deal if this fails.
                // log.warn("failed to notify renderer of mods change err={}", .{err});
            }
        }
    }

    /// Call this whenever the mouse moves or mods changed. The time
    /// at which this is called may matter for the correctness of other
    /// mouse events (see cursorPosCallback) but this is shared logic
    /// for multiple events.
    // fn mouseRefreshLinks(
    //     self: *Surface,
    //     pos: apprt.CursorPos,
    //     pos_vp: terminal.point.Coordinate,
    //     over_link: bool,
    // ) !void {
    //
    // PORT NOTE: the C10 keyCallback (:2733) callsite is live; the C13
    // cursorPosCallback (:4609) callsite lands later (both call with the
    // renderer mutex held). The inferred `!void` is the [`Error`] union.
    fn mouseRefreshLinks(
        &mut self,
        pos: apprt::CursorPos,
        pos_vp: terminal::point::Coordinate,
        over_link: bool,
    ) -> Result<(), Error> {
        // If the position is outside our viewport, do nothing
        // if (pos.x < 0 or pos.y < 0) return;
        if pos.x < 0.0 || pos.y < 0.0 {
            return Ok(());
        }

        // Update the last point that we checked for links so we don't
        // recheck if the mouse moves some pixels to the same point.
        // self.mouse.link_point = pos_vp;
        self.mouse.link_point = Some(pos_vp);

        // We use an arena for everything below to make things easy to clean up.
        // In the case we don't do any allocs this is very cheap to setup
        // (effectively just struct init).
        // var arena = ArenaAllocator.init(self.alloc);
        // defer arena.deinit();
        // const alloc = arena.allocator();
        //
        // PORT NOTE (collapsed arena): the allocations below are owned values
        // that free on drop (the DerivedConfig arena-collapse precedent).
        let alloc = self.alloc;

        // Get our link at the current position. This returns null if there
        // isn't a link OR if we shouldn't be showing links for some reason
        // (see further comments for cases).
        // const link_: ?apprt.action.MouseOverLink, const preview: bool = link: {
        //
        // PORT NOTE: the Zig multi-value labeled break is a Rust tuple; the
        // `MouseOverLink` payload BORROWS (`url: &[u8]`), so the block yields
        // the OWNED url bytes (the collapsed arena's allocation) and the
        // `.{ .url = ... }` literal is realized at the performAction sites
        // below.
        let (link_, preview): (Option<Vec<u8>>, bool) = 'link: {
            // If we clicked and our mouse moved cells then we never
            // highlight links until the mouse is unclicked. This follows
            // standard macOS and Linux behavior where a click and drag cancels
            // mouse actions.
            // const left_idx = @intFromEnum(input.MouseButton.left);
            let left_idx = input_mouse::Button::Left as usize;
            // if (self.mouse.click_state[left_idx] == .press) click: {
            if self.mouse.click_state[left_idx] == input_mouse::ButtonState::Press {
                'click: {
                    // const pin = self.mouse.activeLeftClickPin(&self.io.terminal.screens) orelse break :click;
                    let pin = match self.mouse.activeLeftClickPin(&self.io.terminal.screens) {
                        Some(v) => v,
                        None => break 'click,
                    };
                    // const click_pt = self.io.terminal.screens.active.pages.pointFromPin(
                    //     .viewport,
                    //     pin.*,
                    // ) orelse break :click;
                    let click_pt = match unsafe {
                        (*self.io.terminal.screens.active)
                            .pages
                            .point_from_pin(terminal::point::Tag::Viewport, *pin)
                    } {
                        Some(v) => v,
                        None => break 'click,
                    };

                    // if (!click_pt.coord().eql(pos_vp)) {
                    if !click_pt.coord().eql(pos_vp) {
                        // log.debug("mouse moved while left click held, ignoring link hover", .{});
                        // break :link .{ null, false };
                        break 'link (None, false);
                    }
                }
            }

            // const link = (try self.linkAtPos(pos)) orelse break :link .{ null, false };
            let link = match self.linkAtPos(pos)? {
                Some(v) => v,
                None => break 'link (None, false),
            };
            // switch (link.action) {
            match link.action {
                // .open => {
                link::Action::Open => {
                    // const str = try self.io.terminal.screens.active.selectionString(alloc, .{
                    //     .sel = link.selection,
                    //     .trim = false,
                    // });
                    let str_ = unsafe { &mut *self.io.terminal.screens.active }.selection_string(
                        alloc,
                        terminal::screen::SelectionString {
                            sel: link.selection,
                            trim: false,
                            map: None,
                        },
                    )?;
                    // break :link .{
                    //     .{ .url = str },
                    //     self.config.link_previews == .true,
                    // };
                    break 'link (
                        Some(str_),
                        self.config.link_previews == configpkg::value_types::LinkPreviews::True,
                    );
                }

                // ._open_osc8 => {
                link::Action::OpenOsc8 => {
                    // Show the URL in the status bar
                    // const pin = link.selection.start();
                    let pin = link.selection.start();
                    // const uri = self.osc8URI(pin) orelse {
                    //     log.warn("failed to get URI for OSC8 hyperlink", .{});
                    //     break :link .{ null, false };
                    // };
                    let uri = match self.osc8URI(pin) {
                        Some(v) => v,
                        None => {
                            // log.warn("failed to get URI for OSC8 hyperlink", .{});
                            break 'link (None, false);
                        }
                    };
                    // break :link .{
                    //     .{ .url = try alloc.dupeZ(u8, uri) },
                    //     self.config.link_previews != .false,
                    // };
                    //
                    // (`dupeZ` — an owned copy of the raw URI bytes; the
                    // OOM-only `try` collapses.)
                    break 'link (
                        Some(uri.to_vec()),
                        self.config.link_previews != configpkg::value_types::LinkPreviews::False,
                    );
                }
            }
        };

        // If we found a link, setup our internal state and notify the
        // apprt so it can highlight it.
        // if (link_) |link| {
        if let Some(link) = link_ {
            // self.renderer_state.mouse.point = pos_vp;
            self.renderer_state.mouse.point = Some(pos_vp);
            // self.mouse.over_link = true;
            self.mouse.over_link = true;
            // self.renderer_state.terminal.screens.active.dirty.hyperlink_hover = true;
            unsafe {
                (*(*self.renderer_state.terminal).screens.active)
                    .dirty
                    .hyperlink_hover = true;
            }
            // _ = try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .mouse_shape,
            //     .pointer,
            // );
            let _ = unsafe { &*self.rt_app }.perform_action(
                apprt::Target::Surface(&*self),
                apprt::Action::MouseShape(terminal::mouse::Shape::Pointer),
            )?;

            // if (preview) {
            if preview {
                // _ = try self.rt_app.performAction(
                //     .{ .surface = self },
                //     .mouse_over_link,
                //     link,
                // );
                let _ = unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::Surface(&*self),
                    apprt::Action::MouseOverLink(MouseOverLink { url: &link }),
                )?;
            }

            // try self.queueRender();
            self.queueRender()?;
            // return;
            return Ok(());
        }

        // No link, if we're previously over a link then we need to clear
        // the over-link apprt state.
        // if (over_link) {
        if over_link {
            // _ = try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .mouse_shape,
            //     self.io.terminal.mouse_shape,
            // );
            let _ = unsafe { &*self.rt_app }.perform_action(
                apprt::Target::Surface(&*self),
                apprt::Action::MouseShape(self.io.terminal.mouse_shape),
            )?;
            // _ = try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .mouse_over_link,
            //     .{ .url = "" },
            // );
            let _ = unsafe { &*self.rt_app }.perform_action(
                apprt::Target::Surface(&*self),
                apprt::Action::MouseOverLink(MouseOverLink { url: b"" }),
            )?;
            // try self.queueRender();
            self.queueRender()?;
            // return;
            return Ok(());
        }

        Ok(())
    }

    /// Called when our renderer health state changes.
    // fn updateRendererHealth(self: *Surface, health: rendererpkg.Health) void {
    fn updateRendererHealth(&mut self, health: rendererpkg::Health) {
        // log.warn("renderer health status change status={}", .{health});
        // _ = self.rt_app.performAction(
        //     .{ .surface = self },
        //     .renderer_health,
        //     health,
        // ) catch |err| {
        //     log.warn("failed to notify app of renderer health change err={}", .{err});
        // };
        if let Err(_err) = unsafe { &*self.rt_app }.perform_action(
            apprt::Target::Surface(&*self),
            apprt::Action::RendererHealth(health),
        ) {
            // log.warn("failed to notify app of renderer health change err={}", .{err});
        }
    }

    /// Called when the scrollbar state changes.
    // fn updateScrollbar(self: *Surface, scrollbar: terminal.Scrollbar) void {
    fn updateScrollbar(&mut self, scrollbar: terminal::pagelist::grow::Scrollbar) {
        // _ = self.rt_app.performAction(
        //     .{ .surface = self },
        //     .scrollbar,
        //     scrollbar,
        // ) catch |err| {
        //     log.warn("failed to notify app of scrollbar change err={}", .{err});
        // };
        if let Err(_err) = unsafe { &*self.rt_app }.perform_action(
            apprt::Target::Surface(&*self),
            apprt::Action::Scrollbar(scrollbar),
        ) {
            // log.warn("failed to notify app of scrollbar change err={}", .{err});
        }
    }

    /// This should be called anytime `config_conditional_state` changes
    /// so that the apprt can reload the configuration.
    // fn notifyConfigConditionalState(self: *Surface) void {
    //
    // PORT NOTE: the C13 colorSchemeCallback (:4720) callsite lands later;
    // the allow is removed when it does.
    #[allow(dead_code)]
    fn notifyConfigConditionalState(&mut self) {
        // _ = self.rt_app.performAction(
        //     .{ .surface = self },
        //     .reload_config,
        //     .{ .soft = true },
        // ) catch |err| {
        //     log.warn("failed to notify app of config state change err={}", .{err});
        // };
        if let Err(_err) = unsafe { &*self.rt_app }.perform_action(
            apprt::Target::Surface(&*self),
            apprt::Action::ReloadConfig(ReloadConfig { soft: true }),
        ) {
            // log.warn("failed to notify app of config state change err={}", .{err});
        }
    }

    /// Update our configuration at runtime. This can be called by the apprt
    /// to set a surface-specific configuration that differs from the app
    /// or other surfaces.
    // pub fn updateConfig(
    //     self: *Surface,
    //     original: *const configpkg.Config,
    // ) !void {
    pub fn updateConfig(&mut self, original: &configpkg::Config) -> Result<(), Error> {
        // Apply our conditional state. If we fail to apply the conditional state
        // then we log and attempt to move forward with the old config.
        // var config_: ?configpkg.Config = original.changeConditionalState(
        //     self.config_conditional_state,
        // ) catch |err| err: {
        //     log.warn("failed to apply conditional state to config err={}", .{err});
        //     break :err null;
        // };
        //
        // PARITY: not `.unwrap_or_default()` — the arm IS the reference's
        // labeled catch block with its log site (the init precedent).
        #[allow(clippy::manual_unwrap_or_default)]
        let mut config_: Option<configpkg::Config> =
            match original.change_conditional_state(self.config_conditional_state) {
                Ok(v) => v,
                Err(_err) => {
                    // log.warn("failed to apply conditional state to config err={}", .{err});
                    None
                }
            };
        // defer if (config_) |*c| c.deinit();
        //
        // PORT NOTE: fired explicitly on the success path at the end of this
        // fn; every early-return path maps to `config_`'s Drop (the init
        // precedent).

        // We want a config pointer for everything so we get that either
        // based on our conditional state or the original config.
        // const config: *const configpkg.Config = if (config_) |*c| c else original;
        let config: &configpkg::Config = match config_.as_ref() {
            Some(c) => c,
            None => original,
        };

        // Update our new derived config immediately
        // const derived = DerivedConfig.init(self.alloc, config) catch |err| {
        //     // If the derivation fails then we just log and return. We don't
        //     // hard fail in this case because we don't want to error the surface
        //     // when config fails we just want to keep using the old config.
        //     log.err("error updating configuration err={}", .{err});
        //     return;
        // };
        let derived = match DerivedConfig::init(self.alloc, config) {
            Ok(v) => v,
            Err(_err) => {
                // If the derivation fails then we just log and return. We don't
                // hard fail in this case because we don't want to error the surface
                // when config fails we just want to keep using the old config.
                // log.err("error updating configuration err={}", .{err});
                return Ok(());
            }
        };
        // self.config.deinit();
        self.config.deinit();
        // self.config = derived;
        //
        // (the assignment drops the deinit-ed old value — the initErrdefers
        // deinit-then-drop precedent.)
        self.config = derived;

        // If our mouse is hidden but we disabled mouse hiding, then show it again.
        // if (!self.config.mouse_hide_while_typing and self.mouse.hidden) {
        if !self.config.mouse_hide_while_typing && self.mouse.hidden {
            // self.showMouse();
            self.showMouse();
        }

        // If we are in the middle of a key sequence, clear it.
        // self.endKeySequence(.drop, .free);
        self.endKeySequence(KeySequenceQueued::Drop, KeySequenceMemory::Free);

        // Deactivate all key tables since they may have changed. Importantly,
        // we store pointers into the config as part of our table stack so
        // we can't keep them active across config changes. But this behavior
        // also matches key sequences.
        // _ = self.deactivateAllKeyTables() catch |err| {
        //     log.warn("failed to deactivate key tables err={}", .{err});
        // };
        if let Err(_err) = self.deactivateAllKeyTables() {
            // log.warn("failed to deactivate key tables err={}", .{err});
        }

        // Before sending any other config changes, we give the renderer a new font
        // grid. We could check to see if there was an actual change to the font,
        // but this is easier and pretty rare so it's not a performance concern.
        //
        // (Calling setFontSize builds and sends a new font grid to the renderer.)
        // try self.setFontSize(font_size: {
        self.setFontSize('font_size: {
            // If we have manually adjusted the font size, keep it that way.
            // if (self.font_size_adjusted) {
            if self.font_size_adjusted {
                // log.info("font size manually adjusted, preserving previous size on config reload", .{});
                // break :font_size self.font_size;
                break 'font_size self.font_size;
            }

            // If we haven't, then we update to the configured font size.
            // This allows config changes to update the font size. We used to
            // never do this but it was a common source of confusion and people
            // assumed that Ghostty was broken! This logic makes more sense.
            // var size = self.font_size;
            let mut size = self.font_size;
            // size.points = std.math.clamp(config.@"font-size", 1.0, 255.0);
            size.points = config.font_size.clamp(1.0, 255.0);
            // break :font_size size;
            size
        })?;

        // We need to store our configs in a heap-allocated pointer so that
        // our messages aren't huge.
        // var renderer_message = try rendererpkg.Message.initChangeConfig(self.alloc, config);
        let renderer_message = rendererpkg::Message::initChangeConfig(self.alloc, config)?;
        // errdefer renderer_message.deinit();
        // var termio_config_ptr = try self.alloc.create(termio.Termio.DerivedConfig);
        // errdefer self.alloc.destroy(termio_config_ptr);
        // termio_config_ptr.* = try termio.Termio.DerivedConfig.init(self.alloc, config);
        // errdefer termio_config_ptr.deinit();
        //
        // PORT NOTE: `alloc.create` + the out-write collapse into `Box::new`
        // and the ported `termio::DerivedConfig::init` is infallible (its
        // PORT NOTE), so no fallible site remains inside the errdefers' live
        // ranges — all three collapse into ownership (message/Box drops).
        let termio_config_ptr: Box<termio::DerivedConfig> =
            Box::new(termio::DerivedConfig::init(config));

        // _ = self.renderer_thread.mailbox.push(renderer_message, .{ .forever = {} });
        let _ = unsafe { &*self.renderer_thread.mailbox }.push(renderer_message, Timeout::Forever);
        // self.queueIo(.{
        //     .change_config = .{
        //         .alloc = self.alloc,
        //         .ptr = termio_config_ptr,
        //     },
        // }, .unlocked);
        //
        // (the `.alloc` field collapses into the `ChangeConfig` Box ownership
        // — the termio message.rs PORT NOTE.)
        self.queueIo(
            termio::Message::ChangeConfig {
                ptr: termio_config_ptr,
            },
            termio::termio_impl::MutexState::Unlocked,
        );

        // With mailbox messages sent, we have to wake them up so they process it.
        // self.queueRender() catch |err| {
        //     log.warn("failed to notify renderer of config change err={}", .{err});
        // };
        if let Err(_err) = self.queueRender() {
            // log.warn("failed to notify renderer of config change err={}", .{err});
        }

        // If we have a title set then we update our window to have the
        // newly configured title.
        // if (config.title) |title| _ = try self.rt_app.performAction(
        //     .{ .surface = self },
        //     .set_title,
        //     .{ .title = title },
        // );
        if let Some(title) = &config.title {
            let _ = unsafe { &*self.rt_app }.perform_action(
                apprt::Target::Surface(&*self),
                apprt::Action::SetTitle(SetTitle { title }),
            )?;
        }

        // Notify the window
        // _ = try self.rt_app.performAction(
        //     .{ .surface = self },
        //     .config_change,
        //     .{ .config = config },
        // );
        let _ = unsafe { &*self.rt_app }.perform_action(
            apprt::Target::Surface(&*self),
            apprt::Action::ConfigChange(ConfigChange { config }),
        )?;

        // (the reference `defer if (config_) |*c| c.deinit();` — success
        // path; see the PORT NOTE at the defer site above.)
        if let Some(c) = config_.as_mut() {
            c.deinit();
        }

        Ok(())
    }

    /// Recalculate the initial size of the window based on the
    /// configuration and invoke the apprt `initial_size` action if
    /// necessary.
    // fn recomputeInitialSize(
    //     self: *Surface,
    // ) InitialSizeError!void {
    fn recomputeInitialSize(&mut self) -> Result<(), InitialSizeError> {
        // Both width and height must be set for this to work, as
        // documented on the config options.
        // if (self.config.window_height <= 0 or
        //     self.config.window_width <= 0) return;
        //
        // (`<= 0` on the unsigned reference fields is `== 0`.)
        if self.config.window_height == 0 || self.config.window_width == 0 {
            return Ok(());
        }

        // const scale = self.rt_surface.getContentScale() catch
        //     return error.ContentScaleUnavailable;
        let scale = match unsafe { &*self.rt_surface }.get_content_scale() {
            Ok(v) => v,
            Err(_) => return Err(InitialSizeError::ContentScaleUnavailable),
        };
        // const height = @max(
        //     self.config.window_height,
        //     min_window_height_cells,
        // ) * self.size.cell.height;
        let height = self.config.window_height.max(MIN_WINDOW_HEIGHT_CELLS) * self.size.cell.height;
        // const width = @max(
        //     self.config.window_width,
        //     min_window_width_cells,
        // ) * self.size.cell.width;
        let width = self.config.window_width.max(MIN_WINDOW_WIDTH_CELLS) * self.size.cell.width;
        // const width_f32: f32 = @floatFromInt(width);
        let width_f32: f32 = width as f32;
        // const height_f32: f32 = @floatFromInt(height);
        let height_f32: f32 = height as f32;

        // The final values are affected by content scale and we need to
        // account for the padding so we get the exact correct grid size.
        // const final_width: u32 =
        //     @as(u32, @intFromFloat(@ceil(width_f32 / scale.x))) +
        //     self.size.padding.left +
        //     self.size.padding.right;
        let final_width: u32 =
            (width_f32 / scale.x).ceil() as u32 + self.size.padding.left + self.size.padding.right;
        // const final_height: u32 =
        //     @as(u32, @intFromFloat(@ceil(height_f32 / scale.y))) +
        //     self.size.padding.top +
        //     self.size.padding.bottom;
        let final_height: u32 =
            (height_f32 / scale.y).ceil() as u32 + self.size.padding.top + self.size.padding.bottom;

        // _ = self.rt_app.performAction(
        //     .{ .surface = self },
        //     .initial_size,
        //     .{ .width = final_width, .height = final_height },
        // ) catch return error.AppActionFailed;
        match unsafe { &*self.rt_app }.perform_action(
            apprt::Target::Surface(&*self),
            apprt::Action::InitialSize(InitialSize {
                width: final_width,
                height: final_height,
            }),
        ) {
            Ok(_) => {}
            Err(_) => return Err(InitialSizeError::AppActionFailed),
        }

        Ok(())
    }

    /// Grab the value of text at the given selection point. Note that the
    /// selection structure is used as a way to determine the area of the
    /// screen to read from, it doesn't have to match the user's current
    /// selection state.
    ///
    /// The returned value contains allocated data and must be deinitialized.
    // pub fn dumpText(
    //     self: *Surface,
    //     alloc: Allocator,
    //     sel: terminal.Selection,
    // ) !Text {
    //
    // (the inferred `!Text` error set is `dumpTextLocked`'s — the [`Error`]
    // union.)
    pub fn dumpText(
        &mut self,
        alloc: Allocator,
        sel: terminal::selection::Selection,
    ) -> Result<Text, Error> {
        // self.renderer_state.mutex.lock();
        unsafe { (*self.renderer_state.mutex).lock() };
        // defer self.renderer_state.mutex.unlock();
        //
        // (guard-less zigstd Mutex — CORE_BLUEPRINT §3; the defer is the
        // explicit unlock below.)
        // return try self.dumpTextLocked(alloc, sel);
        let result = self.dumpTextLocked(alloc, sel);
        // (the deferred mutex unlock)
        unsafe { (*self.renderer_state.mutex).unlock() };
        result
    }

    /// Same as `dumpText` but assumes the renderer state mutex is already
    /// held.
    // pub fn dumpTextLocked(
    //     self: *Surface,
    //     alloc: Allocator,
    //     sel: terminal.Selection,
    // ) !Text {
    pub fn dumpTextLocked(
        &mut self,
        alloc: Allocator,
        sel: terminal::selection::Selection,
    ) -> Result<Text, Error> {
        // Read out the text
        // const text = try self.io.terminal.screens.active.selectionString(alloc, .{
        //     .sel = sel,
        //     .trim = false,
        // });
        let text = unsafe { &mut *self.io.terminal.screens.active }.selection_string(
            alloc,
            terminal::screen::SelectionString {
                sel,
                trim: false,
                map: None,
            },
        )?;
        // errdefer alloc.free(text);
        // (the owned `text` drops on any early error return.)

        // Calculate our viewport info if we can.
        // const vp: ?Text.Viewport = viewport: {
        let vp: Option<TextViewport> = 'viewport: {
            // If our bottom right pin is before the viewport, then we can't
            // possibly have this text be within the viewport.
            // const vp_tl_pin = self.io.terminal.screens.active.pages.getTopLeft(.viewport);
            let vp_tl_pin = unsafe {
                (*self.io.terminal.screens.active)
                    .pages
                    .get_top_left(terminal::point::Tag::Viewport)
            };
            // const br_pin = sel.bottomRight(self.io.terminal.screens.active);
            let br_pin = sel.bottom_right(unsafe { &*self.io.terminal.screens.active });
            // if (br_pin.before(vp_tl_pin)) break :viewport null;
            if br_pin.before(vp_tl_pin) {
                break 'viewport None;
            }

            // If our top-left pin is after the viewport, then we can't possibly
            // have this text be within the viewport.
            // const vp_br_pin = self.io.terminal.screens.active.pages.getBottomRight(.viewport) orelse {
            let vp_br_pin = match unsafe {
                (*self.io.terminal.screens.active)
                    .pages
                    .get_bottom_right(terminal::point::Tag::Viewport)
            } {
                Some(v) => v,
                None => {
                    // I don't think this is possible but I don't want to crash on
                    // that assertion so let's just break out...
                    // log.warn("viewport bottom-right pin not found, bug?", .{});
                    break 'viewport None;
                }
            };
            // const tl_pin = sel.topLeft(self.io.terminal.screens.active);
            let tl_pin = sel.top_left(unsafe { &*self.io.terminal.screens.active });
            // if (vp_br_pin.before(tl_pin)) break :viewport null;
            if vp_br_pin.before(tl_pin) {
                break 'viewport None;
            }

            // We established that our top-left somewhere before the viewport
            // bottom-right and that our bottom-right is somewhere after
            // the top-left. This means that at least some portion of our
            // selection is within the viewport.

            // Our top-left point. If it doesn't exist in the viewport it must
            // be before and we can return (0,0).
            // const tl_pt: terminal.Point = self.io.terminal.screens.active.pages.pointFromPin(
            //     .viewport,
            //     tl_pin,
            // ) orelse tl: {
            let tl_pt: terminal::point::Point = match unsafe {
                (*self.io.terminal.screens.active)
                    .pages
                    .point_from_pin(terminal::point::Tag::Viewport, tl_pin)
            } {
                Some(v) => v,
                None => {
                    // if (comptime std.debug.runtime_safety) {
                    //     assert(tl_pin.before(vp_tl_pin));
                    // }
                    //
                    // (the runtime-safety-gated assert IS `debug_assert!` —
                    // the established quirks.inlineAssert mapping.)
                    debug_assert!(tl_pin.before(vp_tl_pin));

                    // break :tl .{ .viewport = .{} };
                    terminal::point::Point::Viewport(terminal::point::Coordinate { x: 0, y: 0 })
                }
            };

            // Our bottom-right point. If it doesn't exist in the viewport
            // it must be the bottom-right of the viewport.
            // const br_pt = self.io.terminal.screens.active.pages.pointFromPin(
            //     .viewport,
            //     br_pin,
            // ) orelse br: {
            let br_pt = match unsafe {
                (*self.io.terminal.screens.active)
                    .pages
                    .point_from_pin(terminal::point::Tag::Viewport, br_pin)
            } {
                Some(v) => v,
                None => {
                    // if (comptime std.debug.runtime_safety) {
                    //     assert(vp_br_pin.before(br_pin));
                    // }
                    debug_assert!(vp_br_pin.before(br_pin));

                    // break :br self.io.terminal.screens.active.pages.pointFromPin(
                    //     .viewport,
                    //     vp_br_pin,
                    // ).?;
                    unsafe {
                        (*self.io.terminal.screens.active)
                            .pages
                            .point_from_pin(terminal::point::Tag::Viewport, vp_br_pin)
                    }
                    .unwrap()
                }
            };

            // const tl_coord = tl_pt.coord();
            let tl_coord = tl_pt.coord();
            // const br_coord = br_pt.coord();
            let br_coord = br_pt.coord();

            // Our sizes are all scaled so we need to send the unscaled values back.
            // const content_scale = self.rt_surface.getContentScale() catch .{ .x = 1, .y = 1 };
            let content_scale = unsafe { &*self.rt_surface }
                .get_content_scale()
                .unwrap_or(apprt::ContentScale { x: 1.0, y: 1.0 });
            // const x: f64 = x: {
            let x: f64 = {
                // Simple x * cell width gives the left
                // var x: f64 = @floatFromInt(tl_coord.x * self.size.cell.width);
                let mut x: f64 = f64::from(u32::from(tl_coord.x) * self.size.cell.width);

                // Add padding
                // x += @floatFromInt(self.size.padding.left);
                x += f64::from(self.size.padding.left);

                // Scale
                // x /= content_scale.x;
                x /= f64::from(content_scale.x);

                // break :x x;
                x
            };
            // const y: f64 = y: {
            let y: f64 = {
                // Simple y * cell height gives the top
                // var y: f64 = @floatFromInt(tl_coord.y * self.size.cell.height);
                let mut y: f64 = f64::from(tl_coord.y * self.size.cell.height);

                // We want the text baseline
                // y += @floatFromInt(self.size.cell.height);
                y += f64::from(self.size.cell.height);
                // y -= @floatFromInt(self.font_metrics.cell_baseline);
                y -= f64::from(self.font_metrics.cell_baseline);

                // Add padding
                // y += @floatFromInt(self.size.padding.top);
                y += f64::from(self.size.padding.top);

                // Scale
                // y /= content_scale.y;
                y /= f64::from(content_scale.y);

                // break :y y;
                y
            };

            // Utilize viewport sizing to convert to offsets
            // const start = tl_coord.y * self.io.terminal.screens.active.pages.cols + tl_coord.x;
            let start = tl_coord.y
                * u32::from(unsafe { (*self.io.terminal.screens.active).pages.cols })
                + u32::from(tl_coord.x);
            // const end = br_coord.y * self.io.terminal.screens.active.pages.cols + br_coord.x;
            let end = br_coord.y
                * u32::from(unsafe { (*self.io.terminal.screens.active).pages.cols })
                + u32::from(br_coord.x);

            // break :viewport .{
            //     .tl_px_x = x,
            //     .tl_px_y = y,
            //     .offset_start = start,
            //     .offset_len = end - start,
            // };
            break 'viewport Some(TextViewport {
                tl_px_x: x,
                tl_px_y: y,
                offset_start: start,
                offset_len: end - start,
            });
        };

        // return .{
        //     .text = text,
        //     .viewport = vp,
        // };
        Ok(Text { text, viewport: vp })
    }

    /// Returns true if the terminal has a selection.
    // pub fn hasSelection(self: *const Surface) bool {
    pub fn hasSelection(&self) -> bool {
        // self.renderer_state.mutex.lock();
        unsafe { (*self.renderer_state.mutex).lock() };
        // defer self.renderer_state.mutex.unlock();
        //
        // (guard-less zigstd Mutex — the defer is the explicit unlock below.)
        // return self.io.terminal.screens.active.selection != null;
        let result = unsafe { (*self.io.terminal.screens.active).selection }.is_some();
        // (the deferred mutex unlock)
        unsafe { (*self.renderer_state.mutex).unlock() };
        result
    }

    /// Returns the selected text. This is allocated.
    // pub fn selectionString(self: *Surface, alloc: Allocator) !?[:0]const u8 {
    //
    // (the return carries raw bytes — the vt `selectionString` `Vec<u8>`
    // convention; the inferred `!` set is the screen's OOM.)
    pub fn selectionString(&mut self, alloc: Allocator) -> Result<Option<Vec<u8>>, Error> {
        // self.renderer_state.mutex.lock();
        unsafe { (*self.renderer_state.mutex).lock() };
        // defer self.renderer_state.mutex.unlock();
        //
        // (guard-less zigstd Mutex — the defer is the explicit unlock at
        // every exit below.)
        // const sel = self.io.terminal.screens.active.selection orelse return null;
        let sel = match unsafe { (*self.io.terminal.screens.active).selection } {
            Some(v) => v,
            None => {
                // (the deferred mutex unlock — the null exit)
                unsafe { (*self.renderer_state.mutex).unlock() };
                return Ok(None);
            }
        };
        // return try self.io.terminal.screens.active.selectionString(alloc, .{
        //     .sel = sel,
        //     .trim = false,
        // });
        let result = unsafe { &mut *self.io.terminal.screens.active }.selection_string(
            alloc,
            terminal::screen::SelectionString {
                sel,
                trim: false,
                map: None,
            },
        );
        // (the deferred mutex unlock)
        unsafe { (*self.renderer_state.mutex).unlock() };
        match result {
            Ok(v) => Ok(Some(v)),
            Err(err) => Err(err.into()),
        }
    }

    /// Returns the pwd of the terminal, if any. This is always copied because
    /// the pwd can change at any point from termio. If we are calling from the IO
    /// thread you should just check the terminal directly.
    // pub fn pwd(
    //     self: *const Surface,
    //     alloc: Allocator,
    // ) Allocator.Error!?[]const u8 {
    //
    // PORT NOTE: the `Allocator.Error!` collapses (infallible Rust
    // allocation — the app.rs precedent); the owned `Vec<u8>` carries the
    // raw pwd bytes.
    pub fn pwd(&self, alloc: Allocator) -> Option<Vec<u8>> {
        // (the DerivedConfig.init precedent: the allocator param is carried
        // for signature parity; the `Vec` below allocates via the global
        // allocator.)
        let _ = alloc;
        // self.renderer_state.mutex.lock();
        unsafe { (*self.renderer_state.mutex).lock() };
        // defer self.renderer_state.mutex.unlock();
        //
        // (guard-less zigstd Mutex — the defer is the explicit unlock at
        // every exit below.)
        // const terminal_pwd = self.io.terminal.getPwd() orelse return null;
        let terminal_pwd = match self.io.terminal.get_pwd() {
            Some(v) => v,
            None => {
                // (the deferred mutex unlock — the null exit)
                unsafe { (*self.renderer_state.mutex).unlock() };
                return None;
            }
        };
        // return try alloc.dupe(u8, terminal_pwd);
        //
        // (`dupe` — an owned copy of the raw pwd bytes; the OOM-only `try`
        // collapses.)
        let result = terminal_pwd.to_vec();
        // (the deferred mutex unlock)
        unsafe { (*self.renderer_state.mutex).unlock() };
        Some(result)
    }

    /// Resolves a relative file path to an absolute path using the terminal's pwd.
    // fn resolvePathForOpening(
    //     self: *Surface,
    //     path: []const u8,
    // ) Allocator.Error!?[]const u8 {
    //
    // PORT NOTE: the `Allocator.Error!` collapses (infallible Rust
    // allocation); raw path bytes in and out (`&[u8]`/`Vec<u8>`, never a
    // lossy `String`). The C13 processLinks callsite (:4396) lands later;
    // the allow is removed when it does.
    #[allow(dead_code)]
    fn resolvePathForOpening(&mut self, path: &[u8]) -> Option<Vec<u8>> {
        // if (!std.fs.path.isAbsolute(path)) {
        if !is_absolute(path) {
            // const terminal_pwd = self.io.terminal.getPwd() orelse {
            //     return null;
            // };
            let terminal_pwd = match self.io.terminal.get_pwd() {
                Some(v) => v,
                None => {
                    return None;
                }
            };

            // const resolved = try std.fs.path.resolve(self.alloc, &.{ terminal_pwd, path });
            let resolved = resolve(&[terminal_pwd, path]);

            // std.fs.accessAbsolute(resolved, .{}) catch {
            //     self.alloc.free(resolved);
            //     return null;
            // };
            //
            // (the rustty-os resourcesdir `accessAbsolute` precedent; the
            // reference's free is the owned `Vec`'s drop.)
            if !std::path::Path::new(os_str_from_bytes(&resolved)).exists() {
                return None;
            }

            // return resolved;
            return Some(resolved);
        }

        // return null;
        None
    }

    /// Returns the x/y coordinate of where the IME (Input Method Editor)
    /// keyboard should be rendered.
    // pub fn imePoint(self: *const Surface) apprt.IMEPos {
    pub fn imePoint(&self) -> apprt::IMEPos {
        // self.renderer_state.mutex.lock();
        //
        // (guard-less zigstd Mutex — CORE_BLUEPRINT §3; the reference locks
        // and unlocks WITHOUT defer, and so does this body: the unlock below
        // pairs with this lock, verbatim.)
        unsafe { (*self.renderer_state.mutex).lock() };
        // const cursor = self.renderer_state.terminal.screens.active.cursor;
        //
        // (`Cursor` is `Copy` — the Zig value-copy, read under the lock.)
        let cursor = unsafe { (*(*self.renderer_state.terminal).screens.active).cursor };
        // const preedit_width: usize = if (self.renderer_state.preedit) |preedit| preedit.width() else 0;
        let preedit_width: usize = if let Some(preedit) = &self.renderer_state.preedit {
            preedit.width()
        } else {
            0
        };
        // self.renderer_state.mutex.unlock();
        unsafe { (*self.renderer_state.mutex).unlock() };

        // TODO: need to handle when scrolling and the cursor is not
        // in the visible portion of the screen.

        // Our sizes are all scaled so we need to send the unscaled values back.
        // const content_scale = self.rt_surface.getContentScale() catch .{ .x = 1, .y = 1 };
        let content_scale = unsafe { &*self.rt_surface }
            .get_content_scale()
            .unwrap_or(apprt::ContentScale { x: 1.0, y: 1.0 });

        // const x: f64 = x: {
        let x: f64 = {
            // Simple x * cell width gives the top-left corner, then add padding offset
            // var x: f64 = @floatFromInt(cursor.x * self.size.cell.width + self.size.padding.left);
            let mut x: f64 =
                f64::from(u32::from(cursor.x) * self.size.cell.width + self.size.padding.left);

            // We want the midpoint
            // x += @as(f64, @floatFromInt(self.size.cell.width)) / 2;
            x += f64::from(self.size.cell.width) / 2.0;

            // And scale it
            // x /= content_scale.x;
            x /= f64::from(content_scale.x);

            // break :x x;
            x
        };

        // const y: f64 = y: {
        let y: f64 = {
            // Simple y * cell height gives the top-left corner, then add padding offset
            // var y: f64 = @floatFromInt(cursor.y * self.size.cell.height + self.size.padding.top);
            let mut y: f64 =
                f64::from(u32::from(cursor.y) * self.size.cell.height + self.size.padding.top);

            // We want the bottom
            // y += @floatFromInt(self.size.cell.height);
            y += f64::from(self.size.cell.height);

            // And scale it
            // y /= content_scale.y;
            y /= f64::from(content_scale.y);

            // break :y y;
            y
        };

        // Our height for now is always just the cell height because our preedit
        // rendering only renders in a single line.
        // const height: f64 = height: {
        let height: f64 = {
            // var height: f64 = @floatFromInt(self.size.cell.height);
            let mut height: f64 = f64::from(self.size.cell.height);
            // height /= content_scale.y;
            height /= f64::from(content_scale.y);
            // break :height height;
            height
        };
        // const width: f64 = width: {
        let width: f64 = {
            // var width: f64 = @floatFromInt(preedit_width * self.size.cell.width);
            let mut width: f64 = (preedit_width * (self.size.cell.width as usize)) as f64;

            // Our max width is the remaining screen width after the cursor.
            // We don't have to deal with wrapping because the preedit doesn't
            // wrap right now.
            // const screen_width: f64 = @floatFromInt(self.size.terminal().width);
            let screen_width: f64 = f64::from(self.size.terminal().width);
            // const x_offset: f64 = @floatFromInt((cursor.x + 1) * self.size.cell.width);
            let x_offset: f64 = f64::from((u32::from(cursor.x) + 1) * self.size.cell.width);
            // const max = screen_width - x_offset;
            let max = screen_width - x_offset;
            // width = @min(width, max);
            width = width.min(max);

            // Note: we don't apply content scale here because it looks like
            // for some reason in macOS its already scaled. I'm not sure why
            // that is so I'm going to just leave this comment here so its known
            // that I left this out on purpose pending more investigation.

            // break :width width;
            width
        };

        // return .{
        //     .x = x,
        //     .y = y,
        //     .width = width,
        //     .height = height,
        // };
        apprt::IMEPos {
            x,
            y,
            width,
            height,
        }
    }

    // fn clipboardWrite(self: *const Surface, data: []const u8, loc: apprt.Clipboard) !void {
    //
    // (the inferred `!void` is `allocSentinel`'s OOM — collapsed; the
    // `Result` shape carries the handleMessage callers' `try`s.)
    fn clipboardWrite(&self, data: &[u8], loc: apprt::Clipboard) -> Result<(), Error> {
        // if (self.config.clipboard_write == .deny) {
        if self.config.clipboard_write == configpkg::value_types::ClipboardAccess::Deny {
            // log.info("application attempted to write clipboard, but 'clipboard-write' is set to deny", .{});
            return Ok(());
        }

        // const dec = std.base64.standard.Decoder;
        //
        // PORT NOTE: `zig_base64` (file scope below) is the minimal faithful
        // port of the Zig std.base64 standard codec's Decoder surface (the
        // rustty-os file.rs `b64_encoder` precedent).

        // Build buffer
        // const size = dec.calcSizeForSlice(data) catch |err| switch (err) {
        let size = match zig_base64::calc_size_for_slice(data) {
            Ok(v) => v,
            Err(err) => match err {
                // error.InvalidPadding => {
                zig_base64::Error::InvalidPadding => {
                    // log.info("application sent invalid base64 data for OSC 52", .{});
                    return Ok(());
                }

                // Should not be reachable but don't want to risk it.
                // else => return,
                _ => return Ok(()),
            },
        };
        // var buf = try self.alloc.allocSentinel(u8, size, 0);
        //
        // (`allocSentinel` — `size` bytes plus the NUL sentinel slot; the
        // OOM-only `try` collapses.)
        let mut buf: Vec<u8> = vec![0u8; size + 1];
        // defer self.alloc.free(buf);
        // (the owned `buf` drops on every exit below.)
        // buf[buf.len] = 0;
        //
        // (`buf.len` is `size` — the sentinel-slot write.)
        buf[size] = 0;

        // Decode
        // dec.decode(buf, data) catch |err| switch (err) {
        match zig_base64::decode(&mut buf[0..size], data) {
            Ok(()) => {}

            // Ignore this. It is possible to actually have valid data and
            // get this error, so we allow it.
            // error.InvalidPadding => {},
            Err(zig_base64::Error::InvalidPadding) => {}

            // else => {
            Err(_) => {
                // log.info("application sent invalid base64 data for OSC 52", .{});
                return Ok(());
            }
        }
        // assert(buf[buf.len] == 0);
        debug_assert!(buf[size] == 0);

        // When clipboard-write is "ask" a prompt is displayed to the user asking
        // them to confirm the clipboard access. Each app runtime handles this
        // differently.
        // const confirm = self.config.clipboard_write == .ask;
        let confirm = self.config.clipboard_write == configpkg::value_types::ClipboardAccess::Ask;
        // self.rt_surface.setClipboard(loc, &.{.{
        //     .mime = "text/plain",
        //     .data = buf,
        // }}, confirm) catch |err| {
        //     log.err("error setting clipboard string err={}", .{err});
        //     return;
        // };
        if let Err(_err) = unsafe { &*self.rt_surface }.set_clipboard(
            loc,
            &[apprt::ClipboardContent {
                mime: b"text/plain".as_slice(),
                data: &buf[0..size],
            }],
            confirm,
        ) {
            // log.err("error setting clipboard string err={}", .{err});
            return Ok(());
        }

        Ok(())
    }

    // fn copySelectionToClipboards(
    //     self: *Surface,
    //     sel: terminal.Selection,
    //     clipboards: []const apprt.Clipboard,
    //     format: input.Binding.Action.CopyToClipboard,
    // ) !void {
    //
    // (the inferred `!void` set is the allocating-writer OOM — the [`Error`]
    // union.)
    fn copySelectionToClipboards(
        &mut self,
        sel: terminal::selection::Selection,
        clipboards: &[apprt::Clipboard],
        format: binding::CopyToClipboard,
    ) -> Result<(), Error> {
        // Create an arena to simplify memory management here.
        // var arena = ArenaAllocator.init(self.alloc);
        // defer arena.deinit();
        // const alloc = arena.allocator();
        //
        // PORT NOTE (collapsed arena): the owned buffers below free on drop
        // (the childExitedAbnormally precedent); the `Vec` sinks allocate
        // via the global allocator, so the arena allocator is not
        // materialized.

        // The options we'll use for all formatting. We'll just override the
        // emit format.
        // const opts: terminal.formatter.Options = .{
        let opts: terminal::formatter::Options = terminal::formatter::Options {
            // .emit = .plain, // We'll override this below
            emit: terminal::formatter::Format::Plain, // We'll override this below
            // .unwrap = true,
            unwrap: true,
            // .trim = self.config.clipboard_trim_trailing_spaces,
            trim: self.config.clipboard_trim_trailing_spaces,
            // .codepoint_map = self.config.clipboard_codepoint_map.map.list,
            //
            // (Zig copies the MultiArrayList header shallowly; the owned
            // `Options` clones the entries — the formatter.rs `CodepointMap`
            // Clone PORT NOTE.)
            codepoint_map: Some(self.config.clipboard_codepoint_map.map.list.clone()),
            // .background = self.io.terminal.colors.background.get(),
            background: self.io.terminal.colors.background.get(),
            // .foreground = self.io.terminal.colors.foreground.get(),
            foreground: self.io.terminal.colors.foreground.get(),
            // .palette = &self.io.terminal.colors.palette.current,
            palette: Some(&raw const self.io.terminal.colors.palette.current),
        };

        // const ScreenFormatter = terminal.formatter.ScreenFormatter;
        use rustty_vt::formatter::ScreenFormatter;
        // var aw: std.Io.Writer.Allocating = .init(alloc);
        //
        // (the `Vec<u8>` allocating-writer sink — the vt selectionString
        // precedent; each `toOwnedSliceSentinel(0)` below is a `mem::take`,
        // the length excluding the NUL sentinel.)
        let mut aw: Vec<u8> = Vec::new();
        // var contents: std.ArrayList(apprt.ClipboardContent) = .empty;
        //
        // PORT NOTE (arena ownership): `ClipboardContent` BORROWS its bytes;
        // the reference arena-allocates each buffer. The owned (mime, data)
        // pairs collected here are those arena allocations; the borrowed
        // `ClipboardContent` view is materialized at the `setClipboard` loop
        // below, once the buffers stop moving.
        let mut contents: Vec<(&[u8], Vec<u8>)> = Vec::new();
        // switch (format) {
        match format {
            // .plain => {
            binding::CopyToClipboard::Plain => {
                // var formatter: ScreenFormatter = .init(self.io.terminal.screens.active, opts);
                let mut formatter: ScreenFormatter =
                    ScreenFormatter::init(self.io.terminal.screens.active, copy_options(&opts));
                // formatter.content = .{ .selection = sel };
                formatter.content = terminal::formatter::Content::Selection(Some(sel));
                // try formatter.format(&aw.writer);
                //
                // (allocating writer: a failed write is an OOM — the vt
                // selectionString precedent.)
                formatter
                    .format(&mut aw)
                    .map_err(|_| terminal::pagelist::OutOfMemory)?;
                // try contents.append(alloc, .{
                //     .mime = "text/plain",
                //     .data = try aw.toOwnedSliceSentinel(0),
                // });
                contents.push((b"text/plain".as_slice(), std::mem::take(&mut aw)));
            }

            // .vt => {
            binding::CopyToClipboard::Vt => {
                // var formatter: ScreenFormatter = .init(self.io.terminal.screens.active, opts: {
                //     var copy = opts;
                //     copy.emit = .vt;
                //     break :opts copy;
                // });
                let mut formatter: ScreenFormatter =
                    ScreenFormatter::init(self.io.terminal.screens.active, {
                        let mut copy = copy_options(&opts);
                        copy.emit = terminal::formatter::Format::Vt;
                        copy
                    });
                // formatter.content = .{ .selection = sel };
                formatter.content = terminal::formatter::Content::Selection(Some(sel));
                // try formatter.format(&aw.writer);
                formatter
                    .format(&mut aw)
                    .map_err(|_| terminal::pagelist::OutOfMemory)?;

                // Note: We don't apply codepoint mappings to VT format since it contains
                // escape sequences that should be preserved as-is
                // try contents.append(alloc, .{
                //     .mime = "text/plain",
                //     .data = try aw.toOwnedSliceSentinel(0),
                // });
                contents.push((b"text/plain".as_slice(), std::mem::take(&mut aw)));
            }

            // .html => {
            binding::CopyToClipboard::Html => {
                // var formatter: ScreenFormatter = .init(self.io.terminal.screens.active, opts: {
                //     var copy = opts;
                //     copy.emit = .html;
                //     break :opts copy;
                // });
                let mut formatter: ScreenFormatter =
                    ScreenFormatter::init(self.io.terminal.screens.active, {
                        let mut copy = copy_options(&opts);
                        copy.emit = terminal::formatter::Format::Html;
                        copy
                    });
                // formatter.content = .{ .selection = sel };
                formatter.content = terminal::formatter::Content::Selection(Some(sel));
                // try formatter.format(&aw.writer);
                formatter
                    .format(&mut aw)
                    .map_err(|_| terminal::pagelist::OutOfMemory)?;

                // Note: We don't apply codepoint mappings to HTML format since HTML
                // has its own character encoding and entity system
                // try contents.append(alloc, .{
                //     .mime = "text/html",
                //     .data = try aw.toOwnedSliceSentinel(0),
                // });
                contents.push((b"text/html".as_slice(), std::mem::take(&mut aw)));
            }

            // .mixed => {
            binding::CopyToClipboard::Mixed => {
                // First, generate plain text with codepoint mappings applied
                // var formatter: ScreenFormatter = .init(self.io.terminal.screens.active, opts);
                let mut formatter: ScreenFormatter =
                    ScreenFormatter::init(self.io.terminal.screens.active, copy_options(&opts));
                // formatter.content = .{ .selection = sel };
                formatter.content = terminal::formatter::Content::Selection(Some(sel));
                // try formatter.format(&aw.writer);
                formatter
                    .format(&mut aw)
                    .map_err(|_| terminal::pagelist::OutOfMemory)?;
                // try contents.append(alloc, .{
                //     .mime = "text/plain",
                //     .data = try aw.toOwnedSliceSentinel(0),
                // });
                contents.push((b"text/plain".as_slice(), std::mem::take(&mut aw)));

                // assert(aw.written().len == 0);
                debug_assert!(aw.is_empty());
                // Second, generate HTML without codepoint mappings
                // formatter = .init(self.io.terminal.screens.active, opts: {
                //     var copy = opts;
                //     copy.emit = .html;
                //
                //     ...
                //     break :opts copy;
                // });
                formatter = ScreenFormatter::init(self.io.terminal.screens.active, {
                    let mut copy = copy_options(&opts);
                    copy.emit = terminal::formatter::Format::Html;

                    // We purposely don't emit background/foreground for mixed
                    // mode because the HTML contents is often used for rich text
                    // input and with trimmed spaces it looks pretty bad.
                    // copy.background = null;
                    copy.background = None;
                    // copy.foreground = null;
                    copy.foreground = None;

                    copy
                });
                // formatter.content = .{ .selection = sel };
                formatter.content = terminal::formatter::Content::Selection(Some(sel));
                // try formatter.format(&aw.writer);
                formatter
                    .format(&mut aw)
                    .map_err(|_| terminal::pagelist::OutOfMemory)?;

                // Note: We don't apply codepoint mappings to HTML format
                // try contents.append(alloc, .{
                //     .mime = "text/html",
                //     .data = try aw.toOwnedSliceSentinel(0),
                // });
                contents.push((b"text/html".as_slice(), std::mem::take(&mut aw)));
            }
        }

        // assert(contents.items.len > 0);
        debug_assert!(!contents.is_empty());
        // (the borrowed `ClipboardContent` view over the owned pairs — the
        // arena-ownership PORT NOTE above.)
        let contents_items: Vec<apprt::ClipboardContent<'_>> = contents
            .iter()
            .map(|c| apprt::ClipboardContent {
                mime: c.0,
                data: &c.1,
            })
            .collect();
        // for (clipboards) |clipboard| self.rt_surface.setClipboard(
        //     clipboard,
        //     contents.items,
        //     false,
        // ) catch |err| {
        for clipboard in clipboards {
            if let Err(_err) =
                unsafe { &*self.rt_surface }.set_clipboard(*clipboard, &contents_items, false)
            {
                // log.err(
                //     "error setting clipboard string clipboard={} err={}",
                //     .{ clipboard, err },
                // );
            }
        }

        Ok(())
    }

    /// Set the active selection and notify the apprt on a genuine state
    /// transition. All selection mutations route through here rather than
    /// `screen.select` directly so the notification fires consistently. To
    /// also copy per `copy_on_select`, use `setSelectionAndCopy`.
    ///
    /// This must be called with the renderer mutex held.
    // fn setSelection(self: *Surface, sel_: ?terminal.Selection) !void {
    fn setSelection(&mut self, sel_: Option<terminal::selection::Selection>) -> Result<(), Error> {
        // Compute the transition before `select` below, which untracks (frees)
        // the previous selection's tracked pins; reading them after would be a
        // use-after-free.
        // const prev_ = self.io.terminal.screens.active.selection;
        let prev_ = unsafe { (*self.io.terminal.screens.active).selection };
        // const changed = changed: {
        //     const prev = prev_ orelse break :changed sel_ != null;
        //     const sel = sel_ orelse break :changed true;
        //     break :changed !sel.eql(prev);
        // };
        let changed = 'changed: {
            let prev = match prev_ {
                Some(v) => v,
                None => break 'changed sel_.is_some(),
            };
            let sel = match sel_ {
                Some(v) => v,
                None => break 'changed true,
            };
            !sel.eql(prev)
        };

        // try self.io.terminal.screens.active.select(sel_);
        unsafe { (*self.io.terminal.screens.active).select(sel_) }?;

        // if (changed) {
        if changed {
            // _ = self.rt_app.performAction(
            //     .{ .surface = self },
            //     .selection_changed,
            //     {},
            // ) catch |err| {
            //     log.warn("apprt failed selection_changed notification err={}", .{err});
            // };
            if let Err(_err) = unsafe { &*self.rt_app }.perform_action(
                apprt::Target::Surface(&*self),
                apprt::Action::SelectionChanged,
            ) {
                // log.warn("apprt failed selection_changed notification err={}", .{err});
            }
        }

        Ok(())
    }

    /// Set a selection and, per `copy_on_select`, copy it to the clipboard.
    /// For committing selection gestures (mouse release, select-all binding).
    ///
    /// This must be called with the renderer mutex held.
    // fn setSelectionAndCopy(self: *Surface, sel: terminal.Selection) !void {
    //
    // PORT NOTE: the C12 mouseButtonCallback (:3866) / C13 cursorPosCallback
    // (:4086/:4092) callsites land later; the allow is removed when the
    // first one does.
    #[allow(dead_code)]
    fn setSelectionAndCopy(&mut self, sel: terminal::selection::Selection) -> Result<(), Error> {
        // try self.setSelection(sel);
        self.setSelection(Some(sel))?;

        // If copy on select is false then exit early.
        // if (self.config.copy_on_select == .false) return;
        if self.config.copy_on_select == configpkg::value_types::CopyOnSelect::False {
            return Ok(());
        }

        // switch (self.config.copy_on_select) {
        match self.config.copy_on_select {
            // .false => unreachable, // handled above with an early exit
            configpkg::value_types::CopyOnSelect::False => unreachable!(), // handled above with an early exit

            // Both standard and selection clipboards are set.
            // .clipboard => try self.copySelectionToClipboards(
            //     sel,
            //     &.{ .standard, .selection },
            //     .mixed,
            // ),
            configpkg::value_types::CopyOnSelect::Clipboard => self.copySelectionToClipboards(
                sel,
                &[apprt::Clipboard::Standard, apprt::Clipboard::Selection],
                binding::CopyToClipboard::Mixed,
            )?,

            // The selection clipboard is set if supported, otherwise the standard.
            // .true => {
            configpkg::value_types::CopyOnSelect::True => {
                // const clipboard: apprt.Clipboard = if (self.rt_surface.supportsClipboard(.selection))
                //     .selection
                // else
                //     .standard;
                let clipboard: apprt::Clipboard = if unsafe { &*self.rt_surface }
                    .supports_clipboard(apprt::Clipboard::Selection)
                {
                    apprt::Clipboard::Selection
                } else {
                    apprt::Clipboard::Standard
                };
                // try self.copySelectionToClipboards(
                //     sel,
                //     &.{clipboard},
                //     .mixed,
                // );
                self.copySelectionToClipboards(sel, &[clipboard], binding::CopyToClipboard::Mixed)?;
            }
        }

        Ok(())
    }

    /// Change the cell size for the terminal grid. This can happen as
    /// a result of changing the font size at runtime.
    // fn setCellSize(self: *Surface, size: rendererpkg.CellSize) !void {
    fn setCellSize(&mut self, size: rendererpkg::size::CellSize) -> Result<(), Error> {
        // Update our cell size within our size struct
        // self.size.cell = size;
        self.size.cell = size;
        // self.balancePaddingIfNeeded();
        self.balancePaddingIfNeeded();

        // Notify the terminal
        // self.queueIo(.{ .resize = self.size }, .unlocked);
        self.queueIo(
            termio::Message::Resize(self.size),
            termio::termio_impl::MutexState::Unlocked,
        );

        // Update our terminal default size if necessary.
        // self.recomputeInitialSize() catch |err| {
        //     // We don't treat this as a fatal error because not setting
        //     // an initial size shouldn't stop our terminal from working.
        //     log.warn("unable to recompute initial window size: {}", .{err});
        // };
        if let Err(_err) = self.recomputeInitialSize() {
            // We don't treat this as a fatal error because not setting
            // an initial size shouldn't stop our terminal from working.
            // log.warn("unable to recompute initial window size: {}", .{err});
        }

        // Notify the window
        // _ = try self.rt_app.performAction(
        //     .{ .surface = self },
        //     .cell_size,
        //     .{ .width = size.width, .height = size.height },
        // );
        let _ = unsafe { &*self.rt_app }.perform_action(
            apprt::Target::Surface(&*self),
            apprt::Action::CellSize(CellSize {
                width: size.width,
                height: size.height,
            }),
        )?;

        Ok(())
    }

    /// Change the font size.
    ///
    /// This can only be called from the main thread.
    // pub fn setFontSize(self: *Surface, size: font.face.DesiredSize) !void {
    pub fn setFontSize(&mut self, size: DesiredSize) -> Result<(), Error> {
        // log.debug("set font size size={}", .{size.points});

        // Update our font size so future changes work
        // self.font_size = size;
        self.font_size = size;

        // We need to build up a new font stack for this font size.
        // const font_grid_key, const font_grid = try self.app.font_grid_set.ref(
        //     &self.config.font,
        //     self.font_size,
        // );
        let (font_grid_key, font_grid) = unsafe {
            (*self.app)
                .font_grid_set
                .r#ref(&self.config.font, self.font_size)
        }?;
        // errdefer self.app.font_grid_set.deref(font_grid_key);
        // (fired explicitly at the failure site below — the only fallible
        // site in its live range.)

        // Set our cell size
        // try self.setCellSize(.{
        //     .width = font_grid.metrics.cell_width,
        //     .height = font_grid.metrics.cell_height,
        // });
        if let Err(err) = self.setCellSize(rendererpkg::size::CellSize {
            width: unsafe { &*font_grid }.metrics.cell_width,
            height: unsafe { &*font_grid }.metrics.cell_height,
        }) {
            // errdefer self.app.font_grid_set.deref(font_grid_key);
            unsafe { (*self.app).font_grid_set.deref(font_grid_key) };
            return Err(err);
        }

        // Notify our render thread of the new font stack. The renderer
        // MUST accept the new font grid and deref the old.
        // _ = self.renderer_thread.mailbox.push(.{
        //     .font_grid = .{
        //         .grid = font_grid,
        //         .set = &self.app.font_grid_set,
        //         .old_key = self.font_grid_key,
        //         .new_key = font_grid_key,
        //     },
        // }, .{ .forever = {} });
        //
        // PORT NOTE (Rust ownership): the reference key copies are SHALLOW
        // (Zig value-copies; the key memory belongs to the set's map). The
        // rustty `Key` is OWNED (the shared_grid_set PORT NOTE), so the
        // message's copies are `Clone`s (the Key `Clone` PORT NOTE) and the
        // surface's old key value is dropped at the replacement below (a
        // plain free the reference doesn't need).
        let _ = unsafe { &*self.renderer_thread.mailbox }.push(
            rendererpkg::Message::FontGrid {
                grid: font_grid,
                set: unsafe { &raw mut (*self.app).font_grid_set },
                old_key: self.font_grid_key.clone(),
                new_key: font_grid_key.clone(),
            },
            Timeout::Forever,
        );

        // Once we've sent the key we can replace our key
        // self.font_grid_key = font_grid_key;
        self.font_grid_key = font_grid_key;
        // self.font_metrics = font_grid.metrics;
        self.font_metrics = unsafe { &*font_grid }.metrics;

        // Schedule render which also drains our mailbox
        // self.queueRender() catch unreachable;
        self.queueRender().unwrap_or_else(|_| unreachable!());

        Ok(())
    }

    /// This queues a render operation with the renderer thread. The render
    /// isn't guaranteed to happen immediately but it will happen as soon as
    /// practical.
    //
    // fn queueRender(self: *Surface) !void {
    //
    // PORT NOTE (ported with C7, ahead of its C8 row — Surface.zig:2461-2465;
    // selectionScrollTick/passwordInput call it): the inferred `!void` is the
    // xev notify error (the CORE_BLUEPRINT §5 xev-blocked one-line leaf).
    fn queueRender(&mut self) -> Result<(), rustty_xev::NotifyError> {
        // try self.renderer_thread.wakeup.notify();
        self.renderer_thread.wakeup.notify()?;
        Ok(())
    }

    // pub fn sizeCallback(self: *Surface, size: apprt.SurfaceSize) !void {
    //
    // (the inferred `!void` is `resize`'s — the [`Error`] union.)
    pub fn sizeCallback(&mut self, size: apprt::SurfaceSize) -> Result<(), Error> {
        // Crash metadata in case we crash in here
        // crash.sentry.thread_state = self.crashThreadState();
        crash::sentry::setThreadState(self.crashThreadState());
        // defer crash.sentry.thread_state = null;
        // ([ADV 6.1] — the Drop guard clears at every exit below.)
        let _thread_state = ThreadStateGuard;

        // const new_screen_size: rendererpkg.ScreenSize = .{
        //     .width = size.width,
        //     .height = size.height,
        // };
        let new_screen_size: rendererpkg::size::ScreenSize = rendererpkg::size::ScreenSize {
            width: size.width,
            height: size.height,
        };

        // Update our screen size, but only if it actually changed. And if
        // the screen size didn't change, then our grid size could not have
        // changed, so we just return.
        // if (self.size.screen.equals(new_screen_size)) return;
        if self.size.screen.equals(new_screen_size) {
            return Ok(());
        }

        // try self.resize(new_screen_size);
        self.resize(new_screen_size)?;
        Ok(())
    }

    // fn resize(self: *Surface, size: rendererpkg.ScreenSize) !void {
    //
    // (the reference body has no failing site — the inferred `!void` set is
    // empty today; the `Result` shape carries the callers' `try`s.)
    fn resize(&mut self, size: rendererpkg::size::ScreenSize) -> Result<(), Error> {
        // Save our screen size
        // self.size.screen = size;
        self.size.screen = size;
        // self.balancePaddingIfNeeded();
        self.balancePaddingIfNeeded();

        // Recalculate our grid size. Because Ghostty supports fluid resizing,
        // its possible the grid doesn't change at all even if the screen size changes.
        // We have to update the IO thread no matter what because we send
        // pixel-level sizing to the subprocess.
        // const grid_size = self.size.grid();
        let grid_size = self.size.grid();
        // if (grid_size.columns < 5 and (self.size.padding.left > 0 or self.size.padding.right > 0)) {
        if grid_size.columns < 5 && (self.size.padding.left > 0 || self.size.padding.right > 0) {
            // log.warn("WARNING: very small terminal grid detected with padding " ++
            //     "set. Is your padding reasonable?", .{});
        }
        // if (grid_size.rows < 2 and (self.size.padding.top > 0 or self.size.padding.bottom > 0)) {
        if grid_size.rows < 2 && (self.size.padding.top > 0 || self.size.padding.bottom > 0) {
            // log.warn("WARNING: very small terminal grid detected with padding " ++
            //     "set. Is your padding reasonable?", .{});
        }

        // Mail the IO thread
        // self.queueIo(.{ .resize = self.size }, .unlocked);
        self.queueIo(
            termio::Message::Resize(self.size),
            termio::termio_impl::MutexState::Unlocked,
        );

        Ok(())
    }

    /// Recalculate the balanced padding if needed.
    // fn balancePaddingIfNeeded(self: *Surface) void {
    fn balancePaddingIfNeeded(&mut self) {
        // if (self.config.window_padding_balance == .false) return;
        if self.config.window_padding_balance == configpkg::value_types::WindowPaddingBalance::False
        {
            return;
        }
        // const content_scale = try self.rt_surface.getContentScale();
        //
        // PORT NOTE: the reference `try` inside a `void` fn type-checks only
        // because the comptime backend's `getContentScale` error set is EMPTY
        // (an infallible field read — embedded.zig:650); the seam trait
        // returns the opaque `ApprtError`, so the statically-empty `try` maps
        // to the infallible unwrap (the catch-unreachable convention).
        let content_scale = unsafe { &*self.rt_surface }
            .get_content_scale()
            .unwrap_or_else(|_| unreachable!());
        // const x_dpi = content_scale.x * font.face.default_dpi;
        let x_dpi = content_scale.x * f32::from(face::DEFAULT_DPI);
        // const y_dpi = content_scale.y * font.face.default_dpi;
        let y_dpi = content_scale.y * f32::from(face::DEFAULT_DPI);
        // self.size.balancePadding(self.config.scaledPadding(x_dpi, y_dpi), self.config.window_padding_balance);
        self.size.balance_padding(
            self.config.scaledPadding(x_dpi, y_dpi),
            self.config.window_padding_balance,
        );
    }

    /// Called to set the preedit state for character input. Preedit is used
    /// with dead key states, for example, when typing an accent character.
    /// This should be called with null to reset the preedit state.
    ///
    /// The core surface will NOT reset the preedit state on charCallback or
    /// keyCallback and we rely completely on the apprt implementation to track
    /// the preedit state correctly.
    ///
    /// The preedit input must be UTF-8 encoded.
    // pub fn preeditCallback(self: *Surface, preedit_: ?[]const u8) !void {
    pub fn preeditCallback(&mut self, preedit_: Option<&[u8]>) -> Result<(), Error> {
        // log.debug("text preeditCallback value={any}", .{preedit_});

        // Crash metadata in case we crash in here
        // crash.sentry.thread_state = self.crashThreadState();
        crash::sentry::setThreadState(self.crashThreadState());
        // defer crash.sentry.thread_state = null;
        // ([ADV 6.1] — the Drop guard clears at every exit below, AFTER the
        // explicit mutex unlocks: the reference's LIFO defer order.)
        let _thread_state = ThreadStateGuard;

        // self.renderer_state.mutex.lock();
        // defer self.renderer_state.mutex.unlock();
        //
        // (guard-less zigstd Mutex — CORE_BLUEPRINT §3; the defer is the
        // explicit unlock at every exit below.)
        unsafe { (*self.renderer_state.mutex).lock() };

        // We clear our selection when ANY OF:
        // 1. We have an existing preedit
        // 2. We have preedit text
        // if (self.renderer_state.preedit != null or
        //     preedit_ != null)
        // {
        if self.renderer_state.preedit.is_some() || preedit_.is_some() {
            // if (self.config.selection_clear_on_typing) {
            if self.config.selection_clear_on_typing {
                // self.setSelection(null) catch {};
                let _ = self.setSelection(None);
            }
        }

        // We always clear our prior preedit
        // if (self.renderer_state.preedit) |p| {
        if let Some(p) = self.renderer_state.preedit.take() {
            // self.alloc.free(p.codepoints);
            drop(p.codepoints);
            // self.renderer_state.preedit = null;
            // (the `take()` above.)
        }

        // Mark preedit dirty flag
        // self.io.terminal.flags.dirty.preedit = true;
        self.io.terminal.flags.dirty.preedit = true;

        // If we have no text, we're done. We queue a render in case we cleared
        // a prior preedit (likely).
        // const text = preedit_ orelse {
        //     try self.queueRender();
        //     return;
        // };
        let text = match preedit_ {
            Some(v) => v,
            None => {
                let result = self.queueRender();
                // (the deferred mutex unlock)
                unsafe { (*self.renderer_state.mutex).unlock() };
                return result.map_err(Error::from);
            }
        };

        // We convert the UTF-8 text to codepoints.
        // const view = try std.unicode.Utf8View.init(text);
        // var it = view.iterator();
        let view = match std::str::from_utf8(text) {
            Ok(v) => v,
            Err(err) => {
                // (the deferred mutex unlock — the error exit)
                unsafe { (*self.renderer_state.mutex).unlock() };
                return Err(err.into());
            }
        };
        let it = view.chars();

        // Allocate the codepoints slice
        // const Codepoint = rendererpkg.State.Preedit.Codepoint;
        //
        // (the reference nests it as `State.Preedit.Codepoint`; the port
        // hoists it to `state::Codepoint` — the state.rs PORT NOTE.)
        // var codepoints: std.ArrayListUnmanaged(Codepoint) = .{};
        // defer codepoints.deinit(self.alloc);
        //
        // (the defer covers the toOwnedSlice-emptied list on every exit —
        // Vec ownership: moved into the Preedit below or dropped.)
        let mut codepoints: Vec<rendererpkg::state::Codepoint> = Vec::new();
        // while (it.nextCodepoint()) |cp| {
        for cp in it {
            // const width: usize = @intCast(unicode.table.get(cp).width);
            let width: usize = usize::from(unicode::table().get(cp as u32).width);

            // I've never seen a preedit text with a zero-width character. In
            // theory its possible but we can't really handle it right now.
            // Let's just ignore it.
            // if (width <= 0) continue;
            //
            // (`<= 0` on the unsigned width is `== 0`.)
            if width == 0 {
                continue;
            }

            // try codepoints.append(
            //     self.alloc,
            //     .{ .codepoint = cp, .wide = width >= 2 },
            // );
            codepoints.push(rendererpkg::state::Codepoint {
                codepoint: cp as u32,
                wide: width >= 2,
            });
        }

        // If we have no codepoints, then we're done.
        // if (codepoints.items.len == 0) {
        if codepoints.is_empty() {
            // try self.queueRender();
            let result = self.queueRender();
            // (the deferred mutex unlock)
            unsafe { (*self.renderer_state.mutex).unlock() };
            // return;
            return result.map_err(Error::from);
        }

        // self.renderer_state.preedit = .{
        //     .codepoints = try codepoints.toOwnedSlice(self.alloc),
        // };
        //
        // (`toOwnedSlice` — the `Vec` IS the owned slice; the state.rs
        // Preedit PORT NOTE.)
        self.renderer_state.preedit = Some(rendererpkg::state::Preedit { codepoints });
        // try self.queueRender();
        let result = self.queueRender();
        // (the deferred mutex unlock — the normal exit)
        unsafe { (*self.renderer_state.mutex).unlock() };
        result.map_err(Error::from)
    }

    /// Returns true if the given key event would trigger a keybinding
    /// if it were to be processed. This is useful for determining if
    /// a key event should be sent to the terminal or not.
    ///
    /// Note that this function does not check if the binding itself
    /// is performable, only if the key event would trigger a binding.
    /// If a performable binding is found and the event is not performable,
    /// then Ghosty will act as though the binding does not exist.
    // pub fn keyEventIsBinding(
    //     self: *Surface,
    //     event_orig: input.KeyEvent,
    // ) ?input.Binding.Flags {
    pub fn keyEventIsBinding(&mut self, event_orig: KeyEvent) -> Option<binding::Flags> {
        // Apply key remappings for consistency with keyCallback
        // var event = event_orig;
        //
        // (PORT: `event_orig` moves into `event` — the Rust KeyEvent owns its
        // utf8 bytes; both `event_orig.mods` reads below still see the
        // pre-assignment mods, the RHS evaluates first.)
        let mut event = event_orig;
        // if (self.config.key_remaps.isRemapped(event_orig.mods)) {
        if self.config.key_remaps.is_remapped(event.mods) {
            // event.mods = self.config.key_remaps.apply(event_orig.mods);
            event.mods = self.config.key_remaps.apply(event.mods);
        }

        // switch (event.action) {
        match event.action {
            // .release => return null,
            key::Action::Release => return None,
            // .press, .repeat => {},
            key::Action::Press | key::Action::Repeat => {}
        }

        // Look up our entry
        // const entry: input.Binding.Set.Entry = entry: {
        //
        // PORT NOTE: the reference `Entry` is a key/value POINTER pair; the
        // ported `Set.getEvent` returns the entry INDEX (the app.rs keyEvent
        // precedent), so the entry is the `(set, index)` pair here —
        // `value_at`/`key_at` are the `entry.value_ptr.*`/`entry.key_ptr.*`
        // reads. The set side is a raw pointer because two of the three
        // sources (`sequence_set`, the table stack) already store raw
        // pointers into config-owned sets, exactly as the reference.
        let entry: (*const binding::Set, usize) = 'entry: {
            // If we're in a sequence, check the sequence set
            // if (self.keyboard.sequence_set) |set| {
            //     break :entry set.getEvent(event) orelse return null;
            // }
            if let Some(set) = self.keyboard.sequence_set {
                match unsafe { &*set }.get_event(&event) {
                    Some(v) => break 'entry (set, v),
                    None => return None,
                }
            }

            // Check active key tables (inner-most to outer-most)
            // const table_items = self.keyboard.table_stack.items;
            let table_items = &self.keyboard.table_stack;
            // for (0..table_items.len) |i| {
            for i in 0..table_items.len() {
                // const rev_i: usize = table_items.len - 1 - i;
                let rev_i: usize = table_items.len() - 1 - i;
                // if (table_items[rev_i].set.getEvent(event)) |entry| {
                //     break :entry entry;
                // }
                if let Some(entry) = unsafe { &*table_items[rev_i].set }.get_event(&event) {
                    break 'entry (table_items[rev_i].set, entry);
                }
            }

            // Check the root set
            // break :entry self.config.keybind.set.getEvent(event) orelse return null;
            match self.config.keybind.set.get_event(&event) {
                Some(v) => break 'entry (&self.config.keybind.set as *const binding::Set, v),
                None => return None,
            }
        };

        // Return flags based on the
        // return switch (entry.value_ptr.*) {
        match unsafe { &*entry.0 }.value_at(entry.1) {
            // .leader => .{},
            binding::Value::Leader(_) => Some(binding::Flags::default()),
            // inline .leaf, .leaf_chained => |v| v.flags,
            binding::Value::Leaf(v) => Some(v.flags),
            binding::Value::LeafChained(v) => Some(v.flags),
        }
        // };
    }

    /// Called for any key events. This handles keybindings, encoding and
    /// sending to the terminal, etc.
    // pub fn keyCallback(
    //     self: *Surface,
    //     event_orig: input.KeyEvent,
    // ) !InputEffect {
    //
    // (the inferred `!InputEffect` is the [`Error`] union.)
    pub fn keyCallback(&mut self, event_orig: KeyEvent) -> Result<InputEffect, Error> {
        // log.warn("text keyCallback event={}", .{event_orig});

        // Apply key remappings to transform modifiers before any processing.
        // This allows users to remap modifier keys at the app level.
        // var event = event_orig;
        //
        // (PORT: the keyEventIsBinding move-and-read note above.)
        let mut event = event_orig;
        // if (self.config.key_remaps.isRemapped(event_orig.mods)) {
        if self.config.key_remaps.is_remapped(event.mods) {
            // event.mods = self.config.key_remaps.apply(event_orig.mods);
            event.mods = self.config.key_remaps.apply(event.mods);
        }

        // Crash metadata in case we crash in here
        // crash.sentry.thread_state = self.crashThreadState();
        crash::sentry::setThreadState(self.crashThreadState());
        // defer crash.sentry.thread_state = null;
        //
        // ([ADV 6.1] — the Drop guard clears at every exit below; declared
        // BEFORE the inspector guard so it drops AFTER it, the reference's
        // LIFO defer order.)
        let _thread_state = ThreadStateGuard;

        // Setup our inspector event if we have an inspector.
        // var insp_ev: ?inspectorpkg.KeyEvent = if (self.inspector != null) ev: {
        //     var copy = event;
        //     copy.utf8 = "";
        //     if (event.utf8.len > 0) copy.utf8 = try self.alloc.dupe(u8, event.utf8);
        //     break :ev .{ .event = copy };
        // } else null;
        //
        // (the OOM-only `try alloc.dupe` collapses to the owned clone — the
        // inspector widgets/key.rs Event::init precedent.)
        let insp_ev_: Option<inspectorpkg::KeyEvent> = if self.inspector.is_some() {
            let mut copy = event.clone();
            copy.utf8 = Vec::new();
            if !event.utf8.is_empty() {
                copy.utf8 = event.utf8.clone();
            }
            Some(inspectorpkg::KeyEvent {
                event: copy,
                ..Default::default()
            })
        } else {
            None
        };

        // When we're done processing, we always want to add the event to
        // the inspector.
        //
        // (the reference `defer if (insp_ev) |ev| ev: {...}` — Surface.zig:
        // 2674-2689 — is [`InspEvGuard`]'s `Drop`; the `if (insp_ev) |*ev| ev
        // else null` argument sites below are the `.ev.as_mut()` reborrows.)
        let mut insp_ev = InspEvGuard::<A> {
            surface: self,
            ev: insp_ev_,
        };

        // Handle keybindings first. We need to handle this on all events
        // (press, repeat, release) because a press may perform a binding but
        // a release should not encode if we consumed the press.
        // if (try self.maybeHandleBinding(
        //     event,
        //     if (insp_ev) |*ev| ev else null,
        // )) |v| return v;
        if let Some(v) = self.maybeHandleBinding(&event, insp_ev.ev.as_mut())? {
            return Ok(v);
        }

        // If we allow KAM and KAM is enabled then we do nothing.
        // if (self.config.vt_kam_allowed) {
        if self.config.vt_kam_allowed {
            // self.renderer_state.mutex.lock();
            unsafe { (*self.renderer_state.mutex).lock() };
            // defer self.renderer_state.mutex.unlock();
            // if (self.io.terminal.modes.get(.disable_keyboard)) return .consumed;
            if self
                .io
                .terminal
                .modes
                .get(terminal::modes::Mode::DisableKeyboard)
            {
                // (the deferred mutex unlock — the consumed return)
                unsafe { (*self.renderer_state.mutex).unlock() };
                return Ok(InputEffect::Consumed);
            }
            // (the deferred mutex unlock — the fall-through)
            unsafe { (*self.renderer_state.mutex).unlock() };
        }

        // If this input event has text, then we hide the mouse if configured.
        // We only do this on pressed events to avoid hiding the mouse when we
        // change focus due to a keybinding (i.e. switching tabs).
        // if (self.config.mouse_hide_while_typing and
        //     event.action == .press and
        //     !self.mouse.hidden and
        //     event.utf8.len > 0)
        if self.config.mouse_hide_while_typing
            && event.action == key::Action::Press
            && !self.mouse.hidden
            && !event.utf8.is_empty()
        {
            self.hideMouse();
        }

        // If our mouse modifiers change we may need to change our
        // link highlight state.
        // if (!self.mouse.mods.equal(event.mods)) mouse_mods: {
        if !self.mouse.mods.equal(event.mods) {
            'mouse_mods: {
                // Update our modifiers, this will update mouse mods too
                self.modsChanged(event.mods);

                // We only refresh links if
                // 1. mouse reporting is off
                // OR
                // 2. mouse reporting is on and we are not reporting shift to the terminal
                // if (self.io.terminal.flags.mouse_event == .none or
                //     (self.mouse.mods.shift and !self.mouseShiftCapture(false)))
                if self.io.terminal.flags.mouse_event == terminal::mouse::Event::None
                    || (self.mouse.mods.shift && !self.mouseShiftCapture(false))
                {
                    // Refresh our link state
                    // const pos = self.rt_surface.getCursorPos() catch break :mouse_mods;
                    let pos = match unsafe { &*self.rt_surface }.get_cursor_pos() {
                        Ok(v) => v,
                        Err(_) => break 'mouse_mods,
                    };
                    // self.renderer_state.mutex.lock();
                    unsafe { (*self.renderer_state.mutex).lock() };
                    // defer self.renderer_state.mutex.unlock();
                    // self.mouseRefreshLinks(
                    //     pos,
                    //     self.posToViewport(pos.x, pos.y),
                    //     self.mouse.over_link,
                    // ) catch |err| {
                    //     log.warn("failed to refresh links err={}", .{err});
                    //     break :mouse_mods;
                    // };
                    //
                    // (Zig widens the f32 cursor coordinates to the f64
                    // params — the selectionScrollTick precedent.)
                    let pos_vp = self.posToViewport(f64::from(pos.x), f64::from(pos.y));
                    if let Err(_err) = self.mouseRefreshLinks(pos, pos_vp, self.mouse.over_link) {
                        // log.warn("failed to refresh links err={}", .{err});
                        // (the deferred mutex unlock — the catch break)
                        unsafe { (*self.renderer_state.mutex).unlock() };
                        break 'mouse_mods;
                    }
                    // (the deferred mutex unlock — the normal exit)
                    unsafe { (*self.renderer_state.mutex).unlock() };
                // } else if (self.io.terminal.flags.mouse_event != .none and !self.mouse.mods.shift) {
                } else if self.io.terminal.flags.mouse_event != terminal::mouse::Event::None
                    && !self.mouse.mods.shift
                {
                    // If we have mouse reports on and we don't have shift pressed, we reset state
                    // _ = try self.rt_app.performAction(
                    //     .{ .surface = self },
                    //     .mouse_shape,
                    //     self.io.terminal.mouse_shape,
                    // );
                    let _ = unsafe { &*self.rt_app }.perform_action(
                        apprt::Target::Surface(&*self),
                        apprt::Action::MouseShape(self.io.terminal.mouse_shape),
                    )?;
                    // _ = try self.rt_app.performAction(
                    //     .{ .surface = self },
                    //     .mouse_over_link,
                    //     .{ .url = "" },
                    // );
                    let _ = unsafe { &*self.rt_app }.perform_action(
                        apprt::Target::Surface(&*self),
                        apprt::Action::MouseOverLink(MouseOverLink { url: b"" }),
                    )?;
                    // try self.queueRender();
                    self.queueRender()?;
                }
            }
        }

        // Process the cursor state logic. This will update the cursor shape if
        // needed, depending on the key state.
        // if ((SurfaceMouse{
        //     .physical_key = event.key,
        //     .mouse_event = self.io.terminal.flags.mouse_event,
        //     .mouse_shape = self.io.terminal.mouse_shape,
        //     .mods = self.mouse.mods,
        //     .over_link = self.mouse.over_link,
        //     .hidden = self.mouse.hidden,
        // }).keyToMouseShape()) |shape| _ = try self.rt_app.performAction(
        //     .{ .surface = self },
        //     .mouse_shape,
        //     shape,
        // );
        if let Some(shape) = (SurfaceMouse {
            physical_key: event.key,
            mouse_event: self.io.terminal.flags.mouse_event,
            mouse_shape: self.io.terminal.mouse_shape,
            mods: self.mouse.mods,
            over_link: self.mouse.over_link,
            hidden: self.mouse.hidden,
        })
        .key_to_mouse_shape()
        {
            let _ = unsafe { &*self.rt_app }.perform_action(
                apprt::Target::Surface(&*self),
                apprt::Action::MouseShape(shape),
            )?;
        }

        // We've processed a key event that produced some data so we want to
        // track the last pressed key.
        // self.pressed_key = event: {
        self.pressed_key = 'event: {
            // We need to unset the allocated fields that will become invalid
            // var copy = event;
            // copy.utf8 = "";
            let mut copy = event.clone();
            copy.utf8 = Vec::new();

            // If we have a previous pressed key and we're releasing it
            // then we set it to invalid to prevent repeating the release event.
            // if (event.action == .release) {
            if event.action == key::Action::Release {
                // if we didn't have a previous event and this is a release
                // event then we just want to set it to null.
                // const prev = self.pressed_key orelse break :event null;
                let prev = match &self.pressed_key {
                    Some(v) => v,
                    None => break 'event None,
                };
                // if (prev.key == copy.key) copy.key = .unidentified;
                if prev.key == copy.key {
                    copy.key = Key::Unidentified;
                }
            }

            // If our key is invalid and we have no mods, then we're done!
            // This helps catch the state that we naturally released all keys.
            // if (copy.key == .unidentified and copy.mods.empty()) break :event null;
            if copy.key == Key::Unidentified && copy.mods.empty() {
                break 'event None;
            }

            // break :event copy;
            Some(copy)
        };

        // Encode and send our key. If we didn't encode anything, then we
        // return the effect as ignored.
        // if (try self.encodeKey(
        //     event,
        //     if (insp_ev) |*ev| ev else null,
        // )) |write_req| {
        if let Some(write_req) = self.encodeKey(&event, insp_ev.ev.as_mut())? {
            // If our process is exited and we press a key that results in
            // an encoded value, we close the surface. We want to eventually
            // move this behavior to the apprt probably.
            // if (self.child_exited) {
            if self.child_exited {
                // self.close();
                self.close();
                // return .closed;
                return Ok(InputEffect::Closed);
            }

            // errdefer write_req.deinit();
            //
            // (Drop-owned `WriteReq`; `queueIo` cannot fail, the errdefer
            // never fires.)
            // self.queueIo(switch (write_req) {
            self.queueIo(
                match write_req {
                    // .small => |v| .{ .write_small = v },
                    MessageData::Small(v) => termio::Message::WriteSmall(v),
                    // .stable => |v| .{ .write_stable = v },
                    MessageData::Stable(v) => termio::Message::WriteStable(v),
                    // .alloc => |v| .{ .write_alloc = v },
                    MessageData::Alloc(v) => termio::Message::WriteAlloc(v),
                },
                // }, .unlocked);
                termio::termio_impl::MutexState::Unlocked,
            );
        } else {
            // No valid request means that we didn't encode anything.
            return Ok(InputEffect::Ignored);
        }

        // If our event is any keypress that isn't a modifier and we generated
        // some data to send to the pty, then we move the viewport down to the
        // bottom. We also clear the selection for any key other then modifiers.
        // if (!event.key.modifier()) {
        if !event.key.modifier() {
            // self.renderer_state.mutex.lock();
            unsafe { (*self.renderer_state.mutex).lock() };
            // defer self.renderer_state.mutex.unlock();

            // if (self.config.selection_clear_on_typing or
            //     event.key == .escape)
            if self.config.selection_clear_on_typing || event.key == Key::Escape {
                // try self.setSelection(null);
                if let Err(err) = self.setSelection(None) {
                    // (the deferred mutex unlock — the error return)
                    unsafe { (*self.renderer_state.mutex).unlock() };
                    return Err(err);
                }
            }

            // if (self.config.scroll_to_bottom.keystroke) self.io.terminal.scrollViewport(.bottom);
            if self.config.scroll_to_bottom.keystroke {
                self.io
                    .terminal
                    .scroll_viewport(terminal::terminal::ScrollViewport::Bottom);
            }

            // try self.queueRender();
            let result = self.queueRender();
            // (the deferred mutex unlock — the normal/error exit)
            unsafe { (*self.renderer_state.mutex).unlock() };
            result?;
        }

        // return .consumed;
        Ok(InputEffect::Consumed)
    }

    /// Maybe handles a binding for a given event and if so returns the effect.
    /// Returns null if the event is not handled in any way and processing should
    /// continue.
    // fn maybeHandleBinding(
    //     self: *Surface,
    //     event: input.KeyEvent,
    //     insp_ev: ?*inspectorpkg.KeyEvent,
    // ) !?InputEffect {
    //
    // (by-value `event` -> `&KeyEvent` — the Rust KeyEvent owns its utf8
    // bytes, the inspector Event::init receiver precedent; the inferred
    // `!?InputEffect` is the [`Error`] union.)
    fn maybeHandleBinding(
        &mut self,
        event: &KeyEvent,
        mut insp_ev: Option<&mut inspectorpkg::KeyEvent>,
    ) -> Result<Option<InputEffect>, Error> {
        // switch (event.action) {
        match event.action {
            // Release events never trigger a binding but we need to check if
            // we consumed the press event so we don't encode the release.
            // .release => {
            key::Action::Release => {
                // if (self.keyboard.last_trigger) |last| {
                if let Some(last) = self.keyboard.last_trigger {
                    // if (last == event.bindingHash()) {
                    if last == event.binding_hash() {
                        // We don't reset the last trigger on release because
                        // an apprt may send multiple release events for a single
                        // press event.
                        return Ok(Some(InputEffect::Consumed));
                    }
                }

                // return null;
                return Ok(None);
            }

            // Carry on processing.
            // .press, .repeat => {},
            key::Action::Press | key::Action::Repeat => {}
        }

        // Find an entry in the keybind set that matches our event.
        // const entry: input.Binding.Set.Entry = entry: {
        //
        // (the `(set, index)` entry representation — the keyEventIsBinding
        // PORT NOTE above.)
        let entry: (*const binding::Set, usize) = 'entry: {
            // Handle key sequences first.
            // if (self.keyboard.sequence_set) |set| {
            if let Some(set) = self.keyboard.sequence_set {
                // Get our entry from the set for the given event.
                // if (set.getEvent(event)) |v| break :entry v;
                if let Some(v) = unsafe { &*set }.get_event(event) {
                    break 'entry (set, v);
                }

                // No entry found. We need to encode everything up to this
                // point and send to the pty since we're in a sequence.

                // We ignore modifiers so that nested sequences such as
                // ctrl+a>ctrl+b>c work.
                // if (event.key.modifier()) return null;
                if event.key.modifier() {
                    return Ok(None);
                }

                // If we have a catch-all of ignore, then we special case our
                // invalid sequence handling to ignore it.
                // if (self.catchAllIsIgnore()) {
                if self.catchAllIsIgnore() {
                    // self.endKeySequence(.drop, .retain);
                    self.endKeySequence(KeySequenceQueued::Drop, KeySequenceMemory::Retain);
                    // return .ignored;
                    return Ok(Some(InputEffect::Ignored));
                }

                // Encode everything up to this point
                // self.endKeySequence(.flush, .retain);
                self.endKeySequence(KeySequenceQueued::Flush, KeySequenceMemory::Retain);

                // return null;
                return Ok(None);
            }

            // No currently active sequence, move on to tables. For tables,
            // we search inner-most table to outer-most. The table stack does
            // NOT include the root set.
            // const table_items = self.keyboard.table_stack.items;
            // if (table_items.len > 0) {
            //
            // (the loop performs a binding action MID-iteration —
            // `deactivate_key_table` pops this very stack — so it iterates
            // by the captured length and copies each element out; the
            // reference `const table = table_items[rev_i];` is a by-value
            // copy too.)
            let table_items_len = self.keyboard.table_stack.len();
            if table_items_len > 0 {
                // for (0..table_items.len) |i| {
                for i in 0..table_items_len {
                    // const rev_i: usize = table_items.len - 1 - i;
                    let rev_i: usize = table_items_len - 1 - i;
                    // const table = table_items[rev_i];
                    let table = self.keyboard.table_stack[rev_i];
                    // if (table.set.getEvent(event)) |v| {
                    if let Some(v) = unsafe { &*table.set }.get_event(event) {
                        // If this is a one-shot activation AND its the currently
                        // active table, then we deactivate it after this.
                        // Note: we may want to change the semantics here to
                        // remove this table no matter where it is in the stack,
                        // maybe.
                        // if (table.once and i == 0) _ = try self.performBindingAction(
                        //     .deactivate_key_table,
                        // );
                        if table.once && i == 0 {
                            let _ =
                                self.performBindingAction(binding::Action::DeactivateKeyTable)?;
                        }

                        // break :entry v;
                        break 'entry (table.set, v);
                    }
                }
            }

            // No table, use our default set
            // break :entry self.config.keybind.set.getEvent(event) orelse
            //     return null;
            match self.config.keybind.set.get_event(event) {
                Some(v) => break 'entry (&self.config.keybind.set as *const binding::Set, v),
                None => return Ok(None),
            }
        };

        // Determine if this entry has an action or if its a leader key.
        // const leaf: input.Binding.Set.GenericLeaf = switch (entry.value_ptr.*) {
        let leaf: binding::GenericLeaf = match unsafe { &*entry.0 }.value_at(entry.1) {
            // .leader => |set| {
            binding::Value::Leader(set) => {
                // Setup the next set we'll look at.
                // self.keyboard.sequence_set = set;
                self.keyboard.sequence_set = Some(&**set as *const binding::Set);

                // Store this event so that we can drain and encode on invalid.
                // We don't need to cap this because it is naturally capped by
                // the config validation.
                // if (try self.encodeKey(event, insp_ev)) |req| {
                //     try self.keyboard.sequence_queued.append(self.alloc, req);
                // }
                if let Some(req) = self.encodeKey(event, insp_ev.as_deref_mut())? {
                    self.keyboard.sequence_queued.push(req);
                }

                // Start or continue our key sequence
                // _ = self.rt_app.performAction(
                //     .{ .surface = self },
                //     .key_sequence,
                //     .{ .trigger = entry.key_ptr.* },
                // ) catch |err| {
                //     log.warn(
                //         "failed to notify app of key sequence err={}",
                //         .{err},
                //     );
                // };
                let trigger = *unsafe { &*entry.0 }.key_at(entry.1);
                if let Err(_err) = unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::Surface(&*self),
                    apprt::Action::KeySequence(KeySequence::Trigger(trigger)),
                ) {
                    // log.warn("failed to notify app of key sequence err={}", .{err});
                }

                // return .consumed;
                return Ok(Some(InputEffect::Consumed));
            }

            // inline .leaf, .leaf_chained => |leaf| leaf.generic(),
            binding::Value::Leaf(leaf) => leaf.generic(),
            binding::Value::LeafChained(leaf) => leaf.generic(),
        };

        // consumed determines if the input is consumed or if we continue
        // encoding the key (if we have a key to encode).
        // const consumed = consumed: {
        let consumed = 'consumed: {
            // If the consumed flag is explicitly set, then we are consumed.
            // if (leaf.flags.consumed) break :consumed true;
            if leaf.flags.consumed {
                break 'consumed true;
            }

            // If the global or all flag is set, we always consume.
            // if (leaf.flags.global or leaf.flags.all) break :consumed true;
            if leaf.flags.global || leaf.flags.all {
                break 'consumed true;
            }

            // break :consumed false;
            false
        };

        // We have an action, so at this point we're handling SOMETHING so
        // we reset the last trigger to null. We only set this if we actually
        // perform an action (below)
        // self.keyboard.last_trigger = null;
        self.keyboard.last_trigger = None;

        // An action also always resets the sequence set.
        // self.keyboard.sequence_set = null;
        self.keyboard.sequence_set = None;

        // Setup our actions
        // const actions = leaf.actionsSlice();
        let actions: &[binding::Action] = leaf.actions_slice();

        // Attempt to perform the action
        // log.debug("key event binding flags={} action={any}", .{
        //     leaf.flags,
        //     actions,
        // });
        // const performed = performed: {
        let performed = 'performed: {
            // If this is a global or all action, then we perform it on
            // the app and it applies to every surface.
            // if (leaf.flags.global or leaf.flags.all) {
            if leaf.flags.global || leaf.flags.all {
                // self.app.performAllChainedAction(
                //     self.rt_app,
                //     actions,
                // );
                unsafe { (*self.app).performAllChainedAction(&*self.rt_app, actions) };

                // "All" actions are always performed since they are global.
                // break :performed true;
                break 'performed true;
            }

            // Perform each action. We are performed if ANY of the chained
            // actions perform.
            // var performed: bool = false;
            let mut performed: bool = false;
            // for (actions) |action| {
            for action in actions {
                // if (self.performBindingAction(action)) |v| {
                //     performed = performed or v;
                // } else |err| {
                //     log.info(
                //         "key binding action failed action={t} err={}",
                //         .{ action, err },
                //     );
                // }
                //
                // (Zig passes the action by value — the app.rs
                // performAllChainedAction clone precedent.)
                match self.performBindingAction(action.clone()) {
                    Ok(v) => performed = performed || v,
                    Err(_err) => {
                        // log.info("key binding action failed action={t} err={}", .{ action, err });
                    }
                }
            }

            // break :performed performed;
            performed
        };

        // if (performed) {
        if performed {
            // If we performed an action and it was a closing action,
            // our "self" pointer is not safe to use anymore so we need to
            // just exit immediately.
            // for (actions) |action| if (closingAction(action)) {
            for action in actions {
                if closingAction(action) {
                    // log.debug("key binding is a closing binding, halting key event processing", .{});
                    // return .closed;
                    return Ok(Some(InputEffect::Closed));
                }
            }

            // If our action was "ignore" then we return the special input
            // effect of "ignored".
            // for (actions) |action| if (action == .ignore) {
            //
            // (Zig `==` on a tagged union with an enum literal is a TAG
            // compare; `Ignore` is payloadless so `matches!` is exact.)
            for action in actions {
                if matches!(action, binding::Action::Ignore) {
                    // If we're in a sequence, clear it.
                    // self.endKeySequence(.drop, .retain);
                    self.endKeySequence(KeySequenceQueued::Drop, KeySequenceMemory::Retain);

                    // return .ignored;
                    return Ok(Some(InputEffect::Ignored));
                }
            }
        }

        // If we have the performable flag and the action was not performed,
        // then we act as though a binding didn't exist.
        // if (leaf.flags.performable and !performed) {
        if leaf.flags.performable && !performed {
            // If we're in a sequence, we treat this as if we pressed a key
            // that doesn't exist in the sequence. Reset our sequence and flush
            // any queued events.
            // self.endKeySequence(.flush, .retain);
            self.endKeySequence(KeySequenceQueued::Flush, KeySequenceMemory::Retain);

            // return null;
            return Ok(None);
        }

        // If we consume this event, then we are done. If we don't consume
        // it, we processed the action but we still want to process our
        // encodings, too.
        // if (consumed) {
        if consumed {
            // If we had queued events, we deinit them since we consumed
            // self.endKeySequence(.drop, .retain);
            self.endKeySequence(KeySequenceQueued::Drop, KeySequenceMemory::Retain);

            // Store our last trigger so we don't encode the release event
            // self.keyboard.last_trigger = event.bindingHash();
            self.keyboard.last_trigger = Some(event.binding_hash());

            // if (insp_ev) |ev| {
            if let Some(ev) = insp_ev {
                // ev.binding = self.alloc.dupe(
                //     input.Binding.Action,
                //     actions,
                // ) catch |err| binding: {
                //     log.warn(
                //         "error allocating binding action for inspector err={}",
                //         .{err},
                //     );
                //     break :binding &.{};
                // };
                //
                // (the OOM-only catch collapses — infallible global
                // allocator; the `&.{}` fallback arm is unreachable.)
                ev.binding = actions.to_vec();
            }
            // return .consumed;
            return Ok(Some(InputEffect::Consumed));
        }

        // If we didn't perform OR we didn't consume, then we want to
        // encode any queued events for a sequence.
        // self.endKeySequence(.flush, .retain);
        self.endKeySequence(KeySequenceQueued::Flush, KeySequenceMemory::Retain);

        // return null;
        Ok(None)
    }

    // fn deactivateAllKeyTables(self: *Surface) !bool {
    //
    // (the inferred `!bool` — the body's only failure is `catch`-swallowed,
    // so the set is empty; the opaque [`Error`] marker keeps the callers'
    // `try` shape — updateConfig :1755.)
    fn deactivateAllKeyTables(&mut self) -> Result<bool, Error> {
        // switch (self.keyboard.table_stack.items.len) {
        match self.keyboard.table_stack.len() {
            // No key table active. This does nothing.
            // 0 => return false,
            0 => return Ok(false),

            // Clear the entire table stack.
            // else => self.keyboard.table_stack.clearAndFree(self.alloc),
            //
            // (`clearAndFree` — drop the storage too, the deinit mem::take
            // precedent.)
            _ => drop(std::mem::take(&mut self.keyboard.table_stack)),
        }

        // Notify the UI.
        // _ = self.rt_app.performAction(
        //     .{ .surface = self },
        //     .key_table,
        //     .deactivate_all,
        // ) catch |err| {
        //     log.warn(
        //         "failed to notify app of key table err={}",
        //         .{err},
        //     );
        // };
        if let Err(_err) = unsafe { &*self.rt_app }.perform_action(
            apprt::Target::Surface(&*self),
            apprt::Action::KeyTable(KeyTable::DeactivateAll),
        ) {
            // log.warn("failed to notify app of key table err={}", .{err});
        }

        // return true;
        Ok(true)
    }

    /// This checks if the current keybinding sets have a catch_all binding
    /// with `ignore`. This is used to determine some special input cases.
    // fn catchAllIsIgnore(self: *Surface) bool {
    fn catchAllIsIgnore(&mut self) -> bool {
        // Get our catch all
        // const entry: input.Binding.Set.Entry = entry: {
        //
        // (the `(set, index)` entry representation — the keyEventIsBinding
        // PORT NOTE above; `Set.get` is the ported `get_index`.)
        let entry: (*const binding::Set, usize) = 'entry: {
            // const trigger: input.Binding.Trigger = .{ .key = .catch_all };
            let trigger = binding::Trigger {
                key: binding::TriggerKey::CatchAll,
                ..Default::default()
            };

            // const table_items = self.keyboard.table_stack.items;
            let table_items = &self.keyboard.table_stack;
            // for (0..table_items.len) |i| {
            for i in 0..table_items.len() {
                // const rev_i: usize = table_items.len - 1 - i;
                let rev_i: usize = table_items.len() - 1 - i;
                // const entry = table_items[rev_i].set.get(trigger) orelse continue;
                let entry = match unsafe { &*table_items[rev_i].set }.get_index(trigger) {
                    Some(v) => v,
                    None => continue,
                };
                // break :entry entry;
                break 'entry (table_items[rev_i].set, entry);
            }

            // break :entry self.config.keybind.set.get(trigger) orelse
            //     return false;
            match self.config.keybind.set.get_index(trigger) {
                Some(v) => break 'entry (&self.config.keybind.set as *const binding::Set, v),
                None => return false,
            }
        };

        // We have a catch-all entry, see if its an ignore
        // return switch (entry.value_ptr.*) {
        match unsafe { &*entry.0 }.value_at(entry.1) {
            // .leader => false,
            binding::Value::Leader(_) => false,
            // .leaf => |leaf| leaf.action == .ignore,
            binding::Value::Leaf(leaf) => matches!(leaf.action, binding::Action::Ignore),
            // .leaf_chained => |leaf| chained: for (leaf.actions.items) |action| {
            //     if (action == .ignore) break :chained true;
            // } else false,
            binding::Value::LeafChained(leaf) => 'chained: {
                for action in &leaf.actions {
                    if matches!(action, binding::Action::Ignore) {
                        break 'chained true;
                    }
                }
                false
            }
        }
        // };
    }

    /// End a key sequence. Safe to call if no key sequence is active.
    ///
    /// Action and mem determine the behavior of the queued inputs up to this
    /// point.
    // fn endKeySequence(
    //     self: *Surface,
    //     action: KeySequenceQueued,
    //     mem: KeySequenceMemory,
    // ) void {
    fn endKeySequence(&mut self, action: KeySequenceQueued, mem: KeySequenceMemory) {
        // Notify apprt key sequence ended
        // _ = self.rt_app.performAction(
        //     .{ .surface = self },
        //     .key_sequence,
        //     .end,
        // ) catch |err| {
        //     log.warn(
        //         "failed to notify app of key sequence end err={}",
        //         .{err},
        //     );
        // };
        if let Err(_err) = unsafe { &*self.rt_app }.perform_action(
            apprt::Target::Surface(&*self),
            apprt::Action::KeySequence(KeySequence::End),
        ) {
            // log.warn("failed to notify app of key sequence end err={}", .{err});
        }

        // No matter what we clear our current sequence set. This restores
        // the set we look at to the root set.
        // self.keyboard.sequence_set = null;
        self.keyboard.sequence_set = None;

        // If we have no queued data, there is nothing else to do.
        // if (self.keyboard.sequence_queued.items.len == 0) return;
        if self.keyboard.sequence_queued.is_empty() {
            return;
        }

        // Run the proper action first
        //
        // PORT NOTE: the Zig loops copy each queued request out of the list
        // (`items` stays populated until the `mem` switch); the Rust
        // `WriteReq` is move-only, so the list moves out for the loop and
        // the emptied (capacity-retaining) vec goes back for the `mem`
        // switch below — same observable content.
        let mut queued = std::mem::take(&mut self.keyboard.sequence_queued);
        // switch (action) {
        match action {
            // .flush => for (self.keyboard.sequence_queued.items) |write_req| {
            //     self.queueIo(switch (write_req) {
            //         .small => |v| .{ .write_small = v },
            //         .stable => |v| .{ .write_stable = v },
            //         .alloc => |v| .{ .write_alloc = v },
            //     }, .unlocked);
            // },
            KeySequenceQueued::Flush => {
                for write_req in queued.drain(..) {
                    self.queueIo(
                        match write_req {
                            MessageData::Small(v) => termio::Message::WriteSmall(v),
                            MessageData::Stable(v) => termio::Message::WriteStable(v),
                            MessageData::Alloc(v) => termio::Message::WriteAlloc(v),
                        },
                        termio::termio_impl::MutexState::Unlocked,
                    );
                }
            }

            // .drop => for (self.keyboard.sequence_queued.items) |req| req.deinit(),
            //
            // (`WriteReq.deinit` — the alloc-variant free — is Drop in
            // rustty; the deinit precedent.)
            KeySequenceQueued::Drop => {
                for req in queued.drain(..) {
                    drop(req);
                }
            }
        }
        self.keyboard.sequence_queued = queued;

        // Memory handling of the sequence after the action
        // switch (mem) {
        match mem {
            // .free => self.keyboard.sequence_queued.clearAndFree(self.alloc),
            KeySequenceMemory::Free => drop(std::mem::take(&mut self.keyboard.sequence_queued)),
            // .retain => self.keyboard.sequence_queued.clearRetainingCapacity(),
            KeySequenceMemory::Retain => self.keyboard.sequence_queued.clear(),
        }
    }

    /// Encodes the key event into a write request. The write request will
    /// always copy or allocate so the caller can safely free the event.
    // fn encodeKey(
    //     self: *Surface,
    //     event: input.KeyEvent,
    //     insp_ev: ?*inspectorpkg.KeyEvent,
    // ) !?termio.Message.WriteReq {
    //
    // (by-value `event` -> `&KeyEvent`, the maybeHandleBinding note; the
    // inferred `!?WriteReq` is the [`Error`] union at the `'static` queued
    // instantiation.)
    fn encodeKey(
        &mut self,
        event: &KeyEvent,
        insp_ev: Option<&mut inspectorpkg::KeyEvent>,
    ) -> Result<Option<termio::message::WriteReq<'static>>, Error> {
        // const write_req: termio.Message.WriteReq = req: {
        let write_req: termio::message::WriteReq<'static> = 'req: {
            // Build our encoding options, which requires the lock.
            // const encoding_opts = self.encodeKeyOpts();
            let encoding_opts = self.encodeKeyOpts();

            // Try to write the input into a small array. This fits almost
            // every scenario. Larger situations can happen due to long
            // pre-edits.
            // var data: termio.Message.WriteReq.Small.Array = undefined;
            // var writer: std.Io.Writer = .fixed(&data);
            //
            // PORT NOTE: `Small.Array` == `[u8; WriteReq::MAX]`; Zig
            // `undefined` -> zeroed (the tail past `len` is never read). The
            // `.fixed` writer is a Cursor over the stack array — a write
            // past the end is io's `WriteZero`, the reference's
            // `error.WriteFailed`.
            let mut data = [0u8; termio::message::WriteReq::MAX];
            let mut writer = std::io::Cursor::new(&mut data[..]);
            // if (input.key_encode.encode(
            //     &writer,
            //     event,
            //     encoding_opts,
            // )) {
            if let Ok(()) = key_encode::encode(&mut writer, event, encoding_opts) {
                // const written = writer.buffered();
                let written_len = writer.position() as usize;

                // Special-case: we did nothing.
                // if (written.len == 0) return null;
                if written_len == 0 {
                    return Ok(None);
                }

                // break :req .{ .small = .{
                //     .data = data,
                //     .len = @intCast(written.len),
                // } };
                break 'req MessageData::Small(Small {
                    data,
                    len: written_len,
                });
            }
            // } else |err| switch (err) {
            //     // Means we need to allocate
            //     error.WriteFailed => {},
            // }
            //
            // (the fixed Cursor's only failure is buffer-full — the
            // `error.WriteFailed` arm; fall through to allocate.)

            // We need to allocate. We allocate double the UTF-8 length
            // or double the small array size, whichever is larger. That's
            // a heuristic that should work. The only scenario I know while
            // typing this where we don't have enough space is a long preedit,
            // and in that case the size we need is exactly the UTF-8 length,
            // so the double is being safe.
            // var alloc_writer: std.Io.Writer.Allocating = try .initCapacity(
            //     self.alloc,
            //     @max(event.utf8.len * 2, data.len * 2),
            // );
            // defer alloc_writer.deinit();
            //
            // (the growable `Vec` sink is the Allocating writer; the defer
            // is its drop — `WriteReq.init` below copies out of it.)
            let mut alloc_writer: Vec<u8> =
                Vec::with_capacity(std::cmp::max(event.utf8.len() * 2, data.len() * 2));

            // This results in a double allocation but this is such an unlikely
            // path the performance impact is unimportant.
            // try input.key_encode.encode(
            //     &alloc_writer.writer,
            //     event,
            //     encoding_opts,
            // );
            //
            // (the Allocating writer's only error is OOM — the `Vec` sink is
            // infallible, the `try` collapses.)
            key_encode::encode(&mut alloc_writer, event, encoding_opts)
                .expect("Vec writer is infallible");
            // break :req try termio.Message.WriteReq.init(
            //     self.alloc,
            //     alloc_writer.writer.buffered(),
            // );
            //
            // (`WriteReq.init`'s only error is OOM — collapses, the termio
            // message.rs writeReq precedent.)
            break 'req termio::message::WriteReq::init(&alloc_writer);
        };

        // Copy the encoded data into the inspector event if we have one.
        // We do this before the mailbox because the IO thread could
        // release the memory before we get a chance to copy it.
        // if (insp_ev) |ev| pty: {
        if let Some(ev) = insp_ev {
            // const slice = write_req.slice();
            let slice = write_req.slice();
            // const copy = self.alloc.alloc(u8, slice.len) catch |err| {
            //     log.warn("error allocating pty data for inspector err={}", .{err});
            //     break :pty;
            // };
            // errdefer self.alloc.free(copy);
            // @memcpy(copy, slice);
            //
            // (the OOM-only catch + errdefer collapse — infallible owned
            // copy.)
            let copy = slice.to_vec();
            // ev.pty = copy;
            ev.pty = copy;
        }

        // return write_req;
        Ok(Some(write_req))
    }

    // fn encodeKeyOpts(self: *const Surface) input.key_encode.Options {
    fn encodeKeyOpts(&self) -> key_encode::Options {
        // self.renderer_state.mutex.lock();
        unsafe { (*self.renderer_state.mutex).lock() };
        // defer self.renderer_state.mutex.unlock();
        // const t = &self.io.terminal;
        let t = &self.io.terminal;

        // var opts: input.key_encode.Options = .fromTerminal(t);
        let mut opts: key_encode::Options = key_encode::Options::from_terminal(t);
        // if (comptime builtin.os.tag != .macos) return opts;
        //
        // (`cfg!` — the comptime OS check, the init xdg-terminal-exec
        // precedent.)
        if cfg!(not(target_os = "macos")) {
            // (the deferred mutex unlock — the non-macOS comptime return)
            unsafe { (*self.renderer_state.mutex).unlock() };
            return opts;
        }

        // opts.macos_option_as_alt = self.config.macos_option_as_alt orelse detect: {
        //
        // PORT NOTE (crate-split glue): the reference has ONE
        // `input.OptionAsAlt`; the port has the config-side enum
        // (rustty-input — `self.config` and `detectOptionAsAlt`) and the
        // VT-side copy (the key_encode Options field, duplicated there so
        // the VT key encoder has no config dependency). The 1:1 variant map
        // below is the seam (identical discriminants).
        let v: OptionAsAlt = match self.config.macos_option_as_alt {
            Some(v) => v,
            None => 'detect: {
                // If we don't have alt pressed, it doesn't matter what this
                // config is so we can just say "false" and break out and avoid
                // more expensive checks below.
                // if (!self.mouse.mods.alt) break :detect .false;
                if !self.mouse.mods.alt {
                    break 'detect OptionAsAlt::False;
                }

                // Alt is pressed, we're on macOS. We break some encapsulation
                // here and assume libghostty for ease...
                // break :detect self.rt_app.keyboardLayout().detectOptionAsAlt();
                break 'detect unsafe { &*self.rt_app }
                    .keyboard_layout()
                    .detect_option_as_alt();
            }
        };
        opts.macos_option_as_alt = match v {
            OptionAsAlt::False => rustty_vt::input::config::OptionAsAlt::r#false,
            OptionAsAlt::True => rustty_vt::input::config::OptionAsAlt::r#true,
            OptionAsAlt::Left => rustty_vt::input::config::OptionAsAlt::left,
            OptionAsAlt::Right => rustty_vt::input::config::OptionAsAlt::right,
        };

        // return opts;
        // (the deferred mutex unlock — the macOS return)
        unsafe { (*self.renderer_state.mutex).unlock() };
        opts
    }

    /// Sends text as-is to the terminal without triggering any keyboard
    /// protocol. This will treat the input text as if it was pasted
    /// from the clipboard so the same logic will be applied. Namely,
    /// if bracketed mode is on this will do a bracketed paste. Otherwise,
    /// this will filter newlines to '\r'.
    // pub fn textCallback(self: *Surface, text: []const u8) !void {
    //
    // (raw bytes `&[u8]`, never a lossy String; the inferred `!void` is
    // `completeClipboardPaste`'s — the [`Error`] union.)
    pub fn textCallback(&mut self, text: &[u8]) -> Result<(), Error> {
        // Crash metadata in case we crash in here
        // crash.sentry.thread_state = self.crashThreadState();
        crash::sentry::setThreadState(self.crashThreadState());
        // defer crash.sentry.thread_state = null;
        // ([ADV 6.1] — the Drop guard clears at every exit below.)
        let _thread_state = ThreadStateGuard;

        // try self.completeClipboardPaste(text, true);
        self.completeClipboardPaste(text, true)?;
        Ok(())
    }

    /// Callback for when the surface is fully visible or not, regardless
    /// of focus state. This is used to pause rendering when the surface
    /// is not visible, and also re-render when it becomes visible again.
    // pub fn occlusionCallback(self: *Surface, visible: bool) !void {
    //
    // (the inferred `!void` is `queueRender`'s xev notify error — carried by
    // the [`Error`] union, the sizeCallback convention.)
    pub fn occlusionCallback(&mut self, visible: bool) -> Result<(), Error> {
        // Crash metadata in case we crash in here
        // crash.sentry.thread_state = self.crashThreadState();
        crash::sentry::setThreadState(self.crashThreadState());
        // defer crash.sentry.thread_state = null;
        // ([ADV 6.1] — the Drop guard clears at every exit below.)
        let _thread_state = ThreadStateGuard;

        // _ = self.renderer_thread.mailbox.push(.{
        //     .visible = visible,
        // }, .{ .forever = {} });
        let _ = unsafe { &*self.renderer_thread.mailbox }
            .push(rendererpkg::Message::Visible(visible), Timeout::Forever);
        // try self.queueRender();
        self.queueRender()?;
        Ok(())
    }

    // pub fn focusCallback(self: *Surface, focused: bool) !void {
    pub fn focusCallback(&mut self, focused: bool) -> Result<(), Error> {
        // Crash metadata in case we crash in here
        // crash.sentry.thread_state = self.crashThreadState();
        crash::sentry::setThreadState(self.crashThreadState());
        // defer crash.sentry.thread_state = null;
        // ([ADV 6.1] — the Drop guard clears at every exit below.)
        let _thread_state = ThreadStateGuard;

        // Always update the app focused surface, otherwise we miss
        // the first surface created.
        // if (focused) self.app.focusSurface(self);
        if focused {
            let app = self.app;
            unsafe { (*app).focusSurface(self) };
        }

        // If our focus state is unchanged we do nothing else.
        // if (self.focused == focused) return;
        if self.focused == focused {
            return Ok(());
        }
        // self.focused = focused;
        self.focused = focused;

        // Notify our render thread of the new state
        // _ = self.renderer_thread.mailbox.push(.{
        //     .focus = focused,
        // }, .{ .forever = {} });
        let _ = unsafe { &*self.renderer_thread.mailbox }
            .push(rendererpkg::Message::Focus(focused), Timeout::Forever);

        // if (!focused) unfocused: {
        if !focused {
            'unfocused: {
                // If we lost focus and we have a keypress, then we want to send a key
                // release event for it. Depending on the apprt, this CAN result in
                // duplicate key release events, but that is better than not sending
                // a key release event at all.
                // var pressed_key = self.pressed_key orelse break :unfocused;
                // self.pressed_key = null;
                //
                // (the `orelse` value copy + the null-out collapse into the
                // owning `take()` — KeyEvent owns its utf8 bytes.)
                let mut pressed_key = match self.pressed_key.take() {
                    Some(v) => v,
                    None => break 'unfocused,
                };

                // All our actions will be releases
                // pressed_key.action = .release;
                pressed_key.action = key::Action::Release;

                // Release the full key first
                // if (pressed_key.key != .unidentified) {
                if pressed_key.key != Key::Unidentified {
                    // assert(self.keyCallback(pressed_key) catch |err| err: {
                    //     log.warn("error releasing key on focus loss err={}", .{err});
                    //     break :err .ignored;
                    // } != .closed);
                    //
                    // (`assert` -> `debug_assert!` on the hoisted keyCallback
                    // result — the side-effecting call always evaluates, only
                    // the comparison is safety-gated; the caught error
                    // collapses to the `.ignored` fallback, the log is the
                    // comment above.)
                    let effect = self
                        .keyCallback(pressed_key.clone())
                        .unwrap_or(InputEffect::Ignored);
                    debug_assert!(effect != InputEffect::Closed);
                }

                // Release any modifiers if set
                // if (pressed_key.mods.empty()) break :unfocused;
                if pressed_key.mods.empty() {
                    break 'unfocused;
                }

                // This is kind of nasty comptime meta programming but all we're doing
                // here is going through all the modifiers and if they're set, releasing
                // both the left and right sides of the modifier. This may not match
                // the exact input event but it ensures a full reset.
                // const keys = &.{ "shift", "ctrl", "alt", "super" };
                // const original_key = pressed_key.key;
                let original_key = pressed_key.key;
                // inline for (keys) |key| {
                //     if (@field(pressed_key.mods, key)) {
                //         @field(pressed_key.mods, key) = false;
                //         inline for (&.{ "right", "left" }) |side| {
                //             const keyname = comptime keyname: {
                //                 break :keyname if (std.mem.eql(u8, key, "ctrl"))
                //                     "control"
                //                 else if (std.mem.eql(u8, key, "super"))
                //                     "meta"
                //                 else
                //                     key;
                //             };
                //             pressed_key.key = @field(input.Key, keyname ++ "_" ++ side);
                //             if (pressed_key.key != original_key) {
                //                 assert(self.keyCallback(pressed_key) catch |err| err: {
                //                     log.warn("error releasing key on focus loss err={}", .{err});
                //                     break :err .ignored;
                //                 } != .closed);
                //             }
                //         }
                //     }
                // }
                //
                // PORT NOTE (comptime unroll): Rust has no `@field`; the
                // local macro replays the comptime loop — same mods order
                // (shift, ctrl, alt, super), same side order (right, left),
                // same keyname map (ctrl -> control, super -> meta); the
                // inner assert maps exactly as the release-the-full-key site
                // above.
                macro_rules! release_mod {
                    ($field:ident, $right:ident, $left:ident) => {
                        if pressed_key.mods.$field {
                            pressed_key.mods.$field = false;
                            for side in [Key::$right, Key::$left] {
                                pressed_key.key = side;
                                if pressed_key.key != original_key {
                                    let effect = self
                                        .keyCallback(pressed_key.clone())
                                        .unwrap_or(InputEffect::Ignored);
                                    debug_assert!(effect != InputEffect::Closed);
                                }
                            }
                        }
                    };
                }
                release_mod!(shift, ShiftRight, ShiftLeft);
                release_mod!(ctrl, ControlRight, ControlLeft);
                release_mod!(alt, AltRight, AltLeft);
                release_mod!(super_, MetaRight, MetaLeft);
            }
        }

        // Schedule render which also drains our mailbox
        // try self.queueRender();
        self.queueRender()?;

        // Whenever our focus changes we unhide the mouse. The mouse will be
        // hidden again if the user starts typing. This helps alleviate some
        // buggy behavior upstream in macOS with the mouse never becoming visible
        // again when tabbing between programs (see #2525).
        // self.showMouse();
        self.showMouse();

        // Update the focus state and notify the terminal
        {
            // self.renderer_state.mutex.lock();
            unsafe { (*self.renderer_state.mutex).lock() };
            // self.io.terminal.flags.focused = focused;
            self.io.terminal.flags.focused = focused;
            // self.renderer_state.mutex.unlock();
            unsafe { (*self.renderer_state.mutex).unlock() };
            // self.queueIo(.{ .focused = focused }, .unlocked);
            self.queueIo(
                termio::Message::Focused(focused),
                termio::termio_impl::MutexState::Unlocked,
            );
        }
        Ok(())
    }

    // pub fn refreshCallback(self: *Surface) !void {
    pub fn refreshCallback(&mut self) -> Result<(), Error> {
        // Crash metadata in case we crash in here
        // crash.sentry.thread_state = self.crashThreadState();
        crash::sentry::setThreadState(self.crashThreadState());
        // defer crash.sentry.thread_state = null;
        // ([ADV 6.1] — the Drop guard clears at every exit below.)
        let _thread_state = ThreadStateGuard;

        // The point of this callback is to schedule a render, so do that.
        // try self.queueRender();
        self.queueRender()?;
        Ok(())
    }

    // PORT NOTE: the file-scope `ScrollAmount` struct sits HERE in the
    // reference (Surface.zig:3400-3415, between refreshCallback and
    // scrollCallback); Rust impl blocks cannot hold type items, so it is
    // ported at module scope below the impl (the KeySequenceQueued trailing
    // cluster) — see [`ScrollAmount`].

    /// Mouse scroll event. Negative is down, left. Positive is up, right.
    ///
    /// "Natural scrolling" is a macOS term for inverting the scroll direction.
    /// This should be handled by the apprt implementation. At this layer,
    /// negative is always down, left.
    // pub fn scrollCallback(
    //     self: *Surface,
    //     xoff: f64,
    //     yoff: f64,
    //     scroll_mods: input.ScrollMods,
    // ) !void {
    pub fn scrollCallback(
        &mut self,
        xoff: f64,
        yoff: f64,
        scroll_mods: input_mouse::ScrollMods,
    ) -> Result<(), Error> {
        // log.info("SCROLL: xoff={} yoff={} mods={}", .{ xoff, yoff, scroll_mods });

        // Crash metadata in case we crash in here
        // crash.sentry.thread_state = self.crashThreadState();
        crash::sentry::setThreadState(self.crashThreadState());
        // defer crash.sentry.thread_state = null;
        // ([ADV 6.1] — the Drop guard clears at every exit below, AFTER the
        // explicit mutex unlocks: the reference's LIFO defer order.)
        let _thread_state = ThreadStateGuard;

        // Always show the mouse again if it is hidden
        // if (self.mouse.hidden) self.showMouse();
        if self.mouse.hidden {
            self.showMouse();
        }

        // const y: ScrollAmount = if (yoff == 0) .{} else y: {
        let y: ScrollAmount = if yoff == 0.0 {
            ScrollAmount::default()
        } else {
            'y: {
                // We use cell_size to determine if we have accumulated enough to trigger a scroll
                // const cell_size: f64 = @floatFromInt(self.size.cell.height);
                let cell_size: f64 = f64::from(self.size.cell.height);

                // If we have precision scroll, yoff is the number of pixels to scroll. In non-precision
                // scroll, yoff is the number of wheel ticks. Some mice are capable of reporting fractional
                // wheel ticks, which don't necessarily get reported as precision scrolls. We normalize all
                // scroll events to pixels by multiplying the wheel tick value and the cell size. This means
                // that a wheel tick of 1 results in single scroll event.
                // const yoff_adjusted: f64 = if (scroll_mods.precision)
                //     yoff * self.config.mouse_scroll_multiplier.precision
                // else yoff_adjusted: {
                let yoff_adjusted: f64 = if scroll_mods.precision {
                    yoff * self.config.mouse_scroll_multiplier.precision
                } else {
                    'yoff_adjusted: {
                        // if (comptime builtin.target.os.tag.isDarwin()) {
                        //
                        // (`cfg!` — the childExitedAbnormally isDarwin
                        // precedent.)
                        if cfg!(target_vendor = "apple") {
                            // Round out the yoff to an absolute minimum of 1. macos tries to
                            // simulate precision scrolling with non precision events by
                            // ramping up the magnitude of the offsets as it detects faster
                            // scrolling. Single click (very slow) scrolls are reported with a
                            // magnitude of 0.1 which would normally require a few clicks
                            // before we register an actual scroll event (depending on cell
                            // height and the mouse_scroll_multiplier setting).
                            // const yoff_max: f64 = if (yoff > 0)
                            //     @max(yoff, 1)
                            // else
                            //     @min(yoff, -1);
                            let yoff_max: f64 = if yoff > 0.0 {
                                yoff.max(1.0)
                            } else {
                                yoff.min(-1.0)
                            };

                            // break :yoff_adjusted yoff_max * cell_size * self.config.mouse_scroll_multiplier.discrete;
                            break 'yoff_adjusted yoff_max
                                * cell_size
                                * self.config.mouse_scroll_multiplier.discrete;
                        } else {
                            // break :yoff_adjusted yoff * cell_size * self.config.mouse_scroll_multiplier.discrete;
                            break 'yoff_adjusted yoff
                                * cell_size
                                * self.config.mouse_scroll_multiplier.discrete;
                        }
                    }
                };

                // Add our previously saved pending amount to the offset to get the
                // new offset value. The signs of the pending and yoff should match
                // so that we move further away from zero, but we don't assert
                // this because in theory a user could scroll in the opposite
                // direction and undo a pending scroll.
                // const poff: f64 = self.mouse.pending_scroll_y + yoff_adjusted;
                let poff: f64 = self.mouse.pending_scroll_y + yoff_adjusted;

                // If the new offset is less than a single unit of scroll, we save
                // the new pending value and do not scroll yet.
                // if (@abs(poff) < cell_size) {
                if poff.abs() < cell_size {
                    // self.mouse.pending_scroll_y = poff;
                    self.mouse.pending_scroll_y = poff;
                    // break :y .{};
                    break 'y ScrollAmount::default();
                }

                // We scroll by the number of rows in the offset and save the remainder
                // const amount = poff / cell_size;
                let amount = poff / cell_size;
                // assert(@abs(amount) >= 1);
                debug_assert!(amount.abs() >= 1.0);
                // self.mouse.pending_scroll_y = poff - (amount * cell_size);
                self.mouse.pending_scroll_y = poff - (amount * cell_size);

                // Round towards zero.
                // const delta: isize = @intFromFloat(@trunc(amount));
                let delta: isize = amount.trunc() as isize;
                // assert(@abs(delta) >= 1);
                debug_assert!(delta.unsigned_abs() >= 1);

                // break :y .{ .delta = delta };
                break 'y ScrollAmount { delta };
            }
        };

        // For detailed comments see the y calculation above.
        // const x: ScrollAmount = if (xoff == 0) .{} else x: {
        let x: ScrollAmount = if xoff == 0.0 {
            ScrollAmount::default()
        } else {
            'x: {
                // if (!scroll_mods.precision) {
                if !scroll_mods.precision {
                    // const x_delta_isize: isize = @intFromFloat(@round(xoff));
                    let x_delta_isize: isize = xoff.round() as isize;
                    // break :x .{ .delta = x_delta_isize };
                    break 'x ScrollAmount {
                        delta: x_delta_isize,
                    };
                }

                // const poff: f64 = self.mouse.pending_scroll_x + xoff;
                let poff: f64 = self.mouse.pending_scroll_x + xoff;
                // const cell_size: f64 = @floatFromInt(self.size.cell.width);
                let cell_size: f64 = f64::from(self.size.cell.width);
                // if (@abs(poff) < cell_size) {
                if poff.abs() < cell_size {
                    // self.mouse.pending_scroll_x = poff;
                    self.mouse.pending_scroll_x = poff;
                    // break :x .{};
                    break 'x ScrollAmount::default();
                }

                // const amount = poff / cell_size;
                let amount = poff / cell_size;
                // assert(@abs(amount) >= 1);
                debug_assert!(amount.abs() >= 1.0);
                // self.mouse.pending_scroll_x = poff - (amount * cell_size);
                self.mouse.pending_scroll_x = poff - (amount * cell_size);
                // const delta: isize = @intFromFloat(@trunc(amount));
                let delta: isize = amount.trunc() as isize;
                // assert(@abs(delta) >= 1);
                debug_assert!(delta.unsigned_abs() >= 1);
                // break :x .{ .delta = delta };
                break 'x ScrollAmount { delta };
            }
        };

        // log.info("SCROLL: delta_y={} delta_x={}", .{ y.delta, x.delta });

        {
            // self.renderer_state.mutex.lock();
            // defer self.renderer_state.mutex.unlock();
            //
            // (guard-less zigstd Mutex — CORE_BLUEPRINT §3; the defer is the
            // explicit unlock at every exit below.)
            unsafe { (*self.renderer_state.mutex).lock() };

            // If we have an active mouse reporting mode, clear the selection.
            // The selection can occur if the user uses the shift mod key to
            // override mouse grabbing from the window.
            // if (self.isMouseReporting()) {
            if self.isMouseReporting() {
                // try self.setSelection(null);
                if let Err(err) = self.setSelection(None) {
                    // (the deferred mutex unlock — the error exit)
                    unsafe { (*self.renderer_state.mutex).unlock() };
                    return Err(err);
                }
            }

            // If we're in alternate screen with alternate scroll enabled, then
            // we convert to cursor keys. This only happens if we're:
            // (1) alt screen (2) no explicit mouse reporting and (3) alt
            // scroll mode enabled.
            // if (self.io.terminal.screens.active_key == .alternate and
            //     self.io.terminal.flags.mouse_event == .none and
            //     self.io.terminal.modes.get(.mouse_alternate_scroll))
            // {
            if self.io.terminal.screens.active_key == terminal::screen_set::Key::Alternate
                && self.io.terminal.flags.mouse_event == terminal::mouse::Event::None
                && self
                    .io
                    .terminal
                    .modes
                    .get(terminal::modes::Mode::MouseAlternateScroll)
            {
                // if (y.delta != 0) {
                if y.delta != 0 {
                    // When we send mouse events as cursor keys we always
                    // clear the selection.
                    // try self.setSelection(null);
                    if let Err(err) = self.setSelection(None) {
                        // (the deferred mutex unlock — the error exit)
                        unsafe { (*self.renderer_state.mutex).unlock() };
                        return Err(err);
                    }

                    // const seq = if (self.io.terminal.modes.get(.cursor_keys)) seq: {
                    //     // cursor key: application mode
                    //     break :seq switch (y.direction()) {
                    //         .up_right => "\x1bOA",
                    //         .down_left => "\x1bOB",
                    //     };
                    // } else seq: {
                    //     // cursor key: normal mode
                    //     break :seq switch (y.direction()) {
                    //         .up_right => "\x1b[A",
                    //         .down_left => "\x1b[B",
                    //     };
                    // };
                    //
                    // (the `[:0]const u8` literals are static byte strings.)
                    let seq: &'static [u8] = if self
                        .io
                        .terminal
                        .modes
                        .get(terminal::modes::Mode::CursorKeys)
                    {
                        // cursor key: application mode
                        match y.direction() {
                            ScrollAmountDirection::UpRight => b"\x1bOA",
                            ScrollAmountDirection::DownLeft => b"\x1bOB",
                        }
                    } else {
                        // cursor key: normal mode
                        match y.direction() {
                            ScrollAmountDirection::UpRight => b"\x1b[A",
                            ScrollAmountDirection::DownLeft => b"\x1b[B",
                        }
                    };
                    // for (0..y.magnitude()) |_| {
                    for _ in 0..y.magnitude() {
                        // self.queueIo(.{ .write_stable = seq }, .locked);
                        self.queueIo(
                            termio::Message::WriteStable(seq),
                            termio::termio_impl::MutexState::Locked,
                        );
                    }
                }

                // return;
                // (the deferred mutex unlock — the alternate-scroll return)
                unsafe { (*self.renderer_state.mutex).unlock() };
                return Ok(());
            }

            // We have mouse events, are not in an alternate scroll buffer,
            // or have alternate scroll disabled. In this case, we just run
            // the normal logic.

            // If we're scrolling up or down, then send a mouse event.
            // if (self.isMouseReporting()) {
            if self.isMouseReporting() {
                // for (0..@abs(y.delta)) |_| {
                for _ in 0..y.delta.unsigned_abs() {
                    // const pos = try self.rt_surface.getCursorPos();
                    let pos = match unsafe { &*self.rt_surface }.get_cursor_pos() {
                        Ok(v) => v,
                        Err(err) => {
                            // (the deferred mutex unlock — the error exit)
                            unsafe { (*self.renderer_state.mutex).unlock() };
                            return Err(err.into());
                        }
                    };
                    // self.mouseReport(switch (y.direction()) {
                    //     .up_right => .four,
                    //     .down_left => .five,
                    // }, .press, self.mouse.mods, pos);
                    self.mouseReport(
                        Some(match y.direction() {
                            ScrollAmountDirection::UpRight => input_mouse::Button::Four,
                            ScrollAmountDirection::DownLeft => input_mouse::Button::Five,
                        }),
                        input_mouse::Action::Press,
                        self.mouse.mods,
                        pos,
                    );
                }

                // for (0..@abs(x.delta)) |_| {
                for _ in 0..x.delta.unsigned_abs() {
                    // const pos = try self.rt_surface.getCursorPos();
                    let pos = match unsafe { &*self.rt_surface }.get_cursor_pos() {
                        Ok(v) => v,
                        Err(err) => {
                            // (the deferred mutex unlock — the error exit)
                            unsafe { (*self.renderer_state.mutex).unlock() };
                            return Err(err.into());
                        }
                    };
                    // self.mouseReport(switch (x.direction()) {
                    //     .up_right => .six,
                    //     .down_left => .seven,
                    // }, .press, self.mouse.mods, pos);
                    self.mouseReport(
                        Some(match x.direction() {
                            ScrollAmountDirection::UpRight => input_mouse::Button::Six,
                            ScrollAmountDirection::DownLeft => input_mouse::Button::Seven,
                        }),
                        input_mouse::Action::Press,
                        self.mouse.mods,
                        pos,
                    );
                }

                // If mouse reporting is on, we do not want to scroll the
                // viewport.
                // return;
                // (the deferred mutex unlock — the mouse-report return)
                unsafe { (*self.renderer_state.mutex).unlock() };
                return Ok(());
            }

            // if (y.delta != 0) {
            if y.delta != 0 {
                // Modify our viewport, this requires a lock since it affects
                // rendering. We have to switch signs here because our delta
                // is negative down but our viewport is positive down.
                // self.io.terminal.scrollViewport(.{ .delta = y.delta * -1 });
                self.io
                    .terminal
                    .scroll_viewport(terminal::terminal::ScrollViewport::Delta(-y.delta));
            }

            // (the deferred mutex unlock — the block end)
            unsafe { (*self.renderer_state.mutex).unlock() };
        }

        // try self.queueRender();
        self.queueRender()?;
        Ok(())
    }

    /// This is called when the content scale of the surface changes. The surface
    /// can then update any DPI-sensitive state.
    // pub fn contentScaleCallback(self: *Surface, content_scale: apprt.ContentScale) !void {
    pub fn contentScaleCallback(
        &mut self,
        content_scale: apprt::ContentScale,
    ) -> Result<(), Error> {
        // Crash metadata in case we crash in here
        // crash.sentry.thread_state = self.crashThreadState();
        crash::sentry::setThreadState(self.crashThreadState());
        // defer crash.sentry.thread_state = null;
        // ([ADV 6.1] — the Drop guard clears at every exit below.)
        let _thread_state = ThreadStateGuard;

        // Calculate the new DPI
        // const x_dpi = content_scale.x * font.face.default_dpi;
        let x_dpi = content_scale.x * f32::from(face::DEFAULT_DPI);
        // const y_dpi = content_scale.y * font.face.default_dpi;
        let y_dpi = content_scale.y * f32::from(face::DEFAULT_DPI);

        // Update our font size which is dependent on the DPI
        // const size = size: {
        let size = 'size: {
            // var size = self.font_size;
            let mut size = self.font_size;
            // size.xdpi = @intFromFloat(x_dpi);
            size.xdpi = x_dpi as u16;
            // size.ydpi = @intFromFloat(y_dpi);
            size.ydpi = y_dpi as u16;
            // break :size size;
            break 'size size;
        };

        // If our DPI didn't actually change, save a lot of work by doing nothing.
        // if (size.xdpi == self.font_size.xdpi and size.ydpi == self.font_size.ydpi) {
        //     return;
        // }
        if size.xdpi == self.font_size.xdpi && size.ydpi == self.font_size.ydpi {
            return Ok(());
        }

        // try self.setFontSize(size);
        self.setFontSize(size)?;

        // Update our padding which is dependent on DPI. We only do this for
        // unbalanced padding since balanced padding is not dependent on DPI.
        // if (self.config.window_padding_balance == .false) {
        if self.config.window_padding_balance == configpkg::value_types::WindowPaddingBalance::False
        {
            // self.size.padding = self.config.scaledPadding(x_dpi, y_dpi);
            self.size.padding = self.config.scaledPadding(x_dpi, y_dpi);
        }

        // Force a resize event because the change in padding will affect
        // pixel-level changes to the renderer and viewport.
        // try self.resize(self.size.screen);
        self.resize(self.size.screen)?;
        Ok(())
    }

    /// Returns true if mouse reporting is enabled both in the config and
    /// the terminal state.
    //
    // PORT NOTE (STUB — chunk C12 fills): `fn isMouseReporting(self: *const
    // Surface) bool` — Surface.zig:3640 (scrollCallback's :3525/:3568
    // callsites land with C11; `*const` receiver -> `&self`).
    fn isMouseReporting(&self) -> bool {
        unimplemented!("Surface.isMouseReporting lands with core chunk C12 (Surface.zig:3640)")
    }

    // PORT NOTE (STUB — chunk C12 fills): `fn mouseReport(self: *Surface,
    // button: ?input.MouseButton, action: input.MouseAction, mods:
    // input.Mods, pos: apprt.CursorPos) void` — Surface.zig:3646
    // (scrollCallback's :3571/:3579 callsites land with C11).
    fn mouseReport(
        &mut self,
        button: Option<input_mouse::Button>,
        action: input_mouse::Action,
        mods: Mods,
        pos: apprt::CursorPos,
    ) {
        let _ = (button, action, mods, pos);
        unimplemented!("Surface.mouseReport lands with core chunk C12 (Surface.zig:3646)")
    }

    /// Returns the link at the given cursor position, if any.
    ///
    /// Requires the renderer mutex is held.
    //
    // PORT NOTE (STUB — chunk C13 fills): `fn linkAtPos(self: *Surface, pos:
    // apprt.CursorPos) !?Link` — Surface.zig:4279 (mouseRefreshLinks' :1606
    // callsite lands with C8; the file-scope `Link` return type is ported
    // below).
    fn linkAtPos(&mut self, pos: apprt::CursorPos) -> Result<Option<Link>, Error> {
        let _ = pos;
        unimplemented!("Surface.linkAtPos lands with core chunk C13 (Surface.zig:4279)")
    }

    /// Returns true if the shift modifier is allowed to be captured by modifier
    /// events. It is up to the caller to still verify it is a situation in which
    /// shift capture makes sense (i.e. left button, mouse click, etc.)
    //
    // PORT NOTE (STUB — chunk C12 fills): `fn mouseShiftCapture(self: *const
    // Surface, lock: bool) bool` — Surface.zig:3712 (keyCallback's :2727
    // callsite lands with C10; `*const` receiver -> `&self`).
    fn mouseShiftCapture(&self, lock: bool) -> bool {
        let _ = lock;
        unimplemented!("Surface.mouseShiftCapture lands with core chunk C12 (Surface.zig:3712)")
    }

    /// This returns the mouse mods to consider for link highlighting or
    /// other purposes taking into account when shift is pressed for releasing
    /// the mouse from capture.
    ///
    /// The renderer state mutex must be held.
    //
    // PORT NOTE (STUB — chunk C13 fills): `fn mouseModsWithCapture(self:
    // *Surface, mods: input.Mods) input.Mods` — Surface.zig:4368
    // (modsChanged's :1544 callsite lands with C8).
    fn mouseModsWithCapture(&mut self, mods: Mods) -> Mods {
        let _ = mods;
        unimplemented!("Surface.mouseModsWithCapture lands with core chunk C13 (Surface.zig:4368)")
    }

    /// Return the URI for an OSC8 hyperlink at the given position or null
    /// if there is no hyperlink.
    //
    // PORT NOTE (STUB — chunk C13 fills): `fn osc8URI(self: *Surface, pin:
    // terminal.Pin) ?[]const u8` — Surface.zig:4439 (mouseRefreshLinks' :1622
    // callsite lands with C8; the return borrows the pin's page memory — raw
    // bytes `&[u8]`, never a lossy String).
    fn osc8URI(&self, pin: terminal::pagelist::Pin) -> Option<&[u8]> {
        let _ = pin;
        unimplemented!("Surface.osc8URI lands with core chunk C13 (Surface.zig:4439)")
    }

    // PORT NOTE (STUB — chunk C13 fills): `pub fn posToViewport(self: Surface,
    // xpos: f64, ypos: f64) terminal.point.Coordinate` — Surface.zig:4726
    // (note the reference's BY-VALUE receiver — read-only, `&self` here;
    // selectionScrollTick's :1185 callsite lands with C7).
    pub fn posToViewport(&self, xpos: f64, ypos: f64) -> terminal::point::Coordinate {
        let _ = (xpos, ypos);
        unimplemented!("Surface.posToViewport lands with core chunk C13 (Surface.zig:4726)")
    }

    // PORT NOTE (STUB — chunk C13 fills): `fn hideMouse(self: *Surface) void`
    // — Surface.zig:4741 (keyCallback's :2713 callsite lands with C10).
    fn hideMouse(&mut self) {
        unimplemented!("Surface.hideMouse lands with core chunk C13 (Surface.zig:4741)")
    }

    // PORT NOTE (STUB — chunk C13 fills): `fn showMouse(self: *Surface) void`
    // — Surface.zig:4753 (updateConfig's :1745 callsite lands with C8).
    fn showMouse(&mut self) {
        unimplemented!("Surface.showMouse lands with core chunk C13 (Surface.zig:4753)")
    }

    /// Perform a binding action. A binding is a keybinding. This function
    /// must be called from the GUI thread.
    ///
    /// This function returns true if the binding action was performed. This
    /// may return false if the binding action is not supported or if the
    /// binding action would do nothing (i.e. previous tab with no tabs).
    ///
    /// NOTE: At the time of writing this comment, only previous/next tab
    /// will ever return false. We can expand this in the future if it becomes
    /// useful. We did previous/next tab so we could implement #498.
    //
    // PORT NOTE (STUB — chunk C14 fills): `pub fn performBindingAction(self:
    // *Surface, action: input.Binding.Action) !bool` — Surface.zig:4775.
    pub fn performBindingAction(&mut self, action: binding::Action) -> Result<bool, Error> {
        let _ = action;
        unimplemented!("Surface.performBindingAction lands with core chunk C14 (Surface.zig:4775)")
    }

    /// This starts a clipboard request, with some basic validation. For example,
    /// an OSC 52 request is not actually requested if OSC 52 is disabled.
    ///
    /// Returns true if the request was started, false if it was not (e.g., clipboard
    /// doesn't contain text for paste requests). This allows performable keybinds
    /// to pass through when the action cannot be performed.
    //
    // PORT NOTE (STUB — chunk C15 fills): `fn startClipboardRequest(self:
    // *Surface, loc: apprt.Clipboard, req: apprt.ClipboardRequest) !bool` —
    // Surface.zig:5822 (handleMessage's clipboard_read callsite lands with
    // C7; the `!bool` is load-bearing — [ADV-FIX 1]).
    fn startClipboardRequest(
        &mut self,
        loc: apprt::Clipboard,
        req: apprt::ClipboardRequest,
    ) -> Result<bool, Error> {
        let _ = (loc, req);
        unimplemented!("Surface.startClipboardRequest lands with core chunk C15 (Surface.zig:5822)")
    }

    // PORT NOTE (STUB — chunk C15 fills): `fn completeClipboardPaste(self:
    // *Surface, data: []const u8, allow_unsafe: bool) !void` —
    // Surface.zig:5844 (textCallback's :3287 callsite lands with C11; raw
    // bytes `&[u8]`, never a lossy String).
    fn completeClipboardPaste(&mut self, data: &[u8], allow_unsafe: bool) -> Result<(), Error> {
        let _ = (data, allow_unsafe);
        unimplemented!(
            "Surface.completeClipboardPaste lands with core chunk C15 (Surface.zig:5844)"
        )
    }

    // PORT NOTE (STUB — chunk C15 fills): `fn showDesktopNotification(self:
    // *Surface, title: [:0]const u8, body: [:0]const u8) !void` —
    // Surface.zig:5968 (handleMessage's desktop_notification callsite lands
    // with C7; the `[:0]const u8` params are raw bytes — `&[u8]`).
    fn showDesktopNotification(&mut self, title: &[u8], body: &[u8]) -> Result<(), Error> {
        let _ = (title, body);
        unimplemented!(
            "Surface.showDesktopNotification lands with core chunk C15 (Surface.zig:5968)"
        )
    }

    // fn crashThreadState(self: *Surface) crash.sentry.ThreadState {
    //
    // PORT NOTE (ported with C8, ahead of its C10 row — Surface.zig:6014;
    // sizeCallback/preeditCallback are the first `crash.sentry.thread_state`
    // sites, [ADV 6.1]; the C10 key tier added the keyCallback site).
    fn crashThreadState(&self) -> crash::sentry::ThreadState<Surface<A>> {
        // return .{
        crash::sentry::ThreadState {
            // .type = .main,
            type_: crash::sentry::Type::Main,
            // .surface = self,
            surface: self,
        }
        // };
    }

    /// Tell the surface to present itself to the user. This may involve raising the
    /// window and switching tabs.
    // fn presentSurface(self: *Surface) !void {
    //
    // (Surface.zig:6023 — the C10 range's 6014-end tail; the inferred
    // `!void` is the performAction error.)
    fn presentSurface(&mut self) -> Result<(), Error> {
        // _ = try self.rt_app.performAction(
        //     .{ .surface = self },
        //     .present_terminal,
        //     {},
        // );
        let _ = unsafe { &*self.rt_app }.perform_action(
            apprt::Target::Surface(&*self),
            apprt::Action::PresentTerminal,
        )?;
        Ok(())
    }

    /// Get information about the process(es) running within the surface. Returns
    /// `null` if there was an error getting the information or the information is
    /// not available on a particular platform.
    //
    // pub fn getProcessInfo(self: *Surface, comptime info: ProcessInfo) ?ProcessInfo.Type(info) {
    //     return self.io.getProcessInfo(info);
    // }
    //
    // PORT NOTE (comptime-return split — the rustty-os pty.rs /
    // `Termio::getProcessInfo` precedent): Rust has no comptime
    // type-from-value, so the `info` selector splits into the two concrete
    // accessors (`.foreground_pid` -> `u64`, `.tty_name` -> `&CStr`).
    pub fn getForegroundPid(&mut self) -> Option<u64> {
        self.io.getForegroundPid()
    }

    /// See [`Surface::getForegroundPid`] (the `.tty_name` half of the
    /// reference `getProcessInfo`).
    pub fn getTtyName(&mut self) -> Option<&std::ffi::CStr> {
        self.io.getTtyName()
    }
}

// PORT NOTE: the two file-scope param enums of `endKeySequence`
// (Surface.zig:3128-3129, ported ahead of their C10 row — updateConfig's
// `.drop`/`.free` callsite names them; the C10 key tier landed the
// `.flush`/`.retain` construction sites).
// const KeySequenceQueued = enum { flush, drop };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeySequenceQueued {
    Flush,
    Drop,
}
// const KeySequenceMemory = enum { retain, free };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeySequenceMemory {
    Retain,
    Free,
}

// The amount to scroll. This structure is always normalized so that
// negative is down, left and positive is up, right. Note that INTERNALLY,
// vertical scroll on our terminal uses positive for down (right is not
// supported by our screen since scrollback is only vertical).
//
// PORT NOTE: the reference declares this file-scope struct BETWEEN
// refreshCallback and scrollCallback (Surface.zig:3405); Rust impl blocks
// cannot hold type items, so it lives in this trailing file-scope cluster
// (the KeySequenceQueued precedent).
// const ScrollAmount = struct {
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ScrollAmount {
    // delta: isize = 0,
    delta: isize,
}

/// PORT: the anonymous `enum { down_left, up_right }` return type of
/// `ScrollAmount.direction`, hoisted to a named enum (the TextViewport
/// hoist precedent).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScrollAmountDirection {
    // down_left,
    DownLeft,
    // up_right,
    UpRight,
}

impl ScrollAmount {
    // pub fn direction(self: ScrollAmount) enum { down_left, up_right } {
    pub fn direction(self) -> ScrollAmountDirection {
        // return if (self.delta < 0) .down_left else .up_right;
        if self.delta < 0 {
            ScrollAmountDirection::DownLeft
        } else {
            ScrollAmountDirection::UpRight
        }
    }

    // pub fn magnitude(self: ScrollAmount) usize {
    pub fn magnitude(self) -> usize {
        // return @abs(self.delta);
        self.delta.unsigned_abs()
    }
}

// fn closingAction(action: input.Binding.Action) bool {
//
// PORT NOTE (ported ahead of its C14 row — Surface.zig:5633;
// maybeHandleBinding's :3013 closing-check names it; the by-value `action`
// copy is a borrow, the posToViewport receiver precedent).
fn closingAction(action: &binding::Action) -> bool {
    // return switch (action) {
    match action {
        // .close_surface,
        // .close_window,
        // .close_tab,
        // => true,
        //
        // (Zig tag-compare — `.close_tab`'s payload is ignored.)
        binding::Action::CloseSurface
        | binding::Action::CloseWindow
        | binding::Action::CloseTab(_) => true,

        // else => false,
        _ => false,
    }
    // };
}

/// Represents text read from the terminal and some metadata about it
/// that is often useful to apprts.
// pub const Text = struct {
pub struct Text {
    /// The text that was read from the terminal.
    // text: [:0]const u8,
    //
    // PORT NOTE: raw bytes (`Vec<u8>`, never a lossy `String`); the `[:0]`
    // NUL-termination invariant is carried with the length excluding the
    // sentinel (the vt `selectionString` return convention).
    pub text: Vec<u8>,

    /// The viewport information about this text, if it is visible in
    /// the viewport.
    // viewport: ?Viewport = null,
    pub viewport: Option<TextViewport>,
}

/// PORT: the nested `Text.Viewport` (Surface.zig:1875), hoisted to a
/// top-level name (the `DerivedConfigLink` precedent).
// pub const Viewport = struct {
pub struct TextViewport {
    /// The top-left corner of the selection in pixels within the viewport.
    // tl_px_x: f64,
    // tl_px_y: f64,
    pub tl_px_x: f64,
    pub tl_px_y: f64,

    /// The linear offset of the start of the selection and the length.
    /// This is "linear" in the sense that it is the offset in the
    /// flattened viewport as a single array of text.
    ///
    /// Note: these values are currently wrong if there is a partially
    /// visible selection in the viewport (i.e. the top-left or
    /// bottom-right of the selection is outside the viewport). But the
    /// apprt usecase we have right now doesn't require these to be
    /// correct so... let's fix this later. The wrong values will always
    /// be within the text bounds so we aren't risking an overflow.
    // offset_start: u32,
    // offset_len: u32,
    pub offset_start: u32,
    pub offset_len: u32,
}

impl Text {
    // pub fn deinit(self: *Text, alloc: Allocator) void {
    //     alloc.free(self.text);
    // }
    //
    // PORT NOTE: the owned bytes free on drop; the explicit free maps to
    // clearing the field (the DerivedConfig deinit precedent — the alloc
    // param collapses with the Rust ownership).
    pub fn deinit(&mut self) {
        self.text = Vec::new();
    }
}

/// PORT: the file-scope `Link` returned by `linkAtPos` (Surface.zig:4270,
/// ported ahead of its C13 row — mouseRefreshLinks names it). NOT the
/// `DerivedConfig.Link` sibling ([`DerivedConfigLink`]).
// const Link = struct {
struct Link {
    // action: input.Link.Action,
    action: link::Action,
    // selection: terminal.Selection,
    selection: terminal::selection::Selection,
}

/// The already-landed A5 seam (CORE_BLUEPRINT §1): `Target.cval` reads
/// `v.rt_surface` off the core `*CoreSurface` — rustty-core's `Surface`
/// hands the opaque pointer out through the interface trait.
impl<A: apprt::App + 'static> apprt::CoreSurfaceHandle for Surface<A> {
    fn rt_surface_ptr(&self) -> *mut core::ffi::c_void {
        self.rt_surface.cast()
    }
}

/// The crash seam realized (CORE_BLUEPRINT §1; landed with C8's
/// `crashThreadState`, ahead of its C10 row): `beforeSend`'s surface
/// metadata reads — `surface.size.screen`, `surface.size.grid()`,
/// `surface.size.cell` — as plain `(u32, u32)` tuples (crash must not dep
/// rustty-renderer for the size types; the rustty-crash module doc).
impl<A: apprt::App + 'static> crash::sentry::ThreadStateSurface for Surface<A> {
    /// `surface.size.screen.{width, height}` (sentry.zig:217/221).
    fn screen_size(&self) -> (u32, u32) {
        (self.size.screen.width, self.size.screen.height)
    }

    /// `surface.size.grid().{columns, rows}` (sentry.zig:214).
    fn grid_size(&self) -> (u32, u32) {
        let g = self.size.grid();
        (u32::from(g.columns), u32::from(g.rows))
    }

    /// `surface.size.cell.{width, height}` (sentry.zig:233/237).
    fn cell_size(&self) -> (u32, u32) {
        (self.size.cell.width, self.size.cell.height)
    }
}

// PORT NOTE: `apprt.surface.Mailbox` (src/apprt/surface.zig:135) — relocated
// HERE per CORE_BLUEPRINT §1 (it names the core `Surface`, which apprt-api
// must not). Re-exported as [`Mailbox`] (Surface.zig:15 `pub const Mailbox =
// apprt.surface.Mailbox`); implements the apprt-api `AppMailbox` trait below
// (termio's M seam).

/// A surface mailbox.
// pub const Mailbox = struct {
//     surface: *Surface,
//     app: App.Mailbox,
//     ...
// };
pub struct SurfaceMailbox<A: apprt::App + 'static> {
    pub surface: *mut Surface<A>,
    pub app: crate::app::Mailbox<A>,
}

impl<A: apprt::App + 'static> AppMailbox for SurfaceMailbox<A> {
    type Surface = *mut Surface<A>;

    /// Send a message to the surface.
    // pub fn push(
    //     self: Mailbox,
    //     msg: Message,
    //     timeout: App.Mailbox.Queue.Timeout,
    // ) App.Mailbox.Queue.Size {
    //
    // PORT NOTE: `&self` for the reference's by-value `self: Mailbox`
    // receiver (the type is `Copy`); `Message<'static>`/concrete
    // `Timeout`/`Size` per the CORE_BLUEPRINT §3 trait resolution.
    fn push(&self, msg: Message<'static>, timeout: Timeout) -> QueueSize {
        // Surface message sending is actually implemented on the app
        // thread, so we have to rewrap the message with our surface
        // pointer and send it to the app thread.
        // return self.app.push(.{
        //     .surface_message = .{
        //         .surface = self.surface,
        //         .message = msg,
        //     },
        // }, timeout);
        self.app.push(
            crate::app::Message::SurfaceMessage(apprt::SurfaceMessageEnvelope {
                surface: self.surface,
                message: msg,
            }),
            timeout,
        )
    }
}

// PORT NOTE ([ADV-FIX 6a]): like `app::Mailbox<A>`, the reference copies
// `apprt.surface.Mailbox` BY VALUE (`.surface_mailbox = .{ .surface = self,
// .app = app_mailbox }`, Surface.zig:555; by-value `self: Mailbox` receiver
// in the reference `push`). MANUAL `Clone`/`Copy` with no bound on `A` (the
// fields are a raw pointer + a `Copy` mailbox handle).
impl<A: apprt::App + 'static> Clone for SurfaceMailbox<A> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<A: apprt::App + 'static> Copy for SurfaceMailbox<A> {}

// ===========================================================================
// PORT helpers — std analogues used above (std.fs.path / std.fs.access /
// std.base64 / the Zig `Options` value-copy). These add nothing the reference
// doesn't already do (R1): they are the minimal Rust glue for the std calls
// the reference makes inline (the rustty-config theme.rs / path.rs and
// rustty-os file.rs precedents).
// ===========================================================================

/// `std.fs.path.isAbsolute` for the POSIX targets the parity harness runs
/// (the rustty-config theme.rs/path.rs helper).
fn is_absolute(path: &[u8]) -> bool {
    !path.is_empty() && path[0] == b'/'
}

/// PORTS: `std.fs.path.resolve(alloc, &.{ base, path })` for the simple
/// two-component POSIX case used by `resolvePathForOpening`.
//
// PORT NOTE: `resolve` joins the components and normalizes `.`/`..` against
// the (absolute) base, yielding an absolute path WITHOUT touching the
// filesystem (unlike `realpath`/`canonicalize`) — the rustty-config path.rs
// helper, verbatim.
fn resolve(components: &[&[u8]]) -> Vec<u8> {
    let mut stack: Vec<Vec<u8>> = Vec::new();
    let mut is_abs = false;
    for comp in components {
        if comp.is_empty() {
            continue;
        }
        if comp[0] == b'/' {
            // An absolute component resets the stack.
            is_abs = true;
            stack.clear();
        }
        for seg in comp.split(|&b| b == b'/') {
            match seg {
                b"" | b"." => {}
                b".." => {
                    stack.pop();
                }
                _ => stack.push(seg.to_vec()),
            }
        }
    }

    let mut out: Vec<u8> = Vec::new();
    if is_abs {
        out.push(b'/');
    }
    for (i, seg) in stack.iter().enumerate() {
        if i > 0 {
            out.push(b'/');
        }
        out.extend_from_slice(seg);
    }
    if out.is_empty() {
        out.push(b'.');
    }
    out
}

/// Convert raw path bytes to an `OsStr` (POSIX bytes are `OsStr` directly;
/// R3/fidelity: byte-slices stay bytes, never a lossy round-trip).
#[cfg(unix)]
fn os_str_from_bytes(bytes: &[u8]) -> &std::ffi::OsStr {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::OsStr::from_bytes(bytes)
}

#[cfg(not(unix))]
fn os_str_from_bytes(bytes: &[u8]) -> &std::ffi::OsStr {
    // PORT NOTE: on non-unix the bytes are interpreted as UTF-8 (lossy
    // fallback); the path-opening flow is primarily exercised on POSIX.
    std::ffi::OsStr::new(std::str::from_utf8(bytes).unwrap_or(""))
}

/// PORT NOTE (Zig value-copy): `terminal.formatter.Options` is a Zig value
/// type freely copied at each `.init(..., opts)` / `var copy = opts` site
/// (`copySelectionToClipboards`); Rust `Options` owns its `codepoint_map`
/// `Vec`, so this helper materializes each reference copy — unlike the vt
/// formatter.rs `copy_options` (extras-only path), the map is KEPT (cloned;
/// Zig's header copy borrows the same entries).
fn copy_options(opts: &terminal::formatter::Options) -> terminal::formatter::Options {
    terminal::formatter::Options {
        emit: opts.emit,
        unwrap: opts.unwrap,
        trim: opts.trim,
        codepoint_map: opts.codepoint_map.clone(),
        background: opts.background,
        foreground: opts.foreground,
        palette: opts.palette,
    }
}

/// PORTS: the Zig `std.base64` STANDARD codec's `Decoder` surface used by
/// `clipboardWrite` (`calcSizeForSlice` + `decode`) — semantics reproduced
/// bit-for-bit from lib/std/base64.zig (Zig 0.15.2), including the error
/// split the reference branches on (`InvalidPadding` vs `InvalidCharacter`).
/// The Zig decoder's 16-/4-char SIMD-ish fast loops are accelerative only
/// (pad bytes never reach them — the final 4-char group always falls through
/// to the scalar tail); the scalar tail below IS the semantic core, ported
/// verbatim (the rustty-os file.rs `b64_encoder` precedent).
mod zig_base64 {
    // pub const Error = error{ InvalidCharacter, InvalidPadding, NoSpaceLeft };
    //
    // (`NoSpaceLeft` is encoder-only — unused by the decoder surface here.)
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Error {
        InvalidCharacter,
        InvalidPadding,
    }

    // pub const standard_alphabet_chars = "ABC...xyz0123456789+/".*;
    const ALPHABET_CHARS: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    // .pad_char = '=',
    const PAD_CHAR: u8 = b'=';
    // const invalid_char: u8 = 0xff;
    const INVALID_CHAR: u8 = 0xff;

    /// `char_to_index`: e.g. 'A' => 0; `invalid_char` for any value not in
    /// the 64 alphabet chars (the `Base64Decoder.init` table build).
    const CHAR_TO_INDEX: [u8; 256] = build_char_to_index();

    const fn build_char_to_index() -> [u8; 256] {
        let mut table = [INVALID_CHAR; 256];
        let mut i = 0;
        while i < 64 {
            table[ALPHABET_CHARS[i] as usize] = i as u8;
            i += 1;
        }
        table
    }

    /// Return the maximum possible decoded size for a given input length - The actual length may be less if the input includes padding.
    /// `InvalidPadding` is returned if the input length is not valid.
    // pub fn calcSizeUpperBound(decoder: *const Base64Decoder, source_len: usize) Error!usize {
    //
    // PARITY: clippy suggests `.is_multiple_of(4)` for the `% 4 != 0` test,
    // but rewriting would diverge from the reference expression; kept as-is
    // for fidelity (the rustty-os `manual_div_ceil` precedent).
    #[allow(clippy::manual_is_multiple_of)]
    pub const fn calc_size_upper_bound(source_len: usize) -> Result<usize, Error> {
        // var result = source_len / 4 * 3;
        let result = source_len / 4 * 3;
        // const leftover = source_len % 4;
        let leftover = source_len % 4;
        // if (decoder.pad_char != null) {
        //     if (leftover % 4 != 0) return error.InvalidPadding;
        // } else { ... }
        //
        // (the standard codec HAS a pad char — the no-pad arm is dead for
        // this codec and elided.)
        if leftover % 4 != 0 {
            return Err(Error::InvalidPadding);
        }
        // return result;
        Ok(result)
    }

    /// Return the exact decoded size for a slice.
    /// `InvalidPadding` is returned if the input length is not valid.
    // pub fn calcSizeForSlice(decoder: *const Base64Decoder, source: []const u8) Error!usize {
    pub fn calc_size_for_slice(source: &[u8]) -> Result<usize, Error> {
        // const source_len = source.len;
        let source_len = source.len();
        // var result = try decoder.calcSizeUpperBound(source_len);
        let mut result = calc_size_upper_bound(source_len)?;
        // if (decoder.pad_char) |pad_char| {
        //     if (source_len >= 1 and source[source_len - 1] == pad_char) result -= 1;
        //     if (source_len >= 2 and source[source_len - 2] == pad_char) result -= 1;
        // }
        if source_len >= 1 && source[source_len - 1] == PAD_CHAR {
            result -= 1;
        }
        if source_len >= 2 && source[source_len - 2] == PAD_CHAR {
            result -= 1;
        }
        // return result;
        Ok(result)
    }

    /// dest.len must be what you get from ::calcSize.
    /// Invalid characters result in `error.InvalidCharacter`.
    /// Invalid padding results in `error.InvalidPadding`.
    // pub fn decode(decoder: *const Base64Decoder, dest: []u8, source: []const u8) Error!void {
    //
    // PARITY: `% 4 != 0` kept verbatim over clippy's `.is_multiple_of(4)`
    // (see `calc_size_upper_bound`).
    #[allow(clippy::manual_is_multiple_of)]
    pub fn decode(dest: &mut [u8], source: &[u8]) -> Result<(), Error> {
        // if (decoder.pad_char != null and source.len % 4 != 0) return error.InvalidPadding;
        if source.len() % 4 != 0 {
            return Err(Error::InvalidPadding);
        }
        // var dest_idx: usize = 0;
        let mut dest_idx: usize = 0;
        // var acc: u12 = 0;
        //
        // (`u12` emulated as the low 12 bits of a `u16` — the shift below
        // masks exactly as the Zig `u12` truncating `<<` does.)
        let mut acc: u16 = 0;
        // var acc_len: u4 = 0;
        let mut acc_len: u32 = 0;
        // var leftover_idx: ?usize = null;
        let mut leftover_idx: Option<usize> = None;
        // (the two `fast_char_to_index` fast loops are elided — accelerative
        // only, see the module doc; this is the reference's scalar tail over
        // `remaining`, applied to the whole source.)
        // for (remaining, fast_src_idx..) |c, src_idx| {
        for (src_idx, &c) in source.iter().enumerate() {
            // const d = decoder.char_to_index[c];
            let d = CHAR_TO_INDEX[c as usize];
            // if (d == invalid_char) {
            if d == INVALID_CHAR {
                // if (decoder.pad_char == null or c != decoder.pad_char.?) return error.InvalidCharacter;
                if c != PAD_CHAR {
                    return Err(Error::InvalidCharacter);
                }
                // leftover_idx = src_idx;
                leftover_idx = Some(src_idx);
                // break;
                break;
            }
            // acc = (acc << 6) + d;
            acc = ((acc << 6) & 0xfff) + u16::from(d);
            // acc_len += 6;
            acc_len += 6;
            // if (acc_len >= 8) {
            if acc_len >= 8 {
                // acc_len -= 8;
                acc_len -= 8;
                // dest[dest_idx] = @as(u8, @truncate(acc >> acc_len));
                dest[dest_idx] = (acc >> acc_len) as u8;
                // dest_idx += 1;
                dest_idx += 1;
            }
        }
        // if (acc_len > 4 or (acc & (@as(u12, 1) << acc_len) - 1) != 0) {
        //     return error.InvalidPadding;
        // }
        if acc_len > 4 || (acc & ((1u16 << acc_len) - 1)) != 0 {
            return Err(Error::InvalidPadding);
        }
        // if (leftover_idx == null) return;
        let leftover_start = match leftover_idx {
            Some(v) => v,
            None => return Ok(()),
        };
        // const leftover = source[leftover_idx.?..];
        let leftover = &source[leftover_start..];
        // if (decoder.pad_char) |pad_char| {
        //
        // (the standard codec's pad char is '=' — the arm is always taken.)
        {
            // const padding_len = acc_len / 2;
            let padding_len = (acc_len / 2) as usize;
            // var padding_chars: usize = 0;
            let mut padding_chars: usize = 0;
            // for (leftover) |c| {
            for &c in leftover {
                // if (c != pad_char) {
                if c != PAD_CHAR {
                    // return if (c == Base64Decoder.invalid_char) error.InvalidCharacter else error.InvalidPadding;
                    return Err(if c == INVALID_CHAR {
                        Error::InvalidCharacter
                    } else {
                        Error::InvalidPadding
                    });
                }
                // padding_chars += 1;
                padding_chars += 1;
            }
            // if (padding_chars != padding_len) return error.InvalidPadding;
            if padding_chars != padding_len {
                return Err(Error::InvalidPadding);
            }
        }
        Ok(())
    }
}
