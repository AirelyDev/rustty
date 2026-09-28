//! rustty-only (W16-TARGET-01): the app tier's resolved `rustty-vt` build
//! options must match the reference app build.
//!
//! Ghostty's `-Dsimd` defaults to TRUE for every non-WASM target
//! (`ghostty/src/build/Config.zig:196-206` — "Build with SIMD-accelerated code
//! paths. Results in significant performance improvements."), and `SharedDeps`
//! threads `cfg.simd` into the terminal module the app links, so the reference
//! binary runs `terminal/stream.zig`'s SIMD `nextSlice` fast path
//! (`stream.zig:513-517` takes the byte-by-byte fallback only when
//! `debug or !build_options.simd`).
//!
//! rustty-core is the app tier — the `SharedDeps` analog that threads the
//! terminal build knobs — so it is where the default is set. It used to
//! enable only `oniguruma` + `wuffs`, leaving `rustty-vt/simd` off and the app
//! permanently on the scalar fallback.
//!
//! CORRECTION (N420): the fix for that pinned `simd` on the `rustty-vt` edge,
//! which made `-Dsimd=false` impossible for the app. The option is this
//! crate's default-on `simd` feature now, so both arms below check the OPTION:
//! the resolved flag follows the feature (in both positions), and the feature
//! is on by default and reaches `rustty-vt`.

/// The resolved flag `stream.rs` branches on. This is the observable: with it
/// false, `Stream::next_slice` scalar-loops and returns
/// (`crates/rustty-vt/src/stream.rs:757`). Meaningful in both matrix
/// positions: `cargo test -p rustty-core` (the default, SIMD on) and
/// `--no-default-features` (`-Dsimd=false`, SIMD off).
#[test]
fn app_tier_resolves_terminal_simd_from_the_option() {
    assert_eq!(
        rustty_vt::options::SIMD,
        cfg!(all(feature = "simd", not(target_arch = "wasm32"))),
        "`rustty_vt::options::SIMD` does not follow rustty-core's `simd` feature \
         (`-Dsimd`, build/Config.zig:238-249)"
    );
}

/// Manifest arm: the default must stay on (ghostty's app build defaults
/// `-Dsimd` to true on every non-WASM target), the feature must reach
/// `rustty-vt`, and the `rustty-vt` edge must not pin it (or it could not be
/// turned off). This binds no matter which feature-unified graph the test ran
/// in.
#[test]
fn app_tier_manifest_defaults_terminal_simd_on_as_an_option() {
    let manifest = include_str!("../Cargo.toml");
    let find = |prefix: &str| {
        manifest
            .lines()
            .find(|l| l.trim_start().starts_with(prefix))
            .unwrap_or_else(|| panic!("rustty-core's manifest has no `{prefix}` line"))
    };
    let default = find("default = ");
    assert!(
        default.contains("\"simd\""),
        "rustty-core's default features do not include `simd` ({default:?}); \
         the reference app build defaults `-Dsimd` to true"
    );
    let simd = find("simd = ");
    assert!(
        simd.contains("\"rustty-vt/simd\"") && simd.contains("\"rustty-simd/simd\""),
        "rustty-core's `simd` feature must reach the terminal and the SIMD kernels \
         ({simd:?})"
    );
    let line = find("rustty-vt = ");
    assert!(
        !line.contains("\"simd\""),
        "rustty-core's rustty-vt dependency pins `simd` ({line:?}), so \
         `-Dsimd=false` cannot turn it off (N420)"
    );
    // The two features the app tier pins stay pinned (anti-vacuity: this test
    // must not pass on a rewritten-away dependency line).
    assert!(
        line.contains("\"oniguruma\"") && line.contains("\"wuffs\""),
        "{line:?}"
    );
}
