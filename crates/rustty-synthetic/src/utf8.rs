//! Generates UTF-8.
//!
//! This doesn't yet generate multi-codepoint graphemes, but it
//! has the ability to generate a custom distribution of UTF-8
//! encoding lengths (1, 2, 3, or 4 bytes).
//!
//! PORT NOTE: `synthetic/Utf8.zig`.

use crate::enums::EnumArray;
use crate::generator::Generator;
use crate::io::{WriteFailed, Writer};
use crate::rand::{Rng, U21};

/// PORT NOTE: the `std.unicode` helpers `Utf8.zig` (and its tests) lean on. Ported
/// locally — synthetic's allowed deps don't cleanly expose these, and they are
/// fully specified by the UTF-8 encoding rules.
mod unicode {
    /// PORT NOTE: `utf8CodepointSequenceLength(c) !u3`.
    pub fn utf8_codepoint_sequence_length(c: u32) -> Result<u8, ()> {
        if c < 0x80 {
            return Ok(1);
        }
        if c < 0x800 {
            return Ok(2);
        }
        if c < 0x10000 {
            return Ok(3);
        }
        if c < 0x110000 {
            return Ok(4);
        }
        Err(()) // error.CodepointTooLarge
    }

    /// PORT NOTE: `isSurrogateCodepoint(c)`.
    pub fn is_surrogate_codepoint(c: u32) -> bool {
        (0xD800..=0xDFFF).contains(&c)
    }

    /// PORT NOTE: the two-variant error of `utf8Encode`.
    pub enum EncodeError {
        Utf8CannotEncodeSurrogateHalf,
        #[allow(dead_code)]
        CodepointTooLarge,
    }

    /// PORT NOTE: `utf8Encode(c, out)` — writes 1-4 bytes with the standard lead /
    /// continuation masks. Returns the number of bytes written.
    pub fn utf8_encode(c: u32, out: &mut [u8]) -> Result<u8, EncodeError> {
        let length = match utf8_codepoint_sequence_length(c) {
            Ok(l) => l,
            Err(()) => return Err(EncodeError::CodepointTooLarge),
        };
        assert!(out.len() >= length as usize);
        match length {
            // The pattern for each is the same
            // - Increasing the initial shift by 6 each time
            // - Each time after the first shorten the shifted
            //   value to a max of 0b111111 (63)
            1 => out[0] = c as u8, // Can just do 0 + codepoint for initial range
            2 => {
                out[0] = (0b11000000 | (c >> 6)) as u8;
                out[1] = (0b10000000 | (c & 0b111111)) as u8;
            }
            3 => {
                if is_surrogate_codepoint(c) {
                    return Err(EncodeError::Utf8CannotEncodeSurrogateHalf);
                }
                out[0] = (0b11100000 | (c >> 12)) as u8;
                out[1] = (0b10000000 | ((c >> 6) & 0b111111)) as u8;
                out[2] = (0b10000000 | (c & 0b111111)) as u8;
            }
            4 => {
                out[0] = (0b11110000 | (c >> 18)) as u8;
                out[1] = (0b10000000 | ((c >> 12) & 0b111111)) as u8;
                out[2] = (0b10000000 | ((c >> 6) & 0b111111)) as u8;
                out[3] = (0b10000000 | (c & 0b111111)) as u8;
            }
            _ => unreachable!(),
        }
        Ok(length)
    }

    // PORT NOTE: `utf8ValidateSlice` and its `first` table are only consumed by the
    // ported `test {}` (mirroring std.unicode used in the Zig tests), so they
    // live under `cfg(test)`.
    #[cfg(test)]
    // The first nibble is used to identify the continuation byte range to
    // accept. The second nibble is the size.
    const XX: u8 = 0xF1; // invalid: size 1
    #[cfg(test)]
    const AS: u8 = 0xF0; // ASCII: size 1
    #[cfg(test)]
    const S1: u8 = 0x02; // accept 0, size 2
    #[cfg(test)]
    const S2: u8 = 0x13; // accept 1, size 3
    #[cfg(test)]
    const S3: u8 = 0x03; // accept 0, size 3
    #[cfg(test)]
    const S4: u8 = 0x23; // accept 2, size 3
    #[cfg(test)]
    const S5: u8 = 0x34; // accept 3, size 4
    #[cfg(test)]
    const S6: u8 = 0x04; // accept 0, size 4
    #[cfg(test)]
    const S7: u8 = 0x44; // accept 4, size 4

    /// PORT NOTE: the `first` table — info about the first byte in a UTF-8 sequence.
    #[cfg(test)]
    const FIRST: [u8; 256] = {
        let mut t = [AS; 256];
        let mut i = 128;
        while i < 192 {
            t[i] = XX;
            i += 1;
        }
        let tail: [u8; 64] = [
            XX, XX, S1, S1, S1, S1, S1, S1, S1, S1, S1, S1, S1, S1, S1, S1, //
            S1, S1, S1, S1, S1, S1, S1, S1, S1, S1, S1, S1, S1, S1, S1, S1, //
            S2, S3, S3, S3, S3, S3, S3, S3, S3, S3, S3, S3, S3, S4, S3, S3, //
            S5, S6, S6, S6, S7, XX, XX, XX, XX, XX, XX, XX, XX, XX, XX, XX,
        ];
        let mut j = 0;
        while j < 64 {
            t[192 + j] = tail[j];
            j += 1;
        }
        t
    };

    /// PORT NOTE: `utf8ValidateSlice(input)` — returns true if the input consists
    /// entirely of UTF-8 codepoints. The SIMD ASCII fast-path is a
    /// behaviour-identical optimisation; the scalar table walk is ported.
    #[cfg(test)]
    // PORT NOTE: continuation-byte bounds keep the reference's `x < lo or hi < x` form.
    #[allow(clippy::manual_range_contains)]
    pub fn utf8_validate_slice(input: &[u8]) -> bool {
        let remaining = input;

        // default lowest and highest continuation byte
        const LO_CB: u8 = 0b10000000;
        const HI_CB: u8 = 0b10111111;

        const MIN_NON_ASCII_CODEPOINT: u8 = 0x80;

        let n = remaining.len();
        let mut i: usize = 0;
        while i < n {
            let first_byte = remaining[i];
            if first_byte < MIN_NON_ASCII_CODEPOINT {
                i += 1;
                continue;
            }

            let info = FIRST[first_byte as usize];
            if info == XX {
                return false; // Illegal starter byte.
            }

            let size = (info & 7) as usize;
            if i + size > n {
                return false; // Short or invalid.
            }

            // Figure out the acceptable low and high continuation bytes, starting
            // with our defaults.
            let mut accept_lo: u8 = LO_CB;
            let mut accept_hi: u8 = HI_CB;

            match info >> 4 {
                0 => {}
                1 => accept_lo = 0xA0,
                2 => accept_hi = 0x9F,
                3 => accept_lo = 0x90,
                4 => accept_hi = 0x8F,
                _ => unreachable!(),
            }

            let c1 = remaining[i + 1];
            if c1 < accept_lo || accept_hi < c1 {
                return false;
            }

            match size {
                2 => i += 2,
                3 => {
                    let c2 = remaining[i + 2];
                    if c2 < LO_CB || HI_CB < c2 {
                        return false;
                    }
                    i += 3;
                }
                4 => {
                    let c2 = remaining[i + 2];
                    if c2 < LO_CB || HI_CB < c2 {
                        return false;
                    }
                    let c3 = remaining[i + 3];
                    if c3 < LO_CB || HI_CB < c3 {
                        return false;
                    }
                    i += 4;
                }
                _ => unreachable!(),
            }
        }

        true
    }
}

/// Possible UTF-8 encoding lengths.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Utf8Len {
    One = 1,
    Two = 2,
    Three = 3,
    Four = 4,
}

impl Utf8Len {
    /// PORT NOTE: `@intFromEnum(utf8_len)` — the encoding length value (1-4).
    fn value(self) -> u8 {
        self as u8
    }

    /// PORT NOTE: `@enumFromInt(v)` for the length values 1-4.
    fn from_value(v: u8) -> Utf8Len {
        match v {
            1 => Utf8Len::One,
            2 => Utf8Len::Two,
            3 => Utf8Len::Three,
            4 => Utf8Len::Four,
            _ => unreachable!(),
        }
    }

    /// PORT NOTE: `EnumArray(Utf8Len, f64).Indexer.keyForIndex(idx)` (declaration
    /// order: one, two, three, four).
    fn key_for_index(idx: usize) -> Utf8Len {
        match idx {
            0 => Utf8Len::One,
            1 => Utf8Len::Two,
            2 => Utf8Len::Three,
            3 => Utf8Len::Four,
            _ => unreachable!(),
        }
    }

    /// PORT NOTE: the declaration-order index (for `.set`).
    pub fn index(self) -> usize {
        match self {
            Utf8Len::One => 0,
            Utf8Len::Two => 1,
            Utf8Len::Three => 2,
            Utf8Len::Four => 3,
        }
    }
}

/// PORT NOTE: `const InvalidSequence = struct { len: u3, bytes: [4]u8 }`.
#[derive(Clone, Copy)]
struct InvalidSequence {
    len: u8,
    bytes: [u8; 4],
}

impl InvalidSequence {
    fn slice(&self) -> &[u8] {
        &self.bytes[0..self.len as usize]
    }
}

const INVALID_SEQUENCES: [InvalidSequence; 9] = [
    InvalidSequence {
        len: 1,
        bytes: [0x80, 0x00, 0x00, 0x00],
    },
    InvalidSequence {
        len: 1,
        bytes: [0xC0, 0x00, 0x00, 0x00],
    },
    InvalidSequence {
        len: 1,
        bytes: [0xFF, 0x00, 0x00, 0x00],
    },
    InvalidSequence {
        len: 2,
        bytes: [0xC2, 0x20, 0x00, 0x00],
    },
    InvalidSequence {
        len: 2,
        bytes: [0xC0, 0xAF, 0x00, 0x00],
    },
    InvalidSequence {
        len: 2,
        bytes: [0x80, 0x80, 0x00, 0x00],
    },
    InvalidSequence {
        len: 3,
        bytes: [0xED, 0xA0, 0x80, 0x00],
    },
    InvalidSequence {
        len: 3,
        bytes: [0xE2, 0x28, 0x7A, 0x00],
    },
    InvalidSequence {
        len: 4,
        bytes: [0xF0, 0x90, 0x28, 0x7A],
    },
];

/// PORT NOTE: `Utf8` — the `@This()` struct.
pub struct Utf8<'a> {
    /// Random number generator.
    pub rand: Rng<'a>,

    /// The minimum and maximum length of the generated bytes. The maximum
    /// length will be capped to the length of the buffer passed in if the
    /// buffer length is smaller.
    pub min_len: usize,
    pub max_len: usize,

    /// Probability of a specific UTF-8 encoding length being generated.
    /// The probabilities are weighted relative to each other, so they
    /// can sum greater than 1.0. A length of weight 1.0 and a length
    /// of weight 2.0 will have a 2:1 chance of the latter being
    /// selected.
    ///
    /// If a UTF-8 encoding of a chosen length can't fit into the remaining
    /// buffer, a smaller length will be chosen. For small buffers this may
    /// skew the distribution of lengths.
    pub p_length: EnumArray<4>,

    /// If true, ASCII codepoints are limited to printable ASCII.
    pub ascii_printable_only: bool,

    /// Probability that the next generated sequence is malformed UTF-8.
    /// This is checked for each emitted sequence while filling a buffer.
    pub invalid_rate: f64,
}

impl<'a> Utf8<'a> {
    /// PORT NOTE: the struct-literal defaults.
    pub fn new(rand: Rng<'a>) -> Utf8<'a> {
        Utf8 {
            rand,
            min_len: 1,
            max_len: usize::MAX,
            p_length: EnumArray::init_fill(1.0),
            ascii_printable_only: false,
            invalid_rate: 0.0,
        }
    }

    pub fn generator<'g, W: Writer<Error = WriteFailed>>(&'g mut self) -> Generator<'g, W> {
        fn next_trampoline<W: Writer<Error = WriteFailed>>(
            ptr: *mut (),
            writer: &mut W,
            max_len: usize,
        ) -> Result<(), WriteFailed> {
            let self_: &mut Utf8 = unsafe { &mut *(ptr as *mut Utf8) };
            self_.next(writer, max_len)
        }
        Generator::init(self as *mut Utf8 as *mut (), next_trampoline::<W>)
    }

    pub fn next<W: Writer<Error = WriteFailed>>(
        &mut self,
        writer: &mut W,
        max_len: usize,
    ) -> Result<(), WriteFailed> {
        let len = self
            .rand
            .int_range_at_most_biased::<usize>(self.min_len, self.max_len)
            .min(max_len);

        let mut rem: usize = len;
        while rem > 0 {
            if let Some(written) = self.write_invalid(writer, rem)? {
                rem -= written;
                continue;
            }

            let written = self.write_valid(writer, rem)?;
            rem -= written;
        }
        Ok(())
    }

    fn write_invalid<W: Writer<Error = WriteFailed>>(
        &mut self,
        writer: &mut W,
        rem: usize,
    ) -> Result<Option<usize>, WriteFailed> {
        if self.invalid_rate <= 0.0 || self.rand.float_f64() >= self.invalid_rate {
            return Ok(None);
        }

        let seq = match self.invalid_sequence(rem) {
            Some(seq) => seq,
            None => return Ok(None),
        };
        writer.writeAll(seq.slice())?;
        Ok(Some(seq.len as usize))
    }

    fn write_valid<W: Writer<Error = WriteFailed>>(
        &mut self,
        writer: &mut W,
        rem: usize,
    ) -> Result<usize, WriteFailed> {
        loop {
            let utf8_len = self.utf8_len(rem);

            // Generate a UTF-8 sequence that encodes to this length.
            let cp: u32 = match utf8_len {
                Utf8Len::One => {
                    if self.ascii_printable_only {
                        self.rand
                            .int_range_at_most_biased::<U21>(U21(0x20), U21(0x7E))
                            .0 as u32
                    } else {
                        self.rand
                            .int_range_at_most_biased::<U21>(U21(0x00), U21(0x7F))
                            .0 as u32
                    }
                }
                Utf8Len::Two => {
                    self.rand
                        .int_range_at_most_biased::<U21>(U21(0x80), U21(0x7FF))
                        .0 as u32
                }
                Utf8Len::Three => {
                    self.rand
                        .int_range_at_most_biased::<U21>(U21(0x800), U21(0xFFFF))
                        .0 as u32
                }
                Utf8Len::Four => {
                    self.rand
                        .int_range_at_most_biased::<U21>(U21(0x10000), U21(0x10FFFF))
                        .0 as u32
                }
            };

            assert!(unicode::utf8_codepoint_sequence_length(cp).unwrap() == utf8_len.value());

            let mut buf = [0u8; 4];
            let l = match unicode::utf8_encode(cp, &mut buf) {
                Ok(l) => l,
                // Impossible because our generation above is hardcoded to
                // produce a valid range. If not, a bug.
                Err(unicode::EncodeError::CodepointTooLarge) => unreachable!(),
                // Possible, in which case we redo the loop and encode nothing.
                Err(unicode::EncodeError::Utf8CannotEncodeSurrogateHalf) => continue,
            };

            writer.writeAll(&buf[0..l as usize])?;
            return Ok(l as usize);
        }
    }

    fn utf8_len(&mut self, rem: usize) -> Utf8Len {
        let idx = self.rand.weighted_index_f64(&self.p_length.values);
        let mut utf8_len = Utf8Len::key_for_index(idx);
        assert!(rem > 0);
        while utf8_len.value() as usize > rem {
            // If the chosen length can't fit into the remaining buffer,
            // choose a smaller length.
            utf8_len = Utf8Len::from_value(utf8_len.value() - 1);
        }
        utf8_len
    }

    fn invalid_sequence(&mut self, rem: usize) -> Option<InvalidSequence> {
        let candidates = &INVALID_SEQUENCES;

        let mut valid_idx: [usize; 9] = [0; 9];
        let mut valid_len: usize = 0;
        for (i, candidate) in candidates.iter().enumerate() {
            if candidate.len as usize > rem {
                continue;
            }
            valid_idx[valid_len] = i;
            valid_len += 1;
        }

        if valid_len == 0 {
            return None;
        }
        Some(candidates[valid_idx[self.rand.uint_less_than::<usize>(valid_len)]])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::FixedWriter;
    use crate::rand::DefaultPrng;

    #[test]
    fn utf8() {
        let prng = DefaultPrng::init(0);
        let mut buf = [0u8; 256];
        let mut writer = FixedWriter::fixed(&mut buf);
        let mut v = Utf8::new(prng.random());
        v.min_len = 256;
        v.max_len = 256;
        let gen = v.generator();
        gen.next(&mut writer, 256).unwrap();
        let result = writer.buffered();
        assert_eq!(256, result.len());
        assert!(unicode::utf8_validate_slice(result));
    }

    #[test]
    fn utf8_printable_ascii_only() {
        let prng = DefaultPrng::init(0);
        let mut buf = [0u8; 256];
        let mut writer = FixedWriter::fixed(&mut buf);
        let mut v = Utf8 {
            ascii_printable_only: true,
            ..Utf8::new(prng.random())
        };
        v.min_len = 256;
        v.max_len = 256;
        v.p_length.set(Utf8Len::One.index(), 1.0);
        v.p_length.set(Utf8Len::Two.index(), 0.0);
        v.p_length.set(Utf8Len::Three.index(), 0.0);
        v.p_length.set(Utf8Len::Four.index(), 0.0);

        let gen = v.generator();
        gen.next(&mut writer, 256).unwrap();
        let result = writer.buffered();
        assert_eq!(256, result.len());
        assert!(unicode::utf8_validate_slice(result));
        for &c in result {
            assert!(crate::bytes::is_print(c));
        }
    }

    #[test]
    fn utf8_malformed_output() {
        let prng = DefaultPrng::init(0);
        let mut buf = [0u8; 256];
        let mut writer = FixedWriter::fixed(&mut buf);
        let mut v = Utf8 {
            invalid_rate: 1.0,
            ..Utf8::new(prng.random())
        };
        v.min_len = 256;
        v.max_len = 256;

        let gen = v.generator();
        gen.next(&mut writer, 256).unwrap();
        let result = writer.buffered();
        assert_eq!(256, result.len());
        assert!(!unicode::utf8_validate_slice(result));
    }
}
