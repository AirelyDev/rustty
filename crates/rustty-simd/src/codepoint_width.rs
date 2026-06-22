//! codepoint_width — PORTS ghostty src/simd/codepoint_width.zig.
//!
//! PORT STATUS: stub
//!
//! Thin wrapper over the copied `cbits/simd/codepoint_width.cpp` SIMD kernel
//! (`ghostty_simd_codepoint_width`), with a uucode-backed scalar fallback when
//! SIMD is off. R1: every line/comment/test{} of codepoint_width.zig is ported
//! assertion-for-assertion as this module is carved; this is the compiling
//! skeleton only.
