//! Build script for simdutf-sys (simdutf UTF validation/transcoding).
//!
//! PORT STATUS: vendored amalgamation
//!
//! R3 — self-contained, no ghostty dependency:
//!   1. The upstream simdutf single-header amalgamation (`simdutf.cpp` +
//!      `simdutf.h`) is vendored in-tree under `vendor/` (copied; this build
//!      NEVER references /Users/gael/projects/ghostty).
//!   2. It is compiled with the `cc` crate using the reference's flags
//!      (mirrors pkg/simdutf/build.zig + the `no_libcxx`/icelake handling in
//!      SharedDeps.zig). Ghostty builds simdutf with `.no_libcxx = true`.
//!   3. `cargo:rustc-link-*` is emitted by `cc` (static archive `libsimdutf`),
//!      and the vendored header dir is exported via `DEP_SIMDUTF_INCLUDE` so the
//!      consuming crate (rustty-simd) can `#include <simdutf.h>`.

use std::path::PathBuf;

fn main() {
    // Check-only cross lane (W2D_WINDOWS_OS / F-001): type-checking a target
    // this host has no C toolchain/sysroot for (e.g. `cargo check --target
    // x86_64-pc-windows-msvc` on a macOS host). `cargo check` never links, so
    // the native objects are not needed; opt-in via env so a real build can
    // never silently skip.
    println!("cargo:rerun-if-env-changed=RUSTTY_SYS_CHECK_ONLY");
    if std::env::var_os("RUSTTY_SYS_CHECK_ONLY").is_some() {
        return;
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let vendor = manifest_dir.join("vendor");

    let mut build = cc::Build::new();
    build
        .cpp(true)
        // Windows (MSVC ABI): the reference compiles this with Zig's bundled
        // clang, never cl.exe, and the sources rely on it (GCC/clang
        // attributes and builtins). So on an MSVC target cc drives clang-cl, the
        // MSVC-ABI clang driver (a CC/CXX override still wins). Measured on
        // Windows 11 arm64 (task W-1): cl.exe rejects the sources.
        .prefer_clang_cl_over_msvc(true)
        .std("c++17")
        .include(&vendor)
        // Vendored upstream code: do not let its warnings fail the host build.
        .warnings(false)
        .flag_if_supported("-w")
        // Zig 0.13 bug workaround carried by pkg/simdutf/build.zig: disable the
        // ICELAKE implementation.
        .define("SIMDUTF_IMPLEMENTATION_ICELAKE", Some("0"))
        // Fixes linker issues for release builds missing ubsanitizer symbols.
        .flag_if_supported("-fno-sanitize=undefined")
        .flag_if_supported("-fno-sanitize-trap=undefined");

    // The reference builds the vendored simdutf with no_libcxx (SharedDeps.zig
    // passes `.no_libcxx = true`): set SIMDUTF_NO_LIBCXX and drop
    // exceptions/rtti so we don't need the C++ standard library at runtime.
    build
        .define("SIMDUTF_NO_LIBCXX", Some("1"))
        .flag_if_supported("-fno-exceptions")
        .flag_if_supported("-fno-rtti");

    // if (target.result.abi == .msvc) { ... }  (pkg/simdutf/build.zig:55-63)
    //
    // The reference compiles with Zig's bundled clang even when `abi == .msvc`,
    // so all four tokens are clang spellings. On MSVC targets cc drives clang-cl
    // (above), which takes a clang driver flag through `/clang:`; the two
    // `-fno-*` flags are passed that way, so all four reach the compiler.
    //
    // CORRECTION (task W-1b, review W-1 F6): this was a CAVEAT that the mirror
    // was "PARTIALLY INERT": cc drove cl.exe, which rejects `-fno-autolink` and
    // `-fno-stack-protector`, so `flag_if_supported` dropped them and "there is
    // no MSVC lane here to tell which is right". MEASURED on the Windows lane
    // (`CC_ENABLE_DEBUG_OUTPUT=1`): under clang-cl the bare `-fno-*` spellings
    // were still dropped; the `/clang:` spellings are accepted and appear on
    // the compile line.
    //
    // PORT NOTE (S38-F5): the reference adds four MSVC-only flags inside the
    // same `no_libcxx` branch, and they were not mirrored. `_USE_STD_VECTOR_
    // ALGORITHMS=0` and the `_Avx2WmemEnabledWeakValue` alias exist because
    // `-fno-autolink` drops UCRT's `/alternatename` fallback, so the weak
    // symbol has to be pointed at the real one by hand; `-fno-stack-protector`
    // goes with dropping the CRT's own support. The reference's separate
    // `abi == .msvc` `-std=c++17` arm is already covered unconditionally by
    // the `.std("c++17")` above.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        build
            .define("_USE_STD_VECTOR_ALGORITHMS", Some("0"))
            // -fno-autolink also drops UCRT's /alternatename fallback.
            .define("_Avx2WmemEnabledWeakValue", Some("_Avx2WmemEnabled"))
            .flag_if_supported("/clang:-fno-autolink")
            .flag_if_supported("/clang:-fno-stack-protector");
    }

    // wasm32: with SIMDUTF_NO_LIBCXX the amalgamation needs no C++ standard
    // library at link time. The freestanding wasm toolchain has no `-lc++`, and
    // the cc crate would otherwise emit `cargo:rustc-link-lib=c++` and break the
    // final wasm link. Disable the C++ stdlib link (native targets untouched).
    if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("wasm32") {
        build.cpp_link_stdlib(None);
    }

    build.file(vendor.join("simdutf.cpp"));
    println!("cargo:rerun-if-changed=vendor/simdutf.cpp");
    println!("cargo:rerun-if-changed=vendor/simdutf.h");

    // Link-test shim: an extern "C" wrapper over a namespaced simdutf symbol so
    // the crate's link test can call into the compiled archive (not part of the
    // reference; proves the vendored lib linked). Compiled WITHOUT
    // SIMDUTF_NO_LIBCXX-driven -fno-exceptions/-fno-rtti is unnecessary; it
    // shares the same flags as the amalgamation above.
    build.file(manifest_dir.join("cbits/shim.cpp"));
    println!("cargo:rerun-if-changed=cbits/shim.cpp");

    build.compile("simdutf");

    // Export the vendored header dir so dependent crates (rustty-simd) can add
    // it to their C++ include path (Cargo sets DEP_SIMDUTF_INCLUDE).
    println!("cargo:include={}", vendor.display());

    println!("cargo:rerun-if-changed=build.rs");
}
