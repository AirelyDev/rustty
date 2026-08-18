//! CRC32C with hardware acceleration.
//!
//! The Zig standard library implementation processes one byte per table
//! lookup (as of Zig 0.16), which is more than an order of magnitude slower
//! than the dedicated CRC32C instructions available on aarch64 (CRC
//! extension) and x86_64 (SSE4.2). This module selects the best backend at
//! compile time.
//!
//! Targets without a dedicated instruction, such as WebAssembly, use a
//! custom implementation that is faster than Zig's stdlib.
//!
//! The resulting value is identical across all backends: this is the
//! iSCSI CRC32C parameter set (reflected, initial and final XOR
//! `0xFFFFFFFF`), matching `std.hash.crc.Crc32Iscsi`.
//!
//! PORT NOTE: Ports ghostty `src/crc32c.zig` (slicing-by-16 software CRC of
//! `36d8e3f77` + the interleaved-stream end state of `eb09bf829`). Zig's
//! comptime `backend`/`step` selection becomes runtime CPU-feature detection
//! (idiomatic Rust: `std::arch::is_*_feature_detected!`); the hardware
//! `step`/`updateHardware` inline-asm is expressed via the `core::arch` CRC32C
//! intrinsics, which emit the same instructions. The value is identical across
//! all backends, so the KAT/differential tests exercise every path.

/// PORT NOTE: `std.hash.crc.Crc32Iscsi` is the reference the reference tests
/// compare against. Rustty has no stdlib CRC32C, so a straightforward
/// bit-reflected byte-at-a-time implementation of the SAME parameter set
/// (Castagnoli reflected polynomial, init/final XOR `0xFFFFFFFF`) stands in.
/// It is pinned to the catalog check value by the `matches the check value`
/// test through `Crc32c`, and the differential tests prove `Software`/`Crc32c`
/// agree with it byte-for-byte at every length, split, and alignment.
#[cfg(test)]
struct Reference {
    crc: u32,
}

#[cfg(test)]
impl Reference {
    fn update(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.crc ^= byte as u32;
            for _ in 0..8 {
                self.crc = (self.crc >> 1) ^ (Software::REFLECTED_POLY * (self.crc & 1));
            }
        }
    }

    fn hash(bytes: &[u8]) -> u32 {
        let mut r = Reference { crc: 0xFFFF_FFFF };
        r.update(bytes);
        r.crc ^ 0xFFFF_FFFF
    }
}

/// Streaming CRC32C with the same interface shape as `std.hash.crc` types.
#[derive(Debug, Clone, Copy)]
pub struct Crc32c {
    crc: u32,
}

impl Crc32c {
    pub fn init() -> Crc32c {
        Crc32c { crc: 0xFFFF_FFFF }
    }

    pub fn update(&mut self, bytes: &[u8]) {
        self.crc = update_dispatch(self.crc, bytes);
    }

    pub fn r#final(self) -> u32 {
        self.crc ^ 0xFFFF_FFFF
    }

    pub fn hash(bytes: &[u8]) -> u32 {
        let mut c = Crc32c::init();
        c.update(bytes);
        c.r#final()
    }
}

impl Default for Crc32c {
    fn default() -> Crc32c {
        Crc32c::init()
    }
}

// PORT NOTE: Zig selects `backend` at comptime and dispatches `update` on it.
// Rust selects at runtime with CPU-feature detection so a single binary
// benefits from the hardware instruction whenever the running CPU has it,
// falling back to the portable `Software` backend otherwise. Exactly one
// `update_dispatch` compiles per target configuration.

/// One update pass on aarch64: the CRC extension when present, else software.
#[cfg(all(target_arch = "aarch64", not(miri)))]
fn update_dispatch(crc: u32, bytes: &[u8]) -> u32 {
    if std::arch::is_aarch64_feature_detected!("crc") {
        // SAFETY(ghostty:crc32c.zig updateHardware / rustty-lib:crc32c): the
        // `crc` target feature is confirmed present on the running CPU by the
        // `is_aarch64_feature_detected!` guard immediately above, which is the
        // precondition for calling the `#[target_feature(enable = "crc")]`
        // function.
        unsafe { update_hardware_aarch64(crc, bytes) }
    } else {
        Software::update(crc, bytes)
    }
}

/// One update pass on x86_64: SSE4.2 CRC32 when present, else software.
#[cfg(all(target_arch = "x86_64", not(miri)))]
fn update_dispatch(crc: u32, bytes: &[u8]) -> u32 {
    if std::arch::is_x86_feature_detected!("sse4.2") {
        // SAFETY(ghostty:crc32c.zig updateHardware / rustty-lib:crc32c): the
        // `sse4.2` target feature is confirmed present on the running CPU by
        // the `is_x86_feature_detected!` guard immediately above, which is the
        // precondition for calling the `#[target_feature(enable = "sse4.2")]`
        // function.
        unsafe { update_hardware_x86_64(crc, bytes) }
    } else {
        Software::update(crc, bytes)
    }
}

/// One update pass on targets without a dedicated CRC32C instruction.
#[cfg(not(any(
    all(target_arch = "aarch64", not(miri)),
    all(target_arch = "x86_64", not(miri))
)))]
fn update_dispatch(crc: u32, bytes: &[u8]) -> u32 {
    Software::update(crc, bytes)
}

/// One update pass using the dedicated CRC32C instructions. Both supported
/// architectures handle unaligned loads efficiently, so the loop reads
/// little-endian words directly from the input.
#[cfg(all(target_arch = "aarch64", not(miri)))]
#[target_feature(enable = "crc")]
unsafe fn update_hardware_aarch64(initial: u32, bytes: &[u8]) -> u32 {
    use core::arch::aarch64::{__crc32cb, __crc32cd, __crc32cw};

    let mut crc = initial;
    let mut remaining = bytes;

    while remaining.len() >= 8 {
        let value = u64::from_le_bytes(remaining[0..8].try_into().unwrap());
        crc = __crc32cd(crc, value);
        remaining = &remaining[8..];
    }
    if remaining.len() >= 4 {
        let value = u32::from_le_bytes(remaining[0..4].try_into().unwrap());
        crc = __crc32cw(crc, value);
        remaining = &remaining[4..];
    }
    for &byte in remaining {
        crc = __crc32cb(crc, byte);
    }
    crc
}

/// One update pass using the dedicated CRC32C instructions. Both supported
/// architectures handle unaligned loads efficiently, so the loop reads
/// little-endian words directly from the input.
#[cfg(all(target_arch = "x86_64", not(miri)))]
#[target_feature(enable = "sse4.2")]
unsafe fn update_hardware_x86_64(initial: u32, bytes: &[u8]) -> u32 {
    use core::arch::x86_64::{_mm_crc32_u32, _mm_crc32_u64, _mm_crc32_u8};

    let mut crc = initial;
    let mut remaining = bytes;

    while remaining.len() >= 8 {
        let value = u64::from_le_bytes(remaining[0..8].try_into().unwrap());
        crc = _mm_crc32_u64(crc as u64, value) as u32;
        remaining = &remaining[8..];
    }
    if remaining.len() >= 4 {
        let value = u32::from_le_bytes(remaining[0..4].try_into().unwrap());
        crc = _mm_crc32_u32(crc, value);
        remaining = &remaining[4..];
    }
    for &byte in remaining {
        crc = _mm_crc32_u8(crc, byte);
    }
    crc
}

/// The portable software backend, used by targets without a dedicated
/// CRC32C instruction.
struct Software;

impl Software {
    /// The reflected CRC32C (Castagnoli) polynomial.
    const REFLECTED_POLY: u32 = 0x82F6_3B78;

    /// Number of slicing tables, which is also the bytes folded per
    /// iteration.
    const SLICES: usize = 16;

    /// Inputs below this length use the single-stream pass: the
    /// stream-combine matrix work would not pay for itself.
    const MULTI_STREAM_THRESHOLD: usize = 4096;

    /// Slicing tables: `tables[i][b]` is the CRC of byte `b` followed by
    /// `i` zero bytes. Table zero is the classic one-byte-per-step table;
    /// the higher tables let one iteration fold a whole block with
    /// independent lookups instead of a byte-by-byte dependency chain.
    const TABLES: [[u32; 256]; Software::SLICES] = {
        let mut result = [[0u32; 256]; Software::SLICES];
        let mut n = 0;
        while n < 256 {
            let mut crc: u32 = n as u32;
            let mut b = 0;
            while b < 8 {
                crc = (crc >> 1) ^ (Software::REFLECTED_POLY * (crc & 1));
                b += 1;
            }
            result[0][n] = crc;
            n += 1;
        }
        let mut i = 1;
        while i < Software::SLICES {
            let mut n = 0;
            while n < 256 {
                let prev = result[i - 1][n];
                result[i][n] = (prev >> 8) ^ result[0][(prev & 0xFF) as usize];
                n += 1;
            }
            i += 1;
        }
        result
    };

    fn update(initial: u32, bytes: &[u8]) -> u32 {
        if bytes.len() >= Software::MULTI_STREAM_THRESHOLD {
            return Software::update_multi(initial, bytes);
        }

        Software::update_single(initial, bytes)
    }

    /// One single-stream update pass using slicing.
    fn update_single(initial: u32, bytes: &[u8]) -> u32 {
        let t = &Software::TABLES;
        let mut crc = initial;
        let mut i: usize = 0;
        while i + Software::SLICES <= bytes.len() {
            crc = Software::fold_chunk(bytes, i, crc);
            i += Software::SLICES;
        }
        for &byte in &bytes[i..] {
            crc = (crc >> 8) ^ t[0][((crc ^ byte as u32) & 0xFF) as usize];
        }
        crc
    }

    /// One update pass as three independent interleaved streams. Faster
    /// for large enough inputs.
    fn update_multi(initial: u32, bytes: &[u8]) -> u32 {
        // Both leading parts are block multiples so the interleaved loop
        // needs no tail handling; the third part absorbs the remainder.
        let part = (bytes.len() / 3) & !(Software::SLICES - 1);
        let p0 = &bytes[0..part];
        let p1 = &bytes[part..part + part];
        let p2 = &bytes[2 * part..];

        let mut s0 = initial;
        let mut s1: u32 = 0;
        let mut s2: u32 = 0;
        let mut i: usize = 0;
        while i + Software::SLICES <= part {
            s0 = Software::fold_chunk(p0, i, s0);
            s1 = Software::fold_chunk(p1, i, s1);
            s2 = Software::fold_chunk(p2, i, s2);
            i += Software::SLICES;
        }
        s2 = Software::update_single(s2, &p2[part..]);

        let s01 = s1 ^ Software::zero_shift(s0, p1.len());
        s2 ^ Software::zero_shift(s01, p2.len())
    }

    /// Fold one aligned block through the per-position slicing tables.
    /// The running CRC must already be XORed into the block's first word.
    #[inline]
    fn fold_block(words: &[u32; Software::SLICES / 4]) -> u32 {
        let t = &Software::TABLES;
        let mut crc: u32 = 0;
        let mut w = 0;
        while w < Software::SLICES / 4 {
            let word = words[w];
            let base = Software::SLICES - 1 - w * 4;
            crc ^= t[base][(word & 0xFF) as usize]
                ^ t[base - 1][((word >> 8) & 0xFF) as usize]
                ^ t[base - 2][((word >> 16) & 0xFF) as usize]
                ^ t[base - 3][(word >> 24) as usize];
            w += 1;
        }
        crc
    }

    /// Fold the block starting at `offset`, chaining the running CRC state.
    #[inline]
    fn fold_chunk(bytes: &[u8], offset: usize, crc: u32) -> u32 {
        let mut words = [0u32; Software::SLICES / 4];
        let mut w = 0;
        while w < Software::SLICES / 4 {
            words[w] =
                u32::from_le_bytes(bytes[offset + w * 4..offset + w * 4 + 4].try_into().unwrap());
            w += 1;
        }
        words[0] ^= crc;
        Software::fold_block(&words)
    }

    /// Advance a CRC state as if `len` zero bytes had been processed.
    fn zero_shift(state: u32, len: usize) -> u32 {
        let mut s = state;
        let mut remaining = len;
        let mut k: usize = 0;
        while remaining != 0 {
            if remaining & 1 != 0 {
                let mat = &Software::ZERO_SHIFT_MATRICES[k / 2];
                s = Software::mat_times_vec(mat, s);
                // (clippy `manual_is_multiple_of`, rust 1.93: the odd-`k`
                // check spelled through `is_multiple_of` — same predicate.)
                if !k.is_multiple_of(2) {
                    s = Software::mat_times_vec(mat, s);
                }
            }
            remaining >>= 1;
            k += 1;
        }
        s
    }

    /// Multiply the GF(2) matrix by a CRC state column vector.
    #[inline]
    const fn mat_times_vec(mat: &[u32; 32], vec: u32) -> u32 {
        let mut sum: u32 = 0;
        let mut v = vec;
        let mut i: usize = 0;
        while v != 0 {
            if v & 1 != 0 {
                sum ^= mat[i];
            }
            v >>= 1;
            i += 1;
        }
        sum
    }

    const ZERO_SHIFT_MATRICES: [[u32; 32]; 32] = {
        let mut matrices = [[0u32; 32]; 32];
        let mut previous = [0u32; 32];
        let mut i = 0;
        while i < 32 {
            let unit: u32 = 1 << i;
            previous[i] = (unit >> 8) ^ Software::TABLES[0][(unit & 0xFF) as usize];
            i += 1;
        }
        matrices[0] = previous;
        let mut k = 1;
        while k < 64 {
            let mut squared = [0u32; 32];
            let mut i = 0;
            while i < 32 {
                squared[i] = Software::mat_times_vec(&previous, previous[i]);
                i += 1;
            }
            previous = squared;
            if k % 2 == 0 {
                matrices[k / 2] = squared;
            }
            k += 1;
        }
        matrices
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PORT NOTE: `std.Random.DefaultPrng.init(seed)` fills the differential
    /// tests' input buffers. The exact byte values are irrelevant — every
    /// assertion compares two implementations over the SAME bytes — so a small
    /// deterministic `SplitMix64` stands in for the specific xoshiro algorithm,
    /// preserving the "well-distributed pseudo-random coverage" intent.
    struct DefaultPrng {
        state: u64,
    }

    impl DefaultPrng {
        fn init(seed: u64) -> DefaultPrng {
            DefaultPrng { state: seed }
        }

        fn next(&mut self) -> u64 {
            self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = self.state;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        }

        fn bytes(&mut self, buf: &mut [u8]) {
            for chunk in buf.chunks_mut(8) {
                let word = self.next().to_le_bytes();
                chunk.copy_from_slice(&word[..chunk.len()]);
            }
        }
    }

    #[test]
    fn software_slicing_matches_the_standard_library() {
        // The selected backend may be hardware, so cover the sliced software
        // path directly: every length around the sixteen-byte boundary, several
        // alignments, and continuation across arbitrary split points.
        let mut bytes = [0u8; 512 + 19];
        let mut prng = DefaultPrng::init(0x511C);
        prng.bytes(&mut bytes);

        for len in 0..64 + 1 {
            for offset in 0..4 {
                let input = &bytes[offset..offset + len];
                let mut reference = Reference { crc: 0xFFFF_FFFF };
                reference.update(input);
                assert_eq!(reference.crc, Software::update(0xFFFF_FFFF, input));
            }
        }

        let long = &bytes[0..512];
        let mut reference = Reference { crc: 0xFFFF_FFFF };
        reference.update(long);
        for split in [0usize, 1, 15, 16, 17, 100, 511, 512] {
            let first = Software::update(0xFFFF_FFFF, &long[0..split]);
            assert_eq!(reference.crc, Software::update(first, &long[split..]));
        }
    }

    #[test]
    fn software_multi_stream_matches_the_standard_library() {
        // Lengths around and far above the interleaving threshold, plus odd
        // remainders, so all three streams and both combine steps are covered.
        let mut bytes = [0u8; 96 * 1024];
        let mut prng = DefaultPrng::init(0x3517_1A3B);
        prng.bytes(&mut bytes);

        for len in [
            Software::MULTI_STREAM_THRESHOLD - 1,
            Software::MULTI_STREAM_THRESHOLD,
            Software::MULTI_STREAM_THRESHOLD + 1,
            Software::MULTI_STREAM_THRESHOLD + 97,
            12 * 1024,
            64 * 1024 + 31,
            bytes.len(),
        ] {
            let input = &bytes[0..len];
            let mut reference = Reference { crc: 0xFFFF_FFFF };
            reference.update(input);
            assert_eq!(reference.crc, Software::update(0xFFFF_FFFF, input));

            // Continuation across a split inside the multi-stream range.
            let first = Software::update(0xFFFF_FFFF, &input[0..len / 2]);
            assert_eq!(reference.crc, Software::update(first, &input[len / 2..]));
        }
    }

    #[test]
    fn matches_the_check_value() {
        // The catalog check value for CRC-32/ISCSI.
        assert_eq!(0xE306_9283u32, Crc32c::hash(b"123456789"));
    }

    #[test]
    fn matches_the_standard_library_at_every_length_and_split() {
        let mut bytes = [0u8; 259];
        let mut prng = DefaultPrng::init(0xC5_C32C);
        prng.bytes(&mut bytes);

        for len in 0..bytes.len() + 1 {
            let input = &bytes[0..len];
            assert_eq!(Reference::hash(input), Crc32c::hash(input));

            // Streaming across arbitrary split points must not change the
            // result: word batching may not leak state between updates.
            let mut split = Crc32c::init();
            split.update(&input[0..len / 3]);
            split.update(&input[len / 3..len - len / 3]);
            split.update(&input[len - len / 3..]);
            assert_eq!(Reference::hash(input), split.r#final());
        }
    }

    #[test]
    fn matches_the_standard_library_at_every_alignment() {
        let mut bytes = [0u8; 64 + 16];
        let mut prng = DefaultPrng::init(0xA11C);
        prng.bytes(&mut bytes);

        for offset in 0..16 {
            let input = &bytes[offset..offset + 64];
            assert_eq!(Reference::hash(input), Crc32c::hash(input));
        }
    }
}
