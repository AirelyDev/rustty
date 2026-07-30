//! rustty-vt clipboard
//!
//! PORTS: ghostty src/terminal/clipboard.zig (BL-5 634ef7198)
//!
//! PORT NOTE: the intermediate `clipboard_set(kind, base64)` form (BL-5
//! 0a410f18e) is subsumed by this protocol-neutral shape — a normalized
//! `Location` plus decoded MIME `Content` representations.

/// C: RusttyClipboardLocation (R2) — checked by the A5 enum-discriminant arm.
/// The clipboard destination for a write.
// pub const Location = enum(c_int) { standard = 0, selection = 1, primary = 2, _ };
#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Location {
    Standard = 0,
    Selection = 1,
    Primary = 2,
}

/// A single representation of clipboard data.
///
/// The MIME type and data are borrowed and only valid for the duration of a
/// clipboard write callback. Data is binary-safe.
// pub const Content = struct { mime: []const u8, data: []const u8 };
#[derive(Clone, Copy, Debug)]
pub struct Content<'a> {
    pub mime: &'a [u8],
    pub data: &'a [u8],
}

/// One atomic clipboard write.
///
/// Contents are borrowed and only valid for the duration of a clipboard write
/// callback. An empty contents slice clears the destination.
// pub const Write = struct { location: Location, contents: []const Content };
#[derive(Clone, Copy, Debug)]
pub struct Write<'a> {
    pub location: Location,
    pub contents: &'a [Content<'a>],
}

/// The result of a clipboard write.
///
/// This is RETURNED FROM a C embedder callback (`ClipboardWriteFn`), so the
/// oracle deliberately models it as an *open* `enum(c_int) { …, _ }` — a C
/// callback may hand back any `c_int`, and an out-of-range value must be
/// well-defined rather than an invalid discriminant. A closed Rust `enum`
/// crossing the FFI boundary would be instant UB on any unlisted return.
///
/// We therefore represent it as a `#[repr(transparent)]` newtype over `i32`
/// (accepts ANY value without UB) plus associated consts for each named
/// value. The integer values are byte-compatible with the oracle enum and the
/// C header (`RUSTTY_CLIPBOARD_WRITE_RESULT_*`).
// pub const WriteResult = enum(c_int) { success..io_error, _ };
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WriteResult(pub i32);

// Associated consts keep the original variant spellings so existing call
// sites (`WriteResult::Success`, …) are unchanged; silence the naming lint.
#[allow(non_upper_case_globals)]
impl WriteResult {
    /// The clipboard write completed successfully.
    pub const Success: WriteResult = WriteResult(0);
    /// The clipboard write was denied by policy or the user.
    pub const Denied: WriteResult = WriteResult(1);
    /// The destination or one or more representations are unsupported.
    pub const Unsupported: WriteResult = WriteResult(2);
    /// The clipboard is temporarily unavailable.
    pub const Busy: WriteResult = WriteResult(3);
    /// One or more representations contain invalid data.
    pub const InvalidData: WriteResult = WriteResult(4);
    /// The clipboard write failed due to an I/O error.
    pub const IoError: WriteResult = WriteResult(5);
}
