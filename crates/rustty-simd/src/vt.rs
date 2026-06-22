//! vt — PORTS ghostty src/simd/vt.zig.
//!
//! PORT STATUS: stub
//!
//! Thin wrapper over the copied `cbits/simd/vt.cpp` SIMD kernel
//! (`ghostty_simd_decode_utf8_until_control_seq`), with a scalar fallback when
//! SIMD is off. R1: every line/comment/test{} of vt.zig is ported
//! assertion-for-assertion as this module is carved; this is the compiling
//! skeleton only.
