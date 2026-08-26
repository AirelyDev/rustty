//! Kitty's desktop notification protocol (OSC 99)
//! Specification: https://sw.kovidgoyal.net/kitty/desktop-notifications/
//!
//! PORTS: ghostty src/terminal/osc/parsers/kitty_desktop_notification.zig
//!
//! PORT STATUS: ported — full parse logic + `read_option`/`Option_`/`Action`/
//! `Occasion`/`Payload`/`Urgency` machinery + all 57 tests (P5-W5 / B-2 end
//! state at `ca9e5b130`; the intermediate `93bf7d810`/`202e639d9`/`301b69df4`/
//! `9a8e7ae18`/`aa4ec3508`/`07f33ad91`/`073bffcff` forms fold in per the
//! range-diff end-state rule).

// PORT NOTE: `const std = @import("std");` — std facilities are used inline.
// PORT NOTE: `const assert = @import("../../../quirks.zig").inlineAssert;` —
// retargeted to `debug_assert!` at the call site (matching the convention used
// by the other ported OSC parsers).
// PORT NOTE: `const Parser = @import("../../osc.zig").Parser;`
// PORT NOTE: `const Command = @import("../../osc.zig").Command;`
// PORT NOTE: `const Terminator = @import("../../osc.zig").Terminator;`
use crate::osc::{Command, Parser, State, Terminator};

// PORT NOTE: `const kitty_metadata = @import("../kitty_metadata.zig");`
use crate::osc::kitty_metadata;
// PORT NOTE: `const encoding = @import("../encoding.zig");`
use crate::osc::encoding;

// PORT NOTE: `const log = std.log.scoped(.kitty_desktop_notification);` —
// Rustty has no logging facade wired into this crate (matching the convention
// used by the `osc.zig` scoped logger), so the scoped logger is ported as a
// commented line.
// const log = std.log.scoped(.kitty_desktop_notification);

pub const MAX_PLAIN_PAYLOAD_BYTES: usize = 2048;
pub const MAX_ENCODED_PAYLOAD_BYTES: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OSC {
    /// The raw metadata that was received. It can be parsed by using the `readOption` method.
    ///
    /// PORT NOTE (fidelity): reference `metadata: []const u8` — RAW bytes.
    pub metadata: Vec<u8>,
    /// The raw payload. It may be Base64 encoded, check the `e` option.
    ///
    /// PORT NOTE (fidelity): reference `payload: []const u8` — RAW bytes.
    pub payload: Vec<u8>,
    /// The terminator that was used in case we need to send a response.
    pub terminator: Terminator,
}

impl OSC {
    /// Decode an option from the metadata.
    // PORT NOTE: the reference uses a `comptime key: Option` parameter with a
    // dependent return type `key.Type()`. Rust has no comptime-dependent return
    // types, so `read_option` takes a runtime `Option_` and returns a tagged
    // `OptionValue` whose active variant matches the option's `Type()` (the
    // kitty_clipboard_protocol.rs precedent).
    pub fn read_option(&self, key: Option_) -> OptionValue<'_> {
        // return key.read(self.metadata);
        key.read(&self.metadata)
    }
}

// pub const Action = packed struct { focus: bool, report: bool, ... };
// PORT NOTE: `packed struct` of bools (a 2-bit value in Zig); modeled as a
// plain struct — no ABI surface depends on the packing here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Action {
    pub focus: bool,
    pub report: bool,
}

impl Action {
    // pub const default: Action = .{ .focus = true, .report = false };
    pub const DEFAULT: Action = Action {
        focus: true,
        report: false,
    };

    pub fn init(str: &[u8]) -> Action {
        // return parsePackedStruct(Action, str);
        parse_packed_struct(str)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Occasion {
    Always,
    Invisible,
    Unfocused,
}

impl Occasion {
    // pub const default: Occasion = .always;
    pub const DEFAULT: Occasion = Occasion::Always;

    pub fn init(str: &[u8]) -> Occasion {
        // return std.meta.stringToEnum(Occasion, str) orelse .default;
        match str {
            b"always" => Occasion::Always,
            b"invisible" => Occasion::Invisible,
            b"unfocused" => Occasion::Unfocused,
            _ => Occasion::DEFAULT,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Payload {
    Alive,
    Body,
    Buttons,
    Close,
    Icon,
    Query,
    Title,
    /// This is a special value to indicate that an unknown payload value was
    /// specified and it should be ignored.
    Unknown,
}

impl Payload {
    // pub const default: Payload = .title;
    pub const DEFAULT: Payload = Payload::Title;

    pub fn init(str: &[u8]) -> Payload {
        if str.len() == 1 && str[0] == b'?' {
            return Payload::Query;
        }
        // The string `query` is not allowed, it should be a single question
        // mark if you want a query.
        if str == b"query" {
            return Payload::Unknown;
        }
        // return std.meta.stringToEnum(Payload, str) orelse .unknown;
        match str {
            b"alive" => Payload::Alive,
            b"body" => Payload::Body,
            b"buttons" => Payload::Buttons,
            b"close" => Payload::Close,
            b"icon" => Payload::Icon,
            b"title" => Payload::Title,
            b"unknown" => Payload::Unknown,
            _ => Payload::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Urgency {
    Low,
    Normal,
    High,
}

impl Urgency {
    // pub const default: Urgency = .normal;
    pub const DEFAULT: Urgency = Urgency::Normal;

    pub fn init(str: &[u8]) -> Urgency {
        if str.len() != 1 {
            return Urgency::DEFAULT;
        }
        match str[0] {
            b'0' => Urgency::Low,
            b'1' => Urgency::Normal,
            b'2' => Urgency::High,
            _ => Urgency::DEFAULT,
        }
    }
}

// PORT NOTE: `pub const Option = enum { ... }` — Rust reserves `Option` for the
// std optional type, so the ported enum is named `Option_` (the value type
// dispatched on by `read_option`; same convention as
// kitty_clipboard_protocol.rs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Option_ {
    /// What action(s) should be taken when a notification is clicked.
    A,
    /// Should a notification be sent to the application when the notification
    /// is closed?
    C,
    /// Are we done with the notification, and it is ready to be sent?
    D,
    /// Is the payload encoded with Base64?
    E,
    /// The name of the application that is sending the notification.
    F,
    /// Identifier for icon data. Only used when the payload is icon data.
    G,
    /// Identifier for the notification.
    I,
    /// Icon name.
    N,
    /// When to honor the notification request.
    O,
    /// Type of the payload.
    P,
    /// The sound name to play with the notification.
    S,
    /// The type of the notification.
    T,
    /// The urgency of the notification.
    U,
    /// When to auto-close the notification.
    W,
}

/// The dependent value produced by `Option_::read`. The active variant matches
/// the `Option_::Type()` of the option that produced it.
// PORT NOTE: this enum stands in for the comptime-dependent `key.Type()` return
// type of `Option.read` in the reference:
//   .a => Action,
//   .c => bool,
//   .d => bool,
//   .e => bool,
//   .f => ?[]const u8,
//   .g => ?[]const u8,
//   .i => ?[]const u8,
//   .n => kitty_metadata.ValueIterator(@tagName(key), valid_metadata_value_characters),
//   .o => Occasion,
//   .p => Payload,
//   .s => []const u8,
//   .t => kitty_metadata.ValueIterator(@tagName(key), valid_metadata_value_characters),
//   .u => Urgency,
//   .w => i32,
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OptionValue<'a> {
    Action(Action),
    Bool(bool),
    /// PORT NOTE (fidelity): the `?[]const u8` option values are RAW bytes
    /// borrowing the metadata.
    OptBytes(Option<&'a [u8]>),
    Iterator(kitty_metadata::ValueIterator<'a>),
    Occasion(Occasion),
    Payload(Payload),
    /// PORT NOTE (fidelity): the `[]const u8` option value is RAW bytes.
    Bytes(&'a [u8]),
    Urgency(Urgency),
    Int(i32),
}

impl<'a> OptionValue<'a> {
    /// Helper for the `.a => Action` option.
    pub fn as_action(&self) -> Action {
        match self {
            OptionValue::Action(v) => *v,
            _ => panic!("option value is not an action"),
        }
    }

    /// Helper for the `bool` options (`.c`/`.d`/`.e`).
    pub fn as_bool(&self) -> bool {
        match self {
            OptionValue::Bool(v) => *v,
            _ => panic!("option value is not a bool"),
        }
    }

    /// Helper for the `?[]const u8` options (`.f`/`.g`/`.i`).
    ///
    /// PORT NOTE (fidelity): returns the RAW bytes, not a UTF-8 `str`.
    pub fn as_opt_bytes(&self) -> Option<&'a [u8]> {
        match self {
            OptionValue::OptBytes(v) => *v,
            _ => panic!("option value is not an optional byte string"),
        }
    }

    /// Helper for the iterator options (`.n`/`.t`): consumes the value,
    /// yielding the `kitty_metadata.ValueIterator` the reference returns
    /// directly (`var it = osc.readOption(.n);`).
    pub fn into_iterator(self) -> kitty_metadata::ValueIterator<'a> {
        match self {
            OptionValue::Iterator(v) => v,
            _ => panic!("option value is not an iterator"),
        }
    }

    /// Helper for the `.o => Occasion` option.
    pub fn as_occasion(&self) -> Occasion {
        match self {
            OptionValue::Occasion(v) => *v,
            _ => panic!("option value is not an occasion"),
        }
    }

    /// Helper for the `.p => Payload` option.
    pub fn as_payload(&self) -> Payload {
        match self {
            OptionValue::Payload(v) => *v,
            _ => panic!("option value is not a payload"),
        }
    }

    /// Helper for the `.s => []const u8` option.
    ///
    /// PORT NOTE (fidelity): returns the RAW bytes, not a UTF-8 `str`.
    pub fn as_bytes(&self) -> &'a [u8] {
        match self {
            OptionValue::Bytes(v) => v,
            _ => panic!("option value is not a byte string"),
        }
    }

    /// Helper for the `.u => Urgency` option.
    pub fn as_urgency(&self) -> Urgency {
        match self {
            OptionValue::Urgency(v) => *v,
            _ => panic!("option value is not an urgency"),
        }
    }

    /// Helper for the `.w => i32` option.
    pub fn as_int(&self) -> i32 {
        match self {
            OptionValue::Int(v) => *v,
            _ => panic!("option value is not an int"),
        }
    }
}

impl Option_ {
    // PORT NOTE: `@tagName(key)` — the option's metadata key spelling.
    fn tag_name(self) -> &'static [u8] {
        match self {
            Option_::A => b"a",
            Option_::C => b"c",
            Option_::D => b"d",
            Option_::E => b"e",
            Option_::F => b"f",
            Option_::G => b"g",
            Option_::I => b"i",
            Option_::N => b"n",
            Option_::O => b"o",
            Option_::P => b"p",
            Option_::S => b"s",
            Option_::T => b"t",
            Option_::U => b"u",
            Option_::W => b"w",
        }
    }

    // pub fn default(comptime key: Option) key.Type() { ... }
    // PORT NOTE: the dependent `key.Type()` return is the matching
    // `OptionValue` variant (see `OptionValue`).
    pub fn default(self) -> OptionValue<'static> {
        match self {
            // .a => .default,
            Option_::A => OptionValue::Action(Action::DEFAULT),
            // .c => false,
            Option_::C => OptionValue::Bool(false),
            // .d => true,
            Option_::D => OptionValue::Bool(true),
            // .e => false,
            Option_::E => OptionValue::Bool(false),
            // .f => null, .g => null, .i => null,
            Option_::F | Option_::G | Option_::I => OptionValue::OptBytes(None),
            // .n => unreachable,
            Option_::N => unreachable!(),
            // .o => .default,
            Option_::O => OptionValue::Occasion(Occasion::DEFAULT),
            // .p => .default,
            Option_::P => OptionValue::Payload(Payload::DEFAULT),
            // .s => "system",
            Option_::S => OptionValue::Bytes(b"system"),
            // .t => unreachable,
            Option_::T => unreachable!(),
            // .u => .default,
            Option_::U => OptionValue::Urgency(Urgency::DEFAULT),
            // .w => -1,
            Option_::W => OptionValue::Int(-1),
        }
    }

    /// Read the option value from the raw metadata string.
    ///
    /// Unknown and malformed values are ignored. Optional values return null;
    /// all other values return the protocol default.
    pub fn read<'a>(self, metadata: &'a [u8]) -> OptionValue<'a> {
        // var it: kitty_metadata.ValueIterator(
        //     @tagName(key),
        //     valid_metadata_value_characters,
        // ) = switch (key) {
        //     .t, .n => return .init(metadata),
        //     else => .init(metadata),
        // };
        let mut it = kitty_metadata::ValueIterator::init(
            self.tag_name(),
            Some(VALID_METADATA_VALUE_CHARACTERS),
            metadata,
        );
        match self {
            Option_::T | Option_::N => return OptionValue::Iterator(it),
            _ => {}
        }

        // const value = it.next() orelse return key.default();
        let value = match it.next() {
            Some(v) => v,
            None => return self.default(),
        };

        // return the parsed value, the iterator guarantees that it's a valid
        // metadata value
        match self {
            Option_::A => OptionValue::Action(Action::init(value)),
            // .c => parseBool(value) orelse key.default(),  (.c default = false)
            Option_::C => OptionValue::Bool(parse_bool(value).unwrap_or(false)),
            // .d => parseBool(value) orelse key.default(),  (.d default = true)
            Option_::D => OptionValue::Bool(parse_bool(value).unwrap_or(true)),
            // .e => parseBool(value) orelse key.default(),  (.e default = false)
            Option_::E => OptionValue::Bool(parse_bool(value).unwrap_or(false)),
            // .f => value,
            Option_::F => OptionValue::OptBytes(Some(value)),
            // .g => parseIdentifier(value),
            Option_::G => OptionValue::OptBytes(parse_identifier(value)),
            // .i => parseIdentifier(value),
            Option_::I => OptionValue::OptBytes(parse_identifier(value)),
            // .n => unreachable,
            Option_::N => unreachable!(),
            Option_::O => OptionValue::Occasion(Occasion::init(value)),
            Option_::P => OptionValue::Payload(Payload::init(value)),
            // .s => value,
            Option_::S => OptionValue::Bytes(value),
            // .t => unreachable,
            Option_::T => unreachable!(),
            Option_::U => OptionValue::Urgency(Urgency::init(value)),
            Option_::W => OptionValue::Int('value: {
                // Zig's integer parser allows '_', we don't
                if value.contains(&b'_') {
                    // break :value key.default();  (.w default = -1)
                    break 'value -1;
                }
                // const tmp = std.fmt.parseInt(i32, value, 10) catch break :value key.default();
                // PORT NOTE: `i32::from_str` mirrors Zig's base-10 parse here —
                // optional +/- sign, overflow and empty input error (→ default).
                let tmp: i32 = match std::str::from_utf8(value)
                    .ok()
                    .and_then(|s| s.parse::<i32>().ok())
                {
                    Some(v) => v,
                    None => break 'value -1,
                };
                // negative values less than -1 are not allowed
                if tmp < -1 {
                    // break :value key.default();
                    break 'value -1;
                }
                tmp
            }),
        }
    }
}

/// Parse the protocol's booleans
fn parse_bool(str: &[u8]) -> Option<bool> {
    if str.len() != 1 {
        return None;
    }
    match str[0] {
        b'0' => Some(false),
        b'1' => Some(true),
        _ => None,
    }
}

/// This is similar to the packed struct parser used in the configs. The
/// differences are that a literal `true` or `false` value does not turn on/off
/// all the values, and the negation prefix is `-` not `no-`.
// fn parsePackedStruct(comptime T: type, str: []const u8) T {
//     const info = @typeInfo(T).@"struct";
//     comptime assert(info.layout == .@"packed");
//     ...
// }
// PORT NOTE (R8/D4): the reference is comptime-generic over any packed struct
// of bools; `Action` is its ONLY instantiation in this protocol, so the port
// specializes to `Action` with the explicit field arms below standing in for
// the `inline for (info.fields)` walk (declaration order: focus, report; the
// `comptime assert(field.type == bool)` holds by construction). No runtime
// bytes reach the comptime grammar — the field names are the fixed identifiers
// of the struct.
fn parse_packed_struct(str: &[u8]) -> Action {
    // var result: T = .default;
    let mut result: Action = Action::DEFAULT;

    // We split each value by ","
    // var iter = std.mem.splitSequence(u8, str, ",");
    'l: for raw in str.split(|&b| b == b',') {
        // Determine the field we're looking for and the value. If the
        // field is prefixed with "-" then we set the value to false.
        let (part, value): (&[u8], bool) = {
            let negation_prefix: &[u8] = b"-";
            let trimmed = trim_whitespace(raw);
            if trimmed.starts_with(negation_prefix) {
                (&trimmed[negation_prefix.len()..], false)
            } else {
                (trimmed, true)
            }
        };

        // inline for (info.fields) |field| {
        //     comptime assert(field.type == bool);
        //     if (std.mem.eql(u8, field.name, part)) {
        //         @field(result, field.name) = value;
        //         continue :loop;
        //     }
        // }
        if part == b"focus" {
            result.focus = value;
            continue 'l;
        }
        if part == b"report" {
            result.report = value;
            continue 'l;
        }

        // No field matched
        return Action::DEFAULT;
    }

    result
}

/// Characters that are valid in a metadata value. Including `=` is technically
/// against the spec but is needed since Base64 encoded values (with padding)
/// are valid for some options. Including `?` is technically against the spec
/// but is needed since it is a valid value for the `p` option.
// const valid_metadata_value_characters: []const u8 =
//     valid_identifier_characters ++ "/,(){}[]*&^%$#@!`~=?";
// PORT NOTE (R8 derived constant): the `++` concatenation is reproduced as a
// const evaluation over the same two operands, never a folded literal.
const VALID_METADATA_EXTRA_CHARACTERS: &[u8] = b"/,(){}[]*&^%$#@!`~=?";
const VALID_METADATA_VALUE_CHARACTERS_ARR: [u8; VALID_IDENTIFIER_CHARACTERS.len()
    + VALID_METADATA_EXTRA_CHARACTERS.len()] = {
    let mut out = [0u8; VALID_IDENTIFIER_CHARACTERS.len() + VALID_METADATA_EXTRA_CHARACTERS.len()];
    let mut i = 0;
    while i < VALID_IDENTIFIER_CHARACTERS.len() {
        out[i] = VALID_IDENTIFIER_CHARACTERS[i];
        i += 1;
    }
    let mut j = 0;
    while j < VALID_METADATA_EXTRA_CHARACTERS.len() {
        out[VALID_IDENTIFIER_CHARACTERS.len() + j] = VALID_METADATA_EXTRA_CHARACTERS[j];
        j += 1;
    }
    out
};
const VALID_METADATA_VALUE_CHARACTERS: &[u8] = &VALID_METADATA_VALUE_CHARACTERS_ARR;

/// Characters that are valid in identifiers.
const VALID_IDENTIFIER_CHARACTERS: &[u8] =
    b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_+.";

fn is_valid_identifier(str: &[u8]) -> bool {
    // return std.mem.indexOfNone(u8, str, valid_identifier_characters) == null;
    // (NOTE: unlike the OSC 5522 parser's variant, the empty string IS a valid
    // identifier here — see the `i 3`/`i 4` tests.)
    str.iter().all(|b| VALID_IDENTIFIER_CHARACTERS.contains(b))
}

fn parse_identifier(str: &[u8]) -> Option<&[u8]> {
    if is_valid_identifier(str) {
        return Some(str);
    }
    None
}

// PORT NOTE: `std.mem.trim(u8, raw, &std.ascii.whitespace)` — the exact Zig
// whitespace set (space, \t, \n, \r, VT 0x0B, FF 0x0C).
const ASCII_WHITESPACE: &[u8] = &[b' ', b'\t', b'\n', b'\r', 0x0B, 0x0C];

fn trim_whitespace(s: &[u8]) -> &[u8] {
    let mut start = 0;
    let mut end = s.len();
    while start < end && ASCII_WHITESPACE.contains(&s[start]) {
        start += 1;
    }
    while end > start && ASCII_WHITESPACE.contains(&s[end - 1]) {
        end -= 1;
    }
    &s[start..end]
}

pub fn parse<'a>(parser: &'a mut Parser, terminator_ch: Option<u8>) -> Option<&'a mut Command> {
    // assert(parser.state == .@"99");
    debug_assert!(parser.state == State::S99);

    // const cap = if (parser.capture) |*c| c else {
    //     parser.state = .invalid;
    //     return null;
    // };
    //
    // const data = cap.trailing();
    let data: &[u8] = {
        let cap = if let Some(c) = parser.capture.as_ref() {
            c
        } else {
            parser.state = State::Invalid;
            return None;
        };

        cap.trailing(&parser.buffer)
    };

    // const payload_start = std.mem.indexOfScalar(u8, data, ';') orelse { ... };
    let payload_start = match data.iter().position(|&b| b == b';') {
        Some(i) => i,
        None => {
            // log.warn("missing semicolon before payload", .{});
            parser.state = State::Invalid;
            return None;
        }
    };

    let metadata = &data[0..payload_start];
    let payload = &data[payload_start + 1..data.len()];

    let max_payload_bytes: usize = if Option_::E.read(metadata).as_bool() {
        MAX_ENCODED_PAYLOAD_BYTES
    } else {
        MAX_PLAIN_PAYLOAD_BYTES
    };
    if payload.len() > max_payload_bytes {
        // log.warn(
        //     "payload is too large: size={d} max={d}",
        //     .{ payload.len, max_payload_bytes },
        // );
        parser.state = State::Invalid;
        return None;
    }

    // Payload has to be an escape-code-safe UTF-8 string.
    if !encoding::is_safe_utf8(payload) {
        // log.warn("payload is not escape code safe UTF-8", .{});
        parser.state = State::Invalid;
        return None;
    }

    // PORT NOTE (fidelity): the reference `metadata`/`payload` are `[]const u8`
    // slices borrowing the capture; the ported `OSC` owns copies of the RAW
    // bytes (the established convention of the OSC 5522/72 parser ports) —
    // NEVER a lossy String decode.
    let metadata = metadata.to_vec();
    let payload = payload.to_vec();

    parser.command = Command::KittyDesktopNotification(OSC {
        metadata,
        payload,
        terminator: Terminator::init(terminator_ch),
    });

    Some(&mut parser.command)
}

#[cfg(test)]
mod tests {
    use super::{Action, Occasion, Option_, Payload, Urgency, OSC};
    use super::{MAX_ENCODED_PAYLOAD_BYTES, MAX_PLAIN_PAYLOAD_BYTES};
    use crate::osc::{Command, Parser, Terminator};

    // PORT NOTE: most reference tests use `Parser.init(null)` (no allocator);
    // the S99 allocating capture falls back to the fixed buffer, which is big
    // enough for these inputs. The size-limits test passes an allocator like
    // the reference passes `testing.allocator`.

    /// Drive `input` through a fresh parser and return the parsed
    /// `kitty_desktop_notification` command (the shared preamble of the
    /// reference tests: `var p: Parser = .init(null); for (input) |ch|
    /// p.next(ch); const cmd = p.end('\x1b').?.*;`).
    fn parse_kdn(input: &str) -> OSC {
        let mut p: Parser = Parser::init(None);

        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        // try testing.expect(cmd == .kitty_desktop_notification);
        assert!(matches!(cmd, Command::KittyDesktopNotification(_)));
        match cmd {
            Command::KittyDesktopNotification(v) => v,
            _ => unreachable!(),
        }
    }

    // test "OSC 99: empty metadata and payload" {
    #[test]
    fn osc_99_empty_metadata_and_payload() {
        let kdn = parse_kdn("99;;");
        assert_eq!(b"".as_slice(), kdn.metadata.as_slice());
        assert_eq!(b"".as_slice(), kdn.payload.as_slice());
        // try testing.expectEqualDeep(Option.a.default(), cmd...readOption(.a));
        assert_eq!(Option_::A.default(), kdn.read_option(Option_::A));
        assert!(!kdn.read_option(Option_::C).as_bool());
        assert!(kdn.read_option(Option_::D).as_bool());
        assert!(!kdn.read_option(Option_::E).as_bool());
        assert!(kdn.read_option(Option_::F).as_opt_bytes().is_none());
        assert!(kdn.read_option(Option_::G).as_opt_bytes().is_none());
        assert!(kdn.read_option(Option_::I).as_opt_bytes().is_none());
        {
            let mut it = kdn.read_option(Option_::N).into_iterator();
            assert!(it.next().is_none());
        }
        assert_eq!(Occasion::Always, kdn.read_option(Option_::O).as_occasion());
        assert_eq!(Payload::Title, kdn.read_option(Option_::P).as_payload());
        assert_eq!(b"system".as_slice(), kdn.read_option(Option_::S).as_bytes());
        {
            let mut it = kdn.read_option(Option_::T).into_iterator();
            assert!(it.next().is_none());
        }
        assert_eq!(Urgency::Normal, kdn.read_option(Option_::U).as_urgency());
        assert_eq!(-1, kdn.read_option(Option_::W).as_int());
        assert_eq!(Terminator::St, kdn.terminator);
    }

    // test "OSC 99: empty metadata with payload" {
    #[test]
    fn osc_99_empty_metadata_with_payload() {
        let kdn = parse_kdn("99;;bobr");
        assert_eq!(b"".as_slice(), kdn.metadata.as_slice());
        assert_eq!(b"bobr".as_slice(), kdn.payload.as_slice());
        assert_eq!(Option_::A.default(), kdn.read_option(Option_::A));
        assert!(!kdn.read_option(Option_::C).as_bool());
        assert!(kdn.read_option(Option_::D).as_bool());
        assert!(!kdn.read_option(Option_::E).as_bool());
        assert!(kdn.read_option(Option_::F).as_opt_bytes().is_none());
        assert!(kdn.read_option(Option_::G).as_opt_bytes().is_none());
        assert!(kdn.read_option(Option_::I).as_opt_bytes().is_none());
        {
            let mut it = kdn.read_option(Option_::N).into_iterator();
            assert!(it.next().is_none());
        }
        assert_eq!(Occasion::Always, kdn.read_option(Option_::O).as_occasion());
        assert_eq!(Payload::Title, kdn.read_option(Option_::P).as_payload());
        assert_eq!(b"system".as_slice(), kdn.read_option(Option_::S).as_bytes());
        {
            let mut it = kdn.read_option(Option_::T).into_iterator();
            assert!(it.next().is_none());
        }
        assert_eq!(Urgency::Normal, kdn.read_option(Option_::U).as_urgency());
        assert_eq!(-1, kdn.read_option(Option_::W).as_int());
    }

    // test "OSC 99: payload size limits" {
    #[test]
    fn osc_99_payload_size_limits() {
        struct Case {
            metadata: &'static str,
            payload_size: usize,
            valid: bool,
        }
        let cases = [
            Case {
                metadata: "",
                payload_size: MAX_PLAIN_PAYLOAD_BYTES,
                valid: true,
            },
            Case {
                metadata: "",
                payload_size: MAX_PLAIN_PAYLOAD_BYTES + 1,
                valid: false,
            },
            Case {
                metadata: "e=1",
                payload_size: MAX_ENCODED_PAYLOAD_BYTES,
                valid: true,
            },
            Case {
                metadata: "e=1",
                payload_size: MAX_ENCODED_PAYLOAD_BYTES + 1,
                valid: false,
            },
        ];

        for case in cases {
            // var p: Parser = .init(testing.allocator);
            // defer p.deinit();
            let backend = rustty_lib::allocator::GlobalZigAllocator;
            let alloc = rustty_lib::allocator::ZigAllocatorRef::new(&backend);
            let mut p: Parser = Parser::init(Some(alloc));

            for ch in "99;".bytes() {
                p.next(ch);
            }
            for ch in case.metadata.bytes() {
                p.next(ch);
            }
            p.next(b';');
            for _ in 0..case.payload_size {
                p.next(b'a');
            }

            // try testing.expectEqual(case.valid, p.end('\x1b') != null);
            assert_eq!(case.valid, p.end(Some(0x1b)).is_some());
            p.deinit();
        }
    }

    // test "OSC 99: unknown prefix does not hide dotted identifier" {
    #[test]
    fn osc_99_unknown_prefix_does_not_hide_dotted_identifier() {
        let kdn = parse_kdn("99;invalid=wrong:i=org.ghostty;payload");
        // try testing.expectEqualStrings("org.ghostty", cmd...readOption(.i).?);
        assert_eq!(
            b"org.ghostty".as_slice(),
            kdn.read_option(Option_::I).as_opt_bytes().unwrap()
        );
    }

    // test "OSC 99: single parameter i" {
    #[test]
    fn osc_99_single_parameter_i() {
        let kdn = parse_kdn("99;i=bobr;payload");
        assert_eq!(
            b"bobr".as_slice(),
            kdn.read_option(Option_::I).as_opt_bytes().unwrap()
        );
        assert_eq!(b"payload".as_slice(), kdn.payload.as_slice());
    }

    // test "OSC 99: repeated parameter i" {
    #[test]
    fn osc_99_repeated_parameter_i() {
        let kdn = parse_kdn("99;i=bobr:i=foobar;payload");
        assert_eq!(
            b"bobr".as_slice(),
            kdn.read_option(Option_::I).as_opt_bytes().unwrap()
        );
        assert_eq!(b"payload".as_slice(), kdn.payload.as_slice());
    }

    // test "OSC 99: multiple types" {
    #[test]
    fn osc_99_multiple_types() {
        let kdn = parse_kdn("99;t=mail: t = chat : t = alert ;notification");
        assert_eq!(b"notification".as_slice(), kdn.payload.as_slice());
        let mut it = kdn.read_option(Option_::T).into_iterator();
        assert_eq!(b"mail".as_slice(), it.next().unwrap());
        assert_eq!(b"chat".as_slice(), it.next().unwrap());
        assert_eq!(b"alert".as_slice(), it.next().unwrap());
        assert!(it.next().is_none());
    }

    // test "OSC 99: a 1" {
    #[test]
    fn osc_99_a_1() {
        let kdn = parse_kdn("99;a=report,focus;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        // try testing.expectEqualDeep(Action{ .report = true, .focus = true }, ...);
        assert_eq!(
            Action {
                report: true,
                focus: true
            },
            kdn.read_option(Option_::A).as_action()
        );
    }

    // test "OSC 99: a 2" {
    #[test]
    fn osc_99_a_2() {
        let kdn = parse_kdn("99;a=report,-focus;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        assert_eq!(
            Action {
                report: true,
                focus: false
            },
            kdn.read_option(Option_::A).as_action()
        );
    }

    // test "OSC 99: a 3" {
    #[test]
    fn osc_99_a_3() {
        let kdn = parse_kdn("99;a=-report,focus;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        assert_eq!(
            Action {
                report: false,
                focus: true
            },
            kdn.read_option(Option_::A).as_action()
        );
    }

    // test "OSC 99: a 4" {
    #[test]
    fn osc_99_a_4() {
        let kdn = parse_kdn("99;a=-report,-focus;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        assert_eq!(
            Action {
                report: false,
                focus: false
            },
            kdn.read_option(Option_::A).as_action()
        );
    }

    // test "OSC 99: c 1" {
    #[test]
    fn osc_99_c_1() {
        let kdn = parse_kdn("99;c=0;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        assert!(!kdn.read_option(Option_::C).as_bool());
    }

    // test "OSC 99: c 2" {
    #[test]
    fn osc_99_c_2() {
        let kdn = parse_kdn("99;c=1;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        assert!(kdn.read_option(Option_::C).as_bool());
    }

    // test "OSC 99: c 3" {
    #[test]
    fn osc_99_c_3() {
        let kdn = parse_kdn("99;c=bobr;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        assert!(!kdn.read_option(Option_::C).as_bool());
    }

    // test "OSC 99: d 1" {
    #[test]
    fn osc_99_d_1() {
        let kdn = parse_kdn("99;d=0;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        assert!(!kdn.read_option(Option_::D).as_bool());
    }

    // test "OSC 99: d 2" {
    #[test]
    fn osc_99_d_2() {
        let kdn = parse_kdn("99;d=1;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        assert!(kdn.read_option(Option_::D).as_bool());
    }

    // test "OSC 99: d 3" {
    #[test]
    fn osc_99_d_3() {
        let kdn = parse_kdn("99;d=bobr;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        assert!(kdn.read_option(Option_::D).as_bool());
    }

    // test "OSC 99: e 1" {
    #[test]
    fn osc_99_e_1() {
        let kdn = parse_kdn("99;e=0;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        assert!(!kdn.read_option(Option_::E).as_bool());
    }

    // test "OSC 99: e 2" {
    #[test]
    fn osc_99_e_2() {
        let kdn = parse_kdn("99;e=1;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        assert!(kdn.read_option(Option_::E).as_bool());
    }

    // test "OSC 99: e 3" {
    #[test]
    fn osc_99_e_3() {
        let kdn = parse_kdn("99;e=bobr;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        assert!(!kdn.read_option(Option_::E).as_bool());
    }

    // test "OSC 99: f 1" {
    #[test]
    fn osc_99_f_1() {
        let kdn = parse_kdn("99;f=R2hvc3R0eQ==;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        assert_eq!(
            b"R2hvc3R0eQ==".as_slice(),
            kdn.read_option(Option_::F).as_opt_bytes().unwrap()
        );
    }

    // test "OSC 99: f 2" {
    #[test]
    fn osc_99_f_2() {
        let kdn = parse_kdn("99;c=0:f= R2hvc3R0eQ== ;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        assert_eq!(
            b"R2hvc3R0eQ==".as_slice(),
            kdn.read_option(Option_::F).as_opt_bytes().unwrap()
        );
    }

    // test "OSC 99: g 1" {
    #[test]
    fn osc_99_g_1() {
        let kdn = parse_kdn("99;c=0:g=7f8a9129-a35d-4e9f-8043-ce2700e15e2c;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        assert_eq!(
            b"7f8a9129-a35d-4e9f-8043-ce2700e15e2c".as_slice(),
            kdn.read_option(Option_::G).as_opt_bytes().unwrap()
        );
    }

    // test "OSC 99: g 2" {
    #[test]
    fn osc_99_g_2() {
        let kdn = parse_kdn("99;c=0:g=aaa*bbb;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        assert!(kdn.read_option(Option_::G).as_opt_bytes().is_none());
    }

    // test "OSC 99: i 1" {
    #[test]
    fn osc_99_i_1() {
        let kdn = parse_kdn("99;;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        assert!(kdn.read_option(Option_::I).as_opt_bytes().is_none());
    }

    // test "OSC 99: i 2" {
    #[test]
    fn osc_99_i_2() {
        let kdn = parse_kdn("99;i=bobr;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        assert_eq!(
            b"bobr".as_slice(),
            kdn.read_option(Option_::I).as_opt_bytes().unwrap()
        );
    }

    // test "OSC 99: i 3" {
    #[test]
    fn osc_99_i_3() {
        let kdn = parse_kdn("99;i=;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        assert_eq!(
            b"".as_slice(),
            kdn.read_option(Option_::I).as_opt_bytes().unwrap()
        );
    }

    // test "OSC 99: i 4" {
    #[test]
    fn osc_99_i_4() {
        let kdn = parse_kdn("99;i= :;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        assert_eq!(
            b"".as_slice(),
            kdn.read_option(Option_::I).as_opt_bytes().unwrap()
        );
    }

    // test "OSC 99: i 5" {
    #[test]
    fn osc_99_i_5() {
        let kdn = parse_kdn("99;i= bobr ;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        assert_eq!(
            b"bobr".as_slice(),
            kdn.read_option(Option_::I).as_opt_bytes().unwrap()
        );
    }

    // test "OSC 99: i 6" {
    #[test]
    fn osc_99_i_6() {
        let kdn = parse_kdn("99;i= bobr : i=kurwa ;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        assert_eq!(
            b"bobr".as_slice(),
            kdn.read_option(Option_::I).as_opt_bytes().unwrap()
        );
    }

    // test "OSC 99: n 1" {
    #[test]
    fn osc_99_n_1() {
        let kdn = parse_kdn("99;n=R2hvc3R0eQ==;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        let mut it = kdn.read_option(Option_::N).into_iterator();
        assert_eq!(b"R2hvc3R0eQ==".as_slice(), it.next().unwrap());
        assert!(it.next().is_none());
    }

    // test "OSC 99: n 2" {
    #[test]
    fn osc_99_n_2() {
        let kdn = parse_kdn("99;n=R2hvc3R0eQ==:n=R2hvc3R0eQ==;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        let mut it = kdn.read_option(Option_::N).into_iterator();
        assert_eq!(b"R2hvc3R0eQ==".as_slice(), it.next().unwrap());
        assert_eq!(b"R2hvc3R0eQ==".as_slice(), it.next().unwrap());
        assert!(it.next().is_none());
    }

    // test "OSC 99: o 1" {
    #[test]
    fn osc_99_o_1() {
        let kdn = parse_kdn("99;o= ;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        assert_eq!(Occasion::Always, kdn.read_option(Option_::O).as_occasion());
    }

    // test "OSC 99: o 2" {
    #[test]
    fn osc_99_o_2() {
        let kdn = parse_kdn("99;o=always;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        assert_eq!(Occasion::Always, kdn.read_option(Option_::O).as_occasion());
    }

    // test "OSC 99: o 3" {
    #[test]
    fn osc_99_o_3() {
        let kdn = parse_kdn("99;o=unfocused;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        assert_eq!(
            Occasion::Unfocused,
            kdn.read_option(Option_::O).as_occasion()
        );
    }

    // test "OSC 99: o 4" {
    #[test]
    fn osc_99_o_4() {
        let kdn = parse_kdn("99;o=invisible;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        assert_eq!(
            Occasion::Invisible,
            kdn.read_option(Option_::O).as_occasion()
        );
    }

    // test "OSC 99: o 5" {
    #[test]
    fn osc_99_o_5() {
        let kdn = parse_kdn("99;o=bobr;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        assert_eq!(Occasion::Always, kdn.read_option(Option_::O).as_occasion());
    }

    // test "OSC 99: p 1" {
    #[test]
    fn osc_99_p_1() {
        let kdn = parse_kdn("99;p=alive;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        assert_eq!(Payload::Alive, kdn.read_option(Option_::P).as_payload());
    }

    // test "OSC 99: p 2" {
    #[test]
    fn osc_99_p_2() {
        let kdn = parse_kdn("99;p=body;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        assert_eq!(Payload::Body, kdn.read_option(Option_::P).as_payload());
    }

    // test "OSC 99: p 3" {
    #[test]
    fn osc_99_p_3() {
        let kdn = parse_kdn("99;p=buttons;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        assert_eq!(Payload::Buttons, kdn.read_option(Option_::P).as_payload());
    }

    // test "OSC 99: p 4" {
    #[test]
    fn osc_99_p_4() {
        let kdn = parse_kdn("99;p=close;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        assert_eq!(Payload::Close, kdn.read_option(Option_::P).as_payload());
    }

    // test "OSC 99: p 5" {
    #[test]
    fn osc_99_p_5() {
        let kdn = parse_kdn("99;p=icon;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        assert_eq!(Payload::Icon, kdn.read_option(Option_::P).as_payload());
    }

    // test "OSC 99: p 6" {
    #[test]
    fn osc_99_p_6() {
        let kdn = parse_kdn("99;p=?;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        assert_eq!(Payload::Query, kdn.read_option(Option_::P).as_payload());
    }

    // test "OSC 99: p 7" {
    #[test]
    fn osc_99_p_7() {
        let kdn = parse_kdn("99;p=title;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        assert_eq!(Payload::Title, kdn.read_option(Option_::P).as_payload());
    }

    // test "OSC 99: p 8" {
    #[test]
    fn osc_99_p_8() {
        let kdn = parse_kdn("99;p=query;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        assert_eq!(Payload::Unknown, kdn.read_option(Option_::P).as_payload());
    }

    // test "OSC 99: p 9" {
    #[test]
    fn osc_99_p_9() {
        let kdn = parse_kdn("99;p=bobr;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        assert_eq!(Payload::Unknown, kdn.read_option(Option_::P).as_payload());
    }

    // test "OSC 99: s 1" {
    #[test]
    fn osc_99_s_1() {
        let kdn = parse_kdn("99;s=R2hvc3R0eQ==;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        assert_eq!(
            b"R2hvc3R0eQ==".as_slice(),
            kdn.read_option(Option_::S).as_bytes()
        );
    }

    // test "OSC 99: t 1" {
    #[test]
    fn osc_99_t_1() {
        let kdn = parse_kdn("99;t=R2hvc3R0eQ==;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        let mut it = kdn.read_option(Option_::T).into_iterator();
        assert_eq!(b"R2hvc3R0eQ==".as_slice(), it.next().unwrap());
        assert!(it.next().is_none());
    }

    // test "OSC 99: t 2" {
    #[test]
    fn osc_99_t_2() {
        let kdn = parse_kdn("99;t=R2hvc3R0eQ==:t=R2hvc3R0eQ==;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        let mut it = kdn.read_option(Option_::T).into_iterator();
        assert_eq!(b"R2hvc3R0eQ==".as_slice(), it.next().unwrap());
        assert_eq!(b"R2hvc3R0eQ==".as_slice(), it.next().unwrap());
        assert!(it.next().is_none());
    }

    // test "OSC 99: u 1" {
    #[test]
    fn osc_99_u_1() {
        let kdn = parse_kdn("99;u=0;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        assert_eq!(Urgency::Low, kdn.read_option(Option_::U).as_urgency());
    }

    // test "OSC 99: u 2" {
    #[test]
    fn osc_99_u_2() {
        let kdn = parse_kdn("99;u=1;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        assert_eq!(Urgency::Normal, kdn.read_option(Option_::U).as_urgency());
    }

    // test "OSC 99: u 3" {
    #[test]
    fn osc_99_u_3() {
        let kdn = parse_kdn("99;u=2;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        assert_eq!(Urgency::High, kdn.read_option(Option_::U).as_urgency());
    }

    // test "OSC 99: u 4" {
    #[test]
    fn osc_99_u_4() {
        let kdn = parse_kdn("99;u=bobr;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        assert_eq!(Urgency::Normal, kdn.read_option(Option_::U).as_urgency());
    }

    // test "OSC 99: w 1" {
    #[test]
    fn osc_99_w_1() {
        let kdn = parse_kdn("99;w=0;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        assert_eq!(0, kdn.read_option(Option_::W).as_int());
    }

    // test "OSC 99: w 2" {
    #[test]
    fn osc_99_w_2() {
        let kdn = parse_kdn("99;w=-1;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        assert_eq!(-1, kdn.read_option(Option_::W).as_int());
    }

    // test "OSC 99: w 3" {
    #[test]
    fn osc_99_w_3() {
        let kdn = parse_kdn("99;w=-42;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        assert_eq!(-1, kdn.read_option(Option_::W).as_int());
    }

    // test "OSC 99: w 4" {
    #[test]
    fn osc_99_w_4() {
        let kdn = parse_kdn("99;w=4294967296;foobar");
        assert_eq!(b"foobar".as_slice(), kdn.payload.as_slice());
        assert_eq!(-1, kdn.read_option(Option_::W).as_int());
    }
}
