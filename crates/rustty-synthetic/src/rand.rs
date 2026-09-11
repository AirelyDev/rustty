//! Faithful port of the subset of `std.Random` that ghostty's `synthetic`
//! package leans on. This is the ONE module in the crate with no direct
//! upstream `synthetic/*.zig` counterpart: it reproduces, bit-for-bit, the
//! Zig 0.15.2 stdlib facilities the generators are driven by —
//! `std.Random.DefaultPrng` (Xoshiro256++), its `SplitMix64` seeder, and the
//! `std.Random` helper methods (`bytes`, `boolean`, `int`, `uintLessThan`,
//! `intRangeAtMost`, `intRangeAtMostBiased`, `float`, `weightedIndex`).
//!
//! R1 requires bit-faithfulness: the OSC "valid/invalid" tests and the Utf8
//! length distribution depend on the EXACT branch selection the RNG drives, so
//! the algorithm is ported verbatim rather than delegating to the `rand` crate.
//! Little-endian byte order, the Lemire rejection loop's draw count, the
//! `float` leading-zero loop, and the `weightedIndex` float path are all
//! load-bearing.
//!
//! Shared-handle modelling: Zig passes `std.Random` (a `{ptr, fillFn}` vtable)
//! by value; a `*Xoshiro256`'s `.random()` yields a handle that mutates the
//! shared prng state on every draw, and several call sites copy that handle by
//! value while sharing the one generator. We model `Rng` as a `Copy` handle
//! wrapping `&RefCell<Xoshiro256>` — zero `unsafe`, trivially Miri-clean, and
//! bit-identical to the Zig by-value vtable that aliases one generator.

use std::cell::RefCell;

// ===========================================================================
// SplitMix64 — std/Random/SplitMix64.zig
// ===========================================================================

/// Generator to extend 64-bit seed values into longer sequences.
///
/// The number of cycles is thus limited to 64-bits regardless of the engine, but this
/// is still plenty for practical purposes.
pub struct SplitMix64 {
    s: u64,
}

impl SplitMix64 {
    pub fn init(seed: u64) -> SplitMix64 {
        SplitMix64 { s: seed }
    }

    // PORT NOTE: faithfully named `next` (not `Iterator::next`).
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> u64 {
        self.s = self.s.wrapping_add(0x9e3779b97f4a7c15);

        let mut z = self.s;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }
}

// ===========================================================================
// Xoshiro256++ — std/Random/Xoshiro256.zig
// ===========================================================================

/// Xoshiro256++ - http://xoroshiro.di.unimi.it/
///
/// PRNG
pub struct Xoshiro256 {
    s: [u64; 4],
}

/// PORT NOTE: Zig's `u256` used only by `jump()`. Modelled as four little-endian
/// 64-bit limbs (`limbs[0]` is the low word) with the bit operations `jump`
/// needs. `@as(u256, @bitCast(self.s))` maps `s[i]` to `limbs[i]`.
#[derive(Clone, Copy)]
struct U256 {
    limbs: [u64; 4],
}

impl U256 {
    fn zero() -> U256 {
        U256 { limbs: [0; 4] }
    }
    fn from_s(s: [u64; 4]) -> U256 {
        U256 { limbs: s }
    }
    fn to_s(self) -> [u64; 4] {
        self.limbs
    }
    fn is_zero(self) -> bool {
        self.limbs == [0, 0, 0, 0]
    }
    fn bit0(self) -> u64 {
        self.limbs[0] & 1
    }
    fn shr1(&mut self) {
        for i in 0..3 {
            self.limbs[i] = (self.limbs[i] >> 1) | (self.limbs[i + 1] << 63);
        }
        self.limbs[3] >>= 1;
    }
    fn xor_assign(&mut self, other: U256) {
        for i in 0..4 {
            self.limbs[i] ^= other.limbs[i];
        }
    }
}

/// PORT NOTE: `math.rotl(u64, x, k)`.
#[inline]
fn rotl(x: u64, k: u32) -> u64 {
    x.rotate_left(k)
}

impl Xoshiro256 {
    pub fn init(init_s: u64) -> Xoshiro256 {
        let mut x = Xoshiro256 { s: [0; 4] };

        x.seed(init_s);
        x
    }

    // PORT NOTE: faithfully named `next` (not `Iterator::next`).
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> u64 {
        let r = rotl(self.s[0].wrapping_add(self.s[3]), 23).wrapping_add(self.s[0]);

        let t = self.s[1] << 17;

        self.s[2] ^= self.s[0];
        self.s[3] ^= self.s[1];
        self.s[1] ^= self.s[2];
        self.s[0] ^= self.s[3];

        self.s[2] ^= t;

        self.s[3] = rotl(self.s[3], 45);

        r
    }

    // Skip 2^128 places ahead in the sequence
    pub fn jump(&mut self) {
        let mut s: U256 = U256::zero();

        let mut table: U256 = U256 {
            // 0x39abdc4529b1661ca9582618e03fc9aad5a61266f0c9392c180ec6d33cfd0aba
            limbs: [
                0x180ec6d33cfd0aba,
                0xd5a61266f0c9392c,
                0xa9582618e03fc9aa,
                0x39abdc4529b1661c,
            ],
        };

        while !table.is_zero() {
            if table.bit0() != 0 {
                s.xor_assign(U256::from_s(self.s));
            }
            let _ = self.next();
            table.shr1();
        }

        self.s = s.to_s();
    }

    pub fn seed(&mut self, init_s: u64) {
        // Xoshiro requires 256-bits of seed.
        let mut gen = SplitMix64::init(init_s);

        self.s[0] = gen.next();
        self.s[1] = gen.next();
        self.s[2] = gen.next();
        self.s[3] = gen.next();
    }

    pub fn fill(&mut self, buf: &mut [u8]) {
        let mut i: usize = 0;
        let aligned_len = buf.len() - (buf.len() & 7);

        // Complete 8 byte segments.
        while i < aligned_len {
            let mut n = self.next();
            let mut j: usize = 0;
            while j < 8 {
                buf[i + j] = n as u8;
                n >>= 8;
                j += 1;
            }
            i += 8;
        }

        // Remaining. (cuts the stream)
        if i != buf.len() {
            let mut n = self.next();
            while i < buf.len() {
                buf[i] = n as u8;
                n >>= 8;
                i += 1;
            }
        }
    }
}

// ===========================================================================
// DefaultPrng + the shared std.Random handle
// ===========================================================================

/// PORT NOTE: `std.Random.DefaultPrng = Xoshiro256`. Wraps the generator in a
/// `RefCell` so `.random()` can hand out a `Copy` handle aliasing the one
/// generator, matching Zig's by-value `std.Random` vtable.
pub struct DefaultPrng {
    inner: RefCell<Xoshiro256>,
}

impl DefaultPrng {
    pub fn init(seed: u64) -> DefaultPrng {
        DefaultPrng {
            inner: RefCell::new(Xoshiro256::init(seed)),
        }
    }

    /// PORT NOTE: `Xoshiro256.random(self) std.Random`.
    pub fn random(&self) -> Rng<'_> {
        Rng { inner: &self.inner }
    }
}

/// PORT NOTE: `std.Random` — the `{ptr, fillFn}` handle. `Copy`, so it may be freely
/// copied (as `Osc.bytes()` / `Bytes.newAlphabet` / `atMost` do) while every
/// copy draws from the one shared generator.
#[derive(Clone, Copy)]
pub struct Rng<'a> {
    inner: &'a RefCell<Xoshiro256>,
}

// ---------------------------------------------------------------------------
// Uint: the comptime-generic integer width machinery of std.Random, modelled
// over the concrete widths synthetic actually uses.
// ---------------------------------------------------------------------------

/// PORT NOTE: the `comptime T: type` integer parameter of `int`/`uintLessThan`/…
/// specialised to the unsigned widths synthetic draws (`u1, u3, u8, u21, u64,
/// usize`). `to_u128`/`from_u128` carry the value through 128-bit arithmetic;
/// `from_u128` performs the `@truncate` to `BITS` bits.
pub trait Uint: Copy {
    const BITS: u32;
    /// `comptime std.math.divCeil(u16, bits, 8)` — the byte count `int` fills.
    const CEIL_BYTES: usize;
    fn from_u128(v: u128) -> Self;
    fn to_u128(self) -> u128;
    #[inline]
    fn mask() -> u128 {
        (1u128 << Self::BITS) - 1
    }
}

/// PORT NOTE: `u1` (the type `boolean()` draws through).
#[derive(Clone, Copy)]
pub struct U1(pub u64);
/// PORT NOTE: `u3` (conemu progress state; also `Utf8Len`'s tag width).
#[derive(Clone, Copy)]
pub struct U3(pub u64);
/// PORT NOTE: `u21` (a Unicode scalar value / codepoint draw).
#[derive(Clone, Copy)]
pub struct U21(pub u64);

macro_rules! impl_uint_newtype {
    ($t:ty, $bits:expr, $ceil:expr) => {
        impl Uint for $t {
            const BITS: u32 = $bits;
            const CEIL_BYTES: usize = $ceil;
            #[inline]
            fn from_u128(v: u128) -> Self {
                Self((v & Self::mask()) as u64)
            }
            #[inline]
            fn to_u128(self) -> u128 {
                self.0 as u128
            }
        }
    };
}
impl_uint_newtype!(U1, 1, 1);
impl_uint_newtype!(U3, 3, 1);
impl_uint_newtype!(U21, 21, 3);

macro_rules! impl_uint_prim {
    ($t:ty, $bits:expr, $ceil:expr) => {
        impl Uint for $t {
            const BITS: u32 = $bits;
            const CEIL_BYTES: usize = $ceil;
            #[inline]
            fn from_u128(v: u128) -> Self {
                (v & Self::mask()) as $t
            }
            #[inline]
            fn to_u128(self) -> u128 {
                self as u128
            }
        }
    };
}
impl_uint_prim!(u8, 8, 1);
// u16: the `rand.uintLessThan(u16, 1024)` draw of cli/Styled.zig (b11d60818).
impl_uint_prim!(u16, 16, 2);
// u32: the `random.uintLessThan(u32, key_space)` / `random.int(u32)` draws of
// terminal/hash_map.zig's oracle test (parity S155-F5).
impl_uint_prim!(u32, 32, 4);
// NOTE: `usize` is 64-bit on the target — treated as `u64` LE, matching Zig's
// `usize`.
impl_uint_prim!(u64, 64, 8);
impl_uint_prim!(usize, 64, 8);

impl Rng<'_> {
    // -- the fill primitive: the one place the shared generator is borrowed --

    /// PORT NOTE: `std.Random.bytes(r, buf)` → `r.fillFn(r.ptr, buf)`.
    pub fn bytes(&self, buf: &mut [u8]) {
        self.inner.borrow_mut().fill(buf);
    }

    /// PORT NOTE: `int(r, comptime T)`. Fills `ceil(bits/8)` bytes,
    /// `readInt(ByteAligned, .little)`, truncates to `T`. LE byte order is
    /// load-bearing.
    pub fn int<T: Uint>(&self) -> T {
        let mut rand_bytes = [0u8; 8];
        self.bytes(&mut rand_bytes[..T::CEIL_BYTES]);

        // use LE instead of native endian for better portability maybe?
        let mut byte_aligned_result: u128 = 0;
        #[allow(clippy::needless_range_loop)]
        for i in 0..T::CEIL_BYTES {
            byte_aligned_result |= (rand_bytes[i] as u128) << (8 * i);
        }
        T::from_u128(byte_aligned_result)
    }

    /// PORT NOTE: `boolean(r)` = `int(u1) != 0`.
    pub fn boolean(&self) -> bool {
        self.int::<U1>().0 != 0
    }

    /// PORT NOTE: `limitRangeBiased(comptime T, random_int, less_than)`.
    fn limit_range_biased<T: Uint>(random_int: T, less_than: T) -> T {
        // "Integer Multiplication (Biased)"
        let m = random_int.to_u128() * less_than.to_u128();
        T::from_u128(m >> T::BITS)
    }

    /// PORT NOTE: `uintLessThanBiased(r, comptime T, less_than)`.
    pub fn uint_less_than_biased<T: Uint>(&self, less_than: T) -> T {
        Self::limit_range_biased::<T>(self.int::<T>(), less_than)
    }

    /// PORT NOTE: `uintLessThan(r, comptime T, less_than)`. Lemire's, with the extra
    /// tweak. The number of `int()` draws consumed on rejection changes the
    /// whole downstream stream, so the loop is ported exactly.
    pub fn uint_less_than<T: Uint>(&self, less_than: T) -> T {
        let bits = T::BITS;
        let mask = T::mask();
        let lt = less_than.to_u128();

        // adapted from:
        //   http://www.pcg-random.org/posts/bounded-rands.html
        //   "Lemire's (with an extra tweak from me)"
        let mut x = self.int::<T>();
        let mut m = x.to_u128() * lt;
        let mut l = m & mask;
        if l < lt {
            let mut t = lt.wrapping_neg() & mask;

            if t >= lt {
                t -= lt;
                if t >= lt {
                    t %= lt;
                }
            }
            while l < t {
                x = self.int::<T>();
                m = x.to_u128() * lt;
                l = m & mask;
            }
        }
        let _ = x;
        T::from_u128(m >> bits)
    }

    /// PORT NOTE: `uintAtMostBiased(r, comptime T, at_most)`.
    pub fn uint_at_most_biased<T: Uint>(&self, at_most: T) -> T {
        if at_most.to_u128() == T::mask() {
            // have the full range
            return self.int::<T>();
        }
        self.uint_less_than_biased::<T>(T::from_u128(at_most.to_u128() + 1))
    }

    /// PORT NOTE: `uintAtMost(r, comptime T, at_most)`.
    pub fn uint_at_most<T: Uint>(&self, at_most: T) -> T {
        if at_most.to_u128() == T::mask() {
            // have the full range
            return self.int::<T>();
        }
        self.uint_less_than::<T>(T::from_u128(at_most.to_u128() + 1))
    }

    /// PORT NOTE: `intRangeAtMost(r, comptime T, at_least, at_most)`, unsigned branch
    /// (synthetic only uses unsigned `usize`/`u8`/`u3`).
    pub fn int_range_at_most<T: Uint>(&self, at_least: T, at_most: T) -> T {
        let d = T::from_u128(at_most.to_u128() - at_least.to_u128());
        T::from_u128(at_least.to_u128() + self.uint_at_most::<T>(d).to_u128())
    }

    /// PORT NOTE: `intRangeLessThan(r, comptime T, at_least, less_than)`,
    /// unsigned branch (`at_least + r.uintLessThan(T, less_than - at_least)`).
    pub fn int_range_less_than<T: Uint>(&self, at_least: T, less_than: T) -> T {
        assert!(at_least.to_u128() < less_than.to_u128());
        let d = T::from_u128(less_than.to_u128() - at_least.to_u128());
        T::from_u128(at_least.to_u128() + self.uint_less_than::<T>(d).to_u128())
    }

    /// PORT NOTE: `shuffle(r, comptime T, buf)` = `shuffleWithIndex(T, buf,
    /// usize)`; `MinArrayIndex(usize)` is `usize`, so the index draws are
    /// `intRangeLessThan(usize, i, max)` and the number of PRNG draws — hence
    /// the whole downstream stream — depends on taking them in exactly this
    /// order.
    pub fn shuffle<T: Copy>(&self, buf: &mut [T]) {
        if buf.len() < 2 {
            return;
        }
        let max: usize = buf.len();
        let mut i: usize = 0;
        while i < max - 1 {
            let j: usize = self.int_range_less_than::<usize>(i, max);
            buf.swap(i, j);
            i += 1;
        }
    }

    /// PORT NOTE: `intRangeAtMostBiased(r, comptime T, at_least, at_most)`, unsigned
    /// branch.
    pub fn int_range_at_most_biased<T: Uint>(&self, at_least: T, at_most: T) -> T {
        let d = T::from_u128(at_most.to_u128() - at_least.to_u128());
        T::from_u128(at_least.to_u128() + self.uint_at_most_biased::<T>(d).to_u128())
    }

    /// PORT NOTE: `float(r, f64)`. The mantissa + biased-exponent construction with
    /// the `@clz` leading-zero loop. The rare extra-draw loop must match to keep
    /// the stream aligned.
    pub fn float_f64(&self) -> f64 {
        // Use 52 random bits for the mantissa, and the rest for the exponent.
        // If all 12 bits are zero, generate additional random bits, until a
        // set bit is found, or 1022 bits have been generated.
        let rand = self.int::<u64>();
        let mut rand_lz: u64 = rand.leading_zeros() as u64;
        if rand_lz >= 12 {
            rand_lz = 12;
            loop {
                // It is astronomically unlikely for this loop to execute more than once.
                let addl_rand_lz = self.int::<u64>().leading_zeros() as u64;
                rand_lz += addl_rand_lz;
                if addl_rand_lz != 64 {
                    break;
                }
                if rand_lz >= 1022 {
                    rand_lz = 1022;
                    break;
                }
            }
        }
        let mantissa = rand & 0xFFFFFFFFFFFFF;
        let exponent = (1022 - rand_lz) << 52;
        f64::from_bits(exponent | mantissa)
    }

    /// PORT NOTE: `weightedIndex(r, f64, proportions)` — the float path. Selects the
    /// OSC ValidKind/InvalidKind and the Utf8 length; must be exact.
    pub fn weighted_index_f64(&self, proportions: &[f64]) -> usize {
        // This implementation works by summing the proportions and picking a
        // random point in [0, sum).  We then loop over the proportions,
        // accumulating until our accumulator is greater than the random point.
        let sum = {
            let mut sum: f64 = 0.0;
            for v in proportions {
                sum += *v;
            }
            sum
        };

        // take care that imprecision doesn't lead to a value slightly greater than sum
        let point = (self.float_f64() * sum).min(sum - f64::EPSILON);

        assert!(point < sum);

        let mut accumulator: f64 = 0.0;
        for (index, p) in proportions.iter().enumerate() {
            accumulator += *p;
            if point < accumulator {
                return index;
            }
        }
        unreachable!()
    }
}

// ===========================================================================
// Bonus fidelity anchors — the Zig std tests, pinning the PRNG bit-for-bit.
// These RAISE the module test count (the `>=` gate permits it).
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequence() {
        let mut r = Xoshiro256::init(0);

        let seq1: [u64; 6] = [
            0x53175d61490b23df,
            0x61da6f3dc380d507,
            0x5c0fdf91ec9a7bfc,
            0x02eebf8c3bbe5e1a,
            0x7eca04ebaf4a5eea,
            0x0543c37757f08d9a,
        ];

        for s in seq1 {
            assert_eq!(s, r.next());
        }

        r.jump();

        let seq2: [u64; 6] = [
            0xae1db5c5e27807be,
            0xb584c6a7fd8709fe,
            0x0c46a0ee9330fb6e,
            0xdc0c9606f49ed76e,
            0x1f5bb6540f6651fb,
            0x72fa2ca734601488,
        ];

        for s in seq2 {
            assert_eq!(s, r.next());
        }
    }

    #[test]
    fn fill() {
        let mut r = Xoshiro256::init(0);

        let seq: [u64; 6] = [
            0x53175d61490b23df,
            0x61da6f3dc380d507,
            0x5c0fdf91ec9a7bfc,
            0x02eebf8c3bbe5e1a,
            0x7eca04ebaf4a5eea,
            0x0543c37757f08d9a,
        ];

        for s in seq {
            let buf0 = s.to_le_bytes();
            let mut buf1 = [0u8; 7];
            r.fill(&mut buf1);
            assert_eq!(&buf0[0..7], &buf1[..]);
        }
    }

    #[test]
    fn splitmix64() {
        // First outputs of SplitMix64 seeded with 0 (the seeder feeding the
        // Xoshiro256 `sequence` seed above; these are exactly `Xoshiro256::init(0).s`).
        let mut g = SplitMix64::init(0);
        assert_eq!(g.next(), 0xe220a8397b1dcdaf);
        assert_eq!(g.next(), 0x6e789e6aa1b965f4);
        assert_eq!(g.next(), 0x06c45d188009454f);
        assert_eq!(g.next(), 0xf88bb8a8724c81ec);
    }
}
