//! Kitty's drag and drop protocol (OSC 72)
//!
//! This only captures the raw metadata and payload for the OSC. The
//! actual protocol grammar (metadata keys, event types, chunking) is
//! implemented in `kitty/dnd.rs` and its submodules, since the
//! protocol requires stateful handling that doesn't belong in the
//! stateless OSC parser.
//!
//! Specification: https://sw.kovidgoyal.net/kitty/dnd-protocol/
//!
//! PORTS: ghostty src/terminal/osc/parsers/kitty_dnd_protocol.zig
//!
//! PORT STATUS: ported (phase-5 B6 end state at 683d8db64: 50f69b883 shrinks
//! this file 363→138 lines — the pin-era in-parser `readOption`/`Option_`/
//! `EventType` state machinery moved to `kitty/dnd_command.rs`, and 6 of the
//! pin-era 11 tests migrated there as `Metadata` tests).

// const std = @import("std");
// PORT NOTE: std facilities are used inline.
// PORT NOTE: `const assert = @import("../../../quirks.zig").inlineAssert;` —
// retargeted to `debug_assert!` at the call site (matching the convention used
// by `kitty_clipboard_protocol.rs`).
use crate::osc::{Command, Parser, State, Terminator};

// PORT NOTE (parity tail S7-7, N263): `metadata` and `payload` borrow the
// parser's capture, as the reference's slices do; they were owned `Vec<u8>`
// copies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OSC<'p> {
    /// The raw metadata that was received. Parse with
    /// `kitty::dnd::Metadata::parse`.
    ///
    /// PORT NOTE (fidelity): reference `metadata: []const u8` — RAW bytes.
    pub metadata: &'p [u8],

    /// The raw payload. Its meaning and encoding depend on the event
    /// type (`t` metadata key). Null when the OSC had no `;` after the
    /// metadata; an empty payload is distinct from no payload.
    ///
    /// PORT NOTE (fidelity): reference `payload: ?[]const u8` — optional RAW bytes.
    pub payload: Option<&'p [u8]>,

    /// The terminator used for this OSC, so any response can match it.
    pub terminator: Terminator,
}

// pub const C = void;
// pub fn cval(_: OSC) C {}
// PORT NOTE: "We don't currently support encoding this to C in any way." —
// the void C encoding is handled at the Command cval dispatch (osc/mod.rs).

pub fn parse<'a>(parser: &'a mut Parser, terminator_ch: Option<u8>) -> Option<Command<'a>> {
    // assert(parser.state == .@"72");
    debug_assert!(parser.state == State::S72);

    // const cap = if (parser.capture) |*c| c else { ... };
    let data: &'a [u8] = {
        let cap = if let Some(c) = parser.capture.as_ref() {
            c
        } else {
            parser.state = State::Invalid;
            return None;
        };
        // const data = cap.trailing();
        cap.trailing(&parser.buffer)
    };

    let (metadata, payload): (&'a [u8], Option<&'a [u8]>) = 'result: {
        // const sep = std.mem.indexOfScalar(u8, data, ';')
        //     orelse break :result .{ data, null };
        let sep = match data.iter().position(|&b| b == b';') {
            Some(sep) => sep,
            None => break 'result (data, None),
        };
        // break :result .{ data[0..sep], data[sep + 1 .. data.len] };
        (&data[0..sep], Some(&data[sep + 1..data.len()]))
    };

    // PORT NOTE (fidelity): `metadata`/`payload` are `[]const u8` / `?[]const u8`
    // RAW bytes, borrowed from the capture (parity tail S7-7) — NEVER a lossy
    // String decode.

    // parser.command = .{ .kitty_dnd_protocol = .{ ... } };
    let command = Command::KittyDndProtocol(OSC {
        metadata,
        payload,
        terminator: Terminator::init(terminator_ch),
    });

    Some(command)
}

#[cfg(test)]
mod tests {
    use crate::osc::{Command, Parser, Terminator};

    // PORT NOTE: the reference uses `testing.allocator`. The ported `OSC`
    // payload is self-owning (`Vec<u8>`s), so the parser needs no allocator for
    // OSC 72; `Parser::init(None)` is the faithful equivalent.

    #[test]
    fn osc_72_metadata_only_no_payload() {
        let mut p: Parser = Parser::init(None);
        let input = "72;t=a";
        for ch in input.bytes() {
            p.next(ch);
        }
        let cmd = p.end(Some(0x1b)).unwrap();
        let dnd = match cmd {
            Command::KittyDndProtocol(dnd) => dnd,
            _ => unreachable!(),
        };
        assert_eq!(b"t=a".as_slice(), dnd.metadata);
        assert!(dnd.payload.is_none());
    }

    #[test]
    fn osc_72_metadata_and_empty_payload() {
        let mut p: Parser = Parser::init(None);
        let input = "72;t=a;";
        for ch in input.bytes() {
            p.next(ch);
        }
        let cmd = p.end(Some(0x1b)).unwrap();
        let dnd = match cmd {
            Command::KittyDndProtocol(dnd) => dnd,
            _ => unreachable!(),
        };
        assert_eq!(b"t=a".as_slice(), dnd.metadata);
        assert_eq!(b"".as_slice(), dnd.payload.unwrap());
    }

    #[test]
    fn osc_72_metadata_and_non_empty_payload() {
        let mut p: Parser = Parser::init(None);
        let input = "72;t=a:i=5;text/plain text/uri-list";
        for ch in input.bytes() {
            p.next(ch);
        }
        let cmd = p.end(Some(0x1b)).unwrap();
        let dnd = match cmd {
            Command::KittyDndProtocol(dnd) => dnd,
            _ => unreachable!(),
        };
        assert_eq!(b"t=a:i=5".as_slice(), dnd.metadata);
        assert_eq!(b"text/plain text/uri-list".as_slice(), dnd.payload.unwrap());
    }

    #[test]
    fn osc_72_empty_metadata_with_payload() {
        let mut p: Parser = Parser::init(None);
        let input = "72;;payload";
        for ch in input.bytes() {
            p.next(ch);
        }
        let cmd = p.end(Some(0x1b)).unwrap();
        let dnd = match cmd {
            Command::KittyDndProtocol(dnd) => dnd,
            _ => unreachable!(),
        };
        assert_eq!(b"".as_slice(), dnd.metadata);
        assert_eq!(b"payload".as_slice(), dnd.payload.unwrap());
    }

    #[test]
    fn osc_72_bel_terminator_recorded() {
        let mut p: Parser = Parser::init(None);
        let input = "72;t=q";
        for ch in input.bytes() {
            p.next(ch);
        }
        let cmd = p.end(Some(0x07)).unwrap();
        let dnd = match cmd {
            Command::KittyDndProtocol(dnd) => dnd,
            _ => unreachable!(),
        };
        assert_eq!(Terminator::Bel, dnd.terminator);
    }
}
