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

/// The width class of a printSlice batch. Each batch contains only
/// codepoints of a single width class because they fill cells
/// differently: wide codepoints occupy a (wide, spacer_tail) cell
/// pair while narrow codepoints occupy a single cell.
//
// PORT NOTE: the reference is `const PrintSliceWidth = enum(u1) { narrow, wide }`
// used as a `comptime` parameter of `printSliceFill`/`printSliceEligible`.
// Stable Rust cannot use a user enum as a const-generic parameter, so the two
// tag VALUES of the `enum(u1)` are the const-generic argument directly and the
// enum's one method (`cellsPerCp`) is a free `const fn` over that tag.
const PRINT_SLICE_NARROW: u8 = 0;
const PRINT_SLICE_WIDE: u8 = 1;

/// The number of cells each codepoint of this width class occupies.
// fn cellsPerCp(comptime self: PrintSliceWidth) usize
const fn print_slice_cells_per_cp(width: u8) -> usize {
    match width {
        PRINT_SLICE_WIDE => 2,
        _ => 1,
    }
}

/// `simd.lanes(u32)` for the codepoint scans, and `simd.lanes(u64)` for the
/// cell scans/stores (Cell's backing integer). Hoisted to module consts so the
/// loop bodies below read like the reference's `if (simd.lanes(T)) |lanes|`.
const CP_LANES: Option<usize> = rustty_simd::lanes(core::mem::size_of::<u32>());
const CELL_LANES: Option<usize> = rustty_simd::lanes(core::mem::size_of::<u64>());

/// Whether a codepoint above 0xFF is eligible for the batched print
/// fast path with the given width class.
#[inline]
fn print_slice_eligible<const WIDTH: u8>(cp: u32) -> bool {
    assert!(cp > 0xFF);
    // if (comptime build_options.kitty_graphics) {
    //     if (cp == kitty.graphics.unicode.placeholder) return false;
    // }
    if crate::options::KITTY_GRAPHICS && cp == kitty::graphics_unicode::PLACEHOLDER {
        return false;
    }

    unicode::table().get(cp).width
        == match WIDTH {
            PRINT_SLICE_WIDE => 2,
            _ => 1,
        }
}

/// The expected value of a simple cell (per the check mask built from
/// fieldMask in printSliceFill) that already has the given style
/// (so no ref-counting is needed).
#[inline]
fn print_slice_check_expected(style_id: style::Id) -> u64 {
    // var e: Cell = @bitCast(@as(u64, 0));
    // e.style_id = style_id;
    // return @bitCast(e);
    let mut e = pagepkg::Cell::from_bits(0);
    e.set_style_id(style_id);
    e.to_bits()
}

/// Store a run of narrow codepoint cells built from a bit template:
/// for each `idx` in `[from, to)`, `cells[idx]` is assigned the bits
/// `template_bits | (cps[idx] << cp_shift)`.
///
/// The template is a complete Cell (content tag, style, wide state,
/// etc. already baked in by the caller) whose codepoint content bits
/// are zero. Since Cell is a packed struct(u64), OR-ing a codepoint
/// into the content field's bit position yields a finished cell as a
/// single integer, keeping the loop pure data movement: no per-cell
/// field assignments and no branches.
///
/// This loop is manually vectorized: Zig 0.16 (LLVM 21) no longer
/// auto-vectorizes the scalar form the way Zig 0.15 (LLVM 20) did.
//
// PORT NOTE: `cells`/`cps` are RAW pointers, not `&mut [Cell]`/`&[u32]`, per
// the `rust-mut-args-strongly-protected` rule: the caller keeps its own raw
// alias to the same row for the scan loops that bracket this call, and a
// `&mut` argument here would be a Stacked-Borrows-protected retag that
// invalidates it.
///
/// # Safety
///
/// `cells[from..to]` and `cps[from..to]` must both be in bounds.
#[inline]
unsafe fn print_slice_store_run(
    cells: *mut pagepkg::Cell,
    cps: *const u32,
    from: usize,
    to: usize,
    template_bits: u64,
) {
    // The bit position of the `content` field within the packed
    // Cell. A codepoint occupies the low bits of `content`, so
    // shifting a codepoint left by this amount places it exactly
    // where `.content = .{ .codepoint = .{ .data = cp } }` would.
    const CP_SHIFT: u32 = pagepkg::CELL_CONTENT_SHIFT;

    // Since codepoints are OR'd into the content field rather than
    // assigned, any nonzero content bits in the template would
    // corrupt the stored codepoints.
    const CONTENT_MASK: u64 = ((1u64 << pagepkg::CELL_CONTENT_BITS) - 1) << CP_SHIFT;
    debug_assert!(template_bits & CONTENT_MASK == 0);

    let mut idx = from;

    // Vectorized bulk of the run.
    if let Some(lanes) = CELL_LANES {
        // The template and shift amount are loop-invariant, so
        // broadcast them to every lane once up front.
        //
        // PORT NOTE: `template_bits`/`CP_SHIFT` are already scalar constants of
        // the loop in this port, so the reference's explicit `@splat` of each
        // into a `@Vector(lanes, ...)` has no separate spelling here — LLVM
        // hoists and broadcasts them from the fixed-length window below.
        while idx + lanes <= to {
            // Load `lanes` decoded codepoints...
            //
            // ...widen each u32 lane to the u64 cell size...
            //
            // ...shift each codepoint into the content field's bit
            // position and merge with the template, producing
            // `lanes` finished cells (coerced from vector to array
            // so they can be bitcast for the store below)...
            //
            // ...and store them contiguously. Cell is a packed
            // struct(u64) so an array of u64 bit patterns has
            // identical layout to an array of cells.
            //
            // Written over a fixed-length window so LLVM emits the same vector
            // stores the reference's `@Vector(lanes, u64)` does.
            for j in 0..lanes {
                let cp = unsafe { *cps.add(idx + j) } as u64;
                unsafe {
                    *cells.add(idx + j) =
                        pagepkg::Cell::from_bits(template_bits | (cp << CP_SHIFT));
                }
            }
            idx += lanes;
        }
    }

    // Scalar tail: the final `< lanes` cells of the run, or the
    // entire run on targets without SIMD. Note `idx` carries over
    // from the vector loop above. This is the same computation as
    // the vector body, one cell at a time.
    while idx < to {
        let cp = unsafe { *cps.add(idx) } as u64;
        unsafe { *cells.add(idx) = pagepkg::Cell::from_bits(template_bits | (cp << CP_SHIFT)) };
        idx += 1;
    }
}

/// Store `(wide, spacer_tail)` cell pairs for `[from, to)` (which must be
/// pair-aligned).
///
/// Manually vectorized for the same reason as
/// printSliceStoreRun: build a vector of wide cells,
/// interleave with spacer tails, and store (wide,
/// spacer) pairs several at a time.
///
/// # Safety
///
/// `cells[from..to]` and `cps[from/2..to/2]` must both be in bounds.
#[inline]
unsafe fn print_slice_store_pairs(
    cells: *mut pagepkg::Cell,
    cps: *const u32,
    from: usize,
    to: usize,
    wide_bits: u64,
    spacer_bits: u64,
) {
    const CP_SHIFT: u32 = pagepkg::CELL_CONTENT_SHIFT;

    let mut idx = from;
    if let Some(lanes) = CELL_LANES {
        let pair_lanes = lanes / 2;
        while idx + 2 * pair_lanes <= to {
            for j in 0..pair_lanes {
                let cp = unsafe { *cps.add(idx / 2 + j) } as u64;
                unsafe {
                    *cells.add(idx + 2 * j) =
                        pagepkg::Cell::from_bits(wide_bits | (cp << CP_SHIFT));
                    *cells.add(idx + 2 * j + 1) = pagepkg::Cell::from_bits(spacer_bits);
                }
            }
            idx += 2 * pair_lanes;
        }
    }
    while idx < to {
        let cp = unsafe { *cps.add(idx / 2) } as u64;
        unsafe {
            *cells.add(idx) = pagepkg::Cell::from_bits(wide_bits | (cp << CP_SHIFT));
            *cells.add(idx + 1) = pagepkg::Cell::from_bits(spacer_bits);
        }
        idx += 2;
    }
}

// PORT NOTE: `vtStream` threads `self.gpa()` (a `std.mem.Allocator`) into the
// OSC parser via `Stream.initAlloc`, which in Rustty wants a
// `ZigAllocatorRef<'static>`. The reference `gpa()` returns the screen's
// general-purpose allocator (the global allocator). We back the OSC parser with
// a process-static `GlobalZigAllocator` (a stateless ZST) so the returned
// `ZigAllocatorRef` is `'static` and coerces to the stream's lifetime.
pub(crate) static GLOBAL_ZIG_ALLOCATOR: rustty_lib::allocator::GlobalZigAllocator =
    rustty_lib::allocator::GlobalZigAllocator;

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
        // PORT NOTE: the reference threads `self.gpa()` (a `std.mem.Allocator`)
        // into the stream options. Rustty's `stream::Options.allocator` wants a
        // `ZigAllocatorRef`; `gpa()` is the global allocator, so we back the OSC
        // parser with the process-static `GLOBAL_ZIG_ALLOCATOR` (see its note).
        //
        // PORT NOTE (parity tail S7-4): the OSC parser's captures (its dynamic
        // allocations) run on the runtime handle `capture_alloc`, which is set
        // to `gpa()` here, as the reference's `.allocator = self.gpa()` does.
        let gpa = self.gpa();
        let handler = self.vt_handler();
        let mut stream = Stream::init(crate::stream::Options {
            allocator: Some(rustty_lib::allocator::ZigAllocatorRef::new(
                &GLOBAL_ZIG_ALLOCATOR,
            )),
            handler,
            continuation_max_bytes: None,
        });
        stream.parser.osc_parser.capture_alloc = gpa;
        stream
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

    /// Print UTF-8 encoded string to the terminal.
    pub fn print_string(&mut self, str: &[u8]) -> Result<(), PrintStringError> {
        // const view = try std.unicode.Utf8View.init(str);
        // var it = view.iterator();
        // while (it.nextCodepoint()) |cp| { ... }
        // PORT NOTE: `std.unicode.Utf8View.init` validates UTF-8 then iterates
        // codepoints; an invalid sequence is `error.InvalidUtf8`. We decode via
        // `str::from_utf8` (the same validation) and iterate `chars()`.
        let view = core::str::from_utf8(str).map_err(|_| PrintStringError::InvalidUtf8)?;
        for cp in view.chars() {
            match cp {
                '\n' => {
                    self.carriage_return();
                    self.linefeed()?;
                }

                _ => self.print(cp as u32)?,
            }
        }
        Ok(())
    }

    /// Print the previous printed character a repeated amount of times.
    pub fn print_repeat(&mut self, count_req: usize) -> Result<(), OutOfMemory> {
        // const c = self.previous_char orelse return;
        let Some(c) = self.previous_char else {
            return Ok(());
        };
        // var remaining = @max(count_req, 1);
        let mut remaining = core::cmp::max(count_req, 1);

        // Print the repeated codepoint in slices so that eligible runs
        // take the batched printSlice fast path. printSlice is semantically
        // identical to calling print per codepoint: ineligible characters
        // or terminal states (insert mode, grapheme clustering, hyperlinks,
        // etc.) fall back to the per-codepoint print() path internally.
        //
        // The buffer is filled with a runtime-bounded loop rather than
        // `= @splat(c)`: a comptime-known 4096-element splat gets fully
        // unrolled into ~33KB of consecutive stores (LLVM won't re-roll
        // or vectorize it, see quirks_memset.zig), and it would fill the
        // whole buffer even for the typical small repeat counts.
        //
        // PORT NOTE: `var buf: [4096]u32 = undefined;` — Rust has no
        // `undefined` locals; the array is zero-initialized, and only the
        // `min(remaining, buf.len)` prefix the reference fills is ever read
        // (`print_slice` only sees `buf[0..n]` with `n <= remaining`).
        let mut buf: [u32; 4096] = [0; 4096];
        // for (buf[0..@min(remaining, buf.len)]) |*cp| cp.* = c;
        for cp in buf[0..core::cmp::min(remaining, 4096)].iter_mut() {
            *cp = c;
        }
        while remaining > 0 {
            // const n = @min(remaining, buf.len);
            let n = core::cmp::min(remaining, buf.len());
            self.print_slice(&buf[0..n])?;
            remaining -= n;
        }
        Ok(())
    }

    /// Print multiple codepoints to the terminal at once. This is
    /// semantically identical to calling `print` for each codepoint in
    /// order, but is much faster because it can batch cell writes and
    /// hoist per-codepoint checks out of the hot loop.
    ///
    /// The codepoints must all be printable: it is illegal for any
    /// codepoint in this slice to be a C0 control character. Therefore,
    /// this should only be called as a result of a proper VT parser
    /// (like our own).
    ///
    /// This is optimized for the common case: ASCII, soft-wrap, etc.
    /// Sequences of codepoints that require special handling (e.g. wide characters,
    /// grapheme clustering) are handled correctly but fall back to the
    /// slower per-codepoint path. They're less common and this is optimized
    /// for the aforementioned cases.
    pub fn print_slice(&mut self, cps: &[u32]) -> Result<(), OutOfMemory> {
        // Check if we can do the fast path up front. If we can't
        // we need to go back to scalar `print`.
        let fast = 'fast: {
            // Only the main display is supported.
            if self.status_display != ansi::StatusDisplay::Main {
                break 'fast false;
            }

            // Modes that require per-codepoint handling in print().
            // Wraparound is required (its the default) so that our
            // row-fill logic below can assume soft-wrap semantics. Insert
            // mode shifts cells per print.
            if self.modes.get(modespkg::Mode::Insert) {
                break 'fast false;
            }
            if !self.modes.get(modespkg::Mode::Wraparound) {
                break 'fast false;
            }

            // Charset must map ASCII as-is (true unless a DEC special
            // charset is actively invoked, which is rare).
            let screen: *mut Screen = self.screens.active;
            if unsafe { (*screen).charset.single_shift }.is_some() {
                break 'fast false;
            }
            match unsafe { (*screen).charset.charsets.get((*screen).charset.gl) } {
                charsets::Charset::Utf8 | charsets::Charset::Ascii => {}
                _ => break 'fast false,
            }

            // Hyperlinks require per-cell map bookkeeping.
            if unsafe { (*screen).cursor.hyperlink_id } != 0 {
                break 'fast false;
            }

            true
        };
        if !fast {
            // for (cps) |cp| try self.print(@intCast(cp));
            for &cp in cps {
                self.print(cp)?;
            }
            return Ok(());
        }

        let grapheme_cluster = self.modes.get(modespkg::Mode::GraphemeCluster);

        // When grapheme clustering is enabled and a left margin is set,
        // print() consults the cell left of the margin after wrapping,
        // which we can't reason about here. Restrict the fast path to
        // the [0x10, 0xFF] range in that case (those never cluster).
        let allow_unicode = !grapheme_cluster || self.scrolling_region.left == 0;

        let mut i: usize = 0;
        while i < cps.len() {
            // Try the fast-path print first. This will return the number of
            // codepoints it consumed.
            let consumed = self.print_slice_fast(&cps[i..], grapheme_cluster, allow_unicode)?;
            if consumed > 0 {
                i += consumed;
                continue;
            }

            // Consuming zero bytes means that the fast path can't handle
            // the next codepoint or the terminal is in a state we can't
            // fast-path. Fall back to the slow cp-by-cp print then try
            // fast paths again.
            self.print(cps[i])?;
            i += 1;
        }
        Ok(())
    }

    /// Attempt to print a prefix of `cps` using a batched fast path that
    /// writes cells directly. Returns the number of codepoints consumed.
    /// A return value of zero means the caller must print the first
    /// codepoint via the normal `print` path.
    ///
    /// The fast path handles runs of narrow (width 1) and wide (width 2)
    /// codepoints being written to simple cells. Everything else (zero
    /// width codepoints, grapheme cluster continuations, complex cells,
    /// etc.) is rejected so `print` can handle it with full generality.
    fn print_slice_fast(
        &mut self,
        cps: &[u32],
        grapheme_cluster: bool,
        allow_unicode: bool,
    ) -> Result<usize, OutOfMemory> {
        // const screen: *Screen = self.screens.active;
        let screen: *mut Screen = self.screens.active;

        // Codepoints in [0x10, 0xFF] are always narrow (width 1, matching
        // the c <= 0xFF fast path in print) and can never interact with
        // grapheme clustering (which requires a codepoint > 0xFF).
        //
        // Codepoints above 0xFF are batchable if their width is 1 or 2
        // (excluding zero-width characters such as combining marks, ZWJ,
        // and variation selectors) and, when grapheme clustering (mode
        // 2027) is enabled, if they are a grapheme break from the
        // previously printed codepoint (so print would never attach them
        // to the previous cell).

        // Codepoints in [0x10, 0xFF] are always narrow: print()
        // hardcodes width 1 for c <= 0xFF (no width table lookup).
        // They also can never interact with grapheme clustering,
        // which print() only performs for c > 0xFF, so they're
        // immediately eligible for the narrow fill with no further
        // checks.
        let cp0 = cps[0];
        if cp0 <= 0xFF {
            // C0 control characters (0x00-0x0F) aren't printable. The
            // stream never sends these (they're routed to execute), but
            // printSlice is a public API so defer to print() for safety.
            if cp0 < 0x10 {
                return Ok(0);
            }
            return self.print_slice_fill::<PRINT_SLICE_NARROW>(
                cps,
                grapheme_cluster,
                allow_unicode,
            );
        }

        if !allow_unicode {
            return Ok(0);
        }
        // if (comptime build_options.kitty_graphics) {
        //     // The Kitty graphics placeholder requires row bookkeeping.
        //     if (cp0 == kitty.graphics.unicode.placeholder) return 0;
        // }
        if crate::options::KITTY_GRAPHICS && cp0 == kitty::graphics_unicode::PLACEHOLDER {
            return Ok(0);
        }

        // The first codepoint requires care when grapheme clustering is
        // enabled: print() may attach it to the previous *cell* instead
        // of writing a new one. Take the first codepoint only when we can
        // determine — computing exactly what print() would — that it
        // starts a new cluster. At column zero with no pending wrap,
        // print() skips clustering entirely. Otherwise resolve the
        // previous cell the way print() does and check for a break.
        //
        // Note the pending-wrap rejection: print() may attach to the
        // pending cell *instead of wrapping*, which we can't model here.
        if grapheme_cluster && unsafe { (*screen).cursor.x } != 0 {
            'gate: {
                if unsafe { (*screen).cursor.pending_wrap } {
                    return Ok(0);
                }

                // Resolve the content cell to our left exactly like print():
                // if the immediate left cell is a wide spacer tail, the
                // content lives one further left. (A spacer tail can never
                // be at column zero — its wide half would have to be in the
                // previous row — so the second cursorCellLeft is in bounds.)
                let immediate: *mut pagepkg::Cell = unsafe { (*screen).cursor_cell_left(1) };
                let prev: *mut pagepkg::Cell = match unsafe { (*immediate).wide() } {
                    pagepkg::Wide::SpacerTail => unsafe { (*screen).cursor_cell_left(2) },
                    _ => immediate,
                };

                // An empty previous cell is necessarily a grapheme break.
                if unsafe { (*prev).codepoint() } == 0 {
                    break 'gate;
                }

                // Grapheme data on the previous cell requires the full
                // cluster state machine replay; only print() can do that.
                if unsafe { (*prev).has_grapheme() } {
                    return Ok(0);
                }

                // A simple single-codepoint previous cell: print() would run
                // exactly this break check from the default state.
                let mut state = unicode::grapheme::BreakState::Default;
                if !unicode::graphemeBreak(unsafe { (*prev).content_codepoint() }, cp0, &mut state)
                {
                    return Ok(0);
                }
            }
        } else if grapheme_cluster {
            // PORT NOTE: kept as the reference's nested `if` (not collapsed
            // into the `else if` condition) so the branch structure matches
            // line-for-line.
            #[allow(clippy::collapsible_if)]
            if unsafe { (*screen).cursor.pending_wrap } {
                return Ok(0);
            }
        }

        // The width lookup is a runtime value while printSliceFill is
        // specialized at comptime by width class, so this switch selects
        // between the two instantiations rather than passing the width
        // through as an argument.
        match unicode::table().get(cp0).width {
            1 => self.print_slice_fill::<PRINT_SLICE_NARROW>(cps, grapheme_cluster, allow_unicode),
            2 => self.print_slice_fill::<PRINT_SLICE_WIDE>(cps, grapheme_cluster, allow_unicode),
            _ => Ok(0),
        }
    }

    /// The row-filling portion of the printSlice fast path, specialized by
    /// width class. The first codepoint must already be validated by the
    /// caller (printSliceFast).
    fn print_slice_fill<const WIDTH: u8>(
        &mut self,
        cps: &[u32],
        grapheme_cluster: bool,
        allow_unicode: bool,
    ) -> Result<usize, OutOfMemory> {
        let screen: *mut Screen = self.screens.active;

        // PORT NOTE: the reference declares `simple_mask` and `cp_shift` here
        // and uses them in the row-fill body below; that body is
        // `print_slice_fill_inner` in this port (see its doc comment), so both
        // consts live there.

        // Determine the run of codepoints in the same width class that we
        // can batch. For codepoints after the first, the previous codepoint
        // in the run is always written as a fresh, single-codepoint cell,
        // so the grapheme break check against it is exact.
        let run_len: usize = 'run: {
            let mut idx: usize = 1;

            // Vectorized scan for the narrow class: codepoints in
            // [0x10, 0xFF] are always eligible with no further checks
            // and dominate real-world input, so scan for the first
            // codepoint outside that range several lanes at a time.
            // Anything else (including eligible unicode) proceeds via
            // the scalar loop below.
            if WIDTH == PRINT_SLICE_NARROW {
                if let Some(lanes) = CP_LANES {
                    // PERF(port): branchless per lane like the reference
                    // (vector range-compare -> @reduce -> @ctz); see the
                    // printable-run scan in `stream.rs` for the rationale —
                    // the early-exit form was 4.4x the reference's
                    // instruction count.
                    //
                    // The per-group test is the `@reduce(.And, in_range)`
                    // alone; the lane bitmask is built only on the group that
                    // ends the run, as the reference does (task S-4: building
                    // it on every group cost a scalar extract per lane on
                    // NEON, which has no movemask).
                    while idx + lanes <= cps.len() {
                        // const v: V = cps[idx..][0..lanes].*;
                        let window = &cps[idx..idx + lanes];
                        // const in_range = (v >= lo) & (v <= hi);
                        // if (!@reduce(.And, in_range)) {
                        //
                        // PERF(port): the `.And` reduction is spelled as an
                        // unsigned max of `v - 0x10` (every lane is in
                        // [0x10, 0xFF] iff the largest `v -% 0x10` is at most
                        // 0xEF), which LLVM lowers to one vector subtract and
                        // an across-lanes max; the boolean-AND spelling
                        // lowered to a partly scalar sequence here.
                        let mut max_off: u32 = 0;
                        for &v in window {
                            max_off = max_off.max(v.wrapping_sub(0x10));
                        }
                        if max_off > 0xFF - 0x10 {
                            // const bits = @bitCast(in_range);
                            // idx += @ctz(~bits);
                            // break;
                            let mut bad: u32 = 0;
                            for (j, &v) in window.iter().enumerate() {
                                bad |= ((!(0x10..=0xFF).contains(&v)) as u32) << j;
                            }
                            idx += bad.trailing_zeros() as usize;
                            break;
                        }
                        idx += lanes;
                    }
                }
            }

            while idx < cps.len() {
                let cp = cps[idx];
                if WIDTH == PRINT_SLICE_NARROW && (0x10..=0xFF).contains(&cp) {
                    idx += 1;
                    continue;
                }
                if cp > 0xFF && allow_unicode && print_slice_eligible::<WIDTH>(cp) {
                    if !grapheme_cluster {
                        idx += 1;
                        continue;
                    }
                    let mut state = unicode::grapheme::BreakState::Default;
                    if unicode::graphemeBreak(cps[idx - 1], cp, &mut state) {
                        idx += 1;
                        continue;
                    }
                }
                break 'run idx;
            }
            cps.len()
        };
        assert!(run_len > 0);

        // After doing any printing, wrapping, scrolling, etc. we want to
        // ensure that our screen remains in a consistent state.
        // defer screen.assertIntegrity();
        let result = self.print_slice_fill_inner::<WIDTH>(cps, run_len);
        unsafe { (*screen).assert_integrity() };
        result
    }

    /// The body of `print_slice_fill` after the run scan; split out only so
    /// the reference's `defer screen.assertIntegrity()` (which covers every
    /// exit below it, including the error return from `printWrap`) has a Rust
    /// model. Same split as `print`/`print_inner`.
    fn print_slice_fill_inner<const WIDTH: u8>(
        &mut self,
        cps: &[u32],
        run_len: usize,
    ) -> Result<usize, OutOfMemory> {
        // Our fast path can only handle "simple" cells. A simple cell is
        // a codepoint cell (no grapheme data or bg-color tag), narrow, and
        // not a hyperlink. The mask covers every field that must match
        // the expected value (see printSliceCheckExpected) exactly.
        const SIMPLE_MASK: u64 = pagepkg::cell_field_mask(&[
            pagepkg::CellField::ContentTag,
            pagepkg::CellField::StyleId,
            pagepkg::CellField::Wide,
            pagepkg::CellField::Hyperlink,
        ]);

        // The bit offset of the codepoint content within a Cell, used to
        // construct cell values from a template without field assignments.
        const CP_SHIFT: u32 = pagepkg::CELL_CONTENT_SHIFT;

        // The number of cells each codepoint occupies.
        // const cells_per_cp: usize = comptime width.cellsPerCp();
        let cells_per_cp: usize = print_slice_cells_per_cp(WIDTH);

        let mut printed: usize = 0;
        'outer: while printed < run_len {
            // If we're soft-wrapping, handle that first so that our cursor
            // is in the row/column that will receive the next codepoint.
            if unsafe { (*self.screens.active).cursor.pending_wrap } {
                self.print_wrap()?;
            }

            let screen: *mut Screen = self.screens.active;

            // Our right margin depends on where our cursor is now,
            // matching the logic in print().
            let right_limit: usize = if unsafe { (*screen).cursor.x } > self.scrolling_region.right
            {
                self.cols as usize
            } else {
                self.scrolling_region.right as usize + 1
            };

            // A degenerate 1-wide region can't hold a wide char; print()
            // has special handling so fall back to it.
            if WIDTH == PRINT_SLICE_WIDE && right_limit - self.scrolling_region.left as usize <= 1 {
                break;
            }

            // const cursor = &screen.cursor;
            let cursor: *mut crate::screen::Cursor = unsafe { &raw mut (*screen).cursor };
            let avail: usize = right_limit - unsafe { (*cursor).x } as usize;
            assert!(avail > 0);

            // The cursor caches live row and cell pointers into this mapping, so
            // its page cannot be compressed while this print path is active.
            // const page = cursor.page_pin.node.pageAssumeResident();
            //
            // PERF(port): `pageAssumeResident` like the reference — the
            // restore-capable `Node::page` was an out-of-line call (twice per
            // row) that also forced register spills around it.
            let page: *mut Page =
                unsafe { crate::pagelist::Node::page_assume_resident((*(*cursor).page_pin).node) };
            // const cells: [*]Cell = @ptrCast(cursor.page_cell);
            let cells: *mut pagepkg::Cell = unsafe { (*cursor).page_cell };
            let style_id = unsafe { (*cursor).style_id };
            let template: pagepkg::Cell = {
                let mut t = pagepkg::Cell::zeroed();
                t.set_content_tag(pagepkg::ContentTag::Codepoint);
                t.set_content_codepoint(0);
                t.set_style_id(style_id);
                t.set_wide(pagepkg::Wide::Narrow);
                t.set_protected(unsafe { (*cursor).protected });
                t.set_semantic_content(unsafe { (*cursor).semantic_content });
                t
            };
            let template_bits: u64 = template.to_bits();
            let check_expected: u64 = print_slice_check_expected(style_id);

            if WIDTH == PRINT_SLICE_WIDE && avail == 1 {
                // Only one cell left in the row: print() writes a
                // spacer head (or a blank narrow cell if we're inside
                // a right margin) and wraps. We require a simple cell,
                // otherwise fall back to print() for the cleanup.
                let bits: u64 = unsafe { (*cells).to_bits() };
                if (bits & SIMPLE_MASK) != check_expected {
                    break;
                }

                let mut spacer = template;
                if right_limit == self.cols as usize {
                    unsafe { (*(*cursor).page_row).set_wrap(true) };
                    spacer.set_wide(pagepkg::Wide::SpacerHead);
                }
                unsafe { (*(*cursor).page_row).set_dirty(true) };
                if style_id != style::DEFAULT_ID {
                    unsafe { (*(*cursor).page_row).set_styled(true) };
                }
                unsafe { *cells = spacer };
                self.print_wrap()?;
                continue 'outer;
            }

            // Number of codepoints and cells we're writing to this row.
            let count = core::cmp::min(avail / cells_per_cp, run_len - printed);
            assert!(count > 0);
            let cell_count = count * cells_per_cp;

            // Wide cells always come in (wide, spacer_tail) pairs.
            let spacer_bits: u64 = if WIDTH == PRINT_SLICE_WIDE {
                let mut spacer = template;
                spacer.set_wide(pagepkg::Wide::SpacerTail);
                spacer.to_bits()
            } else {
                0 // `undefined` in the reference; never read for the narrow class.
            };
            let wide_bits: u64 = if WIDTH == PRINT_SLICE_WIDE {
                let mut w = template;
                w.set_wide(pagepkg::Wide::Wide);
                w.to_bits()
            } else {
                0 // `undefined` in the reference; never read for the narrow class.
            };

            let mut k: usize = 0; // cells written
            'fill: while k < cell_count {
                // Find the run of simple cells so the store loop below is
                // branch-free (and vectorizable). This is an early-exit
                // search loop that LLVM won't auto-vectorize, and reused
                // rows typically match the whole way through, so scan
                // several cells at a time manually.
                let mut simple = k;
                {
                    if let Some(lanes) = CELL_LANES {
                        // PERF(port): the reference's `SimpleMask.eqlPrefix`
                        // idiom — a BRANCHLESS whole-group compare, paying
                        // the exact-index extraction only on a run's final
                        // group (here: falling through to the scalar tail,
                        // which walks at most lanes-1 cells). A per-lane
                        // early-exit `break` blocked vectorization.
                        while simple + lanes <= cell_count {
                            let mut all = true;
                            for j in 0..lanes {
                                let bits = unsafe { (*cells.add(simple + j)).to_bits() };
                                all &= (bits & SIMPLE_MASK) == check_expected;
                            }
                            if !all {
                                break;
                            }
                            simple += lanes;
                        }
                    }
                    while simple < cell_count {
                        let bits: u64 = unsafe { (*cells.add(simple)).to_bits() };
                        if (bits & SIMPLE_MASK) != check_expected {
                            break;
                        }
                        simple += 1;
                    }
                }

                if WIDTH == PRINT_SLICE_WIDE {
                    // We can only write whole (wide, spacer) pairs.
                    let pair_end = k + (simple - k) / 2 * 2;
                    unsafe {
                        print_slice_store_pairs(
                            cells,
                            cps.as_ptr().add(printed),
                            k,
                            pair_end,
                            wide_bits,
                            spacer_bits,
                        )
                    };
                    // If the simple run ended mid-pair we stop at the pair
                    // boundary and handle the offending cell below.
                    k = pair_end;
                    if simple != pair_end {
                        // The first cell of the next pair is simple but the
                        // second isn't; handle both via the general path.
                        //
                        // PORT NOTE: this assignment is dead in the reference
                        // too — nothing below reads `simple` and the next
                        // `fill` iteration re-derives it. Kept verbatim
                        // (rustc's `unused_assignments` allowed at the
                        // statement) rather than dropped, so the ported body
                        // matches the reference line for line.
                        #[allow(unused_assignments)]
                        {
                            simple = pair_end;
                        }
                    }
                } else {
                    unsafe {
                        print_slice_store_run(
                            cells,
                            cps.as_ptr().add(printed),
                            k,
                            simple,
                            template_bits,
                        )
                    };
                    k = simple;
                }
                if k >= cell_count {
                    break;
                }

                // Bulk path for runs of cells that differ from the
                // expected simple cell only by their style: this is the
                // common case when styled text overwrites previously
                // styled (or default-styled) rows, e.g. TUI redraws.
                // These runs are handled wholesale: one scan to find the
                // run of identical old styles, two ref-count updates,
                // and a branch-free fill.
                if WIDTH == PRINT_SLICE_NARROW {
                    'bulk: {
                        let first = unsafe { (*cells.add(k)).to_bits() } & SIMPLE_MASK;

                        // The old cell must be a plain narrow codepoint cell
                        // with no hyperlink whose only difference is the
                        // style id (see printSliceCheckExpected: every other
                        // masked field must be zero).
                        let style_shift = pagepkg::CELL_STYLE_ID_SHIFT;
                        let old_style: style::Id = (first >> style_shift) as style::Id;
                        if first != print_slice_check_expected(old_style) {
                            break 'bulk;
                        }
                        assert!(old_style != style_id); // it failed the simple check

                        // Find the run of cells with identical masked bits.
                        let mut m = k + 1;
                        {
                            if let Some(lanes) = CELL_LANES {
                                // PERF(port): branchless whole-group compare;
                                // see the simple-cell scan above.
                                while m + lanes <= cell_count {
                                    let mut all = true;
                                    for j in 0..lanes {
                                        let bits = unsafe { (*cells.add(m + j)).to_bits() };
                                        all &= (bits & SIMPLE_MASK) == first;
                                    }
                                    if !all {
                                        break;
                                    }
                                    m += lanes;
                                }
                            }
                            while m < cell_count {
                                if (unsafe { (*cells.add(m)).to_bits() } & SIMPLE_MASK) != first {
                                    break;
                                }
                                m += 1;
                            }
                        }

                        // Fix up the style ref counts for the whole run at
                        // once. Each of the old cells held a reference to
                        // old_style so the release is safe by construction.
                        let n = m - k;
                        unsafe {
                            let base = (*page).memory_ptr();
                            if old_style != style::DEFAULT_ID {
                                (*page)
                                    .styles
                                    .release_multiple(base, old_style, n as style::Id);
                            }
                            if style_id != style::DEFAULT_ID {
                                (*page).styles.use_multiple(base, style_id, n as style::Id);
                            }
                        }

                        unsafe {
                            print_slice_store_run(
                                cells,
                                cps.as_ptr().add(printed),
                                k,
                                m,
                                template_bits,
                            )
                        };
                        k = m;
                        continue 'fill;
                    }
                }

                // General path for cells that failed the masked check:
                // style-only mismatches are handled inline; anything that
                // needs cleanup (wide chars and their spacers, grapheme
                // data, hyperlinks) falls back to print().
                let general_count: usize = cells_per_cp;
                for offset in 0..general_count {
                    let cell = unsafe { &*cells.add(k + offset) };
                    if cell.wide() != pagepkg::Wide::Narrow
                        || cell.has_grapheme()
                        || cell.hyperlink()
                    {
                        break 'fill;
                    }
                }
                for offset in 0..general_count {
                    let cell = unsafe { &*cells.add(k + offset) };
                    if cell.style_id() != style_id {
                        unsafe {
                            let base = (*page).memory_ptr();
                            if cell.style_id() != style::DEFAULT_ID {
                                (*page).styles.release(base, cell.style_id());
                            }
                            if style_id != style::DEFAULT_ID {
                                (*page).styles.use_id(base, style_id);
                            }
                        }
                    }
                }
                if WIDTH == PRINT_SLICE_WIDE {
                    unsafe {
                        *cells.add(k) = pagepkg::Cell::from_bits(
                            wide_bits | ((cps[printed + k / 2] as u64) << CP_SHIFT),
                        );
                        *cells.add(k + 1) = pagepkg::Cell::from_bits(spacer_bits);
                    }
                } else {
                    unsafe {
                        *cells.add(k) = pagepkg::Cell::from_bits(
                            template_bits | ((cps[printed + k] as u64) << CP_SHIFT),
                        );
                    }
                }
                k += cells_per_cp;
            }

            if k > 0 {
                assert!(k.is_multiple_of(cells_per_cp));
                unsafe { (*(*cursor).page_row).set_dirty(true) };
                if style_id != style::DEFAULT_ID {
                    unsafe { (*(*cursor).page_row).set_styled(true) };
                }
                self.previous_char = Some(cps[printed + k / cells_per_cp - 1]);
                printed += k / cells_per_cp;

                // Advance the cursor. If we filled through the right limit
                // then the cursor stays on the last cell with the pending
                // wrap flag set, matching print().
                if unsafe { (*cursor).x } as usize + k >= right_limit {
                    assert!(unsafe { (*cursor).x } as usize + k == right_limit);
                    unsafe { (*screen).cursor_right((k - 1) as size::CellCountInt) };
                    unsafe { (*cursor).pending_wrap = true };
                } else {
                    unsafe { (*screen).cursor_right(k as size::CellCountInt) };
                }
            }

            // We hit a cell that requires the slow path. The cursor is
            // exactly at that cell so return and let the caller print the
            // next codepoint via print().
            if k < cell_count {
                break;
            }
        }

        Ok(printed)
    }

    pub fn print(&mut self, c: u32) -> Result<(), OutOfMemory> {
        // log.debug("print={x} y={} x={}", .{ c, self.screens.active.cursor.y, self.screens.active.cursor.x });

        // If we're not on the main display, do nothing for now
        if self.status_display != ansi::StatusDisplay::Main {
            return Ok(());
        }

        // After doing any printing, wrapping, scrolling, etc. we want to ensure
        // that our screen remains in a consistent state.
        // defer self.screens.active.assertIntegrity();
        // PORT NOTE: Rust has no `defer`; the reference's `defer
        // assertIntegrity()` covers every path after the status check, so we run
        // `print_inner` and assert on the way out (both Ok and Err).
        let result = self.print_inner(c);
        self.active_mut().assert_integrity();
        result
    }

    fn print_inner(&mut self, c: u32) -> Result<(), OutOfMemory> {
        let active: *mut Screen = self.screens.active;

        // Our right margin depends where our cursor is now.
        let right_limit = if unsafe { (*active).cursor.x } > self.scrolling_region.right {
            self.cols
        } else {
            self.scrolling_region.right + 1
        };

        // Perform grapheme clustering if grapheme support is enabled (mode 2027).
        // This is MUCH slower than the normal path so the conditional below is
        // purposely ordered in least-likely to most-likely so we can drop out
        // as quickly as possible.
        if c > 255
            && self.modes.get(modespkg::Mode::GraphemeCluster)
            && unsafe { (*active).cursor.x } > 0
            && self.print_inner_grapheme(c, right_limit)?
        {
            return Ok(());
        }
        let active: *mut Screen = self.screens.active;

        // Determine the width of this character so we can handle
        // non-single-width characters properly. We have a fast-path for
        // byte-sized characters since they're so common. We can ignore
        // control characters because they're always filtered prior.
        let width: usize = if c <= 0xFF {
            1
        } else {
            unicode::table().get(c).width as usize
        };

        // Note: it is possible to have a width of "3" and a width of "-1" from
        // uucode.x's wcwidth. We should look into those cases and handle them
        // appropriately.
        debug_assert!(width <= 2);
        // log.debug("c={x} width={}", .{ c, width });

        // Attach zero-width characters to our cell as grapheme data.
        if width == 0 {
            return self.print_inner_zero_width(c);
        }
        // We have a printable character, save it
        self.previous_char = Some(c);

        // If we're soft-wrapping, then handle that first.
        if unsafe { (*active).cursor.pending_wrap } && self.modes.get(modespkg::Mode::Wraparound) {
            self.print_wrap()?;
        }

        let active: *mut Screen = self.screens.active;

        // If we have insert mode enabled then we need to handle that. We
        // only do insert mode if we're not at the end of the line.
        if self.modes.get(modespkg::Mode::Insert)
            && unsafe { (*active).cursor.x } + (width as size::CellCountInt) < self.cols
        {
            self.insert_blanks(width);
        }

        let active: *mut Screen = self.screens.active;

        match width {
            // Single cell is very easy: just write in the cell
            1 => {
                self.active_mut().cursor_mark_dirty();
                self.print_cell(c, pagepkg::Wide::Narrow);
            }

            // Wide character requires a spacer. We print this by
            // using two cells: the first is flagged "wide" and has the
            // wide char. The second is guaranteed to be a spacer if
            // we're not at the end of the line.
            2 => {
                if self.print_inner_wide(c, right_limit)? {
                    return Ok(());
                }
            }
            _ => unreachable!(),
        }

        let active: *mut Screen = self.screens.active;

        // If we're at the column limit, then we need to wrap the next time.
        // In this case, we don't move the cursor.
        if unsafe { (*active).cursor.x } == right_limit - 1 {
            unsafe { (*active).cursor.pending_wrap = true };
            return Ok(());
        }

        // Move the cursor
        self.active_mut().cursor_right(1);
        Ok(())
    }

    // PERF(port) RECONCILED (WS10 T16, `47e26df60`): upstream's `print_slice`
    // fast path does NOT subsume this outlining, and BOTH STAY. They are
    // complementary, not alternatives:
    //   - `print_slice_fast`'s eligibility check is a WHOLE-BATCH gate. Every
    //     codepoint it rejects — zero-width marks, grapheme continuations,
    //     insert mode, DEC charsets, hyperlinks, the Kitty placeholder, wide
    //     chars in a 1-column region, and every cell needing real cleanup —
    //     is handed straight back to `print` one codepoint at a time
    //     (`print_slice`'s `self.print(cps[i])` arm, and `print_slice_fill`
    //     deliberately returns with the cursor parked on the offending cell).
    //     So `print`'s own hot path is exactly as hot as before for all
    //     non-batchable input, which is the input the outlining was measured
    //     on.
    //   - The outlining is about `print`'s FRAME (6 callee-saved pairs on
    //     every printed character); `print_slice` never enters `print` at all
    //     on the batched path, so it neither benefits from nor conflicts with
    //     the reshape.
    // Deleting the outlining to "match upstream" would be a silent
    // pessimisation of the fallback path with no upstream counterpart — the
    // reference has no equivalent frame cost because Zig inlines those blocks
    // differently. Recorded here rather than in a ledger because it is
    // discharged inside this workstream.
    //
    // PERF(port): the three cold blocks of `print` are outlined into
    // per-codepoint helpers so the hot narrow-print path (the overwhelming
    // majority of printed cells) does not carry their register pressure —
    // without this the fully-inlined slow arms force a 0xc0-byte frame with
    // 6 callee-saved pairs saved/restored on EVERY printed character
    // (instruction-level profile: >50% of `print` self-time). Bodies moved
    // VERBATIM from `print_inner` (same calls, same order, same error
    // mapping); only the block-exit control flow is encoded in the return
    // value. Same reshape precedent as r3-clone (PERF_LOG round 3).

    /// The grapheme clustering path of `print` (reference Terminal.zig:342-...,
    /// the `grapheme:` labeled block). Returns Ok(true) if `c` was consumed
    /// (attached to the previous cell / handled), Ok(false) if the normal
    /// print path must continue (the reference's `break :grapheme`).
    #[cold]
    #[inline(never)]
    fn print_inner_grapheme(
        &mut self,
        c: u32,
        right_limit: size::CellCountInt,
    ) -> Result<bool, OutOfMemory> {
        let active: *mut Screen = self.screens.active;
        'grapheme: {
            // We need the previous cell to determine if we're at a grapheme
            // break or not. If we are NOT, then we are still combining the
            // same grapheme, and will be appending to prev.cell. Otherwise, we are
            // in a new cell.
            // const Prev = struct { cell: *Cell, left: size.CellCountInt };
            let left: size::CellCountInt = {
                // If we have wraparound, then we use the prev col unless
                // there's a pending wrap, in which case we use the current.
                if self.modes.get(modespkg::Mode::Wraparound) {
                    u8::from(!unsafe { (*active).cursor.pending_wrap }) as size::CellCountInt
                } else if unsafe { (*active).cursor.x } != right_limit - 1 {
                    // If we do not have wraparound, the logic is trickier. If
                    // we're not on the last column, then we just use the previous
                    // column. Otherwise, we need to check if there is text to
                    // figure out if we're attaching to the prev or current.
                    1
                } else {
                    u8::from(unsafe { (*(*active).cursor.page_cell).codepoint() } == 0)
                        as size::CellCountInt
                }
            };

            // If the previous cell is a wide spacer tail, then we actually
            // want to use the cell before that because that has the actual
            // content.
            let immediate = self.active_mut().cursor_cell_left(left);
            let (mut prev_cell, prev_left): (*mut pagepkg::Cell, size::CellCountInt) =
                match unsafe { (*immediate).wide() } {
                    pagepkg::Wide::SpacerTail => {
                        (self.active_mut().cursor_cell_left(left + 1), left + 1)
                    }
                    _ => (immediate, left),
                };

            // If our cell has no content, then this is a new cell and
            // necessarily a grapheme break.
            if unsafe { (*prev_cell).codepoint() } == 0 {
                break 'grapheme;
            }

            let active: *mut Screen = self.screens.active;

            let mut previous_codepoint: u21 = unsafe { (*prev_cell).content_codepoint() };
            let grapheme_break = {
                let mut state: unicode::grapheme::BreakState =
                    unicode::grapheme::BreakState::Default;
                if unsafe { (*prev_cell).has_grapheme() } {
                    let page: *mut Page =
                        unsafe { crate::pagelist::Node::page((*(*active).cursor.page_pin).node) };
                    let cps = unsafe { (*page).lookup_grapheme(prev_cell) }.unwrap();
                    for &cp2 in cps {
                        // log.debug("cp1={x} cp2={x}", .{ previous_codepoint, cp2 });
                        // _ = unicode.graphemeBreak(previous_codepoint, cp2, &state);
                        //
                        // With mode 2027 disabled, zero-width codepoints are
                        // attached without applying grapheme boundary rules. If
                        // the mode is enabled later, an existing cell can
                        // therefore contain one or more breaks. Feed those breaks
                        // into the state machine so it can reset its context and
                        // determine the boundary for the new codepoint.
                        //
                        // PORT NOTE (oracle b287f6d1a, `P3-W1 T4`): this used to
                        // be `debug_assert!(!brk)`, which is exactly the
                        // assertion upstream deleted. The pre-existing DG4 note
                        // here — that the call MUTATES `state` and must run
                        // unconditionally outside the macro — was and remains
                        // correct; it is the ASSERT that was wrong, not the call.
                        // Keep the call.
                        let _brk = unicode::graphemeBreak(previous_codepoint, cp2, &mut state);
                        previous_codepoint = cp2;
                    }
                }

                // log.debug("cp1={x} cp2={x} end", .{ previous_codepoint, c });
                unicode::graphemeBreak(previous_codepoint, c, &mut state)
            };

            // If we can NOT break, this means that "c" is part of a grapheme
            // with the previous char.
            if !grapheme_break {
                // The width-decision kernel is shared with the buffered
                // `unicode::grapheme::grapheme_width` API so the streaming
                // printer and the measurement API stay in lockstep. The
                // `Ignore` result covers invalid emoji variation selectors:
                // the terminal does not store them, so we return without
                // appending (leaving break state / prev as they were).
                use unicode::grapheme::GraphemeWidthEffect;
                match unicode::grapheme::grapheme_width_effect(previous_codepoint, c) {
                    GraphemeWidthEffect::Ignore => return Ok(true),
                    GraphemeWidthEffect::Wide => 'wide: {
                        if unsafe { (*prev_cell).wide() } == pagepkg::Wide::Wide {
                            break 'wide;
                        }

                        // Move our cursor back to the previous. We'll move
                        // the cursor within this block to the proper location.
                        self.active_mut().cursor_left(prev_left);

                        let active: *mut Screen = self.screens.active;

                        // If we don't have space for the wide char, we need to
                        // insert spacers and wrap. We need special handling if the
                        // previous cell has grapheme data.
                        if unsafe { (*active).cursor.x } == right_limit - 1 {
                            if !self.modes.get(modespkg::Mode::Wraparound) {
                                return Ok(true);
                            }

                            // This path can write a spacer_head before printWrap
                            // which can trigger integrity violations so mark
                            // the wrap first to keep the intermediary state valid
                            // if we're wrapping.
                            let row_wrap = right_limit == self.cols;
                            if row_wrap {
                                unsafe { (*(*active).cursor.page_row).set_wrap(true) };
                            }

                            let prev_cp = unsafe { (*prev_cell).content_codepoint() };
                            if unsafe { (*prev_cell).has_grapheme() } {
                                // This is like printCell but without clearing the
                                // grapheme data from the cell, so we can move it
                                // later.
                                unsafe {
                                    (*prev_cell).set_wide(if row_wrap {
                                        pagepkg::Wide::SpacerHead
                                    } else {
                                        pagepkg::Wide::Narrow
                                    })
                                };
                                unsafe { (*prev_cell).set_content_codepoint(0) };

                                self.print_wrap()?;
                                self.print_cell(prev_cp, pagepkg::Wide::Wide);

                                let active: *mut Screen = self.screens.active;
                                let new_pin = unsafe { *(*active).cursor.page_pin };
                                let new_rac = new_pin.row_and_cell();

                                'transfer_graphemes: {
                                    let mut old_pin = match new_pin.up(1) {
                                        Some(p) => p,
                                        None => break 'transfer_graphemes,
                                    };
                                    old_pin.x = right_limit - 1;
                                    let old_rac = old_pin.row_and_cell();

                                    if std::ptr::eq(new_pin.node, old_pin.node) {
                                        let page: *mut Page =
                                            unsafe { crate::pagelist::Node::page(new_pin.node) };
                                        unsafe {
                                            (*page).move_grapheme(old_rac.cell, new_rac.cell)
                                        };
                                        unsafe {
                                            (*old_rac.cell)
                                                .set_content_tag(pagepkg::ContentTag::Codepoint)
                                        };
                                        unsafe {
                                            (*new_rac.cell).set_content_tag(
                                                pagepkg::ContentTag::CodepointGrapheme,
                                            )
                                        };
                                        unsafe { (*new_rac.row).set_grapheme(true) };
                                    } else {
                                        let old_page: *mut Page =
                                            unsafe { crate::pagelist::Node::page(old_pin.node) };
                                        // PORT NOTE: the reference iterates the
                                        // old page's grapheme slice in place. Rust
                                        // cannot hold that borrow across
                                        // `append_grapheme` (which takes
                                        // `&mut Screen`), so we keep it as a raw
                                        // ptr+len (the port's raw-pointer doctrine)
                                        // rather than copying it into a `Vec`: a
                                        // copy would add a Rust-only allocation
                                        // whose failure ABORTS instead of
                                        // propagating as `OutOfMemory`. This is
                                        // sound for exactly the reason the
                                        // reference's in-place iteration is: this
                                        // arm requires `old_pin.node !=
                                        // new_pin.node`, and `Screen::append_grapheme`
                                        // only mutates/reallocates the CURSOR's
                                        // node, so the old page (a separate
                                        // allocation) is untouched across the loop.
                                        let (cps_ptr, cps_len): (*const u21, usize) = {
                                            let cps = unsafe {
                                                (*old_page).lookup_grapheme(old_rac.cell)
                                            }
                                            .unwrap();
                                            (cps.as_ptr(), cps.len())
                                        };
                                        for i in 0..cps_len {
                                            let cp: u21 = unsafe { *cps_ptr.add(i) };
                                            // appendGrapheme can grow the cursor
                                            // page, so read the destination from
                                            // the cursor each time rather than
                                            // holding a pointer across the call.
                                            let dst: *mut pagepkg::Cell =
                                                unsafe { (*self.screens.active).cursor.page_cell };
                                            self.active_mut()
                                                .append_grapheme(dst, cp)
                                                .map_err(|_| OutOfMemory)?;
                                        }
                                        let old_page: *mut Page =
                                            unsafe { crate::pagelist::Node::page(old_pin.node) };
                                        unsafe { (*old_page).clear_grapheme(old_rac.cell) };
                                    }

                                    let old_page: *mut Page =
                                        unsafe { crate::pagelist::Node::page(old_pin.node) };
                                    unsafe { (*old_page).update_row_grapheme_flag(old_rac.row) };
                                }

                                // Point prev.cell to our new previous cell that
                                // we'll be appending graphemes to
                                prev_cell = unsafe { (*self.screens.active).cursor.page_cell };
                            } else {
                                self.print_cell(
                                    0,
                                    if row_wrap {
                                        pagepkg::Wide::SpacerHead
                                    } else {
                                        pagepkg::Wide::Narrow
                                    },
                                );
                                self.print_wrap()?;
                                self.print_cell(prev_cp, pagepkg::Wide::Wide);

                                // Point prev.cell to our new previous cell that
                                // we'll be appending graphemes to
                                prev_cell = unsafe { (*self.screens.active).cursor.page_cell };
                            }
                        } else {
                            unsafe { (*prev_cell).set_wide(pagepkg::Wide::Wide) };
                        }

                        // Write our spacer, since prev.cell is now wide
                        self.active_mut().cursor_right(1);

                        // Writing the spacer can grow the page to make room for
                        // the cursor hyperlink. Growing replaces the page, which
                        // invalidates `prev.cell`. Record the page identity first
                        // so the common case where nothing grows stays free.
                        //
                        // A pointer comparison alone isn't enough: pages are
                        // pooled, so a replacement can reuse the same address.
                        // The serial makes the pair a unique identity.
                        let spacer_node: *mut crate::pagelist::Node =
                            unsafe { (*(*self.screens.active).cursor.page_pin).node };
                        let spacer_serial = unsafe { (*spacer_node).serial };

                        self.print_cell(0, pagepkg::Wide::SpacerTail);

                        let active: *mut Screen = self.screens.active;
                        if !std::ptr::eq(unsafe { (*(*active).cursor.page_pin).node }, spacer_node)
                            || unsafe { (*(*(*active).cursor.page_pin).node).serial }
                                != spacer_serial
                        {
                            // @branchHint(.unlikely);

                            // The cursor is on the spacer tail we just wrote, so
                            // the wide cell we append to is the one to its left.
                            prev_cell = self.active_mut().cursor_cell_left(1);
                        }

                        // Move the cursor again so we're beyond our spacer
                        let active: *mut Screen = self.screens.active;
                        if unsafe { (*active).cursor.x } == right_limit - 1 {
                            unsafe { (*active).cursor.pending_wrap = true };
                        } else {
                            self.active_mut().cursor_right(1);
                        }
                    }

                    GraphemeWidthEffect::Narrow => 'narrow: {
                        // Prev cell is no longer wide
                        if unsafe { (*prev_cell).wide() } != pagepkg::Wide::Wide {
                            break 'narrow;
                        }
                        unsafe { (*prev_cell).set_wide(pagepkg::Wide::Narrow) };

                        // Remove the wide spacer tail. The previous cell may be
                        // under the cursor, so locate the tail from the wide base
                        // rather than by subtracting from the cursor distance.
                        let prev_x = unsafe { (*self.screens.active).cursor.x } - prev_left;
                        if prev_x < self.cols - 1 {
                            // const cells: [*]Cell = @ptrCast(prev.cell);
                            // cells[1].wide = .narrow;
                            unsafe { (*prev_cell.add(1)).set_wide(pagepkg::Wide::Narrow) };
                        }

                        // Place the cursor one cell after the now-narrow base,
                        // clamped to the right edge. Usually this moves the cursor
                        // back from after the old tail, but saved cursor state or
                        // changed margins can leave it directly on the base.
                        unsafe { (*self.screens.active).cursor.pending_wrap = false };
                        self.active_mut()
                            .cursor_horizontal_absolute((prev_x + 1).min(right_limit - 1));

                        #[allow(clippy::needless_return)]
                        {
                            break 'narrow;
                        }
                    }

                    GraphemeWidthEffect::NoChange => {}
                }

                // log.debug("c={X} grapheme attach to left={} primary_cp={X}", ...);
                self.active_mut().cursor_mark_dirty();
                self.active_mut()
                    .append_grapheme(prev_cell, c)
                    .map_err(|_| OutOfMemory)?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// The zero-width attach path of `print` (reference Terminal.zig `if
    /// (width == 0)` block). Every exit consumes the codepoint.
    #[cold]
    #[inline(never)]
    fn print_inner_zero_width(&mut self, c: u32) -> Result<(), OutOfMemory> {
        let active: *mut Screen = self.screens.active;

        // If we have grapheme clustering enabled, we don't blindly attach
        // any zero width character to our cells and we instead just ignore
        // it.
        if self.modes.get(modespkg::Mode::GraphemeCluster) {
            return Ok(());
        }

        // If we have wraparound enabled and a pending wrap, the character
        // we're attaching to is still under the cursor. Otherwise, it's the
        // cell to the left.
        let left: size::CellCountInt = if self.modes.get(modespkg::Mode::Wraparound)
            && unsafe { (*active).cursor.pending_wrap }
        {
            0
        } else {
            1
        };

        // If we're at cell zero and not pending a wrap, then this is malformed
        // data and we don't print anything or even store this. Zero-width
        // characters are ALWAYS attached to some other non-zero-width
        // character at the time of writing.
        if unsafe { (*active).cursor.x } == 0 && left == 1 {
            log::warn!(target: LOG_SCOPE, "zero-width character with no prior character, ignoring");
            return Ok(());
        }

        // Find our previous cell
        let prev: *mut pagepkg::Cell = {
            let immediate = self.active_mut().cursor_cell_left(left);
            if unsafe { (*immediate).wide() } != pagepkg::Wide::SpacerTail {
                immediate
            } else {
                self.active_mut().cursor_cell_left(left + 1)
            }
        };

        // If our previous cell has no text, just ignore the zero-width character
        if !unsafe { (*prev).has_text() } {
            log::warn!(target: LOG_SCOPE, "zero-width character with no prior character, ignoring");
            return Ok(());
        }

        // If this is a emoji variation selector, prev must be an emoji
        if c == 0xFE0F || c == 0xFE0E {
            let prev_props = unicode::table().get(unsafe { (*prev).content_codepoint() });
            let emoji = prev_props.grapheme_break
                == unicode::props::GraphemeBreakNoControl::ExtendedPictographic;
            if !emoji {
                return Ok(());
            }
        }

        self.active_mut()
            .append_grapheme(prev, c)
            .map_err(|_| OutOfMemory)?;
        Ok(())
    }

    /// The wide-character arm of `print` (reference Terminal.zig `2 => ...`).
    /// Returns Ok(true) for the xterm no-wraparound early return, Ok(false)
    /// to continue to the shared cursor-advance tail.
    #[cold]
    #[inline(never)]
    fn print_inner_wide(
        &mut self,
        c: u32,
        right_limit: size::CellCountInt,
    ) -> Result<bool, OutOfMemory> {
        let active: *mut Screen = self.screens.active;

        if (right_limit - self.scrolling_region.left) > 1 {
            // If we don't have space for the wide char, we need
            // to insert spacers and wrap. Then we just print the wide
            // char as normal.
            if unsafe { (*active).cursor.x } == right_limit - 1 {
                // If we don't have wraparound enabled then we don't print
                // this character at all and don't move the cursor. This is
                // how xterm behaves.
                if !self.modes.get(modespkg::Mode::Wraparound) {
                    return Ok(true);
                }

                // We only create a spacer head if we're at the real edge
                // of the screen. Otherwise, we clear the space with a narrow.
                // This allows soft wrapping to work correctly.
                if right_limit == self.cols {
                    // Special-case: we need to set wrap to true even
                    // though we call printWrap below because if there is
                    // a page resize during printCell then it'll fail
                    // integrity checks.
                    unsafe { (*(*active).cursor.page_row).set_wrap(true) };
                    self.print_cell(0, pagepkg::Wide::SpacerHead);
                } else {
                    self.print_cell(0, pagepkg::Wide::Narrow);
                }
                self.print_wrap()?;
            }

            self.active_mut().cursor_mark_dirty();
            self.print_cell(c, pagepkg::Wide::Wide);
            self.active_mut().cursor_right(1);
            self.print_cell(0, pagepkg::Wide::SpacerTail);
        } else {
            // This is pretty broken, terminals should never be only 1-wide.
            // We should prevent this downstream.
            self.active_mut().cursor_mark_dirty();
            self.print_cell(0, pagepkg::Wide::Narrow);
        }

        Ok(false)
    }

    fn print_cell(&mut self, unmapped_c: u32, wide: pagepkg::Wide) {
        // defer self.screens.active.assertIntegrity();

        // TODO: spacers should use a bgcolor only cell

        let active: *mut Screen = self.screens.active;

        let c: u32 = {
            // TODO: non-utf8 handling, gr

            // If we're single shifting, then we use the key exactly once.
            let key = if let Some(key_once) = unsafe { (*active).charset.single_shift } {
                unsafe { (*active).charset.single_shift = None };
                key_once
            } else {
                unsafe { (*active).charset.gl }
            };

            let set = unsafe { (*active).charset.charsets.get(key) };

            // UTF-8 or ASCII is used as-is
            if set == charsets::Charset::Utf8 || set == charsets::Charset::Ascii {
                unmapped_c
            } else if unmapped_c > u8::MAX as u32 {
                // If we're outside of ASCII range this is an invalid value in
                // this table so we just return space.
                ' ' as u32
            } else {
                // Get our lookup table and map it
                let table = charsets::table(set);
                table[unmapped_c as usize] as u32
            }
        };

        let cell: *mut pagepkg::Cell = unsafe { (*active).cursor.page_cell };

        // If the wide property of this cell is the same, then we don't
        // need to do the special handling here because the structure will
        // be the same. If it is NOT the same, then we may need to clear some
        // cells.
        if unsafe { (*cell).wide() } != wide {
            match unsafe { (*cell).wide() } {
                // Previous cell was narrow. Do nothing.
                pagepkg::Wide::Narrow => {}

                // Previous cell was wide. We need to clear the tail and head.
                pagepkg::Wide::Wide => 'wide: {
                    if unsafe { (*active).cursor.x } >= self.cols - 1 {
                        break 'wide;
                    }

                    let spacer_cell = self.active_mut().cursor_cell_right(1);
                    let active: *mut Screen = self.screens.active;
                    let page: *mut Page =
                        unsafe { crate::pagelist::Node::page((*(*active).cursor.page_pin).node) };
                    let row = unsafe { (*active).cursor.page_row };
                    self.active_mut().clear_cells(
                        page,
                        row,
                        core::ptr::slice_from_raw_parts_mut(spacer_cell, 1),
                    );

                    // If we're near the left edge, a wide char may have
                    // wrapped from the previous row, leaving a spacer_head
                    // at the end of that row. Clear it so the previous row
                    // doesn't keep a stale spacer_head.
                    let active: *mut Screen = self.screens.active;
                    if unsafe { (*active).cursor.y } > 0 && unsafe { (*active).cursor.x } <= 1 {
                        let head_cell = self.active_mut().cursor_cell_end_of_prev();
                        if unsafe { (*head_cell).wide() } == pagepkg::Wide::SpacerHead {
                            unsafe { (*head_cell).set_wide(pagepkg::Wide::Narrow) };
                        }
                    }
                }

                pagepkg::Wide::SpacerTail => {
                    debug_assert!(unsafe { (*active).cursor.x } > 0);

                    // So integrity checks pass. We fix this up later so we don't
                    // need to do this without safety checks.
                    if cfg!(debug_assertions) {
                        unsafe { (*cell).set_wide(pagepkg::Wide::Narrow) };
                    }

                    let wide_cell = self.active_mut().cursor_cell_left(1);
                    let active: *mut Screen = self.screens.active;
                    let page: *mut Page =
                        unsafe { crate::pagelist::Node::page((*(*active).cursor.page_pin).node) };
                    let row = unsafe { (*active).cursor.page_row };
                    self.active_mut().clear_cells(
                        page,
                        row,
                        core::ptr::slice_from_raw_parts_mut(wide_cell, 1),
                    );
                    // If we're near the left edge, a wide char may have
                    // wrapped from the previous row, leaving a spacer_head
                    // at the end of that row. Clear it so the previous row
                    // doesn't keep a stale spacer_head.
                    let active: *mut Screen = self.screens.active;
                    if unsafe { (*active).cursor.y } > 0 && unsafe { (*active).cursor.x } <= 1 {
                        let head_cell = self.active_mut().cursor_cell_end_of_prev();
                        if unsafe { (*head_cell).wide() } == pagepkg::Wide::SpacerHead {
                            unsafe { (*head_cell).set_wide(pagepkg::Wide::Narrow) };
                        }
                    }
                }

                // TODO: this case was not handled in the old terminal implementation
                // but it feels like we should do something. investigate other
                // terminals (xterm mainly) and see what's up.
                pagepkg::Wide::SpacerHead => {}
            }
        }

        let active: *mut Screen = self.screens.active;

        // If the prior value had graphemes, clear those
        if unsafe { (*cell).has_grapheme() } {
            let page: *mut Page =
                unsafe { crate::pagelist::Node::page((*(*active).cursor.page_pin).node) };
            unsafe { (*page).clear_grapheme(cell) };
            unsafe { (*page).update_row_grapheme_flag((*active).cursor.page_row) };
        }

        // We don't need to update the style refs unless the
        // cell's new style will be different after writing.
        let style_changed = unsafe { (*cell).style_id() } != unsafe { (*active).cursor.style_id };
        if style_changed {
            let page: *mut Page =
                unsafe { crate::pagelist::Node::page((*(*active).cursor.page_pin).node) };

            // Release the old style.
            if unsafe { (*cell).style_id() } != style::DEFAULT_ID {
                debug_assert!(unsafe { (*(*active).cursor.page_row).styled() });
                unsafe {
                    let base = (*page).memory_ptr();
                    (*page).styles.release(base, (*cell).style_id());
                }
            }
        }

        // Keep track if we had a hyperlink so we can unset it.
        let had_hyperlink = unsafe { (*cell).hyperlink() };

        // Write
        let mut new_cell = pagepkg::Cell::zeroed();
        new_cell.set_content_tag(pagepkg::ContentTag::Codepoint);
        new_cell.set_content_codepoint(c as u21);
        new_cell.set_style_id(unsafe { (*active).cursor.style_id });
        new_cell.set_wide(wide);
        new_cell.set_protected(unsafe { (*active).cursor.protected });
        new_cell.set_semantic_content(unsafe { (*active).cursor.semantic_content });
        unsafe { *cell = new_cell };

        if style_changed {
            let page: *mut Page =
                unsafe { crate::pagelist::Node::page((*(*active).cursor.page_pin).node) };

            // Use the new style.
            if unsafe { (*cell).style_id() } != style::DEFAULT_ID {
                unsafe {
                    let base = (*page).memory_ptr();
                    (*page).styles.use_id(base, (*cell).style_id());
                }
                unsafe { (*(*active).cursor.page_row).set_styled(true) };
            }
        }

        // If this is a Kitty unicode placeholder then we need to mark the
        // row so that the renderer can lookup rows with these much faster.
        if crate::options::KITTY_GRAPHICS && c == kitty::graphics_unicode::PLACEHOLDER {
            unsafe { (*(*active).cursor.page_row).set_kitty_virtual_placeholder(true) };
        }

        // We check for an active hyperlink first because setHyperlink
        // handles clearing the old hyperlink and an optimization if we're
        // overwriting the same hyperlink.
        if unsafe { (*active).cursor.hyperlink_id } > 0 {
            if let Err(err) = self.active_mut().cursor_set_hyperlink() {
                // @branchHint(.unlikely);
                log::warn!(
                    target: LOG_SCOPE,
                    "error reallocating for more hyperlink space, ignoring hyperlink err={err:?}"
                );

                // A partially successful grow can replace the page even when the
                // call fails, so `cell` may be stale here. The cursor pointers are
                // always reloaded, so read the cell through the cursor.
                debug_assert!(!unsafe { (*(*active).cursor.page_cell).hyperlink() });
            }
        } else if had_hyperlink {
            // If the previous cell had a hyperlink then we need to clear it.
            let page: *mut Page =
                unsafe { crate::pagelist::Node::page((*(*active).cursor.page_pin).node) };
            unsafe { (*page).clear_hyperlink(cell) };
            unsafe { (*page).update_row_hyperlink_flag((*active).cursor.page_row) };
        }

        self.active_mut().assert_integrity();
    }

    fn print_wrap(&mut self) -> Result<(), OutOfMemory> {
        // We only mark that we soft-wrapped if we're at the edge of our
        // full screen. We don't mark the row as wrapped if we're in the
        // middle due to a right margin.
        // const cursor: *Screen.Cursor = &self.screens.active.cursor;
        // const mark_wrap = cursor.x == self.cols - 1;
        // if (mark_wrap) cursor.page_row.wrap = true;
        let cols = self.cols;
        let active: *mut Screen = self.screens.active;
        let mark_wrap = unsafe { (*active).cursor.x } == cols - 1;
        if mark_wrap {
            unsafe { (*(*active).cursor.page_row).set_wrap(true) };
        }

        // Get the old semantic prompt so we can extend it to the next
        // line. We need to do this before we index() because we may
        // modify memory.
        let old_semantic = unsafe { (*active).cursor.semantic_content };
        let old_semantic_clear = unsafe { (*active).cursor.semantic_content_clear_eol };

        // Move to the next line
        self.index()?;
        let active: *mut Screen = self.screens.active;
        let sr_left = self.scrolling_region.left;
        self.active_mut().cursor_horizontal_absolute(sr_left);

        // Our pointer should never move
        // assert(cursor == &self.screens.active.cursor);

        // We always reset our semantic prompt state
        unsafe { (*active).cursor.semantic_content = old_semantic };
        unsafe { (*active).cursor.semantic_content_clear_eol = old_semantic_clear };
        match old_semantic {
            pagepkg::SemanticContent::Output | pagepkg::SemanticContent::Input => {}
            pagepkg::SemanticContent::Prompt => unsafe {
                (*(*active).cursor.page_row)
                    .set_semantic_prompt(pagepkg::SemanticPrompt::PromptContinuation)
            },
        }

        if mark_wrap {
            let row = unsafe { (*active).cursor.page_row };
            // Always mark the row as a continuation
            unsafe { (*row).set_wrap_continuation(true) };
        }

        // Assure that our screen is consistent
        self.active_mut().assert_integrity();
        Ok(())
    }

    /// Set the charset into the given slot.
    pub fn configure_charset(&mut self, slot: charsets::Slots, set: charsets::Charset) {
        // self.screens.active.charset.charsets.set(slot, set);
        self.active_mut().charset.charsets.set(slot, set);
    }

    /// Invoke the charset in slot into the active slot. If single is true,
    /// then this will only be invoked for a single character.
    pub fn invoke_charset(
        &mut self,
        active: charsets::ActiveSlot,
        slot: charsets::Slots,
        single: bool,
    ) {
        if single {
            debug_assert!(active == charsets::ActiveSlot::GL);
            self.active_mut().charset.single_shift = Some(slot);
            return;
        }

        match active {
            charsets::ActiveSlot::GL => self.active_mut().charset.gl = slot,
            charsets::ActiveSlot::GR => self.active_mut().charset.gr = slot,
        }
    }

    /// Carriage return moves the cursor to the first column.
    pub fn carriage_return(&mut self) {
        // Always reset pending wrap state
        // self.screens.active.cursor.pending_wrap = false;
        unsafe { (*self.screens.active).cursor.pending_wrap = false };

        // In origin mode we always move to the left margin
        // self.screens.active.cursorHorizontalAbsolute(if (self.modes.get(.origin))
        //     self.scrolling_region.left
        // else if (self.screens.active.cursor.x >= self.scrolling_region.left)
        //     self.scrolling_region.left
        // else
        //     0);
        // PORT NOTE: the reference's first two branches both evaluate to
        // `scrolling_region.left`; this is intentional verbatim structure (origin
        // mode vs. cursor-at-or-right-of-left-margin are distinct conditions that
        // happen to share a result), so we silence `if_same_then_else`.
        #[allow(clippy::if_same_then_else)]
        let x = if self.modes.get(modespkg::Mode::Origin) {
            self.scrolling_region.left
        } else if unsafe { (*self.screens.active).cursor.x } >= self.scrolling_region.left {
            self.scrolling_region.left
        } else {
            0
        };
        self.active_mut().cursor_horizontal_absolute(x);
    }

    /// Linefeed moves the cursor to the next line.
    pub fn linefeed(&mut self) -> Result<(), OutOfMemory> {
        // try self.index();
        // if (self.modes.get(.linefeed)) self.carriageReturn();
        self.index()?;
        if self.modes.get(modespkg::Mode::Linefeed) {
            self.carriage_return();
        }
        Ok(())
    }

    /// Backspace moves the cursor back a column (but not less than 0).
    pub fn backspace(&mut self) {
        // self.cursorLeft(1);
        self.cursor_left(1);
    }

    /// Move the cursor up amount lines. If amount is greater than the maximum
    /// move distance then it is internally adjusted to the maximum. If amount is
    /// 0, adjust it to 1.
    pub fn cursor_up(&mut self, count_req: usize) {
        // Always resets pending wrap
        // self.screens.active.cursor.pending_wrap = false;
        unsafe { (*self.screens.active).cursor.pending_wrap = false };

        // The maximum amount the cursor can move up depends on scrolling regions
        // const max = if (self.screens.active.cursor.y >= self.scrolling_region.top)
        //     self.screens.active.cursor.y - self.scrolling_region.top
        // else
        //     self.screens.active.cursor.y;
        let cursor_y = unsafe { (*self.screens.active).cursor.y };
        let max: usize = if cursor_y >= self.scrolling_region.top {
            (cursor_y - self.scrolling_region.top) as usize
        } else {
            cursor_y as usize
        };
        // const count = @min(max, @max(count_req, 1));
        let count = core::cmp::min(max, core::cmp::max(count_req, 1));

        // We can safely intCast below because of the min/max clamping we did above.
        // self.screens.active.cursorUp(@intCast(count));
        self.active_mut().cursor_up(count as size::CellCountInt);
    }

    /// Move the cursor down amount lines. If amount is greater than the maximum
    /// move distance then it is internally adjusted to the maximum. This sequence
    /// will not scroll the screen or scroll region. If amount is 0, adjust it to 1.
    pub fn cursor_down(&mut self, count_req: usize) {
        // Always resets pending wrap
        // self.screens.active.cursor.pending_wrap = false;
        unsafe { (*self.screens.active).cursor.pending_wrap = false };

        // The max the cursor can move to depends where the cursor currently is
        // const max = if (self.screens.active.cursor.y <= self.scrolling_region.bottom)
        //     self.scrolling_region.bottom - self.screens.active.cursor.y
        // else
        //     self.rows - self.screens.active.cursor.y - 1;
        let cursor_y = unsafe { (*self.screens.active).cursor.y };
        let max: usize = if cursor_y <= self.scrolling_region.bottom {
            (self.scrolling_region.bottom - cursor_y) as usize
        } else {
            (self.rows - cursor_y - 1) as usize
        };
        // const count = @min(max, @max(count_req, 1));
        let count = core::cmp::min(max, core::cmp::max(count_req, 1));
        // self.screens.active.cursorDown(@intCast(count));
        self.active_mut().cursor_down(count as size::CellCountInt);
    }

    /// Move the cursor right amount columns. If amount is greater than the
    /// maximum move distance then it is internally adjusted to the maximum.
    /// This sequence will not scroll the screen or scroll region. If amount is
    /// 0, adjust it to 1.
    pub fn cursor_right(&mut self, count_req: usize) {
        // Always resets pending wrap
        // self.screens.active.cursor.pending_wrap = false;
        unsafe { (*self.screens.active).cursor.pending_wrap = false };

        // The max the cursor can move to depends where the cursor currently is
        // const max = if (self.screens.active.cursor.x <= self.scrolling_region.right)
        //     self.scrolling_region.right - self.screens.active.cursor.x
        // else
        //     self.cols - self.screens.active.cursor.x - 1;
        let cursor_x = unsafe { (*self.screens.active).cursor.x };
        let max: usize = if cursor_x <= self.scrolling_region.right {
            (self.scrolling_region.right - cursor_x) as usize
        } else {
            (self.cols - cursor_x - 1) as usize
        };
        // const count = @min(max, @max(count_req, 1));
        let count = core::cmp::min(max, core::cmp::max(count_req, 1));
        // self.screens.active.cursorRight(@intCast(count));
        self.active_mut().cursor_right(count as size::CellCountInt);
    }

    /// Move the cursor to the left amount cells. If amount is 0, adjust it to 1.
    pub fn cursor_left(&mut self, count_req: usize) {
        // Wrapping behavior depends on various terminal modes
        // const WrapMode = enum { none, reverse, reverse_extended };
        enum WrapMode {
            None,
            Reverse,
            ReverseExtended,
        }
        // const wrap_mode: WrapMode = wrap_mode: {
        //     if (!self.modes.get(.wraparound)) break :wrap_mode .none;
        //     if (self.modes.get(.reverse_wrap_extended)) break :wrap_mode .reverse_extended;
        //     if (self.modes.get(.reverse_wrap)) break :wrap_mode .reverse;
        //     break :wrap_mode .none;
        // };
        let wrap_mode: WrapMode = 'wrap_mode: {
            if !self.modes.get(modespkg::Mode::Wraparound) {
                break 'wrap_mode WrapMode::None;
            }
            if self.modes.get(modespkg::Mode::ReverseWrapExtended) {
                break 'wrap_mode WrapMode::ReverseExtended;
            }
            if self.modes.get(modespkg::Mode::ReverseWrap) {
                break 'wrap_mode WrapMode::Reverse;
            }
            WrapMode::None
        };

        // var count = @max(count_req, 1);
        let mut count: usize = core::cmp::max(count_req, 1);

        // If we are in no wrap mode, then we move the cursor left and exit
        // since this is the fastest and most typical path.
        if matches!(wrap_mode, WrapMode::None) {
            // self.screens.active.cursorLeft(@min(count, self.screens.active.cursor.x));
            let cursor_x = unsafe { (*self.screens.active).cursor.x };
            self.active_mut()
                .cursor_left(core::cmp::min(count, cursor_x as usize) as size::CellCountInt);
            // self.screens.active.cursor.pending_wrap = false;
            unsafe { (*self.screens.active).cursor.pending_wrap = false };
            return;
        }

        // If we have a pending wrap state and we are in either reverse wrap
        // modes then we decrement the amount we move by one to match xterm.
        // if (self.screens.active.cursor.pending_wrap) {
        //     count -= 1;
        //     self.screens.active.cursor.pending_wrap = false;
        // }
        if unsafe { (*self.screens.active).cursor.pending_wrap } {
            count -= 1;
            unsafe { (*self.screens.active).cursor.pending_wrap = false };
        }

        // The margins we can move to.
        // const top = self.scrolling_region.top;
        // const bottom = self.scrolling_region.bottom;
        // const right_margin = self.scrolling_region.right;
        // const left_margin = if (self.screens.active.cursor.x < self.scrolling_region.left)
        //     0
        // else
        //     self.scrolling_region.left;
        let top = self.scrolling_region.top;
        let bottom = self.scrolling_region.bottom;
        let right_margin = self.scrolling_region.right;
        let left_margin = if unsafe { (*self.screens.active).cursor.x } < self.scrolling_region.left
        {
            0
        } else {
            self.scrolling_region.left
        };

        // Handle some edge cases when our cursor is already on the left margin.
        // if (self.screens.active.cursor.x == left_margin) {
        if unsafe { (*self.screens.active).cursor.x } == left_margin {
            match wrap_mode {
                // In reverse mode, if we're already before the top margin
                // then we just set our cursor to the top-left and we're done.
                // .reverse => if (self.screens.active.cursor.y <= top) {
                //     self.screens.active.cursorAbsolute(left_margin, top);
                //     return;
                // },
                WrapMode::Reverse => {
                    if unsafe { (*self.screens.active).cursor.y } <= top {
                        self.active_mut().cursor_absolute(left_margin, top);
                        return;
                    }
                }

                // Handled in while loop
                WrapMode::ReverseExtended => {}

                // Handled above
                WrapMode::None => unreachable!(),
            }
        }

        // while (true) {
        loop {
            // We can move at most to the left margin.
            // const max = self.screens.active.cursor.x - left_margin;
            let max = unsafe { (*self.screens.active).cursor.x } - left_margin;

            // We want to move at most the number of columns we have left
            // or our remaining count. Do the move.
            // const amount = @min(max, count);
            // count -= amount;
            // self.screens.active.cursorLeft(amount);
            let amount = core::cmp::min(max as usize, count);
            count -= amount;
            self.active_mut().cursor_left(amount as size::CellCountInt);

            // If we have no more to move, then we're done.
            // if (count == 0) break;
            if count == 0 {
                break;
            }

            // If we are at the top, then we are done.
            // if (self.screens.active.cursor.y == top) {
            //     if (wrap_mode != .reverse_extended) break;
            //     self.screens.active.cursorAbsolute(right_margin, bottom);
            //     count -= 1;
            //     continue;
            // }
            if unsafe { (*self.screens.active).cursor.y } == top {
                if !matches!(wrap_mode, WrapMode::ReverseExtended) {
                    break;
                }

                self.active_mut().cursor_absolute(right_margin, bottom);
                count -= 1;
                continue;
            }

            // UNDEFINED TERMINAL BEHAVIOR. This situation is not handled in xterm
            // and currently results in a crash in xterm. Given no other known
            // terminal [to me] implements XTREVWRAP2, I decided to just mimic
            // the behavior of xterm up and not including the crash by wrapping
            // up to the (0, 0) and stopping there. My reasoning is that for an
            // appropriately sized value of "count" this is the behavior that xterm
            // would have. This is unit tested.
            // if (self.screens.active.cursor.y == 0) {
            //     assert(self.screens.active.cursor.x == left_margin);
            //     break;
            // }
            if unsafe { (*self.screens.active).cursor.y } == 0 {
                debug_assert!(unsafe { (*self.screens.active).cursor.x } == left_margin);
                break;
            }

            // If our previous line is not wrapped then we are done.
            // if (wrap_mode != .reverse_extended) {
            //     const prev_row = self.screens.active.cursorRowUp(1);
            //     if (!prev_row.wrap) break;
            // }
            if !matches!(wrap_mode, WrapMode::ReverseExtended) {
                let prev_row = self.active_mut().cursor_row_up(1);
                if !unsafe { (*prev_row).wrap() } {
                    break;
                }
            }

            // self.screens.active.cursorAbsolute(right_margin, self.screens.active.cursor.y - 1);
            // count -= 1;
            let new_y = unsafe { (*self.screens.active).cursor.y } - 1;
            self.active_mut().cursor_absolute(right_margin, new_y);
            count -= 1;
        }
    }

    /// Save cursor position and further state.
    ///
    /// The primary and alternate screen have distinct save state. One saved state
    /// is kept per screen (main / alternative). If for the current screen state
    /// was already saved it is overwritten.
    pub fn save_cursor(&mut self) {
        // self.screens.active.saved_cursor = .{
        //     .x = self.screens.active.cursor.x,
        //     .y = self.screens.active.cursor.y,
        //     .style = self.screens.active.cursor.style,
        //     .protected = self.screens.active.cursor.protected,
        //     .pending_wrap = self.screens.active.cursor.pending_wrap,
        //     .origin = self.modes.get(.origin),
        //     .charset = self.screens.active.charset,
        // };
        let origin = self.modes.get(modespkg::Mode::Origin);
        let active: *mut Screen = self.screens.active;
        let saved = crate::screen::SavedCursor {
            x: unsafe { (*active).cursor.x },
            y: unsafe { (*active).cursor.y },
            style: unsafe { (*active).cursor.style },
            protected: unsafe { (*active).cursor.protected },
            pending_wrap: unsafe { (*active).cursor.pending_wrap },
            origin,
            charset: unsafe { (*active).charset.clone() },
        };
        self.active_mut().saved_cursor = Some(saved);
    }

    /// Restore cursor position and other state.
    ///
    /// The primary and alternate screen have distinct save state.
    /// If no save was done before values are reset to their initial values.
    pub fn restore_cursor(&mut self) {
        // const saved: Screen.SavedCursor = self.screens.active.saved_cursor orelse .{
        //     .x = 0,
        //     .y = 0,
        //     .style = .{},
        //     .protected = false,
        //     .pending_wrap = false,
        //     .origin = false,
        //     .charset = .{},
        // };
        // PORT NOTE: `SavedCursor` is a Zig value struct copied by `orelse`; in
        // Rust it is not `Copy` (it holds a `CharsetState`/`CharsetArray`). We
        // borrow the saved state (or a fresh default), then clone the `charset`
        // only where the reference reads `saved.charset` below.
        let default_saved;
        let saved: &crate::screen::SavedCursor =
            match unsafe { &(*self.screens.active).saved_cursor } {
                Some(s) => s,
                None => {
                    default_saved = crate::screen::SavedCursor {
                        x: 0,
                        y: 0,
                        style: style::Style::default(),
                        protected: false,
                        pending_wrap: false,
                        origin: false,
                        charset: crate::screen::CharsetState::default(),
                    };
                    &default_saved
                }
            };
        // Copy out the scalar saved fields up front so we don't hold the borrow
        // of `self.screens.active.saved_cursor` across the `&mut self` calls.
        let saved_style = saved.style;
        let saved_charset = saved.charset.clone();
        let saved_origin = saved.origin;
        let saved_pending_wrap = saved.pending_wrap;
        let saved_protected = saved.protected;
        let saved_x = saved.x;
        let saved_y = saved.y;

        // Set the style first because it can fail
        // self.screens.active.cursor.style = saved.style;
        // self.screens.active.manualStyleUpdate() catch |err| {
        //     log.warn("restoreCursor error updating style err={}", .{err});
        //     const screen: *Screen = self.screens.active;
        //     screen.cursor.style = .{};
        //     self.screens.active.manualStyleUpdate() catch unreachable;
        // };
        unsafe { (*self.screens.active).cursor.style = saved_style };
        if let Err(err) = self.active_mut().manual_style_update() {
            // Regardless of the error here, we revert back to an unstyled
            // cursor. It is more important that the restore succeeds in
            // other attributes because terminals have no way to communicate
            // failure back.
            log::warn!(target: LOG_SCOPE, "restoreCursor error updating style err={err:?}");
            let screen: *mut Screen = self.screens.active;
            unsafe { (*screen).cursor.style = style::Style::default() };
            self.active_mut().manual_style_update().unwrap();
        }

        // self.screens.active.charset = saved.charset;
        // self.modes.set(.origin, saved.origin);
        // self.screens.active.cursor.pending_wrap = saved.pending_wrap;
        // self.screens.active.cursor.protected = saved.protected;
        unsafe { (*self.screens.active).charset = saved_charset };
        self.modes.set(modespkg::Mode::Origin, saved_origin);
        unsafe { (*self.screens.active).cursor.pending_wrap = saved_pending_wrap };
        unsafe { (*self.screens.active).cursor.protected = saved_protected };
        // self.screens.active.cursorAbsolute(
        //     @min(saved.x, self.cols - 1),
        //     @min(saved.y, self.rows - 1),
        // );
        let x = core::cmp::min(saved_x, self.cols - 1);
        let y = core::cmp::min(saved_y, self.rows - 1);
        self.active_mut().cursor_absolute(x, y);

        // Ensure our screen is consistent
        // self.screens.active.assertIntegrity();
        self.active_mut().assert_integrity();
    }

    /// Set the character protection mode for the terminal.
    pub fn set_protected_mode(&mut self, mode: ansi::ProtectedMode) {
        match mode {
            ansi::ProtectedMode::Off => {
                self.active_mut().cursor.protected = false;

                // screen.protected_mode is NEVER reset to ".off" because
                // logic such as eraseChars depends on knowing what the
                // _most recent_ mode was.
            }

            ansi::ProtectedMode::Iso => {
                self.active_mut().cursor.protected = true;
                self.active_mut().protected_mode = ansi::ProtectedMode::Iso;
            }

            ansi::ProtectedMode::Dec => {
                self.active_mut().cursor.protected = true;
                self.active_mut().protected_mode = ansi::ProtectedMode::Dec;
            }
        }
    }

    /// Perform a semantic prompt command.
    ///
    /// If there is an error, we do our best to get the terminal into
    /// some coherent state, since callers typically can't handle errors
    /// (since they're sending sequences via the pty).
    pub fn semantic_prompt(&mut self, cmd: osc::SemanticPrompt) -> Result<(), OutOfMemory> {
        use crate::osc::semantic_prompt::option;
        use crate::osc::semantic_prompt::Action;
        match cmd.action {
            // try self.semanticPromptFreshLine();
            Action::FreshLine => self.semantic_prompt_fresh_line()?,

            Action::FreshLineNewPrompt => {
                // "First do a fresh-line."
                self.semantic_prompt_fresh_line()?;

                // const screen: *Screen = self.screens.active;
                let screen: *mut Screen = self.screens.active;

                // "Subsequent text (until a OSC "133;B" or OSC "133;I" command)
                // is a prompt string (as if followed by OSC 133;P;k=i\007)."
                // screen.cursorSetSemanticContent(.{
                //     .prompt = cmd.readOption(.prompt_kind) orelse .initial,
                // });
                unsafe {
                    (*screen).cursor_set_semantic_content(
                        crate::screen::SemanticContentArg::Prompt(
                            cmd.read_option(option::PROMPT_KIND)
                                .unwrap_or(osc::semantic_prompt::PromptKind::Initial),
                        ),
                    )
                };

                // This is a kitty-specific flag that notes that the shell
                // is NOT capable of redraw. Redraw defaults to true so this
                // usually just disables it, but either is possible.
                // if (cmd.readOption(.redraw)) |v| {
                //     self.flags.shell_redraws_prompt = v;
                // }
                if let Some(v) = cmd.read_option(option::REDRAW) {
                    self.flags.shell_redraws_prompt = v;
                }

                // click: {
                //     // Handle click_events as a priority over cl. click_events
                //     // is another Kitty-specific extension that converts clicks
                //     // within a prompt area to SGR mouse events and defers to the
                //     // shell to handle them.
                //     if (cmd.readOption(.click_events)) |v| {
                //         screen.semantic_prompt.click = .{ .click_events = v };
                //         break :click;
                //     }
                //
                //     // If click_events was not set or disabled, fallback to `cl`.
                //     if (cmd.readOption(.cl)) |v| {
                //         screen.semantic_prompt.click = .{ .cl = v };
                //     }
                // }
                #[allow(clippy::collapsible_else_if)]
                if let Some(v) = cmd.read_option(option::CLICK_EVENTS) {
                    unsafe {
                        (*screen).semantic_prompt.click =
                            crate::screen::SemanticClick::ClickEvents(v);
                    }
                } else {
                    if let Some(v) = cmd.read_option(option::CL) {
                        unsafe {
                            (*screen).semantic_prompt.click = crate::screen::SemanticClick::Cl(v);
                        }
                    }
                }

                // The "aid" and "cl" options are also valid for this
                // command but we don't yet handle these in any meaningful way.
            }

            Action::NewCommand => {
                // Spec:
                // Same as OSC "133;A" but may first implicitly terminate a
                // previous command: if the options specify an aid and there
                // is an active (open) command with matching aid, finish the
                // innermost such command (as well as any other commands
                // nested more deeply). If no aid is specified, treat as an
                // aid whose value is the empty string.

                // Ghostty:
                // We don't currently do explicit command tracking in any way
                // so there is no need to terminate prior commands. We just
                // perform the `A` action.
                // try self.semanticPrompt(.{
                //     .action = .fresh_line_new_prompt,
                //     .options_unvalidated = cmd.options_unvalidated,
                // });
                self.semantic_prompt(osc::SemanticPrompt {
                    action: Action::FreshLineNewPrompt,
                    options_unvalidated: cmd.options_unvalidated,
                })?;
            }

            Action::PromptStart => {
                // Explicit start of prompt. Optional after an A or N command.
                // The k (kind) option specifies the type of prompt:
                // regular primary prompt (k=i or default),
                // right-side prompts (k=r), or prompts for continuation lines (k=c or k=s).
                // self.screens.active.cursorSetSemanticContent(.{
                //     .prompt = cmd.readOption(.prompt_kind) orelse .initial,
                // });
                let screen: *mut Screen = self.screens.active;
                unsafe {
                    (*screen).cursor_set_semantic_content(
                        crate::screen::SemanticContentArg::Prompt(
                            cmd.read_option(option::PROMPT_KIND)
                                .unwrap_or(osc::semantic_prompt::PromptKind::Initial),
                        ),
                    )
                };
            }

            Action::EndPromptStartInput => {
                // End of prompt and start of user input, terminated by a OSC
                // "133;C" or another prompt (OSC "133;P").
                // self.screens.active.cursorSetSemanticContent(.{
                //     .input = .clear_explicit,
                // });
                let screen: *mut Screen = self.screens.active;
                unsafe {
                    (*screen).cursor_set_semantic_content(crate::screen::SemanticContentArg::Input(
                        crate::screen::SemanticContentInput::ClearExplicit,
                    ))
                };
            }

            Action::EndPromptStartInputTerminateEol => {
                // End of prompt and start of user input, terminated by end-of-line.
                // self.screens.active.cursorSetSemanticContent(.{
                //     .input = .clear_eol,
                // });
                let screen: *mut Screen = self.screens.active;
                unsafe {
                    (*screen).cursor_set_semantic_content(crate::screen::SemanticContentArg::Input(
                        crate::screen::SemanticContentInput::ClearEol,
                    ))
                };
            }

            Action::EndInputStartOutput => {
                // "End of input, and start of output."
                // self.screens.active.cursorSetSemanticContent(.output);
                let screen: *mut Screen = self.screens.active;
                unsafe {
                    (*screen).cursor_set_semantic_content(crate::screen::SemanticContentArg::Output)
                };

                // If our current row is marked as a prompt and we're
                // at column zero then we assume we're un-prompting. This
                // is a heuristic to deal with fish, mostly. The issue that
                // fish brings up is that it has no PS2 equivalent and its
                // builtin OSC133 marking doesn't output continuation lines
                // as k=s. So, we assume when we get a newline with a prompt
                // cursor that the new line is also a prompt. But fish changes
                // to output on the newline. So if we're at col 0 we just assume
                // we're overwriting the prompt.
                // if (self.screens.active.cursor.page_row.semantic_prompt != .none and
                //     self.screens.active.cursor.x == 0)
                // {
                //     self.screens.active.cursor.page_row.semantic_prompt = .none;
                // }
                if unsafe { (*(*screen).cursor.page_row).semantic_prompt() }
                    != pagepkg::SemanticPrompt::None
                    && unsafe { (*screen).cursor.x } == 0
                {
                    unsafe {
                        (*(*screen).cursor.page_row)
                            .set_semantic_prompt(pagepkg::SemanticPrompt::None)
                    };
                }
            }

            Action::EndCommand => {
                // From a terminal state perspective, this doesn't really do
                // anything. Other terminals appear to do nothing here. I think
                // its reasonable at this point to reset our semantic content
                // state but the spec doesn't really say what to do.
                // self.screens.active.cursorSetSemanticContent(.output);
                let screen: *mut Screen = self.screens.active;
                unsafe {
                    (*screen).cursor_set_semantic_content(crate::screen::SemanticContentArg::Output)
                };
            }
        }

        Ok(())
    }

    // OSC 133;L
    fn semantic_prompt_fresh_line(&mut self) -> Result<(), OutOfMemory> {
        // const left_margin = if (self.screens.active.cursor.x < self.scrolling_region.left)
        //     0
        // else
        //     self.scrolling_region.left;
        let left_margin = if unsafe { (*self.screens.active).cursor.x } < self.scrolling_region.left
        {
            0
        } else {
            self.scrolling_region.left
        };

        // Spec: "If the cursor is the initial column (left, assuming
        // left-to-right writing), do nothing" This specification is very under
        // specified. We are taking the liberty to assume that in a left/right
        // margin context, if the cursor is outside of the left margin, we treat
        // it as being at the left margin for the purposes of this command.
        // This is arbitrary. If someone has a better reasonable idea we can
        // apply it.
        // if (self.screens.active.cursor.x == left_margin) return;
        if unsafe { (*self.screens.active).cursor.x } == left_margin {
            return Ok(());
        }

        // self.carriageReturn();
        self.carriage_return();
        // try self.index();
        self.index()?;

        Ok(())
    }

    /// Returns true if the cursor is currently at a prompt. Another way to look
    /// at this is it returns false if the shell is currently outputting something.
    /// This requires shell integration (semantic prompt integration).
    ///
    /// If the shell integration doesn't exist, this will always return false.
    pub fn cursor_is_at_prompt(&mut self) -> bool {
        // If we're on the secondary screen, we're never at a prompt.
        // if (self.screens.active_key == .alternate) return false;
        if self.screens.active_key == crate::screen_set::Key::Alternate {
            return false;
        }

        // If our page row is a prompt then we're always at a prompt
        // const cursor: *const Screen.Cursor = &self.screens.active.cursor;
        let cursor: *const crate::screen::Cursor = unsafe { &(*self.screens.active).cursor };
        // if (cursor.page_row.semantic_prompt != .none) return true;
        if unsafe { (*(*cursor).page_row).semantic_prompt() } != pagepkg::SemanticPrompt::None {
            return true;
        }

        // Otherwise, determine our cursor state
        // return switch (cursor.semantic_content) {
        //     .input, .prompt => true,
        //     .output => false,
        // };
        match unsafe { (*cursor).semantic_content } {
            pagepkg::SemanticContent::Input | pagepkg::SemanticContent::Prompt => true,
            pagepkg::SemanticContent::Output => false,
        }
    }

    /// Horizontal tab moves the cursor to the next tabstop, clearing
    /// the screen to the left the tabstop.
    pub fn horizontal_tab(&mut self) {
        // while (self.screens.active.cursor.x < self.scrolling_region.right) {
        while unsafe { (*self.screens.active).cursor.x } < self.scrolling_region.right {
            // Move the cursor right
            // self.screens.active.cursorRight(1);
            self.active_mut().cursor_right(1);

            // If the last cursor position was a tabstop we return. We do
            // "last cursor position" because we want a space to be written
            // at the tabstop unless we're at the end (the while condition).
            // if (self.tabstops.get(self.screens.active.cursor.x)) return;
            if self
                .tabstops
                .get(unsafe { (*self.screens.active).cursor.x } as usize)
            {
                return;
            }
        }
    }

    // Same as horizontalTab but moves to the previous tabstop instead of the next.
    pub fn horizontal_tab_back(&mut self) {
        // With origin mode enabled, our leftmost limit is the left margin.
        // const left_limit = if (self.modes.get(.origin)) self.scrolling_region.left else 0;
        let left_limit = if self.modes.get(modespkg::Mode::Origin) {
            self.scrolling_region.left
        } else {
            0
        };

        // while (true) {
        loop {
            // If we're already at the edge of the screen, then we're done.
            // if (self.screens.active.cursor.x <= left_limit) return;
            if unsafe { (*self.screens.active).cursor.x } <= left_limit {
                return;
            }

            // Move the cursor left
            // self.screens.active.cursorLeft(1);
            // if (self.tabstops.get(self.screens.active.cursor.x)) return;
            self.active_mut().cursor_left(1);
            if self
                .tabstops
                .get(unsafe { (*self.screens.active).cursor.x } as usize)
            {
                return;
            }
        }
    }

    /// Clear tab stops.
    pub fn tab_clear(&mut self, cmd: csi::TabClear) {
        // switch (cmd) {
        //     .current => self.tabstops.unset(self.screens.active.cursor.x),
        //     .all => self.tabstops.reset(0),
        //     else => log.warn("invalid or unknown tab clear setting: {}", .{cmd}),
        // }
        match cmd {
            csi::TabClear::Current => self
                .tabstops
                .unset(unsafe { (*self.screens.active).cursor.x } as usize),
            csi::TabClear::All => self.tabstops.reset(0),
            csi::TabClear::Other(_) => {
                log::warn!(target: LOG_SCOPE, "invalid or unknown tab clear setting: {cmd:?}");
            }
        }
    }

    /// Set a tab stop on the current cursor.
    /// TODO: test
    pub fn tab_set(&mut self) {
        // self.tabstops.set(self.screens.active.cursor.x);
        self.tabstops
            .set(unsafe { (*self.screens.active).cursor.x } as usize);
    }

    /// TODO: test
    pub fn tab_reset(&mut self) {
        // self.tabstops.reset(TABSTOP_INTERVAL);
        self.tabstops.reset(TABSTOP_INTERVAL as usize);
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
        // const screen: *Screen = self.screens.active;
        let screen: *mut Screen = self.screens.active;

        // Unset pending wrap state
        // screen.cursor.pending_wrap = false;
        unsafe { (*screen).cursor.pending_wrap = false };

        // We handle our cursor semantic prompt state AFTER doing the
        // scrolling, because we may need to apply to new rows.
        // defer if (screen.cursor.semantic_content != .output) { ... } else { ... };
        // PORT NOTE: Rust has no `defer`; this RAII guard reproduces the
        // semantic-content handling that the reference defers to AFTER the
        // scroll. Its `Drop` reads `screen.cursor` at drop time (post-scroll),
        // matching the reference's deferred evaluation, and runs on every exit
        // path (including the early `return`s and the `?`).
        let _semantic = SemanticIndexGuard { screen };

        // Outside of the scroll region we move the cursor one line down.
        // if (screen.cursor.y < self.scrolling_region.top or
        //     screen.cursor.y > self.scrolling_region.bottom)
        if unsafe { (*screen).cursor.y } < self.scrolling_region.top
            || unsafe { (*screen).cursor.y } > self.scrolling_region.bottom
        {
            // We only move down if we're not already at the bottom of
            // the screen.
            // if (screen.cursor.y < self.rows - 1) {
            if unsafe { (*screen).cursor.y } < self.rows - 1 {
                // screen.cursorDown(1);
                unsafe { (*screen).cursor_down(1) };
            }

            return Ok(());
        }

        // If the cursor is inside the scrolling region and on the bottom-most
        // line, then we scroll up. If our scrolling region is the full screen
        // we create scrollback.
        // if (screen.cursor.y == self.scrolling_region.bottom and
        //     screen.cursor.x >= self.scrolling_region.left and
        //     screen.cursor.x <= self.scrolling_region.right)
        if unsafe { (*screen).cursor.y } == self.scrolling_region.bottom
            && unsafe { (*screen).cursor.x } >= self.scrolling_region.left
            && unsafe { (*screen).cursor.x } <= self.scrolling_region.right
        {
            // if (comptime build_options.kitty_graphics) {
            //     // Scrolling dirties the images because it updates their placements pins.
            //     screen.kitty_images.dirty = true;
            // }
            if crate::options::KITTY_GRAPHICS {
                // Scrolling dirties the images because it updates their placements pins.
                // SAFETY: `screen` is the active screen's stable heap pointer,
                // owned by `self.screens`; we hold `&mut self`.
                unsafe { (*screen).kitty_images.dirty = true };
            }

            // If our scrolling region is at the top, we create scrollback,
            // but only if our screen retains scrollback. If our screen
            // doesn't retain scrollback (e.g. the alternate screen) then
            // creating scrollback is pure overhead: the rows are never
            // visible and are simply pruned later. In that case we use the
            // in-place region scroll below, unless the region is a single
            // row (a one row screen) which cursorScrollRegionUp can't
            // handle (and cursorDownScroll special-cases).
            // if (self.scrolling_region.top == 0 and
            //     self.scrolling_region.left == 0 and
            //     self.scrolling_region.right == self.cols - 1 and
            //     (!screen.no_scrollback or
            //         self.scrolling_region.bottom == 0))
            if self.scrolling_region.top == 0
                && self.scrolling_region.left == 0
                && self.scrolling_region.right == self.cols - 1
                && (!unsafe { (*screen).no_scrollback } || self.scrolling_region.bottom == 0)
            {
                // If a bottom margin is set, kitty image placements may
                // need adjusting around the scroll. The rare placements-
                // present case is handled out of line so this hot path
                // only pays a count check (a load from a cache line we
                // already write, above).
                // if (comptime build_options.kitty_graphics) {
                if crate::options::KITTY_GRAPHICS {
                    // if (screen.kitty_images.placements.count() != 0) {
                    //     @branchHint(.unlikely);
                    // SAFETY: `screen` is the active screen's stable heap
                    // pointer, owned by `self.screens`; we hold `&mut self`.
                    if !unsafe { (*screen).kitty_images.placements.is_empty() } {
                        self.index_scroll_with_images(
                            crate::kitty::graphics_storage::ScrollOp::WindowShift,
                        )?;
                        return Ok(());
                    }
                }

                // try screen.cursorScrollAbove();
                self.active_mut().cursor_scroll_above()?;
                return Ok(());
            }

            // Slow path for left and right scrolling region margins.
            // scrollUp handles the kitty image adjustment itself.
            // if (self.scrolling_region.left != 0 or
            //     self.scrolling_region.right != self.cols - 1)
            //
            // PORT NOTE (oracle 77190bd02, `P3-W5 T3`): the third disjunct
            // `!screen.blankCell().isZero()` was DELETED by the oracle along
            // with its `PERF(mitchellh)` comment. It existed because
            // `eraseRowBounded` (the old fast path) did not preserve an SGR
            // background in the erased row; `cursorScrollRegionUp` does, so
            // the background case now takes the fast path too.
            if self.scrolling_region.left != 0 || self.scrolling_region.right != self.cols - 1 {
                // try self.scrollUp(1);
                self.scroll_up(1)?;
                return Ok(());
            }

            // Kitty image placements may need adjusting around the scroll;
            // handled out of line like the scrollback path above.
            // if (comptime build_options.kitty_graphics) {
            if crate::options::KITTY_GRAPHICS {
                // if (screen.kitty_images.placements.count() != 0) {
                //     @branchHint(.unlikely);
                // SAFETY: `screen` is the active screen's stable heap pointer,
                // owned by `self.screens`; we hold `&mut self`.
                if !unsafe { (*screen).kitty_images.placements.is_empty() } {
                    self.index_scroll_with_images(
                        crate::kitty::graphics_storage::ScrollOp::InPlace,
                    )?;
                    return Ok(());
                }
            }

            // Otherwise use a fast path function to efficiently scroll
            // the contents of the scrolling region.
            // try screen.cursorScrollRegionUp(
            //     self.scrolling_region.bottom - self.scrolling_region.top,
            // );
            let sr_top = self.scrolling_region.top;
            let sr_bottom = self.scrolling_region.bottom;
            self.active_mut()
                .cursor_scroll_region_up((sr_bottom - sr_top) as usize)?;

            return Ok(());
        }

        // Increase cursor by 1, maximum to bottom of scroll region
        // if (screen.cursor.y < self.scrolling_region.bottom) {
        if unsafe { (*screen).cursor.y } < self.scrolling_region.bottom {
            // screen.cursorDown(1);
            unsafe { (*screen).cursor_down(1) };
        }

        Ok(())
    }

    /// The operation when we have Kitty image placements during index.
    /// Split out of index() so its hot paths don't carry the adjustment
    /// state in their stack frame, which measurably slows the scroll
    /// hot path.
    //
    // PORT NOTE (R8): the reference `op` is a `comptime` parameter — a pure
    // branch-specialization for the scroll hot path, with no compile-time
    // grammar or emitted-bytes dependence (no runtime bytes can alias any
    // comptime shape here; the value only selects between two calls below).
    // Rust const generics cannot carry an enum on stable, so it is a plain
    // runtime parameter; the hot paths above still pay only the count check
    // before entering this out-of-line fn.
    fn index_scroll_with_images(
        &mut self,
        op: crate::kitty::graphics_storage::ScrollOp,
    ) -> Result<(), OutOfMemory> {
        // var kitty_scroll = self.kittyScrollMarginsBegin(-1, op);
        // defer if (kitty_scroll) |*state| state.end();
        // PORT NOTE: the guard's `Drop` performs `end()` (exactly-once by
        // ownership), so the Zig `defer` semantics — including the error
        // path through `?` — fall out of scope-drop.
        let kitty_scroll = self.kitty_scroll_margins_begin(-1, op);
        let result = match op {
            // .window_shift => try self.screens.active.cursorScrollAbove(),
            crate::kitty::graphics_storage::ScrollOp::WindowShift => {
                self.active_mut().cursor_scroll_above()
            }
            // .in_place => try self.screens.active.cursorScrollRegionUp(
            //     self.scrolling_region.bottom - self.scrolling_region.top,
            // ),
            crate::kitty::graphics_storage::ScrollOp::InPlace => {
                let sr_top = self.scrolling_region.top;
                let sr_bottom = self.scrolling_region.bottom;
                self.active_mut()
                    .cursor_scroll_region_up((sr_bottom - sr_top) as usize)
            }
        };
        if let Some(state) = kitty_scroll {
            state.end();
        }
        result
    }

    // Handle when Kitty graphics is disabled.
    // const KittyScrollMargins = if (build_options.kitty_graphics)
    //     kitty.graphics.ImageStorage.ScrollMargins
    // else
    //     struct {
    //         pub inline fn end(self: *@This()) void {
    //             _ = self;
    //         }
    //     };
    //
    // PORT NOTE: Zig needs the empty stub type because the disabled comptime
    // branch must still typecheck against a named type. Rust compiles the real
    // `ScrollMargins` type unconditionally (the kitty module is always built;
    // the `KITTY_GRAPHICS` const-gate dead-codes the calls), so no stub type
    // is required.

    /// Begin adjusting kitty image placements for a scroll of the
    /// scrolling region by delta rows (negative moves content up). If
    /// adjustment is needed this returns state whose end() must be called
    /// after the scroll's row operations complete (see
    /// ImageStorage.scrollMarginsBegin for why this is two phases). This
    /// returns null when the scrolling region is the full screen, because
    /// placements then follow their anchored rows via pin tracking which
    /// matches kitty's marginless behavior.
    ///
    /// Callers must comptime-gate on build_options.kitty_graphics and
    /// check that placements exist before calling, which keeps the cost
    /// on the hot scroll paths cheap.
    // @branchHint(.cold) → #[cold].
    #[cold]
    fn kitty_scroll_margins_begin(
        &mut self,
        delta: isize,
        op: crate::kitty::graphics_storage::ScrollOp,
    ) -> Option<crate::kitty::graphics_storage::ScrollMargins> {
        // Full-screen scrolls need no adjustment: placements follow their
        // anchored rows (possibly into the scrollback) via pin tracking.
        if self.scrolling_region.top == 0
            && self.scrolling_region.bottom == self.rows - 1
            && self.scrolling_region.left == 0
            && self.scrolling_region.right == self.cols - 1
        {
            return None;
        }

        // const screen: *Screen = self.screens.active;
        // return screen.kitty_images.scrollMarginsBegin(self.io(), self, delta, op);
        //
        // SAFETY: the storage lives INSIDE the active screen; the `&mut`
        // projection below is scoped to the `kitty_images` field of the
        // screen's separate heap allocation, and `scroll_margins_begin`
        // touches the rest of the terminal/screen only through the raw `t`
        // pointer in narrow read scopes (its hazard-#1 PORT NOTE).
        let t: *mut Terminal = self;
        let screen: *mut Screen = self.screens.active;
        Some(unsafe { &mut (*screen).kitty_images }.scroll_margins_begin(t, delta, op))
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
        // if (self.screens.active.cursor.y != self.scrolling_region.top or
        //     self.screens.active.cursor.x < self.scrolling_region.left or
        //     self.screens.active.cursor.x > self.scrolling_region.right)
        if unsafe { (*self.screens.active).cursor.y } != self.scrolling_region.top
            || unsafe { (*self.screens.active).cursor.x } < self.scrolling_region.left
            || unsafe { (*self.screens.active).cursor.x } > self.scrolling_region.right
        {
            // self.cursorUp(1);
            self.cursor_up(1);
            return;
        }

        // self.scrollDown(1);
        self.scroll_down(1);
    }

    /// Set Cursor Position. Move cursor to the position indicated
    /// by row and column (1-indexed). If column is 0, it is adjusted to 1.
    /// If column is greater than the right-most column it is adjusted to
    /// the right-most column. If row is 0, it is adjusted to 1. If row is
    /// greater than the bottom-most row it is adjusted to the bottom-most
    /// row.
    pub fn set_cursor_pos(&mut self, row_req: usize, col_req: usize) {
        // If cursor origin mode is set the cursor row will be moved relative to
        // the top margin row and adjusted to be above or at bottom-most row in
        // the current scroll region.
        //
        // If origin mode is set and left and right margin mode is set the cursor
        // will be moved relative to the left margin column and adjusted to be on
        // or left of the right margin column.
        // const params: struct {
        //     x_offset: size.CellCountInt = 0,
        //     y_offset: size.CellCountInt = 0,
        //     x_max: size.CellCountInt,
        //     y_max: size.CellCountInt,
        // } = if (self.modes.get(.origin)) .{
        //     .x_offset = self.scrolling_region.left,
        //     .y_offset = self.scrolling_region.top,
        //     .x_max = self.scrolling_region.right + 1, // We need this 1-indexed
        //     .y_max = self.scrolling_region.bottom + 1, // We need this 1-indexed
        // } else .{
        //     .x_max = self.cols,
        //     .y_max = self.rows,
        // };
        struct Params {
            x_offset: size::CellCountInt,
            y_offset: size::CellCountInt,
            x_max: size::CellCountInt,
            y_max: size::CellCountInt,
        }
        let params: Params = if self.modes.get(modespkg::Mode::Origin) {
            Params {
                x_offset: self.scrolling_region.left,
                y_offset: self.scrolling_region.top,
                x_max: self.scrolling_region.right + 1, // We need this 1-indexed
                y_max: self.scrolling_region.bottom + 1, // We need this 1-indexed
            }
        } else {
            Params {
                x_offset: 0,
                y_offset: 0,
                x_max: self.cols,
                y_max: self.rows,
            }
        };

        // Unset pending wrap state
        // self.screens.active.cursor.pending_wrap = false;
        unsafe { (*self.screens.active).cursor.pending_wrap = false };

        // Calculate our new x/y
        // const row = if (row_req == 0) 1 else row_req;
        // const col = if (col_req == 0) 1 else col_req;
        // const x = @min(params.x_max, col +| params.x_offset) -| 1;
        // const y = @min(params.y_max, row +| params.y_offset) -| 1;
        let row = if row_req == 0 { 1 } else { row_req };
        let col = if col_req == 0 { 1 } else { col_req };
        // PORT NOTE: `col +| params.x_offset` is `usize +| CellCountInt`; we widen
        // the offset to usize for the add, then `@min` with `x_max` (also widened)
        // keeps the result within `CellCountInt` range before the saturating `-| 1`.
        // PORT NOTE (parity 0aaedf436): the origin-offset adds are saturating
        // (`+|`); requests near maxInt(usize) land on the region boundary
        // instead of overflowing.
        let x: size::CellCountInt = (core::cmp::min(
            params.x_max as usize,
            col.saturating_add(params.x_offset as usize),
        ) as size::CellCountInt)
            .saturating_sub(1);
        let y: size::CellCountInt = (core::cmp::min(
            params.y_max as usize,
            row.saturating_add(params.y_offset as usize),
        ) as size::CellCountInt)
            .saturating_sub(1);

        // If the y is unchanged then this is fast pointer math
        // if (y == self.screens.active.cursor.y) {
        //     if (x > self.screens.active.cursor.x) {
        //         self.screens.active.cursorRight(x - self.screens.active.cursor.x);
        //     } else {
        //         self.screens.active.cursorLeft(self.screens.active.cursor.x - x);
        //     }
        //     return;
        // }
        if y == unsafe { (*self.screens.active).cursor.y } {
            let cursor_x = unsafe { (*self.screens.active).cursor.x };
            if x > cursor_x {
                self.active_mut().cursor_right(x - cursor_x);
            } else {
                self.active_mut().cursor_left(cursor_x - x);
            }

            return;
        }

        // If everything changed we do an absolute change which is slightly slower
        // self.screens.active.cursorAbsolute(x, y);
        self.active_mut().cursor_absolute(x, y);
        // log.info("set cursor position: col={} row={}", .{ self.screens.active.cursor.x, self.screens.active.cursor.y });
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
        // const top = @max(1, top_req);
        let top = core::cmp::max(1, top_req);
        // const bottom = @min(self.rows, if (bottom_req == 0) self.rows else bottom_req);
        let bottom = core::cmp::min(
            self.rows as usize,
            if bottom_req == 0 {
                self.rows as usize
            } else {
                bottom_req
            },
        );
        // if (top >= bottom) return;
        if top >= bottom {
            return;
        }

        // self.scrolling_region.top = @intCast(top - 1);
        self.scrolling_region.top = (top - 1) as size::CellCountInt;
        // self.scrolling_region.bottom = @intCast(bottom - 1);
        self.scrolling_region.bottom = (bottom - 1) as size::CellCountInt;
        // self.setCursorPos(1, 1);
        self.set_cursor_pos(1, 1);
    }

    /// DECSLRM
    pub fn set_left_and_right_margin(&mut self, left_req: usize, right_req: usize) {
        // We must have this mode enabled to do anything
        // if (!self.modes.get(.enable_left_and_right_margin)) return;
        if !self.modes.get(modespkg::Mode::EnableLeftAndRightMargin) {
            return;
        }

        // const left = @max(1, left_req);
        let left = core::cmp::max(1, left_req);
        // const right = @min(self.cols, if (right_req == 0) self.cols else right_req);
        let right = core::cmp::min(
            self.cols as usize,
            if right_req == 0 {
                self.cols as usize
            } else {
                right_req
            },
        );
        // if (left >= right) return;
        if left >= right {
            return;
        }

        // self.scrolling_region.left = @intCast(left - 1);
        self.scrolling_region.left = (left - 1) as size::CellCountInt;
        // self.scrolling_region.right = @intCast(right - 1);
        self.scrolling_region.right = (right - 1) as size::CellCountInt;
        // self.setCursorPos(1, 1);
        self.set_cursor_pos(1, 1);
    }

    /// Scroll the text down by one row.
    pub fn scroll_down(&mut self, count: usize) {
        // Preserve our x/y to restore.
        // const old_x = self.screens.active.cursor.x;
        // const old_y = self.screens.active.cursor.y;
        // const old_wrap = self.screens.active.cursor.pending_wrap;
        // defer {
        //     self.screens.active.cursorAbsolute(old_x, old_y);
        //     self.screens.active.cursor.pending_wrap = old_wrap;
        // }
        // PORT NOTE: Rust has no `defer`; this RAII guard restores the cursor
        // x/y and pending_wrap when it drops at end of scope, mirroring the
        // reference `defer`. It runs on every exit path.
        let _restore = CursorRestoreGuard {
            screen: self.screens.active,
            old_x: unsafe { (*self.screens.active).cursor.x },
            old_y: unsafe { (*self.screens.active).cursor.y },
            old_wrap: unsafe { (*self.screens.active).cursor.pending_wrap },
        };

        // If margins are set and kitty image placements exist, they need
        // adjusting around the scroll. Note this wraps scrollDown and NOT
        // insertLines: kitty scrolls images for SD/RI but leaves them
        // alone for IL/DL.
        // var kitty_scroll: ?KittyScrollMargins = null;
        // defer if (kitty_scroll) |*state| state.end();
        // PORT NOTE: the guard's `Drop` performs `end()`; locals drop in
        // reverse declaration order, so the guard ends BEFORE `_restore`
        // fires — exactly the reference's LIFO `defer` order.
        let mut kitty_scroll: Option<crate::kitty::graphics_storage::ScrollMargins> = None;
        // if (comptime build_options.kitty_graphics) {
        if crate::options::KITTY_GRAPHICS {
            // if (self.screens.active.kitty_images.placements.count() != 0) {
            //     @branchHint(.unlikely);
            // SAFETY: `self.screens.active` is a stable heap pointer owned by
            // `self.screens`; we hold `&mut self`.
            if !unsafe { (*self.screens.active).kitty_images.placements.is_empty() } {
                // const region_height: usize =
                //     @as(usize, self.scrolling_region.bottom - self.scrolling_region.top) + 1;
                let region_height: usize =
                    (self.scrolling_region.bottom - self.scrolling_region.top) as usize + 1;
                // kitty_scroll = self.kittyScrollMarginsBegin(
                //     @intCast(@min(count, region_height)),
                //     .in_place,
                // );
                kitty_scroll = self.kitty_scroll_margins_begin(
                    isize::try_from(core::cmp::min(count, region_height)).unwrap(),
                    crate::kitty::graphics_storage::ScrollOp::InPlace,
                );
            }
        }

        // Move to the top of the scroll region
        // self.screens.active.cursorAbsolute(self.scrolling_region.left, self.scrolling_region.top);
        let sr_left = self.scrolling_region.left;
        let sr_top = self.scrolling_region.top;
        self.active_mut().cursor_absolute(sr_left, sr_top);
        // self.insertLines(count);
        self.insert_lines(count);

        // defer if (kitty_scroll) |*state| state.end();
        if let Some(state) = kitty_scroll.take() {
            state.end();
        }
    }

    /// Removes amount lines from the top of the scroll region. The remaining lines
    /// to the bottom margin are shifted up and space from the bottom margin up
    /// is filled with empty lines.
    ///
    /// The new lines are created according to the current SGR state.
    ///
    /// Does not change the (absolute) cursor position.
    pub fn scroll_up(&mut self, count: usize) -> Result<(), OutOfMemory> {
        // Preserve our x/y to restore.
        // const old_x = self.screens.active.cursor.x;
        // const old_y = self.screens.active.cursor.y;
        // const old_wrap = self.screens.active.cursor.pending_wrap;
        // defer {
        //     self.screens.active.cursorAbsolute(old_x, old_y);
        //     self.screens.active.cursor.pending_wrap = old_wrap;
        // }
        // PORT NOTE: see scroll_down — RAII guard mirrors the reference `defer`,
        // restoring the cursor on every return (including the early `?`).
        let _restore = CursorRestoreGuard {
            screen: self.screens.active,
            old_x: unsafe { (*self.screens.active).cursor.x },
            old_y: unsafe { (*self.screens.active).cursor.y },
            old_wrap: unsafe { (*self.screens.active).cursor.pending_wrap },
        };

        // If margins are set and kitty image placements exist, they need
        // adjusting around the scroll. Note this wraps scrollUp and NOT
        // deleteLines: kitty scrolls images for SU/IND but leaves them
        // alone for IL/DL.
        // var kitty_scroll: ?KittyScrollMargins = null;
        // defer if (kitty_scroll) |*state| state.end();
        // PORT NOTE: the guard's `Drop` performs `end()` on every exit path
        // (including the early `?`), mirroring the reference `defer`; locals
        // drop in reverse declaration order, so it ends BEFORE `_restore`
        // fires — the reference's LIFO `defer` order.
        let mut _kitty_scroll: Option<crate::kitty::graphics_storage::ScrollMargins> = None;
        // if (comptime build_options.kitty_graphics) {
        if crate::options::KITTY_GRAPHICS {
            // if (self.screens.active.kitty_images.placements.count() != 0) {
            //     @branchHint(.unlikely);
            // SAFETY: `self.screens.active` is a stable heap pointer owned by
            // `self.screens`; we hold `&mut self`.
            if !unsafe { (*self.screens.active).kitty_images.placements.is_empty() } {
                // The op must mirror the branch below: the scrollback path
                // shifts the active window while the deleteLines path
                // moves rows in place.
                // const region_height: usize =
                //     @as(usize, self.scrolling_region.bottom - self.scrolling_region.top) + 1;
                let region_height: usize =
                    (self.scrolling_region.bottom - self.scrolling_region.top) as usize + 1;
                // kitty_scroll = self.kittyScrollMarginsBegin(
                //     -@as(isize, @intCast(@min(count, region_height))),
                //     if (...) .window_shift else .in_place,
                // );
                _kitty_scroll = self.kitty_scroll_margins_begin(
                    -isize::try_from(core::cmp::min(count, region_height)).unwrap(),
                    if self.scrolling_region.top == 0
                        && self.scrolling_region.left == 0
                        && self.scrolling_region.right == self.cols - 1
                        && (!unsafe { (*self.screens.active).no_scrollback }
                            || self.scrolling_region.bottom == self.rows - 1)
                    {
                        crate::kitty::graphics_storage::ScrollOp::WindowShift
                    } else {
                        crate::kitty::graphics_storage::ScrollOp::InPlace
                    },
                );
            }
        }

        // If our scroll region is at the top and we have no left/right
        // margins then we move the scrolled out text into the scrollback.
        //
        // If our screen doesn't retain scrollback (e.g. the alternate
        // screen) then creating scrollback is pure overhead, so we use the
        // deleteLines path below instead, unless the region is the full
        // screen where cursorScrollAbove has a specialized fast path
        // (cursorDownScroll) for scrolling without scrollback.
        // if (self.scrolling_region.top == 0 and
        //     self.scrolling_region.left == 0 and
        //     self.scrolling_region.right == self.cols - 1 and
        //     (!self.screens.active.no_scrollback or
        //         self.scrolling_region.bottom == self.rows - 1))
        if self.scrolling_region.top == 0
            && self.scrolling_region.left == 0
            && self.scrolling_region.right == self.cols - 1
            && (!unsafe { (*self.screens.active).no_scrollback }
                || self.scrolling_region.bottom == self.rows - 1)
        {
            // Scrolling dirties the images because it updates their placements pins.
            // if (comptime build_options.kitty_graphics) {
            //     self.screens.active.kitty_images.dirty = true;
            // }
            if crate::options::KITTY_GRAPHICS {
                // SAFETY: `self.screens.active` is a stable heap pointer owned
                // by `self.screens`; we hold `&mut self`.
                unsafe { (*self.screens.active).kitty_images.dirty = true };
            }

            // Clamp count to the scroll region height.
            // const region_height = self.scrolling_region.bottom + 1;
            let region_height = self.scrolling_region.bottom as usize + 1;
            // const adjusted_count = @min(count, region_height);
            let adjusted_count = core::cmp::min(count, region_height);

            // TODO: Create an optimized version that can scroll N times
            // This isn't critical because in most cases, scrollUp is used
            // with count=1, but it's still a big optimization opportunity.

            // Move our cursor to the bottom of the scroll region so we can
            // use the cursorScrollAbove function to create scrollback
            // self.screens.active.cursorAbsolute(0, self.scrolling_region.bottom);
            let sr_bottom = self.scrolling_region.bottom;
            self.active_mut().cursor_absolute(0, sr_bottom);
            // for (0..adjusted_count) |_| try self.screens.active.cursorScrollAbove();
            for _ in 0..adjusted_count {
                self.active_mut().cursor_scroll_above()?;
            }
            return Ok(());
        }

        // Move to the top of the scroll region
        // self.screens.active.cursorAbsolute(self.scrolling_region.left, self.scrolling_region.top);
        let sr_left = self.scrolling_region.left;
        let sr_top = self.scrolling_region.top;
        self.active_mut().cursor_absolute(sr_left, sr_top);
        // self.deleteLines(count);
        self.delete_lines(count);

        Ok(())
    }

    /// Scroll the viewport of the terminal grid.
    pub fn scroll_viewport(&mut self, behavior: ScrollViewport) {
        // self.screens.active.scroll(switch (behavior) {
        //     .top => .{ .top = {} },
        //     .bottom => .{ .active = {} },
        //     .delta => |delta| .{ .delta_row = delta },
        //     .row => |row| .{ .row = row },
        // });
        self.active_mut().scroll(match behavior {
            ScrollViewport::Top => crate::screen::Scroll::Top,
            ScrollViewport::Bottom => crate::screen::Scroll::Active,
            ScrollViewport::Delta(delta) => crate::screen::Scroll::DeltaRow(delta),
            ScrollViewport::Row(row) => crate::screen::Scroll::Row(row),
        });
    }

    /// Return the current compression activity value.
    ///
    /// Callers should schedule a `compress` call whenever this value changes.
    /// The direction of the change has no meaning; this is an opaque change
    /// token rather than a monotonic sequence exposed by Terminal.
    ///
    /// It is up to the terminal what it decides to compress, but currently
    /// we compress cold (non-viewed, non-editable) scrollback history on
    /// the primary screen.
    ///
    /// Note that compression requires specific system features, namely
    /// the ability to retain virtual memory allocations while discarding their
    /// physical memory backings. Callers must still use `compress` to determine
    /// whether compression is supported on the current target.
    pub fn compression_activity(&self) -> u64 {
        // const state = &self.screens.get(.primary).?.pages.page_compression;
        let primary: *mut Screen = self.screens.get(crate::screen_set::Key::Primary).unwrap();
        let state = unsafe { &(*primary).pages.page_compression };
        // For now we don't use the extra 16 bits.
        // return @as(u64, state.activity_serial);
        //
        // PORT NOTE: the reference field is a `u48` widened here to `u64`.
        // Rustty stores it as a `u64` already masked to 48 bits by
        // `mark_activity`, so the widening is a plain read.
        state.activity_serial
    }

    /// Compress cold memory to save resident memory space.
    ///
    /// Full compression does a full pass compressing everything it can before
    /// returning. This is not recommended for interactive terminals because
    /// compression is relatively slow and with large scrollbacks this can cause
    /// stalls.
    ///
    /// Incremental compression bounds itself on how much data it can look
    /// up to compress and how much compression work it does before returning.
    /// It is stateful (we maintain the state) and the return value tells callers
    /// whether they should continue calling it in the future.
    ///
    /// Callers should schedule compression when it doesn't impact user
    /// experience, for example during idle times.
    ///
    /// PORT NOTE (WS19 T18) — `Pending` does NOT mean "unfinished work remains".
    /// A fresh `compress(Incremental)` issued right after a pass that returned
    /// `Complete` returns `Pending` again, because the incremental traversal
    /// RESTARTS from the beginning rather than remembering that it converged.
    /// So `Pending` means "call me again", never "there is work outstanding", and
    /// `result == Complete` is NOT a durable "compression is done" probe — it is
    /// only true of the pass that just ran. Callers that want an idle-detection
    /// signal must debounce on activity (see the renderer's compression scheduler,
    /// `rustty-core/src/renderer_seam.rs`, which schedules another tick on
    /// `Pending` and stops on `Complete`/`Unsupported` per tick — correct, because
    /// it re-arms from activity, not from a remembered `Complete`), not on this
    /// return value.
    ///
    /// This is rustty-only clarification of ported behavior, not reference prose:
    /// the reference doc comment above says only that "the return value tells
    /// callers whether they should continue calling it in the future", which reads
    /// naturally as the *opposite* of what happens. Observed, not assumed — the
    /// behavior is pinned by
    /// `terminal_compress_incremental_restarts_after_complete` in this file's test
    /// module, and cross-referenced from the compression narrative in
    /// `.port/phase-2/MIRI_EXCEPTIONS.md`.
    pub fn compress(&mut self, mode: TerminalCompressionMode) -> TerminalCompressionResult {
        // const pages = &self.screens.get(.primary).?.pages;
        let primary: *mut Screen = self.screens.get(crate::screen_set::Key::Primary).unwrap();
        let pages = unsafe { &mut (*primary).pages };
        // const result = switch (mode) {
        //     .incremental => pages.compress(.incremental),
        //     .full => pages.compress(.full),
        // };
        let result = match mode {
            TerminalCompressionMode::Incremental => {
                pages.compress(pagelist::compress::CompressionMode::Incremental)
            }
            TerminalCompressionMode::Full => {
                pages.compress(pagelist::compress::CompressionMode::Full)
            }
        };

        // return switch (result) {
        //     .unsupported => .unsupported,
        //     .pending => .pending,
        //     .complete => .complete,
        // };
        match result {
            pagelist::compress::IncrementalCompressionResult::Unsupported => {
                TerminalCompressionResult::Unsupported
            }
            pagelist::compress::IncrementalCompressionResult::Pending => {
                TerminalCompressionResult::Pending
            }
            pagelist::compress::IncrementalCompressionResult::Complete => {
                TerminalCompressionResult::Complete
            }
        }
    }

    /// To be called before shifting a row (as in insertLines and deleteLines)
    ///
    /// Takes care of boundary conditions such as potentially split wide chars
    /// across scrolling region boundaries and orphaned spacer heads at line
    /// ends.
    // PORT NOTE (SOUNDNESS): the reference takes `page: *Page, row: *Row` and
    // re-reads that memory through `clearGrapheme`/`updateRowGraphemeFlag`
    // (which call `assertIntegrity`) while those pointers are live. Zig
    // pointers are UNPROTECTED, so we mirror them as raw `*mut Page`/`*mut Row`
    // rather than `&mut` to avoid Stacked-Borrows aliasing UB (the
    // clear_cells UB-fix rule). We reconstruct local mutable accesses only for
    // the brief field mutations, never holding a `&mut Page` across an
    // integrity check.
    fn row_will_be_shifted(&mut self, page: *mut Page, row: *mut Row) {
        // const cells = row.cells.ptr(page.memory.ptr);
        let cells: *mut pagepkg::Cell = unsafe { (*row).cells().ptr((*page).memory_ptr()) };

        // If our scrolling region includes the rightmost column then we
        // need to turn any spacer heads in to normal empty cells, since
        // once we move them they no longer correspond with soft-wrapped
        // wide characters.
        //
        // If it contains either of the 2 leftmost columns, then the wide
        // characters in the first column which may be associated with a
        // spacer head will be either moved or cleared, so we also need
        // to turn the spacer heads in to empty cells in that case.
        // if (self.scrolling_region.right == self.cols - 1 or
        //     self.scrolling_region.left < 2)
        if self.scrolling_region.right == self.cols - 1 || self.scrolling_region.left < 2 {
            // const end_cell: *Cell = &cells[page.size.cols - 1];
            let page_cols = unsafe { (*page).size.cols };
            let end_cell: *mut pagepkg::Cell = unsafe { cells.add((page_cols - 1) as usize) };
            // if (end_cell.wide == .spacer_head) {
            if unsafe { (*end_cell).wide() } == pagepkg::Wide::SpacerHead {
                // end_cell.wide = .narrow;
                unsafe { (*end_cell).set_wide(pagepkg::Wide::Narrow) };
            }
        }

        // If the leftmost or rightmost cells of our scrolling region
        // are parts of wide chars, we need to clear the cells' contents
        // since they'd be split by the move.
        // const left_cell: *Cell = &cells[self.scrolling_region.left];
        let left_cell: *mut pagepkg::Cell =
            unsafe { cells.add(self.scrolling_region.left as usize) };
        // const right_cell: *Cell = &cells[self.scrolling_region.right];
        let right_cell: *mut pagepkg::Cell =
            unsafe { cells.add(self.scrolling_region.right as usize) };

        // if (left_cell.wide == .spacer_tail) {
        if unsafe { (*left_cell).wide() } == pagepkg::Wide::SpacerTail {
            // const wide_cell: *Cell = &cells[self.scrolling_region.left - 1];
            let wide_cell: *mut pagepkg::Cell =
                unsafe { cells.add((self.scrolling_region.left - 1) as usize) };
            // if (wide_cell.hasGrapheme()) {
            if unsafe { (*wide_cell).has_grapheme() } {
                // page.clearGrapheme(wide_cell);
                unsafe { (*page).clear_grapheme(wide_cell) };
                // page.updateRowGraphemeFlag(row);
                unsafe { (*page).update_row_grapheme_flag(row) };
            }
            // wide_cell.content.codepoint = 0;
            unsafe { (*wide_cell).set_content_codepoint(0) };
            // wide_cell.wide = .narrow;
            unsafe { (*wide_cell).set_wide(pagepkg::Wide::Narrow) };
            // left_cell.wide = .narrow;
            unsafe { (*left_cell).set_wide(pagepkg::Wide::Narrow) };
        }

        // if (right_cell.wide == .wide) {
        if unsafe { (*right_cell).wide() } == pagepkg::Wide::Wide {
            // const tail_cell: *Cell = &cells[self.scrolling_region.right + 1];
            let tail_cell: *mut pagepkg::Cell =
                unsafe { cells.add((self.scrolling_region.right + 1) as usize) };
            // if (right_cell.hasGrapheme()) {
            if unsafe { (*right_cell).has_grapheme() } {
                // page.clearGrapheme(right_cell);
                unsafe { (*page).clear_grapheme(right_cell) };
                // page.updateRowGraphemeFlag(row);
                unsafe { (*page).update_row_grapheme_flag(row) };
            }
            // right_cell.content.codepoint = 0;
            unsafe { (*right_cell).set_content_codepoint(0) };
            // right_cell.wide = .narrow;
            unsafe { (*right_cell).set_wide(pagepkg::Wide::Narrow) };
            // tail_cell.wide = .narrow;
            unsafe { (*tail_cell).set_wide(pagepkg::Wide::Narrow) };
        }
    }

    /// Renew every live page generation in an inclusive range before a
    /// full-width line operation moves logical rows between their coordinates.
    ///
    /// PORTS: oracle@main Terminal.zig `invalidateFullWidthRowRange`
    /// (0bad91adb).
    //
    // PORT NOTE: raw-pointer node arguments, never `&mut Node` — the caller
    // holds live tracked-pin aliases into these nodes across the call (memory:
    // `rust-mut-args-strongly-protected`).
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    fn invalidate_full_width_row_range(
        &mut self,
        first: *mut crate::pagelist::Node,
        last: *mut crate::pagelist::Node,
    ) {
        let mut node = first;
        loop {
            // Full-width line movement remaps cached row coordinates in this page.
            unsafe { (*self.screens.active).pages.invalidate_node_layout(node) };
            if std::ptr::eq(node, last) {
                break;
            }
            node = unsafe { (*node).next };
        }
    }

    // TODO(qwerasd): `insertLines` and `deleteLines` are 99% identical,
    // the majority of their logic can (and should) be abstracted in to
    // a single shared helper function, probably on `Screen` not here.
    // I'm just too lazy to do that rn :p

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
        // Rare, but happens
        // if (count == 0) return;
        if count == 0 {
            return;
        }

        // If the cursor is outside the scroll region we do nothing.
        // if (self.screens.active.cursor.y < self.scrolling_region.top or
        //     self.screens.active.cursor.y > self.scrolling_region.bottom or
        //     self.screens.active.cursor.x < self.scrolling_region.left or
        //     self.screens.active.cursor.x > self.scrolling_region.right) return;
        if unsafe { (*self.screens.active).cursor.y } < self.scrolling_region.top
            || unsafe { (*self.screens.active).cursor.y } > self.scrolling_region.bottom
            || unsafe { (*self.screens.active).cursor.x } < self.scrolling_region.left
            || unsafe { (*self.screens.active).cursor.x } > self.scrolling_region.right
        {
            return;
        }

        // if (comptime build_options.kitty_graphics) {
        //     // Scrolling dirties the images because it updates their placements pins.
        //     self.screens.active.kitty_images.dirty = true;
        // }
        if crate::options::KITTY_GRAPHICS {
            // SAFETY: `self.screens.active` is a stable heap pointer owned by
            // `self.screens`; we hold `&mut self`.
            unsafe { (*self.screens.active).kitty_images.dirty = true };
        }

        // At the end we need to return the cursor to the row it started on.
        // const start_y = self.screens.active.cursor.y;
        let start_y = unsafe { (*self.screens.active).cursor.y };
        // defer {
        //     self.screens.active.cursorAbsolute(self.scrolling_region.left, start_y);
        //     // Always unset pending wrap
        //     self.screens.active.cursor.pending_wrap = false;
        // }
        // PORT NOTE: RAII guard reproduces the reference `defer`, restoring the
        // cursor to the left margin / start row and clearing pending_wrap on
        // every exit path.
        let _restore = ShiftLinesEndGuard {
            screen: self.screens.active,
            left: self.scrolling_region.left,
            start_y,
        };

        // We have a slower path if we have left or right scroll margins.
        // const left_right = self.scrolling_region.left > 0 or
        //     self.scrolling_region.right < self.cols - 1;
        let left_right =
            self.scrolling_region.left > 0 || self.scrolling_region.right < self.cols - 1;

        // Remaining rows from our cursor to the bottom of the scroll region.
        // const rem = self.scrolling_region.bottom - self.screens.active.cursor.y + 1;
        let rem = self.scrolling_region.bottom - unsafe { (*self.screens.active).cursor.y } + 1;

        // We can only insert lines up to our remaining lines in the scroll
        // region. So we take whichever is smaller.
        // const adjusted_count = @min(count, rem);
        let adjusted_count = core::cmp::min(count, rem as usize) as size::CellCountInt;

        // Create a new tracked pin which we'll use to navigate the page list
        // so that if we need to adjust capacity it will be properly tracked.
        // var cur_p = self.screens.active.pages.trackPin(
        //     self.screens.active.cursor.page_pin.down(rem - 1).?,
        // ) catch |err| { ... @panic("insertLines trackPin OOM"); };
        // defer self.screens.active.pages.untrackPin(cur_p);
        let down_pin =
            unsafe { (*(*self.screens.active).cursor.page_pin).down((rem - 1) as usize) }.unwrap();
        let cur_p: *mut crate::pagelist::Pin = match self.active_mut().pages.track_pin(down_pin) {
            Ok(p) => p,
            Err(err) => {
                // comptime assert(@TypeOf(err) == error{OutOfMemory});
                //
                // This error scenario means that our GPA is OOM. This is not a
                // situation we can gracefully handle. We can't just ignore insertLines
                // because it'll result in a corrupted screen. Ideally in the future
                // we flag the state as broken and show an error message to the user.
                // For now, we panic.
                log::error!(target: LOG_SCOPE, "insertLines trackPin error err={err:?}");
                panic!("insertLines trackPin OOM");
            }
        };
        // PORT NOTE: RAII guard reproduces `defer untrackPin(cur_p)`.
        let _untrack = TrackedPinGuard {
            screen: self.screens.active,
            pin: cur_p,
        };

        // Partial-width margins edit cells in stable rows; full-width moves rows.
        if !left_right {
            self.invalidate_full_width_row_range(
                unsafe { (*(*self.screens.active).cursor.page_pin).node },
                unsafe { (*cur_p).node },
            );
        }

        // Our current y position relative to the cursor
        // var y: usize = rem;
        let mut y: usize = rem as usize;

        // Traverse from the bottom up
        // while (y > 0) {
        while y > 0 {
            // const cur_rac = cur_p.rowAndCell();
            // const cur_row: *Row = cur_rac.row;
            let cur_row: *mut Row = unsafe { (*cur_p).row_and_cell() }.row;

            // If this is one of the lines we need to shift, do so
            // if (y > adjusted_count) {
            if y > adjusted_count as usize {
                // const off_p = cur_p.up(adjusted_count).?;
                let off_p = unsafe { (*cur_p).up(adjusted_count as usize) }.unwrap();
                // const off_rac = off_p.rowAndCell();
                // const off_row: *Row = off_rac.row;
                let off_row: *mut Row = off_p.row_and_cell().row;

                // self.rowWillBeShifted(&cur_p.node.data, cur_row);
                let cur_page: *mut Page = unsafe { crate::pagelist::Node::page((*cur_p).node) };
                self.row_will_be_shifted(cur_page, cur_row);
                // self.rowWillBeShifted(&off_p.node.data, off_row);
                let off_page: *mut Page = unsafe { crate::pagelist::Node::page(off_p.node) };
                self.row_will_be_shifted(off_page, off_row);

                // If our scrolling region is full width, then we unset wrap.
                // if (!left_right) {
                if !left_right {
                    // off_row.wrap = false;
                    unsafe { (*off_row).set_wrap(false) };
                    // cur_row.wrap = false;
                    unsafe { (*cur_row).set_wrap(false) };
                    // off_row.wrap_continuation = false;
                    unsafe { (*off_row).set_wrap_continuation(false) };
                    // cur_row.wrap_continuation = false;
                    unsafe { (*cur_row).set_wrap_continuation(false) };
                }

                // const src_p = off_p;
                // const src_row = off_row;
                // const dst_p = cur_p;
                // const dst_row = cur_row;
                let src_p = off_p;
                let src_row = off_row;
                // dst_p is cur_p (the tracked pin), dst_row is cur_row.

                // If our page doesn't match, then we need to do a copy from
                // one page to another. This is the slow path.
                // if (src_p.node != dst_p.node) {
                if !std::ptr::eq(src_p.node, unsafe { (*cur_p).node }) {
                    // The copy may replace the destination node in order
                    // to increase its capacity. Our pins are tracked so
                    // they update automatically; we can discard the
                    // replacement because the remainder of this iteration
                    // only accesses rows through the pins.
                    let dst_node = unsafe { (*cur_p).node };
                    let dst_y = unsafe { (*cur_p).y } as usize;
                    let src_page: *mut Page = unsafe { crate::pagelist::Node::page(src_p.node) };
                    let x_start = self.scrolling_region.left as usize;
                    let x_end = (self.scrolling_region.right + 1) as usize;
                    let _ = self.active_mut().clone_partial_row_grow_capacity(
                        dst_node, dst_y, src_page, src_row, x_start, x_end,
                    );
                } else {
                    // if (!left_right) {
                    if !left_right {
                        // Swap the src/dst cells. This ensures that our dst gets the
                        // proper shifted rows and src gets non-garbage cell data that
                        // we can clear.
                        // const dst = dst_row.*;
                        // dst_row.* = src_row.*;
                        // src_row.* = dst;
                        // PORT NOTE: `Row` is `Copy`; the three-step swap of the
                        // pointed-to rows is `core::ptr::swap` (same memory effect,
                        // no overlap since src_p.node == dst_p.node but distinct rows).
                        unsafe { core::ptr::swap(cur_row, src_row) };

                        // Ensure what we did didn't corrupt the page
                        // cur_p.node.data.assertIntegrity();
                        let cur_page: *const Page =
                            unsafe { crate::pagelist::Node::page((*cur_p).node) };
                        unsafe { (*cur_page).assert_integrity() };
                    } else {
                        // Left/right scroll margins we have to
                        // copy cells, which is much slower...
                        // const page = &cur_p.node.data;
                        let page: *mut Page = unsafe { crate::pagelist::Node::page((*cur_p).node) };
                        // page.moveCells(src_row, self.scrolling_region.left,
                        //     dst_row, self.scrolling_region.left,
                        //     (self.scrolling_region.right - self.scrolling_region.left) + 1);
                        unsafe {
                            (*page).move_cells(
                                src_row,
                                self.scrolling_region.left as usize,
                                cur_row,
                                self.scrolling_region.left as usize,
                                ((self.scrolling_region.right - self.scrolling_region.left) + 1)
                                    as usize,
                            )
                        };
                    }
                }
            } else {
                // Clear the cells for this row, it has been shifted.
                // self.rowWillBeShifted(&cur_p.node.data, cur_row);
                let cur_page: *mut Page = unsafe { crate::pagelist::Node::page((*cur_p).node) };
                self.row_will_be_shifted(cur_page, cur_row);
                // const page = &cur_p.node.data;
                let page: *mut Page = unsafe { crate::pagelist::Node::page((*cur_p).node) };
                // const cells = page.getCells(cur_row);
                let cells: *mut [pagepkg::Cell] = unsafe { (*page).get_cells(cur_row) };
                // self.screens.active.clearCells(page, cur_row,
                //     cells[self.scrolling_region.left .. self.scrolling_region.right + 1]);
                let sub = ptr_slice_range(
                    cells,
                    self.scrolling_region.left as usize,
                    (self.scrolling_region.right + 1) as usize,
                );
                self.active_mut().clear_cells(page, cur_row, sub);

                // With a full-width scroll region the entire row is a
                // fresh blank row: reset the metadata so nothing (wrap
                // state, semantic prompt) is retained from the row whose
                // storage it recycles. With left/right margins the row
                // keeps content outside the margins so the metadata is
                // preserved, matching the shift case above.
                // if (!left_right) cur_row.reset();
                if !left_right {
                    unsafe { (*cur_row).reset() };
                }
            }

            // Mark the row as dirty
            // cur_p.markDirty();
            unsafe { (*cur_p).mark_dirty() };

            // We have successfully processed a line.
            // y -= 1;
            y -= 1;
            // Move our pin up to the next row.
            // if (cur_p.up(1)) |p| cur_p.* = p;
            if let Some(p) = unsafe { (*cur_p).up(1) } {
                unsafe { *cur_p = p };
            }
        }
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
        // Rare, but happens
        // if (count == 0) return;
        if count == 0 {
            return;
        }

        // If the cursor is outside the scroll region we do nothing.
        // if (self.screens.active.cursor.y < self.scrolling_region.top or
        //     self.screens.active.cursor.y > self.scrolling_region.bottom or
        //     self.screens.active.cursor.x < self.scrolling_region.left or
        //     self.screens.active.cursor.x > self.scrolling_region.right) return;
        if unsafe { (*self.screens.active).cursor.y } < self.scrolling_region.top
            || unsafe { (*self.screens.active).cursor.y } > self.scrolling_region.bottom
            || unsafe { (*self.screens.active).cursor.x } < self.scrolling_region.left
            || unsafe { (*self.screens.active).cursor.x } > self.scrolling_region.right
        {
            return;
        }

        // if (comptime build_options.kitty_graphics) {
        //     // Scrolling dirties the images because it updates their placements pins.
        //     self.screens.active.kitty_images.dirty = true;
        // }
        if crate::options::KITTY_GRAPHICS {
            // SAFETY: `self.screens.active` is a stable heap pointer owned by
            // `self.screens`; we hold `&mut self`.
            unsafe { (*self.screens.active).kitty_images.dirty = true };
        }

        // At the end we need to return the cursor to the row it started on.
        // const start_y = self.screens.active.cursor.y;
        let start_y = unsafe { (*self.screens.active).cursor.y };
        // defer {
        //     self.screens.active.cursorAbsolute(self.scrolling_region.left, start_y);
        //     // Always unset pending wrap
        //     self.screens.active.cursor.pending_wrap = false;
        // }
        // PORT NOTE: RAII guard reproduces the reference `defer` (see insertLines).
        let _restore = ShiftLinesEndGuard {
            screen: self.screens.active,
            left: self.scrolling_region.left,
            start_y,
        };

        // We have a slower path if we have left or right scroll margins.
        // const left_right = self.scrolling_region.left > 0 or
        //     self.scrolling_region.right < self.cols - 1;
        let left_right =
            self.scrolling_region.left > 0 || self.scrolling_region.right < self.cols - 1;

        // Remaining rows from our cursor to the bottom of the scroll region.
        // const rem = self.scrolling_region.bottom - self.screens.active.cursor.y + 1;
        let rem = self.scrolling_region.bottom - unsafe { (*self.screens.active).cursor.y } + 1;

        // We can only insert lines up to our remaining lines in the scroll
        // region. So we take whichever is smaller.
        // const adjusted_count = @min(count, rem);
        let adjusted_count = core::cmp::min(count, rem as usize) as size::CellCountInt;

        // Create a new tracked pin which we'll use to navigate the page list
        // so that if we need to adjust capacity it will be properly tracked.
        // var cur_p = self.screens.active.pages.trackPin(
        //     self.screens.active.cursor.page_pin.*,
        // ) catch |err| { ... @panic("deleteLines trackPin OOM"); };
        // defer self.screens.active.pages.untrackPin(cur_p);
        let cursor_pin = unsafe { *(*self.screens.active).cursor.page_pin };
        let cur_p: *mut crate::pagelist::Pin = match self.active_mut().pages.track_pin(cursor_pin) {
            Ok(p) => p,
            Err(err) => {
                // See insertLines
                // comptime assert(@TypeOf(err) == error{OutOfMemory});
                log::error!(target: LOG_SCOPE, "deleteLines trackPin error err={err:?}");
                panic!("deleteLines trackPin OOM");
            }
        };
        // PORT NOTE: RAII guard reproduces `defer untrackPin(cur_p)`.
        let _untrack = TrackedPinGuard {
            screen: self.screens.active,
            pin: cur_p,
        };

        // Partial-width margins edit cells in stable rows; full-width moves rows.
        if !left_right {
            let last = unsafe { (*cur_p).down((rem - 1) as usize) }.unwrap().node;
            self.invalidate_full_width_row_range(unsafe { (*cur_p).node }, last);
        }

        // Our current y position relative to the cursor
        // var y: usize = 0;
        let mut y: usize = 0;

        // Traverse from the top down
        // while (y < rem) {
        while y < rem as usize {
            // const cur_rac = cur_p.rowAndCell();
            // const cur_row: *Row = cur_rac.row;
            let cur_row: *mut Row = unsafe { (*cur_p).row_and_cell() }.row;

            // If this is one of the lines we need to shift, do so
            // if (y < rem - adjusted_count) {
            if y < (rem - adjusted_count) as usize {
                // const off_p = cur_p.down(adjusted_count).?;
                let off_p = unsafe { (*cur_p).down(adjusted_count as usize) }.unwrap();
                // const off_rac = off_p.rowAndCell();
                // const off_row: *Row = off_rac.row;
                let off_row: *mut Row = off_p.row_and_cell().row;

                // self.rowWillBeShifted(&cur_p.node.data, cur_row);
                let cur_page: *mut Page = unsafe { crate::pagelist::Node::page((*cur_p).node) };
                self.row_will_be_shifted(cur_page, cur_row);
                // self.rowWillBeShifted(&off_p.node.data, off_row);
                let off_page: *mut Page = unsafe { crate::pagelist::Node::page(off_p.node) };
                self.row_will_be_shifted(off_page, off_row);

                // If our scrolling region is full width, then we unset wrap.
                // if (!left_right) {
                if !left_right {
                    // off_row.wrap = false;
                    unsafe { (*off_row).set_wrap(false) };
                    // cur_row.wrap = false;
                    unsafe { (*cur_row).set_wrap(false) };
                    // off_row.wrap_continuation = false;
                    unsafe { (*off_row).set_wrap_continuation(false) };
                    // cur_row.wrap_continuation = false;
                    unsafe { (*cur_row).set_wrap_continuation(false) };
                }

                // const src_p = off_p;
                // const src_row = off_row;
                // const dst_p = cur_p;
                // const dst_row = cur_row;
                let src_p = off_p;
                let src_row = off_row;
                // dst_p is cur_p (the tracked pin), dst_row is cur_row.

                // If our page doesn't match, then we need to do a copy from
                // one page to another. This is the slow path.
                // if (src_p.node != dst_p.node) {
                if !std::ptr::eq(src_p.node, unsafe { (*cur_p).node }) {
                    // The copy may replace the destination node in order
                    // to increase its capacity. Our pins are tracked so
                    // they update automatically; we can discard the
                    // replacement because the remainder of this iteration
                    // only accesses rows through the pins.
                    let dst_node = unsafe { (*cur_p).node };
                    let dst_y = unsafe { (*cur_p).y } as usize;
                    let src_page: *mut Page = unsafe { crate::pagelist::Node::page(src_p.node) };
                    let x_start = self.scrolling_region.left as usize;
                    let x_end = (self.scrolling_region.right + 1) as usize;
                    let _ = self.active_mut().clone_partial_row_grow_capacity(
                        dst_node, dst_y, src_page, src_row, x_start, x_end,
                    );
                } else {
                    // if (!left_right) {
                    if !left_right {
                        // Swap the src/dst cells. This ensures that our dst gets the
                        // proper shifted rows and src gets non-garbage cell data that
                        // we can clear.
                        // const dst = dst_row.*;
                        // dst_row.* = src_row.*;
                        // src_row.* = dst;
                        // PORT NOTE: see insertLines — `core::ptr::swap` of the rows.
                        unsafe { core::ptr::swap(cur_row, src_row) };

                        // Ensure what we did didn't corrupt the page
                        // cur_p.node.data.assertIntegrity();
                        let cur_page: *const Page =
                            unsafe { crate::pagelist::Node::page((*cur_p).node) };
                        unsafe { (*cur_page).assert_integrity() };
                    } else {
                        // Left/right scroll margins we have to
                        // copy cells, which is much slower...
                        // const page = &cur_p.node.data;
                        let page: *mut Page = unsafe { crate::pagelist::Node::page((*cur_p).node) };
                        // page.moveCells(src_row, self.scrolling_region.left,
                        //     dst_row, self.scrolling_region.left,
                        //     (self.scrolling_region.right - self.scrolling_region.left) + 1);
                        unsafe {
                            (*page).move_cells(
                                src_row,
                                self.scrolling_region.left as usize,
                                cur_row,
                                self.scrolling_region.left as usize,
                                ((self.scrolling_region.right - self.scrolling_region.left) + 1)
                                    as usize,
                            )
                        };
                    }
                }
            } else {
                // Clear the cells for this row, it's from out of bounds.
                // self.rowWillBeShifted(&cur_p.node.data, cur_row);
                let cur_page: *mut Page = unsafe { crate::pagelist::Node::page((*cur_p).node) };
                self.row_will_be_shifted(cur_page, cur_row);
                // const page = &cur_p.node.data;
                let page: *mut Page = unsafe { crate::pagelist::Node::page((*cur_p).node) };
                // const cells = page.getCells(cur_row);
                let cells: *mut [pagepkg::Cell] = unsafe { (*page).get_cells(cur_row) };
                // self.screens.active.clearCells(page, cur_row,
                //     cells[self.scrolling_region.left .. self.scrolling_region.right + 1]);
                let sub = ptr_slice_range(
                    cells,
                    self.scrolling_region.left as usize,
                    (self.scrolling_region.right + 1) as usize,
                );
                self.active_mut().clear_cells(page, cur_row, sub);

                // With a full-width scroll region the entire row is a
                // fresh blank row: reset the metadata so nothing (wrap
                // state, semantic prompt) is retained from the row whose
                // storage it recycles. With left/right margins the row
                // keeps content outside the margins so the metadata is
                // preserved, matching the shift case above.
                // if (!left_right) cur_row.reset();
                if !left_right {
                    unsafe { (*cur_row).reset() };
                }
            }

            // Mark the row as dirty
            // cur_p.markDirty();
            unsafe { (*cur_p).mark_dirty() };

            // We have successfully processed a line.
            // y += 1;
            y += 1;
            // Move our pin down to the next row.
            // if (cur_p.down(1)) |p| cur_p.* = p;
            if let Some(p) = unsafe { (*cur_p).down(1) } {
                unsafe { *cur_p = p };
            }
        }
    }

    /// Inserts spaces at current cursor position moving existing cell contents
    /// to the right. The contents of the count right-most columns in the scroll
    /// region are lost. The cursor position is not changed.
    ///
    /// This unsets the pending wrap state without wrapping.
    ///
    /// The inserted cells are colored according to the current SGR state.
    pub fn insert_blanks(&mut self, count: usize) {
        // Unset pending wrap state without wrapping. Note: this purposely
        // happens BEFORE the scroll region check below, because that's what
        // xterm does.
        // self.screens.active.cursor.pending_wrap = false;
        unsafe { (*self.screens.active).cursor.pending_wrap = false };

        // If we're given a zero then we do nothing. The rest of this function
        // assumes count > 0 and will crash if zero so return early. Note that
        // this shouldn't be possible with real CSI sequences because the value
        // is clamped to 1 min.
        // if (count == 0) return;
        if count == 0 {
            return;
        }

        // If our cursor is outside the margins then do nothing. We DO reset
        // wrap state still so this must remain below the above logic.
        // if (self.screens.active.cursor.x < self.scrolling_region.left or
        //     self.screens.active.cursor.x > self.scrolling_region.right) return;
        if unsafe { (*self.screens.active).cursor.x } < self.scrolling_region.left
            || unsafe { (*self.screens.active).cursor.x } > self.scrolling_region.right
        {
            return;
        }

        // If our count is larger than the remaining amount, we just erase right.
        // We only do this if we can erase the entire line (no right margin).
        // if (right_limit == self.cols and
        //     count > right_limit - self.screens.active.cursor.x)
        // {
        //     self.eraseLine(.right, false);
        //     return;
        // }

        // left is just the cursor position but as a multi-pointer
        // const left: [*]Cell = @ptrCast(self.screens.active.cursor.page_cell);
        let left: *mut pagepkg::Cell = unsafe { (*self.screens.active).cursor.page_cell };
        // var page = &self.screens.active.cursor.page_pin.node.data;
        let page: *mut Page =
            unsafe { crate::pagelist::Node::page((*(*self.screens.active).cursor.page_pin).node) };

        // If our X is a wide spacer tail then we need to erase the
        // previous cell too so we don't split a multi-cell character.
        // if (self.screens.active.cursor.page_cell.wide == .spacer_tail) {
        if unsafe { (*(*self.screens.active).cursor.page_cell).wide() } == pagepkg::Wide::SpacerTail
        {
            // assert(self.screens.active.cursor.x > 0);
            debug_assert!(unsafe { (*self.screens.active).cursor.x } > 0);
            // self.screens.active.clearCells(page, self.screens.active.cursor.page_row, (left - 1)[0..2]);
            let row: *mut Row = unsafe { (*self.screens.active).cursor.page_row };
            let cells = core::ptr::slice_from_raw_parts_mut(unsafe { left.sub(1) }, 2);
            self.active_mut().clear_cells(page, row, cells);
        }

        // Remaining cols from our cursor to the right margin.
        // const rem = self.scrolling_region.right - self.screens.active.cursor.x + 1;
        let rem = self.scrolling_region.right - unsafe { (*self.screens.active).cursor.x } + 1;

        // If the cell at the right margin is wide, its spacer tail is
        // outside the scroll region and would be orphaned by either the
        // shift or the clear. Clean up both halves up front.
        {
            // const right_cell: *Cell = @ptrCast(left + (rem - 1));
            let right_cell: *mut pagepkg::Cell = unsafe { left.add((rem - 1) as usize) };
            // if (right_cell.wide == .wide) self.screens.active.clearCells(
            //     page, self.screens.active.cursor.page_row,
            //     @as([*]Cell, @ptrCast(right_cell))[0..2]);
            if unsafe { (*right_cell).wide() } == pagepkg::Wide::Wide {
                let row: *mut Row = unsafe { (*self.screens.active).cursor.page_row };
                let cells = core::ptr::slice_from_raw_parts_mut(right_cell, 2);
                self.active_mut().clear_cells(page, row, cells);
            }
        }

        // We can only insert blanks up to our remaining cols
        // const adjusted_count = @min(count, rem);
        let adjusted_count = core::cmp::min(count, rem as usize) as size::CellCountInt;

        // This is the amount of space at the right of the scroll region
        // that will NOT be blank, so we need to shift the correct cols right.
        // "scroll_amount" is the number of such cols.
        // const scroll_amount = rem - adjusted_count;
        let scroll_amount = rem - adjusted_count;
        // if (scroll_amount > 0) {
        if scroll_amount > 0 {
            // page.pauseIntegrityChecks(true);
            unsafe { (*page).pause_integrity_checks(true) };
            // defer page.pauseIntegrityChecks(false);

            // var x: [*]Cell = left + (scroll_amount - 1);
            let mut x: *mut pagepkg::Cell = unsafe { left.add((scroll_amount - 1) as usize) };

            // If our last cell we're shifting is wide, then we need to clear
            // it to be empty so we don't split the multi-cell char.
            // const end: *Cell = @ptrCast(x);
            let end: *mut pagepkg::Cell = x;
            // if (end.wide == .wide) {
            if unsafe { (*end).wide() } == pagepkg::Wide::Wide {
                // const end_multi: [*]Cell = @ptrCast(end);
                let end_multi: *mut pagepkg::Cell = end;
                // assert(end_multi[1].wide == .spacer_tail);
                debug_assert!(unsafe { (*end_multi.add(1)).wide() } == pagepkg::Wide::SpacerTail);
                // self.screens.active.clearCells(page, self.screens.active.cursor.page_row, end_multi[0..2]);
                let row: *mut Row = unsafe { (*self.screens.active).cursor.page_row };
                let cells = core::ptr::slice_from_raw_parts_mut(end_multi, 2);
                self.active_mut().clear_cells(page, row, cells);
            }

            // We work backwards so we don't overwrite data.
            // while (@intFromPtr(x) >= @intFromPtr(left)) : (x -= 1) {
            while (x as usize) >= (left as usize) {
                // const src: *Cell = @ptrCast(x);
                let src: *mut pagepkg::Cell = x;
                // const dst: *Cell = @ptrCast(x + adjusted_count);
                let dst: *mut pagepkg::Cell = unsafe { x.add(adjusted_count as usize) };
                // page.swapCells(src, dst);
                unsafe { (*page).swap_cells(src, dst) };
                // PORT NOTE: the reference `x -= 1` decrements a `[*]Cell` (one
                // Cell). On the final iteration `x == left`, so the next
                // decrement would form a pointer one-before-`left`; the loop then
                // exits on the condition. We mirror this without computing an
                // out-of-bounds pointer: decrement only while we won't underflow,
                // otherwise break (the condition `x >= left` would be false).
                if std::ptr::eq(x, left) {
                    break;
                }
                x = unsafe { x.sub(1) };
            }

            unsafe { (*page).pause_integrity_checks(false) };
        }

        // Insert blanks. The blanks preserve the background color.
        // self.screens.active.clearCells(page, self.screens.active.cursor.page_row, left[0..adjusted_count]);
        let row: *mut Row = unsafe { (*self.screens.active).cursor.page_row };
        let cells = core::ptr::slice_from_raw_parts_mut(left, adjusted_count as usize);
        self.active_mut().clear_cells(page, row, cells);

        // Our row is always dirty
        // self.screens.active.cursorMarkDirty();
        self.active_mut().cursor_mark_dirty();
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
        if count_req == 0 {
            return;
        }

        // If our cursor is outside the margins then do nothing. We DO reset
        // wrap state still so this must remain below the above logic.
        if unsafe { (*self.screens.active).cursor.x } < self.scrolling_region.left
            || unsafe { (*self.screens.active).cursor.x } > self.scrolling_region.right
        {
            return;
        }

        // left is just the cursor position but as a multi-pointer
        // const left: [*]Cell = @ptrCast(self.screens.active.cursor.page_cell);
        let left: *mut pagepkg::Cell = unsafe { (*self.screens.active).cursor.page_cell };
        // var page = &self.screens.active.cursor.page_pin.node.data;
        let page: *mut Page =
            unsafe { crate::pagelist::Node::page((*(*self.screens.active).cursor.page_pin).node) };

        // Remaining cols from our cursor to the right margin.
        let rem = self.scrolling_region.right - unsafe { (*self.screens.active).cursor.x } + 1;

        // We can only insert blanks up to our remaining cols
        let count = core::cmp::min(count_req, rem as usize) as size::CellCountInt;

        let cursor_x = unsafe { (*self.screens.active).cursor.x };
        let sr_right = self.scrolling_region.right;
        self.active_mut().split_cell_boundary(cursor_x);
        self.active_mut().split_cell_boundary(cursor_x + count);
        self.active_mut().split_cell_boundary(sr_right + 1);

        // This is the amount of space at the right of the scroll region
        // that will NOT be blank, so we need to shift the correct cols right.
        // "scroll_amount" is the number of such cols.
        let scroll_amount = rem - count;
        // var x: [*]Cell = left;
        let mut x: *mut pagepkg::Cell = left;
        if scroll_amount > 0 {
            unsafe { (*page).pause_integrity_checks(true) };
            // defer page.pauseIntegrityChecks(false);

            // const right: [*]Cell = left + (scroll_amount - 1);
            let right: *mut pagepkg::Cell = unsafe { left.add((scroll_amount - 1) as usize) };

            // while (@intFromPtr(x) <= @intFromPtr(right)) : (x += 1) {
            while (x as usize) <= (right as usize) {
                // const src: *Cell = @ptrCast(x + count);
                let src: *mut pagepkg::Cell = unsafe { x.add(count as usize) };
                // const dst: *Cell = @ptrCast(x);
                let dst: *mut pagepkg::Cell = x;
                unsafe { (*page).swap_cells(src, dst) };
                x = unsafe { x.add(1) };
            }

            unsafe { (*page).pause_integrity_checks(false) };
        }

        // Insert blanks. The blanks preserve the background color.
        // self.screens.active.clearCells(page, ..., x[0 .. rem - scroll_amount]);
        let row: *mut Row = unsafe { (*self.screens.active).cursor.page_row };
        let cells = core::ptr::slice_from_raw_parts_mut(x, (rem - scroll_amount) as usize);
        self.active_mut().clear_cells(page, row, cells);

        // Our row's soft-wrap is always reset.
        self.active_mut().cursor_reset_wrap();

        // Our row is always dirty
        self.active_mut().cursor_mark_dirty();
    }

    pub fn erase_chars(&mut self, count_req: usize) {
        let count = {
            let remaining = self.cols - unsafe { (*self.screens.active).cursor.x };
            // var end = @min(remaining, @max(count_req, 1));
            // PORT NOTE (VALIDATION_REPORT §3c.8): Zig's peer-type resolution
            // runs the `@min`/`@max` in usize and only then uses the (≤
            // remaining, so ≤ u16::MAX) result in u16 contexts. Clamp in usize
            // FIRST, then narrow — truncating `count_req` to u16 before the min
            // would mangle counts > u16::MAX (e.g. 65537 → 1), reachable via
            // this pub API / FFI.
            let mut end = core::cmp::min(remaining as usize, core::cmp::max(count_req, 1))
                as size::CellCountInt;

            // If our last cell is a wide char then we need to also clear the
            // cell beyond it since we can't just split a wide char.
            if end != remaining {
                let last = self.active_mut().cursor_cell_right(end - 1);
                if unsafe { (*last).wide() } == pagepkg::Wide::Wide {
                    end += 1;
                }
            }

            end
        };

        // Handle any boundary conditions on the edges of the erased area.
        //
        // TODO(qwerasd): This isn't actually correct if you take in to account
        // protected modes. We need to figure out how to make `clearCells` or at
        // least `clearUnprotectedCells` handle boundary conditions...
        let cursor_x = unsafe { (*self.screens.active).cursor.x };
        self.active_mut().split_cell_boundary(cursor_x);
        self.active_mut().split_cell_boundary(cursor_x + count);

        // Reset our row's soft-wrap.
        self.active_mut().cursor_reset_wrap();

        // Mark our cursor row as dirty
        self.active_mut().cursor_mark_dirty();

        // Clear the cells
        // const cells: [*]Cell = @ptrCast(self.screens.active.cursor.page_cell);
        let cells: *mut pagepkg::Cell = unsafe { (*self.screens.active).cursor.page_cell };

        let page: *mut Page =
            unsafe { crate::pagelist::Node::page((*(*self.screens.active).cursor.page_pin).node) };
        let row: *mut Row = unsafe { (*self.screens.active).cursor.page_row };
        let cells_slice = core::ptr::slice_from_raw_parts_mut(cells, count as usize);

        // If we never had a protection mode, then we can assume no cells
        // are protected and go with the fast path. If the last protection
        // mode was not ISO we also always ignore protection attributes.
        if unsafe { (*self.screens.active).protected_mode } != ansi::ProtectedMode::Iso {
            self.active_mut().clear_cells(page, row, cells_slice);
            return;
        }

        self.active_mut()
            .clear_unprotected_cells(page, row, cells_slice);
    }

    /// Erase the line.
    pub fn erase_line(&mut self, mode: csi::EraseLine, protected_req: bool) {
        // Get our start/end positions depending on mode.
        let (start, end): (size::CellCountInt, size::CellCountInt) = match mode {
            csi::EraseLine::Right => {
                let mut x = unsafe { (*self.screens.active).cursor.x };

                // If our X is a wide spacer tail then we need to erase the
                // previous cell too so we don't split a multi-cell character.
                if x > 0
                    && unsafe { (*(*self.screens.active).cursor.page_cell).wide() }
                        == pagepkg::Wide::SpacerTail
                {
                    x -= 1;
                }

                // Reset our row's soft-wrap.
                self.active_mut().cursor_reset_wrap();

                (x, self.cols)
            }

            csi::EraseLine::Left => {
                let mut x = unsafe { (*self.screens.active).cursor.x };

                // If our x is a wide char we need to delete the tail too.
                if unsafe { (*(*self.screens.active).cursor.page_cell).wide() }
                    == pagepkg::Wide::Wide
                {
                    x += 1;
                }

                (0, x + 1)
            }

            // .complete => complete: { ... }
            csi::EraseLine::Complete => {
                // Xterm preserves this flag for EL2, but it also doesn't reflow
                // rows when resizing. Since we do, the erased row must no longer
                // continue onto the next row.
                self.active_mut().cursor_reset_wrap();

                (0, self.cols)
            }

            _ => {
                log::error!(target: LOG_SCOPE, "unimplemented erase line mode: {mode:?}");
                return;
            }
        };

        // All modes will clear the pending wrap state and we know we have
        // a valid mode at this point.
        unsafe { (*self.screens.active).cursor.pending_wrap = false };

        // We always mark our row as dirty
        self.active_mut().cursor_mark_dirty();

        // Start of our cells
        // const cells: [*]Cell = cells - self.screens.active.cursor.x;
        let cells: *mut pagepkg::Cell = {
            let cells: *mut pagepkg::Cell = unsafe { (*self.screens.active).cursor.page_cell };
            let x = unsafe { (*self.screens.active).cursor.x };
            unsafe { cells.sub(x as usize) }
        };

        // We respect protected attributes if explicitly requested (probably
        // a DECSEL sequence) or if our last protected mode was ISO even if its
        // not currently set.
        let protected = unsafe { (*self.screens.active).protected_mode }
            == ansi::ProtectedMode::Iso
            || protected_req;

        let page: *mut Page =
            unsafe { crate::pagelist::Node::page((*(*self.screens.active).cursor.page_pin).node) };
        let row: *mut Row = unsafe { (*self.screens.active).cursor.page_row };
        // cells[start..end]
        let cells_slice = core::ptr::slice_from_raw_parts_mut(
            unsafe { cells.add(start as usize) },
            (end - start) as usize,
        );

        // If we're not respecting protected attributes, we can use a fast-path
        // to fill the entire line.
        if !protected {
            self.active_mut().clear_cells(page, row, cells_slice);
            return;
        }

        self.active_mut()
            .clear_unprotected_cells(page, row, cells_slice);
    }

    /// Clear the active screen's Kitty graphics state for a terminal clear
    /// (the reference's `kitty_images.clearScreen(...)`; upstream `f8b40a023`
    /// replaced the old `delete(.., .{ .all = true })` here so ED2 preserves
    /// scrollback references while reclaiming every unreferenced image).
    ///
    /// PORT NOTE: the reference writes this inline as
    /// `self.screens.active.kitty_images.clearScreen(self.io(), self.screens.active.alloc, self)`,
    /// which aliases the storage field and the whole terminal at once. Rust
    /// cannot, so — exactly as `kitty::graphics_exec::delete` and
    /// `set_kitty_graphics_size_limit` already do — the storage is MOVED out of
    /// the screen, `clear_screen` runs against the borrowed terminal, and the
    /// storage is moved back. `ImageStorage::clear_screen` only reaches the
    /// active screen itself through `t` (never its `kitty_images` slot), so the
    /// temporarily-defaulted slot is never observed.
    fn clear_screen_active_kitty_images(&mut self) {
        // SAFETY: `self.screens.active` is a stable heap pointer owned by
        // `self.screens`; we hold `&mut self`, so no other alias exists.
        let alloc = unsafe { (*self.screens.active).alloc };
        let mut storage = core::mem::take(unsafe { &mut (*self.screens.active).kitty_images });
        storage.clear_screen(alloc, self);
        unsafe { (*self.screens.active).kitty_images = storage };
    }

    /// Erase the display.
    pub fn erase_display(&mut self, mode: csi::EraseDisplay, protected_req: bool) {
        // We respect protected attributes if explicitly requested (probably
        // a DECSEL sequence) or if our last protected mode was ISO even if its
        // not currently set.
        let protected = unsafe { (*self.screens.active).protected_mode }
            == ansi::ProtectedMode::Iso
            || protected_req;

        match mode {
            csi::EraseDisplay::ScrollComplete => {
                // self.screens.active.scrollClear() catch |err| { ... };
                if let Err(err) = self.active_mut().scroll_clear() {
                    log::warn!(target: LOG_SCOPE, "scroll clear failed, doing a normal clear err={err:?}");
                    self.erase_display(csi::EraseDisplay::Complete, protected_req);
                    return;
                }

                // Unsets pending wrap state
                unsafe { (*self.screens.active).cursor.pending_wrap = false };

                // if (comptime build_options.kitty_graphics) {
                //     // Clear only placements still visible after moving the active
                //     // area into scrollback.
                //     self.screens.active.kitty_images.clearScreen(
                //         self.io(),
                //         self.screens.active.alloc,
                //         self,
                //     );
                // }
                if crate::options::KITTY_GRAPHICS {
                    // Clear only placements still visible after moving the active
                    // area into scrollback.
                    self.clear_screen_active_kitty_images();
                }
            }

            csi::EraseDisplay::Complete => {
                // If we're on the primary screen and our last non-empty row is
                // a prompt, then we do a scroll_complete instead. This is a
                // heuristic to get the generally desirable behavior that ^L
                // at a prompt scrolls the screen contents prior to clearing.
                // Most shells send `ESC [ H ESC [ 2 J` so we can't just check
                // our current cursor position. See #905
                if self.screens.active_key == crate::screen_set::Key::Primary {
                    'at_prompt: {
                        // Go from the bottom of the active up and see if we're
                        // at a prompt.
                        let active_br =
                            match self.active_mut().pages.get_bottom_right(point::Tag::Active) {
                                Some(p) => p,
                                None => break 'at_prompt,
                            };
                        let tl = self.active_mut().pages.get_top_left(point::Tag::Active);
                        let mut it = active_br
                            .row_iterator(crate::pagelist::iter::Direction::LeftUp, Some(tl));
                        // while (it.next()) |p| { switch (...) { .prompt,
                        // .prompt_continuation => break, .none => break :at_prompt } }
                        // else break :at_prompt;
                        // PORT NOTE: Zig's `while (cond) {...} else {...}` runs the
                        // `else` only on normal loop exhaustion. We mirror it: a
                        // `found` flag set true only on the prompt/continuation
                        // `break`; any other exit (`.none` or exhaustion) leaves it
                        // false and falls through to `break 'at_prompt`.
                        let mut found = false;
                        // PORT NOTE: every match arm `break`s, so this `while let`
                        // only ever runs one iteration (the reference `while` is
                        // identical: the bottom-most active row decides). We keep
                        // the loop form to mirror the reference `while...else`
                        // exhaustion semantics and allow the resulting lint.
                        #[allow(clippy::never_loop)]
                        while let Some(p) = it.next() {
                            let row = p.row_and_cell().row;
                            match unsafe { (*row).semantic_prompt() } {
                                // If we're at a prompt or input area, then we are at a prompt.
                                pagepkg::SemanticPrompt::Prompt
                                | pagepkg::SemanticPrompt::PromptContinuation => {
                                    found = true;
                                    break;
                                }

                                // If we have command output, then we're most certainly not
                                // at a prompt.
                                pagepkg::SemanticPrompt::None => break,
                            }
                        }
                        if !found {
                            break 'at_prompt;
                        }

                        // self.screens.active.scrollClear() catch {};
                        // If we fail, we just fall back to doing a normal clear
                        // so we don't worry about the error.
                        let _ = self.active_mut().scroll_clear();
                    }
                }

                // All active area
                self.active_mut().clear_rows(
                    point::Point::Active(point::Coordinate::default()),
                    None,
                    protected,
                );

                // Unsets pending wrap state
                unsafe { (*self.screens.active).cursor.pending_wrap = false };

                // if (comptime build_options.kitty_graphics) {
                //     // ED2 clears visible placements but preserves graphics that
                //     // are wholly in scrollback.
                //     self.screens.active.kitty_images.clearScreen(
                //         self.io(),
                //         self.screens.active.alloc,
                //         self,
                //     );
                // }
                if crate::options::KITTY_GRAPHICS {
                    // ED2 clears visible placements but preserves graphics that
                    // are wholly in scrollback.
                    self.clear_screen_active_kitty_images();
                }

                // Cleared screen dirty bit
                self.flags.dirty.clear = true;
            }

            csi::EraseDisplay::Below => {
                // All lines to the right (including the cursor)
                self.erase_line(csi::EraseLine::Right, protected_req);

                // All lines below
                if unsafe { (*self.screens.active).cursor.y } + 1 < self.rows {
                    let y = unsafe { (*self.screens.active).cursor.y } + 1;
                    self.active_mut().clear_rows(
                        point::Point::Active(point::Coordinate { x: 0, y: y as u32 }),
                        None,
                        protected,
                    );
                }

                // Unsets pending wrap state. Should be done by eraseLine.
                debug_assert!(!unsafe { (*self.screens.active).cursor.pending_wrap });
            }

            csi::EraseDisplay::Above => {
                // Erase to the left (including the cursor)
                self.erase_line(csi::EraseLine::Left, protected_req);

                // All lines above
                if unsafe { (*self.screens.active).cursor.y } > 0 {
                    let y = unsafe { (*self.screens.active).cursor.y } - 1;
                    self.active_mut().clear_rows(
                        point::Point::Active(point::Coordinate::default()),
                        Some(point::Point::Active(point::Coordinate {
                            x: 0,
                            y: y as u32,
                        })),
                        protected,
                    );
                }

                // Unsets pending wrap state
                debug_assert!(!unsafe { (*self.screens.active).cursor.pending_wrap });
            }

            csi::EraseDisplay::Scrollback => self.active_mut().erase_history(None),
        }
    }

    /// Resets all margins and fills the whole screen with the character 'E'
    ///
    /// Sets the cursor to the top left corner.
    pub fn decaln(&mut self) -> Result<(), OutOfMemory> {
        // Clear our stylistic attributes. This is the only thing that can
        // fail so we do it first so we can undo it.
        let old_style = unsafe { (*self.screens.active).cursor.style };
        unsafe {
            (*self.screens.active).cursor.style = style::Style {
                bg_color: (*self.screens.active).cursor.style.bg_color,
                fg_color: (*self.screens.active).cursor.style.fg_color,
                ..style::Style::default()
            }
        };
        // errdefer self.screens.active.cursor.style = old_style;
        // PORT NOTE: the reference `errdefer` restores the saved style if
        // `manualStyleUpdate` fails. In Rust we restore explicitly on the error
        // path before returning.
        if self.active_mut().manual_style_update().is_err() {
            unsafe { (*self.screens.active).cursor.style = old_style };
            return Err(OutOfMemory);
        }

        // Reset margins, also sets cursor to top-left
        self.scrolling_region = ScrollingRegion {
            top: 0,
            bottom: self.rows - 1,
            left: 0,
            right: self.cols - 1,
        };

        // Origin mode is disabled
        self.modes.set(modespkg::Mode::Origin, false);

        // Move our cursor to the top-left
        self.set_cursor_pos(1, 1);

        // Use clearRows instead of eraseDisplay because we must NOT respect
        // protected attributes here.
        self.active_mut().clear_rows(
            point::Point::Active(point::Coordinate::default()),
            None,
            false,
        );

        // Fill with Es by moving the cursor but reset it after.
        loop {
            // const page = &self.screens.active.cursor.page_pin.node.data;
            let page: *mut Page = unsafe {
                crate::pagelist::Node::page((*(*self.screens.active).cursor.page_pin).node)
            };
            // const row = self.screens.active.cursor.page_row;
            let row: *mut Row = unsafe { (*self.screens.active).cursor.page_row };
            // const cells_multi: [*]Cell = row.cells.ptr(page.memory);
            // const cells = cells_multi[0..page.size.cols];
            let cells_offset = unsafe { (*row).cells() };
            let cells_multi: *mut pagepkg::Cell = unsafe { cells_offset.ptr((*page).memory_ptr()) };
            let cols = unsafe { (*page).size.cols } as usize;

            // @memset(cells, .{ ... });
            let style_id = unsafe { (*self.screens.active).cursor.style_id };
            for i in 0..cols {
                let cell: *mut pagepkg::Cell = unsafe { cells_multi.add(i) };
                let mut new_cell = pagepkg::Cell::zeroed();
                new_cell.set_content_tag(pagepkg::ContentTag::Codepoint);
                new_cell.set_content_codepoint('E' as u21);
                new_cell.set_style_id(style_id);
                // DECALN does not respect protected state. Verified with xterm.
                new_cell.set_protected(false);
                unsafe { *cell = new_cell };
            }

            // If we have a ref-counted style, increase
            if style_id != style::DEFAULT_ID {
                unsafe {
                    let base = (*page).memory_ptr();
                    (*page).styles.use_multiple(base, style_id, cols as _);
                }
                unsafe { (*row).set_styled(true) };
            }

            // We messed with the page so assert its integrity here.
            unsafe { (*page).assert_integrity() };

            self.active_mut().cursor_mark_dirty();
            if unsafe { (*self.screens.active).cursor.y } == self.rows - 1 {
                break;
            }
            self.active_mut().cursor_down(1);
        }

        // Reset the cursor to the top-left
        self.set_cursor_pos(1, 1);

        Ok(())
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
        // return kitty.graphics.execute(alloc, self, cmd);
        kitty::graphics::execute(alloc, self, cmd)
    }

    /// Execute a Glyph Protocol APC command against this terminal's per-session
    /// glossary. The returned response, if any, should be sent back to the pty as
    /// a complete APC sequence via `Response.formatWire`.
    // pub fn glyphProtocol(self: *Terminal, alloc: Allocator,
    //     req: *const glyph.Request) ?glyph.Response {
    //     const resp = glyph.execute(alloc, &self.glyph_glossary, req);
    //     switch (req.*) {
    //         .register, .clear => self.flags.dirty.glyph_glossary = true,
    //         .support, .query => {},
    //     }
    //     return resp;
    // }
    // PORT NOTE (parity tail S7-3): `glyph::execute` takes no allocator: the
    // glossary map grows through the allocator it was bound to at `init`, and
    // the entries' decoded outlines are `Vec`s on the Rust heap (N272). The
    // reference's per-call `alloc` is that bound allocator, checked here in
    // debug builds (callers pass `terminal.gpa()` or termio's allocator).
    pub fn glyph_protocol(
        &mut self,
        alloc: Allocator,
        req: &glyph::Request,
    ) -> Option<glyph::Response> {
        self.glyph_glossary.allocator().debug_assert_same(alloc);
        let resp = glyph::execute(&mut self.glyph_glossary, req);
        match req {
            glyph::Request::Register(_) | glyph::Request::Clear(_) => {
                self.flags.dirty.glyph_glossary = true;
            }
            glyph::Request::Support | glyph::Request::Query(_) => {}
        }
        resp
    }

    /// Set the storage size limit for Kitty graphics across all screens.
    // PORT NOTE (`4371871bc`): `setLimit` no longer allocates, so this is
    // infallible (`!void` → `void`).
    pub fn set_kitty_graphics_size_limit(&mut self, alloc: Allocator, limit: usize) {
        // if (comptime !build_options.kitty_graphics) return;
        if !crate::options::KITTY_GRAPHICS {
            return;
        }
        // var it = self.screens.all.iterator();
        // while (it.next()) |entry| {
        //     const screen: *Screen = entry.value.*;
        //     screen.kitty_images.setLimit(self.io(), alloc, screen, limit);
        // }
        // PORT NOTE: `screen.kitty_images.setLimit(io, alloc, screen, limit)`
        // borrows `screen.kitty_images` mutably while also handing `screen`
        // itself to `set_limit` (now as a raw `*mut Screen`, per the storage's
        // `s: *mut Screen` PORT NOTE). As in `Screen::deinit`, we MOVE
        // `kitty_images` out of the screen (leaving a default in its slot), run
        // `set_limit` against the raw screen pointer, then move the updated
        // storage back. The screen pointers in `self.screens.all` are stable
        // heap allocations.
        for slot in self.screens.all.iter() {
            if let Some(screen) = *slot {
                // SAFETY: `screen` is a stable, initialized heap allocation owned
                // by the screen set; we hold `&mut self` so no other alias exists.
                let mut images = core::mem::take(unsafe { &mut (*screen).kitty_images });
                images.set_limit(alloc, screen, limit);
                unsafe { (*screen).kitty_images = images };
            }
        }
    }

    /// Set the allowed medium types for Kitty graphics image loading
    /// across all screens.
    pub fn set_kitty_graphics_loading_limits(&mut self, limits: kitty::graphics_image::Limits) {
        // if (comptime !build_options.kitty_graphics) return;
        if !crate::options::KITTY_GRAPHICS {
            return;
        }
        // var it = self.screens.all.iterator();
        // while (it.next()) |entry| {
        //     const screen: *Screen = entry.value.*;
        //     screen.kitty_images.image_limits = limits;
        // }
        for slot in self.screens.all.iter() {
            if let Some(screen) = *slot {
                // SAFETY: stable initialized heap screen; `&mut self` excludes
                // other aliases.
                let screen: &mut Screen = unsafe { &mut *screen };
                screen.kitty_images.image_limits = limits.clone();
            }
        }
    }

    /// Set a style attribute.
    pub fn set_attribute(&mut self, attr: sgr::Attribute) -> Result<(), OutOfMemory> {
        // try self.screens.active.setAttribute(attr);
        // PORT NOTE: `Screen::set_attribute` returns the reference
        // `PageList.IncreaseCapacityError`; the reference `setAttribute` returns
        // `!void` inferred from that single call, and the only failure mode that
        // can propagate here is allocation failure, so it maps to `OutOfMemory`.
        self.active_mut()
            .set_attribute(attr)
            .map_err(|_| OutOfMemory)
    }

    /// Print the active attributes as a string. This is used to respond to DECRQSS
    /// requests.
    ///
    /// Boolean attributes are printed first, followed by foreground color, then
    /// background color. Each attribute is separated by a semicolon.
    pub fn print_attributes<'a>(&mut self, buf: &'a mut [u8]) -> Result<&'a [u8], OutOfMemory> {
        // var stream = std.io.fixedBufferStream(buf);
        // const writer = stream.writer();
        // PORT NOTE: the reference uses `std.io.fixedBufferStream`, whose writer
        // returns `error.NoSpaceLeft` on overflow. We mirror that exactly: `pos`
        // tracks the bytes written and any write past the buffer end is the
        // `NoSpaceLeft` error, which is the only error this `![]const u8` can
        // produce and so maps to `OutOfMemory`.
        let mut pos: usize = 0;
        // `writeByte`: write a single byte, NoSpaceLeft if it doesn't fit.
        macro_rules! write_byte {
            ($b:expr) => {{
                if pos >= buf.len() {
                    return Err(OutOfMemory);
                }
                buf[pos] = $b;
                pos += 1;
            }};
        }
        // `writer.print(fmt, args)`: format into a scratch buffer then copy. The
        // reference formats directly into the stream; copying the formatted bytes
        // is byte-for-byte equivalent and keeps the same NoSpaceLeft semantics.
        macro_rules! write_print {
            ($($arg:tt)*) => {{
                let s = format!($($arg)*);
                if pos + s.len() > buf.len() {
                    return Err(OutOfMemory);
                }
                buf[pos..pos + s.len()].copy_from_slice(s.as_bytes());
                pos += s.len();
            }};
        }

        // The SGR response always starts with a 0. See https://vt100.net/docs/vt510-rm/DECRPSS
        // try writer.writeByte('0');
        write_byte!(b'0');

        // const pen = self.screens.active.cursor.style;
        let pen = self.active_mut().cursor.style;
        // var attrs: [9]u8 = @splat(0);
        let mut attrs: [u8; 9] = [0; 9];
        // var i: usize = 0;
        let mut i: usize = 0;

        if pen.flags.bold {
            attrs[i] = 1;
            i += 1;
        }

        if pen.flags.faint {
            attrs[i] = 2;
            i += 1;
        }

        if pen.flags.italic {
            attrs[i] = 3;
            i += 1;
        }

        if pen.flags.underline != sgr::Underline::None {
            attrs[i] = 4;
            i += 1;
        }

        if pen.flags.overline {
            attrs[i] = 53;
            i += 1;
        }

        if pen.flags.blink {
            attrs[i] = 5;
            i += 1;
        }

        if pen.flags.inverse {
            attrs[i] = 7;
            i += 1;
        }

        if pen.flags.invisible {
            attrs[i] = 8;
            i += 1;
        }

        if pen.flags.strikethrough {
            attrs[i] = 9;
            i += 1;
        }

        // for (attrs[0..i]) |attr| {
        //     // Preserve underline styles. Kind of a hack to special case 4
        //     // here but its easier than changing how we do all attributes.
        //     if (attr == 4 and pen.flags.underline != .single) {
        //         try writer.print(";4:{}", .{@intFromEnum(pen.flags.underline)});
        //         continue;
        //     }
        //
        //     try writer.print(";{}", .{attr});
        // }
        //
        // PORT NOTE (oracle cb2fef390, `P3-W1 T6`): the buffer stays
        // `&mut [u8]` in, `&[u8]` out — nothing here formats through `String`.
        for &attr in &attrs[0..i] {
            // Preserve underline styles. Kind of a hack to special case 4
            // here but its easier than changing how we do all attributes.
            if attr == 4 && pen.flags.underline != sgr::Underline::Single {
                write_print!(";4:{}", pen.flags.underline as u8);
                continue;
            }

            write_print!(";{}", attr);
        }

        // switch (pen.fg_color) {
        match pen.fg_color.tag() {
            style::ColorTag::None => {}
            style::ColorTag::Palette => {
                let idx = pen.fg_color.palette();
                if idx >= 16 {
                    write_print!(";38:5:{}", idx)
                } else if idx >= 8 {
                    write_print!(";9{}", idx - 8)
                } else {
                    write_print!(";3{}", idx)
                }
            }
            style::ColorTag::Rgb => {
                let rgb = pen.fg_color.rgb();
                write_print!(";38:2::{}:{}:{}", rgb.r, rgb.g, rgb.b)
            }
        }

        // switch (pen.bg_color) {
        match pen.bg_color.tag() {
            style::ColorTag::None => {}
            style::ColorTag::Palette => {
                let idx = pen.bg_color.palette();
                if idx >= 16 {
                    write_print!(";48:5:{}", idx)
                } else if idx >= 8 {
                    write_print!(";10{}", idx - 8)
                } else {
                    write_print!(";4{}", idx)
                }
            }
            style::ColorTag::Rgb => {
                let rgb = pen.bg_color.rgb();
                write_print!(";48:2::{}:{}:{}", rgb.r, rgb.g, rgb.b)
            }
        }

        // return stream.getWritten();
        Ok(&buf[..pos])
    }

    /// DECCOLM changes the terminal width between 80 and 132 columns. This
    /// function call will do NOTHING unless `setDeccolmSupported` has been
    /// called with "true".
    ///
    /// This breaks the expectation around modern terminals that they resize
    /// with the window. This will fix the grid at either 80 or 132 columns.
    /// The rows will continue to be variable.
    pub fn deccolm(&mut self, alloc: Allocator, mode: DeccolmMode) -> Result<(), OutOfMemory> {
        // If DEC mode 40 isn't enabled, then this is ignored. We also make
        // sure that we don't have deccolm set because we want to fully ignore
        // set mode.
        // if (!self.modes.get(.enable_mode_3)) {
        //     self.modes.set(.@"132_column", false);
        //     return;
        // }
        if !self.modes.get(modespkg::Mode::EnableMode3) {
            self.modes.set(modespkg::Mode::Column132, false);
            return Ok(());
        }

        // Enable it
        // self.modes.set(.@"132_column", mode == .@"132_cols");
        self.modes
            .set(modespkg::Mode::Column132, mode == DeccolmMode::Cols132);

        // Resize to the requested size
        // try self.resize(alloc, .{
        //     .cols = switch (mode) {
        //         .@"132_cols" => 132,
        //         .@"80_cols" => 80,
        //     },
        //     .rows = self.rows,
        // });
        self.resize(
            alloc,
            Resize {
                cols: match mode {
                    DeccolmMode::Cols132 => 132,
                    DeccolmMode::Cols80 => 80,
                },
                rows: self.rows,
                cell_size_px: None,
            },
        )
        .map_err(|e| match e {
            ResizeError::OutOfMemory => OutOfMemory,
            // PORT NOTE: `deccolm` passes a constant non-zero column count and
            // the terminal's own row count (always >= 1), so the reference's
            // inferred error set can never carry `InvalidValue` here. Rust's
            // error type is explicit, so the impossible arm is spelled out.
            ResizeError::InvalidValue => {
                unreachable!("deccolm resize dimensions are always non-zero")
            }
        })?;

        // Erase our display and move our cursor.
        // self.eraseDisplay(.complete, false);
        // self.setCursorPos(1, 1);
        self.erase_display(csi::EraseDisplay::Complete, false);
        self.set_cursor_pos(1, 1);

        Ok(())
    }

    /// Resize the underlying terminal.
    ///
    /// This has follow-on impacts:
    ///
    ///   - If the column count changes, tabstops are reset.
    ///   - The scroll region is always reset
    ///   - Synchronized output mode is reset for every successful resize.
    ///
    /// This handles errors gracefully and recovers the terminal back to
    /// a clean usable state.
    ///
    /// The only error handling edge case is in the highly exceptional scenario
    /// where the primary screen can be resized but the alternate screen cannot.
    /// In this scenario, we attempt to clear the alt screen at the desired
    /// new size. If that fails, we unconditionally deallocate the alt screen
    /// and move to primary screen. This can break terminal programs but it
    /// requires a really particular scenario where memory exists for one
    /// but not the other and we do our best.
    pub fn resize(
        &mut self,
        alloc: Allocator,
        opts: Resize,
    ) -> core::result::Result<(), ResizeError> {
        // Screen and scrolling-region invariants require non-zero dimensions.
        // Validate before changing any terminal state.
        // if (opts.cols == 0 or opts.rows == 0) return error.InvalidValue;
        if opts.cols == 0 || opts.rows == 0 {
            return Err(ResizeError::InvalidValue);
        }

        let cols = opts.cols;
        let rows = opts.rows;

        // Pixel geometry and synchronized output are updated on every valid
        // resize attempt, including one that doesn't change the grid dimensions.
        // Save their old values so later allocation failures can roll them back.
        // errdefer {
        //     self.width_px = old_width_px;
        //     self.height_px = old_height_px;
        //     self.modes.set(.synchronized_output, old_synchronized_output);
        // }
        //
        // PORT NOTE: Rust has no `errdefer`; the block is spelled out as
        // `resize_rollback_geometry` and invoked at every fallible exit below.
        let old_width_px = self.width_px;
        let old_height_px = self.height_px;
        let old_synchronized_output = self.modes.get(modespkg::Mode::SynchronizedOutput);

        // If our pixel geometry was set, then we set it even if our rows/cols
        // didn't change.
        // PORT NOTE: `std.math.mul(...) catch maxInt(u32)` is saturate-on-overflow.
        if let Some(cell_size) = opts.cell_size_px {
            self.width_px = u32::from(cols).saturating_mul(cell_size.width);
            self.height_px = u32::from(rows).saturating_mul(cell_size.height);
        }

        // self.modes.set(.synchronized_output, false);
        self.modes.set(modespkg::Mode::SynchronizedOutput, false);

        // If our cols/rows didn't change, skip grid work but still apply pixels.
        // if (self.cols == opts.cols and self.rows == opts.rows) return;
        if self.cols == cols && self.rows == rows {
            return Ok(());
        }

        // Build replacement tabstops without touching the current table. Keep
        // ownership here until every fallible resize operation has succeeded.
        // var new_tabstops: ?Tabstops = null;
        // errdefer if (new_tabstops) |*v| v.deinit(alloc);
        let mut new_tabstops: Option<Tabstops> = None;
        if self.cols != cols {
            // try tw.check(.tabstops);
            #[cfg(test)]
            if let Err(e) = resize_tw_check(ResizeFp::Tabstops) {
                self.resize_rollback_geometry(old_width_px, old_height_px, old_synchronized_output);
                return Err(e.into());
            }
            // PORT NOTE: `Tabstops::init`'s allocation-failure type is its own
            // `OutOfMemory`; both map to the reference `Allocator.Error`.
            match Tabstops::init(alloc, cols as usize, TABSTOP_INTERVAL as usize) {
                Ok(t) => new_tabstops = Some(t),
                Err(_e) => {
                    self.resize_rollback_geometry(
                        old_width_px,
                        old_height_px,
                        old_synchronized_output,
                    );
                    return Err(ResizeError::OutOfMemory);
                }
            }
        }

        // Resize primary screen, which supports reflow. We do this first
        // because the cleanup situation is a lot better if this succeeds
        // and alt fails than the reverse.
        // try tw.check(.primary_screen);
        // const primary = self.screens.get(.primary).?;
        // try primary.resize(.{
        //     .cols = opts.cols,
        //     .rows = opts.rows,
        //     .reflow = self.modes.get(.wraparound),
        //     .prompt_redraw = self.flags.shell_redraws_prompt,
        // });
        let reflow = self.modes.get(modespkg::Mode::Wraparound);
        let prompt_redraw = self.flags.shell_redraws_prompt;
        let primary: *mut Screen = self.screens.get(crate::screen_set::Key::Primary).unwrap();

        #[cfg(test)]
        if let Err(e) = resize_tw_check(ResizeFp::PrimaryScreen) {
            // errdefer if (new_tabstops) |*v| v.deinit(alloc);
            if let Some(v) = new_tabstops.take() {
                v.deinit(alloc);
            }
            self.resize_rollback_geometry(old_width_px, old_height_px, old_synchronized_output);
            return Err(e.into());
        }
        // SAFETY: `primary` is a stable heap pointer owned by `self.screens`;
        // `resize` takes `&mut Screen` and does not mutate `self.screens`.
        if let Err(e) = unsafe {
            (*primary).resize(crate::screen::Resize {
                cols,
                rows,
                reflow,
                prompt_redraw,
            })
        } {
            if let Some(v) = new_tabstops.take() {
                v.deinit(alloc);
            }
            self.resize_rollback_geometry(old_width_px, old_height_px, old_synchronized_output);
            return Err(e.into());
        }

        // Alternate screen, if it exists, doesn't reflow. The primary resize
        // above can't be losslessly undone, so if the alternate resize fails we
        // replace it with an empty screen at the requested size. If that
        // also fails, we fall back to the primary screen.
        if let Some(alt) = self.screens.get(crate::screen_set::Key::Alternate) {
            'alt: {
                let err: ResizeError = 'resize: {
                    #[cfg(test)]
                    if let Err(e) = resize_tw_check(ResizeFp::AlternateScreen) {
                        break 'resize e.into();
                    }
                    // SAFETY: `alt` is a stable heap pointer owned by `self.screens`.
                    match unsafe {
                        (*alt).resize(crate::screen::Resize {
                            cols,
                            rows,
                            reflow: false,
                            // PORT NOTE: the reference omits `prompt_redraw`
                            // here, so it takes the `Screen.Resize` struct field
                            // default, which is `.false` (`Screen.zig`
                            // `prompt_redraw: ... = .false`).
                            //
                            // PORT FINDING (P3-W4 T2): this previously read
                            // `Redraw::True`, citing a `.true` default the
                            // reference has never had. The alternate screen
                            // would therefore clear its cursor row on resize
                            // whenever the cursor's semantic content was not
                            // `.output`. Guarded by
                            // `terminal_resize_does_not_prompt_redraw_the_alternate_screen`.
                            prompt_redraw: osc::semantic_prompt::Redraw::False,
                        })
                    } {
                        Err(e) => break 'resize e.into(),
                        // Resize succeeded.
                        Ok(()) => break 'alt,
                    }
                };

                log::warn!(target: LOG_SCOPE, "alternate screen resize failed, replacing it err={err:?}");

                // If the alternate screen isn't active, then we just free it
                // and move on. It'll be reallocated when it gets reinitialized
                // lazily. In this case, we just lose the prior data if the
                // terminal program expected it to be saved.
                if self.screens.active_key != crate::screen_set::Key::Alternate {
                    self.screens
                        .remove(alloc, crate::screen_set::Key::Alternate);
                    break 'alt;
                }

                // The alt screen is active, so we temporarily switch to primary
                // so we can safely remove the alt and recreate it blank. This
                // loses the data but hopefully keeps us on the alt screen.
                // SAFETY: `alt` is still live here (nothing has removed it yet).
                let charset = unsafe { (*alt).charset.clone() };
                self.screens.switch_to(crate::screen_set::Key::Primary);
                self.screens
                    .remove(alloc, crate::screen_set::Key::Alternate);

                // Replace the alt screen with an empty version. If this fails
                // we just go back to the primary screen. Not great, but best
                // we can do.
                // tw.check(.alternate_screen_init) catch break :alt;
                #[cfg(test)]
                if resize_tw_check(ResizeFp::AlternateScreenInit).is_err() {
                    break 'alt;
                }
                // PORT NOTE (oracle b988efcfe): upstream's 0.16 regression fix
                // changed this call's first argument from `alt.io` to
                // `self.io()`. rustty's `ScreenSet::get_init` has no `io`
                // parameter at all, so the pre-fix form is simply never
                // reproduced and there is nothing to port.
                let replacement = match self.screens.get_init(
                    alloc,
                    crate::screen_set::Key::Alternate,
                    crate::screen::Options {
                        cols,
                        rows,
                        // .max_scrollback = 0
                        max_scrollback_bytes: Some(0),
                        max_scrollback_lines: None,
                        // Inherit our Kitty image settings from the primary
                        // screen (see `switch_screen`'s PORT NOTE: kitty
                        // graphics IS enabled in rustty, so the reference's
                        // `build_options.kitty_graphics` arm is the live one).
                        kitty_image_storage_limit: unsafe { (*primary).kitty_images.total_limit },
                        kitty_image_loading_limits: unsafe {
                            (*primary).kitty_images.image_limits.clone()
                        },
                    },
                ) {
                    Ok(r) => r,
                    Err(init_err) => {
                        log::warn!(
                            target: LOG_SCOPE,
                            "alternate screen replacement failed, falling back to primary err={init_err:?}"
                        );
                        break 'alt;
                    }
                };

                // SAFETY: freshly initialized screen owned by `self.screens`.
                unsafe { (*replacement).charset = charset };
                self.screens.switch_to(crate::screen_set::Key::Alternate);
            }
        }

        // errdefer comptime unreachable;
        //
        // PORT NOTE: "No more failures are allowed after this point because the
        // screens have committed their new sizes and the remaining Terminal
        // state must follow." Zig's `errdefer comptime unreachable` is a
        // COMPILE-TIME assertion that no fallible operation follows; Rust has no
        // equivalent, so the property is held by construction — nothing below
        // returns `Err` and the function's only remaining exit is `Ok(())`.
        // The marker below is that contract written down at the commit point,
        // as a real (non-vacuous) invariant: reaching here means every screen
        // has already committed to the new size. A bare `debug_assert!(true)`
        // would be a no-op that clippy rightly rejects.
        debug_assert!(
            unsafe { (*primary).pages.cols == cols && (*primary).pages.rows == rows },
            "resize commit point reached without the primary screen at the new size"
        );

        // All fallible work is complete. Replace the old tabstop table only now.
        if let Some(v) = new_tabstops.take() {
            // PORT NOTE: `Tabstops::deinit` consumes `self` by value (mirrors
            // the reference `self.tabstops.deinit(alloc)`); we take the field
            // out via `core::mem::take` (leaving an empty default) to call it,
            // then overwrite with the freshly initialized tabstops.
            core::mem::take(&mut self.tabstops).deinit(alloc);
            self.tabstops = v;
        }

        // Whenever we resize we just mark it as a screen clear
        // self.flags.dirty.clear = true;
        self.flags.dirty.clear = true;

        // Set our size
        // self.cols = cols;
        // self.rows = rows;
        self.cols = cols;
        self.rows = rows;

        // Reset the scrolling region
        // self.scrolling_region = .{
        //     .top = 0,
        //     .bottom = rows - 1,
        //     .left = 0,
        //     .right = cols - 1,
        // };
        self.scrolling_region = ScrollingRegion {
            top: 0,
            bottom: rows - 1,
            left: 0,
            right: cols - 1,
        };

        Ok(())
    }

    /// The pixel-geometry / synchronized-output `errdefer` of `resize`,
    /// spelled out (Rust has no `errdefer`).
    fn resize_rollback_geometry(
        &mut self,
        old_width_px: u32,
        old_height_px: u32,
        old_synchronized_output: bool,
    ) {
        self.width_px = old_width_px;
        self.height_px = old_height_px;
        // NOTE: when `old_synchronized_output` is false this re-clears the mode
        // and therefore bumps `modes::sync_output_clear_count`. That counter
        // measures the SUCCESS path (see its PORT NOTE); a rollback restoring
        // `false` is indistinguishable from a clear at the `ModeState::set`
        // seam, and no assertion counts an error path.
        self.modes
            .set(modespkg::Mode::SynchronizedOutput, old_synchronized_output);
    }

    /// Set the pwd for the terminal.
    // PORT NOTE (aliasing): `pwd` MAY alias `self.pwd` — a caller can hand
    // `get_pwd()`'s return straight back through the vt-c FFI seam. Every
    // mutation below therefore goes through a raw `*mut Vec<u8>` and the copy
    // uses `core::ptr::copy` (memmove, = Zig `std.mem.copyForwards`). NEVER use
    // `extend_from_slice`/`copy_from_slice` here: both are `memcpy` and are UB
    // on overlapping ranges. That is upstream a14a619ce.
    //
    // PORT FINDING (WS8 T7, proven under Miri): a `&[u8]` parameter is a
    // STRONGLY PROTECTED `SharedReadOnly` argument for the whole call, so
    // writing into the bytes it points at is Stacked-Borrows UB even with a
    // memmove — the protector, not the memcpy, is the blocker (memory:
    // `rust-mut-args-strongly-protected`). The alias-tolerant entry point is
    // therefore `set_pwd_raw`, which takes the source as a raw pointer and
    // forms no reference to it. This `&[u8]` wrapper stays for the many
    // non-aliasing internal callers; the vt-c FFI seam — the one place a
    // caller can hand `get_pwd()`'s own bytes back — calls `set_pwd_raw`.
    pub fn set_pwd(&mut self, pwd: &[u8]) -> Result<(), OutOfMemory> {
        // SAFETY: `pwd` is a live slice readable for `pwd.len()` bytes.
        unsafe { self.set_pwd_raw(pwd.as_ptr(), pwd.len()) }
    }

    /// `set_pwd` over a raw source, tolerating a source that ALIASES
    /// `self.pwd` (see the PORT FINDING on `set_pwd`).
    ///
    /// # Safety
    ///
    /// `src` must be readable for `src_len` bytes. It MAY overlap `self.pwd`'s
    /// buffer; it must not be derived from a live Rust reference that outlives
    /// this call.
    pub unsafe fn set_pwd_raw(
        &mut self,
        src: *const u8,
        src_len: usize,
    ) -> Result<(), OutOfMemory> {
        let pwd_len = src_len;
        // if (pwd.len == 0) { self.pwd.clearRetainingCapacity(); return; }
        if pwd_len == 0 {
            self.pwd.clear_retaining_capacity();
            return Ok(());
        }

        // const capacity = std.math.add(usize, pwd.len, 1) catch
        //     return error.OutOfMemory;
        let capacity = match pwd_len.checked_add(1) {
            Some(c) => c,
            None => return Err(OutOfMemory),
        };

        // try self.pwd.ensureTotalCapacity(self.gpa(), capacity);
        //
        // PORT NOTE: `ensureTotalCapacity` is GROW-ONLY and amortized
        // (`growCapacity`), and fallible: it grows through `self.gpa()`, the
        // active screen's allocator, looked up at call time (parity tail S7-3).
        // Growing can reallocate, which would invalidate an aliased `pwd` —
        // that cannot happen in the aliasing case, because an aliased source is
        // a subslice of `self.pwd`, hence `capacity <= self.pwd.capacity()`.
        let gpa = self.gpa();
        let v: *mut crate::alloc::ArrayListUnmanaged<u8> = core::ptr::addr_of_mut!(self.pwd);
        // SAFETY: `v` is the live `self.pwd`; `src` is readable for `src_len`
        // bytes and may overlap `*v`'s buffer, which `core::ptr::copy`
        // (memmove) tolerates. `set_len` exposes `capacity` bytes that are all
        // written before any read, and `u8` has no drop glue.
        unsafe {
            (*v).ensure_total_capacity(gpa, capacity)
                .map_err(|_| OutOfMemory)?;
            let dst: *mut u8 = (*v).as_mut_ptr();
            // self.pwd.items.len = capacity;
            (*v).set_len(capacity);
            // std.mem.copyForwards(u8, self.pwd.items[0..pwd.len], pwd);
            core::ptr::copy(src, dst, src_len);
            // self.pwd.items[pwd.len] = 0;
            *dst.add(src_len) = 0;
        }
        Ok(())
    }

    /// Returns the pwd for the terminal, if any. The memory is owned by the
    /// Terminal and is not copied. It is safe until a reset or setPwd.
    // pub fn getPwd(self: *const Terminal) ?[:0]const u8
    // PORT NOTE: Zig `[:0]const u8` -> `Option<&[u8]>` (NUL sentinel stripped).
    pub fn get_pwd(&self) -> Option<&[u8]> {
        // if (self.pwd.items.len == 0) return null;
        if self.pwd.is_empty() {
            return None;
        }
        // return self.pwd.items[0 .. self.pwd.items.len - 1 :0];
        Some(&self.pwd[0..self.pwd.len() - 1])
    }

    /// Set the title for the terminal, as set by escape sequences (e.g. OSC 0/2).
    // PORT NOTE (aliasing): `t` MAY alias `self.title` — a caller can hand
    // `get_title()`'s return straight back through the vt-c FFI seam. Every
    // mutation below therefore goes through a raw `*mut Vec<u8>` and the copy
    // uses `core::ptr::copy` (memmove, = Zig `std.mem.copyForwards`). NEVER use
    // `extend_from_slice`/`copy_from_slice` here: both are `memcpy` and are UB
    // on overlapping ranges. That is upstream dace6d1c6.
    //
    // PORT FINDING (WS8 T7): see the finding on `set_pwd` — the alias-tolerant
    // entry point is `set_title_raw`.
    pub fn set_title(&mut self, t: &[u8]) -> Result<(), OutOfMemory> {
        // SAFETY: `t` is a live slice readable for `t.len()` bytes.
        unsafe { self.set_title_raw(t.as_ptr(), t.len()) }
    }

    /// `set_title` over a raw source, tolerating a source that ALIASES
    /// `self.title` (see the PORT FINDING on `set_pwd`).
    ///
    /// # Safety
    ///
    /// `src` must be readable for `src_len` bytes. It MAY overlap
    /// `self.title`'s buffer; it must not be derived from a live Rust
    /// reference that outlives this call.
    pub unsafe fn set_title_raw(
        &mut self,
        src: *const u8,
        src_len: usize,
    ) -> Result<(), OutOfMemory> {
        let t_len = src_len;
        // if (t.len == 0) { self.title.clearRetainingCapacity(); return; }
        if t_len == 0 {
            self.title.clear_retaining_capacity();
            return Ok(());
        }

        // const capacity = std.math.add(usize, t.len, 1) catch
        //     return error.OutOfMemory;
        let capacity = match t_len.checked_add(1) {
            Some(c) => c,
            None => return Err(OutOfMemory),
        };

        // try self.title.ensureTotalCapacity(self.gpa(), capacity);
        //
        // PORT NOTE: grow-only + amortized, through `self.gpa()` (as `set_pwd`).
        let gpa = self.gpa();
        let v: *mut crate::alloc::ArrayListUnmanaged<u8> = core::ptr::addr_of_mut!(self.title);
        // SAFETY: as in `set_pwd` above.
        unsafe {
            (*v).ensure_total_capacity(gpa, capacity)
                .map_err(|_| OutOfMemory)?;
            let dst: *mut u8 = (*v).as_mut_ptr();
            // self.title.items.len = capacity;
            (*v).set_len(capacity);
            // std.mem.copyForwards(u8, self.title.items[0..t.len], t);
            core::ptr::copy(src, dst, src_len);
            // self.title.items[t.len] = 0;
            *dst.add(src_len) = 0;
        }
        Ok(())
    }

    /// Returns the title for the terminal, if any. The memory is owned by the
    /// Terminal and is not copied. It is safe until a reset or setTitle.
    // pub fn getTitle(self: *const Terminal) ?[:0]const u8
    // PORT NOTE: Zig `[:0]const u8` -> `Option<&[u8]>` (NUL sentinel stripped).
    pub fn get_title(&self) -> Option<&[u8]> {
        // if (self.title.items.len == 0) return null;
        if self.title.is_empty() {
            return None;
        }
        // return self.title.items[0 .. self.title.items.len - 1 :0];
        Some(&self.title[0..self.title.len() - 1])
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
        // If we're already on the requested screen we do nothing.
        // if (self.screens.active_key == key) return null;
        if self.screens.active_key == key {
            return Ok(None);
        }
        // const old = self.screens.active;
        let old: *mut Screen = self.screens.active;

        // We always end hyperlink state when switching screens.
        // We need to do this on the original screen.
        // old.endHyperlink();
        // SAFETY: `old` is a stable heap pointer owned by `self.screens`;
        // `end_hyperlink` takes `&mut Screen` and does not mutate `self.screens`.
        unsafe { (*old).end_hyperlink() };

        // Switch the screens/
        // const new = self.screens.get(key) orelse new: {
        //     const primary = self.screens.get(.primary).?;
        //     break :new try self.screens.getInit(
        //         old.alloc,
        //         key,
        //         .{ ... },
        //     );
        // };
        let new: *mut Screen = match self.screens.get(key) {
            Some(s) => s,
            None => {
                let primary: *mut Screen =
                    self.screens.get(crate::screen_set::Key::Primary).unwrap();
                // SAFETY: `old`/`primary` are stable heap pointers owned by
                // `self.screens`; we only read `old.alloc` and
                // `primary.pages.limits.bytes.explicit`.
                let old_alloc = unsafe { (*old).alloc };
                let primary_max = unsafe { (*primary).pages.limits.bytes.explicit };
                // Inherit our Kitty image settings from the primary screen
                // (see the Options fields below).
                let (primary_kitty_limit, primary_kitty_loading_limits) =
                    if crate::options::KITTY_GRAPHICS {
                        unsafe {
                            (
                                (*primary).kitty_images.total_limit,
                                (*primary).kitty_images.image_limits.clone(),
                            )
                        }
                    } else {
                        (0, kitty::graphics_image::Limits::DIRECT)
                    };
                self.screens.get_init(
                    old_alloc,
                    key,
                    crate::screen::Options {
                        cols: self.cols,
                        rows: self.rows,
                        // .max_scrollback_bytes = switch (key) {
                        //     .primary => primary.pages.limits.bytes.explicit,
                        //     .alternate => 0,
                        // },
                        max_scrollback_bytes: match key {
                            crate::screen_set::Key::Primary => Some(primary_max),
                            crate::screen_set::Key::Alternate => Some(0),
                        },
                        max_scrollback_lines: None,

                        // Inherit our Kitty image settings from the primary
                        // screen if we have to initialize.
                        // .kitty_image_storage_limit = if (comptime build_options.kitty_graphics)
                        //     primary.kitty_images.total_limit
                        // else
                        //     0,
                        // .kitty_image_loading_limits = if (comptime build_options.kitty_graphics)
                        //     primary.kitty_images.image_limits
                        // else {},
                        // PORT NOTE (WS8 T16): the original vt-12 deferral
                        // hard-coded `0`/`DIRECT` here on the grounds that
                        // `build_options.kitty_graphics` was the disabled case.
                        // Kitty graphics IS enabled in rustty
                        // (`options::KITTY_GRAPHICS`), so hard-coding `0` gave the
                        // lazily-created alternate screen a DISABLED image storage
                        // — kitty graphics silently did not work on the alt
                        // screen, and `set_limit(0)` also marked the fresh storage
                        // mutated. Now inherits from the primary, as the reference
                        // does. The reference's `else {}` is the `void` field; the
                        // Rustty `Options` field is always `Limits`, so `DIRECT`
                        // (all-false, the empty representation) stands in for the
                        // unused `void` on the disabled branch.
                        kitty_image_storage_limit: primary_kitty_limit,
                        kitty_image_loading_limits: primary_kitty_loading_limits,
                    },
                )?
            }
        };

        // The new screen should not have any hyperlinks set
        // assert(new.cursor.hyperlink_id == 0);
        // SAFETY: `new` is a stable heap pointer owned by `self.screens`.
        debug_assert!(unsafe { (*new).cursor.hyperlink_id } == 0);

        // Bring our charset state with us
        // new.charset = old.charset;
        // SAFETY: `old`/`new` are distinct stable heap pointers owned by
        // `self.screens` (the early return above guarantees `key != active_key`,
        // and `new` is either a different initialized screen or freshly created).
        unsafe { (*new).charset = (*old).charset.clone() };

        // Clear our selection
        // new.clearSelection();
        unsafe { (*new).clear_selection() };

        // if (comptime build_options.kitty_graphics) {
        //     // Mark kitty images as dirty so they redraw. Without this set
        //     // the images will remain where they were (the dirty bit on
        //     // the screen only tracks the terminal grid, not the images).
        //     new.kitty_images.dirty = true;
        // }
        // PORT NOTE (WS8 T16, same correction as the storage-limit inheritance
        // above): Kitty graphics IS enabled in rustty
        // (`options::KITTY_GRAPHICS`), so this set is NOT the disabled case. It
        // matters for re-entry into an ALREADY-CREATED screen (the primary
        // always exists) whose `dirty` a renderer has since cleared: without it
        // the destination screen's images never redraw, because
        // `flags.dirty.clear` below only tracks the terminal grid.
        if crate::options::KITTY_GRAPHICS {
            // SAFETY: `new` is a stable heap pointer owned by `self.screens`
            // (either an existing screen or the one just created above).
            unsafe { (*new).kitty_images.dirty = true };
        }

        // Mark our terminal as dirty to redraw the grid.
        // self.flags.dirty.clear = true;
        self.flags.dirty.clear = true;

        // Finalize the switch
        // self.screens.switchTo(key);
        self.screens.switch_to(key);

        // return old;
        Ok(Some(old))
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
        // The behavior in this function is completely based on reading
        // the xterm source, specifically "charproc.c" for
        // `srm_ALTBUF`, `srm_OPT_ALTBUF`, and `srm_OPT_ALTBUF_CURSOR`.
        // We shouldn't touch anything in here without adding a unit
        // test AND verifying the behavior with xterm.

        // switch (mode) {
        //     .@"47" => {},
        //     .@"1047" => if (!enabled and self.screens.active_key == .alternate) {
        //         self.eraseDisplay(.complete, false);
        //     },
        //     .@"1049" => if (enabled) self.saveCursor(),
        // }
        match mode {
            SwitchScreenMode::M47 => {}

            // If we're disabling 1047 and we're on alt screen then
            // we clear the screen.
            SwitchScreenMode::M1047 => {
                if !enabled && self.screens.active_key == crate::screen_set::Key::Alternate {
                    self.erase_display(csi::EraseDisplay::Complete, false);
                }
            }

            // 1049 unconditionally saves the cursor on enabling, even
            // if we're already on the alternate screen.
            SwitchScreenMode::M1049 => {
                if enabled {
                    self.save_cursor();
                }
            }
        }

        // Switch screens first to whatever we're going to.
        // const to: ScreenSet.Key = if (enabled) .alternate else .primary;
        // const old_ = try self.switchScreen(to);
        let to: crate::screen_set::Key = if enabled {
            crate::screen_set::Key::Alternate
        } else {
            crate::screen_set::Key::Primary
        };
        let old_: Option<*mut Screen> = self.switch_screen(to)?;

        match mode {
            // For these modes, we need to copy the cursor. We only copy
            // the cursor if the screen actually changed, otherwise the
            // cursor is already copied. The cursor is copied regardless
            // of destination screen.
            // .@"47", .@"1047" => if (old_) |old| {
            //     self.screens.active.cursorCopy(old.cursor, .{ .hyperlink = false }) catch |err| {
            //         log.warn("cursor copy failed entering alt screen err={}", .{err});
            //     };
            // },
            SwitchScreenMode::M47 | SwitchScreenMode::M1047 => {
                if let Some(old) = old_ {
                    // SAFETY: `old` and `self.screens.active` are stable heap
                    // pointers owned by `self.screens`; `cursorCopy` takes
                    // `&mut Screen` and `old.cursor` is a `Copy` value.
                    let old_cursor = unsafe { (*old).cursor };
                    let active: *mut Screen = self.screens.active;
                    if let Err(_err) = unsafe {
                        (*active).cursor_copy(
                            old_cursor,
                            crate::screen::CursorCopyOptions { hyperlink: false },
                        )
                    } {
                        // log.warn("cursor copy failed entering alt screen err={}", .{err});
                        // PORT NOTE: this crate has no logging facility (all
                        // `std.log` calls are preserved as comments); on the
                        // copy-failure path we just continue, matching the
                        // reference `catch` that only logs and proceeds.
                    }
                }
            }

            // Mode 1049 restores cursor on the primary screen when
            // we disable it.
            // .@"1049" => if (enabled) {
            //     assert(self.screens.active_key == .alternate);
            //     self.eraseDisplay(.complete, false);
            //     if (old_) |old| {
            //         self.screens.active.cursorCopy(old.cursor, .{ .hyperlink = false }) catch |err| {
            //             log.warn("cursor copy failed entering alt screen err={}", .{err});
            //         };
            //     }
            // } else {
            //     assert(self.screens.active_key == .primary);
            //     self.restoreCursor();
            // },
            SwitchScreenMode::M1049 => {
                if enabled {
                    debug_assert!(self.screens.active_key == crate::screen_set::Key::Alternate);
                    self.erase_display(csi::EraseDisplay::Complete, false);

                    // When we enter alt screen with 1049, we always copy the
                    // cursor from the primary screen (if we weren't already
                    // on it).
                    if let Some(old) = old_ {
                        // SAFETY: see the M47/M1047 arm above.
                        let old_cursor = unsafe { (*old).cursor };
                        let active: *mut Screen = self.screens.active;
                        if let Err(_err) = unsafe {
                            (*active).cursor_copy(
                                old_cursor,
                                crate::screen::CursorCopyOptions { hyperlink: false },
                            )
                        } {
                            // log.warn("cursor copy failed entering alt screen err={}", .{err});
                            // PORT NOTE: see the M47/M1047 arm above; no logging
                            // facility, so the copy-failure path just continues.
                        }
                    }
                } else {
                    debug_assert!(self.screens.active_key == crate::screen_set::Key::Primary);
                    self.restore_cursor();
                }
            }
        }

        Ok(())
    }

    /// Return the current string value of the terminal. Newlines are
    /// encoded as "\n". This omits any formatting such as fg/bg.
    ///
    /// The caller must free the string.
    // PORT NOTE: the reference's inferred error set here also carries
    // `error.UnknownPoint` from `dumpStringAlloc`, but that arm is unreachable
    // for a `.viewport` point (the viewport tag always resolves a bottom-right),
    // so these keep the `OutOfMemory`-only skeleton signature their callers
    // (e.g. rustty-core surface.rs) expect and fold the impossible variant in.
    pub fn plain_string(&mut self, alloc: Allocator) -> Result<Vec<u8>, OutOfMemory> {
        // return try self.screens.active.dumpStringAlloc(alloc, .{ .viewport = .{} });
        self.active_mut()
            .dump_string_alloc(alloc, point::Point::Viewport(point::Coordinate::default()))
            .map_err(|_| OutOfMemory)
    }

    /// Same as plainString, but respects row wrap state when building the string.
    pub fn plain_string_unwrapped(&mut self, alloc: Allocator) -> Result<Vec<u8>, OutOfMemory> {
        // return try self.screens.active.dumpStringAllocUnwrapped(alloc, .{ .viewport = .{} });
        self.active_mut()
            .dump_string_alloc_unwrapped(
                alloc,
                point::Point::Viewport(point::Coordinate::default()),
            )
            .map_err(|_| OutOfMemory)
    }

    /// Full reset.
    ///
    /// This will attempt to free the existing screen memory but if that fails
    /// this will reuse the existing memory. In the latter case, memory may
    /// be wasted (since its unused) but it isn't leaked.
    pub fn full_reset(&mut self) {
        // Ensure we're back on primary screen
        // self.screens.switchTo(.primary);
        // self.screens.remove(self.screens.active.alloc, .alternate);
        self.screens.switch_to(crate::screen_set::Key::Primary);
        // SAFETY: `self.screens.active` is now the primary screen (a stable heap
        // pointer owned by `self.screens`); we read only its `alloc`. `remove`
        // mutates `self.screens.all[alternate]`, which is a distinct entry from
        // the active (primary) screen, so the read isn't invalidated.
        let alloc = unsafe { (*self.screens.active).alloc };
        self.screens
            .remove(alloc, crate::screen_set::Key::Alternate);

        // Reset our screens
        // self.screens.active.reset();
        // SAFETY: `self.screens.active` is the primary screen; `reset` takes
        // `&mut Screen` and does not mutate `self.screens`.
        unsafe { (*self.screens.active).reset() };

        // Rest our basic state
        // self.modes.reset();
        // self.flags = .{};
        // self.tabstops.reset(TABSTOP_INTERVAL);
        // self.previous_char = null;
        // self.pwd.clearRetainingCapacity();
        // self.title.clearRetainingCapacity();
        // self.glyph_glossary.clearAndFree(self.gpa());
        // self.status_display = .main;
        // const visible = self.flags.visible;
        // self.modes.reset();
        // self.flags = .{
        //     // Visibility belongs to the view rather than terminal state, so a
        //     // terminal reset must not make a hidden view potentially visible.
        //     .visible = visible,
        // };
        let visible = self.flags.visible;
        self.modes.reset();
        self.flags = Flags {
            // Visibility belongs to the view rather than terminal state, so a
            // terminal reset must not make a hidden view potentially visible.
            visible,
            ..Flags::default()
        };
        self.tabstops.reset(TABSTOP_INTERVAL as usize);
        self.previous_char = None;
        // self.pwd.clearRetainingCapacity();
        // self.title.clearRetainingCapacity();
        self.pwd.clear_retaining_capacity();
        self.title.clear_retaining_capacity();
        // self.glyph_glossary.clearAndFree(self.gpa());
        // PORT NOTE (parity tail S7-3): `clear_and_free` takes no allocator: it
        // frees every entry and the map's storage through the allocator the
        // map was bound to at `init` (`Glossary::empty_in(alloc)`), which is
        // `self.gpa()` (checked in debug builds).
        let gpa = self.gpa();
        self.glyph_glossary.allocator().debug_assert_same(gpa);
        self.glyph_glossary.clear_and_free();
        // A reset only interrupts an in-progress chunked OSC 72 command;
        // drag and drop registration survives, matching kitty.
        // if (self.kitty_dnd) |dnd| dnd.chunking = .{};
        if let Some(dnd) = self.kitty_dnd.as_mut() {
            dnd.chunking = Default::default();
        }
        self.status_display = ansi::StatusDisplay::Main;
        // self.scrolling_region = .{
        //     .top = 0,
        //     .bottom = self.rows - 1,
        //     .left = 0,
        //     .right = self.cols - 1,
        // };
        self.scrolling_region = ScrollingRegion {
            top: 0,
            bottom: self.rows - 1,
            left: 0,
            right: self.cols - 1,
        };
        // self.setCursorStyle(.default);
        self.set_cursor_style(ansi::CursorStyle::Default);

        // Always mark dirty so we redraw everything
        // self.flags.dirty.clear = true;
        self.flags.dirty.clear = true;
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
mod tests {

    // PORT NOTE: no reference test. `Screen.testWriteString`'s inferred error
    // set carries `PageList.IncreaseCapacityError` through
    // `try self.cursorSetHyperlink()`; the port's `PrintStringError` had no
    // `OutOfSpace`, so all four of those sites did
    // `.map_err(|_| OutOfMemory)?` and the one distinguishable error was lost
    // (parity S134-F2). The four sites now use a bare `?`, which routes through
    // this conversion — pinned here. Reaching `OutOfSpace` from a real page
    // needs a page filled to its hyperlink-map limit, which no reference test
    // constructs either; what IS pinned is that the variant survives the
    // conversion the `?` uses.
    #[test]
    fn print_string_error_preserves_out_of_space() {
        use crate::pagelist::grow::IncreaseCapacityError;
        assert_eq!(
            PrintStringError::OutOfSpace,
            PrintStringError::from(IncreaseCapacityError::OutOfSpace),
        );
        assert_eq!(
            PrintStringError::OutOfMemory,
            PrintStringError::from(IncreaseCapacityError::OutOfMemory),
        );
    }
    use super::*;
    use crate::alloc::Global;

    // test "Terminal: setPwd preserves a sentinel on allocation failure"
    #[test]
    fn terminal_set_pwd_preserves_a_sentinel_on_allocation_failure() {
        // var failing = testing.FailingAllocator.init(testing.allocator, .{});
        // const alloc = failing.allocator();
        let failing = crate::alloc::testing::FailingAllocator::over_heap(
            crate::alloc::testing::Config::default(),
        );
        let c = failing.allocator();
        crate::alloc::testing::with_handle(&c, |alloc| {
            let mut t = Terminal::init(alloc, opts(5, 1)).unwrap();
            // defer t.deinit(alloc);

            // try t.pwd.ensureTotalCapacityPrecise(alloc, 3);
            t.pwd.ensure_total_capacity_precise(alloc, 3).unwrap();
            // failing.fail_index = failing.alloc_index;
            failing.fail_index.set(failing.alloc_index.get());
            // PORT NOTE: the growth tries `remap` first. The reference's inner
            // `testing.allocator` (DebugAllocator) cannot remap a 3-byte
            // size-class slot to the grown capacity, so its remap is null and
            // the growth reaches the failing `alloc`. This test's inner heap
            // remaps with `realloc`, which may move, so the remap is refused
            // here explicitly to reproduce that answer.
            failing.resize_fail_index.set(failing.resize_index.get());
            assert_eq!(Err(OutOfMemory), t.set_pwd(b"pwd"));
            assert!(failing.has_induced_failure.get());
            assert!(t.get_pwd().is_none());

            t.deinit(alloc);
        });
    }

    // test "Terminal: setPwd accepts its current value"
    #[test]
    fn terminal_set_pwd_accepts_its_current_value() {
        let mut t = Terminal::init(Global, opts(5, 1)).unwrap();

        t.set_pwd(b"file:///tmp").unwrap();

        // PORT NOTE: the reference writes `t.setPwd(t.getPwd().?)`. Borrowck
        // forbids holding the `&[u8]` from `get_pwd` across the `&mut self`
        // call, so we re-derive the SAME aliasing source as a RAW pointer and
        // go through `set_pwd_raw`. This is exactly the aliasing the vt-c FFI
        // seam can produce. Passing it as a `&[u8]` to `set_pwd` instead would
        // be Stacked-Borrows UB — the argument is strongly protected for the
        // duration of the call, so writing into it is illegal regardless of
        // memmove-vs-memcpy (proven under Miri, WS8 T7).
        let (ptr, len) = {
            let cur = t.get_pwd().unwrap();
            (cur.as_ptr(), cur.len())
        };
        unsafe { t.set_pwd_raw(ptr, len) }.unwrap();

        assert_eq!(b"file:///tmp".as_slice(), t.get_pwd().unwrap());

        t.deinit(Global);
    }

    // test "Terminal: setTitle preserves a sentinel on allocation failure"
    #[test]
    fn terminal_set_title_preserves_a_sentinel_on_allocation_failure() {
        let failing = crate::alloc::testing::FailingAllocator::over_heap(
            crate::alloc::testing::Config::default(),
        );
        let c = failing.allocator();
        crate::alloc::testing::with_handle(&c, |alloc| {
            let mut t = Terminal::init(alloc, opts(5, 1)).unwrap();
            // defer t.deinit(alloc);

            // try t.title.ensureTotalCapacityPrecise(alloc, 5);
            t.title.ensure_total_capacity_precise(alloc, 5).unwrap();
            // failing.fail_index = failing.alloc_index;
            failing.fail_index.set(failing.alloc_index.get());
            // PORT NOTE: the growth tries `remap` first. The reference's inner
            // `testing.allocator` (DebugAllocator) cannot remap a 5-byte
            // size-class slot to the grown capacity, so its remap is null and
            // the growth reaches the failing `alloc`. This test's inner heap
            // remaps with `realloc`, which may move, so the remap is refused
            // here explicitly to reproduce that answer.
            failing.resize_fail_index.set(failing.resize_index.get());
            assert_eq!(Err(OutOfMemory), t.set_title(b"title"));
            assert!(failing.has_induced_failure.get());
            assert!(t.get_title().is_none());

            t.deinit(alloc);
        });
    }

    // test "Terminal: setTitle accepts its current value"
    #[test]
    fn terminal_set_title_accepts_its_current_value() {
        let mut t = Terminal::init(Global, opts(5, 1)).unwrap();

        t.set_title(b"Rustty").unwrap();

        // See the PORT NOTE in `terminal_set_pwd_accepts_its_current_value`.
        let (ptr, len) = {
            let cur = t.get_title().unwrap();
            (cur.as_ptr(), cur.len())
        };
        unsafe { t.set_title_raw(ptr, len) }.unwrap();

        assert_eq!(b"Rustty".as_slice(), t.get_title().unwrap());

        t.deinit(Global);
    }

    // PORT NOTE: the reference tests call `init(alloc, .{ .cols = .., .rows = .. })`
    // and rely on `Options` field defaults (`max_scrollback_bytes = 10_000`,
    // `colors = .default`, `default_modes = .{}`, kitty limits). Rust `Options`
    // has no field defaults, so this helper supplies them verbatim, mirroring the
    // reference `Options` declaration. `colors = .default` expands to
    // `Colors{ background/foreground/cursor = .unset, palette = .default }`
    // (`Colors::default()` inlined here for tests only).
    fn opts(cols: size::CellCountInt, rows: size::CellCountInt) -> Options {
        Options {
            cols,
            rows,
            max_scrollback_bytes: Some(10_000),
            max_scrollback_lines: None,
            colors: Colors {
                background: DynamicRGB::UNSET,
                foreground: DynamicRGB::UNSET,
                cursor: DynamicRGB::UNSET,
                palette: DynamicPalette::default_palette(),
            },
            default_modes: modespkg::ModePacked::default(),
            default_cursor_style: crate::screen::CursorStyle::Block,
            default_cursor_blink: Some(false),
            kitty_image_storage_limit: 320 * 1000 * 1000,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        }
    }

    // Same as `opts` but with a caller-supplied `max_scrollback_bytes`, mirroring
    // reference inits like `.{ .rows, .cols, .max_scrollback_bytes = 0 }`.
    fn opts_sb(
        cols: size::CellCountInt,
        rows: size::CellCountInt,
        max_scrollback_bytes: usize,
    ) -> Options {
        Options {
            max_scrollback_bytes: Some(max_scrollback_bytes),
            ..opts(cols, rows)
        }
    }

    // Screen-point getCell helper mirroring the reference
    // `t.screens.active.pages.getCell(.{ .screen = .{ .x, .y } })`.
    fn screen_pt(x: size::CellCountInt, y: u32) -> point::Point {
        point::Point::Screen(point::Coordinate { x, y })
    }

    // Active-point helper mirroring the reference
    // `.{ .active = .{ .x, .y } }` literals used in the dirty-bit assertions.
    fn active_pt(x: size::CellCountInt, y: u32) -> point::Point {
        point::Point::Active(point::Coordinate { x, y })
    }

    // Viewport-point helper mirroring the reference
    // `t.screens.active.pages.getCell(.{ .viewport = .{ .x, .y } })` literals.
    fn viewport_pt(x: size::CellCountInt, y: u32) -> point::Point {
        point::Point::Viewport(point::Coordinate { x, y })
    }

    // ref: test "Terminal setScrollbackMaxBytes"
    #[test]
    fn terminal_set_scrollback_max_bytes() {
        let mut t = Terminal::init(
            Global,
            Options {
                max_scrollback_bytes: None,
                ..opts(80, 3)
            },
        )
        .unwrap();

        let primary: *mut crate::screen::Screen =
            t.screens.get(crate::screen_set::Key::Primary).unwrap();
        let page_rows: usize =
            unsafe { crate::pagelist::Node::capacity((*primary).pages.pages.first).rows as usize };

        // Build several complete pages of history so lowering the byte limit has
        // existing allocations to prune immediately.
        for _ in 0..(4 * page_rows) {
            t.linefeed().unwrap();
        }
        let old_page_size = unsafe { (*primary).pages.page_size };
        t.set_scrollback_max_bytes(Some(1));
        unsafe {
            assert_eq!(1usize, (*primary).pages.limits.bytes.explicit);
            assert!((*primary).pages.page_size < old_page_size);
            assert!(
                (*primary).pages.page_size
                    <= (*primary).pages.limits.max(crate::pagelist::Key::Bytes)
            );
            assert!(!(*primary).no_scrollback);
        }

        // Zero switches Screen behavior as well as PageList accounting, discards
        // all retained history, and prevents future linefeeds from recreating it.
        t.set_scrollback_max_bytes(Some(0));
        unsafe {
            assert_eq!(0usize, (*primary).pages.limits.bytes.explicit);
            assert!((*primary).no_scrollback);
            assert_eq!((*primary).pages.rows as usize, (*primary).pages.total_rows);
            assert!((*primary).pages.viewport == crate::pagelist::Viewport::Active);
        }

        for _ in 0..page_rows {
            t.linefeed().unwrap();
        }
        unsafe {
            assert_eq!((*primary).pages.rows as usize, (*primary).pages.total_rows);
        }

        // Re-enabling unlimited scrollback affects subsequent output.
        t.set_scrollback_max_bytes(None);
        unsafe {
            assert_eq!(usize::MAX, (*primary).pages.limits.bytes.explicit);
            assert!(!(*primary).no_scrollback);
        }
        for _ in 0..page_rows {
            t.linefeed().unwrap();
        }
        unsafe {
            assert!((*primary).pages.total_rows > (*primary).pages.rows as usize);
        }

        t.deinit(Global);
    }

    // ref: test "Terminal setScrollbackMaxLines"
    #[test]
    fn terminal_set_scrollback_max_lines() {
        let mut t = Terminal::init(
            Global,
            Options {
                max_scrollback_bytes: None,
                max_scrollback_lines: None,
                ..opts(80, 3)
            },
        )
        .unwrap();

        let primary: *mut crate::screen::Screen =
            t.screens.get(crate::screen_set::Key::Primary).unwrap();
        let page_rows: usize =
            unsafe { crate::pagelist::Node::capacity((*primary).pages.pages.first).rows as usize };

        for _ in 0..(4 * page_rows) {
            t.linefeed().unwrap();
        }
        let old_total_rows = unsafe { (*primary).pages.total_rows };
        t.set_scrollback_max_lines(Some(page_rows));
        unsafe {
            assert_eq!(page_rows, (*primary).pages.limits.lines.explicit);
            assert!((*primary).pages.total_rows < old_total_rows);
            assert!(!(*primary)
                .pages
                .limits
                .exceeded(&(*primary).pages, crate::pagelist::Key::Lines));
        }

        let limited_total_rows = unsafe { (*primary).pages.total_rows };
        t.set_scrollback_max_lines(None);
        unsafe {
            assert_eq!(usize::MAX, (*primary).pages.limits.lines.explicit);
            assert_eq!(limited_total_rows, (*primary).pages.total_rows);
        }
        for _ in 0..(3 * page_rows) {
            t.linefeed().unwrap();
        }
        unsafe {
            assert!((*primary).pages.total_rows - (*primary).pages.rows as usize > page_rows);
        }

        t.deinit(Global);
    }

    // ref: test "Terminal setScrollback only affects primary screen"
    #[test]
    fn terminal_set_scrollback_only_affects_primary_screen() {
        let mut t = Terminal::init(Global, opts(80, 3)).unwrap();

        let _ = t.switch_screen(crate::screen_set::Key::Alternate).unwrap();
        let primary: *mut crate::screen::Screen =
            t.screens.get(crate::screen_set::Key::Primary).unwrap();
        let alternate: *mut crate::screen::Screen =
            t.screens.get(crate::screen_set::Key::Alternate).unwrap();

        t.set_scrollback_max_bytes(Some(123));
        t.set_scrollback_max_lines(Some(456));

        unsafe {
            assert_eq!(123usize, (*primary).pages.limits.bytes.explicit);
            assert_eq!(456usize, (*primary).pages.limits.lines.explicit);
            assert!(!(*primary).no_scrollback);

            assert_eq!(0usize, (*alternate).pages.limits.bytes.explicit);
            assert_eq!(usize::MAX, (*alternate).pages.limits.lines.explicit);
            assert!((*alternate).no_scrollback);
        }
        assert!(core::ptr::eq(alternate, t.screens.active));

        t.deinit(Global);
    }

    // ref: test "Terminal forwards optional scrollback limits"
    #[test]
    fn terminal_forwards_optional_scrollback_limits() {
        let max_lines: usize = 123;
        let mut t = Terminal::init(
            Global,
            Options {
                max_scrollback_bytes: None,
                max_scrollback_lines: Some(max_lines),
                ..opts(80, 24)
            },
        )
        .unwrap();

        assert_eq!(usize::MAX, unsafe {
            (*t.screens.active).pages.limits.bytes.explicit
        });
        assert_eq!(max_lines, unsafe {
            (*t.screens.active).pages.limits.lines.explicit
        });

        t.deinit(Global);
    }

    // S141-F4: `print_inner_zero_width`'s "previous cell has no text" guard
    // logs the SAME `log.warn("zero-width character with no prior
    // character, ignoring", .{})` (Terminal.zig:1491) as the `x == 0` guard
    // above, but on the OTHER branch: cursor not at column 0, yet the cell
    // to its left was never written.
    #[test]
    fn terminal_zero_width_character_with_blank_prior_cell_logs() {
        let mut t = Terminal::init(Global, opts(10, 2)).unwrap();
        t.modes.set(modespkg::Mode::GraphemeCluster, false);

        // Move the cursor to a column whose left neighbor is blank, without
        // ever writing there (`set_cursor_pos` alone writes nothing).
        t.set_cursor_pos(1, 5);

        let cap = crate::log_capture::start();
        t.print(0x0332).unwrap(); // Combining low line — a zero-width mark.
        let messages = cap.messages();
        assert!(
            messages
                .iter()
                .any(|(level, target, msg)| *level == log::Level::Warn
                    && target == "terminal"
                    && *msg == "zero-width character with no prior character, ignoring"),
            "expected the zero-width-no-prior-character diagnostic, got: {messages:?}"
        );
        drop(cap);

        t.deinit(Global);
    }

    // test "Terminal: zero-width character attaches to pending wrap cell"
    // https://github.com/ghostty-org/ghostty/pull/12581
    #[test]
    fn terminal_zero_width_character_attaches_to_pending_wrap_cell() {
        let mut t = Terminal::init(Global, opts(2, 2)).unwrap();

        // Disable grapheme clustering to exercise the fallback path.
        t.modes.set(modespkg::Mode::GraphemeCluster, false);

        t.print('x' as u32).unwrap();
        t.print('å' as u32).unwrap();
        t.print(0x0332).unwrap(); // Combining low line.

        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "xå̲".as_bytes());

        t.deinit(Global);
    }

    // test "Terminal: caps zero-width codepoints attached to one cell"
    #[test]
    fn terminal_caps_zero_width_codepoints_attached_to_one_cell() {
        let mut t = Terminal::init(Global, opts(2, 2)).unwrap();

        t.modes.set(modespkg::Mode::GraphemeCluster, false);
        t.print('A' as u32).unwrap();

        let initial_capacity =
            unsafe { crate::pagelist::Node::capacity((*(*t.screens.active).cursor.page_pin).node) }
                .grapheme_bytes;
        for _ in 0..pagepkg::GRAPHEME_MAX_LEN * 4 {
            t.print(0x0301).unwrap();
        }

        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
        assert_eq!(
            pagepkg::GRAPHEME_MAX_LEN,
            unsafe {
                (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(list_cell.cell)
            }
            .unwrap()
            .len(),
        );
        assert_eq!(
            initial_capacity,
            unsafe { crate::pagelist::Node::capacity(list_cell.node) }.grapheme_bytes
        );

        t.deinit(Global);
    }

    // test "Terminal: VS16 doesn't make character with 2027 disabled"
    #[test]
    fn terminal_vs16_doesnt_make_character_with_2027_disabled() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        // Disable grapheme clustering
        t.modes.set(modespkg::Mode::GraphemeCluster, false);

        t.print(0x2764).unwrap(); // Heart
        t.print(0xFE0F).unwrap(); // VS16 to make wide

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "❤️".as_bytes());
        }

        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0x2764);
            assert!(unsafe { (*cell).has_grapheme() });
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
            let cps =
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell) }
                    .unwrap();
            assert_eq!(cps.len(), 1);
        }

        t.deinit(Global);
    }

    // Terminal.print receives one codepoint at a time, so it can't use
    // unicode::grapheme::grapheme_width directly; that API requires a complete
    // buffered cluster or string end. This keeps the streaming printer's
    // cursor advance in sync with the buffered measurement API. (oracle 65e61282a)
    fn expect_grapheme_width_parity(cps: &[u32]) {
        let mut t = Terminal::init(Global, opts(80, 5)).unwrap();
        t.modes.set(modespkg::Mode::GraphemeCluster, true);

        let mut expected: usize = 0;
        let mut i: usize = 0;
        while i < cps.len() {
            let result = unicode::grapheme::grapheme_width(&cps[i..]);
            assert!(result.len > 0);
            i += result.len;
            expected += result.width as usize;
        }

        for &cp in cps {
            t.print(cp).unwrap();
        }
        assert_eq!(0, unsafe { (*t.screens.active).cursor.y });
        assert_eq!(expected, unsafe { (*t.screens.active).cursor.x } as usize);
        t.deinit(Global);
    }

    // test "Terminal: graphemeWidth parity" (oracle 65e61282a)
    #[test]
    fn terminal_grapheme_width_parity() {
        expect_grapheme_width_parity(&[0x2764, 0xFE0F]);
        expect_grapheme_width_parity(&['x' as u32, 0xFE0F, 0xFE0F]);
        expect_grapheme_width_parity(&[0x231A, 0xFE0E, 0xFE0F]);
        expect_grapheme_width_parity(&[0x1F3F4, 0x200D, 0x2620, 0xFE0F]);
        expect_grapheme_width_parity(&[0x1F468, 0x200D, 0x1F469, 0x200D, 0x1F467]);
        expect_grapheme_width_parity(&[0x23, 0xFE0F, 0x20E3]);
        expect_grapheme_width_parity(&['1' as u32, 0x20E3]);
        expect_grapheme_width_parity(&[0x1F44B, 0x1F3FF]);
        expect_grapheme_width_parity(&[0x1F1E6, 0x1F1E7, 0x1F1E8]);
        expect_grapheme_width_parity(&['a' as u32, 'b' as u32]);
        expect_grapheme_width_parity(&[0x0301, 0x0302]);
    }

    // test "Terminal: print invalid VS16 non-grapheme"
    #[test]
    fn terminal_print_invalid_vs16_non_grapheme() {
        let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

        // https://github.com/mitchellh/ghostty/issues/1482
        t.print('x' as u32).unwrap();
        t.print(0xFE0F).unwrap();

        // We should have 1 narrow cell.
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 1);

        // Assert various properties about our screen to verify
        // we have all expected cells.
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 'x' as u21);
            assert!(!unsafe { (*cell).has_grapheme() });
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
        }
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
        }

        t.deinit(Global);
    }

    // test "Terminal: variation selectors apply to preceding codepoint"
    // https://github.com/ghostty-org/ghostty/pull/12596
    #[test]
    fn terminal_variation_selectors_apply_to_preceding_codepoint() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        // Enable grapheme clustering
        t.modes.set(modespkg::Mode::GraphemeCluster, true);

        // Pirate flag: black flag + ZWJ + skull and crossbones + VS16.
        t.print(0x1F3F4).unwrap();
        t.print(0x200D).unwrap();
        t.print(0x2620).unwrap();
        t.print(0xFE0F).unwrap();

        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0x1F3F4);
        assert!(unsafe { (*cell).has_grapheme() });
        assert_eq!(
            unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell) }
                .unwrap(),
            &[0x200D, 0x2620, 0xFE0F]
        );

        t.deinit(Global);
    }

    // test "Terminal: keypad sequence VS15"
    #[test]
    fn terminal_keypad_sequence_vs15() {
        let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

        // Enable grapheme clustering
        t.modes.set(modespkg::Mode::GraphemeCluster, true);

        // This is: "#︎" (number sign with text presentation selector)
        t.print(0x23).unwrap(); // # Number sign (valid base)
        t.print(0xFE0E).unwrap(); // VS15 (text presentation selector)

        // VS15 should combine with the base character into a single grapheme cluster,
        // taking 1 cell (narrow character).
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 1);

        // Row should be dirty
        assert!(t.is_dirty(screen_pt(0, 0)));

        // The base emoji should be in cell 0 with the skin tone as a grapheme
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0x23);
            assert!(unsafe { (*cell).has_grapheme() });
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
        }

        t.deinit(Global);
    }

    // test "Terminal: keypad sequence VS16"
    #[test]
    fn terminal_keypad_sequence_vs16() {
        let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

        // Enable grapheme clustering
        t.modes.set(modespkg::Mode::GraphemeCluster, true);

        // This is: "#️" (number sign with emoji presentation selector)
        t.print(0x23).unwrap(); // # Number sign (valid base)
        t.print(0xFE0F).unwrap(); // VS16 (emoji presentation selector)

        // VS16 should combine with the base character into a single grapheme cluster,
        // taking 2 cells (wide character).
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);

        // Row should be dirty
        assert!(t.is_dirty(screen_pt(0, 0)));

        // The base emoji should be in cell 0 with the skin tone as a grapheme
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0x23);
            assert!(unsafe { (*cell).has_grapheme() });
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Wide);
        }

        t.deinit(Global);
    }

    // test "Terminal: print invalid VS15 following emoji is wide"
    #[test]
    fn terminal_print_invalid_vs15_following_emoji_is_wide() {
        let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

        // Enable grapheme clustering
        t.modes.set(modespkg::Mode::GraphemeCluster, true);

        t.print('\u{1F9E0}' as u32).unwrap(); // 🧠
        t.print(0xFE0E).unwrap(); // not valid with U+1F9E0 as base

        // We should have 2 cells taken up. It is one character but "wide".
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);

        // Assert various properties about our screen to verify
        // we have all expected cells.
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, '\u{1F9E0}' as u21);
            assert!(!unsafe { (*cell).has_grapheme() });
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Wide);
        }
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::SpacerTail);
        }

        t.deinit(Global);
    }

    // test "Terminal: print invalid VS15 in emoji ZWJ sequence"
    #[test]
    fn terminal_print_invalid_vs15_in_emoji_zwj_sequence() {
        let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

        // Enable grapheme clustering
        t.modes.set(modespkg::Mode::GraphemeCluster, true);

        t.print('\u{1F469}' as u32).unwrap(); // 👩
        t.print(0xFE0E).unwrap(); // not valid with U+1F469 as base
        t.print('\u{200D}' as u32).unwrap(); // ZWJ
        t.print('\u{1F466}' as u32).unwrap(); // 👦

        // We should have 2 cells taken up. It is one character but "wide".
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);

        // Assert various properties about our screen to verify
        // we have all expected cells.
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, '\u{1F469}' as u21);
            assert!(unsafe { (*cell).has_grapheme() });
            assert_eq!(
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell) }
                    .unwrap(),
                &['\u{200D}' as u21, '\u{1F466}' as u21]
            );
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Wide);
        }
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::SpacerTail);
        }

        t.deinit(Global);
    }

    // test "Terminal: print invalid VS16 grapheme"
    #[test]
    fn terminal_print_invalid_vs16_grapheme() {
        let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

        // Enable grapheme clustering
        t.modes.set(modespkg::Mode::GraphemeCluster, true);

        // https://github.com/mitchellh/ghostty/issues/1482
        t.print('x' as u32).unwrap();
        t.print(0xFE0F).unwrap(); // invalid VS16

        // We should have 1 cells taken up, and narrow.
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 1);

        // Assert various properties about our screen to verify
        // we have all expected cells.
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 'x' as u21);
            assert!(!unsafe { (*cell).has_grapheme() });
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
        }
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
        }

        t.deinit(Global);
    }

    // test "Terminal: print invalid VS16 with second char"
    #[test]
    fn terminal_print_invalid_vs16_with_second_char() {
        let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

        // Enable grapheme clustering
        t.modes.set(modespkg::Mode::GraphemeCluster, true);

        // https://github.com/mitchellh/ghostty/issues/1482
        t.print('x' as u32).unwrap();
        t.print(0xFE0F).unwrap();
        t.print('y' as u32).unwrap();

        // We should have 2 cells taken up, from two separate narrow characters.
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);

        // Assert various properties about our screen to verify
        // we have all expected cells.
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 'x' as u21);
            assert!(!unsafe { (*cell).has_grapheme() });
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
        }

        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 'y' as u21);
            assert!(!unsafe { (*cell).has_grapheme() });
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
        }

        t.deinit(Global);
    }

    // test "Terminal: print grapheme ò (o with nonspacing mark) should be narrow"
    #[test]
    fn terminal_print_grapheme_o_with_nonspacing_mark_should_be_narrow() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        // Enable grapheme clustering
        t.modes.set(modespkg::Mode::GraphemeCluster, true);

        t.print('o' as u32).unwrap();
        t.print(0x0300).unwrap(); // combining grave accent

        // We should have 1 cell taken up.
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 1);

        // Assert various properties about our screen to verify
        // we have all expected cells.
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 'o' as u21);
            assert!(unsafe { (*cell).has_grapheme() });
            assert_eq!(
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell) }
                    .unwrap(),
                &[0x0300]
            );
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
        }

        t.deinit(Global);
    }

    // test "Terminal: print Devanagari grapheme should be wide"
    #[test]
    fn terminal_print_devanagari_grapheme_should_be_wide() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        // Enable grapheme clustering
        t.modes.set(modespkg::Mode::GraphemeCluster, true);

        // क्‍ष
        t.print(0x0915).unwrap();
        t.print(0x094D).unwrap();
        t.print(0x200D).unwrap();
        t.print(0x0937).unwrap();

        // We should have 2 cells taken up. It is one character but "wide".
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);

        // Assert various properties about our screen to verify
        // we have all expected cells.
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0x0915);
            assert!(unsafe { (*cell).has_grapheme() });
            assert_eq!(
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell) }
                    .unwrap(),
                &[0x094D, 0x200D, 0x0937]
            );
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Wide);
        }
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::SpacerTail);
        }

        t.deinit(Global);
    }

    // test "Terminal: print invalid VS16 with second char (combining)"
    #[test]
    fn terminal_print_invalid_vs16_with_second_char_combining() {
        let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

        // Enable grapheme clustering
        t.modes.set(modespkg::Mode::GraphemeCluster, true);

        // https://github.com/mitchellh/ghostty/issues/1482
        t.print('n' as u32).unwrap();
        t.print(0xFE0F).unwrap(); // invalid VS16
        t.print(0x0303).unwrap(); // combining tilde

        // We should have 1 cells taken up, and narrow.
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 1);

        // Assert various properties about our screen to verify
        // we have all expected cells.
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 'n' as u21);
            assert!(unsafe { (*cell).has_grapheme() });
            assert_eq!(
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell) }
                    .unwrap(),
                &['\u{0303}' as u21]
            );
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
        }
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
        }

        t.deinit(Global);
    }

    // test "Terminal: print breaks valid grapheme cluster with Prepend + ASCII for speed"
    #[test]
    fn terminal_print_breaks_valid_grapheme_cluster_with_prepend_ascii_for_speed() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();
        t.modes.set(modespkg::Mode::GraphemeCluster, true);

        // Make sure we're not at cursor.x == 0 for the next char.
        t.print('_' as u32).unwrap();

        // U+0600 ARABIC NUMBER SIGN (Prepend)
        t.print(0x0600).unwrap();
        t.print('1' as u32).unwrap();

        // We should have 3 cells taken up, each narrow. Note that this is
        // **incorrect** grapheme break behavior, since a Prepend code point should
        // not break with the one following it per UAX #29 GB9b. However, as an
        // optimization we assume a grapheme break when c <= 255, and note that
        // this deviation only affects these very uncommon scenarios (e.g. the
        // Arabic number sign should precede Arabic-script digits).
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 3);
        // This is what we'd expect if we did break correctly:
        //try testing.expectEqual(@as(usize, 2), t.screens.active.cursor.x);

        // Assert various properties about our screen to verify
        // we have all expected cells.
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0x0600);
            assert!(!unsafe { (*cell).has_grapheme() });
            // This is what we'd expect if we did break correctly:
            //try testing.expect(cell.hasGrapheme());
            //try testing.expectEqualSlices(u21, &.{'1'}, list_cell.node.data.lookupGrapheme(cell).?);
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
        }
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(2, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, '1' as u21);
            // This is what we'd expect if we did break correctly:
            //try testing.expectEqual(@as(u21, 0), cell.content.codepoint);
            assert!(!unsafe { (*cell).has_grapheme() });
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
        }

        t.deinit(Global);
    }

    // test "Terminal: print invoke charset single"
    #[test]
    fn terminal_print_invoke_charset_single() {
        let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

        t.configure_charset(charsets::Slots::G1, charsets::Charset::DecSpecial);

        // Basic grid writing
        t.print('`' as u32).unwrap();
        t.invoke_charset(charsets::ActiveSlot::GL, charsets::Slots::G1, true);
        t.print('`' as u32).unwrap();
        t.print('`' as u32).unwrap();
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "`◆`".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: print single very long line"
    #[test]
    fn terminal_print_single_very_long_line() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        // This would crash for issue 1400. So the assertion here is
        // that we simply do not crash.
        for _ in 0..1000 {
            t.print('x' as u32).unwrap();
        }

        t.deinit(Global);
    }

    // test "Terminal: print wide char"
    #[test]
    fn terminal_print_wide_char() {
        let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

        t.print(0x1F600).unwrap(); // Smiley face
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);

        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0x1F600);
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Wide);
        }
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::SpacerTail);
        }

        assert!(t.is_dirty(screen_pt(0, 0)));

        t.deinit(Global);
    }

    // test "Terminal: print wide char at edge creates spacer head"
    #[test]
    fn terminal_print_wide_char_at_edge_creates_spacer_head() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        t.set_cursor_pos(1, 10);
        t.print(0x1F600).unwrap(); // Smiley face
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 1);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);

        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(9, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::SpacerHead);
        }
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 1)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0x1F600);
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Wide);
        }
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 1)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::SpacerTail);
        }

        // Our first row just had a spacer head added which does not affect
        // rendering so only the place where the wide char was printed
        // should be marked.
        // BUT old row is dirty because cursor moved from there
        assert!(t.is_dirty(screen_pt(0, 0)));
        assert!(t.is_dirty(screen_pt(0, 1)));

        t.deinit(Global);
    }

    // test "Terminal: print wide char with 1-column width"
    #[test]
    fn terminal_print_wide_char_with_1_column_width() {
        let mut t = Terminal::init(Global, opts(1, 2)).unwrap();

        t.print('😀' as u32).unwrap(); // 0x1F600

        // This prints a space so we should be dirty.
        assert!(t.is_dirty(screen_pt(0, 0)));

        t.deinit(Global);
    }

    // test "Terminal: print wide char in single-width terminal"
    #[test]
    fn terminal_print_wide_char_in_single_width_terminal() {
        let mut t = Terminal::init(Global, opts(1, 80)).unwrap();

        t.print(0x1F600).unwrap(); // Smiley face
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 0);
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });

        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
        }

        assert!(t.is_dirty(screen_pt(0, 0)));

        t.deinit(Global);
    }

    // test "Terminal: print over wide char at 0,0"
    #[test]
    fn terminal_print_over_wide_char_at_0_0() {
        let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

        t.print(0x1F600).unwrap(); // Smiley face
        t.set_cursor_pos(0, 0);
        t.print('A' as u32).unwrap();

        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 1);

        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 'A' as u21);
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
        }
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
        }

        assert!(t.is_dirty(screen_pt(0, 0)));
        assert!(!t.is_dirty(screen_pt(0, 1)));

        t.deinit(Global);
    }

    // test "Terminal: print over wide char at col 0 corrupts previous row"
    #[test]
    fn terminal_print_over_wide_char_at_col_0_corrupts_previous_row() {
        // Crash found by AFL++ fuzzer (afl-out/stream/default/crashes/id:000002).
        //
        // printCell, when overwriting a wide cell with a narrow cell at x<=1
        // and y>0, sets the last cell of the previous row to .narrow — even
        // when that cell is a .spacer_tail rather than a .spacer_head. This
        // orphans the .wide cell at cols-2.
        let mut t = Terminal::init(Global, opts(10, 3)).unwrap();

        // Fill rows 0 and 1 with wide chars (5 per row on a 10-col terminal).
        for _ in 0..10 {
            t.print(0x4E2D).unwrap();
        }

        // Move cursor to row 1, col 0 (on top of a wide char) and print a
        // narrow character. This triggers printCell's .wide branch which
        // corrupts row 0's last cell: col 9 changes from .spacer_tail to
        // .narrow, orphaning the .wide at col 8.
        t.set_cursor_pos(2, 1);
        t.print('A' as u32).unwrap();

        // Row 1, col 0 should be narrow (we just overwrote the wide char).
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 1)).unwrap();
            assert_eq!(unsafe { (*list_cell.cell).wide() }, pagepkg::Wide::Narrow);
        }
        // Row 0, col 8 should still be .wide (the last wide char on the row).
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(8, 0)).unwrap();
            assert_eq!(unsafe { (*list_cell.cell).wide() }, pagepkg::Wide::Wide);
        }
        // Row 0, col 9 must remain .spacer_tail to pair with the .wide at col 8.
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(9, 0)).unwrap();
            assert_eq!(
                unsafe { (*list_cell.cell).wide() },
                pagepkg::Wide::SpacerTail
            );
        }

        t.deinit(Global);
    }

    // test "Terminal: print over wide spacer tail"
    #[test]
    fn terminal_print_over_wide_spacer_tail() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print('橋' as u32).unwrap();
        t.set_cursor_pos(1, 2);
        t.print('X' as u32).unwrap();

        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
        }
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 'X' as u21);
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
        }

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), " X".as_bytes());
        }

        assert!(t.is_dirty(screen_pt(0, 0)));

        t.deinit(Global);
    }

    // test "Terminal: print over wide char with bold"
    #[test]
    fn terminal_print_over_wide_char_with_bold() {
        let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

        t.set_attribute(sgr::Attribute::Bold).unwrap();
        t.print(0x1F600).unwrap(); // Smiley face
                                   // verify we have styles in our style map
        {
            let page =
                unsafe { crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node) };
            assert_eq!(1usize, unsafe { (*page).styles.count() });
        }

        // Go back and overwrite with no style
        t.set_cursor_pos(0, 0);
        t.set_attribute(sgr::Attribute::Unset).unwrap();
        t.print('A' as u32).unwrap(); // Smiley face

        // verify our style is gone
        {
            let page =
                unsafe { crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node) };
            assert_eq!(0usize, unsafe { (*page).styles.count() });
        }

        assert!(t.is_dirty(screen_pt(0, 0)));

        t.deinit(Global);
    }

    // test "Terminal: print over wide char with bg color"
    #[test]
    fn terminal_print_over_wide_char_with_bg_color() {
        let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

        t.set_attribute(sgr::Attribute::DirectColorBg(color::RGB {
            r: 0xFF,
            g: 0,
            b: 0,
        }))
        .unwrap();
        t.print(0x1F600).unwrap(); // Smiley face
                                   // verify we have styles in our style map
        {
            let page =
                unsafe { crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node) };
            assert_eq!(1usize, unsafe { (*page).styles.count() });
        }

        // Go back and overwrite with no style
        t.set_cursor_pos(0, 0);
        t.set_attribute(sgr::Attribute::Unset).unwrap();
        t.print('A' as u32).unwrap(); // Smiley face

        // verify our style is gone
        {
            let page =
                unsafe { crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node) };
            assert_eq!(0usize, unsafe { (*page).styles.count() });
        }

        assert!(t.is_dirty(screen_pt(0, 0)));

        t.deinit(Global);
    }

    // test "Terminal: print multicodepoint grapheme, disabled mode 2027"
    #[test]
    fn terminal_print_multicodepoint_grapheme_disabled_mode_2027() {
        let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

        // https://github.com/mitchellh/ghostty/issues/289
        // This is: 👨‍👩‍👧 (which may or may not render correctly)
        t.print(0x1F468).unwrap();
        t.print(0x200D).unwrap();
        t.print(0x1F469).unwrap();
        t.print(0x200D).unwrap();
        t.print(0x1F467).unwrap();

        // We should have 6 cells taken up
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 6);

        // Assert various properties about our screen to verify
        // we have all expected cells.
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0x1F468);
            assert!(unsafe { (*cell).has_grapheme() });
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Wide);
            let cps =
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell) }
                    .unwrap();
            assert_eq!(cps.len(), 1);
        }
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
            assert!(!unsafe { (*cell).has_grapheme() });
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::SpacerTail);
            assert!(unsafe {
                (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell)
            }
            .is_none());
        }
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(2, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0x1F469);
            assert!(unsafe { (*cell).has_grapheme() });
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Wide);
            let cps =
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell) }
                    .unwrap();
            assert_eq!(cps.len(), 1);
        }
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(3, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
            assert!(!unsafe { (*cell).has_grapheme() });
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::SpacerTail);
            assert!(unsafe {
                (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell)
            }
            .is_none());
        }
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(4, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0x1F467);
            assert!(!unsafe { (*cell).has_grapheme() });
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Wide);
            assert!(unsafe {
                (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell)
            }
            .is_none());
        }
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(5, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
            assert!(!unsafe { (*cell).has_grapheme() });
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::SpacerTail);
            assert!(unsafe {
                (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell)
            }
            .is_none());
        }

        assert!(t.is_dirty(screen_pt(0, 0)));

        t.deinit(Global);
    }

    // test "Terminal: print multicodepoint grapheme, mode 2027"
    #[test]
    fn terminal_print_multicodepoint_grapheme_mode_2027() {
        let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

        // Enable grapheme clustering
        t.modes.set(modespkg::Mode::GraphemeCluster, true);

        // https://github.com/mitchellh/ghostty/issues/289
        // This is: 👨‍👩‍👧 (which may or may not render correctly)
        t.print(0x1F468).unwrap();
        t.print(0x200D).unwrap();
        t.print(0x1F469).unwrap();
        t.print(0x200D).unwrap();
        t.print(0x1F467).unwrap();

        // We should have 2 cells taken up. It is one character but "wide".
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);

        // Row should be dirty
        assert!(t.is_dirty(screen_pt(0, 0)));

        // Assert various properties about our screen to verify
        // we have all expected cells.
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0x1F468);
            assert!(unsafe { (*cell).has_grapheme() });
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Wide);
            let cps =
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell) }
                    .unwrap();
            assert_eq!(cps.len(), 4);
        }
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
            assert!(!unsafe { (*cell).has_grapheme() });
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::SpacerTail);
        }

        t.deinit(Global);
    }

    // test "Terminal: Fitzpatrick skin tone next valid base"
    #[test]
    fn terminal_fitzpatrick_skin_tone_next_valid_base() {
        let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

        // Enable grapheme clustering
        t.modes.set(modespkg::Mode::GraphemeCluster, true);

        // This is: "👋🏿" (waving hand with dark skin tone)
        t.print(0x1F44B).unwrap(); // 👋 Waving hand (valid base)
        t.print(0x1F3FF).unwrap(); // 🏿 Dark skin tone modifier

        // The skin tone should combine with the base emoji into a single grapheme cluster,
        // taking 2 cells (wide character).
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);

        // Row should be dirty
        assert!(t.is_dirty(screen_pt(0, 0)));

        // The base emoji should be in cell 0 with the skin tone as a grapheme
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0x1F44B);
            assert!(unsafe { (*cell).has_grapheme() });
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Wide);
        }

        t.deinit(Global);
    }

    // test "Terminal: Fitzpatrick skin tone next to non-base"
    #[test]
    fn terminal_fitzpatrick_skin_tone_next_to_non_base() {
        let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

        // Enable grapheme clustering
        t.modes.set(modespkg::Mode::GraphemeCluster, true);

        // This is: "🏿" (which may not render correctly in your editor!)
        t.print(0x22).unwrap(); // "
        t.print(0x1F3FF).unwrap(); // Dark skin tone
        t.print(0x22).unwrap(); // "

        // We should have 4 cells taken up. Importantly, the skin tone
        // should not join with the quotes.
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 4);

        // Row should be dirty
        assert!(t.is_dirty(screen_pt(0, 0)));

        // Assert various properties about our screen to verify
        // we have all expected cells.
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0x22);
            assert!(!unsafe { (*cell).has_grapheme() });
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
        }
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0x1F3FF);
            assert!(!unsafe { (*cell).has_grapheme() });
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Wide);
        }
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(3, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0x22);
            assert!(!unsafe { (*cell).has_grapheme() });
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
        }

        t.deinit(Global);
    }

    // test "Terminal: VS15 to make narrow character"
    #[test]
    fn terminal_vs15_to_make_narrow_character() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        // Enable grapheme clustering
        t.modes.set(modespkg::Mode::GraphemeCluster, true);

        t.print(0x2614).unwrap(); // Umbrella with rain drops, width=2
        assert!(t.is_dirty(screen_pt(0, 0)));
        t.clear_dirty();

        // We should have 2 cells taken up. It is one character but "wide".
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);

        t.print(0xFE0E).unwrap(); // VS15 to make narrow
        assert!(t.is_dirty(screen_pt(0, 0)));
        t.clear_dirty();

        // VS15 should send us back a cell since our char is no longer wide.
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 1);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "☔︎".as_bytes());
        }

        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0x2614);
            assert!(unsafe { (*cell).has_grapheme() });
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
            let cps =
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell) }
                    .unwrap();
            assert_eq!(cps.len(), 1);
        }

        t.deinit(Global);
    }

    // test "Terminal: VS15 on already narrow emoji"
    #[test]
    fn terminal_vs15_on_already_narrow_emoji() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        // Enable grapheme clustering
        t.modes.set(modespkg::Mode::GraphemeCluster, true);

        t.print(0x26C8).unwrap(); // Thunder cloud and rain, width=1
        assert!(t.is_dirty(screen_pt(0, 0)));
        t.clear_dirty();
        t.print(0xFE0E).unwrap(); // VS15 to make narrow
        assert!(t.is_dirty(screen_pt(0, 0)));
        t.clear_dirty();

        // Character takes up one cell
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 1);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "⛈︎".as_bytes());
        }

        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0x26C8);
            assert!(unsafe { (*cell).has_grapheme() });
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
            let cps =
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell) }
                    .unwrap();
            assert_eq!(cps.len(), 1);
        }

        t.deinit(Global);
    }

    // test "Terminal: VS15 to make narrow character with pending wrap"
    #[test]
    fn terminal_vs15_to_make_narrow_character_with_pending_wrap() {
        let mut t = Terminal::init(Global, opts(4, 5)).unwrap();

        // Enable grapheme clustering
        t.modes.set(modespkg::Mode::GraphemeCluster, true);

        assert!(t.modes.get(modespkg::Mode::Wraparound));

        t.print(0x1F34B).unwrap(); // Lemon, width=2
        t.print(0x2614).unwrap(); // Umbrella with rain drops, width=2
        assert!(t.is_dirty(screen_pt(0, 0)));
        t.clear_dirty();

        // We only move to the end of the line because we're in a pending wrap
        // state.
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 3);
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });

        t.print(0xFE0E).unwrap(); // VS15 to make narrow
        assert!(t.is_dirty(screen_pt(0, 0)));
        t.clear_dirty();

        // VS15 should clear the pending wrap state
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 3);
        assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "🍋☔︎".as_bytes());
        }

        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(2, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0x2614);
            assert!(unsafe { (*cell).has_grapheme() });
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
            let cps =
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell) }
                    .unwrap();
            assert_eq!(cps.len(), 1);
        }

        // VS15 should not affect the previous grapheme
        {
            let lemon_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap().cell;
            assert_eq!(unsafe { (*lemon_cell).content_codepoint() }, 0x1F34B);
            assert_eq!(unsafe { (*lemon_cell).wide() }, pagepkg::Wide::Wide);
            let spacer_cell = t.active_mut().pages.get_cell(screen_pt(1, 0)).unwrap().cell;
            assert_eq!(unsafe { (*spacer_cell).content_codepoint() }, 0);
            assert_eq!(unsafe { (*spacer_cell).wide() }, pagepkg::Wide::SpacerTail);
        }

        t.deinit(Global);
    }

    // test "Terminal: VS15 narrows wide cell under cursor with wraparound disabled"
    #[test]
    fn terminal_vs15_narrows_wide_cell_under_cursor_with_wraparound_disabled() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.modes.set(modespkg::Mode::GraphemeCluster, true);
        t.modes.set(modespkg::Mode::Wraparound, false);

        // First create a wide cell spanning columns 4 and 5.
        t.set_cursor_pos(1, 4);
        t.print(0x2614).unwrap();

        // Make column 4 the right margin and put the cursor on the wide base.
        // With wraparound disabled, grapheme lookup selects the cell under the
        // cursor when it has content.
        t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
        t.set_left_and_right_margin(1, 4);
        t.set_cursor_pos(1, 4);
        t.print(0xFE0E).unwrap();

        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 3);
        assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });
        let base = t.active_mut().pages.get_cell(screen_pt(3, 0)).unwrap().cell;
        assert_eq!(unsafe { (*base).wide() }, pagepkg::Wide::Narrow);
        assert!(unsafe { (*base).has_grapheme() });
        let tail = t.active_mut().pages.get_cell(screen_pt(4, 0)).unwrap().cell;
        assert_eq!(unsafe { (*tail).wide() }, pagepkg::Wide::Narrow);

        t.deinit(Global);
    }

    // test "Terminal: VS15 narrows wide cell under restored pending cursor"
    #[test]
    fn terminal_vs15_narrows_wide_cell_under_restored_pending_cursor() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.modes.set(modespkg::Mode::GraphemeCluster, true);
        t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
        t.set_left_and_right_margin(1, 4);

        // Save a pending-wrap cursor at column 4.
        t.set_cursor_pos(1, 4);
        t.print('X' as u32).unwrap();
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
        t.save_cursor();

        // Widen the margin and replace that cell with a wide character.
        t.set_left_and_right_margin(1, 5);
        t.set_cursor_pos(1, 4);
        t.print(0x2614).unwrap();

        // Restoring also restores pending_wrap, so grapheme lookup selects the
        // wide base under the cursor rather than its spacer tail.
        t.restore_cursor();
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
        t.print(0xFE0E).unwrap();

        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 4);
        assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });
        let base = t.active_mut().pages.get_cell(screen_pt(3, 0)).unwrap().cell;
        assert_eq!(unsafe { (*base).wide() }, pagepkg::Wide::Narrow);
        assert!(unsafe { (*base).has_grapheme() });
        let tail = t.active_mut().pages.get_cell(screen_pt(4, 0)).unwrap().cell;
        assert_eq!(unsafe { (*tail).wide() }, pagepkg::Wide::Narrow);

        t.deinit(Global);
    }

    // test "Terminal: VS16 to make wide character on next line"
    #[test]
    fn terminal_vs16_to_make_wide_character_on_next_line() {
        let mut t = Terminal::init(Global, opts(3, 5)).unwrap();

        // Enable grapheme clustering
        t.modes.set(modespkg::Mode::GraphemeCluster, true);

        t.cursor_right(2);
        t.print('#' as u32).unwrap();
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
        assert!(t.is_dirty(screen_pt(2, 0)));
        t.clear_dirty();

        t.print(0xFE0F).unwrap(); // VS16 to make wide

        assert!(t.is_dirty(screen_pt(2, 0)));
        t.clear_dirty();
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 1);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);
        assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });

        {
            // Previous cell turns into spacer_head
            let list_cell = t.active_mut().pages.get_cell(screen_pt(2, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
            assert!(!unsafe { (*cell).has_grapheme() });
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::SpacerHead);
        }
        {
            // '#' cell is wide
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 1)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, '#' as u21);
            assert!(unsafe { (*cell).has_grapheme() });
            assert_eq!(
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell) }
                    .unwrap(),
                &[0xFE0F]
            );
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Wide);
        }
        {
            // spacer_tail
            let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 1)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
            assert!(!unsafe { (*cell).has_grapheme() });
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::SpacerTail);
        }

        t.deinit(Global);
    }

    // test "Terminal: VS16 to make wide character on next line with hyperlink"
    #[test]
    fn terminal_vs16_to_make_wide_character_on_next_line_with_hyperlink() {
        // Regression test for the crash fixed in print's grapheme `.wide` path:
        // writing a spacer_head at the screen edge before row.wrap was set.
        let mut t = Terminal::init(Global, opts(3, 5)).unwrap();

        // Enable grapheme clustering and activate a hyperlink so printCell
        // calls cursorSetHyperlink (which runs page integrity checks).
        t.modes.set(modespkg::Mode::GraphemeCluster, true);
        t.active_mut()
            .start_hyperlink("http://example.com".as_bytes(), None)
            .unwrap();

        t.cursor_right(2);
        t.print('#' as u32).unwrap();
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });

        // Without the fix, this panicked with UnwrappedSpacerHead.
        t.print(0xFE0F).unwrap(); // VS16 to make wide

        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 1);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);
        assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });

        {
            // Previous cell turns into spacer_head and remains hyperlinked.
            let list_cell = t.active_mut().pages.get_cell(screen_pt(2, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::SpacerHead);
            assert!(unsafe { (*cell).hyperlink() });
            assert!(unsafe { (*list_cell.row).wrap() });
        }
        {
            // '#' cell is now wide and still hyperlinked.
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 1)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, '#' as u21);
            assert!(unsafe { (*cell).has_grapheme() });
            assert_eq!(
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell) }
                    .unwrap(),
                &[0xFE0F]
            );
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Wide);
            assert!(unsafe { (*cell).hyperlink() });
        }
        {
            // spacer_tail inherits hyperlink as part of the same grapheme cell.
            let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 1)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::SpacerTail);
            assert!(unsafe { (*cell).hyperlink() });
        }

        t.deinit(Global);
    }

    // test "Terminal: VS16 widening when the spacer tail grows the page"
    #[test]
    fn terminal_vs16_widening_when_the_spacer_tail_grows_the_page() {
        // Regression test for a stale cell pointer in print's grapheme `.wide`
        // path: writing the spacer tail can grow the page to fit the hyperlink,
        // which replaces the page and invalidates the pointer to the wide cell.
        let mut t = Terminal::init(Global, opts(20, 10)).unwrap();

        t.modes.set(modespkg::Mode::GraphemeCluster, true);
        t.active_mut()
            .start_hyperlink(b"http://example.com", None)
            .unwrap();

        // Fill the page hyperlink map until a single slot is left. The '#' below
        // takes that slot so the spacer tail is what forces the page to grow.
        loop {
            let page: *mut Page =
                unsafe { crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node) };
            let map = unsafe { (*page).hyperlink_map.map((*page).memory_ptr()) };
            if map.max_load() - map.count() == 1 {
                break;
            }
            t.print('x' as u32).unwrap();
        }

        let x = unsafe { (*t.screens.active).cursor.x };
        let y = unsafe { (*t.screens.active).cursor.y };
        t.print('#' as u32).unwrap();

        // Without the fix this crashed appending to a freed page.
        t.print(0xFE0F).unwrap();

        {
            // '#' is wide and carries the VS16 grapheme.
            let list_cell = t
                .active_mut()
                .pages
                .get_cell(active_pt(x, y as u32))
                .unwrap();
            let cell = list_cell.cell;
            assert_eq!('#' as u21, unsafe { (*cell).content_codepoint() });
            assert_eq!(pagepkg::Wide::Wide, unsafe { (*cell).wide() });
            assert!(unsafe { (*cell).has_grapheme() });
            assert_eq!(
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell) }
                    .unwrap(),
                &[0xFE0F]
            );
        }
        {
            let list_cell = t
                .active_mut()
                .pages
                .get_cell(active_pt(x + 1, y as u32))
                .unwrap();
            assert_eq!(pagepkg::Wide::SpacerTail, unsafe {
                (*list_cell.cell).wide()
            });
        }

        t.deinit(Global);
    }

    // test "Terminal: grapheme transfer when widening wraps to the next line"
    #[test]
    fn terminal_grapheme_transfer_when_widening_wraps_to_the_next_line() {
        // Covers print's grapheme `.wide` path where the previous cell already
        // holds grapheme data and has to be moved to the wrapped row.
        let mut t = Terminal::init(Global, opts(3, 5)).unwrap();

        t.modes.set(modespkg::Mode::GraphemeCluster, true);
        t.cursor_right(2);

        // A narrow emoji, then ZWJ, then a second emoji. The ZWJ attaches
        // without changing the width, so the cell has grapheme data by the time
        // the second emoji widens it.
        t.print(0x263A).unwrap();
        t.print(0x200D).unwrap();
        t.print(0x2764).unwrap();

        {
            // The old cell becomes a spacer head on the wrapped row.
            let list_cell = t.active_mut().pages.get_cell(screen_pt(2, 0)).unwrap();
            assert_eq!(pagepkg::Wide::SpacerHead, unsafe {
                (*list_cell.cell).wide()
            });
            assert!(unsafe { (*list_cell.row).wrap() });
        }
        {
            // The grapheme moved with the base codepoint.
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 1)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(0x263A as u21, unsafe { (*cell).content_codepoint() });
            assert_eq!(pagepkg::Wide::Wide, unsafe { (*cell).wide() });
            assert_eq!(
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell) }
                    .unwrap(),
                &[0x200D, 0x2764]
            );
        }
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 1)).unwrap();
            assert_eq!(pagepkg::Wide::SpacerTail, unsafe {
                (*list_cell.cell).wide()
            });
        }

        t.deinit(Global);
    }

    // test "Terminal: VS16 to make wide character with pending wrap"
    #[test]
    fn terminal_vs16_to_make_wide_character_with_pending_wrap() {
        let mut t = Terminal::init(Global, opts(3, 5)).unwrap();

        // Enable grapheme clustering
        t.modes.set(modespkg::Mode::GraphemeCluster, true);

        t.cursor_right(1);
        t.print('#' as u32).unwrap();
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);
        assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });

        t.print(0xFE0F).unwrap(); // VS16 to make wide

        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });

        {
            // '#' cell is wide
            let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, '#' as u21);
            assert!(unsafe { (*cell).has_grapheme() });
            assert_eq!(
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell) }
                    .unwrap(),
                &[0xFE0F]
            );
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Wide);
        }
        {
            // spacer_tail
            let list_cell = t.active_mut().pages.get_cell(screen_pt(2, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
            assert!(!unsafe { (*cell).has_grapheme() });
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::SpacerTail);
        }

        t.deinit(Global);
    }

    // test "Terminal: VS16 to make wide character with mode 2027"
    #[test]
    fn terminal_vs16_to_make_wide_character_with_mode_2027() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        // Enable grapheme clustering
        t.modes.set(modespkg::Mode::GraphemeCluster, true);

        t.print(0x2764).unwrap(); // Heart
        assert!(t.is_dirty(screen_pt(0, 0)));
        t.clear_dirty();
        t.print(0xFE0F).unwrap(); // VS16 to make wide
        assert!(t.is_dirty(screen_pt(0, 0)));
        t.clear_dirty();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "❤️".as_bytes());
        }

        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0x2764);
            assert!(unsafe { (*cell).has_grapheme() });
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Wide);
            let cps =
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell) }
                    .unwrap();
            assert_eq!(cps.len(), 1);
        }

        t.deinit(Global);
    }

    // test "Terminal: VS16 repeated with mode 2027"
    #[test]
    fn terminal_vs16_repeated_with_mode_2027() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        // Enable grapheme clustering
        t.modes.set(modespkg::Mode::GraphemeCluster, true);

        t.print(0x2764).unwrap(); // Heart
        t.print(0xFE0F).unwrap(); // VS16 to make wide
        t.print(0x2764).unwrap(); // Heart
        t.print(0xFE0F).unwrap(); // VS16 to make wide

        assert!(t.is_dirty(screen_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "❤️❤️".as_bytes());
        }

        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0x2764);
            assert!(unsafe { (*cell).has_grapheme() });
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Wide);
            let cps =
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell) }
                    .unwrap();
            assert_eq!(cps.len(), 1);
        }
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(2, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0x2764);
            assert!(unsafe { (*cell).has_grapheme() });
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Wide);
            let cps =
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell) }
                    .unwrap();
            assert_eq!(cps.len(), 1);
        }

        t.deinit(Global);
    }

    // test "Terminal: print Devanagari grapheme should be wide on next line"
    #[test]
    fn terminal_print_devanagari_grapheme_should_be_wide_on_next_line() {
        let mut t = Terminal::init(Global, opts(3, 5)).unwrap();

        // Enable grapheme clustering
        t.modes.set(modespkg::Mode::GraphemeCluster, true);

        t.cursor_right(2);

        // क्‍ष
        t.print(0x0915).unwrap();
        t.print(0x094D).unwrap();
        t.print(0x200D).unwrap();
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });

        // This one increases the width to wide
        t.print(0x0937).unwrap();

        // We should have 2 cells taken up. It is one character but "wide".
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 1);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);
        assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });

        {
            // Previous cell turns into spacer_head
            let list_cell = t.active_mut().pages.get_cell(screen_pt(2, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
            assert!(!unsafe { (*cell).has_grapheme() });
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::SpacerHead);
        }
        {
            // Devanagari grapheme is wide
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 1)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0x0915);
            assert!(unsafe { (*cell).has_grapheme() });
            assert_eq!(
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell) }
                    .unwrap(),
                &[0x094D, 0x200D, 0x0937]
            );
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Wide);
        }
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 1)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::SpacerTail);
        }

        t.deinit(Global);
    }

    // test "Terminal: print Devanagari grapheme should be wide on next page"
    #[test]
    fn terminal_print_devanagari_grapheme_should_be_wide_on_next_page() {
        let rows = pagepkg::std_capacity().rows;
        let cols = pagepkg::std_capacity().cols;
        let mut t = Terminal::init(Global, opts(cols, rows)).unwrap();

        // Enable grapheme clustering
        t.modes.set(modespkg::Mode::GraphemeCluster, true);

        t.cursor_down((rows - 1) as usize);

        let page_cap_rows =
            unsafe { crate::pagelist::Node::capacity(t.active_mut().pages.pages.first).rows };
        for _ in rows..page_cap_rows {
            t.index().unwrap();
        }

        t.cursor_right((cols - 1) as usize);

        assert_eq!(unsafe { (*t.screens.active).cursor.x }, cols - 1);
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, rows - 1);

        // क्‍ष
        t.print(0x0915).unwrap();
        t.print(0x094D).unwrap();
        t.print(0x200D).unwrap();
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, cols - 1);
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });

        // This one increases the width to wide
        t.print(0x0937).unwrap();

        // We should have 2 cells taken up. It is one character but "wide".
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, rows - 1);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);
        assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });

        {
            // Previous cell turns into spacer_head
            let list_cell = t
                .active_mut()
                .pages
                .get_cell(active_pt(cols - 1, (rows - 2) as u32))
                .unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
            assert!(!unsafe { (*cell).has_grapheme() });
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::SpacerHead);
        }
        {
            // Devanagari grapheme is wide
            let list_cell = t
                .active_mut()
                .pages
                .get_cell(active_pt(0, (rows - 1) as u32))
                .unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0x0915);
            assert!(unsafe { (*cell).has_grapheme() });
            assert_eq!(
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_grapheme(cell) }
                    .unwrap(),
                &[0x094D, 0x200D, 0x0937]
            );
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Wide);
        }
        {
            let list_cell = t
                .active_mut()
                .pages
                .get_cell(active_pt(1, (rows - 1) as u32))
                .unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::SpacerTail);
        }

        t.deinit(Global);
    }

    // test "Terminal: overwrite grapheme should clear grapheme data"
    #[test]
    fn terminal_overwrite_grapheme_should_clear_grapheme_data() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        // Enable grapheme clustering
        t.modes.set(modespkg::Mode::GraphemeCluster, true);

        t.print(0x26C8).unwrap(); // Thunder cloud and rain
        t.print(0xFE0E).unwrap(); // VS15 to make narrow
        assert!(t.is_dirty(screen_pt(0, 0)));
        t.clear_dirty();

        t.set_cursor_pos(1, 1);
        t.print('A' as u32).unwrap();
        assert!(t.is_dirty(screen_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "A".as_bytes());
        }

        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 'A' as u21);
            assert!(!unsafe { (*cell).has_grapheme() });
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
        }

        t.deinit(Global);
    }

    // test "Terminal: overwrite multicodepoint grapheme clears grapheme data"
    #[test]
    fn terminal_overwrite_multicodepoint_grapheme_clears_grapheme_data() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        // Enable grapheme clustering
        t.modes.set(modespkg::Mode::GraphemeCluster, true);

        // https://github.com/mitchellh/ghostty/issues/289
        // This is: 👨‍👩‍👧 (which may or may not render correctly)
        t.print(0x1F468).unwrap();
        t.print(0x200D).unwrap();
        t.print(0x1F469).unwrap();
        t.print(0x200D).unwrap();
        t.print(0x1F467).unwrap();

        // We should have 2 cells taken up. It is one character but "wide".
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);

        // We should have one cell with graphemes
        let page =
            unsafe { crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node) };
        assert_eq!(unsafe { (*page).grapheme_count() }, 1);

        // Move back and overwrite wide
        t.set_cursor_pos(1, 1);
        t.clear_dirty();
        t.print('X' as u32).unwrap();
        assert!(t.is_dirty(screen_pt(0, 0)));

        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 1);
        assert_eq!(unsafe { (*page).grapheme_count() }, 0);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "X".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: overwrite multicodepoint grapheme tail clears grapheme data"
    #[test]
    fn terminal_overwrite_multicodepoint_grapheme_tail_clears_grapheme_data() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        // Enable grapheme clustering
        t.modes.set(modespkg::Mode::GraphemeCluster, true);

        // https://github.com/mitchellh/ghostty/issues/289
        // This is: 👨‍👩‍👧 (which may or may not render correctly)
        t.print(0x1F468).unwrap();
        t.print(0x200D).unwrap();
        t.print(0x1F469).unwrap();
        t.print(0x200D).unwrap();
        t.print(0x1F467).unwrap();

        // We should have 2 cells taken up. It is one character but "wide".
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);

        // We should have one cell with graphemes
        let page =
            unsafe { crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node) };
        assert_eq!(unsafe { (*page).grapheme_count() }, 1);

        // Move back and overwrite wide
        t.set_cursor_pos(1, 2);
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), " X".as_bytes());
        }

        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);
        assert_eq!(unsafe { (*page).grapheme_count() }, 0);

        t.deinit(Global);
    }

    // test "Terminal: print writes to bottom if scrolled"
    #[test]
    fn terminal_print_writes_to_bottom_if_scrolled() {
        let mut t = Terminal::init(Global, opts(5, 2)).unwrap();

        // Basic grid writing
        for c in "hello".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(0, 0);

        // Make newlines so we create scrollback
        // 3 pushes hello off the screen
        t.index().unwrap();
        t.index().unwrap();
        t.index().unwrap();
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "".as_bytes());
        }

        // Scroll to the top
        t.active_mut().scroll(crate::screen::Scroll::Top);
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "hello".as_bytes());
        }

        // Type
        t.print('A' as u32).unwrap();
        t.active_mut().scroll(crate::screen::Scroll::Active);
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\nA".as_bytes());
        }

        assert!(
            t.is_dirty(active_pt(unsafe { (*t.screens.active).cursor.x }, unsafe {
                (*t.screens.active).cursor.y
            }
                as u32))
        );

        t.deinit(Global);
    }

    // test "Terminal: scroll_viewport row" — engine-level mirror of the C API
    // scroll_viewport-row KAT (oracle 3a2e28329).
    #[test]
    fn terminal_scroll_viewport_row() {
        let mut t = Terminal::init(Global, opts(5, 2)).unwrap();

        // Write 4 rows so rows "1" and "2" are pushed into scrollback:
        // total rows is 4, viewport length is 2. (The oracle drives this via
        // vt_write "1\r\n2\r\n3\r\n4"; here we invoke the engine primitives
        // directly since print_string does not process control bytes.)
        for (i, ch) in ['1', '2', '3', '4'].iter().enumerate() {
            t.print(*ch as u32).unwrap();
            if i != 3 {
                t.carriage_return();
                t.linefeed().unwrap();
            }
        }

        // Row 0 is the top of the scrollback.
        t.scroll_viewport(ScrollViewport::Row(0));
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "1\n2".as_bytes());
        }

        // An absolute row within the scrollback becomes the first visible row.
        t.scroll_viewport(ScrollViewport::Row(1));
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "2\n3".as_bytes());
        }

        // A row past the end clamps to the active area.
        t.scroll_viewport(ScrollViewport::Row(9999));
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "3\n4".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: print charset"
    #[test]
    fn terminal_print_charset() {
        let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

        // G1 should have no effect
        t.configure_charset(charsets::Slots::G1, charsets::Charset::DecSpecial);
        t.configure_charset(charsets::Slots::G2, charsets::Charset::DecSpecial);
        t.configure_charset(charsets::Slots::G3, charsets::Charset::DecSpecial);

        // No dirty to configure charset
        assert!(!t.is_dirty(screen_pt(0, 0)));

        // Basic grid writing
        t.print('`' as u32).unwrap();
        t.configure_charset(charsets::Slots::G0, charsets::Charset::Utf8);
        t.print('`' as u32).unwrap();
        t.configure_charset(charsets::Slots::G0, charsets::Charset::Ascii);
        t.print('`' as u32).unwrap();
        t.configure_charset(charsets::Slots::G0, charsets::Charset::DecSpecial);
        t.print('`' as u32).unwrap();
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "```◆".as_bytes());
        }

        assert!(t.is_dirty(screen_pt(0, 0)));

        t.deinit(Global);
    }

    // test "Terminal: print charset outside of ASCII"
    #[test]
    fn terminal_print_charset_outside_of_ascii() {
        let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

        // G1 should have no effect
        t.configure_charset(charsets::Slots::G1, charsets::Charset::DecSpecial);
        t.configure_charset(charsets::Slots::G2, charsets::Charset::DecSpecial);
        t.configure_charset(charsets::Slots::G3, charsets::Charset::DecSpecial);

        // No dirty to configure charset
        assert!(!t.is_dirty(screen_pt(0, 0)));

        // Basic grid writing
        t.configure_charset(charsets::Slots::G0, charsets::Charset::DecSpecial);
        t.print('`' as u32).unwrap();
        t.print(0x1F600).unwrap();
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "◆ ".as_bytes());
        }

        assert!(t.is_dirty(screen_pt(0, 0)));

        t.deinit(Global);
    }

    // test "Terminal: print invoke charset"
    #[test]
    fn terminal_print_invoke_charset() {
        let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

        t.configure_charset(charsets::Slots::G1, charsets::Charset::DecSpecial);

        t.print('`' as u32).unwrap();

        // Invokecharset but should not mark dirty on its own
        t.clear_dirty();
        t.invoke_charset(charsets::ActiveSlot::GL, charsets::Slots::G1, false);
        assert!(!t.is_dirty(screen_pt(0, 0)));
        t.print('`' as u32).unwrap();
        assert!(t.is_dirty(screen_pt(0, 0)));
        t.print('`' as u32).unwrap();
        t.invoke_charset(charsets::ActiveSlot::GL, charsets::Slots::G0, false);
        t.print('`' as u32).unwrap();
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "`◆◆`".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: print kitty unicode placeholder"
    #[test]
    #[cfg_attr(
        not(all(feature = "kitty-graphics", not(target_os = "none"))),
        ignore = "SkipZigTest: needs options::KITTY_GRAPHICS (R14)"
    )]
    fn terminal_print_kitty_unicode_placeholder() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        t.print(kitty::graphics_unicode::PLACEHOLDER).unwrap();
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 1);

        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(
                unsafe { (*cell).content_codepoint() },
                kitty::graphics_unicode::PLACEHOLDER
            );
            assert!(unsafe { (*list_cell.row).kitty_virtual_placeholder() });
        }

        assert!(t.is_dirty(screen_pt(0, 0)));

        t.deinit(Global);
    }

    /// PORT NOTE: NOT a reference test. The reference's `Screen.clearCells`
    /// kitty block (`Screen.zig:1818-1828`) was carried only as a comment, and
    /// nothing in either tree exercised it. It clears
    /// `row.kitty_virtual_placeholder` on a FULL-row clear only when no cell in
    /// the row still carries the placeholder codepoint (the Zig `for ... else`
    /// runs its `else` when the loop did not `break`), so both arms are pinned
    /// here.
    #[test]
    #[cfg_attr(
        not(all(feature = "kitty-graphics", not(target_os = "none"))),
        ignore = "SkipZigTest: needs options::KITTY_GRAPHICS (R14)"
    )]
    fn terminal_erase_line_clears_the_kitty_virtual_placeholder_flag() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();
        let flag = |t: &mut Terminal| -> bool {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
            unsafe { (*list_cell.row).kitty_virtual_placeholder() }
        };

        // Printing the placeholder marks the row.
        t.print(kitty::graphics_unicode::PLACEHOLDER).unwrap();
        assert!(flag(&mut t));

        // Overwriting the only placeholder cell does NOT clear the flag (the
        // reference never clears it on print either), so the row is now marked
        // but holds no placeholder.
        t.set_cursor_pos(1, 1);
        t.print('A' as u32).unwrap();
        assert!(flag(&mut t));

        // A full-row clear with no placeholder left in the row clears the flag.
        t.set_cursor_pos(1, 1);
        t.erase_line(csi::EraseLine::Complete, false);
        assert!(!flag(&mut t));

        // The `break` arm: a row that still holds a placeholder keeps the flag,
        // because the loop breaks before the `else`.
        t.set_cursor_pos(1, 1);
        t.print(kitty::graphics_unicode::PLACEHOLDER).unwrap();
        assert!(flag(&mut t));
        t.set_cursor_pos(1, 1);
        t.erase_line(csi::EraseLine::Complete, false);
        assert!(flag(&mut t));

        t.deinit(Global);
    }

    // test "Terminal: soft wrap"
    #[test]
    fn terminal_soft_wrap() {
        let mut t = Terminal::init(Global, opts(3, 80)).unwrap();

        // Basic grid writing
        for c in "hello".chars() {
            t.print(c as u32).unwrap();
        }
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 1);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "hel\nlo".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: soft wrap with semantic prompt"
    #[test]
    fn terminal_soft_wrap_with_semantic_prompt() {
        let mut t = Terminal::init(Global, opts(3, 80)).unwrap();

        // Mark our prompt.
        t.semantic_prompt(osc::SemanticPrompt::init(
            osc::semantic_prompt::Action::PromptStart,
        ))
        .unwrap();
        // Should not make anything dirty on its own.
        assert!(!t.is_dirty(screen_pt(0, 0)));

        // Write and wrap
        for c in "hello".chars() {
            t.print(c as u32).unwrap();
        }
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
            assert_eq!(
                unsafe { (*list_cell.row).semantic_prompt() },
                pagepkg::SemanticPrompt::Prompt
            );
        }
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 1)).unwrap();
            assert_eq!(
                unsafe { (*list_cell.row).semantic_prompt() },
                pagepkg::SemanticPrompt::PromptContinuation
            );
        }

        t.deinit(Global);
    }

    // (no reference test) PORT-ADDITIVE proving test (W29-B02): invalid UTF-8
    // must surface as `PrintStringError::InvalidUtf8` — the Zig
    // `try std.unicode.Utf8View.init` error — not `OutOfMemory`, and must
    // print nothing (validation happens before any codepoint is printed).
    #[test]
    fn terminal_print_string_invalid_utf8() {
        let mut t = Terminal::init(Global, opts(20, 5)).unwrap();

        assert_eq!(
            t.print_string(b"foo\xFFbar"),
            Err(PrintStringError::InvalidUtf8)
        );

        // Nothing was printed: validation fails before iteration begins.
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "".as_bytes());
        }

        // Valid input still prints (regression arm).
        t.print_string(b"hello").unwrap();
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "hello".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: disabled wraparound with wide char and one space"
    #[test]
    fn terminal_disabled_wraparound_with_wide_char_and_one_space() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.modes.set(modespkg::Mode::Wraparound, false);

        // This puts our cursor at the end and there is NO SPACE for a
        // wide character.
        t.print_string("AAAA".as_bytes()).unwrap();
        t.clear_dirty();
        t.print(0x1F6A8).unwrap(); // Police car light
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 4);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "AAAA".as_bytes());
        }

        // Make sure we printed nothing
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(4, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
        }

        // Should not be dirty since we didn't modify anything
        assert!(!t.is_dirty(screen_pt(0, 0)));

        t.deinit(Global);
    }

    // test "Terminal: disabled wraparound with wide char and no space"
    #[test]
    fn terminal_disabled_wraparound_with_wide_char_and_no_space() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.modes.set(modespkg::Mode::Wraparound, false);

        // This puts our cursor at the end and there is NO SPACE for a
        // wide character.
        t.print_string("AAAAA".as_bytes()).unwrap();
        t.clear_dirty();
        t.print(0x1F6A8).unwrap(); // Police car light
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 4);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "AAAAA".as_bytes());
        }

        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(4, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 'A' as u21);
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
        }

        // Should not be dirty since we didn't modify anything
        assert!(!t.is_dirty(screen_pt(0, 0)));

        t.deinit(Global);
    }

    // test "Terminal: disabled wraparound with wide grapheme and half space"
    #[test]
    fn terminal_disabled_wraparound_with_wide_grapheme_and_half_space() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.modes.set(modespkg::Mode::GraphemeCluster, true);
        t.modes.set(modespkg::Mode::Wraparound, false);

        // This puts our cursor at the end and there is NO SPACE for a
        // wide character.
        t.print_string("AAAA".as_bytes()).unwrap();
        t.print(0x2764).unwrap(); // Heart
        t.clear_dirty();
        t.print(0xFE0F).unwrap(); // VS16 to make wide
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 4);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "AAAA❤".as_bytes());
        }

        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(4, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, '❤' as u21);
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
        }

        // Should not be dirty since we didn't modify anything
        assert!(!t.is_dirty(screen_pt(0, 0)));

        t.deinit(Global);
    }

    // test "Terminal: print right margin wrap"
    #[test]
    fn terminal_print_right_margin_wrap() {
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

        t.print_string("123456789".as_bytes()).unwrap();
        t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
        t.set_left_and_right_margin(3, 5);
        t.set_cursor_pos(1, 5);
        t.print_string("XY".as_bytes()).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "1234X6789\n  Y".as_bytes());
        }

        {
            let list_cell = t.active_mut().pages.get_cell(active_pt(0, 0)).unwrap();
            let row = list_cell.row;
            assert!(!unsafe { (*row).wrap() });
        }

        t.deinit(Global);
    }

    // test "Terminal: print right margin wrap dirty tracking"
    #[test]
    fn terminal_print_right_margin_wrap_dirty_tracking() {
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

        t.print_string("123456789".as_bytes()).unwrap();
        t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
        t.set_left_and_right_margin(3, 5);
        t.set_cursor_pos(1, 5);

        // Writing our X on the first line should mark only that line dirty.
        t.clear_dirty();
        t.print('X' as u32).unwrap();
        assert!(t.is_dirty(screen_pt(4, 0)));
        assert!(!t.is_dirty(screen_pt(2, 1)));

        // Writing our Y should wrap. It marks both rows dirty because the
        // cursor moved.
        t.clear_dirty();
        t.print('Y' as u32).unwrap();
        assert!(t.is_dirty(screen_pt(4, 0)));
        assert!(t.is_dirty(screen_pt(2, 1)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "1234X6789\n  Y".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: print right margin outside"
    #[test]
    fn terminal_print_right_margin_outside() {
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

        t.print_string("123456789".as_bytes()).unwrap();
        t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
        t.set_left_and_right_margin(3, 5);
        t.set_cursor_pos(1, 6);
        t.clear_dirty();
        t.print_string("XY".as_bytes()).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "12345XY89".as_bytes());
        }

        assert!(t.is_dirty(screen_pt(5, 0)));

        t.deinit(Global);
    }

    // test "Terminal: print right margin outside wrap"
    #[test]
    fn terminal_print_right_margin_outside_wrap() {
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

        t.print_string("123456789".as_bytes()).unwrap();
        t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
        t.set_left_and_right_margin(3, 5);
        t.set_cursor_pos(1, 10);
        t.print_string("XY".as_bytes()).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "123456789X\n  Y".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: print wide char at right margin does not create spacer head"
    #[test]
    fn terminal_print_wide_char_at_right_margin_does_not_create_spacer_head() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
        t.set_left_and_right_margin(3, 5);
        t.set_cursor_pos(1, 5);
        t.print(0x1F600).unwrap(); // Smiley face
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 1);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 4);

        // Both rows dirty because the cursor moved
        assert!(t.is_dirty(screen_pt(4, 0)));
        assert!(t.is_dirty(screen_pt(4, 1)));

        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(4, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);

            let row = list_cell.row;
            assert!(!unsafe { (*row).wrap() });
        }
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(2, 1)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0x1F600);
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Wide);
        }
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(3, 1)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::SpacerTail);
        }

        t.deinit(Global);
    }

    // test "Terminal: print with hyperlink"
    #[test]
    fn terminal_print_with_hyperlink() {
        let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

        // Setup our hyperlink and print
        t.active_mut()
            .start_hyperlink("http://example.com".as_bytes(), None)
            .unwrap();
        t.print_string("123456".as_bytes()).unwrap();

        // Verify all our cells have a hyperlink
        for x in 0..6 {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(x, 0)).unwrap();
            let row = list_cell.row;
            assert!(unsafe { (*row).hyperlink() });
            let cell = list_cell.cell;
            assert!(unsafe { (*cell).hyperlink() });
            let id =
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell) }
                    .unwrap();
            assert_eq!(id, 1 as crate::hyperlink::Id);
        }

        assert!(t.is_dirty(screen_pt(0, 0)));

        t.deinit(Global);
    }

    // test "Terminal: print over cell with same hyperlink"
    #[test]
    fn terminal_print_over_cell_with_same_hyperlink() {
        let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

        // Setup our hyperlink and print
        t.active_mut()
            .start_hyperlink("http://example.com".as_bytes(), None)
            .unwrap();
        t.print_string("123456".as_bytes()).unwrap();
        t.set_cursor_pos(1, 1);
        t.print_string("123456".as_bytes()).unwrap();

        // Verify all our cells have a hyperlink
        for x in 0..6 {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(x, 0)).unwrap();
            let row = list_cell.row;
            assert!(unsafe { (*row).hyperlink() });
            let cell = list_cell.cell;
            assert!(unsafe { (*cell).hyperlink() });
            let id =
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell) }
                    .unwrap();
            assert_eq!(id, 1 as crate::hyperlink::Id);
        }

        assert!(t.is_dirty(screen_pt(0, 0)));

        t.deinit(Global);
    }

    // test "Terminal: print and end hyperlink"
    #[test]
    fn terminal_print_and_end_hyperlink() {
        let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

        // Setup our hyperlink and print
        t.active_mut()
            .start_hyperlink("http://example.com".as_bytes(), None)
            .unwrap();
        t.print_string("123".as_bytes()).unwrap();
        t.active_mut().end_hyperlink();
        t.print_string("456".as_bytes()).unwrap();

        // Verify all our cells have a hyperlink
        for x in 0..3 {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(x, 0)).unwrap();
            let row = list_cell.row;
            assert!(unsafe { (*row).hyperlink() });
            let cell = list_cell.cell;
            assert!(unsafe { (*cell).hyperlink() });
            let id =
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell) }
                    .unwrap();
            assert_eq!(id, 1 as crate::hyperlink::Id);
        }
        for x in 3..6 {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(x, 0)).unwrap();
            let row = list_cell.row;
            assert!(unsafe { (*row).hyperlink() });
            let cell = list_cell.cell;
            assert!(!unsafe { (*cell).hyperlink() });
        }

        assert!(t.is_dirty(screen_pt(0, 0)));

        t.deinit(Global);
    }

    // test "Terminal: print and change hyperlink"
    #[test]
    fn terminal_print_and_change_hyperlink() {
        let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

        // Setup our hyperlink and print
        t.active_mut()
            .start_hyperlink("http://one.example.com".as_bytes(), None)
            .unwrap();
        t.print_string("123".as_bytes()).unwrap();
        t.active_mut()
            .start_hyperlink("http://two.example.com".as_bytes(), None)
            .unwrap();
        t.print_string("456".as_bytes()).unwrap();

        // Verify all our cells have a hyperlink
        for x in 0..3 {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(x, 0)).unwrap();
            let cell = list_cell.cell;
            assert!(unsafe { (*cell).hyperlink() });
            let id =
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell) }
                    .unwrap();
            assert_eq!(id, 1 as crate::hyperlink::Id);
        }
        for x in 3..6 {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(x, 0)).unwrap();
            let cell = list_cell.cell;
            assert!(unsafe { (*cell).hyperlink() });
            let id =
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell) }
                    .unwrap();
            assert_eq!(id, 2 as crate::hyperlink::Id);
        }

        assert!(t.is_dirty(screen_pt(0, 0)));

        t.deinit(Global);
    }

    // test "Terminal: overwrite hyperlink"
    #[test]
    fn terminal_overwrite_hyperlink() {
        let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

        // Setup our hyperlink and print
        t.active_mut()
            .start_hyperlink("http://one.example.com".as_bytes(), None)
            .unwrap();
        t.print_string("123".as_bytes()).unwrap();
        t.set_cursor_pos(1, 1);
        t.active_mut().end_hyperlink();
        t.print_string("456".as_bytes()).unwrap();

        // Verify all our cells have a hyperlink
        for x in 0..3 {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(x, 0)).unwrap();
            let page = unsafe { crate::pagelist::Node::page(list_cell.node) };
            let row = list_cell.row;
            assert!(!unsafe { (*row).hyperlink() });
            let cell = list_cell.cell;
            assert!(!unsafe { (*cell).hyperlink() });
            assert!(unsafe { (*page).lookup_hyperlink(cell) }.is_none());
            assert_eq!(unsafe { (*page).hyperlink_set.count() }, 0);
        }

        assert!(t.is_dirty(screen_pt(0, 0)));

        t.deinit(Global);
    }

    // Printing a wide char at the right edge with an active hyperlink causes
    // printCell to write a spacer_head before printWrap sets the row wrap
    // flag. The integrity check inside setHyperlink (or increaseCapacity)
    // sees the unwrapped spacer head and panics. Found via fuzzing.
    // test "Terminal: print wide char at right edge with hyperlink"
    #[test]
    fn terminal_print_wide_char_at_right_edge_with_hyperlink() {
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

        t.active_mut()
            .start_hyperlink("http://example.com".as_bytes(), None)
            .unwrap();

        // Move cursor to the last column (1-indexed)
        t.set_cursor_pos(1, 10);

        // Print a wide character; this will call printCell(0, .spacer_head)
        // at the right edge before calling printWrap, triggering the
        // integrity violation.
        t.print(0x4E2D).unwrap(); // U+4E2D '中'

        // Cursor wraps to row 2, after the wide char + spacer tail
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 1);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);

        // Row 0, col 9: spacer head with hyperlink
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(9, 0)).unwrap();
            assert_eq!(
                unsafe { (*list_cell.cell).wide() },
                pagepkg::Wide::SpacerHead
            );
            assert!(unsafe { (*list_cell.cell).hyperlink() });
            assert!(unsafe { (*list_cell.row).wrap() });
        }
        // Row 1, col 0: the wide char with hyperlink
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 1)).unwrap();
            assert_eq!(unsafe { (*list_cell.cell).content_codepoint() }, 0x4E2D);
            assert_eq!(unsafe { (*list_cell.cell).wide() }, pagepkg::Wide::Wide);
            assert!(unsafe { (*list_cell.cell).hyperlink() });
        }
        // Row 1, col 1: spacer tail with hyperlink
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 1)).unwrap();
            assert_eq!(
                unsafe { (*list_cell.cell).wide() },
                pagepkg::Wide::SpacerTail
            );
            assert!(unsafe { (*list_cell.cell).hyperlink() });
        }

        t.deinit(Global);
    }

    // A cursor style or hyperlink ID is an index into a set stored in the
    // page memory of the page the cursor pin points at. When scrollClear
    // (here via ED 22, kitty's scroll_complete) pushes the active area onto
    // a later page while the cursor pin is still on an earlier one,
    // cursorReload must migrate the cursor's style and hyperlink references
    // to the destination page. It previously replaced the pin directly,
    // leaving the cursor holding an ID that was dead or aliased an unrelated
    // entry on the new page, and the next print attached a live cell to it.
    // Found via fuzzing.
    // test "Terminal: scrollClear across pages keeps cursor hyperlink refs page-local"
    #[test]
    fn terminal_scroll_clear_across_pages_keeps_cursor_hyperlink_refs_page_local() {
        // Minimized from a 774-byte AFL fuzz input. Reading it:
        //
        //   A                 print, so REP has something to repeat
        //   ESC [ 48111 b     REP, filling the page and spilling onto a second
        //   ESC ] 8 ; ; 0x93  OSC 8; the C1 byte terminates the OSC and makes
        //                     the URI non-empty, so a hyperlink starts
        //   ESC [ 11 A        CUU, moving the cursor back onto the first page
        //   ESC [ 22 J        ED 22, i.e. scroll_complete -> Screen.scrollClear
        //   B                 print, which attaches the cursor hyperlink
        //   ESC ] 8 ; ; ESC   OSC 8 with an empty URI, ending the hyperlink
        //
        // The grid must be wide enough to fill a page from a single REP, so
        // this does not reproduce at 80x24.
        let input = b"A\x1b[48111b\x1b]8;;\x93\x1b[11A\x1b[22JB\x1b]8;;\x1b";

        let mut t = Terminal::init(Global, opts(200, 50)).unwrap();

        {
            let mut s = t.vt_stream();
            s.next_slice(input);
            s.deinit();
        }

        // With slow runtime safety on, the page integrity checks during the
        // stream above already catch the bug. Verify the ref counts explicitly
        // as well so this test is meaningful with runtime safety off: every
        // cell holding a hyperlink ID owns a reference, so a count below the
        // number of holding cells means a live cell points at an entry that
        // was already freed.
        let mut node_ = t.active_mut().pages.pages.first;
        while !node_.is_null() {
            let node = node_;
            node_ = unsafe { (*node).next };

            let page: *mut Page = unsafe { crate::pagelist::Node::page(node) };
            let base = unsafe { (*page).memory_ptr() };
            let cap = unsafe { (*page).hyperlink_set.layout.cap };
            if cap == 0 {
                continue;
            }

            let mut holders = vec![0u32; cap];

            let rows = unsafe { (*page).rows.ptr(base) };
            for y in 0..(unsafe { (*page).size.rows } as usize) {
                let row: *mut Row = unsafe { rows.add(y) };
                if !unsafe { (*row).hyperlink() } {
                    continue;
                }
                let cells = unsafe { (*row).cells().ptr(base) };
                for x in 0..(unsafe { (*page).size.cols } as usize) {
                    let cell: *mut pagepkg::Cell = unsafe { cells.add(x) };
                    if !unsafe { (*cell).hyperlink() } {
                        continue;
                    }
                    let id = match unsafe { (*page).lookup_hyperlink(cell) } {
                        Some(id) => id,
                        None => continue,
                    };
                    if (id as usize) < cap {
                        holders[id as usize] += 1;
                    }
                }
            }

            for (id, &held) in holders.iter().enumerate() {
                if held == 0 {
                    continue;
                }
                let refs = unsafe {
                    (*page)
                        .hyperlink_set
                        .ref_count(base, id as crate::hyperlink::Id)
                };
                assert!(refs as u32 >= held);
            }
        }

        // If the cursor still has an active hyperlink, its own extra
        // reference must live on the cursor's page.
        let cursor_hyperlink_id = unsafe { (*t.screens.active).cursor.hyperlink_id };
        if cursor_hyperlink_id != 0 {
            let page: *mut Page =
                unsafe { crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node) };
            assert!(
                unsafe {
                    (*page)
                        .hyperlink_set
                        .ref_count((*page).memory_ptr(), cursor_hyperlink_id)
                } > 0
            );
        }

        t.deinit(Global);
    }

    // test "Terminal: default style is empty"
    #[test]
    fn terminal_default_style_is_empty() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print('A' as u32).unwrap();

        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 'A' as u21);
            assert_eq!(unsafe { (*cell).style_id() }, 0);
        }

        t.deinit(Global);
    }

    // test "Terminal: print with style marks the row as styled"
    #[test]
    fn terminal_print_with_style_marks_the_row_as_styled() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.set_attribute(sgr::Attribute::Bold).unwrap();
        t.print('A' as u32).unwrap();
        t.set_attribute(sgr::Attribute::Unset).unwrap();
        t.print('B' as u32).unwrap();

        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
            assert!(unsafe { (*list_cell.row).styled() });
        }

        t.deinit(Global);
    }

    // test "Terminal: printRepeat simple"
    #[test]
    fn terminal_print_repeat_simple() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print_string("A".as_bytes()).unwrap();
        t.print_repeat(1).unwrap();
        assert!(t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "AA".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: printRepeat wrap"
    #[test]
    fn terminal_print_repeat_wrap() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print_string("    A".as_bytes()).unwrap();
        t.print_repeat(1).unwrap();
        assert!(t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "    A\nA".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: printRepeat no previous character"
    #[test]
    fn terminal_print_repeat_no_previous_character() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print_repeat(1).unwrap();
        assert!(!t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "".as_bytes());
        }

        t.deinit(Global);
    }

    // PORT NOTE (vt-12b-2 deferral): test "Terminal: linefeed mode automatic
    // carriage return" calls `linefeed()`, which dispatches to `index()` —
    // still `todo!()` until vt-12b-3b. Deferred there with the other
    // `index()`/scroll-dependent tests.

    // test "Terminal: carriage return unsets pending wrap"
    #[test]
    fn terminal_carriage_return_unsets_pending_wrap() {
        let mut t = Terminal::init(Global, opts(5, 80)).unwrap();

        // Basic grid writing
        for c in "hello".chars() {
            t.print(c as u32).unwrap();
        }
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
        t.carriage_return();
        assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });

        t.deinit(Global);
    }

    // test "Terminal: carriage return origin mode moves to left margin"
    #[test]
    fn terminal_carriage_return_origin_mode_moves_to_left_margin() {
        let mut t = Terminal::init(Global, opts(5, 80)).unwrap();

        t.modes.set(modespkg::Mode::Origin, true);
        unsafe { (*t.screens.active).cursor.x = 0 };
        t.scrolling_region.left = 2;
        t.carriage_return();
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);

        t.deinit(Global);
    }

    // test "Terminal: carriage return left of left margin moves to zero"
    #[test]
    fn terminal_carriage_return_left_of_left_margin_moves_to_zero() {
        let mut t = Terminal::init(Global, opts(5, 80)).unwrap();

        unsafe { (*t.screens.active).cursor.x = 1 };
        t.scrolling_region.left = 2;
        t.carriage_return();
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 0);

        t.deinit(Global);
    }

    // test "Terminal: carriage return right of left margin moves to left margin"
    #[test]
    fn terminal_carriage_return_right_of_left_margin_moves_to_left_margin() {
        let mut t = Terminal::init(Global, opts(5, 80)).unwrap();

        unsafe { (*t.screens.active).cursor.x = 3 };
        t.scrolling_region.left = 2;
        t.carriage_return();
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);

        t.deinit(Global);
    }

    // test "Terminal: backspace"
    #[test]
    fn terminal_backspace() {
        let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

        // BS
        for c in "hello".chars() {
            t.print(c as u32).unwrap();
        }
        t.backspace();
        t.print('y' as u32).unwrap();
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 5);
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "helly".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: horizontal tabs"
    #[test]
    fn terminal_horizontal_tabs() {
        let mut t = Terminal::init(Global, opts(20, 5)).unwrap();

        // HT
        t.print('1' as u32).unwrap();
        t.horizontal_tab();
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 8);

        // HT
        t.horizontal_tab();
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 16);

        // HT at the end
        t.horizontal_tab();
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 19);
        t.horizontal_tab();
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 19);

        t.deinit(Global);
    }

    // test "Terminal: horizontal tabs starting on tabstop"
    #[test]
    fn terminal_horizontal_tabs_starting_on_tabstop() {
        let mut t = Terminal::init(Global, opts(20, 5)).unwrap();

        let cursor_y = unsafe { (*t.screens.active).cursor.y } as usize;
        t.set_cursor_pos(cursor_y, 9);
        t.print('X' as u32).unwrap();
        let cursor_y = unsafe { (*t.screens.active).cursor.y } as usize;
        t.set_cursor_pos(cursor_y, 9);
        t.horizontal_tab();
        t.print('A' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "        X       A".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: horizontal tabs with right margin"
    #[test]
    fn terminal_horizontal_tabs_with_right_margin() {
        let mut t = Terminal::init(Global, opts(20, 5)).unwrap();

        t.scrolling_region.left = 2;
        t.scrolling_region.right = 5;
        let cursor_y = unsafe { (*t.screens.active).cursor.y } as usize;
        t.set_cursor_pos(cursor_y, 1);
        t.print('X' as u32).unwrap();
        t.horizontal_tab();
        t.print('A' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "X    A".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: horizontal tabs back"
    #[test]
    fn terminal_horizontal_tabs_back() {
        let mut t = Terminal::init(Global, opts(20, 5)).unwrap();

        // Edge of screen
        let cursor_y = unsafe { (*t.screens.active).cursor.y } as usize;
        t.set_cursor_pos(cursor_y, 20);

        // HT
        t.horizontal_tab_back();
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 16);

        // HT
        t.horizontal_tab_back();
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 8);

        // HT
        t.horizontal_tab_back();
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 0);
        t.horizontal_tab_back();
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 0);

        t.deinit(Global);
    }

    // test "Terminal: horizontal tabs back starting on tabstop"
    #[test]
    fn terminal_horizontal_tabs_back_starting_on_tabstop() {
        let mut t = Terminal::init(Global, opts(20, 5)).unwrap();

        let cursor_y = unsafe { (*t.screens.active).cursor.y } as usize;
        t.set_cursor_pos(cursor_y, 9);
        t.print('X' as u32).unwrap();
        let cursor_y = unsafe { (*t.screens.active).cursor.y } as usize;
        t.set_cursor_pos(cursor_y, 9);
        t.horizontal_tab_back();
        t.print('A' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "A       X".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: horizontal tabs with left margin in origin mode"
    #[test]
    fn terminal_horizontal_tabs_with_left_margin_in_origin_mode() {
        let mut t = Terminal::init(Global, opts(20, 5)).unwrap();

        t.modes.set(modespkg::Mode::Origin, true);
        t.scrolling_region.left = 2;
        t.scrolling_region.right = 5;
        t.set_cursor_pos(1, 2);
        t.print('X' as u32).unwrap();
        t.horizontal_tab_back();
        t.print('A' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "  AX".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: tabClear single"
    #[test]
    fn terminal_tab_clear_single() {
        let mut t = Terminal::init(Global, opts(30, 5)).unwrap();

        t.horizontal_tab();
        t.tab_clear(csi::TabClear::Current);
        assert!(!t.is_dirty(active_pt(0, 0)));
        t.set_cursor_pos(1, 1);
        t.horizontal_tab();
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 16);

        t.deinit(Global);
    }

    // test "Terminal: tabClear all"
    #[test]
    fn terminal_tab_clear_all() {
        let mut t = Terminal::init(Global, opts(30, 5)).unwrap();

        t.tab_clear(csi::TabClear::All);
        assert!(!t.is_dirty(active_pt(0, 0)));
        t.set_cursor_pos(1, 1);
        t.horizontal_tab();
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 29);

        t.deinit(Global);
    }

    // S142-F1: `tab_clear`'s `else` arm logs `log.warn("invalid or unknown
    // tab clear setting: {}", .{cmd})` (Terminal.zig:2288-2293) for any
    // unrecognized DECST/TBC parameter.
    #[test]
    fn terminal_tab_clear_unknown_setting_logs() {
        let mut t = Terminal::init(Global, opts(30, 5)).unwrap();

        let cap = crate::log_capture::start();
        t.tab_clear(csi::TabClear::Other(99));
        let messages = cap.messages();
        assert!(
            messages
                .iter()
                .any(|(level, target, msg)| *level == log::Level::Warn
                    && target == "terminal"
                    && msg.starts_with("invalid or unknown tab clear setting: ")),
            "expected the unknown-tab-clear diagnostic, got: {messages:?}"
        );
        drop(cap);

        t.deinit(Global);
    }

    // test "Terminal: cursorPos resets wrap"
    #[test]
    fn terminal_cursor_pos_resets_wrap() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABCDE".chars() {
            t.print(c as u32).unwrap();
        }
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
        t.set_cursor_pos(1, 1);
        assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "XBCDE".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: cursorPos off the screen"
    #[test]
    fn terminal_cursor_pos_off_the_screen() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.set_cursor_pos(500, 500);
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\n\n\n\n    X".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: cursorPos relative to origin"
    #[test]
    fn terminal_cursor_pos_relative_to_origin() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.scrolling_region.top = 2;
        t.scrolling_region.bottom = 3;
        t.modes.set(modespkg::Mode::Origin, true);
        t.set_cursor_pos(1, 1);
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\n\nX".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: cursorPos relative to origin with left/right"
    #[test]
    fn terminal_cursor_pos_relative_to_origin_with_left_right() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.scrolling_region.top = 2;
        t.scrolling_region.bottom = 3;
        t.scrolling_region.left = 2;
        t.scrolling_region.right = 4;
        t.modes.set(modespkg::Mode::Origin, true);
        t.set_cursor_pos(1, 1);
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\n\n  X".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: cursorPos limits with full scroll region"
    #[test]
    fn terminal_cursor_pos_limits_with_full_scroll_region() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.scrolling_region.top = 2;
        t.scrolling_region.bottom = 3;
        t.scrolling_region.left = 2;
        t.scrolling_region.right = 4;
        t.modes.set(modespkg::Mode::Origin, true);
        t.set_cursor_pos(500, 500);
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\n\n\n    X".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: cursorUp basic"
    #[test]
    fn terminal_cursor_up_basic() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.set_cursor_pos(3, 1);
        t.print('A' as u32).unwrap();
        t.cursor_up(10);
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), " X\n\nA".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: cursorUp below top scroll margin"
    #[test]
    fn terminal_cursor_up_below_top_scroll_margin() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.set_top_and_bottom_margin(2, 4);
        t.set_cursor_pos(3, 1);
        t.print('A' as u32).unwrap();
        t.cursor_up(5);
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\n X\nA".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: cursorUp above top scroll margin"
    #[test]
    fn terminal_cursor_up_above_top_scroll_margin() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.set_top_and_bottom_margin(3, 5);
        t.set_cursor_pos(3, 1);
        t.print('A' as u32).unwrap();
        t.set_cursor_pos(2, 1);
        t.cursor_up(10);
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "X\n\nA".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: cursorUp resets wrap"
    #[test]
    fn terminal_cursor_up_resets_wrap() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABCDE".chars() {
            t.print(c as u32).unwrap();
        }
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
        t.cursor_up(1);
        assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABCDX".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: cursorLeft no wrap"
    #[test]
    fn terminal_cursor_left_no_wrap() {
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

        t.print('A' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('B' as u32).unwrap();
        t.cursor_left(10);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "A\nB".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: cursorLeft unsets pending wrap state"
    #[test]
    fn terminal_cursor_left_unsets_pending_wrap_state() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABCDE".chars() {
            t.print(c as u32).unwrap();
        }
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
        t.cursor_left(1);
        assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABCXE".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: cursorLeft unsets pending wrap state with longer jump"
    #[test]
    fn terminal_cursor_left_unsets_pending_wrap_state_with_longer_jump() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABCDE".chars() {
            t.print(c as u32).unwrap();
        }
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
        t.cursor_left(3);
        assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "AXCDE".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: cursorLeft reverse wrap with pending wrap state"
    #[test]
    fn terminal_cursor_left_reverse_wrap_with_pending_wrap_state() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.modes.set(modespkg::Mode::Wraparound, true);
        t.modes.set(modespkg::Mode::ReverseWrap, true);

        for c in "ABCDE".chars() {
            t.print(c as u32).unwrap();
        }
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
        t.cursor_left(1);
        assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABCDX".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: cursorLeft reverse wrap extended with pending wrap state"
    #[test]
    fn terminal_cursor_left_reverse_wrap_extended_with_pending_wrap_state() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.modes.set(modespkg::Mode::Wraparound, true);
        t.modes.set(modespkg::Mode::ReverseWrapExtended, true);

        for c in "ABCDE".chars() {
            t.print(c as u32).unwrap();
        }
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
        t.cursor_left(1);
        assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABCDX".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: cursorLeft reverse wrap"
    #[test]
    fn terminal_cursor_left_reverse_wrap() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.modes.set(modespkg::Mode::Wraparound, true);
        t.modes.set(modespkg::Mode::ReverseWrap, true);

        for c in "ABCDE1".chars() {
            t.print(c as u32).unwrap();
        }
        t.cursor_left(2);
        t.print('X' as u32).unwrap();
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABCDX\n1".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: cursorLeft reverse wrap with no soft wrap"
    #[test]
    fn terminal_cursor_left_reverse_wrap_with_no_soft_wrap() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.modes.set(modespkg::Mode::Wraparound, true);
        t.modes.set(modespkg::Mode::ReverseWrap, true);

        for c in "ABCDE".chars() {
            t.print(c as u32).unwrap();
        }
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('1' as u32).unwrap();
        t.cursor_left(2);
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABCDE\nX".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: cursorLeft reverse wrap before left margin"
    #[test]
    fn terminal_cursor_left_reverse_wrap_before_left_margin() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.modes.set(modespkg::Mode::Wraparound, true);
        t.modes.set(modespkg::Mode::ReverseWrap, true);
        t.set_top_and_bottom_margin(3, 0);
        t.cursor_left(1);
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\n\nX".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: cursorLeft extended reverse wrap"
    #[test]
    fn terminal_cursor_left_extended_reverse_wrap() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.modes.set(modespkg::Mode::Wraparound, true);
        t.modes.set(modespkg::Mode::ReverseWrapExtended, true);

        for c in "ABCDE".chars() {
            t.print(c as u32).unwrap();
        }
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('1' as u32).unwrap();
        t.cursor_left(2);
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABCDX\n1".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: cursorLeft extended reverse wrap bottom wraparound"
    #[test]
    fn terminal_cursor_left_extended_reverse_wrap_bottom_wraparound() {
        let mut t = Terminal::init(Global, opts(5, 3)).unwrap();

        t.modes.set(modespkg::Mode::Wraparound, true);
        t.modes.set(modespkg::Mode::ReverseWrapExtended, true);

        for c in "ABCDE".chars() {
            t.print(c as u32).unwrap();
        }
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('1' as u32).unwrap();
        t.cursor_left(1 + t.cols as usize + 1);
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABCDE\n1\n    X".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: cursorLeft extended reverse wrap is priority if both set"
    #[test]
    fn terminal_cursor_left_extended_reverse_wrap_is_priority_if_both_set() {
        let mut t = Terminal::init(Global, opts(5, 3)).unwrap();

        t.modes.set(modespkg::Mode::Wraparound, true);
        t.modes.set(modespkg::Mode::ReverseWrap, true);
        t.modes.set(modespkg::Mode::ReverseWrapExtended, true);

        for c in "ABCDE".chars() {
            t.print(c as u32).unwrap();
        }
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('1' as u32).unwrap();
        t.cursor_left(1 + t.cols as usize + 1);
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABCDE\n1\n    X".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: cursorLeft extended reverse wrap above top scroll region"
    #[test]
    fn terminal_cursor_left_extended_reverse_wrap_above_top_scroll_region() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.modes.set(modespkg::Mode::Wraparound, true);
        t.modes.set(modespkg::Mode::ReverseWrapExtended, true);

        t.set_top_and_bottom_margin(3, 0);
        t.set_cursor_pos(2, 1);
        t.cursor_left(1000);

        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);

        t.deinit(Global);
    }

    // test "Terminal: cursorLeft reverse wrap on first row"
    #[test]
    fn terminal_cursor_left_reverse_wrap_on_first_row() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.modes.set(modespkg::Mode::Wraparound, true);
        t.modes.set(modespkg::Mode::ReverseWrap, true);

        t.set_top_and_bottom_margin(3, 0);
        t.set_cursor_pos(1, 2);
        t.cursor_left(1000);

        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);

        t.deinit(Global);
    }

    // test "Terminal: cursorDown basic"
    #[test]
    fn terminal_cursor_down_basic() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print('A' as u32).unwrap();
        t.cursor_down(10);
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "A\n\n\n\n X".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: cursorDown above bottom scroll margin"
    #[test]
    fn terminal_cursor_down_above_bottom_scroll_margin() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.set_top_and_bottom_margin(1, 3);
        t.print('A' as u32).unwrap();
        t.cursor_down(10);
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "A\n\n X".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: cursorDown below bottom scroll margin"
    #[test]
    fn terminal_cursor_down_below_bottom_scroll_margin() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.set_top_and_bottom_margin(1, 3);
        t.print('A' as u32).unwrap();
        t.set_cursor_pos(4, 1);
        t.cursor_down(10);
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "A\n\n\n\nX".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: cursorDown resets wrap"
    #[test]
    fn terminal_cursor_down_resets_wrap() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABCDE".chars() {
            t.print(c as u32).unwrap();
        }
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
        t.cursor_down(1);
        assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABCDE\n    X".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: cursorRight resets wrap"
    #[test]
    fn terminal_cursor_right_resets_wrap() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABCDE".chars() {
            t.print(c as u32).unwrap();
        }
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
        t.cursor_right(1);
        assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABCDX".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: cursorRight to the edge of screen"
    #[test]
    fn terminal_cursor_right_to_the_edge_of_screen() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.cursor_right(100);
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "    X".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: cursorRight left of right margin"
    #[test]
    fn terminal_cursor_right_left_of_right_margin() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.scrolling_region.right = 2;
        t.cursor_right(100);
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "  X".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: cursorRight right of right margin"
    #[test]
    fn terminal_cursor_right_right_of_right_margin() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.scrolling_region.right = 2;
        t.set_cursor_pos(1, 4);
        t.cursor_right(100);
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "    X".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: saveCursor"
    #[test]
    fn terminal_save_cursor() {
        let mut t = Terminal::init(Global, opts(3, 3)).unwrap();

        t.set_attribute(sgr::Attribute::Bold).unwrap();
        unsafe { (*t.screens.active).charset.gr = charsets::Slots::G3 };
        t.modes.set(modespkg::Mode::Origin, true);
        t.save_cursor();
        unsafe { (*t.screens.active).charset.gr = charsets::Slots::G0 };
        t.set_attribute(sgr::Attribute::Unset).unwrap();
        t.modes.set(modespkg::Mode::Origin, false);
        t.restore_cursor();
        assert!(unsafe { (*t.screens.active).cursor.style.flags.bold });
        assert!(unsafe { (*t.screens.active).charset.gr } == charsets::Slots::G3);
        assert!(t.modes.get(modespkg::Mode::Origin));

        t.deinit(Global);
    }

    // test "Terminal: saveCursor position"
    #[test]
    fn terminal_save_cursor_position() {
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

        t.set_cursor_pos(1, 5);
        t.print('A' as u32).unwrap();
        t.save_cursor();
        t.set_cursor_pos(1, 1);
        t.print('B' as u32).unwrap();
        t.restore_cursor();
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "B   AX".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: saveCursor pending wrap state"
    #[test]
    fn terminal_save_cursor_pending_wrap_state() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.set_cursor_pos(1, 5);
        t.print('A' as u32).unwrap();
        t.save_cursor();
        t.set_cursor_pos(1, 1);
        t.print('B' as u32).unwrap();
        t.restore_cursor();
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "B   A\nX".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: saveCursor origin mode"
    #[test]
    fn terminal_save_cursor_origin_mode() {
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

        t.modes.set(modespkg::Mode::Origin, true);
        t.save_cursor();
        t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
        t.set_left_and_right_margin(3, 5);
        t.set_top_and_bottom_margin(2, 4);
        t.restore_cursor();
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "X".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: saveCursor protected pen"
    #[test]
    fn terminal_save_cursor_protected_pen() {
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

        t.set_protected_mode(ansi::ProtectedMode::Iso);
        assert!(unsafe { (*t.screens.active).cursor.protected });
        t.set_cursor_pos(1, 10);
        t.save_cursor();
        t.set_protected_mode(ansi::ProtectedMode::Off);
        assert!(!unsafe { (*t.screens.active).cursor.protected });
        t.restore_cursor();
        assert!(unsafe { (*t.screens.active).cursor.protected });

        t.deinit(Global);
    }

    // test "Terminal: saveCursor doesn't modify hyperlink state"
    #[test]
    fn terminal_save_cursor_doesn_t_modify_hyperlink_state() {
        let mut t = Terminal::init(Global, opts(3, 3)).unwrap();

        t.active_mut()
            .start_hyperlink("http://example.com".as_bytes(), None)
            .unwrap();
        let id = unsafe { (*t.screens.active).cursor.hyperlink_id };
        t.save_cursor();
        assert_eq!(id, unsafe { (*t.screens.active).cursor.hyperlink_id });
        t.restore_cursor();
        assert_eq!(id, unsafe { (*t.screens.active).cursor.hyperlink_id });

        t.deinit(Global);
    }

    // test "Terminal: restoreCursor uses default style on OutOfSpace"
    #[test]
    fn terminal_restore_cursor_uses_default_style_on_out_of_space() {
        // Tests that restoreCursor falls back to default style when
        // manualStyleUpdate fails with OutOfSpace (can't split a 1-row page
        // and styles are at max capacity).

        // Use a single row so the page can't be split
        let mut t = Terminal::init(Global, opts(10, 1)).unwrap();

        // Set a style and save the cursor
        t.set_attribute(sgr::Attribute::Bold).unwrap();
        t.save_cursor();

        // Clear the style
        t.set_attribute(sgr::Attribute::Unset).unwrap();
        assert!(!unsafe { (*t.screens.active).cursor.style.flags.bold });

        // Fill the style map to max capacity
        let max_styles = size::CellCountInt::MAX;
        loop {
            let node = unsafe { (*(*t.screens.active).cursor.page_pin).node };
            if unsafe { crate::pagelist::Node::capacity(node).styles } >= max_styles {
                break;
            }
            if t.active_mut()
                .increase_capacity(node, Some(pagelist::grow::IncreaseCapacity::Styles))
                .is_err()
            {
                break;
            }
        }

        let page: *mut pagepkg::Page =
            unsafe { crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node) };
        assert_eq!(unsafe { (*page).capacity.styles }, max_styles);

        // Fill all style slots using the StyleSet's layout capacity which accounts
        // for the load factor. The capacity in the layout is the actual max number
        // of items that can be stored.
        {
            unsafe { (*page).pause_integrity_checks(true) };

            let max_items = unsafe { (*page).styles.layout.cap };
            let mut n: usize = 1;
            while n < max_items {
                let base = unsafe { (*page).memory_ptr() };
                if unsafe {
                    (*page).styles.add(
                        base,
                        style::Style {
                            bg_color: style::Color::Rgb(color::RGB {
                                r: (n & 0xFF) as u8,
                                g: ((n >> 8) & 0xFF) as u8,
                                b: ((n >> 16) & 0xFF) as u8,
                            }),
                            ..Default::default()
                        },
                    )
                }
                .is_err()
                {
                    break;
                }
                n += 1;
            }

            unsafe { (*page).assert_integrity() };
            unsafe { (*page).pause_integrity_checks(false) };
        }

        // Restore cursor - should fall back to default style since page
        // can't be split (1 row) and styles are at max capacity
        t.restore_cursor();

        // The style should be reset to default because OutOfSpace occurred
        assert!(!unsafe { (*t.screens.active).cursor.style.flags.bold });
        assert_eq!(
            unsafe { (*t.screens.active).cursor.style_id },
            style::DEFAULT_ID
        );

        t.deinit(Global);
    }

    // === vt-12b-3a: erase methods ===========================================
    //
    // PORT NOTE (vt-12b-3a deferral): erase tests that drive still-`todo!()`
    // methods are deferred to vt-12b-3b. Specifically deferred are any test
    // that:
    //   - soft-wraps (multi-row via `print_wrap()` → `index()`), or uses
    //     `linefeed()`/`\n` in `print_string` (also `index()`),
    //   - calls `set_attribute` (SGR; the "preserves background sgr",
    //     "handles refcounted styles", "preserves color", "preserves cursor"
    //     groups),
    //   - calls `set_protected_mode` (the protected-attribute groups),
    //   - calls `set_top_and_bottom_margin` / `set_left_and_right_margin` /
    //     `scroll_down` (the decaln + scroll-region + right-margin groups).
    // The `decaln`, `erase_display` (all modes but `Scrollback`/single-row),
    // and protected `erase_line`/`erase_chars` test groups land in vt-12b-3b
    // once those methods are filled.

    // test "Terminal: eraseChars simple operation"
    #[test]
    fn terminal_erase_chars_simple_operation() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABC".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 1);
        t.clear_dirty();
        t.erase_chars(2);
        t.print('X' as u32).unwrap();

        assert!(t.is_dirty(active_pt(0, 0)));
        assert!(!t.is_dirty(active_pt(0, 1)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "X C".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseChars minimum one"
    #[test]
    fn terminal_erase_chars_minimum_one() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABC".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 1);
        t.clear_dirty();
        t.erase_chars(0);
        t.print('X' as u32).unwrap();
        assert!(t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "XBC".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseChars beyond screen edge"
    #[test]
    fn terminal_erase_chars_beyond_screen_edge() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "  ABC".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 4);
        t.erase_chars(10);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "  A".as_bytes());
        }

        t.deinit(Global);
    }

    // PORT NOTE (rustty-only regression, NOT a reference test — VALIDATION_REPORT
    // §3c.8): `count_req: usize` past u16::MAX. Zig's `@min(remaining,
    // @max(count_req, 1))` clamps in usize BEFORE the value narrows into u16
    // context; the pre-fix Rust body truncated `count_req as CellCountInt`
    // FIRST, so 65537 became 1 and erased a single cell instead of clamping to
    // `remaining` (unreachable via the VT stream, reachable via this pub API /
    // FFI). Mirrors `eraseChars beyond screen edge` with an over-u16 count.
    #[test]
    fn terminal_erase_chars_count_beyond_u16() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "  ABC".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 4);
        // u16::MAX + 2: must clamp to `remaining` (2), not truncate to 1.
        t.erase_chars(65537);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "  A".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseChars wide character"
    #[test]
    fn terminal_erase_chars_wide_character() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print('橋' as u32).unwrap();
        for c in "BC".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 1);
        t.erase_chars(1);
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "X BC".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseChars resets pending wrap"
    #[test]
    fn terminal_erase_chars_resets_pending_wrap() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABCDE".chars() {
            t.print(c as u32).unwrap();
        }
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
        t.erase_chars(1);
        assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABCDX".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseChars resets wrap"
    #[test]
    fn terminal_erase_chars_resets_wrap() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABCDE123".chars() {
            t.print(c as u32).unwrap();
        }
        {
            let list_cell = t.active_mut().pages.get_cell(active_pt(0, 0)).unwrap();
            let row = list_cell.row;
            assert!(unsafe { (*row).wrap() });
        }

        t.set_cursor_pos(1, 1);
        t.erase_chars(1);

        {
            let list_cell = t.active_mut().pages.get_cell(active_pt(0, 0)).unwrap();
            let row = list_cell.row;
            assert!(!unsafe { (*row).wrap() });
        }

        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "XBCDE\n123".as_bytes());
        }

        t.deinit(Global);
    }

    // PORT NOTE (vt-12b-3a deferral): tests "Terminal: eraseChars preserves
    // background sgr", "eraseChars handles refcounted styles", and the
    // "eraseChars protected attributes *" group drive `set_attribute` /
    // `set_protected_mode`; now ported in vt-12b-4 (see the SGR/protected
    // section at the end of this module).

    // test "Terminal: eraseChars wide char boundary conditions"
    #[test]
    fn terminal_erase_chars_wide_char_boundary_conditions() {
        let mut t = Terminal::init(Global, opts(8, 1)).unwrap();

        t.print_string("😀a😀b😀".as_bytes()).unwrap();
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "😀a😀b😀".as_bytes());
        }

        t.set_cursor_pos(1, 2);
        t.erase_chars(3);
        unsafe {
            (*crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node))
                .assert_integrity()
        };

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "     b😀".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseChars wide char splits proper cell boundaries"
    #[test]
    fn terminal_erase_chars_wide_char_splits_proper_cell_boundaries() {
        let mut t = Terminal::init(Global, opts(30, 1)).unwrap();

        // This is a test for a bug: https://github.com/ghostty-org/ghostty/issues/2817
        // To explain the setup:
        // (1) We need our wide characters starting on an even (1-based) column.
        // (2) We need our cursor to be in the middle somewhere.
        // (3) We need our count to be less than our cursor X and on a split cell.
        // The bug was that we split the wrong cell boundaries.

        t.print_string("x食べて下さい".as_bytes()).unwrap();
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "x食べて下さい".as_bytes());
        }

        t.set_cursor_pos(1, 6); // At: て
        t.erase_chars(4); // Delete: て下
        unsafe {
            (*crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node))
                .assert_integrity()
        };

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "x食べ    さい".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseChars wide char wrap boundary conditions"
    #[test]
    fn terminal_erase_chars_wide_char_wrap_boundary_conditions() {
        let mut t = Terminal::init(Global, opts(8, 3)).unwrap();

        t.print_string(".......😀abcde😀......".as_bytes()).unwrap();
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), ".......\n😀abcde\n😀......".as_bytes());

            let unwrapped = t.plain_string_unwrapped(Global).unwrap();
            assert_eq!(unwrapped.as_slice(), ".......😀abcde😀......".as_bytes());
        }

        t.set_cursor_pos(2, 2);
        t.erase_chars(3);
        unsafe {
            (*crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node))
                .assert_integrity()
        };

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), ".......\n    cde\n😀......".as_bytes());

            let unwrapped = t.plain_string_unwrapped(Global).unwrap();
            assert_eq!(unwrapped.as_slice(), ".......     cde\n😀......".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseChars clearing wrapped wide char marks spacer head row dirty"
    // (`eb722cb26`)
    //
    // NEGATIVE CONTROL: drop the `p_row.mark_dirty()` that `split_cell_boundary`
    // now issues after clearing the previous row's spacer head — the
    // `is_dirty(screen_pt(0, 0))` assertion fails (row 0 stays clean).
    #[test]
    fn terminal_erase_chars_clearing_wrapped_wide_char_marks_spacer_head_row_dirty() {
        let mut t = Terminal::init(Global, opts(5, 3)).unwrap();

        // The wide char doesn't fit so it wraps, leaving a spacer head at
        // the end of the first row.
        t.print_string("ABCD字".as_bytes()).unwrap();
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(4, 0)).unwrap();
            assert_eq!(pagepkg::Wide::SpacerHead, unsafe {
                (*list_cell.cell).wide()
            });
            assert!(unsafe { (*list_cell.row).wrap() });
        }

        t.set_cursor_pos(2, 1);
        t.clear_dirty();
        t.erase_chars(1);
        unsafe {
            (*crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node))
                .assert_integrity()
        };

        // Erasing the wide char also clears the spacer head on the previous
        // row, so that row must be dirty too.
        assert!(t.is_dirty(screen_pt(0, 0)));
        assert!(t.is_dirty(screen_pt(0, 1)));
        assert!(!t.is_dirty(screen_pt(0, 2)));

        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(4, 0)).unwrap();
            assert_eq!(pagepkg::Wide::Narrow, unsafe { (*list_cell.cell).wide() });
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteChars"
    #[test]
    fn terminal_delete_chars() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABCDE".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 2);

        t.clear_dirty();
        t.delete_chars(2);
        assert!(t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ADE".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteChars zero count"
    #[test]
    fn terminal_delete_chars_zero_count() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABCDE".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 2);

        t.clear_dirty();
        t.delete_chars(0);
        assert!(!t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABCDE".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteChars more than half"
    #[test]
    fn terminal_delete_chars_more_than_half() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABCDE".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 2);

        t.clear_dirty();
        t.delete_chars(3);
        assert!(t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "AE".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteChars more than line width"
    #[test]
    fn terminal_delete_chars_more_than_line_width() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABCDE".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 2);

        t.clear_dirty();
        t.delete_chars(10);
        assert!(t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "A".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteChars should shift left"
    #[test]
    fn terminal_delete_chars_should_shift_left() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABCDE".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 2);

        t.clear_dirty();
        t.delete_chars(1);
        assert!(t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ACDE".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteChars resets pending wrap"
    #[test]
    fn terminal_delete_chars_resets_pending_wrap() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABCDE".chars() {
            t.print(c as u32).unwrap();
        }
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
        t.delete_chars(1);
        assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABCDX".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteChars resets wrap"
    #[test]
    fn terminal_delete_chars_resets_wrap() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABCDE123".chars() {
            t.print(c as u32).unwrap();
        }
        {
            let list_cell = t.active_mut().pages.get_cell(active_pt(0, 0)).unwrap();
            let row = list_cell.row;
            assert!(unsafe { (*row).wrap() });
        }
        t.set_cursor_pos(1, 1);
        t.delete_chars(1);

        {
            let list_cell = t.active_mut().pages.get_cell(active_pt(0, 0)).unwrap();
            let row = list_cell.row;
            assert!(!unsafe { (*row).wrap() });
        }

        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "XCDE\n123".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteChars simple operation"
    #[test]
    fn terminal_delete_chars_simple_operation() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        t.print_string("ABC123".as_bytes()).unwrap();
        t.set_cursor_pos(1, 3);

        t.clear_dirty();
        t.delete_chars(2);
        assert!(t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "AB23".as_bytes());
        }

        t.deinit(Global);
    }

    // PORT NOTE (vt-12b-3a deferral): test "Terminal: deleteChars preserves
    // background sgr" drives `set_attribute`; now ported in vt-12b-4 (see the
    // SGR/protected section at the end of this module).

    // test "Terminal: deleteChars outside scroll region"
    #[test]
    fn terminal_delete_chars_outside_scroll_region() {
        let mut t = Terminal::init(Global, opts(6, 10)).unwrap();

        t.print_string("ABC123".as_bytes()).unwrap();
        t.scrolling_region.left = 2;
        t.scrolling_region.right = 4;
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
        t.clear_dirty();
        t.delete_chars(2);
        assert!(!t.is_dirty(active_pt(0, 0)));
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABC123".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteChars inside scroll region"
    #[test]
    fn terminal_delete_chars_inside_scroll_region() {
        let mut t = Terminal::init(Global, opts(6, 10)).unwrap();

        t.print_string("ABC123".as_bytes()).unwrap();
        t.scrolling_region.left = 2;
        t.scrolling_region.right = 4;
        t.set_cursor_pos(1, 4);

        t.clear_dirty();
        t.delete_chars(1);
        assert!(t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABC2 3".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteChars split wide character from spacer tail"
    #[test]
    fn terminal_delete_chars_split_wide_character_from_spacer_tail() {
        let mut t = Terminal::init(Global, opts(6, 10)).unwrap();

        t.print_string("A橋123".as_bytes()).unwrap();
        t.set_cursor_pos(1, 3);
        t.delete_chars(1);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "A 123".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteChars split wide character from wide"
    #[test]
    fn terminal_delete_chars_split_wide_character_from_wide() {
        let mut t = Terminal::init(Global, opts(6, 10)).unwrap();

        t.print_string("橋123".as_bytes()).unwrap();
        t.set_cursor_pos(1, 1);
        t.delete_chars(1);

        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
        }
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, '1' as u21);
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteChars split wide character from end"
    #[test]
    fn terminal_delete_chars_split_wide_character_from_end() {
        let mut t = Terminal::init(Global, opts(6, 10)).unwrap();

        t.print_string("A橋123".as_bytes()).unwrap();
        t.set_cursor_pos(1, 1);
        t.delete_chars(1);

        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0x6A4B);
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Wide);
        }
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::SpacerTail);
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteChars with a spacer head at the end"
    #[test]
    fn terminal_delete_chars_with_a_spacer_head_at_the_end() {
        let mut t = Terminal::init(Global, opts(5, 10)).unwrap();

        t.print_string("0123橋123".as_bytes()).unwrap();
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(4, 0)).unwrap();
            let row = list_cell.row;
            let cell = list_cell.cell;
            assert_eq!(pagepkg::Wide::SpacerHead, unsafe { (*cell).wide() });
            assert!(unsafe { (*row).wrap() });
        }

        t.set_cursor_pos(1, 1);
        t.delete_chars(1);

        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(3, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(0 as u21, unsafe { (*cell).codepoint() });
            assert_eq!(pagepkg::Wide::Narrow, unsafe { (*cell).wide() });
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteChars split wide character tail"
    #[test]
    fn terminal_delete_chars_split_wide_character_tail() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.set_cursor_pos(1, (t.cols - 1) as usize);
        t.print(0x6A4B).unwrap(); // 橋
        t.carriage_return();
        t.delete_chars((t.cols - 1) as usize);
        t.print('0' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "0".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteChars wide char boundary conditions"
    #[test]
    fn terminal_delete_chars_wide_char_boundary_conditions() {
        let mut t = Terminal::init(Global, opts(8, 1)).unwrap();

        // EXPLANATION(qwerasd):
        //
        // There are 3 or 4 boundaries to be concerned with in deleteChars,
        // depending on how you count them. Consider the following terminal:
        //
        //   +--------+
        // 0 |.ABCDEF.|
        //   : ^      : (^ = cursor)
        //   +--------+
        //
        // if we DCH 3 we get
        //
        //   +--------+
        // 0 |.DEF....|
        //   +--------+
        //
        // The boundaries exist at the following points then:
        //
        //   +--------+
        // 0 |.ABCDEF.|
        //   :11 22 33:
        //   +--------+
        //
        // I'm counting 2 for double since it's both the end of the deleted
        // content and the start of the content that is shifted in to place.
        //
        // Now consider wide characters (represented as `WW`) at these boundaries:
        //
        //   +--------+
        // 0 |WWaWWbWW|
        //   : ^      : (^ = cursor)
        //   : ^^^    : (^ = deleted by DCH 3)
        //   +--------+
        //
        // -> DCH 3
        // -> The first 2 wide characters are split & destroyed (verified in xterm)
        //
        //   +--------+
        // 0 |..bWW...|
        //   +--------+

        t.print_string("😀a😀b😀".as_bytes()).unwrap();
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "😀a😀b😀".as_bytes());
        }

        t.set_cursor_pos(1, 2);
        t.delete_chars(3);
        unsafe {
            (*crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node))
                .assert_integrity()
        };

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "  b😀".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteChars wide char wrap boundary conditions"
    #[test]
    fn terminal_delete_chars_wide_char_wrap_boundary_conditions() {
        let mut t = Terminal::init(Global, opts(8, 3)).unwrap();

        // EXPLANATION(qwerasd):
        // (cont. from "Terminal: deleteChars wide char boundary conditions")
        //
        // Additionally consider soft-wrapped wide chars (`H` = spacer head):
        //
        //   +--------+
        // 0 |.......H…
        // 1 …WWabcdeH…
        //   : ^      : (^ = cursor)
        //   : ^^^    : (^ = deleted by DCH 3)
        // 2 …WW......|
        //   +--------+
        //
        // -> DCH 3
        // -> First wide character split and destroyed, including spacer head,
        //    second spacer head removed (verified in xterm).
        // -> Wrap state of row reset
        //
        //   +--------+
        // 0 |........|
        // 1 |.cde....|
        // 2 |WW......|
        //   +--------+
        //

        t.print_string(".......😀abcde😀......".as_bytes()).unwrap();
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), ".......\n😀abcde\n😀......".as_bytes());

            let unwrapped = t.plain_string_unwrapped(Global).unwrap();
            assert_eq!(unwrapped.as_slice(), ".......😀abcde😀......".as_bytes());
        }

        t.set_cursor_pos(2, 2);
        t.clear_dirty();
        t.delete_chars(3);
        unsafe {
            (*crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node))
                .assert_integrity()
        };

        // Deleting the wide char also clears the spacer head on the previous
        // row, so that row must be dirty too.
        assert!(t.is_dirty(screen_pt(0, 0)));
        assert!(t.is_dirty(screen_pt(0, 1)));
        assert!(!t.is_dirty(screen_pt(0, 2)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), ".......\n cde\n😀......".as_bytes());

            let unwrapped = t.plain_string_unwrapped(Global).unwrap();
            assert_eq!(unwrapped.as_slice(), ".......  cde\n😀......".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteChars wide char across right margin"
    #[test]
    fn terminal_delete_chars_wide_char_across_right_margin() {
        let mut t = Terminal::init(Global, opts(8, 3)).unwrap();

        // scroll region
        //    VVVVVV
        //  +-######-+
        //  |.abcdeWW|
        //  : ^      : (^ = cursor)
        //  +--------+
        //
        // DCH 1

        t.print_string("123456橋".as_bytes()).unwrap();
        t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
        t.set_left_and_right_margin(2, 7);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "123456橋".as_bytes());
        }

        t.set_cursor_pos(1, 2);
        t.delete_chars(1);
        unsafe {
            (*crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node))
                .assert_integrity()
        };

        // NOTE: This behavior is slightly inconsistent with xterm. xterm
        // _visually_ splits the wide character (half the wide character shows
        // up in col 6 and half in col 8). In all other wide char split scenarios,
        // xterm clears the cell. Therefore, we've chosen to clear the cell here.
        // Given we have space, we also could actually preserve it, but I haven't
        // yet found a terminal that behaves that way. We should be open to
        // revisiting this behavior but for now we're going with the simpler
        // impl.
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "13456".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseLine simple erase right"
    #[test]
    fn terminal_erase_line_simple_erase_right() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABCDE".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 3);
        t.clear_dirty();
        t.erase_line(csi::EraseLine::Right, false);
        assert!(t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "AB".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseLine resets pending wrap"
    #[test]
    fn terminal_erase_line_resets_pending_wrap() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABCDE".chars() {
            t.print(c as u32).unwrap();
        }
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
        t.erase_line(csi::EraseLine::Right, false);
        assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });
        t.print('B' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABCDB".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseLine resets wrap"
    #[test]
    fn terminal_erase_line_resets_wrap() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABCDE123".chars() {
            t.print(c as u32).unwrap();
        }
        {
            let list_cell = t.active_mut().pages.get_cell(active_pt(0, 0)).unwrap();
            assert!(unsafe { (*list_cell.row).wrap() });
        }

        t.set_cursor_pos(1, 1);
        t.erase_line(csi::EraseLine::Right, false);

        {
            let list_cell = t.active_mut().pages.get_cell(active_pt(0, 0)).unwrap();
            assert!(!unsafe { (*list_cell.row).wrap() });
        }
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "X\n123".as_bytes());
        }

        t.deinit(Global);
    }

    // PORT NOTE (vt-12b-3a deferral): tests "Terminal: eraseLine right preserves
    // background sgr" and the "eraseLine right protected *" group drive
    // `set_attribute` / `set_protected_mode`; now ported in vt-12b-4 (see the
    // SGR/protected section at the end of this module).

    // S142-F1/S143-F2: `erase_line`'s catch-all arm logs `log.err("unimplemented
    // erase line mode: {}", .{mode})` (Terminal.zig:3524-3527) for any
    // `csi::EraseLine::Other` (unrecognized DECSEL/EL) parameter.
    #[test]
    fn terminal_erase_line_unimplemented_mode_logs() {
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

        let cap = crate::log_capture::start();
        t.erase_line(csi::EraseLine::Other(99), false);
        let messages = cap.messages();
        assert!(
            messages
                .iter()
                .any(|(level, target, msg)| *level == log::Level::Error
                    && target == "terminal"
                    && msg.starts_with("unimplemented erase line mode: ")),
            "expected the unimplemented-erase-line-mode diagnostic, got: {messages:?}"
        );
        drop(cap);

        t.deinit(Global);
    }

    // test "Terminal: eraseLine right wide character"
    #[test]
    fn terminal_erase_line_right_wide_character() {
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

        for c in "AB".chars() {
            t.print(c as u32).unwrap();
        }
        t.print('橋' as u32).unwrap();
        for c in "DE".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 4);
        t.clear_dirty();
        t.erase_line(csi::EraseLine::Right, false);
        assert!(t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "AB".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseLine simple erase left"
    #[test]
    fn terminal_erase_line_simple_erase_left() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABCDE".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 3);
        t.clear_dirty();
        t.erase_line(csi::EraseLine::Left, false);
        assert!(t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "   DE".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseLine left resets wrap"
    #[test]
    fn terminal_erase_line_left_resets_wrap() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABCDE".chars() {
            t.print(c as u32).unwrap();
        }
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
        t.clear_dirty();
        t.erase_line(csi::EraseLine::Left, false);
        assert!(t.is_dirty(active_pt(0, 0)));
        assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });
        t.print('B' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "    B".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseLine left wide character"
    #[test]
    fn terminal_erase_line_left_wide_character() {
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

        for c in "AB".chars() {
            t.print(c as u32).unwrap();
        }
        t.print('橋' as u32).unwrap();
        for c in "DE".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 3);
        t.clear_dirty();
        t.erase_line(csi::EraseLine::Left, false);
        assert!(t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "    DE".as_bytes());
        }

        t.deinit(Global);
    }

    // PORT NOTE (vt-12b-3a deferral): the "Terminal: eraseLine left protected
    // *", "eraseLine complete *", "eraseLine left preserves background sgr"
    // groups drive `set_attribute` / `set_protected_mode`; now ported in
    // vt-12b-4 (see the SGR/protected section at the end of this module).

    // PORT NOTE (vt-12b-3a deferral): the "Terminal: eraseDisplay *" tests that
    // drive `set_attribute` / `set_protected_mode` (SGR/protected variants), plus
    // the non-SGR eraseDisplay tests (`simple erase above`, `below/above split
    // multi-cell`, `scroll complete`) tracked with that broader group, are now
    // ported in vt-12b-4 (see the SGR/protected section at the end of this
    // module).

    // ===================================================================
    // vt-12b-3b: scroll / lines / margins tests (+ deferred linefeed tests)
    // ===================================================================

    // test "Terminal: linefeed and carriage return"
    #[test]
    fn terminal_linefeed_and_carriage_return() {
        let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

        // Print and CR.
        for c in "hello".chars() {
            t.print(c as u32).unwrap();
        }
        t.clear_dirty();
        t.carriage_return();

        // CR should not mark row dirty because it doesn't change rendering.
        assert!(!t.is_dirty(screen_pt(0, 0)));

        t.linefeed().unwrap();

        // LF marks row dirty due to cursor movement
        assert!(t.is_dirty(screen_pt(0, 0)));
        assert!(t.is_dirty(screen_pt(0, 1)));

        for c in "world".chars() {
            t.print(c as u32).unwrap();
        }
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 1);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 5);
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "hello\nworld".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: linefeed unsets pending wrap"
    #[test]
    fn terminal_linefeed_unsets_pending_wrap() {
        let mut t = Terminal::init(Global, opts(5, 80)).unwrap();

        // Basic grid writing
        for c in "hello".chars() {
            t.print(c as u32).unwrap();
        }
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
        t.clear_dirty();
        t.linefeed().unwrap();
        assert!(t.is_dirty(screen_pt(0, 0)));
        assert!(t.is_dirty(screen_pt(0, 1)));
        assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });

        t.deinit(Global);
    }

    // test "Terminal: linefeed mode automatic carriage return"
    #[test]
    fn terminal_linefeed_mode_automatic_carriage_return() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        // Basic grid writing
        t.modes.set(modespkg::Mode::Linefeed, true);
        t.print_string("123456".as_bytes()).unwrap();
        t.linefeed().unwrap();
        t.print('X' as u32).unwrap();
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "123456\nX".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: setTopAndBottomMargin simple"
    #[test]
    fn terminal_set_top_and_bottom_margin_simple() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print_string("ABC".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DEF".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GHI".as_bytes()).unwrap();
        t.set_top_and_bottom_margin(0, 0);

        t.clear_dirty();
        t.scroll_down(1);

        // Mark the rows we moved as dirty.
        assert!(t.is_dirty(active_pt(0, 0)));
        assert!(t.is_dirty(active_pt(0, 1)));
        assert!(t.is_dirty(active_pt(0, 2)));
        assert!(t.is_dirty(active_pt(0, 3)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\nABC\nDEF\nGHI".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: setTopAndBottomMargin top only"
    #[test]
    fn terminal_set_top_and_bottom_margin_top_only() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print_string("ABC".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DEF".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GHI".as_bytes()).unwrap();
        t.set_top_and_bottom_margin(2, 0);

        t.clear_dirty();
        t.scroll_down(1);

        // This is dirty because the cursor moves from this row
        assert!(t.is_dirty(active_pt(0, 0)));
        assert!(t.is_dirty(active_pt(0, 1)));
        assert!(t.is_dirty(active_pt(0, 2)));
        assert!(t.is_dirty(active_pt(0, 3)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABC\n\nDEF\nGHI".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: setTopAndBottomMargin top and bottom"
    #[test]
    fn terminal_set_top_and_bottom_margin_top_and_bottom() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print_string("ABC".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DEF".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GHI".as_bytes()).unwrap();
        t.set_top_and_bottom_margin(1, 2);

        t.clear_dirty();
        t.scroll_down(1);

        assert!(t.is_dirty(active_pt(0, 0)));
        assert!(t.is_dirty(active_pt(0, 1)));
        assert!(!t.is_dirty(active_pt(0, 2)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\nABC\nGHI".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: setTopAndBottomMargin top equal to bottom"
    #[test]
    fn terminal_set_top_and_bottom_margin_top_equal_to_bottom() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print_string("ABC".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DEF".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GHI".as_bytes()).unwrap();
        t.set_top_and_bottom_margin(2, 2);

        t.clear_dirty();
        t.scroll_down(1);

        assert!(t.is_dirty(active_pt(0, 0)));
        assert!(t.is_dirty(active_pt(0, 1)));
        assert!(t.is_dirty(active_pt(0, 2)));
        assert!(t.is_dirty(active_pt(0, 3)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\nABC\nDEF\nGHI".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: setLeftAndRightMargin simple"
    #[test]
    fn terminal_set_left_and_right_margin_simple() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print_string("ABC".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DEF".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GHI".as_bytes()).unwrap();
        t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
        t.set_left_and_right_margin(0, 0);

        t.clear_dirty();
        t.erase_chars(1);

        assert!(t.is_dirty(active_pt(0, 0)));
        assert!(!t.is_dirty(active_pt(0, 1)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), " BC\nDEF\nGHI".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: setLeftAndRightMargin left only"
    #[test]
    fn terminal_set_left_and_right_margin_left_only() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print_string("ABC".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DEF".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GHI".as_bytes()).unwrap();
        t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
        t.set_left_and_right_margin(2, 0);
        assert_eq!(t.scrolling_region.left as usize, 1);
        assert_eq!(t.scrolling_region.right as usize, (t.cols - 1) as usize);
        t.set_cursor_pos(1, 2);

        t.clear_dirty();
        t.insert_lines(1);

        assert!(t.is_dirty(active_pt(0, 0)));
        assert!(t.is_dirty(active_pt(0, 1)));
        assert!(t.is_dirty(active_pt(0, 2)));
        assert!(t.is_dirty(active_pt(0, 3)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "A\nDBC\nGEF\n HI".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: setLeftAndRightMargin left and right"
    #[test]
    fn terminal_set_left_and_right_margin_left_and_right() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print_string("ABC".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DEF".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GHI".as_bytes()).unwrap();
        t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
        t.set_left_and_right_margin(1, 2);
        t.set_cursor_pos(1, 2);

        t.clear_dirty();
        t.insert_lines(1);

        assert!(t.is_dirty(active_pt(0, 0)));
        assert!(t.is_dirty(active_pt(0, 1)));
        assert!(t.is_dirty(active_pt(0, 2)));
        assert!(t.is_dirty(active_pt(0, 3)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "  C\nABF\nDEI\nGH".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: setLeftAndRightMargin left equal right"
    #[test]
    fn terminal_set_left_and_right_margin_left_equal_right() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print_string("ABC".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DEF".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GHI".as_bytes()).unwrap();
        t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
        t.set_left_and_right_margin(2, 2);
        t.set_cursor_pos(1, 2);

        t.clear_dirty();
        t.insert_lines(1);

        assert!(t.is_dirty(active_pt(0, 0)));
        assert!(t.is_dirty(active_pt(0, 1)));
        assert!(t.is_dirty(active_pt(0, 2)));
        assert!(t.is_dirty(active_pt(0, 3)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\nABC\nDEF\nGHI".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: setLeftAndRightMargin mode 69 unset"
    #[test]
    fn terminal_set_left_and_right_margin_mode_69_unset() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print_string("ABC".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DEF".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GHI".as_bytes()).unwrap();
        t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, false);
        t.set_left_and_right_margin(1, 2);
        t.set_cursor_pos(1, 2);

        t.clear_dirty();
        t.insert_lines(1);

        assert!(t.is_dirty(active_pt(0, 0)));
        assert!(t.is_dirty(active_pt(0, 1)));
        assert!(t.is_dirty(active_pt(0, 2)));
        assert!(t.is_dirty(active_pt(0, 3)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\nABC\nDEF\nGHI".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: insertLines simple"
    #[test]
    fn terminal_insert_lines_simple() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print_string("ABC".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DEF".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GHI".as_bytes()).unwrap();
        t.set_cursor_pos(2, 2);

        // PORTS: oracle 0bad91adb — a full-width line operation moves whole
        // rows, so the affected page generations are renewed.
        let node = unsafe { (*(*t.screens.active).cursor.page_pin).node };
        let serial = unsafe { (*node).serial };
        t.clear_dirty();
        t.insert_lines(1);
        assert!(!unsafe { (*t.screens.active).pages.node_is_valid(node, serial) });

        assert!(!t.is_dirty(active_pt(0, 0)));
        assert!(t.is_dirty(active_pt(0, 1)));
        assert!(t.is_dirty(active_pt(0, 2)));
        assert!(t.is_dirty(active_pt(0, 3)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABC\n\nDEF\nGHI".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: insertLines outside of scroll region"
    #[test]
    fn terminal_insert_lines_outside_of_scroll_region() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print_string("ABC".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DEF".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GHI".as_bytes()).unwrap();
        t.set_top_and_bottom_margin(3, 4);
        t.set_cursor_pos(2, 2);

        t.clear_dirty();
        t.insert_lines(1);

        assert!(!t.is_dirty(active_pt(0, 0)));
        assert!(!t.is_dirty(active_pt(0, 1)));
        assert!(!t.is_dirty(active_pt(0, 2)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABC\nDEF\nGHI".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: insertLines top/bottom scroll region"
    #[test]
    fn terminal_insert_lines_top_bottom_scroll_region() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print_string("ABC".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DEF".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GHI".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("123".as_bytes()).unwrap();
        t.set_top_and_bottom_margin(1, 3);
        t.set_cursor_pos(2, 2);

        t.clear_dirty();
        t.insert_lines(1);

        assert!(!t.is_dirty(active_pt(0, 0)));
        assert!(t.is_dirty(active_pt(0, 1)));
        assert!(t.is_dirty(active_pt(0, 2)));
        assert!(!t.is_dirty(active_pt(0, 3)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABC\n\nDEF\n123".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: insertLines zero"
    #[test]
    fn terminal_insert_lines_zero() {
        let mut t = Terminal::init(Global, opts(2, 5)).unwrap();

        // This should do nothing
        t.set_cursor_pos(1, 1);
        t.insert_lines(0);

        t.deinit(Global);
    }

    // test "Terminal: insertLines with scroll region"
    #[test]
    fn terminal_insert_lines_with_scroll_region() {
        let mut t = Terminal::init(Global, opts(2, 6)).unwrap();

        // Initial value
        t.print('A' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('B' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('C' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('D' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('E' as u32).unwrap();

        t.set_top_and_bottom_margin(1, 2);
        t.set_cursor_pos(1, 1);

        t.clear_dirty();
        t.insert_lines(1);

        assert!(t.is_dirty(active_pt(0, 0)));
        assert!(t.is_dirty(active_pt(0, 1)));
        assert!(!t.is_dirty(active_pt(0, 2)));

        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "X\nA\nC\nD\nE".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: insertLines more than remaining"
    #[test]
    fn terminal_insert_lines_more_than_remaining() {
        let mut t = Terminal::init(Global, opts(2, 5)).unwrap();

        // Initial value
        t.print('A' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('B' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('C' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('D' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('E' as u32).unwrap();

        // Move to row 2
        t.set_cursor_pos(2, 1);

        // Insert a bunch of  lines
        t.clear_dirty();
        t.insert_lines(20);

        assert!(!t.is_dirty(active_pt(0, 0)));
        assert!(t.is_dirty(active_pt(0, 1)));
        assert!(t.is_dirty(active_pt(0, 2)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "A".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: insertLines resets pending wrap"
    #[test]
    fn terminal_insert_lines_resets_pending_wrap() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABCDE".chars() {
            t.print(c as u32).unwrap();
        }
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
        t.insert_lines(1);
        assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });
        t.print('B' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "B\nABCDE".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: insertLines resets wrap"
    #[test]
    fn terminal_insert_lines_resets_wrap() {
        let mut t = Terminal::init(Global, opts(3, 3)).unwrap();

        t.print('1' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        for c in "ABCDEF".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 1);
        t.insert_lines(1);
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "X\n1\nABC".as_bytes());
        }

        {
            let list_cell = unsafe { (*t.screens.active).pages.get_cell(active_pt(0, 2)) }.unwrap();
            let row = list_cell.row;
            assert!(!unsafe { (*row).wrap() });
        }

        t.deinit(Global);
    }

    // test "Terminal: insertLines left/right scroll region"
    #[test]
    fn terminal_insert_lines_left_right_scroll_region() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        t.print_string("ABC123".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DEF456".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GHI789".as_bytes()).unwrap();
        t.scrolling_region.left = 1;
        t.scrolling_region.right = 3;
        t.set_cursor_pos(2, 2);

        t.clear_dirty();
        t.insert_lines(1);

        assert!(!t.is_dirty(active_pt(0, 0)));
        assert!(t.is_dirty(active_pt(0, 1)));
        assert!(t.is_dirty(active_pt(0, 2)));
        assert!(t.is_dirty(active_pt(0, 3)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABC123\nD   56\nGEF489\n HI7".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: scrollUp simple"
    #[test]
    fn terminal_scroll_up_simple() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print_string("ABC".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DEF".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GHI".as_bytes()).unwrap();
        t.set_cursor_pos(2, 2);

        let cursor_x = unsafe { (*t.screens.active).cursor.x };
        let cursor_y = unsafe { (*t.screens.active).cursor.y };
        let viewport_before = t.active_mut().pages.get_top_left(point::Tag::Viewport);
        t.scroll_up(1).unwrap();
        assert_eq!(cursor_x, unsafe { (*t.screens.active).cursor.x });
        assert_eq!(cursor_y, unsafe { (*t.screens.active).cursor.y });

        // Viewport should have moved. Our entire page should've scrolled!
        // The viewport moving will cause our render state to make the full
        // frame as dirty.
        let viewport_after = t.active_mut().pages.get_top_left(point::Tag::Viewport);
        assert!(!viewport_before.eql(viewport_after));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "DEF\nGHI".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: scrollUp moves hyperlink"
    #[test]
    fn terminal_scroll_up_moves_hyperlink() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print_string("ABC".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.active_mut()
            .start_hyperlink("http://example.com".as_bytes(), None)
            .unwrap();
        t.print_string("DEF".as_bytes()).unwrap();
        t.active_mut().end_hyperlink();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GHI".as_bytes()).unwrap();
        t.set_cursor_pos(2, 2);
        t.scroll_up(1).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "DEF\nGHI".as_bytes());
        }

        for x in 0..3 {
            let list_cell = t.active_mut().pages.get_cell(viewport_pt(x, 0)).unwrap();
            let row = list_cell.row;
            assert!(unsafe { (*row).hyperlink() });
            let cell = list_cell.cell;
            assert!(unsafe { (*cell).hyperlink() });
            let id =
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell) }
                    .unwrap();
            assert_eq!(id, 1 as crate::hyperlink::Id);
            let page = unsafe { crate::pagelist::Node::page(list_cell.node) };
            assert_eq!(unsafe { (*page).hyperlink_set.count() }, 1);
        }
        for x in 0..3 {
            let list_cell = t.active_mut().pages.get_cell(viewport_pt(x, 1)).unwrap();
            let row = list_cell.row;
            assert!(!unsafe { (*row).hyperlink() });
            let cell = list_cell.cell;
            assert!(!unsafe { (*cell).hyperlink() });
            let id =
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell) };
            assert!(id.is_none());
        }

        t.deinit(Global);
    }

    // test "Terminal: scrollUp clears hyperlink"
    #[test]
    fn terminal_scroll_up_clears_hyperlink() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.active_mut()
            .start_hyperlink("http://example.com".as_bytes(), None)
            .unwrap();
        t.print_string("ABC".as_bytes()).unwrap();
        t.active_mut().end_hyperlink();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DEF".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GHI".as_bytes()).unwrap();
        t.set_cursor_pos(2, 2);
        t.scroll_up(1).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "DEF\nGHI".as_bytes());
        }

        for x in 0..3 {
            let list_cell = t.active_mut().pages.get_cell(viewport_pt(x, 0)).unwrap();
            let row = list_cell.row;
            assert!(!unsafe { (*row).hyperlink() });
            let cell = list_cell.cell;
            assert!(!unsafe { (*cell).hyperlink() });
            let id =
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell) };
            assert!(id.is_none());
        }

        t.deinit(Global);
    }

    // test "Terminal: scrollUp top/bottom scroll region"
    #[test]
    fn terminal_scroll_up_top_bottom_scroll_region() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print_string("ABC".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DEF".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GHI".as_bytes()).unwrap();
        t.set_top_and_bottom_margin(2, 3);
        t.set_cursor_pos(1, 1);

        t.clear_dirty();
        t.scroll_up(1).unwrap();

        // This is dirty because the cursor moves from this row
        assert!(t.is_dirty(active_pt(0, 0)));
        assert!(t.is_dirty(active_pt(0, 1)));
        assert!(t.is_dirty(active_pt(0, 2)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABC\nGHI".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: scrollUp left/right scroll region"
    #[test]
    fn terminal_scroll_up_left_right_scroll_region() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        t.print_string("ABC123".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DEF456".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GHI789".as_bytes()).unwrap();
        t.scrolling_region.left = 1;
        t.scrolling_region.right = 3;
        t.set_cursor_pos(2, 2);

        let cursor_x = unsafe { (*t.screens.active).cursor.x };
        let cursor_y = unsafe { (*t.screens.active).cursor.y };
        t.clear_dirty();
        t.scroll_up(1).unwrap();
        assert_eq!(cursor_x, unsafe { (*t.screens.active).cursor.x });
        assert_eq!(cursor_y, unsafe { (*t.screens.active).cursor.y });

        assert!(t.is_dirty(active_pt(0, 0)));
        assert!(t.is_dirty(active_pt(0, 1)));
        assert!(t.is_dirty(active_pt(0, 2)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "AEF423\nDHI756\nG   89".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: scrollUp left/right scroll region hyperlink"
    #[test]
    fn terminal_scroll_up_left_right_scroll_region_hyperlink() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        t.print_string("ABC123".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.active_mut()
            .start_hyperlink("http://example.com".as_bytes(), None)
            .unwrap();
        t.print_string("DEF456".as_bytes()).unwrap();
        t.active_mut().end_hyperlink();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GHI789".as_bytes()).unwrap();
        t.scrolling_region.left = 1;
        t.scrolling_region.right = 3;
        t.set_cursor_pos(2, 2);
        t.scroll_up(1).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "AEF423\nDHI756\nG   89".as_bytes());
        }

        // First row gets some hyperlinks
        {
            for x in 0..1 {
                let list_cell = t.active_mut().pages.get_cell(viewport_pt(x, 0)).unwrap();
                let cell = list_cell.cell;
                assert!(!unsafe { (*cell).hyperlink() });
                let id = unsafe {
                    (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell)
                };
                assert!(id.is_none());
            }
            for x in 1..4 {
                let list_cell = t.active_mut().pages.get_cell(viewport_pt(x, 0)).unwrap();
                let row = list_cell.row;
                assert!(unsafe { (*row).hyperlink() });
                let cell = list_cell.cell;
                assert!(unsafe { (*cell).hyperlink() });
                let id = unsafe {
                    (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell)
                }
                .unwrap();
                assert_eq!(id, 1 as crate::hyperlink::Id);
                let page = unsafe { crate::pagelist::Node::page(list_cell.node) };
                assert_eq!(unsafe { (*page).hyperlink_set.count() }, 1);
            }
            for x in 4..6 {
                let list_cell = t.active_mut().pages.get_cell(viewport_pt(x, 0)).unwrap();
                let cell = list_cell.cell;
                assert!(!unsafe { (*cell).hyperlink() });
                let id = unsafe {
                    (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell)
                };
                assert!(id.is_none());
            }
        }

        // Second row preserves hyperlink where we didn't scroll
        {
            for x in 0..1 {
                let list_cell = t.active_mut().pages.get_cell(viewport_pt(x, 1)).unwrap();
                let row = list_cell.row;
                assert!(unsafe { (*row).hyperlink() });
                let cell = list_cell.cell;
                assert!(unsafe { (*cell).hyperlink() });
                let id = unsafe {
                    (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell)
                }
                .unwrap();
                assert_eq!(id, 1 as crate::hyperlink::Id);
                let page = unsafe { crate::pagelist::Node::page(list_cell.node) };
                assert_eq!(unsafe { (*page).hyperlink_set.count() }, 1);
            }
            for x in 1..4 {
                let list_cell = t.active_mut().pages.get_cell(viewport_pt(x, 1)).unwrap();
                let cell = list_cell.cell;
                assert!(!unsafe { (*cell).hyperlink() });
                let id = unsafe {
                    (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell)
                };
                assert!(id.is_none());
            }
            for x in 4..6 {
                let list_cell = t.active_mut().pages.get_cell(viewport_pt(x, 1)).unwrap();
                let row = list_cell.row;
                assert!(unsafe { (*row).hyperlink() });
                let cell = list_cell.cell;
                assert!(unsafe { (*cell).hyperlink() });
                let id = unsafe {
                    (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell)
                }
                .unwrap();
                assert_eq!(id, 1 as crate::hyperlink::Id);
                let page = unsafe { crate::pagelist::Node::page(list_cell.node) };
                assert_eq!(unsafe { (*page).hyperlink_set.count() }, 1);
            }
        }

        t.deinit(Global);
    }

    // test "Terminal: scrollUp preserves pending wrap"
    #[test]
    fn terminal_scroll_up_preserves_pending_wrap() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.set_cursor_pos(1, 5);
        t.print('A' as u32).unwrap();
        t.set_cursor_pos(2, 5);
        t.print('B' as u32).unwrap();
        t.set_cursor_pos(3, 5);
        t.print('C' as u32).unwrap();
        t.scroll_up(1).unwrap();
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "    B\n    C\n\nX".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: scrollUp full top/bottom region"
    #[test]
    fn terminal_scroll_up_full_top_bottom_region() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print_string("top".as_bytes()).unwrap();
        t.set_cursor_pos(5, 1);
        t.print_string("ABCDE".as_bytes()).unwrap();
        t.set_top_and_bottom_margin(2, 5);

        t.clear_dirty();
        t.scroll_up(4).unwrap();

        // This is dirty because the cursor moves from this row
        assert!(t.is_dirty(active_pt(0, 0)));
        assert!(t.is_dirty(active_pt(0, 1)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "top".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: scrollUp full top/bottomleft/right scroll region"
    #[test]
    fn terminal_scroll_up_full_top_bottomleft_right_scroll_region() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print_string("top".as_bytes()).unwrap();
        t.set_cursor_pos(5, 1);
        t.print_string("ABCDE".as_bytes()).unwrap();
        t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
        t.set_top_and_bottom_margin(2, 5);
        t.set_left_and_right_margin(2, 4);

        t.clear_dirty();
        t.scroll_up(4).unwrap();

        // This is dirty because the cursor moves from this row
        assert!(t.is_dirty(active_pt(0, 0)));
        for y in 1..5u32 {
            assert!(t.is_dirty(active_pt(0, y)));
        }

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "top\n\n\n\nA   E".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: scrollUp creates scrollback in primary screen"
    #[test]
    fn terminal_scroll_up_creates_scrollback_in_primary_screen() {
        // When in primary screen with full-width scroll region at top,
        // scrollUp (CSI S) should push lines into scrollback like xterm.
        let mut t = Terminal::init(Global, opts_sb(5, 5, 10)).unwrap();

        // Fill the screen with content
        t.print_string("AAAAA".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("BBBBB".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("CCCCC".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DDDDD".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("EEEEE".as_bytes()).unwrap();

        t.clear_dirty();

        // Scroll up by 1, which should push "AAAAA" into scrollback
        t.scroll_up(1).unwrap();

        // The cursor row (new empty row) should be dirty
        assert!(unsafe { (*(*t.screens.active).cursor.page_row).dirty() });

        // The active screen should now show BBBBB through EEEEE plus one blank line
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "BBBBB\nCCCCC\nDDDDD\nEEEEE".as_bytes());
        }

        // Now scroll to the top to see scrollback - AAAAA should be there
        t.active_mut().scroll(crate::screen::Scroll::Top);
        {
            let str = t.plain_string(Global).unwrap();
            // Should see AAAAA in scrollback
            assert_eq!(
                str.as_slice(),
                "AAAAA\nBBBBB\nCCCCC\nDDDDD\nEEEEE".as_bytes()
            );
        }

        t.deinit(Global);
    }

    // test "Terminal: scrollUp with max_scrollback_bytes zero"
    #[test]
    fn terminal_scroll_up_with_max_scrollback_bytes_zero() {
        // When max_scrollback_bytes is 0, scrollUp should still work but not retain history
        let mut t = Terminal::init(Global, opts_sb(5, 5, 0)).unwrap();

        t.print_string("AAAAA".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("BBBBB".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("CCCCC".as_bytes()).unwrap();

        t.scroll_up(1).unwrap();

        // Active screen should show scrolled content
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "BBBBB\nCCCCC".as_bytes());
        }

        // Scroll to top - should be same as active since no scrollback
        t.active_mut().scroll(crate::screen::Scroll::Top);
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "BBBBB\nCCCCC".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: scrollUp with max_scrollback_bytes zero and top margin"
    #[test]
    fn terminal_scroll_up_with_max_scrollback_bytes_zero_and_top_margin() {
        // When max_scrollback_bytes is 0 and top margin is set, should use deleteLines path
        let mut t = Terminal::init(Global, opts_sb(5, 5, 0)).unwrap();

        t.print_string("AAAAA".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("BBBBB".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("CCCCC".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DDDDD".as_bytes()).unwrap();

        // Set top margin (not at row 0)
        t.set_top_and_bottom_margin(2, 5);

        t.scroll_up(1).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            // First row preserved, rest scrolled
            assert_eq!(str.as_slice(), "AAAAA\nCCCCC\nDDDDD".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: scrollUp with max_scrollback_bytes zero and left/right margin"
    #[test]
    fn terminal_scroll_up_with_max_scrollback_bytes_zero_and_left_right_margin() {
        // When max_scrollback_bytes is 0 with left/right margins, uses deleteLines path
        let mut t = Terminal::init(Global, opts_sb(10, 5, 0)).unwrap();

        t.print_string("AAAAABBBBB".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("CCCCCDDDDD".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("EEEEEFFFFF".as_bytes()).unwrap();

        // Set left/right margins (columns 2-6, 1-indexed = indices 1-5)
        t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
        t.set_left_and_right_margin(2, 6);

        t.scroll_up(1).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            // cols 1-5 scroll, col 0 and cols 6+ preserved
            assert_eq!(
                str.as_slice(),
                "ACCCCDBBBB\nCEEEEFDDDD\nE     FFFF".as_bytes()
            );
        }

        t.deinit(Global);
    }

    // test "Terminal: scrollDown simple"
    #[test]
    fn terminal_scroll_down_simple() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print_string("ABC".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DEF".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GHI".as_bytes()).unwrap();
        t.set_cursor_pos(2, 2);

        let cursor_x = unsafe { (*t.screens.active).cursor.x };
        let cursor_y = unsafe { (*t.screens.active).cursor.y };
        t.clear_dirty();
        t.scroll_down(1);
        assert_eq!(cursor_x, unsafe { (*t.screens.active).cursor.x });
        assert_eq!(cursor_y, unsafe { (*t.screens.active).cursor.y });

        for y in 0..5u32 {
            assert!(t.is_dirty(active_pt(0, y)));
        }

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\nABC\nDEF\nGHI".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: scrollDown hyperlink moves"
    #[test]
    fn terminal_scroll_down_hyperlink_moves() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.active_mut()
            .start_hyperlink("http://example.com".as_bytes(), None)
            .unwrap();
        t.print_string("ABC".as_bytes()).unwrap();
        t.active_mut().end_hyperlink();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DEF".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GHI".as_bytes()).unwrap();
        t.set_cursor_pos(2, 2);
        t.scroll_down(1);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\nABC\nDEF\nGHI".as_bytes());
        }

        for x in 0..3 {
            let list_cell = t.active_mut().pages.get_cell(viewport_pt(x, 1)).unwrap();
            let row = list_cell.row;
            assert!(unsafe { (*row).hyperlink() });
            let cell = list_cell.cell;
            assert!(unsafe { (*cell).hyperlink() });
            let id =
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell) }
                    .unwrap();
            assert_eq!(id, 1 as crate::hyperlink::Id);
            let page = unsafe { crate::pagelist::Node::page(list_cell.node) };
            assert_eq!(unsafe { (*page).hyperlink_set.count() }, 1);
        }
        for x in 0..3 {
            let list_cell = t.active_mut().pages.get_cell(viewport_pt(x, 0)).unwrap();
            let row = list_cell.row;
            assert!(!unsafe { (*row).hyperlink() });
            let cell = list_cell.cell;
            assert!(!unsafe { (*cell).hyperlink() });
            let id =
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell) };
            assert!(id.is_none());
        }

        t.deinit(Global);
    }

    // test "Terminal: scrollDown outside of scroll region"
    #[test]
    fn terminal_scroll_down_outside_of_scroll_region() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print_string("ABC".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DEF".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GHI".as_bytes()).unwrap();
        t.set_top_and_bottom_margin(3, 4);
        t.set_cursor_pos(2, 2);

        let cursor_x = unsafe { (*t.screens.active).cursor.x };
        let cursor_y = unsafe { (*t.screens.active).cursor.y };
        t.clear_dirty();
        t.scroll_down(1);
        assert_eq!(cursor_x, unsafe { (*t.screens.active).cursor.x });
        assert_eq!(cursor_y, unsafe { (*t.screens.active).cursor.y });

        assert!(!t.is_dirty(active_pt(0, 0)));

        // This is dirty because the cursor moves from this row
        assert!(t.is_dirty(active_pt(0, 1)));
        assert!(t.is_dirty(active_pt(0, 2)));
        assert!(t.is_dirty(active_pt(0, 3)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABC\nDEF\n\nGHI".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: scrollDown left/right scroll region"
    #[test]
    fn terminal_scroll_down_left_right_scroll_region() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        t.print_string("ABC123".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DEF456".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GHI789".as_bytes()).unwrap();
        t.scrolling_region.left = 1;
        t.scrolling_region.right = 3;
        t.set_cursor_pos(2, 2);

        let cursor_x = unsafe { (*t.screens.active).cursor.x };
        let cursor_y = unsafe { (*t.screens.active).cursor.y };
        t.clear_dirty();
        t.scroll_down(1);
        assert_eq!(cursor_x, unsafe { (*t.screens.active).cursor.x });
        assert_eq!(cursor_y, unsafe { (*t.screens.active).cursor.y });

        for y in 0..4u32 {
            assert!(t.is_dirty(active_pt(0, y)));
        }

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "A   23\nDBC156\nGEF489\n HI7".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: scrollDown left/right scroll region hyperlink"
    #[test]
    fn terminal_scroll_down_left_right_scroll_region_hyperlink() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        t.active_mut()
            .start_hyperlink("http://example.com".as_bytes(), None)
            .unwrap();
        t.print_string("ABC123".as_bytes()).unwrap();
        t.active_mut().end_hyperlink();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DEF456".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GHI789".as_bytes()).unwrap();
        t.scrolling_region.left = 1;
        t.scrolling_region.right = 3;
        t.set_cursor_pos(2, 2);
        t.scroll_down(1);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "A   23\nDBC156\nGEF489\n HI7".as_bytes());
        }

        // First row preserves hyperlink where we didn't scroll
        {
            for x in 0..1 {
                let list_cell = t.active_mut().pages.get_cell(viewport_pt(x, 0)).unwrap();
                let row = list_cell.row;
                assert!(unsafe { (*row).hyperlink() });
                let cell = list_cell.cell;
                assert!(unsafe { (*cell).hyperlink() });
                let id = unsafe {
                    (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell)
                }
                .unwrap();
                assert_eq!(id, 1 as crate::hyperlink::Id);
                let page = unsafe { crate::pagelist::Node::page(list_cell.node) };
                assert_eq!(unsafe { (*page).hyperlink_set.count() }, 1);
            }
            for x in 1..4 {
                let list_cell = t.active_mut().pages.get_cell(viewport_pt(x, 0)).unwrap();
                let cell = list_cell.cell;
                assert!(!unsafe { (*cell).hyperlink() });
                let id = unsafe {
                    (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell)
                };
                assert!(id.is_none());
            }
            for x in 4..6 {
                let list_cell = t.active_mut().pages.get_cell(viewport_pt(x, 0)).unwrap();
                let row = list_cell.row;
                assert!(unsafe { (*row).hyperlink() });
                let cell = list_cell.cell;
                assert!(unsafe { (*cell).hyperlink() });
                let id = unsafe {
                    (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell)
                }
                .unwrap();
                assert_eq!(id, 1 as crate::hyperlink::Id);
                let page = unsafe { crate::pagelist::Node::page(list_cell.node) };
                assert_eq!(unsafe { (*page).hyperlink_set.count() }, 1);
            }
        }

        // Second row gets some hyperlinks
        {
            for x in 0..1 {
                let list_cell = t.active_mut().pages.get_cell(viewport_pt(x, 1)).unwrap();
                let cell = list_cell.cell;
                assert!(!unsafe { (*cell).hyperlink() });
                let id = unsafe {
                    (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell)
                };
                assert!(id.is_none());
            }
            for x in 1..4 {
                let list_cell = t.active_mut().pages.get_cell(viewport_pt(x, 1)).unwrap();
                let row = list_cell.row;
                assert!(unsafe { (*row).hyperlink() });
                let cell = list_cell.cell;
                assert!(unsafe { (*cell).hyperlink() });
                let id = unsafe {
                    (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell)
                }
                .unwrap();
                assert_eq!(id, 1 as crate::hyperlink::Id);
                let page = unsafe { crate::pagelist::Node::page(list_cell.node) };
                assert_eq!(unsafe { (*page).hyperlink_set.count() }, 1);
            }
            for x in 4..6 {
                let list_cell = t.active_mut().pages.get_cell(viewport_pt(x, 1)).unwrap();
                let cell = list_cell.cell;
                assert!(!unsafe { (*cell).hyperlink() });
                let id = unsafe {
                    (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell)
                };
                assert!(id.is_none());
            }
        }

        t.deinit(Global);
    }

    // test "Terminal: scrollDown outside of left/right scroll region"
    #[test]
    fn terminal_scroll_down_outside_of_left_right_scroll_region() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        t.print_string("ABC123".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DEF456".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GHI789".as_bytes()).unwrap();
        t.scrolling_region.left = 1;
        t.scrolling_region.right = 3;
        t.set_cursor_pos(1, 1);

        let cursor_x = unsafe { (*t.screens.active).cursor.x };
        let cursor_y = unsafe { (*t.screens.active).cursor.y };
        t.clear_dirty();
        t.scroll_down(1);
        assert_eq!(cursor_x, unsafe { (*t.screens.active).cursor.x });
        assert_eq!(cursor_y, unsafe { (*t.screens.active).cursor.y });

        for y in 0..4u32 {
            assert!(t.is_dirty(active_pt(0, y)));
        }

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "A   23\nDBC156\nGEF489\n HI7".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: scrollDown preserves pending wrap"
    #[test]
    fn terminal_scroll_down_preserves_pending_wrap() {
        let mut t = Terminal::init(Global, opts(5, 10)).unwrap();

        t.set_cursor_pos(1, 5);
        t.print('A' as u32).unwrap();
        t.set_cursor_pos(2, 5);
        t.print('B' as u32).unwrap();
        t.set_cursor_pos(3, 5);
        t.print('C' as u32).unwrap();
        t.scroll_down(1);
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\n    A\n    B\nX   C".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: decaln reset margins"
    #[test]
    fn terminal_decaln_reset_margins() {
        let mut t = Terminal::init(Global, opts(3, 3)).unwrap();

        // Initial value
        t.modes.set(modespkg::Mode::Origin, true);
        t.set_top_and_bottom_margin(2, 3);
        t.decaln().unwrap();
        t.scroll_down(1);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\nEEE\nEEE".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: decaln preserves color"
    #[test]
    fn terminal_decaln_preserves_color() {
        let mut t = Terminal::init(Global, opts(3, 3)).unwrap();

        // Initial value
        t.set_attribute(sgr::Attribute::DirectColorBg(color::RGB {
            r: 0xFF,
            g: 0,
            b: 0,
        }))
        .unwrap();
        t.modes.set(modespkg::Mode::Origin, true);
        t.set_top_and_bottom_margin(2, 3);
        t.decaln().unwrap();
        t.scroll_down(1);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\nEEE\nEEE".as_bytes());
        }

        {
            let list_cell = t.active_mut().pages.get_cell(active_pt(0, 0)).unwrap();
            let cell = list_cell.cell;
            assert!(unsafe { (*cell).content_tag() } == pagepkg::ContentTag::BgColorRgb);
            assert_eq!(
                pagepkg::CellRgb {
                    r: 0xFF,
                    g: 0,
                    b: 0,
                },
                unsafe { (*cell).content_color_rgb() }
            );
        }

        t.deinit(Global);
    }

    // test "Terminal: DECALN resets graphemes with protected mode"
    #[test]
    fn terminal_decaln_resets_graphemes_with_protected_mode() {
        let mut t = Terminal::init(Global, opts(3, 3)).unwrap();

        // Add protected mode. A previous version of DECALN accidentally preserved
        // protected mode which left dangling managed memory.
        t.set_protected_mode(ansi::ProtectedMode::Iso);

        // This is: 👨‍👩‍👧 (which may or may not render correctly)
        t.modes.set(modespkg::Mode::GraphemeCluster, true);
        t.print(0x1F468).unwrap();
        t.print(0x200D).unwrap();
        t.print(0x1F469).unwrap();
        t.print(0x200D).unwrap();
        t.print(0x1F467).unwrap();

        t.decaln().unwrap();

        assert_eq!(0usize, unsafe { (*t.screens.active).cursor.y } as usize);
        assert_eq!(0usize, unsafe { (*t.screens.active).cursor.x } as usize);
        assert!(unsafe { (*t.screens.active).cursor.protected });
        assert!(unsafe { (*t.screens.active).protected_mode } == ansi::ProtectedMode::Iso);

        for y in 0..t.rows {
            assert!(t.is_dirty(active_pt(0, y as u32)));
        }

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "EEE\nEEE\nEEE".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: reverseIndex"
    #[test]
    fn terminal_reverse_index() {
        let mut t = Terminal::init(Global, opts(2, 5)).unwrap();

        // Initial value
        t.print('A' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('B' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('C' as u32).unwrap();
        t.reverse_index();
        t.print('D' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.carriage_return();
        t.linefeed().unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "A\nBD\nC".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: reverseIndex from the top"
    #[test]
    fn terminal_reverse_index_from_the_top() {
        let mut t = Terminal::init(Global, opts(2, 5)).unwrap();

        t.print('A' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('B' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.carriage_return();
        t.linefeed().unwrap();

        t.set_cursor_pos(1, 1);
        t.reverse_index();
        t.print('D' as u32).unwrap();

        t.carriage_return();
        t.linefeed().unwrap();
        t.set_cursor_pos(1, 1);
        t.reverse_index();
        t.print('E' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "E\nD\nA\nB".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: reverseIndex top of scrolling region"
    #[test]
    fn terminal_reverse_index_top_of_scrolling_region() {
        let mut t = Terminal::init(Global, opts(2, 10)).unwrap();

        // Initial value
        t.set_cursor_pos(2, 1);
        t.print('A' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('B' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('C' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('D' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();

        // Set our scroll region
        t.set_top_and_bottom_margin(2, 5);
        t.set_cursor_pos(2, 1);
        t.reverse_index();
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\nX\nA\nB\nC".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: reverseIndex top of screen"
    #[test]
    fn terminal_reverse_index_top_of_screen() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print('A' as u32).unwrap();
        t.set_cursor_pos(2, 1);
        t.print('B' as u32).unwrap();
        t.set_cursor_pos(3, 1);
        t.print('C' as u32).unwrap();
        t.set_cursor_pos(1, 1);
        t.reverse_index();
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "X\nA\nB\nC".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: reverseIndex not top of screen"
    #[test]
    fn terminal_reverse_index_not_top_of_screen() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print('A' as u32).unwrap();
        t.set_cursor_pos(2, 1);
        t.print('B' as u32).unwrap();
        t.set_cursor_pos(3, 1);
        t.print('C' as u32).unwrap();
        t.set_cursor_pos(2, 1);
        t.reverse_index();
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "X\nB\nC".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: reverseIndex top/bottom margins"
    #[test]
    fn terminal_reverse_index_top_bottom_margins() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print('A' as u32).unwrap();
        t.set_cursor_pos(2, 1);
        t.print('B' as u32).unwrap();
        t.set_cursor_pos(3, 1);
        t.print('C' as u32).unwrap();
        t.set_top_and_bottom_margin(2, 3);
        t.set_cursor_pos(2, 1);
        t.reverse_index();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "A\n\nB".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: reverseIndex outside top/bottom margins"
    #[test]
    fn terminal_reverse_index_outside_top_bottom_margins() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print('A' as u32).unwrap();
        t.set_cursor_pos(2, 1);
        t.print('B' as u32).unwrap();
        t.set_cursor_pos(3, 1);
        t.print('C' as u32).unwrap();
        t.set_top_and_bottom_margin(2, 3);
        t.set_cursor_pos(1, 1);
        t.reverse_index();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "A\nB\nC".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: reverseIndex left/right margins"
    #[test]
    fn terminal_reverse_index_left_right_margins() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print_string("ABC".as_bytes()).unwrap();
        t.set_cursor_pos(2, 1);
        t.print_string("DEF".as_bytes()).unwrap();
        t.set_cursor_pos(3, 1);
        t.print_string("GHI".as_bytes()).unwrap();
        t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
        t.set_left_and_right_margin(2, 3);
        t.set_cursor_pos(1, 2);
        t.reverse_index();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "A\nDBC\nGEF\n HI".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: reverseIndex outside left/right margins"
    #[test]
    fn terminal_reverse_index_outside_left_right_margins() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print_string("ABC".as_bytes()).unwrap();
        t.set_cursor_pos(2, 1);
        t.print_string("DEF".as_bytes()).unwrap();
        t.set_cursor_pos(3, 1);
        t.print_string("GHI".as_bytes()).unwrap();
        t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
        t.set_left_and_right_margin(2, 3);
        t.set_cursor_pos(1, 1);
        t.reverse_index();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABC\nDEF\nGHI".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: index"
    #[test]
    fn terminal_index() {
        let mut t = Terminal::init(Global, opts(2, 5)).unwrap();

        t.index().unwrap();
        t.print('A' as u32).unwrap();

        assert!(t.is_dirty(active_pt(0, 0)));
        assert!(t.is_dirty(active_pt(0, 1)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\nA".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: index from the bottom"
    #[test]
    fn terminal_index_from_the_bottom() {
        let mut t = Terminal::init(Global, opts(2, 5)).unwrap();

        t.set_cursor_pos(5, 1);
        t.print('A' as u32).unwrap();
        t.cursor_left(1); // undo moving right from 'A'

        t.clear_dirty();
        t.index().unwrap();
        t.print('B' as u32).unwrap();

        assert!(t.is_dirty(active_pt(0, 3)));
        assert!(t.is_dirty(active_pt(0, 4)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\n\n\nA\nB".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: index outside of scrolling region"
    #[test]
    fn terminal_index_outside_of_scrolling_region() {
        let mut t = Terminal::init(Global, opts(2, 5)).unwrap();

        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        t.set_top_and_bottom_margin(2, 5);
        t.index().unwrap();
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 1);

        t.deinit(Global);
    }

    // test "Terminal: index from the bottom outside of scroll region"
    #[test]
    fn terminal_index_from_the_bottom_outside_of_scroll_region() {
        let mut t = Terminal::init(Global, opts(2, 5)).unwrap();

        t.set_top_and_bottom_margin(1, 2);
        t.set_cursor_pos(5, 1);
        t.print('A' as u32).unwrap();
        t.clear_dirty();
        t.index().unwrap();
        t.print('B' as u32).unwrap();
        assert!(t.is_dirty(active_pt(0, 4)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\n\n\n\nAB".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: index no scroll region, top of screen"
    #[test]
    fn terminal_index_no_scroll_region_top_of_screen() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print('A' as u32).unwrap();
        t.clear_dirty();
        t.index().unwrap();
        t.print('X' as u32).unwrap();

        assert!(t.is_dirty(active_pt(0, 0)));
        assert!(t.is_dirty(active_pt(0, 1)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "A\n X".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: index scrolling with hyperlink"
    #[test]
    fn terminal_index_scrolling_with_hyperlink() {
        let mut t = Terminal::init(Global, opts(2, 5)).unwrap();

        t.set_cursor_pos(5, 1);
        t.active_mut()
            .start_hyperlink("http://example.com".as_bytes(), None)
            .unwrap();
        t.print('A' as u32).unwrap();
        t.active_mut().end_hyperlink();
        t.cursor_left(1); // undo moving right from 'A'
        t.index().unwrap();
        t.print('B' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\n\n\nA\nB".as_bytes());
        }

        {
            let list_cell = t.active_mut().pages.get_cell(viewport_pt(0, 3)).unwrap();
            let row = list_cell.row;
            assert!(unsafe { (*row).hyperlink() });
            let cell = list_cell.cell;
            assert!(unsafe { (*cell).hyperlink() });
            let id =
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell) }
                    .unwrap();
            assert_eq!(id, 1 as crate::hyperlink::Id);
        }
        {
            let list_cell = t.active_mut().pages.get_cell(viewport_pt(0, 4)).unwrap();
            let row = list_cell.row;
            assert!(!unsafe { (*row).hyperlink() });
            let cell = list_cell.cell;
            assert!(!unsafe { (*cell).hyperlink() });
            let id =
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell) };
            assert!(id.is_none());
        }

        t.deinit(Global);
    }

    // test "Terminal: index bottom of primary screen"
    #[test]
    fn terminal_index_bottom_of_primary_screen() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.set_cursor_pos(5, 1);
        t.print('A' as u32).unwrap();
        t.clear_dirty();
        t.index().unwrap();
        t.print('X' as u32).unwrap();

        assert!(t.is_dirty(active_pt(0, 3)));
        assert!(t.is_dirty(active_pt(0, 4)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\n\n\nA\n X".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: index bottom of primary screen background sgr"
    #[test]
    fn terminal_index_bottom_of_primary_screen_background_sgr() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.set_cursor_pos(5, 1);
        t.print('A' as u32).unwrap();
        t.set_attribute(sgr::Attribute::DirectColorBg(color::RGB {
            r: 0xFF,
            g: 0,
            b: 0,
        }))
        .unwrap();
        t.index().unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\n\n\nA".as_bytes());
            for x in 0..5 {
                let list_cell = t.active_mut().pages.get_cell(active_pt(x, 4)).unwrap();
                let cell = list_cell.cell;
                assert!(unsafe { (*cell).content_tag() } == pagepkg::ContentTag::BgColorRgb);
                assert_eq!(
                    pagepkg::CellRgb {
                        r: 0xFF,
                        g: 0,
                        b: 0,
                    },
                    unsafe { (*cell).content_color_rgb() }
                );
            }
        }

        t.deinit(Global);
    }

    // test "Terminal: index inside scroll region"
    #[test]
    fn terminal_index_inside_scroll_region() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.set_top_and_bottom_margin(1, 3);
        t.print('A' as u32).unwrap();
        t.clear_dirty();
        t.index().unwrap();
        t.print('X' as u32).unwrap();

        assert!(t.is_dirty(active_pt(0, 0)));
        assert!(t.is_dirty(active_pt(0, 1)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "A\n X".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: index bottom of scroll region with hyperlinks"
    #[test]
    fn terminal_index_bottom_of_scroll_region_with_hyperlinks() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.set_top_and_bottom_margin(1, 2);
        t.print('A' as u32).unwrap();
        t.index().unwrap();
        t.carriage_return();
        t.active_mut()
            .start_hyperlink("http://example.com".as_bytes(), None)
            .unwrap();
        t.print('B' as u32).unwrap();
        t.active_mut().end_hyperlink();
        t.index().unwrap();
        t.carriage_return();
        t.print('C' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "B\nC".as_bytes());
        }

        {
            let list_cell = t.active_mut().pages.get_cell(viewport_pt(0, 0)).unwrap();
            let row = list_cell.row;
            assert!(unsafe { (*row).hyperlink() });
            let cell = list_cell.cell;
            assert!(unsafe { (*cell).hyperlink() });
            let id =
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell) }
                    .unwrap();
            assert_eq!(id, 1 as crate::hyperlink::Id);
        }
        {
            let list_cell = t.active_mut().pages.get_cell(viewport_pt(0, 1)).unwrap();
            let row = list_cell.row;
            assert!(!unsafe { (*row).hyperlink() });
            let cell = list_cell.cell;
            assert!(!unsafe { (*cell).hyperlink() });
            let id =
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell) };
            assert!(id.is_none());
        }

        t.deinit(Global);
    }

    // test "Terminal: index bottom of scroll region clear hyperlinks"
    #[test]
    fn terminal_index_bottom_of_scroll_region_clear_hyperlinks() {
        let mut t = Terminal::init(Global, opts_sb(5, 5, 0)).unwrap();

        t.set_top_and_bottom_margin(2, 3);
        t.set_cursor_pos(2, 1);
        t.active_mut()
            .start_hyperlink("http://example.com".as_bytes(), None)
            .unwrap();
        t.print('A' as u32).unwrap();
        t.active_mut().end_hyperlink();
        t.index().unwrap();
        t.carriage_return();
        t.print('B' as u32).unwrap();
        t.index().unwrap();
        t.carriage_return();
        t.print('C' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\nB\nC".as_bytes());
        }

        for y in 1..3 {
            let list_cell = t.active_mut().pages.get_cell(viewport_pt(0, y)).unwrap();
            let row = list_cell.row;
            assert!(!unsafe { (*row).hyperlink() });
            let cell = list_cell.cell;
            assert!(!unsafe { (*cell).hyperlink() });
            let id =
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell) };
            assert!(id.is_none());
            let page = unsafe { crate::pagelist::Node::page(list_cell.node) };
            assert_eq!(unsafe { (*page).hyperlink_set.count() }, 0);
        }

        t.deinit(Global);
    }

    // test "Terminal: index bottom of scroll region with background SGR"
    #[test]
    fn terminal_index_bottom_of_scroll_region_with_background_sgr() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.set_top_and_bottom_margin(1, 3);
        t.set_cursor_pos(4, 1);
        t.print('B' as u32).unwrap();
        t.set_cursor_pos(3, 1);
        t.print('A' as u32).unwrap();
        t.set_attribute(sgr::Attribute::DirectColorBg(color::RGB {
            r: 0xFF,
            g: 0,
            b: 0,
        }))
        .unwrap();
        t.index().unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\nA\n\nB".as_bytes());
        }

        for x in 0..t.cols {
            let list_cell = t.active_mut().pages.get_cell(active_pt(x, 2)).unwrap();
            let cell = list_cell.cell;
            assert!(unsafe { (*cell).content_tag() } == pagepkg::ContentTag::BgColorRgb);
            assert_eq!(
                pagepkg::CellRgb {
                    r: 0xFF,
                    g: 0,
                    b: 0,
                },
                unsafe { (*cell).content_color_rgb() }
            );
        }

        t.deinit(Global);
    }

    // test "Terminal: index bottom of primary screen with scroll region"
    #[test]
    fn terminal_index_bottom_of_primary_screen_with_scroll_region() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.set_top_and_bottom_margin(1, 3);
        t.set_cursor_pos(3, 1);
        t.print('A' as u32).unwrap();
        t.set_cursor_pos(5, 1);
        t.clear_dirty();
        t.index().unwrap();
        t.index().unwrap();
        t.index().unwrap();
        t.print('X' as u32).unwrap();

        for y in 0..4u32 {
            assert!(!t.is_dirty(active_pt(0, y)));
        }
        assert!(t.is_dirty(active_pt(0, 4)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\n\nA\n\nX".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: index outside left/right margin"
    #[test]
    fn terminal_index_outside_left_right_margin() {
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

        t.set_top_and_bottom_margin(1, 3);
        t.scrolling_region.left = 3;
        t.scrolling_region.right = 5;
        t.set_cursor_pos(3, 3);
        t.print('A' as u32).unwrap();
        t.set_cursor_pos(3, 1);
        t.clear_dirty();
        t.index().unwrap();
        t.print('X' as u32).unwrap();

        assert!(t.is_dirty(active_pt(0, 2)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\n\nX A".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: index inside left/right margin"
    #[test]
    fn terminal_index_inside_left_right_margin() {
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

        t.print_string("AAAAAA".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("AAAAAA".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("AAAAAA".as_bytes()).unwrap();
        t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
        t.set_top_and_bottom_margin(1, 3);
        t.set_left_and_right_margin(1, 3);
        t.set_cursor_pos(3, 1);

        t.clear_dirty();
        t.index().unwrap();

        assert!(t.is_dirty(active_pt(0, 0)));
        assert!(t.is_dirty(active_pt(0, 1)));
        assert!(t.is_dirty(active_pt(0, 2)));

        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 2);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 0);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "AAAAAA\nAAAAAA\n   AAA".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: index bottom of scroll region creates scrollback"
    #[test]
    fn terminal_index_bottom_of_scroll_region_creates_scrollback() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.set_top_and_bottom_margin(1, 3);
        t.print_string("1\n2\n3".as_bytes()).unwrap();
        t.set_cursor_pos(4, 1);
        t.print('X' as u32).unwrap();
        t.set_cursor_pos(3, 1);
        t.index().unwrap();
        t.print('Y' as u32).unwrap();

        {
            let str = t
                .active_mut()
                .dump_string_alloc(Global, viewport_pt(0, 0))
                .unwrap();
            assert_eq!(str.as_slice(), "2\n3\nY\nX".as_bytes());
        }
        {
            let str = t
                .active_mut()
                .dump_string_alloc(Global, screen_pt(0, 0))
                .unwrap();
            assert_eq!(str.as_slice(), "1\n2\n3\nY\nX".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: index bottom of scroll region no scrollback"
    #[test]
    fn terminal_index_bottom_of_scroll_region_no_scrollback() {
        let mut t = Terminal::init(Global, opts_sb(5, 5, 0)).unwrap();

        t.set_top_and_bottom_margin(1, 3);
        t.set_cursor_pos(4, 1);
        t.print('B' as u32).unwrap();
        t.set_cursor_pos(3, 1);
        t.print('A' as u32).unwrap();
        t.clear_dirty();
        t.index().unwrap();
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\nA\n X\nB".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: index bottom of scroll region blank line preserves SGR"
    #[test]
    fn terminal_index_bottom_of_scroll_region_blank_line_preserves_sgr() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.set_top_and_bottom_margin(1, 3);
        t.print_string("1\n2\n3".as_bytes()).unwrap();
        t.set_cursor_pos(4, 1);
        t.print('X' as u32).unwrap();
        t.set_cursor_pos(3, 1);
        t.set_attribute(sgr::Attribute::DirectColorBg(color::RGB {
            r: 0xFF,
            g: 0,
            b: 0,
        }))
        .unwrap();
        t.index().unwrap();

        {
            let str = t
                .active_mut()
                .dump_string_alloc(Global, viewport_pt(0, 0))
                .unwrap();
            assert_eq!(str.as_slice(), "2\n3\n\nX".as_bytes());
        }
        {
            let str = t
                .active_mut()
                .dump_string_alloc(Global, screen_pt(0, 0))
                .unwrap();
            assert_eq!(str.as_slice(), "1\n2\n3\n\nX".as_bytes());
        }
        for x in 0..t.cols {
            let list_cell = t.active_mut().pages.get_cell(active_pt(x, 2)).unwrap();
            let cell = list_cell.cell;
            assert!(unsafe { (*cell).content_tag() } == pagepkg::ContentTag::BgColorRgb);
            assert_eq!(
                pagepkg::CellRgb {
                    r: 0xFF,
                    g: 0,
                    b: 0,
                },
                unsafe { (*cell).content_color_rgb() }
            );
        }

        t.deinit(Global);
    }

    // -- oracle 77190bd02: index/scrollUp scroll-region routing (`P3-W5 T3`) --

    // test "Terminal: index bottom of scroll region with top margin and background SGR"
    #[test]
    fn terminal_index_bottom_of_scroll_region_with_top_margin_and_background_sgr() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print_string("1\n2\n3\n4\n5".as_bytes()).unwrap();
        t.set_top_and_bottom_margin(2, 4);
        t.set_cursor_pos(4, 1);
        t.set_attribute(sgr::Attribute::DirectColorBg(color::RGB {
            r: 0xFF,
            g: 0,
            b: 0,
        }))
        .unwrap();
        t.index().unwrap();

        // The region (rows 2-4) scrolled up, rows outside are unchanged.
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "1\n3\n4\n\n5".as_bytes());
        }

        // The cursor is on the new blank row.
        assert_eq!(3 as size::CellCountInt, unsafe {
            (*t.screens.active).cursor.y
        });

        // The new blank row must be filled with our background color.
        for x in 0..t.cols {
            let list_cell = t.active_mut().pages.get_cell(active_pt(x, 3)).unwrap();
            let cell = list_cell.cell;
            assert!(unsafe { (*cell).content_tag() } == pagepkg::ContentTag::BgColorRgb);
            assert_eq!(
                pagepkg::CellRgb {
                    r: 0xFF,
                    g: 0,
                    b: 0,
                },
                unsafe { (*cell).content_color_rgb() }
            );
        }

        t.deinit(Global);
    }

    // test "Terminal: index bottom of alt screen full region"
    #[test]
    fn terminal_index_bottom_of_alt_screen_full_region() {
        let mut t = Terminal::init(Global, opts(5, 3)).unwrap();

        t.switch_screen_mode(SwitchScreenMode::M1049, true).unwrap();
        t.print_string("A\nB\nC".as_bytes()).unwrap();
        t.index().unwrap();
        t.carriage_return();
        t.print('D' as u32).unwrap();

        // Content scrolled up and the scrolled-out row is discarded, NOT
        // moved into scrollback (the alt screen has none).
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "B\nC\nD".as_bytes());
        }
        {
            let str = t
                .active_mut()
                .dump_string_alloc(Global, screen_pt(0, 0))
                .unwrap();
            assert_eq!(str.as_slice(), "B\nC\nD".as_bytes());
        }

        // Primary screen is untouched.
        t.switch_screen_mode(SwitchScreenMode::M1049, false)
            .unwrap();
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: index bottom of alt screen top region"
    #[test]
    fn terminal_index_bottom_of_alt_screen_top_region() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.switch_screen_mode(SwitchScreenMode::M1049, true).unwrap();
        t.print_string("1\n2\n3\n4\n5".as_bytes()).unwrap();

        // Region at the top of the screen, excluding the last row. On the
        // alt screen this must NOT create scrollback.
        t.set_top_and_bottom_margin(1, 4);
        t.set_cursor_pos(4, 1);
        t.index().unwrap();
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "2\n3\n4\nX\n5".as_bytes());
        }
        {
            let str = t
                .active_mut()
                .dump_string_alloc(Global, screen_pt(0, 0))
                .unwrap();
            assert_eq!(str.as_slice(), "2\n3\n4\nX\n5".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: scrollUp top region no scrollback"
    #[test]
    fn terminal_scroll_up_top_region_no_scrollback() {
        let mut t = Terminal::init(Global, opts_sb(5, 5, 0)).unwrap();

        t.print_string("A\nB\nC\nD\nE".as_bytes()).unwrap();
        t.set_top_and_bottom_margin(1, 3);
        t.scroll_up(1).unwrap();

        // The region scrolled and the scrolled-out row is discarded.
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "B\nC\n\nD\nE".as_bytes());
        }
        {
            let str = t
                .active_mut()
                .dump_string_alloc(Global, screen_pt(0, 0))
                .unwrap();
            assert_eq!(str.as_slice(), "B\nC\n\nD\nE".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteLines simple"
    #[test]
    fn terminal_delete_lines_simple() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print_string("ABC".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DEF".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GHI".as_bytes()).unwrap();
        t.set_cursor_pos(2, 2);

        // PORTS: oracle 0bad91adb — a full-width line operation moves whole
        // rows, so the affected page generations are renewed.
        let node = unsafe { (*(*t.screens.active).cursor.page_pin).node };
        let serial = unsafe { (*node).serial };
        t.clear_dirty();
        t.delete_lines(1);
        assert!(!unsafe { (*t.screens.active).pages.node_is_valid(node, serial) });

        assert!(!t.is_dirty(active_pt(0, 0)));
        assert!(t.is_dirty(active_pt(0, 1)));
        assert!(t.is_dirty(active_pt(0, 2)));
        assert!(t.is_dirty(active_pt(0, 3)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABC\nGHI".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteLines with scroll region"
    #[test]
    fn terminal_delete_lines_with_scroll_region() {
        let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

        // Initial value
        t.print('A' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('B' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('C' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('D' as u32).unwrap();

        t.set_top_and_bottom_margin(1, 3);
        t.set_cursor_pos(1, 1);

        t.clear_dirty();
        t.delete_lines(1);

        assert!(t.is_dirty(active_pt(0, 0)));
        assert!(t.is_dirty(active_pt(0, 1)));
        assert!(t.is_dirty(active_pt(0, 2)));
        assert!(!t.is_dirty(active_pt(0, 3)));

        t.print('E' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "E\nC\n\nD".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteLines with scroll region, large count"
    #[test]
    fn terminal_delete_lines_with_scroll_region_large_count() {
        let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

        // Initial value
        t.print('A' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('B' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('C' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('D' as u32).unwrap();

        t.set_top_and_bottom_margin(1, 3);
        t.set_cursor_pos(1, 1);

        t.clear_dirty();
        t.delete_lines(5);

        assert!(t.is_dirty(active_pt(0, 0)));
        assert!(t.is_dirty(active_pt(0, 1)));
        assert!(t.is_dirty(active_pt(0, 2)));
        assert!(!t.is_dirty(active_pt(0, 3)));

        t.print('E' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "E\n\n\nD".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: insertBlanks zero"
    #[test]
    fn terminal_insert_blanks_zero() {
        let mut t = Terminal::init(Global, opts(5, 2)).unwrap();

        t.print('A' as u32).unwrap();
        t.print('B' as u32).unwrap();
        t.print('C' as u32).unwrap();
        t.set_cursor_pos(1, 1);

        t.insert_blanks(0);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABC".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: insertBlanks"
    #[test]
    fn terminal_insert_blanks() {
        // NOTE: this is not verified with conformance tests, so these
        // tests might actually be verifying wrong behavior.
        let mut t = Terminal::init(Global, opts(5, 2)).unwrap();

        t.print('A' as u32).unwrap();
        t.print('B' as u32).unwrap();
        t.print('C' as u32).unwrap();
        t.set_cursor_pos(1, 1);

        t.clear_dirty();
        t.insert_blanks(2);
        assert!(t.is_dirty(active_pt(0, 0)));
        assert!(!t.is_dirty(active_pt(0, 1)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "  ABC".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: insertBlanks pushes off end"
    #[test]
    fn terminal_insert_blanks_pushes_off_end() {
        // NOTE: this is not verified with conformance tests, so these
        // tests might actually be verifying wrong behavior.
        let mut t = Terminal::init(Global, opts(3, 2)).unwrap();

        t.print('A' as u32).unwrap();
        t.print('B' as u32).unwrap();
        t.print('C' as u32).unwrap();
        t.set_cursor_pos(1, 1);

        t.clear_dirty();
        t.insert_blanks(2);
        assert!(t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "  A".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: insertBlanks more than size"
    #[test]
    fn terminal_insert_blanks_more_than_size() {
        // NOTE: this is not verified with conformance tests, so these
        // tests might actually be verifying wrong behavior.
        let mut t = Terminal::init(Global, opts(3, 2)).unwrap();

        t.print('A' as u32).unwrap();
        t.print('B' as u32).unwrap();
        t.print('C' as u32).unwrap();
        t.set_cursor_pos(1, 1);

        t.clear_dirty();
        t.insert_blanks(5);
        assert!(t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: insertBlanks no scroll region, fits"
    #[test]
    fn terminal_insert_blanks_no_scroll_region_fits() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        for c in "ABC".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 1);

        t.clear_dirty();
        t.insert_blanks(2);
        assert!(t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "  ABC".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: insertBlanks shift off screen"
    #[test]
    fn terminal_insert_blanks_shift_off_screen() {
        let mut t = Terminal::init(Global, opts(5, 10)).unwrap();

        for c in "  ABC".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 3);
        t.clear_dirty();
        t.insert_blanks(2);
        assert!(t.is_dirty(active_pt(0, 0)));
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "  X A".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: insertBlanks split multi-cell character"
    #[test]
    fn terminal_insert_blanks_split_multi_cell_character() {
        let mut t = Terminal::init(Global, opts(5, 10)).unwrap();

        for c in "123".chars() {
            t.print(c as u32).unwrap();
        }
        t.print('橋' as u32).unwrap();
        t.set_cursor_pos(1, 1);
        t.clear_dirty();
        t.insert_blanks(1);
        assert!(t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), " 123".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: insertBlanks inside left/right scroll region"
    #[test]
    fn terminal_insert_blanks_inside_left_right_scroll_region() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        t.scrolling_region.left = 2;
        t.scrolling_region.right = 4;
        t.set_cursor_pos(1, 3);
        for c in "ABC".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 3);

        t.clear_dirty();
        t.insert_blanks(2);
        assert!(t.is_dirty(active_pt(0, 0)));
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "  X A".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: insertBlanks outside left/right scroll region"
    #[test]
    fn terminal_insert_blanks_outside_left_right_scroll_region() {
        let mut t = Terminal::init(Global, opts(6, 10)).unwrap();

        t.set_cursor_pos(1, 4);
        for c in "ABC".chars() {
            t.print(c as u32).unwrap();
        }
        t.scrolling_region.left = 2;
        t.scrolling_region.right = 4;
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
        t.clear_dirty();
        t.insert_blanks(2);
        assert!(!t.is_dirty(active_pt(0, 0)));
        assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "   ABX".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: insertLines colors with bg color"
    #[test]
    fn terminal_insert_lines_colors_with_bg_color() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print_string("ABC".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DEF".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GHI".as_bytes()).unwrap();
        t.set_cursor_pos(2, 2);

        t.set_attribute(sgr::Attribute::DirectColorBg(color::RGB {
            r: 0xFF,
            g: 0,
            b: 0,
        }))
        .unwrap();
        t.insert_lines(1);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABC\n\nDEF\nGHI".as_bytes());
        }

        for x in 0..t.cols {
            let list_cell = t.active_mut().pages.get_cell(active_pt(x, 1)).unwrap();
            let cell = list_cell.cell;
            assert!(unsafe { (*cell).content_tag() } == pagepkg::ContentTag::BgColorRgb);
            assert_eq!(
                pagepkg::CellRgb {
                    r: 0xFF,
                    g: 0,
                    b: 0,
                },
                unsafe { (*cell).content_color_rgb() }
            );
        }

        t.deinit(Global);
    }

    // test "Terminal: insertLines handles style refs"
    #[test]
    fn terminal_insert_lines_handles_style_refs() {
        let mut t = Terminal::init(Global, opts(5, 3)).unwrap();

        t.print_string("ABC".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DEF".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();

        // For the line being deleted, create a refcounted style
        t.set_attribute(sgr::Attribute::Bold).unwrap();
        t.print_string("GHI".as_bytes()).unwrap();
        t.set_attribute(sgr::Attribute::Unset).unwrap();

        // verify we have styles in our style map
        let page =
            unsafe { crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node) };
        assert_eq!(1usize, unsafe { (*page).styles.count() });

        t.set_cursor_pos(2, 2);
        t.insert_lines(1);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABC\n\nDEF".as_bytes());
        }

        // verify we have no styles in our style map
        assert_eq!(0usize, unsafe { (*page).styles.count() });

        t.deinit(Global);
    }

    // test "Terminal: insertLines across page boundary marks all shifted rows dirty"
    #[cfg_attr(
        all(miri, debug_assertions),
        ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
    )]
    #[test]
    fn terminal_insert_lines_across_page_boundary_marks_all_shifted_rows_dirty() {
        let mut t = Terminal::init(Global, opts_sb(10, 5, 1024)).unwrap();

        let first_page = t.active_mut().pages.pages.first;
        let first_page_nrows = unsafe { crate::pagelist::Node::capacity(first_page).rows };

        // Fill up the first page minus 3 rows
        for _ in 0..(first_page_nrows - 3) {
            t.linefeed().unwrap();
        }

        // Add content that will cross a page boundary
        t.print_string("1AAAA".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("2BBBB".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("3CCCC".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("4DDDD".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("5EEEE".as_bytes()).unwrap();

        // Verify we now have a second page
        let second_page = unsafe { (*first_page).next };
        assert!(!second_page.is_null());
        // PORTS: oracle 0bad91adb — every page in the shifted range is renewed.
        let first_serial = unsafe { (*first_page).serial };
        let second_serial = unsafe { (*second_page).serial };

        t.set_cursor_pos(1, 1);
        t.clear_dirty();
        t.insert_lines(1);
        assert!(!unsafe {
            (*t.screens.active)
                .pages
                .node_is_valid(first_page, first_serial)
        });
        assert!(!unsafe {
            (*t.screens.active)
                .pages
                .node_is_valid(second_page, second_serial)
        });

        assert!(t.is_dirty(active_pt(0, 0)));
        assert!(t.is_dirty(active_pt(0, 1)));
        assert!(t.is_dirty(active_pt(0, 2)));
        assert!(t.is_dirty(active_pt(0, 3)));
        assert!(t.is_dirty(active_pt(0, 4)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\n1AAAA\n2BBBB\n3CCCC\n4DDDD".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: insertLines (legacy test)"
    #[test]
    fn terminal_insert_lines_legacy_test() {
        let mut t = Terminal::init(Global, opts(2, 5)).unwrap();

        // Initial value
        t.print('A' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('B' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('C' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('D' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('E' as u32).unwrap();

        // Move to row 2
        t.set_cursor_pos(2, 1);

        // Insert two lines
        t.insert_lines(2);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "A\n\n\nB\nC".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: insertLines multi-codepoint graphemes"
    #[test]
    fn terminal_insert_lines_multi_codepoint_graphemes() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        // Disable grapheme clustering
        t.modes.set(modespkg::Mode::GraphemeCluster, true);

        t.print_string("ABC".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();

        // This is: 👨‍👩‍👧 (which may or may not render correctly)
        t.print(0x1F468).unwrap();
        t.print(0x200D).unwrap();
        t.print(0x1F469).unwrap();
        t.print(0x200D).unwrap();
        t.print(0x1F467).unwrap();

        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GHI".as_bytes()).unwrap();
        t.set_cursor_pos(2, 2);
        t.insert_lines(1);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(
                str.as_slice(),
                "ABC\n\n👨\u{200D}👩\u{200D}👧\nGHI".as_bytes()
            );
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteLines colors with bg color"
    #[test]
    fn terminal_delete_lines_colors_with_bg_color() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print_string("ABC".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DEF".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GHI".as_bytes()).unwrap();
        t.set_cursor_pos(2, 2);

        t.set_attribute(sgr::Attribute::DirectColorBg(color::RGB {
            r: 0xFF,
            g: 0,
            b: 0,
        }))
        .unwrap();
        t.delete_lines(1);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABC\nGHI".as_bytes());
        }

        for x in 0..t.cols {
            let list_cell = t.active_mut().pages.get_cell(active_pt(x, 4)).unwrap();
            let cell = list_cell.cell;
            assert!(unsafe { (*cell).content_tag() } == pagepkg::ContentTag::BgColorRgb);
            assert_eq!(
                pagepkg::CellRgb {
                    r: 0xFF,
                    g: 0,
                    b: 0,
                },
                unsafe { (*cell).content_color_rgb() }
            );
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteLines across page boundary marks all shifted rows dirty"
    #[cfg_attr(
        all(miri, debug_assertions),
        ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
    )]
    #[test]
    fn terminal_delete_lines_across_page_boundary_marks_all_shifted_rows_dirty() {
        let mut t = Terminal::init(Global, opts_sb(10, 5, 1024)).unwrap();

        let first_page = t.active_mut().pages.pages.first;
        let first_page_nrows = unsafe { crate::pagelist::Node::capacity(first_page).rows };

        // Fill up the first page minus 3 rows
        for _ in 0..(first_page_nrows - 3) {
            t.linefeed().unwrap();
        }

        // Add content that will cross a page boundary
        t.print_string("1AAAA".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("2BBBB".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("3CCCC".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("4DDDD".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("5EEEE".as_bytes()).unwrap();

        // Verify we now have a second page
        let second_page = unsafe { (*first_page).next };
        assert!(!second_page.is_null());
        // PORTS: oracle 0bad91adb — every page in the shifted range is renewed.
        let first_serial = unsafe { (*first_page).serial };
        let second_serial = unsafe { (*second_page).serial };

        t.set_cursor_pos(1, 1);
        t.clear_dirty();
        t.delete_lines(1);
        assert!(!unsafe {
            (*t.screens.active)
                .pages
                .node_is_valid(first_page, first_serial)
        });
        assert!(!unsafe {
            (*t.screens.active)
                .pages
                .node_is_valid(second_page, second_serial)
        });

        assert!(t.is_dirty(active_pt(0, 0)));
        assert!(t.is_dirty(active_pt(0, 1)));
        assert!(t.is_dirty(active_pt(0, 2)));
        assert!(t.is_dirty(active_pt(0, 3)));
        assert!(t.is_dirty(active_pt(0, 4)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "2BBBB\n3CCCC\n4DDDD\n5EEEE".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteLines (legacy)"
    #[test]
    fn terminal_delete_lines_legacy() {
        let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

        // Initial value
        t.print('A' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('B' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('C' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('D' as u32).unwrap();

        t.cursor_up(2);
        t.delete_lines(1);

        t.print('E' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();

        // We should be
        assert_eq!(0usize, unsafe { (*t.screens.active).cursor.x } as usize);
        assert_eq!(2usize, unsafe { (*t.screens.active).cursor.y } as usize);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "A\nE\nD".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteLines with scroll region, cursor outside of region"
    #[test]
    fn terminal_delete_lines_with_scroll_region_cursor_outside_of_region() {
        let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

        // Initial value
        t.print('A' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('B' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('C' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('D' as u32).unwrap();

        t.set_top_and_bottom_margin(1, 3);
        t.set_cursor_pos(4, 1);

        t.clear_dirty();
        t.delete_lines(1);

        for y in 0..4 {
            assert!(!t.is_dirty(active_pt(0, y)));
        }

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "A\nB\nC\nD".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteLines resets pending wrap"
    #[test]
    fn terminal_delete_lines_resets_pending_wrap() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABCDE".chars() {
            t.print(c as u32).unwrap();
        }
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
        t.delete_lines(1);
        assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });
        t.print('B' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "B".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteLines resets wrap"
    #[test]
    fn terminal_delete_lines_resets_wrap() {
        let mut t = Terminal::init(Global, opts(3, 3)).unwrap();

        t.print('1' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        for c in "ABCDEF".chars() {
            t.print(c as u32).unwrap();
        }

        t.set_top_and_bottom_margin(1, 2);
        t.set_cursor_pos(1, 1);
        t.delete_lines(1);
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "XBC\n\nDEF".as_bytes());
        }

        for y in 0..t.rows {
            let list_cell = t
                .active_mut()
                .pages
                .get_cell(active_pt(0, y as u32))
                .unwrap();
            let row = list_cell.row;
            assert!(!unsafe { (*row).wrap() });
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteLines left/right scroll region"
    #[test]
    fn terminal_delete_lines_left_right_scroll_region() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        t.print_string("ABC123".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DEF456".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GHI789".as_bytes()).unwrap();
        t.scrolling_region.left = 1;
        t.scrolling_region.right = 3;
        t.set_cursor_pos(2, 2);

        t.clear_dirty();
        t.delete_lines(1);

        assert!(!t.is_dirty(active_pt(0, 0)));
        for y in 1..3 {
            assert!(t.is_dirty(active_pt(0, y)));
        }

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABC123\nDHI756\nG   89".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteLines left/right scroll region from top"
    #[test]
    fn terminal_delete_lines_left_right_scroll_region_from_top() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        t.print_string("ABC123".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DEF456".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GHI789".as_bytes()).unwrap();
        t.scrolling_region.left = 1;
        t.scrolling_region.right = 3;
        t.set_cursor_pos(1, 2);

        t.clear_dirty();
        t.delete_lines(1);

        for y in 0..3 {
            assert!(t.is_dirty(active_pt(0, y)));
        }

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "AEF423\nDHI756\nG   89".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteLines left/right scroll region high count"
    #[test]
    fn terminal_delete_lines_left_right_scroll_region_high_count() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        t.print_string("ABC123".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DEF456".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GHI789".as_bytes()).unwrap();
        t.scrolling_region.left = 1;
        t.scrolling_region.right = 3;
        t.set_cursor_pos(2, 2);

        t.clear_dirty();
        t.delete_lines(100);

        assert!(!t.is_dirty(active_pt(0, 0)));
        for y in 1..3 {
            assert!(t.is_dirty(active_pt(0, y)));
        }

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABC123\nD   56\nG   89".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteLines wide character spacer head"
    #[test]
    fn terminal_delete_lines_wide_character_spacer_head() {
        let mut t = Terminal::init(Global, opts(5, 3)).unwrap();

        // Initial value
        // +-----+
        // |AAAAA| < Wrapped
        // |BBBB*| < Wrapped     (continued)
        // |WWCCC| < Non-wrapped (continued)
        // +-----+
        // where * represents a spacer head cell
        // and WW is the wide character.
        t.print_string("AAAAABBBB\u{1F600}CCC".as_bytes()).unwrap();

        // Delete the top line
        // +-----+
        // |BBBB | < Non-wrapped
        // |WWCCC| < Non-wrapped
        // |     | < Non-wrapped
        // +-----+
        // This should convert the spacer head to
        // a regular empty cell, and un-set wrap.
        t.set_cursor_pos(1, 1);
        t.delete_lines(1);

        {
            let str = t.plain_string(Global).unwrap();
            let unwrapped_str = t.plain_string_unwrapped(Global).unwrap();
            assert_eq!(str.as_slice(), "BBBB\n\u{1F600}CCC".as_bytes());
            assert_eq!(unwrapped_str.as_slice(), "BBBB\n\u{1F600}CCC".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteLines wide character spacer head left scroll margin"
    #[test]
    fn terminal_delete_lines_wide_character_spacer_head_left_scroll_margin() {
        let mut t = Terminal::init(Global, opts(5, 3)).unwrap();

        // Initial value
        // +-----+
        // |AAAAA| < Wrapped
        // |BBBB*| < Wrapped     (continued)
        // |WWCCC| < Non-wrapped (continued)
        // +-----+
        // where * represents a spacer head cell
        // and WW is the wide character.
        t.print_string("AAAAABBBB\u{1F600}CCC".as_bytes()).unwrap();

        t.scrolling_region.left = 2;

        // Delete the top line
        //    ###  <- scrolling region
        // +-----+
        // |AABB | < Wrapped
        // |BBCCC| < Wrapped     (continued)
        // |WW   | < Non-wrapped (continued)
        // +-----+
        // This should convert the spacer head to
        // a regular empty cell, but due to the
        // left scrolling margin, wrap state should
        // remain.
        t.set_cursor_pos(1, 3);
        t.delete_lines(1);

        {
            let str = t.plain_string(Global).unwrap();
            let unwrapped_str = t.plain_string_unwrapped(Global).unwrap();
            assert_eq!(str.as_slice(), "AABB\nBBCCC\n\u{1F600}".as_bytes());
            assert_eq!(unwrapped_str.as_slice(), "AABB BBCCC\u{1F600}".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteLines wide character spacer head right scroll margin"
    #[test]
    fn terminal_delete_lines_wide_character_spacer_head_right_scroll_margin() {
        let mut t = Terminal::init(Global, opts(5, 3)).unwrap();

        // Initial value
        // +-----+
        // |AAAAA| < Wrapped
        // |BBBB*| < Wrapped     (continued)
        // |WWCCC| < Non-wrapped (continued)
        // +-----+
        // where * represents a spacer head cell
        // and WW is the wide character.
        t.print_string("AAAAABBBB\u{1F600}CCC".as_bytes()).unwrap();

        t.scrolling_region.right = 3;

        // Delete the top line
        //  ####   <- scrolling region
        // +-----+
        // |BBBBA| < Wrapped
        // |WWCC | < Wrapped     (continued)
        // |    C| < Non-wrapped (continued)
        // +-----+
        // This should convert the spacer head to
        // a regular empty cell, but due to the
        // right scrolling margin, wrap state should
        // remain.
        t.set_cursor_pos(1, 1);
        t.delete_lines(1);

        {
            let str = t.plain_string(Global).unwrap();
            let unwrapped_str = t.plain_string_unwrapped(Global).unwrap();
            assert_eq!(str.as_slice(), "BBBBA\n\u{1F600}CC\n    C".as_bytes());
            assert_eq!(
                unwrapped_str.as_slice(),
                "BBBBA\u{1F600}CC     C".as_bytes()
            );
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteLines wide character spacer head left and right scroll margin"
    #[test]
    fn terminal_delete_lines_wide_character_spacer_head_left_and_right_scroll_margin() {
        let mut t = Terminal::init(Global, opts(5, 3)).unwrap();

        // Initial value
        // +-----+
        // |AAAAA| < Wrapped
        // |BBBB*| < Wrapped     (continued)
        // |WWCCC| < Non-wrapped (continued)
        // +-----+
        // where * represents a spacer head cell
        // and WW is the wide character.
        t.print_string("AAAAABBBB\u{1F600}CCC".as_bytes()).unwrap();

        t.scrolling_region.right = 3;
        t.scrolling_region.left = 2;

        // Delete the top line
        //    ##   <- scrolling region
        // +-----+
        // |AABBA| < Wrapped
        // |BBCC*| < Wrapped     (continued)
        // |WW  C| < Non-wrapped (continued)
        // +-----+
        // Because there is both a left scrolling
        // margin > 1 and a right scrolling margin
        // the spacer head should remain, and the
        // wrap state should be untouched.
        t.set_cursor_pos(1, 3);
        t.delete_lines(1);

        {
            let str = t.plain_string(Global).unwrap();
            let unwrapped_str = t.plain_string_unwrapped(Global).unwrap();
            assert_eq!(str.as_slice(), "AABBA\nBBCC\n\u{1F600}  C".as_bytes());
            assert_eq!(unwrapped_str.as_slice(), "AABBABBCC\u{1F600}  C".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteLines wide character spacer head left (< 2) and right scroll margin"
    #[test]
    fn terminal_delete_lines_wide_character_spacer_head_left_2_and_right_scroll_margin() {
        let mut t = Terminal::init(Global, opts(5, 3)).unwrap();

        // Initial value
        // +-----+
        // |AAAAA| < Wrapped
        // |BBBB*| < Wrapped     (continued)
        // |WWCCC| < Non-wrapped (continued)
        // +-----+
        // where * represents a spacer head cell
        // and WW is the wide character.
        t.print_string("AAAAABBBB\u{1F600}CCC".as_bytes()).unwrap();

        t.scrolling_region.right = 3;
        t.scrolling_region.left = 1;

        // Delete the top line
        //   ###   <- scrolling region
        // +-----+
        // |ABBBA| < Wrapped
        // |B CC | < Wrapped     (continued)
        // |    C| < Non-wrapped (continued)
        // +-----+
        // Because the left margin is 1, the wide
        // char is split, and therefore removed,
        // along with the spacer head - however,
        // wrap state should be untouched.
        t.set_cursor_pos(1, 2);
        t.delete_lines(1);

        {
            let str = t.plain_string(Global).unwrap();
            let unwrapped_str = t.plain_string_unwrapped(Global).unwrap();
            assert_eq!(str.as_slice(), "ABBBA\nB CC\n    C".as_bytes());
            assert_eq!(unwrapped_str.as_slice(), "ABBBAB CC     C".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteLines wide characters split by left/right scroll region boundaries"
    #[test]
    fn terminal_delete_lines_wide_characters_split_by_left_right_scroll_region_boundaries() {
        let mut t = Terminal::init(Global, opts(5, 2)).unwrap();

        // Initial value
        // +-----+
        // |AAAAA|
        // |WWBWW|
        // +-----+
        // where WW represents a wide character
        t.print_string("AAAAA\n\u{1F600}B\u{1F600}".as_bytes())
            .unwrap();

        t.scrolling_region.right = 3;
        t.scrolling_region.left = 1;

        // Delete the top line
        //   ###   <- scrolling region
        // +-----+
        // |A B A|
        // |     |
        // +-----+
        // The two wide chars, because they're
        // split by the edge of the scrolling
        // region, get removed.
        t.set_cursor_pos(1, 2);
        t.delete_lines(1);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "A B A".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteLines zero"
    #[test]
    fn terminal_delete_lines_zero() {
        let mut t = Terminal::init(Global, opts(2, 5)).unwrap();

        // This should do nothing
        t.set_cursor_pos(1, 1);
        t.delete_lines(0);

        t.deinit(Global);
    }

    // Reproduces a crash found by AFL++ fuzzer (afl-out/stream/default/crashes/
    // id:000007,sig:06,src:004522). The crash is a page integrity violation
    // "spacer tail not following wide" triggered during scrollUp -> deleteLines
    // -> clearCells. When deleteLines count >= scroll region height, all rows
    // are cleared (no shifting), so rowWillBeShifted is never called and wide
    // characters straddling the right margin boundary leave orphaned spacer_tails.
    // test "Terminal: deleteLines wide char at right margin with full clear"
    #[test]
    fn terminal_delete_lines_wide_char_at_right_margin_with_full_clear() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        // Place a wide character at col 39 (1-indexed) on several rows.
        // The wide cell lands at col 38 (0-indexed) with spacer_tail at col 39.
        t.set_cursor_pos(10, 39);
        t.print(0x4E2D).unwrap(); // '中'

        // Set left/right scroll margins so scrolling_region.right = 38.
        // clearCells will clear cells[4..39], which includes the wide cell
        // at col 38 but NOT the spacer_tail at col 39.
        t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
        t.set_left_and_right_margin(5, 39);

        // scrollUp with count >= region height causes deleteLines to clear
        // ALL rows without any shifting, so rowWillBeShifted is never called
        // and the orphaned spacer_tail at col 39 triggers a page integrity
        // violation in clearCells.
        t.scroll_up(t.rows as usize).unwrap();

        t.deinit(Global);
    }

    // test "Terminal: insertBlanks preserves background sgr"
    #[test]
    fn terminal_insert_blanks_preserves_background_sgr() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        for c in "ABC".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 1);
        t.set_attribute(sgr::Attribute::DirectColorBg(color::RGB {
            r: 0xFF,
            g: 0,
            b: 0,
        }))
        .unwrap();
        t.insert_blanks(2);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "  ABC".as_bytes());
        }
        {
            let list_cell = t.active_mut().pages.get_cell(active_pt(0, 0)).unwrap();
            let cell = list_cell.cell;
            assert!(unsafe { (*cell).content_tag() } == pagepkg::ContentTag::BgColorRgb);
            assert_eq!(
                pagepkg::CellRgb {
                    r: 0xFF,
                    g: 0,
                    b: 0,
                },
                unsafe { (*cell).content_color_rgb() }
            );
        }

        t.deinit(Global);
    }

    // test "Terminal: insertBlanks left/right scroll region large count"
    #[test]
    fn terminal_insert_blanks_left_right_scroll_region_large_count() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        t.modes.set(modespkg::Mode::Origin, true);
        t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
        t.set_left_and_right_margin(3, 5);
        t.set_cursor_pos(1, 1);
        t.clear_dirty();
        t.insert_blanks(140);
        assert!(t.is_dirty(active_pt(0, 0)));
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "  X".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: insertBlanks deleting graphemes"
    #[test]
    fn terminal_insert_blanks_deleting_graphemes() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        // Disable grapheme clustering
        t.modes.set(modespkg::Mode::GraphemeCluster, true);

        t.print_string("ABC".as_bytes()).unwrap();

        // This is: 👨‍👩‍👧 (which may or may not render correctly)
        t.print(0x1F468).unwrap();
        t.print(0x200D).unwrap();
        t.print(0x1F469).unwrap();
        t.print(0x200D).unwrap();
        t.print(0x1F467).unwrap();

        // We should have one cell with graphemes
        let page =
            unsafe { crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node) };
        assert_eq!(1usize, unsafe { (*page).grapheme_count() });

        t.set_cursor_pos(1, 1);
        t.clear_dirty();
        t.insert_blanks(4);
        assert!(t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "    A".as_bytes());
        }

        // We should have no graphemes
        assert_eq!(0usize, unsafe { (*page).grapheme_count() });

        t.deinit(Global);
    }

    // test "Terminal: insertBlanks shift graphemes"
    #[test]
    fn terminal_insert_blanks_shift_graphemes() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        // Enable grapheme clustering
        t.modes.set(modespkg::Mode::GraphemeCluster, true);

        t.print_string("A".as_bytes()).unwrap();

        // This is: 👨‍👩‍👧 (which may or may not render correctly)
        t.print(0x1F468).unwrap();
        t.print(0x200D).unwrap();
        t.print(0x1F469).unwrap();
        t.print(0x200D).unwrap();
        t.print(0x1F467).unwrap();

        // We should have one cell with graphemes
        let page =
            unsafe { crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node) };
        assert_eq!(1usize, unsafe { (*page).grapheme_count() });

        t.set_cursor_pos(1, 1);
        t.clear_dirty();
        t.insert_blanks(1);
        assert!(t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), " A👨\u{200D}👩\u{200D}👧".as_bytes());
        }

        // We should have no graphemes
        assert_eq!(1usize, unsafe { (*page).grapheme_count() });

        t.deinit(Global);
    }

    // test "Terminal: insertBlanks split multi-cell character from tail"
    #[test]
    fn terminal_insert_blanks_split_multi_cell_character_from_tail() {
        let mut t = Terminal::init(Global, opts(5, 10)).unwrap();

        t.print_string("橋123".as_bytes()).unwrap();
        t.set_cursor_pos(1, 2);
        t.insert_blanks(1);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "   12".as_bytes());
        }

        t.deinit(Global);
    }

    #[allow(non_snake_case)]
    // test "Terminal: insertLines hyperlink-dense row crosses page boundary"
    #[test]
    fn terminal_insertLines_hyperlink_dense_row_crosses_page_boundary() {
        // Regression test for the cross-page copy of insertLines: when the
        // shifted row carries more unique hyperlinks than the destination
        // page's hyperlink capacity, the copy must increase the destination
        // page's capacity and retry rather than corrupting the page list.
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

        // Fill the first page so it is exactly full, then two more rows so
        // the second page holds the last two active rows (y=3 and y=4).
        let first_page_rows =
            unsafe { crate::pagelist::Node::capacity(t.active_mut().pages.pages.first).rows };
        for _ in 0..(first_page_rows + 1) {
            t.linefeed().unwrap();
        }
        assert!(!std::ptr::eq(
            t.active_mut().pages.pages.first,
            t.active_mut().pages.pages.last
        ));
        assert_eq!(2, unsafe {
            crate::pagelist::Node::rows(t.active_mut().pages.pages.last)
        });

        // Marker rows so we can verify the shift afterwards.
        t.set_cursor_pos(1, 1);
        t.print_string(b"0").unwrap();
        t.set_cursor_pos(2, 1);
        t.print_string(b"1").unwrap();
        t.set_cursor_pos(4, 1);
        t.print_string(b"3").unwrap();
        t.set_cursor_pos(5, 1);
        t.print_string(b"4").unwrap();

        // Fill the last row of the first page (active y=2) with unique
        // hyperlinks: more than the second page can hold with its default
        // hyperlink capacity. Writing them grows the first page's capacity
        // as needed; the second page keeps its default capacity.
        t.set_cursor_pos(3, 1);
        for i in 0..10u32 {
            let uri = format!("http://example.com/{i}").into_bytes();
            t.active_mut().start_hyperlink(&uri, None).unwrap();
            t.print('A' as u32 + i).unwrap();
            t.active_mut().end_hyperlink();
        }
        {
            let pin = t
                .active_mut()
                .pages
                .pin(point::Point::Active(point::Coordinate { x: 0, y: 2 }))
                .unwrap();
            assert!(std::ptr::eq(t.active_mut().pages.pages.first, pin.node));
            assert_eq!(
                unsafe { crate::pagelist::Node::rows(pin.node) } as usize - 1,
                pin.y as usize
            );
        }
        assert!(
            unsafe {
                (*crate::pagelist::Node::page(t.active_mut().pages.pages.last))
                    .hyperlink_set
                    .layout
                    .cap
            } < 10,
            "the destination page must NOT already fit the dense row, or the retry never runs"
        );

        // Insert a line at the top: every row shifts down by one and the
        // dense row crosses the page boundary into the second page.
        t.set_cursor_pos(1, 1);
        t.insert_lines(1);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), b"\n0\n1\nABCDEFGHIJ\n3");
        }

        // The second page's hyperlink capacity had to grow to receive the
        // row, proving the capacity-retry path ran.
        assert!(
            unsafe {
                (*crate::pagelist::Node::page(t.active_mut().pages.pages.last))
                    .hyperlink_set
                    .layout
                    .cap
            } >= 10
        );

        // Every cell of the dense row must still resolve to a real
        // hyperlink entry with the correct URI. A half-applied shift
        // leaves cells whose hyperlink flag is set but that have no map
        // entry, which aborts in clearCells later.
        for x in 0..10u16 {
            let list_cell = t
                .active_mut()
                .pages
                .get_cell(point::Point::Active(point::Coordinate { x, y: 3 }))
                .unwrap();
            assert!(unsafe { (*list_cell.cell).hyperlink() });
            let page: *mut Page = unsafe { crate::pagelist::Node::page(list_cell.node) };
            let id = unsafe { (*page).lookup_hyperlink(list_cell.cell) }.unwrap();
            let link = unsafe { (*page).hyperlink_set.get((*page).memory_ptr(), id) };
            let uri = format!("http://example.com/{x}").into_bytes();
            assert_eq!(uri, unsafe {
                (*link).uri.slice((*page).memory_ptr()).to_vec()
            });
        }

        // All pages must pass integrity checks.
        let mut node_ = t.active_mut().pages.pages.first;
        while !node_.is_null() {
            unsafe { (*crate::pagelist::Node::page(node_)).assert_integrity() };
            node_ = unsafe { (*node_).next };
        }

        t.deinit(Global);
    }

    #[allow(non_snake_case)]
    // test "Terminal: deleteLines hyperlink-dense row crosses page boundary"
    #[test]
    fn terminal_deleteLines_hyperlink_dense_row_crosses_page_boundary() {
        // Regression test for the cross-page copy of deleteLines: when the
        // shifted row carries more unique hyperlinks than the destination
        // page's hyperlink capacity, the copy must increase the destination
        // page's capacity and retry rather than corrupting the page list.
        //
        // This is the mirror of the insertLines variant: the dense row
        // starts as the first row of the second page and is pulled up into
        // the first page, which also happens to be the cursor's page so
        // this exercises the cursor accounting of the capacity increase.
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

        let first_page_rows =
            unsafe { crate::pagelist::Node::capacity(t.active_mut().pages.pages.first).rows };
        for _ in 0..(first_page_rows + 1) {
            t.linefeed().unwrap();
        }
        assert!(!std::ptr::eq(
            t.active_mut().pages.pages.first,
            t.active_mut().pages.pages.last
        ));
        assert_eq!(2, unsafe {
            crate::pagelist::Node::rows(t.active_mut().pages.pages.last)
        });

        // Marker rows so we can verify the shift afterwards.
        t.set_cursor_pos(1, 1);
        t.print_string(b"0").unwrap();
        t.set_cursor_pos(2, 1);
        t.print_string(b"1").unwrap();
        t.set_cursor_pos(3, 1);
        t.print_string(b"2").unwrap();
        t.set_cursor_pos(5, 1);
        t.print_string(b"4").unwrap();

        // Fill the first row of the second page (active y=3) with unique
        // hyperlinks: more than the first page can hold with its default
        // hyperlink capacity.
        t.set_cursor_pos(4, 1);
        for i in 0..10u32 {
            let uri = format!("http://example.com/{i}").into_bytes();
            t.active_mut().start_hyperlink(&uri, None).unwrap();
            t.print('A' as u32 + i).unwrap();
            t.active_mut().end_hyperlink();
        }
        {
            let pin = t
                .active_mut()
                .pages
                .pin(point::Point::Active(point::Coordinate { x: 0, y: 3 }))
                .unwrap();
            assert!(std::ptr::eq(t.active_mut().pages.pages.last, pin.node));
            assert_eq!(0, pin.y);
        }
        assert!(
            unsafe {
                (*crate::pagelist::Node::page(t.active_mut().pages.pages.first))
                    .hyperlink_set
                    .layout
                    .cap
            } < 10,
            "the destination page must NOT already fit the dense row, or the retry never runs"
        );

        // Delete the top line: every row shifts up by one and the dense
        // row crosses the page boundary into the first page.
        t.set_cursor_pos(1, 1);
        t.delete_lines(1);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), b"1\n2\nABCDEFGHIJ\n4");
        }

        // The first page's hyperlink capacity had to grow to receive the
        // row, proving the capacity-retry path ran.
        assert!(
            unsafe {
                (*crate::pagelist::Node::page(t.active_mut().pages.pages.first))
                    .hyperlink_set
                    .layout
                    .cap
            } >= 10
        );

        for x in 0..10u16 {
            let list_cell = t
                .active_mut()
                .pages
                .get_cell(point::Point::Active(point::Coordinate { x, y: 2 }))
                .unwrap();
            assert!(unsafe { (*list_cell.cell).hyperlink() });
            let page: *mut Page = unsafe { crate::pagelist::Node::page(list_cell.node) };
            let id = unsafe { (*page).lookup_hyperlink(list_cell.cell) }.unwrap();
            let link = unsafe { (*page).hyperlink_set.get((*page).memory_ptr(), id) };
            let uri = format!("http://example.com/{x}").into_bytes();
            assert_eq!(uri, unsafe {
                (*link).uri.slice((*page).memory_ptr()).to_vec()
            });
        }

        let mut node_ = t.active_mut().pages.pages.first;
        while !node_.is_null() {
            unsafe { (*crate::pagelist::Node::page(node_)).assert_integrity() };
            node_ = unsafe { (*node_).next };
        }

        t.deinit(Global);
    }

    // test "Terminal: insertBlanks shifts hyperlinks"
    #[test]
    fn terminal_insert_blanks_shifts_hyperlinks() {
        // osc "8;;http://example.com"
        // printf "link"
        // printf "\r"
        // csi "3@"
        // echo
        //
        // link should be preserved, blanks should not be linked

        let mut t = Terminal::init(Global, opts(10, 2)).unwrap();

        t.active_mut()
            .start_hyperlink("http://example.com".as_bytes(), None)
            .unwrap();
        t.print_string("ABC".as_bytes()).unwrap();
        t.set_cursor_pos(1, 1);
        t.insert_blanks(2);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "  ABC".as_bytes());
        }

        // Verify all our cells have a hyperlink
        for x in 2..5 {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(x, 0)).unwrap();
            let row = list_cell.row;
            assert!(unsafe { (*row).hyperlink() });
            let cell = list_cell.cell;
            assert!(unsafe { (*cell).hyperlink() });
            let id =
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell) }
                    .unwrap();
            assert_eq!(id, 1 as crate::hyperlink::Id);
        }
        for x in 0..2 {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(x, 0)).unwrap();
            let cell = list_cell.cell;
            assert!(!unsafe { (*cell).hyperlink() });
            let id =
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell) };
            assert!(id.is_none());
        }

        t.deinit(Global);
    }

    // test "Terminal: insertBlanks pushes hyperlink off end completely"
    #[test]
    fn terminal_insert_blanks_pushes_hyperlink_off_end_completely() {
        let mut t = Terminal::init(Global, opts(3, 2)).unwrap();

        t.active_mut()
            .start_hyperlink("http://example.com".as_bytes(), None)
            .unwrap();
        t.print_string("ABC".as_bytes()).unwrap();
        t.set_cursor_pos(1, 1);
        t.insert_blanks(3);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "".as_bytes());
        }

        for x in 0..3 {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(x, 0)).unwrap();
            let row = list_cell.row;
            assert!(!unsafe { (*row).hyperlink() });
            let cell = list_cell.cell;
            assert!(!unsafe { (*cell).hyperlink() });
            let id =
                unsafe { (*crate::pagelist::Node::page(list_cell.node)).lookup_hyperlink(cell) };
            assert!(id.is_none());
        }

        t.deinit(Global);
    }

    // test "Terminal: insertBlanks wide char straddling right margin"
    #[test]
    fn terminal_insert_blanks_wide_char_straddling_right_margin() {
        // Crash found by AFL++ fuzzer.
        //
        // When a wide character straddles the right scroll margin (head at the
        // margin, spacer_tail just beyond it), insertBlanks shifts the wide head
        // away via swapCells but leaves the orphaned spacer_tail in place,
        // causing a page integrity violation.
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

        // Fill row: A B C D 橋 _ _ _ _ _
        // Positions: 0 1 2 3 4W 5T 6 7 8 9
        t.set_cursor_pos(1, 1);
        for c in "ABCD".chars() {
            t.print(c as u32).unwrap();
        }
        t.print('橋' as u32).unwrap(); // wide char: head at 4, spacer_tail at 5

        // Set right margin so the wide head is AT the boundary and the
        // spacer_tail is just outside it.
        t.scrolling_region.right = 4;

        // Position cursor at x=2 (1-indexed col 3) and insert one blank.
        // This triggers the swap loop which displaces the wide head at
        // position 4 without clearing the spacer_tail at position 5.
        t.set_cursor_pos(1, 3);
        t.insert_blanks(1);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "AB CD".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: insertBlanks wide char spacer_tail orphaned beyond right margin"
    #[test]
    fn terminal_insert_blanks_wide_char_spacer_tail_orphaned_beyond_right_margin() {
        // Regression test for AFL++ crash.
        //
        // When insertBlanks clears the entire region from cursor to the right
        // margin (scroll_amount == 0), a wide character whose head is AT the
        // right margin gets cleared but its spacer_tail just beyond the margin
        // is left behind, causing a page integrity violation:
        //   "spacer tail not following wide"
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

        // Fill cols 0–9 with wide chars: 中中中中中
        // Positions: 0W 1T 2W 3T 4W 5T 6W 7T 8W 9T
        for _ in 0..5 {
            t.print(0x4E2D).unwrap();
        }

        // Set left/right margins so that the last wide char (cols 8–9)
        // straddles the boundary: head at col 8 (inside), tail at col 9 (outside).
        t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
        t.set_left_and_right_margin(1, 9); // 1-indexed: left=0, right=8

        // Cursor is now at (0, 0) after DECSLRM.  Print a narrow char to
        // advance cursor to col 1.
        t.print('a' as u32).unwrap();

        // ICH 8: insert 8 blanks at cursor x=1.
        // rem = right(8) - x(1) + 1 = 8, adjusted_count = 8, scroll_amount = 0.
        // The code clears cols 1–8 without noticing the spacer_tail at col 9.
        t.insert_blanks(8);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "a".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: insert mode with space"
    #[test]
    fn terminal_insert_mode_with_space() {
        let mut t = Terminal::init(Global, opts(10, 2)).unwrap();

        for c in "hello".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 2);
        t.modes.set(modespkg::Mode::Insert, true);
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "hXello".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: insert mode doesn't wrap pushed characters"
    #[test]
    fn terminal_insert_mode_doesn_t_wrap_pushed_characters() {
        let mut t = Terminal::init(Global, opts(5, 2)).unwrap();

        for c in "hello".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 2);
        t.modes.set(modespkg::Mode::Insert, true);
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "hXell".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: insert mode does nothing at the end of the line"
    #[test]
    fn terminal_insert_mode_does_nothing_at_the_end_of_the_line() {
        let mut t = Terminal::init(Global, opts(5, 2)).unwrap();

        for c in "hello".chars() {
            t.print(c as u32).unwrap();
        }
        t.modes.set(modespkg::Mode::Insert, true);
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "hello\nX".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: insert mode with wide characters"
    #[test]
    fn terminal_insert_mode_with_wide_characters() {
        let mut t = Terminal::init(Global, opts(5, 2)).unwrap();

        for c in "hello".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 2);
        t.modes.set(modespkg::Mode::Insert, true);
        t.print('😀' as u32).unwrap(); // 0x1F600

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "h😀el".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: insert mode with wide characters at end"
    #[test]
    fn terminal_insert_mode_with_wide_characters_at_end() {
        let mut t = Terminal::init(Global, opts(5, 2)).unwrap();

        for c in "well".chars() {
            t.print(c as u32).unwrap();
        }
        t.modes.set(modespkg::Mode::Insert, true);
        t.print('😀' as u32).unwrap(); // 0x1F600

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "well\n😀".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: insert mode pushing off wide character"
    #[test]
    fn terminal_insert_mode_pushing_off_wide_character() {
        let mut t = Terminal::init(Global, opts(5, 2)).unwrap();

        for c in "123".chars() {
            t.print(c as u32).unwrap();
        }
        t.print('😀' as u32).unwrap(); // 0x1F600
        t.modes.set(modespkg::Mode::Insert, true);
        t.set_cursor_pos(1, 1);
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "X123".as_bytes());
        }

        t.deinit(Global);
    }

    // PORT NOTE: NOT a port of a reference `test {}` block. This is the
    // page-pool tracking-allocator gate for this batch's page-touching methods
    // (scroll_up/scroll_down/insert_lines/delete_lines, which delegate row
    // movement and scrollback creation to `pagelist`). Under `cfg(test)` the
    // page pool's backing allocator records into the global `page_tracking`
    // tracker; this test drives multi-page scroll + insert churn and asserts
    // every page allocated during the scope is returned after `deinit` (no
    // page leak across the tracked-pin / page-split paths).
    #[cfg_attr(
        all(miri, debug_assertions),
        ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
    )]
    #[test]
    fn terminal_pagepool_no_leak_scroll_insert_multi_page() {
        let scope = crate::pagelist::page_tracking::PageLeakScope::new();
        {
            let mut t = Terminal::init(Global, opts(20, 5)).unwrap();

            // Fill enough rows (with scrollback) to force several pages of
            // churn through the scroll path.
            for i in 0..400u32 {
                let line = format!("row{i:04}");
                t.print_string(line.as_bytes()).unwrap();
                t.carriage_return();
                t.linefeed().unwrap();
            }

            // scroll_up at the full-screen top creates scrollback (cursorScrollAbove)
            for _ in 0..50 {
                t.scroll_up(1).unwrap();
            }

            // scroll_down delegates to insert_lines.
            for _ in 0..20 {
                t.scroll_down(1);
            }

            // Drive insert/delete lines with a scroll region too.
            t.set_top_and_bottom_margin(1, 4);
            t.set_cursor_pos(1, 1);
            for _ in 0..30 {
                t.insert_lines(2);
                t.delete_lines(1);
            }

            // We should have produced real page churn.
            assert!(
                scope.delta_allocs() > 0,
                "expected scroll/insert churn to allocate at least one new page"
            );

            t.deinit(Global);
        }
        // After deinit every page allocated during this scope must be returned.
        scope.assert_returned_to_baseline();
    }

    // PORT NOTE: NOT a port of a reference `test {}` block. This is the
    // page-pool tracking-allocator gate for this batch's `resize` method, which
    // delegates the active-screen resize (reflow) to `Screen::resize` ->
    // `pagelist` (page split/merge + scrollback trimming). Under `cfg(test)` the
    // page pool's backing allocator records into the global `page_tracking`
    // tracker; this test drives resize grow + shrink churn (with reflow) and
    // asserts every page allocated during the scope is returned after `deinit`
    // (no page leak across the resize page-churn path).
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): TIMEOUT in BOTH Miri lanes — re-measured per test in single-worker isolation at a 6-minute bound with `-C debug-assertions=off`, so removing the integrity assertions does not rescue it; the cost is page-mapping/LZ4 volume, not assertion overhead. Covered natively + by ASan."
    )]
    #[test]
    fn terminal_pagepool_no_leak_resize_grow() {
        let scope = crate::pagelist::page_tracking::PageLeakScope::new();
        {
            let alloc = Global;
            let mut t = Terminal::init(alloc, opts(20, 5)).unwrap();

            // Enable wraparound so resize reflows soft-wrapped content (driving
            // page split/merge through the reflow path).
            t.modes.set(modespkg::Mode::Wraparound, true);

            // Fill enough wrapped content to force several pages of churn.
            for i in 0..400u32 {
                let line = format!("row{i:04}-padding-text-to-wrap");
                t.print_string(line.as_bytes()).unwrap();
                t.carriage_return();
                t.linefeed().unwrap();
            }

            // Grow then shrink columns repeatedly to drive reflow page churn.
            for _ in 0..10 {
                t.resize(
                    alloc,
                    Resize {
                        cols: 60,
                        rows: 5,
                        cell_size_px: None,
                    },
                )
                .unwrap();
                t.resize(
                    alloc,
                    Resize {
                        cols: 8,
                        rows: 5,
                        cell_size_px: None,
                    },
                )
                .unwrap();
                t.resize(
                    alloc,
                    Resize {
                        cols: 20,
                        rows: 5,
                        cell_size_px: None,
                    },
                )
                .unwrap();
            }

            // We should have produced real page churn.
            assert!(
                scope.delta_allocs() > 0,
                "expected resize reflow churn to allocate at least one new page"
            );

            t.deinit(alloc);
        }
        // After deinit every page allocated during this scope must be returned.
        scope.assert_returned_to_baseline();
    }

    // ===================================================================
    // vt-12b-4: SGR (setAttribute / printAttributes) + protected-mode tests
    // (the groups deferred from vt-12b-3a/3b that drive `set_attribute` /
    //  `set_protected_mode`, plus the eraseDisplay group now unblocked).
    // ===================================================================

    // test "Terminal: bold style"
    #[test]
    fn terminal_bold_style() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.set_attribute(sgr::Attribute::Bold).unwrap();
        t.print('A' as u32).unwrap();

        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 'A' as u21);
            assert!(unsafe { (*cell).style_id() } != 0);
            let page =
                unsafe { crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node) };
            assert!(
                unsafe {
                    (*page)
                        .styles
                        .ref_count((*page).memory_ptr(), (*t.screens.active).cursor.style_id)
                } > 1
            );
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseChars preserves background sgr"
    #[test]
    fn terminal_erase_chars_preserves_background_sgr() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        for c in "ABC".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 1);
        t.set_attribute(sgr::Attribute::DirectColorBg(color::RGB {
            r: 0xFF,
            g: 0,
            b: 0,
        }))
        .unwrap();
        t.erase_chars(2);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "  C".as_bytes());
            {
                let list_cell = t.active_mut().pages.get_cell(active_pt(0, 0)).unwrap();
                let cell = list_cell.cell;
                assert!(unsafe { (*cell).content_tag() } == pagepkg::ContentTag::BgColorRgb);
                assert_eq!(
                    pagepkg::CellRgb {
                        r: 0xFF,
                        g: 0,
                        b: 0,
                    },
                    unsafe { (*cell).content_color_rgb() }
                );
            }
            {
                let list_cell = t.active_mut().pages.get_cell(active_pt(1, 0)).unwrap();
                let cell = list_cell.cell;
                assert!(unsafe { (*cell).content_tag() } == pagepkg::ContentTag::BgColorRgb);
                assert_eq!(
                    pagepkg::CellRgb {
                        r: 0xFF,
                        g: 0,
                        b: 0,
                    },
                    unsafe { (*cell).content_color_rgb() }
                );
            }
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseChars handles refcounted styles"
    #[test]
    fn terminal_erase_chars_handles_refcounted_styles() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        t.set_attribute(sgr::Attribute::Bold).unwrap();
        t.print('A' as u32).unwrap();
        t.print('B' as u32).unwrap();
        t.set_attribute(sgr::Attribute::Unset).unwrap();
        t.print('C' as u32).unwrap();

        // verify we have styles in our style map
        let page =
            unsafe { crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node) };
        assert_eq!(1usize, unsafe { (*page).styles.count() });

        t.set_cursor_pos(1, 1);
        t.erase_chars(2);

        // verify we have no styles in our style map
        assert_eq!(0usize, unsafe { (*page).styles.count() });

        t.deinit(Global);
    }

    // test "Terminal: eraseChars protected attributes respected with iso"
    #[test]
    fn terminal_erase_chars_protected_attributes_respected_with_iso() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.set_protected_mode(ansi::ProtectedMode::Iso);
        for c in "ABC".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 1);
        t.erase_chars(2);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABC".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseChars protected attributes ignored with dec most recent"
    #[test]
    fn terminal_erase_chars_protected_attributes_ignored_with_dec_most_recent() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.set_protected_mode(ansi::ProtectedMode::Iso);
        for c in "ABC".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_protected_mode(ansi::ProtectedMode::Dec);
        t.set_protected_mode(ansi::ProtectedMode::Off);
        t.set_cursor_pos(1, 1);
        t.erase_chars(2);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "  C".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseChars protected attributes ignored with dec set"
    #[test]
    fn terminal_erase_chars_protected_attributes_ignored_with_dec_set() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.set_protected_mode(ansi::ProtectedMode::Dec);
        for c in "ABC".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 1);
        t.erase_chars(2);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "  C".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteChars preserves background sgr"
    #[test]
    fn terminal_delete_chars_preserves_background_sgr() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        for c in "ABC123".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 3);
        t.set_attribute(sgr::Attribute::DirectColorBg(color::RGB {
            r: 0xFF,
            g: 0,
            b: 0,
        }))
        .unwrap();
        t.delete_chars(2);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "AB23".as_bytes());
        }
        for x in t.cols - 2..t.cols {
            let list_cell = t.active_mut().pages.get_cell(active_pt(x, 0)).unwrap();
            let cell = list_cell.cell;
            assert!(unsafe { (*cell).content_tag() } == pagepkg::ContentTag::BgColorRgb);
            assert_eq!(
                pagepkg::CellRgb {
                    r: 0xFF,
                    g: 0,
                    b: 0,
                },
                unsafe { (*cell).content_color_rgb() }
            );
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseLine right preserves background sgr"
    #[test]
    fn terminal_erase_line_right_preserves_background_sgr() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABCDE".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 2);
        t.set_attribute(sgr::Attribute::DirectColorBg(color::RGB {
            r: 0xFF,
            g: 0,
            b: 0,
        }))
        .unwrap();
        t.erase_line(csi::EraseLine::Right, false);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "A".as_bytes());
            for x in 1..5 {
                let list_cell = t.active_mut().pages.get_cell(active_pt(x, 0)).unwrap();
                let cell = list_cell.cell;
                assert!(unsafe { (*cell).content_tag() } == pagepkg::ContentTag::BgColorRgb);
                assert_eq!(
                    pagepkg::CellRgb {
                        r: 0xFF,
                        g: 0,
                        b: 0,
                    },
                    unsafe { (*cell).content_color_rgb() }
                );
            }
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseLine right protected attributes respected with iso"
    #[test]
    fn terminal_erase_line_right_protected_attributes_respected_with_iso() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.set_protected_mode(ansi::ProtectedMode::Iso);
        for c in "ABC".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 1);
        t.clear_dirty();
        t.erase_line(csi::EraseLine::Right, false);
        assert!(t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABC".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseLine right protected attributes ignored with dec most recent"
    #[test]
    fn terminal_erase_line_right_protected_attributes_ignored_with_dec_most_recent() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.set_protected_mode(ansi::ProtectedMode::Iso);
        for c in "ABC".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_protected_mode(ansi::ProtectedMode::Dec);
        t.set_protected_mode(ansi::ProtectedMode::Off);
        t.set_cursor_pos(1, 2);
        t.clear_dirty();
        t.erase_line(csi::EraseLine::Right, false);
        assert!(t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "A".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseLine right protected attributes ignored with dec set"
    #[test]
    fn terminal_erase_line_right_protected_attributes_ignored_with_dec_set() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.set_protected_mode(ansi::ProtectedMode::Dec);
        for c in "ABC".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 2);
        t.clear_dirty();
        t.erase_line(csi::EraseLine::Right, false);
        assert!(t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "A".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseLine right protected requested"
    #[test]
    fn terminal_erase_line_right_protected_requested() {
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

        for c in "12345678".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos((unsafe { (*t.screens.active).cursor.y } as usize) + 1, 6);
        t.set_protected_mode(ansi::ProtectedMode::Dec);
        t.print('X' as u32).unwrap();
        t.set_cursor_pos((unsafe { (*t.screens.active).cursor.y } as usize) + 1, 4);
        t.clear_dirty();
        t.erase_line(csi::EraseLine::Right, true);
        assert!(t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "123  X".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseLine left preserves background sgr"
    #[test]
    fn terminal_erase_line_left_preserves_background_sgr() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABCDE".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 2);
        t.set_attribute(sgr::Attribute::DirectColorBg(color::RGB {
            r: 0xFF,
            g: 0,
            b: 0,
        }))
        .unwrap();
        t.erase_line(csi::EraseLine::Left, false);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "  CDE".as_bytes());
            for x in 0..2 {
                let list_cell = t.active_mut().pages.get_cell(active_pt(x, 0)).unwrap();
                let cell = list_cell.cell;
                assert!(unsafe { (*cell).content_tag() } == pagepkg::ContentTag::BgColorRgb);
                assert_eq!(
                    pagepkg::CellRgb {
                        r: 0xFF,
                        g: 0,
                        b: 0,
                    },
                    unsafe { (*cell).content_color_rgb() }
                );
            }
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseLine left protected attributes respected with iso"
    #[test]
    fn terminal_erase_line_left_protected_attributes_respected_with_iso() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.set_protected_mode(ansi::ProtectedMode::Iso);
        for c in "ABC".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 1);
        t.clear_dirty();
        t.erase_line(csi::EraseLine::Left, false);
        assert!(t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABC".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseLine left protected attributes ignored with dec most recent"
    #[test]
    fn terminal_erase_line_left_protected_attributes_ignored_with_dec_most_recent() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.set_protected_mode(ansi::ProtectedMode::Iso);
        for c in "ABC".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_protected_mode(ansi::ProtectedMode::Dec);
        t.set_protected_mode(ansi::ProtectedMode::Off);
        t.set_cursor_pos(1, 2);
        t.clear_dirty();
        t.erase_line(csi::EraseLine::Left, false);
        assert!(t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "  C".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseLine left protected attributes ignored with dec set"
    #[test]
    fn terminal_erase_line_left_protected_attributes_ignored_with_dec_set() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.set_protected_mode(ansi::ProtectedMode::Dec);
        for c in "ABC".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 2);
        t.clear_dirty();
        t.erase_line(csi::EraseLine::Left, false);
        assert!(t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "  C".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseLine left protected requested"
    #[test]
    fn terminal_erase_line_left_protected_requested() {
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

        for c in "123456789".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos((unsafe { (*t.screens.active).cursor.y } as usize) + 1, 6);
        t.set_protected_mode(ansi::ProtectedMode::Dec);
        t.print('X' as u32).unwrap();
        t.set_cursor_pos((unsafe { (*t.screens.active).cursor.y } as usize) + 1, 8);
        t.clear_dirty();
        t.erase_line(csi::EraseLine::Left, true);
        assert!(t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "     X  9".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseLine complete preserves background sgr"
    #[test]
    fn terminal_erase_line_complete_preserves_background_sgr() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABCDE".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 2);
        t.set_attribute(sgr::Attribute::DirectColorBg(color::RGB {
            r: 0xFF,
            g: 0,
            b: 0,
        }))
        .unwrap();
        t.erase_line(csi::EraseLine::Complete, false);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "".as_bytes());
            for x in 0..5 {
                let list_cell = t.active_mut().pages.get_cell(active_pt(x, 0)).unwrap();
                let cell = list_cell.cell;
                assert!(unsafe { (*cell).content_tag() } == pagepkg::ContentTag::BgColorRgb);
                assert_eq!(
                    pagepkg::CellRgb {
                        r: 0xFF,
                        g: 0,
                        b: 0,
                    },
                    unsafe { (*cell).content_color_rgb() }
                );
            }
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseLine complete resets wrap"
    #[test]
    fn terminal_erase_line_complete_resets_wrap() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        // for ("ABCDE123") |c| try t.print(c);
        for c in "ABCDE123".chars() {
            t.print(c as u32).unwrap();
        }
        {
            let list_cell = t.active_mut().pages.get_cell(active_pt(0, 0)).unwrap();
            // try testing.expect(list_cell.row.wrap);
            assert!(unsafe { (*list_cell.row).wrap() });
        }

        t.set_cursor_pos(1, 1);
        t.erase_line(csi::EraseLine::Complete, false);

        {
            let list_cell = t.active_mut().pages.get_cell(active_pt(0, 0)).unwrap();
            // try testing.expect(!list_cell.row.wrap);
            assert!(!unsafe { (*list_cell.row).wrap() });
        }
        t.print('X' as u32).unwrap();
        // try t.resize(alloc, .{ .rows = 5, .cols = 10 });
        t.resize(
            alloc,
            Resize {
                rows: 5,
                cols: 10,
                cell_size_px: None,
            },
        )
        .unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "X\n123".as_bytes());
        }

        t.deinit(alloc);
    }

    // test "Terminal: eraseLine complete protected attributes respected with iso"
    #[test]
    fn terminal_erase_line_complete_protected_attributes_respected_with_iso() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.set_protected_mode(ansi::ProtectedMode::Iso);
        for c in "ABC".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 1);
        t.clear_dirty();
        t.erase_line(csi::EraseLine::Complete, false);
        assert!(t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABC".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseLine complete protected attributes ignored with dec most recent"
    #[test]
    fn terminal_erase_line_complete_protected_attributes_ignored_with_dec_most_recent() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.set_protected_mode(ansi::ProtectedMode::Iso);
        for c in "ABC".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_protected_mode(ansi::ProtectedMode::Dec);
        t.set_protected_mode(ansi::ProtectedMode::Off);
        t.set_cursor_pos(1, 2);
        t.clear_dirty();
        t.erase_line(csi::EraseLine::Complete, false);
        assert!(t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseLine complete protected attributes ignored with dec set"
    #[test]
    fn terminal_erase_line_complete_protected_attributes_ignored_with_dec_set() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.set_protected_mode(ansi::ProtectedMode::Dec);
        for c in "ABC".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 2);
        t.clear_dirty();
        t.erase_line(csi::EraseLine::Complete, false);
        assert!(t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseLine complete protected requested"
    #[test]
    fn terminal_erase_line_complete_protected_requested() {
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

        for c in "123456789".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos((unsafe { (*t.screens.active).cursor.y } as usize) + 1, 6);
        t.set_protected_mode(ansi::ProtectedMode::Dec);
        t.print('X' as u32).unwrap();
        t.set_cursor_pos((unsafe { (*t.screens.active).cursor.y } as usize) + 1, 8);
        t.clear_dirty();
        t.erase_line(csi::EraseLine::Complete, true);
        assert!(t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "     X".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: printSlice simple ascii"
    #[test]
    fn terminal_print_slice_simple_ascii() {
        let mut t = Terminal::init(Global, opts(10, 3)).unwrap();

        t.print_slice(&['h' as u32, 'e' as u32, 'l' as u32, 'l' as u32, 'o' as u32])
            .unwrap();
        assert_eq!(5, unsafe { (*t.screens.active).cursor.x });
        assert_eq!('o' as u32, t.previous_char.unwrap());
        assert!(t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "hello".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: printSlice wraps and scrolls"
    #[test]
    fn terminal_print_slice_wraps_and_scrolls() {
        let mut t = Terminal::init(Global, opts(5, 2)).unwrap();

        // 12 chars: fills row 1 (5), row 2 (5), wraps+scrolls, 2 more.
        t.print_slice(&[
            'a' as u32, 'b' as u32, 'c' as u32, 'd' as u32, 'e' as u32, 'f' as u32, 'g' as u32,
            'h' as u32, 'i' as u32, 'j' as u32, 'k' as u32, 'l' as u32,
        ])
        .unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "fghij\nkl".as_bytes());
        }
        assert_eq!(2, unsafe { (*t.screens.active).cursor.x });
        assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });

        t.deinit(Global);
    }

    // test "Terminal: printSlice pending wrap state"
    #[test]
    fn terminal_print_slice_pending_wrap_state() {
        let mut t = Terminal::init(Global, opts(5, 2)).unwrap();

        t.print_slice(&['a' as u32, 'b' as u32, 'c' as u32, 'd' as u32, 'e' as u32])
            .unwrap();
        assert_eq!(4, unsafe { (*t.screens.active).cursor.x });
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "abcde".as_bytes());
        }

        t.deinit(Global);
    }

    /// Differential testing helper: applies the same logical print
    /// operations to two terminals, one using per-codepoint print() and
    /// the other using printSlice() with random chunking, verifying that
    /// the results are identical.
    //
    // PORT NOTE: `std.Random.DefaultPrng` + `rand.intRangeAtMost` become a
    // seeded LCG plus a `range` helper below. The generator differs from Zig's
    // xoshiro, so the exact operation sequence differs from upstream's — the
    // test's value is the differential invariant (print vs print_slice must
    // agree after EVERY op), which holds for any deterministic sequence, and
    // the seeds are fixed so failures are reproducible.
    fn test_print_slice_differential(
        rand: &mut TestRand,
        ops: usize,
        cols: size::CellCountInt,
        rows: size::CellCountInt,
    ) {
        let mut t1 = Terminal::init(Global, opts(cols, rows)).unwrap();
        let mut t2 = Terminal::init(Global, opts(cols, rows)).unwrap();

        // Alphabet of interesting codepoints: ascii, latin-1, combining
        // marks, CJK (wide), emoji (wide), ZWJ, variation selectors.
        let alphabet: [u32; 40] = [
            'a' as u32,
            'b' as u32,
            'Z' as u32,
            '0' as u32,
            ' ' as u32,
            0x10,
            0x1F,
            0x7F,
            'é' as u32,
            0xFF,
            0x301,
            0x4E00,
            0x4E01,
            0x1F600,
            0x200D,
            0xFE0F,
            'x' as u32,
            'y' as u32,
            0x1F9D1,
            0x0308,
            0xAD,
            0x3042,
            0xAC00,
            'q' as u32,
            'r' as u32,
            's' as u32,
            't' as u32,
            'u' as u32,
            'v' as u32,
            'w' as u32,
            '1' as u32,
            '2' as u32,
            0x1F1E6,
            0x1F1E7,
            0x1100,
            0x1161,
            0x11A8,
            0x200C,
            0x0430,
            0x03B1,
        ];

        let mut cps_buf: [u32; 64] = [0; 64];
        let mut last_n: usize = 0;

        for _ in 0..ops {
            match rand.range(0, 20) {
                // Print a run of codepoints (most common op).
                0..=9 => {
                    let n = rand.range(1, cps_buf.len());
                    last_n = n;
                    for cp in cps_buf[0..n].iter_mut() {
                        *cp = alphabet[rand.range_less_than(alphabet.len())];
                    }

                    // t1: per-codepoint print
                    for &cp in &cps_buf[0..n] {
                        t1.print(cp).unwrap();
                    }

                    // t2: printSlice with random chunking
                    let mut i: usize = 0;
                    while i < n {
                        let chunk = rand.range(1, n - i);
                        t2.print_slice(&cps_buf[i..i + chunk]).unwrap();
                        i += chunk;
                    }
                }
                10 => {
                    t1.carriage_return();
                    t2.carriage_return();
                    t1.linefeed().unwrap();
                    t2.linefeed().unwrap();
                }
                11 => {
                    let row = rand.range(1, rows as usize);
                    let col = rand.range(1, cols as usize);
                    t1.set_cursor_pos(row, col);
                    t2.set_cursor_pos(row, col);
                }
                12 => {
                    let attr: sgr::Attribute = match rand.range(0, 3) {
                        0 => sgr::Attribute::Unset,
                        1 => sgr::Attribute::Bold,
                        2 => sgr::Attribute::DirectColorFg(color::RGB {
                            r: rand.byte(),
                            g: rand.byte(),
                            b: rand.byte(),
                        }),
                        3 => sgr::Attribute::Color8Fg(color::Name::Red),
                        _ => unreachable!(),
                    };
                    t1.set_attribute(attr).unwrap();
                    t2.set_attribute(attr).unwrap();
                }
                13 => {
                    let v = rand.boolean();
                    t1.modes.set(modespkg::Mode::Insert, v);
                    t2.modes.set(modespkg::Mode::Insert, v);
                }
                14 => {
                    let v = rand.boolean();
                    t1.modes.set(modespkg::Mode::Wraparound, v);
                    t2.modes.set(modespkg::Mode::Wraparound, v);
                }
                15 => {
                    let v = rand.boolean();
                    // PORT NOTE (oracle b287f6d1a, `P3-W1 T4`): the
                    // `erase_display` workaround that used to sit here is
                    // DELETED, exactly as upstream deletes it in
                    // `testPrintSliceDifferential`. It existed to dodge the
                    // `debug_assert!(!brk)` in `print`'s stored-cluster walk,
                    // which that commit removes. The differential now exercises
                    // live mode-2027 changes over existing screen content, which
                    // is the coverage the workaround was suppressing.
                    t1.modes.set(modespkg::Mode::GraphemeCluster, v);
                    t2.modes.set(modespkg::Mode::GraphemeCluster, v);
                }
                16 => {
                    // Margins.
                    t1.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
                    t2.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
                    let left = rand.range(1, cols as usize / 2);
                    let right = rand.range(cols as usize / 2, cols as usize);
                    t1.set_left_and_right_margin(left, right);
                    t2.set_left_and_right_margin(left, right);
                }
                17 => {
                    t1.set_left_and_right_margin(0, 0);
                    t2.set_left_and_right_margin(0, 0);
                }
                18 => {
                    t1.active_mut()
                        .start_hyperlink(b"http://example.com", None)
                        .unwrap();
                    t2.active_mut()
                        .start_hyperlink(b"http://example.com", None)
                        .unwrap();
                }
                19 => {
                    t1.active_mut().end_hyperlink();
                    t2.active_mut().end_hyperlink();
                }
                20 => {
                    let set: charsets::Charset = if rand.boolean() {
                        charsets::Charset::DecSpecial
                    } else {
                        charsets::Charset::Utf8
                    };
                    t1.configure_charset(charsets::Slots::G0, set);
                    t2.configure_charset(charsets::Slots::G0, set);
                }
                _ => unreachable!(),
            }

            // Cursor state must match exactly after every op.
            assert_eq!(unsafe { (*t1.screens.active).cursor.x }, unsafe {
                (*t2.screens.active).cursor.x
            });
            assert_eq!(unsafe { (*t1.screens.active).cursor.y }, unsafe {
                (*t2.screens.active).cursor.y
            });
            assert_eq!(
                unsafe { (*t1.screens.active).cursor.pending_wrap },
                unsafe { (*t2.screens.active).cursor.pending_wrap }
            );

            // Full screen contents must match after every op. On failure,
            // dump diagnostics that make the failure reproducible.
            {
                let str1 = t1
                    .active_mut()
                    .dump_string_alloc(Global, point::Point::Screen(point::Coordinate::default()))
                    .unwrap();
                let str2 = t2
                    .active_mut()
                    .dump_string_alloc(Global, point::Point::Screen(point::Coordinate::default()))
                    .unwrap();
                if str1 != str2 {
                    eprintln!("last print cps: {:?}", &cps_buf[0..last_n]);
                    eprintln!(
                        "modes: 2027={} insert={} wrap={} sr.left={} sr.right={} cols={}",
                        t1.modes.get(modespkg::Mode::GraphemeCluster),
                        t1.modes.get(modespkg::Mode::Insert),
                        t1.modes.get(modespkg::Mode::Wraparound),
                        t1.scrolling_region.left,
                        t1.scrolling_region.right,
                        cols,
                    );
                    assert_eq!(
                        String::from_utf8_lossy(&str1),
                        String::from_utf8_lossy(&str2)
                    );
                }
            }
        }

        // Page integrity (styles refcounts, grapheme maps, etc.) must hold.
        unsafe {
            let p1 = crate::pagelist::Node::page((*(*t1.screens.active).cursor.page_pin).node);
            (*p1).verify_integrity().unwrap();
            let p2 = crate::pagelist::Node::page((*(*t2.screens.active).cursor.page_pin).node);
            (*p2).verify_integrity().unwrap();
        }

        t1.deinit(Global);
        t2.deinit(Global);
    }

    /// Deterministic PRNG standing in for `std.Random.DefaultPrng` (see the
    /// PORT NOTE on `test_print_slice_differential`).
    struct TestRand(u64);

    impl TestRand {
        fn init(seed: u64) -> TestRand {
            TestRand(seed)
        }
        fn next(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            self.0 >> 17
        }
        // rand.intRangeAtMost(T, lo, hi)
        fn range(&mut self, lo: usize, hi: usize) -> usize {
            if hi <= lo {
                return lo;
            }
            lo + (self.next() as usize) % (hi - lo + 1)
        }
        // rand.intRangeLessThan(T, 0, n)
        fn range_less_than(&mut self, n: usize) -> usize {
            (self.next() as usize) % n
        }
        // rand.boolean()
        fn boolean(&mut self) -> bool {
            self.next() & 1 == 1
        }
        // rand.int(u8)
        fn byte(&mut self) -> u8 {
            self.next() as u8
        }
    }

    // test "Terminal: printSlice differential fuzz vs print"
    #[test]
    // MIRI: 1700 randomized ops across four terminal geometries, each op
    // followed by two full-screen dumps and a cursor comparison, plus two
    // `verify_integrity` page walks at the end. Measured: killed at >40 min of
    // interpreter time against this repo's 6-minute hard bound (the three
    // sibling `terminal_print_slice_*` tests finish in ~10s each and DO run
    // under Miri, covering the same `print_slice_fill` raw-pointer writes).
    // Registered in .port/phase-2/MIRI_EXCEPTIONS.md (WS10 T16).
    #[cfg_attr(miri, ignore)]
    fn terminal_print_slice_differential_fuzz_vs_print() {
        // Multiple seeds and terminal sizes for coverage, including a
        // tiny terminal to stress wrap/scroll edge cases.
        let mut prng = TestRand::init(0xC0FFEE);
        test_print_slice_differential(&mut prng, 500, 80, 24);
        test_print_slice_differential(&mut prng, 500, 10, 4);
        test_print_slice_differential(&mut prng, 500, 5, 2);
        test_print_slice_differential(&mut prng, 200, 2, 2);
    }

    // test "Terminal: printAttributes"
    #[test]
    fn terminal_print_attributes() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        let mut storage: [u8; 64] = [0; 64];

        {
            t.set_attribute(sgr::Attribute::DirectColorFg(color::RGB {
                r: 1,
                g: 2,
                b: 3,
            }))
            .unwrap();
            let buf = t.print_attributes(&mut storage).unwrap();
            assert_eq!(buf, "0;38:2::1:2:3".as_bytes());
            t.set_attribute(sgr::Attribute::Unset).unwrap();
        }

        {
            t.set_attribute(sgr::Attribute::Bold).unwrap();
            t.set_attribute(sgr::Attribute::DirectColorBg(color::RGB {
                r: 1,
                g: 2,
                b: 3,
            }))
            .unwrap();
            let buf = t.print_attributes(&mut storage).unwrap();
            assert_eq!(buf, "0;1;48:2::1:2:3".as_bytes());
            t.set_attribute(sgr::Attribute::Unset).unwrap();
        }

        {
            t.set_attribute(sgr::Attribute::Bold).unwrap();
            t.set_attribute(sgr::Attribute::Faint).unwrap();
            t.set_attribute(sgr::Attribute::Italic).unwrap();
            t.set_attribute(sgr::Attribute::Underline(sgr::Underline::Single))
                .unwrap();
            t.set_attribute(sgr::Attribute::Blink).unwrap();
            t.set_attribute(sgr::Attribute::Inverse).unwrap();
            t.set_attribute(sgr::Attribute::Invisible).unwrap();
            t.set_attribute(sgr::Attribute::Strikethrough).unwrap();
            t.set_attribute(sgr::Attribute::Overline).unwrap();
            t.set_attribute(sgr::Attribute::DirectColorFg(color::RGB {
                r: 100,
                g: 200,
                b: 255,
            }))
            .unwrap();
            t.set_attribute(sgr::Attribute::DirectColorBg(color::RGB {
                r: 101,
                g: 102,
                b: 103,
            }))
            .unwrap();
            let buf = t.print_attributes(&mut storage).unwrap();
            assert_eq!(
                buf,
                "0;1;2;3;4;53;5;7;8;9;38:2::100:200:255;48:2::101:102:103".as_bytes()
            );
            t.set_attribute(sgr::Attribute::Unset).unwrap();
        }

        // PORT NOTE (oracle cb2fef390, `P3-W1 T6`): the single-underline case
        // that used to stand alone here is now case 1 of a 5-case table. It is
        // deliberately RETAINED rather than replaced: without an
        // `underline == Single` case the test would pass even if the `;4:{n}`
        // branch were taken unconditionally.
        for (underline, expected) in [
            (sgr::Underline::Single, "0;4"),
            (sgr::Underline::Double, "0;4:2"),
            (sgr::Underline::Curly, "0;4:3"),
            (sgr::Underline::Dotted, "0;4:4"),
            (sgr::Underline::Dashed, "0;4:5"),
        ] {
            t.set_attribute(sgr::Attribute::Underline(underline))
                .unwrap();
            let buf = t.print_attributes(&mut storage).unwrap();
            assert_eq!(buf, expected.as_bytes(), "underline {underline:?}");
        }

        t.set_attribute(sgr::Attribute::Unset).unwrap();
        {
            let buf = t.print_attributes(&mut storage).unwrap();
            assert_eq!(buf, "0".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseDisplay simple erase below"
    #[test]
    fn terminal_erase_display_simple_erase_below() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABC".chars() {
            t.print(c as u32).unwrap();
        }
        t.carriage_return();
        t.linefeed().unwrap();
        for c in "DEF".chars() {
            t.print(c as u32).unwrap();
        }
        t.carriage_return();
        t.linefeed().unwrap();
        for c in "GHI".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(2, 2);

        t.clear_dirty();
        t.erase_display(csi::EraseDisplay::Below, false);

        assert!(!t.is_dirty(active_pt(0, 0)));
        assert!(t.is_dirty(active_pt(0, 1)));
        assert!(t.is_dirty(active_pt(0, 2)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABC\nD".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseDisplay erase below preserves SGR bg"
    #[test]
    fn terminal_erase_display_erase_below_preserves_sgr_bg() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABC".chars() {
            t.print(c as u32).unwrap();
        }
        t.carriage_return();
        t.linefeed().unwrap();
        for c in "DEF".chars() {
            t.print(c as u32).unwrap();
        }
        t.carriage_return();
        t.linefeed().unwrap();
        for c in "GHI".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(2, 2);

        t.set_attribute(sgr::Attribute::DirectColorBg(color::RGB {
            r: 0xFF,
            g: 0,
            b: 0,
        }))
        .unwrap();
        t.erase_display(csi::EraseDisplay::Below, false);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABC\nD".as_bytes());
            for x in 1..5 {
                let list_cell = t.active_mut().pages.get_cell(active_pt(x, 1)).unwrap();
                let cell = list_cell.cell;
                assert!(unsafe { (*cell).content_tag() } == pagepkg::ContentTag::BgColorRgb);
                assert_eq!(
                    pagepkg::CellRgb {
                        r: 0xFF,
                        g: 0,
                        b: 0,
                    },
                    unsafe { (*cell).content_color_rgb() }
                );
            }
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseDisplay below split multi-cell"
    #[test]
    fn terminal_erase_display_below_split_multi_cell() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print_string("AB橋C".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DE橋F".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GH橋I".as_bytes()).unwrap();
        t.set_cursor_pos(2, 4);
        t.erase_display(csi::EraseDisplay::Below, false);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "AB橋C\nDE".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseDisplay below protected attributes respected with iso"
    #[test]
    fn terminal_erase_display_below_protected_attributes_respected_with_iso() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.set_protected_mode(ansi::ProtectedMode::Iso);
        for c in "ABC".chars() {
            t.print(c as u32).unwrap();
        }
        t.carriage_return();
        t.linefeed().unwrap();
        for c in "DEF".chars() {
            t.print(c as u32).unwrap();
        }
        t.carriage_return();
        t.linefeed().unwrap();
        for c in "GHI".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(2, 2);
        t.erase_display(csi::EraseDisplay::Below, false);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABC\nDEF\nGHI".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseDisplay below protected attributes ignored with dec most recent"
    #[test]
    fn terminal_erase_display_below_protected_attributes_ignored_with_dec_most_recent() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.set_protected_mode(ansi::ProtectedMode::Iso);
        for c in "ABC".chars() {
            t.print(c as u32).unwrap();
        }
        t.carriage_return();
        t.linefeed().unwrap();
        for c in "DEF".chars() {
            t.print(c as u32).unwrap();
        }
        t.carriage_return();
        t.linefeed().unwrap();
        for c in "GHI".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_protected_mode(ansi::ProtectedMode::Dec);
        t.set_protected_mode(ansi::ProtectedMode::Off);
        t.set_cursor_pos(2, 2);
        t.erase_display(csi::EraseDisplay::Below, false);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABC\nD".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseDisplay below protected attributes ignored with dec set"
    #[test]
    fn terminal_erase_display_below_protected_attributes_ignored_with_dec_set() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.set_protected_mode(ansi::ProtectedMode::Dec);
        for c in "ABC".chars() {
            t.print(c as u32).unwrap();
        }
        t.carriage_return();
        t.linefeed().unwrap();
        for c in "DEF".chars() {
            t.print(c as u32).unwrap();
        }
        t.carriage_return();
        t.linefeed().unwrap();
        for c in "GHI".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(2, 2);
        t.erase_display(csi::EraseDisplay::Below, false);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABC\nD".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseDisplay below protected attributes respected with force"
    #[test]
    fn terminal_erase_display_below_protected_attributes_respected_with_force() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.set_protected_mode(ansi::ProtectedMode::Dec);
        for c in "ABC".chars() {
            t.print(c as u32).unwrap();
        }
        t.carriage_return();
        t.linefeed().unwrap();
        for c in "DEF".chars() {
            t.print(c as u32).unwrap();
        }
        t.carriage_return();
        t.linefeed().unwrap();
        for c in "GHI".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(2, 2);
        t.erase_display(csi::EraseDisplay::Below, true);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABC\nDEF\nGHI".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseDisplay simple erase above"
    #[test]
    fn terminal_erase_display_simple_erase_above() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABC".chars() {
            t.print(c as u32).unwrap();
        }
        t.carriage_return();
        t.linefeed().unwrap();
        for c in "DEF".chars() {
            t.print(c as u32).unwrap();
        }
        t.carriage_return();
        t.linefeed().unwrap();
        for c in "GHI".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(2, 2);

        t.clear_dirty();
        t.erase_display(csi::EraseDisplay::Above, false);
        assert!(t.is_dirty(active_pt(0, 0)));
        assert!(t.is_dirty(active_pt(0, 1)));
        assert!(!t.is_dirty(active_pt(0, 2)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\n  F\nGHI".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseDisplay erase above preserves SGR bg"
    #[test]
    fn terminal_erase_display_erase_above_preserves_sgr_bg() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABC".chars() {
            t.print(c as u32).unwrap();
        }
        t.carriage_return();
        t.linefeed().unwrap();
        for c in "DEF".chars() {
            t.print(c as u32).unwrap();
        }
        t.carriage_return();
        t.linefeed().unwrap();
        for c in "GHI".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(2, 2);

        t.set_attribute(sgr::Attribute::DirectColorBg(color::RGB {
            r: 0xFF,
            g: 0,
            b: 0,
        }))
        .unwrap();
        t.erase_display(csi::EraseDisplay::Above, false);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\n  F\nGHI".as_bytes());
            for x in 0..2 {
                let list_cell = t.active_mut().pages.get_cell(active_pt(x, 1)).unwrap();
                let cell = list_cell.cell;
                assert!(unsafe { (*cell).content_tag() } == pagepkg::ContentTag::BgColorRgb);
                assert_eq!(
                    pagepkg::CellRgb {
                        r: 0xFF,
                        g: 0,
                        b: 0,
                    },
                    unsafe { (*cell).content_color_rgb() }
                );
            }
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseDisplay above split multi-cell"
    #[test]
    fn terminal_erase_display_above_split_multi_cell() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print_string("AB橋C".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DE橋F".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GH橋I".as_bytes()).unwrap();
        t.set_cursor_pos(2, 3);
        t.erase_display(csi::EraseDisplay::Above, false);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\n    F\nGH橋I".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseDisplay above protected attributes respected with iso"
    #[test]
    fn terminal_erase_display_above_protected_attributes_respected_with_iso() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.set_protected_mode(ansi::ProtectedMode::Iso);
        for c in "ABC".chars() {
            t.print(c as u32).unwrap();
        }
        t.carriage_return();
        t.linefeed().unwrap();
        for c in "DEF".chars() {
            t.print(c as u32).unwrap();
        }
        t.carriage_return();
        t.linefeed().unwrap();
        for c in "GHI".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(2, 2);
        t.erase_display(csi::EraseDisplay::Above, false);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABC\nDEF\nGHI".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseDisplay above protected attributes ignored with dec most recent"
    #[test]
    fn terminal_erase_display_above_protected_attributes_ignored_with_dec_most_recent() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.set_protected_mode(ansi::ProtectedMode::Iso);
        for c in "ABC".chars() {
            t.print(c as u32).unwrap();
        }
        t.carriage_return();
        t.linefeed().unwrap();
        for c in "DEF".chars() {
            t.print(c as u32).unwrap();
        }
        t.carriage_return();
        t.linefeed().unwrap();
        for c in "GHI".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_protected_mode(ansi::ProtectedMode::Dec);
        t.set_protected_mode(ansi::ProtectedMode::Off);
        t.set_cursor_pos(2, 2);
        t.erase_display(csi::EraseDisplay::Above, false);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\n  F\nGHI".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseDisplay above protected attributes ignored with dec set"
    #[test]
    fn terminal_erase_display_above_protected_attributes_ignored_with_dec_set() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.set_protected_mode(ansi::ProtectedMode::Dec);
        for c in "ABC".chars() {
            t.print(c as u32).unwrap();
        }
        t.carriage_return();
        t.linefeed().unwrap();
        for c in "DEF".chars() {
            t.print(c as u32).unwrap();
        }
        t.carriage_return();
        t.linefeed().unwrap();
        for c in "GHI".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(2, 2);
        t.erase_display(csi::EraseDisplay::Above, false);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\n  F\nGHI".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseDisplay above protected attributes respected with force"
    #[test]
    fn terminal_erase_display_above_protected_attributes_respected_with_force() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.set_protected_mode(ansi::ProtectedMode::Dec);
        for c in "ABC".chars() {
            t.print(c as u32).unwrap();
        }
        t.carriage_return();
        t.linefeed().unwrap();
        for c in "DEF".chars() {
            t.print(c as u32).unwrap();
        }
        t.carriage_return();
        t.linefeed().unwrap();
        for c in "GHI".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(2, 2);
        t.erase_display(csi::EraseDisplay::Above, true);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABC\nDEF\nGHI".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseDisplay protected complete"
    #[test]
    fn terminal_erase_display_protected_complete() {
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

        t.print('A' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        for c in "123456789".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos((unsafe { (*t.screens.active).cursor.y } as usize) + 1, 6);
        t.set_protected_mode(ansi::ProtectedMode::Dec);
        t.print('X' as u32).unwrap();
        t.set_cursor_pos((unsafe { (*t.screens.active).cursor.y } as usize) + 1, 4);

        t.clear_dirty();
        t.erase_display(csi::EraseDisplay::Complete, true);
        for y in 0..t.rows {
            assert!(t.is_dirty(active_pt(0, y as u32)));
        }

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\n     X".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseDisplay protected below"
    #[test]
    fn terminal_erase_display_protected_below() {
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

        t.print('A' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        for c in "123456789".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos((unsafe { (*t.screens.active).cursor.y } as usize) + 1, 6);
        t.set_protected_mode(ansi::ProtectedMode::Dec);
        t.print('X' as u32).unwrap();
        t.set_cursor_pos((unsafe { (*t.screens.active).cursor.y } as usize) + 1, 4);
        t.erase_display(csi::EraseDisplay::Below, true);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "A\n123  X".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseDisplay scroll complete"
    #[test]
    fn terminal_erase_display_scroll_complete() {
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

        t.print('A' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.erase_display(csi::EraseDisplay::ScrollComplete, false);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseDisplay protected above"
    #[test]
    fn terminal_erase_display_protected_above() {
        let mut t = Terminal::init(Global, opts(10, 3)).unwrap();

        t.print('A' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        for c in "123456789".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos((unsafe { (*t.screens.active).cursor.y } as usize) + 1, 6);
        t.set_protected_mode(ansi::ProtectedMode::Dec);
        t.print('X' as u32).unwrap();
        t.set_cursor_pos((unsafe { (*t.screens.active).cursor.y } as usize) + 1, 8);
        t.erase_display(csi::EraseDisplay::Above, true);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\n     X  9".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseDisplay complete preserves cursor"
    #[test]
    fn terminal_erase_display_complete_preserves_cursor() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        // Set our cursur
        t.set_attribute(sgr::Attribute::Bold).unwrap();
        t.print_string("AAAA".as_bytes()).unwrap();
        assert!(unsafe { (*t.screens.active).cursor.style_id } != style::DEFAULT_ID);

        // Erasing the display may detect that our style is no longer in use
        // and prune our style, which we don't want because its still our
        // active cursor.
        t.erase_display(csi::EraseDisplay::Complete, false);
        assert!(unsafe { (*t.screens.active).cursor.style_id } != style::DEFAULT_ID);

        t.deinit(Global);
    }

    // ===================================================================
    // vt-12b-5: Terminal modes/resize/switch tests (deccolm / resize /
    // switchScreen / switchScreenMode / fullReset), now unblocked.
    // ===================================================================

    // test "Terminal: saveCursor resize"
    #[test]
    fn terminal_save_cursor_resize() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

        t.set_cursor_pos(1, 10);
        t.save_cursor();
        t.resize(
            alloc,
            Resize {
                cols: 5,
                rows: 5,
                cell_size_px: None,
            },
        )
        .unwrap();
        t.restore_cursor();
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "    X".as_bytes());
        }

        t.deinit(alloc);
    }

    // test "Terminal: cursor defaults update current default cursor"
    #[test]
    fn terminal_cursor_defaults_update_current_default_cursor() {
        let mut o = opts(10, 10);
        o.default_cursor_style = crate::screen::CursorStyle::Bar;
        o.default_cursor_blink = Some(true);
        let mut t = Terminal::init(Global, o).unwrap();

        // Initialization applies the configured defaults.
        assert!(t.cursor.is_default);
        assert_eq!(crate::screen::CursorStyle::Bar, unsafe {
            (*t.screens.active).cursor.cursor_style
        });
        assert!(t.modes.get(modespkg::Mode::CursorBlinking));

        // Configuration changes are immediately visible while the cursor still
        // follows its defaults.
        t.set_default_cursor_style(crate::screen::CursorStyle::Underline);
        t.set_default_cursor_blink(Some(false));
        assert!(t.cursor.is_default);
        assert_eq!(crate::screen::CursorStyle::Underline, unsafe {
            (*t.screens.active).cursor.cursor_style
        });
        assert!(!t.modes.get(modespkg::Mode::CursorBlinking));

        // Null restores the terminal emulator's blinking default.
        t.set_default_cursor_blink(None);
        assert!(t.modes.get(modespkg::Mode::CursorBlinking));

        t.deinit(Global);
    }

    // test "Terminal: cursor defaults do not override explicit cursor"
    //
    // NEGATIVE CONTROL (P3-W4 T4 Step 4): the last block is upstream's RIS
    // round-trip — set a style via DECSCUSR, `full_reset`, and assert the
    // CONFIGURED default is restored, NOT the hardcoded initial one. A port that
    // leaves the restore in the handler (or drops it) fails here.
    #[test]
    fn terminal_cursor_defaults_do_not_override_explicit_cursor() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        t.set_cursor_style(ansi::CursorStyle::BlinkingBar);
        assert!(!t.cursor.is_default);
        assert_eq!(crate::screen::CursorStyle::Bar, unsafe {
            (*t.screens.active).cursor.cursor_style
        });
        assert!(t.modes.get(modespkg::Mode::CursorBlinking));

        // New defaults are retained without replacing the explicit appearance.
        t.set_default_cursor_style(crate::screen::CursorStyle::Underline);
        t.set_default_cursor_blink(Some(false));
        assert_eq!(
            crate::screen::CursorStyle::Underline,
            t.cursor.default_style
        );
        assert_eq!(Some(false), t.cursor.default_blink);
        assert_eq!(crate::screen::CursorStyle::Bar, unsafe {
            (*t.screens.active).cursor.cursor_style
        });
        assert!(t.modes.get(modespkg::Mode::CursorBlinking));

        // Selecting the default applies the values that changed above.
        t.set_cursor_style(ansi::CursorStyle::Default);
        assert!(t.cursor.is_default);
        assert_eq!(crate::screen::CursorStyle::Underline, unsafe {
            (*t.screens.active).cursor.cursor_style
        });
        assert!(!t.modes.get(modespkg::Mode::CursorBlinking));

        // A full reset also leaves the cursor on the configured defaults.
        t.set_cursor_style(ansi::CursorStyle::SteadyBlock);
        t.full_reset();
        assert!(t.cursor.is_default);
        assert_eq!(crate::screen::CursorStyle::Underline, unsafe {
            (*t.screens.active).cursor.cursor_style
        });
        assert!(!t.modes.get(modespkg::Mode::CursorBlinking));

        t.deinit(Global);
    }

    // test "Terminal: fullReset with a non-empty pen"
    #[test]
    fn terminal_full_reset_with_a_non_empty_pen() {
        let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

        t.set_attribute(sgr::Attribute::DirectColorFg(color::RGB {
            r: 0xFF,
            g: 0,
            b: 0x7F,
        }))
        .unwrap();
        t.set_attribute(sgr::Attribute::DirectColorBg(color::RGB {
            r: 0xFF,
            g: 0,
            b: 0x7F,
        }))
        .unwrap();
        unsafe { (*t.screens.active).cursor.semantic_content = pagepkg::SemanticContent::Input };
        t.full_reset();

        {
            let cursor_x = unsafe { (*t.screens.active).cursor.x };
            let cursor_y = unsafe { (*t.screens.active).cursor.y };
            let list_cell = t
                .active_mut()
                .pages
                .get_cell(active_pt(cursor_x, cursor_y as u32))
                .unwrap();
            let cell = list_cell.cell;
            assert!(unsafe { (*cell).style_id() } == 0);
        }

        assert_eq!(0 as style::Id, unsafe {
            (*t.screens.active).cursor.style_id
        });
        assert_eq!(pagepkg::SemanticContent::Output, unsafe {
            (*t.screens.active).cursor.semantic_content
        });

        t.deinit(Global);
    }

    // test "Terminal: fullReset hyperlink"
    #[test]
    fn terminal_full_reset_hyperlink() {
        let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

        t.active_mut()
            .start_hyperlink("http://example.com".as_bytes(), None)
            .unwrap();
        t.full_reset();
        assert_eq!(0, unsafe { (*t.screens.active).cursor.hyperlink_id });

        t.deinit(Global);
    }

    // test "Terminal: fullReset with a non-empty saved cursor"
    #[test]
    fn terminal_full_reset_with_a_non_empty_saved_cursor() {
        let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

        t.set_attribute(sgr::Attribute::DirectColorFg(color::RGB {
            r: 0xFF,
            g: 0,
            b: 0x7F,
        }))
        .unwrap();
        t.set_attribute(sgr::Attribute::DirectColorBg(color::RGB {
            r: 0xFF,
            g: 0,
            b: 0x7F,
        }))
        .unwrap();
        t.save_cursor();
        t.full_reset();

        {
            let cursor_x = unsafe { (*t.screens.active).cursor.x };
            let cursor_y = unsafe { (*t.screens.active).cursor.y };
            let list_cell = t
                .active_mut()
                .pages
                .get_cell(active_pt(cursor_x, cursor_y as u32))
                .unwrap();
            let cell = list_cell.cell;
            assert!(unsafe { (*cell).style_id() } == 0);
        }

        assert_eq!(0 as style::Id, unsafe {
            (*t.screens.active).cursor.style_id
        });

        t.deinit(Global);
    }

    // test "Terminal: fullReset origin mode"
    #[test]
    fn terminal_full_reset_origin_mode() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        t.set_cursor_pos(3, 5);
        t.modes.set(modespkg::Mode::Origin, true);
        t.full_reset();

        // Origin mode should be reset and the cursor should be moved
        assert_eq!(0_usize, unsafe { (*t.screens.active).cursor.y } as usize);
        assert_eq!(0_usize, unsafe { (*t.screens.active).cursor.x } as usize);
        assert!(!t.modes.get(modespkg::Mode::Origin));

        t.deinit(Global);
    }

    // test "Terminal: fullReset status display"
    #[test]
    fn terminal_full_reset_status_display() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        t.status_display = ansi::StatusDisplay::StatusLine;
        t.full_reset();
        assert!(t.status_display == ansi::StatusDisplay::Main);

        t.deinit(Global);
    }

    // test "Terminal: fullReset preserves kitty graphics limits"
    //
    #[test]
    #[cfg_attr(
        not(all(feature = "kitty-graphics", not(target_os = "none"))),
        ignore = "SkipZigTest: needs options::KITTY_GRAPHICS (R14)"
    )]
    fn terminal_full_reset_preserves_kitty_graphics_limits() {
        use crate::kitty::graphics_image::{Limits, TemporaryFileLimit};

        // if (comptime !build_options.kitty_graphics) return error.SkipZigTest;
        //
        // PORT NOTE: the `cfg_attr(.., ignore)` above is this crate's idiom for
        // `error.SkipZigTest` behind a build option (R14; see `dcs.rs`'s
        // `tmux_enter_and_implicit_exit`); `kitty_images` is only maintained
        // when the option is on.

        let temp_dir = b"/tmp/rustty-kitty-images";

        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        t.set_kitty_graphics_loading_limits(Limits::all_with_temp_dir(temp_dir));
        for total_limit in [1234_usize, 0] {
            t.set_kitty_graphics_size_limit(Global, total_limit);
            t.full_reset();

            // SAFETY: `screens.active` is the live primary screen for the whole
            // borrow; we only read its kitty storage.
            let storage = unsafe { &(*t.screens.active).kitty_images };
            assert_eq!(total_limit, storage.total_limit);
            assert!(storage.image_limits.file);
            assert!(storage.image_limits.shared_memory);
            match &storage.image_limits.temporary_file {
                TemporaryFileLimit::Enabled { directory } => {
                    assert_eq!(temp_dir.as_slice(), directory.as_slice());
                }
                TemporaryFileLimit::Disabled => panic!("temporary file limit was reset"),
            }
        }

        t.deinit(Global);
    }

    // https://github.com/mitchellh/ghostty/issues/1607
    // test "Terminal: fullReset clears alt screen kitty keyboard state"
    #[test]
    fn terminal_full_reset_clears_alt_screen_kitty_keyboard_state() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        t.switch_screen_mode(SwitchScreenMode::M1049, true).unwrap();
        unsafe {
            (*t.screens.active).kitty_keyboard.push(kitty::key::Flags {
                disambiguate: true,
                report_events: false,
                report_alternates: true,
                report_all: true,
                report_associated: true,
            });
        }
        t.switch_screen_mode(SwitchScreenMode::M1049, false)
            .unwrap();

        t.full_reset();
        assert!(t.screens.get(crate::screen_set::Key::Alternate).is_none());

        t.deinit(Global);
    }

    // test "Terminal: fullReset default modes"
    #[test]
    fn terminal_full_reset_default_modes() {
        // PORT NOTE: the reference `init(alloc, .{ .cols, .rows, .default_modes =
        // .{ .grapheme_cluster = true } })`. Our `opts` helper supplies the field
        // defaults, so we override `default_modes` after building it. The packed
        // bit-set helpers are private, so we build the `ModePacked` value via the
        // public `ModeState::set` and take its `values` (faithfully producing
        // `.{ .grapheme_cluster = true }`).
        let mut o = opts(10, 10);
        let mut ms = modespkg::ModeState::default();
        ms.set(modespkg::Mode::GraphemeCluster, true);
        o.default_modes = ms.values;
        let mut t = Terminal::init(Global, o).unwrap();
        assert!(t.modes.get(modespkg::Mode::GraphemeCluster));
        t.full_reset();
        assert!(t.modes.get(modespkg::Mode::GraphemeCluster));

        t.deinit(Global);
    }

    // test "Terminal: fullReset tracked pins"
    #[test]
    // WS18 release lane: this test drives a slow-runtime-safety mechanism (the
    // pagelist integrity gate / `pin_is_valid`), which the port compiles out when
    // `debug_assertions` is off — faithful to the reference's
    // `build_options.slow_runtime_safety`, under which the oracle's test only
    // exists. Debug-only; it still runs and passes there.
    #[cfg_attr(
        not(debug_assertions),
        ignore = "DEBUG-ONLY BY DESIGN, and this reason was MEASURED (fix round 1 of parity S166-F2): this test drives the slow-runtime-safety mechanism — `PageList::assert_integrity` / `pin_is_valid` — which the port compiles out when `debug_assertions` is off, faithful to the reference's `build_options.slow_runtime_safety`, under which the oracle's counterpart only exists. It is NOT ignored because of the tripwire gate (that claim was false and is gone): un-ignoring it turns the release lane RED, which is why it stays."
    )]
    fn terminal_full_reset_tracked_pins() {
        let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

        // Create a tracked pin
        let cursor_pin = unsafe { *(*t.screens.active).cursor.page_pin };
        let p = t.active_mut().pages.track_pin(cursor_pin).unwrap();
        t.full_reset();
        assert!(t.active_mut().pages.pin_is_valid(unsafe { *p }));

        t.deinit(Global);
    }

    // test "Terminal: glyph APC stores session glossary entries"
    #[test]
    #[cfg_attr(
        not(feature = "glyph-protocol"),
        ignore = "SkipZigTest: needs options::GLYPH_PROTOCOL (R14)"
    )]
    fn terminal_glyph_apc_stores_session_glossary_entries() {
        // if (comptime !build_options.glyph_protocol) return error.SkipZigTest;

        use crate::apc::glyph::response::{Coverage, Response, ResponseQuery, ResponseRegister};

        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(80, 24)).unwrap();

        // var register_parser = glyph.CommandParser.init(alloc, 1024 * 1024);
        // defer register_parser.deinit();
        let mut register_parser = glyph::CommandParser::init(1024 * 1024);
        // for ("r;cp=e0a0;AAAAAAAAAAAAAA==") |byte| try register_parser.feed(byte);
        for byte in *b"r;cp=e0a0;AAAAAAAAAAAAAA==" {
            register_parser.feed(byte).unwrap();
        }
        // var register_req = try register_parser.complete(alloc);
        // defer register_req.deinit(alloc);
        let register_req = register_parser.complete().unwrap();

        // try testing.expectEqual(glyph.Response{ .register = .{ .cp = 0xE0A0 } },
        //     t.glyphProtocol(alloc, &register_req).?);
        assert_eq!(
            Response::Register(ResponseRegister::new(0xE0A0)),
            t.glyph_protocol(alloc, &register_req).unwrap()
        );
        // try testing.expect(t.glyph_glossary.contains(0xE0A0));
        assert!(t.glyph_glossary.contains(0xE0A0));
        // try testing.expect(t.flags.dirty.glyph_glossary);
        assert!(t.flags.dirty.glyph_glossary);

        // var query_parser = glyph.CommandParser.init(alloc, 1024 * 1024);
        // defer query_parser.deinit();
        let mut query_parser = glyph::CommandParser::init(1024 * 1024);
        // for ("q;cp=e0a0") |byte| try query_parser.feed(byte);
        for byte in *b"q;cp=e0a0" {
            query_parser.feed(byte).unwrap();
        }
        // var query_req = try query_parser.complete(alloc);
        // defer query_req.deinit(alloc);
        let query_req = query_parser.complete().unwrap();

        // try testing.expectEqual(glyph.Response{ .query = .{
        //     .cp = 0xE0A0, .status = .{ .glossary = true } } },
        //     t.glyphProtocol(alloc, &query_req).?);
        assert_eq!(
            Response::Query(ResponseQuery {
                cp: 0xE0A0,
                status: Coverage {
                    glossary: true,
                    ..Coverage::default()
                },
            }),
            t.glyph_protocol(alloc, &query_req).unwrap()
        );

        // t.fullReset();
        t.full_reset();
        // try testing.expect(!t.glyph_glossary.contains(0xE0A0));
        assert!(!t.glyph_glossary.contains(0xE0A0));

        t.deinit(alloc);
    }

    // https://github.com/mitchellh/ghostty/issues/272
    // This is also tested in depth in screen resize tests but I want to keep
    // this test around to ensure we don't regress at multiple layers.
    // test "Terminal: resize less cols with wide char then print"
    #[test]
    fn terminal_resize_less_cols_with_wide_char_then_print() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();

        t.print('x' as u32).unwrap();
        t.print('😀' as u32).unwrap(); // 0x1F600
        t.resize(
            alloc,
            Resize {
                cols: 2,
                rows: 3,
                cell_size_px: None,
            },
        )
        .unwrap();
        t.set_cursor_pos(1, 2);
        t.print('😀' as u32).unwrap(); // 0x1F600

        t.deinit(alloc);
    }

    // test "Terminal: resize resets synchronized output"
    #[test]
    fn terminal_resize_resets_synchronized_output() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

        t.modes.set(modespkg::Mode::SynchronizedOutput, true);
        t.resize(
            alloc,
            Resize {
                cols: 10,
                rows: 5,
                cell_size_px: None,
            },
        )
        .unwrap();
        assert!(!t.modes.get(modespkg::Mode::SynchronizedOutput));

        t.deinit(alloc);
    }

    // test "Terminal: resize rejects zero dimensions before mutation"
    #[test]
    fn terminal_resize_rejects_zero_dimensions_before_mutation() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

        t.width_px = 100;
        t.height_px = 100;
        t.flags.dirty.clear = false;

        assert_eq!(
            Err(ResizeError::InvalidValue),
            t.resize(
                alloc,
                Resize {
                    cols: 0,
                    rows: 5,
                    cell_size_px: Some(ResizeCellSize {
                        width: 9,
                        height: 18
                    }),
                }
            )
        );
        assert_eq!(
            Err(ResizeError::InvalidValue),
            t.resize(
                alloc,
                Resize {
                    cols: 10,
                    rows: 0,
                    cell_size_px: Some(ResizeCellSize {
                        width: 9,
                        height: 18
                    }),
                }
            )
        );

        assert_eq!(10 as size::CellCountInt, t.cols);
        assert_eq!(5 as size::CellCountInt, t.rows);
        assert_eq!(100_u32, t.width_px);
        assert_eq!(100_u32, t.height_px);
        assert!(!t.flags.dirty.clear);

        t.deinit(alloc);
    }

    // test "Terminal: resize preserves pixel dimensions when omitted"
    #[test]
    fn terminal_resize_preserves_pixel_dimensions_when_omitted() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

        t.width_px = 90;
        t.height_px = 90;
        t.resize(
            alloc,
            Resize {
                cols: 20,
                rows: 10,
                cell_size_px: None,
            },
        )
        .unwrap();

        assert_eq!(90_u32, t.width_px);
        assert_eq!(90_u32, t.height_px);

        t.deinit(alloc);
    }

    // test "Terminal: resize updates pixels without changing cell dimensions"
    #[test]
    fn terminal_resize_updates_pixels_without_changing_cell_dimensions() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

        t.resize(
            alloc,
            Resize {
                cols: 10,
                rows: 5,
                cell_size_px: Some(ResizeCellSize {
                    width: 9,
                    height: 18,
                }),
            },
        )
        .unwrap();

        assert_eq!(90_u32, t.width_px);
        assert_eq!(90_u32, t.height_px);

        t.deinit(alloc);
    }

    // test "Terminal: resize pixel dimensions saturate"
    #[test]
    fn terminal_resize_pixel_dimensions_saturate() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(2, 3)).unwrap();

        t.resize(
            alloc,
            Resize {
                cols: 2,
                rows: 3,
                cell_size_px: Some(ResizeCellSize {
                    width: u32::MAX,
                    height: u32::MAX,
                }),
            },
        )
        .unwrap();

        assert_eq!(u32::MAX, t.width_px);
        assert_eq!(u32::MAX, t.height_px);

        t.deinit(alloc);
    }

    // PORT FINDING GUARD (P3-W4 T2): the alternate screen's `Screen::resize`
    // must use the reference's `Screen.Resize.prompt_redraw` DEFAULT, which is
    // `.false`. rustty passed `.true` here, citing a default the reference has
    // never had. With `.true` the alternate screen clears the cursor's prompt
    // row on every resize whenever the cursor's semantic content is not
    // `.output`; with `.false` it does not. No upstream test covers this
    // because upstream never had the bug.
    #[test]
    fn terminal_resize_does_not_prompt_redraw_the_alternate_screen() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(10, 3)).unwrap();

        // Enter the alternate screen and put a prompt-ish line on it with the
        // cursor still marked as prompt input (the condition
        // `clear_prompt_for_redraw` keys off).
        t.switch_screen_mode(SwitchScreenMode::M1049, true).unwrap();
        unsafe {
            (*t.screens.active).cursor_set_semantic_content(
                crate::screen::SemanticContentArg::Prompt(
                    osc::semantic_prompt::PromptKind::Initial,
                ),
            );
        }
        t.print_string(b"> abc").unwrap();

        // shell_redraws_prompt is what the PRIMARY screen keys off; the
        // alternate screen must not honor it regardless.
        t.flags.shell_redraws_prompt = osc::semantic_prompt::Redraw::True;

        t.resize(
            alloc,
            Resize {
                cols: 20,
                rows: 3,
                cell_size_px: None,
            },
        )
        .unwrap();

        let s = t.plain_string(Global).unwrap();
        assert_eq!(b"> abc".as_slice(), s.as_slice());

        t.deinit(alloc);
    }

    // test "Terminal: resize preserves tabstops on allocation failure"
    #[test]
    fn terminal_resize_preserves_tabstops_on_allocation_failure() {
        // var failing = testing.FailingAllocator.init(testing.allocator, .{});
        // const alloc = failing.allocator();
        //
        // Ported as written (parity tail S7-3; a `Tabstops` tripwire stood in
        // while the stops lived on the Rust heap). 513 columns is past
        // `PREALLOC_COLUMNS`, so the dynamic-stops allocation IS attempted.
        let failing = crate::alloc::testing::FailingAllocator::over_heap(
            crate::alloc::testing::Config::default(),
        );
        let c = failing.allocator();
        crate::alloc::testing::with_handle(&c, |alloc| {
            let mut t = Terminal::init(alloc, opts(10, 1)).unwrap();
            // defer t.deinit(alloc);

            // failing.fail_index = failing.alloc_index;
            failing.fail_index.set(failing.alloc_index.get());
            assert_eq!(
                Err(ResizeError::OutOfMemory),
                t.resize(
                    alloc,
                    Resize {
                        cols: 513,
                        rows: 1,
                        cell_size_px: None
                    }
                )
            );
            assert!(failing.has_induced_failure.get());

            // The original Tabstops was never replaced: cols is unchanged and the
            // original interval-8 stop is still present.
            assert_eq!(10 as size::CellCountInt, t.cols);
            assert!(t.tabstops.get(8));

            t.deinit(alloc);
        });
    }

    // test "Terminal: resize failure paths preserve consistent state"
    //
    // NEGATIVE CONTROL (P3-W4 T3 Step 3): one arm per fail point, each asserting
    // a DIFFERENT piece of rolled-back state, because a single "resize failed
    // and nothing changed" test cannot say which rollback arm regressed:
    //   - `Tabstops`      trips BEFORE the tabstop table is replaced.
    //   - `PrimaryScreen` trips after the tabstops were built (they must be
    //                     freed) but before any screen mutated.
    //   - `AlternateScreen` is the RECOVERY arm, not a restoration arm: the
    //                     primary already resized, so the resize SUCCEEDS and
    //                     the inactive alternate is dropped instead.
    //
    // WS18 release lane (CORRECTED — fix round 1 of parity S166-F2): the sentence
    // that used to stand here, "`tripwire::Module::enabled()` is
    // `cfg!(debug_assertions)`", is FALSE and had this test `#[ignore]`d in
    // release for a reason that did not exist. `enabled()` is
    // `cfg!(any(test, debug_assertions, feature = "tripwire-test"))`, and
    // rustty-vt turns `rustty-util/tripwire-test` on as a dev-dependency feature
    // precisely so this crate's fail points trip in the RELEASE test lane too —
    // which is what Zig's `builtin.is_test` does at every optimize mode. Verified:
    // this test RUNS and PASSES under `cargo test -p rustty-vt --release`.
    #[test]
    fn terminal_resize_failure_paths_preserve_consistent_state() {
        let alloc = Global;

        for tag in [
            ResizeFp::Tabstops,
            ResizeFp::PrimaryScreen,
            ResizeFp::AlternateScreen,
        ] {
            let mut t = Terminal::init(alloc, opts(10, 3)).unwrap();

            t.print_string(b"primary").unwrap();
            let _ = t.switch_screen(crate::screen_set::Key::Alternate).unwrap();
            t.print_string(b"alternate").unwrap();
            let _ = t.switch_screen(crate::screen_set::Key::Primary).unwrap();

            t.width_px = 100;
            t.height_px = 50;
            t.modes.set(modespkg::Mode::SynchronizedOutput, true);
            t.flags.dirty.clear = false;
            t.scrolling_region = ScrollingRegion {
                top: 1,
                bottom: 2,
                left: 1,
                right: 8,
            };
            t.tabstops.unset(8);
            t.tabstops.set(3);

            let primary = t.screens.get(crate::screen_set::Key::Primary).unwrap();
            let alternate = t.screens.get(crate::screen_set::Key::Alternate).unwrap();
            let alternate_generation = t.screens.generation(crate::screen_set::Key::Alternate);
            assert!(std::ptr::eq(
                unsafe { (*primary).pages.pages.first },
                unsafe { (*primary).pages.pages.last }
            ));
            assert!(std::ptr::eq(
                unsafe { (*alternate).pages.pages.first },
                unsafe { (*alternate).pages.pages.last }
            ));

            // PORT NOTE: `const before = t;` is a Zig by-value struct copy; the
            // Rust port snapshots the fields the reference compares.
            //
            // CORRECTION (S143-F1): the note that stood here said the port
            // "snapshots exactly the fields the reference compares". It did
            // not. The reference compares whole values —
            // `expectEqual(before.flags, t.flags)` (every `Flags` field),
            // `std.meta.eql(before.tabstops, t.tabstops)` (the entire tabstop
            // table) and `std.meta.eql(before_*.cursor, *.cursor)` (the entire
            // `Cursor`) — where the port compared `flags.dirty.clear`, the two
            // tabstop bits at columns 8 and 3, and the cursor `(x, y)` pair. A
            // rollback that cleared another `Flags` field, disturbed another
            // tabstop or altered `cursor.pending_wrap` passed here and failed
            // upstream.
            let before_width_px = t.width_px;
            let before_height_px = t.height_px;
            let before_modes = (t.modes.values, t.modes.saved, t.modes.default);
            let before_cols = t.cols;
            let before_rows = t.rows;
            let before_scrolling_region = (
                t.scrolling_region.top,
                t.scrolling_region.bottom,
                t.scrolling_region.left,
                t.scrolling_region.right,
            );
            let before_flags = t.flags;
            let before_tabstops = t.tabstops.meta_eql_snapshot();
            let before_active_key = t.screens.active_key;
            let before_active = t.screens.active;
            let (before_primary_cols, before_primary_rows, before_primary_total_rows) = unsafe {
                (
                    (*primary).pages.cols,
                    (*primary).pages.rows,
                    (*primary).pages.total_rows,
                )
            };
            let before_primary_cursor = unsafe { (*primary).cursor };
            let before_primary_page: Vec<u8> = unsafe {
                let page: *const Page = crate::pagelist::Node::page((*primary).pages.pages.first);
                core::slice::from_raw_parts((*page).memory_ptr(), (*page).memory_len()).to_vec()
            };
            let (before_alt_cols, before_alt_rows, before_alt_total_rows) = unsafe {
                (
                    (*alternate).pages.cols,
                    (*alternate).pages.rows,
                    (*alternate).pages.total_rows,
                )
            };
            let before_alternate_cursor = unsafe { (*alternate).cursor };
            let before_alternate_page: Vec<u8> = unsafe {
                let page: *const Page = crate::pagelist::Node::page((*alternate).pages.pages.first);
                core::slice::from_raw_parts((*page).memory_ptr(), (*page).memory_len()).to_vec()
            };

            resize_tw_error_always(tag);

            // A failure after the primary screen has resized is recovered by
            // dropping the inactive alternate and completing the resize. This
            // also proves the alternate tripwire is after the primary resize
            // rather than acting as a preflight check.
            if tag == ResizeFp::AlternateScreen {
                t.resize(
                    alloc,
                    Resize {
                        cols: 513,
                        rows: 4,
                        cell_size_px: Some(ResizeCellSize {
                            width: 9,
                            height: 18,
                        }),
                    },
                )
                .unwrap();

                assert_eq!(513 as size::CellCountInt, t.cols);
                assert_eq!(4 as size::CellCountInt, t.rows);
                assert_eq!(4617_u32, t.width_px);
                assert_eq!(72_u32, t.height_px);
                assert!(!t.modes.get(modespkg::Mode::SynchronizedOutput));
                assert!(t.flags.dirty.clear);
                assert_eq!(513 as size::CellCountInt, unsafe { (*primary).pages.cols });
                assert_eq!(4 as size::CellCountInt, unsafe { (*primary).pages.rows });
                assert!(t.screens.get(crate::screen_set::Key::Alternate).is_none());
                assert_eq!(
                    alternate_generation.wrapping_add(1),
                    t.screens.generation(crate::screen_set::Key::Alternate)
                );
                assert_eq!(crate::screen_set::Key::Primary, t.screens.active_key);
                assert!(std::ptr::eq(primary, t.screens.active));
                assert!(t.tabstops.get(8));
                assert!(!t.tabstops.get(3));

                // The alternate is recreated lazily at the terminal's new size.
                let _ = t.switch_screen(crate::screen_set::Key::Alternate).unwrap();
                let replacement = t.screens.get(crate::screen_set::Key::Alternate).unwrap();
                assert_eq!(513 as size::CellCountInt, unsafe {
                    (*replacement).pages.cols
                });
                assert_eq!(4 as size::CellCountInt, unsafe {
                    (*replacement).pages.rows
                });
                assert!(unsafe {
                    (*(*replacement)
                        .pages
                        .get_cell(point::Point::Active(point::Coordinate::default()))
                        .unwrap()
                        .cell)
                        .is_empty()
                });

                resize_tw_end(rustty_util::tripwire::ResetMode::Reset).unwrap();
                t.deinit(alloc);
                continue;
            }

            assert_eq!(
                Err(ResizeError::OutOfMemory),
                t.resize(
                    alloc,
                    Resize {
                        cols: 513,
                        rows: 4,
                        cell_size_px: Some(ResizeCellSize {
                            width: 9,
                            height: 18
                        }),
                    }
                )
            );

            assert_eq!(before_width_px, t.width_px);
            assert_eq!(before_height_px, t.height_px);
            assert_eq!(
                before_modes,
                (t.modes.values, t.modes.saved, t.modes.default)
            );
            assert_eq!(before_cols, t.cols);
            assert_eq!(before_rows, t.rows);
            assert_eq!(
                before_scrolling_region,
                (
                    t.scrolling_region.top,
                    t.scrolling_region.bottom,
                    t.scrolling_region.left,
                    t.scrolling_region.right,
                )
            );
            // try testing.expectEqual(before.flags, t.flags);
            assert_eq!(before_flags, t.flags);
            // try testing.expect(std.meta.eql(before.tabstops, t.tabstops));
            assert_eq!(before_tabstops, t.tabstops.meta_eql_snapshot());
            assert_eq!(before_active_key, t.screens.active_key);
            assert!(std::ptr::eq(before_active, t.screens.active));

            unsafe {
                assert_eq!(before_primary_cols, (*primary).pages.cols);
                assert_eq!(before_primary_rows, (*primary).pages.rows);
                assert_eq!(before_primary_total_rows, (*primary).pages.total_rows);
                // try testing.expect(std.meta.eql(before_primary.cursor, primary.cursor));
                assert!(
                    before_primary_cursor.meta_eql(&(*primary).cursor),
                    "primary cursor changed after a failed resize"
                );
                let page: *const Page = crate::pagelist::Node::page((*primary).pages.pages.first);
                let after = core::slice::from_raw_parts((*page).memory_ptr(), (*page).memory_len());
                assert!(
                    before_primary_page == after,
                    "primary page memory changed after a failed resize"
                );

                assert_eq!(before_alt_cols, (*alternate).pages.cols);
                assert_eq!(before_alt_rows, (*alternate).pages.rows);
                assert_eq!(before_alt_total_rows, (*alternate).pages.total_rows);
                // try testing.expect(std.meta.eql(before_alternate.cursor, alternate.cursor));
                assert!(
                    before_alternate_cursor.meta_eql(&(*alternate).cursor),
                    "alternate cursor changed after a failed resize"
                );
                let page: *const Page = crate::pagelist::Node::page((*alternate).pages.pages.first);
                let after = core::slice::from_raw_parts((*page).memory_ptr(), (*page).memory_len());
                assert!(
                    before_alternate_page == after,
                    "alternate page memory changed after a failed resize"
                );
            }

            resize_tw_end(rustty_util::tripwire::ResetMode::Reset).unwrap();
            t.deinit(alloc);
        }
    }

    // test "Terminal: alternate resize failure replaces active alternate screen"
    //
    // WS18 release lane: tripwire fail point; see the PORT NOTE above.
    #[test]
    fn terminal_alternate_resize_failure_replaces_active_alternate_screen() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(10, 3)).unwrap();

        let _ = t.switch_screen(crate::screen_set::Key::Alternate).unwrap();
        assert_eq!(crate::screen_set::Key::Alternate, t.screens.active_key);
        unsafe { (*t.screens.active).charset.gl = charsets::Slots::G1 };
        t.print_string(b"alternate").unwrap();
        let generation = t.screens.generation(crate::screen_set::Key::Alternate);

        resize_tw_error_always(ResizeFp::AlternateScreen);
        // S142-F1/S143-F2: `log.warn("alternate screen resize failed,
        // replacing it err={}", .{err})` (Terminal.zig:4110).
        let cap = crate::log_capture::start();
        t.resize(
            alloc,
            Resize {
                cols: 20,
                rows: 4,
                cell_size_px: None,
            },
        )
        .unwrap();
        let messages = cap.messages();
        assert!(
            messages
                .iter()
                .any(|(level, target, msg)| *level == log::Level::Warn
                    && target == "terminal"
                    && msg.starts_with("alternate screen resize failed, replacing it err=")),
            "expected the alternate-screen-resize-failed diagnostic, got: {messages:?}"
        );
        drop(cap);
        resize_tw_end(rustty_util::tripwire::ResetMode::Reset).unwrap();

        let alternate = t.screens.get(crate::screen_set::Key::Alternate).unwrap();
        assert_eq!(crate::screen_set::Key::Alternate, t.screens.active_key);
        assert!(std::ptr::eq(alternate, t.screens.active));
        assert_eq!(20 as size::CellCountInt, unsafe { (*alternate).pages.cols });
        assert_eq!(4 as size::CellCountInt, unsafe { (*alternate).pages.rows });
        assert!(unsafe {
            (*(*alternate)
                .pages
                .get_cell(point::Point::Active(point::Coordinate::default()))
                .unwrap()
                .cell)
                .is_empty()
        });
        assert_eq!(charsets::Slots::G1, unsafe { (*alternate).charset.gl });
        assert_eq!(
            generation.wrapping_add(1),
            t.screens.generation(crate::screen_set::Key::Alternate)
        );

        t.deinit(alloc);
    }

    // test "Terminal: alternate resize replacement failure falls back to primary"
    //
    // WS18 release lane: tripwire fail point; see the PORT NOTE above.
    #[test]
    fn terminal_alternate_resize_replacement_failure_falls_back_to_primary() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(10, 3)).unwrap();

        let _ = t.switch_screen(crate::screen_set::Key::Alternate).unwrap();
        resize_tw_error_always(ResizeFp::AlternateScreen);
        // NOT `ResizeFp::AlternateScreenInit`: that tripwire matches the
        // reference's OWN silent `tw.check(.alternate_screen_init) catch
        // break :alt;` guard (Terminal.zig:4131, no log call at all) — using
        // it would short-circuit BEFORE the real `ScreenSet::get_init` call
        // this test needs to fail. Force `PageList::init` itself (which
        // `Screen::init` propagates through `?`) to fail instead, via the
        // pagelist crate's own init tripwire.
        crate::pagelist::INIT_TW.with(|tw| {
            tw.borrow_mut()
                .errorAlways(crate::pagelist::InitFp::InitMemoryPool, OutOfMemory)
        });
        // S142-F1/S143-F2: `log.warn("alternate screen replacement failed,
        // falling back to primary err={}", .{init_err})` (Terminal.zig:4149-4152).
        let cap = crate::log_capture::start();
        t.resize(
            alloc,
            Resize {
                cols: 20,
                rows: 4,
                cell_size_px: None,
            },
        )
        .unwrap();
        let messages = cap.messages();
        assert!(
            messages
                .iter()
                .any(|(level, target, msg)| *level == log::Level::Warn
                    && target == "terminal"
                    && msg.starts_with(
                        "alternate screen replacement failed, falling back to primary err="
                    )),
            "expected the alternate-screen-replacement-failed diagnostic, got: {messages:?}"
        );
        drop(cap);
        crate::pagelist::INIT_TW
            .with(|tw| tw.borrow_mut().end(rustty_util::tripwire::ResetMode::Reset))
            .unwrap();
        resize_tw_end(rustty_util::tripwire::ResetMode::Reset).unwrap();

        let primary = t.screens.get(crate::screen_set::Key::Primary).unwrap();
        assert!(t.screens.get(crate::screen_set::Key::Alternate).is_none());
        assert_eq!(crate::screen_set::Key::Primary, t.screens.active_key);
        assert!(std::ptr::eq(primary, t.screens.active));
        assert_eq!(20 as size::CellCountInt, unsafe { (*primary).pages.cols });
        assert_eq!(4 as size::CellCountInt, unsafe { (*primary).pages.rows });

        t.deinit(alloc);
    }

    // https://github.com/mitchellh/ghostty/issues/723
    // This was found via fuzzing so its highly specific.
    // test "Terminal: resize with left and right margin set"
    #[test]
    fn terminal_resize_with_left_and_right_margin_set() {
        let alloc = Global;
        let cols: size::CellCountInt = 70;
        let rows: size::CellCountInt = 23;
        let mut t = Terminal::init(alloc, opts(cols, rows)).unwrap();

        t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
        t.print('0' as u32).unwrap();
        t.modes.set(modespkg::Mode::EnableMode3, true);
        t.resize(
            alloc,
            Resize {
                cols,
                rows,
                cell_size_px: None,
            },
        )
        .unwrap();
        t.set_left_and_right_margin(2, 0);
        t.print_repeat(1850).unwrap();
        let _ = t.modes.restore(modespkg::Mode::EnableMode3);
        t.resize(
            alloc,
            Resize {
                cols,
                rows,
                cell_size_px: None,
            },
        )
        .unwrap();

        t.deinit(alloc);
    }

    // https://github.com/mitchellh/ghostty/issues/1343
    // test "Terminal: resize with wraparound off"
    #[test]
    fn terminal_resize_with_wraparound_off() {
        let alloc = Global;
        let cols: size::CellCountInt = 4;
        let rows: size::CellCountInt = 2;
        let mut t = Terminal::init(alloc, opts(cols, rows)).unwrap();

        t.modes.set(modespkg::Mode::Wraparound, false);
        t.print('0' as u32).unwrap();
        t.print('1' as u32).unwrap();
        t.print('2' as u32).unwrap();
        t.print('3' as u32).unwrap();
        let new_cols: size::CellCountInt = 2;
        t.resize(
            alloc,
            Resize {
                cols: new_cols,
                rows,
                cell_size_px: None,
            },
        )
        .unwrap();

        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "01".as_bytes());

        t.deinit(alloc);
    }

    // test "Terminal: resize with wraparound on"
    #[test]
    fn terminal_resize_with_wraparound_on() {
        let alloc = Global;
        let cols: size::CellCountInt = 4;
        let rows: size::CellCountInt = 2;
        let mut t = Terminal::init(alloc, opts(cols, rows)).unwrap();

        t.modes.set(modespkg::Mode::Wraparound, true);
        t.print('0' as u32).unwrap();
        t.print('1' as u32).unwrap();
        t.print('2' as u32).unwrap();
        t.print('3' as u32).unwrap();
        let new_cols: size::CellCountInt = 2;
        t.resize(
            alloc,
            Resize {
                cols: new_cols,
                rows,
                cell_size_px: None,
            },
        )
        .unwrap();

        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "01\n23".as_bytes());

        t.deinit(alloc);
    }

    // test "Terminal: resize with high unique style per cell"
    #[test]
    fn terminal_resize_with_high_unique_style_per_cell() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(30, 30)).unwrap();

        for y in 0..t.rows {
            for x in 0..t.cols {
                t.set_cursor_pos(y as usize, x as usize);
                t.set_attribute(sgr::Attribute::DirectColorBg(color::RGB {
                    r: x as u8,
                    g: y as u8,
                    b: 0,
                }))
                .unwrap();
                t.print('x' as u32).unwrap();
            }
        }

        t.resize(
            alloc,
            Resize {
                cols: 60,
                rows: 30,
                cell_size_px: None,
            },
        )
        .unwrap();

        t.deinit(alloc);
    }

    // test "Terminal: resize with high unique style per cell with wrapping"
    #[cfg_attr(
        all(miri, debug_assertions),
        ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
    )]
    #[test]
    fn terminal_resize_with_high_unique_style_per_cell_with_wrapping() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(30, 30)).unwrap();

        let cell_count: u16 = (t.rows as u32 * t.cols as u32) as u16;
        for i in 0..cell_count {
            let r: u8 = (i >> 8) as u8;
            let g: u8 = (i & 0xFF) as u8;

            t.set_attribute(sgr::Attribute::DirectColorBg(color::RGB { r, g, b: 0 }))
                .unwrap();
            t.print('x' as u32).unwrap();
        }

        t.resize(
            alloc,
            Resize {
                cols: 60,
                rows: 30,
                cell_size_px: None,
            },
        )
        .unwrap();

        t.deinit(alloc);
    }

    // test "Terminal: resize with reflow and saved cursor"
    #[test]
    fn terminal_resize_with_reflow_and_saved_cursor() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(2, 3)).unwrap();
        t.print_string("1A2B".as_bytes()).unwrap();
        t.set_cursor_pos(2, 2);
        {
            let cursor_x = unsafe { (*t.screens.active).cursor.x };
            let cursor_y = unsafe { (*t.screens.active).cursor.y };
            let list_cell = t
                .active_mut()
                .pages
                .get_cell(active_pt(cursor_x, cursor_y as u32))
                .unwrap();
            let cell = list_cell.cell;
            assert_eq!('B' as u32, unsafe { (*cell).content_codepoint() } as u32);
        }

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "1A\n2B".as_bytes());
        }

        t.save_cursor();
        t.resize(
            alloc,
            Resize {
                cols: 5,
                rows: 3,
                cell_size_px: None,
            },
        )
        .unwrap();
        t.restore_cursor();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "1A2B".as_bytes());
        }

        // Verify our cursor is still in the same place
        {
            let cursor_x = unsafe { (*t.screens.active).cursor.x };
            let cursor_y = unsafe { (*t.screens.active).cursor.y };
            let list_cell = t
                .active_mut()
                .pages
                .get_cell(active_pt(cursor_x, cursor_y as u32))
                .unwrap();
            let cell = list_cell.cell;
            assert_eq!('B' as u32, unsafe { (*cell).content_codepoint() } as u32);
        }

        t.deinit(alloc);
    }

    // test "Terminal: resize with reflow and saved cursor pending wrap"
    #[test]
    fn terminal_resize_with_reflow_and_saved_cursor_pending_wrap() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(2, 3)).unwrap();
        t.print_string("1A2B".as_bytes()).unwrap();
        {
            let cursor_x = unsafe { (*t.screens.active).cursor.x };
            let cursor_y = unsafe { (*t.screens.active).cursor.y };
            let list_cell = t
                .active_mut()
                .pages
                .get_cell(active_pt(cursor_x, cursor_y as u32))
                .unwrap();
            let cell = list_cell.cell;
            assert_eq!('B' as u32, unsafe { (*cell).content_codepoint() } as u32);
        }

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "1A\n2B".as_bytes());
        }

        t.save_cursor();
        t.resize(
            alloc,
            Resize {
                cols: 5,
                rows: 3,
                cell_size_px: None,
            },
        )
        .unwrap();
        t.restore_cursor();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "1A2B".as_bytes());
        }

        // Pending wrap should be reset
        t.print('X' as u32).unwrap();
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "1A2BX".as_bytes());
        }

        t.deinit(alloc);
    }

    // test "Terminal: DECCOLM without DEC mode 40"
    #[test]
    fn terminal_deccolm_without_dec_mode_40() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        t.modes.set(modespkg::Mode::Column132, true);
        t.deccolm(alloc, DeccolmMode::Cols132).unwrap();
        assert_eq!(5_usize, t.cols as usize);
        assert_eq!(5_usize, t.rows as usize);
        assert!(!t.modes.get(modespkg::Mode::Column132));

        t.deinit(alloc);
    }

    // test "Terminal: DECCOLM unset"
    #[test]
    fn terminal_deccolm_unset() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        t.modes.set(modespkg::Mode::EnableMode3, true);
        t.deccolm(alloc, DeccolmMode::Cols80).unwrap();
        assert_eq!(80_usize, t.cols as usize);
        assert_eq!(5_usize, t.rows as usize);

        t.deinit(alloc);
    }

    // test "Terminal: DECCOLM resets pending wrap"
    #[test]
    fn terminal_deccolm_resets_pending_wrap() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        for c in "ABCDE".chars() {
            t.print(c as u32).unwrap();
        }
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });

        t.modes.set(modespkg::Mode::EnableMode3, true);
        t.deccolm(alloc, DeccolmMode::Cols80).unwrap();
        assert_eq!(80_usize, t.cols as usize);
        assert_eq!(5_usize, t.rows as usize);
        assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });

        t.deinit(alloc);
    }

    // test "Terminal: DECCOLM preserves SGR bg"
    #[test]
    fn terminal_deccolm_preserves_sgr_bg() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        t.set_attribute(sgr::Attribute::DirectColorBg(color::RGB {
            r: 0xFF,
            g: 0,
            b: 0,
        }))
        .unwrap();
        t.modes.set(modespkg::Mode::EnableMode3, true);
        t.deccolm(alloc, DeccolmMode::Cols80).unwrap();

        {
            let list_cell = t.active_mut().pages.get_cell(active_pt(0, 0)).unwrap();
            assert!(unsafe { (*list_cell.cell).content_tag() } == pagepkg::ContentTag::BgColorRgb);
            assert_eq!(
                pagepkg::CellRgb {
                    r: 0xFF,
                    g: 0,
                    b: 0,
                },
                unsafe { (*list_cell.cell).content_color_rgb() }
            );
        }

        t.deinit(alloc);
    }

    // test "Terminal: DECCOLM resets scroll region"
    #[test]
    fn terminal_deccolm_resets_scroll_region() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
        t.set_top_and_bottom_margin(2, 3);
        t.set_left_and_right_margin(3, 5);

        t.modes.set(modespkg::Mode::EnableMode3, true);
        t.deccolm(alloc, DeccolmMode::Cols80).unwrap();

        assert!(t.modes.get(modespkg::Mode::EnableLeftAndRightMargin));
        assert_eq!(0_usize, t.scrolling_region.top as usize);
        assert_eq!(4_usize, t.scrolling_region.bottom as usize);
        assert_eq!(0_usize, t.scrolling_region.left as usize);
        assert_eq!(79_usize, t.scrolling_region.right as usize);

        t.deinit(alloc);
    }

    // test "Terminal: mode 47 alt screen plain"
    #[test]
    fn terminal_mode_47_alt_screen_plain() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        // Print on primary screen
        t.print_string("1A".as_bytes()).unwrap();

        // Go to alt screen with mode 47
        t.switch_screen_mode(SwitchScreenMode::M47, true).unwrap();
        assert_eq!(crate::screen_set::Key::Alternate, t.screens.active_key);

        // Screen should be empty
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "".as_bytes());
        }

        // Print on alt screen. This should be off center because
        // we copy the cursor over from the primary screen
        t.print_string("2B".as_bytes()).unwrap();
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "  2B".as_bytes());
        }

        // Go back to primary
        t.switch_screen_mode(SwitchScreenMode::M47, false).unwrap();
        assert_eq!(crate::screen_set::Key::Primary, t.screens.active_key);

        // Primary screen should still have the original content
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "1A".as_bytes());
        }

        // Go back to alt screen with mode 47
        t.switch_screen_mode(SwitchScreenMode::M47, true).unwrap();
        assert_eq!(crate::screen_set::Key::Alternate, t.screens.active_key);

        // Screen should retain content
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "  2B".as_bytes());
        }

        t.deinit(alloc);
    }

    // test "Terminal: mode 47 copies cursor both directions"
    #[test]
    fn terminal_mode_47_copies_cursor_both_directions() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        // Color our cursor red
        t.set_attribute(sgr::Attribute::DirectColorFg(color::RGB {
            r: 0xFF,
            g: 0,
            b: 0x7F,
        }))
        .unwrap();

        // Go to alt screen with mode 47
        t.switch_screen_mode(SwitchScreenMode::M47, true).unwrap();
        assert_eq!(crate::screen_set::Key::Alternate, t.screens.active_key);

        // Verify that our style is set
        {
            assert!(unsafe { (*t.screens.active).cursor.style_id } != style::DEFAULT_ID);
            let page =
                unsafe { crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node) };
            assert_eq!(1_usize, unsafe { (*page).styles.count() });
            assert!(
                unsafe {
                    (*page)
                        .styles
                        .ref_count((*page).memory_ptr(), (*t.screens.active).cursor.style_id)
                } > 0
            );
        }

        // Set a new style
        t.set_attribute(sgr::Attribute::DirectColorFg(color::RGB {
            r: 0,
            g: 0xFF,
            b: 0,
        }))
        .unwrap();

        // Go back to primary
        t.switch_screen_mode(SwitchScreenMode::M47, false).unwrap();
        assert_eq!(crate::screen_set::Key::Primary, t.screens.active_key);

        // Verify that our style is still set
        {
            assert!(unsafe { (*t.screens.active).cursor.style_id } != style::DEFAULT_ID);
            let page =
                unsafe { crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node) };
            assert_eq!(1_usize, unsafe { (*page).styles.count() });
            assert!(
                unsafe {
                    (*page)
                        .styles
                        .ref_count((*page).memory_ptr(), (*t.screens.active).cursor.style_id)
                } > 0
            );
        }

        t.deinit(alloc);
    }

    // test "Terminal: mode 1047 alt screen plain"
    #[test]
    fn terminal_mode_1047_alt_screen_plain() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        // Print on primary screen
        t.print_string("1A".as_bytes()).unwrap();

        // Go to alt screen with mode 47
        t.switch_screen_mode(SwitchScreenMode::M1047, true).unwrap();
        assert_eq!(crate::screen_set::Key::Alternate, t.screens.active_key);

        // Screen should be empty
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "".as_bytes());
        }

        // Print on alt screen. This should be off center because
        // we copy the cursor over from the primary screen
        t.print_string("2B".as_bytes()).unwrap();
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "  2B".as_bytes());
        }

        // Go back to primary
        t.switch_screen_mode(SwitchScreenMode::M1047, false)
            .unwrap();
        assert_eq!(crate::screen_set::Key::Primary, t.screens.active_key);

        // Primary screen should still have the original content
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "1A".as_bytes());
        }

        // Go back to alt screen with mode 1047
        t.switch_screen_mode(SwitchScreenMode::M1047, true).unwrap();
        assert_eq!(crate::screen_set::Key::Alternate, t.screens.active_key);

        // Screen should be empty
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "".as_bytes());
        }

        t.deinit(alloc);
    }

    // test "Terminal: mode 1047 copies cursor both directions"
    #[test]
    fn terminal_mode_1047_copies_cursor_both_directions() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        // Color our cursor red
        t.set_attribute(sgr::Attribute::DirectColorFg(color::RGB {
            r: 0xFF,
            g: 0,
            b: 0x7F,
        }))
        .unwrap();

        // Go to alt screen with mode 47
        t.switch_screen_mode(SwitchScreenMode::M1047, true).unwrap();
        assert_eq!(crate::screen_set::Key::Alternate, t.screens.active_key);

        // Verify that our style is set
        {
            assert!(unsafe { (*t.screens.active).cursor.style_id } != style::DEFAULT_ID);
            let page =
                unsafe { crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node) };
            assert_eq!(1_usize, unsafe { (*page).styles.count() });
            assert!(
                unsafe {
                    (*page)
                        .styles
                        .ref_count((*page).memory_ptr(), (*t.screens.active).cursor.style_id)
                } > 0
            );
        }

        // Set a new style
        t.set_attribute(sgr::Attribute::DirectColorFg(color::RGB {
            r: 0,
            g: 0xFF,
            b: 0,
        }))
        .unwrap();

        // Go back to primary
        t.switch_screen_mode(SwitchScreenMode::M1047, false)
            .unwrap();
        assert_eq!(crate::screen_set::Key::Primary, t.screens.active_key);

        // Verify that our style is still set
        {
            assert!(unsafe { (*t.screens.active).cursor.style_id } != style::DEFAULT_ID);
            let page =
                unsafe { crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node) };
            assert_eq!(1_usize, unsafe { (*page).styles.count() });
            assert!(
                unsafe {
                    (*page)
                        .styles
                        .ref_count((*page).memory_ptr(), (*t.screens.active).cursor.style_id)
                } > 0
            );
        }

        t.deinit(alloc);
    }

    // test "Terminal: mode 1049 alt screen plain"
    #[test]
    fn terminal_mode_1049_alt_screen_plain() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        // Print on primary screen
        t.print_string("1A".as_bytes()).unwrap();

        // Go to alt screen with mode 47
        t.switch_screen_mode(SwitchScreenMode::M1049, true).unwrap();
        assert_eq!(crate::screen_set::Key::Alternate, t.screens.active_key);

        // Screen should be empty
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "".as_bytes());
        }

        // Print on alt screen. This should be off center because
        // we copy the cursor over from the primary screen
        t.print_string("2B".as_bytes()).unwrap();
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "  2B".as_bytes());
        }

        // Go back to primary
        t.switch_screen_mode(SwitchScreenMode::M1049, false)
            .unwrap();
        assert_eq!(crate::screen_set::Key::Primary, t.screens.active_key);

        // Primary screen should still have the original content
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "1A".as_bytes());
        }

        // Write, our cursor should be restored back.
        t.print_string("C".as_bytes()).unwrap();
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "1AC".as_bytes());
        }

        // Go back to alt screen with mode 1049
        t.switch_screen_mode(SwitchScreenMode::M1049, true).unwrap();
        assert_eq!(crate::screen_set::Key::Alternate, t.screens.active_key);

        // Screen should be empty
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "".as_bytes());
        }

        t.deinit(alloc);
    }

    // test "Terminal: semantic prompt"
    #[test]
    fn terminal_semantic_prompt() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

        // Prompt
        t.semantic_prompt(osc::SemanticPrompt::init(
            osc::semantic_prompt::Action::FreshLineNewPrompt,
        ))
        .unwrap();
        for c in "hello".bytes() {
            t.print(c as u32).unwrap();
        }
        assert_eq!(0usize, unsafe { (*t.screens.active).cursor.y } as usize);
        assert_eq!(5usize, unsafe { (*t.screens.active).cursor.x } as usize);
        {
            let cx = unsafe { (*t.screens.active).cursor.x };
            let cy = unsafe { (*t.screens.active).cursor.y } as u32;
            let list_cell = t
                .active_mut()
                .pages
                .get_cell(screen_pt(cx - 1, cy))
                .unwrap();
            let cell = list_cell.cell;
            assert_eq!(pagepkg::SemanticContent::Prompt, unsafe {
                (*cell).semantic_content()
            });

            let row = list_cell.row;
            assert_eq!(pagepkg::SemanticPrompt::Prompt, unsafe {
                (*row).semantic_prompt()
            });
        }

        // Start input but end it on EOL
        t.semantic_prompt(osc::SemanticPrompt::init(
            osc::semantic_prompt::Action::EndPromptStartInputTerminateEol,
        ))
        .unwrap();
        t.carriage_return();
        t.linefeed().unwrap();

        // Write some output
        assert_eq!(1usize, unsafe { (*t.screens.active).cursor.y } as usize);
        assert_eq!(0usize, unsafe { (*t.screens.active).cursor.x } as usize);
        for c in "world".bytes() {
            t.print(c as u32).unwrap();
        }
        {
            let cx = unsafe { (*t.screens.active).cursor.x };
            let cy = unsafe { (*t.screens.active).cursor.y } as u32;
            let list_cell = t
                .active_mut()
                .pages
                .get_cell(screen_pt(cx - 1, cy))
                .unwrap();
            let cell = list_cell.cell;
            assert_eq!(pagepkg::SemanticContent::Output, unsafe {
                (*cell).semantic_content()
            });

            let row = list_cell.row;
            assert_eq!(pagepkg::SemanticPrompt::None, unsafe {
                (*row).semantic_prompt()
            });
        }

        t.deinit(alloc);
    }

    // test "Terminal: semantic prompt continuations"
    #[test]
    fn terminal_semantic_prompt_continuations() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

        // Prompt
        t.semantic_prompt(osc::SemanticPrompt::init(
            osc::semantic_prompt::Action::FreshLineNewPrompt,
        ))
        .unwrap();
        for c in "hello".bytes() {
            t.print(c as u32).unwrap();
        }
        assert_eq!(0usize, unsafe { (*t.screens.active).cursor.y } as usize);
        assert_eq!(5usize, unsafe { (*t.screens.active).cursor.x } as usize);
        {
            let cx = unsafe { (*t.screens.active).cursor.x };
            let cy = unsafe { (*t.screens.active).cursor.y } as u32;
            let list_cell = t
                .active_mut()
                .pages
                .get_cell(screen_pt(cx - 1, cy))
                .unwrap();
            let cell = list_cell.cell;
            assert_eq!(pagepkg::SemanticContent::Prompt, unsafe {
                (*cell).semantic_content()
            });

            let row = list_cell.row;
            assert_eq!(pagepkg::SemanticPrompt::Prompt, unsafe {
                (*row).semantic_prompt()
            });
        }

        // Start input but end it on EOL
        t.carriage_return();
        t.linefeed().unwrap();
        t.semantic_prompt(osc::SemanticPrompt {
            action: osc::semantic_prompt::Action::PromptStart,
            options_unvalidated: b"k=c".to_vec(),
        })
        .unwrap();

        // Write some output
        assert_eq!(1usize, unsafe { (*t.screens.active).cursor.y } as usize);
        assert_eq!(0usize, unsafe { (*t.screens.active).cursor.x } as usize);
        for c in "world".bytes() {
            t.print(c as u32).unwrap();
        }
        {
            let cx = unsafe { (*t.screens.active).cursor.x };
            let cy = unsafe { (*t.screens.active).cursor.y } as u32;
            let list_cell = t
                .active_mut()
                .pages
                .get_cell(screen_pt(cx - 1, cy))
                .unwrap();
            let cell = list_cell.cell;
            assert_eq!(pagepkg::SemanticContent::Prompt, unsafe {
                (*cell).semantic_content()
            });

            let row = list_cell.row;
            assert_eq!(pagepkg::SemanticPrompt::PromptContinuation, unsafe {
                (*row).semantic_prompt()
            });
        }

        t.deinit(alloc);
    }

    // test "Terminal: index in prompt mode marks new row as prompt continuation"
    #[test]
    fn terminal_index_in_prompt_mode_marks_new_row_as_prompt_continuation() {
        // This tests the Fish shell workaround: when in prompt mode and we get
        // a newline, assume the new row is a prompt continuation (since Fish
        // doesn't emit OSC133 k=s markers for continuation lines).
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

        // Start a prompt
        t.semantic_prompt(osc::SemanticPrompt::init(
            osc::semantic_prompt::Action::PromptStart,
        ))
        .unwrap();
        for c in "hello".bytes() {
            t.print(c as u32).unwrap();
        }

        // Verify first row is marked as prompt
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
            assert_eq!(pagepkg::SemanticPrompt::Prompt, unsafe {
                (*list_cell.row).semantic_prompt()
            });
        }

        // Now do a linefeed while still in prompt mode
        t.carriage_return();
        t.linefeed().unwrap();

        // The new row should automatically be marked as prompt continuation
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 1)).unwrap();
            assert_eq!(pagepkg::SemanticPrompt::PromptContinuation, unsafe {
                (*list_cell.row).semantic_prompt()
            });
        }

        // The cursor semantic content should still be prompt
        assert_eq!(pagepkg::SemanticContent::Prompt, unsafe {
            (*t.screens.active).cursor.semantic_content
        });

        t.deinit(alloc);
    }

    // test "Terminal: index in input mode does not mark new row as prompt"
    #[test]
    fn terminal_index_in_input_mode_does_not_mark_new_row_as_prompt() {
        // Input mode should NOT trigger prompt continuation on newline
        // (only prompt mode does, not input mode)
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

        // Start a prompt then switch to input
        t.semantic_prompt(osc::SemanticPrompt::init(
            osc::semantic_prompt::Action::PromptStart,
        ))
        .unwrap();
        for c in "$ ".bytes() {
            t.print(c as u32).unwrap();
        }
        t.semantic_prompt(osc::SemanticPrompt::init(
            osc::semantic_prompt::Action::EndPromptStartInput,
        ))
        .unwrap();
        for c in "echo \\".bytes() {
            t.print(c as u32).unwrap();
        }

        // Linefeed while in input mode
        t.carriage_return();
        t.linefeed().unwrap();

        // The new row should be marked as prompt continuation
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 1)).unwrap();
            assert_eq!(pagepkg::SemanticPrompt::PromptContinuation, unsafe {
                (*list_cell.row).semantic_prompt()
            });
        }

        // Our cursor should still be in input
        assert_eq!(pagepkg::SemanticContent::Input, unsafe {
            (*t.screens.active).cursor.semantic_content
        });

        t.deinit(alloc);
    }

    // test "Terminal: index in output mode does not mark new row as prompt"
    #[test]
    fn terminal_index_in_output_mode_does_not_mark_new_row_as_prompt() {
        // Output mode should NOT trigger prompt continuation
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

        // Complete prompt cycle: prompt -> input -> output
        t.semantic_prompt(osc::SemanticPrompt::init(
            osc::semantic_prompt::Action::PromptStart,
        ))
        .unwrap();
        for c in "$ ".bytes() {
            t.print(c as u32).unwrap();
        }
        t.semantic_prompt(osc::SemanticPrompt::init(
            osc::semantic_prompt::Action::EndPromptStartInput,
        ))
        .unwrap();
        for c in "ls".bytes() {
            t.print(c as u32).unwrap();
        }
        t.semantic_prompt(osc::SemanticPrompt::init(
            osc::semantic_prompt::Action::EndInputStartOutput,
        ))
        .unwrap();

        // Linefeed while in output mode
        t.carriage_return();
        t.linefeed().unwrap();

        // The new row should NOT be marked as a prompt
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 1)).unwrap();
            assert_eq!(pagepkg::SemanticPrompt::None, unsafe {
                (*list_cell.row).semantic_prompt()
            });
        }

        t.deinit(alloc);
    }

    // test "Terminal: OSC133C at x=0 on prompt row clears prompt mark"
    #[test]
    fn terminal_osc133c_at_x0_on_prompt_row_clears_prompt_mark() {
        // This tests the second Fish heuristic: when Fish emits a newline
        // then immediately sends OSC133C (start output) at column 0, we
        // should clear the prompt continuation mark we just set.
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

        // Start a prompt
        t.semantic_prompt(osc::SemanticPrompt::init(
            osc::semantic_prompt::Action::PromptStart,
        ))
        .unwrap();
        for c in "$ echo \\".bytes() {
            t.print(c as u32).unwrap();
        }

        // Simulate Fish behavior: newline first (which marks next row as prompt)
        t.carriage_return();
        t.linefeed().unwrap();

        // Verify the new row is marked as prompt continuation
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 1)).unwrap();
            assert_eq!(pagepkg::SemanticPrompt::PromptContinuation, unsafe {
                (*list_cell.row).semantic_prompt()
            });
        }

        // Now Fish sends OSC133C at column 0 (cursor is still at x=0)
        assert_eq!(0usize, unsafe { (*t.screens.active).cursor.x } as usize);
        t.semantic_prompt(osc::SemanticPrompt::init(
            osc::semantic_prompt::Action::EndInputStartOutput,
        ))
        .unwrap();

        // The prompt continuation should be cleared
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 1)).unwrap();
            assert_eq!(pagepkg::SemanticPrompt::None, unsafe {
                (*list_cell.row).semantic_prompt()
            });
        }

        t.deinit(alloc);
    }

    // test "Terminal: OSC133C at x>0 on prompt row does not clear prompt mark"
    #[test]
    fn terminal_osc133c_at_xgt0_on_prompt_row_does_not_clear_prompt_mark() {
        // If we're not at column 0, we shouldn't clear the prompt mark
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

        // Start a prompt on a row
        t.semantic_prompt(osc::SemanticPrompt::init(
            osc::semantic_prompt::Action::PromptStart,
        ))
        .unwrap();
        for c in "$ ".bytes() {
            t.print(c as u32).unwrap();
        }

        // Move to a new line and mark it as prompt continuation manually
        t.carriage_return();
        t.linefeed().unwrap();
        t.semantic_prompt(osc::SemanticPrompt {
            action: osc::semantic_prompt::Action::PromptStart,
            options_unvalidated: b"k=c".to_vec(),
        })
        .unwrap();
        for c in "> ".bytes() {
            t.print(c as u32).unwrap();
        }

        // Verify the row is marked as prompt continuation
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 1)).unwrap();
            assert_eq!(pagepkg::SemanticPrompt::PromptContinuation, unsafe {
                (*list_cell.row).semantic_prompt()
            });
        }

        // Now send OSC133C but cursor is NOT at column 0
        assert!(unsafe { (*t.screens.active).cursor.x } > 0);
        t.semantic_prompt(osc::SemanticPrompt::init(
            osc::semantic_prompt::Action::EndInputStartOutput,
        ))
        .unwrap();

        // The prompt continuation should NOT be cleared (we're not at x=0)
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 1)).unwrap();
            assert_eq!(pagepkg::SemanticPrompt::PromptContinuation, unsafe {
                (*list_cell.row).semantic_prompt()
            });
        }

        t.deinit(alloc);
    }

    // test "Terminal: multiple newlines in prompt mode marks all rows"
    #[test]
    fn terminal_multiple_newlines_in_prompt_mode_marks_all_rows() {
        // Multiple newlines should each mark their row as prompt continuation
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

        // Start a prompt
        t.semantic_prompt(osc::SemanticPrompt::init(
            osc::semantic_prompt::Action::PromptStart,
        ))
        .unwrap();
        for c in "line1".bytes() {
            t.print(c as u32).unwrap();
        }

        // Multiple newlines
        t.carriage_return();
        t.linefeed().unwrap();
        for c in "line2".bytes() {
            t.print(c as u32).unwrap();
        }
        t.carriage_return();
        t.linefeed().unwrap();
        for c in "line3".bytes() {
            t.print(c as u32).unwrap();
        }

        // First row should be prompt
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
            assert_eq!(pagepkg::SemanticPrompt::Prompt, unsafe {
                (*list_cell.row).semantic_prompt()
            });
        }

        // Second and third rows should be prompt continuation
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 1)).unwrap();
            assert_eq!(pagepkg::SemanticPrompt::PromptContinuation, unsafe {
                (*list_cell.row).semantic_prompt()
            });
        }
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 2)).unwrap();
            assert_eq!(pagepkg::SemanticPrompt::PromptContinuation, unsafe {
                (*list_cell.row).semantic_prompt()
            });
        }

        t.deinit(alloc);
    }

    // test "Terminal: OSC133A click_events=1 sets click to click_events"
    #[test]
    fn terminal_osc133a_click_events_1_sets_click_to_click_events() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

        // Verify default state is none
        assert_eq!(crate::screen::SemanticClick::None, unsafe {
            (*t.screens.active).semantic_prompt.click
        });

        // OSC 133;A with click_events=1
        t.semantic_prompt(osc::SemanticPrompt {
            action: osc::semantic_prompt::Action::FreshLineNewPrompt,
            options_unvalidated: b"click_events=1".to_vec(),
        })
        .unwrap();

        assert_eq!(
            crate::screen::SemanticClick::ClickEvents(osc::semantic_prompt::ClickEvents::Absolute),
            unsafe { (*t.screens.active).semantic_prompt.click }
        );

        t.deinit(alloc);
    }

    // test "Terminal: OSC133A click_events=2 sets click to click_events (relative)"
    #[test]
    fn terminal_osc133a_click_events_2_sets_click_to_click_events_relative() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

        // Verify default state is none
        assert_eq!(crate::screen::SemanticClick::None, unsafe {
            (*t.screens.active).semantic_prompt.click
        });

        // OSC 133;A with click_events=2
        t.semantic_prompt(osc::SemanticPrompt {
            action: osc::semantic_prompt::Action::FreshLineNewPrompt,
            options_unvalidated: b"click_events=2".to_vec(),
        })
        .unwrap();

        assert_eq!(
            crate::screen::SemanticClick::ClickEvents(osc::semantic_prompt::ClickEvents::Relative),
            unsafe { (*t.screens.active).semantic_prompt.click }
        );

        t.deinit(alloc);
    }

    // test "Terminal: OSC133A click_events=0 does not set click_events"
    #[test]
    fn terminal_osc133a_click_events_0_does_not_set_click_events() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

        // OSC 133;A with click_events=0
        t.semantic_prompt(osc::SemanticPrompt {
            action: osc::semantic_prompt::Action::FreshLineNewPrompt,
            options_unvalidated: b"click_events=0".to_vec(),
        })
        .unwrap();

        // Should remain none since click_events=0 doesn't activate anything
        assert_eq!(crate::screen::SemanticClick::None, unsafe {
            (*t.screens.active).semantic_prompt.click
        });

        t.deinit(alloc);
    }

    // test "Terminal: OSC133A cl option sets click to cl value"
    #[test]
    fn terminal_osc133a_cl_option_sets_click_to_cl_value() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

        // OSC 133;A with cl=m (multiple)
        t.semantic_prompt(osc::SemanticPrompt {
            action: osc::semantic_prompt::Action::FreshLineNewPrompt,
            options_unvalidated: b"cl=m".to_vec(),
        })
        .unwrap();

        assert_eq!(
            crate::screen::SemanticClick::Cl(osc::semantic_prompt::Click::Multiple),
            unsafe { (*t.screens.active).semantic_prompt.click }
        );

        t.deinit(alloc);
    }

    // test "Terminal: OSC133A cl=line sets click to line"
    #[test]
    fn terminal_osc133a_cl_line_sets_click_to_line() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

        t.semantic_prompt(osc::SemanticPrompt {
            action: osc::semantic_prompt::Action::FreshLineNewPrompt,
            options_unvalidated: b"cl=line".to_vec(),
        })
        .unwrap();

        assert_eq!(
            crate::screen::SemanticClick::Cl(osc::semantic_prompt::Click::Line),
            unsafe { (*t.screens.active).semantic_prompt.click }
        );

        t.deinit(alloc);
    }

    // test "Terminal: OSC133A click_events=1 takes priority over cl"
    #[test]
    fn terminal_osc133a_click_events_1_takes_priority_over_cl() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

        // OSC 133;A with both click_events=1 and cl=m
        t.semantic_prompt(osc::SemanticPrompt {
            action: osc::semantic_prompt::Action::FreshLineNewPrompt,
            options_unvalidated: b"click_events=1;cl=m".to_vec(),
        })
        .unwrap();

        // click_events should take priority
        assert_eq!(
            crate::screen::SemanticClick::ClickEvents(osc::semantic_prompt::ClickEvents::Absolute),
            unsafe { (*t.screens.active).semantic_prompt.click }
        );

        t.deinit(alloc);
    }

    // test "Terminal: OSC133A click_events=0 falls back to cl"
    #[test]
    fn terminal_osc133a_click_events_0_falls_back_to_cl() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

        // OSC 133;A with click_events=0 and cl=v
        t.semantic_prompt(osc::SemanticPrompt {
            action: osc::semantic_prompt::Action::FreshLineNewPrompt,
            options_unvalidated: b"click_events=0;cl=v".to_vec(),
        })
        .unwrap();

        // Should fall back to cl since click_events is disabled
        assert_eq!(
            crate::screen::SemanticClick::Cl(osc::semantic_prompt::Click::ConservativeVertical),
            unsafe { (*t.screens.active).semantic_prompt.click }
        );

        t.deinit(alloc);
    }

    // test "Terminal: OSC133A no click options leaves click as none"
    #[test]
    fn terminal_osc133a_no_click_options_leaves_click_as_none() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

        // OSC 133;A with no click-related options
        t.semantic_prompt(osc::SemanticPrompt {
            action: osc::semantic_prompt::Action::FreshLineNewPrompt,
            options_unvalidated: b"aid=123".to_vec(),
        })
        .unwrap();

        assert_eq!(crate::screen::SemanticClick::None, unsafe {
            (*t.screens.active).semantic_prompt.click
        });

        t.deinit(alloc);
    }

    // test "Terminal: cursorIsAtPrompt"
    #[test]
    fn terminal_cursor_is_at_prompt() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(10, 3)).unwrap();

        assert!(!t.cursor_is_at_prompt());
        t.semantic_prompt(osc::SemanticPrompt::init(
            osc::semantic_prompt::Action::PromptStart,
        ))
        .unwrap();
        assert!(t.cursor_is_at_prompt());
        for c in "$ ".bytes() {
            t.print(c as u32).unwrap();
        }

        // Input is also a prompt
        t.semantic_prompt(osc::SemanticPrompt::init(
            osc::semantic_prompt::Action::EndPromptStartInput,
        ))
        .unwrap();
        assert!(t.cursor_is_at_prompt());
        for c in "ls".bytes() {
            t.print(c as u32).unwrap();
        }

        // But once we say we're starting output, we're not a prompt
        // (cursor is not at x=0, so the Fish heuristic doesn't trigger)
        t.semantic_prompt(osc::SemanticPrompt::init(
            osc::semantic_prompt::Action::EndInputStartOutput,
        ))
        .unwrap();
        // Still a prompt because this line has a prompt
        assert!(t.cursor_is_at_prompt());
        t.linefeed().unwrap();
        assert!(!t.cursor_is_at_prompt());

        // Until we know we're at a prompt again
        t.linefeed().unwrap();
        t.semantic_prompt(osc::SemanticPrompt::init(
            osc::semantic_prompt::Action::PromptStart,
        ))
        .unwrap();
        assert!(t.cursor_is_at_prompt());

        t.deinit(alloc);
    }

    // test "Terminal: cursorIsAtPrompt alternate screen"
    #[test]
    fn terminal_cursor_is_at_prompt_alternate_screen() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(3, 2)).unwrap();

        assert!(!t.cursor_is_at_prompt());
        t.semantic_prompt(osc::SemanticPrompt::init(
            osc::semantic_prompt::Action::PromptStart,
        ))
        .unwrap();
        assert!(t.cursor_is_at_prompt());

        // Secondary screen is never a prompt
        t.switch_screen_mode(SwitchScreenMode::M1049, true).unwrap();
        assert!(!t.cursor_is_at_prompt());
        t.semantic_prompt(osc::SemanticPrompt::init(
            osc::semantic_prompt::Action::PromptStart,
        ))
        .unwrap();
        assert!(!t.cursor_is_at_prompt());

        t.deinit(alloc);
    }

    // PORT NOTE: pwd/title round-trip tests. The reference `Terminal.zig` has no
    // dedicated tests for setPwd/getPwd/setTitle/getTitle, so these exercise the
    // ported `Vec<u8>` + trailing-0 sentinel contract (getters return the
    // NUL-stripped byte slice or None) with independent expected values.
    #[test]
    fn terminal_set_get_pwd() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

        // Empty by default.
        assert!(t.get_pwd().is_none());

        // Set a pwd and read it back NUL-stripped.
        t.set_pwd(b"/home/user").unwrap();
        assert_eq!(Some(b"/home/user".as_slice()), t.get_pwd());
        // The backing buffer carries a trailing NUL sentinel.
        assert_eq!(b"/home/user\x00".as_slice(), t.pwd.as_slice());

        // Setting again clears the prior value first.
        t.set_pwd(b"/tmp").unwrap();
        assert_eq!(Some(b"/tmp".as_slice()), t.get_pwd());

        // Setting empty clears it back to None (no sentinel appended).
        t.set_pwd(b"").unwrap();
        assert!(t.get_pwd().is_none());
        assert!(t.pwd.is_empty());

        t.deinit(alloc);
    }

    #[test]
    fn terminal_set_get_title() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

        // Empty by default.
        assert!(t.get_title().is_none());

        // Set a title and read it back NUL-stripped.
        t.set_title(b"hello world").unwrap();
        assert_eq!(Some(b"hello world".as_slice()), t.get_title());
        // The backing buffer carries a trailing NUL sentinel.
        assert_eq!(b"hello world\x00".as_slice(), t.title.as_slice());

        // Setting again clears the prior value first.
        t.set_title(b"x").unwrap();
        assert_eq!(Some(b"x".as_slice()), t.get_title());

        // Setting empty clears it back to None (no sentinel appended).
        t.set_title(b"").unwrap();
        assert!(t.get_title().is_none());
        assert!(t.title.is_empty());

        t.deinit(alloc);
    }

    // ===========================================================================
    // vt-12 recovery: Terminal input-sweep family + full-diff stragglers.
    // ===========================================================================

    // test "Terminal: setCursorPos saturates overflowing origin offsets"
    // PORT NOTE (parity 0aaedf436): test hunk of the upstream saturating
    // origin-offset fix in setCursorPos.
    #[test]
    fn terminal_set_cursor_pos_saturates_overflowing_origin_offsets() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(10, 10)).unwrap();

        t.scrolling_region = ScrollingRegion {
            top: 2,
            bottom: 7,
            left: 3,
            right: 8,
        };
        t.modes.set(modespkg::Mode::Origin, true);

        t.set_cursor_pos(usize::MAX, usize::MAX);
        // try testing.expectEqual(@as(size.CellCountInt, 8), t.screens.active.cursor.x);
        assert_eq!(8 as size::CellCountInt, unsafe {
            (*t.screens.active).cursor.x
        });
        // try testing.expectEqual(@as(size.CellCountInt, 7), t.screens.active.cursor.y);
        assert_eq!(7 as size::CellCountInt, unsafe {
            (*t.screens.active).cursor.y
        });

        t.deinit(alloc);
    }

    // test "Terminal: input with no control characters"
    #[test]
    fn terminal_input_with_no_control_characters() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(40, 40)).unwrap();

        // Basic grid writing
        for c in "hello".chars() {
            t.print(c as u32).unwrap();
        }
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 5);
        {
            let str = t.plain_string(alloc).unwrap();
            assert_eq!(str.as_slice(), "hello".as_bytes());
        }

        // The first row should be dirty
        assert!(t.is_dirty(screen_pt(5, 0)));
        assert!(!t.is_dirty(screen_pt(5, 1)));

        t.deinit(alloc);
    }

    // test "Terminal: input with basic wraparound"
    #[test]
    fn terminal_input_with_basic_wraparound() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(5, 40)).unwrap();

        // Basic grid writing
        for c in "helloworldabc12".chars() {
            t.print(c as u32).unwrap();
        }
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 2);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 4);
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
        {
            let str = t.plain_string(alloc).unwrap();
            assert_eq!(str.as_slice(), "hello\nworld\nabc12".as_bytes());
        }

        t.deinit(alloc);
    }

    // test "Terminal: input with basic wraparound dirty"
    #[test]
    fn terminal_input_with_basic_wraparound_dirty() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(5, 40)).unwrap();

        for c in "hello".chars() {
            t.print(c as u32).unwrap();
        }
        assert!(t.is_dirty(screen_pt(4, 0)));
        t.clear_dirty();
        t.print('w' as u32).unwrap();

        // Old row is dirty because cursor moved from there
        assert!(t.is_dirty(screen_pt(4, 0)));
        assert!(t.is_dirty(screen_pt(0, 1)));

        t.deinit(alloc);
    }

    // test "Terminal: input that forces scroll"
    #[test]
    fn terminal_input_that_forces_scroll() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(1, 5)).unwrap();

        // Basic grid writing
        for c in "abcdef".chars() {
            t.print(c as u32).unwrap();
        }
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 4);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 0);
        {
            let str = t.plain_string(alloc).unwrap();
            assert_eq!(str.as_slice(), "b\nc\nd\ne\nf".as_bytes());
        }

        t.deinit(alloc);
    }

    // test "Terminal: input unique style per cell"
    #[test]
    fn terminal_input_unique_style_per_cell() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(30, 30)).unwrap();

        for y in 0..t.rows {
            for x in 0..t.cols {
                t.set_cursor_pos(y as usize, x as usize);
                t.set_attribute(sgr::Attribute::DirectColorBg(color::RGB {
                    r: x as u8,
                    g: y as u8,
                    b: 0,
                }))
                .unwrap();
                t.print('x' as u32).unwrap();
            }
        }

        t.deinit(alloc);
    }

    // test "Terminal: input glitch text"
    #[cfg_attr(
        all(miri, debug_assertions),
        ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
    )]
    #[test]
    fn terminal_input_glitch_text() {
        const GLITCH: &[u8] = include_bytes!("res/glitch.txt");
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(30, 30)).unwrap();

        // Get our initial grapheme capacity.
        let grapheme_cap = {
            let page = t.active_mut().pages.pages.first;
            unsafe { crate::pagelist::Node::capacity(page).grapheme_bytes }
        };

        // Print glitch text until our capacity changes
        loop {
            let page = t.active_mut().pages.pages.first;
            if unsafe { crate::pagelist::Node::capacity(page).grapheme_bytes } != grapheme_cap {
                break;
            }
            t.print_string(GLITCH).unwrap();
        }

        // We're testing to make sure that grapheme capacity gets increased.
        let page = t.active_mut().pages.pages.first;
        assert!(unsafe { crate::pagelist::Node::capacity(page).grapheme_bytes } > grapheme_cap);

        t.deinit(alloc);
    }

    // test "Terminal: zero-width character at start"
    #[test]
    fn terminal_zero_width_character_at_start() {
        let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

        // This used to crash the terminal. This is not allowed so we should
        // just ignore it.
        //
        // S141-F4: `print_inner_zero_width`'s `x == 0` guard logs
        // `log.warn("zero-width character with no prior character,
        // ignoring", .{})` (Terminal.zig:1480) — this used to crash the
        // terminal, so it's ignored instead.
        let cap = crate::log_capture::start();
        t.print(0x200D).unwrap();
        let messages = cap.messages();
        assert!(
            messages
                .iter()
                .any(|(level, target, msg)| *level == log::Level::Warn
                    && target == "terminal"
                    && *msg == "zero-width character with no prior character, ignoring"),
            "expected the zero-width-no-prior-character diagnostic, got: {messages:?}"
        );
        drop(cap);

        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 0);

        // Should not be dirty since we changed nothing.
        assert!(!t.is_dirty(screen_pt(0, 0)));

        t.deinit(Global);
    }

    // test "Terminal: ignored VS16 doesn't mark dirty"
    #[test]
    fn terminal_ignored_vs16_doesnt_mark_dirty() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        // Disable grapheme clustering
        t.modes.set(modespkg::Mode::GraphemeCluster, false);

        t.print(0x2764).unwrap(); // Heart
        assert!(t.is_dirty(screen_pt(0, 0)));

        t.clear_dirty();
        t.print(0xFE0F).unwrap(); // VS16 to make wide
        assert!(!t.is_dirty(screen_pt(0, 0)));

        t.deinit(Global);
    }

    // test "Terminal: invalid VS16 doesn't mark dirty"
    #[test]
    fn terminal_invalid_vs16_doesnt_mark_dirty() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        // Disable grapheme clustering
        t.modes.set(modespkg::Mode::GraphemeCluster, false);

        t.print('x' as u32).unwrap();
        assert!(t.is_dirty(screen_pt(0, 0)));

        t.clear_dirty();
        t.print(0xFE0F).unwrap(); // VS16 to make wide
        assert!(!t.is_dirty(screen_pt(0, 0)));

        t.deinit(Global);
    }

    // test "Terminal: multicodepoint grapheme marks dirty on every codepoint"
    #[test]
    fn terminal_multicodepoint_grapheme_marks_dirty_on_every_codepoint() {
        let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

        // Enable grapheme clustering
        t.modes.set(modespkg::Mode::GraphemeCluster, true);

        // https://github.com/mitchellh/ghostty/issues/289
        // This is: 👨‍👩‍👧 (which may or may not render correctly)
        t.print(0x1F468).unwrap();
        assert!(t.is_dirty(screen_pt(0, 0)));
        t.clear_dirty();
        t.print(0x200D).unwrap();
        assert!(t.is_dirty(screen_pt(0, 0)));
        t.clear_dirty();
        t.print(0x1F469).unwrap();
        assert!(t.is_dirty(screen_pt(0, 0)));
        t.clear_dirty();
        t.print(0x200D).unwrap();
        assert!(t.is_dirty(screen_pt(0, 0)));
        t.clear_dirty();
        t.print(0x1F467).unwrap();
        assert!(t.is_dirty(screen_pt(0, 0)));

        // We should have 2 cells taken up. It is one character but "wide".
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);

        t.deinit(Global);
    }

    // test "Terminal: horizontal tab back with cursor before left margin"
    #[test]
    fn terminal_horizontal_tab_back_with_cursor_before_left_margin() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(20, 5)).unwrap();

        t.modes.set(modespkg::Mode::Origin, true);
        t.save_cursor();
        t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
        t.set_left_and_right_margin(5, 0);
        t.restore_cursor();
        t.horizontal_tab_back();
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "X".as_bytes());
        }

        t.deinit(alloc);
    }

    // Probably outdated, but dates back to the original terminal implementation.
    // test "Terminal: setCursorPos (original test)"
    #[test]
    fn terminal_set_cursor_pos_original_test() {
        let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);

        // Setting it to 0 should keep it zero (1 based)
        t.set_cursor_pos(0, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);

        // Should clamp to size
        t.set_cursor_pos(81, 81);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 79);
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 79);

        // Should reset pending wrap
        t.set_cursor_pos(0, 80);
        t.print('c' as u32).unwrap();
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
        t.set_cursor_pos(0, 80);
        assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });

        // Origin mode
        t.modes.set(modespkg::Mode::Origin, true);

        // No change without a scroll region
        t.set_cursor_pos(81, 81);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 79);
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 79);

        // Set the scroll region
        t.set_top_and_bottom_margin(10, t.rows as usize);
        t.set_cursor_pos(0, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 9);

        t.set_cursor_pos(1, 1);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 9);

        t.set_cursor_pos(100, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 79);

        t.set_top_and_bottom_margin(10, 11);
        t.set_cursor_pos(2, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 10);

        t.deinit(Global);
    }

    // test "Terminal: garbage collect overwritten"
    #[test]
    fn terminal_garbage_collect_overwritten() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        t.set_attribute(sgr::Attribute::Bold).unwrap();
        t.print('A' as u32).unwrap();
        t.set_cursor_pos(1, 1);
        t.set_attribute(sgr::Attribute::Unset).unwrap();
        t.print('B' as u32).unwrap();

        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 'B' as u32);
            assert!(unsafe { (*cell).style_id() } == 0);
        }

        // verify we have no styles in our style map
        let page =
            unsafe { crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node) };
        assert_eq!(0usize, unsafe { (*page).styles.count() });

        t.deinit(alloc);
    }

    // test "Terminal: do not garbage collect old styles in use"
    #[test]
    fn terminal_do_not_garbage_collect_old_styles_in_use() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        t.set_attribute(sgr::Attribute::Bold).unwrap();
        t.print('A' as u32).unwrap();
        t.set_attribute(sgr::Attribute::Unset).unwrap();
        t.print('B' as u32).unwrap();

        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 'B' as u32);
            assert!(unsafe { (*cell).style_id() } == 0);
        }

        // verify we have no styles in our style map
        let page =
            unsafe { crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node) };
        assert_eq!(1usize, unsafe { (*page).styles.count() });

        t.deinit(alloc);
    }

    // test "Terminal: DECALN"
    #[test]
    fn terminal_decaln() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(2, 2)).unwrap();

        // Initial value
        t.print('A' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('B' as u32).unwrap();
        t.decaln().unwrap();

        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 0);

        for y in 0..t.rows {
            assert!(t.is_dirty(active_pt(0, y as u32)));
        }

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "EE\nEE".as_bytes());
        }

        t.deinit(alloc);
    }

    // test "Terminal: setProtectedMode"
    #[test]
    fn terminal_set_protected_mode() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();

        assert!(!unsafe { (*t.screens.active).cursor.protected });
        t.set_protected_mode(ansi::ProtectedMode::Off);
        assert!(!unsafe { (*t.screens.active).cursor.protected });
        t.set_protected_mode(ansi::ProtectedMode::Iso);
        assert!(unsafe { (*t.screens.active).cursor.protected });
        t.set_protected_mode(ansi::ProtectedMode::Dec);
        assert!(unsafe { (*t.screens.active).cursor.protected });
        t.set_protected_mode(ansi::ProtectedMode::Off);
        assert!(!unsafe { (*t.screens.active).cursor.protected });

        t.deinit(alloc);
    }

    /// The activity token changes when the page list mutates, and does NOT
    /// change merely because compression ran.
    ///
    /// PORT NOTE: this test has no reference counterpart; the compression API
    /// is exercised upstream only through `PageList`. It pins the two
    /// properties `Terminal::compression_activity` promises to its callers.
    #[test]
    fn terminal_compression_activity_tracks_pagelist_mutation_not_compression() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts_sb(80, 24, 10 * 1024 * 1024)).unwrap();
        let initial = t.compression_activity();

        let mut stream = t.vt_stream();
        let line = b"repeated and compressible terminal history\r\n";
        for _ in 0..4_000 {
            stream.next_slice(line);
        }
        stream.deinit();
        drop(stream);

        let activity = t.compression_activity();
        assert_ne!(activity, initial);

        let mut result = TerminalCompressionResult::Pending;
        for _ in 0..1_000 {
            result = t.compress(TerminalCompressionMode::Incremental);
            match result {
                TerminalCompressionResult::Pending => continue,
                TerminalCompressionResult::Complete => break,
                TerminalCompressionResult::Unsupported => {
                    panic!("test builds always support strict reclamation")
                }
            }
        }
        assert_eq!(result, TerminalCompressionResult::Complete);

        // Compression changes storage representation, not the activity token.
        assert_eq!(t.compression_activity(), activity);

        t.deinit(alloc);
    }

    /// PINS THE `compress(Incremental)` RESTART (WS19 T18, audit row N10).
    ///
    /// A fresh incremental call issued immediately after a pass returned `Complete`
    /// returns **`Pending`**, not `Complete`: the incremental traversal restarts from
    /// the beginning instead of remembering that it converged. That makes the obvious
    /// "is compression done?" probe — call it once and look for `Complete` — invalid,
    /// and it is the opposite of what the reference doc comment ("whether they should
    /// continue calling it in the future") suggests to a reader.
    ///
    /// This test exists so the `PORT NOTE` on `Terminal::compress` cannot rot: if a
    /// future change ever makes a converged pass STAY `Complete`, this test fails and
    /// whoever changes it must update the doc comment in the same commit. Referenced
    /// BY NAME from that PORT NOTE, so a reader who doubts the claim can run it.
    ///
    /// PORT NOTE: no reference counterpart — upstream exercises compression only
    /// through `PageList`, and documents nothing about the restart.
    #[test]
    fn terminal_compress_incremental_restarts_after_complete() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts_sb(80, 24, 10 * 1024 * 1024)).unwrap();

        let mut stream = t.vt_stream();
        let line = b"repeated and compressible terminal history\r\n";
        for _ in 0..4_000 {
            stream.next_slice(line);
        }
        stream.deinit();
        drop(stream);

        // Drive one incremental pass to convergence.
        let mut result = TerminalCompressionResult::Pending;
        let mut steps = 0usize;
        for _ in 0..10_000 {
            result = t.compress(TerminalCompressionMode::Incremental);
            steps += 1;
            match result {
                TerminalCompressionResult::Pending => continue,
                TerminalCompressionResult::Complete => break,
                TerminalCompressionResult::Unsupported => {
                    panic!("test builds always support strict reclamation")
                }
            }
        }
        assert_eq!(
            result,
            TerminalCompressionResult::Complete,
            "the incremental pass must converge first, or this test proves nothing \
             (steps={steps})"
        );
        assert!(
            steps > 1,
            "the pass converged in ONE call, so `Complete` here carries no information \
             about a restart — the fixture is not compressible enough to be a valid \
             discriminator (steps={steps})"
        );

        // THE PROPERTY: the very next call is Pending again, not Complete.
        let again = t.compress(TerminalCompressionMode::Incremental);
        assert_eq!(
            again,
            TerminalCompressionResult::Pending,
            "a fresh compress(Incremental) right after a Complete pass must return \
             Pending, because the incremental traversal RESTARTS. If this now returns \
             Complete, the behavior changed — update the PORT NOTE on \
             `Terminal::compress` and the note in .port/phase-2/MIRI_EXCEPTIONS.md in \
             the same commit."
        );

        t.deinit(alloc);
    }

    // -- P3-W1 T4 step 1: stored grapheme boundaries (oracle b287f6d1a) -------

    // ref: test "Terminal: enabling grapheme mode handles stored breaks"
    #[test]
    fn terminal_enabling_grapheme_mode_handles_stored_breaks() {
        let mut t = Terminal::init(Global, opts(5, 1)).unwrap();

        t.modes.set(modespkg::Mode::GraphemeCluster, false);
        t.print(b'a' as u21).unwrap();
        t.print(0x200B).unwrap(); // Zero width space is stored on the prior cell.

        t.modes.set(modespkg::Mode::GraphemeCluster, true);
        t.print(0x0301).unwrap();

        let str = t.plain_string(Global).unwrap();
        assert_eq!(b"a\xE2\x80\x8B\xCC\x81".as_slice(), str.as_slice());

        t.deinit(Global);
    }

    /// `REACHABILITY.md` row 5 demanded that this be driven END TO END through
    /// `Stream`, not by calling `print` directly, before the row could be
    /// ranked. This is that drive: the mode toggles are real DECSET/DECRST
    /// sequences (`CSI ? 2027 h` / `l`) and the codepoints are real UTF-8 bytes
    /// off the wire, so nothing here depends on a test reaching into `print`.
    /// On today's pre-fix tree this panics in `print`'s stored-cluster walk.
    ///
    /// The release-mode consequence, which row 5 also asked for: the site is a
    /// `debug_assert!`, so in release it is COMPILED OUT and the pre-fix code
    /// silently produced the same output as the fixed code — the defect is a
    /// debug/ReleaseSafe-only crash, not a release miscompute. That is why this
    /// test is a plain assertion on the resulting string: it is meaningful in
    /// both profiles, and in debug it also catches the panic.
    #[test]
    fn terminal_enabling_grapheme_mode_handles_stored_breaks_via_stream() {
        let mut t = Terminal::init(Global, opts(5, 1)).unwrap();

        {
            let mut stream = t.vt_stream();
            // CSI ? 2027 l  -- disable grapheme clustering
            stream.next_slice(b"\x1b[?2027l");
            // "a" then U+200B ZERO WIDTH SPACE, attached with no boundary rules
            stream.next_slice("a\u{200B}".as_bytes());
            // CSI ? 2027 h  -- enable grapheme clustering
            stream.next_slice(b"\x1b[?2027h");
            // U+0301 COMBINING ACUTE ACCENT
            stream.next_slice("\u{0301}".as_bytes());
            stream.deinit();
        }

        let str = t.plain_string(Global).unwrap();
        assert_eq!(b"a\xE2\x80\x8B\xCC\x81".as_slice(), str.as_slice());

        t.deinit(Global);
    }

    // =========================================================================
    // Kitty graphics state on the scroll / erase-display / screen-switch routes.
    //
    // PORT NOTE: NOT ports of reference `test {}` blocks — the reference has no
    // tests for these writes. They are the parity gate for the
    // `kitty_images.dirty = true` sets in `index` / `scroll_up` / `insert_lines`
    // / `delete_lines` / `switch_screen` (Terminal.zig:2264, 2471, 2720, 2886,
    // 4509) and for the `kitty_images.delete(.{ .all = true })` calls in
    // `erase_display` (Terminal.zig:3329-3337 and 3387-3397). All of those were
    // omitted behind a stale "vt-12 deferral" PORT NOTE even though Kitty
    // graphics is ENABLED (`options::KITTY_GRAPHICS`) and `ImageStorage` is
    // live. Every case below is one the pre-fix tree got wrong: the routes are
    // chosen so the flag can NOT come from the `Screen`-level helpers
    // (`cursor_down_scroll` / `cursor_scroll_above` / `scroll`), i.e. they are
    // the region / margin routes, not the full-screen scrollback ones.
    // =========================================================================

    fn kitty_dirty(t: &Terminal) -> bool {
        // SAFETY: `screens.active` is a stable heap pointer owned by `screens`.
        unsafe { (*t.screens.active).kitty_images.dirty }
    }

    /// Clear the `dirty` flag on EVERY screen, which is what a renderer does
    /// once it has drawn the images.
    fn clear_kitty_dirty(t: &mut Terminal) {
        for slot in t.screens.all.iter() {
            if let Some(s) = *slot {
                // SAFETY: stable heap pointers owned by `screens`.
                unsafe { (*s).kitty_images.dirty = false };
            }
        }
    }

    // Terminal.zig:2264 — index() inside a top/bottom scrolling region.
    #[test]
    fn terminal_kitty_dirty_index_top_bottom_region() {
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

        t.set_top_and_bottom_margin(2, 4);
        t.set_cursor_pos(4, 1);
        clear_kitty_dirty(&mut t);
        t.index().unwrap();

        assert!(kitty_dirty(&t));

        t.deinit(Global);
    }

    // Terminal.zig:2264 — index() inside left/right margins.
    #[test]
    fn terminal_kitty_dirty_index_left_right_margin() {
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

        t.set_top_and_bottom_margin(1, 3);
        t.scrolling_region.left = 3;
        t.scrolling_region.right = 5;
        t.set_cursor_pos(3, 4);
        clear_kitty_dirty(&mut t);
        t.index().unwrap();

        assert!(kitty_dirty(&t));

        t.deinit(Global);
    }

    // Terminal.zig:2471 — scrollUp's scrollback branch with a partial region.
    #[test]
    fn terminal_kitty_dirty_scroll_up_top_partial_region() {
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

        t.set_top_and_bottom_margin(1, 3);
        clear_kitty_dirty(&mut t);
        t.scroll_up(1).unwrap();

        assert!(kitty_dirty(&t));

        t.deinit(Global);
    }

    // Terminal.zig:2471/2886 — scrollUp with left/right margins routes through
    // deleteLines, which has its own dirty write.
    #[test]
    fn terminal_kitty_dirty_scroll_up_left_right_margin() {
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

        t.set_top_and_bottom_margin(1, 3);
        t.scrolling_region.left = 3;
        t.scrolling_region.right = 5;
        t.set_cursor_pos(1, 4);
        clear_kitty_dirty(&mut t);
        t.scroll_up(1).unwrap();

        assert!(kitty_dirty(&t));

        t.deinit(Global);
    }

    // Terminal.zig:2720 — insertLines.
    #[test]
    fn terminal_kitty_dirty_insert_lines() {
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

        t.set_top_and_bottom_margin(2, 4);
        t.set_cursor_pos(2, 1);
        clear_kitty_dirty(&mut t);
        t.insert_lines(1);

        assert!(kitty_dirty(&t));

        t.deinit(Global);
    }

    // Terminal.zig:2886 — deleteLines.
    #[test]
    fn terminal_kitty_dirty_delete_lines() {
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

        t.set_top_and_bottom_margin(2, 4);
        t.set_cursor_pos(2, 1);
        clear_kitty_dirty(&mut t);
        t.delete_lines(1);

        assert!(kitty_dirty(&t));

        t.deinit(Global);
    }

    // Terminal.zig:2320 -> 2423 -> 2720 — reverseIndex at the top of the region
    // inherits the dirty write from scrollDown/insertLines, exactly as the
    // reference does (it has no write of its own).
    #[test]
    fn terminal_kitty_dirty_reverse_index_at_region_top() {
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

        t.set_top_and_bottom_margin(2, 4);
        t.set_cursor_pos(2, 1);
        clear_kitty_dirty(&mut t);
        t.reverse_index();

        assert!(kitty_dirty(&t));

        t.deinit(Global);
    }

    // Terminal.zig:4509 — switchScreen marks the DESTINATION screen's images
    // dirty on every switch, including re-entry into an already-created screen
    // whose flag a renderer has since cleared.
    #[test]
    fn terminal_kitty_dirty_switch_screen_marks_destination() {
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

        // First switch: the alternate screen is lazily created here.
        clear_kitty_dirty(&mut t);
        t.switch_screen(crate::screen_set::Key::Alternate).unwrap();
        assert!(kitty_dirty(&t), "alternate screen, first entry");

        // Re-entry into the ALREADY-CREATED primary (the primary always
        // exists, so this arm can never be excused as "fresh screens have no
        // images yet").
        clear_kitty_dirty(&mut t);
        t.switch_screen(crate::screen_set::Key::Primary).unwrap();
        assert!(kitty_dirty(&t), "primary screen, re-entry");

        // Re-entry into the already-created alternate.
        clear_kitty_dirty(&mut t);
        t.switch_screen(crate::screen_set::Key::Alternate).unwrap();
        assert!(kitty_dirty(&t), "alternate screen, re-entry");

        t.deinit(Global);
    }

    /// Put one image plus one (non-virtual) placement into the active screen's
    /// Kitty image storage, the state `kitty_images.delete(.{ .all = true })`
    /// is supposed to tear down.
    fn seed_kitty_image(t: &mut Terminal) {
        use crate::kitty::graphics_image::Image;
        use crate::kitty::graphics_storage::{Location, Placement};

        // SAFETY: `screens.active` is a stable heap pointer owned by `screens`
        // and we hold `&mut t`. Each access re-derives its reference from the
        // raw base pointer so no alias outlives its statement (the storage's
        // `s: *mut Screen` PORT NOTE shape).
        let sp = t.screens.active;
        unsafe { &mut (*sp).kitty_images }
            .add_image(
                Global,
                sp,
                Image {
                    id: 1,
                    width: 4,
                    height: 4,
                    ..Image::default()
                },
            )
            .unwrap();
        let pin = {
            let pages = unsafe { &mut (*sp).pages };
            let pin = pages
                .pin(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
                .unwrap();
            pages.track_pin(pin).unwrap()
        };
        unsafe { &mut (*sp).kitty_images }
            .add_placement(
                Global,
                sp,
                1,
                0,
                Placement::with_location(Location::Pin(pin)),
            )
            .unwrap();

        assert_eq!(1, unsafe { &(*sp).kitty_images }.images.len());
        assert_eq!(1, unsafe { &(*sp).kitty_images }.placements.len());
    }

    fn kitty_image_counts(t: &Terminal) -> (usize, usize) {
        // SAFETY: `screens.active` is a stable heap pointer owned by `screens`.
        let s = unsafe { &(*t.screens.active).kitty_images };
        (s.images.len(), s.placements.len())
    }

    // Terminal.zig:3387-3397 — ED complete clears all Kitty graphics state.
    #[test]
    fn terminal_erase_display_complete_deletes_kitty_images() {
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();
        seed_kitty_image(&mut t);

        t.erase_display(csi::EraseDisplay::Complete, false);

        assert_eq!((0, 0), kitty_image_counts(&t));

        t.deinit(Global);
    }

    // Terminal.zig:3329-3337 — ED scroll-complete clears all Kitty graphics
    // state too.
    #[test]
    fn terminal_erase_display_scroll_complete_deletes_kitty_images() {
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();
        seed_kitty_image(&mut t);

        t.erase_display(csi::EraseDisplay::ScrollComplete, false);

        assert_eq!((0, 0), kitty_image_counts(&t));

        t.deinit(Global);
    }

    // Terminal.zig:5512-ish — DECCOLM inherits the clear through
    // eraseDisplay(.complete).
    #[test]
    fn terminal_deccolm_deletes_kitty_images() {
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();
        t.modes.set(modespkg::Mode::EnableMode3, true);
        seed_kitty_image(&mut t);

        t.deccolm(Global, DeccolmMode::Cols132).unwrap();

        assert_eq!((0, 0), kitty_image_counts(&t));

        t.deinit(Global);
    }

    // =========================================================================
    // W29-B03: cross-page wide-grapheme transfer (`print`'s `transfer_graphemes`
    // block, Terminal.zig:1247-1266).
    //
    // PORT NOTE: NOT a port of a reference `test {}` block — the reference has
    // no test for this block at all. It is the correctness gate for the
    // raw-ptr+len iteration of the OLD page's grapheme slice: the copy it
    // replaced (`cps.to_vec()`) was a Rust-only allocation, and the ONLY thing
    // that makes iterating the old page in place legal is that this arm requires
    // `old_pin.node != new_pin.node` while `Screen::append_grapheme` mutates and
    // reallocates the CURSOR's node only. The negative control for the
    // allocation itself lives in `tests/print_grapheme_transfer_alloc.rs`, which
    // counts allocations; this one gates the DATA.
    // =========================================================================

    /// Drive `print` until a wide-grapheme wrap lands on a freshly-created page
    /// (`new_pin.node != old_pin.node`), then hand back the terminal parked
    /// right after that print. `None` if the seam never happened.
    const COLS: size::CellCountInt = 120;

    /// Drive `print` until a wide-grapheme wrap lands on a freshly-created page
    /// (`new_pin.node != old_pin.node`, the cross-page arm of
    /// `transfer_graphemes`). Returns false if that never happened.
    fn drive_wide_grapheme_page_seam(t: &mut Terminal) -> bool {
        t.modes.set(modespkg::Mode::GraphemeCluster, true);

        // Pad the pagelist with blank rows until the cursor's page has EXACTLY
        // one free row left. Then the `linefeed` below takes that last row and
        // the soft wrap after it is forced to start a new page — which is the
        // only way to reach the `old_pin.node != new_pin.node` arm.
        let mut guard = 0usize;
        loop {
            // SAFETY: stable pointers owned by the active screen; the cursor is
            // parked on the last page because it sits on the bottom row.
            let node = unsafe { (*(*t.screens.active).cursor.page_pin).node };
            let page = unsafe { crate::pagelist::Node::page(node) };
            let free = unsafe { (*page).capacity.rows - (*page).size.rows };
            if free == 1 {
                break;
            }
            guard += 1;
            if guard > 100_000 {
                return false;
            }
            t.carriage_return();
            t.linefeed().unwrap();
        }

        t.carriage_return();
        t.linefeed().unwrap();

        // Park the cursor on the last column with a NARROW base that already
        // carries grapheme data...
        for _ in 0..(COLS - 1) {
            t.print('a' as u32).unwrap();
        }
        t.print(0x261D).unwrap(); // U+261D BLACK UP POINTING INDEX (ExtPict, width 1)
        for _ in 0..7 {
            t.print(0x0301).unwrap(); // U+0301 COMBINING ACUTE ACCENT
        }
        t.print(0x200D).unwrap(); // U+200D ZERO WIDTH JOINER

        // SAFETY: stable pointers owned by the active screen.
        let before = unsafe { (*(*t.screens.active).cursor.page_pin).node };
        // ...then make the cluster WIDE, which wraps it to the next row and has
        // to carry the old page's grapheme data with it.
        t.print(0x1F600).unwrap(); // U+1F600 GRINNING FACE (ExtPict, width 2)
        let after = unsafe { (*(*t.screens.active).cursor.page_pin).node };

        !core::ptr::eq(before, after)
    }

    #[test]
    fn terminal_print_wide_grapheme_transfers_across_a_page_seam() {
        let mut t = Terminal::init(Global, opts(COLS, 5)).unwrap();

        assert!(
            drive_wide_grapheme_page_seam(&mut t),
            "VACUOUS: never produced a wrap that crossed a page seam, so the \
             cross-page arm of `transfer_graphemes` was never exercised"
        );

        // SAFETY: stable pointers owned by the active screen.
        let pin = unsafe { *(*t.screens.active).cursor.page_pin };

        // The wide cluster now lives at x=0 of the new page's row, carrying
        // every codepoint that was on the old page plus the one just printed.
        let mut new_pin = pin;
        new_pin.x = 0;
        let new_rac = new_pin.row_and_cell();
        let new_page = unsafe { crate::pagelist::Node::page(new_pin.node) };
        assert_eq!(0x261D, unsafe { (*new_rac.cell).content_codepoint() });
        assert_eq!(pagepkg::Wide::Wide, unsafe { (*new_rac.cell).wide() });
        assert!(unsafe { (*new_rac.cell).has_grapheme() });
        assert_eq!(
            &[0x0301, 0x0301, 0x0301, 0x0301, 0x0301, 0x0301, 0x0301, 0x200D, 0x1F600][..],
            unsafe { (*new_page).lookup_grapheme(new_rac.cell) }.unwrap(),
        );
        assert!(unsafe { (*new_rac.row).grapheme() });

        // ...and the old page's cell has been emptied: it is the spacer head of
        // the wrap, with its grapheme data cleared.
        let mut old_pin = new_pin.up(1).unwrap();
        old_pin.x = COLS - 1;
        assert!(
            !core::ptr::eq(old_pin.node, new_pin.node),
            "VACUOUS: the rows are on the same page"
        );
        let old_rac = old_pin.row_and_cell();
        let old_page = unsafe { crate::pagelist::Node::page(old_pin.node) };
        assert_eq!(0, unsafe { (*old_rac.cell).content_codepoint() });
        assert_eq!(pagepkg::Wide::SpacerHead, unsafe { (*old_rac.cell).wide() });
        assert!(!unsafe { (*old_rac.cell).has_grapheme() });
        assert!(unsafe { (*old_page).lookup_grapheme(old_rac.cell) }.is_none());

        t.deinit(Global);
    }

    // test "Terminal: scroll region linefeed recycled row has default metadata"
    #[test]
    fn terminal_scroll_region_linefeed_recycled_row_has_default_metadata() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        // A soft-wrapped line across rows 0-2 so that row 1 has both wrap
        // flags set. Mark row 1 as a prompt as well (OSC 133 A would).
        for _ in 0..12 {
            t.print('A' as u32).unwrap();
        }
        unsafe {
            (*t.active_mut().pages.get_cell(active_pt(0, 1)).unwrap().row)
                .set_semantic_prompt(pagepkg::SemanticPrompt::Prompt)
        };

        // DECSTBM rows 2-4, cursor to the region bottom, and linefeed:
        // row 1 is discarded and its Row storage recycled as the new
        // blank region-bottom row.
        t.set_top_and_bottom_margin(2, 4);
        t.set_cursor_pos(4, 1);
        t.linefeed().unwrap();

        {
            let rac = t.active_mut().pages.get_cell(active_pt(0, 3)).unwrap();
            unsafe {
                assert!(!(*rac.row).wrap());
                assert!(!(*rac.row).wrap_continuation());
                assert_eq!(pagepkg::SemanticPrompt::None, (*rac.row).semantic_prompt());
            }
        }

        t.deinit(Global);
    }

    // test "Terminal: alt screen scroll up recycled row has default metadata"
    #[test]
    fn terminal_alt_screen_scroll_up_recycled_row_has_default_metadata() {
        let mut t = Terminal::init(Global, opts(5, 3)).unwrap();

        t.switch_screen_mode(SwitchScreenMode::M1049, true).unwrap();

        // A soft-wrapped line across rows 0-1 and a prompt mark on row 0.
        for _ in 0..7 {
            t.print('A' as u32).unwrap();
        }
        unsafe {
            (*t.active_mut().pages.get_cell(active_pt(0, 0)).unwrap().row)
                .set_semantic_prompt(pagepkg::SemanticPrompt::Prompt)
        };

        // Scroll up: with no scrollback, row 0 is discarded and its Row
        // storage recycled as the new blank bottom row.
        t.scroll_up(1).unwrap();

        {
            let rac = t.active_mut().pages.get_cell(active_pt(0, 2)).unwrap();
            unsafe {
                assert!(!(*rac.row).wrap());
                assert!(!(*rac.row).wrap_continuation());
                assert_eq!(pagepkg::SemanticPrompt::None, (*rac.row).semantic_prompt());
            }
        }

        t.deinit(Global);
    }

    // test "Terminal: insertLines count over region blanks row metadata"
    #[test]
    fn terminal_insert_lines_count_over_region_blanks_row_metadata() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        // A soft-wrapped line across rows 0-2 so that row 1 has both wrap
        // flags set, plus a prompt mark on row 1.
        for _ in 0..12 {
            t.print('A' as u32).unwrap();
        }
        unsafe {
            (*t.active_mut().pages.get_cell(active_pt(0, 1)).unwrap().row)
                .set_semantic_prompt(pagepkg::SemanticPrompt::Prompt)
        };

        // Insert more lines than remain in the region: every row from the
        // cursor to the region bottom is blanked in place, with no shifts.
        t.set_cursor_pos(2, 1);
        t.insert_lines(10);

        for y in 1..5u32 {
            let rac = t.active_mut().pages.get_cell(active_pt(0, y)).unwrap();
            unsafe {
                assert!(!(*rac.row).wrap());
                assert!(!(*rac.row).wrap_continuation());
                assert_eq!(pagepkg::SemanticPrompt::None, (*rac.row).semantic_prompt());
            }
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteLines count over region blanks row metadata"
    #[test]
    fn terminal_delete_lines_count_over_region_blanks_row_metadata() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for _ in 0..12 {
            t.print('A' as u32).unwrap();
        }
        unsafe {
            (*t.active_mut().pages.get_cell(active_pt(0, 1)).unwrap().row)
                .set_semantic_prompt(pagepkg::SemanticPrompt::Prompt)
        };

        t.set_cursor_pos(2, 1);
        t.delete_lines(10);

        for y in 1..5u32 {
            let rac = t.active_mut().pages.get_cell(active_pt(0, y)).unwrap();
            unsafe {
                assert!(!(*rac.row).wrap());
                assert!(!(*rac.row).wrap_continuation());
                assert_eq!(pagepkg::SemanticPrompt::None, (*rac.row).semantic_prompt());
            }
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteLines blank row does not retain semantic prompt"
    #[test]
    fn terminal_delete_lines_blank_row_does_not_retain_semantic_prompt() {
        let mut t = Terminal::init(Global, opts(5, 3)).unwrap();

        // Mark row 0 as a prompt row, then delete it. The blank row that
        // appears at the region bottom reuses the deleted row's storage
        // and must not read as a prompt (e.g. for prompt navigation).
        t.print('$' as u32).unwrap();
        unsafe {
            (*t.active_mut().pages.get_cell(active_pt(0, 0)).unwrap().row)
                .set_semantic_prompt(pagepkg::SemanticPrompt::Prompt)
        };

        t.set_cursor_pos(1, 1);
        t.delete_lines(1);

        {
            let rac = t.active_mut().pages.get_cell(active_pt(0, 2)).unwrap();
            unsafe {
                assert!(!(*rac.row).wrap());
                assert!(!(*rac.row).wrap_continuation());
                assert_eq!(pagepkg::SemanticPrompt::None, (*rac.row).semantic_prompt());
            }
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseDisplay complete ignores stale prompt on recycled row"
    #[test]
    fn terminal_erase_display_complete_ignores_stale_prompt_on_recycled_row() {
        let mut t = Terminal::init(Global, opts(10, 3)).unwrap();

        // Screen content that must NOT enter the scrollback on a clear.
        t.print_string(b"hello").unwrap();

        // Mark row 1 as a prompt row and then discard it with a region
        // scroll, recycling its storage as the blank bottom row.
        unsafe {
            (*t.active_mut().pages.get_cell(active_pt(0, 1)).unwrap().row)
                .set_semantic_prompt(pagepkg::SemanticPrompt::Prompt)
        };
        t.set_top_and_bottom_margin(2, 3);
        t.set_cursor_pos(3, 1);
        t.linefeed().unwrap();
        t.set_top_and_bottom_margin(0, 0);

        // ED2: since no prompt is on screen, this must NOT take the
        // scroll-and-clear path that pushes content into scrollback. A
        // stale prompt flag on the recycled blank bottom row would.
        t.erase_display(csi::EraseDisplay::Complete, false);

        assert_eq!(
            t.active_mut().pages.rows as usize,
            t.active_mut().pages.total_rows
        );

        t.deinit(Global);
    }

    // ---------------------------------------------------------------------
    // UD80 (R13): a terminal wide enough to leave the standard page capacity
    // gets `std_capacity` with only `cols` changed (`PageList.zig:535-540`),
    // 215 rows x cols cells, so more than 65,535 cells can share one style on
    // one page and the `u16` style refcount overflows. The port saturates the
    // refcount and pins the style (see `ref_counted_set.rs`), so every cell
    // keeps its style. These tests drive each native route to the ceiling.
    //
    // Width: 50,000 leaves the standard capacity in Ghostty and in the port,
    // in every build measured. The first width that does is, for Ghostty,
    // 47,932 shipped on macOS arm64, 46,908 shipped on Linux aarch64 and
    // 48,047 in a macOS test build (it depends on the platform's page size);
    // the port's are the same, since its style set is laid out with the
    // reference's item geometry (N88). `ud80_scan`
    // asserts the page really is the wide one, so the tests cannot go vacuous
    // if those thresholds move. The page is 215 x 50,000, but only the few
    // rows a test writes are touched. Count: the cursor holds one reference
    // and each styled cell one more, so 65,534 styled cells bring the count
    // to 65,535 and the 65,535th overflows it (measured in the reference);
    // 70,000 (the reference probe's count) passes the ceiling by 4,466.
    // Three screen rows hold 150,000 cells, which DECALN fills in one go.
    const UD80_COLS: size::CellCountInt = 50_000;
    const UD80_ROWS: size::CellCountInt = 3;
    const UD80_PRINTS: usize = 70_000;

    /// Walk the active area in reading order and return, for the cells whose
    /// codepoint is `cp`, how many there are and whether each one carries
    /// `style_id` resolving to a style for which `want` holds. Also returns
    /// that style's refcount on the (single) page.
    fn ud80_scan(
        t: &mut Terminal,
        cp: u21,
        style_id: style::Id,
        want: impl Fn(&style::Style) -> bool,
    ) -> (usize, bool, u16) {
        let rows = t.rows as u32;
        let mut n = 0usize;
        let mut all = true;
        let mut page_seen: *mut Page = core::ptr::null_mut();
        for y in 0..rows {
            let lc = t.active_mut().pages.get_cell(active_pt(0, y)).unwrap();
            let page = unsafe { crate::pagelist::Node::page(lc.node) };
            assert!(page_seen.is_null() || page_seen == page, "one page");
            let cap = unsafe { (*page).capacity };
            assert!(
                cap.rows as usize * cap.cols as usize > usize::from(u16::MAX),
                "the page is the wide, non-standard one"
            );
            page_seen = page;
            let cells = unsafe { &*(*page).get_cells(lc.row) };
            for c in cells {
                if c.content_codepoint() != cp {
                    continue;
                }
                n += 1;
                if c.style_id() != style_id || style_id == style::DEFAULT_ID {
                    all = false;
                    continue;
                }
                let st = unsafe { &*(*page).styles.get((*page).memory_ptr(), c.style_id()) };
                all &= want(st);
            }
        }
        let rc = unsafe {
            (*page_seen)
                .styles
                .ref_count((*page_seen).memory_ptr(), style_id)
        };
        (n, all, rc)
    }

    fn ud80_cursor_style_id(t: &mut Terminal) -> style::Id {
        unsafe { (*t.screens.active).cursor.style_id }
    }

    // UD80 through `printCell` (`Terminal.zig:1722`, the `Terminal.print`
    // API route): one style, 70,000 single prints, checked at the exact
    // ceiling on the way.
    #[test]
    fn ud80_wide_terminal_one_style_past_the_refcount_ceiling_keeps_every_cell_styled() {
        let mut t = Terminal::init(Global, opts(UD80_COLS, UD80_ROWS)).unwrap();
        t.set_attribute(sgr::Attribute::Bold).unwrap();
        for _ in 0..65_534 {
            t.print('A' as u32).unwrap();
        }
        let id = ud80_cursor_style_id(&mut t);
        let (_, _, rc) = ud80_scan(&mut t, 'A' as u21, id, |s| s.flags.bold);
        assert_eq!(
            rc,
            u16::MAX,
            "65,534 cells plus the cursor: the exact ceiling"
        );
        // The 65,535th styled cell is the one the reference overflows on.
        t.print('A' as u32).unwrap();
        for _ in 65_535..UD80_PRINTS {
            t.print('A' as u32).unwrap();
        }
        let (n, all, rc) = ud80_scan(&mut t, 'A' as u21, id, |s| s.flags.bold);
        assert_eq!(n, UD80_PRINTS, "every print landed");
        assert!(all, "every printed cell keeps the bold style");
        assert_eq!(rc, u16::MAX, "the refcount saturated and pinned");

        // Releasing references on a pinned style (overwrite, then erase) does
        // not underflow the count or free the style while cells use it.
        t.set_cursor_pos(1, 1);
        t.set_attribute(sgr::Attribute::Unset).unwrap();
        for _ in 0..10 {
            t.print('B' as u32).unwrap();
        }
        let (n, all, rc) = ud80_scan(&mut t, 'A' as u21, id, |s| s.flags.bold);
        assert_eq!(n, UD80_PRINTS - 10);
        assert!(all, "the remaining cells keep the bold style");
        assert_eq!(rc, u16::MAX, "a pinned style stays pinned on release");
        t.erase_display(csi::EraseDisplay::Complete, false);
        let (n, _, rc) = ud80_scan(&mut t, 'A' as u21, id, |s| s.flags.bold);
        assert_eq!(n, 0);
        assert_eq!(rc, u16::MAX);

        // The pinned style is reused by the next add of the same value.
        t.set_attribute(sgr::Attribute::Bold).unwrap();
        assert_eq!(ud80_cursor_style_id(&mut t), id);
        t.print('C' as u32).unwrap();
        let (n, all, _) = ud80_scan(&mut t, 'C' as u21, id, |s| s.flags.bold);
        assert_eq!((n, all), (1, true));

        t.deinit(Global);
    }

    // UD80 through the batched `print_slice` path (`printSliceFill`'s
    // `useMultiple`, `Terminal.zig:1114`).
    #[test]
    fn ud80_wide_terminal_print_slice_past_the_refcount_ceiling_keeps_every_cell_styled() {
        let mut t = Terminal::init(Global, opts(UD80_COLS, UD80_ROWS)).unwrap();
        t.set_attribute(sgr::Attribute::Bold).unwrap();
        let chunk = vec!['A' as u32; 4096];
        let mut left = UD80_PRINTS;
        while left > 0 {
            let k = left.min(chunk.len());
            t.print_slice(&chunk[..k]).unwrap();
            left -= k;
        }
        let id = ud80_cursor_style_id(&mut t);
        let (n, all, rc) = ud80_scan(&mut t, 'A' as u21, id, |s| s.flags.bold);
        assert_eq!(n, UD80_PRINTS);
        assert!(all, "every printed cell keeps the bold style");
        assert_eq!(rc, u16::MAX);
        t.deinit(Global);
    }

    // UD80 through DECALN (`use_multiple(cols)` per row): three full rows.
    #[test]
    fn ud80_wide_terminal_decaln_past_the_refcount_ceiling_keeps_every_cell_styled() {
        let mut t = Terminal::init(Global, opts(UD80_COLS, UD80_ROWS)).unwrap();
        let red = color::RGB {
            r: 0xFF,
            g: 0,
            b: 0,
        };
        t.set_attribute(sgr::Attribute::DirectColorBg(red)).unwrap();
        t.decaln().unwrap();
        let id = ud80_cursor_style_id(&mut t);
        let (n, all, rc) = ud80_scan(&mut t, 'E' as u21, id, |s| {
            s.bg_color.tag() == style::ColorTag::Rgb && s.bg_color.rgb() == red
        });
        assert_eq!(n, UD80_COLS as usize * UD80_ROWS as usize);
        assert!(all, "every DECALN cell keeps the background");
        assert_eq!(rc, u16::MAX);
        t.deinit(Global);
    }

    // UD80 through reflow: a resize re-wraps the saturated rows into new
    // pages (`use_multiple` / `add_with_id` in the clone chain).
    #[test]
    fn ud80_wide_terminal_reflow_past_the_refcount_ceiling_keeps_every_cell_styled() {
        let mut t = Terminal::init(Global, opts(UD80_COLS, UD80_ROWS)).unwrap();
        t.set_attribute(sgr::Attribute::Bold).unwrap();
        for _ in 0..UD80_PRINTS {
            t.print('A' as u32).unwrap();
        }
        t.resize(
            Global,
            Resize {
                cols: UD80_COLS + 49,
                rows: UD80_ROWS,
                cell_size_px: None,
            },
        )
        .unwrap();
        let id = ud80_cursor_style_id(&mut t);
        let (n, all, rc) = ud80_scan(&mut t, 'A' as u21, id, |s| s.flags.bold);
        assert_eq!(n, UD80_PRINTS);
        assert!(all, "every reflowed cell keeps the bold style");
        assert_eq!(rc, u16::MAX);
        t.deinit(Global);
    }

    // UD80 through a page clone: distinct styles past the page's style
    // capacity make `manualStyleUpdate` grow it (`increaseCapacity`), which
    // clones every row, the saturated style included, into a new page
    // (`add_with_id` / `use_multiple` in the clone chain). 110,000 prints fill
    // two rows and start the third, so the clone's per-row `add_with_id` for
    // the third row's first cell lands on an already-saturated style.
    #[test]
    fn ud80_wide_terminal_capacity_clone_past_the_refcount_ceiling_keeps_every_cell_styled() {
        const PRINTS: usize = 110_000;
        let mut t = Terminal::init(Global, opts(UD80_COLS, UD80_ROWS)).unwrap();
        t.set_attribute(sgr::Attribute::Bold).unwrap();
        for _ in 0..PRINTS {
            t.print('A' as u32).unwrap();
        }
        let page_cap = |t: &mut Terminal| unsafe {
            let node = (*(*t.screens.active).cursor.page_pin).node;
            (*crate::pagelist::Node::page(node)).capacity.styles
        };
        let styles_before = page_cap(&mut t);
        for i in 0..300u32 {
            let fg = color::RGB {
                r: (i & 0xFF) as u8,
                g: (i >> 8) as u8,
                b: 1,
            };
            t.set_attribute(sgr::Attribute::DirectColorFg(fg)).unwrap();
            t.print('D' as u32).unwrap();
        }
        assert!(
            page_cap(&mut t) > styles_before,
            "the page was grown (cloned)"
        );
        t.set_attribute(sgr::Attribute::Unset).unwrap();
        t.set_attribute(sgr::Attribute::Bold).unwrap();
        let id = ud80_cursor_style_id(&mut t);
        let (n, all, rc) = ud80_scan(&mut t, 'A' as u21, id, |s| s.flags.bold);
        assert_eq!(n, PRINTS);
        assert!(all, "every cloned cell keeps the bold style");
        assert_eq!(rc, u16::MAX);
        t.deinit(Global);
    }

    // UD80 through the VT stream, the route an embedder's `vt_write` takes:
    // SGR 1, then 70,000 printable bytes, as ordinary output. The stream hands
    // runs of printables to `print_slice`, so this is the reference's
    // `stream_terminal.zig:362` -> `printSliceFast` -> `printSliceFill` ->
    // `useMultiple` route (`Terminal.zig:1114`, `ref_counted_set.zig:397`).
    #[test]
    fn ud80_wide_terminal_vt_stream_past_the_refcount_ceiling_keeps_every_cell_styled() {
        let mut t = Terminal::init(Global, opts(UD80_COLS, UD80_ROWS)).unwrap();
        {
            let mut stream = t.vt_stream();
            stream.next_slice(b"\x1b[1m");
            let chunk = [b'A'; 1000];
            for _ in 0..UD80_PRINTS / chunk.len() {
                stream.next_slice(&chunk);
            }
            stream.deinit();
        }
        let id = ud80_cursor_style_id(&mut t);
        let (n, all, rc) = ud80_scan(&mut t, 'A' as u21, id, |s| s.flags.bold);
        assert_eq!(n, UD80_PRINTS);
        assert!(all, "every printed cell keeps the bold style");
        assert_eq!(rc, u16::MAX);
        t.deinit(Global);
    }

    // ---------------------------------------------------------------------
    // UD80 (R13), the style-capacity ceiling. A pinned style stays counted
    // after its cells are overwritten (an "orphan"). Page clones recount from
    // the cells and drop orphans, but the capacity projection counts them, so
    // the style capacity can climb to `maxInt(u16)`; there growth returns
    // `OutOfSpace`. The port re-clones the page once at the same capacity
    // before the reference's split (`manualStyleUpdate`) or crash (the
    // row-copy retry loops), when the set holds a pinned style.
    //
    // The review-R13W probe reached the ceiling organically on a 50,000 x 3
    // terminal (a new RGB background plus DECALN per iteration) after 56,675
    // iterations, about 8.5 billion cell writes. These tests reach the same
    // state in a few iterations: they grow the style capacity straight to the
    // ceiling with the reference test's own loop ("restoreCursor uses default
    // style on OutOfSpace"), which skips the two projection jumps
    // (128 -> 4,096 -> 65,535), and then fill the rest of the set with pinned
    // styles no cell uses. Such a style is exactly what the probe accumulates:
    // a count at 65,535 and no cell left.

    /// Grow the style capacity of the page `node` returns to the `u16`
    /// ceiling, the way the reference test does, and return that page.
    fn ud80_presize_styles(
        t: &mut Terminal,
        node: impl Fn(&mut Terminal) -> *mut crate::pagelist::Node,
    ) -> *mut Page {
        loop {
            let n = node(t);
            if unsafe { crate::pagelist::Node::capacity(n).styles } == size::CellCountInt::MAX {
                return unsafe { crate::pagelist::Node::page(n) };
            }
            t.active_mut()
                .increase_capacity(n, Some(pagelist::grow::IncreaseCapacity::Styles))
                .unwrap();
        }
    }

    /// Fill `page`'s style set with pinned styles that no cell uses until
    /// `add` fails. Each gets one reference from `add` and 65,534 more, which
    /// pins it at 65,535. `tag` keeps the values distinct from the styles a
    /// test sets itself. Once `add` has failed with `OutOfMemory` (the ids are
    /// used up, or a probe sequence reached the last `psl_stats` bucket),
    /// every later `add` of a new value fails the same way, which is the
    /// state the probe reached.
    ///
    /// NOTE (R13W2 review): this fabricated fill stops when it trips the
    /// last `psl_stats` bucket, which its structured RGB values reach about
    /// 189 ids BEFORE the ids are actually exhausted — not on true id
    /// exhaustion as the organic probe does. Both surface the same
    /// `AddError::OutOfMemory`, and the item states are the same kind (all
    /// pinned orphans, no cells), so the mitigation exercises the same path;
    /// only the failure trigger differs by those ~189 ids.
    fn ud80_fill_with_orphans(page: *mut Page, tag: u8) -> usize {
        let mut n = 0u32;
        unsafe {
            (*page).pause_integrity_checks(true);
            loop {
                let base = (*page).memory_ptr();
                let st = style::Style {
                    fg_color: style::Color::Rgb(color::RGB {
                        r: (n & 0xFF) as u8,
                        g: (n >> 8) as u8,
                        b: tag,
                    }),
                    ..Default::default()
                };
                let id = match (*page).styles.add(base, st) {
                    Ok(id) => id,
                    Err(e) => {
                        assert_eq!(e, crate::ref_counted_set::AddError::OutOfMemory);
                        break;
                    }
                };
                (*page).styles.use_multiple(base, id, u16::MAX - 1);
                assert!((*page).styles.ref_count_saturated(base, id));
                n += 1;
            }
            (*page).pause_integrity_checks(false);
            (*page).assert_integrity();
        }
        n as usize
    }

    fn ud80_cursor_page(t: &mut Terminal) -> *mut Page {
        unsafe { crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node) }
    }

    // The review-R13W probe as a test: a new RGB background plus DECALN per
    // iteration, past the iteration at which the unmitigated code starts
    // failing every SGR. The first `REAL` iterations run at the ceiling, each
    // DECALN pinning its new style and orphaning the previous one; then the
    // rest of the set is filled with orphans, so iteration `REAL` finds the
    // set full. Without the re-clone the page is split, the cursor (homed by
    // DECALN) stays on the full half, and that SGR and every later one fail
    // (`set_attribute` keeps the previous style).
    #[test]
    fn ud80_wide_terminal_orphaned_pinned_styles_at_the_capacity_ceiling_are_reclaimed_not_split() {
        const REAL: u32 = 4;
        const PAST: u32 = 6;
        let mut t = Terminal::init(Global, opts(UD80_COLS, UD80_ROWS)).unwrap();
        let page = ud80_presize_styles(&mut t, |t| unsafe {
            (*(*t.screens.active).cursor.page_pin).node
        });

        let mut reclaimed_at = None;
        for i in 0..REAL + PAST {
            if i == REAL {
                assert_eq!(ud80_cursor_page(&mut t), page, "not cloned yet");
                let orphans = ud80_fill_with_orphans(page, 0x55);
                assert!(orphans > 50_000, "the set is full of orphans: {orphans}");
            }
            let bg = color::RGB {
                r: i as u8,
                g: 0x11,
                b: 0xAA,
            };
            let before = unsafe { (*ud80_cursor_page(&mut t)).styles.count() };
            t.set_attribute(sgr::Attribute::DirectColorBg(bg))
                .unwrap_or_else(|e| panic!("iteration {i}: SGR failed: {e:?}"));
            let cursor_bg = unsafe { (*t.screens.active).cursor.style.bg_color };
            assert!(
                cursor_bg.tag() == style::ColorTag::Rgb && cursor_bg.rgb() == bg,
                "iteration {i}: the cursor takes the new style"
            );
            let id = ud80_cursor_style_id(&mut t);
            assert_ne!(id, style::DEFAULT_ID, "iteration {i}");
            t.decaln().unwrap();

            let page = ud80_cursor_page(&mut t);
            let after = unsafe { (*page).styles.count() };
            if after < before {
                reclaimed_at.get_or_insert(i);
            }
            assert_eq!(
                t.active_mut().pages.total_pages(),
                1,
                "iteration {i}: no split"
            );
            assert_eq!(
                unsafe { (*page).capacity.styles },
                size::CellCountInt::MAX,
                "iteration {i}: still at the ceiling"
            );
            let (n, all, rc) = ud80_scan(&mut t, 'E' as u21, id, |s| {
                s.bg_color.tag() == style::ColorTag::Rgb && s.bg_color.rgb() == bg
            });
            assert_eq!(n, UD80_COLS as usize * UD80_ROWS as usize);
            assert!(
                all,
                "iteration {i}: every DECALN cell takes the new background"
            );
            assert_eq!(rc, u16::MAX, "iteration {i}");
        }
        // The re-clone ran exactly where the unmitigated code split, and
        // dropped every orphan: only the last DECALN style is left.
        assert_eq!(reclaimed_at, Some(REAL));
        assert!(unsafe { (*ud80_cursor_page(&mut t)).styles.count() } < 10);
        t.deinit(Global);
    }

    // The no-reclaim case: the set is full at the ceiling of styles that are
    // genuinely in use (one pinned by 70,000 bold cells, the rest one cell
    // each), so the re-clone reclaims nothing and the reference's split still
    // happens, once per SGR, and returns.
    #[test]
    fn ud80_wide_terminal_live_styles_at_the_capacity_ceiling_still_split() {
        let mut t = Terminal::init(Global, opts(UD80_COLS, UD80_ROWS)).unwrap();
        t.set_attribute(sgr::Attribute::Bold).unwrap();
        for _ in 0..UD80_PRINTS {
            t.print('A' as u32).unwrap();
        }
        let bold = ud80_cursor_style_id(&mut t);
        let page = ud80_presize_styles(&mut t, |t| unsafe {
            (*(*t.screens.active).cursor.page_pin).node
        });

        // One live style per cell after the cursor, until the set is full.
        let mut live = 0u32;
        unsafe {
            (*page).pause_integrity_checks(true);
            let base = (*page).memory_ptr();
            'fill: for y in 1..UD80_ROWS as usize {
                let row = (*page).get_row(y);
                let cells = &mut *(*page).get_cells(row);
                let start = if y == 1 {
                    UD80_PRINTS - UD80_COLS as usize + 1
                } else {
                    0
                };
                for c in &mut cells[start..] {
                    let st = style::Style {
                        fg_color: style::Color::Rgb(color::RGB {
                            r: (live & 0xFF) as u8,
                            g: (live >> 8) as u8,
                            b: 0x77,
                        }),
                        ..Default::default()
                    };
                    let Ok(id) = (*page).styles.add(base, st) else {
                        break 'fill;
                    };
                    c.set_content_codepoint('L' as u21);
                    c.set_style_id(id);
                    (*row).set_styled(true);
                    live += 1;
                }
            }
            (*page).pause_integrity_checks(false);
            (*page).assert_integrity();
            assert!((*page).styles.ref_count_saturated(base, bold));
        }
        assert!(live > 50_000, "the set is full of live styles: {live}");

        let count_l = |t: &mut Terminal| {
            let mut n = 0usize;
            for y in 0..UD80_ROWS as u32 {
                let lc = t.active_mut().pages.get_cell(active_pt(0, y)).unwrap();
                let page = unsafe { crate::pagelist::Node::page(lc.node) };
                for c in unsafe { &*(*page).get_cells(lc.row) } {
                    if c.content_codepoint() == 'L' as u21 {
                        let st =
                            unsafe { &*(*page).styles.get((*page).memory_ptr(), c.style_id()) };
                        assert_eq!(st.fg_color.rgb().b, 0x77, "a live style survived");
                        n += 1;
                    }
                }
            }
            n
        };

        // The first new style: the re-clone reclaims nothing, so the page is
        // split, as in the reference.
        assert_eq!(t.active_mut().pages.total_pages(), 1);
        let red = color::RGB {
            r: 0xFF,
            g: 0,
            b: 0,
        };
        let r = t.set_attribute(sgr::Attribute::DirectColorBg(red));
        assert_eq!(t.active_mut().pages.total_pages(), 2, "the page was split");
        assert_eq!(count_l(&mut t), live as usize, "no live style was dropped");
        // The split moves the cursor's row, with row 2, to the new page. Both
        // rows' live styles go with it, so that page's set is full too and the
        // add fails: the reference's own "we tried" outcome, which keeps the
        // previous style (bold, no background).
        assert!(r.is_err());
        let cursor = unsafe { &(*t.screens.active).cursor };
        assert!(cursor.style.flags.bold);
        assert_eq!(cursor.style.bg_color.tag(), style::ColorTag::None);

        // And again: the call returns, one attempt per SGR, no loop.
        let blue = color::RGB {
            r: 0,
            g: 0,
            b: 0xFF,
        };
        let r = t.set_attribute(sgr::Attribute::DirectColorBg(blue));
        assert!(r.is_err());
        assert_eq!(count_l(&mut t), live as usize);
        t.deinit(Global);
    }

    // The row-copy retry loop (`Screen.clonePartialRowGrowCapacity`, reached
    // by DL across a page boundary): the destination page's style set is full
    // of orphaned pinned styles at the ceiling, and the row pulled into it
    // carries a style it does not have. The reference's loop panics
    // ("increaseCapacity OutOfSpace") here; the port re-clones once, which
    // drops the orphans, and the copy succeeds.
    #[test]
    fn ud80_delete_lines_into_a_page_full_of_orphaned_pinned_styles_reclaims_them() {
        let mut t = Terminal::init(Global, opts(4000, 5)).unwrap();
        let first_page_rows =
            unsafe { crate::pagelist::Node::capacity(t.active_mut().pages.pages.first).rows };
        for _ in 0..(first_page_rows + 1) {
            t.linefeed().unwrap();
        }
        assert_eq!(2, unsafe {
            crate::pagelist::Node::rows(t.active_mut().pages.pages.last)
        });
        // The first row of the second page (active y = 3) carries a style.
        t.set_cursor_pos(4, 1);
        t.set_attribute(sgr::Attribute::Bold).unwrap();
        t.print_string(b"S").unwrap();
        t.set_attribute(sgr::Attribute::Unset).unwrap();
        t.set_cursor_pos(1, 1);

        let page = ud80_presize_styles(&mut t, |t| t.active_mut().pages.pages.first);
        assert!(ud80_fill_with_orphans(page, 0x33) > 50_000);

        t.delete_lines(1);
        let lc = t.active_mut().pages.get_cell(active_pt(0, 2)).unwrap();
        let page = unsafe { crate::pagelist::Node::page(lc.node) };
        assert!(std::ptr::eq(lc.node, t.active_mut().pages.pages.first));
        assert_eq!(unsafe { (*lc.cell).content_codepoint() }, 'S' as u21);
        let st = unsafe {
            &*(*page)
                .styles
                .get((*page).memory_ptr(), (*lc.cell).style_id())
        };
        assert!(st.flags.bold, "the copied cell keeps its style");
        assert!(
            unsafe { (*page).styles.count() } < 10,
            "the orphans were dropped"
        );
        // The re-clone is at the SAME capacity: the ceiling is unchanged
        // (a smaller-capacity re-clone would still drop the orphans but is a
        // divergence the reclaim asserts alone do not catch; R13W2 review).
        assert_eq!(
            unsafe { (*page).capacity.styles },
            size::CellCountInt::MAX,
            "the style capacity stays at the ceiling"
        );
        t.deinit(Global);
    }

    // UD131 (R13) through REAL VT input, one test per route. RELEASE ONLY:
    //
    //     cargo test -p rustty-vt --release --lib -- --ignored ud131_vt_release
    //
    // Each case takes well under a second in release; in a debug build the
    // per-write page integrity check walks the whole 50,000 x 215 page and a
    // case does not finish in ten minutes, which is why the unit tests of the
    // two loops fabricate the full set instead. In a debug build these tests
    // return at once.
    //
    // The recipe is the reference probe's (UPSTREAM-DEFECTS.md UD131): a
    // 50,000-column terminal, `fills` cells each with a distinct SGR 38;2
    // colour (53,247 fill the page's style set with live styles at the `u16`
    // ceiling), one cell 'Y' with a new style (background 1;2;3) on a row of
    // another page, then the trigger that copies that row onto the full page.
    // The reference `@panic`s at `Screen.zig:747` for DL and IL and at
    // `PageList.zig:5108` for the alternate-screen LF and SU and the
    // primary-screen scroll region (task R13-X's probe, re-measured by the
    // R13-X review). The port keeps 'Y', drops its unstorable style to the
    // default, and logs the degrade once.
    //
    // NEGATIVE CONTROLS (task R13-X2, macOS 2026-09-23, release): restoring
    // the reference's `@panic("increaseCapacity OutOfSpace")` in the Screen
    // loop's `OutOfSpace` arm fails exactly the DL and IL tests; restoring it
    // in the PageList loop's arm fails exactly the alternate-screen LF, SU and
    // scroll-region tests.
    struct Ud131VtCase {
        rows: size::CellCountInt,
        fills: u32,
        pre: &'static [u8],
        fill_row: u32,
        y_row: u32,
        trigger: &'static [u8],
    }

    /// Runs one case; returns 'Y''s background (`None` when it degraded to
    /// the default style) and the UD131 log records.
    fn ud131_vt_run(c: &Ud131VtCase) -> (Option<String>, Vec<String>) {
        let mut t = Terminal::init(Global, opts(50_000, c.rows)).unwrap();
        let cap = crate::log_capture::start();
        {
            let mut st = t.vt_stream();
            st.next_slice(c.pre);
            st.next_slice(format!("\x1b[{};1H", c.fill_row).as_bytes());
            let mut buf = Vec::with_capacity(1 << 20);
            for i in 0..c.fills {
                // Distinct 24-bit colours (an odd multiplier is a bijection).
                let v = i.wrapping_add(1).wrapping_mul(2_654_435_761) & 0xFF_FFFF;
                buf.extend_from_slice(
                    format!("\x1b[38;2;{};{};{}mX", v >> 16, (v >> 8) & 255, v & 255).as_bytes(),
                );
            }
            buf.extend_from_slice(b"\x1b[0m");
            st.next_slice(&buf);
            st.next_slice(format!("\x1b[{};1H\x1b[48;2;1;2;3mY\x1b[0m", c.y_row).as_bytes());
            st.next_slice(c.trigger);
            st.deinit();
        }
        let logs: Vec<String> = cap
            .messages()
            .into_iter()
            .filter(|(_, _, m)| m.contains("UD131"))
            .map(|(_, _, m)| m)
            .collect();
        drop(cap);

        let mut y_bg = None;
        let mut found = false;
        for y in 0..c.rows as u32 {
            let lc = t.active_mut().pages.get_cell(active_pt(0, y)).unwrap();
            if unsafe { (*lc.cell).content_codepoint() } != 'Y' as u21 {
                continue;
            }
            found = true;
            let page = unsafe { crate::pagelist::Node::page(lc.node) };
            let sid = unsafe { (*lc.cell).style_id() };
            if sid != crate::style::DEFAULT_ID {
                let s = unsafe { *(*page).styles.get((*page).memory_ptr(), sid) };
                y_bg = Some(format!("{:?}", s.bg_color));
            }
            break;
        }
        assert!(found, "the 'Y' cell survives the copy");

        // `assert_integrity` is a no-op without `debug_assertions`, and these
        // tests run in RELEASE, so call `verify_integrity` (whose body runs in
        // every build) directly and assert it on each page (R13-X3, review N1).
        let mut node = t.active_mut().pages.pages.first;
        while !node.is_null() {
            let page = unsafe { &*crate::pagelist::Node::page(node) };
            assert_eq!(page.verify_integrity(), Ok(()), "page integrity holds");
            node = unsafe { (*node).next };
        }
        t.deinit(Global);
        (y_bg, logs)
    }

    fn ud131_vt_expect_degraded(c: Ud131VtCase) {
        // R14: these tests are #[ignore]d; asked for in a debug build they FAIL
        // with how to run them instead of returning `ok` having run nothing.
        assert!(
            !cfg!(debug_assertions),
            "ud131_vt_release: release only (a debug build does not finish); run \
             cargo test -p rustty-vt --release --lib -- --ignored ud131_vt_release"
        );
        let (bg, logs) = ud131_vt_run(&c);
        assert_eq!(bg, None, "'Y''s new style degrades to the default");
        assert_eq!(
            logs,
            vec![
                "row copy at a page capacity ceiling: 1 cell part(s) degraded to default \
                 (styles=1, hyperlinks=0, graphemes=0); see UD131"
                    .to_string()
            ],
            "one degraded copy, logged once"
        );
    }

    const UD131_FULL: u32 = 53_247;

    // DL at the bottom row: `deleteLines` -> the Screen loop.
    #[test]
    #[ignore = "release only: cargo test -p rustty-vt --release --lib -- --ignored ud131_vt_release"]
    fn ud131_vt_release_delete_lines_degrades_at_the_screen_site() {
        ud131_vt_expect_degraded(Ud131VtCase {
            rows: 216,
            fills: UD131_FULL,
            pre: b"",
            fill_row: 2,
            y_row: 216,
            trigger: b"\x1b[215;1H\x1b[M",
        });
    }

    // IL: `insertLines` -> the Screen loop.
    #[test]
    #[ignore = "release only: cargo test -p rustty-vt --release --lib -- --ignored ud131_vt_release"]
    fn ud131_vt_release_insert_lines_degrades_at_the_screen_site() {
        ud131_vt_expect_degraded(Ud131VtCase {
            rows: 430,
            fills: UD131_FULL,
            pre: b"",
            fill_row: 217,
            y_row: 215,
            trigger: b"\x1b[215;1H\x1b[L",
        });
    }

    // Alternate-screen LF at the bottom: `index` -> `eraseRowBounded` -> the
    // PageList loop.
    #[test]
    #[ignore = "release only: cargo test -p rustty-vt --release --lib -- --ignored ud131_vt_release"]
    fn ud131_vt_release_alt_screen_linefeed_degrades_at_the_pagelist_site() {
        ud131_vt_expect_degraded(Ud131VtCase {
            rows: 216,
            fills: UD131_FULL,
            pre: b"\x1b[?1049h",
            fill_row: 2,
            y_row: 216,
            trigger: b"\x1b[216;1H\n",
        });
    }

    // Alternate-screen SU: `scrollUp` -> `eraseRow` -> the PageList loop.
    #[test]
    #[ignore = "release only: cargo test -p rustty-vt --release --lib -- --ignored ud131_vt_release"]
    fn ud131_vt_release_alt_screen_scroll_up_degrades_at_the_pagelist_site() {
        ud131_vt_expect_degraded(Ud131VtCase {
            rows: 216,
            fills: UD131_FULL,
            pre: b"\x1b[?1049h",
            fill_row: 2,
            y_row: 216,
            trigger: b"\x1b[S",
        });
    }

    // Primary-screen scroll region with a top margin, LF at its bottom:
    // `index` -> `eraseRowBounded` -> the PageList loop.
    #[test]
    #[ignore = "release only: cargo test -p rustty-vt --release --lib -- --ignored ud131_vt_release"]
    fn ud131_vt_release_scroll_region_linefeed_degrades_at_the_pagelist_site() {
        ud131_vt_expect_degraded(Ud131VtCase {
            rows: 216,
            fills: UD131_FULL,
            pre: b"",
            fill_row: 3,
            y_row: 216,
            trigger: b"\x1b[2;216r\x1b[216;1H\n",
        });
    }

    // Boundary control: one fill fewer (53,246) leaves room for 'Y''s style,
    // so the same DL keeps it and logs nothing (the reference survives this
    // too).
    #[test]
    #[ignore = "release only: cargo test -p rustty-vt --release --lib -- --ignored ud131_vt_release"]
    fn ud131_vt_release_one_style_below_the_ceiling_keeps_the_style() {
        // R14: these tests are #[ignore]d; asked for in a debug build they FAIL
        // with how to run them instead of returning `ok` having run nothing.
        assert!(
            !cfg!(debug_assertions),
            "ud131_vt_release: release only (a debug build does not finish); run \
             cargo test -p rustty-vt --release --lib -- --ignored ud131_vt_release"
        );
        let (bg, logs) = ud131_vt_run(&Ud131VtCase {
            rows: 216,
            fills: UD131_FULL - 1,
            pre: b"",
            fill_row: 2,
            y_row: 216,
            trigger: b"\x1b[215;1H\x1b[M",
        });
        assert!(
            bg.as_deref()
                .is_some_and(|s| s.contains('1') && s.contains('2') && s.contains('3')),
            "'Y' keeps its 1;2;3 background: {bg:?}"
        );
        assert!(logs.is_empty(), "no degrade: {logs:?}");
    }

    // UD131 (R13) on an ORDINARY 80x24 terminal, in the DEFAULT debug suite.
    //
    // Unlike the wide-terminal `ud131_vt_release_*` tests, this route does NOT
    // need a page to leave the standard capacity. It drives the ref-counted
    // style set into its OTHER out-of-memory arm: the probe-sequence-length
    // overflow (`ref_counted_set.zig:268`, "the hash function is not perfect ...
    // we claim to be out of memory"), which fires as soon as one item sits 31
    // slots from its home bucket. That takes only 32 live styles whose unseeded,
    // deterministic style hashes (`style.zig:664`, `std.hash.int(wide[0] ^
    // wide[1])`) share their low 16 bits. The 32 colours below all hash to low
    // bits 0x2e2a, so they stay clustered at every capacity doubling and
    // `increaseCapacity(.styles)` climbs 128 -> ... -> 65535 and then returns
    // OutOfSpace, on a page holding only ~32 live styles. About 1.2 KB of
    // ordinary output (591 line feeds fill page 1). The style hash is identical in the port, so the same
    // colours cluster here (MEASURED, task R13-X3).
    //
    // Reference (MEASURED, task R13-X3, scratch ReleaseSafe/ReleaseFast builds at
    // 20abdb50a): 80x24, these 32 colours, then `Y` with a new style on the first
    // row of the next page, then DL across the page boundary -> @panic
    // "increaseCapacity OutOfSpace" at Screen.zig:747 <- deleteLines
    // (Terminal.zig:3215). ReleaseSafe SIGABRT (44 of 29,923 sweep runs die),
    // ReleaseFast SIGTRAP (8 of 253). 31 colours never trigger; no DL never
    // triggers.
    //
    // NEGATIVE CONTROL (task R13-X3): restoring the reference's `@panic` in the
    // Screen loop's `OutOfSpace` arm (`clone_partial_row_grow_capacity`, at the
    // `IncreaseCapacityError::OutOfSpace` match arm) makes
    // `ud131_delete_lines_degrades_on_an_ordinary_80_column_terminal` fail.
    #[rustfmt::skip]
    const UD131_PSL_COLOURS: [(u8, u8, u8); 32] = [
        (0, 3, 183), (0, 36, 22), (0, 212, 121), (1, 39, 95), (1, 89, 203),
        (1, 98, 209), (1, 196, 183), (3, 10, 143), (3, 27, 63), (3, 136, 88),
        (3, 177, 19), (4, 4, 80), (4, 38, 52), (5, 170, 136), (6, 82, 67),
        (6, 211, 183), (6, 221, 6), (6, 247, 172), (7, 62, 169), (7, 87, 247),
        (7, 177, 15), (8, 49, 218), (8, 179, 161), (9, 140, 163), (9, 205, 72),
        (10, 83, 2), (10, 93, 46), (10, 248, 1), (11, 112, 126), (11, 210, 50),
        (13, 123, 81), (14, 56, 135),
    ];

    struct Ud131PslOutcome {
        y_degraded: bool,
        y_found: bool,
        first_colliding_kept: bool,
        logs: Vec<String>,
    }

    /// Drives the PSL-overflow recipe on an 80x24 terminal with the first
    /// `n_colliding` colours, then DL across a page boundary. Walks every page's
    /// `assert_integrity` (real in a debug build).
    fn ud131_psl_80col_run(n_colliding: usize) -> Ud131PslOutcome {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        let cap = crate::log_capture::start();

        // Scroll until the active area straddles a page boundary.
        {
            let mut st = t.vt_stream();
            st.next_slice(b"\x1b[24;1H");
            st.deinit();
        }
        let mut boundary = None;
        for _ in 0..4000 {
            {
                let mut st = t.vt_stream();
                st.next_slice(b"\n");
                st.deinit();
            }
            let n0 = t.active_mut().pages.get_cell(active_pt(0, 0)).unwrap().node;
            for y in 1..24u32 {
                if t.active_mut().pages.get_cell(active_pt(0, y)).unwrap().node != n0 {
                    if (4..=20).contains(&y) {
                        boundary = Some(y);
                    }
                    break;
                }
            }
            if boundary.is_some() {
                break;
            }
        }
        let b = boundary.expect("the active area straddles a page boundary");

        {
            let mut st = t.vt_stream();
            let mut buf = Vec::new();
            // The colliding styles on active row 1 (page 1, the DL destination).
            buf.extend_from_slice(b"\x1b[1;1H");
            for &(r, g, bb) in &UD131_PSL_COLOURS[..n_colliding] {
                buf.extend_from_slice(format!("\x1b[38;2;{r};{g};{bb}mX").as_bytes());
            }
            buf.extend_from_slice(b"\x1b[0m");
            // 'Y' with a NEW style on the first row of page 2.
            buf.extend_from_slice(format!("\x1b[{};1H\x1b[48;2;1;2;3mY\x1b[0m", b + 1).as_bytes());
            // DL at row 2 pulls page 2's first row (Y) into the full page 1.
            buf.extend_from_slice(b"\x1b[2;1H\x1b[M");
            st.next_slice(&buf);
            st.deinit();
        }

        let logs: Vec<String> = cap
            .messages()
            .into_iter()
            .filter(|(_, _, m)| m.contains("UD131"))
            .map(|(_, _, m)| m)
            .collect();
        drop(cap);

        // 'Y' survives the copy; its new style degraded to the default.
        let mut y_found = false;
        let mut y_degraded = false;
        for y in 0..24u32 {
            let lc = t.active_mut().pages.get_cell(active_pt(0, y)).unwrap();
            if unsafe { (*lc.cell).content_codepoint() } != 'Y' as u21 {
                continue;
            }
            y_found = true;
            y_degraded = unsafe { (*lc.cell).style_id() } == crate::style::DEFAULT_ID;
            break;
        }

        // The first colliding cell (active row 1, untouched by DL at row 2) keeps
        // its non-default style: only the UNSTORABLE cell degrades.
        let first = t.active_mut().pages.get_cell(active_pt(0, 0)).unwrap();
        let first_colliding_kept = unsafe { (*first.cell).style_id() } != crate::style::DEFAULT_ID;

        // Integrity holds across every page (real check in a debug build).
        let mut node = t.active_mut().pages.pages.first;
        while !node.is_null() {
            unsafe { (*crate::pagelist::Node::page(node)).assert_integrity() };
            node = unsafe { (*node).next };
        }
        // And the whole page list.
        t.active_mut().pages.assert_integrity();

        t.deinit(Global);
        Ud131PslOutcome {
            y_degraded,
            y_found,
            first_colliding_kept,
            logs,
        }
    }

    #[test]
    fn ud131_delete_lines_degrades_on_an_ordinary_80_column_terminal() {
        let o = ud131_psl_80col_run(32);
        assert!(o.y_found, "the 'Y' cell survives the copy");
        assert!(o.y_degraded, "'Y''s new style degrades to the default");
        assert!(
            o.first_colliding_kept,
            "the storable colliding cell keeps its style; only the unstorable degrades"
        );
        assert_eq!(
            o.logs,
            vec![
                "row copy at a page capacity ceiling: 1 cell part(s) degraded to \
                 default (styles=1, hyperlinks=0, graphemes=0); see UD131"
                    .to_string()
            ],
            "exactly one degrade record for one copy"
        );
    }

    // 31 colliding colours stay one below the PSL-overflow threshold, so the set
    // rehashes instead of failing: nothing degrades, nothing is logged (the
    // reference survives this too).
    #[test]
    fn ud131_thirty_one_colours_is_the_80_column_control() {
        let o = ud131_psl_80col_run(31);
        assert!(o.y_found, "'Y' is present");
        assert!(!o.y_degraded, "'Y' keeps its new style");
        assert!(o.logs.is_empty(), "no degrade: {:?}", o.logs);
    }

    // Parity tail S7-3 routing evidence (Rust-only; the reference routes by
    // construction): the terminal-owned containers in S7-3's scope reach the
    // terminal's allocator — the tabstops' dynamic stops (> 512 columns) and
    // the pwd/title lists (grown through `gpa()`) — and `deinit` returns every
    // byte. (The screens and PageLists are covered by the Screen and PageList
    // routing tests; the glyph glossary map by glossary.rs's.)
    #[test]
    fn terminal_internals_route_through_the_allocator_handle() {
        use crate::alloc::testing::{with_handle, CountingAllocator};
        use crate::alloc::ArrayListUnmanaged;

        let counting = CountingAllocator::over_heap();
        let c = counting.allocator();
        with_handle(&c, |alloc| {
            // Baseline: 80 columns fit the preallocated tabstops.
            let mut t = Terminal::init(alloc, opts(80, 5)).unwrap();
            let base_calls = counting.alloc_calls.get();
            let base_live = counting.live_bytes.get();
            t.deinit(alloc);
            drop(t);
            assert_eq!(0, counting.live_bytes.get());

            // 600 columns: exactly one more allocation, the dynamic stops
            // (`divCeil(600 - 512, 8)` = 11 bytes).
            let before = counting.alloc_calls.get();
            let mut t = Terminal::init(alloc, opts(600, 5)).unwrap();
            assert_eq!(
                base_calls + 1,
                counting.alloc_calls.get() - before,
                "Tabstops.init(alloc, 600, ...) grows its dynamic stops through alloc"
            );
            assert_eq!(base_live + 11, counting.live_bytes.get());

            // pwd/title: `ensureTotalCapacity(self.gpa(), len + 1)`, rounded up
            // by `growCapacity`.
            let live = counting.live_bytes.get();
            t.set_pwd(b"/tmp").unwrap();
            assert_eq!(
                Some((ArrayListUnmanaged::<u8>::grow_capacity(5), 0)),
                counting.last_alloc.get(),
                "setPwd grows through gpa()"
            );
            t.set_title(b"title").unwrap();
            assert_eq!(
                Some((ArrayListUnmanaged::<u8>::grow_capacity(6), 0)),
                counting.last_alloc.get(),
                "setTitle grows through gpa()"
            );
            assert_eq!(
                live + ArrayListUnmanaged::<u8>::grow_capacity(5)
                    + ArrayListUnmanaged::<u8>::grow_capacity(6),
                counting.live_bytes.get()
            );

            t.deinit(alloc);
        });
        assert_eq!(0, counting.live_bytes.get(), "deinit returns every byte");
    }

    // Parity tail S7-3b (review F5, Rust-only): the glossary is bound to the
    // terminal's allocator at `init`; the reference grows it through the
    // per-call `alloc` of `glyphProtocol`, so a different allocator there is a
    // caller bug the port now reports instead of silently ignoring.
    #[test]
    #[cfg_attr(
        not(debug_assertions),
        ignore = "debug_assert_same is a debug-build check (R14)"
    )]
    #[should_panic(expected = "allocator mismatch")]
    fn glyph_protocol_checks_the_glossary_allocator() {
        use crate::alloc::testing::{with_handle, CountingAllocator};

        let counting = CountingAllocator::over_heap();
        let c = counting.allocator();
        with_handle(&c, |alloc| {
            let mut t = Terminal::init(alloc, opts(10, 2)).unwrap();
            // The matching allocator is accepted...
            let _ = t.glyph_protocol(alloc, &glyph::Request::Support);
            // ...a different one is not.
            let _ = t.glyph_protocol(Global, &glyph::Request::Support);
        });
    }
}
