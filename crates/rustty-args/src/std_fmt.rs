//! Ports of the two Zig standard-library number parsers that `cli/args.zig`
//! calls directly: `std.fmt.parseInt` and `std.fmt.parseFloat` (Zig 0.16.0, the
//! version `ghostty/build.zig.zon` pins as `.minimum_zig_version`).
//!
//! PORT NOTE (parity S67-F3, S67-F4): `parseIntoField` parses integer fields
//! with `std.fmt.parseInt(Int, value, 0)` and float fields with
//! `std.fmt.parseFloat(Float, value)`. The port used Rust's `from_str_radix` and
//! `str::parse::<f32/f64>()`, whose grammars are narrower: neither accepts a `_`
//! digit separator, Rust has no hexadecimal float literal, and `from_str_radix`
//! rejects `-0` for an unsigned type and accepts a second sign (`+-5`). Every
//! difference was a value the reference accepted and the port rejected, or the
//! reverse. The grammars are ported here in full because the grammar decides
//! acceptance.
//!
//! The DECIMAL float conversion (`convertFast` / `convertEiselLemire` /
//! `convertSlow`) is delegated to Rust's `str::parse` once the reference's
//! grammar has accepted the input: both are correctly rounded over the same
//! digits, so re-deriving them would reproduce the same bits. The HEX
//! conversion (`convert_hex.zig`) has no Rust equivalent and is ported.
//!
//! PORT NOTE (duplication, 2026-09-23): `rustty-config/src/value_types.rs`
//! carries an f64-only copy of the same `parse_float` grammar (`pf_*`, parity
//! S75-F1). That crate sits above this one; switching it to this module is
//! recorded in `NEW-FINDINGS.md` N120, not done here (config is another area).

/// `std.fmt.ParseIntError`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseIntError {
    /// The result cannot fit in the type specified.
    Overflow,
    /// The input was empty or contained an invalid character.
    InvalidCharacter,
}

/// `std.fmt.ParseFloatError` (`error{InvalidCharacter}`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidCharacter;

/// An integer type `std.fmt.parseInt` can produce. The port accumulates in
/// `i128`, which holds every value of every implementor, and range-checks at the
/// end (`from_i128`).
//
// PORT NOTE: the reference accumulates in `Accumulate` (the result type widened
// to at least 8 bits) and fails with `Overflow` the moment the partial value
// leaves that range. A partial value only grows in magnitude as digits are
// added, so a value that leaves the range part-way also ends outside it: the
// port fails at the end instead, with the same error. Only the ERROR VARIANT
// ordering could differ (the reference stops at the first overflow before it
// sees a later bad character), and both variants are errors.
pub trait ZigInt: Sized {
    fn from_i128(v: i128) -> Option<Self>;
}

macro_rules! impl_zig_int {
    ($($t:ty),*) => {$(
        impl ZigInt for $t {
            fn from_i128(v: i128) -> Option<Self> {
                <$t>::try_from(v).ok()
            }
        }
    )*};
}
impl_zig_int!(u8, u16, u32, u64, usize, i8, i16, i32, i64, isize);

/// `std.fmt.charToDigit`.
fn char_to_digit(c: u8, base: u8) -> Result<u8, ParseIntError> {
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

/// `std.fmt.parseInt(T, buf, base)`.
///
/// When `base` is zero the string prefix is examined to detect the true base:
///  * A prefix of "0b" implies base=2,
///  * A prefix of "0o" implies base=8,
///  * A prefix of "0x" implies base=16,
///  * Otherwise base=10 is assumed.
///
/// Ignores '_' character in `buf`.
pub fn parse_int<T: ZigInt>(buf: &[u8], base: u8) -> Result<T, ParseIntError> {
    // if (buf.len == 0) return error.InvalidCharacter;
    // if (buf[0] == '+') return parseIntWithSign(Result, Character, buf[1..], base, .pos);
    // if (buf[0] == '-') return parseIntWithSign(Result, Character, buf[1..], base, .neg);
    // return parseIntWithSign(Result, Character, buf, base, .pos);
    match buf.first() {
        None => Err(ParseIntError::InvalidCharacter),
        Some(b'+') => parse_int_with_sign(&buf[1..], base, false),
        Some(b'-') => parse_int_with_sign(&buf[1..], base, true),
        Some(_) => parse_int_with_sign(buf, base, false),
    }
}

/// `std.fmt.parseIntWithSign`.
fn parse_int_with_sign<T: ZigInt>(buf: &[u8], base: u8, neg: bool) -> Result<T, ParseIntError> {
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

    // if (buf_start[0] == '_' or buf_start[buf_start.len - 1] == '_') return error.InvalidCharacter;
    if buf_start[0] == b'_' || buf_start[buf_start.len() - 1] == b'_' {
        return Err(ParseIntError::InvalidCharacter);
    }

    let mut accumulate: i128 = 0;
    for &c in buf_start {
        // if (c == '_') continue;
        if c == b'_' {
            continue;
        }
        let digit = i128::from(char_to_digit(c, buf_base)?);
        // .pos => math.add, .neg => math.sub
        accumulate = accumulate
            .checked_mul(i128::from(buf_base))
            .and_then(|a| {
                if neg {
                    a.checked_sub(digit)
                } else {
                    a.checked_add(digit)
                }
            })
            .ok_or(ParseIntError::Overflow)?;
    }

    // return math.cast(Result, accumulate) orelse return error.Overflow;
    T::from_i128(accumulate).ok_or(ParseIntError::Overflow)
}

/// A float type `std.fmt.parseFloat` can produce here (`f32`, `f64`). Both have
/// `common.mantissaType(T) == u64`, so they share one `Number` shape and the
/// 16-hex / 19-decimal `max_mantissa_digits`.
pub trait ZigFloat: Copy + std::str::FromStr {
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
    /// `common.floatFromUnsigned(T, u64, bits)` — `@bitCast(@truncate(bits))`.
    fn from_bits_truncate(bits: u64) -> Self;
}

impl ZigFloat for f32 {
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
    fn from_bits_truncate(bits: u64) -> Self {
        // @as(f32, @bitCast(@as(u32, @truncate(v))))
        f32::from_bits(bits as u32)
    }
}

impl ZigFloat for f64 {
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
    fn from_bits_truncate(bits: u64) -> Self {
        f64::from_bits(bits)
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

/// `parse_float/FloatStream.zig` — a wrapper over a byte-slice, providing useful
/// methods for parsing string floating point values.
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

/// `parse_float/common.zig` `Number(T)` for a `T` whose `mantissaType` is `u64`.
struct Number {
    exponent: i64,
    mantissa: u64,
    negative: bool,
    many_digits: bool,
    hex: bool,
}

/// `parse.zig` `tryParseDigits` — parse digits until a non-digit character is
/// found.
//
// PERF(port): the reference first consumes eight decimal digits at a time with
// `parse8Digits` (a SWAR trick). It computes the same recurrence as the scalar
// loop below, so it changes no result.
fn try_parse_digits(stream: &mut FloatStream, x: &mut u64, base: u8) {
    while let Some(digit) = stream.scan_digit(base) {
        *x = x
            .wrapping_mul(u64::from(base))
            .wrapping_add(u64::from(digit));
    }
}

/// `parse.zig` `min_n_digit_int`.
fn min_n_digit_int(digit_count: usize) -> u64 {
    let mut n: u64 = 1;
    let mut i: usize = 1;
    while i < digit_count {
        n *= 10;
        i += 1;
    }
    n
}

/// `parse.zig` `tryParseNDigits` — parse up to N digits.
fn try_parse_n_digits(stream: &mut FloatStream, x: &mut u64, base: u8, n: usize) {
    while *x < min_n_digit_int(n) {
        match stream.scan_digit(base) {
            Some(digit) => {
                *x = x
                    .wrapping_mul(u64::from(base))
                    .wrapping_add(u64::from(digit));
            }
            None => break,
        }
    }
}

/// `parse.zig` `parseScientific` — parse the scientific notation component of a
/// float.
fn parse_scientific(stream: &mut FloatStream) -> Option<i64> {
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

/// `parse.zig` `parsePartialNumberBase`.
fn parse_partial_number_base(
    stream: &mut FloatStream,
    negative: bool,
    n: &mut usize,
    base: u8,
    max_mantissa_digits: usize,
    exp_char_lower: u8,
) -> Option<Number> {
    debug_assert!(base == 10 || base == 16);

    // parse initial digits before dot
    let mut mantissa: u64 = 0;
    try_parse_digits(stream, &mut mantissa, base);
    let int_end = stream.offset_true();
    let mut n_digits = isize::try_from(stream.offset_true()).unwrap();

    // handle dot with the following digits
    let mut exponent: i64 = 0;
    if stream.first_is(b".") {
        stream.advance(1);
        let marker = stream.offset_true();
        try_parse_digits(stream, &mut mantissa, base);
        let n_after_dot = stream.offset_true() - marker;
        exponent = -i64::try_from(n_after_dot).unwrap();
        n_digits += isize::try_from(n_after_dot).unwrap();
    }

    // adjust required shift to offset mantissa for base-16 (2^4)
    if base == 16 {
        exponent *= 4;
    }

    if n_digits == 0 {
        return None;
    }

    // handle scientific format
    let mut exp_number: i64 = 0;
    if stream.first_is_lower(&[exp_char_lower]) {
        stream.advance(1);
        exp_number = parse_scientific(stream)?;
        exponent += exp_number;
    }

    let len = stream.offset; // length must be complete parsed length
    *n += len;

    if stream.underscore_count > 0 && !valid_underscores(stream.slice, base) {
        return None;
    }

    // common case with not many digits
    if n_digits <= isize::try_from(max_mantissa_digits).unwrap() {
        return Some(Number {
            exponent,
            mantissa,
            negative,
            many_digits: false,
            hex: base == 16,
        });
    }

    n_digits -= isize::try_from(max_mantissa_digits).unwrap();
    let mut many_digits = false;
    stream.reset(); // re-parse from beginning
    while stream.first_is(b"0._") {
        // '0' = '.' + 2
        let next = stream.slice[stream.offset];
        if next != b'_' {
            n_digits -= isize::from(next.saturating_sub(b'0' - 1));
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
        try_parse_n_digits(stream, &mut mantissa, base, max_mantissa_digits);

        exponent = if mantissa >= min_n_digit_int(max_mantissa_digits) {
            // big int
            i64::try_from(int_end).unwrap() - i64::try_from(stream.offset_true()).unwrap()
        } else {
            // the next byte must be present and be '.'
            // We know this is true because we had more than 19
            // digits previously, so we overflowed a 64-bit integer,
            // but parsing only the integral digits produced less
            // than 19 digits. That means we must have a decimal
            // point, and at least 1 fractional digit.
            stream.advance(1);
            let marker = stream.offset_true();
            try_parse_n_digits(stream, &mut mantissa, base, max_mantissa_digits);
            i64::try_from(marker).unwrap() - i64::try_from(stream.offset_true()).unwrap()
        };
        if base == 16 {
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
        hex: base == 16,
    })
}

/// `parse.zig` `parsePartialNumber` — parse a partial, non-special floating
/// point number.
///
/// This creates a representation of the float as the
/// significant digits and the decimal exponent.
fn parse_partial_number(s: &[u8], negative: bool, n: &mut usize) -> Option<Number> {
    debug_assert!(!s.is_empty());
    *n = 0;

    if s.len() >= 2 && s[0] == b'0' && s[1].eq_ignore_ascii_case(&b'x') {
        let mut stream = FloatStream::init(&s[2..]);
        *n += 2;
        // .max_mantissa_digits = if (MantissaT == u64) 16 else 32,
        parse_partial_number_base(&mut stream, negative, n, 16, 16, b'p')
    } else {
        let mut stream = FloatStream::init(s);
        // .max_mantissa_digits = if (MantissaT == u64) 19 else 38,
        parse_partial_number_base(&mut stream, negative, n, 10, 19, b'e')
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

/// `convert_hex.zig` `convertHex` — converts the form `0xMMM.NNNpEEE`.
///
/// MMM.NNN = mantissa
/// EEE = exponent
///
/// MMM.NNN is stored as an integer, the exponent is offset.
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
        if n.mantissa == 1u64 << (1 + fractional_bits) {
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
    // @as(MantissaT, @intCast((n.exponent - exp_bias) & ((1 << exp_bits) - 1))) << mantissa_bits
    bits |=
        u64::try_from((n.exponent - exp_bias) & ((1i64 << exp_bits) - 1)).unwrap() << mantissa_bits;
    if n.negative {
        bits |= 1u64 << (mantissa_bits + exp_bits);
    }
    T::from_bits_truncate(bits)
}

/// `std.fmt.parseFloat(T, s)`.
pub fn parse_float<T: ZigFloat>(s: &[u8]) -> Result<T, InvalidCharacter> {
    if s.is_empty() {
        return Err(InvalidCharacter);
    }

    let mut i: usize = 0;
    let negative = s[i] == b'-';
    if s[i] == b'-' || s[i] == b'+' {
        i += 1;
    }
    if s.len() == i {
        return Err(InvalidCharacter);
    }
    let rest = &s[i..];

    // const n = parse.parseNumber(T, s[i..], negative) orelse {
    //     return parse.parseInfOrNan(T, s[i..], negative) orelse error.InvalidCharacter;
    // };
    let n = match parse_number(rest, negative) {
        Some(n) => n,
        None => return parse_inf_or_nan::<T>(rest, negative).ok_or(InvalidCharacter),
    };

    // if (n.hex) return convertHex(T, n);
    if n.hex {
        return Ok(convert_hex::<T>(n));
    }

    // convertFast / convertEiselLemire / convertSlow, delegated (see the module
    // header). The reference grammar has accepted `rest` and `validUnderscores`
    // has proven every `_` sits between two digits, so dropping them is not a
    // relaxation; what remains is decimal syntax Rust's parser reads.
    let mut cleaned: Vec<u8> = Vec::with_capacity(s.len());
    if negative {
        cleaned.push(b'-');
    }
    cleaned.extend(rest.iter().copied().filter(|&c| c != b'_'));
    std::str::from_utf8(&cleaned)
        .ok()
        .and_then(|t| t.parse::<T>().ok())
        .ok_or(InvalidCharacter)
}

#[cfg(test)]
// The literals are the reference tests' own (`3.141`, `123456.789000e10`).
#[allow(clippy::approx_constant, clippy::excessive_precision)]
mod tests {
    use super::*;

    // The Zig 0.16.0 `test parseInt` block (std/fmt.zig), for the result types
    // the port supports (`i1`/`i2`/`i4`/`i5`/`i43` have no Rust counterpart).
    #[test]
    fn std_parse_int() {
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
    }

    // The Zig 0.16.0 `test parseUnsigned` underscore assertion.
    #[test]
    fn std_parse_unsigned_underscore() {
        assert_eq!(parse_int::<u16>(b"65_535", 10), Ok(65535));
    }

    // Inputs the port's former `from_str_radix` front end got wrong, each
    // MEASURED against Zig 0.16.0 `std.fmt.parseInt(T, s, 0)` (see the task
    // report): `+-5` was accepted, `-0` for an unsigned type was rejected, and
    // a doubled `_` is accepted by the reference.
    #[test]
    fn parse_int_sign_and_separator_edges() {
        assert!(parse_int::<i32>(b"+-5", 0).is_err());
        assert_eq!(parse_int::<u32>(b"-0", 0), Ok(0));
        assert_eq!(parse_int::<u32>(b"1__0", 0), Ok(10));
        assert_eq!(parse_int::<u64>(b"10_000_000", 0), Ok(10_000_000));
    }

    fn approx<T: Into<f64>>(a: T, b: f64) -> bool {
        (a.into() - b).abs() <= 1e-7_f64.max(b.abs() * 1e-7)
    }

    // The Zig 0.16.0 `test parseFloat` block, for f32 and f64.
    macro_rules! std_parse_float_for {
        ($name:ident, $t:ty) => {
            #[test]
            fn $name() {
                type T = $t;
                assert!(parse_float::<T>(b"").is_err());
                assert!(parse_float::<T>(b"   1").is_err());
                assert!(parse_float::<T>(b"1abc").is_err());
                assert!(parse_float::<T>(b"+").is_err());
                assert!(parse_float::<T>(b"-").is_err());

                assert_eq!(parse_float::<T>(b"0").unwrap(), 0.0);
                assert_eq!(parse_float::<T>(b"+0").unwrap(), 0.0);
                assert_eq!(parse_float::<T>(b"-0").unwrap(), 0.0);

                assert_eq!(parse_float::<T>(b"0e0").unwrap(), 0.0);
                assert_eq!(parse_float::<T>(b"2e3").unwrap(), 2000.0);
                assert_eq!(parse_float::<T>(b"1e0").unwrap(), 1.0);
                assert_eq!(parse_float::<T>(b"-2e3").unwrap(), -2000.0);
                assert_eq!(parse_float::<T>(b"-1e0").unwrap(), -1.0);
                assert_eq!(parse_float::<T>(b"1.234e3").unwrap(), 1234.0);

                assert!(approx(parse_float::<T>(b"3.141").unwrap(), 3.141));
                assert!(approx(parse_float::<T>(b"-3.141").unwrap(), -3.141));

                assert_eq!(parse_float::<T>(b"1e-5000").unwrap(), 0.0);
                assert_eq!(parse_float::<T>(b"1e+5000").unwrap(), T::INFINITY);

                assert_eq!(
                    parse_float::<T>(
                        b"0.4e0066999999999999999999999999999999999999999999999999999"
                    )
                    .unwrap(),
                    T::INFINITY
                );
                assert_eq!(
                    parse_float::<T>(b"0_1_2_3_4_5_6.7_8_9_0_0_0e0_0_1_0").unwrap(),
                    123456.789000e10 as T
                );

                // underscore rule is simple and reduces to "can only occur between two digits" and multiple are not supported.
                assert!(parse_float::<T>(b"0123456.789000e_0010").is_err()); // cannot occur immediately after exponent
                assert!(parse_float::<T>(b"_0123456.789000e0010").is_err()); // cannot occur before any digits
                assert!(parse_float::<T>(b"0__123456.789000e_0010").is_err()); // cannot occur twice in a row
                assert!(parse_float::<T>(b"0123456_.789000e0010").is_err()); // cannot occur before decimal point
                assert!(parse_float::<T>(b"0123456.789000e0010_").is_err()); // cannot occur at end of number

                assert!(approx(parse_float::<T>(b"1e-2").unwrap(), 0.01));
                assert!(approx(parse_float::<T>(b"1234e-2").unwrap(), 12.34));

                assert!(approx(parse_float::<T>(b"1.").unwrap(), 1.0));
                assert!(approx(parse_float::<T>(b"0.").unwrap(), 0.0));
                assert!(approx(parse_float::<T>(b".1").unwrap(), 0.1));
                assert!(approx(parse_float::<T>(b".0").unwrap(), 0.0));
                assert!(approx(parse_float::<T>(b".1e-1").unwrap(), 0.01));

                assert!(parse_float::<T>(b".").is_err()); // At least one digit is required.
                assert!(parse_float::<T>(b".e1").is_err()); // At least one digit is required.
                assert!(parse_float::<T>(b"0.e").is_err()); // At least one digit is required.

                assert!(approx(
                    parse_float::<T>(b"123142.1").unwrap(),
                    (123142.1 as T).into()
                ));
                assert!(approx(
                    parse_float::<T>(b"-123142.1124").unwrap(),
                    (-123142.1124 as T).into()
                ));
                assert!(approx(
                    parse_float::<T>(b"0.7062146892655368").unwrap(),
                    (0.7062146892655368 as T).into()
                ));
                assert!(approx(
                    parse_float::<T>(b"2.71828182845904523536").unwrap(),
                    (2.718281828459045 as T).into()
                ));

                // test "nan and inf"
                assert_eq!(
                    parse_float::<T>(b"nAn").unwrap().to_bits(),
                    T::NAN.to_bits()
                );
                assert_eq!(parse_float::<T>(b"inF").unwrap(), T::INFINITY);
                assert_eq!(parse_float::<T>(b"-INF").unwrap(), T::NEG_INFINITY);
            }
        };
    }
    std_parse_float_for!(std_parse_float_f32, f32);
    std_parse_float_for!(std_parse_float_f64, f64);

    // test "largest normals" (f32, f64 arms).
    #[test]
    fn std_largest_normals() {
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

    // test "hex.special"
    #[test]
    fn std_hex_special() {
        assert!(parse_float::<f32>(b"nAn").unwrap().is_nan());
        assert_eq!(parse_float::<f32>(b"iNf").unwrap(), f32::INFINITY);
        assert_eq!(parse_float::<f32>(b"+Inf").unwrap(), f32::INFINITY);
        assert_eq!(parse_float::<f32>(b"-iNf").unwrap(), f32::NEG_INFINITY);

        assert_eq!(parse_float::<f32>(b"0x9999p9999").unwrap(), f32::INFINITY);
        assert_eq!(
            parse_float::<f32>(b"-0x9999p9999").unwrap(),
            f32::NEG_INFINITY
        );
    }

    // test "hex.zero"
    #[test]
    fn std_hex_zero() {
        assert_eq!(parse_float::<f32>(b"0x0").unwrap(), 0.0);
        assert_eq!(parse_float::<f32>(b"-0x0").unwrap(), 0.0);
        assert_eq!(parse_float::<f32>(b"0x0p42").unwrap(), 0.0);
        assert_eq!(parse_float::<f32>(b"-0x0.00000p42").unwrap(), 0.0);
        assert_eq!(parse_float::<f32>(b"0x0.00000p666").unwrap(), 0.0);
    }

    // test "hex.f32"
    #[test]
    fn std_hex_f32() {
        assert!(parse_float::<f32>(b"0x").is_err());
        assert_eq!(parse_float::<f32>(b"0x1p0").unwrap(), 1.0);
        assert_eq!(parse_float::<f32>(b"-0x1p-1").unwrap(), -0.5);
        assert_eq!(parse_float::<f32>(b"0x10p+10").unwrap(), 16384.0);
        assert_eq!(parse_float::<f32>(b"0x10p-10").unwrap(), 0.015625);
        // 0x0.ffffffp128 == (2^24 - 1) * 2^104
        assert_eq!(
            parse_float::<f32>(b"0x0.ffffffp128").unwrap(),
            16_777_215.0_f32 * 2f32.powi(104)
        );
        // 0x0.1234570p-125 == 0x1234570 * 2^-153
        assert_eq!(
            parse_float::<f32>(b"0x0.1234570p-125").unwrap(),
            (0x0123_4570 as f64 * 2f64.powi(-153)) as f32
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
    fn std_hex_f64() {
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

    // test "many_digits hex": the f32 result equals the f128 result narrowed.
    // 0xffffffffffffffff.0p0 == 2^64 - 1, which narrows to 2^64 in f32.
    #[test]
    fn std_many_digits_hex() {
        assert_eq!(
            parse_float::<f32>(b"0xffffffffffffffff.0p0").unwrap(),
            2f32.powi(64)
        );
    }
}
