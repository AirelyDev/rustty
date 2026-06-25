//! rustty-vt-c row
//!
//! PORTS: ghostty src/terminal/c/row.zig
//!
//! PORT STATUS: SKELETON (cluster Stage B). Types byte-exact; `get`/`get_multi`
//! bodies are `todo!()` placeholders filled in a later sub-batch.

use crate::result::Result;
use std::ffi::c_void;

// const page = @import("../page.zig");
// const Row = page.Row;

/// C: GhosttyRow
// pub const CRow = Row.C;  (Row.C = u64)
pub type CRow = u64;

/// C: GhosttyRowSemanticPrompt
// pub const SemanticPrompt = enum(c_int) { none = 0, prompt = 1, prompt_continuation = 2 };
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticPrompt {
    None = 0,
    Prompt = 1,
    PromptContinuation = 2,
}

/// C: GhosttyRowData
// pub const RowData = enum(c_int) { invalid = 0, ... };
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowData {
    Invalid = 0,

    /// Whether this row is soft-wrapped.
    /// Output type: bool *
    Wrap = 1,

    /// Whether this row is a continuation of a soft-wrapped row.
    /// Output type: bool *
    WrapContinuation = 2,

    /// Whether any cells in this row have grapheme clusters.
    /// Output type: bool *
    Grapheme = 3,

    /// Whether any cells in this row have styling (may have false positives).
    /// Output type: bool *
    Styled = 4,

    /// Whether any cells in this row have hyperlinks (may have false positives).
    /// Output type: bool *
    Hyperlink = 5,

    /// The semantic prompt state of this row.
    /// Output type: GhosttyRowSemanticPrompt *
    SemanticPrompt = 6,

    /// Whether this row contains a Kitty virtual placeholder.
    /// Output type: bool *
    KittyVirtualPlaceholder = 7,

    /// Whether this row is dirty and requires a redraw.
    /// Output type: bool *
    Dirty = 8,
}

/// PORT NOTE: `pub fn get(row_, data, out) callconv(...) Result`.
///
/// # Safety
/// `out` must point to writable storage of the type required by `data`.
pub unsafe fn get(row_: CRow, data: RowData, out: *mut c_void) -> Result {
    let _ = (row_, data, out);
    todo!("vt-c row::get")
}

/// PORT NOTE: `pub fn get_multi(row_, count, keys, values, out_written) Result`.
///
/// # Safety
/// `keys`/`values`/`out_written` validity is the caller's contract.
pub unsafe fn get_multi(
    row_: CRow,
    count: usize,
    keys: *const RowData,
    values: *mut *mut c_void,
    out_written: *mut usize,
) -> Result {
    let _ = (row_, count, keys, values, out_written);
    todo!("vt-c row::get_multi")
}
