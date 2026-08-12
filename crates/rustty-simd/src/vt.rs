//! vt — PORTS ghostty src/simd/vt.zig.

use crate::index_of::indexOf;
use rustty_util::quirks::inlineAssert as assert;

/// `options.simd` — whether SIMD-accelerated code paths are built. Mirrors the
/// reference's `build_options.simd`, which defaults to `true`.
// WASM DIVERGENCE: on `wasm32` the SIMD branch is gated OFF (no C++ kernel on
// freestanding wasm); the scalar maximal-subpart decoder below is always taken.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
const SIMD: bool = true;

// vt.cpp — gated OFF on `wasm32` (no C++ kernel on freestanding wasm).
#[cfg(not(target_arch = "wasm32"))]
extern "C" {
    fn rustty_simd_decode_utf8_until_control_seq(
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

// PORT NOTE (Miri): Miri cannot execute the C/C++ side of the FFI ("can't call
// foreign function"), so under Miri this entry point takes the scalar lane
// instead — the exact code the reference builds when `build_options.simd` is
// off, with identical output by construction. `cfg!(miri)` is const-false in
// every real build, so the SIMD lane is untouched everywhere else (in-repo
// precedent: `base64.rs`'s two entry points and the `cfg(miri)` std-heap
// allocator fallback in rustty-vt `pagelist/mod.rs`). The SIMD lane itself
// stays covered by this crate's ASan G6 route and `cargo test`. This became
// load-bearing when the shipping tiers flipped `simd` on by default
// (codex-vet W16-TARGET-01): `Stream::next_slice`'s SIMD arm reaches here from
// the rustty-vt/-vt-c/-core/-apprt-c Miri lanes.
#[allow(non_snake_case)]
pub fn utf8DecodeUntilControlSeq(input: &[u8], output: &mut [u32]) -> DecodeResult {
    assert(output.len() >= input.len());

    #[cfg(not(target_arch = "wasm32"))]
    if SIMD && !cfg!(miri) {
        let mut decoded: usize = 0;
        let consumed = unsafe {
            rustty_simd_decode_utf8_until_control_seq(
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
                // The sequence is cut off by the end of the decode
                // region. If the region ends at the true end of the
                // input then it may be completed by future input, so
                // stop without consuming these bytes. If the region
                // was bounded by an ESC then the sequence can never
                // be completed; the valid-so-far prefix is a maximal
                // subpart which maps to a single U+FFFD below.
                if decode.len() == input.len() {
                    return DecodeResult {
                        consumed: decode_offset,
                        decoded: decode_count,
                    };
                }
                break;
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
    // The "Table 3-N: ..." tests below mirror the reference test names
    // verbatim (Unicode 15.0 §3.9 Tables 3-8..3-11); Rust identifiers cannot
    // hold the spaces/colons, so the reference name is carried as the closest
    // valid slug, which keeps the capital `Table` from the reference name.
    #![allow(non_snake_case)]
    use super::*;

    // Regression: the SIMD kernel must never read past `input.len()`, even when
    // the input lives in a TIGHT allocation (no page/sentinel slack). Under
    // AddressSanitizer this fails loudly if any LoadU/IndexOf chunk over-reads.
    //
    // The reference's Zig string literals are sentinel-terminated (`[N:0]u8`),
    // so the reference's callers can pass a count that reads an in-bounds 0x00;
    // Rust byte-string literals carry no such slack, so we exercise exact-sized
    // heap buffers here to prove the kernel stays in bounds without it.
    #[test]
    fn decode_tight_buffer_no_overread() {
        for s in [
            &b"A"[..],
            &b"hello\xf0\x90"[..], // 4-byte sequence truncated at end of buffer
            &b"hello\xc2"[..],     // 2-byte sequence truncated at end of buffer
            &b"\x1b[48;2;10;20;30mA"[..], // ESC scan then a tight 1-byte tail
            &b"world"[..],
            &b"\xff"[..],     // lone invalid lead byte
            &b"\xe2\x28"[..], // ill-formed 3-byte prefix
        ] {
            // Exact-sized heap buffer: ASan red-zones surround it, so any read
            // past `input.len()` is caught.
            let input: Vec<u8> = s.to_vec();
            let mut output = vec![0u32; input.len() + 64];
            let _ = utf8DecodeUntilControlSeq(&input, &mut output);
        }
    }

    // rustty-only regression for `083d9709b` (WS10 T15). The ASCII fast path
    // widens and stores a WHOLE vector chunk unconditionally —
    //
    //     WidenAsciiStore(d, input_vec, output + i);
    //     if (stop < 0) continue;
    //
    // — writing N `uint32`s even when the run stops mid-chunk, on the stated
    // contract that "output is guaranteed to be at least as large as input"
    // (asserted here by `utf8DecodeUntilControlSeq`'s
    // `assert(output.len() >= input.len())`). That is a speculative store the
    // pre-commit kernel never made: before this commit `DecodeUTF8` only ever
    // wrote one codepoint per byte it had already consumed.
    //
    // `decode_tight_buffer_no_overread` above cannot see the new hazard: it
    // sizes the output at `input.len() + 64`, i.e. with 64 words of slack, so a
    // chunk overwrite would land in slack. This test sizes the output at
    // EXACTLY `input.len()` — the documented minimum — so under ASan a
    // red-zoned heap allocation catches any write past the contract. Lengths
    // sweep 0..=257 to cross every plausible vector width (16/32/64 lanes) and
    // every mid-chunk stop position, with the stop byte placed at each offset
    // in turn, as both an ESC and a non-ASCII lead.
    #[test]
    fn decode_ascii_fast_path_tight_output_no_overwrite() {
        for len in 0..=257usize {
            for stop_at in 0..=len {
                for stop_byte in [0x1Bu8, 0xC3u8] {
                    let mut input = vec![b'a'; len];
                    if stop_at < len {
                        input[stop_at] = stop_byte;
                    }
                    // Exactly `input.len()` words — the contract minimum.
                    let mut output = vec![0u32; input.len()];
                    let res = utf8DecodeUntilControlSeq(&input, &mut output);
                    assert!(res.consumed <= input.len());
                    assert!(res.decoded <= output.len());
                }
            }
        }
    }

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
    fn decode_incomplete_utf_8() {
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
    fn decode_invalid_utf_8() {
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
    fn decode_partial_utf8_before_escape() {
        // A valid-so-far but incomplete sequence cut off by an ESC can
        // never be completed, so it is consumed and replaced by a single
        // U+FFFD (maximal subpart) rather than left pending. Only
        // sequences cut off by the true end of input are left pending.
        let mut output: [u32; 64] = [0; 64];

        // 2-byte lead cut off by ESC.
        {
            let str = b"hi\xc2\x1b[0m";
            let result = utf8DecodeUntilControlSeq(str, &mut output);
            assert_eq!(3_usize, result.consumed);
            assert_eq!(3_usize, result.decoded);
            assert_eq!(0xFFFD_u32, output[2]);
        }

        // 3-byte lead plus one valid continuation cut off by ESC:
        // the whole prefix is one maximal subpart, one U+FFFD.
        {
            let str = b"\xe0\xa0\x1bX";
            let result = utf8DecodeUntilControlSeq(str, &mut output);
            assert_eq!(2_usize, result.consumed);
            assert_eq!(1_usize, result.decoded);
            assert_eq!(0xFFFD_u32, output[0]);
        }
    }

    // Same two cases driven through the SCALAR decoder directly. This is the
    // arm that actually covers `cb4c49fbf`: on every non-wasm target the public
    // `utf8DecodeUntilControlSeq` takes the C++/simdutf kernel, so the test
    // above never reaches the scalar code the fix edits. Before the fix both
    // cases returned `consumed = 2` / `consumed = 0` with the trailing bytes
    // left pending, diverging from the SIMD path.
    #[test]
    fn decode_partial_utf8_before_escape_scalar() {
        let mut output: [u32; 64] = [0; 64];

        {
            let str = b"hi\xc2\x1b[0m";
            let result = utf8_decode_until_control_seq_scalar(str, &mut output);
            assert_eq!(3_usize, result.consumed);
            assert_eq!(3_usize, result.decoded);
            assert_eq!(0xFFFD_u32, output[2]);
        }

        {
            let str = b"\xe0\xa0\x1bX";
            let result = utf8_decode_until_control_seq_scalar(str, &mut output);
            assert_eq!(2_usize, result.consumed);
            assert_eq!(1_usize, result.decoded);
            assert_eq!(0xFFFD_u32, output[0]);
        }

        // Control: truncation at the TRUE end of input must still leave the
        // bytes pending (this is the half of the branch the fix preserves).
        {
            let str = b"hi\xc2";
            let result = utf8_decode_until_control_seq_scalar(str, &mut output);
            assert_eq!(2_usize, result.consumed);
            assert_eq!(2_usize, result.decoded);
        }
    }

    // Differential test: the SIMD implementation must agree with the
    // scalar implementation on any input. Exercises random mixtures of
    // ASCII, escapes, controls, valid and invalid UTF-8, at various
    // lengths (including chunk-boundary straddling cases).
    //
    // PORT NOTE (divergence, deliberate): the reference drives this fuzz with
    // `std.Random.DefaultPrng` (Xoshiro256++) seeded `0xf00dface`. Reproducing
    // that bit-for-bit would mean duplicating ~80 lines of `std.Random` into
    // this near-leaf crate (rustty-synthetic has the faithful port, but making
    // rustty-simd dev-depend on it inverts the crate layering). The corpus
    // bytes therefore differ from the reference's; the PROPERTY asserted --
    // SIMD and scalar agree on consumed, decoded, and every decoded codepoint
    // over 10k random mixtures -- is identical, and the seed is fixed so any
    // failure is reproducible. This test is also the FIRST coverage the scalar
    // decoder has ever had on a non-wasm target.
    //
    // Mirrors the reference's `if (comptime !options.simd) return SkipZigTest`:
    // on wasm the public entry point IS the scalar decoder, so the comparison
    // would be vacuous.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn decode_simd_matches_scalar() {
        // SplitMix64 -- adequate for a differential corpus, and the same
        // seeding primitive Zig's DefaultPrng uses.
        struct Rng(u64);
        impl Rng {
            fn next(&mut self) -> u64 {
                self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
                let mut z = self.0;
                z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
                z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
                z ^ (z >> 31)
            }
            /// Inclusive range, mirroring `std.Random.intRangeAtMost`.
            fn range(&mut self, min: u64, max: u64) -> u64 {
                min + self.next() % (max - min + 1)
            }
        }

        let mut rng = Rng(0xf00d_face);
        let mut input = [0u8; 257];
        let mut out_simd = [0u32; 257];
        let mut out_scalar = [0u32; 257];

        for _ in 0..10_000 {
            let len = rng.range(0, input.len() as u64) as usize;
            let style = rng.range(0, 2);
            for b in input[0..len].iter_mut() {
                *b = match style {
                    // Mostly ASCII with occasional specials.
                    0 => match rng.range(0, 20) {
                        0 => 0x1B,
                        1 => rng.range(0, 0x1F) as u8,
                        2 => rng.next() as u8,
                        _ => rng.range(0x20, 0x7E) as u8,
                    },
                    // Heavy multi-byte/invalid UTF-8.
                    1 => match rng.range(0, 3) {
                        0 => rng.range(0x80, 0xBF) as u8,
                        1 => rng.range(0xC0, 0xFF) as u8,
                        2 => 0x1B,
                        _ => rng.range(0x20, 0x7E) as u8,
                    },
                    // Fully random bytes.
                    _ => rng.next() as u8,
                };
            }

            let res_simd = utf8DecodeUntilControlSeq(&input[0..len], &mut out_simd);
            let res_scalar = utf8_decode_until_control_seq_scalar(&input[0..len], &mut out_scalar);
            assert_eq!(
                res_scalar.consumed,
                res_simd.consumed,
                "consumed mismatch, input={:x?}",
                &input[0..len]
            );
            assert_eq!(
                res_scalar.decoded,
                res_simd.decoded,
                "decoded mismatch, input={:x?}",
                &input[0..len]
            );
            assert_eq!(
                out_scalar[0..res_scalar.decoded],
                out_simd[0..res_simd.decoded],
                "codepoint mismatch, input={:x?}",
                &input[0..len]
            );
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
    fn Table_3_8_non_shortest_form_sequences() {
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
    fn Table_3_9_surrogate_sequences() {
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
    fn Table_3_10_other_ill_formed_sequences() {
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
    fn Table_3_11_truncated_sequences() {
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
