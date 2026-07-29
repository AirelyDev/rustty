//! Build script for gettext-sys (GNU gettext gnulib `localename` family).
//!
//! PORT STATUS: complete (full macOS libintl — catalog API + localename)
//!
//! R3 — self-contained, no ghostty dependency:
//!   1. The upstream C sources for `intl` are vendored in-tree under
//!      `vendor/intl/` (copied verbatim from gettext 0.24; this build NEVER
//!      references /Users/gael/projects/ghostty). The layout mirrors upstream:
//!      `vendor/intl/*.c` is the gettext-runtime `intl/` core and
//!      `vendor/intl/gnulib-lib/*.c` the gnulib support units. The build
//!      template (config.h, libgnuintl.h, libintl.h) stays at `vendor/` and is
//!      copied from ghostty/pkg/libintl, which is the reference's macOS libintl
//!      build.
//!   2. They are compiled with the `cc` crate using the reference's portable
//!      flags (`-DHAVE_CONFIG_H -DLOCALEDIR=""`; see pkg/libintl/build.zig), and
//!      link Apple CoreFoundation (the macOS `gl_locale_name_canonicalize`
//!      consults CFPreferences for legacy locale names).
//!   3. `bindgen` auto-generates the thin FFI for the canonicalize symbol from
//!      `vendor/wrapper.h`.
//!   4. `cargo:rustc-link-*` is emitted by `cc` (static archive `libintl`).
//!
//! macOS-only: the reference builds libintl from gettext only on macOS (Linux
//! gets libintl from libc). On non-macOS this build is a no-op so the crate
//! still compiles (the FFI is gated `#[cfg(target_os = "macos")]` in lib.rs).

use std::env;
use std::path::PathBuf;

/// Vendored gettext translation units compiled into this crate.
///
/// This is the FULL macOS libintl the reference builds — the exact `srcs` list
/// of `ghostty/pkg/libintl/build.zig`, in the same order: the 23
/// `gettext-runtime/intl/` core units (message catalogs, domain bindings, the
/// plural-form parser) plus the 7 `gnulib-lib/` units macOS needs. The
/// reference's commented-out `osdep.c`/`printf.c` are deliberately NOT here —
/// they are for platforms the reference does not build libintl for.
///
/// `plural.c` is the checked-in Bison-generated parser that ships pre-generated
/// in the upstream tarball; no yacc/bison runs at build time.
const SOURCES: &[&str] = &[
    "vendor/intl/bindtextdom.c",
    "vendor/intl/dcgettext.c",
    "vendor/intl/dcigettext.c",
    "vendor/intl/dcngettext.c",
    "vendor/intl/dgettext.c",
    "vendor/intl/dngettext.c",
    "vendor/intl/explodename.c",
    "vendor/intl/finddomain.c",
    "vendor/intl/gettext.c",
    "vendor/intl/hash-string.c",
    "vendor/intl/intl-compat.c",
    "vendor/intl/l10nflist.c",
    "vendor/intl/langprefs.c",
    "vendor/intl/loadmsgcat.c",
    "vendor/intl/localealias.c",
    "vendor/intl/log.c",
    "vendor/intl/ngettext.c",
    "vendor/intl/plural-exp.c",
    "vendor/intl/plural.c",
    "vendor/intl/setlocale.c",
    "vendor/intl/textdomain.c",
    "vendor/intl/version.c",
    "vendor/intl/compat.c",
    "vendor/intl/gnulib-lib/getlocalename_l-unsafe.c",
    "vendor/intl/gnulib-lib/localename.c",
    "vendor/intl/gnulib-lib/localename-environ.c",
    "vendor/intl/gnulib-lib/localename-unsafe.c",
    "vendor/intl/gnulib-lib/setlocale-lock.c",
    "vendor/intl/gnulib-lib/setlocale_null.c",
    "vendor/intl/gnulib-lib/setlocale_null-unlocked.c",
];

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let vendor = manifest_dir.join("vendor");

    // macOS-only: the reference builds libintl from gettext only on macOS
    // (Linux provides libintl in libc). On other targets do nothing — the FFI
    // is cfg'd out in lib.rs and there is no native lib to link.
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "macos" {
        println!("cargo:rerun-if-changed=build.rs");
        return;
    }

    let mut build = cc::Build::new();
    build
        // Include-path layering mirrors the reference exactly
        // (pkg/libintl/build.zig): first the checked-in build template
        // (config.h, libintl.h, libgnuintl.h), then the upstream `intl/`
        // sources, then `intl/gnulib-lib/`.
        .include(&vendor)
        .include(vendor.join("intl"))
        .include(vendor.join("intl/gnulib-lib"))
        // Match the reference's flags (pkg/libintl/build.zig): config.h-driven
        // build with an empty LOCALEDIR. No other -D is passed there and none
        // is needed here — the full 30-TU set compiles clean with just these.
        .define("HAVE_CONFIG_H", None)
        .define("LOCALEDIR", Some("\"\""))
        // Vendored upstream code: do not let its warnings fail the host build.
        .warnings(false)
        .flag_if_supported("-w");
    for src in SOURCES {
        build.file(manifest_dir.join(src));
        println!("cargo:rerun-if-changed={src}");
    }
    // Compiles the vendored sources into a static archive and emits the
    // `cargo:rustc-link-lib=static=...` + search-path directives. Because the
    // crate declares `links = "intl"`, Cargo enforces a single linkage.
    //
    // The archive is named `rustty_intl`, NOT `intl`. That is deliberate: the
    // `links` key stays "intl" (the pkg-config-style uniqueness tag for the
    // underlying C library concept), while the ARCHIVE name is namespaced so it
    // can never be confused with — or silently shadowed by — a system/Homebrew
    // `libintl`. As of WS9 nothing in the workspace passes `-lintl` any more:
    // this archive is the full libintl (catalog API included), so
    // `rustty-os/build.rs`'s Homebrew link-search and `macos/build-rustty.sh`'s
    // `OTHER_LDFLAGS -lintl` were both removed.
    build.compile("rustty_intl");

    // The macOS `gl_locale_name_canonicalize` consults CFPreferences for legacy
    // (NeXTstep-inherited) locale names, so the static archive references Apple
    // CoreFoundation; declare the framework so downstream binaries link it.
    println!("cargo:rustc-link-lib=framework=CoreFoundation");

    // Auto-generate the thin FFI for the canonicalize symbol from wrapper.h.
    let bindings = bindgen::Builder::default()
        .header(vendor.join("wrapper.h").to_string_lossy())
        // Only emit the one symbol this crate exposes; nothing else.
        .allowlist_function("_libintl_locale_name_canonicalize")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("unable to generate gettext-sys bindings");
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("unable to write gettext-sys bindings");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=vendor/wrapper.h");
    println!("cargo:rerun-if-changed=vendor/config.h");
    println!("cargo:rerun-if-changed=vendor/libintl.h");
    println!("cargo:rerun-if-changed=vendor/libgnuintl.h");
    println!("cargo:rerun-if-changed=vendor/intl");
}
