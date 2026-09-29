//! PORTS: ghostty src/terminal/kitty/color.zig
//!
//! PORT STATUS: done

// PORT NOTE: `const terminal = @import("../main.zig");` then
// `const RGB = terminal.color.RGB;` and
// `const Terminator = terminal.osc.Terminator;` — retargeted to the ported
// crate-relative paths.
use crate::color::RGB;
use crate::osc::Terminator;

pub struct OSC {
    /// list of requests
    // PORT NOTE (parity tail S7-7, S174-F3): `std.ArrayList(Request)` grown
    // through the OSC parser's allocator (`list.append(alloc, ..)`,
    // osc/parsers/kitty_color.zig). It was a Rust-heap `Vec`. The list is
    // unmanaged, as the reference's is; `alloc` below is the allocator its
    // growth used. The OSC lives in the parser's `kitty_color` field and is
    // dropped at `reset`, which frees the list (the reference's
    // `OSC.deinit(alloc)`, which the parser's `reset` calls).
    pub list: crate::alloc::ArrayListUnmanaged<Request>,

    /// We must reply with the same string terminator (ST) as used in the
    /// request.
    pub terminator: Terminator,

    /// The allocator `list` grows through (`parser.alloc`).
    // PORT NOTE: not a reference field; see `list`.
    pub alloc: crate::alloc::Allocator,
}

impl core::fmt::Debug for OSC {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("OSC")
            .field("list", &self.list.items())
            .field("terminator", &self.terminator)
            .finish()
    }
}

/// Equality of the requests and the terminator (not of the allocator).
impl PartialEq for OSC {
    fn eq(&self, other: &Self) -> bool {
        self.list.items() == other.list.items() && self.terminator == other.terminator
    }
}

impl Eq for OSC {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
    Query(Kind),
    Set { key: Kind, color: RGB },
    Reset(Kind),
}

impl OSC {
    /// `.{ .list = .empty, .terminator = .. }` over `alloc`.
    pub fn init(alloc: crate::alloc::Allocator, terminator: Terminator) -> OSC {
        OSC {
            list: crate::alloc::ArrayListUnmanaged::EMPTY,
            terminator,
            alloc,
        }
    }

    /// `try self.list.append(alloc, request)`: Zig 0.16's `append`
    /// (`ensureUnusedCapacity(1)`, which rounds up with `growCapacity`).
    pub fn append(&mut self, request: Request) -> Result<(), crate::alloc::OutOfMemory> {
        let len = self.list.len();
        self.list.ensure_total_capacity(self.alloc, len + 1)?;
        // SAFETY: capacity is at least `len + 1`; the slot is written before
        // `len` exposes it.
        unsafe {
            self.list.as_mut_ptr().add(len).write(request);
            self.list.set_len(len + 1);
        }
        Ok(())
    }

    // pub fn deinit(self: *OSC, alloc: std.mem.Allocator) void {
    //     self.list.deinit(alloc);
    // }
    // PORT NOTE: frees the list through the allocator it grew with, leaving it
    // empty; `Drop` calls it, so a second call (or the drop after an explicit
    // call) frees nothing.
    pub fn deinit(&mut self) {
        self.list.deinit(self.alloc);
    }

    // PORT NOTE: `pub const C = void;` and `pub fn cval(_: OSC) C { return {}; }`
    // — "We don't currently support encoding this to C in any way." Modeled as
    // the unit type / a no-op returning `()`.
    /// We don't currently support encoding this to C in any way.
    pub fn cval(&self) {}
}

impl Drop for OSC {
    fn drop(&mut self) {
        self.deinit();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Special {
    Foreground,
    Background,
    SelectionForeground,
    SelectionBackground,
    Cursor,
    CursorText,
    VisualBell,
    SecondTransparentBackground,
}

impl Special {
    // PORT NOTE: `std.meta.stringToEnum(Special, key)` — match the lowercase
    // Zig field names verbatim.
    fn string_to_enum(key: &str) -> Option<Special> {
        match key {
            "foreground" => Some(Special::Foreground),
            "background" => Some(Special::Background),
            "selection_foreground" => Some(Special::SelectionForeground),
            "selection_background" => Some(Special::SelectionBackground),
            "cursor" => Some(Special::Cursor),
            "cursor_text" => Some(Special::CursorText),
            "visual_bell" => Some(Special::VisualBell),
            "second_transparent_background" => Some(Special::SecondTransparentBackground),
            _ => None,
        }
    }

    // PORT NOTE: `@tagName(s)` — the Zig field name as a string.
    fn tag_name(self) -> &'static str {
        match self {
            Special::Foreground => "foreground",
            Special::Background => "background",
            Special::SelectionForeground => "selection_foreground",
            Special::SelectionBackground => "selection_background",
            Special::Cursor => "cursor",
            Special::CursorText => "cursor_text",
            Special::VisualBell => "visual_bell",
            Special::SecondTransparentBackground => "second_transparent_background",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Palette(u8),
    Special(Special),
}

impl Kind {
    // PORT NOTE: `pub const max: usize = std.math.maxInt(u8) +
    // @typeInfo(Special).@"enum".fields.len;` — there are 8 Special fields.
    pub const MAX: usize = u8::MAX as usize + 8;

    pub fn parse(key: &str) -> Option<Kind> {
        if let Some(s) = Special::string_to_enum(key) {
            return Some(Kind::Special(s));
        }
        // PORT NOTE: `lib.parseInt(u8, key, 10) catch return null` — base-10
        // digits only; on failure return null. This is NOT Rust's
        // `str::parse::<u8>()`, which accepts a `+` prefix (`"+1"`).
        Some(Kind::Palette(
            match rustty_lib::parse_int::<u8, 10>(key.as_bytes()) {
                Ok(p) => p,
                Err(_) => return None,
            },
        ))
    }

    /// Returns true when a terminal has built-in state for this key.
    ///
    /// Unsupported special colors may still be valid Kitty protocol keys, but
    /// libghostty-vt cannot report them because Terminal does not store them.
    pub fn has_terminal_query_color(&self) -> bool {
        match self {
            Kind::Palette(_) => true,
            Kind::Special(special) => matches!(
                special,
                Special::Foreground | Special::Background | Special::Cursor
            ),
        }
    }

    // PORT NOTE: the Zig `format` writes `{d}` for palette (decimal) and `{s}`
    // for special (the tag name). Modeled via `std::fmt::Display`.
    pub fn format(&self, writer: &mut impl std::fmt::Write) -> std::fmt::Result {
        match self {
            Kind::Palette(p) => write!(writer, "{}", p),
            Kind::Special(s) => write!(writer, "{}", s.tag_name()),
        }
    }
}

impl std::fmt::Display for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.format(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn osc_kitty_color_protocol_kind_string() {
        // PORT NOTE: `std.fmt.bufPrint(&buf, "{f}", .{kind})` invokes `Kind.format`;
        // here `to_string()` (via `Display`) is the faithful equivalent.
        {
            let actual = Kind::Special(Special::Foreground).to_string();
            assert_eq!("foreground", actual);
        }
        {
            let actual = Kind::Palette(42).to_string();
            assert_eq!("42", actual);
        }
        assert!(Kind::Palette(42).has_terminal_query_color());
        assert!(Kind::Special(Special::Foreground).has_terminal_query_color());
        assert!(!Kind::Special(Special::SelectionBackground).has_terminal_query_color());
    }

    // test "OSC: kitty color indexes reject digit separators" {
    #[test]
    fn osc_kitty_color_indexes_reject_digit_separators() {
        assert_eq!(None, Kind::parse("1_0"));
        assert_eq!(Kind::Palette(10), Kind::parse("10").unwrap());
        assert_eq!(
            Kind::Special(Special::SelectionForeground),
            Kind::parse("selection_foreground").unwrap(),
        );
    }

    // PARITY (rustty-added): `Kind.parse` routes the palette key through
    // `lib.parseInt(u8, key, 10)`, which is NOT Rust's `str::parse::<u8>()`:
    // it rejects a sign prefix. Since `73768913b` it also rejects `_` digit
    // separators (`std.fmt.parseUnsigned`, used before, accepted them).
    #[test]
    fn osc_kitty_color_protocol_kind_parse_unsigned_semantics() {
        assert_eq!(Kind::parse("42"), Some(Kind::Palette(42)));
        assert_eq!(Kind::parse("255"), Some(Kind::Palette(255)));
        // `parseInt` rejects `_` separators between digits.
        assert_eq!(Kind::parse("1_2"), None);
        assert_eq!(Kind::parse("2_5_5"), None);
        // `parseInt` accepts NO sign prefix on an unsigned type.
        assert_eq!(Kind::parse("+1"), None);
        assert_eq!(Kind::parse("-1"), None);
        assert_eq!(Kind::parse("-0"), None);
        // Everything else that the base-10 unsigned parse rejects.
        for bad in ["_1", "1_", "_", "", " 1", "1 ", "256", "0x1", "0b1"] {
            assert_eq!(Kind::parse(bad), None, "expected None for {bad:?}");
        }
        // Special keys still win over the numeric parse.
        assert_eq!(
            Kind::parse("foreground"),
            Some(Kind::Special(Special::Foreground))
        );
    }
}
