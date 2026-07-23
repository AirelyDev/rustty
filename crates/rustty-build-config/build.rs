//! build.rs — capture the Rust toolchain version at compile time.
//!
//! Reference analog: Zig exposes `builtin.zig_version_string` to every module;
//! `cli/version.zig` prints it as the toolchain line. Rust has no builtin, so
//! this build script asks the ACTUAL compiler cargo is driving (`$RUSTC`,
//! falling back to `rustc` on PATH) and bakes the answer into the crate env.

fn main() {
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    let out = std::process::Command::new(&rustc)
        .arg("--version")
        .output()
        .expect("failed to run `rustc --version`");
    let full = String::from_utf8(out.stdout).expect("rustc --version is UTF-8");
    // "rustc 1.93.1 (hash date)" -> "1.93.1 (hash date)" (a bare version
    // string, mirroring `builtin.zig_version_string`).
    let trimmed = full.trim();
    let version = trimmed.strip_prefix("rustc ").unwrap_or(trimmed);
    println!("cargo:rustc-env=RUSTTY_RUSTC_VERSION={version}");
    println!("cargo:rerun-if-env-changed=RUSTC");
}
