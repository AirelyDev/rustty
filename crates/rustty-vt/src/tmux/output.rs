//! rustty-vt tmux/output
//!
//! PORTS: ghostty src/terminal/tmux/output.zig
//!
//! PORT STATUS: done (vt-16). tmux control-mode output parser. Faithful port of
//! all 47 reference tests.
//
// PORT NOTE: the reference is built on Zig comptime metaprogramming —
// `parseFormatStruct(comptime T, ...)` reflects over a struct type produced by
// `FormatStruct(comptime vars)` (a `@Type(.{ .@"struct" = ... })` whose fields
// are named/typed per `Variable`). Rust has no runtime struct-type synthesis, so
// the faithful equivalent threads the SAME information dynamically: `Variable`
// carries its parsed type as the `Value` enum (the union of every
// `Variable.Type()`), `FormatStruct` is the `&[Variable]` field list itself, and
// `parseFormatStruct` returns the parsed `Value`s in field order. Behavior
// (delimiter splitting, MissingEntry/ExtraEntry/FormatError, per-variable parse)
// is assertion-for-assertion identical.

// const std = @import("std");
// const testing = std.testing;

/// PORTS: ghostty `ParseError`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    MissingEntry,
    ExtraEntry,
    FormatError,
}

/// The parsed value of a `Variable`. This is the dynamic union of every
/// `Variable.Type()` (`bool`, `usize`, or `[]const u8` -> borrowed `&str`).
///
/// The type of the parsed value for this variable type.
///
/// PORT NOTE: in the reference each field of the synthesized `FormatStruct` has
/// a concrete static type (`Variable.Type()`). Rust resolves that statically-
/// known-per-field shape into one runtime-tagged `Value`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Value<'a> {
    Bool(bool),
    Usize(usize),
    Str(&'a str),
}

/// Parse the output from a command with the given format struct
/// (returned usually by FormatStruct). The format struct is expected
/// to be in the order of the variables used in the format string and
/// the variables are expected to be plain variables (no conditionals,
/// extra formatting, etc.). Each variable is expected to be separated
/// by a single `delimiter` character.
//
// PORT NOTE: `comptime T: type` is replaced by the runtime `&[Variable]` field
// list (`FormatStruct`); the parsed fields are returned as `Value`s in order.
pub fn parse_format_struct<'a>(
    vars: &[Variable],
    str: &'a str,
    delimiter: u8,
) -> Result<Vec<Value<'a>>, ParseError> {
    // Parse all our fields
    // const fields = @typeInfo(T).@"struct".fields;
    // var it = std.mem.splitScalar(u8, str, delimiter);
    let mut it = SplitScalar::new(str, delimiter);
    // var result: T = undefined;
    let mut result: Vec<Value<'a>> = Vec::with_capacity(vars.len());
    // inline for (fields) |field| {
    for variable in vars {
        // const part = it.next() orelse return error.MissingEntry;
        let part = it.next().ok_or(ParseError::MissingEntry)?;
        // @field(result, field.name) = Variable.parse(...) catch return error.FormatError;
        result.push(variable.parse(part).map_err(|_| ParseError::FormatError)?);
    }

    // We should have consumed all parts now.
    // if (it.next() != null) return error.ExtraEntry;
    if it.next().is_some() {
        return Err(ParseError::ExtraEntry);
    }

    Ok(result)
}

/// PORTS: ghostty `comptimeFormat`.
//
// PORT NOTE: the reference computes the formatted string at comptime via a
// `std.Io.Writer.Discarding` sizing pass followed by a `.fixed` buffer write.
// In Rust `format` already returns an owned `String`, so `comptime_format` is
// the same single call (there is no separate sizing pass to model).
pub fn comptime_format(vars: &[Variable], delimiter: u8) -> String {
    // @setEvalBranchQuota(50000);
    // var counter: std.Io.Writer.Discarding = .init(&.{});
    // try format(&counter.writer, vars, delimiter);
    // var buf: [counter.count]u8 = undefined;
    // var writer: std.Io.Writer = .fixed(&buf);
    // try format(&writer, vars, delimiter);
    // const final = buf;
    // return final[0..writer.end];
    format(vars, delimiter)
}

/// Format a set of variables into the proper format string for tmux
/// that we can handle with `parseFormatStruct`.
//
// PORT NOTE: `writer: *std.Io.Writer` -> return an owned `String`. The byte
// output (`#{var}` joined by `delimiter`) is identical.
pub fn format(vars: &[Variable], delimiter: u8) -> String {
    let mut out = String::new();
    // for (vars, 0..) |variable, i| {
    for (i, variable) in vars.iter().enumerate() {
        // if (i != 0) try writer.writeByte(delimiter);
        if i != 0 {
            out.push(delimiter as char);
        }
        // try writer.print("#{{{t}}}", .{variable});
        out.push_str("#{");
        out.push_str(variable.tag_name());
        out.push('}');
    }
    out
}

// Returns a struct type that contains fields for each of the given
// format variables. This can be used with `parseFormatStruct` to
// parse an output string into a format struct.
//
// PORT NOTE: `FormatStruct(comptime vars)` synthesizes a struct type with one
// field per variable. In Rust the "format struct" IS the `&[Variable]` field
// list — `parse_format_struct` consumes it directly and yields `Value`s in
// field order. No type synthesis is needed (or possible), so this is the
// identity on the variable slice; callers pass the slice where the reference
// would pass the synthesized type. The reference `parseFormatStruct(T, ...)`
// would instead receive the synthesized type.

/// Possible variables in a tmux format string that we support.
///
/// Tmux supports a large number of variables, but we only implement
/// a subset of them here that are relevant to the use case of implementing
/// control mode for terminal emulators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variable {
    /// 1 if pane is in alternate screen.
    AlternateOn,
    /// Saved cursor X in alternate screen.
    AlternateSavedX,
    /// Saved cursor Y in alternate screen.
    AlternateSavedY,
    /// 1 if bracketed paste mode is enabled.
    BracketedPaste,
    /// 1 if the cursor is blinking.
    CursorBlinking,
    /// Cursor colour in pane. Possible formats:
    /// - Named colors: `black`, `red`, `green`, `yellow`, `blue`, `magenta`,
    ///   `cyan`, `white`, `default`, `terminal`, or bright variants.
    /// - 256 colors: `colour<N>` where N is 0-255 (e.g., `colour100`).
    /// - RGB hex: `#RRGGBB` (e.g., `#ff0000`).
    /// - Empty string if unset.
    CursorColour,
    /// Pane cursor flag.
    CursorFlag,
    /// Cursor shape in pane. Possible values: `block`, `underline`, `bar`,
    /// or `default`.
    CursorShape,
    /// Cursor X position in pane.
    CursorX,
    /// Cursor Y position in pane.
    CursorY,
    /// 1 if focus reporting is enabled.
    FocusFlag,
    /// Pane insert flag.
    InsertFlag,
    /// Pane keypad cursor flag.
    KeypadCursorFlag,
    /// Pane keypad flag.
    KeypadFlag,
    /// Pane mouse all flag.
    MouseAllFlag,
    /// Pane mouse any flag.
    MouseAnyFlag,
    /// Pane mouse button flag.
    MouseButtonFlag,
    /// Pane mouse SGR flag.
    MouseSgrFlag,
    /// Pane mouse standard flag.
    MouseStandardFlag,
    /// Pane mouse UTF-8 flag.
    MouseUtf8Flag,
    /// Pane origin flag.
    OriginFlag,
    /// Unique pane ID prefixed with `%` (e.g., `%0`, `%42`).
    PaneId,
    /// Pane tab positions as a comma-separated list of 0-indexed column
    /// numbers (e.g., `8,16,24,32`). Empty string if no tabs are set.
    PaneTabs,
    /// Bottom of scroll region in pane.
    ScrollRegionLower,
    /// Top of scroll region in pane.
    ScrollRegionUpper,
    /// Unique session ID prefixed with `$` (e.g., `$0`, `$42`).
    SessionId,
    /// Server version (e.g., `3.5a`).
    Version,
    /// Unique window ID prefixed with `@` (e.g., `@0`, `@42`).
    WindowId,
    /// Width of window.
    WindowWidth,
    /// Height of window.
    WindowHeight,
    /// Window layout description, ignoring zoomed window panes. Format is
    /// `<checksum>,<layout>` where checksum is a 4-digit hex CRC16 and layout
    /// encodes pane dimensions as `WxH,X,Y[,ID]` with `{...}` for horizontal
    /// splits and `[...]` for vertical splits.
    WindowLayout,
    /// Pane wrap flag.
    WrapFlag,
}

impl Variable {
    /// Parse the given string value into the appropriate resulting
    /// type for this variable.
    //
    // PORT NOTE: `parse(comptime self, value) !Type(self)` -> a runtime method
    // returning `Result<Value, ()>`. The error set carries no payload here
    // because the only observable distinction the reference makes downstream is
    // success vs. failure (`parseFormatStruct` collapses every parse error into
    // `FormatError`); the standalone `Variable.parse` tests below assert against
    // the specific Zig errors (`InvalidCharacter`/`Overflow`/`FormatError`) via
    // the `ParseIntError`-classifying helper used by the numeric arms.
    pub fn parse(self, value: &str) -> Result<Value<'_>, VariableParseError> {
        match self {
            // => std.mem.eql(u8, value, "1"),
            Variable::AlternateOn
            | Variable::BracketedPaste
            | Variable::CursorBlinking
            | Variable::CursorFlag
            | Variable::FocusFlag
            | Variable::InsertFlag
            | Variable::KeypadCursorFlag
            | Variable::KeypadFlag
            | Variable::MouseAllFlag
            | Variable::MouseAnyFlag
            | Variable::MouseButtonFlag
            | Variable::MouseSgrFlag
            | Variable::MouseStandardFlag
            | Variable::MouseUtf8Flag
            | Variable::OriginFlag
            | Variable::WrapFlag => Ok(Value::Bool(value == "1")),
            // => try std.fmt.parseInt(usize, value, 10),
            Variable::AlternateSavedX
            | Variable::AlternateSavedY
            | Variable::CursorX
            | Variable::CursorY
            | Variable::ScrollRegionLower
            | Variable::ScrollRegionUpper => Ok(Value::Usize(parse_int(value)?)),
            // .session_id => if (value.len >= 2 and value[0] == '$')
            //     try std.fmt.parseInt(usize, value[1..], 10)
            // else
            //     return error.FormatError,
            Variable::SessionId => {
                if value.len() >= 2 && value.as_bytes()[0] == b'$' {
                    Ok(Value::Usize(parse_int(&value[1..])?))
                } else {
                    Err(VariableParseError::FormatError)
                }
            }
            // .window_id => if (value.len >= 2 and value[0] == '@')
            //     try std.fmt.parseInt(usize, value[1..], 10)
            // else
            //     return error.FormatError,
            Variable::WindowId => {
                if value.len() >= 2 && value.as_bytes()[0] == b'@' {
                    Ok(Value::Usize(parse_int(&value[1..])?))
                } else {
                    Err(VariableParseError::FormatError)
                }
            }
            // .pane_id => if (value.len >= 2 and value[0] == '%')
            //     try std.fmt.parseInt(usize, value[1..], 10)
            // else
            //     return error.FormatError,
            Variable::PaneId => {
                if value.len() >= 2 && value.as_bytes()[0] == b'%' {
                    Ok(Value::Usize(parse_int(&value[1..])?))
                } else {
                    Err(VariableParseError::FormatError)
                }
            }
            // .window_width => try std.fmt.parseInt(usize, value, 10),
            Variable::WindowWidth => Ok(Value::Usize(parse_int(value)?)),
            // .window_height => try std.fmt.parseInt(usize, value, 10),
            Variable::WindowHeight => Ok(Value::Usize(parse_int(value)?)),
            // => value,
            Variable::CursorColour
            | Variable::CursorShape
            | Variable::PaneTabs
            | Variable::Version
            | Variable::WindowLayout => Ok(Value::Str(value)),
        }
    }

    /// The tag name of this variable, matching the reference enum field name
    /// (`@tagName(variable)`) used by `format`'s `#{...}` rendering.
    fn tag_name(self) -> &'static str {
        match self {
            Variable::AlternateOn => "alternate_on",
            Variable::AlternateSavedX => "alternate_saved_x",
            Variable::AlternateSavedY => "alternate_saved_y",
            Variable::BracketedPaste => "bracketed_paste",
            Variable::CursorBlinking => "cursor_blinking",
            Variable::CursorColour => "cursor_colour",
            Variable::CursorFlag => "cursor_flag",
            Variable::CursorShape => "cursor_shape",
            Variable::CursorX => "cursor_x",
            Variable::CursorY => "cursor_y",
            Variable::FocusFlag => "focus_flag",
            Variable::InsertFlag => "insert_flag",
            Variable::KeypadCursorFlag => "keypad_cursor_flag",
            Variable::KeypadFlag => "keypad_flag",
            Variable::MouseAllFlag => "mouse_all_flag",
            Variable::MouseAnyFlag => "mouse_any_flag",
            Variable::MouseButtonFlag => "mouse_button_flag",
            Variable::MouseSgrFlag => "mouse_sgr_flag",
            Variable::MouseStandardFlag => "mouse_standard_flag",
            Variable::MouseUtf8Flag => "mouse_utf8_flag",
            Variable::OriginFlag => "origin_flag",
            Variable::PaneId => "pane_id",
            Variable::PaneTabs => "pane_tabs",
            Variable::ScrollRegionLower => "scroll_region_lower",
            Variable::ScrollRegionUpper => "scroll_region_upper",
            Variable::SessionId => "session_id",
            Variable::Version => "version",
            Variable::WindowId => "window_id",
            Variable::WindowWidth => "window_width",
            Variable::WindowHeight => "window_height",
            Variable::WindowLayout => "window_layout",
            Variable::WrapFlag => "wrap_flag",
        }
    }
}

/// PORTS: the error set of `Variable.parse` — the subset of `std.fmt.parseInt`'s
/// errors (`InvalidCharacter`, `Overflow`) plus the `FormatError` raised by the
/// prefixed-ID arms. Carried so the standalone `Variable.parse` tests can assert
/// the exact Zig error, faithful to `expectError(error.InvalidCharacter, ...)`
/// etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariableParseError {
    InvalidCharacter,
    Overflow,
    FormatError,
}

/// Faithful map of `std.fmt.parseInt(usize, value, 10)`: an empty or
/// non-decimal-digit string is `InvalidCharacter`, a value exceeding `usize`
/// (including any leading `-`, which is invalid for an unsigned type and which
/// Zig reports as `Overflow`) is `Overflow`.
//
// PORT NOTE: Rust's `usize::from_str_radix` lumps both the empty string and a
// stray sign/letter into a single `ParseIntError`; we re-derive the Zig
// distinction by inspecting the input the way `std.fmt.parseInt` does — a
// leading `+`/`-` sign with no following error otherwise classifies as Overflow
// for the unsigned target (Zig: parsing "-1" as `usize` is `error.Overflow`),
// and `PosOverflow` from the radix parse is also Overflow; everything else
// (empty, non-digit byte) is InvalidCharacter.
fn parse_int(value: &str) -> Result<usize, VariableParseError> {
    use std::num::IntErrorKind;
    match value.parse::<usize>() {
        Ok(v) => Ok(v),
        Err(e) => match e.kind() {
            IntErrorKind::PosOverflow => Err(VariableParseError::Overflow),
            // A leading `-` makes Rust report `InvalidDigit` for an unsigned
            // target, but Zig's `parseInt` reports `error.Overflow` for a
            // negative value parsed into an unsigned type (see the `-1` cases in
            // the window width/height tests). Re-classify the signed-negative
            // case here so the asserted error matches the reference.
            IntErrorKind::InvalidDigit if value.starts_with('-') => {
                Err(VariableParseError::Overflow)
            }
            _ => Err(VariableParseError::InvalidCharacter),
        },
    }
}

/// Faithful map of `std.mem.splitScalar(u8, str, delimiter)`: yields each
/// segment between `delimiter` bytes, always producing at least one (possibly
/// empty) segment, and one trailing empty segment after a terminal delimiter.
//
// PORT NOTE: Rust's `str::split` matches `splitScalar`'s semantics exactly
// (leading/trailing/consecutive delimiters all yield empty segments), but the
// reference relies on `it.next()` returning `null` after the final segment to
// detect `ExtraEntry`. A bare `str::split(...)` iterator never reports the
// "one past the end" `null`; this thin wrapper reproduces the `?[]const u8`
// next() contract so the consumed-everything check is byte-faithful.
struct SplitScalar<'a> {
    inner: std::str::Split<'a, char>,
}

impl<'a> SplitScalar<'a> {
    fn new(str: &'a str, delimiter: u8) -> SplitScalar<'a> {
        SplitScalar {
            inner: str.split(delimiter as char),
        }
    }

    fn next(&mut self) -> Option<&'a str> {
        self.inner.next()
    }
}

#[cfg(test)]
// PORT NOTE: the reference asserts flag parses with `expectEqual(true, ...)` /
// `expectEqual(false, ...)`; preserving that literal-bool form
// assertion-for-assertion trips `clippy::bool_assert_comparison`, allowed here.
#[allow(clippy::bool_assert_comparison)]
mod tests {
    use super::*;

    // PORT NOTE: helpers that unwrap the parsed `Value` to the concrete shape
    // each reference assertion expects (`bool`, `usize`, `[]const u8`). The
    // `try Variable.parse(...)` call sites in the reference yield that concrete
    // type directly; these collapse our tagged `Value` back to it.
    fn pb(v: Variable, s: &str) -> Result<bool, VariableParseError> {
        match v.parse(s)? {
            Value::Bool(b) => Ok(b),
            _ => unreachable!(),
        }
    }
    fn pu(v: Variable, s: &str) -> Result<usize, VariableParseError> {
        match v.parse(s)? {
            Value::Usize(n) => Ok(n),
            _ => unreachable!(),
        }
    }
    fn ps(v: Variable, s: &str) -> Result<&str, VariableParseError> {
        match v.parse(s)? {
            Value::Str(t) => Ok(t),
            _ => unreachable!(),
        }
    }

    #[test]
    fn parse_alternate_on() {
        assert_eq!(true, pb(Variable::AlternateOn, "1").unwrap());
        assert_eq!(false, pb(Variable::AlternateOn, "0").unwrap());
        assert_eq!(false, pb(Variable::AlternateOn, "").unwrap());
        assert_eq!(false, pb(Variable::AlternateOn, "true").unwrap());
        assert_eq!(false, pb(Variable::AlternateOn, "yes").unwrap());
    }

    #[test]
    fn parse_alternate_saved_x() {
        assert_eq!(0, pu(Variable::AlternateSavedX, "0").unwrap());
        assert_eq!(42, pu(Variable::AlternateSavedX, "42").unwrap());
        assert_eq!(
            Err(VariableParseError::InvalidCharacter),
            pu(Variable::AlternateSavedX, "abc")
        );
    }

    #[test]
    fn parse_alternate_saved_y() {
        assert_eq!(0, pu(Variable::AlternateSavedY, "0").unwrap());
        assert_eq!(42, pu(Variable::AlternateSavedY, "42").unwrap());
        assert_eq!(
            Err(VariableParseError::InvalidCharacter),
            pu(Variable::AlternateSavedY, "abc")
        );
    }

    #[test]
    fn parse_cursor_x() {
        assert_eq!(0, pu(Variable::CursorX, "0").unwrap());
        assert_eq!(79, pu(Variable::CursorX, "79").unwrap());
        assert_eq!(
            Err(VariableParseError::InvalidCharacter),
            pu(Variable::CursorX, "abc")
        );
    }

    #[test]
    fn parse_cursor_y() {
        assert_eq!(0, pu(Variable::CursorY, "0").unwrap());
        assert_eq!(23, pu(Variable::CursorY, "23").unwrap());
        assert_eq!(
            Err(VariableParseError::InvalidCharacter),
            pu(Variable::CursorY, "abc")
        );
    }

    #[test]
    fn parse_scroll_region_upper() {
        assert_eq!(0, pu(Variable::ScrollRegionUpper, "0").unwrap());
        assert_eq!(5, pu(Variable::ScrollRegionUpper, "5").unwrap());
        assert_eq!(
            Err(VariableParseError::InvalidCharacter),
            pu(Variable::ScrollRegionUpper, "abc")
        );
    }

    #[test]
    fn parse_scroll_region_lower() {
        assert_eq!(0, pu(Variable::ScrollRegionLower, "0").unwrap());
        assert_eq!(23, pu(Variable::ScrollRegionLower, "23").unwrap());
        assert_eq!(
            Err(VariableParseError::InvalidCharacter),
            pu(Variable::ScrollRegionLower, "abc")
        );
    }

    #[test]
    fn parse_session_id() {
        assert_eq!(42, pu(Variable::SessionId, "$42").unwrap());
        assert_eq!(0, pu(Variable::SessionId, "$0").unwrap());
        assert_eq!(
            Err(VariableParseError::FormatError),
            pu(Variable::SessionId, "0")
        );
        assert_eq!(
            Err(VariableParseError::FormatError),
            pu(Variable::SessionId, "@0")
        );
        assert_eq!(
            Err(VariableParseError::FormatError),
            pu(Variable::SessionId, "$")
        );
        assert_eq!(
            Err(VariableParseError::FormatError),
            pu(Variable::SessionId, "")
        );
        assert_eq!(
            Err(VariableParseError::InvalidCharacter),
            pu(Variable::SessionId, "$abc")
        );
    }

    #[test]
    fn parse_window_id() {
        assert_eq!(42, pu(Variable::WindowId, "@42").unwrap());
        assert_eq!(0, pu(Variable::WindowId, "@0").unwrap());
        assert_eq!(12345, pu(Variable::WindowId, "@12345").unwrap());
        assert_eq!(
            Err(VariableParseError::FormatError),
            pu(Variable::WindowId, "0")
        );
        assert_eq!(
            Err(VariableParseError::FormatError),
            pu(Variable::WindowId, "$0")
        );
        assert_eq!(
            Err(VariableParseError::FormatError),
            pu(Variable::WindowId, "@")
        );
        assert_eq!(
            Err(VariableParseError::FormatError),
            pu(Variable::WindowId, "")
        );
        assert_eq!(
            Err(VariableParseError::InvalidCharacter),
            pu(Variable::WindowId, "@abc")
        );
    }

    #[test]
    fn parse_window_width() {
        assert_eq!(80, pu(Variable::WindowWidth, "80").unwrap());
        assert_eq!(0, pu(Variable::WindowWidth, "0").unwrap());
        assert_eq!(12345, pu(Variable::WindowWidth, "12345").unwrap());
        assert_eq!(
            Err(VariableParseError::InvalidCharacter),
            pu(Variable::WindowWidth, "abc")
        );
        assert_eq!(
            Err(VariableParseError::InvalidCharacter),
            pu(Variable::WindowWidth, "80px")
        );
        assert_eq!(
            Err(VariableParseError::Overflow),
            pu(Variable::WindowWidth, "-1")
        );
    }

    #[test]
    fn parse_window_height() {
        assert_eq!(24, pu(Variable::WindowHeight, "24").unwrap());
        assert_eq!(0, pu(Variable::WindowHeight, "0").unwrap());
        assert_eq!(12345, pu(Variable::WindowHeight, "12345").unwrap());
        assert_eq!(
            Err(VariableParseError::InvalidCharacter),
            pu(Variable::WindowHeight, "abc")
        );
        assert_eq!(
            Err(VariableParseError::InvalidCharacter),
            pu(Variable::WindowHeight, "24px")
        );
        assert_eq!(
            Err(VariableParseError::Overflow),
            pu(Variable::WindowHeight, "-1")
        );
    }

    #[test]
    fn parse_window_layout() {
        assert_eq!("abc123", ps(Variable::WindowLayout, "abc123").unwrap());
        assert_eq!("", ps(Variable::WindowLayout, "").unwrap());
        assert_eq!(
            "a]b,c{d}e(f)",
            ps(Variable::WindowLayout, "a]b,c{d}e(f)").unwrap()
        );
    }

    #[test]
    fn parse_cursor_flag() {
        assert_eq!(true, pb(Variable::CursorFlag, "1").unwrap());
        assert_eq!(false, pb(Variable::CursorFlag, "0").unwrap());
        assert_eq!(false, pb(Variable::CursorFlag, "").unwrap());
        assert_eq!(false, pb(Variable::CursorFlag, "true").unwrap());
    }

    #[test]
    fn parse_insert_flag() {
        assert_eq!(true, pb(Variable::InsertFlag, "1").unwrap());
        assert_eq!(false, pb(Variable::InsertFlag, "0").unwrap());
        assert_eq!(false, pb(Variable::InsertFlag, "").unwrap());
        assert_eq!(false, pb(Variable::InsertFlag, "true").unwrap());
    }

    #[test]
    fn parse_keypad_cursor_flag() {
        assert_eq!(true, pb(Variable::KeypadCursorFlag, "1").unwrap());
        assert_eq!(false, pb(Variable::KeypadCursorFlag, "0").unwrap());
        assert_eq!(false, pb(Variable::KeypadCursorFlag, "").unwrap());
        assert_eq!(false, pb(Variable::KeypadCursorFlag, "true").unwrap());
    }

    #[test]
    fn parse_keypad_flag() {
        assert_eq!(true, pb(Variable::KeypadFlag, "1").unwrap());
        assert_eq!(false, pb(Variable::KeypadFlag, "0").unwrap());
        assert_eq!(false, pb(Variable::KeypadFlag, "").unwrap());
        assert_eq!(false, pb(Variable::KeypadFlag, "true").unwrap());
    }

    #[test]
    fn parse_mouse_any_flag() {
        assert_eq!(true, pb(Variable::MouseAnyFlag, "1").unwrap());
        assert_eq!(false, pb(Variable::MouseAnyFlag, "0").unwrap());
        assert_eq!(false, pb(Variable::MouseAnyFlag, "").unwrap());
        assert_eq!(false, pb(Variable::MouseAnyFlag, "true").unwrap());
    }

    #[test]
    fn parse_mouse_button_flag() {
        assert_eq!(true, pb(Variable::MouseButtonFlag, "1").unwrap());
        assert_eq!(false, pb(Variable::MouseButtonFlag, "0").unwrap());
        assert_eq!(false, pb(Variable::MouseButtonFlag, "").unwrap());
        assert_eq!(false, pb(Variable::MouseButtonFlag, "true").unwrap());
    }

    #[test]
    fn parse_mouse_sgr_flag() {
        assert_eq!(true, pb(Variable::MouseSgrFlag, "1").unwrap());
        assert_eq!(false, pb(Variable::MouseSgrFlag, "0").unwrap());
        assert_eq!(false, pb(Variable::MouseSgrFlag, "").unwrap());
        assert_eq!(false, pb(Variable::MouseSgrFlag, "true").unwrap());
    }

    #[test]
    fn parse_mouse_standard_flag() {
        assert_eq!(true, pb(Variable::MouseStandardFlag, "1").unwrap());
        assert_eq!(false, pb(Variable::MouseStandardFlag, "0").unwrap());
        assert_eq!(false, pb(Variable::MouseStandardFlag, "").unwrap());
        assert_eq!(false, pb(Variable::MouseStandardFlag, "true").unwrap());
    }

    #[test]
    fn parse_mouse_utf8_flag() {
        assert_eq!(true, pb(Variable::MouseUtf8Flag, "1").unwrap());
        assert_eq!(false, pb(Variable::MouseUtf8Flag, "0").unwrap());
        assert_eq!(false, pb(Variable::MouseUtf8Flag, "").unwrap());
        assert_eq!(false, pb(Variable::MouseUtf8Flag, "true").unwrap());
    }

    #[test]
    fn parse_wrap_flag() {
        assert_eq!(true, pb(Variable::WrapFlag, "1").unwrap());
        assert_eq!(false, pb(Variable::WrapFlag, "0").unwrap());
        assert_eq!(false, pb(Variable::WrapFlag, "").unwrap());
        assert_eq!(false, pb(Variable::WrapFlag, "true").unwrap());
    }

    #[test]
    fn parse_bracketed_paste() {
        assert_eq!(true, pb(Variable::BracketedPaste, "1").unwrap());
        assert_eq!(false, pb(Variable::BracketedPaste, "0").unwrap());
        assert_eq!(false, pb(Variable::BracketedPaste, "").unwrap());
        assert_eq!(false, pb(Variable::BracketedPaste, "true").unwrap());
    }

    #[test]
    fn parse_cursor_blinking() {
        assert_eq!(true, pb(Variable::CursorBlinking, "1").unwrap());
        assert_eq!(false, pb(Variable::CursorBlinking, "0").unwrap());
        assert_eq!(false, pb(Variable::CursorBlinking, "").unwrap());
        assert_eq!(false, pb(Variable::CursorBlinking, "true").unwrap());
    }

    #[test]
    fn parse_focus_flag() {
        assert_eq!(true, pb(Variable::FocusFlag, "1").unwrap());
        assert_eq!(false, pb(Variable::FocusFlag, "0").unwrap());
        assert_eq!(false, pb(Variable::FocusFlag, "").unwrap());
        assert_eq!(false, pb(Variable::FocusFlag, "true").unwrap());
    }

    #[test]
    fn parse_mouse_all_flag() {
        assert_eq!(true, pb(Variable::MouseAllFlag, "1").unwrap());
        assert_eq!(false, pb(Variable::MouseAllFlag, "0").unwrap());
        assert_eq!(false, pb(Variable::MouseAllFlag, "").unwrap());
        assert_eq!(false, pb(Variable::MouseAllFlag, "true").unwrap());
    }

    #[test]
    fn parse_origin_flag() {
        assert_eq!(true, pb(Variable::OriginFlag, "1").unwrap());
        assert_eq!(false, pb(Variable::OriginFlag, "0").unwrap());
        assert_eq!(false, pb(Variable::OriginFlag, "").unwrap());
        assert_eq!(false, pb(Variable::OriginFlag, "true").unwrap());
    }

    #[test]
    fn parse_pane_id() {
        assert_eq!(42, pu(Variable::PaneId, "%42").unwrap());
        assert_eq!(0, pu(Variable::PaneId, "%0").unwrap());
        assert_eq!(
            Err(VariableParseError::FormatError),
            pu(Variable::PaneId, "0")
        );
        assert_eq!(
            Err(VariableParseError::FormatError),
            pu(Variable::PaneId, "@0")
        );
        assert_eq!(
            Err(VariableParseError::FormatError),
            pu(Variable::PaneId, "%")
        );
        assert_eq!(
            Err(VariableParseError::FormatError),
            pu(Variable::PaneId, "")
        );
        assert_eq!(
            Err(VariableParseError::InvalidCharacter),
            pu(Variable::PaneId, "%abc")
        );
    }

    #[test]
    fn parse_cursor_colour() {
        assert_eq!("red", ps(Variable::CursorColour, "red").unwrap());
        assert_eq!("#ff0000", ps(Variable::CursorColour, "#ff0000").unwrap());
        assert_eq!("", ps(Variable::CursorColour, "").unwrap());
    }

    #[test]
    fn parse_cursor_shape() {
        assert_eq!("block", ps(Variable::CursorShape, "block").unwrap());
        assert_eq!("underline", ps(Variable::CursorShape, "underline").unwrap());
        assert_eq!("bar", ps(Variable::CursorShape, "bar").unwrap());
        assert_eq!("", ps(Variable::CursorShape, "").unwrap());
    }

    #[test]
    fn parse_pane_tabs() {
        assert_eq!("0,8,16,24", ps(Variable::PaneTabs, "0,8,16,24").unwrap());
        assert_eq!("", ps(Variable::PaneTabs, "").unwrap());
        assert_eq!("0", ps(Variable::PaneTabs, "0").unwrap());
    }

    #[test]
    fn parse_version() {
        assert_eq!("3.5a", ps(Variable::Version, "3.5a").unwrap());
        assert_eq!("3.5", ps(Variable::Version, "3.5").unwrap());
        assert_eq!("next-3.5", ps(Variable::Version, "next-3.5").unwrap());
        assert_eq!("", ps(Variable::Version, "").unwrap());
    }

    #[test]
    fn parse_format_struct_single_field() {
        let t = &[Variable::SessionId];
        let result = parse_format_struct(t, "$42", b' ').unwrap();
        assert_eq!(Value::Usize(42), result[0]);
    }

    #[test]
    fn parse_format_struct_multiple_fields() {
        let t = &[
            Variable::SessionId,
            Variable::WindowId,
            Variable::WindowWidth,
            Variable::WindowHeight,
        ];
        let result = parse_format_struct(t, "$1 @2 80 24", b' ').unwrap();
        assert_eq!(Value::Usize(1), result[0]);
        assert_eq!(Value::Usize(2), result[1]);
        assert_eq!(Value::Usize(80), result[2]);
        assert_eq!(Value::Usize(24), result[3]);
    }

    #[test]
    fn parse_format_struct_with_string_field() {
        let t = &[Variable::WindowId, Variable::WindowLayout];
        let result = parse_format_struct(t, "@5,abc123", b',').unwrap();
        assert_eq!(Value::Usize(5), result[0]);
        assert_eq!(Value::Str("abc123"), result[1]);
    }

    #[test]
    fn parse_format_struct_different_delimiter() {
        let t = &[Variable::WindowWidth, Variable::WindowHeight];
        let result = parse_format_struct(t, "120\t40", b'\t').unwrap();
        assert_eq!(Value::Usize(120), result[0]);
        assert_eq!(Value::Usize(40), result[1]);
    }

    #[test]
    fn parse_format_struct_missing_entry() {
        let t = &[Variable::SessionId, Variable::WindowId];
        assert_eq!(
            Err(ParseError::MissingEntry),
            parse_format_struct(t, "$1", b' ')
        );
    }

    #[test]
    fn parse_format_struct_extra_entry() {
        let t = &[Variable::SessionId];
        assert_eq!(
            Err(ParseError::ExtraEntry),
            parse_format_struct(t, "$1 @2", b' ')
        );
    }

    #[test]
    fn parse_format_struct_format_error() {
        let t = &[Variable::SessionId];
        assert_eq!(
            Err(ParseError::FormatError),
            parse_format_struct(t, "42", b' ')
        );
        assert_eq!(
            Err(ParseError::FormatError),
            parse_format_struct(t, "@42", b' ')
        );
        assert_eq!(
            Err(ParseError::FormatError),
            parse_format_struct(t, "$abc", b' ')
        );
    }

    #[test]
    fn parse_format_struct_empty_string() {
        let t = &[Variable::SessionId];
        assert_eq!(
            Err(ParseError::FormatError),
            parse_format_struct(t, "", b' ')
        );
    }

    #[test]
    fn parse_format_struct_with_empty_layout_field() {
        let t = &[Variable::SessionId, Variable::WindowLayout];
        let result = parse_format_struct(t, "$1,", b',').unwrap();
        assert_eq!(Value::Usize(1), result[0]);
        assert_eq!(Value::Str(""), result[1]);
    }

    // PORT NOTE: the reference `testFormat` checks BOTH `comptimeFormat` and the
    // runtime `format` against `expected`; we do the same, with `comptime_format`
    // delegating to `format` (no separate comptime sizing pass exists in Rust).
    fn test_format(vars: &[Variable], delimiter: u8, expected: &str) {
        let comptime_result = comptime_format(vars, delimiter);
        assert_eq!(expected, comptime_result);

        let writer = format(vars, delimiter);
        assert_eq!(expected, writer);
    }

    #[test]
    fn format_single_variable() {
        test_format(&[Variable::SessionId], b' ', "#{session_id}");
    }

    #[test]
    fn format_multiple_variables() {
        test_format(
            &[
                Variable::SessionId,
                Variable::WindowId,
                Variable::WindowWidth,
                Variable::WindowHeight,
            ],
            b' ',
            "#{session_id} #{window_id} #{window_width} #{window_height}",
        );
    }

    #[test]
    fn format_with_comma_delimiter() {
        test_format(
            &[Variable::WindowId, Variable::WindowLayout],
            b',',
            "#{window_id},#{window_layout}",
        );
    }

    #[test]
    fn format_with_tab_delimiter() {
        test_format(
            &[Variable::WindowWidth, Variable::WindowHeight],
            b'\t',
            "#{window_width}\t#{window_height}",
        );
    }

    #[test]
    fn format_empty_variables() {
        test_format(&[], b' ', "");
    }

    #[test]
    fn format_all_variables() {
        test_format(
            &[
                Variable::SessionId,
                Variable::WindowId,
                Variable::WindowWidth,
                Variable::WindowHeight,
                Variable::WindowLayout,
            ],
            b' ',
            "#{session_id} #{window_id} #{window_width} #{window_height} #{window_layout}",
        );
    }
}
