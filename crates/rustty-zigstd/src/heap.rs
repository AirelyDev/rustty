//! `std.heap` (Zig 0.16.0 `lib/std/heap.zig`): the per-target
//! `page_size_min` constant.
//!
//! PORTS: `std.heap.page_size_min` / `std.heap.page_size_min_default`.
//!
//! The reference resolves `page_size_min` at comptime from the target
//! `(builtin.os.tag, builtin.cpu.arch)`. The port resolves it from the same
//! target through `cfg(target_os/target_arch)`. `src/terminal/page.zig` aligns
//! a page's `total_size` to it, `PageList.zig` aligns its page pool items to it,
//! and `src/datastruct/untouched_pool.zig`'s test item is one such page.

/// `std.heap.page_size_min`: the smallest page size the target can have.
pub const PAGE_SIZE_MIN: usize = page_size_min_default();

/// Comptime `std.heap.page_size_min` for the build target, mirroring
/// `std.heap.page_size_min_default` in `lib/std/heap.zig`. We resolve it from
/// `cfg!(target_os/target_arch)` so it is a faithful per-target constant.
const fn page_size_min_default() -> usize {
    // macOS / iOS / tvOS / watchOS / visionOS / driverkit (Apple/Darwin):
    //   x86_64 => 4 KiB, aarch64 => 16 KiB.
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        16 << 10
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        4 << 10
    }
    #[cfg(all(target_os = "ios", target_arch = "aarch64"))]
    {
        16 << 10
    }
    // Wasm: the reference's `else => switch (cpu.arch)` arm maps `.wasm32` /
    // `.wasm64` to the wasm page size (64 KiB) — `memory.grow` operates in whole
    // 64 KiB pages, and the wasm page pool (`492c26067`,
    // `rustty_datastruct::wasm_page_pool`) requires `std_size` to be an exact
    // multiple of it (`Page.layout` aligns `total_size` up to this constant).
    #[cfg(target_arch = "wasm32")]
    {
        64 << 10
    }
    // Linux: every arch we support uses 4 KiB (aarch64/x86_64/etc.). sparc64 is
    // 8 KiB in the reference but we do not target it.
    #[cfg(all(
        any(target_os = "linux", target_os = "android"),
        not(target_arch = "sparc64"),
        not(target_arch = "m68k")
    ))]
    {
        4 << 10
    }
    // Fallback for any other (os, arch) we have not enumerated: 4 KiB matches
    // the reference for all currently-supported x86_64/aarch64 Unix targets
    // (FreeBSD/NetBSD/OpenBSD/etc.). This keeps the const total even though we
    // only build/test on macOS and Linux.
    #[cfg(not(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "ios", target_arch = "aarch64"),
        target_arch = "wasm32",
        all(
            any(target_os = "linux", target_os = "android"),
            not(target_arch = "sparc64"),
            not(target_arch = "m68k")
        )
    )))]
    {
        4 << 10
    }
}
