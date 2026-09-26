//! PORTS: ghostty src/terminal/osc/parsers/report_pwd.zig
//!
//! PORT STATUS: ported — full parse logic + tests (vt-6).

use crate::osc::{Command, Parser, ZStr};

/// Parse OSC 7
pub fn parse<'a>(parser: &'a mut Parser, _: Option<u8>) -> Option<Command<'a>> {
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
    let command = Command::ReportPwd {
        // data[0 .. data.len() - 1 :0]
        // PORT NOTE (fidelity): reference `[:0]const u8` RAW bytes (trailing NUL
        // excluded), NOT a lossy String decode.
        value: ZStr::sentinel(data, 0, data.len() - 1),
    };
    Some(command)
}

#[cfg(test)]
mod tests {
    use crate::osc::Parser;

    #[test]
    fn osc_7_report_pwd() {
        let mut p: Parser = Parser::init(None);

        let input = "7;file:///tmp/example";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap();
        assert!(matches!(cmd, crate::osc::Command::ReportPwd { .. }));
        match cmd {
            crate::osc::Command::ReportPwd { value } => {
                assert_eq!(b"file:///tmp/example", value.as_slice())
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_7_report_pwd_empty() {
        let mut p: Parser = Parser::init(None);

        let input = "7;";
        for ch in input.bytes() {
            p.next(ch);
        }
        let cmd = p.end(None).unwrap();
        assert!(matches!(cmd, crate::osc::Command::ReportPwd { .. }));
        match cmd {
            crate::osc::Command::ReportPwd { value } => assert_eq!(b"", value.as_slice()),
            _ => unreachable!(),
        }
    }
}
