//! Kitty's clipboard protocol (OSC 5522)
//! Specification: https://sw.kovidgoyal.net/kitty/clipboard/
//! https://rockorager.dev/misc/bracketed-paste-mime/
//!
//! PORTS: ghostty src/terminal/osc/parsers/kitty_clipboard_protocol.zig
//!
//! PORT STATUS: ported — full parse logic + `readOption`/`Option`/`Location`/
//! `Operation`/`Status` machinery + tests (vt-6).

// PORT NOTE: `const std = @import("std");` — std facilities are used inline.
// PORT NOTE: `const build_options = @import("terminal_options");` — unused in
// this module's logic; ported as this note.
// PORT NOTE: `const assert = @import("../../../quirks.zig").inlineAssert;` —
// retargeted to `debug_assert!` at the call site (matching the convention used
// by the other ported OSC parsers).
// PORT NOTE: `const Parser = @import("../../osc.zig").Parser;`
// PORT NOTE: `const Command = @import("../../osc.zig").Command;`
// PORT NOTE: `const Terminator = @import("../../osc.zig").Terminator;`
use crate::osc::{Command, Parser, State, Terminator};

// PORT NOTE: `const encoding = @import("../encoding.zig");` — imported by the
// reference but not referenced in this module's logic; ported as this note.

// PORT NOTE: `const log = std.log.scoped(.kitty_clipboard_protocol);` — Rustty
// has no logging facade wired into this crate (matching the convention used by
// the `osc.zig` scoped logger), so the scoped logger is ported as a commented
// line.
// const log = std.log.scoped(.kitty_clipboard_protocol);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OSC {
    /// The raw metadata that was received. It can be parsed by using the `readOption` method.
    ///
    /// PORT NOTE (fidelity): reference `metadata: []const u8` — RAW bytes.
    pub metadata: Vec<u8>,
    /// The raw payload. It may be Base64 encoded, check the `e` option.
    ///
    /// PORT NOTE (fidelity): reference `payload: ?[]const u8` — optional RAW bytes.
    pub payload: Option<Vec<u8>>,
    /// The terminator that was used in case we need to send a response.
    pub terminator: Terminator,
}

impl OSC {
    /// Decode an option from the metadata.
    // PORT NOTE: the reference uses a `comptime key: Option` parameter with a
    // dependent return type `key.Type()`. Rust has no comptime-dependent return
    // types, so `read_option` takes a runtime `Option` and returns a tagged
    // `OptionValue` whose active variant matches `Option::value_type`.
    pub fn read_option(&self, key: Option_) -> OptionValue {
        key.read(&self.metadata)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Location {
    Primary,
}

impl Location {
    pub fn init(str: &[u8]) -> Option<Location> {
        // PORT NOTE: `std.meta.stringToEnum(Location, str)` — match each tag
        // name verbatim against the value bytes.
        match str {
            b"primary" => Some(Location::Primary),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Read,
    Walias,
    Wdata,
    Write,
}

impl Operation {
    pub fn init(str: &[u8]) -> Option<Operation> {
        // PORT NOTE: `std.meta.stringToEnum(Operation, str)`.
        match str {
            b"read" => Some(Operation::Read),
            b"walias" => Some(Operation::Walias),
            b"wdata" => Some(Operation::Wdata),
            b"write" => Some(Operation::Write),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Data,
    Done,
    Ebusy,
    Einval,
    Eio,
    Enosys,
    Eperm,
    Ok,
}

impl Status {
    pub fn init(str: &[u8]) -> Option<Status> {
        // PORT NOTE: `std.meta.stringToEnum(Status, str)` — the Zig tag names
        // are upper-case (DATA, DONE, EBUSY, ...); match those spellings
        // verbatim against the value bytes.
        match str {
            b"DATA" => Some(Status::Data),
            b"DONE" => Some(Status::Done),
            b"EBUSY" => Some(Status::Ebusy),
            b"EINVAL" => Some(Status::Einval),
            b"EIO" => Some(Status::Eio),
            b"ENOSYS" => Some(Status::Enosys),
            b"EPERM" => Some(Status::Eperm),
            b"OK" => Some(Status::Ok),
            _ => None,
        }
    }
}

// PORT NOTE: `pub const Option = enum { ... }` — Rust reserves `Option` for the
// std optional type, so the ported enum is named `Option_` (the value type
// dispatched on by `read_option`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Option_ {
    Id,
    Loc,
    Mime,
    Name,
    Password,
    Pw,
    Status,
    Type,
}

/// The dependent value produced by `Option_::read`. The active variant matches
/// the `Option_::Type()` of the option that produced it.
// PORT NOTE: this enum stands in for the comptime-dependent `key.Type()` return
// type of `Option.read` in the reference:
//   .id       => ?[]const u8
//   .loc      => ?Location
//   .mime     => ?[]const u8
//   .name     => ?[]const u8
//   .password => ?[]const u8
//   .pw       => ?[]const u8
//   .status   => ?Status
//   .type     => ?Operation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OptionValue {
    /// PORT NOTE (fidelity): the `[]const u8` option types are RAW bytes.
    Str(Option<Vec<u8>>),
    Loc(Option<Location>),
    Status(Option<Status>),
    Type(Option<Operation>),
}

impl OptionValue {
    /// Helper for `?[]const u8` options.
    ///
    /// PORT NOTE (fidelity): returns the RAW bytes, not a UTF-8 `str`.
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            OptionValue::Str(v) => v.as_deref(),
            _ => panic!("option value is not a string"),
        }
    }

    /// Helper for `?Location` options.
    pub fn as_loc(&self) -> Option<Location> {
        match self {
            OptionValue::Loc(v) => *v,
            _ => panic!("option value is not a location"),
        }
    }

    /// Helper for `?Status` options.
    pub fn as_status(&self) -> Option<Status> {
        match self {
            OptionValue::Status(v) => *v,
            _ => panic!("option value is not a status"),
        }
    }

    /// Helper for `?Operation` options.
    pub fn as_type(&self) -> Option<Operation> {
        match self {
            OptionValue::Type(v) => *v,
            _ => panic!("option value is not an operation"),
        }
    }

    /// Whether the optional option value is absent (`null`).
    pub fn is_null(&self) -> bool {
        match self {
            OptionValue::Str(v) => v.is_none(),
            OptionValue::Loc(v) => v.is_none(),
            OptionValue::Status(v) => v.is_none(),
            OptionValue::Type(v) => v.is_none(),
        }
    }
}

impl Option_ {
    // PORT NOTE: `@tagName(key)` — the option's metadata key spelling.
    fn tag_name(self) -> &'static [u8] {
        match self {
            Option_::Id => b"id",
            Option_::Loc => b"loc",
            Option_::Mime => b"mime",
            Option_::Name => b"name",
            Option_::Password => b"password",
            Option_::Pw => b"pw",
            Option_::Status => b"status",
            Option_::Type => b"type",
        }
    }

    /// Read the option value from the raw metadata string.
    pub fn read(self, metadata: &[u8]) -> OptionValue {
        // PORT NOTE: the dependent `key.Type()` is selected at the end via the
        // `OptionValue` variant matching `self`; the `null`/not-found path
        // (`return null;`) maps to the corresponding variant carrying `None`.
        let value: &[u8] = 'value: {
            let key = self.tag_name();
            let mut pos: usize = 0;
            while pos < metadata.len() {
                // skip any whitespace
                while pos < metadata.len() && metadata[pos].is_ascii_whitespace() {
                    pos += 1;
                }
                // bail if we are out of metadata
                if pos >= metadata.len() {
                    return self.null_value();
                }
                if !metadata[pos..].starts_with(key) {
                    // this isn't the key we are looking for, skip to the next option, or bail if
                    // there is no next option
                    pos = match metadata[pos..].iter().position(|&b| b == b':') {
                        Some(rel) => pos + rel,
                        None => return self.null_value(),
                    };
                    pos += 1;
                    continue;
                }
                // skip past the key
                pos += key.len();
                // skip any whitespace
                while pos < metadata.len() && metadata[pos].is_ascii_whitespace() {
                    pos += 1;
                }
                // bail if we are out of metadata
                if pos >= metadata.len() {
                    return self.null_value();
                }
                // a valid option has an '='
                if metadata[pos] != b'=' {
                    return self.null_value();
                }
                // the end of the value is bounded by a ':' or the end of the metadata
                let end = match metadata[pos..].iter().position(|&b| b == b':') {
                    Some(rel) => pos + rel,
                    None => metadata.len(),
                };
                let start = pos + 1;
                // strip any leading or trailing whitespace
                break 'value trim_whitespace(&metadata[start..end]);
            }
            // the key was not found
            return self.null_value();
        };

        // return the parsed value
        match self {
            Option_::Id => OptionValue::Str(parse_identifier(value)),
            Option_::Loc => OptionValue::Loc(Location::init(value)),
            // PORT NOTE (fidelity): the byte-slice values (`mime`/`name`/
            // `password`/`pw`) are `[]const u8`; the reference exposes the bytes
            // verbatim, so copy the RAW bytes out — NEVER a lossy String decode.
            Option_::Mime => OptionValue::Str(Some(value.to_vec())),
            Option_::Name => OptionValue::Str(Some(value.to_vec())),
            Option_::Password => OptionValue::Str(Some(value.to_vec())),
            Option_::Pw => OptionValue::Str(Some(value.to_vec())),
            Option_::Status => OptionValue::Status(Status::init(value)),
            Option_::Type => OptionValue::Type(Operation::init(value)),
        }
    }

    /// The `null` / not-found value for this option, with the variant matching
    /// `self`'s dependent type.
    // PORT NOTE: `return null;` in the reference — typed per `key.Type()`.
    fn null_value(self) -> OptionValue {
        match self {
            Option_::Id | Option_::Mime | Option_::Name | Option_::Password | Option_::Pw => {
                OptionValue::Str(None)
            }
            Option_::Loc => OptionValue::Loc(None),
            Option_::Status => OptionValue::Status(None),
            Option_::Type => OptionValue::Type(None),
        }
    }
}

/// Characters that are valid in identifiers.
const VALID_IDENTIFIER_CHARACTERS: &[u8] =
    b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_+.";

fn is_valid_identifier(str: &[u8]) -> bool {
    if str.is_empty() {
        return false;
    }
    // PORT NOTE: `std.mem.indexOfNone(u8, str, valid_identifier_characters) ==
    // null` — true when every byte of `str` is in the valid set.
    str.iter().all(|b| VALID_IDENTIFIER_CHARACTERS.contains(b))
}

fn parse_identifier(str: &[u8]) -> Option<Vec<u8>> {
    if is_valid_identifier(str) {
        // PORT NOTE (fidelity): the reference returns the byte slice directly;
        // copy the RAW bytes out (`[]const u8`), NOT a lossy String decode.
        return Some(str.to_vec());
    }
    None
}

// PORT NOTE: `std.mem.trim(u8, slice, &std.ascii.whitespace)` — strip leading
// and trailing ASCII whitespace bytes from the slice.
fn trim_whitespace(s: &[u8]) -> &[u8] {
    let mut start = 0;
    let mut end = s.len();
    while start < end && s[start].is_ascii_whitespace() {
        start += 1;
    }
    while end > start && s[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    &s[start..end]
}

pub fn parse<'a>(parser: &'a mut Parser, terminator_ch: Option<u8>) -> Option<&'a mut Command> {
    // assert(parser.state == .@"5522");
    debug_assert!(parser.state == State::S5522);

    // const cap = if (parser.capture) |*c| c else { ... };
    let data: &[u8] = {
        let cap = if let Some(c) = parser.capture.as_ref() {
            c
        } else {
            parser.state = State::Invalid;
            return None;
        };

        // const data = cap.trailing();
        cap.trailing(&parser.buffer)
    };

    let (metadata, payload): (&[u8], Option<&[u8]>) = 'result: {
        // const start = std.mem.indexOfScalar(u8, data, ';') orelse break :result .{ data, null };
        let start = match data.iter().position(|&b| b == b';') {
            Some(start) => start,
            None => break 'result (data, None),
        };
        // break :result .{ data[0..start], data[start + 1 .. data.len] };
        (&data[0..start], Some(&data[start + 1..data.len()]))
    };

    // PORT NOTE (fidelity): `metadata`/`payload` are `[]const u8` / `?[]const u8`
    // RAW bytes; copy the bytes out, NEVER a lossy String decode.
    let metadata = metadata.to_vec();
    let payload = payload.map(|p| p.to_vec());

    parser.command = Command::KittyClipboardProtocol(OSC {
        metadata,
        payload,
        terminator: Terminator::init(terminator_ch),
    });

    Some(&mut parser.command)
}

#[cfg(test)]
mod tests {
    use super::{Location, Operation, Option_, Status};
    use crate::osc::{Command, Parser};

    // PORT NOTE: the reference uses `testing.allocator`. The ported `OSC`
    // payload is self-owning (`String`s), so the parser needs no allocator for
    // OSC 5522; `Parser::init(None)` is the faithful equivalent.

    #[test]
    fn osc_5522_empty_metadata_and_missing_payload() {
        let mut p: Parser = Parser::init(None);

        let input = "5522;";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::KittyClipboardProtocol(_)));
        let kcp = match cmd {
            Command::KittyClipboardProtocol(kcp) => kcp,
            _ => unreachable!(),
        };
        assert_eq!(b"".as_slice(), kcp.metadata.as_slice());
        assert!(kcp.payload.is_none());
        assert!(kcp.read_option(Option_::Id).is_null());
        assert!(kcp.read_option(Option_::Loc).is_null());
        assert!(kcp.read_option(Option_::Mime).is_null());
        assert!(kcp.read_option(Option_::Name).is_null());
        assert!(kcp.read_option(Option_::Password).is_null());
        assert!(kcp.read_option(Option_::Pw).is_null());
        assert!(kcp.read_option(Option_::Status).is_null());
        assert!(kcp.read_option(Option_::Type).is_null());
    }

    #[test]
    fn osc_5522_empty_metadata_and_empty_payload() {
        let mut p: Parser = Parser::init(None);

        let input = "5522;;";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::KittyClipboardProtocol(_)));
        let kcp = match cmd {
            Command::KittyClipboardProtocol(kcp) => kcp,
            _ => unreachable!(),
        };
        assert_eq!(b"".as_slice(), kcp.metadata.as_slice());
        assert_eq!(b"".as_slice(), kcp.payload.as_ref().unwrap().as_slice());
        assert!(kcp.read_option(Option_::Id).is_null());
        assert!(kcp.read_option(Option_::Loc).is_null());
        assert!(kcp.read_option(Option_::Mime).is_null());
        assert!(kcp.read_option(Option_::Name).is_null());
        assert!(kcp.read_option(Option_::Password).is_null());
        assert!(kcp.read_option(Option_::Pw).is_null());
        assert!(kcp.read_option(Option_::Status).is_null());
        assert!(kcp.read_option(Option_::Type).is_null());
    }

    #[test]
    fn osc_5522_non_empty_metadata_and_payload() {
        let mut p: Parser = Parser::init(None);

        let input = "5522;type=read;dGV4dC9wbGFpbg==";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::KittyClipboardProtocol(_)));
        let kcp = match cmd {
            Command::KittyClipboardProtocol(kcp) => kcp,
            _ => unreachable!(),
        };
        assert_eq!(b"type=read".as_slice(), kcp.metadata.as_slice());
        assert_eq!(
            b"dGV4dC9wbGFpbg==".as_slice(),
            kcp.payload.as_ref().unwrap().as_slice()
        );
        assert!(kcp.read_option(Option_::Id).is_null());
        assert!(kcp.read_option(Option_::Loc).is_null());
        assert!(kcp.read_option(Option_::Mime).is_null());
        assert!(kcp.read_option(Option_::Name).is_null());
        assert!(kcp.read_option(Option_::Password).is_null());
        assert!(kcp.read_option(Option_::Pw).is_null());
        assert!(kcp.read_option(Option_::Status).is_null());
        assert_eq!(
            Operation::Read,
            kcp.read_option(Option_::Type).as_type().unwrap()
        );
    }

    #[test]
    fn osc_5522_empty_id() {
        let mut p: Parser = Parser::init(None);

        let input = "5522;id=";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::KittyClipboardProtocol(_)));
        let kcp = match cmd {
            Command::KittyClipboardProtocol(kcp) => kcp,
            _ => unreachable!(),
        };
        assert!(kcp.read_option(Option_::Id).is_null());
    }

    #[test]
    fn osc_5522_valid_id() {
        let mut p: Parser = Parser::init(None);

        let input = "5522;id=5c076ad9-d36f-4705-847b-d4dbf356cc0d";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::KittyClipboardProtocol(_)));
        let kcp = match cmd {
            Command::KittyClipboardProtocol(kcp) => kcp,
            _ => unreachable!(),
        };
        assert_eq!(
            b"5c076ad9-d36f-4705-847b-d4dbf356cc0d".as_slice(),
            kcp.read_option(Option_::Id).as_bytes().unwrap()
        );
    }

    #[test]
    fn osc_5522_invalid_id() {
        let mut p: Parser = Parser::init(None);

        let input = "5522;id=*42*";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::KittyClipboardProtocol(_)));
        let kcp = match cmd {
            Command::KittyClipboardProtocol(kcp) => kcp,
            _ => unreachable!(),
        };
        assert!(kcp.read_option(Option_::Id).is_null());
    }

    #[test]
    fn osc_5522_invalid_status() {
        let mut p: Parser = Parser::init(None);

        let input = "5522;status=BOBR";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::KittyClipboardProtocol(_)));
        let kcp = match cmd {
            Command::KittyClipboardProtocol(kcp) => kcp,
            _ => unreachable!(),
        };
        assert!(kcp.read_option(Option_::Status).is_null());
    }

    #[test]
    fn osc_5522_valid_status() {
        let mut p: Parser = Parser::init(None);

        let input = "5522;status=DONE";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::KittyClipboardProtocol(_)));
        let kcp = match cmd {
            Command::KittyClipboardProtocol(kcp) => kcp,
            _ => unreachable!(),
        };
        assert_eq!(
            Status::Done,
            kcp.read_option(Option_::Status).as_status().unwrap()
        );
    }

    #[test]
    fn osc_5522_invalid_location() {
        let mut p: Parser = Parser::init(None);

        let input = "5522;loc=bobr";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::KittyClipboardProtocol(_)));
        let kcp = match cmd {
            Command::KittyClipboardProtocol(kcp) => kcp,
            _ => unreachable!(),
        };
        assert!(kcp.read_option(Option_::Loc).is_null());
    }

    #[test]
    fn osc_5522_valid_location() {
        let mut p: Parser = Parser::init(None);

        let input = "5522;loc=primary";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::KittyClipboardProtocol(_)));
        let kcp = match cmd {
            Command::KittyClipboardProtocol(kcp) => kcp,
            _ => unreachable!(),
        };
        assert_eq!(
            Location::Primary,
            kcp.read_option(Option_::Loc).as_loc().unwrap()
        );
    }

    #[test]
    fn osc_5522_password_1() {
        let mut p: Parser = Parser::init(None);

        let input = "5522;pw=R2hvc3R0eQ==:name=Qk9CUiBLVVJXQQ==";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::KittyClipboardProtocol(_)));
        let kcp = match cmd {
            Command::KittyClipboardProtocol(kcp) => kcp,
            _ => unreachable!(),
        };
        assert_eq!(
            b"R2hvc3R0eQ==".as_slice(),
            kcp.read_option(Option_::Pw).as_bytes().unwrap()
        );
        assert_eq!(
            b"Qk9CUiBLVVJXQQ==".as_slice(),
            kcp.read_option(Option_::Name).as_bytes().unwrap()
        );
    }

    #[test]
    fn osc_5522_password_2() {
        let mut p: Parser = Parser::init(None);

        let input = "5522;password=R2hvc3R0eQ==";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::KittyClipboardProtocol(_)));
        let kcp = match cmd {
            Command::KittyClipboardProtocol(kcp) => kcp,
            _ => unreachable!(),
        };
        assert_eq!(
            b"R2hvc3R0eQ==".as_slice(),
            kcp.read_option(Option_::Password).as_bytes().unwrap()
        );
    }

    #[test]
    fn osc_5522_example_1() {
        let mut p: Parser = Parser::init(None);

        let input = "5522;type=read:status=OK";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::KittyClipboardProtocol(_)));
        let kcp = match cmd {
            Command::KittyClipboardProtocol(kcp) => kcp,
            _ => unreachable!(),
        };
        assert!(kcp.payload.is_none());
        assert!(kcp.read_option(Option_::Id).is_null());
        assert!(kcp.read_option(Option_::Loc).is_null());
        assert!(kcp.read_option(Option_::Mime).is_null());
        assert!(kcp.read_option(Option_::Name).is_null());
        assert!(kcp.read_option(Option_::Password).is_null());
        assert!(kcp.read_option(Option_::Pw).is_null());
        assert_eq!(
            Status::Ok,
            kcp.read_option(Option_::Status).as_status().unwrap()
        );
        assert_eq!(
            Operation::Read,
            kcp.read_option(Option_::Type).as_type().unwrap()
        );
    }

    #[test]
    fn osc_5522_example_2() {
        let mut p: Parser = Parser::init(None);

        let input = "5522;type=read:mime=dGV4dC9wbGFpbg==;R2hvc3R0eQ==";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::KittyClipboardProtocol(_)));
        let kcp = match cmd {
            Command::KittyClipboardProtocol(kcp) => kcp,
            _ => unreachable!(),
        };
        assert_eq!(
            b"R2hvc3R0eQ==".as_slice(),
            kcp.payload.as_ref().unwrap().as_slice()
        );
        assert!(kcp.read_option(Option_::Id).is_null());
        assert!(kcp.read_option(Option_::Loc).is_null());
        assert_eq!(
            b"dGV4dC9wbGFpbg==".as_slice(),
            kcp.read_option(Option_::Mime).as_bytes().unwrap()
        );
        assert!(kcp.read_option(Option_::Name).is_null());
        assert!(kcp.read_option(Option_::Password).is_null());
        assert!(kcp.read_option(Option_::Pw).is_null());
        assert!(kcp.read_option(Option_::Status).is_null());
        assert_eq!(
            Operation::Read,
            kcp.read_option(Option_::Type).as_type().unwrap()
        );
    }

    #[test]
    fn osc_5522_example_3() {
        let mut p: Parser = Parser::init(None);

        let input = "5522;type=read:status=OK";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::KittyClipboardProtocol(_)));
        let kcp = match cmd {
            Command::KittyClipboardProtocol(kcp) => kcp,
            _ => unreachable!(),
        };
        assert!(kcp.payload.is_none());
        assert!(kcp.read_option(Option_::Id).is_null());
        assert!(kcp.read_option(Option_::Loc).is_null());
        assert!(kcp.read_option(Option_::Mime).is_null());
        assert!(kcp.read_option(Option_::Name).is_null());
        assert!(kcp.read_option(Option_::Password).is_null());
        assert!(kcp.read_option(Option_::Pw).is_null());
        assert_eq!(
            Status::Ok,
            kcp.read_option(Option_::Status).as_status().unwrap()
        );
        assert_eq!(
            Operation::Read,
            kcp.read_option(Option_::Type).as_type().unwrap()
        );
    }

    #[test]
    fn osc_5522_example_4() {
        let mut p: Parser = Parser::init(None);

        let input = "5522;type=write";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::KittyClipboardProtocol(_)));
        let kcp = match cmd {
            Command::KittyClipboardProtocol(kcp) => kcp,
            _ => unreachable!(),
        };
        assert!(kcp.payload.is_none());
        assert!(kcp.read_option(Option_::Id).is_null());
        assert!(kcp.read_option(Option_::Loc).is_null());
        assert!(kcp.read_option(Option_::Mime).is_null());
        assert!(kcp.read_option(Option_::Name).is_null());
        assert!(kcp.read_option(Option_::Password).is_null());
        assert!(kcp.read_option(Option_::Pw).is_null());
        assert!(kcp.read_option(Option_::Status).is_null());
        assert_eq!(
            Operation::Write,
            kcp.read_option(Option_::Type).as_type().unwrap()
        );
    }

    #[test]
    fn osc_5522_example_5() {
        let mut p: Parser = Parser::init(None);

        let input = "5522;type=wdata:mime=dGV4dC9wbGFpbg==;R2hvc3R0eQ==";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::KittyClipboardProtocol(_)));
        let kcp = match cmd {
            Command::KittyClipboardProtocol(kcp) => kcp,
            _ => unreachable!(),
        };
        assert_eq!(
            b"R2hvc3R0eQ==".as_slice(),
            kcp.payload.as_ref().unwrap().as_slice()
        );
        assert!(kcp.read_option(Option_::Id).is_null());
        assert!(kcp.read_option(Option_::Loc).is_null());
        assert_eq!(
            b"dGV4dC9wbGFpbg==".as_slice(),
            kcp.read_option(Option_::Mime).as_bytes().unwrap()
        );
        assert!(kcp.read_option(Option_::Name).is_null());
        assert!(kcp.read_option(Option_::Password).is_null());
        assert!(kcp.read_option(Option_::Pw).is_null());
        assert!(kcp.read_option(Option_::Status).is_null());
        assert_eq!(
            Operation::Wdata,
            kcp.read_option(Option_::Type).as_type().unwrap()
        );
    }

    #[test]
    fn osc_5522_example_6() {
        let mut p: Parser = Parser::init(None);

        let input = "5522;type=wdata";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::KittyClipboardProtocol(_)));
        let kcp = match cmd {
            Command::KittyClipboardProtocol(kcp) => kcp,
            _ => unreachable!(),
        };
        assert!(kcp.payload.is_none());
        assert!(kcp.read_option(Option_::Id).is_null());
        assert!(kcp.read_option(Option_::Loc).is_null());
        assert!(kcp.read_option(Option_::Mime).is_null());
        assert!(kcp.read_option(Option_::Name).is_null());
        assert!(kcp.read_option(Option_::Password).is_null());
        assert!(kcp.read_option(Option_::Pw).is_null());
        assert!(kcp.read_option(Option_::Status).is_null());
        assert_eq!(
            Operation::Wdata,
            kcp.read_option(Option_::Type).as_type().unwrap()
        );
    }

    #[test]
    fn osc_5522_example_7() {
        let mut p: Parser = Parser::init(None);

        let input = "5522;type=write:status=DONE";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::KittyClipboardProtocol(_)));
        let kcp = match cmd {
            Command::KittyClipboardProtocol(kcp) => kcp,
            _ => unreachable!(),
        };
        assert!(kcp.payload.is_none());
        assert!(kcp.read_option(Option_::Id).is_null());
        assert!(kcp.read_option(Option_::Loc).is_null());
        assert!(kcp.read_option(Option_::Mime).is_null());
        assert!(kcp.read_option(Option_::Name).is_null());
        assert!(kcp.read_option(Option_::Password).is_null());
        assert!(kcp.read_option(Option_::Pw).is_null());
        assert_eq!(
            Status::Done,
            kcp.read_option(Option_::Status).as_status().unwrap()
        );
        assert_eq!(
            Operation::Write,
            kcp.read_option(Option_::Type).as_type().unwrap()
        );
    }

    #[test]
    fn osc_5522_example_8() {
        let mut p: Parser = Parser::init(None);

        let input = "5522;type=write:status=EPERM";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::KittyClipboardProtocol(_)));
        let kcp = match cmd {
            Command::KittyClipboardProtocol(kcp) => kcp,
            _ => unreachable!(),
        };
        assert!(kcp.payload.is_none());
        assert!(kcp.read_option(Option_::Id).is_null());
        assert!(kcp.read_option(Option_::Loc).is_null());
        assert!(kcp.read_option(Option_::Mime).is_null());
        assert!(kcp.read_option(Option_::Name).is_null());
        assert!(kcp.read_option(Option_::Password).is_null());
        assert!(kcp.read_option(Option_::Pw).is_null());
        assert_eq!(
            Status::Eperm,
            kcp.read_option(Option_::Status).as_status().unwrap()
        );
        assert_eq!(
            Operation::Write,
            kcp.read_option(Option_::Type).as_type().unwrap()
        );
    }

    #[test]
    fn osc_5522_example_9() {
        let mut p: Parser = Parser::init(None);

        let input = "5522;type=walias:mime=dGV4dC9wbGFpbg==;dGV4dC9odG1sIGFwcGxpY2F0aW9uL2pzb24=";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::KittyClipboardProtocol(_)));
        let kcp = match cmd {
            Command::KittyClipboardProtocol(kcp) => kcp,
            _ => unreachable!(),
        };
        assert_eq!(
            b"dGV4dC9odG1sIGFwcGxpY2F0aW9uL2pzb24=".as_slice(),
            kcp.payload.as_ref().unwrap().as_slice()
        );
        assert!(kcp.read_option(Option_::Id).is_null());
        assert!(kcp.read_option(Option_::Loc).is_null());
        assert_eq!(
            b"dGV4dC9wbGFpbg==".as_slice(),
            kcp.read_option(Option_::Mime).as_bytes().unwrap()
        );
        assert!(kcp.read_option(Option_::Name).is_null());
        assert!(kcp.read_option(Option_::Password).is_null());
        assert!(kcp.read_option(Option_::Pw).is_null());
        assert!(kcp.read_option(Option_::Status).is_null());
        assert_eq!(
            Operation::Walias,
            kcp.read_option(Option_::Type).as_type().unwrap()
        );
    }

    #[test]
    fn osc_5522_example_10() {
        let mut p: Parser = Parser::init(None);

        let input = "5522;type=read:status=OK:password=Qk9CUiBLVVJXQQ==";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::KittyClipboardProtocol(_)));
        let kcp = match cmd {
            Command::KittyClipboardProtocol(kcp) => kcp,
            _ => unreachable!(),
        };
        assert!(kcp.payload.is_none());
        assert!(kcp.read_option(Option_::Id).is_null());
        assert!(kcp.read_option(Option_::Loc).is_null());
        assert!(kcp.read_option(Option_::Mime).is_null());
        assert!(kcp.read_option(Option_::Name).is_null());
        assert_eq!(
            b"Qk9CUiBLVVJXQQ==".as_slice(),
            kcp.read_option(Option_::Password).as_bytes().unwrap()
        );
        assert!(kcp.read_option(Option_::Pw).is_null());
        assert_eq!(
            Status::Ok,
            kcp.read_option(Option_::Status).as_status().unwrap()
        );
        assert_eq!(
            Operation::Read,
            kcp.read_option(Option_::Type).as_type().unwrap()
        );
    }

    #[test]
    fn osc_5522_example_11() {
        let mut p: Parser = Parser::init(None);

        let input = "5522;type=read:status=DATA:mime=dGV4dC9wbGFpbg==";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::KittyClipboardProtocol(_)));
        let kcp = match cmd {
            Command::KittyClipboardProtocol(kcp) => kcp,
            _ => unreachable!(),
        };
        assert!(kcp.payload.is_none());
        assert!(kcp.read_option(Option_::Id).is_null());
        assert!(kcp.read_option(Option_::Loc).is_null());
        assert_eq!(
            b"dGV4dC9wbGFpbg==".as_slice(),
            kcp.read_option(Option_::Mime).as_bytes().unwrap()
        );
        assert!(kcp.read_option(Option_::Name).is_null());
        assert!(kcp.read_option(Option_::Password).is_null());
        assert!(kcp.read_option(Option_::Pw).is_null());
        assert_eq!(
            Status::Data,
            kcp.read_option(Option_::Status).as_status().unwrap()
        );
        assert_eq!(
            Operation::Read,
            kcp.read_option(Option_::Type).as_type().unwrap()
        );
    }

    #[test]
    fn osc_5522_example_12() {
        let mut p: Parser = Parser::init(None);

        let input = "5522;type=read:mime=dGV4dC9wbGFpbg==:password=Qk9CUiBLVVJXQQ==";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::KittyClipboardProtocol(_)));
        let kcp = match cmd {
            Command::KittyClipboardProtocol(kcp) => kcp,
            _ => unreachable!(),
        };
        assert!(kcp.payload.is_none());
        assert!(kcp.read_option(Option_::Id).is_null());
        assert!(kcp.read_option(Option_::Loc).is_null());
        assert_eq!(
            b"dGV4dC9wbGFpbg==".as_slice(),
            kcp.read_option(Option_::Mime).as_bytes().unwrap()
        );
        assert!(kcp.read_option(Option_::Name).is_null());
        assert_eq!(
            b"Qk9CUiBLVVJXQQ==".as_slice(),
            kcp.read_option(Option_::Password).as_bytes().unwrap()
        );
        assert!(kcp.read_option(Option_::Pw).is_null());
        assert!(kcp.read_option(Option_::Status).is_null());
        assert_eq!(
            Operation::Read,
            kcp.read_option(Option_::Type).as_type().unwrap()
        );
    }

    #[test]
    fn osc_5522_example_13() {
        let mut p: Parser = Parser::init(None);

        let input = "5522;type=read:status=OK";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::KittyClipboardProtocol(_)));
        let kcp = match cmd {
            Command::KittyClipboardProtocol(kcp) => kcp,
            _ => unreachable!(),
        };
        assert!(kcp.payload.is_none());
        assert!(kcp.read_option(Option_::Id).is_null());
        assert!(kcp.read_option(Option_::Loc).is_null());
        assert!(kcp.read_option(Option_::Mime).is_null());
        assert!(kcp.read_option(Option_::Name).is_null());
        assert!(kcp.read_option(Option_::Password).is_null());
        assert!(kcp.read_option(Option_::Pw).is_null());
        assert_eq!(
            Status::Ok,
            kcp.read_option(Option_::Status).as_status().unwrap()
        );
        assert_eq!(
            Operation::Read,
            kcp.read_option(Option_::Type).as_type().unwrap()
        );
    }

    #[test]
    fn osc_5522_example_14() {
        let mut p: Parser = Parser::init(None);

        let input = "5522;type=read:status=DATA:mime=dGV4dC9wbGFpbg==;Qk9CUiBLVVJXQQ==";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::KittyClipboardProtocol(_)));
        let kcp = match cmd {
            Command::KittyClipboardProtocol(kcp) => kcp,
            _ => unreachable!(),
        };
        assert_eq!(
            b"Qk9CUiBLVVJXQQ==".as_slice(),
            kcp.payload.as_ref().unwrap().as_slice()
        );
        assert!(kcp.read_option(Option_::Id).is_null());
        assert!(kcp.read_option(Option_::Loc).is_null());
        assert_eq!(
            b"dGV4dC9wbGFpbg==".as_slice(),
            kcp.read_option(Option_::Mime).as_bytes().unwrap()
        );
        assert!(kcp.read_option(Option_::Name).is_null());
        assert!(kcp.read_option(Option_::Password).is_null());
        assert!(kcp.read_option(Option_::Pw).is_null());
        assert_eq!(
            Status::Data,
            kcp.read_option(Option_::Status).as_status().unwrap()
        );
        assert_eq!(
            Operation::Read,
            kcp.read_option(Option_::Type).as_type().unwrap()
        );
    }

    #[test]
    fn osc_5522_example_15() {
        let mut p: Parser = Parser::init(None);

        let input = "5522;type=read:status=OK";
        for ch in input.bytes() {
            p.next(ch);
        }

        let cmd = p.end(Some(0x1b)).unwrap().clone();
        assert!(matches!(cmd, Command::KittyClipboardProtocol(_)));
        let kcp = match cmd {
            Command::KittyClipboardProtocol(kcp) => kcp,
            _ => unreachable!(),
        };
        assert!(kcp.payload.is_none());
        assert!(kcp.read_option(Option_::Id).is_null());
        assert!(kcp.read_option(Option_::Loc).is_null());
        assert!(kcp.read_option(Option_::Mime).is_null());
        assert!(kcp.read_option(Option_::Name).is_null());
        assert!(kcp.read_option(Option_::Password).is_null());
        assert!(kcp.read_option(Option_::Pw).is_null());
        assert_eq!(
            Status::Ok,
            kcp.read_option(Option_::Status).as_status().unwrap()
        );
        assert_eq!(
            Operation::Read,
            kcp.read_option(Option_::Type).as_type().unwrap()
        );
    }
}
