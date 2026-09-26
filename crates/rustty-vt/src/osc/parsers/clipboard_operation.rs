//! PORTS: ghostty src/terminal/osc/parsers/clipboard_operation.zig
//!
//! PORT STATUS: ported — full parse logic + tests (vt-6; e03475c0c terminator
//! capture for OSC 52 read responses).

use crate::osc::{Command, Parser, Terminator, ZStr};

/// Parse OSC 52
// pub fn parse(parser: *Parser, terminator_ch: ?u8) ?*Command {
pub fn parse<'a>(parser: &'a mut Parser, terminator_ch: Option<u8>) -> Option<Command<'a>> {
    // assert(parser.state == .@"52");
    debug_assert!(parser.state == crate::osc::State::S52);
    let cap = if let Some(c) = parser.capture.as_mut() {
        c
    } else {
        parser.state = crate::osc::State::Invalid;
        return None;
    };
    if !cap.write_byte(&mut parser.buffer, 0) {
        parser.state = crate::osc::State::Invalid;
        return None;
    }
    let data = cap.trailing(&parser.buffer);
    if data.len() == 1 {
        parser.state = crate::osc::State::Invalid;
        return None;
    }
    let command;
    if data[0] == b';' {
        command = Command::ClipboardContents {
            kind: b'c',
            // data[1 .. data.len() - 1 :0]
            // PORT NOTE (fidelity): reference `[:0]const u8` RAW bytes.
            data: ZStr::sentinel(data, 1, data.len() - 1),
            // .terminator = .init(terminator_ch),
            terminator: Terminator::init(terminator_ch),
        };
    } else {
        if data.len() < 2 {
            parser.state = crate::osc::State::Invalid;
            return None;
        }
        if data[1] != b';' {
            parser.state = crate::osc::State::Invalid;
            return None;
        }
        command = Command::ClipboardContents {
            kind: data[0],
            // data[2 .. data.len() - 1 :0]
            // PORT NOTE (fidelity): reference `[:0]const u8` RAW bytes.
            data: ZStr::sentinel(data, 2, data.len() - 1),
            // .terminator = .init(terminator_ch),
            terminator: Terminator::init(terminator_ch),
        };
    }
    Some(command)
}

#[cfg(test)]
mod tests {
    use crate::osc::{Parser, Terminator};

    #[test]
    fn osc_52_get_set_clipboard() {
        // var p: Parser = .init(null);
        //
        // CORRECTION (task 19 fix round 1): task 19's S194-F2 commit
        // (`3eb3763e`) also switched THIS test to `Parser::init(Some(alloc))`
        // by an unbounded text replacement — the reference's "OSC 52: get/set
        // clipboard" is `.init(null)`. That made the two tests identical again,
        // the other way round. This one exercises the FIXED capture backing.
        let mut p: Parser = Parser::init(None);

        let input = "52;s;?";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap();
        assert!(matches!(cmd, crate::osc::Command::ClipboardContents { .. }));
        match cmd {
            crate::osc::Command::ClipboardContents {
                kind,
                data,
                terminator,
            } => {
                assert!(kind == b's');
                assert_eq!(b"?", data.as_slice());
                // try testing.expectEqual(.st, cmd.clipboard_contents.terminator);
                assert_eq!(Terminator::St, terminator);
            }
            _ => unreachable!(),
        }
    }

    // test "OSC 52: get clipboard with BEL terminator"
    #[test]
    fn osc_52_get_clipboard_with_bel_terminator() {
        let mut p: Parser = Parser::init(None);

        let input = "52;c;?";
        for ch in input.bytes() {
            p.next(ch);
        }

        // const cmd = p.end(0x07).?.*;
        let cmd = p.end(Some(0x07)).unwrap();
        assert!(matches!(cmd, crate::osc::Command::ClipboardContents { .. }));
        match cmd {
            crate::osc::Command::ClipboardContents { terminator, .. } => {
                // try testing.expectEqual(.bel, cmd.clipboard_contents.terminator);
                assert_eq!(Terminator::Bel, terminator);
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_52_get_set_clipboard_optional_parameter() {
        let mut p: Parser = Parser::init(None);

        let input = "52;;?";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap();
        assert!(matches!(cmd, crate::osc::Command::ClipboardContents { .. }));
        match cmd {
            crate::osc::Command::ClipboardContents { kind, data, .. } => {
                assert!(kind == b'c');
                assert_eq!(b"?", data.as_slice());
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_52_get_set_clipboard_with_allocator() {
        // PORT NOTE: `var p: Parser = .init(testing.allocator)`. OSC 52 asks for
        // `captureTrailing(.allocating)`, so this test exists to exercise the
        // ALLOCATING backing.
        //
        // FIX (S194-F2): this used to be `Parser::init(None)`, and
        // `capture_trailing(Allocating)` falls back to `Fixed` when
        // `self.alloc.is_none()` — so the test was byte-for-byte the
        // non-allocator test above it and covered none of the path it names.
        // The PORT NOTE that stood here said "no Zig allocator is threaded for
        // the test (the captured bytes are identical either way)"; the bytes
        // are, the code path is not.
        let alloc = crate::alloc::Global;
        let mut p: Parser = Parser::init(Some(alloc));

        let input = "52;s;?";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap();
        assert!(matches!(cmd, crate::osc::Command::ClipboardContents { .. }));
        match cmd {
            crate::osc::Command::ClipboardContents { kind, data, .. } => {
                assert!(kind == b's');
                assert_eq!(b"?", data.as_slice());
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_52_clear_clipboard() {
        let mut p: Parser = Parser::init(None);

        let input = "52;;";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap();
        assert!(matches!(cmd, crate::osc::Command::ClipboardContents { .. }));
        match cmd {
            crate::osc::Command::ClipboardContents { kind, data, .. } => {
                assert!(kind == b'c');
                assert_eq!(b"", data.as_slice());
            }
            _ => unreachable!(),
        }
    }
}
