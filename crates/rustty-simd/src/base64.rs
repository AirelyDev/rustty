//! base64 — PORTS ghostty src/simd/base64.zig.
//!
//! PORT STATUS: stub
//!
//! Thin wrapper over the copied `cbits/simd/base64.cpp` SIMD kernel
//! (`ghostty_simd_base64_max_length` / `ghostty_simd_base64_decode`, which call
//! into simdutf), with a scalar fallback in `base64_scalar` when SIMD is off.
//! R1: every line/comment/test{} of base64.zig is ported assertion-for-assertion
//! as this module is carved; this is the compiling skeleton only.
