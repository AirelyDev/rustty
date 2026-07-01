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
//! entry point and shared `zutil` support are now vendored and compiled here.

use std::path::PathBuf;

/// Vendored zlib translation units compiled into this crate.
///
/// `adler32`/`crc32` provide the checksums; `inftrees`/`inffast`/`inflate`
/// provide the DEFLATE decompressor; `uncompr` provides the one-shot
/// `uncompress` entry point; `zutil` provides the shared support routines.
/// These are the verbatim upstream zlib 1.3.1 sources needed for the inflate
/// (decompression) surface the reference's `std.compress.flate` covers.
const SOURCES: &[&str] = &[
    "vendor/adler32.c",
    "vendor/crc32.c",
    "vendor/inftrees.c",
    "vendor/inffast.c",
    "vendor/inflate.c",
    "vendor/uncompr.c",
    "vendor/zutil.c",
];

fn main() {
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
}
