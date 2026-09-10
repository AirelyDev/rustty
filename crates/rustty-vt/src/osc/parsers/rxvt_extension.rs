//! PORTS: ghostty src/terminal/osc/parsers/rxvt_extension.zig
//!
//! PORT STATUS: ported — full parse logic + tests (vt-6).

use crate::osc::{Command, Parser};

// const log = std.log.scoped(.osc_rxvt_extension);
const LOG_SCOPE: &str = "osc_rxvt_extension";

/// Parse OSC 777
pub fn parse<'a>(parser: &'a mut Parser, _: Option<u8>) -> Option<&'a mut Command> {
    let cap = if let Some(c) = parser.capture.as_mut() {
        c
    } else {
        parser.state = crate::osc::State::Invalid;
        return None;
    };
    // ensure that we are sentinel terminated
    if !cap.write_byte(&mut parser.buffer, 0) {
        parser.state = crate::osc::State::Invalid;
        return None;
    }
    let data = cap.trailing_mut(&mut parser.buffer);
    let k = if let Some(k) = data.iter().position(|&b| b == b';') {
        k
    } else {
        parser.state = crate::osc::State::Invalid;
        return None;
    };
    let ext = &data[0..k];
    if ext != b"notify" {
        log::warn!(target: LOG_SCOPE, "unknown rxvt extension: {}", String::from_utf8_lossy(ext));
        parser.state = crate::osc::State::Invalid;
        return None;
    }
    let t = if let Some(off) = data[k + 1..].iter().position(|&b| b == b';') {
        k + 1 + off
    } else {
        log::warn!(target: LOG_SCOPE, "rxvt notify extension is missing the title");
        parser.state = crate::osc::State::Invalid;
        return None;
    };
    data[t] = 0;
    let data_len = data.len();
    // data[k + 1 .. t :0]
    // PORT NOTE (fidelity): reference `title: [:0]const u8` RAW bytes.
    let title = data[k + 1..t].to_vec();
    // data[t + 1 .. data.len - 1 :0]
    // PORT NOTE (fidelity): reference `body: [:0]const u8` RAW bytes.
    let body = data[t + 1..data_len - 1].to_vec();
    parser.command = Command::ShowDesktopNotification { title, body };
    Some(&mut parser.command)
}

#[cfg(test)]
mod tests {
    use crate::osc::Parser;

    #[test]
    fn osc_777_show_desktop_notification_with_title() {
        let mut p: Parser = Parser::init(None);

        let input = "777;notify;Title;Body";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(
            cmd,
            crate::osc::Command::ShowDesktopNotification { .. }
        ));
        match cmd {
            crate::osc::Command::ShowDesktopNotification { title, body } => {
                assert_eq!(title.as_slice(), b"Title");
                assert_eq!(body.as_slice(), b"Body");
            }
            _ => unreachable!(),
        }
    }

    // S196-F1: an unrecognized rxvt extension name logs
    // `log.warn("unknown rxvt extension: {s}", .{ext})`
    // (rxvt_extension.zig:27-30).
    #[test]
    fn osc_777_unknown_extension_logs() {
        let mut p: Parser = Parser::init(None);
        let input = "777;bogus;whatever";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cap = crate::log_capture::start();
        assert!(p.end(Some(0x1b)).is_none());
        let messages = cap.messages();
        assert!(
            messages
                .iter()
                .any(|(level, target, msg)| *level == log::Level::Warn
                    && target == "osc_rxvt_extension"
                    && *msg == "unknown rxvt extension: bogus"),
            "expected the unknown-rxvt-extension diagnostic, got: {messages:?}"
        );
        drop(cap);
    }

    // S196-F1: a `notify` extension missing its title separator logs
    // `log.warn("rxvt notify extension is missing the title", .{})`
    // (rxvt_extension.zig:31-34).
    #[test]
    fn osc_777_notify_missing_title_logs() {
        let mut p: Parser = Parser::init(None);
        let input = "777;notify;TitleOnlyNoSecondSemicolon";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cap = crate::log_capture::start();
        assert!(p.end(Some(0x1b)).is_none());
        let messages = cap.messages();
        assert!(
            messages
                .iter()
                .any(|(level, target, msg)| *level == log::Level::Warn
                    && target == "osc_rxvt_extension"
                    && *msg == "rxvt notify extension is missing the title"),
            "expected the missing-title diagnostic, got: {messages:?}"
        );
        drop(cap);
    }
}
