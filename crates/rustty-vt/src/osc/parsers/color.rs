//! PORTS: ghostty src/terminal/osc/parsers/color.zig
//!
//! PORT STATUS: done

// PARITY: the reference names the OSC operations `osc_4`, `osc_5`, ... which are
// snake/lower-case enum variants; keep them verbatim (R2).
#![allow(non_camel_case_types)]

// const std = @import("std");
// const Allocator = std.mem.Allocator;
//
// const DynamicColor = @import("../../color.zig").Dynamic;
// const SpecialColor = @import("../../color.zig").Special;
// const RGB = @import("../../color.zig").RGB;
// const Parser = @import("../../osc.zig").Parser;
// const Command = @import("../../osc.zig").Command;
//
// const log = std.log.scoped(.osc_color);
// PORT NOTE: Zig imports retargeted to ported crate paths (R2). `DynamicColor`
// is `crate::color::Dynamic`; `SpecialColor` is `crate::color::Special`; `RGB`
// is `crate::color::RGB`; `Parser`/`Command` are from the parent `osc` module.
// The scoped logger `log` has no facade wired into this crate (matching the
// convention used elsewhere in rustty-vt), so its single `log.info` call is
// ported as a commented line at its call site.
use crate::color::Dynamic as DynamicColor;
use crate::color::Special as SpecialColor;
use crate::color::RGB;
use crate::osc::{Command, Parser};

// const ParseError = Allocator.Error || error{
//     MissingOperation,
// };
// PORT NOTE: `ParseError` is `Allocator.Error || error{MissingOperation}`. The
// ported `List` is a `Vec<Request>` (see the parent stub note), so there is no
// allocation-failure path, and `MissingOperation` is never actually returned by
// the body. `parseColor` therefore cannot fail in the port; its callers' `catch`
// path (logging + empty list) is preserved as a comment at the call site. The
// error set is carried here verbatim as a comment per R2.

/// The possible operations we support for colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    osc_4,
    osc_5,
    osc_10,
    osc_11,
    osc_12,
    osc_13,
    osc_14,
    osc_15,
    osc_16,
    osc_17,
    osc_18,
    osc_19,
    osc_104,
    osc_105,
    osc_110,
    osc_111,
    osc_112,
    osc_113,
    osc_114,
    osc_115,
    osc_116,
    osc_117,
    osc_118,
    osc_119,
}

/// Parse OSCs 4, 5, 10-19, 104, 110-119
pub fn parse<'a>(parser: &'a mut Parser, terminator_ch: Option<u8>) -> Option<&'a mut Command> {
    // const alloc = parser.alloc orelse {
    //     parser.state = .invalid;
    //     return null;
    // };
    // PORT NOTE: the reference requires an allocator (it backs the `List`). The
    // ported `List` is a `Vec<Request>` that allocates via the global allocator,
    // but the reference's gate is faithful behavior: with no allocator the parse
    // is abandoned. We preserve that gate by checking `parser.alloc`.
    if parser.alloc.is_none() {
        parser.state = crate::osc::State::Invalid;
        return None;
    }
    // If we've collected any extra data parse that, otherwise use an empty
    // string.
    let data: &[u8] = match parser.capture.as_ref() {
        Some(c) => c.trailing(),
        None => b"",
    };
    // Check and make sure that we're parsing the correct OSCs
    let op: Operation = match parser.state {
        crate::osc::State::S4 => Operation::osc_4,
        crate::osc::State::S5 => Operation::osc_5,
        crate::osc::State::S10 => Operation::osc_10,
        crate::osc::State::S11 => Operation::osc_11,
        crate::osc::State::S12 => Operation::osc_12,
        crate::osc::State::S13 => Operation::osc_13,
        crate::osc::State::S14 => Operation::osc_14,
        crate::osc::State::S15 => Operation::osc_15,
        crate::osc::State::S16 => Operation::osc_16,
        crate::osc::State::S17 => Operation::osc_17,
        crate::osc::State::S18 => Operation::osc_18,
        crate::osc::State::S19 => Operation::osc_19,
        crate::osc::State::S104 => Operation::osc_104,
        crate::osc::State::S110 => Operation::osc_110,
        crate::osc::State::S111 => Operation::osc_111,
        crate::osc::State::S112 => Operation::osc_112,
        crate::osc::State::S113 => Operation::osc_113,
        crate::osc::State::S114 => Operation::osc_114,
        crate::osc::State::S115 => Operation::osc_115,
        crate::osc::State::S116 => Operation::osc_116,
        crate::osc::State::S117 => Operation::osc_117,
        crate::osc::State::S118 => Operation::osc_118,
        crate::osc::State::S119 => Operation::osc_119,
        _ => {
            parser.state = crate::osc::State::Invalid;
            return None;
        }
    };
    // parser.command = .{
    //     .color_operation = .{
    //         .op = op,
    //         .requests = parseColor(alloc, op, data) catch |err| list: {
    //             log.info(
    //                 "failed to parse OSC {t} color request err={} data={s}",
    //                 .{ parser.state, err, data },
    //             );
    //             break :list .{};
    //         },
    //         .terminator = .init(terminator_ch),
    //     },
    // };
    // PORT NOTE: `parseColor` cannot fail in the port (see the `ParseError`
    // note), so the `catch` branch (logging + empty list) is unreachable and
    // carried as a comment above. We assign the parsed list directly.
    parser.command = Command::ColorOperation {
        op,
        requests: parse_color(op, data),
        terminator: crate::osc::Terminator::init(terminator_ch),
    };
    Some(&mut parser.command)
}

/// Parse any color operation string. This should NOT include the operation
/// itself, but only the body of the operation. e.g. for "4;a;b;c" the body
/// should be "a;b;c" and the operation should be set accordingly.
///
/// Color parsing is fairly complicated so we pull this out to a specialized
/// function rather than go through our OSC parsing state machine. This is
/// much slower and requires more memory (since we need to buffer the full
/// request) but grants us an easier to understand and testable implementation.
///
/// If color changing ends up being a bottleneck we can optimize this later.
fn parse_color(op: Operation, buf: &[u8]) -> List {
    // var it = std.mem.tokenizeScalar(u8, buf, ';');
    let mut it = TokenizeScalar::new(buf, b';');
    match op {
        Operation::osc_4 => parse_get_set_ansi_color(Operation::osc_4, &mut it),
        Operation::osc_5 => parse_get_set_ansi_color(Operation::osc_5, &mut it),
        Operation::osc_104 => parse_reset_ansi_color(Operation::osc_104, &mut it),
        Operation::osc_105 => parse_reset_ansi_color(Operation::osc_105, &mut it),
        Operation::osc_10 => parse_get_set_dynamic_color(DynamicColor::Foreground, &mut it),
        Operation::osc_11 => parse_get_set_dynamic_color(DynamicColor::Background, &mut it),
        Operation::osc_12 => parse_get_set_dynamic_color(DynamicColor::Cursor, &mut it),
        Operation::osc_13 => parse_get_set_dynamic_color(DynamicColor::PointerForeground, &mut it),
        Operation::osc_14 => parse_get_set_dynamic_color(DynamicColor::PointerBackground, &mut it),
        Operation::osc_15 => {
            parse_get_set_dynamic_color(DynamicColor::TektronixForeground, &mut it)
        }
        Operation::osc_16 => {
            parse_get_set_dynamic_color(DynamicColor::TektronixBackground, &mut it)
        }
        Operation::osc_17 => {
            parse_get_set_dynamic_color(DynamicColor::HighlightBackground, &mut it)
        }
        Operation::osc_18 => parse_get_set_dynamic_color(DynamicColor::TektronixCursor, &mut it),
        Operation::osc_19 => {
            parse_get_set_dynamic_color(DynamicColor::HighlightForeground, &mut it)
        }
        Operation::osc_110 => parse_reset_dynamic_color(DynamicColor::Foreground, &mut it),
        Operation::osc_111 => parse_reset_dynamic_color(DynamicColor::Background, &mut it),
        Operation::osc_112 => parse_reset_dynamic_color(DynamicColor::Cursor, &mut it),
        Operation::osc_113 => parse_reset_dynamic_color(DynamicColor::PointerForeground, &mut it),
        Operation::osc_114 => parse_reset_dynamic_color(DynamicColor::PointerBackground, &mut it),
        Operation::osc_115 => parse_reset_dynamic_color(DynamicColor::TektronixForeground, &mut it),
        Operation::osc_116 => parse_reset_dynamic_color(DynamicColor::TektronixBackground, &mut it),
        Operation::osc_117 => parse_reset_dynamic_color(DynamicColor::HighlightBackground, &mut it),
        Operation::osc_118 => parse_reset_dynamic_color(DynamicColor::TektronixCursor, &mut it),
        Operation::osc_119 => parse_reset_dynamic_color(DynamicColor::HighlightForeground, &mut it),
    }
}

/// OSC 4/5
fn parse_get_set_ansi_color(op: Operation, it: &mut TokenizeScalar<'_>) -> List {
    // Note: in ANY error scenario below we return the accumulated results.
    // This matches the xterm behavior (see misc.c ChangeAnsiColorRequest)

    let mut result: List = List::new();
    // errdefer result.deinit(alloc);
    // PORT NOTE: `errdefer` cleanup is automatic via `Vec`'s `Drop`.
    loop {
        // We expect a `c; spec` pair. If either doesn't exist then
        // we return the results up to this point.
        let color_str = match it.next() {
            Some(v) => v,
            None => return result,
        };
        let spec_str = match it.next() {
            Some(v) => v,
            None => return result,
        };

        // Color must be numeric. u9 because that'll fit our palette + special
        // const color: u9 = std.fmt.parseInt(u9, color_str, 10) catch return result;
        // PORT NOTE: `u9` (0..=511) has no Rust primitive; parse as `u16` then
        // reject values that overflow `u9` (so a `parseInt(u9)` failure and a
        // `> 511` value both bail to the accumulated results).
        let color: u16 = match parse_int_u9(color_str) {
            Some(v) => v,
            None => return result,
        };

        // Parse the color.
        let target: Target = match op {
            // OSC5 maps directly to the Special enum.
            Operation::osc_5 => Target::Special({
                let cast = match cast_u3(color) {
                    Some(v) => v,
                    None => return result,
                };
                match int_to_special(cast) {
                    Some(v) => v,
                    None => return result,
                }
            }),

            // OSC4 maps 0-255 to palette, 256-259 to special offset
            // by the palette count.
            Operation::osc_4 => match cast_u8(color) {
                Some(idx) => Target::Palette(idx),
                None => Target::Special({
                    let cast = match cast_u3(color - 256) {
                        Some(v) => v,
                        None => return result,
                    };
                    match int_to_special(cast) {
                        Some(v) => v,
                        None => return result,
                    }
                }),
            },

            // else => comptime unreachable,
            _ => unreachable!(),
        };

        // "?" always results in a query.
        if spec_str == b"?" {
            // const req = try result.addOne(alloc);
            // req.* = .{ .query = target };
            result.push(Request::Query(target));
            continue;
        }

        // const rgb = RGB.parse(spec_str) catch return result;
        let rgb = match str_from_bytes(spec_str).and_then(|s| RGB::parse(s).ok()) {
            Some(v) => v,
            None => return result,
        };
        // const req = try result.addOne(alloc);
        // req.* = .{ .set = .{ .target = target, .color = rgb } };
        result.push(Request::Set(ColoredTarget { target, color: rgb }));
    }
}

/// OSC 104/105: Reset ANSI Colors
fn parse_reset_ansi_color(op: Operation, it: &mut TokenizeScalar<'_>) -> List {
    // Note: xterm stops parsing the reset list on any error, but we're
    // more flexible and try the next value. This matches the behavior of
    // Kitty and I don't see a downside to being more flexible here. Hopefully
    // no one depends on the exact behavior of xterm.

    let mut result: List = List::new();
    // errdefer result.deinit(alloc);
    loop {
        let color_str = match it.next() {
            Some(v) => v,
            None => {
                // If no parameters are given, we reset the full table.
                // PORT NOTE: `result.count() == 0` => `result.is_empty()`.
                if result.is_empty() {
                    // const req = try result.addOne(alloc);
                    // req.* = switch (op) { ... };
                    let req = match op {
                        Operation::osc_104 => Request::ResetPalette,
                        Operation::osc_105 => Request::ResetSpecial,
                        _ => unreachable!(),
                    };
                    result.push(req);
                }
                return result;
            }
        };

        // Empty color strings are ignored, not treated as an error.
        if color_str.is_empty() {
            continue;
        }

        // Color must be numeric. u9 because that'll fit our palette + special
        // const color: u9 = std.fmt.parseInt(u9, color_str, 10) catch continue;
        let color: u16 = match parse_int_u9(color_str) {
            Some(v) => v,
            None => continue,
        };

        // Parse the color.
        let target: Target = match op {
            // OSC105 maps directly to the Special enum.
            Operation::osc_105 => Target::Special({
                let cast = match cast_u3(color) {
                    Some(v) => v,
                    None => continue,
                };
                match int_to_special(cast) {
                    Some(v) => v,
                    None => continue,
                }
            }),

            // OSC104 maps 0-255 to palette, 256-259 to special offset
            // by the palette count.
            Operation::osc_104 => match cast_u8(color) {
                Some(idx) => Target::Palette(idx),
                None => Target::Special({
                    let cast = match cast_u3(color - 256) {
                        Some(v) => v,
                        None => continue,
                    };
                    match int_to_special(cast) {
                        Some(v) => v,
                        None => continue,
                    }
                }),
            },

            // else => comptime unreachable,
            _ => unreachable!(),
        };

        // const req = try result.addOne(alloc);
        // req.* = .{ .reset = target };
        result.push(Request::Reset(target));
    }
}

/// OSC 10-19: Get/Set Dynamic Colors
fn parse_get_set_dynamic_color(start: DynamicColor, it: &mut TokenizeScalar<'_>) -> List {
    // Note: in ANY error scenario below we return the accumulated results.
    // This matches the xterm behavior (see misc.c ChangeColorsRequest)

    let mut result: List = List::new();
    let mut color: DynamicColor = start;
    loop {
        let spec_str = match it.next() {
            Some(v) => v,
            None => return result,
        };

        if spec_str == b"?" {
            // const req = try result.addOne(alloc);
            // req.* = .{ .query = .{ .dynamic = color } };
            result.push(Request::Query(Target::Dynamic(color)));
        } else {
            // const rgb = RGB.parse(spec_str) catch return result;
            let rgb = match str_from_bytes(spec_str).and_then(|s| RGB::parse(s).ok()) {
                Some(v) => v,
                None => return result,
            };
            // const req = try result.addOne(alloc);
            // req.* = .{ .set = .{ .target = .{ .dynamic = color }, .color = rgb } };
            result.push(Request::Set(ColoredTarget {
                target: Target::Dynamic(color),
                color: rgb,
            }));
        }

        // Each successive value uses the next color so long as it exists.
        // color = color.next() orelse return result;
        color = match color.next() {
            Some(v) => v,
            None => return result,
        };
    }
}

/// OSC 110-119: Reset Dynamic Colors
fn parse_reset_dynamic_color(color: DynamicColor, it: &mut TokenizeScalar<'_>) -> List {
    let mut result: List = List::new();
    // errdefer result.deinit(alloc);
    // if (it.next() != null) return result;
    if it.next().is_some() {
        return result;
    }
    // const req = try result.addOne(alloc);
    // req.* = .{ .reset = .{ .dynamic = color } };
    result.push(Request::Reset(Target::Dynamic(color)));
    result
}

/// A segmented list is used to avoid copying when many operations
/// are given in a single OSC. In most cases, OSC 4/104/etc. send
/// very few so the prealloc is optimized for that.
///
/// The exact prealloc value is chosen arbitrarily assuming most
/// color ops have very few. If we can get empirical data on more
/// typical values we can switch to that.
// pub const List = std.SegmentedList(Request, 2);
// PORT NOTE: `std.SegmentedList(Request, 2)` is a segmented list with 2 prealloc
// slots. `std.SegmentedList` is not ported; the storage is modeled as a
// `Vec<Request>` carrying the same elements. The prealloc-count tuning (2) is a
// perf detail with no observable behavior.
pub type List = Vec<Request>;

/// A single operation related to the terminal color palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
    Set(ColoredTarget),
    Query(Target),
    Reset(Target),
    ResetPalette,
    ResetSpecial,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Palette(u8),
    Special(SpecialColor),
    Dynamic(DynamicColor),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColoredTarget {
    pub target: Target,
    pub color: RGB,
}

// PORT NOTE: glue mirroring `std.mem.tokenizeScalar(u8, buf, delimiter)`. It
// yields the non-empty runs of bytes between `delimiter` occurrences, skipping
// any run of consecutive delimiters (exactly the scalar tokenizer semantics).
struct TokenizeScalar<'a> {
    buf: &'a [u8],
    delimiter: u8,
    index: usize,
}

impl<'a> TokenizeScalar<'a> {
    fn new(buf: &'a [u8], delimiter: u8) -> TokenizeScalar<'a> {
        TokenizeScalar {
            buf,
            delimiter,
            index: 0,
        }
    }

    fn next(&mut self) -> Option<&'a [u8]> {
        // Skip leading delimiters.
        while self.index < self.buf.len() && self.buf[self.index] == self.delimiter {
            self.index += 1;
        }
        if self.index >= self.buf.len() {
            return None;
        }
        let start = self.index;
        while self.index < self.buf.len() && self.buf[self.index] != self.delimiter {
            self.index += 1;
        }
        Some(&self.buf[start..self.index])
    }
}

// PORT NOTE: glue for `std.fmt.parseInt(u9, s, 10)`. Returns `None` on any parse
// failure or when the value overflows `u9` (0..=511), mirroring the Zig integer
// width. Parses ASCII bytes only (the tokenizer yields byte slices).
fn parse_int_u9(s: &[u8]) -> Option<u16> {
    let text = str_from_bytes(s)?;
    match text.parse::<u16>() {
        Ok(v) if v <= 511 => Some(v),
        _ => None,
    }
}

// PORT NOTE: glue for `std.math.cast(u8, color)` — returns the value if it fits
// in a `u8`, else `None`.
fn cast_u8(color: u16) -> Option<u8> {
    if color <= 255 {
        Some(color as u8)
    } else {
        None
    }
}

// PORT NOTE: glue for `std.math.cast(u3, color)` — returns the value if it fits
// in a `u3` (0..=7), else `None`.
fn cast_u3(color: u16) -> Option<u8> {
    if color <= 7 {
        Some(color as u8)
    } else {
        None
    }
}

// PORT NOTE: glue for `std.meta.intToEnum(SpecialColor, x)`. `SpecialColor` is
// backed by `enum(u3)` with 5 named values (0..=4); other values yield `None`.
fn int_to_special(v: u8) -> Option<SpecialColor> {
    match v {
        0 => Some(SpecialColor::Bold),
        1 => Some(SpecialColor::Underline),
        2 => Some(SpecialColor::Blink),
        3 => Some(SpecialColor::Reverse),
        4 => Some(SpecialColor::Italic),
        _ => None,
    }
}

// PORT NOTE: glue to view a byte slice as `&str` for the `&str`-based ported
// `RGB::parse`. Non-UTF-8 bytes yield `None`, which the callers fold into the
// reference's parse-failure path. The tokens here are always ASCII in practice.
fn str_from_bytes(s: &[u8]) -> Option<&str> {
    std::str::from_utf8(s).ok()
}

#[cfg(test)]
mod tests {
    // PARITY: several tests faithfully mirror the reference's index-based loops
    // `for (0..fields.len) |i|` where `i` is used both to index the field table
    // and in arithmetic (`256 + i`, `format!("{};red", i)`); the `enumerate`
    // rewrite clippy suggests would diverge from that structure, so allow it.
    #![allow(clippy::needless_range_loop)]

    use super::*;

    // test "OSC 4: empty param" {
    #[test]
    fn osc_4_empty_param() {
        // var p: Parser = .init(null);
        let mut p = Parser::init(None);

        let input = b"4;;";
        for &ch in input {
            p.next(ch);
        }

        let cmd = p.end(Some(b'\x1b'));
        // try testing.expect(cmd == null);
        assert!(cmd.is_none());
    }

    // PORT NOTE: the reference tests call `parseColor(alloc, op, body)` directly
    // and then `list.deinit(alloc)`. The ported `parse_color` takes only
    // `(op, body)` (the `List` is a `Vec` that frees itself), so the allocator
    // argument and the `deinit` are dropped. `list.count()` => `list.len()`,
    // `list.at(0).*` => `list[0]`. Each `intToEnum`/`comptimePrint` reflection
    // in the loop bodies is reproduced with the explicit enum tables below.

    // PORT NOTE: glue mirroring the test-only `std.meta.intToEnum(SpecialColor,
    // i)` over the 5 special-color fields, used to enumerate them by index.
    const SPECIAL_FIELDS: [SpecialColor; 5] = [
        SpecialColor::Bold,
        SpecialColor::Underline,
        SpecialColor::Blink,
        SpecialColor::Reverse,
        SpecialColor::Italic,
    ];

    // PORT NOTE: glue mirroring `@typeInfo(DynamicColor).@"enum".fields` — the
    // 10 dynamic-color fields paired with their integer values (10..=19), used
    // to build the `osc_{value}` / `osc_1{value}` operations in the loops.
    const DYNAMIC_FIELDS: [(DynamicColor, u32); 10] = [
        (DynamicColor::Foreground, 10),
        (DynamicColor::Background, 11),
        (DynamicColor::Cursor, 12),
        (DynamicColor::PointerForeground, 13),
        (DynamicColor::PointerBackground, 14),
        (DynamicColor::TektronixForeground, 15),
        (DynamicColor::TektronixBackground, 16),
        (DynamicColor::HighlightBackground, 17),
        (DynamicColor::TektronixCursor, 18),
        (DynamicColor::HighlightForeground, 19),
    ];

    // test "OSC 4:" {
    #[test]
    fn osc_4() {
        // Test every palette index
        // for (0..std.math.maxInt(u8)) |idx|  -- 0..255 (exclusive of 255)
        for idx in 0..255u16 {
            // Simple color set
            // printf '\e]4;0;red\\'
            {
                let body = format!("{};red", idx);

                let list = parse_color(Operation::osc_4, body.as_bytes());
                assert_eq!(1, list.len());
                assert_eq!(
                    Request::Set(ColoredTarget {
                        target: Target::Palette(idx as u8),
                        color: RGB { r: 255, g: 0, b: 0 },
                    }),
                    list[0],
                );
            }

            // Simple color query
            // printf '\e]4;0;?\\'
            {
                let body = format!("{};?", idx);

                let list = parse_color(Operation::osc_4, body.as_bytes());
                assert_eq!(1, list.len());
                assert_eq!(Request::Query(Target::Palette(idx as u8)), list[0],);
            }

            // Trailing invalid data produces results up to that point
            // printf '\e]4;0;red;\e\\'
            {
                let body = format!("{};red;", idx);

                let list = parse_color(Operation::osc_4, body.as_bytes());
                assert_eq!(1, list.len());
                assert_eq!(
                    Request::Set(ColoredTarget {
                        target: Target::Palette(idx as u8),
                        color: RGB { r: 255, g: 0, b: 0 },
                    }),
                    list[0],
                );
            }

            // Whitespace doesn't produce a working value in xterm but we
            // allow it because Kitty does and it seems harmless.
            //
            // printf '\e]4;0;red \e\\'
            {
                let body = format!("{};red ", idx);

                let list = parse_color(Operation::osc_4, body.as_bytes());
                assert_eq!(1, list.len());
                assert_eq!(
                    Request::Set(ColoredTarget {
                        target: Target::Palette(idx as u8),
                        color: RGB { r: 255, g: 0, b: 0 },
                    }),
                    list[0],
                );
            }
        }

        // Test every special color
        // for (0..@typeInfo(SpecialColor).@"enum".fields.len) |i|
        for i in 0..SPECIAL_FIELDS.len() {
            let special = SPECIAL_FIELDS[i];

            // Simple color set
            // printf '\e]4;256;red\\'
            {
                let body = format!("{};red", 256 + i);

                let list = parse_color(Operation::osc_4, body.as_bytes());
                assert_eq!(1, list.len());
                assert_eq!(
                    Request::Set(ColoredTarget {
                        target: Target::Special(special),
                        color: RGB { r: 255, g: 0, b: 0 },
                    }),
                    list[0],
                );
            }
        }
    }

    // test "OSC 5:" {
    #[test]
    fn osc_5() {
        // Test every special color
        for i in 0..SPECIAL_FIELDS.len() {
            let special = SPECIAL_FIELDS[i];

            // Simple color set
            // printf '\e]4;256;red\\'
            {
                let body = format!("{};red", i);

                let list = parse_color(Operation::osc_5, body.as_bytes());
                assert_eq!(1, list.len());
                assert_eq!(
                    Request::Set(ColoredTarget {
                        target: Target::Special(special),
                        color: RGB { r: 255, g: 0, b: 0 },
                    }),
                    list[0],
                );
            }
        }
    }

    // test "OSC 4: multiple requests" {
    #[test]
    fn osc_4_multiple_requests() {
        // printf '\e]4;0;red;1;blue\e\\'
        {
            let list = parse_color(Operation::osc_4, b"0;red;1;blue");
            assert_eq!(2, list.len());
            assert_eq!(
                Request::Set(ColoredTarget {
                    target: Target::Palette(0),
                    color: RGB { r: 255, g: 0, b: 0 },
                }),
                list[0],
            );
            assert_eq!(
                Request::Set(ColoredTarget {
                    target: Target::Palette(1),
                    color: RGB { r: 0, g: 0, b: 255 },
                }),
                list[1],
            );
        }

        // Multiple requests with same index overwrite each other
        // printf '\e]4;0;red;0;blue\e\\'
        {
            let list = parse_color(Operation::osc_4, b"0;red;0;blue");
            assert_eq!(2, list.len());
            assert_eq!(
                Request::Set(ColoredTarget {
                    target: Target::Palette(0),
                    color: RGB { r: 255, g: 0, b: 0 },
                }),
                list[0],
            );
            assert_eq!(
                Request::Set(ColoredTarget {
                    target: Target::Palette(0),
                    color: RGB { r: 0, g: 0, b: 255 },
                }),
                list[1],
            );
        }
    }

    // test "OSC 104:" {
    #[test]
    fn osc_104() {
        // Test every palette index
        for idx in 0..255u16 {
            // Simple color set
            // printf '\e]104;0\\'
            {
                let body = format!("{}", idx);

                let list = parse_color(Operation::osc_104, body.as_bytes());
                assert_eq!(1, list.len());
                assert_eq!(Request::Reset(Target::Palette(idx as u8)), list[0],);
            }
        }

        // Test every special color
        for i in 0..SPECIAL_FIELDS.len() {
            let special = SPECIAL_FIELDS[i];

            // Simple color set
            // printf '\e]104;256\\'
            {
                let body = format!("{}", 256 + i);

                let list = parse_color(Operation::osc_104, body.as_bytes());
                assert_eq!(1, list.len());
                assert_eq!(Request::Reset(Target::Special(special)), list[0],);
            }
        }
    }

    // test "OSC 104: empty index" {
    #[test]
    fn osc_104_empty_index() {
        let list = parse_color(Operation::osc_104, b"0;;1");
        assert_eq!(2, list.len());
        assert_eq!(Request::Reset(Target::Palette(0)), list[0],);
        assert_eq!(Request::Reset(Target::Palette(1)), list[1],);
    }

    // test "OSC 104: invalid index" {
    #[test]
    fn osc_104_invalid_index() {
        let list = parse_color(Operation::osc_104, b"ffff;1");
        assert_eq!(1, list.len());
        assert_eq!(Request::Reset(Target::Palette(1)), list[0],);
    }

    // test "OSC 104: reset all" {
    #[test]
    fn osc_104_reset_all() {
        let list = parse_color(Operation::osc_104, b"");
        assert_eq!(1, list.len());
        assert_eq!(Request::ResetPalette, list[0],);
    }

    // test "OSC 105: reset all" {
    #[test]
    fn osc_105_reset_all() {
        let list = parse_color(Operation::osc_105, b"");
        assert_eq!(1, list.len());
        assert_eq!(Request::ResetSpecial, list[0],);
    }

    // OSC 10-19: Get/Set Dynamic Colors
    // test "OSC 10: OSC 11: OSC 12: OSC: 13: OSC 14: OSC 15: OSC: 16: OSC 17: OSC 18: OSC 19: dynamic" {
    #[test]
    fn osc_10_osc_11_osc_12_osc_13_osc_14_osc_15_osc_16_osc_17_osc_18_osc_19_dynamic() {
        // inline for (@typeInfo(DynamicColor).@"enum".fields) |field|
        for (color, value) in DYNAMIC_FIELDS {
            // const op = @field(Operation, std.fmt.comptimePrint("osc_{d}", .{field.value}));
            let op = operation_from_value(value);

            // Example script:
            // printf '\e]10;red\e\\'
            {
                let list = parse_color(op, b"red");
                assert_eq!(1, list.len());
                assert_eq!(
                    Request::Set(ColoredTarget {
                        target: Target::Dynamic(color),
                        color: RGB { r: 255, g: 0, b: 0 },
                    }),
                    list[0],
                );
            }
        }
    }

    // test "OSC 10: ... dynamic multiple" {
    #[test]
    fn osc_10_osc_11_osc_12_osc_13_osc_14_osc_15_osc_16_osc_17_osc_18_osc_19_dynamic_multiple() {
        // Example script:
        // printf '\e]11;red;blue\e\\'
        {
            let list = parse_color(Operation::osc_11, b"red;blue");
            assert_eq!(2, list.len());
            assert_eq!(
                Request::Set(ColoredTarget {
                    target: Target::Dynamic(DynamicColor::Background),
                    color: RGB { r: 255, g: 0, b: 0 },
                }),
                list[0],
            );
            assert_eq!(
                Request::Set(ColoredTarget {
                    target: Target::Dynamic(DynamicColor::Cursor),
                    color: RGB { r: 0, g: 0, b: 255 },
                }),
                list[1],
            );
        }
    }

    // OSC 110-119: Reset Dynamic Colors
    // test "OSC 110: ... reset dynamic" {
    #[test]
    fn osc_110_osc_111_osc_112_osc_113_osc_114_osc_115_osc_116_osc_117_osc_118_osc_119_reset_dynamic(
    ) {
        // inline for (@typeInfo(DynamicColor).@"enum".fields) |field|
        for (color, value) in DYNAMIC_FIELDS {
            // const op = @field(Operation, std.fmt.comptimePrint("osc_1{d}", .{field.value}));
            let op = operation_from_reset_value(value);

            // Example script:
            // printf '\e]110\e\\'
            {
                let list = parse_color(op, b"");
                assert_eq!(1, list.len());
                assert_eq!(Request::Reset(Target::Dynamic(color)), list[0],);
            }

            // xterm allows a trailing semicolon. script to verify:
            //
            // printf '\e]110;\e\\'
            {
                let list = parse_color(op, b";");
                assert_eq!(1, list.len());
                assert_eq!(Request::Reset(Target::Dynamic(color)), list[0],);
            }

            // xterm does NOT allow any whitespace
            //
            // printf '\e]110 \e\\'
            {
                let list = parse_color(op, b" ");
                assert_eq!(0, list.len());
            }
        }
    }

    // PORT NOTE: glue mirroring `@field(Operation, comptimePrint("osc_{d}", value))`
    // for the dynamic-color values (10..=19).
    fn operation_from_value(value: u32) -> Operation {
        match value {
            10 => Operation::osc_10,
            11 => Operation::osc_11,
            12 => Operation::osc_12,
            13 => Operation::osc_13,
            14 => Operation::osc_14,
            15 => Operation::osc_15,
            16 => Operation::osc_16,
            17 => Operation::osc_17,
            18 => Operation::osc_18,
            19 => Operation::osc_19,
            _ => unreachable!(),
        }
    }

    // PORT NOTE: glue mirroring `@field(Operation, comptimePrint("osc_1{d}", value))`
    // for the dynamic-color values (10..=19) => osc_110..osc_119.
    fn operation_from_reset_value(value: u32) -> Operation {
        match value {
            10 => Operation::osc_110,
            11 => Operation::osc_111,
            12 => Operation::osc_112,
            13 => Operation::osc_113,
            14 => Operation::osc_114,
            15 => Operation::osc_115,
            16 => Operation::osc_116,
            17 => Operation::osc_117,
            18 => Operation::osc_118,
            19 => Operation::osc_119,
            _ => unreachable!(),
        }
    }
}
