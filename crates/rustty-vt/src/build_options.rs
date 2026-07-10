//! rustty-vt build_options
//!
//! PORTS: ghostty src/terminal/build_options.zig
//!
//! PORT STATUS: done

// PORT NOTE: the reference imports are
//   const std = @import("std");
//
// The reference `build_options.zig` is the *build-time* helper consumed by
// `zig build.zig`: it defines the [`Options`] struct (the set of knobs the build
// graph chooses) plus [`Options::add`], which writes those knobs — together with
// a handful of *synthesized* ones (`kitty_graphics`, `tmux_control_mode`, the
// `version_*` fields) — into a generated `terminal_options` module that the rest
// of the `terminal` module then `@import`s as `build_options`.
//
// In the Rust port there is no Zig build graph and no generated module. The
// resolved option *values* that downstream gating code reads live in the sibling
// `options` module (the ported `terminal_options`). This module ports the
// build-time [`Options`] struct itself faithfully, and [`Options::add`] is ported
// as the pure transformation it performs: given an [`Options`] and the resolved
// target, it produces the synthesized [`Resolved`] option set (mirroring exactly
// the option keys/values the reference `add` writes into the generated module).

/// A semantic version `MAJOR.MINOR.PATCH` with optional `pre`/`build` suffixes,
/// mirroring the reference `std.SemanticVersion` fields used by [`Options`].
// PORT NOTE: the reference field type is `std.SemanticVersion`. We port the
// subset of fields that `Options::add` reads (`major`/`minor`/`patch`/`pre`/
// `build`) as a faithful local struct, matching the pattern used by
// `rustty-build-config`'s `SemanticVersion`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SemanticVersion {
    pub major: usize,
    pub minor: usize,
    pub patch: usize,
    pub pre: Option<&'static str>,
    pub build: Option<&'static str>,
}

impl SemanticVersion {
    /// Parse a `MAJOR.MINOR.PATCH[-PRE][+BUILD]` version string at compile
    /// time, mirroring `std.SemanticVersion.parse` — the call the reference
    /// build makes on the vt lib version string (`try
    /// std.SemanticVersion.parse(libVersion)`, src/build/Config.zig:307-309).
    /// Every call site is a `const` initializer of a compile-time identity
    /// constant, so a malformed version is a COMPILE error (const-eval panic) —
    /// the Rust analog of the reference's `try` failing the build. The
    /// reference's additional pre-release/build identifier charset validation
    /// (semver spec items 9/10) is not replicated for those fixed constants;
    /// the parsed values are pinned by tests.
    pub const fn parse(text: &'static str) -> SemanticVersion {
        let bytes = text.as_bytes();

        // const extra_index = std.mem.indexOfAny(u8, text, "-+");
        // const required = text[0..(extra_index orelse text.len)];
        let mut extra_index: Option<usize> = None;
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'-' || bytes[i] == b'+' {
                extra_index = Some(i);
                break;
            }
            i += 1;
        }
        let required_end = match extra_index {
            Some(idx) => idx,
            None => bytes.len(),
        };

        // var it = std.mem.splitScalar(u8, required, '.');
        // major/minor/patch = parseNum(...); a 4th component is InvalidVersion.
        let mut dot1: Option<usize> = None;
        let mut dot2: Option<usize> = None;
        let mut i = 0;
        while i < required_end {
            if bytes[i] == b'.' {
                match (dot1, dot2) {
                    (None, _) => dot1 = Some(i),
                    (_, None) => dot2 = Some(i),
                    // if (it.next() != null) return error.InvalidVersion;
                    _ => panic!("invalid semantic version: more than three numeric components"),
                }
            }
            i += 1;
        }
        let (d1, d2) = match (dot1, dot2) {
            (Some(a), Some(b)) => (a, b),
            // it.next() orelse return error.InvalidVersion
            _ => panic!("invalid semantic version: expected MAJOR.MINOR.PATCH"),
        };
        let major = parse_num(bytes, 0, d1);
        let minor = parse_num(bytes, d1 + 1, d2);
        let patch = parse_num(bytes, d2 + 1, required_end);

        // Slice optional pre-release or build metadata components.
        // const extra: []const u8 = text[extra_index.?..text.len];
        // if (extra[0] == '-') { pre = extra[1..build_index]; build = ... }
        // else { build = extra[1..]; }
        let mut pre: Option<&'static str> = None;
        let mut build: Option<&'static str> = None;
        if let Some(ei) = extra_index {
            if bytes[ei] == b'-' {
                // const build_index = std.mem.indexOfScalar(u8, extra, '+');
                let mut build_index: Option<usize> = None;
                let mut j = ei;
                while j < bytes.len() {
                    if bytes[j] == b'+' {
                        build_index = Some(j);
                        break;
                    }
                    j += 1;
                }
                match build_index {
                    Some(bi) => {
                        pre = Some(subslice(text, ei + 1, bi));
                        build = Some(subslice(text, bi + 1, bytes.len()));
                    }
                    None => pre = Some(subslice(text, ei + 1, bytes.len())),
                }
            } else {
                build = Some(subslice(text, ei + 1, bytes.len()));
            }
        }

        SemanticVersion {
            major,
            minor,
            patch,
            pre,
            build,
        }
    }
}

/// Parse one non-negative decimal component at compile time, mirroring the
/// reference's `parseNum` (`std.SemanticVersion.zig`): leading zeroes and
/// non-digit characters are `error.InvalidVersion`, which here is a const-eval
/// panic (a compile error, since every call site is a `const` initializer).
const fn parse_num(bytes: &[u8], start: usize, end: usize) -> usize {
    // if (text.len > 1 and text[0] == '0') return error.InvalidVersion;
    if end - start > 1 && bytes[start] == b'0' {
        panic!("invalid semantic version: leading zero in numeric component");
    }
    if end == start {
        panic!("invalid semantic version: empty numeric component");
    }
    let mut value: usize = 0;
    let mut i = start;
    while i < end {
        let c = bytes[i];
        if c < b'0' || c > b'9' {
            // error.InvalidCharacter => error.InvalidVersion
            panic!("invalid semantic version: non-digit in numeric component");
        }
        value = value * 10 + (c - b'0') as usize;
        i += 1;
    }
    value
}

/// Compile-time `text[start..end]` for the pre/build components (the Zig
/// slices `extra[1..build_index]` / `extra[(idx + 1)..]`). The bounds always
/// sit on the ASCII `-`/`+` delimiters found in `parse`, so the byte slice is
/// valid UTF-8 whenever the input string is; a failed re-validation is
/// unreachable and would be a compile error, never a runtime one.
const fn subslice(text: &'static str, start: usize, end: usize) -> &'static str {
    let (head, _) = text.as_bytes().split_at(end);
    let (_, mid) = head.split_at(start);
    match core::str::from_utf8(mid) {
        Ok(s) => s,
        Err(_) => panic!("invalid semantic version: non-UTF-8 component slice"),
    }
}

/// Options set by Zig build.zig and exposed via `terminal_options`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    /// The target artifact to build. This will gate some functionality.
    pub artifact: Artifact,

    /// Whether Oniguruma regex support is available. If this isn't
    /// available, some features will be disabled. This may be outdated,
    /// but the specific disabled features are:
    ///
    /// - Kitty graphics protocol
    /// - Tmux control mode
    ///
    pub oniguruma: bool,

    /// Whether to build SIMD-accelerated code paths. This pulls in more
    /// build-time dependencies and adds libc as a runtime dependency,
    /// but results in significant performance improvements.
    pub simd: bool,

    /// True if we should enable the "slow" runtime safety checks. These
    /// are runtime safety checks that are slower than typical and should
    /// generally be disabled in production builds.
    pub slow_runtime_safety: bool,

    /// Force C ABI mode on or off. If not set, then it will be set based on
    /// Options.
    pub c_abi: bool,

    /// The version of the application.
    pub version: SemanticVersion,
}

/// The target artifact to build.
// PORT NOTE: the reference declares this as a nested `pub const Artifact = enum`
// inside `Options`; Rust has no nested type declarations inside a struct, so it
// is hoisted to a sibling `pub enum` (referenced as `Options.Artifact` in Zig,
// here `build_options::Artifact`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Artifact {
    /// Ghostty application
    Ghostty,

    /// libghostty-vt, Zig module
    Lib,
}

/// The resolved terminal option set produced by [`Options::add`]: the [`Options`]
/// knobs plus the options synthesized from them and the build target. These are
/// the exact option keys the reference `add` writes into the generated
/// `terminal_options` module.
// PORT NOTE: the reference `add` writes its results into a `*std.Build.Module`'s
// generated options via `opts.addOption(...)`. There is no build module in the
// Rust port, so `add` returns this faithful value capturing every option key it
// writes, in the same order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolved {
    pub artifact: Artifact,
    pub c_abi: bool,
    pub oniguruma: bool,
    pub simd: bool,
    pub slow_runtime_safety: bool,
    pub kitty_graphics: bool,
    pub tmux_control_mode: bool,
    pub version_string: String,
    pub version_major: usize,
    pub version_minor: usize,
    pub version_patch: usize,
    pub version_pre: Option<&'static str>,
    pub version_build: Option<&'static str>,
}

/// The build target an [`Options::add`] call resolves its synthesized options
/// against.
// PORT NOTE: the reference reads `m.resolved_target.?.result` to learn the CPU
// arch / OS tag (used to disable kitty graphics on wasm32-freestanding). The Rust
// port has no build module, so `add` takes the resolved target explicitly via
// this faithful subset (the only fields `add` inspects).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Target {
    pub cpu_arch: CpuArch,
    pub os_tag: OsTag,
}

/// The CPU architecture of a build [`Target`], the subset [`Options::add`]
/// inspects. Mirrors the reference `target.cpu.arch` values it compares against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CpuArch {
    /// `.wasm32`
    Wasm32,
    /// Any other architecture (the reference compares only against `.wasm32`).
    Other,
}

/// The OS tag of a build [`Target`], the subset [`Options::add`] inspects.
/// Mirrors the reference `target.os.tag` values it compares against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OsTag {
    /// `.freestanding`
    Freestanding,
    /// Any other OS (the reference compares only against `.freestanding`).
    Other,
}

impl Options {
    /// Add the required build options for the terminal module.
    ///
    /// The memory referenced by self is expected to stick around (it isn't
    /// copied), since we expect we're in a build environment.
    // PORT NOTE: the reference signature is
    //   pub fn add(self: Options, b: *std.Build, m: *std.Build.Module) void
    // which appends options onto the build module. The Rust port has neither a
    // `*std.Build` nor a `*std.Build.Module`; the build module's only role is to
    // (a) supply `m.resolved_target.?.result` for the kitty_graphics computation
    // and (b) receive the synthesized options. We thread the resolved target in
    // via [`Target`] and return the synthesized [`Resolved`] set, preserving the
    // exact option keys/values and the order they are written.
    //
    // PORT NOTE: the method name `add` is kept verbatim from the reference
    // (`pub fn add`). clippy flags it as confusable with `std::ops::Add::add`;
    // we keep the reference name for parity and allow the lint here.
    #[allow(clippy::should_implement_trait)]
    pub fn add(self, target: Target) -> Resolved {
        // PORT NOTE: `b.fmt("{f}", .{self.version})` formats the semantic version
        // via `std.SemanticVersion`'s formatter. We reproduce that
        // `MAJOR.MINOR.PATCH[-PRE][+BUILD]` rendering directly.
        let version_string = format_version(&self.version);

        Resolved {
            artifact: self.artifact,
            c_abi: self.c_abi,
            oniguruma: self.oniguruma,
            simd: self.simd,
            slow_runtime_safety: self.slow_runtime_safety,

            // Kitty graphics is almost always true. This used to be conditional on
            // some other factors but we've since generalized the implementation
            // to support optional PNG decoding, OS capabilities like filesystems,
            // etc. So its safe to always enable it and just have the
            // implementation deal with unsupported features as needed.
            //
            // We disable it on wasm32-freestanding because we at the least
            // require the ability to get timestamps and there is no way to
            // do that with freestanding targets.
            kitty_graphics: !(target.cpu_arch == CpuArch::Wasm32
                && target.os_tag == OsTag::Freestanding),

            // These are synthesized based on other options.
            tmux_control_mode: self.oniguruma,

            // Version information.
            version_string,
            version_major: self.version.major,
            version_minor: self.version.minor,
            version_patch: self.version.patch,
            version_pre: self.version.pre,
            version_build: self.version.build,
        }
    }
}

/// Render a [`SemanticVersion`] as `MAJOR.MINOR.PATCH[-PRE][+BUILD]`, mirroring
/// `std.SemanticVersion`'s `{f}` formatter (the form `b.fmt("{f}", .{version})`
/// produces in the reference `add`).
fn format_version(version: &SemanticVersion) -> String {
    use core::fmt::Write as _;
    let mut s = String::new();
    let _ = write!(s, "{}.{}.{}", version.major, version.minor, version.patch);
    if let Some(pre) = version.pre {
        let _ = write!(s, "-{pre}");
    }
    if let Some(build) = version.build {
        let _ = write!(s, "+{build}");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_version() -> SemanticVersion {
        SemanticVersion {
            major: 1,
            minor: 2,
            patch: 3,
            pre: None,
            build: None,
        }
    }

    fn base_options() -> Options {
        Options {
            artifact: Artifact::Ghostty,
            oniguruma: true,
            simd: true,
            slow_runtime_safety: false,
            c_abi: false,
            version: base_version(),
        }
    }

    #[test]
    fn add_carries_through_knobs() {
        let resolved = base_options().add(Target {
            cpu_arch: CpuArch::Other,
            os_tag: OsTag::Other,
        });
        assert_eq!(Artifact::Ghostty, resolved.artifact);
        assert!(!resolved.c_abi);
        assert!(resolved.oniguruma);
        assert!(resolved.simd);
        assert!(!resolved.slow_runtime_safety);
    }

    #[test]
    fn kitty_graphics_enabled_on_normal_target() {
        let resolved = base_options().add(Target {
            cpu_arch: CpuArch::Other,
            os_tag: OsTag::Other,
        });
        assert!(resolved.kitty_graphics);
    }

    #[test]
    fn kitty_graphics_enabled_on_wasm32_non_freestanding() {
        let resolved = base_options().add(Target {
            cpu_arch: CpuArch::Wasm32,
            os_tag: OsTag::Other,
        });
        assert!(resolved.kitty_graphics);
    }

    #[test]
    fn kitty_graphics_enabled_on_freestanding_non_wasm32() {
        let resolved = base_options().add(Target {
            cpu_arch: CpuArch::Other,
            os_tag: OsTag::Freestanding,
        });
        assert!(resolved.kitty_graphics);
    }

    #[test]
    fn kitty_graphics_disabled_on_wasm32_freestanding() {
        let resolved = base_options().add(Target {
            cpu_arch: CpuArch::Wasm32,
            os_tag: OsTag::Freestanding,
        });
        assert!(!resolved.kitty_graphics);
    }

    #[test]
    fn tmux_control_mode_tracks_oniguruma_true() {
        let mut opts = base_options();
        opts.oniguruma = true;
        let resolved = opts.add(Target {
            cpu_arch: CpuArch::Other,
            os_tag: OsTag::Other,
        });
        assert!(resolved.tmux_control_mode);
    }

    #[test]
    fn tmux_control_mode_tracks_oniguruma_false() {
        let mut opts = base_options();
        opts.oniguruma = false;
        let resolved = opts.add(Target {
            cpu_arch: CpuArch::Other,
            os_tag: OsTag::Other,
        });
        assert!(!resolved.tmux_control_mode);
    }

    #[test]
    fn version_fields_are_synthesized() {
        let resolved = base_options().add(Target {
            cpu_arch: CpuArch::Other,
            os_tag: OsTag::Other,
        });
        assert_eq!("1.2.3", resolved.version_string);
        assert_eq!(1, resolved.version_major);
        assert_eq!(2, resolved.version_minor);
        assert_eq!(3, resolved.version_patch);
        assert_eq!(None, resolved.version_pre);
        assert_eq!(None, resolved.version_build);
    }

    #[test]
    fn version_string_renders_pre_and_build() {
        let mut opts = base_options();
        opts.version = SemanticVersion {
            major: 0,
            minor: 9,
            patch: 1,
            pre: Some("alpha.1"),
            build: Some("g1234abc"),
        };
        let resolved = opts.add(Target {
            cpu_arch: CpuArch::Other,
            os_tag: OsTag::Other,
        });
        assert_eq!("0.9.1-alpha.1+g1234abc", resolved.version_string);
        assert_eq!(Some("alpha.1"), resolved.version_pre);
        assert_eq!(Some("g1234abc"), resolved.version_build);
    }

    #[test]
    fn semantic_version_parse_mirrors_zig() {
        // The same shapes `std.SemanticVersion.parse` produces for these
        // inputs (see the reference tests in std/SemanticVersion.zig). The
        // first is the reference's vt lib version (`build.zig:10`).
        let v = SemanticVersion::parse("0.1.0-dev");
        assert_eq!((0, 1, 0), (v.major, v.minor, v.patch));
        assert_eq!(Some("dev"), v.pre);
        assert_eq!(None, v.build);

        let v = SemanticVersion::parse("1.3.2");
        assert_eq!((1, 3, 2), (v.major, v.minor, v.patch));
        assert_eq!(None, v.pre);
        assert_eq!(None, v.build);

        let v = SemanticVersion::parse("1.1.2-prerelease+meta");
        assert_eq!((1, 1, 2), (v.major, v.minor, v.patch));
        assert_eq!(Some("prerelease"), v.pre);
        assert_eq!(Some("meta"), v.build);

        let v = SemanticVersion::parse("10.20.30+build.4");
        assert_eq!((10, 20, 30), (v.major, v.minor, v.patch));
        assert_eq!(None, v.pre);
        assert_eq!(Some("build.4"), v.build);
    }
}
