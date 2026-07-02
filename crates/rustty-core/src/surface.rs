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
//! `SurfaceMailbox<A>` push/`AppMailbox` impl; CORE_BLUEPRINT §6 rows C6..C15
//! fill the function bodies IN PLACE — the C4-declared method stubs below
//! are owned by C6/C7/C14).

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
// PORT NOTE: `rustty_global` — first reached by the C6+ bodies (init's
// resources-dir/crash sites); preserved as this comment until then.
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
// lives in rustty-font (pure, BELOW vt); `SharedGridSet`/`face.DesiredSize`
// live in rustty-font-shaper (the shaper tier ABOVE vt+font).
use rustty_font::metrics::Metrics;
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
// PORT NOTE: `rustty_os` — first reached by the C6+ bodies (open/homedir
// paths); preserved as this comment.
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
    // PORT NOTE: the C6 (:536) / C8 (:2514) / C11 (:3630) callsites land
    // later; the allow is removed when the first one does.
    #[allow(dead_code)]
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

impl<A: apprt::App + 'static> Surface<A> {
    /// Close this surface. This will trigger the runtime to start the
    /// close process, which should ultimately deinitialize this surface.
    //
    // PORT NOTE (STUB — chunk C6 fills): `pub fn close(self: *Surface) void`
    // — Surface.zig:842 (`self.rt_surface.close(self.needsConfirmQuit());`).
    pub fn close(&mut self) {
        unimplemented!("Surface.close lands with core chunk C6 (Surface.zig:842)")
    }

    /// True if the surface requires confirmation to quit. This should be called
    /// by apprt to determine if the surface should confirm before quitting.
    //
    // PORT NOTE (STUB — chunk C6 fills): `pub fn needsConfirmQuit(self:
    // *Surface) bool` — Surface.zig:938. `&self`: the reference receiver is
    // non-const but the body only queries state ([ADV-FIX 2.4] discipline).
    pub fn needsConfirmQuit(&self) -> bool {
        unimplemented!("Surface.needsConfirmQuit lands with core chunk C6 (Surface.zig:938)")
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
