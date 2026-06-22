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
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let vendor = manifest_dir.join("vendor");

    let mut build = cc::Build::new();
    build
        .cpp(true)
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
