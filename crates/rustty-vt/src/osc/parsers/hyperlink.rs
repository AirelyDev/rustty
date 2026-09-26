//! PORTS: ghostty src/terminal/osc/parsers/hyperlink.zig
//!
//! PORT STATUS: ported — full parse logic + tests (vt-6).

use crate::osc::{Command, Parser, ZStr};

// const log = std.log.scoped(.osc_hyperlink);
// The scope, carried through the `log` facade as `target: LOG_SCOPE`.
// CORRECTION (task S-1c): a PORT NOTE here said "Rustty has no logging facade
// wired into this crate", and the call below was ported as a comment. The
// crate logs through the `log` facade, so the call is live.
const LOG_SCOPE: &str = "osc_hyperlink";

/// Parse OSC 8 hyperlinks
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
    let data = cap.trailing_mut(&mut parser.buffer);
    let s = if let Some(s) = data.iter().position(|&b| b == b';') {
        s
    } else {
        parser.state = crate::osc::State::Invalid;
        return None;
    };

    let data_len = data.len();
    // parser.command = .{
    //     .hyperlink_start = .{
    //         .uri = data[s + 1 .. data.len - 1 :0],
    //     },
    // };
    // PORT NOTE (parity tail S7-7): the uri is sliced below, once the key/value
    // prefix `data[0..s + 1]` has been rewritten in place; the two ranges are
    // disjoint, so the order does not change any byte of the uri.

    data[s] = 0;
    let kvs_len = s + 1;
    // std.mem.replaceScalar(u8, kvs, ':', 0);
    for b in data[0..kvs_len].iter_mut() {
        if *b == b':' {
            *b = 0;
        }
    }
    let data: &'a [u8] = data;
    // data[s + 1 .. data.len - 1 :0]
    // PORT NOTE (fidelity): reference `uri: [:0]const u8` RAW bytes.
    let uri = ZStr::sentinel(data, s + 1, data_len - 1);
    // PORT NOTE (fidelity): reference `id: ?[:0]const u8` — optional RAW bytes.
    let mut id: Option<ZStr<'a>> = None;
    let mut kv_start: usize = 0;
    while kv_start < kvs_len {
        // std.mem.indexOfScalarPos(u8, kvs, kv_start + 1, 0)
        let kv_end = if let Some(off) = data[kv_start + 1..kvs_len].iter().position(|&b| b == 0) {
            kv_start + 1 + off
        } else {
            break;
        };
        let kv = &data[kv_start..kv_end + 1];
        let v = if let Some(v) = kv.iter().position(|&b| b == b'=') {
            v
        } else {
            break;
        };
        let key = &kv[0..v];
        // kv[v + 1 .. kv.len - 1 :0]
        let kv_len = kv.len();
        let value = ZStr::sentinel(kv, v + 1, kv_len - 1);
        if key == b"id" {
            if !value.is_empty() {
                id = Some(value);
            }
        } else {
            // log.warn("unknown hyperlink option: '{s}'", .{key});
            log::warn!(
                target: LOG_SCOPE,
                "unknown hyperlink option: '{}'",
                String::from_utf8_lossy(key)
            );
        }
        kv_start = kv_end + 1;
    }

    if uri.is_empty() {
        if id.is_some() {
            parser.state = crate::osc::State::Invalid;
            return None;
        }
        // parser.command = .hyperlink_end;
        return Some(Command::HyperlinkEnd);
    }

    // return &parser.command;
    Some(Command::HyperlinkStart { id, uri })
}

#[cfg(test)]
mod tests {
    use crate::osc::{Command, Parser};

    #[test]
    fn osc_8_hyperlink() {
        let mut p: Parser = Parser::init(None);

        let input = "8;;http://example.com";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap();
        assert!(matches!(cmd, Command::HyperlinkStart { .. }));
        match cmd {
            Command::HyperlinkStart { uri, .. } => {
                assert_eq!(uri.as_slice(), b"http://example.com")
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_8_hyperlink_with_id_set() {
        let mut p: Parser = Parser::init(None);

        let input = "8;id=foo;http://example.com";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap();
        assert!(matches!(cmd, Command::HyperlinkStart { .. }));
        match cmd {
            Command::HyperlinkStart { id, uri } => {
                assert_eq!(id.unwrap().as_slice(), b"foo");
                assert_eq!(uri.as_slice(), b"http://example.com");
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_8_hyperlink_with_empty_id() {
        let mut p: Parser = Parser::init(None);

        let input = "8;id=;http://example.com";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap();
        assert!(matches!(cmd, Command::HyperlinkStart { .. }));
        match cmd {
            Command::HyperlinkStart { id, uri } => {
                assert_eq!(id, None);
                assert_eq!(uri.as_slice(), b"http://example.com");
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_8_hyperlink_with_incomplete_key() {
        let mut p: Parser = Parser::init(None);

        let input = "8;id;http://example.com";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap();
        assert!(matches!(cmd, Command::HyperlinkStart { .. }));
        match cmd {
            Command::HyperlinkStart { id, uri } => {
                assert_eq!(id, None);
                assert_eq!(uri.as_slice(), b"http://example.com");
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_8_hyperlink_with_empty_key() {
        let mut p: Parser = Parser::init(None);

        let input = "8;=value;http://example.com";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap();
        assert!(matches!(cmd, Command::HyperlinkStart { .. }));
        match cmd {
            Command::HyperlinkStart { id, uri } => {
                assert_eq!(id, None);
                assert_eq!(uri.as_slice(), b"http://example.com");
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_8_hyperlink_with_empty_key_and_id() {
        let mut p: Parser = Parser::init(None);

        let input = "8;=value:id=foo;http://example.com";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap();
        assert!(matches!(cmd, Command::HyperlinkStart { .. }));
        match cmd {
            Command::HyperlinkStart { id, uri } => {
                assert_eq!(id.unwrap().as_slice(), b"foo");
                assert_eq!(uri.as_slice(), b"http://example.com");
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_8_hyperlink_with_empty_uri() {
        let mut p: Parser = Parser::init(None);

        let input = "8;id=foo;";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b));
        assert!(cmd.is_none());
    }

    #[test]
    fn osc_8_hyperlink_end() {
        let mut p: Parser = Parser::init(None);

        let input = "8;;";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap();
        assert!(matches!(cmd, Command::HyperlinkEnd));
    }

    // Task S-1c (no reference test): an option other than `id` logs the
    // reference's warning (hyperlink.zig:43) and the link still parses.
    #[test]
    fn osc_8_unknown_option_logs_a_warning() {
        let mut p: Parser = Parser::init(None);
        let cap = crate::log_capture::start();
        for ch in "8;foo=bar;http://example.com".bytes() {
            p.next(ch);
        }
        let cmd = p.end(Some(0x1b)).unwrap();
        let messages = cap.messages();
        drop(cap);
        assert!(matches!(cmd, Command::HyperlinkStart { .. }));
        assert_eq!(
            vec![(
                log::Level::Warn,
                "osc_hyperlink".to_string(),
                "unknown hyperlink option: 'foo'".to_string(),
            )],
            messages
        );
    }
}
