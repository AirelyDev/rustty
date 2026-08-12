//! rustty-only (W16-TARGET-01): the published libghostty C-ABI layer's DEFAULT
//! feature set must resolve the terminal build knobs the way ghostty's default
//! app build does.
//!
//! `-Dsimd` defaults to TRUE for every non-WASM target
//! (`ghostty/src/build/Config.zig:196-206`) and `SharedDeps` threads `cfg.simd`
//! into the terminal module, so the reference libghostty runs
//! `terminal/stream.zig`'s SIMD `nextSlice` fast path. `rustty-apprt-c`'s `simd`
//! passthrough used to be opt-in, so a plain `cargo build -p rustty-apprt-c` —
//! and therefore `xtask xcframework`, which builds the shipped staticlib with
//! the DEFAULT feature set — produced a scalar-only library.

/// Manifest arm: `default` must include `simd`. Binds however this test run was
/// invoked, and is the arm that was RED before the fix (no `default` key
/// existed).
#[test]
fn default_feature_set_enables_simd() {
    let manifest = include_str!("../Cargo.toml");
    let features = manifest
        .split_once("\n[features]")
        .expect("rustty-apprt-c Cargo.toml has a [features] section")
        .1;
    let default_line = features
        .lines()
        .find(|l| l.trim_start().starts_with("default"))
        .expect(
            "rustty-apprt-c declares no `default` feature, so a plain \
             `cargo build -p rustty-apprt-c` (and `xtask xcframework`) leaves \
             `rustty-vt/simd` OFF; ghostty's `-Dsimd` defaults to true for every \
             non-WASM target (build/Config.zig:196-206)",
        );
    assert!(
        default_line.contains("\"simd\""),
        "rustty-apprt-c's default feature set does not include `simd` \
         ({default_line:?})"
    );
}

/// Behavior arm: under the default feature set the linked terminal really does
/// take the SIMD `next_slice` path. `--no-default-features` is rustty's
/// `-Dsimd=false` analog, so this arm stands down there.
// The constant IS the subject: `options::SIMD` is a `const bool` resolved from
// the cargo feature graph, so this fires exactly when the default feature set
// stops enabling `rustty-vt/simd`.
#[allow(clippy::assertions_on_constants)]
#[test]
fn default_feature_set_resolves_terminal_simd_on() {
    if !cfg!(feature = "default") {
        return;
    }
    assert!(
        rustty_vt::options::SIMD,
        "the DEFAULT rustty-apprt-c build resolves \
         `rustty_vt::options::SIMD` to false, so `Stream::next_slice` takes the \
         scalar fallback"
    );
}
