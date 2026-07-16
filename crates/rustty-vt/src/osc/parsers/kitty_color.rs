//! PORTS: ghostty src/terminal/osc/parsers/kitty_color.zig
//!
//! PORT STATUS: ported — full parse logic + tests (vt-6). The OSC 21 payload
//! type is `crate::kitty::color::OSC` (already ported), referenced by the
//! parent `Command::KittyColorProtocol`.

// PORT NOTE: Zig imports retargeted to ported crate paths (R2). `kitty_color`
// is `crate::kitty::color`; `RGB` is `crate::color::RGB`.
use crate::color::{InvalidFormat, RGB};
use crate::kitty::color as kitty_color;
use crate::osc::{Command, Parser};

// PORT NOTE: `const log = std.log.scoped(.osc_kitty_color);` — Rustty has no
// logging facade wired into this crate (matching the convention used by the
// `osc.zig` scoped logger), so the scoped logger and its calls are ported as
// commented lines.
// const log = std.log.scoped(.osc_kitty_color);

/// Parse OSC 21, the Kitty Color Protocol.
pub fn parse<'a>(parser: &'a mut Parser, terminator_ch: Option<u8>) -> Option<&'a mut Command> {
    // assert(parser.state == .@"21");
    debug_assert!(parser.state == crate::osc::State::S21);

    // const alloc = parser.alloc orelse { ... };
    if parser.alloc.is_none() {
        parser.state = crate::osc::State::Invalid;
        return None;
    }
    if parser.capture.is_none() {
        parser.state = crate::osc::State::Invalid;
        return None;
    }
    parser.command = Command::KittyColorProtocol(kitty_color::OSC {
        list: Vec::new(),
        terminator: crate::osc::Terminator::init(terminator_ch),
    });
    // PORT NOTE: `data = cap.trailing()` is borrowed up front; in Rust we take
    // an owned copy so the mutable `parser.command` borrow below does not alias
    // the capture buffer.
    let data: Vec<u8> = parser
        .capture
        .as_ref()
        .unwrap()
        .trailing(&parser.buffer)
        .to_vec();
    let list = match &mut parser.command {
        Command::KittyColorProtocol(osc) => &mut osc.list,
        _ => unreachable!(),
    };
    // var kv_it = std.mem.splitScalar(u8, data, ';');
    let kv_it = data.split(|&b| b == b';');
    for kv in kv_it {
        if list.len() >= kitty_color::Kind::MAX * 2 {
            // log.warn("exceeded limit for number of keys in kitty color protocol, ignoring", .{});
            parser.state = crate::osc::State::Invalid;
            return None;
        }
        // var it = std.mem.splitScalar(u8, kv, '=');
        let mut it = kv.splitn(2, |&b| b == b'=');
        // const k = it.next() orelse continue;
        let k = match it.next() {
            Some(k) => k,
            None => continue,
        };
        if k.is_empty() {
            // log.warn("zero length key in kitty color protocol", .{});
            continue;
        }
        // PORT NOTE: keys are ASCII identifiers / decimal palette indices; an
        // invalid-UTF-8 key cannot match any `Kind`, so the lossy decode here
        // is faithful (`Kind.parse` returns null for it either way).
        let k_str = String::from_utf8_lossy(k);
        let key = match kitty_color::Kind::parse(&k_str) {
            Some(key) => key,
            None => {
                // log.warn("unknown key in kitty color protocol: {s}", .{k});
                continue;
            }
        };
        // const value = std.mem.trim(u8, it.rest(), " ");
        // PORT NOTE: `it.rest()` is the remainder after the first '='; with a
        // 2-way split, `it.next()` is exactly that remainder (or empty when no
        // '=' was present).
        let rest = it.next().unwrap_or(&[]);
        let value = trim_spaces(rest);
        if value.is_empty() {
            // list.append(alloc, .{ .reset = key }) catch ...
            list.push(kitty_color::Request::Reset(key));
        } else if value == b"?" {
            // list.append(alloc, .{ .query = key }) catch ...
            list.push(kitty_color::Request::Query(key));
        } else {
            // PORT NOTE: `RGB.parse(value)` accepts ASCII color specs; invalid
            // UTF-8 cannot be a valid spec, so a lossy decode is faithful (the
            // parse fails with InvalidFormat either way).
            let value_str = String::from_utf8_lossy(value);
            let color = match RGB::parse(&value_str) {
                Ok(color) => color,
                Err(InvalidFormat) => {
                    // log.warn("invalid color format in kitty color protocol: {s}", .{value});
                    continue;
                }
            };
            // list.append(alloc, .{ .set = .{ .key = key, .color = ... } }) catch ...
            list.push(kitty_color::Request::Set { key, color });
        }
    }
    Some(&mut parser.command)
}

// PORT NOTE: `std.mem.trim(u8, slice, " ")` — strip leading/trailing space
// (0x20) bytes from the slice.
fn trim_spaces(s: &[u8]) -> &[u8] {
    let mut start = 0;
    let mut end = s.len();
    while start < end && s[start] == b' ' {
        start += 1;
    }
    while end > start && s[end - 1] == b' ' {
        end -= 1;
    }
    &s[start..end]
}

#[cfg(test)]
mod tests {
    use crate::kitty::color::{Kind, Request, Special};
    use crate::osc::{Command, Parser};
    use rustty_lib::allocator::{GlobalZigAllocator, ZigAllocatorRef};

    // PORT NOTE: the reference uses `testing.allocator`. The OSC 21 list is a
    // Rust `Vec` (self-owning), so the parser only needs the allocator's
    // *presence* (the `parser.alloc orelse` gate). `GlobalZigAllocator` is the
    // always-available stateless backend that satisfies that gate.

    #[test]
    fn osc_21_kitty_color_protocol() {
        let backend = GlobalZigAllocator;
        let alloc = ZigAllocatorRef::new(&backend);
        let mut p: Parser = Parser::init(Some(alloc));

        let input = "21;foreground=?;background=rgb:f0/f8/ff;cursor=aliceblue;cursor_text;visual_bell=;selection_foreground=#xxxyyzz;selection_background=?;selection_background=#aabbcc;2=?;3=rgbi:1.0/1.0/1.0";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::KittyColorProtocol(_)));
        let osc = match cmd {
            Command::KittyColorProtocol(osc) => osc,
            _ => unreachable!(),
        };
        assert_eq!(9usize, osc.list.len());
        {
            let item = osc.list[0];
            assert!(matches!(item, Request::Query(_)));
            assert_eq!(Request::Query(Kind::Special(Special::Foreground)), item);
        }
        {
            let item = osc.list[1];
            assert!(matches!(item, Request::Set { .. }));
            match item {
                Request::Set { key, color } => {
                    assert_eq!(Kind::Special(Special::Background), key);
                    assert_eq!(0xf0u8, color.r);
                    assert_eq!(0xf8u8, color.g);
                    assert_eq!(0xffu8, color.b);
                }
                _ => unreachable!(),
            }
        }
        {
            let item = osc.list[2];
            assert!(matches!(item, Request::Set { .. }));
            match item {
                Request::Set { key, color } => {
                    assert_eq!(Kind::Special(Special::Cursor), key);
                    assert_eq!(0xf0u8, color.r);
                    assert_eq!(0xf8u8, color.g);
                    assert_eq!(0xffu8, color.b);
                }
                _ => unreachable!(),
            }
        }
        {
            let item = osc.list[3];
            assert!(matches!(item, Request::Reset(_)));
            assert_eq!(Request::Reset(Kind::Special(Special::CursorText)), item);
        }
        {
            let item = osc.list[4];
            assert!(matches!(item, Request::Reset(_)));
            assert_eq!(Request::Reset(Kind::Special(Special::VisualBell)), item);
        }
        {
            let item = osc.list[5];
            assert!(matches!(item, Request::Query(_)));
            assert_eq!(
                Request::Query(Kind::Special(Special::SelectionBackground)),
                item
            );
        }
        {
            let item = osc.list[6];
            assert!(matches!(item, Request::Set { .. }));
            match item {
                Request::Set { key, color } => {
                    assert_eq!(Kind::Special(Special::SelectionBackground), key);
                    assert_eq!(0xaau8, color.r);
                    assert_eq!(0xbbu8, color.g);
                    assert_eq!(0xccu8, color.b);
                }
                _ => unreachable!(),
            }
        }
        {
            let item = osc.list[7];
            assert!(matches!(item, Request::Query(_)));
            assert_eq!(Request::Query(Kind::Palette(2)), item);
        }
        {
            let item = osc.list[8];
            assert!(matches!(item, Request::Set { .. }));
            match item {
                Request::Set { key, color } => {
                    assert_eq!(Kind::Palette(3), key);
                    assert_eq!(0xffu8, color.r);
                    assert_eq!(0xffu8, color.g);
                    assert_eq!(0xffu8, color.b);
                }
                _ => unreachable!(),
            }
        }
    }

    #[test]
    fn osc_21_kitty_color_protocol_without_allocator() {
        let mut p: Parser = Parser::init(None);

        let input = "21;foreground=?";
        for ch in input.bytes() {
            p.next(ch);
        }
        assert!(p.end(Some(0x1b)).is_none());
    }

    #[test]
    fn osc_21_kitty_color_protocol_double_reset() {
        let backend = GlobalZigAllocator;
        let alloc = ZigAllocatorRef::new(&backend);
        let mut p: Parser = Parser::init(Some(alloc));

        let input = "21;foreground=?;background=rgb:f0/f8/ff;cursor=aliceblue;cursor_text;visual_bell=;selection_foreground=#xxxyyzz;selection_background=?;selection_background=#aabbcc;2=?;3=rgbi:1.0/1.0/1.0";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::KittyColorProtocol(_)));

        p.reset();
        p.reset();
    }

    #[test]
    fn osc_21_kitty_color_protocol_reset_after_invalid() {
        let backend = GlobalZigAllocator;
        let alloc = ZigAllocatorRef::new(&backend);
        let mut p: Parser = Parser::init(Some(alloc));

        let input = "21;foreground=?;background=rgb:f0/f8/ff;cursor=aliceblue;cursor_text;visual_bell=;selection_foreground=#xxxyyzz;selection_background=?;selection_background=#aabbcc;2=?;3=rgbi:1.0/1.0/1.0";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::KittyColorProtocol(_)));

        p.reset();

        assert_eq!(crate::osc::State::Start, p.state);
        p.next(b'X');
        assert_eq!(crate::osc::State::Invalid, p.state);

        p.reset();
    }

    #[test]
    fn osc_21_kitty_color_protocol_no_key() {
        let backend = GlobalZigAllocator;
        let alloc = ZigAllocatorRef::new(&backend);
        let mut p: Parser = Parser::init(Some(alloc));

        let input = "21;";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::KittyColorProtocol(_)));
        match cmd {
            Command::KittyColorProtocol(osc) => assert_eq!(0, osc.list.len()),
            _ => unreachable!(),
        }
    }
}
