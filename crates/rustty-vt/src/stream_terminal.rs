//! rustty-vt stream_terminal
//!
//! PORTS: ghostty src/terminal/stream_terminal.zig
//!
//! PORT STATUS: done. (Corrected WS10 T10, 2026-07-30: this line read "stub"
//! from the file's first landing and was never updated, even though the PORT
//! NOTEs immediately below record every dispatch arm landing. It is cited as a
//! blocker by `.port/phase-3/assessments/B-compression.md` §3.4 — "the
//! `TerminalStream` bench swap needs `terminal.TerminalStream` from area A" —
//! which is stale for the same reason. `pub type Stream<'a> = stream::Stream<'a,
//! Handler<'a>>` below has existed with filled arms throughout, and WS10 T10's
//! `terminal-stream` benchmark now depends on it.)
//!
//! vt-12b-7a fills the STATE-mutating dispatch arms (the `vtFallible` arms that
//! call already-ported Terminal methods), the Effects scaffolding, and the
//! simple helpers (`horizontalTab`/`horizontalTabBack`/`setMode`). vt-12b-7b
//! fills the RESPONSE/QUERY arms (device_attributes/device_status/report_*/
//! request_mode*/send_mode_report/query_kitty_keyboard/enquiry/xtversion/
//! size_report/window_title/report_pwd) — they encode a VT response and write it
//! via the Effects `write_pty` callback. Stage A.3 wires the real `apc::Handler`:
//! `apcStart`/`apcPut`/`apcEnd` drive it and `apcEnd` dispatches the kitty
//! graphics + Glyph Protocol responses (the apc cluster is unblocked).

// PORT NOTE (vt-12b-7b): the RESPONSE/QUERY arms are now filled, so the
// `clippy::todo` allow is dropped. `dead_code`/`unused_variables` are retained:
// the reference's no-op arms (`.title_push`/`.title_pop`/`.dcs_*`/
// `.show_desktop_notification`/...) carry value params they ignore, and the
// `vt_raw` argument remains unread.
#![allow(dead_code)]
#![allow(unused_variables)]

// const std = @import("std");
// const build_options = @import("terminal_options");
// const testing = std.testing;
// const apc = @import("apc.zig");
// const csi = @import("csi.zig");
// const device_attributes = @import("device_attributes.zig");
// const device_status = @import("device_status.zig");
// const stream = @import("stream.zig");
// const Action = stream.Action;
// const Screen = @import("Screen.zig");
// const modes = @import("modes.zig");
// const osc_color = @import("osc/parsers/color.zig");
// const kitty_color = @import("kitty/color.zig");
// const size_report = @import("size_report.zig");
// const Terminal = @import("Terminal.zig");
use crate::ansi;
use crate::clipboard;
use crate::color;
use crate::csi;
use crate::device_attributes;
use crate::device_status;
use crate::kitty;
use crate::modes;
use crate::osc;
use crate::paste as paste_pkg;
use crate::sgr;
use crate::size_report;
use crate::stream;
use crate::terminal::{DeccolmMode, SwitchScreenMode, Terminal};

use core::marker::PhantomData;

mod dispatch;
mod paste_clipboard;
mod reports;

// const log = std.log.scoped(.stream_terminal);
const LOG_SCOPE: &str = "stream_terminal";

/// This is a Stream implementation that processes actions against
/// a Terminal and updates the Terminal state.
// pub const Stream = stream.Stream(Handler);
// PORT NOTE: the reference `stream.Stream(Handler)` becomes the generic
// `stream::Stream<Handler<'a>>`. The `'a` threads the OSC parser allocator
// lifetime (see `stream::Stream::init_alloc`) through both the stream and the
// handler's effect callbacks.
pub type Stream<'a> = stream::Stream<Handler<'a>>;

/// The APC command handler maintains the APC state.
///
/// PORT NOTE (Stage A.3 — apc complete): the apc cluster is unblocked
/// (rustty-font landed); this is now the real `apc::Handler`, not a ZST stub.
/// `ApcHandler` is a type alias so the field type and `apc.Handler` call sites
/// (`start`/`feed`/`end`/`deinit`) map directly onto the ported handler.
pub type ApcHandler = crate::apc::Handler;

/// A stream handler that updates terminal state. By default, it is
/// readonly in the sense that it only updates terminal state and ignores
/// all other sequences that require a response or otherwise have side
/// effects (e.g. clipboards).
///
/// You can manually set various effects callbacks in the `effects` field
/// to implement certain effects such as bells, titles, clipboard, etc.
pub struct Handler<'a> {
    /// The terminal state to modify.
    // terminal: *Terminal
    // PORT NOTE: `*mut Terminal` (raw ptr, NOT `&mut`). The `Stream` owns this
    // `Handler`, and the `Handler` aliases the `Terminal` it is created from
    // (`Terminal::vt_handler`). This mirrors `ScreenSet`'s `active: *mut Screen`
    // seam: the borrow checker can't express the self-referential aliasing, so
    // we carry the raw pointer and deref it (`unsafe { &mut *self.terminal }`)
    // in the method bodies (vt-12b-7).
    pub terminal: *mut Terminal,

    /// True after an error prevented a terminal-owned semantic update.
    ///
    /// When an error happens during terminal processing, streams continue
    /// forward and remain best-effort. A terminal can't really stop in
    /// the middle it must go on. But this is flagged to true to let
    /// consumers know some sort of unhandle-able error state happened
    /// (e.g. an allocation failure).
    ///
    /// Only non-handled outcomes set this. Gracefully handled outcomes
    /// that don't meaningfully negatively impact the terminal state
    /// such as hitting Kitty image limits, failure to write a response,
    /// do not flag this.
    // semantic_failure: bool = false,
    pub semantic_failure: bool,

    /// Callbacks for certain effects that handlers may have. These
    /// may or may not fully replace internal handling of certain effects,
    /// but they allow for the handler to trigger or query external
    /// effects.
    pub effects: Effects<'a>,

    /// Whether CSI 21 t may report the terminal title. This is disabled by
    /// default because reporting an attacker-controlled title to the pty can
    /// inject text into the input stream of the foreground process.
    // title_report: bool = false,
    pub title_report: bool,

    /// The APC command handler maintains the APC state. APC is like
    /// CSI or OSC, but it is a private escape sequence that is used
    /// to send commands to the terminal emulator. This is used by
    /// the kitty graphics protocol.
    pub apc_handler: ApcHandler,

    /// The DCS command handler maintains state for DCS queries.
    // dcs_handler: dcs.Handler = .{},
    pub dcs_handler: crate::dcs::Handler,

    /// The in-flight Kitty clipboard protocol (OSC 5522) write
    /// transaction, if any. Null means no transaction is active.
    /// Heap-allocated since transactions are rare and short-lived.
    // kitty_clipboard_write: ?*kitty_clipboard.WriteState = null,
    // PORT NOTE: `?*` (manual create/destroy) -> `Option<Box<_>>`; the abort
    // and finish paths become `take()` + drop.
    pub kitty_clipboard_write: Option<crate::alloc::ABox<kitty::clipboard::WriteState>>,

    /// Kitty clipboard protocol (OSC 5522) session password grants,
    /// recorded when a clipboard_read or clipboard_write reply asks to
    /// remember the user's decision.
    // kitty_clipboard_grants: kitty_clipboard.Grants = .{},
    pub kitty_clipboard_grants: kitty::clipboard::Grants,

    /// Maximum total decoded bytes accumulated by one Kitty clipboard
    /// protocol (OSC 5522) write transaction, captured when the
    /// transaction begins. Data beyond the limit fails the transaction
    /// with EFBIG.
    // kitty_clipboard_write_max_bytes: usize = kitty_clipboard.max_write_size,
    pub kitty_clipboard_write_max_bytes: usize,

    /// Called for escape sequences this library does not implement, so you
    /// can implement them yourself. See `UnknownSequence` for the kinds of
    /// sequences that are reported.
    ///
    /// Nothing is reported until you also set a byte limit for each kind
    /// you want. The limits default to zero, which turns reporting off:
    ///
    ///   - APC: `apc_handler.unknown_max_bytes`
    ///   - OSC: `Stream.Options.osc_unknown_max_bytes` when creating the
    ///     stream, or `stream.parser.osc_parser.unknown_max_bytes` later.
    ///
    /// This example handles a made-up OSC 7400 and ignores everything
    /// else:
    ///
    /// ```ignore
    /// fn on_unknown(handler: &mut Handler, seq: UnknownSequence) {
    ///     let v = match seq {
    ///         UnknownSequence::Osc(v) => v,
    ///         _ => return,
    ///     };
    ///
    ///     // Match the number and its ";" so "74000;" is not included.
    ///     let prefix = b"7400;";
    ///     if v.truncated || !v.content.starts_with(prefix) {
    ///         return;
    ///     }
    ///     handle_status(handler, &v.content[prefix.len()..]);
    /// }
    ///
    /// let mut handler = terminal.vt_handler();
    /// handler.unknown_sequence = Some(on_unknown);
    /// let mut stream = Stream::init(stream::Options {
    ///     allocator: Some(alloc),
    ///     handler,
    ///     continuation_max_bytes: None,
    ///     osc_unknown_max_bytes: 4096,
    /// });
    /// ```
    ///
    /// The callback runs while the stream processes input. It may write a
    /// reply to the pty, and the reply stays in order with the terminal's
    /// own replies. It must not feed more input to the same stream.
    // unknown_sequence: ?*const fn (*Handler, UnknownSequence) void = null,
    //
    // PORT NOTE: the `for<'b>` is the same "borrowed and only valid for the
    // duration of the callback" spelling the `desktop_notification` effect
    // uses — the content slice points into the APC handler's own buffer, or
    // into the OSC parser's capture for `UnknownSequence::Osc`.
    pub unknown_sequence: Option<for<'b> fn(&mut Handler<'a>, UnknownSequence<'b>)>,

    /// The name of the terminfo entry this terminal runs as, reported in
    /// response to an XTGETTCAP query for "TN".
    ///
    /// The memory must remain valid for the lifetime of the handler.
    /// Empty names and names longer than `max_terminfo_name_bytes` are
    /// silently ignored.
    // terminfo_name: ?[]const u8 = null,
    //
    // PORT NOTE: "must remain valid for the lifetime of the handler" is the
    // `'a` bound (the same lifetime the `enquiry`/`xtversion` effect payloads
    // use).
    pub terminfo_name: Option<&'a [u8]>,

    // PORT NOTE: ties the `'a` effect-callback lifetime to the handler without a
    // stored borrow; the actual aliased `Terminal` is held as a raw pointer.
    _phantom: PhantomData<&'a ()>,
}

/// A sequence this library does not implement, passed to the
/// `unknown_sequence` callback. The data is only valid until the
/// callback returns. Copy it if you need it later.
///
/// More kinds of sequences may be added later, so switch on this with
/// an `else` branch that ignores kinds you don't handle.
// pub const UnknownSequence = union(enum) {
//     apc: String,
//     osc: Osc,
//     ...
// };
// PORT NOTE: the reference nests this type (and its `String` payload struct)
// inside `Handler`; Rust types cannot nest in a struct, so both live at module
// scope with the `Handler.`-qualified names flattened (`UnknownSequenceString`
// for `UnknownSequence.String` — `String` alone would collide with
// `std::string::String`). `UnknownSequence.Osc = osc.Command.Unknown` is used
// directly. Not `#[non_exhaustive]` (DECISIONS P7): the "keep an else branch"
// advice above is the reference's contract for callers, and the crate's own
// matches are exhaustive so a new kind fails to compile here first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnknownSequence<'b> {
    /// An APC sequence (`ESC _`) whose identifier is not implemented.
    Apc(UnknownSequenceString<'b>),

    /// An OSC sequence (`ESC ]`) whose number is not implemented.
    Osc(osc::Unknown<'b>),
}

/// Content between a string sequence's introducer and terminator.
// pub const String = struct {
//     content: []const u8,
//     truncated: bool,
// };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnknownSequenceString<'b> {
    pub content: &'b [u8],
    pub truncated: bool,
}

/// A shell integration event, passed to the `semantic_prompt` effect.
///
/// Many shells tell the terminal where each prompt, command, and
/// command output begins. Each command goes through four steps, and
/// the effect is called once for each step the shell reports:
///
///   1. `prompt_start`: the shell starts drawing a prompt.
///   2. `input_start`: the prompt is drawn and the user can type.
///   3. `output_start`: the user submitted the command and it runs.
///   4. `command_end`: the command finished.
///
/// Then the shell draws the next prompt and the steps start over.
///
/// Shells differ in what they report. Many don't send the command
/// line or the exit code, and some skip steps, so handle each event
/// on its own instead of expecting a strict order. A shell may also
/// start the same prompt more than once, for example when it redraws
/// the prompt after a resize, so treat a repeated `prompt_start` as
/// harmless.
///
/// The event describes what happened, not how the shell said it.
/// Today events come from OSC 133. More shell integration protocols
/// may report through this same type later.
///
/// The strings are only valid until the callback returns. Copy them
/// if you need them later.
///
/// This example logs each command's result:
///
/// ```ignore
/// fn on_semantic_prompt(handler: &mut Handler, event: SemanticPrompt) {
///     let _ = handler;
///     match event.kind {
///         SemanticPromptKind::CommandEnd => match event.exit_code {
///             Some(code) => log::info!("command exited with {code}"),
///             None => log::info!("command finished"),
///         },
///         _ => {}
///     }
/// }
///
/// let mut handler = terminal.vt_handler();
/// handler.effects.semantic_prompt = Some(on_semantic_prompt);
/// ```
// pub const SemanticPrompt = struct { ... };
//
// PORT NOTE: nested `Handler.SemanticPrompt`, flattened to module scope (the
// `UnknownSequence` precedent); its nested `Kind` / `PromptKind` become
// `SemanticPromptKind` / `SemanticPromptPromptKind`, the C names. The crate
// root re-exports this as `SemanticPrompt` (`main.zig:64`, DECISIONS P9);
// `stream::SemanticPrompt` stays the OSC command alias. The Zig field
// defaults are `SemanticPrompt::init`. `'b` is "only valid until the
// callback returns" (the `UnknownSequence` spelling).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SemanticPrompt<'b> {
    /// Which step of the command this event reports.
    // kind: Kind,
    pub kind: SemanticPromptKind,

    /// Which prompt is starting, for `prompt_start`. Always
    /// `primary` for other kinds.
    // prompt_kind: PromptKind = .primary,
    pub prompt_kind: SemanticPromptPromptKind,

    /// The command's exit code, for `command_end` when the shell
    /// reported one. Null otherwise.
    // exit_code: ?i32 = null,
    pub exit_code: Option<i32>,

    /// The command line about to run, for `output_start`. The shell
    /// sends it encoded, and this is the decoded text. Empty if the
    /// shell didn't send one or it couldn't be decoded.
    // command: []const u8 = "",
    pub command: &'b [u8],

    /// A description of what went wrong, for `command_end` when the
    /// shell sent one. Empty otherwise. Few shells send this, and the
    /// exit code is the usual way to tell whether a command failed.
    // err: []const u8 = "",
    pub err: &'b [u8],
}

impl<'b> SemanticPrompt<'b> {
    /// `.{ .kind = kind }` with every other field at its Zig default.
    pub const fn init(kind: SemanticPromptKind) -> SemanticPrompt<'b> {
        SemanticPrompt {
            kind,
            prompt_kind: SemanticPromptPromptKind::Primary,
            exit_code: None,
            command: b"",
            err: b"",
        }
    }
}

/// C: RusttySemanticPromptKind
// pub const Kind = lib.Enum(lib.target, &.{ ... });
// PORT NOTE: `lib.Enum`'s `.c` target is a `c_int` enum numbered in
// declaration order, hence `repr(i32)` (the `program_status::State` shape).
#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticPromptKind {
    // Never reported. This exists so that a zeroed C value is not
    // mistaken for a real event.
    Invalid,

    // The shell started drawing a prompt.
    PromptStart,

    // The prompt is drawn and the user can start typing.
    InputStart,

    // The user submitted the command and it started running.
    OutputStart,

    // The command finished running.
    CommandEnd,
}

/// C: RusttySemanticPromptPromptKind
// pub const PromptKind = lib.Enum(lib.target, &.{ ... });
#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticPromptPromptKind {
    // The main prompt shown before each command. This is used
    // when the shell doesn't say which prompt it is drawing.
    Primary,

    // A prompt drawn at the right edge of the line.
    Right,

    // A prompt at the start of an extra line of a command that
    // spans several lines.
    Continuation,

    // Another prompt for an extra line of input, such as bash's
    // PS2. Shells differ in whether they report extra lines as
    // continuation or secondary prompts, so most callers should
    // treat the two the same.
    Secondary,
}

/// Maximum byte length accepted for `terminfo_name`.
// pub const max_terminfo_name_bytes = 128;
//
// PORT NOTE: the reference nests this const inside `Handler`
// (`Handler.max_terminfo_name_bytes`); it lives at module scope here (the
// `UnknownSequence` flattening precedent) so the response-size const below can
// use it in a const context.
pub const MAX_TERMINFO_NAME_BYTES: usize = 128;

// Hex-encoded "TN", the XTGETTCAP key naming the terminfo entry.
// The static map also carries this key with Rustty's own name, so
// it is intercepted before the lookup: an embedder that never
// configured a name must not be reported as Rustty's entry.
// const encoded_tn_key = &std.fmt.bytesToHex("TN", .upper);
const ENCODED_TN_KEY: &[u8] = b"544E";

/// The static XTGETTCAP capability map.
// const map = comptime terminfo.ghostty.xtgettcapMap();
//
// PORT NOTE: the reference builds the map at comptime over the comptime
// terminfo Source; `rustty_terminfo::entry::rustty()` builds its Source at
// runtime (its names come from `rustty-identity`, R2), so the map is built
// once at first use (the same OnceLock idiom `rustty_terminfo::entry::
// version()` uses).
fn xtgettcap_map() -> &'static std::collections::HashMap<Vec<u8>, Vec<u8>> {
    static MAP: std::sync::OnceLock<std::collections::HashMap<Vec<u8>, Vec<u8>>> =
        std::sync::OnceLock::new();
    MAP.get_or_init(|| rustty_terminfo::entry::rustty().xtgettcap_map())
}

/// Callbacks for certain effects that handlers may have.
// pub const Effects = struct { ... }
pub struct Effects<'a> {
    /// Called when the terminal needs to write data back to the pty,
    /// e.g. in response to a DECRQM query. The data is only valid
    /// during the lifetime of the call so callers must copy it
    /// if it needs to be stored or used after the call returns.
    // write_pty: ?*const fn (*Handler, [:0]const u8) void
    pub write_pty: Option<fn(&mut Handler<'a>, &[u8])>,

    /// Called when the bell is rung (BEL).
    // bell: ?*const fn (*Handler) void
    pub bell: Option<fn(&mut Handler<'a>)>,

    /// Called when the running program writes to a clipboard.
    // clipboard_write: ?*const fn (*Handler, clipboard.Write) void
    // PORT NOTE (7ae9b1113): the pre-range `-> WriteResult` return became the
    // reply-based `clipboard.Write.reply` carried on the request.
    // PORT NOTE (soundness, parity S166-F1): the reference's reply state holds
    // `handler: *Handler` and applies the reply's side effects (the pty
    // response, and a `remember` grant) INSIDE `reply`, while the callback is
    // still running. Here the callback holds `&mut Handler`, a protected
    // argument for the whole call, so a handler pointer stored in the reply
    // state before the call could not be written through during it
    // (`rust-mut-args-strongly-protected`). Instead the callback passes its
    // own handler to `write.reply(handler, result)` / `read.reply(handler,
    // result)`, and the reply applies the side effects through a pointer
    // derived from that argument, inside `reply`, as the reference does. The
    // observable order is the reference's: the reply's response precedes pty
    // bytes the callback writes after replying, and a `remember` grant is
    // visible to the callback as soon as `reply` returns. Pinned by
    // `clipboard_read_reply_side_effects_happen_inside_reply` and
    // `kitty_clipboard_write_reply_grant_is_visible_inside_the_callback`.
    pub clipboard_write: Option<fn(&mut Handler<'a>, clipboard::Write)>,

    /// Called when the running program requests clipboard contents
    /// (OSC 52 with a "?" payload, or a Kitty clipboard (OSC 5522)
    /// read). Answering one lets the program read the user's
    /// clipboard, so the embedder is expected to mediate consent.
    ///
    /// Reads are synchronous: the callback must answer through
    /// `read.reply` before it returns, so an embedder that needs to
    /// ask the user must block (e.g. run a modal prompt) while the
    /// stream waits. Returning without a reply, or replying with any
    /// failure, answers the program with an empty clipboard (OSC 52)
    /// or the matching protocol status (OSC 5522) so it doesn't hang.
    /// If this is null, OSC 52 reads are ignored and OSC 5522 reads
    /// are refused with EPERM.
    ///
    /// OSC 5522 requests carry the program's MIME list, name, and
    /// password grant state; a reply that sets `remember` records a
    /// session grant so later requests with the same password arrive
    /// with `granted` set. Kitty itself serves a request for only the
    /// targets listing (`list` with no `mimes`) without prompting.
    ///
    /// Installing this also enables Kitty paste events (mode 5522):
    /// `paste` sends the program an event instead of the text, and
    /// the program's follow-up read arrives here with `granted` set
    /// since the user already pasted. See `paste`.
    // clipboard_read: ?*const fn (*Handler, clipboard.Read) void
    // PORT NOTE: the `reply(handler, result)` note on `clipboard_write` above
    // applies here too.
    pub clipboard_read: Option<fn(&mut Handler<'a>, clipboard::Read)>,

    /// Called when drag and drop protocol state changes in a way the
    /// embedder may need to act on: the running program registering
    /// or unregistering to accept drops, answering a drag, or
    /// concluding a drop. The event says what changed; the details
    /// are read from `handler.terminal.kitty_dnd` (Kitty's OSC 72 is
    /// the only drag and drop protocol today). Native drag events
    /// flow the other way, by calling `kitty.dnd.State` directly.
    // drag_and_drop: ?*const fn (*Handler, kitty_dnd.Event) void
    pub drag_and_drop: Option<fn(&mut Handler<'a>, kitty::dnd::Event)>,

    /// Called when the running program requests a desktop notification
    /// via OSC 9 or OSC 777. The title and body are borrowed and only
    /// valid for the duration of the callback.
    // desktop_notification: ?*const fn (*Handler, Action.ShowDesktopNotification) void
    //
    // PORT NOTE: the `for<'b>` is the Rust spelling of "borrowed and only valid
    // for the duration of the callback" — the title/body slices point into the
    // stream's own parse buffer, so they must NOT be tied to the handler's `'a`
    // (which outlives any one call). A higher-ranked bound is what stops a
    // callback from stashing them.
    pub desktop_notification:
        Option<for<'b> fn(&mut Handler<'a>, stream::ShowDesktopNotification<'b>)>,

    /// Called in response to a color scheme DSR query (CSI ? 996 n).
    /// Returns the current color scheme. Return null to silently
    /// ignore the query.
    // color_scheme: ?*const fn (*Handler) ?device_status.ColorScheme
    pub color_scheme: Option<fn(&mut Handler<'a>) -> Option<device_status::ColorScheme>>,

    /// Called in response to a device attributes query (CSI c,
    /// CSI > c, CSI = c). Returns the response to encode and
    /// write back to the pty.
    // device_attributes: ?*const fn (*Handler) device_attributes.Attributes
    pub device_attributes: Option<fn(&mut Handler<'a>) -> device_attributes::Attributes>,

    /// Called in response to ENQ (0x05). Returns the raw response
    /// bytes to write back to the pty. The returned memory must be
    /// valid for the lifetime of the call.
    // enquiry: ?*const fn (*Handler) []const u8
    pub enquiry: Option<fn(&mut Handler<'a>) -> &'a [u8]>,

    /// Called for XTWINOPS size queries (CSI 14/16/18 t) and when VT input
    /// enables in-band size reports (mode 2048). Returns the current
    /// terminal geometry used for encoding. Return null to suppress the
    /// XTWINOPS response or mode 2048 report.
    // size: ?*const fn (*Handler) ?size_report.Size
    pub size: Option<fn(&mut Handler<'a>) -> Option<size_report::Size>>,

    /// Called when the terminal title changes via escape sequences
    /// (e.g. OSC 0/2). The new title can be queried via
    /// handler.terminal.getTitle().
    // title_changed: ?*const fn (*Handler) void
    pub title_changed: Option<fn(&mut Handler<'a>)>,

    /// Called when the terminal pwd changes via escape sequences
    /// (e.g. OSC 7). The new pwd can be queried via
    /// handler.terminal.getPwd().
    // pwd_changed: ?*const fn (*Handler) void
    pub pwd_changed: Option<fn(&mut Handler<'a>)>,

    /// Called when the running program reports progress via OSC 9;4.
    // progress_report: ?*const fn (*Handler, osc.Command.ProgressReport) void
    pub progress_report: Option<fn(&mut Handler<'a>, osc::ProgressReport)>,

    /// Called when the running program sends a program status report
    /// (OSC 7501). Programs use these to say what they are doing, such
    /// as working, done, or waiting on the user. The report's
    /// text is only valid until this returns, so copy what you keep.
    ///
    /// A full reset (RIS) removes every record. When that happens, the
    /// terminal calls this with a `clear` report without an id, and
    /// then calls `reset`.
    ///
    /// Setting this also makes the terminal answer the support query,
    /// `OSC 7501 ; ?`, through `write_pty`. While this is null, the
    /// query gets no reply, so programs know not to send reports.
    // program_status: ?*const fn (*Handler, osc.Command.ProgramStatus.Report) void,
    //
    // PORT NOTE (SB, LIFETIMES.tsv `Handler.Effects program_status`): the
    // report borrows the OSC parser's capture (`for<'b>`, the
    // `desktop_notification` spelling). The callback may re-enter the
    // terminal through the handler's raw `terminal` alias; no `term()`
    // borrow is live across it.
    pub program_status:
        Option<for<'b> fn(&mut Handler<'a>, osc::parsers::program_status::Report<'b>)>,

    /// Called when the shell reports a step of a command through
    /// shell integration: a prompt starts, input starts, output
    /// starts, or the command ends. See `SemanticPrompt` for the
    /// steps and an example.
    ///
    /// The terminal has already applied the sequence when this is
    /// called. A sequence the terminal rejects is never reported.
    // semantic_prompt: ?*const fn (*Handler, SemanticPrompt) void,
    //
    // PORT NOTE (SB, LIFETIMES.tsv `Handler.Effects semantic_prompt`): `err`
    // borrows the OSC parser's capture and `command` a gpa buffer freed when
    // the callback returns (`for<'b>`). The terminal call that applies the
    // sequence ends before the callback runs, so no `&mut Terminal` is live
    // across it.
    pub semantic_prompt: Option<for<'b> fn(&mut Handler<'a>, SemanticPrompt<'b>)>,

    /// Called after the running program performs a full reset (RIS,
    /// `ESC c`). The terminal has already reset itself, which clears
    /// the screen, scrollback, title, and pwd. `title_changed` and
    /// `pwd_changed` are not called for this, so update anything that
    /// shows them here.
    ///
    /// A full reset also removes the progress report and program
    /// status records, and `progress_report` and `program_status` are
    /// called for that before this is called. A soft reset (DECSTR)
    /// doesn't call this.
    ///
    /// Shells don't report the end of a command that a reset
    /// interrupts, so clear any state you keep for the current
    /// command here.
    // reset: ?*const fn (*Handler) void,
    //
    // PORT NOTE (SB, LIFETIMES.tsv `Handler.Effects reset`): `full_reset`
    // drops every `term()` borrow before this runs (the `render_hold` rule).
    pub reset: Option<fn(&mut Handler<'a>)>,

    /// Called in response to an XTVERSION query. Returns the version
    /// string to report (e.g. "ghostty 1.2.3"). The returned memory
    /// must be valid for the lifetime of the call. The maximum length
    /// is 256 bytes; longer strings will be silently ignored.
    // xtversion: ?*const fn (*Handler) []const u8
    pub xtversion: Option<fn(&mut Handler<'a>) -> &'a [u8]>,

    /// Called with `true` when the running program asks the terminal
    /// to stop updating the screen, and with `false` when it allows
    /// updates again. The time in between is a "render hold". Programs
    /// use a hold so that the user never sees a half-drawn frame.
    ///
    /// Today the only source of a hold is synchronized output (mode
    /// 2026). The hold begins when VT input sets the mode. It ends
    /// when VT input resets the mode, on a full reset, and on a resize
    /// through `Handler.resize`. The calls always come in pairs:
    /// setting the mode during a hold does nothing, and neither does
    /// resetting it when there is no hold. Writing `terminal.modes`
    /// directly never calls this.
    ///
    /// When a hold begins, nothing after the sequence that began it
    /// has been processed yet, so the terminal contains exactly the
    /// frame the program wants left on screen. A renderer can capture
    /// that frame from within this callback (e.g. `RenderState.update`)
    /// and then skip updates until the hold ends. Checking the mode
    /// before each draw instead can't do this. It leaves whatever was
    /// drawn last on screen, and it loses a finished frame entirely
    /// when one hold ends and the next begins between two draws.
    ///
    /// The terminal has no clock so it never ends a hold on its own.
    /// The caller must use a timeout so that a program that never
    /// releases its hold can't freeze the screen.
    // render_hold: ?*const fn (*Handler, bool) void,
    //
    // PORT NOTE (SB, LIFETIMES.tsv `Handler.Effects render_hold`): the
    // callback may re-enter the terminal (`RenderState::update` through
    // `handler.terminal`), so every call site (`do_set_mode`, `resize`,
    // `full_reset`) drops its `term()` borrow before `render_hold` runs; no
    // `&mut Terminal`/`Screen`/`Page` is live across the callback.
    pub render_hold: Option<fn(&mut Handler<'a>, bool)>,
}

impl<'a> Effects<'a> {
    /// No effects means that the stream effectively becomes readonly
    /// that only affects pure terminal state and ignores all side
    /// effects beyond that.
    // pub const readonly: Effects = .{ ... all null ... };
    pub const READONLY: Effects<'a> = Effects {
        bell: None,
        clipboard_read: None,
        clipboard_write: None,
        color_scheme: None,
        desktop_notification: None,
        device_attributes: None,
        drag_and_drop: None,
        enquiry: None,
        progress_report: None,
        program_status: None,
        reset: None,
        semantic_prompt: None,
        size: None,
        render_hold: None,
        title_changed: None,
        pwd_changed: None,
        write_pty: None,
        xtversion: None,
    };
}

impl<'a> Default for Effects<'a> {
    // effects: Effects = .readonly
    fn default() -> Effects<'a> {
        Effects::READONLY
    }
}

impl<'a> Handler<'a> {
    // pub fn init(terminal: *Terminal) Handler {
    //     return .{ .terminal = terminal };
    // }
    pub fn init(terminal: *mut Terminal) -> Handler<'a> {
        Handler {
            terminal,
            semantic_failure: false,
            effects: Effects::READONLY,
            // title_report: false (the reference field default)
            title_report: false,
            // apc_handler: .{} (the reference field default)
            apc_handler: ApcHandler::default(),
            // dcs_handler: .{} (the reference field default)
            dcs_handler: crate::dcs::Handler::default(),
            // kitty_clipboard_write: ... = null (the reference field default)
            kitty_clipboard_write: None,
            // kitty_clipboard_grants: .{} (the reference field default)
            kitty_clipboard_grants: kitty::clipboard::Grants::default(),
            // ... = kitty_clipboard.max_write_size (the reference field default)
            kitty_clipboard_write_max_bytes: kitty::clipboard::max_write_size,
            // unknown_sequence: ... = null (the reference field default)
            unknown_sequence: None,
            // terminfo_name: ... = null (the reference field default)
            terminfo_name: None,
            _phantom: PhantomData,
        }
    }

    // fn programStatusReport(
    //     self: *Handler,
    //     report: osc.Command.ProgramStatus.Report,
    // ) void {
    //     const func = self.effects.program_status orelse return;
    //     func(self, report);
    // }
    fn program_status_report(&mut self, report: osc::parsers::program_status::Report<'_>) {
        let Some(func) = self.effects.program_status else {
            return;
        };
        func(self, report);
    }

    // fn unknownSequence(self: *Handler, value: UnknownSequence) void {
    //     const func = self.unknown_sequence orelse return;
    //     func(self, value);
    // }
    fn unknown_sequence(&mut self, value: UnknownSequence) {
        let func = match self.unknown_sequence {
            Some(func) => func,
            None => return,
        };
        func(self, value);
    }

    /// Resize the terminal and apply any side effects (if supported)
    /// as a result of that.
    ///
    /// This is different than a direct `Terminal.resize` operation
    /// because it also handles the side effects like mode 2048 in-band
    /// size reports if write_pty is set.
    //
    // PORT NOTE (area D §6.5): this stays on `stream_terminal::Handler`. It
    // touches only `Terminal` + `Effects`, both already in `rustty-vt`; moving
    // it up a tier would force a `rustty-vt` -> apprt edge (R4).
    pub fn resize(
        &mut self,
        value: crate::terminal::Resize,
    ) -> Result<(), crate::terminal::ResizeError> {
        // Resize always turns off synchronized output, ending its hold.
        // const sync = self.terminal.modes.get(.synchronized_output);
        let sync = self.term().modes.get(modes::Mode::SynchronizedOutput);
        // try self.terminal.resize(self.terminal.gpa(), value);
        let gpa = self.term().gpa();
        self.term().resize(gpa, value)?;
        // if (sync) self.renderHold(false);
        if sync {
            self.render_hold(false);
        }

        // Mode 2048 reports require complete, current cell pixel geometry.
        // const cell_size = value.cell_size_px orelse return;
        let Some(cell_size) = value.cell_size_px else {
            return Ok(());
        };

        // If we have no in-band size reports enabled then do nothing.
        if !self.term().modes.get(modes::Mode::InBandSizeReports) {
            return Ok(());
        }

        // If we have no write_pty effect, do nothing.
        if self.effects.write_pty.is_none() {
            return Ok(());
        }

        // The maximum mode-2048 response is covered by size_report's maximum
        // value test. Reserve the final byte for the callback's sentinel.
        //
        // PORT NOTE: the reference reserves `buf[writer.end]` for the Zig
        // `[:0]const u8` sentinel the callback takes. rustty's `write_pty`
        // callback takes a plain `&[u8]` (see the `Effects::write_pty` PORT
        // NOTE), so the reserved byte is kept — the encoder still only gets
        // `buf.len() - 1` bytes, so the capacity budget is identical — but no
        // NUL is appended.
        let mut buf = [0u8; 128];
        let end = {
            let mut writer = crate::fixed_writer::FixedWriter::new(&mut buf[..127]);
            // size_report.encode(...) catch unreachable;
            size_report::encode(
                &mut writer,
                size_report::Style::Mode2048,
                size_report::Size {
                    rows: value.rows,
                    columns: value.cols,
                    cell_width: cell_size.width,
                    cell_height: cell_size.height,
                },
            )
            .expect("mode 2048 report always fits in 127 bytes");
            writer.end
        };
        self.write_pty(&buf[..end]);

        Ok(())
    }

    // PORT NOTE: the reference's `vt`/`vtFallible` comptime dispatcher (a single
    // `switch (action)` over `Action.Tag`) is distributed across the
    // `impl stream::Handler for Handler` methods below: the Rust `Stream`
    // calls the individual trait methods directly (monomorphized per action),
    // so each `vtFallible` arm becomes the corresponding trait-method body. The
    // OSC/CSI helper functions the arms delegate to (`writePty`/`setMode`/
    // `horizontalTab`/...) are the inherent methods below.

    // pub fn vt(
    //     self: *Handler,
    //     comptime action: Action.Tag,
    //     value: Action.Value(action),
    // ) void {
    //     self.vtFallible(action, value) catch |err| {
    //         log.warn("error handling VT action action={} err={}", .{ action, err });
    //     };
    // }
    //
    // PORT NOTE: the reference's `vt` wraps the fallible per-action dispatch and
    // swallows+logs any error. Since the dispatch is distributed across the
    // trait-method arms here, `vt` becomes a small helper each fallible arm
    // funnels its `Result`-returning Terminal call through: on `Err` it logs and
    // continues (the `catch |err| log.warn(...)` behavior), so a transient
    // allocation failure never propagates out of the stream.
    #[inline]
    fn vt<F>(&mut self, action: &'static str, f: F)
    where
        F: FnOnce(&mut Terminal) -> Result<(), crate::pagelist::OutOfMemory>,
    {
        let r = f(self.term());
        self.vt_catch(action, r);
    }

    // The body of the reference's single `vtFallible(...) catch |err| { ... }`:
    //
    //     self.semantic_failure = true;
    //     log.warn("error handling VT action action={} err={}", .{ action, err });
    //
    // PORT NOTE: because the dispatch is distributed across the trait-method
    // arms, this is the shared catch every FALLIBLE arm funnels its `Result`
    // through. Arms the reference deliberately keeps GRACEFUL
    // (`color_operation`, `kitty_color_report`, `clipboard_contents`) must NOT
    // route through here — they log and continue without setting the flag.
    //
    // PORT NOTE: the reference's `catch` is over `vtFallible`'s INFERRED error
    // set (`!void`), i.e. it catches whatever any arm can fail with, not one
    // concrete error. The Rust analog is therefore generic over the error type:
    // arms whose Terminal call fails with something other than `OutOfMemory`
    // (e.g. `Screen::start_hyperlink`'s `IncreaseCapacityError`) funnel through
    // the very same catch instead of dropping their `Result`.
    #[inline]
    fn vt_catch<E: std::fmt::Debug>(&mut self, action: &'static str, r: Result<(), E>) {
        if let Err(err) = r {
            // self.semantic_failure = true;
            self.semantic_failure = true;
            log::warn!(target: LOG_SCOPE, "error handling VT action action={action} err={err:?}");
        }
    }

    // PORT NOTE: convenience accessor for the `*mut Terminal` deref. Each call
    // re-derefs the raw pointer (no `&mut Terminal` is held across re-reads, per
    // the soundness rule); callers must not nest two live `term()` borrows.
    #[inline]
    #[allow(clippy::mut_from_ref)]
    fn term(&self) -> &mut Terminal {
        // SAFETY: `self.terminal` aliases the `Terminal` that created this
        // `Handler` (via `Terminal::vt_handler`); the `Stream` owns the
        // `Handler` and the borrow checker cannot express that self-referential
        // aliasing (mirrors `ScreenSet`'s `active: *mut Screen`). The deref is
        // short-lived and never overlaps another live `&mut Terminal`.
        unsafe { &mut *self.terminal }
    }

    // inline fn writePty(self: *Handler, data: [:0]const u8) void {
    //     const func = self.effects.write_pty orelse return;
    //     func(self, data);
    // }
    //
    // PORT NOTE (vt-12b-7a): the `write_pty` callback takes `&[u8]`. The
    // reference's `[:0]const u8` sentinel is a Zig encoding detail of the
    // response buffers (filled in vt-12b-7b alongside the response arms); the
    // callback boundary itself only needs the byte slice.
    #[inline]
    fn write_pty(&mut self, data: &[u8]) {
        let Some(func) = self.effects.write_pty else {
            return;
        };
        func(self, data);
    }
}

// impl stream::Handler for Handler ------------------------------------------
//
// PORT NOTE: each method body is the corresponding per-action arm of the
// reference `vtFallible` switch (plus the OSC/DCS/APC helpers): the
// state-mutating arms landed in vt-12b-7a and the response/query arms in
// vt-12b-7b. The APC end + kitty/glyph arms remain stubbed (font-blocked,
// deferred past vt-12; see DEFERRALS.md).
/// A paste request; see `paste`.
// pub const Paste = paste_pkg.Request;
pub type Paste<'a> = paste_pkg::Request<'a>;

// pub const PasteError = Allocator.Error || std.Io.RandomSecureError || error{...};
// PORT NOTE: nested `Handler.PasteError`; flattened to module scope (the
// `UnknownSequence` precedent). `Allocator.Error` -> `OutOfMemory`;
// `std.Io.RandomSecureError` -> `EntropyUnavailable`/`Canceled` (see
// `paste::Error` for the member mapping).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasteError {
    /// `Allocator.Error` — `error.OutOfMemory`.
    OutOfMemory,

    /// `std.Io.RandomSecureError` — `error.EntropyUnavailable`.
    EntropyUnavailable,

    /// `std.Io.RandomSecureError` — `error.Canceled` (parity member; the
    /// sys.rs entropy stand-in never produces it).
    Canceled,

    /// The data could inject commands and allow_unsafe was false.
    /// Nothing was written.
    UnsafePaste,

    /// The contents reader failed. Nothing was written.
    ReadFailed,

    /// No write_pty effect is set, so nothing can be written.
    NoWritePty,
}

/// The size of the chunks a paste streams to write_pty in.
// pub const paste_chunk_size = 4096;
// PORT NOTE: nested `Handler.paste_chunk_size`; module scope (the
// `MAX_TERMINFO_NAME_BYTES` precedent).
pub const PASTE_CHUNK_SIZE: usize = 4096;

impl<'a> stream::Handler for Handler<'a> {
    // pub fn deinit(self: *Handler) void {
    //     self.kittyClipboardAbort();
    //     self.kitty_clipboard_grants.deinit(self.terminal.gpa());
    //     self.apc_handler.deinit();
    //     self.dcs_handler.deinit();
    // }
    fn deinit(&mut self) {
        self.kitty_clipboard_abort();
        let gpa = self.term().gpa();
        self.kitty_clipboard_grants.deinit(gpa);
        self.apc_handler.deinit();
        self.dcs_handler.deinit();
    }

    // PORT NOTE: the reference has no `vtRaw` declared on this handler, so the
    // stream's `@hasDecl(T, "vtRaw")` branch is skipped (no raw interception).
    // The trait's default returns `false`; we keep the explicit override as a
    // no-op for fidelity with that "not declared" state.
    //
    // WS10 T14: for the same reason `HAS_VT_RAW` is deliberately left at its
    // `false` default here and must NOT be set. This handler is the one the
    // real IO thread and the `terminal-stream` benchmark use, so it is exactly
    // the handler the CSI fast paths exist for. The no-op `vt_raw` override
    // below is fidelity decoration, not a declaration.
    fn vt_raw(&mut self, action: &crate::parser::Action) -> bool {
        false
    }

    // .print => try self.terminal.print(value.cp),
    fn print(&mut self, cp: u32) {
        self.vt("print", |t| t.print(cp));
    }

    // .print_slice => try self.terminal.printSlice(value.cps),
    fn print_slice(&mut self, cps: &[u32]) {
        self.vt("print_slice", |t| t.print_slice(cps));
    }

    // .print_repeat => try self.terminal.printRepeat(value),
    fn print_repeat(&mut self, count: usize) {
        self.vt("print_repeat", |t| t.print_repeat(count));
    }

    // .bell => self.bell(),
    fn bell(&mut self) {
        self.do_bell();
    }

    // .backspace => self.terminal.backspace(),
    fn backspace(&mut self) {
        self.term().backspace();
    }

    // .horizontal_tab => self.horizontalTab(value),
    fn horizontal_tab(&mut self, count: u16) {
        self.do_horizontal_tab(count);
    }

    // .horizontal_tab_back => self.horizontalTabBack(value),
    fn horizontal_tab_back(&mut self, count: u16) {
        self.do_horizontal_tab_back(count);
    }

    // .linefeed => try self.terminal.linefeed(),
    fn linefeed(&mut self) {
        self.vt("linefeed", |t| t.linefeed());
    }

    // .carriage_return => self.terminal.carriageReturn(),
    fn carriage_return(&mut self) {
        self.term().carriage_return();
    }

    // .enquiry => self.reportEnquiry(),
    fn enquiry(&mut self) {
        self.report_enquiry();
    }

    // .invoke_charset => self.terminal.invokeCharset(value.bank, value.charset, value.locking),
    fn invoke_charset(&mut self, v: stream::InvokeCharset) {
        self.term().invoke_charset(v.bank, v.charset, v.locking);
    }

    // .cursor_up => self.terminal.cursorUp(value.value),
    fn cursor_up(&mut self, v: stream::CursorMovement) {
        self.term().cursor_up(v.value as usize);
    }

    // .cursor_down => self.terminal.cursorDown(value.value),
    fn cursor_down(&mut self, v: stream::CursorMovement) {
        self.term().cursor_down(v.value as usize);
    }

    // .cursor_left => self.terminal.cursorLeft(value.value),
    fn cursor_left(&mut self, v: stream::CursorMovement) {
        self.term().cursor_left(v.value as usize);
    }

    // .cursor_right => self.terminal.cursorRight(value.value),
    fn cursor_right(&mut self, v: stream::CursorMovement) {
        self.term().cursor_right(v.value as usize);
    }

    // .cursor_col => self.terminal.setCursorPos(self.terminal.screens.active.cursor.y + 1, value.value),
    fn cursor_col(&mut self, v: stream::CursorMovement) {
        let y = self.term().active_mut().cursor.y;
        self.term().set_cursor_pos(y as usize + 1, v.value as usize);
    }

    // .cursor_row => self.terminal.setCursorPos(value.value, self.terminal.screens.active.cursor.x + 1),
    fn cursor_row(&mut self, v: stream::CursorMovement) {
        let x = self.term().active_mut().cursor.x;
        self.term().set_cursor_pos(v.value as usize, x as usize + 1);
    }

    // .cursor_col_relative => self.terminal.setCursorPos(
    //     self.terminal.screens.active.cursor.y + 1,
    //     self.terminal.screens.active.cursor.x + 1 +| value.value,
    // ),
    fn cursor_col_relative(&mut self, v: stream::CursorMovement) {
        let y = self.term().active_mut().cursor.y as usize;
        let x = self.term().active_mut().cursor.x as usize;
        self.term()
            .set_cursor_pos(y + 1, (x + 1).saturating_add(v.value as usize));
    }

    // .cursor_row_relative => self.terminal.setCursorPos(
    //     self.terminal.screens.active.cursor.y + 1 +| value.value,
    //     self.terminal.screens.active.cursor.x + 1,
    // ),
    fn cursor_row_relative(&mut self, v: stream::CursorMovement) {
        let y = self.term().active_mut().cursor.y as usize;
        let x = self.term().active_mut().cursor.x as usize;
        self.term()
            .set_cursor_pos((y + 1).saturating_add(v.value as usize), x + 1);
    }

    // .cursor_pos => self.terminal.setCursorPos(value.row, value.col),
    fn cursor_pos(&mut self, v: stream::CursorPos) {
        self.term().set_cursor_pos(v.row as usize, v.col as usize);
    }

    // .cursor_style => self.terminal.setCursorStyle(value),
    //
    // PORT NOTE (oracle c594031d5): the DECSCUSR style/blink resolution and the
    // `default_cursor*` fields it read moved onto `Terminal` itself. Do NOT
    // re-add them here: two owners would double-apply DECSCUSR.
    fn cursor_style(&mut self, v: ansi::CursorStyle) {
        self.term().set_cursor_style(v);
    }

    // .erase_display_below => self.terminal.eraseDisplay(.below, value),
    fn erase_display_below(&mut self, protected: bool) {
        self.term()
            .erase_display(csi::EraseDisplay::Below, protected);
    }

    // .erase_display_above => self.terminal.eraseDisplay(.above, value),
    fn erase_display_above(&mut self, protected: bool) {
        self.term()
            .erase_display(csi::EraseDisplay::Above, protected);
    }

    // .erase_display_complete => self.terminal.eraseDisplay(.complete, value),
    fn erase_display_complete(&mut self, protected: bool) {
        self.term()
            .erase_display(csi::EraseDisplay::Complete, protected);
    }

    // .erase_display_scrollback => self.terminal.eraseDisplay(.scrollback, value),
    fn erase_display_scrollback(&mut self, protected: bool) {
        self.term()
            .erase_display(csi::EraseDisplay::Scrollback, protected);
    }

    // .erase_display_scroll_complete => self.terminal.eraseDisplay(.scroll_complete, value),
    fn erase_display_scroll_complete(&mut self, protected: bool) {
        self.term()
            .erase_display(csi::EraseDisplay::ScrollComplete, protected);
    }

    // .erase_line_right => self.terminal.eraseLine(.right, value),
    fn erase_line_right(&mut self, protected: bool) {
        self.term().erase_line(csi::EraseLine::Right, protected);
    }

    // .erase_line_left => self.terminal.eraseLine(.left, value),
    fn erase_line_left(&mut self, protected: bool) {
        self.term().erase_line(csi::EraseLine::Left, protected);
    }

    // .erase_line_complete => self.terminal.eraseLine(.complete, value),
    fn erase_line_complete(&mut self, protected: bool) {
        self.term().erase_line(csi::EraseLine::Complete, protected);
    }

    // .erase_line_right_unless_pending_wrap => self.terminal.eraseLine(.right_unless_pending_wrap, value),
    fn erase_line_right_unless_pending_wrap(&mut self, protected: bool) {
        self.term()
            .erase_line(csi::EraseLine::RightUnlessPendingWrap, protected);
    }

    // .delete_chars => self.terminal.deleteChars(value),
    fn delete_chars(&mut self, count: usize) {
        self.term().delete_chars(count);
    }

    // .erase_chars => self.terminal.eraseChars(value),
    fn erase_chars(&mut self, count: usize) {
        self.term().erase_chars(count);
    }

    // .insert_lines => self.terminal.insertLines(value),
    fn insert_lines(&mut self, count: usize) {
        self.term().insert_lines(count);
    }

    // .insert_blanks => self.terminal.insertBlanks(value),
    fn insert_blanks(&mut self, count: usize) {
        self.term().insert_blanks(count);
    }

    // .delete_lines => self.terminal.deleteLines(value),
    fn delete_lines(&mut self, count: usize) {
        self.term().delete_lines(count);
    }

    // .scroll_up => try self.terminal.scrollUp(value),
    fn scroll_up(&mut self, count: usize) {
        self.vt("scroll_up", |t| t.scroll_up(count));
    }

    // .scroll_down => self.terminal.scrollDown(value),
    fn scroll_down(&mut self, count: usize) {
        self.term().scroll_down(count);
    }

    // .tab_clear_current => self.terminal.tabClear(.current),
    fn tab_clear_current(&mut self) {
        self.term().tab_clear(csi::TabClear::Current);
    }

    // .tab_clear_all => self.terminal.tabClear(.all),
    fn tab_clear_all(&mut self) {
        self.term().tab_clear(csi::TabClear::All);
    }

    // .tab_set => self.terminal.tabSet(),
    fn tab_set(&mut self) {
        self.term().tab_set();
    }

    // .tab_reset => self.terminal.tabReset(),
    fn tab_reset(&mut self) {
        self.term().tab_reset();
    }

    // .index => try self.terminal.index(),
    fn index(&mut self) {
        self.vt("index", |t| t.index());
    }

    // .next_line => {
    //     try self.terminal.index();
    //     self.terminal.carriageReturn();
    // },
    fn next_line(&mut self) {
        self.vt("index", |t| t.index());
        self.term().carriage_return();
    }

    // .reverse_index => self.terminal.reverseIndex(),
    fn reverse_index(&mut self) {
        self.term().reverse_index();
    }

    // .full_reset => {
    //     // A reset turns off synchronized output, ending its hold.
    //     const sync = self.terminal.modes.get(.synchronized_output);
    //     self.terminal.fullReset();
    //     if (sync) self.renderHold(false);
    //
    //     // Full reset clears grants
    //     self.kitty_clipboard_grants.deinit(self.terminal.gpa());
    //     self.kitty_clipboard_grants = .{};
    //
    //     // Clear the progress bar and program status records
    //     self.progressReport(.{ .state = .remove });
    //     self.programStatusReport(.{ .state = .clear });
    //
    //     if (self.effects.reset) |func| func(self);
    // },
    //
    // PORT NOTE (oracle c594031d5): `Terminal::full_reset` now performs the
    // cursor-default restore itself (`setCursorStyle(.default)`).
    //
    // PORT NOTE (SB): every `term()` borrow above ends before the
    // `progress_report`, `program_status` and `reset` effects run, so a
    // callback that re-enters the terminal through `handler.terminal` holds
    // the only live reference (the `render_hold` rule).
    fn full_reset(&mut self) {
        // A reset turns off synchronized output, ending its hold.
        let sync = self.term().modes.get(modes::Mode::SynchronizedOutput);
        self.term().full_reset();
        if sync {
            self.render_hold(false);
        }

        // Full reset clears grants
        let gpa = self.term().gpa();
        self.kitty_clipboard_grants.deinit(gpa);
        self.kitty_clipboard_grants = kitty::clipboard::Grants::default();

        // Clear the progress bar and program status records (eeab85b96; the
        // b56c6d88f "only if there is one" guard was reverted in-range by
        // b7ee3ab6b — end state is the unconditional remove).
        if let Some(func) = self.effects.progress_report {
            func(
                self,
                osc::ProgressReport {
                    state: osc::ProgressReportState::Remove,
                    progress: None,
                },
            );
        }
        // self.programStatusReport(.{ .state = .clear });
        self.program_status_report(osc::parsers::program_status::Report {
            state: osc::parsers::program_status::State::Clear,
            data: b"",
        });

        // if (self.effects.reset) |func| func(self);
        if let Some(func) = self.effects.reset {
            func(self);
        }
    }

    // .set_mode => try self.setMode(value.mode, true),
    //
    // PORT NOTE: `setMode` is `try`'d inside `vtFallible` (its alt-screen arms
    // `try switchScreenMode` and its `132_column` arm `try deccolm`), so a mode
    // failure reaches the shared catch. Funnel the `Result` through `vt_catch`
    // instead of dropping it — same shape as the `dcs_hook` arm below.
    fn set_mode(&mut self, v: stream::Mode) {
        let r = self.do_set_mode(v.mode, true);
        self.vt_catch("set_mode", r);
    }

    // .reset_mode => try self.setMode(value.mode, false),
    fn reset_mode(&mut self, v: stream::Mode) {
        let r = self.do_set_mode(v.mode, false);
        self.vt_catch("reset_mode", r);
    }

    // .save_mode => self.terminal.modes.save(value.mode),
    fn save_mode(&mut self, v: stream::Mode) {
        self.term().modes.save(v.mode);
    }

    // .restore_mode => {
    //     const prev = self.terminal.modes.get(value.mode);
    //     const v = self.terminal.modes.restore(value.mode);
    //
    //     // Restore writes the value directly. Put the old value
    //     // back for synchronized output so that setMode can see
    //     // the change and report the render hold.
    //     if (value.mode == .synchronized_output) {
    //         self.terminal.modes.set(value.mode, prev);
    //     }
    //
    //     try self.setMode(value.mode, v);
    // },
    fn restore_mode(&mut self, v: stream::Mode) {
        let prev = self.term().modes.get(v.mode);
        let enabled = self.term().modes.restore(v.mode);

        // Restore writes the value directly. Put the old value
        // back for synchronized output so that setMode can see
        // the change and report the render hold.
        if v.mode == modes::Mode::SynchronizedOutput {
            self.term().modes.set(v.mode, prev);
        }

        let r = self.do_set_mode(v.mode, enabled);
        self.vt_catch("restore_mode", r);
    }

    // .request_mode => self.requestMode(value.mode),
    fn request_mode(&mut self, v: stream::Mode) {
        self.do_request_mode(v.mode);
    }

    // .request_mode_unknown => self.requestModeUnknown(value.mode, value.ansi),
    fn request_mode_unknown(&mut self, v: stream::RawMode) {
        self.do_request_mode_unknown(v.mode, v.ansi);
    }

    // .top_and_bottom_margin => self.terminal.setTopAndBottomMargin(value.top_left, value.bottom_right),
    fn top_and_bottom_margin(&mut self, v: stream::Margin) {
        self.term()
            .set_top_and_bottom_margin(v.top_left as usize, v.bottom_right as usize);
    }

    // .left_and_right_margin => self.terminal.setLeftAndRightMargin(value.top_left, value.bottom_right),
    fn left_and_right_margin(&mut self, v: stream::Margin) {
        self.term()
            .set_left_and_right_margin(v.top_left as usize, v.bottom_right as usize);
    }

    // .left_and_right_margin_ambiguous => {
    //     if (self.terminal.modes.get(.enable_left_and_right_margin)) {
    //         self.terminal.setLeftAndRightMargin(0, 0);
    //     } else {
    //         self.terminal.saveCursor();
    //     }
    // },
    fn left_and_right_margin_ambiguous(&mut self) {
        if self.term().modes.get(modes::Mode::EnableLeftAndRightMargin) {
            self.term().set_left_and_right_margin(0, 0);
        } else {
            self.term().save_cursor();
        }
    }

    // .save_cursor => self.terminal.saveCursor(),
    fn save_cursor(&mut self) {
        self.term().save_cursor();
    }

    // .restore_cursor => self.terminal.restoreCursor(),
    fn restore_cursor(&mut self) {
        self.term().restore_cursor();
    }

    // .modify_key_format => {
    //     self.terminal.flags.modify_other_keys_2 = false;
    //     switch (value) {
    //         .other_keys_numeric => self.terminal.flags.modify_other_keys_2 = true,
    //         else => {},
    //     }
    // },
    fn modify_key_format(&mut self, v: ansi::ModifyKeyFormat) {
        self.term().flags.modify_other_keys_2 = false;
        // PORT NOTE: the reference switch has a single non-default arm
        // (`.other_keys_numeric => ...; else => {}`); clippy recognizes that as
        // an equality test, so it is written as an `if` here.
        if v == ansi::ModifyKeyFormat::OtherKeysNumeric {
            self.term().flags.modify_other_keys_2 = true;
        }
    }

    // .mouse_shift_capture => self.terminal.flags.mouse_shift_capture = if (value) .true else .false,
    fn mouse_shift_capture(&mut self, capture: bool) {
        self.term().flags.mouse_shift_capture = if capture {
            crate::terminal::MouseShiftCapture::True
        } else {
            crate::terminal::MouseShiftCapture::False
        };
    }

    // .protected_mode_off => self.terminal.setProtectedMode(.off),
    fn protected_mode_off(&mut self) {
        self.term().set_protected_mode(ansi::ProtectedMode::Off);
    }

    // .protected_mode_iso => self.terminal.setProtectedMode(.iso),
    fn protected_mode_iso(&mut self) {
        self.term().set_protected_mode(ansi::ProtectedMode::Iso);
    }

    // .protected_mode_dec => self.terminal.setProtectedMode(.dec),
    fn protected_mode_dec(&mut self) {
        self.term().set_protected_mode(ansi::ProtectedMode::Dec);
    }

    // .size_report => self.reportSize(value),
    fn size_report(&mut self, v: csi::SizeReportStyle) {
        self.report_size(v);
    }

    // .title_push, .title_pop => {} (have no terminal-modifying effect)
    fn title_push(&mut self, v: u16) {}

    // .title_push, .title_pop => {} (have no terminal-modifying effect)
    fn title_pop(&mut self, v: u16) {}

    // .resize_window => {} (has no terminal-modifying effect)
    fn resize_window(&mut self, v: stream::ResizeWindow) {}

    // .xtversion => self.reportXtversion(),
    fn xtversion(&mut self) {
        self.report_xtversion();
    }

    // .device_attributes => self.reportDeviceAttributes(value),
    fn device_attributes(&mut self, req: device_attributes::Req) {
        self.report_device_attributes(req);
    }

    // .device_status => self.deviceStatus(value.request),
    fn device_status(&mut self, v: stream::DeviceStatus) {
        Handler::device_status(self, v.request);
    }

    // .kitty_keyboard_query => self.queryKittyKeyboard(),
    fn kitty_keyboard_query(&mut self) {
        self.query_kitty_keyboard();
    }

    // .kitty_keyboard_push => self.terminal.screens.active.kitty_keyboard.push(value.flags),
    fn kitty_keyboard_push(&mut self, v: stream::KittyKeyboardFlags) {
        self.term().active_mut().kitty_keyboard.push(v.flags);
    }

    // .kitty_keyboard_pop => self.terminal.screens.active.kitty_keyboard.pop(@intCast(value)),
    fn kitty_keyboard_pop(&mut self, v: u16) {
        self.term().active_mut().kitty_keyboard.pop(v as usize);
    }

    // .kitty_keyboard_set => self.terminal.screens.active.kitty_keyboard.set(.set, value.flags),
    fn kitty_keyboard_set(&mut self, v: stream::KittyKeyboardFlags) {
        self.term()
            .active_mut()
            .kitty_keyboard
            .set(kitty::key::SetMode::Set, v.flags);
    }

    // .kitty_keyboard_set_or => self.terminal.screens.active.kitty_keyboard.set(.@"or", value.flags),
    fn kitty_keyboard_set_or(&mut self, v: stream::KittyKeyboardFlags) {
        self.term()
            .active_mut()
            .kitty_keyboard
            .set(kitty::key::SetMode::Or, v.flags);
    }

    // .kitty_keyboard_set_not => self.terminal.screens.active.kitty_keyboard.set(.not, value.flags),
    fn kitty_keyboard_set_not(&mut self, v: stream::KittyKeyboardFlags) {
        self.term()
            .active_mut()
            .kitty_keyboard
            .set(kitty::key::SetMode::Not, v.flags);
    }

    // .dcs_hook => try self.dcsHook(value),
    fn dcs_hook(&mut self, dcs: crate::parser::DCS) {
        let r = self.do_dcs_hook(dcs);
        self.vt_catch("dcs_hook", r);
    }

    // .dcs_put => try self.dcsPut(value),
    fn dcs_put(&mut self, code: u8) {
        let r = self.do_dcs_put(code);
        self.vt_catch("dcs_put", r);
    }

    // .dcs_unhook => try self.dcsUnhook(),
    fn dcs_unhook(&mut self) {
        let r = self.do_dcs_unhook();
        self.vt_catch("dcs_unhook", r);
    }

    // .apc_start => self.apc_handler.start(),
    fn apc_start(&mut self) {
        self.apc_handler.start();
    }

    // .apc_put => self.apc_handler.feed(self.terminal.gpa(), value),
    fn apc_put(&mut self, code: u8) {
        let gpa = self.term().gpa();
        self.apc_handler.feed(gpa, code);
    }

    // .apc_put_slice => self.apc_handler.feedSlice(self.terminal.gpa(), value.bytes),
    fn apc_put_slice(&mut self, bytes: &[u8]) {
        let gpa = self.term().gpa();
        self.apc_handler.feed_slice(gpa, bytes);
    }

    // .apc_end => self.apcEnd(value.terminated),
    fn apc_end(&mut self, v: stream::ApcEnd) {
        Handler::apc_end(self, v.terminated);
    }

    // .start_hyperlink => try self.terminal.screens.active.startHyperlink(value.uri, value.id),
    //
    // PORT NOTE: `startHyperlink` is `try`'d inside `vtFallible`, so its failure
    // (OOM or page-capacity exhaustion) reaches the shared catch and sets
    // `semantic_failure`. Route the `Result` through `vt_catch` rather than
    // dropping it; the error type is `IncreaseCapacityError`, not `OutOfMemory`,
    // which is why `vt_catch` is generic over the error (see its PORT NOTE).
    fn start_hyperlink(&mut self, v: stream::StartHyperlink) {
        let r = self.term().active_mut().start_hyperlink(v.uri, v.id);
        self.vt_catch("start_hyperlink", r);
    }

    // .end_hyperlink => self.terminal.screens.active.endHyperlink(),
    fn end_hyperlink(&mut self) {
        self.term().active_mut().end_hyperlink();
    }

    // .active_status_display => self.terminal.status_display = value,
    fn active_status_display(&mut self, v: ansi::StatusDisplay) {
        self.term().status_display = v;
    }

    // .decaln => try self.terminal.decaln(),
    fn decaln(&mut self) {
        self.vt("decaln", |t| t.decaln());
    }

    // .window_title => try self.windowTitle(value.title),
    fn window_title(&mut self, v: stream::WindowTitle) {
        let r = self.do_window_title(v.title);
        self.vt_catch("window_title", r);
    }

    // .report_pwd => try self.reportPwd(value.url),
    fn report_pwd(&mut self, v: stream::ReportPwd) {
        let r = self.do_report_pwd(v.url);
        self.vt_catch("report_pwd", r);
    }

    // .show_desktop_notification => self.desktopNotification(value),
    //
    // Was a deliberate no-op stub matching the pin; wired to the
    // `desktop_notification` effect by oracle c3655ba25.
    fn show_desktop_notification(&mut self, v: stream::ShowDesktopNotification) {
        // fn desktopNotification(self, notification) void {
        //     const func = self.effects.desktop_notification orelse return;
        //     func(self, notification);
        // }
        let Some(func) = self.effects.desktop_notification else {
            return;
        };
        func(self, v);
    }

    // .progress_report => self.progressReport(value),
    //
    // Was a deliberate no-op stub matching the pin; wired to the
    // `progress_report` effect by oracle 47d602c42.
    fn progress_report(&mut self, v: osc::ProgressReport) {
        // fn progressReport(self, report) void {
        //     const func = self.effects.progress_report orelse return;
        //     func(self, report);
        // }
        let Some(func) = self.effects.progress_report else {
            return;
        };
        func(self, v);
    }

    // fn clipboardContents(self: *Handler, kind: u8, data: []const u8) !void
    //
    // PORT NOTE: the reference is `!void` and its dispatch arm swallows the
    // error with `catch |err| { ... log.warn("error handling clipboard write
    // err={}", .{err}); }` because:
    //
    //   Clipboard writes are external effects, not terminal state.
    //
    // The port's `do_clipboard_contents` returns `Result<(), OutOfMemory>`
    // and `clipboard_contents`, the dispatch arm, logs the error the same way
    // (parity tail S7-4).
    // A malformed payload is a silent no-op (the callback is not invoked). The
    // decode scratch is allocated through the terminal's allocator, fallibly,
    // and on failure the callback is not invoked, which is what the
    // reference's `try` + dispatch-arm `catch` pair does. The oracle test
    // "clipboard_write allocation failure is ignored" is ported as written:
    //
    //   Only the decoded scratch data uses the terminal allocator here. Swap in
    //   an allocator that always fails, then restore it before terminal teardown.
    // .clipboard_contents => self.clipboardContents(
    //     value.kind,
    //     value.data,
    //     value.terminator,
    // ) catch |err| {
    //     // Clipboard operations are external effects, not terminal state.
    //     log.warn("error handling clipboard operation err={}", .{err});
    // },
    fn clipboard_contents(&mut self, v: stream::ClipboardContents) {
        if let Err(err) = self.do_clipboard_contents(v.kind, v.data, v.terminator) {
            log::warn!(target: LOG_SCOPE, "error handling clipboard operation err={err:?}");
        }
    }

    // .kitty_clipboard => self.kittyClipboard(value) catch |err| {
    //     // Clipboard operations are external effects, not terminal
    //     // state; a failed transaction was already answered.
    //     log.warn("error handling kitty clipboard err={}", .{err});
    // },
    fn kitty_clipboard(&mut self, v: osc::KittyClipboardProtocol) {
        if let Err(err) = self.do_kitty_clipboard(&v) {
            log::warn!(target: LOG_SCOPE, "error handling kitty clipboard err={err:?}");
        }
    }

    // .kitty_dnd => self.kittyDnd(value) catch |err| {
    //     // Drag and drop is a self-contained subsystem: an OOM
    //     // updating its state or a failure writing a response
    //     // degrades it without corrupting terminal state, so we log.
    //     log.warn("error handling kitty dnd err={}", .{err});
    // },
    fn kitty_dnd(&mut self, v: osc::KittyDndProtocol) {
        if let Err(err) = self.do_kitty_dnd(&v) {
            log::warn!(target: LOG_SCOPE, "error handling kitty dnd err={err:?}");
        }
    }

    // .mouse_shape => self.terminal.mouse_shape = value,
    fn mouse_shape(&mut self, v: crate::mouse::Shape) {
        self.term().mouse_shape = v;
    }

    // .mouse_shape_reset => self.terminal.mouse_shape = .text,
    fn mouse_shape_reset(&mut self) {
        self.term().mouse_shape = crate::mouse::Shape::Text;
    }

    // .configure_charset => self.terminal.configureCharset(value.slot, value.charset),
    fn configure_charset(&mut self, v: stream::ConfigureCharset) {
        self.term().configure_charset(v.slot, v.charset);
    }

    // .set_attribute => switch (value) {
    //     .unknown => {},
    //     else => try self.terminal.setAttribute(value),
    // },
    //
    // PORT NOTE (oracle 439d35e27): PROMOTED from `catch {}` to propagating —
    // the cursor pen is terminal-owned semantic state.
    fn set_attribute(&mut self, attr: sgr::Attribute) {
        match attr {
            sgr::Attribute::Unknown { .. } => {}
            _ => {
                let r = self
                    .term()
                    .set_attribute(attr)
                    .map_err(|_| crate::pagelist::OutOfMemory);
                self.vt_catch("set_attribute", r);
            }
        }
    }

    // .kitty_color_report => self.kittyColorOperation(value) catch |err| {
    //     log.warn("error reporting Kitty colors err={}", .{err});
    // },
    //
    // PORT NOTE (oracle 439d35e27): stays GRACEFUL. A failure here is a query
    // RESPONSE failure, not a divergence of terminal-owned state, so it must
    // NOT route through `vt_catch` / set `semantic_failure`.
    fn kitty_color_report(&mut self, v: &kitty::color::OSC) {
        if let Err(err) = self.do_kitty_color_operation(v) {
            log::warn!(target: LOG_SCOPE, "error reporting Kitty colors err={err:?}");
        }
    }

    // .color_operation => self.colorOperation(
    //     &value.requests,
    //     value.terminator,
    // ) catch |err| log.warn("error reporting OSC color err={}", .{err}),
    //
    // PORT NOTE (oracle 439d35e27): stays GRACEFUL; see `kitty_color_report`.
    fn color_operation(&mut self, v: stream::ColorOperation<'_>) {
        if let Err(err) = self.do_color_operation(v.requests, v.terminator) {
            log::warn!(target: LOG_SCOPE, "error reporting OSC color err={err:?}");
        }
    }

    // .semantic_prompt => try self.semanticPrompt(value),
    //
    // fn semanticPrompt(self: *Handler, cmd: osc.Command.SemanticPrompt) !void {
    //     try self.terminal.semanticPrompt(cmd);
    //     const func = self.effects.semantic_prompt orelse return;
    //     switch (cmd.action) { ... }
    // }
    //
    // PORT NOTE (7bb45ba34): the effect fires only when the terminal accepted
    // the sequence. The `vt` helper logs and swallows the `Result`, so the
    // body branches on it itself: an `Err` takes the shared catch
    // (`vt_catch`, which sets `semantic_failure` and logs) and returns before
    // the effect, as the reference's `try` does. `cmd` is `Copy`, so the
    // terminal call takes a copy and the effect still reads the options.
    fn semantic_prompt(&mut self, cmd: stream::SemanticPrompt) {
        use crate::osc::semantic_prompt::{option, Action, PromptKind};

        // try self.terminal.semanticPrompt(cmd);
        let r = self.term().semantic_prompt(cmd);
        if r.is_err() {
            self.vt_catch("semantic_prompt", r);
            return;
        }
        // const func = self.effects.semantic_prompt orelse return;
        let Some(func) = self.effects.semantic_prompt else {
            return;
        };
        match cmd.action {
            Action::FreshLine => {}

            // A and N accept the same k= option as P, and the terminal
            // applies it, so report it for all three.
            Action::FreshLineNewPrompt | Action::NewCommand | Action::PromptStart => func(
                self,
                SemanticPrompt {
                    prompt_kind: match cmd.read_option(option::PROMPT_KIND) {
                        Some(PromptKind::Initial) => SemanticPromptPromptKind::Primary,
                        Some(PromptKind::Right) => SemanticPromptPromptKind::Right,
                        Some(PromptKind::Continuation) => SemanticPromptPromptKind::Continuation,
                        Some(PromptKind::Secondary) => SemanticPromptPromptKind::Secondary,
                        None => SemanticPromptPromptKind::Primary,
                    },
                    ..SemanticPrompt::init(SemanticPromptKind::PromptStart)
                },
            ),

            Action::EndPromptStartInput | Action::EndPromptStartInputTerminateEol => {
                func(self, SemanticPrompt::init(SemanticPromptKind::InputStart))
            }

            Action::EndInputStartOutput => {
                // Decoding never makes the command line longer, so a
                // buffer the size of the raw options always fits it. If
                // the buffer can't be allocated or the command line
                // can't be decoded, we still report the step with an
                // empty command.
                // const alloc = self.terminal.gpa();
                // const buf = alloc.alloc(u8, cmd.options_unvalidated.len) catch |err| { ... };
                // defer alloc.free(buf);
                let alloc = self.term().gpa();
                let mut buf =
                    match kitty::clipboard_command::alloc_buf(alloc, cmd.options_unvalidated.len())
                    {
                        Ok(buf) => buf,
                        Err(err) => {
                            // log.warn("error allocating semantic prompt command line err={}", .{err});
                            log::warn!(
                                target: LOG_SCOPE,
                                "error allocating semantic prompt command line err={err:?}"
                            );
                            func(self, SemanticPrompt::init(SemanticPromptKind::OutputStart));
                            return;
                        }
                    };
                // var writer: std.Io.Writer = .fixed(buf);
                // const command: []const u8 = if (cmd.writeCommandLine(&writer))
                //     writer.buffered()
                // else |_|
                //     "";
                let mut writer = rustty_os::string_encoding::FixedWriter::fixed(&mut buf);
                let command: &[u8] = match cmd.write_command_line(&mut writer) {
                    Ok(()) => writer.buffered(),
                    Err(_) => b"",
                };
                func(
                    self,
                    SemanticPrompt {
                        command,
                        ..SemanticPrompt::init(SemanticPromptKind::OutputStart)
                    },
                );
            }

            Action::EndCommand => func(
                self,
                SemanticPrompt {
                    exit_code: cmd.read_option(option::EXIT_CODE),
                    err: cmd.read_option(option::ERR).unwrap_or(b""),
                    ..SemanticPrompt::init(SemanticPromptKind::CommandEnd)
                },
            ),
        }
    }

    // Unrecognized OSC. The OSC parser already dropped aborted
    // sequences, so everything that reaches here is reported.
    // .osc_unknown => self.unknownSequence(.{ .osc = value }),
    fn osc_unknown(&mut self, v: osc::Unknown<'_>) {
        Handler::unknown_sequence(self, UnknownSequence::Osc(v));
    }

    // .program_status => self.programStatus(value),
    //
    // fn programStatus(self: *Handler, cmd: osc.Command.ProgramStatus) void {
    fn program_status(&mut self, cmd: stream::ProgramStatus<'_>) {
        use crate::osc::parsers::program_status::Command;
        match cmd {
            // .report => |report| self.programStatusReport(report),
            Command::Report(report) => self.program_status_report(report),

            // Only claim support when something handles the reports. The
            // reply is always the same fixed bytes. The specification
            // never allows sending report contents back to the program.
            // .query => |terminator| if (self.effects.program_status != null) {
            //     switch (terminator) {
            //         inline else => |t| self.writePty(
            //             "\x1b]7501;?" ++ comptime t.string(),
            //         ),
            //     }
            // },
            Command::Query(terminator) => {
                if self.effects.program_status.is_some() {
                    // PORT NOTE: the comptime `++` concatenation, as a
                    // `const` per terminator.
                    const ST: &[u8] = b"\x1b]7501;?\x1b\\";
                    const BEL: &[u8] = b"\x1b]7501;?\x07";
                    self.write_pty(match terminator {
                        osc::Terminator::St => ST,
                        osc::Terminator::Bel => BEL,
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
