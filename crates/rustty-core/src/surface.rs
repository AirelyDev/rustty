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
//! `Inspector`); CORE_BLUEPRINT §6 rows C7..C15 fill the remaining function
//! bodies IN PLACE — the C4-declared method stubs below are owned by C7/C8/
//! C14).

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
// PORT NOTE: `rustty_crash` — first reached by the C10 key tier
// (setThreadState/clearThreadState un-defer); preserved as this comment.
// const unicode = @import("unicode/main.zig");
// PORT NOTE: `rustty_unicode` — first reached by C9 (selectionString word
// chars); preserved as this comment.
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
use rustty_vt::input::key::u21;
use rustty_vt::input::key::KeyEvent;
use rustty_vt::input::key_mods::Mods;
use rustty_vt::input::mouse as input_mouse;
// const App = @import("App.zig");
use crate::app::App;
// const internal_os = @import("os/main.zig");
use rustty_os as internal_os;
// const inspectorpkg = @import("inspector/main.zig");
use rustty_inspector as inspectorpkg;
// const SurfaceMouse = @import("surface_mouse.zig");
// PORT NOTE: `crate::surface_mouse::SurfaceMouse` — first reached by the C10
// key tier (modsChanged/keyCallback); preserved as this comment.
// const ProcessInfo = @import("pty.zig").ProcessInfo;
// PORT NOTE: `ProcessInfo` — first reached by C15 getProcessInfo; preserved
// as this comment.

// AppMailbox trait + the concrete queue Timeout/Size for the
// `SurfaceMailbox` impl below (CORE_BLUEPRINT §3).
use rustty_apprt_api::surface::AppMailbox;
use rustty_datastruct::blocking_queue::{Size as QueueSize, Timeout};

// The apprt action payloads init sends (`performAction` — cell_size /
// size_limit / set_title), and the `Surface` seam trait for the typed
// `rt_surface.*` method edges (aliased per the C0 DECISION note — core's own
// struct is also named `Surface`).
use rustty_apprt_api::action::{CellSize, SetTitle, SizeLimit};
use rustty_apprt_api::Surface as ApprtSurface;

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
    // PORT NOTE: the C8 (:1594) / C13+ (:4650) callsites land later; the
    // allow is removed when the first one does.
    #[allow(dead_code)]
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
/// [`Keyboard::table_stack`] PORT NOTE).
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

/// PORT NOTE (GROWTH POINT, Rust-only glue): the Surface methods App reaches
/// today (`handleMessage`/`performBindingAction`) return `!void`/`!bool` —
/// error unions whose error SETS are *inferred* from the (C6/C7/C14-owned)
/// bodies. Until those chunks land, this opaque marker types the seam
/// (precedent: `rustty_termio::Error`, `rustty_apprt_api::ApprtError`); it
/// gains the real error sets at the growth point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Error;

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "core surface operation failed")
    }
}

impl std::error::Error for Error {}

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

impl<A: apprt::App + 'static> Surface<A> {
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
    // PORT NOTE: reference `inline fn` -> `#[inline]`; the C7+ callsites
    // (`handleMessage` etc.) land later — the allow is removed when the first
    // one does.
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
    // cross-thread-queue precedent). The C7+ callsites (`handleMessage` etc.)
    // land later — the allow is removed when the first one does (the deferred
    // activate/deactivateInspector bodies below name it meanwhile).
    #[allow(dead_code)]
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
    // PORT NOTE (STUB — chunk C7 fills): `pub fn handleMessage(self:
    // *Surface, msg: Message) !void` — Surface.zig:960. `Message<'_>` (not
    // `'static`): the one borrowed variant, `ChangeConfig(&Config)`, is
    // DIRECT-dispatched by `App.updateConfig`, never queued (apprt-api
    // `AppMailbox` note).
    pub fn handleMessage(&mut self, msg: Message<'_>) -> Result<(), Error> {
        let _ = msg;
        unimplemented!("Surface.handleMessage lands with core chunk C7 (Surface.zig:960)")
    }

    /// Recalculate the initial size of the window based on the
    /// configuration and invoke the apprt `initial_size` action if
    /// necessary.
    //
    // PORT NOTE (STUB — chunk C8 fills): `fn recomputeInitialSize(self:
    // *Surface) InitialSizeError!void` — Surface.zig:1826 (init's caught
    // callsite lands with C6; updateConfig's with C8; the `InitialSizeError`
    // set lands with the body — the opaque [`Error`] marker meanwhile).
    fn recomputeInitialSize(&mut self) -> Result<(), Error> {
        unimplemented!("Surface.recomputeInitialSize lands with core chunk C8 (Surface.zig:1826)")
    }

    // PORT NOTE (STUB — chunk C8 fills): `fn resize(self: *Surface, size:
    // rendererpkg.ScreenSize) !void` — Surface.zig:2485 (init's :708 callsite
    // lands with C6; the sizeCallback/setFontSize callsites with C8/C11; the
    // inferred error set lands with the body — the opaque [`Error`] marker
    // meanwhile).
    fn resize(&mut self, size: rendererpkg::size::ScreenSize) -> Result<(), Error> {
        let _ = size;
        unimplemented!("Surface.resize lands with core chunk C8 (Surface.zig:2485)")
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
}

/// The already-landed A5 seam (CORE_BLUEPRINT §1): `Target.cval` reads
/// `v.rt_surface` off the core `*CoreSurface` — rustty-core's `Surface`
/// hands the opaque pointer out through the interface trait.
impl<A: apprt::App + 'static> apprt::CoreSurfaceHandle for Surface<A> {
    fn rt_surface_ptr(&self) -> *mut core::ffi::c_void {
        self.rt_surface.cast()
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
