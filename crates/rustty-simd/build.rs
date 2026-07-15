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
    // WASM DIVERGENCE (`wasm32`): the freestanding `wasm32-unknown-unknown`
    // browser target (and the `wasm32-wasip1` VT build) compile PURE-RUST scalar
    // fallbacks instead of the C++ SIMD kernels — the `highway-sys`/`simdutf-sys`
    // deps are excluded from the wasm dependency graph (Cargo.toml), so their
    // `DEP_*_INCLUDE` vars are unset here and there is nothing to compile. Skip
    // the whole C++ build on wasm. Native/other targets are untouched.
    if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("wasm32") {
        return;
    }

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

    // wasm32: the SIMD kernels are compiled with HWY_NO_LIBCXX/SIMDUTF_NO_LIBCXX
    // (above), so they need no C++ standard library at link time. The
    // freestanding wasm toolchain has no `-lc++`; without this the cc crate emits
    // `cargo:rustc-link-lib=c++` and the final wasm link fails. Native untouched.
    if target_arch == "wasm32" {
        build.cpp_link_stdlib(None);
    }

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

    // R6 — Rust↔C FFI memory-safety gate: AddressSanitizer.
    //
    // Miri cannot interpret the foreign C++ SIMD kernels, so ASan covers UB
    // across the FFI boundary (R6). When the Rust side is built with
    // `-Zsanitizer=address` (passed via RUSTFLAGS), the C++ objects MUST also be
    // instrumented with `-fsanitize=address`, otherwise the shadow-memory model
    // is inconsistent across the boundary (the kernels would be uninstrumented
    // blind spots, and link-time/runtime ASan errors can result). `cc` does not
    // auto-propagate this from RUSTFLAGS, so we detect the sanitizer cfg here and
    // mirror the flag onto the C++ build.
    if rustflags_request_address_sanitizer() {
        build.flag("-fsanitize=address");
        // Match the Rust side's frame-pointer/debug expectations for readable
        // ASan backtraces through the C++ frames.
        build.flag_if_supported("-fno-omit-frame-pointer");
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

    // Under the ASan build, compile the version-symbol bridge as a SEPARATE,
    // UN-instrumented translation unit (it must not reference the version-check
    // symbol it defines) and link it alongside the instrumented kernels. See
    // cbits/asan_version_bridge.c for the full rationale.
    if rustflags_request_address_sanitizer() {
        let mut bridge = cc::Build::new();
        bridge
            .warnings(false)
            .flag_if_supported("-w")
            .file(cbits.join("asan_version_bridge.c"));
        // Deliberately NOT instrumented: no `-fsanitize=address` here.
        bridge.compile("rustty_simd_asan_bridge");
        println!("cargo:rerun-if-changed=cbits/asan_version_bridge.c");
    }

    println!("cargo:rerun-if-changed=build.rs");
}

/// Returns true when the Rust compilation requests AddressSanitizer via
/// `-Zsanitizer=address` (e.g. `RUSTFLAGS="-Zsanitizer=address"`). Cargo encodes
/// the active RUSTFLAGS into `CARGO_ENCODED_RUSTFLAGS` (0x1f-separated); we also
/// check the legacy `RUSTFLAGS` as a fallback. We only react to the `address`
/// sanitizer (the one that requires matching C++ instrumentation for the FFI
/// boundary gate).
fn rustflags_request_address_sanitizer() -> bool {
    println!("cargo:rerun-if-env-changed=CARGO_ENCODED_RUSTFLAGS");
    println!("cargo:rerun-if-env-changed=RUSTFLAGS");

    let mut flags: Vec<String> = Vec::new();
    if let Ok(encoded) = std::env::var("CARGO_ENCODED_RUSTFLAGS") {
        flags.extend(encoded.split('\u{1f}').map(|s| s.to_string()));
    }
    if let Ok(raw) = std::env::var("RUSTFLAGS") {
        flags.extend(raw.split_whitespace().map(|s| s.to_string()));
    }

    let mut iter = flags.iter().peekable();
    while let Some(flag) = iter.next() {
        // `-Zsanitizer=address` (single token) or `-Z sanitizer=address`
        // (split token) forms.
        if flag == "-Zsanitizer=address" {
            return true;
        }
        if flag == "-Z" {
            if let Some(next) = iter.peek() {
                if next.as_str() == "sanitizer=address" {
                    return true;
                }
            }
        }
        // Defensive: handle a comma-list like `-Zsanitizer=address,leak`.
        if let Some(rest) = flag.strip_prefix("-Zsanitizer=") {
            if rest.split(',').any(|s| s == "address") {
                return true;
            }
        }
    }
    false
}
