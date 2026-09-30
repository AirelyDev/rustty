//! `std.atomic` (Zig 0.16.0 `lib/std/atomic.zig`): the per-target
//! `cache_line` constant.
//!
//! PORTS: `std.atomic.cache_line` / `std.atomic.cacheLineForCpu`.
//!
//! The reference resolves `cache_line` at comptime from the target CPU
//! (`cacheLineForCpu(builtin.cpu)`). The port resolves it from the same
//! target through `cfg(target_arch)`, arm for arm, so the value is the
//! reference's on every target Rust can name, not a guess of the host's.
//! `src/terminal/page.zig` aligns a page's cell array to it (`cells_align`,
//! upstream `d2ff6d77a`), so it feeds the page geometry: the rows per page,
//! `maxCols` and the page size.
//!
//! Zig arches Rust has no `target_arch` for (`arc`, `arceb`, `microblaze`,
//! `microblazeel`, `sh`, `sheb`, `hppa`, `hppa64`, `or1k`, `ve`) cannot be a
//! Rust build target, so their arms have no `cfg`. Rust's endian variants
//! share one `target_arch` (`aarch64` covers `aarch64_be`, `arm` covers
//! `armeb`/`thumb`/`thumbeb`, `mips` and `mips64` cover the `el` variants,
//! `powerpc64` covers `powerpc64le`), and the reference gives each pair the
//! same value. `arm64ec` is an aarch64 CPU (the reference has no arm64ec;
//! its CPU arch there would be `aarch64`), so it takes the aarch64 value.

/// The estimated size of the CPU's cache line when atomically updating memory.
/// Add this much padding or align to this boundary to avoid atomically-updated
/// memory from forcing cache invalidations on near, but non-atomic, memory.
///
/// https://en.wikipedia.org/wiki/False_sharing
/// https://github.com/golang/go/search?q=CacheLinePadSize
pub const CACHE_LINE: usize = cache_line_for_cpu();

/// A zero-sized marker aligned to [`CACHE_LINE`]: `align(std.atomic.cache_line)`.
///
/// PORT NOTE: `#[repr(align(N))]` takes a literal, not a `const`, so the
/// per-target values are repeated here as `cfg_attr` arms of the same table,
/// and the `const _` below proves they agree with [`CACHE_LINE`] on every
/// target. Put a `[CacheLineAligned; 0]` field in a struct to give it the
/// reference's `align(std.atomic.cache_line)`.
#[derive(Clone, Copy, Debug, Default)]
#[cfg_attr(
    any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "arm64ec",
        target_arch = "powerpc64"
    ),
    repr(align(128))
)]
#[cfg_attr(
    any(
        target_arch = "hexagon",
        target_arch = "arm",
        target_arch = "mips",
        target_arch = "mips32r6",
        target_arch = "mips64",
        target_arch = "mips64r6",
        target_arch = "sparc",
        target_arch = "sparc64"
    ),
    repr(align(32))
)]
#[cfg_attr(target_arch = "m68k", repr(align(16)))]
#[cfg_attr(target_arch = "msp430", repr(align(8)))]
#[cfg_attr(target_arch = "s390x", repr(align(256)))]
#[cfg_attr(
    not(any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "arm64ec",
        target_arch = "powerpc64",
        target_arch = "hexagon",
        target_arch = "arm",
        target_arch = "mips",
        target_arch = "mips32r6",
        target_arch = "mips64",
        target_arch = "mips64r6",
        target_arch = "sparc",
        target_arch = "sparc64",
        target_arch = "m68k",
        target_arch = "msp430",
        target_arch = "s390x"
    )),
    repr(align(64))
)]
pub struct CacheLineAligned;

const _: () = assert!(core::mem::align_of::<CacheLineAligned>() == CACHE_LINE);

/// `cacheLineForCpu(builtin.cpu)` for the build target.
const fn cache_line_for_cpu() -> usize {
    // x86_64: Starting from Intel's Sandy Bridge, the spatial prefetcher pulls in pairs of 64-byte cache lines at a time.
    // - https://www.intel.com/content/dam/www/public/us/en/documents/manuals/64-ia-32-architectures-optimization-manual.pdf
    // - https://github.com/facebook/folly/blob/1b5288e6eea6df074758f877c849b6e73bbb9fbb/folly/lang/Align.h#L107
    //
    // aarch64: Some big.LITTLE ARM archs have "big" cores with 128-byte cache lines:
    // - https://www.mono-project.com/news/2016/09/12/arm64-icache/
    // - https://cpufun.substack.com/p/more-m1-fun-hardware-information
    //
    // - https://github.com/torvalds/linux/blob/3a7e02c040b130b5545e4b115aada7bacd80a2b6/arch/arc/Kconfig#L212
    // - https://github.com/golang/go/blob/3dd58676054223962cd915bb0934d1f9f489d4d2/src/internal/cpu/cpu_ppc64x.go#L9
    if cfg!(any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "arm64ec",
        target_arch = "powerpc64"
    )) {
        return 128;
    }

    // https://github.com/llvm/llvm-project/blob/e379094328e49731a606304f7e3559d4f1fa96f9/clang/lib/Basic/Targets/Hexagon.h#L145-L151
    //
    // PORT NOTE: the reference picks 64 for a CPU with the `v73` feature and
    // 32 otherwise. rustc (1.93) has no Hexagon ISA-version target feature to
    // test (`cfg(target_feature = "v73")` is an unknown value), so the port
    // takes the pre-v73 arm, 32. No Rustty build targets Hexagon.
    if cfg!(target_arch = "hexagon") {
        return 32;
    }

    // - https://github.com/golang/go/blob/3dd58676054223962cd915bb0934d1f9f489d4d2/src/internal/cpu/cpu_arm.go#L7
    // - https://github.com/golang/go/blob/3dd58676054223962cd915bb0934d1f9f489d4d2/src/internal/cpu/cpu_mips.go#L7
    // - https://github.com/golang/go/blob/3dd58676054223962cd915bb0934d1f9f489d4d2/src/internal/cpu/cpu_mipsle.go#L7
    // - https://github.com/golang/go/blob/3dd58676054223962cd915bb0934d1f9f489d4d2/src/internal/cpu/cpu_mips64x.go#L9
    // - https://github.com/torvalds/linux/blob/3a7e02c040b130b5545e4b115aada7bacd80a2b6/arch/sparc/include/asm/cache.h#L14
    // - https://github.com/torvalds/linux/blob/3a7e02c040b130b5545e4b115aada7bacd80a2b6/arch/microblaze/include/asm/cache.h#L15
    // - https://github.com/torvalds/linux/blob/3a7e02c040b130b5545e4b115aada7bacd80a2b6/arch/sh/include/cpu-sh4/cpu/cache.h#L10
    if cfg!(any(
        target_arch = "arm",
        target_arch = "mips",
        target_arch = "mips32r6",
        target_arch = "mips64",
        target_arch = "mips64r6",
        target_arch = "sparc",
        target_arch = "sparc64"
    )) {
        return 32;
    }

    // - https://github.com/torvalds/linux/blob/3a7e02c040b130b5545e4b115aada7bacd80a2b6/arch/m68k/include/asm/cache.h#L10
    // - https://github.com/torvalds/linux/blob/3a7e02c040b130b5545e4b115aada7bacd80a2b6/arch/openrisc/include/asm/cache.h#L24
    // - https://github.com/torvalds/linux/blob/3a7e02c040b130b5545e4b115aada7bacd80a2b6/arch/parisc/include/asm/cache.h#L16
    if cfg!(target_arch = "m68k") {
        return 16;
    }

    // - https://www.ti.com/lit/pdf/slaa498
    if cfg!(target_arch = "msp430") {
        return 8;
    }

    // - https://github.com/golang/go/blob/3dd58676054223962cd915bb0934d1f9f489d4d2/src/internal/cpu/cpu_s390x.go#L7
    // - https://sxauroratsubasa.sakura.ne.jp/documents/guide/pdfs/Aurora_ISA_guide.pdf
    if cfg!(target_arch = "s390x") {
        return 256;
    }

    // Other x86 and WASM platforms have 64-byte cache lines.
    // The rest of the architectures are assumed to be similar.
    // - https://github.com/golang/go/blob/dda2991c2ea0c5914714469c4defc2562a907230/src/internal/cpu/cpu_x86.go#L9
    // - https://github.com/golang/go/blob/0a9321ad7f8c91e1b0c7184731257df923977eb9/src/internal/cpu/cpu_loong64.go#L11
    // - https://github.com/golang/go/blob/3dd58676054223962cd915bb0934d1f9f489d4d2/src/internal/cpu/cpu_wasm.go#L7
    // - https://github.com/golang/go/blob/19e923182e590ae6568c2c714f20f32512aeb3e3/src/internal/cpu/cpu_riscv64.go#L7
    // - https://github.com/torvalds/linux/blob/3a7e02c040b130b5545e4b115aada7bacd80a2b6/arch/xtensa/variants/csp/include/variant/core.h#L209
    // - https://github.com/torvalds/linux/blob/3a7e02c040b130b5545e4b115aada7bacd80a2b6/arch/csky/Kconfig#L183
    // - https://github.com/torvalds/linux/blob/3a7e02c040b130b5545e4b115aada7bacd80a2b6/arch/alpha/include/asm/cache.h#L11
    // - https://www.xmos.com/download/The-XMOS-XS3-Architecture.pdf
    64
}

#[cfg(test)]
mod tests {
    use super::*;

    // Rustty-only: the host value is the reference's table entry for the
    // host arch (`cacheLineForCpu`, Zig 0.16.0 `lib/std/atomic.zig:413-491`).
    #[test]
    fn cache_line_matches_the_reference_table_for_the_host() {
        let want = if cfg!(any(target_arch = "x86_64", target_arch = "aarch64")) {
            128
        } else if cfg!(target_arch = "wasm32") || cfg!(target_arch = "x86") {
            64
        } else {
            CACHE_LINE
        };
        assert_eq!(want, CACHE_LINE);
        assert!(CACHE_LINE.is_power_of_two());
    }
}
