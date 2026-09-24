//! build.rs — bake cargo's `OPT_LEVEL` into the crate.
//!
//! `std.valgrind`'s `builtin.valgrind_support` is true only in Zig's `Debug`
//! mode (parity S114-F1, review T-I F6). The Cargo-profile analogue of `Debug`
//! is opt-level 0 (`rustty-build-config`'s `mode_string_for`); the crate cannot
//! see the optimisation level on its own, so the build script passes it in.

fn main() {
    let opt_level = std::env::var("OPT_LEVEL").unwrap_or_else(|_| "0".to_string());
    println!("cargo:rustc-env=RUSTTY_ZIGSTD_OPT_LEVEL={opt_level}");
    println!("cargo:rerun-if-env-changed=OPT_LEVEL");
    println!("cargo:rerun-if-changed=build.rs");
}
