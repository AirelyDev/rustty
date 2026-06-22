//! Build script for rustty-simd.
//!
//! PORT STATUS: SIMD kernels (cc-built copied ghostty C++)
//!
//! R3 — self-contained, no ghostty dependency:
//!   The SIMD C++ kernels in `cbits/simd/` are ghostty's OWN code, copied
//!   VERBATIM (byte-identical) from the reference's src/simd/. They are
//!   compiled here with the `cc` crate against the vendored highway-sys +
//!   simdutf-sys headers, mirroring how the reference's
//!   src/build/SharedDeps.zig `addSimd` compiles src/simd/*.cpp.
//!
//! The flag set below mirrors SharedDeps.zig addSimd:
//!   - `-std=c++17` (unconditional, as the reference does for MSVC parity).
//!   - `-DHWY_NO_LIBCXX` (keep our SIMD sources in the same Highway header mode
//!     as the vendored package build so HWY's inline dispatch/runtime helpers
//!     have a consistent ABI).
//!   - `-DSIMDUTF_NO_LIBCXX` (vendored simdutf is built in no-libcxx mode).
//!   - On x86_64, `-DHWY_DISABLED_TARGETS=<mask>` to disable AVX512 (Zig 0.13
//!     bug workaround carried by the reference; the AVX2/AVX512 perf delta is
//!     not significant and AVX512 is rare on consumer hardware).
//!   - On Windows, disable ubsan for the C/C++ objects.
//!
//! The include path `cbits/` makes `<simd/index_of.h>` / `<simd/vt.h>` resolve
//! and `HWY_TARGET_INCLUDE "simd/<file>.cpp"` resolve, exactly as the
//! reference's `m.addIncludePath(b.path("src"))` does for src/simd.

use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let cbits = manifest_dir.join("cbits");

    let mut build = cc::Build::new();
    build
        .cpp(true)
        // Vendored/copied upstream-style C++: do not let warnings fail the host
        // build (the reference compiles these with highway's own warning model).
        .warnings(false)
        .flag_if_supported("-w")
        // include `cbits/` so `<simd/index_of.h>`, `<simd/vt.h>` and the
        // HWY_TARGET_INCLUDE "simd/<file>.cpp" multi-pass include resolve.
        .include(&cbits);

    // Vendored highway + simdutf headers, exported by the -sys crates via
    // `cargo:include=` (Cargo sets DEP_HWY_INCLUDE / DEP_SIMDUTF_INCLUDE for
    // dependents of those `links` crates).
    if let Ok(dir) = std::env::var("DEP_HWY_INCLUDE") {
        build.include(dir);
    }
    if let Ok(dir) = std::env::var("DEP_SIMDUTF_INCLUDE") {
        build.include(dir);
    }

    // MSVC requires explicit std specification otherwise these are guarded, at
    // least on Windows 2025. Doing it unconditionally doesn't cause any issues
    // on other platforms and ensures we get C++17 support on MSVC.
    build.std("c++17");

    // Keep our SIMD sources in the same Highway header mode as the vendored
    // package build so HWY's inline dispatch/runtime helpers have a consistent
    // ABI.
    build.define("HWY_NO_LIBCXX", None);

    // When using the vendored simdutf, build its headers in no-libcxx mode so we
    // don't need C++ standard library headers at all. System simdutf headers may
    // not support this define.
    build.define("SIMDUTF_NO_LIBCXX", None);

    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    // From hwy/detect_targets.h
    const HWY_AVX10_2: i64 = 1 << 3;
    const HWY_AVX3_SPR: i64 = 1 << 4;
    const HWY_AVX3_ZEN4: i64 = 1 << 6;
    const HWY_AVX3_DL: i64 = 1 << 7;
    const HWY_AVX3: i64 = 1 << 8;

    // Zig 0.13 bug: https://github.com/ziglang/zig/issues/20414
    // To workaround this we just disable AVX512 support completely. The
    // performance difference between AVX2 and AVX512 is not significant for our
    // use case and AVX512 is very rare on consumer hardware anyways.
    const HWY_DISABLED_TARGETS: i64 =
        HWY_AVX10_2 | HWY_AVX3_SPR | HWY_AVX3_ZEN4 | HWY_AVX3_DL | HWY_AVX3;
    if target_arch == "x86_64" {
        build.define(
            "HWY_DISABLED_TARGETS",
            Some(HWY_DISABLED_TARGETS.to_string().as_str()),
        );
    }

    // Disable ubsan for Windows C/C++ objects to avoid undefined
    // __ubsan_handle_* references. The Zig libraries on Windows don't currently
    // bundle a matching UBSan runtime for these objects in our build
    // configurations (this affects both MSVC and GNU ABIs).
    if target_os == "windows" {
        build.flag_if_supported("-fno-sanitize=undefined");
        build.flag_if_supported("-fno-sanitize-trap=undefined");
    }

    // The copied ghostty SIMD C++ kernels (verbatim from src/simd/).
    for src in [
        "simd/base64.cpp",
        "simd/codepoint_width.cpp",
        "simd/index_of.cpp",
        "simd/vt.cpp",
    ] {
        build.file(cbits.join(src));
        println!("cargo:rerun-if-changed=cbits/{src}");
    }
    // Headers (rebuild if they change).
    println!("cargo:rerun-if-changed=cbits/simd/index_of.h");
    println!("cargo:rerun-if-changed=cbits/simd/vt.h");

    build.compile("rustty_simd_kernels");

    println!("cargo:rerun-if-changed=build.rs");
}
