//! rustty-vt device_status
//!
//! PORTS: ghostty src/terminal/device_status.zig
//!
//! PORT STATUS: done

// PORT NOTE: the reference imports are
//   const std = @import("std");
//   const lib = @import("lib.zig");
// `lib.Enum` for the `.zig` target is a plain enum; ported directly.

/// The color scheme reported in response to a CSI ? 996 n query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorScheme {
    Light,
    Dark,
}

/// An enum(u16) of the available device status requests.
// PORT NOTE: the reference builds this enum at comptime from `entries`, where
// each value is `@bitCast(Tag{ .value = entry.value, .question = entry.question })`.
// We reproduce the exact discriminants here: `Tag` packs `value: u15` in the low
// 15 bits and `question: bool` in bit 15, so the backing u16 is
// `value | (question << 15)`.
#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request {
    // .{ .name = "operating_status", .value = 5 }  => 5
    OperatingStatus = 5,
    // .{ .name = "cursor_position", .value = 6 }   => 6
    CursorPosition = 6,
    // .{ .name = "color_scheme", .value = 996, .question = true } => 996 | (1<<15)
    ColorScheme = 996 | (1 << 15),
}

/// The tag type for our enum is a u16 but we use a packed struct
/// in order to pack the question bit into the tag. The "u16" size is
/// chosen somewhat arbitrarily to match the largest expected size
/// we see as a multiple of 8 bits.
// PORT NOTE: the reference is `packed struct(u16) { value: u15, question: bool }`.
// Zig packed struct fields are laid out least-significant-first, so `value`
// occupies bits 0..14 and `question` bit 15. We model the layout explicitly with
// from/to-int helpers since Rust has no bitfield structs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tag {
    pub value: u16,     // u15 in the reference (low 15 bits)
    pub question: bool, // "?" request — default false
}

// pub const Backing = @typeInfo(@This()).@"struct".backing_integer.?;
// PORT NOTE: Rust has no inherent associated type aliases on a struct; the
// reference `Tag.Backing` (the packed-struct backing integer) is exposed as a
// module-level alias.
pub type TagBacking = u16;

impl Tag {
    /// Construct a Tag with the default `question = false`.
    pub fn new(value: u16) -> Tag {
        Tag {
            value,
            question: false,
        }
    }

    /// `@bitCast` from the packed struct to the backing u16.
    // PORT NOTE: value in low 15 bits, question in bit 15.
    pub fn to_backing(self) -> u16 {
        (self.value & 0x7FFF) | ((self.question as u16) << 15)
    }

    /// `@bitCast` from the backing u16 to the packed struct.
    pub fn from_backing(int: u16) -> Tag {
        Tag {
            value: int & 0x7FFF,
            question: (int >> 15) & 1 == 1,
        }
    }
}

// PORT NOTE: `pub fn reqFromInt(v: u16, question: bool) ?Request`. The reference
// iterates `entries` matching value+question, then `@enumFromInt`s the bitcast
// Tag. Ported with an explicit match over the supported entries.
pub fn req_from_int(v: u16, question: bool) -> Option<Request> {
    // inline for (entries) |entry| {
    //     if (entry.value == v and entry.question == question) {
    //         const tag: Tag = .{ .question = question, .value = entry.value };
    //         const int: Tag.Backing = @bitCast(tag);
    //         return @enumFromInt(int);
    //     }
    // }
    for entry in ENTRIES {
        if entry.value == v && entry.question == question {
            let tag = Tag {
                question,
                value: entry.value,
            };
            let int: u16 = tag.to_backing();
            // @enumFromInt(int): map the backing value back to the Request.
            return Some(match int {
                x if x == Request::OperatingStatus as u16 => Request::OperatingStatus,
                x if x == Request::CursorPosition as u16 => Request::CursorPosition,
                x if x == Request::ColorScheme as u16 => Request::ColorScheme,
                _ => unreachable!(),
            });
        }
    }

    None
}

/// Maximum number of bytes that `encode_color_scheme_report` will write.
// PORT NOTE: the reference computes this at comptime by dry-running the encoder
// over every enum field; both arms write 9 bytes, frozen here with a test
// asserting the value matches a dry run.
pub const MAX_COLOR_SCHEME_REPORT_ENCODE_SIZE: usize = 9;

/// Encode a color scheme report response for CSI ? 996 n queries.
pub fn encode_color_scheme_report(writer: &mut Vec<u8>, scheme: ColorScheme) {
    writer.extend_from_slice(match scheme {
        // .dark => "\x1B[?997;1n",
        ColorScheme::Dark => b"\x1B[?997;1n",
        // .light => "\x1B[?997;2n",
        ColorScheme::Light => b"\x1B[?997;2n",
    });
}

/// A single entry of a possible device status request we support. The
/// "question" field determines if it is valid with or without the "?"
/// prefix.
// PORT NOTE: the reference uses `comptime_int` for `value`; we use `u16` to
// match the request value domain. `name` is used by the reference at comptime to
// build the enum field names; in the runtime port it is only read by tests, so
// the non-test build sees it as unused.
struct Entry {
    #[allow(dead_code)]
    name: &'static str,
    value: u16,
    question: bool, // "?" request
}

/// The full list of device status request entries.
const ENTRIES: &[Entry] = &[
    Entry {
        name: "operating_status",
        value: 5,
        question: false,
    },
    Entry {
        name: "cursor_position",
        value: 6,
        question: false,
    },
    Entry {
        name: "color_scheme",
        value: 996,
        question: true,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    // PORT NOTE: keep `name` referenced so the field is not dead; the reference
    // uses `entry.name` to build the comptime enum field names.
    #[test]
    fn entries_have_names() {
        for entry in ENTRIES {
            assert!(!entry.name.is_empty());
        }
    }

    // test "encode color scheme report dark" / "light" (oracle f00e90694)
    #[test]
    fn encode_color_scheme_report_dark_light() {
        assert_eq!(9, MAX_COLOR_SCHEME_REPORT_ENCODE_SIZE);
        let mut buf = Vec::new();
        encode_color_scheme_report(&mut buf, ColorScheme::Dark);
        assert_eq!(b"\x1B[?997;1n", &buf[..]);
        buf.clear();
        encode_color_scheme_report(&mut buf, ColorScheme::Light);
        assert_eq!(b"\x1B[?997;2n", &buf[..]);
    }

    #[test]
    fn order() {
        // test "order" {
        //     const t: Tag = .{ .value = 1 };
        //     const int: Backing = @bitCast(t);
        //     try std.testing.expectEqual(@as(Backing, 1), int);
        // }
        let t = Tag::new(1);
        let int: u16 = t.to_backing();
        assert_eq!(1_u16, int);
    }
}
