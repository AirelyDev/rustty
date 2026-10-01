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
//! PORT NOTE: helpers at the bottom; chunk C10 landed the key tier, Surface.zig:
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
//! (C15) — their first callsites are this tier; chunk C12 landed the mouse
//! tier, Surface.zig:3640-4278 — `isMouseReporting`/`mouseReport`/
//! `mouseShiftCapture`/`mouseCaptured`/`mouseButtonCallback`/
//! `maybePromptClick` LIVE (one more [ADV 6.1] guard site; the
//! defer-unlock blocks realize the explicit unlock at every exit — the
//! C7..C11 convention — including the right-click paste arms' inverted
//! unlock→`defer` relock replayed in LIFO order) with STUBS added for the
//! C13 helpers its bodies call (`cursorPosCallback`/`linkAtPin`/
//! `processLinks`); chunk C13 landed the link/cursor-pos/color-scheme tier,
//! Surface.zig:4279-4774 — `linkAtPos`/`linkAtPin`/`mouseModsWithCapture`/
//! `processLinks`/`openUrl`/`osc8URI`/`mousePressureCallback`/
//! `cursorPosCallback`/`colorSchemeCallback`/`posToViewport`/
//! `scrollToBottom`/`hideMouse`/`showMouse` LIVE (the C12-declared stubs
//! filled IN PLACE at their reference positions; `linkAtPin` is the first
//! core user of vt's oniguruma-gated `StringMap.searchIterator` — the
//! Zig value-copied map/regex handles move-through/alias, see its PORT
//! NOTEs; `openUrl`'s fallback retyped rustty-os `open` to the reference's
//! byte URL; the last three [ADV 6.1] guard sites land here);
//! CORE_BLUEPRINT §6 row C14 filled `performBindingAction`/`closingAction`;
//! chunk C15 landed the tail, Surface.zig:5651-6036 — `writeScreenFile`
//! (TempDir/ScreenFormatter/openUrl file I/O — the `TmpDirGuard` Drop-guard
//! reproduces errdefer + the null-selection deinit; the `.mode = 0o600` bit is
//! re-applied via fchmod)/`completeClipboardRequest`/`startClipboardRequest`
//! (the `!bool` [ADV-FIX 1] pass-through)/`completeClipboardPaste` (input.paste
//! encode split, the guard-less mutex block)/`completeClipboardReadOSC52` (the
//! std.base64 STANDARD Encoder added to `zig_base64`)/`showDesktopNotification`
//! (Wyhash de-dup -> `DefaultHasher`) LIVE, with `error.UnsafePaste`/
//! `error.UnauthorizedPaste` grown onto the [`Error`] union (load-bearing —
//! apprt catches them for confirmation); `crashThreadState`/`presentSurface`/
//! `getProcessInfo` had landed with C8/C10 — the Surface.zig file body is now
//! COMPLETE).

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
// tier added its six sites; the C12 mouse tier added its mouseButtonCallback
// site; the C13 tier added the last three (mousePressureCallback/
// cursorPosCallback/colorSchemeCallback) — all 13 reference sites live.
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
// PORT NOTE (D1 vt slice): `input.mouse_encode` lives in rustty-vt (the
// mouse encoder is VT-protocol logic) — `mouseReport` (C12).
use rustty_vt::input::mouse_encode;
// PORT NOTE (D1 vt slice): `input.paste` lives in rustty-vt (paste encoding is
// VT-protocol logic — bracketed-paste framing) — `completeClipboardPaste` (C15).
use rustty_vt::input::paste;
// const kitty_clipboard = terminal.kitty.clipboard (the Surface.zig usage).
use rustty_vt::kitty::clipboard as kitty_clipboard;
// const App = @import("App.zig");
use crate::app::App;
use crate::thread_shared::ThreadSharedBox;
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
// reload_config — the [ADV-FIX 4] dispatch-arm list) and the C13 tier's
// (mouse_visibility / open_url), and the `Surface` seam trait for the typed
// `rt_surface.*` method edges (aliased per the C0 DECISION note — core's own
// struct is also named `Surface`).
use rustty_apprt_api::action::{
    CellSize, ColorChange, ColorKind, CommandFinished, ConfigChange, DesktopNotification,
    InitialSize, KeySequence, KeyTable, MouseOverLink, MouseVisibility, OpenUrl, OpenUrlKind,
    PromptTitle, Pwd, ReloadConfig, ResizeWindow, SearchSelected, SearchTotal, SecureInput,
    SetTitle, SizeLimit, StartSearch,
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
use rustty_vt::terminal::PrintStringError;
use rustty_vt::terminal::Terminal;

// const log = std.log.scoped(.surface);
// PORT NOTE: the reference `log.debug/info/warn/err` sites are preserved
// verbatim as comments at each site, with the LIVE emission added directly
// beneath each one (task 14d-1, `.port/parity-closure/DECISION-logging.md` §6
// call shape 2 — `rustty-core` is below `rustty-vt-c` in R4's layering, so
// restored sites go through the `log` facade and reach whichever backend the
// running binary installed). `LOG_SCOPE` is the `@tagName` of the reference's
// `std.log.scoped(.surface)`.
//
// Six sites in this file stay COMMENT-ONLY because they are commented out in
// the ORACLE too (`Surface.zig:2540, 2666, 3460, 3548, 3798, 4539` — the
// preedit/key/scroll/mouse/cursor-pos traces); each carries a note saying so.
const LOG_SCOPE: &str = "surface";

// The renderer implementation to use.
// const Renderer = rendererpkg.Renderer;
//
// PORT NOTE (CORE_BLUEPRINT §5): the comptime-chosen GPU renderer is the
// core-local dependency placeholder.
use crate::renderer_seam::Renderer;

// The Surface.zig topic files (P7-W13 E). The types stay here; the method
// clusters live in `surface/*.rs` as further `impl Surface<A>` blocks.
mod binding_action;
mod clipboard;
mod derived_config;
mod helpers;
mod key_input;
mod lifecycle;
mod message;
mod mouse_input;
mod size;
#[cfg(test)]
mod tests;
mod text_font;

pub use self::derived_config::*;
pub use self::helpers::*;
pub(crate) use self::size::*;

/// Minimum window size in cells. This is used to prevent the window from
/// being resized to a size that is too small to be useful. These defaults
/// are chosen to match the default size of Mac's Terminal.app, but is
/// otherwise somewhat arbitrary.
// pub const min_window_width_cells: u32 = 10;
// pub const min_window_height_cells: u32 = 4;
pub const MIN_WINDOW_WIDTH_CELLS: u32 = 10;
pub const MIN_WINDOW_HEIGHT_CELLS: u32 = 4;

/// Minimum size in cells that the running program can resize the window
/// to (CSI 8 t). This is larger than the minimum window size so that a
/// program can't shrink the window to hide its output.
// const min_resize_width_cells: u32 = 40;
// const min_resize_height_cells: u32 = 10;
const MIN_RESIZE_WIDTH_CELLS: u32 = 40;
const MIN_RESIZE_HEIGHT_CELLS: u32 = 10;

/// The maximum number of key tables that can be active at any
/// given time. `activate_key_table` calls after this are ignored.
// const max_active_key_tables = 8;
//
// PORT NOTE: the C14b `performBindingAction` `activate_key_table` arm
// (Surface.zig:5495) is the only consumer — landed with C14b.
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
    //
    // PORT NOTE (N441, P7-W7 T5b): the reference embeds the renderer by value;
    // the port keeps it in its own allocation (`RendererBox`), shared with the
    // render thread through one raw root, so a main-thread `&mut Surface` /
    // `&Surface` (every apprt callback) never covers the bytes the render
    // thread writes under the draw mutex. Proof: `.port/phase-2/mirihar-n441`.
    pub renderer: crate::renderer_seam::RendererBox,

    /// The render state
    // renderer_state: rendererpkg.State,
    //
    // PORT NOTE (N444, P7-W7 T5c): the reference embeds this block by value;
    // the port keeps it in its own allocation (`ThreadSharedBox`), shared with
    // the thread that writes it through one raw root, so a main-thread
    // `&mut Surface` / `&Surface` (every apprt callback) never covers its bytes.
    // Reached by place through `as_ptr()`. Proof: `.port/phase-2/mirihar-n444`.
    pub renderer_state: ThreadSharedBox<rendererpkg::state::State>,

    /// The renderer thread manager
    // renderer_thread: rendererpkg.Thread,
    //
    // PORT NOTE (N444, P7-W7 T5c): the reference embeds this block by value;
    // the port keeps it in its own allocation (`ThreadSharedBox`), shared with
    // the thread that writes it through one raw root, so a main-thread
    // `&mut Surface` / `&Surface` (every apprt callback) never covers its bytes.
    // Reached by place through `as_ptr()`. Proof: `.port/phase-2/mirihar-n444`.
    pub renderer_thread: ThreadSharedBox<crate::renderer_seam::Thread<A>>,

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
    //
    // PORT NOTE (N444, P7-W7 T5c): the reference embeds `io` and `io_thread` by value;
    // the port keeps it in its own allocation (`ThreadSharedBox`), shared with
    // the thread that writes it through one raw root, so a main-thread
    // `&mut Surface` / `&Surface` (every apprt callback) never covers its bytes.
    // Reached by place through `as_ptr()`. Proof: `.port/phase-2/mirihar-n444`.
    pub io: ThreadSharedBox<termio::Termio<SurfaceMailbox<A>>>,
    pub io_thread: ThreadSharedBox<termio::Thread>,
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
    /// This is managed completely by Rustty core but an apprt action
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

    /// Whether this surface may be visible. Unknown visibility is considered
    /// visible so reporting remains conservative.
    // visible: bool = true,
    pub visible: bool,

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
/// wasn't handled in any way by Rustty.
// pub const InputEffect = enum { ... };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputEffect {
    /// The input was not handled in any way by Rustty and should be
    /// forwarded to other subsystems (i.e. the OS) for further
    /// processing.
    // ignored,
    Ignored,

    /// The input was handled and consumed by Rustty.
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
    //
    // PORT NOTE (N444, P7-W7 T5c): in its own allocation (`ThreadSharedBox`),
    // not inline in the surface: the search thread writes it for its whole
    // life, and every main-thread `&mut Surface` / `&Surface` would otherwise
    // cover it (`.port/phase-2/mirihar-n444` `n444_inline_search_state`).
    // `deinit` drops it normally after its explicit `deinit`, as the inline
    // field was dropped with the `Search`.
    pub state: ThreadSharedBox<terminal::search::thread::Thread>,
    // thread: std.Thread,
    //
    // PORT NOTE: retyped `JoinHandle` -> `Option<JoinHandle>` for the
    // construct-then-spawn two-phase store (the reference `.thread = undefined`
    // then `s.thread = try .spawn(...)`): a bare `JoinHandle` cannot be
    // `undefined`, so `None` is the placeholder written when the `Search` is
    // stored, and `Some(handle)` is assigned once the spawn returns
    // (SEARCH_BINDING_ARM_DESIGN §2.3). `deinit` `take()`s it to join.
    pub thread: Option<std::thread::JoinHandle<()>>,

    // -- rustty-only decoupled producer handles (SEARCH_BINDING_ARM_DESIGN §3) --
    //
    // PORT NOTE: these carry NO reference field — they exist because the search
    // thread runs `thread_main(&mut self)` holding a `&mut Thread` (a raw
    // `*mut Thread`) for its ENTIRE life, so ANY post-spawn main-thread access
    // that reborrows `&mut Search`/`&mut Thread` over `state` (e.g.
    // `self.search.as_mut().unwrap().state.mailbox…`) is Stacked-Borrows UB
    // (proven `mirihar-chunk3b` `search_naive`). Every post-spawn producer
    // reaches the running thread ONLY through these decoupled handles, which
    // address memory OUTSIDE the `Thread` struct's SB coverage:
    //   * `mailbox` — the `Box<Mailbox>` pointee, DERIVED FROM THE POST-SPAWN
    //     `state_ptr` LINEAGE (the same provenance the child derefs
    //     `self.mailbox` through; a pre-move `addr_of!` alias would be popped by
    //     the child's `Box` noalias — proven `search_naive`/`search_shipped`).
    //   * `wakeup`/`stop` — `xev::Notifier`s holding the loop's `Arc<Shared>`
    //     (heap-refcounted, outside the struct).
    mailbox: *const terminal::search::thread::Mailbox,
    wakeup: terminal::search::thread::xev::Notifier,
    stop: terminal::search::thread::xev::Notifier,

    // PORT-ONLY (N444 fix round 1, P7-W7 T5c): the search callback's userdata,
    // a boxed `SearchCallbackCtx<A>` captured at spawn, and its typed free.
    // `Search` is not generic over the apprt, so the pointer is erased; `deinit`
    // frees it once, after the join (the callback's `Quit` runs before that).
    callback_ctx: *mut core::ffi::c_void,
    callback_ctx_free: unsafe fn(*mut core::ffi::c_void),
}

/// PORT-ONLY (N444 fix round 1, P7-W7 T5c): what `searchCallback_` needs,
/// captured on the main thread when the search thread is spawned.
///
/// The reference passes the surface itself as `event_userdata` and reads
/// `self.renderer_thread.mailbox` / `.wakeup` and `self.surfaceMailbox()`
/// (`rt_app`, `app`) on the search thread. Those are the surface's own bytes:
/// every main-thread `&mut Surface` retag races with the reads under Stacked
/// Borrows (`.port/phase-2/mirihar-n444` `n444_search_cb_reads_surface`). The
/// values never change for the surface's life, so the callback reads this copy
/// instead and never dereferences the surface (`n444_search_cb_ctx`, CLEAN).
struct SearchCallbackCtx<A: apprt::App + 'static> {
    /// `self.renderer_thread.mailbox` (the renderer's queue, its own allocation).
    renderer_mailbox: *mut rendererpkg::thread::Mailbox,
    /// `&self.renderer_thread.wakeup`, inside the out-of-line render-thread block.
    renderer_wakeup: *const rustty_xev::Async,
    /// `self.surfaceMailbox()`: the surface pointer only as the envelope's
    /// identity, and the app's own mailbox.
    surface_mailbox: Mailbox<A>,
}

impl<A: apprt::App + 'static> SearchCallbackCtx<A> {
    /// Free a context made by `Box::into_raw` (the `Search.callback_ctx_free`
    /// entry).
    ///
    /// # Safety
    /// `p` came from `Box::into_raw(Box<SearchCallbackCtx<A>>)`, the search
    /// thread is joined or never started, and this is called once.
    unsafe fn free(p: *mut core::ffi::c_void) {
        unsafe { drop(Box::from_raw(p.cast::<SearchCallbackCtx<A>>())) };
    }
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
        //
        // PORT NOTE (SEARCH_BINDING_ARM_DESIGN §3.3): route the stop-notify
        // through the DECOUPLED `self.stop` notifier, NOT `self.state.stop` —
        // the child still holds its `&mut Thread` here (stop-then-join), so a
        // `self.state.stop` reborrow of `state` would be SB UB. `self.stop`
        // touches only the loop's `Arc<Shared>` (outside `state`).
        if let Err(err) = self.stop.notify() {
            // log.err("error notifying search thread to stop, may stall err={}", .{err});
            log::error!(
                target: LOG_SCOPE,
                "error notifying search thread to stop, may stall err={err:?}"
            );
        }

        // Wait for the OS thread to quit
        // self.thread.join();
        //
        // (`take()` the `Option<JoinHandle>` placeholder; the child is now dead,
        // so the subsequent `state` deinit has exclusive access.)
        self.thread
            .take()
            .expect("search thread spawned")
            .join()
            .unwrap();

        // Now it is safe to deinit the state
        //
        // (N444: through the box's raw root; then the state is dropped and its
        // allocation freed, where the inline field was dropped with `self`.)
        unsafe {
            (*self.state.as_ptr()).deinit();
            self.state.drop_and_dealloc();
            // (N444 fix round 1: the callback's context, once, after the join.)
            (self.callback_ctx_free)(self.callback_ctx);
        }
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

/// PORT NOTE: the reference's anonymous `table_stack` element struct (see the
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
    /// `terminal.search.Thread.init`'s xev-handle init (`performBindingAction`'s
    /// `.search` arm — the vt search-loop shim's `xev::Error`, a DIFFERENT type
    /// from the renderer-thread `rustty_xev::Error` above; the reference `try
    /// .init(...)` folds it into `performBindingAction`'s inferred `!bool`).
    SearchXev(terminal::search::thread::xev::Error),
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
    /// `std.fmt.bufPrint`'s fixed-buffer write (`maybePromptClick` — C12) —
    /// the reference `error.NoSpaceLeft` surfaced as io's `WriteZero` (the
    /// encodeKey fixed-Cursor mapping).
    Io(std::io::Error),
    /// The oniguruma regex search (`linkAtPin`'s `it.next()` — C13).
    Oni(oni::Error),
    /// `internal_os.open`'s spawn failures (`openUrl` — C13).
    OsOpen(internal_os::open::OpenError),
    /// `error.UnsafePaste` — `completeClipboardPaste` rejects a potentially
    /// unsafe paste until confirmation (Surface.zig:5886, [ADV-FIX 1] family).
    /// Load-bearing: apprt catches it to show a confirmation prompt rather than
    /// log it (embedded.zig:714), so it must stay a distinguishable variant.
    UnsafePaste,
    /// `error.UnauthorizedPaste` — `completeClipboardReadOSC52` rejects an
    /// unconfirmed clipboard read when `clipboard-read = ask` (Surface.zig:5935);
    /// apprt catches it alongside `UnsafePaste` (embedded.zig:715).
    UnauthorizedPaste,
}

impl From<apprt::ApprtError> for Error {
    fn from(err: apprt::ApprtError) -> Error {
        Error::Apprt(err)
    }
}

// PORT NOTE (chunk C14a): `performBindingAction`'s app-scoped block calls
// `App.newWindow`/`App.performAction`, whose `app::Error` must fold back into
// the surface `Error` set (the reference's single inferred error union). The
// `app::Error` variants map 1:1 — `Apprt` passes through, `Surface` unwraps to
// the surface error it already carries (`app::Error` holds `From<surface::Error>`
// for the opposite edge, so this bridge is non-circular).
impl From<crate::app::Error> for Error {
    fn from(err: crate::app::Error) -> Error {
        match err {
            crate::app::Error::Apprt(e) => Error::Apprt(e),
            crate::app::Error::Surface(e) => e,
        }
    }
}

impl From<rustty_xev::NotifyError> for Error {
    fn from(err: rustty_xev::NotifyError) -> Error {
        Error::Xev(err)
    }
}

impl From<terminal::search::thread::xev::Error> for Error {
    fn from(err: terminal::search::thread::xev::Error) -> Error {
        Error::SearchXev(err)
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

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Error {
        Error::Io(err)
    }
}

impl From<oni::Error> for Error {
    fn from(err: oni::Error) -> Error {
        Error::Oni(err)
    }
}

impl From<internal_os::open::OpenError> for Error {
    fn from(err: internal_os::open::OpenError) -> Error {
        Error::OsOpen(err)
    }
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::Apprt(err) => err.fmt(f),
            Error::Xev(err) => write!(f, "core surface operation failed: {err:?}"),
            Error::SearchXev(err) => write!(f, "core surface operation failed: {err:?}"),
            Error::OutOfMemory(err) => write!(f, "core surface operation failed: {err:?}"),
            Error::FontGrid(err) => write!(f, "core surface operation failed: {err:?}"),
            Error::InitChangeConfig(err) => write!(f, "core surface operation failed: {err:?}"),
            Error::Utf8(err) => err.fmt(f),
            Error::Io(err) => err.fmt(f),
            Error::Oni(err) => write!(f, "core surface operation failed: {err:?}"),
            Error::OsOpen(err) => write!(f, "core surface operation failed: {err:?}"),
            Error::UnsafePaste => write!(f, "core surface operation failed: UnsafePaste"),
            Error::UnauthorizedPaste => {
                write!(f, "core surface operation failed: UnauthorizedPaste")
            }
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
    /// `init`/`finalizeSurfaceInit`, `draw`'s `drawFrame`) — Phase-3 R-d
    /// (CORE_BLUEPRINT §5).
    Renderer(crate::renderer_seam::RendererError),
    /// The renderer `DerivedConfig.init` — the config-link `oniRegex()` compile
    /// failures + arena OOM (`rendererpkg::generic::DerivedConfigError`). Now the
    /// REAL `rustty_renderer::DerivedConfig::init` (swapped in from the former
    /// seam placeholder), constructed at the `Surface::init` derivation.
    RendererDerivedConfig(rendererpkg::generic::DerivedConfigError),
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

impl From<rendererpkg::generic::DerivedConfigError> for InitError {
    fn from(err: rendererpkg::generic::DerivedConfigError) -> InitError {
        InitError::RendererDerivedConfig(err)
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
            InitError::RendererDerivedConfig(err) => write!(f, "surface init: {err:?}"),
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
/// when an inspector existed at keyCallback entry. The inspector is live
/// (WS6), so the raw pointer IS dereferenced whenever an inspector is open;
/// the deref happens strictly AFTER the body's last use of its local
/// reborrow (the guard drops at return — the rust-mut-args raw-ptr
/// discipline). That includes a key that closed the surface: the drop reads
/// the surface after the close callback returned, as the reference's defer
/// does, so a host must not free the surface synchronously inside that
/// callback (N465).
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
            let insp = match unsafe { (*self.surface).inspector } {
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
            // (`recordKeyEvent` takes the owned `Event` by value — the ring records
            // it with the max_capacity=50 grow-then-evict policy. The `log.warn` site
            // is preserved as a comment per surface.rs's no-logging-shim convention;
            // `err` is the ring-full/OutOfMemory signal. The `queueRender` deref
            // happens here in the guard's `Drop` — strictly AFTER the keyCallback
            // body's last `&mut self` use, per the raw-ptr discipline documented on
            // this guard.)
            let alloc = unsafe { (*self.surface).alloc };
            match unsafe { (*insp).recordKeyEvent(alloc, ev) } {
                Ok(()) => {
                    // self.queueRender() catch {};
                    //
                    // (`queueRender` is `renderer_thread.wakeup.notify()`; inlined
                    // here because the method carries the `ApprtSurface` associated-
                    // type bound the `impl Drop for InspEvGuard` cannot restate — a
                    // `Drop` impl must match the type's declared bounds exactly. The
                    // field-level notify needs no such bound. `catch {}` -> `let _`.)
                    let _ = unsafe { &(*(*self.surface).renderer_thread.as_ptr()).wakeup }.notify();
                }
                // } else |err| {
                //     log.warn("error adding key event to inspector err={}", .{err});
                // }
                Err(err) => {
                    log::warn!(
                        target: LOG_SCOPE,
                        "error adding key event to inspector err={err:?}"
                    );
                }
            }
        }
    }
}
