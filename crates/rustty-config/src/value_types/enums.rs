//! PORTS: ghostty src/config/Config.zig — the in-file simple enums the `Config`
//! fields use, and their bare-enum name maps (PARSE `stringToEnum` + FORMAT
//! `@tagName`).

use super::*;

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

/// Options for copy on select behavior.
// pub const CopyOnSelect = enum { none, primary, clipboard, both };
//
// PORT NOTE (`dfccdb2d4` + `e80ce2ed4`, #12600, phase-6): the reference RENAMED
// `false`/`true` to `none`/`primary`, kept `clipboard` (now "system clipboard
// ONLY") and added `both` (the old `clipboard` meaning). `true`/`false` survive
// only as spellings in the `copy-on-select` compatibility handler
// (`Config::compat_copy_on_select`), never as variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyOnSelect {
    /// Disables copy on select entirely. This is the default on macOS.
    None,

    /// Copy on select is enabled, but goes to the selection clipboard.
    /// This is not supported on platforms such as macOS. This is the default
    /// on Linux.
    Primary,

    /// Copy on select is enabled and goes to the system clipboard.
    Clipboard,

    /// Copy on select is enabled and goes to both the system clipboard
    /// and the selection clipboard (for Linux).
    Both,
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
// pub const MiddleClickAction = enum { "primary-paste", "clipboard-paste", ignore };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MiddleClickAction {
    /// Paste from the selection clipboard.
    PrimaryPaste,
    /// Paste from the standard clipboard.
    ClipboardPaste,

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

/// See drag-handle
// pub const DragHandle = enum { always, auto, never };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragHandle {
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

// pub const CopyOnSelect = enum { none, primary, clipboard, both };
bare_enum_names!(CopyOnSelect {
    None => b"none",
    Primary => b"primary",
    Clipboard => b"clipboard",
    Both => b"both",
});

// pub const RightClickAction = enum { ignore, paste, copy, "copy-or-paste", "context-menu" };
bare_enum_names!(RightClickAction {
    Ignore => b"ignore",
    Paste => b"paste",
    Copy => b"copy",
    CopyOrPaste => b"copy-or-paste",
    ContextMenu => b"context-menu",
});

// pub const MiddleClickAction = enum { "primary-paste", "clipboard-paste", ignore };
bare_enum_names!(MiddleClickAction {
    PrimaryPaste => b"primary-paste",
    ClipboardPaste => b"clipboard-paste",
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

// pub const DragHandle = enum { always, auto, never };
bare_enum_names!(DragHandle {
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
