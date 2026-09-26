//! base64_scalar — PORTS ghostty src/simd/base64_scalar.zig.
//!
//! Pure-Rust scalar base64 decoder used as the fallback when SIMD is disabled.

use rustty_util::quirks::inlineAssert as assert;
use std::sync::LazyLock;

// PORT NOTE: the reference declares `scalar_decoder` as a comptime `const` whose
// `init` runs `assert`s at compile time. Rust's const-fn cannot call the
// (non-const) `quirks.inlineAssert`, so this is a lazily-initialized singleton
// instead; `init` runs identically (same asserts) on first access. The value is
// otherwise identical.
#[allow(non_upper_case_globals)]
pub static scalar_decoder: LazyLock<Base64Decoder> =
    LazyLock::new(|| Base64Decoder::init(std_base64_standard_alphabet_chars(), None));

/// Zig's `std.base64.standard_alphabet_chars`: the 64-character standard
/// base64 alphabet (`A-Za-z0-9+/`).
const fn std_base64_standard_alphabet_chars() -> [u8; 64] {
    *b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
}

/// Whether c is one of the 64 standard base64 alphabet characters
/// (padding is not part of the alphabet).
pub fn is_alphabet_char(c: u8) -> bool {
    scalar_decoder.char_to_index[c as usize] != Base64Decoder::INVALID_CHAR
}

/// Decode error mirroring Zig's `error{InvalidCharacter, InvalidPadding, NoSpaceLeft}`.
// PARITY: `NoSpaceLeft` is part of the reference's error set (R1: every error
// case preserved) even though the decode paths below never construct it.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[allow(clippy::enum_variant_names)]
#[allow(dead_code)]
pub enum Error {
    InvalidCharacter,
    InvalidPadding,
    NoSpaceLeft,
}

/// Copied from Zig 0.14.1 stdlib and commented out the invalid padding
/// scenarios, because Kitty Graphics requires a decoder that doesn't care
/// about invalid padding scenarios.
pub struct Base64Decoder {
    // const invalid_char: u8 = 0xff;
    // const invalid_char_tst: u32 = 0xff000000;

    // const Error = error{
    //     InvalidCharacter,
    //     InvalidPadding,
    //     NoSpaceLeft,
    // };
    /// e.g. 'A' => 0.
    /// `invalid_char` for any value not in the 64 alphabet chars.
    char_to_index: [u8; 256],
    fast_char_to_index: [[u32; 256]; 4],
    pad_char: Option<u8>,
}

impl Base64Decoder {
    const INVALID_CHAR: u8 = 0xff;
    const INVALID_CHAR_TST: u32 = 0xff000000;

    pub fn init(alphabet_chars: [u8; 64], pad_char: Option<u8>) -> Base64Decoder {
        let mut result = Base64Decoder {
            char_to_index: [Self::INVALID_CHAR; 256],
            fast_char_to_index: [[Self::INVALID_CHAR_TST; 256]; 4],
            pad_char,
        };

        let mut char_in_alphabet = [false; 256];
        let mut i = 0;
        while i < alphabet_chars.len() {
            let c = alphabet_chars[i];
            assert(!char_in_alphabet[c as usize]);
            assert(match pad_char {
                Some(p) => c != p,
                None => true,
            });

            let ci = i as u32;
            result.fast_char_to_index[0][c as usize] = ci << 2;
            result.fast_char_to_index[1][c as usize] = (ci >> 4) | ((ci & 0x0f) << 12);
            result.fast_char_to_index[2][c as usize] = ((ci & 0x3) << 22) | ((ci & 0x3c) << 6);
            result.fast_char_to_index[3][c as usize] = ci << 16;

            result.char_to_index[c as usize] = i as u8;
            char_in_alphabet[c as usize] = true;
            i += 1;
        }
        result
    }

    /// Return the maximum possible decoded size for a given input length - The actual length may be less if the input includes padding.
    /// `InvalidPadding` is returned if the input length is not valid.
    // PARITY: `% 4 != 0` / `% 4 == 1` mirror the reference's modulo checks.
    #[allow(clippy::manual_is_multiple_of)]
    pub fn calc_size_upper_bound(&self, source_len: usize) -> Result<usize, Error> {
        let mut result = source_len / 4 * 3;
        let leftover = source_len % 4;
        if self.pad_char.is_some() {
            if leftover % 4 != 0 {
                return Err(Error::InvalidPadding);
            }
        } else {
            if leftover % 4 == 1 {
                return Err(Error::InvalidPadding);
            }
            result += leftover * 3 / 4;
        }
        Ok(result)
    }

    /// Return the exact decoded size for a slice.
    /// `InvalidPadding` is returned if the input length is not valid.
    pub fn calc_size_for_slice(&self, source: &[u8]) -> Result<usize, Error> {
        let source_len = source.len();
        let mut result = self.calc_size_upper_bound(source_len)?;
        if let Some(pad_char) = self.pad_char {
            if source_len >= 1 && source[source_len - 1] == pad_char {
                result -= 1;
            }
            if source_len >= 2 && source[source_len - 2] == pad_char {
                result -= 1;
            }
        }
        Ok(result)
    }

    /// dest.len must be what you get from ::calcSize.
    /// Invalid characters result in `error.InvalidCharacter`.
    /// Invalid padding results in `error.InvalidPadding`.
    pub fn decode(&self, dest: &mut [u8], source: &[u8]) -> Result<(), Error> {
        // SAFETY: `dest` and `source` are valid for their lengths, and a `&mut`
        // and a `&` slice never overlap.
        unsafe { self.decode_raw(dest.as_mut_ptr(), dest.len(), source.as_ptr(), source.len()) }
    }

    /// `decode(buf[0..dest_len], buf[0..source_len])`: decode `buf` on top of
    /// itself, as the reference's Kitty graphics payload decode does.
    ///
    /// Rust-only glue (parity tail S7-4b): the scalar lane of
    /// `simd::base64::decode_in_place`. It is safe because every step reads its
    /// source bytes before it writes, and each write ends before the next read
    /// starts: the 16-byte step reads `[k, k+16)` and writes `[3k/4, 3k/4+16)`,
    /// the 4-byte step reads `[k, k+4)` and writes `[3k/4, 3k/4+4)`, and the
    /// tail writes byte `3k/4` after reading byte `k`.
    pub fn decode_in_place(
        &self,
        buf: &mut [u8],
        dest_len: usize,
        source_len: usize,
    ) -> Result<(), Error> {
        assert(dest_len <= buf.len() && source_len <= buf.len());
        let p = buf.as_mut_ptr();
        // SAFETY: `p` is valid for `buf.len()` reads and writes; the source and
        // destination are both derived from it (one tag), and the write order
        // above never overwrites a byte before it is read.
        unsafe { self.decode_raw(p, dest_len, p.cast_const(), source_len) }
    }

    /// The decoder over raw pointers, so the source and destination may be the
    /// same buffer (`decode_in_place`).
    ///
    /// # Safety
    /// `dest` is valid for `dest_len` writes and `source` for `source_len`
    /// reads. If they overlap, `dest` must not start after `source`.
    // PARITY: `source_len % 4 != 0` mirrors the reference's modulo check.
    #[allow(clippy::manual_is_multiple_of)]
    unsafe fn decode_raw(
        &self,
        dest: *mut u8,
        dest_len: usize,
        source: *const u8,
        source_len: usize,
    ) -> Result<(), Error> {
        // SAFETY (all reads and writes below): indices are bounded by
        // `source_len`/`dest_len` exactly as the slice version's were.
        let src = |i: usize| -> u8 {
            debug_assert!(i < source_len);
            unsafe { *source.add(i) }
        };
        if self.pad_char.is_some() && source_len % 4 != 0 {
            return Err(Error::InvalidPadding);
        }
        let mut dest_idx: usize = 0;
        let mut fast_src_idx: usize = 0;
        let mut acc: u16 = 0; // u12
        let mut acc_len: u8 = 0; // u4
        let mut leftover_idx: Option<usize> = None;
        while fast_src_idx + 16 < source_len && dest_idx + 15 < dest_len {
            let mut bits: u128 = 0;
            // inline for (0..4) |i|
            let mut i = 0;
            while i < 4 {
                let mut new_bits: u128 =
                    self.fast_char_to_index[0][src(fast_src_idx + i * 4) as usize] as u128;
                new_bits |=
                    self.fast_char_to_index[1][src(fast_src_idx + 1 + i * 4) as usize] as u128;
                new_bits |=
                    self.fast_char_to_index[2][src(fast_src_idx + 2 + i * 4) as usize] as u128;
                new_bits |=
                    self.fast_char_to_index[3][src(fast_src_idx + 3 + i * 4) as usize] as u128;
                if (new_bits & Self::INVALID_CHAR_TST as u128) != 0 {
                    return Err(Error::InvalidCharacter);
                }
                bits |= new_bits << (24 * i);
                i += 1;
            }
            // dest[dest_idx..][0..16].* = @bitCast(bits);
            unsafe {
                core::ptr::copy_nonoverlapping(bits.to_le_bytes().as_ptr(), dest.add(dest_idx), 16)
            };

            fast_src_idx += 16;
            dest_idx += 12;
        }
        while fast_src_idx + 4 < source_len && dest_idx + 3 < dest_len {
            let mut bits = self.fast_char_to_index[0][src(fast_src_idx) as usize];
            bits |= self.fast_char_to_index[1][src(fast_src_idx + 1) as usize];
            bits |= self.fast_char_to_index[2][src(fast_src_idx + 2) as usize];
            bits |= self.fast_char_to_index[3][src(fast_src_idx + 3) as usize];
            if (bits & Self::INVALID_CHAR_TST) != 0 {
                return Err(Error::InvalidCharacter);
            }
            unsafe {
                core::ptr::copy_nonoverlapping(bits.to_le_bytes().as_ptr(), dest.add(dest_idx), 4)
            };

            fast_src_idx += 4;
            dest_idx += 3;
        }
        let mut src_idx = fast_src_idx;
        while src_idx < source_len {
            let c = src(src_idx);
            let d = self.char_to_index[c as usize];
            if d == Self::INVALID_CHAR {
                if self.pad_char.is_none() || Some(c) != self.pad_char {
                    return Err(Error::InvalidCharacter);
                }
                leftover_idx = Some(src_idx);
                break;
            }
            acc = (acc << 6) + d as u16;
            acc_len += 6;
            if acc_len >= 8 {
                acc_len -= 8;
                debug_assert!(dest_idx < dest_len);
                unsafe { *dest.add(dest_idx) = (acc >> acc_len) as u8 };
                dest_idx += 1;
            }
            src_idx += 1;
        }
        // if (acc_len > 4 or (acc & (@as(u12, 1) << acc_len) - 1) != 0) {
        //     return error.InvalidPadding;
        // }
        let Some(leftover_idx) = leftover_idx else {
            return Ok(());
        };
        if let Some(pad_char) = self.pad_char {
            let padding_len = acc_len / 2;
            let mut padding_chars: usize = 0;
            for i in leftover_idx..source_len {
                let c = src(i);
                if c != pad_char {
                    return Err(if c == Base64Decoder::INVALID_CHAR {
                        Error::InvalidCharacter
                    } else {
                        Error::InvalidPadding
                    });
                }
                padding_chars += 1;
            }
            if padding_chars != padding_len as usize {
                return Err(Error::InvalidPadding);
            }
        }
        Ok(())
    }
}
