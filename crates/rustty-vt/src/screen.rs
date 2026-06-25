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
// `splitCellBoundary`'s reference doc block uses `o`-prefixed sub-bullets indented
// under `-` list items; preserved VERBATIM per R1, which trips this style lint.
#![allow(clippy::doc_overindented_list_items)]

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
// const ScreenFormatter = @import("formatter.zig").ScreenFormatter;
use crate::formatter;
use crate::formatter::{Content, ScreenFormatter};
use crate::hyperlink;
use crate::kitty;
use crate::osc;
use crate::page as pagepkg;
use crate::page::u21;
use crate::page::{Cell, Page, Row};
use crate::pagelist::{self, OutOfMemory, PageList, Pin};
use crate::point;
use crate::selection;
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

// PORT NOTE: `const unicode = @import("../unicode/main.zig")`. The width seam
// (`unicode.table.get(c).width`) maps to `rustty_unicode::table().get(c).width`.
use rustty_unicode as unicode;

// PORT NOTE: `const fastmem = @import("../fastmem.zig")`. The in-place
// `rotateOnceR` (used by the cursor-scroll helpers) maps to
// `rustty_util::fastmem::rotateOnceR`.
use rustty_util::fastmem;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticClick {
    None,
    ClickEvents(osc::semantic_prompt::ClickEvents),
    Cl(osc::semantic_prompt::Click),
}

/// The cursor position and style.
// PORT NOTE: the reference `Cursor` is a Zig value type passed/copied by value
// (e.g. `cursorCopy(other: Cursor, ...)`, `const old = self.cursor`). `Copy`
// mirrors that shallow bit-copy: `page_pin`/`page_row`/`page_cell` are
// non-owning raw back-references and `hyperlink` is a raw `*mut` whose box
// ownership is tracked explicitly by `endHyperlink`/`deinit` (a copy never
// double-frees because `cursorCopy` clears the copy's hyperlink before use).
#[derive(Clone, Copy)]
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
        // if (self.hyperlink) |link| {
        //     link.deinit(alloc);
        //     alloc.destroy(link);
        // }
        // PORT NOTE: the reference frees the heap-allocated `*Hyperlink` via
        // `link.deinit(alloc)` + `alloc.destroy(link)`. In Rust the link is owned
        // through a `*mut Hyperlink` produced by `Box::into_raw`; reclaiming the
        // `Box` here calls `Hyperlink::deinit` (a no-op; the inner `Vec`s drop on
        // their own) and frees the box, mirroring deinit-then-destroy.
        let _ = alloc;
        if let Some(link) = self.hyperlink.take() {
            let boxed = unsafe { Box::from_raw(link) };
            boxed.deinit();
            drop(boxed);
        }
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
#[derive(Clone)]
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
#[derive(Clone)]
pub struct CharsetArray {
    pub g0: charsets::Charset,
    pub g1: charsets::Charset,
    pub g2: charsets::Charset,
    pub g3: charsets::Charset,
}

impl Default for CharsetArray {
    // PORT NOTE: reference field defaults are all `.utf8`.
    fn default() -> CharsetArray {
        CharsetArray {
            g0: charsets::Charset::Utf8,
            g1: charsets::Charset::Utf8,
            g2: charsets::Charset::Utf8,
            g3: charsets::Charset::Utf8,
        }
    }
}

impl Default for CharsetState {
    // PORT NOTE: reference field defaults are `charsets = .{}`, `gl = .G0`,
    // `gr = .G2`, `single_shift = null`.
    fn default() -> CharsetState {
        CharsetState {
            charsets: CharsetArray::default(),
            gl: charsets::Slots::G0,
            gr: charsets::Slots::G2,
            single_shift: None,
        }
    }
}

impl CharsetArray {
    #[inline]
    pub fn get(&self, slot: charsets::Slots) -> charsets::Charset {
        match slot {
            charsets::Slots::G0 => self.g0,
            charsets::Slots::G1 => self.g1,
            charsets::Slots::G2 => self.g2,
            charsets::Slots::G3 => self.g3,
        }
    }

    #[inline]
    pub fn set(&mut self, slot: charsets::Slots, charset: charsets::Charset) {
        match slot {
            charsets::Slots::G0 => self.g0 = charset,
            charsets::Slots::G1 => self.g1 = charset,
            charsets::Slots::G2 => self.g2 = charset,
            charsets::Slots::G3 => self.g3 = charset,
        }
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
        // pub const default: Options = .{
        //     .cols = 80,
        //     .rows = 24,
        //     .max_scrollback = 0,
        // };
        // PORT NOTE: the reference `Options.default` only sets cols/rows/
        // max_scrollback; the remaining fields take their struct-default values
        // (kitty_image_storage_limit per artifact; kitty_image_loading_limits
        // `.direct`). The kitty graphics fields are the deferred placeholders
        // (kitty graphics is DEFERRED to vt-12), so they default trivially here.
        Options {
            cols: 80,
            rows: 24,
            max_scrollback: 0,
            kitty_image_storage_limit: 0,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        }
    }
}

/// Options for scrolling the viewport of the terminal grid. The reason
/// we have this in addition to PageList.Scroll is because we have additional
/// scroll behaviors that are not part of the PageList.Scroll enum.
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

// const selectionString_tw = tripwire.module(enum {
//     copy_map,
// }, selectionString);
//
// PORT NOTE: the Zig `tripwire.module(P, E)` is a comptime function returning a
// type with type-static state. The Rust `rustty_util::tripwire::Module<P, E>` is
// an INSTANCE holding the same state. To share that state between
// `selectionString` (which calls `check`) and the `selectionString map
// allocation failure cleanup` test (which calls `error_always`), we hold the
// module in a `#[cfg(test)] thread_local!` and route access through the small
// `cfg(test)` helper below. In non-test builds the helper is a no-op returning
// `Ok(())`, mirroring the Zig `enabled = builtin.is_test` gate. (Reuses the
// pagelist `init_tw` tripwire idiom.)
#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum SelectionStringFp {
    CopyMap,
}

#[cfg(test)]
thread_local! {
    pub(crate) static SELECTION_STRING_TW:
        core::cell::RefCell<rustty_util::tripwire::Module<SelectionStringFp, OutOfMemory>> =
        core::cell::RefCell::new(rustty_util::tripwire::module());
}

// `try selectionString_tw.check(point)` for the `selectionString` tripwire
// module. Only compiled in test builds; the call site is `#[cfg(test)]`-gated so
// production `selectionString` is unchanged, mirroring the Zig
// `enabled = builtin.is_test` gate where the non-test `check` is a no-op.
#[cfg(test)]
#[inline]
fn selection_string_tw_check(point: SelectionStringFp) -> Result<(), OutOfMemory> {
    SELECTION_STRING_TW.with(|tw| tw.borrow_mut().check(point))
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
        let current = self.current?;
        let result = match self.screen.select_line(SelectLine {
            pin: current,
            whitespace: None,
            semantic_prompt_boundary: false,
        }) {
            Some(r) => r,
            None => {
                self.current = None;
                return None;
            }
        };

        self.current = result.end().down(1);
        Some(result)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PromptClickMove {
    pub left: usize,
    pub right: usize,
}

impl PromptClickMove {
    pub const ZERO: PromptClickMove = PromptClickMove { left: 0, right: 0 };
}

// PORT NOTE: the error set for `startHyperlinkOnce` is the reference
// `(Allocator.Error || Page.InsertHyperlinkError)`. Ported as an explicit enum
// uniting the system-OOM case with the page insert-error cases so the
// `startHyperlink` retry loop can discriminate (it adjusts page capacities for
// the `Insert` cases and treats `OutOfMemory` as fatal).
enum StartHyperlinkOnceError {
    OutOfMemory,
    Insert(crate::page::InsertHyperlinkError),
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
        // Initialize our backing pages.
        // var pages = try PageList.init(alloc, opts.cols, opts.rows, opts.max_scrollback);
        // errdefer pages.deinit();
        // PORT NOTE: the reference `PageList.init` takes `?usize` and Zig coerces
        // the `usize` `opts.max_scrollback` into `Some(..)`; `0` therefore becomes
        // `Some(0)` (the no-scrollback case is captured by `no_scrollback` below).
        let mut pages = PageList::init(opts.cols, opts.rows, Some(opts.max_scrollback))?;

        // Create our tracked pin for the cursor.
        // const page_pin = try pages.trackPin(.{ .node = pages.pages.first.? });
        // errdefer pages.untrackPin(page_pin);
        let first = pages.pages.first;
        assert!(!first.is_null());
        let page_pin = match pages.track_pin(Pin {
            node: first,
            y: 0,
            x: 0,
            garbage: false,
        }) {
            Ok(p) => p,
            Err(e) => {
                // errdefer pages.deinit();
                pages.deinit();
                return Err(e);
            }
        };
        // const page_rac = page_pin.rowAndCell();
        let page_rac = unsafe { (*page_pin).row_and_cell() };

        let result: Screen = Screen {
            alloc,
            pages,
            no_scrollback: opts.max_scrollback == 0,
            cursor: Cursor {
                x: 0,
                y: 0,
                cursor_style: CursorStyle::Block,
                pending_wrap: false,
                protected: false,
                style: style::Style::default(),
                style_id: style::DEFAULT_ID,
                hyperlink_id: 0,
                hyperlink_implicit_id: 0,
                hyperlink: None,
                semantic_content: pagepkg::SemanticContent::Output,
                semantic_content_clear_eol: false,
                page_pin,
                page_row: page_rac.row,
                page_cell: page_rac.cell,
            },
            saved_cursor: None,
            selection: None,
            charset: CharsetState::default(),
            protected_mode: ansi::ProtectedMode::Off,
            kitty_keyboard: kitty::KeyFlagStack::default(),
            // PORT NOTE: the reference here calls
            //   result.kitty_images.setLimit(alloc, &result, opts.kitty_image_storage_limit)
            //   result.kitty_images.image_limits = opts.kitty_image_loading_limits;
            // inside `if (comptime build_options.kitty_graphics)`. The kitty
            // graphics `ImageStorage` is DEFERRED to vt-12 (the field is the
            // `graphics_storage::Deferred` placeholder), so the limit/init wiring
            // lands with it. The placeholder is constructed trivially here.
            kitty_images: kitty::graphics_storage::Deferred,
            semantic_prompt: SemanticPrompt::DISABLED,
            dirty: Dirty::default(),
        };

        Ok(result)
    }

    pub fn deinit(&mut self) {
        // PORT NOTE: the reference here calls
        //   self.kitty_images.deinit(self.alloc, self);
        // inside `if (comptime build_options.kitty_graphics)`. The kitty graphics
        // `ImageStorage.deinit` is DEFERRED to vt-12 (the field is the
        // `graphics_storage::Deferred` placeholder).
        self.cursor.deinit(self.alloc);
        self.pages.deinit();
    }

    /// Assert that the screen is in a consistent state. This doesn't check
    /// all pages in the page list because that is SO SLOW even just for
    /// tests. This only asserts the screen specific data so callers should
    /// ensure they're also calling page integrity checks if necessary.
    pub fn assert_integrity(&self) {
        // if (build_options.slow_runtime_safety) { ... }
        // PORT NOTE: gated on `build_options.slow_runtime_safety`
        // (`crate::options::SLOW_RUNTIME_SAFETY`, i.e. debug builds). The Valgrind
        // short-circuit (`std.valgrind.runningOnValgrind() > 0`) has no Rust
        // analogue here, so it is elided (we never run under Valgrind).
        if crate::options::SLOW_RUNTIME_SAFETY {
            debug_assert!(self.cursor.x < self.pages.cols);
            debug_assert!(self.cursor.y < self.pages.rows);

            // Our cursor x/y should always match the pin. If this doesn't
            // match then it indicates that the tracked pin moved and we didn't
            // account for it by either calling cursorReload or manually
            // adjusting.
            let pt: point::Point = self
                .pages
                .point_from_pin(point::Tag::Active, unsafe { *self.cursor.page_pin })
                .expect("pin points outside active area");
            debug_assert!(self.cursor.x == pt.coord().x);
            debug_assert!(self.cursor.y as u32 == pt.coord().y);
        }
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
        // Reset our pages
        self.pages.reset();

        // The above reset preserves tracked pins so we can still use
        // our cursor pin, which should be at the top-left already.
        let cursor_pin: *mut Pin = self.cursor.page_pin;
        debug_assert!(std::ptr::eq(
            unsafe { (*cursor_pin).node },
            self.pages.pages.first
        ));
        debug_assert!(unsafe { (*cursor_pin).x } == 0);
        debug_assert!(unsafe { (*cursor_pin).y } == 0);
        let cursor_rac = unsafe { (*cursor_pin).row_and_cell() };
        self.cursor.deinit(self.alloc);
        self.cursor = Cursor {
            x: 0,
            y: 0,
            cursor_style: CursorStyle::Block,
            pending_wrap: false,
            protected: false,
            style: style::Style::default(),
            style_id: style::DEFAULT_ID,
            hyperlink_id: 0,
            hyperlink_implicit_id: 0,
            hyperlink: None,
            semantic_content: pagepkg::SemanticContent::Output,
            semantic_content_clear_eol: false,
            page_pin: cursor_pin,
            page_row: cursor_rac.row,
            page_cell: cursor_rac.cell,
        };

        // PORT NOTE: the reference here resets kitty graphics storage inside
        //   if (comptime build_options.kitty_graphics) {
        //       self.kitty_images.deinit(self.alloc, self);
        //       self.kitty_images = .{ .dirty = true };
        //   }
        // The kitty graphics `ImageStorage` is DEFERRED to vt-12 (the field is the
        // `graphics_storage::Deferred` placeholder), so this lands with it.

        // Reset our basic state
        self.saved_cursor = None;
        self.charset = CharsetState::default();
        self.kitty_keyboard = kitty::KeyFlagStack::default();
        self.protected_mode = ansi::ProtectedMode::Off;
        self.semantic_prompt = SemanticPrompt::DISABLED;
        self.clear_selection();
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
        // Create a tracked pin remapper for our selection and cursor. Note
        // that we may want to expose this generally in the future but at the
        // time of doing this we don't need to.
        //
        // PORT NOTE: the reference threads a `*TrackedPinsRemap` into
        // `pages.clone` via `opts.tracked_pins`. The Rust `PageList::clone`
        // takes `&mut Clone` and populates `opts.tracked_pins` in place, so we
        // own the `Clone` opts here and read the populated map back from it. The
        // `pin_remap.deinit()` `defer` in the reference frees the HashMap; the
        // Rust `HashMap` drops on scope exit.
        let mut opts = pagelist::access::Clone {
            top,
            bot,
            tracked_pins: Some(pagelist::access::TrackedPinsRemap::new()),
        };

        // PORT NOTE: `PageList::clone` returns `CloneError` (OutOfMemory |
        // CloneFrom). `Screen::clone` returns `OutOfMemory` to match the
        // reference's inferred error set; the only failure paths reachable here
        // are allocator failures, so we map any clone error into `OutOfMemory`.
        let mut pages = self.pages.clone(&mut opts).map_err(|_| OutOfMemory)?;
        // errdefer pages.deinit();

        let pin_remap = opts.tracked_pins.as_ref().unwrap();

        // Find our cursor. If the cursor isn't in the cloned area, we move it
        // to the top-left arbitrarily because a screen must have SOME cursor.
        let cursor: Cursor = 'cursor: {
            if let Some(&p) = pin_remap.get(&self.cursor.page_pin) {
                'remap: {
                    let page_rac = unsafe { (*p).row_and_cell() };
                    let pt = match pages
                        .point_from_pin(point::Tag::Active, unsafe { (*p).pin_copy() })
                    {
                        Some(pt) => pt,
                        None => break 'remap,
                    };
                    break 'cursor self.clone_cursor(
                        pt.coord().x as size::CellCountInt,
                        pt.coord().y as size::CellCountInt,
                        p,
                        page_rac.row,
                        page_rac.cell,
                    );
                }
            }

            let page_pin = pages.track_pin(Pin {
                node: pages.pages.first,
                x: 0,
                y: 0,
                garbage: false,
            })?;
            let page_rac = unsafe { (*page_pin).row_and_cell() };
            self.clone_cursor(0, 0, page_pin, page_rac.row, page_rac.cell)
        };

        // Preserve our selection if we have one.
        let sel: Option<Selection> = if let Some(sel) = self.selection {
            'sel: {
                debug_assert!(sel.tracked());

                struct Ordered {
                    tl: *mut Pin,
                    br: *mut Pin,
                }

                let (tl_pin, br_pin) = match sel.bounds {
                    selection::Bounds::Tracked { start, end } => (start, end),
                    // PORT NOTE: the `tracked()` assert above guarantees this is a
                    // tracked selection; the untracked arm is unreachable.
                    selection::Bounds::Untracked { .. } => unreachable!(),
                };

                let ordered: Ordered = match sel.order(self) {
                    selection::Order::Forward | selection::Order::MirroredForward => Ordered {
                        tl: tl_pin,
                        br: br_pin,
                    },
                    selection::Order::Reverse | selection::Order::MirroredReverse => Ordered {
                        tl: br_pin,
                        br: tl_pin,
                    },
                };

                let start_pin: *mut Pin = match pin_remap.get(&ordered.tl) {
                    Some(&p) => p,
                    None => 'start: {
                        // No start means it is outside the cloned area.

                        // If we have no end pin then either
                        // (1) our whole selection is outside the cloned area or
                        // (2) our cloned area is within the selection
                        if pin_remap.get(&ordered.br).is_none() {
                            // We check if the selection bottom right pin is above
                            // the cloned area or if the top left pin is below the
                            // cloned area, in either of these cases it means that
                            // the selection is fully out of bounds, so we have no
                            // selection in the cloned area and break out now.
                            let clone_top = match self.pages.pin(top) {
                                Some(p) => p,
                                None => break 'sel None,
                            };
                            let clone_top_y = self
                                .pages
                                .point_from_pin(point::Tag::Screen, clone_top)
                                .unwrap()
                                .coord()
                                .y;
                            if self
                                .pages
                                .point_from_pin(point::Tag::Screen, unsafe {
                                    (*ordered.br).pin_copy()
                                })
                                .unwrap()
                                .coord()
                                .y
                                < clone_top_y
                            {
                                break 'sel None;
                            }
                            if self
                                .pages
                                .point_from_pin(point::Tag::Screen, unsafe {
                                    (*ordered.tl).pin_copy()
                                })
                                .unwrap()
                                .coord()
                                .y
                                > clone_top_y
                            {
                                break 'sel None;
                            }
                        }

                        // We move the top pin back in bounds to the top row.
                        break 'start pages.track_pin(Pin {
                            node: pages.pages.first,
                            x: if sel.rectangle {
                                unsafe { (*ordered.tl).x }
                            } else {
                                0
                            },
                            y: 0,
                            garbage: false,
                        })?;
                    }
                };

                // If we got to this point it means that the selection is not
                // fully out of bounds, so we move the bottom right pin back
                // in bounds if it isn't already.
                let end_pin: *mut Pin = match pin_remap.get(&ordered.br) {
                    Some(&p) => p,
                    None => pages.track_pin(Pin {
                        node: pages.pages.last,
                        x: if sel.rectangle {
                            unsafe { (*ordered.br).x }
                        } else {
                            pages.cols - 1
                        },
                        y: unsafe { (*pages.pages.last).data.size.rows } - 1,
                        garbage: false,
                    })?,
                };

                Some(Selection {
                    bounds: selection::Bounds::Tracked {
                        start: start_pin,
                        end: end_pin,
                    },
                    rectangle: sel.rectangle,
                })
            }
        } else {
            None
        };

        let result: Screen = Screen {
            alloc,
            pages,
            no_scrollback: self.no_scrollback,
            cursor,
            selection: sel,
            dirty: self.dirty,
            // PORT NOTE: the reference `result` struct sets only `alloc`,
            // `pages`, `no_scrollback`, `cursor`, `selection`, `dirty`. Every
            // other field takes its Zig struct default (see the `Screen` field
            // declarations): `saved_cursor = null`, `charset = .{}`,
            // `protected_mode = .off`, `kitty_keyboard = .{}`,
            // `kitty_images = .{}`, `semantic_prompt = .{}`. We materialize those
            // defaults explicitly here.
            saved_cursor: None,
            charset: CharsetState::default(),
            protected_mode: ansi::ProtectedMode::Off,
            kitty_keyboard: kitty::KeyFlagStack::default(),
            kitty_images: kitty::graphics_storage::Deferred,
            semantic_prompt: SemanticPrompt::DISABLED,
        };
        result.assert_integrity();
        Ok(result)
    }

    /// Build a cloned `Cursor` from the remapped pin/row/cell pointers,
    /// mirroring the reference's inline cursor construction in `clone`. The
    /// non-pin fields default exactly as the reference (which leaves them at
    /// their Zig `Cursor` struct defaults in the cloned screen).
    //
    // PORT NOTE: extracted into a helper because the reference repeats the same
    // cursor literal in both `clone` branches (remapped vs. top-left fallback);
    // the only differences are x/y and the pin/row/cell pointers.
    fn clone_cursor(
        &self,
        x: size::CellCountInt,
        y: size::CellCountInt,
        page_pin: *mut Pin,
        page_row: *mut pagepkg::Row,
        page_cell: *mut pagepkg::Cell,
    ) -> Cursor {
        Cursor {
            x,
            y,
            cursor_style: CursorStyle::Block,
            pending_wrap: false,
            protected: false,
            style: style::Style::default(),
            style_id: style::DEFAULT_ID,
            hyperlink_id: 0,
            hyperlink_implicit_id: 0,
            hyperlink: None,
            semantic_content: pagepkg::SemanticContent::Output,
            semantic_content_clear_eol: false,
            page_pin,
            page_row,
            page_cell,
        }
    }

    pub fn increase_capacity(
        &mut self,
        node: *mut pagelist::Node,
        adjustment: Option<pagelist::grow::IncreaseCapacity>,
    ) -> Result<*mut pagelist::Node, pagelist::grow::IncreaseCapacityError> {
        // If the page being modified isn't our cursor page then
        // this is a quick operation because we have no additional
        // accounting. We have to do this check here BEFORE calling
        // increaseCapacity because increaseCapacity will update all
        // our tracked pins (including our cursor).
        if !std::ptr::eq(node, unsafe { (*self.cursor.page_pin).node }) {
            return self.pages.increase_capacity(node, adjustment);
        }

        // We're modifying the cursor page. When we increase the
        // capacity below it will be short the ref count on our
        // current style and hyperlink, so we need to init those.
        let new_node = self.pages.increase_capacity(node, adjustment)?;
        let new_page: *mut Page = unsafe { core::ptr::addr_of_mut!((*new_node).data) };

        // Re-add the style, if the page somehow doesn't have enough
        // memory to add it, we emit a warning and gracefully degrade
        // to the default style for the cursor.
        if self.cursor.style_id != style::DEFAULT_ID {
            self.cursor.style_id = match unsafe {
                let base = (*new_page).memory_ptr();
                (*new_page).styles.add(base, self.cursor.style)
            } {
                Ok(id) => id,
                Err(_err) => {
                    // TODO: Should we increase the capacity further in this case?
                    // log.warn("(Screen.increaseCapacity) Failed to add cursor
                    //   style back to page, err={}", .{err});

                    // Reset the cursor style.
                    self.cursor.style = style::Style::default();
                    style::DEFAULT_ID
                }
            };
        }

        // Re-add the hyperlink, if the page somehow doesn't have enough
        // memory to add it, we emit a warning and gracefully degrade to
        // no hyperlink.
        if let Some(link) = self.cursor.hyperlink {
            // So we don't attempt to free any memory in the replaced page.
            self.cursor.hyperlink_id = 0;
            self.cursor.hyperlink = None;

            // Re-add
            if let Err(_err) = self.start_hyperlink_once(unsafe { (*link).clone() }) {
                // TODO: Should we increase the capacity further in this case?
                // log.warn("(Screen.increaseCapacity) Failed to add cursor
                //   hyperlink back to page, err={}", .{err});
            }

            // Remove our old link
            let boxed = unsafe { Box::from_raw(link) };
            boxed.deinit();
            drop(boxed);
        }

        // Reload the cursor information because the pin changed.
        // So our page row/cell and so on are all off.
        self.cursor_reload();

        Ok(new_node)
    }

    #[inline]
    pub fn cursor_cell_right(&mut self, n: size::CellCountInt) -> *mut pagepkg::Cell {
        debug_assert!(self.cursor.x + n < self.pages.cols);
        let cell: *mut pagepkg::Cell = self.cursor.page_cell;
        unsafe { cell.add(n as usize) }
    }

    #[inline]
    pub fn cursor_cell_left(&mut self, n: size::CellCountInt) -> *mut pagepkg::Cell {
        debug_assert!(self.cursor.x >= n);
        let cell: *mut pagepkg::Cell = self.cursor.page_cell;
        unsafe { cell.sub(n as usize) }
    }

    pub fn cursor_cell_end_of_prev(&mut self) -> *mut pagepkg::Cell {
        debug_assert!(self.cursor.y > 0);

        let mut page_pin = unsafe { (*self.cursor.page_pin).up(1) }.unwrap();
        page_pin.x = self.pages.cols - 1;
        let page_rac = page_pin.row_and_cell();
        page_rac.cell
    }

    /// Move the cursor right. This is a specialized function that is very fast
    /// if the caller can guarantee we have space to move right (no wrapping).
    pub fn cursor_right(&mut self, n: size::CellCountInt) {
        debug_assert!(self.cursor.x + n < self.pages.cols);

        let cell: *mut pagepkg::Cell = self.cursor.page_cell;
        self.cursor.page_cell = unsafe { cell.add(n as usize) };
        unsafe { (*self.cursor.page_pin).x += n };
        self.cursor.x += n;

        self.assert_integrity();
    }

    /// Move the cursor left.
    pub fn cursor_left(&mut self, n: size::CellCountInt) {
        debug_assert!(self.cursor.x >= n);

        let cell: *mut pagepkg::Cell = self.cursor.page_cell;
        self.cursor.page_cell = unsafe { cell.sub(n as usize) };
        unsafe { (*self.cursor.page_pin).x -= n };
        self.cursor.x -= n;

        self.assert_integrity();
    }

    /// Move the cursor up.
    ///
    /// Precondition: The cursor is not at the top of the screen.
    pub fn cursor_up(&mut self, n: size::CellCountInt) {
        debug_assert!(self.cursor.y >= n);

        self.cursor.y -= n; // Must be set before cursorChangePin
        let new = unsafe { (*self.cursor.page_pin).up(n as usize) }.unwrap();
        self.cursor_change_pin(new);
        let page_rac = unsafe { (*self.cursor.page_pin).row_and_cell() };
        self.cursor.page_row = page_rac.row;
        self.cursor.page_cell = page_rac.cell;

        self.assert_integrity();
    }

    pub fn cursor_row_up(&mut self, n: size::CellCountInt) -> *mut pagepkg::Row {
        debug_assert!(self.cursor.y >= n);

        let page_pin = unsafe { (*self.cursor.page_pin).up(n as usize) }.unwrap();
        let page_rac = page_pin.row_and_cell();

        self.assert_integrity();
        page_rac.row
    }

    /// Move the cursor down.
    ///
    /// Precondition: The cursor is not at the bottom of the screen.
    pub fn cursor_down(&mut self, n: size::CellCountInt) {
        debug_assert!(self.cursor.y + n < self.pages.rows);

        self.cursor.y += n; // Must be set before cursorChangePin

        // We move the offset into our page list to the next row and then
        // get the pointers to the row/cell and set all the cursor state up.
        let new = unsafe { (*self.cursor.page_pin).down(n as usize) }.unwrap();
        self.cursor_change_pin(new);
        let page_rac = unsafe { (*self.cursor.page_pin).row_and_cell() };
        self.cursor.page_row = page_rac.row;
        self.cursor.page_cell = page_rac.cell;

        self.assert_integrity();
    }

    /// Move the cursor to some absolute horizontal position.
    pub fn cursor_horizontal_absolute(&mut self, x: size::CellCountInt) {
        debug_assert!(x < self.pages.cols);

        unsafe { (*self.cursor.page_pin).x = x };
        let page_rac = unsafe { (*self.cursor.page_pin).row_and_cell() };
        self.cursor.page_cell = page_rac.cell;
        self.cursor.x = x;

        self.assert_integrity();
    }

    /// Move the cursor to some absolute position.
    pub fn cursor_absolute(&mut self, x: size::CellCountInt, y: size::CellCountInt) {
        debug_assert!(x < self.pages.cols);
        debug_assert!(y < self.pages.rows);

        let mut page_pin = if y < self.cursor.y {
            unsafe { (*self.cursor.page_pin).up((self.cursor.y - y) as usize) }.unwrap()
        } else if y > self.cursor.y {
            unsafe { (*self.cursor.page_pin).down((y - self.cursor.y) as usize) }.unwrap()
        } else {
            unsafe { *self.cursor.page_pin }
        };
        page_pin.x = x;
        self.cursor.x = x; // Must be set before cursorChangePin
        self.cursor.y = y;
        self.cursor_change_pin(page_pin);
        let page_rac = unsafe { (*self.cursor.page_pin).row_and_cell() };
        self.cursor.page_row = page_rac.row;
        self.cursor.page_cell = page_rac.cell;

        self.assert_integrity();
    }

    /// Reloads the cursor pointer information into the screen. This is expensive
    /// so it should only be done in cases where the pointers are invalidated
    /// in such a way that its difficult to recover otherwise.
    pub fn cursor_reload(&mut self) {
        // Our tracked pin is ALWAYS accurate, so we derive the active
        // point from the pin. If this returns null it means our pin
        // points outside the active area. In that case, we update the
        // pin to be the top-left.
        let pt: point::Point = match self
            .pages
            .point_from_pin(point::Tag::Active, unsafe { *self.cursor.page_pin })
        {
            Some(pt) => pt,
            None => {
                let pin = self
                    .pages
                    .pin(point::Point::Active(point::Coordinate::default()))
                    .unwrap();
                unsafe { *self.cursor.page_pin = pin };
                self.pages.point_from_pin(point::Tag::Active, pin).unwrap()
            }
        };

        self.cursor.x = pt.coord().x;
        self.cursor.y = pt.coord().y as size::CellCountInt;
        let page_rac = unsafe { (*self.cursor.page_pin).row_and_cell() };
        self.cursor.page_row = page_rac.row;
        self.cursor.page_cell = page_rac.cell;

        // If we have a style, we need to ensure it is in the page because this
        // method may also be called after a page change.
        if self.cursor.style_id != style::DEFAULT_ID {
            if let Err(_err) = self.manual_style_update() {
                // This failure should not happen because manualStyleUpdate
                // handles page splitting, overflow, and more. This should only
                // happen if we're out of RAM. In this case, we'll just degrade
                // gracefully back to the default style.
                // log.err("failed to update style on cursor reload err={}", .{err});
                self.cursor.style = style::Style::default();
                self.cursor.style_id = 0;
            }
        }

        self.assert_integrity();
    }

    /// Scroll the active area and keep the cursor at the bottom of the screen.
    /// This is a very specialized function but it keeps it fast.
    pub fn cursor_down_scroll(&mut self) -> Result<(), OutOfMemory> {
        debug_assert!(self.cursor.y == self.pages.rows - 1);

        // PORT NOTE: the reference here sets `self.kitty_images.dirty = true`
        // inside `if (comptime build_options.kitty_graphics)`. Kitty graphics
        // `ImageStorage` is DEFERRED to vt-12 (the field is the
        // `graphics_storage::Deferred` placeholder).

        // If we have no scrollback, then we shift all our rows instead.
        if self.no_scrollback {
            // If we have a single-row screen, we have no rows to shift
            // so our cursor is in the correct place we just have to clear
            // the cells.
            if self.pages.rows == 1 {
                let page: *mut Page =
                    unsafe { core::ptr::addr_of_mut!((*(*self.cursor.page_pin).node).data) };
                let row = self.cursor.page_row;
                let cells = unsafe { (*page).get_cells(row) };
                self.clear_cells(page, row, cells);
                self.cursor_mark_dirty();
            } else {
                // The call to `eraseRow` will move the tracked cursor pin up by one
                // row, but we don't actually want that, so we keep the old pin and
                // put it back after calling `eraseRow`.
                let old_pin = unsafe { *self.cursor.page_pin };

                // eraseRow will shift everything below it up.
                self.pages
                    .erase_row(point::Point::Active(point::Coordinate::default()))?;

                // Note we don't need to mark anything dirty in this branch
                // because eraseRow will mark all the rotated rows as dirty
                // in the entire page.

                // We don't use `cursorChangePin` here because we aren't
                // actually changing the pin, we're keeping it the same.
                unsafe { *self.cursor.page_pin = old_pin };

                // We do, however, need to refresh the cached page row
                // and cell, because `eraseRow` will have moved the row.
                let page_rac = unsafe { (*self.cursor.page_pin).row_and_cell() };
                self.cursor.page_row = page_rac.row;
                self.cursor.page_cell = page_rac.cell;
            }
        } else {
            let old_pin = unsafe { *self.cursor.page_pin };

            // Grow our pages by one row. The PageList will handle if we need to
            // allocate, prune scrollback, whatever.
            let _ = self.pages.grow()?;

            // We do this all in a block here because referencing this pin
            // after cursorChangePin is unsafe, and we want to keep it out
            // of scope.
            let new_pin = {
                // If our pin page change it means that the page that the pin
                // was on was pruned. In this case, grow() moves the pin to
                // the top-left of the new page. This effectively moves it by
                // one already, we just need to fix up the x value.
                let page_pin =
                    if std::ptr::eq(old_pin.node, unsafe { (*self.cursor.page_pin).node }) {
                        unsafe { (*self.cursor.page_pin).down(1) }.unwrap()
                    } else {
                        let mut pin = unsafe { *self.cursor.page_pin };
                        pin.x = self.cursor.x;
                        pin
                    };

                // These assertions help catch some pagelist math errors. Our
                // x/y should be unchanged after the grow.
                if crate::options::SLOW_RUNTIME_SAFETY {
                    let active = self
                        .pages
                        .point_from_pin(point::Tag::Active, page_pin)
                        .unwrap()
                        .coord();
                    debug_assert!(active.x == self.cursor.x);
                    debug_assert!(active.y == self.cursor.y as u32);
                }

                page_pin
            };
            self.cursor_change_pin(new_pin);
            let page_rac = unsafe { (*self.cursor.page_pin).row_and_cell() };
            self.cursor.page_row = page_rac.row;
            self.cursor.page_cell = page_rac.cell;

            // Our new row is always dirty
            self.cursor_mark_dirty();

            // Clear the new row so it gets our bg color. We only do this
            // if we have a bg color at all.
            if !matches!(self.cursor.style.bg_color, style::Color::None) {
                let page: *mut Page =
                    unsafe { core::ptr::addr_of_mut!((*(*self.cursor.page_pin).node).data) };
                let row = self.cursor.page_row;
                let cells = unsafe { (*page).get_cells(row) };
                self.clear_cells(page, row, cells);
            }
        }

        if self.cursor.style_id != style::DEFAULT_ID {
            // The newly created line needs to be styled according to
            // the bg color if it is set.
            if let Some(blank_cell) = self.cursor.style.bg_cell() {
                let cell_current: *mut pagepkg::Cell = self.cursor.page_cell;
                let cells = unsafe { cell_current.sub(self.cursor.x as usize) };
                unsafe {
                    core::slice::from_raw_parts_mut(cells, self.pages.cols as usize)
                        .fill(blank_cell)
                };
            }
        }

        self.assert_integrity();
        Ok(())
    }

    /// This scrolls the active area at and above the cursor.
    /// The lines below the cursor are not scrolled.
    pub fn cursor_scroll_above(&mut self) -> Result<(), OutOfMemory> {
        // We unconditionally mark the cursor row as dirty here because
        // the cursor always changes page rows inside this function, and
        // when that happens it can mean the text in the old row needs to
        // be re-shaped because the cursor splits runs to break ligatures.
        self.cursor_mark_dirty();

        // If the cursor is on the bottom of the screen, its faster to use
        // our specialized function for that case.
        if self.cursor.y == self.pages.rows - 1 {
            return self.cursor_down_scroll();
        }

        // Logic below assumes we always have at least one row that isn't moving
        debug_assert!(self.cursor.y < self.pages.rows - 1);

        // Explanation:
        //  We don't actually move everything that's at or above the cursor row,
        //  since this would require us to shift up our ENTIRE scrollback, which
        //  would be ridiculously expensive. Instead, we insert a new row at the
        //  end of the pagelist (`grow()`), and move everything BELOW the cursor
        //  DOWN by one row. This has the same practical result but it's a whole
        //  lot cheaper in 99% of cases.

        let old_pin = unsafe { *self.cursor.page_pin };
        if self.pages.grow()?.is_some() {
            self.cursor_scroll_above_rotate()?;
        } else {
            // In this case, it means grow() didn't allocate a new page.

            if std::ptr::eq(
                unsafe { (*self.cursor.page_pin).node },
                self.pages.pages.last,
            ) {
                // If we're on the last page we can do a very fast path because
                // all the rows we need to move around are within a single page.

                // Note: we don't need to call cursorChangePin here because
                // the pin page is the same so there is no accounting to do
                // for styles or any of that.
                debug_assert!(std::ptr::eq(old_pin.node, unsafe {
                    (*self.cursor.page_pin).node
                }));
                let down = unsafe { (*self.cursor.page_pin).down(1) }.unwrap();
                unsafe { *self.cursor.page_pin = down };

                let pin = self.cursor.page_pin;
                let page: *mut Page =
                    unsafe { core::ptr::addr_of_mut!((*(*self.cursor.page_pin).node).data) };

                // Rotate the rows so that the newly created empty row is at the
                // beginning. e.g. [ 0 1 2 3 ] in to [ 3 0 1 2 ].
                let rows = unsafe { (*page).rows.ptr((*page).memory_ptr()) };
                let pin_y = unsafe { (*pin).y } as usize;
                let size_rows = unsafe { (*page).size.rows } as usize;
                fastmem::rotateOnceR(unsafe {
                    core::slice::from_raw_parts_mut(rows.add(pin_y), size_rows - pin_y)
                });

                // Mark the whole page as dirty.
                //
                // Technically we only need to mark from the cursor row to the
                // end but this is a hot function, so we want to minimize work.
                unsafe { (*page).dirty = true };

                // Setup our cursor caches after the rotation so it points to the
                // correct data
                let page_rac = unsafe { (*self.cursor.page_pin).row_and_cell() };
                self.cursor.page_row = page_rac.row;
                self.cursor.page_cell = page_rac.cell;
            } else {
                // We didn't grow pages but our cursor isn't on the last page.
                // In this case we need to do more work because we need to copy
                // elements between pages.
                //
                // An example scenario of this is shown below:
                //
                //      +----------+ = PAGE 0
                //  ... :          :
                //     +-------------+ ACTIVE
                // 4302 |1A00000000| | 0
                // 4303 |2B00000000| | 1
                //      :^         : : = PIN 0
                // 4304 |3C00000000| | 2
                //      +----------+ :
                //      +----------+ : = PAGE 1
                //    0 |4D00000000| | 3
                //    1 |5E00000000| | 4
                //      +----------+ :
                //     +-------------+
                self.cursor_scroll_above_rotate()?;
            }
        }

        if self.cursor.style_id != style::DEFAULT_ID {
            // The newly created line needs to be styled according to
            // the bg color if it is set.
            if let Some(blank_cell) = self.cursor.style.bg_cell() {
                let cell_current: *mut pagepkg::Cell = self.cursor.page_cell;
                let cells = unsafe { cell_current.sub(self.cursor.x as usize) };
                unsafe {
                    core::slice::from_raw_parts_mut(cells, self.pages.cols as usize)
                        .fill(blank_cell)
                };
            }
        }

        self.assert_integrity();
        Ok(())
    }

    fn cursor_scroll_above_rotate(&mut self) -> Result<(), OutOfMemory> {
        let down = unsafe { (*self.cursor.page_pin).down(1) }.unwrap();
        self.cursor_change_pin(down);

        // Go through each of the pages following our pin, shift all rows
        // down by one, and copy the last row of the previous page.
        let mut current = self.pages.pages.last;
        assert!(!current.is_null());
        while !std::ptr::eq(current, unsafe { (*self.cursor.page_pin).node }) {
            let prev = unsafe { (*current).prev };
            let prev_page: *mut Page = unsafe { core::ptr::addr_of_mut!((*prev).data) };
            let cur_page: *mut Page = unsafe { core::ptr::addr_of_mut!((*current).data) };
            let prev_rows = unsafe { (*prev_page).rows.ptr((*prev_page).memory_ptr()) };
            let cur_rows = unsafe { (*cur_page).rows.ptr((*cur_page).memory_ptr()) };

            // Rotate the pages down: [ 0 1 2 3 ] => [ 3 0 1 2 ]
            let cur_size_rows = unsafe { (*cur_page).size.rows } as usize;
            fastmem::rotateOnceR(unsafe {
                core::slice::from_raw_parts_mut(cur_rows, cur_size_rows)
            });

            // Copy the last row of the previous page to the top of current.
            let prev_size_rows = unsafe { (*prev_page).size.rows } as usize;
            unsafe {
                (*cur_page)
                    .clone_row_from(prev_page, cur_rows, prev_rows.add(prev_size_rows - 1))
                    .map_err(|_| OutOfMemory)?
            };

            // Mark dirty on the page, since we are dirtying all rows with this.
            unsafe { (*cur_page).dirty = true };

            current = prev;
        }

        // Our current is our cursor page, we need to rotate down from
        // our cursor and clear our row.
        debug_assert!(std::ptr::eq(current, unsafe {
            (*self.cursor.page_pin).node
        }));
        let cur_page: *mut Page = unsafe { core::ptr::addr_of_mut!((*current).data) };
        let cur_rows = unsafe { (*cur_page).rows.ptr((*cur_page).memory_ptr()) };
        let pin_y = unsafe { (*self.cursor.page_pin).y } as usize;
        let cur_size_rows = unsafe { (*cur_page).size.rows } as usize;
        fastmem::rotateOnceR(unsafe {
            core::slice::from_raw_parts_mut(cur_rows.add(pin_y), cur_size_rows - pin_y)
        });
        let row = unsafe { cur_rows.add(pin_y) };
        let cells = unsafe { (*cur_page).get_cells(row) };
        self.clear_cells(cur_page, row, cells);

        // Mark the whole page as dirty.
        //
        // Technically we only need to mark from the cursor row to the
        // end but this is a hot function, so we want to minimize work.
        unsafe { (*cur_page).dirty = true };

        // Setup cursor cache data after all the rotations so our
        // row is valid.
        let page_rac = unsafe { (*self.cursor.page_pin).row_and_cell() };
        self.cursor.page_row = page_rac.row;
        self.cursor.page_cell = page_rac.cell;
        Ok(())
    }

    /// Move the cursor down if we're not at the bottom of the screen. Otherwise
    /// scroll. Currently only used for testing.
    fn cursor_down_or_scroll(&mut self) -> Result<(), OutOfMemory> {
        if self.cursor.y + 1 < self.pages.rows {
            self.cursor_down(1);
            Ok(())
        } else {
            self.cursor_down_scroll()
        }
    }

    /// Always use this to write to cursor.page_pin.*.
    ///
    /// This specifically handles the case when the new pin is on a different
    /// page than the old AND we have a style or hyperlink set. In that case,
    /// we must release our old one and insert the new one, since styles are
    /// stored per-page.
    ///
    /// Note that this can change the cursor pin AGAIN if the process of
    /// setting up our cursor forces a capacity adjustment of the underlying
    /// cursor page, so any references to the page pin should be re-read
    /// from `self.cursor.page_pin` after calling this.
    fn cursor_change_pin(&mut self, new: Pin) {
        // Moving the cursor affects text run splitting (ligatures) so
        // we must mark the old and new page dirty. We do this as long
        // as the pins are not equal
        if !unsafe { (*self.cursor.page_pin).eql(new) } {
            self.cursor_mark_dirty();
            new.mark_dirty();
        }

        // If our pin is on the same page, then we can just update the pin.
        // We don't need to migrate any state.
        if std::ptr::eq(unsafe { (*self.cursor.page_pin).node }, new.node) {
            unsafe { *self.cursor.page_pin = new };
            return;
        }

        // If we have an old style then we need to release it from the old page.
        let old_style_: Option<style::Style> = if self.cursor.style_id == style::DEFAULT_ID {
            None
        } else {
            Some(self.cursor.style)
        };
        if old_style_.is_some() {
            // Release the style directly from the old page instead of going through
            // manualStyleUpdate, because the cursor position may have already been
            // updated but the pin has not, which would fail integrity checks.
            let old_page: *mut Page =
                unsafe { core::ptr::addr_of_mut!((*(*self.cursor.page_pin).node).data) };
            unsafe {
                let base = (*old_page).memory_ptr();
                (*old_page).styles.release(base, self.cursor.style_id);
            }
            self.cursor.style = style::Style::default();
            self.cursor.style_id = style::DEFAULT_ID;
        }

        // If we have a hyperlink then we need to release it from the old page.
        if self.cursor.hyperlink.is_some() {
            let old_page: *mut Page =
                unsafe { core::ptr::addr_of_mut!((*(*self.cursor.page_pin).node).data) };
            unsafe {
                let base = (*old_page).memory_ptr();
                (*old_page)
                    .hyperlink_set
                    .release(base, self.cursor.hyperlink_id);
            }
        }

        // Update our pin to the new page
        unsafe { *self.cursor.page_pin = new };

        // On the new page, we need to migrate our style
        if let Some(old_style) = old_style_ {
            self.cursor.style = old_style;
            if let Err(_err) = self.manual_style_update() {
                // This failure should not happen because manualStyleUpdate
                // handles page splitting, overflow, and more. This should only
                // happen if we're out of RAM. In this case, we'll just degrade
                // gracefully back to the default style.
                // log.err("failed to update style on cursor change err={}", .{err});
                self.cursor.style = style::Style::default();
                self.cursor.style_id = 0;
            }
        }

        // On the new page, we need to migrate our hyperlink
        if let Some(link) = self.cursor.hyperlink {
            // So we don't attempt to free any memory in the replaced page.
            self.cursor.hyperlink_id = 0;
            self.cursor.hyperlink = None;

            // Re-add
            let uri = unsafe { (*link).uri.clone() };
            let id_ = match unsafe { &(*link).id } {
                hyperlink::HyperlinkId::Explicit(v) => Some(v.clone()),
                hyperlink::HyperlinkId::Implicit(_) => None,
            };
            if let Err(_err) = self.start_hyperlink(&uri, id_.as_deref()) {
                // This shouldn't happen because startHyperlink should handle
                // resizing. This only happens if we're truly out of RAM. Degrade
                // to forgetting the hyperlink.
                // log.err("failed to update hyperlink on cursor change err={}", .{err});
            }

            // Remove our old link
            let boxed = unsafe { Box::from_raw(link) };
            boxed.deinit();
            drop(boxed);
        }
    }

    /// Copy another cursor. The cursor can be on any screen but the x/y
    /// must be within our screen bounds.
    pub fn cursor_copy(
        &mut self,
        other: Cursor,
        opts: CursorCopyOptions,
    ) -> Result<(), OutOfMemory> {
        debug_assert!(other.x < self.pages.cols);
        debug_assert!(other.y < self.pages.rows);

        // End any currently active hyperlink on our cursor.
        self.end_hyperlink();

        // PORT NOTE: the reference does `const old = self.cursor; self.cursor =
        // other; errdefer self.cursor = old;`. `other` is taken by value
        // (`Cursor` is a plain value type here). We capture the fields of the old
        // cursor we still need (`style_id`, `page_pin`, `x`, `y`) before
        // overwriting, since `Cursor` is not trivially `Copy` (it owns the heap
        // hyperlink pointer). The `errdefer` revert is not reachable in the ports
        // we exercise (the only fallible call below, `manualStyleUpdate`, on
        // failure degrades internally and the reference's `try` would propagate),
        // so we propagate the error after restoring observable state.
        let old_style_id = self.cursor.style_id;
        let old_page_pin = self.cursor.page_pin;
        let old_x = self.cursor.x;
        let old_y = self.cursor.y;

        // Take ownership of `other`'s heap hyperlink (the reference moves the
        // whole struct); we then clear it on `self.cursor` and manage it below.
        let other_hyperlink_id = other.hyperlink_id;
        let other_page_pin = other.page_pin;
        let other_x = other.x;
        let other_y = other.y;

        let mut new_cursor = other;
        // Keep our old style ID so it can be properly cleaned up below.
        new_cursor.style_id = old_style_id;
        // Hyperlinks will be managed separately below.
        new_cursor.hyperlink_id = 0;
        new_cursor.hyperlink = None;
        // Keep our old page pin and X/Y because:
        // 1. The old style will need to be cleaned up from the page it's from.
        // 2. The new position navigated to by `cursorAbsolute` needs to be in our
        //    own screen.
        new_cursor.page_pin = old_page_pin;
        new_cursor.x = old_x;
        new_cursor.y = old_y;

        self.cursor = new_cursor;

        // Call manual style update in order to clean up our old style, if we have
        // one, and also to load the style from the other cursor, if it had one.
        self.manual_style_update().map_err(|_| OutOfMemory)?;

        // Move to the correct location to match the other cursor.
        self.cursor_absolute(other_x, other_y);

        // If the other cursor had a hyperlink, add it to ours.
        if opts.hyperlink && other_hyperlink_id != 0 {
            // Get the hyperlink from the other cursor's page.
            let other_page: *mut Page =
                unsafe { core::ptr::addr_of_mut!((*(*other_page_pin).node).data) };
            let other_link = unsafe {
                let base = (*other_page).memory_ptr();
                (*other_page).hyperlink_set.get(base, other_hyperlink_id)
            };

            let other_base = unsafe { (*other_page).memory_ptr() };
            let uri = unsafe { (*other_link).uri.slice(other_base) }.to_vec();
            let id_: Option<Vec<u8>> = match unsafe { &(*other_link).id } {
                hyperlink::PageEntryId::Explicit(slice) => {
                    Some(unsafe { slice.slice(other_base) }.to_vec())
                }
                hyperlink::PageEntryId::Implicit(_) => None,
            };

            // And it to our cursor.
            if let Err(_err) = self.start_hyperlink(&uri, id_.as_deref()) {
                // This shouldn't happen because startHyperlink should handle
                // resizing. This only happens if we're truly out of RAM. Degrade
                // to forgetting the hyperlink.
                // log.err("failed to update hyperlink on cursor change err={}", .{err});
            }
        }

        Ok(())
    }

    /// Mark the cursor position as dirty.
    /// TODO: test
    #[inline]
    pub fn cursor_mark_dirty(&mut self) {
        unsafe { (*self.cursor.page_row).set_dirty(true) };
    }

    /// Reset the cursor row's soft-wrap state and the cursor's pending wrap.
    /// Also handles clearing the spacer head on the cursor row and resetting
    /// the wrap_continuation flag on the next row if necessary.
    ///
    /// NOTE(qwerasd): This method is not scrolling region aware, and cannot be
    /// since it's on Screen not Terminal. This needs to be addressed down the
    /// line. Not an extremely urgent issue since it's an edge case of an edge
    /// case, but not ideal.
    pub fn cursor_reset_wrap(&mut self) {
        // Reset the cursor's pending wrap state
        self.cursor.pending_wrap = false;

        let page_row = self.cursor.page_row;

        if !unsafe { (*page_row).wrap() } {
            return;
        }

        // This row does not wrap and the next row is not wrapped to
        unsafe { (*page_row).set_wrap(false) };

        if let Some(next_row) = unsafe { (*self.cursor.page_pin).down(1) } {
            unsafe { (*next_row.row_and_cell().row).set_wrap_continuation(false) };
        }

        // If the last cell in the row is a spacer head we need to clear it.
        let cells = unsafe { (*self.cursor.page_pin).cells(pagelist::pin::CellSubset::All) };
        let cols = unsafe { (*(*self.cursor.page_pin).node).data.size.cols } as usize;
        let cell = unsafe { *(cells as *mut pagepkg::Cell).add(cols - 1) };
        if cell.wide() == pagepkg::Wide::SpacerHead {
            let page: *mut Page =
                unsafe { core::ptr::addr_of_mut!((*(*self.cursor.page_pin).node).data) };
            let last = unsafe { (cells as *mut pagepkg::Cell).add(cols - 1) };
            let slice = core::ptr::slice_from_raw_parts_mut(last, 1);
            self.clear_cells(page, page_row, slice);
        }
    }

    /// Scroll the viewport of the terminal grid.
    pub fn scroll(&mut self, behavior: Scroll) {
        // PORT NOTE: the reference here sets `self.kitty_images.dirty = true`
        // inside `if (comptime build_options.kitty_graphics)`. Kitty graphics
        // `ImageStorage` is DEFERRED to vt-12 (the field is the
        // `graphics_storage::Deferred` placeholder).

        match behavior {
            Scroll::Active => self.pages.scroll(pagelist::scroll::Scroll::Active),
            Scroll::Top => self.pages.scroll(pagelist::scroll::Scroll::Top),
            Scroll::Pin(p) => self.pages.scroll(pagelist::scroll::Scroll::Pin(p)),
            Scroll::Row(v) => self.pages.scroll(pagelist::scroll::Scroll::Row(v)),
            Scroll::DeltaRow(v) => self.pages.scroll(pagelist::scroll::Scroll::DeltaRow(v)),
            Scroll::DeltaPrompt(v) => self.pages.scroll(pagelist::scroll::Scroll::DeltaPrompt(v)),
        }

        self.assert_integrity();
    }

    /// See PageList.scrollClear. In addition to that, we reset the cursor
    /// to be on top.
    pub fn scroll_clear(&mut self) -> Result<(), OutOfMemory> {
        self.pages.scroll_clear()?;
        self.cursor_reload();

        // PORT NOTE: the reference here sets `self.kitty_images.dirty = true`
        // inside `if (comptime build_options.kitty_graphics)`. Kitty graphics
        // `ImageStorage` is DEFERRED to vt-12 (the field is the
        // `graphics_storage::Deferred` placeholder).

        self.assert_integrity();
        Ok(())
    }

    /// Returns true if the viewport is scrolled to the bottom of the screen.
    pub fn viewport_is_bottom(&self) -> bool {
        matches!(self.pages.viewport, pagelist::Viewport::Active)
    }

    /// Erase the region specified by tl and br, inclusive. This will physically
    /// erase the rows meaning the memory will be reclaimed (if the underlying
    /// page is empty) and other rows will be shifted up.
    pub fn erase_history(&mut self, bl: Option<point::Point>) {
        // defer self.assertIntegrity();
        // PORT NOTE: Rust has no `defer`; the reference runs `assertIntegrity` at
        // the end, which here is the single exit path.
        self.pages.erase_history(bl);
        self.cursor_reload();
        self.assert_integrity();
    }

    pub fn erase_active(&mut self, y: size::CellCountInt) {
        // defer self.assertIntegrity();
        self.pages.erase_active(y);
        self.cursor_reload();
        self.assert_integrity();
    }

    // Clear the region specified by tl and bl, inclusive. Cleared cells are
    // colored with the current style background color. This will clear all
    // cells in the rows.
    //
    // If protected is true, the protected flag will be respected and only
    // unprotected cells will be cleared. Otherwise, all cells will be cleared.
    pub fn clear_rows(&mut self, tl: point::Point, bl: Option<point::Point>, protected: bool) {
        // defer self.assertIntegrity();
        // PORT NOTE: Rust has no `defer`; we run `assertIntegrity` at the end,
        // the single exit path of this body.

        let cols = self.pages.cols;
        let mut it = self
            .pages
            .page_iterator(pagelist::iter::Direction::RightDown, tl, bl);
        while let Some(chunk) = it.next() {
            // PORT NOTE: `chunk.node` is the `*mut Node`; `&chunk.node.data` is
            // the page. We take a raw `*mut Page` via `addr_of_mut!` (the page
            // lives in the node) and read `chunk.rows()` (a `*mut [Row]`).
            let page: *mut Page = unsafe { core::ptr::addr_of_mut!((*chunk.node).data) };
            let rows = chunk.rows();
            let rows_len = rows.len();
            let rows_base = rows as *mut Row;
            for i in 0..rows_len {
                let row: *mut Row = unsafe { rows_base.add(i) };

                let cells_offset = unsafe { (*row).cells() };
                // const cells_multi: [*]Cell = row.cells.ptr(chunk.node.data.memory);
                let cells_multi: *mut Cell = unsafe { cells_offset.ptr((*page).memory_ptr()) };
                // const cells = cells_multi[0..self.pages.cols];
                let cells: *mut [Cell] =
                    core::ptr::slice_from_raw_parts_mut(cells_multi, cols as usize);

                // Clear all cells
                if protected {
                    self.clear_unprotected_cells(page, row, cells);
                    // We need to preserve other row attributes since we only
                    // cleared unprotected cells.
                    unsafe { (*row).set_cells(cells_offset) };
                } else {
                    self.clear_cells(page, row, cells);
                    // row.* = .{ .cells = cells_offset };
                    // PORT NOTE: the reference resets the entire `Row` to a fresh
                    // value carrying only the `cells` offset (all other row flags
                    // — wrap, grapheme, styled, dirty, etc. — reset to their
                    // zero/false defaults). We mirror that with a zeroed `Row`
                    // whose cells offset is restored.
                    let mut fresh = Row::zeroed();
                    fresh.set_cells(cells_offset);
                    unsafe { *row = fresh };
                }

                unsafe { (*row).set_dirty(true) };
            }
        }

        self.assert_integrity();
    }

    /// Clear the cells with the blank cell.
    ///
    /// This takes care to handle cleaning up graphemes and styles.
    ///
    /// PORT NOTE (Stacked Borrows / Miri): the reference signature is
    /// `clearCells(self, page: *Page, row: *Row, cells: []Cell)` — Zig pointers
    /// and slices are UNPROTECTED. If we mirror it with `&mut Page`/`&mut Row`/
    /// `&mut [Cell]` ARGS, those references are *strongly protected* for the
    /// entire call; the trailing `assert_integrity()` then walks `self.pages`
    /// and re-reads the SAME cell/page memory through a different tag, which
    /// Miri (correctly) rejects as "would remove a strongly-protected tag" UB.
    /// We therefore take RAW pointers (`*mut Page`, `*mut Row`, `*mut [Cell]`),
    /// matching Zig's unprotected semantics, and reconstruct local `&mut`
    /// references ONLY for the brief mutation scopes — never live across an
    /// `assert_integrity()` call.
    #[allow(clippy::not_unsafe_ptr_arg_deref)] // reference takes `page: *Page, row: *Row, cells: []Cell`
    pub fn clear_cells(&mut self, page: *mut Page, row: *mut Row, cells: *mut [Cell]) {
        let cells_base = cells as *mut Cell;
        let cells_len = cells.len();

        // This whole operation does unsafe things, so we just want to assert
        // the end state.
        unsafe { (*page).pause_integrity_checks(true) };

        // PORT NOTE: the reference uses a `defer` block to resume integrity
        // checks + assert at every exit; with a single exit path we run it at
        // the end. (No early returns in this body.)

        if cfg!(debug_assertions) {
            // Our row and cells should be within the page.
            let page_rows = unsafe { (*page).rows.ptr((*page).memory_ptr()) };
            debug_assert!(row as usize >= page_rows as usize);
            debug_assert!(
                row as usize <= unsafe { page_rows.add((*page).size.rows as usize - 1) } as usize
            );

            let row_cells = unsafe { (*page).get_cells(row) };
            let row_cells_base = row_cells as *mut Cell;
            let row_cells_len = row_cells.len();
            debug_assert!(cells_base as usize >= row_cells_base as usize);
            debug_assert!(
                unsafe { cells_base.add(cells_len - 1) } as usize
                    <= unsafe { row_cells_base.add(row_cells_len - 1) } as usize
            );
        }

        // If we have managed memory (styles, graphemes, or hyperlinks)
        // in this row then we go cell by cell and clear them if present.
        if unsafe { (*row).grapheme() } {
            for i in 0..cells_len {
                let cell = unsafe { cells_base.add(i) };
                if unsafe { (*cell).has_grapheme() } {
                    unsafe { (*page).clear_grapheme(cell) };
                }
            }

            // If we have no left/right scroll region we can be sure
            // that we've cleared all the graphemes, so we clear the
            // flag, otherwise we ask the page to update the flag.
            if cells_len == self.pages.cols as usize {
                unsafe { (*row).set_grapheme(false) };
            } else {
                unsafe { (*page).update_row_grapheme_flag(row) };
            }
        }

        if unsafe { (*row).hyperlink() } {
            for i in 0..cells_len {
                let cell = unsafe { cells_base.add(i) };
                if unsafe { (*cell).hyperlink() } {
                    unsafe { (*page).clear_hyperlink(cell) };
                }
            }

            // If we have no left/right scroll region we can be sure
            // that we've cleared all the hyperlinks, so we clear the
            // flag, otherwise we ask the page to update the flag.
            if cells_len == self.pages.cols as usize {
                unsafe { (*row).set_hyperlink(false) };
            } else {
                unsafe { (*page).update_row_hyperlink_flag(row) };
            }
        }

        if unsafe { (*row).styled() } {
            for i in 0..cells_len {
                let cell = unsafe { cells_base.add(i) };
                if unsafe { (*cell).has_styling() } {
                    unsafe {
                        let base = (*page).memory_ptr();
                        (*page).styles.release(base, (*cell).style_id());
                    }
                }
            }

            // If we have no left/right scroll region we can be sure
            // that we've cleared all the styles, so we clear the
            // flag, otherwise we ask the page to update the flag.
            if cells_len == self.pages.cols as usize {
                unsafe { (*row).set_styled(false) };
            } else {
                unsafe { (*page).update_row_styled_flag(row) };
            }
        }

        // PORT NOTE: the reference has a kitty-graphics block here that clears
        // the `kitty_virtual_placeholder` row flag. The kitty graphics feature
        // operates over the deferred `ImageStorage`; the placeholder-clearing
        // logic is row-local and lands with the kitty co-port (vt-12).

        let blank = self.blank_cell();
        // Reconstruct a local `&mut [Cell]` ONLY for the brief fill; it is
        // dropped before any assert_integrity() below.
        {
            let cells_slice: &mut [Cell] =
                unsafe { core::slice::from_raw_parts_mut(cells_base, cells_len) };
            cells_slice.fill(blank);
        }

        // defer { page.pauseIntegrityChecks(false); page.assertIntegrity();
        //   self.assertIntegrity(); }
        unsafe { (*page).pause_integrity_checks(false) };
        unsafe { (*page).assert_integrity() };
        self.assert_integrity();
    }

    /// Clear cells but only if they are not protected.
    ///
    /// PORT NOTE (Stacked Borrows / Miri): like `clear_cells`, the reference
    /// signature `clearUnprotectedCells(self, page: *Page, row: *Row,
    /// cells: []Cell)` uses UNPROTECTED Zig pointers/slices. We take RAW
    /// pointers and read each cell's `protected` flag through the raw pointer,
    /// passing raw sub-slice pointers down to `clear_cells`.
    #[allow(clippy::not_unsafe_ptr_arg_deref)] // reference takes `page: *Page, row: *Row, cells: []Cell`
    pub fn clear_unprotected_cells(&mut self, page: *mut Page, row: *mut Row, cells: *mut [Cell]) {
        let cells_base = cells as *mut Cell;
        let cells_len = cells.len();

        let mut x0: usize = 0;
        let mut x1: usize;

        'clear: while x0 < cells_len {
            while unsafe { (*cells_base.add(x0)).protected() } {
                x0 += 1;
                if x0 >= cells_len {
                    break 'clear;
                }
            }
            x1 = x0 + 1;
            while x1 < cells_len && !unsafe { (*cells_base.add(x1)).protected() } {
                x1 += 1;
            }
            let sub: *mut [Cell] =
                core::ptr::slice_from_raw_parts_mut(unsafe { cells_base.add(x0) }, x1 - x0);
            self.clear_cells(page, row, sub);
            x0 = x1;
        }

        unsafe { (*page).assert_integrity() };
        self.assert_integrity();
    }

    /// Clean up boundary conditions where a cell will become discontiguous with
    /// a neighboring cell because either one of them will be moved and/or cleared.
    ///
    /// For performance reasons this is specialized to operate on the cursor row.
    ///
    /// Handles the boundary between the cell at `x` and the cell at `x - 1`.
    ///
    /// So, for example, when moving a region of cells [a, b] (inclusive), call this
    /// function with `x = a` and `x = b + 1`. It is okay if `x` is out of bounds by
    /// 1, this will be interpreted correctly.
    ///
    /// DOES NOT MODIFY ROW WRAP STATE! See `cursor_reset_wrap` for that.
    ///
    /// The following boundary conditions are handled:
    ///
    /// - `x - 1` is a wide character and `x` is a spacer tail:
    ///   o Both cells will be cleared.
    ///   o If `x - 1` is the start of the row and was wrapped from a previous row
    ///     then the previous row is checked for a spacer head, which is cleared if
    ///     present.
    ///
    /// - `x == 0` and is a wide character:
    ///   o If the row is a wrap continuation then the previous row will be checked
    ///     for a spacer head, which is cleared if present.
    ///
    /// - `x == cols` and `x - 1` is a spacer head:
    ///   o `x - 1` will be cleared.
    ///
    /// NOTE(qwerasd): This method is not scrolling region aware, and cannot be
    /// since it's on Screen not Terminal. This needs to be addressed down the
    /// line. Not an extremely urgent issue since it's an edge case of an edge
    /// case, but not ideal.
    pub fn split_cell_boundary(&mut self, x: size::CellCountInt) {
        // const page = &self.cursor.page_pin.node.data;
        let page: *mut Page =
            unsafe { core::ptr::addr_of_mut!((*(*self.cursor.page_pin).node).data) };

        // PORT NOTE: stash the raw cursor pin/row pointers in locals up front so
        // the `clear_cells(&mut self, ...)` calls below don't reborrow `self`
        // through `self.cursor.*` while `self` is already mutably borrowed by the
        // method receiver (a Rust borrow-checker constraint with no reference
        // analogue). These are non-owning copies of the same raw pointers.
        let cursor_page_pin: *mut Pin = self.cursor.page_pin;
        let cursor_page_row: *mut Row = self.cursor.page_row;

        unsafe { (*page).pause_integrity_checks(true) };
        // defer page.pauseIntegrityChecks(false);
        // PORT NOTE: Rust has no `defer`; this body has multiple early returns,
        // so we resume integrity checks before each `return` and at the natural
        // end. The reference's single `defer` runs at every exit identically.

        // const cols = self.cursor.page_pin.node.data.size.cols;
        let cols = unsafe { (*(*cursor_page_pin).node).data.size.cols };

        // `x` may be up to an INCLUDING `cols`, since that signifies splitting
        // the boundary to the right of the final cell in the row.
        debug_assert!(x <= cols);

        // [ A B C D E F|]
        //              ^ Boundary between final cell and row end.
        if x == cols {
            if !unsafe { (*cursor_page_row).wrap() } {
                unsafe { (*page).pause_integrity_checks(false) };
                return;
            }

            let cells = unsafe { (*cursor_page_pin).cells(pagelist::pin::CellSubset::All) };
            let cells_base = cells as *mut Cell;

            // Spacer head at end of wrapped row.
            if unsafe { (*cells_base.add(cols as usize - 1)).wide() } == pagepkg::Wide::SpacerHead {
                let slice = core::ptr::slice_from_raw_parts_mut(
                    unsafe { cells_base.add(cols as usize - 1) },
                    1,
                );
                self.clear_cells(page, cursor_page_row, slice);
            }

            unsafe { (*page).pause_integrity_checks(false) };
            return;
        }

        // [|A B C D E F ]
        //  ^ Boundary between first cell and row start.
        //
        //  OR
        //
        // [ A|B C D E F ]
        //    ^ Boundary between first cell and second cell.
        //
        // First cell may be a wrapped wide cell with a spacer
        // head on the previous row that needs to be cleared.
        if (x == 0 || x == 1) && unsafe { (*cursor_page_row).wrap_continuation() } {
            let cells = unsafe { (*cursor_page_pin).cells(pagelist::pin::CellSubset::All) };
            let cells_base = cells as *mut Cell;

            // If the first cell in a row is wide the previous row
            // may have a spacer head which needs to be cleared.
            if unsafe { (*cells_base).wide() } == pagepkg::Wide::Wide {
                if let Some(p_row) = unsafe { (*cursor_page_pin).up(1) } {
                    let p_rac = p_row.row_and_cell();
                    let p_cells = p_row.cells(pagelist::pin::CellSubset::All);
                    let p_cells_base = p_cells as *mut Cell;
                    let p_cols = unsafe { (*p_row.node).data.size.cols };
                    let p_cell = unsafe { *p_cells_base.add(p_cols as usize - 1) };
                    if p_cell.wide() == pagepkg::Wide::SpacerHead {
                        let p_page: *mut Page =
                            unsafe { core::ptr::addr_of_mut!((*p_row.node).data) };
                        let slice = core::ptr::slice_from_raw_parts_mut(
                            unsafe { p_cells_base.add(p_cols as usize - 1) },
                            1,
                        );
                        self.clear_cells(p_page, p_rac.row, slice);
                    }
                }
            }
        }

        // If x is 0 then we're done.
        if x == 0 {
            unsafe { (*page).pause_integrity_checks(false) };
            return;
        }

        // [ ... X|Y ... ]
        //        ^ Boundary between two cells in the middle of the row.
        {
            debug_assert!(x > 0);
            debug_assert!(x < cols);

            let cells = unsafe { (*cursor_page_pin).cells(pagelist::pin::CellSubset::All) };
            let cells_base = cells as *mut Cell;

            let left = unsafe { *cells_base.add(x as usize - 1) };
            match left.wide() {
                // There should not be spacer heads in the middle of the row.
                pagepkg::Wide::SpacerHead => unreachable!(),

                // We don't need to do anything for narrow cells or spacer tails.
                pagepkg::Wide::Narrow | pagepkg::Wide::SpacerTail => {}

                // A wide char would be split, so must be cleared.
                pagepkg::Wide::Wide => {
                    let slice = core::ptr::slice_from_raw_parts_mut(
                        unsafe { cells_base.add(x as usize - 1) },
                        2,
                    );
                    self.clear_cells(page, cursor_page_row, slice);
                }
            }
        }

        unsafe { (*page).pause_integrity_checks(false) };
    }

    /// Returns the blank cell to use when doing terminal operations that
    /// require preserving the bg color.
    #[inline]
    pub fn blank_cell(&self) -> Cell {
        if self.cursor.style_id == style::DEFAULT_ID {
            return Cell::zeroed();
        }
        self.cursor.style.bg_cell().unwrap_or_else(Cell::zeroed)
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
        // defer self.assertIntegrity();
        // PORT NOTE: Rust has no `defer`. The reference runs `assertIntegrity`
        // at every exit; we wrap the body in a closure and run the assert after.
        let result = self.resize_inner(opts);
        self.assert_integrity();
        result
    }

    fn resize_inner(&mut self, opts: Resize) -> Result<(), OutOfMemory> {
        // PORT NOTE: the reference here sets `self.kitty_images.dirty = true`
        // inside `if (comptime build_options.kitty_graphics)`. Kitty graphics
        // `ImageStorage` is DEFERRED to vt-12 (the field is the
        // `graphics_storage::Deferred` placeholder).

        // Release the cursor style while resizing just
        // in case the cursor ends up on a different page.
        let cursor_style = self.cursor.style;
        self.cursor.style = style::Style::default();
        self.manual_style_update()
            .expect("manualStyleUpdate to default cannot fail");
        // PORT NOTE: the reference restores the cursor style in a `defer` block.
        // Rust has no `defer`; we run the restore at every exit below by routing
        // the rest of the body through `resize_body` and restoring after.
        let body = self.resize_body(opts, cursor_style);

        // defer { restore cursor style }
        self.cursor.style = cursor_style;
        if self.manual_style_update().is_err() {
            // This failure should not happen because manualStyleUpdate
            // handles page splitting, overflow, and more. This should only
            // happen if we're out of RAM. In this case, we'll just degrade
            // gracefully back to the default style.
            // log.err("failed to update style on cursor reload err={}", .{err});
            self.cursor.style = style::Style::default();
            self.cursor.style_id = 0;
        }

        body
    }

    fn resize_body(
        &mut self,
        opts: Resize,
        _cursor_style: style::Style,
    ) -> Result<(), OutOfMemory> {
        // If we have a hyperlink, release it from the old page
        // and then we need to re-add it to the new page. This needs
        // to happen because resize below typically reallocates a
        // new page so the old hyperlink is invalid.
        let hyperlink_ = self.cursor.hyperlink;
        if self.cursor.hyperlink_id != 0 {
            // Note we do NOT use endHyperlink because we want to keep
            // our allocated self.cursor.hyperlink valid.
            let page: *mut Page =
                unsafe { core::ptr::addr_of_mut!((*(*self.cursor.page_pin).node).data) };
            unsafe {
                let base = (*page).memory_ptr();
                (*page)
                    .hyperlink_set
                    .release(base, self.cursor.hyperlink_id);
            }
            self.cursor.hyperlink_id = 0;
            self.cursor.hyperlink = None;
        }

        // We need to insert a tracked pin for our saved cursor so we can
        // modify its X/Y for reflow.
        let saved_cursor_pin: Option<*mut Pin> = match &self.saved_cursor {
            None => None,
            Some(sc) => {
                match self.pages.pin(point::Point::Active(point::Coordinate {
                    x: sc.x,
                    y: sc.y as u32,
                })) {
                    None => None,
                    Some(pin) => Some(self.pages.track_pin(pin)?),
                }
            }
        };
        // PORT NOTE: the reference `defer if (saved_cursor_pin) |p|
        // self.pages.untrackPin(p);` is run at every exit below via
        // `resize_finish`.
        let result = self.resize_finish(opts, hyperlink_, saved_cursor_pin);
        if let Some(p) = saved_cursor_pin {
            self.pages.untrack_pin(p);
        }
        result
    }

    fn resize_finish(
        &mut self,
        opts: Resize,
        hyperlink_: Option<*mut hyperlink::Hyperlink>,
        saved_cursor_pin: Option<*mut Pin>,
    ) -> Result<(), OutOfMemory> {
        // If our cursor is on a prompt or input line, clear it so the shell can
        // redraw it. This works with OSC 133 semantic prompts.
        //
        // We check cursor.semantic_content rather than page_row.semantic_prompt
        // because some shells (e.g., Nu) mark input areas with OSC 133 B but don't
        // mark continuation lines with k=s. If the input spans multiple lines and
        // continuation lines are unmarked, checking only page_row.semantic_prompt
        // would miss them. By checking semantic_content, we assume that if the
        // cursor is on anything other than command output, we're at a prompt/input
        // line and should clear from there.
        if !matches!(opts.prompt_redraw, osc::semantic_prompt::Redraw::False)
            && !matches!(
                self.cursor.semantic_content,
                pagepkg::SemanticContent::Output
            )
        {
            'prompt: {
                match opts.prompt_redraw {
                    osc::semantic_prompt::Redraw::False => unreachable!(),

                    // For `.last`, only clear the current line where the cursor is.
                    // For `.true`, clear all prompt lines starting from the beginning.
                    osc::semantic_prompt::Redraw::Last => {
                        let page: *mut Page = unsafe {
                            core::ptr::addr_of_mut!((*(*self.cursor.page_pin).node).data)
                        };
                        let row = self.cursor.page_row;
                        let cells = unsafe { (*page).get_cells(row) };
                        self.clear_cells(page, row, cells);
                    }

                    osc::semantic_prompt::Redraw::True => {
                        let start = {
                            let mut it = unsafe { *self.cursor.page_pin }
                                .prompt_iterator(pagelist::iter::Direction::LeftUp, None);
                            match it.next() {
                                Some(p) => p,
                                None => {
                                    // This should never happen because promptIterator
                                    // should always find a prompt if we already verified
                                    // our row is some kind of prompt.
                                    // log.warn("cursor on prompt line but promptIterator
                                    //   found no prompt", .{});
                                    break 'prompt;
                                }
                            }
                        };

                        // Clear cells from our start down. We replace it with spaces,
                        // and do not physically erase the rows (eraseRows) because the
                        // shell is going to expect this space to be available.
                        let mut it = start.row_iterator(pagelist::iter::Direction::RightDown, None);
                        while let Some(pin) = it.next() {
                            let page: *mut Page =
                                unsafe { core::ptr::addr_of_mut!((*pin.node).data) };
                            let row = pin.row_and_cell().row;
                            let cells = unsafe { (*page).get_cells(row) };
                            self.clear_cells(page, row, cells);
                        }
                    }
                }
            }
        }

        // Perform the resize operation.
        self.pages.resize(pagelist::resize::Resize {
            rows: Some(opts.rows),
            cols: Some(opts.cols),
            reflow: opts.reflow,
            cursor: Some(pagelist::resize::Cursor {
                x: self.cursor.x,
                y: self.cursor.y,
                pin: Some(self.cursor.page_pin),
            }),
        })?;

        // If we have no scrollback and we shrunk our rows, we must explicitly
        // erase our history. This is because PageList always keeps at least
        // a page size of history.
        if self.no_scrollback {
            self.pages.erase_history(None);
        }

        // If our cursor was updated, we do a full reload so all our cursor
        // state is correct.
        self.cursor_reload();

        // If we reflowed a saved cursor, update it.
        if let Some(p) = saved_cursor_pin {
            // This should never fail because a non-null saved_cursor_pin
            // implies a non-null saved_cursor.
            let pin_val = unsafe { *p };
            match self.pages.point_from_pin(point::Tag::Active, pin_val) {
                Some(pt) => {
                    let sc = self.saved_cursor.as_mut().unwrap();
                    sc.x = pt.coord().x;
                    sc.y = pt.coord().y as size::CellCountInt;

                    // If we had pending wrap set and we're no longer at the end of
                    // the line, we unset the pending wrap and move the cursor to
                    // reflect the correct next position.
                    if sc.pending_wrap && sc.x != opts.cols - 1 {
                        sc.pending_wrap = false;
                        sc.x += 1;
                    }
                }
                None => {
                    // I think this can happen if the screen is resized to be
                    // less rows or less cols and our saved cursor moves outside
                    // the active area. In this case, there isn't anything really
                    // reasonable we can do so we just move the cursor to the
                    // top-left. It may be reasonable to also move the cursor to
                    // match the primary cursor. Any behavior is fine since this is
                    // totally unspecified.
                    let sc = self.saved_cursor.as_mut().unwrap();
                    sc.x = 0;
                    sc.y = 0;
                    sc.pending_wrap = false;
                }
            }
        }

        // Fix up our hyperlink if we had one.
        if let Some(link) = hyperlink_ {
            let uri = unsafe { (*link).uri.clone() };
            let id_ = match unsafe { &(*link).id } {
                hyperlink::HyperlinkId::Explicit(v) => Some(v.clone()),
                hyperlink::HyperlinkId::Implicit(_) => None,
            };
            if let Err(_err) = self.start_hyperlink(&uri, id_.as_deref()) {
                // This shouldn't happen because startHyperlink should handle
                // resizing. This only happens if we're truly out of RAM. Degrade
                // to forgetting the hyperlink.
                // log.err("failed to update hyperlink on resize err={}", .{err});
            }

            // Remove our old link
            let boxed = unsafe { Box::from_raw(link) };
            boxed.deinit();
            drop(boxed);
        }

        Ok(())
    }

    /// Set a style attribute for the current cursor.
    ///
    /// If the style can't be set due to any internal errors (memory-related),
    /// then this will revert back to the existing style and return an error.
    pub fn set_attribute(
        &mut self,
        attr: crate::sgr::Attribute<'_>,
    ) -> Result<(), pagelist::grow::IncreaseCapacityError> {
        use crate::sgr::Attribute;
        use crate::style::Color;

        // If we fail to set our style for any reason, we should revert
        // back to the old style. If we fail to do that, we revert back to
        // the default style.
        let old_style = self.cursor.style;
        // PORT NOTE: this closure mirrors the reference `errdefer` block. Rust
        // has no `errdefer`, so we invoke it on the error path below.
        let revert = |s: &mut Screen| {
            s.cursor.style = old_style;
            if s.manual_style_update().is_err() {
                // log.warn("setAttribute error restoring old style after failure ...");
                s.cursor.style = style::Style::default();
                s.manual_style_update()
                    .expect("manualStyleUpdate to default cannot fail");
            }
        };

        match attr {
            Attribute::Unset => {
                self.cursor.style = style::Style::default();
            }

            Attribute::Bold => {
                self.cursor.style.flags.bold = true;
            }

            Attribute::ResetBold => {
                // Bold and faint share the same SGR code for this
                self.cursor.style.flags.bold = false;
                self.cursor.style.flags.faint = false;
            }

            Attribute::Italic => {
                self.cursor.style.flags.italic = true;
            }

            Attribute::ResetItalic => {
                self.cursor.style.flags.italic = false;
            }

            Attribute::Faint => {
                self.cursor.style.flags.faint = true;
            }

            Attribute::Underline(v) => {
                self.cursor.style.flags.underline = v;
            }

            Attribute::UnderlineColor(rgb) => {
                self.cursor.style.underline_color = Color::Rgb(crate::color::RGB {
                    r: rgb.r,
                    g: rgb.g,
                    b: rgb.b,
                });
            }

            Attribute::Color256Underline(idx) => {
                self.cursor.style.underline_color = Color::Palette(idx);
            }

            Attribute::ResetUnderlineColor => {
                self.cursor.style.underline_color = Color::None;
            }

            Attribute::Overline => {
                self.cursor.style.flags.overline = true;
            }

            Attribute::ResetOverline => {
                self.cursor.style.flags.overline = false;
            }

            Attribute::Blink => {
                self.cursor.style.flags.blink = true;
            }

            Attribute::ResetBlink => {
                self.cursor.style.flags.blink = false;
            }

            Attribute::Inverse => {
                self.cursor.style.flags.inverse = true;
            }

            Attribute::ResetInverse => {
                self.cursor.style.flags.inverse = false;
            }

            Attribute::Invisible => {
                self.cursor.style.flags.invisible = true;
            }

            Attribute::ResetInvisible => {
                self.cursor.style.flags.invisible = false;
            }

            Attribute::Strikethrough => {
                self.cursor.style.flags.strikethrough = true;
            }

            Attribute::ResetStrikethrough => {
                self.cursor.style.flags.strikethrough = false;
            }

            Attribute::DirectColorFg(rgb) => {
                self.cursor.style.fg_color = Color::Rgb(crate::color::RGB {
                    r: rgb.r,
                    g: rgb.g,
                    b: rgb.b,
                });
            }

            Attribute::DirectColorBg(rgb) => {
                self.cursor.style.bg_color = Color::Rgb(crate::color::RGB {
                    r: rgb.r,
                    g: rgb.g,
                    b: rgb.b,
                });
            }

            Attribute::Color8Fg(n) => {
                self.cursor.style.fg_color = Color::Palette(n.cval());
            }

            Attribute::Color8Bg(n) => {
                self.cursor.style.bg_color = Color::Palette(n.cval());
            }

            Attribute::ResetFg => self.cursor.style.fg_color = Color::None,

            Attribute::ResetBg => self.cursor.style.bg_color = Color::None,

            Attribute::Color8BrightFg(n) => {
                self.cursor.style.fg_color = Color::Palette(n.cval());
            }

            Attribute::Color8BrightBg(n) => {
                self.cursor.style.bg_color = Color::Palette(n.cval());
            }

            Attribute::Color256Fg(idx) => {
                self.cursor.style.fg_color = Color::Palette(idx);
            }

            Attribute::Color256Bg(idx) => {
                self.cursor.style.bg_color = Color::Palette(idx);
            }

            Attribute::Unknown(_) => return Ok(()),
        }

        match self.manual_style_update() {
            Ok(()) => Ok(()),
            Err(e) => {
                revert(self);
                Err(e)
            }
        }
    }

    /// Call this whenever you manually change the cursor style.
    ///
    /// This function can NOT fail if the cursor style is changing to the
    /// default style.
    ///
    /// If this returns an error, the style change did not take effect and
    /// the cursor style is reverted back to the default. The only scenario
    /// this returns an error is if there is a physical memory allocation failure
    /// or if there is no possible way to increase style capacity to store
    /// the style.
    ///
    /// This function WILL split pages as necessary to accommodate the new style.
    /// So if OutOfSpace is returned, it means that even after splitting the page
    /// there was still no room for the new style.
    pub fn manual_style_update(&mut self) -> Result<(), pagelist::grow::IncreaseCapacityError> {
        use crate::pagelist::grow::{IncreaseCapacity, IncreaseCapacityError};
        use crate::ref_counted_set::AddError;

        let mut page: *mut Page =
            unsafe { core::ptr::addr_of_mut!((*(*self.cursor.page_pin).node).data) };

        // std.log.warn("active styles={}", .{page.styles.count()});

        // Release our previous style if it was not default.
        if self.cursor.style_id != style::DEFAULT_ID {
            unsafe {
                let base = (*page).memory_ptr();
                (*page).styles.release(base, self.cursor.style_id);
            }
        }

        // If our new style is the default, just reset to that
        if self.cursor.style.default_style() {
            self.cursor.style_id = style::DEFAULT_ID;
            self.assert_integrity();
            return Ok(());
        }

        // Clear the cursor style ID to prevent weird things from happening
        // if the page capacity has to be adjusted which would end up calling
        // manual_style_update again.
        //
        // This also ensures that if anything fails below, we fall back to
        // clearing our style.
        self.cursor.style_id = style::DEFAULT_ID;

        // After setting the style, we need to update our style map.
        // Note that we COULD lazily do this in print. We should look into
        // if that makes a meaningful difference. Our priority is to keep print
        // fast because setting a ton of styles that do nothing is uncommon
        // and weird.
        let add_result = unsafe {
            let base = (*page).memory_ptr();
            (*page).styles.add(base, self.cursor.style)
        };
        let id = match add_result {
            Ok(id) => id,
            Err(err) => {
                // Our style map is full or needs to be rehashed, so we need to
                // increase style capacity (or rehash).
                let node = match self.increase_capacity(
                    unsafe { (*self.cursor.page_pin).node },
                    match err {
                        AddError::OutOfMemory => Some(IncreaseCapacity::Styles),
                        AddError::NeedsRehash => None,
                    },
                ) {
                    Ok(node) => node,
                    Err(IncreaseCapacityError::OutOfMemory) => {
                        self.assert_integrity();
                        return Err(IncreaseCapacityError::OutOfMemory);
                    }
                    Err(IncreaseCapacityError::OutOfSpace) => {
                        // Out of space, we need to split the page. Split wherever
                        // is using less capacity and hope that works. If it doesn't
                        // work, we tried.
                        // PORT NOTE: the reference `try self.splitForCapacity(..)`
                        // unifies `SplitError` into `IncreaseCapacityError` (the
                        // two error sets share `{OutOfMemory, OutOfSpace}`); we map
                        // explicitly.
                        self.split_for_capacity(unsafe { *self.cursor.page_pin })
                            .map_err(|e| match e {
                                pagelist::grow::SplitError::OutOfMemory => {
                                    IncreaseCapacityError::OutOfMemory
                                }
                                pagelist::grow::SplitError::OutOfSpace => {
                                    IncreaseCapacityError::OutOfSpace
                                }
                            })?;
                        unsafe { (*self.cursor.page_pin).node }
                    }
                };

                page = unsafe { core::ptr::addr_of_mut!((*node).data) };
                match unsafe {
                    let base = (*page).memory_ptr();
                    (*page).styles.add(base, self.cursor.style)
                } {
                    Ok(id) => id,
                    Err(AddError::OutOfMemory) => {
                        // This shouldn't happen because increaseCapacity is
                        // guaranteed to increase our capacity by at least one and
                        // we only need one space, but again, I don't want to crash
                        // here so let's log loudly and reset.
                        // log.err("style addition failed after capacity increase", .{});
                        self.assert_integrity();
                        return Err(IncreaseCapacityError::OutOfMemory);
                    }
                    Err(AddError::NeedsRehash) => {
                        // This should be impossible because we rehash above
                        // and rehashing should never result in a duplicate. But
                        // we don't want to simply hard crash so log it and
                        // clear our style.
                        // log.err("style rehash resulted in needs rehash", .{});
                        self.assert_integrity();
                        return Ok(());
                    }
                }
            }
        };

        self.cursor.style_id = id;
        self.assert_integrity();
        Ok(())
    }

    /// Split at the given pin so that the pinned row moves to the page
    /// with less used capacity after the split.
    ///
    /// The primary use case for this is to handle IncreaseCapacityError
    /// OutOfSpace conditions where we need to split the page in order
    /// to make room for more managed memory.
    ///
    /// If the caller cares about where the pin moves to, they should
    /// setup a tracked pin before calling this and then check that.
    /// In many calling cases, the input pin is tracked (e.g. the cursor
    /// pin).
    ///
    /// If this returns OOM then its a system OOM. If this returns OutOfSpace
    /// then it means the page can't be split further.
    fn split_for_capacity(&mut self, pin: Pin) -> Result<(), pagelist::grow::SplitError> {
        // Get our capacities. We include our target row because its
        // capacity will be preserved.
        let bytes_above =
            Page::layout(unsafe { (*pin.node).data.exact_row_capacity(0, pin.y as usize + 1) })
                .total_size;
        let bytes_below = Page::layout(unsafe {
            let rows = (*pin.node).data.size.rows as usize;
            (*pin.node).data.exact_row_capacity(pin.y as usize, rows)
        })
        .total_size;

        // We need to track the old cursor pin because if our split
        // moves the cursor pin we need to update our accounting.
        let old_cursor = unsafe { *self.cursor.page_pin };

        // If our bytes above are less than bytes below, we move the pin
        // to split down one since splitting includes the pinned row in
        // the new node.
        self.pages.split(if bytes_above < bytes_below {
            pin.down(1).unwrap_or(pin)
        } else {
            pin
        })?;

        // Cursor didn't change nodes, we're done.
        if std::ptr::eq(unsafe { (*self.cursor.page_pin).node }, old_cursor.node) {
            return Ok(());
        }

        // Cursor changed, we need to restore the old pin then use
        // cursorChangePin to move to the new pin. The old node is guaranteed
        // to still exist, just not the row.
        //
        // Note that page_row and all that will be invalid, it points to the
        // new node, but at the time of writing this we don't need any of that
        // to be right in cursorChangePin.
        let new_cursor = unsafe { *self.cursor.page_pin };
        unsafe { *self.cursor.page_pin = old_cursor };
        self.cursor_change_pin(new_cursor);
        Ok(())
    }

    /// Append a grapheme to the given cell within the current cursor row.
    pub fn append_grapheme(
        &mut self,
        cell: *mut Cell,
        cp: u21,
    ) -> Result<(), pagelist::grow::IncreaseCapacityError> {
        use crate::pagelist::grow::IncreaseCapacity;

        let result = unsafe {
            (*(*self.cursor.page_pin).node)
                .data
                .append_grapheme(self.cursor.page_row, cell, cp)
        };
        match result {
            Ok(()) => {}
            Err(_oom) => {
                // We need to determine the actual cell index of the cell so
                // that after we adjust the capacity we can reload the cell.
                let cell_idx: usize = {
                    let cells: *mut Cell = self.cursor.page_cell;
                    let zero: *mut Cell = unsafe { cells.sub(self.cursor.x as usize) };
                    let target: *mut Cell = cell;
                    (target as usize - zero as usize) / core::mem::size_of::<Cell>()
                };

                // Adjust our capacity. This will update our cursor page pin and
                // force us to reload.
                let _ = self.increase_capacity(
                    unsafe { (*self.cursor.page_pin).node },
                    Some(IncreaseCapacity::GraphemeBytes),
                )?;

                // The cell pointer is now invalid, so we need to get it from
                // the reloaded cursor pointers.
                let reloaded_cell: *mut Cell = match cell_idx.cmp(&(self.cursor.x as usize)) {
                    core::cmp::Ordering::Equal => self.cursor.page_cell,
                    core::cmp::Ordering::Less => self.cursor_cell_left(
                        (self.cursor.x as usize - cell_idx) as size::CellCountInt,
                    ),
                    core::cmp::Ordering::Greater => self.cursor_cell_right(
                        (cell_idx - self.cursor.x as usize) as size::CellCountInt,
                    ),
                };

                unsafe {
                    (*(*self.cursor.page_pin).node).data.append_grapheme(
                        self.cursor.page_row,
                        reloaded_cell,
                        cp,
                    )
                }
                .map_err(|_err2| {
                    // This should never happen because we just increased capacity.
                    // Log loudly but still return an error so we don't just
                    // crash.
                    // log.err("grapheme append failed after capacity increase", .{});
                    pagelist::grow::IncreaseCapacityError::OutOfMemory
                })?;
            }
        }
        unsafe { (*(*self.cursor.page_pin).node).data.assert_integrity() };
        Ok(())
    }

    /// Start the hyperlink state. Future cells will be marked as hyperlinks with
    /// this state. Note that various terminal operations may clear the hyperlink
    /// state, such as switching screens (alt screen).
    pub fn start_hyperlink(
        &mut self,
        uri: &[u8],
        id_: Option<&[u8]>,
    ) -> Result<(), pagelist::grow::IncreaseCapacityError> {
        use crate::page::InsertHyperlinkError;
        use crate::pagelist::grow::IncreaseCapacity;

        // Create our pending entry.
        let link: hyperlink::Hyperlink = hyperlink::Hyperlink {
            uri: uri.to_vec(),
            id: if let Some(id) = id_ {
                hyperlink::HyperlinkId::Explicit(id.to_vec())
            } else {
                let implicit = self.cursor.hyperlink_implicit_id;
                self.cursor.hyperlink_implicit_id += 1;
                hyperlink::HyperlinkId::Implicit(implicit)
            },
        };
        // PORT NOTE: the reference `errdefer` decrements `hyperlink_implicit_id`
        // on the implicit path. The fallible work below is the
        // `increaseCapacity` calls; on those errors we restore the counter to
        // mirror the `errdefer`.
        let implicit_inc = matches!(link.id, hyperlink::HyperlinkId::Implicit(_));

        // Loop until we have enough page memory to add the hyperlink
        loop {
            match self.start_hyperlink_once(link.clone()) {
                Ok(()) => return Ok(()),
                Err(StartHyperlinkOnceError::OutOfMemory) => {
                    // An actual self.alloc OOM is a fatal error.
                    if implicit_inc {
                        self.cursor.hyperlink_implicit_id -= 1;
                    }
                    return Err(pagelist::grow::IncreaseCapacityError::OutOfMemory);
                }
                Err(StartHyperlinkOnceError::Insert(insert_err)) => {
                    let adj = match insert_err {
                        // strings table is out of memory, adjust it up
                        InsertHyperlinkError::StringsOutOfMemory => {
                            Some(IncreaseCapacity::StringBytes)
                        }
                        // hyperlink set is out of memory, adjust it up
                        InsertHyperlinkError::SetOutOfMemory => {
                            Some(IncreaseCapacity::HyperlinkBytes)
                        }
                        // hyperlink set is too full, rehash it
                        InsertHyperlinkError::SetNeedsRehash => None,
                    };
                    if let Err(e) =
                        self.increase_capacity(unsafe { (*self.cursor.page_pin).node }, adj)
                    {
                        if implicit_inc {
                            self.cursor.hyperlink_implicit_id -= 1;
                        }
                        return Err(e);
                    }
                }
            }

            self.assert_integrity();
        }
    }

    /// This is like startHyperlink but if we have to adjust page capacities
    /// this returns error.PageAdjusted. This is useful so that we unwind
    /// all the previous state and try again.
    fn start_hyperlink_once(
        &mut self,
        source: hyperlink::Hyperlink,
    ) -> Result<(), StartHyperlinkOnceError> {
        // End any prior hyperlink
        self.end_hyperlink();

        // Allocate our new Hyperlink entry in non-page memory. This
        // lets us quickly get access to URI, ID.
        // PORT NOTE: `alloc.create(Hyperlink)` + `link.* = source.dupe(alloc)`
        // maps to a heap `Box`; `Box::into_raw` yields the `*mut Hyperlink` the
        // cursor stores. On the insert-error path we reclaim the box (mirroring
        // the reference's `errdefer alloc.destroy(link)` / `link.deinit`).
        let link: *mut hyperlink::Hyperlink = Box::into_raw(Box::new(source.dupe()));

        // Insert the hyperlink into page memory
        let page: *mut Page =
            unsafe { core::ptr::addr_of_mut!((*(*self.cursor.page_pin).node).data) };
        let id: hyperlink::Id = match unsafe { (*page).insert_hyperlink((*link).clone()) } {
            Ok(id) => id,
            Err(e) => {
                let boxed = unsafe { Box::from_raw(link) };
                boxed.deinit();
                drop(boxed);
                return Err(StartHyperlinkOnceError::Insert(e));
            }
        };

        // Save it all
        self.cursor.hyperlink = Some(link);
        self.cursor.hyperlink_id = id;
        Ok(())
    }

    /// End the hyperlink state so that future cells aren't part of the
    /// current hyperlink (if any). This is safe to call multiple times.
    pub fn end_hyperlink(&mut self) {
        // If we have no hyperlink state then do nothing
        if self.cursor.hyperlink_id == 0 {
            debug_assert!(self.cursor.hyperlink.is_none());
            return;
        }

        // Release the old hyperlink state. If there are cells using the
        // hyperlink this will work because the creation creates a reference
        // and all additional cells create a new reference. This release will
        // just release our initial reference.
        //
        // If the ref count reaches zero the set will not delete the item
        // immediately; it is kept around in case it is used again (this is
        // how RefCountedSet works). This causes some memory fragmentation but
        // is fine because if it is ever pruned the context deleted callback
        // will be called.
        let page: *mut Page =
            unsafe { core::ptr::addr_of_mut!((*(*self.cursor.page_pin).node).data) };
        unsafe {
            let base = (*page).memory_ptr();
            (*page)
                .hyperlink_set
                .release(base, self.cursor.hyperlink_id);
        }
        let link = self.cursor.hyperlink.unwrap();
        let boxed = unsafe { Box::from_raw(link) };
        boxed.deinit();
        drop(boxed);
        self.cursor.hyperlink_id = 0;
        self.cursor.hyperlink = None;
    }

    /// Set the current hyperlink state on the current cell.
    pub fn cursor_set_hyperlink(&mut self) -> Result<(), pagelist::grow::IncreaseCapacityError> {
        use crate::pagelist::grow::IncreaseCapacity;

        debug_assert!(self.cursor.hyperlink_id != 0);

        let mut page: *mut Page =
            unsafe { core::ptr::addr_of_mut!((*(*self.cursor.page_pin).node).data) };
        match unsafe {
            (*page).set_hyperlink(
                self.cursor.page_row,
                self.cursor.page_cell,
                self.cursor.hyperlink_id,
            )
        } {
            Ok(()) => {
                // Success, increase the refcount for the hyperlink.
                unsafe {
                    let base = (*page).memory_ptr();
                    (*page).hyperlink_set.use_id(base, self.cursor.hyperlink_id);
                }
                Ok(())
            }
            // hyperlink_map is out of space, realloc the page to be larger
            Err(_hyperlink_map_oom) => {
                // Attempt to allocate the space that would be required to
                // insert a new copy of the cursor hyperlink uri in to the
                // string alloc, since right now increaseCapacity always just
                // adds an extra copy even if one already exists in the page.
                // If this alloc fails then we know we also need to grow our
                // string bytes.
                //
                // FIXME: increaseCapacity should not do this.
                while let Some(link) = self.cursor.hyperlink {
                    let uri_len = unsafe { (*link).uri.len() };
                    if unsafe { Page::string_alloc_alloc(page, uri_len) }.is_ok() {
                        // We don't bother freeing because we're
                        // about to free the entire page anyway.
                        break;
                    }

                    // We didn't have enough room, let's increase string bytes
                    let new_node = self.increase_capacity(
                        unsafe { (*self.cursor.page_pin).node },
                        Some(IncreaseCapacity::StringBytes),
                    )?;
                    debug_assert!(std::ptr::eq(new_node, unsafe {
                        (*self.cursor.page_pin).node
                    }));
                    page = unsafe { core::ptr::addr_of_mut!((*new_node).data) };
                }

                let _ = self.increase_capacity(
                    unsafe { (*self.cursor.page_pin).node },
                    Some(IncreaseCapacity::HyperlinkBytes),
                )?;

                // Retry
                //
                // We check that the cursor hyperlink hasn't been destroyed
                // by the capacity adjustment first though- since despite the
                // terrible code above, that can still apparently happen ._.
                if self.cursor.hyperlink_id > 0 {
                    return self.cursor_set_hyperlink();
                }
                Ok(())
            }
        }
    }

    /// Modify the semantic content type of the cursor. This should
    /// be preferred over setting it manually since it handles all the
    /// proper accounting.
    pub fn cursor_set_semantic_content(&mut self, t: SemanticContentArg) {
        match t {
            SemanticContentArg::Output => {
                self.cursor.semantic_content = pagepkg::SemanticContent::Output;
                self.cursor.semantic_content_clear_eol = false;
            }

            SemanticContentArg::Input(clear) => {
                self.cursor.semantic_content = pagepkg::SemanticContent::Input;
                self.cursor.semantic_content_clear_eol = match clear {
                    SemanticContentInput::ClearExplicit => false,
                    SemanticContentInput::ClearEol => true,
                };
            }

            SemanticContentArg::Prompt(kind) => {
                self.semantic_prompt.seen = true;
                self.cursor.semantic_content = pagepkg::SemanticContent::Prompt;
                self.cursor.semantic_content_clear_eol = false;
                let sp = match kind {
                    osc::semantic_prompt::PromptKind::Initial
                    | osc::semantic_prompt::PromptKind::Right => pagepkg::SemanticPrompt::Prompt,
                    osc::semantic_prompt::PromptKind::Continuation
                    | osc::semantic_prompt::PromptKind::Secondary => {
                        pagepkg::SemanticPrompt::PromptContinuation
                    }
                };
                unsafe { (*self.cursor.page_row).set_semantic_prompt(sp) };
            }
        }
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
        let sel = match sel_ {
            Some(sel) => sel,
            None => {
                self.clear_selection();
                return Ok(());
            }
        };

        // If this selection is untracked then we track it.
        // errdefer if (!sel.tracked()) tracked_sel.deinit(self);
        let tracked_sel = if sel.tracked() { sel } else { sel.track(self)? };

        // Untrack prior selection
        if let Some(old) = self.selection.take() {
            old.deinit(self);
        }
        self.selection = Some(tracked_sel);
        self.dirty.selection = true;
        Ok(())
    }

    /// Same as select(null) but can't fail.
    pub fn clear_selection(&mut self) {
        // if (self.selection) |*sel| {
        //     sel.deinit(self);
        //     self.dirty.selection = true;
        // }
        // self.selection = null;
        // PORT NOTE: `Selection::deinit` lands in the Selection-methods sub-batch
        // (vt-11.1). The foundation tests never set a selection, so this path is
        // exercised only with `self.selection == None`. The faithful body is
        // ported here; `deinit` is reached only once Selection is filled.
        if let Some(sel) = self.selection.take() {
            sel.deinit(self);
            self.dirty.selection = true;
        }
        self.selection = None;
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
        // We'll use this as our buffer to build our string.
        // PORT NOTE: the reference builds a `std.Io.Writer.Allocating` over
        // `alloc` and returns `toOwnedSliceSentinel(0)`. A `Vec<u8>` is the
        // faithful sink (the same convention as `dumpStringAlloc`); the
        // NUL-termination invariant of the `[:0]const u8` return is carried by
        // the `Vec<u8>` (its length excludes the sentinel, matching
        // `toOwnedSliceSentinel`).
        let mut aw: Vec<u8> = Vec::new();

        // Create a formatter and use that to emit our text.
        let mut formatter: ScreenFormatter = ScreenFormatter::init(
            self as *const Screen,
            formatter::Options {
                emit: formatter::Format::Plain,
                unwrap: true,
                trim: opts.trim,
                // PORT NOTE: the reference `.{ .emit = .plain, .unwrap = true,
                // .trim = opts.trim }` leaves the remaining `Options` fields at
                // their struct-literal defaults (`codepoint_map = .{}`,
                // background/foreground/palette = null). We mirror those here.
                codepoint_map: Some(Vec::new()),
                background: None,
                foreground: None,
                palette: None,
            },
        );
        formatter.content = Content::Selection(Some(opts.sel));

        // If we have a string map, we need to set that up.
        let mut pins: Vec<Pin> = Vec::new();
        if opts.map.is_some() {
            formatter.pin_map = Some(formatter::PinMap {
                alloc,
                map: &mut pins,
            });
        }

        // Emit. Since this is an allocating writer, a failed write
        // just becomes an OOM.
        formatter.format(&mut aw).map_err(|_| OutOfMemory)?;

        // Build our final text and if we have a string map set that up.
        let text = aw;
        // errdefer alloc.free(text);
        if let Some(map) = opts.map {
            let map_string = text.clone();
            // errdefer alloc.free(map_string);
            // PORT NOTE: the reference `errdefer alloc.free(map_string)` frees the
            // duplicated string if `selectionString_tw.check`/`toOwnedSlice` fails.
            // `map_string` is an owning `Vec` local, so any early `?` return below
            // drops it automatically — that IS the faithful errdefer.
            //
            // try selectionString_tw.check(.copy_map);
            // PORT NOTE: the `selectionString` tripwire is a fault-injection point
            // exercised only by the one map-allocation-failure test; the check is
            // `#[cfg(test)]`-gated (a no-op in production, mirroring the Zig
            // `enabled = builtin.is_test` gate).
            #[cfg(test)]
            selection_string_tw_check(SelectionStringFp::CopyMap)?;
            // const map_pins = try pins.toOwnedSlice(alloc);
            let map_pins = pins;
            *map = StringMap {
                string: map_string,
                map: map_pins,
            };
        }

        Ok(text)
    }

    /// Select the line under the given point. This will select across soft-wrapped
    /// lines and will omit the leading and trailing whitespace. If the point is
    /// over whitespace but the line has non-whitespace characters elsewhere, the
    /// line will be selected.
    pub fn select_line(&self, opts: SelectLine<'_>) -> Option<Selection> {
        // _ = self;

        // Get the current point semantic prompt state since that determines
        // boundary conditions too. This makes it so that line selection can
        // only happen within the same prompt state. For example, if you triple
        // click output, but the shell uses spaces to soft-wrap to the prompt
        // then the selection will stop prior to the prompt. See issue #1329.
        let semantic_prompt_state: Option<pagepkg::SemanticContent> = 'state: {
            if !opts.semantic_prompt_boundary {
                break 'state None;
            }
            let rac = opts.pin.row_and_cell();
            break 'state Some(unsafe { (*rac.cell).semantic_content() });
        };

        // The real start of the row is the first row in the soft-wrap.
        let start_pin: Pin = 'start_pin: {
            let mut it = opts
                .pin
                .row_iterator(pagelist::iter::Direction::LeftUp, None);
            let mut it_prev: Pin = it.next().unwrap(); // skip self

            // First, check the current row for semantic boundaries before the clicked position.
            if let Some(v) = semantic_prompt_state {
                let row = it_prev.row_and_cell().row;
                let cells = unsafe { (*it_prev.node).data.get_cells(row) };
                let cells = unsafe { &*cells };
                // Scan backwards from clicked position to find where our content starts
                for i in 0..(opts.pin.x as usize + 1) {
                    let x_rev = opts.pin.x as usize - i;
                    if cells[x_rev].semantic_content() != v {
                        let mut copy = it_prev;
                        copy.x = (x_rev + 1) as size::CellCountInt;
                        break 'start_pin copy;
                    }
                }

                // No boundary found before clicked position on current row.
                // If row doesn't wrap from above, start is at column 0.
                // Otherwise, continue checking previous rows.
            }

            // PORT NOTE: the reference `while (it.next()) |p| { ... } else { ... }`
            // runs the `else` block only if the loop finishes without `break`.
            // We mirror that with a labeled loop and a fall-through after it.
            loop {
                let p = match it.next() {
                    Some(p) => p,
                    None => {
                        let mut copy = it_prev;
                        copy.x = 0;
                        break 'start_pin copy;
                    }
                };
                let row = p.row_and_cell().row;

                if !unsafe { (*row).wrap() } {
                    let mut copy = it_prev;
                    copy.x = 0;
                    break 'start_pin copy;
                }

                if let Some(v) = semantic_prompt_state {
                    // We need to check every cell in this row in reverse
                    // order since we're going up and back.
                    let cells = unsafe { (*p.node).data.get_cells(row) };
                    let cells = unsafe { &*cells };
                    for x in 0..cells.len() {
                        let x_rev = cells.len() - 1 - x;
                        let cell = cells[x_rev];
                        if cell.semantic_content() != v {
                            break 'start_pin it_prev;
                        }
                        it_prev = p;
                        it_prev.x = x_rev as size::CellCountInt;
                    }

                    continue;
                }

                it_prev = p;
            }
        };

        // The real end of the row is the final row in the soft-wrap.
        let end_pin: Pin = 'end_pin: {
            let mut it = opts
                .pin
                .row_iterator(pagelist::iter::Direction::RightDown, None);
            while let Some(p) = it.next() {
                let row = p.row_and_cell().row;

                if let Some(v) = semantic_prompt_state {
                    // We need to check every cell in this row
                    let cells = unsafe { (*p.node).data.get_cells(row) };
                    let cells = unsafe { &*cells };

                    // If this is our pin row we can start from our x because
                    // the start_pin logic already found the real start.
                    let start_offset = if core::ptr::eq(p.node, opts.pin.node) && p.y == opts.pin.y
                    {
                        opts.pin.x as usize
                    } else {
                        0
                    };

                    // Handle the zero case specially because if the first
                    // col doesn't match then we end at the end of the prior
                    // row. But if this is the first row, we can't go back,
                    // so we scan forward to find where our content ends.
                    if start_offset == 0 && cells[0].semantic_content() != v {
                        let mut prev = p.up(1).unwrap();
                        prev.x = unsafe { (*p.node).data.size.cols } - 1;
                        break 'end_pin prev;
                    }

                    // For every other case, we end at the prior cell.
                    for (x, cell) in cells.iter().enumerate().skip(start_offset) {
                        if cell.semantic_content() != v {
                            let mut copy = p;
                            copy.x = (x - 1) as size::CellCountInt;
                            break 'end_pin copy;
                        }
                    }
                }

                if !unsafe { (*row).wrap() } {
                    let mut copy = p;
                    copy.x = unsafe { (*p.node).data.size.cols } - 1;
                    break 'end_pin copy;
                }
            }

            return None;
        };

        // Go forward from the start to find the first non-whitespace character.
        let start: Pin = 'start: {
            let whitespace = match opts.whitespace {
                Some(w) => w,
                None => break 'start start_pin,
            };
            let mut it =
                start_pin.cell_iterator(pagelist::iter::Direction::RightDown, Some(end_pin));
            while let Some(p) = it.next() {
                let cell = p.row_and_cell().cell;
                if !unsafe { (*cell).has_text() } {
                    continue;
                }

                // Non-empty means we found it.
                let this_whitespace = whitespace
                    .iter()
                    .any(|&w| w == unsafe { (*cell).codepoint() });
                if this_whitespace {
                    continue;
                }

                break 'start p;
            }

            return None;
        };

        // Go backward from the end to find the first non-whitespace character.
        let end: Pin = 'end: {
            let whitespace = match opts.whitespace {
                Some(w) => w,
                None => break 'end end_pin,
            };
            let mut it = end_pin.cell_iterator(pagelist::iter::Direction::LeftUp, Some(start_pin));
            while let Some(p) = it.next() {
                let cell = p.row_and_cell().cell;
                if !unsafe { (*cell).has_text() } {
                    continue;
                }

                // Non-empty means we found it.
                let this_whitespace = whitespace
                    .iter()
                    .any(|&w| w == unsafe { (*cell).codepoint() });
                if this_whitespace {
                    continue;
                }

                break 'end p;
            }

            return None;
        };

        Some(Selection::init(start, end, false))
    }

    /// Return the selection for all contents on the screen. Surrounding
    /// whitespace is omitted. If there is no selection, this returns null.
    pub fn select_all(&mut self) -> Option<Selection> {
        let whitespace: &[u32] = &[0, b' ' as u32, b'\t' as u32];

        let start: Pin = 'start: {
            let mut it = self.pages.cell_iterator(
                pagelist::iter::Direction::RightDown,
                point::Point::Screen(point::Coordinate::default()),
                None,
            );
            while let Some(p) = it.next() {
                let cell = p.row_and_cell().cell;
                if !unsafe { (*cell).has_text() } {
                    continue;
                }

                // Non-empty means we found it.
                let this_whitespace = whitespace
                    .iter()
                    .any(|&w| w == unsafe { (*cell).codepoint() });
                if this_whitespace {
                    continue;
                }

                break 'start p;
            }

            return None;
        };

        let end: Pin = 'end: {
            let mut it = self.pages.cell_iterator(
                pagelist::iter::Direction::LeftUp,
                point::Point::Screen(point::Coordinate::default()),
                None,
            );
            while let Some(p) = it.next() {
                let cell = p.row_and_cell().cell;
                if !unsafe { (*cell).has_text() } {
                    continue;
                }

                // Non-empty means we found it.
                let this_whitespace = whitespace
                    .iter()
                    .any(|&w| w == unsafe { (*cell).codepoint() });
                if this_whitespace {
                    continue;
                }

                break 'end p;
            }

            return None;
        };

        Some(Selection::init(start, end, false))
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
        let dir: pagelist::iter::Direction = if start.before(end) {
            pagelist::iter::Direction::RightDown
        } else {
            pagelist::iter::Direction::LeftUp
        };
        let mut it = start.cell_iterator(dir, Some(end));
        while let Some(pin) = it.next() {
            // Boundary conditions
            match dir {
                pagelist::iter::Direction::RightDown => {
                    if end.before(pin) {
                        return None;
                    }
                }
                pagelist::iter::Direction::LeftUp => {
                    if pin.before(end) {
                        return None;
                    }
                }
            }

            // If we found a word, then return it
            if let Some(sel) = self.select_word(pin, boundary_codepoints) {
                return Some(sel);
            }
        }

        None
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
        // _ = self;

        // If our cell is empty we can't select a word, because we can't select
        // areas where the screen is not yet written.
        let start_cell = pin.row_and_cell().cell;
        if !unsafe { (*start_cell).has_text() } {
            return None;
        }

        // Determine if we are a boundary or not to determine what our boundary is.
        let expect_boundary = boundary_codepoints
            .iter()
            .any(|&b| b == unsafe { (*start_cell).codepoint() });

        // Go forwards to find our end boundary
        let end: Pin = 'end: {
            let mut it = pin.cell_iterator(pagelist::iter::Direction::RightDown, None);
            let mut prev = it.next().unwrap(); // Consume one, our start
            while let Some(p) = it.next() {
                let rac = p.row_and_cell();
                let cell = rac.cell;

                // If we reached an empty cell its always a boundary
                if !unsafe { (*cell).has_text() } {
                    break 'end prev;
                }

                // If we do not match our expected set, we hit a boundary
                let this_boundary = boundary_codepoints
                    .iter()
                    .any(|&b| b == unsafe { (*cell).codepoint() });
                if this_boundary != expect_boundary {
                    break 'end prev;
                }

                // If we are going to the next row and it isn't wrapped, we
                // return the previous.
                if p.x == unsafe { (*p.node).data.size.cols } - 1 && !unsafe { (*rac.row).wrap() } {
                    break 'end p;
                }

                prev = p;
            }

            break 'end prev;
        };

        // Go backwards to find our start boundary
        let start: Pin = 'start: {
            let mut it = pin.cell_iterator(pagelist::iter::Direction::LeftUp, None);
            let mut prev = it.next().unwrap(); // Consume one, our start
            while let Some(p) = it.next() {
                let rac = p.row_and_cell();
                let cell = rac.cell;

                // If we are going to the next row and it isn't wrapped, we
                // return the previous.
                if p.x == unsafe { (*p.node).data.size.cols } - 1 && !unsafe { (*rac.row).wrap() } {
                    break 'start prev;
                }

                // If we reached an empty cell its always a boundary
                if !unsafe { (*cell).has_text() } {
                    break 'start prev;
                }

                // If we do not match our expected set, we hit a boundary
                let this_boundary = boundary_codepoints
                    .iter()
                    .any(|&b| b == unsafe { (*cell).codepoint() });
                if this_boundary != expect_boundary {
                    break 'start prev;
                }

                prev = p;
            }

            break 'start prev;
        };

        Some(Selection::init(start, end, false))
    }

    /// Select the command output under the given point. The limits of the output
    /// are determined by semantic prompt information provided by shell integration.
    /// A selection can span multiple physical lines if they are soft-wrapped.
    ///
    /// This will return null if a selection is impossible:
    ///  - the point pt is outside of the written screen space.
    ///  - the point pt is on a prompt / input line.
    pub fn select_output(&mut self, pin: Pin) -> Option<Selection> {
        // If our pin right now is not on output, then we return nothing.
        if unsafe { (*pin.row_and_cell().cell).semantic_content() }
            != pagepkg::SemanticContent::Output
        {
            return None;
        }

        // Get the post prior prompt from this pin. This is the prompt whose
        // output we'll be capturing.
        let prompt_pin: Pin = 'prompt: {
            // If we have a prompt above this point (including this point),
            // then thats the prompt we want to capture output from.
            let mut it = pin.prompt_iterator(pagelist::iter::Direction::LeftUp, None);
            if let Some(p) = it.next() {
                break 'prompt p;
            }

            // If we don't have a prompt, then we assume that we're
            // capturing all the output up to the next prompt.
            let mut it = pin.prompt_iterator(pagelist::iter::Direction::RightDown, None);
            let next = it.next()?;

            // We'll capture from the start of the screen to just above
            // the prompt and will trim the trailing whitespace.
            let start_pin = self.pages.get_top_left(point::Tag::Screen);
            let mut end_pin = next.up(1)?;
            end_pin.x = unsafe { (*end_pin.node).data.size.cols } - 1;
            let mut cell_it =
                end_pin.cell_iterator(pagelist::iter::Direction::LeftUp, Some(start_pin));
            while let Some(p) = cell_it.next() {
                let cell = p.row_and_cell().cell;
                end_pin = p;
                if unsafe { (*cell).has_text() } {
                    break;
                }
            }

            return Some(Selection::init(start_pin, end_pin, false));
        };

        // Grab our content
        let mut hl = self
            .pages
            .highlight_semantic_content(prompt_pin, pagepkg::SemanticContent::Output)?;

        // Trim our trailing whitespace
        let mut cell_it = hl
            .end
            .cell_iterator(pagelist::iter::Direction::LeftUp, Some(hl.start));
        while let Some(p) = cell_it.next() {
            let cell = p.row_and_cell().cell;
            hl.end = p;
            if unsafe { (*cell).has_text() } {
                break;
            }
        }

        Some(Selection::init(hl.start, hl.end, false))
    }

    /// Returns an iterator to move through the soft-wrapped lines starting
    /// from pin.
    pub fn line_iterator(&self, start: Pin) -> LineIterator<'_> {
        LineIterator {
            screen: self,
            current: Some(start),
        }
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
        // If we're not at an input cell with our cursor, no movement will
        // ever be possible.
        if self.cursor.semantic_content != pagepkg::SemanticContent::Input
            && unsafe { (*self.cursor.page_cell).semantic_content() }
                != pagepkg::SemanticContent::Input
        {
            return PromptClickMove::ZERO;
        }

        match &self.semantic_prompt.click {
            // None doesn't support movement and click_events must use a
            // different mechanism (SGR mouse events) that callers must handle.
            SemanticClick::None | SemanticClick::ClickEvents(_) => PromptClickMove::ZERO,
            SemanticClick::Cl(cl) => match cl {
                // All of these currently use dumb line-based navigation.
                // But eventually we'll support more.
                osc::semantic_prompt::Click::Line
                | osc::semantic_prompt::Click::Multiple
                | osc::semantic_prompt::Click::ConservativeVertical
                | osc::semantic_prompt::Click::SmartVertical => self.prompt_click_line(click_pin),
            },
        }
    }

    /// Determine the inputs required to move from the cursor to the given
    /// click location. If the cursor isn't currently at a prompt input
    /// location, this will return zero.
    ///
    /// This currently only supports moving a single line.
    fn prompt_click_line(&mut self, click_pin: Pin) -> PromptClickMove {
        // If our click pin is our cursor pin, no movement is needed.
        // Do this early so we can assume later that they are different.
        let cursor_pin = unsafe { *self.cursor.page_pin };
        if cursor_pin.eql(click_pin) {
            return PromptClickMove::ZERO;
        }

        // If our cursor is before our click, we're only emitting right inputs.
        if cursor_pin.before(click_pin) {
            let mut count: usize = 0;

            // We go row-by-row because soft-wrapped rows are still a single
            // line to a shell, so we can't just look at our page row.
            let mut row_it =
                cursor_pin.row_iterator(pagelist::iter::Direction::RightDown, Some(click_pin));
            'row_it: while let Some(row_pin) = row_it.next() {
                let rac = row_pin.row_and_cell();
                let cells: &[pagepkg::Cell] = unsafe { &*(*row_pin.node).data.get_cells(rac.row) };

                // Determine if this row is our cursor.
                let is_cursor_row =
                    std::ptr::eq(row_pin.node, cursor_pin.node) && row_pin.y == cursor_pin.y;

                // If this is not the cursor row, verify it's still part of the
                // continuation of our starting prompt.
                if !is_cursor_row
                    && unsafe { (*rac.row).semantic_prompt() }
                        != pagepkg::SemanticPrompt::PromptContinuation
                {
                    break;
                }

                // Determine where our input starts.
                let start_x: usize = 'start_x: {
                    // If this is our cursor row then we start after the cursor.
                    if is_cursor_row {
                        break 'start_x cursor_pin.x as usize + 1;
                    }

                    // Otherwise, we start at the first input cell, because
                    // we expect the shell to properly translate arrows across
                    // lines to the start of the input. Some shells indent
                    // where input starts on subsequent lines so we must do
                    // this.
                    for (x, cell) in cells.iter().enumerate() {
                        if cell.semantic_content() == pagepkg::SemanticContent::Input {
                            break 'start_x x;
                        }
                    }

                    // We never found an input cell, so we need to move to the
                    // next row.
                    break 'start_x cells.len();
                };

                // Iterate over the input cells and assume arrow keys only
                // jump to input cells.
                for (x, cell) in cells.iter().enumerate().skip(start_x) {
                    // Ignore non-input cells, but allow breaks. We assume
                    // the shell will translate arrow keys to only input
                    // areas.
                    if cell.semantic_content() != pagepkg::SemanticContent::Input {
                        continue;
                    }

                    // Increment our input count
                    count += 1;

                    // If this is our target, we're done.
                    if std::ptr::eq(row_pin.node, click_pin.node)
                        && row_pin.y == click_pin.y
                        && x == click_pin.x as usize
                    {
                        break 'row_it;
                    }
                }

                // If this row isn't soft-wrapped, we need to break out
                // because line based moving only handles single lines.
                // We're done!
                if !unsafe { (*rac.row).wrap() } {
                    // If we never found our pin, that means we clicked further
                    // right/beyond it. If we're already on a non-empty input cell
                    // then we add one so we can move to the newest, empty cell
                    // at the end, matching typical editor behavior.
                    if unsafe { (*self.cursor.page_cell).semantic_content() }
                        == pagepkg::SemanticContent::Input
                    {
                        count += 1;
                    }

                    break;
                }
            }

            return PromptClickMove {
                left: 0,
                right: count,
            };
        }

        // Otherwise, cursor is after click, so we're emitting left inputs.
        let mut count: usize = 0;

        // We go row-by-row because soft-wrapped rows are still a single
        // line to a shell, so we can't just look at our page row.
        let mut row_it =
            cursor_pin.row_iterator(pagelist::iter::Direction::LeftUp, Some(click_pin));
        'row_it: while let Some(row_pin) = row_it.next() {
            let rac = row_pin.row_and_cell();
            let cells: &[pagepkg::Cell] = unsafe { &*(*row_pin.node).data.get_cells(rac.row) };

            // Determine the length of the cells we look at in this row.
            let end_len: usize = 'end_len: {
                // If this is our cursor row then we end before the cursor.
                if std::ptr::eq(row_pin.node, cursor_pin.node) && row_pin.y == cursor_pin.y {
                    break 'end_len cursor_pin.x as usize;
                }

                // Otherwise, we end at the last cell in the row.
                break 'end_len cells.len();
            };

            // Iterate backwards over the input cells.
            for rev_x in 0..end_len {
                let x: usize = end_len - 1 - rev_x;
                let cell = &cells[x];

                // Ignore non-input cells.
                if cell.semantic_content() != pagepkg::SemanticContent::Input {
                    continue;
                }

                // Increment our input count
                count += 1;

                // If this is our target, we're done.
                if std::ptr::eq(row_pin.node, click_pin.node)
                    && row_pin.y == click_pin.y
                    && x == click_pin.x as usize
                {
                    break 'row_it;
                }
            }

            // If this row is not a wrap continuation, then break out
            if !unsafe { (*rac.row).wrap_continuation() } {
                break;
            }
        }

        PromptClickMove {
            left: count,
            right: 0,
        }
    }

    /// Dump the screen to a string. The writer given should be buffered;
    /// this function does not attempt to efficiently write and generally writes
    /// one byte at a time.
    pub fn dump_string(
        &self,
        writer: &mut dyn io::Write,
        opts: DumpStringOptions,
    ) -> io::Result<()> {
        // Create a formatter and use that to emit our text.
        let mut formatter: ScreenFormatter = ScreenFormatter::init(
            self as *const Screen,
            formatter::Options {
                emit: formatter::Format::Plain,
                unwrap: opts.unwrap,
                trim: false,
                codepoint_map: Some(Vec::new()),
                background: None,
                foreground: None,
                palette: None,
            },
        );

        // Set up the selection based on the pins
        let tl = opts.tl;
        let br = opts
            .br
            .unwrap_or_else(|| self.pages.get_bottom_right(point::Tag::Screen).unwrap());

        formatter.content = Content::Selection(Some(Selection::init(
            tl, br, false, // not rectangle
        )));

        // Emit
        formatter.format(writer)
    }

    /// You should use dumpString, this is a restricted version mostly for
    /// legacy and convenience reasons for unit tests.
    pub fn dump_string_alloc(
        &self,
        alloc: Allocator,
        tl: point::Point,
    ) -> Result<Vec<u8>, OutOfMemory> {
        // PORT NOTE: the reference builds an `Allocating` writer over `alloc`
        // and returns `toOwnedSlice()`. A `Vec<u8>` is the faithful sink; the
        // allocator is the crate's `Global` seam (matching the rest of Screen).
        let _ = alloc;
        let mut builder: Vec<u8> = Vec::new();

        // PORT NOTE: `getBottomRight(tl) orelse return error.UnknownPoint`. The
        // skeleton signature for this convenience helper returns
        // `Result<_, OutOfMemory>`; the reference's `error.UnknownPoint` cannot
        // occur for the valid screen/active/viewport points the unit tests pass
        // (those tags always resolve), so we surface it as `OutOfMemory` (the
        // only error variant in this seam) rather than introducing a new error
        // type that would ripple through every caller.
        let br = match self.pages.get_bottom_right(tl.tag()) {
            Some(p) => p,
            None => return Err(OutOfMemory),
        };

        self.dump_string(
            &mut builder,
            DumpStringOptions {
                tl: self.pages.get_top_left(tl.tag()),
                br: Some(br),
                unwrap: false,
            },
        )
        .map_err(|_| OutOfMemory)?;

        Ok(builder)
    }

    /// You should use dumpString, this is a restricted version mostly for
    /// legacy and convenience reasons for unit tests.
    pub fn dump_string_alloc_unwrapped(
        &self,
        alloc: Allocator,
        tl: point::Point,
    ) -> Result<Vec<u8>, OutOfMemory> {
        let _ = alloc;
        let mut builder: Vec<u8> = Vec::new();

        let br = match self.pages.get_bottom_right(tl.tag()) {
            Some(p) => p,
            None => return Err(OutOfMemory),
        };

        self.dump_string(
            &mut builder,
            DumpStringOptions {
                tl: self.pages.get_top_left(tl.tag()),
                br: Some(br),
                unwrap: true,
            },
        )
        .map_err(|_| OutOfMemory)?;

        Ok(builder)
    }

    /// This is basically a really jank version of Terminal.printString. We
    /// have to reimplement it here because we want a way to print to the screen
    /// to test it but don't want all the features of Terminal.
    pub fn test_write_string(&mut self, text: &[u8]) -> Result<(), OutOfMemory> {
        // const view = try std.unicode.Utf8View.init(text);
        // var iter = view.iterator();
        // while (iter.nextCodepoint()) |c| { ... }
        // PORT NOTE: `std.unicode.Utf8View.init` validates UTF-8 then iterates
        // codepoints; an invalid sequence is an error. We decode via
        // `str::from_utf8` (the same validation) and iterate `chars()`.
        let s = core::str::from_utf8(text).map_err(|_| OutOfMemory)?;
        for ch in s.chars() {
            let c = ch as u32;

            // Explicit newline forces a new row
            if c == '\n' as u32 {
                self.cursor_down_or_scroll().map_err(|_| OutOfMemory)?;
                self.cursor_horizontal_absolute(0);
                self.cursor.pending_wrap = false;
                if self.cursor.semantic_content_clear_eol {
                    self.cursor_set_semantic_content(SemanticContentArg::Output);
                } else {
                    match self.cursor.semantic_content {
                        pagepkg::SemanticContent::Output => {}
                        pagepkg::SemanticContent::Prompt | pagepkg::SemanticContent::Input => {
                            unsafe {
                                (*self.cursor.page_row).set_semantic_prompt(
                                    pagepkg::SemanticPrompt::PromptContinuation,
                                )
                            };
                        }
                    }
                }
                continue;
            }

            let width: usize = if c <= 0xFF {
                1
            } else {
                unicode::table().get(c).width as usize
            };
            if width == 0 {
                let cell = {
                    let mut cell = self.cursor_cell_left(1);
                    match unsafe { (*cell).wide() } {
                        pagepkg::Wide::Narrow => {}
                        pagepkg::Wide::Wide => {}
                        pagepkg::Wide::SpacerHead => unreachable!(),
                        pagepkg::Wide::SpacerTail => cell = self.cursor_cell_left(2),
                    }

                    cell
                };

                unsafe {
                    (*(*self.cursor.page_pin).node).data.append_grapheme(
                        self.cursor.page_row,
                        cell,
                        c,
                    )
                }
                .map_err(|_| OutOfMemory)?;
                continue;
            }

            if self.cursor.pending_wrap {
                debug_assert!(self.cursor.x == self.pages.cols - 1);
                self.cursor.pending_wrap = false;
                unsafe { (*self.cursor.page_row).set_wrap(true) };
                self.cursor_down_or_scroll().map_err(|_| OutOfMemory)?;
                self.cursor_horizontal_absolute(0);
                unsafe { (*self.cursor.page_row).set_wrap_continuation(true) };
                match self.cursor.semantic_content {
                    pagepkg::SemanticContent::Output => {}
                    pagepkg::SemanticContent::Input | pagepkg::SemanticContent::Prompt => unsafe {
                        (*self.cursor.page_row)
                            .set_semantic_prompt(pagepkg::SemanticPrompt::PromptContinuation)
                    },
                }
            }

            debug_assert!(width == 1 || width == 2);
            match width {
                1 => {
                    let mut cell = Cell::zeroed();
                    cell.set_content_tag(pagepkg::ContentTag::Codepoint);
                    cell.set_content_codepoint(c);
                    cell.set_style_id(self.cursor.style_id);
                    cell.set_protected(self.cursor.protected);
                    cell.set_semantic_content(self.cursor.semantic_content);
                    unsafe { *self.cursor.page_cell = cell };

                    // If we have a ref-counted style, increase.
                    if self.cursor.style_id != style::DEFAULT_ID {
                        let page = unsafe {
                            core::ptr::addr_of_mut!((*(*self.cursor.page_pin).node).data)
                        };
                        unsafe {
                            let base = (*page).memory_ptr();
                            (*page).styles.use_id(base, self.cursor.style_id);
                        }
                        unsafe { (*self.cursor.page_row).set_styled(true) };
                    }

                    // If we have a hyperlink, add it to the cell.
                    if self.cursor.hyperlink_id > 0 {
                        self.cursor_set_hyperlink().map_err(|_| OutOfMemory)?;
                    }
                }

                2 => {
                    // Need a wide spacer head
                    if self.cursor.x == self.pages.cols - 1 {
                        let mut cell = Cell::zeroed();
                        cell.set_content_tag(pagepkg::ContentTag::Codepoint);
                        cell.set_content_codepoint(0);
                        cell.set_wide(pagepkg::Wide::SpacerHead);
                        cell.set_protected(self.cursor.protected);
                        cell.set_semantic_content(self.cursor.semantic_content);
                        unsafe { *self.cursor.page_cell = cell };

                        // If we have a hyperlink, add it to the cell.
                        if self.cursor.hyperlink_id > 0 {
                            self.cursor_set_hyperlink().map_err(|_| OutOfMemory)?;
                        }

                        unsafe { (*self.cursor.page_row).set_wrap(true) };
                        self.cursor_down_or_scroll().map_err(|_| OutOfMemory)?;
                        self.cursor_horizontal_absolute(0);
                        unsafe { (*self.cursor.page_row).set_wrap_continuation(true) };
                    }

                    // Write our wide char
                    let mut cell = Cell::zeroed();
                    cell.set_content_tag(pagepkg::ContentTag::Codepoint);
                    cell.set_content_codepoint(c);
                    cell.set_style_id(self.cursor.style_id);
                    cell.set_wide(pagepkg::Wide::Wide);
                    cell.set_protected(self.cursor.protected);
                    cell.set_semantic_content(self.cursor.semantic_content);
                    unsafe { *self.cursor.page_cell = cell };

                    // If we have a hyperlink, add it to the cell.
                    if self.cursor.hyperlink_id > 0 {
                        self.cursor_set_hyperlink().map_err(|_| OutOfMemory)?;
                    }

                    // Write our tail
                    self.cursor_right(1);
                    let mut cell = Cell::zeroed();
                    cell.set_content_tag(pagepkg::ContentTag::Codepoint);
                    cell.set_content_codepoint(0);
                    cell.set_wide(pagepkg::Wide::SpacerTail);
                    cell.set_protected(self.cursor.protected);
                    cell.set_semantic_content(self.cursor.semantic_content);
                    unsafe { *self.cursor.page_cell = cell };

                    // If we have a hyperlink, add it to the cell.
                    if self.cursor.hyperlink_id > 0 {
                        self.cursor_set_hyperlink().map_err(|_| OutOfMemory)?;
                    }

                    // If we have a ref-counted style, increase twice.
                    if self.cursor.style_id != style::DEFAULT_ID {
                        let page = unsafe {
                            core::ptr::addr_of_mut!((*(*self.cursor.page_pin).node).data)
                        };
                        unsafe {
                            let base = (*page).memory_ptr();
                            (*page).styles.use_id(base, self.cursor.style_id);
                            (*page).styles.use_id(base, self.cursor.style_id);
                        }
                        unsafe { (*self.cursor.page_row).set_styled(true) };
                    }
                }

                _ => unreachable!(),
            }

            if self.cursor.x + 1 < self.pages.cols {
                self.cursor_right(1);
            } else {
                self.cursor.pending_wrap = true;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    // PORT NOTE: these are the Screen foundation tests that exercise ONLY the
    // methods filled in vt-11a (init/cursor movement/cursorCopy/setAttribute).
    // The read/write + `cursorCopy x/y` tests depend on `dumpStringAlloc`
    // (DEFERRED), so they land with the dump methods in a later sub-batch.
    use super::*;
    use crate::selection_codepoints;
    use crate::style;

    // test "Screen cursorCopy style deref"
    #[test]
    fn screen_cursor_copy_style_deref() {
        let s = Screen::init(
            Allocator,
            Options {
                cols: 10,
                rows: 10,
                max_scrollback: 0,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();
        let mut s = s;

        let mut s2 = Screen::init(
            Allocator,
            Options {
                cols: 10,
                rows: 10,
                max_scrollback: 0,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();
        let page: *mut Page =
            unsafe { core::ptr::addr_of_mut!((*(*s2.cursor.page_pin).node).data) };

        // Bold should create our style
        s2.set_attribute(crate::sgr::Attribute::Bold).unwrap();
        assert_eq!(1usize, unsafe { (*page).styles.count() });
        assert!(s2.cursor.style.flags.bold);

        // Copy default style, should release our style
        s2.cursor_copy(s.cursor, CursorCopyOptions { hyperlink: true })
            .unwrap();
        assert!(!s2.cursor.style.flags.bold);
        assert_eq!(0usize, unsafe { (*page).styles.count() });

        s.deinit();
        s2.deinit();
    }

    // test "Screen cursorCopy style deref new page"
    #[test]
    fn screen_cursor_copy_style_deref_new_page() {
        let mut s = Screen::init(
            Allocator,
            Options {
                cols: 10,
                rows: 10,
                max_scrollback: 0,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();

        let mut s2 = Screen::init(
            Allocator,
            Options {
                cols: 10,
                rows: 10,
                max_scrollback: 2048,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();

        // We need to get the cursor on a new page.
        let first_page_size = unsafe { (*s2.pages.pages.first).data.capacity.rows };

        // Fill the scrollback with blank lines until
        // there are only 5 rows left on the first page.
        unsafe { (*s2.pages.pages.first).data.pause_integrity_checks(true) };
        for _ in 0..(first_page_size - 5) {
            s2.test_write_string(b"\n").unwrap();
        }
        unsafe { (*s2.pages.pages.first).data.pause_integrity_checks(false) };

        s2.test_write_string(b"1\n2\n3\n4\n5\n6\n7\n8\n9\n10")
            .unwrap();

        // This should be PAGE 1
        let page: *mut Page =
            unsafe { core::ptr::addr_of_mut!((*(*s2.cursor.page_pin).node).data) };

        // It should be the last page in the list.
        assert!(std::ptr::eq(unsafe { &(*s2.pages.pages.last).data }, page));
        // It should have a previous page.
        assert!(!unsafe { (*(*s2.cursor.page_pin).node).prev }.is_null());

        // The cursor should be at 2, 9
        assert!(s2.cursor.x == 2);
        assert!(s2.cursor.y == 9);

        // Bold should create our style in page 1.
        s2.set_attribute(crate::sgr::Attribute::Bold).unwrap();
        assert_eq!(1usize, unsafe { (*page).styles.count() });
        assert!(s2.cursor.style.flags.bold);

        // Copy the cursor for the first screen. This should release
        // the style from page 1 and move the cursor back to page 0.
        s2.cursor_copy(s.cursor, CursorCopyOptions { hyperlink: true })
            .unwrap();
        assert!(!s2.cursor.style.flags.bold);
        assert_eq!(0usize, unsafe { (*page).styles.count() });
        // The page after the page the cursor is now in should be page 1.
        assert!(std::ptr::eq(page, unsafe {
            core::ptr::addr_of_mut!((*(*(*s2.cursor.page_pin).node).next).data)
        }));
        // The cursor should be at 0, 0
        assert!(s2.cursor.x == 0);
        assert!(s2.cursor.y == 0);

        s.deinit();
        s2.deinit();
    }

    // test "Screen cursorCopy style copy"
    #[test]
    fn screen_cursor_copy_style_copy() {
        let mut s = Screen::init(
            Allocator,
            Options {
                cols: 10,
                rows: 10,
                max_scrollback: 0,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();
        s.set_attribute(crate::sgr::Attribute::Bold).unwrap();

        let mut s2 = Screen::init(
            Allocator,
            Options {
                cols: 10,
                rows: 10,
                max_scrollback: 0,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();
        let page: *mut Page =
            unsafe { core::ptr::addr_of_mut!((*(*s2.cursor.page_pin).node).data) };
        s2.cursor_copy(s.cursor, CursorCopyOptions { hyperlink: true })
            .unwrap();
        assert!(s2.cursor.style.flags.bold);
        assert_eq!(1usize, unsafe { (*page).styles.count() });

        s.deinit();
        s2.deinit();
    }

    // test "Screen cursorCopy hyperlink deref"
    #[test]
    fn screen_cursor_copy_hyperlink_deref() {
        let mut s = Screen::init(
            Allocator,
            Options {
                cols: 10,
                rows: 10,
                max_scrollback: 0,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();

        let mut s2 = Screen::init(
            Allocator,
            Options {
                cols: 10,
                rows: 10,
                max_scrollback: 0,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();
        let page: *mut Page =
            unsafe { core::ptr::addr_of_mut!((*(*s2.cursor.page_pin).node).data) };

        // Create a hyperlink for the cursor.
        s2.start_hyperlink(b"https://example.com/", None).unwrap();
        assert_eq!(1usize, unsafe { (*page).hyperlink_set.count() });
        assert!(s2.cursor.hyperlink_id != 0);

        // Copy a cursor with no hyperlink, should release our hyperlink.
        s2.cursor_copy(s.cursor, CursorCopyOptions { hyperlink: true })
            .unwrap();
        assert_eq!(0usize, unsafe { (*page).hyperlink_set.count() });
        assert!(s2.cursor.hyperlink_id == 0);

        s.deinit();
        s2.deinit();
    }

    // test "Screen cursorCopy hyperlink deref new page"
    #[test]
    fn screen_cursor_copy_hyperlink_deref_new_page() {
        let mut s = Screen::init(
            Allocator,
            Options {
                cols: 10,
                rows: 10,
                max_scrollback: 0,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();

        let mut s2 = Screen::init(
            Allocator,
            Options {
                cols: 10,
                rows: 10,
                max_scrollback: 2048,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();

        // We need to get the cursor on a new page.
        let first_page_size = unsafe { (*s2.pages.pages.first).data.capacity.rows };

        // Fill the scrollback with blank lines until
        // there are only 5 rows left on the first page.
        unsafe { (*s2.pages.pages.first).data.pause_integrity_checks(true) };
        for _ in 0..(first_page_size - 5) {
            s2.test_write_string(b"\n").unwrap();
        }
        unsafe { (*s2.pages.pages.first).data.pause_integrity_checks(false) };

        s2.test_write_string(b"1\n2\n3\n4\n5\n6\n7\n8\n9\n10")
            .unwrap();

        // This should be PAGE 1
        let page: *mut Page =
            unsafe { core::ptr::addr_of_mut!((*(*s2.cursor.page_pin).node).data) };

        // It should be the last page in the list.
        assert!(std::ptr::eq(unsafe { &(*s2.pages.pages.last).data }, page));
        // It should have a previous page.
        assert!(!unsafe { (*(*s2.cursor.page_pin).node).prev }.is_null());

        // The cursor should be at 2, 9
        assert!(s2.cursor.x == 2);
        assert!(s2.cursor.y == 9);

        // Create a hyperlink for the cursor, should be in page 1.
        s2.start_hyperlink(b"https://example.com/", None).unwrap();
        assert_eq!(1usize, unsafe { (*page).hyperlink_set.count() });
        assert!(s2.cursor.hyperlink_id != 0);

        // Copy the cursor for the first screen. This should release
        // the hyperlink from page 1 and move the cursor back to page 0.
        s2.cursor_copy(s.cursor, CursorCopyOptions { hyperlink: true })
            .unwrap();
        assert_eq!(0usize, unsafe { (*page).hyperlink_set.count() });
        assert!(s2.cursor.hyperlink_id == 0);
        // The page after the page the cursor is now in should be page 1.
        assert!(std::ptr::eq(page, unsafe {
            core::ptr::addr_of_mut!((*(*(*s2.cursor.page_pin).node).next).data)
        }));
        // The cursor should be at 0, 0
        assert!(s2.cursor.x == 0);
        assert!(s2.cursor.y == 0);

        s.deinit();
        s2.deinit();
    }

    // test "Screen cursorCopy hyperlink copy"
    #[test]
    fn screen_cursor_copy_hyperlink_copy() {
        let mut s = Screen::init(
            Allocator,
            Options {
                cols: 10,
                rows: 10,
                max_scrollback: 0,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();

        // Create a hyperlink for the cursor.
        s.start_hyperlink(b"https://example.com/", None).unwrap();
        assert_eq!(1usize, unsafe {
            (*(*s.cursor.page_pin).node).data.hyperlink_set.count()
        });
        assert!(s.cursor.hyperlink_id != 0);

        let mut s2 = Screen::init(
            Allocator,
            Options {
                cols: 10,
                rows: 10,
                max_scrollback: 0,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();
        let page: *mut Page =
            unsafe { core::ptr::addr_of_mut!((*(*s2.cursor.page_pin).node).data) };

        assert_eq!(0usize, unsafe { (*page).hyperlink_set.count() });
        assert!(s2.cursor.hyperlink_id == 0);

        // Copy the cursor with the hyperlink.
        s2.cursor_copy(s.cursor, CursorCopyOptions { hyperlink: true })
            .unwrap();
        assert_eq!(1usize, unsafe { (*page).hyperlink_set.count() });
        assert!(s2.cursor.hyperlink_id != 0);

        s.deinit();
        s2.deinit();
    }

    // test "Screen cursorCopy hyperlink copy disabled"
    #[test]
    fn screen_cursor_copy_hyperlink_copy_disabled() {
        let mut s = Screen::init(
            Allocator,
            Options {
                cols: 10,
                rows: 10,
                max_scrollback: 0,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();

        // Create a hyperlink for the cursor.
        s.start_hyperlink(b"https://example.com/", None).unwrap();
        assert_eq!(1usize, unsafe {
            (*(*s.cursor.page_pin).node).data.hyperlink_set.count()
        });
        assert!(s.cursor.hyperlink_id != 0);

        let mut s2 = Screen::init(
            Allocator,
            Options {
                cols: 10,
                rows: 10,
                max_scrollback: 0,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();
        let page: *mut Page =
            unsafe { core::ptr::addr_of_mut!((*(*s2.cursor.page_pin).node).data) };

        assert_eq!(0usize, unsafe { (*page).hyperlink_set.count() });
        assert!(s2.cursor.hyperlink_id == 0);

        // Copy the cursor with the hyperlink.
        s2.cursor_copy(s.cursor, CursorCopyOptions { hyperlink: false })
            .unwrap();
        assert_eq!(0usize, unsafe { (*page).hyperlink_set.count() });
        assert!(s2.cursor.hyperlink_id == 0);

        s.deinit();
        s2.deinit();
    }

    // test "Screen style basics"
    #[test]
    fn screen_style_basics() {
        let mut s = Screen::init(
            Allocator,
            Options {
                cols: 80,
                rows: 24,
                max_scrollback: 1000,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();
        let page: *mut Page = unsafe { core::ptr::addr_of_mut!((*(*s.cursor.page_pin).node).data) };
        assert_eq!(0usize, unsafe { (*page).styles.count() });

        // Set a new style
        s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
        assert!(s.cursor.style_id != 0);
        assert_eq!(1usize, unsafe { (*page).styles.count() });
        assert!(s.cursor.style.flags.bold);

        // Set another style, we should still only have one since it was unused
        s.set_attribute(crate::sgr::Attribute::Italic).unwrap();
        assert!(s.cursor.style_id != 0);
        assert_eq!(1usize, unsafe { (*page).styles.count() });
        assert!(s.cursor.style.flags.italic);

        s.deinit();
    }

    // test "Screen style reset to default"
    #[test]
    fn screen_style_reset_to_default() {
        let mut s = Screen::init(
            Allocator,
            Options {
                cols: 80,
                rows: 24,
                max_scrollback: 1000,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();
        let page: *mut Page = unsafe { core::ptr::addr_of_mut!((*(*s.cursor.page_pin).node).data) };
        assert_eq!(0usize, unsafe { (*page).styles.count() });

        // Set a new style
        s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
        assert!(s.cursor.style_id != 0);
        assert_eq!(1usize, unsafe { (*page).styles.count() });

        // Reset to default
        s.set_attribute(crate::sgr::Attribute::ResetBold).unwrap();
        assert!(s.cursor.style_id == 0);
        assert_eq!(0usize, unsafe { (*page).styles.count() });

        s.deinit();
    }

    // test "Screen style reset with unset"
    #[test]
    fn screen_style_reset_with_unset() {
        let mut s = Screen::init(
            Allocator,
            Options {
                cols: 80,
                rows: 24,
                max_scrollback: 1000,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();
        let page: *mut Page = unsafe { core::ptr::addr_of_mut!((*(*s.cursor.page_pin).node).data) };
        assert_eq!(0usize, unsafe { (*page).styles.count() });

        // Set a new style
        s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
        assert!(s.cursor.style_id != 0);
        assert_eq!(1usize, unsafe { (*page).styles.count() });

        // Reset to default
        s.set_attribute(crate::sgr::Attribute::Unset).unwrap();
        assert!(s.cursor.style_id == 0);
        assert_eq!(0usize, unsafe { (*page).styles.count() });

        s.deinit();
    }

    // test "Screen: cursorDown across pages preserves style"
    #[test]
    fn screen_cursor_down_across_pages_preserves_style() {
        let mut s = Screen::init(
            Allocator,
            Options {
                cols: 10,
                rows: 3,
                max_scrollback: 1,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();

        // Scroll down enough to go to another page
        let start_page: *mut Page = unsafe { core::ptr::addr_of_mut!((*s.pages.pages.last).data) };
        let rem = unsafe { (*start_page).capacity.rows };
        unsafe { (*start_page).pause_integrity_checks(true) };
        for _ in 0..rem {
            s.cursor_down_or_scroll().unwrap();
        }
        unsafe { (*start_page).pause_integrity_checks(false) };

        // We need our page to change for this test o make sense. If this
        // assertion fails then the bug is in the test: we should be scrolling
        // above enough for a new page to show up.
        {
            let page: *mut Page =
                unsafe { core::ptr::addr_of_mut!((*(*s.cursor.page_pin).node).data) };
            assert!(!std::ptr::eq(start_page, page));
        }

        // Scroll back to the previous page
        s.cursor_up(1);
        {
            let page: *mut Page =
                unsafe { core::ptr::addr_of_mut!((*(*s.cursor.page_pin).node).data) };
            assert!(std::ptr::eq(start_page, page));
        }

        // Go back up, set a style
        s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
        {
            let page: *mut Page =
                unsafe { core::ptr::addr_of_mut!((*(*s.cursor.page_pin).node).data) };
            let styleval = unsafe {
                let base = (*page).memory_ptr();
                &*(*page).styles.get(base, s.cursor.style_id)
            };
            assert!(styleval.flags.bold);
        }

        // Go back down into the next page and we should have that style
        s.cursor_down(1);
        {
            let page: *mut Page =
                unsafe { core::ptr::addr_of_mut!((*(*s.cursor.page_pin).node).data) };
            let styleval = unsafe {
                let base = (*page).memory_ptr();
                &*(*page).styles.get(base, s.cursor.style_id)
            };
            assert!(styleval.flags.bold);
        }

        s.deinit();
    }

    // test "Screen: cursorUp across pages preserves style"
    #[test]
    fn screen_cursor_up_across_pages_preserves_style() {
        let mut s = Screen::init(
            Allocator,
            Options {
                cols: 10,
                rows: 3,
                max_scrollback: 1,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();

        let start_page: *mut Page = unsafe { core::ptr::addr_of_mut!((*s.pages.pages.last).data) };
        let rem = unsafe { (*start_page).capacity.rows };
        unsafe { (*start_page).pause_integrity_checks(true) };
        for _ in 0..rem {
            s.cursor_down_or_scroll().unwrap();
        }
        unsafe { (*start_page).pause_integrity_checks(false) };

        {
            let page: *mut Page =
                unsafe { core::ptr::addr_of_mut!((*(*s.cursor.page_pin).node).data) };
            assert!(!std::ptr::eq(start_page, page));
        }

        // Go back up, set a style
        s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
        {
            let page: *mut Page =
                unsafe { core::ptr::addr_of_mut!((*(*s.cursor.page_pin).node).data) };
            let styleval = unsafe {
                let base = (*page).memory_ptr();
                &*(*page).styles.get(base, s.cursor.style_id)
            };
            assert!(styleval.flags.bold);
        }

        // Go back down into the prev page and we should have that style
        s.cursor_up(1);
        {
            let page: *mut Page =
                unsafe { core::ptr::addr_of_mut!((*(*s.cursor.page_pin).node).data) };
            assert!(std::ptr::eq(start_page, page));

            let styleval = unsafe {
                let base = (*page).memory_ptr();
                &*(*page).styles.get(base, s.cursor.style_id)
            };
            assert!(styleval.flags.bold);
        }

        s.deinit();
    }

    // test "Screen: cursorAbsolute across pages preserves style"
    #[test]
    fn screen_cursor_absolute_across_pages_preserves_style() {
        let mut s = Screen::init(
            Allocator,
            Options {
                cols: 10,
                rows: 3,
                max_scrollback: 1,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();

        let start_page: *mut Page = unsafe { core::ptr::addr_of_mut!((*s.pages.pages.last).data) };
        let rem = unsafe { (*start_page).capacity.rows };
        unsafe { (*start_page).pause_integrity_checks(true) };
        for _ in 0..rem {
            s.cursor_down_or_scroll().unwrap();
        }
        unsafe { (*start_page).pause_integrity_checks(false) };

        {
            let page: *mut Page =
                unsafe { core::ptr::addr_of_mut!((*(*s.cursor.page_pin).node).data) };
            assert!(!std::ptr::eq(start_page, page));
        }

        // Go back up, set a style
        s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
        {
            let page: *mut Page =
                unsafe { core::ptr::addr_of_mut!((*(*s.cursor.page_pin).node).data) };
            let styleval = unsafe {
                let base = (*page).memory_ptr();
                &*(*page).styles.get(base, s.cursor.style_id)
            };
            assert!(styleval.flags.bold);
        }

        // Go back down into the prev page and we should have that style
        s.cursor_absolute(1, 1);
        {
            let page: *mut Page =
                unsafe { core::ptr::addr_of_mut!((*(*s.cursor.page_pin).node).data) };
            assert!(std::ptr::eq(start_page, page));

            let styleval = unsafe {
                let base = (*page).memory_ptr();
                &*(*page).styles.get(base, s.cursor.style_id)
            };
            assert!(styleval.flags.bold);
        }

        s.deinit();
    }

    // test "Screen: cursorAbsolute to page with insufficient capacity"
    #[test]
    fn screen_cursor_absolute_to_page_with_insufficient_capacity() {
        // This test checks for a very specific edge case
        // which previously resulted in memory corruption.
        //
        // The conditions for this edge case are as such:
        // - The cursor has an associated style or other managed memory.
        // - The cursor moves to a different page.
        // - The new page is at capacity and must have its capacity adjusted.
        let mut s = Screen::init(
            Allocator,
            Options {
                cols: 10,
                rows: 3,
                max_scrollback: 1,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();

        // Scroll down enough to go to another page
        let start_page: *mut Page = unsafe { core::ptr::addr_of_mut!((*s.pages.pages.last).data) };
        let rem = unsafe { (*start_page).capacity.rows };
        unsafe { (*start_page).pause_integrity_checks(true) };
        for _ in 0..rem {
            s.cursor_down_or_scroll().unwrap();
        }
        unsafe { (*start_page).pause_integrity_checks(false) };

        let new_page: *mut Page =
            unsafe { core::ptr::addr_of_mut!((*(*s.cursor.page_pin).node).data) };

        // We need our page to change for this test to make sense. If this
        // assertion fails then the bug is in the test: we should be scrolling
        // above enough for a new page to show up.
        assert!(!std::ptr::eq(start_page, new_page));

        // Add styles to the start page until it reaches capacity.
        {
            // Pause integrity checks because they're slow and
            // we're not testing this, this is just setup.
            unsafe { (*start_page).pause_integrity_checks(true) };

            let mut n: u32 = 1;
            loop {
                let st = style::Style {
                    bg_color: style::Color::Rgb(crate::color::RGB {
                        r: (n & 0xFF) as u8,
                        g: ((n >> 8) & 0xFF) as u8,
                        b: ((n >> 16) & 0xFF) as u8,
                    }),
                    fg_color: style::Color::None,
                    underline_color: style::Color::None,
                    flags: style::Flags::default(),
                };
                let res = unsafe {
                    let base = (*start_page).memory_ptr();
                    (*start_page).styles.add(base, st)
                };
                if res.is_ok() {
                    n += 1;
                } else {
                    break;
                }
            }

            unsafe { (*start_page).pause_integrity_checks(false) };
            unsafe { (*start_page).assert_integrity() };
        }

        // Set a style on the cursor.
        s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
        {
            let styleval = unsafe {
                let base = (*new_page).memory_ptr();
                &*(*new_page).styles.get(base, s.cursor.style_id)
            };
            assert!(styleval.flags.bold);
        }

        // Go back up into the start page and we should still have that style.
        s.cursor_absolute(1, 1);
        {
            let cur_page: *mut Page =
                unsafe { core::ptr::addr_of_mut!((*(*s.cursor.page_pin).node).data) };
            // The page we're on now should NOT equal start_page, since its
            // capacity should have been adjusted, which invalidates our ptr.
            assert!(!std::ptr::eq(start_page, cur_page));
            // To make sure we DID change pages we check we're not on new_page.
            assert!(!std::ptr::eq(new_page, cur_page));

            let styleval = unsafe {
                let base = (*cur_page).memory_ptr();
                &*(*cur_page).styles.get(base, s.cursor.style_id)
            };
            assert!(styleval.flags.bold);
        }

        unsafe { (*(*s.cursor.page_pin).node).data.assert_integrity() };
        unsafe { (*new_page).assert_integrity() };

        s.deinit();
    }

    // PORT NOTE: these tests were DEFERRED from vt-11a because they require
    // `dumpStringAlloc`, which lands here in vt-11b alongside the formatter
    // production code.

    // test "Screen read and write"
    #[test]
    fn screen_read_and_write() {
        let mut s = Screen::init(
            Allocator,
            Options {
                cols: 80,
                rows: 24,
                max_scrollback: 1000,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();
        assert_eq!(s.cursor.style_id, 0 as style::Id);

        s.test_write_string(b"hello, world").unwrap();
        let str = s
            .dump_string_alloc(
                Allocator,
                point::Point::Screen(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(str, b"hello, world");

        s.deinit();
    }

    // test "Screen read and write newline"
    #[test]
    fn screen_read_and_write_newline() {
        let mut s = Screen::init(
            Allocator,
            Options {
                cols: 80,
                rows: 24,
                max_scrollback: 1000,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();
        assert_eq!(s.cursor.style_id, 0 as style::Id);

        s.test_write_string(b"hello\nworld").unwrap();
        let str = s
            .dump_string_alloc(
                Allocator,
                point::Point::Screen(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(str, b"hello\nworld");

        s.deinit();
    }

    // test "Screen read and write scrollback"
    #[test]
    fn screen_read_and_write_scrollback() {
        let mut s = Screen::init(
            Allocator,
            Options {
                cols: 80,
                rows: 2,
                max_scrollback: 1000,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();

        s.test_write_string(b"hello\nworld\ntest").unwrap();
        {
            let str = s
                .dump_string_alloc(
                    Allocator,
                    point::Point::Screen(point::Coordinate::default()),
                )
                .unwrap();
            assert_eq!(str, b"hello\nworld\ntest");
        }
        {
            let str = s
                .dump_string_alloc(
                    Allocator,
                    point::Point::Active(point::Coordinate::default()),
                )
                .unwrap();
            assert_eq!(str, b"world\ntest");
        }

        s.deinit();
    }

    // test "Screen read and write no scrollback small"
    #[test]
    fn screen_read_and_write_no_scrollback_small() {
        let mut s = Screen::init(
            Allocator,
            Options {
                cols: 80,
                rows: 2,
                max_scrollback: 0,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();

        s.test_write_string(b"hello\nworld\ntest").unwrap();
        {
            let str = s
                .dump_string_alloc(
                    Allocator,
                    point::Point::Screen(point::Coordinate::default()),
                )
                .unwrap();
            assert_eq!(str, b"world\ntest");
        }
        {
            let str = s
                .dump_string_alloc(
                    Allocator,
                    point::Point::Active(point::Coordinate::default()),
                )
                .unwrap();
            assert_eq!(str, b"world\ntest");
        }

        s.deinit();
    }

    // test "Screen read and write no scrollback large"
    #[test]
    fn screen_read_and_write_no_scrollback_large() {
        let mut s = Screen::init(
            Allocator,
            Options {
                cols: 80,
                rows: 2,
                max_scrollback: 0,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();

        for i in 0..1_000u32 {
            // PORT NOTE: `std.fmt.bufPrint(&buf, "{}\n", .{i})`.
            let str = format!("{}\n", i);
            s.test_write_string(str.as_bytes()).unwrap();
        }
        s.test_write_string(b"1000").unwrap();

        {
            let str = s
                .dump_string_alloc(
                    Allocator,
                    point::Point::Screen(point::Coordinate::default()),
                )
                .unwrap();
            assert_eq!(str, b"999\n1000");
        }

        s.deinit();
    }

    // test "Screen cursorCopy x/y"
    #[test]
    fn screen_cursor_copy_x_y() {
        let mut s = Screen::init(
            Allocator,
            Options {
                cols: 10,
                rows: 10,
                max_scrollback: 0,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();
        s.cursor_absolute(2, 3);
        assert!(s.cursor.x == 2);
        assert!(s.cursor.y == 3);

        let mut s2 = Screen::init(
            Allocator,
            Options {
                cols: 10,
                rows: 10,
                max_scrollback: 0,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();
        s2.cursor_copy(s.cursor, CursorCopyOptions { hyperlink: false })
            .unwrap();
        assert!(s2.cursor.x == 2);
        assert!(s2.cursor.y == 3);
        s2.test_write_string(b"Hello").unwrap();

        {
            let str = s2
                .dump_string_alloc(
                    Allocator,
                    point::Point::Screen(point::Coordinate::default()),
                )
                .unwrap();
            assert_eq!(str, b"\n\n\n  Hello");
        }

        s.deinit();
        s2.deinit();
    }

    // PORT NOTE: small helpers to keep the resize tests close to the Zig form.
    // `mkscreen` mirrors `try init(alloc, .{ ... })`; `dump`/`dump_unwrapped`
    // mirror `try s.dumpStringAlloc(alloc, <point>)`.
    fn mkscreen(
        cols: size::CellCountInt,
        rows: size::CellCountInt,
        max_scrollback: usize,
    ) -> Screen {
        Screen::init(
            Allocator,
            Options {
                cols,
                rows,
                max_scrollback,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap()
    }

    fn vp() -> point::Point {
        point::Point::Viewport(point::Coordinate::default())
    }
    fn scr() -> point::Point {
        point::Point::Screen(point::Coordinate::default())
    }
    fn act() -> point::Point {
        point::Point::Active(point::Coordinate::default())
    }
    fn dump(s: &Screen, pt: point::Point) -> Vec<u8> {
        s.dump_string_alloc(Allocator, pt).unwrap()
    }

    // test "Screen: resize (no reflow) more rows"
    #[test]
    fn screen_resize_no_reflow_more_rows() {
        let mut s = mkscreen(10, 3, 0);
        let str = b"1ABCD\n2EFGH\n3IJKL";
        s.test_write_string(str).unwrap();

        // Resize
        s.resize(Resize {
            cols: 10,
            rows: 10,
            reflow: false,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, str);
        }
        s.deinit();
    }

    // test "Screen: resize (no reflow) less rows"
    #[test]
    fn screen_resize_no_reflow_less_rows() {
        let mut s = mkscreen(10, 3, 0);
        let str = b"1ABCD\n2EFGH\n3IJKL";
        s.test_write_string(str).unwrap();
        assert_eq!(5, s.cursor.x);
        assert_eq!(2, s.cursor.y);
        s.resize(Resize {
            cols: 10,
            rows: 2,
            reflow: false,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();

        // Since we shrunk, we should adjust our cursor
        assert_eq!(5, s.cursor.x);
        assert_eq!(1, s.cursor.y);

        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"2EFGH\n3IJKL");
        }
        s.deinit();
    }

    // test "Screen: resize (no reflow) less rows trims blank lines"
    #[test]
    fn screen_resize_no_reflow_less_rows_trims_blank_lines() {
        let mut s = mkscreen(10, 3, 0);
        let str = b"1ABCD";
        s.test_write_string(str).unwrap();

        // Write only a background color into the remaining rows
        for y in 1..s.pages.rows {
            let list_cell = s
                .pages
                .get_cell(point::Point::Active(point::Coordinate {
                    x: 0,
                    y: y as u32,
                }))
                .unwrap();
            unsafe {
                (*list_cell.cell).set_content_tag(pagepkg::ContentTag::BgColorRgb);
                (*list_cell.cell).set_content_color_rgb(pagepkg::CellRgb {
                    r: 0xFF,
                    g: 0,
                    b: 0,
                });
            }
        }

        let cursor = s.cursor;
        s.resize(Resize {
            cols: 6,
            rows: 2,
            reflow: false,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();

        // Cursor should not move
        assert_eq!(cursor.x, s.cursor.x);
        assert_eq!(cursor.y, s.cursor.y);

        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"1ABCD");
        }
        s.deinit();
    }

    // test "Screen: resize (no reflow) more rows trims blank lines"
    #[test]
    fn screen_resize_no_reflow_more_rows_trims_blank_lines() {
        let mut s = mkscreen(10, 3, 0);
        let str = b"1ABCD";
        s.test_write_string(str).unwrap();

        // Write only a background color into the remaining rows
        for y in 1..s.pages.rows {
            let list_cell = s
                .pages
                .get_cell(point::Point::Active(point::Coordinate {
                    x: 0,
                    y: y as u32,
                }))
                .unwrap();
            unsafe {
                (*list_cell.cell).set_content_tag(pagepkg::ContentTag::BgColorRgb);
                (*list_cell.cell).set_content_color_rgb(pagepkg::CellRgb {
                    r: 0xFF,
                    g: 0,
                    b: 0,
                });
            }
        }

        let cursor = s.cursor;
        s.resize(Resize {
            cols: 10,
            rows: 7,
            reflow: false,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();

        // Cursor should not move
        assert_eq!(cursor.x, s.cursor.x);
        assert_eq!(cursor.y, s.cursor.y);

        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"1ABCD");
        }
        s.deinit();
    }

    // test "Screen: resize (no reflow) more cols"
    #[test]
    fn screen_resize_no_reflow_more_cols() {
        let mut s = mkscreen(10, 3, 0);
        let str = b"1ABCD\n2EFGH\n3IJKL";
        s.test_write_string(str).unwrap();
        s.resize(Resize {
            cols: 20,
            rows: 3,
            reflow: false,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();

        {
            let contents = dump(&s, vp());
            assert_eq!(contents, str);
        }
        s.deinit();
    }

    // test "Screen: resize (no reflow) less cols"
    #[test]
    fn screen_resize_no_reflow_less_cols() {
        let mut s = mkscreen(10, 3, 0);
        let str = b"1ABCD\n2EFGH\n3IJKL";
        s.test_write_string(str).unwrap();
        s.resize(Resize {
            cols: 4,
            rows: 3,
            reflow: false,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();

        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"1ABC\n2EFG\n3IJK");
        }
        s.deinit();
    }

    // test "Screen: resize (no reflow) more rows with scrollback cursor end"
    #[test]
    fn screen_resize_no_reflow_more_rows_with_scrollback_cursor_end() {
        let mut s = mkscreen(7, 3, 2);
        let str = b"1ABCD\n2EFGH\n3IJKL\n4ABCD\n5EFGH";
        s.test_write_string(str).unwrap();
        s.resize(Resize {
            cols: 7,
            rows: 10,
            reflow: false,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();

        {
            let contents = dump(&s, vp());
            assert_eq!(contents, str);
        }
        s.deinit();
    }

    // test "Screen: resize (no reflow) less rows with scrollback"
    #[test]
    fn screen_resize_no_reflow_less_rows_with_scrollback() {
        let mut s = mkscreen(7, 3, 2);
        let str = b"1ABCD\n2EFGH\n3IJKL\n4ABCD\n5EFGH";
        s.test_write_string(str).unwrap();
        s.resize(Resize {
            cols: 7,
            rows: 2,
            reflow: false,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();

        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"4ABCD\n5EFGH");
        }
        s.deinit();
    }

    // https://github.com/mitchellh/ghostty/issues/1030
    // test "Screen: resize (no reflow) less rows with empty trailing"
    #[test]
    fn screen_resize_no_reflow_less_rows_with_empty_trailing() {
        let mut s = mkscreen(5, 3, 5);
        let str = b"1\n2\n3\n4\n5\n6\n7\n8";
        s.test_write_string(str).unwrap();
        s.scroll_clear().unwrap();
        s.cursor_absolute(0, 0);
        s.test_write_string(b"A\nB").unwrap();

        let cursor = s.cursor;
        s.resize(Resize {
            cols: 5,
            rows: 2,
            reflow: false,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();
        assert_eq!(cursor.x, s.cursor.x);
        assert_eq!(cursor.y, s.cursor.y);

        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"A\nB");
        }
        s.deinit();
    }

    // test "Screen: resize (no reflow) more rows with soft wrapping"
    #[test]
    fn screen_resize_no_reflow_more_rows_with_soft_wrapping() {
        let mut s = mkscreen(2, 3, 3);
        let str = b"1A2B\n3C4E\n5F6G";
        s.test_write_string(str).unwrap();

        // Every second row should be wrapped
        for y in 0..6 {
            let list_cell = s
                .pages
                .get_cell(point::Point::Screen(point::Coordinate {
                    x: 0,
                    y: y as u32,
                }))
                .unwrap();
            let wrapped = y % 2 == 0;
            assert_eq!(wrapped, unsafe { (*list_cell.row).wrap() });
        }

        // Resize
        s.resize(Resize {
            cols: 2,
            rows: 10,
            reflow: false,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"1A\n2B\n3C\n4E\n5F\n6G");
        }

        // Every second row should be wrapped
        for y in 0..6 {
            let list_cell = s
                .pages
                .get_cell(point::Point::Screen(point::Coordinate {
                    x: 0,
                    y: y as u32,
                }))
                .unwrap();
            let wrapped = y % 2 == 0;
            assert_eq!(wrapped, unsafe { (*list_cell.row).wrap() });
        }
        s.deinit();
    }

    // test "Screen: resize more rows no scrollback"
    #[test]
    fn screen_resize_more_rows_no_scrollback() {
        let mut s = mkscreen(5, 3, 0);
        let str = b"1ABCD\n2EFGH\n3IJKL";
        s.test_write_string(str).unwrap();
        let cursor = s.cursor;
        s.resize(Resize {
            cols: 5,
            rows: 10,
            reflow: true,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();

        // Cursor should not move
        assert_eq!(cursor.x, s.cursor.x);
        assert_eq!(cursor.y, s.cursor.y);

        {
            let contents = dump(&s, vp());
            assert_eq!(contents, str);
        }
        {
            let contents = dump(&s, scr());
            assert_eq!(contents, str);
        }
        s.deinit();
    }

    // test "Screen: resize more rows with empty scrollback"
    #[test]
    fn screen_resize_more_rows_with_empty_scrollback() {
        let mut s = mkscreen(5, 3, 10);
        let str = b"1ABCD\n2EFGH\n3IJKL";
        s.test_write_string(str).unwrap();
        let cursor = s.cursor;
        s.resize(Resize {
            cols: 5,
            rows: 10,
            reflow: true,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();

        // Cursor should not move
        assert_eq!(cursor.x, s.cursor.x);
        assert_eq!(cursor.y, s.cursor.y);

        {
            let contents = dump(&s, vp());
            assert_eq!(contents, str);
        }
        {
            let contents = dump(&s, scr());
            assert_eq!(contents, str);
        }
        s.deinit();
    }

    // test "Screen: resize more rows with populated scrollback"
    #[test]
    fn screen_resize_more_rows_with_populated_scrollback() {
        let mut s = mkscreen(5, 3, 5);
        let str = b"1ABCD\n2EFGH\n3IJKL\n4ABCD\n5EFGH";
        s.test_write_string(str).unwrap();
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"3IJKL\n4ABCD\n5EFGH");
        }

        // Set our cursor to be on the "4"
        s.cursor_absolute(0, 1);
        {
            let list_cell = s
                .pages
                .get_cell(point::Point::Active(point::Coordinate {
                    x: s.cursor.x,
                    y: s.cursor.y as u32,
                }))
                .unwrap();
            assert_eq!('4' as u21, unsafe { (*list_cell.cell).content_codepoint() });
        }

        // Resize
        s.resize(Resize {
            cols: 5,
            rows: 10,
            reflow: true,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();

        // Cursor should still be on the "4"
        {
            let list_cell = s
                .pages
                .get_cell(point::Point::Active(point::Coordinate {
                    x: s.cursor.x,
                    y: s.cursor.y as u32,
                }))
                .unwrap();
            assert_eq!('4' as u21, unsafe { (*list_cell.cell).content_codepoint() });
        }

        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"3IJKL\n4ABCD\n5EFGH");
        }
        s.deinit();
    }

    // test "Screen: resize more cols no reflow"
    #[test]
    fn screen_resize_more_cols_no_reflow() {
        let mut s = mkscreen(5, 3, 0);
        let str = b"1ABCD\n2EFGH\n3IJKL";
        s.test_write_string(str).unwrap();

        let cursor = s.cursor;
        s.resize(Resize {
            cols: 10,
            rows: 3,
            reflow: true,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();

        // Cursor should not move
        assert_eq!(cursor.x, s.cursor.x);
        assert_eq!(cursor.y, s.cursor.y);

        {
            let contents = dump(&s, vp());
            assert_eq!(contents, str);
        }
        {
            let contents = dump(&s, scr());
            assert_eq!(contents, str);
        }
        s.deinit();
    }

    // https://github.com/mitchellh/ghostty/issues/272#issuecomment-1676038963
    // test "Screen: resize more cols perfect split"
    #[test]
    fn screen_resize_more_cols_perfect_split() {
        let mut s = mkscreen(5, 3, 0);
        let str = b"1ABCD2EFGH3IJKL";
        s.test_write_string(str).unwrap();
        s.resize(Resize {
            cols: 10,
            rows: 3,
            reflow: true,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();

        {
            let contents = dump(&s, scr());
            assert_eq!(contents, b"1ABCD2EFGH\n3IJKL");
        }
        s.deinit();
    }

    // https://github.com/mitchellh/ghostty/issues/1159
    // test "Screen: resize (no reflow) more cols with scrollback scrolled up"
    #[test]
    fn screen_resize_no_reflow_more_cols_with_scrollback_scrolled_up() {
        let mut s = mkscreen(5, 3, 5);
        let str = b"1\n2\n3\n4\n5\n6\n7\n8";
        s.test_write_string(str).unwrap();

        // Cursor at bottom
        assert_eq!(1 as size::CellCountInt, s.cursor.x);
        assert_eq!(2 as size::CellCountInt, s.cursor.y);

        s.scroll(Scroll::DeltaRow(-4));
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"2\n3\n4");
        }

        s.resize(Resize {
            cols: 8,
            rows: 3,
            reflow: true,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();
        {
            let contents = dump(&s, scr());
            assert_eq!(contents, str);
        }

        // Cursor remains at bottom
        assert_eq!(1 as size::CellCountInt, s.cursor.x);
        assert_eq!(2 as size::CellCountInt, s.cursor.y);
        s.deinit();
    }

    // https://github.com/mitchellh/ghostty/issues/1159
    // test "Screen: resize (no reflow) less cols with scrollback scrolled up"
    #[test]
    fn screen_resize_no_reflow_less_cols_with_scrollback_scrolled_up() {
        let mut s = mkscreen(5, 3, 5);
        let str = b"1\n2\n3\n4\n5\n6\n7\n8";
        s.test_write_string(str).unwrap();

        // Cursor at bottom
        assert_eq!(1 as size::CellCountInt, s.cursor.x);
        assert_eq!(2 as size::CellCountInt, s.cursor.y);

        s.scroll(Scroll::DeltaRow(-4));
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"2\n3\n4");
        }

        s.resize(Resize {
            cols: 4,
            rows: 3,
            reflow: true,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();
        {
            let contents = dump(&s, scr());
            assert_eq!(contents, str);
        }
        {
            let contents = dump(&s, act());
            assert_eq!(contents, b"6\n7\n8");
        }

        // Cursor remains at bottom
        assert_eq!(1 as size::CellCountInt, s.cursor.x);
        assert_eq!(2 as size::CellCountInt, s.cursor.y);

        // Old implementation doesn't do this but it makes sense to me:
        // {
        //     const contents = try s.dumpStringAlloc(alloc, .{ .viewport = .{} });
        //     defer alloc.free(contents);
        //     try testing.expectEqualStrings("2\n3\n4", contents);
        // }
        s.deinit();
    }

    // test "Screen: resize more cols no reflow preserves semantic prompt"
    #[test]
    fn screen_resize_more_cols_no_reflow_preserves_semantic_prompt() {
        let mut s = mkscreen(5, 3, 0);

        // Set one of the rows to be a prompt
        s.cursor_set_semantic_content(SemanticContentArg::Output);
        s.test_write_string(b"1ABCD\n").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Prompt(
            osc::semantic_prompt::PromptKind::Initial,
        ));
        s.test_write_string(b"2EFGH").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Output);
        s.test_write_string(b"\n3IJKL").unwrap();

        s.resize(Resize {
            cols: 10,
            rows: 3,
            reflow: false,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();

        let expected = b"1ABCD\n2EFGH\n3IJKL";
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, expected);
        }
        {
            let contents = dump(&s, scr());
            assert_eq!(contents, expected);
        }

        // Our one row should still be a semantic prompt, the others should not.
        {
            let list_cell = s
                .pages
                .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
                .unwrap();
            assert!(unsafe { (*list_cell.row).semantic_prompt() } == pagepkg::SemanticPrompt::None);
        }
        {
            let list_cell = s
                .pages
                .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 1 }))
                .unwrap();
            assert!(
                unsafe { (*list_cell.row).semantic_prompt() } == pagepkg::SemanticPrompt::Prompt
            );
        }
        {
            let list_cell = s
                .pages
                .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 2 }))
                .unwrap();
            assert!(unsafe { (*list_cell.row).semantic_prompt() } == pagepkg::SemanticPrompt::None);
        }
        s.deinit();
    }

    // test "Screen: resize more cols with reflow that fits full width"
    #[test]
    fn screen_resize_more_cols_with_reflow_that_fits_full_width() {
        let mut s = mkscreen(5, 3, 0);
        let str = b"1ABCD2EFGH\n3IJKL";
        s.test_write_string(str).unwrap();

        // Verify we soft wrapped
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"1ABCD\n2EFGH\n3IJKL");
        }

        // Let's put our cursor on row 2, where the soft wrap is
        s.cursor_absolute(0, 1);
        {
            let list_cell = s
                .pages
                .get_cell(point::Point::Active(point::Coordinate {
                    x: s.cursor.x,
                    y: s.cursor.y as u32,
                }))
                .unwrap();
            assert_eq!('2' as u21, unsafe { (*list_cell.cell).content_codepoint() });
        }

        // Resize and verify we undid the soft wrap because we have space now
        s.resize(Resize {
            cols: 10,
            rows: 3,
            reflow: true,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, str);
        }

        // Our cursor should've moved
        assert_eq!(5, s.cursor.x);
        assert_eq!(0, s.cursor.y);
        s.deinit();
    }

    // test "Screen: resize more cols with reflow that ends in newline"
    #[test]
    fn screen_resize_more_cols_with_reflow_that_ends_in_newline() {
        let mut s = mkscreen(6, 3, 0);
        let str = b"1ABCD2EFGH\n3IJKL";
        s.test_write_string(str).unwrap();

        // Verify we soft wrapped
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"1ABCD2\nEFGH\n3IJKL");
        }

        // Let's put our cursor on the last row
        s.cursor_absolute(0, 2);
        {
            let list_cell = s
                .pages
                .get_cell(point::Point::Active(point::Coordinate {
                    x: s.cursor.x,
                    y: s.cursor.y as u32,
                }))
                .unwrap();
            assert_eq!('3' as u21, unsafe { (*list_cell.cell).content_codepoint() });
        }

        // Resize and verify we undid the soft wrap because we have space now
        s.resize(Resize {
            cols: 10,
            rows: 3,
            reflow: true,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, str);
        }

        // Our cursor should still be on the 3
        {
            let list_cell = s
                .pages
                .get_cell(point::Point::Active(point::Coordinate {
                    x: s.cursor.x,
                    y: s.cursor.y as u32,
                }))
                .unwrap();
            assert_eq!('3' as u21, unsafe { (*list_cell.cell).content_codepoint() });
        }
        s.deinit();
    }

    // test "Screen: resize more cols with reflow that forces more wrapping"
    #[test]
    fn screen_resize_more_cols_with_reflow_that_forces_more_wrapping() {
        let mut s = mkscreen(5, 3, 0);
        let str = b"1ABCD2EFGH\n3IJKL";
        s.test_write_string(str).unwrap();

        // Let's put our cursor on row 2, where the soft wrap is
        s.cursor_absolute(0, 1);
        {
            let list_cell = s
                .pages
                .get_cell(point::Point::Active(point::Coordinate {
                    x: s.cursor.x,
                    y: s.cursor.y as u32,
                }))
                .unwrap();
            assert_eq!('2' as u21, unsafe { (*list_cell.cell).content_codepoint() });
        }

        // Verify we soft wrapped
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"1ABCD\n2EFGH\n3IJKL");
        }

        // Resize and verify we undid the soft wrap because we have space now
        s.resize(Resize {
            cols: 7,
            rows: 3,
            reflow: true,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"1ABCD2E\nFGH\n3IJKL");
        }

        // Our cursor should've moved
        assert_eq!(5 as size::CellCountInt, s.cursor.x);
        assert_eq!(0 as size::CellCountInt, s.cursor.y);
        s.deinit();
    }

    // test "Screen: resize more cols with reflow that unwraps multiple times"
    #[test]
    fn screen_resize_more_cols_with_reflow_that_unwraps_multiple_times() {
        let mut s = mkscreen(5, 3, 0);
        let str = b"1ABCD2EFGH3IJKL";
        s.test_write_string(str).unwrap();

        // Let's put our cursor on row 2, where the soft wrap is
        s.cursor_absolute(0, 2);
        {
            let list_cell = s
                .pages
                .get_cell(point::Point::Active(point::Coordinate {
                    x: s.cursor.x,
                    y: s.cursor.y as u32,
                }))
                .unwrap();
            assert_eq!('3' as u21, unsafe { (*list_cell.cell).content_codepoint() });
        }

        // Verify we soft wrapped
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"1ABCD\n2EFGH\n3IJKL");
        }

        // Resize and verify we undid the soft wrap because we have space now
        s.resize(Resize {
            cols: 15,
            rows: 3,
            reflow: true,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"1ABCD2EFGH3IJKL");
        }

        // Our cursor should've moved
        assert_eq!(10 as size::CellCountInt, s.cursor.x);
        assert_eq!(0 as size::CellCountInt, s.cursor.y);
        s.deinit();
    }

    // test "Screen: resize more cols with populated scrollback"
    #[test]
    fn screen_resize_more_cols_with_populated_scrollback() {
        let mut s = mkscreen(5, 3, 5);
        let str = b"1ABCD\n2EFGH\n3IJKL\n4ABCD5EFGH";
        s.test_write_string(str).unwrap();
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"3IJKL\n4ABCD\n5EFGH");
        }

        // // Set our cursor to be on the "5"
        s.cursor_absolute(0, 2);
        {
            let list_cell = s
                .pages
                .get_cell(point::Point::Active(point::Coordinate {
                    x: s.cursor.x,
                    y: s.cursor.y as u32,
                }))
                .unwrap();
            assert_eq!('5' as u21, unsafe { (*list_cell.cell).content_codepoint() });
        }

        // Resize
        s.resize(Resize {
            cols: 10,
            rows: 3,
            reflow: true,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"2EFGH\n3IJKL\n4ABCD5EFGH");
        }

        // Cursor should still be on the "5"
        {
            let list_cell = s
                .pages
                .get_cell(point::Point::Active(point::Coordinate {
                    x: s.cursor.x,
                    y: s.cursor.y as u32,
                }))
                .unwrap();
            assert_eq!('5' as u21, unsafe { (*list_cell.cell).content_codepoint() });
        }
        s.deinit();
    }

    // test "Screen: resize more cols bounded scrollback keeps viewport valid"
    #[test]
    fn screen_resize_more_cols_bounded_scrollback_keeps_viewport_valid() {
        // Regression test for issue #12298.
        //
        // This needs to live at the Screen layer rather than PageList because the
        // bad state only appears once Screen forwards the active cursor into the
        // resize path. A direct PageList resize repro does not hit the same bug.
        let mut s = mkscreen(2, 10, 10_000);

        // Build 30 rows of scrollback on top of our 10-row viewport so we have a
        // 40-row screen with history above the active area.
        for _ in 0..30 {
            let _ = s.pages.grow().unwrap();
        }
        s.cursor_reload();
        assert_eq!(40usize, s.pages.scrollbar().total);

        // Fill the entire screen with two-row wrapped runs:
        // - even rows mark the end of a wrapped line
        // - odd rows mark the continuation
        //
        // With 2 columns, each logical line occupies two rows. When we grow to 4
        // columns with reflow enabled, those pairs unwrap back into single rows.
        // That cuts the total row count down and is what stresses the viewport pin.
        let cols = s.pages.cols;
        let mut it = s
            .pages
            .page_iterator(pagelist::iter::Direction::RightDown, scr(), None);
        while let Some(chunk) = it.next() {
            let page: *mut Page = unsafe { core::ptr::addr_of_mut!((*chunk.node).data) };
            for y in chunk.start..chunk.end {
                let rac = unsafe { (*page).get_row_and_cell(0, y as usize) };
                if y % 2 == 0 {
                    unsafe { (*rac.row).set_wrap(true) };
                } else {
                    unsafe { (*rac.row).set_wrap_continuation(true) };
                }

                for x in 0..cols {
                    let rac = unsafe { (*page).get_row_and_cell(x as usize, y as usize) };
                    unsafe { (*rac.cell).set_content_codepoint('A' as u21) };
                }
            }
        }

        // Pin the viewport to a history row just above the active area.
        //
        // Before resize:
        // - total rows = 40
        // - active area starts at row 30
        // - viewport is pinned at row 28
        //
        // After unwrap during resize:
        // - total rows shrinks to 20
        // - the old row 28 remaps into what is now the active area
        //
        // The bug was that resize/grow would temporarily keep the viewport as a
        // history pin even after reflow had moved it into the active area, leaving
        // fewer than `rows` visible rows beneath the pin and tripping integrity
        // checks.
        s.pages.scroll(pagelist::scroll::Scroll::Pin(
            s.pages
                .pin(point::Point::Screen(point::Coordinate { x: 0, y: 28 }))
                .unwrap(),
        ));
        assert!(s.pages.viewport == pagelist::Viewport::Pin);
        assert!(s.pages.get_bottom_right(point::Tag::Viewport).is_some());

        // Growing columns triggers reflow, which unwraps the synthetic wrapped
        // rows above. This used to panic during the resize path.
        let rows = s.pages.rows;
        s.resize(Resize {
            cols: 4,
            rows,
            reflow: true,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();

        // After the fix, the viewport is normalized back to the active area as
        // soon as the pinned row lands there, so viewport queries remain valid.
        assert_eq!(4usize, s.pages.cols as usize);
        assert!(s.pages.scrollbar().total < 40);
        assert!(s.pages.viewport == pagelist::Viewport::Active);
        assert!(s.pages.get_bottom_right(point::Tag::Viewport).is_some());
        s.deinit();
    }

    // test "Screen: resize more cols with reflow"
    #[test]
    fn screen_resize_more_cols_with_reflow() {
        let mut s = mkscreen(2, 3, 5);
        let str = b"1ABC\n2DEF\n3ABC\n4DEF";
        s.test_write_string(str).unwrap();

        // Let's put our cursor on row 2, where the soft wrap is
        s.cursor_absolute(0, 2);
        {
            let list_cell = s
                .pages
                .get_cell(point::Point::Active(point::Coordinate {
                    x: s.cursor.x,
                    y: s.cursor.y as u32,
                }))
                .unwrap();
            assert_eq!('E' as u32, unsafe {
                (*list_cell.cell).content_codepoint() as u32
            });
        }

        // Verify we soft wrapped
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"BC\n4D\nEF");
        }

        // Resize and verify we undid the soft wrap because we have space now
        s.resize(Resize {
            cols: 7,
            rows: 3,
            reflow: true,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();
        {
            let contents = dump(&s, scr());
            assert_eq!(contents, b"1ABC\n2DEF\n3ABC\n4DEF");
        }

        // Our cursor should've moved
        assert_eq!(2 as size::CellCountInt, s.cursor.x);
        assert_eq!(2 as size::CellCountInt, s.cursor.y);
        s.deinit();
    }

    // test "Screen: resize more rows and cols with wrapping"
    #[test]
    fn screen_resize_more_rows_and_cols_with_wrapping() {
        let mut s = mkscreen(2, 4, 0);
        let str = b"1A2B\n3C4D";
        s.test_write_string(str).unwrap();
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"1A\n2B\n3C\n4D");
        }

        s.resize(Resize {
            cols: 5,
            rows: 10,
            reflow: true,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();

        // Cursor should move due to wrapping
        assert_eq!(3 as size::CellCountInt, s.cursor.x);
        assert_eq!(1 as size::CellCountInt, s.cursor.y);

        {
            let contents = dump(&s, vp());
            assert_eq!(contents, str);
        }
        {
            let contents = dump(&s, scr());
            assert_eq!(contents, str);
        }
        s.deinit();
    }

    // test "Screen: resize less rows no scrollback"
    #[test]
    fn screen_resize_less_rows_no_scrollback() {
        let mut s = mkscreen(5, 3, 0);
        let str = b"1ABCD\n2EFGH\n3IJKL";
        s.test_write_string(str).unwrap();

        s.cursor_absolute(0, 0);
        let cursor = s.cursor;
        s.resize(Resize {
            cols: 5,
            rows: 1,
            reflow: true,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();

        // Cursor should not move
        assert_eq!(cursor.x, s.cursor.x);
        assert_eq!(cursor.y, s.cursor.y);

        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"3IJKL");
        }
        {
            let contents = dump(&s, scr());
            assert_eq!(contents, b"3IJKL");
        }
        s.deinit();
    }

    // test "Screen: resize less rows moving cursor"
    #[test]
    fn screen_resize_less_rows_moving_cursor() {
        let mut s = mkscreen(5, 3, 0);
        let str = b"1ABCD\n2EFGH\n3IJKL";
        s.test_write_string(str).unwrap();

        // Put our cursor on the last line
        s.cursor_absolute(1, 2);
        {
            let list_cell = s
                .pages
                .get_cell(point::Point::Active(point::Coordinate {
                    x: s.cursor.x,
                    y: s.cursor.y as u32,
                }))
                .unwrap();
            assert_eq!('I' as u32, unsafe {
                (*list_cell.cell).content_codepoint() as u32
            });
        }

        // Resize
        s.resize(Resize {
            cols: 5,
            rows: 1,
            reflow: true,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();

        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"3IJKL");
        }
        {
            let contents = dump(&s, scr());
            assert_eq!(contents, b"3IJKL");
        }

        // Cursor should be on the last line
        assert_eq!(1 as size::CellCountInt, s.cursor.x);
        assert_eq!(0 as size::CellCountInt, s.cursor.y);
        s.deinit();
    }

    // test "Screen: resize less rows with empty scrollback"
    #[test]
    fn screen_resize_less_rows_with_empty_scrollback() {
        let mut s = mkscreen(5, 3, 10);
        let str = b"1ABCD\n2EFGH\n3IJKL";
        s.test_write_string(str).unwrap();
        s.resize(Resize {
            cols: 5,
            rows: 1,
            reflow: true,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();

        {
            let contents = dump(&s, scr());
            assert_eq!(contents, str);
        }
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"3IJKL");
        }
        s.deinit();
    }

    // test "Screen: resize less rows with populated scrollback"
    #[test]
    fn screen_resize_less_rows_with_populated_scrollback() {
        let mut s = mkscreen(5, 3, 5);
        let str = b"1ABCD\n2EFGH\n3IJKL\n4ABCD\n5EFGH";
        s.test_write_string(str).unwrap();
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"3IJKL\n4ABCD\n5EFGH");
        }

        // Resize
        s.resize(Resize {
            cols: 5,
            rows: 1,
            reflow: true,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();

        {
            let contents = dump(&s, scr());
            assert_eq!(contents, str);
        }
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"5EFGH");
        }
        s.deinit();
    }

    // test "Screen: resize less rows with full scrollback"
    #[test]
    fn screen_resize_less_rows_with_full_scrollback() {
        let mut s = mkscreen(5, 3, 3);
        let str = b"00000\n1ABCD\n2EFGH\n3IJKL\n4ABCD\n5EFGH";
        s.test_write_string(str).unwrap();
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"3IJKL\n4ABCD\n5EFGH");
        }

        assert_eq!(4 as size::CellCountInt, s.cursor.x);
        assert_eq!(2 as size::CellCountInt, s.cursor.y);

        // Resize
        s.resize(Resize {
            cols: 5,
            rows: 2,
            reflow: true,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();

        // Cursor should stay in the same relative place (bottom of the
        // screen, same character).
        assert_eq!(4 as size::CellCountInt, s.cursor.x);
        assert_eq!(1 as size::CellCountInt, s.cursor.y);

        {
            let contents = dump(&s, scr());
            assert_eq!(contents, b"00000\n1ABCD\n2EFGH\n3IJKL\n4ABCD\n5EFGH");
        }
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"4ABCD\n5EFGH");
        }
        s.deinit();
    }

    // test "Screen: resize less cols no reflow"
    #[test]
    fn screen_resize_less_cols_no_reflow() {
        let mut s = mkscreen(5, 3, 0);
        let str = b"1AB\n2EF\n3IJ";
        s.test_write_string(str).unwrap();

        s.cursor_absolute(0, 0);
        let cursor = s.cursor;
        s.resize(Resize {
            cols: 3,
            rows: 3,
            reflow: true,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();

        // Cursor should not move
        assert_eq!(cursor.x, s.cursor.x);
        assert_eq!(cursor.y, s.cursor.y);

        {
            let contents = dump(&s, vp());
            assert_eq!(contents, str);
        }
        {
            let contents = dump(&s, scr());
            assert_eq!(contents, str);
        }
        s.deinit();
    }

    // test "Screen: resize less cols with reflow but row space"
    #[test]
    fn screen_resize_less_cols_with_reflow_but_row_space() {
        let mut s = mkscreen(5, 3, 1);
        let str = b"1ABCD";
        s.test_write_string(str).unwrap();

        // Put our cursor on the end
        s.cursor_absolute(4, 0);
        {
            let list_cell = s
                .pages
                .get_cell(point::Point::Active(point::Coordinate {
                    x: s.cursor.x,
                    y: s.cursor.y as u32,
                }))
                .unwrap();
            assert_eq!('D' as u32, unsafe {
                (*list_cell.cell).content_codepoint() as u32
            });
        }

        s.resize(Resize {
            cols: 3,
            rows: 3,
            reflow: true,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"1AB\nCD");
        }
        {
            let contents = dump(&s, scr());
            assert_eq!(contents, b"1AB\nCD");
        }

        // Cursor should be on the last line
        assert_eq!(1 as size::CellCountInt, s.cursor.x);
        assert_eq!(1 as size::CellCountInt, s.cursor.y);
        s.deinit();
    }

    // test "Screen: resize less cols with reflow with trimmed rows"
    #[test]
    fn screen_resize_less_cols_with_reflow_with_trimmed_rows() {
        let mut s = mkscreen(5, 3, 0);
        let str = b"3IJKL\n4ABCD\n5EFGH";
        s.test_write_string(str).unwrap();
        s.resize(Resize {
            cols: 3,
            rows: 3,
            reflow: true,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();

        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"CD\n5EF\nGH");
        }
        {
            let contents = dump(&s, scr());
            assert_eq!(contents, b"CD\n5EF\nGH");
        }
        s.deinit();
    }

    // test "Screen: resize less cols with reflow with trimmed rows and scrollback"
    #[test]
    fn screen_resize_less_cols_with_reflow_with_trimmed_rows_and_scrollback() {
        let mut s = mkscreen(5, 3, 1);
        let str = b"3IJKL\n4ABCD\n5EFGH";
        s.test_write_string(str).unwrap();
        s.resize(Resize {
            cols: 3,
            rows: 3,
            reflow: true,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();

        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"CD\n5EF\nGH");
        }
        {
            let contents = dump(&s, scr());
            assert_eq!(contents, b"3IJ\nKL\n4AB\nCD\n5EF\nGH");
        }
        s.deinit();
    }

    // test "Screen: resize less cols with reflow previously wrapped"
    #[test]
    fn screen_resize_less_cols_with_reflow_previously_wrapped() {
        let mut s = mkscreen(5, 3, 0);
        let str = b"3IJKL4ABCD5EFGH";
        s.test_write_string(str).unwrap();

        // Check
        {
            let contents = dump(&s, scr());
            assert_eq!(contents, b"3IJKL\n4ABCD\n5EFGH");
        }

        s.resize(Resize {
            cols: 3,
            rows: 3,
            reflow: true,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();

        // {
        //     const contents = try s.testString(alloc, .viewport);
        //     defer alloc.free(contents);
        //     const expected = "CD\n5EF\nGH";
        //     try testing.expectEqualStrings(expected, contents);
        // }
        {
            let contents = dump(&s, scr());
            assert_eq!(contents, b"ABC\nD5E\nFGH");
        }
        s.deinit();
    }

    // test "Screen: resize less cols with reflow and scrollback"
    #[test]
    fn screen_resize_less_cols_with_reflow_and_scrollback() {
        let mut s = mkscreen(5, 3, 5);
        let str = b"1A\n2B\n3C\n4D\n5E";
        s.test_write_string(str).unwrap();

        // Put our cursor on the end
        s.cursor_absolute(1, s.pages.rows - 1);
        {
            let list_cell = s
                .pages
                .get_cell(point::Point::Active(point::Coordinate {
                    x: s.cursor.x,
                    y: s.cursor.y as u32,
                }))
                .unwrap();
            assert_eq!('E' as u32, unsafe {
                (*list_cell.cell).content_codepoint() as u32
            });
        }

        s.resize(Resize {
            cols: 3,
            rows: 3,
            reflow: true,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();

        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"3C\n4D\n5E");
        }

        // Cursor should be on the last line
        assert_eq!(1 as size::CellCountInt, s.cursor.x);
        assert_eq!(2 as size::CellCountInt, s.cursor.y);
        s.deinit();
    }

    // test "Screen: resize less cols with reflow previously wrapped and scrollback"
    #[test]
    fn screen_resize_less_cols_with_reflow_previously_wrapped_and_scrollback() {
        let mut s = mkscreen(5, 3, 2);
        let str = b"1ABCD2EFGH3IJKL4ABCD5EFGH";
        s.test_write_string(str).unwrap();

        // Check
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"3IJKL\n4ABCD\n5EFGH");
        }

        // Put our cursor on the end
        s.cursor_absolute(s.pages.cols - 1, s.pages.rows - 1);
        {
            let list_cell = s
                .pages
                .get_cell(point::Point::Active(point::Coordinate {
                    x: s.cursor.x,
                    y: s.cursor.y as u32,
                }))
                .unwrap();
            assert_eq!('H' as u32, unsafe {
                (*list_cell.cell).content_codepoint() as u32
            });
        }

        s.resize(Resize {
            cols: 3,
            rows: 3,
            reflow: true,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();

        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"CD5\nEFG\nH");
        }
        {
            let contents = dump(&s, scr());
            assert_eq!(contents, b"1AB\nCD2\nEFG\nH3I\nJKL\n4AB\nCD5\nEFG\nH");
        }

        // Cursor should be on the last line
        assert_eq!(0 as size::CellCountInt, s.cursor.x);
        assert_eq!(2 as size::CellCountInt, s.cursor.y);
        {
            let list_cell = s
                .pages
                .get_cell(point::Point::Active(point::Coordinate {
                    x: s.cursor.x,
                    y: s.cursor.y as u32,
                }))
                .unwrap();
            assert_eq!('H' as u32, unsafe {
                (*list_cell.cell).content_codepoint() as u32
            });
        }
        s.deinit();
    }

    // test "Screen: resize less cols with scrollback keeps cursor row"
    #[test]
    fn screen_resize_less_cols_with_scrollback_keeps_cursor_row() {
        let mut s = mkscreen(5, 3, 5);
        let str = b"1A\n2B\n3C\n4D\n5E";
        s.test_write_string(str).unwrap();

        // Lets do a scroll and clear operation
        s.scroll_clear().unwrap();

        // Move our cursor to the beginning
        s.cursor_absolute(0, 0);

        s.resize(Resize {
            cols: 3,
            rows: 3,
            reflow: true,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();

        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"");
        }

        // Cursor should be on the last line
        assert_eq!(0 as size::CellCountInt, s.cursor.x);
        assert_eq!(0 as size::CellCountInt, s.cursor.y);
        s.deinit();
    }

    // test "Screen: resize more rows, less cols with reflow with scrollback"
    #[test]
    fn screen_resize_more_rows_less_cols_with_reflow_with_scrollback() {
        let mut s = mkscreen(5, 3, 3);
        let str = b"1ABCD\n2EFGH3IJKL\n4MNOP";
        s.test_write_string(str).unwrap();

        {
            let contents = dump(&s, scr());
            assert_eq!(contents, b"1ABCD\n2EFGH\n3IJKL\n4MNOP");
        }
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"2EFGH\n3IJKL\n4MNOP");
        }

        s.resize(Resize {
            cols: 2,
            rows: 10,
            reflow: true,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();

        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"BC\nD\n2E\nFG\nH3\nIJ\nKL\n4M\nNO\nP");
        }
        {
            let contents = dump(&s, scr());
            assert_eq!(contents, b"1A\nBC\nD\n2E\nFG\nH3\nIJ\nKL\n4M\nNO\nP");
        }
        s.deinit();
    }

    // This seems like it should work fine but for some reason in practice
    // in the initial implementation I found this bug! This is a regression
    // test for that.
    // test "Screen: resize more rows then shrink again"
    #[test]
    fn screen_resize_more_rows_then_shrink_again() {
        let mut s = mkscreen(5, 3, 10);
        let str = b"1ABC";
        s.test_write_string(str).unwrap();

        // Grow
        s.resize(Resize {
            cols: 5,
            rows: 10,
            reflow: true,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, str);
        }
        {
            let contents = dump(&s, scr());
            assert_eq!(contents, str);
        }

        // Shrink
        s.resize(Resize {
            cols: 5,
            rows: 3,
            reflow: true,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();
        {
            let contents = dump(&s, scr());
            assert_eq!(contents, str);
        }
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, str);
        }

        // Grow again
        s.resize(Resize {
            cols: 5,
            rows: 10,
            reflow: true,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, str);
        }
        {
            let contents = dump(&s, scr());
            assert_eq!(contents, str);
        }
        s.deinit();
    }

    // test "Screen: resize less cols to eliminate wide char"
    #[test]
    fn screen_resize_less_cols_to_eliminate_wide_char() {
        let mut s = mkscreen(2, 1, 0);
        let str = "😀".as_bytes();
        s.test_write_string(str).unwrap();
        {
            let contents = dump(&s, scr());
            assert_eq!(contents, str);
        }
        {
            let list_cell = s
                .pages
                .get_cell(point::Point::Screen(point::Coordinate { x: 0, y: 0 }))
                .unwrap();
            assert_eq!(pagepkg::Wide::Wide, unsafe { (*list_cell.cell).wide() });
            assert_eq!('😀' as u21, unsafe {
                (*list_cell.cell).content_codepoint()
            });
        }

        // Resize to 1 column can't fit a wide char. So it should be deleted.
        s.resize(Resize {
            cols: 1,
            rows: 1,
            reflow: true,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();
        {
            let contents = dump(&s, scr());
            assert_eq!(contents, b"");
        }
        {
            let list_cell = s
                .pages
                .get_cell(point::Point::Screen(point::Coordinate { x: 0, y: 0 }))
                .unwrap();
            assert_eq!(0 as u21, unsafe { (*list_cell.cell).content_codepoint() });
            assert_eq!(pagepkg::Wide::Narrow, unsafe { (*list_cell.cell).wide() });
        }
        s.deinit();
    }

    // test "Screen: resize less cols to wrap wide char"
    #[test]
    fn screen_resize_less_cols_to_wrap_wide_char() {
        let mut s = mkscreen(3, 3, 0);
        let str = "x😀".as_bytes();
        s.test_write_string(str).unwrap();
        {
            let contents = dump(&s, scr());
            assert_eq!(contents, str);
        }
        {
            let list_cell = s
                .pages
                .get_cell(point::Point::Screen(point::Coordinate { x: 1, y: 0 }))
                .unwrap();
            assert_eq!(pagepkg::Wide::Wide, unsafe { (*list_cell.cell).wide() });
            assert_eq!('😀' as u21, unsafe {
                (*list_cell.cell).content_codepoint()
            });
        }
        {
            let list_cell = s
                .pages
                .get_cell(point::Point::Screen(point::Coordinate { x: 2, y: 0 }))
                .unwrap();
            assert_eq!(pagepkg::Wide::SpacerTail, unsafe {
                (*list_cell.cell).wide()
            });
        }

        s.resize(Resize {
            cols: 2,
            rows: 3,
            reflow: true,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();
        {
            let contents = dump(&s, scr());
            assert_eq!(contents, "x\n😀".as_bytes());
        }
        {
            let list_cell = s
                .pages
                .get_cell(point::Point::Screen(point::Coordinate { x: 1, y: 0 }))
                .unwrap();
            assert_eq!(pagepkg::Wide::SpacerHead, unsafe {
                (*list_cell.cell).wide()
            });
            assert!(unsafe { (*list_cell.row).wrap() });
        }
        s.deinit();
    }

    // test "Screen: resize less cols to eliminate wide char with row space"
    #[test]
    fn screen_resize_less_cols_to_eliminate_wide_char_with_row_space() {
        let mut s = mkscreen(2, 2, 0);
        let str = "😀".as_bytes();
        s.test_write_string(str).unwrap();
        {
            let contents = dump(&s, scr());
            assert_eq!(contents, str);
        }
        {
            let list_cell = s
                .pages
                .get_cell(point::Point::Screen(point::Coordinate { x: 0, y: 0 }))
                .unwrap();
            assert_eq!(pagepkg::Wide::Wide, unsafe { (*list_cell.cell).wide() });
            assert_eq!('😀' as u21, unsafe {
                (*list_cell.cell).content_codepoint()
            });
        }
        {
            let list_cell = s
                .pages
                .get_cell(point::Point::Screen(point::Coordinate { x: 1, y: 0 }))
                .unwrap();
            assert_eq!(pagepkg::Wide::SpacerTail, unsafe {
                (*list_cell.cell).wide()
            });
        }

        s.resize(Resize {
            cols: 1,
            rows: 2,
            reflow: true,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();
        {
            let contents = dump(&s, scr());
            assert_eq!(contents, b"");
        }
        s.deinit();
    }

    // test "Screen: resize less cols reflows cursor after wrapped text"
    #[test]
    fn screen_resize_less_cols_reflows_cursor_after_wrapped_text() {
        let mut s = mkscreen(50, 7, 0);

        for _ in 0..30 {
            s.test_write_string(b"a").unwrap();
        }

        assert_eq!(0, s.cursor.y);
        assert_eq!(30, s.cursor.x);

        s.resize(Resize {
            cols: 25,
            rows: 7,
            reflow: true,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();

        assert_eq!(1, s.cursor.y);
        assert_eq!(5, s.cursor.x);
        s.deinit();
    }

    // test "Screen: resize less cols reflows cursor after empty cells"
    #[test]
    fn screen_resize_less_cols_reflows_cursor_after_empty_cells() {
        let mut s = mkscreen(10, 3, 0);

        s.test_write_string(b"abc").unwrap();
        s.cursor_right(6);

        assert_eq!(0, s.cursor.y);
        assert_eq!(9, s.cursor.x);

        s.resize(Resize {
            cols: 5,
            rows: 3,
            reflow: true,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();

        assert_eq!(1, s.cursor.y);
        assert_eq!(4, s.cursor.x);
        s.deinit();
    }

    // test "Screen: resize more cols with wide spacer head"
    #[test]
    fn screen_resize_more_cols_with_wide_spacer_head() {
        let mut s = mkscreen(3, 2, 0);
        let str = "  😀".as_bytes();
        s.test_write_string(str).unwrap();
        {
            let contents = dump(&s, scr());
            assert_eq!(contents, "  \n😀".as_bytes());
        }

        // So this is the key point: we end up with a wide spacer head at
        // the end of row 1, then the emoji, then a wide spacer tail on row 2.
        // We should expect that if we resize to more cols, the wide spacer
        // head is replaced with the emoji.
        {
            let list_cell = s
                .pages
                .get_cell(point::Point::Screen(point::Coordinate { x: 2, y: 0 }))
                .unwrap();
            assert_eq!(pagepkg::Wide::SpacerHead, unsafe {
                (*list_cell.cell).wide()
            });
        }
        {
            let list_cell = s
                .pages
                .get_cell(point::Point::Screen(point::Coordinate { x: 0, y: 1 }))
                .unwrap();
            assert_eq!(pagepkg::Wide::Wide, unsafe { (*list_cell.cell).wide() });
        }
        {
            let list_cell = s
                .pages
                .get_cell(point::Point::Screen(point::Coordinate { x: 1, y: 1 }))
                .unwrap();
            assert_eq!(pagepkg::Wide::SpacerTail, unsafe {
                (*list_cell.cell).wide()
            });
        }

        s.resize(Resize {
            cols: 4,
            rows: 2,
            reflow: true,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();
        {
            let contents = dump(&s, scr());
            assert_eq!(contents, str);
        }
        {
            let list_cell = s
                .pages
                .get_cell(point::Point::Screen(point::Coordinate { x: 2, y: 0 }))
                .unwrap();
            assert_eq!(pagepkg::Wide::Wide, unsafe { (*list_cell.cell).wide() });
            assert_eq!('😀' as u21, unsafe {
                (*list_cell.cell).content_codepoint()
            });
        }
        {
            let list_cell = s
                .pages
                .get_cell(point::Point::Screen(point::Coordinate { x: 3, y: 0 }))
                .unwrap();
            assert_eq!(pagepkg::Wide::SpacerTail, unsafe {
                (*list_cell.cell).wide()
            });
        }
        s.deinit();
    }

    // test "Screen: resize more cols with wide spacer head multiple lines"
    #[test]
    fn screen_resize_more_cols_with_wide_spacer_head_multiple_lines() {
        let mut s = mkscreen(3, 3, 0);
        let str = "xxxyy😀".as_bytes();
        s.test_write_string(str).unwrap();
        {
            let contents = dump(&s, scr());
            assert_eq!(contents, "xxx\nyy\n😀".as_bytes());
        }

        // Similar to the "wide spacer head" test, but this time we'er going
        // to increase our columns such that multiple rows are unwrapped.
        {
            let list_cell = s
                .pages
                .get_cell(point::Point::Screen(point::Coordinate { x: 2, y: 1 }))
                .unwrap();
            assert_eq!(pagepkg::Wide::SpacerHead, unsafe {
                (*list_cell.cell).wide()
            });
        }
        {
            let list_cell = s
                .pages
                .get_cell(point::Point::Screen(point::Coordinate { x: 0, y: 2 }))
                .unwrap();
            assert_eq!(pagepkg::Wide::Wide, unsafe { (*list_cell.cell).wide() });
        }
        {
            let list_cell = s
                .pages
                .get_cell(point::Point::Screen(point::Coordinate { x: 1, y: 2 }))
                .unwrap();
            assert_eq!(pagepkg::Wide::SpacerTail, unsafe {
                (*list_cell.cell).wide()
            });
        }

        s.resize(Resize {
            cols: 8,
            rows: 2,
            reflow: true,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();
        {
            let contents = dump(&s, scr());
            assert_eq!(contents, str);
        }
        {
            let list_cell = s
                .pages
                .get_cell(point::Point::Screen(point::Coordinate { x: 5, y: 0 }))
                .unwrap();
            assert_eq!(pagepkg::Wide::Wide, unsafe { (*list_cell.cell).wide() });
            assert_eq!('😀' as u21, unsafe {
                (*list_cell.cell).content_codepoint()
            });
        }
        {
            let list_cell = s
                .pages
                .get_cell(point::Point::Screen(point::Coordinate { x: 6, y: 0 }))
                .unwrap();
            assert_eq!(pagepkg::Wide::SpacerTail, unsafe {
                (*list_cell.cell).wide()
            });
        }
        s.deinit();
    }

    // test "Screen: resize more cols requiring a wide spacer head"
    #[test]
    fn screen_resize_more_cols_requiring_a_wide_spacer_head() {
        let mut s = mkscreen(2, 2, 0);
        let str = "xx😀".as_bytes();
        s.test_write_string(str).unwrap();
        {
            let contents = dump(&s, scr());
            assert_eq!(contents, "xx\n😀".as_bytes());
        }
        {
            let list_cell = s
                .pages
                .get_cell(point::Point::Screen(point::Coordinate { x: 0, y: 1 }))
                .unwrap();
            assert_eq!(pagepkg::Wide::Wide, unsafe { (*list_cell.cell).wide() });
        }
        {
            let list_cell = s
                .pages
                .get_cell(point::Point::Screen(point::Coordinate { x: 1, y: 1 }))
                .unwrap();
            assert_eq!(pagepkg::Wide::SpacerTail, unsafe {
                (*list_cell.cell).wide()
            });
        }

        // This resizes to 3 columns, which isn't enough space for our wide
        // char to enter row 1. But we need to mark the wide spacer head on the
        // end of the first row since we're wrapping to the next row.
        s.resize(Resize {
            cols: 3,
            rows: 2,
            reflow: true,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();
        {
            let contents = dump(&s, scr());
            assert_eq!(contents, "xx\n😀".as_bytes());
        }
        {
            let list_cell = s
                .pages
                .get_cell(point::Point::Screen(point::Coordinate { x: 2, y: 0 }))
                .unwrap();
            assert_eq!(pagepkg::Wide::SpacerHead, unsafe {
                (*list_cell.cell).wide()
            });
        }
        {
            let list_cell = s
                .pages
                .get_cell(point::Point::Screen(point::Coordinate { x: 0, y: 1 }))
                .unwrap();
            assert_eq!(pagepkg::Wide::Wide, unsafe { (*list_cell.cell).wide() });
            assert_eq!('😀' as u21, unsafe {
                (*list_cell.cell).content_codepoint()
            });
        }
        {
            let list_cell = s
                .pages
                .get_cell(point::Point::Screen(point::Coordinate { x: 1, y: 1 }))
                .unwrap();
            assert_eq!(pagepkg::Wide::SpacerTail, unsafe {
                (*list_cell.cell).wide()
            });
        }
        s.deinit();
    }

    // test "Screen: resize more cols with cursor at prompt"
    #[test]
    fn screen_resize_more_cols_with_cursor_at_prompt() {
        let mut s = mkscreen(10, 3, 5);

        s.test_write_string(b"ABCDE\n").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Prompt(
            osc::semantic_prompt::PromptKind::Initial,
        ));
        s.test_write_string(b"> ").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Input(SemanticContentInput::ClearEol));
        s.test_write_string(b"echo").unwrap();

        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"ABCDE\n> echo");
        }

        s.resize(Resize {
            cols: 20,
            rows: 3,
            reflow: true,
            prompt_redraw: osc::semantic_prompt::Redraw::True,
        })
        .unwrap();

        // Cursor should not move
        assert_eq!(6, s.cursor.x);
        assert_eq!(1, s.cursor.y);

        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"ABCDE");
        }
        s.deinit();
    }

    // test "Screen: resize more cols with cursor not at prompt"
    #[test]
    fn screen_resize_more_cols_with_cursor_not_at_prompt() {
        let mut s = mkscreen(10, 3, 5);

        s.test_write_string(b"ABCDE\n").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Prompt(
            osc::semantic_prompt::PromptKind::Initial,
        ));
        s.test_write_string(b"> ").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Input(SemanticContentInput::ClearEol));
        s.test_write_string(b"echo\n").unwrap();
        s.test_write_string(b"output").unwrap();

        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"ABCDE\n> echo\noutput");
        }

        s.resize(Resize {
            cols: 20,
            rows: 3,
            reflow: true,
            prompt_redraw: osc::semantic_prompt::Redraw::True,
        })
        .unwrap();

        // Cursor should not move
        assert_eq!(6, s.cursor.x);
        assert_eq!(2, s.cursor.y);

        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"ABCDE\n> echo\noutput");
        }
        s.deinit();
    }

    // test "Screen: resize with prompt_redraw last clears only one line"
    #[test]
    fn screen_resize_with_prompt_redraw_last_clears_only_one_line() {
        let mut s = mkscreen(10, 4, 5);

        s.test_write_string(b"ABCDE\n").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Prompt(
            osc::semantic_prompt::PromptKind::Initial,
        ));
        s.test_write_string(b"> ").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Input(
            SemanticContentInput::ClearExplicit,
        ));
        s.test_write_string(b"hello\n").unwrap();
        s.test_write_string(b"world").unwrap();

        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"ABCDE\n> hello\nworld");
        }

        // Cursor is at end of "world" line with semantic_content = .input
        s.resize(Resize {
            cols: 20,
            rows: 4,
            reflow: true,
            prompt_redraw: osc::semantic_prompt::Redraw::Last,
        })
        .unwrap();

        // With .last, only the current line where cursor is should be cleared
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"ABCDE\n> hello");
        }
        s.deinit();
    }

    // test "Screen: resize with prompt_redraw last multiline prompt clears only last line"
    #[test]
    fn screen_resize_with_prompt_redraw_last_multiline_prompt_clears_only_last_line() {
        let mut s = mkscreen(20, 5, 5);

        // Create a 3-line prompt: 1 initial + 2 continuation lines
        s.cursor_set_semantic_content(SemanticContentArg::Prompt(
            osc::semantic_prompt::PromptKind::Initial,
        ));
        s.test_write_string(b"line1\n").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Prompt(
            osc::semantic_prompt::PromptKind::Continuation,
        ));
        s.test_write_string(b"line2\n").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Prompt(
            osc::semantic_prompt::PromptKind::Continuation,
        ));
        s.test_write_string(b"line3").unwrap();

        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"line1\nline2\nline3");
        }

        // Cursor is at end of line3 (the last continuation line)
        s.resize(Resize {
            cols: 30,
            rows: 5,
            reflow: true,
            prompt_redraw: osc::semantic_prompt::Redraw::Last,
        })
        .unwrap();

        // With .last, only line3 (where cursor is) should be cleared
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"line1\nline2");
        }
        s.deinit();
    }

    // PORT NOTE: helpers for the selection tests below.
    //   `spin(s, pt)` mirrors `s.pages.pin(pt).?`.
    //   `pfp_screen(s, p)` mirrors `s.pages.pointFromPin(.screen, p).?`.
    //   `pfp_active(s, p)` mirrors `s.pages.pointFromPin(.active, p).?`.
    //   `selstr(s, sel, trim)` mirrors `try s.selectionString(alloc, .{ .sel, .trim })`.
    fn spin(s: &Screen, pt: point::Point) -> Pin {
        s.pages.pin(pt).unwrap()
    }
    fn pscreen(x: size::CellCountInt, y: u32) -> point::Point {
        point::Point::Screen(point::Coordinate { x, y })
    }
    fn pactive(x: size::CellCountInt, y: u32) -> point::Point {
        point::Point::Active(point::Coordinate { x, y })
    }
    fn pfp_screen(s: &Screen, p: Pin) -> point::Point {
        s.pages.point_from_pin(point::Tag::Screen, p).unwrap()
    }
    fn pfp_active(s: &Screen, p: Pin) -> point::Point {
        s.pages.point_from_pin(point::Tag::Active, p).unwrap()
    }
    fn selstr(s: &mut Screen, sel: Selection, trim: bool) -> Vec<u8> {
        s.selection_string(
            Allocator,
            SelectionString {
                sel,
                trim,
                map: None,
            },
        )
        .unwrap()
    }

    // test "Screen: selectAll"
    #[test]
    fn screen_select_all() {
        let mut s = mkscreen(10, 10, 0);

        {
            s.test_write_string(b"ABC  DEF\n 123\n456").unwrap();
            let sel = s.select_all().unwrap();
            assert_eq!(pscreen(0, 0), pfp_screen(&s, sel.start()));
            assert_eq!(pscreen(2, 2), pfp_screen(&s, sel.end()));
            sel.deinit(&mut s);
        }

        {
            s.test_write_string(b"\nFOO\n BAR\n BAZ\n QWERTY\n 12345678")
                .unwrap();
            let sel = s.select_all().unwrap();
            assert_eq!(pscreen(0, 0), pfp_screen(&s, sel.start()));
            assert_eq!(pscreen(8, 7), pfp_screen(&s, sel.end()));
            sel.deinit(&mut s);
        }

        s.deinit();
    }

    // test "Screen: selectLine"
    #[test]
    fn screen_select_line() {
        let mut s = mkscreen(10, 10, 0);
        s.test_write_string(b"ABC  DEF\n 123\n456").unwrap();

        // Outside of active area
        // try testing.expect(s.selectLine(.{ .x = 13, .y = 0 }) == null);
        // try testing.expect(s.selectLine(.{ .x = 0, .y = 5 }) == null);

        // Going forward
        {
            let pin = spin(&s, pactive(0, 0));
            let sel = s
                .select_line(SelectLine {
                    pin,
                    whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                    semantic_prompt_boundary: true,
                })
                .unwrap();
            assert_eq!(pscreen(0, 0), pfp_screen(&s, sel.start()));
            assert_eq!(pscreen(7, 0), pfp_screen(&s, sel.end()));
            sel.deinit(&mut s);
        }

        // Going backward
        {
            let pin = spin(&s, pactive(7, 0));
            let sel = s
                .select_line(SelectLine {
                    pin,
                    whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                    semantic_prompt_boundary: true,
                })
                .unwrap();
            assert_eq!(pscreen(0, 0), pfp_screen(&s, sel.start()));
            assert_eq!(pscreen(7, 0), pfp_screen(&s, sel.end()));
            sel.deinit(&mut s);
        }

        // Going forward and backward
        {
            let pin = spin(&s, pactive(3, 0));
            let sel = s
                .select_line(SelectLine {
                    pin,
                    whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                    semantic_prompt_boundary: true,
                })
                .unwrap();
            assert_eq!(pscreen(0, 0), pfp_screen(&s, sel.start()));
            assert_eq!(pscreen(7, 0), pfp_screen(&s, sel.end()));
            sel.deinit(&mut s);
        }

        // Outside active area
        {
            let pin = spin(&s, pactive(9, 0));
            let sel = s
                .select_line(SelectLine {
                    pin,
                    whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                    semantic_prompt_boundary: true,
                })
                .unwrap();
            assert_eq!(pscreen(0, 0), pfp_screen(&s, sel.start()));
            assert_eq!(pscreen(7, 0), pfp_screen(&s, sel.end()));
            sel.deinit(&mut s);
        }

        s.deinit();
    }

    // test "Screen: selectLine across soft-wrap"
    #[test]
    fn screen_select_line_across_soft_wrap() {
        let mut s = mkscreen(5, 10, 0);
        s.test_write_string(b" 12 34012   \n 123").unwrap();

        // Going forward
        {
            let pin = spin(&s, pactive(1, 0));
            let sel = s
                .select_line(SelectLine {
                    pin,
                    whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                    semantic_prompt_boundary: true,
                })
                .unwrap();
            assert_eq!(pscreen(1, 0), pfp_screen(&s, sel.start()));
            assert_eq!(pscreen(3, 1), pfp_screen(&s, sel.end()));
            sel.deinit(&mut s);
        }

        s.deinit();
    }

    // test "Screen: selectLine across full soft-wrap"
    #[test]
    fn screen_select_line_across_full_soft_wrap() {
        let mut s = mkscreen(5, 5, 0);
        s.test_write_string(b"1ABCD2EFGH\n3IJKL").unwrap();

        {
            let pin = spin(&s, pactive(2, 1));
            let sel = s
                .select_line(SelectLine {
                    pin,
                    whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                    semantic_prompt_boundary: true,
                })
                .unwrap();
            assert_eq!(pscreen(0, 0), pfp_screen(&s, sel.start()));
            assert_eq!(pscreen(4, 1), pfp_screen(&s, sel.end()));
            sel.deinit(&mut s);
        }

        s.deinit();
    }

    // test "Screen: selectLine across soft-wrap ignores blank lines"
    #[test]
    fn screen_select_line_across_soft_wrap_ignores_blank_lines() {
        let mut s = mkscreen(5, 10, 0);
        s.test_write_string(b" 12 34012             \n 123")
            .unwrap();

        // Going forward
        {
            let pin = spin(&s, pactive(1, 0));
            let sel = s
                .select_line(SelectLine {
                    pin,
                    whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                    semantic_prompt_boundary: true,
                })
                .unwrap();
            assert_eq!(pscreen(1, 0), pfp_screen(&s, sel.start()));
            assert_eq!(pscreen(3, 1), pfp_screen(&s, sel.end()));
            sel.deinit(&mut s);
        }

        // Going backward
        {
            let pin = spin(&s, pactive(1, 1));
            let sel = s
                .select_line(SelectLine {
                    pin,
                    whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                    semantic_prompt_boundary: true,
                })
                .unwrap();
            assert_eq!(pscreen(1, 0), pfp_screen(&s, sel.start()));
            assert_eq!(pscreen(3, 1), pfp_screen(&s, sel.end()));
            sel.deinit(&mut s);
        }

        // Going forward and backward
        {
            let pin = spin(&s, pactive(3, 0));
            let sel = s
                .select_line(SelectLine {
                    pin,
                    whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                    semantic_prompt_boundary: true,
                })
                .unwrap();
            assert_eq!(pscreen(1, 0), pfp_screen(&s, sel.start()));
            assert_eq!(pscreen(3, 1), pfp_screen(&s, sel.end()));
            sel.deinit(&mut s);
        }

        s.deinit();
    }

    // test "Screen: selectLine disabled whitespace trimming"
    #[test]
    fn screen_select_line_disabled_whitespace_trimming() {
        let mut s = mkscreen(5, 10, 0);
        s.test_write_string(b" 12 34012   \n 123").unwrap();

        // Going forward
        {
            let pin = spin(&s, pactive(1, 0));
            let sel = s
                .select_line(SelectLine {
                    pin,
                    whitespace: None,
                    semantic_prompt_boundary: true,
                })
                .unwrap();
            assert_eq!(pscreen(0, 0), pfp_screen(&s, sel.start()));
            assert_eq!(pscreen(4, 2), pfp_screen(&s, sel.end()));
            sel.deinit(&mut s);
        }

        // Non-wrapped
        {
            let pin = spin(&s, pactive(1, 3));
            let sel = s
                .select_line(SelectLine {
                    pin,
                    whitespace: None,
                    semantic_prompt_boundary: true,
                })
                .unwrap();
            assert_eq!(pscreen(0, 3), pfp_screen(&s, sel.start()));
            assert_eq!(pscreen(4, 3), pfp_screen(&s, sel.end()));
            sel.deinit(&mut s);
        }

        s.deinit();
    }

    // test "Screen: selectLine with scrollback"
    #[test]
    fn screen_select_line_with_scrollback() {
        let mut s = mkscreen(2, 3, 5);
        s.test_write_string(b"1A\n2B\n3C\n4D\n5E").unwrap();

        // Selecting first line
        {
            let pin = spin(&s, pactive(0, 0));
            let sel = s
                .select_line(SelectLine {
                    pin,
                    whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                    semantic_prompt_boundary: true,
                })
                .unwrap();
            assert_eq!(pactive(0, 0), pfp_active(&s, sel.start()));
            assert_eq!(pactive(1, 0), pfp_active(&s, sel.end()));
            sel.deinit(&mut s);
        }

        // Selecting last line
        {
            let pin = spin(&s, pactive(0, 2));
            let sel = s
                .select_line(SelectLine {
                    pin,
                    whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                    semantic_prompt_boundary: true,
                })
                .unwrap();
            assert_eq!(pactive(0, 2), pfp_active(&s, sel.start()));
            assert_eq!(pactive(1, 2), pfp_active(&s, sel.end()));
            sel.deinit(&mut s);
        }

        s.deinit();
    }

    // test "Screen: selectLine semantic prompt boundary"
    #[test]
    fn screen_select_line_semantic_prompt_boundary() {
        let mut s = mkscreen(5, 10, 0);
        s.test_write_string(b"ABCDE\n").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Prompt(
            osc::semantic_prompt::PromptKind::Initial,
        ));
        s.test_write_string(b"A    ").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Output);
        s.test_write_string(b"> ").unwrap();

        {
            let contents = dump(&s, scr());
            assert_eq!(contents, b"ABCDE\nA    \n> ");
        }

        // Selecting output stops at the prompt even if soft-wrapped
        {
            let pin = spin(&s, pactive(1, 1));
            let sel = s
                .select_line(SelectLine {
                    pin,
                    whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                    semantic_prompt_boundary: true,
                })
                .unwrap();
            let contents = selstr(&mut s, sel, false);
            let expected = b"A";
            assert_eq!(contents.as_slice(), expected);
            sel.deinit(&mut s);
        }
        {
            let pin = spin(&s, pactive(1, 2));
            let sel = s
                .select_line(SelectLine {
                    pin,
                    whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                    semantic_prompt_boundary: true,
                })
                .unwrap();
            assert_eq!(pactive(0, 2), pfp_active(&s, sel.start()));
            assert_eq!(pactive(0, 2), pfp_active(&s, sel.end()));
            sel.deinit(&mut s);
        }

        s.deinit();
    }

    // test "Screen: selectLine semantic prompt to input boundary"
    #[test]
    fn screen_select_line_semantic_prompt_to_input_boundary() {
        let mut s = mkscreen(10, 5, 0);

        // Write prompt followed by user input on same row: "$>command"
        // Using non-whitespace to avoid whitespace trimming affecting the test
        s.cursor_set_semantic_content(SemanticContentArg::Prompt(
            osc::semantic_prompt::PromptKind::Initial,
        ));
        s.test_write_string(b"$>").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Input(
            SemanticContentInput::ClearExplicit,
        ));
        s.test_write_string(b"command").unwrap();

        // Selecting from prompt should only select prompt
        {
            let pin = spin(&s, pactive(0, 0));
            let sel = s
                .select_line(SelectLine {
                    pin,
                    whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                    semantic_prompt_boundary: true,
                })
                .unwrap();
            assert_eq!(pactive(0, 0), pfp_active(&s, sel.start()));
            assert_eq!(pactive(1, 0), pfp_active(&s, sel.end()));
            sel.deinit(&mut s);
        }

        // Selecting from input should only select input
        {
            let pin = spin(&s, pactive(5, 0));
            let sel = s
                .select_line(SelectLine {
                    pin,
                    whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                    semantic_prompt_boundary: true,
                })
                .unwrap();
            assert_eq!(pactive(2, 0), pfp_active(&s, sel.start()));
            assert_eq!(pactive(8, 0), pfp_active(&s, sel.end()));
            sel.deinit(&mut s);
        }

        s.deinit();
    }

    // test "Screen: selectLine semantic input to output boundary"
    #[test]
    fn screen_select_line_semantic_input_to_output_boundary() {
        let mut s = mkscreen(10, 5, 0);

        // Row 0: user input
        s.cursor_set_semantic_content(SemanticContentArg::Input(
            SemanticContentInput::ClearExplicit,
        ));
        s.test_write_string(b"ls -la\n").unwrap();
        // Row 1: command output
        s.cursor_set_semantic_content(SemanticContentArg::Output);
        s.test_write_string(b"file.txt").unwrap();

        // Selecting from input should only select input
        {
            let pin = spin(&s, pactive(2, 0));
            let sel = s
                .select_line(SelectLine {
                    pin,
                    whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                    semantic_prompt_boundary: true,
                })
                .unwrap();
            let contents = selstr(&mut s, sel, false);
            assert_eq!(contents.as_slice(), b"ls -la");
            sel.deinit(&mut s);
        }

        // Selecting from output should only select output
        {
            let pin = spin(&s, pactive(2, 1));
            let sel = s
                .select_line(SelectLine {
                    pin,
                    whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                    semantic_prompt_boundary: true,
                })
                .unwrap();
            let contents = selstr(&mut s, sel, false);
            assert_eq!(contents.as_slice(), b"file.txt");
            sel.deinit(&mut s);
        }

        s.deinit();
    }

    // test "Screen: selectLine semantic mid-row boundary"
    #[test]
    fn screen_select_line_semantic_mid_row_boundary() {
        let mut s = mkscreen(10, 5, 0);

        // Single row with output then prompt then input: "out$>cmd"
        // Using non-whitespace to avoid whitespace trimming affecting the test
        s.cursor_set_semantic_content(SemanticContentArg::Output);
        s.test_write_string(b"out").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Prompt(
            osc::semantic_prompt::PromptKind::Initial,
        ));
        s.test_write_string(b"$>").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Input(
            SemanticContentInput::ClearExplicit,
        ));
        s.test_write_string(b"cmd").unwrap();

        // Selecting from output should stop at prompt
        {
            let pin = spin(&s, pactive(1, 0));
            let sel = s
                .select_line(SelectLine {
                    pin,
                    whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                    semantic_prompt_boundary: true,
                })
                .unwrap();
            assert_eq!(pactive(0, 0), pfp_active(&s, sel.start()));
            assert_eq!(pactive(2, 0), pfp_active(&s, sel.end()));
            sel.deinit(&mut s);
        }

        // Selecting from prompt should only select prompt
        {
            let pin = spin(&s, pactive(3, 0));
            let sel = s
                .select_line(SelectLine {
                    pin,
                    whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                    semantic_prompt_boundary: true,
                })
                .unwrap();
            assert_eq!(pactive(3, 0), pfp_active(&s, sel.start()));
            assert_eq!(pactive(4, 0), pfp_active(&s, sel.end()));
            sel.deinit(&mut s);
        }

        // Selecting from input should only select input
        {
            let pin = spin(&s, pactive(6, 0));
            let sel = s
                .select_line(SelectLine {
                    pin,
                    whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                    semantic_prompt_boundary: true,
                })
                .unwrap();
            assert_eq!(pactive(5, 0), pfp_active(&s, sel.start()));
            assert_eq!(pactive(7, 0), pfp_active(&s, sel.end()));
            sel.deinit(&mut s);
        }

        s.deinit();
    }

    // test "Screen: selectLine semantic boundary soft-wrap with mid-row transition"
    #[test]
    fn screen_select_line_semantic_boundary_soft_wrap_with_mid_row_transition() {
        let mut s = mkscreen(5, 5, 0);

        // Row 0: prompt "$ " + input "cmd" (soft-wraps)
        // Row 1: input continues "12" + output "out"
        s.cursor_set_semantic_content(SemanticContentArg::Prompt(
            osc::semantic_prompt::PromptKind::Initial,
        ));
        s.test_write_string(b"$ ").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Input(
            SemanticContentInput::ClearExplicit,
        ));
        s.test_write_string(b"cmd12").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Output);
        s.test_write_string(b"out").unwrap();

        // Verify layout
        {
            let contents = dump(&s, scr());
            assert_eq!(contents, b"$ cmd\n12out");
        }

        // Selecting from input on row 0 should get all input across soft-wrap
        {
            let pin = spin(&s, pactive(3, 0));
            let sel = s
                .select_line(SelectLine {
                    pin,
                    whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                    semantic_prompt_boundary: true,
                })
                .unwrap();
            let contents = selstr(&mut s, sel, false);
            assert_eq!(contents.as_slice(), b"cmd12");
            sel.deinit(&mut s);
        }

        // Selecting from input on row 1 should get all input across soft-wrap
        {
            let pin = spin(&s, pactive(0, 1));
            let sel = s
                .select_line(SelectLine {
                    pin,
                    whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                    semantic_prompt_boundary: true,
                })
                .unwrap();
            let contents = selstr(&mut s, sel, false);
            assert_eq!(contents.as_slice(), b"cmd12");
            sel.deinit(&mut s);
        }

        // Selecting from output should only get output
        {
            let pin = spin(&s, pactive(3, 1));
            let sel = s
                .select_line(SelectLine {
                    pin,
                    whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                    semantic_prompt_boundary: true,
                })
                .unwrap();
            let contents = selstr(&mut s, sel, false);
            assert_eq!(contents.as_slice(), b"out");
            sel.deinit(&mut s);
        }

        s.deinit();
    }

    // test "Screen: selectLine semantic boundary disabled"
    #[test]
    fn screen_select_line_semantic_boundary_disabled() {
        let mut s = mkscreen(10, 5, 0);

        // Write prompt followed by input
        s.cursor_set_semantic_content(SemanticContentArg::Prompt(
            osc::semantic_prompt::PromptKind::Initial,
        ));
        s.test_write_string(b"$ ").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Input(
            SemanticContentInput::ClearExplicit,
        ));
        s.test_write_string(b"command").unwrap();

        // With semantic_prompt_boundary = false, should select entire line
        {
            let pin = spin(&s, pactive(0, 0));
            let sel = s
                .select_line(SelectLine {
                    pin,
                    whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                    semantic_prompt_boundary: false,
                })
                .unwrap();
            let contents = selstr(&mut s, sel, false);
            assert_eq!(contents.as_slice(), b"$ command");
            sel.deinit(&mut s);
        }

        s.deinit();
    }

    // test "Screen: selectLine semantic boundary first cell of row"
    #[test]
    fn screen_select_line_semantic_boundary_first_cell_of_row() {
        let mut s = mkscreen(5, 5, 0);

        // Row 0: input that soft-wraps
        // Row 1: output starts at first cell
        s.cursor_set_semantic_content(SemanticContentArg::Input(
            SemanticContentInput::ClearExplicit,
        ));
        s.test_write_string(b"12345").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Output);
        s.test_write_string(b"ABCDE").unwrap();

        // Verify soft-wrap happened
        {
            let pin = spin(&s, pactive(0, 0));
            let row = pin.row_and_cell().row;
            assert!(unsafe { (*row).wrap() });
        }

        // Selecting from input should stop before output on row 1
        {
            let pin = spin(&s, pactive(2, 0));
            let sel = s
                .select_line(SelectLine {
                    pin,
                    whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                    semantic_prompt_boundary: true,
                })
                .unwrap();
            assert_eq!(pactive(0, 0), pfp_active(&s, sel.start()));
            assert_eq!(pactive(4, 0), pfp_active(&s, sel.end()));
            sel.deinit(&mut s);
        }

        // Selecting from output should only get output
        {
            let pin = spin(&s, pactive(2, 1));
            let sel = s
                .select_line(SelectLine {
                    pin,
                    whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                    semantic_prompt_boundary: true,
                })
                .unwrap();
            assert_eq!(pactive(0, 1), pfp_active(&s, sel.start()));
            assert_eq!(pactive(4, 1), pfp_active(&s, sel.end()));
            sel.deinit(&mut s);
        }

        s.deinit();
    }

    // test "Screen: selectLine semantic all same content"
    #[test]
    fn screen_select_line_semantic_all_same_content() {
        let mut s = mkscreen(5, 5, 0);

        // All prompt content that soft-wraps
        s.cursor_set_semantic_content(SemanticContentArg::Prompt(
            osc::semantic_prompt::PromptKind::Initial,
        ));
        s.test_write_string(b"prompt text").unwrap();

        // Verify soft-wrap
        {
            let contents = dump(&s, scr());
            assert_eq!(contents, b"promp\nt tex\nt");
        }

        // Should select all prompt content across soft-wraps
        {
            let pin = spin(&s, pactive(2, 1));
            let sel = s
                .select_line(SelectLine {
                    pin,
                    whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                    semantic_prompt_boundary: true,
                })
                .unwrap();
            let contents = selstr(&mut s, sel, false);
            assert_eq!(contents.as_slice(), b"prompt text");
            sel.deinit(&mut s);
        }

        s.deinit();
    }

    // PORT NOTE: the selectWord tests use the default word boundary codepoints
    // inline; mirrored here as a local const matching the reference array.
    // Default boundary codepoints for word selection
    const SELECT_WORD_BOUNDARY: [u32; 20] = [
        0,
        b' ' as u32,
        b'\t' as u32,
        b'\'' as u32,
        b'"' as u32,
        '│' as u32,
        b'`' as u32,
        b'|' as u32,
        b':' as u32,
        b';' as u32,
        b',' as u32,
        b'(' as u32,
        b')' as u32,
        b'[' as u32,
        b']' as u32,
        b'{' as u32,
        b'}' as u32,
        b'<' as u32,
        b'>' as u32,
        b'$' as u32,
    ];

    // test "Screen: selectWord"
    #[test]
    fn screen_select_word() {
        let mut s = mkscreen(10, 10, 0);
        s.test_write_string(b"ABC  DEF\n 123\n456").unwrap();

        let boundary_codepoints = &SELECT_WORD_BOUNDARY;

        // Outside of active area
        // try testing.expect(s.selectWord(.{ .x = 9, .y = 0 }) == null);
        // try testing.expect(s.selectWord(.{ .x = 0, .y = 5 }) == null);

        // Going forward
        {
            let pin = spin(&s, pactive(0, 0));
            let sel = s.select_word(pin, boundary_codepoints).unwrap();
            assert_eq!(pscreen(0, 0), pfp_screen(&s, sel.start()));
            assert_eq!(pscreen(2, 0), pfp_screen(&s, sel.end()));
            sel.deinit(&mut s);
        }

        // Going backward
        {
            let pin = spin(&s, pactive(2, 0));
            let sel = s.select_word(pin, boundary_codepoints).unwrap();
            assert_eq!(pscreen(0, 0), pfp_screen(&s, sel.start()));
            assert_eq!(pscreen(2, 0), pfp_screen(&s, sel.end()));
            sel.deinit(&mut s);
        }

        // Going forward and backward
        {
            let pin = spin(&s, pactive(1, 0));
            let sel = s.select_word(pin, boundary_codepoints).unwrap();
            assert_eq!(pscreen(0, 0), pfp_screen(&s, sel.start()));
            assert_eq!(pscreen(2, 0), pfp_screen(&s, sel.end()));
            sel.deinit(&mut s);
        }

        // Whitespace
        {
            let pin = spin(&s, pactive(3, 0));
            let sel = s.select_word(pin, boundary_codepoints).unwrap();
            assert_eq!(pscreen(3, 0), pfp_screen(&s, sel.start()));
            assert_eq!(pscreen(4, 0), pfp_screen(&s, sel.end()));
            sel.deinit(&mut s);
        }

        // Whitespace single char
        {
            let pin = spin(&s, pactive(0, 1));
            let sel = s.select_word(pin, boundary_codepoints).unwrap();
            assert_eq!(pscreen(0, 1), pfp_screen(&s, sel.start()));
            assert_eq!(pscreen(0, 1), pfp_screen(&s, sel.end()));
            sel.deinit(&mut s);
        }

        // End of screen
        {
            let pin = spin(&s, pactive(1, 2));
            let sel = s.select_word(pin, boundary_codepoints).unwrap();
            assert_eq!(pscreen(0, 2), pfp_screen(&s, sel.start()));
            assert_eq!(pscreen(2, 2), pfp_screen(&s, sel.end()));
            sel.deinit(&mut s);
        }

        s.deinit();
    }

    // test "Screen: selectWord across soft-wrap"
    #[test]
    fn screen_select_word_across_soft_wrap() {
        let mut s = mkscreen(5, 10, 0);
        s.test_write_string(b" 1234012\n 123").unwrap();

        let boundary_codepoints = &SELECT_WORD_BOUNDARY;

        {
            let contents = dump(&s, scr());
            assert_eq!(contents, b" 1234\n012\n 123");
        }

        // Going forward
        {
            let pin = spin(&s, pactive(1, 0));
            let sel = s.select_word(pin, boundary_codepoints).unwrap();
            assert_eq!(pscreen(1, 0), pfp_screen(&s, sel.start()));
            assert_eq!(pscreen(2, 1), pfp_screen(&s, sel.end()));
            sel.deinit(&mut s);
        }

        // Going backward
        {
            let pin = spin(&s, pactive(1, 1));
            let sel = s.select_word(pin, boundary_codepoints).unwrap();
            assert_eq!(pscreen(1, 0), pfp_screen(&s, sel.start()));
            assert_eq!(pscreen(2, 1), pfp_screen(&s, sel.end()));
            sel.deinit(&mut s);
        }

        // Going forward and backward
        {
            let pin = spin(&s, pactive(3, 0));
            let sel = s.select_word(pin, boundary_codepoints).unwrap();
            assert_eq!(pscreen(1, 0), pfp_screen(&s, sel.start()));
            assert_eq!(pscreen(2, 1), pfp_screen(&s, sel.end()));
            sel.deinit(&mut s);
        }

        s.deinit();
    }

    // test "Screen: selectWord whitespace across soft-wrap"
    #[test]
    fn screen_select_word_whitespace_across_soft_wrap() {
        let mut s = mkscreen(5, 10, 0);
        s.test_write_string(b"1       1\n 123").unwrap();

        let boundary_codepoints = &SELECT_WORD_BOUNDARY;

        // Going forward
        {
            let pin = spin(&s, pactive(1, 0));
            let sel = s.select_word(pin, boundary_codepoints).unwrap();
            assert_eq!(pscreen(1, 0), pfp_screen(&s, sel.start()));
            assert_eq!(pscreen(2, 1), pfp_screen(&s, sel.end()));
            sel.deinit(&mut s);
        }

        // Going backward
        {
            let pin = spin(&s, pactive(1, 1));
            let sel = s.select_word(pin, boundary_codepoints).unwrap();
            assert_eq!(pscreen(1, 0), pfp_screen(&s, sel.start()));
            assert_eq!(pscreen(2, 1), pfp_screen(&s, sel.end()));
            sel.deinit(&mut s);
        }

        // Going forward and backward
        {
            let pin = spin(&s, pactive(3, 0));
            let sel = s.select_word(pin, boundary_codepoints).unwrap();
            assert_eq!(pscreen(1, 0), pfp_screen(&s, sel.start()));
            assert_eq!(pscreen(2, 1), pfp_screen(&s, sel.end()));
            sel.deinit(&mut s);
        }

        s.deinit();
    }

    // test "Screen: selectWord with character boundary"
    #[test]
    fn screen_select_word_with_character_boundary() {
        let boundary_codepoints = &SELECT_WORD_BOUNDARY;

        let cases: [&[u8]; 17] = [
            " 'abc' \n123".as_bytes(),
            " \"abc\" \n123".as_bytes(),
            " │abc│ \n123".as_bytes(),
            " `abc` \n123".as_bytes(),
            " |abc| \n123".as_bytes(),
            " :abc: \n123".as_bytes(),
            " ;abc; \n123".as_bytes(),
            " ,abc, \n123".as_bytes(),
            " (abc( \n123".as_bytes(),
            " )abc) \n123".as_bytes(),
            " [abc[ \n123".as_bytes(),
            " ]abc] \n123".as_bytes(),
            " {abc{ \n123".as_bytes(),
            " }abc} \n123".as_bytes(),
            " <abc< \n123".as_bytes(),
            " >abc> \n123".as_bytes(),
            " $abc$ \n123".as_bytes(),
        ];

        for case in cases {
            let mut s = mkscreen(20, 10, 0);
            s.test_write_string(case).unwrap();

            // Inside character forward
            {
                let pin = spin(&s, pactive(2, 0));
                let sel = s.select_word(pin, boundary_codepoints).unwrap();
                assert_eq!(pscreen(2, 0), pfp_screen(&s, sel.start()));
                assert_eq!(pscreen(4, 0), pfp_screen(&s, sel.end()));
                sel.deinit(&mut s);
            }

            // Inside character backward
            {
                let pin = spin(&s, pactive(4, 0));
                let sel = s.select_word(pin, boundary_codepoints).unwrap();
                assert_eq!(pscreen(2, 0), pfp_screen(&s, sel.start()));
                assert_eq!(pscreen(4, 0), pfp_screen(&s, sel.end()));
                sel.deinit(&mut s);
            }

            // Inside character bidirectional
            {
                let pin = spin(&s, pactive(3, 0));
                let sel = s.select_word(pin, boundary_codepoints).unwrap();
                assert_eq!(pscreen(2, 0), pfp_screen(&s, sel.start()));
                assert_eq!(pscreen(4, 0), pfp_screen(&s, sel.end()));
                sel.deinit(&mut s);
            }

            // On quote
            // NOTE: this behavior is not ideal, so we can change this one day,
            // but I think its also not that important compared to the above.
            {
                let pin = spin(&s, pactive(1, 0));
                let sel = s.select_word(pin, boundary_codepoints).unwrap();
                assert_eq!(pscreen(0, 0), pfp_screen(&s, sel.start()));
                assert_eq!(pscreen(1, 0), pfp_screen(&s, sel.end()));
                sel.deinit(&mut s);
            }

            s.deinit();
        }
    }

    // test "Screen: selectOutput"
    #[test]
    fn screen_select_output() {
        let mut s = mkscreen(10, 15, 0);

        // Build content with cell-level semantic content:
        // Row 0-1: output1 (output)
        // Row 2: prompt2 (prompt)
        // Row 3: input2 (input)
        // Row 4-7: output2 (output, with overflow causing wrap)
        // Row 8: "$ " (prompt) + "input3" (input)
        // Row 9-11: output3 (output)
        s.cursor_set_semantic_content(SemanticContentArg::Output);
        s.test_write_string(b"output1\n").unwrap();
        s.test_write_string(b"output1\n").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Prompt(
            osc::semantic_prompt::PromptKind::Initial,
        ));
        s.test_write_string(b"prompt2\n").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Input(
            SemanticContentInput::ClearExplicit,
        ));
        s.test_write_string(b"input2\n").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Output);
        s.test_write_string(b"output2output2output2output2\n")
            .unwrap();
        s.test_write_string(b"output2\n").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Prompt(
            osc::semantic_prompt::PromptKind::Initial,
        ));
        s.test_write_string(b"$ ").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Input(
            SemanticContentInput::ClearExplicit,
        ));
        s.test_write_string(b"input3\n").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Output);
        s.test_write_string(b"output3\n").unwrap();
        s.test_write_string(b"output3\n").unwrap();
        s.test_write_string(b"output3").unwrap();

        // First output block (rows 0-1), should select those rows
        {
            let pin = spin(&s, pactive(1, 1));
            let sel = s.select_output(pin).unwrap();
            let contents = selstr(&mut s, sel, false);
            assert_eq!(contents.as_slice(), b"output1\noutput1");
            sel.deinit(&mut s);
        }
        // Second output block (rows 4-7)
        {
            let pin = spin(&s, pactive(3, 7));
            let sel = s.select_output(pin).unwrap();
            let contents = selstr(&mut s, sel, false);
            assert_eq!(
                contents.as_slice(),
                b"output2output2output2output2\noutput2"
            );
            sel.deinit(&mut s);
        }
        // Third output block (rows 9-11)
        {
            let pin = spin(&s, pactive(2, 10));
            let sel = s.select_output(pin).unwrap();
            assert_eq!(pactive(0, 9), pfp_active(&s, sel.start()));
            assert_eq!(pactive(6, 11), pfp_active(&s, sel.end()));
            sel.deinit(&mut s);
        }
        // Click on prompt should return null
        {
            let pin = spin(&s, pactive(1, 8));
            assert!(s.select_output(pin).is_none());
        }
        // Click on input should return null
        {
            let pin = spin(&s, pactive(5, 8));
            assert!(s.select_output(pin).is_none());
        }

        s.deinit();
    }

    // test "Screen: selectionString basic"
    #[test]
    fn screen_selection_string_basic() {
        let mut s = mkscreen(5, 3, 0);
        let str = b"1ABCD\n2EFGH\n3IJKL";
        s.test_write_string(str).unwrap();

        {
            let sel = Selection::init(spin(&s, pscreen(0, 1)), spin(&s, pscreen(2, 2)), false);
            let contents = selstr(&mut s, sel, true);
            let expected = b"2EFGH\n3IJ";
            assert_eq!(contents.as_slice(), expected);
        }

        s.deinit();
    }

    // test "Screen: selectionString start outside of written area"
    #[test]
    fn screen_selection_string_start_outside_of_written_area() {
        let mut s = mkscreen(5, 10, 0);
        let str = b"1ABCD\n2EFGH\n3IJKL";
        s.test_write_string(str).unwrap();

        {
            let sel = Selection::init(spin(&s, pscreen(0, 5)), spin(&s, pscreen(2, 6)), false);
            let contents = selstr(&mut s, sel, true);
            let expected = b"";
            assert_eq!(contents.as_slice(), expected);
        }

        s.deinit();
    }

    // test "Screen: selectionString end outside of written area"
    #[test]
    fn screen_selection_string_end_outside_of_written_area() {
        let mut s = mkscreen(5, 10, 0);
        let str = b"1ABCD\n2EFGH\n3IJKL";
        s.test_write_string(str).unwrap();

        {
            let sel = Selection::init(spin(&s, pscreen(0, 2)), spin(&s, pscreen(2, 6)), false);
            let contents = selstr(&mut s, sel, true);
            let expected = b"3IJKL";
            assert_eq!(contents.as_slice(), expected);
        }

        s.deinit();
    }

    // test "Screen: selectionString trim space"
    #[test]
    fn screen_selection_string_trim_space() {
        let mut s = mkscreen(5, 3, 0);
        let str = b"1AB  \n2EFGH\n3IJKL";
        s.test_write_string(str).unwrap();

        let sel = Selection::init(spin(&s, pscreen(0, 0)), spin(&s, pscreen(2, 1)), false);

        {
            let contents = selstr(&mut s, sel, true);
            let expected = b"1AB\n2EF";
            assert_eq!(contents.as_slice(), expected);
        }

        // No trim
        {
            let contents = selstr(&mut s, sel, false);
            let expected = b"1AB  \n2EF";
            assert_eq!(contents.as_slice(), expected);
        }

        s.deinit();
    }

    // test "Screen: selectionString trim empty line"
    #[test]
    fn screen_selection_string_trim_empty_line() {
        let mut s = mkscreen(5, 5, 0);
        let str = b"1AB  \n\n2EFGH\n3IJKL";
        s.test_write_string(str).unwrap();

        let sel = Selection::init(spin(&s, pscreen(0, 0)), spin(&s, pscreen(2, 2)), false);

        {
            let contents = selstr(&mut s, sel, true);
            let expected = b"1AB\n\n2EF";
            assert_eq!(contents.as_slice(), expected);
        }

        // No trim
        {
            let contents = selstr(&mut s, sel, false);
            let expected = b"1AB  \n\n2EF";
            assert_eq!(contents.as_slice(), expected);
        }

        s.deinit();
    }

    // test "Screen: selectionString soft wrap"
    #[test]
    fn screen_selection_string_soft_wrap() {
        let mut s = mkscreen(5, 3, 0);
        let str = b"1ABCD2EFGH3IJKL";
        s.test_write_string(str).unwrap();

        {
            let sel = Selection::init(spin(&s, pscreen(0, 1)), spin(&s, pscreen(2, 2)), false);
            let contents = selstr(&mut s, sel, true);
            let expected = b"2EFGH3IJ";
            assert_eq!(contents.as_slice(), expected);
        }

        s.deinit();
    }

    // test "Screen: selectionString wide char"
    #[test]
    fn screen_selection_string_wide_char() {
        let mut s = mkscreen(5, 3, 0);
        let str = "1A⚡".as_bytes();
        s.test_write_string(str).unwrap();

        {
            let sel = Selection::init(spin(&s, pscreen(0, 0)), spin(&s, pscreen(3, 0)), false);
            let contents = selstr(&mut s, sel, true);
            let expected = str;
            assert_eq!(contents.as_slice(), expected);
        }

        {
            let sel = Selection::init(spin(&s, pscreen(0, 0)), spin(&s, pscreen(2, 0)), false);
            let contents = selstr(&mut s, sel, true);
            let expected = str;
            assert_eq!(contents.as_slice(), expected);
        }

        {
            let sel = Selection::init(spin(&s, pscreen(3, 0)), spin(&s, pscreen(3, 0)), false);
            let contents = selstr(&mut s, sel, true);
            let expected = "⚡".as_bytes();
            assert_eq!(contents.as_slice(), expected);
        }

        s.deinit();
    }

    // test "Screen: selectionString wide char with header"
    #[test]
    fn screen_selection_string_wide_char_with_header() {
        let mut s = mkscreen(5, 3, 0);
        let str = "1ABC⚡".as_bytes();
        s.test_write_string(str).unwrap();

        {
            let sel = Selection::init(spin(&s, pscreen(0, 0)), spin(&s, pscreen(4, 0)), false);
            let contents = selstr(&mut s, sel, true);
            let expected = str;
            assert_eq!(contents.as_slice(), expected);
        }

        s.deinit();
    }

    // https://github.com/mitchellh/ghostty/issues/289
    // test "Screen: selectionString empty with soft wrap"
    #[test]
    fn screen_selection_string_empty_with_soft_wrap() {
        let mut s = mkscreen(5, 2, 0);

        // Let me describe the situation that caused this because this
        // test is not obvious. By writing an emoji below, we introduce
        // one cell with the emoji and one cell as a "wide char spacer".
        // We then soft wrap the line by writing spaces.
        //
        // By selecting only the tail, we'd select nothing and we had
        // a logic error that would cause a crash.
        s.test_write_string("👨".as_bytes()).unwrap();
        s.test_write_string(b"      ").unwrap();

        {
            let sel = Selection::init(spin(&s, pscreen(1, 0)), spin(&s, pscreen(2, 0)), false);
            let contents = selstr(&mut s, sel, true);
            let expected = "👨".as_bytes();
            assert_eq!(contents.as_slice(), expected);
        }

        s.deinit();
    }

    // test "Screen: selectionString with zero width joiner"
    #[test]
    fn screen_selection_string_with_zero_width_joiner() {
        let mut s = mkscreen(10, 1, 0);
        let str = "👨\u{200d}".as_bytes(); // this has a ZWJ
        s.test_write_string(str).unwrap();

        // Integrity check
        {
            let pin = spin(&s, pscreen(0, 0));
            let cell = pin.row_and_cell().cell;
            assert_eq!(0x1F468u32, unsafe { (*cell).codepoint() });
            assert_eq!(pagepkg::Wide::Wide, unsafe { (*cell).wide() });
            let cps = unsafe { (*pin.node).data.lookup_grapheme(cell).unwrap() };
            assert_eq!(1usize, cps.len());
        }

        // The real test
        {
            let sel = Selection::init(spin(&s, pscreen(0, 0)), spin(&s, pscreen(1, 0)), false);
            let contents = selstr(&mut s, sel, true);
            let expected = "👨\u{200d}".as_bytes();
            assert_eq!(contents.as_slice(), expected);
        }

        s.deinit();
    }

    // test "Screen: selectionString, rectangle, basic"
    #[test]
    fn screen_selection_string_rectangle_basic() {
        let mut s = mkscreen(30, 5, 0);
        let str = "Lorem ipsum dolor\nsit amet, consectetur\nadipiscing elit, sed do\neiusmod tempor incididunt\nut labore et dolore".as_bytes();
        let sel = Selection::init(spin(&s, pscreen(2, 1)), spin(&s, pscreen(6, 3)), true);
        let expected = "t ame\nipisc\nusmod".as_bytes();
        s.test_write_string(str).unwrap();

        let contents = selstr(&mut s, sel, true);
        assert_eq!(contents.as_slice(), expected);

        s.deinit();
    }

    // test "Screen: selectionString, rectangle, w/EOL"
    #[test]
    fn screen_selection_string_rectangle_w_eol() {
        let mut s = mkscreen(30, 5, 0);
        let str = "Lorem ipsum dolor\nsit amet, consectetur\nadipiscing elit, sed do\neiusmod tempor incididunt\nut labore et dolore".as_bytes();
        let sel = Selection::init(spin(&s, pscreen(12, 0)), spin(&s, pscreen(26, 4)), true);
        let expected = "dolor\nnsectetur\nlit, sed do\nor incididunt\n dolore".as_bytes();
        s.test_write_string(str).unwrap();

        let contents = selstr(&mut s, sel, true);
        assert_eq!(contents.as_slice(), expected);

        s.deinit();
    }

    // test "Screen: selectionString, rectangle, more complex w/breaks"
    #[test]
    fn screen_selection_string_rectangle_more_complex_w_breaks() {
        let mut s = mkscreen(30, 8, 0);
        let str = "Lorem ipsum dolor\nsit amet, consectetur\nadipiscing elit, sed do\neiusmod tempor incididunt\nut labore et dolore\n\nmagna aliqua. Ut enim\nad minim veniam, quis".as_bytes();
        let sel = Selection::init(spin(&s, pscreen(11, 2)), spin(&s, pscreen(26, 7)), true);
        let expected =
            "elit, sed do\npor incididunt\nt dolore\n\na. Ut enim\nniam, quis".as_bytes();
        s.test_write_string(str).unwrap();

        let contents = selstr(&mut s, sel, true);
        assert_eq!(contents.as_slice(), expected);

        s.deinit();
    }

    // test "Screen: selectionString multi-page"
    #[test]
    fn screen_selection_string_multi_page() {
        let mut s = mkscreen(10, 3, 2048);

        let first_page_size = unsafe { (*s.pages.pages.first).data.capacity.rows };

        // Lazy way to seek to the first page boundary.
        unsafe { (*s.pages.pages.first).data.pause_integrity_checks(true) };
        for _ in 0..(first_page_size - 1) {
            s.test_write_string(b"\n").unwrap();
        }
        unsafe { (*s.pages.pages.first).data.pause_integrity_checks(false) };

        s.test_write_string(b"123456789\n!@#$%^&*(\n123456789")
            .unwrap();

        {
            let sel = Selection::init(spin(&s, pactive(0, 0)), spin(&s, pactive(2, 2)), false);
            let contents = selstr(&mut s, sel, true);
            let expected = b"123456789\n!@#$%^&*(\n123";
            assert_eq!(contents.as_slice(), expected);
        }

        s.deinit();
    }

    // test "Screen: lineIterator"
    #[test]
    fn screen_line_iterator() {
        let mut s = mkscreen(5, 5, 0);
        let str = b"1ABCD\n2EFGH";
        s.test_write_string(str).unwrap();

        // Test the line iterator
        // PORT NOTE: the reference holds the iterator's `*const Screen` and
        // interleaves `iter.next()` with `selectionString` (a `*mut Screen`
        // method). Rust's `LineIterator<'a>` borrows `&Screen`, so we collect
        // both selections first (dropping the iterator borrow) before calling
        // the `&mut self` `selection_string`. The order of operations and
        // assertions is identical.
        let start = spin(&s, vp());
        let (sel0, sel1) = {
            let mut iter = s.line_iterator(start);
            let a = iter.next().unwrap();
            let b = iter.next().unwrap();
            (a, b)
        };
        {
            let actual = selstr(&mut s, sel0, false);
            assert_eq!(actual.as_slice(), b"1ABCD");
        }
        {
            let actual = selstr(&mut s, sel1, false);
            assert_eq!(actual.as_slice(), b"2EFGH");
        }

        s.deinit();
    }

    // test "Screen: lineIterator soft wrap"
    #[test]
    fn screen_line_iterator_soft_wrap() {
        let mut s = mkscreen(5, 5, 0);
        let str = b"1ABCD2EFGH\n3ABCD";
        s.test_write_string(str).unwrap();

        // Test the line iterator
        // PORT NOTE: see `screen_line_iterator` — selections are collected
        // before the `&mut self` `selection_string` calls due to Rust's
        // borrow of `&Screen` by `LineIterator`.
        let start = spin(&s, vp());
        let (sel0, sel1) = {
            let mut iter = s.line_iterator(start);
            let a = iter.next().unwrap();
            let b = iter.next().unwrap();
            (a, b)
        };
        {
            let actual = selstr(&mut s, sel0, false);
            assert_eq!(actual.as_slice(), b"1ABCD2EFGH");
        }
        {
            let actual = selstr(&mut s, sel1, false);
            assert_eq!(actual.as_slice(), b"3ABCD");
        }
        // try testing.expect(iter.next() == null);

        s.deinit();
    }

    // PORT NOTE: `init` helper mirroring the reference's
    // `try init(alloc, .{ .cols, .rows, .max_scrollback })`. The kitty image
    // limit fields are not exercised by these tests (kitty graphics is deferred
    // to vt-12); they take the same defaults used by every other test in this
    // module.
    fn clone_clear_init(
        cols: size::CellCountInt,
        rows: size::CellCountInt,
        max_scrollback: usize,
    ) -> Screen {
        Screen::init(
            Allocator,
            Options {
                cols,
                rows,
                max_scrollback,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap()
    }

    // test "Screen: clone"
    #[test]
    fn screen_clone() {
        let mut s = clone_clear_init(10, 3, 10);
        s.test_write_string(b"1ABCD\n2EFGH").unwrap();
        {
            let contents = s
                .dump_string_alloc(
                    Allocator,
                    point::Point::Active(point::Coordinate::default()),
                )
                .unwrap();
            assert_eq!(contents, b"1ABCD\n2EFGH");
        }
        assert_eq!(5usize, s.cursor.x as usize);
        assert_eq!(1usize, s.cursor.y as usize);

        // Clone
        let mut s2 = s
            .clone(
                Allocator,
                point::Point::Active(point::Coordinate::default()),
                None,
            )
            .unwrap();
        {
            let contents = s2
                .dump_string_alloc(
                    Allocator,
                    point::Point::Active(point::Coordinate::default()),
                )
                .unwrap();
            assert_eq!(contents, b"1ABCD\n2EFGH");
        }
        assert_eq!(5usize, s2.cursor.x as usize);
        assert_eq!(1usize, s2.cursor.y as usize);

        // Write to s1, should not be in s2
        s.test_write_string(b"\n34567").unwrap();
        {
            let contents = s
                .dump_string_alloc(
                    Allocator,
                    point::Point::Active(point::Coordinate::default()),
                )
                .unwrap();
            assert_eq!(contents, b"1ABCD\n2EFGH\n34567");
        }
        {
            let contents = s2
                .dump_string_alloc(
                    Allocator,
                    point::Point::Active(point::Coordinate::default()),
                )
                .unwrap();
            assert_eq!(contents, b"1ABCD\n2EFGH");
        }
        assert_eq!(5usize, s2.cursor.x as usize);
        assert_eq!(1usize, s2.cursor.y as usize);

        s2.deinit();
        s.deinit();
    }

    // test "Screen: clone partial"
    #[test]
    fn screen_clone_partial() {
        let mut s = clone_clear_init(10, 3, 10);
        s.test_write_string(b"1ABCD\n2EFGH").unwrap();
        {
            let contents = s
                .dump_string_alloc(
                    Allocator,
                    point::Point::Active(point::Coordinate::default()),
                )
                .unwrap();
            assert_eq!(contents, b"1ABCD\n2EFGH");
        }
        assert_eq!(5usize, s.cursor.x as usize);
        assert_eq!(1usize, s.cursor.y as usize);

        // Clone
        let mut s2 = s
            .clone(
                Allocator,
                point::Point::Active(point::Coordinate { x: 0, y: 1 }),
                None,
            )
            .unwrap();
        {
            let contents = s2
                .dump_string_alloc(
                    Allocator,
                    point::Point::Active(point::Coordinate::default()),
                )
                .unwrap();
            assert_eq!(contents, b"2EFGH");
        }

        // Cursor is shifted since we cloned partial
        assert_eq!(5usize, s2.cursor.x as usize);
        assert_eq!(0usize, s2.cursor.y as usize);

        s2.deinit();
        s.deinit();
    }

    // test "Screen: clone partial cursor out of bounds"
    #[test]
    fn screen_clone_partial_cursor_out_of_bounds() {
        let mut s = clone_clear_init(10, 3, 10);
        s.test_write_string(b"1ABCD\n2EFGH").unwrap();
        {
            let contents = s
                .dump_string_alloc(
                    Allocator,
                    point::Point::Active(point::Coordinate::default()),
                )
                .unwrap();
            assert_eq!(contents, b"1ABCD\n2EFGH");
        }
        assert_eq!(5usize, s.cursor.x as usize);
        assert_eq!(1usize, s.cursor.y as usize);

        // Clone
        let mut s2 = s
            .clone(
                Allocator,
                point::Point::Active(point::Coordinate { x: 0, y: 0 }),
                Some(point::Point::Active(point::Coordinate { x: 0, y: 0 })),
            )
            .unwrap();
        {
            let contents = s2
                .dump_string_alloc(
                    Allocator,
                    point::Point::Active(point::Coordinate::default()),
                )
                .unwrap();
            assert_eq!(contents, b"1ABCD");
        }

        // Cursor is shifted since we cloned partial
        assert_eq!(0usize, s2.cursor.x as usize);
        assert_eq!(0usize, s2.cursor.y as usize);

        s2.deinit();
        s.deinit();
    }

    // test "Screen: clone contains full selection"
    #[test]
    fn screen_clone_contains_full_selection() {
        let mut s = clone_clear_init(5, 3, 1);
        s.test_write_string(b"1ABCD\n2EFGH\n3IJKL").unwrap();

        // Select a single line
        let cols = s.pages.cols;
        s.select(Some(Selection::init(
            s.pages
                .pin(point::Point::Active(point::Coordinate { x: 0, y: 1 }))
                .unwrap(),
            s.pages
                .pin(point::Point::Active(point::Coordinate {
                    x: cols - 1,
                    y: 1,
                }))
                .unwrap(),
            false,
        )))
        .unwrap();

        // Clone
        let mut s2 = s
            .clone(
                Allocator,
                point::Point::Active(point::Coordinate::default()),
                None,
            )
            .unwrap();

        // Our selection should remain valid
        {
            let sel = s2.selection.unwrap();
            assert_eq!(
                point::Point::Active(point::Coordinate { x: 0, y: 1 }),
                s2.pages
                    .point_from_pin(point::Tag::Active, sel.start())
                    .unwrap(),
            );
            assert_eq!(
                point::Point::Active(point::Coordinate {
                    x: s2.pages.cols - 1,
                    y: 1,
                }),
                s2.pages
                    .point_from_pin(point::Tag::Active, sel.end())
                    .unwrap(),
            );
        }

        s2.deinit();
        s.deinit();
    }

    // test "Screen: clone contains none of selection"
    #[test]
    fn screen_clone_contains_none_of_selection() {
        let mut s = clone_clear_init(5, 3, 1);
        s.test_write_string(b"1ABCD\n2EFGH\n3IJKL").unwrap();

        // Select a single line
        let cols = s.pages.cols;
        s.select(Some(Selection::init(
            s.pages
                .pin(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
                .unwrap(),
            s.pages
                .pin(point::Point::Active(point::Coordinate {
                    x: cols - 1,
                    y: 0,
                }))
                .unwrap(),
            false,
        )))
        .unwrap();

        // Clone
        let mut s2 = s
            .clone(
                Allocator,
                point::Point::Active(point::Coordinate { x: 0, y: 1 }),
                None,
            )
            .unwrap();

        // Our selection should be null
        assert!(s2.selection.is_none());

        s2.deinit();
        s.deinit();
    }

    // test "Screen: clone contains selection start cutoff"
    #[test]
    fn screen_clone_contains_selection_start_cutoff() {
        let mut s = clone_clear_init(5, 3, 1);
        s.test_write_string(b"1ABCD\n2EFGH\n3IJKL").unwrap();

        // Select a single line
        let cols = s.pages.cols;
        s.select(Some(Selection::init(
            s.pages
                .pin(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
                .unwrap(),
            s.pages
                .pin(point::Point::Active(point::Coordinate {
                    x: cols - 1,
                    y: 1,
                }))
                .unwrap(),
            false,
        )))
        .unwrap();

        // Clone
        let mut s2 = s
            .clone(
                Allocator,
                point::Point::Active(point::Coordinate { x: 0, y: 1 }),
                None,
            )
            .unwrap();

        // Our selection should remain valid
        {
            let sel = s2.selection.unwrap();
            assert_eq!(
                point::Point::Active(point::Coordinate { x: 0, y: 0 }),
                s2.pages
                    .point_from_pin(point::Tag::Active, sel.start())
                    .unwrap(),
            );
            assert_eq!(
                point::Point::Active(point::Coordinate {
                    x: s2.pages.cols - 1,
                    y: 0,
                }),
                s2.pages
                    .point_from_pin(point::Tag::Active, sel.end())
                    .unwrap(),
            );
        }

        s2.deinit();
        s.deinit();
    }

    // test "Screen: clone contains selection end cutoff"
    #[test]
    fn screen_clone_contains_selection_end_cutoff() {
        let mut s = clone_clear_init(5, 3, 1);
        s.test_write_string(b"1ABCD\n2EFGH\n3IJKL").unwrap();

        // Select a single line
        s.select(Some(Selection::init(
            s.pages
                .pin(point::Point::Active(point::Coordinate { x: 0, y: 1 }))
                .unwrap(),
            s.pages
                .pin(point::Point::Active(point::Coordinate { x: 2, y: 2 }))
                .unwrap(),
            false,
        )))
        .unwrap();

        // Clone
        let mut s2 = s
            .clone(
                Allocator,
                point::Point::Active(point::Coordinate { x: 0, y: 0 }),
                Some(point::Point::Active(point::Coordinate { x: 0, y: 1 })),
            )
            .unwrap();

        // Our selection should remain valid
        {
            let sel = s2.selection.unwrap();
            assert_eq!(
                point::Point::Active(point::Coordinate { x: 0, y: 1 }),
                s2.pages
                    .point_from_pin(point::Tag::Active, sel.start())
                    .unwrap(),
            );
            assert_eq!(
                point::Point::Active(point::Coordinate {
                    x: s2.pages.cols - 1,
                    y: 2,
                }),
                s2.pages
                    .point_from_pin(point::Tag::Active, sel.end())
                    .unwrap(),
            );
        }

        s2.deinit();
        s.deinit();
    }

    // test "Screen: clone contains selection end cutoff reversed"
    #[test]
    fn screen_clone_contains_selection_end_cutoff_reversed() {
        let mut s = clone_clear_init(5, 3, 1);
        s.test_write_string(b"1ABCD\n2EFGH\n3IJKL").unwrap();

        // Select a single line
        s.select(Some(Selection::init(
            s.pages
                .pin(point::Point::Active(point::Coordinate { x: 2, y: 2 }))
                .unwrap(),
            s.pages
                .pin(point::Point::Active(point::Coordinate { x: 0, y: 1 }))
                .unwrap(),
            false,
        )))
        .unwrap();

        // Clone
        let mut s2 = s
            .clone(
                Allocator,
                point::Point::Active(point::Coordinate { x: 0, y: 0 }),
                Some(point::Point::Active(point::Coordinate { x: 0, y: 1 })),
            )
            .unwrap();

        // Our selection should remain valid
        {
            let sel = s2.selection.unwrap();
            assert_eq!(
                point::Point::Active(point::Coordinate { x: 0, y: 1 }),
                s2.pages
                    .point_from_pin(point::Tag::Active, sel.start())
                    .unwrap(),
            );
            assert_eq!(
                point::Point::Active(point::Coordinate {
                    x: s2.pages.cols - 1,
                    y: 2,
                }),
                s2.pages
                    .point_from_pin(point::Tag::Active, sel.end())
                    .unwrap(),
            );
        }

        s2.deinit();
        s.deinit();
    }

    // test "Screen: clone contains subset of selection"
    #[test]
    fn screen_clone_contains_subset_of_selection() {
        let mut s = clone_clear_init(5, 4, 1);
        s.test_write_string(b"1ABCD\n2EFGH\n3IJKL\n4ABCD").unwrap();

        // Select the full screen
        s.select(Some(Selection::init(
            s.pages
                .pin(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
                .unwrap(),
            s.pages
                .pin(point::Point::Active(point::Coordinate { x: 0, y: 3 }))
                .unwrap(),
            false,
        )))
        .unwrap();

        // Clone
        let mut s2 = s
            .clone(
                Allocator,
                point::Point::Active(point::Coordinate { x: 0, y: 1 }),
                Some(point::Point::Active(point::Coordinate { x: 0, y: 2 })),
            )
            .unwrap();

        // Our selection should remain valid
        {
            let sel = s2.selection.unwrap();
            assert_eq!(
                point::Point::Active(point::Coordinate { x: 0, y: 0 }),
                s2.pages
                    .point_from_pin(point::Tag::Active, sel.start())
                    .unwrap(),
            );
            assert_eq!(
                point::Point::Active(point::Coordinate {
                    x: s2.pages.cols - 1,
                    y: 3,
                }),
                s2.pages
                    .point_from_pin(point::Tag::Active, sel.end())
                    .unwrap(),
            );
        }

        s2.deinit();
        s.deinit();
    }

    // test "Screen: clone contains subset of rectangle selection"
    #[test]
    fn screen_clone_contains_subset_of_rectangle_selection() {
        let mut s = clone_clear_init(5, 4, 1);
        s.test_write_string(b"1ABCD\n2EFGH\n3IJKL\n4ABCD").unwrap();

        // Select the full screen from x=1 to x=3
        s.select(Some(Selection::init(
            s.pages
                .pin(point::Point::Active(point::Coordinate { x: 1, y: 0 }))
                .unwrap(),
            s.pages
                .pin(point::Point::Active(point::Coordinate { x: 3, y: 3 }))
                .unwrap(),
            true,
        )))
        .unwrap();

        // Clone
        let mut s2 = s
            .clone(
                Allocator,
                point::Point::Active(point::Coordinate { x: 0, y: 1 }),
                Some(point::Point::Active(point::Coordinate { x: 0, y: 2 })),
            )
            .unwrap();

        // Our selection should remain valid and be properly clipped
        // preserving the columns of the start and end points of the
        // selection.
        {
            let sel = s2.selection.unwrap();
            assert_eq!(
                point::Point::Active(point::Coordinate { x: 1, y: 0 }),
                s2.pages
                    .point_from_pin(point::Tag::Active, sel.start())
                    .unwrap(),
            );
            assert_eq!(
                point::Point::Active(point::Coordinate { x: 3, y: 3 }),
                s2.pages
                    .point_from_pin(point::Tag::Active, sel.end())
                    .unwrap(),
            );
        }

        s2.deinit();
        s.deinit();
    }

    // test "Screen: clone basic"
    #[test]
    fn screen_clone_basic() {
        let mut s = clone_clear_init(10, 3, 0);
        s.test_write_string(b"1ABCD\n2EFGH\n3IJKL").unwrap();

        {
            let mut s2 = s
                .clone(
                    Allocator,
                    point::Point::Active(point::Coordinate { x: 0, y: 1 }),
                    Some(point::Point::Active(point::Coordinate { x: 0, y: 1 })),
                )
                .unwrap();

            // Test our contents rotated
            let contents = s2
                .dump_string_alloc(
                    Allocator,
                    point::Point::Active(point::Coordinate::default()),
                )
                .unwrap();
            assert_eq!(contents, b"2EFGH");

            s2.deinit();
        }

        {
            let mut s2 = s
                .clone(
                    Allocator,
                    point::Point::Active(point::Coordinate { x: 0, y: 1 }),
                    Some(point::Point::Active(point::Coordinate { x: 0, y: 2 })),
                )
                .unwrap();

            // Test our contents rotated
            let contents = s2
                .dump_string_alloc(
                    Allocator,
                    point::Point::Active(point::Coordinate::default()),
                )
                .unwrap();
            assert_eq!(contents, b"2EFGH\n3IJKL");

            s2.deinit();
        }

        s.deinit();
    }

    // test "Screen: clone empty viewport"
    #[test]
    fn screen_clone_empty_viewport() {
        let mut s = clone_clear_init(10, 3, 0);

        {
            let mut s2 = s
                .clone(
                    Allocator,
                    point::Point::Viewport(point::Coordinate { x: 0, y: 0 }),
                    Some(point::Point::Viewport(point::Coordinate { x: 0, y: 0 })),
                )
                .unwrap();

            // Test our contents rotated
            let contents = s2
                .dump_string_alloc(
                    Allocator,
                    point::Point::Viewport(point::Coordinate::default()),
                )
                .unwrap();
            assert_eq!(contents, b"");

            s2.deinit();
        }

        s.deinit();
    }

    // test "Screen: clone one line viewport"
    #[test]
    fn screen_clone_one_line_viewport() {
        let mut s = clone_clear_init(10, 3, 0);
        s.test_write_string(b"1ABC").unwrap();

        {
            let mut s2 = s
                .clone(
                    Allocator,
                    point::Point::Viewport(point::Coordinate { x: 0, y: 0 }),
                    Some(point::Point::Viewport(point::Coordinate { x: 0, y: 0 })),
                )
                .unwrap();

            // Test our contents
            let contents = s2
                .dump_string_alloc(
                    Allocator,
                    point::Point::Viewport(point::Coordinate::default()),
                )
                .unwrap();
            assert_eq!(contents, b"1ABC");

            s2.deinit();
        }

        s.deinit();
    }

    // test "Screen: clone empty active"
    #[test]
    fn screen_clone_empty_active() {
        let mut s = clone_clear_init(10, 3, 0);

        {
            let mut s2 = s
                .clone(
                    Allocator,
                    point::Point::Active(point::Coordinate { x: 0, y: 0 }),
                    Some(point::Point::Active(point::Coordinate { x: 0, y: 0 })),
                )
                .unwrap();

            // Test our contents rotated
            let contents = s2
                .dump_string_alloc(
                    Allocator,
                    point::Point::Active(point::Coordinate::default()),
                )
                .unwrap();
            assert_eq!(contents, b"");

            s2.deinit();
        }

        s.deinit();
    }

    // test "Screen: clone one line active with extra space"
    #[test]
    fn screen_clone_one_line_active_with_extra_space() {
        let mut s = clone_clear_init(10, 3, 0);
        s.test_write_string(b"1ABC").unwrap();

        {
            let mut s2 = s
                .clone(
                    Allocator,
                    point::Point::Active(point::Coordinate { x: 0, y: 0 }),
                    None,
                )
                .unwrap();

            // Test our contents rotated
            let contents = s2
                .dump_string_alloc(
                    Allocator,
                    point::Point::Active(point::Coordinate::default()),
                )
                .unwrap();
            assert_eq!(contents, b"1ABC");

            s2.deinit();
        }

        s.deinit();
    }

    // test "Screen: clear history with no history"
    #[test]
    fn screen_clear_history_with_no_history() {
        let mut s = clone_clear_init(10, 3, 3);
        s.test_write_string(b"4ABCD\n5EFGH\n6IJKL").unwrap();
        assert!(s.pages.viewport == pagelist::Viewport::Active);
        s.erase_history(None);
        assert!(s.pages.viewport == pagelist::Viewport::Active);
        {
            // Test our contents rotated
            let contents = s
                .dump_string_alloc(
                    Allocator,
                    point::Point::Viewport(point::Coordinate::default()),
                )
                .unwrap();
            assert_eq!(contents, b"4ABCD\n5EFGH\n6IJKL");
        }
        {
            // Test our contents rotated
            let contents = s
                .dump_string_alloc(
                    Allocator,
                    point::Point::Screen(point::Coordinate::default()),
                )
                .unwrap();
            assert_eq!(contents, b"4ABCD\n5EFGH\n6IJKL");
        }

        s.deinit();
    }

    // test "Screen: clear history"
    #[test]
    fn screen_clear_history() {
        let mut s = clone_clear_init(10, 3, 3);
        s.test_write_string(b"1ABCD\n2EFGH\n3IJKL\n4ABCD\n5EFGH\n6IJKL")
            .unwrap();
        assert!(s.pages.viewport == pagelist::Viewport::Active);

        // Scroll to top
        s.scroll(Scroll::Top);
        {
            // Test our contents rotated
            let contents = s
                .dump_string_alloc(
                    Allocator,
                    point::Point::Viewport(point::Coordinate::default()),
                )
                .unwrap();
            assert_eq!(contents, b"1ABCD\n2EFGH\n3IJKL");
        }

        s.erase_history(None);
        assert!(s.pages.viewport == pagelist::Viewport::Active);
        {
            // Test our contents rotated
            let contents = s
                .dump_string_alloc(
                    Allocator,
                    point::Point::Viewport(point::Coordinate::default()),
                )
                .unwrap();
            assert_eq!(contents, b"4ABCD\n5EFGH\n6IJKL");
        }
        {
            // Test our contents rotated
            let contents = s
                .dump_string_alloc(
                    Allocator,
                    point::Point::Screen(point::Coordinate::default()),
                )
                .unwrap();
            assert_eq!(contents, b"4ABCD\n5EFGH\n6IJKL");
        }

        s.deinit();
    }

    // test "Screen: clear above cursor"
    #[test]
    fn screen_clear_above_cursor() {
        let mut s = clone_clear_init(10, 10, 3);
        s.test_write_string(b"4ABCD\n5EFGH\n6IJKL").unwrap();
        s.clear_rows(
            point::Point::Active(point::Coordinate { x: 0, y: 0 }),
            Some(point::Point::Active(point::Coordinate {
                x: 0,
                y: (s.cursor.y - 1) as u32,
            })),
            false,
        );
        {
            let contents = s
                .dump_string_alloc(
                    Allocator,
                    point::Point::Viewport(point::Coordinate::default()),
                )
                .unwrap();
            assert_eq!(contents, b"\n\n6IJKL");
        }
        {
            let contents = s
                .dump_string_alloc(
                    Allocator,
                    point::Point::Screen(point::Coordinate::default()),
                )
                .unwrap();
            assert_eq!(contents, b"\n\n6IJKL");
        }

        assert_eq!(5usize, s.cursor.x as usize);
        assert_eq!(2usize, s.cursor.y as usize);

        s.deinit();
    }

    // test "Screen: clear above cursor with history"
    #[test]
    fn screen_clear_above_cursor_with_history() {
        let mut s = clone_clear_init(10, 3, 3);
        s.test_write_string(b"1ABCD\n2EFGH\n3IJKL\n").unwrap();
        s.test_write_string(b"4ABCD\n5EFGH\n6IJKL").unwrap();
        s.clear_rows(
            point::Point::Active(point::Coordinate { x: 0, y: 0 }),
            Some(point::Point::Active(point::Coordinate {
                x: 0,
                y: (s.cursor.y - 1) as u32,
            })),
            false,
        );
        {
            let contents = s
                .dump_string_alloc(
                    Allocator,
                    point::Point::Viewport(point::Coordinate::default()),
                )
                .unwrap();
            assert_eq!(contents, b"\n\n6IJKL");
        }
        {
            let contents = s
                .dump_string_alloc(
                    Allocator,
                    point::Point::Screen(point::Coordinate::default()),
                )
                .unwrap();
            assert_eq!(contents, b"1ABCD\n2EFGH\n3IJKL\n\n\n6IJKL");
        }

        assert_eq!(5usize, s.cursor.x as usize);
        assert_eq!(2usize, s.cursor.y as usize);

        s.deinit();
    }

    // test "Screen clearRows active one line"
    #[test]
    fn screen_clear_rows_active_one_line() {
        let mut s = clone_clear_init(80, 24, 1000);

        s.test_write_string(b"hello, world").unwrap();
        s.clear_rows(
            point::Point::Active(point::Coordinate::default()),
            None,
            false,
        );
        assert!(s
            .pages
            .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 0 })));
        let str = s
            .dump_string_alloc(
                Allocator,
                point::Point::Screen(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(str, b"");

        s.deinit();
    }

    // test "Screen clearRows active multi line"
    #[test]
    fn screen_clear_rows_active_multi_line() {
        let mut s = clone_clear_init(80, 24, 1000);

        s.test_write_string(b"hello\nworld").unwrap();
        s.clear_rows(
            point::Point::Active(point::Coordinate::default()),
            None,
            false,
        );
        assert!(s
            .pages
            .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 0 })));
        assert!(s
            .pages
            .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 1 })));
        let str = s
            .dump_string_alloc(
                Allocator,
                point::Point::Screen(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(str, b"");

        s.deinit();
    }

    // test "Screen clearRows active styled line"
    #[test]
    fn screen_clear_rows_active_styled_line() {
        let mut s = clone_clear_init(80, 24, 1000);

        s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
        s.test_write_string(b"hello world").unwrap();
        s.set_attribute(crate::sgr::Attribute::Unset).unwrap();

        // We should have one style
        let page: *mut Page = unsafe { core::ptr::addr_of_mut!((*(*s.cursor.page_pin).node).data) };
        assert_eq!(1usize, unsafe { (*page).styles.count() });

        s.clear_rows(
            point::Point::Active(point::Coordinate::default()),
            None,
            false,
        );

        // We should have none because active cleared it
        assert_eq!(0usize, unsafe { (*page).styles.count() });

        let str = s
            .dump_string_alloc(
                Allocator,
                point::Point::Screen(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(str, b"");

        s.deinit();
    }

    // test "Screen clearRows protected"
    #[test]
    fn screen_clear_rows_protected() {
        let mut s = clone_clear_init(80, 24, 1000);

        s.test_write_string(b"UNPROTECTED").unwrap();
        s.cursor.protected = true;
        s.test_write_string(b"PROTECTED").unwrap();
        s.cursor.protected = false;
        s.test_write_string(b"UNPROTECTED").unwrap();
        s.test_write_string(b"\n").unwrap();
        s.cursor.protected = true;
        s.test_write_string(b"PROTECTED").unwrap();
        s.cursor.protected = false;
        s.test_write_string(b"UNPROTECTED").unwrap();
        s.cursor.protected = true;
        s.test_write_string(b"PROTECTED").unwrap();
        s.cursor.protected = false;

        s.clear_rows(
            point::Point::Active(point::Coordinate::default()),
            None,
            true,
        );

        let str = s
            .dump_string_alloc(
                Allocator,
                point::Point::Screen(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(
            str,
            b"           PROTECTED\nPROTECTED           PROTECTED".as_slice()
        );

        s.deinit();
    }

    // test "Screen eraseRows history"
    #[test]
    fn screen_erase_rows_history() {
        let mut s = clone_clear_init(5, 5, 1000);

        s.test_write_string(b"1\n2\n3\n4\n5\n6").unwrap();

        {
            let str = s
                .dump_string_alloc(
                    Allocator,
                    point::Point::Active(point::Coordinate::default()),
                )
                .unwrap();
            assert_eq!(str, b"2\n3\n4\n5\n6");
        }
        {
            let str = s
                .dump_string_alloc(
                    Allocator,
                    point::Point::Screen(point::Coordinate::default()),
                )
                .unwrap();
            assert_eq!(str, b"1\n2\n3\n4\n5\n6");
        }

        s.erase_history(None);

        {
            let str = s
                .dump_string_alloc(
                    Allocator,
                    point::Point::Active(point::Coordinate::default()),
                )
                .unwrap();
            assert_eq!(str, b"2\n3\n4\n5\n6");
        }
        {
            let str = s
                .dump_string_alloc(
                    Allocator,
                    point::Point::Screen(point::Coordinate::default()),
                )
                .unwrap();
            assert_eq!(str, b"2\n3\n4\n5\n6");
        }

        s.deinit();
    }

    // test "Screen eraseRows history with more lines"
    #[test]
    fn screen_erase_rows_history_with_more_lines() {
        let mut s = clone_clear_init(5, 5, 1000);

        s.test_write_string(b"A\nB\nC\n1\n2\n3\n4\n5\n6").unwrap();

        {
            let str = s
                .dump_string_alloc(
                    Allocator,
                    point::Point::Active(point::Coordinate::default()),
                )
                .unwrap();
            assert_eq!(str, b"2\n3\n4\n5\n6");
        }
        {
            let str = s
                .dump_string_alloc(
                    Allocator,
                    point::Point::Screen(point::Coordinate::default()),
                )
                .unwrap();
            assert_eq!(str, b"A\nB\nC\n1\n2\n3\n4\n5\n6");
        }

        s.erase_history(None);

        {
            let str = s
                .dump_string_alloc(
                    Allocator,
                    point::Point::Active(point::Coordinate::default()),
                )
                .unwrap();
            assert_eq!(str, b"2\n3\n4\n5\n6");
        }
        {
            let str = s
                .dump_string_alloc(
                    Allocator,
                    point::Point::Screen(point::Coordinate::default()),
                )
                .unwrap();
            assert_eq!(str, b"2\n3\n4\n5\n6");
        }

        s.deinit();
    }

    // test "Screen eraseRows active partial"
    #[test]
    fn screen_erase_rows_active_partial() {
        let mut s = clone_clear_init(5, 5, 0);

        s.test_write_string(b"1\n2\n3").unwrap();

        {
            let str = s
                .dump_string_alloc(
                    Allocator,
                    point::Point::Active(point::Coordinate::default()),
                )
                .unwrap();
            assert_eq!(str, b"1\n2\n3");
        }

        s.erase_active(1);

        {
            let str = s
                .dump_string_alloc(
                    Allocator,
                    point::Point::Active(point::Coordinate::default()),
                )
                .unwrap();
            assert_eq!(str, b"3");
        }
        {
            let str = s
                .dump_string_alloc(
                    Allocator,
                    point::Point::Screen(point::Coordinate::default()),
                )
                .unwrap();
            assert_eq!(str, b"3");
        }

        s.deinit();
    }

    // test "Screen: promptClickMove line right basic"
    #[test]
    fn screen_prompt_click_move_line_right_basic() {
        let mut s = clone_clear_init(20, 5, 0);

        // Enable line click mode
        s.semantic_prompt.click = SemanticClick::Cl(osc::semantic_prompt::Click::Line);

        // Write a prompt and input
        s.cursor_set_semantic_content(SemanticContentArg::Prompt(
            osc::semantic_prompt::PromptKind::Initial,
        ));
        s.test_write_string(b"> ").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Input(
            SemanticContentInput::ClearExplicit,
        ));
        s.test_write_string(b"hello").unwrap();

        // Move cursor back to start of input (column 2, the 'h')
        s.cursor_absolute(2, 0);

        // Click on first 'l' (column 4), should require 2 right movements (h->e->l)
        let click_pin = s
            .pages
            .pin(point::Point::Active(point::Coordinate { x: 4, y: 0 }))
            .unwrap();
        let result = s.prompt_click_move(click_pin);

        assert_eq!(result.right, 2);
        assert_eq!(result.left, 0);

        s.deinit();
    }

    // test "Screen: promptClickMove line right cursor not on input"
    #[test]
    fn screen_prompt_click_move_line_right_cursor_not_on_input() {
        let mut s = clone_clear_init(20, 5, 0);

        // Enable line click mode
        s.semantic_prompt.click = SemanticClick::Cl(osc::semantic_prompt::Click::Line);

        // Write a prompt and input
        s.cursor_set_semantic_content(SemanticContentArg::Prompt(
            osc::semantic_prompt::PromptKind::Initial,
        ));
        s.test_write_string(b"> ").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Input(
            SemanticContentInput::ClearExplicit,
        ));
        s.test_write_string(b"hello").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Output);

        // Move cursor back to prompt area (column 0, the '>')
        s.cursor_absolute(0, 0);

        // Cursor is on prompt, not input - should return zero
        let click_pin = s
            .pages
            .pin(point::Point::Active(point::Coordinate { x: 4, y: 0 }))
            .unwrap();
        let result = s.prompt_click_move(click_pin);

        assert_eq!(result, PromptClickMove::ZERO);

        s.deinit();
    }

    // test "Screen: promptClickMove line right click on same position"
    #[test]
    fn screen_prompt_click_move_line_right_click_on_same_position() {
        let mut s = clone_clear_init(20, 5, 0);

        // Enable line click mode
        s.semantic_prompt.click = SemanticClick::Cl(osc::semantic_prompt::Click::Line);

        // Write a prompt and input
        s.cursor_set_semantic_content(SemanticContentArg::Prompt(
            osc::semantic_prompt::PromptKind::Initial,
        ));
        s.test_write_string(b"> ").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Input(
            SemanticContentInput::ClearExplicit,
        ));
        s.test_write_string(b"hello").unwrap();

        // Move cursor to column 4
        s.cursor_absolute(4, 0);

        // Click on same position - no movement needed
        let click_pin = s
            .pages
            .pin(point::Point::Active(point::Coordinate { x: 4, y: 0 }))
            .unwrap();
        let result = s.prompt_click_move(click_pin);

        assert_eq!(result, PromptClickMove::ZERO);

        s.deinit();
    }

    // test "Screen: promptClickMove line right skips non-input cells"
    #[test]
    fn screen_prompt_click_move_line_right_skips_non_input_cells() {
        let mut s = clone_clear_init(20, 5, 0);

        // Enable line click mode
        s.semantic_prompt.click = SemanticClick::Cl(osc::semantic_prompt::Click::Line);

        // Write: "> h" then output "X" then input "llo"
        s.cursor_set_semantic_content(SemanticContentArg::Prompt(
            osc::semantic_prompt::PromptKind::Initial,
        ));
        s.test_write_string(b"> ").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Input(
            SemanticContentInput::ClearExplicit,
        ));
        s.test_write_string(b"h").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Output);
        s.test_write_string(b"X").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Input(
            SemanticContentInput::ClearExplicit,
        ));
        s.test_write_string(b"llo").unwrap();

        // Move cursor to column 2 (the 'h')
        s.cursor_absolute(2, 0);

        // Click on 'l' at column 5 - should skip the 'X' output cell
        // Movement: h (start) -> l (col 4) -> l (col 5) = 2 right movements
        let click_pin = s
            .pages
            .pin(point::Point::Active(point::Coordinate { x: 5, y: 0 }))
            .unwrap();
        let result = s.prompt_click_move(click_pin);

        assert_eq!(result.right, 2);
        assert_eq!(result.left, 0);

        s.deinit();
    }

    // test "Screen: promptClickMove line right soft-wrapped line"
    #[test]
    fn screen_prompt_click_move_line_right_soft_wrapped_line() {
        let mut s = clone_clear_init(10, 5, 0);

        // Enable line click mode
        s.semantic_prompt.click = SemanticClick::Cl(osc::semantic_prompt::Click::Line);

        // Write a prompt and input that wraps
        s.cursor_set_semantic_content(SemanticContentArg::Prompt(
            osc::semantic_prompt::PromptKind::Initial,
        ));
        s.test_write_string(b"> ").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Input(
            SemanticContentInput::ClearExplicit,
        ));
        // Write 8 chars of input, first row has 2 for prompt + 8 input = 10 cols
        s.test_write_string(b"abcdefgh").unwrap();
        // Continue on next row (soft-wrapped)
        s.test_write_string(b"ij").unwrap();

        // Verify soft wrap occurred
        {
            let contents = s
                .dump_string_alloc(
                    Allocator,
                    point::Point::Viewport(point::Coordinate::default()),
                )
                .unwrap();
            assert_eq!(contents, b"> abcdefgh\nij");
        }

        // Move cursor to column 2 (the 'a')
        s.cursor_absolute(2, 0);

        // Click on 'j' at column 1, row 1 - should count all input cells
        // Movement: a->b->c->d->e->f->g->h->i->j = 9 right movements
        let click_pin = s
            .pages
            .pin(point::Point::Active(point::Coordinate { x: 1, y: 1 }))
            .unwrap();
        let result = s.prompt_click_move(click_pin);

        assert_eq!(result.right, 9);
        assert_eq!(result.left, 0);

        s.deinit();
    }

    // test "Screen: promptClickMove disabled when click is none"
    #[test]
    fn screen_prompt_click_move_disabled_when_click_is_none() {
        let mut s = clone_clear_init(20, 5, 0);

        // Click mode is .none by default (disabled)
        assert_eq!(s.semantic_prompt.click, SemanticClick::None);

        // Write a prompt and input
        s.cursor_set_semantic_content(SemanticContentArg::Prompt(
            osc::semantic_prompt::PromptKind::Initial,
        ));
        s.test_write_string(b"> ").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Input(
            SemanticContentInput::ClearExplicit,
        ));
        s.test_write_string(b"hello").unwrap();

        // Move cursor to start of input
        s.cursor_absolute(2, 0);

        // Click should return zero since click mode is disabled
        let click_pin = s
            .pages
            .pin(point::Point::Active(point::Coordinate { x: 4, y: 0 }))
            .unwrap();
        let result = s.prompt_click_move(click_pin);

        assert_eq!(result, PromptClickMove::ZERO);

        s.deinit();
    }

    // test "Screen: promptClickMove line right stops at hard wrap"
    #[test]
    fn screen_prompt_click_move_line_right_stops_at_hard_wrap() {
        let mut s = clone_clear_init(20, 5, 0);

        // Enable line click mode
        s.semantic_prompt.click = SemanticClick::Cl(osc::semantic_prompt::Click::Line);

        // Write prompt and input on first line
        s.cursor_set_semantic_content(SemanticContentArg::Prompt(
            osc::semantic_prompt::PromptKind::Initial,
        ));
        s.test_write_string(b"> ").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Input(
            SemanticContentInput::ClearExplicit,
        ));
        s.test_write_string(b"hello").unwrap();
        // Hard wrap (newline)
        s.test_write_string(b"\n").unwrap();
        s.test_write_string(b"world").unwrap();

        // Move cursor to column 2 (the 'h')
        s.cursor_absolute(2, 0);

        // Click on 'w' at column 0, row 1 - but line mode stops at hard wrap
        // Should only move to end of first line: h->e->l->l->o = 4 movements
        let click_pin = s
            .pages
            .pin(point::Point::Active(point::Coordinate { x: 0, y: 1 }))
            .unwrap();
        let result = s.prompt_click_move(click_pin);

        // Should stop at end of first line, not cross hard wrap
        assert_eq!(result.right, 5);
        assert_eq!(result.left, 0);

        s.deinit();
    }

    // test "Screen: promptClickMove line right stops at non-continuation row"
    #[test]
    fn screen_prompt_click_move_line_right_stops_at_non_continuation_row() {
        let mut s = clone_clear_init(20, 5, 0);

        // Enable line click mode
        s.semantic_prompt.click = SemanticClick::Cl(osc::semantic_prompt::Click::Line);

        // Row 0: PROMPT "> hello"
        s.cursor_set_semantic_content(SemanticContentArg::Prompt(
            osc::semantic_prompt::PromptKind::Initial,
        ));
        s.test_write_string(b"> ").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Input(
            SemanticContentInput::ClearExplicit,
        ));
        s.test_write_string(b"hello\n").unwrap();

        // Row 1: CONTINUATION "world"
        s.cursor_set_semantic_content(SemanticContentArg::Prompt(
            osc::semantic_prompt::PromptKind::Continuation,
        ));
        s.cursor_set_semantic_content(SemanticContentArg::Input(
            SemanticContentInput::ClearExplicit,
        ));
        s.test_write_string(b"world\n").unwrap();

        // Row 2: NEW PROMPT "> again"
        s.cursor_set_semantic_content(SemanticContentArg::Prompt(
            osc::semantic_prompt::PromptKind::Initial,
        ));
        s.test_write_string(b"> ").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Input(
            SemanticContentInput::ClearExplicit,
        ));
        s.test_write_string(b"again").unwrap();

        // Verify content
        {
            let contents = s
                .dump_string_alloc(
                    Allocator,
                    point::Point::Viewport(point::Coordinate::default()),
                )
                .unwrap();
            assert_eq!(contents, b"> hello\nworld\n> again");
        }

        // Move cursor to 'w' at column 0, row 1
        s.cursor_absolute(0, 1);

        // Click on 'a' at column 2, row 2 - but row 2 is a new prompt
        // Should stop at end of "world": w->o->r->l->d = 4 movements
        let click_pin = s
            .pages
            .pin(point::Point::Active(point::Coordinate { x: 2, y: 2 }))
            .unwrap();
        let result = s.prompt_click_move(click_pin);

        // Should stop at 'd' (end of world), not cross to new prompt
        assert_eq!(result.right, 5);
        assert_eq!(result.left, 0);

        s.deinit();
    }

    // test "Screen: promptClickMove line left basic"
    #[test]
    fn screen_prompt_click_move_line_left_basic() {
        let mut s = clone_clear_init(20, 5, 0);

        // Enable line click mode
        s.semantic_prompt.click = SemanticClick::Cl(osc::semantic_prompt::Click::Line);

        // Write a prompt and input
        s.cursor_set_semantic_content(SemanticContentArg::Prompt(
            osc::semantic_prompt::PromptKind::Initial,
        ));
        s.test_write_string(b"> ").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Input(
            SemanticContentInput::ClearExplicit,
        ));
        s.test_write_string(b"hello").unwrap();

        // Cursor is at column 7 (after 'o'), move it to column 6 (the 'o')
        s.cursor_absolute(6, 0);

        // Click on 'h' (column 2), should require 4 left movements (o->l->l->e->h)
        let click_pin = s
            .pages
            .pin(point::Point::Active(point::Coordinate { x: 2, y: 0 }))
            .unwrap();
        let result = s.prompt_click_move(click_pin);

        assert_eq!(result.left, 4);
        assert_eq!(result.right, 0);

        s.deinit();
    }

    // test "Screen: promptClickMove line left skips non-input cells"
    #[test]
    fn screen_prompt_click_move_line_left_skips_non_input_cells() {
        let mut s = clone_clear_init(20, 5, 0);

        // Enable line click mode
        s.semantic_prompt.click = SemanticClick::Cl(osc::semantic_prompt::Click::Line);

        // Write: "> h" then output "X" then input "llo"
        s.cursor_set_semantic_content(SemanticContentArg::Prompt(
            osc::semantic_prompt::PromptKind::Initial,
        ));
        s.test_write_string(b"> ").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Input(
            SemanticContentInput::ClearExplicit,
        ));
        s.test_write_string(b"h").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Output);
        s.test_write_string(b"X").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Input(
            SemanticContentInput::ClearExplicit,
        ));
        s.test_write_string(b"llo").unwrap();

        // Move cursor to column 6 (the 'o')
        s.cursor_absolute(6, 0);

        // Click on 'h' at column 2 - should skip the 'X' output cell
        // Movement: o->l->l->h = 3 left movements (skipping X)
        let click_pin = s
            .pages
            .pin(point::Point::Active(point::Coordinate { x: 2, y: 0 }))
            .unwrap();
        let result = s.prompt_click_move(click_pin);

        assert_eq!(result.left, 3);
        assert_eq!(result.right, 0);

        s.deinit();
    }

    // test "Screen: promptClickMove line left soft-wrapped line"
    #[test]
    fn screen_prompt_click_move_line_left_soft_wrapped_line() {
        let mut s = clone_clear_init(10, 5, 0);

        // Enable line click mode
        s.semantic_prompt.click = SemanticClick::Cl(osc::semantic_prompt::Click::Line);

        // Write a prompt and input that wraps
        s.cursor_set_semantic_content(SemanticContentArg::Prompt(
            osc::semantic_prompt::PromptKind::Initial,
        ));
        s.test_write_string(b"> ").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Input(
            SemanticContentInput::ClearExplicit,
        ));
        // Write 8 chars of input, first row has 2 for prompt + 8 input = 10 cols
        s.test_write_string(b"abcdefgh").unwrap();
        // Continue on next row (soft-wrapped)
        s.test_write_string(b"ij").unwrap();

        // Verify soft wrap occurred
        {
            let contents = s
                .dump_string_alloc(
                    Allocator,
                    point::Point::Viewport(point::Coordinate::default()),
                )
                .unwrap();
            assert_eq!(contents, b"> abcdefgh\nij");
        }

        // Cursor is at column 2, row 1 (after 'j'). Move to 'j' at column 1.
        s.cursor_absolute(1, 1);

        // Click on 'a' at column 2, row 0 - should count all input cells backwards
        // Movement: j->i->h->g->f->e->d->c->b->a = 9 left movements
        let click_pin = s
            .pages
            .pin(point::Point::Active(point::Coordinate { x: 2, y: 0 }))
            .unwrap();
        let result = s.prompt_click_move(click_pin);

        assert_eq!(result.left, 9);
        assert_eq!(result.right, 0);

        s.deinit();
    }

    // test "Screen: promptClickMove line left stops at hard wrap"
    #[test]
    fn screen_prompt_click_move_line_left_stops_at_hard_wrap() {
        let mut s = clone_clear_init(20, 5, 0);

        // Enable line click mode
        s.semantic_prompt.click = SemanticClick::Cl(osc::semantic_prompt::Click::Line);

        // Write prompt and input on first line, then hard wrap
        s.cursor_set_semantic_content(SemanticContentArg::Prompt(
            osc::semantic_prompt::PromptKind::Initial,
        ));
        s.test_write_string(b"> ").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Input(
            SemanticContentInput::ClearExplicit,
        ));
        s.test_write_string(b"hello").unwrap();
        // Hard wrap (newline)
        s.test_write_string(b"\n").unwrap();
        s.test_write_string(b"world").unwrap();

        // Move cursor to 'd' at column 4, row 1 (an actual input cell)
        s.cursor_absolute(4, 1);

        // Click on 'h' at column 2, row 0 - but line mode stops at hard wrap
        // Should only move to start of second line, not cross to row 0
        let click_pin = s
            .pages
            .pin(point::Point::Active(point::Coordinate { x: 2, y: 0 }))
            .unwrap();
        let result = s.prompt_click_move(click_pin);

        // Should stop at start of second line: d->l->r->o->w = 4 movements
        assert_eq!(result.left, 4);
        assert_eq!(result.right, 0);

        s.deinit();
    }

    // test "Screen: promptClickMove click right of input same line"
    #[test]
    fn screen_prompt_click_move_click_right_of_input_same_line() {
        // Set up: "> hello" where "> " is prompt and "hello" is input
        // Clicking to the right of the 'o' should move cursor past the input

        let mut s = clone_clear_init(20, 5, 0);

        // Enable line click mode
        s.semantic_prompt.click = SemanticClick::Cl(osc::semantic_prompt::Click::Line);

        // Write a prompt and input
        s.cursor_set_semantic_content(SemanticContentArg::Prompt(
            osc::semantic_prompt::PromptKind::Initial,
        ));
        s.test_write_string(b"> ").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Input(
            SemanticContentInput::ClearExplicit,
        ));
        s.test_write_string(b"hello").unwrap();

        // Move cursor to start of input (column 2, the 'h')
        s.cursor_absolute(2, 0);

        // Click beyond the input (column 15) - should move to one past the 'o'
        let click_pin = s
            .pages
            .pin(point::Point::Active(point::Coordinate { x: 15, y: 0 }))
            .unwrap();
        let result = s.prompt_click_move(click_pin);

        assert_eq!(result.right, 5);
        assert_eq!(result.left, 0);

        s.deinit();
    }

    // test "Screen: promptClickMove click right of input cursor at end"
    #[test]
    fn screen_prompt_click_move_click_right_of_input_cursor_at_end() {
        let mut s = clone_clear_init(20, 5, 0);

        // Enable line click mode
        s.semantic_prompt.click = SemanticClick::Cl(osc::semantic_prompt::Click::Line);

        // Write a prompt and input
        s.cursor_set_semantic_content(SemanticContentArg::Prompt(
            osc::semantic_prompt::PromptKind::Initial,
        ));
        s.test_write_string(b"> ").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Input(
            SemanticContentInput::ClearExplicit,
        ));
        s.test_write_string(b"hello").unwrap();

        // Cursor is already at column 7 (one past 'o') after writing
        // Click beyond the input (column 15) - no movement needed since
        // cursor is already at the end position
        let click_pin = s
            .pages
            .pin(point::Point::Active(point::Coordinate { x: 15, y: 0 }))
            .unwrap();
        let result = s.prompt_click_move(click_pin);

        assert_eq!(result.right, 0);
        assert_eq!(result.left, 0);

        s.deinit();
    }

    // test "Screen: promptClickMove click right of input on lower line"
    #[test]
    fn screen_prompt_click_move_click_right_of_input_on_lower_line() {
        let mut s = clone_clear_init(20, 5, 0);

        // Enable line click mode
        s.semantic_prompt.click = SemanticClick::Cl(osc::semantic_prompt::Click::Line);

        // Write a prompt and input
        s.cursor_set_semantic_content(SemanticContentArg::Prompt(
            osc::semantic_prompt::PromptKind::Initial,
        ));
        s.test_write_string(b"> ").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Input(
            SemanticContentInput::ClearExplicit,
        ));
        s.test_write_string(b"hello").unwrap();

        // Move cursor to start of input (column 2, the 'h')
        s.cursor_absolute(2, 0);

        // Click on a lower line (row 1) - should move to end of input
        // This is outside the prompt area so should clamp to end
        let click_pin = s
            .pages
            .pin(point::Point::Active(point::Coordinate { x: 5, y: 1 }))
            .unwrap();
        let result = s.prompt_click_move(click_pin);

        // From 'h', we need to pass e, l, l, o (4 cells) + 1 past end = 5
        assert_eq!(result.right, 5);
        assert_eq!(result.left, 0);

        s.deinit();
    }

    // test "Screen: promptClickMove click right of input cursor at end lower line"
    #[test]
    fn screen_prompt_click_move_click_right_of_input_cursor_at_end_lower_line() {
        let mut s = clone_clear_init(20, 5, 0);

        // Enable line click mode
        s.semantic_prompt.click = SemanticClick::Cl(osc::semantic_prompt::Click::Line);

        // Write a prompt and input
        s.cursor_set_semantic_content(SemanticContentArg::Prompt(
            osc::semantic_prompt::PromptKind::Initial,
        ));
        s.test_write_string(b"> ").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Input(
            SemanticContentInput::ClearExplicit,
        ));
        s.test_write_string(b"hello").unwrap();

        // Cursor is at column 7 after writing (one past 'o')
        // Click on a lower line (row 1) - cursor already at end, no movement needed
        let click_pin = s
            .pages
            .pin(point::Point::Active(point::Coordinate { x: 5, y: 1 }))
            .unwrap();
        let result = s.prompt_click_move(click_pin);

        assert_eq!(result.right, 0);
        assert_eq!(result.left, 0);

        s.deinit();
    }

    // test "Screen: promptClickMove click right of input cursor on last char"
    #[test]
    fn screen_prompt_click_move_click_right_of_input_cursor_on_last_char() {
        let mut s = clone_clear_init(20, 5, 0);

        // Enable line click mode
        s.semantic_prompt.click = SemanticClick::Cl(osc::semantic_prompt::Click::Line);

        // Write a prompt and input
        s.cursor_set_semantic_content(SemanticContentArg::Prompt(
            osc::semantic_prompt::PromptKind::Initial,
        ));
        s.test_write_string(b"> ").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Input(
            SemanticContentInput::ClearExplicit,
        ));
        s.test_write_string(b"hello").unwrap();

        // Move cursor to last input char (column 6, the 'o')
        s.cursor_absolute(6, 0);

        // Click beyond the input (column 15)
        let click_pin = s
            .pages
            .pin(point::Point::Active(point::Coordinate { x: 15, y: 0 }))
            .unwrap();
        let result = s.prompt_click_move(click_pin);

        assert_eq!(result.right, 1);
        assert_eq!(result.left, 0);

        s.deinit();
    }

    // test "selectionString map allocation failure cleanup"
    #[test]
    fn selection_string_map_allocation_failure_cleanup() {
        use rustty_util::tripwire::ResetMode;

        // This test verifies that if toOwnedSlice fails when building
        // the StringMap, we don't leak the already-allocated map.string.
        let mut s = clone_clear_init(10, 5, 0);

        s.test_write_string(b"hello").unwrap();

        // Get a selection
        let sel = Selection::init(
            s.pages
                .pin(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
                .unwrap(),
            s.pages
                .pin(point::Point::Active(point::Coordinate { x: 4, y: 0 }))
                .unwrap(),
            false,
        );

        // Trigger allocation failure on toOwnedSlice
        let mut map: StringMap = StringMap {
            string: Vec::new(),
            map: Vec::new(),
        };
        SELECTION_STRING_TW.with(|tw| {
            tw.borrow_mut()
                .errorAlways(SelectionStringFp::CopyMap, OutOfMemory)
        });
        let result = s.selection_string(
            Allocator,
            SelectionString {
                sel,
                // PORT NOTE: the reference omits `.trim`, taking the
                // `SelectionString.trim` field default of `true`.
                trim: true,
                map: Some(&mut map),
            },
        );
        assert!(matches!(result, Err(OutOfMemory)));
        SELECTION_STRING_TW.with(|tw| tw.borrow_mut().end(ResetMode::Reset).unwrap());

        // If this test passes without memory leaks (when run with testing.allocator),
        // it means the errdefer properly cleaned up map.string when toOwnedSlice failed.

        s.deinit();
    }
}
