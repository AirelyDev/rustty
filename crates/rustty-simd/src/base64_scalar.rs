//! base64_scalar — PORTS ghostty src/simd/base64_scalar.zig.
//!
//! PORT STATUS: stub
//!
//! Pure-Rust scalar base64 decoder used as the fallback when SIMD is disabled.
//! Copied from Zig 0.14.1 stdlib (per the reference) with the invalid-padding
//! scenarios commented out, because Kitty Graphics requires a decoder that
//! doesn't care about invalid padding scenarios. R1: every line/comment/test{}
//! is ported assertion-for-assertion as this module is carved; this is the
//! compiling skeleton only.
