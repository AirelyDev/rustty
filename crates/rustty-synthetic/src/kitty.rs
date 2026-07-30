//! Generates random Kitty graphics protocol APC sequences.
//!
//! The payload is random base64, NOT a valid image: decoding it as
//! PNG will fail. This corpus is meant for benchmarking the stream,
//! APC, and Kitty command parsing paths, which never decode the
//! image data. It does not exercise successful image loading.
//!
//! PORT NOTE: `synthetic/Kitty.zig`.
// const Kitty = @This();

// const std = @import("std");
// const assert = std.debug.assert;
// const Generator = @import("Generator.zig");
// const Bytes = @import("Bytes.zig");
use crate::bytes::{generate_alphabet, Bytes};
use crate::generator::Generator;
use crate::io::{WriteFailed, Writer, WriterExt};
use crate::rand::Rng;

/// PORT NOTE: `const Kitty = @This()` — the generator's own struct.
pub struct Kitty<'a> {
    /// Random number generator.
    // rand: std.Random,
    pub rand: Rng<'a>,

    /// The base64 payload length for each generated command. This is
    /// rounded down to a multiple of four so the payload is always valid
    /// base64 without requiring padding. Kitty clients typically chunk
    /// payloads at 4096 bytes.
    // data_len: usize = 4096,
    pub data_len: usize,
}

impl<'a> Kitty<'a> {
    /// The struct-literal default for `data_len` (`= 4096`).
    pub fn new(rand: Rng<'a>) -> Kitty<'a> {
        Kitty {
            rand,
            data_len: 4096,
        }
    }
}

// fn checkBase64Alphabet(c: u8) bool { ... }
// /// The base64 alphabet, without the padding character.
// pub const base64_alphabet = Bytes.generateAlphabet(checkBase64Alphabet);
generate_alphabet!(pub base64_alphabet, |c| matches!(
    c,
    b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'+' | b'/'
));

// const prefix = "\x1b_G";
const PREFIX: &[u8] = b"\x1b_G";
// const st = "\x1b\\";
const ST: &[u8] = b"\x1b\\";

impl<'a> Kitty<'a> {
    // pub fn generator(self: *Kitty) Generator
    pub fn generator<W: Writer<Error = WriteFailed>>(&mut self) -> Generator<'_, W> {
        Generator::init(self as *mut Self as *mut (), |ptr, writer, max_len| {
            let self_: &mut Kitty<'_> = unsafe { &mut *(ptr as *mut Kitty<'_>) };
            self_.next(writer, max_len)
        })
    }

    /// Get the next Kitty graphics APC sequence: a well-formed transmit
    /// command with a random base64 payload (not a valid image; see the
    /// module comment), including the APC prefix and the ST terminator.
    // pub fn next(self: *Kitty, writer: *std.Io.Writer, max_len: usize) Generator.Error!void
    pub fn next<W: Writer<Error = WriteFailed>>(
        &mut self,
        writer: &mut W,
        max_len: usize,
    ) -> Result<(), WriteFailed> {
        // var control_buf: [64]u8 = undefined;
        // const control = std.fmt.bufPrint(
        //     &control_buf, "a=t,f=100,i={d};",
        //     .{self.rand.intRangeAtMost(u32, 1, 1_000_000)},
        // ) catch unreachable;
        let mut control_buf = [0u8; 64];
        let control: &[u8] = {
            // PORT NOTE: `rand.intRangeAtMost(u32, 1, 1_000_000)`. The port's
            // `Uint` is implemented for `u8`/`u64`/`usize` and the newtype
            // widths, not `u32`; the range fits in `u64` and the draw is
            // identical (the generator consumes the same number of bytes for
            // both widths only if the width matches, so `u64` is used and the
            // value is narrowed for formatting).
            let id = self.rand.int_range_at_most::<u64>(1, 1_000_000);
            let mut w = crate::io::FixedWriter::fixed(&mut control_buf);
            w.writeAll(b"a=t,f=100,i=").unwrap();
            w.writeUint(id).unwrap();
            w.writeAll(b";").unwrap();
            let n = w.buffered().len();
            &control_buf[..n]
        };

        // const overhead = prefix.len + control.len + st.len;
        let overhead = PREFIX.len() + control.len() + ST.len();
        // assert(max_len > overhead);
        assert!(max_len > overhead);

        // const avail = @min(self.data_len, max_len - overhead);
        let avail = core::cmp::min(self.data_len, max_len - overhead);
        // const payload_len = avail - (avail % 4);
        let payload_len = avail - (avail % 4);

        // try writer.writeAll(prefix);
        writer.writeAll(PREFIX)?;
        // try writer.writeAll(control);
        writer.writeAll(control)?;
        // if (payload_len > 0) { ... }
        if payload_len > 0 {
            let bytes = Bytes {
                rand: self.rand,
                alphabet: Some(base64_alphabet()),
                min_len: payload_len,
                max_len: payload_len,
            };
            // _ = try bytes.write(writer);
            let _ = bytes.write(writer)?;
        }
        // try writer.writeAll(st);
        writer.writeAll(ST)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::FixedWriter;
    use crate::rand::DefaultPrng;

    // test "kitty"
    #[test]
    fn kitty() {
        let prng = DefaultPrng::init(0);

        let mut buf = [0u8; 8192];
        let mut writer = FixedWriter::fixed(&mut buf);
        let mut v = Kitty::new(prng.random());
        let gen = v.generator();
        gen.next(&mut writer, 8192).unwrap();

        let data = writer.buffered();
        assert!(data.starts_with(b"\x1b_Ga=t,f=100,i="));
        assert!(data.ends_with(b"\x1b\\"));
    }

    /// RUSTTY-SIDE (WS10 T17 Step 7): generator determinism.
    ///
    /// The plan's step reads `rustty-synthetic +kitty --seed=42` twice and
    /// `cmp`s — but upstream's `synthetic/cli/Kitty.zig` `Options` has only
    /// `data-len`, NO `seed` (unlike `cli/Utf8.zig`, which does), and
    /// `mainActionImpl` always time-seeds. Adding a `--seed` flag would be a
    /// divergence from the oracle, so determinism is asserted where it is
    /// actually specified: same seed in, same bytes out. It also pins the
    /// payload alphabet and the 4-byte alignment the module comment promises.
    #[test]
    fn kitty_is_deterministic_for_a_fixed_seed() {
        let render = |seed: u64| -> Vec<u8> {
            let prng = DefaultPrng::init(seed);
            let mut buf = [0u8; 8192];
            let mut writer = FixedWriter::fixed(&mut buf);
            let mut v = Kitty::new(prng.random());
            for _ in 0..4 {
                v.next(&mut writer, 8192 / 4).unwrap();
            }
            writer.buffered().to_vec()
        };

        let a = render(42);
        let b = render(42);
        assert_eq!(a, b, "same seed must produce identical output");
        assert_ne!(a, render(43), "different seeds must diverge");

        // The payload between the control terminator ';' and the ST is base64
        // alphabet only, and its length is a multiple of four (no padding
        // needed) — both promised by the module comment.
        let alphabet = base64_alphabet();
        let mut rest: &[u8] = &a;
        while let Some(semi) = rest.iter().position(|&c| c == b';') {
            let after = &rest[semi + 1..];
            let st = after
                .windows(2)
                .position(|w| w == b"\x1b\\")
                .expect("ST terminator");
            let payload = &after[..st];
            assert!(payload.len().is_multiple_of(4), "len={}", payload.len());
            assert!(payload.iter().all(|c| alphabet.contains(c)));
            rest = &after[st + 2..];
        }
    }
}
