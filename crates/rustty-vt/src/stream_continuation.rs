//! rustty-vt stream_continuation
//!
//! PORTS: ghostty src/terminal/stream_continuation.zig
//!
//! PORT STATUS: done (phase-4 A-3) — `ValidateError`/`validate`, `Tracker`,
//! `findVTReplayStart`/`findUtf8ReplayStart`, `BoundaryScanner`, with all 8
//! upstream tests.

// const std = @import("std");
// const assert = @import("../quirks.zig").inlineAssert;
// const Allocator = std.mem.Allocator;
// const Parser = @import("Parser.zig");
// const UTF8Decoder = @import("UTF8Decoder.zig");
use crate::parser::{self, Parser};
use crate::utf8_decoder::UTF8Decoder;
use std::io;

/// `std.simd.suggestVectorLength(u8)` for `findVTReplayStart`'s backward scan.
/// Same width source as `stream.rs`'s `APC_SCAN_LANES` (the reference calls
/// `std.simd.suggestVectorLength` directly).
const ESC_SCAN_LANES: Option<usize> = match rustty_simd::native_vector_bits() {
    Some(bits) => Some(bits / 8),
    None => None,
};

/// Errors possible while validating a snapshot continuation.
// pub const ValidateError = error{ ... };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValidateError {
    /// Nonempty input left both the VT parser and UTF-8 decoder at ground.
    ///
    /// A continuation exists only to reconstruct state that was unfinished at
    /// the snapshot cut. Input that returns to ground is a complete PTY
    /// fragment, not a continuation. Replaying it would repeat work already
    /// represented by the Terminal snapshot. For example, `ESC [ 3 1 m`
    /// completes an SGR command and must not be stored as a continuation.
    NoPendingState,

    /// The input does not begin at its effective replay start.
    ///
    /// A later ESC supersedes earlier VT parser state, and a pending UTF-8
    /// codepoint begins at its lead byte. Any prefix before that effective
    /// start is unnecessary and would disappear when the restored Stream
    /// exports its continuation again. Rejecting the prefix preserves the
    /// byte-identical re-export invariant and prevents unrelated prior input
    /// from being replayed.
    NonCanonicalContinuation,

    /// Replaying the input would perform handler-visible work.
    ///
    /// The Terminal snapshot already contains every mutation committed before
    /// its capture cut. A continuation may rebuild unfinished parser or
    /// builder state, but it must not mutate the Terminal or repeat an external
    /// effect while doing so. For example, BEL inside an unfinished CSI would
    /// ring again even though the CSI itself remains pending.
    ReplayWouldCommit,
}

/// Validate continuation bytes.
///
/// Ensure that:
///
///   - replay ends with either VT or UTF-8 state unfinished
///   - byte zero is the effective start needed to reconstruct that state
///   - replay commits no Terminal mutation or external handler effect
// pub fn validate(bytes: []const u8) ValidateError!void {
pub fn validate(bytes: &[u8]) -> Result<(), ValidateError> {
    // Empty explicitly requests ground state and needs no replay.
    // if (bytes.len == 0) return;
    if bytes.is_empty() {
        return Ok(());
    }

    // We need the final parser and decoder states to classify the input, so a
    // committed byte cannot return early. Remember it while scanning the rest.
    // var scanner: BoundaryScanner = .init();
    let mut scanner = BoundaryScanner::init();
    // var committed_work = false;
    let mut committed_work = false;
    // for (bytes) |byte| {
    //     if (scanner.next(byte) != .uncommitted) committed_work = true;
    // }
    for &byte in bytes {
        if scanner.next(byte) != Effect::Uncommitted {
            committed_work = true;
        }
    }

    // Nonempty continuation bytes must leave state that future input needs.
    // if (scanner.ground()) return error.NoPendingState;
    if scanner.ground() {
        return Err(ValidateError::NoPendingState);
    }

    // The tracker exports the minimal suffix beginning at the effective replay
    // start. A different prefix would not survive byte-identical re-export.
    // const replay_start = if (scanner.parser.state != .ground)
    //     findVTReplayStart(bytes)
    // else
    //     findUtf8ReplayStart(bytes);
    // if (replay_start != 0) return error.NonCanonicalContinuation;
    let replay_start = if scanner.parser.state != parser::State::Ground {
        find_vt_replay_start(bytes)
    } else {
        find_utf8_replay_start(bytes)
    };
    if replay_start != Some(0) {
        return Err(ValidateError::NonCanonicalContinuation);
    }

    // At this point the input is unfinished and minimal, but replay must also
    // be inert with respect to the already-restored Terminal and its effects.
    // if (committed_work) return error.ReplayWouldCommit;
    if committed_work {
        return Err(ValidateError::ReplayWouldCommit);
    }

    Ok(())
}

/// Retains the input needed to reconstruct unfinished Stream parser state.
///
/// A feed is one chunk of bytes given to a Stream. It can end in the middle of
/// a VT sequence or a UTF-8 codepoint. To continue in another Stream, that
/// Stream starts from ground (with no sequence in progress) and reads the
/// saved bytes again. The first saved byte is called the replay start.
///
/// Stream updates this tracker once per feed based on where that replay start
/// is:
///
///   - Ground (parser and UTF-8 decoder): `reset`, no suffix is needed.
///   - Anything unfinished: `append`, which replaces the suffix with the
///     bytes from the replay start onward when that start is inside this
///     feed, and otherwise extends the suffix with this whole feed because
///     the unfinished state began in an earlier one.
///
/// This keeps the retained bytes minimal: they always begin at the replay
/// start, so the feed path never needs to parse or trim old input. The suffix
/// may still contain a byte whose visible terminal effect already happened,
/// such as BEL inside an unfinished CSI sequence. `write` leaves out those
/// bytes so replay does not perform the same effect twice.
///
/// If the suffix exceeds `max_bytes`, or retaining it fails, `broken`
/// is set until a later feed ends at ground (`reset`) or contains a new
/// replay start (`replace`), both of which need nothing that was lost.
///
/// Replay semantics are guaranteed for the standard TerminalStream handler.
/// Custom handlers, including handlers that intercept vtRaw, are unsupported.
// pub const Tracker = struct { ... };
pub struct Tracker {
    // alloc: Allocator,
    // PORT NOTE (parity tail S7-4): the runtime allocator handle; `bytes`
    // grows through it, fallibly (`catch { markBroken }`).
    alloc: crate::alloc::Allocator,
    // max_bytes: usize,
    pub max_bytes: usize,
    // bytes: std.ArrayList(u8) = .empty,
    // (parity tail S7-4b: `alloc::ArrayListUnmanaged`, so `ensureTotalCapacity`
    // and `appendSlice` grow by Zig's `growCapacity`, as the reference's list
    // does. `Drop` frees it through `alloc`.)
    pub bytes: crate::alloc::ArrayListUnmanaged<u8>,
    // broken: bool = false,
    pub broken: bool,
}

impl Tracker {
    // const initial_capacity = 4096;
    const INITIAL_CAPACITY: usize = 4096;

    /// Initialize a tracker.
    // pub fn init(alloc: Allocator, max_bytes: usize) Tracker {
    pub fn init(alloc: crate::alloc::Allocator, max_bytes: usize) -> Tracker {
        let mut result = Tracker {
            alloc,
            max_bytes,
            bytes: crate::alloc::ArrayListUnmanaged::EMPTY,
            broken: false,
        };
        // result.bytes.ensureTotalCapacity(
        //     alloc,
        //     @min(max_bytes, initial_capacity),
        // ) catch {
        //     // We ignore memory errors here. They'll mark the tracker
        //     // as broken in a future append.
        // };
        if result
            .try_reserve(core::cmp::min(max_bytes, Tracker::INITIAL_CAPACITY))
            .is_err()
        {
            // We ignore memory errors here. They'll mark the tracker
            // as broken in a future append.
        }
        result
    }

    // pub fn deinit(self: *Tracker) void {
    //     self.bytes.deinit(self.alloc);
    // }
    pub fn deinit(&mut self) {
        // (Leaves an empty list, so a later drop frees nothing.)
        self.bytes.deinit(self.alloc);
    }

    /// The allocator the tracker grows through (`self.alloc`).
    pub fn allocator(&self) -> crate::alloc::Allocator {
        self.alloc
    }

    /// Clear the current continuation suffix and broken state while keeping
    /// its allocation. Stream calls this after reaching ground because no
    /// earlier input is needed to reconstruct the next unfinished state.
    // pub fn reset(self: *Tracker) void {
    pub fn reset(&mut self) {
        self.broken = false;
        // self.bytes.clearRetainingCapacity();
        self.bytes.clear_retaining_capacity();
    }

    /// Retain the part of `input` needed to replay a feed that ended with
    /// `pending` state unfinished.
    ///
    /// When the input contains the replay start for that state, it replaces
    /// the retained suffix outright because nothing earlier is needed. The
    /// new suffix is complete on its own, so this also repairs a broken
    /// tracker. Otherwise the unfinished state began in an earlier feed and
    /// this whole input extends the current suffix.
    // pub fn append(self: *Tracker, pending: Pending, input: []const u8) void {
    pub fn append(&mut self, pending: Pending, input: &[u8]) {
        // const start = switch (pending) {
        //     .vt => findVTReplayStart(input),
        //     .utf8 => findUtf8ReplayStart(input),
        // } orelse { self.extend(input); return; };
        let start = match match pending {
            Pending::Vt => find_vt_replay_start(input),
            Pending::Utf8 => find_utf8_replay_start(input),
        } {
            Some(start) => start,
            None => {
                self.extend(input);
                return;
            }
        };
        // self.replace(input[start..]);
        self.replace(&input[start..]);
    }

    /// Replace the continuation suffix with one that has a new replay start.
    // fn replace(self: *Tracker, suffix: []const u8) void {
    fn replace(&mut self, suffix: &[u8]) {
        // self.bytes.clearRetainingCapacity();
        self.bytes.clear_retaining_capacity();
        // if (suffix.len > self.max_bytes) { self.markBroken(); return; }
        if suffix.len() > self.max_bytes {
            self.mark_broken();
            return;
        }
        // self.bytes.appendSlice(self.alloc, suffix) catch {
        //     self.markBroken();
        //     return;
        // };
        if self.append_slice(suffix).is_err() {
            self.mark_broken();
            return;
        }
        // self.broken = false;
        self.broken = false;
    }

    /// Extend the continuation suffix with a fragment that contains no replay
    /// start of its own. The stream stayed unfinished for the whole fragment,
    /// so the existing suffix plus this fragment reproduces the current state.
    // fn extend(self: *Tracker, fragment: []const u8) void {
    fn extend(&mut self, fragment: &[u8]) {
        // if (self.broken) return;
        if self.broken {
            return;
        }
        // if (fragment.len > self.max_bytes -| self.bytes.items.len) {
        //     self.markBroken();
        //     return;
        // }
        if fragment.len() > self.max_bytes.saturating_sub(self.bytes.len()) {
            self.mark_broken();
            return;
        }
        // self.bytes.appendSlice(self.alloc, fragment) catch self.markBroken();
        if self.append_slice(fragment).is_err() {
            self.mark_broken();
        }
    }

    /// Write the replay-safe continuation suffix to `writer`. The retained
    /// bytes already begin at the replay start; this omits only bytes that
    /// would repeat committed terminal effects without contributing to the
    /// unfinished parser state (e.g. a BEL inside an unfinished CSI). The
    /// tracker must not be broken.
    // pub fn write(
    //     self: *const Tracker,
    //     writer: *std.Io.Writer,
    // ) std.Io.Writer.Error!void {
    // PORT NOTE: `*std.Io.Writer` ports to `&mut dyn std::io::Write` (the
    // `Screen::dumpString` seam precedent); `error.WriteFailed` is the writer's
    // own `io::Error`.
    pub fn write(&self, writer: &mut dyn io::Write) -> io::Result<()> {
        // var scanner: BoundaryScanner = .init();
        let mut scanner = BoundaryScanner::init();
        // for (self.bytes.items) |c| {
        //     if (scanner.next(c) == .omittable) continue;
        //     try writer.writeByte(c);
        // }
        for &c in self.bytes.iter() {
            if scanner.next(c) == Effect::Omittable {
                continue;
            }
            writer.write_all(&[c])?;
        }
        Ok(())
    }

    // fn markBroken(self: *Tracker) void {
    fn mark_broken(&mut self) {
        self.broken = true;
        // self.bytes.clearRetainingCapacity();
        self.bytes.clear_retaining_capacity();
    }

    /// `self.bytes.appendSlice(self.alloc, ...)`: `ensureUnusedCapacity`
    /// (`ensureTotalCapacity(len + n)`, rounded by `growCapacity`), then the
    /// copy.
    fn append_slice(&mut self, bytes: &[u8]) -> Result<(), crate::alloc::OutOfMemory> {
        let len = self.bytes.len();
        let new_len = len
            .checked_add(bytes.len())
            .ok_or(crate::alloc::OutOfMemory)?;
        self.bytes.ensure_total_capacity(self.alloc, new_len)?;
        // SAFETY: capacity >= new_len; the copied range is initialized
        // before `set_len` exposes it.
        unsafe {
            core::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                self.bytes.as_mut_ptr().add(len),
                bytes.len(),
            );
            self.bytes.set_len(new_len);
        }
        Ok(())
    }

    /// `self.bytes.ensureTotalCapacity(alloc, n)` — the fallible best-effort
    /// reservation done by `init` (rounded up by `growCapacity`).
    fn try_reserve(&mut self, capacity: usize) -> Result<(), crate::alloc::OutOfMemory> {
        self.bytes.ensure_total_capacity(self.alloc, capacity)
    }
}

impl Drop for Tracker {
    fn drop(&mut self) {
        self.bytes.deinit(self.alloc);
    }
}

/// Which state machine is unfinished at the end of a feed. When the VT
/// parser is outside ground the unfinished state is an escape sequence.
/// When it is grounded, only the UTF-8 decoder can be unfinished, with
/// an incomplete codepoint ending the feed.
// pub const Pending = enum { vt, utf8 };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pending {
    Vt,
    Utf8,
}

/// Find where replay must begin when a feed ends inside a VT sequence.
///
/// VT parsers treat ESC specially: it abandons the previous parser state and
/// starts a fresh escape state no matter what was being parsed. Because of
/// that rule, feeding the bytes from the last ESC onward into a grounded
/// Stream recreates the unfinished state at the end of `input`.
///
/// The caller must only use this when the VT parser ended outside ground.
/// The returned value is the index of the last ESC in `input`. A null result
/// means the sequence began in an earlier feed, so this entire input must be
/// appended to the continuation bytes already retained.
// fn findVTReplayStart(input: []const u8) ?usize {
fn find_vt_replay_start(input: &[u8]) -> Option<usize> {
    // const esc: u8 = 0x1B;
    const ESC: u8 = 0x1B;
    // var rem: usize = input.len;
    let mut rem: usize = input.len();

    // Escape sequences are short, so when the stream ends inside one the
    // replay start is usually within the last few bytes. Scan backward in
    // vector chunks; pure payload inputs (no ESC at all) still scan quickly.
    // if (comptime std.simd.suggestVectorLength(u8)) |lanes| { ... }
    if let Some(lanes) = ESC_SCAN_LANES {
        // Process several vectors per iteration so long inputs without ESC
        // (e.g. huge string payloads) stay memory-bound rather than
        // loop-bound. On a match, rescan the group precisely.
        // const unroll = 4;
        const UNROLL: usize = 4;
        // while (rem >= lanes * unroll) { ... }
        while rem >= lanes * UNROLL {
            let start = rem - lanes * UNROLL;
            // var match: @Vector(lanes, bool) = @splat(false);
            // inline for (0..unroll) |block| { ... match = match | (v == needle); }
            let mut matched = false;
            for block in 0..UNROLL {
                for j in 0..lanes {
                    if input[start + block * lanes + j] == ESC {
                        matched = true;
                    }
                }
            }
            // if (@reduce(.Or, match)) { ... }
            if matched {
                // inline for (0..unroll) |i| {
                //     const block = unroll - 1 - i;
                //     ...
                //     if (bits != 0) return start + block * lanes + (lanes - 1 - @clz(bits));
                // }
                for i in 0..UNROLL {
                    let block = UNROLL - 1 - i;
                    // `(lanes - 1 - @clz(bits))` = the highest matching lane.
                    for j in (0..lanes).rev() {
                        if input[start + block * lanes + j] == ESC {
                            return Some(start + block * lanes + j);
                        }
                    }
                }
                // unreachable;
                unreachable!();
            }
            rem = start;
        }

        // while (rem >= lanes) { ... }
        while rem >= lanes {
            let start = rem - lanes;
            // if (bits != 0) return start + (lanes - 1 - @clz(bits));
            for j in (0..lanes).rev() {
                if input[start + j] == ESC {
                    return Some(start + j);
                }
            }
            rem = start;
        }
    }

    // while (rem > 0) {
    //     rem -= 1;
    //     if (input[rem] == esc) return rem;
    // }
    while rem > 0 {
        rem -= 1;
        if input[rem] == ESC {
            return Some(rem);
        }
    }
    // return null;
    None
}

/// Find where replay must begin when a feed ends inside a UTF-8 codepoint.
///
/// A UTF-8 codepoint starts with a lead byte and may have up to three
/// continuation bytes. If the lead byte is in this input, replay must begin
/// there so the decoder sees the complete partial codepoint again. Since an
/// incomplete codepoint can contain at most three bytes, only the final three
/// input bytes need to be searched.
///
/// The caller must only use this when the VT parser is at ground and the
/// UTF-8 decoder ended mid-codepoint. The returned value is the lead byte's
/// index. A null result means the lead byte was in an earlier feed, so this
/// entire input must be appended to the continuation bytes already retained.
// fn findUtf8ReplayStart(input: []const u8) ?usize {
fn find_utf8_replay_start(input: &[u8]) -> Option<usize> {
    // const max_pending = 3;
    const MAX_PENDING: usize = 3;
    // var idx = input.len;
    let mut idx = input.len();
    // while (idx > 0 and input.len - idx < max_pending) {
    //     idx -= 1;
    //     if (input[idx] >= 0xC0) return idx;
    // }
    while idx > 0 && input.len() - idx < MAX_PENDING {
        idx -= 1;
        if input[idx] >= 0xC0 {
            return Some(idx);
        }
    }
    // return null;
    None
}

/// Classifies bytes of a continuation suffix for export.
///
/// The retained suffix reconstructs unfinished state exactly, but it can
/// contain bytes whose handler-visible work already committed the first
/// time (e.g. a C0 control executed inside an unfinished CSI sequence).
/// This implements a minimal VT stream processor (to avoid circular imports
/// with stream.zig) so `Tracker.write` can omit those bytes and replay
/// never repeats a terminal effect. It only runs at export time, never on
/// the feed path.
///
/// This is allocation-free.
// const BoundaryScanner = struct { ... };
struct BoundaryScanner {
    parser: Parser,
    utf8decoder: UTF8Decoder,
}

/// How one byte affects committed work and the continuation suffix.
// const Effect = enum { ... };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Effect {
    /// The byte commits no handler-visible work. It must remain because it
    /// may still build the unfinished VT, UTF-8, APC, or DCS state.
    Uncommitted,

    /// The byte commits handler-visible work but also changes a parser
    /// state tag or reaches ground. It cannot be omitted in isolation;
    /// boundary analysis decides whether the surrounding prefix is safe.
    Committed,

    /// The byte commits handler-visible work without changing either
    /// state-machine tag or reaching ground. It can be omitted without
    /// changing the unfinished state, avoiding a duplicate effect.
    Omittable,
}

impl BoundaryScanner {
    /// Initialize both state machines at runtime. Parser initialization may
    /// inspect the runtime environment, so it cannot be a struct field default
    /// that Zig requires to be comptime-known.
    // fn init() BoundaryScanner {
    fn init() -> BoundaryScanner {
        BoundaryScanner {
            parser: Parser::init(),
            utf8decoder: UTF8Decoder::new(),
        }
    }

    /// True only when neither state machine needs prior bytes to continue.
    // fn ground(self: *const BoundaryScanner) bool {
    fn ground(&self) -> bool {
        self.parser.state == parser::State::Ground && self.utf8decoder.state == 0
    }

    /// Consume one byte using the same scalar UTF-8 retry and VT transitions
    /// as Stream, then classify its effect on continuation construction.
    // fn next(self: *BoundaryScanner, c: u8) Effect {
    fn next(&mut self, c: u8) -> Effect {
        // Preserve the state tags so we can recognize committed controls that
        // do not contribute to the unfinished sequence.
        // const parser_state = self.parser.state;
        // const utf8_state = self.utf8decoder.state;
        let parser_state = self.parser.state;
        let utf8_state = self.utf8decoder.state;

        // A byte is committed when replaying it would repeat handler-visible
        // work already captured outside the continuation suffix. Actions that
        // only build unfinished APC or DCS state are not committed because
        // replay must reconstruct that state.
        // const committed = committed: { ... };
        let committed = 'committed: {
            if self.parser.state == parser::State::Ground {
                // Match Stream's scalar UTF-8 path, including retrying a byte that
                // follows a malformed sequence.
                // var committed = false;
                let mut committed = false;
                // const res = self.utf8decoder.next(c);
                let res = self.utf8decoder.next(c);
                // if (res[0]) |cp| committed = self.codepoint(cp) or committed;
                if let Some(cp) = res.0 {
                    committed = self.codepoint(cp) || committed;
                }
                // if (!res[1]) {
                //     const retry = self.utf8decoder.next(c);
                //     assert(retry[1]);
                //     if (retry[0]) |cp| committed = self.codepoint(cp) or committed;
                // }
                if !res.1 {
                    let retry = self.utf8decoder.next(c);
                    debug_assert!(retry.1);
                    if let Some(cp) = retry.0 {
                        committed = self.codepoint(cp) || committed;
                    }
                }

                break 'committed committed;
            }

            // const actions = self.parser.next(c);
            let actions = self.parser.next(c);
            // for (actions) |action_opt| {
            //     const action = action_opt orelse continue;
            //     switch (action) {
            //         .dcs_hook, .dcs_put, .apc_start, .apc_put => {},
            //         else => break :committed true,
            //     }
            // }
            for action_opt in actions {
                let action = match action_opt {
                    Some(a) => a,
                    None => continue,
                };
                match action {
                    // These actions only build standard handler state. Their
                    // matching end/unhook actions are committed work.
                    parser::Action::DcsHook(_)
                    | parser::Action::DcsPut(_)
                    | parser::Action::ApcStart
                    | parser::Action::ApcPut(_) => {}
                    _ => break 'committed true,
                }
            }

            break 'committed false;
        };

        // if (!committed) return .uncommitted;
        if !committed {
            return Effect::Uncommitted;
        }

        // A committed byte is independently omittable only while the stream
        // remains unfinished and neither state-machine tag changes.
        // const state_changed = parser_state != self.parser.state or
        //     utf8_state != self.utf8decoder.state;
        let state_changed =
            parser_state != self.parser.state || utf8_state != self.utf8decoder.state;
        // return if (!state_changed and !self.ground()) .omittable else .committed;
        if !state_changed && !self.ground() {
            Effect::Omittable
        } else {
            Effect::Committed
        }
    }

    /// Apply the Stream.handleCodepoint behavior relevant to replay. Returns
    /// whether the accepted codepoint would commit handler-visible work.
    // fn codepoint(self: *BoundaryScanner, cp: u21) bool {
    fn codepoint(&mut self, cp: u32) -> bool {
        // if (cp == 0x1B) {
        //     self.parser.state = .escape;
        //     self.parser.clear();
        //     return false;
        // }
        if cp == 0x1B {
            self.parser.state = parser::State::Escape;
            self.parser.clear();
            return false;
        }

        // Match Stream.handleCodepoint: all other accepted codepoints have
        // already caused a handler-visible action (including supported C0s).
        true
    }
}

/// `std.Io.Writer.fixed(&buf)` for the byte-writer seam: an
/// `io::Write` over a fixed-size buffer that fails (like
/// `error.WriteFailed`) when the buffer would overflow. Test fixture shared
/// with `stream.rs`'s continuation tests.
#[cfg(test)]
pub(crate) struct FixedIoWriter<'a> {
    buf: &'a mut [u8],
    /// Number of bytes written so far. (Zig: `std.Io.Writer.end`.)
    pub end: usize,
}

#[cfg(test)]
impl<'a> FixedIoWriter<'a> {
    pub fn new(buf: &'a mut [u8]) -> FixedIoWriter<'a> {
        FixedIoWriter { buf, end: 0 }
    }

    /// `writer.buffered()`
    pub fn buffered(&self) -> &[u8] {
        &self.buf[..self.end]
    }
}

#[cfg(test)]
impl io::Write for FixedIoWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let next = match self.end.checked_add(bytes.len()) {
            Some(v) => v,
            None => return Err(io::Error::new(io::ErrorKind::WriteZero, "WriteFailed")),
        };
        if next > self.buf.len() {
            return Err(io::Error::new(io::ErrorKind::WriteZero, "WriteFailed"));
        }
        self.buf[self.end..next].copy_from_slice(bytes);
        self.end = next;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alloc::testing::{with_handle, Config, CountingAllocator, FailingAllocator};
    use crate::alloc::Global;

    // test "boundary scanner classifies effects and ground"
    #[test]
    fn boundary_scanner_classifies_effects_and_ground() {
        let mut scanner = BoundaryScanner::init();

        assert!(scanner.ground());
        assert_eq!(Effect::Committed, scanner.next(b'A'));
        assert!(scanner.ground());

        assert_eq!(Effect::Uncommitted, scanner.next(0x1B));
        assert!(!scanner.ground());
        assert_eq!(Effect::Uncommitted, scanner.next(b'['));
        assert_eq!(Effect::Uncommitted, scanner.next(b'1'));

        // BEL commits an execute action without changing the unfinished CSI.
        assert_eq!(Effect::Omittable, scanner.next(0x07));
        assert!(!scanner.ground());

        assert_eq!(Effect::Committed, scanner.next(b'm'));
        assert!(scanner.ground());
    }

    // test "boundary scanner preserves unfinished builder actions"
    #[test]
    fn boundary_scanner_preserves_unfinished_builder_actions() {
        let mut apc = BoundaryScanner::init();
        assert_eq!(Effect::Uncommitted, apc.next(0x1B));
        assert_eq!(Effect::Uncommitted, apc.next(b'_'));
        assert_eq!(Effect::Uncommitted, apc.next(b'G'));
        assert_eq!(Effect::Committed, apc.next(0x1B));

        let mut dcs = BoundaryScanner::init();
        assert_eq!(Effect::Uncommitted, dcs.next(0x1B));
        assert_eq!(Effect::Uncommitted, dcs.next(b'P'));
        assert_eq!(Effect::Uncommitted, dcs.next(b'q'));
        assert_eq!(Effect::Uncommitted, dcs.next(b'x'));
        assert_eq!(Effect::Committed, dcs.next(0x1B));
    }

    // test "boundary scanner handles UTF-8 and malformed retries"
    #[test]
    fn boundary_scanner_handles_utf8_and_malformed_retries() {
        let mut valid = BoundaryScanner::init();
        assert_eq!(Effect::Uncommitted, valid.next(0xF0));
        assert!(!valid.ground());
        assert_eq!(Effect::Uncommitted, valid.next(0x9F));
        assert_eq!(Effect::Uncommitted, valid.next(0x98));
        assert_eq!(Effect::Committed, valid.next(0x84));
        assert!(valid.ground());

        let mut malformed = BoundaryScanner::init();
        assert_eq!(Effect::Uncommitted, malformed.next(0xE0));
        assert_eq!(Effect::Uncommitted, malformed.next(0xA0));
        assert!(malformed.next(0xF0) != Effect::Uncommitted);
        assert!(!malformed.ground());
    }

    // test "findVTReplayStart finds the latest ESC across vector boundaries"
    #[test]
    fn find_vt_replay_start_finds_the_latest_esc_across_vector_boundaries() {
        assert_eq!(None, find_vt_replay_start(b""));
        assert_eq!(None, find_vt_replay_start(b"no escapes here"));
        assert_eq!(Some(0), find_vt_replay_start(b"\x1b"));
        assert_eq!(Some(5), find_vt_replay_start(b"\x1b[1m\x20\x1b[2"));

        // Exercise both the vector loop and the scalar remainder with replay
        // starts at every position of a buffer larger than any vector width.
        let mut buf = [0u8; 193];
        buf.fill(b'a');
        assert_eq!(None, find_vt_replay_start(&buf));
        for idx in 0..buf.len() {
            buf.fill(b'a');
            buf[idx] = 0x1B;
            assert_eq!(Some(idx), find_vt_replay_start(&buf));

            // The latest ESC wins.
            if idx > 0 {
                buf[idx - 1] = 0x1B;
                assert_eq!(Some(idx), find_vt_replay_start(&buf));
            }
        }
    }

    // test "findUtf8ReplayStart finds the pending lead byte"
    #[test]
    fn find_utf8_replay_start_finds_the_pending_lead_byte() {
        assert_eq!(None, find_utf8_replay_start(b""));
        assert_eq!(Some(4), find_utf8_replay_start(b"text\xF0"));
        assert_eq!(Some(4), find_utf8_replay_start(b"text\xF0\x9F"));
        assert_eq!(Some(4), find_utf8_replay_start(b"text\xF0\x9F\x98"));
        assert_eq!(Some(0), find_utf8_replay_start(b"\xE0\xA0"));

        // A malformed prefix rejected earlier doesn't hide the pending lead.
        assert_eq!(Some(2), find_utf8_replay_start(b"\xE0\xA0\xF0"));

        // Continuation bytes of a sequence that started in an earlier input have
        // no replay start of their own.
        assert_eq!(None, find_utf8_replay_start(b"\x9F"));
        assert_eq!(None, find_utf8_replay_start(b"\x9F\x98"));
    }

    // test "tracker retains and normalizes replay-safe bytes"
    #[test]
    fn tracker_retains_and_normalizes_replay_safe_bytes() {
        let mut tracker = Tracker::init(Global, 64);
        // defer tracker.deinit();

        tracker.append(Pending::Vt, b"committed\x1b[1\x07");
        tracker.append(Pending::Vt, b";2");
        assert!(!tracker.broken);

        let mut buf = [0u8; 64];
        let mut writer = FixedIoWriter::new(&mut buf);
        tracker.write(&mut writer).unwrap();
        assert_eq!(b"\x1b[1;2".as_slice(), writer.buffered());

        // A feed with a later replay start drops the previous suffix entirely.
        tracker.append(Pending::Vt, b"committed\x1b]0;t");
        let mut replaced_buf = [0u8; 64];
        let mut replaced_writer = FixedIoWriter::new(&mut replaced_buf);
        tracker.write(&mut replaced_writer).unwrap();
        assert_eq!(b"\x1b]0;t".as_slice(), replaced_writer.buffered());

        // An incomplete UTF-8 codepoint seeds at its lead byte and grows with
        // continuation bytes from later feeds.
        tracker.append(Pending::Utf8, b"committed\xF0");
        tracker.append(Pending::Utf8, b"\x9F");
        let mut utf8_buf = [0u8; 4];
        let mut utf8_writer = FixedIoWriter::new(&mut utf8_buf);
        tracker.write(&mut utf8_writer).unwrap();
        assert_eq!(b"\xF0\x9F".as_slice(), utf8_writer.buffered());

        tracker.deinit();
    }

    // test "tracker cap, reset, and broken recovery"
    #[test]
    fn tracker_cap_reset_and_broken_recovery() {
        let mut tracker = Tracker::init(Global, 4);
        // defer tracker.deinit();

        tracker.append(Pending::Vt, b"\x1b[123");
        assert!(tracker.broken);

        // Feeds without a replay start are dropped while broken.
        tracker.append(Pending::Vt, b"4");
        assert!(tracker.broken);
        assert_eq!(0, tracker.bytes.len());

        // Suffixes that grow past the cap break tracking.
        tracker.reset();
        tracker.append(Pending::Vt, b"\x1b[1");
        tracker.append(Pending::Vt, b"23");
        assert!(tracker.broken);

        // A new replay start that fits recovers without a reset.
        tracker.append(Pending::Vt, b"\x1b[");
        assert!(!tracker.broken);
        let mut buf = [0u8; 4];
        let mut writer = FixedIoWriter::new(&mut buf);
        tracker.write(&mut writer).unwrap();
        assert_eq!(b"\x1b[".as_slice(), writer.buffered());

        tracker.append(Pending::Vt, b"\x1b[123");
        assert!(tracker.broken);
        tracker.reset();
        assert!(!tracker.broken);
        let mut empty_buf = [0u8; 1];
        let mut empty_writer = FixedIoWriter::new(&mut empty_buf);
        tracker.write(&mut empty_writer).unwrap();
        assert_eq!(0, empty_writer.end);

        tracker.deinit();
    }

    // test "tracker reports writer failure and defers allocation failure"
    #[test]
    fn tracker_reports_writer_failure_and_defers_allocation_failure() {
        let mut tracker = Tracker::init(Global, 64);
        // defer tracker.deinit();
        tracker.append(Pending::Vt, b"\x1b[");

        let mut buf = [0u8; 1];
        let mut writer = FixedIoWriter::new(&mut buf);
        // try testing.expectError(error.WriteFailed, tracker.write(&writer));
        assert!(tracker.write(&mut writer).is_err());

        // var failing = testing.FailingAllocator.init(testing.allocator, .{
        //     .fail_index = 0,
        // });
        let failing = FailingAllocator::over_heap(Config {
            fail_index: 0,
            ..Config::default()
        });
        let c = failing.allocator();
        with_handle(&c, |alloc| {
            // var failing_tracker = Tracker.init(failing.allocator(), 64);
            let mut failing_tracker = Tracker::init(alloc, 64);
            // defer failing_tracker.deinit();
            assert!(!failing_tracker.broken);

            // The best-effort initial reservation failed, so the first required
            // retention retries allocation and marks the tracker broken.
            failing_tracker.append(Pending::Vt, b"\x1b[");
            assert!(failing_tracker.broken);
            failing_tracker.deinit();
        });

        tracker.deinit();
    }

    // Parity tail S7-4 (routing evidence, no reference counterpart): the
    // retained suffix is allocated through the tracker's allocator: `init`'s
    // best-effort reservation of `min(max_bytes, 4096)` bytes, then growth past
    // it. `deinit` returns every byte.
    #[test]
    fn tracker_routes_through_the_allocator_handle() {
        let counting = CountingAllocator::over_heap();
        let c = counting.allocator();
        with_handle(&c, |alloc| {
            let mut tracker = Tracker::init(alloc, 16384);
            // `ensureTotalCapacity(4096)` asks for `growCapacity(4096)`.
            let first = crate::alloc::ArrayListUnmanaged::<u8>::grow_capacity(4096);
            #[cfg(any(target_arch = "aarch64", target_arch = "x86_64"))]
            assert_eq!(6272, first, "4096 + 2048 + cache_line (128)");
            assert_eq!(1, counting.alloc_calls.get(), "the initial reservation");
            assert_eq!(Some((first, 0)), counting.last_alloc.get());
            // Past the reservation the suffix grows by `growCapacity` too.
            let mut input = vec![b'1'; 7000];
            input[0..2].copy_from_slice(b"\x1b[");
            tracker.append(Pending::Vt, &input);
            assert!(!tracker.broken);
            assert_eq!(7000, tracker.bytes.len());
            let grown = crate::alloc::ArrayListUnmanaged::<u8>::grow_capacity(7000);
            assert_eq!(grown, tracker.bytes.capacity());
            assert_eq!(grown, counting.live_bytes.get(), "the grown suffix");
            tracker.deinit();
            assert_eq!(0, counting.live_bytes.get(), "every byte is returned");
        });
    }
}
