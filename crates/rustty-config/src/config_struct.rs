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

use rustty_args::args::{
    parse_field_bool, parse_field_enum, parse_field_float, parse_field_int,
    parse_field_sentinel_string, parse_field_string, Error, ParseError, Target,
};
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
    FontSize,
    FontVariation,
    FontVariationBold,
    FontVariationItalic,
    FontVariationBoldItalic,
    FontCodepointMap,
    ClipboardCodepointMap,
    FontThicken,
    FontThickenStrength,
    FontShapingBreak,
    AlphaBlending,
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
    NotifyOnCommandFinishAction,
    NotifyOnCommandFinishAfter,
    Env,
    Input,
    WaitAfterCommand,
    AbnormalCommandExitRuntime,
    ScrollbackLimit,
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
    CopyOnSelect,
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
    QuickTerminalAutohide,
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
    LinuxCgroup,
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

    // @"font-size": f32 = switch (builtin.os.tag) { .macos => 13, else => 12 },
    pub font_size: f32,

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

    // @"alpha-blending": AlphaBlending =
    //     if (builtin.os.tag == .macos) .native else .@"linear-corrected",
    pub alpha_blending: AlphaBlending,

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
    // @"notify-on-command-finish-action": NotifyOnCommandFinishAction = .{ .bell = true, .notify = false },
    pub notify_on_command_finish_action: NotifyOnCommandFinishAction,
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
    // @"scrollback-limit": usize = 10_000_000, // 10MB
    pub scrollback_limit: usize,
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

    // @"copy-on-select": CopyOnSelect = switch (builtin.os.tag) {
    //     .linux => .true, .macos => .true, else => .false,
    // },
    pub copy_on_select: CopyOnSelect,

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
    // @"quick-terminal-autohide": bool = switch (builtin.os.tag) {
    //     .linux => false, .macos => true, else => false,
    // },
    pub quick_terminal_autohide: bool,
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

    // @"linux-cgroup": LinuxCgroup = if (builtin.os.tag == .linux)
    //     .@"single-instance" else .never,
    pub linux_cgroup: LinuxCgroup,

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
            // switch (builtin.os.tag) { .macos => 13, else => 12 }
            font_size: if cfg!(target_os = "macos") {
                13.0
            } else {
                12.0
            },
            font_variation: RepeatableFontVariation::default(),
            font_variation_bold: RepeatableFontVariation::default(),
            font_variation_italic: RepeatableFontVariation::default(),
            font_variation_bold_italic: RepeatableFontVariation::default(),
            font_codepoint_map: RepeatableCodepointMap::default(),
            clipboard_codepoint_map: RepeatableClipboardCodepointMap::default(),
            font_thicken: false,
            font_thicken_strength: 255,
            font_shaping_break: FontShapingBreak::default(),
            // if (builtin.os.tag == .macos) .native else .@"linear-corrected"
            alpha_blending: if cfg!(target_os = "macos") {
                AlphaBlending::Native
            } else {
                AlphaBlending::LinearCorrected
            },
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
            // .{ .bell = true, .notify = false }
            notify_on_command_finish_action: NotifyOnCommandFinishAction::default(),
            notify_on_command_finish_after: Duration {
                duration: 5 * NS_PER_S,
            },
            env: RepeatableStringMap::default(),
            input: RepeatableReadableIO::default(),
            wait_after_command: false,
            abnormal_command_exit_runtime: 250,
            // 10_000_000 (10MB)
            scrollback_limit: 10_000_000,
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
            // switch (builtin.os.tag) { .linux => .true, .macos => .true, else => .false }
            copy_on_select: if cfg!(target_os = "linux") || cfg!(target_os = "macos") {
                CopyOnSelect::True
            } else {
                CopyOnSelect::False
            },
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
            // switch (builtin.os.tag) { .linux => false, .macos => true, else => false }
            quick_terminal_autohide: cfg!(target_os = "macos"),
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
            // if (builtin.os.tag == .linux) .@"single-instance" else .never
            linux_cgroup: if cfg!(target_os = "linux") {
                LinuxCgroup::SingleInstance
            } else {
                LinuxCgroup::Never
            },
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
// Bare-enum name maps for the EXTERNAL enums used by Config fields.
//
// `CursorStyle` (terminal `cursor.Style`), `inputpkg.OptionAsAlt`, and
// `build_config.ReleaseChannel` are bare enums parsed by `parseIntoField` via
// `std.meta.stringToEnum`. They live in their own crates (rustty-vt /
// rustty-input / rustty-build-config) and the value-type `bare_enum_names!`
// macro is local to `value_types`, so the `stringToEnum` name maps are provided
// here over their Zig variant-name literals (none are hyphenated).
// ===========================================================================

/// `std.meta.stringToEnum(terminal.CursorStyle, input)`.
// pub const Style = enum { bar, block, underline, block_hollow };
fn cursor_style_string_to_enum(input: &[u8]) -> Option<CursorStyle> {
    match input {
        b"bar" => Some(CursorStyle::Bar),
        b"block" => Some(CursorStyle::Block),
        b"underline" => Some(CursorStyle::Underline),
        b"block_hollow" => Some(CursorStyle::BlockHollow),
        _ => None,
    }
}

/// `std.meta.stringToEnum(inputpkg.OptionAsAlt, input)`.
// pub const OptionAsAlt = enum(c_int) { false, true, left, right };
fn option_as_alt_string_to_enum(input: &[u8]) -> Option<rustty_input::config::OptionAsAlt> {
    use rustty_input::config::OptionAsAlt;
    match input {
        b"false" => Some(OptionAsAlt::False),
        b"true" => Some(OptionAsAlt::True),
        b"left" => Some(OptionAsAlt::Left),
        b"right" => Some(OptionAsAlt::Right),
        _ => None,
    }
}

/// `std.meta.stringToEnum(build_config.ReleaseChannel, input)`.
// pub const ReleaseChannel = enum { tip, stable };
fn release_channel_string_to_enum(input: &[u8]) -> Option<rustty_build_config::ReleaseChannel> {
    use rustty_build_config::ReleaseChannel;
    match input {
        b"tip" => Some(ReleaseChannel::Tip),
        b"stable" => Some(ReleaseChannel::Stable),
        _ => None,
    }
}

/// `std.meta.stringToEnum(renderer.size.PaddingBalance, input)`.
// pub const PaddingBalance = enum { false, true, equal };
fn window_padding_balance_string_to_enum(input: &[u8]) -> Option<WindowPaddingBalance> {
    match input {
        b"false" => Some(WindowPaddingBalance::False),
        b"true" => Some(WindowPaddingBalance::True),
        b"equal" => Some(WindowPaddingBalance::Equal),
        _ => None,
    }
}

/// Maps `path::ParseError` (`{ValueRequired}`) into the args error union, as the
/// reference propagates `Path.parseCLI`'s error.
fn path_parse_err(e: crate::path::ParseError) -> ParseError {
    match e {
        crate::path::ParseError::ValueRequired => Error::ValueRequired,
    }
}

/// Maps `command::ParseError` (`{ValueRequired}`) into the args error union, as
/// the reference propagates `Command.parseCLI`'s error.
fn command_parse_err(e: crate::command::ParseError) -> ParseError {
    match e {
        crate::command::ParseError::ValueRequired => Error::ValueRequired,
    }
}

/// Maps `repeatable_string_map::ParseError` (`{ValueRequired}`) into the args
/// error union, as the reference propagates `RepeatableStringMap.parseCLI`.
fn rsm_parse_err(e: crate::repeatable_string_map::ParseError) -> ParseError {
    match e {
        crate::repeatable_string_map::ParseError::ValueRequired => Error::ValueRequired,
    }
}

/// Maps `io::ParseError` (`{ValueRequired, InvalidString}`) into the args error
/// union, as the reference propagates `RepeatableReadableIO.parseCLI`. The
/// reference's `InvalidString` is a value-parse failure (`InvalidValue`).
fn io_parse_err(e: crate::io::ParseError) -> ParseError {
    match e {
        crate::io::ParseError::ValueRequired => Error::ValueRequired,
        crate::io::ParseError::InvalidString => Error::InvalidValue,
    }
}

// ===========================================================================
// Target impl — the PARSE pass per-field dispatch (Decision B).
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
    /// identical to the reference.
    ///
    /// The reference logic, reproduced uniformly per arm:
    ///   - underscore-prefixed fields never match (they are simply absent from
    ///     the dispatch below; an unmatched `key` falls through to `InvalidField`).
    ///   - empty-string `value` resets the field to its default (or, for the two
    ///     types with an `init` decl — `Keybinds`/`RepeatableCommand` — calls
    ///     `init()`); this is the `if (value) |v| default: { if (v.len != 0)
    ///     break :default; ... }` block.
    ///   - a non-empty (or absent) `value` is parsed by the field's `parseCLI`
    ///     (if it has one) or by the type-`switch` "magic" (`stringToEnum` for
    ///     bare enums, `parseInt`/`parseFloat`/`parseBool`/string-dup for
    ///     primitives).
    //
    // PORT NOTE: in Zig this is one comptime `inline for` over the fields; in Rust
    // (no struct reflection) it is an explicit `match key` with one arm per
    // non-`_` field. The hyphenated Zig field names are the match patterns (e.g.
    // `b"font-family"`), exactly the names `parseIntoField` matches against.
    fn parse_into_field(&mut self, key: &[u8], value: Option<&[u8]>) -> Result<(), ParseError> {
        // The empty-string reset: `if (value) |v| { if (v.len == 0) <reset> }`.
        // For a field whose type has no `init` decl, the reset is to the struct's
        // declared default (`field.default_value_ptr`), which `Config::default()`
        // materializes. We compute the empty flag once; each arm consults it.
        let is_empty = matches!(value, Some(v) if v.is_empty());

        // Reset field `$f` to the struct's declared default.
        macro_rules! reset_default {
            ($f:ident) => {{
                self.$f = Config::default().$f;
                return Ok(());
            }};
        }
        // String field (`[]const u8`): empty -> reset to default; else dup.
        macro_rules! field_string {
            ($f:ident) => {{
                if is_empty {
                    reset_default!($f);
                }
                self.$f = parse_field_string(value)?;
                Ok(())
            }};
        }
        // Optional string field (`?[:0]const u8` / `?[]const u8`).
        macro_rules! field_opt_sentinel_string {
            ($f:ident) => {{
                if is_empty {
                    reset_default!($f);
                }
                self.$f = Some(parse_field_sentinel_string(value)?);
                Ok(())
            }};
        }
        // Sentinel string field (`[:0]const u8`, non-optional).
        macro_rules! field_sentinel_string {
            ($f:ident) => {{
                if is_empty {
                    reset_default!($f);
                }
                self.$f = parse_field_sentinel_string(value)?;
                Ok(())
            }};
        }
        // Bool field.
        macro_rules! field_bool {
            ($f:ident) => {{
                if is_empty {
                    reset_default!($f);
                }
                self.$f = parse_field_bool(value)?;
                Ok(())
            }};
        }
        // Integer field of type `$t`.
        macro_rules! field_int {
            ($f:ident, $t:ty) => {{
                if is_empty {
                    reset_default!($f);
                }
                self.$f = parse_field_int::<$t>(value)?;
                Ok(())
            }};
        }
        // Optional integer field of type `$t`.
        macro_rules! field_opt_int {
            ($f:ident, $t:ty) => {{
                if is_empty {
                    reset_default!($f);
                }
                self.$f = Some(parse_field_int::<$t>(value)?);
                Ok(())
            }};
        }
        // Float field of type `$t`.
        macro_rules! field_float {
            ($f:ident, $t:ty) => {{
                if is_empty {
                    reset_default!($f);
                }
                self.$f = parse_field_float::<$t>(value)?;
                Ok(())
            }};
        }
        // Bare-enum field (`stringToEnum`). `$map` is the type's `string_to_enum`.
        macro_rules! field_enum {
            ($f:ident, $map:expr) => {{
                if is_empty {
                    reset_default!($f);
                }
                self.$f = parse_field_enum(value, $map)?;
                Ok(())
            }};
        }
        // Optional bare-enum field.
        macro_rules! field_opt_enum {
            ($f:ident, $map:expr) => {{
                if is_empty {
                    reset_default!($f);
                }
                self.$f = Some(parse_field_enum(value, $map)?);
                Ok(())
            }};
        }
        // parseCLI 1-arg form `(input) => output`. `$ty` provides `parse_cli`.
        macro_rules! field_parsecli_value {
            ($f:ident, $ty:ty) => {{
                if is_empty {
                    reset_default!($f);
                }
                self.$f = <$ty>::parse_cli(value)?;
                Ok(())
            }};
        }
        // parseCLI 1-arg form into an optional field.
        macro_rules! field_opt_parsecli_value {
            ($f:ident, $ty:ty) => {{
                if is_empty {
                    reset_default!($f);
                }
                self.$f = Some(<$ty>::parse_cli(value)?);
                Ok(())
            }};
        }
        // `?MetricModifier` field. `MetricModifier::parse_cli` returns the font
        // crate's `Error` (`InvalidFormat`/`ValueRequired`); the reference
        // propagates this into the args error union, where `InvalidFormat` is an
        // invalid value and `ValueRequired` is a missing value.
        macro_rules! field_opt_metric_modifier {
            ($f:ident) => {{
                if is_empty {
                    reset_default!($f);
                }
                self.$f = Some(MetricModifier::parse_cli(value).map_err(|e| match e {
                    rustty_font::metrics::Error::ValueRequired => Error::ValueRequired,
                    rustty_font::metrics::Error::InvalidFormat
                    | rustty_font::metrics::Error::OutOfMemory => Error::InvalidValue,
                })?);
                Ok(())
            }};
        }
        // parseCLI self-form `(self, input)/(self, alloc, input) => void` on a
        // non-optional field: mutate in place.
        macro_rules! field_parsecli_self {
            ($f:ident) => {{
                if is_empty {
                    reset_default!($f);
                }
                self.$f.parse_cli(value)?;
                Ok(())
            }};
        }
        // parseCLI self-form whose `parse_cli` returns a type-specific error
        // (not the args error union); `$map` converts it, mirroring the
        // reference propagating the type's `parseCLI` error.
        macro_rules! field_parsecli_self_map {
            ($f:ident, $map:expr) => {{
                if is_empty {
                    reset_default!($f);
                }
                self.$f.parse_cli(value).map_err($map)?;
                Ok(())
            }};
        }

        match key {
            // language: ?[:0]const u8
            b"language" => field_opt_sentinel_string!(language),

            // @"font-family": RepeatableString (parseCLI self)
            b"font-family" => field_parsecli_self!(font_family),
            b"font-family-bold" => field_parsecli_self!(font_family_bold),
            b"font-family-italic" => field_parsecli_self!(font_family_italic),
            b"font-family-bold-italic" => field_parsecli_self!(font_family_bold_italic),

            // @"font-style": FontStyle (parseCLI self)
            b"font-style" => field_parsecli_self!(font_style),
            b"font-style-bold" => field_parsecli_self!(font_style_bold),
            b"font-style-italic" => field_parsecli_self!(font_style_italic),
            b"font-style-bold-italic" => field_parsecli_self!(font_style_bold_italic),

            // @"font-synthetic-style": FontSyntheticStyle (packed struct, no parseCLI)
            b"font-synthetic-style" => {
                if is_empty {
                    reset_default!(font_synthetic_style);
                }
                self.font_synthetic_style =
                    rustty_args::args::parse_packed_struct(value.ok_or(Error::ValueRequired)?)?;
                Ok(())
            }

            // @"font-feature": RepeatableString (parseCLI self)
            b"font-feature" => field_parsecli_self!(font_feature),

            // @"font-size": f32
            b"font-size" => field_float!(font_size, f32),

            // @"font-variation": RepeatableFontVariation (parseCLI self)
            b"font-variation" => field_parsecli_self!(font_variation),
            b"font-variation-bold" => field_parsecli_self!(font_variation_bold),
            b"font-variation-italic" => field_parsecli_self!(font_variation_italic),
            b"font-variation-bold-italic" => field_parsecli_self!(font_variation_bold_italic),

            // @"font-codepoint-map": RepeatableCodepointMap (parseCLI self)
            b"font-codepoint-map" => field_parsecli_self!(font_codepoint_map),

            // @"clipboard-codepoint-map": RepeatableClipboardCodepointMap (parseCLI self)
            b"clipboard-codepoint-map" => field_parsecli_self!(clipboard_codepoint_map),

            // @"font-thicken": bool
            b"font-thicken" => field_bool!(font_thicken),
            // @"font-thicken-strength": u8
            b"font-thicken-strength" => field_int!(font_thicken_strength, u8),

            // @"font-shaping-break": FontShapingBreak (packed struct)
            b"font-shaping-break" => {
                if is_empty {
                    reset_default!(font_shaping_break);
                }
                self.font_shaping_break =
                    rustty_args::args::parse_packed_struct(value.ok_or(Error::ValueRequired)?)?;
                Ok(())
            }

            // @"alpha-blending": AlphaBlending (bare enum)
            b"alpha-blending" => field_enum!(alpha_blending, AlphaBlending::string_to_enum),

            // @"adjust-*": ?MetricModifier (parseCLI value, 1-arg)
            b"adjust-cell-width" => field_opt_metric_modifier!(adjust_cell_width),
            b"adjust-cell-height" => field_opt_metric_modifier!(adjust_cell_height),
            b"adjust-font-baseline" => {
                field_opt_metric_modifier!(adjust_font_baseline)
            }
            b"adjust-underline-position" => {
                field_opt_metric_modifier!(adjust_underline_position)
            }
            b"adjust-underline-thickness" => {
                field_opt_metric_modifier!(adjust_underline_thickness)
            }
            b"adjust-strikethrough-position" => {
                field_opt_metric_modifier!(adjust_strikethrough_position)
            }
            b"adjust-strikethrough-thickness" => {
                field_opt_metric_modifier!(adjust_strikethrough_thickness)
            }
            b"adjust-overline-position" => {
                field_opt_metric_modifier!(adjust_overline_position)
            }
            b"adjust-overline-thickness" => {
                field_opt_metric_modifier!(adjust_overline_thickness)
            }
            b"adjust-cursor-thickness" => {
                field_opt_metric_modifier!(adjust_cursor_thickness)
            }
            b"adjust-cursor-height" => {
                field_opt_metric_modifier!(adjust_cursor_height)
            }
            b"adjust-box-thickness" => {
                field_opt_metric_modifier!(adjust_box_thickness)
            }
            b"adjust-icon-height" => {
                field_opt_metric_modifier!(adjust_icon_height)
            }

            // @"grapheme-width-method": GraphemeWidthMethod (bare enum)
            b"grapheme-width-method" => {
                field_enum!(grapheme_width_method, GraphemeWidthMethod::string_to_enum)
            }

            // @"freetype-load-flags": FreetypeLoadFlags (packed struct)
            b"freetype-load-flags" => {
                if is_empty {
                    reset_default!(freetype_load_flags);
                }
                self.freetype_load_flags =
                    rustty_args::args::parse_packed_struct(value.ok_or(Error::ValueRequired)?)?;
                Ok(())
            }

            // theme: ?Theme (parseCLI self, optional)
            b"theme" => {
                if is_empty {
                    reset_default!(theme);
                }
                // .optional => if (@field(dst, name)) |*v| v.parseCLI(...) else
                // { var tmp = undefined; tmp.parseCLI(...); @field = tmp; }
                match &mut self.theme {
                    Some(v) => v.parse_cli(value)?,
                    None => {
                        let mut tmp = Theme::default();
                        tmp.parse_cli(value)?;
                        self.theme = Some(tmp);
                    }
                }
                Ok(())
            }

            // background: Color (parseCLI value)
            b"background" => field_parsecli_value!(background, Color),
            // foreground: Color (parseCLI value)
            b"foreground" => field_parsecli_value!(foreground, Color),

            // @"background-image": ?Path (parseCLI self, optional)
            b"background-image" => {
                if is_empty {
                    reset_default!(background_image);
                }
                // The reference seeds `var tmp: Field = undefined` for the unset
                // optional; a non-empty input always overwrites the placeholder.
                match &mut self.background_image {
                    Some(v) => v.parse_cli(value).map_err(path_parse_err)?,
                    None => {
                        let mut tmp = Path::Optional(Vec::new());
                        tmp.parse_cli(value).map_err(path_parse_err)?;
                        self.background_image = Some(tmp);
                    }
                }
                Ok(())
            }
            // @"background-image-opacity": f32
            b"background-image-opacity" => field_float!(background_image_opacity, f32),
            // @"background-image-position": BackgroundImagePosition (bare enum)
            b"background-image-position" => {
                field_enum!(
                    background_image_position,
                    BackgroundImagePosition::string_to_enum
                )
            }
            // @"background-image-fit": BackgroundImageFit (bare enum)
            b"background-image-fit" => {
                field_enum!(background_image_fit, BackgroundImageFit::string_to_enum)
            }
            // @"background-image-repeat": bool
            b"background-image-repeat" => field_bool!(background_image_repeat),

            // @"selection-foreground": ?TerminalColor (parseCLI value, optional)
            b"selection-foreground" => {
                field_opt_parsecli_value!(selection_foreground, TerminalColor)
            }
            b"selection-background" => {
                field_opt_parsecli_value!(selection_background, TerminalColor)
            }
            // @"selection-clear-on-typing": bool
            b"selection-clear-on-typing" => field_bool!(selection_clear_on_typing),
            // @"selection-clear-on-copy": bool
            b"selection-clear-on-copy" => field_bool!(selection_clear_on_copy),
            // @"selection-word-chars": SelectionWordChars (parseCLI self)
            b"selection-word-chars" => field_parsecli_self!(selection_word_chars),

            // @"minimum-contrast": f64
            b"minimum-contrast" => field_float!(minimum_contrast, f64),

            // palette: Palette (parseCLI self)
            b"palette" => {
                if is_empty {
                    reset_default!(palette);
                }
                self.palette
                    .parse_cli(value)
                    .map_err(|_| Error::InvalidValue)?;
                Ok(())
            }
            // @"palette-generate": bool
            b"palette-generate" => field_bool!(palette_generate),
            // @"palette-harmonious": bool
            b"palette-harmonious" => field_bool!(palette_harmonious),

            // @"cursor-color": ?TerminalColor (parseCLI value, optional)
            b"cursor-color" => field_opt_parsecli_value!(cursor_color, TerminalColor),
            // @"cursor-opacity": f64
            b"cursor-opacity" => field_float!(cursor_opacity, f64),
            // @"cursor-style": terminal.CursorStyle (bare enum)
            b"cursor-style" => field_enum!(cursor_style, cursor_style_string_to_enum),
            // @"cursor-style-blink": ?bool
            b"cursor-style-blink" => {
                if is_empty {
                    reset_default!(cursor_style_blink);
                }
                self.cursor_style_blink = Some(parse_field_bool(value)?);
                Ok(())
            }
            // @"cursor-text": ?TerminalColor (parseCLI value, optional)
            b"cursor-text" => field_opt_parsecli_value!(cursor_text, TerminalColor),
            // @"cursor-click-to-move": bool
            b"cursor-click-to-move" => field_bool!(cursor_click_to_move),

            // @"mouse-hide-while-typing": bool
            b"mouse-hide-while-typing" => field_bool!(mouse_hide_while_typing),
            // @"scroll-to-bottom": ScrollToBottom (packed struct)
            b"scroll-to-bottom" => {
                if is_empty {
                    reset_default!(scroll_to_bottom);
                }
                self.scroll_to_bottom =
                    rustty_args::args::parse_packed_struct(value.ok_or(Error::ValueRequired)?)?;
                Ok(())
            }
            // @"mouse-shift-capture": MouseShiftCapture (bare enum)
            b"mouse-shift-capture" => {
                field_enum!(mouse_shift_capture, MouseShiftCapture::string_to_enum)
            }
            // @"mouse-reporting": bool
            b"mouse-reporting" => field_bool!(mouse_reporting),
            // @"mouse-scroll-multiplier": MouseScrollMultiplier (parseCLI self)
            b"mouse-scroll-multiplier" => field_parsecli_self!(mouse_scroll_multiplier),

            // @"background-opacity": f64
            b"background-opacity" => field_float!(background_opacity, f64),
            // @"background-opacity-cells": bool
            b"background-opacity-cells" => field_bool!(background_opacity_cells),
            // @"background-blur": BackgroundBlur (parseCLI self)
            b"background-blur" => field_parsecli_self!(background_blur),

            // @"unfocused-split-opacity": f64
            b"unfocused-split-opacity" => field_float!(unfocused_split_opacity, f64),
            // @"unfocused-split-fill": ?Color (parseCLI value, optional)
            b"unfocused-split-fill" => field_opt_parsecli_value!(unfocused_split_fill, Color),
            // @"split-divider-color": ?Color (parseCLI value, optional)
            b"split-divider-color" => field_opt_parsecli_value!(split_divider_color, Color),
            // @"split-preserve-zoom": SplitPreserveZoom (packed struct)
            b"split-preserve-zoom" => {
                if is_empty {
                    reset_default!(split_preserve_zoom);
                }
                self.split_preserve_zoom =
                    rustty_args::args::parse_packed_struct(value.ok_or(Error::ValueRequired)?)?;
                Ok(())
            }

            // @"search-foreground": TerminalColor (parseCLI value)
            b"search-foreground" => field_parsecli_value!(search_foreground, TerminalColor),
            b"search-background" => field_parsecli_value!(search_background, TerminalColor),
            b"search-selected-foreground" => {
                field_parsecli_value!(search_selected_foreground, TerminalColor)
            }
            b"search-selected-background" => {
                field_parsecli_value!(search_selected_background, TerminalColor)
            }

            // command: ?Command (parseCLI self, optional)
            b"command" => {
                if is_empty {
                    reset_default!(command);
                }
                // `Command::parse_cli` always overwrites `self` (or errors), so
                // the `var tmp = undefined` placeholder is never observed.
                match &mut self.command {
                    Some(v) => v.parse_cli(value).map_err(command_parse_err)?,
                    None => {
                        let mut tmp = Command::Shell(Vec::new());
                        tmp.parse_cli(value).map_err(command_parse_err)?;
                        self.command = Some(tmp);
                    }
                }
                Ok(())
            }
            // @"initial-command": ?Command (parseCLI self, optional)
            b"initial-command" => {
                if is_empty {
                    reset_default!(initial_command);
                }
                match &mut self.initial_command {
                    Some(v) => v.parse_cli(value).map_err(command_parse_err)?,
                    None => {
                        let mut tmp = Command::Shell(Vec::new());
                        tmp.parse_cli(value).map_err(command_parse_err)?;
                        self.initial_command = Some(tmp);
                    }
                }
                Ok(())
            }

            // @"notify-on-command-finish": NotifyOnCommandFinish (bare enum)
            b"notify-on-command-finish" => {
                field_enum!(
                    notify_on_command_finish,
                    NotifyOnCommandFinish::string_to_enum
                )
            }
            // @"notify-on-command-finish-action": NotifyOnCommandFinishAction (packed struct)
            b"notify-on-command-finish-action" => {
                if is_empty {
                    reset_default!(notify_on_command_finish_action);
                }
                self.notify_on_command_finish_action =
                    rustty_args::args::parse_packed_struct(value.ok_or(Error::ValueRequired)?)?;
                Ok(())
            }
            // @"notify-on-command-finish-after": Duration (parseCLI value)
            b"notify-on-command-finish-after" => {
                field_parsecli_value!(notify_on_command_finish_after, Duration)
            }

            // env: RepeatableStringMap (parseCLI self)
            b"env" => field_parsecli_self_map!(env, rsm_parse_err),
            // input: RepeatableReadableIO (parseCLI self)
            b"input" => field_parsecli_self_map!(input, io_parse_err),
            // @"wait-after-command": bool
            b"wait-after-command" => field_bool!(wait_after_command),
            // @"abnormal-command-exit-runtime": u32
            b"abnormal-command-exit-runtime" => field_int!(abnormal_command_exit_runtime, u32),
            // @"scrollback-limit": usize
            b"scrollback-limit" => field_int!(scrollback_limit, usize),
            // scrollbar: Scrollbar (bare enum)
            b"scrollbar" => field_enum!(scrollbar, Scrollbar::string_to_enum),

            // link: RepeatableLink (parseCLI self)
            b"link" => field_parsecli_self!(link),
            // @"link-url": bool
            b"link-url" => field_bool!(link_url),
            // @"link-previews": LinkPreviews (bare enum)
            b"link-previews" => field_enum!(link_previews, LinkPreviews::string_to_enum),

            // maximize: bool
            b"maximize" => field_bool!(maximize),
            // fullscreen: Fullscreen (bare enum)
            b"fullscreen" => field_enum!(fullscreen, Fullscreen::string_to_enum),
            // title: ?[:0]const u8
            b"title" => field_opt_sentinel_string!(title),
            // class: ?[:0]const u8
            b"class" => field_opt_sentinel_string!(class),
            // @"x11-instance-name": ?[:0]const u8
            b"x11-instance-name" => field_opt_sentinel_string!(x11_instance_name),

            // @"working-directory": ?WorkingDirectory (parseCLI self, optional)
            b"working-directory" => {
                if is_empty {
                    reset_default!(working_directory);
                }
                match &mut self.working_directory {
                    Some(v) => v.parse_cli(value)?,
                    None => {
                        let mut tmp = WorkingDirectory::Home;
                        tmp.parse_cli(value)?;
                        self.working_directory = Some(tmp);
                    }
                }
                Ok(())
            }

            // keybind: Keybinds (parseCLI self) — note: `init` decl reset semantics
            b"keybind" => {
                if is_empty {
                    // canHaveDecls and @hasDecl(Field, "init")
                    self.keybind.init();
                    return Ok(());
                }
                self.keybind
                    .parse_cli(value)
                    .map_err(|_| Error::InvalidValue)?;
                Ok(())
            }
            // @"key-remap": KeyRemapSet (parseCLI self)
            b"key-remap" => {
                if is_empty {
                    reset_default!(key_remap);
                }
                self.key_remap
                    .parse_cli(value)
                    .map_err(|_| Error::InvalidValue)?;
                Ok(())
            }

            // @"window-padding-x": WindowPadding (parseCLI value)
            b"window-padding-x" => field_parsecli_value!(window_padding_x, WindowPadding),
            // @"window-padding-y": WindowPadding (parseCLI value)
            b"window-padding-y" => field_parsecli_value!(window_padding_y, WindowPadding),
            // @"window-padding-balance": WindowPaddingBalance (bare enum)
            b"window-padding-balance" => {
                field_enum!(
                    window_padding_balance,
                    window_padding_balance_string_to_enum
                )
            }
            // @"window-padding-color": WindowPaddingColor (bare enum)
            b"window-padding-color" => {
                field_enum!(window_padding_color, WindowPaddingColor::string_to_enum)
            }
            // @"window-vsync": bool
            b"window-vsync" => field_bool!(window_vsync),
            // @"window-inherit-working-directory": bool
            b"window-inherit-working-directory" => field_bool!(window_inherit_working_directory),
            // @"tab-inherit-working-directory": bool
            b"tab-inherit-working-directory" => field_bool!(tab_inherit_working_directory),
            // @"split-inherit-working-directory": bool
            b"split-inherit-working-directory" => field_bool!(split_inherit_working_directory),
            // @"window-inherit-font-size": bool
            b"window-inherit-font-size" => field_bool!(window_inherit_font_size),
            // @"window-decoration": WindowDecoration (parseCLI value)
            b"window-decoration" => field_parsecli_value!(window_decoration, WindowDecoration),
            // @"window-title-font-family": ?[:0]const u8
            b"window-title-font-family" => field_opt_sentinel_string!(window_title_font_family),
            // @"window-subtitle": WindowSubtitle (bare enum)
            b"window-subtitle" => field_enum!(window_subtitle, WindowSubtitle::string_to_enum),
            // @"window-theme": WindowTheme (bare enum)
            b"window-theme" => field_enum!(window_theme, WindowTheme::string_to_enum),
            // @"window-colorspace": WindowColorspace (bare enum)
            b"window-colorspace" => {
                field_enum!(window_colorspace, WindowColorspace::string_to_enum)
            }
            // @"window-height": u32
            b"window-height" => field_int!(window_height, u32),
            // @"window-width": u32
            b"window-width" => field_int!(window_width, u32),
            // @"window-position-x": ?i16
            b"window-position-x" => field_opt_int!(window_position_x, i16),
            // @"window-position-y": ?i16
            b"window-position-y" => field_opt_int!(window_position_y, i16),
            // @"window-save-state": WindowSaveState (bare enum)
            b"window-save-state" => field_enum!(window_save_state, WindowSaveState::string_to_enum),
            // @"window-step-resize": bool
            b"window-step-resize" => field_bool!(window_step_resize),
            // @"window-new-tab-position": WindowNewTabPosition (bare enum)
            b"window-new-tab-position" => {
                field_enum!(
                    window_new_tab_position,
                    WindowNewTabPosition::string_to_enum
                )
            }
            // @"window-show-tab-bar": WindowShowTabBar (bare enum)
            b"window-show-tab-bar" => {
                field_enum!(window_show_tab_bar, WindowShowTabBar::string_to_enum)
            }
            // @"window-titlebar-background": ?Color (parseCLI value, optional)
            b"window-titlebar-background" => {
                field_opt_parsecli_value!(window_titlebar_background, Color)
            }
            // @"window-titlebar-foreground": ?Color (parseCLI value, optional)
            b"window-titlebar-foreground" => {
                field_opt_parsecli_value!(window_titlebar_foreground, Color)
            }

            // @"resize-overlay": ResizeOverlay (bare enum)
            b"resize-overlay" => field_enum!(resize_overlay, ResizeOverlay::string_to_enum),
            // @"resize-overlay-position": ResizeOverlayPosition (bare enum)
            b"resize-overlay-position" => {
                field_enum!(
                    resize_overlay_position,
                    ResizeOverlayPosition::string_to_enum
                )
            }
            // @"resize-overlay-duration": Duration (parseCLI value)
            b"resize-overlay-duration" => field_parsecli_value!(resize_overlay_duration, Duration),

            // @"focus-follows-mouse": bool
            b"focus-follows-mouse" => field_bool!(focus_follows_mouse),

            // @"clipboard-read": ClipboardAccess (bare enum)
            b"clipboard-read" => field_enum!(clipboard_read, ClipboardAccess::string_to_enum),
            // @"clipboard-write": ClipboardAccess (bare enum)
            b"clipboard-write" => field_enum!(clipboard_write, ClipboardAccess::string_to_enum),
            // @"clipboard-trim-trailing-spaces": bool
            b"clipboard-trim-trailing-spaces" => field_bool!(clipboard_trim_trailing_spaces),
            // @"clipboard-paste-protection": bool
            b"clipboard-paste-protection" => field_bool!(clipboard_paste_protection),
            // @"clipboard-paste-bracketed-safe": bool
            b"clipboard-paste-bracketed-safe" => field_bool!(clipboard_paste_bracketed_safe),

            // @"title-report": bool
            b"title-report" => field_bool!(title_report),
            // @"image-storage-limit": u32
            b"image-storage-limit" => field_int!(image_storage_limit, u32),

            // @"copy-on-select": CopyOnSelect (bare enum)
            b"copy-on-select" => field_enum!(copy_on_select, CopyOnSelect::string_to_enum),

            // @"right-click-action": RightClickAction (bare enum)
            b"right-click-action" => {
                field_enum!(right_click_action, RightClickAction::string_to_enum)
            }
            // @"middle-click-action": MiddleClickAction (bare enum)
            b"middle-click-action" => {
                field_enum!(middle_click_action, MiddleClickAction::string_to_enum)
            }
            // @"click-repeat-interval": u32
            b"click-repeat-interval" => field_int!(click_repeat_interval, u32),

            // @"config-file": RepeatablePath (parseCLI self)
            b"config-file" => field_parsecli_self_map!(config_file, path_parse_err),
            // @"config-default-files": bool
            b"config-default-files" => field_bool!(config_default_files),

            // @"confirm-close-surface": ConfirmCloseSurface (bare enum)
            b"confirm-close-surface" => {
                field_enum!(confirm_close_surface, ConfirmCloseSurface::string_to_enum)
            }
            // @"quit-after-last-window-closed": bool
            b"quit-after-last-window-closed" => field_bool!(quit_after_last_window_closed),
            // @"quit-after-last-window-closed-delay": ?Duration (parseCLI value, optional)
            b"quit-after-last-window-closed-delay" => {
                field_opt_parsecli_value!(quit_after_last_window_closed_delay, Duration)
            }
            // @"initial-window": bool
            b"initial-window" => field_bool!(initial_window),

            // @"undo-timeout": Duration (parseCLI value)
            b"undo-timeout" => field_parsecli_value!(undo_timeout, Duration),

            // @"quick-terminal-position": QuickTerminalPosition (bare enum)
            b"quick-terminal-position" => {
                field_enum!(
                    quick_terminal_position,
                    QuickTerminalPosition::string_to_enum
                )
            }
            // @"quick-terminal-size": QuickTerminalSize (parseCLI self)
            b"quick-terminal-size" => {
                if is_empty {
                    reset_default!(quick_terminal_size);
                }
                self.quick_terminal_size
                    .parse_cli(value)
                    .map_err(|_| Error::InvalidValue)?;
                Ok(())
            }
            // @"gtk-quick-terminal-layer": QuickTerminalLayer (bare enum)
            b"gtk-quick-terminal-layer" => {
                field_enum!(gtk_quick_terminal_layer, QuickTerminalLayer::string_to_enum)
            }
            // @"gtk-quick-terminal-namespace": [:0]const u8
            b"gtk-quick-terminal-namespace" => field_sentinel_string!(gtk_quick_terminal_namespace),
            // @"quick-terminal-screen": QuickTerminalScreen (bare enum)
            b"quick-terminal-screen" => {
                field_enum!(quick_terminal_screen, QuickTerminalScreen::string_to_enum)
            }
            // @"quick-terminal-animation-duration": f64
            b"quick-terminal-animation-duration" => {
                field_float!(quick_terminal_animation_duration, f64)
            }
            // @"quick-terminal-autohide": bool
            b"quick-terminal-autohide" => field_bool!(quick_terminal_autohide),
            // @"quick-terminal-space-behavior": QuickTerminalSpaceBehavior (bare enum)
            b"quick-terminal-space-behavior" => {
                field_enum!(
                    quick_terminal_space_behavior,
                    QuickTerminalSpaceBehavior::string_to_enum
                )
            }
            // @"quick-terminal-keyboard-interactivity": QuickTerminalKeyboardInteractivity (bare enum)
            b"quick-terminal-keyboard-interactivity" => {
                field_enum!(
                    quick_terminal_keyboard_interactivity,
                    QuickTerminalKeyboardInteractivity::string_to_enum
                )
            }

            // @"shell-integration": ShellIntegration (bare enum)
            b"shell-integration" => {
                field_enum!(shell_integration, ShellIntegration::string_to_enum)
            }
            // @"shell-integration-features": ShellIntegrationFeatures (packed struct)
            b"shell-integration-features" => {
                if is_empty {
                    reset_default!(shell_integration_features);
                }
                self.shell_integration_features =
                    rustty_args::args::parse_packed_struct(value.ok_or(Error::ValueRequired)?)?;
                Ok(())
            }

            // @"command-palette-entry": RepeatableCommand (parseCLI self) — `init` reset
            b"command-palette-entry" => {
                if is_empty {
                    // canHaveDecls and @hasDecl(Field, "init")
                    self.command_palette_entry.init();
                    return Ok(());
                }
                self.command_palette_entry.parse_cli(value)?;
                Ok(())
            }

            // @"osc-color-report-format": OSCColorReportFormat (bare enum)
            b"osc-color-report-format" => {
                field_enum!(
                    osc_color_report_format,
                    OSCColorReportFormat::string_to_enum
                )
            }
            // @"vt-kam-allowed": bool
            b"vt-kam-allowed" => field_bool!(vt_kam_allowed),

            // @"custom-shader": RepeatablePath (parseCLI self)
            b"custom-shader" => field_parsecli_self_map!(custom_shader, path_parse_err),
            // @"custom-shader-animation": CustomShaderAnimation (bare enum)
            b"custom-shader-animation" => {
                field_enum!(
                    custom_shader_animation,
                    CustomShaderAnimation::string_to_enum
                )
            }

            // @"bell-features": BellFeatures (packed struct)
            b"bell-features" => {
                if is_empty {
                    reset_default!(bell_features);
                }
                self.bell_features =
                    rustty_args::args::parse_packed_struct(value.ok_or(Error::ValueRequired)?)?;
                Ok(())
            }
            // @"bell-audio-path": ?Path (parseCLI self, optional)
            b"bell-audio-path" => {
                if is_empty {
                    reset_default!(bell_audio_path);
                }
                match &mut self.bell_audio_path {
                    Some(v) => v.parse_cli(value).map_err(path_parse_err)?,
                    None => {
                        let mut tmp = Path::Optional(Vec::new());
                        tmp.parse_cli(value).map_err(path_parse_err)?;
                        self.bell_audio_path = Some(tmp);
                    }
                }
                Ok(())
            }
            // @"bell-audio-volume": f64
            b"bell-audio-volume" => field_float!(bell_audio_volume, f64),

            // @"app-notifications": AppNotifications (packed struct)
            b"app-notifications" => {
                if is_empty {
                    reset_default!(app_notifications);
                }
                self.app_notifications =
                    rustty_args::args::parse_packed_struct(value.ok_or(Error::ValueRequired)?)?;
                Ok(())
            }

            // @"macos-non-native-fullscreen": NonNativeFullscreen (bare enum)
            b"macos-non-native-fullscreen" => {
                field_enum!(
                    macos_non_native_fullscreen,
                    NonNativeFullscreen::string_to_enum
                )
            }
            // @"macos-window-buttons": MacWindowButtons (bare enum)
            b"macos-window-buttons" => {
                field_enum!(macos_window_buttons, MacWindowButtons::string_to_enum)
            }
            // @"macos-titlebar-style": MacTitlebarStyle (bare enum)
            b"macos-titlebar-style" => {
                field_enum!(macos_titlebar_style, MacTitlebarStyle::string_to_enum)
            }
            // @"macos-titlebar-proxy-icon": MacTitlebarProxyIcon (bare enum)
            b"macos-titlebar-proxy-icon" => {
                field_enum!(
                    macos_titlebar_proxy_icon,
                    MacTitlebarProxyIcon::string_to_enum
                )
            }
            // @"macos-dock-drop-behavior": MacOSDockDropBehavior (bare enum)
            b"macos-dock-drop-behavior" => {
                field_enum!(
                    macos_dock_drop_behavior,
                    MacOSDockDropBehavior::string_to_enum
                )
            }
            // @"macos-option-as-alt": ?inputpkg.OptionAsAlt (bare enum, optional)
            b"macos-option-as-alt" => {
                field_opt_enum!(macos_option_as_alt, option_as_alt_string_to_enum)
            }
            // @"macos-window-shadow": bool
            b"macos-window-shadow" => field_bool!(macos_window_shadow),
            // @"macos-hidden": MacHidden (bare enum)
            b"macos-hidden" => field_enum!(macos_hidden, MacHidden::string_to_enum),
            // @"macos-auto-secure-input": bool
            b"macos-auto-secure-input" => field_bool!(macos_auto_secure_input),
            // @"macos-secure-input-indication": bool
            b"macos-secure-input-indication" => field_bool!(macos_secure_input_indication),
            // @"macos-applescript": bool
            b"macos-applescript" => field_bool!(macos_applescript),
            // @"macos-icon": MacAppIcon (bare enum)
            b"macos-icon" => field_enum!(macos_icon, MacAppIcon::string_to_enum),
            // @"macos-custom-icon": ?[:0]const u8
            b"macos-custom-icon" => field_opt_sentinel_string!(macos_custom_icon),
            // @"macos-icon-frame": MacAppIconFrame (bare enum)
            b"macos-icon-frame" => field_enum!(macos_icon_frame, MacAppIconFrame::string_to_enum),
            // @"macos-icon-ghost-color": ?Color (parseCLI value, optional)
            b"macos-icon-ghost-color" => field_opt_parsecli_value!(macos_icon_ghost_color, Color),
            // @"macos-icon-screen-color": ?ColorList (parseCLI self, optional)
            b"macos-icon-screen-color" => {
                if is_empty {
                    reset_default!(macos_icon_screen_color);
                }
                match &mut self.macos_icon_screen_color {
                    Some(v) => v.parse_cli(value)?,
                    None => {
                        let mut tmp = ColorList::default();
                        tmp.parse_cli(value)?;
                        self.macos_icon_screen_color = Some(tmp);
                    }
                }
                Ok(())
            }
            // @"macos-shortcuts": MacShortcuts (bare enum)
            b"macos-shortcuts" => field_enum!(macos_shortcuts, MacShortcuts::string_to_enum),

            // @"linux-cgroup": LinuxCgroup (bare enum)
            b"linux-cgroup" => field_enum!(linux_cgroup, LinuxCgroup::string_to_enum),
            // @"linux-cgroup-memory-limit": ?u64
            b"linux-cgroup-memory-limit" => field_opt_int!(linux_cgroup_memory_limit, u64),
            // @"linux-cgroup-processes-limit": ?u64
            b"linux-cgroup-processes-limit" => field_opt_int!(linux_cgroup_processes_limit, u64),
            // @"linux-cgroup-hard-fail": bool
            b"linux-cgroup-hard-fail" => field_bool!(linux_cgroup_hard_fail),

            // @"gtk-opengl-debug": bool
            b"gtk-opengl-debug" => field_bool!(gtk_opengl_debug),
            // @"gtk-single-instance": GtkSingleInstance (bare enum)
            b"gtk-single-instance" => {
                field_enum!(gtk_single_instance, GtkSingleInstance::string_to_enum)
            }
            // @"gtk-titlebar": bool
            b"gtk-titlebar" => field_bool!(gtk_titlebar),
            // @"gtk-tabs-location": GtkTabsLocation (bare enum)
            b"gtk-tabs-location" => field_enum!(gtk_tabs_location, GtkTabsLocation::string_to_enum),
            // @"gtk-titlebar-hide-when-maximized": bool
            b"gtk-titlebar-hide-when-maximized" => field_bool!(gtk_titlebar_hide_when_maximized),
            // @"gtk-toolbar-style": GtkToolbarStyle (bare enum)
            b"gtk-toolbar-style" => field_enum!(gtk_toolbar_style, GtkToolbarStyle::string_to_enum),
            // @"gtk-titlebar-style": GtkTitlebarStyle (bare enum)
            b"gtk-titlebar-style" => {
                field_enum!(gtk_titlebar_style, GtkTitlebarStyle::string_to_enum)
            }
            // @"gtk-wide-tabs": bool
            b"gtk-wide-tabs" => field_bool!(gtk_wide_tabs),
            // @"gtk-custom-css": RepeatablePath (parseCLI self)
            b"gtk-custom-css" => field_parsecli_self_map!(gtk_custom_css, path_parse_err),

            // @"desktop-notifications": bool
            b"desktop-notifications" => field_bool!(desktop_notifications),
            // @"progress-style": bool
            b"progress-style" => field_bool!(progress_style),

            // @"bold-color": ?BoldColor (parseCLI value, optional)
            b"bold-color" => field_opt_parsecli_value!(bold_color, BoldColor),
            // @"faint-opacity": f64
            b"faint-opacity" => field_float!(faint_opacity, f64),

            // term: []const u8
            b"term" => field_string!(term),
            // @"enquiry-response": []const u8
            b"enquiry-response" => field_string!(enquiry_response),

            // @"async-backend": AsyncBackend (bare enum)
            b"async-backend" => field_enum!(async_backend, AsyncBackend::string_to_enum),
            // @"auto-update": ?AutoUpdate (bare enum, optional)
            b"auto-update" => field_opt_enum!(auto_update, AutoUpdate::string_to_enum),
            // @"auto-update-channel": ?build_config.ReleaseChannel (bare enum, optional)
            b"auto-update-channel" => {
                field_opt_enum!(auto_update_channel, release_channel_string_to_enum)
            }

            // No non-underscore field matched. `return error.InvalidField;`
            _ => Err(Error::InvalidField),
        }
    }

    // @hasDecl(Config, "parseManuallyHook")
    fn parse_manually_hook(&mut self, _arg: &[u8]) -> Option<Result<bool, ParseError>> {
        Some(Config::parse_manually_hook(self, _arg))
    }
}

// ===========================================================================
// PARSE pass tests.
//
// PORTS: the parse-level coverage for the Config Target dispatch. The
// reference's generic `parseIntoField` is exercised by the args.zig tests
// (ported in rustty-args); these assert the Config-specific per-field dispatch
// built above. `parse_into_field_exhaustive` is the ONE exhaustiveness test
// that replaces Zig's comptime completeness guarantee (the `inline for` over
// `std.meta.fields(Config)`): it asserts every non-`_` Config field — the same
// 203 names that derive the `Key` enum, a COUNT CONTRACT — dispatches to a real
// arm (never `InvalidField`), and that a bogus name does return `InvalidField`.
// ===========================================================================
#[cfg(test)]
mod parse_tests {
    use super::*;

    /// The complete, ordered list of the non-`_` Config field names (the Zig
    /// hyphenated spellings). This is a CONTRACT: it must equal the set of
    /// `Key` variants (203) and the set of `parse_into_field` arms. A drift in
    /// any of the three is a parity blocker.
    const CONFIG_FIELD_NAMES: &[&[u8]] = &[
        b"abnormal-command-exit-runtime",
        b"adjust-box-thickness",
        b"adjust-cell-height",
        b"adjust-cell-width",
        b"adjust-cursor-height",
        b"adjust-cursor-thickness",
        b"adjust-font-baseline",
        b"adjust-icon-height",
        b"adjust-overline-position",
        b"adjust-overline-thickness",
        b"adjust-strikethrough-position",
        b"adjust-strikethrough-thickness",
        b"adjust-underline-position",
        b"adjust-underline-thickness",
        b"alpha-blending",
        b"app-notifications",
        b"async-backend",
        b"auto-update",
        b"auto-update-channel",
        b"background",
        b"background-blur",
        b"background-image",
        b"background-image-fit",
        b"background-image-opacity",
        b"background-image-position",
        b"background-image-repeat",
        b"background-opacity",
        b"background-opacity-cells",
        b"bell-audio-path",
        b"bell-audio-volume",
        b"bell-features",
        b"bold-color",
        b"class",
        b"click-repeat-interval",
        b"clipboard-codepoint-map",
        b"clipboard-paste-bracketed-safe",
        b"clipboard-paste-protection",
        b"clipboard-read",
        b"clipboard-trim-trailing-spaces",
        b"clipboard-write",
        b"command",
        b"command-palette-entry",
        b"config-default-files",
        b"config-file",
        b"confirm-close-surface",
        b"copy-on-select",
        b"cursor-click-to-move",
        b"cursor-color",
        b"cursor-opacity",
        b"cursor-style",
        b"cursor-style-blink",
        b"cursor-text",
        b"custom-shader",
        b"custom-shader-animation",
        b"desktop-notifications",
        b"enquiry-response",
        b"env",
        b"faint-opacity",
        b"focus-follows-mouse",
        b"font-codepoint-map",
        b"font-family",
        b"font-family-bold",
        b"font-family-bold-italic",
        b"font-family-italic",
        b"font-feature",
        b"font-shaping-break",
        b"font-size",
        b"font-style",
        b"font-style-bold",
        b"font-style-bold-italic",
        b"font-style-italic",
        b"font-synthetic-style",
        b"font-thicken",
        b"font-thicken-strength",
        b"font-variation",
        b"font-variation-bold",
        b"font-variation-bold-italic",
        b"font-variation-italic",
        b"foreground",
        b"freetype-load-flags",
        b"fullscreen",
        b"grapheme-width-method",
        b"gtk-custom-css",
        b"gtk-opengl-debug",
        b"gtk-quick-terminal-layer",
        b"gtk-quick-terminal-namespace",
        b"gtk-single-instance",
        b"gtk-tabs-location",
        b"gtk-titlebar",
        b"gtk-titlebar-hide-when-maximized",
        b"gtk-titlebar-style",
        b"gtk-toolbar-style",
        b"gtk-wide-tabs",
        b"image-storage-limit",
        b"initial-command",
        b"initial-window",
        b"input",
        b"key-remap",
        b"keybind",
        b"language",
        b"link",
        b"link-previews",
        b"link-url",
        b"linux-cgroup",
        b"linux-cgroup-hard-fail",
        b"linux-cgroup-memory-limit",
        b"linux-cgroup-processes-limit",
        b"macos-applescript",
        b"macos-auto-secure-input",
        b"macos-custom-icon",
        b"macos-dock-drop-behavior",
        b"macos-hidden",
        b"macos-icon",
        b"macos-icon-frame",
        b"macos-icon-ghost-color",
        b"macos-icon-screen-color",
        b"macos-non-native-fullscreen",
        b"macos-option-as-alt",
        b"macos-secure-input-indication",
        b"macos-shortcuts",
        b"macos-titlebar-proxy-icon",
        b"macos-titlebar-style",
        b"macos-window-buttons",
        b"macos-window-shadow",
        b"maximize",
        b"middle-click-action",
        b"minimum-contrast",
        b"mouse-hide-while-typing",
        b"mouse-reporting",
        b"mouse-scroll-multiplier",
        b"mouse-shift-capture",
        b"notify-on-command-finish",
        b"notify-on-command-finish-action",
        b"notify-on-command-finish-after",
        b"osc-color-report-format",
        b"palette",
        b"palette-generate",
        b"palette-harmonious",
        b"progress-style",
        b"quick-terminal-animation-duration",
        b"quick-terminal-autohide",
        b"quick-terminal-keyboard-interactivity",
        b"quick-terminal-position",
        b"quick-terminal-screen",
        b"quick-terminal-size",
        b"quick-terminal-space-behavior",
        b"quit-after-last-window-closed",
        b"quit-after-last-window-closed-delay",
        b"resize-overlay",
        b"resize-overlay-duration",
        b"resize-overlay-position",
        b"right-click-action",
        b"scroll-to-bottom",
        b"scrollback-limit",
        b"scrollbar",
        b"search-background",
        b"search-foreground",
        b"search-selected-background",
        b"search-selected-foreground",
        b"selection-background",
        b"selection-clear-on-copy",
        b"selection-clear-on-typing",
        b"selection-foreground",
        b"selection-word-chars",
        b"shell-integration",
        b"shell-integration-features",
        b"split-divider-color",
        b"split-inherit-working-directory",
        b"split-preserve-zoom",
        b"tab-inherit-working-directory",
        b"term",
        b"theme",
        b"title",
        b"title-report",
        b"undo-timeout",
        b"unfocused-split-fill",
        b"unfocused-split-opacity",
        b"vt-kam-allowed",
        b"wait-after-command",
        b"window-colorspace",
        b"window-decoration",
        b"window-height",
        b"window-inherit-font-size",
        b"window-inherit-working-directory",
        b"window-new-tab-position",
        b"window-padding-balance",
        b"window-padding-color",
        b"window-padding-x",
        b"window-padding-y",
        b"window-position-x",
        b"window-position-y",
        b"window-save-state",
        b"window-show-tab-bar",
        b"window-step-resize",
        b"window-subtitle",
        b"window-theme",
        b"window-title-font-family",
        b"window-titlebar-background",
        b"window-titlebar-foreground",
        b"window-vsync",
        b"window-width",
        b"working-directory",
        b"x11-instance-name",
    ];

    // The field-name list and the `Key` enum are both derived (in the
    // reference) from `std.meta.fields(Config)`, so their counts must match.
    #[test]
    fn config_field_names_count_is_contract() {
        assert_eq!(CONFIG_FIELD_NAMES.len(), 203);
    }

    // Every non-`_` Config field dispatches to a real parse arm. We use the
    // empty-string value, which `parseIntoField` resets to the field default
    // for ALL fields (or `init()` for the two `init`-decl types) — always
    // `Ok(())`, and crucially never `InvalidField`. This proves the dispatch is
    // exhaustive over the field set.
    #[test]
    fn parse_into_field_exhaustive() {
        for name in CONFIG_FIELD_NAMES {
            let mut cfg = Config::default();
            let r = cfg.parse_into_field(name, Some(b""));
            assert!(
                r.is_ok(),
                "field {:?} did not dispatch (got {:?})",
                std::str::from_utf8(name),
                r
            );
        }
    }

    // An unknown (non-existent) field name returns `error.InvalidField`, as the
    // reference's final `return error.InvalidField;` does after the `inline for`.
    #[test]
    fn parse_into_field_unknown_is_invalid_field() {
        let mut cfg = Config::default();
        assert_eq!(
            cfg.parse_into_field(b"this-field-does-not-exist", Some(b"x")),
            Err(Error::InvalidField)
        );
    }

    // Underscore-prefixed (internal) fields are never matched — they have no
    // arm, so they fall through to `InvalidField` exactly like an unknown name.
    #[test]
    fn parse_into_field_ignores_underscore_fields() {
        let mut cfg = Config::default();
        assert_eq!(
            cfg.parse_into_field(b"_arena", Some(b"t")),
            Err(Error::InvalidField)
        );
    }

    // ---- Representative per-category dispatch tests. ----

    // bool field.
    #[test]
    fn parse_bool_field() {
        let mut cfg = Config::default();
        cfg.parse_into_field(b"font-thicken", Some(b"true"))
            .unwrap();
        assert!(cfg.font_thicken);
        cfg.parse_into_field(b"font-thicken", Some(b"false"))
            .unwrap();
        assert!(!cfg.font_thicken);
        // No value defaults to "t" (set the flag).
        cfg.parse_into_field(b"maximize", None).unwrap();
        assert!(cfg.maximize);
    }

    // Integer field (base-0 radix detection).
    #[test]
    fn parse_int_field() {
        let mut cfg = Config::default();
        cfg.parse_into_field(b"font-thicken-strength", Some(b"128"))
            .unwrap();
        assert_eq!(cfg.font_thicken_strength, 128);
        cfg.parse_into_field(b"window-height", Some(b"0x10"))
            .unwrap();
        assert_eq!(cfg.window_height, 16);
        // Out of range -> InvalidValue.
        assert_eq!(
            cfg.parse_into_field(b"font-thicken-strength", Some(b"999")),
            Err(Error::InvalidValue)
        );
    }

    // Optional integer field.
    #[test]
    fn parse_opt_int_field() {
        let mut cfg = Config::default();
        cfg.parse_into_field(b"window-position-x", Some(b"-3"))
            .unwrap();
        assert_eq!(cfg.window_position_x, Some(-3));
    }

    // Float field.
    #[test]
    fn parse_float_field() {
        let mut cfg = Config::default();
        cfg.parse_into_field(b"font-size", Some(b"15.5")).unwrap();
        assert_eq!(cfg.font_size, 15.5);
    }

    // String field (`[]const u8`) is duped.
    #[test]
    fn parse_string_field() {
        let mut cfg = Config::default();
        cfg.parse_into_field(b"term", Some(b"xterm-rustty"))
            .unwrap();
        assert_eq!(cfg.term.as_slice(), b"xterm-rustty");
    }

    // Optional sentinel-string field.
    #[test]
    fn parse_opt_sentinel_string_field() {
        let mut cfg = Config::default();
        cfg.parse_into_field(b"language", Some(b"de")).unwrap();
        assert_eq!(cfg.language.as_deref(), Some(b"de".as_slice()));
    }

    // Bare enum (stringToEnum) with a HYPHENATED variant name.
    #[test]
    fn parse_bare_enum_field_hyphenated() {
        let mut cfg = Config::default();
        cfg.parse_into_field(b"macos-dock-drop-behavior", Some(b"new-window"))
            .unwrap();
        assert_eq!(
            cfg.macos_dock_drop_behavior,
            MacOSDockDropBehavior::NewWindow
        );
        // Unknown variant -> InvalidValue.
        assert_eq!(
            cfg.parse_into_field(b"macos-dock-drop-behavior", Some(b"bogus")),
            Err(Error::InvalidValue)
        );
    }

    // Bare enum with a numeric-prefixed hyphenated name ("16-bit").
    #[test]
    fn parse_bare_enum_field_numeric_name() {
        let mut cfg = Config::default();
        cfg.parse_into_field(b"osc-color-report-format", Some(b"8-bit"))
            .unwrap();
        assert_eq!(cfg.osc_color_report_format, OSCColorReportFormat::Bit8);
    }

    // Optional bare enum field.
    #[test]
    fn parse_opt_bare_enum_field() {
        let mut cfg = Config::default();
        cfg.parse_into_field(b"auto-update", Some(b"download"))
            .unwrap();
        assert_eq!(cfg.auto_update, Some(AutoUpdate::Download));
    }

    // Packed struct field (comma-separated parts + whole-struct bool shorthand).
    #[test]
    fn parse_packed_struct_field() {
        let mut cfg = Config::default();
        cfg.parse_into_field(b"bell-features", Some(b"audio,no-title"))
            .unwrap();
        assert!(cfg.bell_features.audio);
        assert!(!cfg.bell_features.title);
        // Whole-struct shorthand.
        cfg.parse_into_field(b"bell-features", Some(b"true"))
            .unwrap();
        assert!(cfg.bell_features.system && cfg.bell_features.audio);
    }

    // parseCLI 1-arg value form (Color).
    #[test]
    fn parse_parsecli_value_field() {
        let mut cfg = Config::default();
        cfg.parse_into_field(b"background", Some(b"#112233"))
            .unwrap();
        assert_eq!(
            cfg.background,
            Color {
                r: 0x11,
                g: 0x22,
                b: 0x33
            }
        );
    }

    // parseCLI self form (RepeatableString appends).
    #[test]
    fn parse_parsecli_self_field() {
        let mut cfg = Config::default();
        cfg.parse_into_field(b"font-family", Some(b"Comic Sans"))
            .unwrap();
        cfg.parse_into_field(b"font-family", Some(b"Monaco"))
            .unwrap();
        assert_eq!(cfg.font_family.list.len(), 2);
    }

    // Optional parseCLI value form (TerminalColor).
    #[test]
    fn parse_opt_parsecli_value_field() {
        let mut cfg = Config::default();
        cfg.parse_into_field(b"cursor-color", Some(b"#abcdef"))
            .unwrap();
        assert!(cfg.cursor_color.is_some());
    }

    // Empty value resets a parseCLI-self field to its default.
    #[test]
    fn parse_empty_resets_to_default() {
        let mut cfg = Config::default();
        cfg.parse_into_field(b"font-family", Some(b"Monaco"))
            .unwrap();
        assert_eq!(cfg.font_family.list.len(), 1);
        // Empty -> reset to default (empty list).
        cfg.parse_into_field(b"font-family", Some(b"")).unwrap();
        assert_eq!(cfg.font_family.list.len(), 0);
    }

    // The cursor-style bare enum uses the terminal `Style` map.
    #[test]
    fn parse_cursor_style_external_enum() {
        let mut cfg = Config::default();
        cfg.parse_into_field(b"cursor-style", Some(b"bar")).unwrap();
        assert_eq!(cfg.cursor_style, CursorStyle::Bar);
        cfg.parse_into_field(b"cursor-style", Some(b"block_hollow"))
            .unwrap();
        assert_eq!(cfg.cursor_style, CursorStyle::BlockHollow);
    }
}
