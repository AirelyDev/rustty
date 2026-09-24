//! Tests for `fmt`: Zig 0.16.0's own std tests, ported assertion for
//! assertion for the types this port has, then a differential check against
//! Zig's results over a wider corpus (`zig_corpus.rs`).
//!
//! Skipped from the reference tests, each because Rust has no such type:
//! - `test parseInt`: the `i1`, `i2`, `i4`, `i5` and `i43` rows ("ensure minInt
//!   is parsed correctly", "edge cases which previously errored due to base
//!   overflowing T");
//! - `test parseUnsigned`: the `u7`, `u1` and `u2` rows;
//! - `test parseFloat` / `"nan and inf"`: the `f16`, `f80` and `f128`
//!   instantiations (the `f32` and `f64` ones are ported);
//! - `"largest normals"`: the `f16`, `f80` and `f128` rows;
//! - `"#11169"`, `"hex.f16"`, `"hex.f80"`, `"hex.f128"`: `f16`/`f80`/`f128` only;
//! - `"many_digits hex"` compares the `f32` parse with an `f128` parse cast to
//!   `f32`; the `f128` side is replaced by its value, `2^64` (the `f32`
//!   nearest to `0xffffffffffffffff`).

use super::zig_corpus::{FLOATS, INTS};
use super::*;

const EPSILON: f64 = 1e-7;

/// `std.math.approxEqAbs`.
fn approx_eq_abs(a: f64, b: f64, tolerance: f64) -> bool {
    if a == b {
        return true;
    }
    (a - b).abs() <= tolerance
}

// test parseInt {
#[test]
fn test_parse_int() {
    assert_eq!(parse_int::<i32>(b"-10", 10), Ok(-10));
    assert_eq!(parse_int::<i32>(b"+10", 10), Ok(10));
    assert_eq!(parse_int::<u32>(b"+10", 10), Ok(10));
    assert_eq!(parse_int::<u32>(b"-10", 10), Err(ParseIntError::Overflow));
    assert_eq!(
        parse_int::<u32>(b" 10", 10),
        Err(ParseIntError::InvalidCharacter)
    );
    assert_eq!(
        parse_int::<u32>(b"10 ", 10),
        Err(ParseIntError::InvalidCharacter)
    );
    assert_eq!(
        parse_int::<u32>(b"_10_", 10),
        Err(ParseIntError::InvalidCharacter)
    );
    assert_eq!(
        parse_int::<u32>(b"0x_10_", 10),
        Err(ParseIntError::InvalidCharacter)
    );
    assert_eq!(
        parse_int::<u32>(b"0x10_", 10),
        Err(ParseIntError::InvalidCharacter)
    );
    assert_eq!(
        parse_int::<u32>(b"0x_10", 10),
        Err(ParseIntError::InvalidCharacter)
    );
    assert_eq!(parse_int::<u8>(b"255", 10), Ok(255));
    assert_eq!(parse_int::<u8>(b"256", 10), Err(ParseIntError::Overflow));

    // +0 and -0 should work for unsigned
    assert_eq!(parse_int::<u8>(b"-0", 10), Ok(0));
    assert_eq!(parse_int::<u8>(b"+0", 10), Ok(0));

    // ensure minInt is parsed correctly
    // (the i1 and i43 rows are skipped: no such Rust type)
    assert_eq!(parse_int::<i8>(b"-128", 10), Ok(i8::MIN));

    // empty string or bare +- is invalid
    assert_eq!(
        parse_int::<u32>(b"", 10),
        Err(ParseIntError::InvalidCharacter)
    );
    assert_eq!(
        parse_int::<i32>(b"", 10),
        Err(ParseIntError::InvalidCharacter)
    );
    assert_eq!(
        parse_int::<u32>(b"+", 10),
        Err(ParseIntError::InvalidCharacter)
    );
    assert_eq!(
        parse_int::<i32>(b"+", 10),
        Err(ParseIntError::InvalidCharacter)
    );
    assert_eq!(
        parse_int::<u32>(b"-", 10),
        Err(ParseIntError::InvalidCharacter)
    );
    assert_eq!(
        parse_int::<i32>(b"-", 10),
        Err(ParseIntError::InvalidCharacter)
    );

    // autodectect the base
    assert_eq!(parse_int::<i32>(b"111", 0), Ok(111));
    assert_eq!(parse_int::<i32>(b"1_1_1", 0), Ok(111));
    assert_eq!(parse_int::<i32>(b"1_1_1", 0), Ok(111));
    assert_eq!(parse_int::<i32>(b"+0b111", 0), Ok(7));
    assert_eq!(parse_int::<i32>(b"+0B111", 0), Ok(7));
    assert_eq!(parse_int::<i32>(b"+0b1_11", 0), Ok(7));
    assert_eq!(parse_int::<i32>(b"+0o111", 0), Ok(73));
    assert_eq!(parse_int::<i32>(b"+0O111", 0), Ok(73));
    assert_eq!(parse_int::<i32>(b"+0o11_1", 0), Ok(73));
    assert_eq!(parse_int::<i32>(b"+0x111", 0), Ok(273));
    assert_eq!(parse_int::<i32>(b"-0b111", 0), Ok(-7));
    assert_eq!(parse_int::<i32>(b"-0b11_1", 0), Ok(-7));
    assert_eq!(parse_int::<i32>(b"-0o111", 0), Ok(-73));
    assert_eq!(parse_int::<i32>(b"-0x111", 0), Ok(-273));
    assert_eq!(parse_int::<i32>(b"-0X111", 0), Ok(-273));
    assert_eq!(parse_int::<i32>(b"-0x1_11", 0), Ok(-273));

    // bare binary/octal/decimal prefix is invalid
    assert_eq!(
        parse_int::<u32>(b"0b", 0),
        Err(ParseIntError::InvalidCharacter)
    );
    assert_eq!(
        parse_int::<u32>(b"0o", 0),
        Err(ParseIntError::InvalidCharacter)
    );
    assert_eq!(
        parse_int::<u32>(b"0x", 0),
        Err(ParseIntError::InvalidCharacter)
    );

    // edge cases which previously errored due to base overflowing T
    // (the i2, i4 and i5 rows are skipped: no such Rust type)
}

// test parseUnsigned {
#[test]
fn test_parse_unsigned() {
    assert_eq!(parse_unsigned::<u16>(b"050124", 10), Ok(50124));
    assert_eq!(parse_unsigned::<u16>(b"65535", 10), Ok(65535));
    assert_eq!(parse_unsigned::<u16>(b"65_535", 10), Ok(65535));
    assert_eq!(
        parse_unsigned::<u16>(b"65536", 10),
        Err(ParseIntError::Overflow)
    );

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
        Err(ParseIntError::Overflow)
    );

    assert_eq!(parse_unsigned::<u32>(b"DeadBeef", 16), Ok(0xDEADBEEF));

    // (the u7 rows are skipped: no such Rust type)

    assert_eq!(
        parse_unsigned::<u32>(b"f", 10),
        Err(ParseIntError::InvalidCharacter)
    );
    assert_eq!(
        parse_unsigned::<u8>(b"109", 8),
        Err(ParseIntError::InvalidCharacter)
    );

    assert_eq!(parse_unsigned::<u32>(b"NUMBER", 36), Ok(1442151747));

    // these numbers should fit even though the base itself doesn't fit in the destination type
    // (the u1 and u2 rows are skipped: no such Rust type)

    // parseUnsigned does not expect a sign
    assert_eq!(
        parse_unsigned::<u8>(b"+0", 10),
        Err(ParseIntError::InvalidCharacter)
    );
    assert_eq!(
        parse_unsigned::<u8>(b"-0", 10),
        Err(ParseIntError::InvalidCharacter)
    );

    // test empty string error
    assert_eq!(
        parse_unsigned::<u8>(b"", 10),
        Err(ParseIntError::InvalidCharacter)
    );
}

/// `test parseFloat`'s body for one float type (`inline for` over the types).
macro_rules! parse_float_body {
    ($t:ty) => {{
        type T = $t;
        let p = |s: &[u8]| parse_float::<T>(s);
        let inv = Err(ParseFloatError::InvalidCharacter);
        assert_eq!(p(b""), inv);
        assert_eq!(p(b"   1"), inv);
        assert_eq!(p(b"1abc"), inv);
        assert_eq!(p(b"+"), inv);
        assert_eq!(p(b"-"), inv);

        assert_eq!(p(b"0").unwrap(), 0.0);
        assert_eq!(p(b"0").unwrap(), 0.0);
        assert_eq!(p(b"+0").unwrap(), 0.0);
        assert_eq!(p(b"-0").unwrap(), 0.0);

        assert_eq!(p(b"0e0").unwrap(), 0.0);
        assert_eq!(p(b"2e3").unwrap(), 2000.0);
        assert_eq!(p(b"1e0").unwrap(), 1.0);
        assert_eq!(p(b"-2e3").unwrap(), -2000.0);
        assert_eq!(p(b"-1e0").unwrap(), -1.0);
        assert_eq!(p(b"1.234e3").unwrap(), 1234.0);

        let approx = |s: &[u8], v: T| approx_eq_abs(p(s).unwrap() as f64, v as f64, EPSILON);
        assert!(approx(b"3.141", 3.141));
        assert!(approx(b"-3.141", -3.141));

        assert_eq!(p(b"1e-5000").unwrap(), 0.0);
        assert_eq!(p(b"1e+5000").unwrap(), T::INFINITY);

        assert_eq!(
            p(b"0.4e0066999999999999999999999999999999999999999999999999999").unwrap(),
            T::INFINITY
        );
        assert!(approx(
            b"0_1_2_3_4_5_6.7_8_9_0_0_0e0_0_1_0",
            123456.789000e10 as T
        ));

        // underscore rule is simple and reduces to "can only occur between two digits" and multiple are not supported.
        assert_eq!(p(b"0123456.789000e_0010"), inv); // cannot occur immediately after exponent
        assert_eq!(p(b"_0123456.789000e0010"), inv); // cannot occur before any digits
        assert_eq!(p(b"0__123456.789000e_0010"), inv); // cannot occur twice in a row
        assert_eq!(p(b"0123456_.789000e0010"), inv); // cannot occur before decimal point
        assert_eq!(p(b"0123456.789000e0010_"), inv); // cannot occur at end of number

        assert!(approx(b"1e-2", 0.01));
        assert!(approx(b"1234e-2", 12.34));

        assert!(approx(b"1.", 1.0));
        assert!(approx(b"0.", 0.0));
        assert!(approx(b".1", 0.1));
        assert!(approx(b".0", 0.0));
        assert!(approx(b".1e-1", 0.01));

        assert_eq!(p(b"."), inv); // At least one digit is required.
        assert_eq!(p(b".e1"), inv); // At least one digit is required.
        assert_eq!(p(b"0.e"), inv); // At least one digit is required.

        assert!(approx(b"123142.1", 123142.1));
        assert!(approx(b"-123142.1124", -123142.1124 as T));
        assert!(approx(b"0.7062146892655368", 0.7062146892655368 as T));
        assert!(approx(b"2.71828182845904523536", 2.718281828459045 as T));
    }};
}

// test parseFloat { inline for ([_]type{ f16, f32, f64, f80, f128 }) |T| { ... } }
// (the f16, f80 and f128 instantiations are skipped: no such Rust type)
#[test]
fn test_parse_float() {
    parse_float_body!(f32);
    parse_float_body!(f64);
}

// test "nan and inf" { (f32 and f64 of the five types) }
#[test]
fn nan_and_inf() {
    assert_eq!(
        parse_float::<f32>(b"nAn").unwrap().to_bits(),
        f32::NAN.to_bits()
    );
    assert_eq!(parse_float::<f32>(b"inF").unwrap(), f32::INFINITY);
    assert_eq!(parse_float::<f32>(b"-INF").unwrap(), -f32::INFINITY);
    assert_eq!(
        parse_float::<f64>(b"nAn").unwrap().to_bits(),
        f64::NAN.to_bits()
    );
    assert_eq!(parse_float::<f64>(b"inF").unwrap(), f64::INFINITY);
    assert_eq!(parse_float::<f64>(b"-INF").unwrap(), -f64::INFINITY);
}

// test "largest normals" (the f32 and f64 rows)
#[test]
fn largest_normals() {
    assert_eq!(
        parse_float::<f32>(b"3.4028234664E38").unwrap().to_bits(),
        0x7f7f_ffff
    );
    assert_eq!(
        parse_float::<f64>(b"1.7976931348623157E308")
            .unwrap()
            .to_bits(),
        0x7fef_ffff_ffff_ffff
    );
}

// test "many_digits hex"
#[test]
fn many_digits_hex() {
    let a: f32 = parse_float::<f32>(b"0xffffffffffffffff.0p0").unwrap();
    // `b` is `@floatCast(try parseFloat(f128, ...))`: 2^64 - 1 rounded to f32.
    let b: f32 = 18446744073709551616.0;
    assert_eq!(a, b);
}

// test "hex.special"
#[test]
fn hex_special() {
    assert!(parse_float::<f32>(b"nAn").unwrap().is_nan());
    assert!(parse_float::<f32>(b"iNf").unwrap() == f32::INFINITY);
    assert!(parse_float::<f32>(b"+Inf").unwrap() == f32::INFINITY);
    assert!(parse_float::<f32>(b"-iNf").unwrap() == f32::NEG_INFINITY);

    assert!(parse_float::<f32>(b"0x9999p9999").unwrap() == f32::INFINITY);
    assert!(parse_float::<f32>(b"-0x9999p9999").unwrap() == f32::NEG_INFINITY);
}

// test "hex.zero"
#[test]
fn hex_zero() {
    assert_eq!(0.0f32, parse_float::<f32>(b"0x0").unwrap());
    assert_eq!(0.0f32, parse_float::<f32>(b"-0x0").unwrap());
    assert_eq!(0.0f32, parse_float::<f32>(b"0x0p42").unwrap());
    assert_eq!(0.0f32, parse_float::<f32>(b"-0x0.00000p42").unwrap());
    assert_eq!(0.0f32, parse_float::<f32>(b"0x0.00000p666").unwrap());
}

// test "hex.f32"
#[test]
fn hex_f32() {
    assert_eq!(
        parse_float::<f32>(b"0x"),
        Err(ParseFloatError::InvalidCharacter)
    );
    assert_eq!(parse_float::<f32>(b"0x1p0").unwrap(), 1.0);
    assert_eq!(parse_float::<f32>(b"-0x1p-1").unwrap(), -0.5);
    assert_eq!(parse_float::<f32>(b"0x10p+10").unwrap(), 16384.0);
    assert_eq!(parse_float::<f32>(b"0x10p-10").unwrap(), 0.015625);
    // 0x0.ffffffp128 (Rust has no hex float literals; the same value):
    assert_eq!(
        parse_float::<f32>(b"0x0.ffffffp128").unwrap(),
        f32::from_bits(0x7f7f_ffff)
    );
    // 0x0.1234570p-125:
    assert_eq!(
        parse_float::<f32>(b"0x0.1234570p-125").unwrap(),
        (0x1234570 as f64 * 2f64.powi(-28 - 125)) as f32
    );
    // Max normalized value.
    assert_eq!(parse_float::<f32>(b"0x1.fffffeP+127").unwrap(), f32::MAX);
    assert_eq!(parse_float::<f32>(b"-0x1.fffffeP+127").unwrap(), -f32::MAX);
    // Min normalized value.
    assert_eq!(parse_float::<f32>(b"0x1p-126").unwrap(), f32::MIN_POSITIVE);
    assert_eq!(
        parse_float::<f32>(b"-0x1p-126").unwrap(),
        -f32::MIN_POSITIVE
    );
    // Min denormal value.
    assert_eq!(parse_float::<f32>(b"0x1P-149").unwrap(), f32::from_bits(1));
    assert_eq!(
        parse_float::<f32>(b"-0x1P-149").unwrap(),
        -f32::from_bits(1)
    );
}

// test "hex.f64"
#[test]
fn hex_f64() {
    assert_eq!(parse_float::<f64>(b"0x1p0").unwrap(), 1.0);
    assert_eq!(parse_float::<f64>(b"-0x1p-1").unwrap(), -0.5);
    assert_eq!(parse_float::<f64>(b"0x10p+10").unwrap(), 16384.0);
    assert_eq!(parse_float::<f64>(b"0x10p-10").unwrap(), 0.015625);
    // Max normalized value.
    assert_eq!(
        parse_float::<f64>(b"0x1.fffffffffffffp+1023").unwrap(),
        f64::MAX
    );
    assert_eq!(
        parse_float::<f64>(b"-0x1.fffffffffffffp1023").unwrap(),
        -f64::MAX
    );
    // Min normalized value.
    assert_eq!(parse_float::<f64>(b"0x1p-1022").unwrap(), f64::MIN_POSITIVE);
    assert_eq!(
        parse_float::<f64>(b"-0x1p-1022").unwrap(),
        -f64::MIN_POSITIVE
    );
    // Min denormalized value.
    assert_eq!(parse_float::<f64>(b"0x1p-1074").unwrap(), f64::from_bits(1));
    assert_eq!(
        parse_float::<f64>(b"-0x1p-1074").unwrap(),
        -f64::from_bits(1)
    );
}

/// The error name, or the value `{d}`, as `zig_corpus.zig` prints an integer.
fn int_result<T: ZigInt + core::fmt::Display>(s: &[u8]) -> String {
    match parse_int::<T>(s, 10) {
        Ok(v) => v.to_string(),
        Err(ParseIntError::Overflow) => "Overflow".to_string(),
        Err(ParseIntError::InvalidCharacter) => "InvalidCharacter".to_string(),
    }
}

/// Zig 0.16.0's own `parseInt(T, s, 10)` results for every corpus input and
/// eight integer types (`zig_corpus.zig`).
#[test]
fn parse_int_matches_zig_over_the_corpus() {
    assert!(INTS.len() > 40);
    for (input, want) in INTS {
        let got = [
            int_result::<i16>(input),
            int_result::<u16>(input),
            int_result::<i32>(input),
            int_result::<u32>(input),
            int_result::<isize>(input),
            int_result::<usize>(input),
            int_result::<u8>(input),
            int_result::<i64>(input),
        ];
        assert_eq!(got, *want, "input {:?}", String::from_utf8_lossy(input));
    }
}

/// Zig 0.16.0's own `parseFloat(f32/f64, s)` results, bit for bit, for every
/// corpus input (`zig_corpus.zig`).
#[test]
fn parse_float_matches_zig_over_the_corpus() {
    assert!(FLOATS.len() > 80);
    for (input, want32, want64) in FLOATS {
        let got32 = match parse_float::<f32>(input) {
            Ok(v) => format!("0x{:08x}", v.to_bits()),
            Err(ParseFloatError::InvalidCharacter) => "InvalidCharacter".to_string(),
        };
        let got64 = match parse_float::<f64>(input) {
            Ok(v) => format!("0x{:016x}", v.to_bits()),
            Err(ParseFloatError::InvalidCharacter) => "InvalidCharacter".to_string(),
        };
        assert_eq!(
            (got32.as_str(), got64.as_str()),
            (*want32, *want64),
            "input {:?}",
            String::from_utf8_lossy(input)
        );
    }
}
