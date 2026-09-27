//! Build script for highway-sys (Google Highway SIMD).
//!
//! PORT STATUS: vendored runtime
//!
//! R3 — self-contained, no ghostty dependency:
//!   1. The upstream Highway header tree (`hwy/`) is vendored in-tree under
//!      `vendor/` (copied; this build NEVER references
//!      /Users/gael/projects/ghostty). This is exactly what the reference's
//!      pkg/highway/build.zig adds via `addIncludePath(upstream.path(""))`.
//!   2. The three Highway runtime translation units the reference compiles
//!      (its locally-modified `abort.cc`, `per_target.cc`, `targets.cpp`) are
//!      vendored byte-identical under `cpp/` and compiled here with the
//!      reference's flags (mirrors pkg/highway/build.zig's flag list).
//!   3. `cpp/detect.c` ports the reference's Zig-provided
//!      `rustty_hwy_detect_targets` to C (R3: rustty has no Zig; the runtime's
//!      `cpp/targets.cpp` references this symbol via `extern "C"`).
//!   4. `cargo:rustc-link-*` is emitted by `cc` (static archive `libhwy`), and
//!      the vendored header dir is exported via `DEP_HWY_INCLUDE` so the
//!      consuming crate (rustty-simd) can `#include <hwy/highway.h>`.

use std::path::PathBuf;

/// Whether cc drives clang-cl here (an MSVC target, `prefer_clang_cl_over_msvc`).
fn is_msvc() -> bool {
    std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
}

/// A clang driver `-f` flag in the spelling the compiler accepts: bare for clang,
/// `/clang:<flag>` for clang-cl on MSVC targets.
///
/// Task W-2 (N347): clang-cl rejects `-fno-slp-vectorize` and `-fno-vectorize`
/// (and `-fno-cxx-exceptions`, see `no_cxx_exceptions`) in their bare spelling,
/// and `flag_if_supported` silently dropped them (MEASURED on the Windows lane
/// with `CC_ENABLE_DEBUG_OUTPUT=1`), while the reference passes all three to
/// Zig's clang on every target (`pkg/highway/build.zig:83-85`).
fn clang_flag(flag: &str) -> String {
    if is_msvc() {
        format!("/clang:{flag}")
    } else {
        flag.to_string()
    }
}

/// `-fno-cxx-exceptions` in the spelling the compiler accepts. clang-cl ignores
/// the clang driver flag even through `/clang:` ("argument unused during
/// compilation", MEASURED, task W-2) because clang-cl drives C++ exceptions with
/// `/EH`: its `/EHs-c-` is the same switch (MEASURED: with it, or with no `/EH`
/// at all, `throw` is "cannot use 'throw' with exceptions disabled").
fn no_cxx_exceptions() -> String {
    if is_msvc() {
        "/EHs-c-".to_string()
    } else {
        "-fno-cxx-exceptions".to_string()
    }
}

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
    let cpp = manifest_dir.join("cpp");

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
        // The vendored Highway headers live under vendor/ (so <hwy/highway.h>
        // resolves). The reference adds the upstream root as an include path.
        .include(&vendor)
        // Vendored upstream code: do not let its warnings fail the host build.
        .warnings(false)
        .flag_if_supported("-w")
        // Mirror pkg/highway/build.zig flags that affect ABI / behavior:
        // Highway can avoid libc++ entirely as long as all users compile
        // against the headers with the same define.
        .define("HWY_NO_LIBCXX", None)
        // Avoid changing binaries based on the current time and date.
        .flag_if_supported("-Wno-builtin-macro-redefined")
        .define("__DATE__", Some("\"redacted\""))
        .define("__TIMESTAMP__", Some("\"redacted\""))
        .define("__TIME__", Some("\"redacted\""))
        // Optimizations
        .flag_if_supported("-fmerge-all-constants")
        .flag_if_supported(no_cxx_exceptions())
        .flag_if_supported(clang_flag("-fno-slp-vectorize"))
        .flag_if_supported(clang_flag("-fno-vectorize"))
        // Fixes linker issues for release builds missing ubsanitizer symbols
        .flag_if_supported("-fno-sanitize=undefined")
        .flag_if_supported("-fno-sanitize-trap=undefined");

    // Non-Windows: the reference adds these.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        build.flag_if_supported("-fmath-errno");
        build.flag_if_supported("-fno-exceptions");
    } else if is_msvc() {
        // } else if (target.result.abi == .msvc) {  (pkg/highway/build.zig:102-108)
        //
        // Task W-2 (N347): this arm was not mirrored. Same four tokens and
        // reason as simdutf-sys's MSVC arm; the `-f` flags go through `/clang:`.
        build
            // -fno-autolink also drops UCRT's /alternatename fallback.
            .define("_Avx2WmemEnabledWeakValue", Some("_Avx2WmemEnabled"))
            .flag_if_supported("/clang:-fno-autolink")
            .flag_if_supported("/clang:-fno-stack-protector");
    }

    // wasm32: Highway is compiled with HWY_NO_LIBCXX (above), so it needs no C++
    // standard library at link time. The freestanding wasm toolchain (zig's
    // bundled libc++ is not surfaced as a plain `-lc++`) has no `-lc++`, and the
    // cc crate would otherwise emit `cargo:rustc-link-lib=c++` and break the
    // final wasm link. Disable the C++ stdlib link (native targets untouched).
    if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("wasm32") {
        build.cpp_link_stdlib(None);
    }

    // The three Highway runtime TUs the reference compiles (abort.cc,
    // per_target.cc, targets.cpp), plus the C-ported target detection.
    for src in ["cpp/abort.cc", "cpp/per_target.cc", "cpp/targets.cpp"] {
        build.file(manifest_dir.join(src));
        println!("cargo:rerun-if-changed={src}");
    }
    // detect.c is C, not C++; compile it in a separate unit to avoid the C++
    // flags above. cc handles mixed builds via a second Build instance.
    build.compile("hwy");

    let mut cbuild = cc::Build::new();
    cbuild
        // (the same compiler as the C++ unit above: clang-cl on MSVC)
        .prefer_clang_cl_over_msvc(true)
        .include(&vendor)
        .warnings(false)
        .flag_if_supported("-w")
        .file(cpp.join("detect.c"));
    println!("cargo:rerun-if-changed=cpp/detect.c");
    cbuild.compile("hwy_detect");

    // Export the vendored header dir so dependent crates (rustty-simd) can add
    // it to their own C++ include path (Cargo sets DEP_HWY_INCLUDE for any
    // crate that depends on this `links = "hwy"` crate).
    println!("cargo:include={}", vendor.display());

    println!("cargo:rerun-if-changed=build.rs");
}
