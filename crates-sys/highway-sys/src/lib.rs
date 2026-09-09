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

    /// Test seam (S34-F4): the PowerPC target gate (`ppc_targets_enabled` in
    /// `cpp/detect.c`), ported from the reference's `ppc.zig:16-40`, driven
    /// with synthetic AT_HWCAP/AT_HWCAP2 values. Pure function of its
    /// arguments (no CPU instructions), so it is callable and meaningful on
    /// every host this crate builds on — including this non-PowerPC dev
    /// machine, where `rustty_hwy_detect_targets`'s real ppc branch cannot
    /// execute at all (excluded by preprocessor `#if`).
    pub fn rustty_hwy_detect_targets_ppc_for_test(hwcap: u64, hwcap2: u64) -> c_longlong;

    /// Test seam (S34-F5): the s390x target gate (`s390x_targets_enabled` in
    /// `cpp/detect.c`), ported from the reference's `s390x.zig:9-25`, driven
    /// with a synthetic AT_HWCAP value. Same rationale as the ppc seam above.
    pub fn rustty_hwy_detect_targets_s390x_for_test(hwcap: u64) -> c_longlong;

    /// Test seam (S34-F6): the RISC-V target gate (`riscv_targets_enabled`
    /// in `cpp/detect.c`), ported from the reference's `riscv.zig:9-17`,
    /// driven with a synthetic AT_HWCAP value. Same rationale as the ppc seam
    /// above.
    pub fn rustty_hwy_detect_targets_riscv_for_test(hwcap: u64) -> c_longlong;
}

/// Bit position of `TGT_AVX2` in the Highway target mask (from the reference's
/// `pkg/highway/src/targets.zig` comptime asserts; mirrored in
/// `cpp/detect.c`'s `TGT_AVX2` define).
#[cfg(test)]
const TGT_AVX2: c_longlong = 1 << 9;

/// Bit positions of `TGT_RVV`/`TGT_PPC8`/`TGT_PPC9`/`TGT_PPC10`/`TGT_Z14`/
/// `TGT_Z15` in the Highway target mask (from the reference's
/// `pkg/highway/src/targets.zig` comptime asserts; mirrored in
/// `cpp/detect.c`'s `TGT_*` defines).
#[cfg(test)]
const TGT_RVV: c_longlong = 1 << 37;
#[cfg(test)]
const TGT_PPC10: c_longlong = 1 << 47;
#[cfg(test)]
const TGT_PPC9: c_longlong = 1 << 48;
#[cfg(test)]
const TGT_PPC8: c_longlong = 1 << 49;
#[cfg(test)]
const TGT_Z15: c_longlong = 1 << 50;
#[cfg(test)]
const TGT_Z14: c_longlong = 1 << 51;

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

    // -- S34-F4: PowerPC target tiers (PPC8/PPC9/PPC10) must nest exactly as
    // ppc.zig:16-40 does. Drives cpp/detect.c's ppc_targets_enabled() (the
    // exact gate the real ppc detector calls) via the synthetic-hwcap test
    // seam rustty_hwy_detect_targets_ppc_for_test — pure integer logic, no
    // CPU instructions, so this runs and is meaningful on every host
    // (including this arm64 dev host, where the real ppc detection branch in
    // rustty_hwy_detect_targets() is excluded by preprocessor #if entirely).

    const PPC_ALTIVEC: u64 = 0x10000000;
    const PPC_VSX: u64 = 0x00000080;
    const PPC2_ARCH_2_07: u64 = 0x80000000;
    const PPC2_VEC_CRYPTO: u64 = 0x02000000;
    const PPC2_ARCH_3_00: u64 = 0x00800000;
    const PPC2_ARCH_3_1: u64 = 0x00040000;
    const PPC2_MMA: u64 = 0x00020000;

    #[test]
    fn ppc8_requires_altivec_vsx_arch207_veccrypto() {
        // Nothing set: no tier.
        let mask = unsafe { rustty_hwy_detect_targets_ppc_for_test(0, 0) };
        assert_eq!(mask, 0, "PPC8 enabled with nothing set");

        // hwcap complete, hwcap2 missing VEC_CRYPTO: PPC8 must stay off.
        let mask = unsafe {
            rustty_hwy_detect_targets_ppc_for_test(
                PPC_ALTIVEC | PPC_VSX,
                PPC2_ARCH_2_07, // VEC_CRYPTO missing
            )
        };
        assert_eq!(mask, 0, "PPC8 enabled without VEC_CRYPTO");

        // Positive control: PPC8 tier complete, nothing above it.
        let mask = unsafe {
            rustty_hwy_detect_targets_ppc_for_test(
                PPC_ALTIVEC | PPC_VSX,
                PPC2_ARCH_2_07 | PPC2_VEC_CRYPTO,
            )
        };
        assert_eq!(
            mask, TGT_PPC8,
            "PPC8 not enabled with all required bits present"
        );
    }

    #[test]
    fn ppc9_nests_inside_ppc8_plus_arch300() {
        // PPC8 complete + ARCH_3_00: PPC9 set, PPC8 still set, PPC10 not.
        let mask = unsafe {
            rustty_hwy_detect_targets_ppc_for_test(
                PPC_ALTIVEC | PPC_VSX,
                PPC2_ARCH_2_07 | PPC2_VEC_CRYPTO | PPC2_ARCH_3_00,
            )
        };
        assert_eq!(mask, TGT_PPC8 | TGT_PPC9, "PPC9 tier mismatch");

        // ARCH_3_00 alone, without the PPC8 prerequisite bits: PPC9 must NOT
        // be reachable (the oracle nests it under the PPC8 `if`).
        let mask = unsafe { rustty_hwy_detect_targets_ppc_for_test(0, PPC2_ARCH_3_00) };
        assert_eq!(mask, 0, "PPC9 enabled without PPC8 prerequisites");
    }

    #[test]
    fn ppc10_nests_inside_ppc9_plus_arch31_and_mma() {
        // PPC9 complete + ARCH_3_1 but missing MMA: PPC10 must stay off.
        let mask = unsafe {
            rustty_hwy_detect_targets_ppc_for_test(
                PPC_ALTIVEC | PPC_VSX,
                PPC2_ARCH_2_07 | PPC2_VEC_CRYPTO | PPC2_ARCH_3_00 | PPC2_ARCH_3_1, // MMA missing
            )
        };
        assert_eq!(mask, TGT_PPC8 | TGT_PPC9, "PPC10 enabled without MMA");

        // Positive control: full PPC10 tier.
        let mask = unsafe {
            rustty_hwy_detect_targets_ppc_for_test(
                PPC_ALTIVEC | PPC_VSX,
                PPC2_ARCH_2_07 | PPC2_VEC_CRYPTO | PPC2_ARCH_3_00 | PPC2_ARCH_3_1 | PPC2_MMA,
            )
        };
        assert_eq!(
            mask,
            TGT_PPC8 | TGT_PPC9 | TGT_PPC10,
            "PPC10 not enabled with all required bits present"
        );
    }

    // -- S34-F5: s390x target tiers (Z14/Z15) must nest exactly as
    // s390x.zig:9-25 does.

    const S390_VX: u64 = 1 << 11;
    const S390_VXE: u64 = 1 << 13;
    const S390_VXE2: u64 = 1 << 15;

    #[test]
    fn z14_requires_vx_and_vxe() {
        let mask = unsafe { rustty_hwy_detect_targets_s390x_for_test(0) };
        assert_eq!(mask, 0, "Z14 enabled with nothing set");

        // VX alone (no VXE): Z14 must stay off.
        let mask = unsafe { rustty_hwy_detect_targets_s390x_for_test(S390_VX) };
        assert_eq!(mask, 0, "Z14 enabled without VXE");

        // VXE alone (no VX): oracle nests the VXE check inside `if (hwcap &
        // HWCAP_VX)`, so this must also stay off.
        let mask = unsafe { rustty_hwy_detect_targets_s390x_for_test(S390_VXE) };
        assert_eq!(mask, 0, "Z14 enabled without VX");

        // Positive control.
        let mask = unsafe { rustty_hwy_detect_targets_s390x_for_test(S390_VX | S390_VXE) };
        assert_eq!(mask, TGT_Z14, "Z14 not enabled with VX+VXE present");
    }

    #[test]
    fn z15_nests_inside_z14_plus_vxe2() {
        // Z14 tier complete + VXE2: Z15 set, Z14 still set.
        let mask =
            unsafe { rustty_hwy_detect_targets_s390x_for_test(S390_VX | S390_VXE | S390_VXE2) };
        assert_eq!(mask, TGT_Z14 | TGT_Z15, "Z15 tier mismatch");

        // VXE2 alone, without VX/VXE prerequisites: Z15 must NOT be reachable.
        let mask = unsafe { rustty_hwy_detect_targets_s390x_for_test(S390_VXE2) };
        assert_eq!(mask, 0, "Z15 enabled without Z14 prerequisites");
    }

    // -- S34-F6: RISC-V RVV detection must match riscv.zig:9-17 (HWCAP bit
    // ('V' - 'A') == 21).

    #[test]
    fn rvv_requires_hwcap_v_bit() {
        let mask = unsafe { rustty_hwy_detect_targets_riscv_for_test(0) };
        assert_eq!(mask, 0, "RVV enabled with nothing set");

        // A neighboring bit (20, 'U'-'A') set but not bit 21: RVV must stay off.
        let mask = unsafe { rustty_hwy_detect_targets_riscv_for_test(1 << 20) };
        assert_eq!(mask, 0, "RVV enabled by an unrelated HWCAP bit");

        // Positive control: bit 21 set.
        let mask = unsafe { rustty_hwy_detect_targets_riscv_for_test(1 << 21) };
        assert_eq!(mask, TGT_RVV, "RVV not enabled with HWCAP bit 21 set");
    }
}
