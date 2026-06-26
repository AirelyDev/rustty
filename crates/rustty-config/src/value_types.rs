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

use rustty_args::args::{parse_auto_struct, parse_bool, AutoStruct, Error, ParseError, WHITESPACE};
use rustty_cfg_fmt::{
    format_entry_display, format_entry_string, format_entry_void, FormatEntry, Writer,
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

// PORT NOTE: `fontpkg.CodepointMap` is not yet ported in rustty-font. Skeleton
// placeholder for `font/CodepointMap.zig`. Wired when CodepointMap.zig lands.
#[derive(Debug, Clone, Default)]
pub struct CodepointMap;

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
/// If this is changed, you must also update ghostty.h
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
/// If this is changed, you must also update ghostty.h
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
/// If this is changed, you must also update ghostty.h
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowPaddingColor {
    Background,
    Extend,
    ExtendAlways,
}

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
    Ghostty,
}

// pub const WindowColorspace = enum { srgb, "display-p3" };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowColorspace {
    Srgb,
    DisplayP3,
}

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

// pub const QuickTerminalSpaceBehavior = enum { remain, move };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuickTerminalSpaceBehavior {
    Remain,
    Move,
}

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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlphaBlending {
    Native,
    Linear,
    LinearCorrected,
}

impl AlphaBlending {
    // pub fn isLinear(self: AlphaBlending) bool
    pub fn is_linear(self: AlphaBlending) -> bool {
        // return switch (self) {
        //     .native => false,
        //     .linear, .@"linear-corrected" => true,
        // };
        match self {
            AlphaBlending::Native => false,
            AlphaBlending::Linear | AlphaBlending::LinearCorrected => true,
        }
    }
}

// pub const BackgroundImagePosition = enum { "top-left", "top-center", "top-right", "center-left", "center-center", "center-right", "bottom-left", "bottom-center", "bottom-right", center };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackgroundImagePosition {
    TopLeft,
    TopCenter,
    TopRight,
    CenterLeft,
    CenterCenter,
    CenterRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
    Center,
}

// pub const BackgroundImageFit = enum { contain, cover, stretch, none };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackgroundImageFit {
    Contain,
    Cover,
    Stretch,
    None,
}

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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FontShapingBreak {
    pub cursor: bool,
}

impl Default for FontShapingBreak {
    fn default() -> Self {
        Self { cursor: true }
    }
}

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

        // var buf: [std.fs.max_path_bytes]u8 = undefined;
        // PORT NOTE: `std.fs.max_path_bytes` is the platform max path length
        // (4096 on Linux/macOS); we size the stack buffer the same.
        let mut buf = [0u8; MAX_PATH_BYTES];
        // const expanded = internal_os.expandHome(path, &buf) catch |err| {
        //     log.warn(...);
        //     return;
        // };
        let expanded = match rustty_os::homedir::expandHome(&path, &mut buf) {
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

/// ghostty_config_color_s
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
    pub fn parse_cli(input_: Option<&[u8]>) -> Result<Color, ParseError> {
        // const input = input_ orelse return error.ValueRequired;
        let input = input_.ok_or(Error::ValueRequired)?;
        // Trim any whitespace before processing
        // const trimmed = std.mem.trim(u8, input, " \t");
        let trimmed = trim_ws(input);

        // if (terminal.x11_color.map.get(trimmed)) |rgb| return .{
        //     .r = rgb.r,
        //     .g = rgb.g,
        //     .b = rgb.b,
        // };
        //
        // PORT NOTE: `x11_color.map.get` takes a `&str`; X11 color names are ASCII,
        // so non-UTF-8 input simply has no match (the reference's byte map likewise
        // never matches such input), preserving behavior.
        if let Ok(s) = core::str::from_utf8(trimmed) {
            if let Some(rgb) = rustty_vt::x11_color::map.get(s) {
                return Ok(Color {
                    r: rgb.r,
                    g: rgb.g,
                    b: rgb.b,
                });
            }
        }

        // return fromHex(trimmed);
        Color::from_hex(trimmed)
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

    /// fromHex parses a color from a hex value such as #RRGGBB. The "#"
    /// is optional.
    // pub fn fromHex(input: []const u8) !Color
    pub fn from_hex(input: &[u8]) -> Result<Color, ParseError> {
        // Trim the beginning '#' if it exists
        // const trimmed = if (input.len != 0 and input[0] == '#') input[1..] else input;
        let trimmed = if !input.is_empty() && input[0] == b'#' {
            &input[1..]
        } else {
            input
        };
        // if (trimmed.len != 6 and trimmed.len != 3) return error.InvalidValue;
        if trimmed.len() != 6 && trimmed.len() != 3 {
            return Err(Error::InvalidValue);
        }

        // Expand short hex values to full hex values
        // const rgb: []const u8 = if (trimmed.len == 3) &.{
        //     trimmed[0], trimmed[0],
        //     trimmed[1], trimmed[1],
        //     trimmed[2], trimmed[2],
        // } else trimmed;
        let expanded: [u8; 6];
        let rgb: &[u8] = if trimmed.len() == 3 {
            expanded = [
                trimmed[0], trimmed[0], trimmed[1], trimmed[1], trimmed[2], trimmed[2],
            ];
            &expanded
        } else {
            trimmed
        };

        // Parse the colors two at a time.
        // var result: Color = undefined;
        let mut result = Color { r: 0, g: 0, b: 0 };
        // comptime var i: usize = 0;
        // inline while (i < 6) : (i += 2) {
        let mut i: usize = 0;
        while i < 6 {
            // const v: u8 = ((try std.fmt.charToDigit(rgb[i], 16)) * 16) +
            //     try std.fmt.charToDigit(rgb[i + 1], 16);
            let hi = char_to_digit(rgb[i], 16)?;
            let lo = char_to_digit(rgb[i + 1], 16)?;
            let v: u8 = hi * 16 + lo;

            // @field(result, switch (i) { 0 => "r", 2 => "g", 4 => "b", else => unreachable }) = v;
            match i {
                0 => result.r = v,
                2 => result.g = v,
                4 => result.b = v,
                _ => unreachable!(),
            }
            i += 2;
        }

        // return result;
        Ok(result)
    }
}

/// PORTS: `std.fmt.charToDigit(c, radix)` — returns the digit value of `c` in
/// the given radix, or `error.InvalidCharacter`. The reference maps any parse
/// failure here into `error.InvalidValue` at the call site (via `try`), but
/// `charToDigit` itself returns `InvalidCharacter`; `fromHex` propagates that as
/// the function's error union, which `parseIntoField` collapses to InvalidValue.
fn char_to_digit(c: u8, radix: u8) -> Result<u8, ParseError> {
    // const value = switch (c) {
    //     '0'...'9' => c - '0',
    //     'A'...'Z' => c - 'A' + 10,
    //     'a'...'z' => c - 'a' + 10,
    //     else => return error.InvalidCharacter,
    // };
    let value = match c {
        b'0'..=b'9' => c - b'0',
        b'A'..=b'Z' => c - b'A' + 10,
        b'a'..=b'z' => c - b'a' + 10,
        _ => return Err(Error::InvalidValue),
    };
    // if (value >= radix) return error.InvalidCharacter;
    if value >= radix {
        return Err(Error::InvalidValue);
    }
    // return value;
    Ok(value)
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackgroundBlur {
    False,
    True,
    MacosGlassRegular,
    MacosGlassClear,
    Radius(u8),
}

impl BackgroundBlur {
    // pub fn parseCLI(self: *BackgroundBlur, input: ?[]const u8) !void
    pub fn parse_cli(&mut self, input: Option<&[u8]>) -> Result<(), ParseError> {
        // const input_ = input orelse {
        //     // Emulate behavior for bools
        //     self.* = .true;
        //     return;
        // };
        let input_ = match input {
            Some(v) => v,
            None => {
                // Emulate behavior for bools
                *self = BackgroundBlur::True;
                return Ok(());
            }
        };

        // Try to parse normal bools
        // if (cli.args.parseBool(input_)) |b| {
        //     self.* = if (b) .true else .false;
        //     return;
        // } else |_| {}
        if let Ok(b) = parse_bool(input_) {
            *self = if b {
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
                *self = BackgroundBlur::False;
                return Ok(());
            }
            b"true" => {
                *self = BackgroundBlur::True;
                return Ok(());
            }
            b"macos-glass-regular" => {
                *self = BackgroundBlur::MacosGlassRegular;
                return Ok(());
            }
            b"macos-glass-clear" => {
                *self = BackgroundBlur::MacosGlassClear;
                return Ok(());
            }
            _ => {}
        }

        // self.* = .{ .radius = std.fmt.parseInt(u8, input_, 0) catch return error.InvalidValue };
        *self =
            BackgroundBlur::Radius(parse_int_u8_base0(input_).map_err(|_| Error::InvalidValue)?);
        Ok(())
    }

    // pub fn enabled(self: BackgroundBlur) bool
    pub fn enabled(self: BackgroundBlur) -> bool {
        // return switch (self) {
        match self {
            // .false => false,
            BackgroundBlur::False => false,
            // .true => true,
            BackgroundBlur::True => true,
            // .radius => |v| v > 0,
            BackgroundBlur::Radius(v) => v > 0,
            // We treat these as true because they both imply some blur!
            // This has the effect of making the standard blur happen on
            // Linux.
            // .@"macos-glass-regular", .@"macos-glass-clear" => true,
            BackgroundBlur::MacosGlassRegular | BackgroundBlur::MacosGlassClear => true,
        }
    }

    // pub fn cval(self: BackgroundBlur) i16
    pub fn cval(self: BackgroundBlur) -> i16 {
        // return switch (self) {
        match self {
            // .false => 0,
            BackgroundBlur::False => 0,
            // .true => 20,
            BackgroundBlur::True => 20,
            // .radius => |v| v,
            BackgroundBlur::Radius(v) => v as i16,
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
    pub fn format_entry<W: Writer>(
        self: BackgroundBlur,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), W::Error> {
        // switch (self) {
        match self {
            // .false => try formatter.formatEntry(bool, false),
            BackgroundBlur::False => format_entry_display(name, &false, writer),
            // .true => try formatter.formatEntry(bool, true),
            BackgroundBlur::True => format_entry_display(name, &true, writer),
            // .radius => |v| try formatter.formatEntry(u8, v),
            BackgroundBlur::Radius(v) => format_entry_display(name, &v, writer),
            // .@"macos-glass-regular" => try formatter.formatEntry([]const u8, "macos-glass-regular"),
            BackgroundBlur::MacosGlassRegular => {
                format_entry_string(name, b"macos-glass-regular", writer)
            }
            // .@"macos-glass-clear" => try formatter.formatEntry([]const u8, "macos-glass-clear"),
            BackgroundBlur::MacosGlassClear => {
                format_entry_string(name, b"macos-glass-clear", writer)
            }
        }
    }
}

impl FormatEntry for BackgroundBlur {
    fn format_entry<W: Writer>(&self, name: &[u8], writer: &mut W) -> Result<(), W::Error> {
        BackgroundBlur::format_entry(*self, name, writer)
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

/// ghostty_config_color_list_s
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

/// ghostty_config_palette_s
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
    pub fn parse_cli(&mut self, input: Option<&[u8]>) -> Result<(), PaletteParseError> {
        // const value = input orelse return error.ValueRequired;
        let value = input.ok_or(PaletteParseError::ValueRequired)?;
        // const eqlIdx = std.mem.indexOf(u8, value, "=") orelse return error.InvalidValue;
        let eql_idx = index_of(value, b'=').ok_or(PaletteParseError::InvalidValue)?;

        // Parse the key part (trim whitespace)
        // const key = try std.fmt.parseInt(u8, std.mem.trim(u8, value[0..eqlIdx], " \t"), 0);
        let key = parse_int_u8_base0(trim_ws(&value[0..eql_idx])).map_err(|e| match e {
            ParseIntError::Overflow => PaletteParseError::Overflow,
            ParseIntError::InvalidCharacter => PaletteParseError::InvalidCharacter,
        })?;

        // Parse the color part (Color.parseCLI will handle whitespace)
        // const rgb = try Color.parseCLI(value[eqlIdx + 1 ..]);
        let rgb = Color::parse_cli(Some(&value[eql_idx + 1..])).map_err(|e| match e {
            Error::ValueRequired => PaletteParseError::ValueRequired,
            _ => PaletteParseError::InvalidValue,
        })?;
        // self.value[key] = .{ .r = rgb.r, .g = rgb.g, .b = rgb.b };
        self.value[key as usize] = rustty_vt::color::RGB {
            r: rgb.r,
            g: rgb.g,
            b: rgb.b,
        };
        // self.mask.set(key);
        self.mask.set(key as usize);
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
#[derive(Debug, Clone, Default)]
pub struct Keybinds {
    // set: inputpkg.Binding.Set = .{},
    pub set: rustty_input::binding::Set,

    /// Defined key tables. The default key table is always the root "set",
    /// which allows all table names to be available without reservation.
    // tables: std.StringArrayHashMapUnmanaged(inputpkg.Binding.Set) = .empty,
    pub tables: std::collections::HashMap<Vec<u8>, rustty_input::binding::Set>,

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

// PORT NOTE (DEFERRED SEAM, this sub-batch): `Keybinds` is the single largest
// Config value-type. Its `init` reproduces the entire default keybind set
// (hundreds of `set.put`/`putFlags` calls with the full physical/unicode/mods
// trigger matrix) and its `parseCLI` drives the cross-table `chain=`/table-name
// state machine over `inputpkg.Binding.Set`. This is its own fill sub-batch (the
// skeleton flags it separately at Config.zig:6415); it is deferred here so the
// self-contained value-types in this sub-batch land under a scoped gate first.
// The method bodies below preserve the reference signatures; each is a clearly
// surfaced deferral (NOT a stub-to-pass: no Keybinds value-type test is claimed
// green by this sub-batch).
impl Keybinds {
    // pub fn init(self: *Keybinds, alloc: Allocator) !void
    pub fn init(&mut self) {
        unimplemented!("Keybinds.init — deferred to the Keybinds fill sub-batch")
    }

    // pub fn parseCLI(self: *Keybinds, alloc: Allocator, input: ?[]const u8) !void
    pub fn parse_cli(&mut self, _input: Option<&[u8]>) -> Result<(), ParseError> {
        unimplemented!("Keybinds.parseCLI — deferred to the Keybinds fill sub-batch")
    }

    // pub fn clone(self: *const Keybinds, alloc: Allocator) Allocator.Error!Keybinds
    pub fn clone_in(&self) -> Keybinds {
        unimplemented!("Keybinds.clone — deferred to the Keybinds fill sub-batch")
    }

    // pub fn equal(self: Keybinds, other: Keybinds) bool
    pub fn equal(&self, _other: &Keybinds) -> bool {
        unimplemented!("Keybinds.equal — deferred to the Keybinds fill sub-batch")
    }

    // pub fn formatEntryDocs(self: Keybinds, formatter, docs: bool) !void
    pub fn format_entry_docs(&self, _docs: bool) {
        unimplemented!("Keybinds.formatEntryDocs — deferred to the Keybinds fill sub-batch")
    }

    // pub fn formatEntry(self: Keybinds, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry(&self) {
        unimplemented!("Keybinds.formatEntry — deferred to the Keybinds fill sub-batch")
    }
}

// pub const RepeatableCodepointMap = struct { map: fontpkg.CodepointMap };
#[derive(Debug, Clone, Default)]
pub struct RepeatableCodepointMap {
    pub map: CodepointMap,
}

// PORT NOTE (DEFERRED SEAM, R4): `RepeatableCodepointMap` wraps `fontpkg.CodepointMap`
// (`font/CodepointMap.zig`), which is not yet ported in rustty-font (modeled here
// as the `CodepointMap` unit-struct placeholder). Its `parseCLI`/`clone`/`equal`/
// `formatEntry` all operate on that font map's `add`/`list`/`descriptor` API. The
// shared `UnicodeRangeParser` IS ported (module-scope, consumed by
// `RepeatableClipboardCodepointMap`); only the font-map-coupled bodies are
// deferred until `font/CodepointMap.zig` lands. Each is a clearly surfaced
// deferral (NOT a stub-to-pass).
impl RepeatableCodepointMap {
    // pub fn parseCLI(self: *Self, alloc: Allocator, input_: ?[]const u8) !void
    pub fn parse_cli(&mut self, _input: Option<&[u8]>) -> Result<(), ParseError> {
        unimplemented!(
            "RepeatableCodepointMap.parseCLI — deferred until font/CodepointMap.zig lands"
        )
    }

    // pub fn clone(self: *const Self, alloc: Allocator) Allocator.Error!Self
    pub fn clone_in(&self) -> RepeatableCodepointMap {
        unimplemented!("RepeatableCodepointMap.clone — deferred until font/CodepointMap.zig lands")
    }

    // pub fn equal(self: Self, other: Self) bool
    pub fn equal(&self, _other: &RepeatableCodepointMap) -> bool {
        unimplemented!("RepeatableCodepointMap.equal — deferred until font/CodepointMap.zig lands")
    }

    // pub fn formatEntry(self: Self, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry(&self) {
        unimplemented!(
            "RepeatableCodepointMap.formatEntry — deferred until font/CodepointMap.zig lands"
        )
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

/// ghostty_config_command_palette_entry_s (the extern C view).
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
// DEFERRED (not ported in this sub-batch): the `Keybinds` and
// `RepeatableCodepointMap` value-type tests, whose impls are deferred DI/infra
// seams (see the PORT NOTEs on those types). `RepeatableCommand`'s tests ARE
// ported below (the `Command: AutoStruct` seam is now resolved).
// ===========================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use rustty_cfg_fmt::{entry_formatter, AllocatingWriter};

    // -----------------------------------------------------------------------
    // Color
    // -----------------------------------------------------------------------

    // test "fromHex"
    #[test]
    fn color_from_hex() {
        // try testing.expectEqual(Color{ .r = 0, .g = 0, .b = 0 }, try Color.fromHex("#000000"));
        assert_eq!(
            Color { r: 0, g: 0, b: 0 },
            Color::from_hex(b"#000000").unwrap()
        );
        // try testing.expectEqual(Color{ .r = 10, .g = 11, .b = 12 }, try Color.fromHex("#0A0B0C"));
        assert_eq!(
            Color {
                r: 10,
                g: 11,
                b: 12
            },
            Color::from_hex(b"#0A0B0C").unwrap()
        );
        // try testing.expectEqual(Color{ .r = 10, .g = 11, .b = 12 }, try Color.fromHex("0A0B0C"));
        assert_eq!(
            Color {
                r: 10,
                g: 11,
                b: 12
            },
            Color::from_hex(b"0A0B0C").unwrap()
        );
        // try testing.expectEqual(Color{ .r = 255, .g = 255, .b = 255 }, try Color.fromHex("FFFFFF"));
        assert_eq!(
            Color {
                r: 255,
                g: 255,
                b: 255
            },
            Color::from_hex(b"FFFFFF").unwrap()
        );
        // try testing.expectEqual(Color{ .r = 255, .g = 255, .b = 255 }, try Color.fromHex("FFF"));
        assert_eq!(
            Color {
                r: 255,
                g: 255,
                b: 255
            },
            Color::from_hex(b"FFF").unwrap()
        );
        // try testing.expectEqual(Color{ .r = 51, .g = 68, .b = 85 }, try Color.fromHex("#345"));
        assert_eq!(
            Color {
                r: 51,
                g: 68,
                b: 85
            },
            Color::from_hex(b"#345").unwrap()
        );
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
        let expected: Vec<u8> =
            match rustty_os::homedir::expandHome(b"~/projects/ghostty", &mut buf) {
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
    fn mouse_scroll_multiplier_format_entry() {
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
        let mut v = BackgroundBlur::False;

        // try v.parseCLI(null);
        v.parse_cli(None).unwrap();
        // try testing.expectEqual(.true, v);
        assert_eq!(BackgroundBlur::True, v);

        // try v.parseCLI("true");
        v.parse_cli(Some(b"true")).unwrap();
        assert_eq!(BackgroundBlur::True, v);

        // try v.parseCLI("false");
        v.parse_cli(Some(b"false")).unwrap();
        assert_eq!(BackgroundBlur::False, v);

        // try v.parseCLI("42");
        v.parse_cli(Some(b"42")).unwrap();
        // try testing.expectEqual(42, v.radius);
        assert_eq!(BackgroundBlur::Radius(42), v);

        // try v.parseCLI("macos-glass-regular");
        v.parse_cli(Some(b"macos-glass-regular")).unwrap();
        assert_eq!(BackgroundBlur::MacosGlassRegular, v);

        // try v.parseCLI("macos-glass-clear");
        v.parse_cli(Some(b"macos-glass-clear")).unwrap();
        assert_eq!(BackgroundBlur::MacosGlassClear, v);

        // try testing.expectError(error.InvalidValue, v.parseCLI(""));
        assert_eq!(Err(Error::InvalidValue), v.parse_cli(Some(b"")));
        // try testing.expectError(error.InvalidValue, v.parseCLI("aaaa"));
        assert_eq!(Err(Error::InvalidValue), v.parse_cli(Some(b"aaaa")));
        // try testing.expectError(error.InvalidValue, v.parseCLI("420"));
        assert_eq!(Err(Error::InvalidValue), v.parse_cli(Some(b"420")));
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
}
