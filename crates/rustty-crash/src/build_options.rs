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
//! port has no separate build-options blob, so the option's per-target
//! `orelse` default is reproduced here as a `const` computed from the crate's
//! own compilation target — the same seam style as `rustty-build-config`'s
//! `host_target()` defaults — ORed with the `sentry` cargo feature, the
//! port's `zig build -Dsentry=true` seam (an explicit enablement on any
//! target, e.g. Linux). Cargo features are additive, so only the
//! enable-override direction of `-Dsentry=...` is expressible; the
//! disable-override (`-Dsentry=false` on macOS) has no cargo seam.

/// Build with Sentry crash reporting. Default for macOS is true, false for
/// any other system; the `sentry` cargo feature is the explicit
/// `-Dsentry=true` override (W15-04: Linux opt-in enablement).
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
pub const SENTRY: bool =
    cfg!(feature = "sentry") || cfg!(any(target_os = "macos", target_os = "ios"));

#[cfg(test)]
mod tests {
    use super::SENTRY;

    /// The per-target `orelse` default of Config.zig's `sentry` option:
    /// macOS/iOS default on; everywhere else default off (unless the `sentry`
    /// feature — the explicit `-Dsentry=true` override — is set).
    // The constant IS the subject under test (a build-options value), so the
    // constant-assertion lint does not apply.
    #[allow(clippy::assertions_on_constants)]
    #[test]
    fn sentry_default_matches_config_zig() {
        if cfg!(any(target_os = "macos", target_os = "ios")) {
            assert!(SENTRY, "macOS/iOS must default sentry=true");
        } else if !cfg!(feature = "sentry") {
            assert!(!SENTRY, "non-Apple targets must default sentry=false");
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
