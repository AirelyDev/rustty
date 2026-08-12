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
/// `adler32`/`crc32` provide the checksums; `inftrees`/`inffast`/`inflate`
/// provide the DEFLATE decompressor; `uncompr` provides the one-shot
/// `uncompress` entry point; `deflate`/`trees` provide the DEFLATE compressor;
/// `compress` provides the one-shot `compress`/`compress2` entry point; `zutil`
/// provides the shared support routines. These are the verbatim upstream zlib
/// 1.3.1 sources — the full inflate AND deflate surface, so the emitted archive
/// is self-contained (P3: Sentry's crash reporter links `deflate`).
const SOURCES: &[&str] = &[
    "vendor/adler32.c",
    "vendor/crc32.c",
    "vendor/inftrees.c",
    "vendor/inffast.c",
    "vendor/inflate.c",
    "vendor/uncompr.c",
    "vendor/deflate.c",
    "vendor/trees.c",
    "vendor/compress.c",
    "vendor/zutil.c",
    // The DEFLATE (compression) translation units, needed by the framegen asset
    // step (`cli +boo`): `deflate` is the compressor core, `trees` builds the
    // Huffman trees it emits, and `compress` provides the `compressBound` size
    // helper framegen's `main.c` uses. These are the verbatim upstream zlib
    // 1.3.1 sources (byte-identical to the reference's vendored zlib).
    "vendor/deflate.c",
    "vendor/trees.c",
    "vendor/compress.c",
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
        .include(&vendor)
        // Vendored upstream code: do not let its warnings fail the host build.
        .warnings(false)
        .flag_if_supported("-w");
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
}
