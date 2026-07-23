//! Build script for gettext-sys (GNU gettext gnulib `localename` family).
//!
//! PORT STATUS: vendor slice (macOS `_libintl_locale_name_canonicalize`)
//!
//! R3 — self-contained, no ghostty dependency:
//!   1. The upstream C sources for `intl` are vendored in-tree under `vendor/`
//!      (copied; this build NEVER references /Users/gael/projects/ghostty). The
//!      gnulib-lib `.c`/`.h` come from the gettext upstream
//!      (gettext-runtime/intl/gnulib-lib); the build template (config.h,
//!      libgnuintl.h, libintl.h) is copied from ghostty/pkg/libintl, which is
//!      the reference's macOS libintl build.
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

/// Vendored gnulib `localename` translation units compiled into this crate.
///
/// This is the macOS subset from `ghostty/pkg/libintl/build.zig`'s `srcs` that
/// is required to define and link `gl_locale_name_canonicalize`
/// (`_libintl_locale_name_canonicalize`). The threaded `localename.c` is not
/// included: it pulls in gnulib's `glthread/` locking layer and is unnecessary
/// for the canonicalize symbol (defined in the `-unsafe` unit).
const SOURCES: &[&str] = &[
    "vendor/localename-unsafe.c",
    "vendor/getlocalename_l-unsafe.c",
    "vendor/localename-environ.c",
    "vendor/setlocale_null.c",
    "vendor/setlocale_null-unlocked.c",
    "vendor/setlocale-lock.c",
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
        .include(&vendor)
        // Match the reference's flags (pkg/libintl/build.zig): config.h-driven
        // build with an empty LOCALEDIR.
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
    // WS4 NOTE: the archive is named `rustty_gettext_localename`, NOT `intl` —
    // this is a DELIBERATE divergence from the `links` key (which stays
    // "intl" as the pkg-config-style uniqueness tag for the underlying C
    // library concept). A `-lintl` name-based link (added by rustty-os/build.rs
    // WS4 to reach Homebrew's FULL libintl for `bindtextdomain`/`dgettext`) would
    // otherwise resolve to THIS crate's own vendored archive first (whichever
    // `-L` search path lands earlier), which provides only the localename
    // canonicalizer slice — silently shadowing the real symbols and producing
    // a link-time "undefined symbol: _bindtextdomain" instead. A unique
    // archive name sidesteps the collision entirely.
    build.compile("rustty_gettext_localename");

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
}
