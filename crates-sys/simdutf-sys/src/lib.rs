//! simdutf-sys — FFI to the vendored simdutf UTF validation/transcoding
//! (`links = "simdutf"`).
//!
//! PORT STATUS: vendored amalgamation
//!
//! R3: vendors the SAME C++ library the reference uses (simdutf single-header
//! amalgamation, copied from pkg/simdutf/vendor) and builds it standalone
//! (`cc`; see `build.rs`). No pure-Rust substitution during parity phases. The
//! vendored `simdutf.h` is re-exported via `DEP_SIMDUTF_INCLUDE` so rustty-simd
//! compiles its copied `src/simd/base64.cpp` (which `#include <simdutf.h>`)
//! against the same header the reference uses.
//!
//! simdutf's public API is C++-namespaced; the thin `extern "C"` link-test
//! wrapper below (compiled from `cbits/shim.cpp`) exercises the linked archive.

use std::os::raw::{c_char, c_int};

extern "C" {
    /// `extern "C"` wrapper over `simdutf::validate_utf8` (see `cbits/shim.cpp`).
    /// Returns 1 if `buf[0..len]` is valid UTF-8, else 0.
    ///
    /// # Safety
    /// `buf` must point to at least `len` readable bytes.
    // Only referenced by the link test below; the wrapper exists solely to prove
    // the vendored simdutf archive links (R3 vendor-toolchain proof).
    #[cfg_attr(not(test), allow(dead_code))]
    fn simdutf_sys_validate_utf8(buf: *const c_char, len: usize) -> c_int;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Link test: valid UTF-8 (ASCII "hello") validates as true. Linking and
    /// matching proves the vendored simdutf compiled, linked, and runs (R3
    /// vendor-toolchain proof). Expected truth value is from the UTF-8 spec
    /// (ASCII is always valid UTF-8), independent of the C++ under test.
    #[test]
    fn validate_utf8_valid_ascii() {
        let s = b"hello";
        // SAFETY: s points to 5 readable bytes.
        let rc = unsafe { simdutf_sys_validate_utf8(s.as_ptr() as *const c_char, s.len()) };
        assert_eq!(rc, 1);
    }

    /// Link test: an invalid UTF-8 byte sequence (a lone continuation byte 0x80)
    /// validates as false. 0x80 cannot start a UTF-8 sequence (per the UTF-8
    /// spec), so a buffer of just 0x80 is invalid — an independent expected
    /// value, not derived from the C++ under test.
    #[test]
    fn validate_utf8_invalid_continuation() {
        let s = [0x80u8];
        // SAFETY: s points to 1 readable byte.
        let rc = unsafe { simdutf_sys_validate_utf8(s.as_ptr() as *const c_char, s.len()) };
        assert_eq!(rc, 0);
    }
}
