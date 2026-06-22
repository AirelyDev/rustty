//! vt — PORTS ghostty src/simd/vt.zig.

use crate::index_of::indexOf;
use rustty_util::quirks::inlineAssert as assert;

/// `options.simd` — whether SIMD-accelerated code paths are built. Mirrors the
/// reference's `build_options.simd`, which defaults to `true`.
const SIMD: bool = true;

// vt.cpp
extern "C" {
    fn ghostty_simd_decode_utf8_until_control_seq(
        input: *const u8,
        count: usize,
        output: *mut u32,
        output_count: *mut usize,
    ) -> usize;
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct DecodeResult {
    pub consumed: usize,
    pub decoded: usize,
}

#[allow(non_snake_case)]
pub fn utf8DecodeUntilControlSeq(input: &[u8], output: &mut [u32]) -> DecodeResult {
    assert(output.len() >= input.len());

    if SIMD {
        let mut decoded: usize = 0;
        let consumed = unsafe {
            ghostty_simd_decode_utf8_until_control_seq(
                input.as_ptr(),
                input.len(),
                output.as_mut_ptr(),
                &mut decoded,
            )
        };

        return DecodeResult { consumed, decoded };
    }

    utf8_decode_until_control_seq_scalar(input, output)
}

fn utf8_decode_until_control_seq_scalar(input: &[u8], output: &mut [u32]) -> DecodeResult {
    // Find our escape
    let idx = indexOf(input, 0x1B).unwrap_or(input.len());
    let decode = &input[0..idx];

    // Go through and decode one item at a time, following the W3C/Unicode
    // "U+FFFD Substitution of Maximal Subparts" algorithm for ill-formed
    // subsequences.
    let mut decode_offset: usize = 0;
    let mut decode_count: usize = 0;
    while decode_offset < decode.len() {
        let b0 = decode[decode_offset];

        // ASCII fast path
        if b0 < 0x80 {
            output[decode_count] = b0 as u32;
            decode_count += 1;
            decode_offset += 1;
            continue;
        }

        // Continuation byte (80-BF) or invalid byte (C0-C1, F5-FF)
        // as lead: each is its own maximal subpart → one FFFD per byte.
        // PARITY: kept as two comparisons to mirror the reference's
        // `b0 < 0xC2 or b0 > 0xF4`.
        #[allow(clippy::manual_range_contains)]
        if b0 < 0xC2 || b0 > 0xF4 {
            output[decode_count] = 0xFFFD;
            decode_count += 1;
            decode_offset += 1;
            continue;
        }

        // Multi-byte sequence. Determine expected length and the valid
        // range for each continuation byte per Unicode Table 3-7.
        let seq = utf8_seq_info(b0);

        // Check how many continuation bytes form a valid prefix (the
        // maximal subpart). We check each byte against its specific
        // valid range.
        let mut valid: usize = 1; // lead byte is valid
        for ci in 0..(seq.len as usize - 1) {
            if decode_offset + valid >= decode.len() {
                // Truncated at end of buffer: treat as incomplete
                // input that may be completed later. Stop decoding
                // without consuming these bytes.
                return DecodeResult {
                    consumed: decode_offset,
                    decoded: decode_count,
                };
            }
            let cb = decode[decode_offset + valid];
            if cb < seq.ranges[ci][0] || cb > seq.ranges[ci][1] {
                // Byte doesn't match expected range. The maximal
                // subpart ends here.
                break;
            }
            valid += 1;
        }

        if valid == seq.len as usize {
            // Full sequence present and structurally valid. Decode it.
            // (Structural validity per Table 3-7 guarantees decode success.)
            let cp_bytes = &decode[decode_offset..][0..seq.len as usize];
            match utf8_decode(cp_bytes) {
                Ok(cp) => {
                    output[decode_count] = cp;
                    decode_count += 1;
                    decode_offset += seq.len as usize;
                }
                Err(_) => {
                    // Should not happen given Table 3-7 validation, but
                    // be safe: emit FFFD for the lead byte.
                    output[decode_count] = 0xFFFD;
                    decode_count += 1;
                    decode_offset += 1;
                }
            }
        } else {
            // Incomplete/ill-formed: the maximal subpart (valid bytes)
            // maps to a single FFFD.
            output[decode_count] = 0xFFFD;
            decode_count += 1;
            decode_offset += valid;
        }
    }

    DecodeResult {
        consumed: decode_offset,
        decoded: decode_count,
    }
}

struct Utf8SeqInfo {
    len: u8, // u3
    ranges: [[u8; 2]; 3],
}

/// Returns the expected byte count and valid continuation byte ranges
/// for a UTF-8 sequence based on its lead byte, per Unicode Table 3-7.
fn utf8_seq_info(lead: u8) -> Utf8SeqInfo {
    match lead {
        0xC2..=0xDF => Utf8SeqInfo {
            len: 2,
            ranges: [[0x80, 0xBF], [0, 0], [0, 0]],
        },
        0xE0 => Utf8SeqInfo {
            len: 3,
            ranges: [[0xA0, 0xBF], [0x80, 0xBF], [0, 0]],
        },
        0xE1..=0xEC => Utf8SeqInfo {
            len: 3,
            ranges: [[0x80, 0xBF], [0x80, 0xBF], [0, 0]],
        },
        0xED => Utf8SeqInfo {
            len: 3,
            ranges: [[0x80, 0x9F], [0x80, 0xBF], [0, 0]],
        },
        0xEE..=0xEF => Utf8SeqInfo {
            len: 3,
            ranges: [[0x80, 0xBF], [0x80, 0xBF], [0, 0]],
        },
        0xF0 => Utf8SeqInfo {
            len: 4,
            ranges: [[0x90, 0xBF], [0x80, 0xBF], [0x80, 0xBF]],
        },
        0xF1..=0xF3 => Utf8SeqInfo {
            len: 4,
            ranges: [[0x80, 0xBF], [0x80, 0xBF], [0x80, 0xBF]],
        },
        0xF4 => Utf8SeqInfo {
            len: 4,
            ranges: [[0x80, 0x8F], [0x80, 0xBF], [0x80, 0xBF]],
        },
        _ => unreachable!(),
    }
}

/// `std.unicode.utf8Decode`: decode a structurally validated UTF-8 byte
/// sequence into its scalar value. Returns `Err` on any decode failure.
fn utf8_decode(bytes: &[u8]) -> Result<u32, ()> {
    std::str::from_utf8(bytes)
        .ok()
        .and_then(|s| s.chars().next())
        .map(|c| c as u32)
        .ok_or(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_no_escape() {
        let mut output: [u32; 1024] = [0; 1024];

        // TODO: many more test cases
        {
            let str = "hello".repeat(128);
            let str = str.as_bytes();
            assert_eq!(
                DecodeResult {
                    consumed: str.len(),
                    decoded: str.len(),
                },
                utf8DecodeUntilControlSeq(str, &mut output)
            );
        }
    }

    #[test]
    fn decode_ascii_to_escape() {
        let mut output: [u32; 1024] = [0; 1024];

        // TODO: many more test cases
        {
            let prefix = "hello".repeat(64);
            let str = format!("{}{}{}", prefix, "\x1b", "world".repeat(64));
            assert_eq!(
                DecodeResult {
                    consumed: prefix.len(),
                    decoded: prefix.len(),
                },
                utf8DecodeUntilControlSeq(str.as_bytes(), &mut output)
            );
        }
    }

    #[test]
    fn decode_immediate_esc_sequence() {
        let mut output: [u32; 64] = [0; 64];
        let str = b"\x1b[?5s";
        assert_eq!(
            DecodeResult {
                consumed: 0,
                decoded: 0,
            },
            utf8DecodeUntilControlSeq(str, &mut output)
        );
    }

    #[test]
    fn decode_incomplete_utf8() {
        let mut output: [u32; 64] = [0; 64];

        // 2-byte truncated at end of buffer
        {
            let str = b"hello\xc2";
            assert_eq!(
                DecodeResult {
                    consumed: 5,
                    decoded: 5,
                },
                utf8DecodeUntilControlSeq(str, &mut output)
            );
        }

        // 3-byte: \xe0 expects A0-BF next, but \x00 is not in range.
        // \xe0 is a maximal subpart of length 1 → FFFD, then \x00 is ASCII NUL.
        {
            let str = b"hello\xe0\x00";
            let result = utf8DecodeUntilControlSeq(str, &mut output);
            assert_eq!(7_usize, result.consumed);
            assert_eq!(7_usize, result.decoded);
            assert_eq!(0xFFFD_u32, output[5]);
            assert_eq!(0x00_u32, output[6]);
        }

        // 4-byte truncated at end of buffer (F0 90 is valid so far)
        {
            let str = b"hello\xf0\x90";
            assert_eq!(
                DecodeResult {
                    consumed: 5,
                    decoded: 5,
                },
                utf8DecodeUntilControlSeq(str, &mut output)
            );
        }
    }

    #[test]
    fn decode_invalid_utf8() {
        let mut output: [u32; 64] = [0; 64];

        // Invalid leading 2-byte sequence
        {
            let str = b"hello\xc2\x01";
            assert_eq!(
                DecodeResult {
                    consumed: 7,
                    decoded: 7,
                },
                utf8DecodeUntilControlSeq(str, &mut output)
            );
        }

        // Replacement will only replace the invalid leading byte.
        assert_eq!(0xFFFD_u32, output[5]);
        assert_eq!(0x01_u32, output[6]);
    }

    // Per the maximal subpart spec, bytes F5-FF are each replaced with FFFD.
    #[test]
    fn decode_invalid_leading_byte_is_replaced() {
        let mut output: [u32; 64] = [0; 64];

        {
            let str = b"hello\xFF";
            let result = utf8DecodeUntilControlSeq(str, &mut output);
            assert_eq!(6_usize, result.consumed);
            assert_eq!(6_usize, result.decoded);
            assert_eq!(0xFFFD_u32, output[5]);
        }
    }

    #[test]
    fn decode_invalid_continuation_in_3_byte_sequence() {
        let mut output: [u32; 64] = [0; 64];

        // \xe2 expects two continuation bytes, \x28 is not one
        {
            let str = b"hello\xe2\x28world";
            let result = utf8DecodeUntilControlSeq(str, &mut output);
            // "hello" + replacement + "(" + "world" = 12 codepoints
            assert_eq!(12_usize, result.decoded);
            assert_eq!(0xFFFD_u32, output[5]);
            assert_eq!('(' as u32, output[6]);
            assert_eq!('w' as u32, output[7]);
        }
    }

    #[test]
    fn decode_invalid_continuation_in_4_byte_sequence() {
        let mut output: [u32; 64] = [0; 64];

        // \xf0\x90 is a valid prefix of a 4-byte sequence, but \x28 breaks it.
        // Maximal subpart is F0 90 (length 2) → single FFFD, then '(' proceeds.
        {
            let str = b"hello\xf0\x90\x28world";
            let result = utf8DecodeUntilControlSeq(str, &mut output);
            // "hello" + FFFD + "(" + "world" = 12 codepoints
            assert_eq!(12_usize, result.decoded);
            assert_eq!(0xFFFD_u32, output[5]);
            assert_eq!('(' as u32, output[6]);
            assert_eq!('w' as u32, output[7]);
        }
    }

    #[test]
    fn decode_multiple_consecutive_invalid_bytes() {
        let mut output: [u32; 64] = [0; 64];

        // Each lone continuation byte is its own maximal subpart → one FFFD each.
        {
            let str = b"a\x80\x80b";
            let result = utf8DecodeUntilControlSeq(str, &mut output);
            // "a" + FFFD + FFFD + "b" = 4 codepoints
            assert_eq!(4_usize, result.decoded);
            assert_eq!('a' as u32, output[0]);
            assert_eq!(0xFFFD_u32, output[1]);
            assert_eq!(0xFFFD_u32, output[2]);
            assert_eq!('b' as u32, output[3]);
        }

        // C0 is an invalid lead byte (< C2), each byte gets its own FFFD.
        {
            let str = b"a\xc0\xc0b";
            let result = utf8DecodeUntilControlSeq(str, &mut output);
            // "a" + FFFD + FFFD + "b" = 4 codepoints
            assert_eq!(4_usize, result.decoded);
            assert_eq!('a' as u32, output[0]);
            assert_eq!(0xFFFD_u32, output[1]);
            assert_eq!(0xFFFD_u32, output[2]);
            assert_eq!('b' as u32, output[3]);
        }
    }

    #[test]
    fn decode_unexpected_continuation_byte_as_lead() {
        let mut output: [u32; 64] = [0; 64];

        // 0x80 is a continuation byte appearing as a lead byte
        {
            let str = b"a\x80b";
            let result = utf8DecodeUntilControlSeq(str, &mut output);
            // "a" + replacement + "b" = 3 codepoints
            assert_eq!(3_usize, result.decoded);
            assert_eq!('a' as u32, output[0]);
            assert_eq!(0xFFFD_u32, output[1]);
            assert_eq!('b' as u32, output[2]);
        }
    }

    #[test]
    fn decode_overlong_2_byte_encoding() {
        let mut output: [u32; 64] = [0; 64];

        // \xc0\xaf: C0 is invalid lead (< C2) → FFFD, AF is lone continuation → FFFD
        // Per Table 3-8: C0 AF → FFFD FFFD
        {
            let str = b"a\xc0\xafb";
            let result = utf8DecodeUntilControlSeq(str, &mut output);
            // "a" + FFFD + FFFD + "b" = 4 codepoints
            assert_eq!(4_usize, result.decoded);
            assert_eq!('a' as u32, output[0]);
            assert_eq!(0xFFFD_u32, output[1]);
            assert_eq!(0xFFFD_u32, output[2]);
            assert_eq!('b' as u32, output[3]);
        }
    }

    #[test]
    fn decode_surrogate_half() {
        let mut output: [u32; 64] = [0; 64];

        // \xed\xa0\x80 encodes U+D800 (a surrogate). Per Table 3-7, after ED
        // the next byte must be 80-9F. A0 is out of range, so ED is a maximal
        // subpart of length 1 → FFFD. Then A0 and 80 are lone continuations
        // → FFFD each. Per Table 3-9: ED A0 80 → FFFD FFFD FFFD
        {
            let str = b"a\xed\xa0\x80b";
            let result = utf8DecodeUntilControlSeq(str, &mut output);
            // "a" + FFFD + FFFD + FFFD + "b" = 5 codepoints
            assert_eq!(5_usize, result.decoded);
            assert_eq!('a' as u32, output[0]);
            assert_eq!(0xFFFD_u32, output[1]);
            assert_eq!(0xFFFD_u32, output[2]);
            assert_eq!(0xFFFD_u32, output[3]);
            assert_eq!('b' as u32, output[4]);
        }
    }

    #[test]
    fn decode_valid_multibyte_surrounded_by_invalid() {
        let mut output: [u32; 64] = [0; 64];

        // \xc3\xa9 = é (U+00E9), surrounded by invalid continuation bytes
        {
            let str = b"\x80\xc3\xa9\x80";
            let result = utf8DecodeUntilControlSeq(str, &mut output);
            // replacement + é + replacement = 3 codepoints
            assert_eq!(3_usize, result.decoded);
            assert_eq!(0xFFFD_u32, output[0]);
            assert_eq!(0x00E9_u32, output[1]);
            assert_eq!(0xFFFD_u32, output[2]);
        }
    }

    #[test]
    fn decode_invalid_byte_before_escape() {
        let mut output: [u32; 64] = [0; 64];

        // Invalid byte followed by ESC - should replace then stop
        {
            let str = b"hi\x80\x1b[0m";
            let result = utf8DecodeUntilControlSeq(str, &mut output);
            assert_eq!(3_usize, result.consumed);
            assert_eq!(3_usize, result.decoded);
            assert_eq!('h' as u32, output[0]);
            assert_eq!('i' as u32, output[1]);
            assert_eq!(0xFFFD_u32, output[2]);
        }
    }

    // Unicode Table 3-8: U+FFFD for Non-Shortest Form Sequences
    // Bytes:  C0  AF  E0  80  BF  F0  81  82  41
    // Output: FFFD FFFD FFFD FFFD FFFD FFFD FFFD FFFD 0041
    #[test]
    // PARITY: indexed loop mirrors the reference's `for (0..8) |i|`.
    #[allow(clippy::needless_range_loop)]
    fn table_3_8_non_shortest_form_sequences() {
        let mut output: [u32; 64] = [0; 64];

        let str = b"\xC0\xAF\xE0\x80\xBF\xF0\x81\x82\x41";
        let result = utf8DecodeUntilControlSeq(str, &mut output);
        assert_eq!(9_usize, result.consumed);
        assert_eq!(9_usize, result.decoded);
        for i in 0..8 {
            assert_eq!(0xFFFD_u32, output[i]);
        }
        assert_eq!(0x41_u32, output[8]);
    }

    // Unicode Table 3-9: U+FFFD for Ill-Formed Sequences for Surrogates
    // Bytes:  ED  A0  80  ED  BF  BF  ED  AF  41
    // Output: FFFD FFFD FFFD FFFD FFFD FFFD FFFD FFFD 0041
    #[test]
    // PARITY: indexed loop mirrors the reference's `for (0..8) |i|`.
    #[allow(clippy::needless_range_loop)]
    fn table_3_9_surrogate_sequences() {
        let mut output: [u32; 64] = [0; 64];

        let str = b"\xED\xA0\x80\xED\xBF\xBF\xED\xAF\x41";
        let result = utf8DecodeUntilControlSeq(str, &mut output);
        assert_eq!(9_usize, result.consumed);
        assert_eq!(9_usize, result.decoded);
        for i in 0..8 {
            assert_eq!(0xFFFD_u32, output[i]);
        }
        assert_eq!(0x41_u32, output[8]);
    }

    // Unicode Table 3-10: U+FFFD for Other Ill-Formed Sequences
    // Bytes:  F4  91  92  93  FF  41  80  BF  42
    // Output: FFFD FFFD FFFD FFFD FFFD 0041 FFFD FFFD 0042
    #[test]
    fn table_3_10_other_ill_formed_sequences() {
        let mut output: [u32; 64] = [0; 64];

        let str = b"\xF4\x91\x92\x93\xFF\x41\x80\xBF\x42";
        let result = utf8DecodeUntilControlSeq(str, &mut output);
        assert_eq!(9_usize, result.consumed);
        assert_eq!(9_usize, result.decoded);
        assert_eq!(0xFFFD_u32, output[0]); // F4
        assert_eq!(0xFFFD_u32, output[1]); // 91
        assert_eq!(0xFFFD_u32, output[2]); // 92
        assert_eq!(0xFFFD_u32, output[3]); // 93
        assert_eq!(0xFFFD_u32, output[4]); // FF
        assert_eq!(0x0041_u32, output[5]); // 41
        assert_eq!(0xFFFD_u32, output[6]); // 80
        assert_eq!(0xFFFD_u32, output[7]); // BF
        assert_eq!(0x0042_u32, output[8]); // 42
    }

    // Unicode Table 3-11: U+FFFD for Truncated Sequences
    // Bytes:  E1  80  E2  F0  91  92  F1  BF  41
    // Output: FFFD     FFFD    FFFD         FFFD     0041
    #[test]
    fn table_3_11_truncated_sequences() {
        let mut output: [u32; 64] = [0; 64];

        let str = b"\xE1\x80\xE2\xF0\x91\x92\xF1\xBF\x41";
        let result = utf8DecodeUntilControlSeq(str, &mut output);
        assert_eq!(9_usize, result.consumed);
        assert_eq!(5_usize, result.decoded);
        assert_eq!(0xFFFD_u32, output[0]); // E1 80 (truncated 3-byte)
        assert_eq!(0xFFFD_u32, output[1]); // E2 (truncated 3-byte, next byte F0 not continuation)
        assert_eq!(0xFFFD_u32, output[2]); // F0 91 92 (truncated 4-byte)
        assert_eq!(0xFFFD_u32, output[3]); // F1 BF (truncated 4-byte, next byte 41 not continuation)
        assert_eq!(0x0041_u32, output[4]); // 41
    }
}
