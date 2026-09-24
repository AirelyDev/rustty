//! rustty-vt clipboard
//!
//! PORTS: ghostty src/terminal/clipboard.zig (phase-5 B-1 end state at
//! 683d8db64: BL-5 634ef7198 base; 5984d6f73 `isTextMime`; da27e6c90
//! `MimeReader`; e03475c0c `Read`; 7ae9b1113 reply-based `Write` +
//! `Write.Status`/`Write.Result`; 928c7f0e7 `Read.granted` targets-exemption
//! doc; a5bb22e23 paste-event cross-references)
//!
//! PORT NOTE: the intermediate `clipboard_set(kind, base64)` form (BL-5
//! 0a410f18e) and the pre-7ae9b1113 `Write -> WriteResult` return-value shape
//! are subsumed by this end state — writes and reads are answered through a
//! synchronous `reply` callback carried on the request.

/// C: RusttyClipboardLocation (R2) — checked by the A5 enum-discriminant arm.
/// The clipboard destination for a write.
// pub const Location = enum(c_int) { standard = 0, selection = 1, primary = 2, _ };
// PORT NOTE: the reference enum is NON-exhaustive (`_`), so any `c_int` is a
// `Location` and an unnamed one passes through to the handler. Ported like
// `WriteStatus`/`ReadStatus` below: a `#[repr(transparent)]` newtype over
// `i32` with an associated const per named value, so a `match` needs the
// reference's `_`/`else` arm.
// CORRECTION (parity S150-F2): this was a closed `#[repr(i32)] enum`, which
// made the C boundary reject every unnamed location (`paste.rs`, S174-F4).
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Location(pub i32);

// Associated consts keep the variant spellings; silence the naming lint (same
// shape as `WriteStatus`).
#[allow(non_upper_case_globals)]
impl Location {
    pub const Standard: Location = Location(0);
    pub const Selection: Location = Location(1);
    pub const Primary: Location = Location(2);
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

/// A request from the running program to write a clipboard.
///
/// Writes are synchronous: the effect callback must answer through `reply`
/// before it returns, and the request (including its reply context) is
/// invalid afterwards. An embedder that needs user consent must block until
/// it has an answer; the VT stream waits with it. Protocols without a write
/// acknowledgement (OSC 52, OSC 1337 Copy) discard the reply.
///
/// Contents are borrowed and only valid for the duration of the callback.
/// An empty contents slice clears the destination.
// pub const Write = struct { location, contents, name, granted, can_remember,
//     reply_ctx: *anyopaque, reply_fn: *const fn (*anyopaque, Result) void };
#[derive(Clone, Copy)]
pub struct Write<'a> {
    pub location: Location,
    pub contents: &'a [Content<'a>],

    /// Name of the writing program for permission prompts, if the
    /// protocol carries one. Empty otherwise.
    pub name: &'a [u8],

    /// True if the terminal already holds a session grant for this
    /// request (kitty clipboard protocol passwords). The embedder should
    /// skip any permission prompt and perform the write.
    pub granted: bool,

    /// True if the program supplied a session password, so the embedder
    /// may offer to remember the user's decision via
    /// Result.Success.remember. When false, remember is ignored.
    pub can_remember: bool,

    /// Terminal-owned reply state, only valid during the callback.
    // PORT NOTE (R9 callback-ctx): `*anyopaque` context + fn-pointer pair; the
    // ctx points at stack state on the terminal side and is borrowed only for
    // the duration of the effect callback (LIFETIMES.tsv: borrowed-during-call).
    // `reply_fn`'s second argument is the handler `reply` was given; see
    // `reply`.
    pub reply_ctx: *mut core::ffi::c_void,
    pub reply_fn: fn(*mut core::ffi::c_void, *mut core::ffi::c_void, WriteResult),
}

impl Write<'_> {
    /// Answer the write. May be called at most once; later calls are
    /// ignored. `handler` is the handler the effect callback was given.
    // pub fn reply(self: Write, result: Result) void {
    //     self.reply_fn(self.reply_ctx, result);
    // }
    //
    // PORT NOTE (soundness, parity S166-F1): the reference's reply state
    // holds `handler: *Handler` and applies the reply's side effects (a
    // `remember` grant; for reads, the pty response) inside `reply`, while
    // the callback runs. In Rust the callback holds `&mut Handler`, a
    // protected argument for the whole call, so a handler pointer stored in
    // the reply state before the call would be invalidated by that argument's
    // retag, and writing through it would be undefined behavior. The callback
    // therefore hands its own `&mut Handler` to `reply`, and the reply state
    // reaches the handler through a pointer derived from it (a child of the
    // protected tag, which is sound). The observable behaviour is the
    // reference's: the side effects happen inside `reply`.
    pub fn reply(&self, handler: &mut crate::stream_terminal::Handler<'_>, result: WriteResult) {
        let handler: *mut crate::stream_terminal::Handler<'_> = handler;
        (self.reply_fn)(self.reply_ctx, handler.cast(), result);
    }
}

/// C: RusttyClipboardWriteResult (R2)
/// The status of a clipboard write reply.
///
/// This crosses the C embedding boundary (`RusttyClipboardWriteResult`), so
/// the oracle deliberately models it as an *open* `enum(c_int) { …, _ }` — a
/// C callback may hand back any `c_int`, and an out-of-range value must be
/// well-defined rather than an invalid discriminant. A closed Rust `enum`
/// crossing the FFI boundary would be instant UB on any unlisted value.
///
/// We therefore represent it as a `#[repr(transparent)]` newtype over `i32`
/// (accepts ANY value without UB) plus associated consts for each named
/// value. The integer values are byte-compatible with the oracle enum and the
/// C header (`RUSTTY_CLIPBOARD_WRITE_RESULT_*`).
// pub const Status = enum(c_int) {
//     success = 0, denied = 1, unsupported = 2, busy = 3,
//     invalid_data = 4, io_error = 5, _,
// };
// PORT NOTE: the reference nests this as `Write.Status`; flattened to
// `WriteStatus` at module scope (the crate's nesting convention).
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WriteStatus(pub i32);

// Associated consts keep the original variant spellings; silence the naming
// lint (same shape as the pre-range `WriteResult` this supersedes).
#[allow(non_upper_case_globals)]
impl WriteStatus {
    pub const Success: WriteStatus = WriteStatus(0);
    pub const Denied: WriteStatus = WriteStatus(1);
    pub const Unsupported: WriteStatus = WriteStatus(2);
    pub const Busy: WriteStatus = WriteStatus(3);
    pub const InvalidData: WriteStatus = WriteStatus(4);
    pub const IoError: WriteStatus = WriteStatus(5);
}

/// The reply to a clipboard write.
// pub const Result = union(enum) {
//     denied, unsupported, busy, invalid_data, io_error,
//     success: Success,
//     pub const Success = struct { remember: bool = false };
// };
// PORT NOTE: the reference nests this as `Write.Result` (and
// `Write.Result.Success`); flattened to `WriteResult`/`WriteResultSuccess`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteResult {
    /// The write was denied by policy or the user.
    Denied,

    /// The embedder cannot write this clipboard.
    Unsupported,

    /// The clipboard is temporarily unavailable.
    Busy,

    /// One or more representations contain invalid data.
    InvalidData,

    /// Writing the clipboard failed.
    IoError,

    /// The write succeeded.
    Success(WriteResultSuccess),
}

// pub const Success = struct { remember: bool = false };
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct WriteResultSuccess {
    /// Record a session grant so future requests from the same
    /// program skip the permission prompt. Only honored when the
    /// request set `can_remember`.
    pub remember: bool,
}

/// A request from the running program to read a clipboard.
///
/// Reads are synchronous: the effect callback must answer through `reply`
/// before it returns, and the request (including its reply context) is
/// invalid afterwards. An embedder that needs user consent must block until
/// it has an answer; the VT stream waits with it.
// pub const Read = struct { ... };
#[derive(Clone, Copy)]
pub struct Read<'a> {
    pub location: Location,

    /// The MIME types the program wants, in order of preference. The
    /// reply should carry every requested representation the clipboard
    /// has; unrequested ones are ignored. Protocols that only carry text
    /// (OSC 52) request "text/plain".
    pub mimes: &'a [&'a [u8]],

    /// The program also wants the list of MIME types available on the
    /// clipboard, delivered as Result.Success.available.
    pub list: bool,

    /// Name of the requesting program for permission prompts, if the
    /// protocol carries one. Empty otherwise.
    pub name: &'a [u8],

    /// True if the terminal already holds a session grant for this
    /// request (kitty clipboard protocol passwords). The embedder should
    /// skip any permission prompt and serve the read.
    ///
    /// Always false when mimes is empty: such a request is served
    /// without a prompt (kitty's targets-listing exemption), so the
    /// terminal never consults grants for it and a one-time password
    /// is preserved for the follow-up data read.
    pub granted: bool,

    /// True if the program supplied a session password, so the embedder
    /// may offer to remember the user's decision via
    /// Result.Success.remember. When false, remember is ignored.
    pub can_remember: bool,

    /// Terminal-owned reply state, only valid during the callback.
    ///
    /// The result is delivered through a call rather than returned so
    /// the terminal consumes it while the embedder's memory is still
    /// alive; a returned slice would have to outlive the callback.
    // PORT NOTE (R9 callback-ctx): see `Write.reply_ctx`. The `for<'b>` on
    // `reply_fn` is the Rust spelling of "Result memory is borrowed only for
    // the duration of this call".
    pub reply_ctx: *mut core::ffi::c_void,
    pub reply_fn: for<'b> fn(*mut core::ffi::c_void, *mut core::ffi::c_void, ReadResult<'b>),
}

impl Read<'_> {
    /// Answer the read. May be called at most once; later calls are
    /// ignored. Result memory is borrowed only for the duration of this
    /// call. `handler` is the handler the effect callback was given.
    // pub fn reply(self: Read, result: Result) void {
    //     self.reply_fn(self.reply_ctx, result);
    // }
    // PORT NOTE (soundness, parity S166-F1): the `handler` argument; see
    // `Write::reply`.
    pub fn reply(&self, handler: &mut crate::stream_terminal::Handler<'_>, result: ReadResult<'_>) {
        let handler: *mut crate::stream_terminal::Handler<'_> = handler;
        (self.reply_fn)(self.reply_ctx, handler.cast(), result);
    }
}

/// C: RusttyClipboardReadResult (R2)
/// The status of a clipboard read reply.
// pub const Status = enum(c_int) {
//     success = 0, denied = 1, unsupported = 2, busy = 3, io_error = 4, _,
// };
// PORT NOTE: nested `Read.Status`; flattened. Open-enum newtype for the same
// C-boundary reason as `WriteStatus` (note `io_error = 4` here — the read
// status set has no `invalid_data`).
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadStatus(pub i32);

#[allow(non_upper_case_globals)]
impl ReadStatus {
    pub const Success: ReadStatus = ReadStatus(0);
    pub const Denied: ReadStatus = ReadStatus(1);
    pub const Unsupported: ReadStatus = ReadStatus(2);
    pub const Busy: ReadStatus = ReadStatus(3);
    pub const IoError: ReadStatus = ReadStatus(4);
}

/// The reply to a clipboard read.
// pub const Result = union(enum) {
//     denied, unsupported, busy, io_error,
//     success: Success,
//     pub const Success = struct { contents, available, remember };
// };
// PORT NOTE: nested `Read.Result`/`Read.Result.Success`; flattened to
// `ReadResult`/`ReadResultSuccess`.
#[derive(Clone, Copy, Debug)]
pub enum ReadResult<'a> {
    /// The read was denied by policy or the user.
    Denied,

    /// The embedder cannot read this clipboard.
    Unsupported,

    /// The clipboard is temporarily unavailable.
    Busy,

    /// Reading the clipboard failed.
    IoError,

    /// The read succeeded. All memory is borrowed for the duration
    /// of the reply call.
    Success(ReadResultSuccess<'a>),
}

// pub const Success = struct {
//     contents: []const Content = &.{},
//     available: []const []const u8 = &.{},
//     remember: bool = false,
// };
#[derive(Clone, Copy, Debug, Default)]
pub struct ReadResultSuccess<'a> {
    /// Representations of the clipboard contents, one per
    /// requested MIME type the clipboard has. Protocols that
    /// carry a single text value use the first entry with a
    /// text MIME type (see isTextMime).
    pub contents: &'a [Content<'a>],

    /// All MIME types available on the clipboard. Only used when
    /// the request set `list`.
    pub available: &'a [&'a [u8]],

    /// Record a session grant so future requests from the same
    /// program skip the permission prompt. Only honored when the
    /// request set `can_remember`.
    pub remember: bool,
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
