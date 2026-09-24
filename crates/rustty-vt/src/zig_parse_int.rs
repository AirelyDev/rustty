//! A port of Zig's `std.fmt.parseInt` (zig 0.16.0 `std/fmt.zig:317-460`,
//! `charToDigit` at `:570-581`), for the terminal parsers whose reference
//! calls it on untrusted input.
//!
//! PORT NOTE: Rust's `str::parse`/`from_str_radix` are not equivalent. They
//! reject the interior `_` digit separators that Zig accepts (`"1_0"` is 10),
//! reject `"-0"` for an unsigned type (Zig returns 0), and report different
//! error kinds in several orderings. The parsers that call `std.fmt.parseInt`
//! go through this module instead (parity S170-F1, S186-F1, S194-F3, S195-F2,
//! S197-F1).
//!
//! Zig's result type is a comptime integer type of any width (`u4`, `u9`,
//! `u21`, ...). Rust has no such types, so the width and signedness are a
//! runtime [`IntType`] and the value is returned widened to `i128`; the
//! [`ZigInt`] trait gives the native-width callers a typed result.

// pub const ParseIntError = error{
//     /// The result cannot fit in the type specified.
//     Overflow,
//     /// The input was empty or contained an invalid character.
//     InvalidCharacter,
// };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ParseIntError {
    /// The result cannot fit in the type specified.
    Overflow,
    /// The input was empty or contained an invalid character.
    InvalidCharacter,
}

/// A Zig integer type: its bit width and signedness.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct IntType {
    pub bits: u32,
    pub signed: bool,
}

impl IntType {
    pub(crate) const fn unsigned(bits: u32) -> IntType {
        IntType {
            bits,
            signed: false,
        }
    }

    pub(crate) const fn signed(bits: u32) -> IntType {
        IntType { bits, signed: true }
    }

    fn min(self) -> i128 {
        if self.signed {
            -(1i128 << (self.bits - 1))
        } else {
            0
        }
    }

    fn max(self) -> i128 {
        if self.signed {
            (1i128 << (self.bits - 1)) - 1
        } else {
            (1i128 << self.bits) - 1
        }
    }

    /// `math.cast(Self, v)`: `Some(v)` when `v` fits in this type.
    fn cast(self, v: i128) -> Option<i128> {
        (self.min() <= v && v <= self.max()).then_some(v)
    }
}

/// A native Rust integer type with a matching Zig integer type.
pub(crate) trait ZigInt: Sized {
    const TYPE: IntType;
    fn from_i128(v: i128) -> Self;
}

macro_rules! zig_int {
    ($($t:ty => $it:expr),* $(,)?) => {$(
        impl ZigInt for $t {
            const TYPE: IntType = $it;
            fn from_i128(v: i128) -> Self {
                <$t>::try_from(v).expect("parse_int result fits its type")
            }
        }
    )*};
}

zig_int! {
    u8 => IntType::unsigned(8),
    u16 => IntType::unsigned(16),
    u32 => IntType::unsigned(32),
    u64 => IntType::unsigned(64),
    i8 => IntType::signed(8),
    i16 => IntType::signed(16),
    i32 => IntType::signed(32),
    i64 => IntType::signed(64),
    usize => IntType::unsigned(usize::BITS),
}

/// `std.fmt.parseInt(T, buf, base)` for a native Rust integer type.
pub(crate) fn parse<T: ZigInt>(buf: &[u8], base: u8) -> Result<T, ParseIntError> {
    parse_int(T::TYPE, buf, base).map(T::from_i128)
}

/// Parses the string `buf` as unsigned representation in the specified base
/// of an integral value of type `T`.
///
/// When `base` is zero the string prefix is examined to detect the true base:
///  * A prefix of "0b" implies base=2,
///  * A prefix of "0o" implies base=8,
///  * A prefix of "0x" implies base=16,
///  * Otherwise base=10 is assumed.
///
/// Ignores '_' character in `buf`.
/// See also `parseInt`.
// pub fn parseUnsigned(comptime T: type, buf: []const u8, base: u8) ParseIntError!T {
//     return parseIntWithSign(T, u8, buf, base, .pos);
// }
pub(crate) fn parse_unsigned<T: ZigInt>(buf: &[u8], base: u8) -> Result<T, ParseIntError> {
    parse_int_with_sign(T::TYPE, buf, base, false).map(T::from_i128)
}

/// Parses the string `buf` as signed or unsigned representation in the
/// specified base of an integral value of type `T`.
///
/// When `base` is zero the string prefix is examined to detect the true base:
///  * A prefix of "0b" implies base=2,
///  * A prefix of "0o" implies base=8,
///  * A prefix of "0x" implies base=16,
///  * Otherwise base=10 is assumed.
///
/// Ignores '_' character in `buf`.
// pub fn parseInt(comptime T: type, buf: []const u8, base: u8) ParseIntError!T {
//     return parseIntWithGenericCharacter(T, u8, buf, base);
// }
pub(crate) fn parse_int(t: IntType, buf: &[u8], base: u8) -> Result<i128, ParseIntError> {
    // if (buf.len == 0) return error.InvalidCharacter;
    if buf.is_empty() {
        return Err(ParseIntError::InvalidCharacter);
    }
    // if (buf[0] == '+') return parseIntWithSign(Result, Character, buf[1..], base, .pos);
    if buf[0] == b'+' {
        return parse_int_with_sign(t, &buf[1..], base, false);
    }
    // if (buf[0] == '-') return parseIntWithSign(Result, Character, buf[1..], base, .neg);
    if buf[0] == b'-' {
        return parse_int_with_sign(t, &buf[1..], base, true);
    }
    // return parseIntWithSign(Result, Character, buf, base, .pos);
    parse_int_with_sign(t, buf, base, false)
}

// fn parseIntWithSign(
//     comptime Result: type,
//     comptime Character: type,
//     buf: []const Character,
//     base: u8,
//     comptime sign: enum { pos, neg },
// ) ParseIntError!Result {
fn parse_int_with_sign(
    result: IntType,
    buf: &[u8],
    base: u8,
    neg: bool,
) -> Result<i128, ParseIntError> {
    // if (buf.len == 0) return error.InvalidCharacter;
    if buf.is_empty() {
        return Err(ParseIntError::InvalidCharacter);
    }

    let mut buf_base = base;
    let mut buf_start = buf;
    if base == 0 {
        // Treat is as a decimal number by default.
        buf_base = 10;
        // Detect the base by looking at buf prefix.
        if buf.len() > 2 && buf[0] == b'0' {
            match buf[1].to_ascii_lowercase() {
                b'b' => {
                    buf_base = 2;
                    buf_start = &buf[2..];
                }
                b'o' => {
                    buf_base = 8;
                    buf_start = &buf[2..];
                }
                b'x' => {
                    buf_base = 16;
                    buf_start = &buf[2..];
                }
                _ => {}
            }
        }
    }

    // accumulate into Accumulate which is always 8 bits or larger.  this prevents
    // `buf_base` from overflowing Result.
    // const Accumulate = std.meta.Int(info.int.signedness, @max(8, info.int.bits));
    let accumulate_t = IntType {
        bits: result.bits.max(8),
        signed: result.signed,
    };
    let mut accumulate: i128 = 0;

    // if (buf_start[0] == '_' or buf_start[buf_start.len - 1] == '_') return error.InvalidCharacter;
    if buf_start[0] == b'_' || buf_start[buf_start.len() - 1] == b'_' {
        return Err(ParseIntError::InvalidCharacter);
    }

    for &c in buf_start {
        // if (c == '_') continue;
        if c == b'_' {
            continue;
        }
        // const digit = try charToDigit(math.cast(u8, c) orelse return error.InvalidCharacter, buf_base);
        let digit = i128::from(char_to_digit(c, buf_base)?);
        if accumulate != 0 {
            // accumulate = try math.mul(Accumulate, accumulate, math.cast(Accumulate, buf_base) orelse return error.Overflow);
            let b = accumulate_t
                .cast(i128::from(buf_base))
                .ok_or(ParseIntError::Overflow)?;
            accumulate = accumulate_t
                .cast(accumulate * b)
                .ok_or(ParseIntError::Overflow)?;
        } else if neg {
            // The first digit of a negative number.
            // Consider parsing "-4" as an i3.
            // This should work, but positive 4 overflows i3, so we can't cast the digit to T and subtract.
            // accumulate = math.cast(Accumulate, -@as(i8, @intCast(digit))) orelse return error.Overflow;
            accumulate = accumulate_t.cast(-digit).ok_or(ParseIntError::Overflow)?;
            continue;
        }
        // const add = switch (sign) { .pos => math.add, .neg => math.sub };
        // accumulate = try add(Accumulate, accumulate, math.cast(Accumulate, digit) orelse return error.Overflow);
        let d = accumulate_t.cast(digit).ok_or(ParseIntError::Overflow)?;
        let next = if neg { accumulate - d } else { accumulate + d };
        accumulate = accumulate_t.cast(next).ok_or(ParseIntError::Overflow)?;
    }

    // return if (Result == Accumulate)
    //     accumulate
    // else
    //     math.cast(Result, accumulate) orelse return error.Overflow;
    result.cast(accumulate).ok_or(ParseIntError::Overflow)
}

/// Refuses Zig's `_` digit separator in a wire-protocol integer, before
/// `parse`/`parse_int`/`parse_unsigned` runs.
///
/// PORT NOTE: reference defect (R13, extended 2026-09-23), UD141. Zig's
/// `std.fmt.parseInt`/`parseUnsigned` accept interior `_` separators (a Zig
/// source-literal feature), so the reference's glyph APC (`cp`, `upm`, `aw`,
/// `lh`), kitty graphics key values, OSC 4/104 color indices, OSC 66 text
/// sizing values, the OSC 133 exit code and the OSC 9;1 / 9;4 numbers all read
/// `1_0` as 10. None of those protocols allows it (their specs define plain
/// decimal or hex integers), and Ghostty states its own position in a sibling
/// parser: "Zig's integer parser allows '_', we don't"
/// (`osc/parsers/kitty_desktop_notification.zig:210-211`, which checks
/// `std.mem.indexOfScalar(u8, value, '_')` and takes the key's failure path),
/// and `osc/parsers/context_signal.zig:165-167` accepts only digits. So each
/// of those sites calls this first and takes its existing failure path, as the
/// OSC 99 parser does. Only `_` is refused: the OSC 99 filter leaves the sign
/// handling (`+10`, `-0`) to `parseInt`, so the port keeps it too. The tmux
/// control-mode parsers (N100) stay faithful: their input is tmux's own
/// generated output, not a protocol programs write against.
pub(crate) fn reject_digit_separators(buf: &[u8]) -> Result<(), ParseIntError> {
    // if (std.mem.indexOfScalar(u8, value, '_')) |_| break :value key.default();
    if buf.contains(&b'_') {
        return Err(ParseIntError::InvalidCharacter);
    }
    Ok(())
}

// pub fn charToDigit(c: u8, base: u8) (error{InvalidCharacter}!u8) {
fn char_to_digit(c: u8, base: u8) -> Result<u8, ParseIntError> {
    let value = match c {
        b'0'..=b'9' => c - b'0',
        b'A'..=b'Z' => c - b'A' + 10,
        b'a'..=b'z' => c - b'a' + 10,
        _ => return Err(ParseIntError::InvalidCharacter),
    };

    if value >= base {
        return Err(ParseIntError::InvalidCharacter);
    }

    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ParseIntError::{InvalidCharacter, Overflow};

    // PORT NOTE: zig 0.16.0 `std/fmt.zig`'s `test parseInt` (:334-392, all 45
    // assertions, including its duplicated `1_1_1` line) and `test
    // parseUnsigned` (:477-511, all 22), ported verbatim and in order, so the
    // port is checked against the reference's own tables rather than against
    // Rust's parser. Zig's odd-width result types (`i1`, `i2`, `i4`, `i5`,
    // `i43`, `u1`, `u2`, `u7`) are the runtime `IntType` of the same width
    // and signedness; the native widths go through `parse`/`parse_unsigned`.

    /// `parseUnsigned` for an odd-width Zig type (`u1`, `u2`, `u7`).
    fn parse_unsigned_t(t: IntType, buf: &[u8], base: u8) -> Result<i128, ParseIntError> {
        parse_int_with_sign(t, buf, base, false)
    }

    // test parseInt
    #[test]
    fn std_fmt_parse_int_table() {
        assert_eq!(parse::<i32>(b"-10", 10), Ok(-10));
        assert_eq!(parse::<i32>(b"+10", 10), Ok(10));
        assert_eq!(parse::<u32>(b"+10", 10), Ok(10));
        assert_eq!(parse::<u32>(b"-10", 10), Err(Overflow));
        assert_eq!(parse::<u32>(b" 10", 10), Err(InvalidCharacter));
        assert_eq!(parse::<u32>(b"10 ", 10), Err(InvalidCharacter));
        assert_eq!(parse::<u32>(b"_10_", 10), Err(InvalidCharacter));
        assert_eq!(parse::<u32>(b"0x_10_", 10), Err(InvalidCharacter));
        assert_eq!(parse::<u32>(b"0x10_", 10), Err(InvalidCharacter));
        assert_eq!(parse::<u32>(b"0x_10", 10), Err(InvalidCharacter));
        assert_eq!(parse::<u8>(b"255", 10), Ok(255));
        assert_eq!(parse::<u8>(b"256", 10), Err(Overflow));

        // +0 and -0 should work for unsigned
        assert_eq!(parse::<u8>(b"-0", 10), Ok(0));
        assert_eq!(parse::<u8>(b"+0", 10), Ok(0));

        // ensure minInt is parsed correctly
        let i1 = IntType::signed(1);
        assert_eq!(parse_int(i1, b"-1", 10), Ok(i1.min()));
        assert_eq!(parse::<i8>(b"-128", 10), Ok(i8::MIN));
        let i43 = IntType::signed(43);
        assert_eq!(parse_int(i43, b"-4398046511104", 10), Ok(i43.min()));

        // empty string or bare +- is invalid
        assert_eq!(parse::<u32>(b"", 10), Err(InvalidCharacter));
        assert_eq!(parse::<i32>(b"", 10), Err(InvalidCharacter));
        assert_eq!(parse::<u32>(b"+", 10), Err(InvalidCharacter));
        assert_eq!(parse::<i32>(b"+", 10), Err(InvalidCharacter));
        assert_eq!(parse::<u32>(b"-", 10), Err(InvalidCharacter));
        assert_eq!(parse::<i32>(b"-", 10), Err(InvalidCharacter));

        // autodectect the base
        assert_eq!(parse::<i32>(b"111", 0), Ok(111));
        assert_eq!(parse::<i32>(b"1_1_1", 0), Ok(111));
        assert_eq!(parse::<i32>(b"1_1_1", 0), Ok(111));
        assert_eq!(parse::<i32>(b"+0b111", 0), Ok(7));
        assert_eq!(parse::<i32>(b"+0B111", 0), Ok(7));
        assert_eq!(parse::<i32>(b"+0b1_11", 0), Ok(7));
        assert_eq!(parse::<i32>(b"+0o111", 0), Ok(73));
        assert_eq!(parse::<i32>(b"+0O111", 0), Ok(73));
        assert_eq!(parse::<i32>(b"+0o11_1", 0), Ok(73));
        assert_eq!(parse::<i32>(b"+0x111", 0), Ok(273));
        assert_eq!(parse::<i32>(b"-0b111", 0), Ok(-7));
        assert_eq!(parse::<i32>(b"-0b11_1", 0), Ok(-7));
        assert_eq!(parse::<i32>(b"-0o111", 0), Ok(-73));
        assert_eq!(parse::<i32>(b"-0x111", 0), Ok(-273));
        assert_eq!(parse::<i32>(b"-0X111", 0), Ok(-273));
        assert_eq!(parse::<i32>(b"-0x1_11", 0), Ok(-273));

        // bare binary/octal/decimal prefix is invalid
        assert_eq!(parse::<u32>(b"0b", 0), Err(InvalidCharacter));
        assert_eq!(parse::<u32>(b"0o", 0), Err(InvalidCharacter));
        assert_eq!(parse::<u32>(b"0x", 0), Err(InvalidCharacter));

        // edge cases which previously errored due to base overflowing T
        assert_eq!(parse_int(IntType::signed(2), b"-10", 2), Ok(-2));
        assert_eq!(parse_int(IntType::signed(4), b"-10", 8), Ok(-8));
        assert_eq!(parse_int(IntType::signed(5), b"-10", 16), Ok(-16));
    }

    // test parseUnsigned
    #[test]
    fn std_fmt_parse_unsigned_table() {
        assert_eq!(parse_unsigned::<u16>(b"050124", 10), Ok(50124));
        assert_eq!(parse_unsigned::<u16>(b"65535", 10), Ok(65535));
        assert_eq!(parse_unsigned::<u16>(b"65_535", 10), Ok(65535));
        assert_eq!(parse_unsigned::<u16>(b"65536", 10), Err(Overflow));

        assert_eq!(
            parse_unsigned::<u64>(b"0ffffffffffffffff", 16),
            Ok(0xffffffffffffffff)
        );
        assert_eq!(
            parse_unsigned::<u64>(b"0f_fff_fff_fff_fff_fff", 16),
            Ok(0xffffffffffffffff)
        );
        assert_eq!(
            parse_unsigned::<u64>(b"10000000000000000", 16),
            Err(Overflow)
        );

        assert_eq!(parse_unsigned::<u32>(b"DeadBeef", 16), Ok(0xDEADBEEF));

        let u7 = IntType::unsigned(7);
        assert_eq!(parse_unsigned_t(u7, b"1", 10), Ok(1));
        assert_eq!(parse_unsigned_t(u7, b"1000", 2), Ok(8));

        assert_eq!(parse_unsigned::<u32>(b"f", 10), Err(InvalidCharacter));
        assert_eq!(parse_unsigned::<u8>(b"109", 8), Err(InvalidCharacter));

        assert_eq!(parse_unsigned::<u32>(b"NUMBER", 36), Ok(1442151747));

        // these numbers should fit even though the base itself doesn't fit in the destination type
        let u1 = IntType::unsigned(1);
        assert_eq!(parse_unsigned_t(u1, b"0", 10), Ok(0));
        assert_eq!(parse_unsigned_t(u1, b"1", 10), Ok(1));
        assert_eq!(parse_unsigned_t(u1, b"2", 10), Err(Overflow));
        assert_eq!(parse_unsigned_t(u1, b"001", 16), Ok(1));
        let u2 = IntType::unsigned(2);
        assert_eq!(parse_unsigned_t(u2, b"3", 16), Ok(3));
        assert_eq!(parse_unsigned_t(u2, b"4", 16), Err(Overflow));

        // parseUnsigned does not expect a sign
        assert_eq!(parse_unsigned::<u8>(b"+0", 10), Err(InvalidCharacter));
        assert_eq!(parse_unsigned::<u8>(b"-0", 10), Err(InvalidCharacter));

        // test empty string error
        assert_eq!(parse_unsigned::<u8>(b"", 10), Err(InvalidCharacter));
    }

    // The digit separators and the Accumulate width, which are what the
    // terminal parsers depend on.
    #[test]
    fn digit_separators_and_accumulate_width() {
        assert_eq!(parse::<u32>(b"1_0", 10), Ok(10));
        assert_eq!(parse::<i32>(b"-1_0", 10), Ok(-10));
        assert_eq!(parse::<u32>(b"1__0", 10), Ok(10));
        assert_eq!(parse::<u32>(b"10_", 10), Err(InvalidCharacter));
        assert_eq!(parse::<u32>(b"_10", 10), Err(InvalidCharacter));
        assert_eq!(parse::<u32>(b"_", 10), Err(InvalidCharacter));
        assert_eq!(parse::<i32>(b"2147483647", 10), Ok(i32::MAX));
        assert_eq!(parse::<i32>(b"-2147483648", 10), Ok(i32::MIN));
        assert_eq!(parse::<i32>(b"2147483648", 10), Err(Overflow));
        assert_eq!(parse::<u32>(b"-1a", 10), Err(Overflow));

        // u4 accumulates in u8: "99x" reaches the invalid character before
        // the final cast, and "20" fits u8 but not u4.
        let u4 = IntType::unsigned(4);
        assert_eq!(parse_int(u4, b"15", 10), Ok(15));
        assert_eq!(parse_int(u4, b"16", 10), Err(Overflow));
        assert_eq!(parse_int(u4, b"99x", 10), Err(InvalidCharacter));
        assert_eq!(parse_int(u4, b"1_2", 10), Ok(12));

        // parseUnsigned takes no sign byte.
        assert_eq!(parse_unsigned::<u16>(b"1_0", 10), Ok(10));
        assert_eq!(parse_unsigned::<u16>(b"+1", 10), Err(InvalidCharacter));
        assert_eq!(parse_unsigned::<u16>(b"-0", 10), Err(InvalidCharacter));
        assert_eq!(parse::<usize>(b"-0", 10), Ok(0));

        let u21 = IntType::unsigned(21);
        assert_eq!(parse_int(u21, b"1F_FFFF", 16), Ok(0x1F_FFFF));
        assert_eq!(parse_int(u21, b"20_0000", 16), Err(Overflow));
    }
}
