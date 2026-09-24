//! PORTS: Zig 0.16.0 `std.fmt.parseInt` / `std.fmt.parseUnsigned`
//! (`lib/std/fmt.zig:306-460`, `charToDigit` `:570-581`) and
//! `std.fmt.parseFloat` (`lib/std/fmt/parse_float.zig` with
//! `parse_float/{parse,FloatStream,common,convert_hex}.zig`), for every integer
//! width the port parses (the native Rust types, and Zig's odd widths such as
//! `u4`, `u9`, `u21` through [`IntType`]) and for `f32`/`f64`.
//!
//! Rust's `str::parse` accepts a different grammar. Zig's integer parser skips
//! `_` digit separators, reads `-0` into an unsigned type and rejects a second
//! sign (`+-5`); its float parser takes `_` separators between two digits and
//! hex floats (`0x1.8p3`, with an optional exponent). Every such input is one
//! Ghostty accepts and a port using `str::parse` rejects, or the reverse
//! (S67-F3/F4 `cli/args.zig` `parseIntoField`, S75-F1 the config values,
//! S85-F1 `Metrics.Modifier.parse`, S100-F1 the keybind action parameters, and
//! the terminal parsers of S170-F1, S186-F1, S194-F3, S195-F2, S197-F1).
//!
//! PORT NOTE (NEW-FINDINGS N120, N150): this is the ONE port of these parsers.
//! Three batches ported them separately (`rustty_args::std_fmt`, this module,
//! and rustty-vt's `zig_parse_int.rs`, plus partial copies in rustty-vt's
//! `color.rs` and rustty-config's `u21` hex parser); task tail-s9 compared them
//! against Zig 0.16.0 over the union of their tables and a fresh probe (no
//! value differed), kept the arbitrary-width integer core of the rustty-vt copy
//! here, and deleted the others. rustty-vt keeps only its UD141 `_` filter
//! (`protocol_int::reject_digit_separators`), which runs before this parser.
//!
//! The float GRAMMAR is ported in full, because that decides acceptance. The
//! DECIMAL numeric conversion (`convertFast` / `convertEiselLemire` /
//! `convertSlow`) is delegated to Rust's `f32`/`f64` `from_str` once the grammar
//! has accepted: both are correctly rounded (round half to even) over the same
//! digits, so re-deriving them would reproduce the same bits. The HEX
//! conversion has no Rust counterpart and is ported (`convert_hex.zig`).

use core::fmt;

// pub const ParseIntError = error{
//     /// The result cannot fit in the type specified.
//     Overflow,
//     /// The input was empty or contained an invalid character.
//     InvalidCharacter,
// };
/// `std.fmt.ParseIntError`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseIntError {
    /// The result cannot fit in the type specified.
    Overflow,
    /// The input was empty or contained an invalid character.
    InvalidCharacter,
}

impl fmt::Display for ParseIntError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // (Zig formats an error value as `error.Name`.)
        match self {
            ParseIntError::Overflow => f.write_str("error.Overflow"),
            ParseIntError::InvalidCharacter => f.write_str("error.InvalidCharacter"),
        }
    }
}

/// `std.fmt.ParseFloatError`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseFloatError {
    InvalidCharacter,
}

impl fmt::Display for ParseFloatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("error.InvalidCharacter")
    }
}

/// A Zig integer type: its bit width and signedness.
///
/// Zig's result type is a comptime integer type of any width (`u4`, `u9`,
/// `u21`, `i43`, ...). Rust has no such types, so [`parse_int_type`] and
/// [`parse_unsigned_type`] take the width and signedness at run time and return
/// the value widened to `i128`; [`parse_int`] and [`parse_unsigned`] give the
/// native-width callers a typed result. Widths from 1 to 64 bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntType {
    pub bits: u32,
    pub signed: bool,
}

impl IntType {
    /// `uN`.
    pub const fn unsigned(bits: u32) -> IntType {
        assert!(bits >= 1 && bits <= 64);
        IntType {
            bits,
            signed: false,
        }
    }

    /// `iN`.
    pub const fn signed(bits: u32) -> IntType {
        assert!(bits >= 1 && bits <= 64);
        IntType { bits, signed: true }
    }

    /// `math.minInt(Self)`.
    pub const fn min(self) -> i128 {
        if self.signed {
            -(1i128 << (self.bits - 1))
        } else {
            0
        }
    }

    /// `math.maxInt(Self)`.
    pub const fn max(self) -> i128 {
        if self.signed {
            (1i128 << (self.bits - 1)) - 1
        } else {
            (1i128 << self.bits) - 1
        }
    }

    /// `math.cast(Self, v)`: `Some(v)` when `v` fits in this type.
    #[inline]
    const fn cast(self, v: i128) -> Option<i128> {
        if self.min() <= v && v <= self.max() {
            Some(v)
        } else {
            None
        }
    }
}

/// A native Rust integer type `parse_int` produces, with its Zig integer type.
pub trait ZigInt: Copy + Sized {
    const TYPE: IntType;
    /// The parsed value, which [`parse_int_type`] has already range-checked
    /// against `TYPE`.
    fn from_i128(v: i128) -> Self;
}

macro_rules! zig_int {
    ($($t:ty => $it:expr),* $(,)?) => {$(
        impl ZigInt for $t {
            const TYPE: IntType = $it;
            #[inline]
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
    usize => IntType::unsigned(usize::BITS),
    i8 => IntType::signed(8),
    i16 => IntType::signed(16),
    i32 => IntType::signed(32),
    i64 => IntType::signed(64),
    isize => IntType::signed(isize::BITS),
}

// pub fn charToDigit(c: u8, base: u8) (error{InvalidCharacter}!u8) {
/// `std.fmt.charToDigit`.
#[inline]
pub fn char_to_digit(c: u8, base: u8) -> Result<u8, ParseIntError> {
    // const value = switch (c) { '0'...'9' => c - '0', 'A'...'Z' => c - 'A' + 10,
    //     'a'...'z' => c - 'a' + 10, else => return error.InvalidCharacter };
    let value = match c {
        b'0'..=b'9' => c - b'0',
        b'A'..=b'Z' => c - b'A' + 10,
        b'a'..=b'z' => c - b'a' + 10,
        _ => return Err(ParseIntError::InvalidCharacter),
    };
    // if (value >= base) return error.InvalidCharacter;
    if value >= base {
        return Err(ParseIntError::InvalidCharacter);
    }
    Ok(value)
}

/// `std.fmt.parseInt(T, buf, base)` for a native Rust integer type.
///
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
/// See also `parseUnsigned`.
#[inline]
pub fn parse_int<T: ZigInt>(buf: &[u8], base: u8) -> Result<T, ParseIntError> {
    parse_int_type(T::TYPE, buf, base).map(T::from_i128)
}

/// `std.fmt.parseUnsigned(T, buf, base)` for a native Rust integer type.
///
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
#[inline]
pub fn parse_unsigned<T: ZigInt>(buf: &[u8], base: u8) -> Result<T, ParseIntError> {
    parse_unsigned_type(T::TYPE, buf, base).map(T::from_i128)
}

// pub fn parseInt(comptime T: type, buf: []const u8, base: u8) ParseIntError!T {
//     return parseIntWithGenericCharacter(T, u8, buf, base);
// }
/// `std.fmt.parseInt(T, buf, base)` for the Zig integer type `t` (any width),
/// the value widened to `i128`.
pub fn parse_int_type(t: IntType, buf: &[u8], base: u8) -> Result<i128, ParseIntError> {
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

// pub fn parseUnsigned(comptime T: type, buf: []const u8, base: u8) ParseIntError!T {
//     return parseIntWithSign(T, u8, buf, base, .pos);
// }
/// `std.fmt.parseUnsigned(T, buf, base)` for the Zig integer type `t` (any
/// width), the value widened to `i128`.
pub fn parse_unsigned_type(t: IntType, buf: &[u8], base: u8) -> Result<i128, ParseIntError> {
    parse_int_with_sign(t, buf, base, false)
}

// fn parseIntWithSign(
//     comptime Result: type,
//     comptime Character: type,
//     buf: []const Character,
//     base: u8,
//     comptime sign: enum { pos, neg },
// ) ParseIntError!Result {
//
// PORT NOTE: every step is computed in `i128`, which holds every value of a
// type of at most 64 bits times any `u8` base, and range-checked against
// `Accumulate` right away, so `Overflow` is reported at the same digit as in
// the reference, BEFORE a later invalid character is read
// (`parseInt(i64, "99999999999999999999x", 10)` and `parseInt(u8, "300x", 0)`
// are `error.Overflow`, MEASURED with Zig 0.16.0).
//
// PERF(port): `parse_int_type`, `parse_unsigned_type` and this function are
// deliberately NOT `#[inline]`. With the hint, every rustty-vt call site got
// its own copy, which reshuffled rustty-vt's codegen units and cost the
// unrelated `+terminal-parser` bench about 10% (paired runs, task tail-s9);
// without it the benches match the pre-consolidation binary.
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

    // const add = switch (sign) { .pos => math.add, .neg => math.sub };

    // accumulate into Accumulate which is always 8 bits or larger.  this prevents
    // `buf_base` from overflowing Result.
    // const Accumulate = std.meta.Int(info.int.signedness, @max(8, info.int.bits));
    let accumulate_t = IntType {
        bits: result.bits.max(8),
        signed: result.signed,
    };
    // var accumulate: Accumulate = 0;
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

// ===========================================================================
// std.fmt.parseFloat
// ===========================================================================

/// A float type `parse_float` produces: `f32` or `f64`. The constants are the
/// `std.math.float*` queries `convert_hex.zig` reads; both types use a `u64`
/// mantissa (`common.mantissaType`).
pub trait ZigFloat: Copy + PartialEq + fmt::Display {
    /// The sign bit (`std.math.signbit`).
    fn sign_bit(self) -> bool;
    /// `std.math.isNan`.
    fn nan(self) -> bool;
    /// `math.floatExponentMax(T)`.
    const MAX_EXP: i64;
    /// `math.floatExponentMin(T)`.
    const MIN_EXP: i64;
    /// `math.floatMantissaBits(T)`.
    const MANTISSA_BITS: u32;
    /// `math.floatFractionalBits(T)`.
    const FRACTIONAL_BITS: u32;
    /// `math.floatExponentBits(T)`.
    const EXP_BITS: u32;
    const INFINITY: Self;
    const NEG_INFINITY: Self;
    const NAN: Self;
    const ZERO: Self;
    const NEG_ZERO: Self;
    /// `floatFromUnsigned(T, u64, bits)`.
    fn from_bits_u64(bits: u64) -> Self;
    /// The correctly rounded value of an accepted decimal (see the module doc).
    fn from_decimal(s: &str) -> Option<Self>;
}

impl ZigFloat for f64 {
    fn sign_bit(self) -> bool {
        self.is_sign_negative()
    }
    fn nan(self) -> bool {
        self.is_nan()
    }
    const MAX_EXP: i64 = 1023;
    const MIN_EXP: i64 = -1022;
    const MANTISSA_BITS: u32 = 52;
    const FRACTIONAL_BITS: u32 = 52;
    const EXP_BITS: u32 = 11;
    const INFINITY: Self = f64::INFINITY;
    const NEG_INFINITY: Self = f64::NEG_INFINITY;
    const NAN: Self = f64::NAN;
    const ZERO: Self = 0.0;
    const NEG_ZERO: Self = -0.0;
    fn from_bits_u64(bits: u64) -> Self {
        f64::from_bits(bits)
    }
    fn from_decimal(s: &str) -> Option<Self> {
        s.parse::<f64>().ok()
    }
}

impl ZigFloat for f32 {
    fn sign_bit(self) -> bool {
        self.is_sign_negative()
    }
    fn nan(self) -> bool {
        self.is_nan()
    }
    const MAX_EXP: i64 = 127;
    const MIN_EXP: i64 = -126;
    const MANTISSA_BITS: u32 = 23;
    const FRACTIONAL_BITS: u32 = 23;
    const EXP_BITS: u32 = 8;
    const INFINITY: Self = f32::INFINITY;
    const NEG_INFINITY: Self = f32::NEG_INFINITY;
    const NAN: Self = f32::NAN;
    const ZERO: Self = 0.0;
    const NEG_ZERO: Self = -0.0;
    fn from_bits_u64(bits: u64) -> Self {
        // @bitCast(@as(u32, @truncate(bits)))
        f32::from_bits(bits as u32)
    }
    fn from_decimal(s: &str) -> Option<Self> {
        s.parse::<f32>().ok()
    }
}

/// `parse_float/common.zig` `isDigit`.
fn is_digit(c: u8, base: u8) -> bool {
    debug_assert!(base == 10 || base == 16);
    if base == 10 {
        c.is_ascii_digit()
    } else {
        c.is_ascii_hexdigit()
    }
}

/// `parse_float/parse.zig` `validUnderscores`.
fn valid_underscores(s: &[u8], base: u8) -> bool {
    let mut i: usize = 0;
    while i < s.len() {
        if s[i] == b'_' {
            // underscore at start of end
            if i == 0 || i + 1 == s.len() {
                return false;
            }
            // consecutive underscores
            if !is_digit(s[i - 1], base) || !is_digit(s[i + 1], base) {
                return false;
            }

            // next is guaranteed a digit, skip an extra
            i += 1;
        }
        i += 1;
    }
    true
}

/// `parse_float/FloatStream.zig`.
struct FloatStream<'a> {
    slice: &'a [u8],
    offset: usize,
    underscore_count: usize,
}

impl<'a> FloatStream<'a> {
    fn init(s: &'a [u8]) -> Self {
        FloatStream {
            slice: s,
            offset: 0,
            underscore_count: 0,
        }
    }

    // Returns the offset from the start *excluding* any underscores that were found.
    fn offset_true(&self) -> usize {
        self.offset - self.underscore_count
    }

    fn reset(&mut self) {
        self.offset = 0;
        self.underscore_count = 0;
    }

    fn has_len(&self, n: usize) -> bool {
        self.offset + n <= self.slice.len()
    }

    fn first(&self) -> Option<u8> {
        if self.has_len(1) {
            Some(self.slice[self.offset])
        } else {
            None
        }
    }

    fn first_is(&self, cs: &[u8]) -> bool {
        matches!(self.first(), Some(c) if cs.contains(&c))
    }

    fn first_is_lower(&self, cs: &[u8]) -> bool {
        matches!(self.first(), Some(c) if cs.contains(&(c | 0x20)))
    }

    fn first_is_digit(&self, base: u8) -> bool {
        matches!(self.first(), Some(c) if is_digit(c, base))
    }

    fn advance(&mut self, n: usize) {
        self.offset += n;
    }

    fn scan_digit(&mut self, base: u8) -> Option<u8> {
        debug_assert!(base == 10 || base == 16);
        // retry: while (true) {
        loop {
            match self.first() {
                Some(c) if c.is_ascii_digit() => {
                    self.advance(1);
                    return Some(c - b'0');
                }
                Some(c) if base == 16 && (b'a'..=b'f').contains(&c) => {
                    self.advance(1);
                    return Some(c - b'a' + 10);
                }
                Some(c) if base == 16 && (b'A'..=b'F').contains(&c) => {
                    self.advance(1);
                    return Some(c - b'A' + 10);
                }
                Some(b'_') => {
                    self.advance(1);
                    self.underscore_count += 1;
                    // continue :retry;
                }
                _ => return None,
            }
        }
    }
}

/// `parse_float/common.zig` `Number(T)` (a `u64` mantissa for `f32`/`f64`).
struct Number {
    exponent: i64,
    mantissa: u64,
    negative: bool,
    /// More than max_mantissa_digits digits were parsed.
    many_digits: bool,
    /// The number was a hex-float (e.g. 0x1.234p567).
    hex: bool,
}

/// `parse.zig` `tryParseDigits`.
///
/// PORT NOTE: the reference reads 8 decimal digits at a time first
/// (`parse8Digits`), an optimisation of this same recurrence that stops at the
/// first non-digit (an `_` included); it changes no result and is not ported.
fn try_parse_digits(stream: &mut FloatStream<'_>, x: &mut u64, base: u8) {
    while let Some(digit) = stream.scan_digit(base) {
        *x = x.wrapping_mul(u64::from(base));
        *x = x.wrapping_add(u64::from(digit));
    }
}

/// `parse.zig` `min_n_digit_int` (`n *= 10` whatever the base).
fn min_n_digit_int(digit_count: usize) -> u64 {
    let mut n: u64 = 1;
    let mut i: usize = 1;
    while i < digit_count {
        n *= 10;
        i += 1;
    }
    n
}

/// `parse.zig` `tryParseNDigits`.
fn try_parse_n_digits(stream: &mut FloatStream<'_>, x: &mut u64, base: u8, n: usize) {
    while *x < min_n_digit_int(n) {
        match stream.scan_digit(base) {
            Some(digit) => {
                *x = x.wrapping_mul(u64::from(base));
                *x = x.wrapping_add(u64::from(digit));
            }
            None => break,
        }
    }
}

/// `parse.zig` `parseScientific`.
fn parse_scientific(stream: &mut FloatStream<'_>) -> Option<i64> {
    let mut exponent: i64 = 0;
    let mut negative = false;

    if let Some(c) = stream.first() {
        negative = c == b'-';
        if c == b'-' || c == b'+' {
            stream.advance(1);
        }
    }
    if stream.first_is_digit(10) {
        while let Some(digit) = stream.scan_digit(10) {
            // no overflows here, saturate well before overflow
            if exponent < 0x1000_0000 {
                exponent = 10 * exponent + i64::from(digit);
            }
        }

        return Some(if negative { -exponent } else { exponent });
    }

    None
}

/// `parse.zig` `ParseInfo`.
struct ParseInfo {
    // 10 or 16
    base: u8,
    // 10^19 fits in u64, 16^16 fits in u64
    max_mantissa_digits: usize,
    // e.g. e or p (E and P also checked)
    exp_char_lower: u8,
}

/// `parse.zig` `parsePartialNumberBase`.
fn parse_partial_number_base(
    stream: &mut FloatStream<'_>,
    negative: bool,
    n: &mut usize,
    info: ParseInfo,
) -> Option<Number> {
    debug_assert!(info.base == 10 || info.base == 16);

    // parse initial digits before dot
    let mut mantissa: u64 = 0;
    try_parse_digits(stream, &mut mantissa, info.base);
    let int_end = stream.offset_true();
    let mut n_digits = stream.offset_true() as isize;

    // handle dot with the following digits
    let mut exponent: i64 = 0;
    if stream.first_is(b".") {
        stream.advance(1);
        let marker = stream.offset_true();
        try_parse_digits(stream, &mut mantissa, info.base);
        let n_after_dot = stream.offset_true() - marker;
        exponent = -(n_after_dot as i64);
        n_digits += n_after_dot as isize;
    }

    // adjust required shift to offset mantissa for base-16 (2^4)
    if info.base == 16 {
        exponent *= 4;
    }

    if n_digits == 0 {
        return None;
    }

    // handle scientific format
    let mut exp_number: i64 = 0;
    if stream.first_is_lower(&[info.exp_char_lower]) {
        stream.advance(1);
        exp_number = parse_scientific(stream)?;
        exponent += exp_number;
    }

    let len = stream.offset; // length must be complete parsed length
    *n += len;

    if stream.underscore_count > 0 && !valid_underscores(stream.slice, info.base) {
        return None;
    }

    // common case with not many digits
    if n_digits <= info.max_mantissa_digits as isize {
        return Some(Number {
            exponent,
            mantissa,
            negative,
            many_digits: false,
            hex: info.base == 16,
        });
    }

    n_digits -= info.max_mantissa_digits as isize;
    let mut many_digits = false;
    stream.reset(); // re-parse from beginning
    while stream.first_is(b"0._") {
        // '0' = '.' + 2
        let next = stream.slice[stream.offset];
        if next != b'_' {
            // n_digits -= @as(isize, @intCast(next -| ('0' - 1)));
            n_digits -= next.saturating_sub(b'0' - 1) as isize;
        } else {
            stream.underscore_count += 1;
        }
        stream.advance(1);
    }
    if n_digits > 0 {
        // at this point we have more than max_mantissa_digits significant digits, let's try again
        many_digits = true;
        mantissa = 0;
        stream.reset();
        try_parse_n_digits(stream, &mut mantissa, info.base, info.max_mantissa_digits);

        exponent = if mantissa >= min_n_digit_int(info.max_mantissa_digits) {
            // big int
            int_end as i64 - stream.offset_true() as i64
        } else {
            // the next byte must be present and be '.'
            // We know this is true because we had more than 19
            // digits previously, so we overflowed a 64-bit integer,
            // but parsing only the integral digits produced less
            // than 19 digits. That means we must have a decimal
            // point, and at least 1 fractional digit.
            stream.advance(1);
            let marker = stream.offset_true();
            try_parse_n_digits(stream, &mut mantissa, info.base, info.max_mantissa_digits);
            marker as i64 - stream.offset_true() as i64
        };
        if info.base == 16 {
            exponent *= 4;
        }
        // add back the explicit part
        exponent += exp_number;
    }

    Some(Number {
        exponent,
        mantissa,
        negative,
        many_digits,
        hex: info.base == 16,
    })
}

/// `parse.zig` `parsePartialNumber`.
fn parse_partial_number(s: &[u8], negative: bool, n: &mut usize) -> Option<Number> {
    debug_assert!(!s.is_empty());
    *n = 0;

    if s.len() >= 2 && s[0] == b'0' && s[1].eq_ignore_ascii_case(&b'x') {
        let mut stream = FloatStream::init(&s[2..]);
        *n += 2;
        parse_partial_number_base(
            &mut stream,
            negative,
            n,
            ParseInfo {
                base: 16,
                max_mantissa_digits: 16,
                exp_char_lower: b'p',
            },
        )
    } else {
        let mut stream = FloatStream::init(s);
        parse_partial_number_base(
            &mut stream,
            negative,
            n,
            ParseInfo {
                base: 10,
                max_mantissa_digits: 19,
                exp_char_lower: b'e',
            },
        )
    }
}

/// `parse.zig` `parseNumber`.
fn parse_number(s: &[u8], negative: bool) -> Option<Number> {
    let mut consumed: usize = 0;
    let number = parse_partial_number(s, negative, &mut consumed)?;
    // must consume entire float (no trailing data)
    if s.len() == consumed {
        Some(number)
    } else {
        None
    }
}

/// `std.ascii.startsWithIgnoreCase`.
fn starts_with_ignore_case(s: &[u8], prefix: &[u8]) -> bool {
    s.len() >= prefix.len() && s[..prefix.len()].eq_ignore_ascii_case(prefix)
}

/// `parse.zig` `parsePartialInfOrNan`.
fn parse_partial_inf_or_nan<T: ZigFloat>(s: &[u8], negative: bool, n: &mut usize) -> Option<T> {
    // inf/infinity; infxxx should only consume inf.
    if starts_with_ignore_case(s, b"inf") {
        *n = 3;
        if starts_with_ignore_case(&s[3..], b"inity") {
            *n = 8;
        }
        return Some(if !negative {
            T::INFINITY
        } else {
            T::NEG_INFINITY
        });
    }

    if starts_with_ignore_case(s, b"nan") {
        *n = 3;
        return Some(T::NAN);
    }

    None
}

/// `parse.zig` `parseInfOrNan`.
fn parse_inf_or_nan<T: ZigFloat>(s: &[u8], negative: bool) -> Option<T> {
    let mut consumed: usize = 0;
    let special = parse_partial_inf_or_nan::<T>(s, negative, &mut consumed)?;
    if s.len() == consumed {
        Some(special)
    } else {
        None
    }
}

/// `convert_hex.zig` `convertHex`: converts the form `0xMMM.NNNpEEE`
/// (`MMM.NNN` = mantissa stored as an integer, `EEE` = the offset exponent).
/// Derived (in the reference) from golang strconv/atof.go.
fn convert_hex<T: ZigFloat>(n_: Number) -> T {
    let mut n = n_;

    if n.mantissa == 0 {
        return if n.negative { T::NEG_ZERO } else { T::ZERO };
    }

    let max_exp = T::MAX_EXP;
    let min_exp = T::MIN_EXP;
    let mantissa_bits = T::MANTISSA_BITS;
    let fractional_bits = T::FRACTIONAL_BITS;
    let exp_bits = T::EXP_BITS;
    let exp_bias = min_exp - 1;

    // mantissa now implicitly divided by 2^fractional_bits
    n.exponent += i64::from(fractional_bits);

    // Shift mantissa and exponent to bring representation into float range.
    // Eventually we want a mantissa with a leading 1-bit followed by mantbits other bits.
    // For rounding, we need two more, where the bottom bit represents
    // whether that bit or any later bit was non-zero.
    // (If the mantissa has already lost non-zero bits, trunc is true,
    // and we OR in a 1 below after shifting left appropriately.)
    while n.mantissa != 0 && n.mantissa >> (mantissa_bits + 2) == 0 {
        n.mantissa <<= 1;
        n.exponent -= 1;
    }
    if n.many_digits {
        n.mantissa |= 1;
    }
    while n.mantissa >> (1 + fractional_bits + 2) != 0 {
        n.mantissa = (n.mantissa >> 1) | (n.mantissa & 1);
        n.exponent += 1;
    }

    // If exponent is too negative,
    // denormalize in hopes of making it representable.
    // (The -2 is for the rounding bits.)
    while n.mantissa > 1 && n.exponent < min_exp - 2 {
        n.mantissa = (n.mantissa >> 1) | (n.mantissa & 1);
        n.exponent += 1;
    }

    // Round using two bottom bits.
    let mut round = n.mantissa & 3;
    n.mantissa >>= 2;
    round |= n.mantissa & 1; // round to even (round up if mantissa is odd)
    n.exponent += 2;
    if round == 3 {
        n.mantissa += 1;
        if n.mantissa == 1 << (1 + fractional_bits) {
            n.mantissa >>= 1;
            n.exponent += 1;
        }
    }

    // Denormal or zero
    if n.mantissa >> fractional_bits == 0 {
        n.exponent = exp_bias;
    }

    // Infinity and range error
    if n.exponent > max_exp {
        return if n.negative {
            T::NEG_INFINITY
        } else {
            T::INFINITY
        };
    }

    let mut bits = n.mantissa & ((1u64 << mantissa_bits) - 1);
    bits |= (((n.exponent - exp_bias) & ((1i64 << exp_bits) - 1)) as u64) << mantissa_bits;
    if n.negative {
        bits |= 1u64 << (mantissa_bits + exp_bits);
    }
    T::from_bits_u64(bits)
}

/// `std.fmt.parseFloat(T, s)` for `T` = `f32` or `f64`.
pub fn parse_float<T: ZigFloat>(s: &[u8]) -> Result<T, ParseFloatError> {
    if s.is_empty() {
        return Err(ParseFloatError::InvalidCharacter);
    }

    let mut i: usize = 0;
    let negative = s[i] == b'-';
    if s[i] == b'-' || s[i] == b'+' {
        i += 1;
    }
    if s.len() == i {
        return Err(ParseFloatError::InvalidCharacter);
    }

    // const n = parse.parseNumber(T, s[i..], negative) orelse {
    //     return parse.parseInfOrNan(T, s[i..], negative) orelse error.InvalidCharacter;
    // };
    let Some(n) = parse_number(&s[i..], negative) else {
        return parse_inf_or_nan::<T>(&s[i..], negative).ok_or(ParseFloatError::InvalidCharacter);
    };

    // if (n.hex) return convertHex(T, n);
    if n.hex {
        return Ok(convert_hex::<T>(n));
    }

    // convertFast / convertEiselLemire / convertSlow: see the module doc. The
    // grammar has accepted, so what remains is a sign, digits, at most one
    // '.', an optional exponent, and `_` separators `validUnderscores` has
    // proven well placed; with those removed, `from_str` reads the same number.
    let mut cleaned = String::with_capacity(s.len());
    if negative {
        cleaned.push('-');
    }
    cleaned.extend(s[i..].iter().filter(|&&c| c != b'_').map(|&c| c as char));
    T::from_decimal(&cleaned).ok_or(ParseFloatError::InvalidCharacter)
}

/// Zig's `{d}` for a float (`std.fmt` decimal float formatting): the shortest
/// decimal that reads back to the same value, never in exponent form, and
/// `inf` / `-inf` / `nan` / `-nan`. Rust's `Display` for `f32`/`f64` is the
/// same shortest round-trip decimal without an exponent; the one difference is
/// NaN, which Rust spells `NaN` for either sign (`std/fmt/float.zig`
/// `copySpecialStr` prints the sign). Checked against Zig 0.16.0 over
/// `zig_float_d_corpus.rs`.
pub fn format_float_d<T: ZigFloat>(v: T) -> String {
    if v.nan() {
        return if v.sign_bit() { "-nan" } else { "nan" }.to_string();
    }
    format!("{v}")
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod zig_corpus;
#[cfg(test)]
mod zig_float_d_corpus;
