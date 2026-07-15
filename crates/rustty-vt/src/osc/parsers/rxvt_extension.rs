//! PORTS: ghostty src/terminal/osc/parsers/rxvt_extension.zig
//!
//! PORT STATUS: ported — full parse logic + tests (vt-6).

use crate::osc::{Command, Parser};

// PORT NOTE: `const log = std.log.scoped(.osc_rxvt_extension);` — Rustty has no
// logging facade wired into this crate (matching the convention used by the
// `osc.zig` scoped logger), so the scoped logger and its calls are ported as
// commented lines.
// const log = std.log.scoped(.osc_rxvt_extension);

/// Parse OSC 777
pub fn parse<'a>(parser: &'a mut Parser, _: Option<u8>) -> Option<&'a mut Command> {
    let cap = if let Some(c) = parser.capture.as_mut() {
        c
    } else {
        parser.state = crate::osc::State::Invalid;
        return None;
    };
    // ensure that we are sentinel terminated
    if !cap.write_byte(0) {
        parser.state = crate::osc::State::Invalid;
        return None;
    }
    let data = cap.trailing_mut();
    let k = if let Some(k) = data.iter().position(|&b| b == b';') {
        k
    } else {
        parser.state = crate::osc::State::Invalid;
        return None;
    };
    let ext = &data[0..k];
    if ext != b"notify" {
        // log.warn("unknown rxvt extension: {s}", .{ext});
        parser.state = crate::osc::State::Invalid;
        return None;
    }
    let t = if let Some(off) = data[k + 1..].iter().position(|&b| b == b';') {
        k + 1 + off
    } else {
        // log.warn("rxvt notify extension is missing the title", .{});
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
}
