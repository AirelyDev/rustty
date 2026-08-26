//! Build script for oniguruma-sys (Oniguruma 6.9.9 regex engine).
//!
//! PORT STATUS: vendored (full library)
//!
//! R3 — self-contained, no ghostty dependency:
//!   1. The upstream C sources for Oniguruma 6.9.9 are vendored in-tree under
//!      `vendor/src/` (fetched from the exact URL in the reference's
//!      pkg/oniguruma/build.zig.zon — the deps.files.ghostty.org mirror of the
//!      `kkos/oniguruma` 6.9.9 release). This build NEVER references
//!      /Users/gael/projects/ghostty.
//!   2. `config.h` is GENERATED here from the reference's `addConfigHeader`
//!      substitutions over `src/config.h.cmake.in` (the one novel piece vs. the
//!      other -sys crates): `PACKAGE`/`PACKAGE_VERSION`/`VERSION`, the
//!      `HAVE_*`/`USE_CRNL_*` capability flags keyed off the target OS, and the
//!      `SIZEOF_INT`/`SIZEOF_LONG`/`SIZEOF_LONG_LONG`/`SIZEOF_VOIDP` sizes keyed
//!      off the target ABI. It is written to `OUT_DIR` (kept out of the pristine
//!      vendor tree) and placed first on the include path so the vendored
//!      `#include "config.h"` (regenc.h / regposerr.c) resolves to it.
//!   3. The vendored sources are compiled with the `cc` crate, mirroring the
//!      reference's pkg/oniguruma/build.zig `buildLib` exactly: the same 47
//!      `src/*.c` translation units and the same include dir (`vendor/src`).
//!      Vendored upstream warnings are silenced; on MSVC the reference's ubsan
//!      opt-outs are applied.
//!   4. `bindgen` auto-generates the FFI from `wrapper.h` — the same public
//!      header the reference's `c.zig` @cImports (`oniguruma.h`).
//!   5. `cargo:rustc-link-*` is emitted by `cc` (static archive `libonig`).
//!      The crate declares `links = "onig"`, so Cargo enforces a single
//!      linkage; the vendored `src` (header) dir is re-exported via
//!      `DEP_ONIG_INCLUDE` for downstream crates.

use std::env;
use std::fs;
use std::path::PathBuf;

/// Oniguruma translation units compiled into the static library.
///
/// This is the `files` list from the reference's pkg/oniguruma/build.zig
/// `buildLib` (`lib.addCSourceFiles`), plus `src/koi8.c` — see below.
///
/// DELIBERATE ADDITION: `src/koi8.c`. The reference compiles `koi8_r.c` but
/// not `koi8.c`, while its pkg/oniguruma/types.zig exposes BOTH
/// `Encoding.koi8` (-> `OnigEncodingKOI8`, defined only in koi8.c) and
/// `Encoding.koi8_r`. It gets away with that because `pub const koi8` is a
/// lazily-evaluated Zig declaration: nothing in ghostty uses it, so no object
/// file ever emits the reference and the archive is self-contained anyway.
///
/// rustty's `encoding!` macro compiles to a `pub fn`, which DOES emit the
/// `extern` reference whether or not anything calls it. That made
/// `librustty_vt_c.a` carry an `_OnigEncodingKOI8` that nothing in the archive
/// defines, so any static consumer that pulled in the containing object failed
/// to link — found by the pkg-config static-link check. `nm` over the archive
/// confirms KOI8 was the ONLY encoding in that state; every other
/// `OnigEncoding*` is defined by some object in the archive.
///
/// Compiling one more vendored translation unit is the fix that keeps the
/// public API surface faithful (the reference does expose `koi8`) AND makes it
/// usable. The alternative — deleting `Encoding::koi8` — would diverge from
/// the reference's API instead.
const SOURCES: &[&str] = &[
    "src/regerror.c",
    "src/regparse.c",
    "src/regext.c",
    "src/regcomp.c",
    "src/regexec.c",
    "src/reggnu.c",
    "src/regenc.c",
    "src/regsyntax.c",
    "src/regtrav.c",
    "src/regversion.c",
    "src/st.c",
    "src/onig_init.c",
    "src/unicode.c",
    "src/ascii.c",
    "src/utf8.c",
    "src/utf16_be.c",
    "src/utf16_le.c",
    "src/utf32_be.c",
    "src/utf32_le.c",
    "src/euc_jp.c",
    "src/sjis.c",
    "src/iso8859_1.c",
    "src/iso8859_2.c",
    "src/iso8859_3.c",
    "src/iso8859_4.c",
    "src/iso8859_5.c",
    "src/iso8859_6.c",
    "src/iso8859_7.c",
    "src/iso8859_8.c",
    "src/iso8859_9.c",
    "src/iso8859_10.c",
    "src/iso8859_11.c",
    "src/iso8859_13.c",
    "src/iso8859_14.c",
    "src/iso8859_15.c",
    "src/iso8859_16.c",
    "src/euc_tw.c",
    "src/euc_kr.c",
    "src/big5.c",
    "src/gb18030.c",
    "src/koi8.c", // NOT in the reference's list — see the SOURCES doc comment.
    "src/koi8_r.c",
    "src/cp1251.c",
    "src/euc_jp_prop.c",
    "src/sjis_prop.c",
    "src/unicode_unfold_key.c",
    "src/unicode_fold1_key.c",
    "src/unicode_fold2_key.c",
    "src/unicode_fold3_key.c",
];

/// Reproduce the reference's `addConfigHeader` over `src/config.h.cmake.in`.
///
/// The reference passes exactly these substitutions; here we emit the resulting
/// `config.h` directly (autoconf/cmake `#define NAME value` for enabled entries,
/// `/* #undef NAME */` for disabled ones). Values keyed off the target reproduce
/// Zig's `t.cTypeByteSize(...)` / `t.ptrBitWidth()` computations.
fn generate_config_h(target_os: &str, ptr_bytes: u64) -> String {
    let is_windows = target_os == "windows";

    // Zig `cTypeByteSize`: `int` is 4 and `long long` is 8 on every target this
    // crate builds for. `long` follows the data model: LLP64 on Windows (4),
    // LP64/ILP32 elsewhere (= pointer size).
    let sizeof_int: u64 = 4;
    let sizeof_long_long: u64 = 8;
    let sizeof_long: u64 = if is_windows { 4 } else { ptr_bytes };
    let sizeof_voidp: u64 = ptr_bytes;

    // Helper closures produce the two `#cmakedefine` outcomes.
    fn flag(name: &str, enabled: bool) -> String {
        if enabled {
            format!("#define {name} 1\n")
        } else {
            format!("/* #undef {name} */\n")
        }
    }

    let mut s = String::new();
    s.push_str("/* Generated by oniguruma-sys/build.rs — do not edit.\n");
    s.push_str("   Reproduces the reference pkg/oniguruma/build.zig addConfigHeader\n");
    s.push_str("   over src/config.h.cmake.in. */\n\n");

    // String / version substitutions.
    s.push_str("#define PACKAGE \"oniguruma\"\n");
    s.push_str("#define PACKAGE_VERSION \"6.9.9\"\n");
    s.push_str("#define VERSION \"6.9.9\"\n\n");

    // Capability flags (reference values).
    s.push_str(&flag("HAVE_ALLOCA", true));
    s.push_str(&flag("HAVE_ALLOCA_H", !is_windows));
    s.push_str(&flag("USE_CRNL_AS_LINE_TERMINATOR", is_windows));
    s.push_str(&flag("HAVE_STDINT_H", true));
    s.push_str(&flag("HAVE_SYS_TIMES_H", !is_windows));
    s.push_str(&flag("HAVE_SYS_TIME_H", !is_windows));
    s.push_str(&flag("HAVE_SYS_TYPES_H", true));
    s.push_str(&flag("HAVE_UNISTD_H", !is_windows));
    s.push_str(&flag("HAVE_INTTYPES_H", true));
    s.push('\n');

    // Type sizes.
    s.push_str(&format!("#define SIZEOF_INT {sizeof_int}\n"));
    s.push_str(&format!("#define SIZEOF_LONG {sizeof_long}\n"));
    s.push_str(&format!("#define SIZEOF_LONG_LONG {sizeof_long_long}\n"));
    s.push_str(&format!("#define SIZEOF_VOIDP {sizeof_voidp}\n"));

    s
}

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let vendor = manifest_dir.join("vendor");
    let onig_src = vendor.join("src");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    let ptr_bytes: u64 = env::var("CARGO_CFG_TARGET_POINTER_WIDTH")
        .ok()
        .and_then(|w| w.parse::<u64>().ok())
        .map(|bits| bits / 8)
        .unwrap_or(8);

    // Generate config.h into OUT_DIR (keeps the vendor tree pristine). The
    // vendored `#include "config.h"` (regenc.h) resolves to it because OUT_DIR
    // is placed first on the include path below.
    let config_h = generate_config_h(&target_os, ptr_bytes);
    fs::write(out_dir.join("config.h"), &config_h).expect("write config.h");

    let mut build = cc::Build::new();
    build
        // config.h first (generated), then the vendored headers.
        .include(&out_dir)
        .include(&onig_src)
        // Vendored upstream code: do not let its warnings fail the host build.
        .warnings(false)
        .flag_if_supported("-w")
        // Pin the C standard to gnu17 (pre-C23). GCC 16 defaults to gnu23, in
        // which an empty parameter list `()` now means `(void)` instead of the
        // historical "unspecified arguments". Oniguruma's st.h declares
        // `onig_st_foreach` with an ANYARGS (== empty, in C) callback prototype
        // `int (*)()` while st.c defines it with the concrete
        // `int (*)(st_data_t, st_data_t, st_data_t)` — compatible under the K&R
        // rule, but a hard "conflicting types" error under C23 (and the source
        // of the -Wincompatible-pointer-types errors at the call sites). The
        // reference's clang toolchain (Zig 0.15.2) defaults to gnu17, so gnu17
        // reproduces exactly how the vendored source was compiled upstream; the
        // emitted library is byte-for-byte identical (a front-end language-mode
        // choice only, no codegen difference for this C89-era source).
        .flag_if_supported("-std=gnu17");

    // The reference disables ubsan on MSVC (Zig's ubsan runtime cannot be
    // bundled on Windows, leaving __ubsan_handle_* unresolved).
    if target_env == "msvc" {
        build
            .flag_if_supported("-fno-sanitize=undefined")
            .flag_if_supported("-fno-sanitize-trap=undefined");
    }

    for src in SOURCES {
        build.file(vendor.join(src));
        println!("cargo:rerun-if-changed=vendor/{src}");
    }

    // Compiles the vendored sources into a static archive and emits the
    // `cargo:rustc-link-lib=static=onig` + search-path directives. Because the
    // crate declares `links = "onig"`, Cargo enforces a single linkage.
    build.compile("onig");

    // Auto-generate the FFI from the same public header the reference's `c.zig`
    // @cImports (see wrapper.h: `oniguruma.h`), resolved against the vendored
    // include dir.
    let bindings = bindgen::Builder::default()
        .header(manifest_dir.join("wrapper.h").to_string_lossy())
        .clang_arg(format!("-I{}", onig_src.display()))
        .clang_arg(format!("-I{}", out_dir.display()))
        // Emit the Oniguruma public surface (onig_* functions, Onig* types, the
        // ONIG_/ONIGERR_ constants, and the encoding/syntax externs).
        .allowlist_function("onig_.*")
        .allowlist_type("Onig.*")
        .allowlist_type("OnigEncodingType.*")
        .allowlist_var("ONIG_.*")
        .allowlist_var("ONIGERR_.*")
        .allowlist_var("Onig.*")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("unable to generate oniguruma-sys bindings");
    bindings
        .write_to_file(out_dir.join("bindings.rs"))
        .expect("unable to write oniguruma-sys bindings");

    // Re-export the vendored Oniguruma src (header) dir for downstream crates.
    // Cargo sets `DEP_ONIG_INCLUDE`.
    println!("cargo:include={}", onig_src.display());

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=wrapper.h");
}
