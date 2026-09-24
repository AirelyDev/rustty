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

// const log = std.log.scoped(.kitty_text_sizing);
//
// PORT NOTE (task-14e-2): the note that stood here said "Rustty has no logging
// facade wired into this crate". That has not been true since task 14b —
// `rustty-vt` carries `log = { workspace = true }` and 138 live emissions in
// `stream.rs` alone. The reference's scope tag is carried through the facade as
// `target: LOG_SCOPE` (DECISION-logging.md §6r4).
const LOG_SCOPE: &str = "kitty_text_sizing";

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
// (Debug is derived for the reference's `log.warn("... : {}", .{err})` payload;
// dead-code analysis intentionally ignores derived Debug, hence the allow.)
#[derive(Debug)]
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
        // value into a u4 (0 - 15), through the port of the std function in
        // `rustty_zigstd::fmt`; any parse failure (incl. overflow) =>
        // InvalidValue.
        // CORRECTION (parity S195-F2): this used `str::parse::<u8>()` plus a
        // range check, which rejects the `_` digit separators and the `-0`
        // that `std.fmt.parseInt` accepts.
        // PORT NOTE: reference defect (R13), UD141: a `_` digit separator is refused
        // before parsing, as Ghostty's own OSC 99 parser does; see
        // `protocol_int::reject_digit_separators`.
        let v: u8 = match crate::protocol_int::reject_digit_separators(value).and_then(|()| {
            rustty_zigstd::fmt::parse_int_type(rustty_zigstd::fmt::IntType::unsigned(4), value, 10)
        }) {
            Ok(v) => u8::try_from(v).expect("a u4 fits in u8"),
            Err(_) => return Err(UpdateError::InvalidValue),
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
        log::warn!(target: LOG_SCOPE, "missing semicolon before payload");
        parser.state = crate::osc::State::Invalid;
        return None;
    };
    // data[payload_start + 1 .. data.len - 1 :0]
    let payload = &data[payload_start + 1..data.len() - 1];

    // Payload has to be a URL-safe UTF-8 string,
    // and be under the size limit.
    if payload.len() > MAX_PAYLOAD_LENGTH {
        // log.warn("payload is too long", .{});
        log::warn!(target: LOG_SCOPE, "payload is too long");
        parser.state = crate::osc::State::Invalid;
        return None;
    }
    if !encoding::is_safe_utf8(payload) {
        // log.warn("payload is not escape code safe UTF-8", .{});
        log::warn!(target: LOG_SCOPE, "payload is not escape code safe UTF-8");
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
                    log::warn!(target: LOG_SCOPE, "missing key");
                    continue;
                }
            };
            if k.len() != 1 {
                // log.warn("key must be a single character", .{});
                log::warn!(target: LOG_SCOPE, "key must be a single character");
                continue;
            }

            // const value = it.next() orelse { log.warn("missing value"); continue; };
            let value = match it.next() {
                Some(value) => value,
                None => {
                    // log.warn("missing value", .{});
                    log::warn!(target: LOG_SCOPE, "missing value");
                    continue;
                }
            };

            // cmd.update(k[0], value) catch |err| { ... continue; };
            match cmd.update(k[0], value) {
                Ok(()) => {}
                Err(err) => {
                    match err {
                        // log.warn("unknown key: '{c}'", .{k[0]})
                        UpdateError::UnknownKey => {
                            log::warn!(
                                target: LOG_SCOPE,
                                "unknown key: '{}'",
                                k[0] as char
                            );
                        }
                        // log.warn("invalid value for key '{c}': {}", .{ k[0], err })
                        //
                        // PORT NOTE (N11): Zig's `{}` on an error renders
                        // `error.<Name>`; `{err:?}` renders the bare variant
                        // name. Workspace-dominant rendering; N11's scope.
                        UpdateError::InvalidValue => {
                            log::warn!(
                                target: LOG_SCOPE,
                                "invalid value for key '{}': {err:?}",
                                k[0] as char
                            );
                        }
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
    use super::{HAlign, VAlign, MAX_PAYLOAD_LENGTH};
    use crate::osc::{Command, Parser};

    // ---- task-14e-2: the restored `kitty_text_sizing.zig` diagnostics -------
    //
    // `assert_eq!` on the FULL captured record vector, never `.any()`.

    fn warn(msg: &str) -> crate::log_capture::CapturedRecord {
        (
            log::Level::Warn,
            "kitty_text_sizing".to_string(),
            msg.to_string(),
        )
    }

    fn drive(input: &str) -> Vec<crate::log_capture::CapturedRecord> {
        let mut p: Parser = Parser::init(None);
        let cap = crate::log_capture::start();
        for ch in input.bytes() {
            p.next(ch);
        }
        let _ = p.end(Some(0x1b));
        let messages = cap.messages();
        drop(cap);
        messages
    }

    /// `kitty_text_sizing.zig:87` — no `;` in the captured payload.
    ///
    /// The capture starts AFTER the `66;` introducer, so `66;abc` is the
    /// no-semicolon case; a bare `66` never reaches this parser at all (the OSC
    /// dispatcher goes `Invalid` on the missing introducer separator).
    #[test]
    fn a_missing_semicolon_warns() {
        assert_eq!(
            vec![warn("missing semicolon before payload")],
            drive("66;abc")
        );
    }

    /// `:122`/`:126`/`:131` — the key/value shape diagnostics.
    #[test]
    fn malformed_key_value_pairs_warn() {
        // `s` alone: `splitn(2, '=')` yields the key and no value.
        assert_eq!(vec![warn("missing value")], drive("66;s;hi"));
        // A two-character key.
        assert_eq!(
            vec![warn("key must be a single character")],
            drive("66;sc=2;hi")
        );
    }

    /// `:137` — an unrecognised single-character key.
    #[test]
    fn an_unknown_key_warns_with_the_character() {
        assert_eq!(vec![warn("unknown key: 'z'")], drive("66;z=1;hi"));
    }

    /// `:96` — a payload over `MAX_PAYLOAD_LENGTH`.
    ///
    /// Needs an ALLOCATING parser: with `Parser::init(None)` the OSC capture is
    /// capped at the inline `MAX_BUF` (2048) and the dispatcher goes `Invalid`
    /// long before this parser sees the payload — so the test would have passed
    /// its `Vec::new()` assertion while never reaching the guard.
    #[test]
    fn an_over_long_payload_warns() {
        let backend = rustty_lib::allocator::GlobalZigAllocator;
        let alloc = rustty_lib::allocator::ZigAllocatorRef::new(&backend);
        let mut p: Parser = Parser::init(Some(alloc));
        let mut input = String::from("66;;");
        input.push_str(&"a".repeat(MAX_PAYLOAD_LENGTH + 1));

        let cap = crate::log_capture::start();
        for ch in input.bytes() {
            p.next(ch);
        }
        let out = p.end(Some(0x1b)).is_none();
        let messages = cap.messages();
        drop(cap);

        // Positive reach: the guard's own `return None`.
        assert!(out);
        assert_eq!(vec![warn("payload is too long")], messages);
    }

    /// The absence half: a well-formed OSC 66 emits NOTHING, and the parse
    /// really happened (the positive reach).
    #[test]
    fn a_well_formed_osc_66_is_silent() {
        let mut p: Parser = Parser::init(None);
        let cap = crate::log_capture::start();
        for ch in b"66;s=2;bobr" {
            p.next(*ch);
        }
        let cmd = p.end(Some(0x1b)).map(|c| c.clone());
        let messages = cap.messages();
        drop(cap);

        match cmd {
            Some(Command::KittyTextSizing(osc)) => {
                assert_eq!(2u8, osc.scale);
                assert_eq!(b"bobr", osc.text.as_slice());
            }
            other => panic!(
                "the OSC did not parse ({other:?}), so this test never reached \
                 the code whose silence it asserts"
            ),
        }
        assert_eq!(Vec::<crate::log_capture::CapturedRecord>::new(), messages);
    }

    // Parity S195-F2: no reference test. `update` parses every value with
    // `std.fmt.parseInt(u4, value, 10)`, which accepts a leading `+`. UD141
    // (R13): its `_` digit separators are refused, as Ghostty's own OSC 99
    // parser refuses them (the field then keeps its default, the reference's
    // `InvalidValue` path).
    #[test]
    fn osc_66_values_follow_zig_parse_int_but_refuse_digit_separators() {
        let mut p: Parser = Parser::init(None);
        for ch in b"66;n=12:d=+15;bobr" {
            p.next(*ch);
        }
        match p.end(Some(0x1b)).map(|c| c.clone()) {
            Some(Command::KittyTextSizing(osc)) => {
                assert_eq!(12u8, osc.numerator);
                assert_eq!(15u8, osc.denominator);
            }
            other => panic!("the OSC did not parse ({other:?})"),
        }

        let mut p: Parser = Parser::init(None);
        for ch in b"66;n=1_2:d=+15:s=_2:w=2_;bobr" {
            p.next(*ch);
        }
        match p.end(Some(0x1b)).map(|c| c.clone()) {
            Some(Command::KittyTextSizing(osc)) => {
                assert_eq!(0u8, osc.numerator);
                assert_eq!(15u8, osc.denominator);
                assert_eq!(1u8, osc.scale);
                assert_eq!(0u8, osc.width);
                assert_eq!(b"bobr", osc.text.as_slice());
            }
            other => panic!("the OSC did not parse ({other:?})"),
        }
    }

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
