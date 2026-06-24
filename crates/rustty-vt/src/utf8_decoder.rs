//! DFA-based non-allocating error-replacing UTF-8 decoder.
//!
//! This implementation is based largely on the excellent work of
//! Bjoern Hoehrmann, with slight modifications to support error-
//! replacement.
//!
//! For details on Bjoern's DFA-based UTF-8 decoder, see
//! http://bjoern.hoehrmann.de/utf-8/decoder/dfa (MIT licensed)

// PORT NOTE: the reference is a Zig file-as-struct (`const UTF8Decoder = @This();`)
// with the imports
//   const std = @import("std");
//   const testing = std.testing;
//   const log = std.log.scoped(.utf8decoder);
// `std`/`testing` are the standard library, and `log` is an unused scoped logger
// (no `log.*` calls exist in this module), so neither has a ported analogue. The
// file-as-struct is ported as the `UTF8Decoder` struct below.

// PORT NOTE: `char_classes` is `[_]u4{...}` (256 entries). The `u4` element type
// is only ever used to index/shift, so it is ported as a `u8` array; the values
// are all <= 11, well within `u4` range, and reproduced verbatim.
// zig fmt: off
// PORT NOTE: `#[rustfmt::skip]` preserves the reference's verbatim grid layout
// (rustfmt does not honor the `// zig fmt: off`/`on` markers).
#[rustfmt::skip]
const CHAR_CLASSES: [u8; 256] = [
   0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,  0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
   0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,  0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
   0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,  0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
   0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,  0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
   1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,  9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,
   7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,  7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,
   8,8,2,2,2,2,2,2,2,2,2,2,2,2,2,2,  2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,
  10,3,3,3,3,3,3,3,3,3,3,3,3,4,3,3, 11,6,6,6,5,8,8,8,8,8,8,8,8,8,8,8,
];

#[rustfmt::skip]
const TRANSITIONS: [u8; 108] = [
   0,12,24,36,60,96,84,12,12,12,48,72, 12,12,12,12,12,12,12,12,12,12,12,12,
  12, 0,12,12,12,12,12, 0,12, 0,12,12, 12,24,12,12,12,12,12,24,12,24,12,12,
  12,12,12,12,12,12,12,24,12,12,12,12, 12,24,12,12,12,12,12,12,12,24,12,12,
  12,12,12,12,12,12,12,36,12,36,12,12, 12,36,12,12,12,12,12,36,12,36,12,12,
  12,36,12,12,12,12,12,12,12,12,12,12,
];
// zig fmt: on

// DFA states
const ACCEPT_STATE: u8 = 0;
const REJECT_STATE: u8 = 12;

#[derive(Clone, Copy, Debug, Default)]
pub struct UTF8Decoder {
    // This is where we accumulate our current codepoint.
    // PORT NOTE: the reference field is `accumulator: u21 = 0`. Rust has no `u21`,
    // so it is ported as `u32` (the codepoint never exceeds 21 bits by construction
    // of the DFA); the emitted codepoint values match the reference exactly.
    accumulator: u32,
    // The internal state of the DFA.
    state: u8,
}

impl UTF8Decoder {
    // PORT NOTE: the reference default field values (`accumulator: u21 = 0`,
    // `state: u8 = ACCEPT_STATE`). `ACCEPT_STATE == 0`, so this matches the derived
    // `Default` (all zero); `new()` is provided for an explicit `.{}` analogue.
    pub fn new() -> UTF8Decoder {
        UTF8Decoder {
            accumulator: 0,
            state: ACCEPT_STATE,
        }
    }

    /// Takes the next byte in the utf-8 sequence and emits a tuple of
    /// - The codepoint that was generated, if there is one.
    /// - A boolean that indicates whether the provided byte was consumed.
    ///
    /// The only case where the byte is not consumed is if an ill-formed
    /// sequence is reached, in which case a replacement character will be
    /// emitted and the byte will not be consumed.
    ///
    /// If the byte is not consumed, the caller is responsible for calling
    /// again with the same byte before continuing.
    #[inline]
    pub fn next(&mut self, byte: u8) -> (Option<u32>, bool) {
        let char_class = CHAR_CLASSES[byte as usize];

        let initial_state = self.state;

        if self.state != ACCEPT_STATE {
            self.accumulator <<= 6;
            self.accumulator |= (byte & 0x3F) as u32;
        } else {
            self.accumulator = ((0xFF_u32 >> char_class) & (byte as u32)) & 0x1F_FFFF;
        }

        self.state = TRANSITIONS[(self.state + char_class) as usize];

        if self.state == ACCEPT_STATE {
            // Emit the fully decoded codepoint.
            // PORT NOTE: the reference uses `defer self.accumulator = 0;` to reset
            // the accumulator after capturing the return value. Ported by capturing
            // the value first, then resetting before returning.
            let cp = self.accumulator;
            self.accumulator = 0;
            (Some(cp), true)
        } else if self.state == REJECT_STATE {
            self.accumulator = 0;
            self.state = ACCEPT_STATE;
            // Emit a replacement character. If we rejected the first byte
            // in a sequence, then it was consumed, otherwise it was not.
            (Some(0xFFFD), initial_state == ACCEPT_STATE)
        } else {
            // Emit nothing, we're in the middle of a sequence.
            (None, true)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii() {
        let mut d = UTF8Decoder::new();
        let mut out: [u8; 13] = [0; 13];
        for (i, byte) in "Hello, World!".bytes().enumerate() {
            let res = d.next(byte);
            assert!(res.1);
            if let Some(codepoint) = res.0 {
                out[i] = codepoint as u8;
            }
        }

        assert_eq!(&out, "Hello, World!".as_bytes());
    }

    #[test]
    fn well_formed_utf_8() {
        let mut d = UTF8Decoder::new();
        let mut out: [u32; 4] = [0; 4];
        let mut i: usize = 0;
        // 4 bytes, 3 bytes, 2 bytes, 1 byte
        for byte in "😄✤ÁA".bytes() {
            let mut consumed = false;
            while !consumed {
                let res = d.next(byte);
                consumed = res.1;
                // There are no errors in this sequence, so
                // every byte should be consumed first try.
                assert!(consumed);
                if let Some(codepoint) = res.0 {
                    out[i] = codepoint;
                    i += 1;
                }
            }
        }

        assert_eq!(out, [0x1F604, 0x2724, 0xC1, 0x41]);
    }

    #[test]
    fn partially_invalid_utf_8() {
        let mut d = UTF8Decoder::new();
        let mut out: [u32; 5] = [0; 5];
        let mut i: usize = 0;
        // Illegally terminated sequence, valid sequence, illegal surrogate pair.
        for byte in *b"\xF0\x9F\xF0\x9F\x98\x84\xED\xA0\x80" {
            let mut consumed = false;
            while !consumed {
                let res = d.next(byte);
                consumed = res.1;
                if let Some(codepoint) = res.0 {
                    out[i] = codepoint;
                    i += 1;
                }
            }
        }

        assert_eq!(out, [0xFFFD, 0x1F604, 0xFFFD, 0xFFFD, 0xFFFD]);
    }
}
