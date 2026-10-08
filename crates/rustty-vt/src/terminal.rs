//! The primary terminal emulation structure. This represents a single
//! "terminal" containing a grid of characters and exposes various operations
//! on that grid. This also maintains the scrollback buffer.
//!
//! rustty-vt terminal
//!
//! PORTS: ghostty src/terminal/Terminal.zig
//!
//! PORT STATUS: done (vt-12b) — struct, method bodies, and tests are ported and
//! filled; no `todo!()` bodies remain (the only rustty-vt seam left is the
//! dcimgui-gated `imguiKey` in `input/key.rs`).

// PORT NOTE: the method bodies are filled (vt-12b). The crate-wide allows below
// are retained from the skeleton phase where the filled file still trips them
// (`dead_code` on fields not yet read by every downstream tier, adapted
// signatures); `clippy::todo` is now inert (no `todo!()` bodies remain).
#![allow(clippy::todo)]
#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(clippy::new_without_default)]
// PORT NOTE: the reference exposes `Colors.default` as a named constant; the
// Rust port mirrors it as a `Colors::default()` inherent constructor. This is
// intentional parity, not the std `Default` trait.
#![allow(clippy::should_implement_trait)]

// const std = @import("std");
// const build_options = @import("terminal_options");
// const lib = @import("lib.zig");
// const assert = @import("../quirks.zig").inlineAssert;
// const testing = std.testing;
// const Allocator = std.mem.Allocator;
// const unicode = @import("../unicode/main.zig");
// const uucode = @import("uucode");
//
// const ansi = @import("ansi.zig");
// const modespkg = @import("modes.zig");
// const charsets = @import("charsets.zig");
// const csi = @import("csi.zig");
// const hyperlink = @import("hyperlink.zig");
// const glyph = @import("apc/glyph.zig");
// const kitty = @import("kitty.zig");
// const osc = @import("osc.zig");
// const point = @import("point.zig");
// const sgr = @import("sgr.zig");
// const Tabstops = @import("Tabstops.zig");
// const color = @import("color.zig");
// const mouse = @import("mouse.zig");
// const Stream = @import("stream_terminal.zig").Stream;
//
// const size = @import("size.zig");
// const pagepkg = @import("page.zig");
// const style = @import("style.zig");
// const Screen = @import("Screen.zig");
// const ScreenSet = @import("ScreenSet.zig");
// const Page = pagepkg.Page;
// const Cell = pagepkg.Cell;
// const Row = pagepkg.Row;
//
// PORT NOTE: the reference `Allocator.Error` maps to `pagelist::OutOfMemory`
// throughout this crate (matching Screen/ScreenSet); fallible (`!`) returns
// become `Result<_, OutOfMemory>`.
use crate::ansi;
// const glyph = @import("apc/glyph.zig");
use crate::apc::glyph;
use crate::charsets;
use crate::csi;
use crate::kitty;
use crate::modes as modespkg;
use crate::mouse;
use crate::osc;
use crate::page as pagepkg;
use crate::page::u21;
use crate::page::{Page, Row};
use crate::pagelist;
use crate::pagelist::OutOfMemory;
use crate::point;
use crate::screen::Screen;
use crate::screen_set::ScreenSet;
use crate::sgr;
use crate::size;
use crate::stream_terminal::{Handler, Stream};
use crate::style;
use crate::tabstops::Tabstops;
use color::{DynamicPalette, DynamicRGB};

// PORT NOTE: `terminal/color.zig` is `crate::color`.
use crate::color;

// const unicode = @import("../unicode/main.zig");
// PORT NOTE: the width/grapheme-break seam (`unicode.table.get`,
// `unicode.graphemeBreak`) maps to the `rustty_unicode` crate, matching Screen.
use rustty_unicode as unicode;

use crate::alloc::Allocator;

mod attrs;
mod cursor;
mod edit;
mod print;
mod resize;
mod scroll;
mod state;

// CORRECTION (parity tail S7-7): a process-static `GlobalZigAllocator`
// (`GLOBAL_ZIG_ALLOCATOR`) stood in for `self.gpa()` in `vtStream` because
// `Stream.initAlloc` wanted a `ZigAllocatorRef<'static>`. The stream takes the
// runtime allocator handle now, and `vt_stream` passes `gpa()` itself.

// const log = std.log.scoped(.terminal);
const LOG_SCOPE: &str = "terminal";

/// Default tabstop interval
const TABSTOP_INTERVAL: u16 = 8;

// PORT NOTE (Stage A.3 — apc complete): the apc cluster is unblocked (rustty-font
// landed); `apc/glyph`'s `Glossary` / `Request` / `Response` are the real ported
// types, no longer ZST stubs. The previous `GlyphGlossary` / `GlyphRequest` /
// `GlyphResponse` stand-ins are dropped in favour of the real `glyph::Glossary`
// (`glyph_glossary` field), `glyph::Request` (`glyph_protocol` arg) and
// `glyph::Response` (`glyph_protocol` return).

/// The most recently set mouse shift capture state.
///
/// Set via the XTSHIFTESCAPE sequence. If true (XTSHIFTESCAPE = 1)
/// then we want to capture the shift key for the mouse protocol
/// if the configuration allows it.
// enum(u2) { null, false, true }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MouseShiftCapture {
    #[default]
    Null,
    False,
    True,
}

/// These are just a packed set of flags we may set on the terminal.
// flags: packed struct { ... } = .{}
//
// PORT NOTE (S143-F1): `PartialEq`/`Eq` so the resize failure-path test can do
// the reference's `try testing.expectEqual(before.flags, t.flags)` — whole
// struct, every field — instead of picking out `dirty.clear`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Flags {
    // This supports a Kitty extension where programs using semantic
    // prompts (OSC133) can annotate their new prompts with `redraw=0` to
    // disable clearing the prompt on resize.
    pub shell_redraws_prompt: osc::semantic_prompt::Redraw,

    // This is set via ESC[4;2m. Any other modify key mode just sets
    // this to false and we act in mode 1 by default.
    pub modify_other_keys_2: bool,

    /// The mouse event mode and format. These are set to the last
    /// set mode in modes. You can't get the right event/format to use
    /// based on modes alone because modes don't show you what order
    /// this was called so we have to track it separately.
    pub mouse_event: mouse::Event,
    pub mouse_format: mouse::Format,

    /// Set via the XTSHIFTESCAPE sequence. If true (XTSHIFTESCAPE = 1)
    /// then we want to capture the shift key for the mouse protocol
    /// if the configuration allows it.
    pub mouse_shift_capture: MouseShiftCapture,

    /// True if the window is focused.
    pub focused: bool,

    /// True if the terminal view may be visible. Unknown visibility is
    /// represented as visible so callers behave conservatively.
    // visible: bool = true,
    pub visible: bool,

    /// Whether a resize may pull rows out of scrollback back into the
    /// active area. This should be false if the pty keeps its own screen
    /// buffer without scrollback (e.g. Windows ConPTY) so that we stay in
    /// sync with it. See PageList.Resize for details. This is configuration
    /// rather than terminal state so it is preserved across a full reset.
    // resize_pull_scrollback: bool = true,
    pub resize_pull_scrollback: bool,

    /// True if the terminal is in a password entry mode. This is set
    /// to true based on termios state. This is set
    /// to true based on termios state.
    pub password_input: bool,

    /// True if the terminal should perform selection scrolling.
    pub selection_scroll: bool,

    /// Dirty flag used only by the search thread. The renderer is expected
    /// to set this to true if the viewport was dirty as it was rendering.
    /// This is used by the search thread to more efficiently re-search the
    /// viewport and active area.
    ///
    /// Since the renderer is going to inspect the viewport/active area ANYWAYS,
    /// this lets our search thread do less work and hold the lock less time,
    /// resulting in more throughput for everything.
    pub search_viewport_dirty: bool,

    /// Dirty flags for the renderer.
    pub dirty: Dirty,
}

impl Default for Flags {
    fn default() -> Flags {
        Flags {
            shell_redraws_prompt: osc::semantic_prompt::Redraw::True,
            modify_other_keys_2: false,
            mouse_event: mouse::Event::None,
            mouse_format: mouse::Format::X10,
            mouse_shift_capture: MouseShiftCapture::Null,
            focused: true,
            visible: true,
            resize_pull_scrollback: true,
            password_input: false,
            selection_scroll: false,
            search_viewport_dirty: false,
            dirty: Dirty::default(),
        }
    }
}

/// The various color configurations a terminal maintains and that can
/// be set dynamically via OSC, with defaults usually coming from a
/// configuration.
// PORT NOTE: Clone/Debug/PartialEq are derived for the snapshot TERMINAL
// codec's round-trip test (Zig: `std.testing.expectEqualDeep(colors, ...)`).
#[derive(Clone, Debug, PartialEq)]
pub struct Colors {
    pub background: DynamicRGB,
    pub foreground: DynamicRGB,
    pub cursor: DynamicRGB,
    pub palette: DynamicPalette,
}

impl Colors {
    // pub const default: Colors = .{
    //     .background = .unset,
    //     .foreground = .unset,
    //     .cursor = .unset,
    //     .palette = .default,
    // };
    pub fn default() -> Colors {
        Colors {
            background: DynamicRGB::UNSET,
            foreground: DynamicRGB::UNSET,
            cursor: DynamicRGB::UNSET,
            palette: DynamicPalette::default_palette(),
        }
    }
}

/// This is a set of dirty flags the renderer can use to determine
/// what parts of the screen need to be redrawn. It is up to the renderer
/// to clear these flags.
///
/// This only contains dirty flags for terminal state, not for the screen
/// state. The screen state has its own dirty flags.
// pub const Dirty = packed struct { ... }
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Dirty {
    /// Set when the color palette is modified in any way.
    pub palette: bool,

    /// Set when the reverse colors mode is modified.
    pub reverse_colors: bool,

    /// Screen clear of some kind. This can be due to a screen change,
    /// erase display, etc.
    pub clear: bool,

    /// Set when the pre-edit is modified.
    pub preedit: bool,

    /// Set when Glyph Protocol registrations may have changed. Registered
    /// glyphs can affect already-visible PUA cells, so this requires a full
    /// render-state rebuild.
    pub glyph_glossary: bool,
}

/// Scrolling region is the area of the screen designated where scrolling
/// occurs. When scrolling the screen, only this viewport is scrolled.
// PORT NOTE: derives — see Colors (snapshot TERMINAL round-trip equality).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScrollingRegion {
    // Top and bottom of the scroll region (0-indexed)
    // Precondition: top < bottom
    pub top: size::CellCountInt,
    pub bottom: size::CellCountInt,

    // Left/right scroll regions.
    // Precondition: right > left
    // Precondition: right <= cols - 1
    pub left: size::CellCountInt,
    pub right: size::CellCountInt,
}

pub struct Options {
    pub cols: size::CellCountInt,
    pub rows: size::CellCountInt,
    /// The maximum size of scrollback in bytes. Null is unlimited and zero
    /// disables scrollback.
    pub max_scrollback_bytes: Option<usize>,

    /// The maximum number of physical scrollback rows, excluding the active
    /// area. Null is unlimited. The effective limit permits at least one
    /// standard page and only complete historical pages are pruned.
    pub max_scrollback_lines: Option<usize>,

    pub colors: Colors,

    /// The default mode state. When the terminal gets a reset, it
    /// will revert back to this state.
    pub default_modes: modespkg::ModePacked,

    /// Cursor state restored by DECSCUSR default and RIS.
    // default_cursor_style: Screen.CursorStyle = .block,
    // default_cursor_blink: ?bool = false,
    pub default_cursor_style: crate::screen::CursorStyle,
    pub default_cursor_blink: Option<bool>,

    /// The total storage limit for Kitty images in bytes. Has no effect
    /// if kitty images are disabled at build-time.
    // kitty_image_storage_limit: usize = switch (build_options.artifact) {
    //     .ghostty => 320 * 1000 * 1000, // 320MB
    //
    //     // librustty we start with a much lower limit since this is an
    //     // embedded library and we want to be more conservative with memory
    //     // usage by default.
    //     .lib => 10 * 1000 * 1000, // 10MB
    // },
    // PORT NOTE: a Rust struct field has no default initializer; the
    // reference's artifact-selected field default is
    // `Options::kitty_image_storage_limit_default(artifact)` below.
    pub kitty_image_storage_limit: usize,

    /// The limits for what medium types are allowed for Kitty image loading.
    /// Has no effect if kitty images are disabled otherwise. For example,
    // if no `sys.decode_png` hook is specified, png formats are disabled
    // no matter what.
    // kitty_image_loading_limits: if (build_options.kitty_graphics)
    //     kitty.graphics.LoadingImage.Limits
    // else
    //     void
    pub kitty_image_loading_limits: kitty::graphics_image::Limits,
}

impl Options {
    /// The reference's field default for `kitty_image_storage_limit`, which
    /// depends on the build artifact (see the field's comment above).
    // PORT NOTE: the reference's `switch (build_options.artifact)` field
    // initializer, as a const fn over the artifact. rustty-vt is built once
    // for the whole workspace with `crate::options::ARTIFACT == .ghostty`, so
    // the `.lib` artifact (rustty-vt-c, the libghostty-vt C API, whose
    // `terminal_new` relies on this default) selects `Artifact::Lib` here.
    pub const fn kitty_image_storage_limit_default(
        artifact: crate::build_options::Artifact,
    ) -> usize {
        match artifact {
            crate::build_options::Artifact::Ghostty => 320 * 1000 * 1000, // 320MB
            crate::build_options::Artifact::Lib => 10 * 1000 * 1000,      // 10MB
        }
    }
}

/// The semantic prompt type. This is used when tracking a line type and
/// requires integration with the shell. By default, we mark a line as "none"
/// meaning we don't know what type it is.
///
/// See: https://gitlab.freedesktop.org/Per_Bothner/specifications/blob/master/proposals/semantic-prompts.md
pub enum SemanticPrompt {
    Prompt,
    PromptContinuation,
    Input,
    Command,
}

/// Options for scrolling the viewport of the terminal grid.
///
/// PORT NOTE: the reference `ScrollViewport = union(Tag)` carries a C-ABI
/// `TaggedUnion` (`C`/`CValue`/`cval`). That C-ABI surface is hoisted into
/// `rustty-vt-c` (deferred); the pure-VT type here is a plain Rust enum.
pub enum ScrollViewport {
    /// Scroll to the top of the scrollback
    Top,

    /// Scroll to the bottom, i.e. the top of the active area
    Bottom,

    /// Scroll by some delta amount, up is negative.
    Delta(isize),

    /// Scroll to the given absolute row offset from the top of the
    /// scrollable area. A value of zero is the top row. The requested
    /// row becomes the first visible row of the viewport, clamped so
    /// the viewport never scrolls beyond the top of the active area.
    /// This is the same row space as `Scrollbar` offset.
    Row(usize),
}

/// The amount of compression work performed by `compress` before returning.
///
/// The declaration order is part of the librustty-vt C ABI. Removed values
/// must leave a `null` hole so later values retain their integer values.
// PORT NOTE: the reference declares this inside `Terminal` as
// `CompressionMode = lib.Enum(lib.target, &.{ "incremental", "full" })`. Rust
// has no namespaced type declarations inside an `impl`, so it is hoisted to the
// module and prefixed like the other hoisted `Terminal` types. For the `.zig`
// target `lib.Enum(...)` is a plain enum with the keys in order; the C-ABI
// surface lives in `rustty-vt-c`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalCompressionMode {
    Incremental = 0,
    Full = 1,
}

/// The scheduling result of a `compress` call.
///
/// The declaration order is part of the librustty-vt C ABI. Removed values
/// must leave a `null` hole so later values retain their integer values.
// PORT NOTE: hoisted out of `Terminal` and prefixed; see
// `TerminalCompressionMode`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalCompressionResult {
    Unsupported = 0,
    Pending = 1,
    Complete = 2,
}

/// The modes for DECCOLM.
// pub const DeccolmMode = enum(u1) { @"80_cols" = 0, @"132_cols" = 1 };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeccolmMode {
    Cols80 = 0,
    Cols132 = 1,
}

/// Terminal-level cursor state shared by all screens.
// pub const Cursor = struct {
//     is_default: bool = true,
//     default_style: Screen.CursorStyle = .block,
//     default_blink: ?bool = false,
// };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    /// Whether the current cursor appearance follows the configured defaults.
    pub is_default: bool,

    /// Configured style restored by DECSCUSR default and RIS.
    pub default_style: crate::screen::CursorStyle,

    /// Configured blink restored by DECSCUSR default and RIS. Null selects
    /// the terminal emulator default, which is blinking.
    pub default_blink: Option<bool>,
}

impl Default for Cursor {
    fn default() -> Cursor {
        Cursor {
            is_default: true,
            default_style: crate::screen::CursorStyle::Block,
            default_blink: Some(false),
        }
    }
}

/// A terminal resize expressed in cells with optional per-cell pixel
/// geometry. It is highly recommended that callers supply cell geometry
/// but a terminal can technically function without it (but some reports
/// like certain mouse reporting modes and Kitty image protocol will
/// not be functional).
///
/// Cell pixel dimensions must already be scaled for the current display DPI.
// pub const Resize = struct {
//     cols: size.CellCountInt,
//     rows: size.CellCountInt,
//     cell_size_px: ?struct { width: u32, height: u32 } = null,
// };
//
// PORT NOTE: the reference `cell_size_px` is an anonymous inline struct; Rust
// has no anonymous struct types, so it is named `ResizeCellSize`. There is no
// field default in Rust, so every construction spells out `cell_size_px: None`
// where the reference relies on the `= null` default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resize {
    pub cols: size::CellCountInt,
    pub rows: size::CellCountInt,
    pub cell_size_px: Option<ResizeCellSize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResizeCellSize {
    pub width: u32,
    pub height: u32,
}

// pub const ResizeError = error{ OutOfMemory, InvalidValue };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeError {
    /// Resize requires allocation
    OutOfMemory,

    /// Input value was invalid, such as a 0-sized dimension.
    InvalidValue,
}

impl From<OutOfMemory> for ResizeError {
    fn from(_: OutOfMemory) -> ResizeError {
        ResizeError::OutOfMemory
    }
}

// PORT NOTE: the reference `printString` has an inferred error set: the
// `std.unicode.Utf8View.init` validation propagates `error.InvalidUtf8`
// while the print/linefeed paths propagate `Allocator.Error` (OutOfMemory).
// This enum names that inferred set. `Screen.testWriteString` shares it
// (same `try Utf8View.init` + allocation error surface).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrintStringError {
    /// Printing requires allocation (Zig `Allocator.Error`).
    OutOfMemory,

    /// The input was not valid UTF-8 (Zig `error.InvalidUtf8` from
    /// `std.unicode.Utf8View.init`).
    InvalidUtf8,

    /// PORT NOTE (parity S134-F2): `testWriteString`'s inferred error set
    /// includes `PageList.IncreaseCapacityError`, via
    /// `try self.cursorSetHyperlink()`. That error set is
    /// `{ OutOfMemory, OutOfSpace }`, and the port flattened `OutOfSpace` into
    /// `OutOfMemory` — the one distinguishable error the helper could lose.
    /// `cursorDownOrScroll` and the zero-width `appendGrapheme` are
    /// `Allocator.Error`-only upstream, so they lose nothing.
    OutOfSpace,
}

impl From<crate::pagelist::grow::IncreaseCapacityError> for PrintStringError {
    fn from(err: crate::pagelist::grow::IncreaseCapacityError) -> PrintStringError {
        match err {
            crate::pagelist::grow::IncreaseCapacityError::OutOfMemory => {
                PrintStringError::OutOfMemory
            }
            crate::pagelist::grow::IncreaseCapacityError::OutOfSpace => {
                PrintStringError::OutOfSpace
            }
        }
    }
}

impl From<OutOfMemory> for PrintStringError {
    fn from(_: OutOfMemory) -> PrintStringError {
        PrintStringError::OutOfMemory
    }
}

/// Modal screen changes. These map to the literal terminal
/// modes to enable or disable alternate screen modes. They each
/// have subtle behaviors so we define them as an enum here.
pub enum SwitchScreenMode {
    /// Legacy alternate screen mode. This goes to the alternate
    /// screen or primary screen and only copies the cursor. The
    /// screen is not erased.
    // @"47"
    M47,

    /// Alternate screen mode where the alternate screen is cleared
    /// on exit. The primary screen is never cleared. The cursor is
    /// copied.
    // @"1047"
    M1047,

    /// Save primary screen cursor, switch to alternate screen,
    /// and clear the alternate screen on entry. On exit,
    /// do not clear the screen, and restore the cursor on the
    /// primary screen.
    // @"1049"
    M1049,
}

/// The primary terminal emulation structure. This represents a single
/// "terminal" containing a grid of characters and exposes various operations
/// on that grid. This also maintains the scrollback buffer.
// const Terminal = @This();
pub struct Terminal {
    /// The set of screens behind this terminal (e.g. primary vs alternate).
    pub screens: ScreenSet,

    /// Whether we're currently writing to the status line (DECSASD and DECSSDT).
    /// We don't support a status line currently so we just black hole this
    /// data so that it doesn't mess up our main display.
    pub status_display: ansi::StatusDisplay,

    /// Where the tabstops are.
    pub tabstops: Tabstops,

    /// The size of the terminal.
    pub rows: size::CellCountInt,
    pub cols: size::CellCountInt,

    /// The size of the screen in pixels. This is used for pty events and images
    pub width_px: u32,
    pub height_px: u32,

    /// The current scrolling region.
    pub scrolling_region: ScrollingRegion,

    /// The last reported pwd, if any.
    // pwd: std.ArrayList(u8)
    // PORT NOTE: Zig `std.ArrayList(u8)` (unmanaged) byte payload ->
    // `alloc::ArrayListUnmanaged<u8>` with a trailing-0 sentinel (NEVER lossy
    // String), growing through `self.gpa()` per call as the reference does
    // (parity tail S7-3). Getter returns `Option<&[u8]>` (NUL-stripped).
    pub pwd: crate::alloc::ArrayListUnmanaged<u8>,

    /// The title of the terminal as set by escape sequences (e.g. OSC 0/2).
    pub title: crate::alloc::ArrayListUnmanaged<u8>,

    /// The color state for this terminal.
    pub colors: Colors,

    /// The previous printed character. This is used for the repeat previous
    /// char CSI (ESC [ <n> b).
    // previous_char: ?u21 = null
    pub previous_char: Option<u32>,

    /// The modes that this terminal currently has active.
    pub modes: modespkg::ModeState,

    /// Terminal-level cursor state.
    // cursor: Cursor = .{}
    pub cursor: Cursor,

    /// The most recently set mouse shape for the terminal.
    pub mouse_shape: mouse::Shape,

    /// Per-session Glyph Protocol registrations.
    // glyph_glossary: glyph.Glossary = .empty,
    pub glyph_glossary: glyph::Glossary,

    /// Kitty drag and drop protocol (OSC 72) state. Allocated when a client
    /// registers to accept drops (t=a) and freed when it unregisters (t=A),
    /// so a terminal that never runs a drag and drop aware program pays
    /// nothing for it. Non-null means a client currently accepts drops.
    // kitty_dnd: ?*kitty.dnd.State = null,
    // PORT NOTE (af8d28a94, parity tail S7-3): `?*State` (heap create/destroy)
    // maps to `Option<StateBox>`, a box allocated through the terminal's
    // allocator — `kitty::dnd::handle_command` allocates into and frees from
    // this slot.
    pub kitty_dnd: Option<kitty::dnd::StateBox>,

    /// The I/O implementation we should use for this terminal.
    // PORT NOTE (io-stand-in, W1 sys.rs decision): the reference threads
    // `io: std.Io` through `Terminal.init` onto `Screen.io` and exposes it via
    // `Terminal.io()`. The port's `sys::Io` is the entropy-only projection
    // (see sys.rs NON_PORTS TinyIo re-read note), so the field lives here on
    // the Terminal (rustty's Screen carries no io) and defaults to
    // `sys::io_native`; tests that construct with `std.Io.failing` set it to
    // `sys::io_failing` after init.
    pub io: crate::sys::Io,

    /// These are just a packed set of flags we may set on the terminal.
    pub flags: Flags,
}

/// PORT NOTE: `scrollUp`/`scrollDown` use a `defer` block to restore the
/// cursor x/y and pending_wrap on every exit. Rust has no `defer`; this RAII
/// guard reproduces it — its `Drop` runs the same restore logic whenever the
/// guard leaves scope (normal exit, early `return`, or error `?`).
struct CursorRestoreGuard {
    screen: *mut Screen,
    old_x: size::CellCountInt,
    old_y: size::CellCountInt,
    old_wrap: bool,
}

impl Drop for CursorRestoreGuard {
    fn drop(&mut self) {
        // self.screens.active.cursorAbsolute(old_x, old_y);
        unsafe { (*self.screen).cursor_absolute(self.old_x, self.old_y) };
        // self.screens.active.cursor.pending_wrap = old_wrap;
        unsafe { (*self.screen).cursor.pending_wrap = self.old_wrap };
    }
}

/// PORT NOTE: `index` defers its cursor semantic-prompt handling to AFTER the
/// scroll because it may need to apply to new rows. Rust has no `defer`; this
/// RAII guard's `Drop` reproduces that deferred block, reading `screen.cursor`
/// at drop time (post-scroll) and running on every exit path of `index`.
struct SemanticIndexGuard {
    screen: *mut Screen,
}

impl Drop for SemanticIndexGuard {
    fn drop(&mut self) {
        let screen = self.screen;
        // if (screen.cursor.semantic_content != .output) {
        if unsafe { (*screen).cursor.semantic_content } != pagepkg::SemanticContent::Output {
            // @branchHint(.unlikely);

            // Always reset any semantic content clear-eol state.
            //
            // The specification is not clear what "end-of-line" means. If we
            // discover that there are more scenarios we should be unsetting
            // this we should document and test it.
            // if (screen.cursor.semantic_content_clear_eol) {
            if unsafe { (*screen).cursor.semantic_content_clear_eol } {
                // screen.cursor.semantic_content = .output;
                unsafe { (*screen).cursor.semantic_content = pagepkg::SemanticContent::Output };
                // screen.cursor.semantic_content_clear_eol = false;
                unsafe { (*screen).cursor.semantic_content_clear_eol = false };
            } else {
                // If we aren't clearing our state at EOL and we're not output,
                // then we mark the new row as a prompt continuation. This is
                // to work around shells that don't send OSC 133 k=s sequences
                // for continuations.
                //
                // This can be a false positive if the shell changes content
                // type later and outputs something. We handle that in the
                // semanticPrompt function.
                // screen.cursor.page_row.semantic_prompt = .prompt_continuation;
                unsafe {
                    (*(*screen).cursor.page_row)
                        .set_semantic_prompt(pagepkg::SemanticPrompt::PromptContinuation)
                };
            }
        } else {
            // This should never be set in the output mode.
            // assert(!screen.cursor.semantic_content_clear_eol);
            debug_assert!(!unsafe { (*screen).cursor.semantic_content_clear_eol });
        }
    }
}

/// PORT NOTE: `insertLines`/`deleteLines` end with a `defer` that returns the
/// cursor to the left margin on the row it started on and clears pending_wrap.
/// This RAII guard reproduces that on every exit path.
struct ShiftLinesEndGuard {
    screen: *mut Screen,
    left: size::CellCountInt,
    start_y: size::CellCountInt,
}

impl Drop for ShiftLinesEndGuard {
    fn drop(&mut self) {
        // self.screens.active.cursorAbsolute(self.scrolling_region.left, start_y);
        unsafe { (*self.screen).cursor_absolute(self.left, self.start_y) };
        // Always unset pending wrap
        // self.screens.active.cursor.pending_wrap = false;
        unsafe { (*self.screen).cursor.pending_wrap = false };
    }
}

/// PORT NOTE: reproduces `defer self.screens.active.pages.untrackPin(cur_p)`.
/// `Drop` untracks the pin on every exit path.
struct TrackedPinGuard {
    screen: *mut Screen,
    pin: *mut crate::pagelist::Pin,
}

impl Drop for TrackedPinGuard {
    fn drop(&mut self) {
        unsafe { (*self.screen).pages.untrack_pin(self.pin) };
    }
}

/// PORT NOTE: helper for the reference slice expression `cells[start..end]`
/// over a raw `*mut [Cell]`. Reproduces the sub-slice as a raw `*mut [Cell]`
/// (no intermediate `&mut [Cell]`, preserving the unprotected-pointer aliasing
/// the reference relies on).
fn ptr_slice_range(cells: *mut [pagepkg::Cell], start: usize, end: usize) -> *mut [pagepkg::Cell] {
    let base: *mut pagepkg::Cell = cells as *mut pagepkg::Cell;
    core::ptr::slice_from_raw_parts_mut(unsafe { base.add(start) }, end - start)
}

impl Terminal {
    /// Initialize a new terminal.
    pub fn init(alloc: Allocator, opts: Options) -> Result<Terminal, OutOfMemory> {
        let cols = opts.cols;
        let rows = opts.rows;

        // var screen_set: ScreenSet = try .init(alloc, .{ ... });
        // errdefer screen_set.deinit(alloc);
        let mut screen_set: ScreenSet = ScreenSet::init(
            alloc,
            crate::screen::Options {
                cols,
                rows,
                max_scrollback_bytes: opts.max_scrollback_bytes,
                max_scrollback_lines: opts.max_scrollback_lines,
                kitty_image_storage_limit: opts.kitty_image_storage_limit,
                kitty_image_loading_limits: opts.kitty_image_loading_limits,
            },
        )?;

        // PORT NOTE: the reference `errdefer screen_set.deinit(alloc)` covers the
        // tabstops-init failure path. In Rust the `?` on `Tabstops::init` returns
        // before `screen_set` is moved into the returned struct, so we deinit it
        // explicitly on that error to mirror the errdefer (no leak).
        let tabstops = match Tabstops::init(alloc, cols as usize, TABSTOP_INTERVAL as usize) {
            Ok(t) => t,
            Err(_e) => {
                // PORT NOTE: `Tabstops`'s allocation-failure type is its own
                // `OutOfMemory`; both map to the reference `Allocator.Error`. We
                // normalize to `pagelist::OutOfMemory` (this crate's canonical
                // OOM) to match the `init` signature.
                screen_set.deinit(alloc);
                return Err(OutOfMemory);
            }
        };

        // var result: Terminal = .{ ... };
        // result.setCursorStyle(.default);
        // return result;
        let mut result = Terminal {
            cols,
            rows,
            width_px: 0,
            height_px: 0,
            screens: screen_set,
            status_display: ansi::StatusDisplay::Main,
            tabstops,
            scrolling_region: ScrollingRegion {
                top: 0,
                bottom: rows - 1,
                left: 0,
                right: cols - 1,
            },
            pwd: crate::alloc::ArrayListUnmanaged::EMPTY,
            title: crate::alloc::ArrayListUnmanaged::EMPTY,
            colors: opts.colors,
            previous_char: None,
            modes: modespkg::ModeState {
                values: opts.default_modes,
                default: opts.default_modes,
                ..modespkg::ModeState::default()
            },
            cursor: Cursor {
                default_style: opts.default_cursor_style,
                default_blink: opts.default_cursor_blink,
                ..Cursor::default()
            },
            mouse_shape: mouse::Shape::Text,
            glyph_glossary: glyph::Glossary::empty_in(alloc),
            kitty_dnd: None,
            io: crate::sys::io_native,
            flags: Flags::default(),
        };
        result.set_cursor_style(ansi::CursorStyle::Default);
        Ok(result)
    }

    pub fn deinit(&mut self, alloc: Allocator) {
        // PORT NOTE: `Tabstops::deinit` consumes `self` by value (the owning
        // allocator frees on drop); we take it out via `core::mem::take` (leaving
        // an empty default) to call deinit, matching the reference
        // `self.tabstops.deinit(alloc)`.
        core::mem::take(&mut self.tabstops).deinit(alloc);
        self.screens.deinit(alloc);
        // self.colors.palette.deinit(alloc);
        self.colors.palette.deinit(alloc);
        // self.pwd.deinit(alloc); self.title.deinit(alloc);
        self.pwd.deinit(alloc);
        self.title.deinit(alloc);
        // self.glyph_glossary.deinit(alloc);
        // PORT NOTE (parity tail S7-3): the glossary map frees through the
        // allocator it was bound to at `init` (its entries on drop); the
        // reference's `alloc` is that allocator, checked here in debug builds.
        self.glyph_glossary.allocator().debug_assert_same(alloc);
        self.glyph_glossary.deinit();
        // if (self.kitty_dnd) |dnd| dnd.destroy(alloc);
        if let Some(dnd) = self.kitty_dnd.take() {
            kitty::dnd::State::destroy(dnd, alloc);
        }
        // self.* = undefined;
    }

    /// Return a terminal.Stream that can process VT streams and update this
    /// terminal state. The streams will only process read-only data that
    /// modifies terminal state.
    ///
    /// Sequences that query or otherwise require output will be ignored.
    /// If you want to handle side effects, use `vtHandler` and set the
    /// effects field yourself, then initialize a stream.
    ///
    /// This must be deinitialized by the caller.
    ///
    /// Important: this creates a new stream each time with fresh parser state.
    /// If you need to persist parser state across multiple writes (e.g.
    /// for handling escape sequences split across write boundaries), you
    /// must store and reuse the returned stream.
    pub fn vt_stream(&mut self) -> Stream<'_> {
        // return Stream.init(.{
        //     .allocator = self.gpa(),
        //     .handler = self.vtHandler(),
        // });
        // CORRECTION (parity tail S7-7): `stream::Options.allocator` is the
        // runtime handle, so `self.gpa()` is threaded as the reference threads
        // it; it used to be the process-static `GLOBAL_ZIG_ALLOCATOR` (the
        // gate) with `gpa()` set separately as the capture's allocator (S7-4).
        let gpa = self.gpa();
        let handler = self.vt_handler();
        Stream::init(crate::stream::Options {
            allocator: Some(gpa),
            handler,
            continuation_max_bytes: None,
            osc_unknown_max_bytes: 0,
        })
    }

    /// This is the handler-side only for vtStream.
    pub fn vt_handler(&mut self) -> Handler<'_> {
        // return .init(self);
        let self_ptr: *mut Terminal = self;
        Handler::init(self_ptr)
    }

    /// Change the cursor's current shape and blink behavior.
    ///
    /// The terminal parser uses this for DECSCUSR (`CSI Ps SP q`), but the behavior
    /// is general: `.default` selects the configured defaults, while any other
    /// value selects a concrete appearance until it is changed again or reset.
    pub fn set_cursor_style(&mut self, value: ansi::CursorStyle) {
        use crate::screen::CursorStyle as Style;
        use ansi::CursorStyle as Req;

        // Remember whether future configuration changes should update the visible
        // cursor. An explicit appearance must remain in effect until the program
        // selects the default again.
        self.cursor.is_default = value == Req::Default;

        // Convert the request into the concrete values used by the renderer and
        // terminal mode state. A null default blink means the emulator default.
        let blink = match value {
            Req::Default => self.cursor.default_blink.unwrap_or(true),
            Req::SteadyBlock | Req::SteadyBar | Req::SteadyUnderline => false,
            Req::BlinkingBlock | Req::BlinkingBar | Req::BlinkingUnderline => true,
        };
        self.modes.set(modespkg::Mode::CursorBlinking, blink);

        let style = match value {
            Req::Default => self.cursor.default_style,
            Req::BlinkingBlock | Req::SteadyBlock => Style::Block,
            Req::BlinkingBar | Req::SteadyBar => Style::Bar,
            Req::BlinkingUnderline | Req::SteadyUnderline => Style::Underline,
        };
        // SAFETY: `screens.active` is a stable heap pointer owned by
        // `self.screens`; the write is short-lived and does not alias.
        unsafe { (*self.screens.active).cursor.cursor_style = style };
    }

    /// Change the default cursor shape.
    ///
    /// If the cursor currently follows its defaults, the visible shape changes
    /// immediately. Otherwise the new shape is saved for the next reset or default
    /// selection, such as DECSCUSR `CSI 0 SP q`.
    pub fn set_default_cursor_style(&mut self, configured_style: crate::screen::CursorStyle) {
        // Always retain the new default, even while an explicit appearance is
        // active, so a later reset or default request can restore it.
        self.cursor.default_style = configured_style;

        // Do not overwrite an appearance explicitly selected by the program.
        if self.cursor.is_default {
            self.set_cursor_style(ansi::CursorStyle::Default);
        }
    }

    /// Change the default cursor blink behavior.
    ///
    /// Null selects the terminal emulator default (blinking). Like the default
    /// shape, this is applied immediately only when the cursor currently follows
    /// its defaults; otherwise it is saved for the next reset or default selection.
    pub fn set_default_cursor_blink(&mut self, blink: Option<bool>) {
        // Keep the configured value separate from the currently resolved mode so
        // null can continue to mean "use the emulator default."
        self.cursor.default_blink = blink;

        // Do not overwrite blink behavior explicitly selected by the program.
        if self.cursor.is_default {
            self.set_cursor_style(ansi::CursorStyle::Default);
        }
    }

    /// The I/O implementation we should use for this terminal.
    // pub fn io(self: *Terminal) std.Io {
    //     return self.screens.active.io;
    // }
    // PORT NOTE: see the `io` field — rustty stores it on the Terminal.
    pub fn io(&self) -> crate::sys::Io {
        self.io
    }

    /// The general allocator we should use for this terminal.
    pub fn gpa(&mut self) -> Allocator {
        // return self.screens.active.alloc;
        unsafe { (*self.screens.active).alloc }
    }

    /// Returns the current color for an xterm OSC color target.
    ///
    /// Unsupported dynamic and special colors return `None`. The cursor color
    /// follows xterm-style reporting and falls back to the foreground color when
    /// no explicit cursor color is set.
    pub fn color_for_xterm(&self, target: osc::parsers::color::Target) -> Option<color::RGB> {
        use osc::parsers::color::Target;
        match target {
            Target::Palette(i) => Some(self.colors.palette.current[i as usize]),
            Target::Dynamic(dynamic) => match dynamic {
                color::Dynamic::Foreground => self.colors.foreground.get(),
                color::Dynamic::Background => self.colors.background.get(),
                color::Dynamic::Cursor => self
                    .colors
                    .cursor
                    .get()
                    .or_else(|| self.colors.foreground.get()),
                color::Dynamic::PointerForeground
                | color::Dynamic::PointerBackground
                | color::Dynamic::TektronixForeground
                | color::Dynamic::TektronixBackground
                | color::Dynamic::HighlightBackground
                | color::Dynamic::TektronixCursor
                | color::Dynamic::HighlightForeground => None,
            },
            Target::Special(_) => None,
        }
    }

    /// Returns the current color for a Kitty color protocol key.
    ///
    /// Only palette, foreground, background, and cursor colors are backed by
    /// Terminal state. Unsupported keys, or supported dynamic colors without a
    /// value, return `None`.
    pub fn color_for_kitty(&self, key: kitty::color::Kind) -> Option<color::RGB> {
        match key {
            kitty::color::Kind::Palette(palette) => {
                Some(self.colors.palette.current[palette as usize])
            }
            kitty::color::Kind::Special(special) => match special {
                kitty::color::Special::Foreground => self.colors.foreground.get(),
                kitty::color::Special::Background => self.colors.background.get(),
                kitty::color::Special::Cursor => self.colors.cursor.get(),
                _ => None,
            },
        }
    }

    /// Change the primary screen's maximum scrollback allocation in bytes.
    ///
    /// Null removes the byte limit and zero disables scrollback. Disabling
    /// scrollback also immediately erases retained history and changes future
    /// scrolling to use the no-scrollback path. The alternate screen is
    /// intentionally unaffected because it never retains scrollback.
    pub fn set_scrollback_max_bytes(&mut self, max: Option<usize>) {
        // const primary = self.screens.get(.primary).?;
        let primary: *mut Screen = self.screens.get(crate::screen_set::Key::Primary).unwrap();

        // SAFETY: `primary` is a stable heap pointer owned by `self.screens`
        // and is not aliased for the duration of these calls.
        unsafe {
            (*primary).pages.set_max_bytes(max);
            (*primary).no_scrollback = max == Some(0);

            if (*primary).no_scrollback {
                (*primary).erase_history(None);
            }
        }
    }

    /// Change the primary screen's maximum number of physical scrollback lines.
    ///
    /// Null removes the line limit. The alternate screen is intentionally
    /// unaffected because it never retains scrollback.
    pub fn set_scrollback_max_lines(&mut self, max: Option<usize>) {
        // const primary = self.screens.get(.primary).?;
        let primary: *mut Screen = self.screens.get(crate::screen_set::Key::Primary).unwrap();

        // SAFETY: as above.
        unsafe { (*primary).pages.set_max_lines(max) };
    }

    /// Returns true if the point is dirty, used for testing.
    fn is_dirty(&self, pt: point::Point) -> bool {
        // return t.screens.active.pages.getCell(pt).?.isDirty();
        unsafe {
            (*self.screens.active)
                .pages
                .get_cell(pt)
                .unwrap()
                .is_dirty()
        }
    }

    /// Clear all dirty bits. Testing only.
    fn clear_dirty(&mut self) {
        // t.screens.active.pages.clearDirty();
        self.active_mut().pages.clear_dirty();
    }

    // --- PORT NOTE: Rust-only glue (no logic; not from Terminal.zig) -------
    // The reference accesses the active screen inline as `self.screens.active`
    // (a `*Screen` raw pointer). The Rust port centralizes that raw-pointer
    // deref behind a helper, mirroring `screen_set` exposing `active: *mut
    // Screen`. This is the `*mut Screen` seam the method bodies use in vt-12b.
    /// The active screen for this terminal (`self.screens.active`).
    ///
    /// # Safety
    /// `self.screens.active` aliases an entry owned by `self.screens.all`; this
    /// returns a `&mut Screen` borrowed from `&mut self`, so the usual aliasing
    /// rules apply (don't hold it across a `screens` mutation that reallocates).
    #[inline]
    pub fn active_mut(&mut self) -> &mut Screen {
        // unsafe { &mut *self.screens.active }
        unsafe { &mut *self.screens.active }
    }
}

// PORT NOTE (parity tail S7-3): upstream's `setTitle`/`setPwd` allocation-
// failure tests (fdd255c78, 1d5b6f90e) and the C-API `get vt_processing_error`
// test (439d35e27) use `std.testing.FailingAllocator` / `failing_allocator`,
// and so does the port now that `pwd`/`title` grow through `self.gpa()`. The
// port-only `SetStringFp` tripwire stand-in that used to sit here is gone.

// const resize_tw = tripwire.module(enum {
//     tabstops,
//     primary_screen,
//     alternate_screen,
//     alternate_screen_init,
// }, resize);
//
// PORT NOTE: the Zig `tripwire.module(P, E)` is a comptime function returning a
// type with type-static state; `rustty_util::tripwire::Module<P, E>` is an
// INSTANCE, held here in a `thread_local!`. The fail-point NAMES are the
// reference's verbatim. Distinct from
// `tabstops::RESIZE_TW` (which injects inside `Tabstops::init`) and from
// `screen::RESIZE_TW` (which injects inside `Screen::resize`).
#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum ResizeFp {
    Tabstops,
    PrimaryScreen,
    AlternateScreen,
    AlternateScreenInit,
}

#[cfg(test)]
thread_local! {
    pub(crate) static RESIZE_TW:
        core::cell::RefCell<rustty_util::tripwire::Module<ResizeFp, OutOfMemory>> =
        core::cell::RefCell::new(rustty_util::tripwire::module());
}

/// `try resize_tw.check(point)`.
#[cfg(test)]
#[inline]
fn resize_tw_check(point: ResizeFp) -> Result<(), OutOfMemory> {
    RESIZE_TW.with(|tw| tw.borrow_mut().check(point))
}

/// `resize_tw.errorAlways(point, error.OutOfMemory)`.
#[cfg(test)]
pub(crate) fn resize_tw_error_always(point: ResizeFp) {
    RESIZE_TW.with(|tw| tw.borrow_mut().errorAlways(point, OutOfMemory));
}

/// `try resize_tw.end(mode)`.
#[cfg(test)]
pub(crate) fn resize_tw_end(
    mode: rustty_util::tripwire::ResetMode,
) -> Result<(), rustty_util::tripwire::UntrippedError> {
    RESIZE_TW.with(|tw| tw.borrow_mut().end(mode))
}

#[cfg(test)]
mod tests;
