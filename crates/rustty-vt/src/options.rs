//! rustty-vt options
//!
//! PORTS: ghostty `terminal_options` build-generated module (terminal/build_options.zig Options)
//!
//! PORT STATUS: done

// PORT NOTE: in the reference there is no `terminal_options.zig` source file —
// the module is *generated* by `build.zig` calling `build_options.Options.add`,
// and the rest of the `terminal` module reads it via
//   const build_options = @import("terminal_options");
// then `build_options.kitty_graphics`, `build_options.slow_runtime_safety`, etc.
//
// This module is the ported equivalent of that generated module: it exposes the
// exact option keys `Options.add` writes (see `build_options.rs` `Resolved`),
// resolved for the rustty build. Downstream ported `terminal` code reads these as
// `crate::options::<KEY>` (mirroring `build_options.<key>` in the reference), and
// uses them in `if`/`#[cfg(...)]` gates exactly where the reference uses
// `if (comptime build_options.<key>)`.
//
// The synthesized values follow `Options.add` faithfully:
//   - `kitty_graphics`     = !(arch == .wasm32 and os == .freestanding)
//   - `tmux_control_mode`  = oniguruma
// resolved against the current build target (`cfg!(target_arch/target_os)`) and
// the rustty feature flags below.

use crate::build_options::{Artifact, SemanticVersion};

/// The target artifact to build. This will gate some functionality.
// PORT NOTE: `opts.addOption(Artifact, "artifact", self.artifact)`. rustty builds
// the application artifact (the reference `.ghostty`); the `lib` (libghostty-vt)
// artifact is selected by a later build seam.
pub const ARTIFACT: Artifact = Artifact::Ghostty;

/// Force C ABI mode on or off.
// PORT NOTE: `opts.addOption(bool, "c_abi", self.c_abi)`. The C ABI surface
// (terminal/c/) is the separate `rustty-vt-c` crate, so this in-crate option is
// false; the C library build sets it via that crate.
pub const C_ABI: bool = false;

/// Whether Oniguruma regex support is available. If this isn't available, some
/// features will be disabled (Kitty graphics protocol, Tmux control mode).
// PORT NOTE: `opts.addOption(bool, "oniguruma", self.oniguruma)`. Gated by the
// `oniguruma` cargo feature so the build graph can toggle it as `build.zig` does.
pub const ONIGURUMA: bool = cfg!(feature = "oniguruma");

/// Whether SIMD-accelerated code paths are built.
// PORT NOTE: `opts.addOption(bool, "simd", self.simd)`. Gated by the `simd` cargo
// feature, matching the reference build knob.
pub const SIMD: bool = cfg!(feature = "simd");

/// True if we should enable the "slow" runtime safety checks. These are runtime
/// safety checks that are slower than typical and should generally be disabled in
/// production builds.
// PORT NOTE: `opts.addOption(bool, "slow_runtime_safety", self.slow_runtime_safety)`.
// rustty ties this to `debug_assertions` (the convention used across this crate,
// e.g. `page.rs`), so the slow checks are present in debug builds and stripped in
// release — the same lifecycle as the reference build knob.
pub const SLOW_RUNTIME_SAFETY: bool = cfg!(debug_assertions);

/// Whether the Kitty graphics protocol is enabled.
// PORT NOTE: synthesized by `Options.add`:
//   !(target.cpu.arch == .wasm32 and target.os.tag == .freestanding)
// Disabled only on wasm32-freestanding (no way to get timestamps there).
pub const KITTY_GRAPHICS: bool = !(cfg!(target_arch = "wasm32") && cfg!(target_os = "none"));

/// Whether Tmux control mode is enabled.
// PORT NOTE: synthesized by `Options.add`: `opts.addOption(bool,
// "tmux_control_mode", self.oniguruma)` — it tracks `oniguruma` exactly.
pub const TMUX_CONTROL_MODE: bool = ONIGURUMA;

// PORT NOTE (version source of truth): for the terminal module the reference
// version is ARTIFACT-selected — `Config.terminalOptions` (src/build/Config.zig)
// plumbs `.version = switch (artifact) { .ghostty => self.version, .lib =>
// self.lib_version }`, and `lib_version` is the DELIBERATELY SEPARATE
// libghostty version (`build.zig:7-10`: `const lib_version = "0.1.0-dev";` —
// "We use a separate version from the app"), parsed via
// `std.SemanticVersion.parse` (src/build/Config.zig:307). This crate IS the vt
// library, so its version is the vt LIB version from `rustty-identity` (the R2
// identity source of truth) — NOT `CARGO_PKG_VERSION` (the workspace/app
// version, which only coincidentally matched): `0.1.0` under the rustty
// profile, the reference's exact `0.1.0-dev` under `identity-oracle`.
const VERSION: SemanticVersion = SemanticVersion::parse(rustty_identity::VT_LIB_VERSION);

/// The version string, `MAJOR.MINOR.PATCH[-PRE][+BUILD]`.
// PORT NOTE: `opts.addOption([]const u8, "version_string", b.fmt("{f}", .{self.version}))`.
// The reference formats the parsed semver back to a string
// (`SemanticVersion.format`: `major.minor.patch[-pre][+build]`); for a
// well-formed input that round-trips to the source string, so this is the
// identity constant verbatim (asserted by the `version_string_is_semver_format`
// test below).
pub const VERSION_STRING: &str = rustty_identity::VT_LIB_VERSION;

/// The major component of the version.
// PORT NOTE: `opts.addOption(usize, "version_major", self.version.major)`.
pub const VERSION_MAJOR: usize = VERSION.major;

/// The minor component of the version.
// PORT NOTE: `opts.addOption(usize, "version_minor", self.version.minor)`.
pub const VERSION_MINOR: usize = VERSION.minor;

/// The patch component of the version.
// PORT NOTE: `opts.addOption(usize, "version_patch", self.version.patch)`.
pub const VERSION_PATCH: usize = VERSION.patch;

/// The pre-release component of the version, if any.
// PORT NOTE: `opts.addOption(?[]const u8, "version_pre", self.version.pre)`.
// The reference models "no pre-release" as `null`; `None` here.
pub const VERSION_PRE: Option<&str> = VERSION.pre;

/// The build-metadata component of the version, if any.
// PORT NOTE: `opts.addOption(?[]const u8, "version_build", self.version.build)`.
// `null` in the reference when the version string has no `+BUILD` suffix.
pub const VERSION_BUILD: Option<&str> = VERSION.build;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tmux_control_mode_tracks_oniguruma() {
        // Synthesized exactly as `Options.add` does: tmux tracks oniguruma.
        assert_eq!(ONIGURUMA, TMUX_CONTROL_MODE);
    }

    #[test]
    fn kitty_graphics_enabled_on_this_target() {
        // This crate is not built for wasm32-freestanding, so kitty graphics is
        // enabled (the only target the reference disables it on).
        let wasm_freestanding = cfg!(target_arch = "wasm32") && cfg!(target_os = "none");
        assert_eq!(!wasm_freestanding, KITTY_GRAPHICS);
    }

    #[test]
    fn artifact_is_application() {
        assert_eq!(Artifact::Ghostty, ARTIFACT);
    }

    #[test]
    fn version_string_is_semver_format() {
        // `version_string` in the reference is `b.fmt("{f}", .{self.version})` —
        // the PARSED semver re-formatted (`SemanticVersion.format`:
        // `major.minor.patch[-pre][+build]`). Assert the same round-trip here:
        // re-formatting the parsed components reproduces VERSION_STRING exactly.
        let mut formatted = format!("{VERSION_MAJOR}.{VERSION_MINOR}.{VERSION_PATCH}");
        if let Some(pre) = VERSION_PRE {
            formatted.push('-');
            formatted.push_str(pre);
        }
        if let Some(build) = VERSION_BUILD {
            formatted.push('+');
            formatted.push_str(build);
        }
        assert_eq!(VERSION_STRING, formatted);
    }

    /// Under the default (rustty) profile the vt lib version is `0.1.0`.
    #[cfg(not(feature = "identity-oracle"))]
    #[test]
    fn version_is_rustty_vt_lib_version() {
        assert_eq!("0.1.0", VERSION_STRING);
        assert_eq!(0, VERSION_MAJOR);
        assert_eq!(1, VERSION_MINOR);
        assert_eq!(0, VERSION_PATCH);
        assert_eq!(None, VERSION_PRE);
        assert_eq!(None, VERSION_BUILD);
    }

    /// Under `identity-oracle` the vt lib version is the reference's SEPARATE
    /// libghostty version (`build.zig:10`), exactly as the c-vt-build-info
    /// golden records it: "Version: 0.1.0-dev", "Version pre  : dev".
    #[cfg(feature = "identity-oracle")]
    #[test]
    fn version_is_reference_lib_version() {
        assert_eq!("0.1.0-dev", VERSION_STRING);
        assert_eq!(0, VERSION_MAJOR);
        assert_eq!(1, VERSION_MINOR);
        assert_eq!(0, VERSION_PATCH);
        assert_eq!(Some("dev"), VERSION_PRE);
        assert_eq!(None, VERSION_BUILD);
    }
}
