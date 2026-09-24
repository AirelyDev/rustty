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
            // `rand.intRangeAtMost(u32, 1, 1_000_000)`: a 4-byte draw on the
            // 32-bit Lemire lane (parity S121-F1). CORRECTION: this drew a
            // `u64` because `Uint` then lacked `u32`, which consumed 8 bytes
            // and produced a different id for the same seed.
            let id = self.rand.int_range_at_most::<u32>(1, 1_000_000);
            let mut w = crate::io::FixedWriter::fixed(&mut control_buf);
            w.writeAll(b"a=t,f=100,i=").unwrap();
            w.writeUint(u64::from(id)).unwrap();
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

    /// Parity S121-F1: seed 0 reproduces the reference's bytes exactly. The
    /// golden was MEASURED: `zig test` (0.16.0) on a scratch root that imports
    /// the pinned `src/synthetic/Kitty.zig`, `DefaultPrng.init(0)`,
    /// `.data_len = 16`, three `gen.next(&writer, 1024)` calls, printed with
    /// `std.zig.fmtString`.
    #[test]
    fn kitty_seed_0_matches_the_reference_bytes() {
        let prng = DefaultPrng::init(0);
        let mut buf = [0u8; 8192];
        let mut writer = FixedWriter::fixed(&mut buf);
        let mut v = Kitty::new(prng.random());
        v.data_len = 16;
        let gen = v.generator();
        for _ in 0..3 {
            gen.next(&mut writer, 1024).unwrap();
        }
        assert_eq!(
            &b"\x1b_Ga=t,f=100,i=285327;wvOgFTDQOSyvAzi0\x1b\\\
               \x1b_Ga=t,f=100,i=684729;i0pO5EoPN794a1nM\x1b\\\
               \x1b_Ga=t,f=100,i=138824;cN/4m22EwBedx5eE\x1b\\"[..],
            writer.buffered(),
        );
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

    /// RUSTTY-SIDE (`P3-W9 T4` step 4): every emitted sequence must actually
    /// PARSE as a Kitty graphics APC command.
    ///
    /// Why this exists on top of `kitty_is_deterministic_for_a_fixed_seed`: that
    /// test checks the payload's SHAPE (base64 alphabet, length a multiple of
    /// four) and the `\x1b_G…\x1b\\` envelope by string matching. A generator
    /// that emitted a structurally malformed control block — a bad key, a
    /// missing `;`, an out-of-range `i=` — would still pass every one of those
    /// assertions while silently invalidating every benchmark built on this
    /// corpus, because the benchmark's parser would reject the input and measure
    /// the reject path instead of the parse path. This runs the real
    /// `rustty_vt::apc::Handler` and requires a `Command::Kitty` out the other
    /// end.
    ///
    /// The handler is fed the APC BODY, i.e. what the VT parser hands it after
    /// `\x1b_` and before `\x1b\\` — so the `G` identifier is the first byte.
    #[test]
    fn every_generated_sequence_parses_as_a_kitty_apc_command() {
        use allocator_api2::alloc::Global;
        use rustty_vt::apc::{Command, Handler};

        let prng = DefaultPrng::init(7);
        let mut buf = [0u8; 8192];
        let mut writer = FixedWriter::fixed(&mut buf);
        let mut v = Kitty::new(prng.random());
        // Several sequences, so a per-draw defect cannot hide behind one sample.
        const N: usize = 8;
        for _ in 0..N {
            v.next(&mut writer, 8192 / N).unwrap();
        }
        let data = writer.buffered().to_vec();

        let mut parsed = 0usize;
        let mut rest: &[u8] = &data;
        while !rest.is_empty() {
            assert!(rest.starts_with(b"\x1b_"), "not an APC start");
            let end = rest
                .windows(2)
                .position(|w| w == b"\x1b\\")
                .expect("ST terminator");
            // The body is everything between `\x1b_` and `\x1b\\`.
            let body = &rest[2..end];

            let mut h: Handler = Handler::default();
            h.start();
            h.feed_slice(Global, body);
            match h.end(Global) {
                Some(Command::Kitty(_)) => parsed += 1,
                Some(Command::Glyph(_)) => panic!("parsed as the GLYPH protocol"),
                // b53728241: an unrecognized APC now surfaces as `.unknown`
                // instead of null — for a generated kitty sequence that is
                // still a rejection.
                Some(Command::Unknown(_)) => panic!(
                    "the APC handler did not recognize a generated sequence: {:?}",
                    String::from_utf8_lossy(&body[..body.len().min(48)])
                ),
                None => panic!(
                    "the APC handler REJECTED a generated sequence: {:?}",
                    String::from_utf8_lossy(&body[..body.len().min(48)])
                ),
            }

            rest = &rest[end + 2..];
        }
        assert_eq!(parsed, N, "expected {N} parsed commands");
    }
}
