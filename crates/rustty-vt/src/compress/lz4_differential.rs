//! Differential and property tests for the LZ4 block codec.
//!
//! Every generated input must compress into a block which:
//!
//!   1. fits within `compress_bound`,
//!   2. is structurally valid LZ4 with the stricter guarantees our
//!      compressor documents (final five bytes are literals, matches start
//!      at least twelve bytes before the end), verified by an independent
//!      walker which shares no code with the codec,
//!   3. decompresses to exactly the original bytes, and
//!   4. is rejected when decompressed into a buffer of the wrong size.
//!
//! Valid blocks are additionally mutated (bit flips, splices, truncations)
//! and fed to the decompressor, which must fail cleanly or succeed, but
//! never read or write out of bounds. Rust's slice bounds checks are always
//! active in the test build, so any out-of-bounds access fails the test.
//!
//! The light suite below runs as a normal unit test and finishes quickly.
//! The exhaustive suite multiplies the same properties across far more
//! sizes, periods, seeds, and mutations; it is slow and therefore skipped
//! unless the environment variable `RUSTTY_LZ4_SLOW` is set:
//!
//!     RUSTTY_LZ4_SLOW=1 cargo test -p rustty-vt --lib lz4_differential --release
//!
//! PORTS: ghostty src/terminal/compress/lz4_differential.zig (oracle
//! origin/main @ 9a4bd2120).
//!
//! PORT NOTE (WS7): the oracle drives its generators with
//! `std.Random.DefaultPrng` (xoshiro256++ seeded through SplitMix64). That
//! generator is reimplemented here so the suite stays deterministic and
//! allocation-light without adding a `rand` dependency. The exact byte stream
//! is not part of the property being tested — every assertion is structural.
//!
//! PORT NOTE (WS7): `GHOSTTY_LZ4_SLOW` -> `RUSTTY_LZ4_SLOW` (R2 casing rule).
//! The oracle expresses "skip" as `error.SkipZigTest`; Rust's test harness has
//! no runtime skip, so the exhaustive test returns early instead.

use super::lz4::{self, DecompressError, HashTable};

/// Number of trailing block bytes which must be literals, mirroring the
/// documented guarantee of the compressor. Kept as an independent constant
/// so a codec regression cannot silently weaken the check.
const LAST_LITERALS: usize = 5;

/// A match may not begin in the final twelve bytes. See `LAST_LITERALS`.
const MATCH_FIND_LIMIT: usize = 12;

/// Sizes around every encoding boundary: token nibble limits (14/15),
/// minimum match and find limits (4/12), length-extension steps (15 + 255k),
/// and power-of-two neighborhoods.
const BOUNDARY_SIZES: [usize; 38] = [
    0, 1, 2, 3, 4, 5, 6, 7, //
    8, 9, 11, 12, 13, 14, 15, 16, //
    17, 18, 19, 20, 31, 32, 33, 63, //
    64, 65, 254, 255, 256, 269, 270, 271, //
    272, 1023, 1024, 4095, 4096, 4097,
];

// -- Deterministic PRNG ----------------------------------------------------
//
// PORT NOTE: this is Zig's `std.Random.DefaultPrng` (xoshiro256++) with its
// SplitMix64 seeding, plus the handful of `std.Random` helpers the oracle's
// generators call.

struct Prng {
    s: [u64; 4],
}

impl Prng {
    fn init(seed: u64) -> Prng {
        let mut x = seed;
        let mut s = [0u64; 4];
        for slot in s.iter_mut() {
            x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = x;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            *slot = z ^ (z >> 31);
        }
        Prng { s }
    }

    fn next(&mut self) -> u64 {
        let result = self.s[0]
            .wrapping_add(self.s[3])
            .rotate_left(23)
            .wrapping_add(self.s[0]);
        let t = self.s[1] << 17;
        self.s[2] ^= self.s[0];
        self.s[3] ^= self.s[1];
        self.s[1] ^= self.s[2];
        self.s[0] ^= self.s[3];
        self.s[2] ^= t;
        self.s[3] = self.s[3].rotate_left(45);
        result
    }

    fn int_u8(&mut self) -> u8 {
        self.next() as u8
    }

    fn bytes(&mut self, buf: &mut [u8]) {
        for byte in buf.iter_mut() {
            *byte = self.next() as u8;
        }
    }

    /// Uniform over `0..less_than`. `less_than` must be nonzero.
    fn uint_less_than(&mut self, less_than: usize) -> usize {
        debug_assert!(less_than > 0);
        (self.next() % less_than as u64) as usize
    }

    fn uint_at_most(&mut self, at_most: usize) -> usize {
        self.uint_less_than(at_most + 1)
    }

    fn int_range_at_most(&mut self, at_least: usize, at_most: usize) -> usize {
        at_least + self.uint_less_than(at_most - at_least + 1)
    }

    fn boolean(&mut self) -> bool {
        self.next() & 1 == 1
    }

    /// Uniform in `[0, 1)`, matching `std.Random.float(f64)`.
    fn float(&mut self) -> f64 {
        (self.next() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    fn enum_value(&mut self) -> Generator {
        Generator::ALL[self.uint_less_than(Generator::ALL.len())]
    }
}

/// Input generators exercising distinct codec behaviors. Every generator is
/// deterministic for a given random state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Generator {
    /// Uniform random bytes; largely incompressible.
    RandomBytes,

    /// Runs of one repeated byte with random lengths; period-one matches.
    Runs,

    /// One repeating pattern; exercises a fixed match period end to end.
    Periodic,

    /// Eight-byte records with random small payloads and zero padding, with
    /// some records repeated; resembles terminal cell memory.
    Cells,

    /// Dictionary words with separators; text-like literal/match mix.
    Words,

    /// Mostly zeros with scattered random bytes; long matches with isolated
    /// literals.
    Sparse,

    /// Random segments of all other generators; exercises transitions.
    Mixed,
}

impl Generator {
    const ALL: [Generator; 7] = [
        Generator::RandomBytes,
        Generator::Runs,
        Generator::Periodic,
        Generator::Cells,
        Generator::Words,
        Generator::Sparse,
        Generator::Mixed,
    ];

    fn fill(self, random: &mut Prng, buf: &mut [u8]) {
        match self {
            Generator::RandomBytes => random.bytes(buf),

            Generator::Runs => {
                let mut i: usize = 0;
                while i < buf.len() {
                    let run = core::cmp::min(random.int_range_at_most(1, 300), buf.len() - i);
                    let byte = random.int_u8();
                    buf[i..i + run].fill(byte);
                    i += run;
                }
            }

            Generator::Periodic => {
                let period = random.int_range_at_most(1, 40);
                fill_periodic(random, buf, period);
            }

            Generator::Cells => {
                let mut i: usize = 0;
                while i + 8 <= buf.len() {
                    if i >= 8 && random.boolean() {
                        // Repeat one of the last 256 records.
                        let back = 8 * random.int_range_at_most(1, core::cmp::min(i / 8, 256));
                        buf.copy_within(i - back..i - back + 8, i);
                    } else {
                        buf[i..i + 8].fill(0);
                        buf[i] = b' ' + (random.next() % 95) as u8;
                        buf[i + 1] = (random.next() % 4) as u8;
                    }
                    i += 8;
                }
                random.bytes(&mut buf[i..]);
            }

            Generator::Words => {
                const WORDS: [&[u8]; 24] = [
                    b"the",
                    b"terminal",
                    b"page",
                    b"compress",
                    b"row",
                    b"cell",
                    b"style",
                    b"zig",
                    b"lz4",
                    b"block",
                    b"offset",
                    b"match",
                    b"a",
                    b"of",
                    b"and",
                    b"literal",
                    b"0x00",
                    b"0xFF",
                    b"    ",
                    b"\r\n",
                    b"-----",
                    b"=",
                    b"pub fn",
                    b"const",
                ];
                let mut i: usize = 0;
                while i < buf.len() {
                    let word = WORDS[random.uint_less_than(WORDS.len())];
                    let n = core::cmp::min(word.len(), buf.len() - i);
                    buf[i..i + n].copy_from_slice(&word[..n]);
                    i += n;
                    if i < buf.len() {
                        buf[i] = if random.boolean() { b' ' } else { b'\n' };
                        i += 1;
                    }
                }
            }

            Generator::Sparse => {
                buf.fill(0);
                if buf.is_empty() {
                    return;
                }
                for _ in 0..buf.len() / 32 + 1 {
                    let at = random.uint_less_than(buf.len());
                    buf[at] = random.int_u8();
                }
            }

            Generator::Mixed => {
                let mut i: usize = 0;
                while i < buf.len() {
                    let segment = core::cmp::min(random.int_range_at_most(1, 2048), buf.len() - i);
                    let sub = random.enum_value();
                    if sub != Generator::Mixed {
                        sub.fill(random, &mut buf[i..i + segment]);
                    }
                    i += segment;
                }
            }
        }
    }
}

/// Fill `buf` with one repeating pattern of the given period.
fn fill_periodic(random: &mut Prng, buf: &mut [u8], period: usize) {
    if buf.is_empty() {
        return;
    }
    let head = core::cmp::min(period, buf.len());
    random.bytes(&mut buf[..head]);
    for i in head..buf.len() {
        buf[i] = buf[i - period];
    }
}

/// Reusable buffers sized for the largest input a suite generates.
struct Workspace {
    input: Vec<u8>,
    encoded: Vec<u8>,
    decoded: Vec<u8>,
    table: Box<HashTable>,
}

impl Workspace {
    fn init(max_input: usize) -> Workspace {
        Workspace {
            input: vec![0u8; max_input],
            encoded: vec![0u8; lz4::compress_bound(max_input).unwrap()],
            // One extra byte so wrong-size decompression can be tested above
            // the exact length as well as below it.
            decoded: vec![0u8; max_input + 1],
            table: Box::new([0u32; 4096]),
        }
    }
}

/// Compress one input and verify every property promised by the codec.
/// Returns the encoded length so callers can reuse the encoded block.
///
/// PORT NOTE: the oracle passes the input slice; here the caller passes its
/// length so the input and the scratch buffers stay disjoint fields of the
/// same workspace instead of an alias of one of them.
fn expect_codec_properties(ws: &mut Workspace, input_len: usize) -> usize {
    let encoded_len =
        lz4::compress(&ws.input[..input_len], &mut ws.encoded, &mut ws.table).unwrap();
    assert!(encoded_len <= lz4::compress_bound(input_len).unwrap());

    expect_valid_block(&ws.encoded[..encoded_len], input_len);

    // Exact-size decompression must reproduce the input bit for bit. The
    // output is poisoned first so unwritten bytes cannot pass as correct.
    ws.decoded[..input_len].fill(0xAA);
    assert_eq!(
        lz4::decompress(&ws.encoded[..encoded_len], &mut ws.decoded[..input_len]),
        Ok(input_len)
    );
    assert_eq!(ws.input[..input_len], ws.decoded[..input_len]);

    // The exact-size contract must reject both smaller and larger buffers.
    if input_len > 0 {
        assert_eq!(
            lz4::decompress(&ws.encoded[..encoded_len], &mut ws.decoded[..input_len - 1]),
            Err(DecompressError::OutputTooSmall)
        );
    }
    assert_eq!(
        lz4::decompress(&ws.encoded[..encoded_len], &mut ws.decoded[..input_len + 1]),
        Err(DecompressError::OutputSizeMismatch)
    );

    encoded_len
}

/// Structurally validate one encoded block against the LZ4 block format and
/// the stricter guarantees documented by our compressor. This deliberately
/// reimplements the format rather than reusing codec internals.
fn expect_valid_block(encoded: &[u8], raw_len: usize) {
    let mut ip: usize = 0;
    let mut op: usize = 0;
    let mut last_match_end: usize = 0;

    loop {
        // Every sequence, including the final one, starts with a token.
        assert!(ip < encoded.len());
        let token = encoded[ip];
        ip += 1;

        let mut literal_len: usize = (token >> 4) as usize;
        if literal_len == 15 {
            literal_len += read_extension(encoded, &mut ip);
        }
        assert!(encoded.len() - ip >= literal_len);
        ip += literal_len;
        op += literal_len;

        // The final sequence contains only literals and ends the block.
        if ip == encoded.len() {
            assert_eq!(raw_len, op);
            if last_match_end > 0 {
                assert!(raw_len - last_match_end >= LAST_LITERALS);
            }
            return;
        }

        assert!(encoded.len() - ip >= 2);
        let offset = u16::from_le_bytes([encoded[ip], encoded[ip + 1]]) as usize;
        ip += 2;
        assert!(offset >= 1);
        assert!(offset <= op);

        // Our compressor starts matches at least `MATCH_FIND_LIMIT` bytes
        // before the end and never lets one run into the final literals.
        assert!(op + MATCH_FIND_LIMIT <= raw_len);
        let mut match_len: usize = (token & 0x0F) as usize + 4;
        if token & 0x0F == 15 {
            match_len += read_extension(encoded, &mut ip);
        }
        op += match_len;
        assert!(op + LAST_LITERALS <= raw_len);
        last_match_end = op;
    }
}

/// Read one length extension: bytes of 255 accumulate until a terminator
/// below 255, which is included in the sum.
fn read_extension(encoded: &[u8], ip: &mut usize) -> usize {
    let mut total: usize = 0;
    loop {
        assert!(*ip < encoded.len());
        let value = encoded[*ip];
        *ip += 1;
        total += value as usize;
        if value != 255 {
            return total;
        }
    }
}

/// Decode deterministic corruptions of a valid block. Any result is
/// acceptable except memory unsafety, which Rust's always-on slice bounds
/// checks turn into a failure. Mutations may still form a valid block, so
/// output contents are intentionally not asserted.
fn expect_mutation_safety(
    ws: &mut Workspace,
    random: &mut Prng,
    encoded_len: usize,
    raw_len: usize,
    mutations: usize,
) {
    let original: Vec<u8> = ws.encoded[..encoded_len].to_vec();

    for _ in 0..mutations {
        ws.encoded[..encoded_len].copy_from_slice(&original);
        let block_len = encoded_len;

        match random.uint_less_than(4) {
            // Flip up to eight random bits.
            0 => {
                for _ in 0..random.int_range_at_most(1, 8) {
                    let at = random.uint_less_than(block_len);
                    let bit = (random.next() % 8) as u32;
                    ws.encoded[at] ^= 1u8 << bit;
                }
            }
            // Overwrite one random byte. Token and length bytes are the
            // most interesting targets and small blocks are mostly tokens.
            1 => {
                let at = random.uint_less_than(block_len);
                ws.encoded[at] = random.int_u8();
            }
            // Splice random garbage over a random span.
            2 => {
                let at = random.uint_less_than(block_len);
                let span = core::cmp::min(random.int_range_at_most(1, 16), block_len - at);
                random.bytes(&mut ws.encoded[at..at + span]);
            }
            // Decode a random prefix of the intact block.
            _ => {}
        }

        let len = if random.boolean() {
            random.uint_at_most(block_len)
        } else {
            block_len
        };
        let _ = lz4::decompress(&ws.encoded[..len], &mut ws.decoded[..raw_len]);
    }
}

/// Workload knobs shared by the light and exhaustive suites.
struct Budget {
    /// Upper bound and step of the contiguous small-size sweep applied to
    /// every generator.
    sweep_max: usize,
    sweep_step: usize,

    /// Number and maximum size of random-parameter inputs.
    random_inputs: usize,
    random_max: usize,

    /// Explicit match periods checked with `fill_periodic`.
    periods: &'static [usize],
    period_len: usize,

    /// Number of corrupted decode attempts per mutation base block.
    mutations: usize,
}

impl Budget {
    fn max_input(&self) -> usize {
        core::cmp::max(
            self.random_max,
            core::cmp::max(self.period_len, BOUNDARY_SIZES[BOUNDARY_SIZES.len() - 1]),
        )
    }
}

const LIGHT_BUDGET: Budget = Budget {
    sweep_max: 96,
    sweep_step: 1,
    random_inputs: 24,
    random_max: 32 * 1024,
    // One period per copy strategy: byte propagation (3), pattern words
    // (1/2/4/8), word strides (9), wide strides (17), plus the 64 KiB
    // window edge cases.
    periods: &[1, 2, 3, 4, 8, 9, 17, 65534, 65535, 65536, 65537],
    period_len: 160 * 1024,
    mutations: 64,
};

const EXHAUSTIVE_BUDGET: Budget = Budget {
    sweep_max: 2048,
    sweep_step: 1,
    random_inputs: 512,
    random_max: 512 * 1024,
    periods: &[
        1, 2, 3, 4, 5, 6, 7, 8, //
        9, 10, 11, 12, 13, 14, 15, 16, //
        17, 18, 19, 23, 24, 31, 32, 33, //
        48, 63, 64, 65, 127, 128, 255, 256, //
        257, 4095, 4096, 32768, 65533, 65534, 65535, 65536, //
        65537, 65538,
    ],
    period_len: 320 * 1024,
    mutations: 4096,
};

fn run_suite(budget: &Budget, seed: u64) {
    let mut random = Prng::init(seed);
    let mut ws = Workspace::init(budget.max_input());

    // Every generator across every boundary size and the small-size sweep.
    // Small inputs hit the literal-only format edges: below the minimum
    // match sizes, around nibble limits, and around extension steps.
    for gen in Generator::ALL {
        for size in BOUNDARY_SIZES {
            gen.fill(&mut random, &mut ws.input[..size]);
            let _ = expect_codec_properties(&mut ws, size);
        }

        let mut size: usize = 0;
        while size <= budget.sweep_max {
            gen.fill(&mut random, &mut ws.input[..size]);
            let _ = expect_codec_properties(&mut ws, size);
            size += budget.sweep_step;
        }
    }

    // Random generator and size pairs, biased toward interesting sizes by
    // squaring so both tiny and large inputs appear.
    for _ in 0..budget.random_inputs {
        let gen = random.enum_value();
        let scale = random.float();
        let size: usize = (scale * scale * budget.random_max as f64) as usize;
        gen.fill(&mut random, &mut ws.input[..size]);
        let encoded_len = expect_codec_properties(&mut ws, size);

        // Reuse a handful of these encodings as corruption bases.
        expect_mutation_safety(
            &mut ws,
            &mut random,
            encoded_len,
            size,
            budget.mutations / budget.random_inputs + 1,
        );
    }

    // Directed match periods, including the 64 KiB window edge where the
    // compressor's 16-bit position arithmetic wraps.
    for &period in budget.periods {
        let len = budget.period_len;
        fill_periodic(&mut random, &mut ws.input[..len], period);
        let _ = expect_codec_properties(&mut ws, len);
    }

    // Dedicated corruption run over a text-like block, plus an exhaustive
    // truncation sweep: every prefix of a valid block must decode cleanly
    // or fail cleanly.
    {
        let len = core::cmp::min(16 * 1024, budget.random_max);
        Generator::Words.fill(&mut random, &mut ws.input[..len]);
        let encoded_len = expect_codec_properties(&mut ws, len);
        expect_mutation_safety(&mut ws, &mut random, encoded_len, len, budget.mutations);

        for prefix in 0..encoded_len {
            let _ = lz4::decompress(&ws.encoded[..prefix], &mut ws.decoded[..len]);
        }
    }
}

#[test]
fn lz4_differential_light() {
    run_suite(&LIGHT_BUDGET, 0x4C5A_3403);
}

#[test]
fn lz4_differential_exhaustive() {
    // Slow. Enable explicitly, ideally together with a test filter:
    //   RUSTTY_LZ4_SLOW=1 cargo test -p rustty-vt --lib lz4_differential --release
    //
    // PORT NOTE: the oracle returns `error.SkipZigTest`; Rust's harness has no
    // runtime skip, so an unset gate makes this test a no-op.
    if std::env::var_os("RUSTTY_LZ4_SLOW").is_none() {
        return;
    }

    // Several independent seeds; the suite is deterministic per seed.
    for seed in 0..4u64 {
        run_suite(&EXHAUSTIVE_BUDGET, 0x4C5A_4000 + seed);
    }
}
