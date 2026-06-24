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

use crate::build_options::Artifact;

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

/// The version string, `MAJOR.MINOR.PATCH[-PRE][+BUILD]`.
// PORT NOTE: `opts.addOption([]const u8, "version_string", b.fmt("{f}", .{self.version}))`.
// Sourced from the rustty crate version (the build-time source of truth).
pub const VERSION_STRING: &str = env!("CARGO_PKG_VERSION");

/// The major component of the version.
// PORT NOTE: `opts.addOption(usize, "version_major", self.version.major)`.
pub const VERSION_MAJOR: usize = parse_env_usize(env!("CARGO_PKG_VERSION_MAJOR"));

/// The minor component of the version.
// PORT NOTE: `opts.addOption(usize, "version_minor", self.version.minor)`.
pub const VERSION_MINOR: usize = parse_env_usize(env!("CARGO_PKG_VERSION_MINOR"));

/// The patch component of the version.
// PORT NOTE: `opts.addOption(usize, "version_patch", self.version.patch)`.
pub const VERSION_PATCH: usize = parse_env_usize(env!("CARGO_PKG_VERSION_PATCH"));

/// The pre-release component of the version, if any.
// PORT NOTE: `opts.addOption(?[]const u8, "version_pre", self.version.pre)`.
// `CARGO_PKG_VERSION_PRE` is the empty string when there is no pre-release; the
// reference uses `null` for that case, so map empty to `None`.
pub const VERSION_PRE: Option<&str> = none_if_empty(env!("CARGO_PKG_VERSION_PRE"));

/// The build-metadata component of the version, if any.
// PORT NOTE: `opts.addOption(?[]const u8, "version_build", self.version.build)`.
// Cargo has no build-metadata env var, so this is always `None` (matching the
// `null` the reference writes when `self.version.build` is unset).
pub const VERSION_BUILD: Option<&str> = None;

/// Parse a non-negative decimal integer at compile time, mirroring how the
/// reference reads `self.version.major/minor/patch` as `usize` values.
const fn parse_env_usize(s: &str) -> usize {
    let bytes = s.as_bytes();
    let mut value: usize = 0;
    let mut i = 0;
    while i < bytes.len() {
        value = value * 10 + (bytes[i] - b'0') as usize;
        i += 1;
    }
    value
}

/// Map an empty string to `None`, otherwise `Some`. Used for the optional
/// `version_pre`/`version_build` fields, whose absence the reference models as
/// `null`.
const fn none_if_empty(s: &str) -> Option<&str> {
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

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
    fn version_components_match_string() {
        // The major.minor.patch components reconstruct the package version prefix.
        let prefix = format!("{VERSION_MAJOR}.{VERSION_MINOR}.{VERSION_PATCH}");
        assert!(
            VERSION_STRING.starts_with(&prefix),
            "version string {VERSION_STRING:?} should start with {prefix:?}",
        );
    }

    #[test]
    fn parse_env_usize_reads_decimal() {
        assert_eq!(0, parse_env_usize("0"));
        assert_eq!(7, parse_env_usize("7"));
        assert_eq!(123, parse_env_usize("123"));
    }

    #[test]
    fn none_if_empty_maps_empty_to_none() {
        assert_eq!(None, none_if_empty(""));
        assert_eq!(Some("rc.1"), none_if_empty("rc.1"));
    }
}
