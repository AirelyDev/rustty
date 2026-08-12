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
//! terminal build knobs — so it is where the default is pinned. It used to
//! enable only `oniguruma` + `wuffs`, leaving `rustty-vt/simd` off and the app
//! permanently on the scalar fallback.

/// The resolved flag `stream.rs` branches on. This is the observable: with it
/// false, `Stream::next_slice` scalar-loops and returns
/// (`crates/rustty-vt/src/stream.rs:757`).
// The constant IS the subject: `options::SIMD` is a `const bool` resolved from
// the cargo feature graph, so this fires exactly when the app tier's `simd`
// default regresses.
#[allow(clippy::assertions_on_constants)]
#[test]
fn app_tier_resolves_terminal_simd_on() {
    assert!(
        rustty_vt::options::SIMD,
        "the app tier resolves `rustty_vt::options::SIMD` to false, so \
         `Stream::next_slice` takes the scalar fallback; ghostty's app build \
         defaults `-Dsimd` to true on every non-WASM target \
         (build/Config.zig:196-206)"
    );
}

/// Manifest arm: pin the knob in `rustty-core`'s own dependency declaration, so
/// the check binds no matter which feature-unified graph the test ran in. This
/// is the arm that was RED before the fix.
#[test]
fn app_tier_manifest_enables_terminal_simd() {
    let manifest = include_str!("../Cargo.toml");
    let line = manifest
        .lines()
        .find(|l| l.trim_start().starts_with("rustty-vt = "))
        .expect("rustty-core declares a `rustty-vt` dependency");
    assert!(
        line.contains("\"simd\""),
        "rustty-core's rustty-vt dependency does not enable `simd` ({line:?}); \
         the reference app build defaults `-Dsimd` to true. Keep the feature \
         list in sync with ghostty's SharedDeps terminalOptions."
    );
    // The two features the app tier already pinned stay pinned (anti-vacuity:
    // this test must not pass on a rewritten-away dependency line).
    assert!(
        line.contains("\"oniguruma\"") && line.contains("\"wuffs\""),
        "{line:?}"
    );
}
