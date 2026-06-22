//! SIMD-optimized routines. If `build_options.simd` is false, then the API
//! still works but we fall back to pure Rust scalar implementations.
//!
//! PORT STATUS: stub
//!
//! This is the crate-root aggregator porting ghostty src/simd/main.zig. It
//! re-exports the SIMD submodules. The SIMD kernels are ghostty's OWN C++
//! (src/simd/*.cpp,*.h), copied VERBATIM into `cbits/simd/` and cc-built against
//! highway-sys + simdutf-sys (R3: same C libs, no pure-Rust substitution). The
//! Zig wrappers (base64.zig, base64_scalar.zig, codepoint_width.zig,
//! index_of.zig, vt.zig) are ported to the Rust modules below as the crate is
//! carved, leaves-first, under the acyclicity gate (R4). R1: every
//! line/comment/test{} of the reference is ported assertion-for-assertion; the
//! stubs below carry no real logic yet.

// Force the cc-built C++ kernel archive (and its highway/simdutf deps) to be
// linked even before the Rust wrappers reference its symbols, so the copied
// kernels are present in the final link.
extern crate highway_sys as _;
extern crate simdutf_sys as _;

pub mod base64;
pub mod base64_scalar;
pub mod codepoint_width;
pub mod index_of;
pub mod vt;
