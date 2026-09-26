//! Complete terminal snapshot encoding and restoration.
//!
//! PORTS: ghostty src/terminal/snapshot/snapshot.zig (imported by the
//! reference `main.zig` under the local name `codec`, hence this file name).
//!
//! PORT STATUS: complete (phase-4 wsnap-3) — the codec drivers are live:
//! `EncodeError`/`DecodeError`/`DecodeExactError`, `encode`, `decode`,
//! `decode_exact`, `Decoded` (with `deinit`/`to_owned`), and the incremental
//! `Decoder` (`init` → `ready` → `next`, END STATE e37865bed with the
//! invalid-state reporting of cbc9f360b/7d9aaa297/58e92098a and the
//! kitty-placeholder acceptance of f8ac0ca98). NO BLAKE3 anywhere (removed by
//! 219173ab3). The reference `io` parameter (`std.Io`) threaded through
//! `decode`/`ready` has no Rust counterpart yet and is dropped from the
//! signatures (the crate-wide treatment; see `snapshot/screen.rs`'s decode
//! PORT NOTE) — area C's TinyIo analogue owns that seam.

// const std = @import("std");
// const build_options = @import("terminal_options");
// const assert = std.debug.assert;
// const Allocator = std.mem.Allocator;
// const checkpoint = @import("checkpoint.zig");
// const continuation = @import("continuation.zig");
// const envelope = @import("envelope.zig");
// const test_fixture = @import("fixture.zig");
// const history = @import("history.zig");
// const page = @import("page.zig");
// const record = @import("record.zig");
// const screen = @import("screen.zig");
// const size = @import("../size.zig");
// const terminal = @import("terminal.zig");
// const Terminal = @import("../Terminal.zig");
// const TerminalStream = @import("../stream_terminal.zig").Stream;
// const terminal_kitty = @import("../kitty.zig");
// const TerminalPageList = @import("../PageList.zig");
// const TerminalScreen = @import("../Screen.zig");
// const TerminalScreenKey = @import("../ScreenSet.zig").Key;
use super::checkpoint;
use super::continuation;
use super::envelope;
use super::history;
use super::io;
use super::page;
use super::record;
use super::screen;
use super::terminal;
use crate::screen::Screen as TerminalScreen;
use crate::screen_set::Key as TerminalScreenKey;
use crate::size;
use crate::terminal::Terminal;

// PORT NOTE: the reference `Allocator.Error` maps to the crate's canonical
// OOM; the general-purpose allocator maps to `crate::alloc::Global`
// (the crate-wide treatment; see `snapshot/screen.rs`).
use crate::alloc::Allocator;

/// Re-export continuation to make it a bit more ergonomic to reference.
// pub const Continuation = continuation.Value;
pub type Continuation<'a> = continuation::Value<'a>;

/// Errors possible while encoding one complete terminal snapshot.
// pub const EncodeError = terminal.EncodeError || screen.EncodeError ||
//     history.EncodeError || checkpoint.EncodeError || continuation.EncodeError;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncodeError {
    // Allocator.Error (screen/history)
    OutOfMemory,
    // std.Io.Writer.Error / record.Writer.FinishError
    WriteFailed,
    PayloadTooLarge,
    // terminal.EncodeError
    ScrollbackLimitOverflow,
    InvalidDimensions,
    InvalidScrollingRegion,
    InvalidScreenCount,
    InvalidActiveScreenKey,
    InvalidPreviousCodepoint,
    InvalidScrollbackLimit,
    InvalidTabStops,
    InvalidPalette,
    StringTooLong,
    // screen.EncodeError
    InvalidUri,
    InvalidExplicitId,
    InvalidCursorFlags,
    InvalidCharsetState,
    InvalidKittyKeyboardIndex,
    InvalidKittyKeyboardFlags,
    InvalidSavedCursorFlags,
    InvalidWideCell,
    TooManyGraphemes,
    // screen/history
    PageCountOverflow,
    // continuation.EncodeError
    NoPendingState,
    NonCanonicalContinuation,
    ReplayWouldCommit,
    Overflow,
}

impl From<io::WriteFailed> for EncodeError {
    fn from(_: io::WriteFailed) -> EncodeError {
        EncodeError::WriteFailed
    }
}

impl From<continuation::ValidateError> for EncodeError {
    fn from(err: continuation::ValidateError) -> EncodeError {
        match err {
            continuation::ValidateError::NoPendingState => EncodeError::NoPendingState,
            continuation::ValidateError::NonCanonicalContinuation => {
                EncodeError::NonCanonicalContinuation
            }
            continuation::ValidateError::ReplayWouldCommit => EncodeError::ReplayWouldCommit,
            continuation::ValidateError::Overflow => EncodeError::Overflow,
        }
    }
}

impl From<continuation::EncodeError> for EncodeError {
    fn from(err: continuation::EncodeError) -> EncodeError {
        match err {
            continuation::EncodeError::NoPendingState => EncodeError::NoPendingState,
            continuation::EncodeError::NonCanonicalContinuation => {
                EncodeError::NonCanonicalContinuation
            }
            continuation::EncodeError::ReplayWouldCommit => EncodeError::ReplayWouldCommit,
            continuation::EncodeError::Overflow => EncodeError::Overflow,
            continuation::EncodeError::WriteFailed => EncodeError::WriteFailed,
            continuation::EncodeError::PayloadTooLarge => EncodeError::PayloadTooLarge,
        }
    }
}

impl From<terminal::EncodeError> for EncodeError {
    fn from(err: terminal::EncodeError) -> EncodeError {
        match err {
            terminal::EncodeError::ScrollbackLimitOverflow => EncodeError::ScrollbackLimitOverflow,
            terminal::EncodeError::WriteFailed => EncodeError::WriteFailed,
            terminal::EncodeError::InvalidDimensions => EncodeError::InvalidDimensions,
            terminal::EncodeError::InvalidScrollingRegion => EncodeError::InvalidScrollingRegion,
            terminal::EncodeError::InvalidScreenCount => EncodeError::InvalidScreenCount,
            terminal::EncodeError::InvalidActiveScreenKey => EncodeError::InvalidActiveScreenKey,
            terminal::EncodeError::InvalidPreviousCodepoint => {
                EncodeError::InvalidPreviousCodepoint
            }
            terminal::EncodeError::InvalidScrollbackLimit => EncodeError::InvalidScrollbackLimit,
            terminal::EncodeError::InvalidTabStops => EncodeError::InvalidTabStops,
            terminal::EncodeError::InvalidPalette => EncodeError::InvalidPalette,
            terminal::EncodeError::StringTooLong => EncodeError::StringTooLong,
            terminal::EncodeError::PayloadTooLarge => EncodeError::PayloadTooLarge,
        }
    }
}

impl From<screen::EncodeError> for EncodeError {
    fn from(err: screen::EncodeError) -> EncodeError {
        match err {
            screen::EncodeError::OutOfMemory => EncodeError::OutOfMemory,
            screen::EncodeError::WriteFailed => EncodeError::WriteFailed,
            screen::EncodeError::InvalidUri => EncodeError::InvalidUri,
            screen::EncodeError::InvalidExplicitId => EncodeError::InvalidExplicitId,
            screen::EncodeError::StringTooLong => EncodeError::StringTooLong,
            screen::EncodeError::InvalidCursorFlags => EncodeError::InvalidCursorFlags,
            screen::EncodeError::InvalidCharsetState => EncodeError::InvalidCharsetState,
            screen::EncodeError::InvalidKittyKeyboardIndex => {
                EncodeError::InvalidKittyKeyboardIndex
            }
            screen::EncodeError::InvalidKittyKeyboardFlags => {
                EncodeError::InvalidKittyKeyboardFlags
            }
            screen::EncodeError::InvalidSavedCursorFlags => EncodeError::InvalidSavedCursorFlags,
            screen::EncodeError::InvalidWideCell => EncodeError::InvalidWideCell,
            screen::EncodeError::TooManyGraphemes => EncodeError::TooManyGraphemes,
            screen::EncodeError::PayloadTooLarge => EncodeError::PayloadTooLarge,
            screen::EncodeError::PageCountOverflow => EncodeError::PageCountOverflow,
        }
    }
}

impl From<history::EncodeError> for EncodeError {
    fn from(err: history::EncodeError) -> EncodeError {
        match err {
            history::EncodeError::OutOfMemory => EncodeError::OutOfMemory,
            history::EncodeError::WriteFailed => EncodeError::WriteFailed,
            history::EncodeError::InvalidUri => EncodeError::InvalidUri,
            history::EncodeError::InvalidExplicitId => EncodeError::InvalidExplicitId,
            history::EncodeError::StringTooLong => EncodeError::StringTooLong,
            history::EncodeError::InvalidWideCell => EncodeError::InvalidWideCell,
            history::EncodeError::TooManyGraphemes => EncodeError::TooManyGraphemes,
            history::EncodeError::PayloadTooLarge => EncodeError::PayloadTooLarge,
            history::EncodeError::PageCountOverflow => EncodeError::PageCountOverflow,
        }
    }
}

// checkpoint.EncodeError == record.Writer.FinishError
impl From<record::FinishError> for EncodeError {
    fn from(err: record::FinishError) -> EncodeError {
        match err {
            record::FinishError::WriteFailed => EncodeError::WriteFailed,
            record::FinishError::PayloadTooLarge => EncodeError::PayloadTooLarge,
        }
    }
}

pub struct EncodeOptions<'a> {
    pub continuation: Continuation<'a>,
}

/// Encode one complete terminal snapshot.
///
/// Encoding starts at the destination's current position. Only one record
/// payload is buffered at a time; completed records stream immediately. On
/// failure, the destination may contain a snapshot prefix without its required
/// FINISH marker, and an output failure may have written part of a record.
// PORT NOTE: the reference threads `alloc` into `record.Writer`'s scratch
// buffer; the port's `record::Writer` uses the global allocator (see its PORT
// NOTE), so the parameter is dropped. The destination is `&mut dyn io::Writer`
// (the reference's type-erased `*std.Io.Writer`).
pub fn encode(
    destination: &mut dyn io::Writer,
    t: &Terminal,
    options: EncodeOptions<'_>,
) -> Result<(), EncodeError> {
    // Continuation errors must not emit even the snapshot envelope.
    continuation::validate(&options.continuation)?;

    // A measured 512-byte buffer handles our smallest records on the stack
    // before larger records fall back to the heap.
    //
    // PORT NOTE (`ee8095d37`): `var stack_alloc = std.heap.stackFallback(512,
    // alloc); var stream: record.Writer = .init(stack_alloc.get(), destination);`
    // — the port's `record::Writer` carries the 512-byte stack scratch itself
    // (`record::RECORD_SCRATCH_STACK_BYTES`), so no allocator is threaded here.
    let mut stream = record::Writer::init(destination);
    // defer stream.deinit(); — the writer's scratch frees on drop at every
    // early return; the success path deinits explicitly below.

    // 1. Envelope
    envelope::encode(stream.writer())?;

    // 2. Terminal
    terminal::encode(t, &mut stream)?;

    // 3. Primary and alt screen
    //
    // SAFETY (each deref below): `screens.get` returns live screens owned by
    // `t`'s screen set; encode only reads them.
    let primary = t.screens.get(TerminalScreenKey::Primary).unwrap();
    screen::encode(
        unsafe { &*primary },
        TerminalScreenKey::Primary,
        &mut stream,
    )?;
    if let Some(alternate) = t.screens.get(TerminalScreenKey::Alternate) {
        screen::encode(
            unsafe { &*alternate },
            TerminalScreenKey::Alternate,
            &mut stream,
        )?;
    }

    // 4. Standard Stream continuation.
    continuation::encode(&options.continuation, &mut stream)?;

    // 5. Ready marker.
    checkpoint::encode(checkpoint::Kind::Ready, &mut stream)?;

    // 6. History
    history::encode(
        unsafe { &*primary },
        TerminalScreenKey::Primary,
        &mut stream,
    )?;
    if let Some(alternate) = t.screens.get(TerminalScreenKey::Alternate) {
        history::encode(
            unsafe { &*alternate },
            TerminalScreenKey::Alternate,
            &mut stream,
        )?;
    }

    // 7. Finish
    checkpoint::encode(checkpoint::Kind::Finish, &mut stream)?;
    stream.deinit();
    Ok(())
}

/// Errors possible while restoring one complete terminal snapshot.
// pub const DecodeError = Decoder.ReadyError || Decoder.NextError;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    // ReadyError
    ReadFailed,
    EndOfStream,
    InvalidMagic,
    UnsupportedVersion,
    OutOfMemory,
    InvalidDimensions,
    InvalidScreenCount,
    InvalidTag,
    InvalidChecksum,
    PayloadNotExhausted,
    UnexpectedRecordTag,
    InvalidKey,
    InvalidRowCellCount,
    InvalidKind,
    InvalidDestinationCapacity,
    Integrity(crate::page::IntegrityError),
    InvalidPageDimensions,
    NoPages,
    InsufficientRows,
    OutOfSpace,
    InvalidPageCount,
    NoPendingState,
    NonCanonicalContinuation,
    ReplayWouldCommit,
    ContinuationLimitExceeded,
    UnexpectedScreenKey,
    DuplicateScreen,
    // NextError additions
    DecoderNotReady,
    DecoderFailed,
    UnexpectedHistoryKey,
    DuplicateHistory,
    RowCountOverflow,
    PageSizeOverflow,
}

impl From<ReadyError> for DecodeError {
    fn from(err: ReadyError) -> DecodeError {
        match err {
            ReadyError::ReadFailed => DecodeError::ReadFailed,
            ReadyError::EndOfStream => DecodeError::EndOfStream,
            ReadyError::InvalidMagic => DecodeError::InvalidMagic,
            ReadyError::UnsupportedVersion => DecodeError::UnsupportedVersion,
            ReadyError::OutOfMemory => DecodeError::OutOfMemory,
            ReadyError::InvalidDimensions => DecodeError::InvalidDimensions,
            ReadyError::InvalidScreenCount => DecodeError::InvalidScreenCount,
            ReadyError::InvalidTag => DecodeError::InvalidTag,
            ReadyError::InvalidChecksum => DecodeError::InvalidChecksum,
            ReadyError::PayloadNotExhausted => DecodeError::PayloadNotExhausted,
            ReadyError::UnexpectedRecordTag => DecodeError::UnexpectedRecordTag,
            ReadyError::InvalidKey => DecodeError::InvalidKey,
            ReadyError::InvalidRowCellCount => DecodeError::InvalidRowCellCount,
            ReadyError::InvalidKind => DecodeError::InvalidKind,
            ReadyError::InvalidDestinationCapacity => DecodeError::InvalidDestinationCapacity,
            ReadyError::Integrity(err) => DecodeError::Integrity(err),
            ReadyError::InvalidPageDimensions => DecodeError::InvalidPageDimensions,
            ReadyError::NoPages => DecodeError::NoPages,
            ReadyError::InsufficientRows => DecodeError::InsufficientRows,
            ReadyError::OutOfSpace => DecodeError::OutOfSpace,
            ReadyError::InvalidPageCount => DecodeError::InvalidPageCount,
            ReadyError::NoPendingState => DecodeError::NoPendingState,
            ReadyError::NonCanonicalContinuation => DecodeError::NonCanonicalContinuation,
            ReadyError::ReplayWouldCommit => DecodeError::ReplayWouldCommit,
            ReadyError::ContinuationLimitExceeded => DecodeError::ContinuationLimitExceeded,
            ReadyError::UnexpectedScreenKey => DecodeError::UnexpectedScreenKey,
            ReadyError::DuplicateScreen => DecodeError::DuplicateScreen,
        }
    }
}

impl From<NextError> for DecodeError {
    fn from(err: NextError) -> DecodeError {
        match err {
            NextError::ReadFailed => DecodeError::ReadFailed,
            NextError::EndOfStream => DecodeError::EndOfStream,
            NextError::InvalidTag => DecodeError::InvalidTag,
            NextError::InvalidChecksum => DecodeError::InvalidChecksum,
            NextError::PayloadNotExhausted => DecodeError::PayloadNotExhausted,
            NextError::UnexpectedRecordTag => DecodeError::UnexpectedRecordTag,
            NextError::InvalidKey => DecodeError::InvalidKey,
            NextError::InvalidDimensions => DecodeError::InvalidDimensions,
            NextError::InvalidRowCellCount => DecodeError::InvalidRowCellCount,
            NextError::InvalidKind => DecodeError::InvalidKind,
            NextError::OutOfMemory => DecodeError::OutOfMemory,
            NextError::InvalidDestinationCapacity => DecodeError::InvalidDestinationCapacity,
            NextError::Integrity(err) => DecodeError::Integrity(err),
            NextError::DecoderNotReady => DecodeError::DecoderNotReady,
            NextError::DecoderFailed => DecodeError::DecoderFailed,
            NextError::UnexpectedHistoryKey => DecodeError::UnexpectedHistoryKey,
            NextError::DuplicateHistory => DecodeError::DuplicateHistory,
            NextError::InvalidPageDimensions => DecodeError::InvalidPageDimensions,
            NextError::RowCountOverflow => DecodeError::RowCountOverflow,
            NextError::PageSizeOverflow => DecodeError::PageSizeOverflow,
        }
    }
}

pub struct DecodeOptions {
    /// Largest non-ground continuation the decoder may allocate and return.
    /// Set this to zero when only ground-state snapshots are acceptable.
    pub max_continuation_bytes: usize,
}

/// PORT NOTE: `std.EnumMap(TerminalScreenKey, u64)` — a per-key array of
/// optional values indexed by `Key as usize` (the `screen_set.rs` convention)
/// with the reference's `get`/`put` surface.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HistoryRows {
    values: [Option<u64>; TerminalScreenKey::COUNT],
}

impl HistoryRows {
    pub fn get(&self, key: TerminalScreenKey) -> Option<u64> {
        self.values[key as usize]
    }

    pub fn put(&mut self, key: TerminalScreenKey, value: u64) {
        self.values[key as usize] = Some(value);
    }
}

/// The screen keys in `std.EnumMap` iteration order (declaration order).
const KEYS: [TerminalScreenKey; TerminalScreenKey::COUNT] =
    [TerminalScreenKey::Primary, TerminalScreenKey::Alternate];

/// One decoded Terminal and the bytes needed to resume its Stream.
///
/// `decode` returns this with all history present; `Decoder.ready` returns
/// it at READY, when the terminal is complete for interactive use but its
/// scrollback is still arriving through `Decoder.next`.
///
/// A successful decode owns both values. Keep this result alive until the
/// Terminal's Stream has been restored, and always finish by calling `deinit`.
/// The usual restoration sequence is:
///
///   1. Inspect `continuation` and use its length to size continuation tracking.
///   2. Call `toOwned` once and store the returned Terminal at its final address.
///   3. Create the persistent, read-only standard TerminalStream for that
///      address. If `continuation` contains bytes, feed them exactly once and
///      verify that re-exporting the continuation returns the same bytes.
///   4. Call `deinit` to release the decoded continuation. The transferred
///      Terminal and its Stream remain owned by the caller.
///   5. Process PTY bytes that came after the snapshot cut.
///
/// Do not create a Stream against the address of `terminal` in this struct.
/// `toOwned` moves the Terminal, which would leave such a Stream pointing at
/// its old address.
pub struct Decoded {
    /// Present until `toOwned` transfers the Terminal to the caller. Callers
    /// may inspect it, but must transfer it before attaching a persistent
    /// TerminalStream.
    pub terminal: Option<Terminal>,

    /// For decoded results, nonempty bytes are allocator-owned and remain
    /// valid until `deinit`. Ground needs no replay. Bytes must be replayed
    /// exactly once after the Terminal has reached its final address.
    pub continuation: Continuation<'static>,

    /// Advisory complete logical history extent declared by each restored
    /// screen, including the resident overlap already carried by that
    /// screen's own pages. A client may size scrollback UI from this at
    /// READY, before HISTORY pages arrive; native row totals always remain
    /// derived from the pages actually applied.
    pub history_rows: HistoryRows,
}

impl Decoded {
    /// Destroy an untransferred Terminal and always free continuation bytes.
    /// After `toOwned`, this leaves the caller-owned Terminal untouched.
    pub fn deinit(&mut self, alloc: Allocator) {
        if let Some(value) = self.terminal.as_mut() {
            value.deinit(alloc);
        }
        self.terminal = None;
        // switch (self.continuation) { .ground => {}, .bytes => |b| alloc.free(b) }
        // — the owned Cow frees on replacement. self.* = undefined;
        self.continuation = Continuation::Ground;
    }

    /// Transfer the Terminal while retaining the continuation for replay.
    ///
    /// This may be called exactly once. Store the returned value directly at
    /// its final address before creating the TerminalStream that will replay
    /// `continuation`.
    pub fn to_owned(&mut self) -> Terminal {
        // const result = self.terminal.?; self.terminal = null; return result;
        self.terminal.take().unwrap()
    }
}

/// Incrementally restore one snapshot: renderable state first, then history.
///
/// The snapshot record order makes a terminal renderable at the READY
/// marker, after only a small prefix of the stream. `Decoder` exposes
/// that boundary: `ready` returns a complete, integrity-checked `Decoded` while
/// scrollback is still in flight, and each `next` call then applies one
/// history PAGE record off the critical path. `snapshot.decode` is the
/// one-shot form and is implemented over this type.
///
/// ```zig
/// var decoder: snapshot.Decoder = .init(&reader);
/// var decoded = try decoder.ready(alloc, io, .{
///     .max_continuation_bytes = 1024 * 1024,
/// });
/// defer decoded.deinit(alloc);
///
/// var terminal = decoded.toOwned();
/// defer terminal.deinit(alloc);
/// // The terminal is usable now: restore its Stream from the decoded
/// // continuation and size scrollback UI from `decoded.history_rows`.
///
/// while (try decoder.next(alloc, &terminal)) |progress| {
///     _ = progress; // Scrollback grew; e.g. refresh the scrollbar.
/// }
/// // FINISH validated: the complete snapshot has been decoded.
/// ```
///
/// The terminal is live between `next` calls: the caller may render it and
/// even feed it PTY bytes that arrived after the snapshot cut. `next`
/// re-validates its destination on every call and degrades to discarding,
/// consuming and validating wire bytes without applying them, when live
/// changes have made a page inapplicable. See `next` for the drop rules.
///
/// The decoder owns no allocations and needs no deinit. It retains the
/// source pointer given to `init`; the source must stay valid, and must not
/// be read elsewhere, until decoding ends. The decoder itself is movable
/// between calls. After any returned error both the decoder and the stream
/// position are invalid: abandon the decoder, and destroy an already
/// transferred Terminal or keep it with partial history, but do not resume.
pub struct Decoder<'s> {
    source: &'s mut dyn io::Reader,
    state: State,
}

// const State = union(enum) { start, history: History, finished, failed };
enum State {
    /// `ready` has not been called.
    Start,

    /// READY validated; HISTORY sequences remain before FINISH.
    History(History),

    /// FINISH validated. `next` returns null.
    Finished,

    /// A call returned an error; the stream position is unknown.
    Failed,
}

struct History {
    /// The generation observed at READY for every screen the snapshot
    /// declared. Presence is the declaration check for HISTORY routing;
    /// the value detects screens removed or replaced by live terminal
    /// use between calls.
    // PORT NOTE: `std.EnumMap(TerminalScreenKey, usize)` → per-key array
    // (see `HistoryRows`).
    generations: [Option<usize>; TerminalScreenKey::COUNT],

    /// Keys whose HISTORY sequence was already consumed.
    // PORT NOTE: `std.EnumSet(TerminalScreenKey)` → per-key bool array.
    routed: [bool; TerminalScreenKey::COUNT],

    /// HISTORY sequences that have not begun streaming yet.
    pending: usize,

    /// The sequence currently streaming PAGE records.
    current: Option<Sequence>,

    /// Terminal width at READY. Encoded pages assume this width and are
    /// dropped rather than mixed into a screen reflowed to another.
    cols: size::CellCountInt,
}

struct Sequence {
    key: TerminalScreenKey,

    /// PAGE records not yet consumed from this sequence.
    remaining: u32,

    /// False once any page in this sequence was dropped. Later pages
    /// are older, and applying one above a gap would corrupt scrollback
    /// order, so a single drop discards the remainder of the sequence.
    apply: bool,
}

/// Errors possible while decoding the renderable prefix through READY.
// pub const ReadyError = envelope.DecodeError || terminal.DecodeError ||
//     screen.DecodeError || continuation.DecodeError || checkpoint.DecodeError ||
//     error{ UnexpectedScreenKey, DuplicateScreen };
//
// PORT NOTE: hoisted out of `Decoder` to the module (Rust has no namespaced
// type declarations inside an `impl`), like `Progress` and `NextError`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadyError {
    // envelope.DecodeError
    ReadFailed,
    EndOfStream,
    InvalidMagic,
    UnsupportedVersion,
    // terminal.DecodeError
    OutOfMemory,
    InvalidDimensions,
    InvalidScreenCount,
    InvalidTag,
    InvalidChecksum,
    PayloadNotExhausted,
    UnexpectedRecordTag,
    // screen.DecodeError
    InvalidKey,
    InvalidRowCellCount,
    InvalidKind,
    InvalidDestinationCapacity,
    Integrity(crate::page::IntegrityError),
    InvalidPageDimensions,
    NoPages,
    InsufficientRows,
    OutOfSpace,
    InvalidPageCount,
    // continuation.DecodeError
    NoPendingState,
    NonCanonicalContinuation,
    ReplayWouldCommit,
    ContinuationLimitExceeded,

    /// A SCREEN names a key not declared by TERMINAL.
    UnexpectedScreenKey,

    /// More than one SCREEN names the same key.
    DuplicateScreen,
}

impl From<envelope::DecodeError> for ReadyError {
    fn from(err: envelope::DecodeError) -> ReadyError {
        match err {
            envelope::DecodeError::ReadFailed => ReadyError::ReadFailed,
            envelope::DecodeError::EndOfStream => ReadyError::EndOfStream,
            envelope::DecodeError::InvalidMagic => ReadyError::InvalidMagic,
            envelope::DecodeError::UnsupportedVersion => ReadyError::UnsupportedVersion,
        }
    }
}

impl From<terminal::DecodeError> for ReadyError {
    fn from(err: terminal::DecodeError) -> ReadyError {
        match err {
            terminal::DecodeError::ReadFailed => ReadyError::ReadFailed,
            terminal::DecodeError::EndOfStream => ReadyError::EndOfStream,
            terminal::DecodeError::OutOfMemory => ReadyError::OutOfMemory,
            terminal::DecodeError::InvalidDimensions => ReadyError::InvalidDimensions,
            terminal::DecodeError::InvalidScreenCount => ReadyError::InvalidScreenCount,
            terminal::DecodeError::InvalidTag => ReadyError::InvalidTag,
            terminal::DecodeError::InvalidChecksum => ReadyError::InvalidChecksum,
            terminal::DecodeError::PayloadNotExhausted => ReadyError::PayloadNotExhausted,
            terminal::DecodeError::UnexpectedRecordTag => ReadyError::UnexpectedRecordTag,
        }
    }
}

impl From<screen::DecodeError> for ReadyError {
    fn from(err: screen::DecodeError) -> ReadyError {
        match err {
            screen::DecodeError::ReadFailed => ReadyError::ReadFailed,
            screen::DecodeError::EndOfStream => ReadyError::EndOfStream,
            screen::DecodeError::InvalidKey => ReadyError::InvalidKey,
            screen::DecodeError::InvalidDimensions => ReadyError::InvalidDimensions,
            screen::DecodeError::InvalidRowCellCount => ReadyError::InvalidRowCellCount,
            screen::DecodeError::InvalidKind => ReadyError::InvalidKind,
            screen::DecodeError::OutOfMemory => ReadyError::OutOfMemory,
            screen::DecodeError::InvalidDestinationCapacity => {
                ReadyError::InvalidDestinationCapacity
            }
            screen::DecodeError::Integrity(err) => ReadyError::Integrity(err),
            screen::DecodeError::InvalidTag => ReadyError::InvalidTag,
            screen::DecodeError::InvalidChecksum => ReadyError::InvalidChecksum,
            screen::DecodeError::PayloadNotExhausted => ReadyError::PayloadNotExhausted,
            screen::DecodeError::InvalidPageDimensions => ReadyError::InvalidPageDimensions,
            screen::DecodeError::NoPages => ReadyError::NoPages,
            screen::DecodeError::InsufficientRows => ReadyError::InsufficientRows,
            screen::DecodeError::OutOfSpace => ReadyError::OutOfSpace,
            screen::DecodeError::UnexpectedRecordTag => ReadyError::UnexpectedRecordTag,
            screen::DecodeError::InvalidPageCount => ReadyError::InvalidPageCount,
        }
    }
}

impl From<continuation::DecodeError> for ReadyError {
    fn from(err: continuation::DecodeError) -> ReadyError {
        match err {
            continuation::DecodeError::OutOfMemory => ReadyError::OutOfMemory,
            continuation::DecodeError::NoPendingState => ReadyError::NoPendingState,
            continuation::DecodeError::NonCanonicalContinuation => {
                ReadyError::NonCanonicalContinuation
            }
            continuation::DecodeError::ReplayWouldCommit => ReadyError::ReplayWouldCommit,
            continuation::DecodeError::ReadFailed => ReadyError::ReadFailed,
            continuation::DecodeError::EndOfStream => ReadyError::EndOfStream,
            continuation::DecodeError::InvalidTag => ReadyError::InvalidTag,
            continuation::DecodeError::InvalidChecksum => ReadyError::InvalidChecksum,
            continuation::DecodeError::PayloadNotExhausted => ReadyError::PayloadNotExhausted,
            continuation::DecodeError::UnexpectedRecordTag => ReadyError::UnexpectedRecordTag,
            continuation::DecodeError::ContinuationLimitExceeded => {
                ReadyError::ContinuationLimitExceeded
            }
        }
    }
}

impl From<checkpoint::DecodeError> for ReadyError {
    fn from(err: checkpoint::DecodeError) -> ReadyError {
        match err {
            checkpoint::DecodeError::ReadFailed => ReadyError::ReadFailed,
            checkpoint::DecodeError::EndOfStream => ReadyError::EndOfStream,
            checkpoint::DecodeError::InvalidTag => ReadyError::InvalidTag,
            checkpoint::DecodeError::InvalidChecksum => ReadyError::InvalidChecksum,
            checkpoint::DecodeError::PayloadNotExhausted => ReadyError::PayloadNotExhausted,
            checkpoint::DecodeError::UnexpectedRecordTag => ReadyError::UnexpectedRecordTag,
        }
    }
}

/// One history PAGE record consumed by `next`.
// PORT NOTE: hoisted out of `Decoder` (see `ReadyError`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Progress {
    /// The screen whose HISTORY sequence contained the decoded page.
    pub key: TerminalScreenKey,

    /// Rows prepended above that screen's existing content, or zero
    /// when the page was consumed and validated but dropped.
    pub rows: usize,

    /// PAGE records still pending in the same HISTORY sequence.
    pub remaining: u32,
}

/// Errors possible while decoding history records through FINISH.
// pub const NextError = checkpoint.DecodeError || history.Decoder.InitError ||
//     page.DiscardError || page.DecodeError || Allocator.Error || error{...};
//
// PORT NOTE: hoisted out of `Decoder` (see `ReadyError`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NextError {
    // checkpoint.DecodeError / page.DiscardError
    ReadFailed,
    EndOfStream,
    InvalidTag,
    InvalidChecksum,
    PayloadNotExhausted,
    UnexpectedRecordTag,
    // history.Decoder.InitError
    InvalidKey,
    // page.DecodeError / Allocator.Error
    InvalidDimensions,
    InvalidRowCellCount,
    InvalidKind,
    OutOfMemory,
    InvalidDestinationCapacity,
    Integrity(crate::page::IntegrityError),

    /// `next` was called before `ready` completed successfully.
    DecoderNotReady,

    /// A prior `next` call failed and invalidated the stream position.
    DecoderFailed,

    /// A HISTORY names a key not declared by TERMINAL.
    UnexpectedHistoryKey,

    /// More than one HISTORY names the same key.
    DuplicateHistory,

    /// A history PAGE cannot join a native PageList.
    /// These propagate from `nextPage`'s non-limit finalize errors;
    /// only limit failures are deliberately converted into drops.
    InvalidPageDimensions,
    RowCountOverflow,
    PageSizeOverflow,
}

impl From<checkpoint::DecodeError> for NextError {
    fn from(err: checkpoint::DecodeError) -> NextError {
        match err {
            checkpoint::DecodeError::ReadFailed => NextError::ReadFailed,
            checkpoint::DecodeError::EndOfStream => NextError::EndOfStream,
            checkpoint::DecodeError::InvalidTag => NextError::InvalidTag,
            checkpoint::DecodeError::InvalidChecksum => NextError::InvalidChecksum,
            checkpoint::DecodeError::PayloadNotExhausted => NextError::PayloadNotExhausted,
            checkpoint::DecodeError::UnexpectedRecordTag => NextError::UnexpectedRecordTag,
        }
    }
}

impl From<history::InitError> for NextError {
    fn from(err: history::InitError) -> NextError {
        match err {
            history::InitError::ReadFailed => NextError::ReadFailed,
            history::InitError::EndOfStream => NextError::EndOfStream,
            history::InitError::InvalidKey => NextError::InvalidKey,
            history::InitError::InvalidTag => NextError::InvalidTag,
            history::InitError::InvalidChecksum => NextError::InvalidChecksum,
            history::InitError::PayloadNotExhausted => NextError::PayloadNotExhausted,
            history::InitError::UnexpectedRecordTag => NextError::UnexpectedRecordTag,
        }
    }
}

impl From<page::DiscardError> for NextError {
    fn from(err: page::DiscardError) -> NextError {
        match err {
            page::DiscardError::ReadFailed => NextError::ReadFailed,
            page::DiscardError::EndOfStream => NextError::EndOfStream,
            page::DiscardError::InvalidTag => NextError::InvalidTag,
            page::DiscardError::InvalidChecksum => NextError::InvalidChecksum,
            page::DiscardError::PayloadNotExhausted => NextError::PayloadNotExhausted,
            page::DiscardError::UnexpectedRecordTag => NextError::UnexpectedRecordTag,
        }
    }
}

/// The non-limit arms of `history.DecodePageError` (the two limit errors are
/// converted into drops by `nextPage` before this conversion applies).
fn next_error_from_decode_page(err: history::DecodePageError) -> NextError {
    match err {
        history::DecodePageError::OutOfMemory => NextError::OutOfMemory,
        history::DecodePageError::ReadFailed => NextError::ReadFailed,
        history::DecodePageError::EndOfStream => NextError::EndOfStream,
        history::DecodePageError::InvalidTag => NextError::InvalidTag,
        history::DecodePageError::InvalidChecksum => NextError::InvalidChecksum,
        history::DecodePageError::PayloadNotExhausted => NextError::PayloadNotExhausted,
        history::DecodePageError::UnexpectedRecordTag => NextError::UnexpectedRecordTag,
        history::DecodePageError::InvalidDimensions => NextError::InvalidDimensions,
        history::DecodePageError::InvalidRowCellCount => NextError::InvalidRowCellCount,
        history::DecodePageError::InvalidKind => NextError::InvalidKind,
        history::DecodePageError::InvalidDestinationCapacity => {
            NextError::InvalidDestinationCapacity
        }
        history::DecodePageError::Integrity(err) => NextError::Integrity(err),
        history::DecodePageError::InvalidPageDimensions => NextError::InvalidPageDimensions,
        history::DecodePageError::RowCountOverflow => NextError::RowCountOverflow,
        history::DecodePageError::PageSizeOverflow => NextError::PageSizeOverflow,
        history::DecodePageError::MaxSizeExceeded | history::DecodePageError::MaxLinesExceeded => {
            unreachable!("limit failures are converted into drops by nextPage")
        }
    }
}

impl<'s> Decoder<'s> {
    /// The decoder begins reading only when `ready` is called.
    pub fn init(source: &'s mut dyn io::Reader) -> Decoder<'s> {
        Decoder {
            source,
            state: State::Start,
        }
    }

    /// Decode through the READY marker and return the renderable state.
    ///
    /// The result is transactional and complete for interactive use: the
    /// Terminal, its continuation, and the advisory history extents are all
    /// validated through READY. Scrollback history is not present yet and
    /// arrives through `next`. Asserts this is the first call.
    pub fn ready(
        &mut self,
        alloc: Allocator,
        options: &DecodeOptions,
    ) -> Result<Decoded, ReadyError> {
        assert!(matches!(self.state, State::Start));
        // errdefer self.state = .failed;
        match self.ready_inner(alloc, options) {
            Ok(decoded) => Ok(decoded),
            Err(err) => {
                self.state = State::Failed;
                Err(err)
            }
        }
    }

    fn ready_inner(
        &mut self,
        alloc: Allocator,
        options: &DecodeOptions,
    ) -> Result<Decoded, ReadyError> {
        // Read the envelope, which is currently just a verification step.
        envelope::decode(&mut *self.source)?;

        // TERMINAL establishes terminal-wide state and allocates empty
        // screen slots with their final routing. SCREEN values replace those
        // slots in place so ScreenSet pointers, including the active
        // pointer, stay valid.
        let mut result = terminal::decode(&mut *self.source, alloc)?;
        // errdefer result.deinit(alloc); — explicit on each fallible path.

        // TERMINAL initializes exactly the number of screen slots it
        // declared. Decode that many SCREEN sequences and route each one by
        // its encoded key.
        let screen_count = result.screens.all.iter().filter(|s| s.is_some()).count();
        let screen_options = {
            let primary = result.screens.get(TerminalScreenKey::Primary).unwrap();
            // SAFETY: `primary` is a live screen owned by the freshly decoded
            // terminal; shared reads of its limit policies only.
            let (explicit_bytes, explicit_lines) = unsafe {
                (
                    (*primary).pages.limits.bytes.explicit,
                    (*primary).pages.limits.lines.explicit,
                )
            };
            let cols = result.cols;
            let rows = result.rows;
            // PORT NOTE: `TerminalScreen.Options` is not `Clone`, so the
            // reused reference value becomes a per-iteration constructor. The
            // kitty knobs take the reference struct-literal field defaults
            // (ghostty-artifact; see terminal.rs's decode PORT NOTE).
            move || crate::screen::Options {
                cols,
                rows,
                max_scrollback_bytes: if explicit_bytes == usize::MAX {
                    None
                } else {
                    Some(explicit_bytes)
                },
                max_scrollback_lines: if explicit_lines == usize::MAX {
                    None
                } else {
                    Some(explicit_lines)
                },
                kitty_image_storage_limit: 320 * 1000 * 1000,
                kitty_image_loading_limits: crate::kitty::graphics_image::Limits::DIRECT,
            }
        };

        let mut routed = [false; TerminalScreenKey::COUNT];
        let mut history_rows = HistoryRows::default();
        for _ in 0..screen_count {
            let mut decoded = match screen::decode(&mut *self.source, alloc, screen_options()) {
                Ok(decoded) => decoded,
                Err(err) => {
                    result.deinit(alloc);
                    return Err(err.into());
                }
            };
            // errdefer decoded.deinit(); — explicit on the two routing errors.

            let slot = match result.screens.get(decoded.key) {
                Some(slot) => slot,
                None => {
                    decoded.deinit();
                    result.deinit(alloc);
                    return Err(ReadyError::UnexpectedScreenKey);
                }
            };
            if routed[decoded.key as usize] {
                decoded.deinit();
                result.deinit(alloc);
                return Err(ReadyError::DuplicateScreen);
            }
            routed[decoded.key as usize] = true;
            history_rows.put(decoded.key, decoded.history_rows);

            // slot.deinit();
            // slot.* = decoded.screen;
            // decoded.screen = undefined;
            let screen::Decoded {
                screen: replacement,
                ..
            } = decoded;
            // SAFETY: `slot` is the live screen owned by the set; `deinit`
            // releases its resources and the write installs the decoded
            // replacement. PORT NOTE: unlike the reference assignment, the
            // port must also DROP the deinitialized value — the PageList's
            // pool memory is freed by `Drop`, not by `deinit` (the crate's
            // deinit-then-drop pattern; `ptr::write` alone would leak it).
            unsafe {
                (*slot).deinit();
                drop(core::ptr::read(slot));
                core::ptr::write(slot, replacement);
            }
        }

        // CONTINUATION is required after every active screen. Nonempty bytes
        // are owned locally until the READY prefix validates.
        let decoded_continuation =
            match continuation::decode(&mut *self.source, options.max_continuation_bytes) {
                Ok(value) => value,
                Err(err) => {
                    result.deinit(alloc);
                    return Err(err.into());
                }
            };
        // errdefer: the owned bytes free on drop.

        // READY marks the exact envelope-through-CONTINUATION boundary.
        if let Err(err) = checkpoint::decode(checkpoint::Kind::Ready, &mut *self.source) {
            drop(decoded_continuation);
            result.deinit(alloc);
            return Err(err.into());
        }

        // Record where history may be applied. The declared keys route the
        // HISTORY sequences, and the generations detect declared screens
        // that live terminal use destroys or replaces before their history
        // arrives.
        let mut generations = [None; TerminalScreenKey::COUNT];
        for (index, entry) in result.screens.all.iter().enumerate() {
            if entry.is_some() {
                generations[index] = Some(result.screens.generation(KEYS[index]));
            }
        }

        self.state = State::History(History {
            generations,
            routed: [false; TerminalScreenKey::COUNT],
            pending: screen_count,
            current: None,
            cols: result.cols,
        });
        Ok(Decoded {
            terminal: Some(result),
            continuation: decoded_continuation,
            history_rows,
        })
    }

    /// Decode one history PAGE record and prepend it to its screen in `t`.
    ///
    /// `t` must be the Terminal restored by this decoder's `ready` call,
    /// wherever the caller now stores it. Returns null once FINISH has
    /// validated, which completes the snapshot; the source is
    /// left positioned exactly after it. Further calls return null.
    ///
    /// Wire validation stays strict: framing, CRCs, record order, routing,
    /// and both marker records are always enforced. Application is
    /// forgiving, because the terminal is live and may have changed since
    /// READY. A page is consumed but dropped, reported as zero rows, when:
    ///
    ///   * its screen was removed, or removed and recreated, since READY
    ///     (detected through ScreenSet generations);
    ///   * the terminal was resized since READY, since encoded pages assume
    ///     the encoded width;
    ///   * prepending it would exceed the screen's scrollback limits, e.g.
    ///     because post-cut output already consumed that budget, making
    ///     immediate eviction the only alternative.
    ///
    /// Once a page in a sequence is dropped, the rest of that sequence is
    /// discarded too: later pages are older, and applying one above a gap
    /// would corrupt scrollback ordering. History for other screens is
    /// unaffected.
    pub fn next(
        &mut self,
        alloc: Allocator,
        t: &mut Terminal,
    ) -> Result<Option<Progress>, NextError> {
        match self.state {
            // `ready` must succeed before history exists to decode, and a
            // failed decoder no longer knows its stream position.
            State::Start => return Err(NextError::DecoderNotReady),
            State::Failed => return Err(NextError::DecoderFailed),
            State::Finished => return Ok(None),
            State::History(_) => {}
        }
        // errdefer self.state = .failed;
        match self.next_inner(alloc, t) {
            Ok(progress) => Ok(progress),
            Err(err) => {
                self.state = State::Failed;
                Err(err)
            }
        }
    }

    fn next_inner(
        &mut self,
        alloc: Allocator,
        t: &mut Terminal,
    ) -> Result<Option<Progress>, NextError> {
        loop {
            let state = match &mut self.state {
                State::History(state) => state,
                _ => unreachable!(),
            };

            // Stream the current sequence's next PAGE if one remains.
            if let Some(sequence) = state.current.as_ref() {
                if sequence.remaining > 0 {
                    return Ok(Some(next_page(&mut *self.source, state, alloc, t)?));
                }
                state.current = None;
            }

            // All sequences are consumed: FINISH ends the snapshot, leaving
            // trailing transport bytes unread.
            if state.pending == 0 {
                let generations = state.generations;
                checkpoint::decode(checkpoint::Kind::Finish, &mut *self.source)?;
                if crate::options::SLOW_RUNTIME_SAFETY {
                    for (index, entry) in generations.iter().enumerate() {
                        if entry.is_none() {
                            continue;
                        }
                        let restored = match t.screens.get(KEYS[index]) {
                            Some(restored) => restored,
                            None => continue,
                        };
                        // SAFETY: `restored` is a live screen owned by `t`;
                        // the integrity checks only read.
                        unsafe {
                            (*restored).pages.assert_integrity();
                            (*restored).assert_integrity();
                        }
                    }
                }
                self.state = State::Finished;
                return Ok(None);
            }

            // Begin the next HISTORY sequence. Keys route the sequences
            // order-independently, but each must name a declared screen and
            // may not repeat.
            state.pending -= 1;
            let (key, page_count) = {
                let manifest = history::Decoder::init(&mut *self.source)?;
                (manifest.header.key, manifest.header.page_count)
            };
            if state.generations[key as usize].is_none() {
                return Err(NextError::UnexpectedHistoryKey);
            }
            if state.routed[key as usize] {
                return Err(NextError::DuplicateHistory);
            }
            state.routed[key as usize] = true;
            state.current = Some(Sequence {
                key,
                remaining: page_count,
                apply: true,
            });
        }
    }
}

/// Consume one PAGE record from the current sequence, applying it to
/// the live terminal when it is still applicable and discarding it
/// otherwise.
// PORT NOTE: a free function over the split borrows (`source` + the history
// state) instead of a `&mut self` method — the reference `nextPage` re-derives
// both through `self`, which one Rust `&mut self` cannot express while the
// state is already borrowed by `next`.
fn next_page(
    source: &mut dyn io::Reader,
    state: &mut History,
    alloc: Allocator,
    t: &mut Terminal,
) -> Result<Progress, NextError> {
    // PORT NOTE: the reference threads `alloc` into `history.decodePage`; the
    // port's `decode_page` allocates through the global allocator.
    let _ = alloc;

    // Application is re-decided on every page so live terminal changes
    // between calls degrade to discarding instead of failing decoding.
    let destination: Option<*mut TerminalScreen> = 'destination: {
        let sequence = state.current.as_ref().unwrap();
        if !sequence.apply {
            break 'destination None;
        }
        if t.cols != state.cols {
            break 'destination None;
        }
        let restored = match t.screens.get(sequence.key) {
            Some(restored) => restored,
            None => break 'destination None,
        };
        if t.screens.generation(sequence.key) != state.generations[sequence.key as usize].unwrap() {
            break 'destination None;
        }
        Some(restored)
    };

    let sequence = state.current.as_mut().unwrap();
    sequence.remaining -= 1;
    let rows: usize = 'rows: {
        let restored = match destination {
            Some(restored) => restored,
            None => {
                // The record must still be consumed so decoding remains
                // aligned with the following records and FINISH marker.
                sequence.apply = false;
                page::discard(source)?;
                break 'rows 0;
            }
        };
        // The one-shot history decoder rejects a Screen which already has
        // complete history. This incremental path intentionally bypasses
        // that guard: the terminal has been live since READY and may have
        // accumulated new history. Generation and width checks above keep
        // the destination compatible, while per-page finalization enforces
        // its current scrollback limits.
        //
        // SAFETY: `restored` is a live screen owned by `t`; the exclusive
        // borrow lives only for this decode call.
        match history::decode_page(source, unsafe { &mut *restored }) {
            Ok(rows) => rows,
            // The page no longer fits the screen's scrollback limits,
            // e.g. because post-cut output consumed the budget, so it
            // would be evicted immediately anyway. Its record was fully
            // consumed, leaving the stream aligned on the next record.
            Err(history::DecodePageError::MaxSizeExceeded)
            | Err(history::DecodePageError::MaxLinesExceeded) => {
                sequence.apply = false;
                0
            }
            Err(other) => return Err(next_error_from_decode_page(other)),
        }
    };

    Ok(Progress {
        key: sequence.key,
        rows,
        remaining: sequence.remaining,
    })
}

/// Restore one complete snapshot into a native Terminal and continuation.
///
/// This consumes one snapshot through FINISH and leaves any following bytes
/// in the reader for the containing transport. Restoration is transactional:
/// the returned result is either complete and ready or not (error return).
/// Individual record codecs normalize optional semantic state, and history
/// beyond the declared scrollback limits is dropped rather than rejected,
/// while framing, markers, declared sequence counts, and unique
/// cross-record screen routing remain strict.
///
/// This is the one-shot form of `Decoder`, which can additionally hand the
/// renderable terminal to the caller at READY, before history arrives.
pub fn decode(
    source: &mut dyn io::Reader,
    alloc: Allocator,
    options: &DecodeOptions,
) -> Result<Decoded, DecodeError> {
    let mut decoder = Decoder::init(source);
    let mut result = decoder.ready(alloc, options)?;
    // errdefer result.deinit(alloc);

    // The Terminal has not been transferred, so history applies in place.
    loop {
        let t = result.terminal.as_mut().unwrap();
        match decoder.next(alloc, t) {
            Ok(Some(_)) => continue,
            Ok(None) => break,
            Err(err) => {
                result.deinit(alloc);
                return Err(err.into());
            }
        }
    }
    Ok(result)
}

/// Errors possible while restoring a snapshot that must end at end-of-file.
// pub const DecodeExactError = DecodeError || std.Io.Reader.Error || error{TrailingData};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeExactError {
    Decode(DecodeError),
    /// FINISH was followed by additional bytes.
    TrailingData,
}

impl From<DecodeError> for DecodeExactError {
    fn from(err: DecodeError) -> DecodeExactError {
        DecodeExactError::Decode(err)
    }
}

impl From<io::ReadError> for DecodeExactError {
    fn from(err: io::ReadError) -> DecodeExactError {
        match err {
            io::ReadError::ReadFailed => DecodeExactError::Decode(DecodeError::ReadFailed),
            io::ReadError::EndOfStream => DecodeExactError::Decode(DecodeError::EndOfStream),
        }
    }
}

/// Restore one snapshot and require FINISH to be followed by end-of-file.
///
/// This is intended for bounded snapshot files and buffers. On a live stream,
/// checking for end-of-file may block; use `decode` to stop at FINISH instead.
pub fn decode_exact(
    source: &mut dyn io::Reader,
    alloc: Allocator,
    options: &DecodeOptions,
) -> Result<Decoded, DecodeExactError> {
    let mut result = decode(&mut *source, alloc, options)?;
    // errdefer result.deinit(alloc);

    // _ = source.peekByte() catch |err| switch (err) {
    //     error.EndOfStream => return result,
    //     else => return err,
    // };
    match source.peek_byte() {
        Err(io::ReadError::EndOfStream) => return Ok(result),
        Err(err @ io::ReadError::ReadFailed) => {
            result.deinit(alloc);
            return Err(err.into());
        }
        Ok(_) => {}
    }
    result.deinit(alloc);
    Err(DecodeExactError::TrailingData)
}

#[cfg(test)]
mod tests {
    use super::super::fixture;
    use super::*;
    use crate::page as terminal_page;
    use crate::pagelist::{self, builder::Builder, Node};
    use crate::point;
    use crate::screen::{Cursor as ScreenCursor, CursorStyle, SemanticClick, SemanticPrompt};
    use crate::snapshot::io::Reader as _;
    use crate::snapshot::io::Writer as _;
    use crate::stream;
    use crate::stream_continuation::FixedIoWriter;
    use crate::stream_terminal::{Handler, Stream as TerminalStream};
    use crate::terminal::{Colors as TerminalColors, Options as TerminalOptions};
    use std::borrow::Cow;

    // PORT NOTE: `testing.allocator` for the Stream — the Rust heap handle
    // (the `stream_terminal.rs` test idiom; parity tail S7-7 made
    // `Options.allocator` the runtime handle).
    fn cont_alloc() -> crate::alloc::Allocator {
        crate::alloc::Global
    }

    // const test_encode_options: EncodeOptions = .{ .continuation = .ground };
    fn test_encode_options() -> EncodeOptions<'static> {
        EncodeOptions {
            continuation: Continuation::Ground,
        }
    }

    // const test_decode_options: DecodeOptions = .{ .max_continuation_bytes = 1024 };
    fn test_decode_options() -> DecodeOptions {
        DecodeOptions {
            max_continuation_bytes: 1024,
        }
    }

    // const test_complete_fixture = test_fixture.parse(@embedFile("testdata/complete-v1.hex"));
    fn test_complete_fixture() -> Vec<u8> {
        fixture::parse(include_str!("testdata/complete-v1.hex"))
    }

    // PORT NOTE: the reference `Terminal.init(testing.io, testing.allocator,
    // .{ ... })` relies on `Terminal.Options` field defaults; this helper
    // supplies them for the named literal fields (see terminal.rs's test
    // helper). `max_scrollback_bytes`/`max_scrollback_lines` are always
    // spelled by the callers, matching each reference literal.
    fn terminal_options(
        cols: u16,
        rows: u16,
        max_scrollback_bytes: Option<usize>,
        max_scrollback_lines: Option<usize>,
    ) -> TerminalOptions {
        TerminalOptions {
            cols,
            rows,
            max_scrollback_bytes,
            max_scrollback_lines,
            colors: TerminalColors::default(),
            default_modes: crate::modes::ModePacked::default(),
            default_cursor_style: CursorStyle::Block,
            default_cursor_blink: Some(false),
            kitty_image_storage_limit: 320 * 1000 * 1000,
            kitty_image_loading_limits: crate::kitty::graphics_image::Limits::DIRECT,
        }
    }

    /// The reference `Options` default for `max_scrollback_bytes` (10,000
    /// bytes), for literals that do not name the field.
    const DEFAULT_MSB: Option<usize> = Some(10_000);

    /// `decoded.continuation.bytes` (Zig direct union-payload access).
    fn continuation_bytes<'a>(value: &'a Continuation<'_>) -> &'a [u8] {
        match value {
            Continuation::Bytes(bytes) => bytes,
            Continuation::Ground => panic!("continuation is ground"),
        }
    }

    /// Replace `screen`'s pages with a Builder-built PageList and a tracked
    /// cursor at the active origin — the shared tail of the reference's
    /// in-test Screen replacements (`.{ .io, .alloc, .pages, .cursor }`
    /// struct literal; the remaining fields take their defaults).
    fn replacement_screen(pages: pagelist::PageList) -> TerminalScreen {
        let mut pages = pages;
        let cursor_pin = pages
            .track_pin(
                pages
                    .pin(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
                    .unwrap(),
            )
            .unwrap();
        // SAFETY: `cursor_pin` is a live tracked pin owned by `pages`.
        let cursor_rac = unsafe { (*cursor_pin).row_and_cell() };
        TerminalScreen {
            // .io = testing.io — no counterpart (see snapshot/screen.rs).
            alloc: Allocator,
            pages,
            cursor: ScreenCursor {
                page_pin: cursor_pin,
                page_row: cursor_rac.row,
                page_cell: cursor_rac.cell,
                // (Zig struct-literal defaults for the remaining fields.)
                x: 0,
                y: 0,
                cursor_style: CursorStyle::Block,
                pending_wrap: false,
                protected: false,
                style: crate::style::Style::default(),
                style_id: crate::style::DEFAULT_ID,
                hyperlink_id: 0,
                hyperlink_implicit_id: 0,
                hyperlink: None,
                semantic_content: terminal_page::SemanticContent::Output,
                semantic_content_clear_eol: false,
            },
            // (Zig struct-literal defaults for the remaining fields.)
            no_scrollback: false,
            saved_cursor: None,
            selection: None,
            charset: crate::screen::CharsetState::default(),
            protected_mode: crate::ansi::ProtectedMode::Off,
            kitty_keyboard: crate::kitty::KeyFlagStack::default(),
            kitty_images: crate::kitty::graphics_storage::ImageStorage::default(),
            semantic_prompt: SemanticPrompt {
                seen: false,
                click: SemanticClick::None,
            },
            dirty: crate::screen::Dirty::default(),
        }
    }

    /// Build a 2x3 test terminal whose primary screen carries `history_pages`
    /// complete two-row pages above a three-row active page. The top-left cell
    /// of each page is marked 'A', 'B', ... from oldest history to active.
    fn test_history_terminal(history_pages: u8) -> Terminal {
        let t = Terminal::init(Allocator, terminal_options(2, 3, None, None)).unwrap();
        // errdefer t.deinit(testing.allocator);

        let primary = t.screens.get(TerminalScreenKey::Primary).unwrap();
        let replacement: TerminalScreen = {
            let mut builder = Builder::init(
                crate::alloc::Global,
                pagelist::Options {
                    max_size: None,
                    max_lines: None,
                    ..pagelist::Options::new(t.cols, t.rows)
                },
            )
            .unwrap();
            // defer builder.deinit();

            for index in 0..history_pages {
                let built = builder
                    .allocate_page(terminal_page::Capacity {
                        cols: 2,
                        rows: 2,
                        ..terminal_page::Capacity::default()
                    })
                    .unwrap();
                // SAFETY: `built` is the freshly allocated builder-owned page.
                unsafe {
                    (*built).size.rows = 2;
                    *(*built).get_row_and_cell(0, 0).cell =
                        terminal_page::Cell::init('A' as u32 + index as u32);
                }
            }
            let active = builder
                .allocate_page(terminal_page::Capacity {
                    cols: 2,
                    rows: 3,
                    ..terminal_page::Capacity::default()
                })
                .unwrap();
            // SAFETY: as above.
            unsafe {
                (*active).size.rows = 3;
                *(*active).get_row_and_cell(0, 0).cell =
                    terminal_page::Cell::init('A' as u32 + history_pages as u32);
            }

            let pages = builder.finish().unwrap();
            builder.deinit();
            replacement_screen(pages)
        };
        // primary.deinit(); primary.* = replacement; replacement = undefined;
        //
        // SAFETY: `primary` is the live screen owned by the set; `deinit`
        // releases it, the read-and-drop frees its pool memory (see the
        // `ready` slot-replacement PORT NOTE), and the write installs the
        // replacement.
        unsafe {
            (*primary).deinit();
            drop(core::ptr::read(primary));
            core::ptr::write(primary, replacement);
        }
        t
    }

    /// The top-left screen cell, which lands in the oldest applied history page.
    fn test_top_left_codepoint(terminal_screen: *mut TerminalScreen) -> u32 {
        // SAFETY: `terminal_screen` is a live screen; shared reads of resident
        // page memory only.
        unsafe {
            let node = (*terminal_screen)
                .pages
                .get_top_left(point::Tag::Screen)
                .node;
            (*(*Node::page(node)).get_row_and_cell(0, 0).cell).codepoint()
        }
    }

    /// `std.Io.Writer.Discarding` + `.hashed(record.Crc32c.init(), ...)` —
    /// counts and hashes the stream without retaining it.
    struct HashingDiscardWriter {
        hasher: record::Crc32c,
        full_count: u64,
    }

    impl io::Writer for HashingDiscardWriter {
        fn write_all(&mut self, bytes: &[u8]) -> Result<(), io::WriteFailed> {
            self.hasher.update(bytes);
            self.full_count += bytes.len() as u64;
            Ok(())
        }
    }

    // test "complete snapshot round trip with history and alternate screen"
    #[test]
    fn complete_snapshot_round_trip_with_history_and_alternate_screen() {
        let test_complete_fixture = test_complete_fixture();

        let mut t = Terminal::init(Allocator, terminal_options(2, 3, None, None)).unwrap();
        // defer t.deinit(testing.allocator);

        // Exercise terminal-wide state.
        t.width_px = 800;
        t.height_px = 600;
        t.colors
            .palette
            .set(7, crate::color::RGB { r: 1, g: 2, b: 3 });
        t.modes
            .values
            .set_mode(crate::modes::Mode::BracketedPaste, true);
        t.set_pwd(b"file:///tmp/snapshot").unwrap();
        t.set_title(b"complete snapshot").unwrap();

        let primary = t.screens.get(TerminalScreenKey::Primary).unwrap();

        // Use small exact capacities so this compound golden remains practical
        // to review while still containing two complete history pages and one
        // active page. Replacing the Screen in place preserves ScreenSet
        // routing.
        let replacement: TerminalScreen = {
            let mut builder = Builder::init(
                crate::alloc::Global,
                pagelist::Options {
                    max_size: None,
                    max_lines: None,
                    ..pagelist::Options::new(t.cols, t.rows)
                },
            )
            .unwrap();
            // defer builder.deinit();

            let oldest = builder
                .allocate_page(terminal_page::Capacity {
                    cols: 2,
                    rows: 2,
                    ..terminal_page::Capacity::default()
                })
                .unwrap();
            // SAFETY (each block below): freshly allocated builder-owned pages.
            unsafe {
                (*oldest).size.rows = 2;
                *(*oldest).get_row_and_cell(0, 0).cell = terminal_page::Cell::init('A' as u32);
            }

            let recent = builder
                .allocate_page(terminal_page::Capacity {
                    cols: 2,
                    rows: 2,
                    ..terminal_page::Capacity::default()
                })
                .unwrap();
            unsafe {
                (*recent).size.rows = 2;
                *(*recent).get_row_and_cell(0, 0).cell = terminal_page::Cell::init('B' as u32);
            }

            let active = builder
                .allocate_page(terminal_page::Capacity {
                    cols: 2,
                    rows: 3,
                    ..terminal_page::Capacity::default()
                })
                .unwrap();
            unsafe {
                (*active).size.rows = 3;
                *(*active).get_row_and_cell(0, 0).cell = terminal_page::Cell::init('C' as u32);
                *(*active).get_row_and_cell(0, 1).cell = terminal_page::Cell::init('D' as u32);
                *(*active).get_row_and_cell(0, 2).cell = terminal_page::Cell::init('E' as u32);
            }

            let pages = builder.finish().unwrap();
            builder.deinit();
            replacement_screen(pages)
        };
        // SAFETY: see `test_history_terminal`.
        unsafe {
            (*primary).deinit();
            drop(core::ptr::read(primary));
            core::ptr::write(primary, replacement);
        }

        // SAFETY (all `primary` derefs below): live screen owned by `t`.
        assert!(unsafe { (*primary).pages.scrollbar().total } > t.rows as usize);

        // Compression is an internal source representation and must remain
        // unchanged while the complete history is inspected for encoding.
        let _ = unsafe {
            (*primary)
                .pages
                .compress(pagelist::compress::CompressionMode::Full)
        };
        let source_memory = unsafe { (*primary).pages.memory_stats() };

        // The optional alternate screen participates in both phases and
        // remains the active screen after restoration.
        t.switch_screen(TerminalScreenKey::Alternate).unwrap();
        t.print_string(b"alternate").unwrap();

        let mut encoded = io::AllocatingWriter::new();
        encode(&mut encoded, &t, test_encode_options()).unwrap();
        assert_eq!(source_memory, unsafe { (*primary).pages.memory_stats() });
        fixture::expect_equal(
            fixture::Kind::Snapshot,
            "src/snapshot/testdata/complete-v1.hex",
            "snapshot_fixture-complete-v1.hex",
            &test_complete_fixture,
            encoded.written(),
        );

        // A complete snapshot can stream through a non-allocating destination.
        // Independently hash that output so both its length and complete byte
        // sequence are checked without retaining a second snapshot copy.
        let mut hashing = HashingDiscardWriter {
            hasher: record::Crc32c::init(),
            full_count: 0,
        };
        encode(&mut hashing, &t, test_encode_options()).unwrap();
        assert_eq!(test_complete_fixture.len() as u64, hashing.full_count);
        assert_eq!(
            record::Crc32c::hash(&test_complete_fixture),
            hashing.hasher.r#final(),
        );

        // Restore the checked-in reference rather than the just-generated
        // bytes.
        let mut encoded_source = io::FixedReader::new(&test_complete_fixture);
        let mut source_buffer = [0u8; 1];
        let mut limited = io::LimitedReader::new(
            &mut encoded_source,
            io::Limit::Unlimited,
            &mut source_buffer,
        );
        let mut restored = decode(&mut limited, Allocator, &test_decode_options()).unwrap();
        // defer restored.deinit(testing.allocator);
        {
            let restored_terminal = restored.terminal.as_mut().unwrap();

            assert_eq!(
                TerminalScreenKey::Alternate,
                restored_terminal.screens.active_key,
            );
            assert_eq!(
                restored_terminal
                    .screens
                    .get(TerminalScreenKey::Alternate)
                    .unwrap(),
                restored_terminal.screens.active,
            );
            assert_eq!(
                b"file:///tmp/snapshot".as_slice(),
                restored_terminal.get_pwd().unwrap(),
            );
            assert_eq!(
                b"complete snapshot".as_slice(),
                restored_terminal.get_title().unwrap(),
            );
            let restored_primary = restored_terminal
                .screens
                .get(TerminalScreenKey::Primary)
                .unwrap();
            // SAFETY: live screens owned by their terminals.
            unsafe {
                assert_eq!(
                    (*primary).pages.scrollbar().total,
                    (*restored_primary).pages.scrollbar().total,
                );
            }

            // Re-encoding is a compact semantic equality check over all
            // TERMINAL, SCREEN, PAGE, and HISTORY fields and both marker
            // boundaries.
            let mut reencoded = io::AllocatingWriter::new();
            encode(&mut reencoded, restored_terminal, test_encode_options()).unwrap();
            assert_eq!(test_complete_fixture.as_slice(), reencoded.written());
        }
        restored.deinit(Allocator);

        // SCREEN and HISTORY keys make both sequence groups order independent.
        let mut reversed = io::AllocatingWriter::new();
        {
            let mut reversed_stream = record::Writer::init(&mut reversed);
            // defer reversed_stream.deinit();
            envelope::encode(reversed_stream.writer()).unwrap();
            terminal::encode(&t, &mut reversed_stream).unwrap();
            let alternate = t.screens.get(TerminalScreenKey::Alternate).unwrap();
            // SAFETY: live screens owned by `t`; encode only reads.
            unsafe {
                screen::encode(
                    &*alternate,
                    TerminalScreenKey::Alternate,
                    &mut reversed_stream,
                )
                .unwrap();
                screen::encode(&*primary, TerminalScreenKey::Primary, &mut reversed_stream)
                    .unwrap();
            }
            continuation::encode(&Continuation::Ground, &mut reversed_stream).unwrap();
            checkpoint::encode(checkpoint::Kind::Ready, &mut reversed_stream).unwrap();
            // SAFETY: as above.
            unsafe {
                history::encode(
                    &*alternate,
                    TerminalScreenKey::Alternate,
                    &mut reversed_stream,
                )
                .unwrap();
                history::encode(&*primary, TerminalScreenKey::Primary, &mut reversed_stream)
                    .unwrap();
            }
            checkpoint::encode(checkpoint::Kind::Finish, &mut reversed_stream).unwrap();
            reversed_stream.deinit();
        }

        let reversed_written = reversed.written().to_vec();
        let mut reversed_source = io::FixedReader::new(&reversed_written);
        let mut reversed_restored =
            decode(&mut reversed_source, Allocator, &test_decode_options()).unwrap();
        // defer reversed_restored.deinit(testing.allocator);
        {
            let reversed_terminal = reversed_restored.terminal.as_ref().unwrap();
            assert_eq!(
                TerminalScreenKey::Alternate,
                reversed_terminal.screens.active_key,
            );
            assert_eq!(
                0usize,
                reversed_terminal
                    .screens
                    .generation(TerminalScreenKey::Primary),
            );
            assert_eq!(
                0usize,
                reversed_terminal
                    .screens
                    .generation(TerminalScreenKey::Alternate),
            );
        }
        reversed_restored.deinit(Allocator);

        t.deinit(Allocator);
    }

    // test "complete snapshot restores a canonical Stream continuation"
    #[test]
    fn complete_snapshot_restores_a_canonical_stream_continuation() {
        let mut source_terminal =
            Terminal::init(Allocator, terminal_options(8, 2, DEFAULT_MSB, None)).unwrap();
        // defer source_terminal.deinit(testing.allocator);
        let mut source_stream = TerminalStream::init(stream::Options {
            allocator: Some(cont_alloc()),
            handler: Handler::init(&mut source_terminal as *mut Terminal),
            continuation_max_bytes: Some(1024),
        });
        // defer source_stream.deinit();

        source_stream.next_slice(b"A\x1b[31");
        let mut exported_bytes = [0u8; 1024];
        let mut exported = FixedIoWriter::new(&mut exported_bytes);
        source_stream.write_continuation(&mut exported).unwrap();
        assert_eq!(b"\x1b[31".as_slice(), exported.buffered());

        let mut encoded = io::AllocatingWriter::new();
        encode(
            &mut encoded,
            &source_terminal,
            EncodeOptions {
                continuation: Continuation::Bytes(Cow::Borrowed(exported.buffered())),
            },
        )
        .unwrap();

        let encoded_written = encoded.written().to_vec();
        let mut encoded_source = io::FixedReader::new(&encoded_written);
        let mut decoded = decode(
            &mut encoded_source,
            Allocator,
            &DecodeOptions {
                max_continuation_bytes: 1024,
            },
        )
        .unwrap();
        // defer decoded.deinit(testing.allocator);
        assert_eq!(
            exported.buffered(),
            continuation_bytes(&decoded.continuation),
        );

        let mut restored_terminal = decoded.to_owned();
        // defer restored_terminal.deinit(testing.allocator);
        assert!(decoded.terminal.is_none());
        let mut restored_stream = TerminalStream::init(stream::Options {
            allocator: Some(cont_alloc()),
            handler: Handler::init(&mut restored_terminal as *mut Terminal),
            continuation_max_bytes: Some(1024),
        });
        // defer restored_stream.deinit();

        restored_stream.next_slice(continuation_bytes(&decoded.continuation));
        let mut reexported_bytes = [0u8; 1024];
        let mut reexported = FixedIoWriter::new(&mut reexported_bytes);
        restored_stream.write_continuation(&mut reexported).unwrap();
        assert_eq!(exported.buffered(), reexported.buffered());

        // The identical post-cut bytes may use different feed chunking without
        // changing terminal semantics.
        source_stream.next_slice(b"mB");
        restored_stream.next(b'm');
        restored_stream.next(b'B');
        let source_text = source_terminal.plain_string(Allocator).unwrap();
        // defer testing.allocator.free(source_text);
        let restored_text = restored_terminal.plain_string(Allocator).unwrap();
        // defer testing.allocator.free(restored_text);
        assert_eq!(source_text, restored_text);
        // SAFETY: live active screens owned by their terminals; shared reads.
        unsafe {
            assert_eq!(
                (*source_terminal.screens.active).cursor.style_id,
                (*restored_terminal.screens.active).cursor.style_id,
            );
        }

        restored_stream.deinit();
        source_stream.deinit();
        decoded.deinit(Allocator);
        restored_terminal.deinit(Allocator);
        source_terminal.deinit(Allocator);
    }

    // test "complete snapshot validates continuation before writing"
    // (the post-encode `tail[2] = '4'` write is deliberately never read back:
    // it proves the encoder consumed the bytes before the mutation)
    #[allow(unused_assignments)]
    #[test]
    fn complete_snapshot_validates_continuation_before_writing() {
        let mut t = Terminal::init(Allocator, terminal_options(2, 1, DEFAULT_MSB, None)).unwrap();
        // defer t.deinit(testing.allocator);

        let mut destination = io::AllocatingWriter::new();
        destination.write_all(b"prefix").unwrap();
        assert_eq!(
            Err(EncodeError::NoPendingState),
            encode(
                &mut destination,
                &t,
                EncodeOptions {
                    continuation: Continuation::Bytes(Cow::Borrowed(b"\x1b[31m")),
                },
            ),
        );
        assert_eq!(b"prefix".as_slice(), destination.written());

        let mut tail = [0x1bu8, b'[', b'3', b'1'];
        let mut encoded = io::AllocatingWriter::new();
        encode(
            &mut encoded,
            &t,
            EncodeOptions {
                continuation: Continuation::Bytes(Cow::Borrowed(&tail)),
            },
        )
        .unwrap();
        tail[2] = b'4';

        let encoded_written = encoded.written().to_vec();
        let mut source = io::FixedReader::new(&encoded_written);
        let mut decoded = decode(&mut source, Allocator, &test_decode_options()).unwrap();
        // defer decoded.deinit(testing.allocator);
        assert_eq!(
            b"\x1b[31".as_slice(),
            continuation_bytes(&decoded.continuation),
        );

        decoded.deinit(Allocator);
        t.deinit(Allocator);
    }

    // test "complete snapshot waits for continuation tracking recovery"
    #[test]
    fn complete_snapshot_waits_for_continuation_tracking_recovery() {
        let mut t = Terminal::init(Allocator, terminal_options(2, 1, DEFAULT_MSB, None)).unwrap();
        // defer t.deinit(testing.allocator);
        let mut stream_value = TerminalStream::init(stream::Options {
            allocator: Some(cont_alloc()),
            handler: Handler::init(&mut t as *mut Terminal),
            continuation_max_bytes: Some(4),
        });
        // defer stream.deinit();

        stream_value.next_slice(b"\x1b[123");
        let mut unavailable_bytes = [0u8; 4];
        let mut unavailable = FixedIoWriter::new(&mut unavailable_bytes);
        assert!(matches!(
            stream_value.write_continuation(&mut unavailable),
            Err(crate::stream::WriteContinuationError::ContinuationUnavailable),
        ));

        // A new replay start replaces the lost suffix and makes a later cut
        // publishable without affecting normal terminal parsing.
        stream_value.next_slice(b"\x1b[");
        let mut recovered_bytes = [0u8; 4];
        let mut recovered = FixedIoWriter::new(&mut recovered_bytes);
        stream_value.write_continuation(&mut recovered).unwrap();
        assert_eq!(b"\x1b[".as_slice(), recovered.buffered());

        let mut encoded = io::AllocatingWriter::new();
        encode(
            &mut encoded,
            &t,
            EncodeOptions {
                continuation: Continuation::Bytes(Cow::Borrowed(recovered.buffered())),
            },
        )
        .unwrap();

        stream_value.deinit();
        t.deinit(Allocator);
    }

    // test "complete snapshot preserves every supported continuation cut"
    #[test]
    fn complete_snapshot_preserves_every_supported_continuation_cut() {
        let corpora: [&[u8]; 10] = [
            b"plain \xF0\x9F\x98\x84 utf8",
            b"bad \xE0\xA0\xF0\x9F\x98\x84 utf8",
            b"\x1b[1\x07;2mstyled\x1b[0m",
            b"\x1b]2;window title\x1b\\text",
            b"\x1bP$qm\x1b\\text",
            b"\x1b_Ga=q;payload\x1b\\text",
            b"\x1b_25a1;s\x1b\\text",
            b"\x1b]2;first\x1b\\\x1b_Gsecond",
            b"\x1b[12\x9D2;title\x1b\\text",
            b"\x1b[12\x18text\x1b[1\x1Atext",
        ];

        for corpus in corpora {
            for cut in 0..corpus.len() + 1 {
                let mut source_terminal =
                    Terminal::init(Allocator, terminal_options(20, 4, DEFAULT_MSB, None)).unwrap();
                // defer source_terminal.deinit(testing.allocator);
                let mut source_stream = TerminalStream::init(stream::Options {
                    allocator: Some(cont_alloc()),
                    handler: Handler::init(&mut source_terminal as *mut Terminal),
                    continuation_max_bytes: Some(4096),
                });
                // defer source_stream.deinit();
                source_stream.next_slice(&corpus[0..cut]);

                let mut cut_bytes = [0u8; 4096];
                let mut cut_writer = FixedIoWriter::new(&mut cut_bytes);
                source_stream.write_continuation(&mut cut_writer).unwrap();
                let cut_continuation: Continuation<'_> = if cut_writer.end == 0 {
                    Continuation::Ground
                } else {
                    Continuation::Bytes(Cow::Borrowed(cut_writer.buffered()))
                };

                let mut snapshot_bytes = io::AllocatingWriter::new();
                encode(
                    &mut snapshot_bytes,
                    &source_terminal,
                    EncodeOptions {
                        continuation: cut_continuation,
                    },
                )
                .unwrap();

                let snapshot_written = snapshot_bytes.written().to_vec();
                let mut snapshot_source = io::FixedReader::new(&snapshot_written);
                let mut decoded = decode(
                    &mut snapshot_source,
                    Allocator,
                    &DecodeOptions {
                        max_continuation_bytes: 4096,
                    },
                )
                .unwrap();
                // defer decoded.deinit(testing.allocator);
                let mut restored_terminal = decoded.to_owned();
                // defer restored_terminal.deinit(testing.allocator);
                let mut restored_stream = TerminalStream::init(stream::Options {
                    allocator: Some(cont_alloc()),
                    handler: Handler::init(&mut restored_terminal as *mut Terminal),
                    continuation_max_bytes: Some(4096),
                });
                // defer restored_stream.deinit();
                match &decoded.continuation {
                    Continuation::Ground => {}
                    Continuation::Bytes(bytes) => restored_stream.next_slice(bytes),
                }

                let mut reexport_bytes = [0u8; 4096];
                let mut reexport_writer = FixedIoWriter::new(&mut reexport_bytes);
                restored_stream
                    .write_continuation(&mut reexport_writer)
                    .unwrap();
                assert_eq!(cut_writer.buffered(), reexport_writer.buffered());

                source_stream.next_slice(&corpus[cut..]);
                let mut offset = cut;
                let mut partition = cut.wrapping_add(corpus.len()).wrapping_add(1);
                while offset < corpus.len() {
                    partition = partition.wrapping_mul(1664525).wrapping_add(1013904223);
                    let len = (1 + partition % 7).min(corpus.len() - offset);
                    restored_stream.next_slice(&corpus[offset..offset + len]);
                    offset += len;
                }

                let mut source_final_bytes = [0u8; 4096];
                let mut source_final_writer = FixedIoWriter::new(&mut source_final_bytes);
                source_stream
                    .write_continuation(&mut source_final_writer)
                    .unwrap();
                let mut restored_final_bytes = [0u8; 4096];
                let mut restored_final_writer = FixedIoWriter::new(&mut restored_final_bytes);
                restored_stream
                    .write_continuation(&mut restored_final_writer)
                    .unwrap();
                assert_eq!(
                    source_final_writer.buffered(),
                    restored_final_writer.buffered(),
                );

                let mut source_final_snapshot = io::AllocatingWriter::new();
                encode(
                    &mut source_final_snapshot,
                    &source_terminal,
                    EncodeOptions {
                        continuation: if source_final_writer.end == 0 {
                            Continuation::Ground
                        } else {
                            Continuation::Bytes(Cow::Borrowed(source_final_writer.buffered()))
                        },
                    },
                )
                .unwrap();
                let mut restored_final_snapshot = io::AllocatingWriter::new();
                encode(
                    &mut restored_final_snapshot,
                    &restored_terminal,
                    EncodeOptions {
                        continuation: if source_final_writer.end == 0 {
                            Continuation::Ground
                        } else {
                            Continuation::Bytes(Cow::Borrowed(source_final_writer.buffered()))
                        },
                    },
                )
                .unwrap();
                assert_eq!(
                    source_final_snapshot.written(),
                    restored_final_snapshot.written(),
                );

                restored_stream.deinit();
                source_stream.deinit();
                decoded.deinit(Allocator);
                restored_terminal.deinit(Allocator);
                source_terminal.deinit(Allocator);
            }
        }
    }

    // test "complete snapshot preserves Kitty virtual placeholders"
    #[test]
    #[cfg_attr(
        not(all(feature = "kitty-graphics", not(target_os = "none"))),
        ignore = "SkipZigTest: needs options::KITTY_GRAPHICS (R14)"
    )]
    fn complete_snapshot_preserves_kitty_virtual_placeholders() {
        // if (comptime !build_options.kitty_graphics) return error.SkipZigTest;

        let mut t = Terminal::init(Allocator, terminal_options(2, 1, DEFAULT_MSB, None)).unwrap();
        // defer t.deinit(testing.allocator);

        // Register a real virtual placement, then write its grid
        // representation: U+10EEEE followed by row and column diacritics. The
        // image and placement registry is intentionally omitted, but the grid
        // content must remain decodable.
        //
        // PORT NOTE: the reference calls the storage methods through
        // `t.screens.active.kitty_images`; the port moves the storage out and
        // back around each `*mut Screen`-taking call (the `Screen::init`
        // idiom; see snapshot/screen.rs's kitty PORT NOTE).
        let active = t.screens.active;
        // SAFETY: `active` is the live active screen owned by `t`.
        unsafe {
            let mut images = core::mem::take(&mut (*active).kitty_images);
            images
                .add_image(
                    Allocator,
                    active,
                    crate::kitty::graphics_image::Image {
                        id: 1,
                        ..crate::kitty::graphics_image::Image::default()
                    },
                )
                .unwrap();
            images
                .add_placement(
                    Allocator,
                    active,
                    1,
                    0,
                    crate::kitty::graphics_storage::Placement {
                        location: crate::kitty::graphics_storage::Location::Virtual,
                        columns: 1,
                        rows: 1,
                        ..crate::kitty::graphics_storage::Placement::with_location(
                            crate::kitty::graphics_storage::Location::Virtual,
                        )
                    },
                )
                .unwrap();
            (*active).kitty_images = images;
        }
        t.set_attribute(crate::sgr::Attribute::Color256Fg(1))
            .unwrap();
        t.print_string("\u{10EEEE}\u{0305}\u{0305}".as_bytes())
            .unwrap();
        // SAFETY: live active screen owned by `t`; shared reads only.
        let source_cell = unsafe {
            (*t.screens.active)
                .pages
                .get_cell(point::Point::Screen(point::Coordinate { x: 0, y: 0 }))
                .unwrap()
        };
        // SAFETY: live page memory of the active screen; shared reads only.
        unsafe {
            assert_eq!(
                crate::kitty::graphics_unicode::PLACEHOLDER,
                (*source_cell.cell).codepoint(),
            );
            assert!((*source_cell.row).kitty_virtual_placeholder());
        }

        let mut encoded = io::AllocatingWriter::new();
        encode(&mut encoded, &t, test_encode_options()).unwrap();

        let encoded_written = encoded.written().to_vec();
        let mut encoded_source = io::FixedReader::new(&encoded_written);
        let mut restored = decode(&mut encoded_source, Allocator, &test_decode_options()).unwrap();
        // defer restored.deinit(testing.allocator);
        {
            let restored_terminal = restored.terminal.as_mut().unwrap();

            // SAFETY: live active screen of the restored terminal; reads only.
            unsafe {
                let restored_cell = (*restored_terminal.screens.active)
                    .pages
                    .get_cell(point::Point::Screen(point::Coordinate { x: 0, y: 0 }))
                    .unwrap();
                assert_eq!(
                    crate::kitty::graphics_unicode::PLACEHOLDER,
                    (*restored_cell.cell).codepoint(),
                );
                assert!((*restored_cell.cell).has_grapheme());
                assert!((*restored_cell.row).kitty_virtual_placeholder());
                assert_eq!(
                    0usize,
                    (*restored_terminal.screens.active)
                        .kitty_images
                        .images
                        .len(),
                );
                assert_eq!(
                    0usize,
                    (*restored_terminal.screens.active)
                        .kitty_images
                        .placements
                        .len(),
                );
            }
        }
        restored.deinit(Allocator);
        t.deinit(Allocator);
    }

    // test "complete snapshot encoding streams from the current writer position"
    #[test]
    fn complete_snapshot_encoding_streams_from_the_current_writer_position() {
        let mut t = Terminal::init(Allocator, terminal_options(2, 1, DEFAULT_MSB, None)).unwrap();
        // defer t.deinit(testing.allocator);

        // Encoding begins with this call's envelope, independent of bytes that
        // were already present in the destination.
        let mut nonempty = io::AllocatingWriter::new();
        nonempty.write_all(b"prefix").unwrap();
        let snapshot_offset = nonempty.written().len();
        encode(&mut nonempty, &t, test_encode_options()).unwrap();
        assert_eq!(
            b"prefix".as_slice(),
            &nonempty.written()[0..snapshot_offset]
        );
        let appended_written = nonempty.written()[snapshot_offset..].to_vec();
        let mut appended_source = io::FixedReader::new(&appended_written);
        let mut appended = decode(&mut appended_source, Allocator, &test_decode_options()).unwrap();
        appended.deinit(Allocator);

        // Payload validation happens in the record-local scratch allocation.
        // The already-streamed envelope remains, but no partial TERMINAL is
        // emitted.
        t.colors.palette.current[7] = crate::color::RGB { r: 1, g: 2, b: 3 };
        let mut destination = io::AllocatingWriter::new();
        destination.write_all(b"prefix").unwrap();
        assert_eq!(
            Err(EncodeError::InvalidPalette),
            encode(&mut destination, &t, test_encode_options()),
        );
        let mut expected_envelope = [0u8; envelope::ENCODED_LEN];
        let mut envelope_writer = io::FixedWriter::new(&mut expected_envelope);
        envelope::encode(&mut envelope_writer).unwrap();
        assert_eq!(
            expected_envelope.as_slice(),
            &destination.written()[b"prefix".len()..],
        );

        t.deinit(Allocator);
    }

    // test "complete snapshot rejects ordering and invalid markers"
    #[test]
    fn complete_snapshot_rejects_ordering_and_invalid_markers() {
        let mut t = Terminal::init(Allocator, terminal_options(2, 1, DEFAULT_MSB, None)).unwrap();
        // defer t.deinit(testing.allocator);
        let primary = t.screens.get(TerminalScreenKey::Primary).unwrap();

        // Old version-1 snapshots proceeded directly from SCREEN to READY.
        // READY is individually valid here, but CONTINUATION is now required
        // first.
        let mut old_order = io::AllocatingWriter::new();
        {
            let mut old_order_stream = record::Writer::init(&mut old_order);
            // defer old_order_stream.deinit();
            envelope::encode(old_order_stream.writer()).unwrap();
            terminal::encode(&t, &mut old_order_stream).unwrap();
            // SAFETY: live screen owned by `t`; encode only reads.
            unsafe {
                screen::encode(&*primary, TerminalScreenKey::Primary, &mut old_order_stream)
                    .unwrap();
            }
            checkpoint::encode(checkpoint::Kind::Ready, &mut old_order_stream).unwrap();
            old_order_stream.deinit();
        }
        let old_order_written = old_order.written().to_vec();
        let mut old_order_source = io::FixedReader::new(&old_order_written);
        assert_eq!(
            Err(DecodeError::UnexpectedRecordTag),
            decode(&mut old_order_source, Allocator, &test_decode_options())
                .map(|mut d| d.deinit(Allocator)),
        );

        // A second CONTINUATION is rejected where READY is required.
        let mut duplicate_continuation = io::AllocatingWriter::new();
        {
            let mut duplicate_continuation_stream =
                record::Writer::init(&mut duplicate_continuation);
            // defer duplicate_continuation_stream.deinit();
            envelope::encode(duplicate_continuation_stream.writer()).unwrap();
            terminal::encode(&t, &mut duplicate_continuation_stream).unwrap();
            // SAFETY: as above.
            unsafe {
                screen::encode(
                    &*primary,
                    TerminalScreenKey::Primary,
                    &mut duplicate_continuation_stream,
                )
                .unwrap();
            }
            continuation::encode(&Continuation::Ground, &mut duplicate_continuation_stream)
                .unwrap();
            continuation::encode(&Continuation::Ground, &mut duplicate_continuation_stream)
                .unwrap();
            duplicate_continuation_stream.deinit();
        }
        let duplicate_continuation_written = duplicate_continuation.written().to_vec();
        let mut duplicate_continuation_source =
            io::FixedReader::new(&duplicate_continuation_written);
        assert_eq!(
            Err(DecodeError::UnexpectedRecordTag),
            decode(
                &mut duplicate_continuation_source,
                Allocator,
                &test_decode_options(),
            )
            .map(|mut d| d.deinit(Allocator)),
        );

        // HISTORY is individually valid here, but the full decoder requires
        // the primary SCREEN before READY.
        let mut reordered = io::AllocatingWriter::new();
        {
            let mut reordered_stream = record::Writer::init(&mut reordered);
            // defer reordered_stream.deinit();
            envelope::encode(reordered_stream.writer()).unwrap();
            terminal::encode(&t, &mut reordered_stream).unwrap();
            // SAFETY: as above.
            unsafe {
                history::encode(&*primary, TerminalScreenKey::Primary, &mut reordered_stream)
                    .unwrap();
            }
            reordered_stream.deinit();
        }
        let reordered_written = reordered.written().to_vec();
        let mut reordered_source = io::FixedReader::new(&reordered_written);
        assert_eq!(
            Err(DecodeError::UnexpectedRecordTag),
            decode(&mut reordered_source, Allocator, &test_decode_options())
                .map(|mut d| d.deinit(Allocator)),
        );

        // Construct a correctly framed READY with a nonempty payload so the
        // full driver rejects a marker that does not match the required empty
        // shape.
        let mut invalid_ready = io::AllocatingWriter::new();
        {
            let mut invalid_ready_stream = record::Writer::init(&mut invalid_ready);
            // defer invalid_ready_stream.deinit();
            envelope::encode(invalid_ready_stream.writer()).unwrap();
            terminal::encode(&t, &mut invalid_ready_stream).unwrap();
            // SAFETY: as above.
            unsafe {
                screen::encode(
                    &*primary,
                    TerminalScreenKey::Primary,
                    &mut invalid_ready_stream,
                )
                .unwrap();
            }
            continuation::encode(&Continuation::Ground, &mut invalid_ready_stream).unwrap();
            let ready_payload = invalid_ready_stream.begin(record::Tag::Ready);
            // errdefer invalid_ready_stream.cancel();
            ready_payload.write_byte(0).unwrap();
            invalid_ready_stream.finish().unwrap();
            invalid_ready_stream.deinit();
        }
        let invalid_ready_written = invalid_ready.written().to_vec();
        let mut invalid_ready_source = io::FixedReader::new(&invalid_ready_written);
        assert_eq!(
            Err(DecodeError::PayloadNotExhausted),
            decode(&mut invalid_ready_source, Allocator, &test_decode_options())
                .map(|mut d| d.deinit(Allocator)),
        );

        // A SCREEN key must name one of the slots declared by TERMINAL.
        let mut undeclared = io::AllocatingWriter::new();
        {
            let mut undeclared_stream = record::Writer::init(&mut undeclared);
            // defer undeclared_stream.deinit();
            envelope::encode(undeclared_stream.writer()).unwrap();
            terminal::encode(&t, &mut undeclared_stream).unwrap();
            // SAFETY: as above.
            unsafe {
                screen::encode(
                    &*primary,
                    TerminalScreenKey::Alternate,
                    &mut undeclared_stream,
                )
                .unwrap();
            }
            undeclared_stream.deinit();
        }
        let undeclared_written = undeclared.written().to_vec();
        let mut undeclared_source = io::FixedReader::new(&undeclared_written);
        assert_eq!(
            Err(DecodeError::UnexpectedScreenKey),
            decode(&mut undeclared_source, Allocator, &test_decode_options())
                .map(|mut d| d.deinit(Allocator)),
        );

        // HISTORY sequences are also routed by key, which must name a declared
        // screen even when the sequence contains no PAGE records.
        let mut undeclared_history = io::AllocatingWriter::new();
        {
            let mut undeclared_history_stream = record::Writer::init(&mut undeclared_history);
            // defer undeclared_history_stream.deinit();
            envelope::encode(undeclared_history_stream.writer()).unwrap();
            terminal::encode(&t, &mut undeclared_history_stream).unwrap();
            // SAFETY: as above.
            unsafe {
                screen::encode(
                    &*primary,
                    TerminalScreenKey::Primary,
                    &mut undeclared_history_stream,
                )
                .unwrap();
            }
            continuation::encode(&Continuation::Ground, &mut undeclared_history_stream).unwrap();
            checkpoint::encode(checkpoint::Kind::Ready, &mut undeclared_history_stream).unwrap();
            // SAFETY: as above.
            unsafe {
                history::encode(
                    &*primary,
                    TerminalScreenKey::Alternate,
                    &mut undeclared_history_stream,
                )
                .unwrap();
            }
            undeclared_history_stream.deinit();
        }
        let undeclared_history_written = undeclared_history.written().to_vec();
        let mut undeclared_history_source = io::FixedReader::new(&undeclared_history_written);
        assert_eq!(
            Err(DecodeError::UnexpectedHistoryKey),
            decode(
                &mut undeclared_history_source,
                Allocator,
                &test_decode_options(),
            )
            .map(|mut d| d.deinit(Allocator)),
        );

        // The declared count cannot be satisfied by repeating the same key.
        t.switch_screen(TerminalScreenKey::Alternate).unwrap();
        let mut duplicate = io::AllocatingWriter::new();
        {
            let mut duplicate_stream = record::Writer::init(&mut duplicate);
            // defer duplicate_stream.deinit();
            envelope::encode(duplicate_stream.writer()).unwrap();
            terminal::encode(&t, &mut duplicate_stream).unwrap();
            // SAFETY: as above.
            unsafe {
                screen::encode(&*primary, TerminalScreenKey::Primary, &mut duplicate_stream)
                    .unwrap();
                screen::encode(&*primary, TerminalScreenKey::Primary, &mut duplicate_stream)
                    .unwrap();
            }
            duplicate_stream.deinit();
        }
        let duplicate_written = duplicate.written().to_vec();
        let mut duplicate_source = io::FixedReader::new(&duplicate_written);
        assert_eq!(
            Err(DecodeError::DuplicateScreen),
            decode(&mut duplicate_source, Allocator, &test_decode_options())
                .map(|mut d| d.deinit(Allocator)),
        );

        // The declared count cannot be satisfied by repeating one HISTORY key.
        let mut duplicate_history = io::AllocatingWriter::new();
        {
            let mut duplicate_history_stream = record::Writer::init(&mut duplicate_history);
            // defer duplicate_history_stream.deinit();
            envelope::encode(duplicate_history_stream.writer()).unwrap();
            terminal::encode(&t, &mut duplicate_history_stream).unwrap();
            let alternate = t.screens.get(TerminalScreenKey::Alternate).unwrap();
            // SAFETY: as above.
            unsafe {
                screen::encode(
                    &*primary,
                    TerminalScreenKey::Primary,
                    &mut duplicate_history_stream,
                )
                .unwrap();
                screen::encode(
                    &*alternate,
                    TerminalScreenKey::Alternate,
                    &mut duplicate_history_stream,
                )
                .unwrap();
            }
            continuation::encode(&Continuation::Ground, &mut duplicate_history_stream).unwrap();
            checkpoint::encode(checkpoint::Kind::Ready, &mut duplicate_history_stream).unwrap();
            // SAFETY: as above.
            unsafe {
                history::encode(
                    &*primary,
                    TerminalScreenKey::Primary,
                    &mut duplicate_history_stream,
                )
                .unwrap();
                history::encode(
                    &*primary,
                    TerminalScreenKey::Primary,
                    &mut duplicate_history_stream,
                )
                .unwrap();
            }
            duplicate_history_stream.deinit();
        }
        let duplicate_history_written = duplicate_history.written().to_vec();
        let mut duplicate_history_source = io::FixedReader::new(&duplicate_history_written);
        assert_eq!(
            Err(DecodeError::DuplicateHistory),
            decode(
                &mut duplicate_history_source,
                Allocator,
                &test_decode_options(),
            )
            .map(|mut d| d.deinit(Allocator)),
        );

        t.deinit(Allocator);
    }

    // test "complete snapshot leaves continuation bytes unread"
    #[test]
    fn complete_snapshot_leaves_continuation_bytes_unread() {
        let mut t = Terminal::init(Allocator, terminal_options(2, 1, DEFAULT_MSB, None)).unwrap();
        // defer t.deinit(testing.allocator);

        let mut encoded = io::AllocatingWriter::new();
        encode(&mut encoded, &t, test_encode_options()).unwrap();
        let snapshot_len = encoded.written().len();
        encoded.write_all(b"pty").unwrap();

        let encoded_written = encoded.written().to_vec();
        let mut source = io::FixedReader::new(&encoded_written);
        let mut restored = decode(&mut source, Allocator, &test_decode_options()).unwrap();
        // defer restored.deinit(testing.allocator);

        let mut trailing = [0u8; 3];
        source.read_slice_all(&mut trailing).unwrap();
        assert_eq!(b"pty", &trailing);

        let mut exact_source = io::FixedReader::new(&encoded_written);
        assert_eq!(
            Err(DecodeExactError::TrailingData),
            decode_exact(&mut exact_source, Allocator, &test_decode_options())
                .map(|mut d| d.deinit(Allocator)),
        );

        let mut bounded_source = io::FixedReader::new(&encoded_written[0..snapshot_len]);
        let mut bounded =
            decode_exact(&mut bounded_source, Allocator, &test_decode_options()).unwrap();
        // defer bounded.deinit(testing.allocator);

        bounded.deinit(Allocator);
        restored.deinit(Allocator);
        t.deinit(Allocator);
    }

    // test "complete snapshots decode sequentially from one reader"
    #[test]
    fn complete_snapshots_decode_sequentially_from_one_reader() {
        let mut t = Terminal::init(Allocator, terminal_options(2, 1, DEFAULT_MSB, None)).unwrap();
        // defer t.deinit(testing.allocator);

        let mut encoded = io::AllocatingWriter::new();
        encode(&mut encoded, &t, test_encode_options()).unwrap();
        encode(&mut encoded, &t, test_encode_options()).unwrap();

        let encoded_written = encoded.written().to_vec();
        let mut source = io::FixedReader::new(&encoded_written);
        let mut first = decode(&mut source, Allocator, &test_decode_options()).unwrap();
        // defer first.deinit(testing.allocator);
        let mut second = decode(&mut source, Allocator, &test_decode_options()).unwrap();
        // defer second.deinit(testing.allocator);
        assert_eq!(Err(io::ReadError::EndOfStream), source.take_byte());

        second.deinit(Allocator);
        first.deinit(Allocator);
        t.deinit(Allocator);
    }

    // test "complete snapshot decode allocation failures are transactional"
    //
    // PORT NOTE: `std.testing.FailingAllocator` sweeping EVERY allocation →
    // the SNAPSHOT_TW tripwire seam (the crate's failure-injection stand-in;
    // see mod.rs), which is instrumented at the subsystem's fallible decode
    // allocations (the list is in `snapshot/mod.rs`'s `tw` doc comment). The
    // sweep bound is the baseline run's reach count, the analogue of
    // `FailingAllocator.alloc_index`; every index in it must actually trip.
    // Leak-freedom is proven by the Miri lane rather than the
    // FailingAllocator's accounting.
    //
    // PORT NOTE (parity S206-F1, closed under `PROCEDURE.md` blind-spot 1's
    // carve-out): one decode allocation in this subsystem was NOT
    // instrumented — `snapshot/grid.rs`'s `Remap::init`, which the reference
    // declares `Allocator.Error!Self` and the port made infallible.
    // UPDATE (batch T-A, S204-F1 closed): `Remap::init` is fallible and
    // instrumented now, and the fourth fixture below (a styled cell) reaches
    // the style remap; the hyperlink fixture reaches the hyperlink remap. The
    // rest of this note is the history of why. It lives in
    // `snapshot/grid.zig`, outside this row's cited oracle anchors
    // (`snapshot.zig:1626-1694`). It is **S204-F1**'s, re-opened 2026-09-11
    // (that row is anchored on the port side, `snapshot/page.rs:561-573`, which
    // is why a search of the ledger's oracle column missed it); the reading is
    // written up as N30 in `.port/parity-closure/NEW-FINDINGS.md`. The complete
    // golden's pages carry empty style/hyperlink tables, so they take
    // `Remap::empty()` and this sweep would not reach it even if it were
    // instrumented — closing S204-F1 needs a fixture whose pages carry styles,
    // and a raised floor here.
    //
    // PORT NOTE (which assertion is doing the work, so it is not weakened by
    // accident): the FLOOR. `allocation_count > 0` is subsumed by it — the floor
    // is a larger lower bound, and it is kept because the reference has it, not
    // because it adds coverage — and the per-index "must actually have TRIPPED"
    // assertion catches no mutation on its own; it exists to stop the bound
    // drifting ABOVE reality, which is a different failure from a site going
    // missing. Remove the floor and this seam stops being checked.
    //
    // PORT NOTE (the floor protects a site only if a FIXTURE REACHES IT, and
    // reach is badly asymmetric — so do not read the site list in `mod.rs` as a
    // coverage list). Measured by neutralising every other site and reading the
    // baseline counts back: `terminal::decode_alloc_check` is reached 3 times by
    // EVERY fixture (tab stops, pwd, title), and the complete golden's total
    // before this seam was widened was exactly 3 — that one site was the only
    // one the first fixture reached, and it alone was load-bearing.
    // `continuation::decode_alloc` was reached only by the continuation fixture
    // (4 = 3 + 1) and `hyperlink::decode_alloc` by neither. So the silently
    // losable pair was those two, and the loss was never order-independent: what
    // is losable depends on which fixture reaches what. If you add a fixture,
    // add it because it reaches something nothing else does — that is the only
    // reason the third one below exists.
    //
    // PORT NOTE (what this test still cannot see — a FIDELITY LIMIT, not a
    // regression): a site that is still REACHED but stops PROPAGATING its
    // failure keeps this test green. The reach count is unchanged, so the floor
    // is satisfied; the fail point still trips, so the per-index assertion is
    // satisfied; and `saw_out_of_memory` is satisfied by any other site.
    // Asserting `Err` at every index would catch it, but the reference test
    // deliberately permits `Ok`, so that assertion would be a divergence. The
    // reference's own test has the same blind spot
    // (`.port/parity-closure/UPSTREAM-DEFECTS.md` UD17).
    //
    // CORRECTION (task R13-R): this note used to give the reason as
    // `decodePageString`'s degrade to `Ok(0)`. That site allocates from the
    // page's own string allocator (`snapshot/hyperlink.zig`,
    // `page.string_alloc.alloc`), which the reference test's `FailingAllocator`
    // never sees. The reference site that can turn a failing heap allocation
    // into `Ok` is `decodeCursorHyperlink` (`snapshot/screen.zig:1117-1140`,
    // `error.OutOfMemory` → no cursor hyperlink). In this port the seam ALSO
    // arms `hyperlink::decode_page_string` (its caller degrades to `Ok(0)` as
    // the reference does on `StringsOutOfMemory`), which is a second way to
    // reach `Ok` here. Either way the conclusion stands.
    //
    // PORT NOTE: this test was previously `#[cfg(debug_assertions)]`-gated on
    // the belief that "tripwires only arm in debug builds". That is wrong for
    // this crate: `rustty-vt`'s dev-dependency turns on `rustty-util`'s
    // `tripwire-test` feature precisely so `tripwire::Module::enabled()` is
    // true in RELEASE test builds too (Zig's `builtin.is_test`, which holds at
    // any optimize mode). The gate only suppressed the test under
    // `cargo test --release`; the fail points themselves are `#[cfg(test)]`.
    #[test]
    fn complete_snapshot_decode_allocation_failures_are_transactional() {
        use crate::snapshot::tw;

        fn exercise(bytes: &[u8], min_allocations: usize) {
            // The baseline decode runs with no fail point armed; its reach
            // count is this seam's `FailingAllocator.alloc_index`.
            let _ = tw::take_reached();
            let mut baseline_source = io::FixedReader::new(bytes);
            let mut baseline_decoded =
                decode(&mut baseline_source, Allocator, &test_decode_options()).unwrap();
            baseline_decoded.deinit(Allocator);
            let allocation_count = tw::take_reached();
            assert!(allocation_count > 0);
            // PORT NOTE: `FailingAllocator` sees every allocation, so the
            // reference's count needs no floor. This seam is OPT-IN, so a
            // site that silently loses its `tw::check` would simply shrink
            // the sweep and stay green. The floor is a one-way ratchet that
            // makes such a loss RED; raise it when instrumentation is added,
            // never lower it. See the instrumented sites listed in the
            // `SnapshotFp` doc comment in `snapshot/mod.rs`.
            assert!(
                allocation_count >= min_allocations,
                "instrumented decode allocations regressed: \
                 {allocation_count} < {min_allocations}",
            );

            let mut saw_out_of_memory = false;
            for fail_index in 0..allocation_count {
                tw::error_after(tw::SnapshotFp::Alloc, fail_index);
                let mut source = io::FixedReader::new(bytes);
                match decode(&mut source, Allocator, &test_decode_options()) {
                    Ok(mut decoded) => decoded.deinit(Allocator),
                    Err(DecodeError::OutOfMemory) => saw_out_of_memory = true,
                    Err(err) => panic!("unexpected decode error: {err:?}"),
                }
                // Every index below `allocation_count` must actually have
                // tripped: this is what binds the sweep bound to the decode's
                // real allocation behaviour instead of to a hardcoded number.
                // `end` reports an UNTRIPPED expectation as an error.
                assert_eq!(
                    Ok(()),
                    tw::end(rustty_util::tripwire::ResetMode::Reset),
                    "fail point never tripped at fail_index={fail_index} \
                     (allocation_count={allocation_count})",
                );
            }
            assert!(saw_out_of_memory);
        }

        // The complete golden exercises Terminal, both screens, and history.
        exercise(&test_complete_fixture(), 15);

        // A non-ground component additionally exercises owned continuation
        // bytes.
        let mut t = Terminal::init(Allocator, terminal_options(2, 1, DEFAULT_MSB, None)).unwrap();
        // defer t.deinit(testing.allocator);
        let mut encoded = io::AllocatingWriter::new();
        encode(
            &mut encoded,
            &t,
            EncodeOptions {
                continuation: Continuation::Bytes(Cow::Borrowed(b"\x1b[31")),
            },
        )
        .unwrap();
        exercise(encoded.written(), 10);

        t.deinit(Allocator);

        // A terminal carrying an EXPLICIT hyperlink, so the page's own string
        // allocator is exercised.
        //
        // PORT NOTE: net-new relative to the reference, which needs no such
        // fixture because `FailingAllocator` sees every allocation whether a
        // fixture reaches it or not. This seam is opt-in, so a `tw::check` is
        // only protected by the floor if some fixture REACHES it — and neither
        // fixture above reaches `hyperlink::decode_page_string`
        // (`hyperlink.rs`'s page string-allocator site). Without this case,
        // deleting that one `tw::check` left the whole crate suite green.
        let mut t = Terminal::init(Allocator, terminal_options(10, 3, DEFAULT_MSB, None)).unwrap();
        // SAFETY: `screens.active` is the live active screen owned by `t`.
        let active: *mut TerminalScreen = t.screens.active;
        unsafe { (*active).start_hyperlink(b"https://example.com/parity", Some(b"link-1")) }
            .unwrap();
        unsafe { (*active).test_write_string(b"hi") }.unwrap();
        let mut encoded = io::AllocatingWriter::new();
        encode(&mut encoded, &t, test_encode_options()).unwrap();
        exercise(encoded.written(), 14);

        t.deinit(Allocator);

        // A terminal carrying a STYLED cell, so a page's style table is
        // non-empty and its decode takes `grid::StyleRemap::init`.
        //
        // PORT NOTE (parity S204-F1): net-new for the same reason as the
        // hyperlink fixture above. No other fixture writes a style, so without
        // this case the style remap's `tw::check` is reached by nothing in
        // this sweep. (styled_fixture_added)
        let mut t = Terminal::init(Allocator, terminal_options(10, 3, DEFAULT_MSB, None)).unwrap();
        {
            let mut stream = t.vt_stream();
            stream.next_slice(b"\x1b[1mhi");
        }
        let mut encoded = io::AllocatingWriter::new();
        encode(&mut encoded, &t, test_encode_options()).unwrap();
        exercise(encoded.written(), 10);

        t.deinit(Allocator);
    }

    // test "incremental decode restores a renderable terminal at READY"
    #[test]
    fn incremental_decode_restores_a_renderable_terminal_at_ready() {
        let test_complete_fixture = test_complete_fixture();

        // Trailing transport bytes must remain unread after FINISH.
        let mut transport = io::AllocatingWriter::new();
        transport.write_all(&test_complete_fixture).unwrap();
        transport.write_all(b"pty").unwrap();

        let transport_written = transport.written().to_vec();
        let mut source = io::FixedReader::new(&transport_written);
        let mut decoder = Decoder::init(&mut source);
        let mut decoded = decoder.ready(Allocator, &test_decode_options()).unwrap();
        // defer decoded.deinit(testing.allocator);

        // READY restored the complete interactive state, so the Terminal can
        // move to its final address before any history has been decoded.
        let mut restored = decoded.to_owned();
        // defer restored.deinit(testing.allocator);
        assert_eq!(TerminalScreenKey::Alternate, restored.screens.active_key,);
        assert_eq!(
            b"complete snapshot".as_slice(),
            restored.get_title().unwrap(),
        );

        // The advisory extents size scrollback UI immediately even though
        // only the resident SCREEN pages exist. The alternate screen's extent
        // is entirely resident overlap, so it must match its live row count.
        let primary = restored.screens.get(TerminalScreenKey::Primary).unwrap();
        let alternate = restored.screens.get(TerminalScreenKey::Alternate).unwrap();
        // SAFETY (each deref below): live screens owned by `restored`.
        assert_eq!(3usize, unsafe { (*primary).pages.scrollbar().total });
        assert_eq!(
            4u64,
            decoded
                .history_rows
                .get(TerminalScreenKey::Primary)
                .unwrap(),
        );
        assert_eq!(
            (unsafe { (*alternate).pages.scrollbar().total } - restored.rows as usize) as u64,
            decoded
                .history_rows
                .get(TerminalScreenKey::Alternate)
                .unwrap(),
        );

        // Each call prepends exactly one history page, newest first.
        let first = decoder.next(Allocator, &mut restored).unwrap().unwrap();
        assert_eq!(
            Progress {
                key: TerminalScreenKey::Primary,
                rows: 2,
                remaining: 1,
            },
            first,
        );
        assert_eq!(5usize, unsafe { (*primary).pages.scrollbar().total });
        assert_eq!('B' as u32, test_top_left_codepoint(primary));

        let second = decoder.next(Allocator, &mut restored).unwrap().unwrap();
        assert_eq!(
            Progress {
                key: TerminalScreenKey::Primary,
                rows: 2,
                remaining: 0,
            },
            second,
        );
        assert_eq!(7usize, unsafe { (*primary).pages.scrollbar().total });
        assert_eq!('A' as u32, test_top_left_codepoint(primary));

        // The alternate screen's empty sequence produces no event: the next
        // call validates FINISH, and further calls remain null.
        assert!(decoder.next(Allocator, &mut restored).unwrap().is_none());
        assert!(decoder.next(Allocator, &mut restored).unwrap().is_none());
        assert_eq!(b"pty".as_slice(), source.take(3).unwrap());

        // The incremental path restores the same complete terminal as the
        // one-shot decoder, byte for byte.
        let mut reencoded = io::AllocatingWriter::new();
        encode(&mut reencoded, &restored, test_encode_options()).unwrap();
        assert_eq!(test_complete_fixture.as_slice(), reencoded.written());

        decoded.deinit(Allocator);
        restored.deinit(Allocator);
    }

    // test "incremental next reports invalid decoder states"
    #[test]
    fn incremental_next_reports_invalid_decoder_states() {
        let mut source = io::FixedReader::new(b"");
        let mut decoder = Decoder::init(&mut source);
        let mut terminal_value =
            Terminal::init(Allocator, terminal_options(1, 1, DEFAULT_MSB, None)).unwrap();
        // defer terminal_value.deinit(testing.allocator);

        assert_eq!(
            Err(NextError::DecoderNotReady),
            decoder.next(Allocator, &mut terminal_value),
        );

        terminal_value.deinit(Allocator);
    }

    // test "incremental decode applies history below live PTY output"
    #[test]
    fn incremental_decode_applies_history_below_live_pty_output() {
        let mut t = test_history_terminal(2);
        // defer t.deinit(testing.allocator);
        let mut encoded = io::AllocatingWriter::new();
        encode(&mut encoded, &t, test_encode_options()).unwrap();

        let encoded_written = encoded.written().to_vec();
        let mut source = io::FixedReader::new(&encoded_written);
        let mut decoder = Decoder::init(&mut source);
        let mut decoded = decoder.ready(Allocator, &test_decode_options()).unwrap();
        // defer decoded.deinit(testing.allocator);
        let mut restored = decoded.to_owned();
        // defer restored.deinit(testing.allocator);

        // The terminal is interactive at READY: post-cut PTY bytes append to
        // the bottom while snapshot history is still pending.
        let mut live = restored.vt_stream();
        // defer live.deinit();
        live.next_slice(b"1\r\n2\r\n3\r\n4\r\n5\r\n");
        live.deinit();
        let primary = restored.screens.get(TerminalScreenKey::Primary).unwrap();
        // SAFETY (each deref below): live screen owned by `restored`.
        let live_total = unsafe { (*primary).pages.scrollbar().total };
        assert!(live_total > 3);

        // History still lands above both the snapshot content and the live
        // output, preserving complete scrollback ordering.
        while let Some(progress) = decoder.next(Allocator, &mut restored).unwrap() {
            assert_eq!(TerminalScreenKey::Primary, progress.key);
            assert_eq!(2usize, progress.rows);
        }
        assert_eq!(live_total + 4, unsafe {
            (*primary).pages.scrollbar().total
        });
        assert_eq!('A' as u32, test_top_left_codepoint(primary));
        // SAFETY: live nodes/pages of the restored screen; shared reads only.
        unsafe {
            let newest_history = (*(*primary).pages.get_top_left(point::Tag::Screen).node).next;
            assert_eq!(
                'B' as u32,
                (*(*Node::page(newest_history)).get_row_and_cell(0, 0).cell).codepoint(),
            );

            // The page below the applied history is the snapshot's active
            // page, whose top-left cell the live output already overwrote.
            assert_eq!(
                '1' as u32,
                (*(*Node::page((*newest_history).next))
                    .get_row_and_cell(0, 0)
                    .cell)
                    .codepoint(),
            );
        }

        decoded.deinit(Allocator);
        restored.deinit(Allocator);
        t.deinit(Allocator);
    }

    // test "incremental decode discards history for screens changed since READY"
    #[test]
    fn incremental_decode_discards_history_for_screens_changed_since_ready() {
        let mut t = Terminal::init(Allocator, terminal_options(2, 3, DEFAULT_MSB, None)).unwrap();
        // defer t.deinit(testing.allocator);
        t.switch_screen(TerminalScreenKey::Alternate).unwrap();
        t.print_string(b"alt").unwrap();
        t.switch_screen(TerminalScreenKey::Primary).unwrap();

        // The encoder never produces alternate-screen history, but the format
        // permits it, so the decoder must stay stream-aligned even when the
        // pages can no longer be applied. Order independence lets the
        // discarded sequence come first.
        let mut crafted = io::AllocatingWriter::new();
        {
            let mut stream_value = record::Writer::init(&mut crafted);
            // defer stream.deinit();
            envelope::encode(stream_value.writer()).unwrap();
            terminal::encode(&t, &mut stream_value).unwrap();
            let primary = t.screens.get(TerminalScreenKey::Primary).unwrap();
            let alternate = t.screens.get(TerminalScreenKey::Alternate).unwrap();
            // SAFETY (each deref below): live screens owned by `t`.
            unsafe {
                screen::encode(&*primary, TerminalScreenKey::Primary, &mut stream_value).unwrap();
                screen::encode(&*alternate, TerminalScreenKey::Alternate, &mut stream_value)
                    .unwrap();
            }
            continuation::encode(&Continuation::Ground, &mut stream_value).unwrap();
            checkpoint::encode(checkpoint::Kind::Ready, &mut stream_value).unwrap();
            {
                let payload = stream_value.begin(record::Tag::History);
                // errdefer stream.cancel();
                (history::Header {
                    key: TerminalScreenKey::Alternate,
                    page_count: 1,
                })
                .encode(payload)
                .unwrap();
                stream_value.finish().unwrap();
            }
            // SAFETY: the alternate screen's top-left node is live and
            // resident; encode only reads.
            unsafe {
                page::encode(
                    &*Node::page_assume_resident(
                        (*alternate).pages.get_top_left(point::Tag::Screen).node,
                    ),
                    &mut stream_value,
                )
                .unwrap();
                history::encode(&*primary, TerminalScreenKey::Primary, &mut stream_value).unwrap();
            }
            checkpoint::encode(checkpoint::Kind::Finish, &mut stream_value).unwrap();
            stream_value.deinit();
        }
        let crafted_written = crafted.written().to_vec();

        // An unchanged declared screen accepts crafted history, establishing
        // the baseline that the following runs degrade from.
        {
            let mut source = io::FixedReader::new(&crafted_written);
            let mut decoder = Decoder::init(&mut source);
            let mut decoded = decoder.ready(Allocator, &test_decode_options()).unwrap();
            // defer decoded.deinit(testing.allocator);
            let restored = decoded.terminal.as_mut().unwrap();

            let progress = decoder.next(Allocator, restored).unwrap().unwrap();
            assert_eq!(TerminalScreenKey::Alternate, progress.key);
            assert!(progress.rows > 0);
            assert!(decoder.next(Allocator, restored).unwrap().is_none());
            let alternate = restored.screens.get(TerminalScreenKey::Alternate).unwrap();
            // SAFETY: live screen owned by the decoded terminal.
            assert_eq!(2usize, unsafe { (*alternate).pages.total_pages() });

            decoded.deinit(Allocator);
        }

        // A screen removed since READY discards its history. FINISH still
        // validates, proving discarded records leave the stream aligned.
        {
            let mut source = io::FixedReader::new(&crafted_written);
            let mut decoder = Decoder::init(&mut source);
            let mut decoded = decoder.ready(Allocator, &test_decode_options()).unwrap();
            // defer decoded.deinit(testing.allocator);
            let restored = decoded.terminal.as_mut().unwrap();
            restored
                .screens
                .remove(Allocator, TerminalScreenKey::Alternate);

            let progress = decoder.next(Allocator, restored).unwrap().unwrap();
            assert_eq!(
                Progress {
                    key: TerminalScreenKey::Alternate,
                    rows: 0,
                    remaining: 0,
                },
                progress,
            );
            assert!(decoder.next(Allocator, restored).unwrap().is_none());

            decoded.deinit(Allocator);
        }

        // A screen removed and recreated since READY has a new generation, so
        // stale history never mixes into its replacement's content.
        {
            let mut source = io::FixedReader::new(&crafted_written);
            let mut decoder = Decoder::init(&mut source);
            let mut decoded = decoder.ready(Allocator, &test_decode_options()).unwrap();
            // defer decoded.deinit(testing.allocator);
            let restored = decoded.terminal.as_mut().unwrap();
            restored
                .screens
                .remove(Allocator, TerminalScreenKey::Alternate);
            restored
                .switch_screen(TerminalScreenKey::Alternate)
                .unwrap();

            let progress = decoder.next(Allocator, restored).unwrap().unwrap();
            assert_eq!(TerminalScreenKey::Alternate, progress.key);
            assert_eq!(0usize, progress.rows);
            assert!(decoder.next(Allocator, restored).unwrap().is_none());
            let alternate = restored.screens.get(TerminalScreenKey::Alternate).unwrap();
            // SAFETY: live screen owned by the decoded terminal.
            assert_eq!(1usize, unsafe { (*alternate).pages.total_pages() });

            decoded.deinit(Allocator);
        }

        t.deinit(Allocator);
    }

    // test "incremental decode discards history after a resize"
    #[test]
    fn incremental_decode_discards_history_after_a_resize() {
        let mut t = test_history_terminal(2);
        // defer t.deinit(testing.allocator);
        let mut encoded = io::AllocatingWriter::new();
        encode(&mut encoded, &t, test_encode_options()).unwrap();

        let encoded_written = encoded.written().to_vec();
        let mut source = io::FixedReader::new(&encoded_written);
        let mut decoder = Decoder::init(&mut source);
        let mut decoded = decoder.ready(Allocator, &test_decode_options()).unwrap();
        // defer decoded.deinit(testing.allocator);
        let mut restored = decoded.to_owned();
        // defer restored.deinit(testing.allocator);

        // Encoded pages assume the READY width. After a live resize reflows
        // the restored screen, pending history is consumed but never applied.
        restored
            .resize(
                Allocator,
                crate::terminal::Resize {
                    cols: 3,
                    rows: 3,
                    cell_size_px: None,
                },
            )
            .unwrap();
        let primary = restored.screens.get(TerminalScreenKey::Primary).unwrap();
        // SAFETY (each deref below): live screen owned by `restored`.
        let resized_total = unsafe { (*primary).pages.scrollbar().total };

        let mut pages: usize = 0;
        while let Some(progress) = decoder.next(Allocator, &mut restored).unwrap() {
            assert_eq!(TerminalScreenKey::Primary, progress.key);
            assert_eq!(0usize, progress.rows);
            pages += 1;
        }
        assert_eq!(2usize, pages);
        assert_eq!(resized_total, unsafe { (*primary).pages.scrollbar().total });

        decoded.deinit(Allocator);
        restored.deinit(Allocator);
        t.deinit(Allocator);
    }

    // test "decode drops history beyond the declared scrollback limits"
    #[test]
    fn decode_drops_history_beyond_the_declared_scrollback_limits() {
        // Declare a byte limit the complete history cannot satisfy. The limit
        // is validated at page granularity on decode, so the snapshot itself
        // remains well formed and fully validated.
        let mut t = test_history_terminal(3);
        // defer t.deinit(testing.allocator);
        {
            let primary = t.screens.get(TerminalScreenKey::Primary).unwrap();
            // SAFETY: live screen owned by `t`.
            unsafe { (*primary).pages.limits.set(pagelist::Key::Bytes, Some(1)) };
        }
        let mut encoded = io::AllocatingWriter::new();
        encode(&mut encoded, &t, test_encode_options()).unwrap();
        let encoded_written = encoded.written().to_vec();

        // Incremental: the newest page fits the effective floor, the next
        // page exceeds it, and one drop discards the remainder of the
        // sequence so no ordering gap can form.
        {
            let mut source = io::FixedReader::new(&encoded_written);
            let mut decoder = Decoder::init(&mut source);
            let mut decoded = decoder.ready(Allocator, &test_decode_options()).unwrap();
            // defer decoded.deinit(testing.allocator);
            let restored = decoded.terminal.as_mut().unwrap();

            let mut rows = [0usize; 3];
            let mut pages: usize = 0;
            while let Some(progress) = decoder.next(Allocator, restored).unwrap() {
                rows[pages] = progress.rows;
                pages += 1;
            }
            assert_eq!(3usize, pages);
            assert_eq!([2usize, 0, 0], rows);

            let primary = restored.screens.get(TerminalScreenKey::Primary).unwrap();
            // SAFETY: live screen owned by the decoded terminal.
            assert_eq!(2usize, unsafe { (*primary).pages.total_pages() });
            assert_eq!('C' as u32, test_top_left_codepoint(primary));

            decoded.deinit(Allocator);
        }

        // The one-shot decoder degrades identically instead of rejecting the
        // snapshot.
        {
            let mut source = io::FixedReader::new(&encoded_written);
            let mut decoded = decode(&mut source, Allocator, &test_decode_options()).unwrap();
            // defer decoded.deinit(testing.allocator);
            let primary = decoded
                .terminal
                .as_mut()
                .unwrap()
                .screens
                .get(TerminalScreenKey::Primary)
                .unwrap();
            // SAFETY: live screen owned by the decoded terminal.
            assert_eq!(2usize, unsafe { (*primary).pages.total_pages() });
            assert_eq!('C' as u32, test_top_left_codepoint(primary));

            decoded.deinit(Allocator);
        }

        t.deinit(Allocator);
    }

    // test "incremental decode failure leaves applied history usable"
    #[test]
    fn incremental_decode_failure_leaves_applied_history_usable() {
        let mut t = test_history_terminal(2);
        // defer t.deinit(testing.allocator);
        let mut encoded = io::AllocatingWriter::new();
        encode(&mut encoded, &t, test_encode_options()).unwrap();

        // Truncate FINISH. READY and both history pages decode, then the
        // failure surfaces on the call that would have completed the snapshot.
        let encoded_written = encoded.written().to_vec();
        let mut source = io::FixedReader::new(&encoded_written[0..encoded_written.len() - 1]);
        let mut decoder = Decoder::init(&mut source);
        let mut decoded = decoder.ready(Allocator, &test_decode_options()).unwrap();
        // defer decoded.deinit(testing.allocator);
        let mut restored = decoded.to_owned();
        // defer restored.deinit(testing.allocator);

        for _ in 0..2 {
            let progress = decoder.next(Allocator, &mut restored).unwrap().unwrap();
            assert_eq!(2usize, progress.rows);
        }
        assert_eq!(
            Err(NextError::EndOfStream),
            decoder.next(Allocator, &mut restored),
        );
        assert_eq!(
            Err(NextError::DecoderFailed),
            decoder.next(Allocator, &mut restored),
        );

        // The transferred terminal was never owned by the decoder: it remains
        // valid with the contiguous history prefix that did apply; only the
        // incomplete snapshot stream is lost.
        let primary = restored.screens.get(TerminalScreenKey::Primary).unwrap();
        // SAFETY (each deref below): live screen owned by `restored`.
        assert_eq!(3usize, unsafe { (*primary).pages.total_pages() });
        assert_eq!('A' as u32, test_top_left_codepoint(primary));
        unsafe {
            (*primary).pages.assert_integrity();
            (*primary).assert_integrity();
        }

        decoded.deinit(Allocator);
        restored.deinit(Allocator);
        t.deinit(Allocator);
    }
}
