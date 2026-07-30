//! SIMD-optimized routines. If `build_options.simd` is false, then the API
//! still works but we fall back to pure Zig scalar implementations.
//!
//! This is the crate-root aggregator porting ghostty src/simd/main.zig. It
//! re-exports the SIMD submodules. The SIMD kernels are ghostty's OWN C++
//! (src/simd/*.cpp,*.h), copied VERBATIM into `cbits/simd/` and cc-built against
//! highway-sys + simdutf-sys (R3: same C libs, no pure-Rust substitution).
//!
//! PORT STATUS: done

// Force the cc-built C++ kernel archive (and its highway/simdutf deps) to be
// linked even before the Rust wrappers reference its symbols, so the copied
// kernels are present in the final link.
//
// WASM DIVERGENCE (`cfg(target_arch = "wasm32")`): the freestanding
// `wasm32-unknown-unknown` browser target has no libc/libc++ and cannot link the
// C++ SIMD kernels (highway/simdutf). On wasm we compile PURE-RUST scalar paths
// instead — the exact code the reference builds when `build_options.simd` is
// false — so the two `-sys` force-links (and the `cc` C++ compile in `build.rs`)
// are gated OFF. Same class as the existing `cfg!(miri)` scalar fallback; see the
// per-module SIMD gates. Native/wasip1 builds are untouched.
#[cfg(not(target_arch = "wasm32"))]
extern crate highway_sys as _;
#[cfg(not(target_arch = "wasm32"))]
extern crate simdutf_sys as _;

pub mod base64;
mod codepoint_width;
pub mod index_of;
pub mod vt;
pub use codepoint_width::codepointWidth;

/// The number of vector lanes to use for manually vectorized hot
/// loops operating on elements of type T, or null if the target has
/// no usable SIMD support and a plain scalar loop should be used
/// instead (e.g. wasm32 without simd128).
///
/// This is twice the native vector width so that each iteration works
/// two vector registers, giving better instruction-level parallelism.
//
// PORT NOTE: the reference is
//   pub fn lanes(comptime T: type) ?comptime_int {
//       const native = std.simd.suggestVectorLength(T) orelse return null;
//       return native * 2;
//   }
// `std.simd.suggestVectorLength` is Zig's comptime query for the target's
// native vector width in elements of T. Stable Rust has no portable-SIMD
// equivalent (`core::simd` is nightly-only and `LTO is banned` rules out
// cross-crate specialisation tricks), so the native width is derived here from
// the same target facts Zig uses: the widest enabled vector register on the
// target, divided by `size_of::<T>()`. Callers get a `const`-evaluable
// `Option<usize>` and instantiate their fill/scan loops over
// `[T; LANES]` chunks — the shape LLVM reliably lowers to the same vector
// stores the reference's `@Vector(lanes, T)` produces.
//
// This is `const fn` so call sites can use it in array lengths, matching the
// reference's `comptime_int` return.
pub const fn native_vector_bits() -> Option<usize> {
    // aarch64 NEON is mandatory in the base ISA (128-bit Q registers).
    #[cfg(target_arch = "aarch64")]
    {
        Some(128)
    }
    // x86-64: AVX2 (256-bit) when enabled, otherwise the baseline SSE2 128-bit.
    #[cfg(all(
        any(target_arch = "x86_64", target_arch = "x86"),
        target_feature = "avx2"
    ))]
    {
        Some(256)
    }
    #[cfg(all(
        any(target_arch = "x86_64", target_arch = "x86"),
        not(target_feature = "avx2"),
        target_feature = "sse2"
    ))]
    {
        Some(128)
    }
    // wasm32 only has vectors with the `simd128` feature; without it the
    // reference returns null and the caller runs a plain scalar loop.
    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    {
        Some(128)
    }
    #[cfg(not(any(
        target_arch = "aarch64",
        all(
            any(target_arch = "x86_64", target_arch = "x86"),
            any(target_feature = "avx2", target_feature = "sse2")
        ),
        all(target_arch = "wasm32", target_feature = "simd128"),
    )))]
    {
        None
    }
}

/// `pub fn lanes(comptime T: type) ?comptime_int` — see `native_vector_bits`
/// for why the native width is spelled out rather than queried.
///
/// `T` is passed as its size in bytes because Rust has no `comptime T: type`
/// parameter usable in a `const fn`; every call site is
/// `lanes(size_of::<T>())`, which is exactly the reference's
/// `simd.lanes(T)`.
pub const fn lanes(elem_size: usize) -> Option<usize> {
    // const native = std.simd.suggestVectorLength(T) orelse return null;
    let native = match native_vector_bits() {
        Some(bits) => bits / (elem_size * 8),
        None => return None,
    };
    if native == 0 {
        return None;
    }
    // return native * 2;
    Some(native * 2)
}

// PORT NOTE: `base64_scalar.zig` is imported transitively by `base64.zig` (it is not
// re-exported by main.zig). It is a private submodule of this crate here.
mod base64_scalar;

#[cfg(test)]
mod tests {
    // @import("std").testing.refAllDecls(@This());
    //
    // PORT NOTE: Zig's `refAllDecls` forces every public declaration in the module
    // tree to be referenced (and thus semantically analyzed / its tests
    // discovered) at test time. Rust's test harness already compiles and runs
    // every `#[test]` in every submodule unconditionally, so this is a no-op
    // assertion of the same property: the submodules below are referenced.
    #[test]
    fn ref_all_decls() {
        let _ = super::base64::max_len;
        let _ = super::base64_scalar::scalar_decoder;
        let _ = super::codepointWidth;
        let _ = super::index_of::indexOf;
        let _ = super::vt::utf8DecodeUntilControlSeq;
        let _ = super::lanes(4);
    }

    // `simd.lanes(T)` is `2 * suggestVectorLength(T)` on every SIMD target and
    // `null` only where the target has no vectors. On the two targets rustty
    // actually ships hot loops for (aarch64 NEON, x86-64 SSE2/AVX2) it must be
    // Some(...) and a power of two, or the fill/scan loops silently degrade to
    // their scalar tails.
    #[test]
    fn lanes_matches_the_native_vector_width() {
        let bits = super::native_vector_bits();
        #[cfg(any(target_arch = "aarch64", target_arch = "x86_64"))]
        assert!(bits.is_some(), "expected vectors on this target");
        if let Some(bits) = bits {
            for elem in [1usize, 2, 4, 8] {
                let l = super::lanes(elem).unwrap();
                assert_eq!(l, 2 * (bits / (elem * 8)));
                assert!(l.is_power_of_two());
            }
        } else {
            assert_eq!(None, super::lanes(4));
        }
    }
}
