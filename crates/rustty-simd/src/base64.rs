//! base64 — PORTS ghostty src/simd/base64.zig.

use crate::base64_scalar;
use crate::base64_scalar::scalar_decoder;
use rustty_util::quirks::inlineAssert as assert;

// const log = std.log.scoped(.simd_base64);
//
// PORT NOTE: the reference's scope tag, carried through the `log` facade as
// `target: LOG_SCOPE` (DECISION-logging.md §6r4).
const LOG_SCOPE: &str = "simd_base64";

/// `options.simd` — whether SIMD-accelerated code paths are built. Mirrors the
/// reference's `build_options.simd`, which defaults to `true`.
// WASM DIVERGENCE: on `wasm32` both SIMD branches are gated OFF (no C++ kernel on
// freestanding wasm), so this const is unreferenced there — the scalar lanes are
// always taken.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
const SIMD: bool = true;

// PORT NOTE (Miri): Miri cannot execute the C/C++ side of the FFI ("can't
// call foreign function"), so under Miri the two entry points below take the
// scalar lane instead — the exact code the reference builds when
// `build_options.simd` is off, with identical output by construction (see
// `scalar_input`). `cfg!(miri)` is const-false in every real build, so the
// SIMD lane is untouched everywhere else (in-repo precedent: the `cfg(miri)`
// std-heap allocator fallback in rustty-vt pagelist/mod.rs). The SIMD lane
// itself stays covered by this crate's ASan G6 route and `cargo test`.
pub fn max_len(input: &[u8]) -> usize {
    // WASM DIVERGENCE: the `wasm32` freestanding target has no C++ SIMD kernel
    // (see `lib.rs`), so the SIMD branch is gated OFF and the pure-Rust scalar
    // lane below is always taken — identical output by construction, same as the
    // `cfg!(miri)` fallback the branch guard already carries.
    #[cfg(not(target_arch = "wasm32"))]
    if SIMD && !cfg!(miri) {
        return unsafe { rustty_simd_base64_max_length(input.as_ptr(), input.len()) };
    }

    max_len_scalar(input)
}

fn max_len_scalar(input: &[u8]) -> usize {
    // PARITY: the reference is `catch |err| { log.warn(...); return 0; }`; kept
    // as an explicit match so the error-logging branch is preserved.
    #[allow(clippy::manual_unwrap_or, clippy::manual_unwrap_or_default)]
    match scalar_decoder.calc_size_for_slice(scalar_input(input)) {
        Ok(v) => v,
        Err(err) => {
            // log.warn("failed to calculate base64 size for payload: {}", .{err});
            //
            // PORT NOTE (N11): Zig's `{}` on an error renders `error.<Name>`;
            // the Rust `{err:?}` renders the bare variant name. That is the
            // workspace-dominant rendering for a ported `err={}`/`: {}` and is
            // N11's scope to change, not this site's.
            log::warn!(
                target: LOG_SCOPE,
                "failed to calculate base64 size for payload: {err:?}"
            );
            0
        }
    }
}

/// Mirrors Zig's `error{Base64Invalid}`.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct Base64Invalid;

pub fn decode<'a>(input: &[u8], output: &'a mut [u8]) -> Result<&'a [u8], Base64Invalid> {
    // See the Miri PORT NOTE on `max_len`. WASM DIVERGENCE: SIMD branch gated
    // OFF on `wasm32` (freestanding has no C++ kernel); scalar lane always taken.
    #[cfg(not(target_arch = "wasm32"))]
    if SIMD && !cfg!(miri) {
        let res =
            unsafe { rustty_simd_base64_decode(input.as_ptr(), input.len(), output.as_mut_ptr()) };
        if res < 0 {
            return Err(Base64Invalid);
        }
        return Ok(&output[0..res as usize]);
    }

    decode_scalar(input, output)
}

fn decode_scalar<'a>(input_raw: &[u8], output: &'a mut [u8]) -> Result<&'a [u8], Base64Invalid> {
    let input = scalar_input(input_raw);
    let size = max_len_scalar(input);
    if size == 0 {
        return Ok(b"");
    }
    assert(output.len() >= size);
    scalar_decoder
        .decode(output, scalar_input(input))
        .map_err(|_| Base64Invalid)?;
    Ok(&output[0..size])
}

/// For non-SIMD enabled builds, we trim the padding from the end of the
/// base64 input in order to get identical output with the SIMD version.
// R7 NOTE: the reference fix replaced an underflowing reverse scan
// (`input[input.len - i - 1]`, which crashed on empty input) with this
// `while end > 0` form; the guard is load-bearing, not style.
fn scalar_input(input: &[u8]) -> &[u8] {
    let mut end = input.len();
    while end > 0 && input[end - 1] == b'=' {
        end -= 1;
    }
    &input[0..end]
}

/// Whether strict decoding requires the input to be padded to a
/// multiple of four bytes (RFC 4648 section 3.2). The Kitty clipboard
/// protocol requires padding; the legacy OSC 52 protocol tolerates a
/// missing-padding tail because it has no way to report errors to the
/// client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Padding {
    Required,
    Optional,
}

/// Decode strict RFC 4648 standard-alphabet base64: characters outside
/// the alphabet (including whitespace) and misplaced padding are
/// errors rather than being skipped, and padding is validated per the
/// given requirement. This is the decoding the Kitty clipboard
/// protocol specifies:
/// https://sw.kovidgoyal.net/kitty/clipboard/#encoding-of-payloads
///
/// The output must be at least max_len(input) bytes.
// PARITY: the `% 4` comparisons mirror the reference's modulo checks.
#[allow(clippy::manual_is_multiple_of)]
pub fn decode_strict<'a>(
    input: &[u8],
    output: &'a mut [u8],
    padding: Padding,
) -> Result<&'a [u8], Base64Invalid> {
    // Padding can only be a suffix of at most two bytes; any '='
    // elsewhere is rejected by the underlying decode.
    let mut pad: usize = 0;
    if !input.is_empty() && input[input.len() - 1] == b'=' {
        pad += 1;
    }
    if input.len() > 1 && input[input.len() - 2] == b'=' {
        pad += 1;
    }

    match padding {
        Padding::Required => {
            if input.len() % 4 != 0 {
                return Err(Base64Invalid);
            }
        }
        // Present padding must still complete a four byte group; only
        // fully absent padding is tolerated. A single leftover byte
        // can never carry a decodable value.
        Padding::Optional => {
            if pad > 0 {
                if input.len() % 4 != 0 {
                    return Err(Base64Invalid);
                }
            } else if input.len() % 4 == 1 {
                return Err(Base64Invalid);
            }
        }
    }

    // The permissive decode already rejects every invalid character
    // except whitespace, which simdutf silently skips. Skipped
    // characters make the decoded length fall short of the exact
    // length the input length implies, so comparing the two rejects
    // whitespace without a separate validation pass over the input.
    let decoded = decode(input, output)?;
    let expected = match input.len() % 4 {
        0 => input.len() / 4 * 3 - pad,
        2 => input.len() / 4 * 3 + 1,
        3 => input.len() / 4 * 3 + 2,
        _ => unreachable!(), // rejected above
    };
    if decoded.len() != expected {
        return Err(Base64Invalid);
    }
    Ok(decoded)
}

/// A streaming strict base64 decoder for one logical payload split
/// across multiple chunks at arbitrary byte boundaries (e.g. the Kitty
/// clipboard protocol's wdata packets): the concatenation of the fed
/// chunks must be valid RFC 4648 standard-alphabet base64.
///
/// Padding is terminal within a feed: a padded group followed by more
/// data in the same feed is an error. A feed that ends exactly at
/// terminal padding resets the decoder so the next feed starts a
/// fresh stream, exactly like the kitty reference implementation
/// (which resets its aklomp streaming decoder on EOF); this keeps
/// clients that pad each chunk independently working.
#[derive(Default)]
pub struct Streaming {
    /// Partial group carried between feeds; a group only decodes once
    /// all four of its characters have arrived.
    // PORT NOTE: the reference leaves `carry` `= undefined` until written; Rust
    // requires initialization, so `Default` zero-fills it (it is never read
    // beyond the first `carry_len` bytes written).
    carry: [u8; 4],
    carry_len: u8, // u3
}

impl Streaming {
    /// Maximum decoded bytes one feed of input can produce.
    pub fn max_len(&self, input: &[u8]) -> usize {
        (self.carry_len as usize + input.len()) / 4 * 3
    }

    /// Decode the complete groups of input (with any carried bytes
    /// prepended) into output, which must be at least max_len(input)
    /// bytes, and carry the remainder for the next feed.
    pub fn feed<'a>(
        &mut self,
        input: &[u8],
        output: &'a mut [u8],
    ) -> Result<&'a [u8], Base64Invalid> {
        assert(output.len() >= self.max_len(input));
        if input.is_empty() {
            return Ok(&output[0..0]);
        }

        let mut rem = input;
        let mut written: usize = 0;

        // Complete a carried partial group first.
        if self.carry_len > 0 {
            let take = (4 - self.carry_len as usize).min(rem.len());
            for (&c, pos) in rem[0..take].iter().zip(self.carry_len as usize..) {
                if !Self::valid_partial_char(pos, c) {
                    return Err(Base64Invalid);
                }
            }
            self.carry[self.carry_len as usize..][0..take].copy_from_slice(&rem[0..take]);
            self.carry_len += u8::try_from(take).unwrap();
            rem = &rem[take..];
            if self.carry_len < 4 {
                return Ok(&output[0..0]);
            }
            self.carry_len = 0;
            let (decoded, padded) = Self::group(self.carry, output)?;
            written += decoded;
            if padded {
                if !rem.is_empty() {
                    return Err(Base64Invalid);
                }
                return Ok(&output[0..written]);
            }
        }

        // Bulk-decode all complete groups with the strict single-shot
        // decode, which also enforces that padding only appears as a
        // terminal suffix. Terminal padding must then end the feed.
        let bulk = &rem[0..rem.len() - (rem.len() % 4)];
        written += decode_strict(bulk, &mut output[written..], Padding::Required)?.len();
        if !bulk.is_empty() && bulk[bulk.len() - 1] == b'=' {
            if bulk.len() != rem.len() {
                return Err(Base64Invalid);
            }
            return Ok(&output[0..written]);
        }

        // Carry the trailing partial group, validated eagerly so
        // garbage is reported on the feed that contains it.
        let tail = &rem[bulk.len()..];
        for (pos, &c) in tail.iter().enumerate() {
            if !Self::valid_partial_char(pos, c) {
                return Err(Base64Invalid);
            }
        }
        self.carry[0..tail.len()].copy_from_slice(tail);
        self.carry_len = u8::try_from(tail.len()).unwrap();
        Ok(&output[0..written])
    }

    /// Decode one complete four character group, the only place
    /// padding is legal: the last two characters may be '=' ('=' in
    /// the second-to-last position requires it in the last). Returns
    /// the decoded length and whether the group was padded.
    fn group(g: [u8; 4], output: &mut [u8]) -> Result<(usize, bool), Base64Invalid> {
        let padded = g[3] == b'=';
        let decoded = decode_strict(&g, output, Padding::Required)?;
        Ok((decoded.len(), padded))
    }

    /// Whether c is valid at the given position of a partial group
    /// still waiting for its remaining characters: the first two
    /// positions must be alphabet characters and only the last two
    /// may open the padding suffix.
    fn valid_partial_char(pos: usize, c: u8) -> bool {
        base64_scalar::is_alphabet_char(c) || (pos >= 2 && c == b'=')
    }

    /// Finish the stream: the concatenation of the fed chunks must
    /// have formed complete groups, so a carried partial group is an
    /// error (the stream was not correctly padded). The decoder is
    /// ready for a fresh stream afterwards either way.
    pub fn finish(&mut self) -> Result<(), Base64Invalid> {
        // Zig `defer self.* = .{}`: the reset runs on both return paths.
        let carry_len = self.carry_len;
        *self = Streaming::default();
        if carry_len != 0 {
            return Err(Base64Invalid);
        }
        Ok(())
    }
}

// base64.cpp — gated OFF on `wasm32` (no C++ kernel on freestanding wasm).
#[cfg(not(target_arch = "wasm32"))]
extern "C" {
    fn rustty_simd_base64_max_length(input: *const u8, len: usize) -> usize;
    fn rustty_simd_base64_decode(input: *const u8, len: usize, output: *mut u8) -> isize;
}

#[cfg(test)]
mod tests {
    // `base64_maxLen` carries the reference test name `"base64 maxLen"`
    // (camelCase `maxLen` preserved); spaces are not valid in a Rust fn name.
    #![allow(non_snake_case)]
    use super::*;

    // ---- task-14e-2 / S120-F2 ------------------------------------------------

    /// `maxLenScalar`'s failure arm emits the reference's message under the
    /// reference's `.simd_base64` scope, and returns 0.
    ///
    /// `pad_char` is `None` for `scalar_decoder`, so `calcSizeUpperBound`
    /// returns `error.InvalidPadding` for any length ≡ 1 (mod 4) — a real
    /// driver, not a synthesised error value.
    #[test]
    fn max_len_scalar_warns_when_the_size_cannot_be_calculated() {
        let cap = crate::log_capture::start();
        let len = max_len_scalar(b"aGVsb");
        let messages = cap.messages();
        drop(cap);

        // Positive reach: the reference's `catch` arm returns 0.
        assert_eq!(0, len);
        assert_eq!(
            vec![(
                log::Level::Warn,
                "simd_base64".to_string(),
                "failed to calculate base64 size for payload: InvalidPadding".to_string(),
            )],
            messages
        );
    }

    /// The absence half: a payload whose size CAN be calculated emits nothing.
    /// Guards the "diagnostic on a success path" defect, and asserts positively
    /// that it reached past the `catch` the silence is about — the returned
    /// length only exists on the `Ok` arm.
    #[test]
    fn max_len_scalar_is_silent_on_success() {
        let cap = crate::log_capture::start();
        let len = max_len_scalar(b"aGVsbG8gd29ybGQ=");
        let messages = cap.messages();
        drop(cap);

        assert_eq!(
            11, len,
            "maxLenScalar did not return on the Ok arm, so this test never \
             reached the code whose silence it asserts"
        );
        assert_eq!(Vec::<crate::log_capture::CapturedRecord>::new(), messages);
    }

    #[test]
    fn base64_maxLen() {
        let len = max_len(b"aGVsbG8gd29ybGQ=");
        assert_eq!(11, len);
    }

    #[test]
    fn base64_empty_input() {
        let mut output: [u8; 0] = [];

        assert_eq!(0, max_len(b""));
        assert_eq!(b"".as_slice(), decode(b"", &mut output).unwrap());
    }

    #[test]
    fn base64_decode() {
        let input = b"aGVsbG8gd29ybGQ=";
        let len = max_len(input);
        let mut output = vec![0u8; len];
        let str = decode(input, &mut output).unwrap();
        assert_eq!(b"hello world".as_slice(), str);
    }

    #[test]
    fn base64_strict_decode_valid() {
        let mut output = [0u8; 128];

        struct Case {
            input: &'static [u8],
            expect: &'static [u8],
        }
        let cases = [
            Case {
                input: b"",
                expect: b"",
            },
            Case {
                input: b"aGVsbG8gd29ybGQ=",
                expect: b"hello world",
            },
            Case {
                input: b"bGlnaHQgdw==",
                expect: b"light w",
            },
            Case {
                input: b"Zm9vYmFy",
                expect: b"foobar",
            },
            // Every alphabet character in one input.
            Case {
                input: b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/",
                expect: b"\x00\x10\x83\x10\x51\x87\x20\x92\x8b\x30\xd3\x8f\x41\x14\x93\x51\x55\x97\x61\x96\x9b\x71\xd7\x9f\x82\x18\xa3\x92\x59\xa7\xa2\x9a\xab\xb2\xdb\xaf\xc3\x1c\xb3\xd3\x5d\xb7\xe3\x9e\xbb\xf3\xdf\xbf",
            },
        ];
        for case in &cases {
            assert_eq!(
                case.expect,
                decode_strict(case.input, &mut output, Padding::Required).unwrap()
            );
            assert_eq!(
                case.expect,
                decode_strict(case.input, &mut output, Padding::Optional).unwrap()
            );
        }
    }

    #[test]
    fn base64_strict_decode_invalid() {
        let mut output = [0u8; 128];

        // The invalid inputs from kitty's own strict decoding tests plus
        // some extra padding-placement cases. All of these are invalid for
        // both padding requirements.
        let cases: [&[u8]; 11] = [
            b"bGlnaHQgdw=",    // missing one padding byte
            b"bGln!!Qgdw==",   // invalid characters
            b"bGlnaHQgdw==\n", // trailing whitespace
            b"\nbGlnaHQgdw==", // leading whitespace
            b"bGlnaHQg dw==",  // interior whitespace
            b"!!!!",
            b"=",
            b"==",
            b"A===",
            b"AB=C", // padding must be the suffix
            b"Zm9v YmFy",
        ];
        for &case in &cases {
            assert_eq!(
                Err(Base64Invalid),
                decode_strict(case, &mut output, Padding::Required)
            );
            assert_eq!(
                Err(Base64Invalid),
                decode_strict(case, &mut output, Padding::Optional)
            );
        }

        // Unpadded input is only tolerated when padding is optional. A
        // single leftover character can never be decoded.
        assert_eq!(
            Err(Base64Invalid),
            decode_strict(b"bGlnaHQgdw", &mut output, Padding::Required)
        );
        assert_eq!(
            b"light w".as_slice(),
            decode_strict(b"bGlnaHQgdw", &mut output, Padding::Optional).unwrap()
        );
        assert_eq!(
            Err(Base64Invalid),
            decode_strict(b"bGl", &mut output, Padding::Required)
        );
        assert_eq!(
            b"li".as_slice(),
            decode_strict(b"bGl", &mut output, Padding::Optional).unwrap()
        );
        assert_eq!(
            Err(Base64Invalid),
            decode_strict(b"bGlna", &mut output, Padding::Optional)
        );
    }

    #[test]
    fn base64_streaming_decode_chunk_boundaries() {
        // Decoding a stream split at every possible boundary, including
        // one byte at a time, matches the single-shot decode.
        let input = b"c29tZSBsb25nZXIgZGF0YSB3aXRoIHBhZGRpbmc+Pz8=";
        let expect = b"some longer data with padding>??";
        for split in 0..input.len() + 1 {
            let mut s = Streaming::default();
            let mut result: Vec<u8> = Vec::new();
            let mut output = [0u8; 64];
            result.extend_from_slice(s.feed(&input[0..split], &mut output).unwrap());
            result.extend_from_slice(s.feed(&input[split..], &mut output).unwrap());
            s.finish().unwrap();
            assert_eq!(expect.as_slice(), result.as_slice());
        }
        {
            let mut s = Streaming::default();
            let mut result: Vec<u8> = Vec::new();
            let mut output = [0u8; 4];
            for i in 0..input.len() {
                result.extend_from_slice(s.feed(&input[i..][0..1], &mut output).unwrap());
            }
            s.finish().unwrap();
            assert_eq!(expect.as_slice(), result.as_slice());
        }
    }

    #[test]
    fn base64_streaming_decode_invalid() {
        let mut output = [0u8; 64];

        // Invalid characters are rejected wherever they appear.
        {
            let mut s = Streaming::default();
            assert_eq!(Err(Base64Invalid), s.feed(b"!!!!", &mut output));
        }
        {
            let mut s = Streaming::default();
            assert_eq!(Err(Base64Invalid), s.feed(b"SGVs!!!bG8=", &mut output));
        }
        {
            let mut s = Streaming::default();
            assert_eq!(Err(Base64Invalid), s.feed(b"\nc29tZSBkYXRh", &mut output));
        }

        // Data after terminal padding within one feed is rejected, even
        // when the padded group only completes in that feed.
        {
            let mut s = Streaming::default();
            assert_eq!(Err(Base64Invalid), s.feed(b"Z29vZA==SGVsbG8=", &mut output));
        }
        {
            let mut s = Streaming::default();
            let _ = s.feed(b"Z29vZA=", &mut output).unwrap();
            assert_eq!(Err(Base64Invalid), s.feed(b"=SGVs", &mut output));
        }

        // A feed that ends exactly at terminal padding resets the stream:
        // the next feed starts fresh, so clients that pad every chunk
        // independently keep working (matching the kitty implementation,
        // which resets its streaming decoder on EOF).
        {
            let mut s = Streaming::default();
            assert_eq!(
                b"good".as_slice(),
                s.feed(b"Z29vZA==", &mut output).unwrap()
            );
            assert_eq!(
                b"Hello".as_slice(),
                s.feed(b"SGVsbG8=", &mut output).unwrap()
            );
            s.finish().unwrap();
        }
        {
            // Padding split across feeds resets too.
            let mut s = Streaming::default();
            assert_eq!(b"goo".as_slice(), s.feed(b"Z29vZA=", &mut output).unwrap());
            assert_eq!(b"d".as_slice(), s.feed(b"=", &mut output).unwrap());
            assert_eq!(
                b"more".as_slice(),
                s.feed(b"bW9yZQ==", &mut output).unwrap()
            );
            s.finish().unwrap();
        }

        // Misplaced padding within a group.
        {
            let mut s = Streaming::default();
            assert_eq!(Err(Base64Invalid), s.feed(b"YQ=X", &mut output));
        }
        {
            let mut s = Streaming::default();
            assert_eq!(Err(Base64Invalid), s.feed(b"=AAA", &mut output));
        }

        // A stream that ends in a partial group is missing its padding.
        // The failed finish resets the decoder for the next stream.
        {
            let mut s = Streaming::default();
            assert_eq!(b"Hel".as_slice(), s.feed(b"SGVsbG8", &mut output).unwrap());
            assert_eq!(Err(Base64Invalid), s.finish());
            assert_eq!(b"Hel".as_slice(), s.feed(b"SGVsbG8", &mut output).unwrap());
            assert_eq!(b"lo".as_slice(), s.feed(b"=", &mut output).unwrap());
            s.finish().unwrap();
        }
    }
}
