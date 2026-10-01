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

    /// Whether the resize may pull rows out of scrollback back into the
    /// active area. See PageList.Resize for details.
    pub pull_scrollback: bool,
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

/// Return the codepoint for word selection, following wide-character spacers.
fn select_word_codepoint(pin: Pin) -> Option<u21> {
    let rac = pin.row_and_cell();
    let cell: *mut Cell = match unsafe { (*rac.cell).wide() } {
        pagepkg::Wide::Narrow | pagepkg::Wide::Wide => rac.cell,
        pagepkg::Wide::SpacerTail => pin.left(1).row_and_cell().cell,
        pagepkg::Wide::SpacerHead => {
            let next_row = pin.down(1)?;
            let owner: *mut Cell = next_row.cells(pagelist::pin::CellSubset::All) as *mut Cell;
            if unsafe { (*owner).wide() } != pagepkg::Wide::Wide {
                return None;
            }
            owner
        }
    };
    if unsafe { (*cell).has_text() } {
        Some(unsafe { (*cell).codepoint() })
    } else {
        None
    }
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

mod attrs;
mod clear;
mod cursor;
mod select;

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
        //
        // PORT NOTE: the errdefer is `errdefer_pages!` at every fallible step
        // below. It is load-bearing since P7-W1 T5: the PageList's node and
        // page pools assert that every node came back, so a PageList dropped
        // without `deinit` aborts instead of leaking.
        macro_rules! errdefer_pages {
            ($e:expr) => {
                match $e {
                    Ok(v) => v,
                    Err(e) => {
                        pages.deinit();
                        return Err(e.into());
                    }
                }
            };
        }

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

            let page_pin = errdefer_pages!(pages.track_pin(Pin {
                node: pages.pages.first,
                x: 0,
                y: 0,
                garbage: false,
            }));
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
                        break 'start errdefer_pages!(pages.track_pin(Pin {
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
                        }));
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
                        errdefer_pages!(pages.track_pin(Pin {
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
                        }))
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
mod tests;
