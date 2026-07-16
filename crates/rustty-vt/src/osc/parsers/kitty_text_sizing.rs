//! Kitty's text sizing protocol (OSC 66)
//! Specification: https://sw.kovidgoyal.net/kitty/text-sizing-protocol/
//!
//! PORTS: ghostty src/terminal/osc/parsers/kitty_text_sizing.zig
//!
//! PORT STATUS: ported — full parse logic + `update`/`cval` + tests (vt-6).
//! The `OSC` payload (referenced by the OSC parent `Command::KittyTextSizing`)
//! plus `VAlign`/`HAlign` are defined here.

// PORT NOTE: Zig imports retargeted to ported crate paths (R2). `encoding` is
// the sibling `crate::osc::encoding`; `lib` is `rustty_lib`.
use crate::osc::encoding;
use crate::osc::{Command, Parser};

// PORT NOTE: `const log = std.log.scoped(.kitty_text_sizing);` — Rustty has no
// logging facade wired into this crate (matching the convention used by the
// `osc.zig` scoped logger), so the scoped logger and its calls are ported as
// commented lines.
// const log = std.log.scoped(.kitty_text_sizing);

pub const MAX_PAYLOAD_LENGTH: usize = 4096;

// PORT NOTE: `pub const VAlign = lib.Enum(lib.target, &.{ "top", "bottom",
// "center" })` — a plain enum with the keys in order for the `.zig` target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VAlign {
    Top,
    Bottom,
    Center,
}

impl VAlign {
    // PORT NOTE: `std.enums.fromInt(VAlign, v)` — the value `v` indexes the
    // ordered enum keys (0 = top, 1 = bottom, 2 = center); out of range => null.
    fn from_int(v: u8) -> Option<VAlign> {
        match v {
            0 => Some(VAlign::Top),
            1 => Some(VAlign::Bottom),
            2 => Some(VAlign::Center),
            _ => None,
        }
    }
}

// PORT NOTE: `pub const HAlign = lib.Enum(lib.target, &.{ "left", "right",
// "center" })` — a plain enum with the keys in order for the `.zig` target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HAlign {
    Left,
    Right,
    Center,
}

impl HAlign {
    // PORT NOTE: `std.enums.fromInt(HAlign, v)` — the value `v` indexes the
    // ordered enum keys (0 = left, 1 = right, 2 = center); out of range => null.
    fn from_int(v: u8) -> Option<HAlign> {
        match v {
            0 => Some(HAlign::Left),
            1 => Some(HAlign::Right),
            2 => Some(HAlign::Center),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OSC {
    pub scale: u8,       // 1 - 7 (u3, default 1)
    pub width: u8,       // 0 - 7 (0 means default) (u3)
    pub numerator: u8,   // u4
    pub denominator: u8, // u4
    pub valign: VAlign,
    pub halign: HAlign,
    /// PORT NOTE (fidelity): reference `text: [:0]const u8` — raw bytes, not lossy.
    pub text: Vec<u8>,
}

// PORT NOTE: `error{ UnknownKey, InvalidValue }` — the error set returned by
// `update`.
enum UpdateError {
    UnknownKey,
    InvalidValue,
}

impl OSC {
    // PORT NOTE: `pub const C = void;` and `pub fn cval(_: OSC) C { return {}; }`
    // — "We don't currently support encoding this to C in any way." Modeled as
    // the unit type / a no-op returning `()`.
    /// We don't currently support encoding this to C in any way.
    pub fn cval(&self) {}

    fn update(&mut self, key: u8, value: &[u8]) -> Result<(), UpdateError> {
        // All values are numeric, so we can do a small hack here
        // PORT NOTE: `std.fmt.parseInt(u4, value, 10)` parses a base-10 unsigned
        // value into a u4 (0 - 15); any parse failure (incl. overflow) =>
        // InvalidValue.
        let value_str = match std::str::from_utf8(value) {
            Ok(s) => s,
            Err(_) => return Err(UpdateError::InvalidValue),
        };
        let v: u8 = match value_str.parse::<u8>() {
            Ok(v) if v <= 15 => v,
            _ => return Err(UpdateError::InvalidValue),
        };

        match key {
            b's' => {
                if v == 0 {
                    return Err(UpdateError::InvalidValue);
                }
                // std.math.cast(u3, v) orelse return error.InvalidValue
                self.scale = match cast_u3(v) {
                    Some(s) => s,
                    None => return Err(UpdateError::InvalidValue),
                };
            }
            b'w' => {
                self.width = match cast_u3(v) {
                    Some(w) => w,
                    None => return Err(UpdateError::InvalidValue),
                }
            }
            b'n' => self.numerator = v,
            b'd' => self.denominator = v,
            b'v' => {
                self.valign = match VAlign::from_int(v) {
                    Some(va) => va,
                    None => return Err(UpdateError::InvalidValue),
                }
            }
            b'h' => {
                self.halign = match HAlign::from_int(v) {
                    Some(ha) => ha,
                    None => return Err(UpdateError::InvalidValue),
                }
            }
            _ => return Err(UpdateError::UnknownKey),
        }
        Ok(())
    }
}

// PORT NOTE: `std.math.cast(u3, v)` — return the value only if it fits in a u3
// (0 - 7), otherwise null.
fn cast_u3(v: u8) -> Option<u8> {
    if v <= 7 {
        Some(v)
    } else {
        None
    }
}

pub fn parse<'a>(parser: &'a mut Parser, _: Option<u8>) -> Option<&'a mut Command> {
    // assert(parser.state == .@"66");
    debug_assert!(parser.state == crate::osc::State::S66);

    let cap = if let Some(c) = parser.capture.as_mut() {
        c
    } else {
        parser.state = crate::osc::State::Invalid;
        return None;
    };

    // Write a NUL byte to ensure that `text` is NUL-terminated
    if !cap.write_byte(&mut parser.buffer, 0) {
        parser.state = crate::osc::State::Invalid;
        return None;
    }
    let data = cap.trailing(&parser.buffer);

    let payload_start = if let Some(s) = data.iter().position(|&b| b == b';') {
        s
    } else {
        // log.warn("missing semicolon before payload", .{});
        parser.state = crate::osc::State::Invalid;
        return None;
    };
    // data[payload_start + 1 .. data.len - 1 :0]
    let payload = &data[payload_start + 1..data.len() - 1];

    // Payload has to be a URL-safe UTF-8 string,
    // and be under the size limit.
    if payload.len() > MAX_PAYLOAD_LENGTH {
        // log.warn("payload is too long", .{});
        parser.state = crate::osc::State::Invalid;
        return None;
    }
    if !encoding::is_safe_utf8(payload) {
        // log.warn("payload is not escape code safe UTF-8", .{});
        parser.state = crate::osc::State::Invalid;
        return None;
    }

    // PORT NOTE (fidelity): the reference assigns `payload` (a `[:0]const u8`
    // slice) directly to `text`; copy the RAW bytes out (so the mutable
    // `parser.command` borrow below does not alias the capture buffer), NOT a
    // lossy String decode.
    let text = payload.to_vec();
    // PORT NOTE: the args region precedes the payload; copy it out for the same
    // aliasing reason before reborrowing `parser.command`.
    let args: Vec<u8> = data[0..payload_start].to_vec();

    parser.command = Command::KittyTextSizing(OSC {
        scale: 1,
        width: 0,
        numerator: 0,
        denominator: 0,
        valign: VAlign::Top,
        halign: HAlign::Left,
        text,
    });
    let cmd = match &mut parser.command {
        Command::KittyTextSizing(osc) => osc,
        _ => unreachable!(),
    };

    // Parse any arguments if given
    if payload_start > 0 {
        // var kv_it = std.mem.splitScalar(u8, data[0..payload_start], ':');
        let kv_it = args.split(|&b| b == b':');

        for kv in kv_it {
            // var it = std.mem.splitScalar(u8, kv, '=');
            let mut it = kv.splitn(2, |&b| b == b'=');
            // const k = it.next() orelse { log.warn("missing key"); continue; };
            let k = match it.next() {
                Some(k) => k,
                None => {
                    // log.warn("missing key", .{});
                    continue;
                }
            };
            if k.len() != 1 {
                // log.warn("key must be a single character", .{});
                continue;
            }

            // const value = it.next() orelse { log.warn("missing value"); continue; };
            let value = match it.next() {
                Some(value) => value,
                None => {
                    // log.warn("missing value", .{});
                    continue;
                }
            };

            // cmd.update(k[0], value) catch |err| { ... continue; };
            match cmd.update(k[0], value) {
                Ok(()) => {}
                Err(err) => {
                    match err {
                        // log.warn("unknown key: '{c}'", .{k[0]})
                        UpdateError::UnknownKey => {}
                        // log.warn("invalid value for key '{c}': {}", .{ k[0], err })
                        UpdateError::InvalidValue => {}
                    }
                    continue;
                }
            }
        }
    }

    Some(&mut parser.command)
}

#[cfg(test)]
mod tests {
    use super::{HAlign, VAlign};
    use crate::osc::{Command, Parser};

    #[test]
    fn osc_66_empty_parameters() {
        let mut p: Parser = Parser::init(None);

        let input = "66;;bobr";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::KittyTextSizing(_)));
        match cmd {
            Command::KittyTextSizing(osc) => {
                assert_eq!(1u8, osc.scale);
                assert_eq!(b"bobr", osc.text.as_slice());
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_66_single_parameter() {
        let mut p: Parser = Parser::init(None);

        let input = "66;s=2;kurwa";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::KittyTextSizing(_)));
        match cmd {
            Command::KittyTextSizing(osc) => {
                assert_eq!(2u8, osc.scale);
                assert_eq!(b"kurwa", osc.text.as_slice());
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_66_multiple_parameters() {
        let mut p: Parser = Parser::init(None);

        let input = "66;s=2:w=7:n=13:d=15:v=1:h=2;long";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::KittyTextSizing(_)));
        match cmd {
            Command::KittyTextSizing(osc) => {
                assert_eq!(2u8, osc.scale);
                assert_eq!(7u8, osc.width);
                assert_eq!(13u8, osc.numerator);
                assert_eq!(15u8, osc.denominator);
                assert_eq!(VAlign::Bottom, osc.valign);
                assert_eq!(HAlign::Center, osc.halign);
                assert_eq!(b"long", osc.text.as_slice());
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_66_scale_is_zero() {
        let mut p: Parser = Parser::init(None);

        let input = "66;s=0;nope";
        for ch in input.bytes() {
            p.next(ch);
        }
        let cmd = p.end(Some(0x1b)).unwrap().clone();

        assert!(matches!(cmd, Command::KittyTextSizing(_)));
        match cmd {
            Command::KittyTextSizing(osc) => assert_eq!(1u8, osc.scale),
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_66_invalid_parameters() {
        let mut p: Parser = Parser::init(None);

        for ch in "66;w=8:v=3:n=16;".bytes() {
            p.next(ch);
        }
        let cmd = p.end(Some(0x1b)).unwrap().clone();

        assert!(matches!(cmd, Command::KittyTextSizing(_)));
        match cmd {
            Command::KittyTextSizing(osc) => {
                assert_eq!(0u8, osc.width);
                assert!(osc.valign == VAlign::Top);
                assert_eq!(0u8, osc.numerator);
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_66_utf8() {
        let mut p: Parser = Parser::init(None);

        let input = "66;;👻魑魅魍魉ゴースッティ";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::KittyTextSizing(_)));
        match cmd {
            Command::KittyTextSizing(osc) => {
                assert_eq!("👻魑魅魍魉ゴースッティ".as_bytes(), osc.text.as_slice())
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn osc_66_unsafe_utf8() {
        let mut p: Parser = Parser::init(None);

        let input = "66;;\n";
        for ch in input.bytes() {
            p.next(ch);
        }

        assert!(p.end(Some(0x1b)).is_none());
    }

    #[test]
    fn osc_66_overlong_utf8() {
        let mut p: Parser = Parser::init(None);

        let input = format!("66;;{}", "bobr".repeat(1025));
        for ch in input.bytes() {
            p.next(ch);
        }

        assert!(p.end(Some(0x1b)).is_none());
    }
}
