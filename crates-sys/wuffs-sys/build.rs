//! Build script for wuffs-sys (Wuffs 0.4.0-alpha.9 image codecs).
//!
//! PORT STATUS: vendored (full single-file library, ghostty's module subset)
//!
//! R3 — self-contained, no ghostty dependency:
//!   1. The upstream Wuffs single-file C library is vendored in-tree at
//!      `vendor/release/c/wuffs-v0.4.c` (fetched from the exact URL in the
//!      reference's pkg/wuffs/build.zig.zon — the deps.files.ghostty.org mirror
//!      of the `google/wuffs` v0.4.0-alpha.9 release). This build NEVER
//!      references /Users/gael/projects/ghostty.
//!   2. It is compiled with the `cc` crate as ONE C translation unit, mirroring
//!      the reference's pkg/wuffs/build.zig exactly: `-DWUFFS_IMPLEMENTATION`
//!      (turns the stb-style header into its implementation) plus the precise
//!      `WUFFS_CONFIG__MODULE__*` define list from pkg/wuffs/src/c.zig — only
//!      the modules ghostty actually uses (MODULES gate + AUX__BASE/AUX__IMAGE +
//!      BASE + the ADLER32/CRC32/DEFLATE/JPEG/PNG/ZLIB codecs). The include dir
//!      is `release/c` (reference: `module.addIncludePath(release/c)`). The AUX
//!      modules are C++-only (`#if defined(__cplusplus)`-guarded); like the
//!      reference's `addCSourceFile` we compile as C, so those two defines are
//!      inert but kept for a faithful 1:1 mirror. Vendored upstream warnings are
//!      silenced.
//!   3. `bindgen` auto-generates the FFI from `wrapper.h`, which `#include`s the
//!      same `wuffs-v0.4.c` the reference's `c.zig` @cIncludes — WITHOUT
//!      `WUFFS_IMPLEMENTATION` (declarations only, matching the `@cImport`), but
//!      WITH the identical module defines so the declared surface matches the
//!      compiled object.
//!   4. `cargo:rustc-link-*` is emitted by `cc` (static archive `libwuffs`).
//!      The crate declares `links = "wuffs"`, so Cargo enforces a single
//!      linkage; the vendored `release/c` include dir is re-exported via
//!      `DEP_WUFFS_INCLUDE` for downstream crates.

use std::env;
use std::path::PathBuf;

/// The `WUFFS_CONFIG__MODULE__*` gate list — the verbatim `defines` array from
/// the reference's pkg/wuffs/src/c.zig. Enabling `WUFFS_CONFIG__MODULES` turns
/// on selective compilation; each `MODULE__*` then opts a single module in.
/// AUX__BASE/AUX__IMAGE are the C++ auxiliary helpers (inert under a C compile);
/// BASE is the shared runtime; the rest are the exact codecs ghostty decodes.
const MODULE_DEFINES: &[&str] = &[
    "WUFFS_CONFIG__MODULES",
    "WUFFS_CONFIG__MODULE__AUX__BASE",
    "WUFFS_CONFIG__MODULE__AUX__IMAGE",
    "WUFFS_CONFIG__MODULE__BASE",
    "WUFFS_CONFIG__MODULE__ADLER32",
    "WUFFS_CONFIG__MODULE__CRC32",
    "WUFFS_CONFIG__MODULE__DEFLATE",
    "WUFFS_CONFIG__MODULE__JPEG",
    "WUFFS_CONFIG__MODULE__PNG",
    "WUFFS_CONFIG__MODULE__ZLIB",
];

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let vendor = manifest_dir.join("vendor");
    // Reference layout preserved: the source and its include dir are the
    // vendored `release/c` subtree.
    let c_dir = vendor.join("release").join("c");
    let source = c_dir.join("wuffs-v0.4.c");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();

    let mut build = cc::Build::new();
    build
        .include(&c_dir)
        // Vendored upstream code: do not let its warnings fail the host build.
        .warnings(false)
        .flag_if_supported("-w")
        // Reference flag: `-DWUFFS_IMPLEMENTATION` (stb-style: emit the impl).
        .define("WUFFS_IMPLEMENTATION", None);

    // The exact `WUFFS_CONFIG__MODULE__*` gate list (reference: c.zig `defines`,
    // applied by build.zig as `-D<key>`). Bare `-D` (value None) matches the
    // reference `flags.append("-D" ++ key)`.
    for def in MODULE_DEFINES {
        build.define(def, None);
    }

    // The reference disables ubsan on MSVC (Zig's ubsan runtime cannot be
    // bundled on Windows, leaving __ubsan_handle_* unresolved):
    //   if (target.result.abi == .msvc) { -fno-sanitize=undefined; ... }
    if target_env == "msvc" {
        build
            .flag_if_supported("-fno-sanitize=undefined")
            .flag_if_supported("-fno-sanitize-trap=undefined");
    }

    build.file(&source);
    println!("cargo:rerun-if-changed=vendor/release/c/wuffs-v0.4.c");

    // Compiles the single vendored TU into a static archive and emits the
    // `cargo:rustc-link-lib=static=wuffs` + search-path directives. Because the
    // crate declares `links = "wuffs"`, Cargo enforces a single linkage.
    build.compile("wuffs");

    // The static-inline shim (shim.c): a SECOND C translation unit compiled
    // WITHOUT `WUFFS_IMPLEMENTATION` (so it emits NO duplicate copy of the codec
    // implementation — only its `rustty_wuffs_*` external-linkage wrappers). Each
    // wrapper forwards to a Wuffs base-runtime `static inline` helper (fully
    // defined inline in the header, independent of the impl), so the TU is
    // self-contained. This exposes the ~5 `static inline` helpers the PNG/JPEG
    // decode path needs but bindgen cannot bind (it never emits a linkable symbol
    // for a `static inline`). Compiled into its OWN static archive; the `links =
    // "wuffs"` uniqueness key is unaffected (it gates the -sys crate, not the
    // count of `rustc-link-lib` directives).
    let mut shim = cc::Build::new();
    shim.include(&c_dir).warnings(false).flag_if_supported("-w");
    // The SAME module gate list, WITHOUT `WUFFS_IMPLEMENTATION` (declarations +
    // static inlines only — mirrors wrapper.h / the `@cImport`).
    for def in MODULE_DEFINES {
        shim.define(def, None);
    }
    if target_env == "msvc" {
        shim.flag_if_supported("-fno-sanitize=undefined")
            .flag_if_supported("-fno-sanitize-trap=undefined");
    }
    shim.file(manifest_dir.join("shim.c"));
    println!("cargo:rerun-if-changed=shim.c");
    println!("cargo:rerun-if-changed=shim.h");
    shim.compile("wuffs_shim");

    // Auto-generate the FFI from `wrapper.h` (which `#include`s the same
    // `wuffs-v0.4.c` the reference's `c.zig` @cIncludes). The module defines are
    // passed here too — WITHOUT `WUFFS_IMPLEMENTATION`, mirroring the `@cImport`
    // (declarations only) — so the generated surface matches the compiled
    // object.
    let mut builder = bindgen::Builder::default()
        .header(manifest_dir.join("wrapper.h").to_string_lossy())
        .clang_arg(format!("-I{}", c_dir.display()));
    for def in MODULE_DEFINES {
        builder = builder.clang_arg(format!("-D{def}=1"));
    }

    let bindings = builder
        // Emit the Wuffs public surface: the `wuffs_*` functions/types, the
        // `sizeof__wuffs_*` struct-size helpers, and the `WUFFS_*` constants.
        .allowlist_function("wuffs_.*")
        .allowlist_function("sizeof__wuffs_.*")
        // The static-inline shim wrappers (shim.h prototypes, shim.c definitions)
        // that expose the base-runtime `static inline` helpers the decode path
        // drives. Effective ONLY because wrapper.h `#include`s shim.h (bindgen must
        // parse a prototype to emit its binding).
        .allowlist_function("rustty_wuffs_.*")
        .allowlist_type("wuffs_.*")
        .allowlist_var("WUFFS_.*")
        // Wuffs' generated header is enormous; block the layout-tests bindgen
        // would otherwise emit for every internal struct (they slow the build
        // and add nothing to the FFI surface).
        .layout_tests(false)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("unable to generate wuffs-sys bindings");
    bindings
        .write_to_file(out_dir.join("bindings.rs"))
        .expect("unable to write wuffs-sys bindings");

    // Re-export the vendored `release/c` include dir for downstream crates that
    // build against the Wuffs header (Cargo sets `DEP_WUFFS_INCLUDE`).
    println!("cargo:include={}", c_dir.display());

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=wrapper.h");
}
