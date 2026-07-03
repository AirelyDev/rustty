//! xtask — build orchestration for rustty.
//!
//! PORTS: ghostty src/build/ (34 files) + helpgen.zig — build orchestration:
//! cbindgen headers, terminfo compilation, shell-integration/theme bundling,
//! man pages, i18n, dist tarball, macOS xcframework/.app.
//!
//! PORT STATUS: scaffold (only the `header-gen` subcommand is wired so far).
//!
//! Independent of the parity port order (R4). NO ghostty Cargo dep (R3).

use std::path::PathBuf;
use std::process::{Command, ExitCode};

const USAGE: &str = "\
xtask — rustty build orchestration

USAGE:
    cargo run -p xtask -- <SUBCOMMAND>

SUBCOMMANDS:
    header-gen         Generate include/rustty.h from rustty-vt-c via cbindgen (R2)
    header-gen-apprt   Generate include/rustty-apprt.h from rustty-apprt-c via cbindgen (R2)
    help               Show this help
";

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("header-gen") => match header_gen() {
            Ok(out) => {
                println!("wrote {}", out.display());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("xtask: header-gen failed: {e}");
                ExitCode::FAILURE
            }
        },
        Some("header-gen-apprt") => match header_gen_apprt() {
            Ok(out) => {
                println!("wrote {}", out.display());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("xtask: header-gen-apprt failed: {e}");
                ExitCode::FAILURE
            }
        },
        Some("help") | Some("--help") | Some("-h") | None => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("xtask: unknown subcommand `{other}`\n");
            print!("{USAGE}");
            ExitCode::FAILURE
        }
    }
}

/// Workspace root = the parent of this crate's directory (`xtask/..`).
fn workspace_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .expect("xtask has a parent dir")
        .to_path_buf()
}

/// Generate `include/rustty.h` from the `rustty-vt-c` crate via the cbindgen
/// CLI (R2: `rustty_*` symbols, `include/rustty.h` + `rustty/vt/*`).
///
/// Per R2 the reference `ghostty.h` ABI-equivalence gate is DROPPED; the gate
/// here is header self-consistency (cbindgen succeeds + the header compiles as
/// C) plus behavioral parity via the `identity-ghostty` harness.
///
/// This shells out to the installed `cbindgen` CLI rather than linking the
/// cbindgen crate, keeping xtask light. Equivalent manual command:
///
/// ```sh
/// cbindgen \
///   --config crates/rustty-vt-c/cbindgen.toml \
///   --crate rustty-vt-c \
///   --output include/rustty.h
/// ```
fn header_gen() -> Result<PathBuf, String> {
    let root = workspace_root();
    let config = root.join("crates/rustty-vt-c/cbindgen.toml");
    let include_dir = root.join("include");
    let output = include_dir.join("rustty.h");

    std::fs::create_dir_all(&include_dir).map_err(|e| format!("create include/: {e}"))?;

    let status = Command::new("cbindgen")
        .arg("--config")
        .arg(&config)
        .arg("--crate")
        .arg("rustty-vt-c")
        .arg("--output")
        .arg(&output)
        // Run from the WORKSPACE ROOT (not the crate dir). With `parse_deps =
        // true` cbindgen must resolve the sibling workspace crates that define
        // the cross-crate C-ABI types (rustty-vt / rustty-lib /
        // rustty-render-types); resolving those is reliable only from the root
        // (matches the documented manual `cbindgen --config ... --crate
        // rustty-vt-c --output include/rustty.h` invocation run at the root).
        .current_dir(&root)
        .status()
        .map_err(|e| format!("spawn cbindgen (install via `cargo install cbindgen`): {e}"))?;

    if !status.success() {
        return Err(format!("cbindgen exited with {status}"));
    }
    Ok(output)
}

/// Generate `include/rustty-apprt.h` from the `rustty-apprt-c` crate via the
/// cbindgen CLI (R2: the libghostty embedding C ABI, `rustty_*` symbols, chunk
/// E11 of EMBEDDED_BLUEPRINT §3/§6).
///
/// Like [`header_gen`] the reference `ghostty.h` ABI-equivalence gate is DROPPED
/// (R2); the gate is header self-consistency (cbindgen succeeds + the header
/// compiles standalone as **C and C++**, `cpp_compat = true`) PLUS the C
/// compile+link smoke KAT (`crates/rustty-apprt-c/tests/smoke.rs` drives
/// `smoke.c` against the staticlib). cbindgen's `Can't find` warnings for the
/// cross-crate types are EXPECTED — `parse_deps = false`, so those layouts are
/// hand-pinned in the crate's `cbindgen.toml` `after_includes` (rustty-vt-c
/// precedent); cbindgen references them by name only.
///
/// Equivalent manual command (run from the workspace root):
///
/// ```sh
/// cbindgen \
///   --config crates/rustty-apprt-c/cbindgen.toml \
///   --crate rustty-apprt-c \
///   --output include/rustty-apprt.h
/// ```
fn header_gen_apprt() -> Result<PathBuf, String> {
    let root = workspace_root();
    let config = root.join("crates/rustty-apprt-c/cbindgen.toml");
    let include_dir = root.join("include");
    let output = include_dir.join("rustty-apprt.h");

    std::fs::create_dir_all(&include_dir).map_err(|e| format!("create include/: {e}"))?;

    let status = Command::new("cbindgen")
        .arg("--config")
        .arg(&config)
        .arg("--crate")
        .arg("rustty-apprt-c")
        .arg("--output")
        .arg(&output)
        // Run from the WORKSPACE ROOT (matches the documented manual invocation).
        .current_dir(&root)
        .status()
        .map_err(|e| format!("spawn cbindgen (install via `cargo install cbindgen`): {e}"))?;

    if !status.success() {
        return Err(format!("cbindgen exited with {status}"));
    }
    Ok(output)
}
