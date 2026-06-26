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

use rustty_args::args::ParseError;

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
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FaceVariation {
    pub id: u32,
    pub value: f64,
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
        let _ = self;
        todo!("port AlphaBlending.isLinear")
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
    // pub fn parseCLI(input_: ?[]const u8) !WindowDecoration
    pub fn parse_cli(_input: Option<&[u8]>) -> Result<WindowDecoration, ParseError> {
        todo!("port WindowDecoration.parseCLI")
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
    pub fn parse_cli(&mut self, _input: Option<&[u8]>) -> Result<(), ParseError> {
        todo!("port WorkingDirectory.parseCLI")
    }

    /// Expand tilde paths in .path values.
    // pub fn finalize(self: *Self, alloc: Allocator) Allocator.Error!void
    pub fn finalize(&mut self) {
        todo!("port WorkingDirectory.finalize")
    }

    // pub fn value(self: Self) ?[]const u8
    pub fn value(&self) -> Option<&[u8]> {
        todo!("port WorkingDirectory.value")
    }

    // pub fn clone(self: Self, alloc: Allocator) Allocator.Error!Self
    pub fn clone_in(&self) -> WorkingDirectory {
        todo!("port WorkingDirectory.clone")
    }

    // pub fn formatEntry(self: Self, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry(&self) {
        todo!("port WorkingDirectory.formatEntry")
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
        todo!("port Color.cval")
    }

    /// Convert this to the terminal RGB struct
    // pub fn toTerminalRGB(self: Color) terminal.color.RGB
    pub fn to_terminal_rgb(self: Color) -> rustty_vt::color::RGB {
        let _ = self;
        todo!("port Color.toTerminalRGB")
    }

    // pub fn parseCLI(input_: ?[]const u8) !Color
    pub fn parse_cli(_input: Option<&[u8]>) -> Result<Color, ParseError> {
        todo!("port Color.parseCLI")
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
    pub fn format_entry(self: Color) {
        let _ = self;
        todo!("port Color.formatEntry")
    }

    /// Format the color as a string.
    // pub fn formatBuf(self: Color, buf: []u8) Allocator.Error![]const u8
    pub fn format_buf(self: Color, _buf: &mut [u8]) -> Vec<u8> {
        let _ = self;
        todo!("port Color.formatBuf")
    }

    /// fromHex parses a color from a hex value such as #RRGGBB. The "#"
    /// is optional.
    // pub fn fromHex(input: []const u8) !Color
    pub fn from_hex(_input: &[u8]) -> Result<Color, ParseError> {
        todo!("port Color.fromHex")
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
    pub fn parse_cli(_input: Option<&[u8]>) -> Result<TerminalColor, ParseError> {
        todo!("port TerminalColor.parseCLI")
    }

    // pub fn toTerminalRGB(self: TerminalColor) ?terminal.color.RGB
    pub fn to_terminal_rgb(self: TerminalColor) -> Option<rustty_vt::color::RGB> {
        let _ = self;
        todo!("port TerminalColor.toTerminalRGB")
    }

    /// Used by Formatter
    // pub fn formatEntry(self: TerminalColor, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry(self: TerminalColor) {
        let _ = self;
        todo!("port TerminalColor.formatEntry")
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
        let _ = self;
        todo!("port BoldColor.toTerminal")
    }

    // pub fn parseCLI(input_: ?[]const u8) !BoldColor
    pub fn parse_cli(_input: Option<&[u8]>) -> Result<BoldColor, ParseError> {
        todo!("port BoldColor.parseCLI")
    }

    /// Used by Formatter
    // pub fn formatEntry(self: BoldColor, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry(self: BoldColor) {
        let _ = self;
        todo!("port BoldColor.formatEntry")
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
    pub fn parse_cli(&mut self, _input: Option<&[u8]>) -> Result<(), ParseError> {
        todo!("port FontStyle.parseCLI")
    }

    // pub fn nameValue(self: Self) ?[:0]const u8
    pub fn name_value(&self) -> Option<&[u8]> {
        todo!("port FontStyle.nameValue")
    }

    // pub fn clone(self: Self, alloc: Allocator) Allocator.Error!Self
    pub fn clone_in(&self) -> FontStyle {
        todo!("port FontStyle.clone")
    }

    // pub fn formatEntry(self: Self, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry(&self) {
        todo!("port FontStyle.formatEntry")
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
    pub fn parse_cli(&mut self, _input: Option<&[u8]>) -> Result<(), ParseError> {
        todo!("port BackgroundBlur.parseCLI")
    }

    // pub fn enabled(self: BackgroundBlur) bool
    pub fn enabled(self: BackgroundBlur) -> bool {
        let _ = self;
        todo!("port BackgroundBlur.enabled")
    }

    // pub fn cval(self: BackgroundBlur) i16
    pub fn cval(self: BackgroundBlur) -> i16 {
        let _ = self;
        todo!("port BackgroundBlur.cval")
    }

    // pub fn formatEntry(self: BackgroundBlur, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry(self: BackgroundBlur) {
        let _ = self;
        todo!("port BackgroundBlur.formatEntry")
    }
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
        todo!("port ColorList.cval")
    }

    // pub fn parseCLI(self: *Self, alloc: Allocator, input_: ?[]const u8) !void
    pub fn parse_cli(&mut self, _input: Option<&[u8]>) -> Result<(), ParseError> {
        todo!("port ColorList.parseCLI")
    }

    // pub fn clone(self: *const Self, alloc: Allocator) Allocator.Error!Self
    pub fn clone_in(&self) -> ColorList {
        todo!("port ColorList.clone")
    }

    // pub fn equal(self: Self, other: Self) bool
    pub fn equal(&self, _other: &ColorList) -> bool {
        todo!("port ColorList.equal")
    }

    // pub fn formatEntry(self: Self, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry(&self) {
        todo!("port ColorList.formatEntry")
    }
}

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

impl Palette {
    // pub fn cval(self: Self) Palette.C
    pub fn cval(&self) -> PaletteC {
        todo!("port Palette.cval")
    }

    // pub fn parseCLI(self: *Self, alloc: Allocator, input_: ?[]const u8) !void
    pub fn parse_cli(&mut self, _input: Option<&[u8]>) -> Result<(), ParseError> {
        todo!("port Palette.parseCLI")
    }

    // pub fn clone(self: Self, _: Allocator) error{}!Self
    pub fn clone_in(&self) -> Palette {
        self.clone()
    }

    // pub fn equal(self: Self, other: Self) bool
    pub fn equal(&self, _other: &Palette) -> bool {
        todo!("port Palette.equal")
    }

    // pub fn formatEntry(self: Self, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry(&self) {
        todo!("port Palette.formatEntry")
    }
}

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
    pub fn parse_cli(&mut self, _input: Option<&[u8]>) -> Result<(), ParseError> {
        todo!("port RepeatableString.parseCLI")
    }

    // pub fn clone(self: *const Self, alloc: Allocator) Allocator.Error!Self
    pub fn clone_in(&self) -> RepeatableString {
        todo!("port RepeatableString.clone")
    }

    // pub fn count(self: Self) usize
    pub fn count(&self) -> usize {
        todo!("port RepeatableString.count")
    }

    // pub fn equal(self: Self, other: Self) bool
    pub fn equal(&self, _other: &RepeatableString) -> bool {
        todo!("port RepeatableString.equal")
    }

    // pub fn formatEntry(self: Self, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry(&self) {
        todo!("port RepeatableString.formatEntry")
    }
}

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
    pub fn parse_cli(&mut self, _input: Option<&[u8]>) -> Result<(), ParseError> {
        todo!("port SelectionWordChars.parseCLI")
    }

    // pub fn clone(self: *const Self, alloc: Allocator) Allocator.Error!Self
    pub fn clone_in(&self) -> SelectionWordChars {
        todo!("port SelectionWordChars.clone")
    }

    // pub fn equal(self: Self, other: Self) bool
    pub fn equal(&self, _other: &SelectionWordChars) -> bool {
        todo!("port SelectionWordChars.equal")
    }

    // pub fn formatEntry(self: Self, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry(&self) {
        todo!("port SelectionWordChars.formatEntry")
    }
}

// pub const RepeatableFontVariation = struct { list: ArrayListUnmanaged(fontpkg.face.Variation) };
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RepeatableFontVariation {
    // Allocator for the list is the arena for the parent config.
    pub list: Vec<FaceVariation>,
}

impl RepeatableFontVariation {
    // pub fn parseCLI(self: *Self, alloc: Allocator, input_: ?[]const u8) !void
    pub fn parse_cli(&mut self, _input: Option<&[u8]>) -> Result<(), ParseError> {
        todo!("port RepeatableFontVariation.parseCLI")
    }

    // pub fn clone(self: *const Self, alloc: Allocator) Allocator.Error!Self
    pub fn clone_in(&self) -> RepeatableFontVariation {
        todo!("port RepeatableFontVariation.clone")
    }

    // pub fn equal(self: Self, other: Self) bool
    pub fn equal(&self, _other: &RepeatableFontVariation) -> bool {
        todo!("port RepeatableFontVariation.equal")
    }

    // pub fn formatEntry(self: Self, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry(&self) {
        todo!("port RepeatableFontVariation.formatEntry")
    }
}

/// PORTS: `Keybinds` (Config.zig:6415). The big keybind set wrapping
/// `inputpkg.Binding.Set`. Skeleton with the field set + method sigs.
// pub const Keybinds = struct { set, tables, chain_target: union(enum) {...} };
#[derive(Debug, Clone, Default)]
pub struct Keybinds {
    pub set: rustty_input::binding::Set,
    // tables: std.StringArrayHashMapUnmanaged(inputpkg.Binding.Set) = .empty,
    pub tables: std::collections::HashMap<Vec<u8>, rustty_input::binding::Set>,
}

impl Keybinds {
    // pub fn init(self: *Keybinds, alloc: Allocator) !void
    pub fn init(&mut self) {
        todo!("port Keybinds.init")
    }

    // pub fn parseCLI(self: *Keybinds, alloc: Allocator, input: ?[]const u8) !void
    pub fn parse_cli(&mut self, _input: Option<&[u8]>) -> Result<(), ParseError> {
        todo!("port Keybinds.parseCLI")
    }

    // pub fn clone(self: *const Keybinds, alloc: Allocator) Allocator.Error!Keybinds
    pub fn clone_in(&self) -> Keybinds {
        todo!("port Keybinds.clone")
    }

    // pub fn equal(self: Keybinds, other: Keybinds) bool
    pub fn equal(&self, _other: &Keybinds) -> bool {
        todo!("port Keybinds.equal")
    }

    // pub fn formatEntryDocs(self: Keybinds, formatter, docs: bool) !void
    pub fn format_entry_docs(&self, _docs: bool) {
        todo!("port Keybinds.formatEntryDocs")
    }

    // pub fn formatEntry(self: Keybinds, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry(&self) {
        todo!("port Keybinds.formatEntry")
    }
}

// pub const RepeatableCodepointMap = struct { map: fontpkg.CodepointMap };
#[derive(Debug, Clone, Default)]
pub struct RepeatableCodepointMap {
    pub map: CodepointMap,
}

impl RepeatableCodepointMap {
    // pub fn parseCLI(self: *Self, alloc: Allocator, input_: ?[]const u8) !void
    pub fn parse_cli(&mut self, _input: Option<&[u8]>) -> Result<(), ParseError> {
        todo!("port RepeatableCodepointMap.parseCLI")
    }

    // pub fn clone(self: *const Self, alloc: Allocator) Allocator.Error!Self
    pub fn clone_in(&self) -> RepeatableCodepointMap {
        todo!("port RepeatableCodepointMap.clone")
    }

    // pub fn equal(self: Self, other: Self) bool
    pub fn equal(&self, _other: &RepeatableCodepointMap) -> bool {
        todo!("port RepeatableCodepointMap.equal")
    }

    // pub fn formatEntry(self: Self, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry(&self) {
        todo!("port RepeatableCodepointMap.formatEntry")
    }
}

// pub const RepeatableClipboardCodepointMap = struct { map: ClipboardCodepointMap };
#[derive(Default)]
pub struct RepeatableClipboardCodepointMap {
    pub map: crate::ClipboardCodepointMap,
}

impl RepeatableClipboardCodepointMap {
    // pub fn parseCLI(self: *Self, alloc: Allocator, input_: ?[]const u8) !void
    pub fn parse_cli(&mut self, _input: Option<&[u8]>) -> Result<(), ParseError> {
        todo!("port RepeatableClipboardCodepointMap.parseCLI")
    }

    // pub fn clone(self: *const Self, alloc: Allocator) Allocator.Error!Self
    pub fn clone_in(&self) -> RepeatableClipboardCodepointMap {
        todo!("port RepeatableClipboardCodepointMap.clone")
    }

    // pub fn equal(self: Self, other: Self) bool
    pub fn equal(&self, _other: &RepeatableClipboardCodepointMap) -> bool {
        todo!("port RepeatableClipboardCodepointMap.equal")
    }

    // pub fn formatEntry(self: Self, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry(&self) {
        todo!("port RepeatableClipboardCodepointMap.formatEntry")
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
        todo!("port RepeatableLink.parseCLI")
    }

    // pub fn clone(self: *const Self, alloc: Allocator) Allocator.Error!Self
    pub fn clone_in(&self) -> RepeatableLink {
        todo!("port RepeatableLink.clone")
    }

    // pub fn equal(self: Self, other: Self) bool
    pub fn equal(&self, _other: &RepeatableLink) -> bool {
        todo!("port RepeatableLink.equal")
    }

    // pub fn formatEntry(self: Self, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry(&self) {
        todo!("port RepeatableLink.formatEntry")
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

impl RepeatableCommand {
    // pub fn cval(self: *const Self) C
    pub fn cval(&self) -> RepeatableCommandC {
        todo!("port RepeatableCommand.cval")
    }

    // pub fn init(self: *Self, alloc: Allocator) !void
    pub fn init(&mut self) {
        todo!("port RepeatableCommand.init")
    }

    // pub fn parseCLI(self: *Self, alloc: Allocator, input_: ?[]const u8) !void
    pub fn parse_cli(&mut self, _input: Option<&[u8]>) -> Result<(), ParseError> {
        todo!("port RepeatableCommand.parseCLI")
    }

    // pub fn clone(self: *const Self, alloc: Allocator) Allocator.Error!Self
    pub fn clone_in(&self) -> RepeatableCommand {
        todo!("port RepeatableCommand.clone")
    }

    // pub fn equal(self: RepeatableCommand, other: RepeatableCommand) bool
    pub fn equal(&self, _other: &RepeatableCommand) -> bool {
        todo!("port RepeatableCommand.equal")
    }

    // pub fn formatEntry(self: Self, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry(&self) {
        todo!("port RepeatableCommand.formatEntry")
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
    pub fn parse_cli(&mut self, _input: Option<&[u8]>) -> Result<(), ParseError> {
        todo!("port MouseScrollMultiplier.parseCLI")
    }

    // pub fn clone(self: *const Self, alloc: Allocator) Allocator.Error!Self
    pub fn clone_in(&self) -> MouseScrollMultiplier {
        todo!("port MouseScrollMultiplier.clone")
    }

    // pub fn equal(self: Self, other: Self) bool
    pub fn equal(&self, _other: &MouseScrollMultiplier) -> bool {
        todo!("port MouseScrollMultiplier.equal")
    }

    // pub fn formatEntry(self: Self, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry(&self) {
        todo!("port MouseScrollMultiplier.formatEntry")
    }
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

impl QuickTerminalSizeSize {
    // pub fn toPixels(self: Size, parent_dimensions: u32) u32
    pub fn to_pixels(self: QuickTerminalSizeSize, _parent_dimensions: u32) -> u32 {
        let _ = self;
        todo!("port QuickTerminalSize.Size.toPixels")
    }

    // pub fn parse(input: []const u8) !Size
    pub fn parse(_input: &[u8]) -> Result<QuickTerminalSizeSize, ParseError> {
        todo!("port QuickTerminalSize.Size.parse")
    }
}

// pub const Dimensions = struct { width: u32, height: u32 };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuickTerminalSizeDimensions {
    pub width: u32,
    pub height: u32,
}

impl QuickTerminalSize {
    // pub fn cval(self: QuickTerminalSize) C
    // pub fn calculate(...) Dimensions
    pub fn calculate(&self) -> QuickTerminalSizeDimensions {
        todo!("port QuickTerminalSize.calculate")
    }

    // pub fn parseCLI(self: *QuickTerminalSize, input: ?[]const u8) !void
    pub fn parse_cli(&mut self, _input: Option<&[u8]>) -> Result<(), ParseError> {
        todo!("port QuickTerminalSize.parseCLI")
    }

    // pub fn clone(self: *const QuickTerminalSize, _: Allocator) Allocator.Error!QuickTerminalSize
    pub fn clone_in(&self) -> QuickTerminalSize {
        *self
    }

    // pub fn formatEntry(self: QuickTerminalSize, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry(&self) {
        todo!("port QuickTerminalSize.formatEntry")
    }
}

// pub const Theme = struct { light: []const u8, dark: []const u8 };
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Theme {
    pub light: Vec<u8>,
    pub dark: Vec<u8>,
}

impl Theme {
    // pub fn parseCLI(self: *Theme, alloc: Allocator, input_: ?[]const u8) !void
    pub fn parse_cli(&mut self, _input: Option<&[u8]>) -> Result<(), ParseError> {
        todo!("port Theme.parseCLI")
    }

    // pub fn clone(self: *const Theme, alloc: Allocator) Allocator.Error!Theme
    pub fn clone_in(&self) -> Theme {
        todo!("port Theme.clone")
    }

    // pub fn formatEntry(self: Theme, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry(&self) {
        todo!("port Theme.formatEntry")
    }
}

// pub const Duration = struct { duration: u64 = 0; const units = [...]; ... };
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Duration {
    /// Duration in nanoseconds
    pub duration: u64,
}

impl Duration {
    // pub fn clone(self: *const Duration, _: Allocator) error{}!Duration
    pub fn clone_in(&self) -> Duration {
        *self
    }

    // pub fn equal(self: Duration, other: Duration) bool
    pub fn equal(self: Duration, other: Duration) -> bool {
        self == other
    }

    // pub fn round(self: Duration, to: u64) Duration
    pub fn round(self: Duration, _to: u64) -> Duration {
        todo!("port Duration.round")
    }

    // pub fn lte(self: Duration, other: Duration) bool
    pub fn lte(self: Duration, _other: Duration) -> bool {
        todo!("port Duration.lte")
    }

    // pub fn parseCLI(input: ?[]const u8) !Duration
    pub fn parse_cli(_input: Option<&[u8]>) -> Result<Duration, ParseError> {
        todo!("port Duration.parseCLI")
    }

    // pub fn formatEntry(self: Duration, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry(&self) {
        todo!("port Duration.formatEntry")
    }

    // pub fn cval(self: Duration) usize
    pub fn cval(self: Duration) -> usize {
        let _ = self;
        todo!("port Duration.cval")
    }

    // pub fn asMilliseconds(self: Duration) c_uint
    pub fn as_milliseconds(self: Duration) -> std::os::raw::c_uint {
        let _ = self;
        todo!("port Duration.asMilliseconds")
    }
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
        *self
    }

    // pub fn equal(self: Self, other: Self) bool
    pub fn equal(self: WindowPadding, other: WindowPadding) -> bool {
        self == other
    }

    // pub fn parseCLI(input_: ?[]const u8) !WindowPadding
    pub fn parse_cli(_input: Option<&[u8]>) -> Result<WindowPadding, ParseError> {
        todo!("port WindowPadding.parseCLI")
    }

    // pub fn formatEntry(self: Self, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry(&self) {
        todo!("port WindowPadding.formatEntry")
    }
}
