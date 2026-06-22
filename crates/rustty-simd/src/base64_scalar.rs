//! base64_scalar — PORTS ghostty src/simd/base64_scalar.zig.
//!
//! Pure-Rust scalar base64 decoder used as the fallback when SIMD is disabled.

use rustty_util::quirks::inlineAssert as assert;
use std::sync::LazyLock;

// PORT: the reference declares `scalar_decoder` as a comptime `const` whose
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
    // PARITY: `source.len() % 4 != 0` mirrors the reference's modulo check.
    #[allow(clippy::manual_is_multiple_of)]
    pub fn decode(&self, dest: &mut [u8], source: &[u8]) -> Result<(), Error> {
        if self.pad_char.is_some() && source.len() % 4 != 0 {
            return Err(Error::InvalidPadding);
        }
        let mut dest_idx: usize = 0;
        let mut fast_src_idx: usize = 0;
        let mut acc: u16 = 0; // u12
        let mut acc_len: u8 = 0; // u4
        let mut leftover_idx: Option<usize> = None;
        while fast_src_idx + 16 < source.len() && dest_idx + 15 < dest.len() {
            let mut bits: u128 = 0;
            // inline for (0..4) |i|
            let mut i = 0;
            while i < 4 {
                let mut new_bits: u128 =
                    self.fast_char_to_index[0][source[fast_src_idx + i * 4] as usize] as u128;
                new_bits |=
                    self.fast_char_to_index[1][source[fast_src_idx + 1 + i * 4] as usize] as u128;
                new_bits |=
                    self.fast_char_to_index[2][source[fast_src_idx + 2 + i * 4] as usize] as u128;
                new_bits |=
                    self.fast_char_to_index[3][source[fast_src_idx + 3 + i * 4] as usize] as u128;
                if (new_bits & Self::INVALID_CHAR_TST as u128) != 0 {
                    return Err(Error::InvalidCharacter);
                }
                bits |= new_bits << (24 * i);
                i += 1;
            }
            dest[dest_idx..][0..16].copy_from_slice(&bits.to_le_bytes());

            fast_src_idx += 16;
            dest_idx += 12;
        }
        while fast_src_idx + 4 < source.len() && dest_idx + 3 < dest.len() {
            let mut bits = self.fast_char_to_index[0][source[fast_src_idx] as usize];
            bits |= self.fast_char_to_index[1][source[fast_src_idx + 1] as usize];
            bits |= self.fast_char_to_index[2][source[fast_src_idx + 2] as usize];
            bits |= self.fast_char_to_index[3][source[fast_src_idx + 3] as usize];
            if (bits & Self::INVALID_CHAR_TST) != 0 {
                return Err(Error::InvalidCharacter);
            }
            dest[dest_idx..][0..4].copy_from_slice(&bits.to_le_bytes());

            fast_src_idx += 4;
            dest_idx += 3;
        }
        let remaining = &source[fast_src_idx..];
        for (offset, &c) in remaining.iter().enumerate() {
            let src_idx = fast_src_idx + offset;
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
                dest[dest_idx] = (acc >> acc_len) as u8;
                dest_idx += 1;
            }
        }
        // if (acc_len > 4 or (acc & (@as(u12, 1) << acc_len) - 1) != 0) {
        //     return error.InvalidPadding;
        // }
        if leftover_idx.is_none() {
            return Ok(());
        }
        let leftover = &source[leftover_idx.unwrap()..];
        if let Some(pad_char) = self.pad_char {
            let padding_len = acc_len / 2;
            let mut padding_chars: usize = 0;
            for &c in leftover {
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
