//! PORTS: Zig stdlib `std.hash.Wyhash` (`lib/std/hash/wyhash.zig`, Zig 0.16) AS
//! THE PORT USES IT — the final-v3 wyhash used by `std.hash.autoHash` and, in
//! ghostty, by `src/terminal/hash_map.zig`'s `AutoContext.hash`
//! (`std.hash.Wyhash.hash(0, std.mem.asBytes(&key))`).
//!
//! Why this exists: the terminal page hash maps are stored INSIDE the page's
//! single backing allocation, so the hash function is not an implementation
//! detail — it decides home slots, metadata fingerprints, probe order, iterator
//! order, and therefore the raw bytes of the page mapping that
//! `compress/page.rs` feeds to LZ4. Reproducing ghostty's physical page bytes
//! requires reproducing ghostty's hash bit-for-bit, which means porting Wyhash
//! rather than substituting a Rust hasher.
//!
//! Like the rest of rustty-zigstd this is ADDITIVE INFRASTRUCTURE GLUE (R1
//! allows necessary additions): `std.hash.Wyhash` is Zig stdlib, not ghostty
//! src/, so it is NOT in the parity test contract; the faithfulness target is
//! the exact observable output of the Zig API as ghostty uses it (verified
//! against the upstream `test_vector.cpp` rows shipped in Zig's own
//! `wyhash.zig` tests, plus oracle-generated vectors).

// PARITY: the reference is a single `pub const Wyhash = struct { ... }` with
// `init`/`update`/`final`/`hash` plus inline private helpers. The port keeps the
// helper names (`round`, `read`, `mum`, `mix`, `small_key`, `final0`, `final1`,
// `final2`) and their bodies line-for-line; only Zig's `[]const u8` slicing
// spellings become Rust slice expressions.
#![allow(clippy::many_single_char_names)]

/// `const secret = [_]u64{ ... }`
const SECRET: [u64; 4] = [
    0xa076_1d64_78bd_642f,
    0xe703_7ed1_a0b4_28db,
    0x8ebc_6af0_9c88_c6e3,
    0x5899_65cc_7537_4cc3,
];

/// Zig `std.hash.Wyhash`.
///
/// Two entry points, mirroring the reference:
/// - one-shot [`Wyhash::hash`] (`pub fn hash(seed: u64, input: []const u8) u64`),
///   which is what ghostty's `AutoContext.hash` calls;
/// - the iterative API [`Wyhash::init`] / [`Wyhash::update`] /
///   [`Wyhash::finalize`] (`init`/`update`/`final`), which produces the same
///   digest as the one-shot form for the same concatenated input.
//
// PORT NOTE: Zig leaves `a`, `b`, `buf` `undefined` at `init`; Rust has no
// `undefined`, so they are zeroed. They are always written before being read
// (`smallKey`/`final1` set `a`/`b`; `buf[0..buf_len]` is the only part of `buf`
// ever read), so zeroing cannot change any digest.
#[derive(Clone, Copy)]
pub struct Wyhash {
    a: u64,
    b: u64,
    state: [u64; 3],
    total_len: usize,

    buf: [u8; 48],
    buf_len: usize,
}

impl Wyhash {
    /// `pub fn init(seed: u64) Wyhash`
    // PERF(port): `#[inline]` on the whole public API so cross-crate callers
    // can inline without LTO (banned; PERF_LOG.md). The reference needs the
    // same treatment on the Zig side — hash_map.zig force-inlines its hash
    // call ("LLVM 21 (Zig 0.16) failed to inline this which resulted in a
    // measurable almost 2x slowdown on our hyperlink map benchmark"). Without
    // these, `release-fast` paid 3 real calls + a PLT memcpy + an 88-byte
    // stack spill per 4-byte key hash (measured 6.1x on hyperlink churn).
    #[inline]
    pub fn init(seed: u64) -> Wyhash {
        let mut self_ = Wyhash {
            a: 0,
            b: 0,
            state: [0; 3],
            total_len: 0,
            buf: [0; 48],
            buf_len: 0,
        };

        self_.state[0] = seed ^ mix(seed ^ SECRET[0], SECRET[1]);
        self_.state[1] = self_.state[0];
        self_.state[2] = self_.state[0];
        self_
    }

    // This is subtly different from other hash function update calls. Wyhash requires the last
    // full 48-byte block to be run through final1 if is exactly aligned to 48-bytes.
    /// `pub fn update(self: *Wyhash, input: []const u8) void`
    #[inline]
    pub fn update(&mut self, input: &[u8]) {
        self.total_len += input.len();

        if input.len() <= 48 - self.buf_len {
            self.buf[self.buf_len..self.buf_len + input.len()].copy_from_slice(input);
            self.buf_len += input.len();
            return;
        }

        let mut i: usize = 0;

        if self.buf_len > 0 {
            i = 48 - self.buf_len;
            let buf_len = self.buf_len;
            self.buf[buf_len..48].copy_from_slice(&input[0..i]);
            let block = self.buf;
            self.round(&block);
            self.buf_len = 0;
        }

        while i + 48 < input.len() {
            let block: [u8; 48] = input[i..i + 48].try_into().unwrap();
            self.round(&block);
            i += 48;
        }

        let remaining_bytes = &input[i..];
        if remaining_bytes.len() < 16 && i >= 48 {
            let rem = 16 - remaining_bytes.len();
            self.buf[48 - rem..].copy_from_slice(&input[i - rem..i]);
        }
        self.buf[0..remaining_bytes.len()].copy_from_slice(remaining_bytes);
        self.buf_len = remaining_bytes.len();
    }

    /// `pub fn final(self: *Wyhash) u64`
    //
    // PORT NOTE: `final` is Rust-reserved-ish only as a name style; the reference
    // takes `*Wyhash` but explicitly `shallowCopy()`s "to ensure idempotency", so
    // the port takes `&self` and works on the copy — same observable behavior,
    // and callers may finalize repeatedly.
    #[inline]
    pub fn finalize(&self) -> u64 {
        let mut input: &[u8] = &self.buf[0..self.buf_len];
        let mut new_self = self.shallow_copy();
        let mut scratch: [u8; 16] = [0; 16];

        if self.total_len <= 16 {
            new_self.small_key(input);
        } else {
            let mut offset: usize = 0;
            if self.buf_len < 16 {
                let rem = 16 - self.buf_len;
                scratch[0..rem].copy_from_slice(&self.buf[48 - rem..48]);
                scratch[rem..rem + self.buf_len].copy_from_slice(&self.buf[0..self.buf_len]);

                // Same as input but with additional bytes preceding start in case of a short buffer
                input = &scratch;
                offset = rem;
            }

            new_self.final0();
            new_self.final1(input, offset);
        }

        new_self.final2()
    }

    // Copies the core wyhash state but not any internal buffers.
    /// `inline fn shallowCopy(self: *Wyhash) Wyhash`
    #[inline]
    fn shallow_copy(&self) -> Wyhash {
        Wyhash {
            a: self.a,
            b: self.b,
            state: self.state,
            total_len: self.total_len,
            buf: [0; 48],
            buf_len: 0,
        }
    }

    /// `inline fn smallKey(self: *Wyhash, input: []const u8) void`
    #[inline]
    fn small_key(&mut self, input: &[u8]) {
        debug_assert!(input.len() <= 16);

        if input.len() >= 4 {
            let end = input.len() - 4;
            let quarter = (input.len() >> 3) << 2;
            self.a = (read(4, input) << 32) | read(4, &input[quarter..]);
            self.b = (read(4, &input[end..]) << 32) | read(4, &input[end - quarter..]);
        } else if !input.is_empty() {
            self.a = (u64::from(input[0]) << 16)
                | (u64::from(input[input.len() >> 1]) << 8)
                | u64::from(input[input.len() - 1]);
            self.b = 0;
        } else {
            self.a = 0;
            self.b = 0;
        }
    }

    /// `inline fn round(self: *Wyhash, input: *const [48]u8) void`
    #[inline]
    fn round(&mut self, input: &[u8; 48]) {
        for i in 0..3 {
            let a = read(8, &input[8 * (2 * i)..]);
            let b = read(8, &input[8 * (2 * i + 1)..]);
            self.state[i] = mix(a ^ SECRET[i + 1], b ^ self.state[i]);
        }
    }

    /// `inline fn final0(self: *Wyhash) void`
    #[inline]
    fn final0(&mut self) {
        self.state[0] ^= self.state[1] ^ self.state[2];
    }

    // input_lb must be at least 16-bytes long (in shorter key cases the smallKey function will be
    // used instead). We use an index into a slice to for comptime processing as opposed to if we
    // used pointers.
    /// `inline fn final1(self: *Wyhash, input_lb: []const u8, start_pos: usize) void`
    #[inline]
    fn final1(&mut self, input_lb: &[u8], start_pos: usize) {
        debug_assert!(input_lb.len() >= 16);
        debug_assert!(input_lb.len() - start_pos <= 48);
        let input = &input_lb[start_pos..];

        let mut i: usize = 0;
        while i + 16 < input.len() {
            self.state[0] = mix(
                read(8, &input[i..]) ^ SECRET[1],
                read(8, &input[i + 8..]) ^ self.state[0],
            );
            i += 16;
        }

        self.a = read(8, &input_lb[input_lb.len() - 16..]);
        self.b = read(8, &input_lb[input_lb.len() - 8..]);
    }

    /// `inline fn final2(self: *Wyhash) u64`
    #[inline]
    fn final2(&mut self) -> u64 {
        self.a ^= SECRET[1];
        self.b ^= self.state[0];
        mum(&mut self.a, &mut self.b);
        mix(
            self.a ^ SECRET[0] ^ (self.total_len as u64),
            self.b ^ SECRET[1],
        )
    }

    /// `pub fn hash(seed: u64, input: []const u8) u64`
    #[inline]
    pub fn hash(seed: u64, input: &[u8]) -> u64 {
        let mut self_ = Wyhash::init(seed);

        if input.len() <= 16 {
            self_.small_key(input);
        } else {
            let mut i: usize = 0;
            if input.len() >= 48 {
                while i + 48 < input.len() {
                    let block: [u8; 48] = input[i..i + 48].try_into().unwrap();
                    self_.round(&block);
                    i += 48;
                }
                self_.final0();
            }
            self_.final1(input, i);
        }

        self_.total_len = input.len();
        self_.final2()
    }
}

/// `inline fn read(comptime bytes: usize, data: []const u8) u64` — a
/// little-endian load of `bytes` bytes (`std.mem.readInt(T, .., .little)`).
#[inline]
fn read(bytes: usize, data: &[u8]) -> u64 {
    debug_assert!(bytes <= 8);
    let mut buf = [0u8; 8];
    buf[0..bytes].copy_from_slice(&data[0..bytes]);
    u64::from_le_bytes(buf)
}

/// `inline fn mum(a: *u64, b: *u64) void`
#[inline]
fn mum(a: &mut u64, b: &mut u64) {
    let x = u128::from(*a).wrapping_mul(u128::from(*b));
    *a = x as u64;
    *b = (x >> 64) as u64;
}

/// `inline fn mix(a_: u64, b_: u64) u64`
#[inline]
fn mix(a_: u64, b_: u64) -> u64 {
    let mut a = a_;
    let mut b = b_;
    mum(&mut a, &mut b);
    a ^ b
}

// PORT NOTE (ADDITIVE, no reference counterpart): a `core::hash::Hasher` face on
// the iterative API so `AutoContext` can hash any `core::hash::Hash` key the way
// Zig's `AutoContext.hash` hashes a `hasUniqueRepresentation` key: Rust's default
// `write_uN`/`write_iN` methods forward to `write(&x.to_ne_bytes())`, which for
// the port's key types (integers and `#[repr(transparent)]` `Offset<T>` over a
// `u32`) feeds exactly the bytes `std.mem.asBytes(&key)` would, in the same
// order. It is NOT a general `asBytes` equivalent: Rust's `Hash` for slices and
// `str` writes a length prefix and Zig's `autoHash` does not, so keep this
// hasher scoped to the fixed-width key types the terminal maps use.
impl core::hash::Hasher for Wyhash {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        self.update(bytes);
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.finalize()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::hash::{Hash, Hasher};

    /// The upstream wyhash `test_vector.cpp` rows shipped as `vectors` in Zig's
    /// own `lib/std/hash/wyhash.zig` test block.
    #[test]
    fn test_vectors() {
        let vectors: [(u64, u64, &[u8]); 7] = [
            (0x0409_638e_e2bd_e459, 0, b""),
            (0xa841_2d09_1b5f_e0a9, 1, b"a"),
            (0x32dd_92e4_b291_5153, 2, b"abc"),
            (0x8619_1240_89a3_a16b, 3, b"message digest"),
            (0x7a43_afb6_1d7f_5f40, 4, b"abcdefghijklmnopqrstuvwxyz"),
            (
                0xff42_329b_90e5_0d58,
                5,
                b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789",
            ),
            (
                0xc39c_ab13_b115_aad3,
                6,
                b"12345678901234567890123456789012345678901234567890123456789012345678901234567890",
            ),
        ];

        for (expected, seed, input) in vectors {
            assert_eq!(expected, Wyhash::hash(seed, input));
        }
    }

    // test "test vectors at comptime" runs the same `vectors` table inside a
    // `comptime` block, and `test "smhasher"` / `test "iterative api"` below
    // each run their check a second time as `try comptime Test.do()`. Those
    // compile-time halves are not ported: Rust has no counterpart to a Zig
    // `comptime` block, which can evaluate any ordinary function at compile
    // time. Running this code in a Rust `const` would need the port's `Wyhash`
    // rewritten as `const fn`, a different function from the one under test.
    // The runtime halves are the same checks on the same inputs.

    /// `verify.smhasher` (`lib/std/hash/verify.zig`): hash keys `{}`, `{0}`,
    /// `{0,1}`, ... `{0..254}` with seed `256 - i`, concatenate the 8-byte
    /// little-endian hashes, and hash that with seed 0; the low 32 bits are
    /// the SMHasher verification code.
    fn smhasher() -> u32 {
        let mut buf = [0u8; 256];
        let mut buf_all = [0u8; 256 * 8];
        for i in 0..256usize {
            // buf[i] = @intCast(i);
            buf[i] = u8::try_from(i).unwrap();
            let h = Wyhash::hash((256 - i) as u64, &buf[0..i]);
            buf_all[i * 8..i * 8 + 8].copy_from_slice(&h.to_le_bytes());
        }
        Wyhash::hash(0, &buf_all) as u32
    }

    /// Zig's `test "smhasher"`.
    #[test]
    fn test_smhasher() {
        assert_eq!(smhasher(), 0xBD5E_840C);
    }

    /// Zig's `test "iterative api"` (`verify.iterativeApi(Wyhash)`): feed
    /// chunks of 1, 2, ... 31 zero bytes; after each, `final` twice must agree
    /// (idempotent) and must equal the one-shot hash of everything so far.
    #[test]
    fn test_iterative_api() {
        // Sum(1..32) = 528
        let buf = [0u8; 528];
        let mut len: usize = 0;
        let seed = 0;

        let mut hasher = Wyhash::init(seed);
        for i in 1..32 {
            let r = Wyhash::hash(seed, &buf[0..len + i]);
            hasher.update(&buf[len..len + i]);
            let f1 = hasher.finalize();
            let f2 = hasher.finalize();
            assert_eq!(f1, f2, "IterativeHashWasNotIdempotent");
            assert_eq!(f1, r, "IterativeHashDidNotMatchDirect");
            len += i;
        }
    }

    /// A differential against Zig 0.16.0's `std.hash.Wyhash` (task tail-s2,
    /// NEW-FINDINGS N82). The inputs are 602 random byte strings, lengths
    /// `k % 301` (0 to 300, twice), seed 0 for the first 301 and a random seed
    /// after that, drawn from a SplitMix64 stream that the Zig probe
    /// (`probe_n82.zig`) draws identically; every input is also fed to the
    /// iterative API in random 1..64-byte chunks. All 602 one-shot and 602
    /// iterative hashes matched the probe; the rows below keep the ones at
    /// the length boundaries of the algorithm (`smallKey` <= 16, `final1`
    /// <= 48, the 48-byte `round`s) with the probe's values.
    #[test]
    fn differential_against_zig() {
        struct SplitMix64(u64);
        impl SplitMix64 {
            fn next(&mut self) -> u64 {
                self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
                let mut z = self.0;
                z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
                z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
                z ^ (z >> 31)
            }
        }

        // (k, seed, expected) from the Zig probe.
        let rows: [(usize, u64, u64); 25] = [
            (0, 0, 0x0409_638e_e2bd_e459),
            (1, 0, 0x069a_5c31_5a3d_f180),
            (3, 0, 0xbd38_a3f4_2b32_aecd),
            (4, 0, 0x078a_be22_5607_e127),
            (8, 0, 0xca6b_af7b_a8c1_c2c6),
            (15, 0, 0x3a27_cdba_9081_4fdd),
            (16, 0, 0x6152_564e_0cd8_9363),
            (17, 0, 0x3e15_ab60_fbc1_3d38),
            (47, 0, 0x7e5b_64ec_926e_1fe2),
            (48, 0, 0xa320_60d8_0387_f7ce),
            (49, 0, 0x8773_0997_8877_aa51),
            (95, 0, 0x7ea5_bed2_f124_8abd),
            (96, 0, 0xa196_869a_a021_bbe9),
            (97, 0, 0x443b_14e4_34df_dffd),
            (144, 0, 0x4e25_db19_f4d4_9aae),
            (145, 0, 0x7689_a8d2_84b4_b4b5),
            (200, 0, 0x5aa8_5eba_a5ed_668a),
            (300, 0, 0x3e80_9dd3_7697_d664),
            (301, 0x1a7e_44f7_add4_1273, 0x2ec8_976a_1b40_d84a),
            (305, 0x7b23_e450_a17d_5d1b, 0xa897_9d7e_1abd_df65),
            (349, 0x2d63_c9e8_4c37_1461, 0xcb78_0feb_2d3c_bdeb),
            (350, 0x340d_ed0f_b6a9_b7be, 0x41b2_07c7_dde5_831c),
            (397, 0xb27e_d942_accf_5f44, 0x40f7_d00b_3399_bfd2),
            (450, 0x3997_3b12_5dce_b591, 0x3bf6_a15b_c5e4_f6cc),
            (601, 0x4241_4865_7410_51df, 0x94e6_aec8_03fe_ecd8),
        ];

        let mut rng = SplitMix64(0x5eed_0082);
        let mut checked = 0;
        for k in 0..602usize {
            let len = k % 301;
            let seed = if k < 301 { 0 } else { rng.next() };
            let buf: Vec<u8> = (0..len).map(|_| rng.next() as u8).collect();
            let one = Wyhash::hash(seed, &buf);
            let mut wh = Wyhash::init(seed);
            let mut i = 0;
            while i < len {
                let step = (1 + rng.next() % 64) as usize;
                let end = len.min(i + step);
                wh.update(&buf[i..end]);
                i = end;
            }
            assert_eq!(one, wh.finalize(), "k={k}");
            if let Some(&(_, want_seed, want)) = rows.iter().find(|r| r.0 == k) {
                assert_eq!(want_seed, seed, "k={k}");
                assert_eq!(want, one, "k={k}");
                checked += 1;
            }
        }
        assert_eq!(rows.len(), checked);
    }

    /// Seed-0 rows — the seed ghostty's `AutoContext` uses — generated from the
    /// oracle's Zig 0.16 `std.hash.Wyhash`.
    #[test]
    fn wyhash_seed0_vectors() {
        let vectors: [(u64, &[u8]); 7] = [
            (0x0409_638e_e2bd_e459, b""),
            (0x28d2_0533_09d2_8531, b"a"),
            (0x02a4_f1d7_cb51_6c72, b"abc"),
            (0x41d0_32e1_df79_b67e, b"message digest"),
            (0x774f_a8c2_1ed6_acd2, b"abcdefghijklmnopqrstuvwxyz"),
            (
                0x0369_bcbe_3f0f_0c0d,
                b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789",
            ),
            (
                0x4866_2e17_cabf_ab37,
                b"12345678901234567890123456789012345678901234567890123456789012345678901234567890",
            ),
        ];

        for (expected, input) in vectors {
            assert_eq!(expected, Wyhash::hash(0, input));
        }
    }

    /// `std.hash.Wyhash.hash(0, std.mem.asBytes(&key))` for the `u32` keys the
    /// hash-map parity probe uses (oracle-generated).
    #[test]
    fn wyhash_u32_asbytes_vectors() {
        let vectors: [(u32, u64); 8] = [
            (0, 0x14b8_3016_dc46_0955),
            (1, 0xd145_1a1b_34d5_fb64),
            (2, 0xb52d_3031_bc13_56f4),
            (3, 0x3579_1793_81f3_5bc6),
            (4, 0x63b2_677c_5d80_f97a),
            (8, 0xe850_de3f_7e60_b2fa),
            (16, 0x052a_446e_fa88_3433),
            (24, 0xa226_4b2b_07f9_9c5a),
        ];

        for (key, expected) in vectors {
            assert_eq!(expected, Wyhash::hash(0, &key.to_ne_bytes()));

            // The `Hasher` face must agree with the one-shot form for a key
            // whose `Hash` impl writes exactly its native bytes.
            let mut hasher = Wyhash::init(0);
            key.hash(&mut hasher);
            assert_eq!(expected, hasher.finish());
        }
    }

    /// Zig's `test "iterative maintains last sixteen"` plus a multi-`update`
    /// split across the 48-byte block boundary.
    #[test]
    fn iterative_matches_one_shot() {
        let mut big = [0u8; 200];
        for (i, b) in big.iter_mut().enumerate() {
            *b = (i % 251) as u8;
        }
        assert_eq!(0xb964_93db_7846_fe8c, Wyhash::hash(0, &big));
        assert_eq!(0x9ef0_0d3d_0756_bdab, Wyhash::hash(12345, &big));

        let mut wh = Wyhash::init(0);
        wh.update(&big[0..37]);
        wh.update(&big[37..100]);
        wh.update(&big[100..200]);
        assert_eq!(0xb964_93db_7846_fe8c, wh.finalize());
        // `final` is idempotent (the reference `shallowCopy`s for exactly this).
        assert_eq!(0xb964_93db_7846_fe8c, wh.finalize());

        let input: Vec<u8> = b"Z"
            .repeat(48)
            .into_iter()
            .chain(b"01234567890abcdefg".iter().copied())
            .collect();
        for i in 0..17 {
            let payload = &input[0..input.len() - i];
            let non_iterative = Wyhash::hash(0, payload);
            let mut wh = Wyhash::init(0);
            wh.update(payload);
            assert_eq!(non_iterative, wh.finalize());
        }
    }
}
