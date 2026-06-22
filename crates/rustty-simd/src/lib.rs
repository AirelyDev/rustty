//! SIMD-optimized routines. If `build_options.simd` is false, then the API
//! still works but we fall back to pure Zig scalar implementations.
//!
//! This is the crate-root aggregator porting ghostty src/simd/main.zig. It
//! re-exports the SIMD submodules. The SIMD kernels are ghostty's OWN C++
//! (src/simd/*.cpp,*.h), copied VERBATIM into `cbits/simd/` and cc-built against
//! highway-sys + simdutf-sys (R3: same C libs, no pure-Rust substitution).

// Force the cc-built C++ kernel archive (and its highway/simdutf deps) to be
// linked even before the Rust wrappers reference its symbols, so the copied
// kernels are present in the final link.
extern crate highway_sys as _;
extern crate simdutf_sys as _;

pub mod base64;
mod codepoint_width;
pub mod index_of;
pub mod vt;
pub use codepoint_width::codepointWidth;

// PORT: `base64_scalar.zig` is imported transitively by `base64.zig` (it is not
// re-exported by main.zig). It is a private submodule of this crate here.
mod base64_scalar;

#[cfg(test)]
mod tests {
    // @import("std").testing.refAllDecls(@This());
    //
    // PORT: Zig's `refAllDecls` forces every public declaration in the module
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
    }
}
