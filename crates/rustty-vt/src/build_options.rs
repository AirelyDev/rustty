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

    /// Optional feature gates, all enabled by default. See Features.
    pub features: Features,

    /// The version of the application.
    pub version: SemanticVersion,
}

/// Optional features for lib artifacts. Disabling a feature removes
/// its C API exports and any stream-integrated handling from the
/// build, primarily so size-conscious embedders (e.g. wasm) can trim
/// the binary. The Ghostty application itself always builds with
/// every feature enabled.
///
/// Field names are the accepted tokens for `-Dvt-features`, which
/// acts like `-Dcpu`: comma-separated modifications applied on top
/// of the default (all enabled) set. `+feature` and `feature`
/// enable, `-feature` disables. The special name `all` refers to
/// every feature, so `-Dvt-features=-all,+render-state` builds only
/// the render state API. Hyphens in names are interchangeable with
/// underscores. Fields are emitted verbatim as bool options on the
/// `terminal_options` module.
// PORT NOTE: the reference declares this as `pub const Features = packed
// struct` nested inside `Options`; hoisted to a sibling type like `Artifact`
// above. The packed-struct backing integer only serves the test's
// `@bitCast(0)` all-false constructor, ported as [`Features::NONE`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Features {
    /// Terminal state serialization: encode a full terminal (screen
    /// contents, scrollback, cursor, styles, modes, etc.) to a
    /// compact binary format and decode it back into a live
    /// terminal. Used for session persistence, window restoration,
    /// and moving terminal state between processes.
    ///
    /// C API: `ghostty_snapshot_*` and, together with
    /// `grid_introspection`, `ghostty_tracked_grid_ref_snapshot`.
    pub snapshot: bool,

    /// Textual export of terminal contents as plain text, VT
    /// (replayable escape sequences), or HTML, with optional extras
    /// such as cursor position, palette, modes, and tabstops. Used
    /// for copy-to-clipboard, dump-to-file, and debugging.
    ///
    /// C API: `ghostty_formatter_*` and, together with `selection`,
    /// `ghostty_terminal_selection_format_*`.
    pub formatter: bool,

    /// Text selection: select word/line/output/all, selection
    /// ordering, adjustment, and containment math, and the
    /// click/drag gesture state machine (single/double/triple
    /// click behaviors). Selections are expressed in grid
    /// references, so enabling this also keeps the untracked grid
    /// reference constructors even if `grid_introspection` is
    /// disabled.
    ///
    /// C API: `ghostty_terminal_select_*`,
    /// `ghostty_terminal_selection_*`, `ghostty_selection_gesture_*`.
    pub selection: bool,

    /// Terminal search: find matches for a string in the active
    /// area and scrollback (ASCII case-insensitive), with results
    /// that survive primary/alternate screen switches, resize, and
    /// scrollback pruning, plus match selection with wrap-around
    /// and viewport scrolling. Used to implement find bars.
    ///
    /// C API: `ghostty_search_*`.
    // PORT NOTE (`32601cd79`, P6-W1 carries the build-options MODEL only): the
    // cargo `search` feature, `options::SEARCH`, and the `rustty_search_*`
    // exports it gates are the P6-W3 search-C-API workstream's.
    pub search: bool,

    /// The render state API: a coherent, update-in-place view of
    /// the visible screen (rows, cells, styles, cursor, colors,
    /// palette) designed to drive a renderer at frame rates. This
    /// is the primary read path for embedders that draw the
    /// terminal.
    ///
    /// C API: `ghostty_render_state_*`.
    pub render_state: bool,

    /// Encoding of host input events into the byte sequences a
    /// terminal application expects: keyboard input (including the
    /// Kitty keyboard protocol), mouse reporting, focus reporting,
    /// and (bracketed) paste. Any interactive embedder needs this;
    /// a read-only viewer does not.
    ///
    /// C API: `ghostty_key_event_*`, `ghostty_key_encoder_*`,
    /// `ghostty_mouse_event_*`, `ghostty_mouse_encoder_*`,
    /// `ghostty_focus_encode`, `ghostty_paste_*`.
    pub input_encode: bool,

    /// Color utilities: luminance, perceived luminance, and
    /// contrast math (e.g. for minimum-contrast rendering), color
    /// string parsing (`#rrggbb`, X11 color names, XParseColor
    /// `rgb:`/`rgbi:` device specs), the default 256-color palette
    /// and palette generation from configurable bases, and the X11
    /// color name table itself.
    ///
    /// C API: `ghostty_color_*`.
    pub color: bool,

    /// Direct inspection of grid contents at a position via grid
    /// references: cell and row data getters (codepoints,
    /// graphemes, style, hyperlink URI, wrap state, ...) plus
    /// tracked grid references that stay valid across terminal
    /// updates. Used for hyperlink hover/open, accessibility, and
    /// tests; deliberately not built for render loops (use
    /// `render_state` for that).
    ///
    /// C API: `ghostty_grid_ref_*`, `ghostty_tracked_grid_ref_*`,
    /// `ghostty_cell_get*`, `ghostty_row_get*`, and (shared with
    /// `selection`) `ghostty_terminal_grid_ref` /
    /// `ghostty_terminal_point_from_grid_ref`.
    pub grid_introspection: bool,

    /// The APC glyph protocol: a Ghostty extension that lets
    /// terminal applications register custom font glyphs (glyf
    /// outlines) for private-use-area codepoints. Embedders that
    /// don't render registered glyphs can disable this; the
    /// sequences are still consumed and safely ignored.
    ///
    /// No C API surface of its own; this gates the stream handling
    /// and glossary storage.
    pub glyph_protocol: bool,

    /// The Kitty graphics protocol: APC command parsing, image
    /// transmission and storage (PNG decoding via the sys
    /// interface), and placement tracking, plus the read APIs a
    /// renderer uses to draw placed images. Disabled sequences are
    /// still consumed and safely ignored.
    ///
    /// This requires the ability to get timestamps from the OS, so
    /// it is always disabled on freestanding targets (e.g.
    /// wasm32-freestanding) regardless of this setting.
    ///
    /// C API: `ghostty_kitty_graphics_*`.
    pub kitty_graphics: bool,
}

/// `error{UnknownFeature}` from [`Features::parse`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnknownFeature;

// PORT NOTE: the reference iterates `@typeInfo(Features).@"struct".fields` in
// `parse` and `Options.add` (`inline for`). Rust has no field reflection, so
// this macro is the single source for the field list, invoked wherever the
// reference does an `inline for` over the fields. Order matches the struct
// declaration (the reference's field order, which is also the option-emission
// order in `add`).
macro_rules! for_each_feature {
    ($m:ident, $features:expr, $($args:tt)*) => {{
        $m!($features, snapshot, $($args)*);
        $m!($features, formatter, $($args)*);
        $m!($features, selection, $($args)*);
        $m!($features, search, $($args)*);
        $m!($features, render_state, $($args)*);
        $m!($features, input_encode, $($args)*);
        $m!($features, color, $($args)*);
        $m!($features, grid_introspection, $($args)*);
        $m!($features, glyph_protocol, $($args)*);
        $m!($features, kitty_graphics, $($args)*);
    }};
}

impl Default for Features {
    // `.{}` — every field takes its `= true` default.
    fn default() -> Features {
        Features {
            snapshot: true,
            formatter: true,
            selection: true,
            search: true,
            render_state: true,
            input_encode: true,
            color: true,
            grid_introspection: true,
            glyph_protocol: true,
            kitty_graphics: true,
        }
    }
}

impl Features {
    /// The all-false feature set.
    // PORT NOTE: the reference test builds this via `@bitCast(0)` over the
    // packed struct's backing integer; Rust structs have no bit backing, so
    // the all-false value is a named const.
    pub const NONE: Features = Features {
        snapshot: false,
        formatter: false,
        selection: false,
        search: false,
        render_state: false,
        input_encode: false,
        color: false,
        grid_introspection: false,
        glyph_protocol: false,
        kitty_graphics: false,
    };

    // pub fn parse(list: []const u8) error{UnknownFeature}!Features {
    pub fn parse(list: &[u8]) -> Result<Features, UnknownFeature> {
        // Modifications apply on top of the default set.
        let mut result: Features = Features::default();

        // var it = std.mem.splitAny(u8, list, ", ");
        // while (it.next()) |raw| { ... }
        for raw in list.split(|&b| b == b',' || b == b' ') {
            if raw.is_empty() {
                continue;
            }

            let mut name = raw;
            let mut enable = true;
            match name[0] {
                b'+' => name = &name[1..],
                b'-' => {
                    enable = false;
                    name = &name[1..];
                }
                _ => {}
            }

            // A dangling sign with no name is malformed.
            if name.is_empty() {
                return Err(UnknownFeature);
            }

            // The special name "all" refers to every feature.
            if name == b"all" {
                macro_rules! set_all {
                    ($features:expr, $field:ident, $enable:expr) => {
                        $features.$field = $enable
                    };
                }
                for_each_feature!(set_all, result, enable);
                continue;
            }

            let mut found = false;
            macro_rules! set_named {
                ($features:expr, $field:ident, $name:expr, $enable:expr, $found:expr) => {
                    if eql_name($name, stringify!($field).as_bytes()) {
                        $features.$field = $enable;
                        $found = true;
                    }
                };
            }
            for_each_feature!(set_named, result, name, enable, found);
            if !found {
                return Err(UnknownFeature);
            }
        }

        Ok(result)
    }
}

/// Feature name equality where '-' in the input matches '_' in
/// the field name.
// fn eqlName(input: []const u8, name: []const u8) bool { ... }
fn eql_name(input: &[u8], name: &[u8]) -> bool {
    if input.len() != name.len() {
        return false;
    }
    for (&a, &b) in input.iter().zip(name.iter()) {
        let norm: u8 = if a == b'-' { b'_' } else { a };
        if norm != b {
            return false;
        }
    }
    true
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
    pub tmux_control_mode: bool,
    // Feature gates, emitted as flat bools (e.g. `options.snapshot`) in the
    // reference's feature loop, in field order (`1fdbb8c91`).
    pub snapshot: bool,
    pub formatter: bool,
    pub selection: bool,
    pub search: bool,
    pub render_state: bool,
    pub input_encode: bool,
    pub color: bool,
    pub grid_introspection: bool,
    pub glyph_protocol: bool,
    pub kitty_graphics: bool,
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
// arch / OS tag (used to disable kitty graphics on freestanding targets). The
// Rust port has no build module, so `add` takes the resolved target explicitly
// via this faithful subset (the fields `add` has inspected).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Target {
    pub cpu_arch: CpuArch,
    pub os_tag: OsTag,
}

/// The CPU architecture of a build [`Target`]. Mirrors the reference
/// `target.cpu.arch` values [`Options::kitty_graphics`] compared against
/// before `3376153a4` (it now inspects only the OS tag; the field is kept so a
/// caller can still describe a full target).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CpuArch {
    /// `.wasm32`
    Wasm32,
    /// Any other architecture (the reference compared only against `.wasm32`).
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
    /// Whether the Kitty graphics feature is effectively enabled for
    /// the given target. Kitty graphics requires the ability to get
    /// timestamps and there is no way to do that on freestanding
    /// targets, so it is always disabled there regardless of the
    /// feature setting.
    // PORT NOTE (`3376153a4`): the reference condition was
    // `target.cpu.arch == .wasm32 and target.os.tag == .freestanding`; native
    // freestanding libghostty-vt made it ANY freestanding OS. The arch is no
    // longer inspected (the `Target.cpu_arch` field stays for callers that
    // describe a full target; nothing here reads it any more).
    pub fn kitty_graphics(self, target: Target) -> bool {
        if target.os_tag == OsTag::Freestanding {
            return false;
        }
        self.features.kitty_graphics
    }

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

        // Feature gates, emitted as flat bools (e.g. `options.snapshot`).
        //
        // PORT NOTE: the reference's `inline for` reads each feature field,
        // applies the kitty_graphics freestanding override, and emits
        // `opts.addOption(bool, field.name, value)`. Ported field-by-field
        // (Rust has no field reflection); only kitty_graphics carries a
        // per-field override, exactly like the reference's `comptime
        // std.mem.eql` name check.
        let features = self.features;
        // Kitty graphics is force-disabled on some targets; see
        // kittyGraphics for details.
        let kitty_graphics = self.kitty_graphics(target);

        Resolved {
            artifact: self.artifact,
            c_abi: self.c_abi,
            oniguruma: self.oniguruma,
            simd: self.simd,
            slow_runtime_safety: self.slow_runtime_safety,

            // These are synthesized based on other options.
            tmux_control_mode: self.oniguruma,

            snapshot: features.snapshot,
            formatter: features.formatter,
            selection: features.selection,
            search: features.search,
            render_state: features.render_state,
            input_encode: features.input_encode,
            color: features.color,
            grid_introspection: features.grid_introspection,
            glyph_protocol: features.glyph_protocol,
            kitty_graphics,

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
            // features: .{} (the reference field default — all enabled)
            features: Features::default(),
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

    // PORT NOTE (`3376153a4`): this test was `kitty_graphics_enabled_on_
    // freestanding_non_wasm32` asserting `resolved.kitty_graphics` TRUE — the
    // reference's `kittyGraphics` compared `cpu.arch == .wasm32 and os.tag ==
    // .freestanding`, so a non-wasm freestanding target kept the feature. The
    // native freestanding libghostty-vt build changed that to ANY freestanding
    // OS, so the expectation is now FALSE and the test is renamed to say so.
    // These four are Rust-only tests of `Options::add`/`kitty_graphics` (the
    // reference has no test for `kittyGraphics`); they pin the exact truth
    // table of the reference's one-line condition.
    #[test]
    fn kitty_graphics_disabled_on_freestanding_non_wasm32() {
        let resolved = base_options().add(Target {
            cpu_arch: CpuArch::Other,
            os_tag: OsTag::Freestanding,
        });
        assert!(!resolved.kitty_graphics);
    }

    #[test]
    fn kitty_graphics_method_ignores_feature_on_freestanding() {
        // `kittyGraphics` is "always disabled there regardless of the feature
        // setting": the feature bit does not matter on a freestanding target.
        let mut opts = base_options();
        opts.features.kitty_graphics = true;
        assert!(!opts.kitty_graphics(Target {
            cpu_arch: CpuArch::Other,
            os_tag: OsTag::Freestanding,
        }));
        assert!(opts.kitty_graphics(Target {
            cpu_arch: CpuArch::Other,
            os_tag: OsTag::Other,
        }));
        opts.features.kitty_graphics = false;
        assert!(!opts.kitty_graphics(Target {
            cpu_arch: CpuArch::Other,
            os_tag: OsTag::Other,
        }));
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

    // test "parse" (nested in Features; discovered via the reference's
    // trailing `test { _ = Options.Features; }`)
    #[test]
    fn features_parse() {
        // const none: Features = @bitCast(
        //     @as(@typeInfo(Features).@"struct".backing_integer.?, 0),
        // );
        let none: Features = Features::NONE;

        // Empty input keeps the defaults.
        assert_eq!(Features::default(), Features::parse(b"").unwrap());

        // All three token forms, comma separated.
        assert_eq!(
            Features {
                snapshot: false,
                ..Features::default()
            },
            Features::parse(b"-snapshot").unwrap(),
        );
        assert_eq!(
            Features {
                snapshot: false,
                formatter: false,
                ..Features::default()
            },
            Features::parse(b"-snapshot,+render_state,-formatter,color").unwrap(),
        );

        // The special "all" name refers to every feature.
        assert_eq!(none, Features::parse(b"-all").unwrap());
        assert_eq!(Features::default(), Features::parse(b"-all,+all").unwrap());
        // var render_only = none;
        // render_only.render_state = true;
        let mut render_only = none;
        render_only.render_state = true;
        assert_eq!(render_only, Features::parse(b"-all,+render-state").unwrap());

        // Hyphens are interchangeable with underscores.
        assert_eq!(
            Features {
                input_encode: false,
                grid_introspection: false,
                ..Features::default()
            },
            Features::parse(b"-input-encode,-grid_introspection").unwrap(),
        );

        // Later tokens win.
        assert_eq!(
            Features::default(),
            Features::parse(b"-snapshot,+snapshot").unwrap(),
        );

        // Unknown names and dangling signs are errors.
        assert_eq!(Err(UnknownFeature), Features::parse(b"bogus"));
        assert_eq!(Err(UnknownFeature), Features::parse(b"-snapshot,-"));
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
