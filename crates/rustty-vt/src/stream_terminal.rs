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

// const log = std.log.scoped(.stream_terminal);
const LOG_SCOPE: &str = "stream_terminal";

/// This is a Stream implementation that processes actions against
/// a Terminal and updates the Terminal state.
// pub const Stream = stream.Stream(Handler);
// PORT NOTE: the reference `stream.Stream(Handler)` becomes the generic
// `stream::Stream<'a, Handler<'a>>`. The `'a` threads the OSC parser allocator
// lifetime (see `stream::Stream::init_alloc`) through both the stream and the
// handler's effect callbacks.
pub type Stream<'a> = stream::Stream<'a, Handler<'a>>;

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
    pub kitty_clipboard_write: Option<Box<kitty::clipboard::WriteState>>,

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

    /// Called for sequence identifiers not supported by this library.
    /// Currently, only APC is reported. Content is borrowed and only valid
    /// for the duration of the callback. Set `apc_handler.unknown_max_bytes`
    /// before starting the Stream to enable APC capture.
    // unknown_sequence: ?*const fn (*Handler, UnknownSequence) void = null,
    //
    // PORT NOTE: the `for<'b>` is the same "borrowed and only valid for the
    // duration of the callback" spelling the `desktop_notification` effect
    // uses — the content slice points into the APC handler's own buffer.
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

/// A sequence unsupported by the active handler. Payload data is borrowed
/// only for the duration of the handler callback.
// pub const UnknownSequence = union(enum) {
//     apc: String,
//     ...
// };
// PORT NOTE: the reference nests this type (and its `String` payload struct)
// inside `Handler`; Rust types cannot nest in a struct, so both live at module
// scope with the `Handler.`-qualified names flattened (`UnknownSequenceString`
// for `UnknownSequence.String` — `String` alone would collide with
// `std::string::String`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnknownSequence<'b> {
    Apc(UnknownSequenceString<'b>),
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

    /// Called in response to an XTVERSION query. Returns the version
    /// string to report (e.g. "ghostty 1.2.3"). The returned memory
    /// must be valid for the lifetime of the call. The maximum length
    /// is 256 bytes; longer strings will be silently ignored.
    // xtversion: ?*const fn (*Handler) []const u8
    pub xtversion: Option<fn(&mut Handler<'a>) -> &'a [u8]>,
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
        size: None,
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

// PORT NOTE (P3-W4 T5): a fault-injection seam for `do_color_operation`. See
// the PORT FINDING inside that function: rustty's response writer cannot fail,
// so without this the "graceful arms do not set `semantic_failure`" control
// would pass whether or not the re-partition was ported. Same
// `#[cfg(test)] thread_local!` idiom as the crate's other tripwire modules.
#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum ColorOperationFp {
    Respond,
}

#[cfg(test)]
thread_local! {
    pub(crate) static COLOR_OPERATION_TW: core::cell::RefCell<
        rustty_util::tripwire::Module<ColorOperationFp, crate::pagelist::OutOfMemory>,
    > = core::cell::RefCell::new(rustty_util::tripwire::module());
}

#[cfg(test)]
#[inline]
fn color_operation_tw_check(point: ColorOperationFp) -> Result<(), crate::pagelist::OutOfMemory> {
    COLOR_OPERATION_TW.with(|tw| tw.borrow_mut().check(point))
}

#[cfg(test)]
pub(crate) fn color_operation_tw_error_always(point: ColorOperationFp) {
    COLOR_OPERATION_TW.with(|tw| {
        tw.borrow_mut()
            .errorAlways(point, crate::pagelist::OutOfMemory)
    });
}

#[cfg(test)]
pub(crate) fn color_operation_tw_end(
    mode: rustty_util::tripwire::ResetMode,
) -> Result<(), rustty_util::tripwire::UntrippedError> {
    COLOR_OPERATION_TW.with(|tw| tw.borrow_mut().end(mode))
}

// PORT NOTE: a fault-injection seam for the OSC 52 decode scratch allocation in
// `clipboardContents`. The reference injects `testing.failing_allocator` by
// swapping `t.screens.active.alloc` (oracle test "clipboard_write allocation
// failure is ignored", stream_terminal.zig:2309-2339); rustty's allocator seam
// is the zero-sized `allocator_api2::alloc::Global`, so the allocator itself
// cannot be swapped out. This tripwire is the port's stand-in and sits exactly
// where the reference's `try alloc.alloc(u8, decoded_len)` can fail. Same
// `#[cfg(test)] thread_local!` idiom as `COLOR_OPERATION_TW` above.
#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum ClipboardContentsFp {
    DecodeScratch,
}

#[cfg(test)]
thread_local! {
    pub(crate) static CLIPBOARD_CONTENTS_TW: core::cell::RefCell<
        rustty_util::tripwire::Module<ClipboardContentsFp, crate::pagelist::OutOfMemory>,
    > = core::cell::RefCell::new(rustty_util::tripwire::module());
}

#[cfg(test)]
#[inline]
fn clipboard_contents_tw_check(
    point: ClipboardContentsFp,
) -> Result<(), crate::pagelist::OutOfMemory> {
    CLIPBOARD_CONTENTS_TW.with(|tw| tw.borrow_mut().check(point))
}

#[cfg(test)]
pub(crate) fn clipboard_contents_tw_error_always(point: ClipboardContentsFp) {
    CLIPBOARD_CONTENTS_TW.with(|tw| {
        tw.borrow_mut()
            .errorAlways(point, crate::pagelist::OutOfMemory)
    });
}

#[cfg(test)]
pub(crate) fn clipboard_contents_tw_end(
    mode: rustty_util::tripwire::ResetMode,
) -> Result<(), rustty_util::tripwire::UntrippedError> {
    CLIPBOARD_CONTENTS_TW.with(|tw| tw.borrow_mut().end(mode))
}

// PORT NOTE: a fault-injection seam for the Kitty clipboard (OSC 5522) write
// transaction allocation in `kitty_clipboard_write_begin`. The reference test
// "kitty clipboard allocation failure is ignored" swaps `t.screens.active.alloc`
// for `testing.failing_allocator` so `alloc.create(WriteState)` fails; rustty's
// `Box::new` cannot fail, so this tripwire sits exactly where that `try` can.
// Same `#[cfg(test)] thread_local!` idiom as `CLIPBOARD_CONTENTS_TW` above.
#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum KittyClipboardFp {
    CreateWriteState,
}

#[cfg(test)]
thread_local! {
    pub(crate) static KITTY_CLIPBOARD_TW: core::cell::RefCell<
        rustty_util::tripwire::Module<KittyClipboardFp, crate::pagelist::OutOfMemory>,
    > = core::cell::RefCell::new(rustty_util::tripwire::module());
}

#[cfg(test)]
#[inline]
fn kitty_clipboard_tw_check(point: KittyClipboardFp) -> Result<(), crate::pagelist::OutOfMemory> {
    KITTY_CLIPBOARD_TW.with(|tw| tw.borrow_mut().check(point))
}

#[cfg(test)]
pub(crate) fn kitty_clipboard_tw_error_always(point: KittyClipboardFp) {
    KITTY_CLIPBOARD_TW.with(|tw| {
        tw.borrow_mut()
            .errorAlways(point, crate::pagelist::OutOfMemory)
    });
}

#[cfg(test)]
pub(crate) fn kitty_clipboard_tw_end(
    mode: rustty_util::tripwire::ResetMode,
) -> Result<(), rustty_util::tripwire::UntrippedError> {
    KITTY_CLIPBOARD_TW.with(|tw| tw.borrow_mut().end(mode))
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
        // try self.terminal.resize(self.terminal.gpa(), value);
        let gpa = self.term().gpa();
        self.term().resize(gpa, value)?;

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
    fn vt<F>(&mut self, f: F)
    where
        F: FnOnce(&mut Terminal) -> Result<(), crate::pagelist::OutOfMemory>,
    {
        let r = f(self.term());
        self.vt_catch(r);
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
    fn vt_catch<E>(&mut self, r: Result<(), E>) {
        if r.is_err() {
            // self.semantic_failure = true;
            self.semantic_failure = true;
            // log.warn("error handling VT action action={} err={}", .{ action, err });
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

    /// Paste into the terminal, applying the terminal's current state
    /// as necessary to owner mode 5522, bracketed paste, unsafe paste, etc.
    /// Returns true if anything was written to the pty.
    ///
    /// The output streams to write_pty in chunks of `paste_chunk_size`.
    /// The contents are read at most once and only the pasted text
    /// representation is ever read, buffered whole while it is checked
    /// and encoded; see `terminal.paste`.
    // pub fn paste(self: *Handler, req: Paste) PasteError!bool {
    //
    // PORT NOTE (SB discipline): the reference `PtyWriter` holds `*Handler`
    // and drains through `handler.writePty` DURING the paste call, while the
    // paste context also holds `&grants` into the same Handler — mirroring
    // that would interleave a raw Handler deref with a live field borrow
    // (the `shaper-hook-sb-clean-interleave` hazard). The port's `PtyChunker`
    // therefore queues the identical chunk sequence and the handler delivers
    // it through `write_pty` immediately after the paste call returns —
    // the observable call sequence (count, sizes, order, delivery on error
    // too: "a partial paste has its frame closed") is unchanged, and no raw
    // Handler alias exists while the grants borrow is live.
    pub fn paste(&mut self, req: Paste<'_>) -> Result<bool, PasteError> {
        // if (self.effects.write_pty == null) return error.NoWritePty;
        if self.effects.write_pty.is_none() {
            return Err(PasteError::NoWritePty);
        }

        // var buf: [paste_chunk_size]u8 = undefined;
        // var pty: PtyWriter = .init(self, &buf);
        let mut pty = PtyChunker::new();

        // SAFETY: `self.terminal` aliases the Terminal that created this
        // Handler (see `term()`); the shared borrow lives only for the paste
        // call and does not overlap a `&mut Terminal`.
        let terminal: &Terminal = unsafe { &*self.terminal };
        let io = terminal.io;

        let result = paste_pkg::paste(
            paste_pkg::Context {
                terminal,
                // Paste events need the program's follow-up Kitty
                // clipboard read served.
                // .kitty_clipboard = if (self.effects.clipboard_read != null) .{
                //     .grants = &self.kitty_clipboard_grants,
                //     .io = self.terminal.io(),
                // } else null,
                kitty_clipboard: if self.effects.clipboard_read.is_some() {
                    Some(paste_pkg::KittyClipboard {
                        grants: &mut self.kitty_clipboard_grants,
                        io,
                    })
                } else {
                    None
                },
                writer: &mut pty,
            },
            req,
        );

        // Delivered on error too: a partial paste has its frame closed
        // and the program must see that.
        // defer pty.writer.flush() catch unreachable;
        pty.flush();
        for chunk in &pty.chunks {
            let Some(func) = self.effects.write_pty else {
                break;
            };
            func(self, chunk);
        }

        // ... catch |err| switch (err) {
        //     // The pty writer never fails.
        //     error.WriteFailed => unreachable,
        //     error.ReadFailed, error.OutOfMemory, error.UnsafePaste,
        //     error.EntropyUnavailable, error.Canceled, => |e| e,
        // };
        result.map_err(|err| match err {
            paste_pkg::Error::WriteFailed => unreachable!("the pty writer never fails"),
            paste_pkg::Error::ReadFailed => PasteError::ReadFailed,
            paste_pkg::Error::OutOfMemory => PasteError::OutOfMemory,
            paste_pkg::Error::UnsafePaste => PasteError::UnsafePaste,
            paste_pkg::Error::EntropyUnavailable => PasteError::EntropyUnavailable,
            paste_pkg::Error::Canceled => PasteError::Canceled,
        })
    }

    // fn clipboardContents(
    //     self: *Handler,
    //     kind: u8,
    //     data: []const u8,
    //     terminator: osc.Terminator,
    // ) !void
    //
    // PORT NOTE: the reference is `!void` and its dispatch arm swallows the
    // error ("Clipboard operations are external effects, not terminal
    // state."); rustty's arm is infallible — the fallible decode-scratch
    // allocation is ported over the `CLIPBOARD_CONTENTS_TW` seam (see its
    // module note), and every failure returns without invoking the effect,
    // exactly what the reference's `try` + graceful dispatch catch does.
    fn do_clipboard_contents(&mut self, kind: u8, data: &[u8], terminator: osc::Terminator) {
        // const location: clipboard.Location = switch (kind) { ... };
        let location = match kind {
            b's' => clipboard::Location::Selection,
            b'p' => clipboard::Location::Primary,
            _ => clipboard::Location::Standard,
        };

        // OSC 52 uses a "?" payload to request the clipboard contents.
        if data.len() == 1 && data[0] == b'?' {
            self.do_clipboard_read(location, terminator);
            return;
        }

        // const func = self.effects.clipboard_write orelse return;
        let Some(func) = self.effects.clipboard_write else {
            return;
        };

        // OSC 52 uses an empty payload to clear the selected clipboard.
        if data.is_empty() {
            func(
                self,
                clipboard::Write {
                    location,
                    contents: &[],
                    name: b"",
                    granted: false,
                    can_remember: false,
                    reply_ctx: core::ptr::null_mut(),
                    reply_fn: ignore_write_reply,
                },
            );
            return;
        }

        // Decode the base64 payload with the SIMD decoder (the same one
        // used for Kitty clipboard payloads) rather than the scalar std
        // implementation; clipboard payloads can be megabytes. The
        // Kitty clipboard spec governs OSC 52 base64 handling too: a
        // request with characters outside the base64 alphabet is
        // discarded entirely (never partially decoded), while a
        // missing-padding tail is tolerated since OSC 52 has no way
        // to report errors to the client.
        // const alloc = self.terminal.gpa();
        // const buf = try alloc.alloc(u8, simd.base64.maxLen(data));
        // defer alloc.free(buf);
        let alloc = self.term().gpa();
        let max_len = rustty_simd::base64::max_len(data);
        let mut buf: allocator_api2::vec::Vec<u8, _> = allocator_api2::vec::Vec::new_in(alloc);
        #[cfg(test)]
        if clipboard_contents_tw_check(ClipboardContentsFp::DecodeScratch).is_err() {
            return;
        }
        if buf.try_reserve_exact(max_len).is_err() {
            return;
        }
        // The capacity above is already reserved, so this never allocates.
        buf.resize(max_len, 0);
        // const decoded = simd.base64.decodeStrict(data, buf, .optional) catch {
        //     log.warn("OSC 52 clipboard write is not valid base64, ignoring", .{});
        //     return;
        // };
        let Ok(decoded) = rustty_simd::base64::decode_strict(
            data,
            &mut buf,
            rustty_simd::base64::Padding::Optional,
        ) else {
            log::warn!(target: LOG_SCOPE, "OSC 52 clipboard write is not valid base64, ignoring");
            return;
        };

        // const contents = [_]clipboard.Content{.{ .mime = "text/plain", .data = decoded }};
        let contents = [clipboard::Content {
            mime: b"text/plain",
            data: decoded,
        }];
        func(
            self,
            clipboard::Write {
                location,
                contents: &contents,
                name: b"",
                granted: false,
                can_remember: false,
                reply_ctx: core::ptr::null_mut(),
                reply_fn: ignore_write_reply,
            },
        );
    }

    // fn clipboardRead(
    //     self: *Handler,
    //     location: clipboard.Location,
    //     terminator: osc.Terminator,
    // ) void
    fn do_clipboard_read(&mut self, location: clipboard::Location, terminator: osc::Terminator) {
        // const func = self.effects.clipboard_read orelse return;
        let Some(func) = self.effects.clipboard_read else {
            return;
        };

        // var state: ClipboardReadState = .{ ... };
        let mut state = ClipboardReadState {
            location,
            terminator,
            replied: false,
            response: Vec::new(),
        };
        func(
            self,
            clipboard::Read {
                location,
                mimes: &[b"text/plain"],
                list: false,
                name: b"",
                granted: false,
                can_remember: false,
                reply_ctx: (&mut state) as *mut ClipboardReadState as *mut core::ffi::c_void,
                reply_fn: ClipboardReadState::reply,
            },
        );

        // The program is waiting on us, so a callback that returned
        // without a (successful) reply gets an empty clipboard rather
        // than silence.
        // if (!state.replied) state.respond("") catch ...;
        if !state.replied {
            state.respond(b"");
        }
        // PORT NOTE (SB discipline): the reference reply state carries
        // `handler: *Handler` and writes the pty from INSIDE the effect
        // callback; the port's reply only ENCODES into the stack state (no
        // Handler alias while the callback's `&mut Handler` is live) and the
        // handler delivers the bytes here, immediately after the callback —
        // the observable pty write sequence is unchanged.
        let response = core::mem::take(&mut state.response);
        if !response.is_empty() {
            self.write_pty(&response);
        }
    }

    /// Handle one Kitty clipboard protocol (OSC 5522) packet.
    // fn kittyClipboard(self: *Handler, v: Action.Value(.kitty_clipboard))
    //     error{OutOfMemory}!void
    fn do_kitty_clipboard(&mut self, v: &osc::KittyClipboardProtocol) {
        // Decode and validate the metadata.
        // var arena: std.heap.ArenaAllocator = .init(self.terminal.gpa());
        // defer arena.deinit();
        // (PORT NOTE: the arena scopes the Metadata allocations; the Rust
        // Metadata owns its Vecs and frees on drop.)
        use crate::osc::parsers::kitty_clipboard_protocol::Operation;
        let meta = match kitty::clipboard::Metadata::parse(&v.metadata) {
            Ok(Some(meta)) => meta,
            Ok(None) => return,
            Err(kitty::clipboard::ParseError::OutOfMemory) => return,
            Err(kitty::clipboard::ParseError::InvalidValue) => {
                // const state = self.kitty_clipboard_write orelse return;
                // switch (kitty_clipboard.Metadata.operation(v.metadata) orelse return) {
                //     .wdata, .walias => self.kittyClipboardFinish(state, .EINVAL, v.terminator),
                //     .read, .write => {},
                // }
                if self.kitty_clipboard_write.is_none() {
                    return;
                }
                match kitty::clipboard::Metadata::operation(&v.metadata) {
                    Some(Operation::Wdata) | Some(Operation::Walias) => {
                        if let Some(state) = self.kitty_clipboard_write.take() {
                            self.kitty_clipboard_finish(
                                &state,
                                kitty::clipboard::Status::Einval,
                                v.terminator,
                            );
                        }
                    }
                    Some(Operation::Read) | Some(Operation::Write) | None => {}
                }
                return;
            }
        };

        // const payload = v.payload orelse "";
        let payload: &[u8] = v.payload.as_deref().unwrap_or(b"");
        match meta.op {
            Operation::Read => self.kitty_clipboard_read(&meta, payload, v.terminator),
            Operation::Write => self.kitty_clipboard_write_begin(&meta, v.terminator),
            Operation::Wdata => self.kitty_clipboard_data(&meta, payload, v.terminator),
            Operation::Walias => self.kitty_clipboard_alias(&meta, payload, v.terminator),
        }
    }

    // fn kittyClipboardRead(self, meta, payload, terminator) error{OutOfMemory}!void
    fn kitty_clipboard_read(
        &mut self,
        meta: &kitty::clipboard::Metadata,
        payload: &[u8],
        terminator: osc::Terminator,
    ) {
        // The payload is the requested MIME list. Kitty drops a read
        // request with an undecodable payload without any response.
        let decoded = match kitty::clipboard::Payload::init(payload) {
            Ok(decoded) => decoded,
            Err(_) => return,
        };
        if !decoded.is_valid_utf8() {
            return;
        }

        // Without a clipboard_read effect nothing can serve the read.
        // EPERM is the protocol's denial so clients degrade gracefully.
        let Some(func) = self.effects.clipboard_read else {
            self.kitty_clipboard_respond(&kitty::clipboard::Response {
                id: &meta.id,
                terminator,
                ..kitty::clipboard::Response::new(
                    crate::osc::parsers::kitty_clipboard_protocol::Operation::Read,
                    kitty::clipboard::Status::Eperm,
                )
            });
            return;
        };

        // The targets type ('.') asks for the listing of available
        // types rather than data. Requested types beyond the cap are
        // dropped and simply never served, which is how the protocol
        // reports an unavailable type anyway.
        // var mimes_buf: [kitty_clipboard.max_read_mimes][]const u8 = undefined;
        let mut mimes_buf: [&[u8]; kitty::clipboard::max_read_mimes] =
            [b""; kitty::clipboard::max_read_mimes];
        let (mimes, list): (&[&[u8]], bool) = {
            let mut targets = false;
            let mut len: usize = 0;
            let mut it = decoded.mime_iterator();
            while let Some(mime) = it.next() {
                if mime == kitty::clipboard::targets_mime {
                    targets = true;
                    continue;
                }
                if len == mimes_buf.len() {
                    continue;
                }
                mimes_buf[len] = mime;
                len += 1;
            }
            (&mimes_buf[0..len], targets)
        };

        // Per the spec a password without a name is no password. A
        // stored grant for it lets the embedder skip its prompt. A
        // prompt-exempt request never consults the grants: the
        // embedder serves it without a prompt anyway, and consuming a
        // one-time paste password on a listing would burn the grant
        // before the follow-up data read.
        let pw: &[u8] = if !meta.name.is_empty() { &meta.pw } else { b"" };
        let granted = !kitty::clipboard::read_prompt_exempt(mimes.len())
            && self
                .kitty_clipboard_grants
                .use_(pw, kitty::clipboard::Direction::Read);

        // var state: KittyClipboardReadState = .{ ... };
        let mut state = KittyClipboardReadState {
            primary: meta.loc == clipboard::Location::Primary,
            id: meta.id.clone(),
            pw: pw.to_vec(),
            mimes: mimes.iter().map(|m| m.to_vec()).collect(),
            list,
            terminator,
            write_pty_some: self.effects.write_pty.is_some(),
            replied: false,
            remember: false,
            response: Vec::new(),
        };
        func(
            self,
            clipboard::Read {
                location: meta.loc,
                mimes,
                list,
                name: &meta.name,
                granted,
                can_remember: !pw.is_empty(),
                reply_ctx: (&mut state) as *mut KittyClipboardReadState as *mut core::ffi::c_void,
                reply_fn: KittyClipboardReadState::reply,
            },
        );

        // The program is waiting on us, so a callback that returned
        // without a reply is answered as a denial rather than silence.
        // if (!state.replied) state.respondStatus(.EPERM);
        if !state.replied {
            state.respond_status(kitty::clipboard::Status::Eperm);
        }

        // PORT NOTE (SB discipline): the reference reply state carries
        // `handler: *Handler` and grants/writes from INSIDE the effect
        // callback; the port's reply only records into the stack state and
        // the handler applies the grant + pty write here, immediately after
        // the callback — the observable sequence is unchanged (see
        // `do_clipboard_read`).
        if state.remember && !state.pw.is_empty() {
            // self.handler.kitty_clipboard_grants.grant(..., self.pw, .read, false)
            self.kitty_clipboard_grants
                .grant(&state.pw, kitty::clipboard::Direction::Read, false);
        }
        let response = core::mem::take(&mut state.response);
        if !response.is_empty() {
            self.write_pty(&response);
        }
    }

    // fn kittyClipboardWriteBegin(self, meta, terminator) error{OutOfMemory}!void
    fn kitty_clipboard_write_begin(
        &mut self,
        meta: &kitty::clipboard::Metadata,
        terminator: osc::Terminator,
    ) {
        // A new write silently replaces any in-flight transaction.
        self.kitty_clipboard_abort();

        // Without a clipboard_write effect a commit can never succeed,
        // so fail the transaction up front instead of spooling data
        // we'd only throw away. Later wdata packets are ignored.
        if self.effects.clipboard_write.is_none() {
            self.kitty_clipboard_respond(&kitty::clipboard::Response {
                id: &meta.id,
                terminator,
                ..kitty::clipboard::Response::new(
                    crate::osc::parsers::kitty_clipboard_protocol::Operation::Write,
                    kitty::clipboard::Status::Enosys,
                )
            });
            return;
        }

        // Setup our write state
        // const alloc = self.terminal.gpa();
        // const state = try alloc.create(kitty_clipboard.WriteState);
        // errdefer alloc.destroy(state);
        // state.* = try .init(alloc, meta, .{ .max_size = ... });
        //
        // PORT NOTE: the `try alloc.create` OOM (whose failure the dispatch
        // arm swallows) is ported over the `KITTY_CLIPBOARD_TW` seam — on
        // injected failure we return without creating the transaction, which
        // is exactly the reference's propagated-OOM end state.
        #[cfg(test)]
        if kitty_clipboard_tw_check(KittyClipboardFp::CreateWriteState).is_err() {
            return;
        }
        let state = Box::new(kitty::clipboard::WriteState::init(
            meta,
            kitty::clipboard::WriteOptions {
                max_size: self.kitty_clipboard_write_max_bytes,
            },
        ));
        self.kitty_clipboard_write = Some(state);
    }

    // fn kittyClipboardData(self, meta, payload, terminator) error{OutOfMemory}!void
    fn kitty_clipboard_data(
        &mut self,
        meta: &kitty::clipboard::Metadata,
        payload: &[u8],
        terminator: osc::Terminator,
    ) {
        // Data without a transaction is silently ignored.
        // const state = self.kitty_clipboard_write orelse return;
        // PORT NOTE: the Box is taken out of the slot while it is operated on
        // (and put back on success) so the failure paths can respond through
        // `&mut self` without aliasing the stored transaction.
        let Some(mut state) = self.kitty_clipboard_write.take() else {
            return;
        };

        // A wdata packet without a MIME type commits the transaction.
        if meta.mime.is_empty() {
            return self.kitty_clipboard_commit(state, terminator);
        }

        match state.data(meta, payload) {
            Ok(()) => self.kitty_clipboard_write = Some(state),

            // Failing to spool matches kitty's EIO for a failed buffer
            // write.
            // (error.OutOfMemory: the reference re-raises after finishing;
            // rustty's spool allocation is infallible, carried for parity.)
            Err(kitty::clipboard::DataError::OutOfMemory) => {
                self.kitty_clipboard_finish(&state, kitty::clipboard::Status::Eio, terminator);
            }

            // Data over the write limit aborts the transaction and is
            // reported to the client.
            Err(kitty::clipboard::DataError::TooLarge) => {
                self.kitty_clipboard_finish(&state, kitty::clipboard::Status::Efbig, terminator);
            }

            // An invalid base64 payload stream aborts the transaction.
            Err(kitty::clipboard::DataError::Invalid) => {
                self.kitty_clipboard_finish(&state, kitty::clipboard::Status::Einval, terminator);
            }
        }
    }

    // fn kittyClipboardAlias(self, meta, payload, terminator) error{OutOfMemory}!void
    fn kitty_clipboard_alias(
        &mut self,
        meta: &kitty::clipboard::Metadata,
        payload: &[u8],
        terminator: osc::Terminator,
    ) {
        // Aliases without a transaction are silently ignored. Once a
        // transaction exists, a missing target MIME type is invalid and
        // aborts the transaction.
        let Some(mut state) = self.kitty_clipboard_write.take() else {
            return;
        };
        if meta.mime.is_empty() {
            return self.kitty_clipboard_finish(
                &state,
                kitty::clipboard::Status::Einval,
                terminator,
            );
        }

        match state.alias(meta, payload) {
            Ok(()) => self.kitty_clipboard_write = Some(state),

            Err(kitty::clipboard::AliasError::OutOfMemory) => {
                self.kitty_clipboard_finish(&state, kitty::clipboard::Status::Eio, terminator);
            }

            // An undecodable alias payload aborts the transaction.
            Err(kitty::clipboard::AliasError::Invalid) => {
                self.kitty_clipboard_finish(&state, kitty::clipboard::Status::Einval, terminator);
            }
        }
    }

    // fn kittyClipboardCommit(self, state, terminator) error{OutOfMemory}!void
    fn kitty_clipboard_commit(
        &mut self,
        mut state: Box<kitty::clipboard::WriteState>,
        terminator: osc::Terminator,
    ) {
        // const committed = state.commit(alloc) catch |err| switch (err) { ... };
        // defer committed.deinit(alloc);
        //
        // PORT NOTE: `commit` borrows the transaction, so the reply capture is
        // recorded first and the effect invoked while `committed` is live;
        // the grant + response ride after (see the SB discipline note on
        // `kitty_clipboard_read`).
        let mut reply_state = KittyClipboardWriteReplyState {
            status: None,
            remember: false,
        };
        let pw_owned: Vec<u8> = {
            let committed = match state.commit() {
                Ok(committed) => committed,
                Err(kitty::clipboard::CommitError::OutOfMemory) => {
                    self.kitty_clipboard_finish(&state, kitty::clipboard::Status::Eio, terminator);
                    return;
                }

                // The last MIME type's payload stream was not correctly
                // padded, which aborts the transaction.
                Err(kitty::clipboard::CommitError::Invalid) => {
                    self.kitty_clipboard_finish(
                        &state,
                        kitty::clipboard::Status::Einval,
                        terminator,
                    );
                    return;
                }
            };

            // The effect can't be null here (checked when the transaction
            // began) but if an embedder cleared it mid-transaction that's
            // ENOSYS.
            let Some(func) = self.effects.clipboard_write else {
                drop(committed);
                self.kitty_clipboard_finish(&state, kitty::clipboard::Status::Enosys, terminator);
                return;
            };

            // Per the spec a password without a name is no password. A
            // stored grant for it lets the embedder skip its prompt.
            let pw: &[u8] = if !committed.name.is_empty() {
                committed.pw
            } else {
                b""
            };
            let granted = self
                .kitty_clipboard_grants
                .use_(pw, kitty::clipboard::Direction::Write);

            func(
                self,
                clipboard::Write {
                    location: committed.loc,
                    contents: &committed.contents,
                    name: committed.name,
                    granted,
                    can_remember: !pw.is_empty(),
                    reply_ctx: (&mut reply_state) as *mut KittyClipboardWriteReplyState
                        as *mut core::ffi::c_void,
                    reply_fn: KittyClipboardWriteReplyState::reply,
                },
            );

            pw.to_vec()
        };

        // Remembering is only offered when the request carried a usable
        // password. (Recorded by the reply; applied here — see the SB
        // discipline note on `kitty_clipboard_read`.)
        if reply_state.remember && !pw_owned.is_empty() {
            self.kitty_clipboard_grants
                .grant(&pw_owned, kitty::clipboard::Direction::Write, false);
        }

        // The program is waiting on the commit status, so a callback
        // that returned without a reply is answered as a denial rather
        // than silence.
        self.kitty_clipboard_finish(
            &state,
            reply_state
                .status
                .unwrap_or(kitty::clipboard::Status::Eperm),
            terminator,
        );
    }

    /// Answer a write transaction with its final status and drop it.
    /// The id echoed is the one from the transaction's opening write
    /// packet, matching kitty.
    // fn kittyClipboardFinish(self, state: *const WriteState, status, terminator) void
    //
    // PORT NOTE: callers hand the taken-out transaction by reference; the
    // reference's trailing `kittyClipboardAbort` (destroy + null) is the drop
    // of that Box at the caller plus the abort of any stored one.
    fn kitty_clipboard_finish(
        &mut self,
        state: &kitty::clipboard::WriteState,
        status: kitty::clipboard::Status,
        terminator: osc::Terminator,
    ) {
        self.kitty_clipboard_respond(&kitty::clipboard::Response {
            id: &state.id,
            terminator,
            ..kitty::clipboard::Response::new(
                crate::osc::parsers::kitty_clipboard_protocol::Operation::Write,
                status,
            )
        });
        self.kitty_clipboard_abort();
    }

    /// Drop any in-flight write transaction without responding.
    // fn kittyClipboardAbort(self: *Handler) void {
    //     if (self.kitty_clipboard_write) |state| {
    //         state.deinit(alloc); alloc.destroy(state);
    //         self.kitty_clipboard_write = null;
    //     }
    // }
    fn kitty_clipboard_abort(&mut self) {
        // PORT NOTE: dropping the Box is the deinit + destroy.
        self.kitty_clipboard_write = None;
    }

    /// Encode and write a single response packet. Unlike kitty, which
    /// always terminates responses with ST, we echo the terminator of
    /// the request being answered, matching our other OSC responses.
    // fn kittyClipboardRespond(self: *Handler, response: *const kitty_clipboard.Response) void
    fn kitty_clipboard_respond(&mut self, response: &kitty::clipboard::Response) {
        if self.effects.write_pty.is_none() {
            return;
        }

        // Our responses carry at most a status and the echoed id so
        // they virtually always fit on the stack.
        // var stack = std.heap.stackFallback(1024, self.terminal.gpa());
        // PORT NOTE (R10): stackFallback is a Zig allocation idiom; the port
        // encodes into an owned buffer (PERF(port): stackFallback).
        let mut aw = crate::snapshot::io::AllocatingWriter::new();
        if response.encode(&mut aw).is_err() {
            return;
        }
        let resp = aw.written().to_vec();
        self.write_pty(&resp);
    }

    /// Handle an OSC 72 drag and drop command.
    // fn kittyDnd(self: *Handler, v: Action.KittyDnd)
    //     (Allocator.Error || std.Io.Writer.Error)!void
    fn do_kitty_dnd(&mut self, v: &osc::KittyDndProtocol) -> std::io::Result<()> {
        // Responses are usually small (queries, errors) but data
        // serving can produce many chunks, so fall back to the heap.
        // var stack = std.heap.stackFallback(512, self.terminal.gpa());
        // PORT NOTE (R10): PERF(port): stackFallback — owned Vec buffer.
        let mut aw: Vec<u8> = Vec::new();

        // The state is allocated on registration and owned by the
        // terminal, so it uses the terminal's allocator, not the
        // response's.
        let event = {
            // SAFETY: `self.terminal` aliases the Terminal that created this
            // Handler (see `term()`); the `&mut` on its `kitty_dnd` slot lives
            // only for this call and does not overlap another Terminal borrow.
            let slot = unsafe { &mut (*self.terminal).kitty_dnd };
            kitty::dnd::handle_command(slot, &mut aw, v)?
        };

        if !aw.is_empty() {
            self.write_pty(&aw);
        }

        if let Some(ev) = event {
            let Some(func) = self.effects.drag_and_drop else {
                return Ok(());
            };
            func(self, ev);
        }
        Ok(())
    }

    // fn reportMode2048(self: *Handler) void {
    //     const get_size = self.effects.size orelse return;
    //     const current = get_size(self) orelse return;
    //     var buf: [128]u8 = undefined;
    //     var writer: std.Io.Writer = .fixed(buf[0 .. buf.len - 1]);
    //     size_report.encode(&writer, .mode_2048, current) catch return;
    //     buf[writer.end] = 0;
    //     self.writePty(buf[0..writer.end :0]);
    // }
    fn report_mode_2048(&mut self) {
        let Some(get_size) = self.effects.size else {
            return;
        };
        let Some(current) = get_size(self) else {
            return;
        };

        // PORT NOTE: the reference reserves the final byte for the Zig
        // sentinel; rustty's `write_pty` takes a plain `&[u8]` (the resize
        // report precedent), so the encoded bytes are written directly.
        let mut buf = String::new();
        if size_report::encode(&mut buf, size_report::Style::Mode2048, current).is_err() {
            return;
        }
        self.write_pty(buf.as_bytes());
    }

    // fn dcsHook(self: *Handler, value: Action.Value(.dcs_hook)) !void {
    //     var cmd = self.dcs_handler.hook(self.terminal.gpa(), value) orelse return;
    //     defer cmd.deinit();
    //     try self.dcsCommand(&cmd);
    // }
    fn do_dcs_hook(&mut self, dcs: crate::parser::DCS) -> Result<(), crate::pagelist::OutOfMemory> {
        let gpa = self.term().gpa();
        // (PORT NOTE: the trait hands the parser's owned DCS; the dcs Handler
        // takes the borrowed view — the same coercion Zig's `terminal.DCS` does.)
        let mut cmd = match self.dcs_handler.hook(
            gpa,
            crate::dcs::DCS {
                intermediates: &dcs.intermediates,
                params: &dcs.params,
                final_: dcs.final_,
            },
        ) {
            Some(cmd) => cmd,
            None => return Ok(()),
        };
        // defer cmd.deinit();
        let result = self.do_dcs_command(&mut cmd);
        cmd.deinit();
        result
    }

    // fn dcsPut(self: *Handler, value: u8) !void { ... }
    fn do_dcs_put(&mut self, value: u8) -> Result<(), crate::pagelist::OutOfMemory> {
        let mut cmd = match self.dcs_handler.put(value) {
            Some(cmd) => cmd,
            None => return Ok(()),
        };
        let result = self.do_dcs_command(&mut cmd);
        cmd.deinit();
        result
    }

    // fn dcsUnhook(self: *Handler) !void { ... }
    fn do_dcs_unhook(&mut self) -> Result<(), crate::pagelist::OutOfMemory> {
        let mut cmd = match self.dcs_handler.unhook() {
            Some(cmd) => cmd,
            None => return Ok(()),
        };
        let result = self.do_dcs_command(&mut cmd);
        cmd.deinit();
        result
    }

    // fn dcsCommand(self: *Handler, cmd: *dcs.Command) !void {
    fn do_dcs_command(
        &mut self,
        cmd: &mut crate::dcs::Command,
    ) -> Result<(), crate::pagelist::OutOfMemory> {
        match cmd {
            // .decrqss => |request| { ... }
            crate::dcs::Command::Decrqss(request) => {
                // var response: [max_response_bytes + 1]u8 = undefined;
                // const encoded = try request.encode(self.terminal, response[0 .. response.len - 1]);
                // response[encoded.len] = 0;
                // self.writePty(response[0..encoded.len :0]);
                //
                // PORT NOTE: the reference reserves the final byte for the Zig
                // `[:0]const u8` sentinel the callback takes. rustty's
                // `write_pty` callback takes a plain `&[u8]`, so the reserved
                // byte is kept (identical capacity budget) but no NUL is
                // appended.
                let request = *request;
                let mut response = [0u8; crate::dcs::Decrqss::MAX_RESPONSE_BYTES + 1];
                let last = response.len() - 1;
                let encoded_len = {
                    let t: *mut Terminal = self.terminal;
                    // SAFETY: `self.terminal` aliases the Terminal that created
                    // this Handler (see `term()`); the deref is short-lived and
                    // does not overlap another live `&mut Terminal`.
                    // (PORT NOTE: `dcs::OutOfMemory` and `pagelist::OutOfMemory`
                    // are both the reference's single `Allocator.Error`; the
                    // crate has no `From` between them, so map explicitly.)
                    let encoded = request
                        .encode(unsafe { &mut *t }, &mut response[..last])
                        .map_err(|_| crate::pagelist::OutOfMemory)?;
                    encoded.len()
                };
                self.write_pty(&response[..encoded_len]);
                Ok(())
            }

            // .xtgettcap => |*gettcap| { ... }
            crate::dcs::Command::Xtgettcap(gettcap) => {
                // if (self.effects.write_pty == null) return;
                if self.effects.write_pty.is_none() {
                    return Ok(());
                }
                // const map = comptime terminfo.ghostty.xtgettcapMap();
                let map = xtgettcap_map();
                // while (gettcap.next()) |key| { ... }
                while let Some(key) = gettcap.next() {
                    if key == ENCODED_TN_KEY {
                        self.write_terminfo_name();
                        continue;
                    }
                    // self.writePty(map.get(key) orelse continue);
                    let Some(response) = map.get(key) else {
                        continue;
                    };
                    self.write_pty(response);
                }
                Ok(())
            }

            // .tmux => {},
            crate::dcs::Command::Tmux(_) => Ok(()),
        }
    }

    /// Answer an XTGETTCAP "TN" query from the configured terminfo name.
    /// Unset, empty, or over-long names leave the query unanswered.
    // fn writeTerminfoName(self: *Handler) void { ... }
    fn write_terminfo_name(&mut self) {
        // const name = self.terminfo_name orelse return;
        let Some(name) = self.terminfo_name else {
            return;
        };
        // if (name.len == 0 or name.len > max_terminfo_name_bytes) return;
        if name.is_empty() || name.len() > MAX_TERMINFO_NAME_BYTES {
            return;
        }

        // Fixed upper bound for an encoded "TN" reply calculated
        // at comptime from our max terminfo size.
        const MAX_TN_RESPONSE_BYTES: usize = "\x1bP1+r".len()
            + ENCODED_TN_KEY.len()
            + "=".len()
            + (MAX_TERMINFO_NAME_BYTES * 2)
            + "\x1b\\".len()
            + 1; // null terminator

        // Values are hex-encoded uppercase, matching the static map. The
        // buffer fits any name allowed above, so the print cannot fail.
        //
        // PORT NOTE: the reference `std.fmt.bufPrintZ(&buf, "\x1bP1+r" ++
        // encoded_tn_key ++ "={X}\x1b\\", .{name}) catch unreachable` appends a
        // NUL sentinel for the Zig `[:0]const u8` callback parameter; rustty's
        // `write_pty` takes a plain `&[u8]` (the DCS-response precedent), so
        // the reserved byte is kept in the capacity budget but no NUL is
        // appended. `{X}` is uppercase hex, one byte -> two digits.
        let mut buf = [0u8; MAX_TN_RESPONSE_BYTES];
        let mut len: usize = 0;
        for part in [b"\x1bP1+r".as_slice(), ENCODED_TN_KEY, b"="] {
            buf[len..len + part.len()].copy_from_slice(part);
            len += part.len();
        }
        const CHARSET: &[u8; 16] = b"0123456789ABCDEF";
        for &b in name {
            buf[len] = CHARSET[(b >> 4) as usize];
            buf[len + 1] = CHARSET[(b & 0xf) as usize];
            len += 2;
        }
        buf[len..len + 2].copy_from_slice(b"\x1b\\");
        len += 2;
        self.write_pty(&buf[..len]);
    }

    // fn bell(self: *Handler) void {
    //     const func = self.effects.bell orelse return;
    //     func(self);
    // }
    fn do_bell(&mut self) {
        let Some(func) = self.effects.bell else {
            return;
        };
        func(self);
    }

    // inline fn horizontalTab(self: *Handler, count: u16) void {
    //     for (0..count) |_| {
    //         const x = self.terminal.screens.active.cursor.x;
    //         self.terminal.horizontalTab();
    //         if (x == self.terminal.screens.active.cursor.x) break;
    //     }
    // }
    #[inline]
    fn do_horizontal_tab(&mut self, count: u16) {
        for _ in 0..count {
            let x = self.term().active_mut().cursor.x;
            self.term().horizontal_tab();
            if x == self.term().active_mut().cursor.x {
                break;
            }
        }
    }

    // inline fn horizontalTabBack(self: *Handler, count: u16) void {
    //     for (0..count) |_| {
    //         const x = self.terminal.screens.active.cursor.x;
    //         self.terminal.horizontalTabBack();
    //         if (x == self.terminal.screens.active.cursor.x) break;
    //     }
    // }
    #[inline]
    fn do_horizontal_tab_back(&mut self, count: u16) {
        for _ in 0..count {
            let x = self.term().active_mut().cursor.x;
            self.term().horizontal_tab_back();
            if x == self.term().active_mut().cursor.x {
                break;
            }
        }
    }

    // fn setMode(self: *Handler, mode: modes.Mode, enabled: bool) !void
    fn do_set_mode(
        &mut self,
        mode: modes::Mode,
        enabled: bool,
    ) -> Result<(), crate::pagelist::OutOfMemory> {
        // Set the mode on the terminal
        // self.terminal.modes.set(mode, enabled);
        self.term().modes.set(mode, enabled);

        // Some modes require additional processing
        match mode {
            // .autorepeat,
            // .reverse_colors,
            // => {},
            modes::Mode::Autorepeat | modes::Mode::ReverseColors => {}

            // .origin => self.terminal.setCursorPos(1, 1),
            modes::Mode::Origin => self.term().set_cursor_pos(1, 1),

            // .enable_left_and_right_margin => if (!enabled) {
            //     self.terminal.scrolling_region.left = 0;
            //     self.terminal.scrolling_region.right = self.terminal.cols - 1;
            // },
            modes::Mode::EnableLeftAndRightMargin => {
                if !enabled {
                    self.term().scrolling_region.left = 0;
                    let right = self.term().cols - 1;
                    self.term().scrolling_region.right = right;
                }
            }

            // .alt_screen_legacy => try self.terminal.switchScreenMode(.@"47", enabled),
            modes::Mode::AltScreenLegacy => {
                self.term()
                    .switch_screen_mode(SwitchScreenMode::M47, enabled)?;
            }
            // .alt_screen => try self.terminal.switchScreenMode(.@"1047", enabled),
            modes::Mode::AltScreen => {
                self.term()
                    .switch_screen_mode(SwitchScreenMode::M1047, enabled)?;
            }
            // .alt_screen_save_cursor_clear_enter => try self.terminal.switchScreenMode(.@"1049", enabled),
            modes::Mode::AltScreenSaveCursorClearEnter => {
                self.term()
                    .switch_screen_mode(SwitchScreenMode::M1049, enabled)?;
            }

            // .save_cursor => if (enabled) {
            //     self.terminal.saveCursor();
            // } else {
            //     self.terminal.restoreCursor();
            // },
            modes::Mode::SaveCursor => {
                if enabled {
                    self.term().save_cursor();
                } else {
                    self.term().restore_cursor();
                }
            }

            // .enable_mode_3 => {},
            modes::Mode::EnableMode3 => {}

            // .@"132_column" => try self.terminal.deccolm(
            //     self.terminal.screens.active.alloc,
            //     if (enabled) .@"132_cols" else .@"80_cols",
            // ),
            modes::Mode::Column132 => {
                let alloc = unsafe { (*self.term().screens.active).alloc };
                self.term().deccolm(
                    alloc,
                    if enabled {
                        DeccolmMode::Cols132
                    } else {
                        DeccolmMode::Cols80
                    },
                )?;
            }

            // .synchronized_output,
            // .linefeed,
            // .focus_event,
            // => {},
            modes::Mode::SynchronizedOutput | modes::Mode::Linefeed | modes::Mode::FocusEvent => {}

            // Enabling mode 2048 reports already-committed pixel geometry.
            // Waiting for the next resize leaves late-enabling clients without
            // the dimensions they need for their first image frame.
            // .in_band_size_reports => if (enabled) self.reportMode2048(),
            modes::Mode::InBandSizeReports => {
                if enabled {
                    self.report_mode_2048();
                }
            }

            // .report_visibility => if (enabled) self.sendVisibilityReport(),
            modes::Mode::ReportVisibility => {
                if enabled {
                    self.send_visibility_report();
                }
            }

            // .mouse_event_x10 => {
            //     if (enabled) {
            //         self.terminal.flags.mouse_event = .x10;
            //     } else {
            //         self.terminal.flags.mouse_event = .none;
            //     }
            // },
            modes::Mode::MouseEventX10 => {
                self.term().flags.mouse_event = if enabled {
                    crate::mouse::Event::X10
                } else {
                    crate::mouse::Event::None
                };
            }
            // .mouse_event_normal => {
            //     if (enabled) {
            //         self.terminal.flags.mouse_event = .normal;
            //     } else {
            //         self.terminal.flags.mouse_event = .none;
            //     }
            // },
            modes::Mode::MouseEventNormal => {
                self.term().flags.mouse_event = if enabled {
                    crate::mouse::Event::Normal
                } else {
                    crate::mouse::Event::None
                };
            }
            // .mouse_event_button => {
            //     if (enabled) {
            //         self.terminal.flags.mouse_event = .button;
            //     } else {
            //         self.terminal.flags.mouse_event = .none;
            //     }
            // },
            modes::Mode::MouseEventButton => {
                self.term().flags.mouse_event = if enabled {
                    crate::mouse::Event::Button
                } else {
                    crate::mouse::Event::None
                };
            }
            // .mouse_event_any => {
            //     if (enabled) {
            //         self.terminal.flags.mouse_event = .any;
            //     } else {
            //         self.terminal.flags.mouse_event = .none;
            //     }
            // },
            modes::Mode::MouseEventAny => {
                self.term().flags.mouse_event = if enabled {
                    crate::mouse::Event::Any
                } else {
                    crate::mouse::Event::None
                };
            }

            // .mouse_format_utf8 => self.terminal.flags.mouse_format = if (enabled) .utf8 else .x10,
            modes::Mode::MouseFormatUtf8 => {
                self.term().flags.mouse_format = if enabled {
                    crate::mouse::Format::Utf8
                } else {
                    crate::mouse::Format::X10
                };
            }
            // .mouse_format_sgr => self.terminal.flags.mouse_format = if (enabled) .sgr else .x10,
            modes::Mode::MouseFormatSgr => {
                self.term().flags.mouse_format = if enabled {
                    crate::mouse::Format::Sgr
                } else {
                    crate::mouse::Format::X10
                };
            }
            // .mouse_format_urxvt => self.terminal.flags.mouse_format = if (enabled) .urxvt else .x10,
            modes::Mode::MouseFormatUrxvt => {
                self.term().flags.mouse_format = if enabled {
                    crate::mouse::Format::Urxvt
                } else {
                    crate::mouse::Format::X10
                };
            }
            // .mouse_format_sgr_pixels => self.terminal.flags.mouse_format = if (enabled) .sgr_pixels else .x10,
            modes::Mode::MouseFormatSgrPixels => {
                self.term().flags.mouse_format = if enabled {
                    crate::mouse::Format::SgrPixels
                } else {
                    crate::mouse::Format::X10
                };
            }

            // else => {},
            _ => {}
        }

        Ok(())
    }

    // fn colorOperation(
    //     self: *Handler,
    //     requests: *const osc_color.List,
    //     terminator: osc.Terminator,
    // ) !void
    fn do_color_operation(
        &mut self,
        requests: &osc::color::List,
        terminator: osc::Terminator,
    ) -> Result<(), crate::pagelist::OutOfMemory> {
        // if (requests.count() == 0) return;
        if requests.is_empty() {
            return Ok(());
        }

        // var stack = std.heap.stackFallback(1024, self.terminal.gpa());
        // const alloc = stack.get();
        // var response: std.Io.Writer.Allocating = .init(alloc);
        // defer response.deinit();
        // const writer = &response.writer;
        //
        // PORT FINDING (P3-W4 T5): in the reference, EVERY `writer.print` below
        // is fallible (an allocating writer), which is what makes
        // `colorOperation` a `!void` the caller can `catch |err| log.warn(...)`.
        // In rustty the writer is a `Vec<u8>` that ABORTS on OOM (the crate's
        // DG5 policy), so this function has NO reachable error path at all —
        // every `return` below is `Ok(())`. That makes the `439d35e27`
        // re-partition ("color_operation stays graceful") IMPOSSIBLE to
        // exercise by feeding a real failure, and any test that tries is
        // VACUOUS. The fail point below is the injectable stand-in for the
        // reference's fallible `writer.print`; it exists ONLY so the
        // "a color-operation failure must NOT set `semantic_failure`" control
        // is a real control. See `COLOR_OPERATION_TW`.
        let mut response = Vec::new();

        #[cfg(test)]
        color_operation_tw_check(ColorOperationFp::Respond)?;

        // var it = requests.constIterator(0);
        // while (it.next()) |req| {
        for req in requests.iter() {
            match req {
                // .set => |set| {
                osc::color::Request::Set(set) => {
                    match set.target {
                        // .palette => |i| {
                        //     self.terminal.flags.dirty.palette = true;
                        //     self.terminal.colors.palette.set(i, set.color);
                        // },
                        osc::color::Target::Palette(i) => {
                            self.term().flags.dirty.palette = true;
                            self.term().colors.palette.set(i, set.color);
                        }
                        // .dynamic => |dynamic| switch (dynamic) {
                        osc::color::Target::Dynamic(dynamic) => match dynamic {
                            // .foreground => self.terminal.colors.foreground.set(set.color),
                            color::Dynamic::Foreground => {
                                self.term().colors.foreground.set(set.color);
                            }
                            // .background => self.terminal.colors.background.set(set.color),
                            color::Dynamic::Background => {
                                self.term().colors.background.set(set.color);
                            }
                            // .cursor => self.terminal.colors.cursor.set(set.color),
                            color::Dynamic::Cursor => {
                                self.term().colors.cursor.set(set.color);
                            }
                            // .pointer_foreground,
                            // .pointer_background,
                            // .tektronix_foreground,
                            // .tektronix_background,
                            // .highlight_background,
                            // .tektronix_cursor,
                            // .highlight_foreground,
                            // => {},
                            color::Dynamic::PointerForeground
                            | color::Dynamic::PointerBackground
                            | color::Dynamic::TektronixForeground
                            | color::Dynamic::TektronixBackground
                            | color::Dynamic::HighlightBackground
                            | color::Dynamic::TektronixCursor
                            | color::Dynamic::HighlightForeground => {}
                        },
                        // .special => {},
                        osc::color::Target::Special(_) => {}
                    }
                }

                // .reset => |target| switch (target) {
                osc::color::Request::Reset(target) => match target {
                    // .palette => |i| {
                    //     self.terminal.flags.dirty.palette = true;
                    //     self.terminal.colors.palette.reset(i);
                    // },
                    osc::color::Target::Palette(i) => {
                        self.term().flags.dirty.palette = true;
                        self.term().colors.palette.reset(*i);
                    }
                    // .dynamic => |dynamic| switch (dynamic) {
                    osc::color::Target::Dynamic(dynamic) => match dynamic {
                        // .foreground => self.terminal.colors.foreground.reset(),
                        color::Dynamic::Foreground => self.term().colors.foreground.reset(),
                        // .background => self.terminal.colors.background.reset(),
                        color::Dynamic::Background => self.term().colors.background.reset(),
                        // .cursor => self.terminal.colors.cursor.reset(),
                        color::Dynamic::Cursor => self.term().colors.cursor.reset(),
                        // .pointer_foreground,
                        // ...
                        // => {},
                        color::Dynamic::PointerForeground
                        | color::Dynamic::PointerBackground
                        | color::Dynamic::TektronixForeground
                        | color::Dynamic::TektronixBackground
                        | color::Dynamic::HighlightBackground
                        | color::Dynamic::TektronixCursor
                        | color::Dynamic::HighlightForeground => {}
                    },
                    // .special => {},
                    osc::color::Target::Special(_) => {}
                },

                // .reset_palette => {
                //     const mask = &self.terminal.colors.palette.mask;
                //     var mask_it = mask.iterator(.{});
                //     while (mask_it.next()) |i| {
                //         self.terminal.flags.dirty.palette = true;
                //         self.terminal.colors.palette.reset(@intCast(i));
                //     }
                //     mask.* = .initEmpty();
                // },
                osc::color::Request::ResetPalette => {
                    let indices: Vec<usize> = self.term().colors.palette.mask.iter_set().collect();
                    for i in indices {
                        self.term().flags.dirty.palette = true;
                        self.term().colors.palette.reset(i as u8);
                    }
                    self.term().colors.palette.mask = color::PaletteMask::init_empty();
                }

                // .query => |target| {
                //     if (self.effects.write_pty == null) continue;
                //     const c = self.terminal.colorForXterm(target) orelse continue;
                //     try writeXtermColorReport(writer, target, c, terminator);
                // },
                osc::color::Request::Query(target) => {
                    if self.effects.write_pty.is_none() {
                        continue;
                    }
                    let Some(c) = self.term().color_for_xterm(*target) else {
                        continue;
                    };
                    Self::write_xterm_color_report(&mut response, target, c, terminator);
                }

                // .reset_special => {},
                osc::color::Request::ResetSpecial => {}
            }
        }

        // if (response.written().len > 0) {
        //     const resp = try response.toOwnedSliceSentinel(0);
        //     defer alloc.free(resp);
        //     self.writePty(resp);
        // }
        if !response.is_empty() {
            self.write_pty(&response);
        }

        Ok(())
    }

    // fn writeXtermColorReport(
    //     writer: *std.Io.Writer,
    //     target: osc_color.Target,
    //     c: color.RGB,
    //     terminator: osc.Terminator,
    // ) !void
    fn write_xterm_color_report(
        out: &mut Vec<u8>,
        target: &osc::color::Target,
        c: color::RGB,
        terminator: osc::Terminator,
    ) {
        use std::io::Write as _;
        match target {
            // .palette => |i| {
            //     try writer.print("\x1b]4;{d};", .{i});
            //     try c.encodeRgb16(writer);
            //     try writer.writeAll(terminator.string());
            // },
            osc::color::Target::Palette(i) => {
                let _ = write!(out, "\x1b]4;{};", i);
                c.encode_rgb16(out);
                out.extend_from_slice(terminator.string());
            }
            // .dynamic => |dynamic| switch (dynamic) {
            osc::color::Target::Dynamic(dynamic) => match dynamic {
                // .foreground, .background, .cursor => {
                //     try writer.print("\x1b]{d};", .{@intFromEnum(dynamic)});
                //     try c.encodeRgb16(writer);
                //     try writer.writeAll(terminator.string());
                // },
                color::Dynamic::Foreground
                | color::Dynamic::Background
                | color::Dynamic::Cursor => {
                    let _ = write!(out, "\x1b]{};", *dynamic as u32);
                    c.encode_rgb16(out);
                    out.extend_from_slice(terminator.string());
                }
                // .pointer_foreground, ... => {},
                color::Dynamic::PointerForeground
                | color::Dynamic::PointerBackground
                | color::Dynamic::TektronixForeground
                | color::Dynamic::TektronixBackground
                | color::Dynamic::HighlightBackground
                | color::Dynamic::TektronixCursor
                | color::Dynamic::HighlightForeground => {}
            },
            // .special => {},
            osc::color::Target::Special(_) => {}
        }
    }

    // fn kittyColorOperation(
    //     self: *Handler,
    //     request: kitty_color.OSC,
    // ) !void
    fn do_kitty_color_operation(
        &mut self,
        request: kitty::color::OSC,
    ) -> Result<(), crate::pagelist::OutOfMemory> {
        // var stack = std.heap.stackFallback(1024, self.terminal.gpa());
        // const alloc = stack.get();
        // var response: std.Io.Writer.Allocating = .init(alloc);
        // defer response.deinit();
        // const writer = &response.writer;
        let mut response = Vec::new();

        // for (request.list.items) |item| {
        for item in &request.list {
            match item {
                // .set => |v| switch (v.key) {
                kitty::color::Request::Set { key, color } => match key {
                    // .palette => |palette| {
                    //     self.terminal.flags.dirty.palette = true;
                    //     self.terminal.colors.palette.set(palette, v.color);
                    // },
                    kitty::color::Kind::Palette(palette) => {
                        self.term().flags.dirty.palette = true;
                        self.term().colors.palette.set(*palette, *color);
                    }
                    // .special => |special| switch (special) {
                    kitty::color::Kind::Special(special) => match special {
                        // .foreground => self.terminal.colors.foreground.set(v.color),
                        kitty::color::Special::Foreground => {
                            self.term().colors.foreground.set(*color);
                        }
                        // .background => self.terminal.colors.background.set(v.color),
                        kitty::color::Special::Background => {
                            self.term().colors.background.set(*color);
                        }
                        // .cursor => self.terminal.colors.cursor.set(v.color),
                        kitty::color::Special::Cursor => {
                            self.term().colors.cursor.set(*color);
                        }
                        // else => {},
                        _ => {}
                    },
                },
                // .reset => |key| switch (key) {
                kitty::color::Request::Reset(key) => match key {
                    // .palette => |palette| {
                    //     self.terminal.flags.dirty.palette = true;
                    //     self.terminal.colors.palette.reset(palette);
                    // },
                    kitty::color::Kind::Palette(palette) => {
                        self.term().flags.dirty.palette = true;
                        self.term().colors.palette.reset(*palette);
                    }
                    // .special => |special| switch (special) {
                    kitty::color::Kind::Special(special) => match special {
                        // .foreground => self.terminal.colors.foreground.reset(),
                        kitty::color::Special::Foreground => self.term().colors.foreground.reset(),
                        // .background => self.terminal.colors.background.reset(),
                        kitty::color::Special::Background => self.term().colors.background.reset(),
                        // .cursor => self.terminal.colors.cursor.reset(),
                        kitty::color::Special::Cursor => self.term().colors.cursor.reset(),
                        // else => {},
                        _ => {}
                    },
                },
                // .query => |key| {
                //     if (self.effects.write_pty == null) continue;
                //     const c = self.terminal.colorForKitty(key) orelse {
                //         if (!key.hasTerminalQueryColor()) continue;
                //         if (response.written().len == 0) try writer.writeAll("\x1b]21");
                //         try writer.print(";{f}=", .{key});
                //         continue;
                //     };
                //
                //     if (response.written().len == 0) try writer.writeAll("\x1b]21");
                //     try writer.print(";{f}=", .{key});
                //     try c.encodeRgb8(writer);
                // },
                kitty::color::Request::Query(key) => {
                    use std::io::Write as _;
                    if self.effects.write_pty.is_none() {
                        continue;
                    }
                    match self.term().color_for_kitty(*key) {
                        None => {
                            if !key.has_terminal_query_color() {
                                continue;
                            }
                            if response.is_empty() {
                                response.extend_from_slice(b"\x1b]21");
                            }
                            let _ = write!(response, ";{}=", key);
                        }
                        Some(c) => {
                            if response.is_empty() {
                                response.extend_from_slice(b"\x1b]21");
                            }
                            let _ = write!(response, ";{}=", key);
                            c.encode_rgb8(&mut response);
                        }
                    }
                }
            }
        }

        // if (response.written().len > 0) {
        //     try writer.writeAll(request.terminator.string());
        //     const resp = try response.toOwnedSliceSentinel(0);
        //     defer alloc.free(resp);
        //     self.writePty(resp);
        // }
        if !response.is_empty() {
            response.extend_from_slice(request.terminator.string());
            self.write_pty(&response);
        }

        Ok(())
    }

    // fn reportDeviceAttributes(self: *Handler, req: device_attributes.Req) void {
    //     const func = self.effects.device_attributes orelse return;
    //     const attrs = func(self);
    //
    //     var stack = std.heap.stackFallback(128, self.terminal.gpa());
    //     const alloc = stack.get();
    //
    //     var aw: std.Io.Writer.Allocating = .init(alloc);
    //     defer aw.deinit();
    //
    //     attrs.encode(req, &aw.writer) catch return;
    //
    //     const written = aw.toOwnedSliceSentinel(0) catch return;
    //     defer alloc.free(written);
    //     self.writePty(written);
    // }
    //
    // PORT NOTE (vt-12b-7b): the reference accumulates the encoded response into a
    // `std.Io.Writer.Allocating` (a `stackFallback(128, ...)`-backed growable
    // buffer) and then writes the owned sentinel slice. Rust's `String` is the
    // growable byte accumulator here; `device_attributes::Attributes::encode`
    // writes via `core::fmt::Write` (matching the reference `attrs.encode`), and
    // an encode failure (`catch return`) is the `is_err()` early-out. The
    // resulting bytes (no sentinel needed at the `&[u8]` callback boundary) are
    // handed to `writePty`.
    fn report_device_attributes(&mut self, req: device_attributes::Req) {
        let Some(func) = self.effects.device_attributes else {
            return;
        };
        let attrs = func(self);

        let mut aw = String::new();
        if attrs.encode(req, &mut aw).is_err() {
            return;
        }

        self.write_pty(aw.as_bytes());
    }

    // fn deviceStatus(self: *Handler, req: device_status.Request) void {
    //     switch (req) {
    //         .operating_status => self.writePty("\x1B[0n"),
    //
    //         .cursor_position => { ... },
    //
    //         .color_scheme => { ... },
    //     }
    // }
    fn device_status(&mut self, req: device_status::Request) {
        match req {
            // .operating_status => self.writePty("\x1B[0n"),
            device_status::Request::OperatingStatus => self.write_pty(b"\x1B[0n"),

            // .cursor_position => {
            //     const pos: struct { x: usize, y: usize } =
            //         if (self.terminal.modes.get(.origin)) .{
            //             .x = self.terminal.screens.active.cursor.x -| self.terminal.scrolling_region.left,
            //             .y = self.terminal.screens.active.cursor.y -| self.terminal.scrolling_region.top,
            //         } else .{
            //             .x = self.terminal.screens.active.cursor.x,
            //             .y = self.terminal.screens.active.cursor.y,
            //         };
            //
            //     var buf: [64]u8 = undefined;
            //     const resp = std.fmt.bufPrintZ(&buf, "\x1B[{};{}R", .{ pos.y + 1, pos.x + 1 }) catch return;
            //     self.writePty(resp);
            // },
            device_status::Request::CursorPosition => {
                let (x, y): (usize, usize) = if self.term().modes.get(modes::Mode::Origin) {
                    let cx = self.term().active_mut().cursor.x;
                    let cy = self.term().active_mut().cursor.y;
                    let left = self.term().scrolling_region.left;
                    let top = self.term().scrolling_region.top;
                    (
                        cx.saturating_sub(left) as usize,
                        cy.saturating_sub(top) as usize,
                    )
                } else {
                    (
                        self.term().active_mut().cursor.x as usize,
                        self.term().active_mut().cursor.y as usize,
                    )
                };

                // PORT NOTE: the reference formats into a fixed `[64]u8` via
                // `bufPrintZ`; Rust's `String` accumulator is the analog (the
                // 64-byte cap is a Zig stack-buffer detail, not observable).
                let resp = format!("\x1B[{};{}R", y + 1, x + 1);
                self.write_pty(resp.as_bytes());
            }

            // .color_scheme => {
            //     const func = self.effects.color_scheme orelse return;
            //     const scheme = func(self) orelse return;
            //     self.writePty(switch (scheme) {
            //         .dark => "\x1B[?997;1n",
            //         .light => "\x1B[?997;2n",
            //     });
            // },
            device_status::Request::ColorScheme => {
                let Some(func) = self.effects.color_scheme else {
                    return;
                };
                let Some(scheme) = func(self) else {
                    return;
                };
                let mut resp = Vec::new();
                device_status::encode_color_scheme_report(&mut resp, scheme);
                self.write_pty(&resp);
            }

            // .visibility => self.sendVisibilityReport(),
            device_status::Request::Visibility => self.send_visibility_report(),
        }
    }

    // fn sendVisibilityReport(self: *Handler) void {
    //     const write_pty = self.effects.write_pty orelse return;
    //     var buf: [device_status.max_visibility_report_encode_size + 1]u8 = undefined;
    //     var writer: std.Io.Writer = .fixed(buf[0..device_status.max_visibility_report_encode_size]);
    //     device_status.encodeVisibilityReport(&writer, ...) catch return;
    //     buf[writer.end] = 0;
    //     write_pty(self, buf[0..writer.end :0]);
    // }
    //
    // PORT NOTE: rustty's `encode_visibility_report` appends to a `Vec<u8>` (the
    // established seam shared with `encode_color_scheme_report`) and cannot
    // fail, so the reference's `catch return` has no reachable arm. The
    // sentinel byte the reference reserves is a Zig `[:0]const u8` encoding
    // detail; rustty's `write_pty` callback takes a plain `&[u8]`.
    fn send_visibility_report(&mut self) {
        if self.effects.write_pty.is_none() {
            return;
        }
        let visibility = if self.term().flags.visible {
            device_status::Visibility::PotentiallyVisible
        } else {
            device_status::Visibility::NotVisible
        };
        let mut buf = Vec::with_capacity(device_status::MAX_VISIBILITY_REPORT_ENCODE_SIZE);
        device_status::encode_visibility_report(&mut buf, visibility);
        self.write_pty(&buf);
    }

    // fn reportEnquiry(self: *Handler) void {
    //     const func = self.effects.enquiry orelse return;
    //     const response = func(self);
    //     if (response.len == 0) return;
    //     var buf: [256]u8 = undefined;
    //     if (response.len >= buf.len) return;
    //     @memcpy(buf[0..response.len], response);
    //     buf[response.len] = 0;
    //     self.writePty(buf[0..response.len :0]);
    // }
    //
    // PORT NOTE (vt-12b-7b): the reference copies the response into a fixed
    // `[256]u8` (NUL-terminating it) and rejects responses `>= 256` bytes. We
    // preserve both guards (empty -> silent; `>= 256` -> silent); the sentinel
    // copy is a Zig encoding detail, so we pass the response slice directly.
    fn report_enquiry(&mut self) {
        let Some(func) = self.effects.enquiry else {
            return;
        };
        let response = func(self);
        if response.is_empty() {
            return;
        }
        if response.len() >= 256 {
            return;
        }
        self.write_pty(response);
    }

    // fn reportXtversion(self: *Handler) void {
    //     const version = if (self.effects.xtversion) |func| func(self) else "";
    //     var buf: [288]u8 = undefined;
    //     const resp = std.fmt.bufPrintZ(
    //         &buf,
    //         "\x1BP>|{s}\x1B\\",
    //         .{if (version.len > 0) version else "libghostty"},
    //     ) catch return;
    //     self.writePty(resp);
    // }
    //
    // PORT NOTE (R2 ghostty->Rustty): the reference's fallback version string is
    // "libghostty"; retargeted to "librustty" per the rename rule. The DCS
    // wrapping (`ESC P > | <version> ESC \`) is unchanged.
    fn report_xtversion(&mut self) {
        let version: &[u8] = if let Some(func) = self.effects.xtversion {
            func(self)
        } else {
            b""
        };
        // PORT NOTE: the reference formats into a fixed `[288]u8`; Rust's `Vec`
        // accumulator is the analog. A non-UTF-8 version slice is written
        // verbatim, so we build the byte sequence directly rather than via a
        // format string (the `{s}` in the reference is a raw byte splice).
        //
        // PORT NOTE: the reference's `bufPrintZ` must fit the formatted bytes
        // PLUS a NUL sentinel in the 288-byte buffer, and its `catch return`
        // turns an overflow into SILENT SUPPRESSION. A growable `Vec` has no
        // such bound, so the cap is enforced explicitly: responses whose
        // framed length + 1 exceeds 288 (i.e. an effective version string
        // longer than 281 bytes: 288 - 1 NUL - 4 `ESC P > |` - 2 `ESC \`) are
        // dropped without touching the pty.
        let mut resp: Vec<u8> = Vec::new();
        resp.extend_from_slice(b"\x1BP>|");
        if version.is_empty() {
            resp.extend_from_slice(b"librustty");
        } else {
            resp.extend_from_slice(version);
        }
        resp.extend_from_slice(b"\x1B\\");
        // `catch return` on `bufPrintZ`'s NoSpaceLeft: the formatted bytes plus
        // the NUL sentinel must fit in the 288-byte buffer.
        if resp.len() + 1 > 288 {
            return;
        }
        self.write_pty(&resp);
    }

    // fn reportSize(self: *Handler, style: csi.SizeReportStyle) void { ... }
    fn report_size(&mut self, style: csi::SizeReportStyle) {
        // Almost all size reports will fit in 256 bytes so try that
        // on the stack before falling back to a heap allocation.
        // var stack = std.heap.stackFallback(256, self.terminal.gpa());
        // const alloc = stack.get();
        //
        // // Allocating writing to accumulate the response.
        // var aw: std.Io.Writer.Allocating = .init(alloc);
        // defer aw.deinit();
        //
        // PORT NOTE (vt-12b-7b): the reference accumulates into a
        // `stackFallback(256, ...)`-backed `std.Io.Writer.Allocating`; Rust's
        // `String` is the growable accumulator. An encode/print failure
        // (`catch return` / `catch |err| { log.warn(...); return; }`) is the
        // `is_err()` early-out.
        let mut aw = String::new();

        // Build the response.
        match style {
            // .csi_21_t => {
            //     if (!self.title_report) return;
            //     const title = self.terminal.getTitle() orelse "";
            //     aw.writer.print("\x1b]l{s}\x1b\\", .{title}) catch return;
            // },
            csi::SizeReportStyle::Csi21T => {
                if !self.title_report {
                    return;
                }
                // PORT NOTE: the title is a raw byte slice spliced via `{s}`; we
                // build the bytes directly to avoid assuming UTF-8.
                let mut resp: Vec<u8> = Vec::new();
                resp.extend_from_slice(b"\x1b]l");
                if let Some(title) = self.term().get_title() {
                    resp.extend_from_slice(title);
                }
                resp.extend_from_slice(b"\x1b\\");
                self.write_pty(&resp);
                return;
            }

            // .csi_14_t, .csi_16_t, .csi_18_t => {
            //     const get_size = self.effects.size orelse return;
            //     const s = get_size(self) orelse return;
            //     const report_style: size_report.Style = switch (style) {
            //         .csi_14_t => .csi_14_t,
            //         .csi_16_t => .csi_16_t,
            //         .csi_18_t => .csi_18_t,
            //         .csi_21_t => unreachable,
            //     };
            //     size_report.encode(&aw.writer, report_style, s) catch |err| {
            //         log.warn("error encoding size report err={}", .{err});
            //         return;
            //     };
            // },
            csi::SizeReportStyle::Csi14T
            | csi::SizeReportStyle::Csi16T
            | csi::SizeReportStyle::Csi18T => {
                let Some(get_size) = self.effects.size else {
                    return;
                };
                let Some(s) = get_size(self) else {
                    return;
                };
                let report_style: size_report::Style = match style {
                    csi::SizeReportStyle::Csi14T => size_report::Style::Csi14T,
                    csi::SizeReportStyle::Csi16T => size_report::Style::Csi16T,
                    csi::SizeReportStyle::Csi18T => size_report::Style::Csi18T,
                    csi::SizeReportStyle::Csi21T => unreachable!(),
                };
                if let Err(err) = size_report::encode(&mut aw, report_style, s) {
                    log::warn!(target: LOG_SCOPE, "error encoding size report err={err:?}");
                    return;
                }
            }
        }

        // const resp = aw.toOwnedSliceSentinel(0) catch return;
        // defer alloc.free(resp);
        // self.writePty(resp);
        self.write_pty(aw.as_bytes());
    }

    // fn windowTitle(self: *Handler, title_raw: []const u8) !void { ... }
    fn do_window_title(&mut self, title_raw: &[u8]) -> Result<(), crate::pagelist::OutOfMemory> {
        // Prevent DoS attacks by limiting title length.
        // const max_title_len = 1024;
        // const title = if (title_raw.len > max_title_len) title: {
        //     log.warn("title length {d} exceeds max length {d}, truncating", ...);
        //     break :title title_raw[0..max_title_len];
        // } else title_raw;
        let max_title_len = 1024;
        let title = if title_raw.len() > max_title_len {
            log::warn!(
                target: LOG_SCOPE,
                "title length {} exceeds max length {max_title_len}, truncating",
                title_raw.len()
            );
            &title_raw[0..max_title_len]
        } else {
            title_raw
        };

        // try self.terminal.setTitle(title);
        //
        // PORT NOTE (oracle 439d35e27): PROMOTED from swallow-and-log to
        // propagating. The title is terminal-owned semantic state, so a failure
        // to set it must reach the shared `vt_catch` and raise
        // `semantic_failure`.
        self.term().set_title(title)?;

        // const func = self.effects.title_changed orelse return;
        // func(self);
        let Some(func) = self.effects.title_changed else {
            return Ok(());
        };
        func(self);
        Ok(())
    }

    // fn reportPwd(self: *Handler, url_raw: []const u8) !void { ... }
    fn do_report_pwd(&mut self, url_raw: &[u8]) -> Result<(), crate::pagelist::OutOfMemory> {
        // Prevent DoS attacks by limiting url length. Headroom for
        // Linux PATH_MAX (4096) plus URI scheme/host and percent-encoding.
        // const max_url_len = 4096;
        // const url = if (url_raw.len > max_url_len) url: {
        //     log.warn("pwd url length {d} exceeds max length {d}, truncating", ...);
        //     break :url url_raw[0..max_url_len];
        // } else url_raw;
        let max_url_len = 4096;
        let url = if url_raw.len() > max_url_len {
            log::warn!(
                target: LOG_SCOPE,
                "pwd url length {} exceeds max length {max_url_len}, truncating",
                url_raw.len()
            );
            &url_raw[0..max_url_len]
        } else {
            url_raw
        };

        // We store the raw payload unparsed. Embedders read it via
        // getPwd() and are responsible for decoding any URI scheme.
        // try self.terminal.setPwd(url);
        //
        // PORT NOTE (oracle 439d35e27): PROMOTED from swallow-and-log to
        // propagating; see `do_window_title`.
        self.term().set_pwd(url)?;

        // const func = self.effects.pwd_changed orelse return;
        // func(self);
        let Some(func) = self.effects.pwd_changed else {
            return Ok(());
        };
        func(self);
        Ok(())
    }

    // fn requestMode(self: *Handler, mode: modes.Mode) void {
    //     var report = self.terminal.modes.getReport(.fromMode(mode));
    //     ...
    //     self.sendModeReport(report);
    // }
    fn do_request_mode(&mut self, mode: modes::Mode) {
        let mut report = self
            .term()
            .modes
            .get_report(modes::ModeTag::from_mode(mode));

        // Kitty paste events (mode 5522) can't work without a clipboard
        // read effect, so if that isn't set mark it as unrecognized.
        if mode == modes::Mode::KittyPasteEvents && self.effects.clipboard_read.is_none() {
            report.state = modes::State::NotRecognized;
        }

        self.send_mode_report(report);
    }

    // fn requestModeUnknown(self: *Handler, mode_raw: u16, ansi: bool) void {
    //     const report = self.terminal.modes.getReport(.{
    //         .value = @truncate(mode_raw),
    //         .ansi = ansi,
    //     });
    //     self.sendModeReport(report);
    // }
    fn do_request_mode_unknown(&mut self, mode_raw: u16, ansi: bool) {
        let report = self.term().modes.get_report(modes::ModeTag {
            // .value = @truncate(mode_raw),
            value: mode_raw & 0x7FFF,
            ansi,
        });
        self.send_mode_report(report);
    }

    // fn sendModeReport(self: *Handler, report: modes.Report) void {
    //     var buf: [modes.Report.max_size + 1]u8 = undefined;
    //     var writer: std.Io.Writer = .fixed(&buf);
    //     report.encode(&writer) catch |err| {
    //         log.warn("error encoding mode report err={}", .{err});
    //         return;
    //     };
    //     const len = writer.buffered().len;
    //     buf[len] = 0;
    //     self.writePty(buf[0..len :0]);
    // }
    //
    // PORT NOTE (vt-12b-7b): the reference encodes into a fixed
    // `[Report.max_size + 1]u8` (the +1 is the NUL sentinel slot). Rust's
    // `String` accumulator is the analog; the sentinel slot is a Zig encoding
    // detail, so we pass the buffered bytes directly.
    fn send_mode_report(&mut self, report: modes::Report) {
        let mut buf = String::new();
        if let Err(err) = report.encode(&mut buf) {
            log::warn!(target: LOG_SCOPE, "error encoding mode report err={err:?}");
            return;
        }
        self.write_pty(buf.as_bytes());
    }

    // fn queryKittyKeyboard(self: *Handler) void {
    //     // Max response is "\x1b[?31u\x00" (7 bytes): the flags are a u5 (max 31).
    //     var buf: [32]u8 = undefined;
    //     const resp = std.fmt.bufPrintZ(&buf, "\x1b[?{}u", .{
    //         self.terminal.screens.active.kitty_keyboard.current().int(),
    //     }) catch return;
    //     self.writePty(resp);
    // }
    fn query_kitty_keyboard(&mut self) {
        // Max response is "\x1b[?31u\x00" (7 bytes): the flags are a u5 (max 31).
        // PORT NOTE: the reference formats into a fixed `[32]u8`; Rust's `String`
        // is the analog accumulator.
        let flags = self.term().active_mut().kitty_keyboard.current().int();
        let resp = format!("\x1b[?{flags}u");
        self.write_pty(resp.as_bytes());
    }

    // fn apcEnd(self: *Handler) void {
    //     const alloc = self.terminal.gpa();
    //     var cmd = self.apc_handler.end() orelse return;
    //     defer cmd.deinit(alloc);
    //     switch (cmd) { ... };
    // }
    // PORT NOTE (Stage A.3 — apc complete): the apc cluster is unblocked. The
    // reference `end()` takes no allocator; the Rust `apc::Handler::end` threads
    // `alloc` for call-site fidelity (kitty `complete` consumes it in the
    // reference). The `defer cmd.deinit(alloc)` is reproduced by an explicit
    // `cmd.deinit(alloc)` at every exit path (both arms own their data via `Vec`s
    // and free on drop, so it is a no-op fidelity hook). The reference buffers the
    // response then appends a `\0` and writes `final[0..final.len-1 :0]` (a Zig
    // sentinel slice); our `write_pty` takes a bare `&[u8]`, so we write the
    // encoded bytes directly with no sentinel byte.
    fn apc_end(&mut self, terminated: bool) {
        let alloc = self.term().gpa();
        let Some(mut cmd) = self.apc_handler.end(alloc) else {
            return;
        };

        match &mut cmd {
            // .unknown => |*unknown| {
            //     if (terminated) self.unknownSequence(.{ .apc = .{
            //         .content = unknown.content,
            //         .truncated = unknown.truncated,
            //     } });
            // },
            crate::apc::Command::Unknown(unknown) => {
                if terminated {
                    self.unknown_sequence(UnknownSequence::Apc(UnknownSequenceString {
                        content: &unknown.content,
                        truncated: unknown.truncated,
                    }));
                }
            }

            // .kitty => |*kitty_cmd| if (comptime build_options.kitty_graphics) {
            crate::apc::Command::Kitty(kitty_cmd) => {
                if crate::options::KITTY_GRAPHICS {
                    // if (self.terminal.kittyGraphics(alloc, kitty_cmd)) |resp| resp: {
                    if let Some(resp) = self.term().kitty_graphics(alloc, kitty_cmd) {
                        'resp: {
                            // Don't waste time encoding if we can't write responses
                            // anyways.
                            // if (self.effects.write_pty == null) break :resp;
                            if self.effects.write_pty.is_none() {
                                break 'resp;
                            }

                            // Encode and write the response if we have one.
                            // var buf: [1024]u8 = undefined;
                            // var writer: std.Io.Writer = .fixed(&buf);
                            // resp.encode(&writer) catch return;
                            // writer.writeByte(0) catch return;
                            // const final = writer.buffered();
                            // if (final.len > 3) self.writePty(final[0 .. final.len - 1 :0]);
                            // PORT NOTE: the reference encodes into a fixed `[1024]u8`
                            // and only writes when `final.len > 3` (i.e. more than the
                            // bare `\x1b_\x1b\\` framing plus the appended `\0`). With
                            // no sentinel byte the guard is `final.len > 2` on the
                            // encoded bytes — but the kitty `Response::encode` only
                            // emits when there is a real payload, so we keep the
                            // reference's `> 3` semantics by comparing the encoded
                            // length plus the (omitted) sentinel byte.
                            let mut buf: Vec<u8> = Vec::new();
                            // PORT NOTE: the reference `resp.encode(...) catch
                            // return` (and the glyph arm's `formatWire ... catch
                            // return`) bail without the `defer cmd.deinit` having
                            // run yet — but `cmd` is `var ...defer deinit` so it
                            // still deinits on the deferred path. Here `cmd` drops
                            // at scope end (freeing its data), so the bare `return`
                            // reproduces the reference cleanup without re-borrowing
                            // `cmd` while `kitty_cmd`/`glyph_req` is live.
                            if resp.encode(&mut buf).is_err() {
                                return;
                            }
                            let final_ = buf;
                            if final_.len() + 1 > 3 {
                                self.write_pty(&final_);
                            }
                        }
                    }
                } else {
                    unreachable!();
                }
            }

            // .glyph => |*glyph_req| if (comptime build_options.glyph_protocol) {
            //
            // PORT NOTE: the comptime gate is the const `options::GLYPH_PROTOCOL`
            // wrapped around the arm body (a Rust match guard would make the
            // match non-exhaustive when false); with the feature off the
            // sequence is consumed and safely ignored, like the reference.
            crate::apc::Command::Glyph(glyph_req) => {
                if crate::options::GLYPH_PROTOCOL {
                    // const resp = self.terminal.glyphProtocol(alloc, glyph_req);
                    let resp = self.term().glyph_protocol(alloc, glyph_req);
                    // if (resp) |r| resp_block: {
                    if let Some(r) = resp {
                        'resp_block: {
                            // Don't waste time encoding if we can't write responses
                            // anyways.
                            // if (self.effects.write_pty == null) break :resp_block;
                            if self.effects.write_pty.is_none() {
                                break 'resp_block;
                            }

                            // Glyph responses are short and bounded by the protocol
                            // fields we emit, so this matches the Kitty response
                            // buffer size above with ample headroom.
                            // var buf: [apc.glyph.Response.max_wire_bytes]u8 = undefined;
                            // var writer: std.Io.Writer = .fixed(&buf);
                            // r.formatWire(&writer) catch return;
                            // writer.writeByte(0) catch return;
                            // const final = writer.buffered();
                            // self.writePty(final[0 .. final.len - 1 :0]);
                            // PORT NOTE: `formatWire` writes to a `std.Io.Writer` over a
                            // fixed `[max_wire_bytes]u8`; the Rust `format_wire` writes to
                            // a `String` (the bounded wire payload is ASCII). We then
                            // write the UTF-8 bytes directly (no sentinel `\0`).
                            let mut buf = String::new();
                            // PORT NOTE: see the kitty-arm note above — bare `return`
                            // (cmd drops/frees) reproduces the reference `catch return`.
                            if r.format_wire(&mut buf).is_err() {
                                return;
                            }
                            self.write_pty(buf.as_bytes());
                        }
                    }
                }
            }
        }

        cmd.deinit(alloc);
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

/// A writer that delivers everything through the write_pty effect:
/// the buffer as it fills, and data that doesn't fit it directly.
/// Never fails, since the effect can't.
// const PtyWriter = struct { handler: *Handler, writer: std.Io.Writer, ... };
//
// PORT NOTE (SB discipline): the reference PtyWriter drains straight through
// `handler.writePty`; the port queues the identical `paste_chunk_size` chunk
// sequence and the handler delivers it right after the paste call returns —
// see the PORT NOTE on `Handler::paste`.
struct PtyChunker {
    buf: [u8; PASTE_CHUNK_SIZE],
    end: usize,
    chunks: Vec<Vec<u8>>,
}

impl PtyChunker {
    fn new() -> PtyChunker {
        PtyChunker {
            buf: [0u8; PASTE_CHUNK_SIZE],
            end: 0,
            chunks: Vec::new(),
        }
    }

    /// The reference `pty.writer.flush()`: everything still buffered becomes
    /// the final (partial) chunk.
    fn flush(&mut self) {
        if self.end > 0 {
            self.chunks.push(self.buf[..self.end].to_vec());
            self.end = 0;
        }
    }
}

impl crate::snapshot::io::Writer for PtyChunker {
    // fn drain(w: *std.Io.Writer, data: []const []const u8, splat: usize) ...
    // (the buffered bytes go first to keep the order; full buffers become
    // whole `paste_chunk_size` chunks, exactly the reference drain cadence.)
    fn write_all(&mut self, mut bytes: &[u8]) -> Result<(), crate::snapshot::io::WriteFailed> {
        while !bytes.is_empty() {
            let n = (PASTE_CHUNK_SIZE - self.end).min(bytes.len());
            self.buf[self.end..self.end + n].copy_from_slice(&bytes[..n]);
            self.end += n;
            bytes = &bytes[n..];
            if self.end == PASTE_CHUNK_SIZE {
                self.chunks.push(self.buf.to_vec());
                self.end = 0;
            }
        }
        Ok(())
    }
}

/// Reply target for clipboard writes on protocols without a write
/// acknowledgement (OSC 52, OSC 1337 Copy): the reply is accepted
/// and discarded.
// fn ignoreWriteReply(_: *anyopaque, _: clipboard.Write.Result) void {}
fn ignore_write_reply(_: *mut core::ffi::c_void, _: clipboard::WriteResult) {}

/// Reply state for one synchronous clipboard read. This lives on the
/// do_clipboard_read stack frame, so it is only valid during the callback.
// const ClipboardReadState = struct { handler, location, terminator, replied };
//
// PORT NOTE (SB discipline): the reference carries `handler: *Handler` and
// writes the pty from inside `reply`; the port's reply only encodes the
// response into this stack state — see the PORT NOTE in `do_clipboard_read`.
struct ClipboardReadState {
    location: clipboard::Location,
    terminator: osc::Terminator,
    replied: bool,
    /// The encoded response, delivered by the handler after the callback.
    response: Vec<u8>,
}

impl ClipboardReadState {
    // fn reply(ctx: *anyopaque, result: clipboard.Read.Result) void {
    fn reply(ctx: *mut core::ffi::c_void, result: clipboard::ReadResult<'_>) {
        // const self: *ClipboardReadState = @ptrCast(@alignCast(ctx));
        // SAFETY: `ctx` is the `&mut ClipboardReadState` the caller passed as
        // `reply_ctx`, valid for the duration of the effect callback; no other
        // reference to the state exists while the callback runs.
        let state = unsafe { &mut *(ctx as *mut ClipboardReadState) };
        if state.replied {
            log::warn!(target: LOG_SCOPE, "clipboard read replied more than once, ignoring");
            return;
        }

        // OSC 52 carries a single text value.
        let data: &[u8] = match &result {
            clipboard::ReadResult::Denied
            | clipboard::ReadResult::Unsupported
            | clipboard::ReadResult::Busy
            | clipboard::ReadResult::IoError => b"",
            clipboard::ReadResult::Success(s) => s
                .contents
                .iter()
                .find(|c| clipboard::is_text_mime(c.mime))
                .map(|c| c.data)
                .unwrap_or(b""),
        };

        // self.respond(data) catch |err| {
        //     // Leave replied unset so clipboardRead falls back to the
        //     // empty reply.
        //     log.warn("error replying to clipboard read err={}", .{err});
        //     return;
        // };
        // self.replied = true;
        // (the ported respond cannot fail; see `respond`.)
        state.respond(data);
        state.replied = true;
    }

    // fn respond(self: *ClipboardReadState, data: []const u8)
    //     error{ OutOfMemory, WriteFailed }!void
    //
    // PORT NOTE (R10): the reference encodes over stackFallback(256) +
    // Allocating writer; the port encodes into the owned response buffer
    // (PERF(port): stackFallback). Infallible by construction.
    fn respond(&mut self, data: &[u8]) {
        use crate::snapshot::io::Writer as _;
        let kind: u8 = match self.location {
            clipboard::Location::Selection => b's',
            clipboard::Location::Primary => b'p',
            // .standard, _ => 'c',
            clipboard::Location::Standard => b'c',
        };
        let mut aw = crate::snapshot::io::AllocatingWriter::new();
        // try aw.writer.print("\x1b]52;{c};", .{kind});
        aw.write_all(b"\x1b]52;").unwrap();
        aw.write_byte(kind).unwrap();
        aw.write_all(b";").unwrap();
        // try std.base64.standard.Encoder.encodeWriter(&aw.writer, data);
        crate::kitty::clipboard_response::b64_encode_writer(&mut aw, data).unwrap();
        // try aw.writer.writeAll(self.terminator.string());
        aw.write_all(self.terminator.string()).unwrap();
        self.response = aw.written().to_vec();
    }
}

/// Reply state for one synchronous Kitty clipboard read. This lives
/// on the kitty_clipboard_read stack frame, so it is only valid during
/// the callback.
// const KittyClipboardReadState = struct { handler, primary, id, pw, mimes,
//     list, terminator, replied };
//
// PORT NOTE (SB discipline): no `handler: *Handler` — the reply encodes the
// response and records the grant intent into this stack state; the handler
// applies both right after the callback (see `kitty_clipboard_read`). The
// borrowed `id`/`pw`/`mimes` slices become owned copies for the same reason.
struct KittyClipboardReadState {
    primary: bool,
    id: Vec<u8>,

    /// The effective password, empty when the request had none.
    pw: Vec<u8>,

    /// The requested types; only these are served from a reply.
    mimes: Vec<Vec<u8>>,
    list: bool,
    terminator: osc::Terminator,

    /// Whether the handler has a write_pty effect (the respond gate the
    /// reference reads through `handler.effects`).
    write_pty_some: bool,
    replied: bool,

    /// Grant intent recorded by a success reply with `remember` set.
    remember: bool,

    /// The encoded response, delivered by the handler after the callback.
    response: Vec<u8>,
}

impl KittyClipboardReadState {
    // fn reply(ctx: *anyopaque, result: clipboard.Read.Result) void {
    fn reply(ctx: *mut core::ffi::c_void, result: clipboard::ReadResult<'_>) {
        // SAFETY: `ctx` is the `&mut KittyClipboardReadState` the caller
        // passed as `reply_ctx`, valid for the duration of the effect
        // callback; no other reference to the state exists while it runs.
        let state = unsafe { &mut *(ctx as *mut KittyClipboardReadState) };
        if state.replied {
            log::warn!(target: LOG_SCOPE, "clipboard read replied more than once, ignoring");
            return;
        }
        state.replied = true;

        let success = match result {
            clipboard::ReadResult::Denied => {
                return state.respond_status(kitty::clipboard::Status::Eperm);
            }
            clipboard::ReadResult::Unsupported => {
                return state.respond_status(kitty::clipboard::Status::Enosys);
            }
            clipboard::ReadResult::Busy => {
                return state.respond_status(kitty::clipboard::Status::Ebusy);
            }
            clipboard::ReadResult::IoError => {
                return state.respond_status(kitty::clipboard::Status::Eio);
            }
            clipboard::ReadResult::Success(s) => s,
        };

        // Remembering is only offered when the request carried a
        // usable password. (Recorded here; the handler records the
        // grant after the callback returns.)
        if success.remember && !state.pw.is_empty() {
            state.remember = true;
        }

        // self.respondSuccess(&success) catch |err| { ...; self.respondStatus(.EIO); };
        // (the ported respond_success cannot fail; see its note.)
        state.respond_success(&success);
    }

    /// Answer with a single status packet.
    // fn respondStatus(self, status: kitty_clipboard.Status) void
    fn respond_status(&mut self, status: kitty::clipboard::Status) {
        // (handler.kittyClipboardRespond's write_pty gate.)
        if !self.write_pty_some {
            return;
        }
        let mut aw = crate::snapshot::io::AllocatingWriter::new();
        if (kitty::clipboard::Response {
            id: &self.id,
            terminator: self.terminator,
            ..kitty::clipboard::Response::new(
                crate::osc::parsers::kitty_clipboard_protocol::Operation::Read,
                status,
            )
        })
        .encode(&mut aw)
        .is_err()
        {
            return;
        }
        self.response = aw.written().to_vec();
    }

    /// Answer with the full success sequence (OK, listing, DATA
    /// chunks, DONE), serving only the requested representations
    /// in request order.
    // fn respondSuccess(self, success: *const clipboard.Read.Result.Success)
    //     error{ OutOfMemory, WriteFailed }!void
    fn respond_success(&mut self, success: &clipboard::ReadResultSuccess<'_>) {
        // if (handler.effects.write_pty == null) return;
        if !self.write_pty_some {
            return;
        }

        // var served_buf: [kitty_clipboard.max_read_mimes]clipboard.Content = undefined;
        let mut served_buf: [clipboard::Content; kitty::clipboard::max_read_mimes] =
            [clipboard::Content {
                mime: b"",
                data: b"",
            }; kitty::clipboard::max_read_mimes];
        let mut served_len: usize = 0;
        for mime in &self.mimes {
            for content in success.contents {
                if content.mime != &mime[..] {
                    continue;
                }
                served_buf[served_len] = *content;
                served_len += 1;
                break;
            }
        }

        // Status packets fit on the stack; DATA packets carry the
        // clipboard contents and fall back to the heap.
        // var stack = std.heap.stackFallback(1024, handler.terminal.gpa());
        // PORT NOTE (R10): PERF(port): stackFallback — owned buffer.
        let mut aw = crate::snapshot::io::AllocatingWriter::new();
        if (kitty::clipboard::ReadSuccess {
            primary: self.primary,
            id: &self.id,
            list: self.list,
            available: success.available,
            contents: &served_buf[0..served_len],
            terminator: self.terminator,
            ..kitty::clipboard::ReadSuccess::default()
        })
        .encode(&mut aw)
        .is_err()
        {
            return;
        }
        self.response = aw.written().to_vec();
    }
}

/// Reply state for one synchronous Kitty clipboard write. This lives
/// on the kitty_clipboard_commit stack frame, so it is only valid
/// during the callback.
// const KittyClipboardWriteReplyState = struct { handler, pw, status };
//
// PORT NOTE (SB discipline): no `handler: *Handler`; the grant intent is
// recorded and applied by the handler after the callback (see
// `kitty_clipboard_commit`).
struct KittyClipboardWriteReplyState {
    /// The replied commit status, mapped 1:1 from the reply result;
    /// null until the callback replies.
    status: Option<kitty::clipboard::Status>,

    /// Grant intent recorded by a success reply with `remember` set.
    remember: bool,
}

impl KittyClipboardWriteReplyState {
    // fn reply(ctx: *anyopaque, result: clipboard.Write.Result) void {
    fn reply(ctx: *mut core::ffi::c_void, result: clipboard::WriteResult) {
        // SAFETY: `ctx` is the `&mut KittyClipboardWriteReplyState` the caller
        // passed as `reply_ctx`, valid for the duration of the effect
        // callback; no other reference to the state exists while it runs.
        let state = unsafe { &mut *(ctx as *mut KittyClipboardWriteReplyState) };
        if state.status.is_some() {
            log::warn!(target: LOG_SCOPE, "clipboard write replied more than once, ignoring");
            return;
        }
        state.status = Some(match result {
            clipboard::WriteResult::Denied => kitty::clipboard::Status::Eperm,
            clipboard::WriteResult::Unsupported => kitty::clipboard::Status::Enosys,
            clipboard::WriteResult::Busy => kitty::clipboard::Status::Ebusy,
            clipboard::WriteResult::InvalidData => kitty::clipboard::Status::Einval,
            clipboard::WriteResult::IoError => kitty::clipboard::Status::Eio,
            clipboard::WriteResult::Success(success) => {
                // Remembering is only offered when the request carried a
                // usable password. (Recorded here; the handler applies the
                // grant after the callback returns.)
                if success.remember {
                    state.remember = true;
                }
                kitty::clipboard::Status::Done
            }
        });
    }
}

impl<'a> stream::Handler for Handler<'a> {
    // pub fn deinit(self: *Handler) void {
    //     self.kittyClipboardAbort();
    //     self.kitty_clipboard_grants.deinit(self.terminal.gpa());
    //     self.apc_handler.deinit();
    //     self.dcs_handler.deinit();
    // }
    fn deinit(&mut self) {
        self.kitty_clipboard_abort();
        // PORT NOTE: `Grants.deinit(alloc)` frees the entries; replacing with
        // the empty default drops them.
        self.kitty_clipboard_grants = kitty::clipboard::Grants::default();
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
        self.vt(|t| t.print(cp));
    }

    // .print_slice => try self.terminal.printSlice(value.cps),
    fn print_slice(&mut self, cps: &[u32]) {
        self.vt(|t| t.print_slice(cps));
    }

    // .print_repeat => try self.terminal.printRepeat(value),
    fn print_repeat(&mut self, count: usize) {
        self.vt(|t| t.print_repeat(count));
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
        self.vt(|t| t.linefeed());
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
        self.vt(|t| t.scroll_up(count));
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
        self.vt(|t| t.index());
    }

    // .next_line => {
    //     try self.terminal.index();
    //     self.terminal.carriageReturn();
    // },
    fn next_line(&mut self) {
        self.vt(|t| t.index());
        self.term().carriage_return();
    }

    // .reverse_index => self.terminal.reverseIndex(),
    fn reverse_index(&mut self) {
        self.term().reverse_index();
    }

    // .full_reset => {
    //     self.terminal.fullReset();
    //
    //     // Full reset clears grants
    //     self.kitty_clipboard_grants.deinit(self.terminal.gpa());
    //     self.kitty_clipboard_grants = .{};
    //
    //     // Clear the progress bar
    //     self.progressReport(.{ .state = .remove });
    // },
    //
    // PORT NOTE (oracle c594031d5): `Terminal::full_reset` now performs the
    // cursor-default restore itself (`setCursorStyle(.default)`).
    fn full_reset(&mut self) {
        self.term().full_reset();

        // Full reset clears grants
        self.kitty_clipboard_grants = kitty::clipboard::Grants::default();

        // Clear the progress bar (eeab85b96; the b56c6d88f "only if there is
        // one" guard was reverted in-range by b7ee3ab6b — end state is the
        // unconditional remove).
        if let Some(func) = self.effects.progress_report {
            func(
                self,
                osc::ProgressReport {
                    state: osc::ProgressReportState::Remove,
                    progress: None,
                },
            );
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
        self.vt_catch(r);
    }

    // .reset_mode => try self.setMode(value.mode, false),
    fn reset_mode(&mut self, v: stream::Mode) {
        let r = self.do_set_mode(v.mode, false);
        self.vt_catch(r);
    }

    // .save_mode => self.terminal.modes.save(value.mode),
    fn save_mode(&mut self, v: stream::Mode) {
        self.term().modes.save(v.mode);
    }

    // .restore_mode => {
    //     const v = self.terminal.modes.restore(value.mode);
    //     try self.setMode(value.mode, v);
    // },
    fn restore_mode(&mut self, v: stream::Mode) {
        let enabled = self.term().modes.restore(v.mode);
        let r = self.do_set_mode(v.mode, enabled);
        self.vt_catch(r);
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
        self.vt_catch(r);
    }

    // .dcs_put => try self.dcsPut(value),
    fn dcs_put(&mut self, code: u8) {
        let r = self.do_dcs_put(code);
        self.vt_catch(r);
    }

    // .dcs_unhook => try self.dcsUnhook(),
    fn dcs_unhook(&mut self) {
        let r = self.do_dcs_unhook();
        self.vt_catch(r);
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
        self.vt_catch(r);
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
        self.vt(|t| t.decaln());
    }

    // .window_title => try self.windowTitle(value.title),
    fn window_title(&mut self, v: stream::WindowTitle) {
        let r = self.do_window_title(v.title);
        self.vt_catch(r);
    }

    // .report_pwd => try self.reportPwd(value.url),
    fn report_pwd(&mut self, v: stream::ReportPwd) {
        let r = self.do_report_pwd(v.url);
        self.vt_catch(r);
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
    // rustty's OSC dispatch
    // arm is infallible, so a malformed payload is a silent no-op here — the net
    // observable behavior is identical (the callback is simply not invoked), and
    // the oracle stream test only asserts the un-invoked count. The reference's
    // fallible-allocator OOM path is ALSO ported: the decode scratch is a
    // fallible `allocator_api2::vec::Vec<u8, _>` over the terminal allocator
    // (`try_reserve_exact`, mirroring `dcs.rs`'s `Allocating::init_capacity`),
    // and on failure we return without invoking the callback — exactly what the
    // reference's `try` + dispatch-arm `catch` pair does. The oracle test
    // "clipboard_write allocation failure is ignored" is ported over the
    // `CLIPBOARD_CONTENTS_TW` seam (rustty's allocator is the zero-sized
    // `Global`, so `testing.failing_allocator` cannot be swapped in). What that
    // test injects is:
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
        self.do_clipboard_contents(v.kind, v.data, v.terminator);
    }

    // .kitty_clipboard => self.kittyClipboard(value) catch |err| {
    //     // Clipboard operations are external effects, not terminal
    //     // state; a failed transaction was already answered.
    //     log.warn("error handling kitty clipboard err={}", .{err});
    // },
    fn kitty_clipboard(&mut self, v: osc::KittyClipboardProtocol) {
        self.do_kitty_clipboard(&v);
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
                self.vt_catch(r);
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
    fn kitty_color_report(&mut self, v: kitty::color::OSC) {
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
    fn color_operation(&mut self, v: stream::ColorOperation) {
        if let Err(err) = self.do_color_operation(&v.requests, v.terminator) {
            log::warn!(target: LOG_SCOPE, "error reporting OSC color err={err:?}");
        }
    }

    // .semantic_prompt => try self.terminal.semanticPrompt(value),
    fn semantic_prompt(&mut self, v: stream::SemanticPrompt) {
        self.vt(|t| t.semantic_prompt(v));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::{DynamicPalette, DynamicRGB};
    use crate::modes as modespkg;
    use crate::terminal::Colors;
    use crate::terminal::Options;
    use crate::{kitty, size};
    use allocator_api2::alloc::Global;

    // PORT NOTE: the reference tests construct `Terminal: try .init(testing.allocator,
    // .{ .cols, .rows })` and drive bytes through a `Stream`. Rustty's `Options`
    // has no field defaults, so we supply them via this helper (matching
    // `terminal.rs`'s test `opts`). `max_scrollback_bytes` defaults to 10_000 except
    // where a test overrides it.
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

    // -- Handler.resize tests (oracle 89b103dd5) ------------------------------
    //
    // PORT NOTE: the reference tests hang a `var` on a local `S` struct to count
    // / capture `writePty` calls. Rust's `write_pty` is a bare `fn` pointer, so
    // the counters live in thread-locals (same idiom as `CAPTURE`/`cap_*`).
    thread_local! {
        static WRITE_CALLS: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
    }

    fn calls_reset() {
        WRITE_CALLS.with(|c| c.set(0));
        CAPTURE.with(|c| *c.borrow_mut() = None);
    }

    fn calls_get() -> usize {
        WRITE_CALLS.with(|c| c.get())
    }

    fn count_write_pty(_: &mut Handler, data: &[u8]) {
        WRITE_CALLS.with(|c| c.set(c.get() + 1));
        CAPTURE.with(|c| *c.borrow_mut() = Some(data.to_vec()));
    }

    fn cell_9x18() -> crate::terminal::ResizeCellSize {
        crate::terminal::ResizeCellSize {
            width: 9,
            height: 18,
        }
    }

    // test "resize clears synchronized output on unchanged cell dimensions"
    #[test]
    fn resize_clears_synchronized_output_on_unchanged_cell_dimensions() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        t.modes.set(modespkg::Mode::SynchronizedOutput, true);
        {
            let mut s = t.vt_stream();
            s.handler
                .resize(crate::terminal::Resize {
                    cols: 80,
                    rows: 24,
                    cell_size_px: Some(cell_9x18()),
                })
                .unwrap();
        }

        assert!(!t.modes.get(modespkg::Mode::SynchronizedOutput));
        assert_eq!(720_u32, t.width_px);
        assert_eq!(432_u32, t.height_px);

        t.deinit(Global);
    }

    // test "unknown APC effect callback"
    #[test]
    fn unknown_apc_effect_callback() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        // const S = struct {
        //     var count: usize = 0;
        //     var content: [16]u8 = undefined;
        //     var content_len: usize = undefined;
        //     var truncated: bool = undefined;
        //     fn unknownSequence(_: *Handler, value: Handler.UnknownSequence) void { ... }
        // };
        // PORT NOTE: the reference hangs `var`s on a local `S` struct; the
        // callback is a bare `fn` pointer here, so the captures live in
        // thread-locals (the module's established `CAPTURE` idiom).
        thread_local! {
            static UNKNOWN_COUNT: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
            static UNKNOWN_CONTENT: core::cell::RefCell<Vec<u8>> =
                const { core::cell::RefCell::new(Vec::new()) };
            static UNKNOWN_TRUNCATED: core::cell::Cell<bool> =
                const { core::cell::Cell::new(false) };
        }

        fn unknown_sequence(_: &mut Handler, value: UnknownSequence) {
            match value {
                UnknownSequence::Apc(apc_value) => {
                    UNKNOWN_CONTENT.with(|c| *c.borrow_mut() = apc_value.content.to_vec());
                    UNKNOWN_TRUNCATED.with(|c| c.set(apc_value.truncated));
                }
            }
            UNKNOWN_COUNT.with(|c| c.set(c.get() + 1));
        }
        // S.count = 0;
        UNKNOWN_COUNT.with(|c| c.set(0));

        // var handler: Handler = .init(&t);
        // handler.unknown_sequence = &S.unknownSequence;
        // handler.apc_handler.unknown_max_bytes = 8;
        // var s: Stream = .init(.{ .allocator = testing.allocator, .handler = handler });
        // defer s.deinit();
        let mut s = t.vt_stream();
        s.handler.unknown_sequence = Some(unknown_sequence);
        s.handler.apc_handler.unknown_max_bytes = 8;

        // Unknown OSC commands retain their legacy behavior and are ignored.
        s.next_slice(b"\x1B]999;abcdef\x07");
        s.next_slice(b"\x1B_abcd;payload\x1B\\");

        assert_eq!(1, UNKNOWN_COUNT.with(|c| c.get()));
        assert_eq!(
            b"abcd;pay".as_slice(),
            UNKNOWN_CONTENT.with(|c| c.borrow().clone()).as_slice()
        );
        assert!(UNKNOWN_TRUNCATED.with(|c| c.get()));

        // Aborted unknown APCs are suppressed.
        s.next_slice(b"\x1B_Xpayload\x18");
        assert_eq!(1, UNKNOWN_COUNT.with(|c| c.get()));

        drop(s);
        t.deinit(Global);
    }

    // test "resize reports mode 2048 geometry"
    #[test]
    fn resize_reports_mode_2048_geometry() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        calls_reset();

        t.modes.set(modespkg::Mode::InBandSizeReports, true);
        {
            let mut s = t.vt_stream();
            s.handler.effects.write_pty = Some(count_write_pty);
            s.handler
                .resize(crate::terminal::Resize {
                    cols: 100,
                    rows: 40,
                    cell_size_px: Some(cell_9x18()),
                })
                .unwrap();
        }

        assert_eq!(
            b"\x1B[48;40;100;720;900t".as_slice(),
            cap_take().unwrap().as_slice()
        );

        t.deinit(Global);
    }

    // test "resize suppresses mode 2048 reports"
    #[test]
    fn resize_suppresses_mode_2048_reports() {
        calls_reset();

        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        // Disabled mode suppresses a report even with pixels and a callback.
        {
            let mut s = t.vt_stream();
            s.handler.effects.write_pty = Some(count_write_pty);
            s.handler
                .resize(crate::terminal::Resize {
                    cols: 80,
                    rows: 24,
                    cell_size_px: Some(cell_9x18()),
                })
                .unwrap();
        }
        assert_eq!(0_usize, calls_get());

        // Missing pixel geometry suppresses a report even with the mode enabled.
        t.modes.set(modespkg::Mode::InBandSizeReports, true);
        {
            let mut s = t.vt_stream();
            s.handler.effects.write_pty = Some(count_write_pty);
            s.handler
                .resize(crate::terminal::Resize {
                    cols: 80,
                    rows: 24,
                    cell_size_px: None,
                })
                .unwrap();
        }
        assert_eq!(0_usize, calls_get());

        // A read-only stream has no write effect and remains successful.
        let mut readonly_terminal = Terminal::init(Global, opts(80, 24)).unwrap();
        readonly_terminal
            .modes
            .set(modespkg::Mode::InBandSizeReports, true);
        {
            let mut readonly_stream = readonly_terminal.vt_stream();
            readonly_stream
                .handler
                .resize(crate::terminal::Resize {
                    cols: 80,
                    rows: 24,
                    cell_size_px: Some(cell_9x18()),
                })
                .unwrap();
        }
        assert_eq!(0_usize, calls_get());

        readonly_terminal.deinit(Global);
        t.deinit(Global);
    }

    // test "resize failure preserves terminal state and does not write"
    //
    // WS18 release lane: this test injects a failure through a `rustty_util::tripwire`
    // fail point, and `tripwire::Module::enabled()` is `cfg!(debug_assertions)` (the
    // faithful stand-in for Zig's `builtin.is_test`; see its PORT NOTE). Without
    // `debug_assertions` the fail point never trips, so the test can only run in
    // debug — where it still runs and passes. NOT disabled to hide a failure.
    #[test]
    #[cfg_attr(not(debug_assertions), ignore)]
    fn resize_failure_preserves_terminal_state_and_does_not_write() {
        calls_reset();

        let mut t = Terminal::init(Global, opts(10, 1)).unwrap();

        t.modes.set(modespkg::Mode::SynchronizedOutput, true);
        t.modes.set(modespkg::Mode::InBandSizeReports, true);

        // PORT NOTE: the reference swaps in a `testing.FailingAllocator`. This
        // port's seam is the `Tabstops::resize` tripwire, whose only fail point
        // (`DynamicAlloc`) sits at the dynamic-stops allocation that
        // `Tabstops::init` reaches from `Terminal::resize`. 513 columns is past
        // `PREALLOC_COLUMNS`, so the allocation IS attempted.
        crate::tabstops::resize_tw_error_always(crate::tabstops::ResizeFp::DynamicAlloc);
        {
            let mut s = t.vt_stream();
            s.handler.effects.write_pty = Some(count_write_pty);
            assert_eq!(
                Err(crate::terminal::ResizeError::OutOfMemory),
                s.handler.resize(crate::terminal::Resize {
                    cols: 513,
                    rows: 1,
                    cell_size_px: Some(cell_9x18()),
                })
            );
        }
        crate::tabstops::resize_tw_end(rustty_util::tripwire::ResetMode::Reset).unwrap();

        // oracle a3c1caba5: the failed resize now ROLLS BACK, so
        // synchronized output stays as it was and the pixel geometry is
        // untouched (it was never set on this terminal).
        assert!(t.modes.get(modespkg::Mode::SynchronizedOutput));
        assert_eq!(0_usize, calls_get());
        assert_eq!(10 as size::CellCountInt, t.cols);
        assert_eq!(0_u32, t.width_px);
        assert_eq!(0_u32, t.height_px);

        t.deinit(Global);
    }

    // test "resize effects do not change canonical terminal state"
    #[test]
    fn resize_effects_do_not_change_canonical_terminal_state() {
        let mut authoritative = Terminal::init(Global, opts(10, 5)).unwrap();
        let mut readonly = Terminal::init(Global, opts(10, 5)).unwrap();

        authoritative
            .modes
            .set(modespkg::Mode::InBandSizeReports, true);
        readonly.modes.set(modespkg::Mode::InBandSizeReports, true);

        let value = crate::terminal::Resize {
            cols: 20,
            rows: 10,
            cell_size_px: Some(cell_9x18()),
        };
        {
            let mut authoritative_stream = authoritative.vt_stream();
            authoritative_stream.handler.effects.write_pty = Some(noop_write_pty);
            authoritative_stream.handler.resize(value).unwrap();
        }
        {
            let mut readonly_stream = readonly.vt_stream();
            readonly_stream.handler.resize(value).unwrap();
        }

        assert_eq!(authoritative.cols, readonly.cols);
        assert_eq!(authoritative.rows, readonly.rows);
        assert_eq!(authoritative.width_px, readonly.width_px);
        assert_eq!(authoritative.height_px, readonly.height_px);
        // std.meta.eql(authoritative.modes, readonly.modes)
        assert_eq!(authoritative.modes.values, readonly.modes.values);
        assert_eq!(authoritative.modes.saved, readonly.modes.saved);
        assert_eq!(authoritative.modes.default, readonly.modes.default);
        assert_eq!(
            authoritative.scrolling_region.top,
            readonly.scrolling_region.top
        );
        assert_eq!(
            authoritative.scrolling_region.bottom,
            readonly.scrolling_region.bottom
        );
        assert_eq!(
            authoritative.scrolling_region.left,
            readonly.scrolling_region.left
        );
        assert_eq!(
            authoritative.scrolling_region.right,
            readonly.scrolling_region.right
        );

        readonly.deinit(Global);
        authoritative.deinit(Global);
    }

    fn noop_write_pty(_: &mut Handler, _: &[u8]) {}

    // test "basic print"
    #[test]
    fn basic_print() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        {
            let mut s = t.vt_stream();
            s.next_slice(b"Hello");
        }

        assert_eq!(5_usize, t.active_mut().cursor.x as usize);
        assert_eq!(0_usize, t.active_mut().cursor.y as usize);

        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), b"Hello");

        t.deinit(Global);
    }

    // test "semantic failure is sticky while processing continues"
    //
    // WS18 release lane: this test injects a failure through a `rustty_util::tripwire`
    // fail point (the port's stand-in for the reference's
    // `std.testing.FailingAllocator` — see the `SET_STRING_TW` PORT NOTE in
    // terminal.rs), and `tripwire::Module::enabled()` is `cfg!(debug_assertions)`.
    // Without `debug_assertions` the fail point never trips, so the test can only
    // run in debug — where it still runs and passes. NOT disabled to hide a failure.
    #[test]
    // P3-W5 repair: the seam this test drives (`SET_STRING_TW`, `SetStringFp`,
    // `set_string_tw_*`) is itself `#[cfg(debug_assertions)]`, so an `ignore`
    // attribute is not enough — the test would not COMPILE in release, which is
    // what broke `cargo test -p rustty-vt --release` on `main`. Gate the test
    // out entirely; it still runs and passes in debug.
    #[cfg(debug_assertions)]
    fn semantic_failure_is_sticky_while_processing_continues() {
        let mut t = Terminal::init(Global, opts(10, 2)).unwrap();

        {
            let mut s = t.vt_stream();
            assert!(!s.handler.semantic_failure);

            // Setting the title is a terminal-owned semantic update. Force its
            // allocation to fail at the central vtFallible boundary.
            crate::terminal::set_string_tw_error_always(crate::terminal::SetStringFp::Reserve);
            s.next_slice(b"\x1B]2;unavailable\x1B\\");
            crate::terminal::set_string_tw_end(rustty_util::tripwire::ResetMode::Reset).unwrap();
            assert!(s.handler.semantic_failure);

            // Later input and RIS remain best-effort and never clear the
            // diagnostic.
            s.next_slice(b"ignored");
            s.next_slice(b"\x1Bc");
            s.next_slice(b"OK");
            assert!(s.handler.semantic_failure);
        }

        let str = t.plain_string(Global).unwrap();
        assert_eq!(b"OK".as_slice(), str.as_slice());

        // A new execution root starts without inheriting the diagnostic.
        let fresh = Handler::init(&raw mut t);
        assert!(!fresh.semantic_failure);

        t.deinit(Global);
    }

    // NEW TEST (no oracle counterpart; the reference gets this for free from
    // `vtFallible`'s single `try` chain): the `set_mode` / `reset_mode` /
    // `restore_mode` actions are `try self.setMode(...)` in the reference, so a
    // failure inside `setMode` reaches the `vt()` catch and sets
    // `semantic_failure`. `setMode`'s fallible arms are the alt-screen switches
    // and `132_column` -> `deccolm` -> `resize`; `resize` is the one with an
    // established failure-injection seam (`terminal::RESIZE_TW`), and it is the
    // only one reachable from all THREE actions (the alt-screen seam can only
    // fail on the lazy first creation of the alternate screen, so it cannot
    // exercise `reset_mode`/`restore_mode`).
    //
    // Each arm gets its own terminal because `semantic_failure` is sticky.
    #[test]
    fn semantic_failure_is_set_by_a_failing_mode_action() {
        use crate::terminal::{resize_tw_end, resize_tw_error_always, ResizeFp};
        use rustty_util::tripwire::ResetMode;

        // .set_mode
        {
            let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
            {
                let mut s = t.vt_stream();
                // DECSET 40 is required before mode 3 does anything; it is an
                // infallible arm and must leave the flag clear.
                s.next_slice(b"\x1b[?40h");
                assert!(!s.handler.semantic_failure);

                resize_tw_error_always(ResizeFp::Tabstops);
                s.next_slice(b"\x1b[?3h");
                resize_tw_end(ResetMode::Reset).unwrap();
                assert!(s.handler.semantic_failure);
            }
            t.deinit(Global);
        }

        // .reset_mode (with the unarmed success path as the negative control)
        {
            let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
            {
                let mut s = t.vt_stream();
                s.next_slice(b"\x1b[?40h");
                s.next_slice(b"\x1b[?3h");
                assert!(!s.handler.semantic_failure);

                resize_tw_error_always(ResizeFp::Tabstops);
                s.next_slice(b"\x1b[?3l");
                resize_tw_end(ResetMode::Reset).unwrap();
                assert!(s.handler.semantic_failure);
            }
            // The successful DECSET 3 above really did resize (side effects on
            // the success path are unchanged), and the failed DECRST 3 rolled
            // the geometry back rather than half-applying it.
            assert_eq!(132, t.cols);
            t.deinit(Global);
        }

        // .restore_mode
        {
            let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
            {
                let mut s = t.vt_stream();
                s.next_slice(b"\x1b[?40h");
                s.next_slice(b"\x1b[?3h");
                s.next_slice(b"\x1b[?3s");
                s.next_slice(b"\x1b[?3l");
                assert!(!s.handler.semantic_failure);
                assert_eq!(80, s.handler.term().cols);

                resize_tw_error_always(ResizeFp::Tabstops);
                s.next_slice(b"\x1b[?3r");
                resize_tw_end(ResetMode::Reset).unwrap();
                assert!(s.handler.semantic_failure);
            }
            t.deinit(Global);
        }
    }

    // NEW TEST (no oracle counterpart): `.start_hyperlink => try
    // self.terminal.screens.active.startHyperlink(...)` is inside the
    // reference's `vtFallible`, so an OSC 8 failure sets `semantic_failure`.
    // The injection seam is `screen::START_HYPERLINK_TW` (the same fail point
    // `Screen: implicit hyperlink id wrap rolls back on allocation failure`
    // uses).
    #[test]
    fn semantic_failure_is_set_by_a_failing_start_hyperlink() {
        use crate::screen::{StartHyperlinkFp, START_HYPERLINK_TW};

        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();

            // NEGATIVE CONTROL: the arm is reached and succeeds, flag stays clear.
            s.next_slice(b"\x1b]8;;http://example.com/a\x1b\\");
            assert!(!s.handler.semantic_failure);
            assert!(s.handler.term().active_mut().cursor.hyperlink_id != 0);

            START_HYPERLINK_TW.with(|tw| {
                tw.borrow_mut()
                    .errorAlways(StartHyperlinkFp::DupeLink, crate::pagelist::OutOfMemory)
            });
            s.next_slice(b"\x1b]8;;http://example.com/b\x1b\\");
            START_HYPERLINK_TW.with(|tw| {
                tw.borrow_mut()
                    .end(rustty_util::tripwire::ResetMode::Reset)
                    .unwrap()
            });
            assert!(s.handler.semantic_failure);

            // Processing continues after the failure (best-effort, like every
            // other arm that funnels through the shared catch).
            s.next_slice(b"OK");
        }
        let str = t.plain_string(Global).unwrap();
        assert_eq!(b"OK".as_slice(), str.as_slice());

        t.deinit(Global);
    }

    // NEGATIVE CONTROL (P3-W4 T5 Step 3): a `color_operation` failure must
    // leave the flag FALSE. This is the assertion that proves the error
    // RE-PARTITION was ported, rather than a blanket "any handler error sets
    // the flag" — which is what a mechanical port of the `vt()` catch alone
    // would produce, and which every other assertion in this file would still
    // pass with.
    //
    // WS18 release lane: tripwire fail point; see the SET_STRING_TW PORT NOTE.
    #[test]
    #[cfg_attr(not(debug_assertions), ignore)]
    fn semantic_failure_is_not_set_by_a_color_operation_failure() {
        // (a) Baseline: the arm IS reached and DOES produce a response, so the
        //     failure injected in (b) is landing on a live code path. (Without
        //     this the whole control could be passing because the input never
        //     reaches `colorOperation` at all.)
        let mut t = Terminal::init(Global, opts(10, 2)).unwrap();
        cap_reset();
        {
            let mut s = t.vt_stream();
            s.handler.effects.write_pty = Some(cap_write_pty);
            s.next_slice(b"\x1B]4;0;?\x1B\\");
            assert!(
                cap_take().is_some(),
                "colorOperation must be reached for this control to mean anything"
            );
            assert!(!s.handler.semantic_failure);
        }
        t.deinit(Global);

        // (b) The same input with `do_color_operation` FORCED to fail. The
        //     response must not be produced (proving the failure was injected
        //     on the live path) and `semantic_failure` must stay FALSE — that
        //     is the re-partition assertion. Routing this arm through
        //     `vt_catch` (the mechanical "any error sets the flag" port) makes
        //     this fail.
        let mut t = Terminal::init(Global, opts(10, 2)).unwrap();
        cap_reset();
        {
            let mut s = t.vt_stream();
            s.handler.effects.write_pty = Some(cap_write_pty);
            color_operation_tw_error_always(ColorOperationFp::Respond);
            s.next_slice(b"\x1B]4;0;?\x1B\\");
            color_operation_tw_end(rustty_util::tripwire::ResetMode::Reset).unwrap();
            assert!(
                cap_take().is_none(),
                "the injected failure must actually abort the response"
            );
            assert!(
                !s.handler.semantic_failure,
                "a color-operation failure is GRACEFUL and must not set semantic_failure"
            );
        }
        t.deinit(Global);
    }

    // test "DECRQSS responses"
    #[test]
    fn decrqss_responses() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        calls_reset();

        {
            let mut s = t.vt_stream();
            s.handler.effects.write_pty = Some(count_write_pty);

            // SGR
            s.next_slice(b"\x1B[1m\x1BP$qm\x1B\\");
            assert_eq!(1_usize, calls_get());
            assert_eq!(
                b"\x1BP1$r0;1m\x1B\\".as_slice(),
                cap_take().unwrap().as_slice()
            );
            calls_reset();

            // Overline
            s.next_slice(b"\x1B[0;53m\x1BP$qm\x1B\\");
            assert_eq!(1_usize, calls_get());
            assert_eq!(
                b"\x1BP1$r0;53m\x1B\\".as_slice(),
                cap_take().unwrap().as_slice()
            );
            calls_reset();

            // Requests larger than the parser's fixed request buffer are
            // ignored, and the next DCS command must still be processed
            // normally.
            s.next_slice(b"\x1BP$qfoo\x1B\\");
            assert_eq!(0_usize, calls_get());
            assert!(!s.handler.semantic_failure);
            s.next_slice(b"\x1BP$qm\x1B\\");
            assert_eq!(1_usize, calls_get());
            assert_eq!(
                b"\x1BP1$r0;53m\x1B\\".as_slice(),
                cap_take().unwrap().as_slice()
            );
        }

        t.deinit(Global);
    }

    // test "DECRQSS without write effect is ignored"
    #[test]
    fn decrqss_without_write_effect_is_ignored() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1BP$qm\x1B\\");
            assert!(!s.handler.semantic_failure);
        }
        t.deinit(Global);
    }

    // `std.fmt.bytesToHex(bytes, .upper / .lower)` for the XTGETTCAP tests'
    // query construction (the reference builds these at comptime with `++`).
    fn test_hex(input: &[u8], upper: bool) -> Vec<u8> {
        let charset: &[u8; 16] = if upper {
            b"0123456789ABCDEF"
        } else {
            b"0123456789abcdef"
        };
        let mut out = Vec::with_capacity(input.len() * 2);
        for &b in input {
            out.push(charset[(b >> 4) as usize]);
            out.push(charset[(b & 0xf) as usize]);
        }
        out
    }

    // Query/response byte-string concatenation (the reference's comptime `++`).
    fn test_concat(parts: &[&[u8]]) -> Vec<u8> {
        let mut out = Vec::new();
        for part in parts {
            out.extend_from_slice(part);
        }
        out
    }

    // PORT NOTE: the reference XTGETTCAP tests hang `var response/response_len/
    // calls` + `writePty`/`expectResponse` on a local `S` struct; the callback
    // is a bare `fn` pointer here, so the captures live in thread-locals (the
    // module's established `CAPTURE`/`WRITE_CALLS` idiom, shared by both
    // XTGETTCAP tests exactly as each reference test's `S` is test-local).
    thread_local! {
        static XT_RESPONSE: core::cell::RefCell<Vec<u8>> =
            const { core::cell::RefCell::new(Vec::new()) };
        static XT_CALLS: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
    }

    // fn reset() void { response_len = 0; calls = 0; }
    fn xt_reset() {
        XT_RESPONSE.with(|c| c.borrow_mut().clear());
        XT_CALLS.with(|c| c.set(0));
    }

    // fn writePty(_: *Handler, data: [:0]const u8) void { ... }
    fn xt_write_pty(_: &mut Handler, data: &[u8]) {
        XT_RESPONSE.with(|c| *c.borrow_mut() = data.to_vec());
        XT_CALLS.with(|c| c.set(c.get() + 1));
    }

    // fn expectResponse(expected: []const u8) !void { ... }
    fn xt_expect_response(expected: &[u8]) {
        assert_eq!(1usize, XT_CALLS.with(|c| c.get()));
        assert_eq!(
            expected,
            XT_RESPONSE.with(|c| c.borrow().clone()).as_slice(),
        );
        xt_reset();
    }

    // test "XTGETTCAP responses"
    #[test]
    fn xtgettcap_responses() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        xt_reset();

        {
            let mut s = t.vt_stream();
            s.handler.effects.write_pty = Some(xt_write_pty);

            // The full capability table comes from the static terminfo map; this
            // checks the wiring for a valued and a valueless (boolean) capability.
            s.next_slice(&test_concat(&[
                b"\x1BP+q",
                &test_hex(b"Co", true),
                b"\x1B\\",
            ]));
            xt_expect_response(&test_concat(&[
                b"\x1BP1+r",
                &test_hex(b"Co", true),
                b"=",
                &test_hex(b"256", true),
                b"\x1B\\",
            ]));
            s.next_slice(&test_concat(&[
                b"\x1BP+q",
                &test_hex(b"am", true),
                b"\x1B\\",
            ]));
            xt_expect_response(&test_concat(&[
                b"\x1BP1+r",
                &test_hex(b"am", true),
                b"\x1B\\",
            ]));

            // One response per requested key; lowercase hex is normalized by the
            // DCS parser. The capture holds the last ("Co") reply.
            s.next_slice(&test_concat(&[
                b"\x1BP+q",
                &test_hex(b"am", false),
                b";",
                &test_hex(b"Co", false),
                b"\x1B\\",
            ]));
            assert_eq!(2usize, XT_CALLS.with(|c| c.get()));
            assert_eq!(
                test_concat(&[
                    b"\x1BP1+r",
                    &test_hex(b"Co", true),
                    b"=",
                    &test_hex(b"256", true),
                    b"\x1B\\",
                ])
                .as_slice(),
                XT_RESPONSE.with(|c| c.borrow().clone()).as_slice(),
            );
            xt_reset();

            // Unknown and malformed keys are skipped without an error.
            s.next_slice(b"\x1BP+qWHO;5;GG\x1B\\");
            assert_eq!(0usize, XT_CALLS.with(|c| c.get()));
            assert!(!s.handler.semantic_failure);
        }

        t.deinit(Global);
    }

    // test "XTGETTCAP without write effect is ignored"
    #[test]
    fn xtgettcap_without_write_effect_is_ignored() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();

            s.next_slice(&test_concat(&[
                b"\x1BP+q",
                &test_hex(b"TN", true),
                b";",
                &test_hex(b"am", true),
                b"\x1B\\",
            ]));
            assert!(!s.handler.semantic_failure);
        }
        t.deinit(Global);
    }

    // test "XTGETTCAP TN responses"
    #[test]
    fn xtgettcap_tn_responses() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        xt_reset();

        {
            let mut s = t.vt_stream();
            s.handler.effects.write_pty = Some(xt_write_pty);

            // const tn_query = "\x1BP+q" ++ std.fmt.bytesToHex("TN", .upper) ++ "\x1B\\";
            let tn_query = test_concat(&[b"\x1BP+q", &test_hex(b"TN", true), b"\x1B\\"]);

            // While no name is configured the query goes unanswered.
            s.next_slice(&tn_query);
            assert_eq!(0usize, XT_CALLS.with(|c| c.get()));

            // A configured name is reported hex-encoded.
            s.handler.terminfo_name = Some(b"xterm-256color");
            s.next_slice(&tn_query);
            xt_expect_response(&test_concat(&[
                b"\x1BP1+r",
                &test_hex(b"TN", true),
                b"=",
                &test_hex(b"xterm-256color", true),
                b"\x1B\\",
            ]));

            // A maximum-length name is still reported in full.
            // const max_name = "a" ** Handler.max_terminfo_name_bytes;
            const MAX_NAME: [u8; MAX_TERMINFO_NAME_BYTES] = [b'a'; MAX_TERMINFO_NAME_BYTES];
            s.handler.terminfo_name = Some(&MAX_NAME);
            s.next_slice(&tn_query);
            xt_expect_response(&test_concat(&[
                b"\x1BP1+r",
                &test_hex(b"TN", true),
                b"=",
                &test_hex(&MAX_NAME, true),
                b"\x1B\\",
            ]));

            // An empty name is silent; "Co" is still answered.
            s.handler.terminfo_name = Some(b"");
            s.next_slice(&test_concat(&[
                b"\x1BP+q",
                &test_hex(b"TN", true),
                b";",
                &test_hex(b"Co", true),
                b"\x1B\\",
            ]));
            xt_expect_response(&test_concat(&[
                b"\x1BP1+r",
                &test_hex(b"Co", true),
                b"=",
                &test_hex(b"256", true),
                b"\x1B\\",
            ]));

            // As are names beyond the maximum length.
            // s.handler.terminfo_name = "a" ** (Handler.max_terminfo_name_bytes + 1);
            const OVERLONG_NAME: [u8; MAX_TERMINFO_NAME_BYTES + 1] =
                [b'a'; MAX_TERMINFO_NAME_BYTES + 1];
            s.handler.terminfo_name = Some(&OVERLONG_NAME);
            s.next_slice(&tn_query);
            assert_eq!(0usize, XT_CALLS.with(|c| c.get()));
            assert!(!s.handler.semantic_failure);
        }

        t.deinit(Global);
    }

    // test "DCS command memory is released"
    //
    // PORT NOTE: the reference relies on `testing.allocator` to detect a leak.
    // rustty has no leak-detecting test allocator here; the value of the test in
    // this port is that `dcsCommand` releases a COMPLETED unsupported command
    // and `Handler::deinit` releases an INCOMPLETE one — both exercised below
    // and both observable under Miri / a leak checker.
    #[test]
    fn dcs_command_memory_is_released() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();

            // A completed command transfers its allocation to Command;
            // dcsCommand must release it even when there is no write effect.
            s.next_slice(b"\x1BP+q536D756C78\x1B\\");

            // An incomplete command remains owned by the handler and must be
            // released when the stream is deinitialized.
            s.next_slice(b"\x1BP+q536D756C78");
        }
        t.deinit(Global);
    }

    // WS3-A KAT (rustty addition, not a reference test): a kitty-graphics PNG
    // transmit sequence through the vt Stream decodes via the wuffs DEFAULT
    // hook — no embedder installed a decoder — proving the non-lib comptime
    // default (sys.zig:18-24) survives the port. `q=2` (quiet always) keeps the
    // handler from emitting an OK response to the (test) pty.
    #[cfg(feature = "wuffs")]
    #[test]
    fn kitty_graphics_png_transmit_decodes_via_default_hook() {
        // Base64 of kitty/testdata/image-png-none-50x76-2147483647-raw.data
        // (the `t=d` direct-medium payload is base64 per the kitty protocol).
        const PNG_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAADIAAABMAQMAAAAV2XZDAAAAA1BMVEUrLzc5wwm5AAAADklEQVR4AWOgIRgFowAAAmAAATnED6oAAAAASUVORK5CYII=";

        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            // APC G: a=t (transmit), f=100 (PNG), t=d (direct), i=31, q=2.
            let seq = format!("\x1b_Ga=t,f=100,t=d,i=31,q=2;{PNG_B64}\x1b\\");
            s.next_slice(seq.as_bytes());
        }
        // The graphics_exec.rs:172 storage-access idiom.
        let img = unsafe { &(*t.screens.active).kitty_images }
            .image_by_id(31)
            .expect("transmitted image stored (PNG decoded by the default hook)");
        assert_eq!(50, img.width);
        assert_eq!(76, img.height);
        assert_eq!(img.format, kitty::graphics_command::Format::Rgba);
        t.deinit(Global);
    }

    // test "cursor movement"
    #[test]
    fn cursor_movement() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        {
            let mut s = t.vt_stream();
            // Move cursor using escape sequences
            s.next_slice(b"Hello\x1B[1;1H");
        }
        assert_eq!(0_usize, t.active_mut().cursor.x as usize);
        assert_eq!(0_usize, t.active_mut().cursor.y as usize);

        {
            let mut s = t.vt_stream();
            // Move to position 2,3
            s.next_slice(b"\x1B[2;3H");
        }
        assert_eq!(2_usize, t.active_mut().cursor.x as usize);
        assert_eq!(1_usize, t.active_mut().cursor.y as usize);

        t.deinit(Global);
    }

    // test "erase operations"
    #[test]
    fn erase_operations() {
        let mut t = Terminal::init(Global, opts(20, 10)).unwrap();

        {
            let mut s = t.vt_stream();
            // Print some text
            s.next_slice(b"Hello World");
        }
        assert_eq!(11_usize, t.active_mut().cursor.x as usize);
        assert_eq!(0_usize, t.active_mut().cursor.y as usize);

        {
            let mut s = t.vt_stream();
            // Move cursor to position 1,6 and erase from cursor to end of line
            s.next_slice(b"\x1B[1;6H");
            s.next_slice(b"\x1B[K");
        }

        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), b"Hello");

        t.deinit(Global);
    }

    // test "tabs"
    #[test]
    fn tabs() {
        let mut t = Terminal::init(Global, opts(80, 10)).unwrap();

        {
            let mut s = t.vt_stream();
            s.next_slice(b"A\tB");
        }
        assert_eq!(9_usize, t.active_mut().cursor.x as usize);

        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), b"A       B");

        t.deinit(Global);
    }

    // test "modes"
    #[test]
    fn modes() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        // Test wraparound mode
        assert!(t.modes.get(modespkg::Mode::Wraparound));
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1B[?7l"); // Disable wraparound
        }
        assert!(!t.modes.get(modespkg::Mode::Wraparound));
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1B[?7h"); // Enable wraparound
        }
        assert!(t.modes.get(modespkg::Mode::Wraparound));

        t.deinit(Global);
    }

    // test "scrolling regions"
    #[test]
    fn scrolling_regions() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        {
            let mut s = t.vt_stream();
            // Set scrolling region from line 5 to 20
            s.next_slice(b"\x1B[5;20r");
        }
        assert_eq!(4_usize, t.scrolling_region.top as usize);
        assert_eq!(19_usize, t.scrolling_region.bottom as usize);
        assert_eq!(0_usize, t.scrolling_region.left as usize);
        assert_eq!(79_usize, t.scrolling_region.right as usize);

        t.deinit(Global);
    }

    // test "charsets"
    #[test]
    fn charsets() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        {
            let mut s = t.vt_stream();
            // Configure G0 as DEC special graphics
            s.next_slice(b"\x1B(0");
            s.next_slice(b"`"); // Should print diamond character
        }

        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "◆".as_bytes());

        t.deinit(Global);
    }

    // test "alt screen"
    #[test]
    fn alt_screen() {
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

        {
            let mut s = t.vt_stream();
            // Write to primary screen
            s.next_slice(b"Primary");
        }
        assert_eq!(crate::screen_set::Key::Primary, t.screens.active_key);

        {
            let mut s = t.vt_stream();
            // Switch to alt screen
            s.next_slice(b"\x1B[?1049h");
        }
        assert_eq!(crate::screen_set::Key::Alternate, t.screens.active_key);

        {
            let mut s = t.vt_stream();
            // Write to alt screen
            s.next_slice(b"Alt");

            // Switch back to primary
            s.next_slice(b"\x1B[?1049l");
        }
        assert_eq!(crate::screen_set::Key::Primary, t.screens.active_key);

        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), b"Primary");

        t.deinit(Global);
    }

    // test "cursor save and restore"
    #[test]
    fn cursor_save_and_restore() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        {
            let mut s = t.vt_stream();
            // Move cursor to 10,15
            s.next_slice(b"\x1B[10;15H");
        }
        assert_eq!(14_usize, t.active_mut().cursor.x as usize);
        assert_eq!(9_usize, t.active_mut().cursor.y as usize);

        {
            let mut s = t.vt_stream();
            // Save cursor
            s.next_slice(b"\x1B7");
            // Move cursor elsewhere
            s.next_slice(b"\x1B[1;1H");
        }
        assert_eq!(0_usize, t.active_mut().cursor.x as usize);
        assert_eq!(0_usize, t.active_mut().cursor.y as usize);

        {
            let mut s = t.vt_stream();
            // Restore cursor
            s.next_slice(b"\x1B8");
        }
        assert_eq!(14_usize, t.active_mut().cursor.x as usize);
        assert_eq!(9_usize, t.active_mut().cursor.y as usize);

        t.deinit(Global);
    }

    // test "attributes"
    #[test]
    fn attributes() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        {
            let mut s = t.vt_stream();
            // Set bold and write text
            s.next_slice(b"\x1B[1mBold\x1B[0m");
        }

        // Verify we can write attributes - just check the string was written
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), b"Bold");

        t.deinit(Global);
    }

    // test "DECALN screen alignment"
    #[test]
    fn decaln_screen_alignment() {
        let mut t = Terminal::init(Global, opts(10, 3)).unwrap();

        {
            let mut s = t.vt_stream();
            // Run DECALN
            s.next_slice(b"\x1B#8");
        }

        // Verify entire screen is filled with 'E'
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), b"EEEEEEEEEE\nEEEEEEEEEE\nEEEEEEEEEE");

        // Cursor should be at 1,1
        assert_eq!(0_usize, t.active_mut().cursor.x as usize);
        assert_eq!(0_usize, t.active_mut().cursor.y as usize);

        t.deinit(Global);
    }

    // test "OSC 4 set and reset palette"
    #[test]
    fn osc_4_set_and_reset_palette() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        // Save default color
        let default_color_0 = t.colors.palette.original[0];

        {
            let mut s = t.vt_stream();
            // Set color 0 to red
            s.next_slice(b"\x1b]4;0;rgb:ff/00/00\x1b\\");
        }
        assert_eq!(0xff_u8, t.colors.palette.current[0].r);
        assert_eq!(0x00_u8, t.colors.palette.current[0].g);
        assert_eq!(0x00_u8, t.colors.palette.current[0].b);
        assert!(t.colors.palette.mask.is_set(0));

        {
            let mut s = t.vt_stream();
            // Reset color 0
            s.next_slice(b"\x1b]104;0\x1b\\");
        }
        assert_eq!(default_color_0, t.colors.palette.current[0]);
        assert!(!t.colors.palette.mask.is_set(0));

        t.deinit(Global);
    }

    // test "OSC 104 reset all palette colors"
    #[test]
    fn osc_104_reset_all_palette_colors() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        {
            let mut s = t.vt_stream();
            // Set multiple colors
            s.next_slice(b"\x1b]4;0;rgb:ff/00/00\x1b\\");
            s.next_slice(b"\x1b]4;1;rgb:00/ff/00\x1b\\");
            s.next_slice(b"\x1b]4;2;rgb:00/00/ff\x1b\\");
        }
        assert!(t.colors.palette.mask.is_set(0));
        assert!(t.colors.palette.mask.is_set(1));
        assert!(t.colors.palette.mask.is_set(2));

        {
            let mut s = t.vt_stream();
            // Reset all palette colors
            s.next_slice(b"\x1b]104\x1b\\");
        }
        assert_eq!(t.colors.palette.original[0], t.colors.palette.current[0]);
        assert_eq!(t.colors.palette.original[1], t.colors.palette.current[1]);
        assert_eq!(t.colors.palette.original[2], t.colors.palette.current[2]);
        assert!(!t.colors.palette.mask.is_set(0));
        assert!(!t.colors.palette.mask.is_set(1));
        assert!(!t.colors.palette.mask.is_set(2));

        t.deinit(Global);
    }

    // test "OSC 10 set and reset foreground color"
    #[test]
    fn osc_10_set_and_reset_foreground_color() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        // Initially unset
        assert!(t.colors.foreground.get().is_none());

        {
            let mut s = t.vt_stream();
            // Set foreground to red
            s.next_slice(b"\x1b]10;rgb:ff/00/00\x1b\\");
        }
        let fg = t.colors.foreground.get().unwrap();
        assert_eq!(0xff_u8, fg.r);
        assert_eq!(0x00_u8, fg.g);
        assert_eq!(0x00_u8, fg.b);

        {
            let mut s = t.vt_stream();
            // Reset foreground
            s.next_slice(b"\x1b]110\x1b\\");
        }
        assert!(t.colors.foreground.get().is_none());

        t.deinit(Global);
    }

    // test "OSC 11 set and reset background color"
    #[test]
    fn osc_11_set_and_reset_background_color() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        let default = color::RGB {
            r: 0x10,
            g: 0x20,
            b: 0x30,
        };
        t.colors.background.default = Some(default);

        {
            let mut s = t.vt_stream();
            // Set background to green
            s.next_slice(b"\x1b]11;rgb:00/ff/00\x1b\\");
        }
        let bg = t.colors.background.get().unwrap();
        assert_eq!(0x00_u8, bg.r);
        assert_eq!(0xff_u8, bg.g);
        assert_eq!(0x00_u8, bg.b);

        {
            let mut s = t.vt_stream();
            // Reset background
            s.next_slice(b"\x1b]111\x1b\\");
        }
        assert_eq!(default, t.colors.background.get().unwrap());
        assert_eq!(None, t.colors.background.override_);

        // A reset color continues to follow later configuration changes.
        let updated = color::RGB {
            r: 0x40,
            g: 0x50,
            b: 0x60,
        };
        t.colors.background.default = Some(updated);
        assert_eq!(updated, t.colors.background.get().unwrap());

        t.deinit(Global);
    }

    // test "OSC 12 set and reset cursor color"
    #[test]
    fn osc_12_set_and_reset_cursor_color() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        {
            let mut s = t.vt_stream();
            // Set cursor to blue
            s.next_slice(b"\x1b]12;rgb:00/00/ff\x1b\\");
        }
        let cursor = t.colors.cursor.get().unwrap();
        assert_eq!(0x00_u8, cursor.r);
        assert_eq!(0x00_u8, cursor.g);
        assert_eq!(0xff_u8, cursor.b);

        {
            let mut s = t.vt_stream();
            // Reset cursor
            s.next_slice(b"\x1b]112\x1b\\");
        }
        // After reset, cursor might be null (using default)

        t.deinit(Global);
    }

    // test "OSC color query responses" (oracle 14c829883)
    #[test]
    fn osc_color_query_responses() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);

        s.next_slice(b"\x1b]10;?\x1b\\");
        assert!(cap_take().is_none());

        s.next_slice(b"\x1b]11;?\x1b\\");
        assert!(cap_take().is_none());

        s.next_slice(b"\x1b]4;2;rgb:12/34/56;2;?\x1b\\");
        assert_eq!(
            cap_take().unwrap().as_slice(),
            b"\x1b]4;2;rgb:1212/3434/5656\x1b\\"
        );

        s.next_slice(b"\x1b]10;rgb:01/02/03\x1b\\");
        s.next_slice(b"\x1b]11;rgb:04/05/06\x1b\\");
        s.next_slice(b"\x1b]12;rgb:07/08/09\x1b\\");
        s.next_slice(b"\x1b]10;?;?;?\x1b\\");
        assert_eq!(
            cap_take().unwrap().as_slice(),
            [
                &b"\x1b]10;rgb:0101/0202/0303\x1b\\"[..],
                &b"\x1b]11;rgb:0404/0505/0606\x1b\\"[..],
                &b"\x1b]12;rgb:0707/0808/0909\x1b\\"[..],
            ]
            .concat()
            .as_slice()
        );

        s.next_slice(b"\x1b]112\x1b\\");
        s.next_slice(b"\x1b]12;?\x07");
        assert_eq!(
            cap_take().unwrap().as_slice(),
            b"\x1b]12;rgb:0101/0202/0303\x07"
        );

        drop(s);
        t.deinit(Global);
    }

    // test "kitty color protocol set palette"
    #[test]
    fn kitty_color_protocol_set_palette() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        {
            let mut s = t.vt_stream();
            // Set palette color 5 to magenta using kitty protocol
            s.next_slice(b"\x1b]21;5=rgb:ff/00/ff\x1b\\");
        }
        assert_eq!(0xff_u8, t.colors.palette.current[5].r);
        assert_eq!(0x00_u8, t.colors.palette.current[5].g);
        assert_eq!(0xff_u8, t.colors.palette.current[5].b);
        assert!(t.colors.palette.mask.is_set(5));
        assert!(t.flags.dirty.palette);

        t.deinit(Global);
    }

    // test "kitty color protocol reset palette"
    #[test]
    fn kitty_color_protocol_reset_palette() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        // Set and then reset palette color
        let original = t.colors.palette.original[7];
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b]21;7=rgb:aa/bb/cc\x1b\\");
        }
        assert!(t.colors.palette.mask.is_set(7));

        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b]21;7=\x1b\\");
        }
        assert_eq!(original, t.colors.palette.current[7]);
        assert!(!t.colors.palette.mask.is_set(7));

        t.deinit(Global);
    }

    // test "kitty color protocol set foreground"
    #[test]
    fn kitty_color_protocol_set_foreground() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        {
            let mut s = t.vt_stream();
            // Set foreground using kitty protocol
            s.next_slice(b"\x1b]21;foreground=rgb:12/34/56\x1b\\");
        }
        let fg = t.colors.foreground.get().unwrap();
        assert_eq!(0x12_u8, fg.r);
        assert_eq!(0x34_u8, fg.g);
        assert_eq!(0x56_u8, fg.b);

        t.deinit(Global);
    }

    // test "kitty color protocol set background"
    #[test]
    fn kitty_color_protocol_set_background() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        {
            let mut s = t.vt_stream();
            // Set background using kitty protocol
            s.next_slice(b"\x1b]21;background=rgb:78/9a/bc\x1b\\");
        }
        let bg = t.colors.background.get().unwrap();
        assert_eq!(0x78_u8, bg.r);
        assert_eq!(0x9a_u8, bg.g);
        assert_eq!(0xbc_u8, bg.b);

        t.deinit(Global);
    }

    // test "kitty color protocol set cursor"
    #[test]
    fn kitty_color_protocol_set_cursor() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        {
            let mut s = t.vt_stream();
            // Set cursor using kitty protocol
            s.next_slice(b"\x1b]21;cursor=rgb:de/f0/12\x1b\\");
        }
        let cursor = t.colors.cursor.get().unwrap();
        assert_eq!(0xde_u8, cursor.r);
        assert_eq!(0xf0_u8, cursor.g);
        assert_eq!(0x12_u8, cursor.b);

        t.deinit(Global);
    }

    // test "kitty color protocol reset foreground"
    #[test]
    fn kitty_color_protocol_reset_foreground() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        {
            let mut s = t.vt_stream();
            // Set and reset foreground
            s.next_slice(b"\x1b]21;foreground=rgb:11/22/33\x1b\\");
        }
        assert!(t.colors.foreground.get().is_some());

        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b]21;foreground=\x1b\\");
        }
        // After reset, should be unset
        assert!(t.colors.foreground.get().is_none());

        t.deinit(Global);
    }

    // test "kitty color protocol query responses" (oracle 14c829883)
    #[test]
    fn kitty_color_protocol_query_responses() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);

        s.next_slice(b"\x1b]21;background=?\x1b\\");
        assert_eq!(cap_take().unwrap().as_slice(), b"\x1b]21;background=\x1b\\");

        s.next_slice(b"\x1b]21;foreground=rgb:12/34/56;2=rgb:aa/bb/cc\x1b\\");
        s.next_slice(b"\x1b]21;foreground=?;background=?;2=?\x1b\\");
        assert_eq!(
            cap_take().unwrap().as_slice(),
            b"\x1b]21;foreground=rgb:12/34/56;background=;2=rgb:aa/bb/cc\x1b\\"
        );

        drop(s);
        t.deinit(Global);
    }

    // test "palette dirty flag set on color change"
    #[test]
    fn palette_dirty_flag_set_on_color_change() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        // Clear dirty flag
        t.flags.dirty.palette = false;

        {
            let mut s = t.vt_stream();
            // Setting palette color should set dirty flag
            s.next_slice(b"\x1b]4;0;rgb:ff/00/00\x1b\\");
        }
        assert!(t.flags.dirty.palette);

        // Clear and test reset
        t.flags.dirty.palette = false;
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b]104;0\x1b\\");
        }
        assert!(t.flags.dirty.palette);

        // Clear and test kitty protocol
        t.flags.dirty.palette = false;
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b]21;1=rgb:00/ff/00\x1b\\");
        }
        assert!(t.flags.dirty.palette);

        t.deinit(Global);
    }

    // test "semantic prompt fresh line"
    #[test]
    fn semantic_prompt_fresh_line() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        {
            let mut s = t.vt_stream();
            s.next_slice(b"Hello");
            s.next_slice(b"\x1b]133;L\x07");
        }
        assert_eq!(0_usize, t.active_mut().cursor.x as usize);
        assert_eq!(1_usize, t.active_mut().cursor.y as usize);

        t.deinit(Global);
    }

    // test "semantic prompt fresh line new prompt"
    #[test]
    fn semantic_prompt_fresh_line_new_prompt() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        {
            let mut s = t.vt_stream();
            // Write some text and then send OSC 133;A (fresh_line_new_prompt)
            s.next_slice(b"Hello");
            s.next_slice(b"\x1b]133;A\x07");
        }

        // Should do a fresh line (carriage return + index)
        assert_eq!(0_usize, t.active_mut().cursor.x as usize);
        assert_eq!(1_usize, t.active_mut().cursor.y as usize);

        // Should set cursor semantic_content to prompt
        assert_eq!(
            crate::page::SemanticContent::Prompt,
            t.active_mut().cursor.semantic_content
        );

        {
            let mut s = t.vt_stream();
            // Test with redraw option
            s.next_slice(b"prompt$ ");
            s.next_slice(b"\x1b]133;A;redraw=1\x07");
        }
        assert_eq!(
            crate::osc::semantic_prompt::Redraw::True,
            t.flags.shell_redraws_prompt
        );

        t.deinit(Global);
    }

    // test "semantic prompt end of input, then start output"
    #[test]
    fn semantic_prompt_end_of_input_then_start_output() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        {
            let mut s = t.vt_stream();
            // Write some text and then send OSC 133;A (fresh_line_new_prompt)
            s.next_slice(b"Hello");
            s.next_slice(b"\x1b]133;A\x07");
            s.next_slice(b"prompt$ ");
            s.next_slice(b"\x1b]133;B\x07");
        }
        assert_eq!(
            crate::page::SemanticContent::Input,
            t.active_mut().cursor.semantic_content
        );
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b]133;C\x07");
        }
        assert_eq!(
            crate::page::SemanticContent::Output,
            t.active_mut().cursor.semantic_content
        );

        t.deinit(Global);
    }

    // test "semantic prompt prompt_start"
    #[test]
    fn semantic_prompt_prompt_start() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        {
            let mut s = t.vt_stream();
            // Write some text
            s.next_slice(b"Hello");

            // OSC 133;P marks the start of a prompt (without fresh line behavior)
            s.next_slice(b"\x1b]133;P\x07");
        }
        assert_eq!(
            crate::page::SemanticContent::Prompt,
            t.active_mut().cursor.semantic_content
        );
        assert_eq!(5_usize, t.active_mut().cursor.x as usize);
        assert_eq!(0_usize, t.active_mut().cursor.y as usize);

        t.deinit(Global);
    }

    // test "semantic prompt new_command"
    #[test]
    fn semantic_prompt_new_command() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        {
            let mut s = t.vt_stream();
            // Write some text
            s.next_slice(b"Hello");
            s.next_slice(b"\x1b]133;N\x07");
        }

        // Should behave like fresh_line_new_prompt - cursor moves to column 0
        // on next line since we had content
        assert_eq!(0_usize, t.active_mut().cursor.x as usize);
        assert_eq!(1_usize, t.active_mut().cursor.y as usize);
        assert_eq!(
            crate::page::SemanticContent::Prompt,
            t.active_mut().cursor.semantic_content
        );

        t.deinit(Global);
    }

    // test "semantic prompt new_command at column zero"
    #[test]
    fn semantic_prompt_new_command_at_column_zero() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        {
            let mut s = t.vt_stream();
            // OSC 133;N when already at column 0 should stay on same line
            s.next_slice(b"\x1b]133;N\x07");
        }
        assert_eq!(0_usize, t.active_mut().cursor.x as usize);
        assert_eq!(0_usize, t.active_mut().cursor.y as usize);
        assert_eq!(
            crate::page::SemanticContent::Prompt,
            t.active_mut().cursor.semantic_content
        );

        t.deinit(Global);
    }

    // test "semantic prompt end_prompt_start_input_terminate_eol clears on linefeed"
    #[test]
    fn semantic_prompt_end_prompt_start_input_terminate_eol_clears_on_linefeed() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        {
            let mut s = t.vt_stream();
            // Set input terminated by EOL
            s.next_slice(b"\x1b]133;I\x07");
        }
        assert_eq!(
            crate::page::SemanticContent::Input,
            t.active_mut().cursor.semantic_content
        );

        {
            let mut s = t.vt_stream();
            // Linefeed should reset semantic content to output
            s.next_slice(b"\n");
        }
        assert_eq!(
            crate::page::SemanticContent::Output,
            t.active_mut().cursor.semantic_content
        );

        t.deinit(Global);
    }

    // test "full reset"
    #[test]
    fn full_reset() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        {
            let mut s = t.vt_stream();
            // Make some changes
            s.next_slice(b"Hello");
            s.next_slice(b"\x1B[10;20H");
            s.next_slice(b"\x1B[5;20r"); // Set scroll region
            s.next_slice(b"\x1B[?7l"); // Disable wraparound
            s.next_slice(b"\x1B_25a1;r;cp=e0a0;AAAAAAAAAAAAAA==\x1B\\");
        }
        // try testing.expect(t.glyph_glossary.contains(0xE0A0));
        assert!(t.glyph_glossary.contains(0xE0A0));

        {
            let mut s = t.vt_stream();
            // Full reset
            s.next_slice(b"\x1Bc");
        }

        // Verify reset state
        assert_eq!(0usize, t.active_mut().cursor.x as usize);
        assert_eq!(0usize, t.active_mut().cursor.y as usize);
        assert_eq!(0usize, t.scrolling_region.top as usize);
        assert_eq!(23usize, t.scrolling_region.bottom as usize);
        assert!(t.modes.get(modespkg::Mode::Wraparound));
        // try testing.expect(!t.glyph_glossary.contains(0xE0A0));
        assert!(!t.glyph_glossary.contains(0xE0A0));

        t.deinit(Global);
    }

    // test "glyph protocol APC with write_pty callback"
    #[test]
    fn glyph_protocol_apc_with_write_pty_callback() {
        // if (comptime !build_options.glyph_protocol) return error.SkipZigTest;
        if !crate::options::GLYPH_PROTOCOL {
            return;
        }

        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        cap_reset();
        let mut s = t.vt_stream();
        // handler.effects.write_pty = &S.writePty;
        s.handler.effects.write_pty = Some(cap_write_pty);

        // s.nextSlice("\x1B_25a1;s\x1B\\");
        s.next_slice(b"\x1B_25a1;s\x1B\\");
        // try testing.expectEqualStrings("\x1B_25a1;s;fmt=glyf\x1B\\", S.last_response.?);
        assert_eq!(
            cap_take().unwrap().as_slice(),
            b"\x1B_25a1;s;fmt=glyf\x1B\\"
        );

        // s.nextSlice("\x1B_25a1;r;cp=e0a0;AAAAAAAAAAAAAA==\x1B\\");
        s.next_slice(b"\x1B_25a1;r;cp=e0a0;AAAAAAAAAAAAAA==\x1B\\");
        // try testing.expectEqualStrings("\x1B_25a1;r;cp=e0a0;status=0\x1B\\", S.last_response.?);
        assert_eq!(
            cap_take().unwrap().as_slice(),
            b"\x1B_25a1;r;cp=e0a0;status=0\x1B\\"
        );
        // try testing.expect(t.glyph_glossary.contains(0xE0A0));
        drop(s);
        assert!(t.glyph_glossary.contains(0xE0A0));

        t.deinit(Global);
    }

    // test "ignores query actions"
    //
    // PORT NOTE (vt-12b-7b): reactivated now that the RESPONSE/QUERY arms are
    // filled. With default (readonly) effects, `\x1B[c` (device_attributes) and
    // `\x1B[5n`/`\x1B[6n` (device_status) are silent no-ops, so the terminal
    // remains functional afterward.
    #[test]
    fn ignores_query_actions() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        {
            let mut s = t.vt_stream();
            // These should be ignored without error
            s.next_slice(b"\x1B[c"); // Device attributes
            s.next_slice(b"\x1B[5n"); // Device status report
            s.next_slice(b"\x1B[6n"); // Cursor position report
            s.next_slice(b"\x1B]4;0;?\x1B\\"); // OSC color query
            s.next_slice(b"\x1B]21;foreground=?\x1B\\"); // Kitty color query
            s.next_slice(b"\x1B]52;c;%%%invalid-base64%%%\x1B\\");
            s.next_slice(b"\x1B_Ga=p,i=999\x1B\\"); // Missing Kitty image
            s.next_slice(b"\x1B_25a1;r;cp=41;%%%invalid%%%\x1B\\"); // Rejected glyph

            // Query, malformed input, protocol failure responses, and
            // external-effect failures do not imply that terminal-owned
            // semantic state diverged.
            assert!(!s.handler.semantic_failure);

            // Terminal should still be functional
            s.next_slice(b"Test");
        }
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), b"Test");

        t.deinit(Global);
    }

    // test "stream: CSI W with intermediate but no params"
    #[test]
    fn stream_csi_w_with_intermediate_but_no_params() {
        // Regression test from AFL++ crash. CSI ? W without
        // parameters caused an out-of-bounds access on input.params[0].
        let mut t = Terminal::init(
            Global,
            Options {
                max_scrollback_bytes: Some(100),
                max_scrollback_lines: None,
                ..opts(80, 24)
            },
        )
        .unwrap();

        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b[?W");
        }

        t.deinit(Global);
    }

    // test "bell effect callback"
    //
    // PORT NOTE (vt-12b-7a): the reference's callback-count half captures the
    // count in a local-struct `var` static (`S.bell_count`); Rust's `bell`
    // effect is a bare `fn` pointer (no closures/captures), so the count is
    // routed through the tests-module `COUNTER` thread_local cell — the same
    // analog the vt-12b-7b effect tests below use for `title_changed`.
    #[test]
    fn bell_effect_callback() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        // Test bell with null callback (default readonly effects) doesn't crash
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x07");

            // Terminal should still be functional after bell
            s.next_slice(b"AfterBell");
        }
        let str = t.plain_string(Global).unwrap();
        // try testing.expectEqualStrings("AfterBell", str);
        assert_eq!(str.as_slice(), b"AfterBell");

        // t.fullReset();
        t.full_reset();

        // Test bell with a callback
        {
            // const S = struct { var bell_count: usize = 0;
            //     fn bell(_: *Handler) void { bell_count += 1; } };
            fn bell(_: &mut Handler) {
                COUNTER.with(|c| c.set(c.get() + 1));
            }
            // S.bell_count = 0;
            cap_reset();

            // var handler: Handler = .init(&t); handler.effects.bell = &S.bell;
            let mut s = t.vt_stream();
            s.handler.effects.bell = Some(bell);

            s.next_slice(b"\x07");
            // try testing.expectEqual(@as(usize, 1), S.bell_count);
            assert_eq!(1_usize, COUNTER.with(|c| c.get()));

            s.next_slice(b"\x07\x07");
            // try testing.expectEqual(@as(usize, 3), S.bell_count);
            assert_eq!(3_usize, COUNTER.with(|c| c.get()));
        }

        t.deinit(Global);
    }

    // -- vt-12b-7b response/query effect tests --------------------------------
    //
    // PORT NOTE (vt-12b-7b): the reference tests capture the `write_pty` output
    // into a local-struct `var` static (`S.written`/`S.last_response`) inside a
    // `fn writePty(...)` callback. Rust's `write_pty` is a bare `fn` pointer (no
    // closures/captures), so the captured bytes are routed through a
    // `thread_local!` cell. The `CAPTURE` helpers below are the analog of the
    // reference's per-test `S` struct: `cap_write_pty` is the `writePty` callback
    // (it stores the latest response, freeing/replacing the prior one — matching
    // the reference's `if (old) free; dupe` pattern); `cap_take`/`cap_reset` read
    // and clear it. Effect callbacks that return fixed strings
    // (`enquiry`/`xtversion`/`getSize`/`colorScheme`/`da`/`titleChanged`) become
    // free `fn`s. The `title_changed`/`bell` counters likewise live in a cell.

    thread_local! {
        static CAPTURE: core::cell::RefCell<Option<Vec<u8>>> = const { core::cell::RefCell::new(None) };
        static COUNTER: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
    }

    fn cap_reset() {
        CAPTURE.with(|c| *c.borrow_mut() = None);
        COUNTER.with(|c| c.set(0));
    }

    fn cap_take() -> Option<Vec<u8>> {
        CAPTURE.with(|c| c.borrow().clone())
    }

    // fn writePty(_: *Handler, data: [:0]const u8) void { ...dupe into S.written... }
    fn cap_write_pty(_: &mut Handler, data: &[u8]) {
        CAPTURE.with(|c| *c.borrow_mut() = Some(data.to_vec()));
    }

    // test "request mode DECRQM with write_pty callback"
    #[test]
    fn request_mode_decrqm_with_write_pty_callback() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        // Without callback, DECRQM should not crash
        {
            let mut s = t.vt_stream();
            // DECRQM for mode 7 (wraparound) — should be silently ignored
            s.next_slice(b"\x1B[?7$p");
        }

        t.full_reset();

        // With callback, DECRQM should produce a response
        {
            cap_reset();
            let mut s = t.vt_stream();
            s.handler.effects.write_pty = Some(cap_write_pty);

            // Wraparound mode (7) is set by default
            s.next_slice(b"\x1B[?7$p");
            assert_eq!(cap_take().unwrap().as_slice(), b"\x1B[?7;1$y");

            // Disable wraparound and query again
            s.next_slice(b"\x1B[?7l");
            s.next_slice(b"\x1B[?7$p");
            assert_eq!(cap_take().unwrap().as_slice(), b"\x1B[?7;2$y");

            // Query an unknown mode
            s.next_slice(b"\x1B[?9999$p");
            assert_eq!(cap_take().unwrap().as_slice(), b"\x1B[?9999;0$y");

            // Query DECECM, which Rustty recognizes but does not allow changing
            s.next_slice(b"\x1B[?117$p");
            assert_eq!(cap_take().unwrap().as_slice(), b"\x1B[?117;4$y");
        }

        t.deinit(Global);
    }

    // Capture cells for the desktop_notification effect test (oracle c3655ba25).
    // Same `S`-struct-via-thread-local shape the clipboard test below uses.
    //
    // PER THE BYTE-SLICE RULE the captured title/body are `Vec<u8>`, never
    // `String`: OSC 9 / OSC 777 payloads are raw bytes from the pty and a
    // lossy UTF-8 conversion here would make the test unable to see a
    // non-UTF-8 title at all.
    thread_local! {
        static DN_COUNT: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
        static DN_TITLE: core::cell::RefCell<Vec<u8>> =
            const { core::cell::RefCell::new(Vec::new()) };
        static DN_BODY: core::cell::RefCell<Vec<u8>> =
            const { core::cell::RefCell::new(Vec::new()) };
    }

    fn cap_desktop_notification(_: &mut Handler, n: stream::ShowDesktopNotification) {
        DN_COUNT.with(|c| c.set(c.get() + 1));
        DN_TITLE.with(|c| *c.borrow_mut() = n.title.to_vec());
        DN_BODY.with(|c| *c.borrow_mut() = n.body.to_vec());
    }

    // test "desktop_notification effect callback" (oracle c3655ba25)
    #[test]
    fn desktop_notification_effect_callback() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        // A null callback (the default readonly effects) silently ignores
        // notifications and leaves the terminal usable.
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1B]9;Ignored\x1B\\AfterNotification");
        }
        let str_ = t.plain_string(Global).unwrap();
        assert_eq!(b"AfterNotification".as_slice(), str_.as_slice());

        t.full_reset();

        DN_COUNT.with(|c| c.set(0));
        DN_TITLE.with(|c| c.borrow_mut().clear());
        DN_BODY.with(|c| c.borrow_mut().clear());

        let mut s = t.vt_stream();
        s.handler.effects.desktop_notification = Some(cap_desktop_notification);

        // OSC 9 is split across writes and carries only a body.
        s.next_slice(b"\x1B]9;Build ");
        assert_eq!(0usize, DN_COUNT.with(|c| c.get()));
        s.next_slice(b"complete\x1B\\");
        assert_eq!(1usize, DN_COUNT.with(|c| c.get()));
        assert_eq!(
            b"".as_slice(),
            DN_TITLE.with(|c| c.borrow().clone()).as_slice()
        );
        assert_eq!(
            b"Build complete".as_slice(),
            DN_BODY.with(|c| c.borrow().clone()).as_slice()
        );

        // OSC 777 preserves its separate title and body fields.
        s.next_slice(b"\x1B]777;notify;Codex;Needs attention\x07");
        assert_eq!(2usize, DN_COUNT.with(|c| c.get()));
        assert_eq!(
            b"Codex".as_slice(),
            DN_TITLE.with(|c| c.borrow().clone()).as_slice()
        );
        assert_eq!(
            b"Needs attention".as_slice(),
            DN_BODY.with(|c| c.borrow().clone()).as_slice()
        );

        t.deinit(Global);
    }

    // Capture cells for the progress_report effect test (oracle 47d602c42).
    thread_local! {
        static PR_COUNT: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
        static PR_STATE: core::cell::Cell<osc::ProgressReportState> =
            const { core::cell::Cell::new(osc::ProgressReportState::Remove) };
        static PR_PROGRESS: core::cell::Cell<Option<u8>> = const { core::cell::Cell::new(None) };
    }

    fn cap_progress_report(_: &mut Handler, r: osc::ProgressReport) {
        PR_COUNT.with(|c| c.set(c.get() + 1));
        PR_STATE.with(|c| c.set(r.state));
        PR_PROGRESS.with(|c| c.set(r.progress));
    }

    // test "progress_report effect callback" (oracle 47d602c42)
    #[test]
    fn progress_report_effect_callback() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        // A null callback (the default readonly effects) silently ignores reports.
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1B]9;4;1;25\x1B\\");
        }

        PR_COUNT.with(|c| c.set(0));
        PR_STATE.with(|c| c.set(osc::ProgressReportState::Remove));
        PR_PROGRESS.with(|c| c.set(None));

        let mut s = t.vt_stream();
        s.handler.effects.progress_report = Some(cap_progress_report);

        let cases: &[(&[u8], osc::ProgressReportState, Option<u8>)] = &[
            (b"\x1B]9;4;0;\x1B\\", osc::ProgressReportState::Remove, None),
            (
                b"\x1B]9;4;1;42\x07",
                osc::ProgressReportState::Set,
                Some(42),
            ),
            (
                b"\x1B]9;4;2;7\x1B\\",
                osc::ProgressReportState::Error,
                Some(7),
            ),
            (
                b"\x1B]9;4;3\x1B\\",
                osc::ProgressReportState::Indeterminate,
                None,
            ),
            (
                b"\x1B]9;4;4;75\x1B\\",
                osc::ProgressReportState::Pause,
                Some(75),
            ),
        ];

        for (i, (sequence, state, progress)) in cases.iter().enumerate() {
            let expected_count = i + 1;
            // Split each sequence to verify parsing survives PTY read boundaries.
            let midpoint = sequence.len() / 2;
            s.next_slice(&sequence[0..midpoint]);
            assert_eq!(expected_count - 1, PR_COUNT.with(|c| c.get()));
            s.next_slice(&sequence[midpoint..]);
            assert_eq!(expected_count, PR_COUNT.with(|c| c.get()));
            assert_eq!(*state, PR_STATE.with(|c| c.get()));
            assert_eq!(*progress, PR_PROGRESS.with(|c| c.get()));
        }

        // A full reset (RIS) removes any active progress bar.
        // s.nextSlice("\x1B]9;4;1;50\x1B\\");
        s.next_slice(b"\x1B]9;4;1;50\x1B\\");
        // try testing.expectEqual(@as(usize, cases.len + 1), S.count);
        assert_eq!(cases.len() + 1, PR_COUNT.with(|c| c.get()));
        // try testing.expectEqual(osc.Command.ProgressReport.State.set, S.last_state);
        assert_eq!(osc::ProgressReportState::Set, PR_STATE.with(|c| c.get()));
        // s.nextSlice("\x1Bc");
        s.next_slice(b"\x1Bc");
        // try testing.expectEqual(@as(usize, cases.len + 2), S.count);
        assert_eq!(cases.len() + 2, PR_COUNT.with(|c| c.get()));
        // try testing.expectEqual(osc.Command.ProgressReport.State.remove, S.last_state);
        assert_eq!(osc::ProgressReportState::Remove, PR_STATE.with(|c| c.get()));
        // try testing.expectEqual(@as(?u8, null), S.last_progress);
        assert_eq!(None::<u8>, PR_PROGRESS.with(|c| c.get()));

        t.deinit(Global);
    }

    // Capture cells for the clipboard_write effect test (the analog of the
    // reference's per-test `S` struct; bare `fn` callbacks route through
    // thread-locals since Rust fn pointers cannot capture).
    thread_local! {
        static CLIP_COUNT: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
        static CLIP_RESULT: core::cell::Cell<clipboard::WriteResult> =
            const { core::cell::Cell::new(clipboard::WriteResult::Success(
                clipboard::WriteResultSuccess { remember: false },
            )) };
        static CLIP_LOCATION: core::cell::Cell<clipboard::Location> =
            const { core::cell::Cell::new(clipboard::Location::Standard) };
        static CLIP_CONTENTS_LEN: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
        static CLIP_MIME: core::cell::RefCell<Option<Vec<u8>>> = const { core::cell::RefCell::new(None) };
        static CLIP_DATA: core::cell::RefCell<Option<Vec<u8>>> = const { core::cell::RefCell::new(None) };
    }

    fn clip_clear_capture() {
        CLIP_MIME.with(|c| *c.borrow_mut() = None);
        CLIP_DATA.with(|c| *c.borrow_mut() = None);
        CLIP_CONTENTS_LEN.with(|c| c.set(0));
    }

    // fn clipboardWrite(_: *Handler, write: clipboard.Write) void { ...
    //     write.reply(result); }
    fn cap_clipboard_write(_: &mut Handler, write: clipboard::Write) {
        clip_clear_capture();
        CLIP_COUNT.with(|c| c.set(c.get() + 1));
        CLIP_LOCATION.with(|c| c.set(write.location));
        CLIP_CONTENTS_LEN.with(|c| c.set(write.contents.len()));
        if !write.contents.is_empty() {
            CLIP_MIME.with(|c| *c.borrow_mut() = Some(write.contents[0].mime.to_vec()));
            CLIP_DATA.with(|c| *c.borrow_mut() = Some(write.contents[0].data.to_vec()));
        }
        write.reply(CLIP_RESULT.with(|c| c.get()));
    }

    // test "clipboard_write effect callback" (oracle 634ef7198)
    #[test]
    fn clipboard_write_effect_callback() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        // A null callback (the default readonly effects) silently ignores writes.
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1B]52;c;aGVsbG8=\x1B\\");

            // Terminal should still be functional after the ignored sequence
            s.next_slice(b"AfterClipboard");
        }
        let str = t.plain_string(Global).unwrap();
        assert_eq!(b"AfterClipboard".as_slice(), str.as_slice());

        t.full_reset();

        CLIP_COUNT.with(|c| c.set(0));
        CLIP_RESULT.with(|c| c.set(clipboard::WriteResult::Denied));
        clip_clear_capture();

        let mut s = t.vt_stream();
        s.handler.effects.clipboard_write = Some(cap_clipboard_write);

        // Selectors are normalized and payloads are decoded before the callback.
        let cases: &[(&[u8], clipboard::Location, &[u8])] = &[
            (
                b"\x1B]52;c;aGVsbG8=\x1B\\",
                clipboard::Location::Standard,
                b"hello",
            ),
            (
                b"\x1B]52;s;d29ybGQ=\x07",
                clipboard::Location::Selection,
                b"world",
            ),
            (
                b"\x1B]52;p;cHJpbWFyeQ==\x1B\\",
                clipboard::Location::Primary,
                b"primary",
            ),
            (
                b"\x1B]52;0;Y3V0\x1B\\",
                clipboard::Location::Standard,
                b"cut",
            ),
            (
                b"\x1B]52;x;ZmFsbGJhY2s=\x1B\\",
                clipboard::Location::Standard,
                b"fallback",
            ),
            (
                b"\x1B]52;c;YQBi\x1B\\",
                clipboard::Location::Standard,
                b"a\x00b",
            ),
            // Missing padding is tolerated for OSC 52 since it has no way
            // to report errors to the client, matching kitty.
            (
                b"\x1B]52;c;dW5wYWRkZWQ\x1B\\",
                clipboard::Location::Standard,
                b"unpadded",
            ),
        ];

        for (i, (sequence, location, data)) in cases.iter().enumerate() {
            s.next_slice(sequence);
            assert_eq!(i + 1, CLIP_COUNT.with(|c| c.get()));
            assert_eq!(*location, CLIP_LOCATION.with(|c| c.get()));
            assert_eq!(1, CLIP_CONTENTS_LEN.with(|c| c.get()));
            assert_eq!(
                b"text/plain".as_slice(),
                CLIP_MIME.with(|c| c.borrow().clone()).unwrap().as_slice()
            );
            assert_eq!(
                *data,
                CLIP_DATA.with(|c| c.borrow().clone()).unwrap().as_slice()
            );
        }

        // Empty data is a clear, represented by an empty contents slice.
        s.next_slice(b"\x1B]52;s;\x1B\\");
        assert_eq!(cases.len() + 1, CLIP_COUNT.with(|c| c.get()));
        assert_eq!(
            clipboard::Location::Selection,
            CLIP_LOCATION.with(|c| c.get())
        );
        assert_eq!(0, CLIP_CONTENTS_LEN.with(|c| c.get()));
        assert!(CLIP_MIME.with(|c| c.borrow().is_none()));
        assert!(CLIP_DATA.with(|c| c.borrow().is_none()));

        // Reads and malformed base64 are ignored. The whole request is
        // discarded on invalid characters (including whitespace) rather
        // than decoding around them, per the Kitty clipboard spec that
        // governs OSC 52 base64 handling.
        s.next_slice(b"\x1B]52;c;?\x1B\\");
        s.next_slice(b"\x1B]52;c;***\x1B\\");
        s.next_slice(b"\x1B]52;c;SGVs!!!bG8=\x1B\\");
        s.next_slice(b"\x1B]52;c;aGVs bG8=\x1B\\");
        assert_eq!(cases.len() + 1, CLIP_COUNT.with(|c| c.get()));

        // OSC 1337 Copy shares the normalized clipboard write path.
        s.next_slice(b"\x1B]1337;Copy=:aVRlcm0y\x1B\\");
        assert_eq!(cases.len() + 2, CLIP_COUNT.with(|c| c.get()));
        assert_eq!(
            clipboard::Location::Standard,
            CLIP_LOCATION.with(|c| c.get())
        );
        assert_eq!(
            b"text/plain".as_slice(),
            CLIP_MIME.with(|c| c.borrow().clone()).unwrap().as_slice()
        );
        assert_eq!(
            b"iTerm2".as_slice(),
            CLIP_DATA.with(|c| c.borrow().clone()).unwrap().as_slice()
        );

        // Parsing across write boundaries still invokes exactly one atomic write.
        s.next_slice(b"\x1B]52;p;ZnJh");
        s.next_slice(b"Z21lbnRlZA==\x1B");
        s.next_slice(b"\\");
        assert_eq!(cases.len() + 3, CLIP_COUNT.with(|c| c.get()));
        assert_eq!(
            clipboard::Location::Primary,
            CLIP_LOCATION.with(|c| c.get())
        );
        assert_eq!(
            b"text/plain".as_slice(),
            CLIP_MIME.with(|c| c.borrow().clone()).unwrap().as_slice()
        );
        assert_eq!(
            b"fragmented".as_slice(),
            CLIP_DATA.with(|c| c.borrow().clone()).unwrap().as_slice()
        );

        // Reply results are intentionally ignored for protocols without a
        // write acknowledgement. The denied reply above did not stop later writes.
        assert_eq!(
            clipboard::WriteResult::Denied,
            CLIP_RESULT.with(|c| c.get())
        );

        clip_clear_capture();
        t.deinit(Global);
    }

    // test "clipboard_write allocation failure is ignored"
    //
    // PORT NOTE: the reference injects `testing.failing_allocator` by swapping
    // `t.screens.active.alloc` for the duration of the sequence. rustty's
    // allocator seam is the zero-sized `allocator_api2::alloc::Global`, so
    // there is nothing to swap; the `CLIPBOARD_CONTENTS_TW` fail point sits
    // exactly where the reference's `try alloc.alloc(u8, decoded_len)` is, and
    // makes that allocation fail instead.
    //
    // WS18 release lane: `tripwire::Module::enabled()` is the union of
    // `cfg!(test)`, `cfg!(debug_assertions)` and the `tripwire-test` feature
    // (a dev-dependency feature of this crate), reproducing Zig's
    // `builtin.is_test`, so this runs in the release test lane too.
    #[test]
    fn clipboard_write_allocation_failure_is_ignored() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        CLIP_COUNT.with(|c| c.set(0));
        CLIP_RESULT.with(|c| {
            c.set(clipboard::WriteResult::Success(
                clipboard::WriteResultSuccess::default(),
            ))
        });
        clip_clear_capture();

        {
            let mut s = t.vt_stream();
            s.handler.effects.clipboard_write = Some(cap_clipboard_write);

            // Only the decoded scratch data uses the terminal allocator here.
            clipboard_contents_tw_error_always(ClipboardContentsFp::DecodeScratch);
            s.next_slice(b"\x1B]52;c;aGVsbG8=\x1B\\");
            clipboard_contents_tw_end(rustty_util::tripwire::ResetMode::Reset).unwrap();

            assert_eq!(0, CLIP_COUNT.with(|c| c.get()));
            assert!(!s.handler.semantic_failure);

            // The failure is swallowed, not fatal: the very same sequence
            // succeeds once the allocation does.
            s.next_slice(b"\x1B]52;c;aGVsbG8=\x1B\\");
            assert_eq!(1, CLIP_COUNT.with(|c| c.get()));
            assert_eq!(
                b"hello".as_slice(),
                CLIP_DATA.with(|c| c.borrow().clone()).unwrap().as_slice()
            );
        }

        clip_clear_capture();
        t.deinit(Global);
    }

    // test "window_title effect is called"
    #[test]
    fn window_title_effect_is_called() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        // fn titleChanged(_: *Handler) void { title_changed_count += 1; }
        fn title_changed(_: &mut Handler) {
            COUNTER.with(|c| c.set(c.get() + 1));
        }

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.title_changed = Some(title_changed);

        {
            // Set window title via OSC 2
            s.next_slice(b"\x1b]2;Hello World\x1b\\");
        }
        assert_eq!(t.get_title().unwrap(), b"Hello World");
        assert_eq!(1_usize, COUNTER.with(|c| c.get()));

        t.deinit(Global);
    }

    // test "window_title effect not called without callback"
    #[test]
    fn window_title_effect_not_called_without_callback() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        {
            let mut s = t.vt_stream();
            // Should not crash when no callback is set
            s.next_slice(b"\x1b]2;Hello World\x1b\\");
        }

        // Title should still be set on terminal state
        assert_eq!(t.get_title().unwrap(), b"Hello World");

        {
            let mut s = t.vt_stream();
            // Terminal should still be functional
            s.next_slice(b"Test");
        }
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), b"Test");

        t.deinit(Global);
    }

    // test "window_title effect with empty title"
    #[test]
    fn window_title_effect_with_empty_title() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        fn title_changed(_: &mut Handler) {
            COUNTER.with(|c| c.set(c.get() + 1));
        }

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.title_changed = Some(title_changed);

        {
            // Set empty window title
            s.next_slice(b"\x1b]2;\x1b\\");
        }
        assert!(t.get_title().is_none());
        assert_eq!(1_usize, COUNTER.with(|c| c.get()));

        t.deinit(Global);
    }

    // S166-F3: `do_window_title` logs `log.warn("title length {d} exceeds max
    // length {d}, truncating", .{...})` (stream_terminal.zig:1471-1474) when
    // the OSC 2 title payload exceeds 1024 bytes.
    #[test]
    fn window_title_over_max_length_logs_and_truncates() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        let mut osc = Vec::new();
        osc.extend_from_slice(b"\x1b]2;");
        osc.extend(std::iter::repeat(b'A').take(2000));
        osc.extend_from_slice(b"\x1b\\");

        let cap = crate::log_capture::start();
        let mut s = t.vt_stream();
        s.next_slice(&osc);
        let messages = cap.messages();
        assert!(
            messages
                .iter()
                .any(|(level, target, msg)| *level == log::Level::Warn
                    && target == "stream_terminal"
                    && msg.starts_with("title length 2000 exceeds max length 1024, truncating")),
            "expected the title-length-exceeded diagnostic, got: {messages:?}"
        );
        drop(cap);

        assert_eq!(1024, t.get_title().unwrap().len());

        t.deinit(Global);
    }

    // test "kitty_keyboard_query"
    #[test]
    fn kitty_keyboard_query() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);

        // Default kitty keyboard flags should be 0
        s.next_slice(b"\x1b[?u");
        assert_eq!(cap_take().unwrap().as_slice(), b"\x1b[?0u");

        // Push kitty keyboard mode with flags and query again
        cap_reset();
        s.next_slice(b"\x1b[>1u"); // push with disambiguate flag
        s.next_slice(b"\x1b[?u");
        assert_eq!(cap_take().unwrap().as_slice(), b"\x1b[?1u");

        drop(s);
        t.deinit(Global);
    }

    // test "xtversion default"
    //
    // PORT NOTE (R2 ghostty->Rustty): the reference asserts the fallback version
    // "libghostty"; the retargeted fallback emits "librustty".
    #[test]
    fn xtversion_default() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);

        {
            // Without xtversion effect set, should report "librustty"
            s.next_slice(b"\x1b[>0q");
        }
        assert_eq!(cap_take().unwrap().as_slice(), b"\x1bP>|librustty\x1b\\");

        t.deinit(Global);
    }

    // test "xtversion with effect"
    #[test]
    fn xtversion_with_effect() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        // fn xtversion(_: *Handler) []const u8 { return "ghostty 1.2.3"; }
        fn xtversion(_: &mut Handler) -> &'static [u8] {
            b"ghostty 1.2.3"
        }

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);
        s.handler.effects.xtversion = Some(xtversion);

        {
            s.next_slice(b"\x1b[>0q");
        }
        assert_eq!(
            cap_take().unwrap().as_slice(),
            b"\x1bP>|ghostty 1.2.3\x1b\\"
        );

        t.deinit(Global);
    }

    // test "xtversion with empty string effect"
    //
    // PORT NOTE (R2 ghostty->Rustty): the empty-string effect falls back to the
    // retargeted "librustty".
    #[test]
    fn xtversion_with_empty_string_effect() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        // fn xtversion(_: *Handler) []const u8 { return ""; }
        fn xtversion(_: &mut Handler) -> &'static [u8] {
            b""
        }

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);
        s.handler.effects.xtversion = Some(xtversion);

        {
            // Empty string from effect should fall back to "librustty"
            s.next_slice(b"\x1b[>0q");
        }
        assert_eq!(cap_take().unwrap().as_slice(), b"\x1bP>|librustty\x1b\\");

        t.deinit(Global);
    }

    // NEW TESTS (no oracle counterpart): the reference formats the response
    // with `std.fmt.bufPrintZ` into a fixed `[288]u8` and `catch return`s, so
    // the response (framing + version + NUL) must fit in 288 bytes. That caps
    // the effective version string at 288 - 1 - 4 - 2 = 281 bytes; a 282-byte
    // version produces NO pty write at all.
    #[test]
    fn xtversion_281_byte_version_is_written() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        static V281: [u8; 281] = [b'v'; 281];
        fn xtversion(_: &mut Handler) -> &'static [u8] {
            &V281
        }

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);
        s.handler.effects.xtversion = Some(xtversion);
        s.next_slice(b"\x1b[>0q");

        let mut expect: Vec<u8> = Vec::new();
        expect.extend_from_slice(b"\x1bP>|");
        expect.extend_from_slice(&V281);
        expect.extend_from_slice(b"\x1b\\");
        assert_eq!(287, expect.len());
        assert_eq!(expect.as_slice(), cap_take().unwrap().as_slice());

        t.deinit(Global);
    }

    #[test]
    fn xtversion_282_byte_version_is_suppressed() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        static V282: [u8; 282] = [b'v'; 282];
        fn xtversion(_: &mut Handler) -> &'static [u8] {
            &V282
        }

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);
        s.handler.effects.xtversion = Some(xtversion);
        s.next_slice(b"\x1b[>0q");

        assert_eq!(None, cap_take());

        t.deinit(Global);
    }

    // fn getSize(_: *Handler) ?size_report.Size {
    //     return .{ .rows = 24, .columns = 80, .cell_width = 9, .cell_height = 18 };
    // }
    fn get_size(_: &mut Handler) -> Option<size_report::Size> {
        Some(size_report::Size {
            rows: 24,
            columns: 80,
            cell_width: 9,
            cell_height: 18,
        })
    }

    // test "size report csi_14_t with effect"
    #[test]
    fn size_report_csi_14_t_with_effect() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);
        s.handler.effects.size = Some(get_size);

        {
            // CSI 14 t - report text area size in pixels
            s.next_slice(b"\x1b[14t");
        }
        assert_eq!(cap_take().unwrap().as_slice(), b"\x1b[4;432;720t");

        t.deinit(Global);
    }

    // test "size report csi_16_t with effect"
    #[test]
    fn size_report_csi_16_t_with_effect() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);
        s.handler.effects.size = Some(get_size);

        {
            // CSI 16 t - report cell size in pixels
            s.next_slice(b"\x1b[16t");
        }
        assert_eq!(cap_take().unwrap().as_slice(), b"\x1b[6;18;9t");

        t.deinit(Global);
    }

    // test "size report csi_18_t with effect"
    #[test]
    fn size_report_csi_18_t_with_effect() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);
        s.handler.effects.size = Some(get_size);

        {
            // CSI 18 t - report text area size in characters
            s.next_slice(b"\x1b[18t");
        }
        assert_eq!(cap_take().unwrap().as_slice(), b"\x1b[8;24;80t");

        t.deinit(Global);
    }

    // test "size report no effect callback"
    #[test]
    fn size_report_no_effect_callback() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);

        {
            // Without size effect, size reports should be silently ignored
            s.next_slice(b"\x1b[14t");
        }
        assert!(cap_take().is_none());

        t.deinit(Global);
    }

    // test "size report csi_21_t title disabled by default"
    #[test]
    fn size_report_csi_21_t_title_disabled_by_default() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);

        {
            // Set a title first
            s.next_slice(b"\x1b]2;My Title\x1b\\");

            // CSI 21 t - report title (no size effect needed)
            s.next_slice(b"\x1b[21t");
        }
        assert!(cap_take().is_none());

        t.deinit(Global);
    }

    // test "size report csi_21_t title enabled"
    #[test]
    fn size_report_csi_21_t_title_enabled() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);
        s.handler.title_report = true;

        {
            // Set a title first
            s.next_slice(b"\x1b]2;My Title\x1b\\");

            // CSI 21 t - report title (no size effect needed)
            s.next_slice(b"\x1b[21t");
        }
        assert_eq!(cap_take().unwrap().as_slice(), b"\x1b]lMy Title\x1b\\");

        t.deinit(Global);
    }

    // test "enquiry no effect"
    #[test]
    fn enquiry_no_effect() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);

        {
            // ENQ without enquiry effect should not write anything
            s.next_slice(b"\x05");
        }
        assert!(cap_take().is_none());

        t.deinit(Global);
    }

    // test "enquiry with effect"
    #[test]
    fn enquiry_with_effect() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        // fn enquiry(_: *Handler) []const u8 { return "ghostty"; }
        fn enquiry(_: &mut Handler) -> &'static [u8] {
            b"ghostty"
        }

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);
        s.handler.effects.enquiry = Some(enquiry);

        {
            s.next_slice(b"\x05");
        }
        assert_eq!(cap_take().unwrap().as_slice(), b"ghostty");

        t.deinit(Global);
    }

    // test "enquiry with empty response"
    #[test]
    fn enquiry_with_empty_response() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        // fn enquiry(_: *Handler) []const u8 { return ""; }
        fn enquiry(_: &mut Handler) -> &'static [u8] {
            b""
        }

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);
        s.handler.effects.enquiry = Some(enquiry);

        {
            // Empty enquiry response should not write anything
            s.next_slice(b"\x05");
        }
        assert!(cap_take().is_none());

        t.deinit(Global);
    }

    // test "device status: operating status"
    #[test]
    fn device_status_operating_status() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);

        {
            // CSI 5 n — operating status report
            s.next_slice(b"\x1B[5n");
        }
        assert_eq!(cap_take().unwrap().as_slice(), b"\x1B[0n");

        t.deinit(Global);
    }

    // test "device status: cursor position"
    #[test]
    fn device_status_cursor_position() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);

        // Default position is 0,0 — reported as 1,1
        s.next_slice(b"\x1B[6n");
        assert_eq!(cap_take().unwrap().as_slice(), b"\x1B[1;1R");

        // Move cursor to row 5, col 10
        s.next_slice(b"\x1B[5;10H");
        s.next_slice(b"\x1B[6n");
        assert_eq!(cap_take().unwrap().as_slice(), b"\x1B[5;10R");

        drop(s);
        t.deinit(Global);
    }

    // test "device status: cursor position with origin mode"
    #[test]
    fn device_status_cursor_position_with_origin_mode() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);

        {
            // Set scroll region rows 5-20
            s.next_slice(b"\x1B[5;20r");
            // Enable origin mode
            s.next_slice(b"\x1B[?6h");
            // Move to row 3, col 5 within the region
            s.next_slice(b"\x1B[3;5H");
            // Query cursor position
            s.next_slice(b"\x1B[6n");
        }
        // Should report position relative to the scroll region
        assert_eq!(cap_take().unwrap().as_slice(), b"\x1B[3;5R");

        t.deinit(Global);
    }

    // fn colorScheme(_: *Handler) ?device_status.ColorScheme { return .dark; }
    fn color_scheme_dark(_: &mut Handler) -> Option<device_status::ColorScheme> {
        Some(device_status::ColorScheme::Dark)
    }

    // fn colorScheme(_: *Handler) ?device_status.ColorScheme { return .light; }
    fn color_scheme_light(_: &mut Handler) -> Option<device_status::ColorScheme> {
        Some(device_status::ColorScheme::Light)
    }

    // test "device status: color scheme dark"
    #[test]
    fn device_status_color_scheme_dark() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);
        s.handler.effects.color_scheme = Some(color_scheme_dark);

        {
            // CSI ? 996 n — color scheme query
            s.next_slice(b"\x1B[?996n");
        }
        assert_eq!(cap_take().unwrap().as_slice(), b"\x1B[?997;1n");

        t.deinit(Global);
    }

    // test "device status: color scheme light"
    #[test]
    fn device_status_color_scheme_light() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);
        s.handler.effects.color_scheme = Some(color_scheme_light);

        {
            // CSI ? 996 n — color scheme query
            s.next_slice(b"\x1B[?996n");
        }
        assert_eq!(cap_take().unwrap().as_slice(), b"\x1B[?997;2n");

        t.deinit(Global);
    }

    // test "device status: color scheme without callback"
    #[test]
    fn device_status_color_scheme_without_callback() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);

        {
            // Without color_scheme effect, query should be silently ignored
            s.next_slice(b"\x1B[?996n");
        }
        assert!(cap_take().is_none());

        t.deinit(Global);
    }

    // test "visibility reports"
    //
    // NEGATIVE CONTROL (P3-W4 T7 Step 5), vt half. Three independently
    // droppable behaviours, three assertions:
    //   (1) mode 2033 exists and an explicit CSI ? 998 n query answers without
    //       enabling it,
    //   (2) ENABLING always sends an immediate report, even when already
    //       enabled (and DISABLING sends nothing) — a count assertion, because
    //       a value assertion cannot tell "sent again" from "still the old
    //       value",
    //   (3) RIS PRESERVES `flags.visible` (the exemption). Removing the
    //       exemption makes the final assertion fail.
    #[test]
    fn visibility_reports() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        calls_reset();

        {
            let mut s = t.vt_stream();
            s.handler.effects.write_pty = Some(count_write_pty);

            // Mode 2033 is supported and initially disabled.
            s.next_slice(b"\x1B[?2033$p");
            assert_eq!(b"\x1B[?2033;2$y".as_slice(), cap_take().unwrap().as_slice());

            // A one-shot query reports the current state without enabling the
            // mode.
            s.next_slice(b"\x1B[?998n");
            assert_eq!(b"\x1B[?999;1n".as_slice(), cap_take().unwrap().as_slice());
            assert!(!s.handler.term().modes.get(modespkg::Mode::ReportVisibility));

            // Enabling always sends an immediate report, even when already
            // enabled.
            s.handler.term().flags.visible = false;
            s.next_slice(b"\x1B[?2033h");
            assert_eq!(b"\x1B[?999;2n".as_slice(), cap_take().unwrap().as_slice());
            let count = calls_get();
            s.next_slice(b"\x1B[?2033h");
            assert_eq!(count + 1, calls_get());

            // Disabling sends no report.
            s.next_slice(b"\x1B[?2033l");
            assert_eq!(count + 1, calls_get());

            // A terminal reset preserves the view's externally owned
            // visibility.
            s.next_slice(b"\x1Bc");
            s.next_slice(b"\x1B[?998n");
            assert_eq!(b"\x1B[?999;2n".as_slice(), cap_take().unwrap().as_slice());
        }

        t.deinit(Global);
    }

    // test "device status: readonly ignores all"
    #[test]
    fn device_status_readonly_ignores_all() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        {
            let mut s = t.vt_stream();
            // All device status queries should be silently ignored without effects
            s.next_slice(b"\x1B[5n");
            s.next_slice(b"\x1B[6n");
            s.next_slice(b"\x1B[?996n");
            s.next_slice(b"\x1B[?998n");

            // Terminal should still be functional
            s.next_slice(b"Test");
        }
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), b"Test");

        t.deinit(Global);
    }

    // fn da(_: *Handler) device_attributes.Attributes { return .{}; }
    fn da_default(_: &mut Handler) -> device_attributes::Attributes {
        device_attributes::Attributes::default()
    }

    // test "device attributes: primary DA"
    #[test]
    fn device_attributes_primary_da() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);
        s.handler.effects.device_attributes = Some(da_default);

        {
            s.next_slice(b"\x1B[c");
        }
        assert_eq!(cap_take().unwrap().as_slice(), b"\x1b[?62;22c");

        t.deinit(Global);
    }

    // test "device attributes: secondary DA"
    #[test]
    fn device_attributes_secondary_da() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);
        s.handler.effects.device_attributes = Some(da_default);

        {
            s.next_slice(b"\x1B[>c");
        }
        assert_eq!(cap_take().unwrap().as_slice(), b"\x1b[>1;0;0c");

        t.deinit(Global);
    }

    // test "device attributes: tertiary DA"
    #[test]
    fn device_attributes_tertiary_da() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);
        s.handler.effects.device_attributes = Some(da_default);

        {
            s.next_slice(b"\x1B[=c");
        }
        assert_eq!(cap_take().unwrap().as_slice(), b"\x1bP!|00000000\x1b\\");

        t.deinit(Global);
    }

    // test "device attributes: readonly ignores"
    #[test]
    fn device_attributes_readonly_ignores() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        {
            let mut s = t.vt_stream();
            // All DA queries should be silently ignored without effects
            s.next_slice(b"\x1B[c");
            s.next_slice(b"\x1B[>c");
            s.next_slice(b"\x1B[=c");

            // Terminal should still be functional
            s.next_slice(b"Test");
        }
        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), b"Test");

        t.deinit(Global);
    }

    // fn da(_: *Handler) device_attributes.Attributes {
    //     return .{
    //         .primary = .{
    //             .conformance_level = .vt420,
    //             .features = &.{ .ansi_color, .clipboard },
    //         },
    //         .secondary = .{
    //             .device_type = .vt420,
    //             .firmware_version = 100,
    //         },
    //     };
    // }
    fn da_custom(_: &mut Handler) -> device_attributes::Attributes {
        device_attributes::Attributes {
            primary: device_attributes::Primary {
                conformance_level: device_attributes::ConformanceLevel::VT420,
                features: &[
                    device_attributes::Feature::AnsiColor,
                    device_attributes::Feature::Clipboard,
                ],
            },
            secondary: device_attributes::Secondary {
                device_type: device_attributes::DeviceType::VT420,
                firmware_version: 100,
                ..device_attributes::Secondary::default()
            },
            ..device_attributes::Attributes::default()
        }
    }

    // test "device attributes: custom response"
    #[test]
    fn device_attributes_custom_response() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        cap_reset();
        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cap_write_pty);
        s.handler.effects.device_attributes = Some(da_custom);

        s.next_slice(b"\x1B[c");
        assert_eq!(cap_take().unwrap().as_slice(), b"\x1b[?64;22;52c");

        s.next_slice(b"\x1B[>c");
        assert_eq!(cap_take().unwrap().as_slice(), b"\x1b[>41;100;0c");

        drop(s);
        t.deinit(Global);
    }

    // test "kitty graphics APC response"
    #[test]
    fn kitty_graphics_apc_response() {
        // if (comptime !build_options.kitty_graphics) return error.SkipZigTest;
        if !crate::options::KITTY_GRAPHICS {
            return;
        }

        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        cap_reset();
        let mut s = t.vt_stream();
        // handler.effects.write_pty = &S.writePty;
        s.handler.effects.write_pty = Some(cap_write_pty);

        // Send a kitty graphics transmit command with image id 1
        s.next_slice(b"\x1b_Ga=t,t=d,f=24,i=1,s=1,v=2,c=10,r=1;////////\x1b\\");

        // Should have written a response back
        // try testing.expectEqualStrings("\x1b_Gi=1;OK\x1b\\", S.written.?);
        drop(s);
        assert_eq!(cap_take().unwrap().as_slice(), b"\x1b_Gi=1;OK\x1b\\");

        t.deinit(Global);
    }

    // test "kitty graphics via APC"
    #[test]
    fn kitty_graphics_via_apc() {
        // if (comptime !build_options.kitty_graphics) return error.SkipZigTest;
        if !crate::options::KITTY_GRAPHICS {
            return;
        }

        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        {
            let mut s = t.vt_stream();
            // Send a kitty graphics transmit command via APC:
            // ESC _ G <payload> ESC \
            // a=t,t=d,f=24,i=1,s=1,v=2,c=10,r=1;//////// (1x2 RGB direct)
            s.next_slice(b"\x1b_Ga=t,t=d,f=24,i=1,s=1,v=2,c=10,r=1;////////\x1b\\");
        }

        // const storage = &t.screens.active.kitty_images;
        // const img = storage.imageById(1).?;
        // try testing.expectEqual(.rgb, img.format);
        let storage = unsafe { &(*t.screens.active).kitty_images };
        let img = storage.image_by_id(1).unwrap();
        assert_eq!(kitty::graphics_command::Format::Rgb, img.format);

        t.deinit(Global);
    }

    // test "continuation reconstructs standard stream without duplicate effects"
    #[test]
    fn continuation_reconstructs_standard_stream_without_duplicate_effects() {
        use crate::clipboard;
        use crate::stream_continuation::FixedIoWriter;

        // `testing.allocator` for the `Options.allocator` gate (the same
        // process-global zig allocator `Terminal::vt_stream` uses).
        static CONT_ZIG_ALLOC: rustty_lib::allocator::GlobalZigAllocator =
            rustty_lib::allocator::GlobalZigAllocator;
        fn cont_alloc() -> rustty_lib::allocator::ZigAllocatorRef<'static> {
            rustty_lib::allocator::ZigAllocatorRef::new(&CONT_ZIG_ALLOC)
        }

        // const S = struct { var bell_count ... fn reset() ... };
        // PORT NOTE: the reference hangs `var`s on a local `S` struct; the
        // callbacks are bare `fn` pointers here, so the counters live in
        // thread-locals (the module's established idiom).
        thread_local! {
            static BELL_COUNT: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
            static TITLE_COUNT: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
            static WRITE_COUNT: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
            static NOTIFICATION_COUNT: core::cell::Cell<usize> =
                const { core::cell::Cell::new(0) };
            static CLIPBOARD_COUNT: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
        }

        fn bell(_: &mut Handler) {
            BELL_COUNT.with(|c| c.set(c.get() + 1));
        }
        fn title_changed(_: &mut Handler) {
            TITLE_COUNT.with(|c| c.set(c.get() + 1));
        }
        fn write_pty(_: &mut Handler, _: &[u8]) {
            WRITE_COUNT.with(|c| c.set(c.get() + 1));
        }
        fn desktop_notification(_: &mut Handler, _: stream::ShowDesktopNotification) {
            NOTIFICATION_COUNT.with(|c| c.set(c.get() + 1));
        }
        fn clipboard_write(_: &mut Handler, write: clipboard::Write) {
            CLIPBOARD_COUNT.with(|c| c.set(c.get() + 1));
            // write.reply(.{ .success = .{} });
            write.reply(clipboard::WriteResult::Success(
                clipboard::WriteResultSuccess::default(),
            ));
        }
        fn reset() {
            BELL_COUNT.with(|c| c.set(0));
            TITLE_COUNT.with(|c| c.set(0));
            WRITE_COUNT.with(|c| c.set(0));
            NOTIFICATION_COUNT.with(|c| c.set(0));
            CLIPBOARD_COUNT.with(|c| c.set(0));
        }
        reset();

        // const committed = "A\n\x07" ++ "\x1b]2;title\x1b\\" ++ "\x1b[5n" ++
        //     "\x1b]9;body\x1b\\" ++ "\x1b]52;c;aA==\x1b\\";
        const COMMITTED: &[u8] =
            b"A\n\x07\x1b]2;title\x1b\\\x1b[5n\x1b]9;body\x1b\\\x1b]52;c;aA==\x1b\\";

        let mut source_terminal = Terminal::init(Global, opts(80, 24)).unwrap();
        // PORT NOTE (SB discipline): the handler aliases the terminal through a
        // raw pointer while the test also reads terminal state directly. Both
        // go through ONE `*mut Terminal` per terminal so no fresh `&mut`
        // invalidates the stream's stored pointer mid-test (the module's
        // raw-pointer seam; memory rust-mut-args-strongly-protected).
        let source_ptr: *mut Terminal = &mut source_terminal;

        // var source_handler: Handler = .init(&source_terminal);
        let mut source_handler = Handler::init(source_ptr);
        source_handler.effects.bell = Some(bell);
        source_handler.effects.title_changed = Some(title_changed);
        source_handler.effects.write_pty = Some(write_pty);
        source_handler.effects.desktop_notification = Some(desktop_notification);
        source_handler.effects.clipboard_write = Some(clipboard_write);
        // var source = Stream.init(.{
        //     .allocator = testing.allocator,
        //     .handler = source_handler,
        //     .continuation_max_bytes = 1024,
        // });
        // defer source.deinit();
        let mut source = Stream::init(stream::Options {
            allocator: Some(cont_alloc()),
            handler: source_handler,
            continuation_max_bytes: Some(1024),
        });

        // Terminal mutation and all callbacks have already committed. The
        // unfinished CSI is the only input needed to recreate the stream state.
        let mut committed_and_pending = COMMITTED.to_vec();
        committed_and_pending.extend_from_slice(b"\x1b[31");
        source.next_slice(&committed_and_pending);
        assert_eq!(1, BELL_COUNT.with(|c| c.get()));
        assert_eq!(1, TITLE_COUNT.with(|c| c.get()));
        assert_eq!(1, WRITE_COUNT.with(|c| c.get()));
        assert_eq!(1, NOTIFICATION_COUNT.with(|c| c.get()));
        assert_eq!(1, CLIPBOARD_COUNT.with(|c| c.get()));

        let mut continuation_buf = [0u8; 1024];
        let mut continuation_writer = FixedIoWriter::new(&mut continuation_buf);
        source.write_continuation(&mut continuation_writer).unwrap();
        assert_eq!(b"\x1b[31".as_slice(), continuation_writer.buffered());

        let mut restored_terminal = Terminal::init(Global, opts(80, 24)).unwrap();
        let restored_ptr: *mut Terminal = &mut restored_terminal;

        // Stand in for restoring the already-committed terminal snapshot.
        {
            // var snapshot_stream: Stream = .init(.{
            //     .allocator = testing.allocator,
            //     .handler = .init(&restored_terminal),
            // });
            // defer snapshot_stream.deinit();
            let mut snapshot_stream = Stream::init(stream::Options {
                allocator: Some(cont_alloc()),
                handler: Handler::init(restored_ptr),
                continuation_max_bytes: None,
            });
            snapshot_stream.next_slice(COMMITTED);
        }

        // const before = try restored_terminal.plainString(testing.allocator);
        let before = unsafe { (*restored_ptr).plain_string(Global) }.unwrap();
        let before_x = unsafe { (*(*restored_ptr).screens.active).cursor.x };
        let before_y = unsafe { (*(*restored_ptr).screens.active).cursor.y };
        let before_style = unsafe { (*(*restored_ptr).screens.active).cursor.style_id };
        let before_title = unsafe { (*restored_ptr).get_title() }.unwrap().to_vec();

        let mut restored_handler = Handler::init(restored_ptr);
        restored_handler.effects.bell = Some(bell);
        restored_handler.effects.title_changed = Some(title_changed);
        restored_handler.effects.write_pty = Some(write_pty);
        restored_handler.effects.desktop_notification = Some(desktop_notification);
        restored_handler.effects.clipboard_write = Some(clipboard_write);
        // var restored = Stream.init(.{ ... .continuation_max_bytes = 1024 });
        // defer restored.deinit();
        let mut restored = Stream::init(stream::Options {
            allocator: Some(cont_alloc()),
            handler: restored_handler,
            continuation_max_bytes: Some(1024),
        });

        // S.reset();
        reset();
        restored.next_slice(continuation_writer.buffered());
        assert_eq!(0, BELL_COUNT.with(|c| c.get()));
        assert_eq!(0, TITLE_COUNT.with(|c| c.get()));
        assert_eq!(0, WRITE_COUNT.with(|c| c.get()));
        assert_eq!(0, NOTIFICATION_COUNT.with(|c| c.get()));
        assert_eq!(0, CLIPBOARD_COUNT.with(|c| c.get()));
        let after = unsafe { (*restored_ptr).plain_string(Global) }.unwrap();
        assert_eq!(before, after);
        assert_eq!(before_x, unsafe {
            (*(*restored_ptr).screens.active).cursor.x
        });
        assert_eq!(before_y, unsafe {
            (*(*restored_ptr).screens.active).cursor.y
        });
        assert_eq!(before_style, unsafe {
            (*(*restored_ptr).screens.active).cursor.style_id
        });
        assert_eq!(
            before_title.as_slice(),
            unsafe { (*restored_ptr).get_title() }.unwrap()
        );

        source.next_slice(b"mB");
        restored.next_slice(b"mB");
        let source_text = unsafe { (*source_ptr).plain_string(Global) }.unwrap();
        let restored_text = unsafe { (*restored_ptr).plain_string(Global) }.unwrap();
        assert_eq!(source_text, restored_text);
        assert_eq!(
            unsafe { (*(*source_ptr).screens.active).cursor.style_id },
            unsafe { (*(*restored_ptr).screens.active).cursor.style_id },
        );

        source.deinit();
        restored.deinit();
        restored_terminal.deinit(Global);
        source_terminal.deinit(Global);
    }

    // ==== Phase-5 dispatch tier: Kitty clipboard (OSC 5522), Kitty dnd
    // ==== (OSC 72), paste, clipboard_read, and mode-2048 tests.

    /// Shared capture state for the Kitty clipboard (OSC 5522) tests below:
    /// records every pty response and the most recent clipboard write.
    // const KittyClipboardCapture = struct { ... };
    // PORT NOTE: the reference hangs `var`s on a file-scope struct; the
    // callbacks are bare `fn` pointers here, so the capture lives in a
    // thread-local cell (the module's established idiom). The reference's
    // fixed `[8][64]u8`-style capture buffers become owned Vecs.
    #[derive(Default)]
    struct KittyCap {
        responses: Vec<u8>,

        // Write capture. A None write_result returns without replying.
        write_count: usize,
        write_result: Option<clipboard::WriteResult>,
        write_reply_twice: bool,
        last_location: Option<clipboard::Location>,
        last_contents: Vec<(Vec<u8>, Vec<u8>)>,
        last_write_name: Vec<u8>,
        last_write_granted: bool,
        last_write_can_remember: bool,

        // Read capture. A None read_result returns without replying.
        read_count: usize,
        read_result: Option<KcReadResult>,
        read_reply_twice: bool,
        last_read_location: Option<clipboard::Location>,
        last_read_mimes: Vec<Vec<u8>>,
        last_read_list: bool,
        last_read_name: Vec<u8>,
        last_read_granted: bool,
        last_read_can_remember: bool,
    }

    /// Owned analog of `clipboard.Read.Result` for the thread-local capture
    /// (the real result's memory is only valid during the reply call).
    #[derive(Clone)]
    enum KcReadResult {
        Denied,
        Unsupported,
        Busy,
        IoError,
        Success {
            contents: Vec<(Vec<u8>, Vec<u8>)>,
            available: Vec<Vec<u8>>,
            remember: bool,
        },
    }

    impl KcReadResult {
        /// Build the borrowed reply and deliver it.
        fn reply_with(&self, read: &clipboard::Read) {
            match self {
                KcReadResult::Denied => read.reply(clipboard::ReadResult::Denied),
                KcReadResult::Unsupported => read.reply(clipboard::ReadResult::Unsupported),
                KcReadResult::Busy => read.reply(clipboard::ReadResult::Busy),
                KcReadResult::IoError => read.reply(clipboard::ReadResult::IoError),
                KcReadResult::Success {
                    contents,
                    available,
                    remember,
                } => {
                    let contents_b: Vec<clipboard::Content> = contents
                        .iter()
                        .map(|(mime, data)| clipboard::Content { mime, data })
                        .collect();
                    let available_b: Vec<&[u8]> = available.iter().map(|a| a.as_slice()).collect();
                    read.reply(clipboard::ReadResult::Success(
                        clipboard::ReadResultSuccess {
                            contents: &contents_b,
                            available: &available_b,
                            remember: *remember,
                        },
                    ));
                }
            }
        }
    }

    thread_local! {
        static KC: core::cell::RefCell<KittyCap> =
            core::cell::RefCell::new(KittyCap::default());
    }

    // fn reset() void { ... }
    fn kc_reset() {
        KC.with(|c| {
            *c.borrow_mut() = KittyCap {
                write_result: Some(clipboard::WriteResult::Success(
                    clipboard::WriteResultSuccess::default(),
                )),
                ..KittyCap::default()
            };
        });
    }

    // fn writePty(_: *Handler, data: []const u8) void { ... }
    fn kc_write_pty(_: &mut Handler, data: &[u8]) {
        KC.with(|c| c.borrow_mut().responses.extend_from_slice(data));
    }

    // fn clipboardWrite(_: *Handler, write: clipboard.Write) void { ... }
    fn kc_clipboard_write(_: &mut Handler, write: clipboard::Write) {
        let (result, twice) = KC.with(|c| {
            let mut cap = c.borrow_mut();
            cap.write_count += 1;
            cap.last_location = Some(write.location);
            cap.last_contents = write
                .contents
                .iter()
                .map(|content| (content.mime.to_vec(), content.data.to_vec()))
                .collect();
            cap.last_write_name = write.name.to_vec();
            cap.last_write_granted = write.granted;
            cap.last_write_can_remember = write.can_remember;
            (cap.write_result, cap.write_reply_twice)
        });
        if let Some(r) = result {
            write.reply(r);
        }
        if twice {
            write.reply(clipboard::WriteResult::IoError);
        }
    }

    // fn clipboardRead(_: *Handler, read: clipboard.Read) void { ... }
    fn kc_clipboard_read(_: &mut Handler, read: clipboard::Read) {
        let (result, twice) = KC.with(|c| {
            let mut cap = c.borrow_mut();
            cap.read_count += 1;
            cap.last_read_location = Some(read.location);
            cap.last_read_mimes = read.mimes.iter().map(|m| m.to_vec()).collect();
            cap.last_read_list = read.list;
            cap.last_read_name = read.name.to_vec();
            cap.last_read_granted = read.granted;
            cap.last_read_can_remember = read.can_remember;
            (cap.read_result.clone(), cap.read_reply_twice)
        });
        if let Some(r) = &result {
            r.reply_with(&read);
        }
        if twice {
            read.reply(clipboard::ReadResult::Denied);
        }
    }

    fn kc_responses() -> Vec<u8> {
        KC.with(|c| c.borrow().responses.clone())
    }

    fn kc_responses_len() -> usize {
        KC.with(|c| c.borrow().responses.len())
    }

    fn kc<T>(f: impl FnOnce(&KittyCap) -> T) -> T {
        KC.with(|c| f(&c.borrow()))
    }

    fn kc_mut(f: impl FnOnce(&mut KittyCap)) {
        KC.with(|c| f(&mut c.borrow_mut()));
    }

    // test "kitty clipboard write transaction round trip"
    #[test]
    fn kitty_clipboard_write_transaction_round_trip() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        kc_reset();

        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(kc_write_pty);
        s.handler.effects.clipboard_write = Some(kc_clipboard_write);

        // Begin a write, stream two MIME types (one chunked), alias the
        // plain text, and commit. Only the commit produces a response.
        s.next_slice(b"\x1B]5522;type=write:id=42\x1B\\");
        s.next_slice(b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;R2hvc3Q=\x1B\\"); // "Ghost"
        s.next_slice(b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;dHk=\x1B\\"); // "ty"
        s.next_slice(b"\x1B]5522;type=wdata:mime=dGV4dC9odG1s;PGI+aGk8L2I+\x1B\\"); // "<b>hi</b>"
                                                                                    // Alias "TEXT UTF8_STRING" -> text/plain.
        s.next_slice(b"\x1B]5522;type=walias:mime=dGV4dC9wbGFpbg==;VEVYVCBVVEY4X1NUUklORw==\x1B\\");
        assert_eq!(0, kc(|c| c.write_count));
        assert_eq!(0, kc_responses_len());

        s.next_slice(b"\x1B]5522;type=wdata\x1B\\");
        assert_eq!(1, kc(|c| c.write_count));
        assert_eq!(Some(clipboard::Location::Standard), kc(|c| c.last_location));
        assert_eq!(4, kc(|c| c.last_contents.len()));
        // PORT NOTE (R2): the aliased text payload decodes to "Ghostty" in the
        // reference; the bytes are protocol data, carried verbatim.
        kc(|c| {
            assert_eq!(b"text/plain".as_slice(), c.last_contents[0].0.as_slice());
            assert_eq!(b"Ghostty".as_slice(), c.last_contents[0].1.as_slice());
            assert_eq!(b"text/html".as_slice(), c.last_contents[1].0.as_slice());
            assert_eq!(b"<b>hi</b>".as_slice(), c.last_contents[1].1.as_slice());
            assert_eq!(b"TEXT".as_slice(), c.last_contents[2].0.as_slice());
            assert_eq!(b"Ghostty".as_slice(), c.last_contents[2].1.as_slice());
            assert_eq!(b"UTF8_STRING".as_slice(), c.last_contents[3].0.as_slice());
            assert_eq!(b"Ghostty".as_slice(), c.last_contents[3].1.as_slice());
        });
        assert_eq!(
            b"\x1B]5522;type=write:status=DONE:id=42\x1B\\".as_slice(),
            kc_responses().as_slice(),
        );

        // A commit with no transaction in flight is silently ignored.
        s.next_slice(b"\x1B]5522;type=wdata\x1B\\");
        assert_eq!(1, kc(|c| c.write_count));

        s.deinit();
        t.deinit(Global);
    }

    // test "kitty clipboard write result maps to response status"
    #[test]
    fn kitty_clipboard_write_result_maps_to_response_status() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(kc_write_pty);
        s.handler.effects.clipboard_write = Some(kc_clipboard_write);

        let cases: &[(Option<clipboard::WriteResult>, &[u8])] = &[
            (
                Some(clipboard::WriteResult::Success(
                    clipboard::WriteResultSuccess::default(),
                )),
                b"\x1B]5522;type=write:status=DONE\x1B\\",
            ),
            (
                Some(clipboard::WriteResult::Denied),
                b"\x1B]5522;type=write:status=EPERM\x1B\\",
            ),
            (
                Some(clipboard::WriteResult::Unsupported),
                b"\x1B]5522;type=write:status=ENOSYS\x1B\\",
            ),
            (
                Some(clipboard::WriteResult::Busy),
                b"\x1B]5522;type=write:status=EBUSY\x1B\\",
            ),
            (
                Some(clipboard::WriteResult::InvalidData),
                b"\x1B]5522;type=write:status=EINVAL\x1B\\",
            ),
            (
                Some(clipboard::WriteResult::IoError),
                b"\x1B]5522;type=write:status=EIO\x1B\\",
            ),
            // No reply at all is a denial rather than silence.
            (None, b"\x1B]5522;type=write:status=EPERM\x1B\\"),
        ];

        for (result, response) in cases {
            kc_reset();
            kc_mut(|c| c.write_result = *result);

            // An immediately-committed write with no data is a clear.
            s.next_slice(b"\x1B]5522;type=write\x1B\\");
            s.next_slice(b"\x1B]5522;type=wdata\x1B\\");
            assert_eq!(1, kc(|c| c.write_count));
            assert_eq!(0, kc(|c| c.last_contents.len()));
            assert_eq!(*response, kc_responses().as_slice());
        }

        // A second reply is ignored.
        kc_reset();
        kc_mut(|c| c.write_reply_twice = true);
        s.next_slice(b"\x1B]5522;type=write\x1B\\");
        s.next_slice(b"\x1B]5522;type=wdata\x1B\\");
        assert_eq!(
            b"\x1B]5522;type=write:status=DONE\x1B\\".as_slice(),
            kc_responses().as_slice(),
        );

        // The response echoes the request terminator, unlike kitty which
        // always uses ST.
        kc_reset();
        s.next_slice(b"\x1B]5522;type=write:loc=primary\x07");
        s.next_slice(b"\x1B]5522;type=wdata\x07");
        assert_eq!(Some(clipboard::Location::Primary), kc(|c| c.last_location));
        assert_eq!(
            b"\x1B]5522;type=write:status=DONE\x07".as_slice(),
            kc_responses().as_slice(),
        );

        // A system without a primary selection answers a loc=primary write
        // with ENOSYS, echoing the id.
        kc_reset();
        kc_mut(|c| c.write_result = Some(clipboard::WriteResult::Unsupported));
        s.next_slice(b"\x1B]5522;type=write:loc=primary:id=p1\x1B\\");
        s.next_slice(b"\x1B]5522;type=wdata\x1B\\");
        assert_eq!(Some(clipboard::Location::Primary), kc(|c| c.last_location));
        assert_eq!(
            b"\x1B]5522;type=write:status=ENOSYS:id=p1\x1B\\".as_slice(),
            kc_responses().as_slice(),
        );

        s.deinit();
        t.deinit(Global);
    }

    // test "kitty clipboard write without clipboard effect responds ENOSYS"
    #[test]
    fn kitty_clipboard_write_without_clipboard_effect_responds_enosys() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        kc_reset();

        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(kc_write_pty);

        // The transaction fails as soon as it begins; the rest of it is
        // ignored without further responses.
        s.next_slice(b"\x1B]5522;type=write:id=x\x1B\\");
        assert_eq!(
            b"\x1B]5522;type=write:status=ENOSYS:id=x\x1B\\".as_slice(),
            kc_responses().as_slice(),
        );
        s.next_slice(b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;R2hvc3Q=\x1B\\");
        s.next_slice(b"\x1B]5522;type=wdata\x1B\\");
        assert_eq!(
            b"\x1B]5522;type=write:status=ENOSYS:id=x\x1B\\".as_slice(),
            kc_responses().as_slice(),
        );

        s.deinit();
        t.deinit(Global);
    }

    // test "kitty clipboard read without effect is denied with EPERM"
    #[test]
    fn kitty_clipboard_read_without_effect_is_denied_with_eperm() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        kc_reset();

        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(kc_write_pty);
        s.handler.effects.clipboard_write = Some(kc_clipboard_write);

        // The denial never includes loc (only OK responses do) and echoes
        // the sanitized id.
        s.next_slice(b"\x1B]5522;type=read:loc=primary:id=*4 2*;dGV4dC9wbGFpbg==\x1B\\");
        assert_eq!(
            b"\x1B]5522;type=read:status=EPERM:id=42\x1B\\".as_slice(),
            kc_responses().as_slice(),
        );

        // A missing payload is an empty MIME list, still answered.
        kc_reset();
        s.next_slice(b"\x1B]5522;type=read\x07");
        assert_eq!(
            b"\x1B]5522;type=read:status=EPERM\x07".as_slice(),
            kc_responses().as_slice(),
        );

        // An undecodable payload is dropped without a response.
        kc_reset();
        s.next_slice(b"\x1B]5522;type=read;!!!\x1B\\");
        assert_eq!(0, kc_responses_len());

        s.deinit();
        t.deinit(Global);
    }

    // test "kitty clipboard read round trip"
    #[test]
    fn kitty_clipboard_read_round_trip() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        kc_reset();

        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(kc_write_pty);
        s.handler.effects.clipboard_read = Some(kc_clipboard_read);

        kc_mut(|c| {
            c.read_result = Some(KcReadResult::Success {
                contents: vec![
                    // Unrequested representations are never served, and the
                    // served ones follow request order, not reply order.
                    (b"image/png".to_vec(), b"\x89PNG".to_vec()),
                    (b"text/html".to_vec(), b"<b>hi</b>".to_vec()),
                    (b"text/plain".to_vec(), b"Ghostty".to_vec()),
                ],
                available: vec![b"text/plain".to_vec(), b"text/html".to_vec()],
                remember: false,
            });
        });

        // Request the targets listing plus two types from the primary
        // selection: ". text/plain text/html".
        s.next_slice(
            b"\x1B]5522;type=read:loc=primary:id=r1;LiB0ZXh0L3BsYWluIHRleHQvaHRtbA==\x1B\\",
        );
        assert_eq!(1, kc(|c| c.read_count));
        assert_eq!(
            Some(clipboard::Location::Primary),
            kc(|c| c.last_read_location)
        );
        assert_eq!(2, kc(|c| c.last_read_mimes.len()));
        kc(|c| {
            assert_eq!(b"text/plain".as_slice(), c.last_read_mimes[0].as_slice());
            assert_eq!(b"text/html".as_slice(), c.last_read_mimes[1].as_slice());
        });
        assert!(kc(|c| c.last_read_list));
        assert_eq!(0, kc(|c| c.last_read_name.len()));
        assert!(!kc(|c| c.last_read_granted));
        assert!(!kc(|c| c.last_read_can_remember));
        let mut expected: Vec<u8> = Vec::new();
        expected.extend_from_slice(b"\x1B]5522;type=read:status=OK:loc=primary:id=r1\x1B\\");
        expected.extend_from_slice(
            b"\x1B]5522;type=read:status=DATA:id=r1:mime=Lg==;dGV4dC9wbGFpbiB0ZXh0L2h0bWwK\x1B\\",
        );
        expected.extend_from_slice(
            b"\x1B]5522;type=read:status=DATA:id=r1:mime=dGV4dC9wbGFpbg==;R2hvc3R0eQ==\x1B\\",
        );
        expected.extend_from_slice(
            b"\x1B]5522;type=read:status=DATA:id=r1:mime=dGV4dC9odG1s;PGI+aGk8L2I+\x1B\\",
        );
        expected.extend_from_slice(b"\x1B]5522;type=read:status=DONE:id=r1\x1B\\");
        assert_eq!(expected, kc_responses());

        // Without the listing request `available` is ignored. The response
        // echoes the request terminator.
        kc_mut(|c| c.responses.clear());
        s.next_slice(b"\x1B]5522;type=read:id=r2;dGV4dC9wbGFpbg==\x07");
        assert_eq!(
            Some(clipboard::Location::Standard),
            kc(|c| c.last_read_location)
        );
        assert_eq!(1, kc(|c| c.last_read_mimes.len()));
        assert!(!kc(|c| c.last_read_list));
        let mut expected: Vec<u8> = Vec::new();
        expected.extend_from_slice(b"\x1B]5522;type=read:status=OK:id=r2\x07");
        expected.extend_from_slice(
            b"\x1B]5522;type=read:status=DATA:id=r2:mime=dGV4dC9wbGFpbg==;R2hvc3R0eQ==\x07",
        );
        expected.extend_from_slice(b"\x1B]5522;type=read:status=DONE:id=r2\x07");
        assert_eq!(expected, kc_responses());

        // A listing-only request carries no types.
        kc_mut(|c| c.responses.clear());
        s.next_slice(b"\x1B]5522;type=read;Lg==\x1B\\");
        assert_eq!(0, kc(|c| c.last_read_mimes.len()));
        assert!(kc(|c| c.last_read_list));
        let mut expected: Vec<u8> = Vec::new();
        expected.extend_from_slice(b"\x1B]5522;type=read:status=OK\x1B\\");
        expected.extend_from_slice(
            b"\x1B]5522;type=read:status=DATA:mime=Lg==;dGV4dC9wbGFpbiB0ZXh0L2h0bWwK\x1B\\",
        );
        expected.extend_from_slice(b"\x1B]5522;type=read:status=DONE\x1B\\");
        assert_eq!(expected, kc_responses());

        s.deinit();
        t.deinit(Global);
    }

    // test "kitty clipboard read result maps to response status"
    #[test]
    fn kitty_clipboard_read_result_maps_to_response_status() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(kc_write_pty);
        s.handler.effects.clipboard_read = Some(kc_clipboard_read);

        let mut ok_done: Vec<u8> = Vec::new();
        ok_done.extend_from_slice(b"\x1B]5522;type=read:status=OK:id=x\x1B\\");
        ok_done.extend_from_slice(b"\x1B]5522;type=read:status=DONE:id=x\x1B\\");

        let cases: &[(Option<KcReadResult>, &[u8])] = &[
            (
                Some(KcReadResult::Denied),
                b"\x1B]5522;type=read:status=EPERM:id=x\x1B\\",
            ),
            (
                Some(KcReadResult::Unsupported),
                b"\x1B]5522;type=read:status=ENOSYS:id=x\x1B\\",
            ),
            (
                Some(KcReadResult::Busy),
                b"\x1B]5522;type=read:status=EBUSY:id=x\x1B\\",
            ),
            (
                Some(KcReadResult::IoError),
                b"\x1B]5522;type=read:status=EIO:id=x\x1B\\",
            ),
            // No reply at all is a denial rather than silence.
            (None, b"\x1B]5522;type=read:status=EPERM:id=x\x1B\\"),
            // A success with nothing to serve is still OK then DONE.
            (
                Some(KcReadResult::Success {
                    contents: Vec::new(),
                    available: Vec::new(),
                    remember: false,
                }),
                ok_done.as_slice(),
            ),
        ];

        for (result, response) in cases {
            kc_reset();
            kc_mut(|c| c.read_result = result.clone());
            s.next_slice(b"\x1B]5522;type=read:id=x;dGV4dC9wbGFpbg==\x1B\\");
            assert_eq!(1, kc(|c| c.read_count));
            assert_eq!(*response, kc_responses().as_slice());
        }

        // A second reply is ignored.
        kc_reset();
        kc_mut(|c| {
            c.read_result = Some(KcReadResult::Success {
                contents: vec![(b"text/plain".to_vec(), b"hello".to_vec())],
                available: Vec::new(),
                remember: false,
            });
            c.read_reply_twice = true;
        });
        s.next_slice(b"\x1B]5522;type=read;dGV4dC9wbGFpbg==\x1B\\");
        let mut expected: Vec<u8> = Vec::new();
        expected.extend_from_slice(b"\x1B]5522;type=read:status=OK\x1B\\");
        expected.extend_from_slice(
            b"\x1B]5522;type=read:status=DATA:mime=dGV4dC9wbGFpbg==;aGVsbG8=\x1B\\",
        );
        expected.extend_from_slice(b"\x1B]5522;type=read:status=DONE\x1B\\");
        assert_eq!(expected, kc_responses());

        s.deinit();
        t.deinit(Global);
    }

    // test "kitty clipboard read caps requested types"
    #[test]
    fn kitty_clipboard_read_caps_requested_types() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        kc_reset();

        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(kc_write_pty);
        s.handler.effects.clipboard_read = Some(kc_clipboard_read);

        // "a/0 a/1 a/2 a/3 a/4 a/5 .": extras are dropped but the listing
        // request after them still counts.
        kc_mut(|c| {
            c.read_result = Some(KcReadResult::Success {
                contents: Vec::new(),
                available: Vec::new(),
                remember: false,
            });
        });
        s.next_slice(b"\x1B]5522;type=read;YS8wIGEvMSBhLzIgYS8zIGEvNCBhLzUgLg==\x1B\\");
        assert_eq!(1, kc(|c| c.read_count));
        assert_eq!(
            kitty::clipboard::max_read_mimes,
            kc(|c| c.last_read_mimes.len())
        );
        kc(|c| {
            assert_eq!(b"a/0".as_slice(), c.last_read_mimes[0].as_slice());
            assert_eq!(
                b"a/3".as_slice(),
                c.last_read_mimes[kitty::clipboard::max_read_mimes - 1].as_slice()
            );
        });
        assert!(kc(|c| c.last_read_list));

        s.deinit();
        t.deinit(Global);
    }

    // test "kitty clipboard read password grants"
    #[test]
    fn kitty_clipboard_read_password_grants() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        kc_reset();

        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(kc_write_pty);
        s.handler.effects.clipboard_read = Some(kc_clipboard_read);

        let success_remember = KcReadResult::Success {
            contents: Vec::new(),
            available: Vec::new(),
            remember: true,
        };
        let success_plain = KcReadResult::Success {
            contents: Vec::new(),
            available: Vec::new(),
            remember: false,
        };

        // Every read requests a data type ("text/plain"): a request with
        // no data types never consults the grants at all.
        //
        // pw="secret", name="app": the first request isn't granted but the
        // reply may ask to remember it.
        kc_mut(|c| c.read_result = Some(success_remember.clone()));
        s.next_slice(b"\x1B]5522;type=read:pw=c2VjcmV0:name=YXBw;dGV4dC9wbGFpbg==\x1B\\");
        assert_eq!(1, kc(|c| c.read_count));
        kc(|c| assert_eq!(b"app".as_slice(), c.last_read_name.as_slice()));
        assert!(!kc(|c| c.last_read_granted));
        assert!(kc(|c| c.last_read_can_remember));

        // The same password is now granted; a different one is not.
        kc_mut(|c| c.read_result = Some(success_plain.clone()));
        s.next_slice(b"\x1B]5522;type=read:pw=c2VjcmV0:name=YXBw;dGV4dC9wbGFpbg==\x1B\\");
        assert!(kc(|c| c.last_read_granted));
        s.next_slice(b"\x1B]5522;type=read:pw=b3RoZXI=:name=YXBw;dGV4dC9wbGFpbg==\x1B\\");
        assert!(!kc(|c| c.last_read_granted));
        assert!(kc(|c| c.last_read_can_remember));

        // A password without a name doesn't count: it is neither granted
        // nor rememberable, even if the reply asks.
        kc_mut(|c| c.read_result = Some(success_remember.clone()));
        s.next_slice(b"\x1B]5522;type=read:pw=c2VjcmV0;dGV4dC9wbGFpbg==\x1B\\");
        assert_eq!(0, kc(|c| c.last_read_name.len()));
        assert!(!kc(|c| c.last_read_granted));
        assert!(!kc(|c| c.last_read_can_remember));
        s.next_slice(b"\x1B]5522;type=read:pw=b3RoZXI=;dGV4dC9wbGFpbg==\x1B\\");
        assert!(!kc(|c| c.last_read_can_remember));
        kc_mut(|c| c.read_result = Some(success_plain.clone()));
        s.next_slice(b"\x1B]5522;type=read:pw=b3RoZXI=:name=YXBw;dGV4dC9wbGFpbg==\x1B\\");
        assert!(!kc(|c| c.last_read_granted));

        // A grant is advisory: the request is still forwarded and the
        // embedder may deny it.
        kc_mut(|c| {
            c.responses.clear();
            c.read_result = Some(KcReadResult::Denied);
        });
        s.next_slice(b"\x1B]5522;type=read:id=d:pw=c2VjcmV0:name=YXBw;dGV4dC9wbGFpbg==\x1B\\");
        assert!(kc(|c| c.last_read_granted));
        assert_eq!(
            b"\x1B]5522;type=read:status=EPERM:id=d\x1B\\".as_slice(),
            kc_responses().as_slice(),
        );

        // Grants are freed with the stream (the testing allocator catches
        // the leak otherwise).
        s.deinit();
        t.deinit(Global);
    }

    // test "kitty clipboard read targets-only never consumes a one-time grant"
    #[test]
    fn kitty_clipboard_read_targets_only_never_consumes_a_one_time_grant() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        kc_reset();

        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(kc_write_pty);
        s.handler.effects.clipboard_read = Some(kc_clipboard_read);

        // A one-time read grant, as minted for a paste event.
        s.handler
            .kitty_clipboard_grants
            .grant(b"otp", kitty::clipboard::Direction::Read, true);

        // A targets-only read (payload ".") is prompt-exempt so it never
        // consults, and must not burn, the one-time password.
        kc_mut(|c| {
            c.read_result = Some(KcReadResult::Success {
                contents: Vec::new(),
                available: Vec::new(),
                remember: false,
            });
        });
        s.next_slice(b"\x1B]5522;type=read:pw=b3Rw:name=YXBw;Lg==\x1B\\");
        assert_eq!(1, kc(|c| c.read_count));
        assert!(kc(|c| c.last_read_list));
        assert_eq!(0, kc(|c| c.last_read_mimes.len()));
        assert!(!kc(|c| c.last_read_granted));

        // The follow-up data read still consumes the grant, exactly once.
        s.next_slice(b"\x1B]5522;type=read:pw=b3Rw:name=YXBw;dGV4dC9wbGFpbg==\x1B\\");
        assert!(kc(|c| c.last_read_granted));
        s.next_slice(b"\x1B]5522;type=read:pw=b3Rw:name=YXBw;dGV4dC9wbGFpbg==\x1B\\");
        assert!(!kc(|c| c.last_read_granted));

        s.deinit();
        t.deinit(Global);
    }

    // test "kitty clipboard write password grants"
    #[test]
    fn kitty_clipboard_write_password_grants() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        kc_reset();

        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(kc_write_pty);
        s.handler.effects.clipboard_write = Some(kc_clipboard_write);
        s.handler.effects.clipboard_read = Some(kc_clipboard_read);

        // pw="secret", name="app": the first commit isn't granted but the
        // reply may ask to remember it.
        kc_mut(|c| {
            c.write_result = Some(clipboard::WriteResult::Success(
                clipboard::WriteResultSuccess { remember: true },
            ));
        });
        s.next_slice(b"\x1B]5522;type=write:pw=c2VjcmV0:name=YXBw\x1B\\");
        s.next_slice(b"\x1B]5522;type=wdata\x1B\\");
        assert_eq!(1, kc(|c| c.write_count));
        kc(|c| assert_eq!(b"app".as_slice(), c.last_write_name.as_slice()));
        assert!(!kc(|c| c.last_write_granted));
        assert!(kc(|c| c.last_write_can_remember));

        // The same password is now granted; a different one is not.
        kc_mut(|c| {
            c.write_result = Some(clipboard::WriteResult::Success(
                clipboard::WriteResultSuccess::default(),
            ));
        });
        s.next_slice(b"\x1B]5522;type=write:pw=c2VjcmV0:name=YXBw\x1B\\");
        s.next_slice(b"\x1B]5522;type=wdata\x1B\\");
        assert!(kc(|c| c.last_write_granted));
        s.next_slice(b"\x1B]5522;type=write:pw=b3RoZXI=:name=YXBw\x1B\\");
        s.next_slice(b"\x1B]5522;type=wdata\x1B\\");
        assert!(!kc(|c| c.last_write_granted));
        assert!(kc(|c| c.last_write_can_remember));

        // Directions are independent: a write grant doesn't satisfy reads.
        kc_mut(|c| {
            c.read_result = Some(KcReadResult::Success {
                contents: Vec::new(),
                available: Vec::new(),
                remember: false,
            });
        });
        s.next_slice(b"\x1B]5522;type=read:pw=c2VjcmV0:name=YXBw\x1B\\");
        assert!(!kc(|c| c.last_read_granted));

        // A password without a name doesn't count: it is neither granted
        // nor rememberable, even if the reply asks.
        kc_mut(|c| {
            c.write_result = Some(clipboard::WriteResult::Success(
                clipboard::WriteResultSuccess { remember: true },
            ));
        });
        s.next_slice(b"\x1B]5522;type=write:pw=c2VjcmV0\x1B\\");
        s.next_slice(b"\x1B]5522;type=wdata\x1B\\");
        assert_eq!(0, kc(|c| c.last_write_name.len()));
        assert!(!kc(|c| c.last_write_granted));
        assert!(!kc(|c| c.last_write_can_remember));

        // A grant is advisory: the request is still forwarded and the
        // embedder may deny it.
        kc_mut(|c| {
            c.responses.clear();
            c.write_result = Some(clipboard::WriteResult::Denied);
        });
        s.next_slice(b"\x1B]5522;type=write:id=d:pw=c2VjcmV0:name=YXBw\x1B\\");
        s.next_slice(b"\x1B]5522;type=wdata\x1B\\");
        assert!(kc(|c| c.last_write_granted));
        assert_eq!(
            b"\x1B]5522;type=write:status=EPERM:id=d\x1B\\".as_slice(),
            kc_responses().as_slice(),
        );

        // Grants are freed with the stream (the testing allocator catches
        // the leak otherwise).
        s.deinit();
        t.deinit(Global);
    }

    // test "kitty clipboard malformed packets are silently dropped"
    #[test]
    fn kitty_clipboard_malformed_packets_are_silently_dropped() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        kc_reset();

        {
            let mut s = t.vt_stream();
            s.handler.effects.write_pty = Some(kc_write_pty);
            s.handler.effects.clipboard_write = Some(kc_clipboard_write);

            // Missing type, unknown type, bare metadata record, invalid mime
            // base64, and orphaned transaction packets all drop silently.
            s.next_slice(b"\x1B]5522;loc=primary\x1B\\");
            s.next_slice(b"\x1B]5522;type=bobr\x1B\\");
            s.next_slice(b"\x1B]5522;type=read:bare\x1B\\");
            s.next_slice(b"\x1B]5522;type=wdata:mime=!!!;R2hvc3Q=\x1B\\");
            s.next_slice(b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;R2hvc3Q=\x1B\\");
            s.next_slice(b"\x1B]5522;type=walias:mime=dGV4dC9wbGFpbg==;VEVYVA==\x1B\\");
            assert_eq!(0, kc(|c| c.write_count));
            assert_eq!(0, kc_responses_len());
            assert!(!s.handler.semantic_failure);

            // The terminal is still functional afterwards.
            s.next_slice(b"ok");
            s.deinit();
        }
        let str = t.plain_string(Global).unwrap();
        assert_eq!(b"ok".as_slice(), str.as_slice());

        t.deinit(Global);
    }

    // test "kitty clipboard new write replaces in-flight transaction"
    #[test]
    fn kitty_clipboard_new_write_replaces_in_flight_transaction() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        kc_reset();

        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(kc_write_pty);
        s.handler.effects.clipboard_write = Some(kc_clipboard_write);

        s.next_slice(b"\x1B]5522;type=write:id=old\x1B\\");
        s.next_slice(b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;b2xk\x1B\\"); // "old"
        s.next_slice(b"\x1B]5522;type=write:id=new\x1B\\");
        s.next_slice(b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;bmV3\x1B\\"); // "new"
        s.next_slice(b"\x1B]5522;type=wdata\x1B\\");

        assert_eq!(1, kc(|c| c.write_count));
        assert_eq!(1, kc(|c| c.last_contents.len()));
        kc(|c| assert_eq!(b"new".as_slice(), c.last_contents[0].1.as_slice()));
        assert_eq!(
            b"\x1B]5522;type=write:status=DONE:id=new\x1B\\".as_slice(),
            kc_responses().as_slice(),
        );

        s.deinit();
        t.deinit(Global);
    }

    // test "kitty clipboard invalid write packets abort with EINVAL"
    #[test]
    fn kitty_clipboard_invalid_write_packets_abort_with_einval() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        kc_reset();

        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(kc_write_pty);
        s.handler.effects.clipboard_write = Some(kc_clipboard_write);

        let invalid_packets: &[&[u8]] = &[
            // Alias payload decodes to a non-UTF-8 byte.
            b"\x1B]5522;type=walias:mime=dGV4dC9wbGFpbg==;/w==\x1B\\",
            // Alias has no target MIME type.
            b"\x1B]5522;type=walias;VEVYVA==\x1B\\",
            // Alias target MIME decodes to non-UTF-8 bytes.
            b"\x1B]5522;type=walias:mime=//4=;VEVYVA==\x1B\\",
            // Write data MIME decodes to non-UTF-8 bytes.
            b"\x1B]5522;type=wdata:mime=//4=;R2hvc3Q=\x1B\\",
        ];

        for packet in invalid_packets {
            kc_mut(|c| c.responses.clear());
            s.next_slice(b"\x1B]5522;type=write:id=w\x1B\\");
            s.next_slice(b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;R2hvc3Q=\x1B\\");
            s.next_slice(packet);
            assert_eq!(
                b"\x1B]5522;type=write:status=EINVAL:id=w\x1B\\".as_slice(),
                kc_responses().as_slice(),
            );
            assert!(!s.handler.semantic_failure);

            // The transaction is gone: a commit does nothing further.
            s.next_slice(b"\x1B]5522;type=wdata\x1B\\");
            assert_eq!(0, kc(|c| c.write_count));
            assert_eq!(
                b"\x1B]5522;type=write:status=EINVAL:id=w\x1B\\".as_slice(),
                kc_responses().as_slice(),
            );
        }

        s.deinit();
        t.deinit(Global);
    }

    // test "kitty clipboard invalid read text does not abort a write"
    #[test]
    fn kitty_clipboard_invalid_read_text_does_not_abort_a_write() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        kc_reset();
        kc_mut(|c| {
            c.read_result = Some(KcReadResult::Success {
                contents: Vec::new(),
                available: Vec::new(),
                remember: false,
            });
        });

        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(kc_write_pty);
        s.handler.effects.clipboard_read = Some(kc_clipboard_read);
        s.handler.effects.clipboard_write = Some(kc_clipboard_write);

        s.next_slice(b"\x1B]5522;type=write:id=w\x1B\\");
        s.next_slice(b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;R2hvc3Q=\x1B\\");

        // The read payload decodes to a non-UTF-8 byte. It is dropped without
        // invoking the clipboard effect or disturbing the write transaction.
        s.next_slice(b"\x1B]5522;type=read;/w==\x1B\\");
        assert_eq!(0, kc(|c| c.read_count));
        assert_eq!(0, kc_responses_len());

        s.next_slice(b"\x1B]5522;type=wdata\x1B\\");
        assert_eq!(1, kc(|c| c.write_count));
        assert_eq!(
            b"\x1B]5522;type=write:status=DONE:id=w\x1B\\".as_slice(),
            kc_responses().as_slice(),
        );

        s.deinit();
        t.deinit(Global);
    }

    // test "kitty clipboard oversized text write aborts with EFBIG"
    #[test]
    fn kitty_clipboard_oversized_text_write_aborts_with_efbig() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        kc_reset();

        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(kc_write_pty);
        s.handler.effects.clipboard_write = Some(kc_clipboard_write);

        // Shrink the limit so the test doesn't have to stream the
        // default 64MiB.
        s.handler.kitty_clipboard_write_max_bytes = 4;

        s.next_slice(b"\x1B]5522;type=write:id=w\x1B\\");
        s.next_slice(b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;SGVsbA==\x1B\\"); // "Hell"
        assert_eq!(0, kc_responses_len());
        s.next_slice(b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;bw==\x1B\\"); // "o"
        assert_eq!(
            b"\x1B]5522;type=write:status=EFBIG:id=w\x1B\\".as_slice(),
            kc_responses().as_slice(),
        );
        assert!(!s.handler.semantic_failure);

        // The transaction is gone: later data and the commit do nothing.
        s.next_slice(b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;IQ==\x1B\\");
        s.next_slice(b"\x1B]5522;type=wdata\x1B\\");
        assert_eq!(0, kc(|c| c.write_count));
        assert_eq!(
            b"\x1B]5522;type=write:status=EFBIG:id=w\x1B\\".as_slice(),
            kc_responses().as_slice(),
        );

        s.deinit();
        t.deinit(Global);
    }

    // test "kitty clipboard invalid wdata chunk aborts with EINVAL"
    #[test]
    fn kitty_clipboard_invalid_wdata_chunk_aborts_with_einval() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        kc_reset();

        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(kc_write_pty);
        s.handler.effects.clipboard_write = Some(kc_clipboard_write);

        s.next_slice(b"\x1B]5522;type=write:id=w\x1B\\");
        s.next_slice(b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;SGVsbG8=\x1B\\"); // "Hello"
        s.next_slice(b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;!!!bad!!!\x1B\\");
        assert_eq!(
            b"\x1B]5522;type=write:status=EINVAL:id=w\x1B\\".as_slice(),
            kc_responses().as_slice(),
        );
        assert!(!s.handler.semantic_failure);

        // The transaction is gone: later data and the commit do nothing.
        s.next_slice(b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;V29ybGQ=\x1B\\"); // "World"
        s.next_slice(b"\x1B]5522;type=wdata\x1B\\");
        assert_eq!(0, kc(|c| c.write_count));
        assert_eq!(
            b"\x1B]5522;type=write:status=EINVAL:id=w\x1B\\".as_slice(),
            kc_responses().as_slice(),
        );

        s.deinit();
        t.deinit(Global);
    }

    // test "kitty clipboard wdata chunks split one base64 stream"
    #[test]
    fn kitty_clipboard_wdata_chunks_split_one_base64_stream() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        kc_reset();

        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(kc_write_pty);
        s.handler.effects.clipboard_write = Some(kc_clipboard_write);

        // "some data" encoded as one stream, split at non-group
        // boundaries across packets.
        s.next_slice(b"\x1B]5522;type=write\x1B\\");
        s.next_slice(b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;c29\x1B\\");
        s.next_slice(b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;tZSBk\x1B\\");
        s.next_slice(b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;YXRh\x1B\\");
        s.next_slice(b"\x1B]5522;type=wdata\x1B\\");

        assert_eq!(1, kc(|c| c.write_count));
        kc(|c| assert_eq!(b"some data".as_slice(), c.last_contents[0].1.as_slice()));
        assert_eq!(
            b"\x1B]5522;type=write:status=DONE\x1B\\".as_slice(),
            kc_responses().as_slice(),
        );

        s.deinit();
        t.deinit(Global);
    }

    // test "kitty clipboard unpadded wdata stream aborts at commit"
    #[test]
    fn kitty_clipboard_unpadded_wdata_stream_aborts_at_commit() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        kc_reset();

        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(kc_write_pty);
        s.handler.effects.clipboard_write = Some(kc_clipboard_write);

        // "Hello" without its final padding byte: every packet decodes,
        // but the stream ends mid-group so the commit reports EINVAL.
        s.next_slice(b"\x1B]5522;type=write:id=w\x1B\\");
        s.next_slice(b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;SGVsbG8\x1B\\");
        assert_eq!(0, kc_responses_len());
        s.next_slice(b"\x1B]5522;type=wdata\x1B\\");
        assert_eq!(0, kc(|c| c.write_count));
        assert_eq!(
            b"\x1B]5522;type=write:status=EINVAL:id=w\x1B\\".as_slice(),
            kc_responses().as_slice(),
        );
        assert!(!s.handler.semantic_failure);

        s.deinit();
        t.deinit(Global);
    }

    // test "kitty clipboard in-flight transaction is freed on deinit"
    #[test]
    fn kitty_clipboard_in_flight_transaction_is_freed_on_deinit() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        kc_reset();

        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(kc_write_pty);
        s.handler.effects.clipboard_write = Some(kc_clipboard_write);

        // Never committed: stream deinit must free the transaction (the
        // testing allocator catches the leak otherwise; the ported Box
        // frees on drop and `deinit` clears it explicitly).
        s.next_slice(b"\x1B]5522;type=write\x1B\\");
        s.next_slice(b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;R2hvc3Q=\x1B\\");
        assert_eq!(0, kc(|c| c.write_count));
        assert!(s.handler.kitty_clipboard_write.is_some());
        s.deinit();
        assert!(s.handler.kitty_clipboard_write.is_none());

        t.deinit(Global);
    }

    // test "kitty clipboard allocation failure is ignored"
    //
    // PORT NOTE: the reference swaps `t.screens.active.alloc` for
    // `testing.failing_allocator`; rustty's allocator seam is the zero-sized
    // Global, so the `KITTY_CLIPBOARD_TW` fail point sits exactly where the
    // reference's `try alloc.create(WriteState)` is (see its module note).
    #[test]
    fn kitty_clipboard_allocation_failure_is_ignored() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        kc_reset();

        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(kc_write_pty);
        s.handler.effects.clipboard_write = Some(kc_clipboard_write);

        // Only transaction state uses the terminal allocator here. Swap in
        // an allocator that always fails, then restore it before teardown.
        {
            kitty_clipboard_tw_error_always(KittyClipboardFp::CreateWriteState);
            s.next_slice(b"\x1B]5522;type=write\x1B\\");
            kitty_clipboard_tw_end(rustty_util::tripwire::ResetMode::Reset).unwrap();
        }

        // Clipboard writes are external effects, best-effort like OSC 52;
        // the failed transaction never started and is not a semantic
        // failure.
        assert!(!s.handler.semantic_failure);
        s.next_slice(b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;R2hvc3Q=\x1B\\");
        s.next_slice(b"\x1B]5522;type=wdata\x1B\\");
        assert_eq!(0, kc(|c| c.write_count));
        assert_eq!(0, kc_responses_len());

        s.deinit();
        t.deinit(Global);
    }

    // test "kitty clipboard without write_pty still commits writes"
    #[test]
    fn kitty_clipboard_without_write_pty_still_commits_writes() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        kc_reset();

        let mut s = t.vt_stream();
        s.handler.effects.clipboard_write = Some(kc_clipboard_write);

        s.next_slice(b"\x1B]5522;type=write\x1B\\");
        s.next_slice(b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;R2hvc3Q=\x1B\\");
        s.next_slice(b"\x1B]5522;type=wdata\x1B\\");
        assert_eq!(1, kc(|c| c.write_count));
        kc(|c| assert_eq!(b"Ghost".as_slice(), c.last_contents[0].1.as_slice()));

        // Reads are dropped without a way to respond.
        s.next_slice(b"\x1B]5522;type=read\x1B\\");
        assert_eq!(0, kc_responses_len());

        s.deinit();
        t.deinit(Global);
    }

    // Capture state for the clipboard_read (OSC 52) effect test. Mirrors the
    // reference's per-test `S` struct through the thread-local idiom.
    thread_local! {
        static CR_WRITTEN: core::cell::RefCell<Vec<u8>> =
            const { core::cell::RefCell::new(Vec::new()) };
        static CR_COUNT: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
        static CR_LOCATION: core::cell::Cell<Option<clipboard::Location>> =
            const { core::cell::Cell::new(None) };
        static CR_MIMES: core::cell::RefCell<Vec<Vec<u8>>> =
            const { core::cell::RefCell::new(Vec::new()) };
        static CR_LIST: core::cell::Cell<bool> = const { core::cell::Cell::new(true) };
        static CR_NAME: core::cell::RefCell<Vec<u8>> =
            const { core::cell::RefCell::new(Vec::new()) };
        static CR_GRANTED: core::cell::Cell<bool> = const { core::cell::Cell::new(true) };
        static CR_CAN_REMEMBER: core::cell::Cell<bool> = const { core::cell::Cell::new(true) };
        static CR_RESULT: core::cell::RefCell<Option<KcReadResult>> =
            const { core::cell::RefCell::new(None) };
        static CR_REPLY_TWICE: core::cell::Cell<bool> = const { core::cell::Cell::new(false) };
    }

    fn cr_write_pty(_: &mut Handler, data: &[u8]) {
        CR_WRITTEN.with(|c| c.borrow_mut().extend_from_slice(data));
    }

    fn cr_clipboard_read(_: &mut Handler, read: clipboard::Read) {
        CR_COUNT.with(|c| c.set(c.get() + 1));
        CR_LOCATION.with(|c| c.set(Some(read.location)));
        CR_MIMES.with(|c| *c.borrow_mut() = read.mimes.iter().map(|m| m.to_vec()).collect());
        CR_LIST.with(|c| c.set(read.list));
        CR_NAME.with(|c| *c.borrow_mut() = read.name.to_vec());
        CR_GRANTED.with(|c| c.set(read.granted));
        CR_CAN_REMEMBER.with(|c| c.set(read.can_remember));
        let result = CR_RESULT.with(|c| c.borrow().clone());
        if let Some(r) = &result {
            r.reply_with(&read);
        }
        if CR_REPLY_TWICE.with(|c| c.get()) {
            read.reply(clipboard::ReadResult::Success(
                clipboard::ReadResultSuccess {
                    contents: &[clipboard::Content {
                        mime: b"text/plain",
                        data: b"again",
                    }],
                    ..clipboard::ReadResultSuccess::default()
                },
            ));
        }
    }

    fn cr_written() -> Vec<u8> {
        CR_WRITTEN.with(|c| c.borrow().clone())
    }

    // test "clipboard_read effect callback"
    #[test]
    fn clipboard_read_effect_callback() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        CR_WRITTEN.with(|c| c.borrow_mut().clear());
        CR_COUNT.with(|c| c.set(0));
        CR_RESULT.with(|c| {
            *c.borrow_mut() = Some(KcReadResult::Success {
                contents: vec![(b"text/plain".to_vec(), b"hello".to_vec())],
                available: Vec::new(),
                remember: false,
            })
        });
        CR_REPLY_TWICE.with(|c| c.set(false));

        // A null callback (the default readonly effects) silently ignores reads.
        {
            let mut s = t.vt_stream();
            s.handler.effects.write_pty = Some(cr_write_pty);
            s.next_slice(b"\x1B]52;c;?\x1B\\");
            assert_eq!(0, cr_written().len());
            s.deinit();
        }

        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(cr_write_pty);
        s.handler.effects.clipboard_read = Some(cr_clipboard_read);

        // Success echoes the normalized selector and request terminator.
        s.next_slice(b"\x1B]52;c;?\x1B\\");
        assert_eq!(1, CR_COUNT.with(|c| c.get()));
        assert_eq!(
            Some(clipboard::Location::Standard),
            CR_LOCATION.with(|c| c.get())
        );
        assert_eq!(1, CR_MIMES.with(|c| c.borrow().len()));
        CR_MIMES.with(|c| assert_eq!(b"text/plain".as_slice(), c.borrow()[0].as_slice()));
        assert!(!CR_LIST.with(|c| c.get()));
        assert_eq!(0, CR_NAME.with(|c| c.borrow().len()));
        assert!(!CR_GRANTED.with(|c| c.get()));
        assert!(!CR_CAN_REMEMBER.with(|c| c.get()));
        assert_eq!(
            b"\x1B]52;c;aGVsbG8=\x1B\\".as_slice(),
            cr_written().as_slice()
        );

        CR_WRITTEN.with(|c| c.borrow_mut().clear());
        s.next_slice(b"\x1B]52;p;?\x07");
        assert_eq!(
            Some(clipboard::Location::Primary),
            CR_LOCATION.with(|c| c.get())
        );
        assert_eq!(
            b"\x1B]52;p;aGVsbG8=\x07".as_slice(),
            cr_written().as_slice()
        );

        // Only the first text representation is used.
        CR_WRITTEN.with(|c| c.borrow_mut().clear());
        CR_RESULT.with(|c| {
            *c.borrow_mut() = Some(KcReadResult::Success {
                contents: vec![
                    (b"image/png".to_vec(), b"\x89PNG".to_vec()),
                    (b"UTF8_STRING".to_vec(), b"hi".to_vec()),
                ],
                available: Vec::new(),
                // OSC 52 has no session passwords, so remember is ignored.
                remember: true,
            })
        });
        s.next_slice(b"\x1B]52;s;?\x1B\\");
        assert_eq!(
            Some(clipboard::Location::Selection),
            CR_LOCATION.with(|c| c.get())
        );
        assert_eq!(b"\x1B]52;s;aGk=\x1B\\".as_slice(), cr_written().as_slice());

        // Every failure, no text, and no reply all answer with an empty
        // clipboard.
        let failure_results: &[Option<KcReadResult>] = &[
            Some(KcReadResult::Denied),
            Some(KcReadResult::Unsupported),
            Some(KcReadResult::Busy),
            Some(KcReadResult::IoError),
            Some(KcReadResult::Success {
                contents: Vec::new(),
                available: Vec::new(),
                remember: false,
            }),
            None,
        ];
        for result in failure_results {
            CR_WRITTEN.with(|c| c.borrow_mut().clear());
            CR_RESULT.with(|c| *c.borrow_mut() = result.clone());
            s.next_slice(b"\x1B]52;c;?\x1B\\");
            assert_eq!(b"\x1B]52;c;\x1B\\".as_slice(), cr_written().as_slice());
        }

        // A second reply is ignored.
        CR_WRITTEN.with(|c| c.borrow_mut().clear());
        CR_RESULT.with(|c| {
            *c.borrow_mut() = Some(KcReadResult::Success {
                contents: vec![(b"text/plain".to_vec(), b"hello".to_vec())],
                available: Vec::new(),
                remember: false,
            })
        });
        CR_REPLY_TWICE.with(|c| c.set(true));
        s.next_slice(b"\x1B]52;c;?\x1B\\");
        assert_eq!(
            b"\x1B]52;c;aGVsbG8=\x1B\\".as_slice(),
            cr_written().as_slice()
        );

        CR_REPLY_TWICE.with(|c| c.set(false));
        s.deinit();
        t.deinit(Global);
    }

    // test "mode 2048 enable reports current geometry and disable is silent"
    #[test]
    fn mode_2048_enable_reports_current_geometry_and_disable_is_silent() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        fn get_size(_: &mut Handler) -> Option<size_report::Size> {
            Some(size_report::Size {
                rows: 24,
                columns: 80,
                cell_width: 8,
                cell_height: 16,
            })
        }
        calls_reset();

        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(count_write_pty);
        s.handler.effects.size = Some(get_size);

        s.next_slice(b"\x1b[?2048h");
        s.next_slice(b"\x1b[?2048h");

        assert_eq!(2, calls_get());
        assert_eq!(
            b"\x1b[48;24;80;384;640t".as_slice(),
            cap_take().unwrap().as_slice()
        );

        s.next_slice(b"\x1b[?2048l");
        assert_eq!(2, calls_get());
        assert!(!unsafe { &*s.handler.terminal }
            .modes
            .get(modespkg::Mode::InBandSizeReports));

        s.deinit();
        t.deinit(Global);
    }

    // test "mode 2048 enable tolerates missing effects"
    #[test]
    fn mode_2048_enable_tolerates_missing_effects() {
        fn get_size(_: &mut Handler) -> Option<size_report::Size> {
            Some(size_report::Size {
                rows: 24,
                columns: 80,
                cell_width: 8,
                cell_height: 16,
            })
        }
        calls_reset();

        let mut no_size_terminal = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut no_size_stream = no_size_terminal.vt_stream();
            no_size_stream.handler.effects.write_pty = Some(count_write_pty);

            no_size_stream.next_slice(b"\x1b[?2048h");
            assert!(unsafe { &*no_size_stream.handler.terminal }
                .modes
                .get(modespkg::Mode::InBandSizeReports));
            assert_eq!(0, calls_get());
            no_size_stream.deinit();
        }
        no_size_terminal.deinit(Global);

        let mut no_write_terminal = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut no_write_stream = no_write_terminal.vt_stream();
            no_write_stream.handler.effects.size = Some(get_size);

            no_write_stream.next_slice(b"\x1b[?2048h");
            assert!(unsafe { &*no_write_stream.handler.terminal }
                .modes
                .get(modespkg::Mode::InBandSizeReports));
            assert_eq!(0, calls_get());
            no_write_stream.deinit();
        }
        no_write_terminal.deinit(Global);
    }

    // Capture for the kitty dnd tests: every pty write.
    thread_local! {
        static DND_PTY: core::cell::RefCell<Vec<u8>> =
            const { core::cell::RefCell::new(Vec::new()) };
        static DND_EVENTS: core::cell::RefCell<Vec<kitty::dnd::Event>> =
            const { core::cell::RefCell::new(Vec::new()) };
        static DND_MIMES: core::cell::RefCell<Vec<u8>> =
            const { core::cell::RefCell::new(Vec::new()) };
    }

    fn dnd_write_pty(_: &mut Handler, data: &[u8]) {
        DND_PTY.with(|c| c.borrow_mut().extend_from_slice(data));
    }

    fn dnd_pty() -> Vec<u8> {
        DND_PTY.with(|c| c.borrow().clone())
    }

    // test "kitty dnd: query response"
    #[test]
    fn kitty_dnd_query_response() {
        DND_PTY.with(|c| c.borrow_mut().clear());

        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();

        let mut s = t.vt_stream();
        s.handler.effects.write_pty = Some(dnd_write_pty);

        s.next_slice(b"\x1B]72;t=q:i=3\x1B\\");
        assert_eq!(b"\x1b]72;t=q:i=3\x1b\\".as_slice(), dnd_pty().as_slice());

        s.deinit();
        t.deinit(Global);
    }

    // test "kitty dnd: register, drop, and serve data"
    #[test]
    fn kitty_dnd_register_drop_and_serve_data() {
        DND_PTY.with(|c| c.borrow_mut().clear());

        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        let ptr: *mut Terminal = &mut t;

        let mut s = Stream::init(stream::Options {
            allocator: Some(rustty_lib::allocator::ZigAllocatorRef::new(
                &crate::terminal::GLOBAL_ZIG_ALLOCATOR,
            )),
            handler: Handler::init(ptr),
            continuation_max_bytes: None,
        });
        s.handler.effects.write_pty = Some(dnd_write_pty);

        // Client registers to accept drops.
        s.next_slice(b"\x1B]72;t=a;text/plain text/uri-list\x1B\\");
        assert_eq!(0, dnd_pty().len());
        assert!(unsafe { (*ptr).kitty_dnd.is_some() });

        // A native drop arrives; the embedder feeds it to the terminal
        // state and delivers the produced event bytes itself.
        {
            let mut aw: Vec<u8> = Vec::new();
            unsafe { (*ptr).kitty_dnd.as_mut() }
                .unwrap()
                .drag_drop(
                    &mut aw,
                    kitty::dnd::MoveEvent {
                        cell_x: 2,
                        cell_y: 1,
                        pixel_x: 20,
                        pixel_y: 18,
                        operations: kitty::dnd::Operations {
                            copy: true,
                            r#move: false,
                        },
                    },
                    &[kitty::dnd::Item {
                        mime: b"text/plain".to_vec(),
                        data: b"hello".to_vec(),
                    }],
                )
                .unwrap();
            assert_eq!(
                b"\x1b]72;t=M:x=2:y=1:X=20:Y=18:o=1:m=0;text/plain \x1b\\".as_slice(),
                aw.as_slice(),
            );
        }

        // The client requests the data and concludes.
        s.next_slice(b"\x1B]72;t=r:x=1\x1B\\");
        let mut expected: Vec<u8> = Vec::new();
        expected.extend_from_slice(b"\x1b]72;t=r:x=1:m=0;aGVsbG8=\x1b\\");
        expected.extend_from_slice(b"\x1b]72;t=r:x=1\x1b\\");
        assert_eq!(expected, dnd_pty());
        DND_PTY.with(|c| c.borrow_mut().clear());

        s.next_slice(b"\x1B]72;t=r\x1B\\");
        assert_eq!(0, dnd_pty().len());
        assert!(unsafe { (*ptr).kitty_dnd.as_ref() }
            .unwrap()
            .drop
            .items
            .is_none());

        s.deinit();
        t.deinit(Global);
    }

    // test "kitty dnd: state updates work without write_pty effect"
    #[test]
    fn kitty_dnd_state_updates_work_without_write_pty_effect() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        let ptr: *mut Terminal = &mut t;

        let mut s = Stream::init(stream::Options {
            allocator: Some(rustty_lib::allocator::ZigAllocatorRef::new(
                &crate::terminal::GLOBAL_ZIG_ALLOCATOR,
            )),
            handler: Handler::init(ptr),
            continuation_max_bytes: None,
        });

        // Queries produce no output (nowhere to write) but registration
        // state still updates.
        s.next_slice(b"\x1B]72;t=q\x1B\\");
        s.next_slice(b"\x1B]72;t=a\x1B\\");
        assert!(unsafe { (*ptr).kitty_dnd.is_some() });

        // The terminal remains functional.
        s.next_slice(b"ok");
        let str = unsafe { (*ptr).plain_string(Global) }.unwrap();
        assert_eq!(b"ok".as_slice(), str.as_slice());

        s.deinit();
        t.deinit(Global);
    }

    // test "kitty dnd: registration survives terminal reset"
    #[test]
    fn kitty_dnd_registration_survives_terminal_reset() {
        DND_PTY.with(|c| c.borrow_mut().clear());

        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        let ptr: *mut Terminal = &mut t;

        let mut s = Stream::init(stream::Options {
            allocator: Some(rustty_lib::allocator::ZigAllocatorRef::new(
                &crate::terminal::GLOBAL_ZIG_ALLOCATOR,
            )),
            handler: Handler::init(ptr),
            continuation_max_bytes: None,
        });
        s.handler.effects.write_pty = Some(dnd_write_pty);

        // Start a chunked command, then reset mid-chunk.
        s.next_slice(b"\x1B]72;t=a:i=5\x1B\\");
        s.next_slice(b"\x1B]72;t=m:o=1:m=1;text/pl\x1B\\");
        s.next_slice(b"\x1Bc");

        // Registration survives (matching kitty), chunking was interrupted
        // so a new command is not treated as a continuation.
        assert!(unsafe { (*ptr).kitty_dnd.is_some() });
        assert!(
            !unsafe { (*ptr).kitty_dnd.as_ref() }
                .unwrap()
                .chunking
                .active
        );
        s.next_slice(b"\x1B]72;t=q\x1B\\");
        assert_eq!(b"\x1b]72;t=q\x1b\\".as_slice(), dnd_pty().as_slice());

        s.deinit();
        t.deinit(Global);
    }

    // test "kitty dnd: effect reports registration, acceptance, and conclusion"
    #[test]
    fn kitty_dnd_effect_reports_registration_acceptance_and_conclusion() {
        DND_EVENTS.with(|c| c.borrow_mut().clear());
        DND_MIMES.with(|c| c.borrow_mut().clear());

        fn drag_and_drop(handler: &mut Handler, ev: kitty::dnd::Event) {
            DND_EVENTS.with(|c| c.borrow_mut().push(ev));
            // Registration details are read from the terminal state.
            if ev == kitty::dnd::Event::Registration {
                DND_MIMES.with(|c| {
                    let mut mimes = c.borrow_mut();
                    mimes.clear();
                    // const state = handler.terminal.kitty_dnd orelse return;
                    // SAFETY: the handler aliases the terminal (see `term()`);
                    // the shared read is short-lived inside the callback.
                    let Some(state) = (unsafe { (*handler.terminal).kitty_dnd.as_ref() }) else {
                        return;
                    };
                    let mut it = state.registered_mimes();
                    // `while let ... = it.next()`, not `for m in it`: the
                    // reference writes `while (it.next()) |m|` and the port
                    // mirrors its loop form.
                    #[allow(clippy::while_let_on_iterator)]
                    while let Some(m) = it.next() {
                        mimes.extend_from_slice(m);
                        mimes.push(b',');
                    }
                });
            }
        }

        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        let ptr: *mut Terminal = &mut t;

        let mut s = Stream::init(stream::Options {
            allocator: Some(rustty_lib::allocator::ZigAllocatorRef::new(
                &crate::terminal::GLOBAL_ZIG_ALLOCATOR,
            )),
            handler: Handler::init(ptr),
            continuation_max_bytes: None,
        });
        s.handler.effects.drag_and_drop = Some(drag_and_drop);

        // Registration with a MIME list, read back from the state.
        s.next_slice(b"\x1B]72;t=a;image/png text/plain\x1B\\");
        assert_eq!(1, DND_EVENTS.with(|c| c.borrow().len()));
        assert_eq!(
            kitty::dnd::Event::Registration,
            DND_EVENTS.with(|c| c.borrow()[0])
        );
        DND_MIMES.with(|c| assert_eq!(b"image/png,text/plain,".as_slice(), c.borrow().as_slice()));

        // A native drag and the client's answer.
        {
            let mut aw: Vec<u8> = Vec::new();
            unsafe { (*ptr).kitty_dnd.as_mut() }
                .unwrap()
                .drag_drop(
                    &mut aw,
                    kitty::dnd::MoveEvent {
                        cell_x: 0,
                        cell_y: 0,
                        pixel_x: 0,
                        pixel_y: 0,
                        operations: kitty::dnd::Operations {
                            copy: true,
                            r#move: false,
                        },
                    },
                    &[kitty::dnd::Item {
                        mime: b"text/plain".to_vec(),
                        data: b"x".to_vec(),
                    }],
                )
                .unwrap();
        }
        s.next_slice(b"\x1B]72;t=m:o=2;text/plain\x1B\\");
        assert_eq!(2, DND_EVENTS.with(|c| c.borrow().len()));
        assert_eq!(
            kitty::dnd::Event::Acceptance,
            DND_EVENTS.with(|c| c.borrow()[1])
        );

        // Conclusion carries the performed operation.
        s.next_slice(b"\x1B]72;t=r:o=2\x1B\\");
        assert_eq!(3, DND_EVENTS.with(|c| c.borrow().len()));
        assert_eq!(
            kitty::dnd::Event::ConcludedMove,
            DND_EVENTS.with(|c| c.borrow()[2])
        );

        // Unregistration reports with the state gone.
        s.next_slice(b"\x1B]72;t=A\x1B\\");
        assert_eq!(4, DND_EVENTS.with(|c| c.borrow().len()));
        assert_eq!(
            kitty::dnd::Event::Registration,
            DND_EVENTS.with(|c| c.borrow()[3])
        );
        assert!(unsafe { (*ptr).kitty_dnd.is_none() });
        DND_MIMES.with(|c| assert_eq!(0, c.borrow().len()));

        s.deinit();
        t.deinit(Global);
    }

    /// Capture state for the Handler.paste tests below: every pty write and
    /// the clipboard reads the program makes afterwards.
    // const PasteCapture = struct { ... };
    #[derive(Default)]
    struct PasteCap {
        written: Vec<u8>,
        write_count: usize,
        read_count: usize,
        read_granted_count: usize,
        last_read_granted: bool,
        last_read_name: Vec<u8>,
    }

    thread_local! {
        static PASTE_CAP: core::cell::RefCell<PasteCap> =
            core::cell::RefCell::new(PasteCap::default());
    }

    fn pc_reset() {
        PASTE_CAP.with(|c| *c.borrow_mut() = PasteCap::default());
    }

    fn pc<T>(f: impl FnOnce(&PasteCap) -> T) -> T {
        PASTE_CAP.with(|c| f(&c.borrow()))
    }

    fn pc_written() -> Vec<u8> {
        pc(|c| c.written.clone())
    }

    // fn writePty(_: *Handler, data: []const u8) void { ... }
    fn pc_write_pty(_: &mut Handler, data: &[u8]) {
        PASTE_CAP.with(|c| {
            let mut cap = c.borrow_mut();
            cap.written.extend_from_slice(data);
            cap.write_count += 1;
        });
    }

    // fn clipboardRead(_: *Handler, read: clipboard.Read) void { ... }
    fn pc_clipboard_read(_: &mut Handler, read: clipboard::Read) {
        PASTE_CAP.with(|c| {
            let mut cap = c.borrow_mut();
            cap.read_count += 1;
            cap.last_read_granted = read.granted;
            if read.granted {
                cap.read_granted_count += 1;
            }
            cap.last_read_name = read.name.to_vec();
        });
        read.reply(clipboard::ReadResult::Success(
            clipboard::ReadResultSuccess {
                contents: &[clipboard::Content {
                    mime: b"text/plain",
                    data: b"Ghostty",
                }],
                ..clipboard::ReadResultSuccess::default()
            },
        ));
    }

    // test "paste: no write_pty effect is an error"
    #[test]
    fn paste_no_write_pty_effect_is_an_error() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        let ptr: *mut Terminal = &mut t;

        let mut handler = Handler::init(ptr);
        assert_eq!(
            Err(PasteError::NoWritePty),
            handler.paste(Paste {
                source: paste_pkg::Source::default(),
                contents: paste_pkg::Contents::Memory(&[clipboard::Content {
                    mime: b"text/plain",
                    data: b"hello",
                }]),
                allow_unsafe: false,
            })
        );
        use crate::stream::Handler as _;
        handler.deinit();
        t.deinit(Global);
    }

    // test "paste: plain text converts newlines and strips unsafe bytes"
    #[test]
    fn paste_plain_text_converts_newlines_and_strips_unsafe_bytes() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        let ptr: *mut Terminal = &mut t;
        pc_reset();

        let mut handler = Handler::init(ptr);
        handler.effects.write_pty = Some(pc_write_pty);

        // Newlines are unsafe unbracketed; the embedder confirmed.
        assert!(handler
            .paste(Paste {
                source: paste_pkg::Source::default(),
                contents: paste_pkg::Contents::Memory(&[clipboard::Content {
                    mime: b"text/plain",
                    data: b"hel\x1blo\nwor\x00ld",
                }]),
                allow_unsafe: true,
            })
            .unwrap());
        assert_eq!(b"hel lo\rwor ld".as_slice(), pc_written().as_slice());
        assert_eq!(1, pc(|c| c.write_count));

        // The first text representation is used; others are ignored.
        pc_reset();
        assert!(handler
            .paste(Paste {
                source: paste_pkg::Source::default(),
                contents: paste_pkg::Contents::Memory(&[
                    clipboard::Content {
                        mime: b"image/png",
                        data: b"\x89PNG",
                    },
                    clipboard::Content {
                        mime: b"UTF8_STRING",
                        data: b"hi",
                    },
                    clipboard::Content {
                        mime: b"text/plain",
                        data: b"ignored",
                    },
                ]),
                allow_unsafe: false,
            })
            .unwrap());
        assert_eq!(b"hi".as_slice(), pc_written().as_slice());
        assert_eq!(1, pc(|c| c.write_count));

        use crate::stream::Handler as _;
        handler.deinit();
        t.deinit(Global);
    }

    // test "paste: unsafe text is refused unless allowed"
    #[test]
    fn paste_unsafe_text_is_refused_unless_allowed() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        let ptr: *mut Terminal = &mut t;
        pc_reset();

        let mut handler = Handler::init(ptr);
        handler.effects.write_pty = Some(pc_write_pty);

        assert_eq!(
            Err(PasteError::UnsafePaste),
            handler.paste(Paste {
                source: paste_pkg::Source::default(),
                contents: paste_pkg::Contents::Memory(&[clipboard::Content {
                    mime: b"text/plain",
                    data: b"rm -rf /\n",
                }]),
                allow_unsafe: false,
            })
        );
        assert_eq!(0, pc(|c| c.write_count));

        assert!(handler
            .paste(Paste {
                source: paste_pkg::Source::default(),
                contents: paste_pkg::Contents::Memory(&[clipboard::Content {
                    mime: b"text/plain",
                    data: b"rm -rf /\n",
                }]),
                allow_unsafe: true,
            })
            .unwrap());
        assert_eq!(b"rm -rf /\r".as_slice(), pc_written().as_slice());

        use crate::stream::Handler as _;
        handler.deinit();
        t.deinit(Global);
    }

    // test "paste: bracketed paste frames the text"
    #[test]
    fn paste_bracketed_paste_frames_the_text() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        let ptr: *mut Terminal = &mut t;
        pc_reset();

        let mut handler = Handler::init(ptr);
        handler.effects.write_pty = Some(pc_write_pty);
        unsafe { (*ptr).modes.set(modespkg::Mode::BracketedPaste, true) };

        // Newlines are safe inside the frame and are preserved.
        assert!(handler
            .paste(Paste {
                source: paste_pkg::Source::default(),
                contents: paste_pkg::Contents::Memory(&[clipboard::Content {
                    mime: b"text/plain",
                    data: b"hello\nworld",
                }]),
                allow_unsafe: false,
            })
            .unwrap());
        assert_eq!(
            b"\x1b[200~hello\nworld\x1b[201~".as_slice(),
            pc_written().as_slice()
        );
        assert_eq!(1, pc(|c| c.write_count));

        // The frame terminator is not.
        pc_reset();
        assert_eq!(
            Err(PasteError::UnsafePaste),
            handler.paste(Paste {
                source: paste_pkg::Source::default(),
                contents: paste_pkg::Contents::Memory(&[clipboard::Content {
                    mime: b"text/plain",
                    data: b"he\x1b[201~llo",
                }]),
                allow_unsafe: false,
            })
        );
        assert_eq!(0, pc(|c| c.write_count));

        // Allowed, the stripper still defuses it: ESC becomes a space.
        assert!(handler
            .paste(Paste {
                source: paste_pkg::Source::default(),
                contents: paste_pkg::Contents::Memory(&[clipboard::Content {
                    mime: b"text/plain",
                    data: b"he\x1b[201~llo",
                }]),
                allow_unsafe: true,
            })
            .unwrap());
        assert_eq!(
            b"\x1b[200~he [201~llo\x1b[201~".as_slice(),
            pc_written().as_slice()
        );

        use crate::stream::Handler as _;
        handler.deinit();
        t.deinit(Global);
    }

    // test "paste: no text representation writes nothing"
    #[test]
    fn paste_no_text_representation_writes_nothing() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        let ptr: *mut Terminal = &mut t;
        pc_reset();

        let mut handler = Handler::init(ptr);
        handler.effects.write_pty = Some(pc_write_pty);

        assert!(!handler
            .paste(Paste {
                source: paste_pkg::Source::default(),
                contents: paste_pkg::Contents::Memory(&[clipboard::Content {
                    mime: b"image/png",
                    data: b"\x89PNG",
                }]),
                allow_unsafe: false,
            })
            .unwrap());
        assert!(!handler
            .paste(Paste {
                source: paste_pkg::Source::default(),
                contents: paste_pkg::Contents::Memory(&[]),
                allow_unsafe: false,
            })
            .unwrap());
        assert!(!handler
            .paste(Paste {
                source: paste_pkg::Source::default(),
                contents: paste_pkg::Contents::Memory(&[clipboard::Content {
                    mime: b"text/plain",
                    data: b"",
                }]),
                allow_unsafe: false,
            })
            .unwrap());
        assert_eq!(0, pc(|c| c.write_count));

        use crate::stream::Handler as _;
        handler.deinit();
        t.deinit(Global);
    }

    // test "paste: large text streams to the pty in chunks"
    #[test]
    fn paste_large_text_streams_to_the_pty_in_chunks() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        let ptr: *mut Terminal = &mut t;
        pc_reset();

        let mut handler = Handler::init(ptr);
        handler.effects.write_pty = Some(pc_write_pty);
        unsafe { (*ptr).modes.set(modespkg::Mode::BracketedPaste, true) };

        // Two full chunks and a partial one with the frame, never the
        // whole thing at once.
        let data = vec![b'x'; 10_000];
        assert!(handler
            .paste(Paste {
                source: paste_pkg::Source::default(),
                contents: paste_pkg::Contents::Memory(&[clipboard::Content {
                    mime: b"text/plain",
                    data: &data,
                }]),
                allow_unsafe: false,
            })
            .unwrap());
        let total = data.len() + b"\x1b[200~\x1b[201~".len();
        assert_eq!(total.div_ceil(PASTE_CHUNK_SIZE), pc(|c| c.write_count));
        assert_eq!(total, pc(|c| c.written.len()));
        assert!(pc_written().starts_with(b"\x1b[200~xxx"));
        assert!(pc_written().ends_with(b"xxx\x1b[201~"));

        use crate::stream::Handler as _;
        handler.deinit();
        t.deinit(Global);
    }

    /// A paste contents reader for the tests: serves fixed data per MIME
    /// type in pieces, counting the reads of each representation.
    // const PasteReader = struct { ... };
    struct PasteReader {
        mimes: &'static [&'static [u8]],
        data: &'static [&'static [u8]],
        piece: usize,
        reads: [usize; 4],
        /// Fail after this many bytes of a read.
        fail_after: Option<usize>,
    }

    impl PasteReader {
        fn new(mimes: &'static [&'static [u8]], data: &'static [&'static [u8]]) -> PasteReader {
            PasteReader {
                mimes,
                data,
                // piece: usize = 3 (the reference field default)
                piece: 3,
                reads: [0; 4],
                fail_after: None,
            }
        }

        // fn contents(self: *PasteReader) paste_pkg.Contents { ... }
        fn contents(&mut self) -> paste_pkg::Contents<'static> {
            paste_pkg::Contents::Reader(paste_pkg::Reader {
                mimes: self.mimes,
                read: clipboard::MimeReader {
                    ctx: self as *mut PasteReader as *mut core::ffi::c_void,
                    read_fn: PasteReader::read,
                },
            })
        }

        // fn read(ctx: ?*anyopaque, mime: []const u8, sink: *std.Io.Writer)
        //     clipboard.MimeReader.Error!void { ... }
        fn read(
            ctx: *mut core::ffi::c_void,
            mime: &[u8],
            sink: &mut dyn crate::snapshot::io::Writer,
        ) -> Result<(), clipboard::MimeReaderError> {
            // SAFETY: `ctx` is the `&mut PasteReader` `contents` captured; the
            // reader is only used during the paste call and no other reference
            // to it is live then.
            let this = unsafe { &mut *(ctx as *mut PasteReader) };
            let index = match this.mimes.iter().position(|m| *m == mime) {
                Some(i) => i,
                None => return Err(clipboard::MimeReaderError::ReadFailed),
            };
            this.reads[index] += 1;
            let data = this.data[index];
            let mut offset: usize = 0;
            while offset < data.len() {
                if let Some(limit) = this.fail_after {
                    if offset >= limit {
                        return Err(clipboard::MimeReaderError::ReadFailed);
                    }
                }
                let n = this.piece.min(data.len() - offset);
                sink.write_all(&data[offset..offset + n])
                    .map_err(|_| clipboard::MimeReaderError::WriteFailed)?;
                offset += n;
            }
            Ok(())
        }
    }

    // test "paste: reader contents are read on demand"
    #[test]
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION: sys::io_native OTP entropy reaches the real \
                  OS RNG (`arc4random_buf`), which Miri does not support; the \
                  paste/grant aliasing is Miri-verified by the sibling 5522 \
                  tests that use the sys override / io_failing"
    )]
    fn paste_reader_contents_are_read_on_demand() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        let ptr: *mut Terminal = &mut t;
        pc_reset();

        let mut handler = Handler::init(ptr);
        handler.effects.write_pty = Some(pc_write_pty);
        handler.effects.clipboard_read = Some(pc_clipboard_read);

        // Unsafe text is refused from one read with nothing written; the
        // image is never read.
        let mut reader = PasteReader::new(
            &[b"image/png", b"text/plain"],
            &[b"\x89PNG", b"echo hi\nrm -rf /\n"],
        );
        assert_eq!(
            Err(PasteError::UnsafePaste),
            handler.paste(Paste {
                source: paste_pkg::Source::default(),
                contents: reader.contents(),
                allow_unsafe: false,
            })
        );
        assert_eq!(0, pc(|c| c.write_count));
        assert_eq!(0, reader.reads[0]);
        assert_eq!(1, reader.reads[1]);

        // Allowed, the text is read once, encoded.
        reader.reads = [0; 4];
        assert!(handler
            .paste(Paste {
                source: paste_pkg::Source::default(),
                contents: reader.contents(),
                allow_unsafe: true,
            })
            .unwrap());
        assert_eq!(b"echo hi\rrm -rf /\r".as_slice(), pc_written().as_slice());
        assert_eq!(0, reader.reads[0]);
        assert_eq!(1, reader.reads[1]);

        // Safe text is read once too: buffered for the check, then written.
        pc_reset();
        let mut reader = PasteReader::new(&[b"text/plain"], &[b"hello world"]);
        assert!(handler
            .paste(Paste {
                source: paste_pkg::Source::default(),
                contents: reader.contents(),
                allow_unsafe: false,
            })
            .unwrap());
        assert_eq!(b"hello world".as_slice(), pc_written().as_slice());
        assert_eq!(1, reader.reads[0]);

        // Empty text is nothing to paste, found on the one read.
        pc_reset();
        let mut reader = PasteReader::new(&[b"text/plain"], &[b""]);
        assert!(!handler
            .paste(Paste {
                source: paste_pkg::Source::default(),
                contents: reader.contents(),
                allow_unsafe: false,
            })
            .unwrap());
        assert_eq!(0, pc(|c| c.write_count));
        assert_eq!(1, reader.reads[0]);

        // A paste event lists the types and reads nothing at all.
        pc_reset();
        unsafe { (*ptr).modes.set(modespkg::Mode::KittyPasteEvents, true) };
        let mut reader = PasteReader::new(&[b"text/plain", b"image/png"], &[b"secret", b"\x89PNG"]);
        assert!(handler
            .paste(Paste {
                source: paste_pkg::Source::default(),
                contents: reader.contents(),
                allow_unsafe: false,
            })
            .unwrap());
        let written = pc_written();
        assert!(written
            .windows(b";dGV4dC9wbGFpbiBpbWFnZS9wbmcK\x1b\\".len())
            .any(|w| w == b";dGV4dC9wbGFpbiBpbWFnZS9wbmcK\x1b\\"));
        assert!(!written.windows(b"secret".len()).any(|w| w == b"secret"));
        assert_eq!(0, reader.reads[0]);
        assert_eq!(0, reader.reads[1]);

        use crate::stream::Handler as _;
        handler.deinit();
        t.deinit(Global);
    }

    // test "paste: reader failure writes nothing"
    #[test]
    fn paste_reader_failure_writes_nothing() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        let ptr: *mut Terminal = &mut t;
        pc_reset();

        let mut handler = Handler::init(ptr);
        handler.effects.write_pty = Some(pc_write_pty);
        unsafe { (*ptr).modes.set(modespkg::Mode::BracketedPaste, true) };

        // The read is buffered whole before anything is written, so a
        // mid-read failure discards the buffer, checked or not.
        let mut reader = PasteReader::new(&[b"text/plain"], &[b"hello world"]);
        reader.fail_after = Some(6);
        assert_eq!(
            Err(PasteError::ReadFailed),
            handler.paste(Paste {
                source: paste_pkg::Source::default(),
                contents: reader.contents(),
                allow_unsafe: false,
            })
        );
        assert_eq!(0, pc(|c| c.write_count));

        assert_eq!(
            Err(PasteError::ReadFailed),
            handler.paste(Paste {
                source: paste_pkg::Source::default(),
                contents: reader.contents(),
                allow_unsafe: true,
            })
        );
        assert_eq!(0, pc(|c| c.write_count));

        use crate::stream::Handler as _;
        handler.deinit();
        t.deinit(Global);
    }

    // test "paste: mode 5522 sends an event the program can read with"
    #[test]
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION: sys::io_native OTP entropy reaches the real \
                  OS RNG (`arc4random_buf`), which Miri does not support; the \
                  paste/grant aliasing is Miri-verified by the sibling 5522 \
                  tests that use the sys override / io_failing"
    )]
    fn paste_mode_5522_sends_an_event_the_program_can_read_with() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        let ptr: *mut Terminal = &mut t;
        pc_reset();

        let mut handler = Handler::init(ptr);
        handler.effects.write_pty = Some(pc_write_pty);
        handler.effects.clipboard_read = Some(pc_clipboard_read);
        let mut s = Stream::init(stream::Options {
            allocator: Some(rustty_lib::allocator::ZigAllocatorRef::new(
                &crate::terminal::GLOBAL_ZIG_ALLOCATOR,
            )),
            handler,
            continuation_max_bytes: None,
        });
        unsafe { (*ptr).modes.set(modespkg::Mode::KittyPasteEvents, true) };
        unsafe { (*ptr).modes.set(modespkg::Mode::BracketedPaste, true) };

        // Every representation is listed, the data is never written, and
        // the one-time password rides on every packet.
        assert!(s
            .handler
            .paste(Paste {
                source: paste_pkg::Source::default(),
                contents: paste_pkg::Contents::Memory(&[
                    clipboard::Content {
                        mime: b"text/plain",
                        data: b"secret",
                    },
                    clipboard::Content {
                        mime: b"image/png",
                        data: b"",
                    },
                ]),
                allow_unsafe: false,
            })
            .unwrap());
        assert_eq!(1, pc(|c| c.write_count));
        let written = pc_written();
        assert_eq!(
            3,
            written
                .windows(b"\x1b]5522;".len())
                .filter(|w| *w == b"\x1b]5522;")
                .count()
        );
        assert!(!written.windows(b"secret".len()).any(|w| w == b"secret"));
        assert!(!written
            .windows(b"\x1b[200~".len())
            .any(|w| w == b"\x1b[200~"));

        // OK packet: parse the (base64) password out.
        let ok_prefix = b"\x1b]5522;type=read:status=OK:pw=";
        assert!(written.starts_with(ok_prefix));
        let pw_end = ok_prefix.len()
            + written[ok_prefix.len()..]
                .windows(2)
                .position(|w| w == b"\x1b\\")
                .unwrap();
        // Copied out since the capture buffer is reused below.
        let pw_b64: Vec<u8> = written[ok_prefix.len()..pw_end].to_vec();
        // std.base64.standard.Encoder.calcSize(kitty_clipboard.otp_len)
        assert_eq!(kitty::clipboard::otp_len.div_ceil(3) * 4, pw_b64.len());

        // Listing packet: base64 of "text/plain image/png\n".
        let mut expected: Vec<u8> = Vec::new();
        expected.extend_from_slice(b"\x1b]5522;type=read:status=OK:pw=");
        expected.extend_from_slice(&pw_b64);
        expected.extend_from_slice(b"\x1b\\");
        expected.extend_from_slice(b"\x1b]5522;type=read:status=DATA:mime=Lg==:pw=");
        expected.extend_from_slice(&pw_b64);
        expected.extend_from_slice(b";dGV4dC9wbGFpbiBpbWFnZS9wbmcK\x1b\\");
        expected.extend_from_slice(b"\x1b]5522;type=read:status=DONE:pw=");
        expected.extend_from_slice(&pw_b64);
        expected.extend_from_slice(b"\x1b\\");
        assert_eq!(expected, written);

        // The program reads with the password and the name "Paste event":
        // the read arrives granted, exactly once.
        let mut read: Vec<u8> = Vec::new();
        read.extend_from_slice(b"\x1b]5522;type=read:pw=");
        read.extend_from_slice(&pw_b64);
        read.extend_from_slice(b":name=UGFzdGUgZXZlbnQ=;dGV4dC9wbGFpbg==\x1b\\");
        pc_reset();
        s.next_slice(&read);
        assert_eq!(1, pc(|c| c.read_count));
        assert!(pc(|c| c.last_read_granted));
        pc(|c| assert_eq!(b"Paste event".as_slice(), c.last_read_name.as_slice()));
        let mut expected: Vec<u8> = Vec::new();
        expected.extend_from_slice(b"\x1b]5522;type=read:status=OK\x1b\\");
        expected.extend_from_slice(
            b"\x1b]5522;type=read:status=DATA:mime=dGV4dC9wbGFpbg==;R2hvc3R0eQ==\x1b\\",
        );
        expected.extend_from_slice(b"\x1b]5522;type=read:status=DONE\x1b\\");
        assert_eq!(expected, pc_written());

        // The password was one-time: a second read is not granted.
        pc_reset();
        s.next_slice(&read);
        assert_eq!(1, pc(|c| c.read_count));
        assert!(!pc(|c| c.last_read_granted));
        assert_eq!(0, pc(|c| c.read_granted_count));

        // Every event mints a fresh password.
        pc_reset();
        assert!(s
            .handler
            .paste(Paste {
                source: paste_pkg::Source::default(),
                contents: paste_pkg::Contents::Memory(&[clipboard::Content {
                    mime: b"text/plain",
                    data: b"secret",
                }]),
                allow_unsafe: false,
            })
            .unwrap());
        let written = pc_written();
        assert!(!written
            .windows(pw_b64.len())
            .any(|w| w == pw_b64.as_slice()));

        s.deinit();
        t.deinit(Global);
    }

    // test "paste: mode 5522 reports the selection as primary"
    #[test]
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION: sys::io_native OTP entropy reaches the real \
                  OS RNG (`arc4random_buf`), which Miri does not support; the \
                  paste/grant aliasing is Miri-verified by the sibling 5522 \
                  tests that use the sys override / io_failing"
    )]
    fn paste_mode_5522_reports_the_selection_as_primary() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        let ptr: *mut Terminal = &mut t;
        pc_reset();

        let mut handler = Handler::init(ptr);
        handler.effects.write_pty = Some(pc_write_pty);
        handler.effects.clipboard_read = Some(pc_clipboard_read);
        unsafe { (*ptr).modes.set(modespkg::Mode::KittyPasteEvents, true) };

        for location in [clipboard::Location::Primary, clipboard::Location::Selection] {
            pc_reset();
            assert!(handler
                .paste(Paste {
                    source: paste_pkg::Source::Clipboard(location),
                    contents: paste_pkg::Contents::Memory(&[clipboard::Content {
                        mime: b"text/plain",
                        data: b"x",
                    }]),
                    allow_unsafe: false,
                })
                .unwrap());
            let written = pc_written();
            assert!(written.starts_with(b"\x1b]5522;type=read:status=OK:loc=primary:pw="));
            // Only on the OK packet.
            assert_eq!(
                1,
                written
                    .windows(b"loc=primary".len())
                    .filter(|w| *w == b"loc=primary")
                    .count()
            );
        }

        pc_reset();
        assert!(handler
            .paste(Paste {
                source: paste_pkg::Source::Clipboard(clipboard::Location::Standard),
                contents: paste_pkg::Contents::Memory(&[clipboard::Content {
                    mime: b"text/plain",
                    data: b"x",
                }]),
                allow_unsafe: false,
            })
            .unwrap());
        let written = pc_written();
        assert!(!written.windows(b"loc=".len()).any(|w| w == b"loc="));

        use crate::stream::Handler as _;
        handler.deinit();
        t.deinit(Global);
    }

    // test "paste: mode 5522 without clipboard_read pastes text"
    #[test]
    fn paste_mode_5522_without_clipboard_read_pastes_text() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        let ptr: *mut Terminal = &mut t;
        pc_reset();

        let mut handler = Handler::init(ptr);
        handler.effects.write_pty = Some(pc_write_pty);
        unsafe { (*ptr).modes.set(modespkg::Mode::KittyPasteEvents, true) };

        assert!(handler
            .paste(Paste {
                source: paste_pkg::Source::default(),
                contents: paste_pkg::Contents::Memory(&[clipboard::Content {
                    mime: b"text/plain",
                    data: b"hello",
                }]),
                allow_unsafe: false,
            })
            .unwrap());
        assert_eq!(b"hello".as_slice(), pc_written().as_slice());
        assert_eq!(0, handler.kitty_clipboard_grants.len());

        use crate::stream::Handler as _;
        handler.deinit();
        t.deinit(Global);
    }

    // test "paste: text source never becomes an event"
    #[test]
    fn paste_text_source_never_becomes_an_event() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        let ptr: *mut Terminal = &mut t;
        pc_reset();

        let mut handler = Handler::init(ptr);
        handler.effects.write_pty = Some(pc_write_pty);
        handler.effects.clipboard_read = Some(pc_clipboard_read);
        unsafe { (*ptr).modes.set(modespkg::Mode::KittyPasteEvents, true) };

        assert!(handler
            .paste(Paste {
                source: paste_pkg::Source::Text,
                contents: paste_pkg::Contents::Memory(&[clipboard::Content {
                    mime: b"text/plain",
                    data: b"committed",
                }]),
                allow_unsafe: false,
            })
            .unwrap());
        assert_eq!(b"committed".as_slice(), pc_written().as_slice());
        assert_eq!(0, handler.kitty_clipboard_grants.len());

        use crate::stream::Handler as _;
        handler.deinit();
        t.deinit(Global);
    }

    // test "paste: mode 5522 without entropy fails and records no grant"
    #[test]
    fn paste_mode_5522_without_entropy_fails_and_records_no_grant() {
        // var t: Terminal = try .init(std.Io.failing, testing.allocator, ...);
        // PORT NOTE: rustty's Terminal stores its Io stand-in as a field (see
        // `Terminal.io`); `std.Io.failing` maps to `sys::io_failing`. The
        // `random_secure` override slot is shared process state, so hold the
        // sys test lock like every entropy-dependent test.
        let _lock = crate::sys::random_secure_test_lock.lock().unwrap();

        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        t.io = crate::sys::io_failing;
        let ptr: *mut Terminal = &mut t;
        pc_reset();

        let mut handler = Handler::init(ptr);
        handler.effects.write_pty = Some(pc_write_pty);
        handler.effects.clipboard_read = Some(pc_clipboard_read);
        unsafe { (*ptr).modes.set(modespkg::Mode::KittyPasteEvents, true) };

        assert_eq!(
            Err(PasteError::EntropyUnavailable),
            handler.paste(Paste {
                source: paste_pkg::Source::default(),
                contents: paste_pkg::Contents::Memory(&[clipboard::Content {
                    mime: b"text/plain",
                    data: b"secret",
                }]),
                allow_unsafe: false,
            })
        );
        assert_eq!(0, pc(|c| c.write_count));
        assert_eq!(0, handler.kitty_clipboard_grants.len());

        // Text pastes need no entropy and still work.
        assert!(handler
            .paste(Paste {
                source: paste_pkg::Source::Text,
                contents: paste_pkg::Contents::Memory(&[clipboard::Content {
                    mime: b"text/plain",
                    data: b"hello",
                }]),
                allow_unsafe: false,
            })
            .unwrap());
        assert_eq!(b"hello".as_slice(), pc_written().as_slice());

        use crate::stream::Handler as _;
        handler.deinit();
        t.deinit(Global);
    }

    // test "paste: mode 5522 DECRQM requires clipboard read effect"
    #[test]
    fn paste_mode_5522_decrqm_requires_clipboard_read_effect() {
        // if (comptime build_options.artifact != .lib) return error.SkipZigTest;
        // PORT NOTE (R8): the reference runs this only in the lib artifact
        // (where the kitty_paste_events mode entry is unconditionally
        // enabled); the same comptime gate is carried here.
        if !matches!(
            crate::options::ARTIFACT,
            crate::build_options::Artifact::Lib
        ) {
            // On non-lib artifacts the mode entry's enablement is
            // platform-dependent; the reference skips.
            return;
        }

        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        let ptr: *mut Terminal = &mut t;
        pc_reset();

        let mut handler = Handler::init(ptr);
        handler.effects.write_pty = Some(pc_write_pty);
        let mut s = Stream::init(stream::Options {
            allocator: Some(rustty_lib::allocator::ZigAllocatorRef::new(
                &crate::terminal::GLOBAL_ZIG_ALLOCATOR,
            )),
            handler,
            continuation_max_bytes: None,
        });

        // A paste event is unusable if its follow-up read can't be served, so
        // the mode isn't advertised without a clipboard read effect.
        s.next_slice(b"\x1B[?5522$p");
        assert_eq!(b"\x1B[?5522;0$y".as_slice(), pc_written().as_slice());

        pc_reset();
        s.next_slice(b"\x1B[?5522h");
        assert!(unsafe { (*ptr).modes.get(modespkg::Mode::KittyPasteEvents) });
        s.next_slice(b"\x1B[?5522$p");
        assert_eq!(b"\x1B[?5522;0$y".as_slice(), pc_written().as_slice());

        // Installing the effect makes the current mode state reportable.
        pc_reset();
        s.handler.effects.clipboard_read = Some(pc_clipboard_read);
        s.next_slice(b"\x1B[?5522$p");
        assert_eq!(b"\x1B[?5522;1$y".as_slice(), pc_written().as_slice());

        pc_reset();
        s.next_slice(b"\x1B[?5522l");
        assert!(!unsafe { (*ptr).modes.get(modespkg::Mode::KittyPasteEvents) });
        s.next_slice(b"\x1B[?5522$p");
        assert_eq!(b"\x1B[?5522;2$y".as_slice(), pc_written().as_slice());

        // Removing the effect stops advertising the capability again.
        pc_reset();
        s.handler.effects.clipboard_read = None;
        s.next_slice(b"\x1B[?5522$p");
        assert_eq!(b"\x1B[?5522;0$y".as_slice(), pc_written().as_slice());

        s.deinit();
        t.deinit(Global);
    }

    // test "full reset drops kitty clipboard grants"
    #[test]
    fn full_reset_drops_kitty_clipboard_grants() {
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        let ptr: *mut Terminal = &mut t;
        pc_reset();

        let mut handler = Handler::init(ptr);
        handler.effects.write_pty = Some(pc_write_pty);
        handler.effects.clipboard_read = Some(pc_clipboard_read);
        let mut s = Stream::init(stream::Options {
            allocator: Some(rustty_lib::allocator::ZigAllocatorRef::new(
                &crate::terminal::GLOBAL_ZIG_ALLOCATOR,
            )),
            handler,
            continuation_max_bytes: None,
        });

        s.handler
            .kitty_clipboard_grants
            .grant(b"pw", kitty::clipboard::Direction::Read, false);
        assert_eq!(1, s.handler.kitty_clipboard_grants.len());

        s.next_slice(b"\x1Bc");
        assert_eq!(0, s.handler.kitty_clipboard_grants.len());

        // A read with the old password is no longer granted, and the
        // handler keeps working (grants can be recorded again).
        pc_reset();
        s.next_slice(b"\x1b]5522;type=read:pw=cHc=:name=YXBw;dGV4dC9wbGFpbg==\x1b\\");
        assert_eq!(1, pc(|c| c.read_count));
        assert!(!pc(|c| c.last_read_granted));
        s.handler
            .kitty_clipboard_grants
            .grant(b"pw", kitty::clipboard::Direction::Read, false);
        assert_eq!(1, s.handler.kitty_clipboard_grants.len());

        s.deinit();
        t.deinit(Global);
    }
}
