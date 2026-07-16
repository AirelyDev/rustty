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

// PERF(port): pad table so the u8-derived index is provably in-bounds; Zig
// ReleaseFast has no check here (UTF8Decoder.zig:68). Entries 0..108 are the
// reference `transitions` table verbatim (the grid below); entries 108..=511
// are unreachable filler set to 12 (REJECT — the reference's own fail-safe
// value). Reachable states are {0,24,36,48,60,72,84,96} and char_class <= 11,
// so the max reachable index is exactly 107; the pad only exists so LLVM can
// elide the per-byte bounds check. The pad is 512 (not 256) because the index
// is `state as usize + char_class as usize` with both operands zero-extended
// from u8: the sum is <= 510 < 512, so the access is in-bounds for EVERY
// possible u8 pair with no comparison at all (a 256 pad still left a residual
// per-byte `cmp #0xff; b.hi` pair in the hot loop, disassembly-verified).
#[rustfmt::skip]
const TRANSITIONS: [u8; 512] = {
    let reference: [u8; 108] = [
       0,12,24,36,60,96,84,12,12,12,48,72, 12,12,12,12,12,12,12,12,12,12,12,12,
      12, 0,12,12,12,12,12, 0,12, 0,12,12, 12,24,12,12,12,12,12,24,12,24,12,12,
      12,12,12,12,12,12,12,24,12,12,12,12, 12,24,12,12,12,12,12,12,12,24,12,12,
      12,12,12,12,12,12,12,36,12,36,12,12, 12,36,12,12,12,12,12,36,12,36,12,12,
      12,36,12,12,12,12,12,12,12,12,12,12,
    ];
    let mut table = [12u8; 512];
    let mut i = 0;
    while i < reference.len() {
        table[i] = reference[i];
        i += 1;
    }
    table
};

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
    // PORT NOTE: `pub(crate)` so the stream SIMD fast path (`nextSliceCapped`)
    // can read `self.utf8decoder.state` directly as the reference does — Zig
    // struct fields are package-visible, and stream.rs lives in this crate.
    pub(crate) state: u8,
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
    // PERF(port): Zig `pub inline fn` (UTF8Decoder.zig:56) is a MANDATORY
    // inline — upgrade #[inline] to #[inline(always)] to restore it.
    //
    // PERF(port) (reshape): same control flow as the reference
    // (UTF8Decoder.zig:56-86) with two codegen-level reshapes, byte-identical
    // observable behavior for ALL inputs (machine-checked exhaustively by the
    // `reshape_matches_reference_control_flow` test below against a verbatim
    // port of the reference's body):
    //   1. the accumulator update computes both candidates unconditionally
    //      (pure ALU) and selects — LLVM already emitted a csel for the
    //      reference's `if`; this pins that shape at the source level.
    //   2. the transition is loaded into a LOCAL and the `self.state` store is
    //      issued per-arm, with the accept arm storing the literal
    //      ACCEPT_STATE — on the hot accept path the stored state is a
    //      constant, so the loop-carried DFA state dependency breaks at every
    //      character boundary (the reference gets this from Zig whole-program
    //      codegen; storing through `self.state` before branching re-reads the
    //      field and keeps a serial add+load chain).
    // Together with the panic-free 512-entry pad above, the per-byte body has
    // no bounds check and no memory side effects a caller does not observe:
    // when a caller discards the return entirely (decode-only loops) the whole
    // DFA folds away, and callers that consume the codepoint keep the
    // reference's speculation-friendly branch structure.
    #[inline(always)]
    pub fn next(&mut self, byte: u8) -> (Option<u32>, bool) {
        let char_class = CHAR_CLASSES[byte as usize];

        let initial_state = self.state;

        // Both accumulator candidates of the reference's `if` (UTF8Decoder.zig:
        // 61-66), unconditionally (pure ALU), then selected — same stored value.
        let cont_acc = (self.accumulator << 6) | ((byte & 0x3F) as u32);
        let first_acc = ((0xFF_u32 >> char_class) & (byte as u32)) & 0x1F_FFFF;
        let acc = if initial_state != ACCEPT_STATE {
            cont_acc
        } else {
            first_acc
        };

        // PERF(port): index as usize + usize (both zext from u8, <= 255) so the
        // add cannot overflow; together with the 512-entry pad this makes the
        // access provably in-bounds with no residual comparison — matching
        // Zig's checkless `transitions[self.state + char_class]`
        // (UTF8Decoder.zig:68, ReleaseFast).
        let next_state = TRANSITIONS[initial_state as usize + char_class as usize];

        if next_state == ACCEPT_STATE {
            // Emit the fully decoded codepoint.
            // PORT NOTE: the reference uses `defer self.accumulator = 0;` to reset
            // the accumulator after capturing the return value. Ported by capturing
            // the value first, then resetting before returning.
            self.state = ACCEPT_STATE;
            self.accumulator = 0;
            (Some(acc), true)
        } else if next_state == REJECT_STATE {
            self.accumulator = 0;
            self.state = ACCEPT_STATE;
            // Emit a replacement character. If we rejected the first byte
            // in a sequence, then it was consumed, otherwise it was not.
            (Some(0xFFFD), initial_state == ACCEPT_STATE)
        } else {
            // Emit nothing, we're in the middle of a sequence.
            self.state = next_state;
            self.accumulator = acc;
            (None, true)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // PERF(port): guards the 512-entry pad of TRANSITIONS — entries 0..108 must
    // stay byte-identical to the reference `transitions` table (UTF8Decoder.zig:28),
    // and every pad entry must be the fail-safe REJECT value.
    #[test]
    fn transitions_table_matches_reference_with_reject_padding() {
        // The reference table, embedded verbatim (108 entries).
        #[rustfmt::skip]
        const REFERENCE: [u8; 108] = [
           0,12,24,36,60,96,84,12,12,12,48,72, 12,12,12,12,12,12,12,12,12,12,12,12,
          12, 0,12,12,12,12,12, 0,12, 0,12,12, 12,24,12,12,12,12,12,24,12,24,12,12,
          12,12,12,12,12,12,12,24,12,12,12,12, 12,24,12,12,12,12,12,12,12,24,12,12,
          12,12,12,12,12,12,12,36,12,36,12,12, 12,36,12,12,12,12,12,36,12,36,12,12,
          12,36,12,12,12,12,12,12,12,12,12,12,
        ];
        assert_eq!(&TRANSITIONS[..108], &REFERENCE[..]);
        for (i, &entry) in TRANSITIONS[108..].iter().enumerate() {
            assert_eq!(entry, REJECT_STATE, "pad entry {} is not REJECT", 108 + i);
        }
    }

    /// A verbatim port of the reference's branchy `next` body
    /// (UTF8Decoder.zig:56-86), kept as the oracle for the branchless reshape.
    fn reference_next(d: &mut UTF8Decoder, byte: u8) -> (Option<u32>, bool) {
        let char_class = CHAR_CLASSES[byte as usize];
        let initial_state = d.state;
        if d.state != ACCEPT_STATE {
            d.accumulator <<= 6;
            d.accumulator |= (byte & 0x3F) as u32;
        } else {
            d.accumulator = ((0xFF_u32 >> char_class) & (byte as u32)) & 0x1F_FFFF;
        }
        d.state = TRANSITIONS[d.state as usize + char_class as usize];
        if d.state == ACCEPT_STATE {
            let cp = d.accumulator;
            d.accumulator = 0;
            (Some(cp), true)
        } else if d.state == REJECT_STATE {
            d.accumulator = 0;
            d.state = ACCEPT_STATE;
            (Some(0xFFFD), initial_state == ACCEPT_STATE)
        } else {
            (None, true)
        }
    }

    // PERF(port) (reshape): machine-check for the reshaped `next` — the
    // return AND the post-call struct state must match the reference's
    // control flow exactly, over
    //   (a) every (reachable state, accumulator pattern, byte) triple, and
    //   (b) every 2-byte sequence from a fresh decoder, compared per step, and
    //   (c) a long deterministic pseudo-random byte stream, compared per step.
    #[test]
    fn reshape_matches_reference_control_flow() {
        // (a) exhaustive over the DFA's state space: all states that appear
        // in the transition table (plus ACCEPT) x accumulator bit patterns
        // (bit-parallel ops: 0, alternating, all-ones-u21 distinguish any
        // shift/mask/select error) x all 256 bytes.
        let mut states: Vec<u8> = TRANSITIONS.to_vec();
        states.push(ACCEPT_STATE);
        states.sort_unstable();
        states.dedup();
        for &state in &states {
            for acc in [0u32, 0x15555, 0x0AAAA, 0x1F_FFFF] {
                for byte in 0..=255u8 {
                    let mut a = UTF8Decoder {
                        accumulator: acc,
                        state,
                    };
                    let mut b = a;
                    let ra = a.next(byte);
                    let rb = reference_next(&mut b, byte);
                    assert_eq!(
                        ra, rb,
                        "return diverges: state={state} acc={acc:#x} byte={byte:#x}"
                    );
                    assert_eq!(
                        (a.state, a.accumulator),
                        (b.state, b.accumulator),
                        "post-state diverges: state={state} acc={acc:#x} byte={byte:#x}"
                    );
                }
            }
        }

        // (b) every 2-byte sequence from a fresh decoder, stepwise.
        for b1 in 0..=255u8 {
            for b2 in 0..=255u8 {
                let mut a = UTF8Decoder::new();
                let mut b = UTF8Decoder::new();
                for byte in [b1, b2] {
                    assert_eq!(a.next(byte), reference_next(&mut b, byte));
                    assert_eq!((a.state, a.accumulator), (b.state, b.accumulator));
                }
            }
        }

        // (c) a deterministic xorshift byte stream (valid and invalid UTF-8
        // interleave freely), stepwise.
        let mut a = UTF8Decoder::new();
        let mut b = UTF8Decoder::new();
        let mut x: u32 = 0x243F_6A88;
        for i in 0..1_000_000u32 {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            let byte = (x >> 11) as u8;
            assert_eq!(
                a.next(byte),
                reference_next(&mut b, byte),
                "stream step {i}"
            );
            assert_eq!((a.state, a.accumulator), (b.state, b.accumulator));
        }
    }

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
