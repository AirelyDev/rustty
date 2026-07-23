//! Build script for rustty-os.
//!
//! PORT STATUS: stub (module skeleton — see src/lib.rs)
//!
//! R3 — self-contained, no ghostty dependency: the reference's `src/pty.c` is
//! vendored in-tree under `cbits/` (copied byte-for-byte; this build NEVER
//! references /Users/gael/projects/ghostty) and compiled standalone with the
//! `cc` crate. In the reference, `pty.c` is `@cImport`ed as the `pty-c` module
//! to expose the platform `TIOC*`/`TIOCPTYGNAME` ioctl constants to `pty.zig`
//! (see ghostty `src/build/SharedDeps.zig` + `src/pty.zig`'s `@import("pty-c")`).
//!
//! `pty.c` is includes + `#define`s only (no functions); compiling it proves
//! the C toolchain links here and stages the unit for `pty.zig`'s port (where
//! the constants are surfaced to Rust). The `#else #error "unsupported platform"`
//! guard means it only compiles on FreeBSD/Linux/macOS — matching the reference.

use std::env;
use std::path::PathBuf;

fn main() {
    // `pty.c` guards `#else -> #error "unsupported platform"`; compile it only
    // on the platforms the reference supports (FreeBSD/Linux/macOS).
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let supported = matches!(target_os.as_str(), "macos" | "linux" | "freebsd");

    if supported {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        cc::Build::new()
            .file(manifest_dir.join("cbits/pty.c"))
            // Vendored upstream-style code: do not let warnings fail the build.
            .warnings(false)
            .flag_if_supported("-w")
            .compile("rustty_pty");
        println!("cargo:rerun-if-changed=cbits/pty.c");
    }

    // macOS unified-logging shim (see cbits/log_shim.c) — the ghostty
    // `zig_os_log_with_type` analogue for pkg/macos/os/log.zig's Log.log.
    if target_os == "macos" {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        cc::Build::new()
            .file(manifest_dir.join("cbits/log_shim.c"))
            .warnings(false)
            .flag_if_supported("-w")
            .compile("rustty_os_log_shim");
        println!("cargo:rerun-if-changed=cbits/log_shim.c");
    }

    // WS4: `crate::i18n` declares `extern "C" { fn bindtextdomain/textdomain/
    // dgettext(...) }` (the gettext message-catalog runtime). The vendored
    // `gettext-sys` crate provides ONLY the gnulib `localename` canonicalizer
    // (`_libintl_locale_name_canonicalize`) — the catalog API is a documented,
    // deferred vendor gap (DEFERRALS.md "gettext-sys — self-contained libintl
    // deferred"). Until that full vendor lands, resolve those three symbols
    // from Homebrew's `libintl`, mirroring the EXACT flags the Swift app link
    // already uses (`macos/build-rustty.sh` `OTHER_LDFLAGS`: `-L/opt/homebrew/
    // opt/gettext/lib -lintl`) so `cargo build`/`cargo test` link the same way
    // the shipped app does — this is a link-search addition only, not a new
    // dependency shape (R3: still binding the SAME system gettext library).
    if target_os == "macos" {
        // NOTE: `gettext-sys`'s OWN vendored archive is named
        // `rustty_gettext_localename` (not `intl`, see its build.rs) so this
        // `-lintl` unambiguously resolves to Homebrew's full libintl and never
        // silently shadows onto the vendored localename-only slice.
        println!("cargo:rustc-link-search=native=/opt/homebrew/opt/gettext/lib");
        println!("cargo:rustc-link-lib=dylib=intl");
    }

    println!("cargo:rerun-if-changed=build.rs");
}
