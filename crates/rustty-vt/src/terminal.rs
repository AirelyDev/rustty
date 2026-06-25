//! The primary terminal emulation structure. This represents a single
//! "terminal" containing a grid of characters and exposes various operations
//! on that grid. This also maintains the scrollback buffer.
//!
//! rustty-vt terminal
//!
//! PORTS: ghostty src/terminal/Terminal.zig
//!
//! PORT STATUS: skeleton (struct-first; method bodies are `todo!()`, ported in
//! vt-12b)

// PORT NOTE (vt-12a skeleton): every method body is `todo!()` and many fields
// are not yet read, so we allow the lints that the skeleton trivially trips.
// These allows are removed as vt-12b fills the bodies.
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
use crate::charsets;
use crate::csi;
use crate::kitty;
use crate::modes as modespkg;
use crate::mouse;
use crate::osc;
use crate::page as pagepkg;
use crate::page::{Page, Row};
use crate::pagelist::OutOfMemory;
use crate::point;
use crate::screen::Screen;
use crate::screen_set::ScreenSet;
use crate::sgr;
use crate::size;
use crate::stream_terminal::{Handler, Stream};
use crate::tabstops::Tabstops;
use color::{DynamicPalette, DynamicRGB};

// PORT NOTE: `terminal/color.zig` is `crate::color`.
use crate::color;

use allocator_api2::alloc::Global as Allocator;

// const log = std.log.scoped(.terminal);

/// Default tabstop interval
const TABSTOP_INTERVAL: u16 = 8;

/// Per-session Glyph Protocol registrations.
///
/// PORT NOTE (vt-12a deferral): the Glyph Protocol is font-blocked and remains
/// deferred past vt-12 (see DEFERRALS.md). `apc/glyph.zig`'s `Glossary` is a
/// ZST/no-op stub here; the `glyphProtocol` request/response types
/// (`glyph.Request`/`glyph.Response`) are likewise ZST stubs until reactivated.
#[derive(Debug, Default)]
pub struct GlyphGlossary;

impl GlyphGlossary {
    /// `.empty` — an empty glossary.
    pub const EMPTY: GlyphGlossary = GlyphGlossary;

    // pub fn deinit(self: *Glossary, alloc: Allocator) void
    pub fn deinit(&mut self, alloc: Allocator) {
        todo!()
    }
}

/// `glyph.Request` — Glyph Protocol request (font-blocked ZST stub; see
/// `GlyphGlossary`).
#[derive(Debug)]
pub struct GlyphRequest;

/// `glyph.Response` — Glyph Protocol response (font-blocked ZST stub; see
/// `GlyphGlossary`).
#[derive(Debug)]
pub struct GlyphResponse;

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
#[derive(Debug, Clone, Copy)]
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
pub struct Colors {
    pub background: DynamicRGB,
    pub foreground: DynamicRGB,
    pub cursor: DynamicRGB,
    pub palette: DynamicPalette,
}

impl Colors {
    // pub const default: Colors = .{ ... };
    pub fn default() -> Colors {
        todo!()
    }
}

/// This is a set of dirty flags the renderer can use to determine
/// what parts of the screen need to be redrawn. It is up to the renderer
/// to clear these flags.
///
/// This only contains dirty flags for terminal state, not for the screen
/// state. The screen state has its own dirty flags.
// pub const Dirty = packed struct { ... }
#[derive(Debug, Clone, Copy, Default)]
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
    pub max_scrollback: usize,
    pub colors: Colors,

    /// The default mode state. When the terminal gets a reset, it
    /// will revert back to this state.
    pub default_modes: modespkg::ModePacked,

    /// The total storage limit for Kitty images in bytes. Has no effect
    /// if kitty images are disabled at build-time.
    // kitty_image_storage_limit: usize = switch (build_options.artifact) { ... }
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
}

/// The modes for DECCOLM.
// pub const DeccolmMode = enum(u1) { @"80_cols" = 0, @"132_cols" = 1 };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeccolmMode {
    Cols80 = 0,
    Cols132 = 1,
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
    // PORT NOTE: Zig `std.ArrayList(u8)` byte payload -> `Vec<u8>` with a
    // trailing-0 sentinel (NEVER lossy String). Getter returns `Option<&[u8]>`
    // (NUL-stripped).
    pub pwd: Vec<u8>,

    /// The title of the terminal as set by escape sequences (e.g. OSC 0/2).
    pub title: Vec<u8>,

    /// The color state for this terminal.
    pub colors: Colors,

    /// The previous printed character. This is used for the repeat previous
    /// char CSI (ESC [ <n> b).
    // previous_char: ?u21 = null
    pub previous_char: Option<u32>,

    /// The modes that this terminal currently has active.
    pub modes: modespkg::ModeState,

    /// The most recently set mouse shape for the terminal.
    pub mouse_shape: mouse::Shape,

    /// Per-session Glyph Protocol registrations.
    pub glyph_glossary: GlyphGlossary,

    /// These are just a packed set of flags we may set on the terminal.
    pub flags: Flags,
}

impl Terminal {
    /// Initialize a new terminal.
    pub fn init(alloc: Allocator, opts: Options) -> Result<Terminal, OutOfMemory> {
        todo!()
    }

    pub fn deinit(&mut self, alloc: Allocator) {
        todo!()
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
        todo!()
    }

    /// This is the handler-side only for vtStream.
    pub fn vt_handler(&mut self) -> Handler<'_> {
        todo!()
    }

    /// The general allocator we should use for this terminal.
    pub fn gpa(&mut self) -> Allocator {
        todo!()
    }

    /// Print UTF-8 encoded string to the terminal.
    pub fn print_string(&mut self, str: &[u8]) -> Result<(), OutOfMemory> {
        todo!()
    }

    /// Print the previous printed character a repeated amount of times.
    pub fn print_repeat(&mut self, count_req: usize) -> Result<(), OutOfMemory> {
        todo!()
    }

    pub fn print(&mut self, c: u32) -> Result<(), OutOfMemory> {
        todo!()
    }

    fn print_cell(&mut self, unmapped_c: u32, wide: pagepkg::Wide) {
        todo!()
    }

    fn print_wrap(&mut self) -> Result<(), OutOfMemory> {
        todo!()
    }

    /// Set the charset into the given slot.
    pub fn configure_charset(&mut self, slot: charsets::Slots, set: charsets::Charset) {
        todo!()
    }

    /// Invoke the charset in slot into the active slot. If single is true,
    /// then this will only be invoked for a single character.
    pub fn invoke_charset(
        &mut self,
        active: charsets::ActiveSlot,
        slot: charsets::Slots,
        single: bool,
    ) {
        todo!()
    }

    /// Carriage return moves the cursor to the first column.
    pub fn carriage_return(&mut self) {
        todo!()
    }

    /// Linefeed moves the cursor to the next line.
    pub fn linefeed(&mut self) -> Result<(), OutOfMemory> {
        todo!()
    }

    /// Backspace moves the cursor back a column (but not less than 0).
    pub fn backspace(&mut self) {
        todo!()
    }

    /// Move the cursor up amount lines. If amount is greater than the maximum
    /// move distance then it is internally adjusted to the maximum. If amount is
    /// 0, adjust it to 1.
    pub fn cursor_up(&mut self, count_req: usize) {
        todo!()
    }

    /// Move the cursor down amount lines. If amount is greater than the maximum
    /// move distance then it is internally adjusted to the maximum. This sequence
    /// will not scroll the screen or scroll region. If amount is 0, adjust it to 1.
    pub fn cursor_down(&mut self, count_req: usize) {
        todo!()
    }

    /// Move the cursor right amount columns. If amount is greater than the
    /// maximum move distance then it is internally adjusted to the maximum.
    /// This sequence will not scroll the screen or scroll region. If amount is
    /// 0, adjust it to 1.
    pub fn cursor_right(&mut self, count_req: usize) {
        todo!()
    }

    /// Move the cursor to the left amount cells. If amount is 0, adjust it to 1.
    pub fn cursor_left(&mut self, count_req: usize) {
        todo!()
    }

    /// Save cursor position and further state.
    ///
    /// The primary and alternate screen have distinct save state. One saved state
    /// is kept per screen (main / alternative). If for the current screen state
    /// was already saved it is overwritten.
    pub fn save_cursor(&mut self) {
        todo!()
    }

    /// Restore cursor position and other state.
    ///
    /// The primary and alternate screen have distinct save state.
    /// If no save was done before values are reset to their initial values.
    pub fn restore_cursor(&mut self) {
        todo!()
    }

    /// Set the character protection mode for the terminal.
    pub fn set_protected_mode(&mut self, mode: ansi::ProtectedMode) {
        todo!()
    }

    /// Perform a semantic prompt command.
    ///
    /// If there is an error, we do our best to get the terminal into
    /// some coherent state, since callers typically can't handle errors
    /// (since they're sending sequences via the pty).
    pub fn semantic_prompt(&mut self, cmd: osc::SemanticPrompt) -> Result<(), OutOfMemory> {
        todo!()
    }

    // OSC 133;L
    fn semantic_prompt_fresh_line(&mut self) -> Result<(), OutOfMemory> {
        todo!()
    }

    /// Returns true if the cursor is currently at a prompt. Another way to look
    /// at this is it returns false if the shell is currently outputting something.
    /// This requires shell integration (semantic prompt integration).
    ///
    /// If the shell integration doesn't exist, this will always return false.
    pub fn cursor_is_at_prompt(&mut self) -> bool {
        todo!()
    }

    /// Horizontal tab moves the cursor to the next tabstop, clearing
    /// the screen to the left the tabstop.
    pub fn horizontal_tab(&mut self) {
        todo!()
    }

    // Same as horizontalTab but moves to the previous tabstop instead of the next.
    pub fn horizontal_tab_back(&mut self) {
        todo!()
    }

    /// Clear tab stops.
    pub fn tab_clear(&mut self, cmd: csi::TabClear) {
        todo!()
    }

    /// Set a tab stop on the current cursor.
    /// TODO: test
    pub fn tab_set(&mut self) {
        todo!()
    }

    /// TODO: test
    pub fn tab_reset(&mut self) {
        todo!()
    }

    /// Move the cursor to the next line in the scrolling region, possibly scrolling.
    ///
    /// If the cursor is outside of the scrolling region: move the cursor one line
    /// down if it is not on the bottom-most line of the screen.
    ///
    /// If the cursor is inside the scrolling region:
    ///   If the cursor is on the bottom-most line of the scrolling region:
    ///     invoke scroll up with amount=1
    ///   If the cursor is not on the bottom-most line of the scrolling region:
    ///     move the cursor one line down
    ///
    /// This unsets the pending wrap state without wrapping.
    pub fn index(&mut self) -> Result<(), OutOfMemory> {
        todo!()
    }

    /// Move the cursor to the previous line in the scrolling region, possibly
    /// scrolling.
    ///
    /// If the cursor is outside of the scrolling region, move the cursor one
    /// line up if it is not on the top-most line of the screen.
    ///
    /// If the cursor is inside the scrolling region:
    ///
    ///   * If the cursor is on the top-most line of the scrolling region:
    ///     invoke scroll down with amount=1
    ///   * If the cursor is not on the top-most line of the scrolling region:
    ///     move the cursor one line up
    pub fn reverse_index(&mut self) {
        todo!()
    }

    /// Set Cursor Position. Move cursor to the position indicated
    /// by row and column (1-indexed). If column is 0, it is adjusted to 1.
    /// If column is greater than the right-most column it is adjusted to
    /// the right-most column. If row is 0, it is adjusted to 1. If row is
    /// greater than the bottom-most row it is adjusted to the bottom-most
    /// row.
    pub fn set_cursor_pos(&mut self, row_req: usize, col_req: usize) {
        todo!()
    }

    /// Set Top and Bottom Margins If bottom is not specified, 0 or bigger than
    /// the number of the bottom-most row, it is adjusted to the number of the
    /// bottom most row.
    ///
    /// If top < bottom set the top and bottom row of the scroll region according
    /// to top and bottom and move the cursor to the top-left cell of the display
    /// (when in cursor origin mode is set to the top-left cell of the scroll region).
    ///
    /// Otherwise: Set the top and bottom row of the scroll region to the top-most
    /// and bottom-most line of the screen.
    ///
    /// Top and bottom are 1-indexed.
    pub fn set_top_and_bottom_margin(&mut self, top_req: usize, bottom_req: usize) {
        todo!()
    }

    /// DECSLRM
    pub fn set_left_and_right_margin(&mut self, left_req: usize, right_req: usize) {
        todo!()
    }

    /// Scroll the text down by one row.
    pub fn scroll_down(&mut self, count: usize) {
        todo!()
    }

    /// Removes amount lines from the top of the scroll region. The remaining lines
    /// to the bottom margin are shifted up and space from the bottom margin up
    /// is filled with empty lines.
    ///
    /// The new lines are created according to the current SGR state.
    ///
    /// Does not change the (absolute) cursor position.
    pub fn scroll_up(&mut self, count: usize) -> Result<(), OutOfMemory> {
        todo!()
    }

    /// Scroll the viewport of the terminal grid.
    pub fn scroll_viewport(&mut self, behavior: ScrollViewport) {
        todo!()
    }

    /// To be called before shifting a row (as in insertLines and deleteLines)
    ///
    /// Takes care of boundary conditions such as potentially split wide chars
    /// across scrolling region boundaries and orphaned spacer heads at line
    /// ends.
    fn row_will_be_shifted(&mut self, page: &mut Page, row: &mut Row) {
        todo!()
    }

    /// Insert amount lines at the current cursor row. The contents of the line
    /// at the current cursor row and below (to the bottom-most line in the
    /// scrolling region) are shifted down by amount lines. The contents of the
    /// amount bottom-most lines in the scroll region are lost.
    ///
    /// This unsets the pending wrap state without wrapping. If the current cursor
    /// position is outside of the current scroll region it does nothing.
    ///
    /// If amount is greater than the remaining number of lines in the scrolling
    /// region it is adjusted down (still allowing for scrolling out every remaining
    /// line in the scrolling region)
    ///
    /// In left and right margin mode the margins are respected; lines are only
    /// scrolled in the scroll region.
    ///
    /// All cleared space is colored according to the current SGR state.
    ///
    /// Moves the cursor to the left margin.
    pub fn insert_lines(&mut self, count: usize) {
        todo!()
    }

    /// Removes amount lines from the current cursor row down. The remaining lines
    /// to the bottom margin are shifted up and space from the bottom margin up is
    /// filled with empty lines.
    ///
    /// If the current cursor position is outside of the current scroll region it
    /// does nothing. If amount is greater than the remaining number of lines in the
    /// scrolling region it is adjusted down.
    ///
    /// In left and right margin mode the margins are respected; lines are only
    /// scrolled in the scroll region.
    ///
    /// If the cell movement splits a multi cell character that character cleared,
    /// by replacing it by spaces, keeping its current attributes. All other
    /// cleared space is colored according to the current SGR state.
    ///
    /// Moves the cursor to the left margin.
    pub fn delete_lines(&mut self, count: usize) {
        todo!()
    }

    /// Inserts spaces at current cursor position moving existing cell contents
    /// to the right. The contents of the count right-most columns in the scroll
    /// region are lost. The cursor position is not changed.
    ///
    /// This unsets the pending wrap state without wrapping.
    ///
    /// The inserted cells are colored according to the current SGR state.
    pub fn insert_blanks(&mut self, count: usize) {
        todo!()
    }

    /// Removes amount characters from the current cursor position to the right.
    /// The remaining characters are shifted to the left and space from the right
    /// margin is filled with spaces.
    ///
    /// If amount is greater than the remaining number of characters in the
    /// scrolling region, it is adjusted down.
    ///
    /// Does not change the cursor position.
    pub fn delete_chars(&mut self, count_req: usize) {
        todo!()
    }

    pub fn erase_chars(&mut self, count_req: usize) {
        todo!()
    }

    /// Erase the line.
    pub fn erase_line(&mut self, mode: csi::EraseLine, protected_req: bool) {
        todo!()
    }

    /// Erase the display.
    pub fn erase_display(&mut self, mode: csi::EraseDisplay, protected_req: bool) {
        todo!()
    }

    /// Resets all margins and fills the whole screen with the character 'E'
    ///
    /// Sets the cursor to the top left corner.
    pub fn decaln(&mut self) -> Result<(), OutOfMemory> {
        todo!()
    }

    /// Execute a kitty graphics command. The buf is used to populate with
    /// the response that should be sent as an APC sequence. The response will
    /// be a full, valid APC sequence.
    ///
    /// If an error occurs, the caller should response to the pty that a
    /// an error occurred otherwise the behavior of the graphics protocol is
    /// undefined.
    pub fn kitty_graphics(
        &mut self,
        alloc: Allocator,
        cmd: &mut kitty::graphics::Command,
    ) -> Option<kitty::graphics::Response> {
        todo!()
    }

    /// Execute a Glyph Protocol APC command against this terminal's per-session
    /// glossary. The returned response, if any, should be sent back to the pty as
    /// a complete APC sequence via `Response.formatWire`.
    pub fn glyph_protocol(
        &mut self,
        alloc: Allocator,
        req: &GlyphRequest,
    ) -> Option<GlyphResponse> {
        todo!()
    }

    /// Set the storage size limit for Kitty graphics across all screens.
    pub fn set_kitty_graphics_size_limit(
        &mut self,
        alloc: Allocator,
        limit: usize,
    ) -> Result<(), OutOfMemory> {
        todo!()
    }

    /// Set the allowed medium types for Kitty graphics image loading
    /// across all screens.
    pub fn set_kitty_graphics_loading_limits(&mut self, limits: kitty::graphics_image::Limits) {
        todo!()
    }

    /// Set a style attribute.
    pub fn set_attribute(&mut self, attr: sgr::Attribute) -> Result<(), OutOfMemory> {
        todo!()
    }

    /// Print the active attributes as a string. This is used to respond to DECRQSS
    /// requests.
    ///
    /// Boolean attributes are printed first, followed by foreground color, then
    /// background color. Each attribute is separated by a semicolon.
    pub fn print_attributes<'a>(&mut self, buf: &'a mut [u8]) -> Result<&'a [u8], OutOfMemory> {
        todo!()
    }

    /// DECCOLM changes the terminal width between 80 and 132 columns. This
    /// function call will do NOTHING unless `setDeccolmSupported` has been
    /// called with "true".
    ///
    /// This breaks the expectation around modern terminals that they resize
    /// with the window. This will fix the grid at either 80 or 132 columns.
    /// The rows will continue to be variable.
    pub fn deccolm(&mut self, alloc: Allocator, mode: DeccolmMode) -> Result<(), OutOfMemory> {
        todo!()
    }

    /// Resize the underlying terminal.
    pub fn resize(
        &mut self,
        alloc: Allocator,
        cols: size::CellCountInt,
        rows: size::CellCountInt,
    ) -> Result<(), OutOfMemory> {
        todo!()
    }

    /// Set the pwd for the terminal.
    pub fn set_pwd(&mut self, pwd: &[u8]) -> Result<(), OutOfMemory> {
        todo!()
    }

    /// Returns the pwd for the terminal, if any. The memory is owned by the
    /// Terminal and is not copied. It is safe until a reset or setPwd.
    // pub fn getPwd(self: *const Terminal) ?[:0]const u8
    // PORT NOTE: Zig `[:0]const u8` -> `Option<&[u8]>` (NUL sentinel stripped).
    pub fn get_pwd(&self) -> Option<&[u8]> {
        todo!()
    }

    /// Set the title for the terminal, as set by escape sequences (e.g. OSC 0/2).
    pub fn set_title(&mut self, t: &[u8]) -> Result<(), OutOfMemory> {
        todo!()
    }

    /// Returns the title for the terminal, if any. The memory is owned by the
    /// Terminal and is not copied. It is safe until a reset or setTitle.
    pub fn get_title(&self) -> Option<&[u8]> {
        todo!()
    }

    /// Switch to the given screen type (alternate or primary).
    ///
    /// This does NOT handle behaviors such as clearing the screen,
    /// copying the cursor, etc. This should be handled by downstream
    /// callers.
    ///
    /// After calling this function, the `self.screen` field will point
    /// to the current screen, and the returned value will be the previous
    /// screen. If the return value is null, then the screen was not
    /// switched because it was already the active screen.
    ///
    /// Note: This is written in a generic way so that we can support
    /// more than two screens in the future if needed. There isn't
    /// currently a spec for this, but it is something I think might
    /// be useful in the future.
    pub fn switch_screen(
        &mut self,
        key: crate::screen_set::Key,
    ) -> Result<Option<*mut Screen>, OutOfMemory> {
        todo!()
    }

    /// Switch screen via a mode switch (e.g. mode 47, 1047, 1049).
    /// This is a much more opinionated operation than `switchScreen`
    /// since it also handles the behaviors of the specific mode,
    /// such as clearing the screen, saving/restoring the cursor,
    /// etc.
    ///
    /// This should be used for legacy compatibility with VT protocols,
    /// but more modern usage should use `switchScreen` instead and handle
    /// details like clearing the screen, cursor saving, etc. manually.
    pub fn switch_screen_mode(
        &mut self,
        mode: SwitchScreenMode,
        enabled: bool,
    ) -> Result<(), OutOfMemory> {
        todo!()
    }

    /// Return the current string value of the terminal. Newlines are
    /// encoded as "\n". This omits any formatting such as fg/bg.
    ///
    /// The caller must free the string.
    pub fn plain_string(&mut self, alloc: Allocator) -> Result<Vec<u8>, OutOfMemory> {
        todo!()
    }

    /// Same as plainString, but respects row wrap state when building the string.
    pub fn plain_string_unwrapped(&mut self, alloc: Allocator) -> Result<Vec<u8>, OutOfMemory> {
        todo!()
    }

    /// Full reset.
    ///
    /// This will attempt to free the existing screen memory but if that fails
    /// this will reuse the existing memory. In the latter case, memory may
    /// be wasted (since its unused) but it isn't leaked.
    pub fn full_reset(&mut self) {
        todo!()
    }

    /// Returns true if the point is dirty, used for testing.
    fn is_dirty(&self, pt: point::Point) -> bool {
        todo!()
    }

    /// Clear all dirty bits. Testing only.
    fn clear_dirty(&mut self) {
        todo!()
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
