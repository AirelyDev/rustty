//! highway-sys — FFI to the vendored Google Highway SIMD runtime (`links = "hwy"`).
//!
//! PORT STATUS: vendored runtime
//!
//! R3: vendors the SAME C++ library the reference uses (google/highway @
//! 66486a10623fa0d72fe91260f96c892e41aceb06) and builds the reference's minimal
//! Highway runtime standalone (`cc`; see `build.rs`). No pure-Rust substitution
//! during parity phases. The vendored `hwy/` header tree is re-exported via
//! `DEP_HWY_INCLUDE` so rustty-simd compiles its copied `src/simd/*.cpp` against
//! `<hwy/highway.h>` exactly as the reference does.
//!
//! The thin FFI below exposes the runtime's C entry points
//! (`hwy_supported_targets` from the vendored `targets.cpp`, and the C-ported
//! `rustty_hwy_detect_targets`) so the link test exercises the compiled
//! archive end-to-end.

#![allow(non_camel_case_types)]

use std::os::raw::c_longlong;

extern "C" {
    /// Final supported-target mask from Highway's runtime dispatch state
    /// (`extern "C" int64_t hwy_supported_targets()` in vendored `targets.cpp`).
    pub fn hwy_supported_targets() -> c_longlong;

    /// Runtime CPU target detection (C port of the reference's Zig
    /// `rustty_hwy_detect_targets`; vendored `cpp/detect.c`). Returns the
    /// Highway target bitmask for the running CPU.
    pub fn rustty_hwy_detect_targets() -> c_longlong;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Link test: the vendored Highway runtime archive links and runs. Highway's
    /// `SupportedTargets()` always returns at least `HWY_STATIC_TARGET` (never
    /// zero) — see vendored `targets.cpp` `SupportedTargets`. Calling it proves
    /// the cc-built archive linked and executes.
    #[test]
    fn supported_targets_is_nonzero() {
        // SAFETY: no arguments; the C function reads/initializes static dispatch
        // state and returns the mask.
        let mask = unsafe { hwy_supported_targets() };
        assert_ne!(mask, 0);
    }

    /// Link test for the C-ported detection symbol the runtime depends on. On
    /// every supported CPU the reference detection sets at least one bit; the
    /// fallback path returns baseline NEON / 0 only on unsupported arches, so we
    /// only assert it links and returns without trapping here.
    #[test]
    fn detect_targets_links() {
        // SAFETY: no arguments; pure CPU-feature probing.
        let _ = unsafe { rustty_hwy_detect_targets() };
    }
}
