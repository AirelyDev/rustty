//! rustty-vt clipboard
//!
//! PORTS: ghostty src/terminal/clipboard.zig (BL-5 634ef7198; phase-5
//! paste-cluster slices: 5984d6f73 `isTextMime`, da27e6c90 `MimeReader`)
//!
//! PORT NOTE: the intermediate `clipboard_set(kind, base64)` form (BL-5
//! 0a410f18e) is subsumed by this protocol-neutral shape — a normalized
//! `Location` plus decoded MIME `Content` representations.
//!
//! PORT NOTE (W3/W6 seam): the phase-5 head reworks `Write` into the
//! reply-based effect shape (`reply_ctx`/`reply_fn` + `Result` union) and adds
//! the `Read` effect; those land with the W3 kitty-clipboard / W6 C-effects
//! bundles. This file carries only the paste-cluster slices `paste.rs` needs.

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

/// MIME types that name plain text across the platforms terminals run
/// on. We accept the union everywhere since serving text under any of these
/// names is harmless.
pub fn is_text_mime(mime: &[u8]) -> bool {
    // const names: []const []const u8 = &.{ ... };
    const NAMES: &[&[u8]] = &[
        b"text/plain",
        b"text/plain;charset=utf-8",
        b"UTF8_STRING",
        b"TEXT",
        b"STRING",
    ];
    // for (names) |n| if (std.mem.eql(u8, mime, n)) return true;
    for n in NAMES {
        if mime == *n {
            return true;
        }
    }
    false
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

/// Requests content of a specific mime-type. For now this is used for
/// on-demand clipboard access since content can be large (particularly
/// non-text content), but it is generic so that this could handle other
/// mime-typed sources in the future like maybe drag-and-drop.
///
/// C: RusttyMimeReader (R2)
// pub const MimeReader = struct {
//     ctx: ?*anyopaque = null,
//     read_fn: *const fn (
//         ctx: ?*anyopaque,
//         mime: []const u8,
//         sink: *std.Io.Writer,
//     ) Error!void,
// };
// PORT NOTE: `*std.Io.Writer` maps to the crate's byte-oriented stand-in,
// `snapshot::io::Writer`. The `?*anyopaque = null` context is a nullable raw
// pointer; the struct is `Copy` like the reference value type. Raw-pointer
// field: the reader is borrowed only for the duration of a `read` call, so no
// lifetime is tied to `ctx` (LIFETIMES.tsv: borrowed-during-call).
#[derive(Clone, Copy)]
pub struct MimeReader {
    /// Passed through to `read_fn`.
    pub ctx: *mut core::ffi::c_void,

    /// Write all the data of the representation named by `mime` to
    /// `sink`, in as many writes as is convenient. The mime and sink
    /// are borrowed, only valid for the duration of the call, and
    /// nothing written to the sink is retained, so the data may be
    /// borrowed from anywhere. Return error.ReadFailed if the data
    /// can't be produced and propagate error.WriteFailed from the
    /// sink.
    pub read_fn: fn(
        ctx: *mut core::ffi::c_void,
        mime: &[u8],
        sink: &mut dyn crate::snapshot::io::Writer,
    ) -> Result<(), MimeReaderError>,
}

// PORT NOTE: the reference nests this as `MimeReader.Error`; Rust structs
// cannot nest type declarations, so it is flattened to module scope.
// pub const Error = error{
//     /// The data could not be read.
//     ReadFailed,
// } || std.Io.Writer.Error;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MimeReaderError {
    /// The data could not be read.
    ReadFailed,
    /// `std.Io.Writer.Error` — `error.WriteFailed`, propagated from the sink.
    WriteFailed,
}

impl MimeReader {
    pub fn read(
        &self,
        mime: &[u8],
        sink: &mut dyn crate::snapshot::io::Writer,
    ) -> Result<(), MimeReaderError> {
        (self.read_fn)(self.ctx, mime, sink)
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    // test isTextMime
    #[test]
    fn is_text_mime_test() {
        assert!(is_text_mime(b"text/plain"));
        assert!(is_text_mime(b"UTF8_STRING"));
        assert!(!is_text_mime(b"image/png"));
        assert!(!is_text_mime(b"."));
    }
}
