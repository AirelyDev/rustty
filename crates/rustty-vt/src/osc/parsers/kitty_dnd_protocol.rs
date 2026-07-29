//! Kitty's drag and drop protocol (OSC 72)
//! Specification: https://sw.kovidgoyal.net/kitty/drag-and-drop-protocol/
//!
//! PORTS: ghostty src/terminal/osc/parsers/kitty_dnd_protocol.zig
//!
//! PORT STATUS: ported — full parse logic + `readOption`/`Option_`/`EventType`
//! machinery + tests (WS8 BL-7, oracle 91c87e2cf + dd6c09bcf + a6132c18b +
//! c488ccda6 — ported from the final post-c488ccda6 state).
//!
//! No chunking reassembly or event-semantics handling — this module only
//! parses individual OSC 72 events, matching upstream scope exactly. The
//! action/dispatch layer (not yet ported) owns chunked-transfer reassembly
//! and drag-state semantics.

// PORT NOTE: `const std = @import("std");` — std facilities are used inline.
// PORT NOTE: `const assert = @import("../../../quirks.zig").inlineAssert;` —
// retargeted to `debug_assert!` at the call site (matching the convention used
// by `kitty_clipboard_protocol.rs`).
// PORT NOTE: `const log = std.log.scoped(.kitty_dnd_protocol);` — Rustty has no
// logging facade wired into this crate; ported as this note.
use crate::osc::{Command, Parser, State, Terminator};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OSC {
    /// The raw metadata that was received. Parse individual values with
    /// `read_option`.
    ///
    /// PORT NOTE (fidelity): reference `metadata: []const u8` — RAW bytes.
    pub metadata: Vec<u8>,
    /// The raw payload. Its meaning and encoding depend on the event type
    /// (`t` key).
    ///
    /// PORT NOTE (fidelity): reference `payload: ?[]const u8` — optional RAW bytes.
    pub payload: Option<Vec<u8>>,
    /// The terminator used for this OSC, so any response can match it.
    pub terminator: Terminator,
}

impl OSC {
    /// Decode an option from the metadata.
    // PORT NOTE: the reference uses a `comptime key: Option` parameter with a
    // dependent return type `key.Type()`. Rust has no comptime-dependent return
    // types, so `read_option` takes a runtime `Option_` and returns a tagged
    // `OptionValue` whose active variant matches the key's reference `Type()`.
    pub fn read_option(&self, key: Option_) -> OptionValue {
        key.read(&self.metadata)
    }
}

/// Values for the `t` (event type) metadata key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventType {
    /// ('a') Terminal registers itself as willing to accept drops.
    AcceptDrops,
    /// ('A') Terminal unregisters itself; drops should no longer be forwarded.
    StopAcceptingDrops,
    /// ('m') Pointer is moving over the terminal while a drag is in progress.
    /// Carries `x`/`y` cursor position; -1 signals the drag left the window.
    DropMove,
    /// ('M') Items were dropped onto the terminal.
    /// Carries `x`/`y` drop position and `i` (multiplexer session ID).
    DropDropped,
    /// ('r') Terminal requests data for a specific MIME type from the drag source.
    /// Carries `i` (multiplexer session ID) and `y` (1-based MIME type index).
    RequestData,
    /// ('R') Error response to a `request_data` event.
    RequestError,
    /// ('o') Terminal offers data for an outgoing drag (drag-out from terminal).
    OfferDrag,
    /// ('p') Drag source presents the actual payload for a previously requested MIME type.
    /// Carries `i` (multiplexer session ID), `o` (operation), and `m` (chunking flag).
    PresentData,
    /// ('P') Replace the current drag image with a new one.
    /// Payload is the image data; `X`/`Y` carry image dimensions in pixels.
    ChangeDragImage,
    /// ('e') Notification of an event on an outgoing drag offer (e.g., accepted or rejected).
    DragOfferEvent,
    /// ('E') Error on an outgoing drag offer.
    DragOfferError,
    /// ('k') URI list data delivered as part of a drag or clipboard transfer.
    UriListData,
    /// ('q') Query terminal capabilities related to the drag-and-drop protocol.
    Query,
}

impl EventType {
    pub fn init(str: &[u8]) -> Option<EventType> {
        if str.len() != 1 {
            return None;
        }
        match str[0] {
            b'a' => Some(EventType::AcceptDrops),
            b'A' => Some(EventType::StopAcceptingDrops),
            b'm' => Some(EventType::DropMove),
            b'M' => Some(EventType::DropDropped),
            b'r' => Some(EventType::RequestData),
            b'R' => Some(EventType::RequestError),
            b'o' => Some(EventType::OfferDrag),
            b'p' => Some(EventType::PresentData),
            b'P' => Some(EventType::ChangeDragImage),
            b'e' => Some(EventType::DragOfferEvent),
            b'E' => Some(EventType::DragOfferError),
            b'k' => Some(EventType::UriListData),
            b'q' => Some(EventType::Query),
            _ => None,
        }
    }
}

/// Metadata keys defined by the protocol. Keys are case-sensitive: `x` and
/// `X` are distinct (as are `y`/`Y`).
///
/// PORT NOTE: the reference `Option` enum uses bare `x`/`X`/`y`/`Y` tag names.
/// Rust identifiers are case-sensitive too, but to keep the lowercase/uppercase
/// pair visually unambiguous at call sites (and consistent with the
/// `Option_`-not-`Option` naming already established for
/// `kitty_clipboard_protocol`), the uppercase members are spelled
/// `CapX`/`CapY`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Option_ {
    /// Event type. Maps to `EventType`; present in every OSC 72 sequence.
    T,
    /// Chunking flag. `0` = this is the final (or only) chunk; `1` = more chunks follow.
    M,
    /// Multiplexer session ID. Echoed back in responses so a terminal
    /// multiplexer (e.g. tmux) can route data to the correct pane.
    I,
    /// Drop operation. `0` = reject, `1` = copy, `2` = move, `3` = copy or move.
    O,
    /// Cursor column in cell units (zero-based). -1 signals the drag has left the window.
    X,
    /// Cursor row in cell units (zero-based). Also used as a 1-based MIME type index
    /// in some events (e.g. `request_data`). -1 signals the drag has left the window.
    Y,
    /// Pixel offset from the left edge of the cell; also used as image width
    /// (with `change_drag_image`) or as a symlink/directory marker.
    CapX,
    /// Pixel offset from the top edge of the cell; also used as image height
    /// (with `change_drag_image`) or as a parent directory handle.
    CapY,
}

/// The dependent value produced by `Option_::read`. The active variant
/// matches the reference `Option.Type()` of the option that produced it (`T`
/// reads an `EventType`; every other key reads an `i32`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionValue {
    Int(Option<i32>),
    Type(Option<EventType>),
}

impl OptionValue {
    pub fn as_int(&self) -> Option<i32> {
        match self {
            OptionValue::Int(v) => *v,
            _ => panic!("option value is not an integer"),
        }
    }

    pub fn as_type(&self) -> Option<EventType> {
        match self {
            OptionValue::Type(v) => *v,
            _ => panic!("option value is not an event type"),
        }
    }

    pub fn is_null(&self) -> bool {
        match self {
            OptionValue::Int(v) => v.is_none(),
            OptionValue::Type(v) => v.is_none(),
        }
    }
}

impl Option_ {
    // PORT NOTE: `const name = @tagName(key);` — the reference tag names are the
    // literal protocol key bytes.
    fn tag_name(self) -> &'static [u8] {
        match self {
            Option_::T => b"t",
            Option_::M => b"m",
            Option_::I => b"i",
            Option_::O => b"o",
            Option_::X => b"x",
            Option_::Y => b"y",
            Option_::CapX => b"X",
            Option_::CapY => b"Y",
        }
    }

    /// Look up an option in the metadata string. Returns the null variant if
    /// the key is absent, malformed, or its value cannot be parsed as the
    /// target type. The default values from the spec are NOT substituted
    /// here; callers should apply defaults explicitly so missing-vs-present
    /// can still be distinguished where it matters.
    pub fn read(self, metadata: &[u8]) -> OptionValue {
        let value: &[u8] = 'value: {
            let name = self.tag_name();
            let mut pos: usize = 0;
            while pos < metadata.len() {
                while pos < metadata.len() && metadata[pos].is_ascii_whitespace() {
                    pos += 1;
                }
                if pos >= metadata.len() {
                    return self.null_value();
                }
                // Case-sensitive match: x and X must not be confused.
                if !metadata[pos..].starts_with(name) {
                    // pos = std.mem.indexOfScalarPos(u8, metadata, pos, ':')
                    //     orelse return null;
                    pos = match metadata[pos..].iter().position(|&b| b == b':') {
                        Some(rel) => pos + rel,
                        None => return self.null_value(),
                    };
                    pos += 1;
                    continue;
                }
                pos += name.len();

                while pos < metadata.len() && metadata[pos].is_ascii_whitespace() {
                    pos += 1;
                }
                if pos >= metadata.len() {
                    return self.null_value();
                }
                if metadata[pos] != b'=' {
                    return self.null_value();
                }

                // const end = std.mem.indexOfScalarPos(u8, metadata, pos, ':')
                //     orelse metadata.len;
                let end = match metadata[pos..].iter().position(|&b| b == b':') {
                    Some(rel) => pos + rel,
                    None => metadata.len(),
                };
                let start = pos + 1;
                break 'value trim_whitespace(&metadata[start..end]);
            }
            return self.null_value();
        };

        match self {
            Option_::T => OptionValue::Type(EventType::init(value)),
            Option_::M
            | Option_::I
            | Option_::O
            | Option_::X
            | Option_::Y
            | Option_::CapX
            | Option_::CapY => OptionValue::Int(parse_i32(value)),
        }
    }

    fn null_value(self) -> OptionValue {
        match self {
            Option_::T => OptionValue::Type(None),
            _ => OptionValue::Int(None),
        }
    }
}

// PORT NOTE: `std.fmt.parseInt(i32, value, 10) catch null`.
fn parse_i32(value: &[u8]) -> Option<i32> {
    core::str::from_utf8(value).ok()?.parse::<i32>().ok()
}

// PORT NOTE: `std.mem.trim(u8, slice, &std.ascii.whitespace)`.
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
    // assert(parser.state == .@"72");
    debug_assert!(parser.state == State::S72);

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
        // const sep = std.mem.indexOfScalar(u8, data, ';')
        //     orelse break :result .{ data, null };
        let sep = match data.iter().position(|&b| b == b';') {
            Some(sep) => sep,
            None => break 'result (data, None),
        };
        // break :result .{ data[0..sep], data[sep + 1 .. data.len] };
        (&data[0..sep], Some(&data[sep + 1..data.len()]))
    };

    // PORT NOTE (fidelity): `metadata`/`payload` are `[]const u8` / `?[]const u8`
    // RAW bytes; copy the bytes out, NEVER a lossy String decode.
    let metadata = metadata.to_vec();
    let payload = payload.map(|p| p.to_vec());

    parser.command = Command::KittyDndProtocol(OSC {
        metadata,
        payload,
        terminator: Terminator::init(terminator_ch),
    });

    Some(&mut parser.command)
}

#[cfg(test)]
mod tests {
    use super::{EventType, Option_};
    use crate::osc::{Command, Parser, Terminator};

    // PORT NOTE: the reference uses `testing.allocator`. The ported `OSC`
    // payload is self-owning (`Vec<u8>`s), so the parser needs no allocator for
    // OSC 72; `Parser::init(None)` is the faithful equivalent.

    #[test]
    fn osc_72_metadata_only_no_payload() {
        let mut p: Parser = Parser::init(None);
        let input = "72;t=a";
        for ch in input.bytes() {
            p.next(ch);
        }
        let cmd = p.end(Some(0x1b)).unwrap().clone();
        let dnd = match cmd {
            Command::KittyDndProtocol(dnd) => dnd,
            _ => unreachable!(),
        };
        assert_eq!(b"t=a".as_slice(), dnd.metadata.as_slice());
        assert!(dnd.payload.is_none());
    }

    #[test]
    fn osc_72_metadata_and_empty_payload() {
        let mut p: Parser = Parser::init(None);
        let input = "72;t=a;";
        for ch in input.bytes() {
            p.next(ch);
        }
        let cmd = p.end(Some(0x1b)).unwrap().clone();
        let dnd = match cmd {
            Command::KittyDndProtocol(dnd) => dnd,
            _ => unreachable!(),
        };
        assert_eq!(b"t=a".as_slice(), dnd.metadata.as_slice());
        assert_eq!(b"".as_slice(), dnd.payload.as_ref().unwrap().as_slice());
    }

    #[test]
    fn osc_72_metadata_and_non_empty_payload() {
        let mut p: Parser = Parser::init(None);
        let input = "72;t=a:i=5;text/plain text/uri-list";
        for ch in input.bytes() {
            p.next(ch);
        }
        let cmd = p.end(Some(0x1b)).unwrap().clone();
        let dnd = match cmd {
            Command::KittyDndProtocol(dnd) => dnd,
            _ => unreachable!(),
        };
        assert_eq!(b"t=a:i=5".as_slice(), dnd.metadata.as_slice());
        assert_eq!(
            b"text/plain text/uri-list".as_slice(),
            dnd.payload.as_ref().unwrap().as_slice()
        );
    }

    #[test]
    fn osc_72_read_option_t_valid_event_types() {
        let cases: &[(&str, EventType)] = &[
            ("72;t=a", EventType::AcceptDrops),
            ("72;t=A", EventType::StopAcceptingDrops),
            ("72;t=m", EventType::DropMove),
            ("72;t=M", EventType::DropDropped),
            ("72;t=r", EventType::RequestData),
            ("72;t=R", EventType::RequestError),
            ("72;t=o", EventType::OfferDrag),
            ("72;t=p", EventType::PresentData),
            ("72;t=P", EventType::ChangeDragImage),
            ("72;t=e", EventType::DragOfferEvent),
            ("72;t=E", EventType::DragOfferError),
            ("72;t=k", EventType::UriListData),
            ("72;t=q", EventType::Query),
        ];
        for (input, expected) in cases {
            let mut p: Parser = Parser::init(None);
            for ch in input.bytes() {
                p.next(ch);
            }
            let cmd = p.end(Some(0x1b)).unwrap().clone();
            let dnd = match cmd {
                Command::KittyDndProtocol(dnd) => dnd,
                _ => unreachable!(),
            };
            assert_eq!(*expected, dnd.read_option(Option_::T).as_type().unwrap());
        }
    }

    #[test]
    fn osc_72_read_option_t_unknown_value_returns_null() {
        let mut p: Parser = Parser::init(None);
        let input = "72;t=z";
        for ch in input.bytes() {
            p.next(ch);
        }
        let cmd = p.end(Some(0x1b)).unwrap().clone();
        let dnd = match cmd {
            Command::KittyDndProtocol(dnd) => dnd,
            _ => unreachable!(),
        };
        assert!(dnd.read_option(Option_::T).is_null());
    }

    #[test]
    fn osc_72_read_option_integer_keys() {
        let mut p: Parser = Parser::init(None);
        let input = "72;t=m:i=3:x=10:y=5:X=320:Y=200:o=1:m=0";
        for ch in input.bytes() {
            p.next(ch);
        }
        let cmd = p.end(Some(0x1b)).unwrap().clone();
        let dnd = match cmd {
            Command::KittyDndProtocol(dnd) => dnd,
            _ => unreachable!(),
        };
        assert_eq!(3, dnd.read_option(Option_::I).as_int().unwrap());
        assert_eq!(10, dnd.read_option(Option_::X).as_int().unwrap());
        assert_eq!(5, dnd.read_option(Option_::Y).as_int().unwrap());
        assert_eq!(320, dnd.read_option(Option_::CapX).as_int().unwrap());
        assert_eq!(200, dnd.read_option(Option_::CapY).as_int().unwrap());
        assert_eq!(1, dnd.read_option(Option_::O).as_int().unwrap());
        assert_eq!(0, dnd.read_option(Option_::M).as_int().unwrap());
    }

    #[test]
    fn osc_72_read_option_negative_sentinel() {
        let mut p: Parser = Parser::init(None);
        let input = "72;t=m:x=-1:y=-1";
        for ch in input.bytes() {
            p.next(ch);
        }
        let cmd = p.end(Some(0x1b)).unwrap().clone();
        let dnd = match cmd {
            Command::KittyDndProtocol(dnd) => dnd,
            _ => unreachable!(),
        };
        assert_eq!(-1, dnd.read_option(Option_::X).as_int().unwrap());
        assert_eq!(-1, dnd.read_option(Option_::Y).as_int().unwrap());
    }

    #[test]
    fn osc_72_read_option_case_sensitive_key_matching() {
        let mut p: Parser = Parser::init(None);
        // x=10 must not be returned when asking for CapX (the "X" key)
        let input = "72;x=10:Y=200";
        for ch in input.bytes() {
            p.next(ch);
        }
        let cmd = p.end(Some(0x1b)).unwrap().clone();
        let dnd = match cmd {
            Command::KittyDndProtocol(dnd) => dnd,
            _ => unreachable!(),
        };
        assert_eq!(10, dnd.read_option(Option_::X).as_int().unwrap());
        assert!(dnd.read_option(Option_::CapX).is_null());
        assert_eq!(200, dnd.read_option(Option_::CapY).as_int().unwrap());
        assert!(dnd.read_option(Option_::Y).is_null());
    }

    #[test]
    fn osc_72_read_option_absent_key_returns_null() {
        let mut p: Parser = Parser::init(None);
        let input = "72;t=a";
        for ch in input.bytes() {
            p.next(ch);
        }
        let cmd = p.end(Some(0x1b)).unwrap().clone();
        let dnd = match cmd {
            Command::KittyDndProtocol(dnd) => dnd,
            _ => unreachable!(),
        };
        assert!(dnd.read_option(Option_::I).is_null());
        assert!(dnd.read_option(Option_::X).is_null());
        assert!(dnd.read_option(Option_::CapX).is_null());
        assert!(dnd.read_option(Option_::M).is_null());
    }

    #[test]
    fn osc_72_read_option_malformed_integer_returns_null() {
        let mut p: Parser = Parser::init(None);
        let input = "72;x=notanumber";
        for ch in input.bytes() {
            p.next(ch);
        }
        let cmd = p.end(Some(0x1b)).unwrap().clone();
        let dnd = match cmd {
            Command::KittyDndProtocol(dnd) => dnd,
            _ => unreachable!(),
        };
        assert!(dnd.read_option(Option_::X).is_null());
    }

    #[test]
    fn osc_72_bel_terminator_recorded() {
        let mut p: Parser = Parser::init(None);
        let input = "72;t=q";
        for ch in input.bytes() {
            p.next(ch);
        }
        let cmd = p.end(Some(0x07)).unwrap().clone();
        let dnd = match cmd {
            Command::KittyDndProtocol(dnd) => dnd,
            _ => unreachable!(),
        };
        assert_eq!(Terminator::Bel, dnd.terminator);
    }
}
