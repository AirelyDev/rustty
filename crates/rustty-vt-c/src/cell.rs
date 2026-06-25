//! rustty-vt-c cell
//!
//! PORTS: ghostty src/terminal/c/cell.zig
//!
//! PORT STATUS: SKELETON (cluster Stage B). Types are defined byte-exact; the
//! `get`/`get_multi` bodies are `todo!()` placeholders filled in a later
//! sub-batch. The `#[repr(C)]` enums carry their reference discriminants now so
//! the C ABI is anchored from the start.

use crate::result::Result;
use std::ffi::c_void;

// const page = @import("../page.zig");
// const Cell = page.Cell;
// const color = @import("../color.zig");
// const style_c = @import("style.zig");

/// C: GhosttyCell
// pub const CCell = u64;
pub type CCell = u64;

/// C: GhosttyCellContentTag
// pub const ContentTag = enum(c_int) { codepoint = 0, ... };
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContentTag {
    Codepoint = 0,
    CodepointGrapheme = 1,
    BgColorPalette = 2,
    BgColorRgb = 3,
}

/// C: GhosttyCellWide
// pub const Wide = enum(c_int) { narrow = 0, ... };
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wide {
    Narrow = 0,
    Wide = 1,
    SpacerTail = 2,
    SpacerHead = 3,
}

/// C: GhosttyCellSemanticContent
// pub const SemanticContent = enum(c_int) { output = 0, input = 1, prompt = 2 };
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticContent {
    Output = 0,
    Input = 1,
    Prompt = 2,
}

/// C: GhosttyCellData
// pub const CellData = enum(c_int) { invalid = 0, ... };
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellData {
    Invalid = 0,

    /// The codepoint of the cell (0 if empty or bg-color-only).
    /// Output type: uint32_t * (stored as u21, zero-extended)
    Codepoint = 1,

    /// The content tag describing what kind of content is in the cell.
    /// Output type: GhosttyCellContentTag *
    ContentTag = 2,

    /// The wide property of the cell.
    /// Output type: GhosttyCellWide *
    Wide = 3,

    /// Whether the cell has text to render.
    /// Output type: bool *
    HasText = 4,

    /// Whether the cell has styling (non-default style).
    /// Output type: bool *
    HasStyling = 5,

    /// The style ID for the cell (for use with style lookups).
    /// Output type: uint16_t *
    StyleId = 6,

    /// Whether the cell has a hyperlink.
    /// Output type: bool *
    HasHyperlink = 7,

    /// Whether the cell is protected.
    /// Output type: bool *
    Protected = 8,

    /// The semantic content type of the cell (from OSC 133).
    /// Output type: GhosttyCellSemanticContent *
    SemanticContent = 9,

    /// The palette index for the cell's background color.
    /// Only valid when content_tag is bg_color_palette.
    /// Output type: GhosttyColorPaletteIndex *
    ColorPalette = 10,

    /// The RGB value for the cell's background color.
    /// Only valid when content_tag is bg_color_rgb.
    /// Output type: GhosttyColorRgb *
    ColorRgb = 11,
}

// PORT NOTE: `pub fn OutType(comptime self: CellData) type` selects the output
// type per data kind. In Rust the `getTyped` comptime dispatch collapses into a
// runtime match in the filled body; the OutType mapping is preserved as the
// per-arm pointer cast there.

/// PORT NOTE: `pub fn get(cell_, data, out) callconv(...) Result`.
///
/// # Safety
/// `out` must point to writable storage of the type required by `data`.
pub unsafe fn get(cell_: CCell, data: CellData, out: *mut c_void) -> Result {
    let _ = (cell_, data, out);
    todo!("vt-c cell::get")
}

/// PORT NOTE: `pub fn get_multi(cell_, count, keys, values, out_written) Result`.
///
/// # Safety
/// `keys`/`values`/`out_written` validity is the caller's contract.
pub unsafe fn get_multi(
    cell_: CCell,
    count: usize,
    keys: *const CellData,
    values: *mut *mut c_void,
    out_written: *mut usize,
) -> Result {
    let _ = (cell_, count, keys, values, out_written);
    todo!("vt-c cell::get_multi")
}

// PORT NOTE: the runtime-safety `intToEnum` guard on the raw `c_int` crossing the
// C boundary lives in the `extern "C"` wrapper in `lib.rs` (`rustty_cell_get`),
// mirroring the established pattern for the other guarded getters.
