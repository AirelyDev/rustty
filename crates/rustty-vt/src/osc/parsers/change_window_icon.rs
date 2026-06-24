//! PORTS: ghostty src/terminal/osc/parsers/change_window_icon.zig
//!
//! PORT STATUS: ported — full parse logic + tests (vt-6).

use crate::osc::{Command, Parser};

/// Parse OSC 1
pub fn parse<'a>(parser: &'a mut Parser, _: Option<u8>) -> Option<&'a mut Command> {
    let cap = if let Some(c) = parser.capture.as_mut() {
        c
    } else {
        parser.state = crate::osc::State::Invalid;
        return None;
    };
    if !cap.write_byte(0) {
        parser.state = crate::osc::State::Invalid;
        return None;
    }
    let data = cap.trailing();
    parser.command = Command::ChangeWindowIcon(
        // data[0 .. data.len() - 1 :0]
        // PORT NOTE (fidelity): reference `[:0]const u8` RAW bytes (trailing NUL
        // excluded), NOT a lossy String decode.
        data[0..data.len() - 1].to_vec(),
    );
    Some(&mut parser.command)
}

#[cfg(test)]
mod tests {
    use crate::osc::Parser;

    #[test]
    fn osc_1_change_window_icon() {
        let mut p: Parser = Parser::init(None);
        p.next(b'1');
        p.next(b';');
        p.next(b'a');
        p.next(b'b');
        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, crate::osc::Command::ChangeWindowIcon(_)));
        match cmd {
            crate::osc::Command::ChangeWindowIcon(v) => assert_eq!(b"ab", v.as_slice()),
            _ => unreachable!(),
        }
    }
}
