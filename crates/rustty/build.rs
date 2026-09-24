//! Build script for the `rustty` executable: the target-specific steps of
//! `src/build/GhosttyExe.zig`. The decisions are in `src/build_checks.rs`.

#[path = "src/build_checks.rs"]
mod build_checks;

use std::path::PathBuf;

fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    // Check for possible issues
    check_nix_shell(&target_os);

    // OS-specific
    if target_os == "windows" {
        windows_resources();
    }
}

/// `checkNixShell` (S63-F6).
fn check_nix_shell(target_os: &str) {
    println!("cargo::rerun-if-env-changed=IN_NIX_SHELL");
    // The reference's `isNativeCpu() and isNativeOs()`: cargo builds for the
    // host when TARGET equals HOST.
    let native = std::env::var("TARGET").ok() == std::env::var("HOST").ok();
    let etc_nixos = std::path::Path::new("/etc/NIXOS").exists();
    let in_nix_shell = std::env::var_os("IN_NIX_SHELL").is_some();
    if let Some(msg) = build_checks::nix_shell_error(target_os, native, etc_nixos, in_nix_shell) {
        eprintln!("{msg}");
        panic!("building on and for NixOS outside of the Nix shell environment");
    }
}

/// `exe.root_module.addWin32ResourceFile(.{ .file = b.path("dist/windows/ghostty.rc") })`
/// (S63-F5). The GUI subsystem half is the `windows_subsystem` attribute in
/// `src/main.rs`.
fn windows_resources() {
    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let dist = manifest_dir.join("../../dist/windows");
    for f in ["rustty.rc", "rustty.manifest", "rustty.ico"] {
        println!("cargo::rerun-if-changed={}", dist.join(f).display());
    }
    println!("cargo::rerun-if-env-changed=WINDRES");
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let windres = std::env::var("WINDRES").ok();
    let (mut cmd, obj) = build_checks::windows_resource_command(
        &target_env,
        &target_arch,
        windres.as_deref(),
        &dist.join("rustty.rc"),
        &out_dir,
    );
    let status = cmd
        .status()
        .unwrap_or_else(|e| panic!("spawn the resource compiler {cmd:?}: {e}"));
    assert!(status.success(), "{cmd:?} exited with {status}");
    println!("cargo::rustc-link-arg-bins={}", obj.display());
}
