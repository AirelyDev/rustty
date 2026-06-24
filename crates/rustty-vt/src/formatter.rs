//! rustty-vt formatter
//!
//! PORTS: ghostty src/terminal/formatter.zig
//!
//! PORT STATUS: skeleton (vt-10) — types + method signatures; bodies `todo!()`
//! land in vt-11. `TerminalFormatter` is a compiling stub (needs `Terminal`,
//! vt-12).
// PORT NOTE: this skeleton has `todo!()` method bodies (vt-11 lands the real
// bodies), so the parameters/imports they will consume are not yet read. The
// `unused_*` and `clippy::should_implement_trait` allows are skeleton-only and
// drop out once the bodies land.
#![allow(clippy::todo, dead_code, unused_variables, unused_imports)]
#![allow(clippy::should_implement_trait)]

// const std = @import("std");
// const assert = @import("../quirks.zig").inlineAssert;
// const lib = @import("lib.zig");
// const Allocator = std.mem.Allocator;
// const color = @import("color.zig");
// const size = @import("size.zig");
// const charsets = @import("charsets.zig");
// const hyperlink = @import("hyperlink.zig");
// const kitty = @import("kitty.zig");
// const modespkg = @import("modes.zig");
// const Screen = @import("Screen.zig");
// const Terminal = @import("Terminal.zig");
// const Cell = @import("page.zig").Cell;
// const Coordinate = @import("point.zig").Coordinate;
// const Page = @import("page.zig").Page;
// const PageList = @import("PageList.zig");
// const Pin = PageList.Pin;
// const Row = @import("page.zig").Row;
// const Selection = @import("Selection.zig");
// const Style = @import("style.zig").Style;
use crate::color;
use crate::page::u21;
use crate::page::Page;
use crate::pagelist::{PageList, Pin};
use crate::point::Coordinate;
use crate::screen::Screen;
use crate::selection::Selection;
use crate::size;

// PORT NOTE: the reference allocator maps to `allocator_api2::alloc::Global`
// (the crate's allocator seam).
use allocator_api2::alloc::Global as Allocator;

// PORT NOTE: the reference writer is `*std.Io.Writer` (a byte sink). Per the
// Screen-SCC plan seam, this ports to `&mut dyn std::io::Write`.
use std::io;

/// Formats available.
// PORT NOTE: `Format = lib.Enum(lib.target, &.{...})`; plain enum for the `.zig`
// target with the keys in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    // Plain text.
    Plain,

    // Include VT sequences to preserve colors, styles, URLs, etc.
    // This is predominantly SGR sequences but may contain others as needed.
    //
    // Note that for reference colors, like palette indices, this will
    // vary based on the formatter and you should see the docs. For example,
    // PageFormatter with VT will emit SGR sequences with palette indices,
    // not the color itself.
    //
    // For VT, newlines will be emitted as `\r\n` so that the cursor properly
    // moves back to the beginning prior emitting follow-up lines.
    Vt,

    // HTML output.
    //
    // This will emit inline styles for as much styling as possible,
    // in the interest of simplicity and ease of editing. This isn't meant
    // to build the most beautiful or efficient HTML, but rather to be
    // stylistically correct.
    //
    // For colors, RGB values are emitted as inline CSS (#RRGGBB) while palette
    // indices use CSS variables (var(--vt-palette-N)). The palette colors are
    // emitted by TerminalFormatter.Extra.palette as a <style> block if you
    // want to also include that. But if you only format a screen or lower,
    // the formatter doesn't have access to the current palette to render it.
    //
    // Newlines are emitted as actual '\n' characters. Consumers should use
    // CSS white-space: pre or pre-wrap to preserve spacing and alignment.
    Html,
}

/// Returns true if the format emits styled output (not plaintext).
pub fn format_styled(fmt: Format) -> bool {
    todo!()
}

pub struct CodepointMap {
    /// Unicode codepoint range to replace.
    /// Asserts: range[0] <= range[1]
    pub range: [u21; 2],

    /// Replacement value for this range.
    pub replacement: Replacement,
}

pub enum Replacement {
    /// A single replacement codepoint.
    Codepoint(u21),

    /// A UTF-8 encoded string to replace with. Asserts the
    /// UTF-8 encoding (must be valid).
    // PORT NOTE: `[]const u8` -> `Vec<u8>` (never a lossy `String`).
    String(Vec<u8>),
}

/// Common encoding options regardless of what exact formatter is used.
pub struct Options {
    /// The format to emit.
    pub emit: Format,

    /// Whether to unwrap soft-wrapped lines. If false, this will emit the
    /// screen contents as it is rendered on the page in the given size.
    pub unwrap: bool,

    /// Trim trailing whitespace on lines with other text. Trailing blank
    /// lines are always trimmed. This only affects trailing whitespace
    /// on rows that have at least one other cell with text. Whitespace
    /// is currently only space characters (0x20).
    pub trim: bool,

    /// Replace matching Unicode codepoints with some other values.
    /// This will use the last matching range found in the list.
    // PORT NOTE: the reference `std.MultiArrayList(CodepointMap)` ports to a
    // `Vec<CodepointMap>` (the SoA optimization is not part of the parity
    // contract for this small map).
    pub codepoint_map: Option<Vec<CodepointMap>>,

    /// Set a background and foreground color to use for the "screen".
    /// For styled formats, this will emit the proper sequences or styles.
    pub background: Option<color::RGB>,
    pub foreground: Option<color::RGB>,

    /// If set, then styled formats in `emit` will use this palette to
    /// emit colors directly as RGB. If this is null, styled formats will
    /// still work but will use deferred palette styling (e.g. CSS variables
    /// for HTML or the actual palette indexes for VT).
    pub palette: Option<*const color::Palette>,
}

impl Options {
    pub fn plain() -> Options {
        todo!()
    }
    pub fn vt() -> Options {
        todo!()
    }
    pub fn html() -> Options {
        todo!()
    }
}

/// Maps byte positions in formatted output to PageList pins.
///
/// Used by formatters that operate on PageLists to track the source position
/// of each byte written. The caller is responsible for freeing the map.
pub struct PinMap<'a> {
    pub alloc: Allocator,
    pub map: &'a mut Vec<Pin>,
}

/// Terminal formatter formats the active terminal screen.
///
/// This will always only emit data related to the currently active screen.
/// If you want to emit data for a specific screen (e.g. primary vs alt), then
/// switch to that screen in the terminal prior to using this.
///
/// If you want to emit data for all screens (a less common operation), then
/// you must create a no-content TerminalFormatter followed by multiple
/// explicit ScreenFormatter calls. This isn't a common operation so this
/// little extra work should be acceptable.
///
/// For styled formatting, this will emit the palette colors at the
/// beginning so that the output can be rendered properly according to
/// the current terminal state.
//
// PORT NOTE: `TerminalFormatter` is a COMPILING STUB. It depends on `Terminal`
// (`terminal: *const Terminal`), which is ported in vt-12. The struct and its
// `Extra` packed-struct are defined here verbatim (with comments preserved) but
// the `terminal` field is held back until `Terminal` exists. `init`/`format`
// are signature stubs with `todo!()` bodies.
pub struct TerminalFormatter {
    // PORT NOTE: `terminal: *const Terminal` — DEFERRED to vt-12 (needs the
    // ported `Terminal` type). Restored once `terminal.rs` lands.
    /// The common options
    pub opts: Options,

    /// The content to include.
    pub content: Content,

    /// Extra stuff to emit, such as terminal modes, palette, cursor, etc.
    /// This information is ONLY emitted when the format is "vt".
    pub extra: TerminalExtra,

    /// If non-null, then `map` will contain the Pin of every byte
    /// byte written to the writer offset by the byte index. It is the
    /// caller's responsibility to free the map.
    ///
    /// Note that some emitted bytes may not correspond to any Pin, such as
    /// the extra data around terminal state (palette, modes, etc.). For these,
    /// we'll map it to the most previous pin so there is some continuity but
    /// its an arbitrary choice.
    ///
    /// Warning: there is a significant performance hit to track this
    pub pin_map: Option<PinMap<'static>>,
}

// PORT NOTE: nested `TerminalFormatter.Extra` packed struct, hoisted to a
// top-level name to avoid the inherent-associated-type limitation in Rust.
#[derive(Clone, Copy)]
pub struct TerminalExtra {
    /// Emit the palette using OSC 4 sequences.
    pub palette: bool,

    /// Emit terminal modes that differ from their defaults using CSI h/l
    /// sequences. Defaults are according to the Ghostty defaults which
    /// are generally match most terminal defaults. This will include
    /// things like current screen, bracketed mode, mouse event reporting,
    /// etc.
    pub modes: bool,

    /// Emit scrolling region state using DECSTBM and DECSLRM sequences.
    pub scrolling_region: bool,

    /// Emit tabstop positions by clearing all tabs (CSI 3 g) and setting
    /// each configured tabstop with HTS.
    pub tabstops: bool,

    /// Emit the present working directory using OSC 7.
    pub pwd: bool,

    /// Emit keyboard modes such as ModifyOtherKeys using CSI > 4 m
    /// sequences.
    pub keyboard: bool,

    /// The screen extras to emit. TerminalFormatter always only
    /// emits data for the currently active screen. If you want to emit
    /// data for all screens, you should manually construct a no-content
    /// terminal formatter, followed by screen formatters.
    pub screen: ScreenExtra,
}

impl TerminalExtra {
    /// Emit nothing.
    pub const NONE: TerminalExtra = TerminalExtra {
        palette: false,
        modes: false,
        scrolling_region: false,
        tabstops: false,
        pwd: false,
        keyboard: false,
        screen: ScreenExtra::NONE,
    };

    /// Emit style-relevant information only such as palettes.
    pub const STYLES: TerminalExtra = TerminalExtra {
        palette: true,
        modes: false,
        scrolling_region: false,
        tabstops: false,
        pwd: false,
        keyboard: false,
        screen: ScreenExtra::STYLES,
    };

    /// Emit everything. This reconstructs the terminal state as closely
    /// as possible.
    pub const ALL: TerminalExtra = TerminalExtra {
        palette: true,
        modes: true,
        scrolling_region: true,
        tabstops: true,
        pwd: true,
        keyboard: true,
        screen: ScreenExtra::ALL,
    };
}

impl TerminalFormatter {
    // PORT NOTE: `init` takes `terminal: *const Terminal` in the reference;
    // DEFERRED to vt-12. The stub signature omits the terminal argument until
    // `Terminal` is ported.
    pub fn init(opts: Options) -> TerminalFormatter {
        todo!()
    }

    pub fn format(self, writer: &mut dyn io::Write) -> io::Result<()> {
        todo!()
    }
}

pub struct ScreenFormatter<'a> {
    /// The screen to format.
    pub screen: *const Screen,

    /// The common options
    pub opts: Options,

    /// The content to include.
    pub content: Content,

    /// Extra stuff to emit, such as cursor, style, hyperlinks, etc.
    /// This information is ONLY emitted when the format is "vt".
    pub extra: ScreenExtra,

    /// If non-null, then `map` will contain the Pin of every byte
    /// byte written to the writer offset by the byte index. It is the
    /// caller's responsibility to free the map.
    ///
    /// Note that some emitted bytes may not correspond to any Pin, such as
    /// the extra data around screen state. For these, we'll map it to the
    /// most previous pin so there is some continuity but its an arbitrary
    /// choice.
    ///
    /// Warning: there is a significant performance hit to track this
    pub pin_map: Option<PinMap<'a>>,
}

// PORT NOTE: nested `ScreenFormatter.Content` union(enum), hoisted to a
// top-level name. Shared by `TerminalFormatter` (which references
// `ScreenFormatter.Content`).
pub enum Content {
    /// Emit no content, only terminal state such as modes, palette, etc.
    /// via extra.
    None,

    /// Emit the content specified by the selection. Null for all.
    /// The selection is inclusive on both ends.
    Selection(Option<Selection>),
}

// PORT NOTE: nested `ScreenFormatter.Extra` packed struct, hoisted to a
// top-level name. Referenced by `TerminalExtra.screen`.
#[derive(Clone, Copy)]
pub struct ScreenExtra {
    /// Emit cursor position using CUP (CSI H).
    pub cursor: bool,

    /// Emit current SGR style state based on the cursor's active style_id.
    /// This reconstructs the SGR attributes (bold, italic, colors, etc.) at
    /// the cursor position.
    pub style: bool,

    /// Emit current hyperlink state using OSC 8 sequences.
    /// This sets the active hyperlink based on cursor.hyperlink_id.
    pub hyperlink: bool,

    /// Emit character protection mode using DECSCA.
    pub protection: bool,

    /// Emit Kitty keyboard protocol state using CSI > u and CSI = sequences.
    pub kitty_keyboard: bool,

    /// Emit character set designations and invocations.
    /// This includes G0-G3 designations (ESC ( ) * +) and GL/GR invocations.
    pub charsets: bool,
}

impl ScreenExtra {
    /// Emit nothing.
    pub const NONE: ScreenExtra = ScreenExtra {
        cursor: false,
        style: false,
        hyperlink: false,
        protection: false,
        kitty_keyboard: false,
        charsets: false,
    };

    /// Emit style-relevant information only.
    pub const STYLES: ScreenExtra = ScreenExtra {
        cursor: false,
        style: true,
        hyperlink: true,
        protection: false,
        kitty_keyboard: false,
        charsets: false,
    };

    /// Emit everything. This reconstructs the screen state as closely
    /// as possible.
    pub const ALL: ScreenExtra = ScreenExtra {
        cursor: true,
        style: true,
        hyperlink: true,
        protection: true,
        kitty_keyboard: true,
        charsets: true,
    };

    fn is_set(self) -> bool {
        todo!()
    }
}

impl<'a> ScreenFormatter<'a> {
    pub fn init(screen: *const Screen, opts: Options) -> ScreenFormatter<'a> {
        todo!()
    }

    pub fn format(self, writer: &mut dyn io::Write) -> io::Result<()> {
        todo!()
    }
}

pub struct PageListFormatter<'a> {
    /// The pagelist to format.
    pub list: *const PageList,

    /// The common options
    pub opts: Options,

    /// The bounds of the PageList to format. The top left and bottom right
    /// MUST be ordered properly.
    pub top_left: Option<Pin>,
    pub bottom_right: Option<Pin>,

    /// If true, the boundaries define a rectangle selection where start_x
    /// and end_x apply to every row, not just the first and last.
    pub rectangle: bool,

    /// If non-null, then `map` will contain the Pin of every byte
    /// byte written to the writer offset by the byte index. It is the
    /// caller's responsibility to free the map.
    ///
    /// Warning: there is a significant performance hit to track this
    pub pin_map: Option<PinMap<'a>>,
}

impl<'a> PageListFormatter<'a> {
    pub fn init(list: *const PageList, opts: Options) -> PageListFormatter<'a> {
        todo!()
    }

    pub fn format(self, writer: &mut dyn io::Write) -> io::Result<()> {
        todo!()
    }
}

/// Page formatter.
///
/// For styled formatting such as VT, this will emit references for palette
/// colors. If you want to capture the palette as-is at the type of formatting,
/// you'll have to emit the sequences for setting up the palette prior to
/// this formatting. (TODO: A function to do this)
pub struct PageFormatter<'a> {
    /// The page to format.
    pub page: *const Page,

    /// The common options
    pub opts: Options,

    /// Start and end points within the page to format. If end x is not given
    /// then it will be the full width. If end y is not given then it will be
    /// the full height.
    ///
    /// The start and end are both inclusive, so equal values will still
    /// return a non-empty result (i.e. a single cell or row).
    ///
    /// The start x is considered the X in the first row and end X is
    /// X in the final row. This isn't a rectangle selection by default.
    ///
    /// If start X falls on the second column of a wide character, then
    /// the entire character will be included (as if you specified the
    /// previous column).
    pub start_x: size::CellCountInt,
    pub start_y: size::CellCountInt,
    pub end_x: Option<size::CellCountInt>,
    pub end_y: Option<size::CellCountInt>,

    /// If true, the start x/y and end x/y define a rectangle selection.
    /// In this case, the boundaries will apply to every row, not just
    /// the first and last.
    pub rectangle: bool,

    /// If non-null, then `map` will contain the x/y coordinate of every
    /// byte written to the writer offset by the byte index. It is the
    /// caller's responsibility to free the map.
    ///
    /// The x/y coordinate will be the coordinates within the page.
    ///
    /// Warning: there is a significant performance hit to track this
    pub point_map: Option<PointMap<'a>>,

    /// The previous trailing state from the prior page. If you're iterating
    /// over multiple pages this helps ensure that unwrapping and other
    /// accounting works properly.
    pub trailing_state: Option<TrailingState>,
}

// PORT NOTE: inline anonymous param struct for `PageFormatter.point_map`.
pub struct PointMap<'a> {
    pub alloc: Allocator,
    pub map: &'a mut Vec<Coordinate>,
}

/// Trailing state. This is used to ensure that rows wrapped across
/// multiple pages are unwrapped properly, as well as other accounting
/// we may do in the future.
#[derive(Clone, Copy)]
pub struct TrailingState {
    pub rows: usize,
    pub cells: usize,
}

impl TrailingState {
    pub const EMPTY: TrailingState = TrailingState { rows: 0, cells: 0 };
}

impl<'a> PageFormatter<'a> {
    /// Initializes a page formatter. Other options can be set directly on the
    /// struct after initialization and before calling `format()`.
    pub fn init(page: *const Page, opts: Options) -> PageFormatter<'a> {
        todo!()
    }

    pub fn format(self, writer: &mut dyn io::Write) -> io::Result<()> {
        todo!()
    }

    pub fn format_with_state(self, writer: &mut dyn io::Write) -> io::Result<TrailingState> {
        todo!()
    }
}
