//! rustty-vt screen
//!
//! PORTS: ghostty src/terminal/Screen.zig
//!
//! PORT STATUS: skeleton (vt-10) — Screen struct + nested types + method
//! signatures; bodies `todo!()` land in vt-11.
// PORT NOTE: this skeleton has `todo!()` method bodies (vt-11 lands the real
// bodies), so the parameters/imports they will consume are not yet read. The
// `unused_*` and `clippy::should_implement_trait` allows are skeleton-only and
// drop out once the bodies land.
#![allow(clippy::todo, dead_code, unused_variables, unused_imports)]
#![allow(clippy::should_implement_trait)]

// const std = @import("std");
// const build_options = @import("terminal_options");
// const Allocator = std.mem.Allocator;
// const assert = @import("../quirks.zig").inlineAssert;
// const ansi = @import("ansi.zig");
// const charsets = @import("charsets.zig");
// const fastmem = @import("../fastmem.zig");
// const kitty = @import("kitty.zig");
// const sgr = @import("sgr.zig");
// const tripwire = @import("../tripwire.zig");
// const unicode = @import("../unicode/main.zig");
// const Selection = @import("Selection.zig");
// const PageList = @import("PageList.zig");
// const selection_codepoints = @import("selection_codepoints.zig");
// const StringMap = @import("StringMap.zig");
// const ScreenFormatter = @import("formatter.zig").ScreenFormatter;
// const osc = @import("osc.zig");
// const pagepkg = @import("page.zig");
// const point = @import("point.zig");
// const size = @import("size.zig");
// const style = @import("style.zig");
// const hyperlink = @import("hyperlink.zig");
// const Offset = size.Offset;
// const Page = pagepkg.Page;
// const Row = pagepkg.Row;
// const Cell = pagepkg.Cell;
// const Pin = PageList.Pin;
use crate::ansi;
use crate::charsets;
use crate::hyperlink;
use crate::kitty;
use crate::osc;
use crate::page as pagepkg;
use crate::page::u21;
use crate::page::{Cell, Page, Row};
use crate::pagelist::{self, OutOfMemory, PageList, Pin};
use crate::point;
use crate::selection::Selection;
use crate::size;
use crate::string_map::StringMap;
use crate::style;

// PORT NOTE: the reference `Allocator.Error` maps to `pagelist::OutOfMemory`;
// the general-purpose allocator maps to `allocator_api2::alloc::Global`, the
// same allocator the PageList pools use (the crate's allocator seam).
use allocator_api2::alloc::Global as Allocator;

// PORT NOTE: the reference formatter writer is `*std.Io.Writer` (a byte sink).
// Per the Screen-SCC plan seam, this ports to `&mut dyn std::io::Write`.
use std::io;

// pub const CursorStyle = @import("cursor.zig").Style;
pub use crate::cursor::Style as CursorStyle;

/// Represents the terminal screen.
pub struct Screen {
    /// The general purpose allocator to use for all memory allocations.
    /// Unfortunately some screen operations do require allocation.
    pub alloc: Allocator,

    /// The list of pages in the screen.
    pub pages: PageList,

    /// Special-case where we want no scrollback whatsoever. We have to flag
    /// this because max_size 0 in PageList gets rounded up to two pages so
    /// we can always have an active screen.
    pub no_scrollback: bool,

    /// The current cursor position
    pub cursor: Cursor,

    /// The saved cursor
    pub saved_cursor: Option<SavedCursor>,

    /// The selection for this screen (if any). This MUST be a tracked selection
    /// otherwise the selection will become invalid. Instead of accessing this
    /// directly to set it, use the `select` function which will assert and
    /// automatically setup tracking.
    pub selection: Option<Selection>,

    /// The charset state
    pub charset: CharsetState,

    /// The current or most recent protected mode. Once a protection mode is
    /// set, this will never become "off" again until the screen is reset.
    /// The current state of whether protection attributes should be set is
    /// set on the Cell pen; this is only used to determine the most recent
    /// protection mode since some sequences such as ECH depend on this.
    pub protected_mode: ansi::ProtectedMode,

    /// The kitty keyboard settings.
    pub kitty_keyboard: kitty::KeyFlagStack,

    /// Kitty graphics protocol state.
    // PORT NOTE: the reference type is
    //   `if (build_options.kitty_graphics) kitty.graphics.ImageStorage else struct {}`.
    // `kitty.graphics.ImageStorage` is DEFERRED to vt-12 (it operates over
    // `Screen`/`Terminal`), so the field uses the deferred
    // `kitty::graphics_storage::Deferred` placeholder. The real `ImageStorage`
    // is wired in once `graphics_storage` is ported (vt-12).
    pub kitty_images: kitty::graphics_storage::Deferred,

    /// Semantic prompt (OSC133) state.
    pub semantic_prompt: SemanticPrompt,

    /// Dirty flags for the renderer.
    pub dirty: Dirty,
}

/// See Terminal.Dirty. This behaves the same way.
// PORT NOTE: `packed struct` of `bool` fields; ported as a plain struct of
// `bool`s preserving each field's observable meaning.
#[derive(Clone, Copy, Default)]
pub struct Dirty {
    /// Set when the selection is set or unset, regardless of if the
    /// selection is changed or not.
    pub selection: bool,

    /// When an OSC8 hyperlink is hovered, we set the full screen as dirty
    /// because links can span multiple lines.
    pub hyperlink_hover: bool,
}

pub struct SemanticPrompt {
    /// This is flipped to true when any sort of semantic content is
    /// seen. In particular, this is set to true only when a `prompt` type
    /// is ever set on our cursor.
    ///
    /// This is used to optimize away semantic content operations if we know
    /// we've never seen them.
    pub seen: bool,

    /// This is set on any `cl` or `click_events` option set on the
    /// most recent OSC 133 commands to specify how click handling in a
    /// prompt is handling.
    pub click: SemanticClick,
}

impl SemanticPrompt {
    pub const DISABLED: SemanticPrompt = SemanticPrompt {
        seen: false,
        click: SemanticClick::None,
    };
}

pub enum SemanticClick {
    None,
    ClickEvents(osc::semantic_prompt::ClickEvents),
    Cl(osc::semantic_prompt::Click),
}

/// The cursor position and style.
pub struct Cursor {
    // The x/y position within the active area.
    pub x: size::CellCountInt,
    pub y: size::CellCountInt,

    /// The visual style of the cursor. This defaults to block because
    /// it has to default to something, but users of this struct are
    /// encouraged to set their own default.
    pub cursor_style: CursorStyle,

    /// The "last column flag (LCF)" as its called. If this is set then the
    /// next character print will force a soft-wrap.
    pub pending_wrap: bool,

    /// The protected mode state of the cursor. If this is true then
    /// all new characters printed will have the protected state set.
    pub protected: bool,

    /// The currently active style. This is the concrete style value
    /// that should be kept up to date. The style ID to use for cell writing
    /// is below.
    pub style: style::Style,

    /// The currently active style ID. The style is page-specific so when
    /// we change pages we need to ensure that we update that page with
    /// our style when used.
    pub style_id: style::Id,

    /// The hyperlink ID that is currently active for the cursor. A value
    /// of zero means no hyperlink is active. (Implements OSC8, saying that
    /// so code search can find it.).
    pub hyperlink_id: hyperlink::Id,

    /// This is the implicit ID to use for hyperlinks that don't specify
    /// an ID. We do an overflowing add to this so repeats can technically
    /// happen with carefully crafted inputs but for real workloads its
    /// highly unlikely -- and the fix is for the TUI program to use explicit
    /// IDs.
    pub hyperlink_implicit_id: size::OffsetInt,

    /// Heap-allocated hyperlink state so that we can recreate it when
    /// the cursor page pin changes. We can't get it from the old screen
    /// state because the page may be cleared. This is heap allocated
    /// because its most likely null.
    pub hyperlink: Option<*mut hyperlink::Hyperlink>,

    /// The current semantic content type for the cursor that will be
    /// applied to any newly written cells.
    pub semantic_content: pagepkg::SemanticContent,
    pub semantic_content_clear_eol: bool,

    /// The pointers into the page list where the cursor is currently
    /// located. This makes it faster to move the cursor.
    pub page_pin: *mut Pin,
    pub page_row: *mut pagepkg::Row,
    pub page_cell: *mut pagepkg::Cell,
}

impl Cursor {
    pub fn deinit(&mut self, alloc: Allocator) {
        todo!()
    }
}

/// Saved cursor state.
pub struct SavedCursor {
    pub x: size::CellCountInt,
    pub y: size::CellCountInt,
    pub style: style::Style,
    pub protected: bool,
    pub pending_wrap: bool,
    pub origin: bool,
    pub charset: CharsetState,
}

/// State required for all charset operations.
pub struct CharsetState {
    /// The list of graphical charsets by slot
    pub charsets: CharsetArray,

    /// GL is the slot to use when using a 7-bit printable char (up to 127)
    /// GR used for 8-bit printable chars.
    pub gl: charsets::Slots,
    pub gr: charsets::Slots,

    /// Single shift where a slot is used for exactly one char.
    pub single_shift: Option<charsets::Slots>,
}

/// An array to map a charset slot to a lookup table.
///
/// We use this bespoke struct instead of `std.EnumArray` because
/// accessing these slots is very performance critical since it's
/// done for every single print. This benchmarks faster.
pub struct CharsetArray {
    pub g0: charsets::Charset,
    pub g1: charsets::Charset,
    pub g2: charsets::Charset,
    pub g3: charsets::Charset,
}

impl CharsetArray {
    #[inline]
    pub fn get(&self, slot: charsets::Slots) -> charsets::Charset {
        todo!()
    }

    #[inline]
    pub fn set(&mut self, slot: charsets::Slots, charset: charsets::Charset) {
        todo!()
    }
}

pub struct Options {
    pub cols: size::CellCountInt,
    pub rows: size::CellCountInt,

    /// The maximum size of scrollback in bytes. Zero means unlimited. Any
    /// other value will be clamped to support a minimum of the active area.
    pub max_scrollback: usize,

    /// The total storage limit for Kitty images in bytes for this
    /// screen. Kitty image storage is per-screen.
    pub kitty_image_storage_limit: usize,

    /// The limits for what medium types are allowed for Kitty image loading.
    // PORT NOTE: the reference type is
    //   `if (build_options.kitty_graphics) kitty.graphics.LoadingImage.Limits else void`.
    // The reference nests `Limits` under `LoadingImage`; the rustty port carves
    // it as a sibling in `graphics_image`, so this is `graphics_image::Limits`.
    // The conditional `void` (kitty graphics disabled) is the empty/no-limit case.
    pub kitty_image_loading_limits: kitty::graphics_image::Limits,
}

impl Options {
    /// A simple, default terminal. If you rely on specific dimensions or
    /// scrollback (or lack of) then do not use this directly. This is just
    /// for callers that need some defaults.
    pub fn default() -> Options {
        todo!()
    }
}

/// See PageList.Scroll.
pub enum Scroll {
    /// For all of these, see PageList.Scroll.
    Active,
    Top,
    Pin(Pin),
    Row(usize),
    DeltaRow(isize),
    DeltaPrompt(isize),
}

pub struct Resize {
    /// The new size to resize to
    pub cols: size::CellCountInt,
    pub rows: size::CellCountInt,

    /// Whether to reflow soft-wrapped text.
    ///
    /// This will reflow soft-wrapped text. If the screen size is getting
    /// smaller and the maximum scrollback size is exceeded, data will be
    /// lost from the top of the scrollback.
    pub reflow: bool,

    /// Set this to enable prompt redraw on resize. This signals
    /// that the running program can redraw the prompt if the cursor is
    /// currently at a prompt. This detects OSC133 prompts lines and clears
    /// them. If set to `.last`, only the most recent prompt line is cleared.
    pub prompt_redraw: osc::semantic_prompt::Redraw,
}

pub struct SelectionString<'a> {
    /// The selection to convert to a string.
    pub sel: Selection,

    /// If true, trim whitespace around the selection.
    pub trim: bool,

    /// If non-null, a stringmap will be written here. This will use
    /// the same allocator as the call to selectionString. The string will
    /// be duplicated here and in the return value so both must be freed.
    pub map: Option<&'a mut StringMap>,
}

pub struct SelectLine<'a> {
    /// The pin of some part of the line to select.
    pub pin: Pin,

    /// These are the codepoints to consider whitespace to trim
    /// from the ends of the selection.
    pub whitespace: Option<&'a [u32]>,

    /// If true, line selection will consider semantic prompt
    /// state changing a boundary. State changing is ANY state
    /// change.
    pub semantic_prompt_boundary: bool,
}

pub struct LineIterator<'a> {
    pub screen: &'a Screen,
    pub current: Option<Pin>,
}

impl<'a> LineIterator<'a> {
    pub fn next(&mut self) -> Option<Selection> {
        todo!()
    }
}

pub struct PromptClickMove {
    pub left: usize,
    pub right: usize,
}

impl PromptClickMove {
    pub const ZERO: PromptClickMove = PromptClickMove { left: 0, right: 0 };
}

// PORT NOTE: inline anonymous param struct for `cursorCopy`'s `opts`.
pub struct CursorCopyOptions {
    /// Copy the hyperlink from the other cursor. If not set, this will
    /// clear our current hyperlink.
    pub hyperlink: bool,
}

// PORT NOTE: inline anonymous union(enum) param for `cursorSetSemanticContent`'s
// `t` argument. The nested `input` enum is hoisted to `SemanticContentInput`.
pub enum SemanticContentArg {
    Prompt(osc::semantic_prompt::PromptKind),
    Output,
    Input(SemanticContentInput),
}

pub enum SemanticContentInput {
    ClearExplicit,
    ClearEol,
}

// PORT NOTE: inline anonymous param struct for `dumpString`'s `opts`.
pub struct DumpStringOptions {
    /// The start and end points of the dump, both inclusive. The x will
    /// be ignored and the full row will always be dumped.
    pub tl: Pin,
    pub br: Option<Pin>,

    /// If true, this will unwrap soft-wrapped lines. If false, this will
    /// dump the screen as it is visually seen in a rendered window.
    pub unwrap: bool,
}

impl Screen {
    /// Initialize a new screen.
    ///
    /// max_scrollback is the amount of scrollback to keep in bytes. This
    /// will be rounded UP to the nearest page size because our minimum allocation
    /// size is that anyways.
    ///
    /// If max scrollback is 0, then no scrollback is kept at all.
    pub fn init(alloc: Allocator, opts: Options) -> Result<Screen, OutOfMemory> {
        todo!()
    }

    pub fn deinit(&mut self) {
        todo!()
    }

    /// Assert that the screen is in a consistent state. This doesn't check
    /// all pages in the page list because that is SO SLOW even just for
    /// tests. This only asserts the screen specific data so callers should
    /// ensure they're also calling page integrity checks if necessary.
    pub fn assert_integrity(&self) {
        todo!()
    }

    /// Reset the screen according to the logic of a DEC RIS sequence.
    ///
    /// - Clears the screen and attempts to reclaim memory.
    /// - Moves the cursor to the top-left.
    /// - Clears any cursor state: style, hyperlink, etc.
    /// - Resets the charset
    /// - Clears the selection
    /// - Deletes all Kitty graphics
    /// - Resets Kitty Keyboard settings
    /// - Disables protection mode
    ///
    pub fn reset(&mut self) {
        todo!()
    }

    /// Clone the screen.
    ///
    /// This will copy:
    ///
    ///   - Screen dimensions
    ///   - Screen data (cell state, etc.) for the region
    ///
    /// Anything not mentioned above is NOT copied. Some of this is for
    /// very good reason:
    ///
    ///   - Kitty images have a LOT of data. This is not efficient to copy.
    ///     Use a lock and access the image data. The dirty bit is there for
    ///     a reason.
    ///   - Cursor location can be expensive to calculate with respect to the
    ///     specified region. It is faster to grab the cursor from the old
    ///     screen and then move it to the new screen.
    ///   - Current hyperlink cursor state has heap allocations. Since clone
    ///     is only for read-only operations, it is better to not have any
    ///     hyperlink state. Note that already-written hyperlinks are cloned.
    ///
    /// If not mentioned above, then there isn't a specific reason right now
    /// to not copy some data other than we probably didn't need it and it
    /// isn't necessary for screen coherency.
    ///
    /// Other notes:
    ///
    ///   - The viewport will always be set to the active area of the new
    ///     screen. This is the bottom "rows" rows.
    ///   - If the clone region is smaller than a viewport area, blanks will
    ///     be filled in at the bottom.
    ///
    pub fn clone(
        &self,
        alloc: Allocator,
        top: point::Point,
        bot: Option<point::Point>,
    ) -> Result<Screen, OutOfMemory> {
        todo!()
    }

    pub fn increase_capacity(
        &mut self,
        node: *mut pagelist::Node,
        adjustment: Option<pagelist::grow::IncreaseCapacity>,
    ) -> Result<*mut pagelist::Node, pagelist::grow::IncreaseCapacityError> {
        todo!()
    }

    #[inline]
    pub fn cursor_cell_right(&mut self, n: size::CellCountInt) -> *mut pagepkg::Cell {
        todo!()
    }

    #[inline]
    pub fn cursor_cell_left(&mut self, n: size::CellCountInt) -> *mut pagepkg::Cell {
        todo!()
    }

    pub fn cursor_cell_end_of_prev(&mut self) -> *mut pagepkg::Cell {
        todo!()
    }

    /// Move the cursor right. This is a specialized function that is very fast
    /// if the caller can guarantee we have space to move right (no wrapping).
    pub fn cursor_right(&mut self, n: size::CellCountInt) {
        todo!()
    }

    /// Move the cursor left.
    pub fn cursor_left(&mut self, n: size::CellCountInt) {
        todo!()
    }

    /// Move the cursor up.
    pub fn cursor_up(&mut self, n: size::CellCountInt) {
        todo!()
    }

    pub fn cursor_row_up(&mut self, n: size::CellCountInt) -> *mut pagepkg::Row {
        todo!()
    }

    /// Move the cursor down.
    pub fn cursor_down(&mut self, n: size::CellCountInt) {
        todo!()
    }

    pub fn cursor_horizontal_absolute(&mut self, x: size::CellCountInt) {
        todo!()
    }

    pub fn cursor_absolute(&mut self, x: size::CellCountInt, y: size::CellCountInt) {
        todo!()
    }

    pub fn cursor_reload(&mut self) {
        todo!()
    }

    pub fn cursor_down_scroll(&mut self) -> Result<(), OutOfMemory> {
        todo!()
    }

    pub fn cursor_scroll_above(&mut self) -> Result<(), OutOfMemory> {
        todo!()
    }

    fn cursor_scroll_above_rotate(&mut self) -> Result<(), OutOfMemory> {
        todo!()
    }

    fn cursor_down_or_scroll(&mut self) -> Result<(), OutOfMemory> {
        todo!()
    }

    fn cursor_change_pin(&mut self, new: Pin) {
        todo!()
    }

    pub fn cursor_copy(
        &mut self,
        other: Cursor,
        opts: CursorCopyOptions,
    ) -> Result<(), OutOfMemory> {
        todo!()
    }

    pub fn cursor_mark_dirty(&mut self) {
        todo!()
    }

    pub fn cursor_reset_wrap(&mut self) {
        todo!()
    }

    /// Scroll the viewport of the terminal grid.
    pub fn scroll(&mut self, behavior: Scroll) {
        todo!()
    }

    /// See PageList.scrollClear. In addition to that, we reset the cursor
    /// to be on top.
    pub fn scroll_clear(&mut self) -> Result<(), OutOfMemory> {
        todo!()
    }

    /// Returns true if the viewport is scrolled to the bottom of the screen.
    pub fn viewport_is_bottom(&self) -> bool {
        todo!()
    }

    /// Erase the region specified by tl and br, inclusive. This will physically
    /// erase the rows meaning the memory will be reclaimed (if the underlying
    /// page is empty) and other rows will be shifted up.
    pub fn erase_history(&mut self, bl: Option<point::Point>) {
        todo!()
    }

    pub fn erase_active(&mut self, y: size::CellCountInt) {
        todo!()
    }

    // Clear the region specified by tl and bl, inclusive. Cleared cells are
    // colored with the current style background color. This will clear all
    // cells in the rows.
    //
    // If protected is true, the protected flag will be respected and only
    // unprotected cells will be cleared. Otherwise, all cells will be cleared.
    pub fn clear_rows(&mut self, tl: point::Point, bl: Option<point::Point>, protected: bool) {
        todo!()
    }

    /// Clear the cells with the blank cell.
    ///
    /// This takes care to handle cleaning up graphemes and styles.
    pub fn clear_cells(&mut self, page: &mut Page, row: &mut Row, cells: &mut [Cell]) {
        todo!()
    }

    pub fn clear_unprotected_cells(&mut self, page: &mut Page, row: &mut Row, cells: &mut [Cell]) {
        todo!()
    }

    pub fn split_cell_boundary(&mut self, x: size::CellCountInt) {
        todo!()
    }

    pub fn blank_cell(&self) -> Cell {
        todo!()
    }

    /// Resize the screen. The rows or cols can be bigger or smaller.
    ///
    /// If this returns an error, the screen is left in a likely garbage state.
    /// It is very hard to undo this operation without blowing up our memory
    /// usage. The only way to recover is to reset the screen. The only way
    /// this really fails is if page allocation is required and fails, which
    /// probably means the system is in trouble anyways. I'd like to improve this
    /// in the future but it is not a priority particularly because this scenario
    /// (resize) is difficult.
    pub fn resize(&mut self, opts: Resize) -> Result<(), OutOfMemory> {
        todo!()
    }

    pub fn set_attribute(&mut self, attr: crate::sgr::Attribute<'_>) -> Result<(), OutOfMemory> {
        todo!()
    }

    pub fn manual_style_update(&mut self) -> Result<(), pagelist::grow::IncreaseCapacityError> {
        todo!()
    }

    fn split_for_capacity(&mut self) -> Result<(), pagelist::grow::IncreaseCapacityError> {
        todo!()
    }

    pub fn append_grapheme(&mut self, cell: *mut Cell, cp: u21) -> Result<(), OutOfMemory> {
        todo!()
    }

    pub fn start_hyperlink(&mut self, uri: &[u8], id_: Option<&[u8]>) -> Result<(), OutOfMemory> {
        todo!()
    }

    fn start_hyperlink_once(&mut self, link: hyperlink::Hyperlink) -> Result<(), OutOfMemory> {
        todo!()
    }

    pub fn end_hyperlink(&mut self) {
        todo!()
    }

    pub fn cursor_set_hyperlink(&mut self) -> Result<(), pagelist::grow::IncreaseCapacityError> {
        todo!()
    }

    pub fn cursor_set_semantic_content(&mut self, t: SemanticContentArg) {
        todo!()
    }

    /// Set the selection to the given selection. If this is a tracked selection
    /// then the screen will take ownership of the selection. If this is untracked
    /// then the screen will convert it to tracked internally. This will automatically
    /// untrack the prior selection (if any).
    ///
    /// Set the selection to null to clear any previous selection.
    ///
    /// This is always recommended over setting `selection` directly. Beyond
    /// managing memory for you, it also performs safety checks that the selection
    /// is always tracked.
    pub fn select(&mut self, sel_: Option<Selection>) -> Result<(), OutOfMemory> {
        todo!()
    }

    /// Same as select(null) but can't fail.
    pub fn clear_selection(&mut self) {
        todo!()
    }

    /// Returns the raw text associated with a selection. This will unwrap
    /// soft-wrapped edges. The returned slice is owned by the caller and allocated
    /// using alloc, not the allocator associated with the screen (unless they match).
    ///
    /// For more flexibility, use a ScreenFormatter directly.
    // PORT NOTE: the reference returns `[:0]const u8` (NUL-terminated); the
    // faithful map is `Vec<u8>` carrying the NUL-termination invariant (never a
    // lossy `String`).
    pub fn selection_string(
        &mut self,
        alloc: Allocator,
        opts: SelectionString<'_>,
    ) -> Result<Vec<u8>, OutOfMemory> {
        todo!()
    }

    /// Select the line under the given point. This will select across soft-wrapped
    /// lines and will omit the leading and trailing whitespace. If the point is
    /// over whitespace but the line has non-whitespace characters elsewhere, the
    /// line will be selected.
    pub fn select_line(&self, opts: SelectLine<'_>) -> Option<Selection> {
        todo!()
    }

    pub fn select_all(&mut self) -> Option<Selection> {
        todo!()
    }

    /// Select the nearest word to start point that is between start_pt and
    /// end_pt (inclusive). Because it selects "nearest" to start point, start
    /// point can be before or after end point.
    ///
    /// The boundary_codepoints parameter should be a slice of u21 codepoints that
    /// mark word boundaries, passed through to selectWord.
    ///
    /// TODO: test this
    pub fn select_word_between(
        &mut self,
        start: Pin,
        end: Pin,
        boundary_codepoints: &[u32],
    ) -> Option<Selection> {
        todo!()
    }

    /// Select the word under the given point. A word is any consecutive series
    /// of characters that are exclusively whitespace or exclusively non-whitespace.
    /// A selection can span multiple physical lines if they are soft-wrapped.
    ///
    /// This will return null if a selection is impossible. The only scenario
    /// this happens is if the point pt is outside of the written screen space.
    ///
    /// The boundary_codepoints parameter should be a slice of u21 codepoints that
    /// mark word boundaries. This is expected to be pre-parsed from the config.
    pub fn select_word(&self, pin: Pin, boundary_codepoints: &[u32]) -> Option<Selection> {
        todo!()
    }

    pub fn select_output(&mut self, pin: Pin) -> Option<Selection> {
        todo!()
    }

    /// Returns an iterator to move through the soft-wrapped lines starting
    /// from pin.
    pub fn line_iterator(&self, start: Pin) -> LineIterator<'_> {
        todo!()
    }

    /// Determine the inputs necessary to move the cursor to the given
    /// click location within a prompt input area.
    ///
    /// If the cursor isn't currently at a prompt input location, this
    /// returns no movement.
    ///
    /// This feature depends on well-behaved OSC133 shell integration. Specifically,
    /// this only moves over designated input areas (OSC 133 B). It is assumed
    /// that the shell will only move the cursor to input cells, so prompt cells
    /// and other blank cells are ignored as part of the movement calculation.
    pub fn prompt_click_move(&mut self, click_pin: Pin) -> PromptClickMove {
        todo!()
    }

    /// Determine the inputs required to move from the cursor to the given
    /// click location. If the cursor isn't currently at a prompt input
    /// location, this will return zero.
    ///
    /// This currently only supports moving a single line.
    fn prompt_click_line(&mut self, click_pin: Pin) -> PromptClickMove {
        todo!()
    }

    pub fn dump_string(
        &self,
        writer: &mut dyn io::Write,
        opts: DumpStringOptions,
    ) -> io::Result<()> {
        todo!()
    }

    /// You should use dumpString, this is a restricted version mostly for
    /// legacy and convenience reasons for unit tests.
    pub fn dump_string_alloc(
        &self,
        alloc: Allocator,
        tl: point::Point,
    ) -> Result<Vec<u8>, OutOfMemory> {
        todo!()
    }

    /// You should use dumpString, this is a restricted version mostly for
    /// legacy and convenience reasons for unit tests.
    pub fn dump_string_alloc_unwrapped(
        &self,
        alloc: Allocator,
        tl: point::Point,
    ) -> Result<Vec<u8>, OutOfMemory> {
        todo!()
    }

    /// This is basically a really jank version of Terminal.printString. We
    /// have to reimplement it here because we want a way to print to the screen
    /// to test it but don't want all the features of Terminal.
    pub fn test_write_string(&mut self, text: &[u8]) -> Result<(), OutOfMemory> {
        todo!()
    }
}
