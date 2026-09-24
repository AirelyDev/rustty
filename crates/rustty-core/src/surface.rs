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
    PromptTitle, Pwd, ReloadConfig, SearchSelected, SearchTotal, SecureInput, SetTitle, SizeLimit,
    StartSearch,
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

/// Minimum window size in cells. This is used to prevent the window from
/// being resized to a size that is too small to be useful. These defaults
/// are chosen to match the default size of Mac's Terminal.app, but is
/// otherwise somewhat arbitrary.
// pub const min_window_width_cells: u32 = 10;
// pub const min_window_height_cells: u32 = 4;
pub const MIN_WINDOW_WIDTH_CELLS: u32 = 10;
pub const MIN_WINDOW_HEIGHT_CELLS: u32 = 4;

/// `recomputeInitialSize`'s pixel computation (`Surface.zig:1866-1886`), checked.
///
/// PORT NOTE: reference defect (R13, UPSTREAM-DEFECTS.md UD59). The reference
/// multiplies `@max(window_height, 4) * cell.height` (and the width) in `u32`,
/// converts `@ceil(px / scale)` with `@intFromFloat`, and adds the padding, all
/// unchecked. `window-height = 1000000000` overflows the first product: ReleaseSafe
/// panics, ReleaseFast sends a wrapped size. Returns `None` when any step
/// overflows or the scaled size is not a `u32`; the caller then takes its
/// existing skip path.
pub(crate) fn initial_size_px(
    window_width: u32,
    window_height: u32,
    size: rendererpkg::size::Size,
    scale: apprt::ContentScale,
) -> Option<(u32, u32)> {
    let height = window_height
        .max(MIN_WINDOW_HEIGHT_CELLS)
        .checked_mul(size.cell.height)?;
    let width = window_width
        .max(MIN_WINDOW_WIDTH_CELLS)
        .checked_mul(size.cell.width)?;
    let width_f32: f32 = width as f32;
    let height_f32: f32 = height as f32;
    let final_width = u32_from_ceil(width_f32 / scale.x)?
        .checked_add(size.padding.left)?
        .checked_add(size.padding.right)?;
    let final_height = u32_from_ceil(height_f32 / scale.y)?
        .checked_add(size.padding.top)?
        .checked_add(size.padding.bottom)?;
    Some((final_width, final_height))
}

/// `@as(u32, @intFromFloat(@ceil(v)))`, which traps on an infinite value or
/// one outside `u32` in safety-checked builds; `None` there instead (R13,
/// UD59).
///
/// PORT NOTE (R13, UD59; decided by task R13-C): a NaN does NOT trap there.
/// With a NaN content scale the reference's `recomputeInitialSize` computes a
/// size of 0 and sends 0 plus the padding (measured, Zig 0.16.0: `8` for an
/// 800 px width and 4 px padding a side, Debug, ReleaseSafe and ReleaseFast,
/// arm64 and x86_64 under Rosetta 2). The port returns `None` and so takes the
/// function's own skip path. Why not match the 0: the language makes a
/// conversion with no `u32` result illegal behaviour (`@intFromFloat` docs),
/// the safety check merely lets NaN through, and the 0 is what LLVM's
/// `fptoui` of a NaN (a poison value) happens to lower to on these two
/// targets; it is not a size anything asked for. A content scale that cannot
/// size the window is the same case as a size that does not fit, and UD59's
/// handling for that is the skip. Reaching it takes a NaN content scale from
/// the embedder (`rustty_surface_set_content_scale`, or `scale_factor` in the
/// surface config).
fn u32_from_ceil(v: f32) -> Option<u32> {
    let c = v.ceil();
    // NaN is outside every range, so `contains` rejects it too.
    if !(0.0..4_294_967_296.0).contains(&c) {
        return None;
    }
    Some(c as u32)
}

/// The `size_limit` minimums (`Surface.zig:705-706`), checked; `None` when a
/// product overflows `u32` (R13, UPSTREAM-DEFECTS.md UD59).
pub(crate) fn size_limit_min(cell_width: u32, cell_height: u32) -> Option<(u32, u32)> {
    Some((
        cell_width.checked_mul(MIN_WINDOW_WIDTH_CELLS)?,
        cell_height.checked_mul(MIN_WINDOW_HEIGHT_CELLS)?,
    ))
}

// rustty-only (R13, UPSTREAM-DEFECTS.md UD59): the initial-size and
// size-limit arithmetic with values that come through the config loader.
// `Surface::init` cannot run without an apprt, so the tests drive the two
// helpers `recomputeInitialSize` and `init` call, with the parsed values
// (`DerivedConfig` copies `window-width` / `window-height` unchanged).
#[cfg(test)]
mod r13_window_size_overflow_tests {
    use super::*;

    fn load(text: &[u8]) -> configpkg::Config {
        let mut cfg = configpkg::Config::default_config();
        let mut iter = rustty_args::args::LineIterator::init(text);
        cfg.load_iter(&mut iter).unwrap();
        cfg.finalize().unwrap();
        assert!(cfg._diagnostics.empty(), "unexpected config diagnostics");
        cfg
    }

    fn size(cell_width: u32, cell_height: u32, padding: u32) -> rendererpkg::size::Size {
        rendererpkg::size::Size {
            screen: rendererpkg::size::ScreenSize {
                width: 800,
                height: 600,
            },
            cell: rendererpkg::size::CellSize {
                width: cell_width,
                height: cell_height,
            },
            padding: rendererpkg::size::Padding {
                top: padding,
                bottom: padding,
                right: padding,
                left: padding,
            },
        }
    }

    const SCALE: apprt::ContentScale = apprt::ContentScale { x: 1.0, y: 1.0 };

    #[test]
    fn initial_size_px_overflow_takes_the_skip_path() {
        // Positive control: 80x24 cells of 10x20 px, 2 px padding.
        let cfg = load(b"window-width = 80\nwindow-height = 24\n");
        assert_eq!(
            Some((80 * 10 + 4, 24 * 20 + 4)),
            initial_size_px(cfg.window_width, cfg.window_height, size(10, 20, 2), SCALE)
        );

        // The brief's trigger: 1000000000 * 20 overflows the `u32` product.
        let cfg = load(b"window-width = 80\nwindow-height = 1000000000\n");
        assert_eq!(
            1_000_000_000, cfg.window_height,
            "the value reaches the surface"
        );
        assert_eq!(
            None,
            initial_size_px(cfg.window_width, cfg.window_height, size(10, 20, 2), SCALE)
        );
        let cfg = load(b"window-width = 1000000000\nwindow-height = 24\n");
        assert_eq!(
            None,
            initial_size_px(cfg.window_width, cfg.window_height, size(10, 20, 2), SCALE)
        );

        // The product fits (214748364 * 20 = 4294967280) but `@floatFromInt` to
        // f32 rounds it to 2^32, which `@intFromFloat` into `u32` cannot hold.
        // No padding here, so a saturating conversion (4294967295) is not
        // caught by the padding addition instead.
        let cfg = load(b"window-width = 80\nwindow-height = 214748364\n");
        assert_eq!(
            None,
            initial_size_px(cfg.window_width, cfg.window_height, size(10, 20, 0), SCALE)
        );

        // The scaled size fits, and adding the padding overflows. (2147483 * 2000
        // = 4294966000; f32 steps by 256 there, so it rounds to 4294966016.)
        let cfg = load(b"window-width = 80\nwindow-height = 2147483\n");
        assert_eq!(
            Some((80 * 10 + 4, 4_294_966_016 + 4)),
            initial_size_px(
                cfg.window_width,
                cfg.window_height,
                size(10, 2000, 2),
                SCALE
            )
        );
        assert_eq!(
            None,
            initial_size_px(
                cfg.window_width,
                cfg.window_height,
                size(10, 2000, 1_000),
                SCALE
            )
        );
    }

    /// UD71 (R13, task R13-C): `Surface.zig:437-455` `scaledPadding` and the
    /// padding sums in `renderer/size.zig`. `window-padding-x = 4294967295` at
    /// 144 dpi is out of `u32` (the reference traps on the cast); `window-padding-y
    /// = 2147483648` at 72 dpi converts, but its top + bottom overflows `u32` in
    /// `subPadding` (the reference traps there). The port saturates both, and the
    /// grid keeps its one-cell minimum.
    #[test]
    fn an_oversized_window_padding_saturates_and_leaves_a_one_cell_grid() {
        // Positive control: an ordinary padding is scaled by the dpi.
        let cfg = load(b"window-padding-x = 2\nwindow-padding-y = 3\n");
        let mut dc = DerivedConfig::init(Allocator, &cfg).unwrap();
        let pad = dc.scaledPadding(144.0, 144.0);
        assert_eq!((4, 4, 6, 6), (pad.left, pad.right, pad.top, pad.bottom));
        dc.deinit();

        let cfg = load(b"window-padding-x = 4294967295\nwindow-padding-y = 2147483648\n");
        assert_eq!(
            u32::MAX,
            cfg.window_padding_x.top_left,
            "the value reaches the config"
        );
        let mut dc = DerivedConfig::init(Allocator, &cfg).unwrap();
        let pad = dc.scaledPadding(144.0, 72.0);
        dc.deinit();
        assert_eq!(
            (u32::MAX, u32::MAX),
            (pad.left, pad.right),
            "saturated, not trapped"
        );
        assert_eq!(
            (1 << 31, 1 << 31),
            (pad.top, pad.bottom),
            "in range; the SUM is not"
        );

        let mut sz = size(10, 20, 0);
        sz.padding = pad;
        assert_eq!(
            rendererpkg::size::ScreenSize {
                width: 0,
                height: 0
            },
            sz.terminal(),
            "the padding leaves no room"
        );
        let grid = sz.grid();
        assert_eq!((1, 1), (grid.columns, grid.rows), "the grid keeps one cell");
        let blank = sz.screen.blank_padding(pad, grid, sz.cell);
        assert_eq!((0, 0), (blank.right, blank.bottom));

        // Surface.init's balancing path with the same explicit padding.
        sz.balance_padding(pad, configpkg::value_types::WindowPaddingBalance::True);
        assert_eq!((1, 1), (sz.grid().columns, sz.grid().rows));
    }

    /// UD95 (R13, task R13-D): `click-repeat-interval` above 4294 ms. The
    /// reference computes `config.@"click-repeat-interval" * 1_000_000` in `u32`
    /// (`Surface.zig:395`) and traps (ReleaseFast: 5000 -> 705032704 ns,
    /// measured). Driven through the config loader into a real `DerivedConfig`.
    #[test]
    fn a_click_repeat_interval_above_u32_nanoseconds_is_widened_not_trapped() {
        // Positive control: an ordinary interval.
        let cfg = load(b"click-repeat-interval = 500\n");
        let mut dc = DerivedConfig::init(Allocator, &cfg).unwrap();
        assert_eq!(500_000_000, dc.mouse_interval);
        dc.deinit();

        for ms in [5000u32, u32::MAX] {
            let cfg = load(format!("click-repeat-interval = {ms}\n").as_bytes());
            assert_eq!(
                ms, cfg.click_repeat_interval,
                "the value reaches the config"
            );
            let mut dc = DerivedConfig::init(Allocator, &cfg).unwrap();
            assert_eq!(
                u64::from(ms) * 1_000_000,
                dc.mouse_interval,
                "{ms} ms is {ms}000000 ns, not a wrapped u32"
            );
            dc.deinit();
        }
    }

    /// `u32_from_ceil` on a NaN (task R13-C). The reference's `@as(u32,
    /// @intFromFloat(@ceil(width_f32 / scale.x)))` with a NaN content scale does
    /// not trap: it yields 0, so it would send an initial size of 0 plus the
    /// padding (measured, Zig 0.16.0, `final_width = 8` for 800 px and 4 px
    /// padding a side, Debug/ReleaseSafe/ReleaseFast, arm64 and x86_64 under
    /// Rosetta 2). The port skips instead; see `u32_from_ceil`.
    #[test]
    fn initial_size_px_with_a_nan_content_scale_takes_the_skip_path() {
        let cfg = load(b"window-width = 80\nwindow-height = 24\n");
        // Positive control: the same config with a real scale.
        assert_eq!(
            Some((80 * 10 + 8, 24 * 20 + 8)),
            initial_size_px(cfg.window_width, cfg.window_height, size(10, 20, 4), SCALE)
        );
        for scale in [
            apprt::ContentScale {
                x: f32::NAN,
                y: 1.0,
            },
            apprt::ContentScale {
                x: 1.0,
                y: f32::NAN,
            },
        ] {
            assert_eq!(
                None,
                initial_size_px(cfg.window_width, cfg.window_height, size(10, 20, 4), scale),
                "a NaN content scale ({scale:?}) sends no initial size"
            );
        }
        assert_eq!(None, u32_from_ceil(f32::NAN));
        assert_eq!(Some(0), u32_from_ceil(-0.5), "ceil(-0.5) is -0, which is 0");
    }

    #[test]
    fn size_limit_min_overflow_skips_the_action() {
        // `adjust-cell-width` adds an absolute `i32` to the face's cell width.
        let cfg = load(b"adjust-cell-width = 500000000\n");
        let cell_width = cfg
            .adjust_cell_width
            .expect("adjust-cell-width parsed")
            .apply_u32(10);
        assert_eq!(
            500_000_010, cell_width,
            "the modifier reaches the cell size"
        );
        assert_eq!(None, size_limit_min(cell_width, 20));

        // Positive control, and the largest cell that fits.
        assert_eq!(Some((100, 80)), size_limit_min(10, 20));
        assert_eq!(Some((4_294_967_290, 80)), size_limit_min(u32::MAX / 10, 20));
        assert_eq!(None, size_limit_min(10, u32::MAX / 4 + 1));
    }
}

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
    pub state: terminal::search::thread::Thread,
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
    // link_osc8: bool,
    pub link_osc8: bool,
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
            //
            // PORT NOTE: reference defect (R13), UPSTREAM-DEFECTS.md UD95. The
            // reference multiplies in `u32` (the config field's type) before the
            // result widens to the `u64` field, so `click-repeat-interval` above
            // 4294 (ms) traps in ReleaseSafe and wraps in ReleaseFast (5000 ms
            // becomes 705032704 ns, measured). The port widens first, which is
            // the reference's evident intent (the field is `u64` nanoseconds);
            // any `u32` millisecond count fits.
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
            // .link_osc8 = config.@"link-osc8",
            link_osc8: config.link_osc8,
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
            // ordering constraint it documents is moot in Rustty.
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
    //
    // PORT NOTE: reference defect (R13), UD71. Each side is
    // `@intFromFloat(@floor(padding * dpi / 72))`, a safety-checked cast of a
    // config value (`window-padding-x`/`-y`, any `u32`) scaled by the
    // embedder's content scale. `window-padding-x = 4294967295` at 72 dpi, or
    // 3000000000 at 144 dpi (Retina), is out of `u32`: ReleaseSafe panics
    // "integer part of floating point value out of bounds" at startup,
    // ReleaseFast yields 4294967295 (measured, Zig 0.16.0). The port's `as u32`
    // saturates to that same 4294967295 — a padding "as large as it goes",
    // which the reference already treats as a padding wider than the window
    // (one-cell grid, the "Is your padding reasonable?" warning). A NaN dpi
    // gives 0 in both trees. What made the saturated value safe downstream is
    // the saturating padding sums in `rustty-render-types` (`sub_padding`,
    // `blank_padding`, `balance_padding`), which overflowed before, and the
    // saturating cursor-position sums in `imePoint` and the custom-shader
    // cursor uniforms (UD76, task R13-C2), which a saturated padding stored by
    // `contentScaleCallback` reached before the IO thread's resize.
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
pub fn font_shared_grid_set_derived_config(
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
        freetype_load_flags: derived_freetype_load_flags(&config.freetype_load_flags),
    }
}

/// The field-for-field projection of the global config's
/// `config.FreetypeLoadFlags` onto the shaper tier's stand-in for that same
/// reference type (`face::freetype::FreetypeLoadFlags`).
///
/// PORT NOTE (Rust-only glue, break #9 DI realization): in the reference these
/// are ONE type — `font.face.FreetypeLoadFlags` IS `config.FreetypeLoadFlags`
/// on every freetype-active backend, so `SharedGridSet.zig:528` can assign it
/// straight across. rustty's break #9 keeps `rustty-font-shaper` off
/// `rustty-config`, so the shaper declares a structurally identical stand-in
/// (`face/freetype.rs`) and this function is the DI bridge. It is the only
/// place the two spellings meet; every field must be threaded or the user's
/// `freetype-load-flags` setting is silently dropped.
//
// Compiled on every non-wasm target (the shaper's `freetype` module is likewise
// `cfg(not(wasm32))`), so it is unit-testable on macOS even though macOS's
// active backend is `coretext` and takes the `void` arm below.
#[cfg(not(target_arch = "wasm32"))]
pub fn freetype_load_flags_from_config(
    c: &configpkg::value_types::FreetypeLoadFlags,
) -> rustty_font_shaper::face::freetype::FreetypeLoadFlags {
    rustty_font_shaper::face::freetype::FreetypeLoadFlags {
        hinting: c.hinting,
        // PORT NOTE: Zig field `@"force-autohint"`.
        force_autohint: c.force_autohint,
        monochrome: c.monochrome,
        autohint: c.autohint,
        light: c.light,
    }
}

/// `if (font.face.FreetypeLoadFlags != void) config.@"freetype-load-flags" else {}`
/// — the reference's comptime backend selector, modeled with `cfg(target_os)`
/// exactly as `face/mod.rs` models `options.backend.hasFreetype()`.
//
// macOS default backend is `coretext` → `hasFreetype()` is false → `void`
// (Rust `()`), and the config value is legitimately dropped, matching the
// reference `else {}` arm.
#[cfg(target_os = "macos")]
fn derived_freetype_load_flags(
    _c: &configpkg::value_types::FreetypeLoadFlags,
) -> rustty_font_shaper::face::FreetypeLoadFlags {
}

// Linux default backend is `fontconfig_freetype` → `hasFreetype()` is true →
// the configured value flows through.
#[cfg(not(any(target_os = "macos", target_arch = "wasm32")))]
fn derived_freetype_load_flags(
    c: &configpkg::value_types::FreetypeLoadFlags,
) -> rustty_font_shaper::face::FreetypeLoadFlags {
    freetype_load_flags_from_config(c)
}

#[cfg(target_arch = "wasm32")]
fn derived_freetype_load_flags(
    _c: &configpkg::value_types::FreetypeLoadFlags,
) -> rustty_font_shaper::face::FreetypeLoadFlags {
    // The `web_canvas` backend's `hasFreetype()` is likewise false; the shaper's
    // wasm placeholder is the `void` arm.
    rustty_font_shaper::face::FREETYPE_LOAD_FLAGS_DEFAULT
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
                    let _ = unsafe { &(*self.surface).renderer_thread.wakeup }.notify();
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
        log::debug!(
            target: LOG_SCOPE,
            "xscale={} yscale={} xdpi={x_dpi} ydpi={y_dpi}",
            content_scale.x,
            content_scale.y
        );

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
        let renderer_wakeup = render_thread.wakeup;
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
                renderer_impl.deinit();
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
                    inspector_ptr: None,
                    preedit: None,
                    mouse: rendererpkg::state::Mouse::default(),
                    // D4 handoff (oracle d34b54e9b) - reference field defaults are `.init(0)`.
                    demand: Default::default(),
                    handoff_gen: Default::default(),
                    handoff_park: Default::default(),
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
                    &raw mut (*self_).io,
                    alloc,
                    termio::Options {
                        size,
                        full_config: config,
                        config: termio::DerivedConfig::init(config),
                        backend: termio::Backend::Exec(io_exec),
                        mailbox: io_mailbox,
                        renderer_state: &raw mut (*self_).renderer_state,
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
                        apprt::Target::Surface(&*self_),
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

        // W-ATTACH (macos-swift-app tier): route the host NSView into the
        // renderer's windowed layer-attach so frames present to the window (the
        // renderer was built offscreen via `init_offscreen`; without this its
        // `present` drops every frame into a detached `IOSurface` → a blank
        // window). MUST run here — on the main thread, AFTER finalizeSurfaceInit,
        // BEFORE the render thread spawn below — so `loop_enter` installs the
        // display callback against `Some(layer)`. `host_view` is null for
        // non-view-hosted backends, where `attachHostView` is a no-op.
        unsafe {
            let view = (*rt_surface).host_view();
            let scale = (*rt_surface).host_content_scale();
            Self::initTry(self_, true, (*self_).renderer.attachHostView(view, scale))
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
        // Test-only (task R13-H2): observes the thread itself, independently
        // of the logged join (see `init_fault::ThreadProbe`). Not compiled
        // into production builds.
        #[cfg(feature = "init-fault-inject")]
        let probe =
            crate::init_fault::ThreadProbe::start(crate::init_fault::ProbedThread::Renderer);
        let spawn_result = std::thread::Builder::new()
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
        let args = IoThreadArgs::<A> {
            thread: unsafe { &raw mut (*self_).io_thread },
            io: unsafe { &raw mut (*self_).io },
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
        let io_builder = std::thread::Builder::new().name("io".to_string());
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
            // Unlike `deinit`, this path does not re-enter the renderer
            // (`threadEnter`, Surface.zig:811) before `renderer.deinit()`. That
            // is harmless at the pin: OpenGL's `threadEnter` does nothing for
            // GTK or embedded (OpenGL.zig:197-216), and Metal defines none.
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
                let _ = (*self_).renderer_thread.stop.notify();
                #[cfg(feature = "init-fault-inject")]
                crate::init_fault::record(crate::init_fault::Event::RendererStopped);
                let _ = handle.join();
                #[cfg(feature = "init-fault-inject")]
                crate::init_fault::record(crate::init_fault::Event::RendererJoined);
            }
            // The io thread, if it was spawned (`io_thr` is stored straight
            // after its spawn succeeds), the same way `deinit` stops it.
            if let Some(handle) = (*self_).io_thr.take() {
                let _ = (*self_).io_thread.stop.notify();
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
                std::ptr::read(&raw const (*self_).io).deinit();
            }
            // errdefer io_thread.deinit(); (Surface.zig:579 — consuming
            // receiver, read back out.)
            #[cfg(feature = "init-fault-inject")]
            crate::init_fault::before_free("io_thread");
            std::ptr::read(&raw const (*self_).io_thread).deinit();
            // errdefer render_thread.deinit(); (Surface.zig:575)
            //
            // The renderer child was stopped and joined at the top of this
            // function (UD104), so read the field out by-value (no whole
            // `&mut renderer_thread` — E2's aliasing discipline) and deinit,
            // matching the io half above.
            {
                #[cfg(feature = "init-fault-inject")]
                crate::init_fault::before_free("renderer_thread");
                let mut rt = std::ptr::read(&raw const (*self_).renderer_thread);
                rt.deinit();
            }
            // errdefer alloc.destroy(mutex); (Surface.zig:564)
            #[cfg(feature = "init-fault-inject")]
            crate::init_fault::before_free("renderer_state.mutex");
            drop(Box::from_raw((*self_).renderer_state.mutex));
            // errdefer renderer_impl.deinit(); (Surface.zig:559)
            (*self_).renderer.deinit();
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
            if let Err(err) = self.renderer_thread.stop.notify() {
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
            if let Err(err) = self.io_thread.stop.notify() {
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
    pub fn close(&mut self) {
        // self.rt_surface.close(self.needsConfirmQuit());
        let process_alive = self.needsConfirmQuit();
        unsafe { &*self.rt_surface }.close(process_alive);
    }

    /// Returns a mailbox that can be used to send messages to this surface.
    // inline fn surfaceMailbox(self: *Surface) Mailbox {
    //
    // PORT NOTE: reference `inline fn` -> `#[inline]`. The `&mut self` form is
    // preserved verbatim (R1) but currently dead — its sole reached caller,
    // `searchCallback_`, now uses the raw `surface_mailbox_raw` variant (§3.3a,
    // to avoid a whole `&mut Surface` reborrow on the search thread). Future
    // main-thread callers of other ported arms use this `&mut self` form.
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
    unsafe fn surface_mailbox_raw(self_: *mut Surface<A>) -> Mailbox<A> {
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
    fn queueIo(&mut self, msg: termio::Message<'static>, mutex: termio::termio_impl::MutexState) {
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
        self.io.queueMessage(msg, mutex);
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
        unsafe { crate::renderer_seam::Renderer::drawFrame(&raw mut (*self_).renderer, true)? };
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
            unsafe { (*self.renderer_state.mutex).lock() };
            // assert(self.renderer_state.inspector == null);
            assert!(self.renderer_state.inspector.is_none());
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
            self.renderer_state.inspector = Some(info_ptr);
            // The SAME `ptr`, but type-erased (`rendererpkg::state::State::
            // inspector_ptr` PORT NOTE): termio's `recordPtyRead` (INV-8)
            // needs the WHOLE `Inspector`, unreachable through the narrow
            // `renderer_info` sub-object fat pointer above.
            assert!(self.renderer_state.inspector_ptr.is_none());
            self.renderer_state.inspector_ptr = Some(ptr as *mut ());
            unsafe { (*self.renderer_state.mutex).unlock() };
        }

        // Notify our components we have an inspector active
        // _ = self.renderer_thread.mailbox.push(.{ .inspector = true }, .{ .forever = {} });
        let _ = unsafe { &*self.renderer_thread.mailbox }
            .push(rendererpkg::Message::Inspector(true), Timeout::Forever);
        // self.queueIo(.{ .inspector = true }, .unlocked);
        self.queueIo(
            termio::Message::Inspector(true),
            termio::termio_impl::MutexState::Unlocked,
        );

        Ok(())
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
            unsafe { (*self.renderer_state.mutex).lock() };
            // assert(self.renderer_state.inspector != null);
            assert!(self.renderer_state.inspector.is_some());
            // self.renderer_state.inspector = null;
            self.renderer_state.inspector = None;
            // Clear the type-erased alias in lock-step (see activateInspector's
            // `inspector_ptr` PORT NOTE).
            assert!(self.renderer_state.inspector_ptr.is_some());
            self.renderer_state.inspector_ptr = None;
            unsafe { (*self.renderer_state.mutex).unlock() };
        }

        // Notify our components we have deactivated inspector
        // _ = self.renderer_thread.mailbox.push(.{ .inspector = false }, .{ .forever = {} });
        let _ = unsafe { &*self.renderer_thread.mailbox }
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
                    log::debug!(
                        target: LOG_SCOPE,
                        "ignoring title change request since static title is set via config"
                    );
                    return Ok(());
                }

                // The ptrCast just gets sliceTo to return the proper type.
                // We know that our title should end in 0.
                // const slice = std.mem.sliceTo(@as([*:0]const u8, @ptrCast(v)), 0);
                let slice = &v[..v.iter().position(|&b| b == 0).unwrap_or(v.len())];
                // log.debug("changing title \"{s}\"", .{slice});
                log::debug!(
                    target: LOG_SCOPE,
                    "changing title \"{}\"",
                    String::from_utf8_lossy(slice)
                );
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
                    log::info!(
                        target: LOG_SCOPE,
                        "report_title requested, but disabled via config"
                    );
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
                log::debug!(target: LOG_SCOPE, "changing mouse shape: {shape:?}");
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

            // .kitty_clipboard_read => |req| try self.kittyClipboardRead(req),
            //
            // (the Box leaks to the reference's raw `*KittyRead` here; the
            // handler owns destroying it — see `KittyRead::destroy`.)
            Message::KittyClipboardRead(req) => {
                self.kittyClipboardRead(Box::into_raw(req))?;
            }
            // .kitty_clipboard_write => |req| try self.kittyClipboardWrite(req),
            Message::KittyClipboardWrite(req) => {
                self.kittyClipboardWrite(Box::into_raw(req))?;
            }

            // .clipboard_read => |clipboard| {
            Message::ClipboardRead(clipboard) => {
                // if (self.config.clipboard_read == .deny) {
                if self.config.clipboard_read == configpkg::value_types::ClipboardAccess::Deny {
                    // log.info("application attempted to read clipboard, but 'clipboard-read' is set to deny", .{});
                    log::info!(
                        target: LOG_SCOPE,
                        "application attempted to read clipboard, but 'clipboard-read' is set to deny"
                    );
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
                // Rustty; runs at arm scope end, the datastruct precedent.)

                // var stack = std.heap.stackFallback(256, self.alloc);
                // const alloc = stack.get();
                // const str = try alloc.dupeZ(u8, w.slice());
                // defer alloc.free(str);
                //
                // PORT NOTE (`f1d225020`): the apprt-api `Pwd` payload carries
                // raw bytes (`&[u8]`); the reference's NUL-termination
                // (`dupeZ`) is part of the deferred C-ABI pinning (the
                // action.rs `PwdC` PORT NOTE). `stackFallback(256, ..)` serves
                // the `dupeZ` allocation (len + the NUL byte) from a 256-byte
                // stack buffer when it fits and falls back to the heap for
                // longer paths — the two arms below reproduce that split
                // exactly (the SURFACE_ID stack-`bufPrint` precedent); the
                // heap arm's owned dupe + scope-end drop realize the
                // alloc/free pair (Zig defer LIFO == Rust reverse-declaration
                // drop: `str` before `w`).
                let mut stack = [0u8; 256];
                let heap: Vec<u8>;
                let str_: &[u8] = if w.slice().len() + 1 <= stack.len() {
                    let len = w.slice().len();
                    stack[..len].copy_from_slice(w.slice());
                    &stack[..len]
                } else {
                    heap = w.slice().to_vec();
                    &heap
                };

                // _ = try self.rt_app.performAction(
                //     .{ .surface = self },
                //     .pwd,
                //     .{ .pwd = str },
                // );
                // (performAction continues to borrow the value only for the
                // duration of the call.)
                let _ = unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::Surface(&*self),
                    apprt::Action::Pwd(Pwd { pwd: str_ }),
                )?;
            }

            // .close => self.close(),
            Message::Close => self.close(),

            // .child_exited => |v| self.childExited(v),
            Message::ChildExited(v) => self.childExited(v),

            // .desktop_notification => |notification| {
            Message::DesktopNotification(notification) => {
                // if (!self.config.desktop_notifications) {
                if !self.config.desktop_notifications {
                    // log.info("application attempted to display a desktop notification, but 'desktop-notifications' is disabled", .{});
                    log::info!(
                        target: LOG_SCOPE,
                        "application attempted to display a desktop notification, but 'desktop-notifications' is disabled"
                    );
                    return Ok(());
                }

                // const title = std.mem.sliceTo(&notification.title, 0);
                let title = &notification.title[..notification
                    .title
                    .iter()
                    .position(|&b| b == 0)
                    .unwrap_or(notification.title.len())];
                // const body = std.mem.sliceTo(&notification.body, 0);
                let body = &notification.body[..notification
                    .body
                    .iter()
                    .position(|&b| b == 0)
                    .unwrap_or(notification.body.len())];
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
                if let Err(err) = unsafe { &*self.rt_app }
                    .perform_action(apprt::Target::Surface(&*self), apprt::Action::RingBell)
                {
                    // log.warn("apprt failed to ring bell={}", .{err});
                    log::warn!(target: LOG_SCOPE, "apprt failed to ring bell={err:?}");
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
                if let Err(err) = unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::Surface(&*self),
                    apprt::Action::ProgressReport(v),
                ) {
                    // log.warn("apprt failed to report progress err={}", .{err});
                    log::warn!(target: LOG_SCOPE, "apprt failed to report progress err={err:?}");
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
                //
                // (`{f}` calls the type's own `format`; `Duration::format` is
                // the faithful port of it, so this is byte-identical.)
                {
                    let mut duration_buf: Vec<u8> = Vec::new();
                    duration.format(&mut duration_buf);
                    log::debug!(
                        target: LOG_SCOPE,
                        "command took {}",
                        String::from_utf8_lossy(&duration_buf)
                    );
                }

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
                if let Err(err) = unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::Surface(&*self),
                    apprt::Action::CommandFinished(CommandFinished {
                        exit_code: v,
                        duration,
                    }),
                ) {
                    // log.warn("apprt failed to notify command finish={}", .{err});
                    log::warn!(target: LOG_SCOPE, "apprt failed to notify command finish={err:?}");
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
                log::warn!(
                    target: LOG_SCOPE,
                    "abnormal process exit detected, showing error message"
                );

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
                    Err(err) => {
                        // log.err("error trying to show native child exited GUI err={}", .{err});
                        log::error!(
                            target: LOG_SCOPE,
                            "error trying to show native child exited GUI err={err:?}"
                        );
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
                if let Err(err) = self.childExitedAbnormally(info) {
                    // log.err("error handling abnormal child exit err={}", .{err});
                    log::error!(target: LOG_SCOPE, "error handling abnormal child exit err={err:?}");
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
                Err(err) => {
                    // log.err("error trying to show native child exited GUI err={}", .{err});
                    log::error!(
                        target: LOG_SCOPE,
                        "error trying to show native child exited GUI err={err:?}"
                    );
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
    // — Zig `Allocator.Error` surfaced as vt `OutOfMemory`) plus the
    // `printString` UTF-8 validation error (`error.InvalidUtf8`), together
    // `PrintStringError`; the join/allocPrint OOMs collapse (infallible Rust
    // allocation).
    #[allow(clippy::redundant_closure_call)]
    fn childExitedAbnormally(
        &mut self,
        info: apprt::surface::ChildExited,
    ) -> Result<(), PrintStringError> {
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

        let result = (|| -> Result<(), PrintStringError> {
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
                // PORT NOTE (R2 identity): the product name in the user-facing
                // message — `rustty_identity::APP_NAME_TITLECASE` (the oracle
                // prints the capitalized product name here).
                (*t).print_string(
                    format!(
                        "{} failed to launch the requested command:",
                        rustty_identity::APP_NAME_TITLECASE
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
        if let Err(err) = unsafe { &*self.rt_app }.perform_action(
            apprt::Target::Surface(&*self),
            apprt::Action::SecureInput(if v { SecureInput::On } else { SecureInput::Off }),
        ) {
            // We ignore this error because we don't want to fail this
            // entire operation just because the apprt failed to set
            // the secure input state.
            // log.warn("apprt failed to set secure input state err={}", .{err});
            log::warn!(target: LOG_SCOPE, "apprt failed to set secure input state err={err:?}");
        }

        // try self.queueRender();
        self.queueRender()?;
        Ok(())
    }

    // fn searchCallback(event: terminal.search.Thread.Event, ud: ?*anyopaque) void {
    //
    // PORT NOTE: the C14 `performBindingAction` search arm registers this as
    // the vt `search::thread::Options.event_cb` fn pointer (`?*anyopaque` ->
    // the seam's bare `*mut c_void`). Now LIVE (the `.search` arm registers it).
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
        //
        // PORT NOTE (§3.3a): pass the RAW `self_` (NOT `&mut *self_`) — a whole
        // `&mut Surface` reborrow here, on the search thread, races the main
        // thread's `&mut self` = SB UB (mirihar `callback_naive`).
        if let Err(err) = unsafe { Self::searchCallback_(self_, event) } {
            // log.warn("error in search callback err={}", .{err});
            //
            // NOTE (task 14d-1): this emission fires on the SEARCH THREAD, so
            // `log_capture::start`'s thread-isolated capture cannot observe it;
            // a test for it needs `log_capture::start_admitting_all_threads`.
            log::warn!(target: LOG_SCOPE, "error in search callback err={err:?}");
        }
    }

    // fn searchCallback_(
    //     self: *Surface,
    //     event: terminal.search.Thread.Event,
    // ) !void {
    //
    // PORT NOTE (§3.3a): the reference `self: *Surface` (a raw ptr) is retyped
    // off `&mut self` to `self_: *mut Surface` — the callback fires on the SEARCH
    // THREAD while the main thread holds `&mut self`, so a whole `&mut Surface`
    // reborrow is SB UB. Every access below is a NARROW raw field projection off
    // `self_` (never a whole `&mut Surface`/`&Surface`); verdict SB + TB clean
    // (mirihar `callback_shipped`). This preserves the reference's cross-thread
    // Surface access — only the Rust `&mut`-exclusivity narrowing changes.
    unsafe fn searchCallback_(
        self_: *mut Surface<A>,
        event: SearchThreadEvent<'_>,
    ) -> Result<(), Error> {
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
                let _ = unsafe { &*(*self_).renderer_thread.mailbox }.push(
                    rendererpkg::Message::SearchViewportMatches(
                        rendererpkg::message::SearchMatches { matches },
                    ),
                    Timeout::Forever,
                );
                // try self.renderer_thread.wakeup.notify();
                unsafe { &(*self_).renderer_thread.wakeup }.notify()?;
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
                    let _ = unsafe { &*(*self_).renderer_thread.mailbox }.push(
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
                    let _ = unsafe { Self::surface_mailbox_raw(self_) }
                        .push(Message::SearchSelected(Some(sel.idx)), Timeout::Forever);
                } else {
                    // Reset our selected match
                    // _ = self.renderer_thread.mailbox.push(
                    //     .{ .search_selected_match = null },
                    //     .forever,
                    // );
                    let _ = unsafe { &*(*self_).renderer_thread.mailbox }.push(
                        rendererpkg::Message::SearchSelectedMatch(None),
                        Timeout::Forever,
                    );

                    // Reset the selected index
                    // _ = self.surfaceMailbox().push(
                    //     .{ .search_selected = null },
                    //     .forever,
                    // );
                    let _ = unsafe { Self::surface_mailbox_raw(self_) }
                        .push(Message::SearchSelected(None), Timeout::Forever);
                }

                // try self.renderer_thread.wakeup.notify();
                unsafe { &(*self_).renderer_thread.wakeup }.notify()?;
            }

            // .total_matches => |total| {
            SearchThreadEvent::TotalMatches(total) => {
                // _ = self.surfaceMailbox().push(
                //     .{ .search_total = total },
                //     .forever,
                // );
                //
                // (Zig coerces the `usize` into the `?usize` payload.)
                let _ = unsafe { Self::surface_mailbox_raw(self_) }
                    .push(Message::SearchTotal(Some(total)), Timeout::Forever);
            }

            // When we quit, tell our renderer to reset any search state.
            // .quit => {
            SearchThreadEvent::Quit => {
                // _ = self.renderer_thread.mailbox.push(
                //     .{ .search_selected_match = null },
                //     .forever,
                // );
                let _ = unsafe { &*(*self_).renderer_thread.mailbox }.push(
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
                let _ = unsafe { &*(*self_).renderer_thread.mailbox }.push(
                    rendererpkg::Message::SearchViewportMatches(
                        rendererpkg::message::SearchMatches {
                            matches: Vec::new(),
                        },
                    ),
                    Timeout::Forever,
                );
                // try self.renderer_thread.wakeup.notify();
                unsafe { &(*self_).renderer_thread.wakeup }.notify()?;

                // Reset search totals in the surface
                // _ = self.surfaceMailbox().push(
                //     .{ .search_total = null },
                //     .forever,
                // );
                let _ = unsafe { Self::surface_mailbox_raw(self_) }
                    .push(Message::SearchTotal(None), Timeout::Forever);
                // _ = self.surfaceMailbox().push(
                //     .{ .search_selected = null },
                //     .forever,
                // );
                let _ = unsafe { Self::surface_mailbox_raw(self_) }
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
    // PORT NOTE: the C10 keyCallback (:2720) / C12 mouseButtonCallback
    // (:3774) / C13 cursorPosCallback (:4558) callsites are live.
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
            if let Err(err) = self.queueRender() {
                // Not a big deal if this fails.
                // log.warn("failed to notify renderer of mods change err={}", .{err});
                log::warn!(
                    target: LOG_SCOPE,
                    "failed to notify renderer of mods change err={err:?}"
                );
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
    // PORT NOTE: the C10 keyCallback (:2733) and C13 cursorPosCallback
    // (:4609) callsites are live (both call with the renderer mutex held).
    // The inferred `!void` is the [`Error`] union.
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
                        log::debug!(
                            target: LOG_SCOPE,
                            "mouse moved while left click held, ignoring link hover"
                        );
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
                            log::warn!(target: LOG_SCOPE, "failed to get URI for OSC8 hyperlink");
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
        log::warn!(target: LOG_SCOPE, "renderer health status change status={health:?}");
        // _ = self.rt_app.performAction(
        //     .{ .surface = self },
        //     .renderer_health,
        //     health,
        // ) catch |err| {
        //     log.warn("failed to notify app of renderer health change err={}", .{err});
        // };
        if let Err(err) = unsafe { &*self.rt_app }.perform_action(
            apprt::Target::Surface(&*self),
            apprt::Action::RendererHealth(health),
        ) {
            // log.warn("failed to notify app of renderer health change err={}", .{err});
            log::warn!(
                target: LOG_SCOPE,
                "failed to notify app of renderer health change err={err:?}"
            );
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
        if let Err(err) = unsafe { &*self.rt_app }.perform_action(
            apprt::Target::Surface(&*self),
            apprt::Action::Scrollbar(scrollbar),
        ) {
            // log.warn("failed to notify app of scrollbar change err={}", .{err});
            log::warn!(target: LOG_SCOPE, "failed to notify app of scrollbar change err={err:?}");
        }
    }

    /// This should be called anytime `config_conditional_state` changes
    /// so that the apprt can reload the configuration.
    // fn notifyConfigConditionalState(self: *Surface) void {
    //
    // PORT NOTE: the C13 colorSchemeCallback (:4720) callsite is live.
    fn notifyConfigConditionalState(&mut self) {
        // _ = self.rt_app.performAction(
        //     .{ .surface = self },
        //     .reload_config,
        //     .{ .soft = true },
        // ) catch |err| {
        //     log.warn("failed to notify app of config state change err={}", .{err});
        // };
        if let Err(err) = unsafe { &*self.rt_app }.perform_action(
            apprt::Target::Surface(&*self),
            apprt::Action::ReloadConfig(ReloadConfig { soft: true }),
        ) {
            // log.warn("failed to notify app of config state change err={}", .{err});
            log::warn!(
                target: LOG_SCOPE,
                "failed to notify app of config state change err={err:?}"
            );
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
            Err(err) => {
                // If the derivation fails then we just log and return. We don't
                // hard fail in this case because we don't want to error the surface
                // when config fails we just want to keep using the old config.
                // log.err("error updating configuration err={}", .{err});
                log::error!(target: LOG_SCOPE, "error updating configuration err={err:?}");
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
        if let Err(err) = self.deactivateAllKeyTables() {
            // log.warn("failed to deactivate key tables err={}", .{err});
            log::warn!(target: LOG_SCOPE, "failed to deactivate key tables err={err:?}");
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
                log::info!(
                    target: LOG_SCOPE,
                    "font size manually adjusted, preserving previous size on config reload"
                );
                // break :font_size self.font_size;
                break 'font_size self.font_size;
            }

            // If we haven't, then we update to the configured font size.
            // This allows config changes to update the font size. We used to
            // never do this but it was a common source of confusion and people
            // assumed that Rustty was broken! This logic makes more sense.
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
        if let Err(err) = self.queueRender() {
            // log.warn("failed to notify renderer of config change err={}", .{err});
            log::warn!(target: LOG_SCOPE, "failed to notify renderer of config change err={err:?}");
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
        // const width = @max(
        //     self.config.window_width,
        //     min_window_width_cells,
        // ) * self.size.cell.width;
        // const width_f32: f32 = @floatFromInt(width);
        // const height_f32: f32 = @floatFromInt(height);
        //
        // The final values are affected by content scale and we need to
        // account for the padding so we get the exact correct grid size.
        // const final_width: u32 =
        //     @as(u32, @intFromFloat(@ceil(width_f32 / scale.x))) +
        //     self.size.padding.left +
        //     self.size.padding.right;
        // const final_height: u32 =
        //     @as(u32, @intFromFloat(@ceil(height_f32 / scale.y))) +
        //     self.size.padding.top +
        //     self.size.padding.bottom;
        //
        // PORT NOTE: reference defect (R13, UPSTREAM-DEFECTS.md UD59). The
        // computation above is in `initial_size_px`, checked. On overflow the
        // port returns here, the function's existing skip path (the one it
        // takes when either option is unset), so no `initial_size` is sent.
        let Some((final_width, final_height)) = initial_size_px(
            self.config.window_width,
            self.config.window_height,
            self.size,
            scale,
        ) else {
            return Ok(());
        };

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
            terminal::screen::SelectionString { sel, trim: false },
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
                    log::warn!(target: LOG_SCOPE, "viewport bottom-right pin not found, bug?");
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
            terminal::screen::SelectionString { sel, trim: false },
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
    // lossy `String`). The C13 processLinks callsite (:4396) is live.
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
            //
            // PORT NOTE: reference defect (R13), UD76 (task R13-C2). The
            // reference's `u32` product and sum are unchecked; a saturated
            // `padding.left` (UD71) stored by `contentScaleCallback` overflows
            // them for any `cursor.x >= 1` until the IO thread's resize
            // shrinks the grid. Saturate, as the padding sums do (UD71): the
            // point is only an IME position.
            let mut x: f64 = f64::from(
                u32::from(cursor.x)
                    .saturating_mul(self.size.cell.width)
                    .saturating_add(self.size.padding.left),
            );

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
            // (R13, UD76: saturating, as for `x` above.)
            let mut y: f64 = f64::from(
                u32::from(cursor.y)
                    .saturating_mul(self.size.cell.height)
                    .saturating_add(self.size.padding.top),
            );

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
            log::info!(
                target: LOG_SCOPE,
                "application attempted to write clipboard, but 'clipboard-write' is set to deny"
            );
            return Ok(());
        }

        // Decode with the SIMD decoder, strict per the Kitty clipboard
        // spec that also governs OSC 52 base64 handling: a request with
        // characters outside the base64 alphabet is discarded entirely
        // (never partially decoded), while a missing-padding tail is
        // tolerated since OSC 52 has no way to report errors.
        // var buf = try self.alloc.allocSentinel(u8, simd.base64.maxLen(data), 0);
        // defer self.alloc.free(buf);
        //
        // (bc2f7d7d2: the std.base64 Decoder + `zig_base64` calc/decode pair is
        // replaced by the strict SIMD decode; `allocSentinel` — the max_len
        // bytes plus the NUL sentinel slot, and the OOM-only `try` collapses.)
        let max_len = rustty_simd::base64::max_len(data);
        let mut buf: Vec<u8> = vec![0u8; max_len + 1];
        // const decoded = simd.base64.decodeStrict(data, buf, .optional) catch {
        //     log.info("application sent invalid base64 data for OSC 52", .{});
        //     return;
        // };
        let decoded_len = match rustty_simd::base64::decode_strict(
            data,
            &mut buf[0..max_len],
            rustty_simd::base64::Padding::Optional,
        ) {
            Ok(decoded) => decoded.len(),
            Err(_) => {
                // log.info("application sent invalid base64 data for OSC 52", .{});
                log::info!(target: LOG_SCOPE, "application sent invalid base64 data for OSC 52");
                return Ok(());
            }
        };
        // buf[decoded.len] = 0;
        buf[decoded_len] = 0;

        // When clipboard-write is "ask" a prompt is displayed to the user asking
        // them to confirm the clipboard access. Each app runtime handles this
        // differently.
        // const confirm = self.config.clipboard_write == .ask;
        let confirm = self.config.clipboard_write == configpkg::value_types::ClipboardAccess::Ask;
        // self.rt_surface.setClipboard(loc, &.{.{
        //     .mime = "text/plain",
        //     .data = buf[0..decoded.len :0],
        // }}, confirm) catch |err| {
        //     log.err("error setting clipboard string err={}", .{err});
        //     return;
        // };
        if let Err(err) = unsafe { &*self.rt_surface }.set_clipboard(
            loc,
            &[apprt::ClipboardContent {
                mime: b"text/plain".as_slice(),
                data: &buf[0..decoded_len],
            }],
            confirm,
        ) {
            // log.err("error setting clipboard string err={}", .{err});
            log::error!(target: LOG_SCOPE, "error setting clipboard string err={err:?}");
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
            if let Err(err) =
                unsafe { &*self.rt_surface }.set_clipboard(*clipboard, &contents_items, false)
            {
                // log.err(
                //     "error setting clipboard string clipboard={} err={}",
                //     .{ clipboard, err },
                // );
                log::error!(
                    target: LOG_SCOPE,
                    "error setting clipboard string clipboard={clipboard:?} err={err:?}"
                );
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
            if let Err(err) = unsafe { &*self.rt_app }.perform_action(
                apprt::Target::Surface(&*self),
                apprt::Action::SelectionChanged,
            ) {
                // log.warn("apprt failed selection_changed notification err={}", .{err});
                log::warn!(
                    target: LOG_SCOPE,
                    "apprt failed selection_changed notification err={err:?}"
                );
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
    // PORT NOTE: the C12 mouseButtonCallback (:3866/:4086/:4092) callsites
    // are live; the C14 performBindingAction (:5450) callsite lands later.
    fn setSelectionAndCopy(&mut self, sel: terminal::selection::Selection) -> Result<(), Error> {
        // try self.setSelection(sel);
        self.setSelection(Some(sel))?;

        // switch (self.config.copy_on_select) {
        match self.config.copy_on_select {
            // .none => {},
            configpkg::value_types::CopyOnSelect::None => {}

            // The selection clipboard is set if supported, otherwise nothing is copied.
            // .primary => try self.copySelectionToClipboards(
            //     sel,
            //     &.{.selection},
            //     .mixed,
            // ),
            configpkg::value_types::CopyOnSelect::Primary => self.copySelectionToClipboards(
                sel,
                &[apprt::Clipboard::Selection],
                binding::CopyToClipboard::Mixed,
            )?,

            // Only the standard clipboard is set.
            // .clipboard => try self.copySelectionToClipboards(
            //     sel,
            //     &.{.standard},
            //     .mixed,
            // ),
            configpkg::value_types::CopyOnSelect::Clipboard => self.copySelectionToClipboards(
                sel,
                &[apprt::Clipboard::Standard],
                binding::CopyToClipboard::Mixed,
            )?,

            // Both standard and selection clipboards are set.
            // .both => try self.copySelectionToClipboards(
            //     sel,
            //     &.{ .standard, .selection },
            //     .mixed,
            // ),
            configpkg::value_types::CopyOnSelect::Both => self.copySelectionToClipboards(
                sel,
                &[apprt::Clipboard::Standard, apprt::Clipboard::Selection],
                binding::CopyToClipboard::Mixed,
            )?,
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
        if let Err(err) = self.recomputeInitialSize() {
            // We don't treat this as a fatal error because not setting
            // an initial size shouldn't stop our terminal from working.
            // log.warn("unable to recompute initial window size: {}", .{err});
            log::warn!(target: LOG_SCOPE, "unable to recompute initial window size: {err:?}");
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
        log::debug!(target: LOG_SCOPE, "set font size size={}", size.points);

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
        // Rustty `Key` is OWNED (the shared_grid_set PORT NOTE), so the
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

        // Recalculate our grid size. Because Rustty supports fluid resizing,
        // its possible the grid doesn't change at all even if the screen size changes.
        // We have to update the IO thread no matter what because we send
        // pixel-level sizing to the subprocess.
        // const grid_size = self.size.grid();
        let grid_size = self.size.grid();
        // if (grid_size.columns < 5 and (self.size.padding.left > 0 or self.size.padding.right > 0)) {
        if grid_size.columns < 5 && (self.size.padding.left > 0 || self.size.padding.right > 0) {
            // log.warn("WARNING: very small terminal grid detected with padding " ++
            //     "set. Is your padding reasonable?", .{});
            log::warn!(
                target: LOG_SCOPE,
                "WARNING: very small terminal grid detected with padding set. Is your padding reasonable?"
            );
        }
        // if (grid_size.rows < 2 and (self.size.padding.top > 0 or self.size.padding.bottom > 0)) {
        if grid_size.rows < 2 && (self.size.padding.top > 0 || self.size.padding.bottom > 0) {
            // log.warn("WARNING: very small terminal grid detected with padding " ++
            //     "set. Is your padding reasonable?", .{});
            log::warn!(
                target: LOG_SCOPE,
                "WARNING: very small terminal grid detected with padding set. Is your padding reasonable?"
            );
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
        // (COMMENT-ONLY BY DESIGN: commented out in the ORACLE too, Surface.zig:2540.)

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
        // (PORT NOTE: `event_orig` moves into `event` — the Rust KeyEvent owns its
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
        // (COMMENT-ONLY BY DESIGN: commented out in the ORACLE too, Surface.zig:2666.)

        // Apply key remappings to transform modifiers before any processing.
        // This allows users to remap modifier keys at the app level.
        // var event = event_orig;
        //
        // (PORT NOTE: the keyEventIsBinding move-and-read note above.)
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
                    if let Err(err) = self.mouseRefreshLinks(pos, pos_vp, self.mouse.over_link) {
                        // log.warn("failed to refresh links err={}", .{err});
                        log::warn!(target: LOG_SCOPE, "failed to refresh links err={err:?}");
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
                // write_req.deinit();
                //
                // (Drop-owned `WriteReq` — freed by the return below.)
                // self.close();
                self.close();
                // return .closed;
                return Ok(InputEffect::Closed);
            }

            // (the reference's former `errdefer write_req.deinit()` is gone:
            // `queueIo` takes ownership of the message and cannot fail.)
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
                // Store this event so that we can drain and encode on invalid.
                // We don't need to cap this because it is naturally capped by
                // the config validation.
                // if (try self.encodeKey(event, insp_ev)) |req| {
                //     self.keyboard.sequence_queued.append(self.alloc, req) catch |err| {
                //         req.deinit();
                //         return err;
                //     };
                // }
                //
                // (the append `catch` collapses: Vec `push` cannot fail, and
                // `req` is Drop-owned so the reference's on-error `req.deinit()`
                // has no counterpart.)
                if let Some(req) = self.encodeKey(event, insp_ev.as_deref_mut())? {
                    self.keyboard.sequence_queued.push(req);
                }

                // Setup the next set we'll look at only after all fallible work.
                // self.keyboard.sequence_set = set;
                self.keyboard.sequence_set = Some(&**set as *const binding::Set);

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
                if let Err(err) = unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::Surface(&*self),
                    apprt::Action::KeySequence(KeySequence::Trigger(trigger)),
                ) {
                    // log.warn("failed to notify app of key sequence err={}", .{err});
                    log::warn!(target: LOG_SCOPE, "failed to notify app of key sequence err={err:?}");
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
        log::debug!(
            target: LOG_SCOPE,
            "key event binding flags={:?} action={:?}",
            leaf.flags,
            actions
        );
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
                    Err(err) => {
                        // log.info("key binding action failed action={t} err={}", .{ action, err });
                        log::info!(
                            target: LOG_SCOPE,
                            "key binding action failed action={} err={err:?}",
                            action.tag_name()
                        );
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
                    log::debug!(
                        target: LOG_SCOPE,
                        "key binding is a closing binding, halting key event processing"
                    );
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
        if let Err(err) = unsafe { &*self.rt_app }.perform_action(
            apprt::Target::Surface(&*self),
            apprt::Action::KeyTable(KeyTable::DeactivateAll),
        ) {
            // log.warn("failed to notify app of key table err={}", .{err});
            log::warn!(target: LOG_SCOPE, "failed to notify app of key table err={err:?}");
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
        if let Err(err) = unsafe { &*self.rt_app }.perform_action(
            apprt::Target::Surface(&*self),
            apprt::Action::KeySequence(KeySequence::End),
        ) {
            // log.warn("failed to notify app of key sequence end err={}", .{err});
            log::warn!(target: LOG_SCOPE, "failed to notify app of key sequence end err={err:?}");
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
            // Rustty; the deinit precedent.)
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
            // is its drop — `toOwnedSlice` below transfers out of it.)
            let mut alloc_writer: Vec<u8> =
                Vec::with_capacity(std::cmp::max(event.utf8.len() * 2, data.len() * 2));

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
            // break :req .{ .alloc = .{
            //     .alloc = self.alloc,
            //     .data = try alloc_writer.toOwnedSlice(),
            // } };
            //
            // (`fb4c56159`: transfer the encoding buffer instead of copying it.
            // `toOwnedSlice` shrinks the allocation to the written length and
            // hands over ownership — `into_boxed_slice` is the same contract:
            // shrink-to-fit, remapping or copy-and-free as the allocator
            // allows; its only error is OOM, the `try` collapses.)
            break 'req MessageData::Alloc(Alloc {
                data: alloc_writer.into_boxed_slice(),
            });
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

        // PORT NOTE (oracle 6c8c07981): this CHANGES EXISTING BEHAVIOUR, it is
        // not additive. Before this commit every occlusion callback pushed a
        // renderer `Visible` message and queued a render; now a callback whose
        // state matches the last one returns early, so repeated same-state
        // callbacks NO LONGER reach the renderer at all.
        //
        // VERIFIED (V21, `P3-W11 Task 6`): the dedup, the `flags.visible` write
        // under the renderer lock and the mode-2033-gated report are covered by
        // `crates/rustty-core/tests/surface_visibility_kat.rs`, which builds the
        // `TestSurface` the V21 recipe describes. Deleting the early-return
        // below makes it fail (a repeat callback then pushes a second renderer
        // message). The neighbouring halves stay covered by
        //   - `rustty-vt` `stream_terminal::tests::visibility_reports` (the RIS
        //     exemption and the vt-side report/mode arms), and
        //   - `rustty-termio`
        //     `stream_handler::tests::stream_handler_visibility_report_arms_push_forced_messages`
        //     (the mode-2033 + DSR arms).
        // STILL OPEN in V21: the last hop, "the queued `VisibilityReport` is
        // drained by `termio::Thread` and the encoded bytes land in the pty
        // write queue" — that needs a live pty + io thread. The KAT pins the
        // encoded payload (`\x1b[?999;2n`) but not the delivery.
        //
        // Avoid duplicate renderer and visibility reports.
        // if (self.visible == visible) return;
        // self.visible = visible;
        if self.visible == visible {
            return Ok(());
        }
        self.visible = visible;

        // Update the terminal state for synchronous queries, then notify the IO
        // thread so it can emit a mode 2033 report when enabled.
        // self.renderer_state.mutex.lockUncancelable(global.io());
        // self.io.terminal.flags.visible = visible;
        // const report_visibility = self.io.terminal.modes.get(.report_visibility);
        // self.renderer_state.mutex.unlock(global.io());
        //
        // SAFETY: `renderer_state.mutex` is the guard-less shared lock; `io` is
        // the live Termio owned by this Surface.
        unsafe { (*self.renderer_state.mutex).lock() };
        let report_visibility = unsafe {
            let t = &raw mut self.io.terminal;
            (*t).flags.visible = visible;
            (*t).modes.get(rustty_vt::modes::Mode::ReportVisibility)
        };
        unsafe { (*self.renderer_state.mutex).unlock() };
        // if (report_visibility) {
        //     self.queueIo(.{ .visibility_report = .{ .visible = visible, .force = false } }, .unlocked);
        // }
        if report_visibility {
            self.queueIo(
                termio::Message::VisibilityReport {
                    visible,
                    force: false,
                },
                termio::termio_impl::MutexState::Unlocked,
            );
        }

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
                    // CORRECTED (task 14d-1): `unwrap_or` DISCARDED the error
                    // the reference logs. The `catch |err| err: { log.warn(...);
                    // break :err .ignored; }` block is restored whole — the
                    // fallback AND its diagnostic.
                    let effect = match self.keyCallback(pressed_key.clone()) {
                        Ok(v) => v,
                        Err(err) => {
                            // log.warn("error releasing key on focus loss err={}", .{err});
                            log::warn!(
                                target: LOG_SCOPE,
                                "error releasing key on focus loss err={err:?}"
                            );
                            InputEffect::Ignored
                        }
                    };
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
                                    // (same restored catch block as the
                                    // release-the-full-key site above.)
                                    let effect = match self.keyCallback(pressed_key.clone()) {
                                        Ok(v) => v,
                                        Err(err) => {
                                            // log.warn("error releasing key on focus loss err={}", .{err});
                                            log::warn!(
                                                target: LOG_SCOPE,
                                                "error releasing key on focus loss err={err:?}"
                                            );
                                            InputEffect::Ignored
                                        }
                                    };
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
        // (COMMENT-ONLY BY DESIGN: commented out in the ORACLE too, Surface.zig:3460.)

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

        // const y: ScrollAmount = if (yoff == 0) .{} else y: { ... };
        //
        // PORT NOTE (task R13-P2, UD62): the block body is `scroll_amount_y`,
        // lifted out verbatim so its float cast can be driven by a test
        // (`Surface::init` needs a live apprt). It reads exactly the three
        // fields the block reads: the mouse state, the cell size and the
        // scroll multiplier.
        let y: ScrollAmount = scroll_amount_y(
            &mut self.mouse,
            self.size.cell,
            self.config.mouse_scroll_multiplier,
            yoff,
            scroll_mods,
        );

        // For detailed comments see the y calculation above.
        // const x: ScrollAmount = if (xoff == 0) .{} else x: { ... };
        //
        // (task R13-P2 — lifted into `scroll_amount_x`, as the y block is.)
        let x: ScrollAmount = scroll_amount_x(&mut self.mouse, self.size.cell, xoff, scroll_mods);

        // log.info("SCROLL: delta_y={} delta_x={}", .{ y.delta, x.delta });
        // (COMMENT-ONLY BY DESIGN: commented out in the ORACLE too, Surface.zig:3548.)

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

    /// Same as `isMouseReporting` but takes the renderer state lock, so it
    /// is safe to call from the apprt (any thread that is not already holding
    /// the lock).
    /// Returns true if mouse reporting is enabled both in the config and
    /// the terminal state.
    // fn isMouseReporting(self: *const Surface) bool {
    //
    // (`*const` receiver -> `&self`.)
    fn isMouseReporting(&self) -> bool {
        // return self.config.mouse_reporting and
        //     self.io.terminal.flags.mouse_event != .none;
        self.config.mouse_reporting
            && self.io.terminal.flags.mouse_event != terminal::mouse::Event::None
    }

    // pub fn mouseReportingActive(self: *Surface) bool {
    //
    // (`488b7109b` + the `48d85eaeb` lock fix, END STATE: the gtk apprt asks
    // whether to forward a middle click to a mouse-reporting TUI.)
    pub fn mouseReportingActive(&mut self) -> bool {
        // self.renderer_state.mutex.lockUncancelable(global.io());
        // defer self.renderer_state.mutex.unlock(global.io());
        //
        // (guard-less zigstd Mutex — the `lockUncancelable`/deferred-unlock
        // pair maps to the explicit lock/unlock around the read, the
        // `setVisible` precedent.)
        unsafe { (*self.renderer_state.mutex).lock() };
        // return self.isMouseReporting();
        let result = self.isMouseReporting();
        unsafe { (*self.renderer_state.mutex).unlock() };
        result
    }

    // fn mouseReport(
    //     self: *Surface,
    //     button: ?input.MouseButton,
    //     action: input.MouseAction,
    //     mods: input.Mods,
    //     pos: apprt.CursorPos,
    // ) void {
    fn mouseReport(
        &mut self,
        button: Option<input_mouse::Button>,
        action: input_mouse::Action,
        mods: Mods,
        pos: apprt::CursorPos,
    ) {
        // Mouse reporting must be enabled by both config and terminal state
        // assert(self.config.mouse_reporting);
        debug_assert!(self.config.mouse_reporting);
        // assert(self.io.terminal.flags.mouse_event != .none);
        debug_assert!(self.io.terminal.flags.mouse_event != terminal::mouse::Event::None);

        // Build our encoding options.
        // const encoding_opts: input.mouse_encode.Options = opts: {
        let encoding_opts: mouse_encode::Options<'_> = 'opts: {
            // Terminal and size state.
            // var opts: input.mouse_encode.Options = .fromTerminal(
            //     &self.io.terminal,
            //     self.size,
            // );
            //
            // (the ported `from_terminal` takes the two terminal-flags reads
            // directly — the mouse_encode.rs constructor-seam note.)
            let mut opts: mouse_encode::Options<'_> = mouse_encode::Options::from_terminal(
                self.io.terminal.flags.mouse_event,
                self.io.terminal.flags.mouse_format,
                self.size,
            );

            // Whether any button is pressed at all.
            // opts.any_button_pressed = pressed: {
            opts.any_button_pressed = 'pressed: {
                // for (self.mouse.click_state) |state| {
                for state in self.mouse.click_state {
                    // if (state != .release) break :pressed true;
                    if state != input_mouse::ButtonState::Release {
                        break 'pressed true;
                    }
                }

                // break :pressed false;
                false
            };

            // Keep track of our last reported viewport cell for event
            // deduplication.
            // opts.last_cell = &self.mouse.event_point;
            opts.last_cell = Some(&mut self.mouse.event_point);

            // break :opts opts;
            break 'opts opts;
        };

        // var data: termio.Message.WriteReq.Small.Array = undefined;
        // var writer: std.Io.Writer = .fixed(&data);
        //
        // PORT NOTE: `Small.Array` == `[u8; WriteReq::MAX]`; Zig `undefined`
        // -> zeroed (the tail past `len` is never read). The `.fixed` writer
        // is a Cursor over the stack array — a write past the end is io's
        // `WriteZero`, the reference's `error.WriteFailed` (the encodeKey
        // precedent).
        let mut data = [0u8; termio::message::WriteReq::MAX];
        let mut writer = std::io::Cursor::new(&mut data[..]);
        // input.mouse_encode.encode(&writer, .{
        //     .button = button,
        //     .action = action,
        //     .mods = mods,
        //     .pos = .{
        //         .x = pos.x,
        //         .y = pos.y,
        //     },
        // }, encoding_opts) catch |err| switch (err) {
        //     error.WriteFailed => { ... },
        // };
        if let Err(_err) = mouse_encode::encode(
            &mut writer,
            mouse_encode::Event {
                button,
                action,
                mods,
                pos: mouse_encode::Pos { x: pos.x, y: pos.y },
            },
            encoding_opts,
        ) {
            // This should never happen since mouse events should never
            // be able to overflow the size of our small array. But if it
            // does, let's log it and return. No need to crash upstreams.
            // In the future we may want to fall back to allocation.
            // log.warn("failed to encode mouse event err={}", .{err});
            log::warn!(target: LOG_SCOPE, "failed to encode mouse event err={_err:?}");
            return;
        }
        // const written = writer.buffered();
        let written_len = writer.position() as usize;
        // if (written.len == 0) return;
        if written_len == 0 {
            return;
        }

        // self.queueIo(.{ .write_small = .{
        //     .data = data,
        //     .len = @intCast(written.len),
        // } }, .locked);
        self.queueIo(
            termio::Message::WriteSmall(Small {
                data,
                len: written_len,
            }),
            termio::termio_impl::MutexState::Locked,
        );
    }

    /// Returns true if the shift modifier is allowed to be captured by modifier
    /// events. It is up to the caller to still verify it is a situation in which
    /// shift capture makes sense (i.e. left button, mouse click, etc.)
    // fn mouseShiftCapture(self: *const Surface, lock: bool) bool {
    //
    // (`*const` receiver -> `&self`.)
    fn mouseShiftCapture(&self, lock: bool) -> bool {
        // Handle our never/always case where we don't need a lock.
        // switch (self.config.mouse_shift_capture) {
        match self.config.mouse_shift_capture {
            // .never => return false,
            configpkg::value_types::MouseShiftCapture::Never => return false,
            // .always => return true,
            configpkg::value_types::MouseShiftCapture::Always => return true,
            // .false, .true => {},
            configpkg::value_types::MouseShiftCapture::False
            | configpkg::value_types::MouseShiftCapture::True => {}
        }

        // if (lock) self.renderer_state.mutex.lock();
        if lock {
            unsafe { (*self.renderer_state.mutex).lock() };
        }
        // defer if (lock) self.renderer_state.mutex.unlock();
        //
        // (guard-less zigstd Mutex — the defer is the conditional explicit
        // unlock at every exit below.)

        // If the terminal explicitly requests it then we always allow it
        // since we processed never/always at this point.
        // switch (self.io.terminal.flags.mouse_shift_capture) {
        match self.io.terminal.flags.mouse_shift_capture {
            // .false => return false,
            terminal::terminal::MouseShiftCapture::False => {
                // (the deferred conditional unlock — the false return)
                if lock {
                    unsafe { (*self.renderer_state.mutex).unlock() };
                }
                return false;
            }
            // .true => return true,
            terminal::terminal::MouseShiftCapture::True => {
                // (the deferred conditional unlock — the true return)
                if lock {
                    unsafe { (*self.renderer_state.mutex).unlock() };
                }
                return true;
            }
            // .null => {},
            terminal::terminal::MouseShiftCapture::Null => {}
        }

        // Otherwise, go with the user's preference
        // return switch (self.config.mouse_shift_capture) {
        let result = match self.config.mouse_shift_capture {
            // .false => false,
            configpkg::value_types::MouseShiftCapture::False => false,
            // .true => true,
            configpkg::value_types::MouseShiftCapture::True => true,
            // .never, .always => unreachable, // handled earlier
            configpkg::value_types::MouseShiftCapture::Never
            | configpkg::value_types::MouseShiftCapture::Always => unreachable!(), // handled earlier
        };
        // (the deferred conditional unlock)
        if lock {
            unsafe { (*self.renderer_state.mutex).unlock() };
        }
        result
    }

    /// Returns true if the mouse is currently captured by the terminal
    /// (i.e. reporting events).
    // pub fn mouseCaptured(self: *Surface) bool {
    pub fn mouseCaptured(&mut self) -> bool {
        // self.renderer_state.mutex.lock();
        unsafe { (*self.renderer_state.mutex).lock() };
        // defer self.renderer_state.mutex.unlock();
        //
        // (guard-less zigstd Mutex — the defer is the explicit unlock below.)
        // return self.io.terminal.flags.mouse_event != .none;
        let result = self.io.terminal.flags.mouse_event != terminal::mouse::Event::None;
        // (the deferred mutex unlock)
        unsafe { (*self.renderer_state.mutex).unlock() };
        result
    }

    /// Called for mouse button press/release events. This will return true
    /// if the mouse event was consumed in some way (i.e. the program is capturing
    /// mouse events). If the event was not consumed, then false is returned.
    // pub fn mouseButtonCallback(
    //     self: *Surface,
    //     action: input.MouseButtonState,
    //     button: input.MouseButton,
    //     mods: input.Mods,
    // ) !bool {
    //
    // (the inferred `!bool` is the [`Error`] union.)
    pub fn mouseButtonCallback(
        &mut self,
        action: input_mouse::ButtonState,
        button: input_mouse::Button,
        mods: Mods,
    ) -> Result<bool, Error> {
        // Crash metadata in case we crash in here
        // crash.sentry.thread_state = self.crashThreadState();
        crash::sentry::setThreadState(self.crashThreadState());
        // defer crash.sentry.thread_state = null;
        // ([ADV 6.1] — the Drop guard clears at every exit below.)
        let _thread_state = ThreadStateGuard;

        // log.debug("mouse action={} button={} mods={}", .{ action, button, mods });
        // (COMMENT-ONLY BY DESIGN: commented out in the ORACLE too, Surface.zig:3798.)

        // If we have an inspector, we always queue a render
        // if (self.inspector != null) {
        //     defer self.queueRender() catch {};
        // }
        //
        // PORT NOTE: the reference `defer` is scoped to the `if` BLOCK, so it
        // fires at the block's closing brace — immediately.
        if self.inspector.is_some() {
            let _ = self.queueRender();
        }

        // Always record our latest mouse state
        // self.mouse.click_state[@intCast(@intFromEnum(button))] = action;
        self.mouse.click_state[button as usize] = action;

        // Always show the mouse again if it is hidden
        // if (self.mouse.hidden) self.showMouse();
        if self.mouse.hidden {
            self.showMouse();
        }

        // Update our modifiers if they changed
        // self.modsChanged(mods);
        self.modsChanged(mods);

        // This is set to true if the terminal is allowed to capture the shift
        // modifier. Note we can do this more efficiently probably with less
        // locking/unlocking but clicking isn't that frequent enough to be a
        // bottleneck.
        // const shift_capture = self.mouseShiftCapture(true);
        let shift_capture = self.mouseShiftCapture(true);

        // Shift-click continues the previous mouse state if we have a selection.
        // cursorPosCallback will also do a mouse report so we don't need to do any
        // of the logic below.
        // if (button == .left and action == .press) {
        //
        // PARITY: the reference deliberately splits the two `if`s ("more
        // readable as a human"); clippy's `collapsible_if` is allowed rather
        // than rewriting the control flow (the mouse_encode.rs precedent).
        #[allow(clippy::collapsible_if)]
        if button == input_mouse::Button::Left && action == input_mouse::ButtonState::Press {
            // We could do all the conditionals in one but I find it more
            // readable as a human to break this one up.
            // if (mods.shift and
            //     self.mouse.selection_gesture.left_click_count > 0 and
            //     !shift_capture)
            // extend_selection: {
            if mods.shift && self.mouse.selection_gesture.left_click_count > 0 && !shift_capture {
                'extend_selection: {
                    // We split this conditional out on its own because this is the
                    // only one that requires a renderer mutex grab which is VERY
                    // expensive because it could block all our threads.
                    // if (!self.hasSelection()) break :extend_selection;
                    if !self.hasSelection() {
                        break 'extend_selection;
                    }

                    // If we are within the interval that the click would register
                    // an increment then we do not extend the selection.
                    // if (std.time.Instant.now()) |now| {
                    //
                    // (Rust `Instant::now` is infallible — the `|now|` arm is
                    // live; the reference `else |err|` arm is preserved
                    // verbatim below.)
                    let now = std::time::Instant::now();
                    // const click_time = self.mouse.selection_gesture.left_click_time orelse
                    //     break :extend_selection;
                    let click_time = match self.mouse.selection_gesture.left_click_time {
                        Some(v) => v,
                        None => break 'extend_selection,
                    };
                    // const since = now.since(click_time);
                    //
                    // (`Instant.since` is nanoseconds — the selection_gesture
                    // `time_since` mapping.)
                    let since = now.duration_since(click_time).as_nanos() as u64;
                    // if (since <= self.config.mouse_interval) {
                    if since <= self.config.mouse_interval {
                        // Click interval very short, we may be increasing
                        // click counts so we don't extend the selection.
                        // break :extend_selection;
                        break 'extend_selection;
                    }
                    // } else |err| {
                    //     // This is a weird behavior, I think either behavior is actually
                    //     // fine. This failure should be exceptionally rare anyways.
                    //     // My thinking here is that we can't be sure if we should extend
                    //     // the selection or not so we just don't.
                    //     log.warn("failed to get time, not extending selection err={}", .{err});
                    //     break :extend_selection;
                    // }

                    // const pos = try self.rt_surface.getCursorPos();
                    let pos = unsafe { &*self.rt_surface }.get_cursor_pos()?;
                    // try self.cursorPosCallback(pos, null);
                    self.cursorPosCallback(pos, None)?;
                    // return true;
                    return Ok(true);
                }
            }
        }

        // if (button == .left and action == .release) {
        if button == input_mouse::Button::Left && action == input_mouse::ButtonState::Release {
            // self.renderer_state.mutex.lock();
            // defer self.renderer_state.mutex.unlock();
            //
            // (guard-less zigstd Mutex — the defer is the explicit unlock at
            // every exit below.)
            unsafe { (*self.renderer_state.mutex).lock() };

            // The selection gesture tracks whether a press became a drag by
            // comparing the release cell to the original press cell. Resolve the
            // release position and pin before notifying the gesture so later
            // release handling can query that state.
            // const release_pos: ?apprt.CursorPos = self.rt_surface.getCursorPos() catch |err| pos: {
            //     log.warn("error reading cursor position for mouse release err={}", .{err});
            //     break :pos null;
            // };
            //
            // PARITY: not `.ok()` — the Err arm IS the reference's labeled
            // `catch |err| pos:` block with its log site (the childExited
            // precedent).
            #[allow(clippy::manual_ok_err)]
            let release_pos: Option<apprt::CursorPos> =
                match unsafe { &*self.rt_surface }.get_cursor_pos() {
                    Ok(v) => Some(v),
                    Err(err) => {
                        // log.warn("error reading cursor position for mouse release err={}", .{err});
                        log::warn!(
                            target: LOG_SCOPE,
                            "error reading cursor position for mouse release err={err:?}"
                        );
                        None
                    }
                };

            // If we can't map the release position to a cell, pass null so the
            // gesture can conservatively treat the release as having moved away
            // from the pressed cell.
            // const release_pin: ?terminal.Pin = if (release_pos) |pos| pin: {
            //     const release_vp = self.posToViewport(pos.x, pos.y);
            //     break :pin self.io.terminal.screens.active.pages.pin(.{ .viewport = .{
            //         .x = release_vp.x,
            //         .y = release_vp.y,
            //     } });
            // } else null;
            let release_pin: Option<terminal::pagelist::Pin> =
                if let Some(pos) = release_pos {
                    let release_vp = self.posToViewport(f64::from(pos.x), f64::from(pos.y));
                    unsafe {
                        (*self.io.terminal.screens.active).pages.pin(
                            terminal::point::Point::Viewport(terminal::point::Coordinate {
                                x: release_vp.x,
                                y: release_vp.y,
                            }),
                        )
                    }
                } else {
                    None
                };
            // self.mouse.selection_gesture.release(
            //     self.renderer_state.terminal,
            //     .{ .pin = release_pin },
            // );
            self.mouse.selection_gesture.release(
                unsafe { &mut *self.renderer_state.terminal },
                terminal::selection_gesture::Release { pin: release_pin },
            );

            // Stop selection scrolling when releasing the left mouse button
            // but only when selection scrolling is active.
            // if (self.selection_scroll_active) {
            if self.selection_scroll_active {
                // self.queueIo(
                //     .{ .selection_scroll = false },
                //     .locked,
                // );
                self.queueIo(
                    termio::Message::SelectionScroll(false),
                    termio::termio_impl::MutexState::Locked,
                );
            }

            // The selection clipboard is only updated for left-click drag when
            // the left button is released. This is to avoid the clipboard
            // being updated on every mouse move which would be noisy.
            // if (self.config.copy_on_select != .none) {
            if self.config.copy_on_select != configpkg::value_types::CopyOnSelect::None {
                // const prev_ = self.io.terminal.screens.active.selection;
                let prev_ = unsafe { (*self.io.terminal.screens.active).selection };
                // if (prev_) |prev| {
                if let Some(prev) = prev_ {
                    // try self.setSelectionAndCopy(terminal.Selection.init(
                    //     prev.start(),
                    //     prev.end(),
                    //     prev.rectangle,
                    // ));
                    if let Err(err) =
                        self.setSelectionAndCopy(terminal::selection::Selection::init(
                            prev.start(),
                            prev.end(),
                            prev.rectangle,
                        ))
                    {
                        // (the deferred mutex unlock — the error exit)
                        unsafe { (*self.renderer_state.mutex).unlock() };
                        return Err(err);
                    }
                }
            }

            // Handle link clicking. We want to do this before we do mouse
            // reporting or any other mouse handling because a successfully
            // clicked link will swallow the event.
            // if (self.mouse.over_link and !self.mouse.selection_gesture.left_click_dragged) {
            if self.mouse.over_link && !self.mouse.selection_gesture.left_click_dragged {
                // We are holding the renderer lock, but this should just be
                // a cached value.
                // const pos = release_pos orelse try self.rt_surface.getCursorPos();
                let pos = match release_pos {
                    Some(v) => v,
                    None => match unsafe { &*self.rt_surface }.get_cursor_pos() {
                        Ok(v) => v,
                        Err(err) => {
                            // (the deferred mutex unlock — the error exit)
                            unsafe { (*self.renderer_state.mutex).unlock() };
                            return Err(err.into());
                        }
                    },
                };
                // if (self.processLinks(pos)) |processed| {
                //     if (processed) return true;
                // } else |err| {
                //     log.warn("error processing links err={}", .{err});
                // }
                match self.processLinks(pos) {
                    Ok(processed) => {
                        if processed {
                            // (the deferred mutex unlock — the consumed return)
                            unsafe { (*self.renderer_state.mutex).unlock() };
                            return Ok(true);
                        }
                    }
                    Err(err) => {
                        // log.warn("error processing links err={}", .{err});
                        log::warn!(target: LOG_SCOPE, "error processing links err={err:?}");
                    }
                }
            }

            // Handle prompt clicking. If we released our mouse on a prompt
            // and we support some kind of click events, then we need to
            // move to it.
            // if (self.maybePromptClick()) |handled| {
            //     if (handled) return true;
            // } else |err| {
            //     log.warn("error processing prompt click err={}", .{err});
            // }
            match self.maybePromptClick() {
                Ok(handled) => {
                    if handled {
                        // (the deferred mutex unlock — the handled return)
                        unsafe { (*self.renderer_state.mutex).unlock() };
                        return Ok(true);
                    }
                }
                Err(err) => {
                    // log.warn("error processing prompt click err={}", .{err});
                    log::warn!(target: LOG_SCOPE, "error processing prompt click err={err:?}");
                }
            }

            // (the deferred mutex unlock — the block end)
            unsafe { (*self.renderer_state.mutex).unlock() };
        }

        // Report mouse events if enabled
        {
            // self.renderer_state.mutex.lock();
            // defer self.renderer_state.mutex.unlock();
            //
            // (guard-less zigstd Mutex — the defer is the explicit unlock at
            // every exit below.)
            unsafe { (*self.renderer_state.mutex).lock() };
            // if (self.isMouseReporting()) report: {
            if self.isMouseReporting() {
                'report: {
                    // If we have shift-pressed and we aren't allowed to capture it,
                    // then we do not do a mouse report.
                    // if (mods.shift and !shift_capture) break :report;
                    if mods.shift && !shift_capture {
                        break 'report;
                    }

                    // In any other mouse button scenario without shift pressed we
                    // clear the selection since the underlying application can handle
                    // that in any way (i.e. "scrolling").
                    // try self.setSelection(null);
                    if let Err(err) = self.setSelection(None) {
                        // (the deferred mutex unlock — the error exit)
                        unsafe { (*self.renderer_state.mutex).unlock() };
                        return Err(err);
                    }

                    // We also set the left click count to 0 so that if mouse reporting
                    // is disabled in the middle of press (before release) we don't
                    // suddenly start selecting text.
                    // self.mouse.selection_gesture.reset(self.renderer_state.terminal);
                    self.mouse
                        .selection_gesture
                        .reset(unsafe { &mut *self.renderer_state.terminal });

                    // const pos = try self.rt_surface.getCursorPos();
                    let pos = match unsafe { &*self.rt_surface }.get_cursor_pos() {
                        Ok(v) => v,
                        Err(err) => {
                            // (the deferred mutex unlock — the error exit)
                            unsafe { (*self.renderer_state.mutex).unlock() };
                            return Err(err.into());
                        }
                    };

                    // const report_action: input.MouseAction = switch (action) {
                    //     .press => .press,
                    //     .release => .release,
                    // };
                    let report_action: input_mouse::Action = match action {
                        input_mouse::ButtonState::Press => input_mouse::Action::Press,
                        input_mouse::ButtonState::Release => input_mouse::Action::Release,
                    };

                    // self.mouseReport(
                    //     button,
                    //     report_action,
                    //     self.mouse.mods,
                    //     pos,
                    // );
                    self.mouseReport(Some(button), report_action, self.mouse.mods, pos);

                    // If we're doing mouse reporting, we do not support any other
                    // selection or highlighting.
                    // return true;
                    // (the deferred mutex unlock — the reporting return)
                    unsafe { (*self.renderer_state.mutex).unlock() };
                    return Ok(true);
                }
            }
            // (the deferred mutex unlock — the block end)
            unsafe { (*self.renderer_state.mutex).unlock() };
        }

        // For left button clicks we always record some information for
        // selection/highlighting purposes.
        // if (button == .left and action == .press) click: {
        if button == input_mouse::Button::Left && action == input_mouse::ButtonState::Press {
            'click: {
                // self.renderer_state.mutex.lock();
                // defer self.renderer_state.mutex.unlock();
                //
                // (guard-less zigstd Mutex — the defer is the explicit unlock
                // at every exit below.)
                unsafe { (*self.renderer_state.mutex).lock() };
                // const t: *terminal.Terminal = self.renderer_state.terminal;
                let t: *mut Terminal = self.renderer_state.terminal;
                // const screen: *terminal.Screen = self.renderer_state.terminal.screens.active;
                let screen: *mut terminal::screen::Screen =
                    unsafe { (*self.renderer_state.terminal).screens.active };

                // const pos = try self.rt_surface.getCursorPos();
                let pos = match unsafe { &*self.rt_surface }.get_cursor_pos() {
                    Ok(v) => v,
                    Err(err) => {
                        // (the deferred mutex unlock — the error exit)
                        unsafe { (*self.renderer_state.mutex).unlock() };
                        return Err(err.into());
                    }
                };
                // const pin = pin: {
                let pin = 'pin: {
                    // const pt_viewport = self.posToViewport(pos.x, pos.y);
                    let pt_viewport = self.posToViewport(f64::from(pos.x), f64::from(pos.y));
                    // const pin = screen.pages.pin(.{
                    //     .viewport = .{
                    //         .x = pt_viewport.x,
                    //         .y = pt_viewport.y,
                    //     },
                    // }) orelse {
                    let pin = match unsafe {
                        (*screen).pages.pin(terminal::point::Point::Viewport(
                            terminal::point::Coordinate {
                                x: pt_viewport.x,
                                y: pt_viewport.y,
                            },
                        ))
                    } {
                        Some(v) => v,
                        None => {
                            // Weird... our viewport x/y that we just converted isn't
                            // found in our pages. This is probably a bug but we don't
                            // want to crash in releases because its harmless. So, we
                            // only assert in debug mode.
                            // if (comptime std.debug.runtime_safety) unreachable;
                            if cfg!(debug_assertions) {
                                unreachable!();
                            }
                            // break :click;
                            // (the deferred mutex unlock — the break exit)
                            unsafe { (*self.renderer_state.mutex).unlock() };
                            break 'click;
                        }
                    };

                    // break :pin pin;
                    break 'pin pin;
                };

                // const time = std.time.Instant.now() catch |err| time: {
                //     log.err("error reading time, mouse multi-click won't work err={}", .{err});
                //     break :time null;
                // };
                //
                // (Rust `Instant::now` is infallible — the catch arm
                // collapses.)
                let time = Some(std::time::Instant::now());
                // var press_selection = try self.mouse.selection_gesture.press(t, .{
                //     .time = time,
                //     .pin = pin,
                //     .xpos = pos.x,
                //     .ypos = pos.y,
                //     .max_distance = @floatFromInt(self.size.cell.width),
                //     .repeat_interval = self.config.mouse_interval,
                //     .word_boundary_codepoints = self.config.selection_word_chars,
                //     .behaviors = &.{
                //         .cell,
                //         .word,
                //         if (mods.ctrlOrSuper()) .output else .line,
                //     },
                // });
                let mut press_selection = match self.mouse.selection_gesture.press(
                    unsafe { &mut *t },
                    terminal::selection_gesture::Press {
                        time,
                        pin,
                        xpos: f64::from(pos.x),
                        ypos: f64::from(pos.y),
                        max_distance: f64::from(self.size.cell.width),
                        repeat_interval: self.config.mouse_interval,
                        word_boundary_codepoints: &self.config.selection_word_chars,
                        behaviors: &[
                            terminal::selection_gesture::Behavior::Cell,
                            terminal::selection_gesture::Behavior::Word,
                            if mods.ctrl_or_super() {
                                terminal::selection_gesture::Behavior::Output
                            } else {
                                terminal::selection_gesture::Behavior::Line
                            },
                        ],
                    },
                ) {
                    Ok(v) => v,
                    Err(err) => {
                        // (the deferred mutex unlock — the error exit)
                        unsafe { (*self.renderer_state.mutex).unlock() };
                        return Err(err.into());
                    }
                };

                // The gesture owns the standard single/double/triple-click selection
                // behavior. Surface keeps terminal-surface-specific overrides here.
                // switch (self.mouse.selection_gesture.left_click_count) {
                match self.mouse.selection_gesture.left_click_count {
                    // 1 => {},
                    1 => {}

                    // Double click on a URL selects the entire URL instead of the
                    // standard word selection returned by the gesture.
                    // 2 => {
                    2 => {
                        // Try link detection without requiring modifier keys.
                        // if (self.linkAtPin(
                        //     pin,
                        //     null,
                        // )) |result_| {
                        match self.linkAtPin(pin, None) {
                            Ok(result_) => {
                                // if (result_) |result| {
                                if let Some(result) = result_ {
                                    // press_selection = result.selection;
                                    press_selection = Some(result.selection);
                                }
                            }
                            // } else |_| {
                            Err(_) => {
                                // Ignore any errors, likely regex errors.
                            }
                        }
                    }

                    // 3 => {},
                    3 => {}

                    // We should be bounded by 1 to 3
                    // else => unreachable,
                    _ => unreachable!(),
                }

                // Use `setSelection` (not `setSelectionAndCopy`) here to avoid
                // touching the selection clipboard: for left mouse clicks we only
                // copy on release.
                // if (press_selection) |selection| {
                if let Some(selection) = press_selection {
                    // try self.setSelection(selection);
                    if let Err(err) = self.setSelection(Some(selection)) {
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
                // } else if (self.mouse.selection_gesture.left_click_count == 1 and
                //     self.io.terminal.screens.active.selection != null)
                // {
                } else if self.mouse.selection_gesture.left_click_count == 1
                    && unsafe { (*self.io.terminal.screens.active).selection }.is_some()
                {
                    // try self.setSelection(null);
                    if let Err(err) = self.setSelection(None) {
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
                }

                // (the deferred mutex unlock — the block end)
                unsafe { (*self.renderer_state.mutex).unlock() };
            }
        }

        // Middle-click action, either ignore, or paste from clipboard or paste from the selection clipboard if supported.
        // if (button == .middle and action == .press) switch (self.config.middle_click_action) {
        if button == input_mouse::Button::Middle && action == input_mouse::ButtonState::Press {
            match self.config.middle_click_action {
                // .ignore => {},
                configpkg::value_types::MiddleClickAction::Ignore => {}
                // .@"clipboard-paste" => {
                //     _ = try self.startClipboardRequest(.standard, .{ .paste = .standard });
                // },
                configpkg::value_types::MiddleClickAction::ClipboardPaste => {
                    let _ = self.startClipboardRequest(
                        apprt::Clipboard::Standard,
                        apprt::ClipboardRequest::Paste(apprt::Clipboard::Standard),
                    )?;
                }
                // .@"primary-paste" => {
                //     if (self.rt_surface.supportsClipboard(.selection)) {
                //         _ = try self.startClipboardRequest(.selection, .{ .paste = .selection });
                //     }
                // },
                configpkg::value_types::MiddleClickAction::PrimaryPaste => {
                    if unsafe { &*self.rt_surface }.supports_clipboard(apprt::Clipboard::Selection)
                    {
                        let _ = self.startClipboardRequest(
                            apprt::Clipboard::Selection,
                            apprt::ClipboardRequest::Paste(apprt::Clipboard::Selection),
                        )?;
                    }
                }
            }
        }

        // Right-click down selects word for context menus. If the apprt
        // doesn't implement context menus this can be a bit weird but they
        // are supported by our two main apprts so we always do this. If we
        // want to be careful in the future we can add a function to apprts
        // that let's us know.
        // if (button == .right and action == .press) sel: {
        if button == input_mouse::Button::Right && action == input_mouse::ButtonState::Press {
            'sel: {
                // self.renderer_state.mutex.lock();
                // defer self.renderer_state.mutex.unlock();
                //
                // (guard-less zigstd Mutex — the defer is the explicit unlock
                // at every exit below.)
                unsafe { (*self.renderer_state.mutex).lock() };

                // Get our viewport pin
                // const screen: *terminal.Screen = self.renderer_state.terminal.screens.active;
                let screen: *mut terminal::screen::Screen =
                    unsafe { (*self.renderer_state.terminal).screens.active };
                // const pos = try self.rt_surface.getCursorPos();
                let pos = match unsafe { &*self.rt_surface }.get_cursor_pos() {
                    Ok(v) => v,
                    Err(err) => {
                        // (the deferred mutex unlock — the error exit)
                        unsafe { (*self.renderer_state.mutex).unlock() };
                        return Err(err.into());
                    }
                };
                // const pin = pin: {
                let pin = 'pin: {
                    // const pt_viewport = self.posToViewport(pos.x, pos.y);
                    let pt_viewport = self.posToViewport(f64::from(pos.x), f64::from(pos.y));
                    // const pin = screen.pages.pin(.{
                    //     .viewport = .{
                    //         .x = pt_viewport.x,
                    //         .y = pt_viewport.y,
                    //     },
                    // }) orelse {
                    let pin = match unsafe {
                        (*screen).pages.pin(terminal::point::Point::Viewport(
                            terminal::point::Coordinate {
                                x: pt_viewport.x,
                                y: pt_viewport.y,
                            },
                        ))
                    } {
                        Some(v) => v,
                        None => {
                            // if (comptime std.debug.runtime_safety) unreachable;
                            if cfg!(debug_assertions) {
                                unreachable!();
                            }
                            // break :sel;
                            // (the deferred mutex unlock — the break exit)
                            unsafe { (*self.renderer_state.mutex).unlock() };
                            break 'sel;
                        }
                    };

                    // break :pin pin;
                    break 'pin pin;
                };

                // switch (self.config.right_click_action) {
                match self.config.right_click_action {
                    // .ignore => {},
                    configpkg::value_types::RightClickAction::Ignore => {}
                    // .@"context-menu" => {
                    configpkg::value_types::RightClickAction::ContextMenu => {
                        // If we already have a selection and the selection contains
                        // where we clicked then we don't want to modify the selection.
                        // if (self.io.terminal.screens.active.selection) |prev_sel| {
                        if let Some(prev_sel) =
                            unsafe { (*self.io.terminal.screens.active).selection }
                        {
                            // if (prev_sel.contains(screen, pin)) break :sel;
                            if prev_sel.contains(unsafe { &*screen }, pin) {
                                // (the deferred mutex unlock — the break exit)
                                unsafe { (*self.renderer_state.mutex).unlock() };
                                break 'sel;
                            }

                            // The selection doesn't contain our pin, so we create a new
                            // word selection where we clicked.
                        }

                        // If there is a link at this position, we want to
                        // select the link. Otherwise, select the word.
                        // if (try self.linkAtPos(pos)) |link| {
                        let link_ = match self.linkAtPos(pos) {
                            Ok(v) => v,
                            Err(err) => {
                                // (the deferred mutex unlock — the error exit)
                                unsafe { (*self.renderer_state.mutex).unlock() };
                                return Err(err);
                            }
                        };
                        if let Some(link) = link_ {
                            // try self.setSelectionAndCopy(link.selection);
                            if let Err(err) = self.setSelectionAndCopy(link.selection) {
                                // (the deferred mutex unlock — the error exit)
                                unsafe { (*self.renderer_state.mutex).unlock() };
                                return Err(err);
                            }
                        } else {
                            // const sel = screen.selectWord(
                            //     pin,
                            //     self.config.selection_word_chars,
                            // ) orelse break :sel;
                            let sel = match unsafe { &*screen }
                                .select_word(pin, &self.config.selection_word_chars)
                            {
                                Some(v) => v,
                                None => {
                                    // (the deferred mutex unlock — the break
                                    // exit)
                                    unsafe { (*self.renderer_state.mutex).unlock() };
                                    break 'sel;
                                }
                            };
                            // try self.setSelectionAndCopy(sel);
                            if let Err(err) = self.setSelectionAndCopy(sel) {
                                // (the deferred mutex unlock — the error exit)
                                unsafe { (*self.renderer_state.mutex).unlock() };
                                return Err(err);
                            }
                        }
                        // try self.queueRender();
                        if let Err(err) = self.queueRender() {
                            // (the deferred mutex unlock — the error exit)
                            unsafe { (*self.renderer_state.mutex).unlock() };
                            return Err(err.into());
                        }

                        // Don't consume so that we show the context menu in apprt.
                        // return false;
                        // (the deferred mutex unlock — the context-menu return)
                        unsafe { (*self.renderer_state.mutex).unlock() };
                        return Ok(false);
                    }
                    // .copy => {
                    configpkg::value_types::RightClickAction::Copy => {
                        // if (self.io.terminal.screens.active.selection) |sel| {
                        if let Some(sel) = unsafe { (*self.io.terminal.screens.active).selection } {
                            // try self.copySelectionToClipboards(
                            //     sel,
                            //     &.{.standard},
                            //     .mixed,
                            // );
                            if let Err(err) = self.copySelectionToClipboards(
                                sel,
                                &[apprt::Clipboard::Standard],
                                binding::CopyToClipboard::Mixed,
                            ) {
                                // (the deferred mutex unlock — the error exit)
                                unsafe { (*self.renderer_state.mutex).unlock() };
                                return Err(err);
                            }
                        }

                        // try self.setSelection(null);
                        if let Err(err) = self.setSelection(None) {
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
                    }
                    // .@"copy-or-paste" => if (self.io.terminal.screens.active.selection) |sel| {
                    configpkg::value_types::RightClickAction::CopyOrPaste => {
                        if let Some(sel) = unsafe { (*self.io.terminal.screens.active).selection } {
                            // try self.copySelectionToClipboards(
                            //     sel,
                            //     &.{.standard},
                            //     .mixed,
                            // );
                            if let Err(err) = self.copySelectionToClipboards(
                                sel,
                                &[apprt::Clipboard::Standard],
                                binding::CopyToClipboard::Mixed,
                            ) {
                                // (the deferred mutex unlock — the error exit)
                                unsafe { (*self.renderer_state.mutex).unlock() };
                                return Err(err);
                            }
                            // try self.setSelection(null);
                            if let Err(err) = self.setSelection(None) {
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
                        } else {
                            // Pasting can trigger a lock grab in complete clipboard
                            // request so we need to unlock.
                            // self.renderer_state.mutex.unlock();
                            unsafe { (*self.renderer_state.mutex).unlock() };
                            // defer self.renderer_state.mutex.lock();
                            //
                            // (the inverted defer — the explicit relock at
                            // every exit of this arm below.)
                            // _ = try self.startClipboardRequest(.standard, .{ .paste = .standard });
                            match self.startClipboardRequest(
                                apprt::Clipboard::Standard,
                                apprt::ClipboardRequest::Paste(apprt::Clipboard::Standard),
                            ) {
                                Ok(_) => {}
                                Err(err) => {
                                    // (the LIFO defers on the error exit —
                                    // the arm's relock, then the outer
                                    // unlock)
                                    unsafe { (*self.renderer_state.mutex).lock() };
                                    unsafe { (*self.renderer_state.mutex).unlock() };
                                    return Err(err);
                                }
                            }
                            // (the deferred relock — the arm end)
                            unsafe { (*self.renderer_state.mutex).lock() };

                            // We don't need to clear selection because we didn't have
                            // one to begin with.
                        }
                    }
                    // .paste => {
                    configpkg::value_types::RightClickAction::Paste => {
                        // Before we yield the lock, clear our selection if we have
                        // one.
                        // try self.setSelection(null);
                        if let Err(err) = self.setSelection(None) {
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

                        // Pasting can trigger a lock grab in complete clipboard
                        // request so we need to unlock.
                        // self.renderer_state.mutex.unlock();
                        unsafe { (*self.renderer_state.mutex).unlock() };
                        // defer self.renderer_state.mutex.lock();
                        //
                        // (the inverted defer — the explicit relock at every
                        // exit of this arm below.)
                        // _ = try self.startClipboardRequest(.standard, .{ .paste = .standard });
                        match self.startClipboardRequest(
                            apprt::Clipboard::Standard,
                            apprt::ClipboardRequest::Paste(apprt::Clipboard::Standard),
                        ) {
                            Ok(_) => {}
                            Err(err) => {
                                // (the LIFO defers on the error exit — the
                                // arm's relock, then the outer unlock)
                                unsafe { (*self.renderer_state.mutex).lock() };
                                unsafe { (*self.renderer_state.mutex).unlock() };
                                return Err(err);
                            }
                        }
                        // (the deferred relock — the arm end)
                        unsafe { (*self.renderer_state.mutex).lock() };
                    }
                }

                // Consume the event such that the context menu is not displayed.
                // return true;
                // (the deferred mutex unlock — the consumed return)
                unsafe { (*self.renderer_state.mutex).unlock() };
                return Ok(true);
            }
        }

        // return false;
        Ok(false)
    }

    /// Requires the renderer state mutex is held.
    // fn maybePromptClick(self: *Surface) !bool {
    //
    // (the inferred `!bool` is the [`Error`] union.)
    fn maybePromptClick(&mut self) -> Result<bool, Error> {
        // const t: *terminal.Terminal = self.renderer_state.terminal;
        let t: *mut Terminal = self.renderer_state.terminal;
        // const screen: *terminal.Screen = t.screens.active;
        let screen: *mut terminal::screen::Screen = unsafe { (*t).screens.active };

        // If our screen doesn't handle any prompt clicks, then we never
        // do anything.
        // if (screen.semantic_prompt.click == .none) return false;
        if unsafe { (*screen).semantic_prompt.click } == terminal::screen::SemanticClick::None {
            return Ok(false);
        }

        // If cursor-click-to-move is disabled, we don't do any prompt clicking.
        // if (!self.config.cursor_click_to_move) return false;
        if !self.config.cursor_click_to_move {
            return Ok(false);
        }

        // If our cursor isn't currently at a prompt then we don't handle
        // prompt clicks because we can't move if we're not in a prompt!
        // if (!t.cursorIsAtPrompt()) return false;
        if !unsafe { (*t).cursor_is_at_prompt() } {
            return Ok(false);
        }

        // If the left click moved away from its pressed cell then releasing the
        // mouse completes the drag gesture and we don't do prompt moving.
        // if (self.mouse.selection_gesture.left_click_dragged) return false;
        if self.mouse.selection_gesture.left_click_dragged {
            return Ok(false);
        }

        // If we have a selection currently, then releasing the mouse completes
        // the selection and we don't do prompt moving.
        // if (screen.selection != null) return false;
        if unsafe { (*screen).selection }.is_some() {
            return Ok(false);
        }

        // Get the pin for our mouse click.
        // const pos = try self.rt_surface.getCursorPos();
        let pos = unsafe { &*self.rt_surface }.get_cursor_pos()?;
        // const pos_vp = self.posToViewport(pos.x, pos.y);
        let pos_vp = self.posToViewport(f64::from(pos.x), f64::from(pos.y));
        // const click_pin: terminal.Pin = pin: {
        let click_pin: terminal::pagelist::Pin = 'pin: {
            // const pin = screen.pages.pin(.{
            //     .viewport = .{
            //         .x = pos_vp.x,
            //         .y = pos_vp.y,
            //     },
            // }) orelse {
            let pin = match unsafe {
                (*screen).pages.pin(terminal::point::Point::Viewport(
                    terminal::point::Coordinate {
                        x: pos_vp.x,
                        y: pos_vp.y,
                    },
                ))
            } {
                Some(v) => v,
                None => {
                    // See mouseButtonCallback for explanation
                    // if (comptime std.debug.runtime_safety) unreachable;
                    if cfg!(debug_assertions) {
                        unreachable!();
                    }
                    // return false;
                    return Ok(false);
                }
            };

            // break :pin pin;
            break 'pin pin;
        };

        // Get our cursor's most current prompt.
        // const prompt_pin: terminal.Pin = prompt_pin: {
        let prompt_pin: terminal::pagelist::Pin = 'prompt_pin: {
            // var it = screen.cursor.page_pin.promptIterator(
            //     .left_up,
            //     null,
            // );
            let mut it = unsafe {
                (*(*screen).cursor.page_pin)
                    .prompt_iterator(terminal::pagelist::iter::Direction::LeftUp, None)
            };
            // break :prompt_pin it.next() orelse {
            match it.next() {
                Some(v) => break 'prompt_pin v,
                None => {
                    // This shouldn't be possible because we asserted we're at
                    // a prompt above, so we MUST find some prompt in a left_up search.
                    // log.warn("cursor is at prompt but no prompt found", .{});
                    log::warn!(target: LOG_SCOPE, "cursor is at prompt but no prompt found");
                    // if (comptime std.debug.runtime_safety) unreachable;
                    if cfg!(debug_assertions) {
                        unreachable!();
                    }
                    // return false;
                    return Ok(false);
                }
            }
        };

        // If our mouse click is before the prompt, we don't move.
        // We DO ALLOW clicks AFTER the prompt, specifically with Kitty's
        // click_events=1 since we rely on the shell to validate out of
        // bounds clicks. This matches Kitty's logic as best I can tell.
        // if (click_pin.before(prompt_pin)) return false;
        if click_pin.before(prompt_pin) {
            return Ok(false);
        }

        // At this point we've established:
        // - Screen supports prompt clicks
        // - Cursor is at a prompt
        // - Click is at or below our prompt
        // switch (screen.semantic_prompt.click) {
        match unsafe { (*screen).semantic_prompt.click } {
            // Guarded at the start of this function
            // .none => unreachable,
            terminal::screen::SemanticClick::None => unreachable!(),

            // .click_events => |v| {
            terminal::screen::SemanticClick::ClickEvents(v) => {
                // For the event, we always send a left-click press event.
                // This matches what Kitty sends.
                // const key: u8, const y: u32 = switch (v) {
                //     .absolute => .{ 1, pos_vp.y +| 1 },
                //     .relative => .{ 2, pos_vp.y -| prompt_pin.y +| 1 },
                // };
                let (key, y): (u8, u32) = match v {
                    terminal::osc::semantic_prompt::ClickEvents::Absolute => {
                        (1, pos_vp.y.saturating_add(1))
                    }
                    terminal::osc::semantic_prompt::ClickEvents::Relative => (
                        2,
                        pos_vp
                            .y
                            .saturating_sub(u32::from(prompt_pin.y))
                            .saturating_add(1),
                    ),
                };
                // var data: termio.Message.WriteReq.Small.Array = undefined;
                // const resp = try std.fmt.bufPrint(
                //     &data,
                //     "\x1B[<0;{d};{d}M",
                //     .{ pos_vp.x + 1, y },
                // );
                //
                // PORT NOTE: the fixed Cursor over the Small array is
                // `bufPrint` (the encodeKey precedent); its buffer-full error
                // is io's `WriteZero` — the reference's `error.NoSpaceLeft`,
                // `try`-propagated ([`Error::Io`]).
                let mut data = [0u8; termio::message::WriteReq::MAX];
                let resp_len = {
                    use std::io::Write as _;
                    let mut writer = std::io::Cursor::new(&mut data[..]);
                    write!(writer, "\x1B[<0;{};{}M", pos_vp.x + 1, y)?;
                    writer.position() as usize
                };

                // Not that noisy since this only happens on prompt clicks.
                // log.debug(
                //     "sending click_events={} event=ESC{s}",
                //     .{ key, resp[1..] },
                // );
                log::debug!(
                    target: LOG_SCOPE,
                    "sending click_events={} event=ESC{}",
                    key,
                    String::from_utf8_lossy(&data[1..resp_len])
                );

                // Ask our IO thread to write the data
                // self.queueIo(.{ .write_small = .{
                //     .data = data,
                //     .len = @intCast(resp.len),
                // } }, .locked);
                self.queueIo(
                    termio::Message::WriteSmall(Small {
                        data,
                        len: resp_len,
                    }),
                    termio::termio_impl::MutexState::Locked,
                );
            }

            // .cl => {
            terminal::screen::SemanticClick::Cl(_) => {
                // const left_arrow = if (t.modes.get(.cursor_keys)) "\x1bOD" else "\x1b[D";
                let left_arrow: &'static [u8] =
                    if unsafe { (*t).modes.get(terminal::modes::Mode::CursorKeys) } {
                        b"\x1bOD"
                    } else {
                        b"\x1b[D"
                    };
                // const right_arrow = if (t.modes.get(.cursor_keys)) "\x1bOC" else "\x1b[C";
                let right_arrow: &'static [u8] =
                    if unsafe { (*t).modes.get(terminal::modes::Mode::CursorKeys) } {
                        b"\x1bOC"
                    } else {
                        b"\x1b[C"
                    };

                // const move = screen.promptClickMove(click_pin);
                //
                // (`move` is a Rust keyword — the raw identifier keeps the
                // reference name.)
                let r#move = unsafe { (*screen).prompt_click_move(click_pin) };
                // for (0..move.left) |_| {
                for _ in 0..r#move.left {
                    // self.queueIo(
                    //     .{ .write_stable = left_arrow },
                    //     .locked,
                    // );
                    self.queueIo(
                        termio::Message::WriteStable(left_arrow),
                        termio::termio_impl::MutexState::Locked,
                    );
                }
                // for (0..move.right) |_| {
                for _ in 0..r#move.right {
                    // self.queueIo(
                    //     .{ .write_stable = right_arrow },
                    //     .locked,
                    // );
                    self.queueIo(
                        termio::Message::WriteStable(right_arrow),
                        termio::termio_impl::MutexState::Locked,
                    );
                }
            }
        }

        // return true;
        Ok(true)
    }

    // (the file-scope `const Link` the reference declares HERE, Surface.zig:
    // 4271, lives in this file's trailing type cluster — see [`Link`].)

    /// Returns the link at the given cursor position, if any.
    ///
    /// Requires the renderer mutex is held.
    // fn linkAtPos(
    //     self: *Surface,
    //     pos: apprt.CursorPos,
    // ) !?Link {
    //
    // (the inferred `!?Link` is the [`Error`] union.)
    fn linkAtPos(&mut self, pos: apprt::CursorPos) -> Result<Option<Link>, Error> {
        // Convert our cursor position to a screen point.
        // const screen: *terminal.Screen = self.renderer_state.terminal.screens.active;
        let screen: *mut terminal::screen::Screen =
            unsafe { (*self.renderer_state.terminal).screens.active };
        // const mouse_pin: terminal.Pin = mouse_pin: {
        let mouse_pin: terminal::pagelist::Pin = 'mouse_pin: {
            // const point = self.posToViewport(pos.x, pos.y);
            //
            // (Zig widens the f32 cursor coordinates to the f64 params.)
            let point = self.posToViewport(f64::from(pos.x), f64::from(pos.y));
            // const pin = screen.pages.pin(.{ .viewport = point }) orelse {
            //     log.warn("failed to get pin for clicked point", .{});
            //     return null;
            // };
            let pin = match unsafe { (*screen).pages.pin(terminal::point::Point::Viewport(point)) }
            {
                Some(v) => v,
                None => {
                    // log.warn("failed to get pin for clicked point", .{});
                    log::warn!(target: LOG_SCOPE, "failed to get pin for clicked point");
                    return Ok(None);
                }
            };
            // break :mouse_pin pin;
            break 'mouse_pin pin;
        };

        // Get our comparison mods
        // const mouse_mods = self.mouseModsWithCapture(self.mouse.mods);
        let mouse_mods = self.mouseModsWithCapture(self.mouse.mods);

        // If we have the proper modifiers set then we can check for OSC8 links.
        // if (self.config.link_osc8 and
        //     mouse_mods.equal(input.ctrlOrSuper(.{})))
        // hyperlink: {
        if self.config.link_osc8 && mouse_mods.equal(key::ctrl_or_super(Mods::default())) {
            'hyperlink: {
                // const rac = mouse_pin.rowAndCell();
                let rac = mouse_pin.row_and_cell();
                // const cell = rac.cell;
                let cell = rac.cell;
                // if (!cell.hyperlink) break :hyperlink;
                if !unsafe { (*cell).hyperlink() } {
                    break 'hyperlink;
                }
                // const sel = terminal.Selection.init(mouse_pin, mouse_pin, false);
                let sel = terminal::selection::Selection::init(mouse_pin, mouse_pin, false);
                // return .{ .action = ._open_osc8, .selection = sel };
                return Ok(Some(Link {
                    action: link::Action::OpenOsc8,
                    selection: sel,
                }));
            }
        }

        // Fall back to configured links
        // return try self.linkAtPin(mouse_pin, mouse_mods);
        //
        // (the non-optional `mouse_mods` coerces to the `?input.Mods` param.)
        self.linkAtPin(mouse_pin, Some(mouse_mods))
    }

    /// Detects if a link is present at the given pin.
    ///
    /// If mouse mods is null then mouse mod requirements are ignored (all
    /// configured links are checked).
    ///
    /// Requires the renderer state mutex is held.
    // fn linkAtPin(
    //     self: *Surface,
    //     mouse_pin: terminal.Pin,
    //     mouse_mods: ?input.Mods,
    // ) !?Link {
    //
    // (the inferred `!?Link` is the [`Error`] union — the `it.next()` regex
    // search is its `Oni` growth point.)
    fn linkAtPin(
        &mut self,
        mouse_pin: terminal::pagelist::Pin,
        mouse_mods: Option<Mods>,
    ) -> Result<Option<Link>, Error> {
        // if (self.config.links.len == 0) return null;
        if self.config.links.is_empty() {
            return Ok(None);
        }

        // const screen: *terminal.Screen = self.renderer_state.terminal.screens.active;
        let screen: *mut terminal::screen::Screen =
            unsafe { (*self.renderer_state.terminal).screens.active };
        // const line = screen.selectLine(.{
        //     .pin = mouse_pin,
        //     .whitespace = null,
        //     // Respect semantic prompt boundaries so link/path matching doesn't
        //     // merge shell prompt content with the text beside it.
        //     .semantic_prompt_boundary = true,
        // }) orelse return null;
        let line = match unsafe { &*screen }.select_line(terminal::screen::SelectLine {
            pin: mouse_pin,
            whitespace: None,
            // Respect semantic prompt boundaries so link/path matching doesn't
            // merge shell prompt content with the text beside it.
            semantic_prompt_boundary: true,
        }) {
            Some(v) => v,
            None => return Ok(None),
        };

        // const strmap = try screen.selectionStringMap(self.alloc, .{
        //     .sel = line,
        //     .trim = false,
        // });
        let mut strmap: terminal::string_map::StringMap = unsafe { &mut *screen }
            .selection_string_map(
                self.alloc,
                terminal::screen::SelectionString {
                    sel: line,
                    trim: false,
                },
            )?;
        // defer strmap.deinit(self.alloc);
        //
        // PORT NOTE: the owned map frees on drop at every exit below — either
        // as the local itself or moved into the live iterator (see the
        // searchIterator note in the loop).

        // for (self.config.links) |link| {
        for link in &self.config.links {
            // Skip highlight/mods check when mouse_mods is null (double-click mode)
            // if (mouse_mods) |mods| switch (link.highlight) {
            //     .always, .hover => {},
            //     .always_mods, .hover_mods => |v| if (!v.equal(mods)) continue,
            // };
            if let Some(mods) = mouse_mods {
                match link.highlight {
                    link::Highlight::Always | link::Highlight::Hover => {}
                    link::Highlight::AlwaysMods(v) | link::Highlight::HoverMods(v) => {
                        if !v.equal(mods) {
                            continue;
                        }
                    }
                }
            }

            // var it = strmap.searchIterator(link.regex);
            //
            // PORT NOTE (Zig value-copies): the reference `searchIterator`
            // copies BOTH the `StringMap` slice headers and the `oni.Regex`
            // handle by value — shallow, non-owning copies aliasing the same
            // backing data. The vt `search_iterator` takes both by value, so
            // the regex copy is the aliasing handle struct (`link.regex`
            // stays owned by `DerivedConfig`; only its explicit deinit chain
            // frees the OnigRegex — the struct has no `Drop`) and the map
            // moves through the iterator, recovered after each link's scan.
            let mut it = strmap.search_iterator(oni::Regex {
                value: link.regex.value,
            });
            // while (true) {
            //
            // PARITY: clippy's `while_let_loop` fold would hide the
            // reference's explicit `while (true)` + `orelse break` control
            // flow (the vt string_map.rs precedent, verbatim).
            #[allow(clippy::while_let_loop)]
            loop {
                // var match = (try it.next()) orelse break;
                let mut match_ = match it.next()? {
                    Some(v) => v,
                    None => break,
                };
                // defer match.deinit();
                //
                // (explicit at every exit of this iteration below.)
                // const sel = match.selection();
                let sel = match_.selection();
                // if (!sel.contains(screen, mouse_pin)) continue;
                if !sel.contains(unsafe { &*screen }, mouse_pin) {
                    // (the deferred match.deinit — the continue)
                    match_.deinit();
                    continue;
                }
                // return .{
                //     .action = link.action,
                //     .selection = sel,
                // };
                //
                // (the deferred match.deinit — the return)
                match_.deinit();
                return Ok(Some(Link {
                    action: link.action,
                    selection: sel,
                }));
            }
            // (the map moves back out of the finished iterator — the
            // searchIterator PORT NOTE above.)
            strmap = it.map;
        }

        // return null;
        Ok(None)
    }

    /// This returns the mouse mods to consider for link highlighting or
    /// other purposes taking into account when shift is pressed for releasing
    /// the mouse from capture.
    ///
    /// The renderer state mutex must be held.
    // fn mouseModsWithCapture(self: *Surface, mods: input.Mods) input.Mods {
    fn mouseModsWithCapture(&mut self, mods: Mods) -> Mods {
        // In any of these scenarios, whatever mods are set (even shift)
        // are preserved.
        // if (self.io.terminal.flags.mouse_event == .none) return mods;
        if self.io.terminal.flags.mouse_event == terminal::mouse::Event::None {
            return mods;
        }
        // if (!mods.shift) return mods;
        if !mods.shift {
            return mods;
        }
        // if (self.mouseShiftCapture(false)) return mods;
        if self.mouseShiftCapture(false) {
            return mods;
        }

        // We have mouse capture, shift set, and we're not allowed to capture
        // shift, so we can clear shift.
        // var final = mods;
        //
        // (`final` is a Rust reserved keyword — the raw identifier keeps the
        // reference name, the `r#move` precedent.)
        let mut r#final = mods;
        // final.shift = false;
        r#final.shift = false;
        // return final;
        r#final
    }

    /// Attempt to invoke the action of any link that is under the
    /// given position.
    ///
    /// Requires the renderer state mutex is held.
    // fn processLinks(self: *Surface, pos: apprt.CursorPos) !bool {
    fn processLinks(&mut self, pos: apprt::CursorPos) -> Result<bool, Error> {
        // const link = try self.linkAtPos(pos) orelse return false;
        let link = match self.linkAtPos(pos)? {
            Some(v) => v,
            None => return Ok(false),
        };
        // switch (link.action) {
        match link.action {
            // .open => {
            link::Action::Open => {
                // const str = try self.io.terminal.screens.active.selectionString(self.alloc, .{
                //     .sel = link.selection,
                //     .trim = false,
                // });
                let str_ = unsafe { &mut *self.io.terminal.screens.active }.selection_string(
                    self.alloc,
                    terminal::screen::SelectionString {
                        sel: link.selection,
                        trim: false,
                    },
                )?;
                // defer self.alloc.free(str);
                //
                // (owned bytes — freed on drop at every exit.)

                // const resolved_path = try self.resolvePathForOpening(str);
                let resolved_path = self.resolvePathForOpening(&str_);
                // defer if (resolved_path) |p| self.alloc.free(p);
                //
                // (owned bytes — freed on drop at every exit.)

                // const url_to_open = resolved_path orelse str;
                let url_to_open: &[u8] = match &resolved_path {
                    Some(p) => p,
                    None => &str_,
                };
                // try self.openUrl(.{ .kind = .unknown, .url = url_to_open });
                self.openUrl(OpenUrl {
                    kind: OpenUrlKind::Unknown,
                    url: url_to_open,
                })?;
            }

            // ._open_osc8 => {
            link::Action::OpenOsc8 => {
                // const uri = self.osc8URI(link.selection.start()) orelse {
                //     log.warn("failed to get URI for OSC8 hyperlink", .{});
                //     return false;
                // };
                let uri = match self.osc8URI(link.selection.start()) {
                    Some(v) => v,
                    None => {
                        // log.warn("failed to get URI for OSC8 hyperlink", .{});
                        log::warn!(target: LOG_SCOPE, "failed to get URI for OSC8 hyperlink");
                        return Ok(false);
                    }
                };
                // try self.openUrl(.{ .kind = .osc8, .url = uri });
                self.openUrl(OpenUrl {
                    kind: OpenUrlKind::Osc8,
                    url: uri,
                })?;
            }
        }

        // return true;
        Ok(true)
    }

    // fn openUrl(
    //     self: *Surface,
    //     action: apprt.action.OpenUrl,
    // ) !void {
    //
    // (the inferred `!void` is the [`Error`] union — the fallback opener is
    // its `OsOpen` growth point.)
    fn openUrl(&mut self, action: OpenUrl<'_>) -> Result<(), Error> {
        // If the apprt handles it then we're done.
        // if (try self.rt_app.performAction(
        //     .{ .surface = self },
        //     .open_url,
        //     action,
        // )) return;
        if unsafe { &*self.rt_app }.perform_action(
            apprt::Target::Surface(&*self),
            apprt::Action::OpenUrl(action),
        )? {
            return Ok(());
        }

        // apprt didn't handle it, fallback to our simple cross-platform
        // URL opener. We log a warning because we want well-behaved
        // apprts to handle this themselves.
        // log.warn("apprt did not handle open URL action, falling back to default opener", .{});
        log::warn!(
            target: LOG_SCOPE,
            "apprt did not handle open URL action, falling back to default opener"
        );
        // try internal_os.open(
        //     self.alloc,
        //     action.kind,
        //     action.url,
        // );
        //
        // PORT NOTE: the ported `internal_os::open` collapses the allocator
        // param (Rust `Command` ownership) and takes the os-local R4-seam
        // `Kind` — the parity image of `apprt.action.OpenUrl.Kind` (the
        // os/open.rs break-#14 note) — so the enum maps arm-for-arm.
        internal_os::open::open(
            match action.kind {
                OpenUrlKind::Unknown => internal_os::open::Kind::Unknown,
                OpenUrlKind::Text => internal_os::open::Kind::Text,
                OpenUrlKind::Html => internal_os::open::Kind::Html,
                // (`77537c806`: the os-side `Kind::Osc8` arm fails closed on
                // macOS — apprt owns the OSC 8 safe-opening policy.)
                OpenUrlKind::Osc8 => internal_os::open::Kind::Osc8,
            },
            action.url,
        )?;
        Ok(())
    }

    /// Return the URI for an OSC8 hyperlink at the given position or null
    /// if there is no hyperlink.
    // fn osc8URI(self: *Surface, pin: terminal.Pin) ?[]const u8 {
    //
    // PORT NOTE: the returned slice borrows the pin's PAGE memory (the
    // reference `entry.uri.slice(page.memory)`), NOT `self` — the body
    // discards `self` (`_ = self`). The explicit `'a` states that contract
    // (the raw-ptr discipline; vt's `OffsetSlice::slice` is itself
    // caller-lifetime): the bytes are valid while the renderer state mutex is
    // held and the page node is alive — exactly the reference's implicit
    // contract. Raw bytes (`&[u8]`), never a lossy String.
    //
    // PARITY: clippy's `question_mark` fold of the `orelse return null` match
    // is allowed rather than rewriting the reference's explicit early return
    // (the collapsible_if precedent).
    #[allow(clippy::question_mark)]
    fn osc8URI<'a>(&self, pin: terminal::pagelist::Pin) -> Option<&'a [u8]> {
        // _ = self;
        //
        // (the receiver is kept for signature parity.)
        // const page = &pin.node.data;
        let page: *mut terminal::page::Page = unsafe { terminal::pagelist::Node::page(pin.node) };
        // const cell = pin.rowAndCell().cell;
        let cell = pin.row_and_cell().cell;
        // const link_id = page.lookupHyperlink(cell) orelse return null;
        let link_id = match unsafe { (*page).lookup_hyperlink(cell) } {
            Some(v) => v,
            None => return None,
        };
        // const entry = page.hyperlink_set.get(page.memory, link_id);
        let entry = unsafe { (*page).hyperlink_set.get((*page).memory_ptr(), link_id) };
        // return entry.uri.slice(page.memory);
        Some(unsafe { (*entry).uri.slice((*page).memory_ptr()) })
    }

    // pub fn mousePressureCallback(
    //     self: *Surface,
    //     stage: input.MousePressureStage,
    //     pressure: f64,
    // ) !void {
    //
    // (the inferred `!void` is the [`Error`] union.)
    pub fn mousePressureCallback(
        &mut self,
        stage: input_mouse::PressureStage,
        pressure: f64,
    ) -> Result<(), Error> {
        // Crash metadata in case we crash in here
        // crash.sentry.thread_state = self.crashThreadState();
        crash::sentry::setThreadState(self.crashThreadState());
        // defer crash.sentry.thread_state = null;
        // ([ADV 6.1] — the Drop guard clears at every exit below.)
        let _thread_state = ThreadStateGuard;

        // We don't currently use the pressure value for anything. In the
        // future, we could report this to applications using new mouse
        // events or utilize it for some custom UI.
        // _ = pressure;
        let _ = pressure;

        // If the pressure stage is the same as what we already have do nothing
        // if (self.mouse.pressure_stage == stage) return;
        if self.mouse.pressure_stage == stage {
            return Ok(());
        }

        // Update our pressure stage.
        // self.mouse.pressure_stage = stage;
        self.mouse.pressure_stage = stage;

        // A deep press is pressure-sensitive pointer input, such as macOS force
        // click / deep click on a trackpad, that occurs while the left mouse
        // button is already down. Treat it as the platform text-selection
        // affordance: select the pressed word, then consume the active gesture so
        // further cursor motion doesn't drag the selection.
        // const left_idx = @intFromEnum(input.MouseButton.left);
        let left_idx = input_mouse::Button::Left as usize;
        // if (self.mouse.click_state[left_idx] == .press and
        //     stage == .deep)
        // select: {
        if self.mouse.click_state[left_idx] == input_mouse::ButtonState::Press
            && stage == input_mouse::PressureStage::Deep
        {
            'select: {
                // self.renderer_state.mutex.lock();
                // defer self.renderer_state.mutex.unlock();
                //
                // (guard-less zigstd Mutex — the defer is the explicit unlock
                // at every exit below.)
                unsafe { (*self.renderer_state.mutex).lock() };

                // const sel = self.mouse.selection_gesture.deepPress(
                //     self.renderer_state.terminal,
                //     .{ .word_boundary_codepoints = self.config.selection_word_chars },
                // );
                let sel = self.mouse.selection_gesture.deep_press(
                    unsafe { &mut *self.renderer_state.terminal },
                    terminal::selection_gesture::DeepPress {
                        word_boundary_codepoints: &self.config.selection_word_chars,
                    },
                );

                // Deep press consumes the active drag gesture, so stop any pending
                // selection autoscroll timer that may have been started by the drag.
                // if (self.selection_scroll_active) {
                if self.selection_scroll_active {
                    // self.queueIo(
                    //     .{ .selection_scroll = false },
                    //     .locked,
                    // );
                    self.queueIo(
                        termio::Message::SelectionScroll(false),
                        termio::termio_impl::MutexState::Locked,
                    );
                }

                // try self.setSelection(sel orelse break :select);
                let sel = match sel {
                    Some(v) => v,
                    None => {
                        // (the deferred mutex unlock — the labeled break)
                        unsafe { (*self.renderer_state.mutex).unlock() };
                        break 'select;
                    }
                };
                if let Err(err) = self.setSelection(Some(sel)) {
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

                // (the deferred mutex unlock — the block end)
                unsafe { (*self.renderer_state.mutex).unlock() };
            }
        }

        Ok(())
    }

    /// Cursor position callback.
    ///
    /// Send negative x or y values to indicate the cursor is outside the
    /// viewport. The magnitude of the negative values are meaningless;
    /// they are only used to indicate the cursor is outside the viewport.
    /// It's important to do this to ensure hover states are cleared.
    ///
    /// The mods parameter is optional because some apprts do not provide
    /// modifier information on cursor position events. If mods is null then
    /// we'll use the last known mods. This is usually accurate since mod events
    /// will trigger key press events but on some platforms we don't get them.
    /// For example, on macOS, unfocused surfaces don't receive key events but
    /// do receive mouse events so we have to rely on updated mods.
    // pub fn cursorPosCallback(
    //     self: *Surface,
    //     pos: apprt.CursorPos,
    //     mods: ?input.Mods,
    // ) !void {
    //
    // (the inferred `!void` is the [`Error`] union.)
    pub fn cursorPosCallback(
        &mut self,
        pos: apprt::CursorPos,
        mods: Option<Mods>,
    ) -> Result<(), Error> {
        // Crash metadata in case we crash in here
        // crash.sentry.thread_state = self.crashThreadState();
        crash::sentry::setThreadState(self.crashThreadState());
        // defer crash.sentry.thread_state = null;
        // ([ADV 6.1] — the Drop guard clears at every exit below.)
        let _thread_state = ThreadStateGuard;

        // log.debug("cursor pos x={} y={} mods={?}", .{ pos.x, pos.y, mods });
        // (COMMENT-ONLY BY DESIGN: commented out in the ORACLE too, Surface.zig:4539.)

        // If the position is negative, it is outside our viewport and
        // we need to clear any hover states.
        // if (pos.x < 0 or pos.y < 0) {
        if pos.x < 0.0 || pos.y < 0.0 {
            // Reset our hyperlink state
            // self.mouse.link_point = null;
            self.mouse.link_point = None;
            // if (self.mouse.over_link) {
            if self.mouse.over_link {
                // self.mouse.over_link = false;
                self.mouse.over_link = false;
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

            // self.renderer_state.mutex.lock();
            // defer self.renderer_state.mutex.unlock();
            //
            // (guard-less zigstd Mutex — the defer is the explicit unlock at
            // this if-block's end.)
            unsafe { (*self.renderer_state.mutex).lock() };

            // No mouse point so we don't highlight links
            // self.renderer_state.mouse.point = null;
            self.renderer_state.mouse.point = None;

            // Mark the link's row as dirty, but continue with updating the
            // mouse state below so we can scroll when our position is negative.
            // self.renderer_state.terminal.screens.active.dirty.hyperlink_hover = true;
            unsafe {
                (*(*self.renderer_state.terminal).screens.active)
                    .dirty
                    .hyperlink_hover = true;
            }

            // (the deferred mutex unlock — the block end)
            unsafe { (*self.renderer_state.mutex).unlock() };
        }

        // Always show the mouse again if it is hidden
        // if (self.mouse.hidden) self.showMouse();
        if self.mouse.hidden {
            self.showMouse();
        }

        // Update our modifiers if they changed
        // if (mods) |v| self.modsChanged(v);
        if let Some(v) = mods {
            self.modsChanged(v);
        }

        // The mouse position in the viewport
        // const pos_vp = self.posToViewport(pos.x, pos.y);
        let pos_vp = self.posToViewport(f64::from(pos.x), f64::from(pos.y));

        // We always reset the over link status because it will be reprocessed
        // below. But we need the old value to know if we need to undo mouse
        // shape changes.
        // const over_link = self.mouse.over_link;
        let over_link = self.mouse.over_link;
        // self.mouse.over_link = false;
        self.mouse.over_link = false;

        // We are reading/writing state for the remainder
        // self.renderer_state.mutex.lock();
        // defer self.renderer_state.mutex.unlock();
        //
        // (guard-less zigstd Mutex — the defer is the explicit unlock at
        // every exit below.)
        unsafe { (*self.renderer_state.mutex).lock() };

        // Update our mouse state. We set this to null initially because we only
        // want to set it when we're not selecting or doing any other mouse
        // event.
        // self.renderer_state.mouse.point = null;
        self.renderer_state.mouse.point = None;

        // If we have an inspector, we need to always record position information
        // if (self.inspector) |insp| {
        //     insp.mouse.last_xpos = pos.x;
        //     insp.mouse.last_ypos = pos.y;
        //
        //     const screen: *terminal.Screen = self.renderer_state.terminal.screens.active;
        //     insp.mouse.last_point = screen.pages.pin(.{ .viewport = .{
        //         .x = pos_vp.x,
        //         .y = pos_vp.y,
        //     } });
        //     try self.queueRender();
        // }
        //
        // PORT NOTE (Phase-3 fill): `inspectorpkg::Inspector` is now inhabited
        // (its `mouse: widgets::surface::Mouse` sub-state carries
        // `last_xpos`/`last_ypos`/`last_point`, the R3-proven Pin), so the
        // Some-arm preserved verbatim above is reachable and is inhabited here.
        // `insp` is a raw `*mut Inspector` (core's own field, `self.inspector:
        // Option<*mut inspectorpkg::Inspector>`) — dereferenced directly, no
        // new lock (this mirrors the reference, which takes no additional lock
        // beyond the `renderer_state.mutex` already held above). `screen` is
        // obtained the same raw-pointer way the rest of this function/file does
        // (`(*(*self.renderer_state.terminal).screens.active)`), so it never
        // escapes this still-locked scope (INV-1).
        if let Some(insp) = self.inspector {
            // insp.mouse.last_xpos = pos.x;
            // insp.mouse.last_ypos = pos.y;
            unsafe {
                (*insp).mouse.last_xpos = f64::from(pos.x);
                (*insp).mouse.last_ypos = f64::from(pos.y);
            }

            // const screen: *terminal.Screen = self.renderer_state.terminal.screens.active;
            let screen = unsafe { (*self.renderer_state.terminal).screens.active };
            // insp.mouse.last_point = screen.pages.pin(.{ .viewport = .{
            //     .x = pos_vp.x,
            //     .y = pos_vp.y,
            // } });
            unsafe {
                (*insp).mouse.last_point = (*screen).pages.pin(terminal::point::Point::Viewport(
                    terminal::point::Coordinate {
                        x: pos_vp.x,
                        y: pos_vp.y,
                    },
                ));
            }

            // try self.queueRender();
            //
            // (guard-less zigstd Mutex — every other exit in this function
            // unlocks explicitly before returning; do the same here.)
            if let Err(err) = self.queueRender() {
                unsafe { (*self.renderer_state.mutex).unlock() };
                return Err(err.into());
            }
        }

        // Handle link hovering
        // We refresh links when
        // 1. we were previously over a link
        // OR
        // 2. the cursor position has changed (either we have no previous state, or the state has
        //    changed)
        // AND
        // 1. mouse reporting is off
        // OR
        // 2. mouse reporting is on and we are not reporting shift to the terminal
        // if ((over_link or
        //     self.mouse.link_point == null or
        //     (self.mouse.link_point != null and !self.mouse.link_point.?.eql(pos_vp))) and
        //     (self.io.terminal.flags.mouse_event == .none or
        //         (self.mouse.mods.shift and !self.mouseShiftCapture(false))))
        // {
        //
        // PARITY: the `!= null and ... .?.eql` pair keeps the reference's
        // explicit two-step null test; clippy's `unnecessary_unwrap` fold is
        // allowed rather than restructuring (the collapsible_if precedent).
        #[allow(clippy::unnecessary_unwrap)]
        if (over_link
            || self.mouse.link_point.is_none()
            || (self.mouse.link_point.is_some() && !self.mouse.link_point.unwrap().eql(pos_vp)))
            && (self.io.terminal.flags.mouse_event == terminal::mouse::Event::None
                || (self.mouse.mods.shift && !self.mouseShiftCapture(false)))
        {
            // If we were previously over a link, we always update. We do this so that if the text
            // changed underneath us, even if the mouse didn't move, we update the URL hints and state
            // try self.mouseRefreshLinks(pos, pos_vp, over_link);
            if let Err(err) = self.mouseRefreshLinks(pos, pos_vp, over_link) {
                // (the deferred mutex unlock — the error exit)
                unsafe { (*self.renderer_state.mutex).unlock() };
                return Err(err);
            }
        }

        // Do a mouse report
        // if (self.isMouseReporting()) report: {
        if self.isMouseReporting() {
            'report: {
                // Shift overrides mouse "grabbing" in the window, taken from Kitty.
                // This only applies if there is a mouse button pressed so that
                // movement reports are not affected.
                // if (self.mouse.mods.shift and !self.mouseShiftCapture(false)) {
                if self.mouse.mods.shift && !self.mouseShiftCapture(false) {
                    // for (self.mouse.click_state) |state| {
                    for state in self.mouse.click_state {
                        // if (state != .release) break :report;
                        if state != input_mouse::ButtonState::Release {
                            break 'report;
                        }
                    }
                }

                // We use the first mouse button we find pressed in order to report
                // since the spec (afaict) does not say...
                // const button: ?input.MouseButton = button: for (self.mouse.click_state, 0..) |state, i| {
                //     if (state == .press)
                //         break :button @enumFromInt(i);
                // } else null;
                let button: Option<input_mouse::Button> = 'button: {
                    for (i, state) in self.mouse.click_state.iter().enumerate() {
                        if *state == input_mouse::ButtonState::Press {
                            // (`@enumFromInt(i)` — the click_state indices ARE
                            // the `Button` discriminants; realized as the
                            // exhaustive index match.)
                            break 'button Some(match i {
                                0 => input_mouse::Button::Unknown,
                                1 => input_mouse::Button::Left,
                                2 => input_mouse::Button::Right,
                                3 => input_mouse::Button::Middle,
                                4 => input_mouse::Button::Four,
                                5 => input_mouse::Button::Five,
                                6 => input_mouse::Button::Six,
                                7 => input_mouse::Button::Seven,
                                8 => input_mouse::Button::Eight,
                                9 => input_mouse::Button::Nine,
                                10 => input_mouse::Button::Ten,
                                // (the array length is `Button.max` = 11 —
                                // indices stop at 10.)
                                _ => unreachable!(),
                            });
                        }
                    }
                    // } else null;
                    None
                };

                // self.mouseReport(button, .motion, self.mouse.mods, pos);
                self.mouseReport(button, input_mouse::Action::Motion, self.mouse.mods, pos);

                // If we're doing mouse motion tracking, we do not support text
                // selection.
                // return;
                //
                // (the deferred mutex unlock — the return)
                unsafe { (*self.renderer_state.mutex).unlock() };
                return Ok(());
            }
        }

        // Handle cursor position for text selection
        // if (self.mouse.click_state[@intFromEnum(input.MouseButton.left)] == .press) select: {
        if self.mouse.click_state[input_mouse::Button::Left as usize]
            == input_mouse::ButtonState::Press
        {
            'select: {
                // Left click pressed but count zero can happen if mouse reporting is on.
                // In this scenario, we mark the click state because we need that to
                // properly make some mouse reports, but we don't keep track of the
                // count because we don't want to handle selection.
                // if (self.mouse.selection_gesture.left_click_count == 0) break :select;
                if self.mouse.selection_gesture.left_click_count == 0 {
                    break 'select;
                }

                // If our left-click pin no longer belongs to the active screen then we
                // don't process this. We don't invalidate our pin or mouse state
                // because if the same screen switches back then we can continue our
                // selection.
                // const t: *terminal.Terminal = self.renderer_state.terminal;
                let t: *mut Terminal = self.renderer_state.terminal;
                // if (self.mouse.activeLeftClickPin(&t.screens) == null) break :select;
                if self
                    .mouse
                    .activeLeftClickPin(unsafe { &(*t).screens })
                    .is_none()
                {
                    break 'select;
                }

                // All roads lead to requiring a re-render at this point.
                // try self.queueRender();
                if let Err(err) = self.queueRender() {
                    // (the deferred mutex unlock — the error exit)
                    unsafe { (*self.renderer_state.mutex).unlock() };
                    return Err(err.into());
                }

                // Convert to points
                // const screen: *terminal.Screen = t.screens.active;
                let screen: *mut terminal::screen::Screen = unsafe { (*t).screens.active };
                // const pin = screen.pages.pin(.{
                //     .viewport = .{
                //         .x = pos_vp.x,
                //         .y = pos_vp.y,
                //     },
                // }) orelse {
                //     if (comptime std.debug.runtime_safety) unreachable;
                //     return;
                // };
                let pin = match unsafe {
                    (*screen).pages.pin(terminal::point::Point::Viewport(
                        terminal::point::Coordinate {
                            x: pos_vp.x,
                            y: pos_vp.y,
                        },
                    ))
                } {
                    Some(v) => v,
                    None => {
                        // if (comptime std.debug.runtime_safety) unreachable;
                        if cfg!(debug_assertions) {
                            unreachable!();
                        }
                        // return;
                        //
                        // (the deferred mutex unlock — the return)
                        unsafe { (*self.renderer_state.mutex).unlock() };
                        return Ok(());
                    }
                };

                // Perform our drag behavior in our gesture handler.
                // const drag_selection = self.mouse.selection_gesture.drag(t, .{
                //     .pin = pin,
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
                let drag_selection = self.mouse.selection_gesture.drag(
                    unsafe { &mut *t },
                    terminal::selection_gesture::Drag {
                        pin,
                        xpos: f64::from(pos.x),
                        ypos: f64::from(pos.y),
                        rectangle: SurfaceMouse::is_rectangle_select_state(self.mouse.mods),
                        word_boundary_codepoints: &self.config.selection_word_chars,
                        geometry: terminal::selection_gesture::Geometry {
                            columns: u32::from(self.size.grid().columns),
                            cell_width: self.size.cell.width,
                            padding_left: self.size.padding.left,
                            screen_height: self.size.screen.height,
                        },
                    },
                );

                // Update our autoscroll timer based on the gesture state
                // switch (self.mouse.selection_gesture.left_drag_autoscroll) {
                match self.mouse.selection_gesture.left_drag_autoscroll {
                    // .none => if (self.selection_scroll_active) {
                    terminal::selection_gesture::Autoscroll::None => {
                        if self.selection_scroll_active {
                            // self.queueIo(
                            //     .{ .selection_scroll = false },
                            //     .locked,
                            // );
                            self.queueIo(
                                termio::Message::SelectionScroll(false),
                                termio::termio_impl::MutexState::Locked,
                            );
                        }
                    }
                    // .up, .down => if (!self.selection_scroll_active) {
                    terminal::selection_gesture::Autoscroll::Up
                    | terminal::selection_gesture::Autoscroll::Down => {
                        if !self.selection_scroll_active {
                            // self.queueIo(
                            //     .{ .selection_scroll = true },
                            //     .locked,
                            // );
                            self.queueIo(
                                termio::Message::SelectionScroll(true),
                                termio::termio_impl::MutexState::Locked,
                            );
                        }
                    }
                }

                // Update our selection based on the gesture state
                // try self.setSelection(drag_selection);
                if let Err(err) = self.setSelection(drag_selection) {
                    // (the deferred mutex unlock — the error exit)
                    unsafe { (*self.renderer_state.mutex).unlock() };
                    return Err(err);
                }
            }
        }

        // (the deferred mutex unlock — the function end)
        unsafe { (*self.renderer_state.mutex).unlock() };
        Ok(())
    }

    /// Call to notify Rustty that the color scheme for the terminal has
    /// changed.
    // pub fn colorSchemeCallback(self: *Surface, scheme: apprt.ColorScheme) !void {
    //
    // (the inferred `!void` is the [`Error`] union.)
    pub fn colorSchemeCallback(&mut self, scheme: apprt::ColorScheme) -> Result<(), Error> {
        // Crash metadata in case we crash in here
        // crash.sentry.thread_state = self.crashThreadState();
        crash::sentry::setThreadState(self.crashThreadState());
        // defer crash.sentry.thread_state = null;
        // ([ADV 6.1] — the Drop guard clears at every exit below.)
        let _thread_state = ThreadStateGuard;

        // const new_scheme: configpkg.ConditionalState.Theme = switch (scheme) {
        //     .light => .light,
        //     .dark => .dark,
        // };
        let new_scheme: configpkg::conditional::Theme = match scheme {
            apprt::ColorScheme::Light => configpkg::conditional::Theme::Light,
            apprt::ColorScheme::Dark => configpkg::conditional::Theme::Dark,
        };

        // If our scheme didn't change, then we don't do anything.
        // if (self.config_conditional_state.theme == new_scheme) return;
        if self.config_conditional_state.theme == new_scheme {
            return Ok(());
        }

        // Setup our conditional state which has the current color theme.
        // self.config_conditional_state.theme = new_scheme;
        self.config_conditional_state.theme = new_scheme;
        // self.notifyConfigConditionalState();
        self.notifyConfigConditionalState();

        // If mode 2031 is on, then we report the change live.
        // self.queueIo(.{ .color_scheme_report = .{ .force = false } }, .unlocked);
        self.queueIo(
            termio::Message::ColorSchemeReport { force: false },
            termio::termio_impl::MutexState::Unlocked,
        );

        Ok(())
    }

    // pub fn posToViewport(self: Surface, xpos: f64, ypos: f64) terminal.point.Coordinate {
    //
    // (the reference's BY-VALUE receiver is a read-only copy — `&self`.)
    pub fn posToViewport(&self, xpos: f64, ypos: f64) -> terminal::point::Coordinate {
        // Get our grid cell
        // const coord: rendererpkg.Coordinate = .{ .surface = .{ .x = xpos, .y = ypos } };
        let coord: rendererpkg::size::Coordinate =
            rendererpkg::size::Coordinate::Surface(rendererpkg::size::CoordinateSurface {
                x: xpos,
                y: ypos,
            });
        // const grid = coord.convert(.grid, self.size).grid;
        //
        // (the `.grid` union-field read — the tag is known post-convert.)
        let grid = match coord.convert(rendererpkg::size::CoordinateTag::Grid, self.size) {
            rendererpkg::size::Coordinate::Grid(v) => v,
            _ => unreachable!(),
        };
        // return .{ .x = grid.x, .y = grid.y };
        terminal::point::Coordinate {
            x: grid.x,
            y: u32::from(grid.y),
        }
    }

    /// Scroll to the bottom of the viewport.
    ///
    /// Precondition: the render_state mutex must be held.
    // fn scrollToBottom(self: *Surface) !void {
    //
    // PORT NOTE: now LIVE via the C14a performBindingAction csi/esc/cursor_key
    // arms (:4829/:4856/:4872); the remaining C15 completeClipboardPaste
    // (:5891) callsite lands later. The inferred `!void` is the [`Error`]
    // union.
    fn scrollToBottom(&mut self) -> Result<(), Error> {
        // self.io.terminal.scrollViewport(.{ .bottom = {} });
        self.io
            .terminal
            .scroll_viewport(terminal::terminal::ScrollViewport::Bottom);
        // try self.queueRender();
        self.queueRender()?;
        Ok(())
    }

    // fn hideMouse(self: *Surface) void {
    fn hideMouse(&mut self) {
        // if (self.mouse.hidden) return;
        if self.mouse.hidden {
            return;
        }
        // self.mouse.hidden = true;
        self.mouse.hidden = true;
        // _ = self.rt_app.performAction(
        //     .{ .surface = self },
        //     .mouse_visibility,
        //     .hidden,
        // ) catch |err| {
        //     log.warn("apprt failed to set mouse visibility err={}", .{err});
        // };
        if let Err(err) = unsafe { &*self.rt_app }.perform_action(
            apprt::Target::Surface(&*self),
            apprt::Action::MouseVisibility(MouseVisibility::Hidden),
        ) {
            // log.warn("apprt failed to set mouse visibility err={}", .{err});
            log::warn!(target: LOG_SCOPE, "apprt failed to set mouse visibility err={err:?}");
        }
    }

    // fn showMouse(self: *Surface) void {
    fn showMouse(&mut self) {
        // if (!self.mouse.hidden) return;
        if !self.mouse.hidden {
            return;
        }
        // self.mouse.hidden = false;
        self.mouse.hidden = false;
        // _ = self.rt_app.performAction(
        //     .{ .surface = self },
        //     .mouse_visibility,
        //     .visible,
        // ) catch |err| {
        //     log.warn("apprt failed to set mouse visibility err={}", .{err});
        // };
        if let Err(err) = unsafe { &*self.rt_app }.perform_action(
            apprt::Target::Surface(&*self),
            apprt::Action::MouseVisibility(MouseVisibility::Visible),
        ) {
            // log.warn("apprt failed to set mouse visibility err={}", .{err});
            log::warn!(target: LOG_SCOPE, "apprt failed to set mouse visibility err={err:?}");
        }
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
    // pub fn performBindingAction(self: *Surface, action: input.Binding.Action) !bool {
    //
    // PORT NOTE (chunk C14a — Surface.zig:4775-5202, the first half of the
    // switch; the second half `.scroll_to_selection`.. lands with C14b). The
    // `.search` arm is DEFERRED (DEFERRALS.md "core C14a"): it constructs the
    // vt search thread sharing `self.renderer_state.mutex`, which needs a
    // cross-crate mutex-type unification (the search `Options.mutex` is still
    // `*const std::sync::Mutex<()>` while the shared renderer mutex is the
    // guard-less `rustty_zigstd::thread::Mutex`) AND a self-referential
    // long-lived thread spawn (a stable pointer into `self.search.?.state`).
    pub fn performBindingAction(&mut self, action: binding::Action) -> Result<bool, Error> {
        // Forward app-scoped actions to the app. Some app-scoped actions are
        // special-cased here because they do some special things when performed
        // from the surface.
        // if (action.scoped(.app)) |app_action| {
        if let Some(app_action) = action.scoped_app() {
            // switch (app_action) {
            match app_action {
                // .new_window => try self.app.newWindow(
                //     self.rt_app,
                //     .{ .parent = self },
                // ),
                binding::ScopedApp::NewWindow => {
                    let self_ptr: *mut Surface<A> = self;
                    let rt_app: &A = unsafe { &*self.rt_app };
                    unsafe { &mut *self.app }.newWindow(
                        rt_app,
                        crate::app::NewWindow {
                            parent: Some(self_ptr),
                        },
                    )?;
                }

                // Undo and redo both support both surface and app targeting.
                // If we are triggering on a surface then we perform the
                // action with the surface target.
                // .undo => return try self.rt_app.performAction(
                //     .{ .surface = self },
                //     .undo,
                //     {},
                // ),
                binding::ScopedApp::Undo => {
                    return Ok(unsafe { &*self.rt_app }
                        .perform_action(apprt::Target::Surface(&*self), apprt::Action::Undo)?);
                }

                // .redo => return try self.rt_app.performAction(
                //     .{ .surface = self },
                //     .redo,
                //     {},
                // ),
                binding::ScopedApp::Redo => {
                    return Ok(unsafe { &*self.rt_app }
                        .perform_action(apprt::Target::Surface(&*self), apprt::Action::Redo)?);
                }

                // else => try self.app.performAction(
                //     self.rt_app,
                //     action.scoped(.app).?,
                // ),
                //
                // (`app_action` IS `action.scoped(.app).?`, the value the
                // reference re-derives here.)
                _ => {
                    let rt_app: &A = unsafe { &*self.rt_app };
                    unsafe { &mut *self.app }.performAction(rt_app, app_action)?;
                }
            }
            // return true;
            return Ok(true);
        }

        // switch (action.scoped(.surface).?) {
        match action.scoped_surface().unwrap() {
            // .csi, .esc => |data| {
            binding::ScopedSurface::Csi(data) | binding::ScopedSurface::Esc(data) => {
                // We need to send the CSI/ESC sequence as a single write request.
                // If you split it across two then the shell can interpret it
                // as two literals.
                // var buf: [128]u8 = undefined;
                let mut buf = [0u8; 128];
                // const full_data = switch (action) {
                //     .csi => try std.fmt.bufPrint(&buf, "\x1b[{s}", .{data}),
                //     .esc => try std.fmt.bufPrint(&buf, "\x1b{s}", .{data}),
                //     else => unreachable,
                // };
                //
                // (fixed-buffer `bufPrint` over a Cursor — the encodeKey
                // precedent; the reference's `error.NoSpaceLeft` surfaces as
                // io's `WriteZero`, `try`-propagated [`Error::Io`].)
                let full_data_len = {
                    use std::io::Write as _;
                    let mut writer = std::io::Cursor::new(&mut buf[..]);
                    match action {
                        binding::Action::Csi(_) => {
                            writer.write_all(b"\x1b[")?;
                            writer.write_all(data)?;
                        }
                        binding::Action::Esc(_) => {
                            writer.write_all(b"\x1b")?;
                            writer.write_all(data)?;
                        }
                        // else => unreachable,
                        _ => unreachable!(),
                    }
                    writer.position() as usize
                };
                let full_data = &buf[..full_data_len];
                // self.queueIo(try termio.Message.writeReq(
                //     self.alloc,
                //     full_data,
                // ), .unlocked);
                self.queueIo(
                    termio::Message::writeReq(full_data),
                    termio::termio_impl::MutexState::Unlocked,
                );

                // CSI/ESC triggers a scroll.
                // {
                //     self.renderer_state.mutex.lock();
                //     defer self.renderer_state.mutex.unlock();
                //     self.scrollToBottom() catch |err| {
                //         log.warn("error scrolling to bottom err={}", .{err});
                //     };
                // }
                {
                    unsafe { (*self.renderer_state.mutex).lock() };
                    if let Err(err) = self.scrollToBottom() {
                        // log.warn("error scrolling to bottom err={}", .{err});
                        log::warn!(target: LOG_SCOPE, "error scrolling to bottom err={err:?}");
                    }
                    unsafe { (*self.renderer_state.mutex).unlock() };
                }
            }

            // .text => |data| {
            binding::ScopedSurface::Text(data) => {
                // For text we always allocate just because its easier to
                // handle all cases that way.
                // const buf = try self.alloc.alloc(u8, data.len);
                // defer self.alloc.free(buf);
                let mut buf = vec![0u8; data.len()];
                // const text = configpkg.string.parse(buf, data) catch |err| {
                //     log.warn(
                //         "error parsing text binding text={s} err={}",
                //         .{ data, err },
                //     );
                //     return true;
                // };
                //
                // (the reference `parse` returns a slice; the ported `parse`
                // writes into `buf` and returns the written length.)
                let text_len = match configpkg::string::parse(&mut buf, data) {
                    Ok(n) => n,
                    Err(err) => {
                        // log.warn("error parsing text binding text={s} err={}", .{ data, err });
                        log::warn!(
                            target: LOG_SCOPE,
                            "error parsing text binding text={} err={err:?}",
                            String::from_utf8_lossy(data)
                        );
                        return Ok(true);
                    }
                };
                let text = &buf[..text_len];
                // self.queueIo(try termio.Message.writeReq(
                //     self.alloc,
                //     text,
                // ), .unlocked);
                self.queueIo(
                    termio::Message::writeReq(text),
                    termio::termio_impl::MutexState::Unlocked,
                );

                // Text triggers a scroll.
                // {
                //     self.renderer_state.mutex.lock();
                //     defer self.renderer_state.mutex.unlock();
                //     self.scrollToBottom() catch |err| {
                //         log.warn("error scrolling to bottom err={}", .{err});
                //     };
                // }
                {
                    unsafe { (*self.renderer_state.mutex).lock() };
                    if let Err(err) = self.scrollToBottom() {
                        // log.warn("error scrolling to bottom err={}", .{err});
                        log::warn!(target: LOG_SCOPE, "error scrolling to bottom err={err:?}");
                    }
                    unsafe { (*self.renderer_state.mutex).unlock() };
                }
            }

            // .cursor_key => |ck| {
            binding::ScopedSurface::CursorKey(ck) => {
                // We send a different sequence depending on if we're
                // in cursor keys mode. We're in "normal" mode if cursor
                // keys mode is NOT set.
                // const normal = normal: {
                //     self.renderer_state.mutex.lock();
                //     defer self.renderer_state.mutex.unlock();
                //
                //     // With the lock held, we must scroll to the bottom.
                //     // We always scroll to the bottom for these inputs.
                //     self.scrollToBottom() catch |err| {
                //         log.warn("error scrolling to bottom err={}", .{err});
                //     };
                //
                //     break :normal !self.io.terminal.modes.get(.cursor_keys);
                // };
                let normal = {
                    unsafe { (*self.renderer_state.mutex).lock() };

                    // With the lock held, we must scroll to the bottom.
                    // We always scroll to the bottom for these inputs.
                    if let Err(err) = self.scrollToBottom() {
                        // log.warn("error scrolling to bottom err={}", .{err});
                        log::warn!(target: LOG_SCOPE, "error scrolling to bottom err={err:?}");
                    }

                    let normal = !self
                        .io
                        .terminal
                        .modes
                        .get(terminal::modes::Mode::CursorKeys);
                    unsafe { (*self.renderer_state.mutex).unlock() };
                    normal
                };

                // if (normal) {
                //     self.queueIo(.{ .write_stable = ck.normal }, .unlocked);
                // } else {
                //     self.queueIo(.{ .write_stable = ck.application }, .unlocked);
                // }
                //
                // PORT NOTE (write_stable -> writeReq): the reference
                // `write_stable` aliases the keybind config's stable slice
                // storage. The Rust `CursorKey` OWNS its `Vec<u8>` sequences
                // (the byte-slice convention) and this `action` is a cloned,
                // stack-local copy dropped at return, so no "stable pointer"
                // exists; `queueIo` requires a `Message<'static>`, so the bytes
                // are COPIED via `writeReq` (write_small/write_alloc).
                if normal {
                    self.queueIo(
                        termio::Message::writeReq(&ck.normal),
                        termio::termio_impl::MutexState::Unlocked,
                    );
                } else {
                    self.queueIo(
                        termio::Message::writeReq(&ck.application),
                        termio::termio_impl::MutexState::Unlocked,
                    );
                }
            }

            // .reset => {
            binding::ScopedSurface::Reset => {
                // self.renderer_state.mutex.lock();
                // defer self.renderer_state.mutex.unlock();
                // self.renderer_state.terminal.fullReset();
                unsafe { (*self.renderer_state.mutex).lock() };
                unsafe { (*self.renderer_state.terminal).full_reset() };
                unsafe { (*self.renderer_state.mutex).unlock() };
            }

            // .start_search => {
            binding::ScopedSurface::StartSearch => {
                // To save resources, we don't actually start a search here,
                // we just notify the apprt. The real thread will start when
                // the first needles are set.
                // return try self.rt_app.performAction(
                //     .{ .surface = self },
                //     .start_search,
                //     .{ .needle = "" },
                // );
                return Ok(unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::Surface(&*self),
                    apprt::Action::StartSearch(StartSearch { needle: b"" }),
                )?);
            }

            // .search_selection => {
            binding::ScopedSurface::SearchSelection => {
                // const selection = try self.selectionString(self.alloc) orelse return false;
                // defer self.alloc.free(selection);
                //
                // (the owned `Vec` drops at scope end.)
                let selection = match self.selectionString(self.alloc)? {
                    Some(s) => s,
                    None => return Ok(false),
                };
                // return try self.rt_app.performAction(
                //     .{ .surface = self },
                //     .start_search,
                //     .{ .needle = selection },
                // );
                return Ok(unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::Surface(&*self),
                    apprt::Action::StartSearch(StartSearch { needle: &selection }),
                )?);
            }

            // .end_search => {
            binding::ScopedSurface::EndSearch => {
                // We only return that this was performed if we actually
                // stopped a search, but we also send the apprt end_search so
                // that GUIs can clean up stale stuff.
                // const performed = self.search != null;
                let performed = self.search.is_some();

                // if (self.search) |*s| {
                //     s.deinit();
                //     self.search = null;
                // }
                //
                // (`take()` is the reference's `deinit` + `= null`; `deinit`
                // consumes the by-value `Search`.)
                if let Some(s) = self.search.take() {
                    s.deinit();
                }

                // _ = try self.rt_app.performAction(
                //     .{ .surface = self },
                //     .end_search,
                //     {},
                // );
                let _ = unsafe { &*self.rt_app }
                    .perform_action(apprt::Target::Surface(&*self), apprt::Action::EndSearch)?;

                // return performed;
                return Ok(performed);
            }

            // .search => |text| search: {
            //
            // PORT NOTE (SEARCH_BINDING_ARM_DESIGN): the two blockers are
            // resolved — (a) the vt `search::thread::Options.mutex` is retyped to
            // the shared guard-less `rustty_zigstd::thread::Mutex` (so
            // `self.renderer_state.mutex` is type-compatible), and (b) the
            // self-referential construct-then-spawn lands via a stable-address
            // raw-ptr discipline: the `Search` is stored FIRST (`thread: None` ==
            // `.thread = undefined`), a stable `*mut Thread` is projected from the
            // stored state, the thread is spawned on THAT raw pointer, and the
            // returned handle is written back through a disjoint raw sub-place.
            // Post-spawn producers reach the running thread ONLY through decoupled
            // handles (§3; proven SB+TB clean on `mirihar-chunk3b`
            // `search_shipped`/`callback_shipped`).
            binding::ScopedSurface::Search(text) => {
                // const s: *Search = if (self.search) |*s| s else init: {
                if self.search.is_none() {
                    // init: {
                    //     // If we're stopping the search and we had no prior search,
                    //     // then there is nothing to do.
                    //     if (text.len == 0) return false;
                    if text.is_empty() {
                        return Ok(false);
                    }

                    // Stable base address of the pinned Surface: `init` writes it
                    // through a `*mut Surface`, the same address `searchCallback`
                    // casts `event_userdata` back to. The thread re-enters through
                    // it via NARROW raw field projections only (§3.3a).
                    let surface_ptr: *mut Surface<A> = self;

                    // We need to assign directly to self.search because we need
                    // a stable pointer back to the thread state.
                    // self.search = .{
                    //     .state = try .init(self.alloc, .{
                    //         .mutex = self.renderer_state.mutex,
                    //         .terminal = self.renderer_state.terminal,
                    //         .event_cb = &searchCallback,
                    //         .event_userdata = self,
                    //     }),
                    //     .thread = undefined,
                    // };
                    //
                    // ONE-SHOT BUILD (§2.4): build `state` as a LOCAL, arm its
                    // wakeup/stop waits + capture the decoupled wakeup/stop
                    // notifiers OFF the local (Arc<Shared>-backed, move-stable),
                    // THEN move it whole into `self.search`. `thread: None` is the
                    // `.thread = undefined` placeholder; `mailbox` is filled from
                    // the POST-MOVE `state_ptr` lineage below (a pre-move
                    // `addr_of!` alias would be popped by the child's
                    // `Box<Mailbox>` noalias — mirihar `search_shipped`).
                    let mut state = terminal::search::thread::Thread::init(
                        self.alloc,
                        terminal::search::thread::Options {
                            // .mutex = self.renderer_state.mutex,
                            mutex: self.renderer_state.mutex, // *mut -> *const (§1)
                            // .terminal = self.renderer_state.terminal,
                            terminal: self.renderer_state.terminal,
                            // .event_cb = &searchCallback,
                            event_cb: Some(Self::searchCallback),
                            // .event_userdata = self,
                            event_userdata: surface_ptr as *mut core::ffi::c_void,
                        },
                    )?; // needs From<..search::thread::xev::Error> (added above)

                    // Arm the wakeup/stop waits ON THE LOCAL + capture the
                    // decoupled notifiers BEFORE the move (identical to the vt
                    // `search_thread` test setup; `thread_main` re-arms them
                    // idempotently).
                    state.wakeup.wait(
                        &state.loop_,
                        &mut state.wakeup_c,
                        terminal::search::thread::xev::Callback::Wakeup,
                    );
                    state.stop.wait(
                        &state.loop_,
                        &mut state.stop_c,
                        terminal::search::thread::xev::Callback::Stop,
                    );
                    let wn = state.wakeup.notifier(); // holds Arc<Shared>
                    let sn = state.stop.notifier();

                    // self.search = .{ .state = <local>, .thread = undefined };
                    self.search = Some(Search {
                        state,
                        thread: None,               // == `.thread = undefined`
                        mailbox: core::ptr::null(), // set from state_ptr below
                        wakeup: wn,
                        stop: sn,
                    });

                    // const s: *Search = &self.search.?;   — ONE transient &mut,
                    // taken at birth (nothing aliases `state` yet), lowered
                    // immediately to raw. After this NO &mut Search / &mut Thread
                    // is ever formed again.
                    let search_ptr: *mut Search = &raw mut *self.search.as_mut().unwrap();
                    let state_ptr: *mut terminal::search::thread::Thread =
                        unsafe { &raw mut (*search_ptr).state };
                    // Decoupled mailbox = the Box<Mailbox> pointee, via the
                    // POST-MOVE state_ptr lineage (mirihar `search_shipped`).
                    unsafe {
                        (*search_ptr).mailbox = &raw const *(*state_ptr).mailbox;
                    }

                    // errdefer s.state.deinit();
                    // s.thread = try .spawn(
                    //     .{},
                    //     terminal.search.Thread.threadMain,
                    //     .{&s.state},
                    // );
                    // s.thread.setName("search") catch {};
                    struct SendState(*mut terminal::search::thread::Thread);
                    unsafe impl Send for SendState {}
                    let send = SendState(state_ptr);
                    let spawned = std::thread::Builder::new()
                        .name("search".into()) // == setName("search")
                        .spawn(move || {
                            let s = send;
                            unsafe { (*s.0).thread_main() };
                        });
                    let handle = match spawned {
                        Ok(h) => h,
                        Err(e) => {
                            // errdefer s.state.deinit(); + the `try` unwinds
                            // performBindingAction's `!bool` (`Ok(false)` would
                            // SWALLOW the spawn error — not faithful). Deviation
                            // (§2.4): also clear `self.search` — a `Some` holding a
                            // deinit'd state + `thread: None` is a dangling hazard;
                            // `None` is the Rust empty. `Builder::spawn`'s
                            // `io::Error` already has `From` for surface `Error`.
                            unsafe {
                                (*state_ptr).deinit();
                            }
                            self.search = None;
                            return Err(e.into());
                        }
                    };
                    // s.thread = handle;  — through the disjoint `thread` sub-place
                    // of search_ptr (no &mut Search reborrow, so the child's live
                    // state_ptr tag is not popped; SB-clean).
                    unsafe {
                        (*search_ptr).thread = Some(handle);
                    }
                    // break :init s;
                }

                // Zero-length text means stop searching.
                // if (text.len == 0) {
                //     s.deinit();
                //     self.search = null;
                //     break :search;
                // }
                if text.is_empty() {
                    // `take()` is the reference's `deinit` + `= null`; `deinit`
                    // stop-notifies (decoupled handle) THEN joins THEN frees the
                    // state (§3.3). After `take()` the child is dead — no alias.
                    self.search.take().unwrap().deinit();
                    // break :search
                } else {
                    // _ = s.state.mailbox.push(
                    //     .{ .change_needle = try .init(
                    //         self.alloc,
                    //         text,
                    //     ) },
                    //     .forever,
                    // );
                    // s.state.wakeup.notify() catch {};
                    //
                    // PRODUCER PATH: reach the mailbox + wakeup through the
                    // DECOUPLED handles, NEVER `self.search.as_mut().state` (a
                    // &mut Thread reborrow while the child holds a live &mut self
                    // → SB UB; mirihar `search_naive`).
                    let s = self.search.as_ref().unwrap(); // &Search: sibling handles only
                    let mailbox = s.mailbox;
                    let wakeup = s.wakeup.clone();
                    let needle = terminal::search::thread::WriteReq::init(text);
                    let _ = unsafe {
                        (*mailbox).push(
                            terminal::search::thread::Message::ChangeNeedle(needle),
                            Timeout::Forever,
                        )
                    };
                    let _ = wakeup.notify();
                }
            }

            // .navigate_search => |nav| {
            binding::ScopedSurface::NavigateSearch(nav) => {
                // const s: *Search = if (self.search) |*s| s else return false;
                //
                // PORT NOTE (§3.3 retrofit): `as_ref` (not `as_mut`) — the
                // producer reaches the running thread through the DECOUPLED
                // `s.mailbox`/`s.wakeup` handles, NEVER `s.state.mailbox`
                // (a &mut Thread reborrow while the child holds a live &mut self
                // → SB UB; mirihar `search_naive`).
                let (mailbox, wakeup) = match self.search.as_ref() {
                    Some(s) => (s.mailbox, s.wakeup.clone()),
                    None => return Ok(false),
                };
                // _ = s.state.mailbox.push(
                //     .{ .select = switch (nav) {
                //         .next => .next,
                //         .previous => .prev,
                //     } },
                //     .forever,
                // );
                let _ = unsafe {
                    (*mailbox).push(
                        terminal::search::thread::Message::Select(match nav {
                            binding::NavigateSearch::Next => terminal::search::screen::Select::Next,
                            binding::NavigateSearch::Previous => {
                                terminal::search::screen::Select::Prev
                            }
                        }),
                        Timeout::Forever,
                    )
                };
                // s.state.wakeup.notify() catch {};
                let _ = wakeup.notify();
            }

            // .copy_to_clipboard => |format| {
            binding::ScopedSurface::CopyToClipboard(format) => {
                // self.renderer_state.mutex.lock();
                // defer self.renderer_state.mutex.unlock();
                //
                // (the deferred unlock at every exit — the selectionString C9
                // convention; the fallible calls unlock before propagating.)
                unsafe { (*self.renderer_state.mutex).lock() };

                // if (self.io.terminal.screens.active.selection) |sel| {
                if let Some(sel) = unsafe { (*self.io.terminal.screens.active).selection } {
                    // try self.copySelectionToClipboards(
                    //     sel,
                    //     &.{.standard},
                    //     format,
                    // );
                    if let Err(err) =
                        self.copySelectionToClipboards(sel, &[apprt::Clipboard::Standard], format)
                    {
                        unsafe { (*self.renderer_state.mutex).unlock() };
                        return Err(err);
                    }

                    // Clear the selection if configured to do so.
                    // if (self.config.selection_clear_on_copy) {
                    if self.config.selection_clear_on_copy {
                        // if (self.setSelection(null)) {
                        //     self.queueRender() catch |err| {
                        //         log.warn("failed to queue render after clear selection err={}", .{err});
                        //     };
                        // } else |err| {
                        //     log.warn("failed to clear selection after copy err={}", .{err});
                        // }
                        match self.setSelection(None) {
                            Ok(()) => {
                                if let Err(err) = self.queueRender() {
                                    // log.warn("failed to queue render after clear selection err={}", .{err});
                                    log::warn!(
                                        target: LOG_SCOPE,
                                        "failed to queue render after clear selection err={err:?}"
                                    );
                                }
                            }
                            Err(err) => {
                                // log.warn("failed to clear selection after copy err={}", .{err});
                                log::warn!(
                                    target: LOG_SCOPE,
                                    "failed to clear selection after copy err={err:?}"
                                );
                            }
                        }
                    }

                    // return true;
                    unsafe { (*self.renderer_state.mutex).unlock() };
                    return Ok(true);
                }

                // return false;
                unsafe { (*self.renderer_state.mutex).unlock() };
                return Ok(false);
            }

            // .copy_url_to_clipboard => {
            binding::ScopedSurface::CopyUrlToClipboard => {
                // If the mouse isn't over a link, nothing we can do.
                // if (!self.mouse.over_link) return false;
                if !self.mouse.over_link {
                    return Ok(false);
                }
                // const pos = try self.rt_surface.getCursorPos();
                let pos = unsafe { &*self.rt_surface }.get_cursor_pos()?;

                // self.renderer_state.mutex.lock();
                // defer self.renderer_state.mutex.unlock();
                //
                // (the deferred unlock at every exit — the C9 convention.)
                unsafe { (*self.renderer_state.mutex).lock() };
                // if (try self.linkAtPos(pos)) |link_info| {
                let link_info = match self.linkAtPos(pos) {
                    Ok(v) => v,
                    Err(err) => {
                        unsafe { (*self.renderer_state.mutex).unlock() };
                        return Err(err);
                    }
                };
                if let Some(link_info) = link_info {
                    // const url_text = switch (link_info.action) {
                    let url_text: Vec<u8> = match link_info.action {
                        // .open => url_text: {
                        link::Action::Open => {
                            // For regex links, get the text from selection
                            // break :url_text (self.io.terminal.screens.active.selectionString(self.alloc, .{
                            //     .sel = link_info.selection,
                            //     .trim = self.config.clipboard_trim_trailing_spaces,
                            // })) catch |err| {
                            //     log.err("error reading url string err={}", .{err});
                            //     return false;
                            // };
                            let r = unsafe { &mut *self.io.terminal.screens.active }
                                .selection_string(
                                    self.alloc,
                                    terminal::screen::SelectionString {
                                        sel: link_info.selection,
                                        trim: self.config.clipboard_trim_trailing_spaces,
                                    },
                                );
                            match r {
                                Ok(v) => v,
                                Err(err) => {
                                    // log.err("error reading url string err={}", .{err});
                                    log::error!(
                                        target: LOG_SCOPE,
                                        "error reading url string err={err:?}"
                                    );
                                    unsafe { (*self.renderer_state.mutex).unlock() };
                                    return Ok(false);
                                }
                            }
                        }

                        // ._open_osc8 => url_text: {
                        link::Action::OpenOsc8 => {
                            // For OSC8 links, get the URI directly from hyperlink data
                            // const uri = self.osc8URI(link_info.selection.start()) orelse {
                            //     log.warn("failed to get URI for OSC8 hyperlink", .{});
                            //     return false;
                            // };
                            let uri = match self.osc8URI(link_info.selection.start()) {
                                Some(v) => v,
                                None => {
                                    // log.warn("failed to get URI for OSC8 hyperlink", .{});
                                    log::warn!(
                                        target: LOG_SCOPE,
                                        "failed to get URI for OSC8 hyperlink"
                                    );
                                    unsafe { (*self.renderer_state.mutex).unlock() };
                                    return Ok(false);
                                }
                            };
                            // break :url_text try self.alloc.dupeZ(u8, uri);
                            //
                            // (`dupeZ` — an owned copy of the raw URI bytes; the
                            // NUL sentinel drops per the byte-slice convention,
                            // the processLinks C13 precedent.)
                            uri.to_vec()
                        }
                    };
                    // defer self.alloc.free(url_text);
                    // (the owned `Vec` drops at scope end.)

                    // self.rt_surface.setClipboard(.standard, &.{.{
                    //     .mime = "text/plain",
                    //     .data = url_text,
                    // }}, false) catch |err| {
                    //     log.err("error copying url to clipboard err={}", .{err});
                    //     return false;
                    // };
                    if let Err(err) = unsafe { &*self.rt_surface }.set_clipboard(
                        apprt::Clipboard::Standard,
                        &[apprt::ClipboardContent {
                            mime: b"text/plain".as_slice(),
                            data: &url_text,
                        }],
                        false,
                    ) {
                        // log.err("error copying url to clipboard err={}", .{err});
                        log::error!(target: LOG_SCOPE, "error copying url to clipboard err={err:?}");
                        unsafe { (*self.renderer_state.mutex).unlock() };
                        return Ok(false);
                    }

                    // return true;
                    unsafe { (*self.renderer_state.mutex).unlock() };
                    return Ok(true);
                }

                // return false;
                unsafe { (*self.renderer_state.mutex).unlock() };
                return Ok(false);
            }

            // .copy_title_to_clipboard => return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .copy_title_to_clipboard,
            //     {},
            // ),
            binding::ScopedSurface::CopyTitleToClipboard => {
                return Ok(unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::Surface(&*self),
                    apprt::Action::CopyTitleToClipboard,
                )?);
            }

            // .paste_from_clipboard => return (try self.startClipboardRequest(
            //     .standard,
            //     .{ .paste = .standard },
            // )) == .started,
            binding::ScopedSurface::PasteFromClipboard => {
                return Ok(self.startClipboardRequest(
                    apprt::Clipboard::Standard,
                    apprt::ClipboardRequest::Paste(apprt::Clipboard::Standard),
                )? == apprt::ClipboardReadResult::Started);
            }

            // .paste_from_selection => return (try self.startClipboardRequest(
            //     .selection,
            //     .{ .paste = .selection },
            // )) == .started,
            binding::ScopedSurface::PasteFromSelection => {
                return Ok(self.startClipboardRequest(
                    apprt::Clipboard::Selection,
                    apprt::ClipboardRequest::Paste(apprt::Clipboard::Selection),
                )? == apprt::ClipboardReadResult::Started);
            }

            // .increase_font_size => |delta| {
            binding::ScopedSurface::IncreaseFontSize(delta) => {
                // Max delta is somewhat arbitrary.
                // const clamped_delta = @max(0, @min(255, delta));
                //
                // PORT NOTE: kept as nested max/min (NOT `.clamp()`): Zig
                // `@max`/`@min` return the non-NaN operand on NaN input (so a
                // NaN `delta` clamps to 255), which Rust `f32::max`/`f32::min`
                // match but `f32::clamp` does NOT (it returns NaN) — the
                // manual-clamp lint is allowed to preserve that semantics.
                #[allow(clippy::manual_clamp)]
                let clamped_delta = f32::max(0.0, f32::min(255.0, delta));

                // log.debug("increase font size={}", .{clamped_delta});
                log::debug!(target: LOG_SCOPE, "increase font size={clamped_delta}");

                // Max point size is somewhat arbitrary.
                // var size = self.font_size;
                let mut size = self.font_size;
                // size.points = @min(size.points + clamped_delta, 255);
                size.points = f32::min(size.points + clamped_delta, 255.0);
                // try self.setFontSize(size);
                self.setFontSize(size)?;

                // Mark that we manually adjusted the font size
                // self.font_size_adjusted = true;
                self.font_size_adjusted = true;
            }

            // .decrease_font_size => |delta| {
            binding::ScopedSurface::DecreaseFontSize(delta) => {
                // Max delta is somewhat arbitrary.
                // const clamped_delta = @max(0, @min(255, delta));
                //
                // PORT NOTE: kept as nested max/min (NOT `.clamp()`): Zig
                // `@max`/`@min` return the non-NaN operand on NaN input (so a
                // NaN `delta` clamps to 255), which Rust `f32::max`/`f32::min`
                // match but `f32::clamp` does NOT (it returns NaN) — the
                // manual-clamp lint is allowed to preserve that semantics.
                #[allow(clippy::manual_clamp)]
                let clamped_delta = f32::max(0.0, f32::min(255.0, delta));

                // log.debug("decrease font size={}", .{clamped_delta});
                log::debug!(target: LOG_SCOPE, "decrease font size={clamped_delta}");

                // var size = self.font_size;
                let mut size = self.font_size;
                // size.points = @max(1, size.points - clamped_delta);
                size.points = f32::max(1.0, size.points - clamped_delta);
                // try self.setFontSize(size);
                self.setFontSize(size)?;

                // Mark that we manually adjusted the font size
                // self.font_size_adjusted = true;
                self.font_size_adjusted = true;
            }

            // .reset_font_size => {
            binding::ScopedSurface::ResetFontSize => {
                // log.debug("reset font size", .{});
                log::debug!(target: LOG_SCOPE, "reset font size");

                // var size = self.font_size;
                let mut size = self.font_size;
                // size.points = self.config.original_font_size;
                size.points = self.config.original_font_size;
                // try self.setFontSize(size);
                self.setFontSize(size)?;

                // Reset font size also resets the manual adjustment state
                // self.font_size_adjusted = false;
                self.font_size_adjusted = false;
            }

            // .set_font_size => |points| {
            binding::ScopedSurface::SetFontSize(points) => {
                // log.debug("set font size={d}", .{points});
                log::debug!(target: LOG_SCOPE, "set font size={points}");

                // var size = self.font_size;
                let mut size = self.font_size;
                // size.points = std.math.clamp(points, 1.0, 255.0);
                size.points = points.clamp(1.0, 255.0);
                // try self.setFontSize(size);
                self.setFontSize(size)?;

                // Mark that we manually adjusted the font size
                // self.font_size_adjusted = true;
                self.font_size_adjusted = true;
            }

            // .prompt_surface_title => return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .prompt_title,
            //     .surface,
            // ),
            binding::ScopedSurface::PromptSurfaceTitle => {
                return Ok(unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::Surface(&*self),
                    apprt::Action::PromptTitle(PromptTitle::Surface),
                )?);
            }

            // .prompt_tab_title => return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .prompt_title,
            //     .tab,
            // ),
            binding::ScopedSurface::PromptTabTitle => {
                return Ok(unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::Surface(&*self),
                    apprt::Action::PromptTitle(PromptTitle::Tab),
                )?);
            }

            // .set_surface_title => |v| {
            binding::ScopedSurface::SetSurfaceTitle(v) => {
                // const title = try self.alloc.dupeZ(u8, v);
                // defer self.alloc.free(title);
                //
                // PORT NOTE: the `dupeZ` NUL-termination collapses —
                // `SetTitle.title` models `[:0]const u8` as borrowed `&[u8]`
                // (the sentinel is deferred C-ABI pinning, apprt-api action.rs),
                // so the owned copy is unnecessary and the borrowed binding
                // bytes are passed directly to the synchronous performAction.
                // return try self.rt_app.performAction(
                //     .{ .surface = self },
                //     .set_title,
                //     .{ .title = title },
                // );
                return Ok(unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::Surface(&*self),
                    apprt::Action::SetTitle(SetTitle { title: v }),
                )?);
            }

            // .prompt_window_title => return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .prompt_title,
            //     .window,
            // ),
            binding::ScopedSurface::PromptWindowTitle => {
                return Ok(unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::Surface(&*self),
                    apprt::Action::PromptTitle(PromptTitle::Window),
                )?);
            }

            // .set_tab_title => |v| {
            binding::ScopedSurface::SetTabTitle(v) => {
                // const title = try self.alloc.dupeZ(u8, v);
                // defer self.alloc.free(title);
                //
                // PORT NOTE: see `.set_surface_title` — the `dupeZ` collapses
                // into the deferred C-ABI sentinel; the borrowed bytes pass
                // directly.
                // return try self.rt_app.performAction(
                //     .{ .surface = self },
                //     .set_tab_title,
                //     .{ .title = title },
                // );
                return Ok(unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::Surface(&*self),
                    apprt::Action::SetTabTitle(SetTitle { title: v }),
                )?);
            }

            // .set_window_title => |v| {
            binding::ScopedSurface::SetWindowTitle(v) => {
                // const title = try self.alloc.dupeZ(u8, v);
                // defer self.alloc.free(title);
                //
                // PORT NOTE: see `.set_surface_title` — the `dupeZ` collapses
                // into the deferred C-ABI sentinel; the borrowed bytes pass
                // directly.
                // return try self.rt_app.performAction(
                //     .{ .surface = self },
                //     .set_window_title,
                //     .{ .title = title },
                // );
                return Ok(unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::Surface(&*self),
                    apprt::Action::SetWindowTitle(SetTitle { title: v }),
                )?);
            }

            // .clear_screen => {
            binding::ScopedSurface::ClearScreen => {
                // This is a duplicate of some of the logic in termio.clearScreen
                // but we need to do this here so we can know the answer before
                // we send the message. If the currently active screen is on the
                // alternate screen then clear screen does nothing so we want to
                // return false so the keybind can be unconsumed.
                // {
                //     self.renderer_state.mutex.lock();
                //     defer self.renderer_state.mutex.unlock();
                //     if (self.io.terminal.screens.active_key == .alternate) return false;
                // }
                {
                    unsafe { (*self.renderer_state.mutex).lock() };
                    if self.io.terminal.screens.active_key == terminal::screen_set::Key::Alternate {
                        unsafe { (*self.renderer_state.mutex).unlock() };
                        return Ok(false);
                    }
                    unsafe { (*self.renderer_state.mutex).unlock() };
                }

                // self.queueIo(.{
                //     .clear_screen = .{ .history = true },
                // }, .unlocked);
                self.queueIo(
                    termio::Message::ClearScreen { history: true },
                    termio::termio_impl::MutexState::Unlocked,
                );
            }

            // .scroll_to_top => {
            binding::ScopedSurface::ScrollToTop => {
                // self.queueIo(.{
                //     .scroll_viewport = .{ .top = {} },
                // }, .unlocked);
                self.queueIo(
                    termio::Message::ScrollViewport(terminal::terminal::ScrollViewport::Top),
                    termio::termio_impl::MutexState::Unlocked,
                );
            }

            // .scroll_to_bottom => {
            binding::ScopedSurface::ScrollToBottom => {
                // self.queueIo(.{
                //     .scroll_viewport = .{ .bottom = {} },
                // }, .unlocked);
                self.queueIo(
                    termio::Message::ScrollViewport(terminal::terminal::ScrollViewport::Bottom),
                    termio::termio_impl::MutexState::Unlocked,
                );
            }

            // .scroll_to_row => |n| {
            binding::ScopedSurface::ScrollToRow(n) => {
                // {
                //     self.renderer_state.mutex.lock();
                //     defer self.renderer_state.mutex.unlock();
                //     const t: *terminal.Terminal = self.renderer_state.terminal;
                //     t.screens.active.scroll(.{ .row = n });
                // }
                {
                    unsafe { (*self.renderer_state.mutex).lock() };
                    let t: *mut Terminal = self.renderer_state.terminal;
                    unsafe { (*(*t).screens.active).scroll(terminal::screen::Scroll::Row(n)) };
                    unsafe { (*self.renderer_state.mutex).unlock() };
                }

                // try self.queueRender();
                self.queueRender()?;
            }

            // .scroll_to_selection => {
            binding::ScopedSurface::ScrollToSelection => {
                // {
                //     self.renderer_state.mutex.lock();
                //     defer self.renderer_state.mutex.unlock();
                //     const sel = self.io.terminal.screens.active.selection orelse return false;
                //     const tl = sel.topLeft(self.io.terminal.screens.active);
                //     self.io.terminal.screens.active.scroll(.{ .pin = tl });
                // }
                {
                    unsafe { (*self.renderer_state.mutex).lock() };
                    let sel = match unsafe { (*self.io.terminal.screens.active).selection } {
                        Some(sel) => sel,
                        None => {
                            unsafe { (*self.renderer_state.mutex).unlock() };
                            return Ok(false);
                        }
                    };
                    let tl = sel.top_left(unsafe { &*self.io.terminal.screens.active });
                    unsafe {
                        (*self.io.terminal.screens.active).scroll(terminal::screen::Scroll::Pin(tl))
                    };
                    unsafe { (*self.renderer_state.mutex).unlock() };
                }

                // try self.queueRender();
                self.queueRender()?;
            }

            // .scroll_page_up => {
            binding::ScopedSurface::ScrollPageUp => {
                // const rows: isize = @intCast(self.size.grid().rows);
                let rows: isize = self.size.grid().rows as isize;
                // self.queueIo(.{
                //     .scroll_viewport = .{ .delta = -1 * rows },
                // }, .unlocked);
                //
                // (`-1 * rows` -> `-rows`; clippy rejects the literal-multiply.)
                self.queueIo(
                    termio::Message::ScrollViewport(terminal::terminal::ScrollViewport::Delta(
                        -rows,
                    )),
                    termio::termio_impl::MutexState::Unlocked,
                );
            }

            // .scroll_page_down => {
            binding::ScopedSurface::ScrollPageDown => {
                // const rows: isize = @intCast(self.size.grid().rows);
                let rows: isize = self.size.grid().rows as isize;
                // self.queueIo(.{
                //     .scroll_viewport = .{ .delta = rows },
                // }, .unlocked);
                self.queueIo(
                    termio::Message::ScrollViewport(terminal::terminal::ScrollViewport::Delta(
                        rows,
                    )),
                    termio::termio_impl::MutexState::Unlocked,
                );
            }

            // .scroll_page_fractional => |fraction| {
            binding::ScopedSurface::ScrollPageFractional(fraction) => {
                // const rows: f32 = @floatFromInt(self.size.grid().rows);
                let rows: f32 = f32::from(self.size.grid().rows);
                // const delta: isize = @intFromFloat(@trunc(fraction * rows));
                // (f32 site: `f64::from` is exact, so the f64 helper checks the
                // identical value.)
                //
                // PORT NOTE (R13, UD62): a fraction the `isize` cannot hold means
                // "scroll to the end", which `scroll_viewport` clamps to the top
                // or the active area; the reference traps instead.
                let delta: isize =
                    isize_from_float_saturating(f64::from((fraction * rows).trunc()));
                // self.queueIo(.{
                //     .scroll_viewport = .{ .delta = delta },
                // }, .unlocked);
                self.queueIo(
                    termio::Message::ScrollViewport(terminal::terminal::ScrollViewport::Delta(
                        delta,
                    )),
                    termio::termio_impl::MutexState::Unlocked,
                );
            }

            // .scroll_page_lines => |lines| {
            binding::ScopedSurface::ScrollPageLines(lines) => {
                // self.queueIo(.{
                //     .scroll_viewport = .{ .delta = lines },
                // }, .unlocked);
                //
                // (`lines: i16` widens to the `delta: isize` field.)
                self.queueIo(
                    termio::Message::ScrollViewport(terminal::terminal::ScrollViewport::Delta(
                        isize::from(lines),
                    )),
                    termio::termio_impl::MutexState::Unlocked,
                );
            }

            // .jump_to_prompt => |delta| {
            binding::ScopedSurface::JumpToPrompt(delta) => {
                // self.queueIo(.{
                //     .jump_to_prompt = @intCast(delta),
                // }, .unlocked);
                self.queueIo(
                    termio::Message::JumpToPrompt(isize::from(delta)),
                    termio::termio_impl::MutexState::Unlocked,
                );
            }

            // .write_screen_file => |v| try self.writeScreenFile(
            //     .screen,
            //     v,
            // ),
            binding::ScopedSurface::WriteScreenFile(v) => {
                self.writeScreenFile(WriteScreenLoc::Screen, v)?
            }

            // .write_scrollback_file => |v| try self.writeScreenFile(
            //     .history,
            //     v,
            // ),
            binding::ScopedSurface::WriteScrollbackFile(v) => {
                self.writeScreenFile(WriteScreenLoc::History, v)?
            }

            // .write_selection_file => |v| try self.writeScreenFile(
            //     .selection,
            //     v,
            // ),
            binding::ScopedSurface::WriteSelectionFile(v) => {
                self.writeScreenFile(WriteScreenLoc::Selection, v)?
            }

            // .new_tab => return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .new_tab,
            //     {},
            // ),
            binding::ScopedSurface::NewTab => {
                return Ok(unsafe { &*self.rt_app }
                    .perform_action(apprt::Target::Surface(&*self), apprt::Action::NewTab)?);
            }

            // .close_tab => |v| return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .close_tab,
            //     switch (v) {
            //         .this => .this,
            //         .other => .other,
            //         .right => .right,
            //     },
            // ),
            binding::ScopedSurface::CloseTab(v) => {
                return Ok(unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::Surface(&*self),
                    apprt::Action::CloseTab(match v {
                        binding::CloseTabMode::This => apprt::action::CloseTabMode::This,
                        binding::CloseTabMode::Other => apprt::action::CloseTabMode::Other,
                        binding::CloseTabMode::Right => apprt::action::CloseTabMode::Right,
                    }),
                )?);
            }

            // inline .previous_tab,
            // .next_tab,
            // .last_tab,
            // .goto_tab,
            // => |v, tag| return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .goto_tab,
            //     switch (tag) {
            //         .previous_tab => .previous,
            //         .next_tab => .next,
            //         .last_tab => .last,
            //         .goto_tab => @enumFromInt(v),
            //         else => comptime unreachable,
            //     },
            // ),
            //
            // (the `inline` capture's `v` payload + `tag` — the ScopedSurface
            // value is re-projected to recover both, mirroring `|v, tag|`.)
            sfc @ (binding::ScopedSurface::PreviousTab
            | binding::ScopedSurface::NextTab
            | binding::ScopedSurface::LastTab
            | binding::ScopedSurface::GotoTab(_)) => {
                return Ok(unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::Surface(&*self),
                    apprt::Action::GotoTab(match sfc {
                        binding::ScopedSurface::PreviousTab => apprt::action::GotoTab::PREVIOUS,
                        binding::ScopedSurface::NextTab => apprt::action::GotoTab::NEXT,
                        binding::ScopedSurface::LastTab => apprt::action::GotoTab::LAST,
                        // PORT NOTE: reference defect (R13, UPSTREAM-DEFECTS.md
                        // UD58). The reference's `@enumFromInt(v)` narrows the
                        // keybind's `usize` into `GotoTab = enum(c_int)`: above
                        // `c_int` max, ReleaseSafe panics and ReleaseFast
                        // truncates, so `goto_tab:4294967295` becomes `.previous`.
                        // The port rejects such a value where the keybind is
                        // parsed (`binding::GOTO_TAB_MAX`), as the parser rejects
                        // any other integer argument its type cannot hold. Here the
                        // value fits by construction, so the conversion is checked
                        // (R7) rather than `as`, which wrapped (LEDGER S42-F4).
                        binding::ScopedSurface::GotoTab(v) => apprt::action::GotoTab(
                            i32::try_from(v)
                                .expect("goto_tab index bounded by binding::GOTO_TAB_MAX at parse"),
                        ),
                        // else => comptime unreachable,
                        _ => unreachable!(),
                    }),
                )?);
            }

            // .move_tab => |position| return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .move_tab,
            //     .{ .amount = position },
            // ),
            binding::ScopedSurface::MoveTab(position) => {
                return Ok(unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::Surface(&*self),
                    apprt::Action::MoveTab(apprt::action::MoveTab { amount: position }),
                )?);
            }

            // .move_tab_to_new_window => return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .move_tab_to_new_window,
            //     {},
            // ),
            binding::ScopedSurface::MoveTabToNewWindow => {
                return Ok(unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::Surface(&*self),
                    apprt::Action::MoveTabToNewWindow,
                )?);
            }

            // .new_split => |direction| return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .new_split,
            //     switch (direction) {
            //         .right => .right,
            //         .left => .left,
            //         .down => .down,
            //         .up => .up,
            //         .auto => if (self.size.screen.width > self.size.screen.height)
            //             .right
            //         else
            //             .down,
            //     },
            // ),
            binding::ScopedSurface::NewSplit(direction) => {
                return Ok(unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::Surface(&*self),
                    apprt::Action::NewSplit(match direction {
                        binding::SplitDirection::Right => apprt::action::SplitDirection::Right,
                        binding::SplitDirection::Left => apprt::action::SplitDirection::Left,
                        binding::SplitDirection::Down => apprt::action::SplitDirection::Down,
                        binding::SplitDirection::Up => apprt::action::SplitDirection::Up,
                        binding::SplitDirection::Auto => {
                            if self.size.screen.width > self.size.screen.height {
                                apprt::action::SplitDirection::Right
                            } else {
                                apprt::action::SplitDirection::Down
                            }
                        }
                    }),
                )?);
            }

            // .goto_split => |direction| return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .goto_split,
            //     switch (direction) {
            //         inline else => |tag| @field(
            //             apprt.action.GotoSplit,
            //             @tagName(tag),
            //         ),
            //     },
            // ),
            //
            // (`inline else => @field(GotoSplit, @tagName(tag))` maps each
            // binding tag onto the same-named apprt `GotoSplit` variant.)
            binding::ScopedSurface::GotoSplit(direction) => {
                return Ok(unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::Surface(&*self),
                    apprt::Action::GotoSplit(match direction {
                        binding::SplitFocusDirection::Previous => {
                            apprt::action::GotoSplit::Previous
                        }
                        binding::SplitFocusDirection::Next => apprt::action::GotoSplit::Next,
                        binding::SplitFocusDirection::Up => apprt::action::GotoSplit::Up,
                        binding::SplitFocusDirection::Left => apprt::action::GotoSplit::Left,
                        binding::SplitFocusDirection::Down => apprt::action::GotoSplit::Down,
                        binding::SplitFocusDirection::Right => apprt::action::GotoSplit::Right,
                    }),
                )?);
            }

            // .goto_window => |direction| return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .goto_window,
            //     switch (direction) {
            //         .previous => .previous,
            //         .next => .next,
            //     },
            // ),
            binding::ScopedSurface::GotoWindow(direction) => {
                return Ok(unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::Surface(&*self),
                    apprt::Action::GotoWindow(match direction {
                        binding::GotoWindow::Previous => apprt::action::GotoWindow::Previous,
                        binding::GotoWindow::Next => apprt::action::GotoWindow::Next,
                    }),
                )?);
            }

            // .resize_split => |value| return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .resize_split,
            //     .{
            //         .amount = value[1],
            //         .direction = switch (value[0]) {
            //             inline else => |tag| @field(
            //                 apprt.action.ResizeSplit.Direction,
            //                 @tagName(tag),
            //             ),
            //         },
            //     },
            // ),
            binding::ScopedSurface::ResizeSplit(value) => {
                return Ok(unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::Surface(&*self),
                    apprt::Action::ResizeSplit(apprt::action::ResizeSplit {
                        amount: value.1,
                        direction: match value.0 {
                            binding::SplitResizeDirection::Up => {
                                apprt::action::ResizeSplitDirection::Up
                            }
                            binding::SplitResizeDirection::Down => {
                                apprt::action::ResizeSplitDirection::Down
                            }
                            binding::SplitResizeDirection::Left => {
                                apprt::action::ResizeSplitDirection::Left
                            }
                            binding::SplitResizeDirection::Right => {
                                apprt::action::ResizeSplitDirection::Right
                            }
                        },
                    }),
                )?);
            }

            // .equalize_splits => return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .equalize_splits,
            //     {},
            // ),
            binding::ScopedSurface::EqualizeSplits => {
                return Ok(unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::Surface(&*self),
                    apprt::Action::EqualizeSplits,
                )?);
            }

            // .toggle_split_zoom => return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .toggle_split_zoom,
            //     {},
            // ),
            binding::ScopedSurface::ToggleSplitZoom => {
                return Ok(unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::Surface(&*self),
                    apprt::Action::ToggleSplitZoom,
                )?);
            }

            // .toggle_readonly => {
            binding::ScopedSurface::ToggleReadonly => {
                // self.readonly = !self.readonly;
                self.readonly = !self.readonly;
                // _ = try self.rt_app.performAction(
                //     .{ .surface = self },
                //     .readonly,
                //     if (self.readonly) .on else .off,
                // );
                let _ = unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::Surface(&*self),
                    apprt::Action::Readonly(if self.readonly {
                        apprt::action::Readonly::On
                    } else {
                        apprt::action::Readonly::Off
                    }),
                )?;
                // return true;
                return Ok(true);
            }

            // .reset_window_size => return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .reset_window_size,
            //     {},
            // ),
            binding::ScopedSurface::ResetWindowSize => {
                return Ok(unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::Surface(&*self),
                    apprt::Action::ResetWindowSize,
                )?);
            }

            // .toggle_maximize => return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .toggle_maximize,
            //     {},
            // ),
            binding::ScopedSurface::ToggleMaximize => {
                return Ok(unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::Surface(&*self),
                    apprt::Action::ToggleMaximize,
                )?);
            }

            // .toggle_fullscreen => return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .toggle_fullscreen,
            //     switch (self.config.macos_non_native_fullscreen) {
            //         .false => .native,
            //         .true => .macos_non_native,
            //         .@"visible-menu" => .macos_non_native_visible_menu,
            //         .@"padded-notch" => .macos_non_native_padded_notch,
            //     },
            // ),
            binding::ScopedSurface::ToggleFullscreen => {
                return Ok(unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::Surface(&*self),
                    apprt::Action::ToggleFullscreen(
                        match self.config.macos_non_native_fullscreen {
                            configpkg::value_types::NonNativeFullscreen::False => {
                                apprt::action::Fullscreen::Native
                            }
                            configpkg::value_types::NonNativeFullscreen::True => {
                                apprt::action::Fullscreen::MacosNonNative
                            }
                            configpkg::value_types::NonNativeFullscreen::VisibleMenu => {
                                apprt::action::Fullscreen::MacosNonNativeVisibleMenu
                            }
                            configpkg::value_types::NonNativeFullscreen::PaddedNotch => {
                                apprt::action::Fullscreen::MacosNonNativePaddedNotch
                            }
                        },
                    ),
                )?);
            }

            // .toggle_window_decorations => return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .toggle_window_decorations,
            //     {},
            // ),
            binding::ScopedSurface::ToggleWindowDecorations => {
                return Ok(unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::Surface(&*self),
                    apprt::Action::ToggleWindowDecorations,
                )?);
            }

            // .toggle_tab_overview => return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .toggle_tab_overview,
            //     {},
            // ),
            binding::ScopedSurface::ToggleTabOverview => {
                return Ok(unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::Surface(&*self),
                    apprt::Action::ToggleTabOverview,
                )?);
            }

            // .toggle_window_float_on_top => return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .float_window,
            //     .toggle,
            // ),
            binding::ScopedSurface::ToggleWindowFloatOnTop => {
                return Ok(unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::Surface(&*self),
                    apprt::Action::FloatWindow(apprt::action::FloatWindow::Toggle),
                )?);
            }

            // .toggle_secure_input => return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .secure_input,
            //     .toggle,
            // ),
            binding::ScopedSurface::ToggleSecureInput => {
                return Ok(unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::Surface(&*self),
                    apprt::Action::SecureInput(SecureInput::Toggle),
                )?);
            }

            // .toggle_mouse_reporting => {
            binding::ScopedSurface::ToggleMouseReporting => {
                // self.config.mouse_reporting = !self.config.mouse_reporting;
                self.config.mouse_reporting = !self.config.mouse_reporting;
                // log.debug("mouse reporting toggled: {}", .{self.config.mouse_reporting});
                log::debug!(
                    target: LOG_SCOPE,
                    "mouse reporting toggled: {}",
                    self.config.mouse_reporting
                );
            }

            // .toggle_command_palette => return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .toggle_command_palette,
            //     {},
            // ),
            binding::ScopedSurface::ToggleCommandPalette => {
                return Ok(unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::Surface(&*self),
                    apprt::Action::ToggleCommandPalette,
                )?);
            }

            // .toggle_background_opacity => return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .toggle_background_opacity,
            //     {},
            // ),
            binding::ScopedSurface::ToggleBackgroundOpacity => {
                return Ok(unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::Surface(&*self),
                    apprt::Action::ToggleBackgroundOpacity,
                )?);
            }

            // .show_on_screen_keyboard => return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .show_on_screen_keyboard,
            //     {},
            // ),
            binding::ScopedSurface::ShowOnScreenKeyboard => {
                return Ok(unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::Surface(&*self),
                    apprt::Action::ShowOnScreenKeyboard,
                )?);
            }

            // .select_all => {
            binding::ScopedSurface::SelectAll => {
                // self.renderer_state.mutex.lock();
                // defer self.renderer_state.mutex.unlock();
                unsafe { (*self.renderer_state.mutex).lock() };

                // const sel = self.io.terminal.screens.active.selectAll();
                let sel = unsafe { (*self.io.terminal.screens.active).select_all() };
                // if (sel) |s| {
                if let Some(s) = sel {
                    // try self.setSelectionAndCopy(s);
                    if let Err(err) = self.setSelectionAndCopy(s) {
                        unsafe { (*self.renderer_state.mutex).unlock() };
                        return Err(err);
                    }
                    // try self.queueRender();
                    if let Err(err) = self.queueRender() {
                        unsafe { (*self.renderer_state.mutex).unlock() };
                        return Err(err.into());
                    }
                }
                unsafe { (*self.renderer_state.mutex).unlock() };
            }

            // .inspector => |mode| return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .inspector,
            //     switch (mode) {
            //         inline else => |tag| @field(
            //             apprt.action.Inspector,
            //             @tagName(tag),
            //         ),
            //     },
            // ),
            binding::ScopedSurface::Inspector(mode) => {
                return Ok(unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::Surface(&*self),
                    apprt::Action::Inspector(match mode {
                        binding::InspectorMode::Toggle => apprt::action::Inspector::Toggle,
                        binding::InspectorMode::Show => apprt::action::Inspector::Show,
                        binding::InspectorMode::Hide => apprt::action::Inspector::Hide,
                    }),
                )?);
            }

            // .close_surface => self.close(),
            binding::ScopedSurface::CloseSurface => self.close(),

            // .close_window => return try self.rt_app.performAction(
            //     .{ .surface = self },
            //     .close_window,
            //     {},
            // ),
            binding::ScopedSurface::CloseWindow => {
                return Ok(unsafe { &*self.rt_app }
                    .perform_action(apprt::Target::Surface(&*self), apprt::Action::CloseWindow)?);
            }

            // inline .activate_key_table,
            // .activate_key_table_once,
            // => |name, tag| {
            //
            // (the `inline` capture's `name` payload + `tag` — recovered by
            // re-projecting the ScopedSurface value, mirroring `|name, tag|`.)
            sfc @ (binding::ScopedSurface::ActivateKeyTable(_)
            | binding::ScopedSurface::ActivateKeyTableOnce(_)) => {
                let (name, is_once): (&[u8], bool) = match sfc {
                    binding::ScopedSurface::ActivateKeyTable(name) => (name, false),
                    // tag == .activate_key_table_once
                    binding::ScopedSurface::ActivateKeyTableOnce(name) => (name, true),
                    _ => unreachable!(),
                };

                // Look up the table in our config
                // const set = self.config.keybind.tables.getPtr(name) orelse {
                //     log.debug("key table not found: {s}", .{name});
                //     return false;
                // };
                //
                // PORT NOTE: `getPtr` (`?*Set`) -> `get` (`?&Set` cast to a
                // raw `*const Set`) — the stack only STORES and compares the
                // pointer, never mutates through it, so the shared borrow is
                // sufficient. The `Vec<u8>`-keyed map takes `&Vec<u8>`; the
                // `[]const u8` name is copied into a lookup key (a small
                // temporary, behavior-preserving).
                let set: *const binding::Set = match self.config.keybind.tables.get(&name.to_vec())
                {
                    Some(s) => s as *const binding::Set,
                    None => {
                        // log.debug("key table not found: {s}", .{name});
                        log::debug!(
                            target: LOG_SCOPE,
                            "key table not found: {}",
                            String::from_utf8_lossy(name)
                        );
                        return Ok(false);
                    }
                };

                // If this is the same table as is currently active, then
                // do nothing.
                // if (self.keyboard.table_stack.items.len > 0) {
                if !self.keyboard.table_stack.is_empty() {
                    // const items = self.keyboard.table_stack.items;
                    // const active = items[items.len - 1].set;
                    let active = self.keyboard.table_stack[self.keyboard.table_stack.len() - 1].set;
                    // if (active == set) {
                    //     log.debug("ignoring duplicate activate table: {s}", .{name});
                    //     return false;
                    // }
                    if std::ptr::eq(active, set) {
                        // log.debug("ignoring duplicate activate table: {s}", .{name});
                        log::debug!(
                            target: LOG_SCOPE,
                            "ignoring duplicate activate table: {}",
                            String::from_utf8_lossy(name)
                        );
                        return Ok(false);
                    }
                }

                // If we're already at the max, ignore it.
                // if (self.keyboard.table_stack.items.len >= max_active_key_tables) {
                //     log.info(
                //         "ignoring activate table, max depth reached: {s}",
                //         .{name},
                //     );
                //     return false;
                // }
                if self.keyboard.table_stack.len() >= MAX_ACTIVE_KEY_TABLES {
                    // log.info("ignoring activate table, max depth reached: {s}", .{name});
                    log::info!(
                        target: LOG_SCOPE,
                        "ignoring activate table, max depth reached: {}",
                        String::from_utf8_lossy(name)
                    );
                    return Ok(false);
                }

                // Add the table to the stack.
                // try self.keyboard.table_stack.append(self.alloc, .{
                //     .set = set,
                //     .once = tag == .activate_key_table_once,
                // });
                self.keyboard
                    .table_stack
                    .push(TableStackEntry { set, once: is_once });

                // Notify the UI.
                // _ = self.rt_app.performAction(
                //     .{ .surface = self },
                //     .key_table,
                //     .{ .activate = name },
                // ) catch |err| {
                //     log.warn(
                //         "failed to notify app of key table err={}",
                //         .{err},
                //     );
                // };
                if let Err(err) = unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::Surface(&*self),
                    apprt::Action::KeyTable(KeyTable::Activate(name)),
                ) {
                    // log.warn("failed to notify app of key table err={}", .{err});
                    log::warn!(target: LOG_SCOPE, "failed to notify app of key table err={err:?}");
                }

                // log.debug("key table activated: {s}", .{name});
                log::debug!(
                    target: LOG_SCOPE,
                    "key table activated: {}",
                    String::from_utf8_lossy(name)
                );
            }

            // .deactivate_key_table => {
            binding::ScopedSurface::DeactivateKeyTable => {
                // switch (self.keyboard.table_stack.items.len) {
                match self.keyboard.table_stack.len() {
                    // No key table active. This does nothing.
                    // 0 => return false,
                    0 => return Ok(false),

                    // Final key table active, clear our state.
                    // 1 => self.keyboard.table_stack.clearAndFree(self.alloc),
                    //
                    // (`clearAndFree` — drop the storage too, the deinit
                    // mem::take precedent.)
                    1 => drop(std::mem::take(&mut self.keyboard.table_stack)),

                    // Restore the prior key table. We don't free any memory in
                    // this case because we assume it will be freed later when
                    // we finish our key table.
                    // else => _ = self.keyboard.table_stack.pop(),
                    _ => {
                        let _ = self.keyboard.table_stack.pop();
                    }
                }

                // Notify the UI.
                // _ = self.rt_app.performAction(
                //     .{ .surface = self },
                //     .key_table,
                //     .deactivate,
                // ) catch |err| {
                //     log.warn(
                //         "failed to notify app of key table err={}",
                //         .{err},
                //     );
                // };
                if let Err(err) = unsafe { &*self.rt_app }.perform_action(
                    apprt::Target::Surface(&*self),
                    apprt::Action::KeyTable(KeyTable::Deactivate),
                ) {
                    // log.warn("failed to notify app of key table err={}", .{err});
                    log::warn!(target: LOG_SCOPE, "failed to notify app of key table err={err:?}");
                }
            }

            // .deactivate_all_key_tables => {
            //     return try self.deactivateAllKeyTables();
            // },
            binding::ScopedSurface::DeactivateAllKeyTables => {
                return self.deactivateAllKeyTables();
            }

            // .end_key_sequence => {
            binding::ScopedSurface::EndKeySequence => {
                // End the key sequence and flush queued keys to the terminal,
                // but don't encode the key that triggered this action. This
                // will do that because leaf keys (keys with bindings) aren't
                // in the queued encoding list.
                // self.endKeySequence(.flush, .retain);
                self.endKeySequence(KeySequenceQueued::Flush, KeySequenceMemory::Retain);
            }

            // .crash => |location| switch (location) {
            binding::ScopedSurface::Crash(location) => match location {
                // .main => @panic("crash binding action, crashing intentionally"),
                binding::CrashThread::Main => {
                    panic!("crash binding action, crashing intentionally")
                }

                // .render => {
                binding::CrashThread::Render => {
                    // _ = self.renderer_thread.mailbox.push(.{ .crash = {} }, .{ .forever = {} });
                    let _ = unsafe { &*self.renderer_thread.mailbox }
                        .push(rendererpkg::Message::Crash, Timeout::Forever);
                    // self.queueRender() catch |err| {
                    //     // Not a big deal if this fails.
                    //     log.warn("failed to notify renderer of crash message err={}", .{err});
                    // };
                    if let Err(err) = self.queueRender() {
                        // Not a big deal if this fails.
                        // log.warn("failed to notify renderer of crash message err={}", .{err});
                        log::warn!(
                            target: LOG_SCOPE,
                            "failed to notify renderer of crash message err={err:?}"
                        );
                    }
                }

                // .io => self.queueIo(.{ .crash = {} }, .unlocked),
                binding::CrashThread::Io => self.queueIo(
                    termio::Message::Crash,
                    termio::termio_impl::MutexState::Unlocked,
                ),
            },

            // .adjust_selection => |direction| {
            binding::ScopedSurface::AdjustSelection(direction) => {
                // self.renderer_state.mutex.lock();
                // defer self.renderer_state.mutex.unlock();
                unsafe { (*self.renderer_state.mutex).lock() };

                // const screen: *terminal.Screen = self.io.terminal.screens.active;
                let screen: *mut terminal::screen::Screen = self.io.terminal.screens.active;
                // const sel = if (screen.selection) |*sel| sel else {
                //     // If we don't have a selection we do not perform this
                //     // action, allowing the keybind to fall through to the
                //     // terminal.
                //     return false;
                // };
                //
                // PORT NOTE: the reference `sel` is a `*Selection` aliasing
                // `screen.selection` which `sel.adjust(screen, ...)` mutates
                // in place while ALSO passing `screen` as `&Screen` — a
                // `&mut`-field / `&`-parent overlap that is UB in Rust
                // (rust-mut-args memory). We copy the selection out
                // (`Selection: Copy`), adjust the independent copy against
                // `&*screen`, then write it back — behavior-identical because
                // `adjust` reads only `s.pages`, never `s.selection`.
                let mut sel = match unsafe { (*screen).selection } {
                    Some(sel) => sel,
                    None => {
                        unsafe { (*self.renderer_state.mutex).unlock() };
                        return Ok(false);
                    }
                };
                // sel.adjust(screen, switch (direction) {
                //     .left => .left,
                //     .right => .right,
                //     .up => .up,
                //     .down => .down,
                //     .page_up => .page_up,
                //     .page_down => .page_down,
                //     .home => .home,
                //     .end => .end,
                //     .beginning_of_line => .beginning_of_line,
                //     .end_of_line => .end_of_line,
                // });
                sel.adjust(
                    unsafe { &*screen },
                    match direction {
                        binding::AdjustSelection::Left => terminal::selection::Adjustment::Left,
                        binding::AdjustSelection::Right => terminal::selection::Adjustment::Right,
                        binding::AdjustSelection::Up => terminal::selection::Adjustment::Up,
                        binding::AdjustSelection::Down => terminal::selection::Adjustment::Down,
                        binding::AdjustSelection::PageUp => terminal::selection::Adjustment::PageUp,
                        binding::AdjustSelection::PageDown => {
                            terminal::selection::Adjustment::PageDown
                        }
                        binding::AdjustSelection::Home => terminal::selection::Adjustment::Home,
                        binding::AdjustSelection::End => terminal::selection::Adjustment::End,
                        binding::AdjustSelection::BeginningOfLine => {
                            terminal::selection::Adjustment::BeginningOfLine
                        }
                        binding::AdjustSelection::EndOfLine => {
                            terminal::selection::Adjustment::EndOfLine
                        }
                    },
                );
                // (write the adjusted copy back — the reference `*Selection`
                // mutated `screen.selection` in place.)
                unsafe { (*screen).selection = Some(sel) };

                // If the selection endpoint is outside of the current viewpoint,
                // scroll it in to view. Note we always specifically use sel.end
                // because that is what adjust modifies.
                // scroll: {
                'scroll: {
                    // const viewport_tl = screen.pages.getTopLeft(.viewport);
                    let viewport_tl =
                        unsafe { (*screen).pages.get_top_left(terminal::point::Tag::Viewport) };
                    // const viewport_br = screen.pages.getBottomRight(.viewport).?;
                    let viewport_br = unsafe {
                        (*screen)
                            .pages
                            .get_bottom_right(terminal::point::Tag::Viewport)
                    }
                    .unwrap();
                    // if (sel.end().isBetween(viewport_tl, viewport_br))
                    //     break :scroll;
                    if sel.end().is_between(viewport_tl, viewport_br) {
                        break 'scroll;
                    }

                    // Our end point is not within the viewport. If the end
                    // point is after the br then we need to adjust the end so
                    // that it is at the bottom right of the viewport.
                    // const target = if (sel.end().before(viewport_tl))
                    //     sel.end()
                    // else
                    //     sel.end().up(screen.pages.rows - 1) orelse sel.end();
                    let target = if sel.end().before(viewport_tl) {
                        sel.end()
                    } else {
                        sel.end()
                            .up(usize::from(unsafe { (*screen).pages.rows }) - 1)
                            .unwrap_or(sel.end())
                    };

                    // screen.scroll(.{ .pin = target });
                    unsafe { (*screen).scroll(terminal::screen::Scroll::Pin(target)) };
                }

                // Queue a render so its shown
                // screen.dirty.selection = true;
                unsafe { (*screen).dirty.selection = true };
                // try self.queueRender();
                if let Err(err) = self.queueRender() {
                    unsafe { (*self.renderer_state.mutex).unlock() };
                    return Err(err.into());
                }
                unsafe { (*self.renderer_state.mutex).unlock() };
            }
        }

        // return true;
        Ok(true)
    }

    // fn writeScreenFile(
    //     self: *Surface,
    //     loc: WriteScreenLoc,
    //     write_screen: input.Binding.Action.WriteScreen,
    // ) !void {
    //
    // PORT NOTE (chunk C14b forward-declared this skeleton + the file-scope
    // [`WriteScreenLoc`] so C14b's `.write_screen_file`/`.write_scrollback_file`/
    // `.write_selection_file` binding arms compile against a real callee; C15
    // fills the BODY IN PLACE, Surface.zig:5651-5777).
    fn writeScreenFile(
        &mut self,
        loc: WriteScreenLoc,
        write_screen: binding::WriteScreen,
    ) -> Result<(), Error> {
        // Create a temporary directory to store our scrollback.
        // var tmp_dir = try internal_os.TempDir.init();
        // var retain_tmp_dir = false;
        // defer if (retain_tmp_dir) tmp_dir.close(.retain) else tmp_dir.deinit();
        //
        // PORT NOTE (`2b32b5b75`): `internal_os.TempDir` is the struct-file
        // `os/TempDir.zig`, ported to the `tempdir` module
        // (`internal_os::tempdir::TempDir`). The reference defer ALWAYS closes
        // the directory handles; the `retain_tmp_dir` flag — set only after
        // successful dispatch — selects between deleting the tree
        // (`deinit` == `close(.delete)`) and retaining the temp file on disk
        // for the copy/open/paste consumer (`close(.retain)`). [`TmpDirGuard`]'s
        // `Drop` is that defer: every `?` error exit and the null-selection
        // exit run it with `retain` still false (the delete arm), and only the
        // success-with-data path flips `retain` (the
        // [`ThreadStateGuard`] Drop-guard precedent).
        struct TmpDirGuard {
            td: internal_os::tempdir::TempDir,
            retain: bool,
        }
        impl Drop for TmpDirGuard {
            fn drop(&mut self) {
                if self.retain {
                    self.td.close(internal_os::tempdir::CloseMode::Retain);
                } else {
                    self.td.deinit();
                }
            }
        }
        let mut tmp_dir = TmpDirGuard {
            td: internal_os::tempdir::TempDir::init()?,
            retain: false,
        };

        // var filename_buf: [std.fs.max_path_bytes]u8 = undefined;
        // const filename = try std.fmt.bufPrint(
        //     &filename_buf,
        //     "{s}.{s}",
        //     .{
        //         @tagName(loc),
        //         switch (write_screen.emit) {
        //             .plain, .vt => "txt",
        //             .html => "html",
        //         },
        //     },
        // );
        //
        // (`bufPrint` into a fixed [max_path_bytes]u8 -> an owned `String`; the
        // SURFACE_ID `bufPrint` precedent. `@tagName(loc)` is the lowercase
        // enum-tag name.)
        let filename = format!(
            "{}.{}",
            match loc {
                WriteScreenLoc::Screen => "screen",
                WriteScreenLoc::History => "history",
                WriteScreenLoc::Selection => "selection",
            },
            match write_screen.emit {
                binding::WriteScreenFormat::Plain | binding::WriteScreenFormat::Vt => "txt",
                binding::WriteScreenFormat::Html => "html",
            },
        );

        // Open our scrollback file
        // var file = try tmp_dir.dir.createFile(
        //     filename,
        //     switch (builtin.os.tag) {
        //         .windows => .{},
        //         else => .{ .mode = 0o600 },
        //     },
        // );
        // defer file.close();
        //
        // (the rustty-os `createFile(sub_path, read)` collapses the reference
        // `CreateFlags` to `read` — `read = false` in BOTH reference arms; the
        // `.mode = 0o600` security bit is re-applied via `set_permissions`
        // (fchmod on the open handle) on unix — the reference's non-windows
        // `else` arm; file modes are a no-op concept on windows. Byte-identical
        // END state for a fresh temp file (`cfg(unix)` gates the unix-only
        // `PermissionsExt`, matching the `path_to_bytes`/`os_str_from_bytes`
        // helpers). `file.close()` -> the `File` drops at scope end, closing it
        // — BEFORE `tmp_dir` (reverse-declaration drop == Zig defer LIFO).)
        let file = tmp_dir.td.dir().createFile(&filename, false)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        }

        // Screen.dumpString writes byte-by-byte, so buffer it
        // var buf: [4096]u8 = undefined;
        // var file_writer = file.writer(&buf);
        // var buf_writer = &file_writer.interface;
        //
        // (Zig 0.15's buffered `Writer` -> a 4096-capacity `BufWriter`.)
        let mut buf_writer = std::io::BufWriter::with_capacity(4096, file);

        // Write the scrollback contents. This requires a lock.
        // {
        {
            // self.renderer_state.mutex.lock();
            // defer self.renderer_state.mutex.unlock();
            //
            // (guard-less zigstd Mutex — the defer is the explicit unlock at
            // every exit below.)
            unsafe { (*self.renderer_state.mutex).lock() };

            // We only dump history if we have history. We still keep
            // the file and write the empty file to the pty so that this
            // command always works on the primary screen.
            // const pages = &self.io.terminal.screens.active.pages;
            let pages = unsafe { &(*self.io.terminal.screens.active).pages };
            // const sel_: ?terminal.Selection = switch (loc) {
            let sel_: Option<terminal::selection::Selection> = match loc {
                // .history => history: {
                WriteScreenLoc::History => 'history: {
                    // We do not support this for alternate screens
                    // because they don't have scrollback anyways.
                    // if (self.io.terminal.screens.active_key == .alternate) {
                    //     break :history null;
                    // }
                    if self.io.terminal.screens.active_key == terminal::screen_set::Key::Alternate {
                        break 'history None;
                    }

                    // break :history terminal.Selection.init(
                    //     pages.getTopLeft(.history),
                    //     pages.getBottomRight(.history) orelse
                    //         break :history null,
                    //     false,
                    // );
                    let br = match pages.get_bottom_right(terminal::point::Tag::History) {
                        Some(v) => v,
                        None => break 'history None,
                    };
                    Some(terminal::selection::Selection::init(
                        pages.get_top_left(terminal::point::Tag::History),
                        br,
                        false,
                    ))
                }

                // .screen => screen: {
                WriteScreenLoc::Screen => 'screen: {
                    // break :screen terminal.Selection.init(
                    //     pages.getTopLeft(.screen),
                    //     pages.getBottomRight(.screen) orelse
                    //         break :screen null,
                    //     false,
                    // );
                    let br = match pages.get_bottom_right(terminal::point::Tag::Screen) {
                        Some(v) => v,
                        None => break 'screen None,
                    };
                    Some(terminal::selection::Selection::init(
                        pages.get_top_left(terminal::point::Tag::Screen),
                        br,
                        false,
                    ))
                }

                // .selection => self.io.terminal.screens.active.selection,
                WriteScreenLoc::Selection => unsafe {
                    (*self.io.terminal.screens.active).selection
                },
            };

            // const sel = sel_ orelse {
            //     // If we have no selection we have no data so we do nothing.
            //     return;
            // };
            let sel = match sel_ {
                Some(v) => v,
                None => {
                    // If we have no selection we have no data so we do nothing.
                    // (the null-selection exit: unlock, then return — `tmp_dir`
                    // drops with `retain` false, so its `Drop` runs the
                    // reference defer's `tmp_dir.deinit()` arm.)
                    unsafe { (*self.renderer_state.mutex).unlock() };
                    return Ok(());
                }
            };

            // const ScreenFormatter = terminal.formatter.ScreenFormatter;
            use rustty_vt::formatter::ScreenFormatter;
            // var formatter: ScreenFormatter = .init(self.io.terminal.screens.active, .{
            let mut formatter: ScreenFormatter = ScreenFormatter::init(
                self.io.terminal.screens.active,
                terminal::formatter::Options {
                    // .emit = switch (write_screen.emit) {
                    //     .plain => .plain,
                    //     .vt => .vt,
                    //     .html => .html,
                    // },
                    emit: match write_screen.emit {
                        binding::WriteScreenFormat::Plain => terminal::formatter::Format::Plain,
                        binding::WriteScreenFormat::Vt => terminal::formatter::Format::Vt,
                        binding::WriteScreenFormat::Html => terminal::formatter::Format::Html,
                    },
                    // .unwrap = true,
                    unwrap: true,
                    // .trim = false,
                    trim: false,
                    // (the reference omits `codepoint_map`, i.e. the field
                    // default `.{}` = present-but-empty — the formatter.rs
                    // `Options::base` note.)
                    codepoint_map: Some(Vec::new()),
                    // .background = self.io.terminal.colors.background.get(),
                    background: self.io.terminal.colors.background.get(),
                    // .foreground = self.io.terminal.colors.foreground.get(),
                    foreground: self.io.terminal.colors.foreground.get(),
                    // .palette = &self.io.terminal.colors.palette.current,
                    palette: Some(&raw const self.io.terminal.colors.palette.current),
                },
            );
            // formatter.content = .{ .selection = sel.ordered(
            //     self.io.terminal.screens.active,
            //     .forward,
            // ) };
            formatter.content = terminal::formatter::Content::Selection(Some(sel.ordered(
                unsafe { &*self.io.terminal.screens.active },
                terminal::selection::Order::Forward,
            )));
            // try formatter.format(buf_writer);
            if let Err(err) = formatter.format(&mut buf_writer) {
                // (the deferred mutex unlock — the error exit; `tmp_dir` stays
                // armed so its `Drop` reproduces `errdefer tmp_dir.deinit()`.)
                unsafe { (*self.renderer_state.mutex).unlock() };
                return Err(err.into());
            }

            // (the deferred mutex unlock — the normal block exit.)
            unsafe { (*self.renderer_state.mutex).unlock() };
        }
        // try buf_writer.flush();
        {
            use std::io::Write as _;
            buf_writer.flush()?;
        }

        // Get the final path
        // var path_buf: [std.fs.max_path_bytes]u8 = undefined;
        // const path = try tmp_dir.dir.realpath(filename, &path_buf);
        //
        // (rustty-os `realpath` returns an owning `PathBuf`; `path` is its raw
        // bytes — never a lossy String, R3.)
        let path_buf = tmp_dir.td.dir().realpath(&filename)?;
        let path: &[u8] = path_to_bytes(&path_buf);

        // switch (write_screen.action) {
        match write_screen.action {
            // .copy => {
            binding::WriteScreenAction::Copy => {
                // const pathZ = try self.alloc.dupeZ(u8, path);
                // defer self.alloc.free(pathZ);
                //
                // PORT NOTE: `dupeZ` NUL-terminates for the C-ABI; Rustty
                // `ClipboardContent` borrows `&[u8]` and `setClipboard` copies
                // synchronously ("The data is immediately copied"), so `path` is
                // passed directly (the `PwdC` deferred-pinning precedent).
                // try self.rt_surface.setClipboard(.standard, &.{.{
                //     .mime = "text/plain",
                //     .data = pathZ,
                // }}, false);
                unsafe { &*self.rt_surface }.set_clipboard(
                    apprt::Clipboard::Standard,
                    &[apprt::ClipboardContent {
                        mime: b"text/plain",
                        data: path,
                    }],
                    false,
                )?;
            }

            // .open => try self.openUrl(.{
            //     .kind = switch (write_screen.emit) {
            //         .plain, .vt => .text,
            //         .html => .html,
            //     },
            //     .url = path,
            // }),
            binding::WriteScreenAction::Open => {
                self.openUrl(OpenUrl {
                    kind: match write_screen.emit {
                        binding::WriteScreenFormat::Plain | binding::WriteScreenFormat::Vt => {
                            OpenUrlKind::Text
                        }
                        binding::WriteScreenFormat::Html => OpenUrlKind::Html,
                    },
                    url: path,
                })?;
            }

            // .paste => self.queueIo(try termio.Message.writeReq(
            //     self.alloc,
            //     path,
            // ), .unlocked),
            binding::WriteScreenAction::Paste => {
                self.queueIo(
                    termio::Message::writeReq(path),
                    termio::termio_impl::MutexState::Unlocked,
                );
            }
        }

        // The action accepted the path, so retain the file for its consumer.
        // retain_tmp_dir = true;
        //
        // (the guard drop now takes the `close(.retain)` arm — handles
        // released, the temp file left on disk for the copy/open/paste
        // consumer.)
        tmp_dir.retain = true;
        Ok(())
    }

    // (the module-scope `CompleteClipboard` payload + `clipboardTextContent`
    // helper this consumes are declared after this impl block — Rust cannot
    // nest module items inside impls.)
    /// Call this to complete a clipboard request sent to apprt. This should
    /// only be called once for each request.
    ///
    /// If `complete.confirmed` is false then this may return either an
    /// UnsafePaste or UnauthorizedPaste error, depending on the type of
    /// clipboard request. The request state remains alive in that case so
    /// the apprt can run its confirmation flow.
    // pub fn completeClipboardRequest(
    //     self: *Surface,
    //     req: apprt.ClipboardRequest,
    //     complete: CompleteClipboard,
    // ) !void {
    pub fn completeClipboardRequest(
        &mut self,
        req: apprt::ClipboardRequest,
        complete: CompleteClipboard<'_>,
    ) -> Result<(), Error> {
        // switch (req) {
        match req {
            // .paste => try self.completeClipboardPaste(
            //     clipboardTextContent(complete.contents) orelse "",
            //     complete.confirmed,
            // ),
            apprt::ClipboardRequest::Paste(_) => self.completeClipboardPaste(
                clipboardTextContent(complete.contents).unwrap_or(b""),
                complete.confirmed,
            )?,

            // .list => |clipboard| if (!try self.completeClipboardPasteEvent(
            //     clipboard,
            //     complete.available,
            // )) {
            //     log.debug("mode 5522 paste event was not written", .{});
            // },
            apprt::ClipboardRequest::List(clipboard) => {
                if !self.completeClipboardPasteEvent(clipboard, complete.available)? {
                    // log.debug("mode 5522 paste event was not written", .{});
                    log::debug!(target: LOG_SCOPE, "mode 5522 paste event was not written");
                }
            }

            // .osc_52_read => |clipboard| try self.completeClipboardReadOSC52(
            //     clipboardTextContent(complete.contents) orelse "",
            //     clipboard,
            //     complete.confirmed,
            // ),
            apprt::ClipboardRequest::Osc52Read(clipboard) => self.completeClipboardReadOSC52(
                clipboardTextContent(complete.contents).unwrap_or(b""),
                clipboard,
                complete.confirmed,
            )?,

            // .osc_52_write => |clipboard| { ... },
            apprt::ClipboardRequest::Osc52Write(clipboard) => {
                // The write API wants sentinel-terminated data; the write
                // text round-tripped through the apprt confirmation flow as
                // a plain representation.
                // const data = try self.alloc.dupeZ(u8, clipboardTextContent(...) orelse "");
                // defer self.alloc.free(data);
                //
                // (the `dupeZ` NUL-termination is the C-boundary detail — the
                // Rust apprt seam carries `&[u8]`, re-terminated at the C
                // boundary by the embedded backend; the dupe collapses to the
                // borrow.)
                let data = clipboardTextContent(complete.contents).unwrap_or(b"");
                unsafe { &*self.rt_surface }.set_clipboard(
                    clipboard,
                    &[apprt::ClipboardContent {
                        mime: b"text/plain",
                        data,
                    }],
                    !complete.confirmed,
                )?;
            }

            // .kitty_read => |kitty| { ... },
            apprt::ClipboardRequest::KittyRead(kitty) => {
                // If we need confirmation we return an error without
                // consuming the request state; the apprt keeps it alive
                // for the confirmation flow. A session grant carried by
                // the request skips the prompt, and a request with no
                // data types is exempt from prompting entirely; see
                // readPromptExempt.
                //
                // SAFETY: `kitty` is the leaked `Box` (see `KittyRead::destroy`);
                // it stays valid until the destroy below, and no other alias
                // exists (single-owner contract).
                let kitty_ref: &apprt::KittyRead = unsafe { &*kitty };
                if self.config.clipboard_read == configpkg::value_types::ClipboardAccess::Ask
                    && !complete.confirmed
                    && !kitty_ref.granted
                    && !rustty_vt::kitty::clipboard::read_prompt_exempt(kitty_ref.mimes.len())
                {
                    return Err(Error::UnauthorizedPaste);
                }

                // Past the confirmation check the request is consumed:
                // every path from here, including errors, must destroy it.
                // defer kitty.destroy();
                //
                // (Rust: a scope guard would fight the borrows below; the
                // handler's exits are enumerable — the single `?` propagation
                // plus the fall-through — so the destroy is called explicitly
                // on each, LIFO-faithful.)

                // Record a session grant when the user asked to remember
                // their decision and the request carried a usable
                // password. The grants live with the terminal state on
                // the IO thread.
                if complete.remember && !kitty_ref.pw.is_empty() {
                    // const pw = try self.alloc.dupe(u8, kitty.pw);
                    let pw = kitty_ref.pw.clone();
                    // self.queueIo(.{ .kitty_clipboard_grant_read = .{ .alloc, .pw } }, .unlocked);
                    self.queueIo(
                        termio::Message::KittyClipboardGrantRead(
                            termio::message::KittyClipboardGrant { pw },
                        ),
                        termio::termio_impl::MutexState::Unlocked,
                    );
                }

                let result = self.completeKittyClipboardRead(
                    kitty_ref,
                    complete.contents,
                    complete.available,
                );
                // (the deferred destroy — every consumed path.)
                unsafe { apprt::KittyRead::destroy(kitty) };
                result?;
            }

            // .kitty_write => |kitty| { ... },
            apprt::ClipboardRequest::KittyWrite(kitty) => {
                // If we need confirmation we return an error without
                // consuming the request state; the apprt keeps it alive
                // for the confirmation flow. A session grant carried by
                // the request skips the prompt.
                //
                // SAFETY: `kitty` is the leaked `Box` (see `KittyWrite::destroy`).
                let kitty_ref: &apprt::KittyWrite = unsafe { &*kitty };
                if self.config.clipboard_write == configpkg::value_types::ClipboardAccess::Ask
                    && !complete.confirmed
                    && !kitty_ref.granted
                {
                    return Err(Error::UnauthorizedPaste);
                }

                // Past the confirmation check the request is consumed:
                // every path from here, including errors, must destroy it.
                // defer kitty.destroy();

                // Record a session grant when the user asked to remember
                // their decision and the request carried a usable
                // password. The grants live with the terminal state on
                // the IO thread.
                if complete.remember && !kitty_ref.pw.is_empty() {
                    let pw = kitty_ref.pw.clone();
                    self.queueIo(
                        termio::Message::KittyClipboardGrantWrite(
                            termio::message::KittyClipboardGrant { pw },
                        ),
                        termio::termio_impl::MutexState::Unlocked,
                    );
                }

                // Apply the committed representations carried by the
                // request itself; any contents echoed back by the apprt
                // are only what its confirmation prompt displayed. An
                // empty commit clears the clipboard, which the apprt
                // write API expresses as a single empty text entry.
                //
                // (the borrowed `ClipboardContent` views over the owned
                // `ClipboardContentOwned` payload — the KittyWrite PORT NOTE.)
                let borrowed: Vec<apprt::ClipboardContent<'_>> = kitty_ref
                    .contents
                    .iter()
                    .map(|c| apprt::ClipboardContent {
                        mime: &c.mime,
                        data: &c.data,
                    })
                    .collect();
                let set_result = unsafe { &*self.rt_surface }.set_clipboard(
                    kitty_ref.location,
                    if !borrowed.is_empty() {
                        &borrowed
                    } else {
                        &[apprt::ClipboardContent {
                            mime: b"text/plain",
                            data: b"",
                        }]
                    },
                    false,
                );
                if let Err(err) = &set_result {
                    // log.err("error setting clipboard err={}", .{err});
                    log::error!(target: LOG_SCOPE, "error setting clipboard err={err:?}");
                    let result = self.kittyClipboardStatus(
                        kitty_clipboard::Operation::Write,
                        &kitty_ref.id.clone(),
                        kitty_ref.terminator,
                        kitty_clipboard::Status::Eio,
                    );
                    unsafe { apprt::KittyWrite::destroy(kitty) };
                    result?;
                    return Ok(());
                }

                let result = self.kittyClipboardStatus(
                    kitty_clipboard::Operation::Write,
                    &kitty_ref.id.clone(),
                    kitty_ref.terminator,
                    kitty_clipboard::Status::Done,
                );
                unsafe { apprt::KittyWrite::destroy(kitty) };
                result?;
            }
        }
        Ok(())
    }

    /// Deny an in-flight clipboard request. This consumes the request: for
    /// request types whose protocol expects an answer, the denial reply is
    /// written to the pty.
    // pub fn denyClipboardRequest(self: *Surface, req: apprt.ClipboardRequest) void {
    pub fn denyClipboardRequest(&mut self, req: apprt::ClipboardRequest) {
        // switch (req) {
        match req {
            // A denied paste simply doesn't happen.
            // .paste, .list => {},
            apprt::ClipboardRequest::Paste(_) | apprt::ClipboardRequest::List(_) => {}

            // OSC 52 has no error responses, but the client is waiting on
            // a reply, so a denied read is answered with empty contents.
            // .osc_52_read => |clipboard| self.completeClipboardReadOSC52("", clipboard, true)
            //     catch |err| { log.warn(...); },
            apprt::ClipboardRequest::Osc52Read(clipboard) => {
                if let Err(err) = self.completeClipboardReadOSC52(b"", clipboard, true) {
                    // log.warn("error replying to OSC 52 clipboard read err={}", .{err});
                    log::warn!(
                        target: LOG_SCOPE,
                        "error replying to OSC 52 clipboard read err={err:?}"
                    );
                }
            }

            // A denied write simply doesn't happen.
            // .osc_52_write => {},
            apprt::ClipboardRequest::Osc52Write(_) => {}

            // The Kitty clipboard protocol reports denial explicitly.
            // .kitty_read => |kitty| { defer kitty.destroy(); ... },
            apprt::ClipboardRequest::KittyRead(kitty) => {
                // SAFETY: single-owner leaked Box; destroyed exactly once below.
                let kitty_ref: &apprt::KittyRead = unsafe { &*kitty };
                if let Err(err) = self.kittyClipboardStatus(
                    kitty_clipboard::Operation::Read,
                    &kitty_ref.id.clone(),
                    kitty_ref.terminator,
                    kitty_clipboard::Status::Eperm,
                ) {
                    // log.warn("error replying to kitty clipboard read err={}", .{err});
                    log::warn!(
                        target: LOG_SCOPE,
                        "error replying to kitty clipboard read err={err:?}"
                    );
                }
                unsafe { apprt::KittyRead::destroy(kitty) };
            }

            // .kitty_write => |kitty| { defer kitty.destroy(); ... },
            apprt::ClipboardRequest::KittyWrite(kitty) => {
                let kitty_ref: &apprt::KittyWrite = unsafe { &*kitty };
                if let Err(err) = self.kittyClipboardStatus(
                    kitty_clipboard::Operation::Write,
                    &kitty_ref.id.clone(),
                    kitty_ref.terminator,
                    kitty_clipboard::Status::Eperm,
                ) {
                    // log.warn("error replying to kitty clipboard write err={}", .{err});
                    log::warn!(
                        target: LOG_SCOPE,
                        "error replying to kitty clipboard write err={err:?}"
                    );
                }
                unsafe { apprt::KittyWrite::destroy(kitty) };
            }
        }
    }

    /// This starts a clipboard request, with some basic validation. For example,
    /// an OSC 52 request is not actually requested if OSC 52 is disabled.
    ///
    /// The result reports whether the request was started; requests that
    /// weren't started never complete. Callers own reacting to that, e.g.
    /// performable paste keybinds pass through and Kitty clipboard reads
    /// answer the program.
    //
    // fn startClipboardRequest(
    //     self: *Surface,
    //     loc: apprt.Clipboard,
    //     req: apprt.ClipboardRequest,
    // ) !apprt.ClipboardReadResult {
    fn startClipboardRequest(
        &mut self,
        loc: apprt::Clipboard,
        req: apprt::ClipboardRequest,
    ) -> Result<apprt::ClipboardReadResult, Error> {
        // const effective_req: apprt.ClipboardRequest = switch (req) {
        let effective_req: apprt::ClipboardRequest = match req {
            // .paste => |clipboard| effective: { ... },
            apprt::ClipboardRequest::Paste(clipboard) => {
                // Snapshot the mode before asking the apprt for clipboard data.
                // Event pastes request only a MIME listing, while ordinary
                // pastes request the text representation as before.
                // self.renderer_state.mutex.lock();
                // const event = self.io.terminal.modes.get(.kitty_paste_events);
                // self.renderer_state.mutex.unlock();
                unsafe { (*self.renderer_state.mutex).lock() };
                let event = self
                    .io
                    .terminal
                    .modes
                    .get(rustty_vt::modes::Mode::KittyPasteEvents);
                unsafe { (*self.renderer_state.mutex).unlock() };

                // break :effective if (event) .{ .list = clipboard } else req;
                if event {
                    apprt::ClipboardRequest::List(clipboard)
                } else {
                    req
                }
            }
            _ => req,
        };

        // switch (effective_req) {
        match effective_req {
            // .paste, .list => {}, // always allowed
            apprt::ClipboardRequest::Paste(_) | apprt::ClipboardRequest::List(_) => {}
            // .osc_52_read => if (self.config.clipboard_read == .deny) {
            apprt::ClipboardRequest::Osc52Read(_) => {
                if self.config.clipboard_read == configpkg::value_types::ClipboardAccess::Deny {
                    // log.info(
                    //     "application attempted to read clipboard, but 'clipboard-read' is set to deny",
                    //     .{},
                    // );
                    log::info!(
                        target: LOG_SCOPE,
                        "application attempted to read clipboard, but 'clipboard-read' is set to deny"
                    );
                    // return .unsupported;
                    return Ok(apprt::ClipboardReadResult::Unsupported);
                }
            }

            // The clipboard access policies were already applied by
            // kittyClipboardRead and kittyClipboardWrite, which own
            // replying on denial.
            // .kitty_read, .kitty_write => {},
            apprt::ClipboardRequest::KittyRead(_) | apprt::ClipboardRequest::KittyWrite(_) => {}

            // OSC 52 writes don't travel through this function; they go
            // straight to the apprt setClipboard API.
            // .osc_52_write => unreachable,
            apprt::ClipboardRequest::Osc52Write(_) => unreachable!(),
        }

        // return try self.rt_surface.clipboardRequest(loc, effective_req);
        Ok(unsafe { &*self.rt_surface }.clipboard_request(loc, effective_req)?)
    }

    /// Send a Kitty clipboard-protocol paste event when mode 5522 is enabled.
    /// The event only lists the available MIME types; it does not read any of
    /// their data. The shared terminal paste implementation generates and records
    /// the one-time password used by the program's follow-up OSC 5522 read.
    //
    // fn completeClipboardPasteEvent(
    //     self: *Surface,
    //     clipboard: apprt.Clipboard,
    //     available: []const []const u8,
    // ) !bool {
    fn completeClipboardPasteEvent(
        &mut self,
        clipboard: apprt::Clipboard,
        available: &[&[u8]],
    ) -> Result<bool, Error> {
        // if (self.readonly) return false;
        if self.readonly {
            return Ok(false);
        }

        // const kitty_clipboard = terminal.kitty.clipboard;
        // const location: terminal.clipboard.Location = switch (clipboard) { ... };
        let location: terminal::clipboard::Location = match clipboard {
            apprt::Clipboard::Standard => terminal::clipboard::Location::Standard,
            apprt::Clipboard::Selection => terminal::clipboard::Location::Selection,
            apprt::Clipboard::Primary => terminal::clipboard::Location::Primary,
        };

        // The protocol implementation caps listings at this size too. Cap here
        // so the temporary Content array stays on the stack.
        // var contents_buf: [kitty_clipboard.max_listing_mimes]terminal.clipboard.Content = undefined;
        // const contents_len = @min(available.len, contents_buf.len);
        let mut contents_buf: [terminal::clipboard::Content<'_>;
            kitty_clipboard::max_listing_mimes] = [terminal::clipboard::Content {
            mime: b"",
            data: b"",
        }; kitty_clipboard::max_listing_mimes];
        let contents_len = available.len().min(contents_buf.len());
        // for (available[0..contents_len], contents_buf[0..contents_len]) |mime, *content| {
        //     content.* = .{ .mime = mime, .data = "" };
        // }
        for (mime, content) in available[..contents_len]
            .iter()
            .zip(contents_buf[..contents_len].iter_mut())
        {
            *content = terminal::clipboard::Content { mime, data: b"" };
        }

        // var aw: std.Io.Writer.Allocating = .init(self.alloc);
        // defer aw.deinit();
        let mut aw = rustty_vt::snapshot::io::AllocatingWriter::new();

        // self.renderer_state.mutex.lock();
        // defer self.renderer_state.mutex.unlock();
        //
        // (guard-less zigstd Mutex — the defer is the explicit unlock at
        // every exit below.)
        unsafe { (*self.renderer_state.mutex).lock() };

        // const pasted = try terminal.paste.paste(.{
        //     .terminal = &self.io.terminal,
        //     .alloc = self.alloc,
        //     .writer = &aw.writer,
        //     .kitty_clipboard = .{
        //         .grants = &self.io.terminal_stream.handler.kitty_clipboard_grants,
        //         .io = global.io(),
        //     },
        // }, .{ ... });
        //
        // SAFETY (grants aliasing): the grants live in the IO thread's stream
        // handler; every access — the handler's own and this one — happens
        // under the renderer/terminal mutex held above (the KittyClipboardGrant
        // mailbox-arm precedent), so the `&mut` cannot interleave with an IO
        // thread access.
        let io = self.io.terminal.io;
        let pasted = match rustty_vt::paste::paste(
            rustty_vt::paste::Context {
                terminal: &self.io.terminal,
                writer: &mut aw,
                kitty_clipboard: Some(rustty_vt::paste::KittyClipboard {
                    grants: &mut self.io.terminal_stream.handler.kitty_clipboard_grants,
                    io,
                }),
            },
            rustty_vt::paste::Request {
                // .source = .{ .clipboard = location },
                source: rustty_vt::paste::Source::Clipboard(location),
                // .contents = .{ .memory = contents_buf[0..contents_len] },
                contents: rustty_vt::paste::Contents::Memory(&contents_buf[..contents_len]),
                // A paste event discloses no clipboard data, so unsafe-text
                // confirmation does not apply. If mode 5522 is reset, the empty
                // stand-in representations cause the shared helper to write nothing.
                // .allow_unsafe = true,
                allow_unsafe: true,
            },
        ) {
            Ok(v) => v,
            Err(err) => {
                unsafe { (*self.renderer_state.mutex).unlock() };
                // (the reference propagates the inferred paste error union;
                // the port folds it into the surface `Error` set:
                // `UnsafePaste` keeps its distinguishable variant, the
                // writer/reader arms are unreachable here — the Allocating
                // writer and in-memory contents never fail — and the
                // allocation/entropy arms surface as the vt OutOfMemory
                // marker, the crate's Zig-`Allocator.Error` convention.)
                return Err(match err {
                    rustty_vt::paste::Error::UnsafePaste => Error::UnsafePaste,
                    rustty_vt::paste::Error::WriteFailed | rustty_vt::paste::Error::ReadFailed => {
                        unreachable!()
                    }
                    _ => Error::OutOfMemory(terminal::pagelist::OutOfMemory),
                });
            }
        };
        // if (!pasted) return false;
        if !pasted {
            unsafe { (*self.renderer_state.mutex).unlock() };
            return Ok(false);
        }

        // self.queueIo(.{ .write_alloc = .{
        //     .alloc = self.alloc,
        //     .data = try aw.toOwnedSlice(),
        // } }, .locked);
        // return true;
        //
        // PORT NOTE (parity S42-F1): the `defer ... unlock` above runs at
        // function exit, so the reference queues with the renderer mutex STILL
        // HELD, which is what `.locked` tells the mailbox: on a full queue it
        // unlocks that mutex while it waits and relocks it after. CORRECTION:
        // the port used to unlock before this call, so the full-queue path
        // unlocked a mutex nobody held and returned with it locked.
        self.queueIo(
            termio::Message::WriteAlloc(Alloc {
                data: aw.written().to_vec().into_boxed_slice(),
            }),
            termio::termio_impl::MutexState::Locked,
        );
        unsafe { (*self.renderer_state.mutex).unlock() };
        Ok(true)
    }

    /// Handle a Kitty clipboard protocol (OSC 5522) read request forwarded
    /// by the IO thread. This takes ownership of the request state.
    //
    // fn kittyClipboardRead(
    //     self: *Surface,
    //     req: *apprt.ClipboardRequest.KittyRead,
    // ) !void {
    #[allow(non_snake_case)]
    fn kittyClipboardRead(&mut self, req: *mut apprt::KittyRead) -> Result<(), Error> {
        // SAFETY: `req` is the leaked `Box` from the mailbox arm (single
        // owner); it stays valid until a destroy on one of the consumed paths.
        let req_ref: &apprt::KittyRead = unsafe { &*req };

        // A read denied by policy answers EPERM so clients degrade
        // gracefully instead of waiting on a response that never comes.
        // if (self.config.clipboard_read == .deny) {
        if self.config.clipboard_read == configpkg::value_types::ClipboardAccess::Deny {
            // defer req.destroy();
            // log.info("application attempted to read clipboard, but 'clipboard-read' is set to deny", .{});
            log::info!(
                target: LOG_SCOPE,
                "application attempted to read clipboard, but 'clipboard-read' is set to deny"
            );
            // try self.kittyClipboardStatus(.read, req.id, req.terminator, .EPERM);
            // return;
            let result = self.kittyClipboardStatus(
                kitty_clipboard::Operation::Read,
                &req_ref.id.clone(),
                req_ref.terminator,
                kitty_clipboard::Status::Eperm,
            );
            unsafe { apprt::KittyRead::destroy(req) };
            result?;
            return Ok(());
        }

        // const result = self.startClipboardRequest(
        //     req.location,
        //     .{ .kitty_read = req },
        // ) catch |err| {
        //     defer req.destroy();
        //     self.kittyClipboardStatus(.read, req.id, req.terminator, .EIO) catch {};
        //     return err;
        // };
        let result = match self
            .startClipboardRequest(req_ref.location, apprt::ClipboardRequest::KittyRead(req))
        {
            Ok(v) => v,
            Err(err) => {
                let _ = self.kittyClipboardStatus(
                    kitty_clipboard::Operation::Read,
                    &req_ref.id.clone(),
                    req_ref.terminator,
                    kitty_clipboard::Status::Eio,
                );
                unsafe { apprt::KittyRead::destroy(req) };
                return Err(err);
            }
        };

        // switch (result) {
        match result {
            // The request completes asynchronously.
            // .started => {},
            apprt::ClipboardReadResult::Started => {}

            // The clipboard has nothing we can serve, which is a
            // successful read that serves no representations. This never
            // prompts even under an ask policy since there are no contents
            // to disclose.
            // .unavailable => { defer req.destroy(); try self.completeKittyClipboardRead(req, &.{}, &.{}); },
            apprt::ClipboardReadResult::Unavailable => {
                let result = self.completeKittyClipboardRead(req_ref, &[], &[]);
                unsafe { apprt::KittyRead::destroy(req) };
                result?;
            }

            // The apprt can't serve this clipboard at all, e.g. an
            // unsupported primary selection.
            // .unsupported => { defer req.destroy(); try self.kittyClipboardStatus(.read, req.id, req.terminator, .ENOSYS); },
            apprt::ClipboardReadResult::Unsupported => {
                let result = self.kittyClipboardStatus(
                    kitty_clipboard::Operation::Read,
                    &req_ref.id.clone(),
                    req_ref.terminator,
                    kitty_clipboard::Status::Enosys,
                );
                unsafe { apprt::KittyRead::destroy(req) };
                result?;
            }
        }
        Ok(())
    }

    /// Handle a committed Kitty clipboard protocol (OSC 5522) write
    /// transaction forwarded by the IO thread. This takes ownership of the
    /// request state.
    //
    // fn kittyClipboardWrite(
    //     self: *Surface,
    //     req: *apprt.ClipboardRequest.KittyWrite,
    // ) !void {
    #[allow(non_snake_case)]
    fn kittyClipboardWrite(&mut self, req: *mut apprt::KittyWrite) -> Result<(), Error> {
        // SAFETY: `req` is the leaked `Box` from the mailbox arm (single
        // owner); it stays valid until a destroy on one of the consumed paths.
        let req_ref: &apprt::KittyWrite = unsafe { &*req };

        // A write denied by policy answers EPERM so clients degrade
        // gracefully instead of waiting on a response that never comes.
        // The IO thread already fails transactions that begin under a
        // deny policy, but the policy may have changed mid-transaction.
        // if (self.config.clipboard_write == .deny) {
        if self.config.clipboard_write == configpkg::value_types::ClipboardAccess::Deny {
            // defer req.destroy();
            // log.info("application attempted to write clipboard, but 'clipboard-write' is set to deny", .{});
            log::info!(
                target: LOG_SCOPE,
                "application attempted to write clipboard, but 'clipboard-write' is set to deny"
            );
            // try self.kittyClipboardStatus(.write, req.id, req.terminator, .EPERM);
            // return;
            let result = self.kittyClipboardStatus(
                kitty_clipboard::Operation::Write,
                &req_ref.id.clone(),
                req_ref.terminator,
                kitty_clipboard::Status::Eperm,
            );
            unsafe { apprt::KittyWrite::destroy(req) };
            result?;
            return Ok(());
        }

        // const result = self.startClipboardRequest(
        //     req.location,
        //     .{ .kitty_write = req },
        // ) catch |err| {
        //     defer req.destroy();
        //     self.kittyClipboardStatus(.write, req.id, req.terminator, .EIO) catch {};
        //     return err;
        // };
        let result = match self
            .startClipboardRequest(req_ref.location, apprt::ClipboardRequest::KittyWrite(req))
        {
            Ok(v) => v,
            Err(err) => {
                let _ = self.kittyClipboardStatus(
                    kitty_clipboard::Operation::Write,
                    &req_ref.id.clone(),
                    req_ref.terminator,
                    kitty_clipboard::Status::Eio,
                );
                unsafe { apprt::KittyWrite::destroy(req) };
                return Err(err);
            }
        };

        // switch (result) {
        match result {
            // The request completes asynchronously.
            // .started => {},
            apprt::ClipboardReadResult::Started => {}

            // The apprt can't write this clipboard at all, e.g. an
            // unsupported primary selection. Writes carry their own
            // contents so there is no meaningful unavailable state; treat
            // it the same.
            // .unavailable, .unsupported => { defer req.destroy();
            //     try self.kittyClipboardStatus(.write, req.id, req.terminator, .ENOSYS); },
            apprt::ClipboardReadResult::Unavailable | apprt::ClipboardReadResult::Unsupported => {
                let result = self.kittyClipboardStatus(
                    kitty_clipboard::Operation::Write,
                    &req_ref.id.clone(),
                    req_ref.terminator,
                    kitty_clipboard::Status::Enosys,
                );
                unsafe { apprt::KittyWrite::destroy(req) };
                result?;
            }
        }
        Ok(())
    }

    /// Reply to a Kitty clipboard request with a single status packet.
    //
    // fn kittyClipboardStatus(
    //     self: *Surface,
    //     op: terminal.kitty.clipboard.Operation,
    //     id: []const u8,
    //     terminator: terminal.osc.Terminator,
    //     status: terminal.kitty.clipboard.Status,
    // ) error{ OutOfMemory, WriteFailed }!void {
    #[allow(non_snake_case)]
    fn kittyClipboardStatus(
        &mut self,
        op: kitty_clipboard::Operation,
        id: &[u8],
        terminator: rustty_vt::osc::Terminator,
        status: kitty_clipboard::Status,
    ) -> Result<(), Error> {
        // var aw: std.Io.Writer.Allocating = .init(self.alloc);
        // defer aw.deinit();
        let mut aw = rustty_vt::snapshot::io::AllocatingWriter::new();
        // try (terminal.kitty.clipboard.Response{
        //     .op = op,
        //     .status = status,
        //     .id = id,
        //     .terminator = terminator,
        // }).encode(&aw.writer);
        kitty_clipboard::Response {
            id,
            terminator,
            ..kitty_clipboard::Response::new(op, status)
        }
        .encode(&mut aw)
        .expect("allocating writer never fails"); // error{WriteFailed} unreachable through the Allocating writer

        // self.queueIo(.{ .write_alloc = .{
        //     .alloc = self.alloc,
        //     .data = try aw.toOwnedSlice(),
        // } }, .unlocked);
        self.queueIo(
            termio::Message::WriteAlloc(Alloc {
                data: aw.written().to_vec().into_boxed_slice(),
            }),
            termio::termio_impl::MutexState::Unlocked,
        );
        Ok(())
    }

    /// Complete a Kitty clipboard protocol read with the clipboard
    /// contents.
    //
    // fn completeKittyClipboardRead(
    //     self: *Surface,
    //     req: *const apprt.ClipboardRequest.KittyRead,
    //     contents: []const terminal.clipboard.Content,
    //     available: []const []const u8,
    // ) !void {
    #[allow(non_snake_case)]
    fn completeKittyClipboardRead(
        &mut self,
        req: &apprt::KittyRead,
        contents: &[terminal::clipboard::Content<'_>],
        available: &[&[u8]],
    ) -> Result<(), Error> {
        // const kitty_clipboard = terminal.kitty.clipboard;

        // Serve the requested representations in request order under their
        // requested names. Text-like MIME aliases all match the canonical
        // text representation, since that is the only name the apprt
        // serves text under. Requested types without a representation are
        // simply never served, which is how the protocol communicates an
        // unavailable representation.
        // var contents_buf: [kitty_clipboard.max_read_mimes]terminal.clipboard.Content = undefined;
        // var contents_len: usize = 0;
        let mut contents_buf: [terminal::clipboard::Content<'_>; kitty_clipboard::max_read_mimes] =
            [terminal::clipboard::Content {
                mime: b"",
                data: b"",
            }; kitty_clipboard::max_read_mimes];
        let mut contents_len: usize = 0;
        // for (req.mimes) |mime| { ... }
        // (Rust: the Zig `continue` inside the labeled `data:` block needs an
        // explicit loop label.)
        'mimes: for mime in req.mimes.iter() {
            // const data: []const u8 = data: { ... continue; };
            let data: &[u8] = 'data: {
                for content in contents {
                    // if (std.mem.eql(u8, content.mime, mime)) break :data content.data;
                    if content.mime == &mime[..] {
                        break 'data content.data;
                    }
                    // if (terminal.clipboard.isTextMime(mime) and
                    //     terminal.clipboard.isTextMime(content.mime))
                    // { break :data content.data; }
                    if terminal::clipboard::is_text_mime(mime)
                        && terminal::clipboard::is_text_mime(content.mime)
                    {
                        break 'data content.data;
                    }
                }
                continue 'mimes;
            };

            contents_buf[contents_len] = terminal::clipboard::Content { mime, data };
            contents_len += 1;
        }

        // Encode the full success sequence: the OK packet, the targets
        // listing if it was requested, DATA chunks for each served
        // representation, and the final DONE packet.
        // var aw: std.Io.Writer.Allocating = .init(self.alloc);
        // defer aw.deinit();
        let mut aw = rustty_vt::snapshot::io::AllocatingWriter::new();
        // try (kitty_clipboard.ReadSuccess{ ... }).encode(&aw.writer);
        kitty_clipboard::ReadSuccess {
            primary: req.location == apprt::Clipboard::Primary,
            id: &req.id,
            list: req.list,
            available,
            contents: &contents_buf[..contents_len],
            terminator: req.terminator,
            ..kitty_clipboard::ReadSuccess::default()
        }
        .encode(&mut aw)
        .expect("allocating writer never fails"); // error{WriteFailed} unreachable through the Allocating writer

        // self.queueIo(.{ .write_alloc = .{
        //     .alloc = self.alloc,
        //     .data = try aw.toOwnedSlice(),
        // } }, .unlocked);
        self.queueIo(
            termio::Message::WriteAlloc(Alloc {
                data: aw.written().to_vec().into_boxed_slice(),
            }),
            termio::termio_impl::MutexState::Unlocked,
        );
        Ok(())
    }

    // fn completeClipboardPaste(
    //     self: *Surface,
    //     data: []const u8,
    //     allow_unsafe: bool,
    // ) !void {
    //
    // (`data: []const u8` -> raw bytes `&[u8]`, never a lossy String — R3.)
    fn completeClipboardPaste(&mut self, data: &[u8], allow_unsafe: bool) -> Result<(), Error> {
        // if (data.len == 0) return;
        if data.is_empty() {
            return Ok(());
        }

        // const encode_opts: input.paste.Options = encode_opts: {
        let encode_opts: paste::Options = {
            // self.renderer_state.mutex.lock();
            // defer self.renderer_state.mutex.unlock();
            //
            // (guard-less zigstd Mutex — the defer is the explicit unlock at
            // every exit below.)
            unsafe { (*self.renderer_state.mutex).lock() };
            // const opts: input.paste.Options = .fromTerminal(&self.io.terminal);
            let opts: paste::Options = paste::Options::from_terminal(&self.io.terminal);

            // If we have paste protection enabled, we detect unsafe pastes and return
            // an error. The error approach allows apprt to attempt to complete the paste
            // before falling back to requesting confirmation.
            //
            // We do not do this for bracketed pastes because bracketed pastes are
            // by definition safe since they're framed.
            // const unsafe = unsafe: {
            let unsafe_ = 'unsafe_blk: {
                // If we've disabled paste protection then we always allow the paste.
                // if (!self.config.clipboard_paste_protection) break :unsafe false;
                if !self.config.clipboard_paste_protection {
                    break 'unsafe_blk false;
                }

                // If we're allowed to paste unsafe data then we always allow the paste.
                // This is set during confirmation usually.
                // if (allow_unsafe) break :unsafe false;
                if allow_unsafe {
                    break 'unsafe_blk false;
                }

                // if (opts.bracketed) {
                if opts.bracketed {
                    // If we're bracketed and the paste contains and ending
                    // bracket then something naughty might be going on and we
                    // never trust it.
                    // if (std.mem.indexOf(u8, data, "\x1B[201~") != null) break :unsafe true;
                    //
                    // (`std.mem.indexOf(..) != null` -> a subslice scan — the
                    // paste.rs `index_of` precedent.)
                    if data.windows(6).any(|w| w == b"\x1b[201~") {
                        break 'unsafe_blk true;
                    }

                    // If we are bracketed and configured to trust that then the
                    // paste is not unsafe.
                    // if (self.config.clipboard_paste_bracketed_safe) break :unsafe false;
                    if self.config.clipboard_paste_bracketed_safe {
                        break 'unsafe_blk false;
                    }
                }

                // break :unsafe !input.paste.isSafe(data);
                !paste::is_safe(data)
            };

            // if (unsafe) {
            if unsafe_ {
                // log.info("potentially unsafe paste detected, rejecting until confirmation", .{});
                log::info!(
                    target: LOG_SCOPE,
                    "potentially unsafe paste detected, rejecting until confirmation"
                );
                // return error.UnsafePaste;
                //
                // (unlock before propagating — the explicit-unlock convention.)
                unsafe { (*self.renderer_state.mutex).unlock() };
                return Err(Error::UnsafePaste);
            }

            // With the lock held, we must scroll to the bottom.
            // We always scroll to the bottom for these inputs.
            // self.scrollToBottom() catch |err| {
            //     log.warn("error scrolling to bottom err={}", .{err});
            // };
            if let Err(err) = self.scrollToBottom() {
                // log.warn("error scrolling to bottom err={}", .{err});
                log::warn!(target: LOG_SCOPE, "error scrolling to bottom err={err:?}");
            }

            // break :encode_opts opts;
            unsafe { (*self.renderer_state.mutex).unlock() };
            opts
        };

        // Encode the data. In most cases this doesn't require any
        // copies, so we optimize for that case.
        // var data_duped: ?[]u8 = null;
        let mut data_duped: Option<Vec<u8>> = None;
        // const vecs = input.paste.encode(data, encode_opts) catch |err| switch (err) {
        //     error.MutableRequired => vecs: {
        //         const buf: []u8 = try self.alloc.dupe(u8, data);
        //         errdefer self.alloc.free(buf);
        //         data_duped = buf;
        //         break :vecs input.paste.encode(buf, encode_opts);
        //     },
        // };
        // defer if (data_duped) |v| {
        //     // This code path means the data did require a copy and mutation.
        //     // We must free it.
        //     self.alloc.free(v);
        // };
        //
        // PORT NOTE: the reference `encode` is comptime-dispatched on slice
        // mutability — const `data` may return `error.MutableRequired`; the
        // mutable retry over the duped `buf` cannot. Rustty splits this into
        // `paste::encode` (const, may err) and `paste::encode_mut` (mutable,
        // infallible), exactly the two `@TypeOf(data)` branches (the paste.rs
        // split note). `data_duped` is declared BEFORE `vecs` so it outlives the
        // borrow (`encode_mut`'s output aliases the duped buffer); the `defer
        // free` == its `Vec` dropping at scope end. `Option::insert` fuses the
        // reference's `data_duped = buf` store with the `encode(buf, ...)` call
        // (it returns `&mut` to the just-stored buffer).
        let vecs: [&[u8]; 3] = match paste::encode(data, encode_opts) {
            Ok(v) => v,
            Err(paste::Error::MutableRequired) => {
                // const buf: []u8 = try self.alloc.dupe(u8, data);
                // data_duped = buf;
                // break :vecs input.paste.encode(buf, encode_opts);
                paste::encode_mut(data_duped.insert(data.to_vec()), encode_opts)
            }
        };
        // (the reference `defer if (data_duped) |v| self.alloc.free(v);` == the
        // owned `data_duped: Option<Vec<u8>>` dropping at scope end below.)

        // for (vecs) |vec| if (vec.len > 0) {
        for vec in vecs {
            if !vec.is_empty() {
                // self.queueIo(try termio.Message.writeReq(
                //     self.alloc,
                //     vec,
                // ), .unlocked);
                self.queueIo(
                    termio::Message::writeReq(vec),
                    termio::termio_impl::MutexState::Unlocked,
                );
            }
        }

        Ok(())
    }

    // fn completeClipboardReadOSC52(
    //     self: *Surface,
    //     data: []const u8,
    //     clipboard_type: apprt.Clipboard,
    //     confirmed: bool,
    // ) !void {
    //
    // (`data: []const u8` -> raw bytes `&[u8]`, never a lossy String — R3.)
    fn completeClipboardReadOSC52(
        &mut self,
        data: &[u8],
        clipboard_type: apprt::Clipboard,
        confirmed: bool,
    ) -> Result<(), Error> {
        // We should never get here if clipboard-read is set to deny
        // assert(self.config.clipboard_read != .deny);
        debug_assert!(self.config.clipboard_read != configpkg::value_types::ClipboardAccess::Deny);

        // If clipboard-read is set to ask and we haven't confirmed with the user,
        // do that now
        // if (self.config.clipboard_read == .ask and !confirmed) {
        //     return error.UnauthorizedPaste;
        // }
        if self.config.clipboard_read == configpkg::value_types::ClipboardAccess::Ask && !confirmed
        {
            return Err(Error::UnauthorizedPaste);
        }

        // Even if the clipboard data is empty we reply, since presumably
        // the client app is expecting a reply. We first allocate our buffer.
        // This must hold the base64 encoded data PLUS the OSC code surrounding it.
        // const enc = std.base64.standard.Encoder;
        // const size = enc.calcSize(data.len);
        let size = zig_base64::calc_size(data.len());
        // const buf = try self.alloc.alloc(u8, size + 9); // const for OSC
        // errdefer self.alloc.free(buf);
        //
        // (the owned `buf` drops on every error exit below; the success exit
        // transfers it into the `.write_alloc` message — `c11fe5486`.)
        let mut buf: Vec<u8> = vec![0u8; size + 9]; // const for OSC

        // const kind: u8 = switch (clipboard_type) {
        let kind: u8 = match clipboard_type {
            // .standard => 'c',
            apprt::Clipboard::Standard => b'c',
            // .selection => 's',
            apprt::Clipboard::Selection => b's',
            // .primary => 'p',
            apprt::Clipboard::Primary => b'p',
        };

        // Wrap our data with the OSC code
        // const prefix = try std.fmt.bufPrint(buf, "\x1b]52;{c};", .{kind});
        // assert(prefix.len == 7);
        //
        // (the fixed `bufPrint` format `\x1b]52;{c};` is 7 bytes — written
        // directly, the SURFACE_ID `bufPrint` precedent.)
        buf[0] = 0x1b;
        buf[1] = b']';
        buf[2] = b'5';
        buf[3] = b'2';
        buf[4] = b';';
        buf[5] = kind;
        buf[6] = b';';
        let prefix_len = 7;
        debug_assert!(prefix_len == 7);
        // buf[buf.len - 2] = '\x1b';
        // buf[buf.len - 1] = '\\';
        let n = buf.len();
        buf[n - 2] = 0x1b;
        buf[n - 1] = b'\\';

        // Do the base64 encoding
        // const encoded = enc.encode(buf[prefix.len..], data);
        // assert(encoded.len == size);
        let encoded_len = zig_base64::encode(&mut buf[prefix_len..], data).len();
        debug_assert!(encoded_len == size);

        // self.queueIo(.{ .write_alloc = .{
        //     .alloc = self.alloc,
        //     .data = buf,
        // } }, .unlocked);
        //
        // (the allocated response transfers directly — a move, no copy; the
        // exact-capacity `Vec` boxes without reallocating.)
        self.queueIo(
            termio::Message::WriteAlloc(Alloc {
                data: buf.into_boxed_slice(),
            }),
            termio::termio_impl::MutexState::Unlocked,
        );

        Ok(())
    }

    // fn showDesktopNotification(self: *Surface, title: [:0]const u8, body: [:0]const u8) !void {
    //
    // (the `[:0]const u8` params are raw bytes — `&[u8]`, never a lossy String.)
    fn showDesktopNotification(&mut self, title: &[u8], body: &[u8]) -> Result<(), Error> {
        // Wyhash is used to hash the contents of the desktop notification to limit
        // how fast identical notifications can be sent sequentially.
        // const hash_algorithm = std.hash.Wyhash;
        //
        // PORT NOTE: Rustty uses the stdlib `DefaultHasher` (the font-shaper
        // `shared_grid_set::Key::hashcode` precedent). The digest is an internal
        // de-dup key compared only against the previous notification within the
        // same process (never serialized/observed cross-implementation), so the
        // exact algorithm is not part of the parity contract; SipHash's `write`
        // is unframed, so `write(title); write(body)` streams `title++body`
        // exactly as Wyhash's `update; update`.

        // const now = try std.time.Instant.now();
        //
        // (Rust `Instant::now` is infallible — the `try` collapses; the RingBell
        // precedent.)
        let now = std::time::Instant::now();

        // PARITY: std.time.ns_per_s = 1_000_000_000 (the config_struct.rs / RingBell
        // NS_PER_S precedent). `Instant.since` -> `saturating_duration_since` (ns).
        const NS_PER_S: u128 = 1_000_000_000;

        // Set a limit of one desktop notification per second so that the OS
        // doesn't kill us when we run out of resources.
        // if (self.app.last_notification_time) |last| {
        if let Some(last) = unsafe { (*self.app).last_notification_time } {
            // if (now.since(last) < 1 * std.time.ns_per_s) {
            if now.saturating_duration_since(last).as_nanos() < NS_PER_S {
                // log.warn("rate limiting desktop notifications", .{});
                log::warn!(target: LOG_SCOPE, "rate limiting desktop notifications");
                return Ok(());
            }
        }

        // const new_digest = d: {
        let new_digest = {
            use core::hash::Hasher;
            // var hash = hash_algorithm.init(0);
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            // hash.update(title);
            hash.write(title);
            // hash.update(body);
            hash.write(body);
            // break :d hash.final();
            hash.finish()
        };

        // Set a limit of one notification per five seconds for desktop
        // notifications with identical content.
        // if (self.app.last_notification_time) |last| {
        if let Some(last) = unsafe { (*self.app).last_notification_time } {
            // if (self.app.last_notification_digest == new_digest) {
            if unsafe { (*self.app).last_notification_digest } == new_digest {
                // if (now.since(last) < 5 * std.time.ns_per_s) {
                if now.saturating_duration_since(last).as_nanos() < 5 * NS_PER_S {
                    // log.warn("suppressing identical desktop notification", .{});
                    log::warn!(target: LOG_SCOPE, "suppressing identical desktop notification");
                    return Ok(());
                }
            }
        }

        // self.app.last_notification_time = now;
        unsafe { (*self.app).last_notification_time = Some(now) };
        // self.app.last_notification_digest = new_digest;
        unsafe { (*self.app).last_notification_digest = new_digest };
        // _ = try self.rt_app.performAction(
        //     .{ .surface = self },
        //     .desktop_notification,
        //     .{
        //         .title = title,
        //         .body = body,
        //     },
        // );
        let _ = unsafe { &*self.rt_app }.perform_action(
            apprt::Target::Surface(&*self),
            apprt::Action::DesktopNotification(DesktopNotification { title, body }),
        )?;
        Ok(())
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

/// The payload for completing a clipboard request with
/// completeClipboardRequest.
//
// pub const CompleteClipboard = struct { ... };
// (Zig field defaults — every field — become the `Default` impl below;
// borrowed slices, never lossy Strings — R3.)
pub struct CompleteClipboard<'a> {
    /// The representations the apprt could serve for the request's MIME
    /// types. These are immediately copied as needed so they only need
    /// to live for the duration of the completion call. Requesters that
    /// only carry text (paste, OSC 52) use the first text-like
    /// representation.
    // contents: []const terminal.clipboard.Content = &.{},
    pub contents: &'a [terminal::clipboard::Content<'a>],

    /// The listing of MIME types available on the clipboard, only
    /// gathered when the request asked for it.
    // available: []const []const u8 = &.{},
    pub available: &'a [&'a [u8]],

    /// True if any clipboard confirmation prompt was already answered
    /// by the user, skipping further prompts:
    ///
    ///   - For "regular" pasting this means that unsafe pastes are
    ///     allowed. Unsafe data is defined as data that contains
    ///     newlines, though this definition may change later to detect
    ///     other scenarios.
    ///
    ///   - For OSC 52 and Kitty clipboard protocol reads and writes no
    ///     prompt is shown to the user when this is true.
    // confirmed: bool = false,
    pub confirmed: bool,

    /// True if the user asked to remember their decision. This is only
    /// honored by request types that support session grants (Kitty
    /// clipboard protocol requests carrying a password).
    // remember: bool = false,
    pub remember: bool,
}

impl Default for CompleteClipboard<'_> {
    fn default() -> Self {
        CompleteClipboard {
            contents: &[],
            available: &[],
            confirmed: false,
            remember: false,
        }
    }
}

/// The first text-like representation of the contents, if any.
//
// fn clipboardTextContent(contents: []const terminal.clipboard.Content) ?[]const u8 {
//     for (contents) |content| {
//         if (terminal.clipboard.isTextMime(content.mime)) return content.data;
//     }
//     return null;
// }
#[allow(non_snake_case)]
fn clipboardTextContent<'a>(contents: &[terminal::clipboard::Content<'a>]) -> Option<&'a [u8]> {
    for content in contents {
        if terminal::clipboard::is_text_mime(content.mime) {
            return Some(content.data);
        }
    }
    None
}

/// PORT NOTE: helper — Zig `@intFromFloat` for the scroll/selection GESTURE pixel
/// math (`scrollCallback`'s `@intFromFloat(@trunc(amount))` /
/// `@intFromFloat(@round(xoff))` and `performBindingAction`'s
/// `.scroll_page_fractional` `@intFromFloat(@trunc(fraction * rows))`).
///
/// RUST CONTRACT (differs from the Zig builtin it replaces): Zig
/// `@intFromFloat` is safety-checked — Debug/ReleaseSafe TRAP when the value
/// is infinite or outside the destination; ReleaseFast is UB. A NaN is NOT
/// caught by the check: it converts, in every mode, to a target-dependent
/// value — `0` on arm64, `isize::MIN` on x86_64 (measured with Zig 0.16.0,
/// task R13-P2; the x86_64 build under Rosetta 2).
///
/// PORT NOTE: reference defect (R13), `UPSTREAM-DEFECTS.md` UD62. The trap is
/// reachable from ordinary input, so it is NOT ported: `keybind =
/// …=scroll_page_fractional:1e30` panics the reference on the keypress, and a
/// C-ABI embedder's scroll offset reaches the three gesture casts. Measured
/// with Zig 0.16.0 (`r13p-reference-probe.zig`): Debug and ReleaseSafe panic
/// "integer part of floating point value out of bounds", ReleaseFast yields
/// `9223372036854775807`; `std.fmt.parseFloat` accepts `1e30`, `inf` and `nan`,
/// so the config can carry each of them (the first two trap; `nan` does not —
/// see above).
///
/// This returns `None` where the reference traps, and ALSO for NaN, which has
/// no `isize`; each caller takes the handling its own value means — see the
/// call sites. For NaN: `scroll_page_fractional` saturates it to `0` (arm64's
/// value; x86_64's `isize::MIN` would scroll toward the top), and the gesture
/// casts scroll nothing. On the discrete x path that is arm64's delta-0 result
/// (x86_64's `isize::MIN` would drive the per-row loops). On the y and
/// precision-x paths the reference's `assert(@abs(amount) >= 1)` precedes the
/// cast, so a NaN fails that assert in a safe reference build (and ReleaseFast
/// latches it into `pending_scroll_*`); the port's guard runs ahead of its
/// copy of that assert, so a NaN from the C ABI scrolls nothing in every
/// build (task R13-C).
#[inline]
fn isize_from_float(v: f64) -> Option<isize> {
    // `@intFromFloat` converts the INTEGER PART (truncation toward zero) and
    // safety-checks that it fits the destination. `-(isize::MIN as f64)` is
    // exactly 2^63; inf fails a comparison, as it fails the reference's check.
    // NaN fails both too, although the reference's check lets it through (see
    // the doc comment).
    let t = v.trunc();
    if t >= isize::MIN as f64 && t < -(isize::MIN as f64) {
        Some(t as isize)
    } else {
        None
    }
}

/// `isize_from_float` for a value whose out-of-range case MEANS "as far as it
/// goes": the viewport clamps the result anyway. NaN has no direction, so it
/// scrolls nothing. This is also what the reference's ReleaseFast produces
/// (measured), without its undefined behaviour.
#[inline]
fn isize_from_float_saturating(v: f64) -> isize {
    match isize_from_float(v) {
        Some(v) => v,
        None if v.is_nan() => 0,
        None if v > 0.0 => isize::MAX,
        None => isize::MIN,
    }
}

/// `scrollCallback`'s `const y: ScrollAmount = if (yoff == 0) .{} else y: {
/// ... }` block (`Surface.zig:3469-3524`), unchanged but for its receiver.
///
/// PORT NOTE (task R13-P2, UD62): lifted out of `scrollCallback` so the
/// block's float cast is driven by a test — `Surface::init` needs a live
/// apprt, so the callback itself cannot run in one. `self.mouse`,
/// `self.size.cell` and `self.config.mouse_scroll_multiplier` are the only
/// `self` state the block touches, and they are its parameters.
fn scroll_amount_y(
    mouse: &mut Mouse,
    cell: rendererpkg::size::CellSize,
    multiplier: configpkg::value_types::MouseScrollMultiplier,
    yoff: f64,
    scroll_mods: input_mouse::ScrollMods,
) -> ScrollAmount {
    // const y: ScrollAmount = if (yoff == 0) .{} else y: {
    if yoff == 0.0 {
        ScrollAmount::default()
    } else {
        'y: {
            // We use cell_size to determine if we have accumulated enough to trigger a scroll
            // const cell_size: f64 = @floatFromInt(self.size.cell.height);
            let cell_size: f64 = f64::from(cell.height);

            // If we have precision scroll, yoff is the number of pixels to scroll. In non-precision
            // scroll, yoff is the number of wheel ticks. Some mice are capable of reporting fractional
            // wheel ticks, which don't necessarily get reported as precision scrolls. We normalize all
            // scroll events to pixels by multiplying the wheel tick value and the cell size. This means
            // that a wheel tick of 1 results in single scroll event.
            // const yoff_adjusted: f64 = if (scroll_mods.precision)
            //     yoff * self.config.mouse_scroll_multiplier.precision
            // else yoff_adjusted: {
            let yoff_adjusted: f64 = if scroll_mods.precision {
                yoff * multiplier.precision
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
                        break 'yoff_adjusted yoff_max * cell_size * multiplier.discrete;
                    } else {
                        // break :yoff_adjusted yoff * cell_size * self.config.mouse_scroll_multiplier.discrete;
                        break 'yoff_adjusted yoff * cell_size * multiplier.discrete;
                    }
                }
            };

            // Add our previously saved pending amount to the offset to get the
            // new offset value. The signs of the pending and yoff should match
            // so that we move further away from zero, but we don't assert
            // this because in theory a user could scroll in the opposite
            // direction and undo a pending scroll.
            // const poff: f64 = self.mouse.pending_scroll_y + yoff_adjusted;
            let poff: f64 = mouse.pending_scroll_y + yoff_adjusted;

            // If the new offset is less than a single unit of scroll, we save
            // the new pending value and do not scroll yet.
            // if (@abs(poff) < cell_size) {
            if poff.abs() < cell_size {
                // self.mouse.pending_scroll_y = poff;
                mouse.pending_scroll_y = poff;
                // break :y .{};
                break 'y ScrollAmount::default();
            }

            // We scroll by the number of rows in the offset and save the remainder
            // const amount = poff / cell_size;
            let amount = poff / cell_size;
            // Round towards zero.
            // const delta: isize = @intFromFloat(@trunc(amount));
            //
            // PORT NOTE (R13, UD62): an amount with no `isize` representation
            // (a non-finite or astronomically large offset from the OS or a
            // C-ABI embedder) is not scrolled at all — the `.{}` arm this
            // function already takes for an offset too small to act on. It is
            // taken BEFORE the pending store, so `pending_scroll_y` can never
            // hold a NaN that poisons every later scroll, and BEFORE BOTH
            // asserts, which hold for every value that gets past here.
            // Saturating would be worse than the crash it replaces:
            // `scrollCallback`'s consumers of this amount loop `0..delta`
            // writing one report per row.
            //
            // The guard sits ahead of the reference's `assert(@abs(amount) >=
            // 1)` (task R13-C): a NaN offset from `rustty_surface_mouse_scroll`
            // makes `amount` NaN, which fails that assert, and a
            // `debug_assert!` panic inside an `extern "C"` entry point aborts
            // the process in debug and `release-safe` builds. Outside input
            // can violate it, so under R13 it is not an invariant until the
            // guard has run.
            let Some(delta) = isize_from_float(amount.trunc()) else {
                break 'y ScrollAmount::default();
            };
            // assert(@abs(amount) >= 1);
            debug_assert!(amount.abs() >= 1.0);
            // self.mouse.pending_scroll_y = poff - (amount * cell_size);
            mouse.pending_scroll_y = poff - (amount * cell_size);
            // assert(@abs(delta) >= 1);
            debug_assert!(delta.unsigned_abs() >= 1);

            // break :y .{ .delta = delta };
            break 'y ScrollAmount { delta };
        }
    }
}

/// `scrollCallback`'s `const x: ScrollAmount = if (xoff == 0) .{} else x: {
/// ... }` block (`Surface.zig:3527-3546`), lifted out for the same reason as
/// `scroll_amount_y`. It holds two of the three gesture casts.
fn scroll_amount_x(
    mouse: &mut Mouse,
    cell: rendererpkg::size::CellSize,
    xoff: f64,
    scroll_mods: input_mouse::ScrollMods,
) -> ScrollAmount {
    // const x: ScrollAmount = if (xoff == 0) .{} else x: {
    if xoff == 0.0 {
        ScrollAmount::default()
    } else {
        'x: {
            // if (!scroll_mods.precision) {
            if !scroll_mods.precision {
                // const x_delta_isize: isize = @intFromFloat(@round(xoff));
                //
                // (R13, UD62 — see the y calculation's note.)
                let Some(x_delta_isize) = isize_from_float(xoff.round()) else {
                    break 'x ScrollAmount::default();
                };
                // break :x .{ .delta = x_delta_isize };
                break 'x ScrollAmount {
                    delta: x_delta_isize,
                };
            }

            // const poff: f64 = self.mouse.pending_scroll_x + xoff;
            let poff: f64 = mouse.pending_scroll_x + xoff;
            // const cell_size: f64 = @floatFromInt(self.size.cell.width);
            let cell_size: f64 = f64::from(cell.width);
            // if (@abs(poff) < cell_size) {
            if poff.abs() < cell_size {
                // self.mouse.pending_scroll_x = poff;
                mouse.pending_scroll_x = poff;
                // break :x .{};
                break 'x ScrollAmount::default();
            }

            // const amount = poff / cell_size;
            let amount = poff / cell_size;
            // const delta: isize = @intFromFloat(@trunc(amount));
            //
            // (R13, UD62 — see the y calculation's note, including why the
            // guard runs ahead of the `amount` assert.)
            let Some(delta) = isize_from_float(amount.trunc()) else {
                break 'x ScrollAmount::default();
            };
            // assert(@abs(amount) >= 1);
            debug_assert!(amount.abs() >= 1.0);
            // self.mouse.pending_scroll_x = poff - (amount * cell_size);
            mouse.pending_scroll_x = poff - (amount * cell_size);
            // assert(@abs(delta) >= 1);
            debug_assert!(delta.unsigned_abs() >= 1);
            // break :x .{ .delta = delta };
            break 'x ScrollAmount { delta };
        }
    }
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

/// PORT NOTE: the anonymous `enum { down_left, up_right }` return type of
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

/// The portion of the screen to write for writeScreenFile.
// const WriteScreenLoc = enum {
//     screen, // Full screen
//     history, // History (scrollback)
//     selection, // Selected text
// };
//
// PORT NOTE (chunk C14b forward-declaration): a file-scope enum (the
// reference declares it at file scope between `closingAction` and
// `writeScreenFile`, Surface.zig:5644 — a Rust `enum` cannot live inside the
// `impl` block). Consumed by C14b's `performBindingAction` write arms and the
// forward-declared `Surface::writeScreenFile` skeleton; C15 fills the body
// (which uses `@tagName(loc)` for the file name). Tracked in DEFERRALS.md
// "core C14b".
#[derive(Clone, Copy)]
enum WriteScreenLoc {
    Screen,    // Full screen
    History,   // History (scrollback)
    Selection, // Selected text
}

/// Returns true if performing the given action result in closing
/// the surface. This is used to determine if our self pointer is
/// still valid after performing some binding action.
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

/// PORT NOTE: the nested `Text.Viewport` (Surface.zig:1875), hoisted to a
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

/// PORT NOTE: the file-scope `Link` returned by `linkAtPos` (Surface.zig:4271 —
/// declared between `maybePromptClick` and `linkAtPos`; Rust impl blocks
/// cannot hold type items, so it lives in this trailing cluster with a
/// position pointer at the reference site). NOT the `DerivedConfig.Link`
/// sibling ([`DerivedConfigLink`]).
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

/// The termio Copy/Export seam trampolines (`ad96613a8` — the inspector
/// widget's `copyEvents`/`exportEvents` reach `surface.rt_surface`/
/// `surface.rt_app` through the `surface: *Surface` they receive).
/// rustty-inspector cannot name core (the R4 acyclic Surface seam,
/// `rustty_inspector::widgets::surface` module doc), so the apprt caller
/// stores the erased `*mut Surface<A>` plus these monomorphized trampolines
/// into the inspector's `SurfaceData`; the widget invokes them with the values
/// the reference names at its callsites.
impl<A: apprt::App + 'static> Surface<A> {
    /// `surface.rt_surface.setClipboard(.standard, &.{.{ .mime = ..., .data =
    /// ... }}, confirm)` (termio.zig `copyEvents`): the `.standard` clipboard
    /// target is the only kind the widget names, fixed here; the concrete
    /// backend error erases to `Err(())` (consumed by the widget's `catch
    /// log.err` arm). Matches `SurfaceData::rt_set_clipboard`.
    ///
    /// # Safety
    ///
    /// `surface` must be the erased live `*mut Surface<A>` stored into the
    /// same `SurfaceData` as this trampoline, valid for the inspector draw
    /// call (INV-1/INV-11); `data`'s backing memory must carry a NUL one past
    /// the slice (the `[:0]const u8` convention of the `setClipboard`
    /// callsites above).
    pub unsafe fn inspector_rt_set_clipboard(
        surface: *mut core::ffi::c_void,
        mime: &[u8],
        data: &[u8],
        confirm: bool,
    ) -> Result<(), ()> {
        let self_: *mut Surface<A> = surface.cast();
        // SAFETY: caller contract above — `self_` and its `rt_surface` are
        // live for this call; a narrow shared reborrow of the runtime surface
        // (the same shape as this file's other `&*self.rt_surface` callsites).
        unsafe { &*(*self_).rt_surface }
            .set_clipboard(
                apprt::Clipboard::Standard,
                &[apprt::ClipboardContent { mime, data }],
                confirm,
            )
            .map_err(|_| ())
    }

    /// `surface.rt_app.performAction(.{ .surface = surface },
    /// .export_terminal_io, .{ .contents = contents })` (termio.zig
    /// `exportEvents`): returns the reference `!bool` ("handled"); the
    /// concrete backend error erases to `Err(())`. Matches
    /// `SurfaceData::rt_export_terminal_io`.
    ///
    /// # Safety
    ///
    /// `surface` must be the erased live `*mut Surface<A>` stored into the
    /// same `SurfaceData` as this trampoline, valid for the inspector draw
    /// call (INV-1/INV-11).
    pub unsafe fn inspector_rt_export_terminal_io(
        surface: *mut core::ffi::c_void,
        contents: &[u8],
    ) -> Result<bool, ()> {
        let self_: *mut Surface<A> = surface.cast();
        // SAFETY: caller contract above — `self_` and its `rt_app` are live
        // for this call; the `&*self_` target reborrow is narrow (the action
        // dispatch), the same shape as this file's other `performAction`
        // callsites.
        unsafe {
            (*(*self_).rt_app).perform_action(
                apprt::Target::Surface(&*self_),
                apprt::Action::ExportTerminalIO(apprt::action::ExportTerminalIO { contents }),
            )
        }
        .map_err(|_| ())
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

    /// `self.surface` for termio's io-thread crash metadata (LEDGER S211-F2,
    /// S212-F2); `Surface<A>` is the `ThreadStateSurface` impl above.
    fn crash_surface(&self) -> *const dyn crash::sentry::ThreadStateSurface {
        self.surface as *const Surface<A>
    }

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
    //
    // PORT NOTE (core C-T): the reference `push` is realized as the retain-on-
    // failure primitive `push_or_return` (the plain `AppMailbox::push` is the
    // trait's drop-on-failure default over it). termio's
    // `StreamHandler.surfaceMessageWriter` re-pushes its by-value message copy
    // after a failed instant push; since Rust moves the message into the app
    // queue, the failed push hands the App-level message back and we unwrap the
    // surface envelope so the caller regains its original `SurfaceMessage`.
    #[allow(clippy::result_large_err)]
    fn push_or_return(
        &self,
        msg: Message<'static>,
        timeout: Timeout,
    ) -> Result<QueueSize, Message<'static>> {
        // Surface message sending is actually implemented on the app
        // thread, so we have to rewrap the message with our surface
        // pointer and send it to the app thread.
        // return self.app.push(.{
        //     .surface_message = .{
        //         .surface = self.surface,
        //         .message = msg,
        //     },
        // }, timeout);
        match self.app.push_or_return(
            crate::app::Message::SurfaceMessage(apprt::SurfaceMessageEnvelope {
                surface: self.surface,
                message: msg,
            }),
            timeout,
        ) {
            Ok(size) => Ok(size),
            // The app queue hands the App-level message back on failure; unwrap
            // the surface envelope to return the caller's original message.
            Err(crate::app::Message::SurfaceMessage(env)) => Err(env.message),
            // Only `SurfaceMessage` is ever constructed above, so no other
            // returned variant is possible.
            Err(_) => unreachable!(),
        }
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
// PORT NOTE: helpers — std analogues used above (std.fs.path / std.fs.access /
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

/// Raw bytes of a `Path` (the inverse of [`os_str_from_bytes`]): the reference
/// `std.fs.Dir.realpath` returns a `[]const u8` path slice; the Rustty port
/// returns an owning `PathBuf` and `writeScreenFile` recovers its bytes here
/// (POSIX paths are bytes directly — R3/fidelity, never a lossy round-trip).
#[cfg(unix)]
fn path_to_bytes(path: &std::path::Path) -> &[u8] {
    use std::os::unix::ffi::OsStrExt;
    path.as_os_str().as_bytes()
}

#[cfg(not(unix))]
fn path_to_bytes(path: &std::path::Path) -> &[u8] {
    // PORT NOTE: on non-unix the path is UTF-8 (lossy fallback); the
    // scrollback-file flow is primarily exercised on POSIX.
    path.to_str().unwrap_or("").as_bytes()
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

/// PORTS: the Zig `std.base64` STANDARD codec's `Encoder` surface
/// (`calcSize` + `encode`) used by `completeClipboardReadOSC52` (the OSC 52
/// reply) — semantics reproduced bit-for-bit from lib/std/base64.zig
/// (Zig 0.15.2). The encoder's 128-/32-bit `readInt` SIMD-ish fast loops are
/// accelerative only; the scalar tails below ARE the semantic core, ported
/// verbatim (the rustty-os file.rs `b64_encoder` precedent).
///
/// PORT NOTE (bc2f7d7d2): the `Decoder` surface (`calcSizeForSlice` +
/// `decode`) that `clipboardWrite` consumed is GONE with the oracle — the
/// OSC 52 write path now decodes through `simd.base64.decodeStrict`.
mod zig_base64 {
    // pub const standard_alphabet_chars = "ABC...xyz0123456789+/".*;
    const ALPHABET_CHARS: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    // .pad_char = '=',
    const PAD_CHAR: u8 = b'=';

    // ---- Base64Encoder (std.base64.standard.Encoder) ----

    /// Compute the encoded length.
    // pub fn calcSize(encoder: *const Base64Encoder, source_len: usize) usize {
    //     if (encoder.pad_char != null) {
    //         return @divTrunc(source_len + 2, 3) * 4;
    //     } else { ... }
    // }
    //
    // (the standard codec HAS a pad char — the no-pad arm is dead for this
    // codec and elided, matching the decoder's dead-arm elisions above.)
    //
    // PARITY: clippy suggests `.div_ceil(3)` for `(source_len + 2) / 3`, but
    // rewriting would diverge from the reference `@divTrunc` expression; kept
    // as-is for fidelity (the rustty-os file.rs `b64_encoder::calc_size`
    // precedent).
    #[allow(clippy::manual_div_ceil)]
    pub const fn calc_size(source_len: usize) -> usize {
        // return @divTrunc(source_len + 2, 3) * 4;
        (source_len + 2) / 3 * 4
    }

    /// dest.len must at least be what you get from ::calcSize.
    // pub fn encode(encoder: *const Base64Encoder, dest: []u8, source: []const u8) []const u8 {
    pub fn encode<'a>(dest: &'a mut [u8], source: &[u8]) -> &'a [u8] {
        // const out_len = encoder.calcSize(source.len);
        let out_len = calc_size(source.len());
        // assert(dest.len >= out_len);
        debug_assert!(dest.len() >= out_len);

        // var idx: usize = 0;
        let mut idx: usize = 0;
        // var out_idx: usize = 0;
        let mut out_idx: usize = 0;
        // (the 128-bit big-endian `readInt` fast loop is accelerative only — it
        // requires a 4-byte-aligned lookahead and produces output identical to
        // the scalar 3-byte grouping below; elided as with the decoder's fast
        // loops. The 32-bit `readInt` loop below is scalarized: `readInt(u32,
        // .big)` reads a lookahead 4th byte whose low 8 bits are unused, so the
        // four 6-bit fields it extracts equal the byte-shuffle here.)
        // while (idx + 3 < source.len) : (idx += 3) {
        while idx + 3 < source.len() {
            // dest[out_idx] = alphabet[(bits >> 26) & 0x3f];   (= source[idx] >> 2)
            dest[out_idx] = ALPHABET_CHARS[(source[idx] >> 2) as usize];
            // dest[out_idx + 1] = alphabet[(bits >> 20) & 0x3f];
            dest[out_idx + 1] =
                ALPHABET_CHARS[(((source[idx] & 0x3) << 4) | (source[idx + 1] >> 4)) as usize];
            // dest[out_idx + 2] = alphabet[(bits >> 14) & 0x3f];
            dest[out_idx + 2] =
                ALPHABET_CHARS[(((source[idx + 1] & 0xf) << 2) | (source[idx + 2] >> 6)) as usize];
            // dest[out_idx + 3] = alphabet[(bits >> 8) & 0x3f];   (= source[idx+2] & 0x3f)
            dest[out_idx + 3] = ALPHABET_CHARS[(source[idx + 2] & 0x3f) as usize];
            // out_idx += 4;
            out_idx += 4;
            idx += 3;
        }
        // if (idx + 2 < source.len) {
        if idx + 2 < source.len() {
            // dest[out_idx] = alphabet[source[idx] >> 2];
            dest[out_idx] = ALPHABET_CHARS[(source[idx] >> 2) as usize];
            // dest[out_idx + 1] = alphabet[((source[idx] & 0x3) << 4) | (source[idx + 1] >> 4)];
            dest[out_idx + 1] =
                ALPHABET_CHARS[(((source[idx] & 0x3) << 4) | (source[idx + 1] >> 4)) as usize];
            // dest[out_idx + 2] = alphabet[(source[idx + 1] & 0xf) << 2 | (source[idx + 2] >> 6)];
            dest[out_idx + 2] =
                ALPHABET_CHARS[(((source[idx + 1] & 0xf) << 2) | (source[idx + 2] >> 6)) as usize];
            // dest[out_idx + 3] = alphabet[source[idx + 2] & 0x3f];
            dest[out_idx + 3] = ALPHABET_CHARS[(source[idx + 2] & 0x3f) as usize];
            // out_idx += 4;
            out_idx += 4;
        // } else if (idx + 1 < source.len) {
        } else if idx + 1 < source.len() {
            // dest[out_idx] = alphabet[source[idx] >> 2];
            dest[out_idx] = ALPHABET_CHARS[(source[idx] >> 2) as usize];
            // dest[out_idx + 1] = alphabet[((source[idx] & 0x3) << 4) | (source[idx + 1] >> 4)];
            dest[out_idx + 1] =
                ALPHABET_CHARS[(((source[idx] & 0x3) << 4) | (source[idx + 1] >> 4)) as usize];
            // dest[out_idx + 2] = alphabet[(source[idx + 1] & 0xf) << 2];
            dest[out_idx + 2] = ALPHABET_CHARS[((source[idx + 1] & 0xf) << 2) as usize];
            // out_idx += 3;
            out_idx += 3;
        // } else if (idx < source.len) {
        } else if idx < source.len() {
            // dest[out_idx] = alphabet[source[idx] >> 2];
            dest[out_idx] = ALPHABET_CHARS[(source[idx] >> 2) as usize];
            // dest[out_idx + 1] = alphabet[(source[idx] & 0x3) << 4];
            dest[out_idx + 1] = ALPHABET_CHARS[((source[idx] & 0x3) << 4) as usize];
            // out_idx += 2;
            out_idx += 2;
        }
        // if (encoder.pad_char) |pad_char| {
        //     for (dest[out_idx..out_len]) |*pad| pad.* = pad_char;
        // }
        for pad in &mut dest[out_idx..out_len] {
            *pad = PAD_CHAR;
        }
        // return dest[0..out_len];
        &dest[0..out_len]
    }
}

// PORT NOTE (rustty-only, CORE_RENDERER_CONFIG_INTEGRATION_DESIGN §4): these
// tests have NO Zig `test{}` counterpart — they exercise the core-side
// consequence of the non-windowing renderer `DerivedConfig` swap. Core deps
// BOTH `rustty-config` (for `Config`) AND `rustty-renderer`, and binds the
// concrete `C = rustty_config::Config`, so it is the FIRST place anywhere the
// real `impl RendererConfig for Config` (rustty-config's `renderer_config.rs`)
// executes: the renderer crate can only test `init<C>` with its `MockConfig`
// (it cannot dep `rustty-config` — that is the G7 cycle, R4). Both are headless
// (no Metal, no NSView, no thread); `Surface::init` itself stays untestable
// (it panics earlier at the `Renderer::surfaceInit` windowing seam, §3).
#[cfg(test)]
mod renderer_config_tests {
    use super::*;
    // The renderer-defined accessor trait (`config.<field>()`); the concrete
    // `impl RendererConfig for Config` lives in rustty-config.
    use rendererpkg::config::RendererConfig;

    // T1 — a real `DerivedConfig` derived from a real `Config`.
    #[test]
    // Miri cannot execute the oniguruma C FFI this test deliberately reaches
    // (`extern static OnigEncodingUTF8`, via `link::Set::from_config` regex
    // compilation): "unsupported operation", not a soundness verdict. Covered
    // natively by `cargo test --workspace` (CI HARD step 4) and by the
    // rustty-config/rustty-oniguruma ASan G6 lanes. See MIRI_EXCEPTIONS.md
    // ("rustty-core renderer_config_tests").
    #[cfg_attr(
        miri,
        ignore = "real Config compiles link regexes through oniguruma C FFI (extern static OnigEncodingUTF8) — un-executable under Miri; ASan + native lanes cover it"
    )]
    fn renderer_derived_config_from_real_config() {
        // OnceLock C-init guard (memory `parallel-cargo-test-exposes-c-init-races`):
        // `init<C>` reaches `link::Set::from_config`, which compiles the default
        // config's link regex through the non-thread-safe oniguruma C FFI.
        oni::testing::ensure_init().unwrap();
        let cfg = configpkg::Config::default_config();
        let d = rendererpkg::DerivedConfig::init(Allocator, &cfg).expect("init");
        // Assert the extracted fields track the `Config` defaults, proving the
        // config-side `impl RendererConfig for Config` accessors + `init<C>`'s
        // clamps/casts:
        assert_eq!(d.font_thicken, cfg.font_thicken()); // bool passthrough
        assert!((0.0..=1.0).contains(&d.background_opacity)); // init clamps [0,1]
        assert_eq!(d.faint_opacity, (cfg.faint_opacity() * 255.0).ceil() as u8);
        // frees `links`' onig handles (no `Drop`) — no leak.
        let mut d = d;
        d.deinit();
    }

    // T2 — a real `Message::ChangeConfig` end-to-end (no `Surface` needed).
    #[test]
    // Same Miri FFI limit as T1 above (oniguruma extern static via the real
    // `Config`); same native + ASan coverage. See MIRI_EXCEPTIONS.md.
    #[cfg_attr(
        miri,
        ignore = "real Config compiles link regexes through oniguruma C FFI (extern static OnigEncodingUTF8) — un-executable under Miri; ASan + native lanes cover it"
    )]
    fn init_change_config_from_real_config() {
        // OnceLock C-init guard (see T1).
        oni::testing::ensure_init().unwrap();
        let cfg = configpkg::Config::default_config();
        let msg =
            rendererpkg::Message::initChangeConfig(Allocator, &cfg).expect("initChangeConfig");
        match msg {
            // both `DerivedConfig`s (renderer + thread) built.
            rendererpkg::Message::ChangeConfig { .. } => {}
            _ => panic!("expected ChangeConfig"),
        }
        // frees `impl.links` onig handles then drops both boxes.
        msg.deinit();
    }
}

// rustty-only tests: lock the `isize_from_float` cast helper's contract.
//
// Task R13-P (`UPSTREAM-DEFECTS.md` UD62) REVERSED the two tests that used to
// live here, which pinned the reference's trap (a `debug_assert!` copy of
// `@intFromFloat`'s safety check). The trap is reachable from a `keybind` value
// and from a C-ABI embedder's scroll offset, so under R13 the port does not
// reproduce it.
#[cfg(test)]
mod isize_from_float_tests {
    use super::{isize_from_float, isize_from_float_saturating};

    #[test]
    fn isize_from_float_in_range_is_exact() {
        assert_eq!(isize_from_float(3.0), Some(3));
        assert_eq!(isize_from_float(-2.0), Some(-2));
        // `@intFromFloat` converts the integer part (truncation toward zero).
        assert_eq!(isize_from_float(0.9), Some(0));
        assert_eq!(isize_from_float(-1.9), Some(-1));
    }

    /// The values the reference traps on (measured in Debug and ReleaseSafe:
    /// infinities and finite values outside `isize`) have no `isize`, and the
    /// gesture call sites scroll nothing for them instead of crashing or
    /// looping to `isize::MAX` (`scroll_gesture_cast_tests` drives those).
    #[test]
    fn isize_from_float_reports_the_values_the_reference_traps_on() {
        assert_eq!(isize_from_float(f64::INFINITY), None);
        assert_eq!(isize_from_float(f64::NEG_INFINITY), None);
        assert_eq!(isize_from_float(1e300), None);
        assert_eq!(isize_from_float(-1e300), None);
        // The boundary: 2^63 has no `isize`, the largest f64 below it does.
        assert_eq!(isize_from_float(-(isize::MIN as f64)), None);
        assert_eq!(isize_from_float(isize::MIN as f64), Some(isize::MIN));
    }

    /// NaN is NOT a value the reference traps on: the safety check lets it
    /// through, and the result is target-dependent — `0` on arm64 and
    /// `isize::MIN` on x86_64, in every build mode (measured, task R13-P2).
    /// It has no `isize` either, so it is reported like the trapping values and
    /// each caller's arm decides what it means (task R13-P2 re-checked them:
    /// see `isize_from_float`).
    #[test]
    fn isize_from_float_reports_nan_whose_reference_value_is_target_dependent() {
        assert_eq!(isize_from_float(f64::NAN), None);
        assert_eq!(isize_from_float(-f64::NAN), None);
    }

    /// `scroll_page_fractional`'s arm: `keybind = …=scroll_page_fractional:1e30`
    /// means "scroll as far as it goes", and the viewport clamps it (the
    /// companion test below drives the clamp).
    #[test]
    fn isize_from_float_saturating_takes_the_end_in_each_direction() {
        assert_eq!(isize_from_float_saturating(12.0), 12);
        assert_eq!(isize_from_float_saturating(1e300), isize::MAX);
        assert_eq!(isize_from_float_saturating(f64::INFINITY), isize::MAX);
        assert_eq!(isize_from_float_saturating(-1e300), isize::MIN);
        assert_eq!(isize_from_float_saturating(f64::NEG_INFINITY), isize::MIN);
        // A NaN fraction has no direction, so it scrolls nothing.
        assert_eq!(isize_from_float_saturating(f64::NAN), 0);
    }
}

/// The other half of `scroll_page_fractional`'s handling: the saturated delta
/// this surface queues is safe at its destination. `scroll_viewport` takes the
/// magnitude with `unsigned_abs` (so `isize::MIN` is exact) and clamps to the
/// top or the active area, without a panic and without walking one row at a
/// time.
#[cfg(test)]
mod scroll_page_fractional_saturation_tests {
    use super::*;

    #[test]
    fn a_saturated_scroll_delta_is_clamped_by_the_viewport() {
        let mut term = terminal::terminal::Terminal::init(
            Allocator,
            terminal::terminal::Options {
                cols: 10,
                rows: 4,
                max_scrollback_bytes: Some(10_000),
                max_scrollback_lines: None,
                colors: rustty_vt::terminal::Colors::default(),
                default_modes: rustty_vt::modes::ModePacked::default(),
                default_cursor_style: rustty_vt::screen::CursorStyle::Block,
                default_cursor_blink: Some(false),
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: rustty_vt::kitty::graphics_image::Limits::DIRECT,
            },
        )
        .expect("Terminal::init");

        // Some scrollback to scroll through.
        for _ in 0..20 {
            term.print(u32::from('x')).unwrap();
            term.index().unwrap();
        }

        // `scroll_page_fractional:-1e30` -> isize::MIN; `:1e30` -> isize::MAX.
        term.scroll_viewport(terminal::terminal::ScrollViewport::Delta(isize::MIN));
        term.scroll_viewport(terminal::terminal::ScrollViewport::Delta(isize::MAX));
        term.scroll_viewport(terminal::terminal::ScrollViewport::Delta(0));

        term.deinit(Allocator);
    }
}

// rustty-only (task R13-P2, UPSTREAM-DEFECTS.md UD62): the three gesture float
// casts in `scrollCallback` — `Surface.zig:3520` (y), `:3529` (x, discrete) and
// `:3543` (x, precision). `Surface::init` cannot run without an apprt, so these
// drive `scroll_amount_y` / `scroll_amount_x`, the callback's own y and x blocks,
// with the mods decoded by `ScrollMods::from_int` as the C-ABI entry
// `rustty_surface_mouse_scroll` decodes them. Each test pins ONE cast: a value
// the reference traps on must take the no-scroll arm (a saturating cast would
// return `isize::MAX`/`MIN` here, which the callback's per-row loops would then
// walk), the pending offset must be left as it was, and an in-range offset must
// still scroll by the exact amount.
#[cfg(test)]
mod scroll_gesture_cast_tests {
    use super::*;

    const CELL: rendererpkg::size::CellSize = rendererpkg::size::CellSize {
        width: 10,
        height: 20,
    };

    /// `ScrollMods` as the C-ABI entry decodes the host's `int` (bit 0 is
    /// `precision`).
    fn mods(precision: bool) -> input_mouse::ScrollMods {
        input_mouse::ScrollMods::from_int(u8::from(precision))
    }

    /// Offsets the reference traps on at the cast (measured: 1e300 and inf).
    const TRAPPING: [f64; 4] = [1e300, -1e300, f64::INFINITY, f64::NEG_INFINITY];

    fn none() -> ScrollAmount {
        ScrollAmount::default()
    }

    #[test]
    fn y_gesture_cast_out_of_range_scrolls_nothing() {
        let multiplier = configpkg::value_types::MouseScrollMultiplier::default();
        let mut mouse = Mouse::default();

        // Positive control: 45 px of precision scroll over 20 px cells.
        assert_eq!(
            ScrollAmount { delta: 2 },
            scroll_amount_y(&mut mouse, CELL, multiplier, 45.0, mods(true))
        );
        assert_eq!(
            ScrollAmount { delta: -2 },
            scroll_amount_y(&mut mouse, CELL, multiplier, -45.0, mods(true))
        );

        for precision in [true, false] {
            for v in TRAPPING {
                mouse.pending_scroll_y = 5.0;
                assert_eq!(
                    none(),
                    scroll_amount_y(&mut mouse, CELL, multiplier, v, mods(precision)),
                    "yoff {v} (precision {precision}) scrolls nothing"
                );
                assert_eq!(
                    5.0, mouse.pending_scroll_y,
                    "yoff {v} (precision {precision}) leaves the pending offset alone"
                );
            }
        }

        // NaN (task R13-C): on the precision path a NaN offset makes `amount`
        // NaN, which the reference's `assert(@abs(amount) >= 1)` rejects; the
        // guard now runs ahead of that assert, so a NaN from the C ABI scrolls
        // nothing instead of failing the `debug_assert!` (an abort inside the
        // `extern "C"` entry point in debug and `release-safe` builds). The
        // discrete path is not driven with NaN: on Darwin `@min(nan, -1)` is
        // -1 in both trees (measured, Zig 0.16.0), so it is an ordinary tick.
        for v in [f64::NAN, -f64::NAN] {
            mouse.pending_scroll_y = 5.0;
            assert_eq!(
                none(),
                scroll_amount_y(&mut mouse, CELL, multiplier, v, mods(true)),
                "precision yoff NaN scrolls nothing"
            );
            assert!(
                mouse.pending_scroll_y == 5.0,
                "precision yoff NaN leaves the pending offset alone, got {}",
                mouse.pending_scroll_y
            );
        }

        // Not poisoned: the next ordinary scroll still adds to the kept 5 px.
        assert_eq!(
            ScrollAmount { delta: 2 },
            scroll_amount_y(&mut mouse, CELL, multiplier, 40.0, mods(true))
        );
    }

    #[test]
    fn x_discrete_gesture_cast_out_of_range_scrolls_nothing() {
        let mut mouse = Mouse::default();

        // Positive control: a discrete x offset is rounded, not accumulated.
        assert_eq!(
            ScrollAmount { delta: 3 },
            scroll_amount_x(&mut mouse, CELL, 2.6, mods(false))
        );
        assert_eq!(
            ScrollAmount { delta: -2 },
            scroll_amount_x(&mut mouse, CELL, -2.4, mods(false))
        );

        // 9.3e18 is just past `isize::MAX`. NaN does not trap in the reference
        // (0 on arm64, `isize::MIN` on x86_64, measured) and has no assert on
        // this path, so it is driven here too.
        for v in TRAPPING.into_iter().chain([9.3e18, f64::NAN]) {
            mouse.pending_scroll_x = 3.0;
            assert_eq!(
                none(),
                scroll_amount_x(&mut mouse, CELL, v, mods(false)),
                "discrete xoff {v} scrolls nothing"
            );
            assert_eq!(3.0, mouse.pending_scroll_x);
        }
    }

    #[test]
    fn x_precision_gesture_cast_out_of_range_scrolls_nothing() {
        // Positive control: 3 px pending + 25 px over 10 px cells.
        let mut mouse = Mouse {
            pending_scroll_x: 3.0,
            ..Mouse::default()
        };
        assert_eq!(
            ScrollAmount { delta: 2 },
            scroll_amount_x(&mut mouse, CELL, 25.0, mods(true))
        );

        // NaN (task R13-C): as on the y precision path, the guard runs ahead
        // of the `amount` assert a NaN would fail.
        for v in TRAPPING.into_iter().chain([f64::NAN, -f64::NAN]) {
            mouse.pending_scroll_x = 3.0;
            assert_eq!(
                none(),
                scroll_amount_x(&mut mouse, CELL, v, mods(true)),
                "precision xoff {v} scrolls nothing"
            );
            assert_eq!(
                3.0, mouse.pending_scroll_x,
                "precision xoff {v} leaves the pending offset alone"
            );
        }

        // Not poisoned.
        assert_eq!(
            ScrollAmount { delta: 2 },
            scroll_amount_x(&mut mouse, CELL, 25.0, mods(true))
        );
    }
}

// Task 6 inspector-lifecycle KAT. A full Surface-level activate->deactivate
// cannot be exercised in a unit test — `Surface::init` spawns the render/io
// threads and needs a live apprt `App` + pty — so this KAT proves the exact
// cross-crate seams `activateInspector` / `deactivateInspector` / the keyCallback
// inspector defer wire up, which ARE constructible without a Surface. The Surface
// control flow itself (the mutex/mailbox/queueIo sequence) is covered by the
// workspace build gate + the app shakedown (plan Task 18).
#[cfg(test)]
mod inspector_lifecycle_tests {
    use super::*;

    // The R2 acyclic overlay coercion `activateInspector` performs: an inspector's
    // `renderer_info` sub-object unsize-coerces to the renderer-owned
    // `*mut dyn InspectorOverlay` fat pointer, through which the renderer reads
    // overlay features (INV-7). Proves the type wiring compiles AND the call
    // crosses the seam end-to-end (a fresh inspector enables no overlays).
    #[test]
    fn inspector_renderer_info_coerces_to_overlay_and_reads() {
        let mut insp = inspectorpkg::Inspector::init(Allocator);

        // Exactly the coercion `activateInspector` stores onto renderer_state.inspector.
        let info_ref: &mut dyn rendererpkg::state::InspectorOverlay = &mut insp.gui.renderer_info;
        let info_ptr: *mut rendererpkg::state::Inspector = info_ref;

        // The renderer reads overlay features through the fat pointer.
        let features = unsafe { (*info_ptr).overlay_features() };
        assert!(features.is_empty());

        insp.deinit(Allocator);
    }

    // The keyCallback inspector defer records key events via `recordKeyEvent` with
    // the max_capacity=50 grow-then-evict policy (INV-9). Filling well past capacity
    // must never surface an error (the ring self-evicts) and never panic — driving
    // both the `resize` grow branch and the `delete_oldest` + re-append evict branch.
    #[test]
    fn record_key_event_grows_then_evicts_bounded() {
        let mut insp = inspectorpkg::Inspector::init(Allocator);

        for _ in 0..200 {
            insp.recordKeyEvent(Allocator, inspectorpkg::KeyEvent::default())
                .expect("recordKeyEvent self-evicts at capacity, never surfaces OutOfMemory");
        }

        insp.deinit(Allocator);
    }
}

// `freetype-load-flags` DI-bridge KAT (`fix-freetype-tmux` lane).
//
// The bug this pins: `font_shared_grid_set_derived_config` used to assign
// `face::FREETYPE_LOAD_FLAGS_DEFAULT` unconditionally, so a user's configured
// `freetype-load-flags` was parsed, accepted, and then silently discarded on
// every freetype-active build. The reference assigns
// `if (FreetypeLoadFlags != void) config.@"freetype-load-flags" else {}`
// (`SharedGridSet.zig:528`).
#[cfg(test)]
mod freetype_load_flags_tests {
    use super::*;

    /// A value where EVERY field is the opposite of
    /// `config::FreetypeLoadFlags::default()`, so any test below fails if the
    /// implementation substitutes the default for the configured value.
    fn all_flipped() -> configpkg::value_types::FreetypeLoadFlags {
        let d = configpkg::value_types::FreetypeLoadFlags::default();
        let flipped = configpkg::value_types::FreetypeLoadFlags {
            hinting: !d.hinting,
            force_autohint: !d.force_autohint,
            monochrome: !d.monochrome,
            autohint: !d.autohint,
            light: !d.light,
        };
        assert_ne!(flipped, d, "the fixture must be non-default");
        flipped
    }

    // The DI bridge threads all five fields; a hard-coded default fails here.
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn from_config_threads_every_field() {
        let cfg = all_flipped();
        let got = freetype_load_flags_from_config(&cfg);

        assert_eq!(got.hinting, cfg.hinting);
        assert_eq!(got.force_autohint, cfg.force_autohint);
        assert_eq!(got.monochrome, cfg.monochrome);
        assert_eq!(got.autohint, cfg.autohint);
        assert_eq!(got.light, cfg.light);

        // And the whole value must differ from the shaper-tier default — the
        // exact discrimination the original bug lacked.
        assert_ne!(
            got,
            rustty_font_shaper::face::freetype::FreetypeLoadFlags::DEFAULT,
            "a non-default config must NOT map onto the default load flags"
        );
    }

    // The two tiers' stand-ins for the reference's single
    // `config.FreetypeLoadFlags` must stay field-identical: if a field is added
    // to one and not the other this stops compiling / stops round-tripping.
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn default_round_trips_exactly() {
        let d = configpkg::value_types::FreetypeLoadFlags::default();
        assert_eq!(
            freetype_load_flags_from_config(&d),
            rustty_font_shaper::face::freetype::FreetypeLoadFlags::DEFAULT,
        );
    }

    // `SharedGridSet.Key.hash` feeds the load flags to the hasher
    // (`autoHash(hasher, self.freetype_load_flags)`); the port used to skip that
    // field outright, so two grids differing ONLY in `freetype-load-flags` hashed
    // identically and the second config silently reused the first's cached grid.
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn load_flags_are_hash_distinguishing() {
        use core::hash::{Hash, Hasher};

        fn h(v: rustty_font_shaper::face::freetype::FreetypeLoadFlags) -> u64 {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            v.hash(&mut hasher);
            hasher.finish()
        }

        let d = rustty_font_shaper::face::freetype::FreetypeLoadFlags::DEFAULT;
        assert_ne!(
            h(d),
            h(freetype_load_flags_from_config(&all_flipped())),
            "two distinct load-flag configs must not collide on the same grid key"
        );
    }

    // The reference's `else {}` (void) arm: on the macOS `coretext` backend
    // `hasFreetype()` is false, so the config value IS legitimately dropped and
    // the shaper type is `()`. There is nothing runtime to assert (a `#[test]`
    // comparing two unit values always succeeds — a worthless test), so the arm
    // is pinned at COMPILE time instead: the selected type must be zero-sized.
    #[cfg(target_os = "macos")]
    const _: () = assert!(size_of::<rustty_font_shaper::face::FreetypeLoadFlags>() == 0);

    // THE ORIGINAL BUG, at the exact site that had it. Only compiles/runs on the
    // freetype-active backends (Linux `fontconfig_freetype`), because on macOS
    // the reference itself takes the `void` arm and there is nothing to thread.
    #[test]
    #[cfg(not(any(target_os = "macos", target_arch = "wasm32")))]
    fn derived_config_carries_the_configured_flags() {
        let config = configpkg::Config {
            freetype_load_flags: all_flipped(),
            ..Default::default()
        };

        let derived = font_shared_grid_set_derived_config(&config);
        assert_eq!(
            derived.freetype_load_flags,
            freetype_load_flags_from_config(&config.freetype_load_flags),
        );
        assert_ne!(
            derived.freetype_load_flags,
            rustty_font_shaper::face::FREETYPE_LOAD_FLAGS_DEFAULT,
            "regression: the configured freetype-load-flags were replaced by the default"
        );
    }
}

// test "queueIo frees allocated writes in readonly mode" {
//
// (`957ed21d5`; the readonly discard arm's free is `msg.deinit()` at END
// STATE (`2f7fbadb0`) == the Rust drop — see the arm's PORT NOTE in
// `queueIo`.)
#[cfg(test)]
#[allow(non_snake_case)]
mod queue_io_readonly_tests {
    use super::*;
    use crate::app::test_runtime::TestApp;

    #[test]
    fn queueIo_frees_allocated_writes_in_readonly_mode() {
        // const testing = std.testing;
        //
        // const surface = try testing.allocator.create(Surface);
        // defer testing.allocator.destroy(surface);
        // surface.readonly = true;
        //
        // PORT NOTE: `testing.allocator.create(Surface)` returns UNINITIALIZED
        // memory and the test writes exactly ONE field — the established
        // `fabricateSurface`/`surface_visibility_kat` MaybeUninit fabrication
        // (field-projected raw-pointer writes; the untouched fields are never
        // read — the readonly discard arm returns before `self.io` — and
        // `MaybeUninit` never drops, which is `destroy`-without-`deinit`).
        let mut surface: Box<std::mem::MaybeUninit<Surface<TestApp>>> =
            Box::new(std::mem::MaybeUninit::uninit());
        let ptr = surface.as_mut_ptr();
        unsafe { std::ptr::addr_of_mut!((*ptr).readonly).write(true) };

        // queueIo must free allocated writes in read-only mode.
        // const data = try testing.allocator.dupe(u8, "\x1b]lGhostty\x1b\\");
        // surface.queueIo(.{ .write_alloc = .{
        //     .alloc = testing.allocator,
        //     .data = data,
        // } }, .unlocked);
        //
        // (R2: the test payload's product name; the free assertion is the
        // allocator leak check — Zig's `testing.allocator` there, the Miri
        // leak gate here. A leak would also re-appear as the `Box<[u8]>` not
        // dropping under any leak-checking run of this lane.)
        let data: Box<[u8]> = b"\x1b]lRustty\x1b\\".to_vec().into_boxed_slice();
        unsafe { &mut *ptr }.queueIo(
            termio::Message::WriteAlloc(Alloc { data }),
            termio::termio_impl::MutexState::Unlocked,
        );
    }
}

// S151-F1 production-path regression: `Surface::copySelectionToClipboards` is
// the ONLY place the `clipboard-codepoint-map` config option reaches a
// formatter (built with `codepoint_map: Some(config.clipboard_codepoint_map
// .map.list.clone())` a few lines above). The bug lived entirely inside
// `rustty_vt::formatter`'s internal `copy_options` helper, which every
// delegation hop (`ScreenFormatter` -> `PageListFormatter` -> `PageFormatter`)
// used and which hard-coded `codepoint_map: Some(Vec::new())` — so the config
// option compiled, parsed, and did nothing. `formatter.rs`'s own unit test
// (`codepoint_map_survives_the_screen_formatter_delegation`) proves the vt
// crate's delegation chain in isolation; this test proves the actual
// production call: a real `Config` with `clipboard-codepoint-map` set, run
// through the real `copySelectionToClipboards`, landing in the runtime's
// `setClipboard`.
//
// `Surface::init` itself is untestable (it spawns a live pty/renderer/io
// thread — see the `renderer_config_tests` module doc above and
// `surface_visibility_kat.rs`'s V21 note), so this follows that file's
// established fabrication recipe: a heap-pinned `Surface<TestApp>` in
// `MaybeUninit` with ONLY the fields `copySelectionToClipboards` reads
// written (`io.terminal`, `config`, `rt_surface`) — sound because the method
// never touches any other field, and `MaybeUninit` never drops fields that
// were never initialized. The three fields that WERE written (unlike, say,
// `surface_visibility_kat.rs`'s `bool`/pointer fields) own heap memory —
// `Terminal` and `DerivedConfig` both need their own explicit `deinit`, and
// `rt_surface` its own `Box::from_raw` + drop — so the test's teardown frees
// all three explicitly rather than relying on any implicit drop.
#[cfg(test)]
mod clipboard_codepoint_map_e2e {
    use super::*;
    use std::mem::MaybeUninit;
    use std::sync::Mutex;

    struct TestApp {
        config: configpkg::Config,
    }

    impl apprt::App for TestApp {
        type Surface = TestRtSurface;

        fn perform_action<C: apprt::CoreSurfaceHandle>(
            &self,
            _target: apprt::Target<'_, C>,
            _action: apprt::Action<'_>,
        ) -> Result<bool, apprt::ApprtError> {
            unimplemented!("S151-F1 KAT: not driven")
        }

        fn perform_ipc(
            &self,
            _target: apprt::ipc::Target<'_>,
            _action: apprt::ipc::Action<'_>,
        ) -> Result<bool, apprt::ipc::Errors> {
            unimplemented!("S151-F1 KAT: not driven")
        }

        fn wakeup(&self) {}

        fn config(&self) -> &configpkg::Config {
            &self.config
        }

        fn keyboard_layout(&self) -> rustty_input::keyboard::Layout {
            rustty_input::keyboard::Layout::Unknown
        }
    }

    /// Captures every `setClipboard` call so the test can assert on the
    /// bytes the production formatter actually produced.
    struct TestRtSurface {
        core: *mut Surface<TestApp>,
        app: *mut TestApp,
        captured: Mutex<Vec<(Vec<u8>, Vec<u8>)>>,
    }

    impl apprt::Surface for TestRtSurface {
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
            unimplemented!("S151-F1 KAT: not driven")
        }
        fn get_size(&self) -> Result<apprt::SurfaceSize, apprt::ApprtError> {
            unimplemented!("S151-F1 KAT: not driven")
        }
        fn get_title(&self) -> Option<&std::ffi::CStr> {
            None
        }
        fn supports_clipboard(&self, _clipboard_type: apprt::Clipboard) -> bool {
            unimplemented!("S151-F1 KAT: not driven")
        }
        fn clipboard_request(
            &self,
            _clipboard_type: apprt::Clipboard,
            _state: apprt::ClipboardRequest,
        ) -> Result<apprt::ClipboardReadResult, apprt::ApprtError> {
            unimplemented!("S151-F1 KAT: not driven")
        }
        fn set_clipboard(
            &self,
            _clipboard_type: apprt::Clipboard,
            contents: &[apprt::ClipboardContent<'_>],
            _confirm: bool,
        ) -> Result<(), apprt::ApprtError> {
            let mut captured = self.captured.lock().unwrap();
            for c in contents {
                captured.push((c.mime.to_vec(), c.data.to_vec()));
            }
            Ok(())
        }
        fn get_cursor_pos(&self) -> Result<apprt::CursorPos, apprt::ApprtError> {
            unimplemented!("S151-F1 KAT: not driven")
        }
        fn default_termio_env(&self) -> Result<rustty_global::EnvMap, apprt::ApprtError> {
            unimplemented!("S151-F1 KAT: not driven")
        }
    }

    #[test]
    // Same Miri FFI limit as `renderer_config_tests` above (`DerivedConfig::
    // init` compiles the default `Config`'s link regex through the
    // non-thread-safe oniguruma C FFI, `extern static OnigEncodingUTF8`, which
    // Miri hard-errors on): "unsupported operation", not a soundness verdict.
    // Covered natively by `cargo test --workspace` (CI HARD step 4) and by
    // the rustty-config/rustty-oniguruma ASan G6 lanes. The raw-pointer
    // fixture logic this test exists to verify (the field-projected
    // `MaybeUninit<Surface>` writes and the post-copySelectionToClipboards
    // teardown ordering) has NO oniguruma dependency of its own — proven by
    // `clipboard_codepoint_map_fixture_is_miri_clean` below, which runs the
    // identical fixture and IS Miri-covered; see MIRI_EXCEPTIONS.md
    // ("rustty-core clipboard_codepoint_map_e2e").
    #[cfg_attr(
        miri,
        ignore = "DerivedConfig::init compiles the default config's link regex through oniguruma C FFI (extern static OnigEncodingUTF8) — un-executable under Miri; ASan + native lanes cover it; see clipboard_codepoint_map_fixture_is_miri_clean for the Miri-covered variant"
    )]
    fn clipboard_codepoint_map_reaches_copy_selection_to_clipboards() {
        run(configpkg::Config::default_config());
    }

    // Miri-covered twin of the test above: identical fixture and assertions,
    // but built on a `Config` with `link.links` cleared so `DerivedConfig::
    // init` never reaches oniguruma — the ONLY difference from the real
    // `Surface::init` path, and irrelevant to what this test verifies (the
    // codepoint-map plumbing and the raw-pointer fixture's Stacked-Borrows
    // soundness, neither of which touches links). This is what actually
    // exercises the fixture's `unsafe` under Miri, per S151-F1 review.
    #[test]
    fn clipboard_codepoint_map_fixture_is_miri_clean() {
        let mut config = configpkg::Config::default_config();
        config.link.links.clear();
        run(config);
    }

    fn run(mut full_config: configpkg::Config) {
        // `clipboard-codepoint-map` set through its real CLI parser exactly
        // the way the user's config file would (map 'a', 0x61, to 'Z', 0x5A).
        full_config
            .clipboard_codepoint_map
            .parse_cli(Some(b"U+0061=U+005A"))
            .unwrap();

        // The Surface-level `DerivedConfig` clones `clipboard_codepoint_map`
        // out of the real `Config` exactly as `Surface::init` does.
        let derived_config =
            DerivedConfig::init(Allocator, &full_config).expect("DerivedConfig::init");

        // A terminal with "ab" printed to it (same fixture shape as
        // `surface_visibility_kat.rs`'s `TestSurface::build`).
        let mut terminal = rustty_vt::terminal::Terminal::init(
            Allocator,
            rustty_vt::terminal::Options {
                cols: 80,
                rows: 24,
                max_scrollback_bytes: Some(1000),
                max_scrollback_lines: None,
                colors: rustty_vt::terminal::Colors::default(),
                default_modes: rustty_vt::modes::ModePacked::default(),
                default_cursor_style: rustty_vt::screen::CursorStyle::Block,
                default_cursor_blink: Some(false),
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: rustty_vt::kitty::graphics_image::Limits::DIRECT,
            },
        )
        .expect("Terminal::init");
        {
            let mut s = terminal.vt_stream();
            s.next_slice(b"ab");
        }

        let mut mu: Box<MaybeUninit<Surface<TestApp>>> = Box::new(MaybeUninit::uninit());
        let sp = mu.as_mut_ptr();

        let mut rt_app = Box::new(TestApp {
            config: full_config,
        });
        let rt_surface = Box::new(TestRtSurface {
            core: sp,
            app: &raw mut *rt_app,
            captured: Mutex::new(Vec::new()),
        });

        // Field-projected raw-pointer writes: only the three fields
        // `copySelectionToClipboards` reads (`io.terminal`, `config`,
        // `rt_surface`) — see the module doc above.
        unsafe {
            std::ptr::addr_of_mut!((*sp).io.terminal).write(terminal);
            std::ptr::addr_of_mut!((*sp).config).write(derived_config);
            std::ptr::addr_of_mut!((*sp).rt_surface).write(Box::into_raw(rt_surface));
        }

        let surface = unsafe { &mut *sp };
        let pages = unsafe { &(*surface.io.terminal.screens.active).pages };
        let sel = terminal::selection::Selection::init(
            pages.get_top_left(terminal::point::Tag::Screen),
            pages
                .get_bottom_right(terminal::point::Tag::Screen)
                .unwrap(),
            false,
        );

        surface
            .copySelectionToClipboards(
                sel,
                &[apprt::Clipboard::Standard],
                binding::CopyToClipboard::Plain,
            )
            .expect("copySelectionToClipboards");

        // Capture the `rt_surface` pointer into a LOCAL now, before any
        // further write through `sp` — under Stacked Borrows, a write
        // through the parent raw pointer `sp` (the `io.terminal.deinit` /
        // `(*sp).config` writes in teardown below) pops the tag `surface`
        // (`&mut *sp`) was derived with, so reading `surface.rt_surface`
        // AFTER such a write is a use of an invalidated reference. Reading
        // the field now, while `surface`'s tag is still live, and using only
        // this raw-pointer local afterward avoids that.
        let rt_surface_ptr: *mut TestRtSurface = surface.rt_surface;

        let rt_surface_ref: &TestRtSurface = unsafe { &*rt_surface_ptr };
        let captured = rt_surface_ref.captured.lock().unwrap();
        assert_eq!(
            1,
            captured.len(),
            "expected exactly one text/plain clipboard write"
        );
        assert_eq!(b"text/plain".as_slice(), captured[0].0.as_slice());
        let text = String::from_utf8_lossy(&captured[0].1);
        assert_eq!(
            text.trim_end_matches('\0').trim_end(),
            "Zb",
            "clipboard-codepoint-map did not reach copySelectionToClipboards's output \
             (S151-F1 regression: the config option is silently inert)"
        );
        // MUST drop before freeing `rt_surface` below — it borrows through
        // `rt_surface_ref`, which points into the boxed `TestRtSurface` we
        // are about to `Box::from_raw` + drop.
        drop(captured);

        // Teardown: free everything we allocated above via raw-pointer
        // writes. `io.terminal` uses the manual `deinit` convention, not
        // `Drop` (matches `surface_visibility_kat.rs`'s `TestSurface::drop`).
        // `config` is a real `DerivedConfig`: its `.deinit()` frees ONLY the
        // link regexes' oniguruma C handles (`Regex` has no `Drop` of its
        // own — see `DerivedConfig::deinit` above, "the owned fields free on
        // drop"); the REST of its fields (`keybind`, `selection_word_chars`,
        // `title`, `font`, ...) are plain owned `Vec`/`String`/`Option` data
        // that only free through a normal Rust `Drop`, which `MaybeUninit`
        // never runs for an untouched-by-Drop field. So `.deinit()` alone
        // leaks them (caught by `cargo miri test ... --
        // clipboard_codepoint_map_fixture_is_miri_clean`, which reports a
        // leaked `Vec<u32>` and two leaked `Vec<u8>`s originating exactly at
        // `DerivedConfig::init`'s `keybind`/`selection_word_chars` clones);
        // `drop_in_place` after `.deinit()` runs that Drop safely (the FFI
        // `Regex.value` raw pointer was already freed and has no Drop impl
        // of its own, so this can't double-free it). Finally the raw-ptr
        // `rt_surface` Box and `rt_app`. `mu` itself never drops
        // (`MaybeUninit`; every field besides the three written above was
        // never initialized). All writes/reads below go through `sp` (the
        // raw pointer), never through `surface`, for the Stacked-Borrows
        // reason noted above.
        unsafe {
            (*sp).io.terminal.deinit(Allocator);
            (*sp).config.deinit();
            std::ptr::drop_in_place(std::ptr::addr_of_mut!((*sp).config));
            drop(Box::from_raw(rt_surface_ptr));
        }
        drop(rt_app);
        let _ = &mu;
    }
}

// ===========================================================================
// task 14d-1 (S41-F1, S42-F2): the restored `Surface.zig` diagnostics.
//
// `Surface::init` is untestable here (it spawns a live pty/renderer/io
// thread — see `renderer_config_tests`' module doc), so these follow this
// file's established fabrication recipe: a heap-pinned `Surface<TestApp>` in
// `MaybeUninit` with ONLY the fields the method under test reads written.
// Every method driven below returns before touching any other field, which
// is exactly why these four sites were chosen: they need `config` (and, for
// the key-table one, the `Default` `keyboard`) and nothing else.
//
// Four tests for 100 restored sites, chosen for shapes that break
// INDEPENDENTLY: a policy-gate `info` that must NOT fire on the allowed
// path, a second `info` in the SAME function reached only after the gate
// passes, a `debug` on an early return in a different function, and a
// `{s}`-interpolated `debug` (the `[]const u8` -> `String::from_utf8_lossy`
// conversion, which no other test here would catch). Assertions are
// full-vector `assert_eq!` on the captured records.
// ===========================================================================
#[cfg(test)]
mod diagnostics_tests {
    use super::*;
    use crate::app::test_runtime::TestApp;
    use crate::log_capture;
    use std::mem::MaybeUninit;

    /// A `Surface<TestApp>` with only `config` (and `keyboard`) live.
    ///
    /// Returns the `MaybeUninit` box so the caller keeps it alive; the
    /// `DerivedConfig` is deinit-ed + dropped in `teardown` below, matching
    /// `clipboard_codepoint_map_e2e`'s teardown convention (`.deinit()` frees
    /// the oniguruma handles, `drop_in_place` runs the ordinary `Drop` for
    /// the owned `Vec`/`String` fields that `MaybeUninit` would otherwise
    /// leak).
    fn fabricate(
        mutate: impl FnOnce(&mut configpkg::Config),
    ) -> Box<MaybeUninit<Surface<TestApp>>> {
        let mut full_config = configpkg::Config::default_config();
        // No links => `DerivedConfig::init` never reaches oniguruma, so these
        // tests are Miri-clean (the `clipboard_codepoint_map_fixture_is_miri_
        // clean` precedent).
        full_config.link.links.clear();
        mutate(&mut full_config);
        let derived = DerivedConfig::init(Allocator, &full_config).expect("DerivedConfig::init");

        let mut mu: Box<MaybeUninit<Surface<TestApp>>> = Box::new(MaybeUninit::uninit());
        let sp = mu.as_mut_ptr();
        unsafe {
            std::ptr::addr_of_mut!((*sp).config).write(derived);
            std::ptr::addr_of_mut!((*sp).keyboard).write(Keyboard::default());
        }
        mu
    }

    fn teardown(mut mu: Box<MaybeUninit<Surface<TestApp>>>) {
        let sp = mu.as_mut_ptr();
        unsafe {
            (*sp).config.deinit();
            std::ptr::drop_in_place(std::ptr::addr_of_mut!((*sp).config));
            std::ptr::drop_in_place(std::ptr::addr_of_mut!((*sp).keyboard));
        }
    }

    /// `Surface.zig:2194` — the `clipboard-write = deny` gate. Both arms are
    /// asserted: the denied one emits exactly the reference's line, and the
    /// ALLOWED one does not emit it at all. A diagnostic hoisted above the
    /// `if` would fail the second half — the success-path bug class.
    #[test]
    fn clipboard_write_deny_logs_once_and_the_allowed_path_does_not() {
        let mut mu = fabricate(|c| {
            c.clipboard_write = configpkg::value_types::ClipboardAccess::Deny;
        });
        let cap = log_capture::start();
        unsafe { &mut *mu.as_mut_ptr() }
            .clipboardWrite(b"aGk=", apprt::Clipboard::Standard)
            .expect("clipboardWrite");
        let denied = cap.messages();
        drop(cap);
        teardown(mu);

        assert_eq!(
            vec![(
                log::Level::Info,
                "surface".to_string(),
                "application attempted to write clipboard, but 'clipboard-write' is set to deny"
                    .to_string()
            )],
            denied
        );

        // The inverse arm — policy allows — is asserted by the next test:
        // with `clipboard_write = Ask` this same function runs PAST this
        // gate and the deny line is absent from its captured vector. It is
        // split out rather than done here because the allowed path with a
        // VALID payload continues into `set_clipboard`, which needs a live
        // `rt_surface` this fixture deliberately does not fabricate.
    }

    /// `Surface.zig:2206` — the invalid-base64 line, in the SAME function,
    /// reached only when the deny gate above did NOT fire. Proves the two
    /// sites are independent and that the deny line is genuinely gated.
    #[test]
    fn clipboard_write_invalid_base64_logs_and_the_deny_line_does_not() {
        let mut mu = fabricate(|c| {
            c.clipboard_write = configpkg::value_types::ClipboardAccess::Ask;
        });
        let cap = log_capture::start();
        unsafe { &mut *mu.as_mut_ptr() }
            .clipboardWrite(b"!!!not base64!!!", apprt::Clipboard::Standard)
            .expect("clipboardWrite");
        let messages = cap.messages();
        drop(cap);
        teardown(mu);

        assert_eq!(
            vec![(
                log::Level::Info,
                "surface".to_string(),
                "application sent invalid base64 data for OSC 52".to_string()
            )],
            messages
        );
    }

    /// `Surface.zig:980` — `handleMessage(.set_title)` short-circuits when a
    /// static title is configured. Different function, different level
    /// (`debug`), and an early return: a regression that dropped the
    /// early-return branch would still typecheck.
    #[test]
    fn set_title_with_a_static_config_title_logs_and_returns() {
        let mut mu = fabricate(|c| {
            c.title = Some(b"static".to_vec());
        });
        let cap = log_capture::start();
        unsafe { &mut *mu.as_mut_ptr() }
            .handleMessage(Message::SetTitle({
                let mut t = [0u8; 256];
                t[0] = b'x';
                t
            }))
            .expect("handleMessage");
        let messages = cap.messages();
        drop(cap);
        teardown(mu);

        assert_eq!(
            vec![(
                log::Level::Debug,
                "surface".to_string(),
                "ignoring title change request since static title is set via config".to_string()
            )],
            messages
        );
    }

    /// `Surface.zig:5521` — `key table not found: {s}`. The ONLY test here
    /// with an interpolated argument, and deliberately so: `{s}` on a
    /// `[]const u8` becomes `String::from_utf8_lossy`, a conversion the
    /// compiler cannot check and no other assertion in this module would
    /// catch. A missing table is the reference's own miss path.
    #[test]
    fn activate_key_table_logs_the_missing_table_name() {
        // The default config ships no key tables, so "nav" is a genuine
        // miss without touching the map at all.
        let mut mu = fabricate(|_c| {});
        let cap = log_capture::start();
        let performed = unsafe { &mut *mu.as_mut_ptr() }
            .performBindingAction(binding::Action::ActivateKeyTable(b"nav".to_vec()))
            .expect("performBindingAction");
        let messages = cap.messages();
        drop(cap);
        teardown(mu);

        assert!(!performed);
        assert_eq!(
            vec![(
                log::Level::Debug,
                "surface".to_string(),
                "key table not found: nav".to_string()
            )],
            messages
        );
    }
}

// Parity S42-F1: `completeClipboardPasteEvent` queues its Kitty paste event
// with `.locked` while it still holds the renderer mutex (`Surface.zig`'s
// `defer ... unlock` runs after `queueIo`). The case that shows it is a FULL
// IO mailbox: `Mailbox.send` then unlocks the renderer mutex so the writer
// thread can drain, blocks, and relocks it. Fabricated with the
// `MaybeUninit<Surface>` recipe (only the fields this path reads are written).
#[cfg(test)]
mod paste_event_mutex_tests {
    use super::*;
    use crate::app::test_runtime::TestApp;
    use std::mem::MaybeUninit;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    #[test]
    #[cfg_attr(miri, ignore = "spawns a thread and waits on real time")]
    fn paste_event_queues_while_holding_the_renderer_mutex_on_a_full_mailbox() {
        let mut terminal = rustty_vt::terminal::Terminal::init(
            Allocator,
            rustty_vt::terminal::Options {
                cols: 80,
                rows: 24,
                max_scrollback_bytes: Some(1000),
                max_scrollback_lines: None,
                colors: rustty_vt::terminal::Colors::default(),
                default_modes: rustty_vt::modes::ModePacked::default(),
                default_cursor_style: rustty_vt::screen::CursorStyle::Block,
                default_cursor_blink: Some(false),
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: rustty_vt::kitty::graphics_image::Limits::DIRECT,
            },
        )
        .expect("Terminal::init");
        terminal
            .modes
            .set(rustty_vt::modes::Mode::KittyPasteEvents, true);

        let mut mailbox = termio::mailbox::Mailbox::initSPSC().expect("mailbox");
        let queue: *const termio::mailbox::Queue = match &mut mailbox {
            termio::mailbox::Mailbox::Spsc { queue, .. } => {
                // Fill the queue so the paste event takes the slow path.
                while queue
                    .push_or_return(
                        termio::Message::WriteAlloc(Alloc {
                            data: b"filler".to_vec().into_boxed_slice(),
                        }),
                        rustty_datastruct::blocking_queue::Timeout::Instant,
                    )
                    .is_ok()
                {}
                &**queue
            }
        };

        // The writer's wakeup (a `Copy` handle): `Mailbox.send` notifies it on
        // the slow path right before it releases the renderer mutex and blocks.
        let wakeup: rustty_xev::Async = match &mailbox {
            termio::mailbox::Mailbox::Spsc { wakeup, .. } => *wakeup,
        };

        let renderer_mutex = Box::into_raw(Box::new(rustty_zigstd::thread::Mutex::new()));

        let mut mu: Box<MaybeUninit<Surface<TestApp>>> = Box::new(MaybeUninit::uninit());
        let sp = mu.as_mut_ptr();
        unsafe {
            std::ptr::addr_of_mut!((*sp).readonly).write(false);
            std::ptr::addr_of_mut!((*sp).renderer_state.mutex).write(renderer_mutex);
            std::ptr::addr_of_mut!((*sp).io.renderer_state)
                .write(std::ptr::addr_of_mut!((*sp).renderer_state));
            std::ptr::addr_of_mut!((*sp).io.terminal).write(terminal);
            std::ptr::addr_of_mut!((*sp).io.terminal_stream.handler.kitty_clipboard_grants)
                .write(rustty_vt::kitty::clipboard::Grants::default());
            std::ptr::addr_of_mut!((*sp).io.mailbox).write(mailbox);
        }

        // The writer-thread stand-in: once the sender is parked on the full
        // queue it must be able to take the renderer mutex (the mailbox
        // released it), then it drains until the paste event arrives.
        //
        // It waits for the mailbox's wakeup, as the real writer thread does,
        // so it never tests the mutex before the sender has locked it; then
        // it polls `try_lock` against a deadline (no fixed sleep, so a slow
        // host cannot make it look too early; review-T-H item 3). The queue
        // stays full until this thread pops, so the sender cannot finish
        // before this thread has looked.
        //
        // Every wait is bounded (review-T-H2 blocking 2): a 10 s timer is armed
        // on the same loop as the wakeup wait, so `run(Once)` returns even if
        // the sender parks without notifying. On that timeout the drainer
        // skips the mutex poll but still drains, which releases the parked
        // sender; nothing is asserted on this thread (a panic here would leave
        // the sender parked forever). The main thread asserts `woke` after
        // `join`.
        struct SendPtr<T>(*const T);
        unsafe impl<T> Send for SendPtr<T> {}
        let q = SendPtr(queue);
        let m = SendPtr(renderer_mutex as *const rustty_zigstd::thread::Mutex);
        let done = Arc::new(AtomicBool::new(false));
        let done2 = done.clone();
        let drainer = std::thread::spawn(move || {
            let (q, m) = (q, m);
            let mut loop_ = rustty_xev::Loop::init(rustty_xev::Options::default()).unwrap();
            let mut woke = false;
            let mut timed_out = false;
            let mut c_wait = rustty_xev::Completion::default();
            let mut c_timer = rustty_xev::Completion::default();
            let cb = |ud: *mut bool,
                      _l: *mut rustty_xev::Loop,
                      _c: *mut rustty_xev::Completion,
                      r: core::result::Result<(), rustty_xev::WaitError>| {
                if r.is_ok() {
                    unsafe { *ud = true };
                }
                rustty_xev::CallbackAction::Disarm
            };
            let timer_cb =
                |ud: *mut bool,
                 _l: *mut rustty_xev::Loop,
                 _c: *mut rustty_xev::Completion,
                 _r: core::result::Result<(), rustty_xev::TimerRunError>| {
                    unsafe { *ud = true };
                    rustty_xev::CallbackAction::Disarm
                };
            // SAFETY: `loop_`, `c_wait`, `c_timer`, `woke` and `timed_out`
            // outlive both completions: `run(Once)` returns after the first
            // of them fires, and the loop is deinitialized before any of them
            // goes out of scope, so the other never fires later.
            let timer = rustty_xev::Timer::init().unwrap();
            unsafe {
                wakeup.wait(
                    &mut loop_ as *mut rustty_xev::Loop,
                    std::ptr::addr_of_mut!(c_wait),
                    std::ptr::addr_of_mut!(woke),
                    cb,
                );
                timer.run(
                    &mut loop_ as *mut rustty_xev::Loop,
                    std::ptr::addr_of_mut!(c_timer),
                    10_000,
                    std::ptr::addr_of_mut!(timed_out),
                    timer_cb,
                );
            }
            let ran = loop_.run(rustty_xev::RunMode::Once);
            loop_.deinit();
            let woke = woke && ran.is_ok();

            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            let mut took_mutex_while_parked = false;
            while woke && std::time::Instant::now() < deadline {
                if unsafe { (*m.0).try_lock() } {
                    unsafe { (*m.0).unlock() };
                    took_mutex_while_parked = true;
                    break;
                }
                std::thread::yield_now();
            }
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            let mut event: Option<Vec<u8>> = None;
            while event.is_none() && std::time::Instant::now() < deadline {
                while let Some(msg) = unsafe { (*q.0).pop() } {
                    if let termio::Message::WriteAlloc(a) = msg {
                        if &*a.data != b"filler" {
                            event = Some(a.data.to_vec());
                        }
                    }
                }
                if done2.load(Ordering::SeqCst) {
                    break;
                }
                std::thread::yield_now();
            }
            (woke, timed_out, took_mutex_while_parked, event)
        });

        let surface = unsafe { &mut *sp };
        let result = surface
            .completeClipboardPasteEvent(apprt::Clipboard::Standard, &[b"text/plain".as_slice()]);
        done.store(true, Ordering::SeqCst);
        // If the sender never took the slow path, the drainer is still
        // waiting for the wakeup; release it so the test fails instead of
        // hanging.
        let _ = wakeup.notify();
        let (woke, timed_out, took_mutex_while_parked, event) = drainer.join().expect("drainer");
        let unlocked_after = unsafe { (*renderer_mutex).try_lock() };

        // Teardown.
        unsafe {
            (*sp).io.terminal.deinit(Allocator);
            std::ptr::drop_in_place(std::ptr::addr_of_mut!(
                (*sp).io.terminal_stream.handler.kitty_clipboard_grants
            ));
            std::ptr::read(std::ptr::addr_of!((*sp).io.mailbox)).deinit();
            drop(Box::from_raw(renderer_mutex));
        }

        assert!(
            woke,
            "the mailbox never signalled its wakeup (timed out after 10 s: {timed_out})"
        );
        assert_eq!(result.ok(), Some(true), "the paste event was not produced");
        let event = event.expect("the paste event never reached the IO mailbox");
        assert!(
            event.starts_with(b"\x1b]5522;"),
            "unexpected paste event bytes: {:?}",
            String::from_utf8_lossy(&event)
        );
        assert!(
            took_mutex_while_parked,
            "the renderer mutex was not free while the sender waited on the full queue"
        );
        assert!(
            unlocked_after,
            "completeClipboardPasteEvent returned with the renderer mutex still locked"
        );
    }
}
