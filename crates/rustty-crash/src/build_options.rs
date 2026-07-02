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
//! `host_target()` defaults. (A cargo-feature override can be added at the
//! `zig build -Dsentry=...` seam if a build ever needs to flip it.)

/// Build with Sentry crash reporting. Default for macOS is true, false for
/// any other system.
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
pub const SENTRY: bool = cfg!(any(target_os = "macos", target_os = "ios"));
