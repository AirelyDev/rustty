//! PORTS: ghostty src/terminal/osc/parsers/change_window_title.zig
//!
//! PORT STATUS: ported — full parse logic + tests (vt-6).

use crate::osc::{Command, Parser};

/// Parse OSC 0 and OSC 2
pub fn parse<'a>(parser: &'a mut Parser, _: Option<u8>) -> Option<&'a mut Command> {
    let cap = if let Some(c) = parser.capture.as_mut() {
        c
    } else {
        parser.state = crate::osc::State::Invalid;
        return None;
    };
    // cap.writer.writeByte(0) catch { ... };
    if !cap.write_byte(0) {
        parser.state = crate::osc::State::Invalid;
        return None;
    }
    let data = cap.trailing();
    parser.command = Command::ChangeWindowTitle(
        // data[0 .. data.len() - 1 :0]
        String::from_utf8_lossy(&data[0..data.len() - 1]).into_owned(),
    );
    Some(&mut parser.command)
}

#[cfg(test)]
mod tests {
    use crate::osc::Parser;

    #[test]
    fn osc_0_change_window_title() {
        let mut p: Parser = Parser::init(None);
        p.next(b'0');
        p.next(b';');
        p.next(b'a');
        p.next(b'b');
        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, crate::osc::Command::ChangeWindowTitle(_)));
        match cmd {
            crate::osc::Command::ChangeWindowTitle(v) => assert_eq!("ab", v),
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_0_longer_than_buffer() {
        let mut p: Parser = Parser::init(None);

        let mut input = String::from("0;");
        input.push_str(&"a".repeat(crate::osc::MAX_BUF + 2));
        for ch in input.bytes() {
            p.next(ch);
        }

        assert!(p.end(None).is_none());
    }

    #[test]
    fn osc_0_one_shorter_than_buffer_length() {
        let mut p: Parser = Parser::init(None);

        let prefix = "0;";
        let title = "a".repeat(crate::osc::MAX_BUF - 1);
        let input = format!("{prefix}{title}");
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, crate::osc::Command::ChangeWindowTitle(_)));
        match cmd {
            crate::osc::Command::ChangeWindowTitle(v) => assert_eq!(title, v),
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_0_exactly_at_buffer_length() {
        let mut p: Parser = Parser::init(None);

        let prefix = "0;";
        let title = "a".repeat(crate::osc::MAX_BUF);
        let input = format!("{prefix}{title}");
        for ch in input.bytes() {
            p.next(ch);
        }

        // This should be null because we always reserve space for a null terminator.
        assert!(p.end(None).is_none());
    }

    #[test]
    fn osc_2_change_window_title_with_2() {
        let mut p: Parser = Parser::init(None);
        p.next(b'2');
        p.next(b';');
        p.next(b'a');
        p.next(b'b');
        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, crate::osc::Command::ChangeWindowTitle(_)));
        match cmd {
            crate::osc::Command::ChangeWindowTitle(v) => assert_eq!("ab", v),
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_2_change_window_title_with_utf8() {
        let mut p: Parser = Parser::init(None);
        p.next(b'2');
        p.next(b';');
        // '—' EM DASH U+2014 (E2 80 94)
        p.next(0xE2);
        p.next(0x80);
        p.next(0x94);

        p.next(b' ');
        // '‐' HYPHEN U+2010 (E2 80 90)
        // Intententionally chosen to conflict with the 0x90 C1 control
        p.next(0xE2);
        p.next(0x80);
        p.next(0x90);
        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, crate::osc::Command::ChangeWindowTitle(_)));
        match cmd {
            crate::osc::Command::ChangeWindowTitle(v) => assert_eq!("— ‐", v),
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_2_change_window_title_empty() {
        let mut p: Parser = Parser::init(None);
        p.next(b'2');
        p.next(b';');
        let cmd = p.end(None).unwrap().clone();
        assert!(matches!(cmd, crate::osc::Command::ChangeWindowTitle(_)));
        match cmd {
            crate::osc::Command::ChangeWindowTitle(v) => assert_eq!("", v),
            _ => unreachable!(),
        }
    }
}
