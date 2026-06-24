//! PORTS: ghostty src/terminal/osc/parsers/report_pwd.zig
//!
//! PORT STATUS: ported — full parse logic + tests (vt-6).

use crate::osc::{Command, Parser};

/// Parse OSC 7
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
    parser.command = Command::ReportPwd {
        // data[0 .. data.len() - 1 :0]
        value: String::from_utf8_lossy(&data[0..data.len() - 1]).into_owned(),
    };
    Some(&mut parser.command)
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

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, crate::osc::Command::ReportPwd { .. }));
        match cmd {
            crate::osc::Command::ReportPwd { value } => assert_eq!("file:///tmp/example", value),
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
        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, crate::osc::Command::ReportPwd { .. }));
        match cmd {
            crate::osc::Command::ReportPwd { value } => assert_eq!("", value),
            _ => unreachable!(),
        }
    }
}
