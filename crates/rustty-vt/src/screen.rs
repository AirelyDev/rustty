//! rustty-vt screen
//!
//! PORTS: ghostty src/terminal/Screen.zig
//!
//! PORT STATUS: done (vt-11) — Screen struct + nested types + method bodies are
//! ported and filled; no `todo!()` bodies remain.
// PORT NOTE: the method bodies are filled (vt-11). The `unused_*` and
// `clippy::should_implement_trait` allows are retained from the skeleton phase
// where the filled file still trips them (unread params/imports, trait-name
// overlaps); they no longer imply unfinished bodies.
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
use crate::pagelist::{self, Key, OutOfMemory, PageList, Pin};
use crate::point;
use crate::selection;
use crate::selection::Selection;
use crate::size;
use crate::string_map::StringMap;
use crate::style;
use crate::terminal::PrintStringError;

// PORT NOTE: the reference `Allocator.Error` maps to `pagelist::OutOfMemory`;
// the general-purpose allocator maps to `crate::alloc::Global`, the
// same allocator the PageList pools use (the crate's allocator seam).
use crate::alloc::Allocator;

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

// const log = std.log.scoped(.screen);
const LOG_SCOPE: &str = "screen";

// pub const CursorStyle = @import("cursor.zig").Style;
pub use crate::cursor::Style as CursorStyle;

/// Represents the terminal screen.
pub struct Screen {
    // io: std.Io
    //
    // The reference documents this field as:
    //
    //   The I/O implementation to use for any operation that requires one.
    //
    // PORT NOTE (`e8525c0fd` "Update to Zig 0.16.0"): `std.Io` is the Zig 0.16
    // explicit-I/O-context rework, threaded through `Screen.init`/`deinit` and
    // every operation that can block. Rust has no equivalent ambient-context
    // parameter and rustty's screen operations do no I/O, so the field has no
    // counterpart here. Classified with the rest of that commit in
    // `.port/phase-3/NON_PORTS.md`.
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
    // `kitty.graphics.ImageStorage` operates over `Screen`/`Terminal`. The
    // `kitty_graphics` gate is `crate::options::KITTY_GRAPHICS` (always true off
    // wasm-freestanding, matching the reference); the field is always the real
    // `ImageStorage` here and the gate is enforced at the dispatch call sites
    // (see the `KITTY_GRAPHICS` guards below), mirroring the "empty struct when
    // disabled" behavior at usage.
    pub kitty_images: kitty::graphics_storage::ImageStorage,

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
    //
    // PORT NOTE (parity tail S7-3): allocated through the screen's allocator
    // (`create_cursor_hyperlink`: `alloc.create(Hyperlink)` + `source.dupe(alloc)`)
    // and freed through it (`destroy_cursor_hyperlink`), bytes included.
    pub hyperlink: Option<*mut CursorHyperlink>,

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
    /// `std.meta.eql(a, b)` for a `Cursor`.
    ///
    /// PORT NOTE (S143-F1, no reference counterpart): the reference's resize
    /// failure test asserts `std.meta.eql(before_primary.cursor, primary.cursor)`
    /// — the WHOLE cursor, including `pending_wrap`, the style, the hyperlink
    /// state and the three page pointers. `std.meta.eql` compares pointer
    /// fields by ADDRESS (it does not follow them) and recurses into struct
    /// fields, which is what this reproduces; `Style` carries its own `eql`
    /// (the reference's `Style` is a plain struct that `std.meta.eql` walks).
    ///
    /// Both sides are destructured rather than compared field-by-field, so a
    /// field added to `Cursor` fails to compile here instead of silently
    /// dropping out of the comparison — the reflection the reference gets from
    /// `std.meta.eql` for free.
    #[cfg(test)]
    pub(crate) fn meta_eql(&self, other: &Cursor) -> bool {
        let Cursor {
            x,
            y,
            cursor_style,
            pending_wrap,
            protected,
            style,
            style_id,
            hyperlink_id,
            hyperlink_implicit_id,
            hyperlink,
            semantic_content,
            semantic_content_clear_eol,
            page_pin,
            page_row,
            page_cell,
        } = self;
        let Cursor {
            x: o_x,
            y: o_y,
            cursor_style: o_cursor_style,
            pending_wrap: o_pending_wrap,
            protected: o_protected,
            style: o_style,
            style_id: o_style_id,
            hyperlink_id: o_hyperlink_id,
            hyperlink_implicit_id: o_hyperlink_implicit_id,
            hyperlink: o_hyperlink,
            semantic_content: o_semantic_content,
            semantic_content_clear_eol: o_semantic_content_clear_eol,
            page_pin: o_page_pin,
            page_row: o_page_row,
            page_cell: o_page_cell,
        } = other;
        x == o_x
            && y == o_y
            && cursor_style == o_cursor_style
            && pending_wrap == o_pending_wrap
            && protected == o_protected
            && style.eql(o_style)
            && style_id == o_style_id
            && hyperlink_id == o_hyperlink_id
            && hyperlink_implicit_id == o_hyperlink_implicit_id
            && hyperlink == o_hyperlink
            && semantic_content == o_semantic_content
            && semantic_content_clear_eol == o_semantic_content_clear_eol
            && page_pin == o_page_pin
            && page_row == o_page_row
            && page_cell == o_page_cell
    }

    pub fn deinit(&mut self, alloc: Allocator) {
        // if (self.hyperlink) |link| {
        //     link.deinit(alloc);
        //     alloc.destroy(link);
        // }
        // PORT NOTE: the link and its bytes were allocated through the screen's
        // allocator, which the link records (`destroy_cursor_hyperlink` frees
        // through it); `alloc` is that same allocator.
        if let Some(link) = self.hyperlink.take() {
            // SAFETY: `link` came from `create_cursor_hyperlink` and is owned by
            // this cursor.
            unsafe {
                (*link).uri.allocator().debug_assert_same(alloc);
                destroy_cursor_hyperlink(link);
            }
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
// PORT NOTE: Debug/PartialEq are derived for the snapshot codec's saved-cursor
// structs (`snapshot/screen.rs`), which hold a `CharsetState` and compare it
// structurally (Zig: `std.meta.eql` via `std.testing.expectEqual`).
#[derive(Clone, Debug, PartialEq)]
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
// PORT NOTE: Debug/PartialEq — see CharsetState.
#[derive(Clone, Debug, PartialEq)]
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

    /// The maximum size of scrollback in bytes. Null is unlimited, zero
    /// disables scrollback, and any other value is clamped to support a
    /// minimum of the active area.
    pub max_scrollback_bytes: Option<usize>,

    /// The maximum number of physical scrollback rows, excluding the active
    /// area. Null is unlimited. The effective limit permits at least one
    /// standard page and only complete historical pages are pruned.
    pub max_scrollback_lines: Option<usize>,

    /// The total storage limit for Kitty images in bytes for this
    /// screen. Kitty image storage is per-screen.
    // kitty_image_storage_limit: usize = switch (build_options.artifact) {
    //     .ghostty => 320 * 1000 * 1000, // 320MB
    //     .lib => 10 * 1000 * 1000, // 10MB
    // },
    // PORT NOTE: a Rust struct field has no default initializer; the
    // reference's artifact-selected field default is
    // `Options::kitty_image_storage_limit_default(artifact)` below.
    pub kitty_image_storage_limit: usize,

    /// The limits for what medium types are allowed for Kitty image loading.
    // PORT NOTE: the reference type is
    //   `if (build_options.kitty_graphics) kitty.graphics.LoadingImage.Limits else void`.
    // The reference nests `Limits` under `LoadingImage`; the Rustty port carves
    // it as a sibling in `graphics_image`, so this is `graphics_image::Limits`.
    // The conditional `void` (kitty graphics disabled) is the empty/no-limit case.
    pub kitty_image_loading_limits: kitty::graphics_image::Limits,
}

impl Options {
    /// The reference's field default for `kitty_image_storage_limit`, which
    /// depends on the build artifact.
    // PORT NOTE: the reference's `switch (build_options.artifact)` field
    // initializer, as a const fn over the artifact so that a caller building a
    // different artifact than `crate::options::ARTIFACT` (rustty-vt-c is the
    // `.lib` artifact) can select its own default.
    pub const fn kitty_image_storage_limit_default(
        artifact: crate::build_options::Artifact,
    ) -> usize {
        match artifact {
            crate::build_options::Artifact::Ghostty => 320 * 1000 * 1000, // 320MB
            crate::build_options::Artifact::Lib => 10 * 1000 * 1000,      // 10MB
        }
    }

    /// A simple, default terminal. If you rely on specific dimensions or
    /// scrollback (or lack of) then do not use this directly. This is just
    /// for callers that need some defaults.
    pub fn default() -> Options {
        // pub const default: Options = .{
        //     .cols = 80,
        //     .rows = 24,
        //     .max_scrollback_bytes = 0,
        // };
        // PORT NOTE: the reference `Options.default` only sets cols/rows/
        // max_scrollback_bytes; the remaining fields take their struct-default values
        // (kitty_image_storage_limit per artifact; kitty_image_loading_limits
        // `.direct`), written out here.
        // CORRECTION (parity S133-F2): this note used to end "The kitty
        // graphics fields are the deferred placeholders (kitty graphics is
        // DEFERRED to vt-12), so they default trivially here", and the limit was
        // `0`, a zero-byte image budget. Kitty graphics is ported and
        // `Screen::init` hands this value to `ImageStorage::set_limit`.
        Options {
            cols: 80,
            rows: 24,
            max_scrollback_bytes: Some(0),
            max_scrollback_lines: None,
            kitty_image_storage_limit: Options::kitty_image_storage_limit_default(
                crate::options::ARTIFACT,
            ),
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

pub struct SelectionString {
    /// The selection to convert to a string.
    pub sel: Selection,

    /// If true, trim whitespace around the selection.
    pub trim: bool,
}

// PORT NOTE (f1948d505): the `selectionString_tw` tripwire module (fail point
// `copy_map`) and its `map: ?*StringMap` duplicate-copy shape were removed
// upstream along with the `selectionString map allocation failure cleanup`
// test: `selectionStringMap` now TRANSFERS the formatter output and pin map
// into the returned `StringMap`, so there is no duplicated string left to
// leak on allocation failure.

// PORT NOTE (parity tail S7-3): `04810273a`'s rollback test uses
// `std.testing.FailingAllocator` on `alloc.create(Hyperlink)`, and so does the
// port now that the cursor hyperlink is allocated through `Screen.alloc`. The
// port-only tripwire stand-in (`StartHyperlinkFp`) that used to sit here is
// gone.

/// The cursor's heap hyperlink: the reference's `*Hyperlink` whose bytes were
/// duplicated with the screen's allocator (see `Cursor.hyperlink`).
pub type CursorHyperlink = hyperlink::Hyperlink<hyperlink::AllocBytes>;

/// `const link = try alloc.create(Hyperlink); link.* = try source.dupe(alloc);`
/// with both `errdefer`s: the struct and its bytes are allocated through
/// `alloc`, and on failure nothing is left allocated.
fn create_cursor_hyperlink<B: core::ops::Deref<Target = [u8]>>(
    alloc: Allocator,
    source: &hyperlink::Hyperlink<B>,
) -> Result<*mut CursorHyperlink, hyperlink::OutOfMemory> {
    use allocator_api2::alloc::Allocator as _;
    let layout = core::alloc::Layout::new::<CursorHyperlink>();
    // const link = try alloc.create(hyperlink.Hyperlink);
    let raw = alloc
        .allocate(layout)
        .map_err(|_| hyperlink::OutOfMemory)?
        .cast::<CursorHyperlink>();
    // link.* = try source.dupe(alloc);
    match source.dupe_in(alloc) {
        Ok(value) => {
            // SAFETY: `raw` is a fresh allocation of `CursorHyperlink`'s layout.
            unsafe { raw.as_ptr().write(value) };
            Ok(raw.as_ptr())
        }
        Err(e) => {
            // errdefer alloc.destroy(link);
            // SAFETY: `raw`/`layout` are the pair allocated above, unwritten.
            unsafe { alloc.deallocate(raw.cast(), layout) };
            Err(e)
        }
    }
}

/// `link.deinit(alloc); alloc.destroy(link);` through the allocator the link
/// was created with (its bytes record it).
///
/// # Safety
/// `link` came from [`create_cursor_hyperlink`] and is not used afterwards.
unsafe fn destroy_cursor_hyperlink(link: *mut CursorHyperlink) {
    use allocator_api2::alloc::Allocator as _;
    // SAFETY: the caller's contract: `link` is live and exclusively ours.
    unsafe {
        let alloc: Allocator = *(*link).uri.allocator();
        // link.deinit(alloc): the byte vectors free through `alloc` on drop.
        core::ptr::drop_in_place(link);
        // alloc.destroy(link);
        alloc.deallocate(
            core::ptr::NonNull::new_unchecked(link).cast(),
            core::alloc::Layout::new::<CursorHyperlink>(),
        );
    }
}

// PORT NOTE (parity S133-F3): the reference exercises `cursorCopy`'s
// `errdefer self.cursor = old` by making `manualStyleUpdate` fail, which in Zig
// takes a `std.testing.failing_allocator` at the style-set growth. The port's
// page memory comes from an owned pool rather than a swappable `Allocator`, so
// the same stand-in as `grow.rs`'s `GROW_TW` is used: a `cfg(test)` fail point
// at exactly the `try` the reference's `errdefer` guards. Its error type is
// `IncreaseCapacityError` so the test can also prove `OutOfSpace` survives —
// the port used to flatten both variants into `OutOfMemory`.
#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum CursorCopyFp {
    ManualStyleUpdate,
}

#[cfg(test)]
thread_local! {
    pub(crate) static CURSOR_COPY_TW: core::cell::RefCell<
        rustty_util::tripwire::Module<CursorCopyFp, pagelist::grow::IncreaseCapacityError>,
    > = core::cell::RefCell::new(rustty_util::tripwire::module());
}

#[cfg(test)]
#[inline]
fn cursor_copy_tw_check(point: CursorCopyFp) -> Result<(), pagelist::grow::IncreaseCapacityError> {
    CURSOR_COPY_TW.with(|tw| tw.borrow_mut().check(point))
}

// const resize_tw = tripwire.module(enum {
//     saved_cursor_pin,
//     pages,
// }, resize);
//
// PORT NOTE: same instance-vs-comptime-type idiom as `START_HYPERLINK_TW`
// above. The fail-point NAMES are the reference's verbatim (`saved_cursor_pin`,
// `pages`) so the parity is free. Upstream's `Screen: resize errors preserve
// state` test iterates `std.meta.tags(resize_tw.FailPoint)`; the Rust port
// iterates `ResizeFp::ALL`, which must be kept in sync with the enum.
#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum ResizeFp {
    SavedCursorPin,
    Pages,
}

#[cfg(test)]
impl ResizeFp {
    /// `std.meta.tags(resize_tw.FailPoint)`.
    pub(crate) const ALL: [ResizeFp; 2] = [ResizeFp::SavedCursorPin, ResizeFp::Pages];
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
#[derive(Debug)]
enum StartHyperlinkOnceError {
    OutOfMemory,
    Insert(crate::page::InsertHyperlinkError),
}

// PORT NOTE: the error set for `dumpStringAlloc`/`dumpStringAllocUnwrapped` is
// inferred in the reference as `(Allocator.Error || error{UnknownPoint})`: the
// `getBottomRight(tl) orelse return error.UnknownPoint` arm (Screen.zig:3616,
// :3635) is reachable for a `.history` point on a screen with no scrollback
// (PageList.getBottomRight returns null when the active top-left has no row
// above it). Ported as an explicit enum so the two errors stay distinct.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DumpStringAllocError {
    OutOfMemory,
    UnknownPoint,
}

impl From<OutOfMemory> for DumpStringAllocError {
    fn from(_: OutOfMemory) -> DumpStringAllocError {
        DumpStringAllocError::OutOfMemory
    }
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
    /// max_scrollback_bytes is the amount of scrollback to keep in bytes. This
    /// will be rounded UP to the nearest page size because our minimum allocation
    /// size is that anyways.
    ///
    /// If max scrollback is 0, then no scrollback is kept at all.
    pub fn init(alloc: Allocator, opts: Options) -> Result<Screen, OutOfMemory> {
        // Initialize our backing pages.
        // var pages = try PageList.init(alloc, .{
        //     .cols = opts.cols,
        //     .rows = opts.rows,
        //     .max_size = opts.max_scrollback_bytes,
        //     .max_lines = opts.max_scrollback_lines,
        // });
        // errdefer pages.deinit();
        let mut pages = PageList::init(
            alloc,
            pagelist::Options {
                max_size: opts.max_scrollback_bytes,
                max_lines: opts.max_scrollback_lines,
                ..pagelist::Options::new(opts.cols, opts.rows)
            },
        )?;

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

        let mut result: Screen = Screen {
            alloc,
            pages,
            no_scrollback: opts.max_scrollback_bytes == Some(0),
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
            // The kitty graphics storage starts empty; the limit/loading-limits
            // are initialized below (see the `KITTY_GRAPHICS` block) just as the
            // reference does after building `result`.
            kitty_images: kitty::graphics_storage::ImageStorage::default(),
            semantic_prompt: SemanticPrompt::DISABLED,
            dirty: Dirty::default(),
        };

        // if (comptime build_options.kitty_graphics) {
        if crate::options::KITTY_GRAPHICS {
            // result.kitty_images.setLimit(io, alloc, &result, opts.kitty_image_storage_limit);
            // (`4371871bc` made `setLimit` infallible — the pre-range `catch
            // unreachable` and its can't-fail comment are gone upstream.)
            // PORT NOTE: `setLimit` takes `&mut self` (the `kitty_images` field)
            // and the screen it lives in (now a raw `*mut Screen`, per the
            // storage's PORT NOTE). We keep the MOVE-out shape: take
            // `kitty_images` out of `result` (leaving a default placeholder),
            // invoke the method on the owned storage with a raw `*mut result`,
            // then move it back — sound and observably identical (`setLimit`
            // only reads/mutates the storage + `s.pages`, disjoint from the
            // now-empty `kitty_images` slot).
            let mut images = core::mem::take(&mut result.kitty_images);
            images.set_limit(
                alloc,
                &mut result as *mut Screen,
                opts.kitty_image_storage_limit,
            );
            // result.kitty_images.image_limits = opts.kitty_image_loading_limits;
            images.image_limits = opts.kitty_image_loading_limits;
            result.kitty_images = images;
        }

        Ok(result)
    }

    pub fn deinit(&mut self) {
        // if (comptime build_options.kitty_graphics) {
        //     self.kitty_images.deinit(self.alloc, self);
        // }
        // PORT NOTE: `deinit` takes `&mut self` (the `kitty_images` field) and
        // `s: &mut Screen` (`self`). Borrowing `self.kitty_images` as the receiver
        // while also passing `self` aliases (Stacked-Borrows UB), so we MOVE the
        // storage out (leaving a default placeholder), `deinit` it against a clean
        // `&mut *self`, and drop it. `deinit` only touches `s.pages` (via
        // `untrackPin` inside `clearPlacements`), disjoint from the now-empty
        // `kitty_images` slot. `self.alloc` is `Copy`.
        if crate::options::KITTY_GRAPHICS {
            let alloc = self.alloc;
            let mut images = core::mem::take(&mut self.kitty_images);
            images.deinit(alloc, self as *mut Screen);
        }
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
        // analogue here, so it is elided (we never run under Valgrind). The
        // reference's verbatim comments for that elided check are preserved here:
        //   We don't run integrity checks on Valgrind because its soooooo slow,
        //   Valgrind is our integrity checker, and we run these during unit
        //   tests (non-Valgrind) anyways so we're verifying anyways.
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

            // The cursor style and hyperlink if non-zero must reference
            // real data in the page the pin is in.
            // const page: *const Page = self.cursor.page_pin.node.page();
            let page: *mut Page =
                unsafe { crate::pagelist::Node::page((*self.cursor.page_pin).node) };
            if self.cursor.style_id != style::DEFAULT_ID {
                debug_assert!(
                    unsafe {
                        (*page)
                            .styles
                            .ref_count((*page).memory_ptr(), self.cursor.style_id)
                    } > 0
                );
            }
            if self.cursor.hyperlink_id != 0 {
                debug_assert!(
                    unsafe {
                        (*page)
                            .hyperlink_set
                            .ref_count((*page).memory_ptr(), self.cursor.hyperlink_id)
                    } > 0
                );
            }
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
        // our cursor pin, which should be at the top-left already. The
        // reset marks every tracked pin as garbage, but we keep using
        // this one at its new valid position, so clear the flag: copies
        // of the cursor pin (e.g. for Kitty image placements) must not
        // be born garbage.
        //
        // PORT NOTE (oracle `d30a9c424`, ridden in with the `33ca9e5dd`
        // relative-placement end state — its "placements created after a full
        // reset are retained" regression test requires this fix).
        let cursor_pin: *mut Pin = self.cursor.page_pin;
        debug_assert!(std::ptr::eq(
            unsafe { (*cursor_pin).node },
            self.pages.pages.first
        ));
        debug_assert!(unsafe { (*cursor_pin).x } == 0);
        debug_assert!(unsafe { (*cursor_pin).y } == 0);
        // cursor_pin.garbage = false;
        // SAFETY: `cursor_pin` is the screen's own tracked cursor pin.
        unsafe { (*cursor_pin).garbage = false };
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

        // if (comptime build_options.kitty_graphics) {
        //     // Reset kitty graphics storage
        //     const image_limits = self.kitty_images.image_limits;
        //     const total_limit = self.kitty_images.total_limit;
        //     self.kitty_images.deinit(self.alloc, self);
        //     self.kitty_images = .{
        //         .dirty = true,
        //         .image_limits = image_limits,
        //         .total_limit = total_limit,
        //     };
        // }
        // PORT NOTE: the self-referential `&mut Screen` borrow (`deinit` takes both
        // `&mut self.kitty_images` and `self`) is avoided by MOVING the storage out
        // (default placeholder) and `deinit`-ing it against a clean `&mut *self`;
        // `deinit` only touches `s.pages`, disjoint from the empty slot. The
        // `.{ .dirty = true }` reset is the `ImageStorage` default with `dirty` on,
        // carrying the CONFIGURED limits across (`bcbc93a6b`: RIS previously
        // cleared the storage limits and allowed mediums; they are configuration,
        // not terminal state, so a reset must retain them).
        if crate::options::KITTY_GRAPHICS {
            // Reset kitty graphics storage
            let alloc = self.alloc;
            let image_limits = self.kitty_images.image_limits.clone();
            let total_limit = self.kitty_images.total_limit;
            let mut images = core::mem::take(&mut self.kitty_images);
            images.deinit(alloc, self as *mut Screen);
            self.kitty_images = kitty::graphics_storage::ImageStorage {
                dirty: true,
                image_limits,
                total_limit,
                ..kitty::graphics_storage::ImageStorage::default()
            };
        }

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
    ) -> Result<Screen, pagelist::access::CloneError> {
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
            // var pin_remap = PageList.Clone.TrackedPinsRemap.init(alloc);
            // PERF(port): a default hasher (not `RandomState`) since the remap
            // uses the reference's cheap pointer hasher.
            tracked_pins: Some(pagelist::access::TrackedPinsRemap::with_hasher_in(
                Default::default(),
                alloc,
            )),
        };

        // PORT NOTE (parity S133-F4): `PageList::clone` returns `CloneError`
        // (OutOfMemory | CloneFrom), and the reference's `clone` returns `!Screen`
        // whose INFERRED error set is `PageList.clone`'s — which includes
        // `Page.CloneFromError` (`StyleSetError || HyperlinkError ||
        // GraphemeError`, e.g. `NeedsRehash`) from `PageList.zig:1173`'s
        // `try dst_page.cloneFrom(...)`. An earlier note here claimed "the only
        // failure paths reachable here are allocator failures" and collapsed
        // everything into `OutOfMemory`; `CloneError::CloneFrom` exists precisely
        // because `clone_from` can fail with set/rehash errors, so the error set
        // is propagated unchanged instead.
        let mut pages = self.pages.clone(alloc, &mut opts)?;
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
                        // const node = pages.pages.first.?;
                        let node = pages.pages.first;
                        break 'start pages.track_pin(Pin {
                            node,
                            // .x = if (sel.rectangle) @min(ordered.tl.x, node.cols() - 1)
                            //      else 0
                            x: if sel.rectangle {
                                let cols = unsafe { crate::pagelist::Node::cols(node) };
                                core::cmp::min(unsafe { (*ordered.tl).x }, cols - 1)
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
                    None => {
                        // const node = pages.pages.last.?;
                        let node = pages.pages.last;
                        let cols = unsafe { crate::pagelist::Node::cols(node) };
                        pages.track_pin(Pin {
                            node,
                            // .x = if (sel.rectangle) @min(ordered.br.x, node.cols() - 1)
                            //      else node.cols() - 1
                            x: if sel.rectangle {
                                core::cmp::min(unsafe { (*ordered.br).x }, cols - 1)
                            } else {
                                cols - 1
                            },
                            y: unsafe { crate::pagelist::Node::rows(node) } - 1,
                            garbage: false,
                        })?
                    }
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
            kitty_images: kitty::graphics_storage::ImageStorage::default(),
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
        let new_page: *mut Page = unsafe { crate::pagelist::Node::page(new_node) };

        // Re-add the style, if the page somehow doesn't have enough
        // memory to add it, we emit a warning and gracefully degrade
        // to the default style for the cursor.
        if self.cursor.style_id != style::DEFAULT_ID {
            self.cursor.style_id = match unsafe {
                let base = (*new_page).memory_ptr();
                (*new_page).styles.add(base, self.cursor.style)
            } {
                Ok(id) => id,
                Err(err) => {
                    // TODO: Should we increase the capacity further in this case?
                    log::warn!(
                        target: LOG_SCOPE,
                        "(Screen.increaseCapacity) Failed to add cursor style back to page, err={err:?}"
                    );

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
            //
            // PORT NOTE: the reference passes `link.*` by value (borrowed
            // slices, no allocation); `to_heap` is the port's owned argument, on
            // the Rust heap so the caller's allocator sees nothing extra.
            if let Err(err) = self.start_hyperlink_once(unsafe { (*link).to_heap() }) {
                // TODO: Should we increase the capacity further in this case?
                log::warn!(
                    target: LOG_SCOPE,
                    "(Screen.increaseCapacity) Failed to add cursor hyperlink back to page, err={err:?}"
                );
            }

            // Remove our old link
            // link.deinit(self.alloc);
            // self.alloc.destroy(link);
            unsafe { destroy_cursor_hyperlink(link) };
        }

        // Reload the cursor information because the pin changed.
        // So our page row/cell and so on are all off.
        self.cursor_reload();

        Ok(new_node)
    }

    /// Clone the cells in columns [x_start, x_end) of a source row into
    /// the row at `dst_y` of the given node's page, increasing the node's
    /// capacity as necessary to fit the managed memory (styles,
    /// hyperlinks, etc.) of the copied cells.
    ///
    /// This is the Screen-level analog of PageList.cloneRowGrowCapacity:
    /// capacity increases are routed through Screen.increaseCapacity so
    /// that the cursor's style/hyperlink references are migrated when the
    /// destination node is the cursor's page.
    ///
    /// Since increasing capacity replaces the node in the page list, the
    /// (possibly replaced) node is returned and the caller must use it in
    /// place of the old node. Tracked pins are updated automatically. The
    /// source must NOT be on the given node since the node's page memory
    /// may be freed on capacity increase.
    ///
    /// Callers use this mid-mutation (after rows have been rotated or
    /// shifted), so a failure can't be propagated without leaving the
    /// page list half-mutated (and corrupt). On a system OOM while
    /// increasing the capacity this panics: a crash is better than
    /// corruption. When a dimension is already at its maximum capacity the
    /// reference also panics; the port instead degrades that dimension and
    /// finishes the copy (PORT NOTE at the `OutOfSpace` arm below,
    /// UPSTREAM-DEFECTS.md UD131).
    //
    // PORT NOTE: reference defect (R13), UD131 — the reference doc's last
    // sentence (Screen.zig:707-709) describes the crash the port does not
    // keep for the max-capacity case, so it is quoted here rather than in the
    // doc above:
    //   /// page list half-mutated (and corrupt). If the capacity can't be
    //   /// increased (system OOM or the page is already at max capacity),
    //   /// this panics: a crash is better than corruption.
    //
    // UB HAZARD (`P3-W2` T4 Step 3): `src_page` and `node.page()` are two raw
    // `*mut Page`s and `increase_capacity` FREES the latter, which would leave
    // `src_row` dangling on the retry if they aliased. The reference enforces
    // this with `assert(src_page != node.page())`; the port encodes the
    // disjointness as a `debug_assert!` on the node pointers, not a comment.
    //
    // NEGATIVE CONTROL EXECUTED (`P3-W2` T4 Step 4): dropping the retry (return
    // `current` on the first clone error) FAILS both
    // `screen_scroll_above_hyperlink_dense_row_to_{fresh,existing}_page`
    // (`assertion left == right failed; left: 10, right: 2` — the destination
    // page ends up with 2 of the 10 hyperlinks) AND both
    // `terminal_{insert,delete}Lines_hyperlink_dense_row_crosses_page_boundary`.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    #[allow(clippy::too_many_arguments)]
    pub fn clone_partial_row_grow_capacity(
        &mut self,
        node: *mut pagelist::Node,
        dst_y: usize,
        src_page: *mut Page,
        src_row: *const pagepkg::Row,
        x_start: usize,
        x_end: usize,
    ) -> *mut pagelist::Node {
        debug_assert!(
            !core::ptr::eq(src_page, unsafe { pagelist::Node::page(node) }),
            "clone_partial_row_grow_capacity: source page must not be the destination node's \
             page; increase_capacity frees the destination page memory"
        );

        let mut current = node;
        // PORT NOTE: reference defect (R13), UD80: at most one same-capacity
        // re-clone to drop orphaned pinned styles; see
        // `pagelist::grow::styles_hold_pinned`.
        let mut reclaimed_pinned_styles = false;
        // Debug-only guard on that one-shot flag: a regression that re-clones
        // twice fails at once instead of re-cloning forever.
        let mut reclones: u32 = 0;
        // PORT NOTE: reference defect (R13), UD131: the dimensions whose
        // capacity increase returned `OutOfSpace` and are now degraded rather
        // than grown; see `pagelist::grow::degrade_dimension` (and its
        // termination bound).
        let mut degrade = pagepkg::RowDegradeMask::default();
        loop {
            let cur_page: *mut Page = unsafe { pagelist::Node::page(current) };
            let cur_rows = unsafe { (*cur_page).rows.ptr((*cur_page).memory_ptr()) };
            let res = if !degrade.any() {
                unsafe {
                    (*cur_page).clone_partial_row_from(
                        &*src_page,
                        cur_rows.add(dst_y),
                        src_row,
                        x_start,
                        x_end,
                    )
                }
                .map(|()| pagepkg::RowDegrade::default())
            } else {
                unsafe {
                    (*cur_page).clone_partial_row_degrade(
                        src_page,
                        cur_rows.add(dst_y),
                        src_row,
                        x_start,
                        x_end,
                        degrade,
                    )
                }
            };
            let err = match res {
                Ok(rep) => {
                    pagelist::grow::log_row_degrade(&rep);
                    return current;
                }
                Err(err) => err,
            };

            // Adjust our page capacity to make room for what we
            // didn't have space for and retry the copy.
            let adjustment = pagelist::grow::IncreaseCapacity::for_clone_error(err);
            current = match self.increase_capacity(current, adjustment) {
                Ok(n) => n,
                // PORT NOTE: reference defect (R13), UD80. The style set is
                // full at its capacity ceiling, where the reference crashes
                // below. If it holds a pinned style, re-clone once at the same
                // capacity first: the recount drops pinned styles no cell
                // uses. A second failure reaches the reference's crash.
                Err(pagelist::grow::IncreaseCapacityError::OutOfSpace)
                    if adjustment == Some(pagelist::grow::IncreaseCapacity::Styles)
                        && !reclaimed_pinned_styles
                        && pagelist::grow::styles_hold_pinned(cur_page) =>
                {
                    reclaimed_pinned_styles = true;
                    reclones += 1;
                    debug_assert!(
                        reclones <= 1,
                        "UD80 re-clone ran twice: one-shot flag regressed"
                    );
                    match self.increase_capacity(current, None) {
                        Ok(n) => n,
                        Err(_) => panic!("increaseCapacity system allocator OOM"),
                    }
                }
                // A real system-allocator OOM: our callers have already
                // rotated or shifted rows, so returning an error would leave
                // the page list half-mutated (and corrupt); a crash is better.
                //
                // PORT NOTE: reference defect (R13), UD131 — the reference
                // comment (Screen.zig:738-742) covers both arms; the port keeps
                // the crash only for OutOfMemory (OutOfSpace degrades below):
                //   // We can't gracefully recover from either of these
                //   // here: our callers have already rotated or shifted
                //   // rows, so returning an error would leave the page
                //   // list half-mutated (and corrupt), so a crash is
                //   // better.
                Err(pagelist::grow::IncreaseCapacityError::OutOfMemory) => {
                    panic!("increaseCapacity system allocator OOM")
                }
                // PORT NOTE: reference defect (R13), UD131. The reference
                // `@panic("increaseCapacity OutOfSpace")` here (Screen.zig:
                // 747). Reachable from real VT input (IL/DL, scroll, erase
                // across a page boundary) by TWO routes into the style set's
                // OutOfMemory: (1) the PSL-overflow arm, on ANY width including
                // an ordinary 80x24 terminal, with about 1.2 KB of output (32
                // hash-colliding colours drive the set to the ceiling on a page
                // holding ~32 live styles); (2) a wide terminal whose page's
                // style set is full of 53,247 distinct live styles at its
                // capacity ceiling (no orphaned pinned styles to reclaim, or the
                // one re-clone above already dropped them). The rows are already
                // rotated, so instead of crashing the loop degrades THIS
                // dimension only and retries: every other dimension keeps
                // growing, and a cell loses only the part (style, grapheme or
                // hyperlink) that fails in a degraded dimension, keeping its
                // codepoint; the row flags are kept.
                Err(pagelist::grow::IncreaseCapacityError::OutOfSpace) => {
                    pagelist::grow::degrade_dimension(&mut degrade, adjustment);
                    current
                }
            };
        }
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
        // PORT NOTE (oracle a55850c98, `P3-W1 T4`): the column comes from the
        // page the pin actually REACHED, not from the desired screen width. If
        // incomplete reflow left that previous page narrower, the desired width
        // resolved an out-of-bounds column.
        page_pin.x = unsafe { crate::pagelist::Node::cols(page_pin.node) } - 1;
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
                // Our cached row/cell pointers may be invalid (that is often
                // the reason cursorReload is being called), so refresh them
                // from the pin first since cursorChangePin below marks the
                // old cursor row as dirty.
                let old_rac = unsafe { (*self.cursor.page_pin).row_and_cell() };
                self.cursor.page_row = old_rac.row;
                self.cursor.page_cell = old_rac.cell;

                // The cursor style and hyperlink IDs are only valid within the
                // page that the pin points at, so the pin change must go through
                // cursorChangePin, which migrates them when the active top-left
                // is on a different page. Writing the pin directly here would
                // leave the cursor holding IDs that are dead or alias unrelated
                // entries on the new page.
                let pin = self
                    .pages
                    .pin(point::Point::Active(point::Coordinate::default()))
                    .unwrap();
                self.cursor.x = 0; // Must be set before cursorChangePin
                self.cursor.y = 0;
                self.cursor_change_pin(pin);

                // cursorChangePin can trigger a page capacity adjustment which
                // moves the pin again, so we re-read it to derive our point.
                self.pages
                    .point_from_pin(point::Tag::Active, unsafe { *self.cursor.page_pin })
                    .unwrap()
            }
        };

        self.cursor.x = pt.coord().x;
        self.cursor.y = pt.coord().y as size::CellCountInt;
        let page_rac = unsafe { (*self.cursor.page_pin).row_and_cell() };
        self.cursor.page_row = page_rac.row;
        self.cursor.page_cell = page_rac.cell;

        self.assert_integrity();
    }

    /// Scroll the active area and keep the cursor at the bottom of the screen.
    /// This is a very specialized function but it keeps it fast.
    pub fn cursor_down_scroll(&mut self) -> Result<(), OutOfMemory> {
        debug_assert!(self.cursor.y == self.pages.rows - 1);

        // if (comptime build_options.kitty_graphics) {
        //     // Scrolling dirties the images because it updates their placements pins.
        //     self.kitty_images.dirty = true;
        // }
        if crate::options::KITTY_GRAPHICS {
            // Scrolling dirties the images because it updates their placements pins.
            self.kitty_images.dirty = true;
        }

        // If we have no scrollback, then we shift all our rows instead.
        if self.no_scrollback {
            // If we have a single-row screen, we have no rows to shift
            // so our cursor is in the correct place we just have to clear
            // the cells.
            if self.pages.rows == 1 {
                let page: *mut Page =
                    unsafe { crate::pagelist::Node::page((*self.cursor.page_pin).node) };
                let row = self.cursor.page_row;
                let cells = unsafe { (*page).get_cells(row) };
                self.clear_cells(page, row, cells);

                // The row is a fresh blank row now and must not retain
                // metadata (wrap state, semantic prompt) from the
                // discarded content.
                // self.cursor.page_row.reset();
                unsafe { (*self.cursor.page_row).reset() };

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
                // Calculate this before cursorChangePin because that function may
                // adjust the underlying page and invalidate references to its pin.

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
            if std::ptr::eq(unsafe { (*self.cursor.page_pin).node }, new_pin.node) {
                // Scrolling normally stays within one page. Resolve the page once
                // while refreshing the cursor pointers. The cursor already holds
                // live row and cell pointers into this mapping, and grow does not
                // compress pages, so the node must still be resident here.
                self.cursor_mark_dirty();
                let page = unsafe { crate::pagelist::Node::page_assume_resident(new_pin.node) };
                let page_rac =
                    unsafe { (*page).get_row_and_cell(new_pin.x as usize, new_pin.y as usize) };
                unsafe { *self.cursor.page_pin = new_pin };
                self.cursor.page_row = page_rac.row;
                self.cursor.page_cell = page_rac.cell;
            } else {
                // Crossing a page may require migrating the cursor's style and
                // hyperlink references, so retain the general path here.
                self.cursor_change_pin(new_pin);
                let page_rac = unsafe { (*self.cursor.page_pin).row_and_cell() };
                self.cursor.page_row = page_rac.row;
                self.cursor.page_cell = page_rac.cell;
            }

            // Our new row is always dirty
            self.cursor_mark_dirty();

            // Clear the new row so it gets our bg color. We only do this
            // if we have a bg color at all.
            if !matches!(self.cursor.style.bg_color.tag(), style::ColorTag::None) {
                let page: *mut Page =
                    unsafe { crate::pagelist::Node::page((*self.cursor.page_pin).node) };
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
        if let Some(new_node) = self.pages.grow()? {
            self.cursor_scroll_above_rotate(new_node)?;
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
                    unsafe { crate::pagelist::Node::page((*self.cursor.page_pin).node) };

                // Rotate the rows so that the newly created empty row is at the
                // beginning. e.g. [ 0 1 2 3 ] in to [ 3 0 1 2 ].
                let rows = unsafe { (*page).rows.ptr((*page).memory_ptr()) };
                let pin_y = unsafe { (*pin).y } as usize;
                let size_rows = unsafe { (*page).size.rows } as usize;
                // Rotating this suffix changes which logical row its coordinates identify.
                self.pages.invalidate_node_layout(unsafe { (*pin).node });
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
                self.cursor_scroll_above_rotate(core::ptr::null_mut())?;
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

    //
    // PORT NOTE: the reference takes `fresh_node: ?*PageList.List.Node`; the
    // Rust port uses a null `*mut Node` for the `null` case (raw-pointer
    // discipline: no `&mut` into a node the caller also aliases).
    fn cursor_scroll_above_rotate(
        &mut self,
        fresh_node: *mut crate::pagelist::Node,
    ) -> Result<(), OutOfMemory> {
        // A fresh node given to us is always the newly allocated tail of
        // the page list (see grow), which is where our iteration starts.
        debug_assert!(fresh_node.is_null() || std::ptr::eq(fresh_node, self.pages.pages.last));

        let down = unsafe { (*self.cursor.page_pin).down(1) }.unwrap();
        self.cursor_change_pin(down);

        // Go through each of the pages following our pin, shift all rows
        // down by one, and copy the last row of the previous page. We start
        // at the tail, which is the only node that can be freshly allocated.
        let mut current = self.pages.pages.last;
        assert!(!current.is_null());
        let mut current_is_fresh = !fresh_node.is_null();
        while !std::ptr::eq(current, unsafe { (*self.cursor.page_pin).node }) {
            let prev = unsafe { (*current).prev };

            // A newly allocated tail has no earlier references to invalidate.
            if !current_is_fresh {
                // Rotating this page moves every cached row coordinate down by one.
                self.pages.invalidate_node_layout(current);
            }

            // Rotate the pages down: [ 0 1 2 3 ] => [ 3 0 1 2 ]
            {
                let cur_page: *mut Page = unsafe { crate::pagelist::Node::page(current) };
                let cur_rows = unsafe { (*cur_page).rows.ptr((*cur_page).memory_ptr()) };
                let cur_size_rows = unsafe { (*cur_page).size.rows } as usize;
                fastmem::rotateOnceR(unsafe {
                    core::slice::from_raw_parts_mut(cur_rows, cur_size_rows)
                });
            }

            // Copy the last row of the previous page to the top of current.
            // If the current page doesn't have enough capacity for the
            // managed memory of the copied row (styles, hyperlinks, etc.)
            // then its capacity is increased and the copy retried, which
            // may replace `current` in the page list. Our loop condition
            // guarantees `current` is never the cursor page, and `prev` is
            // unaffected by the replacement.
            {
                let prev_page: *mut Page = unsafe { crate::pagelist::Node::page(prev) };
                let prev_rows = unsafe { (*prev_page).rows.ptr((*prev_page).memory_ptr()) };
                let prev_size_rows = unsafe { (*prev_page).size.rows } as usize;
                let cols = self.pages.cols as usize;
                current = self.clone_partial_row_grow_capacity(
                    current,
                    0,
                    prev_page,
                    unsafe { prev_rows.add(prev_size_rows - 1) },
                    0,
                    cols,
                );
            }

            // Mark dirty on the page, since we are dirtying all rows with this.
            unsafe { (*crate::pagelist::Node::page(current)).dirty = true };

            current = prev;
            current_is_fresh = false;
        }

        // Our current is our cursor page, we need to rotate down from
        // our cursor and clear our row.
        debug_assert!(std::ptr::eq(current, unsafe {
            (*self.cursor.page_pin).node
        }));
        let cur_page: *mut Page = unsafe { crate::pagelist::Node::page(current) };
        let cur_rows = unsafe { (*cur_page).rows.ptr((*cur_page).memory_ptr()) };
        let pin_y = unsafe { (*self.cursor.page_pin).y } as usize;
        let cur_size_rows = unsafe { (*cur_page).size.rows } as usize;
        // Rotating the cursor-page suffix changes its cached row coordinates.
        self.pages.invalidate_node_layout(current);
        fastmem::rotateOnceR(unsafe {
            core::slice::from_raw_parts_mut(cur_rows.add(pin_y), cur_size_rows - pin_y)
        });
        let row = unsafe { cur_rows.add(pin_y) };
        let cells = unsafe { (*cur_page).get_cells(row) };
        self.clear_cells(cur_page, row, cells);

        // The recycled storage becomes the new blank cursor row and must
        // not retain metadata (wrap state, semantic prompt) from the row
        // whose content was moved to the next page.
        // cur_rows[self.cursor.page_pin.y].reset();
        unsafe { (*row).reset() };

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

    /// Scroll a full-width scroll region that ends at the cursor row up by
    /// one row. The cursor must be on the bottom row of the region. `limit`
    /// is the number of rows in the region above the cursor (region height
    /// minus one) and must be at least 1.
    ///
    /// The top row of the region is discarded (NOT moved into scrollback).
    /// All other rows in the region shift up by one and the cursor row
    /// becomes a blank row, filled with the current background color (like
    /// other scroll operations such as deleteLines).
    ///
    /// The cursor stays at the same screen position (the new blank row).
    /// Content outside of the region is unmodified.
    ///
    /// This is a very hot path for scroll region usage (e.g. a program
    /// on the alt screen using DECSTBM and scrolling via LF/IND) so this
    /// is optimized for the common case where the full region is within
    /// a single page.
    //
    // pub fn cursorScrollRegionUp(self: *Screen, limit: usize) !void
    //
    // PORTS: oracle 77190bd02 (`Screen.cursorScrollRegionUp`) + the
    // `invalidateNodeLayout` hunk from 0bad91adb (Phase 3 re-vet row P3-RV3).
    //
    // PORT NOTE (UB HAZARD, `P3-W5 T3`): the fast path holds a `*mut Page`
    // while it mutates tracked pins and the cursor's cached row/cell pointers,
    // and `assert_integrity()` re-walks the same memory afterwards. Every
    // access here therefore goes through RAW POINTERS derived from the page's
    // memory base; the only `&mut` ever formed is the `&mut [Row]` handed to
    // `fastmem::rotateOnce`, created from the raw base inside the narrowest
    // possible scope and dropped before the next raw access. In particular the
    // cursor's new `page_row`/`page_cell` are recomputed from the RAW base,
    // never from that temporary slice, so no pointer survives the borrow that
    // created it. Never form `&mut Page` (that is the mutation the Miri
    // harness `.port/phase-3/mirihar-scroll-region-up` proves fails).
    pub fn cursor_scroll_region_up(&mut self, limit: usize) -> Result<(), OutOfMemory> {
        // assert(limit >= 1);
        debug_assert!(limit >= 1);
        // assert(self.cursor.y >= limit);
        debug_assert!(self.cursor.y as usize >= limit);
        // defer self.assertIntegrity();
        //
        // PORT NOTE: Zig's `defer` runs on ERROR returns too, so the body lives
        // in an inner fn and the integrity gate runs unconditionally after it,
        // on every exit path including the slow path's `?` OOM early-return.
        // (Same shape as `PageList::erase_row_bounded`.)
        let result = self.cursor_scroll_region_up_inner(limit);
        self.assert_integrity();
        result
    }

    /// The body of `cursor_scroll_region_up` (the reference fn minus its
    /// `defer`); see the PORT NOTE above.
    fn cursor_scroll_region_up_inner(&mut self, limit: usize) -> Result<(), OutOfMemory> {
        // const pin: *Pin = self.cursor.page_pin;
        let pin: *mut Pin = self.cursor.page_pin;
        let pin_y = unsafe { (*pin).y };
        let pin_node = unsafe { (*pin).node };

        // If the region crosses a page boundary we take a slower path. This
        // is rare: it requires the active area to span multiple pages with
        // the split point inside the scroll region.
        // if (pin.y < limit) return try self.cursorScrollRegionUpSlow(limit);
        if (pin_y as usize) < limit {
            return self.cursor_scroll_region_up_slow(limit);
        }

        // Fast path: the entire region is in a single page. We can clear
        // the top row and rotate it down to the cursor row, updating any
        // tracked pins along the way.
        //
        // The cursor's cached row and cell pointers refer into this mapping.
        // PageList cannot compress the cursor page while those pointers are
        // installed, so the node is known to be resident here.
        // const page: *Page = pin.node.pageAssumeResident();
        let page: *mut Page = unsafe { pagelist::Node::page_assume_resident(pin_node) };
        // const rows = page.rows.ptr(page.memory.ptr)[pin.y - limit ..][0 .. limit + 1];
        //
        // PORT NOTE: the reference's `rows` is a `[]Row` slice used for the
        // clear, the rotate AND the final cursor-row pointer. In Rust it is a
        // raw base pointer for exactly the aliasing reason in the fn's PORT
        // NOTE; the slice is materialized only for `rotateOnce` below.
        let rows: *mut Row = unsafe {
            (*page)
                .rows
                .ptr((*page).memory_ptr())
                .add(pin_y as usize - limit)
        };

        // Clear the erased (top) row.
        {
            // const row: *Row = &rows[0];
            let row: *mut Row = rows;

            // Whether our blank cell is a plain zero cell. This is true
            // unless the cursor has a background color set (see blankCell).
            // const blank_is_zero = self.cursor.style_id == style.default_id or
            //     self.cursor.style.bg_color == .none;
            let blank_is_zero = self.cursor.style_id == style::DEFAULT_ID
                || matches!(self.cursor.style.bg_color.tag(), style::ColorTag::None);

            // if (!row.managedMemory() and blank_is_zero)
            if !unsafe { *row }.managed_memory() && blank_is_zero {
                // Hot path: the row has no managed memory (styles,
                // graphemes, hyperlinks) and our blank is zero so this is
                // a straight zero fill. This is the overwhelmingly common
                // case for scroll region usage.
                // const cells = page.getCells(row);
                // @memset(@as([]u64, @ptrCast(cells)), 0);
                //
                // PORT NOTE: the reference re-casts the cell slice to `[]u64`
                // to memset the backing integers; `Cell` is
                // `#[repr(transparent)] struct Cell(u64)` here, so a byte-wise
                // zero of the same extent is the identical write.
                let cells = unsafe { (*page).get_cells(row) };
                let len = cells.len();
                unsafe {
                    core::ptr::write_bytes(cells as *mut Cell, 0u8, len);
                }
            } else {
                // The generic clear handles managed memory and fills the
                // row with our blank cell, preserving the background color.
                // self.clearCells(page, row, page.getCells(row));
                let cells = unsafe { (*page).get_cells(row) };
                self.clear_cells(page, row, cells);
            }

            // The row becomes the new blank cursor row after the rotation
            // below and must not retain metadata (wrap state, semantic
            // prompt) from the discarded content.
            // row.reset();
            unsafe { (*row).reset() };
        }

        // Rotate the region rows so the now-blank top row moves to the
        // bottom (the cursor row) and everything else shifts up by one.
        // Rotating the region changes which logical row its coordinates identify.
        // self.pages.invalidateNodeLayout(pin.node);
        // fastmem.rotateOnce(Row, rows);
        self.pages.invalidate_node_layout(pin_node);
        fastmem::rotateOnce(unsafe { core::slice::from_raw_parts_mut(rows, limit + 1) });

        // Mark the whole page as dirty.
        //
        // Technically we only need to mark the rotated rows but this is
        // a hot function, so we want to minimize work.
        // page.dirty = true;
        unsafe { (*page).dirty = true };

        // If our viewport is a pin and it's within the rotated region
        // then we need to shift its cached row offset up. See
        // PageList.eraseRowBounded for details; this mirrors that logic.
        // if (self.pages.viewport == .pin) viewport: { ... }
        if self.pages.viewport == pagelist::Viewport::Pin {
            'viewport: {
                if let Some(v) = self.pages.viewport_pin_row_offset.as_mut() {
                    let p = self.pages.viewport_pin;
                    if !core::ptr::eq(unsafe { (*p).node }, pin_node)
                        || unsafe { (*p).y } < pin_y - limit as size::CellCountInt
                        || unsafe { (*p).y } > pin_y
                        || unsafe { (*p).y } == 0
                    {
                        break 'viewport;
                    }
                    *v -= 1;
                }
            }
        }

        // Update tracked pins within the region since their rows moved up
        // by one. The cursor's own pin is skipped because the cursor stays
        // at the region bottom (the new blank row).
        // const pin_keys = self.pages.tracked_pins.keys();
        // for (pin_keys) |p| { ... }
        let pin_keys = self.pages.tracked_pins.keys();
        for p in pin_keys {
            let p: *mut Pin = *p;
            if !core::ptr::eq(unsafe { (*p).node }, pin_node)
                || core::ptr::eq(p, pin)
                || unsafe { (*p).y } < pin_y - limit as size::CellCountInt
                || unsafe { (*p).y } > pin_y
            {
                continue;
            }
            if unsafe { (*p).y } == 0 {
                unsafe { (*p).x = 0 };
            } else {
                unsafe { (*p).y -= 1 };
            }
        }

        // The cursor pin is unchanged, but the Row structure at the pin
        // position now contains the blank row, so we need to refresh our
        // cached row/cell pointers. We compute them directly from the row
        // slice we already have rather than going through the pin since
        // this is a hot path.
        // const cursor_row: *Row = &rows[limit];
        // self.cursor.page_row = cursor_row;
        // self.cursor.page_cell = &page.getCells(cursor_row)[self.cursor.x];
        let cursor_row: *mut Row = unsafe { rows.add(limit) };
        self.cursor.page_row = cursor_row;
        self.cursor.page_cell =
            unsafe { ((*page).get_cells(cursor_row) as *mut Cell).add(self.cursor.x as usize) };

        Ok(())
    }

    /// Slow path for cursorScrollRegionUp: the scroll region spans
    /// multiple pages so we use the generic PageList erase machinery.
    // fn cursorScrollRegionUpSlow(self: *Screen, limit: usize) !void
    fn cursor_scroll_region_up_slow(&mut self, limit: usize) -> Result<(), OutOfMemory> {
        // The call to eraseRowBounded below will move our tracked cursor
        // pin up by one row since it is inside the erased region, but we
        // don't actually want that: the cursor stays put, on the new blank
        // row at the region bottom. We keep the old pin and put it back
        // after, exactly like cursorDownScroll does for the no-scrollback
        // case.
        //
        // This matters beyond performance: when the cursor is on the first
        // row of a page, eraseRowBounded moves the tracked pin to the
        // previous page. Moving the cursor back down with cursorDown would
        // then cross pages via cursorChangePin, which migrates the cursor
        // style refcount from a page that never held it, corrupting the
        // style accounting.
        // const old_pin = self.cursor.page_pin.*;
        let old_pin = unsafe { *self.cursor.page_pin };

        // try self.pages.eraseRowBounded(
        //     .{ .active = .{ .y = @intCast(self.cursor.y - limit) } },
        //     limit,
        // );
        let y = self.cursor.y as usize - limit;
        self.pages.erase_row_bounded(
            point::Point::Active(point::Coordinate { x: 0, y: y as u32 }),
            limit,
        )?;

        // We don't use `cursorChangePin` here because we aren't actually
        // changing the pin, we're keeping it the same. Since the page
        // never changes, the cursor's style ref stays valid and no style
        // accounting needs to be updated.
        // self.cursor.page_pin.* = old_pin;
        unsafe { *self.cursor.page_pin = old_pin };

        // We do, however, need to refresh the cached page row and cell,
        // because the row contents at our pin position changed (it now
        // contains the blank row).
        let page_rac = unsafe { (*self.cursor.page_pin).row_and_cell() };
        self.cursor.page_row = page_rac.row;
        self.cursor.page_cell = page_rac.cell;

        // eraseRowBounded clears the new row with zero cells so if our
        // blank cell isn't zero (bg color is set) we need to fill it.
        // const blank = self.blankCell();
        // if (!blank.isZero()) {
        //     const cells: [*]Cell = @ptrCast(self.cursor.page_cell);
        //     @memset((cells - self.cursor.x)[0..self.pages.cols], blank);
        // }
        let blank = self.blank_cell();
        if !blank.is_zero() {
            let cells = unsafe { self.cursor.page_cell.sub(self.cursor.x as usize) };
            unsafe { core::slice::from_raw_parts_mut(cells, self.pages.cols as usize).fill(blank) };
        }

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
                unsafe { crate::pagelist::Node::page((*self.cursor.page_pin).node) };
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
                unsafe { crate::pagelist::Node::page((*self.cursor.page_pin).node) };
            unsafe {
                let base = (*old_page).memory_ptr();
                (*old_page)
                    .hyperlink_set
                    .release(base, self.cursor.hyperlink_id);
            }
            // Zero the ID, it is invalid now and style changes below may
            // run integrity checks. We still have self.cursor.hyperlink to
            // rebuild this later.
            self.cursor.hyperlink_id = 0;
        }

        // Update our pin to the new page
        unsafe { *self.cursor.page_pin = new };

        // On the new page, we need to migrate our style
        if let Some(old_style) = old_style_ {
            self.cursor.style = old_style;
            if let Err(err) = self.manual_style_update() {
                // This failure should not happen because manualStyleUpdate
                // handles page splitting, overflow, and more. This should only
                // happen if we're out of RAM. In this case, we'll just degrade
                // gracefully back to the default style.
                log::error!(target: LOG_SCOPE, "failed to update style on cursor change err={err:?}");
                self.cursor.style = style::Style::default();
                self.cursor.style_id = 0;
            }
        }

        // On the new page, we need to migrate our hyperlink
        if let Some(link) = self.cursor.hyperlink {
            // startHyperlink will try to free old hyperlinks, so set this
            // to null. We free it ourselves later since we're doing some
            // ref-counting shenanigans in this function.
            self.cursor.hyperlink = None;

            // Re-add
            // PORT NOTE: the reference passes the old link's slices straight
            // through. `link` is detached from `self.cursor` here, so borrowing
            // its bytes does not alias the state `start_hyperlink` releases, and
            // no copy is made.
            let uri: &[u8] = unsafe { &(*link).uri };
            let id_: Option<&[u8]> = match unsafe { &(*link).id } {
                hyperlink::HyperlinkId::Explicit(v) => Some(v),
                hyperlink::HyperlinkId::Implicit(_) => None,
            };
            if let Err(err) = self.start_hyperlink(uri, id_) {
                // This shouldn't happen because startHyperlink should handle
                // resizing. This only happens if we're truly out of RAM. Degrade
                // to forgetting the hyperlink.
                log::error!(target: LOG_SCOPE, "failed to update hyperlink on cursor change err={err:?}");
            }

            // Remove our old link
            // link.deinit(self.alloc);
            // self.alloc.destroy(link);
            unsafe { destroy_cursor_hyperlink(link) };
        }
    }

    /// Copy another cursor. The cursor can be on any screen but the x/y
    /// must be within our screen bounds.
    pub fn cursor_copy(
        &mut self,
        other: Cursor,
        opts: CursorCopyOptions,
    ) -> Result<(), pagelist::grow::IncreaseCapacityError> {
        debug_assert!(other.x < self.pages.cols);
        debug_assert!(other.y < self.pages.rows);

        // End any currently active hyperlink on our cursor.
        self.end_hyperlink();

        // PORT NOTE: the reference does `const old = self.cursor; self.cursor =
        // other; errdefer self.cursor = old;`. `other` is taken by value
        // (`Cursor` is a plain value type here). We capture the fields of the old
        // cursor we still need (`style_id`, `page_pin`, `x`, `y`) before
        // overwriting.
        //
        // PORT NOTE (parity S133-F3): an earlier version of this note
        // claimed `Cursor` is "not trivially Copy" and that the `errdefer` revert
        // is "not reachable". Both are wrong. `Cursor` IS `#[derive(Copy)]` (its
        // `hyperlink` is a raw pointer whose box ownership is tracked
        // explicitly), and `manual_style_update` returns
        // `pagelist::grow::IncreaseCapacityError`, which has BOTH an `OutOfMemory`
        // and an `OutOfSpace` variant — so it really can fail, and on failure the
        // reference's `errdefer self.cursor = old` puts the WHOLE old cursor
        // back. Without it `self.cursor` was left half-applied: `other`'s
        // style/pending_wrap/protected/semantic_content over the old
        // style_id/pin/x/y. The error was also flattened to `OutOfMemory`,
        // losing `OutOfSpace`.
        let old = self.cursor;
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
        // try self.manualStyleUpdate();
        #[cfg(test)]
        let forced = cursor_copy_tw_check(CursorCopyFp::ManualStyleUpdate);
        #[cfg(not(test))]
        let forced: Result<(), pagelist::grow::IncreaseCapacityError> = Ok(());
        if let Err(err) = forced.and_then(|()| self.manual_style_update()) {
            // errdefer self.cursor = old;
            self.cursor = old;
            return Err(err);
        }

        // Move to the correct location to match the other cursor.
        self.cursor_absolute(other_x, other_y);

        // If the other cursor had a hyperlink, add it to ours.
        if opts.hyperlink && other_hyperlink_id != 0 {
            // Get the hyperlink from the other cursor's page.
            let other_page: *mut Page =
                unsafe { crate::pagelist::Node::page((*other_page_pin).node) };
            let other_link = unsafe {
                let base = (*other_page).memory_ptr();
                (*other_page).hyperlink_set.get(base, other_hyperlink_id)
            };

            let other_base = unsafe { (*other_page).memory_ptr() };
            let uri = unsafe { (*other_link).uri.slice(other_base) }.to_vec();
            let other_id = unsafe { (*other_link).id };
            let id_: Option<Vec<u8>> = match other_id.tag() {
                hyperlink::PageEntryIdTag::Explicit => {
                    Some(unsafe { other_id.explicit().slice(other_base) }.to_vec())
                }
                hyperlink::PageEntryIdTag::Implicit => None,
            };

            // And it to our cursor.
            if let Err(err) = self.start_hyperlink(&uri, id_.as_deref()) {
                // This shouldn't happen because startHyperlink should handle
                // resizing. This only happens if we're truly out of RAM. Degrade
                // to forgetting the hyperlink.
                log::error!(target: LOG_SCOPE, "failed to update hyperlink on cursor change err={err:?}");
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
        let cols = unsafe { crate::pagelist::Node::cols((*self.cursor.page_pin).node) } as usize;
        let cell = unsafe { *(cells as *mut pagepkg::Cell).add(cols - 1) };
        if cell.wide() == pagepkg::Wide::SpacerHead {
            let page: *mut Page =
                unsafe { crate::pagelist::Node::page((*self.cursor.page_pin).node) };
            let last = unsafe { (cells as *mut pagepkg::Cell).add(cols - 1) };
            let slice = core::ptr::slice_from_raw_parts_mut(last, 1);
            self.clear_cells(page, page_row, slice);
        }
    }

    /// Scroll the viewport of the terminal grid.
    pub fn scroll(&mut self, behavior: Scroll) {
        // if (comptime build_options.kitty_graphics) {
        //     // No matter what, scrolling marks our image state as dirty since
        //     // it could move placements. If there are no placements or no images
        //     // this is still a very cheap operation.
        //     self.kitty_images.dirty = true;
        // }
        if crate::options::KITTY_GRAPHICS {
            // No matter what, scrolling marks our image state as dirty since
            // it could move placements. If there are no placements or no images
            // this is still a very cheap operation.
            self.kitty_images.dirty = true;
        }

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

        // if (comptime build_options.kitty_graphics) {
        //     // No matter what, scrolling marks our image state as dirty since
        //     // it could move placements. If there are no placements or no images
        //     // this is still a very cheap operation.
        //     self.kitty_images.dirty = true;
        // }
        if crate::options::KITTY_GRAPHICS {
            // No matter what, scrolling marks our image state as dirty since
            // it could move placements. If there are no placements or no images
            // this is still a very cheap operation.
            self.kitty_images.dirty = true;
        }

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

        // PORT NOTE (oracle e727a3658, `P3-W1 T4`): the desired `self.pages.cols`
        // is NOT used to slice the backing cells any more. During incomplete
        // reflow a stored page can be narrower than the desired width, and
        // slicing to the desired width ran off the end of the row and tripped
        // `clear_cells`' own bounds `debug_assert!`s before a single cell was
        // cleared. The cells come from the OWNING page instead.
        let mut it = self
            .pages
            .page_iterator(pagelist::iter::Direction::RightDown, tl, bl);
        while let Some(chunk) = it.next() {
            // PORT NOTE: `chunk.node` is the `*mut Node`; `&chunk.node.data` is
            // the page. We take a raw `*mut Page` via `addr_of_mut!` (the page
            // lives in the node) and read `chunk.rows()` (a `*mut [Row]`).
            let page: *mut Page = unsafe { crate::pagelist::Node::page(chunk.node) };
            let rows = chunk.rows();
            let rows_len = rows.len();
            let rows_base = rows as *mut Row;
            for i in 0..rows_len {
                let row: *mut Row = unsafe { rows_base.add(i) };

                let cells_offset = unsafe { (*row).cells() };
                // const cells = page.getCells(row);
                //
                // Kept in the existing raw-pointer style: `get_cells` returns a
                // `*mut [Cell]` and no `&mut [Cell]` is formed while the
                // `*mut Page` alias is live (see the `clear_cells` PORT NOTE).
                let cells: *mut [Cell] = unsafe { (*page).get_cells(row) };

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
        // if (cells.len == 0) return;
        // PORT NOTE (parity 0cb004734): the runtime-safety validation below
        // indexes the first and last elements unconditionally; an empty range
        // is a valid no-op clear.
        if cells.len() == 0 {
            return;
        }

        let cells_base = cells as *mut Cell;
        let cells_len = cells.len();

        // This whole operation does unsafe things, so we just want to assert
        // the end state.
        unsafe { (*page).pause_integrity_checks(true) };

        // PORT NOTE: the reference uses a `defer` block to resume integrity
        // checks + assert at every exit; with a single exit path we run it at
        // the end. (No early returns in this body after the empty-range
        // guard, which returns before checks are paused.)

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
            if cells_len == unsafe { (*page).size.cols } as usize {
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
            if cells_len == unsafe { (*page).size.cols } as usize {
                unsafe { (*row).set_hyperlink(false) };
            } else {
                unsafe { (*page).update_row_hyperlink_flag(row) };
            }
        }

        if unsafe { (*row).styled() } {
            // Styled cells overwhelmingly come in runs sharing the same
            // style (e.g. a colored status bar or a highlighted region),
            // so group them and release each run with a single ref-count
            // update rather than per cell.
            //
            // PORT NOTE (oracle 8d663a76e, `P3-W1 T6`). UB HAZARD: the style
            // ids are read through the raw `*mut Cell` (`cells_base.add(..)`),
            // exactly as the previous per-cell loop did, because
            // `release_multiple` mutates page memory through `*mut Page`. A
            // live `&mut [Cell]` across that call would be Stacked-Borrows UB
            // (memory: rust-mut-args-strongly-protected).
            //
            // `release_multiple`'s own `ref_count >= n` assertion holds by
            // construction: every cell in the run held a reference.
            let mut i: usize = 0;
            while i < cells_len {
                let id = unsafe { (*cells_base.add(i)).style_id() };
                if id == style::DEFAULT_ID {
                    i += 1;
                    continue;
                }
                let mut j = i + 1;
                while j < cells_len && unsafe { (*cells_base.add(j)).style_id() } == id {
                    j += 1;
                }
                unsafe {
                    let base = (*page).memory_ptr();
                    (*page)
                        .styles
                        .release_multiple(base, id, (j - i) as style::Id);
                }
                i = j;
            }

            // If we have no left/right scroll region we can be sure
            // that we've cleared all the styles, so we clear the
            // flag, otherwise we ask the page to update the flag.
            if cells_len == unsafe { (*page).size.cols } as usize {
                unsafe { (*row).set_styled(false) };
            } else {
                unsafe { (*page).update_row_styled_flag(row) };
            }
        }

        // if (comptime build_options.kitty_graphics) {
        //     if (row.kitty_virtual_placeholder and
        //         cells.len == page.size.cols)
        //     {
        //         for (cells) |c| {
        //             if (c.codepoint() == kitty.graphics.unicode.placeholder) {
        //                 break;
        //             }
        //         } else row.kitty_virtual_placeholder = false;
        //     }
        // }
        //
        // PORT NOTE: the Zig `for ... else` clears the flag only when the loop
        // ran to completion, i.e. when NO surviving cell still carries the
        // placeholder codepoint. The cells are read through the raw
        // `cells_base` for the same Stacked-Borrows reason as the style loop
        // above.
        if crate::options::KITTY_GRAPHICS
            && unsafe { (*row).kitty_virtual_placeholder() }
            && cells_len == unsafe { (*page).size.cols } as usize
        {
            let mut found = false;
            for i in 0..cells_len {
                if unsafe { (*cells_base.add(i)).codepoint() }
                    == crate::kitty::graphics_unicode::PLACEHOLDER
                {
                    found = true;
                    break;
                }
            }
            if !found {
                unsafe { (*row).set_kitty_virtual_placeholder(false) };
            }
        }

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
        let page: *mut Page = unsafe { crate::pagelist::Node::page((*self.cursor.page_pin).node) };

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
        let cols = unsafe { crate::pagelist::Node::cols((*cursor_page_pin).node) };

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
                    let p_cols = unsafe { crate::pagelist::Node::cols(p_row.node) };
                    let p_cell = unsafe { *p_cells_base.add(p_cols as usize - 1) };
                    if p_cell.wide() == pagepkg::Wide::SpacerHead {
                        let p_page: *mut Page = unsafe { crate::pagelist::Node::page(p_row.node) };
                        let slice = core::ptr::slice_from_raw_parts_mut(
                            unsafe { p_cells_base.add(p_cols as usize - 1) },
                            1,
                        );
                        self.clear_cells(p_page, p_rac.row, slice);

                        // `clearCells` does not mark rows dirty, and our
                        // callers only mark the cursor row, so mark the
                        // previous row here.
                        p_row.mark_dirty();
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

    /// Resize the screen. The rows or cols can be bigger or smaller. If this
    /// returns an error, the screen is unchanged.
    // PERF(port): `#[inline]` — the reference is `pub inline fn resize`; the
    // extra call frame was a surplus fixed cost on the 5M-call rows benchmark.
    #[inline]
    pub fn resize(&mut self, opts: Resize) -> Result<(), OutOfMemory> {
        // defer self.assertIntegrity();
        // PORT NOTE: Rust has no `defer`. The reference runs `assertIntegrity`
        // at every exit; we wrap the body in a helper and run the assert after.
        let result = self.resize_inner(opts);
        self.assert_integrity();
        result
    }

    fn resize_inner(&mut self, opts: Resize) -> Result<(), OutOfMemory> {
        // We need to insert a tracked pin for our saved cursor so we can
        // modify its X/Y for reflow. Do this before changing any state since
        // tracking the pin can fail.
        let saved_cursor_pin: Option<*mut Pin> = 'saved_cursor: {
            let Some(sc) = self.saved_cursor.as_ref() else {
                break 'saved_cursor None;
            };
            let (sc_x, sc_y) = (sc.x, sc.y as u32);
            let Some(pin) = self
                .pages
                .pin(point::Point::Active(point::Coordinate { x: sc_x, y: sc_y }))
            else {
                break 'saved_cursor None;
            };
            #[cfg(test)]
            resize_tw_check(ResizeFp::SavedCursorPin)?;
            Some(self.pages.track_pin(pin)?)
        };
        // PORT NOTE: the reference `defer if (saved_cursor_pin) |p|
        // self.pages.untrackPin(p);` is run at every exit below via
        // `resize_tracked`.
        let result = self.resize_tracked(opts, saved_cursor_pin);
        if let Some(p) = saved_cursor_pin {
            self.pages.untrack_pin(p);
        }
        result
    }

    /// The remainder of `resize` once the saved-cursor pin is tracked.
    //
    // PORT NOTE (UB HAZARD, area A §5(a)): `PageList::resize` reallocates and
    // can destroy pages, so nothing here may hold a `&mut Page` (or any
    // reference derived from one) across it. Every page touch below goes
    // through a freshly-materialized `*mut Page`, and the only value that
    // outlives the resize is the raw `cursor_node` POINTER, which is compared
    // but never dereferenced afterwards (see the survival check at the bottom).
    fn resize_tracked(
        &mut self,
        opts: Resize,
        saved_cursor_pin: Option<*mut Pin>,
    ) -> Result<(), OutOfMemory> {
        // A cursor style and hyperlink are partly stored in the page containing
        // the cursor. Their IDs only have meaning within that page, and the page
        // keeps reference counts for them. Resizing can replace or destroy the
        // page, so below we temporarily remove both values from the cursor before
        // asking PageList to resize.
        //
        // Save everything first so we can put the cursor back together afterward,
        // or restore it unchanged if the resize fails. We also save the original
        // node and serial so after a successful resize we can tell whether that
        // page survived and still needs its temporary references released.
        let cursor_node = unsafe { (*self.cursor.page_pin).node };
        let cursor_node_serial = unsafe { (*cursor_node).serial };
        let cursor_style = self.cursor.style;
        let cursor_style_id = self.cursor.style_id;
        let cursor_hyperlink = self.cursor.hyperlink;
        let cursor_hyperlink_id = self.cursor.hyperlink_id;

        // Keep an extra reference to the cursor style and hyperlink while the
        // resize is in progress. Releasing the cursor references below therefore
        // can't delete either entry, and an error can restore the cursor without
        // any allocation.
        {
            let page: *mut Page = unsafe { crate::pagelist::Node::page(cursor_node) };
            unsafe {
                let base = (*page).memory_ptr();
                if cursor_style_id != style::DEFAULT_ID {
                    (*page).styles.use_id(base, cursor_style_id);
                }
                if cursor_hyperlink_id != 0 {
                    (*page).hyperlink_set.use_id(base, cursor_hyperlink_id);
                }
            }
        }
        // errdefer {
        //     self.cursor.style = cursor_style;
        //     self.cursor.style_id = cursor_style_id;
        //     self.cursor.hyperlink = cursor_hyperlink;
        //     self.cursor.hyperlink_id = cursor_hyperlink_id;
        // }
        //
        // PORT NOTE: Rust has no `errdefer`; the block above is spelled out as
        // `resize_restore_cursor` and invoked on each of the two fallible exits
        // below. It performs NO allocation, which is the point of the extra
        // references taken just above.

        // Release the cursor style while resizing just in case the cursor ends
        // up on a different page.
        self.cursor.style = style::Style::default();
        self.manual_style_update()
            .expect("manualStyleUpdate to default cannot fail");

        // If we have a hyperlink, release it from the old page
        // and then we need to re-add it to the new page. This needs
        // to happen because resize below typically reallocates a
        // new page so the old hyperlink is invalid.
        if self.cursor.hyperlink_id != 0 {
            // Note we do NOT use endHyperlink because we want to keep
            // our allocated self.cursor.hyperlink valid.
            let page: *mut Page =
                unsafe { crate::pagelist::Node::page((*self.cursor.page_pin).node) };
            unsafe {
                let base = (*page).memory_ptr();
                (*page)
                    .hyperlink_set
                    .release(base, self.cursor.hyperlink_id);
            }
            self.cursor.hyperlink_id = 0;
            self.cursor.hyperlink = None;
        }

        // Perform the resize operation.
        // try tw.check(.pages);
        #[cfg(test)]
        if let Err(e) = resize_tw_check(ResizeFp::Pages) {
            self.resize_restore_cursor(
                cursor_style,
                cursor_style_id,
                cursor_hyperlink,
                cursor_hyperlink_id,
            );
            return Err(e);
        }
        if let Err(e) = self.pages.resize(pagelist::resize::Resize {
            rows: Some(opts.rows),
            cols: Some(opts.cols),
            reflow: opts.reflow,
            cursor: Some(pagelist::resize::Cursor {
                x: self.cursor.x,
                y: self.cursor.y,
                pin: Some(self.cursor.page_pin),
            }),
        }) {
            self.resize_restore_cursor(
                cursor_style,
                cursor_style_id,
                cursor_hyperlink,
                cursor_hyperlink_id,
            );
            return Err(e);
        }

        // errdefer comptime unreachable;
        //
        // PORT NOTE: "No more failures are possible after this. Enforced by
        // compiler because we do NO cleanup of the state below." Zig's
        // `errdefer comptime unreachable` is a COMPILE-TIME assertion that no
        // fallible operation follows. Rust has no equivalent construct; the
        // property is held by construction — every call below either returns
        // `()` or swallows its error the way the reference does — and the
        // function's single remaining `return` is the `Ok(())` at the end.

        // Resizing moves image placements, so mark their geometry as dirty.
        if crate::options::KITTY_GRAPHICS {
            self.kitty_images.dirty = true;
        }

        // If we have no scrollback and we shrunk our rows, we must explicitly
        // erase our history. This is because PageList always keeps at least
        // a page size of history.
        if self.no_scrollback {
            self.pages.erase_history(None);
        }

        // If our cursor was updated, we do a full reload so all our cursor
        // state is correct.
        self.cursor_reload();

        // Clear any redrawable prompt after the fallible resize but before
        // restoring the cursor style and hyperlink, so cleared cells retain the
        // same default styling they had with the previous ordering.
        self.clear_prompt_for_redraw(opts.prompt_redraw);

        // Restore the cursor style.
        self.cursor.style = cursor_style;
        if let Err(err) = self.manual_style_update() {
            // This failure should not happen because manualStyleUpdate handles
            // page splitting, overflow, and more. This should only happen if
            // we're out of RAM. In this case, we'll just degrade gracefully back
            // to the default style.
            log::error!(target: LOG_SCOPE, "failed to update style on cursor reload err={err:?}");
            self.cursor.style = style::Style::default();
            self.cursor.style_id = 0;
        }

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
        if let Some(link) = cursor_hyperlink {
            // PORT NOTE: the reference passes the old link's slices straight
            // through. `link` is detached from `self.cursor` here, so borrowing
            // its bytes does not alias the state `start_hyperlink` releases, and
            // no copy is made.
            let uri: &[u8] = unsafe { &(*link).uri };
            let id_: Option<&[u8]> = match unsafe { &(*link).id } {
                hyperlink::HyperlinkId::Explicit(v) => Some(v),
                hyperlink::HyperlinkId::Implicit(_) => None,
            };
            if let Err(err) = self.start_hyperlink(uri, id_) {
                // This shouldn't happen because startHyperlink should handle
                // resizing. This only happens if we're truly out of RAM. Degrade
                // to forgetting the hyperlink.
                log::error!(target: LOG_SCOPE, "failed to update hyperlink on resize err={err:?}");
            }

            // Remove our old link
            // link.deinit(self.alloc);
            // self.alloc.destroy(link);
            unsafe { destroy_cursor_hyperlink(link) };
        }

        // A tracked pin follows its content when PageList moves or replaces a
        // page. If the cursor's pin still points to the node we started with,
        // that node survived the resize and still owns the temporary style and
        // hyperlink references we added above.
        //
        // Comparing pointers is not enough by itself. PageList keeps a pool of
        // nodes, so it can destroy a node and create a replacement at the same
        // memory address. Every new use of a node gets a different serial number,
        // making the pointer-and-serial pair an O(1) identity check.
        let live_node = unsafe { (*self.cursor.page_pin).node };
        let cursor_node_survived = core::ptr::eq(
            live_node as *const crate::pagelist::Node,
            cursor_node as *const crate::pagelist::Node,
        ) && unsafe { (*live_node).serial } == cursor_node_serial;
        if crate::options::SLOW_RUNTIME_SAFETY {
            assert!(
                cursor_node_survived == self.pages.node_is_valid(cursor_node, cursor_node_serial)
            );
        }

        // A surviving node still contains our temporary references, so remove
        // them now. If the node was replaced, its destruction removed those
        // references along with the rest of the old page so we don't need to
        // fix it up.
        //
        // PORT NOTE: the reference writes `cursor_node.page()`. We deref
        // `live_node` instead — the same address when we get here, but with
        // live provenance; `cursor_node`'s allocation may have been retired and
        // re-created by the pool, which would make a deref through it UB under
        // Stacked Borrows even though the addresses match.
        if cursor_node_survived {
            let page: *mut Page = unsafe { crate::pagelist::Node::page(live_node) };
            unsafe {
                let base = (*page).memory_ptr();
                if cursor_style_id != style::DEFAULT_ID {
                    (*page).styles.release(base, cursor_style_id);
                }
                if cursor_hyperlink_id != 0 {
                    (*page).hyperlink_set.release(base, cursor_hyperlink_id);
                }
            }
        }

        Ok(())
    }

    /// The `errdefer` of `resize`, spelled out: put the cursor back exactly as
    /// it was, with NO allocation (the extra style/hyperlink references taken
    /// at the top of `resize_tracked` guarantee the IDs are still live).
    fn resize_restore_cursor(
        &mut self,
        cursor_style: style::Style,
        cursor_style_id: style::Id,
        cursor_hyperlink: Option<*mut CursorHyperlink>,
        cursor_hyperlink_id: hyperlink::Id,
    ) {
        self.cursor.style = cursor_style;
        self.cursor.style_id = cursor_style_id;
        self.cursor.hyperlink = cursor_hyperlink;
        self.cursor.hyperlink_id = cursor_hyperlink_id;
    }

    fn clear_prompt_for_redraw(&mut self, redraw: osc::semantic_prompt::Redraw) {
        // If our cursor is on a prompt or input line, clear it so the shell can
        // redraw it. This works with OSC 133 semantic prompts. We do this after
        // the fallible resize so an error leaves the original prompt untouched.
        //
        // We check cursor.semantic_content rather than page_row.semantic_prompt
        // because some shells (e.g., Nu) mark input areas with OSC 133 B but don't
        // mark continuation lines with k=s. If the input spans multiple lines and
        // continuation lines are unmarked, checking only page_row.semantic_prompt
        // would miss them. By checking semantic_content, we assume that if the
        // cursor is on anything other than command output, we're at a prompt/input
        // line and should clear from there.
        if !matches!(redraw, osc::semantic_prompt::Redraw::False)
            && !matches!(
                self.cursor.semantic_content,
                pagepkg::SemanticContent::Output
            )
        {
            'prompt: {
                match redraw {
                    osc::semantic_prompt::Redraw::False => unreachable!(),

                    // For `.last`, only clear the current line where the cursor is.
                    // For `.true`, clear all prompt lines starting from the beginning.
                    osc::semantic_prompt::Redraw::Last => {
                        let page: *mut Page =
                            unsafe { crate::pagelist::Node::page((*self.cursor.page_pin).node) };
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
                                    log::warn!(
                                        target: LOG_SCOPE,
                                        "cursor on prompt line but promptIterator found no prompt"
                                    );
                                    break 'prompt;
                                }
                            }
                        };

                        // Clear cells from our start down. We replace it with spaces,
                        // and do not physically erase the rows (eraseRows) because the
                        // shell is going to expect this space to be available.
                        let mut it = start.row_iterator(pagelist::iter::Direction::RightDown, None);
                        while let Some(pin) = it.next() {
                            let page: *mut Page = unsafe { crate::pagelist::Node::page(pin.node) };
                            let row = pin.row_and_cell().row;
                            let cells = unsafe { (*page).get_cells(row) };
                            self.clear_cells(page, row, cells);
                        }
                    }
                }
            }
        }
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
            if let Err(err) = s.manual_style_update() {
                log::warn!(
                    target: LOG_SCOPE,
                    "setAttribute error restoring old style after failure err={err:?}"
                );
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

        // If the attribute didn't change our style then we can skip the
        // style update entirely: our current style ID is already correct.
        // This is a common case in the wild where programs re-assert the
        // same style repeatedly (e.g. per span or per line).
        // PORT NOTE (parity cee35cabf).
        if self.cursor.style.eql(&old_style) {
            return Ok(());
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
        // PORT NOTE: `defer self.assertIntegrity()` — Zig's defer runs on ERROR
        // returns too, so the body lives in an inner fn and the integrity gate
        // runs unconditionally after it, on every exit path including the `?`
        // splitForCapacity early-return (VALIDATION_REPORT §3e / §3c.9).
        let result = self.manual_style_update_inner();
        self.assert_integrity();
        result
    }

    /// The body of `manual_style_update` (the reference fn minus its `defer`);
    /// see the PORT NOTE above.
    fn manual_style_update_inner(&mut self) -> Result<(), pagelist::grow::IncreaseCapacityError> {
        use crate::pagelist::grow::{IncreaseCapacity, IncreaseCapacityError};
        use crate::ref_counted_set::AddError;

        let mut page: *mut Page =
            unsafe { crate::pagelist::Node::page((*self.cursor.page_pin).node) };

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
                        return Err(IncreaseCapacityError::OutOfMemory);
                    }
                    Err(IncreaseCapacityError::OutOfSpace)
                        if self.reclaim_pinned_styles_before_split(page) =>
                    {
                        // PORT NOTE (R13, UD80): a same-capacity re-clone
                        // freed style slots; see the helper.
                        unsafe { (*self.cursor.page_pin).node }
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

                page = unsafe { crate::pagelist::Node::page(node) };
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
                        log::error!(target: LOG_SCOPE, "style addition failed after capacity increase");
                        return Err(IncreaseCapacityError::OutOfMemory);
                    }
                    Err(AddError::NeedsRehash) => {
                        // This should be impossible because we rehash above
                        // and rehashing should never result in a duplicate. But
                        // we don't want to simply hard crash so log it and
                        // clear our style.
                        log::error!(target: LOG_SCOPE, "style rehash resulted in needs rehash");
                        return Ok(());
                    }
                }
            }
        };

        self.cursor.style_id = id;
        Ok(())
    }

    /// Before `manualStyleUpdate` splits the cursor's page because its full
    /// style set cannot grow (`OutOfSpace`), re-clone the page once at the
    /// SAME capacity if the set holds a pinned style, and report whether the
    /// re-clone freed at least one style slot.
    ///
    /// PORT NOTE: reference defect (R13), UPSTREAM-DEFECTS.md UD80. The
    /// reference has no such step. The port pins a style whose `u16` reference
    /// count saturates (`ref_counted_set.rs`, `bump_ref`), and a pinned style
    /// stays counted after its cells are overwritten. Page clones recount from
    /// the cells and drop such orphans, and below the ceiling every style-set
    /// growth is a clone, but the capacity projection counts the orphans, so
    /// the style capacity can climb to `maxInt(u16)`; there growth returns
    /// `OutOfSpace` and the reference's fallback splits the page. The cursor
    /// can stay on the full half, and every later new style then fails
    /// (`set_attribute` keeps the previous style) although the set holds
    /// styles nothing uses — phantom references the reference never has.
    /// This step is the same clone growth already performs, at the capacity
    /// the page has. Returns `true` only if the style count fell; the caller
    /// then adds on the re-cloned page. Returns `false` if the set holds no
    /// pinned style (every count is exact, a re-clone can reclaim nothing, and
    /// the reference path runs unchanged, with no clone), if the clone
    /// reclaimed nothing (the styles are genuinely in use), or if it could not
    /// allocate; the caller then splits as the reference does. It runs at most
    /// once per `manual_style_update` call, with no retry, so it cannot loop.
    fn reclaim_pinned_styles_before_split(&mut self, page: *mut Page) -> bool {
        if !pagelist::grow::styles_hold_pinned(page) {
            return false;
        }
        let before = unsafe { (*page).styles.count() };
        // `Screen::increase_capacity`, not the PageList's, so the cursor's
        // hyperlink reference and cached row/cell pointers are migrated. The
        // cursor's style was released and its id reset to the default above,
        // so there is no style to re-add.
        match self.increase_capacity(unsafe { (*self.cursor.page_pin).node }, None) {
            Ok(node) => unsafe { (*crate::pagelist::Node::page(node)).styles.count() < before },
            Err(_) => false,
        }
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
        let bytes_above = Page::layout(unsafe {
            (*crate::pagelist::Node::page(pin.node)).exact_row_capacity(0, pin.y as usize + 1)
        })
        .total_size;
        let bytes_below = Page::layout(unsafe {
            let rows = crate::pagelist::Node::rows(pin.node) as usize;
            (*crate::pagelist::Node::page(pin.node)).exact_row_capacity(pin.y as usize, rows)
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
        // PORT NOTE: `defer self.cursor.page_pin.node.data.assertIntegrity()` —
        // Zig's defer runs on ERROR returns too (and reads the pin at DEFER
        // time, i.e. the possibly-reloaded pin), so the body lives in an inner
        // fn and the page integrity gate runs unconditionally after it against
        // the then-current pin (VALIDATION_REPORT §3e / §3c.9).
        let result = self.append_grapheme_inner(cell, cp);
        unsafe { (*crate::pagelist::Node::page((*self.cursor.page_pin).node)).assert_integrity() };
        result
    }

    /// The body of `append_grapheme` (the reference fn minus its `defer`); see
    /// the PORT NOTE above.
    fn append_grapheme_inner(
        &mut self,
        cell: *mut Cell,
        cp: u21,
    ) -> Result<(), pagelist::grow::IncreaseCapacityError> {
        use crate::pagelist::grow::IncreaseCapacity;

        let result = unsafe {
            (*crate::pagelist::Node::page((*self.cursor.page_pin).node)).append_grapheme(
                self.cursor.page_row,
                cell,
                cp,
            )
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
                    (*crate::pagelist::Node::page((*self.cursor.page_pin).node)).append_grapheme(
                        self.cursor.page_row,
                        reloaded_cell,
                        cp,
                    )
                }
                .map_err(|_err2| {
                    // This should never happen because we just increased capacity.
                    // Log loudly but still return an error so we don't just
                    // crash.
                    log::error!(target: LOG_SCOPE, "grapheme append failed after capacity increase");
                    pagelist::grow::IncreaseCapacityError::OutOfMemory
                })?;
            }
        }
        Ok(())
    }

    /// Start the hyperlink state. Future cells will be marked as hyperlinks with
    /// this state. Note that various terminal operations may clear the hyperlink
    /// state, such as switching screens (alt screen).
    ///
    /// `uri`/`id_` must NOT point into the screen's *current* hyperlink state
    /// (`cursor.hyperlink`), which this call releases. Use
    /// [`Screen::start_hyperlink_raw`] for that; see its `# Safety` section for
    /// why the distinction exists.
    ///
    /// This is sound as a safe function because that restriction is
    /// unviolatable from safe code: `cursor.hyperlink` is an
    /// `Option<*mut Hyperlink>`, so a slice into the current hyperlink's
    /// allocation can only be produced by `unsafe` code, which is then subject
    /// to `start_hyperlink_raw`'s contract instead.
    pub fn start_hyperlink(
        &mut self,
        uri: &[u8],
        id_: Option<&[u8]>,
    ) -> Result<(), pagelist::grow::IncreaseCapacityError> {
        // SAFETY: `uri`/`id_` are live readable slices for the whole call, and
        // per this function's documented restriction they do not alias the
        // current hyperlink state.
        unsafe { self.start_hyperlink_raw(uri as *const [u8], id_.map(|id| id as *const [u8])) }
    }

    /// [`Screen::start_hyperlink`] taking RAW slice pointers, so that the
    /// arguments may alias the screen's current hyperlink state.
    ///
    /// PORT NOTE (P3 F2): the oracle is
    /// `startHyperlink(self: *Screen, uri: []const u8, id_: ?[]const u8)` and
    /// its `startHyperlinkOnce` explicitly documents that "the source slices are
    /// allowed to reference our current hyperlink" — the reference even has a
    /// test for it (`"Screen: hyperlink accepts its current values"`). That
    /// contract is expressible in Zig, whose slices are unprotected pointers,
    /// but NOT with Rust `&[u8]` parameters: a reference-typed argument is
    /// STRONGLY PROTECTED for the entire duration of the call, and this call
    /// reaches `end_hyperlink`, which drops the `Box<Hyperlink>` those slices
    /// point into. Freeing a strongly-protected allocation is Undefined
    /// Behavior even though the reference is never read again — Miri: "not
    /// granting access to tag <N> because that would remove [SharedReadOnly for
    /// <M>] which is strongly protected", with `<M>` reported as `id_:
    /// Option<&[u8]>`. Raw pointers carry no protector, which is exactly the
    /// Zig parameter semantics, so the aliasing contract is ported by taking
    /// `*const [u8]`. (Same class as the `&mut Page`/`&mut [Cell]` raw-pointer
    /// argument conversions elsewhere in this crate.)
    ///
    /// # Safety
    ///
    /// `uri` and `id_` must point at readable `[u8]` slices on entry. They MAY
    /// alias the current hyperlink state: both are copied into owned buffers
    /// before that state is released. They must not be dangling.
    pub unsafe fn start_hyperlink_raw(
        &mut self,
        uri: *const [u8],
        id_: Option<*const [u8]>,
    ) -> Result<(), pagelist::grow::IncreaseCapacityError> {
        use crate::page::InsertHyperlinkError;
        use crate::pagelist::grow::IncreaseCapacity;

        // Create our pending entry. This copies both byte slices out of caller
        // memory BEFORE anything below can release the current hyperlink, which
        // is what makes the aliasing contract above work.
        //
        // PORT NOTE (OOM contract): these copies are `try alloc.dupe`'s Rust
        // equivalent (`hyperlink::try_dupe_bytes`), NOT `to_vec` — this function
        // ADVERTISES `IncreaseCapacityError!void` exactly like the reference's
        // `startHyperlink`, so an allocation failure must RETURN, not abort the
        // process. Both copies happen before the implicit-ID counter is bumped,
        // so their failure needs no rollback (the reference's `errdefer` covers
        // the same window because its `dupe` runs after the bump — the
        // observable state, "counter unchanged", is identical).
        // SAFETY: caller contract — `uri` is a readable slice on entry.
        let uri_owned = unsafe { hyperlink::try_dupe_bytes(&*uri) }
            .map_err(|_| pagelist::grow::IncreaseCapacityError::OutOfMemory)?;
        let link_id: hyperlink::HyperlinkId = if let Some(id) = id_ {
            // SAFETY: caller contract — `id_` is a readable slice on entry.
            hyperlink::HyperlinkId::Explicit(
                unsafe { hyperlink::try_dupe_bytes(&*id) }
                    .map_err(|_| pagelist::grow::IncreaseCapacityError::OutOfMemory)?,
            )
        } else {
            let implicit = self.cursor.hyperlink_implicit_id;
            // defer self.cursor.hyperlink_implicit_id +%= 1;
            self.cursor.hyperlink_implicit_id = self.cursor.hyperlink_implicit_id.wrapping_add(1);
            hyperlink::HyperlinkId::Implicit(implicit)
        };
        let link: hyperlink::Hyperlink = hyperlink::Hyperlink {
            uri: uri_owned,
            id: link_id,
        };
        // PORT NOTE: the reference `errdefer` decrements `hyperlink_implicit_id`
        // on the implicit path. The fallible work below is the per-attempt
        // `dupe` and the `increaseCapacity` calls; on those errors we restore
        // the counter to mirror the `errdefer`.
        let implicit_inc = matches!(link.id, hyperlink::HyperlinkId::Implicit(_));

        // Loop until we have enough page memory to add the hyperlink
        loop {
            // PORT NOTE: the reference passes `link` (borrowed slices, no
            // allocation) and `startHyperlinkOnce` does `try source.dupe(alloc)`
            // itself. We hand the callee an OWNED per-attempt copy instead (it
            // moves it straight into its heap allocation), so this is still
            // exactly one deep copy per attempt — but it must be the FALLIBLE
            // one, not `Clone`, which aborts.
            let attempt = match link.dupe() {
                Ok(v) => v,
                Err(hyperlink::OutOfMemory) => {
                    if implicit_inc {
                        // .implicit => self.cursor.hyperlink_implicit_id -%= 1
                        self.cursor.hyperlink_implicit_id =
                            self.cursor.hyperlink_implicit_id.wrapping_sub(1);
                    }
                    return Err(pagelist::grow::IncreaseCapacityError::OutOfMemory);
                }
            };
            match self.start_hyperlink_once(attempt) {
                Ok(()) => return Ok(()),
                Err(StartHyperlinkOnceError::OutOfMemory) => {
                    // An actual self.alloc OOM is a fatal error.
                    if implicit_inc {
                        // .implicit => self.cursor.hyperlink_implicit_id -%= 1
                        self.cursor.hyperlink_implicit_id =
                            self.cursor.hyperlink_implicit_id.wrapping_sub(1);
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
                            // .implicit => self.cursor.hyperlink_implicit_id -%= 1
                            self.cursor.hyperlink_implicit_id =
                                self.cursor.hyperlink_implicit_id.wrapping_sub(1);
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
        // Allocate our new Hyperlink entry in non-page memory. This
        // lets us quickly get access to URI, ID.
        // const link = try self.alloc.create(hyperlink.Hyperlink);
        // errdefer self.alloc.destroy(link);
        // link.* = try source.dupe(self.alloc);
        // errdefer link.deinit(self.alloc);
        //
        // PORT NOTE (parity tail S7-3): both go through the screen's allocator
        // (`create_cursor_hyperlink`), fallibly, as the reference's `try`s do.
        // `source` is the caller's owned argument (the reference's by-value
        // `link` with borrowed slices); it is dropped here, on the Rust heap.
        let link: *mut CursorHyperlink = create_cursor_hyperlink(self.alloc, &source)
            .map_err(|_| StartHyperlinkOnceError::OutOfMemory)?;

        // End any prior hyperlink only after duplicating the new value. The
        // source slices are allowed to reference our current hyperlink.
        self.end_hyperlink();

        // Insert the hyperlink into page memory
        let page: *mut Page = unsafe { crate::pagelist::Node::page((*self.cursor.page_pin).node) };
        // PORT NOTE: the reference passes `link.*` BY VALUE with borrowed slices
        // — no allocation. `Hyperlink` owns its `Vec`s here, so the by-value
        // `insert_hyperlink` argument needs a copy; it must be the fallible
        // `dupe`, not `Clone`, for the same reason as above.
        //
        // The copy is `source` itself (the caller's owned per-attempt value,
        // on the Rust heap): moving it in makes no allocation the reference
        // does not make.
        let id: hyperlink::Id = match unsafe { (*page).insert_hyperlink(source) } {
            Ok(id) => id,
            Err(e) => {
                // errdefer link.deinit(self.alloc); errdefer self.alloc.destroy(link);
                unsafe { destroy_cursor_hyperlink(link) };
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
        let page: *mut Page = unsafe { crate::pagelist::Node::page((*self.cursor.page_pin).node) };
        unsafe {
            let base = (*page).memory_ptr();
            (*page)
                .hyperlink_set
                .release(base, self.cursor.hyperlink_id);
        }
        let link = self.cursor.hyperlink.unwrap();
        // self.cursor.hyperlink.?.deinit(self.alloc);
        // self.alloc.destroy(self.cursor.hyperlink.?);
        unsafe { destroy_cursor_hyperlink(link) };
        self.cursor.hyperlink_id = 0;
        self.cursor.hyperlink = None;
    }

    /// Set the current hyperlink state on the current cell.
    pub fn cursor_set_hyperlink(&mut self) -> Result<(), pagelist::grow::IncreaseCapacityError> {
        use crate::pagelist::grow::IncreaseCapacity;

        debug_assert!(self.cursor.hyperlink_id != 0);

        let mut page: *mut Page =
            unsafe { crate::pagelist::Node::page((*self.cursor.page_pin).node) };
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
                    page = unsafe { crate::pagelist::Node::page(new_node) };
                }

                // The hyperlink map is fixed-capacity, so reaching this error
                // means live entries fill the usable map capacity and the page
                // must grow.
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

        // Untrack prior selection pins that aren't also owned by the
        // replacement. A caller may pass our current tracked selection back to
        // us by value, so releasing both old pins unconditionally would leave
        // the replacement pointing at freed pool entries.
        if let Some(old) = self.selection.take() {
            // const new_bounds = tracked_sel.bounds.tracked;
            // PORT NOTE: `track` always yields `Bounds::Tracked`, so the
            // reference's unchecked `.tracked` field access is faithful; the
            // untracked arm is unreachable.
            let (new_start, new_end): (*mut Pin, *mut Pin) = match tracked_sel.bounds {
                selection::Bounds::Tracked { start, end } => (start, end),
                selection::Bounds::Untracked { .. } => unreachable!(),
            };
            match old.bounds {
                selection::Bounds::Untracked { .. } => old.deinit(self),
                selection::Bounds::Tracked { start, end } => {
                    if !core::ptr::eq(start, new_start) && !core::ptr::eq(start, new_end) {
                        self.pages.untrack_pin(start);
                    }
                    if !core::ptr::eq(end, new_start) && !core::ptr::eq(end, new_end) {
                        self.pages.untrack_pin(end);
                    }
                }
            }
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
    // faithful map is an UNSENTINELED `Vec<u8>` (never a lossy `String`) whose
    // `len()` equals the Zig slice's sentinel-EXCLUSIVE `len`. No NUL byte is
    // stored — see the sink note below.
    // pub fn selectionString(
    //     self: *Screen,
    //     alloc: Allocator,
    //     opts: SelectionString,
    // ) Allocator.Error![:0]const u8 {
    //     return self.selectionStringImpl(alloc, opts, null);
    // }
    pub fn selection_string(
        &mut self,
        alloc: Allocator,
        opts: SelectionString,
    ) -> Result<Vec<u8>, OutOfMemory> {
        self.selection_string_impl(alloc, opts, None)
    }

    /// Returns a StringMap associated with a selection. The map contains the
    /// selection's raw text and a mapping from each byte to its screen location.
    ///
    /// The returned map is owned by the caller.
    // pub fn selectionStringMap(
    //     self: *Screen,
    //     alloc: Allocator,
    //     opts: SelectionString,
    // ) Allocator.Error!StringMap {
    pub fn selection_string_map(
        &mut self,
        alloc: Allocator,
        opts: SelectionString,
    ) -> Result<StringMap, OutOfMemory> {
        // var pins: PinMap.Map = .empty;
        // errdefer pins.deinit(alloc);
        // PORT NOTE: `pins` is an owning value; the early `?` return below drops
        // it, which IS the faithful errdefer. On success ownership of the pin
        // map moves into the returned `StringMap`.
        let mut pins: formatter::PinMapMap = formatter::PinMapMap::EMPTY;
        Ok(StringMap {
            string: self.selection_string_impl(alloc, opts, Some(&mut pins))?,
            map: pins,
        })
    }

    fn selection_string_impl(
        &mut self,
        alloc: Allocator,
        opts: SelectionString,
        pins: Option<&mut formatter::PinMapMap>,
    ) -> Result<Vec<u8>, OutOfMemory> {
        // We'll use this as our buffer to build our string.
        // PORT NOTE: the reference builds a `std.Io.Writer.Allocating` over
        // `alloc` and returns `toOwnedSliceSentinel(0)`. A `Vec<u8>` is the
        // faithful sink (the same convention as `dumpStringAlloc`): its length
        // is the sentinel-EXCLUSIVE length `toOwnedSliceSentinel(0)` reports.
        // The port stores NO trailing NUL. That is safe because no consumer on
        // either side ever reads the sentinel: `StringMap`'s regex search
        // passes explicit start/end byte bounds into oniguruma, and
        // `rustty-core`'s `link_at_pin` goes through that same bounded API. See
        // the `StringMap.string` PORT NOTE (`string_map.rs`), which also explains
        // why appending a 0 byte here would BREAK parity rather than restore it.
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

        // if (pins) |map| formatter.pin_map = .{
        //     .alloc = alloc,
        //     .map = map,
        // };
        if let Some(map) = pins {
            formatter.pin_map = Some(formatter::PinMap { alloc, map });
        }

        // Emit. Since this is an allocating writer, a failed write
        // just becomes an OOM.
        formatter.format(&mut aw).map_err(|_| OutOfMemory)?;
        // return try aw.toOwnedSliceSentinel(0);
        Ok(aw)
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
                let cells = unsafe { (*crate::pagelist::Node::page(it_prev.node)).get_cells(row) };
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
                    let cells = unsafe { (*crate::pagelist::Node::page(p.node)).get_cells(row) };
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
                    let cells = unsafe { (*crate::pagelist::Node::page(p.node)).get_cells(row) };
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
                        // prev.x = prev.node.cols() - 1;
                        prev.x = unsafe { crate::pagelist::Node::cols(prev.node) } - 1;
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
                    copy.x = unsafe { crate::pagelist::Node::cols(p.node) } - 1;
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
                if p.x == unsafe { crate::pagelist::Node::cols(p.node) } - 1
                    && !unsafe { (*rac.row).wrap() }
                {
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
                if p.x == unsafe { crate::pagelist::Node::cols(p.node) } - 1
                    && !unsafe { (*rac.row).wrap() }
                {
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
            end_pin.x = unsafe { crate::pagelist::Node::cols(end_pin.node) } - 1;
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
                let cells: &[pagepkg::Cell] =
                    unsafe { &*(*crate::pagelist::Node::page(row_pin.node)).get_cells(rac.row) };

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
            let cells: &[pagepkg::Cell] =
                unsafe { &*(*crate::pagelist::Node::page(row_pin.node)).get_cells(rac.row) };

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
    ) -> Result<Vec<u8>, DumpStringAllocError> {
        // PORT NOTE: the reference builds an `Allocating` writer over `alloc`
        // and returns `toOwnedSlice()`. A `Vec<u8>` is the faithful sink; the
        // allocator is the crate's `Global` seam (matching the rest of Screen).
        let _ = alloc;
        let mut builder: Vec<u8> = Vec::new();

        // `getBottomRight(tl) orelse return error.UnknownPoint`.
        let br = match self.pages.get_bottom_right(tl.tag()) {
            Some(p) => p,
            None => return Err(DumpStringAllocError::UnknownPoint),
        };

        self.dump_string(
            &mut builder,
            DumpStringOptions {
                tl: self.pages.get_top_left(tl.tag()),
                br: Some(br),
                unwrap: false,
            },
        )
        .map_err(|_| DumpStringAllocError::OutOfMemory)?;

        Ok(builder)
    }

    /// You should use dumpString, this is a restricted version mostly for
    /// legacy and convenience reasons for unit tests.
    pub fn dump_string_alloc_unwrapped(
        &self,
        alloc: Allocator,
        tl: point::Point,
    ) -> Result<Vec<u8>, DumpStringAllocError> {
        let _ = alloc;
        let mut builder: Vec<u8> = Vec::new();

        let br = match self.pages.get_bottom_right(tl.tag()) {
            Some(p) => p,
            None => return Err(DumpStringAllocError::UnknownPoint),
        };

        self.dump_string(
            &mut builder,
            DumpStringOptions {
                tl: self.pages.get_top_left(tl.tag()),
                br: Some(br),
                unwrap: true,
            },
        )
        .map_err(|_| DumpStringAllocError::OutOfMemory)?;

        Ok(builder)
    }

    /// This is basically a really jank version of Terminal.printString. We
    /// have to reimplement it here because we want a way to print to the screen
    /// to test it but don't want all the features of Terminal.
    pub fn test_write_string(&mut self, text: &[u8]) -> Result<(), PrintStringError> {
        // const view = try std.unicode.Utf8View.init(text);
        // var iter = view.iterator();
        // while (iter.nextCodepoint()) |c| { ... }
        // PORT NOTE: `std.unicode.Utf8View.init` validates UTF-8 then iterates
        // codepoints; an invalid sequence is `error.InvalidUtf8`. We decode via
        // `str::from_utf8` (the same validation) and iterate `chars()`; the
        // interior allocation failures convert via `From<OutOfMemory>`.
        let s = core::str::from_utf8(text).map_err(|_| PrintStringError::InvalidUtf8)?;
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
                    (*crate::pagelist::Node::page((*self.cursor.page_pin).node)).append_grapheme(
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
                        let page =
                            unsafe { crate::pagelist::Node::page((*self.cursor.page_pin).node) };
                        unsafe {
                            let base = (*page).memory_ptr();
                            (*page).styles.use_id(base, self.cursor.style_id);
                        }
                        unsafe { (*self.cursor.page_row).set_styled(true) };
                    }

                    // If we have a hyperlink, add it to the cell.
                    if self.cursor.hyperlink_id > 0 {
                        self.cursor_set_hyperlink()?;
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
                            self.cursor_set_hyperlink()?;
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
                        self.cursor_set_hyperlink()?;
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
                        self.cursor_set_hyperlink()?;
                    }

                    // If we have a ref-counted style, increase twice.
                    if self.cursor.style_id != style::DEFAULT_ID {
                        let page =
                            unsafe { crate::pagelist::Node::page((*self.cursor.page_pin).node) };
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
                max_scrollback_bytes: Some(0),
                max_scrollback_lines: None,
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
                max_scrollback_bytes: Some(0),
                max_scrollback_lines: None,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();
        let page: *mut Page = unsafe { crate::pagelist::Node::page((*s2.cursor.page_pin).node) };

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

    // PORT NOTE: no reference test. `Screen.clone` returns `!Screen` whose
    // inferred error set is `PageList.clone`'s, which carries
    // `Page.CloneFromError` (`StyleSetError || HyperlinkError ||
    // GraphemeError`) out of `PageList.zig:1173`'s `try dst_page.cloneFrom(..)`.
    // The port collapsed all of it into `OutOfMemory` (parity S133-F4); it now
    // propagates `pagelist::access::CloneError` unchanged. Reaching a real
    // `CloneFrom` failure needs a page whose style set cannot be rehashed into
    // the destination, which no reference test constructs either; what IS
    // pinned here is that the variant is no longer erased by the signature.
    #[test]
    fn screen_clone_error_carries_clone_from() {
        use crate::page::CloneFromError;
        use crate::pagelist::access::CloneError;

        // The `?` in `Screen::clone` performs exactly this conversion.
        let err: CloneError = CloneFromError::StyleSetNeedsRehash.into();
        assert!(matches!(
            err,
            CloneError::CloneFrom(CloneFromError::StyleSetNeedsRehash)
        ));
        assert!(!matches!(err, CloneError::OutOfMemory));
    }

    // PORT NOTE: no reference test. The reference's `errdefer self.cursor = old`
    // in `cursorCopy` is exercised by a failing allocator inside
    // `manualStyleUpdate`; the port has no swappable allocator there, so the
    // `CURSOR_COPY_TW` fail point stands in (same rationale as `grow.rs`'s
    // `GROW_TW`). Pins BOTH halves of parity S133-F3: the error variant is no
    // longer flattened to `OutOfMemory`, and the cursor is restored WHOLE
    // instead of being left half-applied.
    #[test]
    fn screen_cursor_copy_restores_the_whole_cursor_on_failure() {
        use rustty_util::tripwire::ResetMode;

        let opts = || Options {
            cols: 10,
            rows: 10,
            max_scrollback_bytes: Some(0),
            max_scrollback_lines: None,
            kitty_image_storage_limit: 0,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        };
        let mut s = Screen::init(Allocator, opts()).unwrap();
        let mut s2 = Screen::init(Allocator, opts()).unwrap();

        // Give the two cursors different observable state: `s` is bold at
        // (0, 0); `s2` is plain at (3, 4).
        s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
        s2.cursor_absolute(3, 4);
        let before_x = s2.cursor.x;
        let before_y = s2.cursor.y;
        let before_style = s2.cursor.style;
        let before_style_id = s2.cursor.style_id;
        let before_pin = s2.cursor.page_pin;
        assert!(!s2.cursor.style.flags.bold);

        // Fail the `manualStyleUpdate` the reference's errdefer guards, with
        // the variant the port used to throw away.
        CURSOR_COPY_TW.with(|tw| {
            tw.borrow_mut().errorAlways(
                CursorCopyFp::ManualStyleUpdate,
                pagelist::grow::IncreaseCapacityError::OutOfSpace,
            )
        });
        assert_eq!(
            Err(pagelist::grow::IncreaseCapacityError::OutOfSpace),
            s2.cursor_copy(s.cursor, CursorCopyOptions { hyperlink: true }),
        );
        CURSOR_COPY_TW
            .with(|tw| tw.borrow_mut().end(ResetMode::Reset))
            .unwrap();

        // The whole cursor is back, not a half-applied mixture of the two.
        assert_eq!(before_x, s2.cursor.x);
        assert_eq!(before_y, s2.cursor.y);
        assert_eq!(before_style_id, s2.cursor.style_id);
        assert!(core::ptr::eq(before_pin, s2.cursor.page_pin));
        assert_eq!(before_style.flags.bold, s2.cursor.style.flags.bold);
        assert!(!s2.cursor.style.flags.bold);

        // Disarmed, the same call succeeds and DOES apply the other cursor, so
        // the assertions above are about the failure path.
        s2.cursor_copy(s.cursor, CursorCopyOptions { hyperlink: true })
            .unwrap();
        assert!(s2.cursor.style.flags.bold);
        assert_eq!(0, s2.cursor.x);
        assert_eq!(0, s2.cursor.y);

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
                max_scrollback_bytes: Some(0),
                max_scrollback_lines: None,
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
                max_scrollback_bytes: Some(2048),
                max_scrollback_lines: None,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();

        // We need to get the cursor on a new page.
        let first_page_size = unsafe { crate::pagelist::Node::capacity(s2.pages.pages.first).rows };

        // Fill the scrollback with blank lines until
        // there are only 5 rows left on the first page.
        unsafe {
            (*crate::pagelist::Node::page(s2.pages.pages.first)).pause_integrity_checks(true)
        };
        for _ in 0..(first_page_size - 5) {
            s2.test_write_string(b"\n").unwrap();
        }
        unsafe {
            (*crate::pagelist::Node::page(s2.pages.pages.first)).pause_integrity_checks(false)
        };

        s2.test_write_string(b"1\n2\n3\n4\n5\n6\n7\n8\n9\n10")
            .unwrap();

        // This should be PAGE 1
        let page: *mut Page = unsafe { crate::pagelist::Node::page((*s2.cursor.page_pin).node) };

        // It should be the last page in the list.
        assert!(std::ptr::eq(
            unsafe { crate::pagelist::Node::page(s2.pages.pages.last) } as *const _,
            page as *const _
        ));
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
            crate::pagelist::Node::page((*(*s2.cursor.page_pin).node).next)
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
                max_scrollback_bytes: Some(0),
                max_scrollback_lines: None,
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
                max_scrollback_bytes: Some(0),
                max_scrollback_lines: None,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();
        let page: *mut Page = unsafe { crate::pagelist::Node::page((*s2.cursor.page_pin).node) };
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
                max_scrollback_bytes: Some(0),
                max_scrollback_lines: None,
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
                max_scrollback_bytes: Some(0),
                max_scrollback_lines: None,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();
        let page: *mut Page = unsafe { crate::pagelist::Node::page((*s2.cursor.page_pin).node) };

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
                max_scrollback_bytes: Some(0),
                max_scrollback_lines: None,
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
                max_scrollback_bytes: Some(2048),
                max_scrollback_lines: None,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();

        // We need to get the cursor on a new page.
        let first_page_size = unsafe { crate::pagelist::Node::capacity(s2.pages.pages.first).rows };

        // Fill the scrollback with blank lines until
        // there are only 5 rows left on the first page.
        unsafe {
            (*crate::pagelist::Node::page(s2.pages.pages.first)).pause_integrity_checks(true)
        };
        for _ in 0..(first_page_size - 5) {
            s2.test_write_string(b"\n").unwrap();
        }
        unsafe {
            (*crate::pagelist::Node::page(s2.pages.pages.first)).pause_integrity_checks(false)
        };

        s2.test_write_string(b"1\n2\n3\n4\n5\n6\n7\n8\n9\n10")
            .unwrap();

        // This should be PAGE 1
        let page: *mut Page = unsafe { crate::pagelist::Node::page((*s2.cursor.page_pin).node) };

        // It should be the last page in the list.
        assert!(std::ptr::eq(
            unsafe { crate::pagelist::Node::page(s2.pages.pages.last) } as *const _,
            page as *const _
        ));
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
            crate::pagelist::Node::page((*(*s2.cursor.page_pin).node).next)
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
                max_scrollback_bytes: Some(0),
                max_scrollback_lines: None,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();

        // Create a hyperlink for the cursor.
        s.start_hyperlink(b"https://example.com/", None).unwrap();
        assert_eq!(1usize, unsafe {
            (*crate::pagelist::Node::page((*s.cursor.page_pin).node))
                .hyperlink_set
                .count()
        });
        assert!(s.cursor.hyperlink_id != 0);

        let mut s2 = Screen::init(
            Allocator,
            Options {
                cols: 10,
                rows: 10,
                max_scrollback_bytes: Some(0),
                max_scrollback_lines: None,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();
        let page: *mut Page = unsafe { crate::pagelist::Node::page((*s2.cursor.page_pin).node) };

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
                max_scrollback_bytes: Some(0),
                max_scrollback_lines: None,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();

        // Create a hyperlink for the cursor.
        s.start_hyperlink(b"https://example.com/", None).unwrap();
        assert_eq!(1usize, unsafe {
            (*crate::pagelist::Node::page((*s.cursor.page_pin).node))
                .hyperlink_set
                .count()
        });
        assert!(s.cursor.hyperlink_id != 0);

        let mut s2 = Screen::init(
            Allocator,
            Options {
                cols: 10,
                rows: 10,
                max_scrollback_bytes: Some(0),
                max_scrollback_lines: None,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();
        let page: *mut Page = unsafe { crate::pagelist::Node::page((*s2.cursor.page_pin).node) };

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
                max_scrollback_bytes: Some(1000),
                max_scrollback_lines: None,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();
        let page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
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
                max_scrollback_bytes: Some(1000),
                max_scrollback_lines: None,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();
        let page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
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
                max_scrollback_bytes: Some(1000),
                max_scrollback_lines: None,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();
        let page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
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
                max_scrollback_bytes: Some(1),
                max_scrollback_lines: None,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();

        // Scroll down enough to go to another page
        let start_page: *mut Page = unsafe { crate::pagelist::Node::page(s.pages.pages.last) };
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
            let page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
            assert!(!std::ptr::eq(start_page, page));
        }

        // Scroll back to the previous page
        s.cursor_up(1);
        {
            let page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
            assert!(std::ptr::eq(start_page, page));
        }

        // Go back up, set a style
        s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
        {
            let page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
            let styleval = unsafe {
                let base = (*page).memory_ptr();
                &*(*page).styles.get(base, s.cursor.style_id)
            };
            assert!(styleval.flags.bold);
        }

        // Go back down into the next page and we should have that style
        s.cursor_down(1);
        {
            let page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
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
                max_scrollback_bytes: Some(1),
                max_scrollback_lines: None,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();

        let start_page: *mut Page = unsafe { crate::pagelist::Node::page(s.pages.pages.last) };
        let rem = unsafe { (*start_page).capacity.rows };
        unsafe { (*start_page).pause_integrity_checks(true) };
        for _ in 0..rem {
            s.cursor_down_or_scroll().unwrap();
        }
        unsafe { (*start_page).pause_integrity_checks(false) };

        {
            let page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
            assert!(!std::ptr::eq(start_page, page));
        }

        // Go back up, set a style
        s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
        {
            let page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
            let styleval = unsafe {
                let base = (*page).memory_ptr();
                &*(*page).styles.get(base, s.cursor.style_id)
            };
            assert!(styleval.flags.bold);
        }

        // Go back down into the prev page and we should have that style
        s.cursor_up(1);
        {
            let page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
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
                max_scrollback_bytes: Some(1),
                max_scrollback_lines: None,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();

        let start_page: *mut Page = unsafe { crate::pagelist::Node::page(s.pages.pages.last) };
        let rem = unsafe { (*start_page).capacity.rows };
        unsafe { (*start_page).pause_integrity_checks(true) };
        for _ in 0..rem {
            s.cursor_down_or_scroll().unwrap();
        }
        unsafe { (*start_page).pause_integrity_checks(false) };

        {
            let page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
            assert!(!std::ptr::eq(start_page, page));
        }

        // Go back up, set a style
        s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
        {
            let page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
            let styleval = unsafe {
                let base = (*page).memory_ptr();
                &*(*page).styles.get(base, s.cursor.style_id)
            };
            assert!(styleval.flags.bold);
        }

        // Go back down into the prev page and we should have that style
        s.cursor_absolute(1, 1);
        {
            let page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
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
    #[cfg_attr(
        all(miri, debug_assertions),
        ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
    )]
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
                max_scrollback_bytes: Some(1),
                max_scrollback_lines: None,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();

        // Scroll down enough to go to another page
        let start_page: *mut Page = unsafe { crate::pagelist::Node::page(s.pages.pages.last) };
        let rem = unsafe { (*start_page).capacity.rows };
        unsafe { (*start_page).pause_integrity_checks(true) };
        for _ in 0..rem {
            s.cursor_down_or_scroll().unwrap();
        }
        unsafe { (*start_page).pause_integrity_checks(false) };

        let new_page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };

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
                unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
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

        unsafe { (*crate::pagelist::Node::page((*s.cursor.page_pin).node)).assert_integrity() };
        unsafe { (*new_page).assert_integrity() };

        s.deinit();
    }

    // PORT NOTE: these tests were DEFERRED from vt-11a because they require
    // `dumpStringAlloc`, which lands here in vt-11b alongside the formatter
    // production code.

    // ref: test "Screen forwards optional scrollback limits"
    #[test]
    fn screen_forwards_optional_scrollback_limits() {
        let max_lines: usize = 123;
        let mut s = Screen::init(
            Allocator,
            Options {
                cols: 80,
                rows: 24,
                max_scrollback_bytes: None,
                max_scrollback_lines: Some(max_lines),
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();

        assert_eq!(usize::MAX, s.pages.limits.bytes.explicit);
        assert_eq!(max_lines, s.pages.limits.lines.explicit);
        assert!(!s.no_scrollback);

        s.deinit();
    }

    // test "Screen reset cursor pin is not garbage" (oracle `d30a9c424`)
    #[test]
    fn screen_reset_cursor_pin_is_not_garbage() {
        let mut s = Screen::init(
            Allocator,
            Options {
                cols: 80,
                rows: 24,
                max_scrollback_bytes: Some(1000),
                max_scrollback_lines: None,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();
        s.test_write_string(b"hello, world").unwrap();

        // The page reset marks every tracked pin garbage but the screen
        // keeps using the cursor pin, so it must come back clean: anything
        // that copies it (e.g. Kitty image placements) would otherwise be
        // born garbage and reaped.
        s.reset();
        assert!(!unsafe { (*s.cursor.page_pin).garbage });

        s.deinit();
    }

    // PORT NOTE (rustty-only guard, `P3-W3` T2 Step 3): `10bc43420` makes the
    // byte limit OPTIONAL, so `Some(0)` ("disable scrollback") and `None`
    // ("unlimited") are now two DIFFERENT encodings where the pinned tree had
    // only one `usize` and no way to say "unlimited" at all. A two-case test
    // cannot tell the new encoding from the old, so this asserts all THREE
    // behaviours against the same growth workload.
    #[test]
    #[cfg_attr(
        all(miri, debug_assertions),
        ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound in the debug-assertions-ON G6 lane, where `Page::assert_integrity` runs on nearly every page mutation (measured per test; see the C-1 table there). It RUNS in the G6c lane (`check_leaks.sh --debug-assertions-off`). Do NOT widen this to a plain `miri` ignore: that would drop the test from BOTH lanes."
    )]
    fn screen_scrollback_bytes_zero_none_and_finite_are_distinct() {
        fn sopts(bytes: Option<usize>) -> Options {
            Options {
                cols: 80,
                rows: 1,
                max_scrollback_bytes: bytes,
                max_scrollback_lines: None,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            }
        }

        // (a) Some(0) DISABLES scrollback: the screen reports no_scrollback and
        //     the pagelist keeps only its structural minimum.
        let mut zero = Screen::init(Allocator, sopts(Some(0))).unwrap();
        assert!(zero.no_scrollback);
        assert_eq!(0usize, zero.pages.limits.bytes.explicit);

        let page_rows: usize =
            unsafe { crate::pagelist::Node::capacity(zero.pages.pages.first) }.rows as usize;
        let grow: usize = 20 * page_rows;
        zero.pages.grow_rows(grow).unwrap();
        assert!(zero.pages.page_size <= zero.pages.limits.max(Key::Bytes));
        let zero_rows = zero.pages.total_rows;

        // (b) None is UNLIMITED: scrollback is enabled and growth is unbounded.
        let mut unlimited = Screen::init(Allocator, sopts(None)).unwrap();
        assert!(!unlimited.no_scrollback);
        assert_eq!(usize::MAX, unlimited.pages.limits.bytes.explicit);
        unlimited.pages.grow_rows(grow).unwrap();
        assert_eq!(grow + 1, unlimited.pages.total_rows);
        let unlimited_rows = unlimited.pages.total_rows;

        // (c) Some(n) PRUNES at n: scrollback is enabled but bounded.
        let bounded_limit = 2 * zero.pages.limits.bytes.min;
        let mut bounded = Screen::init(Allocator, sopts(Some(bounded_limit))).unwrap();
        assert!(!bounded.no_scrollback);
        assert_eq!(bounded_limit, bounded.pages.limits.bytes.explicit);
        bounded.pages.grow_rows(grow).unwrap();
        assert!(bounded.pages.page_size <= bounded.pages.limits.max(Key::Bytes));
        let bounded_rows = bounded.pages.total_rows;

        // The three encodings must be strictly ordered; any two of them
        // collapsing would mean the optional encoding is not being honored.
        assert!(zero_rows < bounded_rows, "{zero_rows} < {bounded_rows}");
        assert!(
            bounded_rows < unlimited_rows,
            "{bounded_rows} < {unlimited_rows}"
        );

        bounded.deinit();
        unlimited.deinit();
        zero.deinit();
    }

    // test "Screen read and write"
    #[test]
    fn screen_read_and_write() {
        let mut s = Screen::init(
            Allocator,
            Options {
                cols: 80,
                rows: 24,
                max_scrollback_bytes: Some(1000),
                max_scrollback_lines: None,
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

    // (no reference test) PORT-ADDITIVE proving test (W29-B02): invalid UTF-8
    // must surface as `PrintStringError::InvalidUtf8` — the Zig
    // `try std.unicode.Utf8View.init` error — not `OutOfMemory`, and must
    // write nothing (validation happens before any codepoint is written).
    #[test]
    fn screen_test_write_string_invalid_utf8() {
        let mut s = Screen::init(
            Allocator,
            Options {
                cols: 80,
                rows: 24,
                max_scrollback_bytes: Some(1000),
                max_scrollback_lines: None,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();

        assert_eq!(
            s.test_write_string(b"foo\xFFbar"),
            Err(PrintStringError::InvalidUtf8)
        );

        // Nothing was written: validation fails before iteration begins.
        let str = s
            .dump_string_alloc(
                Allocator,
                point::Point::Screen(point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(str, b"");

        // Valid input still writes (regression arm).
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
                max_scrollback_bytes: Some(1000),
                max_scrollback_lines: None,
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
                max_scrollback_bytes: Some(1000),
                max_scrollback_lines: None,
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
                max_scrollback_bytes: Some(0),
                max_scrollback_lines: None,
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
                max_scrollback_bytes: Some(0),
                max_scrollback_lines: None,
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
                max_scrollback_bytes: Some(0),
                max_scrollback_lines: None,
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
                max_scrollback_bytes: Some(0),
                max_scrollback_lines: None,
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
        max_scrollback_bytes: usize,
    ) -> Screen {
        Screen::init(
            Allocator,
            Options {
                cols,
                rows,
                max_scrollback_bytes: Some(max_scrollback_bytes),
                max_scrollback_lines: None,
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

    // PORT NOTE: no reference test; this pins the `error.UnknownPoint` arm of
    // `dumpStringAlloc`/`dumpStringAllocUnwrapped` (Screen.zig:3616, :3635),
    // which is reachable for a `.history` point when there is no scrollback
    // (PageList.getBottomRight(.history) returns null because the active
    // top-left has no row above it).
    #[test]
    fn screen_dump_string_alloc_history_unknown_point() {
        let mut s = mkscreen(10, 3, 0);

        // No scrollback: there is no history region, so both helpers report
        // `UnknownPoint` rather than collapsing onto the OOM variant.
        assert_eq!(
            Err(DumpStringAllocError::UnknownPoint),
            s.dump_string_alloc(
                Allocator,
                point::Point::History(point::Coordinate::default())
            )
        );
        assert_eq!(
            Err(DumpStringAllocError::UnknownPoint),
            s.dump_string_alloc_unwrapped(
                Allocator,
                point::Point::History(point::Coordinate::default())
            )
        );
        s.deinit();

        // Positive control: once rows have scrolled into history, the same
        // point resolves and the dump succeeds.
        let mut s = mkscreen(10, 3, 1000);
        s.test_write_string(b"1ABCD\n2EFGH\n3IJKL\n4MNOP").unwrap();
        assert_eq!(
            Ok(b"1ABCD".to_vec()),
            s.dump_string_alloc(
                Allocator,
                point::Point::History(point::Coordinate::default())
            )
        );
        assert_eq!(
            Ok(b"1ABCD".to_vec()),
            s.dump_string_alloc_unwrapped(
                Allocator,
                point::Point::History(point::Coordinate::default())
            )
        );
        s.deinit();
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
            let page: *mut Page = unsafe { crate::pagelist::Node::page(chunk.node) };
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

    // test "Screen: resize errors preserve state"
    //
    // WS18 release lane: this test injects a failure through a `rustty_util::tripwire`
    // fail point. `tripwire::Module::enabled()` is the union of `cfg!(test)`,
    // `cfg!(debug_assertions)` and the `tripwire-test` feature, which this crate
    // turns on as a DEV-dependency (Cargo.toml) — reproducing Zig's
    // `builtin.is_test`, true at ANY optimize mode. So this runs in BOTH the debug
    // and the release test lanes, like the reference test does. No `ignore`.
    #[test]
    fn screen_resize_errors_preserve_state() {
        // for (std.meta.tags(resize_tw.FailPoint)) |tag| { ... }
        for tag in ResizeFp::ALL {
            let mut s = mkscreen(10, 3, 0);

            s.cursor_set_semantic_content(SemanticContentArg::Prompt(
                osc::semantic_prompt::PromptKind::Initial,
            ));
            s.test_write_string(b"> ").unwrap();
            s.cursor_set_semantic_content(SemanticContentArg::Input(
                SemanticContentInput::ClearExplicit,
            ));
            s.test_write_string(b"echo").unwrap();
            s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
            s.start_hyperlink(b"https://example.com", Some(b"resize"))
                .unwrap();
            s.saved_cursor = Some(SavedCursor {
                x: 1,
                y: 0,
                style: s.cursor.style,
                protected: s.cursor.protected,
                pending_wrap: s.cursor.pending_wrap,
                origin: false,
                charset: s.charset.clone(),
            });

            if crate::options::KITTY_GRAPHICS {
                s.kitty_images.dirty = false;
            }

            // Keep a shallow copy for all non-page state and a byte-for-byte
            // copy of the sole page so reference counts and prompt contents are
            // covered as well.
            assert!(std::ptr::eq(s.pages.pages.first, s.pages.pages.last));
            // PORT NOTE: `const before = s;` is a Zig by-value struct copy
            // compared with `std.meta.eql`. `Screen` is not `Copy` in Rust and
            // most of its members have no `PartialEq`, so we snapshot exactly
            // the fields the reference then compares, field by field.
            let before_cursor = s.cursor;
            let before_saved_cursor = {
                let sc = s.saved_cursor.as_ref().unwrap();
                (sc.x, sc.y, sc.protected, sc.pending_wrap, sc.origin)
            };
            // The reference compares the WHOLE saved cursor with
            // `std.meta.eql`, which also covers `style` and `charset`; neither
            // has `PartialEq` here, so snapshot them alongside the tuple above.
            let before_saved_style = s.saved_cursor.as_ref().unwrap().style;
            let before_saved_charset = {
                let cs = &s.saved_cursor.as_ref().unwrap().charset;
                (
                    cs.gl,
                    cs.gr,
                    cs.single_shift,
                    cs.charsets.g0,
                    cs.charsets.g1,
                    cs.charsets.g2,
                    cs.charsets.g3,
                )
            };
            let before_selection_is_none = s.selection.is_none();
            // `std.meta.eql(before.charset, s.charset)` also covers the
            // charsets table, so compare g0..g3 too.
            let before_charset = (
                s.charset.gl,
                s.charset.gr,
                s.charset.single_shift,
                s.charset.charsets.g0,
                s.charset.charsets.g1,
                s.charset.charsets.g2,
                s.charset.charsets.g3,
            );
            let before_protected_mode = s.protected_mode;
            let before_kitty_keyboard = s.kitty_keyboard;
            let before_semantic_prompt = (s.semantic_prompt.seen, s.semantic_prompt.click);
            let before_dirty = (s.dirty.selection, s.dirty.hyperlink_hover);
            let before_pages_first = s.pages.pages.first;
            let before_pages_last = s.pages.pages.last;
            let before_cols = s.pages.cols;
            let before_rows = s.pages.rows;
            let before_total_rows = s.pages.total_rows;
            let before_viewport = s.pages.viewport;
            let before_viewport_pin = unsafe { *s.pages.viewport_pin };
            let before_tracked_pins = s.pages.count_tracked_pins();
            let before_page: Vec<u8> = unsafe {
                let page: *const Page = crate::pagelist::Node::page(s.pages.pages.first);
                core::slice::from_raw_parts((*page).memory_ptr(), (*page).memory_len()).to_vec()
            };
            let before_kitty_dirty = s.kitty_images.dirty;

            resize_tw_error_always(tag);
            assert!(matches!(
                s.resize(Resize {
                    cols: 20,
                    rows: 4,
                    reflow: true,
                    prompt_redraw: osc::semantic_prompt::Redraw::True,
                }),
                Err(OutOfMemory)
            ));

            assert_eq!(before_cursor.x, s.cursor.x);
            assert_eq!(before_cursor.y, s.cursor.y);
            assert!(before_cursor.cursor_style == s.cursor.cursor_style);
            assert_eq!(before_cursor.pending_wrap, s.cursor.pending_wrap);
            assert_eq!(before_cursor.protected, s.cursor.protected);
            assert!(before_cursor.style.eql(&s.cursor.style));
            assert_eq!(before_cursor.style_id, s.cursor.style_id);
            assert_eq!(before_cursor.hyperlink_id, s.cursor.hyperlink_id);
            assert_eq!(
                before_cursor.hyperlink_implicit_id,
                s.cursor.hyperlink_implicit_id
            );
            assert_eq!(before_cursor.hyperlink, s.cursor.hyperlink);
            assert!(before_cursor.semantic_content == s.cursor.semantic_content);
            assert_eq!(
                before_cursor.semantic_content_clear_eol,
                s.cursor.semantic_content_clear_eol
            );
            assert!(std::ptr::eq(before_cursor.page_pin, s.cursor.page_pin));
            assert!(std::ptr::eq(before_cursor.page_row, s.cursor.page_row));
            assert!(std::ptr::eq(before_cursor.page_cell, s.cursor.page_cell));
            {
                let sc = s.saved_cursor.as_ref().unwrap();
                assert_eq!(
                    before_saved_cursor,
                    (sc.x, sc.y, sc.protected, sc.pending_wrap, sc.origin)
                );
                assert!(before_saved_style.eql(&sc.style));
                assert_eq!(
                    before_saved_charset,
                    (
                        sc.charset.gl,
                        sc.charset.gr,
                        sc.charset.single_shift,
                        sc.charset.charsets.g0,
                        sc.charset.charsets.g1,
                        sc.charset.charsets.g2,
                        sc.charset.charsets.g3,
                    )
                );
            }
            assert_eq!(before_selection_is_none, s.selection.is_none());
            assert_eq!(
                before_charset,
                (
                    s.charset.gl,
                    s.charset.gr,
                    s.charset.single_shift,
                    s.charset.charsets.g0,
                    s.charset.charsets.g1,
                    s.charset.charsets.g2,
                    s.charset.charsets.g3,
                )
            );
            assert!(before_protected_mode == s.protected_mode);
            assert_eq!(before_kitty_keyboard, s.kitty_keyboard);
            assert_eq!(
                before_semantic_prompt,
                (s.semantic_prompt.seen, s.semantic_prompt.click)
            );
            assert_eq!(before_dirty, (s.dirty.selection, s.dirty.hyperlink_hover));
            assert!(std::ptr::eq(before_pages_first, s.pages.pages.first));
            assert!(std::ptr::eq(before_pages_last, s.pages.pages.last));
            assert_eq!(before_cols, s.pages.cols);
            assert_eq!(before_rows, s.pages.rows);
            assert_eq!(before_total_rows, s.pages.total_rows);
            assert!(before_viewport == s.pages.viewport);
            assert!(before_viewport_pin.eql(unsafe { *s.pages.viewport_pin }));
            assert_eq!(before_tracked_pins, s.pages.count_tracked_pins());
            let after_page: Vec<u8> = unsafe {
                let page: *const Page = crate::pagelist::Node::page(s.pages.pages.first);
                core::slice::from_raw_parts((*page).memory_ptr(), (*page).memory_len()).to_vec()
            };
            // if (std.valgrind.runningOnValgrind() > 0) {
            //     // This assertion deliberately compares the complete raw page,
            //     // including semantically irrelevant struct padding.
            //     std.valgrind.memcheck.makeMemDefined(before_page);
            //     std.valgrind.memcheck.makeMemDefined(
            //         s.pages.pages.first.?.page().memory,
            //     );
            // }
            // (the Valgrind makeMemDefined calls have no Rust analogue — we
            // never run under Valgrind; the assertion below is that same
            // complete-raw-page compare, padding included.)
            assert!(
                before_page == after_page,
                "page memory changed after a failed resize"
            );
            if crate::options::KITTY_GRAPHICS {
                assert_eq!(before_kitty_dirty, s.kitty_images.dirty);
            }

            resize_tw_end(rustty_util::tripwire::ResetMode::Reset).unwrap();
            s.deinit();
        }
    }

    // test "Screen: resize cursor references when node survives"
    #[test]
    fn screen_resize_cursor_references_when_node_survives() {
        let mut s = mkscreen(5, 3, 1000);

        s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
        s.start_hyperlink(b"https://example.com/", Some(b"resize"))
            .unwrap();
        s.test_write_string(b"abc").unwrap();

        let original_node = unsafe { (*s.cursor.page_pin).node };
        let original_serial = unsafe { (*original_node).serial };
        {
            let page = unsafe { &(*crate::pagelist::Node::page(original_node)) };
            let base = page.memory_ptr();
            assert_eq!(4, unsafe { page.styles.ref_count(base, s.cursor.style_id) });
            assert_eq!(4, unsafe {
                page.hyperlink_set.ref_count(base, s.cursor.hyperlink_id)
            });
        }

        // A row-only resize grows the existing page without replacing the
        // cursor's node. The temporary resize references must be released from
        // this page after the cursor state is restored.
        s.resize(Resize {
            cols: 5,
            rows: 4,
            reflow: true,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();

        assert!(std::ptr::eq(original_node, unsafe {
            (*s.cursor.page_pin).node
        }));
        assert_eq!(original_serial, unsafe {
            (*(*s.cursor.page_pin).node).serial
        });
        {
            let page = unsafe { &(*crate::pagelist::Node::page((*s.cursor.page_pin).node)) };
            let base = page.memory_ptr();
            assert_eq!(4, unsafe { page.styles.ref_count(base, s.cursor.style_id) });
            assert_eq!(4, unsafe {
                page.hyperlink_set.ref_count(base, s.cursor.hyperlink_id)
            });
        }
        s.deinit();
    }

    // test "Screen: resize cursor references when node is replaced"
    #[test]
    fn screen_resize_cursor_references_when_node_is_replaced() {
        let mut s = mkscreen(5, 3, 1000);

        s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
        s.start_hyperlink(b"https://example.com/", Some(b"resize"))
            .unwrap();
        s.test_write_string(b"abc").unwrap();

        let original_node = unsafe { (*s.cursor.page_pin).node };
        let original_serial = unsafe { (*original_node).serial };
        {
            let page = unsafe { &(*crate::pagelist::Node::page(original_node)) };
            let base = page.memory_ptr();
            assert_eq!(4, unsafe { page.styles.ref_count(base, s.cursor.style_id) });
            assert_eq!(4, unsafe {
                page.hyperlink_set.ref_count(base, s.cursor.hyperlink_id)
            });
        }

        // A column resize with reflow replaces the page and remaps the tracked
        // cursor pin. The old page owns the temporary references, so destroying
        // it must account for them without attempting to release them afterward.
        s.resize(Resize {
            cols: 10,
            rows: 3,
            reflow: true,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();

        let live_node = unsafe { (*s.cursor.page_pin).node };
        assert!(
            !std::ptr::eq(live_node, original_node)
                || unsafe { (*live_node).serial } != original_serial
        );
        {
            let page = unsafe { &(*crate::pagelist::Node::page(live_node)) };
            let base = page.memory_ptr();
            assert_eq!(4, unsafe { page.styles.ref_count(base, s.cursor.style_id) });
            assert_eq!(4, unsafe {
                page.hyperlink_set.ref_count(base, s.cursor.hyperlink_id)
            });
        }
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

    // S134-F1: `clear_prompt_for_redraw`'s `Redraw::True` arm logs
    // `log.warn("cursor on prompt line but promptIterator found no prompt",
    // .{})` (Screen.zig:2264) when the cursor is FLAGGED semantic-prompt
    // content but no row is actually tagged with a `SemanticPrompt` marker
    // for `prompt_iterator` to find — the exact "cursor.semantic_content
    // says prompt, but the row's own tag was never set" mismatch the
    // function's own doc comment describes (some shells mark input areas but
    // not every continuation line). Reproduced directly, without going
    // through `cursor_set_semantic_content` (which sets BOTH fields
    // consistently and would never trigger this path).
    #[test]
    fn screen_resize_prompt_redraw_true_logs_when_no_prompt_row_is_tagged() {
        let mut s = mkscreen(20, 5, 5);
        s.test_write_string(b"line1").unwrap();

        // Flag the cursor as prompt content WITHOUT tagging any row's
        // `SemanticPrompt`, so `prompt_iterator` finds nothing.
        s.cursor.semantic_content = pagepkg::SemanticContent::Prompt;

        let cap = crate::log_capture::start();
        s.resize(Resize {
            cols: 30,
            rows: 5,
            reflow: true,
            prompt_redraw: osc::semantic_prompt::Redraw::True,
        })
        .unwrap();
        let messages = cap.messages();
        assert!(
            messages
                .iter()
                .any(|(level, target, msg)| *level == log::Level::Warn
                    && target == "screen"
                    && *msg == "cursor on prompt line but promptIterator found no prompt"),
            "expected the no-prompt-found diagnostic, got: {messages:?}"
        );
        drop(cap);

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
        s.selection_string(Allocator, SelectionString { sel, trim })
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

    // test "Screen: selectLine semantic boundary across mixed-width pages"
    #[test]
    fn screen_select_line_semantic_boundary_across_mixed_width_pages() {
        let mut s = mkscreen(4, 2, 0);

        s.cursor_set_semantic_content(SemanticContentArg::Input(
            SemanticContentInput::ClearExplicit,
        ));
        s.test_write_string(b"ABCD").unwrap();
        s.cursor_set_semantic_content(SemanticContentArg::Output);
        s.test_write_string(b"E").unwrap();

        let first = s.pages.pages.first;
        s.pages
            .split(Pin {
                node: first,
                x: 0,
                y: 1,
                garbage: false,
            })
            .unwrap();
        let second = unsafe { (*first).next };
        // PORT NOTE: `first.page().size.cols = 2` — a deliberate mid-reflow
        // narrow page, written directly through the node pointer.
        unsafe { (*crate::pagelist::Node::page(first)).size.cols = 2 };
        s.pages.pause_integrity_checks(true);

        assert_eq!(2 as size::CellCountInt, unsafe {
            crate::pagelist::Node::cols(first)
        });
        assert_eq!(4 as size::CellCountInt, unsafe {
            crate::pagelist::Node::cols(second)
        });

        let sel = s
            .select_line(SelectLine {
                pin: Pin {
                    node: first,
                    x: 1,
                    y: 0,
                    garbage: false,
                },
                whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                semantic_prompt_boundary: true,
            })
            .unwrap();
        assert!(Pin {
            node: first,
            x: 0,
            y: 0,
            garbage: false
        }
        .eql(sel.start()));
        assert!(Pin {
            node: first,
            x: 1,
            y: 0,
            garbage: false
        }
        .eql(sel.end()));
        sel.deinit(&mut s);

        s.pages.pause_integrity_checks(false);
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
            let cps = unsafe {
                (*crate::pagelist::Node::page(pin.node))
                    .lookup_grapheme(cell)
                    .unwrap()
            };
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

        let first_page_size = unsafe { crate::pagelist::Node::capacity(s.pages.pages.first).rows };

        // Lazy way to seek to the first page boundary.
        unsafe { (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(true) };
        for _ in 0..(first_page_size - 1) {
            s.test_write_string(b"\n").unwrap();
        }
        unsafe {
            (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(false)
        };

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
    // `try init(alloc, .{ .cols, .rows, .max_scrollback_bytes })`. The kitty image
    // limit fields are not exercised by these tests (kitty graphics is deferred
    // to vt-12); they take the same defaults used by every other test in this
    // module.
    fn clone_clear_init(
        cols: size::CellCountInt,
        rows: size::CellCountInt,
        max_scrollback_bytes: usize,
    ) -> Screen {
        Screen::init(
            Allocator,
            Options {
                cols,
                rows,
                max_scrollback_bytes: Some(max_scrollback_bytes),
                max_scrollback_lines: None,
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

    // test "Screen: clone clamps clipped selections to mixed-width pages"
    #[test]
    fn screen_clone_clamps_clipped_selections_to_mixed_width_pages() {
        let mut s = mkscreen(4, 3, 0);

        let first = s.pages.pages.first;
        s.pages
            .split(Pin {
                node: first,
                x: 0,
                y: 2,
                garbage: false,
            })
            .unwrap();
        s.pages
            .split(Pin {
                node: first,
                x: 0,
                y: 1,
                garbage: false,
            })
            .unwrap();
        let middle = unsafe { (*first).next };
        let last = unsafe { (*middle).next };
        // PORT NOTE: `middle.page().size.cols = 2` — a deliberate mid-reflow
        // narrow page, written directly through the node pointer.
        unsafe { (*crate::pagelist::Node::page(middle)).size.cols = 2 };

        s.select(Some(Selection::init(
            Pin {
                node: first,
                x: 0,
                y: 0,
                garbage: false,
            },
            Pin {
                node: last,
                x: 3,
                y: 0,
                garbage: false,
            },
            false,
        )))
        .unwrap();
        let mut linear = s
            .clone(
                Allocator,
                point::Point::Screen(point::Coordinate { x: 0, y: 0 }),
                Some(point::Point::Screen(point::Coordinate { x: 0, y: 1 })),
            )
            .unwrap();
        let linear_end = linear.selection.unwrap().end();
        let _ = linear_end.row_and_cell();
        assert_eq!(1 as size::CellCountInt, linear_end.x);
        linear.deinit();

        s.select(Some(Selection::init(
            Pin {
                node: first,
                x: 3,
                y: 0,
                garbage: false,
            },
            Pin {
                node: last,
                x: 3,
                y: 0,
                garbage: false,
            },
            true,
        )))
        .unwrap();
        let mut rectangle = s
            .clone(
                Allocator,
                point::Point::Screen(point::Coordinate { x: 0, y: 1 }),
                None,
            )
            .unwrap();
        let rectangle_start = rectangle.selection.unwrap().start();
        let _ = rectangle_start.row_and_cell();
        assert_eq!(1 as size::CellCountInt, rectangle_start.x);
        rectangle.deinit();

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
        let page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
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

    // PORT NOTE (parity S133-F2): no reference test. Pins that
    // `Options.default` carries the artifact field default for the kitty image
    // storage limit (320MB for `.ghostty`) and that `Screen.init` hands it to
    // the image storage, rather than a zero-byte budget.
    #[test]
    fn screen_options_default_kitty_image_storage_limit() {
        use crate::build_options::Artifact;
        assert_eq!(
            Options::kitty_image_storage_limit_default(Artifact::Ghostty),
            320 * 1000 * 1000
        );
        assert_eq!(
            Options::kitty_image_storage_limit_default(Artifact::Lib),
            10 * 1000 * 1000
        );
        assert_eq!(crate::options::ARTIFACT, Artifact::Ghostty);
        assert_eq!(
            Options::default().kitty_image_storage_limit,
            320 * 1000 * 1000
        );
        let mut s = Screen::init(Allocator, Options::default()).unwrap();
        if crate::options::KITTY_GRAPHICS {
            assert_eq!(s.kitty_images.total_limit, 320 * 1000 * 1000);
        }
        s.deinit();
    }

    // test "Screen clearCells empty range"
    // PORT NOTE (parity 0cb004734): test hunk of the upstream empty-cell-clear
    // guard in clearCells.
    #[test]
    fn screen_clear_cells_empty_range() {
        // var s = try Screen.init(testing.allocator, .default);
        // defer s.deinit();
        let mut s = Screen::init(Allocator, Options::default()).unwrap();

        // const page = s.cursor.page_pin.node.page();
        let page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
        // const row = s.cursor.page_row;
        let row: *mut Row = s.cursor.page_row;
        // const cells = page.getCells(row);
        let cells: *mut [Cell] = unsafe { (*page).get_cells(row) };
        // s.clearCells(page, row, cells[0..0]);
        let empty: *mut [Cell] = core::ptr::slice_from_raw_parts_mut(cells as *mut Cell, 0);
        s.clear_cells(page, row, empty);

        s.deinit();
    }

    // -- P3-W1 T4 step 2: stored page widths (oracle e727a3658, a55850c98) ----

    // ref: test "Screen clearRows uses stored page width"
    #[test]
    fn screen_clear_rows_uses_stored_page_width() {
        let mut s = Screen::init(
            Allocator,
            Options {
                cols: 2,
                rows: 1,
                max_scrollback_bytes: Some(0),
                max_scrollback_lines: None,
                ..Options::default()
            },
        )
        .unwrap();

        s.test_write_string(b"AB").unwrap();
        let node = s.pages.pages.first;
        // Model an incomplete reflow: the DESIRED width is now wider than the
        // width the stored page actually has.
        s.pages.cols = 4;

        s.clear_rows(
            point::Point::Screen(point::Coordinate { x: 0, y: 0 }),
            None,
            false,
        );

        let page: *mut Page = unsafe { crate::pagelist::Node::page(node) };
        let row0: *mut Row = unsafe { (*page).rows.ptr((*page).memory_ptr()) };
        let cells: *mut [Cell] = unsafe { (*page).get_cells(row0) };
        assert_eq!(2usize, cells.len());
        for i in 0..cells.len() {
            let cell = unsafe { (cells as *mut Cell).add(i) };
            assert!(unsafe { (*cell).is_empty() });
        }

        // Restore the real width before deinit.
        s.pages.cols = 2;
        s.deinit();
    }

    // ref: test "Screen: cursorCellEndOfPrev across mixed-width pages"
    #[test]
    fn screen_cursor_cell_end_of_prev_across_mixed_width_pages() {
        let mut s = Screen::init(
            Allocator,
            Options {
                cols: 4,
                rows: 2,
                max_scrollback_bytes: Some(0),
                max_scrollback_lines: None,
                ..Options::default()
            },
        )
        .unwrap();

        s.test_write_string(b"ABCDE").unwrap();
        let first = s.pages.pages.first;
        s.pages
            .split(crate::pagelist::Pin {
                node: first,
                y: 1,
                x: 0,
                garbage: false,
            })
            .unwrap();
        s.cursor_reload();
        let second = unsafe { (*first).next };
        // Model an incomplete reflow: the stored first page is narrower than
        // the desired screen width.
        s.pages.pause_integrity_checks(true);
        unsafe { (*crate::pagelist::Node::page(first)).size.cols = 2 };

        assert!(core::ptr::eq(second, unsafe { (*s.cursor.page_pin).node }));
        let expected = crate::pagelist::Pin {
            node: first,
            y: 0,
            x: 1,
            garbage: false,
        }
        .row_and_cell()
        .cell;
        assert!(core::ptr::eq(expected, s.cursor_cell_end_of_prev()));

        unsafe { (*crate::pagelist::Node::page(first)).size.cols = 4 };
        s.pages.pause_integrity_checks(false);
        s.deinit();
    }

    /// `e727a3658`'s SECOND hunk — `clear_cells` comparing `cells.len` against
    /// `page.size.cols` instead of `self.pages.cols` — is not covered by the
    /// oracle's own `clearRows` test (there, clearing the whole narrow row makes
    /// both branches agree, because `update_row_*_flag` also scans
    /// `page.size.cols`). The observable divergence is in the OTHER reflow
    /// direction: a stored page WIDER than the desired width. Then a partial
    /// clear whose length happens to equal the DESIRED width takes the
    /// "we cleared everything" fast path and drops the row's `styled` flag while
    /// styled cells still exist beyond the desired width — corrupting the row
    /// flags rather than tripping an assert.
    #[test]
    fn screen_clear_cells_uses_stored_page_width_for_row_flags() {
        let mut s = Screen::init(
            Allocator,
            Options {
                cols: 4,
                rows: 1,
                max_scrollback_bytes: Some(0),
                max_scrollback_lines: None,
                ..Options::default()
            },
        )
        .unwrap();

        s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
        s.test_write_string(b"ABCD").unwrap();

        let page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
        let row: *mut Row = unsafe { (*page).rows.ptr((*page).memory_ptr()) };
        assert!(unsafe { (*row).styled() }, "the row starts styled");
        assert_eq!(4, unsafe { (*page).size.cols });

        // Park the cursor inside the narrower desired width so the screen's own
        // integrity assertions stay satisfied for the modelled state.
        s.cursor_horizontal_absolute(0);

        // Model an incomplete reflow the other way: the DESIRED width is now
        // NARROWER than the stored page.
        s.pages.cols = 2;

        // Clear exactly the first two cells — a partial clear whose length
        // equals the desired width but NOT the stored width.
        let cells: *mut [Cell] = unsafe { (*page).get_cells(row) };
        let partial: *mut [Cell] = core::ptr::slice_from_raw_parts_mut(cells as *mut Cell, 2);
        s.clear_cells(page, row, partial);

        assert!(
            unsafe { (*row).styled() },
            "cells 2 and 3 are still styled, so the row flag must stay set; \
             comparing cells.len against the DESIRED width drops it"
        );

        s.pages.cols = 4;
        s.deinit();
    }

    // ref: test "Screen write regrows compacted page capacity"
    #[test]
    fn screen_write_regrows_compacted_page_capacity() {
        let mut s = Screen::init(
            Allocator,
            Options {
                cols: 80,
                rows: 24,
                max_scrollback_bytes: Some(0),
                max_scrollback_lines: None,
                ..Options::default()
            },
        )
        .unwrap();

        // Compact the active page so every managed capacity dimension is
        // zero, then reload the cursor since its cached row/cell pointers
        // point into the replaced page.
        {
            let node = s
                .pages
                .compact(unsafe { (*s.cursor.page_pin).node })
                .unwrap()
                .unwrap();
            let cap = unsafe { crate::pagelist::Node::capacity(node) };
            assert_eq!(0, cap.styles);
            assert_eq!(0, cap.grapheme_bytes);
            assert_eq!(0, cap.string_bytes);
            assert_eq!(0, cap.hyperlink_bytes);
            s.cursor_reload();
        }

        // Styled write: exercises the manual_style_update single-retry
        // path. Prior to increase_capacity handling zero dimensions, the
        // retry would fail and the style would be dropped.
        s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
        s.test_write_string(b"A").unwrap();

        // Grapheme write: exercises the append_grapheme single-retry path.
        // We can't use test_write_string here because it appends graphemes
        // directly on the page without the capacity retry.
        s.test_write_string(b"a").unwrap();
        let prev = s.cursor_cell_left(1);
        s.append_grapheme(prev, 0x0301).unwrap();

        // Hyperlink: exercises the start_hyperlink retry loop, which used
        // to loop forever when capacity growth from zero didn't grow.
        s.start_hyperlink(b"https://example.com/", None).unwrap();
        s.test_write_string(b"B").unwrap();
        s.end_hyperlink();

        // Verify the content landed on the page.
        let page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
        assert!(unsafe { (*page).styles.count() } >= 1);
        assert!(unsafe { (*page).hyperlink_set.count() } >= 1);
        assert!(unsafe { (*page).grapheme_count() } >= 1);

        s.deinit();
    }

    /// `P3-W1 T6` step 4 (oracle 8d663a76e). A "no panic" test is blind to a
    /// DOUBLE release, so this asserts the ref count reaches EXACTLY zero, from
    /// an exactly-known starting value, for both the uniform-run case (one
    /// `release_multiple` call) and the alternating case (which degrades to one
    /// call per cell and is where an off-by-one run length would show up).
    #[test]
    fn screen_clear_cells_releases_styles_per_run() {
        let mut s = Screen::init(
            Allocator,
            Options {
                cols: 8,
                rows: 1,
                max_scrollback_bytes: Some(0),
                max_scrollback_lines: None,
                ..Options::default()
            },
        )
        .unwrap();

        // One uniform run of four styled cells.
        s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
        s.test_write_string(b"AAAA").unwrap();
        s.set_attribute(crate::sgr::Attribute::Unset).unwrap();

        let page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
        let row: *mut Row = unsafe { (*page).rows.ptr((*page).memory_ptr()) };
        let cells: *mut [Cell] = unsafe { (*page).get_cells(row) };
        let base = unsafe { (*page).memory_ptr() };

        let id = unsafe { (*(cells as *mut Cell)).style_id() };
        assert_ne!(
            crate::style::DEFAULT_ID,
            id,
            "the first cell must be styled"
        );
        assert_eq!(
            4,
            usize::from(unsafe { (*page).styles.ref_count(base, id) })
        );

        // Clear exactly the four styled cells.
        let run: *mut [Cell] = core::ptr::slice_from_raw_parts_mut(cells as *mut Cell, 4);
        s.clear_cells(page, row, run);

        assert_eq!(
            0,
            usize::from(unsafe { (*page).styles.ref_count(base, id) }),
            "one release_multiple per run must land the ref count on exactly \
             zero -- not below it (double release) and not above (short run)"
        );

        s.deinit();
    }

    /// Companion to the run test: ALTERNATING styles, where every run has
    /// length one. This is the case an off-by-one in the run scan would break
    /// while the uniform case still passed.
    #[test]
    fn screen_clear_cells_releases_alternating_styles_exactly_once_each() {
        let mut s = Screen::init(
            Allocator,
            Options {
                cols: 8,
                rows: 1,
                max_scrollback_bytes: Some(0),
                max_scrollback_lines: None,
                ..Options::default()
            },
        )
        .unwrap();

        s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
        s.test_write_string(b"A").unwrap();
        s.set_attribute(crate::sgr::Attribute::Unset).unwrap();
        s.set_attribute(crate::sgr::Attribute::Italic).unwrap();
        s.test_write_string(b"B").unwrap();
        s.set_attribute(crate::sgr::Attribute::Unset).unwrap();
        s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
        s.test_write_string(b"C").unwrap();
        s.set_attribute(crate::sgr::Attribute::Unset).unwrap();

        let page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
        let row: *mut Row = unsafe { (*page).rows.ptr((*page).memory_ptr()) };
        let cells: *mut [Cell] = unsafe { (*page).get_cells(row) };
        let base = unsafe { (*page).memory_ptr() };
        let cells_base = cells as *mut Cell;

        let bold_id = unsafe { (*cells_base).style_id() };
        let italic_id = unsafe { (*cells_base.add(1)).style_id() };
        assert_ne!(bold_id, italic_id);
        assert_eq!(bold_id, unsafe { (*cells_base.add(2)).style_id() });
        assert_eq!(
            2,
            usize::from(unsafe { (*page).styles.ref_count(base, bold_id) })
        );
        assert_eq!(
            1,
            usize::from(unsafe { (*page).styles.ref_count(base, italic_id) })
        );

        let run: *mut [Cell] = core::ptr::slice_from_raw_parts_mut(cells_base, 3);
        s.clear_cells(page, row, run);

        assert_eq!(
            0,
            usize::from(unsafe { (*page).styles.ref_count(base, bold_id) })
        );
        assert_eq!(
            0,
            usize::from(unsafe { (*page).styles.ref_count(base, italic_id) })
        );

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

    // PORT PARITY GUARD (no Zig twin — it needs none): Zig's
    // `tripwire.zig: pub const enabled = builtin.is_test` is true for every test
    // build at EVERY optimize mode, so the reference's allocation-failure tests
    // inject in `zig build test -Doptimize=ReleaseFast` exactly as in Debug. This
    // crate reproduces that cross-crate by enabling `rustty-util/tripwire-test` as
    // a DEV-dependency, which is what keeps the fail-point tests in this module
    // out of the release lane's `ignored` bucket. Asserting it directly means the
    // wiring cannot be silently dropped (a `cargo test --release` run would
    // otherwise just report the tests as "ignored" and stay green).
    //
    // (f1948d505 removed the `selectionString_tw` module this guard originally
    // named, and parity tail S7-3 removed the `StartHyperlinkFp` stand-in that
    // replaced it; PageList's `init_tw` is the same idiom and shares the wiring.)
    #[test]
    fn tripwire_fail_points_enabled_in_test_builds_at_any_profile() {
        assert!(
            rustty_util::tripwire::Module::<crate::pagelist::InitFp, OutOfMemory>::enabled(),
            "tripwire fail points must be armed in ALL test builds (Zig: \
             `enabled = builtin.is_test`); if this fires in `--release`, the \
             `rustty-util/tripwire-test` dev-dependency feature was lost"
        );
    }

    // PORT NOTE (f1948d505): the `test "selectionString map allocation failure
    // cleanup"` that lived here was DELETED upstream with the tripwire it armed —
    // `selectionStringMap` now transfers the formatter output into the returned
    // map, so the duplicate string the test guarded against no longer exists.

    // PORT NOTE: these tests were DEFERRED from vt-11 (recorded in
    // DEFERRALS.md) because their fixtures needed Selection / the dirty
    // tracking / page-churn machinery that landed across later sub-batches.
    // They are ported here (vt-12c-screen) assertion-for-assertion.

    // test "Screen: scrolling"
    #[test]
    fn screen_scrolling() {
        let mut s = mkscreen(10, 3, 0);
        s.set_attribute(crate::sgr::Attribute::DirectColorBg(crate::color::RGB {
            r: 155,
            g: 0,
            b: 0,
        }))
        .unwrap();
        s.test_write_string(b"1ABCD\n2EFGH\n3IJKL").unwrap();

        // Scroll down, should still be bottom
        s.cursor_down_scroll().unwrap();
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"2EFGH\n3IJKL");
        }
        {
            let list_cell = s
                .pages
                .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 2 }))
                .unwrap();
            let cell = list_cell.cell;
            assert!(unsafe { (*cell).content_tag() } == pagepkg::ContentTag::BgColorRgb);
            assert_eq!(pagepkg::CellRgb { r: 155, g: 0, b: 0 }, unsafe {
                (*cell).content_color_rgb()
            });
        }

        // Everything is dirty because we have no scrollback
        assert!(s
            .pages
            .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 0 })));
        assert!(s
            .pages
            .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 1 })));
        assert!(s
            .pages
            .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 2 })));

        // Scrolling to the bottom does nothing
        s.scroll(Scroll::Active);

        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"2EFGH\n3IJKL");
        }
        s.deinit();
    }

    // test "Screen: scrolling with a single-row screen no scrollback"
    #[test]
    fn screen_scrolling_with_a_single_row_screen_no_scrollback() {
        let mut s = mkscreen(10, 1, 0);
        s.test_write_string(b"1ABCD").unwrap();

        // Scroll down, should still be bottom
        s.cursor_down_scroll().unwrap();
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"");
        }

        // Screen should be dirty
        assert!(s
            .pages
            .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 0 })));
        s.deinit();
    }

    // test "Screen: scrolling across pages preserves style"
    #[test]
    fn screen_scrolling_across_pages_preserves_style() {
        let mut s = mkscreen(10, 3, 1);
        s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
        s.test_write_string(b"1ABCD\n2EFGH\n3IJKL").unwrap();
        let start_page: *mut Page = unsafe { crate::pagelist::Node::page(s.pages.pages.last) };

        // Scroll down enough to go to another page
        let rem = unsafe { (*start_page).capacity.rows - (*start_page).size.rows + 1 };
        unsafe { (*start_page).pause_integrity_checks(true) };
        for _ in 0..rem {
            s.cursor_down_or_scroll().unwrap();
        }
        unsafe { (*start_page).pause_integrity_checks(false) };

        // We need our page to change for this test o make sense. If this
        // assertion fails then the bug is in the test: we should be scrolling
        // above enough for a new page to show up.
        let page: *mut Page = unsafe { crate::pagelist::Node::page(s.pages.pages.last) };
        assert!(!std::ptr::eq(start_page, page));

        let styleval = unsafe {
            let base = (*page).memory_ptr();
            &*(*page).styles.get(base, s.cursor.style_id)
        };
        assert!(styleval.flags.bold);
        s.deinit();
    }

    // test "Screen: scrolling with a single-row screen with scrollback"
    #[test]
    fn screen_scrolling_with_a_single_row_screen_with_scrollback() {
        let mut s = mkscreen(10, 1, 1);
        s.test_write_string(b"1ABCD").unwrap();

        // Scroll down, should still be bottom
        s.cursor_down_scroll().unwrap();
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"");
        }

        // Active should be dirty
        assert!(s
            .pages
            .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 0 })));

        // Scrollback also dirty because cursor moved from there
        assert!(s
            .pages
            .is_dirty(point::Point::Screen(point::Coordinate { x: 0, y: 0 })));

        s.scroll(Scroll::DeltaRow(-1));
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"1ABCD");
        }
        s.deinit();
    }

    // test "Screen: scroll down from 0"
    #[test]
    fn screen_scroll_down_from_0() {
        let mut s = mkscreen(10, 3, 0);
        s.test_write_string(b"1ABCD\n2EFGH\n3IJKL").unwrap();

        // Scrolling up does nothing, but allows it
        s.scroll(Scroll::DeltaRow(-1));
        assert!(s.pages.viewport == pagelist::Viewport::Active);

        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"1ABCD\n2EFGH\n3IJKL");
        }
        s.deinit();
    }

    // test "Screen: scrollback various cases"
    #[test]
    fn screen_scrollback_various_cases() {
        let mut s = mkscreen(10, 3, 1);
        s.test_write_string(b"1ABCD\n2EFGH\n3IJKL").unwrap();
        s.cursor_down_scroll().unwrap();

        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"2EFGH\n3IJKL");
        }

        // Scrolling to the bottom
        s.scroll(Scroll::Active);
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"2EFGH\n3IJKL");
        }

        // Scrolling back should make it visible again
        s.scroll(Scroll::DeltaRow(-1));
        assert!(s.pages.viewport != pagelist::Viewport::Active);
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"1ABCD\n2EFGH\n3IJKL");
        }

        // Scrolling back again should do nothing
        s.scroll(Scroll::DeltaRow(-1));
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"1ABCD\n2EFGH\n3IJKL");
        }

        // Scrolling to the bottom
        s.scroll(Scroll::Active);
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"2EFGH\n3IJKL");
        }

        // Scrolling forward with no grow should do nothing
        s.scroll(Scroll::DeltaRow(1));
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"2EFGH\n3IJKL");
        }

        // Scrolling to the top should work
        s.scroll(Scroll::Top);
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"1ABCD\n2EFGH\n3IJKL");
        }

        // Should be able to easily clear active area only
        s.clear_rows(act(), None, false);
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"1ABCD");
        }

        // Scrolling to the bottom
        s.scroll(Scroll::Active);
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"");
        }
        s.deinit();
    }

    // test "Screen: scrollback with multi-row delta"
    #[test]
    fn screen_scrollback_with_multi_row_delta() {
        let mut s = mkscreen(10, 3, 3);
        s.test_write_string(b"1ABCD\n2EFGH\n3IJKL\n4ABCD\n5EFGH\n6IJKL")
            .unwrap();

        // Scroll to top
        s.scroll(Scroll::Top);
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"1ABCD\n2EFGH\n3IJKL");
        }

        // Scroll down multiple
        s.scroll(Scroll::DeltaRow(5));
        assert!(s.pages.viewport == pagelist::Viewport::Active);
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"4ABCD\n5EFGH\n6IJKL");
        }
        s.deinit();
    }

    // test "Screen: scrollback empty"
    #[test]
    fn screen_scrollback_empty() {
        let mut s = mkscreen(10, 3, 50);
        s.test_write_string(b"1ABCD\n2EFGH\n3IJKL").unwrap();
        s.scroll(Scroll::DeltaRow(1));
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"1ABCD\n2EFGH\n3IJKL");
        }
        s.deinit();
    }

    // test "Screen: scrollback doesn't move viewport if not at bottom"
    #[test]
    fn screen_scrollback_doesnt_move_viewport_if_not_at_bottom() {
        let mut s = mkscreen(10, 3, 3);
        s.test_write_string(b"1ABCD\n2EFGH\n3IJKL\n4ABCD\n5EFGH")
            .unwrap();

        // First test: we scroll up by 1, so we're not at the bottom anymore.
        s.scroll(Scroll::DeltaRow(-1));
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"2EFGH\n3IJKL\n4ABCD");
        }

        // Next, we scroll back down by 1, this grows the scrollback but we
        // shouldn't move.
        s.cursor_down_scroll().unwrap();
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"2EFGH\n3IJKL\n4ABCD");
        }

        // Scroll again, this clears scrollback so we should move viewports
        // but still see the same thing since our original view fits.
        s.cursor_down_scroll().unwrap();
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"2EFGH\n3IJKL\n4ABCD");
        }
        s.deinit();
    }

    // test "Screen: scrolling when viewport is pruned"
    #[cfg_attr(
        all(miri, debug_assertions),
        ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
    )]
    #[test]
    fn screen_scrolling_when_viewport_is_pruned() {
        let mut s = mkscreen(215, 3, 1);

        // Write some to create scrollback and move back into our scrollback.
        s.test_write_string(b"1ABCD\n2EFGH\n3IJKL\n").unwrap();
        s.test_write_string(b"1ABCD\n2EFGH\n3IJKL").unwrap();
        s.scroll(Scroll::DeltaRow(-2));

        // Our viewport is now somewhere pinned. Create so much scrollback
        // that we prune it.
        s.test_write_string(b"\n").unwrap();
        for _ in 0..1000 {
            s.test_write_string(b"1ABCD\n2EFGH\n3IJKL\n").unwrap();
        }
        s.test_write_string(b"1ABCD\n2EFGH\n3IJKL").unwrap();

        {
            assert_eq!(
                point::Point::Screen(point::Coordinate { x: 0, y: 0 }),
                s.pages
                    .point_from_pin(
                        point::Tag::Screen,
                        s.pages.get_top_left(point::Tag::Viewport)
                    )
                    .unwrap()
            );
        }
        s.deinit();
    }

    // test "Screen: scrolling moves selection"
    #[test]
    fn screen_scrolling_moves_selection() {
        let mut s = mkscreen(5, 3, 1);
        s.test_write_string(b"1ABCD\n2EFGH\n3IJKL").unwrap();

        // Select a single line
        s.select(Some(Selection::init(
            s.pages
                .pin(point::Point::Active(point::Coordinate { x: 0, y: 1 }))
                .unwrap(),
            s.pages
                .pin(point::Point::Active(point::Coordinate {
                    x: s.pages.cols - 1,
                    y: 1,
                }))
                .unwrap(),
            false,
        )))
        .unwrap();

        // Scroll down, should still be bottom
        s.cursor_down_scroll().unwrap();

        // Our selection should've moved up
        {
            let sel = s.selection.unwrap();
            assert_eq!(
                point::Point::Active(point::Coordinate { x: 0, y: 0 }),
                s.pages
                    .point_from_pin(point::Tag::Active, sel.start())
                    .unwrap()
            );
            assert_eq!(
                point::Point::Active(point::Coordinate {
                    x: s.pages.cols - 1,
                    y: 0,
                }),
                s.pages
                    .point_from_pin(point::Tag::Active, sel.end())
                    .unwrap()
            );
        }

        {
            // Test our contents rotated
            let contents = dump(&s, vp());
            assert_eq!(contents, b"2EFGH\n3IJKL");
        }

        // Scrolling to the bottom does nothing
        s.scroll(Scroll::Active);

        // Our selection should've stayed the same
        {
            let sel = s.selection.unwrap();
            assert_eq!(
                point::Point::Active(point::Coordinate { x: 0, y: 0 }),
                s.pages
                    .point_from_pin(point::Tag::Active, sel.start())
                    .unwrap()
            );
            assert_eq!(
                point::Point::Active(point::Coordinate {
                    x: s.pages.cols - 1,
                    y: 0,
                }),
                s.pages
                    .point_from_pin(point::Tag::Active, sel.end())
                    .unwrap()
            );
        }

        {
            // Test our contents rotated
            let contents = dump(&s, vp());
            assert_eq!(contents, b"2EFGH\n3IJKL");
        }

        // Scroll up again
        s.cursor_down_scroll().unwrap();

        {
            // Test our contents rotated
            let contents = dump(&s, vp());
            assert_eq!(contents, b"3IJKL");
        }

        // Our selection should be null because it left the screen.
        {
            let sel = s.selection.unwrap();
            assert!(s
                .pages
                .point_from_pin(point::Tag::Active, sel.start())
                .is_none());
            assert!(s
                .pages
                .point_from_pin(point::Tag::Active, sel.end())
                .is_none());
        }
        s.deinit();
    }

    // -- oracle 77190bd02: Screen.cursorScrollRegionUp (`P3-W5 T3`) ----------

    // test "Screen: cursorScrollRegionUp simple"
    #[test]
    fn screen_cursor_scroll_region_up_simple() {
        let mut s = mkscreen(5, 5, 0);
        s.test_write_string(b"1ABCD\n2EFGH\n3IJKL\n4MNOP\n5QRST")
            .unwrap();

        // Scroll a region ending at row 2 (zero-indexed) up by one. This
        // emulates a scroll region of rows 0-2 with the cursor at the
        // region bottom.
        s.cursor_absolute(1, 2);
        s.cursor_scroll_region_up(2).unwrap();

        // The cursor stays in place, on the new blank row.
        assert_eq!(1 as size::CellCountInt, s.cursor.x);
        assert_eq!(2 as size::CellCountInt, s.cursor.y);

        // Rows in the region scrolled, rows below are unchanged, and
        // nothing was moved into scrollback.
        {
            let contents = dump(&s, scr());
            assert_eq!(contents, b"2EFGH\n3IJKL\n\n4MNOP\n5QRST");
        }
        s.deinit();
    }

    // test "Screen: cursorScrollRegionUp renews page generation"
    //
    // PORTS: oracle 0bad91adb — the in-place region rotation renews the page
    // generation so cached row coordinates cannot pass `node_is_valid`.
    #[test]
    fn screen_cursor_scroll_region_up_renews_page_generation() {
        let mut s = mkscreen(5, 5, 0);
        s.test_write_string(b"1ABCD\n2EFGH\n3IJKL\n4MNOP\n5QRST")
            .unwrap();
        s.cursor_absolute(0, 2);

        let node = unsafe { (*s.cursor.page_pin).node };
        let serial = unsafe { (*node).serial };
        s.cursor_scroll_region_up(2).unwrap();

        assert!(!s.pages.node_is_valid(node, serial));
        s.deinit();
    }

    // test "Screen: cursorScrollRegionUp moves selection"
    #[test]
    fn screen_cursor_scroll_region_up_moves_selection() {
        let mut s = mkscreen(5, 5, 0);
        s.test_write_string(b"1ABCD\n2EFGH\n3IJKL\n4MNOP\n5QRST")
            .unwrap();

        // Select the second row.
        s.select(Some(Selection::init(
            s.pages
                .pin(point::Point::Active(point::Coordinate { x: 0, y: 1 }))
                .unwrap(),
            s.pages
                .pin(point::Point::Active(point::Coordinate {
                    x: s.pages.cols - 1,
                    y: 1,
                }))
                .unwrap(),
            false,
        )))
        .unwrap();

        s.cursor_absolute(0, 2);
        s.cursor_scroll_region_up(2).unwrap();

        // Our selection should've moved up with its row.
        {
            let sel = s.selection.unwrap();
            assert_eq!(
                point::Point::Active(point::Coordinate { x: 0, y: 0 }),
                s.pages
                    .point_from_pin(point::Tag::Active, sel.start())
                    .unwrap()
            );
            assert_eq!(
                point::Point::Active(point::Coordinate {
                    x: s.pages.cols - 1,
                    y: 0,
                }),
                s.pages
                    .point_from_pin(point::Tag::Active, sel.end())
                    .unwrap()
            );
        }

        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"2EFGH\n3IJKL\n\n4MNOP\n5QRST");
        }
        s.deinit();
    }

    // test "Screen: cursorScrollRegionUp region spans pages"
    #[test]
    fn screen_cursor_scroll_region_up_region_spans_pages() {
        let mut s = mkscreen(10, 5, 10);

        // We need to get the cursor to a new page
        let first_page_size = unsafe { crate::pagelist::Node::capacity(s.pages.pages.first).rows };
        unsafe { (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(true) };
        for _ in 0..(first_page_size - 3) {
            s.test_write_string(b"\n").unwrap();
        }
        unsafe {
            (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(false)
        };
        s.test_write_string(b"1A\n2B\n3C\n4D\n5E").unwrap();

        // At this point:
        //      +----------+ = PAGE 0
        //  ... :          :
        //     +-------------+ ACTIVE
        // 4305 |1A00000000| | 0
        // 4306 |2B00000000| | 1
        // 4307 |3C00000000| | 2
        //      +----------+ :
        //      +----------+ : = PAGE 1
        //    0 |4D00000000| | 3
        //      :^         : : = PIN 0
        //    1 |5E00000000| | 4
        //      +----------+ :
        //     +-------------+

        // Move the cursor to the first row of the second page and give it
        // a non-default style. This is important: it verifies that the
        // cursor's style ref stays accounted on the correct page even
        // though eraseRowBounded moves the cursor's tracked pin across
        // the page boundary.
        s.cursor_absolute(0, 3);
        s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
        assert!(std::ptr::eq(
            unsafe { (*s.cursor.page_pin).node },
            s.pages.pages.last
        ));
        assert_eq!(0 as size::CellCountInt, unsafe { (*s.cursor.page_pin).y });

        // Scroll a region of active rows 1-3 with the cursor at the region
        // bottom. The region spans the page boundary so this exercises the
        // slow path.
        s.cursor_scroll_region_up(2).unwrap();

        // The cursor stays in place, on the new blank row.
        assert_eq!(0 as size::CellCountInt, s.cursor.x);
        assert_eq!(3 as size::CellCountInt, s.cursor.y);

        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"1A\n3C\n4D\n\n5E");
        }

        // Our cursor style must remain usable: write a styled cell and
        // verify the style ref counting is intact on the cursor's page.
        s.test_write_string(b"X").unwrap();
        {
            let page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
            assert_eq!(1usize, unsafe { (*page).styles.count() });
        }
        s.deinit();
    }

    // test "Screen: cursorScrollRegionUp region spans pages with background SGR"
    #[test]
    fn screen_cursor_scroll_region_up_region_spans_pages_with_background_sgr() {
        let mut s = mkscreen(10, 5, 10);

        // We need to get the cursor to a new page. See the previous test
        // for a diagram of the page layout.
        let first_page_size = unsafe { crate::pagelist::Node::capacity(s.pages.pages.first).rows };
        unsafe { (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(true) };
        for _ in 0..(first_page_size - 3) {
            s.test_write_string(b"\n").unwrap();
        }
        unsafe {
            (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(false)
        };
        s.test_write_string(b"1A\n2B\n3C\n4D\n5E").unwrap();

        s.cursor_absolute(0, 3);
        s.set_attribute(crate::sgr::Attribute::DirectColorBg(crate::color::RGB {
            r: 0xFF,
            g: 0,
            b: 0,
        }))
        .unwrap();
        assert!(std::ptr::eq(
            unsafe { (*s.cursor.page_pin).node },
            s.pages.pages.last
        ));
        assert_eq!(0 as size::CellCountInt, unsafe { (*s.cursor.page_pin).y });

        s.cursor_scroll_region_up(2).unwrap();

        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"1A\n3C\n4D\n\n5E");
        }

        // The new blank row must be filled with our background color.
        for x in 0..s.pages.cols {
            let list_cell = s
                .pages
                .get_cell(point::Point::Active(point::Coordinate { x, y: 3 }))
                .unwrap();
            let cell = list_cell.cell;
            assert!(unsafe { (*cell).content_tag() } == pagepkg::ContentTag::BgColorRgb);
            assert_eq!(
                pagepkg::CellRgb {
                    r: 0xFF,
                    g: 0,
                    b: 0
                },
                unsafe { (*cell).content_color_rgb() }
            );
        }
        s.deinit();
    }

    // test "Screen: cursorScrollRegionUp with styled erased row"
    #[test]
    fn screen_cursor_scroll_region_up_with_styled_erased_row() {
        let mut s = mkscreen(5, 3, 0);

        // Write a styled row at the top so the erased row has managed
        // memory that must be released.
        s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
        s.test_write_string(b"1ABCD").unwrap();
        s.set_attribute(crate::sgr::Attribute::Unset).unwrap();
        s.test_write_string(b"\n2EFGH\n3IJKL").unwrap();

        s.cursor_absolute(0, 2);
        s.cursor_scroll_region_up(2).unwrap();

        {
            let contents = dump(&s, scr());
            assert_eq!(contents, b"2EFGH\n3IJKL");
        }

        // The style should be gone from the page since the only user
        // was the erased row.
        let page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
        assert_eq!(0usize, unsafe { (*page).styles.count() });
        s.deinit();
    }

    // NEGATIVE CONTROL (`P3-W5 T3 Step 4(a)`) — the fast in-page path and the
    // `_slow` PageList path must be observationally identical.
    //
    // The oracle has NO such test: upstream only covers a handful of shapes by
    // hand. The differential is what makes the fast path falsifiable — it is
    // the ONLY guard that can see a one-cell/one-row error in the rotated
    // region base or extent, because every hand-written case would still
    // produce "something plausible".
    //
    // MUTATIONS EXECUTED AND OBSERVED TO FAIL (`P3-W5 T3`), each reverted:
    //
    //   1. `rows = ...ptr(...).add(pin_y - limit)` -> `.add(pin_y - limit + 1)`
    //      (region base off by one row)
    //   2. `from_raw_parts_mut(rows, limit + 1)` -> `(rows, limit)`
    //      (region extent off by one row)
    //   3. `page_cell = ...add(self.cursor.x)` -> `.add(self.cursor.x + 1)`
    //      (cursor cell off by one)
    //
    // The grid is deliberately built so BOTH clear branches of the fast path
    // are exercised: `bg` toggles the zero-fill hot path vs. `clear_cells`, and
    // the `styled` arm puts managed memory (a style ref) in the erased row.
    #[test]
    fn screen_cursor_scroll_region_up_fast_matches_slow_differential() {
        // One screen, written identically for both arms.
        fn build(
            rows: size::CellCountInt,
            bg: bool,
            styled: bool,
            cursor_y: size::CellCountInt,
        ) -> Screen {
            let mut s = mkscreen(5, rows, 0);
            if styled {
                s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
            }
            for y in 0..rows {
                if y > 0 {
                    s.test_write_string(b"\n").unwrap();
                }
                let line = [b'A' + (y as u8), b'B', b'C', b'D', b'E'];
                s.test_write_string(&line).unwrap();
                if styled && y == 0 {
                    s.set_attribute(crate::sgr::Attribute::Unset).unwrap();
                }
            }
            if bg {
                s.set_attribute(crate::sgr::Attribute::DirectColorBg(crate::color::RGB {
                    r: 0x12,
                    g: 0x34,
                    b: 0x56,
                }))
                .unwrap();
            }
            s.cursor_absolute(2, cursor_y);
            s
        }

        // Every cell of the whole screen, as raw backing bits, plus the
        // cursor's own position AND its cached `page_row`/`page_cell`
        // pointers expressed as offsets from the page bases. Raw bits catch
        // style ids and content tags that a string dump cannot see.
        //
        // VACUITY NOTE (`P3-W5 T3`): the first version of this snapshot omitted
        // the two cached-pointer offsets, and mutation 3 (`page_cell` off by
        // one cell) PASSED it — the differential was vacuous for the one field
        // the fast path computes by hand rather than via `row_and_cell()`. The
        // offsets below are what make it bind; keep them.
        #[allow(clippy::type_complexity)]
        fn snapshot(
            s: &Screen,
        ) -> (
            Vec<u64>,
            size::CellCountInt,
            size::CellCountInt,
            usize,
            usize,
        ) {
            let mut out = Vec::new();
            for y in 0..s.pages.rows {
                for x in 0..s.pages.cols {
                    let lc = s
                        .pages
                        .get_cell(point::Point::Active(point::Coordinate {
                            x,
                            y: u32::from(y),
                        }))
                        .unwrap();
                    out.push(unsafe { *lc.cell }.to_bits());
                }
            }
            let page = unsafe { pagelist::Node::page((*s.cursor.page_pin).node) };
            let rows_base = unsafe { (*page).rows.ptr((*page).memory_ptr()) };
            let row_off =
                (s.cursor.page_row as usize - rows_base as usize) / core::mem::size_of::<Row>();
            let cells_base = unsafe { (*page).get_cells(s.cursor.page_row) } as *mut Cell;
            let cell_off =
                (s.cursor.page_cell as usize - cells_base as usize) / core::mem::size_of::<Cell>();
            (out, s.cursor.x, s.cursor.y, row_off, cell_off)
        }

        let mut cases = 0usize;
        for rows in [2 as size::CellCountInt, 3, 5, 8] {
            for cursor_y in 1..rows {
                for limit in 1..=cursor_y {
                    for bg in [false, true] {
                        for styled in [false, true] {
                            let mut fast = build(rows, bg, styled, cursor_y);
                            let mut slow = build(rows, bg, styled, cursor_y);

                            // The fast arm goes through the public entry point.
                            // With a single page and `pin.y >= limit` this is
                            // always the in-page rotate.
                            assert!(
                                unsafe { (*fast.cursor.page_pin).y } as usize >= limit as usize,
                                "case is not on the fast path; the differential would be vacuous"
                            );
                            fast.cursor_scroll_region_up(limit as usize).unwrap();

                            // The slow arm calls the PageList path directly.
                            slow.cursor_scroll_region_up_slow(limit as usize).unwrap();
                            slow.assert_integrity();

                            assert_eq!(
                                snapshot(&fast),
                                snapshot(&slow),
                                "fast != slow for rows={rows} cursor_y={cursor_y} \
                                 limit={limit} bg={bg} styled={styled}"
                            );
                            assert_eq!(
                                dump(&fast, scr()),
                                dump(&slow, scr()),
                                "dump differs for rows={rows} cursor_y={cursor_y} \
                                 limit={limit} bg={bg} styled={styled}"
                            );

                            fast.deinit();
                            slow.deinit();
                            cases += 1;
                        }
                    }
                }
            }
        }

        // Anti-vacuity floor: if the grid ever collapses to nothing the
        // assertions above would never run and the test would still pass.
        // (R-1)R/2 (cursor_y, limit) pairs per R, x 4 (bg x styled):
        // R=2 -> 1, R=3 -> 3, R=5 -> 10, R=8 -> 28; (1+3+10+28) * 4 = 168.
        assert_eq!(cases, 168, "grid size changed");
    }

    // test "Screen: scrolling moves viewport"
    #[test]
    fn screen_scrolling_moves_viewport() {
        let mut s = mkscreen(10, 3, 1);
        s.test_write_string(b"1ABCD\n2EFGH\n3IJKL\n").unwrap();
        s.test_write_string(b"1ABCD\n2EFGH\n3IJKL").unwrap();
        s.scroll(Scroll::DeltaRow(-2));

        {
            // Test our contents rotated
            let contents = dump(&s, vp());
            assert_eq!(contents, b"2EFGH\n3IJKL\n1ABCD");
        }

        {
            assert_eq!(
                point::Point::Screen(point::Coordinate { x: 0, y: 1 }),
                s.pages
                    .point_from_pin(
                        point::Tag::Screen,
                        s.pages.get_top_left(point::Tag::Viewport)
                    )
                    .unwrap()
            );
        }
        s.deinit();
    }

    // test "Screen: scroll and clear full screen"
    #[test]
    fn screen_scroll_and_clear_full_screen() {
        let mut s = mkscreen(10, 3, 5);
        s.test_write_string(b"1ABCD\n2EFGH\n3IJKL").unwrap();

        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"1ABCD\n2EFGH\n3IJKL");
        }

        s.scroll_clear().unwrap();
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"");
        }
        {
            let contents = dump(&s, scr());
            assert_eq!(contents, b"1ABCD\n2EFGH\n3IJKL");
        }
        s.deinit();
    }

    // test "Screen: scroll and clear partial screen"
    #[test]
    fn screen_scroll_and_clear_partial_screen() {
        let mut s = mkscreen(10, 3, 5);
        s.test_write_string(b"1ABCD\n2EFGH").unwrap();

        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"1ABCD\n2EFGH");
        }

        s.scroll_clear().unwrap();
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"");
        }
        {
            let contents = dump(&s, scr());
            assert_eq!(contents, b"1ABCD\n2EFGH");
        }
        s.deinit();
    }

    // test "Screen: scroll and clear empty screen"
    #[test]
    fn screen_scroll_and_clear_empty_screen() {
        let mut s = mkscreen(10, 3, 5);
        s.scroll_clear().unwrap();
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"");
        }
        {
            let contents = dump(&s, scr());
            assert_eq!(contents, b"");
        }
        s.deinit();
    }

    // test "Screen: scroll and clear ignore blank lines"
    #[test]
    fn screen_scroll_and_clear_ignore_blank_lines() {
        let mut s = mkscreen(10, 3, 10);
        s.test_write_string(b"1ABCD\n2EFGH").unwrap();
        s.scroll_clear().unwrap();
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"");
        }

        // Move back to top-left
        s.cursor_absolute(0, 0);

        // Write and clear
        s.test_write_string(b"3ABCD\n").unwrap();
        {
            let contents = dump(&s, act());
            assert_eq!(contents, b"3ABCD");
        }

        s.scroll_clear().unwrap();
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"");
        }

        // Move back to top-left
        s.cursor_absolute(0, 0);
        s.test_write_string(b"X").unwrap();

        {
            let contents = dump(&s, scr());
            assert_eq!(contents, b"1ABCD\n2EFGH\n3ABCD\nX");
        }
        s.deinit();
    }

    // test "Screen: scroll above same page"
    #[test]
    fn screen_scroll_above_same_page() {
        let mut s = mkscreen(10, 3, 10);
        s.set_attribute(crate::sgr::Attribute::DirectColorBg(crate::color::RGB {
            r: 155,
            g: 0,
            b: 0,
        }))
        .unwrap();
        s.test_write_string(b"1ABCD\n2EFGH\n3IJKL").unwrap();
        s.cursor_absolute(0, 1);
        s.pages.clear_dirty();

        // At this point:
        //  +-------------+ ACTIVE
        //   +----------+ : = PAGE 0
        // 0 |1ABCD00000| | 0
        // 1 |2EFGH00000| | 1
        //   :^         : : = PIN 0
        // 2 |3IJKL00000| | 2
        //   +----------+ :
        //  +-------------+

        // PORTS: oracle 0bad91adb — the in-place row rotation renews this
        // node's generation, so a cached (node, serial) pair is rejected.
        let node = unsafe { (*s.cursor.page_pin).node };
        let serial = unsafe { (*node).serial };
        s.cursor_scroll_above().unwrap();
        assert!(!s.pages.node_is_valid(node, serial));

        //   +----------+ = PAGE 0
        // 0 |1ABCD00000|
        //  +-------------+ ACTIVE
        // 1 |2EFGH00000| | 0
        // 2 |          | | 1
        //   :^         : : = PIN 0
        // 3 |3IJKL00000| | 2
        //   +----------+ :
        //  +-------------+

        // try s.pages.diagram(std.io.getStdErr().writer());

        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"2EFGH\n\n3IJKL");
        }
        {
            let list_cell = s
                .pages
                .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 1 }))
                .unwrap();
            let cell = list_cell.cell;
            assert!(unsafe { (*cell).content_tag() } == pagepkg::ContentTag::BgColorRgb);
            assert_eq!(pagepkg::CellRgb { r: 155, g: 0, b: 0 }, unsafe {
                (*cell).content_color_rgb()
            });
        }

        // Page 0 row 1 (active row 0) is dirty because the cursor moved off of it.
        assert!(s
            .pages
            .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 0 })));
        // Page 0 row 2 (active row 1) is dirty because it was cleared.
        assert!(s
            .pages
            .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 1 })));
        // Page 0 row 3 (active row 2) is dirty because it's new.
        assert!(s
            .pages
            .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 2 })));
        s.deinit();
    }

    // test "Screen: scroll above same page but cursor on previous page"
    #[test]
    fn screen_scroll_above_same_page_but_cursor_on_previous_page() {
        let mut s = mkscreen(10, 5, 10);

        // We need to get the cursor to a new page
        let first_page_size = unsafe { crate::pagelist::Node::capacity(s.pages.pages.first).rows };
        unsafe { (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(true) };
        for _ in 0..(first_page_size - 3) {
            s.test_write_string(b"\n").unwrap();
        }
        unsafe {
            (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(false)
        };

        s.set_attribute(crate::sgr::Attribute::DirectColorBg(crate::color::RGB {
            r: 155,
            g: 0,
            b: 0,
        }))
        .unwrap();
        s.test_write_string(b"1A\n2B\n3C\n4D\n5E").unwrap();
        s.cursor_absolute(0, 1);
        s.pages.clear_dirty();

        // Ensure we're still on the first page and have a second
        assert!(std::ptr::eq(
            unsafe { (*s.cursor.page_pin).node },
            s.pages.pages.first
        ));
        assert!(!unsafe { (*s.pages.pages.first).next }.is_null());

        // At this point:
        //      +----------+ = PAGE 0
        //  ... :          :
        //     +-------------+ ACTIVE
        // 4305 |1A00000000| | 0
        // 4306 |2B00000000| | 1
        //      :^         : : = PIN 0
        // 4307 |3C00000000| | 2
        //      +----------+ :
        //      +----------+ : = PAGE 1
        //    0 |4D00000000| | 3
        //    1 |5E00000000| | 4
        //      +----------+ :
        //     +-------------+

        // PORTS: oracle 0bad91adb — both pages are rotated in place, so both
        // generations are renewed.
        let first_node = s.pages.pages.first;
        let second_node = unsafe { (*first_node).next };
        let first_serial = unsafe { (*first_node).serial };
        let second_serial = unsafe { (*second_node).serial };
        s.cursor_scroll_above().unwrap();
        assert!(!s.pages.node_is_valid(first_node, first_serial));
        assert!(!s.pages.node_is_valid(second_node, second_serial));

        //      +----------+ = PAGE 0
        //  ... :          :
        // 4305 |1A00000000|
        //     +-------------+ ACTIVE
        // 4306 |2B00000000| | 0
        // 4307 |          | | 1
        //      :^         : : = PIN 0
        //      +----------+ :
        //      +----------+ : = PAGE 1
        //    0 |3C00000000| | 2
        //    1 |4D00000000| | 3
        //    2 |5E00000000| | 4
        //      +----------+ :
        //     +-------------+

        // try s.pages.diagram(std.io.getStdErr().writer());

        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"2B\n\n3C\n4D\n5E");
        }
        {
            let list_cell = s
                .pages
                .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 1 }))
                .unwrap();
            let cell = list_cell.cell;
            assert!(unsafe { (*cell).content_tag() } == pagepkg::ContentTag::BgColorRgb);
            assert_eq!(pagepkg::CellRgb { r: 155, g: 0, b: 0 }, unsafe {
                (*cell).content_color_rgb()
            });
        }

        // Page 0's penultimate row is dirty because the cursor moved off of it.
        assert!(s
            .pages
            .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 0 })));
        // The rest of the rows are dirty because they've been modified or are new.
        assert!(s
            .pages
            .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 1 })));
        assert!(s
            .pages
            .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 2 })));
        assert!(s
            .pages
            .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 3 })));
        assert!(s
            .pages
            .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 4 })));
        s.deinit();
    }

    // test "Screen: scroll above same page but cursor on previous page last row"
    #[test]
    fn screen_scroll_above_same_page_but_cursor_on_previous_page_last_row() {
        let mut s = mkscreen(10, 5, 10);

        // We need to get the cursor to a new page
        let first_page_size = unsafe { crate::pagelist::Node::capacity(s.pages.pages.first).rows };
        unsafe { (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(true) };
        for _ in 0..(first_page_size - 2) {
            s.test_write_string(b"\n").unwrap();
        }
        unsafe {
            (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(false)
        };

        s.set_attribute(crate::sgr::Attribute::DirectColorBg(crate::color::RGB {
            r: 155,
            g: 0,
            b: 0,
        }))
        .unwrap();
        s.test_write_string(b"1A\n2B\n3C\n4D\n5E").unwrap();
        s.cursor_absolute(0, 1);
        s.pages.clear_dirty();

        // Ensure we're still on the first page and have a second
        assert!(std::ptr::eq(
            unsafe { (*s.cursor.page_pin).node },
            s.pages.pages.first
        ));
        assert!(!unsafe { (*s.pages.pages.first).next }.is_null());

        // At this point:
        //      +----------+ = PAGE 0
        //  ... :          :
        //     +-------------+ ACTIVE
        // 4306 |1A00000000| | 0
        // 4307 |2B00000000| | 1
        //      :^         : : = PIN 0
        //      +----------+ :
        //      +----------+ : = PAGE 1
        //    0 |3C00000000| | 2
        //    1 |4D00000000| | 3
        //    2 |5E00000000| | 4
        //      +----------+ :
        //     +-------------+

        s.cursor_scroll_above().unwrap();

        //      +----------+ = PAGE 0
        //  ... :          :
        // 4306 |1A00000000|
        //     +-------------+ ACTIVE
        // 4307 |2B00000000| | 0
        //      +----------+ :
        //      +----------+ : = PAGE 1
        //    0 |          | | 1
        //      :^         : : = PIN 0
        //    1 |3C00000000| | 2
        //    2 |4D00000000| | 3
        //    3 |5E00000000| | 4
        //      +----------+ :
        //     +-------------+

        // try s.pages.diagram(std.io.getStdErr().writer());

        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"2B\n\n3C\n4D\n5E");
        }
        {
            let list_cell = s
                .pages
                .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 1 }))
                .unwrap();
            let cell = list_cell.cell;
            assert!(unsafe { (*cell).content_tag() } == pagepkg::ContentTag::BgColorRgb);
            assert_eq!(pagepkg::CellRgb { r: 155, g: 0, b: 0 }, unsafe {
                (*cell).content_color_rgb()
            });
        }

        // Page 0's final row is dirty because the cursor moved off of it.
        assert!(s
            .pages
            .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 0 })));
        // Page 1's rows are all dirty because every row was moved.
        assert!(s
            .pages
            .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 1 })));
        assert!(s
            .pages
            .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 2 })));
        assert!(s
            .pages
            .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 3 })));
        assert!(s
            .pages
            .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 4 })));

        // Attempt to clear the style from the cursor and
        // then assert the integrity of both of our pages.
        //
        // This catches a case of memory corruption where the cursor
        // is moved between pages without accounting for style refs.
        s.set_attribute(crate::sgr::Attribute::ResetBg).unwrap();
        unsafe { (*crate::pagelist::Node::page(s.pages.pages.first)).assert_integrity() };
        unsafe { (*crate::pagelist::Node::page(s.pages.pages.last)).assert_integrity() };
        s.deinit();
    }

    // test "Screen: scroll above creates new page"
    #[test]
    fn screen_scroll_above_creates_new_page() {
        let mut s = mkscreen(10, 3, 10);

        // We need to get the cursor to a new page
        let first_page_size = unsafe { crate::pagelist::Node::capacity(s.pages.pages.first).rows };
        unsafe { (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(true) };
        for _ in 0..(first_page_size - 3) {
            s.test_write_string(b"\n").unwrap();
        }
        unsafe {
            (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(false)
        };

        s.set_attribute(crate::sgr::Attribute::DirectColorBg(crate::color::RGB {
            r: 155,
            g: 0,
            b: 0,
        }))
        .unwrap();
        s.test_write_string(b"1ABCD\n2EFGH\n3IJKL").unwrap();
        s.cursor_absolute(0, 1);
        s.pages.clear_dirty();

        // Ensure we're still on the first page
        assert!(std::ptr::eq(
            unsafe { (*s.cursor.page_pin).node },
            s.pages.pages.first
        ));

        // At this point:
        //      +----------+ = PAGE 0
        //  ... :          :
        //     +-------------+ ACTIVE
        // 4305 |1ABCD00000| | 0
        // 4306 |2EFGH00000| | 1
        //      :^         : : = PIN 0
        // 4307 |3IJKL00000| | 2
        //      +----------+ :
        //     +-------------+
        // PORTS: oracle 0bad91adb — rotating rows into a freshly grown page
        // still renews the existing source page's generation.
        let node = s.pages.pages.first;
        let serial = unsafe { (*node).serial };
        s.cursor_scroll_above().unwrap();
        assert!(!s.pages.node_is_valid(node, serial));

        //      +----------+ = PAGE 0
        //  ... :          :
        // 4305 |1ABCD00000|
        //     +-------------+ ACTIVE
        // 4306 |2EFGH00000| | 0
        // 4307 |          | | 1
        //      :^         : : = PIN 0
        //      +----------+ :
        //      +----------+ : = PAGE 1
        //    0 |3IJKL00000| | 2
        //      +----------+ :
        //     +-------------+

        // try s.pages.diagram(std.io.getStdErr().writer());

        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"2EFGH\n\n3IJKL");
        }
        {
            let list_cell = s
                .pages
                .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 1 }))
                .unwrap();
            let cell = list_cell.cell;
            assert!(unsafe { (*cell).content_tag() } == pagepkg::ContentTag::BgColorRgb);
            assert_eq!(pagepkg::CellRgb { r: 155, g: 0, b: 0 }, unsafe {
                (*cell).content_color_rgb()
            });
        }

        // Page 0's penultimate row is dirty because the cursor moved off of it.
        assert!(s
            .pages
            .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 0 })));
        // Page 0's final row is dirty because it was cleared.
        assert!(s
            .pages
            .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 1 })));
        // Page 1's row is dirty because it's new.
        assert!(s
            .pages
            .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 2 })));
        s.deinit();
    }

    // test "Screen: scroll above with cursor on non-final row"
    #[test]
    fn screen_scroll_above_with_cursor_on_non_final_row() {
        let mut s = mkscreen(10, 4, 10);

        // Get the cursor to be 2 rows above a new page
        let first_page_size = unsafe { crate::pagelist::Node::capacity(s.pages.pages.first).rows };
        unsafe { (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(true) };
        for _ in 0..(first_page_size - 3) {
            s.test_write_string(b"\n").unwrap();
        }
        unsafe {
            (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(false)
        };

        // Write 3 lines of text, forcing the last line into the first
        // row of a new page. Move our cursor onto the previous page.
        s.set_attribute(crate::sgr::Attribute::DirectColorBg(crate::color::RGB {
            r: 155,
            g: 0,
            b: 0,
        }))
        .unwrap();
        s.test_write_string(b"1AB\n2BC\n3DE\n4FG").unwrap();
        s.cursor_absolute(0, 1);
        s.pages.clear_dirty();

        // Ensure we're still on the first page. So our cursor is on the first
        // page but we have two pages of data.
        assert!(std::ptr::eq(
            unsafe { (*s.cursor.page_pin).node },
            s.pages.pages.first
        ));

        //      +----------+ = PAGE 0
        //  ... :          :
        //     +-------------+ ACTIVE
        // 4305 |1AB0000000| | 0
        // 4306 |2BC0000000| | 1
        //      :^         : : = PIN 0
        // 4307 |3DE0000000| | 2
        //      +----------+ :
        //      +----------+ : = PAGE 1
        //    0 |4FG0000000| | 3
        //      +----------+ :
        //     +-------------+
        s.cursor_scroll_above().unwrap();

        //     +----------+ = PAGE 0
        //  ... :          :
        // 4305 |1AB0000000|
        //     +-------------+ ACTIVE
        // 4306 |2BC0000000| | 0
        // 4307 |          | | 1
        //      :^         : : = PIN 0
        //      +----------+ :
        //      +----------+ : = PAGE 1
        //    0 |3DE0000000| | 2
        //    1 |4FG0000000| | 3
        //      +----------+ :
        //     +-------------+
        // try s.pages.diagram(std.io.getStdErr().writer());

        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"2BC\n\n3DE\n4FG");
        }
        {
            let list_cell = s
                .pages
                .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 1 }))
                .unwrap();
            let cell = list_cell.cell;
            assert!(unsafe { (*cell).content_tag() } == pagepkg::ContentTag::BgColorRgb);
            assert_eq!(pagepkg::CellRgb { r: 155, g: 0, b: 0 }, unsafe {
                (*cell).content_color_rgb()
            });
        }

        // Page 0's penultimate row is dirty because the cursor moved off of it.
        assert!(s
            .pages
            .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 0 })));
        // Page 0's final row is dirty because it was cleared.
        assert!(s
            .pages
            .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 1 })));
        // Page 1's row is dirty because it's new.
        assert!(s
            .pages
            .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 2 })));
        s.deinit();
    }

    // test "Screen: scroll above no scrollback bottom of page"
    #[test]
    fn screen_scroll_above_no_scrollback_bottom_of_page() {
        let mut s = mkscreen(10, 3, 0);

        let first_page_size = unsafe { crate::pagelist::Node::capacity(s.pages.pages.first).rows };
        unsafe { (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(true) };
        for _ in 0..(first_page_size - 3) {
            s.test_write_string(b"\n").unwrap();
        }
        unsafe {
            (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(false)
        };

        s.set_attribute(crate::sgr::Attribute::DirectColorBg(crate::color::RGB {
            r: 155,
            g: 0,
            b: 0,
        }))
        .unwrap();
        s.test_write_string(b"1ABCD\n2EFGH\n3IJKL").unwrap();
        s.cursor_absolute(0, 1);
        s.pages.clear_dirty();

        // At this point:
        //  +-------------+ ACTIVE
        //   +----------+ : = PAGE 0
        // 0 |1ABCD00000| | 0
        // 1 |2EFGH00000| | 1
        //   :^         : : = PIN 0
        // 2 |3IJKL00000| | 2
        //   +----------+ :
        //  +-------------+

        s.cursor_scroll_above().unwrap();

        //   +----------+ = PAGE 0
        // 0 |1ABCD00000|
        //  +-------------+ ACTIVE
        // 1 |2EFGH00000| | 0
        // 2 |          | | 1
        //   :^         : : = PIN 0
        // 3 |3IJKL00000| | 2
        //   +----------+ :
        //  +-------------+

        //try s.pages.diagram(std.io.getStdErr().writer());

        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"2EFGH\n\n3IJKL");
        }
        {
            let list_cell = s
                .pages
                .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 1 }))
                .unwrap();
            let cell = list_cell.cell;
            assert!(unsafe { (*cell).content_tag() } == pagepkg::ContentTag::BgColorRgb);
            assert_eq!(pagepkg::CellRgb { r: 155, g: 0, b: 0 }, unsafe {
                (*cell).content_color_rgb()
            });
        }

        // Page 0 row 1 (active row 0) is dirty because the cursor moved off of it.
        assert!(s
            .pages
            .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 0 })));
        // Page 0 row 2 (active row 1) is dirty because it was cleared.
        assert!(s
            .pages
            .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 1 })));
        // Page 0 row 3 (active row 2) is dirty because it is new.
        assert!(s
            .pages
            .is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 2 })));
        s.deinit();
    }

    // test "Screen: scroll above hyperlink-dense row to fresh page"
    #[test]
    #[cfg_attr(
        all(miri, debug_assertions),
        ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound in the debug-assertions-ON G6 lane, where `Page::assert_integrity` runs on nearly every page mutation (measured per test; see the C-1 table there). It RUNS in the G6c lane (`check_leaks.sh --debug-assertions-off`). Do NOT widen this to a plain `miri` ignore: that would drop the test from BOTH lanes."
    )]
    fn screen_scroll_above_hyperlink_dense_row_to_fresh_page() {
        // Regression test for https://github.com/ghostty-org/ghostty/discussions/13160
        //
        // When a scroll-above operation pushes a row carrying more unique
        // hyperlinks than a fresh page's default hyperlink capacity across
        // a page boundary, the cross-page row clone must increase the
        // destination page's capacity (like insertLines/deleteLines do)
        // rather than error out mid-operation, which leaves the page list
        // half-mutated and aborts later (e.g. in clearCells).
        let mut s = mkscreen(10, 5, 10000);

        // Fill the first page so it is exactly full and the cursor is on
        // its last row (which is also the bottom row of the active area).
        // The next grow() will then allocate a fresh page.
        let first_page_rows = unsafe { crate::pagelist::Node::capacity(s.pages.pages.first).rows };
        unsafe { (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(true) };
        for _ in 0..(first_page_rows - 1) {
            s.test_write_string(b"\n").unwrap();
        }
        unsafe {
            (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(false)
        };
        assert!(std::ptr::eq(s.pages.pages.first, s.pages.pages.last));
        assert_eq!(
            unsafe { crate::pagelist::Node::capacity(s.pages.pages.first).rows },
            unsafe {
                (*crate::pagelist::Node::page(s.pages.pages.first))
                    .size
                    .rows
            },
        );
        assert_eq!(s.pages.rows - 1, s.cursor.y);

        // Fill the bottom row with unique hyperlinks: more than a fresh
        // page can hold with default hyperlink capacity.
        for i in 0..s.pages.cols {
            let uri = format!("http://example.com/{i}").into_bytes();
            s.start_hyperlink(&uri, None).unwrap();
            s.test_write_string(b"A").unwrap();
            s.end_hyperlink();
        }
        assert_eq!(s.pages.cols as usize, unsafe {
            (*crate::pagelist::Node::page((*s.cursor.page_pin).node))
                .hyperlink_set
                .count() as usize
        });

        // Move the cursor above the bottom row and scroll. The dense row is
        // pushed across the page boundary into the freshly allocated page.
        s.cursor_absolute(0, 1);
        s.cursor_scroll_above().unwrap();

        // We must have created a second page and the dense row must now be
        // the top row of that page.
        assert!(!std::ptr::eq(s.pages.pages.first, s.pages.pages.last));

        // All hyperlinks must have survived the scroll intact: every cell
        // flagged as a hyperlink must resolve to a real entry in its page's
        // hyperlink map. A half-applied scroll leaves cells whose hyperlink
        // flag is set but that have no map entry, which aborts in
        // clearCells later.
        let mut node_ = s.pages.pages.first;
        while !node_.is_null() {
            let page: *mut Page = unsafe { crate::pagelist::Node::page(node_) };
            unsafe { (*page).assert_integrity() };
            for y in 0..unsafe { (*page).size.rows } as usize {
                let row = unsafe { (*page).get_row(y) };
                if !unsafe { (*row).hyperlink() } {
                    continue;
                }
                let cells = unsafe { (*page).get_cells(row) };
                let cells: &mut [pagepkg::Cell] = unsafe { &mut *cells };
                for cell in cells.iter_mut() {
                    if !cell.hyperlink() {
                        continue;
                    }
                    assert!(unsafe { (*page).lookup_hyperlink(cell) }.is_some());
                }
            }
            node_ = unsafe { (*node_).next };
        }
        assert_eq!(s.pages.cols as usize, unsafe {
            (*crate::pagelist::Node::page(s.pages.pages.last))
                .hyperlink_set
                .count() as usize
        });

        // The dense row is still the bottom row of the active area.
        for x in 0..s.pages.cols {
            let list_cell = s
                .pages
                .get_cell(point::Point::Active(point::Coordinate { x, y: 4 }))
                .unwrap();
            assert!(unsafe { (*list_cell.cell).hyperlink() });
            let page: *mut Page = unsafe { crate::pagelist::Node::page(list_cell.node) };
            let id = unsafe { (*page).lookup_hyperlink(list_cell.cell) }.unwrap();
            let link = unsafe { &*(*page).hyperlink_set.get((*page).memory_ptr(), id) };
            let expected = format!("http://example.com/{x}");
            assert_eq!(expected.as_bytes(), unsafe {
                link.uri.slice((*page).memory_ptr())
            });
        }

        s.deinit();
    }

    // test "Screen: scroll above hyperlink-dense row to existing page"
    #[test]
    #[cfg_attr(
        all(miri, debug_assertions),
        ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound in the debug-assertions-ON G6 lane, where `Page::assert_integrity` runs on nearly every page mutation (measured per test; see the C-1 table there). It RUNS in the G6c lane (`check_leaks.sh --debug-assertions-off`). Do NOT widen this to a plain `miri` ignore: that would drop the test from BOTH lanes."
    )]
    fn screen_scroll_above_hyperlink_dense_row_to_existing_page() {
        // Same as the fresh page variant above but the destination page
        // already exists (fresh_node == null path in cursorScrollAboveRotate).
        let mut s = mkscreen(10, 5, 10000);

        // Fill the first page so it is exactly full and the cursor is on
        // its last row.
        let first_page_rows = unsafe { crate::pagelist::Node::capacity(s.pages.pages.first).rows };
        unsafe { (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(true) };
        for _ in 0..(first_page_rows - 1) {
            s.test_write_string(b"\n").unwrap();
        }
        unsafe {
            (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(false)
        };
        assert_eq!(s.pages.rows - 1, s.cursor.y);

        // Fill the last row of the first page with unique hyperlinks:
        // more than a page can hold with default hyperlink capacity.
        for i in 0..s.pages.cols {
            let uri = format!("http://example.com/{i}").into_bytes();
            s.start_hyperlink(&uri, None).unwrap();
            s.test_write_string(b"A").unwrap();
            s.end_hyperlink();
        }

        // Scroll twice so the active area straddles the page boundary:
        // the last two active rows are on a second page while the dense
        // row remains the last row of the first page.
        s.test_write_string(b"\n\n").unwrap();
        assert!(!std::ptr::eq(s.pages.pages.first, s.pages.pages.last));
        assert!(std::ptr::eq(
            unsafe { (*s.cursor.page_pin).node },
            s.pages.pages.last
        ));

        // Move the cursor to an active row that is still on the first page
        // and above the dense row, then scroll. grow() has capacity in the
        // last page so no fresh page is allocated, but the dense row still
        // crosses the page boundary during the rotate.
        s.cursor_absolute(0, 0);
        assert!(std::ptr::eq(
            unsafe { (*s.cursor.page_pin).node },
            s.pages.pages.first
        ));
        s.cursor_scroll_above().unwrap();

        // All hyperlinks must have survived the scroll intact: every cell
        // flagged as a hyperlink must resolve to a real entry in its page's
        // hyperlink map.
        let mut node_ = s.pages.pages.first;
        while !node_.is_null() {
            let page: *mut Page = unsafe { crate::pagelist::Node::page(node_) };
            unsafe { (*page).assert_integrity() };
            for y in 0..unsafe { (*page).size.rows } as usize {
                let row = unsafe { (*page).get_row(y) };
                if !unsafe { (*row).hyperlink() } {
                    continue;
                }
                let cells = unsafe { (*page).get_cells(row) };
                let cells: &mut [pagepkg::Cell] = unsafe { &mut *cells };
                for cell in cells.iter_mut() {
                    if !cell.hyperlink() {
                        continue;
                    }
                    assert!(unsafe { (*page).lookup_hyperlink(cell) }.is_some());
                }
            }
            node_ = unsafe { (*node_).next };
        }
        assert_eq!(s.pages.cols as usize, unsafe {
            (*crate::pagelist::Node::page(s.pages.pages.last))
                .hyperlink_set
                .count() as usize
        });

        s.deinit();
    }

    // test "Screen: select untracked"
    #[test]
    fn screen_select_untracked() {
        let mut s = mkscreen(10, 10, 0);
        s.test_write_string(b"ABC  DEF\n 123\n456").unwrap();

        assert!(s.selection.is_none());
        let tracked = s.pages.count_tracked_pins();
        s.select(Some(Selection::init(
            s.pages
                .pin(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
                .unwrap(),
            s.pages
                .pin(point::Point::Active(point::Coordinate { x: 3, y: 0 }))
                .unwrap(),
            false,
        )))
        .unwrap();
        assert_eq!(tracked + 2, s.pages.count_tracked_pins());
        s.select(None).unwrap();
        assert_eq!(tracked, s.pages.count_tracked_pins());
        s.deinit();
    }

    // test "Screen: select replaces existing pins"
    #[test]
    fn screen_select_replaces_existing_pins() {
        let mut s = mkscreen(10, 10, 0);
        s.test_write_string(b"ABC  DEF\n 123\n456").unwrap();

        let tracked = s.pages.count_tracked_pins();
        s.select(Some(Selection::init(
            s.pages
                .pin(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
                .unwrap(),
            s.pages
                .pin(point::Point::Active(point::Coordinate { x: 3, y: 0 }))
                .unwrap(),
            false,
        )))
        .unwrap();
        assert_eq!(tracked + 2, s.pages.count_tracked_pins());

        // Replacing the selection must untrack the prior selection's pins
        // rather than leak them.
        s.select(Some(Selection::init(
            s.pages
                .pin(point::Point::Active(point::Coordinate { x: 0, y: 1 }))
                .unwrap(),
            s.pages
                .pin(point::Point::Active(point::Coordinate { x: 2, y: 1 }))
                .unwrap(),
            false,
        )))
        .unwrap();
        assert_eq!(tracked + 2, s.pages.count_tracked_pins());
        s.deinit();
    }

    // test "Screen: reselecting tracked selection preserves its pins"
    #[test]
    fn screen_reselecting_tracked_selection_preserves_its_pins() {
        let mut s = mkscreen(10, 2, 0);

        s.select(Some(Selection::init(
            s.pages
                .pin(point::Point::Active(point::Coordinate { x: 1, y: 0 }))
                .unwrap(),
            s.pages
                .pin(point::Point::Active(point::Coordinate { x: 3, y: 0 }))
                .unwrap(),
            false,
        )))
        .unwrap();

        // `Selection` is `Copy`, so this hands our own tracked selection back
        // to `select` by value — exactly the aliasing upstream fixes.
        let current = s.selection.unwrap();
        s.select(Some(current)).unwrap();

        assert_eq!(selection::Order::Forward, s.selection.unwrap().order(&s));

        s.deinit();
    }

    // test "Screen: hyperlink start/end"
    #[test]
    fn screen_hyperlink_start_end() {
        let mut s = mkscreen(5, 5, 0);
        assert!(s.cursor.hyperlink_id == 0);
        {
            let page = unsafe { &(*crate::pagelist::Node::page((*s.cursor.page_pin).node)) };
            assert_eq!(0usize, page.hyperlink_set.count());
        }

        s.start_hyperlink(b"http://example.com", None).unwrap();
        assert!(s.cursor.hyperlink_id != 0);
        {
            let page = unsafe { &(*crate::pagelist::Node::page((*s.cursor.page_pin).node)) };
            assert_eq!(1usize, page.hyperlink_set.count());
        }

        s.end_hyperlink();
        assert!(s.cursor.hyperlink_id == 0);
        {
            let page = unsafe { &(*crate::pagelist::Node::page((*s.cursor.page_pin).node)) };
            assert_eq!(0usize, page.hyperlink_set.count());
        }
        s.deinit();
    }

    // test "Screen: hyperlink accepts its current values"
    #[test]
    fn screen_hyperlink_accepts_its_current_values() {
        let mut s = mkscreen(5, 5, 0);

        s.start_hyperlink(b"http://example.com", Some(b"current"))
            .unwrap();

        // PORT NOTE: the reference passes `current.uri` / `current.id.explicit`
        // straight back into `s.startHyperlink`. Rust's borrow checker forbids
        // holding `&s.cursor.hyperlink` across the `&mut self` call, so we read
        // the current values through the RAW pointer the cursor already stores
        // (`cursor.hyperlink: Option<*mut Hyperlink>`) — the same aliasing the
        // reference test exercises, expressed in the way this crate allows.
        //
        // PORT NOTE (P3 F2): the slices stay RAW (`*const [u8]`) all the way
        // into `start_hyperlink_raw`. Materializing them as `&[u8]` arguments
        // would strongly protect them for the duration of the call, and the
        // call releases the very allocation they point into — see the PORT NOTE
        // on `Screen::start_hyperlink_raw`. This is the aliasing the reference
        // test exists to exercise, so the test keeps it.
        let link: *mut CursorHyperlink = s.cursor.hyperlink.unwrap();
        let (uri_ptr, uri_len) = unsafe { ((*link).uri.as_ptr(), (*link).uri.len()) };
        let (id_ptr, id_len) = match unsafe { &(*link).id } {
            crate::hyperlink::HyperlinkId::Explicit(v) => (v.as_ptr(), v.len()),
            crate::hyperlink::HyperlinkId::Implicit(_) => unreachable!(),
        };
        let uri: *const [u8] = core::ptr::slice_from_raw_parts(uri_ptr, uri_len);
        let id: *const [u8] = core::ptr::slice_from_raw_parts(id_ptr, id_len);
        unsafe { s.start_hyperlink_raw(uri, Some(id)) }.unwrap();

        let link: *mut CursorHyperlink = s.cursor.hyperlink.unwrap();
        assert_eq!(b"http://example.com".as_slice(), unsafe {
            &(&(*link).uri)[..]
        });
        match unsafe { &(*link).id } {
            crate::hyperlink::HyperlinkId::Explicit(v) => {
                assert_eq!(b"current".as_slice(), &v[..])
            }
            crate::hyperlink::HyperlinkId::Implicit(_) => unreachable!(),
        }

        s.deinit();
    }

    // test "Screen: implicit hyperlink ID wraps" (success half)
    #[test]
    fn screen_implicit_hyperlink_id_wraps() {
        let mut s = mkscreen(5, 5, 0);

        s.cursor.hyperlink_implicit_id = size::OffsetInt::MAX;
        s.start_hyperlink(b"http://example.com", None).unwrap();

        assert_eq!(0 as size::OffsetInt, s.cursor.hyperlink_implicit_id);
        let link: *mut CursorHyperlink = s.cursor.hyperlink.unwrap();
        match unsafe { &(*link).id } {
            crate::hyperlink::HyperlinkId::Implicit(v) => {
                assert_eq!(size::OffsetInt::MAX, *v)
            }
            crate::hyperlink::HyperlinkId::Explicit(_) => unreachable!(),
        }

        s.deinit();
    }

    // test "Screen: implicit hyperlink ID wraps" (allocation-failure rollback half)
    //
    // A failed allocation must roll the wrapped counter back to its
    // original value as well.
    #[test]
    // WS18 release lane: this test injects a failure through a `rustty_util::tripwire`
    // fail point. `tripwire::Module::enabled()` is the union of `cfg!(test)`,
    // `cfg!(debug_assertions)` and the `tripwire-test` feature, which this crate
    // turns on as a DEV-dependency (Cargo.toml) — reproducing Zig's
    // `builtin.is_test`, true at ANY optimize mode. So this runs in BOTH the debug
    // and the release test lanes, like the reference test does. No `ignore`.
    fn screen_implicit_hyperlink_id_wrap_rolls_back_on_allocation_failure() {
        let mut s = mkscreen(5, 5, 0);

        s.cursor.hyperlink_implicit_id = size::OffsetInt::MAX;
        // var failing = testing.FailingAllocator.init(alloc, .{});
        // failing.fail_index = failing.alloc_index;
        let failing = crate::alloc::testing::FailingAllocator::over_heap(
            crate::alloc::testing::Config::default(),
        );
        failing.fail_index.set(failing.alloc_index.get());
        let c = failing.allocator();
        crate::alloc::testing::with_handle(&c, |failing_alloc| {
            // const original_alloc = s.alloc;
            // defer s.alloc = original_alloc;
            // s.alloc = failing.allocator();
            let original_alloc = s.alloc;
            s.alloc = failing_alloc;
            assert_eq!(
                Err(pagelist::grow::IncreaseCapacityError::OutOfMemory),
                s.start_hyperlink(b"http://example.com", None)
            );
            s.alloc = original_alloc;
        });
        assert!(failing.has_induced_failure.get());

        assert_eq!(size::OffsetInt::MAX, s.cursor.hyperlink_implicit_id);

        s.deinit();
    }

    // test "Screen: hyperlink reuse"
    #[test]
    fn screen_hyperlink_reuse() {
        let mut s = mkscreen(5, 5, 0);

        assert!(s.cursor.hyperlink_id == 0);
        {
            let page = unsafe { &(*crate::pagelist::Node::page((*s.cursor.page_pin).node)) };
            assert_eq!(0usize, page.hyperlink_set.count());
        }

        // Use it for the first time
        s.start_hyperlink(b"http://example.com", None).unwrap();
        assert!(s.cursor.hyperlink_id != 0);
        let id = s.cursor.hyperlink_id;

        // Reuse the same hyperlink, expect we have the same ID
        s.start_hyperlink(b"http://example.com", None).unwrap();
        assert_eq!(id, s.cursor.hyperlink_id);
        {
            let page = unsafe { &(*crate::pagelist::Node::page((*s.cursor.page_pin).node)) };
            assert_eq!(1usize, page.hyperlink_set.count());
        }

        s.end_hyperlink();
        assert!(s.cursor.hyperlink_id == 0);
        {
            let page = unsafe { &(*crate::pagelist::Node::page((*s.cursor.page_pin).node)) };
            assert_eq!(0usize, page.hyperlink_set.count());
        }
        s.deinit();
    }

    // test "Screen: hyperlink cursor state on resize"
    #[test]
    fn screen_hyperlink_cursor_state_on_resize() {
        // This test depends on underlying PageList implementation so
        // it may be invalid one day. It's here to document/verify the
        // current behavior.

        let mut s = mkscreen(5, 10, 0);

        // Start a hyperlink
        s.start_hyperlink(b"http://example.com", None).unwrap();
        assert!(s.cursor.hyperlink_id != 0);
        {
            let page = unsafe { &(*crate::pagelist::Node::page((*s.cursor.page_pin).node)) };
            assert_eq!(1usize, page.hyperlink_set.count());
        }

        // Resize. Any column growth will trigger a page to be reallocated.
        s.resize(Resize {
            cols: 10,
            rows: 10,
            reflow: true,
            prompt_redraw: osc::semantic_prompt::Redraw::False,
        })
        .unwrap();
        assert!(s.cursor.hyperlink_id != 0);
        {
            let page = unsafe { &(*crate::pagelist::Node::page((*s.cursor.page_pin).node)) };
            assert_eq!(1usize, page.hyperlink_set.count());
        }

        s.end_hyperlink();
        assert!(s.cursor.hyperlink_id == 0);
        {
            let page = unsafe { &(*crate::pagelist::Node::page((*s.cursor.page_pin).node)) };
            assert_eq!(0usize, page.hyperlink_set.count());
        }
        s.deinit();
    }

    // test "Screen: cursorSetHyperlink OOM + URI too large for string alloc"
    #[cfg_attr(
        all(miri, debug_assertions),
        ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
    )]
    #[test]
    fn screen_cursor_set_hyperlink_oom_uri_too_large_for_string_alloc() {
        let mut s = mkscreen(80, 24, 0);

        // Start a hyperlink with a URI that just barely fits in the string alloc.
        // This will ensure that additional string alloc space is needed for the
        // redundant copy of the URI when the page is re-alloced.
        let uri = vec![b'a'; pagepkg::std_capacity().string_bytes as usize - 8];
        s.start_hyperlink(&uri, None).unwrap();

        // Figure out how many cells should can have hyperlinks in this page,
        // and write twice that number, to guarantee the capacity needs to be
        // increased at some point.
        let base_capacity = unsafe {
            (*crate::pagelist::Node::page((*s.cursor.page_pin).node)).hyperlink_capacity()
        };
        let base_string_bytes =
            unsafe { crate::pagelist::Node::capacity((*s.cursor.page_pin).node).string_bytes };
        for _ in 0..(base_capacity * 2) {
            s.cursor_set_hyperlink().unwrap();
            if s.cursor.x >= s.pages.cols - 1 {
                s.cursor_down_or_scroll().unwrap();
                s.cursor_horizontal_absolute(0);
            } else {
                s.cursor_right(1);
            }
        }

        // Make sure the capacity really did increase.
        assert!(
            base_capacity
                < unsafe {
                    (*crate::pagelist::Node::page((*s.cursor.page_pin).node)).hyperlink_capacity()
                }
        );
        // And that our string_bytes increased as well.
        assert!(
            base_string_bytes
                < unsafe {
                    crate::pagelist::Node::capacity((*s.cursor.page_pin).node).string_bytes
                }
        );
        s.deinit();
    }

    // test "Screen: increaseCapacity cursor style ref count preserved"
    #[test]
    fn screen_increase_capacity_cursor_style_ref_count_preserved() {
        let mut s = mkscreen(5, 5, 0);
        s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
        s.test_write_string(b"1ABCD").unwrap();

        // We should have one page and it should be our cursor page
        assert!(std::ptr::eq(s.pages.pages.first, s.pages.pages.last));
        assert!(std::ptr::eq(s.pages.pages.first, unsafe {
            (*s.cursor.page_pin).node
        }));

        let old_style = s.cursor.style;

        {
            let page = unsafe { &(*crate::pagelist::Node::page(s.pages.pages.last)) };
            // 5 chars + cursor = 6 refs
            assert_eq!(6, unsafe {
                let base = page.memory_ptr();
                page.styles.ref_count(base, s.cursor.style_id)
            });
        }

        // This forces the page to change via increaseCapacity.
        let new_node = s
            .increase_capacity(
                unsafe { (*s.cursor.page_pin).node },
                Some(pagelist::grow::IncreaseCapacity::GraphemeBytes),
            )
            .unwrap();

        // Cursor's page_pin should now point to the new node
        assert!(std::ptr::eq(unsafe { (*s.cursor.page_pin).node }, new_node));

        // Verify cursor's page_cell and page_row are correctly reloaded from the pin
        let page_rac = unsafe { (*s.cursor.page_pin).row_and_cell() };
        assert!(std::ptr::eq(s.cursor.page_row, page_rac.row));
        assert!(std::ptr::eq(s.cursor.page_cell, page_rac.cell));

        // Style should be preserved
        assert!(old_style.eql(&s.cursor.style));
        assert!(s.cursor.style_id != style::DEFAULT_ID);

        // After increaseCapacity, the 5 chars are cloned (5 refs) and
        // the cursor's style is re-added (1 ref) = 6 total.
        {
            let page = unsafe { &(*crate::pagelist::Node::page(s.pages.pages.last)) };
            let ref_count = unsafe {
                let base = page.memory_ptr();
                page.styles.ref_count(base, s.cursor.style_id)
            };
            assert_eq!(6, ref_count);
        }
        s.deinit();
    }

    // test "Screen: increaseCapacity cursor hyperlink ref count preserved"
    #[test]
    fn screen_increase_capacity_cursor_hyperlink_ref_count_preserved() {
        let mut s = mkscreen(5, 5, 0);
        s.start_hyperlink(b"https://example.com/", None).unwrap();
        s.test_write_string(b"1ABCD").unwrap();

        // We should have one page and it should be our cursor page
        assert!(std::ptr::eq(s.pages.pages.first, s.pages.pages.last));
        assert!(std::ptr::eq(s.pages.pages.first, unsafe {
            (*s.cursor.page_pin).node
        }));

        {
            let page = unsafe { &(*crate::pagelist::Node::page(s.pages.pages.last)) };
            // Cursor has the hyperlink active = 1 count in hyperlink_set
            assert_eq!(1usize, page.hyperlink_set.count());
            assert!(s.cursor.hyperlink_id != 0);
            assert!(s.cursor.hyperlink.is_some());
        }

        // This forces the page to change via increaseCapacity.
        let _ = s
            .increase_capacity(
                unsafe { (*s.cursor.page_pin).node },
                Some(pagelist::grow::IncreaseCapacity::GraphemeBytes),
            )
            .unwrap();

        // Hyperlink should be preserved with correct URI
        assert!(s.cursor.hyperlink.is_some());
        assert!(s.cursor.hyperlink_id != 0);
        assert_eq!(
            b"https://example.com/".as_slice(),
            unsafe { &(*s.cursor.hyperlink.unwrap()).uri }.as_slice()
        );

        // After increaseCapacity, the hyperlink is re-added to the new page.
        {
            let page = unsafe { &(*crate::pagelist::Node::page(s.pages.pages.last)) };
            assert_eq!(1usize, page.hyperlink_set.count());
        }
        s.deinit();
    }

    // test "Screen: increaseCapacity cursor with both style and hyperlink preserved"
    #[test]
    fn screen_increase_capacity_cursor_with_both_style_and_hyperlink_preserved() {
        let mut s = mkscreen(5, 5, 0);

        // Set both a non-default style AND an active hyperlink.
        // Write one character first with bold to mark the row as styled,
        // then start the hyperlink and write more characters.
        s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
        s.start_hyperlink(b"https://example.com/", None).unwrap();
        s.test_write_string(b"1ABCD").unwrap();

        // We should have one page and it should be our cursor page
        assert!(std::ptr::eq(s.pages.pages.first, s.pages.pages.last));
        assert!(std::ptr::eq(s.pages.pages.first, unsafe {
            (*s.cursor.page_pin).node
        }));

        let old_style = s.cursor.style;

        {
            let page = unsafe { &(*crate::pagelist::Node::page(s.pages.pages.last)) };
            // 5 chars + cursor = 6 refs for bold style
            assert_eq!(6, unsafe {
                let base = page.memory_ptr();
                page.styles.ref_count(base, s.cursor.style_id)
            });
            // Cursor has the hyperlink active = 1 count in hyperlink_set
            assert_eq!(1usize, page.hyperlink_set.count());
            assert!(s.cursor.style_id != style::DEFAULT_ID);
            assert!(s.cursor.hyperlink_id != 0);
            assert!(s.cursor.hyperlink.is_some());
        }

        // This forces the page to change via increaseCapacity.
        let _ = s
            .increase_capacity(
                unsafe { (*s.cursor.page_pin).node },
                Some(pagelist::grow::IncreaseCapacity::GraphemeBytes),
            )
            .unwrap();

        // Style should be preserved
        assert!(old_style.eql(&s.cursor.style));
        assert!(s.cursor.style_id != style::DEFAULT_ID);

        // Hyperlink should be preserved with correct URI
        assert!(s.cursor.hyperlink.is_some());
        assert!(s.cursor.hyperlink_id != 0);
        assert_eq!(
            b"https://example.com/".as_slice(),
            unsafe { &(*s.cursor.hyperlink.unwrap()).uri }.as_slice()
        );

        // After increaseCapacity, both style and hyperlink are re-added to the new page.
        {
            let page = unsafe { &(*crate::pagelist::Node::page(s.pages.pages.last)) };
            let ref_count = unsafe {
                let base = page.memory_ptr();
                page.styles.ref_count(base, s.cursor.style_id)
            };
            assert_eq!(6, ref_count);
            assert_eq!(1usize, page.hyperlink_set.count());
        }
        s.deinit();
    }

    // test "Screen: increaseCapacity non-cursor page returns early"
    #[test]
    fn screen_increase_capacity_non_cursor_page_returns_early() {
        // Test that calling increaseCapacity on a page that is NOT the cursor's
        // page properly delegates to pages.increaseCapacity without doing the
        // extra cursor accounting (style/hyperlink re-adding).
        let mut s = mkscreen(80, 24, 10000);

        // Set up a custom style and hyperlink on the cursor
        s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
        s.start_hyperlink(b"https://example.com/", None).unwrap();
        s.test_write_string(b"Hello").unwrap();

        // Store cursor state before growing pages
        let old_style = s.cursor.style;
        let old_style_id = s.cursor.style_id;
        let old_hyperlink = s.cursor.hyperlink;
        let old_hyperlink_id = s.cursor.hyperlink_id;

        // The cursor is on the first (and only) page
        assert!(std::ptr::eq(s.pages.pages.first, s.pages.pages.last));
        assert!(std::ptr::eq(
            unsafe { (*s.cursor.page_pin).node },
            s.pages.pages.first
        ));

        // Grow pages until we have multiple pages. The cursor's pin stays on
        // the first page since we're just adding rows.
        let first_page_node = s.pages.pages.first;
        unsafe { (*crate::pagelist::Node::page(first_page_node)).pause_integrity_checks(true) };
        let n_grow = unsafe {
            crate::pagelist::Node::capacity(first_page_node).rows
                - crate::pagelist::Node::rows(first_page_node)
        };
        for _ in 0..n_grow {
            let _ = s.pages.grow().unwrap();
        }
        unsafe { (*crate::pagelist::Node::page(first_page_node)).pause_integrity_checks(false) };
        let _ = s.pages.grow().unwrap();

        // Now we have two pages
        assert!(!std::ptr::eq(s.pages.pages.first, s.pages.pages.last));
        let second_page = s.pages.pages.last;

        // Cursor should still be on the first page (where it was created)
        assert!(std::ptr::eq(
            unsafe { (*s.cursor.page_pin).node },
            s.pages.pages.first
        ));
        assert!(!std::ptr::eq(
            unsafe { (*s.cursor.page_pin).node },
            second_page
        ));

        let second_page_styles_cap = unsafe { crate::pagelist::Node::capacity(second_page).styles };
        let cursor_page_styles_cap =
            unsafe { crate::pagelist::Node::capacity((*s.cursor.page_pin).node).styles };

        // Call increaseCapacity on the second page (NOT the cursor's page)
        let new_second_page = s
            .increase_capacity(second_page, Some(pagelist::grow::IncreaseCapacity::Styles))
            .unwrap();

        // The second page should have increased capacity
        assert_eq!(second_page_styles_cap * 2, unsafe {
            crate::pagelist::Node::capacity(new_second_page).styles
        });

        // The cursor's page (first page) should be unchanged
        assert_eq!(cursor_page_styles_cap, unsafe {
            crate::pagelist::Node::capacity((*s.cursor.page_pin).node).styles
        });

        // Cursor state should be completely unchanged since we didn't touch its page
        assert!(old_style.eql(&s.cursor.style));
        assert_eq!(old_style_id, s.cursor.style_id);
        assert_eq!(old_hyperlink, s.cursor.hyperlink);
        assert_eq!(old_hyperlink_id, s.cursor.hyperlink_id);

        // Verify hyperlink is still valid
        assert!(s.cursor.hyperlink.is_some());
        assert_eq!(
            b"https://example.com/".as_slice(),
            unsafe { &(*s.cursor.hyperlink.unwrap()).uri }.as_slice()
        );
        s.deinit();
    }

    // test "Screen: cursorDown to page with insufficient capacity"
    #[test]
    fn screen_cursor_down_to_page_with_insufficient_capacity() {
        // Regression test for https://github.com/ghostty-org/ghostty/issues/10282
        //
        // This test exposes a use-after-realloc bug in cursorDown (and similar
        // cursor movement functions). The bug pattern:
        //
        // 1. cursorDown creates a by-value copy of the pin via page_pin.down(n)
        // 2. cursorChangePin is called, which may trigger increaseCapacity
        //    if the target page's style map is full
        // 3. increaseCapacity frees the old page and creates a new one
        // 4. The local pin copy still points to the freed page
        // 5. rowAndCell() on the stale pin accesses freed memory

        // Small screen to make page boundary crossing easy to set up
        let mut s = mkscreen(10, 3, 1);

        // Scroll down enough to create a second page
        let start_page: *mut Page = unsafe { crate::pagelist::Node::page(s.pages.pages.last) };
        let rem = unsafe { (*start_page).capacity.rows };
        unsafe { (*start_page).pause_integrity_checks(true) };
        for _ in 0..rem {
            s.cursor_down_or_scroll().unwrap();
        }
        unsafe { (*start_page).pause_integrity_checks(false) };

        // Cursor should now be on a new page
        let new_page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
        assert!(!std::ptr::eq(start_page, new_page));

        // Fill new_page's style map to capacity. When we move INTO this page
        // with a style set, increaseCapacity will be triggered.
        {
            unsafe { (*new_page).pause_integrity_checks(true) };

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
                    let base = (*new_page).memory_ptr();
                    (*new_page).styles.add(base, st)
                };
                if res.is_ok() {
                    n += 1;
                } else {
                    break;
                }
            }

            unsafe { (*new_page).pause_integrity_checks(false) };
            unsafe { (*new_page).assert_integrity() };
        }

        // Move cursor to start of active area and set a style
        s.cursor_absolute(0, 0);
        s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
        assert!(s.cursor.style.flags.bold);
        assert!(s.cursor.style_id != style::DEFAULT_ID);

        // Find the row just before the page boundary
        let mut found = false;
        for row in 0..(s.pages.rows - 1) {
            s.cursor_absolute(0, row);
            let cur_node = unsafe { (*s.cursor.page_pin).node };
            if let Some(next_pin) = unsafe { (*s.cursor.page_pin).down(1) } {
                if !std::ptr::eq(next_pin.node, cur_node) {
                    // Cursor is at 'row', moving down crosses to new_page
                    assert!(std::ptr::eq(
                        unsafe { crate::pagelist::Node::page(next_pin.node) },
                        new_page as *const Page
                    ));

                    // This cursorDown triggers the bug: the local page_pin copy
                    // becomes stale after increaseCapacity, causing rowAndCell()
                    // to access freed memory.
                    s.cursor_down(1);

                    // If the fix is applied, verify correct state
                    assert!(s.cursor.y == row + 1);
                    assert!(s.cursor.style.flags.bold);

                    found = true;
                    break;
                }
            }
        }
        // PORT NOTE: the reference's `else { try testing.expect(false); }` arm
        // on the `for ... else` (the "Didn't find boundary" failure) is mirrored
        // by this assert on the loop-completed-without-break flag.
        if !found {
            // Didn't find boundary
            assert!(found);
        }
        s.deinit();
    }

    // test "Screen setAttribute increases capacity when style map is full"
    #[test]
    fn screen_set_attribute_increases_capacity_when_style_map_is_full() {
        // Tests that setAttribute succeeds when the style map is full by
        // increasing page capacity. When capacity is at max and increaseCapacity
        // returns OutOfSpace, manualStyleUpdate will split the page instead.

        // Use a small screen with multiple rows
        let mut s = mkscreen(10, 5, 10);

        // Write content to multiple rows
        s.test_write_string(b"line1\nline2\nline3\nline4\nline5")
            .unwrap();

        // Get the page and fill its style map to capacity
        let page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
        let original_styles_capacity = unsafe { (*page).capacity.styles };

        // Fill the style map to capacity using the StyleSet's layout capacity
        // which accounts for the load factor
        {
            unsafe { (*page).pause_integrity_checks(true) };

            let max_items = unsafe { (*page).styles.layout.cap };
            let mut n: usize = 1;
            while n < max_items {
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
                    let base = (*page).memory_ptr();
                    (*page).styles.add(base, st)
                };
                if res.is_err() {
                    break;
                }
                n += 1;
            }

            unsafe { (*page).pause_integrity_checks(false) };
            unsafe { (*page).assert_integrity() };
        }

        // Now try to set a new unique attribute that would require a new style slot
        // This should succeed by increasing capacity (or splitting if at max capacity)
        s.set_attribute(crate::sgr::Attribute::Bold).unwrap();

        // The style should have been applied (bold flag set)
        assert!(s.cursor.style.flags.bold);

        // The cursor should have a valid non-default style_id
        assert!(s.cursor.style_id != style::DEFAULT_ID);

        // Either the capacity increased or the page was split/changed
        let current_page: *mut Page =
            unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
        let capacity_increased =
            unsafe { (*current_page).capacity.styles } > original_styles_capacity;
        let page_changed = !std::ptr::eq(current_page, page);
        assert!(capacity_increased || page_changed);
        s.deinit();
    }

    // test "Screen setAttribute splits page on OutOfSpace at max styles"
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (.port/phase-2/MIRI_EXCEPTIONS.md, 'the 2026-07-31 adversarial re-audit'): too slow for either Miri lane, but NOT a hang. RUN TO COMPLETION under Miri with `-C debug-assertions=off`, single worker, no bound: `ok ... finished in 1656.89s` (27.6 min), 0 UB / 0 leaks. Removing the integrity assertions does not rescue it. CORRECTION to the WS19 T6 wording: the cost is NOT `page-mapping/LZ4 volume` -- this test never touches LZ4, and an instrumented run attributes 13.0s (0.8%) to the increase_capacity page-mapping phase and 1650.7s (99.6%) to filling the RefCountedSet style map with 53248 entries. Covered natively + by ASan."
    )]
    #[test]
    fn screen_set_attribute_splits_page_on_out_of_space_at_max_styles() {
        let mut s = mkscreen(10, 10, 0);

        // Write content to multiple rows so we have something to split
        s.test_write_string(b"line1\nline2\nline3\nline4\nline5")
            .unwrap();

        // Remember the original node
        let original_node = unsafe { (*s.cursor.page_pin).node };

        // Increase the page's style capacity to max by repeatedly calling increaseCapacity
        // Use Screen.increaseCapacity to properly maintain cursor state
        let max_styles = size::CellCountInt::MAX;
        while unsafe { crate::pagelist::Node::capacity((*s.cursor.page_pin).node).styles }
            < max_styles
        {
            if s.increase_capacity(
                unsafe { (*s.cursor.page_pin).node },
                Some(pagelist::grow::IncreaseCapacity::Styles),
            )
            .is_err()
            {
                break;
            }
        }

        // Get the page reference after increaseCapacity - cursor may have moved
        let page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
        assert_eq!(max_styles, unsafe { (*page).capacity.styles });

        // Fill the style map to capacity using the StyleSet's layout capacity
        // which accounts for the load factor
        {
            unsafe { (*page).pause_integrity_checks(true) };

            let max_items = unsafe { (*page).styles.layout.cap };
            let mut n: usize = 1;
            while n < max_items {
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
                    let base = (*page).memory_ptr();
                    (*page).styles.add(base, st)
                };
                if res.is_err() {
                    break;
                }
                n += 1;
            }

            unsafe { (*page).pause_integrity_checks(false) };
            unsafe { (*page).assert_integrity() };
        }

        // Track the node before setAttribute
        let node_before_set = unsafe { (*s.cursor.page_pin).node };

        // Now try to set a new unique attribute that would require a new style slot
        // At max capacity, increaseCapacity will return OutOfSpace, triggering page split
        s.set_attribute(crate::sgr::Attribute::Bold).unwrap();

        // The style should have been applied (bold flag set)
        assert!(s.cursor.style.flags.bold);

        // The cursor should have a valid non-default style_id
        assert!(s.cursor.style_id != style::DEFAULT_ID);

        // The page should have been split
        let page_was_split = !std::ptr::eq(unsafe { (*s.cursor.page_pin).node }, node_before_set)
            || !unsafe { (*node_before_set).next }.is_null()
            || !unsafe { (*node_before_set).prev }.is_null()
            || !std::ptr::eq(unsafe { (*s.cursor.page_pin).node }, original_node);
        assert!(page_was_split);
        s.deinit();
    }

    // The cursor style and hyperlink IDs are only meaningful within the page
    // the cursor pin points at. scrollClear can move the active area onto a
    // later page while the cursor pin stays with its content on an earlier
    // page (now scrollback), so the reset in cursorReload must migrate both
    // references to the destination page. It previously replaced the pin
    // directly and then released the old style ID on the new page.
    // test "Screen scrollClear across pages migrates cursor style and hyperlink"
    #[test]
    fn screen_scroll_clear_across_pages_migrates_cursor_style_and_hyperlink() {
        let mut s = mkscreen(10, 10, usize::MAX);

        // Fill the first page so the active area spans two pages.
        let first_page_size = unsafe { crate::pagelist::Node::capacity(s.pages.pages.first).rows };
        unsafe { (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(true) };
        for _ in 0..(first_page_size - 5) {
            s.test_write_string(b"\n").unwrap();
        }
        unsafe {
            (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(false)
        };
        s.test_write_string(b"1\n2\n3\n4\n5\n6\n7\n8\n9\n10")
            .unwrap();
        assert!(!std::ptr::eq(s.pages.pages.first, s.pages.pages.last));

        // Move the cursor to the top of the active area, which is on the
        // first page, and give it a style and a hyperlink there.
        s.cursor_absolute(0, 0);
        assert!(std::ptr::eq(
            unsafe { (*s.cursor.page_pin).node },
            s.pages.pages.first
        ));
        s.set_attribute(crate::sgr::Attribute::Bold).unwrap();
        s.start_hyperlink(b"https://example.com/", None).unwrap();

        let old_page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
        let old_style_id = s.cursor.style_id;
        let old_hyperlink_id = s.cursor.hyperlink_id;
        assert!(old_style_id != style::DEFAULT_ID);
        assert!(old_hyperlink_id != 0);

        // All ten active rows are non-empty, so this moves the active area
        // fully onto the second page while the cursor pin stays with its
        // old row, which is now scrollback.
        s.scroll_clear().unwrap();

        // The cursor was moved to the new active top-left on the second
        // page with its style and hyperlink references rebuilt there.
        let new_page: *mut Page = unsafe { crate::pagelist::Node::page((*s.cursor.page_pin).node) };
        assert!(!std::ptr::eq(new_page, old_page));
        assert!(s.cursor.style_id != style::DEFAULT_ID);
        assert!(s.cursor.hyperlink_id != 0);
        assert!(
            unsafe {
                (*new_page)
                    .styles
                    .ref_count((*new_page).memory_ptr(), s.cursor.style_id)
            } > 0
        );
        assert!(
            unsafe {
                (*new_page)
                    .hyperlink_set
                    .ref_count((*new_page).memory_ptr(), s.cursor.hyperlink_id)
            } > 0
        );

        // The cursor's references on the old page were released. Nothing
        // else referenced either entry, so both are dead there now.
        assert!(
            unsafe {
                (*old_page)
                    .styles
                    .ref_count((*old_page).memory_ptr(), old_style_id)
            } == 0
        );
        assert!(
            unsafe {
                (*old_page)
                    .hyperlink_set
                    .ref_count((*old_page).memory_ptr(), old_hyperlink_id)
            } == 0
        );

        // Printing attaches the migrated style and hyperlink to a cell.
        s.test_write_string(b"B").unwrap();

        s.deinit();
    }

    // test "Screen: cursorScrollRegionUp recycled row has default metadata"
    #[test]
    fn screen_cursor_scroll_region_up_recycled_row_has_default_metadata() {
        let mut s = mkscreen(5, 5, 0);
        s.test_write_string(b"1ABCD\n2EFGH\n3IJKL\n4MNOP\n5QRST")
            .unwrap();

        // Simulate the top region row being part of a soft-wrapped,
        // prompt-marked line. Its Row storage is recycled as the new
        // blank cursor row and must not retain the metadata.
        {
            let rac = s.pages.get_cell(act()).unwrap();
            unsafe {
                (*rac.row).set_wrap(true);
                (*rac.row).set_wrap_continuation(true);
                (*rac.row).set_semantic_prompt(pagepkg::SemanticPrompt::Prompt);
            }
        }

        s.cursor_absolute(1, 2);
        s.cursor_scroll_region_up(2).unwrap();

        {
            let rac = s.pages.get_cell(pactive(0, 2)).unwrap();
            unsafe {
                assert!(!(*rac.row).wrap());
                assert!(!(*rac.row).wrap_continuation());
                assert_eq!(pagepkg::SemanticPrompt::None, (*rac.row).semantic_prompt());
            }
        }
        {
            let contents = dump(&s, scr());
            assert_eq!(contents, b"2EFGH\n3IJKL\n\n4MNOP\n5QRST");
        }
        s.deinit();
    }

    // test "Screen: cursorScrollRegionUp cross-page recycled row has default metadata"
    #[test]
    fn screen_cursor_scroll_region_up_cross_page_recycled_row_has_default_metadata() {
        let mut s = mkscreen(10, 5, 10);

        // We need to get the active area to span two pages so that the
        // scroll region does too, exercising the slow path (eraseRowBounded).
        let first_page_size = unsafe { crate::pagelist::Node::capacity(s.pages.pages.first).rows };
        unsafe { (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(true) };
        for _ in 0..(first_page_size - 3) {
            s.test_write_string(b"\n").unwrap();
        }
        unsafe {
            (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(false)
        };
        s.test_write_string(b"1A\n2B\n3C\n4D\n5E").unwrap();

        // The first row of the last page is the Row storage that ends up
        // recycled as the blank region-bottom row: the erased row's
        // storage stays on the first page (receiving this row's content
        // via clone) while this storage is cleared for the blank row.
        {
            let rac = s.pages.get_cell(pactive(0, 3)).unwrap();
            unsafe {
                (*rac.row).set_wrap(true);
                (*rac.row).set_wrap_continuation(true);
                (*rac.row).set_semantic_prompt(pagepkg::SemanticPrompt::Prompt);
            }
        }

        // Region rows 1-3 with the cursor on the region bottom, which is
        // on the second page while the region top is on the first page.
        s.cursor_absolute(0, 3);
        s.cursor_scroll_region_up(2).unwrap();

        {
            let rac = s.pages.get_cell(pactive(0, 3)).unwrap();
            unsafe {
                assert!(!(*rac.row).wrap());
                assert!(!(*rac.row).wrap_continuation());
                assert_eq!(pagepkg::SemanticPrompt::None, (*rac.row).semantic_prompt());
            }
        }
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"1A\n3C\n4D\n\n5E");
        }
        s.deinit();
    }

    // test "Screen: cursorScrollAbove cross-page recycled row has default metadata"
    #[test]
    fn screen_cursor_scroll_above_cross_page_recycled_row_has_default_metadata() {
        let mut s = mkscreen(10, 5, 10);

        // Get the cursor page and the last page to differ so that
        // cursorScrollAbove takes the cross-page rotate path.
        let first_page_size = unsafe { crate::pagelist::Node::capacity(s.pages.pages.first).rows };
        unsafe { (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(true) };
        for _ in 0..(first_page_size - 3) {
            s.test_write_string(b"\n").unwrap();
        }
        unsafe {
            (*crate::pagelist::Node::page(s.pages.pages.first)).pause_integrity_checks(false)
        };
        s.test_write_string(b"1A\n2B\n3C\n4D\n5E").unwrap();
        s.cursor_absolute(0, 1);
        assert!(std::ptr::eq(
            unsafe { (*s.cursor.page_pin).node },
            s.pages.pages.first
        ));
        assert!(!unsafe { (*s.pages.pages.first).next }.is_null());

        // The last row of the cursor page is the Row storage that gets
        // recycled as the new blank row below the cursor after its content
        // is moved down to the next page.
        {
            let rac = s.pages.get_cell(pactive(0, 2)).unwrap();
            unsafe {
                (*rac.row).set_wrap(true);
                (*rac.row).set_wrap_continuation(true);
                (*rac.row).set_semantic_prompt(pagepkg::SemanticPrompt::Prompt);
            }
        }

        s.cursor_scroll_above().unwrap();

        // One row scrolled into history, so the blank row is at active
        // y=1 (just below the cursor's original row).
        {
            let rac = s.pages.get_cell(pactive(0, 1)).unwrap();
            unsafe {
                assert!(!(*rac.row).wrap());
                assert!(!(*rac.row).wrap_continuation());
                assert_eq!(pagepkg::SemanticPrompt::None, (*rac.row).semantic_prompt());
            }
        }
        {
            let contents = dump(&s, vp());
            assert_eq!(contents, b"2B\n\n3C\n4D\n5E");
        }
        s.deinit();
    }

    // test "Screen: cursorDownScroll no scrollback recycled row has default metadata"
    #[test]
    fn screen_cursor_down_scroll_no_scrollback_recycled_row_has_default_metadata() {
        let mut s = mkscreen(5, 3, 0);
        s.test_write_string(b"1ABCD\n2EFGH\n3IJKL").unwrap();

        {
            let rac = s.pages.get_cell(act()).unwrap();
            unsafe {
                (*rac.row).set_wrap(true);
                (*rac.row).set_wrap_continuation(true);
                (*rac.row).set_semantic_prompt(pagepkg::SemanticPrompt::Prompt);
            }
        }

        s.cursor_absolute(0, 2);
        s.cursor_down_scroll().unwrap();

        {
            let rac = s.pages.get_cell(pactive(0, 2)).unwrap();
            unsafe {
                assert!(!(*rac.row).wrap());
                assert!(!(*rac.row).wrap_continuation());
                assert_eq!(pagepkg::SemanticPrompt::None, (*rac.row).semantic_prompt());
            }
        }
        s.deinit();
    }

    // test "Screen: cursorDownScroll single row no scrollback resets metadata"
    #[test]
    fn screen_cursor_down_scroll_single_row_no_scrollback_resets_metadata() {
        let mut s = mkscreen(5, 1, 0);
        s.test_write_string(b"1ABCD").unwrap();

        {
            let rac = s.pages.get_cell(act()).unwrap();
            unsafe {
                (*rac.row).set_wrap(true);
                (*rac.row).set_wrap_continuation(true);
                (*rac.row).set_semantic_prompt(pagepkg::SemanticPrompt::Prompt);
            }
        }

        s.cursor_down_scroll().unwrap();

        {
            let rac = s.pages.get_cell(act()).unwrap();
            unsafe {
                assert!(!(*rac.row).wrap());
                assert!(!(*rac.row).wrap_continuation());
                assert_eq!(pagepkg::SemanticPrompt::None, (*rac.row).semantic_prompt());
            }
        }
        s.deinit();
    }

    // test "Screen: selectLine does not join lines across a recycled row"
    #[test]
    fn screen_select_line_does_not_join_lines_across_a_recycled_row() {
        let mut s = mkscreen(6, 5, 0);

        // A soft-wrapped line across rows 0 and 1: row 0 gets wrap=true.
        s.test_write_string(b"AAAAAAA").unwrap();

        // Scroll a region of rows 0-2 up by one: row 0 is discarded and
        // its Row storage recycled as the blank row 2.
        s.cursor_absolute(0, 2);
        s.cursor_scroll_region_up(2).unwrap();

        // Write unrelated single-line words on the recycled row and below.
        s.cursor_absolute(0, 2);
        s.test_write_string(b"world").unwrap();
        s.cursor_absolute(0, 3);
        s.test_write_string(b"hello").unwrap();

        // Selecting the line "world" must not extend into "hello": these
        // are separate hard lines. A stale wrap flag on the recycled row
        // would join them.
        {
            let pin = spin(&s, pactive(0, 2));
            let sel = s
                .select_line(SelectLine {
                    pin,
                    whitespace: Some(&selection_codepoints::DEFAULT_LINE_WHITESPACE),
                    semantic_prompt_boundary: true,
                })
                .unwrap();
            let contents = selstr(&mut s, sel, false);
            assert_eq!(contents, b"world");
            sel.deinit(&mut s);
        }
        s.deinit();
    }

    // UD131 (R13): the IL/DL/scroll row-copy retry loop
    // (`Screen::clone_partial_row_grow_capacity`) when the destination page's
    // style set is genuinely FULL of LIVE styles at the `u16` capacity ceiling
    // and the row pulled across the page boundary carries a style the set does
    // not have. The reference `@panic("increaseCapacity OutOfSpace")` here
    // (Screen.zig:747); measured reachable from real VT input (IL/DL, a scroll,
    // or a scroll-region shift crossing a page boundary). The port finishes the
    // copy: the unstorable style degrades to the default while the codepoint,
    // the storable grapheme and hyperlink, and the row flags are kept.
    //
    // Reaching the loop: the same loop is driven by real VT in the W2 orphan
    // test (`ud80_delete_lines_..._reclaims_them`); here it is called directly
    // so the full-LIVE-set precondition can be fabricated. Two pages are made
    // with `split` so each keeps a small `size.rows` (a full 53,247-style set
    // fits on wide rows), which keeps the page clones cheap. The first style is
    // genuinely pinned, so the loop re-clones once (reclaiming nothing) then
    // degrades — the path W2's one-shot flag bounds. A regression of that flag
    // (task R13-X2's control: resetting it whenever the node changed) fails
    // this test in under a second on the loop's debug re-clone counter instead
    // of re-cloning forever.
    #[test]
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION — BOTH LANES (rustty-vt, parity tail C-1; .port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound with debug assertions on (G6) and past an 8-minute bound with them off (G6c), both measured one test per process; runs natively in cargo test --workspace and under ASan (see MIRI_EXCEPTIONS.md)"
    )]
    fn ud131_clone_partial_row_loop_full_live_set_degrades_new_style_to_default() {
        let mut s = Screen::init(
            Allocator,
            Options {
                cols: 50000,
                rows: 8,
                max_scrollback_bytes: Some(1 << 30),
                max_scrollback_lines: None,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();
        s.pages
            .split(Pin {
                node: s.pages.pages.first,
                y: 4,
                x: 0,
                garbage: false,
            })
            .unwrap();
        assert_eq!(2, s.pages.total_pages());
        let dst_node = s.pages.pages.first;
        let src_node = s.pages.pages.last;
        assert!(!std::ptr::eq(dst_node, src_node));

        let mk = |cp: u32| -> Cell {
            let mut c = Cell::zeroed();
            c.set_content_tag(crate::page::ContentTag::Codepoint);
            c.set_content_codepoint(cp);
            c
        };

        // Source row on the second page: a NEW style ('A'), a grapheme ('B'), a
        // hyperlink ('C'), a plain cell ('D'), and the row flags.
        let src_page: *mut Page = unsafe { pagelist::Node::page(src_node) };
        let src_row: *const Row = unsafe { (*src_page).get_row(0) };
        let link_id = unsafe {
            (*src_page).insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(0),
                uri: b"https://ud131.example/".to_vec(),
            })
        }
        .unwrap();
        let new_style = style::Style {
            flags: style::Flags {
                bold: true,
                ..Default::default()
            },
            ..Default::default()
        };
        unsafe {
            let base = (*src_page).memory_ptr();
            let new_id = (*src_page).styles.add(base, new_style).unwrap();
            let row = (*src_page).get_row(0);
            (*row).set_wrap(true);
            (*row).set_semantic_prompt(crate::page::SemanticPrompt::Prompt);

            let c0 = (*src_page).get_row_and_cell(0, 0).cell;
            *c0 = mk('A' as u32);
            (*c0).set_style_id(new_id);
            (*row).set_styled(true);

            let c1 = (*src_page).get_row_and_cell(1, 0).cell;
            *c1 = mk('B' as u32);
            (*src_page).set_graphemes(row, c1, &[0x0301u32]).unwrap();

            let c2 = (*src_page).get_row_and_cell(2, 0).cell;
            *c2 = mk('C' as u32);
            (*src_page).set_hyperlink(row, c2, link_id).unwrap();

            let c3 = (*src_page).get_row_and_cell(3, 0).cell;
            *c3 = mk('D' as u32);
        }

        // Destination page: style capacity grown to the ceiling, then a full
        // LIVE set on rows 1..4 (row 0 blank = clone target), first style pinned.
        while unsafe { pagelist::Node::capacity(s.pages.pages.first).styles }
            < size::CellCountInt::MAX
        {
            let _ = s
                .pages
                .increase_capacity(
                    s.pages.pages.first,
                    Some(pagelist::grow::IncreaseCapacity::Styles),
                )
                .unwrap();
        }
        let dst_node = s.pages.pages.first;
        let (live, _pinned) =
            unsafe { (*pagelist::Node::page(dst_node)).test_fill_full_live_pinned(1..4, true) };
        assert!(live > 50_000, "the set is full of live styles: {live}");
        assert!(pagelist::grow::styles_hold_pinned(unsafe {
            pagelist::Node::page(dst_node)
        }));

        let out = s.clone_partial_row_grow_capacity(dst_node, 0, src_page, src_row, 0, 50000);
        let page: *mut Page = unsafe { pagelist::Node::page(out) };

        unsafe {
            let row = (*page).get_row(0);
            assert!((*row).wrap(), "the row wrap flag is kept");
            assert_eq!(
                (*row).semantic_prompt(),
                crate::page::SemanticPrompt::Prompt,
                "the row semantic prompt is kept"
            );

            let c0 = (*page).get_row_and_cell(0, 0).cell;
            assert_eq!((*c0).codepoint(), 'A' as u32);
            assert_eq!(
                (*c0).style_id(),
                style::DEFAULT_ID,
                "the unstorable style degrades to the default"
            );

            let c1 = (*page).get_row_and_cell(1, 0).cell;
            assert_eq!((*c1).codepoint(), 'B' as u32);
            assert!((*c1).has_grapheme(), "the grapheme is kept");
            assert_eq!((*page).lookup_grapheme(c1).unwrap(), &[0x0301u32]);

            let c2 = (*page).get_row_and_cell(2, 0).cell;
            assert_eq!((*c2).codepoint(), 'C' as u32);
            assert!((*c2).hyperlink(), "the hyperlink is kept");

            let c3 = (*page).get_row_and_cell(3, 0).cell;
            assert_eq!((*c3).codepoint(), 'D' as u32);
            assert_eq!((*c3).style_id(), style::DEFAULT_ID);

            assert!(
                (*page).styles.count() > 50_000,
                "the live styles are kept: {}",
                (*page).styles.count()
            );
            assert!(pagelist::grow::styles_hold_pinned(page));
        }

        let mut node_ = s.pages.pages.first;
        while !node_.is_null() {
            unsafe { (*pagelist::Node::page(node_)).assert_integrity() };
            node_ = unsafe { (*node_).next };
        }
        s.deinit();
    }

    // UD131 (R13), the per-dimension degrade through the IL/DL/scroll loop
    // (`Screen::clone_partial_row_grow_capacity`): the twin of
    // `pagelist::tests::ud131_erase_row_loop_degrades_only_new_styles_and_grows_the_other_dimensions`,
    // with the same mixed source row (`ud131_test_support::build_mixed_src`),
    // the same control comparison, and the same once-per-copy log check. See
    // that test for the row and the negative controls.
    fn ud131_mixed_screen(fill: bool) -> Screen {
        let mut s = Screen::init(
            Allocator,
            Options {
                cols: crate::page::ud131_test_support::mixed_cols(fill),
                rows: 6,
                max_scrollback_bytes: Some(1 << 30),
                max_scrollback_lines: None,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();
        s.pages
            .split(Pin {
                node: s.pages.pages.first,
                y: 3,
                x: 0,
                garbage: false,
            })
            .unwrap();
        assert_eq!(2, s.pages.total_pages());
        // Room on the SOURCE page for the mixed row's graphemes and links.
        for (dim, n) in [
            (pagelist::grow::IncreaseCapacity::GraphemeBytes, 8),
            (pagelist::grow::IncreaseCapacity::HyperlinkBytes, 8),
            (pagelist::grow::IncreaseCapacity::StringBytes, 4),
        ] {
            for _ in 0..n {
                let _ = s
                    .pages
                    .increase_capacity(s.pages.pages.last, Some(dim))
                    .unwrap();
            }
        }
        if fill {
            while unsafe { pagelist::Node::capacity(s.pages.pages.first).styles }
                < size::CellCountInt::MAX
            {
                let _ = s
                    .pages
                    .increase_capacity(
                        s.pages.pages.first,
                        Some(pagelist::grow::IncreaseCapacity::Styles),
                    )
                    .unwrap();
            }
            let (live, _) = unsafe {
                (*pagelist::Node::page(s.pages.pages.first)).test_fill_full_live_pinned(1..3, false)
            };
            assert!(live > 50_000, "the set is full of live styles: {live}");
        }
        s
    }

    #[test]
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION — BOTH LANES (rustty-vt, parity tail C-1; .port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound with debug assertions on (G6) and past an 8-minute bound with them off (G6c), both measured one test per process; runs natively in cargo test --workspace and under ASan (see MIRI_EXCEPTIONS.md)"
    )]
    fn ud131_clone_partial_row_loop_degrades_only_new_styles_and_grows_the_other_dimensions() {
        use crate::page::ud131_test_support as t;

        // Control: an ordinary destination stores everything.
        let mut ctl = ud131_mixed_screen(false);
        let ctl_cols = ctl.pages.cols as usize;
        let ctl_src = unsafe { pagelist::Node::page(ctl.pages.pages.last) };
        let ctl_row = unsafe { t::build_mixed_src(ctl_src, 0) };
        let cap = crate::log_capture::start();
        let ctl_out = ctl.clone_partial_row_grow_capacity(
            ctl.pages.pages.first,
            0,
            ctl_src,
            ctl_row,
            0,
            ctl_cols,
        );
        assert!(
            !cap.messages().iter().any(|(_, _, m)| m.contains("UD131")),
            "the control copy does not degrade"
        );
        drop(cap);
        let ctl_page = unsafe { pagelist::Node::page(ctl_out) };

        // Degraded: the destination's style set is full of live styles.
        let mut deg = ud131_mixed_screen(true);
        let deg_cols = deg.pages.cols as usize;
        let deg_src = unsafe { pagelist::Node::page(deg.pages.pages.last) };
        let deg_row = unsafe { t::build_mixed_src(deg_src, 0) };
        let start_cap = unsafe { pagelist::Node::capacity(deg.pages.pages.first) };
        let styles_before = unsafe {
            (*pagelist::Node::page(deg.pages.pages.first))
                .styles
                .count()
        };

        let cap = crate::log_capture::start();
        let out = deg.clone_partial_row_grow_capacity(
            deg.pages.pages.first,
            0,
            deg_src,
            deg_row,
            0,
            deg_cols,
        );
        let first_copy = cap.messages();
        let out = deg.clone_partial_row_grow_capacity(out, 0, deg_src, deg_row, 0, deg_cols);
        let both_copies = cap.messages();
        drop(cap);
        let page = unsafe { pagelist::Node::page(out) };

        unsafe {
            t::assert_only_new_styles_degraded(ctl_page, page, 0);

            let c = (*page).capacity;
            assert!(
                c.grapheme_bytes > start_cap.grapheme_bytes,
                "grapheme bytes grew"
            );
            assert!(
                c.hyperlink_bytes > start_cap.hyperlink_bytes,
                "hyperlink bytes grew"
            );
            assert!(c.string_bytes > start_cap.string_bytes, "string bytes grew");
            assert_eq!(c.styles, size::CellCountInt::MAX);
            assert_eq!((*page).styles.count(), styles_before);

            (*page).assert_integrity();
            (*ctl_page).assert_integrity();
        }

        let expected = (
            log::Level::Warn,
            crate::page::LOG_SCOPE.to_string(),
            "row copy at a page capacity ceiling: 5 cell part(s) degraded to default \
             (styles=5, hyperlinks=0, graphemes=0); see UD131"
                .to_string(),
        );
        let ud131 = |v: &[crate::log_capture::CapturedRecord]| {
            v.iter()
                .filter(|(_, _, m)| m.contains("UD131"))
                .cloned()
                .collect::<Vec<_>>()
        };
        assert_eq!(
            ud131(&first_copy),
            vec![expected.clone()],
            "one record for one copy"
        );
        assert_eq!(
            ud131(&both_copies),
            vec![expected.clone(), expected],
            "one record per copy"
        );

        ctl.deinit();
        deg.deinit();
    }

    // Parity tail S7-3 routing evidence (Rust-only; the reference routes by
    // construction): the Screen's own storage (`ScreenSet`'s
    // `alloc.create(Screen)`) and the cursor hyperlink (`alloc.create(Hyperlink)`
    // + `source.dupe(alloc)`) reach the screen's allocator, and are returned.
    #[test]
    fn screen_internals_route_through_the_allocator_handle() {
        use crate::alloc::testing::{with_handle, CountingAllocator};
        use crate::screen_set::ScreenSet;

        let counting = CountingAllocator::over_heap();
        let c = counting.allocator();
        with_handle(&c, |alloc| {
            let opts = || Options {
                cols: 5,
                rows: 5,
                max_scrollback_bytes: Some(0),
                max_scrollback_lines: None,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            };

            // A bare Screen: its PageList's allocations.
            let mut s = Screen::init(alloc, opts()).unwrap();
            let screen_calls = counting.alloc_calls.get();
            let screen_live = counting.live_bytes.get();
            assert!(screen_calls > 0);

            // The cursor hyperlink: the struct and both duplicated slices.
            let uri = b"http://example.com";
            let id = b"id";
            s.start_hyperlink(uri, Some(id)).unwrap();
            assert_eq!(
                screen_calls + 3,
                counting.alloc_calls.get(),
                "alloc.create(Hyperlink) + dupe(uri) + dupe(id)"
            );
            assert_eq!(
                screen_live + core::mem::size_of::<CursorHyperlink>() + uri.len() + id.len(),
                counting.live_bytes.get()
            );
            s.end_hyperlink();
            assert_eq!(screen_live, counting.live_bytes.get());

            // Screen.clone: its PageList clone plus the pin remap
            // (`TrackedPinsRemap.init(alloc)`). Baseline: the same PageList
            // clone without a remap.
            let top = point::Point::Screen(point::Coordinate::default());
            let before = counting.alloc_calls.get();
            let mut plain = pagelist::access::Clone {
                top,
                bot: None,
                tracked_pins: None,
            };
            let mut pages = s.pages.clone(alloc, &mut plain).unwrap();
            let pages_calls = counting.alloc_calls.get() - before;
            pages.deinit();
            drop(pages);
            assert_eq!(screen_live, counting.live_bytes.get());

            let before = counting.alloc_calls.get();
            let mut cloned = s.clone(alloc, top, None).unwrap();
            assert_eq!(
                pages_calls + 1,
                counting.alloc_calls.get() - before,
                "Screen.clone = its PageList clone + the pin remap, through alloc"
            );
            cloned.deinit();
            drop(cloned);
            assert_eq!(
                screen_live,
                counting.live_bytes.get(),
                "the clone frees everything it took"
            );

            s.deinit();
            drop(s);
            assert_eq!(0, counting.live_bytes.get());

            // A ScreenSet: one more allocation, of exactly the Screen struct.
            let before = counting.alloc_calls.get();
            let mut set = ScreenSet::init(alloc, opts()).unwrap();
            assert_eq!(
                before + screen_calls + 1,
                counting.alloc_calls.get(),
                "alloc.create(Screen) is one more allocation"
            );
            assert_eq!(
                screen_live + core::mem::size_of::<Screen>(),
                counting.live_bytes.get(),
                "the Screen's own storage comes from the screen's allocator"
            );
            set.deinit(alloc);
        });
        assert_eq!(0, counting.live_bytes.get(), "deinit returns every byte");
    }
}
