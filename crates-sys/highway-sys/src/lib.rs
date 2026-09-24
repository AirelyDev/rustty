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

    /// Test seam (S34-F2): the x86 AVX-512 / AVX10 gate
    /// (`x86_avx512_targets` in `cpp/detect.c`), ported from the reference's
    /// `x86.zig:66-72, 96-154`, driven with synthetic CPUID/XCR0 leaves. Same
    /// rationale as the AVX2 seam above.
    pub fn rustty_hwy_detect_targets_x86_avx512_for_test(leaves: *const X86Leaves) -> c_longlong;

    /// Test seam (S34-F2): the reference's `isAMD()` (`x86.zig:160-166`) on
    /// synthetic `cpuid(0)` ebx/ecx/edx.
    pub fn rustty_hwy_detect_x86_is_amd_for_test(
        leaf0_ebx: u32,
        leaf0_ecx: u32,
        leaf0_edx: u32,
    ) -> std::os::raw::c_int;

    /// Test seam (S34-F3): the aarch64 Linux gate (`aarch64_linux_targets` in
    /// `cpp/detect.c`), ported from the reference's `aarch64_linux.zig:9-50`,
    /// driven with synthetic AT_HWCAP/AT_HWCAP2 values and SVE vector length
    /// (`sveVectorBytes()`). Same rationale as the ppc seam above.
    pub fn rustty_hwy_detect_targets_aarch64_linux_for_test(
        hwcap: u64,
        hwcap2: u64,
        vec_bytes: u64,
    ) -> c_longlong;
}

/// The CPUID/XCR0 values `cpp/detect.c`'s `x86_avx512_targets` reads
/// (`struct rustty_hwy_x86_leaves`, same field order and C layout).
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct X86Leaves {
    pub max_leaf: u32,
    pub leaf1_ecx: u32,
    pub xcr0: u32,
    pub leaf7_eax: u32,
    pub leaf7_ebx: u32,
    pub leaf7_ecx: u32,
    pub leaf7_edx: u32,
    pub leaf7_1_eax: u32,
    pub leaf7_1_edx: u32,
    pub leaf24_ebx: u32,
    pub is_amd: std::os::raw::c_int,
    pub is_darwin: std::os::raw::c_int,
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

/// Bit positions of the AVX-512 and SVE targets in the Highway target mask
/// (`pkg/highway/src/targets.zig:68-82` comptime asserts; mirrored in
/// `cpp/detect.c`'s `TGT_*` defines).
#[cfg(test)]
const TGT_AVX3_SPR: c_longlong = 1 << 4;
#[cfg(test)]
const TGT_AVX3_ZEN4: c_longlong = 1 << 6;
#[cfg(test)]
const TGT_AVX3_DL: c_longlong = 1 << 7;
#[cfg(test)]
const TGT_AVX3: c_longlong = 1 << 8;
#[cfg(test)]
const TGT_SVE2_128: c_longlong = 1 << 18;
#[cfg(test)]
const TGT_SVE_256: c_longlong = 1 << 19;
#[cfg(test)]
const TGT_SVE2: c_longlong = 1 << 23;
#[cfg(test)]
const TGT_SVE: c_longlong = 1 << 24;
#[cfg(test)]
const TGT_NEON_BF16: c_longlong = 1 << 26;
#[cfg(test)]
const TGT_NEON: c_longlong = 1 << 28;

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

    // -- S34-F2: x86 AVX-512 / AVX10 detection must match x86.zig:66-72,
    // 96-154 and isAMD() (:160-166). Drives cpp/detect.c's
    // x86_avx512_targets() (the gate the real x86 detector calls).

    // CPUID.7.0:EBX
    const AVX512F: u32 = 1 << 16;
    const AVX512VL: u32 = 1 << 31;
    const AVX512DQ: u32 = 1 << 17;
    const AVX512BW: u32 = 1 << 30;
    const AVX512CD: u32 = 1 << 28;
    const AVX3_EBX: u32 = AVX512F | AVX512VL | AVX512DQ | AVX512BW | AVX512CD;
    // CPUID.7.0:ECX
    const AVX3_DL_ECX: u32 = (1 << 11) // AVX512VNNI
        | (1 << 10) // VPCLMULQDQ
        | (1 << 1) // AVX512VBMI
        | (1 << 6) // AVX512VBMI2
        | (1 << 9) // VAES
        | (1 << 14) // AVX512VPOPCNTDQ
        | (1 << 12) // AVX512BITALG
        | (1 << 8); // GFNI
    const GFNI: u32 = 1 << 8;
    const AVX512FP16_EDX: u32 = 1 << 23; // CPUID.7.0:EDX
    const AVX512BF16_7_1_EAX: u32 = 1 << 5; // CPUID.7.1:EAX
    const AVX10_1_7_1_EDX: u32 = 1 << 19; // CPUID.7.1:EDX AVX10.1-256
    const AVX10_512_24_EBX: u32 = 1 << 18; // CPUID.24H:EBX AVX10.1-512

    /// OS saves SSE+AVX and the AVX-512 state (opmask, zmm_hi256, hi16_zmm).
    fn avx512_os() -> X86Leaves {
        X86Leaves {
            max_leaf: 7,
            leaf1_ecx: XSAVE | AVX,
            xcr0: 0xE6,
            ..X86Leaves::default()
        }
    }

    fn avx512(l: X86Leaves) -> c_longlong {
        // SAFETY: `l` is a live, correctly laid out `struct rustty_hwy_x86_leaves`.
        unsafe { rustty_hwy_detect_targets_x86_avx512_for_test(&l) }
    }

    #[test]
    fn avx3_requires_f_vl_dq_bw_cd() {
        let l = X86Leaves {
            leaf7_ebx: AVX3_EBX,
            ..avx512_os()
        };
        assert_eq!(avx512(l), TGT_AVX3);

        let l = X86Leaves {
            leaf7_ebx: AVX3_EBX & !AVX512CD,
            ..avx512_os()
        };
        assert_eq!(avx512(l), 0, "avx3 without AVX512CD");
    }

    #[test]
    fn avx512_needs_the_os_to_save_its_state_except_on_darwin() {
        // xcr0 saves SSE+AVX but not the AVX-512 state.
        let l = X86Leaves {
            xcr0: 0x6,
            leaf7_ebx: AVX3_EBX,
            leaf7_ecx: AVX3_DL_ECX,
            ..avx512_os()
        };
        assert_eq!(avx512(l), 0, "AVX-512 without (xcr0 & 0xE0) == 0xE0");
        // "Darwin lazily saves AVX-512 context on first use."
        assert_eq!(
            avx512(X86Leaves { is_darwin: 1, ..l }),
            TGT_AVX3 | TGT_AVX3_DL
        );

        // No AVX save at all (or no OSXSAVE): nothing, even on Darwin.
        let l = X86Leaves {
            xcr0: 0,
            is_darwin: 1,
            leaf7_ebx: AVX3_EBX,
            ..avx512_os()
        };
        assert_eq!(avx512(l), 0);
        let l = X86Leaves {
            leaf1_ecx: AVX,
            leaf7_ebx: AVX3_EBX,
            ..avx512_os()
        };
        assert_eq!(avx512(l), 0, "xgetbv is not read without OSXSAVE");

        // max_leaf < 7: nothing.
        let l = X86Leaves {
            max_leaf: 6,
            leaf7_ebx: AVX3_EBX,
            ..avx512_os()
        };
        assert_eq!(avx512(l), 0);
    }

    #[test]
    fn avx3_dl_requires_all_eight_ecx_features() {
        let l = X86Leaves {
            leaf7_ecx: AVX3_DL_ECX,
            ..avx512_os()
        };
        assert_eq!(avx512(l), TGT_AVX3_DL);
        let l = X86Leaves {
            leaf7_ecx: AVX3_DL_ECX & !GFNI,
            ..avx512_os()
        };
        assert_eq!(avx512(l), 0, "avx3_dl without GFNI");
    }

    #[test]
    fn avx3_zen4_is_bf16_on_amd_and_avx3_spr_is_fp16_plus_bf16() {
        let dl = X86Leaves {
            leaf7_eax: 1,
            leaf7_ebx: AVX3_EBX,
            leaf7_ecx: AVX3_DL_ECX,
            leaf7_1_eax: AVX512BF16_7_1_EAX,
            ..avx512_os()
        };
        // BF16 on AMD: zen4.
        assert_eq!(
            avx512(X86Leaves { is_amd: 1, ..dl }),
            TGT_AVX3 | TGT_AVX3_DL | TGT_AVX3_ZEN4
        );
        // BF16 elsewhere: not zen4.
        assert_eq!(avx512(dl), TGT_AVX3 | TGT_AVX3_DL);
        // FP16 + BF16: spr (on any vendor).
        assert_eq!(
            avx512(X86Leaves {
                leaf7_edx: AVX512FP16_EDX,
                ..dl
            }),
            TGT_AVX3 | TGT_AVX3_DL | TGT_AVX3_SPR
        );
        // FP16 without BF16: not spr.
        assert_eq!(
            avx512(X86Leaves {
                leaf7_edx: AVX512FP16_EDX,
                leaf7_1_eax: 0,
                ..dl
            }),
            TGT_AVX3 | TGT_AVX3_DL
        );
        // No sub-leaf 1 (leaf7.eax == 0): sub-leaf 1 is not consulted.
        assert_eq!(
            avx512(X86Leaves {
                leaf7_eax: 0,
                leaf7_edx: AVX512FP16_EDX,
                is_amd: 1,
                ..dl
            }),
            TGT_AVX3 | TGT_AVX3_DL
        );
        // Not avx3_dl: neither zen4 nor spr.
        assert_eq!(
            avx512(X86Leaves {
                leaf7_ecx: 0,
                leaf7_edx: AVX512FP16_EDX,
                is_amd: 1,
                ..dl
            }),
            TGT_AVX3
        );
    }

    #[test]
    fn avx10_512_enables_avx3_dl_and_spr() {
        // AVX10.1-512 with none of the legacy AVX-512 CPUID bits, and without
        // the AVX-512 save check (the AVX10 block is outside it).
        let l = X86Leaves {
            max_leaf: 0x24,
            xcr0: 0x6,
            leaf7_eax: 1,
            leaf7_1_edx: AVX10_1_7_1_EDX,
            leaf24_ebx: AVX10_512_24_EBX,
            ..avx512_os()
        };
        assert_eq!(avx512(l), TGT_AVX3 | TGT_AVX3_DL | TGT_AVX3_SPR);
        // Leaf 0x24 absent: nothing.
        assert_eq!(
            avx512(X86Leaves {
                max_leaf: 0x23,
                ..l
            }),
            0
        );
        // AVX10.1-256 only: nothing.
        assert_eq!(avx512(X86Leaves { leaf24_ebx: 0, ..l }), 0);
        // No AVX10 bit: leaf 0x24 is not consulted.
        assert_eq!(
            avx512(X86Leaves {
                leaf7_1_edx: 0,
                ..l
            }),
            0
        );
    }

    #[test]
    fn is_amd_matches_the_authentic_amd_vendor_string() {
        // "Auth" "enti" "cAMD" in ebx, edx, ecx.
        // SAFETY: pure integer function.
        assert_eq!(
            unsafe { rustty_hwy_detect_x86_is_amd_for_test(0x68747541, 0x444d4163, 0x69746e65) },
            1
        );
        // "Genu" "ineI" "ntel".
        assert_eq!(
            unsafe { rustty_hwy_detect_x86_is_amd_for_test(0x756e6547, 0x6c65746e, 0x49656e69) },
            0
        );
    }

    // -- S34-F3: aarch64 Linux NEON / SVE detection must match
    // aarch64_linux.zig:9-50. Drives cpp/detect.c's aarch64_linux_targets()
    // (the gate the real Linux detector calls).

    const HWCAP_AES: u64 = 1 << 3;
    const HWCAP_FPHP: u64 = 1 << 9;
    const HWCAP_ASIMDDP: u64 = 1 << 20;
    const HWCAP_SVE: u64 = 1 << 22;
    const HWCAP2_BF16: u64 = 1 << 14;
    const HWCAP2_SVE2: u64 = 1 << 1;
    const HWCAP2_SVEAES: u64 = 1 << 2;

    fn aarch64_linux(hwcap: u64, hwcap2: u64, vec_bytes: u64) -> c_longlong {
        // SAFETY: pure integer function.
        unsafe { rustty_hwy_detect_targets_aarch64_linux_for_test(hwcap, hwcap2, vec_bytes) }
    }

    #[test]
    fn aarch64_linux_neon_and_neon_bf16() {
        assert_eq!(aarch64_linux(0, 0, 0), 0);
        assert_eq!(aarch64_linux(HWCAP_AES, 0, 0), TGT_NEON);
        assert_eq!(
            aarch64_linux(HWCAP_AES | HWCAP_FPHP | HWCAP_ASIMDDP, HWCAP2_BF16, 0),
            TGT_NEON | TGT_NEON_BF16
        );
        // BF16 is nested under AES.
        assert_eq!(aarch64_linux(HWCAP_FPHP | HWCAP_ASIMDDP, HWCAP2_BF16, 0), 0);
    }

    #[test]
    fn aarch64_linux_sve_by_vector_length() {
        assert_eq!(aarch64_linux(HWCAP_SVE, 0, 32), TGT_SVE | TGT_SVE_256);
        assert_eq!(aarch64_linux(HWCAP_SVE, 0, 64), TGT_SVE);
        assert_eq!(aarch64_linux(HWCAP_SVE, 0, 16), 0);
        // The vector length only counts with HWCAP_SVE.
        assert_eq!(aarch64_linux(0, HWCAP2_SVE2 | HWCAP2_SVEAES, 32), 0);
    }

    #[test]
    fn aarch64_linux_sve2_needs_sveaes() {
        let sve2 = HWCAP2_SVE2 | HWCAP2_SVEAES;
        assert_eq!(
            aarch64_linux(HWCAP_SVE, sve2, 32),
            TGT_SVE | TGT_SVE_256 | TGT_SVE2
        );
        assert_eq!(aarch64_linux(HWCAP_SVE, sve2, 64), TGT_SVE | TGT_SVE2);
        assert_eq!(aarch64_linux(HWCAP_SVE, sve2, 16), TGT_SVE2_128);
        // Neither 16 nor >= 32 bytes: no SVE2 flavour.
        assert_eq!(aarch64_linux(HWCAP_SVE, sve2, 24), 0);
        assert_eq!(
            aarch64_linux(HWCAP_SVE, HWCAP2_SVE2, 32),
            TGT_SVE | TGT_SVE_256
        );
    }
}
