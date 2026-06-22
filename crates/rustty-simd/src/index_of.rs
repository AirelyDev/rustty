//! index_of — PORTS ghostty src/simd/index_of.zig.
//!
//! PORT STATUS: stub
//!
//! Thin wrapper over the copied `cbits/simd/index_of.cpp` SIMD kernel
//! (`ghostty_simd_index_of`), with a scalar fallback when SIMD is off. R1: every
//! line/comment/test{} of index_of.zig is ported assertion-for-assertion as this
//! module is carved; this is the compiling skeleton only.
