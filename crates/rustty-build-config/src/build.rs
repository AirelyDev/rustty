//! Build configuration. This is the configuration that is populated
//! during `zig build` to control the rest of the build process.
//!
//! BREAK #3: in the reference, `src/build/Config.zig` lives in the `build/`
//! (xtask) tree and owns the `ReleaseChannel`/`ExeEntrypoint` enums plus the
//! `fromOptions()` runtime view that `src/build_config.zig` re-exports. The
//! runtime-relevant pieces that `build_config.zig` actually consumes — the two
//! enums and the comptime [`Config`] view produced by `fromOptions()` — are
//! relocated HERE so build-config has no upward dependency on the build script
//! (R4). The pure build-time machinery (`init`, `addOptions`, `terminalOptions`,
//! target resolution) stays in the `rustty-xtask` build tooling; git version
//! detection is `crate::git_version`, run by this crate's `build.rs`, and the
//! composition is `crate::version`.

use crate::apprt::Runtime as ApprtRuntime;
use crate::font::Backend as FontBackend;
use crate::os::{Target, WasmTarget};
use crate::renderer::Backend as RendererBackend;

/// Build configuration. This is the configuration that is populated
/// during `zig build` to control the rest of the build process.
///
/// This is the comptime view of the config, mirroring the reference
/// `Config.fromOptions()` (the subset of fields available at runtime via the
/// generated `build_options`). The pure build-time fields (optimize, target,
/// emit_* artifacts, env, ...) are not part of the runtime view.
#[derive(Debug, Clone)]
pub struct Config {
    /// Comptime interfaces.
    pub app_runtime: ApprtRuntime,
    pub renderer: RendererBackend,
    pub font_backend: FontBackend,

    /// Feature flags.
    pub i18n: bool,

    /// Ghostty exe properties.
    pub exe_entrypoint: ExeEntrypoint,
    pub version: SemanticVersion,

    /// Artifacts.
    pub flatpak: bool,
    pub snap: bool,

    /// The wasm target platform.
    pub wasm_target: WasmTarget,
    pub wasm_shared: bool,
}

impl Default for Config {
    /// Mirrors the reference field defaults in `Config`:
    /// `app_runtime: ApprtRuntime = .none`, `renderer: RendererBackend =
    /// .opengl`, `font_backend: FontBackend = .freetype`, `i18n: bool = true`,
    /// `exe_entrypoint: ExeEntrypoint = .ghostty`, `flatpak: bool = false`,
    /// `snap: bool = false`, `wasm_shared: bool = true`, and a zeroed
    /// `version: std.SemanticVersion = .{ .major = 0, .minor = 0, .patch = 0 }`.
    fn default() -> Config {
        Config {
            app_runtime: ApprtRuntime::None,
            renderer: RendererBackend::Opengl,
            font_backend: FontBackend::Freetype,
            i18n: true,
            exe_entrypoint: ExeEntrypoint::Rustty,
            version: SemanticVersion {
                major: 0,
                minor: 0,
                patch: 0,
            },
            flatpak: false,
            snap: false,
            wasm_target: WasmTarget::Browser,
            wasm_shared: true,
        }
    }
}

impl Config {
    /// Rehydrate our Config from the comptime options. Note that not all
    /// options are available at comptime, so look closely at this implementation
    /// to see what is and isn't available.
    ///
    /// Reference: `pub fn fromOptions() Config`. The reference reads the
    /// `build_options` blob `zig build` generated from `Config.init()`'s
    /// per-target `default()` selection logic. The Rust port has no separate
    /// build-options blob, so it reproduces that exact per-target selection
    /// directly from the resolved `target` / `wasm_target` (with cargo features
    /// as the override seam — when no overriding feature is set the per-target
    /// `default()` is used verbatim, matching the reference defaults for the
    /// same OS/target). The fields the reference marks "Unused at runtime"
    /// (`optimize`, `target`, `env`) are not part of this runtime view.
    pub const fn from_options(target: Target, wasm_target: WasmTarget) -> Config {
        Config {
            // Reference `init()`: `config.app_runtime = ... orelse
            // ApprtRuntime.default(target.result);`.
            app_runtime: ApprtRuntime::default_for(target),
            // Reference `init()`: `config.renderer = ... orelse
            // RendererBackend.default(target.result, wasm_target);`.
            renderer: RendererBackend::default(target, wasm_target),
            // Reference `init()`: `config.font_backend = ... orelse
            // FontBackend.default(target.result, wasm_target);`.
            font_backend: FontBackend::default(target, wasm_target),
            // Reference `init()`: `config.i18n = ... orelse switch
            // (target.result.os.tag) { .macos, .ios => true, .linux, .freebsd =>
            // target.result.isGnuLibC(), else => false };`. `isGnuLibC()` maps
            // to `cfg!(target_env = "gnu")`.
            i18n: i18n_default(target),
            // Reference field default: `exe_entrypoint: ExeEntrypoint = .ghostty`
            // (no `init()`/option override of the runtime view).
            exe_entrypoint: ExeEntrypoint::Rustty,
            // Reference `fromOptions`: `.version = options.app_version`.
            version: APP_VERSION,
            // Reference `init()`: `config.flatpak = b.option(bool, "flatpak",
            // ...) orelse false;`. rustty's analogue of a `-D` build option is
            // a cargo feature, so the option is read here rather than being
            // pinned to the field default. Enabled by `rustty-termio/flatpak`,
            // which the Flatpak manifest passes — see this crate's Cargo.toml
            // for why it must be reached through termio and not directly.
            flatpak: cfg!(feature = "flatpak"),
            // Reference `init()`: `config.snap = b.option(bool, "snap", ...)
            // orelse false;`. Passed by `snap/snapcraft.yaml`.
            snap: cfg!(feature = "snap"),
            // Reference `init()`: `const wasm_target: WasmTarget = .browser;`.
            wasm_target,
            // Reference field default: `wasm_shared: bool = true`.
            wasm_shared: true,
        }
    }
}

/// `target.result.isGnuLibC()` — whether the compilation target links glibc.
/// Rust spells this as the `target_env = "gnu"` cfg; it is named here (rather
/// than written inline in `i18n_default`'s switch arm below) because `cfg!`
/// folds to a literal at expansion, which on glibc targets collapses that
/// `switch` into three literal arms and trips `clippy::match_like_matches_macro`
/// — a rewrite that would erase the reference's arm structure.
const IS_GNU_LIBC: bool = cfg!(target_env = "gnu");

/// `i18n` default selection, factored out so it stays `const`. Reference
/// `init()`: `switch (target.result.os.tag) { .macos, .ios => true, .linux,
/// .freebsd => target.result.isGnuLibC(), else => false }`.
const fn i18n_default(target: Target) -> bool {
    match target.os {
        crate::os::Os::Macos | crate::os::Os::Ios => true,
        crate::os::Os::Linux | crate::os::Os::Freebsd => IS_GNU_LIBC,
        _ => false,
    }
}

/// The semantic version of this build, parsed from the composed
/// [`crate::VERSION_STRING`] (S63-F2). Reference: `options.app_version`.
const APP_VERSION: SemanticVersion = SemanticVersion::parse(crate::VERSION_STRING);

/// A semantic version `MAJOR.MINOR.PATCH`, mirroring the reference
/// `std.SemanticVersion` fields read at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticVersion {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl SemanticVersion {
    /// Parse the `MAJOR.MINOR.PATCH` prefix of a semantic-version string,
    /// ignoring any `-PRE`/`+BUILD` suffix. `const` so it can populate
    /// [`APP_VERSION`] at compile time, mirroring the reference reading
    /// `options.app_version` at comptime.
    pub const fn parse(s: &str) -> SemanticVersion {
        let bytes = s.as_bytes();
        let mut i = 0;
        // Parse one dot-separated unsigned integer starting at `bytes[*idx]`,
        // advancing `*idx` past it (and past a trailing `.` if present).
        let major;
        let minor;
        let patch;
        (major, i) = parse_u64(bytes, i);
        if i < bytes.len() && bytes[i] == b'.' {
            i += 1;
        }
        (minor, i) = parse_u64(bytes, i);
        if i < bytes.len() && bytes[i] == b'.' {
            i += 1;
        }
        (patch, _) = parse_u64(bytes, i);
        SemanticVersion {
            major,
            minor,
            patch,
        }
    }
}

/// Parse a leading run of ASCII digits as a `u64`, returning the value and the
/// index just past the last digit consumed.
const fn parse_u64(bytes: &[u8], start: usize) -> (u64, usize) {
    let mut value: u64 = 0;
    let mut i = start;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        value = value * 10 + (bytes[i] - b'0') as u64;
        i += 1;
    }
    (value, i)
}

/// The possible entrypoints for the exe artifact. This has no effect on
/// other artifact types (i.e. lib, wasm_module).
///
/// The whole existence of this enum is to workaround the fact that Zig
/// doesn't allow the main function to be in a file in a subdirctory
/// from the "root" of the module, and I don't want to pollute our root
/// directory with a bunch of individual zig files for each entrypoint.
///
/// Therefore, main.zig uses this to switch between the different entrypoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExeEntrypoint {
    #[default]
    Rustty,
    Helpgen,
    MdgenRustty1,
    MdgenRustty5,
    WebgenConfig,
    WebgenActions,
    WebgenCommands,
}

/// The release channel for the build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseChannel {
    /// Unstable builds on every commit.
    Tip,

    /// Stable tagged releases.
    Stable,
}

// `ReleaseChannel` is used as the `auto-update-channel` config field type
// (`?ReleaseChannel`). The reference's `Config.changed` resolves its `deepEqual`
// via the `.@"enum"` arm (`old == new`). The orphan rule requires this
// `DeepEqual` impl to live in `ReleaseChannel`'s home crate.
impl rustty_datastruct::comparison::DeepEqual for ReleaseChannel {
    fn deep_equal(&self, other: &Self) -> bool {
        self == other
    }
}

impl ReleaseChannel {
    /// Derive the release channel from a version's pre-release component,
    /// mirroring the reference channel logic in `Config.addOptions`:
    /// `const pre = self.version.pre orelse break :channel .stable; if (pre.len
    /// == 0) break :channel .stable; break :channel .tip;`.
    pub const fn from_version_pre(pre: Option<&str>) -> ReleaseChannel {
        match pre {
            None => ReleaseChannel::Stable,
            Some(p) if p.is_empty() => ReleaseChannel::Stable,
            Some(_) => ReleaseChannel::Tip,
        }
    }
}
