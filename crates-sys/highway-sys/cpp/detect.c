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
#define TGT_AVX3       (1LL << 8)
#define TGT_AVX2       (1LL << 9)
#define TGT_SSE4       (1LL << 11)
#define TGT_SSSE3      (1LL << 12)
#define TGT_SSE2       (1LL << 14)
// aarch64
#define TGT_NEON_BF16  (1LL << 26)
#define TGT_NEON       (1LL << 28)
#define TGT_NEON_NO_AES (1LL << 29)

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

  // Bit positions from Linux UAPI asm/hwcap.h. Trailing-underscore names (like
  // AT_HWCAP_ / AT_HWCAP2_ / PR_SVE_GET_VL_ above) so we don't collide with the
  // HWCAP_*/HWCAP2_* macros glibc's <sys/auxv.h> pulls in via <bits/hwcap.h>.
  const unsigned long HWCAP_AES_ = 1UL << 3;
  const unsigned long HWCAP_FPHP_ = 1UL << 9;     // FEAT_FP16
  const unsigned long HWCAP_ASIMDDP_ = 1UL << 20; // DotProd
  const unsigned long HWCAP_SVE_ = 1UL << 22;

  const unsigned long HWCAP2_BF16_ = 1UL << 14;

  if (hwcap & HWCAP_AES_) {
    t |= TGT_NEON;

    if ((hwcap & HWCAP_FPHP_) && (hwcap & HWCAP_ASIMDDP_) &&
        (hwcap2 & HWCAP2_BF16_)) {
      t |= TGT_NEON_BF16;
    }
  }

  // SVE width probing (sve/sve2 bits are not used by the simd kernels here,
  // but we probe to keep the mask faithful to the reference).
  if (hwcap & HWCAP_SVE_) {
    (void)sve_vector_bytes();
  }

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
  }

  return t;
}

#else
int64_t rustty_hwy_detect_targets(void) { return 0; }
#endif
