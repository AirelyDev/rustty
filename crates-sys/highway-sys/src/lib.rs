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

    /// Test seam (S34-F1): the x86 AVX2 gate (`x86_avx2_enabled` in
    /// `cpp/detect.c`), ported from the reference's `x86.zig:63-94`, driven
    /// with synthetic CPUID/XCR0 leaf values. Pure function of its arguments
    /// (no CPU instructions), so it is callable and meaningful on every host
    /// this crate builds on — including non-x86 hosts, where
    /// `rustty_hwy_detect_targets`'s real x86 branch cannot execute at all.
    ///
    /// Returns `TGT_AVX2` (bit 9, i.e. `1 << 9`) if the synthetic inputs
    /// would enable AVX2, else `0`.
    pub fn rustty_hwy_detect_targets_x86_avx2_for_test(
        max_leaf: u32,
        leaf1_ecx: u32,
        leaf7_ebx: u32,
        ext_leaf1_ecx: u32,
        xcr0: u32,
    ) -> c_longlong;
}

/// Bit position of `TGT_AVX2` in the Highway target mask (from the reference's
/// `pkg/highway/src/targets.zig` comptime asserts; mirrored in
/// `cpp/detect.c`'s `TGT_AVX2` define).
#[cfg(test)]
const TGT_AVX2: c_longlong = 1 << 9;

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

    // -- S34-F1: AVX2 must not be enabled without OS xsave support, or without
    // BMI2. Drives cpp/detect.c's x86_avx2_enabled() (the exact gate the real
    // x86 detector calls) via the synthetic-leaf test seam
    // rustty_hwy_detect_targets_x86_avx2_for_test — pure integer logic, no CPU
    // instructions, so this runs and is meaningful on every host (including
    // this crate's aarch64 CI/dev hosts, where the real x86 detection branch
    // in rustty_hwy_detect_targets() cannot execute at all).

    const XSAVE: u32 = 1 << 27; // CPUID.1:ECX OSXSAVE
    const AVX: u32 = 1 << 28; // CPUID.1:ECX AVX
    const FMA: u32 = 1 << 12; // CPUID.1:ECX FMA
    const F16C: u32 = 1 << 29; // CPUID.1:ECX F16C
    const AVX2: u32 = 1 << 5; // CPUID.7.0:EBX AVX2
    const BMI: u32 = 1 << 3; // CPUID.7.0:EBX BMI1
    const BMI2: u32 = 1 << 8; // CPUID.7.0:EBX BMI2
    const LZCNT: u32 = 1 << 5; // CPUID.80000001H:ECX LZCNT

    #[test]
    fn avx2_requires_os_xsave_and_bmi() {
        // AVX2+FMA+F16C+XSAVE+AVX+BMI+BMI2+LZCNT all present in CPUID, but the
        // OS does NOT report saving AVX state (xcr0 = 0). Per x86.zig:70,79
        // (has_avx_save gate), AVX2 must stay off.
        let mask = unsafe {
            rustty_hwy_detect_targets_x86_avx2_for_test(
                /* max_leaf */ 7,
                /* leaf1_ecx */ XSAVE | AVX | FMA | F16C,
                /* leaf7_ebx */ AVX2 | BMI | BMI2,
                /* ext_leaf1_ecx */ LZCNT,
                /* xcr0 */ 0x0,
            )
        };
        assert_eq!(mask & TGT_AVX2, 0, "AVX2 enabled without OS xsave support");

        // Same, but BMI2 missing while the OS does save SSE+AVX state
        // (xcr0 & 0x6 == 0x6). Per x86.zig:86-93, AVX2 must stay off.
        let mask = unsafe {
            rustty_hwy_detect_targets_x86_avx2_for_test(
                /* max_leaf */ 7,
                /* leaf1_ecx */ XSAVE | AVX | FMA | F16C,
                /* leaf7_ebx */ AVX2 | BMI,
                /* ext_leaf1_ecx */ LZCNT,
                /* xcr0 */ 0x6,
            )
        };
        assert_eq!(mask & TGT_AVX2, 0, "AVX2 enabled without BMI2");

        // Positive control: same inputs, but with the OS-save bit AND BMI2
        // both present — AVX2 must now be reported, proving the seam isn't
        // vacuously returning 0.
        let mask = unsafe {
            rustty_hwy_detect_targets_x86_avx2_for_test(
                /* max_leaf */ 7,
                /* leaf1_ecx */ XSAVE | AVX | FMA | F16C,
                /* leaf7_ebx */ AVX2 | BMI | BMI2,
                /* ext_leaf1_ecx */ LZCNT,
                /* xcr0 */ 0x6,
            )
        };
        assert_eq!(
            mask & TGT_AVX2,
            TGT_AVX2,
            "AVX2 not enabled with all required bits present"
        );
    }
}
