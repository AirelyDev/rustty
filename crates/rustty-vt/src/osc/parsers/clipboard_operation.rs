//! PORTS: ghostty src/terminal/osc/parsers/clipboard_operation.zig
//!
//! PORT STATUS: ported — full parse logic + tests (vt-6).

use crate::osc::{Command, Parser};

/// Parse OSC 52
pub fn parse<'a>(parser: &'a mut Parser, _: Option<u8>) -> Option<&'a mut Command> {
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
    if data[0] == b';' {
        parser.command = Command::ClipboardContents {
            kind: b'c',
            // data[1 .. data.len() - 1 :0]
            // PORT NOTE (fidelity): reference `[:0]const u8` RAW bytes.
            data: data[1..data.len() - 1].to_vec(),
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
        parser.command = Command::ClipboardContents {
            kind: data[0],
            // data[2 .. data.len() - 1 :0]
            // PORT NOTE (fidelity): reference `[:0]const u8` RAW bytes.
            data: data[2..data.len() - 1].to_vec(),
        };
    }
    Some(&mut parser.command)
}

#[cfg(test)]
mod tests {
    use crate::osc::Parser;

    #[test]
    fn osc_52_get_set_clipboard() {
        let mut p: Parser = Parser::init(None);

        let input = "52;s;?";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, crate::osc::Command::ClipboardContents { .. }));
        match cmd {
            crate::osc::Command::ClipboardContents { kind, data } => {
                assert!(kind == b's');
                assert_eq!(b"?", data.as_slice());
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

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, crate::osc::Command::ClipboardContents { .. }));
        match cmd {
            crate::osc::Command::ClipboardContents { kind, data } => {
                assert!(kind == b'c');
                assert_eq!(b"?", data.as_slice());
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_52_get_set_clipboard_with_allocator() {
        // PORT NOTE: `var p: Parser = .init(testing.allocator)` — OSC 52 captures
        // via the allocating backing when an allocator is present. The Rustty
        // `Capture` allocating backing is a growable `Vec`; no Zig allocator is
        // threaded for the test (the captured bytes are identical either way).
        let mut p: Parser = Parser::init(None);

        let input = "52;s;?";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, crate::osc::Command::ClipboardContents { .. }));
        match cmd {
            crate::osc::Command::ClipboardContents { kind, data } => {
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

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, crate::osc::Command::ClipboardContents { .. }));
        match cmd {
            crate::osc::Command::ClipboardContents { kind, data } => {
                assert!(kind == b'c');
                assert_eq!(b"", data.as_slice());
            }
            _ => unreachable!(),
        }
    }
}
