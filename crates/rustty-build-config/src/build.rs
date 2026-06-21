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
//! target resolution, git version detection) stays in the `rustty-xtask`
//! build tooling.

use crate::apprt::Runtime as ApprtRuntime;
use crate::font::Backend as FontBackend;
use crate::os::WasmTarget;
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

/// A semantic version `MAJOR.MINOR.PATCH`, mirroring the reference
/// `std.SemanticVersion` fields read at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticVersion {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
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
