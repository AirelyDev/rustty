//! rustty-build-config
//!
//! PORTS: ghostty src/build_config.zig (comptime build config + backend enums defined here, not pulled up).
//!
//! PORT STATUS: done
//!
//! Build options, available at comptime. Used to configure features. This
//! will reproduce some of the fields from builtin and build_options just
//! so we can limit the amount of imports we need AND give us the ability
//! to shim logic and values into them later.
//!
//! # BREAK #3 (R4 cycle-break)
//!
//! In the reference, `src/build_config.zig` imports the backend enums from
//! the upper-tier modules: `apprt.Runtime` (`src/apprt/runtime.zig`),
//! `font.Backend` (`src/font/backend.zig`), and `renderer.Backend`
//! (`src/renderer/backend.zig`). That makes the build-config module sit
//! *above* apprt/font/renderer in the import graph, which would create a
//! cycle once those crates depend on build-config for their feature flags.
//!
//! To break the cycle, this crate is the single source of truth: the backend
//! enums (`Runtime`/app-runtime, `font::Backend`, `renderer::Backend`, plus
//! the supporting `ReleaseChannel`, `Artifact`, `ExeEntrypoint`, and
//! `WasmTarget`) are DEFINED HERE, and the `rustty-apprt`, `rustty-font`, and
//! `rustty-renderer` crates re-export / import them from build-config (not the
//! reverse). This is a behaviour-preserving structural relocation only (R4):
//! the enum definitions, their `default()` selection logic, and the
//! `has*()` capability predicates are ported verbatim from the reference enum
//! files; their `test {}` blocks remain owned by the apprt/font/renderer
//! crates per the Phase-0 inventory contract.
//!
//! Version/identity are pulled from `rustty-identity` (R2).
//!
//! NO dependency on the ghostty tree (R3) — the reference is an external
//! oracle only. No upward deps (R4).

#![forbid(unsafe_code)]

pub mod apprt;
pub mod build;
pub mod font;
#[cfg(test)]
mod git_version;
pub mod os;
pub mod renderer;
pub mod version;

pub use apprt::Runtime;
pub use build::{Config, ExeEntrypoint, ReleaseChannel};
pub use os::wasm::Target as WasmTarget;
pub use os::{Arch, Os, Target};

// Re-export the backend enums (BREAK #3: defined here, imported by the
// upper-tier apprt/font/renderer crates — not the reverse) under the same
// `font::Backend` / `renderer::Backend` paths the reference re-export surface
// uses, so callers can name `build_config::font_backend()` of type
// `font::Backend` exactly as the reference names `font.Backend`.

/// The host target the comptime build-config defaults are computed for.
///
/// In the reference, `build/Config.zig`'s `init()` reads the resolved Zig build
/// target (`target.result`) and computes every per-target default from it. The
/// Rust port has no separate build-options blob, so the "resolved target" is
/// simply the crate's own compilation target, reconstructed here from the
/// `cfg!(target_os = ...)` / `cfg!(target_arch = ...)` toggles. The cargo
/// features (`app-runtime-*`, `renderer-*`, `font-*`, `i18n`, ...) override
/// these per-target defaults exactly as the `-D` build options override the
/// `default()` selectors in the reference; when no overriding feature is set
/// the per-target default is used verbatim.
const fn host_target() -> Target {
    let arch = if cfg!(target_arch = "wasm32") {
        Arch::Wasm32
    } else {
        Arch::Other
    };
    let os = if cfg!(target_os = "macos") {
        Os::Macos
    } else if cfg!(target_os = "ios") {
        Os::Ios
    } else if cfg!(target_os = "linux") {
        Os::Linux
    } else if cfg!(target_os = "freebsd") {
        Os::Freebsd
    } else if cfg!(target_os = "windows") {
        Os::Windows
    } else {
        Os::Other
    };
    Target::new(os, arch)
}

/// The wasm target platform. In the reference `init()` this is hardcoded to
/// `.browser` (the only semi-usable runtime); the port mirrors that.
const HOST_WASM_TARGET: WasmTarget = WasmTarget::Browser;

/// The release channel for this build.
///
/// Reference: `release_channel = std.meta.stringToEnum(ReleaseChannel,
/// @tagName(options.release_channel)).?`. In the reference this is resolved
/// from the `build_options` generated at `zig build` time; here the release
/// channel is derived from the composed [`VERSION`] in the same way
/// `Config.zig:693-701` does (see [`ReleaseChannel::from_version_pre`]): a
/// version with no pre-release component is `stable`, otherwise `tip`.
pub const fn release_channel() -> ReleaseChannel {
    ReleaseChannel::from_version_pre(version_pre())
}

/// The optimization mode as a string.
///
/// Reference (`build_config.zig`):
/// ```text
/// pub const mode_string = mode: {
///     const m = @tagName(builtin.mode);
///     if (std.mem.lastIndexOfScalar(u8, m, '.')) |i| break :mode m[i..];
///     break :mode m;
/// };
/// ```
/// Zig's `@tagName(builtin.mode)` yields the optimization-mode enum tag
/// (`"Debug"`, `"ReleaseSafe"`, `"ReleaseFast"`, `"ReleaseSmall"`); the
/// `lastIndexOfScalar(u8, m, '.')` slice keeps only the trailing component if
/// the tag is ever qualified with a `.`-prefixed namespace.
//
// PORT NOTE (parity S42-F3): Rust has no `OptimizeMode` reflection, and this
// used to report only `"Debug"` or `"ReleaseFast"` — so the two modes a user
// is most likely to be running when they paste `rustty +version` into a bug
// report (a safety-checked optimized build, and a size-optimized one) were
// both reported as `ReleaseFast`. All four are reconstructible from two facts:
//
//   * the OPTIMIZATION LEVEL, which the crate cannot see but cargo hands the
//     build script as `OPT_LEVEL` (baked in as `RUSTTY_OPT_LEVEL`), and
//   * `debug_assertions`, which is what distinguishes a safety-checked
//     optimized build from a fast one — the same distinction Zig draws between
//     `ReleaseSafe` and `ReleaseFast`.
//
// The mapping, which is the Cargo-profile analogue of Zig's four modes:
//
//   opt-level 0                         -> Debug
//   opt-level "s" / "z"                 -> ReleaseSmall
//   opt-level 1/2/3, debug_assertions   -> ReleaseSafe
//   opt-level 1/2/3, no debug_assertions-> ReleaseFast
pub const fn mode_string() -> &'static str {
    // `env!` is const-evaluable, so this stays a `const fn`.
    mode_string_for(env!("RUSTTY_OPT_LEVEL"), cfg!(debug_assertions))
}

/// The decision itself, over its two inputs rather than over this build's.
//
// Split out so the test can drive EVERY mode, including the two this build does
// not have — a test that re-derived the answer beside the function would prove
// nothing about the function (the tautology this parity pass exists to remove).
pub const fn mode_string_for(opt_level: &str, debug_assertions: bool) -> &'static str {
    let opt_level = opt_level.as_bytes();
    // `const fn` cannot use `==` on `&str`; compare the single byte cargo
    // always emits ("0", "1", "2", "3", "s", "z").
    if opt_level.len() == 1 && opt_level[0] == b'0' {
        return "Debug";
    }
    if opt_level.len() == 1 && (opt_level[0] == b's' || opt_level[0] == b'z') {
        return "ReleaseSmall";
    }
    if debug_assertions {
        "ReleaseSafe"
    } else {
        "ReleaseFast"
    }
}

/// The semantic version of this build.
///
/// Reference: `pub const version = options.app_version;`. The app version
/// `X.Y.Z` comes from the `rustty-identity` source-of-truth crate (R2); the
/// pre-release and build metadata come from git, composed as `Config.zig`
/// does (see [`version`]). CORRECTION (S63-F2 / S64-F1, 2026-09-23): this used
/// to be `rustty_identity::VERSION` verbatim, so every build reported `0.1.0`
/// on the `stable` channel.
pub const VERSION: &str = VERSION_STRING;

/// The composed version (`Config.zig:276-339`), from what `build.rs` found.
const COMPOSED: version::Composed = version::compose(
    rustty_identity::VERSION,
    env!("RUSTTY_VERSION_KIND"),
    env!("RUSTTY_VERSION_EXPLICIT"),
    env!("RUSTTY_VERSION_TAG"),
    env!("RUSTTY_VERSION_BRANCH"),
    env!("RUSTTY_VERSION_HASH"),
);

/// The semantic version of this build, as a string.
///
/// Reference: `pub const version_string = options.app_version_string;`.
pub const VERSION_STRING: &str = COMPOSED.as_str();

/// The Rust toolchain version this build was compiled with (bare version, e.g.
/// `1.93.1 (…)`).
///
/// Reference analog: `builtin.zig_version_string` — the reference has no
/// build_config member for this (Zig provides it as a builtin); it lives here
/// because build-config is the port's home for build-environment constants.
/// Captured by `build.rs` from `$RUSTC --version`.
pub const RUSTC_VERSION: &str = env!("RUSTTY_RUSTC_VERSION");

/// Returns the pre-release component of [`VERSION`], if any.
///
/// A semantic version `MAJOR.MINOR.PATCH[-PRE][+BUILD]` has a pre-release
/// component only when a `-` separator appears before any `+`. This mirrors the
/// reference channel derivation in `build/Config.zig`'s `addOptions`:
/// `const pre = self.version.pre orelse break :channel .stable; if (pre.len ==
/// 0) break :channel .stable; break :channel .tip;`.
const fn version_pre() -> Option<&'static str> {
    // `const fn` can't iterate `str` ergonomically, so operate on bytes.
    let bytes = VERSION.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            // A build-metadata separator ends the search with no pre-release.
            b'+' => return None,
            // A pre-release separator: everything after it (up to a `+`) is the
            // pre-release component. We only need to know whether it's
            // non-empty, so report a non-empty marker.
            b'-' => {
                if i + 1 < bytes.len() {
                    return Some("pre");
                }
                return None;
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// The artifact we're producing. This can be used to determine if we're
/// building a standalone exe, an embedded lib, etc.
///
/// Reference: `pub const artifact = Artifact.detect();`. The reference detects
/// this from `builtin.output_mode`/`builtin.target` at comptime; in the Rust
/// port the artifact is selected by the host crate (the `rustty` binary vs the
/// `librustty` library, via the `artifact-lib` feature, vs the wasm module) —
/// see [`Artifact::detect`].
pub const fn artifact() -> Artifact {
    Artifact::detect()
}

/// Our build configuration. We re-export a lot of these back at the
/// top-level so its a bit cleaner to use throughout the code. See the doc
/// comments in BuildConfig for details on each.
///
/// Reference: `const config = BuildConfig.fromOptions();` followed by the
/// `pub const exe_entrypoint/flatpak/snap/app_runtime/font_backend/renderer/i18n`
/// re-exports. In the reference these are read out of the `build_options` blob
/// `zig build` generates from `Config.init()`'s per-target `default()` logic;
/// the Rust port reproduces the same per-target selection from
/// [`host_target()`] (with cargo features as the override seam), so the runtime
/// values match the reference defaults for the same OS/target.
const CONFIG: Config = Config::from_options(host_target(), HOST_WASM_TARGET);

/// The exe entrypoint. Reference: `pub const exe_entrypoint = config.exe_entrypoint;`.
pub const fn exe_entrypoint() -> ExeEntrypoint {
    CONFIG.exe_entrypoint
}

/// Reference: `pub const flatpak = options.flatpak;`.
pub const fn flatpak() -> bool {
    CONFIG.flatpak
}

/// Reference: `os/wasm.zig` reads `options.wasm_shared`. Exposed here as a
/// `pub const fn` mirroring the other option re-exports so wasm32 builds can
/// read it without naming a non-existent crate-root item. (C4, VALIDATION_REPORT_2.)
pub const fn wasm_shared() -> bool {
    CONFIG.wasm_shared
}

/// Reference: `pub const snap = options.snap;`.
pub const fn snap() -> bool {
    CONFIG.snap
}

/// Reference: `pub const app_runtime: apprt.Runtime = config.app_runtime;`.
pub const fn app_runtime() -> Runtime {
    CONFIG.app_runtime
}

/// Reference: `pub const font_backend: font.Backend = config.font_backend;`.
pub const fn font_backend() -> font::Backend {
    CONFIG.font_backend
}

/// Reference: `pub const renderer: rendererpkg.Backend = config.renderer;`.
pub const fn renderer() -> renderer::Backend {
    CONFIG.renderer
}

/// Reference: `pub const i18n: bool = config.i18n;`.
pub const fn i18n() -> bool {
    CONFIG.i18n
}

/// Reference (`build_config.zig`): `pub const wasm_target: WasmTarget =
/// options.wasm_target;`. Consumed by `os/wasm/target.zig` and the
/// font/renderer wasm backend selectors.
pub const fn wasm_target() -> WasmTarget {
    CONFIG.wasm_target
}

/// The bundle ID for the app. This is used in many places and is currently
/// hardcoded here. We could make this configurable in the future if there
/// is a reason to do so.
///
/// On macOS, this must match the App bundle ID. We can get that dynamically
/// via an API but I don't want to pay the cost of that at runtime.
///
/// On GTK, this should match the various folders with resources.
///
/// There are many places that don't use this variable so simply swapping
/// this variable is NOT ENOUGH to change the bundle ID. I just wanted to
/// avoid it in Zig coe as much as possible.
//
// (R2) Pulled from the `rustty-identity` source-of-truth crate; the reference
// hardcodes `"com.mitchellh.ghostty"` here.
pub const BUNDLE_ID: &str = rustty_identity::BUNDLE_ID;

/// True if we should have "slow" runtime safety checks. The initial motivation
/// for this was terminal page/pagelist integrity checks. These were VERY
/// slow but very thorough. But they made it so slow that the terminal couldn't
/// be used for real work. We'd love to have an option to run a build with
/// safety checks that could be used for real work. This lets us do that.
///
/// Reference: `std.debug.runtime_safety and switch (builtin.mode) { .Debug =>
/// true, .ReleaseSafe, .ReleaseSmall, .ReleaseFast => false }`. In Rust,
/// "runtime safety" maps to debug-assertions: slow safety checks are on only
/// in debug builds.
pub const SLOW_RUNTIME_SAFETY: bool = cfg!(debug_assertions);

/// True if runtime safety checks are enabled.
///
/// Reference: `switch (builtin.mode) { .Debug, .ReleaseSafe => true,
/// .ReleaseFast, .ReleaseSmall => false }`. Rust's `debug_assertions` is the
/// equivalent toggle (on for Debug/ReleaseSafe-style builds, off for optimized
/// release builds).
pub const IS_DEBUG: bool = cfg!(debug_assertions);

/// The artifact we're producing. This can be used to determine if we're
/// building a standalone exe, an embedded lib, etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Artifact {
    /// Standalone executable
    Exe,

    /// Embeddable library
    Lib,

    /// The WASM-targeted module.
    WasmModule,
}

impl Artifact {
    /// Reference: `Artifact.detect()`.
    ///
    /// ```text
    /// if (builtin.target.cpu.arch.isWasm()) {
    ///     assert(builtin.output_mode == .Obj);
    ///     assert(builtin.link_mode == .Static);
    ///     return .wasm_module;
    /// }
    /// return switch (builtin.output_mode) {
    ///     .Exe => .exe,
    ///     .Lib => .lib,
    ///     else => { ... @compileError("unsupported artifact output mode"); },
    /// };
    /// ```
    ///
    /// In the Rust port there is no `builtin.output_mode`, so the artifact is
    /// determined by the compilation target plus the crate that hosts the
    /// build: a wasm target produces the wasm module; the `artifact-lib` cargo
    /// feature (enabled by `rustty-apprt-c`, the `librustty` cdylib/staticlib
    /// hub — the `libghostty` analog) selects the embeddable library; otherwise
    /// the default is the standalone executable (the `rustty` binary).
    ///
    /// CAVEAT (mirrors the `rustty-vt` `wuffs` seam): cargo features unify per
    /// BUILD SELECTION, not per artifact. A combined selection that contains
    /// both `rustty-apprt-c` and the `rustty` exe (e.g. `--workspace`) compiles
    /// a single `rustty-build-config` with `artifact-lib` on, so the exe also
    /// reports `.lib`. The only behavioral deltas that ride `artifact()` are
    /// additive (diagnostics precompute allocations; the macOS
    /// `RUSTTY_MAC_LAUNCH_SOURCE` probe in `rustty-os`, which falls through to
    /// the same `getppid()` check), so unification is a superset, never a
    /// behavior break. Published `librustty` artifacts come from isolated
    /// `-p rustty-apprt-c` invocations.
    pub const fn detect() -> Artifact {
        if cfg!(target_arch = "wasm32") {
            return Artifact::WasmModule;
        }
        if cfg!(feature = "artifact-lib") {
            return Artifact::Lib;
        }
        Artifact::Exe
    }
}

#[cfg(test)]
mod tests {
    /// S63-F2 / S64-F1: in a git checkout, with no `RUSTTY_VERSION_STRING`,
    /// the version is `X.Y.Z-<branch>+<short hash>` from `GitVersion.detect`
    /// (`Config.zig:332-338`), and a pre-release makes the channel `tip`
    /// (`Config.zig:693-701`). The expectation is computed here by running
    /// the same detection, so the test checks the build script's result
    /// reached the constants.
    #[test]
    fn version_is_derived_from_git() {
        if option_env!("RUSTTY_VERSION_STRING").is_some_and(|v| !v.is_empty()) {
            return; // the explicit arm, covered by `version`'s own tests
        }
        let core = crate::version::core_version(rustty_identity::VERSION);
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let Ok(v) = crate::git_version::detect(&root) else {
            assert_eq!(crate::VERSION_STRING, format!("{core}-dev+0000000"));
            return;
        };
        let expected = match v.tag.as_deref() {
            Some(t) if t != "tip" => core.to_string(),
            _ => format!("{core}-{}+{}", v.branch, v.short_hash),
        };
        assert_eq!(crate::VERSION_STRING, expected);
        assert_eq!(crate::VERSION, crate::VERSION_STRING);
        if expected.contains('-') {
            assert_eq!(crate::release_channel(), crate::ReleaseChannel::Tip);
        }
    }

    /// `GitVersion.zig:30-35` sanitizes the RAW `rev-parse` output, trailing
    /// newline included, so the branch ends in `-`. Measured on the
    /// reference: its cached build options carry
    /// `app_version_string = "1.3.2-HEAD-+4540d499a"`.
    #[test]
    fn git_branch_keeps_the_reference_trailing_dash() {
        assert_eq!(
            crate::git_version::sanitize_branch("feature/x_y.2\n"),
            "feature-x-y-2-"
        );
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        if let Ok(v) = crate::git_version::detect(&root) {
            assert!(v.branch.ends_with('-'), "{v:?}");
            assert!(!v.short_hash.is_empty());
        }
    }

    /// WS3-B: the toolchain version captured by build.rs. The workspace pins
    /// rustc via rust-toolchain.toml, so the string starts with a 1.x version
    /// and carries no "rustc " prefix (mirrors `builtin.zig_version_string`
    /// being a bare version string).
    #[test]
    fn rustc_version_is_captured() {
        assert!(
            crate::RUSTC_VERSION.starts_with("1."),
            "RUSTC_VERSION should be a bare version, got: {:?}",
            crate::RUSTC_VERSION
        );
        assert!(!crate::RUSTC_VERSION.contains("rustc "));
    }

    /// `mode_string` must report the ACTUAL build mode (parity S42-F3), not one
    /// of two hardcoded answers. It is asserted against the same two facts the
    /// function derives it from — `RUSTTY_OPT_LEVEL` and `debug_assertions` —
    /// so it is meaningful in every profile the workspace builds in, rather
    /// than pinning one literal that is only right in one of them.
    #[test]
    fn mode_string_reports_the_actual_optimize_mode() {
        let opt_level = env!("RUSTTY_OPT_LEVEL");
        let expected = match opt_level {
            "0" => "Debug",
            "s" | "z" => "ReleaseSmall",
            _ if cfg!(debug_assertions) => "ReleaseSafe",
            _ => "ReleaseFast",
        };
        assert_eq!(expected, crate::mode_string(), "opt-level {opt_level:?}");

        // All four Zig modes must be REACHABLE, i.e. the function must not be
        // able to collapse onto a subset. (This is the arm that was false
        // before: nothing could ever produce ReleaseSafe or ReleaseSmall.)
        for (level, debug_assertions, want) in [
            ("0", true, "Debug"),
            ("0", false, "Debug"),
            ("s", false, "ReleaseSmall"),
            ("z", false, "ReleaseSmall"),
            ("1", true, "ReleaseSafe"),
            ("2", true, "ReleaseSafe"),
            ("3", false, "ReleaseFast"),
            ("2", false, "ReleaseFast"),
        ] {
            assert_eq!(
                want,
                crate::mode_string_for(level, debug_assertions),
                "opt-level {level:?}, debug_assertions {debug_assertions}"
            );
        }
    }

    /// The packaging options are OPTIONS, not constants.
    ///
    /// Both were hardcoded `false` with no seam of any kind, so
    /// `-Dflatpak=true` — which the reference's own Flatpak manifest passes —
    /// had no rustty equivalent, and the Flatpak shipped with its portal/DBus
    /// integration, `flatpak-spawn --host` and host passwd lookup all inert.
    /// Asserting against `cfg!` (rather than a literal) makes this test
    /// meaningful in BOTH matrix positions: it fails if the feature stops
    /// driving the value in either direction, which is the only way the old
    /// bug could return.
    #[test]
    fn packaging_options_track_their_features() {
        assert_eq!(
            crate::flatpak(),
            cfg!(feature = "flatpak"),
            "build_config.flatpak must follow the `flatpak` feature"
        );
        assert_eq!(
            crate::snap(),
            cfg!(feature = "snap"),
            "build_config.snap must follow the `snap` feature"
        );
    }
}
