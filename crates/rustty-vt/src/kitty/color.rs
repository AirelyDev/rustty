//! PORTS: ghostty src/terminal/kitty/color.zig
//!
//! PORT STATUS: done

// PORT NOTE: `const terminal = @import("../main.zig");` then
// `const RGB = terminal.color.RGB;` and
// `const Terminator = terminal.osc.Terminator;` — retargeted to the ported
// crate-relative paths.
use crate::color::RGB;
use crate::osc::Terminator;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OSC {
    /// list of requests
    pub list: Vec<Request>,

    /// We must reply with the same string terminator (ST) as used in the
    /// request.
    pub terminator: Terminator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
    Query(Kind),
    Set { key: Kind, color: RGB },
    Reset(Kind),
}

impl OSC {
    // PORT NOTE: `list: std.ArrayList(Request)` has no embedded allocator in
    // Zig's unmanaged ArrayList; `deinit(alloc)` frees the backing storage.
    // Rust's `Vec` owns and frees its own storage on drop, so `deinit` is a
    // no-op retaining the faithful signature shape (the `alloc` is unused).
    pub fn deinit(&mut self) {
        // self.list.deinit(alloc);
    }

    // PORT NOTE: `pub const C = void;` and `pub fn cval(_: OSC) C { return {}; }`
    // — "We don't currently support encoding this to C in any way." Modeled as
    // the unit type / a no-op returning `()`.
    /// We don't currently support encoding this to C in any way.
    pub fn cval(&self) {}
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
        // PORT NOTE: `std.fmt.parseUnsigned(u8, key, 10) catch return null` —
        // base-10 unsigned parse; on failure return null.
        Some(Kind::Palette(match key.parse::<u8>() {
            Ok(p) => p,
            Err(_) => return None,
        }))
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
}
