//! zlib-sys — FFI bindings to the vendored zlib 1.3.1 compression (`links = "z"`).
//!
//! PORT STATUS: vendor-proof slice
//!
//! R3: copies the SAME C library the reference uses (zlib 1.3.1) and builds it
//! standalone (`cc`; see `build.rs`). No pure-Rust substitution during parity
//! phases. The Phase-1 proof slice vendors zlib's `adler32` checksum unit under
//! `vendor/` and exposes the hand-written thin FFI below; the
//! `cargo test -p zlib-sys` known-answer test links and exercises the C symbol,
//! proving the vendor toolchain end-to-end.

#![allow(non_camel_case_types)]

use std::os::raw::{c_uint, c_ulong};

extern "C" {
    /// Update a running Adler-32 checksum with `buf[0..len-1]` and return the
    /// updated checksum (upstream `adler32` in `vendor/adler32.c`). With a NULL
    /// `buf` it returns the initial seed value (`1`).
    ///
    /// # Safety
    /// `buf` must point to at least `len` readable bytes, or be NULL.
    pub fn adler32(adler: c_ulong, buf: *const u8, len: c_uint) -> c_ulong;
}

/// Safe wrapper computing the zlib Adler-32 checksum of `data`.
///
/// Seeds with the zlib initial value (`1`) exactly as `adler32(0L, Z_NULL, 0)`
/// would return, then folds in the bytes. Result matches zlib bit-for-bit.
#[must_use]
pub fn adler32_checksum(data: &[u8]) -> u32 {
    // SAFETY: data.as_ptr() points to data.len() readable bytes; len fits c_uint
    // for any slice this crate's callers pass (proof slice uses tiny inputs).
    let sum = unsafe { adler32(1, data.as_ptr(), data.len() as c_uint) };
    sum as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Known-answer test: zlib's documented Adler-32 of the ASCII string
    /// "Wikipedia" is 0x11E60398 (the canonical worked example). Linking and
    /// matching this proves the vendored C compiled, linked, and runs correctly
    /// (R3 vendor-toolchain proof).
    #[test]
    fn adler32_wikipedia_known_answer() {
        assert_eq!(adler32_checksum(b"Wikipedia"), 0x11E6_0398);
    }

    /// The empty input yields the seed value 1 (zlib invariant).
    #[test]
    fn adler32_empty_is_one() {
        assert_eq!(adler32_checksum(b""), 1);
    }

    /// A single byte: adler32("a") = 0x00620062 (sum1=0x62, sum2=0x62).
    #[test]
    fn adler32_single_byte() {
        assert_eq!(adler32_checksum(b"a"), 0x0062_0062);
    }

    /// The raw FFI seed behavior: adler32(0, NULL, 0) returns 1.
    #[test]
    fn adler32_ffi_null_seed() {
        // SAFETY: NULL buf with len 0 is the documented "return initial value".
        let seed = unsafe { adler32(0, std::ptr::null(), 0) };
        assert_eq!(seed as u32, 1);
    }
}
