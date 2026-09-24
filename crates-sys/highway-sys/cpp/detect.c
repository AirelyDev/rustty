// rustty_hwy_detect_targets() — C port of the reference's Zig target detection.
//
// PORT NOTE (R3, rustty):
//   In the reference (ghostty), this symbol is produced by Zig
//   (pkg/highway/src/detect.zig + detect/*.zig + targets.zig). The vendored
//   Highway runtime (cpp/targets.cpp) calls it via
//   `extern "C" int64_t rustty_hwy_detect_targets();` to learn the runtime
//   target mask. rustty has no Zig, so this translation unit ports that Zig
//   detection to C verbatim (same bit layout, same probing logic, same
//   comments) so the vendored Highway runtime links and behaves identically.
//
//   The bit positions below are the `Targets` packed-struct field offsets from
//   the reference's pkg/highway/src/targets.zig (asserted there at comptime
//   against Highway's own bit constants in hwy/detect_targets.h). They are an
//   independent source from the C code under test.

#include <stdint.h>

// -- Targets bit positions (from reference targets.zig comptime asserts) ------
// x86_64
#define TGT_AVX3_SPR   (1LL << 4)
#define TGT_AVX3_ZEN4  (1LL << 6)
#define TGT_AVX3_DL    (1LL << 7)
#define TGT_AVX3       (1LL << 8)
#define TGT_AVX2       (1LL << 9)
#define TGT_SSE4       (1LL << 11)
#define TGT_SSSE3      (1LL << 12)
#define TGT_SSE2       (1LL << 14)
// aarch64
#define TGT_SVE2_128   (1LL << 18)
#define TGT_SVE_256    (1LL << 19)
#define TGT_SVE2       (1LL << 23)
#define TGT_SVE        (1LL << 24)
#define TGT_NEON_BF16  (1LL << 26)
#define TGT_NEON       (1LL << 28)
#define TGT_NEON_NO_AES (1LL << 29)
// risc-v
#define TGT_RVV        (1LL << 37)
// IBM Power / s390x (targets.zig bitPos asserts: ppc10=47, ppc9=48, ppc8=49,
// z15=50, z14=51)
#define TGT_PPC10      (1LL << 47)
#define TGT_PPC9       (1LL << 48)
#define TGT_PPC8       (1LL << 49)
#define TGT_Z15        (1LL << 50)
#define TGT_Z14        (1LL << 51)

static inline int bit_set(unsigned int val, int pos) {
  return ((val >> pos) & 1u) != 0;
}

// -- x86 AVX2 gate (portable, no CPU instructions) ---------------------------
//
// Ports pkg/highway/src/detect/x86.zig:63-94 bit-for-bit: AVX2 requires (a)
// CPUID.1:ECX OSXSAVE(27) + AVX(28), (b) XGETBV(0) reporting the OS actually
// saves SSE+AVX state ((xcr0 & 0x6) == 0x6), (c) leaf7.EBX AVX2(5) plus
// leaf1.ECX FMA(12) + F16C(29), and (d) extended leaf 0x80000001 ECX
// LZCNT(5), plus leaf7.EBX BMI(3) + BMI2(8).
//
// This is a pure function of already-read CPUID/XCR0 leaf values (S34-F1:
// the previous C port never executed xgetbv and never checked LZCNT/BMI/
// BMI2, so it could enable AVX2 kernels the oracle would refuse — a
// wrong-results/SIGILL class bug on a kernel that doesn't save AVX state).
// Keeping it free of `#if defined(__x86_64__)` and free of inline asm lets
// `rustty_hwy_detect_targets_x86_avx2_for_test` below drive it with synthetic
// leaves on ANY host (including non-x86), and lets the real x86 detector
// (further down, under the x86 `#elif`) call the exact same logic instead of
// a duplicate.
static int x86_avx2_enabled(unsigned int max_leaf, unsigned int leaf1_ecx,
                             unsigned int leaf7_ebx,
                             unsigned int ext_leaf1_ecx, unsigned int xcr0) {
  int has_xsave = bit_set(leaf1_ecx, 27);   // OSXSAVE
  int has_avx_bit = bit_set(leaf1_ecx, 28); // AVX
  // The real detector only executes `xgetbv` when both bits above are set
  // (executing it otherwise is a #UD on CPUs without XSAVE); mirror that by
  // treating xcr0 as 0 unless both are set, so a caller who forgets to zero
  // it can't accidentally satisfy the OS-save check either.
  unsigned int osxsave = (has_xsave && has_avx_bit) ? xcr0 : 0;
  int has_avx_save = (osxsave & 0x6) == 0x6; // SSE + AVX state

  if (!(has_avx_save && max_leaf >= 7)) return 0;

  if (!(bit_set(leaf7_ebx, 5) &&   // AVX2
        bit_set(leaf1_ecx, 12) &&  // FMA
        bit_set(leaf1_ecx, 29))) { // F16C
    return 0;
  }

  // Also need LZCNT (extended leaf), BMI, BMI2.
  return bit_set(ext_leaf1_ecx, 5) && // LZCNT
         bit_set(leaf7_ebx, 3) &&     // BMI
         bit_set(leaf7_ebx, 8);       // BMI2
}

// Test seam (S34-F1): exposes x86_avx2_enabled() to Rust tests with synthetic
// CPUID/XCR0 inputs. Portable (no CPU instructions), so it is callable and
// meaningful on every host this crate builds on, including non-x86 hosts
// where the real x86 detection path in rustty_hwy_detect_targets() below
// cannot execute at all.
int64_t rustty_hwy_detect_targets_x86_avx2_for_test(unsigned int max_leaf,
                                                      unsigned int leaf1_ecx,
                                                      unsigned int leaf7_ebx,
                                                      unsigned int ext_leaf1_ecx,
                                                      unsigned int xcr0) {
  return x86_avx2_enabled(max_leaf, leaf1_ecx, leaf7_ebx, ext_leaf1_ecx, xcr0)
             ? TGT_AVX2
             : 0;
}

// -- PowerPC target gate (portable, no CPU instructions) --------------------
//
// Ports pkg/highway/src/detect/ppc.zig:16-40 bit-for-bit: PPC8 requires
// AT_HWCAP ALTIVEC + VSX and AT_HWCAP2 ARCH_2_07 + VEC_CRYPTO; PPC9 adds
// AT_HWCAP2 ARCH_3_00; PPC10 adds AT_HWCAP2 ARCH_3_1 + MMA (each tier nested
// inside the previous, exactly as the oracle nests them).
//
// Kept free of `#if defined(__powerpc__)` so
// `rustty_hwy_detect_targets_ppc_for_test` below can drive it with synthetic
// hwcap/hwcap2 values on ANY host (including this non-PowerPC dev machine),
// and so the real ppc detector (further down, under the ppc `#elif`) calls
// this exact logic instead of a duplicate.
static int64_t ppc_targets_enabled(uint64_t hwcap, uint64_t hwcap2) {
  const uint64_t PPC_FEATURE_HAS_ALTIVEC = 0x10000000;
  const uint64_t PPC_FEATURE_HAS_VSX = 0x00000080;
  const uint64_t PPC_FEATURE2_ARCH_2_07 = 0x80000000; // POWER8
  const uint64_t PPC_FEATURE2_VEC_CRYPTO = 0x02000000;
  const uint64_t PPC_FEATURE2_ARCH_3_00 = 0x00800000; // POWER9
  const uint64_t PPC_FEATURE2_ARCH_3_1 = 0x00040000;  // POWER10
  const uint64_t PPC_FEATURE2_MMA = 0x00020000;

  int64_t t = 0;
  if ((hwcap & PPC_FEATURE_HAS_ALTIVEC) && (hwcap & PPC_FEATURE_HAS_VSX) &&
      (hwcap2 & PPC_FEATURE2_ARCH_2_07) &&
      (hwcap2 & PPC_FEATURE2_VEC_CRYPTO)) {
    t |= TGT_PPC8;

    if (hwcap2 & PPC_FEATURE2_ARCH_3_00) {
      t |= TGT_PPC9;

      if ((hwcap2 & PPC_FEATURE2_ARCH_3_1) && (hwcap2 & PPC_FEATURE2_MMA)) {
        t |= TGT_PPC10;
      }
    }
  }
  return t;
}

// Test seam (S34-F4): exposes ppc_targets_enabled() to Rust tests with
// synthetic AT_HWCAP/AT_HWCAP2 inputs, callable on every host this crate
// builds on.
int64_t rustty_hwy_detect_targets_ppc_for_test(uint64_t hwcap,
                                                uint64_t hwcap2) {
  return ppc_targets_enabled(hwcap, hwcap2);
}

// -- s390x target gate (portable, no CPU instructions) -----------------------
//
// Ports pkg/highway/src/detect/s390x.zig:9-25 bit-for-bit: Z14 requires
// AT_HWCAP VX + VXE; Z15 adds VXE2 (nested inside VX + VXE, as the oracle
// nests it). Note the oracle also requires VX to be set even though Z14/Z15
// checks are nested under it — mirrored here via the outer `hwcap & HWCAP_VX`
// guard.
static int64_t s390x_targets_enabled(uint64_t hwcap) {
  const uint64_t HWCAP_VX = (uint64_t)1 << 11;
  const uint64_t HWCAP_VXE = (uint64_t)1 << 13;  // z14
  const uint64_t HWCAP_VXE2 = (uint64_t)1 << 15; // z15

  int64_t t = 0;
  if (hwcap & HWCAP_VX) {
    if (hwcap & HWCAP_VXE) {
      t |= TGT_Z14;

      if (hwcap & HWCAP_VXE2) {
        t |= TGT_Z15;
      }
    }
  }
  return t;
}

// Test seam (S34-F5): exposes s390x_targets_enabled() to Rust tests with a
// synthetic AT_HWCAP input, callable on every host this crate builds on.
int64_t rustty_hwy_detect_targets_s390x_for_test(uint64_t hwcap) {
  return s390x_targets_enabled(hwcap);
}

// -- RISC-V target gate (portable, no CPU instructions) ----------------------
//
// Ports pkg/highway/src/detect/riscv.zig:9-17 bit-for-bit: RVV requires
// AT_HWCAP bit ('V' - 'A') == 21.
static int64_t riscv_targets_enabled(uint64_t hwcap) {
  const uint64_t HWCAP_V = (uint64_t)1 << ('V' - 'A'); // bit 21

  return (hwcap & HWCAP_V) ? TGT_RVV : 0;
}

// Test seam (S34-F6): exposes riscv_targets_enabled() to Rust tests with a
// synthetic AT_HWCAP input, callable on every host this crate builds on.
int64_t rustty_hwy_detect_targets_riscv_for_test(uint64_t hwcap) {
  return riscv_targets_enabled(hwcap);
}

// -- x86 AVX-512 / AVX10 gate (portable, no CPU instructions) ----------------
//
// Ports pkg/highway/src/detect/x86.zig:66-72 (has_avx512_save), :96-154
// (AVX-512, AVX10) and :160-166 (isAMD) (S34-F2: the previous C port never
// set any of avx3 / avx3_dl / avx3_zen4 / avx3_spr). The CPUID/XCR0 values
// arrive already read, so tests can drive it with synthetic leaves on any host;
// the real x86 detector below reads each leaf only when the reference does and
// passes 0 for the rest, which this function never consults.
struct rustty_hwy_x86_leaves {
  unsigned int max_leaf;   // cpuid(0).eax
  unsigned int leaf1_ecx;  // cpuid(1).ecx
  unsigned int xcr0;       // xgetbv(0).eax (read only with OSXSAVE + AVX)
  unsigned int leaf7_eax;  // cpuid(7, 0)
  unsigned int leaf7_ebx;
  unsigned int leaf7_ecx;
  unsigned int leaf7_edx;
  unsigned int leaf7_1_eax;  // cpuid(7, 1), read when leaf7_eax >= 1
  unsigned int leaf7_1_edx;
  unsigned int leaf24_ebx;  // cpuid(0x24, 0).ebx, read when max_leaf >= 0x24
  int is_amd;               // isAMD(): cpuid(0) vendor "AuthenticAMD"
  int is_darwin;            // comptime builtin.os.tag.isDarwin()
};

static int64_t x86_avx512_targets(const struct rustty_hwy_x86_leaves* l) {
  int64_t t = 0;

  // Check XSAVE / AVX OS support before enabling any AVX-dependent target.
  int has_xsave = bit_set(l->leaf1_ecx, 27);
  int has_avx_bit = bit_set(l->leaf1_ecx, 28);
  unsigned int xcr0 = (has_xsave && has_avx_bit) ? l->xcr0 : 0;
  int has_avx_save = (xcr0 & 0x6) == 0x6; // SSE + AVX state

  // Darwin lazily saves AVX-512 context on first use.
  int has_avx512_save =
      l->is_darwin ? 1
                   : (xcr0 & 0xE0) == 0xE0; // opmask + zmm_hi256 + hi16_zmm

  if (!(has_avx_save && l->max_leaf >= 7)) return 0;

  // -- AVX-512 ---------------------------------------------------------------
  if (has_avx512_save) {
    if (bit_set(l->leaf7_ebx, 16) && // AVX512F
        bit_set(l->leaf7_ebx, 31) && // AVX512VL
        bit_set(l->leaf7_ebx, 17) && // AVX512DQ
        bit_set(l->leaf7_ebx, 30) && // AVX512BW
        bit_set(l->leaf7_ebx, 28)) { // AVX512CD
      t |= TGT_AVX3;
    }

    if (bit_set(l->leaf7_ecx, 11) && // AVX512VNNI
        bit_set(l->leaf7_ecx, 10) && // VPCLMULQDQ (AVX save ok)
        bit_set(l->leaf7_ecx, 1) &&  // AVX512VBMI
        bit_set(l->leaf7_ecx, 6) &&  // AVX512VBMI2
        bit_set(l->leaf7_ecx, 9) &&  // VAES (AVX save ok)
        bit_set(l->leaf7_ecx, 14) && // AVX512VPOPCNTDQ
        bit_set(l->leaf7_ecx, 12) && // AVX512BITALG
        bit_set(l->leaf7_ecx, 8)) {  // GFNI
      t |= TGT_AVX3_DL;
    }

    // AVX512BF16 is in leaf 7 sub-1.
    if ((t & TGT_AVX3_DL) && l->leaf7_eax >= 1) {
      if (bit_set(l->leaf7_1_eax, 5)) { // AVX512BF16
        if (l->is_amd) {
          t |= TGT_AVX3_ZEN4;
        }
      }

      if (bit_set(l->leaf7_edx, 23) &&   // AVX512FP16
          bit_set(l->leaf7_1_eax, 5)) {  // AVX512BF16
        t |= TGT_AVX3_SPR;
      }
    } else if (bit_set(l->leaf7_edx, 23)) { // AVX512FP16 without sub-leaf
      // Can't check BF16 without sub-leaf support, skip avx3_spr.
    }
  }

  // -- AVX10 -----------------------------------------------------------------
  if (l->max_leaf >= 7 && l->leaf7_eax >= 1) {
    if (bit_set(l->leaf7_1_edx, 19)) { // AVX10.1-256
      if (l->max_leaf >= 0x24) {
        if (bit_set(l->leaf24_ebx, 18)) { // AVX10.1-512
          t |= TGT_AVX3_SPR;
          t |= TGT_AVX3_DL;
          t |= TGT_AVX3;
        }
      }

      // AVX10.2 detection would require a leaf we can't
      // reliably check yet; leave for future.
    }
  }

  return t;
}

// isAMD(): cpuid(0) ebx/edx/ecx spell "Auth" "enti" "cAMD" (x86.zig:160-166).
static int x86_is_amd(unsigned int leaf0_ebx, unsigned int leaf0_ecx,
                      unsigned int leaf0_edx) {
  return leaf0_ebx == 0x68747541 && leaf0_ecx == 0x444d4163 &&
         leaf0_edx == 0x69746e65;
}

// Test seams (S34-F2): x86_avx512_targets() and x86_is_amd() with synthetic
// leaves, callable on every host this crate builds on.
int64_t rustty_hwy_detect_targets_x86_avx512_for_test(
    const struct rustty_hwy_x86_leaves* leaves) {
  return x86_avx512_targets(leaves);
}

int rustty_hwy_detect_x86_is_amd_for_test(unsigned int leaf0_ebx,
                                          unsigned int leaf0_ecx,
                                          unsigned int leaf0_edx) {
  return x86_is_amd(leaf0_ebx, leaf0_ecx, leaf0_edx);
}

// -- aarch64 Linux gate (portable, no CPU instructions) ----------------------
//
// Ports pkg/highway/src/detect/aarch64_linux.zig:9-50 (S34-F3: the previous C
// port probed the SVE vector length, discarded it, and never set sve /
// sve_256 / sve2 / sve2_128). `vec_bytes` is sveVectorBytes()'s result; the
// real detector computes it only when HWCAP_SVE is set, as the reference does,
// and this function reads it only then. The baseline neon_without_aes bit is
// the caller's (detect.zig:47).
static int64_t aarch64_linux_targets(uint64_t hwcap, uint64_t hwcap2,
                                     uint64_t vec_bytes) {
  // Bit positions from Linux UAPI asm/hwcap.h. Trailing-underscore names so we
  // don't collide with the HWCAP_*/HWCAP2_* macros glibc's <sys/auxv.h> pulls
  // in via <bits/hwcap.h>.
  const uint64_t HWCAP_AES_ = (uint64_t)1 << 3;
  const uint64_t HWCAP_FPHP_ = (uint64_t)1 << 9;     // FEAT_FP16
  const uint64_t HWCAP_ASIMDDP_ = (uint64_t)1 << 20; // DotProd
  const uint64_t HWCAP_SVE_ = (uint64_t)1 << 22;

  const uint64_t HWCAP2_BF16_ = (uint64_t)1 << 14;
  const uint64_t HWCAP2_SVE2_ = (uint64_t)1 << 1;
  const uint64_t HWCAP2_SVEAES_ = (uint64_t)1 << 2;

  int64_t t = 0;
  if (hwcap & HWCAP_AES_) {
    t |= TGT_NEON;

    if ((hwcap & HWCAP_FPHP_) && (hwcap & HWCAP_ASIMDDP_) &&
        (hwcap2 & HWCAP2_BF16_)) {
      t |= TGT_NEON_BF16;
    }
  }

  if (hwcap & HWCAP_SVE_) {
    if (vec_bytes >= 32) {
      t |= TGT_SVE;
      if (vec_bytes == 32) {
        t |= TGT_SVE_256;
      }
    }

    if ((hwcap2 & HWCAP2_SVE2_) && (hwcap2 & HWCAP2_SVEAES_)) {
      if (vec_bytes >= 32) {
        t |= TGT_SVE2;
      } else if (vec_bytes == 16) {
        t |= TGT_SVE2_128;
      }
    }
  }

  return t;
}

// Test seam (S34-F3): aarch64_linux_targets() with synthetic hwcap/hwcap2 and
// SVE vector length, callable on every host this crate builds on.
int64_t rustty_hwy_detect_targets_aarch64_linux_for_test(uint64_t hwcap,
                                                         uint64_t hwcap2,
                                                         uint64_t vec_bytes) {
  return aarch64_linux_targets(hwcap, hwcap2, vec_bytes);
}

#if defined(__aarch64__) || defined(_M_ARM64)

#if defined(__APPLE__)
#include <sys/sysctl.h>

// We can rely on libc for macOS because libsystem is always available.
static int darwin_sysctl_bool(const char* name) {
  int value = 0;
  size_t len = sizeof(int);
  int rc = sysctlbyname(name, &value, &len, 0, 0);
  return rc == 0 && value != 0;
}

int64_t rustty_hwy_detect_targets(void) {
  int64_t t = 0;

  // All AArch64 implementations have NEON.
  t |= TGT_NEON_NO_AES;

  // All Apple Silicon has AES.
  t |= TGT_NEON;

  // Every Apple chip from A11 (2017) onward has FP16 + DotProd.
  // BF16 arrived with M2 / A15 (ARM_BLIZZARD_AVALANCHE, 2022).
  // We probe hw.optional.arm.FEAT_BF16 to be precise.
  if (darwin_sysctl_bool("hw.optional.arm.FEAT_BF16")) {
    t |= TGT_NEON_BF16;
  }

  // Apple Silicon does not support SVE.
  return t;
}

#elif defined(__linux__)
#include <sys/auxv.h>
#include <sys/prctl.h>

static long sve_vector_bytes(void) {
  // PR_SVE_GET_VL returns the SVE vector length in the lower 16 bits.
  const int PR_SVE_GET_VL_ = 51;
  long ret = prctl(PR_SVE_GET_VL_, 0, 0, 0, 0);
  if (ret >= 0) {
    return ret & 0xFFFF;
  }
  // prctl failed: assume 128-bit (NEON-width, conservative).
  return 16;
}

int64_t rustty_hwy_detect_targets(void) {
  int64_t t = 0;

  // All AArch64 implementations have NEON.
  t |= TGT_NEON_NO_AES;

  // Linux exposes AArch64 features via getauxval(AT_HWCAP / AT_HWCAP2).
  const unsigned long AT_HWCAP_ = 16;
  const unsigned long AT_HWCAP2_ = 26;

  unsigned long hwcap = getauxval(AT_HWCAP_);
  unsigned long hwcap2 = getauxval(AT_HWCAP2_);

  // sveVectorBytes() runs only when HWCAP_SVE is set (aarch64_linux.zig:33),
  // the only case aarch64_linux_targets() reads it.
  const unsigned long HWCAP_SVE_ = 1UL << 22;
  uint64_t vec_bytes = (hwcap & HWCAP_SVE_) ? (uint64_t)sve_vector_bytes() : 0;

  // NEON / NEON_BF16 / SVE / SVE_256 / SVE2 / SVE2_128 (S34-F3), via the
  // shared aarch64_linux_targets() gate defined above.
  t |= aarch64_linux_targets(hwcap, hwcap2, vec_bytes);

  return t;
}

#else
int64_t rustty_hwy_detect_targets(void) {
  // Other OS: return baseline NEON.
  return TGT_NEON_NO_AES;
}
#endif

#elif defined(__x86_64__) || defined(__i386__) || defined(_M_X64) || \
    defined(_M_IX86)
#include <cpuid.h>

int64_t rustty_hwy_detect_targets(void) {
  int64_t t = 0;

  // x86_64 always has SSE2.
#if defined(__x86_64__) || defined(_M_X64)
  t |= TGT_SSE2;
#endif

  unsigned int a = 0, b = 0, c = 0, d = 0;
  if (!__get_cpuid(0, &a, &b, &c, &d)) return t;
  unsigned int max_leaf = a;
  if (max_leaf < 1) return t;

  unsigned int l1a = 0, l1b = 0, l1c = 0, l1d = 0;
  __get_cpuid(1, &l1a, &l1b, &l1c, &l1d);

  // -- SSE2 on 32-bit x86 -------------------------------------------------
#if defined(__i386__) || defined(_M_IX86)
  if (bit_set(l1d, 25) && bit_set(l1d, 26)) {
    t |= TGT_SSE2;
  }
#endif

  // -- SSSE3 ---------------------------------------------------------------
  if (bit_set(l1c, 0) &&  // SSE3
      bit_set(l1c, 9)) {  // SSSE3
    t |= TGT_SSSE3;
  }

  // -- SSE4 ----------------------------------------------------------------
  if (bit_set(l1c, 19) &&  // SSE4.1
      bit_set(l1c, 20) &&  // SSE4.2
      bit_set(l1c, 1) &&   // PCLMUL
      bit_set(l1c, 25)) {  // AES
    t |= TGT_SSE4;
  }

  // -- AVX2 ------------------------------------------------------------------
  // Ports x86.zig:63-94 (S34-F1): OS-support check (XGETBV) + LZCNT/BMI/BMI2,
  // via the shared x86_avx2_enabled() gate defined above.
  if (max_leaf >= 7) {
    unsigned int l7a = 0, l7b = 0, l7c = 0, l7d = 0;
    __get_cpuid_count(7, 0, &l7a, &l7b, &l7c, &l7d);

    // XGETBV(0) reports which state components the OS actually saves/
    // restores on context switch. Only execute it when CPUID says XSAVE
    // (OSXSAVE, bit 27) and AVX (bit 28) are both present — executing xgetbv
    // otherwise is a #UD.
    unsigned int xcr0 = 0;
    if (bit_set(l1c, 27) && bit_set(l1c, 28)) {
      unsigned int xcr0_eax = 0, xcr0_edx = 0;
      __asm__ __volatile__("xgetbv"
                            : "=a"(xcr0_eax), "=d"(xcr0_edx)
                            : "c"(0));
      xcr0 = xcr0_eax;
    }

    unsigned int e1a = 0, e1b = 0, e1c = 0, e1d = 0;
    __get_cpuid(0x80000001, &e1a, &e1b, &e1c, &e1d);

    if (x86_avx2_enabled(max_leaf, l1c, l7b, e1c, xcr0)) {
      t |= TGT_AVX2;
    }

    // -- AVX-512 / AVX10 (S34-F2) --------------------------------------------
    // Ports x86.zig:96-154 via the shared x86_avx512_targets() gate defined
    // above. cpuid(7, 1) is read only when leaf 7 reports sub-leaves and
    // cpuid(0x24) only when it exists, as the reference reads them.
    struct rustty_hwy_x86_leaves leaves = {0};
    leaves.max_leaf = max_leaf;
    leaves.leaf1_ecx = l1c;
    leaves.xcr0 = xcr0;
    leaves.leaf7_eax = l7a;
    leaves.leaf7_ebx = l7b;
    leaves.leaf7_ecx = l7c;
    leaves.leaf7_edx = l7d;
    if (l7a >= 1) {
      unsigned int s1a = 0, s1b = 0, s1c = 0, s1d = 0;
      __get_cpuid_count(7, 1, &s1a, &s1b, &s1c, &s1d);
      leaves.leaf7_1_eax = s1a;
      leaves.leaf7_1_edx = s1d;
    }
    if (max_leaf >= 0x24) {
      unsigned int x24a = 0, x24b = 0, x24c = 0, x24d = 0;
      __get_cpuid_count(0x24, 0, &x24a, &x24b, &x24c, &x24d);
      leaves.leaf24_ebx = x24b;
    }
    leaves.is_amd = x86_is_amd(b, c, d);
#if defined(__APPLE__)
    leaves.is_darwin = 1;
#endif
    t |= x86_avx512_targets(&leaves);
  }

  return t;
}

// -- PowerPC ------------------------------------------------------------
#elif defined(__powerpc__) || defined(__powerpc64__)

#if defined(__linux__)
#include <sys/auxv.h>

int64_t rustty_hwy_detect_targets(void) {
  // From Linux arch/powerpc/include/uapi/asm/cputable.h, read via
  // getauxval(AT_HWCAP / AT_HWCAP2) exactly as ppc.zig:9-13 does.
  const unsigned long AT_HWCAP_ = 16;
  const unsigned long AT_HWCAP2_ = 26;

  uint64_t hwcap = getauxval(AT_HWCAP_);
  uint64_t hwcap2 = getauxval(AT_HWCAP2_);

  return ppc_targets_enabled(hwcap, hwcap2);
}

#else
int64_t rustty_hwy_detect_targets(void) {
  // Non-Linux: the oracle's ppc.zig returns the zero target struct.
  return 0;
}
#endif

// -- s390x ----------------------------------------------------------------
#elif defined(__s390x__)

#if defined(__linux__)
#include <sys/auxv.h>

int64_t rustty_hwy_detect_targets(void) {
  // From Linux arch/s390/include/asm/elf.h, read via getauxval(AT_HWCAP)
  // exactly as s390x.zig:9-12 does.
  const unsigned long AT_HWCAP_ = 16;

  uint64_t hwcap = getauxval(AT_HWCAP_);

  return s390x_targets_enabled(hwcap);
}

#else
int64_t rustty_hwy_detect_targets(void) {
  // Non-Linux: the oracle's s390x.zig returns the zero target struct.
  return 0;
}
#endif

// -- RISC-V -----------------------------------------------------------------
#elif defined(__riscv)

#if defined(__linux__)
#include <sys/auxv.h>

int64_t rustty_hwy_detect_targets(void) {
  // Read via getauxval(AT_HWCAP) exactly as riscv.zig:9-12 does.
  const unsigned long AT_HWCAP_ = 16;

  uint64_t hwcap = getauxval(AT_HWCAP_);

  return riscv_targets_enabled(hwcap);
}

#else
int64_t rustty_hwy_detect_targets(void) {
  // Non-Linux: the oracle's riscv.zig returns the zero target struct.
  return 0;
}
#endif

#else
int64_t rustty_hwy_detect_targets(void) { return 0; }
#endif
