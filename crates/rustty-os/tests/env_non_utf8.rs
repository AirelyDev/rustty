//! P3-W7 T2 — regression KAT for the non-UTF-8 environment panic (area E §3.1).
//!
//! `rustty_os::env::getEnvMap` used to be `std::env::vars().collect()` into a
//! `HashMap<String, String>`. `std::env::vars()` **panics** on any variable
//! that is not valid UTF-8, and POSIX environment variables are arbitrary byte
//! strings — so a single non-UTF-8 variable in the user's shell made every new
//! surface panic (`getEnvMap` is on the surface-creation path:
//! `rustty-core/src/surface.rs`, `rustty-apprt/src/embedded.rs`,
//! `rustty-apprt/src/gtk/class/surface.rs`).
//!
//! VERIFIED before the fix, not inferred:
//!
//! ```text
//! thread 'get_env_map_survives_non_utf8' panicked at library/std/src/env.rs:162:83:
//! called `Result::unwrap()` on an `Err` value: "\xFF"
//! ```
//!
//! A test cannot set a non-UTF-8 variable in its own process safely (Rust 2024
//! makes `set_var` `unsafe`, and it races every concurrent `getenv` — the
//! `parallel-cargo-test-exposes-c-init-races` lesson), so the guard re-runs
//! **this test binary** as a child with the bad variable in its environment.
//! The child arm is selected by ARGV (`--ignored --exact <name>`), not by an
//! env var, matching the recorded lesson that env-switched controls go vacuous
//! when the environment is not forwarded.

/// The child arm. `#[ignore]`d so it only ever runs when the parent selects it
/// by name on the command line.
#[test]
#[ignore = "child arm of get_env_map_survives_non_utf8; selected by argv"]
fn w7_child_reads_env_map() {
    let m = rustty_os::env::getEnvMap();
    // Must contain the non-UTF-8 variable, byte-exact.
    let v = m
        .get(&b"RUSTTY_W7_BAD"[..])
        .expect("the non-UTF-8 variable must be present in the map");
    assert_eq!(v.as_slice(), &[0xfe_u8, 0xff], "value must be byte-exact");
}

#[test]
#[cfg(unix)]
fn get_env_map_survives_non_utf8() {
    let exe = std::env::current_exe().expect("current_exe");

    // A value that is not valid UTF-8.
    // SAFETY: on POSIX an `OsStr` is unconstrained bytes, so any byte sequence
    // is a valid encoded form. This test is `cfg(unix)`-only for that reason.
    let bad: std::ffi::OsString =
        unsafe { std::ffi::OsString::from_encoded_bytes_unchecked(vec![0xfe, 0xff]) };

    let out = std::process::Command::new(exe)
        .args(["--ignored", "--exact", "w7_child_reads_env_map"])
        .env("RUSTTY_W7_BAD", &bad)
        .output()
        .expect("spawn child");

    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();

    assert!(
        out.status.success(),
        "child FAILED (this is the pre-fix panic):\nstdout: {stdout}\nstderr: {stderr}",
    );
    // NON-VACUITY: a filter that matched nothing would also exit 0. Require
    // that the child arm actually ran.
    assert!(
        stdout.contains("1 passed"),
        "child did not actually run the arm (vacuous control):\nstdout: {stdout}\nstderr: {stderr}",
    );
}
