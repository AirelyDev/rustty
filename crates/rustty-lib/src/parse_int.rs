//! Integer parsing for terminal protocols, without Zig digit separators.
//!
//! Adapted from Zig's `std.fmt.parseInt` (MIT license). See
//! src/lib/compat/README.md for license and details.
//!
//! PORTS: ghostty src/lib/parse_int.zig (`73768913b`, `d92e3400e`,
//! `500040d3b`, `31bbb1273`), exported from `lib/main.zig` as `parseInt` (the
//! crate root re-exports [`parse_int`]).
//!
//! PORT NOTE: `std.fmt.ParseIntError` and `std.fmt.charToDigit` are the port of
//! Zig's std in `rustty_zigstd::fmt`, as the reference takes them from `std`.
//! Zig's result type `T` is a comptime integer type of any width (the callers
//! use `u4`, `u9`, `i32`, `u64`, ...). [`parse_int`] takes a native Rust
//! integer type; [`parse_int_type`] takes any width through
//! `rustty_zigstd::fmt::IntType` and returns the value widened to `i128`. The
//! comptime `base` is a const generic, with the reference's comptime assert as
//! a compile-time assertion.
//!
//! PORT NOTE: the accumulator is width-faithful. Each step is computed in
//! `i128` (which holds any accumulator value of at most 64 bits times a base of
//! at most 36) and range-checked against `Accumulate` =
//! `std.meta.Int(signedness, @max(8, bits))` right away, as `math.mul` and
//! `math.add`/`math.sub` do at that width. So `Overflow` is reported at the
//! same digit as in the reference, before a later invalid character is read
//! (`parseInt(u8, "999x", 10)` is `error.Overflow`), and the final
//! `math.cast(T, accumulate)` decides the narrow types: `i3` `"-4"` fits,
//! `"4"` overflows.

use rustty_zigstd::fmt::{char_to_digit, IntType, ParseIntError, ZigInt};

/// Parse ASCII digits in the given base. Signed types allow a leading sign;
/// unsigned types accept digits only.
///
/// This exists because `std.fmt.parseInt` implements Zig integer literal
/// syntax rather than what terminal protocols specify: it ignores `_` digit
/// separators (so `4_2` parses as 42) and accepts a leading `+` or `-` even
/// for unsigned types. Accepting that input makes us diverge from other
/// terminals, so don't replace this with `std.fmt.parseInt`.
///
/// PORT NOTE: `parseInt(T, value, base)` for a native Rust integer type `T`;
/// see [`parse_int_type`] for Zig's odd widths.
#[inline]
pub fn parse_int<T: ZigInt, const BASE: u8>(value: &[u8]) -> Result<T, ParseIntError> {
    parse_int_type::<BASE>(T::TYPE, value).map(T::from_i128)
}

/// `parseInt(T, value, base)` for the Zig integer type `t` (any width from 1
/// to 64 bits), the value widened to `i128`. See [`parse_int`].
pub fn parse_int_type<const BASE: u8>(t: IntType, value: &[u8]) -> Result<i128, ParseIntError> {
    // comptime std.debug.assert(base >= 2 and base <= 36);
    const { assert!(BASE >= 2 && BASE <= 36) };
    // if (value.len == 0) return error.InvalidCharacter;
    if value.is_empty() {
        return Err(ParseIntError::InvalidCharacter);
    }
    // if (@typeInfo(T).int.signedness == .signed) {
    if t.signed {
        // if (value[0] == '+') return parseWithSign(T, value[1..], base, .pos);
        if value[0] == b'+' {
            return parse_with_sign::<BASE>(t, &value[1..], Sign::Pos);
        }
        // if (value[0] == '-') return parseWithSign(T, value[1..], base, .neg);
        if value[0] == b'-' {
            return parse_with_sign::<BASE>(t, &value[1..], Sign::Neg);
        }
    }
    // return parseWithSign(T, value, base, .pos);
    parse_with_sign::<BASE>(t, value, Sign::Pos)
}

// comptime sign: enum { pos, neg },
#[derive(Clone, Copy, PartialEq, Eq)]
enum Sign {
    Pos,
    Neg,
}

fn parse_with_sign<const BASE: u8>(
    t: IntType,
    value: &[u8],
    sign: Sign,
) -> Result<i128, ParseIntError> {
    // if (value.len == 0) return error.InvalidCharacter;
    if value.is_empty() {
        return Err(ParseIntError::InvalidCharacter);
    }

    // const add = switch (sign) {
    //     .pos => math.add,
    //     .neg => math.sub,
    // };

    // const info = @typeInfo(T).int;
    // const Accumulate = std.meta.Int(info.signedness, @max(8, info.bits));
    let accumulate_t = IntType {
        bits: t.bits.max(8),
        signed: t.signed,
    };
    let (min, max) = (accumulate_t.min(), accumulate_t.max());
    // math.mul / math.add / math.sub at `Accumulate`: error.Overflow when the
    // result does not fit.
    let fit = |v: i128| {
        if min <= v && v <= max {
            Ok(v)
        } else {
            Err(ParseIntError::Overflow)
        }
    };
    // var accumulate: Accumulate = 0;
    let mut accumulate: i128 = 0;
    for &c in value {
        // const digit = try std.fmt.charToDigit(c, base);
        let digit = i128::from(char_to_digit(c, BASE)?);
        // accumulate = try math.mul(Accumulate, accumulate, base);
        accumulate = fit(accumulate * i128::from(BASE))?;
        // accumulate = try add(Accumulate, accumulate, @intCast(digit));
        accumulate = fit(match sign {
            Sign::Pos => accumulate + digit,
            Sign::Neg => accumulate - digit,
        })?;
    }

    // return math.cast(T, accumulate) orelse return error.Overflow;
    if t.min() <= accumulate && accumulate <= t.max() {
        Ok(accumulate)
    } else {
        Err(ParseIntError::Overflow)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn i3() -> IntType {
        IntType::signed(3)
    }

    #[test]
    fn protocol_integer_parsing() {
        assert_eq!(Ok(42), parse_int::<u8, 10>(b"042"));
        assert_eq!(Ok(255), parse_int::<u8, 16>(b"fF"));
        assert_eq!(Ok(42), parse_int::<i32, 10>(b"+42"));
        assert_eq!(Ok(-2147483648), parse_int::<i32, 10>(b"-2147483648"));
        assert_eq!(Ok(2147483647), parse_int::<i32, 10>(b"2147483647"));
        assert_eq!(Ok(-4), parse_int_type::<10>(i3(), b"-4"));
        assert_eq!(
            Err(ParseIntError::Overflow),
            parse_int_type::<10>(i3(), b"4")
        );
        assert_eq!(Err(ParseIntError::Overflow), parse_int::<u8, 10>(b"256"));
        assert_eq!(
            Err(ParseIntError::Overflow),
            parse_int::<i32, 10>(b"2147483648")
        );
        assert_eq!(
            Err(ParseIntError::Overflow),
            parse_int::<i32, 10>(b"-2147483649")
        );
        for value in [
            &b""[..],
            b"4_2",
            b"4__2",
            b"_42",
            b"42_",
            b" 42",
            b"42 ",
            b"0x2a",
            b"4.2",
            b"4e2",
            b"\xff",
        ] {
            assert_eq!(
                Err(ParseIntError::InvalidCharacter),
                parse_int::<u8, 10>(value),
                "{value:?}"
            );
            assert_eq!(
                Err(ParseIntError::InvalidCharacter),
                parse_int::<i32, 10>(value),
                "{value:?}"
            );
        }
        for value in [&b"+42"[..], b"-0", b"-42"] {
            assert_eq!(
                Err(ParseIntError::InvalidCharacter),
                parse_int::<u8, 10>(value),
                "{value:?}"
            );
        }
        for value in [&b"+"[..], b"-", b"-4_2", b"+4__2"] {
            assert_eq!(
                Err(ParseIntError::InvalidCharacter),
                parse_int::<i32, 10>(value),
                "{value:?}"
            );
        }
        assert_eq!(
            Err(ParseIntError::InvalidCharacter),
            parse_int::<u16, 16>(b"f_f")
        );
        assert_eq!(
            Err(ParseIntError::InvalidCharacter),
            parse_int::<u16, 16>(b"0xff")
        );
    }

    // PARITY (rustty-added): the accumulator is `@max(8, bits)` wide and every
    // step is range-checked at that width, so an overflow is reported at the
    // digit that causes it, before a later invalid character. A port that
    // accumulated in a wide type and range-checked only at the end would
    // report `InvalidCharacter` for these.
    #[test]
    fn accumulator_is_width_faithful() {
        // u8: 99 * 10 = 990 overflows `Accumulate = u8` before the `x`.
        assert_eq!(Err(ParseIntError::Overflow), parse_int::<u8, 10>(b"999x"));
        // i3 accumulates in i8: "-200" overflows at the third digit.
        assert_eq!(
            Err(ParseIntError::Overflow),
            parse_int_type::<10>(i3(), b"-200x")
        );
        // Within `Accumulate` but outside `T`: the final `math.cast`.
        assert_eq!(
            Err(ParseIntError::Overflow),
            parse_int_type::<10>(i3(), b"-5")
        );
        assert_eq!(Ok(3), parse_int_type::<10>(i3(), b"+3"));
        // u9 (the OSC 4 color index) accumulates in u9 itself.
        let u9 = IntType::unsigned(9);
        assert_eq!(Ok(511), parse_int_type::<10>(u9, b"511"));
        assert_eq!(
            Err(ParseIntError::Overflow),
            parse_int_type::<10>(u9, b"512")
        );
        // Widest types.
        assert_eq!(Ok(u64::MAX), parse_int::<u64, 10>(b"18446744073709551615"));
        assert_eq!(
            Err(ParseIntError::Overflow),
            parse_int::<u64, 10>(b"18446744073709551616")
        );
        assert_eq!(Ok(i64::MIN), parse_int::<i64, 10>(b"-9223372036854775808"));
        assert_eq!(
            Err(ParseIntError::Overflow),
            parse_int::<i64, 10>(b"9223372036854775808")
        );
        // Base 36 at the narrowest accumulator: 'z' = 35.
        assert_eq!(Ok(35), parse_int::<u8, 36>(b"Z"));
        assert_eq!(Err(ParseIntError::Overflow), parse_int::<i8, 36>(b"zz"));
    }
}
