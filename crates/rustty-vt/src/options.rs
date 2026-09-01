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
// resolved for the Rustty build. Downstream ported `terminal` code reads these as
// `crate::options::<KEY>` (mirroring `build_options.<key>` in the reference), and
// uses them in `if`/`#[cfg(...)]` gates exactly where the reference uses
// `if (comptime build_options.<key>)`.
//
// The synthesized values follow `Options.add` faithfully:
//   - `kitty_graphics`     = !(arch == .wasm32 and os == .freestanding)
//   - `tmux_control_mode`  = oniguruma
// resolved against the current build target (`cfg!(target_arch/target_os)`) and
// the Rustty feature flags below.

use crate::build_options::{Artifact, SemanticVersion};

/// The target artifact to build. This will gate some functionality.
// PORT NOTE: `opts.addOption(Artifact, "artifact", self.artifact)`. Rustty builds
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
//
// TARGET-CONDITIONAL DEFAULT (`src/build/Config.zig:196-206`): `-Dsimd` is
//
//     config.simd = b.option(bool, "simd", "...") orelse simd: {
//         // We can't build our SIMD dependencies for Wasm. Note that we may
//         // still use SIMD features in the Wasm-builds.
//         if (target.result.cpu.arch.isWasm()) break :simd false;
//         break :simd true;
//     };
//
// i.e. ON for every native target and OFF for wasm — which is why ghostty ships
// `ghostty-vt.wasm` built simd=false. Cargo cannot express a per-target `default`
// feature set, so the wasm arm of that `orelse` is resolved HERE, at the same
// place the port already resolves `Options.add`'s other target-conditional
// synthesis (`KITTY_GRAPHICS` below). The cargo feature therefore carries only
// the non-wasm half of the knob: default-on via the shipping crates'
// `default = ["simd"]` (rustty-vt-c, rustty-apprt-c) / rustty-core's dependency
// feature list, and `--no-default-features` as the `-Dsimd=false` analog.
//
// The carve-out is not merely cosmetic parity with a reported flag: with `SIMD`
// true, `stream.rs:757` takes the `next_slice_capped` fast path instead of the
// reference's own byte-by-byte arm, so a wasm artifact would run a code path the
// reference cannot even build there.
pub const SIMD: bool = resolve_simd(cfg!(feature = "simd"), cfg!(target_arch = "wasm32"));

/// `-Dsimd`'s resolution, factored out so both arms of the wasm carve-out are
/// testable from a native test run (`build/Config.zig:196-206`).
///
/// `requested` is the cargo-feature half of the knob (the reference's explicit
/// `-Dsimd=<bool>`, plus the non-wasm `break :simd true` default); `is_wasm` is
/// `target.result.cpu.arch.isWasm()`.
const fn resolve_simd(requested: bool, is_wasm: bool) -> bool {
    // if (target.result.cpu.arch.isWasm()) break :simd false;
    if is_wasm {
        return false;
    }

    requested
}

// Compile-time proof of the wasm arm: EVERY wasm build of this crate resolves
// the option off, whatever feature set the graph unified to. Deleting the
// carve-out above breaks `cargo build -p rustty-vt-c --target
// wasm32-unknown-unknown` (whose `default = ["simd"]` is active there).
#[cfg(target_arch = "wasm32")]
const _: () = assert!(!SIMD, "ghostty builds wasm targets with simd=false");

/// True if we should enable the "slow" runtime safety checks. These are runtime
/// safety checks that are slower than typical and should generally be disabled in
/// production builds.
// PORT NOTE: `opts.addOption(bool, "slow_runtime_safety", self.slow_runtime_safety)`.
// Rustty ties this to `debug_assertions` (the convention used across this crate,
// e.g. `page.rs`), so the slow checks are present in debug builds and stripped in
// release — the same lifecycle as the reference build knob.
pub const SLOW_RUNTIME_SAFETY: bool = cfg!(debug_assertions);

// PORT NOTE (`1fdbb8c91` `-Dvt-features`): the feature gates below are the
// flat bools `Options.add`'s feature loop emits (e.g. `options.snapshot`),
// resolved from `Options.features` (see `build_options::Features`).
//
// Each is `cfg!(feature = ...)`-driven, exactly like `SIMD` above, over the
// nine cargo features named for the `-Dvt-features` tokens (hyphen spelling:
// `render-state`, `input-encode`, `grid-introspection`, `glyph-protocol`,
// `kitty-graphics`; `eqlName` treats `-` and `_` as interchangeable, and cargo
// feature names conventionally use `-`).
//
// MAPPING THE DEFAULT-ON SET ONTO ADDITIVE CARGO FEATURES. `-Dvt-features`
// starts from the all-enabled set and SUBTRACTS (`-all,+render-state`); cargo
// features only ever ADD. The equivalence is the crate's `default` feature
// list, which names all nine: an ordinary build — every application-tier
// consumer, every workspace `cargo test` — therefore compiles with the
// reference's default `Features{}`, and "the Ghostty application itself always
// builds with every feature enabled" (the `Features` doc) holds by
// construction. Trimming is `--no-default-features` plus the features to keep,
// so the reference's `-Dvt-features=-all,+render-state` example is spelled
// `--no-default-features --features render-state`. Because cargo unifies
// features across a build graph, a trimmed artifact must be built for the lib
// crate alone (`-p rustty-vt-c`), which is exactly how the standalone
// libghostty-vt analog is produced (`.port/tools/build-vt-wasm.sh`).

/// Whether terminal state serialization (the snapshot subsystem) is enabled.
pub const SNAPSHOT: bool = cfg!(feature = "snapshot");

/// Whether the textual export formatter is enabled.
pub const FORMATTER: bool = cfg!(feature = "formatter");

/// Whether text selection is enabled.
pub const SELECTION: bool = cfg!(feature = "selection");

/// Whether the render state API is enabled.
pub const RENDER_STATE: bool = cfg!(feature = "render-state");

/// Whether host input-event encoding is enabled.
pub const INPUT_ENCODE: bool = cfg!(feature = "input-encode");

/// Whether the color utilities are enabled.
pub const COLOR: bool = cfg!(feature = "color");

/// Whether direct grid introspection is enabled.
pub const GRID_INTROSPECTION: bool = cfg!(feature = "grid-introspection");

/// Whether the APC glyph protocol is enabled.
// PORT NOTE: gates the stream handling + glossary storage
// (`stream_terminal.rs` glyph arm, the two glyph-protocol tests).
pub const GLYPH_PROTOCOL: bool = cfg!(feature = "glyph-protocol");

/// Whether the Kitty graphics protocol is enabled.
// PORT NOTE: synthesized by `Options.add`'s feature loop (`1fdbb8c91`): the
// `kitty_graphics` feature (default true) with the per-field override
// `Options.kittyGraphics(target)` — `if (target.os.tag == .freestanding)
// return false;` (`3376153a4`: ANY freestanding OS, not only
// wasm32-freestanding; no way to get timestamps there). Zig's `.freestanding`
// OS tag is Rust's `target_os = "none"`.
pub const KITTY_GRAPHICS: bool = cfg!(feature = "kitty-graphics") && !cfg!(target_os = "none");

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
// version, which only coincidentally matched): `0.1.0` under the Rustty
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

    /// W16-TARGET-01 wasm arm. `-Dsimd`'s default is TARGET-CONDITIONAL
    /// (`build/Config.zig:200-206`): `if (target.result.cpu.arch.isWasm())
    /// break :simd false; break :simd true;` — which is why ghostty ships
    /// `ghostty-vt.wasm` built simd=false. Both arms of that `orelse` are
    /// asserted here from a native run; the wasm arm is additionally proven at
    /// compile time by the `#[cfg(target_arch = "wasm32")] const _` above,
    /// which every wasm build of this crate evaluates.
    #[test]
    fn simd_default_is_target_conditional() {
        // Wasm: forced off no matter what the feature graph unified to — the
        // reference "can't build our SIMD dependencies for Wasm".
        assert!(!resolve_simd(true, true));
        assert!(!resolve_simd(false, true));

        // Everything else: the knob passes straight through, so the shipping
        // crates' `default = ["simd"]` reaches `stream.rs`'s fast path and
        // `--no-default-features` is the `-Dsimd=false` analog.
        assert!(resolve_simd(true, false));
        assert!(!resolve_simd(false, false));
    }

    /// The exported constant really is wired to that resolution (anti-vacuity:
    /// `simd_default_is_target_conditional` alone would still pass if `SIMD`
    /// went back to a bare `cfg!(feature = "simd")`).
    #[test]
    fn simd_const_uses_the_target_conditional_resolution() {
        assert_eq!(
            resolve_simd(cfg!(feature = "simd"), cfg!(target_arch = "wasm32")),
            SIMD
        );
        // This test binary is native (cargo cannot run a wasm32 test harness
        // here), so on THIS target the knob is exactly the cargo feature —
        // which is what the shipping tiers' `default = ["simd"]` pins.
        #[cfg(not(target_arch = "wasm32"))]
        assert_eq!(cfg!(feature = "simd"), SIMD);
    }

    #[test]
    fn kitty_graphics_enabled_on_this_target() {
        // This test binary is never a freestanding target, so kitty graphics
        // is enabled (freestanding — any arch since `3376153a4` — is the only
        // OS the reference disables it on).
        let freestanding = cfg!(target_os = "none");
        assert_eq!(!freestanding, KITTY_GRAPHICS);
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

    /// Under the default (Rustty) profile the vt lib version is `0.1.0`.
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
