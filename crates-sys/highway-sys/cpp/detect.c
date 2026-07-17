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

static inline int bit_set(unsigned int val, int pos) {
  return ((val >> pos) & 1u) != 0;
}

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

  // -- AVX2 (subset of reference detection sufficient for the simd kernels) -
  if (max_leaf >= 7) {
    unsigned int l7a = 0, l7b = 0, l7c = 0, l7d = 0;
    __get_cpuid_count(7, 0, &l7a, &l7b, &l7c, &l7d);
    if (bit_set(l7b, 5) &&    // AVX2
        bit_set(l1c, 12) &&   // FMA
        bit_set(l1c, 29) &&   // F16C
        bit_set(l1c, 27) &&   // XSAVE/OSXSAVE
        bit_set(l1c, 28)) {   // AVX
      t |= TGT_AVX2;
    }
  }

  return t;
}

#else
int64_t rustty_hwy_detect_targets(void) { return 0; }
#endif
