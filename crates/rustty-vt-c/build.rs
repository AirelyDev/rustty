//! Build script for rustty-vt-c.
//!
//! Its ONLY job is to reflect the Cargo profile's optimization level into cfgs
//! so `build_info::optimize_mode()` can answer the C-ABI `RUSTTY_BUILD_INFO_
//! OPTIMIZE` query with the faithful four-way analog of the reference's
//! `builtin.mode` (`src/terminal/c/build_info.zig:72-77`).
//!
//! Zig's four optimize modes are the cross product of two independent axes —
//! "is it optimized / how" and "is runtime safety on" — and Rust exposes both
//! at build time, just not from a single reflection value:
//!
//!   * safety   → `cfg!(debug_assertions)`, readable in the crate itself.
//!   * optimize → `OPT_LEVEL`, readable ONLY here in the build script; Cargo
//!     sets it to the `opt-level` of the profile the PACKAGE is built with.
//!
//! So this script exports the optimize axis as two cfgs and the mapping itself
//! lives in `build_info.rs`:
//!
//!   `rustty_optimized` — `OPT_LEVEL != "0"` (any optimizing profile).
//!   `rustty_opt_small` — `OPT_LEVEL` is `"s"` or `"z"` (optimize for size).

fn main() {
    // Declare the cfgs we may set so `--check-cfg` (CI's `-D warnings`) does
    // not warn about unexpected cfg names.
    println!("cargo::rustc-check-cfg=cfg(rustty_optimized)");
    println!("cargo::rustc-check-cfg=cfg(rustty_opt_small)");

    // Nothing in the source tree affects this script's output; only the
    // profile does (Cargo re-runs it when OPT_LEVEL changes).
    println!("cargo::rerun-if-changed=build.rs");

    // Cargo always sets OPT_LEVEL; treat an (impossible) absence as "0" so the
    // fallback is the safety-checked Debug analog rather than a panic.
    let opt_level = std::env::var("OPT_LEVEL").unwrap_or_else(|_| "0".to_string());

    if opt_level != "0" {
        println!("cargo::rustc-cfg=rustty_optimized");
    }
    if opt_level == "s" || opt_level == "z" {
        println!("cargo::rustc-cfg=rustty_opt_small");
    }
}
