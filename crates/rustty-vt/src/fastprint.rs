//! Fastprint has fast printing routines that are significantly
//! faster than going through std.fmt.

use crate::page::u21;

/// Two-ASCII-digit rendering of a value in `0..=99`, zero-padded,
/// matching `std.fmt.digits2`.
fn digits2(value: u8) -> [u8; 2] {
    debug_assert!(value <= 99);
    [b'0' + value / 10, b'0' + value % 10]
}

/// Print a decimal type T. The buffer is expected to be large enough so if
/// necessary use comptime with a buffer-too-large to determine your
/// max size needed. Returns the length written.
///
/// PORT NOTE: Zig's `printDecimal(comptime T, buf, v)` switches on the
/// comptime type and only supports the types we need (`else => comptime
/// unreachable`). Here that comptime dispatch is the `PrintDecimal` trait,
/// implemented for exactly `u8` and `u21`; `T` is inferred from `v`.
pub fn print_decimal<T: PrintDecimal>(buf: &mut [u8], v: T) -> usize {
    // Note this only supports types as we need them.
    v.print_decimal(buf)
}

/// Backing trait for [`print_decimal`]: the reference's comptime `switch (T)`
/// arms, one impl per supported type.
pub trait PrintDecimal: Copy {
    fn print_decimal(self, buf: &mut [u8]) -> usize;
}

impl PrintDecimal for u8 {
    fn print_decimal(self, buf: &mut [u8]) -> usize {
        let v = self;
        if v >= 100 {
            buf[0] = b'0' + v / 100;
            buf[1..3].copy_from_slice(&digits2(v % 100));
            return 3;
        }
        if v >= 10 {
            buf[0..2].copy_from_slice(&digits2(v));
            return 2;
        }
        buf[0] = b'0' + v;
        1
    }
}

// This can probably be generalized...
impl PrintDecimal for u21 {
    fn print_decimal(self, buf: &mut [u8]) -> usize {
        // Maximum u21 is 2097151: at most 7 digits.
        let mut tmp: [u8; 7] = [0; 7];
        let mut val: u32 = self;
        let mut i: usize = tmp.len();
        loop {
            i -= 1;
            tmp[i] = b'0' + (val % 10) as u8;
            val /= 10;
            if val == 0 {
                break;
            }
        }
        let n = tmp.len() - i;
        buf[0..n].copy_from_slice(&tmp[i..]);
        n
    }
}

#[cfg(test)]
mod test {
    use super::print_decimal;
    use crate::page::u21;

    /// `std.math.maxInt(u21)` — the largest value a `u21` codepoint can hold.
    const U21_MAX: u21 = 0x1F_FFFF;

    #[test]
    fn print_decimal_() {
        let mut buf: [u8; 16] = [0; 16];

        // u8: exercise 1, 2, and 3 digit values including boundaries.
        let u8_cases: [u8; 7] = [0, 1, 9, 10, 99, 100, 255];
        for v in u8_cases {
            let expected = format!("{}", v);
            let len = print_decimal(&mut buf, v);
            assert_eq!(&buf[0..len], expected.as_bytes());
        }

        // u21: exercise digit count boundaries up to the maximum.
        let u21_cases: [u21; 7] = [0, 9, 10, 128, 65535, 1114111, U21_MAX];
        for v in u21_cases {
            let expected = format!("{}", v);
            let len = print_decimal(&mut buf, v);
            assert_eq!(&buf[0..len], expected.as_bytes());
        }
    }
}
