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

    println!("cargo:rerun-if-changed=build.rs");
}
