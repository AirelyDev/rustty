//! rustty-config::config_struct
//!
//! PORTS: the main `Config` struct of ghostty `src/config/Config.zig` plus
//! `src/config/key.zig` (the `Key` enum, which key.zig derives FROM the Config
//! fields — so it lives here WITH Config, not in cfg-fmt).
//!
//! PORT STATUS: C2 SKELETON. This lays down:
//!   - the `Config` struct with ALL ~105 public config fields (exact reference
//!     names → Rust idents; hyphens → underscores; `@"_x"` internal fields kept
//!     with a leading `_`), their value-types, and reference defaults via the
//!     `Default` impl;
//!   - the `Key` enum (port of key.zig) — one variant per non-`_` field;
//!   - the `compatibility` rename table entries (data);
//!   - the four-pass method SIGNATURES (`load`/`default`/`loadIter`/`loadFile`/
//!     `finalize`/`parseManuallyHook`/`clone`/`cloneEmpty`/`changed`) as
//!     `todo!()`, plus `ChangeIterator` and `Replay`;
//!   - the `Target` impl skeleton wiring per-field PARSE dispatch.
//!
//! The per-field doc comments (Pandoc Markdown), the method bodies, the
//! `compatibility` handler bodies, and EVERY `test {}` (R1 — the count is a
//! contract) are ported in the subsequent fill sub-batches, each gated on a
//! SCOPED G3b (per-sub-batch comment-coverage; memory `per-subbatch-scoped-g3b`).
//! This skeleton intentionally carries NO logic and NO tests; it exists only so
//! `rustty-config` COMPILES with the full type surface present.
//
// Config is the main config struct. These fields map directly to the
// CLI flag names hence we use a lot of `@""` syntax to support hyphens.
//
// Pandoc is used to automatically generate manual pages and other forms of
// documentation, so documentation comments on fields in the Config struct
// should use Pandoc's flavor of Markdown.
//
// For a reference to Pandoc's Markdown see their [online
// manual.](https://pandoc.org/MANUAL.html#pandocs-markdown)

#![allow(dead_code)]
#![allow(non_camel_case_types)]
#![allow(clippy::type_complexity)]

// const std = @import("std");
// const builtin = @import("builtin");
// const build_config = @import("../build_config.zig");
// const assert = @import("../quirks.zig").inlineAssert;
// const Allocator = std.mem.Allocator;
// const ArenaAllocator = std.heap.ArenaAllocator;
// const global_state = &@import("../global.zig").state;
// const deepEqual = @import("../datastruct/comparison.zig").deepEqual;
// const fontpkg = @import("../font/main.zig");
// const inputpkg = @import("../input.zig");
// const internal_os = @import("../os/main.zig");
// const cli = @import("../cli.zig");
// const conditional = @import("conditional.zig");
// const Conditional = conditional.Conditional;
// const file_load = @import("file_load.zig");
// const formatterpkg = @import("formatter.zig");
// const themepkg = @import("theme.zig");
// const url = @import("url.zig");
// pub const Key = @import("key.zig").Key;
// const MetricModifier = fontpkg.Metrics.Modifier;
// pub const Command = @import("command.zig").Command;
// const RepeatableReadableIO = @import("io.zig").RepeatableReadableIO;
// const RepeatableStringMap = @import("RepeatableStringMap.zig");
// pub const Path = @import("path.zig").Path;
// pub const RepeatablePath = @import("path.zig").RepeatablePath;
// const ClipboardCodepointMap = @import("ClipboardCodepointMap.zig");
// const KeyRemapSet = @import("../input/key_mods.zig").RemapSet;
// pub const WindowPaddingBalance = @import("../renderer/size.zig").PaddingBalance;
// const string = @import("string.zig");

use rustty_args::args::{ParseError, Target};
use rustty_args::diagnostics::DiagnosticList;

use crate::command::Command;
use crate::conditional::{self, Conditional};
use crate::io::RepeatableReadableIO;
use crate::path::{Path, RepeatablePath};
use crate::repeatable_string_map::RepeatableStringMap;
use crate::value_types::*;

// We do this instead of importing all of terminal/main.zig to limit the
// dependency graph. (terminal.CursorStyle / terminal.color.* via rustty-vt)
// const terminal = struct { const CursorStyle = ...; const color = ...; ... };
pub use rustty_vt::cursor::Style as CursorStyle;

// const log = std.log.scoped(.config);

/// Key is an enum of all the available configuration keys. This is used
/// when paired with diff to determine what fields have changed in a config,
/// amongst other things.
///
/// PORTS: `key.zig`'s comptime `Key` — derived from the non-`_` Config fields.
/// In Zig this is generated at comptime via `std.meta.fields(Config)`; Rust has
/// no struct reflection, so the enum is written explicitly. The ONE exhaustiveness
/// test (added in the fill sub-batch) asserts every `Key` variant has a
/// parse+format+clone+changed arm, replacing Zig's comptime completeness check.
// pub const Key = key: { ... derived from Config fields ... };
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    Language,
    FontFamily,
    FontFamilyBold,
    FontFamilyItalic,
    FontFamilyBoldItalic,
    FontStyle,
    FontStyleBold,
    FontStyleItalic,
    FontStyleBoldItalic,
    FontSyntheticStyle,
    FontFeature,
    FontVariation,
    FontVariationBold,
    FontVariationItalic,
    FontVariationBoldItalic,
    FontCodepointMap,
    ClipboardCodepointMap,
    FontThicken,
    FontThickenStrength,
    FontShapingBreak,
    AdjustCellWidth,
    AdjustCellHeight,
    AdjustFontBaseline,
    AdjustUnderlinePosition,
    AdjustUnderlineThickness,
    AdjustStrikethroughPosition,
    AdjustStrikethroughThickness,
    AdjustOverlinePosition,
    AdjustOverlineThickness,
    AdjustCursorThickness,
    AdjustCursorHeight,
    AdjustBoxThickness,
    AdjustIconHeight,
    GraphemeWidthMethod,
    FreetypeLoadFlags,
    Theme,
    Background,
    Foreground,
    BackgroundImage,
    BackgroundImageOpacity,
    BackgroundImagePosition,
    BackgroundImageFit,
    BackgroundImageRepeat,
    SelectionForeground,
    SelectionBackground,
    SelectionClearOnTyping,
    SelectionClearOnCopy,
    SelectionWordChars,
    MinimumContrast,
    Palette,
    PaletteGenerate,
    PaletteHarmonious,
    CursorColor,
    CursorOpacity,
    CursorStyle,
    CursorStyleBlink,
    CursorText,
    CursorClickToMove,
    MouseHideWhileTyping,
    ScrollToBottom,
    MouseShiftCapture,
    MouseReporting,
    MouseScrollMultiplier,
    BackgroundOpacity,
    BackgroundOpacityCells,
    BackgroundBlur,
    UnfocusedSplitOpacity,
    UnfocusedSplitFill,
    SplitDividerColor,
    SplitPreserveZoom,
    SearchForeground,
    SearchBackground,
    SearchSelectedForeground,
    SearchSelectedBackground,
    Command,
    InitialCommand,
    NotifyOnCommandFinish,
    NotifyOnCommandFinishAfter,
    Env,
    Input,
    WaitAfterCommand,
    AbnormalCommandExitRuntime,
    Scrollbar,
    Link,
    LinkUrl,
    LinkPreviews,
    Maximize,
    Fullscreen,
    Title,
    Class,
    X11InstanceName,
    WorkingDirectory,
    Keybind,
    KeyRemap,
    WindowPaddingX,
    WindowPaddingY,
    WindowPaddingBalance,
    WindowPaddingColor,
    WindowVsync,
    WindowInheritWorkingDirectory,
    TabInheritWorkingDirectory,
    SplitInheritWorkingDirectory,
    WindowInheritFontSize,
    WindowDecoration,
    WindowTitleFontFamily,
    WindowSubtitle,
    WindowTheme,
    WindowColorspace,
    WindowHeight,
    WindowWidth,
    WindowPositionX,
    WindowPositionY,
    WindowSaveState,
    WindowStepResize,
    WindowNewTabPosition,
    WindowShowTabBar,
    WindowTitlebarBackground,
    WindowTitlebarForeground,
    ResizeOverlay,
    ResizeOverlayPosition,
    ResizeOverlayDuration,
    FocusFollowsMouse,
    ClipboardRead,
    ClipboardWrite,
    ClipboardTrimTrailingSpaces,
    ClipboardPasteProtection,
    ClipboardPasteBracketedSafe,
    TitleReport,
    ImageStorageLimit,
    RightClickAction,
    MiddleClickAction,
    ClickRepeatInterval,
    ConfigFile,
    ConfigDefaultFiles,
    ConfirmCloseSurface,
    QuitAfterLastWindowClosed,
    QuitAfterLastWindowClosedDelay,
    InitialWindow,
    UndoTimeout,
    QuickTerminalPosition,
    QuickTerminalSize,
    GtkQuickTerminalLayer,
    GtkQuickTerminalNamespace,
    QuickTerminalScreen,
    QuickTerminalAnimationDuration,
    QuickTerminalSpaceBehavior,
    QuickTerminalKeyboardInteractivity,
    ShellIntegration,
    ShellIntegrationFeatures,
    CommandPaletteEntry,
    OscColorReportFormat,
    VtKamAllowed,
    CustomShader,
    CustomShaderAnimation,
    BellFeatures,
    BellAudioPath,
    BellAudioVolume,
    AppNotifications,
    MacosNonNativeFullscreen,
    MacosWindowButtons,
    MacosTitlebarStyle,
    MacosTitlebarProxyIcon,
    MacosDockDropBehavior,
    MacosOptionAsAlt,
    MacosWindowShadow,
    MacosHidden,
    MacosAutoSecureInput,
    MacosSecureInputIndication,
    MacosApplescript,
    MacosIcon,
    MacosCustomIcon,
    MacosIconFrame,
    MacosIconGhostColor,
    MacosIconScreenColor,
    MacosShortcuts,
    LinuxCgroupMemoryLimit,
    LinuxCgroupProcessesLimit,
    LinuxCgroupHardFail,
    GtkOpenglDebug,
    GtkSingleInstance,
    GtkTitlebar,
    GtkTabsLocation,
    GtkTitlebarHideWhenMaximized,
    GtkToolbarStyle,
    GtkTitlebarStyle,
    GtkWideTabs,
    GtkCustomCss,
    DesktopNotifications,
    ProgressStyle,
    BoldColor,
    FaintOpacity,
    Term,
    EnquiryResponse,
    AsyncBackend,
    AutoUpdate,
    AutoUpdateChannel,
}

/// PORTS: `pub const OptionalFileAction = enum { loaded, not_found, "error" };`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionalFileAction {
    Loaded,
    NotFound,
    Error,
}

/// `Config` is the main config struct. The fields below map directly to the CLI
/// flag names (hyphens → underscores in the Rust ident; the original reference
/// name is preserved in the comment for parse/format dispatch).
//
// PORT NOTE: per-field Pandoc doc comments are ported in the fill sub-batch.
pub struct Config {
    // language: ?[:0]const u8 = null,
    pub language: Option<Vec<u8>>,

    // @"font-family": RepeatableString = .{},
    pub font_family: RepeatableString,
    pub font_family_bold: RepeatableString,
    pub font_family_italic: RepeatableString,
    pub font_family_bold_italic: RepeatableString,

    // @"font-style": FontStyle = .{ .default = {} },
    pub font_style: FontStyle,
    pub font_style_bold: FontStyle,
    pub font_style_italic: FontStyle,
    pub font_style_bold_italic: FontStyle,

    // @"font-synthetic-style": FontSyntheticStyle = .{},
    pub font_synthetic_style: FontSyntheticStyle,

    // @"font-feature": RepeatableString = .{},
    pub font_feature: RepeatableString,

    // @"font-variation": RepeatableFontVariation = .{},
    pub font_variation: RepeatableFontVariation,
    pub font_variation_bold: RepeatableFontVariation,
    pub font_variation_italic: RepeatableFontVariation,
    pub font_variation_bold_italic: RepeatableFontVariation,

    // @"font-codepoint-map": RepeatableCodepointMap = .{},
    pub font_codepoint_map: RepeatableCodepointMap,

    // @"clipboard-codepoint-map": RepeatableClipboardCodepointMap = .{},
    pub clipboard_codepoint_map: RepeatableClipboardCodepointMap,

    // @"font-thicken": bool = false,
    pub font_thicken: bool,
    // @"font-thicken-strength": u8 = 255,
    pub font_thicken_strength: u8,

    // @"font-shaping-break": FontShapingBreak = .{},
    pub font_shaping_break: FontShapingBreak,

    // @"adjust-*": ?MetricModifier = null,
    pub adjust_cell_width: Option<MetricModifier>,
    pub adjust_cell_height: Option<MetricModifier>,
    pub adjust_font_baseline: Option<MetricModifier>,
    pub adjust_underline_position: Option<MetricModifier>,
    pub adjust_underline_thickness: Option<MetricModifier>,
    pub adjust_strikethrough_position: Option<MetricModifier>,
    pub adjust_strikethrough_thickness: Option<MetricModifier>,
    pub adjust_overline_position: Option<MetricModifier>,
    pub adjust_overline_thickness: Option<MetricModifier>,
    pub adjust_cursor_thickness: Option<MetricModifier>,
    pub adjust_cursor_height: Option<MetricModifier>,
    pub adjust_box_thickness: Option<MetricModifier>,
    pub adjust_icon_height: Option<MetricModifier>,

    // @"grapheme-width-method": GraphemeWidthMethod = .unicode,
    pub grapheme_width_method: GraphemeWidthMethod,

    // @"freetype-load-flags": FreetypeLoadFlags = .{},
    pub freetype_load_flags: FreetypeLoadFlags,

    // theme: ?Theme = null,
    pub theme: Option<Theme>,

    // background: Color = .{ .r = 0x28, .g = 0x2C, .b = 0x34 },
    pub background: Color,
    // foreground: Color = .{ .r = 0xFF, .g = 0xFF, .b = 0xFF },
    pub foreground: Color,

    // @"background-image": ?Path = null,
    pub background_image: Option<Path>,
    // @"background-image-opacity": f32 = 1.0,
    pub background_image_opacity: f32,
    // @"background-image-position": BackgroundImagePosition = .center,
    pub background_image_position: BackgroundImagePosition,
    // @"background-image-fit": BackgroundImageFit = .contain,
    pub background_image_fit: BackgroundImageFit,
    // @"background-image-repeat": bool = false,
    pub background_image_repeat: bool,

    // @"selection-foreground": ?TerminalColor = null,
    pub selection_foreground: Option<TerminalColor>,
    pub selection_background: Option<TerminalColor>,
    // @"selection-clear-on-typing": bool = true,
    pub selection_clear_on_typing: bool,
    // @"selection-clear-on-copy": bool = false,
    pub selection_clear_on_copy: bool,
    // @"selection-word-chars": SelectionWordChars = .{},
    pub selection_word_chars: SelectionWordChars,

    // @"minimum-contrast": f64 = 1,
    pub minimum_contrast: f64,

    // palette: Palette = .{},
    pub palette: Palette,
    // @"palette-generate": bool = false,
    pub palette_generate: bool,
    // @"palette-harmonious": bool = false,
    pub palette_harmonious: bool,

    // @"cursor-color": ?TerminalColor = null,
    pub cursor_color: Option<TerminalColor>,
    // @"cursor-opacity": f64 = 1.0,
    pub cursor_opacity: f64,
    // @"cursor-style": terminal.CursorStyle = .block,
    pub cursor_style: CursorStyle,
    // @"cursor-style-blink": ?bool = null,
    pub cursor_style_blink: Option<bool>,
    // @"cursor-text": ?TerminalColor = null,
    pub cursor_text: Option<TerminalColor>,
    // @"cursor-click-to-move": bool = true,
    pub cursor_click_to_move: bool,

    // @"mouse-hide-while-typing": bool = false,
    pub mouse_hide_while_typing: bool,
    // @"scroll-to-bottom": ScrollToBottom = .default,
    pub scroll_to_bottom: ScrollToBottom,
    // @"mouse-shift-capture": MouseShiftCapture = .false,
    pub mouse_shift_capture: MouseShiftCapture,
    // @"mouse-reporting": bool = true,
    pub mouse_reporting: bool,
    // @"mouse-scroll-multiplier": MouseScrollMultiplier = .default,
    pub mouse_scroll_multiplier: MouseScrollMultiplier,

    // @"background-opacity": f64 = 1.0,
    pub background_opacity: f64,
    // @"background-opacity-cells": bool = false,
    pub background_opacity_cells: bool,
    // @"background-blur": BackgroundBlur = .false,
    pub background_blur: BackgroundBlur,

    // @"unfocused-split-opacity": f64 = 0.7,
    pub unfocused_split_opacity: f64,
    // @"unfocused-split-fill": ?Color = null,
    pub unfocused_split_fill: Option<Color>,
    // @"split-divider-color": ?Color = null,
    pub split_divider_color: Option<Color>,
    // @"split-preserve-zoom": SplitPreserveZoom = .{},
    pub split_preserve_zoom: SplitPreserveZoom,

    // @"search-foreground": TerminalColor = .{ .color = .{ .r = 0, .g = 0, .b = 0 } },
    pub search_foreground: TerminalColor,
    // @"search-background": TerminalColor = .{ .color = .{ .r = 0xFF, .g = 0xE0, .b = 0x82 } },
    pub search_background: TerminalColor,
    // @"search-selected-foreground": TerminalColor = .{ .color = .{ .r = 0, .g = 0, .b = 0 } },
    pub search_selected_foreground: TerminalColor,
    // @"search-selected-background": TerminalColor = .{ .color = .{ .r = 0xF2, .g = 0xA5, .b = 0x7E } },
    pub search_selected_background: TerminalColor,

    // command: ?Command = null,
    pub command: Option<Command>,
    // @"initial-command": ?Command = null,
    pub initial_command: Option<Command>,

    // @"notify-on-command-finish": NotifyOnCommandFinish = .never,
    pub notify_on_command_finish: NotifyOnCommandFinish,
    // @"notify-on-command-finish-after": Duration = .{ .duration = 5 * std.time.ns_per_s },
    pub notify_on_command_finish_after: Duration,

    // env: RepeatableStringMap = .{},
    pub env: RepeatableStringMap,
    // input: RepeatableReadableIO = .{},
    pub input: RepeatableReadableIO,
    // @"wait-after-command": bool = false,
    pub wait_after_command: bool,
    // @"abnormal-command-exit-runtime": u32 = 250,
    pub abnormal_command_exit_runtime: u32,
    // scrollbar: Scrollbar = .system,
    pub scrollbar: Scrollbar,

    // link: RepeatableLink = .{},
    pub link: RepeatableLink,
    // @"link-url": bool = true,
    pub link_url: bool,
    // @"link-previews": LinkPreviews = .true,
    pub link_previews: LinkPreviews,

    // maximize: bool = false,
    pub maximize: bool,
    // fullscreen: Fullscreen = .false,
    pub fullscreen: Fullscreen,
    // title: ?[:0]const u8 = null,
    pub title: Option<Vec<u8>>,
    // class: ?[:0]const u8 = null,
    pub class: Option<Vec<u8>>,
    // @"x11-instance-name": ?[:0]const u8 = null,
    pub x11_instance_name: Option<Vec<u8>>,

    // @"working-directory": ?WorkingDirectory = null,
    pub working_directory: Option<WorkingDirectory>,

    // keybind: Keybinds = .{},
    pub keybind: Keybinds,
    // @"key-remap": KeyRemapSet = .empty,
    pub key_remap: KeyRemapSet,

    // @"window-padding-x": WindowPadding = .{ .top_left = 2, .bottom_right = 2 },
    pub window_padding_x: WindowPadding,
    // @"window-padding-y": WindowPadding = .{ .top_left = 2, .bottom_right = 2 },
    pub window_padding_y: WindowPadding,
    // @"window-padding-balance": WindowPaddingBalance = .false,
    pub window_padding_balance: WindowPaddingBalance,
    // @"window-padding-color": WindowPaddingColor = .background,
    pub window_padding_color: WindowPaddingColor,
    // @"window-vsync": bool = true,
    pub window_vsync: bool,
    // @"window-inherit-working-directory": bool = true,
    pub window_inherit_working_directory: bool,
    // @"tab-inherit-working-directory": bool = true,
    pub tab_inherit_working_directory: bool,
    // @"split-inherit-working-directory": bool = true,
    pub split_inherit_working_directory: bool,
    // @"window-inherit-font-size": bool = true,
    pub window_inherit_font_size: bool,
    // @"window-decoration": WindowDecoration = .auto,
    pub window_decoration: WindowDecoration,
    // @"window-title-font-family": ?[:0]const u8 = null,
    pub window_title_font_family: Option<Vec<u8>>,
    // @"window-subtitle": WindowSubtitle = .false,
    pub window_subtitle: WindowSubtitle,
    // @"window-theme": WindowTheme = .auto,
    pub window_theme: WindowTheme,
    // @"window-colorspace": WindowColorspace = .srgb,
    pub window_colorspace: WindowColorspace,
    // @"window-height": u32 = 0,
    pub window_height: u32,
    // @"window-width": u32 = 0,
    pub window_width: u32,
    // @"window-position-x": ?i16 = null,
    pub window_position_x: Option<i16>,
    // @"window-position-y": ?i16 = null,
    pub window_position_y: Option<i16>,
    // @"window-save-state": WindowSaveState = .default,
    pub window_save_state: WindowSaveState,
    // @"window-step-resize": bool = false,
    pub window_step_resize: bool,
    // @"window-new-tab-position": WindowNewTabPosition = .current,
    pub window_new_tab_position: WindowNewTabPosition,
    // @"window-show-tab-bar": WindowShowTabBar = .auto,
    pub window_show_tab_bar: WindowShowTabBar,
    // @"window-titlebar-background": ?Color = null,
    pub window_titlebar_background: Option<Color>,
    // @"window-titlebar-foreground": ?Color = null,
    pub window_titlebar_foreground: Option<Color>,

    // @"resize-overlay": ResizeOverlay = .@"after-first",
    pub resize_overlay: ResizeOverlay,
    // @"resize-overlay-position": ResizeOverlayPosition = .center,
    pub resize_overlay_position: ResizeOverlayPosition,
    // @"resize-overlay-duration": Duration = .{ .duration = 750 * std.time.ns_per_ms },
    pub resize_overlay_duration: Duration,

    // @"focus-follows-mouse": bool = false,
    pub focus_follows_mouse: bool,

    // @"clipboard-read": ClipboardAccess = .ask,
    pub clipboard_read: ClipboardAccess,
    // @"clipboard-write": ClipboardAccess = .allow,
    pub clipboard_write: ClipboardAccess,
    // @"clipboard-trim-trailing-spaces": bool = true,
    pub clipboard_trim_trailing_spaces: bool,
    // @"clipboard-paste-protection": bool = true,
    pub clipboard_paste_protection: bool,
    // @"clipboard-paste-bracketed-safe": bool = true,
    pub clipboard_paste_bracketed_safe: bool,

    // @"title-report": bool = false,
    pub title_report: bool,
    // @"image-storage-limit": u32 = 320 * 1000 * 1000,
    pub image_storage_limit: u32,

    // @"right-click-action": RightClickAction = .@"context-menu",
    pub right_click_action: RightClickAction,
    // @"middle-click-action": MiddleClickAction = .@"primary-paste",
    pub middle_click_action: MiddleClickAction,
    // @"click-repeat-interval": u32 = 0,
    pub click_repeat_interval: u32,

    // @"config-file": RepeatablePath = .{},
    pub config_file: RepeatablePath,
    // @"config-default-files": bool = true,
    pub config_default_files: bool,

    // @"confirm-close-surface": ConfirmCloseSurface = .true,
    pub confirm_close_surface: ConfirmCloseSurface,
    // @"quit-after-last-window-closed": bool = builtin.os.tag == .linux,
    pub quit_after_last_window_closed: bool,
    // @"quit-after-last-window-closed-delay": ?Duration = null,
    pub quit_after_last_window_closed_delay: Option<Duration>,
    // @"initial-window": bool = true,
    pub initial_window: bool,

    // @"undo-timeout": Duration = .{ .duration = 5 * std.time.ns_per_s },
    pub undo_timeout: Duration,

    // @"quick-terminal-position": QuickTerminalPosition = .top,
    pub quick_terminal_position: QuickTerminalPosition,
    // @"quick-terminal-size": QuickTerminalSize = .{},
    pub quick_terminal_size: QuickTerminalSize,
    // @"gtk-quick-terminal-layer": QuickTerminalLayer = .top,
    pub gtk_quick_terminal_layer: QuickTerminalLayer,
    // @"gtk-quick-terminal-namespace": [:0]const u8 = "ghostty-quick-terminal",
    pub gtk_quick_terminal_namespace: Vec<u8>,
    // @"quick-terminal-screen": QuickTerminalScreen = .main,
    pub quick_terminal_screen: QuickTerminalScreen,
    // @"quick-terminal-animation-duration": f64 = 0.2,
    pub quick_terminal_animation_duration: f64,
    // @"quick-terminal-space-behavior": QuickTerminalSpaceBehavior = .move,
    pub quick_terminal_space_behavior: QuickTerminalSpaceBehavior,
    // @"quick-terminal-keyboard-interactivity": QuickTerminalKeyboardInteractivity = .@"on-demand",
    pub quick_terminal_keyboard_interactivity: QuickTerminalKeyboardInteractivity,

    // @"shell-integration": ShellIntegration = .detect,
    pub shell_integration: ShellIntegration,
    // @"shell-integration-features": ShellIntegrationFeatures = .{},
    pub shell_integration_features: ShellIntegrationFeatures,

    // @"command-palette-entry": RepeatableCommand = .{},
    pub command_palette_entry: RepeatableCommand,

    // @"osc-color-report-format": OSCColorReportFormat = .@"16-bit",
    pub osc_color_report_format: OSCColorReportFormat,
    // @"vt-kam-allowed": bool = false,
    pub vt_kam_allowed: bool,

    // @"custom-shader": RepeatablePath = .{},
    pub custom_shader: RepeatablePath,
    // @"custom-shader-animation": CustomShaderAnimation = .true,
    pub custom_shader_animation: CustomShaderAnimation,

    // @"bell-features": BellFeatures = .{},
    pub bell_features: BellFeatures,
    // @"bell-audio-path": ?Path = null,
    pub bell_audio_path: Option<Path>,
    // @"bell-audio-volume": f64 = 0.5,
    pub bell_audio_volume: f64,

    // @"app-notifications": AppNotifications = .{},
    pub app_notifications: AppNotifications,

    // @"macos-non-native-fullscreen": NonNativeFullscreen = .false,
    pub macos_non_native_fullscreen: NonNativeFullscreen,
    // @"macos-window-buttons": MacWindowButtons = .visible,
    pub macos_window_buttons: MacWindowButtons,
    // @"macos-titlebar-style": MacTitlebarStyle = .transparent,
    pub macos_titlebar_style: MacTitlebarStyle,
    // @"macos-titlebar-proxy-icon": MacTitlebarProxyIcon = .visible,
    pub macos_titlebar_proxy_icon: MacTitlebarProxyIcon,
    // @"macos-dock-drop-behavior": MacOSDockDropBehavior = .@"new-tab",
    pub macos_dock_drop_behavior: MacOSDockDropBehavior,
    // @"macos-option-as-alt": ?inputpkg.OptionAsAlt = null,
    pub macos_option_as_alt: Option<rustty_input::config::OptionAsAlt>,
    // @"macos-window-shadow": bool = true,
    pub macos_window_shadow: bool,
    // @"macos-hidden": MacHidden = .never,
    pub macos_hidden: MacHidden,
    // @"macos-auto-secure-input": bool = true,
    pub macos_auto_secure_input: bool,
    // @"macos-secure-input-indication": bool = true,
    pub macos_secure_input_indication: bool,
    // @"macos-applescript": bool = true,
    pub macos_applescript: bool,
    // @"macos-icon": MacAppIcon = .official,
    pub macos_icon: MacAppIcon,
    // @"macos-custom-icon": ?[:0]const u8 = null,
    pub macos_custom_icon: Option<Vec<u8>>,
    // @"macos-icon-frame": MacAppIconFrame = .aluminum,
    pub macos_icon_frame: MacAppIconFrame,
    // @"macos-icon-ghost-color": ?Color = null,
    pub macos_icon_ghost_color: Option<Color>,
    // @"macos-icon-screen-color": ?ColorList = null,
    pub macos_icon_screen_color: Option<ColorList>,
    // @"macos-shortcuts": MacShortcuts = .ask,
    pub macos_shortcuts: MacShortcuts,

    // @"linux-cgroup-memory-limit": ?u64 = null,
    pub linux_cgroup_memory_limit: Option<u64>,
    // @"linux-cgroup-processes-limit": ?u64 = null,
    pub linux_cgroup_processes_limit: Option<u64>,
    // @"linux-cgroup-hard-fail": bool = false,
    pub linux_cgroup_hard_fail: bool,

    // @"gtk-opengl-debug": bool = builtin.mode == .Debug,
    pub gtk_opengl_debug: bool,
    // @"gtk-single-instance": GtkSingleInstance = .default,
    pub gtk_single_instance: GtkSingleInstance,
    // @"gtk-titlebar": bool = true,
    pub gtk_titlebar: bool,
    // @"gtk-tabs-location": GtkTabsLocation = .top,
    pub gtk_tabs_location: GtkTabsLocation,
    // @"gtk-titlebar-hide-when-maximized": bool = false,
    pub gtk_titlebar_hide_when_maximized: bool,
    // @"gtk-toolbar-style": GtkToolbarStyle = .raised,
    pub gtk_toolbar_style: GtkToolbarStyle,
    // @"gtk-titlebar-style": GtkTitlebarStyle = .native,
    pub gtk_titlebar_style: GtkTitlebarStyle,
    // @"gtk-wide-tabs": bool = true,
    pub gtk_wide_tabs: bool,
    // @"gtk-custom-css": RepeatablePath = .{},
    pub gtk_custom_css: RepeatablePath,

    // @"desktop-notifications": bool = true,
    pub desktop_notifications: bool,
    // @"progress-style": bool = true,
    pub progress_style: bool,

    // @"bold-color": ?BoldColor = null,
    pub bold_color: Option<BoldColor>,
    // @"faint-opacity": f64 = 0.5,
    pub faint_opacity: f64,

    // term: []const u8 = "xterm-ghostty",
    pub term: Vec<u8>,
    // @"enquiry-response": []const u8 = "",
    pub enquiry_response: Vec<u8>,

    // @"async-backend": AsyncBackend = .auto,
    pub async_backend: AsyncBackend,
    // @"auto-update": ?AutoUpdate = null,
    pub auto_update: Option<AutoUpdate>,
    // @"auto-update-channel": ?build_config.ReleaseChannel = null,
    pub auto_update_channel: Option<rustty_build_config::ReleaseChannel>,

    // ---- Internal (`_`-prefixed) fields. Not copied, not part of `Key`. ----

    // _arena: ?ArenaAllocator = null,
    //
    // PORT NOTE: the reference threads an `ArenaAllocator` for all parsed
    // allocations. The Rust port hands out owned allocations (each parsed value
    // owns its bytes); the arena's observable role is "allocations available".
    // Modeled as a bool flag for the `Target::arena_available` query.
    pub _arena: bool,
    // _diagnostics: cli.DiagnosticList = .{},
    pub _diagnostics: DiagnosticList,
    // _conditional_state: conditional.State = .{},
    pub _conditional_state: conditional::State,
    // _conditional_set: std.EnumSet(conditional.Key) = .{},
    //
    // PORT NOTE: `std.EnumSet(conditional.Key)` — the set of conditional keys
    // present. Modeled as a small set over `conditional::Key`.
    pub _conditional_set: std::collections::HashSet<conditional::Key>,
    // _replay_steps: std.ArrayListUnmanaged(Replay.Step) = .{},
    pub _replay_steps: Vec<ReplayStep>,
    // @"_xdg-terminal-exec": bool = false,
    pub _xdg_terminal_exec: bool,
}

impl Default for Config {
    /// PORTS: the reference field defaults (each `= <expr>` above). This is the
    /// data half of `default(alloc)` (which additionally creates the `_arena`).
    fn default() -> Self {
        // std.time.ns_per_s / ns_per_ms (nanoseconds).
        const NS_PER_S: u64 = 1_000_000_000;
        const NS_PER_MS: u64 = 1_000_000;
        Self {
            language: None,
            font_family: RepeatableString::default(),
            font_family_bold: RepeatableString::default(),
            font_family_italic: RepeatableString::default(),
            font_family_bold_italic: RepeatableString::default(),
            font_style: FontStyle::Default,
            font_style_bold: FontStyle::Default,
            font_style_italic: FontStyle::Default,
            font_style_bold_italic: FontStyle::Default,
            font_synthetic_style: FontSyntheticStyle::default(),
            font_feature: RepeatableString::default(),
            font_variation: RepeatableFontVariation::default(),
            font_variation_bold: RepeatableFontVariation::default(),
            font_variation_italic: RepeatableFontVariation::default(),
            font_variation_bold_italic: RepeatableFontVariation::default(),
            font_codepoint_map: RepeatableCodepointMap::default(),
            clipboard_codepoint_map: RepeatableClipboardCodepointMap::default(),
            font_thicken: false,
            font_thicken_strength: 255,
            font_shaping_break: FontShapingBreak::default(),
            adjust_cell_width: None,
            adjust_cell_height: None,
            adjust_font_baseline: None,
            adjust_underline_position: None,
            adjust_underline_thickness: None,
            adjust_strikethrough_position: None,
            adjust_strikethrough_thickness: None,
            adjust_overline_position: None,
            adjust_overline_thickness: None,
            adjust_cursor_thickness: None,
            adjust_cursor_height: None,
            adjust_box_thickness: None,
            adjust_icon_height: None,
            grapheme_width_method: GraphemeWidthMethod::Unicode,
            freetype_load_flags: FreetypeLoadFlags::default(),
            theme: None,
            background: Color {
                r: 0x28,
                g: 0x2C,
                b: 0x34,
            },
            foreground: Color {
                r: 0xFF,
                g: 0xFF,
                b: 0xFF,
            },
            background_image: None,
            background_image_opacity: 1.0,
            background_image_position: BackgroundImagePosition::Center,
            background_image_fit: BackgroundImageFit::Contain,
            background_image_repeat: false,
            selection_foreground: None,
            selection_background: None,
            selection_clear_on_typing: true,
            selection_clear_on_copy: false,
            selection_word_chars: SelectionWordChars::default(),
            minimum_contrast: 1.0,
            palette: Palette::default(),
            palette_generate: false,
            palette_harmonious: false,
            cursor_color: None,
            cursor_opacity: 1.0,
            cursor_style: CursorStyle::Block,
            cursor_style_blink: None,
            cursor_text: None,
            cursor_click_to_move: true,
            mouse_hide_while_typing: false,
            scroll_to_bottom: ScrollToBottom::default(),
            mouse_shift_capture: MouseShiftCapture::False,
            mouse_reporting: true,
            mouse_scroll_multiplier: MouseScrollMultiplier::default(),
            background_opacity: 1.0,
            background_opacity_cells: false,
            background_blur: BackgroundBlur::False,
            unfocused_split_opacity: 0.7,
            unfocused_split_fill: None,
            split_divider_color: None,
            split_preserve_zoom: SplitPreserveZoom::default(),
            search_foreground: TerminalColor::Color(Color { r: 0, g: 0, b: 0 }),
            search_background: TerminalColor::Color(Color {
                r: 0xFF,
                g: 0xE0,
                b: 0x82,
            }),
            search_selected_foreground: TerminalColor::Color(Color { r: 0, g: 0, b: 0 }),
            search_selected_background: TerminalColor::Color(Color {
                r: 0xF2,
                g: 0xA5,
                b: 0x7E,
            }),
            command: None,
            initial_command: None,
            notify_on_command_finish: NotifyOnCommandFinish::Never,
            notify_on_command_finish_after: Duration {
                duration: 5 * NS_PER_S,
            },
            env: RepeatableStringMap::default(),
            input: RepeatableReadableIO::default(),
            wait_after_command: false,
            abnormal_command_exit_runtime: 250,
            scrollbar: Scrollbar::System,
            link: RepeatableLink::default(),
            link_url: true,
            link_previews: LinkPreviews::True,
            maximize: false,
            fullscreen: Fullscreen::False,
            title: None,
            class: None,
            x11_instance_name: None,
            working_directory: None,
            keybind: Keybinds::default(),
            key_remap: KeyRemapSet::empty(),
            window_padding_x: WindowPadding {
                top_left: 2,
                bottom_right: 2,
            },
            window_padding_y: WindowPadding {
                top_left: 2,
                bottom_right: 2,
            },
            window_padding_balance: WindowPaddingBalance::False,
            window_padding_color: WindowPaddingColor::Background,
            window_vsync: true,
            window_inherit_working_directory: true,
            tab_inherit_working_directory: true,
            split_inherit_working_directory: true,
            window_inherit_font_size: true,
            window_decoration: WindowDecoration::Auto,
            window_title_font_family: None,
            window_subtitle: WindowSubtitle::False,
            window_theme: WindowTheme::Auto,
            window_colorspace: WindowColorspace::Srgb,
            window_height: 0,
            window_width: 0,
            window_position_x: None,
            window_position_y: None,
            window_save_state: WindowSaveState::Default,
            window_step_resize: false,
            window_new_tab_position: WindowNewTabPosition::Current,
            window_show_tab_bar: WindowShowTabBar::Auto,
            window_titlebar_background: None,
            window_titlebar_foreground: None,
            resize_overlay: ResizeOverlay::AfterFirst,
            resize_overlay_position: ResizeOverlayPosition::Center,
            resize_overlay_duration: Duration {
                duration: 750 * NS_PER_MS,
            },
            focus_follows_mouse: false,
            clipboard_read: ClipboardAccess::Ask,
            clipboard_write: ClipboardAccess::Allow,
            clipboard_trim_trailing_spaces: true,
            clipboard_paste_protection: true,
            clipboard_paste_bracketed_safe: true,
            title_report: false,
            image_storage_limit: 320 * 1000 * 1000,
            right_click_action: RightClickAction::ContextMenu,
            middle_click_action: MiddleClickAction::PrimaryPaste,
            click_repeat_interval: 0,
            config_file: RepeatablePath::default(),
            config_default_files: true,
            confirm_close_surface: ConfirmCloseSurface::True,
            // builtin.os.tag == .linux
            quit_after_last_window_closed: cfg!(target_os = "linux"),
            quit_after_last_window_closed_delay: None,
            initial_window: true,
            undo_timeout: Duration {
                duration: 5 * NS_PER_S,
            },
            quick_terminal_position: QuickTerminalPosition::Top,
            quick_terminal_size: QuickTerminalSize::default(),
            gtk_quick_terminal_layer: QuickTerminalLayer::Top,
            gtk_quick_terminal_namespace: b"ghostty-quick-terminal".to_vec(),
            quick_terminal_screen: QuickTerminalScreen::Main,
            quick_terminal_animation_duration: 0.2,
            quick_terminal_space_behavior: QuickTerminalSpaceBehavior::Move,
            quick_terminal_keyboard_interactivity: QuickTerminalKeyboardInteractivity::OnDemand,
            shell_integration: ShellIntegration::Detect,
            shell_integration_features: ShellIntegrationFeatures::default(),
            command_palette_entry: RepeatableCommand::default(),
            osc_color_report_format: OSCColorReportFormat::Bit16,
            vt_kam_allowed: false,
            custom_shader: RepeatablePath::default(),
            custom_shader_animation: CustomShaderAnimation::True,
            bell_features: BellFeatures::default(),
            bell_audio_path: None,
            bell_audio_volume: 0.5,
            app_notifications: AppNotifications::default(),
            macos_non_native_fullscreen: NonNativeFullscreen::False,
            macos_window_buttons: MacWindowButtons::Visible,
            macos_titlebar_style: MacTitlebarStyle::Transparent,
            macos_titlebar_proxy_icon: MacTitlebarProxyIcon::Visible,
            macos_dock_drop_behavior: MacOSDockDropBehavior::NewTab,
            macos_option_as_alt: None,
            macos_window_shadow: true,
            macos_hidden: MacHidden::Never,
            macos_auto_secure_input: true,
            macos_secure_input_indication: true,
            macos_applescript: true,
            macos_icon: MacAppIcon::Official,
            macos_custom_icon: None,
            macos_icon_frame: MacAppIconFrame::Aluminum,
            macos_icon_ghost_color: None,
            macos_icon_screen_color: None,
            macos_shortcuts: MacShortcuts::Ask,
            linux_cgroup_memory_limit: None,
            linux_cgroup_processes_limit: None,
            linux_cgroup_hard_fail: false,
            // builtin.mode == .Debug
            gtk_opengl_debug: cfg!(debug_assertions),
            gtk_single_instance: GtkSingleInstance::DEFAULT,
            gtk_titlebar: true,
            gtk_tabs_location: GtkTabsLocation::Top,
            gtk_titlebar_hide_when_maximized: false,
            gtk_toolbar_style: GtkToolbarStyle::Raised,
            gtk_titlebar_style: GtkTitlebarStyle::Native,
            gtk_wide_tabs: true,
            gtk_custom_css: RepeatablePath::default(),
            desktop_notifications: true,
            progress_style: true,
            bold_color: None,
            faint_opacity: 0.5,
            term: b"xterm-ghostty".to_vec(),
            enquiry_response: b"".to_vec(),
            async_backend: AsyncBackend::Auto,
            auto_update: None,
            auto_update_channel: None,
            _arena: false,
            _diagnostics: DiagnosticList::default(),
            _conditional_state: conditional::State::default(),
            _conditional_set: std::collections::HashSet::new(),
            _replay_steps: Vec::new(),
            _xdg_terminal_exec: false,
        }
    }
}

// ===========================================================================
// Replay (the `_replay_steps` element type) + ChangeIterator.
// ===========================================================================

/// PORTS: `Replay.Step` (Config.zig:5115) — a replayed parse step.
// const Step = union(enum) { arg, expand, conditional_arg, diagnostic, "-e" };
#[derive(Debug, Clone)]
pub enum ReplayStep {
    /// An argument to parse as if it came from the CLI or file.
    Arg(Vec<u8>),
    /// A base path to expand relative paths against.
    Expand(Vec<u8>),
    /// A conditional argument. This arg is parsed only if all
    /// conditions match (an "AND"). An "OR" can be achieved by
    /// having multiple conditional arg entries.
    ConditionalArg {
        conditions: Vec<Conditional>,
        arg: Vec<u8>,
    },
    /// A diagnostic to be added to the new configuration when
    /// replayed. This should only be used for diagnostics that won't
    /// be reproduced during playback. For example, `config-file`
    /// errors are not reloaded so they should be added here.
    ///
    /// Diagnostics cannot be conditional. They are always present
    /// even if the conditionals don't match. This helps users find
    /// errors in their configuration.
    Diagnostic(rustty_args::diagnostics::Diagnostic),
    /// The start of a "-e" argument. This marks the end of
    /// traditional configuration and the beginning of the
    /// "-e" initial command magic. This is separate from "arg"
    /// because there are some behaviors unique to this (i.e.
    /// we want to keep this at the end for config-file).
    ///
    /// Note: when "-e" is used, ONLY this is present and
    /// not an additional "arg" with "-e" value.
    DashE,
}

impl ReplayStep {
    // fn clone(self: Step, alloc: Allocator) Allocator.Error!Step
    pub fn clone_in(&self) -> ReplayStep {
        todo!("port Replay.Step.clone")
    }
}

/// PORTS: `pub const ChangeIterator` (Config.zig:5056) — iterates `Key`s whose
/// value differs between `old` and `new`.
pub struct ChangeIterator<'a> {
    pub old: &'a Config,
    pub new: &'a Config,
    pub i: usize,
}

impl<'a> ChangeIterator<'a> {
    // pub fn next(self: *ChangeIterator) ?Key
    //
    // PORT NOTE: the reference exposes a `next` method (not a Zig std iterator);
    // we keep the same inherent-method name for call-site parity rather than
    // implementing `std::iter::Iterator`.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<Key> {
        todo!("port ChangeIterator.next")
    }
}

// ===========================================================================
// Config pass method signatures (the four comptime passes + lifecycle).
// ===========================================================================

impl Config {
    /// PORTS: `pub fn default(alloc_gpa) Allocator.Error!Config` — the default
    /// config with an initialized arena. (Data half lives in `Default::default`.)
    pub fn default_config() -> Config {
        todo!("port Config.default")
    }

    /// PORTS: `pub fn load(alloc_gpa) !Config`.
    pub fn load() -> Result<Config, ParseError> {
        todo!("port Config.load")
    }

    /// PORTS: `pub fn deinit(self: *Config) void`.
    pub fn deinit(&mut self) {
        todo!("port Config.deinit")
    }

    /// PORTS: `pub fn loadIter(self, alloc, iter) !void`.
    pub fn load_iter<I: Iterator<Item = Vec<u8>>>(&mut self, _iter: I) -> Result<(), ParseError> {
        todo!("port Config.loadIter")
    }

    /// PORTS: `pub fn loadFile(self, alloc, path) !void`.
    pub fn load_file(&mut self, _path: &[u8]) -> Result<(), ParseError> {
        todo!("port Config.loadFile")
    }

    /// PORTS: `pub fn finalize(self: *Config) !void`.
    pub fn finalize(&mut self) -> Result<(), ParseError> {
        todo!("port Config.finalize")
    }

    /// PORTS: `pub fn parseManuallyHook(self, alloc, arg, iter) !bool` — the
    /// `@hasDecl(T, "parseManuallyHook")` entry point used by the args parser.
    pub fn parse_manually_hook(&mut self, _arg: &[u8]) -> Result<bool, ParseError> {
        todo!("port Config.parseManuallyHook")
    }

    /// PORTS: `pub fn cloneEmpty(self, alloc_gpa) Allocator.Error!Config`.
    pub fn clone_empty(&self) -> Config {
        todo!("port Config.cloneEmpty")
    }

    /// PORTS: `pub fn clone(self, alloc_gpa) Allocator.Error!Config`.
    pub fn clone_config(&self) -> Config {
        todo!("port Config.clone")
    }

    /// PORTS: `pub fn changed(self, new, comptime key: Key) bool` — true if the
    /// value for `key` differs between `self` and `new`. The comptime `key` is a
    /// runtime `Key` here; dispatch is per-variant (EQUAL/DIFF pass).
    pub fn changed(&self, _new: &Config, _key: Key) -> bool {
        todo!("port Config.changed")
    }

    /// PORTS: `pub fn changeIterator(self, new) ChangeIterator`.
    pub fn change_iterator<'a>(&'a self, new: &'a Config) -> ChangeIterator<'a> {
        ChangeIterator {
            old: self,
            new,
            i: 0,
        }
    }
}

// ===========================================================================
// Target impl skeleton — the PARSE pass per-field dispatch (Decision B).
// ===========================================================================

impl Target for Config {
    // @hasField(Config, "_arena")
    fn arena_available(&self) -> bool {
        self._arena
    }

    // canTrackDiags(Config) == @hasField(Config, "_diagnostics")
    fn diagnostics_mut(&mut self) -> Option<&mut DiagnosticList> {
        Some(&mut self._diagnostics)
    }

    /// PORTS: the PARSE pass — `parseIntoField(Config, alloc, dst, key, value)`.
    /// This performs the per-field name+type dispatch that the reference does
    /// with `inline for (info.@"struct".fields)`; each field's arm uses the
    /// `parse_field_*` helpers in rustty-args so it is assertion-for-assertion
    /// identical to the reference. Filled per-field in the PARSE fill sub-batch.
    fn parse_into_field(&mut self, _key: &[u8], _value: Option<&[u8]>) -> Result<(), ParseError> {
        todo!("port Config PARSE pass (parseIntoField per-field dispatch)")
    }

    // @hasDecl(Config, "parseManuallyHook")
    fn parse_manually_hook(&mut self, _arg: &[u8]) -> Option<Result<bool, ParseError>> {
        Some(Config::parse_manually_hook(self, _arg))
    }
}
