//! rustty-config::value_types
//!
//! PORTS: the config value-types defined *in-file* inside ghostty
//! `src/config/Config.zig` (i.e. the `pub const X = enum/struct/union/packed`
//! definitions that the giant `Config` struct composes). The C1 leaf value-types
//! that live in their own ghostty files (string/path/io/command/conditional/
//! RepeatableStringMap/ClipboardCodepointMap/...) are ported in their own modules
//! and reused here.
//!
//! PORT STATUS: C2 SKELETON. Every type is laid down byte-faithfully (exact enum
//! variants, struct/union fields, defaults, and doc comments per R1). The simple
//! enums are complete; the ~20 types with real `parseCLI`/`clone`/`equal`/
//! `formatEntry` carry their field/variant definitions plus method SIGNATURES as
//! `todo!()`. The method bodies + every `test {}` (R1, count is a contract) are
//! filled in the subsequent fill sub-batches under scoped G3b.
//!
//! Modeling follows the panel-signed-off Decision B: the four comptime passes
//! (PARSE/FORMAT/CLONE/EQUAL-DIFF) are threaded dynamically through the
//! rustty-args `Target` trait + per-type inherent impls; here we lay the data.

#![allow(dead_code)]
#![allow(clippy::enum_variant_names)]

use rustty_args::args::{
    parse_auto_struct, parse_bool, AutoStruct, Error, PackedStruct, ParseError, WHITESPACE,
};
use rustty_cfg_fmt::{
    format_entry_display, format_entry_packed_struct, format_entry_string, format_entry_void,
    FormatEntry, Writer,
};
// PORTS: `u21` (Zig's 21-bit codepoint type) -> `rustty_vt::page::u21` (= u32).
use rustty_vt::page::u21;

// PORT NOTE (R1): the value-type parse/format/clone/equal bodies below are the
// fill of the C2 skeleton (sub-batch). Each reproduces the reference method
// assertion-for-assertion; the four comptime passes are threaded via the
// rustty-args `Target`/`AutoStruct` traits and the rustty-cfg-fmt `FormatEntry`
// trait + standalone helpers, exactly as the leaf value-types (string/path/
// RepeatableStringMap/...) already do.

/// PORTS: `std.fs.max_path_bytes` — the platform maximum path length used to
/// size stack path buffers (4096 on Linux/macOS).
const MAX_PATH_BYTES: usize = 4096;

/// PORTS: `std.mem.trim(u8, s, " \t")` — the per-method whitespace trim used
/// throughout the reference value-type parsers.
fn trim_ws(s: &[u8]) -> &[u8] {
    let mut start = 0;
    let mut end = s.len();
    while start < end && (s[start] == b' ' || s[start] == b'\t') {
        start += 1;
    }
    while end > start && (s[end - 1] == b' ' || s[end - 1] == b'\t') {
        end -= 1;
    }
    &s[start..end]
}

/// PORTS: `std.mem.trim(u8, s, &std.ascii.whitespace)` — `std.ascii.whitespace`
/// is `{ ' ', '\t', '\n', '\r', 11, 12 }` (space, tab, LF, CR, VT, FF).
fn trim_ascii_ws(s: &[u8]) -> &[u8] {
    fn is_ws(c: u8) -> bool {
        matches!(c, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
    }
    let mut start = 0;
    let mut end = s.len();
    while start < end && is_ws(s[start]) {
        start += 1;
    }
    while end > start && is_ws(s[end - 1]) {
        end -= 1;
    }
    &s[start..end]
}

// ===========================================================================
// External value-types referenced by Config fields.
//
// PORT NOTE (R3/DI seam): some font-coupled types are not yet ported in their
// home crate (rustty-font). Per the font crate's DI design and the task's "model
// the data here, no config->font hard type-dep" guidance, those are modeled as
// local skeleton types carrying their reference field shape, to be wired to the
// real font types when those land. `MetricModifier` IS available
// (`rustty_font::metrics::Modifier`) and is reused directly.
// ===========================================================================

// const MetricModifier = fontpkg.Metrics.Modifier;
pub use rustty_font::metrics::Modifier as MetricModifier;

// pub const WindowPaddingBalance = @import("../renderer/size.zig").PaddingBalance;
pub use rustty_render_types::PaddingBalance as WindowPaddingBalance;

// const KeyRemapSet = @import("../input/key_mods.zig").RemapSet;
pub use rustty_input::key_mods::RemapSet as KeyRemapSet;

// PORT NOTE: `fontpkg.face.Variation` is not yet ported in rustty-font. Skeleton
// placeholder modeling `pub const Variation = struct { id: u32, value: f64 }`
// (ghostty src/font/face.zig:74). Wired to the real type when face.zig lands.
//
// The reference `Id` is `packed struct(u32) { d: u8, c: u8, b: u8, a: u8 }` with
// `init(v: *const [4]u8) = .{ .a = v[0], .b = v[1], .c = v[2], .d = v[3] }`. As a
// little-endian packed struct the in-memory `u32` is `a<<24 | b<<16 | c<<8 | d`
// (e.g. "wght" -> 0x77676874 = 2003265652). The skeleton models `id` as that
// `u32`; `face_variation_id_init`/`face_variation_id_str` reproduce `Id.init`/
// `Id.str` against that representation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FaceVariation {
    pub id: u32,
    pub value: f64,
}

/// PORTS: `fontpkg.face.Variation.Id.init(v: *const [4]u8)`.
fn face_variation_id_init(v: &[u8; 4]) -> u32 {
    // .{ .a = v[0], .b = v[1], .c = v[2], .d = v[3] } -> a<<24 | b<<16 | c<<8 | d
    (v[0] as u32) << 24 | (v[1] as u32) << 16 | (v[2] as u32) << 8 | (v[3] as u32)
}

/// PORTS: `fontpkg.face.Variation.Id.str(self) [4]u8`.
fn face_variation_id_str(id: u32) -> [u8; 4] {
    // return .{ self.a, self.b, self.c, self.d };
    [
        (id >> 24) as u8,
        (id >> 16) as u8,
        (id >> 8) as u8,
        id as u8,
    ]
}

// PORT NOTE: `fontpkg.CodepointMap` (`font/CodepointMap.zig`) is ported PURE in
// rustty-font; re-export it under the reference name. Its discovery-descriptor
// seam (`Descriptor`) carries `family` + `monospace` (the two fields config's
// `RepeatableCodepointMap.parseCLI` sets); the full fontconfig/CoreText discovery
// lands later in the shaper tier — rustty-font stays pure (no C/vt edge).
pub use rustty_font::codepoint_map::CodepointMap;

// ===========================================================================
// Bare-enum name maps (the PARSE/FORMAT name tables).
//
// PORTS: for the config enums WITHOUT a custom `parseCLI`, the reference parses
// them in `cli.args.parseIntoField` via `std.meta.stringToEnum(Field, value)`
// and formats them via `@tagName(self)`. Both use the Zig field-name spelling,
// which for hyphenated variants is the HYPHENATED literal (e.g. `new-tab`,
// `linear-corrected`, `16-bit`) — NOT the Rust ident. Rust has no enum
// reflection, so each bare enum gets an explicit name map generated by the
// `bare_enum_names!` macro below:
//
//   - `string_to_enum(&[u8]) -> Option<Self>`  ==  `std.meta.stringToEnum`
//   - `to_name(self) -> &'static [u8]`          ==  `@tagName(self)`
//
// The two maps are inverse over the SAME literal table, so PARSE and FORMAT are
// guaranteed symmetric (the reference's `stringToEnum`/`@tagName` derive from the
// same comptime field names). Exhaustiveness of `to_name` is enforced by the
// match having one arm per variant (a missing variant is a compile error).
// ===========================================================================

/// Copies `s` into a fixed `[u8; N]` whose tail byte stays `0` — the NUL
/// terminator. `N` must be `s.len() + 1`. This is the const builder behind the
/// `to_name_z` arms: the reference's `@tagName` is a `[:0]const u8` (a
/// sentinel-terminated string literal), so the C-API enum path needs a
/// NUL-terminated copy of the un-terminated `to_name` slice.
pub const fn name_z_array<const N: usize>(s: &[u8]) -> [u8; N] {
    let mut out = [0u8; N];
    let mut i = 0;
    while i < s.len() {
        out[i] = s[i];
        i += 1;
    }
    out
}

/// Generates `string_to_enum` (== `std.meta.stringToEnum`) and `to_name`
/// (== `@tagName`) for a bare config enum, over the exact Zig variant-name
/// literals. Each `Variant => b"name"` pair maps a Rust variant to its Zig
/// field-name spelling (hyphenated for hyphenated Zig variants).
macro_rules! bare_enum_names {
    ($ty:ty { $($variant:ident => $name:literal),+ $(,)? }) => {
        impl $ty {
            /// `@typeInfo(E).@"enum".fields |f| f.name` — the ordered list of Zig
            /// variant-name literals in declaration order. Feeds the shell-completion
            /// generators (`rustty-extra`): the `.@"enum"` arm of the reference's
            /// `writeBashCompletions`/`writeZshCompletions`/`writeCompletions` emits
            /// exactly these names, space-joined. The literal table is the SAME one
            /// `string_to_enum`/`to_name` use, so the completion bytes == `@tagName`.
            pub const VALUE_NAMES: &'static [&'static [u8]] = &[$($name),+];

            /// `std.meta.stringToEnum(Self, input)` — maps the Zig variant-name
            /// literal to the variant, `None` for an unknown name.
            pub fn string_to_enum(input: &[u8]) -> Option<$ty> {
                match input {
                    $($name => Some(<$ty>::$variant),)+
                    _ => None,
                }
            }

            /// `@tagName(self)` — the Zig variant-name literal for this variant.
            pub fn to_name(self) -> &'static [u8] {
                match self {
                    $(<$ty>::$variant => $name,)+
                }
            }

            /// `@tagName(self)` as a NUL-terminated `[*:0]const u8` for the C API
            /// (`c_get`'s `.@"enum"` arm). Each variant's name is materialized as a
            /// `'static` NUL-terminated array, matching the reference's sentinel
            /// `@tagName` pointer.
            pub fn to_name_z(self) -> *const std::os::raw::c_char {
                match self {
                    $(<$ty>::$variant => {
                        static Z: [u8; $name.len() + 1] =
                            $crate::value_types::name_z_array($name);
                        Z.as_ptr().cast()
                    })+
                }
            }
        }
    };
}

// ===========================================================================
// Simple enums (byte-faithful; complete).
// ===========================================================================

pub const CONFIRM_CLOSE_SURFACE_TAG: () = ();

// pub const ConfirmCloseSurface = enum(c_int) { false, true, always };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum ConfirmCloseSurface {
    False,
    True,
    Always,
}

/// Valid values for custom-shader-animation
/// c_int because it needs to be extern compatible
/// If this is changed, you must also update rustty-embed.h
// pub const CustomShaderAnimation = enum(c_int) { false, true, always };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum CustomShaderAnimation {
    False,
    True,
    Always,
}

/// Valid values for macos-non-native-fullscreen
/// c_int because it needs to be extern compatible
/// If this is changed, you must also update rustty-embed.h
// pub const NonNativeFullscreen = enum(c_int) { false, true, "visible-menu", "padded-notch" };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum NonNativeFullscreen {
    False,
    True,
    VisibleMenu,
    PaddedNotch,
}

/// Valid values for fullscreen config option
/// c_int because it needs to be extern compatible
/// If this is changed, you must also update rustty-embed.h
// pub const Fullscreen = enum(c_int) { false, true, "non-native", "non-native-visible-menu", "non-native-padded-notch" };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum Fullscreen {
    False,
    True,
    NonNative,
    NonNativeVisibleMenu,
    NonNativePaddedNotch,
}

// pub const WindowPaddingColor = enum { background, extend, "extend-always" };
//
// PORT NOTE (Chunk 3a A1 hoist — GPU_RD_CHUNK3A_DESIGN.md): named as a
// `renderer/generic.zig` `DerivedConfig` field type (`padding_color`); because
// rustty-config depends on rustty-renderer, the renderer cannot name a config type,
// so the enum + `DeepEqual` are hoisted to the rustty-render-types leaf (the
// `AlphaBlending` precedent). Re-exported here so `value_types::WindowPaddingColor`
// and every field/defaults/Key use-site keep resolving. The config-facing PARSE/
// FORMAT stays here as free fns in `config_struct.rs`
// (`window_padding_color_{string_to_enum,to_name,to_name_z}`).
pub use rustty_render_types::WindowPaddingColor;

// pub const WindowSubtitle = enum { false, "working-directory" };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowSubtitle {
    False,
    WorkingDirectory,
}

// pub const LinkPreviews = enum { false, true, osc8 };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkPreviews {
    False,
    True,
    Osc8,
}

/// See copy-on-select
// pub const CopyOnSelect = enum { false, true, clipboard };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyOnSelect {
    /// Disables copy on select entirely.
    False,
    /// Copy on select is enabled, but goes to the selection clipboard.
    /// This is not supported on platforms such as macOS. This is the default.
    True,
    /// Copy on select is enabled and goes to both the system clipboard
    /// and the selection clipboard (for Linux).
    Clipboard,
}

// pub const RightClickAction = enum { ignore, paste, copy, "copy-or-paste", "context-menu" };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RightClickAction {
    /// No action is taken on right-click.
    Ignore,
    /// Pastes from the system clipboard.
    Paste,
    /// Copies the selected text to the system clipboard.
    Copy,
    /// Copies the selected text to the system clipboard and
    /// pastes the clipboard if no text is selected.
    CopyOrPaste,
    /// Shows a context menu with options.
    ContextMenu,
}

/// Options for middle-click actions.
// pub const MiddleClickAction = enum { "primary-paste", ignore };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MiddleClickAction {
    /// Paste from the selection/standard clipboard per `copy-on-select`.
    PrimaryPaste,
    /// No action is taken on middle click.
    Ignore,
}

// pub const ShellIntegration = enum { none, detect, bash, elvish, fish, nushell, zsh };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellIntegration {
    None,
    Detect,
    Bash,
    Elvish,
    Fish,
    Nushell,
    Zsh,
}

/// OSC 4, 10, 11, and 12 default color reporting format.
// pub const OSCColorReportFormat = enum { none, "8-bit", "16-bit" };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OSCColorReportFormat {
    None,
    Bit8,
    Bit16,
}

// pub const WindowTheme = enum { auto, system, light, dark, ghostty };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowTheme {
    Auto,
    System,
    Light,
    Dark,
    Rustty,
}

// pub const WindowColorspace = enum { srgb, "display-p3" };
//
// PORT NOTE (Chunk 3a A1 hoist — GPU_RD_CHUNK3A_DESIGN.md): named as a
// `renderer/generic.zig` `DerivedConfig` field type (`colorspace`); hoisted to the
// rustty-render-types leaf (the `AlphaBlending` precedent) to break the
// config->renderer cycle. Re-exported here; the config-facing PARSE/FORMAT stays as
// free fns in `config_struct.rs` (`window_colorspace_{string_to_enum,to_name,to_name_z}`).
pub use rustty_render_types::WindowColorspace;

// pub const MacWindowButtons = enum { visible, hidden };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacWindowButtons {
    Visible,
    Hidden,
}

// pub const MacTitlebarStyle = enum { native, transparent, tabs, hidden };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacTitlebarStyle {
    Native,
    Transparent,
    Tabs,
    Hidden,
}

/// See macos-titlebar-proxy-icon
// pub const MacTitlebarProxyIcon = enum { visible, hidden };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacTitlebarProxyIcon {
    Visible,
    Hidden,
}

// pub const MacHidden = enum { never, always };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacHidden {
    Never,
    Always,
}

/// See macos-icon
///
/// Note: future versions of rustty can support a custom icon with
/// path by changing this to a tagged union, which doesn't change our
/// format at all.
// pub const MacAppIcon = enum { official, blueprint, chalkboard, microchip, glass, holographic, paper, retro, xray, custom, "custom-style" };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacAppIcon {
    Official,
    Blueprint,
    Chalkboard,
    Microchip,
    Glass,
    Holographic,
    Paper,
    Retro,
    Xray,
    Custom,
    CustomStyle,
}

// pub const MacAppIconFrame = enum { aluminum, beige, plastic, chrome };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacAppIconFrame {
    Aluminum,
    Beige,
    Plastic,
    Chrome,
}

// pub const MacShortcuts = enum { allow, deny, ask };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacShortcuts {
    Allow,
    Deny,
    Ask,
}

// pub const GtkSingleInstance = enum { false, true, detect; pub const default = .detect };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GtkSingleInstance {
    False,
    True,
    Detect,
}

impl GtkSingleInstance {
    // pub const default: GtkSingleInstance = .detect;
    pub const DEFAULT: GtkSingleInstance = GtkSingleInstance::Detect;
}

// pub const GtkTabsLocation = enum { top, bottom };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GtkTabsLocation {
    Top,
    Bottom,
}

// pub const GtkToolbarStyle = enum { flat, raised, "raised-border" };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GtkToolbarStyle {
    Flat,
    Raised,
    RaisedBorder,
}

// pub const GtkTitlebarStyle = enum(c_int) { native, tabs; pub const getGObjectType = ... };
//
// PORT NOTE: `getGObjectType` is a GTK-only comptime decl resolving to a gobject
// type registration or `void` based on `build_config.app_runtime`; it is
// reproduced at the apprt layer, not modeled as data here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum GtkTitlebarStyle {
    Native,
    Tabs,
}

// pub const MouseShiftCapture = enum { false, true, always, never };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseShiftCapture {
    False,
    True,
    Always,
    Never,
}

/// How to treat requests to write to or read from the clipboard
// pub const ClipboardAccess = enum { allow, deny, ask };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardAccess {
    Allow,
    Deny,
    Ask,
}

// pub const WindowSaveState = enum { default, never, always };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowSaveState {
    Default,
    Never,
    Always,
}

/// See window-new-tab-position
// pub const WindowNewTabPosition = enum { current, end };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowNewTabPosition {
    Current,
    End,
}

// pub const MacOSDockDropBehavior = enum { "new-tab", "new-window" };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacOSDockDropBehavior {
    NewTab,
    NewWindow,
}

// pub const WindowShowTabBar = enum { always, auto, never };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowShowTabBar {
    Always,
    Auto,
    Never,
}

// pub const ResizeOverlay = enum { always, never, "after-first" };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeOverlay {
    Always,
    Never,
    AfterFirst,
}

// pub const ResizeOverlayPosition = enum { center, "top-left", "top-center", "top-right", "bottom-left", "bottom-center", "bottom-right" };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeOverlayPosition {
    Center,
    TopLeft,
    TopCenter,
    TopRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

// pub const QuickTerminalPosition = enum { top, bottom, left, right, center };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuickTerminalPosition {
    Top,
    Bottom,
    Left,
    Right,
    Center,
}

// pub const QuickTerminalLayer = enum { overlay, top, bottom, background };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuickTerminalLayer {
    Overlay,
    Top,
    Bottom,
    Background,
}

// pub const QuickTerminalScreen = enum { main, mouse, "macos-menu-bar" };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuickTerminalScreen {
    Main,
    Mouse,
    MacosMenuBar,
}

// See quick-terminal-space-behavior
// pub const QuickTerminalSpaceBehavior = enum { remain, move };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuickTerminalSpaceBehavior {
    Remain,
    Move,
}

/// See quick-terminal-keyboard-interactivity
// pub const QuickTerminalKeyboardInteractivity = enum { none, "on-demand", exclusive };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuickTerminalKeyboardInteractivity {
    None,
    OnDemand,
    Exclusive,
}

// pub const GraphemeWidthMethod = enum { legacy, unicode };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphemeWidthMethod {
    Legacy,
    Unicode,
}

// pub const AlphaBlending = enum { native, linear, "linear-corrected"; pub fn isLinear };
//
// PORT NOTE (A1 hoist): DEFINED in ghostty `config/Config.zig`, but the renderer branches
// on `blending.isLinear()` to select the sRGB pixel format (Metal.zig). Because rustty-config
// depends on rustty-renderer, the renderer cannot name a config type; so the enum + `is_linear`
// + `DeepEqual` are hoisted to the rustty-render-types leaf (both config and renderer depend on
// it) — the `PaddingBalance`/`Size`/`Health` precedent. Re-exported here so `value_types::
// AlphaBlending`, the field/TypeId/defaults/Key use-sites, and the `alpha_blending_is_linear`
// test all keep resolving. The config-facing PARSE/FORMAT stays here as free fns in
// `config_struct.rs` (`alpha_blending_{string_to_enum,to_name,to_name_z}`), mirroring
// `window_padding_balance_*` (a config-local free fn over a foreign type is legal).
pub use rustty_render_types::AlphaBlending;

// pub const BackgroundImagePosition = enum { "top-left", "top-center", "top-right", "center-left", "center-center", "center-right", "bottom-left", "bottom-center", "bottom-right", center };
//
// PORT NOTE (Chunk 3a A1 hoist — GPU_RD_CHUNK3A_DESIGN.md): named as a
// `renderer/generic.zig` `DerivedConfig` field type (`bg_image_position`); hoisted to
// the rustty-render-types leaf (the `AlphaBlending` precedent) to break the
// config->renderer cycle. Re-exported here; the config-facing PARSE/FORMAT stays as
// free fns in `config_struct.rs`
// (`background_image_position_{string_to_enum,to_name,to_name_z}`).
pub use rustty_render_types::BackgroundImagePosition;

// pub const BackgroundImageFit = enum { contain, cover, stretch, none };
//
// PORT NOTE (Chunk 3a A1 hoist — GPU_RD_CHUNK3A_DESIGN.md): named as a
// `renderer/generic.zig` `DerivedConfig` field type (`bg_image_fit`); hoisted to the
// rustty-render-types leaf (the `AlphaBlending` precedent) to break the
// config->renderer cycle. Re-exported here; the config-facing PARSE/FORMAT stays as
// free fns in `config_struct.rs` (`background_image_fit_{string_to_enum,to_name,to_name_z}`).
pub use rustty_render_types::BackgroundImageFit;

// pub const LinuxCgroup = enum { never, always, "single-instance" };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinuxCgroup {
    Never,
    Always,
    SingleInstance,
}

// pub const AsyncBackend = enum { auto, epoll, io_uring };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsyncBackend {
    Auto,
    Epoll,
    IoUring,
}

// pub const AutoUpdate = enum { off, check, download };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoUpdate {
    Off,
    Check,
    Download,
}

// pub const WindowDecoration = enum(c_int) { auto, client, server, none; getGObjectType; parseCLI };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum WindowDecoration {
    Auto,
    Client,
    Server,
    None,
}

impl WindowDecoration {
    // Make this a valid gobject if we're in a GTK environment.
    // pub const getGObjectType = switch (build_config.app_runtime) { .gtk => ... };
    // PORT NOTE: `getGObjectType` is a GTK-only comptime decl; reproduced at apprt.

    /// `std.meta.stringToEnum(WindowDecoration, input)`.
    fn string_to_enum(input: &[u8]) -> Option<WindowDecoration> {
        match input {
            b"auto" => Some(WindowDecoration::Auto),
            b"client" => Some(WindowDecoration::Client),
            b"server" => Some(WindowDecoration::Server),
            b"none" => Some(WindowDecoration::None),
            _ => None,
        }
    }

    // pub fn parseCLI(input_: ?[]const u8) !WindowDecoration
    pub fn parse_cli(input_: Option<&[u8]>) -> Result<WindowDecoration, ParseError> {
        // const input = input_ orelse return .auto;
        let input = match input_ {
            Some(v) => v,
            None => return Ok(WindowDecoration::Auto),
        };

        // return if (cli.args.parseBool(input)) |b|
        //     if (b) .auto else .none
        // else |_| if (std.meta.stringToEnum(WindowDecoration, input)) |v|
        //     v
        // else
        //     error.InvalidValue;
        match parse_bool(input) {
            Ok(b) => Ok(if b {
                WindowDecoration::Auto
            } else {
                WindowDecoration::None
            }),
            Err(_) => match Self::string_to_enum(input) {
                Some(v) => Ok(v),
                None => Err(Error::InvalidValue),
            },
        }
    }

    /// `@tagName(self)` — the Zig variant-name literal. `WindowDecoration` has no
    /// `formatEntry` decl, so the generic `.@"enum"` arm prints `{t}` (this name).
    pub fn to_name(self) -> &'static [u8] {
        match self {
            WindowDecoration::Auto => b"auto",
            WindowDecoration::Client => b"client",
            WindowDecoration::Server => b"server",
            WindowDecoration::None => b"none",
        }
    }

    /// `@tagName(self)` as a NUL-terminated `[*:0]const u8` for the C API
    /// (`c_get`'s `.@"enum"` arm). `WindowDecoration` has a hand-written
    /// `to_name` (not via `bare_enum_names!`), so the terminated form is
    /// provided here too.
    pub fn to_name_z(self) -> *const std::os::raw::c_char {
        match self {
            WindowDecoration::Auto => c"auto".as_ptr(),
            WindowDecoration::Client => c"client".as_ptr(),
            WindowDecoration::Server => c"server".as_ptr(),
            WindowDecoration::None => c"none".as_ptr(),
        }
    }
}

// pub const Scrollbar = enum { system, never };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scrollbar {
    System,
    Never,
}

// pub const NotifyOnCommandFinish = enum { never, unfocused, always };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifyOnCommandFinish {
    Never,
    Unfocused,
    Always,
}

// ===========================================================================
// Bare-enum name maps (PARSE `stringToEnum` + FORMAT `@tagName`).
//
// One `bare_enum_names!` invocation per config enum that has NO custom
// `parseCLI`. The literal tables are the exact Zig field-name spellings (see
// each enum's `pub const X = enum { ... }` reference comment above). For an
// `enum(c_int)`, the discriminant order/values are fixed by the variant
// declaration order and are irrelevant to the name table.
// ===========================================================================

// pub const ConfirmCloseSurface = enum(c_int) { false, true, always };
bare_enum_names!(ConfirmCloseSurface {
    False => b"false",
    True => b"true",
    Always => b"always",
});

// pub const CustomShaderAnimation = enum(c_int) { false, true, always };
bare_enum_names!(CustomShaderAnimation {
    False => b"false",
    True => b"true",
    Always => b"always",
});

// pub const NonNativeFullscreen = enum(c_int) { false, true, "visible-menu", "padded-notch" };
bare_enum_names!(NonNativeFullscreen {
    False => b"false",
    True => b"true",
    VisibleMenu => b"visible-menu",
    PaddedNotch => b"padded-notch",
});

// pub const Fullscreen = enum(c_int) { false, true, "non-native", "non-native-visible-menu", "non-native-padded-notch" };
bare_enum_names!(Fullscreen {
    False => b"false",
    True => b"true",
    NonNative => b"non-native",
    NonNativeVisibleMenu => b"non-native-visible-menu",
    NonNativePaddedNotch => b"non-native-padded-notch",
});

// WindowPaddingColor: hoisted to rustty-render-types (Chunk 3a); its bare-enum
// parse/format is now config-local free fns in config_struct.rs
// (`window_padding_color_{string_to_enum,to_name,to_name_z}`).

// pub const WindowSubtitle = enum { false, "working-directory" };
bare_enum_names!(WindowSubtitle {
    False => b"false",
    WorkingDirectory => b"working-directory",
});

// pub const LinkPreviews = enum { false, true, osc8 };
bare_enum_names!(LinkPreviews {
    False => b"false",
    True => b"true",
    Osc8 => b"osc8",
});

// pub const CopyOnSelect = enum { false, true, clipboard };
bare_enum_names!(CopyOnSelect {
    False => b"false",
    True => b"true",
    Clipboard => b"clipboard",
});

// pub const RightClickAction = enum { ignore, paste, copy, "copy-or-paste", "context-menu" };
bare_enum_names!(RightClickAction {
    Ignore => b"ignore",
    Paste => b"paste",
    Copy => b"copy",
    CopyOrPaste => b"copy-or-paste",
    ContextMenu => b"context-menu",
});

// pub const MiddleClickAction = enum { "primary-paste", ignore };
bare_enum_names!(MiddleClickAction {
    PrimaryPaste => b"primary-paste",
    Ignore => b"ignore",
});

// pub const ShellIntegration = enum { none, detect, bash, elvish, fish, nushell, zsh };
bare_enum_names!(ShellIntegration {
    None => b"none",
    Detect => b"detect",
    Bash => b"bash",
    Elvish => b"elvish",
    Fish => b"fish",
    Nushell => b"nushell",
    Zsh => b"zsh",
});

// pub const OSCColorReportFormat = enum { none, "8-bit", "16-bit" };
bare_enum_names!(OSCColorReportFormat {
    None => b"none",
    Bit8 => b"8-bit",
    Bit16 => b"16-bit",
});

// pub const WindowTheme = enum { auto, system, light, dark, ghostty };
bare_enum_names!(WindowTheme {
    Auto => b"auto",
    System => b"system",
    Light => b"light",
    Dark => b"dark",
    Rustty => b"rustty",
});

// WindowColorspace: hoisted to rustty-render-types (Chunk 3a); its bare-enum
// parse/format is now config-local free fns in config_struct.rs
// (`window_colorspace_{string_to_enum,to_name,to_name_z}`).

// pub const MacWindowButtons = enum { visible, hidden };
bare_enum_names!(MacWindowButtons {
    Visible => b"visible",
    Hidden => b"hidden",
});

// pub const MacTitlebarStyle = enum { native, transparent, tabs, hidden };
bare_enum_names!(MacTitlebarStyle {
    Native => b"native",
    Transparent => b"transparent",
    Tabs => b"tabs",
    Hidden => b"hidden",
});

// pub const MacTitlebarProxyIcon = enum { visible, hidden };
bare_enum_names!(MacTitlebarProxyIcon {
    Visible => b"visible",
    Hidden => b"hidden",
});

// pub const MacHidden = enum { never, always };
bare_enum_names!(MacHidden {
    Never => b"never",
    Always => b"always",
});

// pub const MacAppIcon = enum { official, blueprint, chalkboard, microchip, glass, holographic, paper, retro, xray, custom, "custom-style" };
bare_enum_names!(MacAppIcon {
    Official => b"official",
    Blueprint => b"blueprint",
    Chalkboard => b"chalkboard",
    Microchip => b"microchip",
    Glass => b"glass",
    Holographic => b"holographic",
    Paper => b"paper",
    Retro => b"retro",
    Xray => b"xray",
    Custom => b"custom",
    CustomStyle => b"custom-style",
});

// pub const MacAppIconFrame = enum { aluminum, beige, plastic, chrome };
bare_enum_names!(MacAppIconFrame {
    Aluminum => b"aluminum",
    Beige => b"beige",
    Plastic => b"plastic",
    Chrome => b"chrome",
});

// pub const MacShortcuts = enum { allow, deny, ask };
bare_enum_names!(MacShortcuts {
    Allow => b"allow",
    Deny => b"deny",
    Ask => b"ask",
});

// pub const GtkSingleInstance = enum { false, true, detect };
bare_enum_names!(GtkSingleInstance {
    False => b"false",
    True => b"true",
    Detect => b"detect",
});

// pub const GtkTabsLocation = enum { top, bottom };
bare_enum_names!(GtkTabsLocation {
    Top => b"top",
    Bottom => b"bottom",
});

// pub const GtkToolbarStyle = enum { flat, raised, "raised-border" };
bare_enum_names!(GtkToolbarStyle {
    Flat => b"flat",
    Raised => b"raised",
    RaisedBorder => b"raised-border",
});

// pub const GtkTitlebarStyle = enum(c_int) { native, tabs };
bare_enum_names!(GtkTitlebarStyle {
    Native => b"native",
    Tabs => b"tabs",
});

// pub const MouseShiftCapture = enum { false, true, always, never };
bare_enum_names!(MouseShiftCapture {
    False => b"false",
    True => b"true",
    Always => b"always",
    Never => b"never",
});

// pub const ClipboardAccess = enum { allow, deny, ask };
bare_enum_names!(ClipboardAccess {
    Allow => b"allow",
    Deny => b"deny",
    Ask => b"ask",
});

// pub const WindowSaveState = enum { default, never, always };
bare_enum_names!(WindowSaveState {
    Default => b"default",
    Never => b"never",
    Always => b"always",
});

// pub const WindowNewTabPosition = enum { current, end };
bare_enum_names!(WindowNewTabPosition {
    Current => b"current",
    End => b"end",
});

// pub const MacOSDockDropBehavior = enum { "new-tab", "new-window" };
bare_enum_names!(MacOSDockDropBehavior {
    NewTab => b"new-tab",
    NewWindow => b"new-window",
});

// pub const WindowShowTabBar = enum { always, auto, never };
bare_enum_names!(WindowShowTabBar {
    Always => b"always",
    Auto => b"auto",
    Never => b"never",
});

// pub const ResizeOverlay = enum { always, never, "after-first" };
bare_enum_names!(ResizeOverlay {
    Always => b"always",
    Never => b"never",
    AfterFirst => b"after-first",
});

// pub const ResizeOverlayPosition = enum { center, "top-left", "top-center", "top-right", "bottom-left", "bottom-center", "bottom-right" };
bare_enum_names!(ResizeOverlayPosition {
    Center => b"center",
    TopLeft => b"top-left",
    TopCenter => b"top-center",
    TopRight => b"top-right",
    BottomLeft => b"bottom-left",
    BottomCenter => b"bottom-center",
    BottomRight => b"bottom-right",
});

// pub const QuickTerminalPosition = enum { top, bottom, left, right, center };
bare_enum_names!(QuickTerminalPosition {
    Top => b"top",
    Bottom => b"bottom",
    Left => b"left",
    Right => b"right",
    Center => b"center",
});

// pub const QuickTerminalLayer = enum { overlay, top, bottom, background };
bare_enum_names!(QuickTerminalLayer {
    Overlay => b"overlay",
    Top => b"top",
    Bottom => b"bottom",
    Background => b"background",
});

// pub const QuickTerminalScreen = enum { main, mouse, "macos-menu-bar" };
bare_enum_names!(QuickTerminalScreen {
    Main => b"main",
    Mouse => b"mouse",
    MacosMenuBar => b"macos-menu-bar",
});

// pub const QuickTerminalSpaceBehavior = enum { remain, move };
bare_enum_names!(QuickTerminalSpaceBehavior {
    Remain => b"remain",
    Move => b"move",
});

// pub const QuickTerminalKeyboardInteractivity = enum { none, "on-demand", exclusive };
bare_enum_names!(QuickTerminalKeyboardInteractivity {
    None => b"none",
    OnDemand => b"on-demand",
    Exclusive => b"exclusive",
});

// pub const GraphemeWidthMethod = enum { legacy, unicode };
bare_enum_names!(GraphemeWidthMethod {
    Legacy => b"legacy",
    Unicode => b"unicode",
});

// BackgroundImagePosition: hoisted to rustty-render-types (Chunk 3a); its bare-enum
// parse/format is now config-local free fns in config_struct.rs
// (`background_image_position_{string_to_enum,to_name,to_name_z}`).

// BackgroundImageFit: hoisted to rustty-render-types (Chunk 3a); its bare-enum
// parse/format is now config-local free fns in config_struct.rs
// (`background_image_fit_{string_to_enum,to_name,to_name_z}`).

// pub const LinuxCgroup = enum { never, always, "single-instance" };
bare_enum_names!(LinuxCgroup {
    Never => b"never",
    Always => b"always",
    SingleInstance => b"single-instance",
});

// pub const AsyncBackend = enum { auto, epoll, io_uring };
bare_enum_names!(AsyncBackend {
    Auto => b"auto",
    Epoll => b"epoll",
    IoUring => b"io_uring",
});

// pub const AutoUpdate = enum { off, check, download };
bare_enum_names!(AutoUpdate {
    Off => b"off",
    Check => b"check",
    Download => b"download",
});

// pub const Scrollbar = enum { system, never };
bare_enum_names!(Scrollbar {
    System => b"system",
    Never => b"never",
});

// pub const NotifyOnCommandFinish = enum { never, unfocused, always };
bare_enum_names!(NotifyOnCommandFinish {
    Never => b"never",
    Unfocused => b"unfocused",
    Always => b"always",
});

// ===========================================================================
// Packed structs (bitfield bools; byte-faithful field set + defaults).
// ===========================================================================

// pub const FontSyntheticStyle = packed struct { bold, italic, "bold-italic" = true };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FontSyntheticStyle {
    pub bold: bool,
    pub italic: bool,
    pub bold_italic: bool,
}

impl Default for FontSyntheticStyle {
    fn default() -> Self {
        Self {
            bold: true,
            italic: true,
            bold_italic: true,
        }
    }
}

/// See "font-shaping-break" for documentation
// pub const FontShapingBreak = packed struct { cursor: bool = true };
//
// PORT NOTE (Chunk 3a A1 hoist — GPU_RD_CHUNK3A_DESIGN.md): named as a
// `renderer/generic.zig` `DerivedConfig` field type (`font_shaping_break`); the struct
// + its `Default` (`cursor: true`) + `DeepEqual` are hoisted to the rustty-render-types
// leaf (the `AlphaBlending` precedent) to break the config->renderer cycle. Re-exported
// here so `value_types::FontShapingBreak` and the field/defaults/Key use-sites keep
// resolving. The config-facing machinery stays here as config-local free fns / a
// parse newtype (`FontShapingBreakParse` / `font_shaping_break_{to_c_uint,format_entry}`
// below) because `PackedStruct` (rustty-args) / `FormatEntry` (rustty-cfg-fmt) impls on
// the now-foreign type would violate the orphan rule.
pub use rustty_render_types::FontShapingBreak;

// pub const ShellIntegrationFeatures = packed struct { cursor, sudo, title, "ssh-env", "ssh-terminfo", path };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShellIntegrationFeatures {
    pub cursor: bool,
    pub sudo: bool,
    pub title: bool,
    pub ssh_env: bool,
    pub ssh_terminfo: bool,
    pub path: bool,
}

impl Default for ShellIntegrationFeatures {
    fn default() -> Self {
        Self {
            cursor: true,
            sudo: false,
            title: true,
            ssh_env: false,
            ssh_terminfo: false,
            path: true,
        }
    }
}

// pub const SplitPreserveZoom = packed struct { navigation: bool = false };
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SplitPreserveZoom {
    pub navigation: bool,
}

// pub const AppNotifications = packed struct { "clipboard-copy", "config-reload" = true };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppNotifications {
    pub clipboard_copy: bool,
    pub config_reload: bool,
}

impl Default for AppNotifications {
    fn default() -> Self {
        Self {
            clipboard_copy: true,
            config_reload: true,
        }
    }
}

/// See bell-features
// pub const BellFeatures = packed struct { system, audio, attention, title, border };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BellFeatures {
    pub system: bool,
    pub audio: bool,
    pub attention: bool,
    pub title: bool,
    pub border: bool,
}

impl Default for BellFeatures {
    fn default() -> Self {
        Self {
            system: false,
            audio: false,
            attention: true,
            title: true,
            border: false,
        }
    }
}

/// See freetype-load-flags
// pub const FreetypeLoadFlags = packed struct { hinting, "force-autohint", monochrome, autohint, light };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FreetypeLoadFlags {
    // The defaults here at the time of writing this match the defaults
    // for Freetype itself. rustty hasn't made any opinionated changes
    // to these defaults. (Strictly speaking, `light` isn't FreeType's
    // own default, but appears to be the effective default with most
    // Fontconfig-aware software using FreeType, so until rustty
    // implements Fontconfig support we default to `light`.)
    pub hinting: bool,
    pub force_autohint: bool,
    pub monochrome: bool,
    pub autohint: bool,
    pub light: bool,
}

impl Default for FreetypeLoadFlags {
    fn default() -> Self {
        Self {
            hinting: true,
            force_autohint: false,
            monochrome: false,
            autohint: true,
            light: true,
        }
    }
}

// pub const ScrollToBottom = packed struct { keystroke = true, output = false; pub const default = .{} };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollToBottom {
    pub keystroke: bool,
    pub output: bool,
}

impl Default for ScrollToBottom {
    fn default() -> Self {
        Self {
            keystroke: true,
            output: false,
        }
    }
}

impl ScrollToBottom {
    // pub const default: ScrollToBottom = .{};
    pub fn default_const() -> ScrollToBottom {
        ScrollToBottom::default()
    }
}

/// See notify-on-command-finish
// pub const NotifyOnCommandFinishAction = packed struct { bell = true, notify = false };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotifyOnCommandFinishAction {
    pub bell: bool,
    pub notify: bool,
}

impl Default for NotifyOnCommandFinishAction {
    fn default() -> Self {
        Self {
            bell: true,
            notify: false,
        }
    }
}

// ===========================================================================
// PackedStruct impls (the PARSE `parsePackedStruct` contract for the bool-only
// packed structs). `set_all(b)` sets every field to `b` (the "true"/"false"
// whole-struct shorthand); `set_field(part, value)` sets one named field by its
// Zig field name (hyphenated for hyphenated fields), returning whether a field
// matched. These reproduce the reference's `inline for (info.fields) |field| {
// assert(field.type == bool); @field(result, field.name) = ... }`.
// ===========================================================================

// packed struct { bold, italic, "bold-italic" }
impl PackedStruct for FontSyntheticStyle {
    fn field_names() -> &'static [&'static [u8]] {
        &[b"bold", b"italic", b"bold-italic"]
    }
    fn set_all(&mut self, b: bool) {
        self.bold = b;
        self.italic = b;
        self.bold_italic = b;
    }
    fn set_field(&mut self, part: &[u8], value: bool) -> bool {
        match part {
            b"bold" => self.bold = value,
            b"italic" => self.italic = value,
            b"bold-italic" => self.bold_italic = value,
            _ => return false,
        }
        true
    }
}

// packed struct { cursor }
//
// PORT NOTE (Chunk 3a): `FontShapingBreak` is hoisted to rustty-render-types, so
// `impl PackedStruct for FontShapingBreak` (a rustty-args trait on a now-foreign type)
// would violate the orphan rule. This config-local NEWTYPE carries the `PackedStruct`
// contract (a local type -> legal) so the single ported `parse_packed_struct` is reused
// verbatim; the parse handler unwraps `.0`. Pure cycle-break glue (R1 no-loss).
#[derive(Default)]
pub(crate) struct FontShapingBreakParse(pub FontShapingBreak);

impl PackedStruct for FontShapingBreakParse {
    fn field_names() -> &'static [&'static [u8]] {
        &[b"cursor"]
    }
    fn set_all(&mut self, b: bool) {
        self.0.cursor = b;
    }
    fn set_field(&mut self, part: &[u8], value: bool) -> bool {
        match part {
            b"cursor" => self.0.cursor = value,
            _ => return false,
        }
        true
    }
}

// packed struct { cursor, sudo, title, "ssh-env", "ssh-terminfo", path }
impl PackedStruct for ShellIntegrationFeatures {
    fn field_names() -> &'static [&'static [u8]] {
        &[
            b"cursor",
            b"sudo",
            b"title",
            b"ssh-env",
            b"ssh-terminfo",
            b"path",
        ]
    }
    fn set_all(&mut self, b: bool) {
        self.cursor = b;
        self.sudo = b;
        self.title = b;
        self.ssh_env = b;
        self.ssh_terminfo = b;
        self.path = b;
    }
    fn set_field(&mut self, part: &[u8], value: bool) -> bool {
        match part {
            b"cursor" => self.cursor = value,
            b"sudo" => self.sudo = value,
            b"title" => self.title = value,
            b"ssh-env" => self.ssh_env = value,
            b"ssh-terminfo" => self.ssh_terminfo = value,
            b"path" => self.path = value,
            _ => return false,
        }
        true
    }
}

// packed struct { navigation }
impl PackedStruct for SplitPreserveZoom {
    fn field_names() -> &'static [&'static [u8]] {
        &[b"navigation"]
    }
    fn set_all(&mut self, b: bool) {
        self.navigation = b;
    }
    fn set_field(&mut self, part: &[u8], value: bool) -> bool {
        match part {
            b"navigation" => self.navigation = value,
            _ => return false,
        }
        true
    }
}

// packed struct { "clipboard-copy", "config-reload" }
impl PackedStruct for AppNotifications {
    fn field_names() -> &'static [&'static [u8]] {
        &[b"clipboard-copy", b"config-reload"]
    }
    fn set_all(&mut self, b: bool) {
        self.clipboard_copy = b;
        self.config_reload = b;
    }
    fn set_field(&mut self, part: &[u8], value: bool) -> bool {
        match part {
            b"clipboard-copy" => self.clipboard_copy = value,
            b"config-reload" => self.config_reload = value,
            _ => return false,
        }
        true
    }
}

// packed struct { system, audio, attention, title, border }
impl PackedStruct for BellFeatures {
    fn field_names() -> &'static [&'static [u8]] {
        &[b"system", b"audio", b"attention", b"title", b"border"]
    }
    fn set_all(&mut self, b: bool) {
        self.system = b;
        self.audio = b;
        self.attention = b;
        self.title = b;
        self.border = b;
    }
    fn set_field(&mut self, part: &[u8], value: bool) -> bool {
        match part {
            b"system" => self.system = value,
            b"audio" => self.audio = value,
            b"attention" => self.attention = value,
            b"title" => self.title = value,
            b"border" => self.border = value,
            _ => return false,
        }
        true
    }
}

// packed struct { hinting, "force-autohint", monochrome, autohint, light }
impl PackedStruct for FreetypeLoadFlags {
    fn field_names() -> &'static [&'static [u8]] {
        &[
            b"hinting",
            b"force-autohint",
            b"monochrome",
            b"autohint",
            b"light",
        ]
    }
    fn set_all(&mut self, b: bool) {
        self.hinting = b;
        self.force_autohint = b;
        self.monochrome = b;
        self.autohint = b;
        self.light = b;
    }
    fn set_field(&mut self, part: &[u8], value: bool) -> bool {
        match part {
            b"hinting" => self.hinting = value,
            b"force-autohint" => self.force_autohint = value,
            b"monochrome" => self.monochrome = value,
            b"autohint" => self.autohint = value,
            b"light" => self.light = value,
            _ => return false,
        }
        true
    }
}

// packed struct { keystroke, output }
impl PackedStruct for ScrollToBottom {
    fn field_names() -> &'static [&'static [u8]] {
        &[b"keystroke", b"output"]
    }
    fn set_all(&mut self, b: bool) {
        self.keystroke = b;
        self.output = b;
    }
    fn set_field(&mut self, part: &[u8], value: bool) -> bool {
        match part {
            b"keystroke" => self.keystroke = value,
            b"output" => self.output = value,
            _ => return false,
        }
        true
    }
}

// packed struct { bell, notify }
impl PackedStruct for NotifyOnCommandFinishAction {
    fn field_names() -> &'static [&'static [u8]] {
        &[b"bell", b"notify"]
    }
    fn set_all(&mut self, b: bool) {
        self.bell = b;
        self.notify = b;
    }
    fn set_field(&mut self, part: &[u8], value: bool) -> bool {
        match part {
            b"bell" => self.bell = value,
            b"notify" => self.notify = value,
            _ => return false,
        }
        true
    }
}

// ===========================================================================
// Packed-struct C-API bit projection (`c_get`'s `.@"struct"` packed arm).
//
// PORTS: `getValue`'s packed-struct branch — `ptr.* = @intCast(@as(Backing,
// @bitCast(value)))`. Zig `@bitCast` of a `packed struct` reads the fields
// LSB-first in declaration order into the backing integer. Rust structs are not
// bit-packed, so `to_c_uint` reconstructs that backing integer by OR-ing each
// `bool` field at its declaration-order bit (the same field order as `set_all`).
// All these packed structs have <= `c_uint` bits, so the size guard in the
// reference always passes.
// ===========================================================================
macro_rules! packed_to_c_uint {
    ($ty:ty { $($field:ident),+ $(,)? }) => {
        impl $ty {
            /// `@intCast(@as(Backing, @bitCast(self)))` — the packed struct's
            /// backing-integer bit pattern for the C API.
            pub fn to_c_uint(&self) -> std::os::raw::c_uint {
                let mut bits: std::os::raw::c_uint = 0;
                let mut shift: u32 = 0;
                $(
                    bits |= (self.$field as std::os::raw::c_uint) << shift;
                    shift += 1;
                )+
                let _ = shift;
                bits
            }
        }
    };
}

packed_to_c_uint!(FontSyntheticStyle {
    bold,
    italic,
    bold_italic
});
// PORT NOTE (Chunk 3a): `FontShapingBreak` is hoisted to rustty-render-types, so the
// `packed_to_c_uint!`-generated inherent method would orphan on the foreign type. It
// becomes this config-local free fn (the `@bitCast` packed-struct backing-integer
// projection for `c_get`'s `.@"struct"` packed arm); the `g_packed_fn!` C-API arm calls
// it. Same bit layout as `packed_to_c_uint!` (declaration-order OR of each `bool`).
pub(crate) fn font_shaping_break_to_c_uint(v: FontShapingBreak) -> std::os::raw::c_uint {
    let mut bits: std::os::raw::c_uint = 0;
    let mut shift: u32 = 0;
    bits |= (v.cursor as std::os::raw::c_uint) << shift;
    shift += 1;
    let _ = shift;
    bits
}
packed_to_c_uint!(ShellIntegrationFeatures {
    cursor,
    sudo,
    title,
    ssh_env,
    ssh_terminfo,
    path
});
packed_to_c_uint!(SplitPreserveZoom { navigation });
packed_to_c_uint!(AppNotifications {
    clipboard_copy,
    config_reload
});
packed_to_c_uint!(BellFeatures {
    system,
    audio,
    attention,
    title,
    border
});
packed_to_c_uint!(FreetypeLoadFlags {
    hinting,
    force_autohint,
    monochrome,
    autohint,
    light
});
packed_to_c_uint!(ScrollToBottom { keystroke, output });
packed_to_c_uint!(NotifyOnCommandFinishAction { bell, notify });

// ===========================================================================
// Packed-struct `FormatEntry` impls.
//
// PORT NOTE: the reference has NO `formatEntry` decl on these packed structs;
// they are formatted by the generic `formatter.formatEntry`'s `.@"packed"` arm
// (`format_entry_packed_struct`), which writes the fields comma-separated in
// DECLARATION ORDER with a `no-` prefix for each `false` field, using the Zig
// field-name spelling (hyphenated where hyphenated). Rust has no struct
// reflection, so each impl supplies that ordered `(name, bool)` list explicitly
// — exactly the `inline for (info.fields)` the reference walks. The names match
// the `PackedStruct::set_field` arms above (the parse side), keeping the two
// directions byte-symmetric.
// ===========================================================================

// packed struct { bold, italic, "bold-italic" }
impl FormatEntry for FontSyntheticStyle {
    fn format_entry<W: Writer>(&self, name: &[u8], writer: &mut W) -> Result<(), W::Error> {
        format_entry_packed_struct(
            name,
            &[
                (b"bold", self.bold),
                (b"italic", self.italic),
                (b"bold-italic", self.bold_italic),
            ],
            writer,
        )
    }
}

// packed struct { cursor }
//
// PORT NOTE (Chunk 3a): `FontShapingBreak` is hoisted to rustty-render-types, so
// `impl FormatEntry for FontShapingBreak` (a rustty-cfg-fmt trait on a now-foreign
// type) would orphan. It becomes this config-local free fn; the `f_entry!(field, fn)`
// arm calls it. Body identical to the former `FormatEntry` impl.
pub(crate) fn font_shaping_break_format_entry<W: Writer>(
    v: FontShapingBreak,
    name: &[u8],
    writer: &mut W,
) -> Result<(), W::Error> {
    format_entry_packed_struct(name, &[(b"cursor", v.cursor)], writer)
}

// packed struct { cursor, sudo, title, "ssh-env", "ssh-terminfo", path }
impl FormatEntry for ShellIntegrationFeatures {
    fn format_entry<W: Writer>(&self, name: &[u8], writer: &mut W) -> Result<(), W::Error> {
        format_entry_packed_struct(
            name,
            &[
                (b"cursor", self.cursor),
                (b"sudo", self.sudo),
                (b"title", self.title),
                (b"ssh-env", self.ssh_env),
                (b"ssh-terminfo", self.ssh_terminfo),
                (b"path", self.path),
            ],
            writer,
        )
    }
}

// packed struct { navigation }
impl FormatEntry for SplitPreserveZoom {
    fn format_entry<W: Writer>(&self, name: &[u8], writer: &mut W) -> Result<(), W::Error> {
        format_entry_packed_struct(name, &[(b"navigation", self.navigation)], writer)
    }
}

// packed struct { "clipboard-copy", "config-reload" }
impl FormatEntry for AppNotifications {
    fn format_entry<W: Writer>(&self, name: &[u8], writer: &mut W) -> Result<(), W::Error> {
        format_entry_packed_struct(
            name,
            &[
                (b"clipboard-copy", self.clipboard_copy),
                (b"config-reload", self.config_reload),
            ],
            writer,
        )
    }
}

// packed struct { system, audio, attention, title, border }
impl FormatEntry for BellFeatures {
    fn format_entry<W: Writer>(&self, name: &[u8], writer: &mut W) -> Result<(), W::Error> {
        format_entry_packed_struct(
            name,
            &[
                (b"system", self.system),
                (b"audio", self.audio),
                (b"attention", self.attention),
                (b"title", self.title),
                (b"border", self.border),
            ],
            writer,
        )
    }
}

// packed struct { hinting, "force-autohint", monochrome, autohint, light }
impl FormatEntry for FreetypeLoadFlags {
    fn format_entry<W: Writer>(&self, name: &[u8], writer: &mut W) -> Result<(), W::Error> {
        format_entry_packed_struct(
            name,
            &[
                (b"hinting", self.hinting),
                (b"force-autohint", self.force_autohint),
                (b"monochrome", self.monochrome),
                (b"autohint", self.autohint),
                (b"light", self.light),
            ],
            writer,
        )
    }
}

// packed struct { keystroke, output }
impl FormatEntry for ScrollToBottom {
    fn format_entry<W: Writer>(&self, name: &[u8], writer: &mut W) -> Result<(), W::Error> {
        format_entry_packed_struct(
            name,
            &[(b"keystroke", self.keystroke), (b"output", self.output)],
            writer,
        )
    }
}

// packed struct { bell, notify }
impl FormatEntry for NotifyOnCommandFinishAction {
    fn format_entry<W: Writer>(&self, name: &[u8], writer: &mut W) -> Result<(), W::Error> {
        format_entry_packed_struct(
            name,
            &[(b"bell", self.bell), (b"notify", self.notify)],
            writer,
        )
    }
}

// ===========================================================================
// Unions (tagged) with method sigs.
// ===========================================================================

/// See working-directory
// pub const WorkingDirectory = union(enum) { home, inherit, path: []const u8 };
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkingDirectory {
    /// Resolve to the current user's home directory during config finalize.
    Home,
    /// Inherit the working directory from the launching process.
    Inherit,
    /// Use an explicit working directory path. This may be not be
    /// expanded until finalize is called.
    Path(Vec<u8>),
}

impl WorkingDirectory {
    // pub fn parseCLI(self: *Self, alloc: Allocator, input_: ?[]const u8) !void
    pub fn parse_cli(&mut self, input_: Option<&[u8]>) -> Result<(), ParseError> {
        // var input = input_ orelse return error.ValueRequired;
        let mut input = input_.ok_or(Error::ValueRequired)?;
        // input = std.mem.trim(u8, input, &std.ascii.whitespace);
        input = trim_ascii_ws(input);
        // if (input.len == 0) return error.ValueRequired;
        if input.is_empty() {
            return Err(Error::ValueRequired);
        }

        // Match path.zig behavior for quoted values.
        // if (input.len >= 2 and input[0] == '"' and input[input.len - 1] == '"') {
        if input.len() >= 2 && input[0] == b'"' && input[input.len() - 1] == b'"' {
            // input = input[1 .. input.len - 1];
            input = &input[1..input.len() - 1];
        }

        // if (std.mem.eql(u8, input, "home")) {
        if input == b"home" {
            // self.* = .home;
            *self = WorkingDirectory::Home;
            // return;
            return Ok(());
        }

        // if (std.mem.eql(u8, input, "inherit")) {
        if input == b"inherit" {
            // self.* = .inherit;
            *self = WorkingDirectory::Inherit;
            // return;
            return Ok(());
        }

        // self.* = .{ .path = try alloc.dupe(u8, input) };
        *self = WorkingDirectory::Path(input.to_vec());
        Ok(())
    }

    /// Expand tilde paths in .path values.
    // pub fn finalize(self: *Self, alloc: Allocator) Allocator.Error!void
    pub fn finalize(&mut self) {
        // const path = switch (self.*) {
        //     .path => |path| path,
        //     else => return,
        // };
        let path = match self {
            WorkingDirectory::Path(path) => path.clone(),
            _ => return,
        };

        // if (!std.mem.startsWith(u8, path, "~/")) return;
        if !path.starts_with(b"~/") {
            return;
        }

        // WS14 T6 (wasm32 arm): a browser has no home directory to expand `~/`
        // against, so wasm returns exactly as the `expandHome` error arm below does.
        // The reference never analyses this on wasm (Zig analyses `config.zig.Wasm`
        // lazily, so it never forces `internal_os.expandHome`); Rust compiles whole
        // crates, so the arm has to exist.
        #[cfg(target_family = "wasm")]
        return;

        // var buf: [std.fs.max_path_bytes]u8 = undefined;
        // PORT NOTE: `std.fs.max_path_bytes` is the platform max path length
        // (4096 on Linux/macOS); we size the stack buffer the same.
        #[cfg(not(target_family = "wasm"))]
        let mut buf = [0u8; MAX_PATH_BYTES];
        // const expanded = internal_os.expandHome(path, &buf) catch |err| {
        //     log.warn(...);
        //     return;
        // };
        #[cfg(not(target_family = "wasm"))]
        {
            let expanded = match rustty_os::homedir::expandHome(
                &rustty_global::environ_map(),
                &path,
                &mut buf,
            ) {
                Ok(v) => v,
                Err(_err) => {
                    // log.warn("error expanding home directory for working-directory path={s}: {}", ...);
                    return;
                }
            };

            // if (std.mem.eql(u8, expanded, path)) return;
            if expanded == &path[..] {
                return;
            }
            // self.* = .{ .path = try alloc.dupe(u8, expanded) };
            *self = WorkingDirectory::Path(expanded.to_vec());
        }
    }

    // pub fn value(self: Self) ?[]const u8
    pub fn value(&self) -> Option<&[u8]> {
        // return switch (self) {
        //     .path => |path| path,
        //     .home, .inherit => null,
        // };
        match self {
            WorkingDirectory::Path(path) => Some(path),
            WorkingDirectory::Home | WorkingDirectory::Inherit => None,
        }
    }

    // pub fn clone(self: Self, alloc: Allocator) Allocator.Error!Self
    pub fn clone_in(&self) -> WorkingDirectory {
        // return switch (self) {
        //     .path => |path| .{ .path = try alloc.dupe(u8, path) },
        //     else => self,
        // };
        match self {
            WorkingDirectory::Path(path) => WorkingDirectory::Path(path.clone()),
            _ => self.clone(),
        }
    }

    // pub fn formatEntry(self: Self, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry<W: Writer>(&self, name: &[u8], writer: &mut W) -> Result<(), W::Error> {
        // switch (self) {
        match self {
            // .home, .inherit => try formatter.formatEntry([]const u8, @tagName(self)),
            WorkingDirectory::Home => format_entry_string(name, b"home", writer),
            WorkingDirectory::Inherit => format_entry_string(name, b"inherit", writer),
            // .path => |path| try formatter.formatEntry([]const u8, path),
            WorkingDirectory::Path(path) => format_entry_string(name, path, writer),
        }
    }
}

impl FormatEntry for WorkingDirectory {
    fn format_entry<W: Writer>(&self, name: &[u8], writer: &mut W) -> Result<(), W::Error> {
        WorkingDirectory::format_entry(self, name, writer)
    }
}

/// Color represents a color using RGB.
///
/// This is a packed struct so that the C API to read color values just
/// works by setting it to a C integer.
// pub const Color = struct { r: u8, g: u8, b: u8 };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// ghostty_config_color_s -> rustty_config_color_s (R2)
// pub const C = extern struct { r: u8, g: u8, b: u8 };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct ColorC {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    // pub fn cval(self: Color) Color.C
    pub fn cval(self: Color) -> ColorC {
        // return .{ .r = self.r, .g = self.g, .b = self.b };
        ColorC {
            r: self.r,
            g: self.g,
            b: self.b,
        }
    }

    /// Convert this to the terminal RGB struct
    // pub fn toTerminalRGB(self: Color) terminal.color.RGB
    pub fn to_terminal_rgb(self: Color) -> rustty_vt::color::RGB {
        // return .{ .r = self.r, .g = self.g, .b = self.b };
        rustty_vt::color::RGB {
            r: self.r,
            g: self.g,
            b: self.b,
        }
    }

    // pub fn parseCLI(input_: ?[]const u8) !Color
    // PORT NOTE (oracle 2970e9a2a Config.zig hunk): delegates to the shared
    // `terminal.color.RGB.parse` so CLI/config and the C ABI stay in lockstep
    // (was: local trim + x11-map lookup + fromHex).
    pub fn parse_cli(input_: Option<&[u8]>) -> Result<Color, ParseError> {
        // const input = input_ orelse return error.ValueRequired;
        let input = input_.ok_or(Error::ValueRequired)?;
        // const rgb: terminal.color.RGB = terminal.color.RGB.parse(input) catch return error.InvalidValue;
        //
        // PORT NOTE: `RGB::parse` takes `&str`; non-UTF-8 input has no valid
        // parse in any accepted form (hex/name/rgb:), so it maps to InvalidValue,
        // matching the reference (whose byte-level parse likewise rejects it).
        let s = core::str::from_utf8(input).map_err(|_| Error::InvalidValue)?;
        let rgb = rustty_vt::color::RGB::parse(s).map_err(|_| Error::InvalidValue)?;
        // return .{ .r = rgb.r, .g = rgb.g, .b = rgb.b };
        Ok(Color {
            r: rgb.r,
            g: rgb.g,
            b: rgb.b,
        })
    }

    /// Deep copy of the struct. Required by Config.
    // pub fn clone(self: Color, _: Allocator) error{}!Color
    pub fn clone_in(self: Color) -> Color {
        self
    }

    /// Compare if two of our value are requal. Required by Config.
    // pub fn equal(self: Color, other: Color) bool
    pub fn equal(self: Color, other: Color) -> bool {
        self == other
    }

    /// Used by Formatter
    // pub fn formatEntry(self: Color, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry<W: Writer>(
        self: Color,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), W::Error> {
        // var buf: [128]u8 = undefined;
        // try formatter.formatEntry([]const u8, try self.formatBuf(&buf));
        let mut buf = [0u8; 128];
        let s = self.format_buf(&mut buf);
        format_entry_string(name, &s, writer)
    }

    /// Format the color as a string.
    // pub fn formatBuf(self: Color, buf: []u8) Allocator.Error![]const u8
    pub fn format_buf(self: Color, _buf: &mut [u8]) -> Vec<u8> {
        // return std.fmt.bufPrint(buf, "#{x:0>2}{x:0>2}{x:0>2}", .{ self.r, self.g, self.b })
        //     catch error.OutOfMemory;
        //
        // PORT NOTE: the reference writes into the caller `buf`; the rustty
        // EntryFormatter consumes a byte slice, so we return an owned `Vec<u8>`
        // carrying the same bytes (`#rrggbb`, lowercase, zero-padded). `_buf` is
        // retained for call-site parity.
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b).into_bytes()
    }

    // PORT NOTE (oracle 2970e9a2a): `fromHex` was deleted upstream — `parseCLI`
    // now delegates to `terminal.color.RGB.parse`, which accepts the same hex
    // forms (with/without leading '#', 3 or 6 digits) plus more. See
    // `test "parseCLI hex"` below (was `test "fromHex"`).
}

impl FormatEntry for Color {
    fn format_entry<W: Writer>(&self, name: &[u8], writer: &mut W) -> Result<(), W::Error> {
        Color::format_entry(*self, name, writer)
    }
}

/// Represents color values that can also reference special color
/// values such as "cell-foreground" or "cell-background".
// pub const TerminalColor = union(enum) { color: Color, "cell-foreground", "cell-background" };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalColor {
    Color(Color),
    CellForeground,
    CellBackground,
}

impl TerminalColor {
    // pub fn parseCLI(input_: ?[]const u8) !TerminalColor
    pub fn parse_cli(input_: Option<&[u8]>) -> Result<TerminalColor, ParseError> {
        // const input = input_ orelse return error.ValueRequired;
        let input = input_.ok_or(Error::ValueRequired)?;
        // if (std.mem.eql(u8, input, "cell-foreground")) return .@"cell-foreground";
        if input == b"cell-foreground" {
            return Ok(TerminalColor::CellForeground);
        }
        // if (std.mem.eql(u8, input, "cell-background")) return .@"cell-background";
        if input == b"cell-background" {
            return Ok(TerminalColor::CellBackground);
        }
        // return .{ .color = try Color.parseCLI(input) };
        Ok(TerminalColor::Color(Color::parse_cli(Some(input))?))
    }

    // pub fn toTerminalRGB(self: TerminalColor) ?terminal.color.RGB
    pub fn to_terminal_rgb(self: TerminalColor) -> Option<rustty_vt::color::RGB> {
        // return switch (self) {
        //     .color => |v| v.toTerminalRGB(),
        //     .@"cell-foreground", .@"cell-background" => null,
        // };
        match self {
            TerminalColor::Color(v) => Some(v.to_terminal_rgb()),
            TerminalColor::CellForeground | TerminalColor::CellBackground => None,
        }
    }

    /// Used by Formatter
    // pub fn formatEntry(self: TerminalColor, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry<W: Writer>(
        self: TerminalColor,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), W::Error> {
        // switch (self) {
        match self {
            // .color => try self.color.formatEntry(formatter),
            TerminalColor::Color(v) => v.format_entry(name, writer),
            // .@"cell-foreground", .@"cell-background"
            // => try formatter.formatEntry([:0]const u8, @tagName(self)),
            TerminalColor::CellForeground => format_entry_string(name, b"cell-foreground", writer),
            TerminalColor::CellBackground => format_entry_string(name, b"cell-background", writer),
        }
    }
}

impl FormatEntry for TerminalColor {
    fn format_entry<W: Writer>(&self, name: &[u8], writer: &mut W) -> Result<(), W::Error> {
        TerminalColor::format_entry(*self, name, writer)
    }
}

/// Represents color values that can be used for bold. See `bold-color`.
// pub const BoldColor = union(enum) { color: Color, bright };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoldColor {
    Color(Color),
    Bright,
}

impl BoldColor {
    /// Convert to the terminal-native BoldColor type.
    // pub fn toTerminal(self: BoldColor) terminal.style.Style.BoldColor
    pub fn to_terminal(self: BoldColor) -> rustty_vt::style::BoldColor {
        // return switch (self) {
        //     .color => |col| .{ .color = col.toTerminalRGB() },
        //     .bright => .bright,
        // };
        match self {
            BoldColor::Color(col) => rustty_vt::style::BoldColor::Color(col.to_terminal_rgb()),
            BoldColor::Bright => rustty_vt::style::BoldColor::Bright,
        }
    }

    // pub fn parseCLI(input_: ?[]const u8) !BoldColor
    pub fn parse_cli(input_: Option<&[u8]>) -> Result<BoldColor, ParseError> {
        // const input = input_ orelse return error.ValueRequired;
        let input = input_.ok_or(Error::ValueRequired)?;
        // if (std.mem.eql(u8, input, "bright")) return .bright;
        if input == b"bright" {
            return Ok(BoldColor::Bright);
        }
        // return .{ .color = try Color.parseCLI(input) };
        Ok(BoldColor::Color(Color::parse_cli(Some(input))?))
    }

    /// Used by Formatter
    // pub fn formatEntry(self: BoldColor, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry<W: Writer>(
        self: BoldColor,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), W::Error> {
        // switch (self) {
        match self {
            // .color => try self.color.formatEntry(formatter),
            BoldColor::Color(v) => v.format_entry(name, writer),
            // .bright => try formatter.formatEntry([:0]const u8, @tagName(self)),
            BoldColor::Bright => format_entry_string(name, b"bright", writer),
        }
    }
}

impl FormatEntry for BoldColor {
    fn format_entry<W: Writer>(&self, name: &[u8], writer: &mut W) -> Result<(), W::Error> {
        BoldColor::format_entry(*self, name, writer)
    }
}

// pub const FontStyle = union(enum) { default: void, false: void, name: [:0]const u8 };
//
// PORT NOTE: the Config field default is `.{ .default = {} }` — the `default`
// arm, which is the first variant, so `#[derive(Default)]` reproduces it exactly.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum FontStyle {
    /// Use the default font style that font discovery finds.
    #[default]
    Default,
    /// Disable this font style completely. This will fall back to using
    /// the regular font when this style is encountered.
    False,
    /// A specific named font style to use for this style.
    Name(Vec<u8>),
}

impl FontStyle {
    // pub fn parseCLI(self: *Self, alloc: Allocator, input: ?[]const u8) !void
    pub fn parse_cli(&mut self, input: Option<&[u8]>) -> Result<(), ParseError> {
        // const value = input orelse return error.ValueRequired;
        let value = input.ok_or(Error::ValueRequired)?;

        // if (std.mem.eql(u8, value, "default")) {
        if value == b"default" {
            // self.* = .{ .default = {} };
            *self = FontStyle::Default;
            // return;
            return Ok(());
        }

        // if (std.mem.eql(u8, value, "false")) {
        if value == b"false" {
            // self.* = .{ .false = {} };
            *self = FontStyle::False;
            // return;
            return Ok(());
        }

        // const nameZ = try alloc.dupeZ(u8, value);
        // self.* = .{ .name = nameZ };
        *self = FontStyle::Name(value.to_vec());
        Ok(())
    }

    /// Returns the string name value that can be used with a font
    /// descriptor.
    // pub fn nameValue(self: Self) ?[:0]const u8
    pub fn name_value(&self) -> Option<&[u8]> {
        // return switch (self) {
        //     .default, .false => null,
        //     .name => self.name,
        // };
        match self {
            FontStyle::Default | FontStyle::False => None,
            FontStyle::Name(name) => Some(name),
        }
    }

    // pub fn clone(self: Self, alloc: Allocator) Allocator.Error!Self
    pub fn clone_in(&self) -> FontStyle {
        // return switch (self) {
        //     .default, .false => self,
        //     .name => |v| .{ .name = try alloc.dupeZ(u8, v) },
        // };
        match self {
            FontStyle::Default | FontStyle::False => self.clone(),
            FontStyle::Name(v) => FontStyle::Name(v.clone()),
        }
    }

    // pub fn formatEntry(self: Self, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry<W: Writer>(&self, name: &[u8], writer: &mut W) -> Result<(), W::Error> {
        // switch (self) {
        match self {
            // .default, .false => try formatter.formatEntry([]const u8, @tagName(self)),
            FontStyle::Default => format_entry_string(name, b"default", writer),
            FontStyle::False => format_entry_string(name, b"false", writer),
            // .name => |name| try formatter.formatEntry([:0]const u8, name),
            FontStyle::Name(v) => format_entry_string(name, v, writer),
        }
    }
}

impl FormatEntry for FontStyle {
    fn format_entry<W: Writer>(&self, name: &[u8], writer: &mut W) -> Result<(), W::Error> {
        FontStyle::format_entry(self, name, writer)
    }
}

// pub const BackgroundBlur = union(enum) { false, true, "macos-glass-regular", "macos-glass-clear", radius: u8 };
//
// PORT NOTE (Chunk 3a A1 hoist — GPU_RD_CHUNK3A_DESIGN.md): named as a
// `renderer/generic.zig` `DerivedConfig` field type (`background_blur`); the union +
// its `DeepEqual` (hand `.@"union"` arm) are hoisted to the rustty-render-types leaf
// (the `AlphaBlending` precedent) to break the config->renderer cycle. Re-exported here
// so `value_types::BackgroundBlur` and the field/defaults/Key use-sites keep resolving.
// The reference's value methods `parse_cli`/`enabled`/`cval`/`format_entry` stay CONFIG-
// local as free fns below (they are config/cfg-fmt-coupled and carry distinctive
// reference comments that must remain in config provenance; the renderer never calls
// them — it matches the variants directly, `generic.zig:1399`).
pub use rustty_render_types::BackgroundBlur;

// pub fn parseCLI(self: *BackgroundBlur, input: ?[]const u8) !void
pub(crate) fn background_blur_parse_cli(
    v: &mut BackgroundBlur,
    input: Option<&[u8]>,
) -> Result<(), ParseError> {
    // const input_ = input orelse {
    //     // Emulate behavior for bools
    //     self.* = .true;
    //     return;
    // };
    let input_ = match input {
        Some(x) => x,
        None => {
            // Emulate behavior for bools
            *v = BackgroundBlur::True;
            return Ok(());
        }
    };

    // Try to parse normal bools
    // if (cli.args.parseBool(input_)) |b| {
    //     self.* = if (b) .true else .false;
    //     return;
    // } else |_| {}
    if let Ok(b) = parse_bool(input_) {
        *v = if b {
            BackgroundBlur::True
        } else {
            BackgroundBlur::False
        };
        return Ok(());
    }

    // Try to parse enums
    // if (std.meta.stringToEnum(std.meta.Tag(BackgroundBlur), input_)) |v| switch (v) {
    //     inline else => |tag| tag: {
    //         // We can only parse void types
    //         const info = std.meta.fieldInfo(BackgroundBlur, tag);
    //         if (info.type != void) break :tag;
    //         self.* = @unionInit(BackgroundBlur, @tagName(tag), {});
    //         return;
    //     },
    // };
    //
    // PORT NOTE: the comptime `stringToEnum` over the tag enum + the
    // `if (info.type != void) break :tag` guard only ever matches the four
    // void-payload tags (`false`/`true`/`macos-glass-regular`/
    // `macos-glass-clear`); the `radius: u8` tag has a non-void payload so its
    // `break :tag` falls through to the int parse below. (`false`/`true` were
    // already consumed by `parseBool` above, but are listed for fidelity.)
    match input_ {
        b"false" => {
            *v = BackgroundBlur::False;
            return Ok(());
        }
        b"true" => {
            *v = BackgroundBlur::True;
            return Ok(());
        }
        b"macos-glass-regular" => {
            *v = BackgroundBlur::MacosGlassRegular;
            return Ok(());
        }
        b"macos-glass-clear" => {
            *v = BackgroundBlur::MacosGlassClear;
            return Ok(());
        }
        _ => {}
    }

    // self.* = .{ .radius = std.fmt.parseInt(u8, input_, 0) catch return error.InvalidValue };
    *v = BackgroundBlur::Radius(parse_int_u8_base0(input_).map_err(|_| Error::InvalidValue)?);
    Ok(())
}

// pub fn enabled(self: BackgroundBlur) bool
//
// PORT NOTE (Chunk 3a): R1-ported verbatim but currently has NO in-tree caller (the
// apprt blur-enable path is not yet ported); `#[allow(dead_code)]` keeps the
// `-D warnings` gate green until that path lands. Not a stub — the body is faithful.
#[allow(dead_code)]
pub(crate) fn background_blur_enabled(v: BackgroundBlur) -> bool {
    // return switch (self) {
    match v {
        // .false => false,
        BackgroundBlur::False => false,
        // .true => true,
        BackgroundBlur::True => true,
        // .radius => |v| v > 0,
        BackgroundBlur::Radius(r) => r > 0,
        // We treat these as true because they both imply some blur!
        // This has the effect of making the standard blur happen on
        // Linux.
        // .@"macos-glass-regular", .@"macos-glass-clear" => true,
        BackgroundBlur::MacosGlassRegular | BackgroundBlur::MacosGlassClear => true,
    }
}

// pub fn cval(self: BackgroundBlur) i16
pub fn background_blur_cval(v: BackgroundBlur) -> i16 {
    // return switch (self) {
    match v {
        // .false => 0,
        BackgroundBlur::False => 0,
        // .true => 20,
        BackgroundBlur::True => 20,
        // .radius => |v| v,
        BackgroundBlur::Radius(r) => r as i16,
        // I hate sentinel values like this but this is only for
        // our macOS application currently. We can switch to a proper
        // tagged union if we ever need to.
        // .@"macos-glass-regular" => -1,
        BackgroundBlur::MacosGlassRegular => -1,
        // .@"macos-glass-clear" => -2,
        BackgroundBlur::MacosGlassClear => -2,
    }
}

// pub fn formatEntry(self: BackgroundBlur, formatter: anytype) !void
//
// PORT NOTE (Chunk 3a): the former `impl FormatEntry for BackgroundBlur` (a
// rustty-cfg-fmt trait on the now-foreign hoisted type) would orphan; this config-local
// free fn carries the identical body and is dispatched by the `f_entry!(field, fn)` arm.
pub(crate) fn background_blur_format_entry<W: Writer>(
    v: BackgroundBlur,
    name: &[u8],
    writer: &mut W,
) -> Result<(), W::Error> {
    // switch (self) {
    match v {
        // .false => try formatter.formatEntry(bool, false),
        BackgroundBlur::False => format_entry_display(name, &false, writer),
        // .true => try formatter.formatEntry(bool, true),
        BackgroundBlur::True => format_entry_display(name, &true, writer),
        // .radius => |v| try formatter.formatEntry(u8, v),
        BackgroundBlur::Radius(r) => format_entry_display(name, &r, writer),
        // .@"macos-glass-regular" => try formatter.formatEntry([]const u8, "macos-glass-regular"),
        BackgroundBlur::MacosGlassRegular => {
            format_entry_string(name, b"macos-glass-regular", writer)
        }
        // .@"macos-glass-clear" => try formatter.formatEntry([]const u8, "macos-glass-clear"),
        BackgroundBlur::MacosGlassClear => format_entry_string(name, b"macos-glass-clear", writer),
    }
}

/// PORTS: the error set of `std.fmt.parseInt` — `error{ Overflow, InvalidCharacter }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseIntError {
    Overflow,
    InvalidCharacter,
}

/// PORTS: `std.fmt.parseInt(u8, input, 0)` — base-0 means the radix is detected
/// from the prefix (`0x`/`0o`/`0b`, optional leading `+`/`-`; here unsigned).
fn parse_int_u8_base0(input: &[u8]) -> Result<u8, ParseIntError> {
    let v = parse_unsigned_base0(input)?;
    u8::try_from(v).map_err(|_| ParseIntError::Overflow)
}

/// PORTS: `std.fmt.parseInt(T, buf, 0)` for unsigned `T` — radix detection from
/// the `0x`/`0o`/`0b` prefix (else base 10), an optional leading sign, and `_`
/// digit separators. Returns `error.Overflow` -> the reference's `error.Overflow`
/// and `error.InvalidCharacter` -> `error.InvalidValue` semantics; both collapse
/// to `InvalidValue` at the config call sites that use base 0 (`parseInt(u8,..,0)`).
//
// PORT NOTE: a u128 accumulator with an explicit overflow check reproduces
// `std.fmt.parseInt`'s digit-by-digit `mulWithOverflow`/`addWithOverflow`.
fn parse_unsigned_base0(buf: &[u8]) -> Result<u128, ParseIntError> {
    // const Result = ... if (buf.len == 0) return error.InvalidCharacter;
    if buf.is_empty() {
        return Err(ParseIntError::InvalidCharacter);
    }

    // Optional sign. `parseInt` accepts a leading '+'/'-'; for unsigned a '-'
    // with a non-zero magnitude overflows. We accept '+' and reject '-' on a
    // non-zero value (matching unsigned `parseInt`).
    let mut i = 0;
    let mut negative = false;
    match buf[0] {
        b'+' => i = 1,
        b'-' => {
            negative = true;
            i = 1;
        }
        _ => {}
    }
    if i >= buf.len() {
        return Err(ParseIntError::InvalidCharacter);
    }

    // Detect the radix from the prefix when base == 0.
    let mut radix: u128 = 10;
    if buf[i] == b'0' && i + 1 < buf.len() {
        match buf[i + 1] {
            b'x' | b'X' => {
                radix = 16;
                i += 2;
            }
            b'o' | b'O' => {
                radix = 8;
                i += 2;
            }
            b'b' | b'B' => {
                radix = 2;
                i += 2;
            }
            _ => {}
        }
    }
    if i >= buf.len() {
        return Err(ParseIntError::InvalidCharacter);
    }

    let mut acc: u128 = 0;
    let mut any = false;
    for &c in &buf[i..] {
        // `_` digit separators are allowed by std.fmt.parseInt (but not as the
        // first/last char or doubled; we accept them anywhere, which is a
        // superset never exercised by config inputs).
        if c == b'_' {
            continue;
        }
        let digit = match c {
            b'0'..=b'9' => (c - b'0') as u128,
            b'a'..=b'z' => (c - b'a' + 10) as u128,
            b'A'..=b'Z' => (c - b'A' + 10) as u128,
            _ => return Err(ParseIntError::InvalidCharacter),
        };
        if digit >= radix {
            return Err(ParseIntError::InvalidCharacter);
        }
        acc = acc
            .checked_mul(radix)
            .and_then(|v| v.checked_add(digit))
            .ok_or(ParseIntError::Overflow)?;
        any = true;
    }
    if !any {
        return Err(ParseIntError::InvalidCharacter);
    }
    if negative && acc != 0 {
        return Err(ParseIntError::Overflow);
    }
    Ok(acc)
}

// ===========================================================================
// Repeatable/list and aggregate structs (with method sigs).
// ===========================================================================

// pub const ColorList = struct { colors: ArrayListUnmanaged(Color), colors_c: ArrayListUnmanaged(Color.C) };
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ColorList {
    pub colors: Vec<Color>,
    pub colors_c: Vec<ColorC>,
}

/// ghostty_config_color_list_s -> rustty_config_color_list_s (R2)
// pub const C = extern struct { colors: [*]Color.C, len: usize };
#[repr(C)]
pub struct ColorListC {
    pub colors: *const ColorC,
    pub len: usize,
}

impl ColorList {
    // pub fn cval(self: *const Self) C
    pub fn cval(&self) -> ColorListC {
        // return .{ .colors = self.colors_c.items.ptr, .len = self.colors_c.items.len };
        ColorListC {
            colors: self.colors_c.as_ptr(),
            len: self.colors_c.len(),
        }
    }

    // pub fn parseCLI(self: *Self, alloc: Allocator, input_: ?[]const u8) !void
    pub fn parse_cli(&mut self, input_: Option<&[u8]>) -> Result<(), ParseError> {
        // const input = input_ orelse return error.ValueRequired;
        let input = input_.ok_or(Error::ValueRequired)?;
        // if (input.len == 0) return error.ValueRequired;
        if input.is_empty() {
            return Err(Error::ValueRequired);
        }

        // Always reset on parse
        // self.* = .{};
        *self = ColorList::default();

        // Split the input by commas and parse each color
        // var it = std.mem.tokenizeScalar(u8, input, ',');
        // var count: usize = 0;
        // while (it.next()) |raw| {
        let mut count: usize = 0;
        for raw in tokenize_scalar(input, b',') {
            // count += 1;
            count += 1;
            // if (count > 64) return error.InvalidValue;
            if count > 64 {
                return Err(Error::InvalidValue);
            }

            // Trim whitespace from each color value
            // const trimmed = std.mem.trim(u8, raw, " \t");
            let trimmed = trim_ws(raw);
            // const color = try Color.parseCLI(trimmed);
            let color = Color::parse_cli(Some(trimmed))?;
            // try self.colors.append(alloc, color);
            self.colors.push(color);
            // try self.colors_c.append(alloc, color.cval());
            self.colors_c.push(color.cval());
        }

        // If no colors were parsed, we need to return an error
        // if (self.colors.items.len == 0) return error.InvalidValue;
        if self.colors.is_empty() {
            return Err(Error::InvalidValue);
        }

        // assert(self.colors.items.len == self.colors_c.items.len);
        assert_eq!(self.colors.len(), self.colors_c.len());
        Ok(())
    }

    // pub fn clone(self: *const Self, alloc: Allocator) Allocator.Error!Self
    pub fn clone_in(&self) -> ColorList {
        // return .{ .colors = try self.colors.clone(alloc), .colors_c = try self.colors_c.clone(alloc) };
        ColorList {
            colors: self.colors.clone(),
            colors_c: self.colors_c.clone(),
        }
    }

    // pub fn equal(self: Self, other: Self) bool
    pub fn equal(&self, other: &ColorList) -> bool {
        // const itemsA = self.colors.items;
        // const itemsB = other.colors.items;
        let items_a = &self.colors;
        let items_b = &other.colors;
        // if (itemsA.len != itemsB.len) return false;
        if items_a.len() != items_b.len() {
            return false;
        }
        // for (itemsA, itemsB) |a, b| { if (!a.equal(b)) return false; } else return true;
        for (a, b) in items_a.iter().zip(items_b.iter()) {
            if !a.equal(*b) {
                return false;
            }
        }
        true
    }

    // pub fn formatEntry(self: Self, formatter: anytype) !void
    pub fn format_entry<W: Writer>(&self, name: &[u8], writer: &mut W) -> Result<(), W::Error> {
        // If no items, we want to render an empty field.
        // if (self.colors.items.len == 0) { try formatter.formatEntry(void, {}); return; }
        if self.colors.is_empty() {
            return format_entry_void(name, writer);
        }

        // Build up the value of our config. Our buffer size should be
        // sized to contain all possible maximum values.
        // var buf: [1024]u8 = undefined;
        // var writer: std.Io.Writer = .fixed(&buf);
        let mut buf: Vec<u8> = Vec::new();
        // for (self.colors.items, 0..) |color, i| {
        for (i, color) in self.colors.iter().enumerate() {
            // var color_buf: [128]u8 = undefined;
            let mut color_buf = [0u8; 128];
            // const color_str = try color.formatBuf(&color_buf);
            let color_str = color.format_buf(&mut color_buf);
            // if (i != 0) writer.writeByte(',') catch return error.OutOfMemory;
            if i != 0 {
                buf.push(b',');
            }
            // writer.writeAll(color_str) catch return error.OutOfMemory;
            buf.extend_from_slice(&color_str);
        }

        // try formatter.formatEntry([]const u8, writer.buffered());
        format_entry_string(name, &buf, writer)
    }
}

impl FormatEntry for ColorList {
    fn format_entry<W: Writer>(&self, name: &[u8], writer: &mut W) -> Result<(), W::Error> {
        ColorList::format_entry(self, name, writer)
    }
}

/// PORTS: `std.mem.tokenizeScalar(u8, input, delim)` — yields non-empty
/// delimiter-separated tokens (consecutive delimiters and leading/trailing
/// delimiters produce no empty tokens), in order.
fn tokenize_scalar(input: &[u8], delim: u8) -> impl Iterator<Item = &[u8]> {
    input.split(move |&c| c == delim).filter(|s| !s.is_empty())
}

/// Palette is the 256 color palette for 256-color mode.
// pub const Palette = struct { value: terminal.color.Palette = default, mask: PaletteMask = initEmpty };
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Palette {
    /// The actual value that is updated as we parse.
    pub value: rustty_vt::color::Palette,
    /// Keep track of which indexes were manually set by the user.
    pub mask: rustty_vt::color::PaletteMask,
}

impl Default for Palette {
    fn default() -> Self {
        // value: terminal.color.default, mask: .initEmpty()
        Self {
            value: rustty_vt::color::DEFAULT,
            mask: rustty_vt::color::PaletteMask::default(),
        }
    }
}

/// ghostty_config_palette_s -> rustty_config_palette_s (R2)
// pub const C = extern struct { colors: [256]Color.C };
#[repr(C)]
pub struct PaletteC {
    pub colors: [ColorC; 256],
}

/// PORTS: the error set of `Palette.parseCLI` — `parseInt`'s `error.Overflow`/
/// `error.InvalidCharacter` plus `error.InvalidValue`/`error.ValueRequired`. The
/// reference's `parseCLI` is `!void`; the distinguishable `error.Overflow` is
/// asserted by `test "parseCLI overflow"`, so it is preserved here (the base
/// args `Error` set has no Overflow variant).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteParseError {
    ValueRequired,
    InvalidValue,
    Overflow,
    InvalidCharacter,
}

impl Palette {
    // pub fn cval(self: Self) Palette.C
    pub fn cval(&self) -> PaletteC {
        // var result: Palette.C = undefined;
        // for (self.value, 0..) |color, i| {
        //     result.colors[i] = Color.C{ .r = color.r, .g = color.g, .b = color.b };
        // }
        // return result;
        let mut colors = [ColorC { r: 0, g: 0, b: 0 }; 256];
        for (i, color) in self.value.iter().enumerate() {
            colors[i] = ColorC {
                r: color.r,
                g: color.g,
                b: color.b,
            };
        }
        PaletteC { colors }
    }

    // pub fn parseCLI(self: *Self, input: ?[]const u8) !void
    // PORT NOTE (oracle 2970e9a2a Config.zig hunk): delegates to the shared
    // `terminal.color.parsePaletteEntry` so CLI/config and the C ABI stay in
    // lockstep (was: local `=`-split + parseInt + Color.parseCLI).
    pub fn parse_cli(&mut self, input: Option<&[u8]>) -> Result<(), PaletteParseError> {
        // const value = input orelse return error.ValueRequired;
        let value = input.ok_or(PaletteParseError::ValueRequired)?;
        // const entry = terminal.color.parsePaletteEntry(value) catch |err| switch (err) {
        //     error.Overflow => return error.Overflow,
        //     error.InvalidFormat => return error.InvalidValue,
        // };
        let entry = rustty_vt::color::parse_palette_entry(value).map_err(|e| match e {
            rustty_vt::color::ParsePaletteEntryError::Overflow => PaletteParseError::Overflow,
            rustty_vt::color::ParsePaletteEntryError::InvalidFormat => {
                PaletteParseError::InvalidValue
            }
        })?;
        // self.value[entry.index] = entry.color;
        self.value[entry.index as usize] = entry.color;
        // self.mask.set(entry.index);
        self.mask.set(entry.index as usize);
        Ok(())
    }

    // pub fn clone(self: Self, _: Allocator) error{}!Self
    pub fn clone_in(&self) -> Palette {
        self.clone()
    }

    // pub fn equal(self: Self, other: Self) bool
    pub fn equal(&self, other: &Palette) -> bool {
        // return std.meta.eql(self, other);
        self == other
    }

    // pub fn formatEntry(self: Self, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry<W: Writer>(&self, name: &[u8], writer: &mut W) -> Result<(), W::Error> {
        // var buf: [128]u8 = undefined;
        // for (0.., self.value) |k, v| {
        //     try formatter.formatEntry([]const u8,
        //         std.fmt.bufPrint(&buf, "{d}=#{x:0>2}{x:0>2}{x:0>2}", .{ k, v.r, v.g, v.b })
        //             catch return error.OutOfMemory);
        // }
        for (k, v) in self.value.iter().enumerate() {
            let s = format!("{}=#{:02x}{:02x}{:02x}", k, v.r, v.g, v.b);
            format_entry_string(name, s.as_bytes(), writer)?;
        }
        Ok(())
    }
}

impl FormatEntry for Palette {
    fn format_entry<W: Writer>(&self, name: &[u8], writer: &mut W) -> Result<(), W::Error> {
        Palette::format_entry(self, name, writer)
    }
}

/// PORTS: `std.mem.indexOf(u8, haystack, &[_]u8{needle})` (single-byte needle).
fn index_of(haystack: &[u8], needle: u8) -> Option<usize> {
    haystack.iter().position(|&c| c == needle)
}

/// RepeatableString is a string value that can be repeated to accumulate
/// a list of strings. This isn't called "StringList" because I find that
/// sometimes leads to confusion that it _accepts_ a list such as
/// comma-separated values.
// pub const RepeatableString = struct { list: ArrayListUnmanaged([:0]const u8), overwrite_next: bool = false };
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RepeatableString {
    // Allocator for the list is the arena for the parent config.
    pub list: Vec<Vec<u8>>,
    // If true, then the next value will clear the list and start over
    // rather than append. This is a bit of a hack but is here to make
    // the font-family set of configurations work with CLI parsing.
    pub overwrite_next: bool,
}

impl RepeatableString {
    // pub fn parseCLI(self: *Self, alloc: Allocator, input: ?[]const u8) !void
    pub fn parse_cli(&mut self, input: Option<&[u8]>) -> Result<(), ParseError> {
        // const value = input orelse return error.ValueRequired;
        let value = input.ok_or(Error::ValueRequired)?;

        // Empty value resets the list
        // if (value.len == 0) { self.list.clearRetainingCapacity(); return; }
        if value.is_empty() {
            self.list.clear();
            return Ok(());
        }

        // If we're overwriting then we clear before appending
        // if (self.overwrite_next) {
        //     self.list.clearRetainingCapacity();
        //     self.overwrite_next = false;
        // }
        if self.overwrite_next {
            self.list.clear();
            self.overwrite_next = false;
        }

        // const copy = try alloc.dupeZ(u8, value);
        // try self.list.append(alloc, copy);
        self.list.push(value.to_vec());
        Ok(())
    }

    /// Deep copy of the struct. Required by Config.
    // pub fn clone(self: *const Self, alloc: Allocator) Allocator.Error!Self
    pub fn clone_in(&self) -> RepeatableString {
        // Copy the list and all the strings in the list.
        // var list = try std.ArrayListUnmanaged([:0]const u8).initCapacity(alloc, self.list.items.len);
        // for (self.list.items) |item| { const copy = try alloc.dupeZ(u8, item); list.appendAssumeCapacity(copy); }
        // return .{ .list = list };
        RepeatableString {
            list: self.list.clone(),
            overwrite_next: false,
        }
    }

    /// The number of items in the list
    // pub fn count(self: Self) usize
    pub fn count(&self) -> usize {
        // return self.list.items.len;
        self.list.len()
    }

    /// Compare if two of our value are requal. Required by Config.
    // pub fn equal(self: Self, other: Self) bool
    pub fn equal(&self, other: &RepeatableString) -> bool {
        // const itemsA = self.list.items;
        // const itemsB = other.list.items;
        // if (itemsA.len != itemsB.len) return false;
        if self.list.len() != other.list.len() {
            return false;
        }
        // for (itemsA, itemsB) |a, b| { if (!std.mem.eql(u8, a, b)) return false; } else return true;
        for (a, b) in self.list.iter().zip(other.list.iter()) {
            if a != b {
                return false;
            }
        }
        true
    }

    /// Used by Formatter
    // pub fn formatEntry(self: Self, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry<W: Writer>(&self, name: &[u8], writer: &mut W) -> Result<(), W::Error> {
        // If no items, we want to render an empty field.
        // if (self.list.items.len == 0) { try formatter.formatEntry(void, {}); return; }
        if self.list.is_empty() {
            return format_entry_void(name, writer);
        }

        // for (self.list.items) |value| { try formatter.formatEntry([]const u8, value); }
        for value in &self.list {
            format_entry_string(name, value, writer)?;
        }
        Ok(())
    }
}

impl FormatEntry for RepeatableString {
    fn format_entry<W: Writer>(&self, name: &[u8], writer: &mut W) -> Result<(), W::Error> {
        RepeatableString::format_entry(self, name, writer)
    }
}

/// SelectionWordChars stores the parsed codepoints for word boundary
/// characters used during text selection. The string is parsed once
/// during configuration and stored as u21 codepoints for efficient
/// lookup during selection operations.
// pub const SelectionWordChars = struct { codepoints: []const u21 = &default_word_boundaries };
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionWordChars {
    /// The parsed codepoints. Always includes null (U+0000) at index 0.
    pub codepoints: Vec<u32>,
}

impl Default for SelectionWordChars {
    fn default() -> Self {
        Self {
            codepoints: rustty_vt::selection_codepoints::DEFAULT_WORD_BOUNDARIES.to_vec(),
        }
    }
}

impl SelectionWordChars {
    // pub fn parseCLI(self: *Self, alloc: Allocator, input: ?[]const u8) !void
    pub fn parse_cli(&mut self, input: Option<&[u8]>) -> Result<(), ParseError> {
        // const value = input orelse return error.ValueRequired;
        let value = input.ok_or(Error::ValueRequired)?;

        // Parse string with Zig escape sequence support into codepoints
        // var list: std.ArrayList(u21) = .empty;
        let mut list: Vec<u32> = Vec::new();

        // Always include null as first boundary
        // try list.append(alloc, 0);
        list.push(0);

        // var it = string.codepointIterator(value);
        // while (it.next() catch return error.InvalidValue) |codepoint| {
        //     try list.append(alloc, codepoint);
        // }
        let mut it = crate::string::codepoint_iterator(value);
        loop {
            match it.next() {
                Ok(Some(codepoint)) => list.push(codepoint),
                Ok(None) => break,
                Err(_) => return Err(Error::InvalidValue),
            }
        }

        // self.codepoints = try list.toOwnedSlice(alloc);
        self.codepoints = list;
        Ok(())
    }

    /// Deep copy of the struct. Required by Config.
    // pub fn clone(self: *const Self, alloc: Allocator) Allocator.Error!Self
    pub fn clone_in(&self) -> SelectionWordChars {
        // const copy = try alloc.dupe(u21, self.codepoints);
        // return .{ .codepoints = copy };
        SelectionWordChars {
            codepoints: self.codepoints.clone(),
        }
    }

    /// Compare if two values are equal. Required by Config.
    // pub fn equal(self: Self, other: Self) bool
    pub fn equal(&self, other: &SelectionWordChars) -> bool {
        // return std.mem.eql(u21, self.codepoints, other.codepoints);
        self.codepoints == other.codepoints
    }

    /// Used by Formatter
    // pub fn formatEntry(self: Self, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry<W: Writer>(&self, name: &[u8], writer: &mut W) -> Result<(), W::Error> {
        // Convert codepoints back to UTF-8 string for display
        // var buf: [4096]u8 = undefined;
        // var pos: usize = 0;
        let mut buf = [0u8; 4096];
        let mut pos: usize = 0;

        // Skip the null character at index 0
        // for (self.codepoints[1..]) |codepoint| {
        for &codepoint in &self.codepoints[1..] {
            // var utf8_buf: [4]u8 = undefined;
            // const len = std.unicode.utf8Encode(codepoint, &utf8_buf) catch continue;
            let mut utf8_buf = [0u8; 4];
            let ch = match char::from_u32(codepoint) {
                Some(c) => c,
                None => continue,
            };
            let len = ch.encode_utf8(&mut utf8_buf).len();
            // if (pos + len > buf.len) break;
            if pos + len > buf.len() {
                break;
            }
            // @memcpy(buf[pos..][0..len], utf8_buf[0..len]);
            buf[pos..pos + len].copy_from_slice(&utf8_buf[0..len]);
            // pos += len;
            pos += len;
        }

        // try formatter.formatEntry([]const u8, buf[0..pos]);
        format_entry_string(name, &buf[0..pos], writer)
    }
}

impl FormatEntry for SelectionWordChars {
    fn format_entry<W: Writer>(&self, name: &[u8], writer: &mut W) -> Result<(), W::Error> {
        SelectionWordChars::format_entry(self, name, writer)
    }
}

/// FontVariation is a repeatable configuration value that sets a single
/// font variation value. Font variations are configurations for what
/// are often called "variable fonts." The font files usually end in
/// "-VF.ttf."
///
/// The value for this is in the format of `id=value` where `id` is the
/// 4-character font variation axis identifier and `value` is the
/// floating point value for that axis. For more details on font variations
/// see the MDN font-variation-settings documentation since this copies that
/// behavior almost exactly:
///
/// https://developer.mozilla.org/en-US/docs/Web/CSS/font-variation-settings
// pub const RepeatableFontVariation = struct { list: ArrayListUnmanaged(fontpkg.face.Variation) };
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RepeatableFontVariation {
    // Allocator for the list is the arena for the parent config.
    pub list: Vec<FaceVariation>,
}

impl RepeatableFontVariation {
    // pub fn parseCLI(self: *Self, alloc: Allocator, input_: ?[]const u8) !void
    pub fn parse_cli(&mut self, input_: Option<&[u8]>) -> Result<(), ParseError> {
        // const input = input_ orelse return error.ValueRequired;
        let input = input_.ok_or(Error::ValueRequired)?;
        // const eql_idx = std.mem.indexOf(u8, input, "=") orelse return error.InvalidValue;
        let eql_idx = index_of(input, b'=').ok_or(Error::InvalidValue)?;
        // const whitespace = " \t";
        // const key = std.mem.trim(u8, input[0..eql_idx], whitespace);
        let key = trim_ws(&input[0..eql_idx]);
        // const value = std.mem.trim(u8, input[eql_idx + 1 ..], whitespace);
        let value = trim_ws(&input[eql_idx + 1..]);
        // if (key.len != 4) return error.InvalidValue;
        if key.len() != 4 {
            return Err(Error::InvalidValue);
        }
        // try self.list.append(alloc, .{
        //     .id = fontpkg.face.Variation.Id.init(@ptrCast(key.ptr)),
        //     .value = std.fmt.parseFloat(f64, value) catch return error.InvalidValue,
        // });
        let key4: [u8; 4] = [key[0], key[1], key[2], key[3]];
        self.list.push(FaceVariation {
            id: face_variation_id_init(&key4),
            value: parse_float_f64(value).map_err(|_| Error::InvalidValue)?,
        });
        Ok(())
    }

    /// Deep copy of the struct. Required by Config.
    // pub fn clone(self: *const Self, alloc: Allocator) Allocator.Error!Self
    pub fn clone_in(&self) -> RepeatableFontVariation {
        // return .{ .list = try self.list.clone(alloc) };
        RepeatableFontVariation {
            list: self.list.clone(),
        }
    }

    /// Compare if two of our value are requal. Required by Config.
    // pub fn equal(self: Self, other: Self) bool
    pub fn equal(&self, other: &RepeatableFontVariation) -> bool {
        // const itemsA = self.list.items;
        // const itemsB = other.list.items;
        // if (itemsA.len != itemsB.len) return false;
        if self.list.len() != other.list.len() {
            return false;
        }
        // for (itemsA, itemsB) |a, b| { if (!std.meta.eql(a, b)) return false; } else return true;
        for (a, b) in self.list.iter().zip(other.list.iter()) {
            if a != b {
                return false;
            }
        }
        true
    }

    /// Used by Formatter
    // pub fn formatEntry(self: Self, formatter: anytype) !void
    pub fn format_entry<W: Writer>(&self, name: &[u8], writer: &mut W) -> Result<(), W::Error> {
        // if (self.list.items.len == 0) { try formatter.formatEntry(void, {}); return; }
        if self.list.is_empty() {
            return format_entry_void(name, writer);
        }

        // var buf: [128]u8 = undefined;
        // for (self.list.items) |value| {
        //     const str = std.fmt.bufPrint(&buf, "{s}={d}", .{ value.id.str(), value.value }) catch ...;
        //     try formatter.formatEntry([]const u8, str);
        // }
        for value in &self.list {
            let id_str = face_variation_id_str(value.id);
            let mut s: Vec<u8> = Vec::new();
            s.extend_from_slice(&id_str);
            s.push(b'=');
            s.extend_from_slice(fmt_float(value.value).as_bytes());
            format_entry_string(name, &s, writer)?;
        }
        Ok(())
    }
}

impl FormatEntry for RepeatableFontVariation {
    fn format_entry<W: Writer>(&self, name: &[u8], writer: &mut W) -> Result<(), W::Error> {
        RepeatableFontVariation::format_entry(self, name, writer)
    }
}

/// Stores a set of keybinds.
///
/// PORTS: `Keybinds` (Config.zig:6415). The big keybind set wrapping
/// `inputpkg.Binding.Set`. Skeleton with the field set + method sigs.
// pub const Keybinds = struct { set, tables, chain_target: union(enum) {...} };
//
// PORT NOTE: no `#[derive(Clone)]` — the reference's deep copy is the explicit
// `clone` method (`clone_in`, via `Set::clone_set` + the table map's deep clone);
// a derived shallow `Clone` would be wrong (and the ordered-map's deep clone is an
// inherent `clone()`, not the `Clone` trait). `Default` gives the `.{}` zero value.
#[derive(Default)]
pub struct Keybinds {
    // set: inputpkg.Binding.Set = .{},
    pub set: rustty_input::binding::Set,

    /// Defined key tables. The default key table is always the root "set",
    /// which allows all table names to be available without reservation.
    // tables: std.StringArrayHashMapUnmanaged(inputpkg.Binding.Set) = .empty,
    //
    // PORT NOTE: `StringArrayHashMapUnmanaged` is an insertion-ordered string map.
    // Its rustty analogue is `AutoArrayHashMapUnmanaged<Vec<u8>, Set>` (the same
    // ordered map `RepeatableStringMap` uses), so `clone`/`equal`/`formatEntryDocs`
    // iterate tables in insertion order exactly as the reference does.
    pub tables: rustty_zigstd::array_hash_map::AutoArrayHashMapUnmanaged<
        Vec<u8>,
        rustty_input::binding::Set,
    >,

    /// The most recent binding target for `chain=` additions.
    ///
    /// This is intentionally tracked at the Keybinds level so that chains can
    /// apply across table boundaries according to parse order.
    // chain_target: union(enum) { root, table: []const u8 } = .root,
    pub chain_target: KeybindsChainTarget,
}

/// PORTS: `Keybinds.chain_target`'s anonymous `union(enum) { root, table: []const u8 }`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum KeybindsChainTarget {
    #[default]
    Root,
    Table(Vec<u8>),
}

/// PORTS: the inferred error set of `Keybinds.parseCLI` — the reference returns
/// `error.ValueRequired`, `error.InvalidFormat`, plus the errors of
/// `Binding.Set.parseAndPut` (`Allocator.Error || error{InvalidFormat,
/// InvalidAction}`) and `Keybinds.init` (`Allocator.Error`). `Allocator.Error`
/// (`OutOfMemory`) aborts in the Rust port (no recoverable allocation failure),
/// and `parseAndPut`'s `InvalidAction` is also surfaced verbatim. The ported
/// tests assert the *specific* error (`error.InvalidFormat`), so this dedicated
/// type carries that variant rather than collapsing into `args::Error`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeybindsParseError {
    // error.ValueRequired
    ValueRequired,
    // error.InvalidFormat
    InvalidFormat,
    // error.InvalidAction (from Binding.Set.parseAndPut)
    InvalidAction,
}

// PORT NOTE: `Binding.Set.parseAndPut` returns `ParseAndPutError {InvalidFormat,
// InvalidAction}`. The reference `try`s it inside `parseCLI`, so each variant
// propagates into `parseCLI`'s error union; this mapping reproduces that.
impl From<rustty_input::binding::ParseAndPutError> for KeybindsParseError {
    fn from(e: rustty_input::binding::ParseAndPutError) -> KeybindsParseError {
        match e {
            rustty_input::binding::ParseAndPutError::InvalidFormat => {
                KeybindsParseError::InvalidFormat
            }
            rustty_input::binding::ParseAndPutError::InvalidAction => {
                KeybindsParseError::InvalidAction
            }
        }
    }
}

impl Keybinds {
    // pub fn init(self: *Keybinds, alloc: Allocator) !void
    pub fn init(&mut self) {
        use rustty_input::binding::{Action, Flags, Trigger, TriggerKey};
        use rustty_input::ctrl_or_super;
        use rustty_vt::input::key::Key;
        use rustty_vt::input::key_mods::Mods;

        // PORT NOTE: `inputpkg.ctrlOrSuper(m)` — re-exported at the input package
        // surface (lib.rs). `inputpkg.Mods` is `rustty_vt::input::key_mods::Mods`.
        // Triggers are built with `Trigger { key, mods }`; `.key = .{ .unicode = c }`
        // -> `TriggerKey::Unicode(c as u21)` and `.key = .{ .physical = .x }` ->
        // `TriggerKey::Physical(Key::X)`. `performable = true` -> `Flags { performable:
        // true, ..Default }`. The `builtin.target.os.tag.isDarwin()` comptime/runtime
        // checks map to `cfg!(target_vendor = "apple")` (same observable behavior).

        // Helper to build a unicode trigger.
        fn uni(c: char, mods: Mods) -> Trigger {
            Trigger {
                key: TriggerKey::Unicode(c as u32),
                mods,
            }
        }
        // Helper to build a physical trigger.
        fn phys(k: Key, mods: Mods) -> Trigger {
            Trigger {
                key: TriggerKey::Physical(k),
                mods,
            }
        }
        // .{ .performable = true }
        let performable = Flags {
            performable: true,
            ..Flags::default()
        };

        // We don't clear the memory because it's in the arena and unlikely
        // to be free-able anyways (since arenas can only clear the last
        // allocated value). This isn't a memory leak because the arena
        // will be freed when the config is freed.
        // self.set = .{};
        self.set = rustty_input::binding::Set::default();
        // self.tables = .empty;
        self.tables = rustty_zigstd::array_hash_map::AutoArrayHashMapUnmanaged::new();
        // self.chain_target = .root;
        self.chain_target = KeybindsChainTarget::Root;

        // keybinds for opening and reloading config
        self.set.put(
            uni(
                ',',
                ctrl_or_super(Mods {
                    shift: true,
                    ..Mods::default()
                }),
            ),
            Action::ReloadConfig,
        );
        self.set
            .put(uni(',', ctrl_or_super(Mods::default())), Action::OpenConfig);

        {
            self.set.put(
                phys(Key::Copy, Mods::default()),
                Action::CopyToClipboard(rustty_input::binding::CopyToClipboard::Mixed),
            );
            self.set.put(
                phys(Key::Paste, Mods::default()),
                Action::PasteFromClipboard,
            );

            // On non-MacOS desktop envs (Windows, KDE, Gnome, Xfce), ctrl+insert is an
            // alt keybinding for Copy and shift+ins is an alt keybinding for Paste
            //
            // The order of these blocks is important. The *last* added keybind for a given action is
            // what will display in the menu. We want the more typical keybinds after this block to be
            // the standard
            if !cfg!(target_vendor = "apple") {
                self.set.put(
                    phys(
                        Key::Insert,
                        Mods {
                            ctrl: true,
                            ..Mods::default()
                        },
                    ),
                    Action::CopyToClipboard(rustty_input::binding::CopyToClipboard::Mixed),
                );
                self.set.put(
                    phys(
                        Key::Insert,
                        Mods {
                            shift: true,
                            ..Mods::default()
                        },
                    ),
                    Action::PasteFromClipboard,
                );
            }

            // On macOS we default to super but Linux ctrl+shift since
            // ctrl+c is to kill the process.
            let mods: Mods = if cfg!(target_vendor = "apple") {
                Mods {
                    super_: true,
                    ..Mods::default()
                }
            } else {
                Mods {
                    ctrl: true,
                    shift: true,
                    ..Mods::default()
                }
            };

            self.set.put_flags(
                uni('c', mods),
                Action::CopyToClipboard(rustty_input::binding::CopyToClipboard::Mixed),
                performable,
            );
            self.set
                .put_flags(uni('v', mods), Action::PasteFromClipboard, performable);
        }

        // Increase font size mapping for keyboards with dedicated plus keys (like german)
        // Note: this order matters below because the C API will only return
        // the last keybinding for a given action. The macOS app uses this to
        // set the expected keybind for the menu.
        self.set.put(
            uni('=', ctrl_or_super(Mods::default())),
            Action::IncreaseFontSize(1.0),
        );
        self.set.put(
            uni('+', ctrl_or_super(Mods::default())),
            Action::IncreaseFontSize(1.0),
        );

        self.set.put(
            uni('-', ctrl_or_super(Mods::default())),
            Action::DecreaseFontSize(1.0),
        );
        self.set.put(
            uni('0', ctrl_or_super(Mods::default())),
            Action::ResetFontSize,
        );

        self.set.put(
            uni(
                'j',
                Mods {
                    shift: true,
                    ctrl: true,
                    super_: true,
                    ..Mods::default()
                },
            ),
            Action::WriteScreenFile(rustty_input::binding::WriteScreen::COPY),
        );

        self.set.put(
            uni(
                'j',
                ctrl_or_super(Mods {
                    shift: true,
                    ..Mods::default()
                }),
            ),
            Action::WriteScreenFile(rustty_input::binding::WriteScreen::PASTE),
        );

        self.set.put(
            uni(
                'j',
                ctrl_or_super(Mods {
                    shift: true,
                    alt: true,
                    ..Mods::default()
                }),
            ),
            Action::WriteScreenFile(rustty_input::binding::WriteScreen::OPEN),
        );

        // Expand Selection
        self.set.put_flags(
            phys(
                Key::ArrowLeft,
                Mods {
                    shift: true,
                    ..Mods::default()
                },
            ),
            Action::AdjustSelection(rustty_input::binding::AdjustSelection::Left),
            performable,
        );
        self.set.put_flags(
            phys(
                Key::ArrowRight,
                Mods {
                    shift: true,
                    ..Mods::default()
                },
            ),
            Action::AdjustSelection(rustty_input::binding::AdjustSelection::Right),
            performable,
        );
        self.set.put_flags(
            phys(
                Key::ArrowUp,
                Mods {
                    shift: true,
                    ..Mods::default()
                },
            ),
            Action::AdjustSelection(rustty_input::binding::AdjustSelection::Up),
            performable,
        );
        self.set.put_flags(
            phys(
                Key::ArrowDown,
                Mods {
                    shift: true,
                    ..Mods::default()
                },
            ),
            Action::AdjustSelection(rustty_input::binding::AdjustSelection::Down),
            performable,
        );
        self.set.put_flags(
            phys(
                Key::PageUp,
                Mods {
                    shift: true,
                    ..Mods::default()
                },
            ),
            Action::AdjustSelection(rustty_input::binding::AdjustSelection::PageUp),
            performable,
        );
        self.set.put_flags(
            phys(
                Key::PageDown,
                Mods {
                    shift: true,
                    ..Mods::default()
                },
            ),
            Action::AdjustSelection(rustty_input::binding::AdjustSelection::PageDown),
            performable,
        );
        self.set.put_flags(
            phys(
                Key::Home,
                Mods {
                    shift: true,
                    ..Mods::default()
                },
            ),
            Action::AdjustSelection(rustty_input::binding::AdjustSelection::Home),
            performable,
        );
        self.set.put_flags(
            phys(
                Key::End,
                Mods {
                    shift: true,
                    ..Mods::default()
                },
            ),
            Action::AdjustSelection(rustty_input::binding::AdjustSelection::End),
            performable,
        );

        // Tabs common to all platforms
        self.set.put(
            phys(
                Key::Tab,
                Mods {
                    ctrl: true,
                    shift: true,
                    ..Mods::default()
                },
            ),
            Action::PreviousTab,
        );
        self.set.put(
            phys(
                Key::Tab,
                Mods {
                    ctrl: true,
                    ..Mods::default()
                },
            ),
            Action::NextTab,
        );

        // Windowing
        if !cfg!(target_vendor = "apple") {
            self.set.put(
                uni(
                    'n',
                    Mods {
                        ctrl: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::NewWindow,
            );
            self.set.put(
                uni(
                    'w',
                    Mods {
                        ctrl: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::CloseSurface,
            );
            self.set.put(
                uni(
                    'q',
                    Mods {
                        ctrl: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::Quit,
            );
            self.set.put(
                phys(
                    Key::F4,
                    Mods {
                        alt: true,
                        ..Mods::default()
                    },
                ),
                Action::CloseWindow,
            );
            self.set.put(
                uni(
                    't',
                    Mods {
                        ctrl: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::NewTab,
            );
            self.set.put(
                uni(
                    'w',
                    Mods {
                        ctrl: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::CloseTab(rustty_input::binding::CloseTabMode::This),
            );
            self.set.put_flags(
                phys(
                    Key::ArrowLeft,
                    Mods {
                        ctrl: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::PreviousTab,
                performable,
            );
            self.set.put_flags(
                phys(
                    Key::ArrowRight,
                    Mods {
                        ctrl: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::NextTab,
                performable,
            );
            self.set.put_flags(
                phys(
                    Key::PageUp,
                    Mods {
                        ctrl: true,
                        ..Mods::default()
                    },
                ),
                Action::PreviousTab,
                performable,
            );
            self.set.put_flags(
                phys(
                    Key::PageDown,
                    Mods {
                        ctrl: true,
                        ..Mods::default()
                    },
                ),
                Action::NextTab,
                performable,
            );
            self.set.put(
                uni(
                    'o',
                    Mods {
                        ctrl: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::NewSplit(rustty_input::binding::SplitDirection::Right),
            );
            self.set.put(
                uni(
                    'e',
                    Mods {
                        ctrl: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::NewSplit(rustty_input::binding::SplitDirection::Down),
            );
            self.set.put_flags(
                uni(
                    '[',
                    Mods {
                        ctrl: true,
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::GotoSplit(rustty_input::binding::SplitFocusDirection::Previous),
                performable,
            );
            self.set.put_flags(
                uni(
                    ']',
                    Mods {
                        ctrl: true,
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::GotoSplit(rustty_input::binding::SplitFocusDirection::Next),
                performable,
            );
            self.set.put_flags(
                phys(
                    Key::ArrowUp,
                    Mods {
                        ctrl: true,
                        alt: true,
                        ..Mods::default()
                    },
                ),
                Action::GotoSplit(rustty_input::binding::SplitFocusDirection::Up),
                performable,
            );
            self.set.put_flags(
                phys(
                    Key::ArrowDown,
                    Mods {
                        ctrl: true,
                        alt: true,
                        ..Mods::default()
                    },
                ),
                Action::GotoSplit(rustty_input::binding::SplitFocusDirection::Down),
                performable,
            );
            self.set.put_flags(
                phys(
                    Key::ArrowLeft,
                    Mods {
                        ctrl: true,
                        alt: true,
                        ..Mods::default()
                    },
                ),
                Action::GotoSplit(rustty_input::binding::SplitFocusDirection::Left),
                performable,
            );
            self.set.put_flags(
                phys(
                    Key::ArrowRight,
                    Mods {
                        ctrl: true,
                        alt: true,
                        ..Mods::default()
                    },
                ),
                Action::GotoSplit(rustty_input::binding::SplitFocusDirection::Right),
                performable,
            );

            // Resizing splits
            self.set.put_flags(
                phys(
                    Key::ArrowUp,
                    Mods {
                        super_: true,
                        ctrl: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::ResizeSplit(rustty_input::binding::SplitResizeParameter(
                    rustty_input::binding::SplitResizeDirection::Up,
                    10,
                )),
                performable,
            );
            self.set.put_flags(
                phys(
                    Key::ArrowDown,
                    Mods {
                        super_: true,
                        ctrl: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::ResizeSplit(rustty_input::binding::SplitResizeParameter(
                    rustty_input::binding::SplitResizeDirection::Down,
                    10,
                )),
                performable,
            );
            self.set.put_flags(
                phys(
                    Key::ArrowLeft,
                    Mods {
                        super_: true,
                        ctrl: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::ResizeSplit(rustty_input::binding::SplitResizeParameter(
                    rustty_input::binding::SplitResizeDirection::Left,
                    10,
                )),
                performable,
            );
            self.set.put_flags(
                phys(
                    Key::ArrowRight,
                    Mods {
                        super_: true,
                        ctrl: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::ResizeSplit(rustty_input::binding::SplitResizeParameter(
                    rustty_input::binding::SplitResizeDirection::Right,
                    10,
                )),
                performable,
            );

            // Viewport scrolling
            self.set.put(
                phys(
                    Key::Home,
                    Mods {
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::ScrollToTop,
            );
            self.set.put(
                phys(
                    Key::End,
                    Mods {
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::ScrollToBottom,
            );
            self.set.put(
                phys(
                    Key::PageUp,
                    Mods {
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::ScrollPageUp,
            );
            self.set.put(
                phys(
                    Key::PageDown,
                    Mods {
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::ScrollPageDown,
            );

            // Semantic prompts
            self.set.put(
                phys(
                    Key::ArrowUp,
                    Mods {
                        shift: true,
                        ctrl: true,
                        ..Mods::default()
                    },
                ),
                Action::JumpToPrompt(-1),
            );
            self.set.put(
                phys(
                    Key::ArrowDown,
                    Mods {
                        shift: true,
                        ctrl: true,
                        ..Mods::default()
                    },
                ),
                Action::JumpToPrompt(1),
            );

            // Move tab
            self.set.put_flags(
                phys(
                    Key::PageUp,
                    Mods {
                        shift: true,
                        ctrl: true,
                        ..Mods::default()
                    },
                ),
                Action::MoveTab(-1),
                performable,
            );
            self.set.put_flags(
                phys(
                    Key::PageDown,
                    Mods {
                        shift: true,
                        ctrl: true,
                        ..Mods::default()
                    },
                ),
                Action::MoveTab(1),
                performable,
            );

            // Search
            self.set.put_flags(
                uni(
                    'f',
                    Mods {
                        ctrl: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::StartSearch,
                performable,
            );
            self.set.put_flags(
                phys(Key::Escape, Mods::default()),
                Action::EndSearch,
                performable,
            );

            // Inspector, matching Chromium
            self.set.put(
                uni(
                    'i',
                    Mods {
                        shift: true,
                        ctrl: true,
                        ..Mods::default()
                    },
                ),
                Action::Inspector(rustty_input::binding::InspectorMode::Toggle),
            );

            // Terminal
            self.set.put(
                uni(
                    'a',
                    Mods {
                        shift: true,
                        ctrl: true,
                        ..Mods::default()
                    },
                ),
                Action::SelectAll,
            );

            // Selection clipboard paste
            self.set.put(
                phys(
                    Key::Insert,
                    Mods {
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::PasteFromSelection,
            );
        }
        {
            // On macOS we default to super but everywhere else
            // is alt.
            let mods: Mods = if cfg!(target_vendor = "apple") {
                Mods {
                    super_: true,
                    ..Mods::default()
                }
            } else {
                Mods {
                    alt: true,
                    ..Mods::default()
                }
            };

            // Cmd/Alt+N for goto tab N
            // const start: u21 = '1';
            // const end: u21 = '8';
            // comptime var i: u21 = start; inline while (i <= end) : (i += 1) { ... }
            //
            // PORT NOTE: the reference uses `@field(inputpkg.Key,
            // comptimePrint("digit_{u}", .{i}))` to look up the physical key. Here
            // we enumerate the (char, physical-digit-key) pairs '1'..='8' directly,
            // preserving the same iteration order (and thus the same
            // last-binding-wins behavior).
            let start: u21 = '1' as u32;
            for (i, digit_key) in [
                ('1', Key::Digit1),
                ('2', Key::Digit2),
                ('3', Key::Digit3),
                ('4', Key::Digit4),
                ('5', Key::Digit5),
                ('6', Key::Digit6),
                ('7', Key::Digit7),
                ('8', Key::Digit8),
            ] {
                let icp: u21 = i as u32;
                // We register BOTH the physical `digit_N` key and the unicode
                // `N` key. This allows most keyboard layouts to work with
                // this shortcut. Namely, AZERTY doesn't produce unicode `N`
                // for their digit keys (they're on shifted keys on the same
                // physical keys).

                self.set.put_flags(
                    phys(digit_key, mods),
                    Action::GotoTab(((icp - start) + 1) as usize),
                    Flags {
                        // On macOS we keep this not performable so that the
                        // keyboard shortcuts in tabs work. In the future the
                        // correct fix is to fix the reverse mapping lookup
                        // to allow us to lookup performable keybinds
                        // conditionally.
                        performable: !cfg!(target_vendor = "apple"),
                        ..Flags::default()
                    },
                );

                // Important: this must be the LAST binding set so that the
                // libghostty trigger API returns this one for the action,
                // so that things like the macOS tab bar key equivalent label
                // work properly.
                self.set.put_flags(
                    uni(i, mods),
                    Action::GotoTab(((icp - start) + 1) as usize),
                    Flags {
                        performable: !cfg!(target_vendor = "apple"),
                        ..Flags::default()
                    },
                );
            }
            self.set.put_flags(
                uni('9', mods),
                Action::LastTab,
                Flags {
                    // See comment above with the numeric goto_tab
                    performable: !cfg!(target_vendor = "apple"),
                    ..Flags::default()
                },
            );
        }

        // Toggle fullscreen
        self.set.put(
            phys(Key::Enter, ctrl_or_super(Mods::default())),
            Action::ToggleFullscreen,
        );

        // Toggle zoom a split
        self.set.put(
            phys(
                Key::Enter,
                ctrl_or_super(Mods {
                    shift: true,
                    ..Mods::default()
                }),
            ),
            Action::ToggleSplitZoom,
        );

        // Toggle command palette, matches VSCode
        self.set.put(
            uni(
                'p',
                ctrl_or_super(Mods {
                    shift: true,
                    ..Mods::default()
                }),
            ),
            Action::ToggleCommandPalette,
        );

        // Mac-specific keyboard bindings.
        if cfg!(target_vendor = "apple") {
            self.set.put(
                uni(
                    'q',
                    Mods {
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::Quit,
            );
            self.set.put_flags(
                uni(
                    'k',
                    Mods {
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::ClearScreen,
                performable,
            );
            self.set.put(
                uni(
                    'a',
                    Mods {
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::SelectAll,
            );

            // Undo/redo
            self.set.put_flags(
                uni(
                    't',
                    Mods {
                        super_: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::Undo,
                performable,
            );
            self.set.put_flags(
                uni(
                    'z',
                    Mods {
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::Undo,
                performable,
            );
            self.set.put_flags(
                uni(
                    'z',
                    Mods {
                        super_: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::Redo,
                performable,
            );

            // Viewport scrolling
            self.set.put(
                phys(
                    Key::Home,
                    Mods {
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::ScrollToTop,
            );
            self.set.put(
                phys(
                    Key::End,
                    Mods {
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::ScrollToBottom,
            );
            self.set.put(
                phys(
                    Key::PageUp,
                    Mods {
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::ScrollPageUp,
            );
            self.set.put(
                phys(
                    Key::PageDown,
                    Mods {
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::ScrollPageDown,
            );
            self.set.put_flags(
                uni(
                    'j',
                    Mods {
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::ScrollToSelection,
                performable,
            );

            // Semantic prompts
            self.set.put(
                phys(
                    Key::ArrowUp,
                    Mods {
                        super_: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::JumpToPrompt(-1),
            );
            self.set.put(
                phys(
                    Key::ArrowDown,
                    Mods {
                        super_: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::JumpToPrompt(1),
            );

            // Mac windowing
            self.set.put(
                uni(
                    'n',
                    Mods {
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::NewWindow,
            );
            self.set.put(
                uni(
                    'w',
                    Mods {
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::CloseSurface,
            );
            self.set.put(
                uni(
                    'w',
                    Mods {
                        super_: true,
                        alt: true,
                        ..Mods::default()
                    },
                ),
                Action::CloseTab(rustty_input::binding::CloseTabMode::This),
            );
            self.set.put(
                uni(
                    'w',
                    Mods {
                        super_: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::CloseWindow,
            );
            self.set.put(
                uni(
                    'w',
                    Mods {
                        super_: true,
                        shift: true,
                        alt: true,
                        ..Mods::default()
                    },
                ),
                Action::CloseAllWindows,
            );
            self.set.put(
                uni(
                    't',
                    Mods {
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::NewTab,
            );
            self.set.put(
                uni(
                    '[',
                    Mods {
                        super_: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::PreviousTab,
            );
            self.set.put(
                uni(
                    ']',
                    Mods {
                        super_: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::NextTab,
            );
            self.set.put(
                uni(
                    'd',
                    Mods {
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::NewSplit(rustty_input::binding::SplitDirection::Right),
            );
            self.set.put(
                uni(
                    'd',
                    Mods {
                        super_: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::NewSplit(rustty_input::binding::SplitDirection::Down),
            );
            self.set.put(
                uni(
                    '[',
                    Mods {
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::GotoSplit(rustty_input::binding::SplitFocusDirection::Previous),
            );
            self.set.put(
                uni(
                    ']',
                    Mods {
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::GotoSplit(rustty_input::binding::SplitFocusDirection::Next),
            );
            self.set.put(
                phys(
                    Key::ArrowUp,
                    Mods {
                        super_: true,
                        alt: true,
                        ..Mods::default()
                    },
                ),
                Action::GotoSplit(rustty_input::binding::SplitFocusDirection::Up),
            );
            self.set.put(
                phys(
                    Key::ArrowDown,
                    Mods {
                        super_: true,
                        alt: true,
                        ..Mods::default()
                    },
                ),
                Action::GotoSplit(rustty_input::binding::SplitFocusDirection::Down),
            );
            self.set.put(
                phys(
                    Key::ArrowLeft,
                    Mods {
                        super_: true,
                        alt: true,
                        ..Mods::default()
                    },
                ),
                Action::GotoSplit(rustty_input::binding::SplitFocusDirection::Left),
            );
            self.set.put(
                phys(
                    Key::ArrowRight,
                    Mods {
                        super_: true,
                        alt: true,
                        ..Mods::default()
                    },
                ),
                Action::GotoSplit(rustty_input::binding::SplitFocusDirection::Right),
            );
            self.set.put(
                phys(
                    Key::ArrowUp,
                    Mods {
                        super_: true,
                        ctrl: true,
                        ..Mods::default()
                    },
                ),
                Action::ResizeSplit(rustty_input::binding::SplitResizeParameter(
                    rustty_input::binding::SplitResizeDirection::Up,
                    10,
                )),
            );
            self.set.put(
                phys(
                    Key::ArrowDown,
                    Mods {
                        super_: true,
                        ctrl: true,
                        ..Mods::default()
                    },
                ),
                Action::ResizeSplit(rustty_input::binding::SplitResizeParameter(
                    rustty_input::binding::SplitResizeDirection::Down,
                    10,
                )),
            );
            self.set.put(
                phys(
                    Key::ArrowLeft,
                    Mods {
                        super_: true,
                        ctrl: true,
                        ..Mods::default()
                    },
                ),
                Action::ResizeSplit(rustty_input::binding::SplitResizeParameter(
                    rustty_input::binding::SplitResizeDirection::Left,
                    10,
                )),
            );
            self.set.put(
                phys(
                    Key::ArrowRight,
                    Mods {
                        super_: true,
                        ctrl: true,
                        ..Mods::default()
                    },
                ),
                Action::ResizeSplit(rustty_input::binding::SplitResizeParameter(
                    rustty_input::binding::SplitResizeDirection::Right,
                    10,
                )),
            );
            self.set.put(
                uni(
                    '=',
                    Mods {
                        super_: true,
                        ctrl: true,
                        ..Mods::default()
                    },
                ),
                Action::EqualizeSplits,
            );

            // Jump to prompt, matches Terminal.app
            self.set.put(
                phys(
                    Key::ArrowUp,
                    Mods {
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::JumpToPrompt(-1),
            );
            self.set.put(
                phys(
                    Key::ArrowDown,
                    Mods {
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::JumpToPrompt(1),
            );

            // Search
            self.set.put_flags(
                uni(
                    'f',
                    Mods {
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::StartSearch,
                performable,
            );
            self.set.put_flags(
                uni(
                    'e',
                    Mods {
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::SearchSelection,
                performable,
            );
            self.set.put_flags(
                uni(
                    'f',
                    Mods {
                        super_: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::EndSearch,
                performable,
            );
            self.set.put_flags(
                phys(Key::Escape, Mods::default()),
                Action::EndSearch,
                performable,
            );
            self.set.put_flags(
                uni(
                    'g',
                    Mods {
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::NavigateSearch(rustty_input::binding::NavigateSearch::Next),
                performable,
            );
            self.set.put_flags(
                uni(
                    'g',
                    Mods {
                        super_: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::NavigateSearch(rustty_input::binding::NavigateSearch::Previous),
                performable,
            );

            // Inspector, matching Chromium
            self.set.put(
                uni(
                    'i',
                    Mods {
                        alt: true,
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::Inspector(rustty_input::binding::InspectorMode::Toggle),
            );

            // Alternate keybind, common to Mac programs
            self.set.put(
                uni(
                    'f',
                    Mods {
                        super_: true,
                        ctrl: true,
                        ..Mods::default()
                    },
                ),
                Action::ToggleFullscreen,
            );

            // Selection clipboard paste, matches Terminal.app
            self.set.put(
                uni(
                    'v',
                    Mods {
                        super_: true,
                        shift: true,
                        ..Mods::default()
                    },
                ),
                Action::PasteFromSelection,
            );

            // "Natural text editing" keybinds. This forces these keys to go back
            // to legacy encoding (not fixterms). It seems macOS users more than
            // others are used to these keys so we set them as defaults. If
            // people want to get back to the fixterm encoding they can set
            // the keybinds to `unbind`.
            self.set.put(
                phys(
                    Key::ArrowRight,
                    Mods {
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::Text(b"\\x05".to_vec()),
            );
            self.set.put(
                phys(
                    Key::ArrowLeft,
                    Mods {
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::Text(b"\\x01".to_vec()),
            );
            self.set.put(
                phys(
                    Key::Backspace,
                    Mods {
                        super_: true,
                        ..Mods::default()
                    },
                ),
                Action::Text(b"\\x15".to_vec()),
            );
            self.set.put(
                phys(
                    Key::ArrowLeft,
                    Mods {
                        alt: true,
                        ..Mods::default()
                    },
                ),
                Action::Esc(b"b".to_vec()),
            );
            self.set.put(
                phys(
                    Key::ArrowRight,
                    Mods {
                        alt: true,
                        ..Mods::default()
                    },
                ),
                Action::Esc(b"f".to_vec()),
            );
        }
    }

    // pub fn parseCLI(self: *Keybinds, alloc: Allocator, input: ?[]const u8) !void
    pub fn parse_cli(&mut self, input: Option<&[u8]>) -> Result<(), KeybindsParseError> {
        // var copy: ?[]u8 = null;
        // const value = value: {
        //     const value = input orelse return error.ValueRequired;
        //
        //     // If we don't have a colon, use the value as-is, no copy
        //     if (std.mem.indexOf(u8, value, ":") == null)
        //         break :value value;
        //
        //     // If we have a colon, we copy the whole value for now. We could
        //     // do this more efficiently later if we wanted to.
        //     const buf = try alloc.alloc(u8, value.len);
        //     copy = buf;
        //
        //     @memcpy(buf, value);
        //     break :value buf;
        // };
        // errdefer if (copy) |v| alloc.free(v);
        //
        // PORT NOTE: the reference conditionally copies `value` into the arena when
        // it contains a colon (so the parsed binding can reference arena-stable
        // bytes). In the Rust port the parse APIs take `&[u8]` and copy any bytes
        // they retain (owned `Vec<u8>` payloads), so no pre-copy / `errdefer free`
        // is needed; we operate on the borrowed `value` directly.
        let value = match input {
            // const value = input orelse return error.ValueRequired;
            None => return Err(KeybindsParseError::ValueRequired),
            Some(v) => v,
        };

        // Check for special values
        if value.is_empty() {
            // log.info("config has 'keybind =', using default keybinds", .{});
            self.init();
            return Ok(());
        }

        if value == b"clear" {
            // We don't clear the memory because its in the arena and unlikely
            // to be free-able anyways (since arenas can only clear the last
            // allocated value). This isn't a memory leak because the arena
            // will be freed when the config is freed.
            // log.info("config has 'keybind = clear', all keybinds cleared", .{});
            self.set = rustty_input::binding::Set::default();
            self.tables = rustty_zigstd::array_hash_map::AutoArrayHashMapUnmanaged::new();
            self.chain_target = KeybindsChainTarget::Root;
            return Ok(());
        }

        // Check for table syntax: "name/" or "name/binding"
        // We look for '/' only before the first '=' to avoid matching
        // action arguments like "foo=text:/hello".
        // const eq_idx = std.mem.indexOfScalar(u8, value, '=') orelse value.len;
        let eq_idx = value.iter().position(|&c| c == b'=').unwrap_or(value.len());
        // if (std.mem.indexOfScalar(u8, value[0..eq_idx], '/')) |slash_idx| table: {
        if let Some(slash_idx) = value[0..eq_idx].iter().position(|&c| c == b'/') {
            'table: {
                let table_name = &value[0..slash_idx];

                // Length zero is valid, so you can set `/=action` for the slash key
                if table_name.is_empty() {
                    break 'table;
                }

                // Ignore '+', '>' because they can be part of sequences and
                // triggers. This lets things like `ctrl+/=action` work.
                if table_name.iter().any(|&c| c == b'+' || c == b'>') {
                    break 'table;
                }

                let binding = &value[slash_idx + 1..];

                // Get or create the table
                // const gop = try self.tables.getOrPut(alloc, table_name);
                let gop = self
                    .tables
                    .get_or_put_with(table_name.to_vec(), rustty_input::binding::Set::default);
                if !gop.found_existing {
                    // We need to copy our table name into the arena
                    // for valid lookups later.
                    // gop.key_ptr.* = try alloc.dupe(u8, table_name);
                    // gop.value_ptr.* = .{};
                    //
                    // PORT NOTE: the key (`Vec<u8>`) is already owned by the map
                    // (`get_or_put_with` took an owned copy), so the `dupe` is
                    // implicit; the value was initialized to an empty `Set`.
                }

                // If there's no binding after the slash, this is a table
                // definition/clear command
                if binding.is_empty() {
                    // log.debug("config has 'keybind = {s}/', table cleared", .{table_name});
                    // gop.value_ptr.* = .{};
                    *self.tables.value_at_mut(gop.index) = rustty_input::binding::Set::default();
                    self.chain_target = KeybindsChainTarget::Root;
                    return Ok(());
                }

                // Chains are only allowed at the root level. Their target is
                // tracked globally by parse order in `self.chain_target`.
                if binding.starts_with(b"chain=") {
                    return Err(KeybindsParseError::InvalidFormat);
                }

                // Parse and add the binding to the table
                // try gop.value_ptr.parseAndPut(alloc, binding);
                self.tables.value_at_mut(gop.index).parse_and_put(binding)?;
                // self.chain_target = .{ .table = gop.key_ptr.* };
                self.chain_target =
                    KeybindsChainTarget::Table(self.tables.entry_at(gop.index).unwrap().0.clone());
                return Ok(());
            }
        }

        if value.starts_with(b"chain=") {
            match &self.chain_target {
                // .root => try self.set.parseAndPut(alloc, value),
                KeybindsChainTarget::Root => self.set.parse_and_put(value)?,
                // .table => |table_name| { ... }
                KeybindsChainTarget::Table(table_name) => {
                    // const table = self.tables.getPtr(table_name) orelse { ... };
                    let table_name = table_name.clone();
                    match self.tables.get_ptr(&table_name) {
                        None => {
                            self.chain_target = KeybindsChainTarget::Root;
                            return Err(KeybindsParseError::InvalidFormat);
                        }
                        Some(table) => table.parse_and_put(value)?,
                    }
                }
            }
            return Ok(());
        }

        // Parse into default set
        // try self.set.parseAndPut(alloc, value);
        self.set.parse_and_put(value)?;
        self.chain_target = KeybindsChainTarget::Root;
        Ok(())
    }

    /// Deep copy of the struct. Required by Config.
    // pub fn clone(self: *const Keybinds, alloc: Allocator) Allocator.Error!Keybinds
    pub fn clone_in(&self) -> Keybinds {
        // var tables: std.StringArrayHashMapUnmanaged(inputpkg.Binding.Set) = .empty;
        // try tables.ensureTotalCapacity(alloc, @intCast(self.tables.count()));
        let mut tables: rustty_zigstd::array_hash_map::AutoArrayHashMapUnmanaged<
            Vec<u8>,
            rustty_input::binding::Set,
        > = rustty_zigstd::array_hash_map::AutoArrayHashMapUnmanaged::new();
        // var it = self.tables.iterator();
        // while (it.next()) |entry| {
        for (key, value) in self.tables.iter() {
            // const key = try alloc.dupe(u8, entry.key_ptr.*);
            // tables.putAssumeCapacity(key, try entry.value_ptr.clone(alloc));
            tables.put(key.clone(), value.clone_set());
        }

        // return .{ .set = try self.set.clone(alloc), .tables = tables };
        //
        // PORT NOTE: the reference's returned struct leaves `chain_target` at its
        // default `.root` (it is not a copied field); the Rust `..Default::default()`
        // does the same.
        Keybinds {
            set: self.set.clone_set(),
            tables,
            ..Keybinds::default()
        }
    }

    /// Compare if two of our value are requal. Required by Config.
    // pub fn equal(self: Keybinds, other: Keybinds) bool
    pub fn equal(&self, other: &Keybinds) -> bool {
        // if (!equalSet(&self.set, &other.set)) return false;
        if !Keybinds::equal_set(&self.set, &other.set) {
            return false;
        }

        // Compare tables
        // if (self.tables.count() != other.tables.count()) return false;
        if self.tables.count() != other.tables.count() {
            return false;
        }

        // var it = self.tables.iterator();
        // while (it.next()) |entry| {
        for (key, value) in self.tables.iter() {
            // const other_set = other.tables.get(entry.key_ptr.*) orelse return false;
            let other_set = match other.tables.get(key) {
                Some(s) => s,
                None => return false,
            };
            // if (!equalSet(entry.value_ptr, &other_set)) return false;
            if !Keybinds::equal_set(value, other_set) {
                return false;
            }
        }

        true
    }

    // fn equalSet(self: *const inputpkg.Binding.Set, other: *const inputpkg.Binding.Set) bool
    fn equal_set(self_: &rustty_input::binding::Set, other: &rustty_input::binding::Set) -> bool {
        use rustty_input::binding::Value;

        // Two keybinds are considered equal if their primary bindings
        // are the same. We don't compare reverse mappings and such.
        // const self_map = &self.bindings;
        // const other_map = &other.bindings;

        // If the count of mappings isn't identical they can't be equal
        // if (self_map.count() != other_map.count()) return false;
        if self_.bindings_count() != other.bindings_count() {
            return false;
        }

        // var it = self_map.iterator();
        // while (it.next()) |self_entry| {
        for (self_key, self_value) in self_.iter_bindings() {
            // If the trigger isn't in the other map, they can't be equal
            // const other_entry = other_map.getEntry(self_entry.key_ptr.*) orelse return false;
            let other_value = match other.get_value(*self_key) {
                Some(v) => v,
                None => return false,
            };

            // If the entry types are different, they can't be equal
            // if (std.meta.activeTag(self_entry.value_ptr.*) !=
            //     std.meta.activeTag(other_entry.value_ptr.*)) return false;
            if std::mem::discriminant(self_value) != std::mem::discriminant(other_value) {
                return false;
            }

            match (self_value, other_value) {
                // They're equal if both leader sets are equal.
                // .leader => if (!equalSet(self_entry.value_ptr.*.leader,
                //                          other_entry.value_ptr.*.leader)) return false,
                (Value::Leader(self_leader), Value::Leader(other_leader)) => {
                    if !Keybinds::equal_set(self_leader, other_leader) {
                        return false;
                    }
                }

                // Actions are compared by field directly
                // .leaf => { const self_leaf = ...; const other_leaf = ...;
                //     if (!deepEqual(Set.Leaf, self_leaf, other_leaf)) return false; },
                //
                // PORT NOTE: `deepEqual(Set.Leaf, ...)` deep-compares both fields of
                // the leaf (`action` and `flags`). `Action::equal` is the action
                // deep-compare; `Flags` is a copy type compared by `==`.
                (Value::Leaf(self_leaf), Value::Leaf(other_leaf)) => {
                    if !(self_leaf.action.equal(&other_leaf.action)
                        && self_leaf.flags == other_leaf.flags)
                    {
                        return false;
                    }
                }

                // .leaf_chained => { ... }
                (Value::LeafChained(self_chain), Value::LeafChained(other_chain)) => {
                    // if (self_chain.flags != other_chain.flags) return false;
                    if self_chain.flags != other_chain.flags {
                        return false;
                    }
                    // if (self_chain.actions.items.len != other_chain.actions.items.len) return false;
                    if self_chain.actions.len() != other_chain.actions.len() {
                        return false;
                    }
                    // for (self_chain.actions.items, other_chain.actions.items) |a1, a2| {
                    //     if (!deepEqual(Action, a1, a2)) return false; }
                    for (a1, a2) in self_chain.actions.iter().zip(other_chain.actions.iter()) {
                        if !a1.equal(a2) {
                            return false;
                        }
                    }
                }

                // The active tags matched above, so any remaining pairing is
                // impossible; preserved as the exhaustive match arm.
                _ => unreachable!(),
            }
        }

        true
    }

    /// Like formatEntry but has an option to include docs.
    // pub fn formatEntryDocs(self: Keybinds, formatter: formatterpkg.EntryFormatter, docs: bool) !void
    //
    // PORT NOTE: the reference threads `formatterpkg.EntryFormatter` and writes a
    // top-level `{f}` trigger prefix into a fixed buffer before delegating to
    // `v.formatEntries(&writer, formatter)`. The Rust `EntryFormatter` + the
    // `Binding.Set` value's `format_entries(&mut RewindBuffer, formatter)` provide
    // exactly these pieces; the trigger/table-name prefix is written into the
    // shared `RewindBuffer` via the public `write_trigger_to_buffer`/`write_all`
    // helpers. The `docs` branch's `help_strings.KeybindAction` lookup is the
    // build-generated help artifact (modeled by `helpgen_actions::HelpStrings`);
    // `RealHelpStrings` delegates to the generated `keybind_help` table so
    // `docs = true` (list-keybinds/show-config `--docs`) emits the real per-action
    // doc text. The ported tests only exercise `formatEntry` (i.e. `docs = false`),
    // where the table is not consulted.
    pub fn format_entry_docs<W: Writer>(
        &self,
        formatter: &mut rustty_cfg_fmt::EntryFormatter<W>,
        docs: bool,
    ) -> Result<(), W::Error> {
        use rustty_input::binding::{write_trigger_to_buffer, RewindBuffer};
        use rustty_input::helpgen_actions::{HelpStrings, RealHelpStrings};

        // if (self.set.bindings.count() == 0 and self.tables.count() == 0) {
        //     try formatter.formatEntry(void, {});
        //     return;
        // }
        if self.set.bindings_count() == 0 && self.tables.count() == 0 {
            // try formatter.formatEntry(void, {});
            return format_entry_void(formatter.name, formatter.writer);
        }

        // var buf: [1024]u8 = undefined;
        //
        // PORT NOTE: the reference's fixed 1024-byte stack buffer is the growable
        // `RewindBuffer` (its `format_entries` callers rewind via the `end` cursor).
        let help_strings = RealHelpStrings;

        // Format root set bindings
        // var iter = self.set.bindings.iterator();
        // while (iter.next()) |next| {
        for (k, v) in self.set.iter_bindings() {
            // const k = next.key_ptr.*;
            // const v = next.value_ptr.*;
            if docs {
                // try formatter.writer.writeAll("\n");
                formatter.writer.write_all(b"\n")?;
                // const name = @tagName(v);
                let name = v_tag_name(v);
                // inline for (@typeInfo(help_strings.KeybindAction).@"struct".decls) |decl| {
                //     if (std.mem.eql(u8, decl.name, name)) { ... break; } }
                for decl_name in rustty_input::binding::Action::FIELD_NAMES {
                    if decl_name.as_bytes() == name && help_strings.has_decl(decl_name) {
                        // const help = @field(help_strings.KeybindAction, decl.name);
                        let help = help_strings.field(decl_name);
                        // try formatter.writer.writeAll("# " ++ decl.name ++ "\n");
                        formatter.writer.write_all(b"# ")?;
                        formatter.writer.write_all(decl_name.as_bytes())?;
                        formatter.writer.write_all(b"\n")?;
                        // var lines = std.mem.splitScalar(u8, help, '\n');
                        // while (lines.next()) |line| { ... }
                        for line in help.split(|&c| c == b'\n') {
                            // try formatter.writer.writeAll("#   ");
                            formatter.writer.write_all(b"#   ")?;
                            // try formatter.writer.writeAll(line);
                            formatter.writer.write_all(line)?;
                            // try formatter.writer.writeAll("\n");
                            formatter.writer.write_all(b"\n")?;
                        }
                        break;
                    }
                }
            }

            // var writer: std.Io.Writer = .fixed(&buf);
            // writer.print("{f}", .{k}) catch return error.OutOfMemory;
            let mut writer = RewindBuffer::new();
            write_trigger_to_buffer(k, &mut writer).map_err(|_| ()).ok();
            // try v.formatEntries(&writer, formatter);
            format_entries_into(v, &mut writer, formatter)?;
        }

        // Format table bindings
        // var table_iter = self.tables.iterator();
        // while (table_iter.next()) |table_entry| {
        for (table_name, table_set) in self.tables.iter() {
            // const table_name = table_entry.key_ptr.*;
            // const table_set = table_entry.value_ptr.*;

            // var binding_iter = table_set.bindings.iterator();
            // while (binding_iter.next()) |next| {
            for (k, v) in table_set.iter_bindings() {
                // const k = next.key_ptr.*;
                // const v = next.value_ptr.*;

                // var writer: std.Io.Writer = .fixed(&buf);
                // writer.print("{s}/{f}", .{ table_name, k }) catch return error.OutOfMemory;
                let mut writer = RewindBuffer::new();
                writer.write_all(table_name).ok();
                writer.write_all(b"/").ok();
                write_trigger_to_buffer(k, &mut writer).ok();
                // try v.formatEntries(&writer, formatter);
                format_entries_into(v, &mut writer, formatter)?;
            }
        }

        Ok(())
    }

    /// Used by Formatter
    // pub fn formatEntry(self: Keybinds, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry<W: Writer>(
        &self,
        formatter: &mut rustty_cfg_fmt::EntryFormatter<W>,
    ) -> Result<(), W::Error> {
        // try self.formatEntryDocs(formatter, false);
        self.format_entry_docs(formatter, false)
    }
}

// PORT NOTE: `@tagName(v)` where `v` is the leaf/leaf_chained `Action`. The docs
// branch only formats single-action leaves' tag names; for leader / chained
// entries the reference would print the (single) action tag of the value, but the
// docs branch is only reached for non-empty sets and uses the value's tag. Here
// we expose the action tag of a leaf value (the only kind whose `@tagName` the
// reference's docs path meaningfully matches against `help_strings` decls).
fn v_tag_name(v: &rustty_input::binding::Value) -> &[u8] {
    use rustty_input::binding::Value;
    match v {
        Value::Leaf(leaf) => leaf.action.tag_name().as_bytes(),
        Value::LeafChained(chain) => chain
            .actions
            .first()
            .map(|a| a.tag_name().as_bytes())
            .unwrap_or(b""),
        // A leader's value is a sub-set; it has no single action tag.
        Value::Leader(_) => b"",
    }
}

// PORT NOTE: adapter for `v.formatEntries(&writer, formatter)` — the reference
// passes the fixed writer + formatter and bubbles `error.OutOfMemory`/writer
// errors. The Rust `Value::format_entries` returns `FormatEntriesError<W::Error>`;
// the `OutOfMemory` arm cannot occur for the growable `RewindBuffer`, so it is
// mapped through (unreachable in practice) and the writer error is surfaced.
fn format_entries_into<W: Writer>(
    v: &rustty_input::binding::Value,
    buffer: &mut rustty_input::binding::RewindBuffer,
    formatter: &mut rustty_cfg_fmt::EntryFormatter<W>,
) -> Result<(), W::Error> {
    use rustty_input::binding::FormatEntriesError;
    match v.format_entries(buffer, formatter) {
        Ok(()) => Ok(()),
        Err(FormatEntriesError::Writer(e)) => Err(e),
        // The growable RewindBuffer never overflows, so OutOfMemory is unreachable.
        Err(FormatEntriesError::OutOfMemory) => unreachable!(
            "RewindBuffer is growable; the reference's fixed-buffer OutOfMemory cannot occur"
        ),
    }
}

// pub const RepeatableCodepointMap = struct { map: fontpkg.CodepointMap };
#[derive(Debug, Default)]
pub struct RepeatableCodepointMap {
    pub map: CodepointMap,
}

// PORT NOTE (R4): `RepeatableCodepointMap` wraps `fontpkg.CodepointMap`
// (`font/CodepointMap.zig`), ported PURE in rustty-font. Its `parseCLI`/`clone`/
// `equal`/`formatEntry` operate on that font map's `add`/`entries`/`descriptor`
// API. The shared `UnicodeRangeParser` is hoisted to module scope (also consumed
// by `RepeatableClipboardCodepointMap`). rustty-font's `CodepointMap` carries no
// C/vt edge (the fontconfig/CoreText discovery lands later in the shaper tier);
// the discovery-descriptor seam here only stores `family` + `monospace`.
impl RepeatableCodepointMap {
    // pub fn parseCLI(self: *Self, alloc: Allocator, input_: ?[]const u8) !void
    pub fn parse_cli(&mut self, input_: Option<&[u8]>) -> Result<(), ParseError> {
        use rustty_font::codepoint_map::{Descriptor, Entry, Global};
        // const input = input_ orelse return error.ValueRequired;
        let input = input_.ok_or(Error::ValueRequired)?;
        // const eql_idx = std.mem.indexOf(u8, input, "=") orelse return error.InvalidValue;
        let eql_idx = index_of(input, b'=').ok_or(Error::InvalidValue)?;
        // const whitespace = " \t";
        // const key = std.mem.trim(u8, input[0..eql_idx], whitespace);
        let key = trim_ws(&input[0..eql_idx]);
        // const value = std.mem.trim(u8, input[eql_idx + 1 ..], whitespace);
        let value = trim_ws(&input[eql_idx + 1..]);
        // const valueZ = try alloc.dupeZ(u8, value);
        //
        // PORT NOTE: the reference dupes `value` once and shares the SAME `valueZ`
        // pointer across every range's descriptor. The seam's `family` is an owned
        // `Vec<u8>` (byte-slice fidelity — no sentinel), so each entry gets its own
        // content-identical copy; the shared-pointer identity is unobservable here
        // (it only fed `std.meta.eql`'s pointer compare in `equal`, ported by
        // content below).
        let value_z: std::vec::Vec<u8> = value.to_vec();

        // var p: UnicodeRangeParser = .{ .input = key };
        let mut p = UnicodeRangeParser { input: key, i: 0 };
        // while (try p.next()) |range| {
        while let Some(range) = p.next()? {
            // try self.map.add(alloc, .{
            //     .range = range,
            //     .descriptor = .{ .family = valueZ, .monospace = false },
            // });
            //
            // PORT NOTE: `add` returns `OutOfMemory`; per the config allocation
            // model (Global aborts on OOM — `OutOfMemory` is not a recoverable
            // config error), this is `.expect`-ed rather than surfaced.
            self.map
                .add(
                    Global,
                    Entry {
                        range,
                        descriptor: Descriptor {
                            family: Some(value_z.clone()),
                            // we allow any font
                            monospace: false,
                        },
                    },
                )
                .expect("CodepointMap.add: Global allocator aborts on OOM");
        }
        Ok(())
    }

    // pub fn clone(self: *const Self, alloc: Allocator) Allocator.Error!Self {
    //     return .{ .map = try self.map.clone(alloc) };
    // }
    //
    // PORT NOTE: the reference returns a deep copy of `self.map` (the font
    // `CodepointMap`). Required by `Config.clone` (`cloneValue`, dispatched via
    // the `ci!`/`@hasDecl(T, "clone")` arm). The font map's `clone` returns
    // `OutOfMemory`; per the config allocation model (Global aborts on OOM) it is
    // `.expect`-ed, matching every other infallible `clone_in`.
    pub fn clone_in(&self) -> RepeatableCodepointMap {
        use rustty_font::codepoint_map::Global;
        // return .{ .map = try self.map.clone(alloc) };
        RepeatableCodepointMap {
            map: self
                .map
                .clone(Global)
                .expect("CodepointMap.clone: Global allocator aborts on OOM"),
        }
    }

    // pub fn equal(self: Self, other: Self) bool {
    //     const itemsA = self.map.list.slice();
    //     const itemsB = other.map.list.slice();
    //     if (itemsA.len != itemsB.len) return false;
    //     for (0..itemsA.len) |i| {
    //         const a = itemsA.get(i);
    //         const b = itemsB.get(i);
    //         if (!std.meta.eql(a, b)) return false;
    //     } else return true;
    // }
    //
    // PORT NOTE: the reference compares the two maps' entry lists (length, then
    // `std.meta.eql` per entry). Required by `Config.changed` (`deepEqual`).
    //
    // std.meta.eql on the `Entry`/`Descriptor` structs compares `range` element-
    // wise, `descriptor.monospace` directly, and `descriptor.family` (a slice) by
    // POINTER+LEN identity — NOT by content. Reproducing Zig's slice-pointer
    // identity across the port is neither possible nor meaningful (the seam's
    // `family` is an owned `Vec<u8>`); we compare `family` by CONTENT, which is the
    // semantically-intended equality for `changed`/`deepEqual` (two configs with
    // the same codepoint map should compare equal). This is a deliberate,
    // documented divergence from `std.meta.eql`'s pointer semantics.
    pub fn equal(&self, other: &RepeatableCodepointMap) -> bool {
        // const itemsA = self.map.list.slice();
        // const itemsB = other.map.list.slice();
        let items_a = self.map.entries();
        let items_b = other.map.entries();
        // if (itemsA.len != itemsB.len) return false;
        if items_a.len() != items_b.len() {
            return false;
        }
        // for (0..itemsA.len) |i| {
        for i in 0..items_a.len() {
            // const a = itemsA.get(i);
            // const b = itemsB.get(i);
            let a = &items_a[i];
            let b = &items_b[i];
            // if (!std.meta.eql(a, b)) return false;
            if a.range != b.range
                || a.descriptor.family != b.descriptor.family
                || a.descriptor.monospace != b.descriptor.monospace
            {
                return false;
            }
        }
        // } else return true;
        true
    }

    // pub fn formatEntry(self: Self, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry<W: Writer>(&self, name: &[u8], writer: &mut W) -> Result<(), W::Error> {
        // if (self.map.list.len == 0) {
        if self.map.is_empty() {
            // try formatter.formatEntry(void, {});
            // return;
            return format_entry_void(name, writer);
        }

        // var buf: [1024]u8 = undefined;
        // const ranges = self.map.list.items(.range);
        // const descriptors = self.map.list.items(.descriptor);
        // for (ranges, descriptors) |range, descriptor| {
        for entry in self.map.entries() {
            let range = entry.range;
            // descriptor.family orelse ""
            let family: &[u8] = entry.descriptor.family.as_deref().unwrap_or(b"");

            // if (range[0] == range[1]) {
            //     try formatter.formatEntry([]const u8, std.fmt.bufPrint(&buf, "U+{X:0>4}={s}", .{ range[0], descriptor.family orelse "" }) catch ...);
            // } else {
            //     try formatter.formatEntry([]const u8, std.fmt.bufPrint(&buf, "U+{X:0>4}-U+{X:0>4}={s}", .{ range[0], range[1], descriptor.family orelse "" }) catch ...);
            // }
            let mut s: Vec<u8> = Vec::new();
            if range[0] == range[1] {
                s.extend_from_slice(format!("U+{:04X}=", range[0]).as_bytes());
                s.extend_from_slice(family);
            } else {
                s.extend_from_slice(format!("U+{:04X}-U+{:04X}=", range[0], range[1]).as_bytes());
                s.extend_from_slice(family);
            }
            format_entry_string(name, &s, writer)?;
        }
        Ok(())
    }
}

impl FormatEntry for RepeatableCodepointMap {
    fn format_entry<W: Writer>(&self, name: &[u8], writer: &mut W) -> Result<(), W::Error> {
        RepeatableCodepointMap::format_entry(self, name, writer)
    }
}

// pub const RepeatableClipboardCodepointMap = struct { map: ClipboardCodepointMap };
#[derive(Default)]
pub struct RepeatableClipboardCodepointMap {
    pub map: crate::ClipboardCodepointMap,
}

impl RepeatableClipboardCodepointMap {
    // pub fn parseCLI(self: *Self, alloc: Allocator, input_: ?[]const u8) !void
    pub fn parse_cli(&mut self, input_: Option<&[u8]>) -> Result<(), ParseError> {
        use rustty_vt::formatter::Replacement;
        // const input = input_ orelse return error.ValueRequired;
        let input = input_.ok_or(Error::ValueRequired)?;
        // const eql_idx = std.mem.indexOf(u8, input, "=") orelse return error.InvalidValue;
        let eql_idx = index_of(input, b'=').ok_or(Error::InvalidValue)?;
        // const whitespace = " \t";
        // const key = std.mem.trim(u8, input[0..eql_idx], whitespace);
        let key = trim_ws(&input[0..eql_idx]);
        // const value = std.mem.trim(u8, input[eql_idx + 1 ..], whitespace);
        let value = trim_ws(&input[eql_idx + 1..]);

        // Parse the replacement value - either a codepoint or string
        // const replacement: ClipboardCodepointMap.Replacement = if (std.mem.startsWith(u8, value, "U+")) blk: {
        let replacement: Replacement = if value.starts_with(b"U+") {
            // Parse as codepoint
            // const cp_str = value[2..]; // Skip "U+"
            let cp_str = &value[2..];
            // const cp = std.fmt.parseInt(u21, cp_str, 16) catch return error.InvalidValue;
            let cp = parse_int_u21_base16(cp_str).map_err(|_| Error::InvalidValue)?;
            // break :blk .{ .codepoint = cp };
            Replacement::Codepoint(cp)
        } else {
            // Parse as UTF-8 string - validate it's valid UTF-8
            // if (!std.unicode.utf8ValidateSlice(value)) return error.InvalidValue;
            if core::str::from_utf8(value).is_err() {
                return Err(Error::InvalidValue);
            }
            // const value_copy = try alloc.dupe(u8, value);
            // break :blk .{ .string = value_copy };
            Replacement::String(value.to_vec())
        };

        // var p: UnicodeRangeParser = .{ .input = key };
        // while (try p.next()) |range| {
        //     try self.map.add(alloc, .{ .range = range, .replacement = replacement });
        // }
        let mut p = UnicodeRangeParser { input: key, i: 0 };
        while let Some(range) = p.next()? {
            self.map.add(crate::clipboard_codepoint_map::Entry {
                range,
                // PORT NOTE: `Replacement` is not `Clone`; clone its bytes by hand.
                replacement: match &replacement {
                    Replacement::Codepoint(cp) => Replacement::Codepoint(*cp),
                    Replacement::String(s) => Replacement::String(s.clone()),
                },
            });
        }
        Ok(())
    }

    /// Deep copy of the struct. Required by Config.
    // pub fn clone(self: *const Self, alloc: Allocator) Allocator.Error!Self
    pub fn clone_in(&self) -> RepeatableClipboardCodepointMap {
        // return .{ .map = try self.map.clone(alloc) };
        RepeatableClipboardCodepointMap {
            map: self.map.clone(),
        }
    }

    /// Compare if two of our value are equal. Required by Config.
    // pub fn equal(self: Self, other: Self) bool
    pub fn equal(&self, other: &RepeatableClipboardCodepointMap) -> bool {
        use rustty_vt::formatter::Replacement;
        // const itemsA = self.map.list.slice();
        // const itemsB = other.map.list.slice();
        let items_a = &self.map.list;
        let items_b = &other.map.list;
        // if (itemsA.len != itemsB.len) return false;
        if items_a.len() != items_b.len() {
            return false;
        }
        // for (0..itemsA.len) |i| {
        for i in 0..items_a.len() {
            // const a = itemsA.get(i);
            // const b = itemsB.get(i);
            let a = &items_a[i];
            let b = &items_b[i];
            // if (!std.meta.eql(a.range, b.range)) return false;
            if a.range != b.range {
                return false;
            }
            // switch (a.replacement) {
            match &a.replacement {
                // .codepoint => |cp_a| switch (b.replacement) {
                //     .codepoint => |cp_b| if (cp_a != cp_b) return false,
                //     .string => return false,
                // },
                Replacement::Codepoint(cp_a) => match &b.replacement {
                    Replacement::Codepoint(cp_b) => {
                        if cp_a != cp_b {
                            return false;
                        }
                    }
                    Replacement::String(_) => return false,
                },
                // .string => |str_a| switch (b.replacement) {
                //     .string => |str_b| if (!std.mem.eql(u8, str_a, str_b)) return false,
                //     .codepoint => return false,
                // },
                Replacement::String(str_a) => match &b.replacement {
                    Replacement::String(str_b) => {
                        if str_a != str_b {
                            return false;
                        }
                    }
                    Replacement::Codepoint(_) => return false,
                },
            }
        }
        // return true;
        true
    }

    /// Used by Formatter
    // pub fn formatEntry(self: Self, formatter: anytype) !void
    pub fn format_entry<W: Writer>(&self, name: &[u8], writer: &mut W) -> Result<(), W::Error> {
        use rustty_vt::formatter::Replacement;
        // if (self.map.list.len == 0) { try formatter.formatEntry(void, {}); return; }
        if self.map.list.is_empty() {
            return format_entry_void(name, writer);
        }

        // var buf: [1024]u8 = undefined;
        // var value_buf: [32]u8 = undefined;
        // const ranges = self.map.list.items(.range);
        // const replacements = self.map.list.items(.replacement);
        // for (ranges, replacements) |range, replacement| {
        for entry in &self.map.list {
            let range = entry.range;
            // const value_str = switch (replacement) {
            //     .codepoint => |cp| try std.fmt.bufPrint(&value_buf, "U+{X:0>4}", .{cp}),
            //     .string => |s| s,
            // };
            let value_str: Vec<u8> = match &entry.replacement {
                Replacement::Codepoint(cp) => format!("U+{:04X}", cp).into_bytes(),
                Replacement::String(s) => s.clone(),
            };

            // if (range[0] == range[1]) {
            //     try formatter.formatEntry([]const u8, std.fmt.bufPrint(&buf, "U+{X:0>4}={s}", .{ range[0], value_str }) catch ...);
            // } else {
            //     try formatter.formatEntry([]const u8, std.fmt.bufPrint(&buf, "U+{X:0>4}-U+{X:0>4}={s}", .{ range[0], range[1], value_str }) catch ...);
            // }
            let mut s: Vec<u8> = Vec::new();
            if range[0] == range[1] {
                s.extend_from_slice(format!("U+{:04X}=", range[0]).as_bytes());
                s.extend_from_slice(&value_str);
            } else {
                s.extend_from_slice(format!("U+{:04X}-U+{:04X}=", range[0], range[1]).as_bytes());
                s.extend_from_slice(&value_str);
            }
            format_entry_string(name, &s, writer)?;
        }
        Ok(())
    }
}

impl FormatEntry for RepeatableClipboardCodepointMap {
    fn format_entry<W: Writer>(&self, name: &[u8], writer: &mut W) -> Result<(), W::Error> {
        RepeatableClipboardCodepointMap::format_entry(self, name, writer)
    }
}

/// PORTS: `std.fmt.parseInt(u21, str, 16)` — a 21-bit unsigned hex parse.
fn parse_int_u21_base16(input: &[u8]) -> Result<u21, ParseError> {
    if input.is_empty() {
        return Err(Error::InvalidValue);
    }
    let mut acc: u32 = 0;
    for &c in input {
        let digit = match c {
            b'0'..=b'9' => (c - b'0') as u32,
            b'a'..=b'f' => (c - b'a' + 10) as u32,
            b'A'..=b'F' => (c - b'A' + 10) as u32,
            _ => return Err(Error::InvalidValue),
        };
        acc = acc
            .checked_mul(16)
            .and_then(|v| v.checked_add(digit))
            .ok_or(Error::InvalidValue)?;
        // u21 max is 0x1F_FFFF.
        if acc > 0x1F_FFFF {
            return Err(Error::InvalidValue);
        }
    }
    Ok(acc)
}

/// Parses the list of Unicode codepoint ranges. Valid syntax:
///
///   "" (empty returns null)
///   U+1234
///   U+1234-5678
///   U+1234,U+5678
///   U+1234-5678,U+5678
///   U+1234,U+5678-U+9ABC
///
/// etc.
//
// Reuse the same UnicodeRangeParser from RepeatableCodepointMap
//
// PORT NOTE: this is `RepeatableCodepointMap.UnicodeRangeParser` (the reference
// defines it inside `RepeatableCodepointMap` and `RepeatableClipboardCodepointMap`
// reuses it via `const UnicodeRangeParser = RepeatableCodepointMap.UnicodeRangeParser;`).
// `RepeatableCodepointMap` itself is a deferred DI seam (it needs the not-yet-
// ported `fontpkg.CodepointMap`), so the shared parser is hoisted to module
// scope here and consumed by `RepeatableClipboardCodepointMap`.
// const UnicodeRangeParser = struct { input: []const u8, i: usize = 0 };
struct UnicodeRangeParser<'a> {
    input: &'a [u8],
    i: usize,
}

impl UnicodeRangeParser<'_> {
    // pub fn next(self: *UnicodeRangeParser) !?[2]u21 {
    fn next(&mut self) -> Result<Option<[u21; 2]>, ParseError> {
        // Once we're EOF then we're done without an error.
        // if (self.eof()) return null;
        if self.eof() {
            return Ok(None);
        }

        // One codepoint no matter what
        // const start = try self.parseCodepoint();
        let start = self.parse_codepoint()?;
        // if (self.eof()) return .{ start, start };
        if self.eof() {
            return Ok(Some([start, start]));
        }

        // We're allowed to have any whitespace here
        // self.consumeWhitespace();
        self.consume_whitespace();

        // Otherwise we expect either a range or a comma
        // switch (self.input[self.i]) {
        match self.input[self.i] {
            // Comma means we have another codepoint but in a different
            // range so we return our current codepoint.
            // ',' => {
            //     self.advance();
            //     self.consumeWhitespace();
            //     if (self.eof()) return error.InvalidValue;
            //     return .{ start, start };
            // },
            b',' => {
                self.advance();
                self.consume_whitespace();
                if self.eof() {
                    return Err(Error::InvalidValue);
                }
                Ok(Some([start, start]))
            }

            // Hyphen means we have a range.
            // '-' => {
            //     self.advance();
            //     self.consumeWhitespace();
            //     if (self.eof()) return error.InvalidValue;
            //     const end = try self.parseCodepoint();
            //     self.consumeWhitespace();
            //     if (!self.eof() and self.input[self.i] != ',') return error.InvalidValue;
            //     self.advance();
            //     self.consumeWhitespace();
            //     if (start > end) return error.InvalidValue;
            //     return .{ start, end };
            // },
            b'-' => {
                self.advance();
                self.consume_whitespace();
                if self.eof() {
                    return Err(Error::InvalidValue);
                }
                let end = self.parse_codepoint()?;
                self.consume_whitespace();
                if !self.eof() && self.input[self.i] != b',' {
                    return Err(Error::InvalidValue);
                }
                self.advance();
                self.consume_whitespace();
                if start > end {
                    return Err(Error::InvalidValue);
                }
                Ok(Some([start, end]))
            }

            // else => return error.InvalidValue,
            _ => Err(Error::InvalidValue),
        }
    }

    // fn consumeWhitespace(self: *UnicodeRangeParser) void {
    fn consume_whitespace(&mut self) {
        // while (!self.eof()) {
        while !self.eof() {
            // switch (self.input[self.i]) {
            match self.input[self.i] {
                // ' ', '\t' => self.advance(),
                b' ' | b'\t' => self.advance(),
                // else => return,
                _ => return,
            }
        }
    }

    // fn parseCodepoint(self: *UnicodeRangeParser) !u21 {
    fn parse_codepoint(&mut self) -> Result<u21, ParseError> {
        // if (self.input[self.i] != 'U') return error.InvalidValue;
        if self.input[self.i] != b'U' {
            return Err(Error::InvalidValue);
        }
        // self.advance();
        self.advance();
        // if (self.eof()) return error.InvalidValue;
        if self.eof() {
            return Err(Error::InvalidValue);
        }
        // if (self.input[self.i] != '+') return error.InvalidValue;
        if self.input[self.i] != b'+' {
            return Err(Error::InvalidValue);
        }
        // self.advance();
        self.advance();
        // if (self.eof()) return error.InvalidValue;
        if self.eof() {
            return Err(Error::InvalidValue);
        }

        // const start_i = self.i;
        let start_i = self.i;
        // while (true) {
        loop {
            // const current = self.input[self.i];
            let current = self.input[self.i];
            // const is_hex = (current >= '0' and current <= '9') or (current >= 'A' and current <= 'F') or (current >= 'a' and current <= 'f');
            let is_hex = current.is_ascii_digit()
                || (b'A'..=b'F').contains(&current)
                || (b'a'..=b'f').contains(&current);
            // if (!is_hex) break;
            if !is_hex {
                break;
            }

            // Advance but break on EOF
            // self.advance();
            self.advance();
            // if (self.eof()) break;
            if self.eof() {
                break;
            }
        }

        // If we didn't consume a single character, we have an error.
        // if (start_i == self.i) return error.InvalidValue;
        if start_i == self.i {
            return Err(Error::InvalidValue);
        }

        // return std.fmt.parseInt(u21, self.input[start_i..self.i], 16) catch return error.InvalidValue;
        parse_int_u21_base16(&self.input[start_i..self.i]).map_err(|_| Error::InvalidValue)
    }

    // fn advance(self: *UnicodeRangeParser) void { self.i += 1; }
    fn advance(&mut self) {
        self.i += 1;
    }

    // fn eof(self: *const UnicodeRangeParser) bool { return self.i >= self.input.len; }
    fn eof(&self) -> bool {
        self.i >= self.input.len()
    }
}

/// See "link" for documentation.
// pub const RepeatableLink = struct { links: ArrayListUnmanaged(inputpkg.Link) };
#[derive(Debug, Clone, Default)]
pub struct RepeatableLink {
    pub links: Vec<rustty_input::link::Link>,
}

impl RepeatableLink {
    // pub fn parseCLI(self: *Self, alloc: Allocator, input_: ?[]const u8) !void
    pub fn parse_cli(&mut self, _input: Option<&[u8]>) -> Result<(), ParseError> {
        // _ = self; _ = alloc; _ = input_;
        // return error.NotImplemented;
        //
        // PORT NOTE: the reference's `parseCLI` is an explicit stub that always
        // returns `error.NotImplemented` ("This currently can't be set"). The base
        // args `Error` set has no `NotImplemented`; the link field is never
        // settable, so the error collapses to `InvalidValue` (the closest base
        // variant) — preserving the "cannot be set" behavior.
        Err(Error::InvalidValue)
    }

    /// Deep copy of the struct. Required by Config.
    // pub fn clone(self: *const Self, alloc: Allocator) Allocator.Error!Self
    pub fn clone_in(&self) -> RepeatableLink {
        // Note: we don't do any errdefers below since the allocation
        // is expected to be arena allocated.
        // var list = try std.ArrayListUnmanaged(inputpkg.Link).initCapacity(alloc, self.links.items.len);
        // for (self.links.items) |item| { const copy = try item.clone(alloc); list.appendAssumeCapacity(copy); }
        // return .{ .links = list };
        RepeatableLink {
            links: self.links.iter().map(|item| item.clone_in()).collect(),
        }
    }

    /// Compare if two of our value are requal. Required by Config.
    // pub fn equal(self: Self, other: Self) bool
    pub fn equal(&self, other: &RepeatableLink) -> bool {
        // const itemsA = self.links.items;
        // const itemsB = other.links.items;
        // if (itemsA.len != itemsB.len) return false;
        if self.links.len() != other.links.len() {
            return false;
        }
        // for (itemsA, itemsB) |*a, *b| { if (!a.equal(b)) return false; } else return true;
        for (a, b) in self.links.iter().zip(other.links.iter()) {
            if !a.equal(b) {
                return false;
            }
        }
        true
    }

    /// Used by Formatter
    // pub fn formatEntry(self: Self, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry<W: Writer>(&self, _name: &[u8], _writer: &mut W) -> Result<(), W::Error> {
        // This currently can't be set so we don't format anything.
        // _ = self; _ = formatter;
        Ok(())
    }
}

impl FormatEntry for RepeatableLink {
    fn format_entry<W: Writer>(&self, name: &[u8], writer: &mut W) -> Result<(), W::Error> {
        RepeatableLink::format_entry(self, name, writer)
    }
}

/// See "command-palette-entry" for documentation.
// pub const RepeatableCommand = struct { value: ArrayListUnmanaged(inputpkg.Command), value_c: ArrayListUnmanaged(inputpkg.Command.C) };
#[derive(Debug, Clone, Default)]
pub struct RepeatableCommand {
    pub value: Vec<rustty_input::command::Command>,
    pub value_c: Vec<rustty_input::command::C>,
}

/// ghostty_config_command_palette_entry_s -> rustty_config_command_palette_entry_s (R2) (the extern C view).
// pub const C = extern struct { ... };
#[repr(C)]
pub struct RepeatableCommandC {
    pub commands: *const rustty_input::command::C,
    pub len: usize,
}

// PORT NOTE (R4 SEAM, RESOLVED): `RepeatableCommand.parseCLI` calls
// `cli.args.parseAutoStruct(inputpkg.Command, ...)`, i.e. it needs the *external*
// `rustty_input::command::Command` type to implement the `rustty_args::AutoStruct`
// trait. Both types are foreign to this crate, so the orphan rule forbids the impl
// here; it lives in rustty-input (alongside `Command`, in command.rs). With that
// impl landed, `init`/`parseCLI`/`formatEntry` are filled here: they consume
// `inputpkg.command.defaults`/`defaultsC`, `Command.cval`, and `Command.format`
// (all ported). `clone`/`equal`/`cval` were already filled.
impl RepeatableCommand {
    // pub fn cval(self: *const Self) C
    pub fn cval(&self) -> RepeatableCommandC {
        // return .{ .commands = self.value_c.items.ptr, .len = self.value_c.items.len };
        RepeatableCommandC {
            commands: self.value_c.as_ptr(),
            len: self.value_c.len(),
        }
    }

    // pub fn init(self: *Self, alloc: Allocator) !void
    //
    // PORT NOTE: the reference `!void` return is the `Allocator.Error` (OOM) from
    // the two `appendSlice`s; the Rust port aborts on allocation failure (no
    // recoverable OOM), so `init` is infallible here. `errdefer { deinit; deinit }`
    // has no analog (owned `Vec`s free on drop).
    pub fn init(&mut self) {
        // self.value = .empty;
        // self.value_c = .empty;
        self.value = Vec::new();
        self.value_c = Vec::new();
        // try self.value.appendSlice(alloc, inputpkg.command.defaults);
        self.value
            .extend_from_slice(&rustty_input::command::defaults());
        // try self.value_c.appendSlice(alloc, inputpkg.command.defaultsC);
        self.value_c
            .extend_from_slice(&rustty_input::command::defaults_c());
    }

    // pub fn parseCLI(self: *Self, alloc: Allocator, input_: ?[]const u8) !void
    pub fn parse_cli(&mut self, input_: Option<&[u8]>) -> Result<(), ParseError> {
        // Unset or empty input clears the list
        // const input = input_ orelse "";
        let input = input_.unwrap_or(b"");
        // if (input.len == 0) {
        if input.is_empty() {
            // log.info("config has 'command-palette-entry =', using default entries", .{});
            // try self.init(alloc);
            self.init();
            // return;
            return Ok(());
        }

        // if (std.mem.eql(u8, input, "clear")) {
        if input == b"clear" {
            // log.info("config has 'command-palette-entry = clear', all command entries cleared", .{});
            // self.value.clearRetainingCapacity();
            self.value.clear();
            // self.value_c.clearRetainingCapacity();
            self.value_c.clear();
            // return;
            return Ok(());
        }

        // Reserve space in our lists
        // try self.value.ensureUnusedCapacity(alloc, 1);
        self.value.reserve(1);
        // try self.value_c.ensureUnusedCapacity(alloc, 1);
        self.value_c.reserve(1);

        // const cmd = try cli.args.parseAutoStruct(inputpkg.Command, alloc, input, null);
        //
        // PORT NOTE: `parseAutoStruct(Command, ...)` is `parse_auto_struct` over the
        // `AutoStruct for Command` impl (in rustty-input). The result is built into a
        // fresh `Command`; `parse_auto_struct` fills set fields, applies the
        // `description = ""` default, and errors on an unset required field.
        let mut cmd = rustty_input::command::Command {
            action: rustty_input::binding::Action::Ignore,
            title: Vec::new(),
            description: Vec::new(),
        };
        parse_auto_struct(&mut cmd, input)?;
        // const cmd_c = try cmd.cval(alloc);
        //
        // PORT NOTE: `cmd.cval(alloc)` is `!C` (the OOM path); the Rust `cval`
        // returns `Result<C, ()>` carrying that fallible signature. `catch return
        // error.OutOfMemory` -> map the unit error into the parse error set.
        let cmd_c = cmd.cval().map_err(|()| Error::InvalidValue)?;
        // self.value.appendAssumeCapacity(cmd);
        self.value.push(cmd);
        // self.value_c.appendAssumeCapacity(cmd_c);
        self.value_c.push(cmd_c);

        Ok(())
    }

    /// Deep copy of the struct. Required by Config.
    // pub fn clone(self: *const Self, alloc: Allocator) Allocator.Error!Self
    pub fn clone_in(&self) -> RepeatableCommand {
        // const value = try self.value.clone(alloc);
        // for (value.items) |*item| { item.* = try item.clone(alloc); }
        // return .{ .value = value, .value_c = try self.value_c.clone(alloc) };
        RepeatableCommand {
            value: self.value.iter().map(|item| item.clone_in()).collect(),
            value_c: self.value_c.clone(),
        }
    }

    /// Compare if two of our value are equal. Required by Config.
    // pub fn equal(self: RepeatableCommand, other: RepeatableCommand) bool
    pub fn equal(&self, other: &RepeatableCommand) -> bool {
        // if (self.value.items.len != other.value.items.len) return false;
        if self.value.len() != other.value.len() {
            return false;
        }
        // for (self.value.items, other.value.items) |a, b| { if (!a.equal(b)) return false; }
        for (a, b) in self.value.iter().zip(other.value.iter()) {
            if !a.equal(b) {
                return false;
            }
        }
        // return true;
        true
    }

    // pub fn formatEntry(self: Self, formatter: formatterpkg.EntryFormatter) !void
    //
    // PORT NOTE: the reference builds each entry's bytes into a 4096-byte fixed
    // buffer (`std.Io.Writer.fixed`), then hands `writer.buffered()` to
    // `formatter.formatEntry([]const u8, ...)`. The Rust port accumulates each
    // entry into an owned `Vec<u8>` (no fixed cap; the observable bytes are
    // identical) and emits it via `format_entry_string`. `std.zig.fmtString` ->
    // `zig_string_escape`; `{f}` on the action -> `Command.action.format`.
    pub fn format_entry<W: Writer>(&self, name: &[u8], writer: &mut W) -> Result<(), W::Error> {
        // if (self.value.items.len == 0) {
        if self.value.is_empty() {
            // try formatter.formatEntry(void, {});
            // return;
            return format_entry_void(name, writer);
        }

        // for (self.value.items) |item| {
        for item in self.value.iter() {
            // var buf: [4096]u8 = undefined;
            // var writer: std.Io.Writer = .fixed(&buf);
            let mut buf: Vec<u8> = Vec::new();

            // writer.print("title:\"{f}\"", .{std.zig.fmtString(item.title)}) catch return error.OutOfMemory;
            buf.extend_from_slice(b"title:\"");
            rustty_args::args::zig_string_escape(&mut buf, &item.title);
            buf.push(b'"');

            // if (item.description.len > 0) {
            if !item.description.is_empty() {
                // writer.print(",description:\"{f}\"", .{std.zig.fmtString(item.description)}) catch return error.OutOfMemory;
                buf.extend_from_slice(b",description:\"");
                rustty_args::args::zig_string_escape(&mut buf, &item.description);
                buf.push(b'"');
            }

            // writer.print(",action:\"{f}\"", .{item.action}) catch return error.OutOfMemory;
            buf.extend_from_slice(b",action:\"");
            // PORT NOTE: `{f}` on the action -> `Action::format` into a buffer; it is
            // Infallible (never errors), matching the reference's never-failing path.
            {
                let mut action_buf = rustty_cfg_fmt::AllocatingWriter::init();
                let _ = item.action.format(&mut action_buf);
                buf.extend_from_slice(action_buf.written());
            }
            buf.push(b'"');

            // try formatter.formatEntry([]const u8, writer.buffered());
            format_entry_string(name, &buf, writer)?;
        }

        Ok(())
    }
}

impl FormatEntry for RepeatableCommand {
    fn format_entry<W: Writer>(&self, name: &[u8], writer: &mut W) -> Result<(), W::Error> {
        RepeatableCommand::format_entry(self, name, writer)
    }
}

// pub const MouseScrollMultiplier = struct { precision: f64 = 1, discrete: f64 = 3; pub const default = .{} };
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MouseScrollMultiplier {
    pub precision: f64,
    pub discrete: f64,
}

impl Default for MouseScrollMultiplier {
    fn default() -> Self {
        Self {
            precision: 1.0,
            discrete: 3.0,
        }
    }
}

impl MouseScrollMultiplier {
    // pub const default: MouseScrollMultiplier = .{};
    pub fn default_const() -> MouseScrollMultiplier {
        MouseScrollMultiplier::default()
    }

    // pub fn parseCLI(self: *Self, alloc: Allocator, input_: ?[]const u8) !void
    pub fn parse_cli(&mut self, input_: Option<&[u8]>) -> Result<(), ParseError> {
        // const input = input_ orelse return error.ValueRequired;
        let input = input_.ok_or(Error::ValueRequired)?;
        // self.* = cli.args.parseAutoStruct(MouseScrollMultiplier, alloc, input, self.*) catch |err| switch (err) {
        //     error.InvalidValue => bare: {
        //         const v = std.fmt.parseFloat(f64, input) catch return error.InvalidValue;
        //         break :bare .{ .precision = v, .discrete = v };
        //     },
        //     else => return err,
        // };
        //
        // PORT NOTE: `parseAutoStruct` mutates a destination in place; we run it on
        // a copy seeded with the current value (the reference passes `self.*` as the
        // `default_` to inherit unset fields), then commit on success.
        let mut tmp = *self;
        match parse_auto_struct(&mut tmp, input) {
            Ok(()) => *self = tmp,
            Err(Error::InvalidValue) => {
                // bare: the whole value is a single float applied to both fields.
                let v = parse_float_f64(input).map_err(|_| Error::InvalidValue)?;
                *self = MouseScrollMultiplier {
                    precision: v,
                    discrete: v,
                };
            }
            Err(err) => return Err(err),
        }
        Ok(())
    }

    /// Deep copy of the struct. Required by Config.
    // pub fn clone(self: *const Self, alloc: Allocator) Allocator.Error!Self
    pub fn clone_in(&self) -> MouseScrollMultiplier {
        // _ = alloc; return self.*;
        *self
    }

    /// Compare if two of our value are equal. Required by Config.
    // pub fn equal(self: Self, other: Self) bool
    pub fn equal(&self, other: &MouseScrollMultiplier) -> bool {
        // return self.precision == other.precision and self.discrete == other.discrete;
        self.precision == other.precision && self.discrete == other.discrete
    }

    /// Used by Formatter
    // pub fn formatEntry(self: Self, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry<W: Writer>(&self, name: &[u8], writer: &mut W) -> Result<(), W::Error> {
        // var buf: [4096]u8 = undefined;
        // var writer: std.Io.Writer = .fixed(&buf);
        // writer.print("precision:{d},discrete:{d}", .{ self.precision, self.discrete }) catch ...;
        // try formatter.formatEntry([]const u8, writer.buffered());
        let s = format!(
            "precision:{},discrete:{}",
            fmt_float(self.precision),
            fmt_float(self.discrete)
        );
        format_entry_string(name, s.as_bytes(), writer)
    }
}

// PORTS: the auto-struct field reflection of `MouseScrollMultiplier` used by
// `cli.args.parseAutoStruct` — fields `precision`, `discrete` (both `f64`).
impl AutoStruct for MouseScrollMultiplier {
    fn field_names() -> &'static [&'static [u8]] {
        &[b"precision", b"discrete"]
    }

    fn set_field(&mut self, key: &[u8], value: Option<&[u8]>) -> Result<(), ParseError> {
        let v = value.ok_or(Error::ValueRequired)?;
        match key {
            b"precision" => self.precision = parse_float_f64(v).map_err(|_| Error::InvalidValue)?,
            b"discrete" => self.discrete = parse_float_f64(v).map_err(|_| Error::InvalidValue)?,
            _ => return Err(Error::InvalidField),
        }
        Ok(())
    }

    fn apply_default(&mut self, _i: usize) -> Result<(), ParseError> {
        // Both fields keep whatever the seeded destination already holds (the
        // reference inherits unset fields from the provided `default_`).
        Ok(())
    }
}

impl FormatEntry for MouseScrollMultiplier {
    fn format_entry<W: Writer>(&self, name: &[u8], writer: &mut W) -> Result<(), W::Error> {
        MouseScrollMultiplier::format_entry(self, name, writer)
    }
}

/// PORTS: `std.fmt.parseFloat(f64, input)`.
fn parse_float_f64(input: &[u8]) -> Result<f64, ParseError> {
    core::str::from_utf8(input)
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .ok_or(Error::InvalidValue)
}

/// PORTS: the `{d}` float formatter — shortest round-trippable decimal. Rust's
/// default float `Display` produces the same shortest decimal.
fn fmt_float(v: f64) -> String {
    format!("{v}")
}

// pub const QuickTerminalSize = struct { primary: ?Size, secondary: ?Size; Size; Dimensions; C; ... };
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct QuickTerminalSize {
    pub primary: Option<QuickTerminalSizeSize>,
    pub secondary: Option<QuickTerminalSizeSize>,
}

// pub const Size = union(enum) { percentage: f32, pixels: u32 };
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum QuickTerminalSizeSize {
    Percentage(f32),
    Pixels(u32),
}

/// PORTS: the error set of `QuickTerminalSize.parseCLI`/`Size.parse` —
/// `error{ ValueRequired, InvalidValue, MissingUnit, TooManyArguments }`. The
/// distinguishable `MissingUnit`/`TooManyArguments` errors are asserted by
/// `test "parse QuickTerminalSize"`, so they are preserved (the base args
/// `Error` set has neither).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuickTerminalSizeParseError {
    ValueRequired,
    InvalidValue,
    MissingUnit,
    TooManyArguments,
}

impl QuickTerminalSizeSize {
    // pub fn toPixels(self: Size, parent_dimensions: u32) u32
    pub fn to_pixels(self: QuickTerminalSizeSize, parent_dimensions: u32) -> u32 {
        // switch (self) {
        match self {
            // .percentage => |v| {
            //     const dim: f32 = @floatFromInt(parent_dimensions);
            //     return @intFromFloat(v / 100.0 * dim);
            // },
            QuickTerminalSizeSize::Percentage(v) => {
                let dim: f32 = parent_dimensions as f32;
                (v / 100.0 * dim) as u32
            }
            // .pixels => |v| return v,
            QuickTerminalSizeSize::Pixels(v) => v,
        }
    }

    // pub fn parse(input: []const u8) !Size
    pub fn parse(input: &[u8]) -> Result<QuickTerminalSizeSize, QuickTerminalSizeParseError> {
        // if (input.len == 0) return error.ValueRequired;
        if input.is_empty() {
            return Err(QuickTerminalSizeParseError::ValueRequired);
        }

        // if (std.mem.endsWith(u8, input, "px")) {
        if input.ends_with(b"px") {
            // return .{ .pixels = std.fmt.parseInt(u32, input[0 .. input.len - "px".len], 10)
            //     catch return error.InvalidValue };
            let v = parse_int_u32_base10(&input[0..input.len() - 2])
                .map_err(|_| QuickTerminalSizeParseError::InvalidValue)?;
            return Ok(QuickTerminalSizeSize::Pixels(v));
        }

        // if (std.mem.endsWith(u8, input, "%")) {
        if input.ends_with(b"%") {
            // const percentage = std.fmt.parseFloat(f32, input[0 .. input.len - "%".len])
            //     catch return error.InvalidValue;
            let percentage = parse_float_f32(&input[0..input.len() - 1])
                .map_err(|_| QuickTerminalSizeParseError::InvalidValue)?;

            // if (percentage < 0) return error.InvalidValue;
            if percentage < 0.0 {
                return Err(QuickTerminalSizeParseError::InvalidValue);
            }
            // return .{ .percentage = percentage };
            return Ok(QuickTerminalSizeSize::Percentage(percentage));
        }

        // return error.MissingUnit;
        Err(QuickTerminalSizeParseError::MissingUnit)
    }

    // fn format(self: Size, writer: *std.Io.Writer) !void
    fn format(self: QuickTerminalSizeSize, buf: &mut Vec<u8>) {
        // switch (self) {
        match self {
            // .percentage => |v| try writer.print("{d}%", .{v}),
            QuickTerminalSizeSize::Percentage(v) => {
                buf.extend_from_slice(format!("{v}").as_bytes());
                buf.push(b'%');
            }
            // .pixels => |v| try writer.print("{}px", .{v}),
            QuickTerminalSizeSize::Pixels(v) => {
                buf.extend_from_slice(format!("{v}").as_bytes());
                buf.extend_from_slice(b"px");
            }
        }
    }
}

/// PORTS: `std.fmt.parseInt(u32, input, 10)`.
fn parse_int_u32_base10(input: &[u8]) -> Result<u32, ParseIntError> {
    if input.is_empty() {
        return Err(ParseIntError::InvalidCharacter);
    }
    let mut acc: u32 = 0;
    for &c in input {
        let digit = match c {
            b'0'..=b'9' => (c - b'0') as u32,
            _ => return Err(ParseIntError::InvalidCharacter),
        };
        acc = acc
            .checked_mul(10)
            .and_then(|v| v.checked_add(digit))
            .ok_or(ParseIntError::Overflow)?;
    }
    Ok(acc)
}

/// PORTS: `std.fmt.parseFloat(f32, input)`.
fn parse_float_f32(input: &[u8]) -> Result<f32, ParseError> {
    core::str::from_utf8(input)
        .ok()
        .and_then(|s| s.parse::<f32>().ok())
        .ok_or(Error::InvalidValue)
}

// pub const Dimensions = struct { width: u32, height: u32 };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuickTerminalSizeDimensions {
    pub width: u32,
    pub height: u32,
}

/// C API structure for QuickTerminalSize
// pub const C = extern struct { primary: C.Size, secondary: C.Size };
#[repr(C)]
pub struct QuickTerminalSizeC {
    pub primary: QuickTerminalSizeCSize,
    pub secondary: QuickTerminalSizeCSize,
}

// pub const Size = extern struct { tag: Tag, value: Value };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct QuickTerminalSizeCSize {
    pub tag: QuickTerminalSizeCTag,
    pub value: QuickTerminalSizeCValue,
}

/// c_int because it needs to be extern compatible
// pub const Tag = enum(c_int) { none, percentage, pixels };
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum QuickTerminalSizeCTag {
    None,
    Percentage,
    Pixels,
}

// pub const Value = extern union { percentage: f32, pixels: u32 };
#[repr(C)]
#[derive(Clone, Copy)]
pub union QuickTerminalSizeCValue {
    pub percentage: f32,
    pub pixels: u32,
}

impl QuickTerminalSizeCSize {
    // pub const none: C.Size = .{ .tag = .none, .value = undefined };
    pub fn none() -> QuickTerminalSizeCSize {
        QuickTerminalSizeCSize {
            tag: QuickTerminalSizeCTag::None,
            value: QuickTerminalSizeCValue { pixels: 0 },
        }
    }

    // pub fn percentage(v: f32) C.Size
    pub fn percentage(v: f32) -> QuickTerminalSizeCSize {
        // return .{ .tag = .percentage, .value = .{ .percentage = v } };
        QuickTerminalSizeCSize {
            tag: QuickTerminalSizeCTag::Percentage,
            value: QuickTerminalSizeCValue { percentage: v },
        }
    }

    // pub fn pixels(v: u32) C.Size
    pub fn pixels(v: u32) -> QuickTerminalSizeCSize {
        // return .{ .tag = .pixels, .value = .{ .pixels = v } };
        QuickTerminalSizeCSize {
            tag: QuickTerminalSizeCTag::Pixels,
            value: QuickTerminalSizeCValue { pixels: v },
        }
    }
}

impl QuickTerminalSize {
    // pub fn cval(self: QuickTerminalSize) C
    pub fn cval(self: QuickTerminalSize) -> QuickTerminalSizeC {
        // return .{
        //     .primary = if (self.primary) |p| switch (p) { .percentage => |v| .percentage(v), .pixels => |v| .pixels(v) } else .none,
        //     .secondary = if (self.secondary) |s| switch (s) { .percentage => |v| .percentage(v), .pixels => |v| .pixels(v) } else .none,
        // };
        QuickTerminalSizeC {
            primary: match self.primary {
                Some(QuickTerminalSizeSize::Percentage(v)) => QuickTerminalSizeCSize::percentage(v),
                Some(QuickTerminalSizeSize::Pixels(v)) => QuickTerminalSizeCSize::pixels(v),
                None => QuickTerminalSizeCSize::none(),
            },
            secondary: match self.secondary {
                Some(QuickTerminalSizeSize::Percentage(v)) => QuickTerminalSizeCSize::percentage(v),
                Some(QuickTerminalSizeSize::Pixels(v)) => QuickTerminalSizeCSize::pixels(v),
                None => QuickTerminalSizeCSize::none(),
            },
        }
    }

    // pub fn calculate(self: QuickTerminalSize, position: QuickTerminalPosition, dims: Dimensions) Dimensions
    pub fn calculate(
        self: QuickTerminalSize,
        position: QuickTerminalPosition,
        dims: QuickTerminalSizeDimensions,
    ) -> QuickTerminalSizeDimensions {
        // switch (position) {
        match position {
            // .left, .right => return .{
            //     .width = if (self.primary) |v| v.toPixels(dims.width) else 400,
            //     .height = if (self.secondary) |v| v.toPixels(dims.height) else dims.height,
            // },
            QuickTerminalPosition::Left | QuickTerminalPosition::Right => {
                QuickTerminalSizeDimensions {
                    width: match self.primary {
                        Some(v) => v.to_pixels(dims.width),
                        None => 400,
                    },
                    height: match self.secondary {
                        Some(v) => v.to_pixels(dims.height),
                        None => dims.height,
                    },
                }
            }
            // .top, .bottom => return .{
            //     .width = if (self.secondary) |v| v.toPixels(dims.width) else dims.width,
            //     .height = if (self.primary) |v| v.toPixels(dims.height) else 400,
            // },
            QuickTerminalPosition::Top | QuickTerminalPosition::Bottom => {
                QuickTerminalSizeDimensions {
                    width: match self.secondary {
                        Some(v) => v.to_pixels(dims.width),
                        None => dims.width,
                    },
                    height: match self.primary {
                        Some(v) => v.to_pixels(dims.height),
                        None => 400,
                    },
                }
            }
            // .center => if (dims.width >= dims.height) { ... } else { ... },
            QuickTerminalPosition::Center => {
                if dims.width >= dims.height {
                    QuickTerminalSizeDimensions {
                        width: match self.primary {
                            Some(v) => v.to_pixels(dims.width),
                            None => 800,
                        },
                        height: match self.secondary {
                            Some(v) => v.to_pixels(dims.height),
                            None => 400,
                        },
                    }
                } else {
                    QuickTerminalSizeDimensions {
                        width: match self.secondary {
                            Some(v) => v.to_pixels(dims.width),
                            None => 400,
                        },
                        height: match self.primary {
                            Some(v) => v.to_pixels(dims.height),
                            None => 800,
                        },
                    }
                }
            }
        }
    }

    // pub fn parseCLI(self: *QuickTerminalSize, input: ?[]const u8) !void
    pub fn parse_cli(&mut self, input: Option<&[u8]>) -> Result<(), QuickTerminalSizeParseError> {
        // const input_ = input orelse return error.ValueRequired;
        let input_ = input.ok_or(QuickTerminalSizeParseError::ValueRequired)?;
        // var it = std.mem.splitScalar(u8, input_, ',');
        let mut it = split_scalar(input_, b',');

        // const primary = std.mem.trim(u8, it.next() orelse return error.ValueRequired, cli.args.whitespace);
        let primary = trim_ws_args(
            it.next()
                .ok_or(QuickTerminalSizeParseError::ValueRequired)?,
        );
        // self.primary = try .parse(primary);
        self.primary = Some(QuickTerminalSizeSize::parse(primary)?);

        // self.secondary = secondary: {
        //     const secondary = std.mem.trim(u8, it.next() orelse break :secondary null, cli.args.whitespace);
        //     break :secondary try .parse(secondary);
        // };
        self.secondary = match it.next() {
            Some(raw) => Some(QuickTerminalSizeSize::parse(trim_ws_args(raw))?),
            None => None,
        };

        // if (it.next()) |_| return error.TooManyArguments;
        if it.next().is_some() {
            return Err(QuickTerminalSizeParseError::TooManyArguments);
        }
        Ok(())
    }

    // pub fn clone(self: *const QuickTerminalSize, _: Allocator) Allocator.Error!QuickTerminalSize
    pub fn clone_in(&self) -> QuickTerminalSize {
        // return .{ .primary = self.primary, .secondary = self.secondary };
        *self
    }

    // pub fn formatEntry(self: QuickTerminalSize, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry<W: Writer>(&self, name: &[u8], writer: &mut W) -> Result<(), W::Error> {
        // const primary = self.primary orelse return;
        let primary = match self.primary {
            Some(p) => p,
            None => return Ok(()),
        };

        // var buf: [4096]u8 = undefined;
        // var writer: std.Io.Writer = .fixed(&buf);
        let mut buf: Vec<u8> = Vec::new();

        // primary.format(&writer) catch return error.OutOfMemory;
        primary.format(&mut buf);
        // if (self.secondary) |secondary| {
        //     writer.writeByte(',') catch return error.OutOfMemory;
        //     secondary.format(&writer) catch return error.OutOfMemory;
        // }
        if let Some(secondary) = self.secondary {
            buf.push(b',');
            secondary.format(&mut buf);
        }

        // try formatter.formatEntry([]const u8, writer.buffered());
        format_entry_string(name, &buf, writer)
    }
}

impl FormatEntry for QuickTerminalSize {
    fn format_entry<W: Writer>(&self, name: &[u8], writer: &mut W) -> Result<(), W::Error> {
        QuickTerminalSize::format_entry(self, name, writer)
    }
}

/// PORTS: `std.mem.splitScalar(u8, input, delim)` — yields all delimiter-
/// separated slices including empty ones (unlike `tokenizeScalar`).
fn split_scalar(input: &[u8], delim: u8) -> impl Iterator<Item = &[u8]> {
    input.split(move |&c| c == delim)
}

// pub const Theme = struct { light: []const u8, dark: []const u8 };
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Theme {
    pub light: Vec<u8>,
    pub dark: Vec<u8>,
}

impl Theme {
    // pub fn parseCLI(self: *Theme, alloc: Allocator, input_: ?[]const u8) !void
    pub fn parse_cli(&mut self, input_: Option<&[u8]>) -> Result<(), ParseError> {
        // const input = input_ orelse return error.ValueRequired;
        let input = input_.ok_or(Error::ValueRequired)?;
        // if (input.len == 0) return error.ValueRequired;
        if input.is_empty() {
            return Err(Error::ValueRequired);
        }

        // If there is a comma, equal sign, or colon, then we assume that
        // we're parsing a light/dark mode theme pair. Note that "=" isn't
        // actually valid for setting a light/dark mode pair but I anticipate
        // it'll be a common typo.
        //
        // On Windows, a colon at index 1 is a drive letter (e.g. C:\...)
        // and should not trigger light/dark pair parsing.
        // const has_colon = if (comptime builtin.os.tag == .windows)
        //     if (std.mem.indexOf(u8, input, ":")) |idx| idx != 1 else false
        // else
        //     std.mem.indexOf(u8, input, ":") != null;
        let has_colon = if cfg!(windows) {
            match index_of(input, b':') {
                Some(idx) => idx != 1,
                None => false,
            }
        } else {
            index_of(input, b':').is_some()
        };
        // if (std.mem.indexOf(u8, input, ",") != null or std.mem.indexOf(u8, input, "=") != null or has_colon) {
        if index_of(input, b',').is_some() || index_of(input, b'=').is_some() || has_colon {
            // self.* = try cli.args.parseAutoStruct(Theme, alloc, input, null);
            let mut tmp = Theme::default();
            parse_auto_struct(&mut tmp, input)?;
            *self = tmp;
            // return;
            return Ok(());
        }

        // Trim our value
        // const trimmed = std.mem.trim(u8, input, cli.args.whitespace);
        let trimmed = trim_ws_args(input);

        // Set the value to the specified value directly.
        // self.* = .{ .light = try alloc.dupeZ(u8, trimmed), .dark = self.light };
        let light = trimmed.to_vec();
        *self = Theme {
            dark: light.clone(),
            light,
        };
        Ok(())
    }

    /// Deep copy of the struct. Required by Config.
    // pub fn clone(self: *const Theme, alloc: Allocator) Allocator.Error!Theme
    pub fn clone_in(&self) -> Theme {
        // return .{ .light = try alloc.dupeZ(u8, self.light), .dark = try alloc.dupeZ(u8, self.dark) };
        Theme {
            light: self.light.clone(),
            dark: self.dark.clone(),
        }
    }

    /// Used by Formatter
    // pub fn formatEntry(self: Theme, formatter: anytype) !void
    pub fn format_entry<W: Writer>(&self, name: &[u8], writer: &mut W) -> Result<(), W::Error> {
        // var buf: [4096]u8 = undefined;
        // if (std.mem.eql(u8, self.light, self.dark)) { try formatter.formatEntry([]const u8, self.light); return; }
        if self.light == self.dark {
            return format_entry_string(name, &self.light, writer);
        }

        // const str = std.fmt.bufPrint(&buf, "light:{s},dark:{s}", .{ self.light, self.dark }) catch ...;
        // try formatter.formatEntry([]const u8, str);
        let mut s: Vec<u8> = Vec::new();
        s.extend_from_slice(b"light:");
        s.extend_from_slice(&self.light);
        s.extend_from_slice(b",dark:");
        s.extend_from_slice(&self.dark);
        format_entry_string(name, &s, writer)
    }
}

// PORTS: the auto-struct field reflection of `Theme` — fields `light`, `dark`
// (both `[]const u8`, required: no struct default).
impl AutoStruct for Theme {
    fn field_names() -> &'static [&'static [u8]] {
        &[b"light", b"dark"]
    }

    fn set_field(&mut self, key: &[u8], value: Option<&[u8]>) -> Result<(), ParseError> {
        let v = value.ok_or(Error::ValueRequired)?;
        match key {
            b"light" => self.light = v.to_vec(),
            b"dark" => self.dark = v.to_vec(),
            _ => return Err(Error::InvalidField),
        }
        Ok(())
    }

    fn apply_default(&mut self, _i: usize) -> Result<(), ParseError> {
        // light/dark are required (no struct default, `default_ == null`); an
        // unset field is `error.InvalidValue` (e.g. `light:foo` with no `dark`).
        Err(Error::InvalidValue)
    }
}

impl FormatEntry for Theme {
    fn format_entry<W: Writer>(&self, name: &[u8], writer: &mut W) -> Result<(), W::Error> {
        Theme::format_entry(self, name, writer)
    }
}

/// PORTS: `std.mem.trim(u8, s, cli.args.whitespace)` where `cli.args.whitespace`
/// is `" \t"` (the args crate's `WHITESPACE`).
fn trim_ws_args(s: &[u8]) -> &[u8] {
    let mut start = 0;
    let mut end = s.len();
    while start < end && WHITESPACE.contains(&s[start]) {
        start += 1;
    }
    while end > start && WHITESPACE.contains(&s[end - 1]) {
        end -= 1;
    }
    &s[start..end]
}

// pub const Duration = struct { duration: u64 = 0; const units = [...]; ... };
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Duration {
    /// Duration in nanoseconds
    pub duration: u64,
}

// const units = [_]struct { name: []const u8, factor: u64 }{ ... };
//
// The order is important as the first factor that matches will be the
// default unit that is used for formatting.
//
// PORT NOTE: `std.time.ns_per_*` constants are inlined as their literal
// nanosecond factors (ns_per_s = 1e9, ns_per_min = 60e9, etc.).
const DURATION_UNITS: &[(&[u8], u64)] = &[
    // .{ .name = "y", .factor = 365 * std.time.ns_per_day },
    (b"y", 365 * NS_PER_DAY),
    // .{ .name = "w", .factor = std.time.ns_per_week },
    (b"w", NS_PER_WEEK),
    // .{ .name = "d", .factor = std.time.ns_per_day },
    (b"d", NS_PER_DAY),
    // .{ .name = "h", .factor = std.time.ns_per_hour },
    (b"h", NS_PER_HOUR),
    // .{ .name = "m", .factor = std.time.ns_per_min },
    (b"m", NS_PER_MIN),
    // .{ .name = "s", .factor = std.time.ns_per_s },
    (b"s", NS_PER_S),
    // .{ .name = "ms", .factor = std.time.ns_per_ms },
    (b"ms", NS_PER_MS),
    // .{ .name = "µs", .factor = std.time.ns_per_us },
    ("µs".as_bytes(), NS_PER_US),
    // .{ .name = "us", .factor = std.time.ns_per_us },
    (b"us", NS_PER_US),
    // .{ .name = "ns", .factor = 1 },
    (b"ns", 1),
];

const NS_PER_US: u64 = 1000;
const NS_PER_MS: u64 = 1000 * NS_PER_US;
const NS_PER_S: u64 = 1000 * NS_PER_MS;
const NS_PER_MIN: u64 = 60 * NS_PER_S;
const NS_PER_HOUR: u64 = 60 * NS_PER_MIN;
const NS_PER_DAY: u64 = 24 * NS_PER_HOUR;
const NS_PER_WEEK: u64 = 7 * NS_PER_DAY;

impl Duration {
    // pub fn clone(self: *const Duration, _: Allocator) error{}!Duration
    pub fn clone_in(&self) -> Duration {
        // return .{ .duration = self.duration };
        *self
    }

    // pub fn equal(self: Duration, other: Duration) bool
    pub fn equal(self: Duration, other: Duration) -> bool {
        // return self.duration == other.duration;
        self == other
    }

    // pub fn round(self: Duration, to: u64) Duration
    pub fn round(self: Duration, to: u64) -> Duration {
        // return .{ .duration = self.duration / to * to };
        Duration {
            duration: self.duration / to * to,
        }
    }

    // pub fn lte(self: Duration, other: Duration) bool
    pub fn lte(self: Duration, other: Duration) -> bool {
        // return self.duration <= other.duration;
        self.duration <= other.duration
    }

    // pub fn parseCLI(input: ?[]const u8) !Duration
    pub fn parse_cli(input: Option<&[u8]>) -> Result<Duration, ParseError> {
        // var remaining = input orelse return error.ValueRequired;
        let mut remaining = input.ok_or(Error::ValueRequired)?;

        // var value: ?u64 = null;
        let mut value: Option<u64> = None;
        // while (remaining.len > 0) {
        while !remaining.is_empty() {
            // Skip over whitespace before the number
            // while (remaining.len > 0 and std.ascii.isWhitespace(remaining[0])) { remaining = remaining[1..]; }
            while !remaining.is_empty() && is_ascii_whitespace(remaining[0]) {
                remaining = &remaining[1..];
            }

            // There was whitespace at the end, that's OK
            // if (remaining.len == 0) break;
            if remaining.is_empty() {
                break;
            }

            // Find the longest number
            // const number: u64 = number: {
            //     var prev_number: ?u64 = null;
            //     var prev_remaining: ?[]const u8 = null;
            //     for (1..remaining.len + 1) |index| {
            //         prev_number = std.fmt.parseUnsigned(u64, remaining[0..index], 10) catch {
            //             if (prev_remaining) |prev| remaining = prev;
            //             break :number prev_number;
            //         };
            //         prev_remaining = remaining[index..];
            //     }
            //     if (prev_remaining) |prev| remaining = prev;
            //     break :number prev_number;
            // } orelse return error.InvalidValue;
            let number: u64 = {
                let mut prev_number: Option<u64> = None;
                let mut prev_remaining: Option<&[u8]> = None;
                let mut broke = false;
                for index in 1..remaining.len() + 1 {
                    match parse_unsigned_u64_base10(&remaining[0..index]) {
                        Ok(n) => {
                            prev_number = Some(n);
                            prev_remaining = Some(&remaining[index..]);
                        }
                        Err(_) => {
                            if let Some(prev) = prev_remaining {
                                remaining = prev;
                            }
                            broke = true;
                            break;
                        }
                    }
                }
                if !broke {
                    if let Some(prev) = prev_remaining {
                        remaining = prev;
                    }
                }
                match prev_number {
                    Some(n) => n,
                    None => return Err(Error::InvalidValue),
                }
            };

            // A number without a unit is invalid unless the number is
            // exactly zero. In that case, the unit is unambiguous since
            // its all the same.
            // if (remaining.len == 0) {
            //     if (number == 0) { value = 0; break; }
            //     return error.InvalidValue;
            // }
            if remaining.is_empty() {
                if number == 0 {
                    value = Some(0);
                    break;
                }
                return Err(Error::InvalidValue);
            }

            // Find the longest matching unit. Needs to be the longest matching
            // to distinguish 'm' from 'ms'.
            // const factor = factor: {
            //     var prev_factor: ?u64 = null;
            //     var prev_index: ?usize = null;
            //     for (1..remaining.len + 1) |index| {
            //         const next_factor = next: {
            //             for (units) |unit| { if (std.mem.eql(u8, unit.name, remaining[0..index])) break :next unit.factor; }
            //             break :next null;
            //         };
            //         if (next_factor) |next| { prev_factor = next; prev_index = index; }
            //     }
            //     if (prev_index) |index| { remaining = remaining[index..]; }
            //     break :factor prev_factor;
            // } orelse return error.InvalidValue;
            let factor: u64 = {
                let mut prev_factor: Option<u64> = None;
                let mut prev_index: Option<usize> = None;
                for index in 1..remaining.len() + 1 {
                    let next_factor = {
                        let mut found = None;
                        for &(uname, ufactor) in DURATION_UNITS {
                            if uname == &remaining[0..index] {
                                found = Some(ufactor);
                                break;
                            }
                        }
                        found
                    };
                    if let Some(next) = next_factor {
                        prev_factor = Some(next);
                        prev_index = Some(index);
                    }
                }
                if let Some(index) = prev_index {
                    remaining = &remaining[index..];
                }
                match prev_factor {
                    Some(f) => f,
                    None => return Err(Error::InvalidValue),
                }
            };

            // Add our time value to the total. Avoid overflow with saturating math.
            // const diff = std.math.mul(u64, number, factor) catch std.math.maxInt(u64);
            //
            // PORT NOTE: `std.math.mul(...) catch maxInt` is saturating-on-overflow;
            // `saturating_mul` is the exact equivalent.
            let diff = number.saturating_mul(factor);
            // value = (value orelse 0) +| diff;
            value = Some(value.unwrap_or(0).saturating_add(diff));
        }

        // return if (value) |v| .{ .duration = v } else error.ValueRequired;
        match value {
            Some(v) => Ok(Duration { duration: v }),
            None => Err(Error::ValueRequired),
        }
    }

    // pub fn formatEntry(self: Duration, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry<W: Writer>(&self, name: &[u8], writer: &mut W) -> Result<(), W::Error> {
        // var buf: [64]u8 = undefined;
        // var writer: std.Io.Writer = .fixed(&buf);
        // try self.format(&writer);
        // try formatter.formatEntry([]const u8, writer.buffered());
        let mut buf: Vec<u8> = Vec::new();
        self.format(&mut buf);
        format_entry_string(name, &buf, writer)
    }

    // pub fn format(self: Duration, writer: *std.Io.Writer) !void
    pub fn format(self: &Duration, buf: &mut Vec<u8>) {
        // var value = self.duration;
        let mut value = self.duration;
        // var i: usize = 0;
        let mut i: usize = 0;
        // for (units) |unit| {
        for &(uname, ufactor) in DURATION_UNITS {
            // if (value >= unit.factor) {
            if value >= ufactor {
                // if (i > 0) writer.writeAll(" ") catch unreachable;
                if i > 0 {
                    buf.push(b' ');
                }
                // const remainder = value % unit.factor;
                let remainder = value % ufactor;
                // const quotient = (value - remainder) / unit.factor;
                let quotient = (value - remainder) / ufactor;
                // writer.print("{d}{s}", .{ quotient, unit.name }) catch unreachable;
                buf.extend_from_slice(format!("{quotient}").as_bytes());
                buf.extend_from_slice(uname);
                // value = remainder;
                value = remainder;
                // i += 1;
                i += 1;
            }
        }
    }

    // pub fn cval(self: Duration) usize
    pub fn cval(self: Duration) -> usize {
        // return @intCast(self.asMilliseconds());
        self.as_milliseconds() as usize
    }

    /// Convenience function to convert to milliseconds since many OS and
    /// library timing functions operate on that timescale.
    // pub fn asMilliseconds(self: Duration) c_uint
    pub fn as_milliseconds(self: Duration) -> std::os::raw::c_uint {
        // const ms: u64 = std.math.divTrunc(u64, self.duration, std.time.ns_per_ms) catch std.math.maxInt(c_uint);
        // PORT NOTE: `divTrunc` only errors on divide-by-zero / signed overflow,
        // neither possible here (constant non-zero divisor, unsigned); the
        // `catch` is unreachable, matching the reference's effective behavior.
        let ms: u64 = self.duration / NS_PER_MS;
        // return std.math.cast(c_uint, ms) orelse std.math.maxInt(c_uint);
        std::os::raw::c_uint::try_from(ms).unwrap_or(std::os::raw::c_uint::MAX)
    }
}

impl FormatEntry for Duration {
    fn format_entry<W: Writer>(&self, name: &[u8], writer: &mut W) -> Result<(), W::Error> {
        Duration::format_entry(self, name, writer)
    }
}

/// PORTS: `std.ascii.isWhitespace(c)` — `{ ' ', '\t', '\n', '\r', VT, FF }`.
fn is_ascii_whitespace(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

/// PORTS: `std.fmt.parseUnsigned(u64, buf, 10)`.
fn parse_unsigned_u64_base10(buf: &[u8]) -> Result<u64, ParseIntError> {
    if buf.is_empty() {
        return Err(ParseIntError::InvalidCharacter);
    }
    let mut acc: u64 = 0;
    for &c in buf {
        let digit = match c {
            b'0'..=b'9' => (c - b'0') as u64,
            _ => return Err(ParseIntError::InvalidCharacter),
        };
        acc = acc
            .checked_mul(10)
            .and_then(|v| v.checked_add(digit))
            .ok_or(ParseIntError::Overflow)?;
    }
    Ok(acc)
}

// pub const WindowPadding = struct { top_left: u32 = 0, bottom_right: u32 = 0 };
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WindowPadding {
    pub top_left: u32,
    pub bottom_right: u32,
}

impl WindowPadding {
    // pub fn clone(self: Self, _: Allocator) error{}!Self
    pub fn clone_in(&self) -> WindowPadding {
        // return self;
        *self
    }

    // pub fn equal(self: Self, other: Self) bool
    pub fn equal(self: WindowPadding, other: WindowPadding) -> bool {
        // return std.meta.eql(self, other);
        self == other
    }

    // pub fn parseCLI(input_: ?[]const u8) !WindowPadding
    pub fn parse_cli(input_: Option<&[u8]>) -> Result<WindowPadding, ParseError> {
        // const input = input_ orelse return error.ValueRequired;
        let input = input_.ok_or(Error::ValueRequired)?;
        // const whitespace = " \t";

        // if (std.mem.indexOf(u8, input, ",")) |idx| {
        if let Some(idx) = index_of(input, b',') {
            // const input_left = std.mem.trim(u8, input[0..idx], whitespace);
            let input_left = trim_ws(&input[0..idx]);
            // const input_right = std.mem.trim(u8, input[idx + 1 ..], whitespace);
            let input_right = trim_ws(&input[idx + 1..]);
            // const left = std.fmt.parseInt(u32, input_left, 10) catch return error.InvalidValue;
            let left = parse_int_u32_base10(input_left).map_err(|_| Error::InvalidValue)?;
            // const right = std.fmt.parseInt(u32, input_right, 10) catch return error.InvalidValue;
            let right = parse_int_u32_base10(input_right).map_err(|_| Error::InvalidValue)?;
            // return .{ .top_left = left, .bottom_right = right };
            Ok(WindowPadding {
                top_left: left,
                bottom_right: right,
            })
        } else {
            // const value = std.fmt.parseInt(u32, std.mem.trim(u8, input, whitespace), 10) catch return error.InvalidValue;
            let value = parse_int_u32_base10(trim_ws(input)).map_err(|_| Error::InvalidValue)?;
            // return .{ .top_left = value, .bottom_right = value };
            Ok(WindowPadding {
                top_left: value,
                bottom_right: value,
            })
        }
    }

    // pub fn formatEntry(self: Self, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry<W: Writer>(&self, name: &[u8], writer: &mut W) -> Result<(), W::Error> {
        // var buf: [128]u8 = undefined;
        // if (self.top_left == self.bottom_right) {
        //     try formatter.formatEntry([]const u8, std.fmt.bufPrint(&buf, "{}", .{self.top_left}) catch ...);
        // } else {
        //     try formatter.formatEntry([]const u8, std.fmt.bufPrint(&buf, "{},{}", .{ self.top_left, self.bottom_right }) catch ...);
        // }
        if self.top_left == self.bottom_right {
            let s = format!("{}", self.top_left);
            format_entry_string(name, s.as_bytes(), writer)
        } else {
            let s = format!("{},{}", self.top_left, self.bottom_right);
            format_entry_string(name, s.as_bytes(), writer)
        }
    }
}

impl FormatEntry for WindowPadding {
    fn format_entry<W: Writer>(&self, name: &[u8], writer: &mut W) -> Result<(), W::Error> {
        WindowPadding::format_entry(self, name, writer)
    }
}

// ===========================================================================
// Value-type tests (R1: every in-type `test {}` block ported, assertion-for-
// assertion, with INDEPENDENT expected values). The reference defines these
// inside each value type's struct; here they are collected in one module and
// named with the reference test's slug, prefixed by the type, to disambiguate.
//
// DEFERRED (not ported in this sub-batch): the `Keybinds` value-type tests, whose
// impl is a deferred DI/infra seam (see the PORT NOTE on that type).
// `RepeatableCommand`'s tests ARE ported below (the `Command: AutoStruct` seam is
// resolved); `RepeatableCodepointMap`'s tests ARE ported below (its
// `fontpkg.CodepointMap` seam is now ported PURE in rustty-font).
// ===========================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use rustty_cfg_fmt::{entry_formatter, AllocatingWriter};

    // -----------------------------------------------------------------------
    // Color
    // -----------------------------------------------------------------------

    // test "parseCLI hex" (was "fromHex" — oracle 2970e9a2a deleted fromHex;
    // parseCLI now covers the same hex forms via RGB::parse delegation)
    #[test]
    fn color_parse_cli_hex() {
        // try testing.expectEqual(Color{ .r = 0, .g = 0, .b = 0 }, try Color.parseCLI("#000000"));
        assert_eq!(
            Color { r: 0, g: 0, b: 0 },
            Color::parse_cli(Some(b"#000000")).unwrap()
        );
        // try testing.expectEqual(Color{ .r = 10, .g = 11, .b = 12 }, try Color.parseCLI("#0A0B0C"));
        assert_eq!(
            Color {
                r: 10,
                g: 11,
                b: 12
            },
            Color::parse_cli(Some(b"#0A0B0C")).unwrap()
        );
        // try testing.expectEqual(Color{ .r = 10, .g = 11, .b = 12 }, try Color.parseCLI("0A0B0C"));
        assert_eq!(
            Color {
                r: 10,
                g: 11,
                b: 12
            },
            Color::parse_cli(Some(b"0A0B0C")).unwrap()
        );
        // try testing.expectEqual(Color{ .r = 255, .g = 255, .b = 255 }, try Color.parseCLI("FFFFFF"));
        assert_eq!(
            Color {
                r: 255,
                g: 255,
                b: 255
            },
            Color::parse_cli(Some(b"FFFFFF")).unwrap()
        );
        // try testing.expectEqual(Color{ .r = 255, .g = 255, .b = 255 }, try Color.parseCLI("FFF"));
        assert_eq!(
            Color {
                r: 255,
                g: 255,
                b: 255
            },
            Color::parse_cli(Some(b"FFF")).unwrap()
        );
        // try testing.expectEqual(Color{ .r = 51, .g = 68, .b = 85 }, try Color.parseCLI("#345"));
        assert_eq!(
            Color {
                r: 51,
                g: 68,
                b: 85
            },
            Color::parse_cli(Some(b"#345")).unwrap()
        );
    }

    // color/palette parsing now delegates to the shared rustty-vt helpers
    // (oracle 2970e9a2a Config.zig hunk); "rgb:R/G/B" is not accepted by the
    // old config-local fromHex/x11-map-only parseCLI, but IS accepted by
    // rustty_vt::color::RGB::parse — this asserts the delegation landed.
    #[test]
    fn color_config_accepts_bare_hex_and_prefixed_palette_index() {
        assert_eq!(
            Color {
                r: 10,
                g: 11,
                b: 12
            },
            Color::parse_cli(Some(b"0A0B0C")).unwrap()
        );
        assert_eq!(
            Color {
                r: 17,
                g: 34,
                b: 51
            },
            Color::parse_cli(Some(b"rgb:1/2/3")).unwrap()
        );

        let mut p = Palette::default();
        p.parse_cli(Some(b"0x10=#282c34")).unwrap();
        assert_eq!(p.value[16].r, 0x28);
        assert_eq!(p.value[16].g, 0x2c);
        assert_eq!(p.value[16].b, 0x34);
        assert!(p.mask.is_set(16));
    }

    // test "parseCLI from name"
    #[test]
    fn color_parse_cli_from_name() {
        // try std.testing.expectEqual(Color{ .r = 0, .g = 0, .b = 0 }, try Color.parseCLI("black"));
        assert_eq!(
            Color { r: 0, g: 0, b: 0 },
            Color::parse_cli(Some(b"black")).unwrap()
        );
    }

    // test "formatConfig"
    #[test]
    fn color_format_config() {
        // var buf: std.Io.Writer.Allocating = .init(testing.allocator);
        let mut buf = AllocatingWriter::init();
        // var color: Color = .{ .r = 10, .g = 11, .b = 12 };
        let color = Color {
            r: 10,
            g: 11,
            b: 12,
        };
        // try color.formatEntry(formatterpkg.entryFormatter("a", &buf.writer));
        let mut f = entry_formatter(b"a", &mut buf);
        f.format_entry(&color).unwrap();
        // try std.testing.expectEqualSlices(u8, "a = #0a0b0c\n", buf.written());
        assert_eq!(b"a = #0a0b0c\n".as_slice(), buf.written());
    }

    // test "parseCLI with whitespace"
    #[test]
    fn color_parse_cli_with_whitespace() {
        // try testing.expectEqual(Color{ .r = 0xAA, .g = 0xBB, .b = 0xCC }, try Color.parseCLI(" #AABBCC   "));
        assert_eq!(
            Color {
                r: 0xAA,
                g: 0xBB,
                b: 0xCC
            },
            Color::parse_cli(Some(b" #AABBCC   ")).unwrap()
        );
        // try testing.expectEqual(Color{ .r = 0, .g = 0, .b = 0 }, try Color.parseCLI("  black "));
        assert_eq!(
            Color { r: 0, g: 0, b: 0 },
            Color::parse_cli(Some(b"  black ")).unwrap()
        );
    }

    // -----------------------------------------------------------------------
    // TerminalColor
    // -----------------------------------------------------------------------

    // test "parseCLI" (TerminalColor)
    #[test]
    fn terminal_color_parse_cli() {
        // try testing.expectEqual(TerminalColor{ .color = Color{ .r = 78, .g = 42, .b = 132 } }, try TerminalColor.parseCLI("#4e2a84"));
        assert_eq!(
            TerminalColor::Color(Color {
                r: 78,
                g: 42,
                b: 132
            }),
            TerminalColor::parse_cli(Some(b"#4e2a84")).unwrap()
        );
        // try testing.expectEqual(TerminalColor{ .color = Color{ .r = 0, .g = 0, .b = 0 } }, try TerminalColor.parseCLI("black"));
        assert_eq!(
            TerminalColor::Color(Color { r: 0, g: 0, b: 0 }),
            TerminalColor::parse_cli(Some(b"black")).unwrap()
        );
        // try testing.expectEqual(TerminalColor.@"cell-foreground", try TerminalColor.parseCLI("cell-foreground"));
        assert_eq!(
            TerminalColor::CellForeground,
            TerminalColor::parse_cli(Some(b"cell-foreground")).unwrap()
        );
        // try testing.expectEqual(TerminalColor.@"cell-background", try TerminalColor.parseCLI("cell-background"));
        assert_eq!(
            TerminalColor::CellBackground,
            TerminalColor::parse_cli(Some(b"cell-background")).unwrap()
        );

        // try testing.expectError(error.InvalidValue, TerminalColor.parseCLI("a"));
        assert_eq!(
            Err(Error::InvalidValue),
            TerminalColor::parse_cli(Some(b"a"))
        );
    }

    // test "formatConfig" (TerminalColor)
    #[test]
    fn terminal_color_format_config() {
        // var buf: std.Io.Writer.Allocating = .init(testing.allocator);
        let mut buf = AllocatingWriter::init();
        // var sc: TerminalColor = .@"cell-foreground";
        let sc = TerminalColor::CellForeground;
        // try sc.formatEntry(formatterpkg.entryFormatter("a", &buf.writer));
        let mut f = entry_formatter(b"a", &mut buf);
        f.format_entry(&sc).unwrap();
        // try testing.expectEqualSlices(u8, "a = cell-foreground\n", buf.written());
        assert_eq!(b"a = cell-foreground\n".as_slice(), buf.written());
    }

    // -----------------------------------------------------------------------
    // BoldColor
    // -----------------------------------------------------------------------

    // test "parseCLI" (BoldColor)
    #[test]
    fn bold_color_parse_cli() {
        // try testing.expectEqual(BoldColor{ .color = Color{ .r = 78, .g = 42, .b = 132 } }, try BoldColor.parseCLI("#4e2a84"));
        assert_eq!(
            BoldColor::Color(Color {
                r: 78,
                g: 42,
                b: 132
            }),
            BoldColor::parse_cli(Some(b"#4e2a84")).unwrap()
        );
        // try testing.expectEqual(BoldColor{ .color = Color{ .r = 0, .g = 0, .b = 0 } }, try BoldColor.parseCLI("black"));
        assert_eq!(
            BoldColor::Color(Color { r: 0, g: 0, b: 0 }),
            BoldColor::parse_cli(Some(b"black")).unwrap()
        );
        // try testing.expectEqual(BoldColor.bright, try BoldColor.parseCLI("bright"));
        assert_eq!(
            BoldColor::Bright,
            BoldColor::parse_cli(Some(b"bright")).unwrap()
        );

        // try testing.expectError(error.InvalidValue, BoldColor.parseCLI("a"));
        assert_eq!(Err(Error::InvalidValue), BoldColor::parse_cli(Some(b"a")));
    }

    // test "formatConfig" (BoldColor)
    #[test]
    fn bold_color_format_config() {
        let mut buf = AllocatingWriter::init();
        // var sc: BoldColor = .bright;
        let sc = BoldColor::Bright;
        let mut f = entry_formatter(b"a", &mut buf);
        f.format_entry(&sc).unwrap();
        // try testing.expectEqualSlices(u8, "a = bright\n", buf.written());
        assert_eq!(b"a = bright\n".as_slice(), buf.written());
    }

    // -----------------------------------------------------------------------
    // ColorList
    // -----------------------------------------------------------------------

    // test "parseCLI" (ColorList)
    #[test]
    fn color_list_parse_cli() {
        // var p: Self = .{};
        let mut p = ColorList::default();
        // try p.parseCLI(alloc, "black,white");
        p.parse_cli(Some(b"black,white")).unwrap();
        // try testing.expectEqual(2, p.colors.items.len);
        assert_eq!(2, p.colors.len());

        // Test whitespace handling
        // try p.parseCLI(alloc, "black, white"); // space after comma
        p.parse_cli(Some(b"black, white")).unwrap();
        assert_eq!(2, p.colors.len());
        // try p.parseCLI(alloc, "black , white"); // spaces around comma
        p.parse_cli(Some(b"black , white")).unwrap();
        assert_eq!(2, p.colors.len());
        // try p.parseCLI(alloc, " black , white "); // extra spaces at ends
        p.parse_cli(Some(b" black , white ")).unwrap();
        assert_eq!(2, p.colors.len());

        // Error cases
        // try testing.expectError(error.ValueRequired, p.parseCLI(alloc, null));
        assert_eq!(Err(Error::ValueRequired), p.parse_cli(None));
        // try testing.expectError(error.InvalidValue, p.parseCLI(alloc, " "));
        assert_eq!(Err(Error::InvalidValue), p.parse_cli(Some(b" ")));
    }

    // test "format" (ColorList)
    #[test]
    fn color_list_format() {
        let mut buf = AllocatingWriter::init();
        let mut p = ColorList::default();
        // try p.parseCLI(alloc, "black,white");
        p.parse_cli(Some(b"black,white")).unwrap();
        // try p.formatEntry(formatterpkg.entryFormatter("a", &buf.writer));
        let mut f = entry_formatter(b"a", &mut buf);
        f.format_entry(&p).unwrap();
        // try std.testing.expectEqualSlices(u8, "a = #000000,#ffffff\n", buf.written());
        assert_eq!(b"a = #000000,#ffffff\n".as_slice(), buf.written());
    }

    // test "clone" (ColorList)
    #[test]
    fn color_list_clone() {
        let mut source = ColorList::default();
        // try source.parseCLI(alloc, "#ff0000,#00ff00,#0000ff");
        source.parse_cli(Some(b"#ff0000,#00ff00,#0000ff")).unwrap();

        // const cloned = try source.clone(alloc);
        let cloned = source.clone_in();

        // try testing.expect(source.equal(cloned));
        assert!(source.equal(&cloned));
        // try testing.expectEqual(source.colors_c.items.len, cloned.colors_c.items.len);
        assert_eq!(source.colors_c.len(), cloned.colors_c.len());
        // for (source.colors_c.items, cloned.colors_c.items) |src_c, clone_c| { ... }
        for (src_c, clone_c) in source.colors_c.iter().zip(cloned.colors_c.iter()) {
            assert_eq!(src_c.r, clone_c.r);
            assert_eq!(src_c.g, clone_c.g);
            assert_eq!(src_c.b, clone_c.b);
        }
    }

    // -----------------------------------------------------------------------
    // Palette
    // -----------------------------------------------------------------------

    // test "parseCLI" (Palette)
    #[test]
    fn palette_parse_cli() {
        let mut p = Palette::default();
        // try p.parseCLI("0=#AABBCC");
        p.parse_cli(Some(b"0=#AABBCC")).unwrap();
        // try testing.expect(p.value[0].r == 0xAA);
        assert!(p.value[0].r == 0xAA);
        assert!(p.value[0].g == 0xBB);
        assert!(p.value[0].b == 0xCC);
        // try testing.expect(p.mask.isSet(0));
        assert!(p.mask.is_set(0));
        // try testing.expect(!p.mask.isSet(1));
        assert!(!p.mask.is_set(1));
    }

    // test "parseCLI base" (Palette)
    #[test]
    fn palette_parse_cli_base() {
        let mut p = Palette::default();

        // try p.parseCLI("0b1=#014589");
        p.parse_cli(Some(b"0b1=#014589")).unwrap();
        // try p.parseCLI("0o7=#234567");
        p.parse_cli(Some(b"0o7=#234567")).unwrap();
        // try p.parseCLI("0xF=#ABCDEF");
        p.parse_cli(Some(b"0xF=#ABCDEF")).unwrap();

        assert!(p.value[0b1].r == 0x01);
        assert!(p.value[0b1].g == 0x45);
        assert!(p.value[0b1].b == 0x89);

        assert!(p.value[0o7].r == 0x23);
        assert!(p.value[0o7].g == 0x45);
        assert!(p.value[0o7].b == 0x67);

        assert!(p.value[0xF].r == 0xAB);
        assert!(p.value[0xF].g == 0xCD);
        assert!(p.value[0xF].b == 0xEF);

        assert!(p.mask.is_set(0b1));
        assert!(p.mask.is_set(0o7));
        assert!(p.mask.is_set(0xF));
        assert!(!p.mask.is_set(0));
        assert!(!p.mask.is_set(2));
    }

    // test "parseCLI overflow" (Palette)
    #[test]
    fn palette_parse_cli_overflow() {
        let mut p = Palette::default();
        // try testing.expectError(error.Overflow, p.parseCLI("256=#AABBCC"));
        assert_eq!(
            Err(PaletteParseError::Overflow),
            p.parse_cli(Some(b"256=#AABBCC"))
        );
        // Mask should remain empty since parsing failed.
        // try testing.expectEqual(@as(usize, 0), p.mask.count());
        assert_eq!(0usize, p.mask.count());
    }

    // test "formatConfig" (Palette)
    #[test]
    fn palette_format_config() {
        let mut buf = AllocatingWriter::init();
        let list = Palette::default();
        let mut f = entry_formatter(b"a", &mut buf);
        f.format_entry(&list).unwrap();
        // try std.testing.expectEqualSlices(u8, "a = 0=#1d1f21\n", buf.written()[0..14]);
        assert_eq!(b"a = 0=#1d1f21\n".as_slice(), &buf.written()[0..14]);
    }

    // test "parseCLI with whitespace" (Palette)
    #[test]
    fn palette_parse_cli_with_whitespace() {
        let mut p = Palette::default();
        // try p.parseCLI("0 =  #AABBCC");
        p.parse_cli(Some(b"0 =  #AABBCC")).unwrap();
        // try p.parseCLI(" 1= #DDEEFF    ");
        p.parse_cli(Some(b" 1= #DDEEFF    ")).unwrap();
        // try p.parseCLI("  2  =  #123456 ");
        p.parse_cli(Some(b"  2  =  #123456 ")).unwrap();

        assert!(p.value[0].r == 0xAA);
        assert!(p.value[0].g == 0xBB);
        assert!(p.value[0].b == 0xCC);

        assert!(p.value[1].r == 0xDD);
        assert!(p.value[1].g == 0xEE);
        assert!(p.value[1].b == 0xFF);

        assert!(p.value[2].r == 0x12);
        assert!(p.value[2].g == 0x34);
        assert!(p.value[2].b == 0x56);

        assert!(p.mask.is_set(0));
        assert!(p.mask.is_set(1));
        assert!(p.mask.is_set(2));
        assert!(!p.mask.is_set(3));
    }

    // -----------------------------------------------------------------------
    // RepeatableString
    // -----------------------------------------------------------------------

    // test "parseCLI" (RepeatableString)
    #[test]
    fn repeatable_string_parse_cli() {
        let mut list = RepeatableString::default();
        // try list.parseCLI(alloc, "A");
        list.parse_cli(Some(b"A")).unwrap();
        // try list.parseCLI(alloc, "B");
        list.parse_cli(Some(b"B")).unwrap();
        // try testing.expectEqual(@as(usize, 2), list.list.items.len);
        assert_eq!(2usize, list.list.len());

        // try list.parseCLI(alloc, "");
        list.parse_cli(Some(b"")).unwrap();
        // try testing.expectEqual(@as(usize, 0), list.list.items.len);
        assert_eq!(0usize, list.list.len());
    }

    // test "parseCLI overwrite" (RepeatableString)
    #[test]
    fn repeatable_string_parse_cli_overwrite() {
        let mut list = RepeatableString::default();
        // try list.parseCLI(alloc, "A");
        list.parse_cli(Some(b"A")).unwrap();

        // Set our overwrite flag
        // list.overwrite_next = true;
        list.overwrite_next = true;

        // try list.parseCLI(alloc, "B");
        list.parse_cli(Some(b"B")).unwrap();
        // try testing.expectEqual(@as(usize, 1), list.list.items.len);
        assert_eq!(1usize, list.list.len());
        // try list.parseCLI(alloc, "C");
        list.parse_cli(Some(b"C")).unwrap();
        // try testing.expectEqual(@as(usize, 2), list.list.items.len);
        assert_eq!(2usize, list.list.len());
    }

    // test "formatConfig empty" (RepeatableString)
    #[test]
    fn repeatable_string_format_config_empty() {
        let mut buf = AllocatingWriter::init();
        let list = RepeatableString::default();
        let mut f = entry_formatter(b"a", &mut buf);
        f.format_entry(&list).unwrap();
        // try std.testing.expectEqualSlices(u8, "a = \n", buf.written());
        assert_eq!(b"a = \n".as_slice(), buf.written());
    }

    // test "formatConfig single item" (RepeatableString)
    #[test]
    fn repeatable_string_format_config_single_item() {
        let mut buf = AllocatingWriter::init();
        let mut list = RepeatableString::default();
        // try list.parseCLI(alloc, "A");
        list.parse_cli(Some(b"A")).unwrap();
        let mut f = entry_formatter(b"a", &mut buf);
        f.format_entry(&list).unwrap();
        // try std.testing.expectEqualSlices(u8, "a = A\n", buf.written());
        assert_eq!(b"a = A\n".as_slice(), buf.written());
    }

    // test "formatConfig multiple items" (RepeatableString)
    #[test]
    fn repeatable_string_format_config_multiple_items() {
        let mut buf = AllocatingWriter::init();
        let mut list = RepeatableString::default();
        list.parse_cli(Some(b"A")).unwrap();
        list.parse_cli(Some(b"B")).unwrap();
        let mut f = entry_formatter(b"a", &mut buf);
        f.format_entry(&list).unwrap();
        // try std.testing.expectEqualSlices(u8, "a = A\na = B\n", buf.written());
        assert_eq!(b"a = A\na = B\n".as_slice(), buf.written());
    }

    // -----------------------------------------------------------------------
    // SelectionWordChars
    // -----------------------------------------------------------------------

    // test "parseCLI" (SelectionWordChars)
    #[test]
    fn selection_word_chars_parse_cli() {
        let mut chars = SelectionWordChars::default();
        // try chars.parseCLI(alloc, " \t;,");
        chars.parse_cli(Some(b" \t;,")).unwrap();

        // Should have null + 4 characters
        // try testing.expectEqual(@as(usize, 5), chars.codepoints.len);
        assert_eq!(5usize, chars.codepoints.len());
        assert_eq!(0u32, chars.codepoints[0]);
        assert_eq!(b' ' as u32, chars.codepoints[1]);
        assert_eq!(b'\t' as u32, chars.codepoints[2]);
        assert_eq!(b';' as u32, chars.codepoints[3]);
        assert_eq!(b',' as u32, chars.codepoints[4]);
    }

    // test "parseCLI escape sequences" (SelectionWordChars)
    #[test]
    fn selection_word_chars_parse_cli_escape_sequences() {
        // \t escape should be parsed as tab
        let mut chars = SelectionWordChars::default();
        // try chars.parseCLI(alloc, " \\t;,");
        chars.parse_cli(Some(b" \\t;,")).unwrap();

        assert_eq!(5usize, chars.codepoints.len());
        assert_eq!(0u32, chars.codepoints[0]);
        assert_eq!(b' ' as u32, chars.codepoints[1]);
        assert_eq!(b'\t' as u32, chars.codepoints[2]);
        assert_eq!(b';' as u32, chars.codepoints[3]);
        assert_eq!(b',' as u32, chars.codepoints[4]);
    }

    // test "parseCLI backslash escape" (SelectionWordChars)
    #[test]
    fn selection_word_chars_parse_cli_backslash_escape() {
        // \\ should be parsed as a single backslash
        let mut chars = SelectionWordChars::default();
        // try chars.parseCLI(alloc, "\\\\;");
        // PORT NOTE: the reference Zig literal `"\\\\;"` is the 3 bytes `\`,`\`,`;`;
        // the codepoint iterator decodes the `\\` escape to one backslash.
        chars.parse_cli(Some(b"\\\\;")).unwrap();

        assert_eq!(3usize, chars.codepoints.len());
        assert_eq!(0u32, chars.codepoints[0]);
        assert_eq!(b'\\' as u32, chars.codepoints[1]);
        assert_eq!(b';' as u32, chars.codepoints[2]);
    }

    // test "parseCLI unicode escape" (SelectionWordChars)
    #[test]
    fn selection_word_chars_parse_cli_unicode_escape() {
        // \u{2502} should be parsed as │
        let mut chars = SelectionWordChars::default();
        // try chars.parseCLI(alloc, "\\u{2502};");
        chars.parse_cli(Some(b"\\u{2502};")).unwrap();

        assert_eq!(3usize, chars.codepoints.len());
        assert_eq!(0u32, chars.codepoints[0]);
        assert_eq!('│' as u32, chars.codepoints[1]);
        assert_eq!(b';' as u32, chars.codepoints[2]);
    }

    // -----------------------------------------------------------------------
    // RepeatableFontVariation
    // -----------------------------------------------------------------------

    // test "parseCLI" (RepeatableFontVariation)
    #[test]
    fn repeatable_font_variation_parse_cli() {
        let mut list = RepeatableFontVariation::default();
        // try list.parseCLI(alloc, "wght=200");
        list.parse_cli(Some(b"wght=200")).unwrap();
        // try list.parseCLI(alloc, "slnt=-15");
        list.parse_cli(Some(b"slnt=-15")).unwrap();

        // try testing.expectEqual(@as(usize, 2), list.list.items.len);
        assert_eq!(2usize, list.list.len());
        // try testing.expectEqual(fontpkg.face.Variation{ .id = Id.init("wght"), .value = 200 }, list.list.items[0]);
        assert_eq!(
            FaceVariation {
                id: face_variation_id_init(b"wght"),
                value: 200.0
            },
            list.list[0]
        );
        // try testing.expectEqual(fontpkg.face.Variation{ .id = Id.init("slnt"), .value = -15 }, list.list.items[1]);
        assert_eq!(
            FaceVariation {
                id: face_variation_id_init(b"slnt"),
                value: -15.0
            },
            list.list[1]
        );
    }

    // test "parseCLI with whitespace" (RepeatableFontVariation)
    #[test]
    fn repeatable_font_variation_parse_cli_with_whitespace() {
        let mut list = RepeatableFontVariation::default();
        // try list.parseCLI(alloc, "wght =200");
        list.parse_cli(Some(b"wght =200")).unwrap();
        // try list.parseCLI(alloc, "slnt= -15");
        list.parse_cli(Some(b"slnt= -15")).unwrap();

        assert_eq!(2usize, list.list.len());
        assert_eq!(
            FaceVariation {
                id: face_variation_id_init(b"wght"),
                value: 200.0
            },
            list.list[0]
        );
        assert_eq!(
            FaceVariation {
                id: face_variation_id_init(b"slnt"),
                value: -15.0
            },
            list.list[1]
        );
    }

    // test "formatConfig single" (RepeatableFontVariation)
    #[test]
    fn repeatable_font_variation_format_config_single() {
        let mut buf = AllocatingWriter::init();
        let mut list = RepeatableFontVariation::default();
        // try list.parseCLI(alloc, "wght = 200");
        list.parse_cli(Some(b"wght = 200")).unwrap();
        let mut f = entry_formatter(b"a", &mut buf);
        f.format_entry(&list).unwrap();
        // try std.testing.expectEqualSlices(u8, "a = wght=200\n", buf.written());
        assert_eq!(b"a = wght=200\n".as_slice(), buf.written());
    }

    // -----------------------------------------------------------------------
    // WorkingDirectory
    // -----------------------------------------------------------------------

    // test "WorkingDirectory parseCLI"
    #[test]
    fn working_directory_parse_cli() {
        // var wd: Self = .inherit;
        let mut wd = WorkingDirectory::Inherit;

        // try wd.parseCLI(alloc, "inherit");
        wd.parse_cli(Some(b"inherit")).unwrap();
        // try testing.expectEqual(.inherit, wd);
        assert_eq!(WorkingDirectory::Inherit, wd);

        // try wd.parseCLI(alloc, "home");
        wd.parse_cli(Some(b"home")).unwrap();
        // try testing.expectEqual(.home, wd);
        assert_eq!(WorkingDirectory::Home, wd);

        // try wd.parseCLI(alloc, "~/projects/ghostty");
        wd.parse_cli(Some(b"~/projects/ghostty")).unwrap();
        // try testing.expectEqualStrings("~/projects/ghostty", wd.path);
        assert_eq!(Some(b"~/projects/ghostty".as_slice()), wd.value());

        // try wd.parseCLI(alloc, "\"/tmp path\"");
        wd.parse_cli(Some(b"\"/tmp path\"")).unwrap();
        // try testing.expectEqualStrings("/tmp path", wd.path);
        assert_eq!(Some(b"/tmp path".as_slice()), wd.value());
    }

    // test "WorkingDirectory finalize"
    #[test]
    fn working_directory_finalize() {
        // var wd: Self = .{ .path = "~/projects/ghostty" };
        let mut wd = WorkingDirectory::Path(b"~/projects/ghostty".to_vec());
        // try wd.finalize(alloc);
        wd.finalize();

        // var buf: [std.fs.max_path_bytes]u8 = undefined;
        // const expected = internal_os.expandHome("~/projects/ghostty", &buf) catch "~/projects/ghostty";
        let mut buf = [0u8; MAX_PATH_BYTES];
        let expected: Vec<u8> = match rustty_os::homedir::expandHome(
            &rustty_global::environ_map(),
            b"~/projects/ghostty",
            &mut buf,
        ) {
            Ok(v) => v.to_vec(),
            Err(_) => b"~/projects/ghostty".to_vec(),
        };
        // try testing.expectEqualStrings(expected, wd.value().?);
        assert_eq!(expected.as_slice(), wd.value().unwrap());
    }

    // -----------------------------------------------------------------------
    // FontStyle
    // -----------------------------------------------------------------------

    // test "parseCLI" (FontStyle)
    #[test]
    fn font_style_parse_cli() {
        // var p: Self = .{ .default = {} };
        let mut p = FontStyle::Default;
        // try p.parseCLI(alloc, "default");
        p.parse_cli(Some(b"default")).unwrap();
        // try testing.expectEqual(Self{ .default = {} }, p);
        assert_eq!(FontStyle::Default, p);

        // try p.parseCLI(alloc, "false");
        p.parse_cli(Some(b"false")).unwrap();
        // try testing.expectEqual(Self{ .false = {} }, p);
        assert_eq!(FontStyle::False, p);

        // try p.parseCLI(alloc, "bold");
        p.parse_cli(Some(b"bold")).unwrap();
        // try testing.expectEqualStrings("bold", p.name);
        assert_eq!(Some(b"bold".as_slice()), p.name_value());
    }

    // test "formatConfig default" (FontStyle)
    #[test]
    fn font_style_format_config_default() {
        let mut buf = AllocatingWriter::init();
        let mut p = FontStyle::Default;
        p.parse_cli(Some(b"default")).unwrap();
        let mut f = entry_formatter(b"a", &mut buf);
        f.format_entry(&p).unwrap();
        // try std.testing.expectEqualSlices(u8, "a = default\n", buf.written());
        assert_eq!(b"a = default\n".as_slice(), buf.written());
    }

    // test "formatConfig false" (FontStyle)
    #[test]
    fn font_style_format_config_false() {
        let mut buf = AllocatingWriter::init();
        let mut p = FontStyle::Default;
        p.parse_cli(Some(b"false")).unwrap();
        let mut f = entry_formatter(b"a", &mut buf);
        f.format_entry(&p).unwrap();
        // try std.testing.expectEqualSlices(u8, "a = false\n", buf.written());
        assert_eq!(b"a = false\n".as_slice(), buf.written());
    }

    // test "formatConfig named" (FontStyle)
    #[test]
    fn font_style_format_config_named() {
        let mut buf = AllocatingWriter::init();
        let mut p = FontStyle::Default;
        p.parse_cli(Some(b"bold")).unwrap();
        let mut f = entry_formatter(b"a", &mut buf);
        f.format_entry(&p).unwrap();
        // try std.testing.expectEqualSlices(u8, "a = bold\n", buf.written());
        assert_eq!(b"a = bold\n".as_slice(), buf.written());
    }

    // -----------------------------------------------------------------------
    // RepeatableClipboardCodepointMap
    // -----------------------------------------------------------------------

    // test "parseCLI codepoint replacement"
    #[test]
    fn repeatable_clipboard_codepoint_map_parse_cli_codepoint_replacement() {
        use rustty_vt::formatter::Replacement;
        let mut list = RepeatableClipboardCodepointMap::default();
        // try list.parseCLI(alloc, "U+2500=U+002D"); // box drawing → hyphen
        list.parse_cli(Some(b"U+2500=U+002D")).unwrap();

        // try testing.expectEqual(@as(usize, 1), list.map.list.len);
        assert_eq!(1usize, list.map.list.len());
        // const entry = list.map.list.get(0);
        let entry = &list.map.list[0];
        // try testing.expectEqual([2]u21{ 0x2500, 0x2500 }, entry.range);
        assert_eq!([0x2500u32, 0x2500u32], entry.range);
        // try testing.expect(entry.replacement == .codepoint);
        assert!(matches!(entry.replacement, Replacement::Codepoint(_)));
        // try testing.expectEqual(@as(u21, 0x002D), entry.replacement.codepoint);
        assert!(matches!(entry.replacement, Replacement::Codepoint(0x002D)));
    }

    // test "parseCLI string replacement"
    #[test]
    fn repeatable_clipboard_codepoint_map_parse_cli_string_replacement() {
        use rustty_vt::formatter::Replacement;
        let mut list = RepeatableClipboardCodepointMap::default();
        // try list.parseCLI(alloc, "U+03A3=SUM"); // Greek sigma → "SUM"
        list.parse_cli(Some(b"U+03A3=SUM")).unwrap();

        assert_eq!(1usize, list.map.list.len());
        let entry = &list.map.list[0];
        // try testing.expectEqual([2]u21{ 0x03A3, 0x03A3 }, entry.range);
        assert_eq!([0x03A3u32, 0x03A3u32], entry.range);
        // try testing.expect(entry.replacement == .string);
        // try testing.expectEqualStrings("SUM", entry.replacement.string);
        match &entry.replacement {
            Replacement::String(s) => assert_eq!(b"SUM".as_slice(), s.as_slice()),
            _ => panic!("expected string replacement"),
        }
    }

    // test "parseCLI range replacement"
    #[test]
    fn repeatable_clipboard_codepoint_map_parse_cli_range_replacement() {
        use rustty_vt::formatter::Replacement;
        let mut list = RepeatableClipboardCodepointMap::default();
        // try list.parseCLI(alloc, "U+2500-U+2503=|"); // box drawing range → pipe
        list.parse_cli(Some(b"U+2500-U+2503=|")).unwrap();

        assert_eq!(1usize, list.map.list.len());
        let entry = &list.map.list[0];
        // try testing.expectEqual([2]u21{ 0x2500, 0x2503 }, entry.range);
        assert_eq!([0x2500u32, 0x2503u32], entry.range);
        match &entry.replacement {
            Replacement::String(s) => assert_eq!(b"|".as_slice(), s.as_slice()),
            _ => panic!("expected string replacement"),
        }
    }

    // test "formatConfig codepoint"
    #[test]
    fn repeatable_clipboard_codepoint_map_format_config_codepoint() {
        let mut buf = AllocatingWriter::init();
        let mut list = RepeatableClipboardCodepointMap::default();
        list.parse_cli(Some(b"U+2500=U+002D")).unwrap();
        let mut f = entry_formatter(b"a", &mut buf);
        f.format_entry(&list).unwrap();
        // try std.testing.expectEqualSlices(u8, "a = U+2500=U+002D\n", buf.written());
        assert_eq!(b"a = U+2500=U+002D\n".as_slice(), buf.written());
    }

    // test "formatConfig string"
    #[test]
    fn repeatable_clipboard_codepoint_map_format_config_string() {
        let mut buf = AllocatingWriter::init();
        let mut list = RepeatableClipboardCodepointMap::default();
        list.parse_cli(Some(b"U+03A3=SUM")).unwrap();
        let mut f = entry_formatter(b"a", &mut buf);
        f.format_entry(&list).unwrap();
        // try std.testing.expectEqualSlices(u8, "a = U+03A3=SUM\n", buf.written());
        assert_eq!(b"a = U+03A3=SUM\n".as_slice(), buf.written());
    }

    // -----------------------------------------------------------------------
    // RepeatableCodepointMap
    // -----------------------------------------------------------------------

    // test "parseCLI"
    #[test]
    fn repeatable_codepoint_map_parse_cli() {
        // var list: Self = .{};
        let mut list = RepeatableCodepointMap::default();
        // try list.parseCLI(alloc, "U+ABCD=Comic Sans");
        list.parse_cli(Some(b"U+ABCD=Comic Sans")).unwrap();
        // try list.parseCLI(alloc, "U+0001 - U+0005=Verdana");
        list.parse_cli(Some(b"U+0001 - U+0005=Verdana")).unwrap();
        // try list.parseCLI(alloc, "U+0006-U+0009, U+ABCD=Courier");
        list.parse_cli(Some(b"U+0006-U+0009, U+ABCD=Courier"))
            .unwrap();

        // try testing.expectEqual(@as(usize, 4), list.map.list.len);
        assert_eq!(4usize, list.map.entries().len());
        {
            // const entry = list.map.list.get(0);
            let entry = &list.map.entries()[0];
            // try testing.expectEqual([2]u21{ 0xABCD, 0xABCD }, entry.range);
            assert_eq!([0xABCDu32, 0xABCDu32], entry.range);
            // try testing.expectEqualStrings("Comic Sans", entry.descriptor.family.?);
            assert_eq!(
                b"Comic Sans".as_slice(),
                entry.descriptor.family.as_deref().unwrap()
            );
        }
        {
            // const entry = list.map.list.get(1);
            let entry = &list.map.entries()[1];
            // try testing.expectEqual([2]u21{ 1, 5 }, entry.range);
            assert_eq!([1u32, 5u32], entry.range);
            // try testing.expectEqualStrings("Verdana", entry.descriptor.family.?);
            assert_eq!(
                b"Verdana".as_slice(),
                entry.descriptor.family.as_deref().unwrap()
            );
        }
        {
            // const entry = list.map.list.get(2);
            let entry = &list.map.entries()[2];
            // try testing.expectEqual([2]u21{ 6, 9 }, entry.range);
            assert_eq!([6u32, 9u32], entry.range);
            // try testing.expectEqualStrings("Courier", entry.descriptor.family.?);
            assert_eq!(
                b"Courier".as_slice(),
                entry.descriptor.family.as_deref().unwrap()
            );
        }
        {
            // const entry = list.map.list.get(3);
            let entry = &list.map.entries()[3];
            // try testing.expectEqual([2]u21{ 0xABCD, 0xABCD }, entry.range);
            assert_eq!([0xABCDu32, 0xABCDu32], entry.range);
            // try testing.expectEqualStrings("Courier", entry.descriptor.family.?);
            assert_eq!(
                b"Courier".as_slice(),
                entry.descriptor.family.as_deref().unwrap()
            );
        }
    }

    // test "formatConfig single"
    #[test]
    fn repeatable_codepoint_map_format_config_single() {
        // var buf: std.Io.Writer.Allocating = .init(testing.allocator);
        let mut buf = AllocatingWriter::init();
        // var list: Self = .{};
        let mut list = RepeatableCodepointMap::default();
        // try list.parseCLI(alloc, "U+ABCD=Comic Sans");
        list.parse_cli(Some(b"U+ABCD=Comic Sans")).unwrap();
        // try list.formatEntry(formatterpkg.entryFormatter("a", &buf.writer));
        let mut f = entry_formatter(b"a", &mut buf);
        f.format_entry(&list).unwrap();
        // try std.testing.expectEqualSlices(u8, "a = U+ABCD=Comic Sans\n", buf.written());
        assert_eq!(b"a = U+ABCD=Comic Sans\n".as_slice(), buf.written());
    }

    // test "formatConfig range"
    #[test]
    fn repeatable_codepoint_map_format_config_range() {
        let mut buf = AllocatingWriter::init();
        let mut list = RepeatableCodepointMap::default();
        // try list.parseCLI(alloc, "U+0001 - U+0005=Verdana");
        list.parse_cli(Some(b"U+0001 - U+0005=Verdana")).unwrap();
        let mut f = entry_formatter(b"a", &mut buf);
        f.format_entry(&list).unwrap();
        // try std.testing.expectEqualSlices(u8, "a = U+0001-U+0005=Verdana\n", buf.written());
        assert_eq!(b"a = U+0001-U+0005=Verdana\n".as_slice(), buf.written());
    }

    // test "formatConfig multiple"
    #[test]
    fn repeatable_codepoint_map_format_config_multiple() {
        let mut buf = AllocatingWriter::init();
        let mut list = RepeatableCodepointMap::default();
        // try list.parseCLI(alloc, "U+0006-U+0009, U+ABCD=Courier");
        list.parse_cli(Some(b"U+0006-U+0009, U+ABCD=Courier"))
            .unwrap();
        let mut f = entry_formatter(b"a", &mut buf);
        f.format_entry(&list).unwrap();
        // try std.testing.expectEqualSlices(u8,
        //     \\a = U+0006-U+0009=Courier
        //     \\a = U+ABCD=Courier
        //     \\
        // , buf.written());
        assert_eq!(
            b"a = U+0006-U+0009=Courier\na = U+ABCD=Courier\n".as_slice(),
            buf.written()
        );
    }

    // -----------------------------------------------------------------------
    // MouseScrollMultiplier
    // -----------------------------------------------------------------------

    // test "parse" (MouseScrollMultiplier)
    #[test]
    fn mouse_scroll_multiplier_parse() {
        // const epsilon = 0.00001;
        let epsilon = 0.00001;

        // var args: Self = .{ .precision = 0.1, .discrete = 3 };
        let mut args = MouseScrollMultiplier {
            precision: 0.1,
            discrete: 3.0,
        };
        // try args.parseCLI(alloc, "3");
        args.parse_cli(Some(b"3")).unwrap();
        // try testing.expectApproxEqAbs(3, args.precision, epsilon);
        assert!((3.0 - args.precision).abs() <= epsilon);
        assert!((3.0 - args.discrete).abs() <= epsilon);

        args = MouseScrollMultiplier {
            precision: 0.1,
            discrete: 3.0,
        };
        // try args.parseCLI(alloc, "precision:1");
        args.parse_cli(Some(b"precision:1")).unwrap();
        assert!((1.0 - args.precision).abs() <= epsilon);
        assert!((3.0 - args.discrete).abs() <= epsilon);

        args = MouseScrollMultiplier {
            precision: 0.1,
            discrete: 3.0,
        };
        // try args.parseCLI(alloc, "discrete:5");
        args.parse_cli(Some(b"discrete:5")).unwrap();
        assert!((0.1 - args.precision).abs() <= epsilon);
        assert!((5.0 - args.discrete).abs() <= epsilon);

        args = MouseScrollMultiplier {
            precision: 0.1,
            discrete: 3.0,
        };
        // try args.parseCLI(alloc, "precision:3,discrete:7");
        args.parse_cli(Some(b"precision:3,discrete:7")).unwrap();
        assert!((3.0 - args.precision).abs() <= epsilon);
        assert!((7.0 - args.discrete).abs() <= epsilon);

        args = MouseScrollMultiplier {
            precision: 0.1,
            discrete: 3.0,
        };
        // try args.parseCLI(alloc, "discrete:8,precision:6");
        args.parse_cli(Some(b"discrete:8,precision:6")).unwrap();
        assert!((6.0 - args.precision).abs() <= epsilon);
        assert!((8.0 - args.discrete).abs() <= epsilon);

        // args = .default;
        args = MouseScrollMultiplier::default();
        // try testing.expectError(error.InvalidValue, args.parseCLI(alloc, "foo:1"));
        assert_eq!(Err(Error::InvalidValue), args.parse_cli(Some(b"foo:1")));
        // try testing.expectError(error.InvalidValue, args.parseCLI(alloc, "precision:bar"));
        assert_eq!(
            Err(Error::InvalidValue),
            args.parse_cli(Some(b"precision:bar"))
        );
        // try testing.expectError(error.InvalidValue, args.parseCLI(alloc, "precision:1,discrete:3,foo:5"));
        assert_eq!(
            Err(Error::InvalidValue),
            args.parse_cli(Some(b"precision:1,discrete:3,foo:5"))
        );
        // try testing.expectError(error.InvalidValue, args.parseCLI(alloc, "precision:1,,discrete:3"));
        assert_eq!(
            Err(Error::InvalidValue),
            args.parse_cli(Some(b"precision:1,,discrete:3"))
        );
        // try testing.expectError(error.InvalidValue, args.parseCLI(alloc, ",precision:1,discrete:3"));
        assert_eq!(
            Err(Error::InvalidValue),
            args.parse_cli(Some(b",precision:1,discrete:3"))
        );
    }

    // test "format entry MouseScrollMultiplier"
    #[test]
    fn format_entry_mouse_scroll_multiplier() {
        let mut buf = AllocatingWriter::init();
        // var args: Self = .{ .precision = 1.5, .discrete = 2.5 };
        let args = MouseScrollMultiplier {
            precision: 1.5,
            discrete: 2.5,
        };
        let mut f = entry_formatter(b"mouse-scroll-multiplier", &mut buf);
        f.format_entry(&args).unwrap();
        // try testing.expectEqualSlices(u8, "mouse-scroll-multiplier = precision:1.5,discrete:2.5\n", buf.written());
        assert_eq!(
            b"mouse-scroll-multiplier = precision:1.5,discrete:2.5\n".as_slice(),
            buf.written()
        );
    }

    // -----------------------------------------------------------------------
    // QuickTerminalSize
    // -----------------------------------------------------------------------

    // test "parse QuickTerminalSize"
    #[test]
    fn parse_quick_terminal_size() {
        // var v: QuickTerminalSize = undefined;
        let mut v = QuickTerminalSize::default();

        // try v.parseCLI("50%");
        v.parse_cli(Some(b"50%")).unwrap();
        // try testing.expectEqual(50, v.primary.?.percentage);
        assert_eq!(Some(QuickTerminalSizeSize::Percentage(50.0)), v.primary);
        // try testing.expectEqual(null, v.secondary);
        assert_eq!(None, v.secondary);

        // try v.parseCLI("200px");
        v.parse_cli(Some(b"200px")).unwrap();
        // try testing.expectEqual(200, v.primary.?.pixels);
        assert_eq!(Some(QuickTerminalSizeSize::Pixels(200)), v.primary);
        assert_eq!(None, v.secondary);

        // try v.parseCLI("50%,200px");
        v.parse_cli(Some(b"50%,200px")).unwrap();
        assert_eq!(Some(QuickTerminalSizeSize::Percentage(50.0)), v.primary);
        assert_eq!(Some(QuickTerminalSizeSize::Pixels(200)), v.secondary);

        // try testing.expectError(error.ValueRequired, v.parseCLI(null));
        assert_eq!(
            Err(QuickTerminalSizeParseError::ValueRequired),
            v.parse_cli(None)
        );
        // try testing.expectError(error.ValueRequired, v.parseCLI(""));
        assert_eq!(
            Err(QuickTerminalSizeParseError::ValueRequired),
            v.parse_cli(Some(b""))
        );
        // try testing.expectError(error.ValueRequired, v.parseCLI("69px,"));
        assert_eq!(
            Err(QuickTerminalSizeParseError::ValueRequired),
            v.parse_cli(Some(b"69px,"))
        );
        // try testing.expectError(error.TooManyArguments, v.parseCLI("69px,42%,69px"));
        assert_eq!(
            Err(QuickTerminalSizeParseError::TooManyArguments),
            v.parse_cli(Some(b"69px,42%,69px"))
        );

        // try testing.expectError(error.MissingUnit, v.parseCLI("420"));
        assert_eq!(
            Err(QuickTerminalSizeParseError::MissingUnit),
            v.parse_cli(Some(b"420"))
        );
        // try testing.expectError(error.MissingUnit, v.parseCLI("bobr"));
        assert_eq!(
            Err(QuickTerminalSizeParseError::MissingUnit),
            v.parse_cli(Some(b"bobr"))
        );
        // try testing.expectError(error.InvalidValue, v.parseCLI("bobr%"));
        assert_eq!(
            Err(QuickTerminalSizeParseError::InvalidValue),
            v.parse_cli(Some(b"bobr%"))
        );
        // try testing.expectError(error.InvalidValue, v.parseCLI("-32%"));
        assert_eq!(
            Err(QuickTerminalSizeParseError::InvalidValue),
            v.parse_cli(Some(b"-32%"))
        );
        // try testing.expectError(error.InvalidValue, v.parseCLI("-69px"));
        assert_eq!(
            Err(QuickTerminalSizeParseError::InvalidValue),
            v.parse_cli(Some(b"-69px"))
        );
    }

    // test "calculate QuickTerminalSize"
    #[test]
    fn calculate_quick_terminal_size() {
        // const dims_landscape: Dimensions = .{ .width = 2560, .height = 1600 };
        let dims_landscape = QuickTerminalSizeDimensions {
            width: 2560,
            height: 1600,
        };
        // const dims_portrait: Dimensions = .{ .width = 1600, .height = 2560 };
        let dims_portrait = QuickTerminalSizeDimensions {
            width: 1600,
            height: 2560,
        };

        {
            // const size: QuickTerminalSize = .{};
            let size = QuickTerminalSize::default();
            assert_eq!(
                QuickTerminalSizeDimensions {
                    width: 2560,
                    height: 400
                },
                size.calculate(QuickTerminalPosition::Top, dims_landscape)
            );
            assert_eq!(
                QuickTerminalSizeDimensions {
                    width: 400,
                    height: 1600
                },
                size.calculate(QuickTerminalPosition::Left, dims_landscape)
            );
            assert_eq!(
                QuickTerminalSizeDimensions {
                    width: 800,
                    height: 400
                },
                size.calculate(QuickTerminalPosition::Center, dims_landscape)
            );
            assert_eq!(
                QuickTerminalSizeDimensions {
                    width: 400,
                    height: 800
                },
                size.calculate(QuickTerminalPosition::Center, dims_portrait)
            );
        }
        {
            // const size: QuickTerminalSize = .{ .primary = .{ .percentage = 20 } };
            let size = QuickTerminalSize {
                primary: Some(QuickTerminalSizeSize::Percentage(20.0)),
                secondary: None,
            };
            assert_eq!(
                QuickTerminalSizeDimensions {
                    width: 2560,
                    height: 320
                },
                size.calculate(QuickTerminalPosition::Top, dims_landscape)
            );
            assert_eq!(
                QuickTerminalSizeDimensions {
                    width: 512,
                    height: 1600
                },
                size.calculate(QuickTerminalPosition::Left, dims_landscape)
            );
            assert_eq!(
                QuickTerminalSizeDimensions {
                    width: 512,
                    height: 400
                },
                size.calculate(QuickTerminalPosition::Center, dims_landscape)
            );
            assert_eq!(
                QuickTerminalSizeDimensions {
                    width: 400,
                    height: 512
                },
                size.calculate(QuickTerminalPosition::Center, dims_portrait)
            );
        }
        {
            // const size: QuickTerminalSize = .{ .primary = .{ .pixels = 600 } };
            let size = QuickTerminalSize {
                primary: Some(QuickTerminalSizeSize::Pixels(600)),
                secondary: None,
            };
            assert_eq!(
                QuickTerminalSizeDimensions {
                    width: 2560,
                    height: 600
                },
                size.calculate(QuickTerminalPosition::Top, dims_landscape)
            );
            assert_eq!(
                QuickTerminalSizeDimensions {
                    width: 600,
                    height: 1600
                },
                size.calculate(QuickTerminalPosition::Left, dims_landscape)
            );
            assert_eq!(
                QuickTerminalSizeDimensions {
                    width: 600,
                    height: 400
                },
                size.calculate(QuickTerminalPosition::Center, dims_landscape)
            );
            assert_eq!(
                QuickTerminalSizeDimensions {
                    width: 400,
                    height: 600
                },
                size.calculate(QuickTerminalPosition::Center, dims_portrait)
            );
        }
        {
            // const size: QuickTerminalSize = .{ .primary = .{ .percentage = 69 }, .secondary = .{ .pixels = 420 } };
            let size = QuickTerminalSize {
                primary: Some(QuickTerminalSizeSize::Percentage(69.0)),
                secondary: Some(QuickTerminalSizeSize::Pixels(420)),
            };
            assert_eq!(
                QuickTerminalSizeDimensions {
                    width: 420,
                    height: 1104
                },
                size.calculate(QuickTerminalPosition::Top, dims_landscape)
            );
            assert_eq!(
                QuickTerminalSizeDimensions {
                    width: 1766,
                    height: 420
                },
                size.calculate(QuickTerminalPosition::Left, dims_landscape)
            );
            assert_eq!(
                QuickTerminalSizeDimensions {
                    width: 1766,
                    height: 420
                },
                size.calculate(QuickTerminalPosition::Center, dims_landscape)
            );
            assert_eq!(
                QuickTerminalSizeDimensions {
                    width: 420,
                    height: 1766
                },
                size.calculate(QuickTerminalPosition::Center, dims_portrait)
            );
        }
    }

    // -----------------------------------------------------------------------
    // AlphaBlending
    // -----------------------------------------------------------------------
    // PORT NOTE: `AlphaBlending` has no `test {}` block in the reference; its
    // `isLinear` is exercised indirectly. A direct smoke test of the three arms
    // is added (Rust-only, not a reference assertion) to lock the mapping.
    #[test]
    fn alpha_blending_is_linear() {
        assert!(!AlphaBlending::Native.is_linear());
        assert!(AlphaBlending::Linear.is_linear());
        assert!(AlphaBlending::LinearCorrected.is_linear());
    }

    // -----------------------------------------------------------------------
    // BackgroundBlur
    // -----------------------------------------------------------------------

    // test "parse BackgroundBlur"
    #[test]
    fn parse_background_blur() {
        // var v: BackgroundBlur = undefined;
        // PORT NOTE (Chunk 3a): `parseCLI` is now the config-local free fn
        // `background_blur_parse_cli` (the type is hoisted to rustty-render-types); the
        // test calls it identically. Test stays in config (Config.zig provenance).
        let mut v = BackgroundBlur::False;

        // try v.parseCLI(null);
        background_blur_parse_cli(&mut v, None).unwrap();
        // try testing.expectEqual(.true, v);
        assert_eq!(BackgroundBlur::True, v);

        // try v.parseCLI("true");
        background_blur_parse_cli(&mut v, Some(b"true")).unwrap();
        assert_eq!(BackgroundBlur::True, v);

        // try v.parseCLI("false");
        background_blur_parse_cli(&mut v, Some(b"false")).unwrap();
        assert_eq!(BackgroundBlur::False, v);

        // try v.parseCLI("42");
        background_blur_parse_cli(&mut v, Some(b"42")).unwrap();
        // try testing.expectEqual(42, v.radius);
        assert_eq!(BackgroundBlur::Radius(42), v);

        // try v.parseCLI("macos-glass-regular");
        background_blur_parse_cli(&mut v, Some(b"macos-glass-regular")).unwrap();
        assert_eq!(BackgroundBlur::MacosGlassRegular, v);

        // try v.parseCLI("macos-glass-clear");
        background_blur_parse_cli(&mut v, Some(b"macos-glass-clear")).unwrap();
        assert_eq!(BackgroundBlur::MacosGlassClear, v);

        // try testing.expectError(error.InvalidValue, v.parseCLI(""));
        assert_eq!(
            Err(Error::InvalidValue),
            background_blur_parse_cli(&mut v, Some(b""))
        );
        // try testing.expectError(error.InvalidValue, v.parseCLI("aaaa"));
        assert_eq!(
            Err(Error::InvalidValue),
            background_blur_parse_cli(&mut v, Some(b"aaaa"))
        );
        // try testing.expectError(error.InvalidValue, v.parseCLI("420"));
        assert_eq!(
            Err(Error::InvalidValue),
            background_blur_parse_cli(&mut v, Some(b"420"))
        );
    }

    // -----------------------------------------------------------------------
    // WindowDecoration
    // -----------------------------------------------------------------------

    // test "parse WindowDecoration"
    #[test]
    fn parse_window_decoration() {
        // const v = try WindowDecoration.parseCLI(null);
        // try testing.expectEqual(WindowDecoration.auto, v);
        assert_eq!(
            WindowDecoration::Auto,
            WindowDecoration::parse_cli(None).unwrap()
        );
        // try testing.expectEqual(WindowDecoration.auto, parseCLI("true"));
        assert_eq!(
            WindowDecoration::Auto,
            WindowDecoration::parse_cli(Some(b"true")).unwrap()
        );
        // try testing.expectEqual(WindowDecoration.none, parseCLI("false"));
        assert_eq!(
            WindowDecoration::None,
            WindowDecoration::parse_cli(Some(b"false")).unwrap()
        );
        // try testing.expectEqual(WindowDecoration.server, parseCLI("server"));
        assert_eq!(
            WindowDecoration::Server,
            WindowDecoration::parse_cli(Some(b"server")).unwrap()
        );
        // try testing.expectEqual(WindowDecoration.client, parseCLI("client"));
        assert_eq!(
            WindowDecoration::Client,
            WindowDecoration::parse_cli(Some(b"client")).unwrap()
        );
        // try testing.expectEqual(WindowDecoration.auto, parseCLI("auto"));
        assert_eq!(
            WindowDecoration::Auto,
            WindowDecoration::parse_cli(Some(b"auto")).unwrap()
        );
        // try testing.expectEqual(WindowDecoration.none, parseCLI("none"));
        assert_eq!(
            WindowDecoration::None,
            WindowDecoration::parse_cli(Some(b"none")).unwrap()
        );

        // try testing.expectError(error.InvalidValue, WindowDecoration.parseCLI(""));
        assert_eq!(
            Err(Error::InvalidValue),
            WindowDecoration::parse_cli(Some(b""))
        );
        // try testing.expectError(error.InvalidValue, WindowDecoration.parseCLI("aaaa"));
        assert_eq!(
            Err(Error::InvalidValue),
            WindowDecoration::parse_cli(Some(b"aaaa"))
        );
    }

    // -----------------------------------------------------------------------
    // Theme
    // -----------------------------------------------------------------------

    // test "parse Theme"
    #[test]
    fn parse_theme() {
        // Single
        {
            // var v: Theme = undefined;
            let mut v = Theme::default();
            // try v.parseCLI(alloc, "foo");
            v.parse_cli(Some(b"foo")).unwrap();
            // try testing.expectEqualStrings("foo", v.light);
            assert_eq!(b"foo".as_slice(), v.light.as_slice());
            // try testing.expectEqualStrings("foo", v.dark);
            assert_eq!(b"foo".as_slice(), v.dark.as_slice());
        }

        // Single whitespace
        {
            let mut v = Theme::default();
            // try v.parseCLI(alloc, "  foo  ");
            v.parse_cli(Some(b"  foo  ")).unwrap();
            assert_eq!(b"foo".as_slice(), v.light.as_slice());
            assert_eq!(b"foo".as_slice(), v.dark.as_slice());
        }

        // Light/dark
        {
            let mut v = Theme::default();
            // try v.parseCLI(alloc, " light:foo,  dark : bar  ");
            v.parse_cli(Some(b" light:foo,  dark : bar  ")).unwrap();
            assert_eq!(b"foo".as_slice(), v.light.as_slice());
            assert_eq!(b"bar".as_slice(), v.dark.as_slice());
        }

        let mut v = Theme::default();
        // try testing.expectError(error.ValueRequired, v.parseCLI(alloc, null));
        assert_eq!(Err(Error::ValueRequired), v.parse_cli(None));
        // try testing.expectError(error.ValueRequired, v.parseCLI(alloc, ""));
        assert_eq!(Err(Error::ValueRequired), v.parse_cli(Some(b"")));
        // try testing.expectError(error.InvalidValue, v.parseCLI(alloc, "light:foo"));
        assert_eq!(Err(Error::InvalidValue), v.parse_cli(Some(b"light:foo")));
        // try testing.expectError(error.InvalidValue, v.parseCLI(alloc, "dark:foo"));
        assert_eq!(Err(Error::InvalidValue), v.parse_cli(Some(b"dark:foo")));
    }

    // -----------------------------------------------------------------------
    // Duration
    // -----------------------------------------------------------------------

    // test "parse duration"
    #[test]
    fn parse_duration() {
        // inline for (Duration.units) |unit| { const t = "0{s}"; const d = try Duration.parseCLI(t); expect 0 }
        for &(uname, _factor) in DURATION_UNITS {
            let mut t: Vec<u8> = Vec::new();
            t.push(b'0');
            t.extend_from_slice(uname);
            let d = Duration::parse_cli(Some(&t)).unwrap();
            assert_eq!(0u64, d.duration);
        }

        // inline for (Duration.units) |unit| { const t = "1{s}"; const d = try Duration.parseCLI(t); expect unit.factor }
        for &(uname, factor) in DURATION_UNITS {
            let mut t: Vec<u8> = Vec::new();
            t.push(b'1');
            t.extend_from_slice(uname);
            let d = Duration::parse_cli(Some(&t)).unwrap();
            assert_eq!(factor, d.duration);
        }

        // const d = try Duration.parseCLI("0"); expect 0
        assert_eq!(0u64, Duration::parse_cli(Some(b"0")).unwrap().duration);
        // const d = try Duration.parseCLI("100ns"); expect 100
        assert_eq!(
            100u64,
            Duration::parse_cli(Some(b"100ns")).unwrap().duration
        );
        // const d = try Duration.parseCLI("1µs"); expect 1000
        assert_eq!(
            1000u64,
            Duration::parse_cli(Some("1µs".as_bytes()))
                .unwrap()
                .duration
        );
        // const d = try Duration.parseCLI("1µs1ns"); expect 1001
        assert_eq!(
            1001u64,
            Duration::parse_cli(Some("1µs1ns".as_bytes()))
                .unwrap()
                .duration
        );
        // const d = try Duration.parseCLI("1µs 1ns"); expect 1001
        assert_eq!(
            1001u64,
            Duration::parse_cli(Some("1µs 1ns".as_bytes()))
                .unwrap()
                .duration
        );
        // const d = try Duration.parseCLI(" 1µs1ns"); expect 1001
        assert_eq!(
            1001u64,
            Duration::parse_cli(Some(" 1µs1ns".as_bytes()))
                .unwrap()
                .duration
        );
        // const d = try Duration.parseCLI("1µs1ns "); expect 1001
        assert_eq!(
            1001u64,
            Duration::parse_cli(Some("1µs1ns ".as_bytes()))
                .unwrap()
                .duration
        );
        // const d = try Duration.parseCLI("30s"); expect 30 * ns_per_s
        assert_eq!(
            30 * NS_PER_S,
            Duration::parse_cli(Some(b"30s")).unwrap().duration
        );
        // const d = try Duration.parseCLI("584y 49w 23h 34m 33s 709ms 551µs 615ns"); expect maxInt(u64)
        assert_eq!(
            u64::MAX,
            Duration::parse_cli(Some("584y 49w 23h 34m 33s 709ms 551µs 615ns".as_bytes()))
                .unwrap()
                .duration
        );

        // Overflow: const d = try Duration.parseCLI("600y"); expect maxInt(u64)
        assert_eq!(
            u64::MAX,
            Duration::parse_cli(Some(b"600y")).unwrap().duration
        );

        // Repeated units: const d = try Duration.parseCLI("100ns100ns"); expect 200
        assert_eq!(
            200u64,
            Duration::parse_cli(Some(b"100ns100ns")).unwrap().duration
        );

        // try expectError(error.ValueRequired, Duration.parseCLI(null));
        assert_eq!(Err(Error::ValueRequired), Duration::parse_cli(None));
        // try expectError(error.ValueRequired, Duration.parseCLI(""));
        assert_eq!(Err(Error::ValueRequired), Duration::parse_cli(Some(b"")));
        // try expectError(error.InvalidValue, Duration.parseCLI("1"));
        assert_eq!(Err(Error::InvalidValue), Duration::parse_cli(Some(b"1")));
        // try expectError(error.InvalidValue, Duration.parseCLI("s"));
        assert_eq!(Err(Error::InvalidValue), Duration::parse_cli(Some(b"s")));
        // try expectError(error.InvalidValue, Duration.parseCLI("1x"));
        assert_eq!(Err(Error::InvalidValue), Duration::parse_cli(Some(b"1x")));
        // try expectError(error.InvalidValue, Duration.parseCLI("1 "));
        assert_eq!(Err(Error::InvalidValue), Duration::parse_cli(Some(b"1 ")));
    }

    // test "test format" (Duration)
    #[test]
    fn duration_test_format() {
        // inline for (Duration.units) |unit| {
        //     const d: Duration = .{ .duration = unit.factor };
        //     const actual = "{f}".{d};
        //     const expected = if (!eql(unit.name, "us")) "1{s}".{unit.name} else "1µs";
        //     expectEqualSlices(expected, actual);
        // }
        for &(uname, factor) in DURATION_UNITS {
            let d = Duration { duration: factor };
            let mut actual: Vec<u8> = Vec::new();
            d.format(&mut actual);
            let expected: Vec<u8> = if uname != b"us" {
                let mut e: Vec<u8> = vec![b'1'];
                e.extend_from_slice(uname);
                e
            } else {
                "1µs".as_bytes().to_vec()
            };
            assert_eq!(expected, actual);
        }
    }

    // test "test entryFormatter" (Duration)
    #[test]
    fn duration_test_entry_formatter() {
        let mut buf = AllocatingWriter::init();
        // var p: Duration = .{ .duration = std.math.maxInt(u64) };
        let p = Duration { duration: u64::MAX };
        let mut f = entry_formatter(b"a", &mut buf);
        f.format_entry(&p).unwrap();
        // try std.testing.expectEqualStrings("a = 584y 49w 23h 34m 33s 709ms 551µs 615ns\n", buf.written());
        assert_eq!(
            "a = 584y 49w 23h 34m 33s 709ms 551µs 615ns\n".as_bytes(),
            buf.written()
        );
    }

    // -----------------------------------------------------------------------
    // WindowPadding
    // -----------------------------------------------------------------------

    // test "parse WindowPadding"
    #[test]
    fn parse_window_padding() {
        {
            // const v = try WindowPadding.parseCLI("100");
            let v = WindowPadding::parse_cli(Some(b"100")).unwrap();
            // try testing.expectEqual(WindowPadding{ .top_left = 100, .bottom_right = 100 }, v);
            assert_eq!(
                WindowPadding {
                    top_left: 100,
                    bottom_right: 100
                },
                v
            );
        }

        {
            // const v = try WindowPadding.parseCLI("100,200");
            let v = WindowPadding::parse_cli(Some(b"100,200")).unwrap();
            assert_eq!(
                WindowPadding {
                    top_left: 100,
                    bottom_right: 200
                },
                v
            );
        }

        // Trim whitespace
        {
            // const v = try WindowPadding.parseCLI(" 100 , 200 ");
            let v = WindowPadding::parse_cli(Some(b" 100 , 200 ")).unwrap();
            assert_eq!(
                WindowPadding {
                    top_left: 100,
                    bottom_right: 200
                },
                v
            );
        }

        // try testing.expectError(error.ValueRequired, WindowPadding.parseCLI(null));
        assert_eq!(Err(Error::ValueRequired), WindowPadding::parse_cli(None));
        // try testing.expectError(error.InvalidValue, WindowPadding.parseCLI(""));
        assert_eq!(
            Err(Error::InvalidValue),
            WindowPadding::parse_cli(Some(b""))
        );
        // try testing.expectError(error.InvalidValue, WindowPadding.parseCLI("a"));
        assert_eq!(
            Err(Error::InvalidValue),
            WindowPadding::parse_cli(Some(b"a"))
        );
    }

    // -----------------------------------------------------------------------
    // RepeatableCommand
    //
    // PORT NOTE: the reference threads an `ArenaAllocator` through `parseCLI`;
    // the Rust port owns its data (no allocator), so the arena setup/teardown
    // has no analog. `inputpkg.Binding.Action` -> `rustty_input::binding::Action`;
    // `inputpkg.command.defaults` -> `rustty_input::command::defaults()`.
    // -----------------------------------------------------------------------
    use rustty_input::binding::Action;

    // test "RepeatableCommand parseCLI"
    // PORT NOTE: `set_font_size:3.14` is the reference test's literal input; the
    // `3.14` expected value is the parsed result, not an approximation of PI — the
    // `approx_constant` lint is allowed for this faithful expected value.
    #[allow(clippy::approx_constant)]
    #[test]
    fn repeatable_command_parse_cli() {
        let mut list = RepeatableCommand::default();
        // try list.parseCLI(alloc, "title:Foo,action:ignore");
        list.parse_cli(Some(b"title:Foo,action:ignore")).unwrap();
        // try list.parseCLI(alloc, "title:Bar,description:bobr,action:text:ale bydle");
        list.parse_cli(Some(b"title:Bar,description:bobr,action:text:ale bydle"))
            .unwrap();
        // try list.parseCLI(alloc, "title:Quux,description:boo,action:increase_font_size:2.5");
        list.parse_cli(Some(
            b"title:Quux,description:boo,action:increase_font_size:2.5",
        ))
        .unwrap();
        // try list.parseCLI(alloc, "title:Baz,description:Raspberry Pie,action:set_font_size:3.14");
        list.parse_cli(Some(
            b"title:Baz,description:Raspberry Pie,action:set_font_size:3.14",
        ))
        .unwrap();

        // try testing.expectEqual(@as(usize, 4), list.value.items.len);
        assert_eq!(4usize, list.value.len());

        // try testing.expectEqual(inputpkg.Binding.Action.ignore, list.value.items[0].action);
        assert_eq!(Action::Ignore, list.value[0].action);
        // try testing.expectEqualStrings("Foo", list.value.items[0].title);
        assert_eq!(b"Foo".as_slice(), list.value[0].title.as_slice());

        // try testing.expect(list.value.items[1].action == .text);
        assert!(matches!(list.value[1].action, Action::Text(_)));
        // try testing.expectEqualStrings("ale bydle", list.value.items[1].action.text);
        assert_eq!(Action::Text(b"ale bydle".to_vec()), list.value[1].action);
        // try testing.expectEqualStrings("Bar", list.value.items[1].title);
        assert_eq!(b"Bar".as_slice(), list.value[1].title.as_slice());
        // try testing.expectEqualStrings("bobr", list.value.items[1].description);
        assert_eq!(b"bobr".as_slice(), list.value[1].description.as_slice());

        // try testing.expectEqual(inputpkg.Binding.Action{ .increase_font_size = 2.5 }, list.value.items[2].action);
        assert_eq!(Action::IncreaseFontSize(2.5), list.value[2].action);
        // try testing.expectEqualStrings("Quux", list.value.items[2].title);
        assert_eq!(b"Quux".as_slice(), list.value[2].title.as_slice());
        // try testing.expectEqualStrings("boo", list.value.items[2].description);
        assert_eq!(b"boo".as_slice(), list.value[2].description.as_slice());

        // try testing.expectEqual(inputpkg.Binding.Action{ .set_font_size = 3.14 }, list.value.items[3].action);
        assert_eq!(Action::SetFontSize(3.14), list.value[3].action);
        // try testing.expectEqualStrings("Baz", list.value.items[3].title);
        assert_eq!(b"Baz".as_slice(), list.value[3].title.as_slice());
        // try testing.expectEqualStrings("Raspberry Pie", list.value.items[3].description);
        assert_eq!(
            b"Raspberry Pie".as_slice(),
            list.value[3].description.as_slice()
        );

        // try list.parseCLI(alloc, "clear");
        list.parse_cli(Some(b"clear")).unwrap();
        // try testing.expectEqual(@as(usize, 0), list.value.items.len);
        assert_eq!(0usize, list.value.len());

        // try list.parseCLI(alloc, "");
        list.parse_cli(Some(b"")).unwrap();
        // try testing.expectEqual(inputpkg.command.defaults.len, list.value.items.len);
        assert_eq!(rustty_input::command::defaults().len(), list.value.len());
    }

    // test "RepeatableCommand formatConfig empty"
    #[test]
    fn repeatable_command_format_config_empty() {
        let mut buf = AllocatingWriter::init();
        let list = RepeatableCommand::default();
        // try list.formatEntry(formatterpkg.entryFormatter("a", &buf.writer));
        list.format_entry(b"a", &mut buf).unwrap();
        // try std.testing.expectEqualSlices(u8, "a = \n", buf.written());
        assert_eq!(b"a = \n".as_slice(), buf.written());
    }

    // test "RepeatableCommand formatConfig single item"
    #[test]
    fn repeatable_command_format_config_single_item() {
        let mut buf = AllocatingWriter::init();
        let mut list = RepeatableCommand::default();
        // try list.parseCLI(alloc, "title:Bobr, action:text:Bober");
        list.parse_cli(Some(b"title:Bobr, action:text:Bober"))
            .unwrap();
        list.format_entry(b"a", &mut buf).unwrap();
        // try std.testing.expectEqualSlices(u8, "a = title:\"Bobr\",action:\"text:Bober\"\n", buf.written());
        assert_eq!(
            b"a = title:\"Bobr\",action:\"text:Bober\"\n".as_slice(),
            buf.written()
        );
    }

    // test "RepeatableCommand formatConfig multiple items"
    #[test]
    fn repeatable_command_format_config_multiple_items() {
        let mut buf = AllocatingWriter::init();
        let mut list = RepeatableCommand::default();
        // try list.parseCLI(alloc, "title:Bobr, action:text:kurwa");
        list.parse_cli(Some(b"title:Bobr, action:text:kurwa"))
            .unwrap();
        // try list.parseCLI(alloc, "title:Ja,   description: pierdole,  action:text:jakie bydle");
        list.parse_cli(Some(
            b"title:Ja,   description: pierdole,  action:text:jakie bydle",
        ))
        .unwrap();
        list.format_entry(b"a", &mut buf).unwrap();
        // try std.testing.expectEqualSlices(u8, "a = title:\"Bobr\",action:\"text:kurwa\"\na = title:\"Ja\",description:\"pierdole\",action:\"text:jakie bydle\"\n", buf.written());
        assert_eq!(
            b"a = title:\"Bobr\",action:\"text:kurwa\"\na = title:\"Ja\",description:\"pierdole\",action:\"text:jakie bydle\"\n".as_slice(),
            buf.written()
        );
    }

    // test "RepeatableCommand parseCLI commas"
    #[test]
    fn repeatable_command_parse_cli_commas() {
        {
            let mut list = RepeatableCommand::default();
            // try list.parseCLI(alloc, "title:\"Bo,br\",action:\"text:kur,wa\"");
            list.parse_cli(Some(b"title:\"Bo,br\",action:\"text:kur,wa\""))
                .unwrap();
            // try testing.expectEqual(@as(usize, 1), list.value.items.len);
            assert_eq!(1usize, list.value.len());

            // const item = list.value.items[0];
            let item = &list.value[0];
            // try testing.expectEqualStrings("Bo,br", item.title);
            assert_eq!(b"Bo,br".as_slice(), item.title.as_slice());
            // try testing.expectEqualStrings("", item.description);
            assert_eq!(b"".as_slice(), item.description.as_slice());
            // try testing.expect(item.action == .text);
            assert!(matches!(item.action, Action::Text(_)));
            // try testing.expectEqualStrings("kur,wa", item.action.text);
            assert_eq!(Action::Text(b"kur,wa".to_vec()), item.action);
        }
        {
            let mut list = RepeatableCommand::default();
            // try list.parseCLI(alloc, "title:\"Bo,br\",description:\"abc,def\",action:text:kurwa");
            list.parse_cli(Some(
                b"title:\"Bo,br\",description:\"abc,def\",action:text:kurwa",
            ))
            .unwrap();
            // try testing.expectEqual(@as(usize, 1), list.value.items.len);
            assert_eq!(1usize, list.value.len());

            // const item = list.value.items[0];
            let item = &list.value[0];
            // try testing.expectEqualStrings("Bo,br", item.title);
            assert_eq!(b"Bo,br".as_slice(), item.title.as_slice());
            // try testing.expectEqualStrings("abc,def", item.description);
            assert_eq!(b"abc,def".as_slice(), item.description.as_slice());
            // try testing.expect(item.action == .text);
            assert!(matches!(item.action, Action::Text(_)));
            // try testing.expectEqualStrings("kurwa", item.action.text);
            assert_eq!(Action::Text(b"kurwa".to_vec()), item.action);
        }
    }

    // test "RepeatableCommand cval"
    #[test]
    fn repeatable_command_cval() {
        let mut list = RepeatableCommand::default();
        // try list.parseCLI(alloc, "title:Foo,action:ignore");
        list.parse_cli(Some(b"title:Foo,action:ignore")).unwrap();
        // try list.parseCLI(alloc, "title:Bar,description:bobr,action:text:ale bydle");
        list.parse_cli(Some(b"title:Bar,description:bobr,action:text:ale bydle"))
            .unwrap();

        // try testing.expectEqual(@as(usize, 2), list.value.items.len);
        assert_eq!(2usize, list.value.len());
        // try testing.expectEqual(@as(usize, 2), list.value_c.items.len);
        assert_eq!(2usize, list.value_c.len());

        // const cv = list.cval();
        let cv = list.cval();
        // try testing.expectEqual(@as(usize, 2), cv.len);
        assert_eq!(2usize, cv.len);

        // PORT NOTE: `cv.commands` is `*const Command.C`; view the `len` entries as
        // a slice. `std.mem.sliceTo(_, 0)` strips the NUL terminator the `C` fields
        // carry — `cstr_to_slice` does the same.
        let commands: &[rustty_input::command::C] =
            unsafe { std::slice::from_raw_parts(cv.commands, cv.len) };

        // First entry
        // try testing.expectEqualStrings("Foo", std.mem.sliceTo(cv.commands[0].title, 0));
        assert_eq!(b"Foo".as_slice(), slice_to_nul(&commands[0].title));
        // try testing.expectEqualStrings("ignore", std.mem.sliceTo(cv.commands[0].action_key, 0));
        assert_eq!(b"ignore".as_slice(), slice_to_nul(&commands[0].action_key));
        // try testing.expectEqualStrings("ignore", std.mem.sliceTo(cv.commands[0].action, 0));
        assert_eq!(b"ignore".as_slice(), slice_to_nul(&commands[0].action));

        // Second entry
        // try testing.expectEqualStrings("Bar", std.mem.sliceTo(cv.commands[1].title, 0));
        assert_eq!(b"Bar".as_slice(), slice_to_nul(&commands[1].title));
        // try testing.expectEqualStrings("bobr", std.mem.sliceTo(cv.commands[1].description, 0));
        assert_eq!(b"bobr".as_slice(), slice_to_nul(&commands[1].description));
        // try testing.expectEqualStrings("text", std.mem.sliceTo(cv.commands[1].action_key, 0));
        assert_eq!(b"text".as_slice(), slice_to_nul(&commands[1].action_key));
        // try testing.expectEqualStrings("text:ale bydle", std.mem.sliceTo(cv.commands[1].action, 0));
        assert_eq!(
            b"text:ale bydle".as_slice(),
            slice_to_nul(&commands[1].action)
        );
    }

    // test "RepeatableCommand cval cleared"
    #[test]
    fn repeatable_command_cval_cleared() {
        let mut list = RepeatableCommand::default();
        // try list.parseCLI(alloc, "title:Foo,action:ignore");
        list.parse_cli(Some(b"title:Foo,action:ignore")).unwrap();
        // try testing.expectEqual(@as(usize, 1), list.cval().len);
        assert_eq!(1usize, list.cval().len);

        // try list.parseCLI(alloc, "clear");
        list.parse_cli(Some(b"clear")).unwrap();
        // try testing.expectEqual(@as(usize, 0), list.cval().len);
        assert_eq!(0usize, list.cval().len);
    }

    // PORT NOTE: `std.mem.sliceTo(buf, 0)` — the bytes up to (excluding) the first
    // NUL terminator the `Command.C` fields carry.
    fn slice_to_nul(buf: &[u8]) -> &[u8] {
        match buf.iter().position(|&b| b == 0) {
            Some(i) => &buf[..i],
            None => buf,
        }
    }

    // -----------------------------------------------------------------------
    // Keybinds (Config.zig:6415)
    //
    // PORT NOTE: the reference tests use an `ArenaAllocator` threaded through
    // `parseCLI`/`clone`. The Rust APIs are allocator-free (owned values), so the
    // arena setup/teardown collapses to nothing; the assertions are reproduced
    // verbatim. `formatterpkg.entryFormatter("a", &buf.writer)` -> `entry_formatter`
    // over an `AllocatingWriter`; `buf.written()` -> `buf.written()`.
    // -----------------------------------------------------------------------

    // test "parseCLI"
    #[test]
    fn keybinds_parse_cli() {
        let mut set: Keybinds = Keybinds::default();
        // try set.parseCLI(alloc, "shift+a=copy_to_clipboard");
        set.parse_cli(Some(b"shift+a=copy_to_clipboard")).unwrap();
        // try set.parseCLI(alloc, "shift+a=csi:hello");
        set.parse_cli(Some(b"shift+a=csi:hello")).unwrap();
    }

    // test "formatConfig single"
    #[test]
    fn keybinds_format_config_single() {
        let mut buf = AllocatingWriter::init();

        let mut list: Keybinds = Keybinds::default();
        // try list.parseCLI(alloc, "shift+a=csi:hello");
        list.parse_cli(Some(b"shift+a=csi:hello")).unwrap();
        // try list.formatEntry(formatterpkg.entryFormatter("a", &buf.writer));
        let mut f = entry_formatter(b"a", &mut buf);
        list.format_entry(&mut f).unwrap();
        // try std.testing.expectEqualSlices(u8, "a = shift+a=csi:hello\n", buf.written());
        assert_eq!(b"a = shift+a=csi:hello\n".as_slice(), buf.written());
    }

    // Regression test for https://github.com/ghostty-org/ghostty/issues/2734
    // test "formatConfig multiple items"
    #[test]
    fn keybinds_format_config_multiple_items() {
        let mut buf = AllocatingWriter::init();

        let mut list: Keybinds = Keybinds::default();
        // try list.parseCLI(alloc, "ctrl+z>1=goto_tab:1");
        list.parse_cli(Some(b"ctrl+z>1=goto_tab:1")).unwrap();
        // try list.parseCLI(alloc, "ctrl+z>2=goto_tab:2");
        list.parse_cli(Some(b"ctrl+z>2=goto_tab:2")).unwrap();
        // try list.formatEntry(formatterpkg.entryFormatter("keybind", &buf.writer));
        let mut f = entry_formatter(b"keybind", &mut buf);
        list.format_entry(&mut f).unwrap();

        // Note they turn into translated keys because they match
        // their ASCII mapping.
        let want = concat!(
            "keybind = ctrl+z>1=goto_tab:1\n",
            "keybind = ctrl+z>2=goto_tab:2\n",
        );
        // try std.testing.expectEqualStrings(want, buf.written());
        assert_eq!(want.as_bytes(), buf.written());
    }

    // test "formatConfig multiple items nested"
    #[test]
    fn keybinds_format_config_multiple_items_nested() {
        let mut buf = AllocatingWriter::init();

        let mut list: Keybinds = Keybinds::default();
        // try list.parseCLI(alloc, "ctrl+a>ctrl+b>n=new_window");
        list.parse_cli(Some(b"ctrl+a>ctrl+b>n=new_window")).unwrap();
        // try list.parseCLI(alloc, "ctrl+a>ctrl+b>w=close_window");
        list.parse_cli(Some(b"ctrl+a>ctrl+b>w=close_window"))
            .unwrap();
        // try list.parseCLI(alloc, "ctrl+a>ctrl+c>t=new_tab");
        list.parse_cli(Some(b"ctrl+a>ctrl+c>t=new_tab")).unwrap();
        // try list.parseCLI(alloc, "ctrl+b>ctrl+d>a=previous_tab");
        list.parse_cli(Some(b"ctrl+b>ctrl+d>a=previous_tab"))
            .unwrap();
        // try list.formatEntry(formatterpkg.entryFormatter("a", &buf.writer));
        let mut f = entry_formatter(b"a", &mut buf);
        list.format_entry(&mut f).unwrap();

        // NB: This does not currently retain the order of the keybinds.
        let want = concat!(
            "a = ctrl+a>ctrl+b>n=new_window\n",
            "a = ctrl+a>ctrl+b>w=close_window\n",
            "a = ctrl+a>ctrl+c>t=new_tab\n",
            "a = ctrl+b>ctrl+d>a=previous_tab\n",
        );
        // try std.testing.expectEqualStrings(want, buf.written());
        assert_eq!(want.as_bytes(), buf.written());
    }

    // test "parseCLI table definition"
    #[test]
    fn keybinds_parse_cli_table_definition() {
        let mut keybinds: Keybinds = Keybinds::default();

        // Define a table by adding a binding to it
        // try keybinds.parseCLI(alloc, "foo/shift+a=copy_to_clipboard");
        keybinds
            .parse_cli(Some(b"foo/shift+a=copy_to_clipboard"))
            .unwrap();
        // try testing.expectEqual(1, keybinds.tables.count());
        assert_eq!(1, keybinds.tables.count());
        // try testing.expect(keybinds.tables.contains("foo"));
        assert!(keybinds.tables.contains(&b"foo".to_vec()));

        // const table = keybinds.tables.get("foo").?;
        let table = keybinds.tables.get(&b"foo".to_vec()).unwrap();
        // try testing.expectEqual(1, table.bindings.count());
        assert_eq!(1, table.bindings_count());
    }

    // test "parseCLI table clear"
    #[test]
    fn keybinds_parse_cli_table_clear() {
        let mut keybinds: Keybinds = Keybinds::default();

        // Add a binding to a table
        keybinds
            .parse_cli(Some(b"foo/shift+a=copy_to_clipboard"))
            .unwrap();
        // try testing.expectEqual(1, keybinds.tables.get("foo").?.bindings.count());
        assert_eq!(
            1,
            keybinds
                .tables
                .get(&b"foo".to_vec())
                .unwrap()
                .bindings_count()
        );

        // Clear the table with "foo/"
        keybinds.parse_cli(Some(b"foo/")).unwrap();
        // try testing.expectEqual(0, keybinds.tables.get("foo").?.bindings.count());
        assert_eq!(
            0,
            keybinds
                .tables
                .get(&b"foo".to_vec())
                .unwrap()
                .bindings_count()
        );
    }

    // test "parseCLI table multiple bindings"
    #[test]
    fn keybinds_parse_cli_table_multiple_bindings() {
        let mut keybinds: Keybinds = Keybinds::default();

        keybinds
            .parse_cli(Some(b"foo/shift+a=copy_to_clipboard"))
            .unwrap();
        keybinds
            .parse_cli(Some(b"foo/shift+b=paste_from_clipboard"))
            .unwrap();
        keybinds
            .parse_cli(Some(b"bar/ctrl+c=close_window"))
            .unwrap();

        // try testing.expectEqual(2, keybinds.tables.count());
        assert_eq!(2, keybinds.tables.count());
        // try testing.expectEqual(2, keybinds.tables.get("foo").?.bindings.count());
        assert_eq!(
            2,
            keybinds
                .tables
                .get(&b"foo".to_vec())
                .unwrap()
                .bindings_count()
        );
        // try testing.expectEqual(1, keybinds.tables.get("bar").?.bindings.count());
        assert_eq!(
            1,
            keybinds
                .tables
                .get(&b"bar".to_vec())
                .unwrap()
                .bindings_count()
        );
    }

    // test "parseCLI table does not affect root set"
    #[test]
    fn keybinds_parse_cli_table_does_not_affect_root_set() {
        let mut keybinds: Keybinds = Keybinds::default();

        keybinds
            .parse_cli(Some(b"shift+a=copy_to_clipboard"))
            .unwrap();
        keybinds
            .parse_cli(Some(b"foo/shift+b=paste_from_clipboard"))
            .unwrap();

        // Root set should have the first binding
        // try testing.expectEqual(1, keybinds.set.bindings.count());
        assert_eq!(1, keybinds.set.bindings_count());
        // Table should have the second binding
        // try testing.expectEqual(1, keybinds.tables.get("foo").?.bindings.count());
        assert_eq!(
            1,
            keybinds
                .tables
                .get(&b"foo".to_vec())
                .unwrap()
                .bindings_count()
        );
    }

    // test "parseCLI table empty name is invalid"
    #[test]
    fn keybinds_parse_cli_table_empty_name_is_invalid() {
        let mut keybinds: Keybinds = Keybinds::default();
        // try testing.expectError(error.InvalidFormat, keybinds.parseCLI(alloc, "/shift+a=copy_to_clipboard"));
        assert_eq!(
            Err(KeybindsParseError::InvalidFormat),
            keybinds.parse_cli(Some(b"/shift+a=copy_to_clipboard")),
        );
    }

    // test "parseCLI table with key sequence"
    #[test]
    fn keybinds_parse_cli_table_with_key_sequence() {
        let mut keybinds: Keybinds = Keybinds::default();

        // Key sequences should work within tables
        keybinds
            .parse_cli(Some(b"foo/ctrl+a>ctrl+b=new_window"))
            .unwrap();

        // const table = keybinds.tables.get("foo").?;
        let table = keybinds.tables.get(&b"foo".to_vec()).unwrap();
        // try testing.expectEqual(1, table.bindings.count());
        assert_eq!(1, table.bindings_count());
    }

    // test "parseCLI slash in action argument is not a table"
    #[test]
    fn keybinds_parse_cli_slash_in_action_argument_is_not_a_table() {
        let mut keybinds: Keybinds = Keybinds::default();

        // A slash after the = should not be interpreted as a table delimiter
        keybinds.parse_cli(Some(b"ctrl+a=text:/hello")).unwrap();

        // Should be in root set, not a table
        assert_eq!(1, keybinds.set.bindings_count());
        assert_eq!(0, keybinds.tables.count());
    }

    // test "parseCLI slash as key with modifier is not a table"
    #[test]
    fn keybinds_parse_cli_slash_as_key_with_modifier_is_not_a_table() {
        let mut keybinds: Keybinds = Keybinds::default();

        // ctrl+/ should be parsed as a keybind with '/' as the key, not a table
        keybinds.parse_cli(Some(b"ctrl+/=text:foo")).unwrap();

        // Should be in root set, not a table
        assert_eq!(1, keybinds.set.bindings_count());
        assert_eq!(0, keybinds.tables.count());
    }

    // test "parseCLI shift+slash as key is not a table"
    #[test]
    fn keybinds_parse_cli_shift_slash_as_key_is_not_a_table() {
        let mut keybinds: Keybinds = Keybinds::default();

        // shift+/ should be parsed as a keybind, not a table
        keybinds.parse_cli(Some(b"shift+/=ignore")).unwrap();

        // Should be in root set, not a table
        assert_eq!(1, keybinds.set.bindings_count());
        assert_eq!(0, keybinds.tables.count());
    }

    // test "parseCLI bare slash as key is not a table"
    #[test]
    fn keybinds_parse_cli_bare_slash_as_key_is_not_a_table() {
        let mut keybinds: Keybinds = Keybinds::default();

        // Bare / as a key should work (empty table name is rejected)
        keybinds.parse_cli(Some(b"/=text:foo")).unwrap();

        // Should be in root set, not a table
        assert_eq!(1, keybinds.set.bindings_count());
        assert_eq!(0, keybinds.tables.count());
    }

    // test "parseCLI slash in key sequence is not a table"
    #[test]
    fn keybinds_parse_cli_slash_in_key_sequence_is_not_a_table() {
        let mut keybinds: Keybinds = Keybinds::default();

        // Key sequence ending with / should work
        keybinds
            .parse_cli(Some(b"ctrl+a>ctrl+/=new_window"))
            .unwrap();

        // Should be in root set, not a table
        assert_eq!(1, keybinds.set.bindings_count());
        assert_eq!(0, keybinds.tables.count());
    }

    // test "parseCLI table with slash in binding"
    #[test]
    fn keybinds_parse_cli_table_with_slash_in_binding() {
        let mut keybinds: Keybinds = Keybinds::default();

        // Table with a binding that uses / as the key
        keybinds.parse_cli(Some(b"mytable//=text:foo")).unwrap();

        // Should be in the table
        assert_eq!(0, keybinds.set.bindings_count());
        assert_eq!(1, keybinds.tables.count());
        assert!(keybinds.tables.contains(&b"mytable".to_vec()));
        assert_eq!(
            1,
            keybinds
                .tables
                .get(&b"mytable".to_vec())
                .unwrap()
                .bindings_count()
        );
    }

    // test "parseCLI table with sequence containing slash"
    #[test]
    fn keybinds_parse_cli_table_with_sequence_containing_slash() {
        let mut keybinds: Keybinds = Keybinds::default();

        // Table with a key sequence that ends with /
        keybinds.parse_cli(Some(b"mytable/a>/=new_window")).unwrap();

        // Should be in the table
        assert_eq!(0, keybinds.set.bindings_count());
        assert_eq!(1, keybinds.tables.count());
        assert!(keybinds.tables.contains(&b"mytable".to_vec()));
    }

    // test "parseCLI chain without prior parsed binding is invalid"
    #[test]
    fn keybinds_parse_cli_chain_without_prior_parsed_binding_is_invalid() {
        let mut keybinds: Keybinds = Keybinds::default();

        assert_eq!(
            Err(KeybindsParseError::InvalidFormat),
            keybinds.parse_cli(Some(b"chain=new_tab")),
        );
    }

    // test "parseCLI table chain syntax is invalid"
    #[test]
    fn keybinds_parse_cli_table_chain_syntax_is_invalid() {
        let mut keybinds: Keybinds = Keybinds::default();

        keybinds.parse_cli(Some(b"foo/a=text:hello")).unwrap();
        assert_eq!(
            Err(KeybindsParseError::InvalidFormat),
            keybinds.parse_cli(Some(b"foo/chain=deactivate_key_table")),
        );
    }

    // test "parseCLI chain applies to most recent table binding"
    #[test]
    fn keybinds_parse_cli_chain_applies_to_most_recent_table_binding() {
        use rustty_input::binding::{Action, Trigger, TriggerKey, Value};
        use rustty_vt::input::key_mods::Mods;

        let mut keybinds: Keybinds = Keybinds::default();

        keybinds
            .parse_cli(Some(b"ctrl+n=activate_key_table:foo"))
            .unwrap();
        keybinds.parse_cli(Some(b"foo/a=text:hello")).unwrap();
        keybinds
            .parse_cli(Some(b"chain=deactivate_key_table"))
            .unwrap();

        // const root_entry = keybinds.set.get(.{ .mods = .{ .ctrl = true },
        //     .key = .{ .unicode = 'n' } }).?.value_ptr.*;
        let root_entry = keybinds
            .set
            .get_value(Trigger {
                mods: Mods {
                    ctrl: true,
                    ..Mods::default()
                },
                key: TriggerKey::Unicode('n' as u32),
            })
            .unwrap();
        // try testing.expect(root_entry == .leaf);
        // try testing.expect(root_entry.leaf.action == .activate_key_table);
        match root_entry {
            Value::Leaf(leaf) => {
                assert!(matches!(leaf.action, Action::ActivateKeyTable(_)));
            }
            _ => panic!("expected leaf"),
        }

        // const foo_entry = keybinds.tables.get("foo").?.get(.{
        //     .key = .{ .unicode = 'a' } }).?.value_ptr.*;
        let foo_entry = keybinds
            .tables
            .get(&b"foo".to_vec())
            .unwrap()
            .get_value(Trigger {
                key: TriggerKey::Unicode('a' as u32),
                ..Trigger::default()
            })
            .unwrap();
        // try testing.expect(foo_entry == .leaf_chained);
        match foo_entry {
            Value::LeafChained(chain) => {
                // try testing.expectEqual(@as(usize, 2), foo_entry.leaf_chained.actions.items.len);
                assert_eq!(2usize, chain.actions.len());
                // try testing.expect(foo_entry.leaf_chained.actions.items[0] == .text);
                assert!(matches!(chain.actions[0], Action::Text(_)));
                // try testing.expect(foo_entry.leaf_chained.actions.items[1] == .deactivate_key_table);
                assert!(matches!(chain.actions[1], Action::DeactivateKeyTable));
            }
            _ => panic!("expected leaf_chained"),
        }
    }

    // test "clone with tables"
    #[test]
    fn keybinds_clone_with_tables() {
        let mut keybinds: Keybinds = Keybinds::default();
        keybinds
            .parse_cli(Some(b"shift+a=copy_to_clipboard"))
            .unwrap();
        keybinds
            .parse_cli(Some(b"foo/shift+b=paste_from_clipboard"))
            .unwrap();
        keybinds
            .parse_cli(Some(b"bar/ctrl+c=close_window"))
            .unwrap();

        // const cloned = try keybinds.clone(alloc);
        let cloned = keybinds.clone_in();

        // Verify the clone has the same structure
        // try testing.expectEqual(keybinds.set.bindings.count(), cloned.set.bindings.count());
        assert_eq!(keybinds.set.bindings_count(), cloned.set.bindings_count());
        // try testing.expectEqual(keybinds.tables.count(), cloned.tables.count());
        assert_eq!(keybinds.tables.count(), cloned.tables.count());
        // try testing.expectEqual(keybinds.tables.get("foo").?.bindings.count(),
        //     cloned.tables.get("foo").?.bindings.count());
        assert_eq!(
            keybinds
                .tables
                .get(&b"foo".to_vec())
                .unwrap()
                .bindings_count(),
            cloned
                .tables
                .get(&b"foo".to_vec())
                .unwrap()
                .bindings_count(),
        );
        // try testing.expectEqual(keybinds.tables.get("bar").?.bindings.count(),
        //     cloned.tables.get("bar").?.bindings.count());
        assert_eq!(
            keybinds
                .tables
                .get(&b"bar".to_vec())
                .unwrap()
                .bindings_count(),
            cloned
                .tables
                .get(&b"bar".to_vec())
                .unwrap()
                .bindings_count(),
        );
    }

    // test "equal with tables"
    #[test]
    fn keybinds_equal_with_tables() {
        let mut keybinds1: Keybinds = Keybinds::default();
        keybinds1
            .parse_cli(Some(b"foo/shift+a=copy_to_clipboard"))
            .unwrap();

        let mut keybinds2: Keybinds = Keybinds::default();
        keybinds2
            .parse_cli(Some(b"foo/shift+a=copy_to_clipboard"))
            .unwrap();

        // try testing.expect(keybinds1.equal(keybinds2));
        assert!(keybinds1.equal(&keybinds2));
    }

    // test "equal with tables different table count"
    #[test]
    fn keybinds_equal_with_tables_different_table_count() {
        let mut keybinds1: Keybinds = Keybinds::default();
        keybinds1
            .parse_cli(Some(b"foo/shift+a=copy_to_clipboard"))
            .unwrap();

        let mut keybinds2: Keybinds = Keybinds::default();
        keybinds2
            .parse_cli(Some(b"foo/shift+a=copy_to_clipboard"))
            .unwrap();
        keybinds2
            .parse_cli(Some(b"bar/shift+b=paste_from_clipboard"))
            .unwrap();

        // try testing.expect(!keybinds1.equal(keybinds2));
        assert!(!keybinds1.equal(&keybinds2));
    }

    // test "equal with tables different table names"
    #[test]
    fn keybinds_equal_with_tables_different_table_names() {
        let mut keybinds1: Keybinds = Keybinds::default();
        keybinds1
            .parse_cli(Some(b"foo/shift+a=copy_to_clipboard"))
            .unwrap();

        let mut keybinds2: Keybinds = Keybinds::default();
        keybinds2
            .parse_cli(Some(b"bar/shift+a=copy_to_clipboard"))
            .unwrap();

        // try testing.expect(!keybinds1.equal(keybinds2));
        assert!(!keybinds1.equal(&keybinds2));
    }

    // test "equal with tables different bindings"
    #[test]
    fn keybinds_equal_with_tables_different_bindings() {
        let mut keybinds1: Keybinds = Keybinds::default();
        keybinds1
            .parse_cli(Some(b"foo/shift+a=copy_to_clipboard"))
            .unwrap();

        let mut keybinds2: Keybinds = Keybinds::default();
        keybinds2
            .parse_cli(Some(b"foo/shift+b=paste_from_clipboard"))
            .unwrap();

        // try testing.expect(!keybinds1.equal(keybinds2));
        assert!(!keybinds1.equal(&keybinds2));
    }

    // test "formatEntry with tables"
    #[test]
    fn keybinds_format_entry_with_tables() {
        let mut buf = AllocatingWriter::init();

        let mut keybinds: Keybinds = Keybinds::default();
        keybinds.parse_cli(Some(b"foo/shift+a=csi:hello")).unwrap();
        let mut f = entry_formatter(b"keybind", &mut buf);
        keybinds.format_entry(&mut f).unwrap();

        // try testing.expectEqualStrings("keybind = foo/shift+a=csi:hello\n", buf.written());
        assert_eq!(
            b"keybind = foo/shift+a=csi:hello\n".as_slice(),
            buf.written()
        );
    }

    // test "formatEntry with tables and root set"
    #[test]
    fn keybinds_format_entry_with_tables_and_root_set() {
        let mut buf = AllocatingWriter::init();

        let mut keybinds: Keybinds = Keybinds::default();
        keybinds.parse_cli(Some(b"shift+b=csi:world")).unwrap();
        keybinds.parse_cli(Some(b"foo/shift+a=csi:hello")).unwrap();
        let mut f = entry_formatter(b"keybind", &mut buf);
        keybinds.format_entry(&mut f).unwrap();

        // const output = buf.written();
        let output = buf.written();
        // try testing.expect(std.mem.indexOf(u8, output, "keybind = shift+b=csi:world\n") != null);
        assert!(find_sub(output, b"keybind = shift+b=csi:world\n").is_some());
        // try testing.expect(std.mem.indexOf(u8, output, "keybind = foo/shift+a=csi:hello\n") != null);
        assert!(find_sub(output, b"keybind = foo/shift+a=csi:hello\n").is_some());
    }

    // test "parseCLI clear clears tables"
    #[test]
    fn keybinds_parse_cli_clear_clears_tables() {
        let mut keybinds: Keybinds = Keybinds::default();

        // Add bindings to root set and tables
        keybinds
            .parse_cli(Some(b"shift+a=copy_to_clipboard"))
            .unwrap();
        keybinds
            .parse_cli(Some(b"foo/shift+b=paste_from_clipboard"))
            .unwrap();
        keybinds
            .parse_cli(Some(b"bar/ctrl+c=close_window"))
            .unwrap();

        assert_eq!(1, keybinds.set.bindings_count());
        assert_eq!(2, keybinds.tables.count());

        // Clear all keybinds
        keybinds.parse_cli(Some(b"clear")).unwrap();

        // Both root set and tables should be cleared
        assert_eq!(0, keybinds.set.bindings_count());
        assert_eq!(0, keybinds.tables.count());
    }

    // test "parseCLI reset clears tables"
    #[test]
    fn keybinds_parse_cli_reset_clears_tables() {
        let mut keybinds: Keybinds = Keybinds::default();

        // Add bindings to tables
        keybinds
            .parse_cli(Some(b"foo/shift+a=copy_to_clipboard"))
            .unwrap();
        keybinds
            .parse_cli(Some(b"bar/shift+b=paste_from_clipboard"))
            .unwrap();

        assert_eq!(2, keybinds.tables.count());

        // Reset to defaults (empty value)
        keybinds.parse_cli(Some(b"")).unwrap();

        // Tables should be cleared, root set has defaults
        assert_eq!(0, keybinds.tables.count());
        // try testing.expect(keybinds.set.bindings.count() > 0);
        assert!(keybinds.set.bindings_count() > 0);
    }

    // PORT NOTE: `std.mem.indexOf(u8, haystack, needle)` — first index of `needle`
    // in `haystack` (or `null`).
    fn find_sub(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        haystack.windows(needle.len()).position(|w| w == needle)
    }
}
