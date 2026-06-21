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
    header-gen    Generate include/rustty.h from rustty-vt-c via cbindgen (R2)
    help          Show this help
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
    let crate_dir = root.join("crates/rustty-vt-c");
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
        .current_dir(&crate_dir)
        .status()
        .map_err(|e| format!("spawn cbindgen (install via `cargo install cbindgen`): {e}"))?;

    if !status.success() {
        return Err(format!("cbindgen exited with {status}"));
    }
    Ok(output)
}
