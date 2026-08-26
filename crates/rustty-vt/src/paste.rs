//! Pasting into a terminal.
//!
//! This is the single place that turns "the user pasted" into bytes for
//! the pty, applying the terminal's current state:
//!
//!   * Mode 5522 (Kitty clipboard protocol paste events) set, a
//!     user-initiated clipboard paste, and the embedder able to serve
//!     the program's follow-up clipboard read: send a paste event
//!     listing the clipboard's MIME types with a fresh one-time password
//!     and record a one-time read grant for it. No data is read.
//!   * Otherwise: write the first text representation, with unsafe bytes
//!     replaced (xterm behavior), framed per mode 2004 (bracketed paste)
//!     or with newlines converted to carriage returns if not.
//!
//! The precedence (5522 event, else 2004 framing, else plain) and the
//! safety rule live only here so every embedder of the terminal shares
//! one implementation.
//!
//! PORTS: ghostty src/terminal/paste.zig (a5bb22e23 end state per da27e6c90)
//!
//! PORT STATUS: done (the W3 kitty-clipboard seams are filled:
//! `KittyClipboard`, the mode-5522 conjunct, and `paste_kitty_event` ride
//! `kitty/clipboard*.rs`).

// PORT NOTE: the reference imports are
//   const std = @import("std");
//   const Allocator = std.mem.Allocator;
//   const clipboard = @import("clipboard.zig");
//   const kitty_clipboard = @import("kitty/clipboard.zig");
//   const input_paste = @import("../input/paste.zig");
//   const Terminal = @import("Terminal.zig");
// `*std.Io.Writer` maps to the crate's byte-oriented stand-in,
// `snapshot::io::Writer`, and `std.Io.Writer.Allocating` to
// `snapshot::io::AllocatingWriter`; `std.Io` (the entropy capability) maps to
// the sys.rs `Io` stand-in (see its NON_PORTS TinyIo re-read note).
use crate::clipboard;
use crate::input::paste as input_paste;
use crate::kitty::clipboard as kitty_clipboard;
use crate::snapshot::io::{AllocatingWriter, Writer};
use crate::sys;
use crate::terminal::Terminal;

/// Why a paste happened. Only clipboard pastes may become paste
/// events, which is why only they carry a location: it's meaningless
/// for text insertion.
///
/// The C form (`rustty_paste_source_e`, declared by the vt-c `paste::Source`)
/// is flattened next to the location since C has no tagged unions.
// pub const Source = union(enum) { clipboard: clipboard.Location, text };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// The user pasted from a clipboard: keybind, menu, middle click.
    /// The payload is the clipboard the contents came from.
    Clipboard(clipboard::Location),

    /// Text inserted some other way: IME commit, drag and drop,
    /// scripted input.
    Text,
}

impl Default for Source {
    // (Zig: the `Request.source` field default, `.{ .clipboard = .standard }`.)
    fn default() -> Source {
        Source::Clipboard(clipboard::Location::Standard)
    }
}

/// A paste of clipboard contents into the terminal. What actually gets
/// written depends on terminal state; see `paste`.
pub struct Request<'a> {
    /// Why this paste happened, and for a clipboard paste, from which
    /// clipboard. Only a user-initiated clipboard paste may become a
    /// paste event; text insertion always writes text.
    // source: Source = .{ .clipboard = .standard },
    // (Zig field default; Rust callers spell it `Source::default()`.)
    pub source: Source,

    /// The representations available. Borrowed only during the
    /// duration of the paste function call.
    pub contents: Contents<'a>,

    /// Write data that could inject commands (see `paste`). The usual
    /// flow is to call with false, confirm with the user on
    /// error.UnsafePaste, and call again with true.
    // allow_unsafe: bool = false,
    pub allow_unsafe: bool,
}

/// The representations available for a paste, in the embedder's
/// preferred order.
pub enum Contents<'a> {
    /// Every representation already in memory. For text the embedder
    /// holds anyway (an IME commit, dropped text) or small clipboards.
    Memory(&'a [clipboard::Content<'a>]),

    /// Representations read on demand, so nothing is loaded that isn't
    /// pasted. This is the form for a real clipboard, whose non-text
    /// items may be huge.
    Reader(Reader<'a>),
}

/// The on-demand form: the MIME types available plus the reader
/// that produces the data of any one of them.
// PORT NOTE: the reference nests this as `Contents.Reader`; Rust enums cannot
// nest type declarations, so it is flattened to module scope.
pub struct Reader<'a> {
    /// The MIME types available, in preferred order.
    pub mimes: &'a [&'a [u8]],

    /// Produces the data of any entry of `mimes`, which is passed
    /// through to it as is. A paste reads at most once: the text
    /// representation being pasted, never anything else and never
    /// anything for a paste event. There is no requirement across
    /// paste calls, so a source that changes between an unsafe
    /// refusal and the embedder's confirmed retry simply pastes
    /// its current contents.
    pub read: clipboard::MimeReader,
}

impl Contents<'_> {
    /// The number of representations.
    // (`is_empty` is not part of the reference surface; `paste` iterates
    // `0..len()`.)
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        match self {
            Contents::Memory(v) => v.len(),
            Contents::Reader(v) => v.mimes.len(),
        }
    }

    /// The MIME type of representation `index`.
    pub fn mime(&self, index: usize) -> &[u8] {
        match self {
            Contents::Memory(v) => v[index].mime,
            Contents::Reader(v) => v.mimes[index],
        }
    }
}

/// What a caller supplies to `paste`: the terminal state the decision
/// depends on, the session state an event records into, and the sink.
pub struct Context<'a> {
    /// The terminal whose modes decide the encoding.
    // terminal: *const Terminal,
    pub terminal: &'a Terminal,

    // PORT NOTE: the reference field
    //     /// Allocator for transient state: the buffered read and a paste
    //     /// event's grant. Must be the allocator `kitty_clipboard.grants`
    //     /// is freed with.
    //     alloc: Allocator,
    // is the crate-wide global-allocator treatment (see snapshot/io.rs
    // `AllocatingWriter`): the buffered read allocates infallibly from the
    // global allocator, so the port carries no field.

    /// Receives the encoded text in chunks, or the event. Must be
    /// buffered (the encoder works in its buffer) and is not flushed
    /// by `paste`; the caller flushes once it returns.
    // writer: *std.Io.Writer,
    pub writer: &'a mut dyn Writer,

    /// Kitty clipboard protocol session state, or null if the embedder
    /// does not serve Kitty clipboard reads (`clipboard.Read`). A
    /// paste event is that protocol: it is only useful if the
    /// program's follow-up read can be answered, since otherwise the
    /// read would be refused and the user's paste would vanish. With
    /// null, `paste` always writes text.
    pub kitty_clipboard: Option<KittyClipboard<'a>>,
}

/// Kitty clipboard protocol session state for a paste event.
// PORT NOTE: the reference nests this as `Context.KittyClipboard`; Rust
// structs cannot nest type declarations, so it is flattened to module scope.
// The reference `grants: *kitty_clipboard.Grants` is a mutable borrow here
// (the grant/revoke calls mutate it), and `io: std.Io` is the sys.rs `Io`
// entropy stand-in.
pub struct KittyClipboard<'a> {
    /// Session grants. A paste event records its one-time password
    /// here so the program's follow-up read is served without a
    /// prompt.
    // grants: *kitty_clipboard.Grants,
    pub grants: &'a mut kitty_clipboard::Grants,

    /// Secure entropy for one-time passwords. See generateOtp for
    /// why there is no fallback when this has none.
    // io: std.Io,
    pub io: sys::Io,
}

// pub const Error = Allocator.Error ||
//     std.Io.RandomSecureError ||
//     clipboard.MimeReader.Error ||
//     error{ UnsafePaste };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// `Allocator.Error` — `error.OutOfMemory`. (Reachable only through the
    /// reference's read-buffering arm; see the catch mapping in `paste`.)
    OutOfMemory,

    /// `std.Io.RandomSecureError` — `error.EntropyUnavailable`. Produced by
    /// the paste-event one-time-password mint (`generateOtp`).
    EntropyUnavailable,

    /// `std.Io.RandomSecureError` — the `std.Io.Cancelable` member,
    /// `error.Canceled`. Carried for reference-error-set parity; the sys.rs
    /// entropy stand-in (`RandomSecureError`) has no cancellation member, so
    /// this variant is never produced today.
    Canceled,

    /// `clipboard.MimeReader.Error` — `error.ReadFailed`.
    ReadFailed,

    /// `clipboard.MimeReader.Error` / `std.Io.Writer.Error` —
    /// `error.WriteFailed`.
    WriteFailed,

    /// The data could inject commands and allow_unsafe was false.
    UnsafePaste,
}

/// Paste into the terminal, applying the terminal's current state as
/// described in the module docs. Returns true if anything was written
/// to `ctx.writer`: the encoded text or a paste event. False means
/// there was nothing to paste (no non-empty text representation).
///
/// The safety rule for a text paste (`input.paste.isSafeWith`): a
/// bracketed paste is unsafe only if it contains the bracket terminator
/// (CSI 201~); an unbracketed paste is unsafe if it contains a newline
/// or the terminator. Embedders wanting a stricter rule check
/// `input.paste.isSafe` themselves before calling. A paste event never
/// puts the data on the input stream, so the rule doesn't apply to it.
///
/// The contents are read at most once per call and buffered whole, so
/// the source needs no stability across reads. Nothing reaches the
/// writer until the read completed and the text passed the rule:
/// every error writes nothing, and a failed event records no grant.
pub fn paste(mut ctx: Context<'_>, req: Request<'_>) -> Result<bool, Error> {
    // If the source is a clipboard and mode 5522 is enabled and
    // the caller can handle kitty events, then do a kitty event.
    //
    // if (req.source == .clipboard and
    //     ctx.terminal.modes.get(.kitty_paste_events))
    if matches!(req.source, Source::Clipboard(_))
        && ctx.terminal.modes.get(crate::modes::Mode::KittyPasteEvents)
    {
        // if (ctx.kitty_clipboard) |kitty| { ... }
        // PORT NOTE: `take()` because the Rust `KittyClipboard` carries a
        // `&mut Grants` (the reference's `*Grants`), which cannot be copied
        // out of the borrowed `ctx`.
        if let Some(kitty) = ctx.kitty_clipboard.take() {
            paste_kitty_event(&mut ctx, kitty, &req)?;
            return Ok(true);
        }
    }

    // For non-Kitty paste events we can only accept text content.
    // const index: usize = for (0..req.contents.len()) |i| {
    //     if (clipboard.isTextMime(req.contents.mime(i))) break i;
    // } else return false;
    let index: usize = match
        (0..req.contents.len()).find(|&i| clipboard::is_text_mime(req.contents.mime(i)))
    {
        Some(i) => i,
        None => return Ok(false),
    };

    let opts: input_paste::Options = input_paste::Options::from_terminal(ctx.terminal);

    // In-memory contents are used directly to avoid a double copy but
    // reader-based contents are read into memory so we can do the unsafe
    // scan.
    // var aw: std.Io.Writer.Allocating = .init(ctx.alloc);
    // defer aw.deinit();
    let mut aw = AllocatingWriter::new();
    let text: &[u8] = match &req.contents {
        Contents::Memory(v) => v[index].data,
        Contents::Reader(v) => {
            // v.read.read(v.mimes[index], &aw.writer) catch |err| switch (err) {
            //     // An allocating writer only fails to allocate.
            //     error.WriteFailed => return error.OutOfMemory,
            //     error.ReadFailed => |e| return e,
            // };
            v.read.read(v.mimes[index], &mut aw).map_err(|err| match err {
                // An allocating writer only fails to allocate.
                clipboard::MimeReaderError::WriteFailed => Error::OutOfMemory,
                clipboard::MimeReaderError::ReadFailed => Error::ReadFailed,
            })?;
            // break :text aw.writer.buffered();
            aw.written()
        }
    };
    if text.is_empty() {
        return Ok(false);
    }
    if !req.allow_unsafe && !input_paste::is_safe_with(text, opts) {
        return Err(Error::UnsafePaste);
    }

    // The text is copied exactly once per chunk, into the writer,
    // where the encoder strips and converts it in place.
    input_paste::encode_writer(ctx.writer, text, opts).map_err(|_| Error::WriteFailed)?;
    Ok(true)
}

// fn pasteKittyEvent(
//     ctx: Context,
//     kitty: Context.KittyClipboard,
//     req: Request,
// ) Error!void {
fn paste_kitty_event(
    ctx: &mut Context<'_>,
    kitty: KittyClipboard<'_>,
    req: &Request<'_>,
) -> Result<(), Error> {
    // const otp = try kitty_clipboard.generateOtp(kitty.io);
    let otp = kitty_clipboard::generate_otp(kitty.io).map_err(|err| match err {
        sys::RandomSecureError::EntropyUnavailable => Error::EntropyUnavailable,
    })?;

    // Every representation is listed, never read. The listing is
    // bounded; a clipboard with more types than that is not a thing.
    // var mimes_buf: [kitty_clipboard.max_listing_mimes][]const u8 = undefined;
    let mut mimes_buf: [&[u8]; kitty_clipboard::max_listing_mimes] =
        [b""; kitty_clipboard::max_listing_mimes];
    // const mimes_len = @min(req.contents.len(), mimes_buf.len);
    let mimes_len = req.contents.len().min(mimes_buf.len());
    // for (mimes_buf[0..mimes_len], 0..) |*mime, i| mime.* = req.contents.mime(i);
    for (i, mime) in mimes_buf[0..mimes_len].iter_mut().enumerate() {
        *mime = req.contents.mime(i);
    }

    // The grant is recorded before the event can reach the program,
    // and revoked if the event fails to be written so a failure never
    // leaves a grant for an event that was never sent. Using a
    // one-time grant consumes it, which is the revocation.
    // try kitty.grants.grant(ctx.alloc, &otp, .read, true);
    kitty
        .grants
        .grant(&otp, kitty_clipboard::Direction::Read, true);

    let event = kitty_clipboard::PasteEvent {
        // The protocol only distinguishes the clipboard from the
        // primary selection, so both non-standard locations report as
        // primary. Only a clipboard paste gets here; see `paste`.
        // .primary = req.source.clipboard != .standard,
        primary: match req.source {
            Source::Clipboard(loc) => loc != clipboard::Location::Standard,
            // (Zig `req.source.clipboard` would safety-panic here too.)
            Source::Text => unreachable!("only a clipboard paste gets here; see paste"),
        },
        ..kitty_clipboard::PasteEvent::new(&otp, &mimes_buf[0..mimes_len])
    };
    // try (...).encode(ctx.writer);
    // (errdefer: revoke the grant when the event was never sent.)
    if event.encode(ctx.writer).is_err() {
        // errdefer _ = kitty.grants.use(ctx.alloc, &otp, .read);
        let _ = kitty
            .grants
            .use_(&otp, kitty_clipboard::Direction::Read);
        return Err(Error::WriteFailed);
    }
    Ok(())
}

// test {
//     // The behavior is tested end to end through the stream handler
//     // (stream_terminal.zig), which is the primary caller.
//     std.testing.refAllDecls(@This());
// }
// PORT NOTE: the reference's only test is a `refAllDecls` compile-coverage
// touch — covered in Rust by compilation itself. The end-to-end behavior
// tests live in `stream_terminal.zig`'s paste handler and land with the
// Dispatch stage (which owns stream_terminal.rs); this module's handler and
// effect types stay `pub` for it.
