//! Build script for zlib-sys (zlib 1.3.1 compression).
//!
//! PORT STATUS: vendor-proof slice
//!
//! R3 — self-contained, no ghostty dependency:
//!   1. The upstream C sources for `z` are vendored in-tree under `vendor/`
//!      (copied; this build NEVER references /Users/gael/projects/ghostty).
//!   2. They are compiled with the `cc` crate (matching the reference's
//!      portable build: warnings off for vendored upstream code).
//!   3. The thin FFI surface is hand-written in `src/lib.rs` (bindgen is added
//!      when a wider header surface is vendored).
//!   4. `cargo:rustc-link-*` is emitted by `cc` (static archive `libz`).
//!
//! Phase-1 vendored the `adler32` checksum translation unit — a small, fully
//! self-contained unit — to prove the R3 toolchain links end-to-end. Additional
//! zlib translation units are appended to `SOURCES` as the crates that need them
//! are carved; the pattern is unchanged. The Kitty graphics image port
//! (`terminal/kitty/graphics_image.zig`) needs DEFLATE decompression
//! (`std.compress.flate`), so the verbatim upstream inflate translation units
//! (`crc32`, `inftrees`, `inffast`, `inflate`) plus the one-shot `uncompr`
//! entry point and shared `zutil` support are vendored and compiled here.
//!
//! P3 self-contained lib: the DEFLATE **compression** translation units
//! (`deflate`, `trees`, `compress`) are now vendored and compiled too, so the
//! archive `cc` emits is FULL zlib — both inflate and deflate. The macOS Swift
//! app links Sentry's crash reporter, which calls `deflate`/`deflateInit2_`/
//! `deflateEnd` to gzip its envelopes; folding those objects into the combined
//! `libGhosttyKit.a` (xtask `xcframework`) makes the shipped archive
//! self-contained for zlib, so the app no longer needs a system `-lz`. All
//! vendored `.c`/`.h` are verbatim upstream zlib 1.3.1 (byte-identical, R3).

use std::path::PathBuf;

/// Vendored zlib translation units compiled into this crate.
///
/// The reference's `srcs` list, verbatim and in its order
/// (`pkg/zlib/build.zig:61-77`): all 15 units of zlib 1.3.1, including the
/// `gz*` file API (`gzclose`, `gzlib`, `gzread`, `gzwrite`) and `infback`
/// (`inflateBack*`), which the port used to omit (S39-F5). No in-tree consumer
/// calls those two surfaces; they are compiled because the reference's `libz`
/// carries them. (This list also used to name `deflate`, `trees` and
/// `compress` twice.)
const SOURCES: &[&str] = &[
    "vendor/adler32.c",
    "vendor/compress.c",
    "vendor/crc32.c",
    "vendor/deflate.c",
    "vendor/gzclose.c",
    "vendor/gzlib.c",
    "vendor/gzread.c",
    "vendor/gzwrite.c",
    "vendor/inflate.c",
    "vendor/infback.c",
    "vendor/inftrees.c",
    "vendor/inffast.c",
    "vendor/trees.c",
    "vendor/uncompr.c",
    "vendor/zutil.c",
];

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
        // Windows (MSVC ABI), task F-1 (N353): the reference compiles this with
        // Zig's clang even when `abi == .msvc`, so on an MSVC target cc drives
        // clang-cl (the MSVC-ABI clang driver; a CC/CXX override still wins),
        // not cl.exe. A no-op on every other target.
        .prefer_clang_cl_over_msvc(true)
        .include(&vendor)
        // Vendored upstream code: do not let its warnings fail the host build.
        .warnings(false)
        .flag_if_supported("-w");

    // The reference's flags (`pkg/zlib/build.zig:28-50`). `Z_HAVE_UNISTD_H`
    // matters to the `gz*` units: `zconf.h` then includes `<unistd.h>`, which
    // declares the `read`/`write`/`close`/`lseek` they call.
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    build
        .define("HAVE_SYS_TYPES_H", None)
        .define("HAVE_STDINT_H", None)
        .define("HAVE_STDDEF_H", None);
    if msvc {
        build
            .flag("-fno-sanitize=undefined")
            .flag("-fno-sanitize-trap=undefined");
    }
    if target_os != "windows" {
        build.define("Z_HAVE_UNISTD_H", None);
    }
    if msvc {
        build
            .define("_CRT_SECURE_NO_DEPRECATE", None)
            .define("_CRT_NONSTDC_NO_DEPRECATE", None);
    }
    for src in SOURCES {
        build.file(manifest_dir.join(src));
        println!("cargo:rerun-if-changed={src}");
    }
    // Compiles the vendored sources into a static archive and emits the
    // `cargo:rustc-link-lib=static=z` + search-path directives. Because the
    // crate declares `links = "z"`, Cargo enforces a single linkage of this lib.
    build.compile("z");

    // Export the vendored header dir so dependent crates can add it to their C
    // include path (Cargo sets `DEP_Z_INCLUDE`). freetype-sys builds FreeType
    // with `FT_CONFIG_OPTION_SYSTEM_ZLIB=1`, whose `src/gzip/ftgzip.c` does
    // `#include <zlib.h>` — the reference gets this include via
    // `lib.linkLibrary(zlib_dep)`; here it flows through `DEP_Z_INCLUDE`.
    println!("cargo:include={}", vendor.display());

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=vendor/zlib.h");
    println!("cargo:rerun-if-changed=vendor/zconf.h");
    println!("cargo:rerun-if-changed=vendor/deflate.h");
    println!("cargo:rerun-if-changed=vendor/trees.h");
    println!("cargo:rerun-if-changed=vendor/gzguts.h");
}
