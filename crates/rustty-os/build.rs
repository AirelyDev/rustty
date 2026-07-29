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

    // i18n / gettext: NOTHING to emit here.
    //
    // `crate::i18n` declares `extern "C" { fn bindtextdomain/textdomain/
    // dgettext(...) }` (the gettext message-catalog runtime). Those symbols
    // come from the `gettext-sys` dependency, which as of WS9 vendors the FULL
    // macOS libintl (the reference's whole `pkg/libintl/build.zig` `srcs` set)
    // and emits `cargo:rustc-link-lib=static=rustty_intl`.
    //
    // WS4 temporarily resolved them from Homebrew with
    // `-L/opt/homebrew/opt/gettext/lib -lintl`. That was a hard host
    // dependency on ARM Homebrew for EVERY macOS build (and silently wrong on
    // Intel Homebrew, which lives under /usr/local) — review finding WS4-M1,
    // RESOLVED by WS9. Do not reintroduce it: `.port/tools/gates/
    // no_homebrew_gettext_gate.sh` fails the build if anything does.

    println!("cargo:rerun-if-changed=build.rs");
}
