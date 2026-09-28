//! Build script for rustty-vt-c.
//!
//! It sets the shared library's versioned identity (`dylib_identity`, S64-F5;
//! CORRECTION 2026-09-23: this header used to say "Its ONLY job" was the
//! cfgs below). Its other job is to reflect the Cargo profile's optimization level into cfgs
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

    dylib_identity();
}

#[path = "src/dylib_link_args.rs"]
mod dylib_link_args;

/// The shared library's extra link arguments: its versioned identity
/// (S64-F5) and Android's 16 KiB page size (S64-F9). See
/// `src/dylib_link_args.rs`.
fn dylib_identity() {
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    // `-Dlib-version-string` (N424): the reference's dylib version is
    // `lib_version`'s MAJOR.MINOR.PATCH (`GhosttyLibVt.zig:396-400`), so an
    // explicit RUSTTY_LIB_VERSION_STRING versions it too. (rustty-vt refuses a
    // malformed value at compile time; this only takes its numeric core.)
    println!("cargo::rerun-if-env-changed=RUSTTY_LIB_VERSION_STRING");
    let version = match std::env::var("RUSTTY_LIB_VERSION_STRING") {
        Ok(v) if !v.is_empty() => v.split(['-', '+']).next().unwrap_or_default().to_string(),
        _ => format!(
            "{}.{}.{}",
            std::env::var("CARGO_PKG_VERSION_MAJOR").unwrap_or_default(),
            std::env::var("CARGO_PKG_VERSION_MINOR").unwrap_or_default(),
            std::env::var("CARGO_PKG_VERSION_PATCH").unwrap_or_default(),
        ),
    };
    println!("cargo::rerun-if-changed=src/dylib_link_args.rs");
    println!("cargo::rerun-if-env-changed=RUSTTY_VT_SONAME");
    let soname = std::env::var("RUSTTY_VT_SONAME").ok();
    for arg in dylib_link_args::dylib_link_args(&os, &version, soname.as_deref()) {
        println!("cargo::rustc-cdylib-link-arg={arg}");
    }
}
