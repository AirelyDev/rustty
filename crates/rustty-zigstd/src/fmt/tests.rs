//! Tests for `fmt`: Zig 0.16.0's own std tests, ported assertion for
//! assertion, then a differential check against Zig's results over a wider
//! corpus (`zig_corpus.rs`), then the tests the other ports of these parsers
//! carried (task tail-s9, NEW-FINDINGS N120/N150; each says where it came
//! from).
//!
//! The odd-width integer rows of `test parseInt` (`i1`, `i2`, `i4`, `i5`,
//! `i43`) and `test parseUnsigned` (`u7`, `u1`, `u2`) run through
//! `parse_int_type` / `parse_unsigned_type` with an `IntType` of the same
//! width and signedness (they were skipped before task tail-s9, which brought
//! in rustty-vt's arbitrary-width core).
//!
//! Skipped from the reference tests, each because Rust has no such type:
//! - `test parseFloat` / `"nan and inf"`: the `f16`, `f80` and `f128`
//!   instantiations (the `f32` and `f64` ones are ported);
//! - `"largest normals"`: the `f16`, `f80` and `f128` rows;
//! - `"#11169"`, `"hex.f16"`, `"hex.f80"`, `"hex.f128"`: `f16`/`f80`/`f128` only;
//! - `"many_digits hex"` compares the `f32` parse with an `f128` parse cast to
//!   `f32`; the `f128` side is replaced by its value, `2^64` (the `f32`
//!   nearest to `0xffffffffffffffff`).
//!
//! Adapted: `"parse unsigned comptime"` runs its assertion at run time
//! (`parse_unsigned` is not a `const fn`).
//!
//! `"float.special"`: ported (a float's `{}` is the same call as a bare `{d}`).
//!
//! `"float.decimal"`: only its four rows that format with a bare `{d}` are
//! ported (`format_float_d`); the rest give a precision, which
//! `format_float_d` does not take.
//!
//! The other tests of `fmt.zig` are for functions this module does not port
//! (`parseIntSizeSuffix`, the general formatter). CORRECTION (task T-F2): this
//! header said every representable test was ported while `"parse u64 digit
//! too big"` and `"parse unsigned comptime"` were missing.

// The float literals are Zig's test literals, verbatim.
#![allow(clippy::approx_constant, clippy::excessive_precision)]

use super::zig_corpus::{FLOATS, INTS};
use super::*;

/// `std.math.approxEqAbs(T, x, y, tolerance)`, over the enclosing test's `T`.
///
/// Zig's `epsilon = 1e-7` and the test's `comptime_float` literal are both
/// coerced to `T`, and the difference is taken in `T` (the form the rustty-args
/// copy of these tests used, task T-C3; this module's copy compared in `f64`
/// before task tail-s9).
macro_rules! approx_eq_abs {
    ($x:expr, $y:expr, $tolerance:expr) => {{
        let x: T = $x;
        let y: T = $y;
        let tolerance: T = $tolerance;
        assert!(tolerance >= 0.0);

        // Fast path for equal values (and signed zeros and infinites).
        if x == y {
            true
        } else if x.is_nan() || y.is_nan() {
            false
        } else {
            (x - y).abs() <= tolerance
        }
    }};
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
    let i1 = IntType::signed(1);
    assert_eq!(parse_int_type(i1, b"-1", 10), Ok(i1.min()));
    assert_eq!(parse_int::<i8>(b"-128", 10), Ok(i8::MIN));
    let i43 = IntType::signed(43);
    assert_eq!(parse_int_type(i43, b"-4398046511104", 10), Ok(i43.min()));

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
    assert_eq!(parse_int_type(IntType::signed(2), b"-10", 2), Ok(-2));
    assert_eq!(parse_int_type(IntType::signed(4), b"-10", 8), Ok(-8));
    assert_eq!(parse_int_type(IntType::signed(5), b"-10", 16), Ok(-16));
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

    let u7 = IntType::unsigned(7);
    assert_eq!(parse_unsigned_type(u7, b"1", 10), Ok(1));
    assert_eq!(parse_unsigned_type(u7, b"1000", 2), Ok(8));

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
    let u1 = IntType::unsigned(1);
    assert_eq!(parse_unsigned_type(u1, b"0", 10), Ok(0));
    assert_eq!(parse_unsigned_type(u1, b"1", 10), Ok(1));
    assert_eq!(
        parse_unsigned_type(u1, b"2", 10),
        Err(ParseIntError::Overflow)
    );
    assert_eq!(parse_unsigned_type(u1, b"001", 16), Ok(1));
    let u2 = IntType::unsigned(2);
    assert_eq!(parse_unsigned_type(u2, b"3", 16), Ok(3));
    assert_eq!(
        parse_unsigned_type(u2, b"4", 16),
        Err(ParseIntError::Overflow)
    );

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

// test "parse u64 digit too big" (std/fmt.zig:673-679): any result other than
// `error.InvalidCharacter` reaches `unreachable`.
#[test]
fn test_parse_u64_digit_too_big() {
    assert_eq!(
        parse_unsigned::<u64>(b"123a", 10),
        Err(ParseIntError::InvalidCharacter)
    );
}

// test "parse unsigned comptime" (std/fmt.zig:681-685). The reference
// evaluates it at comptime; `parse_unsigned` is not a `const fn`, so the same
// assertion runs at run time.
#[test]
fn test_parse_unsigned_comptime() {
    assert_eq!(parse_unsigned::<usize>(b"2", 10), Ok(2));
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

        // const epsilon = 1e-7;
        const EPSILON: T = 1e-7;
        let approx = |s: &[u8], v: T| approx_eq_abs!(p(s).unwrap(), v, EPSILON);
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

// test "float.special" (std/fmt.zig:991-1000). For a float, `{}` reaches the
// same `printFloat(.decimal)` call as a bare `{d}`, which `format_float_d`
// ports. The reference skips the `-nan` row on 32-bit arm only (ARM
// normalizes a negative NaN); no Rust target here is that arch.
#[test]
fn test_float_special() {
    assert_eq!(format!("f64: {}", format_float_d(f64::NAN)), "f64: nan");
    assert_eq!(format!("f64: {}", format_float_d(-f64::NAN)), "f64: -nan");
    assert_eq!(
        format!("f64: {}", format_float_d(f64::INFINITY)),
        "f64: inf"
    );
    assert_eq!(
        format!("f64: {}", format_float_d(f64::NEG_INFINITY)),
        "f64: -inf"
    );
}

// test "float.decimal" (std/fmt.zig:1056-1077): the rows with a bare `{d}`.
#[test]
fn test_float_decimal_bare_d_rows() {
    assert_eq!(
        format_float_d(1.52314e29f64),
        "152314000000000000000000000000"
    );
    assert_eq!(format_float_d(0.0f32), "0");
    assert_eq!(
        format_float_d(1e37f64),
        "10000000000000000000000000000000000000"
    );
    assert_eq!(
        format_float_d(1e38f64),
        "100000000000000000000000000000000000000"
    );
}

/// `format_float_d` gives Zig 0.16.0's own `{d}` text, character for
/// character, for every value of `zig_float_d_corpus.rs` (f32 and f64,
/// including the zeros, the extremes, subnormals, infinities and NaNs).
#[test]
fn format_float_d_matches_zig_over_the_corpus() {
    use super::zig_float_d_corpus::{F32, F64};
    assert!(F32.len() > 30 && F64.len() > 15);
    for &(bits, want) in F32 {
        assert_eq!(
            format_float_d(f32::from_bits(bits)),
            want,
            "f32 {bits:#010x}"
        );
    }
    for &(bits, want) in F64 {
        assert_eq!(
            format_float_d(f64::from_bits(bits)),
            want,
            "f64 {bits:#018x}"
        );
    }
}

// ---------------------------------------------------------------------------
// Tests carried over from the other ports of these parsers (task tail-s9,
// NEW-FINDINGS N120/N150). Each one's origin is named; none duplicates a
// reference table row above.
// ---------------------------------------------------------------------------

// From `rustty_args::std_fmt` (task T-C, parity S67-F3). Inputs the port's
// former `from_str_radix` front end got wrong, each MEASURED against Zig
// 0.16.0 `std.fmt.parseInt(T, s, 0)`: `+-5` was accepted, `-0` for an unsigned
// type was rejected, and a doubled `_` is accepted by the reference.
#[test]
fn parse_int_sign_and_separator_edges() {
    assert!(parse_int::<i32>(b"+-5", 0).is_err());
    assert_eq!(parse_int::<u32>(b"-0", 0), Ok(0));
    assert_eq!(parse_int::<u32>(b"1__0", 0), Ok(10));
    assert_eq!(parse_int::<u64>(b"10_000_000", 0), Ok(10_000_000));
}

// From `rustty_args::std_fmt` (task T-C3). The reference fails with `Overflow`
// at the first digit that leaves the range of `T`, before it reads a later
// invalid character. Each value is MEASURED with Zig 0.16.0
// (`std.fmt.parseInt`); a port that range-checks only at the end returns
// `InvalidCharacter` for every one of them.
#[test]
fn parse_int_overflows_before_a_later_bad_character() {
    assert_eq!(
        parse_int::<i64>(b"99999999999999999999x", 10),
        Err(ParseIntError::Overflow)
    );
    assert_eq!(
        parse_int::<u8>(b"99999999999999999999x", 0),
        Err(ParseIntError::Overflow)
    );
    assert_eq!(parse_int::<u8>(b"300x", 0), Err(ParseIntError::Overflow));
    assert_eq!(parse_int::<i8>(b"300x", 0), Err(ParseIntError::Overflow));
    assert_eq!(parse_int::<i8>(b"-300x", 0), Err(ParseIntError::Overflow));
    assert_eq!(parse_int::<i8>(b"-129x", 10), Err(ParseIntError::Overflow));
    assert_eq!(parse_int::<u32>(b"-1x", 10), Err(ParseIntError::Overflow));
    assert_eq!(parse_int::<u8>(b"0x100x", 0), Err(ParseIntError::Overflow));
}

// From `rustty_args::std_fmt` (task T-C3). The `many_digits` sticky bit of
// `convertHex`: more than 16 significant hex digits set `many_digits`, which
// ORs a 1 into the rounding bits so a value just above a halfway point rounds
// up. `0x1.000001` is exactly halfway between two f32 values (it would round
// to even, down to 1.0); the trailing `...0001` puts it above halfway.
// MEASURED with Zig 0.16.0: `parseFloat(f32, "0x1.000001000000000001p0")` is
// `0x3f800001`. Zig's own std tests do not cover the sticky bit.
#[test]
fn hex_many_digits_sticky_bit_rounds_up() {
    assert_eq!(
        parse_float::<f32>(b"0x1.000001000000000001p0")
            .unwrap()
            .to_bits(),
        0x3f80_0001
    );
}

// From rustty-vt's `zig_parse_int.rs` (parity S170-F1 and siblings). The
// digit separators and the Accumulate width, which are what the terminal
// parsers depend on.
#[test]
fn digit_separators_and_accumulate_width() {
    use ParseIntError::{InvalidCharacter, Overflow};
    assert_eq!(parse_int::<u32>(b"1_0", 10), Ok(10));
    assert_eq!(parse_int::<i32>(b"-1_0", 10), Ok(-10));
    assert_eq!(parse_int::<u32>(b"1__0", 10), Ok(10));
    assert_eq!(parse_int::<u32>(b"10_", 10), Err(InvalidCharacter));
    assert_eq!(parse_int::<u32>(b"_10", 10), Err(InvalidCharacter));
    assert_eq!(parse_int::<u32>(b"_", 10), Err(InvalidCharacter));
    assert_eq!(parse_int::<i32>(b"2147483647", 10), Ok(i32::MAX));
    assert_eq!(parse_int::<i32>(b"-2147483648", 10), Ok(i32::MIN));
    assert_eq!(parse_int::<i32>(b"2147483648", 10), Err(Overflow));
    assert_eq!(parse_int::<u32>(b"-1a", 10), Err(Overflow));

    // u4 accumulates in u8: "99x" reaches the invalid character before
    // the final cast, and "20" fits u8 but not u4.
    let u4 = IntType::unsigned(4);
    assert_eq!(parse_int_type(u4, b"15", 10), Ok(15));
    assert_eq!(parse_int_type(u4, b"16", 10), Err(Overflow));
    assert_eq!(parse_int_type(u4, b"99x", 10), Err(InvalidCharacter));
    assert_eq!(parse_int_type(u4, b"1_2", 10), Ok(12));

    // parseUnsigned takes no sign byte.
    assert_eq!(parse_unsigned::<u16>(b"1_0", 10), Ok(10));
    assert_eq!(parse_unsigned::<u16>(b"+1", 10), Err(InvalidCharacter));
    assert_eq!(parse_unsigned::<u16>(b"-0", 10), Err(InvalidCharacter));
    assert_eq!(parse_int::<usize>(b"-0", 10), Ok(0));

    let u21 = IntType::unsigned(21);
    assert_eq!(parse_int_type(u21, b"1F_FFFF", 16), Ok(0x1F_FFFF));
    assert_eq!(parse_int_type(u21, b"20_0000", 16), Err(Overflow));
}

// From rustty-vt's `color.rs` (its `parse_unsigned_u8` copy): the
// `std.fmt.parseUnsigned(u8, .., 10)` the Kitty OSC 21 key parser calls.
#[test]
fn parse_unsigned_u8_base10_zig_semantics() {
    assert_eq!(parse_unsigned::<u8>(b"0", 10), Ok(0));
    assert_eq!(parse_unsigned::<u8>(b"255", 10), Ok(255));
    assert_eq!(parse_unsigned::<u8>(b"1_2", 10), Ok(12));
    assert_eq!(parse_unsigned::<u8>(b"2_5_5", 10), Ok(255));
    // No sign prefix is accepted.
    assert_eq!(
        parse_unsigned::<u8>(b"+1", 10),
        Err(ParseIntError::InvalidCharacter)
    );
    assert_eq!(
        parse_unsigned::<u8>(b"-1", 10),
        Err(ParseIntError::InvalidCharacter)
    );
    assert_eq!(
        parse_unsigned::<u8>(b"256", 10),
        Err(ParseIntError::Overflow)
    );
    for bad in [&b"_1"[..], &b"1_"[..], &b""[..], &b"0x1"[..], &b" "[..]] {
        assert_eq!(
            parse_unsigned::<u8>(bad, 10),
            Err(ParseIntError::InvalidCharacter),
            "expected InvalidCharacter for {:?}",
            std::str::from_utf8(bad).unwrap()
        );
    }
}

// From rustty-vt's `color.rs` (its `parse_unsigned_u16` copy): the
// `std.fmt.parseUnsigned(u16, .., 16)` that `RGB.fromHex` calls.
#[test]
fn parse_unsigned_u16_base16_zig_semantics() {
    assert_eq!(Ok(0xffff), parse_unsigned::<u16>(b"ffff", 16));
    assert_eq!(Ok(0x0f), parse_unsigned::<u16>(b"F", 16));
    assert_eq!(Ok(0xff), parse_unsigned::<u16>(b"f_f", 16));
    // Signs are `charToDigit` failures, not sign prefixes.
    assert_eq!(
        Err(ParseIntError::InvalidCharacter),
        parse_unsigned::<u16>(b"+f", 16)
    );
    assert_eq!(
        Err(ParseIntError::InvalidCharacter),
        parse_unsigned::<u16>(b"-f", 16)
    );
    // Leading/trailing separators.
    assert_eq!(
        Err(ParseIntError::InvalidCharacter),
        parse_unsigned::<u16>(b"_f", 16)
    );
    assert_eq!(
        Err(ParseIntError::InvalidCharacter),
        parse_unsigned::<u16>(b"f_", 16)
    );
    assert_eq!(
        Err(ParseIntError::InvalidCharacter),
        parse_unsigned::<u16>(b"", 16)
    );
    // `u16` width accumulate: overflow, not truncation.
    assert_eq!(
        Err(ParseIntError::Overflow),
        parse_unsigned::<u16>(b"fffff", 16)
    );
    // base 0 prefix detection.
    assert_eq!(Ok(255), parse_unsigned::<u16>(b"0xff", 0));
    assert_eq!(Ok(10), parse_unsigned::<u16>(b"10", 0));
}
