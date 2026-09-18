//! rustty-crash::build_options
//!
//! PORTS: the `sentry` entry of the reference's generated `build_options` blob
//! (`const build_options = @import("build_options");` in src/crash/sentry.zig).
//!
//! PORT STATUS: done
//!
//! In the reference this value is a `zig build` option resolved in
//! `src/build/Config.zig` (`config.sentry = b.option(bool, "sentry", ...)`)
//! and injected via `step.addOption(bool, "sentry", self.sentry)`. The Rust
//! port has no separate build-options blob, so the option is reproduced here as
//! a `const` read off the crate's `sentry` cargo feature.
//!
//! PORT NOTE (the const's inputs MOVED — a parity-contract change, W16-01):
//! this const used to be `cfg!(feature = "sentry") || cfg!(any(target_os =
//! "macos", target_os = "ios"))`, i.e. it computed Config.zig's per-target
//! `orelse` default itself, from the compilation target. That was faithful for
//! the BOOLEAN and unfaithful for the thing the boolean also decides:
//! `SharedDeps.zig:407` gates on the SAME `config.sentry` whether sentry-native
//! and breakpad are LINKED AT ALL, and a link gate cannot be target-conditional
//! in cargo — `default = [...]` takes no `cfg`, and a `cfg`-gated dependency is
//! still present in the resolve graph. So the per-target switch moved to where
//! the reference's own lives, build orchestration (`xtask` passes
//! `--features sentry` for the Apple xcframework and not for anything else),
//! exactly as `-Dsimd` already does here. The cargo feature is now the single
//! source of truth for both halves, just as `config.sentry` is for both of the
//! reference's. See the `[features]` header of this crate's Cargo.toml.

/// Build with Sentry crash reporting. Default for macOS is true, false for
/// any other system.
///
/// The `sentry` cargo feature IS this option (both directions of
/// `-Dsentry=…`); the per-target default is applied by the builder — see the
/// PORT NOTE in the module header.
///
/// Reference (`src/build/Config.zig`):
/// ```text
/// config.sentry = b.option(bool, "sentry", ...) orelse sentry: {
///     switch (target.result.os.tag) {
///         .macos, .ios => break :sentry true,
///
///         // Note its false for linux because the crash reports on Linux
///         // don't have much useful information.
///         else => break :sentry false,
///     }
/// };
/// ```
pub const SENTRY: bool = cfg!(feature = "sentry");

#[cfg(test)]
mod tests {
    use super::SENTRY;

    /// The `sentry` cargo feature is the resolved `-Dsentry` option: `SENTRY`
    /// tracks it and NOTHING else.
    ///
    /// PORT NOTE (W16-01, a deliberate parity-contract change): this test used
    /// to assert the per-target `orelse` default off the TARGET CFG —
    /// `assert!(SENTRY)` on macOS/iOS, `assert!(!SENTRY)` elsewhere. It cannot
    /// still say that, because the const no longer reads the target: the
    /// per-target switch moved into the builder so that the `SharedDeps.zig:407`
    /// link gate could exist at all (module header). The default is therefore
    /// asserted where it now lives — `xtask`'s Apple artifact passes
    /// `--features sentry`, non-Apple artifacts do not — and what is asserted
    /// HERE is the contract the const itself still owns. The test is narrowed,
    /// never deleted, and the reference quotation stays in the doc comment
    /// above.
    // The constant IS the subject under test (a build-options value), so the
    // constant-assertion lint does not apply.
    #[allow(clippy::assertions_on_constants)]
    #[test]
    fn sentry_tracks_the_resolved_build_option() {
        if cfg!(feature = "sentry") {
            assert!(SENTRY, "`--features sentry` must force SENTRY=true");
        } else {
            assert!(!SENTRY, "without the feature SENTRY must be false");
        }
    }

    /// W15-04 proving test: the `sentry` cargo feature is the explicit
    /// `-Dsentry=true` enablement (Config.zig user override) and must force
    /// `SENTRY` on for ANY target — this is the Linux opt-in contract.
    /// (Without the fix the feature does not exist, so building the crate's
    /// tests with `--features sentry` fails outright.)
    #[allow(clippy::assertions_on_constants)]
    #[cfg(feature = "sentry")]
    #[test]
    fn sentry_feature_forces_enable_on_any_target() {
        assert!(SENTRY, "`--features sentry` must force SENTRY=true");
    }
}
