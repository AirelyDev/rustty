//! rustty-config::config_struct
//!
//! PORTS: the main `Config` struct of ghostty `src/config/Config.zig` plus
//! `src/config/key.zig` (the `Key` enum, which key.zig derives FROM the Config
//! fields — so it lives here WITH Config, not in cfg-fmt).
//!
//! PORT STATUS: C2 COMPLETE. This module carries:
//!   - the `Config` struct with ALL ~105 public config fields (exact reference
//!     names → Rust idents; hyphens → underscores; `@"_x"` internal fields kept
//!     with a leading `_`), their value-types, and reference defaults via the
//!     `Default` impl;
//!   - the `Key` enum (port of key.zig) — one variant per non-`_` field;
//!   - the `compatibility` rename table entries + handler bodies;
//!   - the four passes (`load`/`default`/`loadIter`/`loadFile`/`finalize`/
//!     `parseManuallyHook`/`clone`/`cloneEmpty`/`changed`), `ChangeIterator`,
//!     `Replay`, and theme application — fully implemented;
//!   - the `Target` impl wiring per-field PARSE/FORMAT/CLONE/CHANGED dispatch.
//!
//! EVERY `test {}` (R1 — the count is a contract) is ported, including the Config
//! integration tests and the key.zig `Value` test. Documented deferrals: (1) the
//! `RepeatableCodepointMap` font-blocked `parseCLI`/`formatConfig` tests (see
//! DEFERRALS.md), which land when `font/CodepointMap.zig` arrives in rustty-font;
//! (2) the per-field Pandoc Markdown `///` doc comments (~3499 reference lines)
//! are still deferred to the field-doc fill sub-batch (see the `Config` struct
//! PORT NOTE) — they are NOT yet ported, so the whole-`Config.zig` G3b
//! comment-coverage gate remains RED until that sub-batch lands.
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
    parse_field_sentinel_string, parse_field_string, Error, PackedStruct, ParseError, Target,
};
use rustty_args::diagnostics::DiagnosticList;
use rustty_cfg_fmt::{
    entry_formatter, format_entry_display, format_entry_enum, format_entry_float,
    format_entry_optional, format_entry_string, format_entry_void, FormatEntry, FormatError,
    Writer,
};

use crate::command::Command;
use crate::conditional::{self, Conditional};
use crate::io::RepeatableReadableIO;
use crate::path::{Path, RepeatablePath};
use crate::repeatable_string_map::RepeatableStringMap;
use crate::value_types::*;

// We do this instead of importing all of terminal/main.zig to
// limit the dependency graph. This is important because some things
// like the `rustty-build-data` binary depend on the Config but don't
// want to include all the other stuff.
// PORT NOTE: terminal.CursorStyle / terminal.color.* come via rustty-vt.
// const terminal = struct { const CursorStyle = ...; const color = ...; ... };
pub use rustty_vt::cursor::Style as CursorStyle;

// const log = std.log.scoped(.config);

// Used on Unixes for some defaults.
// const c = @cImport({ @cInclude("unistd.h"); });
// PORT NOTE: the unistd.h cImport is not needed in the port; retained as a
// comment for fidelity (Unix defaults go via rustty-os / libc).

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
    ScrollbackLimitBytes,
    ScrollbackLimitLines,
    ScrollbackCompression,
    Scrollbar,
    Link,
    LinkUrl,
    LinkOsc8,
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
    DragHandle,
    ResizeOverlay,
    ResizeOverlayPosition,
    ResizeOverlayDuration,
    FocusFollowsMouse,
    ClipboardRead,
    ClipboardWrite,
    ClipboardWriteLimitBytes,
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
    GtkHorizontalTabScroll,
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

// ===========================================================================
// `std.meta.stringToEnum(Key, name)` — the `ghostty_config_get` key lookup
// (CApi.zig:100). Zig derives this at comptime from the `Key` enum field names
// (the hyphenated Config field names); Rust has no enum reflection, so the map
// is written explicitly with one arm per non-`_` Config field. The byte-string
// patterns are the exact Zig field-name spellings (`field_ident` with `_`->`-`),
// identical to the PARSE/FORMAT dispatch keys.
// ===========================================================================
impl Key {
    /// `std.meta.stringToEnum(Key, name)` — maps a hyphenated Config key name to
    /// its `Key` variant, `None` for an unknown name.
    pub fn string_to_enum(name: &[u8]) -> Option<Key> {
        match name {
            b"abnormal-command-exit-runtime" => Some(Key::AbnormalCommandExitRuntime),
            b"adjust-box-thickness" => Some(Key::AdjustBoxThickness),
            b"adjust-cell-height" => Some(Key::AdjustCellHeight),
            b"adjust-cell-width" => Some(Key::AdjustCellWidth),
            b"adjust-cursor-height" => Some(Key::AdjustCursorHeight),
            b"adjust-cursor-thickness" => Some(Key::AdjustCursorThickness),
            b"adjust-font-baseline" => Some(Key::AdjustFontBaseline),
            b"adjust-icon-height" => Some(Key::AdjustIconHeight),
            b"adjust-overline-position" => Some(Key::AdjustOverlinePosition),
            b"adjust-overline-thickness" => Some(Key::AdjustOverlineThickness),
            b"adjust-strikethrough-position" => Some(Key::AdjustStrikethroughPosition),
            b"adjust-strikethrough-thickness" => Some(Key::AdjustStrikethroughThickness),
            b"adjust-underline-position" => Some(Key::AdjustUnderlinePosition),
            b"adjust-underline-thickness" => Some(Key::AdjustUnderlineThickness),
            b"alpha-blending" => Some(Key::AlphaBlending),
            b"app-notifications" => Some(Key::AppNotifications),
            b"async-backend" => Some(Key::AsyncBackend),
            b"auto-update" => Some(Key::AutoUpdate),
            b"auto-update-channel" => Some(Key::AutoUpdateChannel),
            b"background" => Some(Key::Background),
            b"background-blur" => Some(Key::BackgroundBlur),
            b"background-image" => Some(Key::BackgroundImage),
            b"background-image-fit" => Some(Key::BackgroundImageFit),
            b"background-image-opacity" => Some(Key::BackgroundImageOpacity),
            b"background-image-position" => Some(Key::BackgroundImagePosition),
            b"background-image-repeat" => Some(Key::BackgroundImageRepeat),
            b"background-opacity" => Some(Key::BackgroundOpacity),
            b"background-opacity-cells" => Some(Key::BackgroundOpacityCells),
            b"bell-audio-path" => Some(Key::BellAudioPath),
            b"bell-audio-volume" => Some(Key::BellAudioVolume),
            b"bell-features" => Some(Key::BellFeatures),
            b"bold-color" => Some(Key::BoldColor),
            b"class" => Some(Key::Class),
            b"click-repeat-interval" => Some(Key::ClickRepeatInterval),
            b"clipboard-codepoint-map" => Some(Key::ClipboardCodepointMap),
            b"clipboard-paste-bracketed-safe" => Some(Key::ClipboardPasteBracketedSafe),
            b"clipboard-paste-protection" => Some(Key::ClipboardPasteProtection),
            b"clipboard-read" => Some(Key::ClipboardRead),
            b"clipboard-trim-trailing-spaces" => Some(Key::ClipboardTrimTrailingSpaces),
            b"clipboard-write" => Some(Key::ClipboardWrite),
            b"clipboard-write-limit-bytes" => Some(Key::ClipboardWriteLimitBytes),
            b"command" => Some(Key::Command),
            b"command-palette-entry" => Some(Key::CommandPaletteEntry),
            b"config-default-files" => Some(Key::ConfigDefaultFiles),
            b"config-file" => Some(Key::ConfigFile),
            b"confirm-close-surface" => Some(Key::ConfirmCloseSurface),
            b"copy-on-select" => Some(Key::CopyOnSelect),
            b"cursor-click-to-move" => Some(Key::CursorClickToMove),
            b"cursor-color" => Some(Key::CursorColor),
            b"cursor-opacity" => Some(Key::CursorOpacity),
            b"cursor-style" => Some(Key::CursorStyle),
            b"cursor-style-blink" => Some(Key::CursorStyleBlink),
            b"cursor-text" => Some(Key::CursorText),
            b"custom-shader" => Some(Key::CustomShader),
            b"custom-shader-animation" => Some(Key::CustomShaderAnimation),
            b"desktop-notifications" => Some(Key::DesktopNotifications),
            b"drag-handle" => Some(Key::DragHandle),
            b"enquiry-response" => Some(Key::EnquiryResponse),
            b"env" => Some(Key::Env),
            b"faint-opacity" => Some(Key::FaintOpacity),
            b"focus-follows-mouse" => Some(Key::FocusFollowsMouse),
            b"font-codepoint-map" => Some(Key::FontCodepointMap),
            b"font-family" => Some(Key::FontFamily),
            b"font-family-bold" => Some(Key::FontFamilyBold),
            b"font-family-bold-italic" => Some(Key::FontFamilyBoldItalic),
            b"font-family-italic" => Some(Key::FontFamilyItalic),
            b"font-feature" => Some(Key::FontFeature),
            b"font-shaping-break" => Some(Key::FontShapingBreak),
            b"font-size" => Some(Key::FontSize),
            b"font-style" => Some(Key::FontStyle),
            b"font-style-bold" => Some(Key::FontStyleBold),
            b"font-style-bold-italic" => Some(Key::FontStyleBoldItalic),
            b"font-style-italic" => Some(Key::FontStyleItalic),
            b"font-synthetic-style" => Some(Key::FontSyntheticStyle),
            b"font-thicken" => Some(Key::FontThicken),
            b"font-thicken-strength" => Some(Key::FontThickenStrength),
            b"font-variation" => Some(Key::FontVariation),
            b"font-variation-bold" => Some(Key::FontVariationBold),
            b"font-variation-bold-italic" => Some(Key::FontVariationBoldItalic),
            b"font-variation-italic" => Some(Key::FontVariationItalic),
            b"foreground" => Some(Key::Foreground),
            b"freetype-load-flags" => Some(Key::FreetypeLoadFlags),
            b"fullscreen" => Some(Key::Fullscreen),
            b"grapheme-width-method" => Some(Key::GraphemeWidthMethod),
            b"gtk-custom-css" => Some(Key::GtkCustomCss),
            b"gtk-horizontal-tab-scroll" => Some(Key::GtkHorizontalTabScroll),
            b"gtk-opengl-debug" => Some(Key::GtkOpenglDebug),
            b"gtk-quick-terminal-layer" => Some(Key::GtkQuickTerminalLayer),
            b"gtk-quick-terminal-namespace" => Some(Key::GtkQuickTerminalNamespace),
            b"gtk-single-instance" => Some(Key::GtkSingleInstance),
            b"gtk-tabs-location" => Some(Key::GtkTabsLocation),
            b"gtk-titlebar" => Some(Key::GtkTitlebar),
            b"gtk-titlebar-hide-when-maximized" => Some(Key::GtkTitlebarHideWhenMaximized),
            b"gtk-titlebar-style" => Some(Key::GtkTitlebarStyle),
            b"gtk-toolbar-style" => Some(Key::GtkToolbarStyle),
            b"gtk-wide-tabs" => Some(Key::GtkWideTabs),
            b"image-storage-limit" => Some(Key::ImageStorageLimit),
            b"initial-command" => Some(Key::InitialCommand),
            b"initial-window" => Some(Key::InitialWindow),
            b"input" => Some(Key::Input),
            b"key-remap" => Some(Key::KeyRemap),
            b"keybind" => Some(Key::Keybind),
            b"language" => Some(Key::Language),
            b"link" => Some(Key::Link),
            b"link-osc8" => Some(Key::LinkOsc8),
            b"link-previews" => Some(Key::LinkPreviews),
            b"link-url" => Some(Key::LinkUrl),
            b"linux-cgroup" => Some(Key::LinuxCgroup),
            b"linux-cgroup-hard-fail" => Some(Key::LinuxCgroupHardFail),
            b"linux-cgroup-memory-limit" => Some(Key::LinuxCgroupMemoryLimit),
            b"linux-cgroup-processes-limit" => Some(Key::LinuxCgroupProcessesLimit),
            b"macos-applescript" => Some(Key::MacosApplescript),
            b"macos-auto-secure-input" => Some(Key::MacosAutoSecureInput),
            b"macos-custom-icon" => Some(Key::MacosCustomIcon),
            b"macos-dock-drop-behavior" => Some(Key::MacosDockDropBehavior),
            b"macos-hidden" => Some(Key::MacosHidden),
            b"macos-icon" => Some(Key::MacosIcon),
            b"macos-icon-frame" => Some(Key::MacosIconFrame),
            b"macos-icon-ghost-color" => Some(Key::MacosIconGhostColor),
            b"macos-icon-screen-color" => Some(Key::MacosIconScreenColor),
            b"macos-non-native-fullscreen" => Some(Key::MacosNonNativeFullscreen),
            b"macos-option-as-alt" => Some(Key::MacosOptionAsAlt),
            b"macos-secure-input-indication" => Some(Key::MacosSecureInputIndication),
            b"macos-shortcuts" => Some(Key::MacosShortcuts),
            b"macos-titlebar-proxy-icon" => Some(Key::MacosTitlebarProxyIcon),
            b"macos-titlebar-style" => Some(Key::MacosTitlebarStyle),
            b"macos-window-buttons" => Some(Key::MacosWindowButtons),
            b"macos-window-shadow" => Some(Key::MacosWindowShadow),
            b"maximize" => Some(Key::Maximize),
            b"middle-click-action" => Some(Key::MiddleClickAction),
            b"minimum-contrast" => Some(Key::MinimumContrast),
            b"mouse-hide-while-typing" => Some(Key::MouseHideWhileTyping),
            b"mouse-reporting" => Some(Key::MouseReporting),
            b"mouse-scroll-multiplier" => Some(Key::MouseScrollMultiplier),
            b"mouse-shift-capture" => Some(Key::MouseShiftCapture),
            b"notify-on-command-finish" => Some(Key::NotifyOnCommandFinish),
            b"notify-on-command-finish-action" => Some(Key::NotifyOnCommandFinishAction),
            b"notify-on-command-finish-after" => Some(Key::NotifyOnCommandFinishAfter),
            b"osc-color-report-format" => Some(Key::OscColorReportFormat),
            b"palette" => Some(Key::Palette),
            b"palette-generate" => Some(Key::PaletteGenerate),
            b"palette-harmonious" => Some(Key::PaletteHarmonious),
            b"progress-style" => Some(Key::ProgressStyle),
            b"quick-terminal-animation-duration" => Some(Key::QuickTerminalAnimationDuration),
            b"quick-terminal-autohide" => Some(Key::QuickTerminalAutohide),
            b"quick-terminal-keyboard-interactivity" => {
                Some(Key::QuickTerminalKeyboardInteractivity)
            }
            b"quick-terminal-position" => Some(Key::QuickTerminalPosition),
            b"quick-terminal-screen" => Some(Key::QuickTerminalScreen),
            b"quick-terminal-size" => Some(Key::QuickTerminalSize),
            b"quick-terminal-space-behavior" => Some(Key::QuickTerminalSpaceBehavior),
            b"quit-after-last-window-closed" => Some(Key::QuitAfterLastWindowClosed),
            b"quit-after-last-window-closed-delay" => Some(Key::QuitAfterLastWindowClosedDelay),
            b"resize-overlay" => Some(Key::ResizeOverlay),
            b"resize-overlay-duration" => Some(Key::ResizeOverlayDuration),
            b"resize-overlay-position" => Some(Key::ResizeOverlayPosition),
            b"right-click-action" => Some(Key::RightClickAction),
            b"scroll-to-bottom" => Some(Key::ScrollToBottom),
            b"scrollback-limit-bytes" => Some(Key::ScrollbackLimitBytes),
            b"scrollback-limit-lines" => Some(Key::ScrollbackLimitLines),
            b"scrollback-compression" => Some(Key::ScrollbackCompression),
            b"scrollbar" => Some(Key::Scrollbar),
            b"search-background" => Some(Key::SearchBackground),
            b"search-foreground" => Some(Key::SearchForeground),
            b"search-selected-background" => Some(Key::SearchSelectedBackground),
            b"search-selected-foreground" => Some(Key::SearchSelectedForeground),
            b"selection-background" => Some(Key::SelectionBackground),
            b"selection-clear-on-copy" => Some(Key::SelectionClearOnCopy),
            b"selection-clear-on-typing" => Some(Key::SelectionClearOnTyping),
            b"selection-foreground" => Some(Key::SelectionForeground),
            b"selection-word-chars" => Some(Key::SelectionWordChars),
            b"shell-integration" => Some(Key::ShellIntegration),
            b"shell-integration-features" => Some(Key::ShellIntegrationFeatures),
            b"split-divider-color" => Some(Key::SplitDividerColor),
            b"split-inherit-working-directory" => Some(Key::SplitInheritWorkingDirectory),
            b"split-preserve-zoom" => Some(Key::SplitPreserveZoom),
            b"tab-inherit-working-directory" => Some(Key::TabInheritWorkingDirectory),
            b"term" => Some(Key::Term),
            b"theme" => Some(Key::Theme),
            b"title" => Some(Key::Title),
            b"title-report" => Some(Key::TitleReport),
            b"undo-timeout" => Some(Key::UndoTimeout),
            b"unfocused-split-fill" => Some(Key::UnfocusedSplitFill),
            b"unfocused-split-opacity" => Some(Key::UnfocusedSplitOpacity),
            b"vt-kam-allowed" => Some(Key::VtKamAllowed),
            b"wait-after-command" => Some(Key::WaitAfterCommand),
            b"window-colorspace" => Some(Key::WindowColorspace),
            b"window-decoration" => Some(Key::WindowDecoration),
            b"window-height" => Some(Key::WindowHeight),
            b"window-inherit-font-size" => Some(Key::WindowInheritFontSize),
            b"window-inherit-working-directory" => Some(Key::WindowInheritWorkingDirectory),
            b"window-new-tab-position" => Some(Key::WindowNewTabPosition),
            b"window-padding-balance" => Some(Key::WindowPaddingBalance),
            b"window-padding-color" => Some(Key::WindowPaddingColor),
            b"window-padding-x" => Some(Key::WindowPaddingX),
            b"window-padding-y" => Some(Key::WindowPaddingY),
            b"window-position-x" => Some(Key::WindowPositionX),
            b"window-position-y" => Some(Key::WindowPositionY),
            b"window-save-state" => Some(Key::WindowSaveState),
            b"window-show-tab-bar" => Some(Key::WindowShowTabBar),
            b"window-step-resize" => Some(Key::WindowStepResize),
            b"window-subtitle" => Some(Key::WindowSubtitle),
            b"window-theme" => Some(Key::WindowTheme),
            b"window-title-font-family" => Some(Key::WindowTitleFontFamily),
            b"window-titlebar-background" => Some(Key::WindowTitlebarBackground),
            b"window-titlebar-foreground" => Some(Key::WindowTitlebarForeground),
            b"window-vsync" => Some(Key::WindowVsync),
            b"window-width" => Some(Key::WindowWidth),
            b"working-directory" => Some(Key::WorkingDirectory),
            b"x11-instance-name" => Some(Key::X11InstanceName),
            _ => None,
        }
    }
}

// ===========================================================================
// PORTS: src/config/key.zig — the `Value(comptime key: Key) type` helper.
//
// key.zig's `Value` is a comptime function returning the *type* of the Config
// field a `Key` names (it iterates `std.meta.fields(Config)`, matches the field
// whose `@field(Key, name) == key`, and returns `field.type`; the no-match path
// is `unreachable`). Rust functions cannot return types, so the faithful
// rendering returns the field type's `std::any::TypeId` — a runtime token that
// is `==` iff the types are identical, exactly the equality the reference's
// `Value(...)` participates in (e.g. `Value(.@"font-family") == RepeatableString`).
// Every `Key` resolves to its field's `TypeId`; the exhaustive `match` is the
// Rust analogue of the comptime field search (no `unreachable` arm is needed —
// the match covers all variants).
// ===========================================================================

/// PORTS: `pub fn Value(comptime key: Key) type` (key.zig:35).
// pub fn Value(comptime key: Key) type {
//     const field = comptime field: {
//         @setEvalBranchQuota(100_000);
//         const fields = std.meta.fields(Config);
//         for (fields) |field| {
//             if (@field(Key, field.name) == key) {
//                 break :field field;
//             }
//         }
//         unreachable;
//     };
//     return field.type;
// }
pub fn value_type(key: Key) -> std::any::TypeId {
    use std::any::TypeId;
    match key {
        Key::Language => TypeId::of::<Option<Vec<u8>>>(),
        Key::FontFamily => TypeId::of::<RepeatableString>(),
        Key::FontFamilyBold => TypeId::of::<RepeatableString>(),
        Key::FontFamilyItalic => TypeId::of::<RepeatableString>(),
        Key::FontFamilyBoldItalic => TypeId::of::<RepeatableString>(),
        Key::FontStyle => TypeId::of::<FontStyle>(),
        Key::FontStyleBold => TypeId::of::<FontStyle>(),
        Key::FontStyleItalic => TypeId::of::<FontStyle>(),
        Key::FontStyleBoldItalic => TypeId::of::<FontStyle>(),
        Key::FontSyntheticStyle => TypeId::of::<FontSyntheticStyle>(),
        Key::FontFeature => TypeId::of::<RepeatableString>(),
        Key::FontSize => TypeId::of::<f32>(),
        Key::FontVariation => TypeId::of::<RepeatableFontVariation>(),
        Key::FontVariationBold => TypeId::of::<RepeatableFontVariation>(),
        Key::FontVariationItalic => TypeId::of::<RepeatableFontVariation>(),
        Key::FontVariationBoldItalic => TypeId::of::<RepeatableFontVariation>(),
        Key::FontCodepointMap => TypeId::of::<RepeatableCodepointMap>(),
        Key::ClipboardCodepointMap => TypeId::of::<RepeatableClipboardCodepointMap>(),
        Key::FontThicken => TypeId::of::<bool>(),
        Key::FontThickenStrength => TypeId::of::<u8>(),
        Key::FontShapingBreak => TypeId::of::<FontShapingBreak>(),
        Key::AlphaBlending => TypeId::of::<AlphaBlending>(),
        Key::AdjustCellWidth => TypeId::of::<Option<MetricModifier>>(),
        Key::AdjustCellHeight => TypeId::of::<Option<MetricModifier>>(),
        Key::AdjustFontBaseline => TypeId::of::<Option<MetricModifier>>(),
        Key::AdjustUnderlinePosition => TypeId::of::<Option<MetricModifier>>(),
        Key::AdjustUnderlineThickness => TypeId::of::<Option<MetricModifier>>(),
        Key::AdjustStrikethroughPosition => TypeId::of::<Option<MetricModifier>>(),
        Key::AdjustStrikethroughThickness => TypeId::of::<Option<MetricModifier>>(),
        Key::AdjustOverlinePosition => TypeId::of::<Option<MetricModifier>>(),
        Key::AdjustOverlineThickness => TypeId::of::<Option<MetricModifier>>(),
        Key::AdjustCursorThickness => TypeId::of::<Option<MetricModifier>>(),
        Key::AdjustCursorHeight => TypeId::of::<Option<MetricModifier>>(),
        Key::AdjustBoxThickness => TypeId::of::<Option<MetricModifier>>(),
        Key::AdjustIconHeight => TypeId::of::<Option<MetricModifier>>(),
        Key::GraphemeWidthMethod => TypeId::of::<GraphemeWidthMethod>(),
        Key::FreetypeLoadFlags => TypeId::of::<FreetypeLoadFlags>(),
        Key::Theme => TypeId::of::<Option<Theme>>(),
        Key::Background => TypeId::of::<Color>(),
        Key::Foreground => TypeId::of::<Color>(),
        Key::BackgroundImage => TypeId::of::<Option<Path>>(),
        Key::BackgroundImageOpacity => TypeId::of::<f32>(),
        Key::BackgroundImagePosition => TypeId::of::<BackgroundImagePosition>(),
        Key::BackgroundImageFit => TypeId::of::<BackgroundImageFit>(),
        Key::BackgroundImageRepeat => TypeId::of::<bool>(),
        Key::SelectionForeground => TypeId::of::<Option<TerminalColor>>(),
        Key::SelectionBackground => TypeId::of::<Option<TerminalColor>>(),
        Key::SelectionClearOnTyping => TypeId::of::<bool>(),
        Key::SelectionClearOnCopy => TypeId::of::<bool>(),
        Key::SelectionWordChars => TypeId::of::<SelectionWordChars>(),
        Key::MinimumContrast => TypeId::of::<f64>(),
        Key::Palette => TypeId::of::<Palette>(),
        Key::PaletteGenerate => TypeId::of::<bool>(),
        Key::PaletteHarmonious => TypeId::of::<bool>(),
        Key::CursorColor => TypeId::of::<Option<TerminalColor>>(),
        Key::CursorOpacity => TypeId::of::<f64>(),
        Key::CursorStyle => TypeId::of::<CursorStyle>(),
        Key::CursorStyleBlink => TypeId::of::<Option<bool>>(),
        Key::CursorText => TypeId::of::<Option<TerminalColor>>(),
        Key::CursorClickToMove => TypeId::of::<bool>(),
        Key::MouseHideWhileTyping => TypeId::of::<bool>(),
        Key::ScrollToBottom => TypeId::of::<ScrollToBottom>(),
        Key::MouseShiftCapture => TypeId::of::<MouseShiftCapture>(),
        Key::MouseReporting => TypeId::of::<bool>(),
        Key::MouseScrollMultiplier => TypeId::of::<MouseScrollMultiplier>(),
        Key::BackgroundOpacity => TypeId::of::<f64>(),
        Key::BackgroundOpacityCells => TypeId::of::<bool>(),
        Key::BackgroundBlur => TypeId::of::<BackgroundBlur>(),
        Key::UnfocusedSplitOpacity => TypeId::of::<f64>(),
        Key::UnfocusedSplitFill => TypeId::of::<Option<Color>>(),
        Key::SplitDividerColor => TypeId::of::<Option<Color>>(),
        Key::SplitPreserveZoom => TypeId::of::<SplitPreserveZoom>(),
        Key::SearchForeground => TypeId::of::<TerminalColor>(),
        Key::SearchBackground => TypeId::of::<TerminalColor>(),
        Key::SearchSelectedForeground => TypeId::of::<TerminalColor>(),
        Key::SearchSelectedBackground => TypeId::of::<TerminalColor>(),
        Key::Command => TypeId::of::<Option<Command>>(),
        Key::InitialCommand => TypeId::of::<Option<Command>>(),
        Key::NotifyOnCommandFinish => TypeId::of::<NotifyOnCommandFinish>(),
        Key::NotifyOnCommandFinishAction => TypeId::of::<NotifyOnCommandFinishAction>(),
        Key::NotifyOnCommandFinishAfter => TypeId::of::<Duration>(),
        Key::Env => TypeId::of::<RepeatableStringMap>(),
        Key::Input => TypeId::of::<RepeatableReadableIO>(),
        Key::WaitAfterCommand => TypeId::of::<bool>(),
        Key::AbnormalCommandExitRuntime => TypeId::of::<u32>(),
        Key::ScrollbackLimitBytes => TypeId::of::<ScrollbackLimitBytes>(),
        Key::ScrollbackLimitLines => TypeId::of::<ScrollbackLimitLines>(),
        Key::ScrollbackCompression => TypeId::of::<bool>(),
        Key::Scrollbar => TypeId::of::<Scrollbar>(),
        Key::Link => TypeId::of::<RepeatableLink>(),
        Key::LinkUrl => TypeId::of::<bool>(),
        Key::LinkOsc8 => TypeId::of::<bool>(),
        Key::LinkPreviews => TypeId::of::<LinkPreviews>(),
        Key::Maximize => TypeId::of::<bool>(),
        Key::Fullscreen => TypeId::of::<Fullscreen>(),
        Key::Title => TypeId::of::<Option<Vec<u8>>>(),
        Key::Class => TypeId::of::<Option<Vec<u8>>>(),
        Key::X11InstanceName => TypeId::of::<Option<Vec<u8>>>(),
        Key::WorkingDirectory => TypeId::of::<Option<WorkingDirectory>>(),
        Key::Keybind => TypeId::of::<Keybinds>(),
        Key::KeyRemap => TypeId::of::<KeyRemapSet>(),
        Key::WindowPaddingX => TypeId::of::<WindowPadding>(),
        Key::WindowPaddingY => TypeId::of::<WindowPadding>(),
        Key::WindowPaddingBalance => TypeId::of::<WindowPaddingBalance>(),
        Key::WindowPaddingColor => TypeId::of::<WindowPaddingColor>(),
        Key::WindowVsync => TypeId::of::<bool>(),
        Key::WindowInheritWorkingDirectory => TypeId::of::<bool>(),
        Key::TabInheritWorkingDirectory => TypeId::of::<bool>(),
        Key::SplitInheritWorkingDirectory => TypeId::of::<bool>(),
        Key::WindowInheritFontSize => TypeId::of::<bool>(),
        Key::WindowDecoration => TypeId::of::<WindowDecoration>(),
        Key::WindowTitleFontFamily => TypeId::of::<Option<Vec<u8>>>(),
        Key::WindowSubtitle => TypeId::of::<WindowSubtitle>(),
        Key::WindowTheme => TypeId::of::<WindowTheme>(),
        Key::WindowColorspace => TypeId::of::<WindowColorspace>(),
        Key::WindowHeight => TypeId::of::<u32>(),
        Key::WindowWidth => TypeId::of::<u32>(),
        Key::WindowPositionX => TypeId::of::<Option<i16>>(),
        Key::WindowPositionY => TypeId::of::<Option<i16>>(),
        Key::WindowSaveState => TypeId::of::<WindowSaveState>(),
        Key::WindowStepResize => TypeId::of::<bool>(),
        Key::WindowNewTabPosition => TypeId::of::<WindowNewTabPosition>(),
        Key::WindowShowTabBar => TypeId::of::<WindowShowTabBar>(),
        Key::WindowTitlebarBackground => TypeId::of::<Option<Color>>(),
        Key::WindowTitlebarForeground => TypeId::of::<Option<Color>>(),
        Key::DragHandle => TypeId::of::<DragHandle>(),
        Key::ResizeOverlay => TypeId::of::<ResizeOverlay>(),
        Key::ResizeOverlayPosition => TypeId::of::<ResizeOverlayPosition>(),
        Key::ResizeOverlayDuration => TypeId::of::<Duration>(),
        Key::FocusFollowsMouse => TypeId::of::<bool>(),
        Key::ClipboardRead => TypeId::of::<ClipboardAccess>(),
        Key::ClipboardWrite => TypeId::of::<ClipboardAccess>(),
        Key::ClipboardWriteLimitBytes => TypeId::of::<ClipboardWriteLimitBytes>(),
        Key::ClipboardTrimTrailingSpaces => TypeId::of::<bool>(),
        Key::ClipboardPasteProtection => TypeId::of::<bool>(),
        Key::ClipboardPasteBracketedSafe => TypeId::of::<bool>(),
        Key::TitleReport => TypeId::of::<bool>(),
        Key::ImageStorageLimit => TypeId::of::<u32>(),
        Key::CopyOnSelect => TypeId::of::<CopyOnSelect>(),
        Key::RightClickAction => TypeId::of::<RightClickAction>(),
        Key::MiddleClickAction => TypeId::of::<MiddleClickAction>(),
        Key::ClickRepeatInterval => TypeId::of::<u32>(),
        Key::ConfigFile => TypeId::of::<RepeatablePath>(),
        Key::ConfigDefaultFiles => TypeId::of::<bool>(),
        Key::ConfirmCloseSurface => TypeId::of::<ConfirmCloseSurface>(),
        Key::QuitAfterLastWindowClosed => TypeId::of::<bool>(),
        Key::QuitAfterLastWindowClosedDelay => TypeId::of::<Option<Duration>>(),
        Key::InitialWindow => TypeId::of::<bool>(),
        Key::UndoTimeout => TypeId::of::<Duration>(),
        Key::QuickTerminalPosition => TypeId::of::<QuickTerminalPosition>(),
        Key::QuickTerminalSize => TypeId::of::<QuickTerminalSize>(),
        Key::GtkQuickTerminalLayer => TypeId::of::<QuickTerminalLayer>(),
        Key::GtkQuickTerminalNamespace => TypeId::of::<Vec<u8>>(),
        Key::QuickTerminalScreen => TypeId::of::<QuickTerminalScreen>(),
        Key::QuickTerminalAnimationDuration => TypeId::of::<f64>(),
        Key::QuickTerminalAutohide => TypeId::of::<bool>(),
        Key::QuickTerminalSpaceBehavior => TypeId::of::<QuickTerminalSpaceBehavior>(),
        Key::QuickTerminalKeyboardInteractivity => {
            TypeId::of::<QuickTerminalKeyboardInteractivity>()
        }
        Key::ShellIntegration => TypeId::of::<ShellIntegration>(),
        Key::ShellIntegrationFeatures => TypeId::of::<ShellIntegrationFeatures>(),
        Key::CommandPaletteEntry => TypeId::of::<RepeatableCommand>(),
        Key::OscColorReportFormat => TypeId::of::<OSCColorReportFormat>(),
        Key::VtKamAllowed => TypeId::of::<bool>(),
        Key::CustomShader => TypeId::of::<RepeatablePath>(),
        Key::CustomShaderAnimation => TypeId::of::<CustomShaderAnimation>(),
        Key::BellFeatures => TypeId::of::<BellFeatures>(),
        Key::BellAudioPath => TypeId::of::<Option<Path>>(),
        Key::BellAudioVolume => TypeId::of::<f64>(),
        Key::AppNotifications => TypeId::of::<AppNotifications>(),
        Key::MacosNonNativeFullscreen => TypeId::of::<NonNativeFullscreen>(),
        Key::MacosWindowButtons => TypeId::of::<MacWindowButtons>(),
        Key::MacosTitlebarStyle => TypeId::of::<MacTitlebarStyle>(),
        Key::MacosTitlebarProxyIcon => TypeId::of::<MacTitlebarProxyIcon>(),
        Key::MacosDockDropBehavior => TypeId::of::<MacOSDockDropBehavior>(),
        Key::MacosOptionAsAlt => TypeId::of::<Option<rustty_input::config::OptionAsAlt>>(),
        Key::MacosWindowShadow => TypeId::of::<bool>(),
        Key::MacosHidden => TypeId::of::<MacHidden>(),
        Key::MacosAutoSecureInput => TypeId::of::<bool>(),
        Key::MacosSecureInputIndication => TypeId::of::<bool>(),
        Key::MacosApplescript => TypeId::of::<bool>(),
        Key::MacosIcon => TypeId::of::<MacAppIcon>(),
        Key::MacosCustomIcon => TypeId::of::<Option<Vec<u8>>>(),
        Key::MacosIconFrame => TypeId::of::<MacAppIconFrame>(),
        Key::MacosIconGhostColor => TypeId::of::<Option<Color>>(),
        Key::MacosIconScreenColor => TypeId::of::<Option<ColorList>>(),
        Key::MacosShortcuts => TypeId::of::<MacShortcuts>(),
        Key::LinuxCgroup => TypeId::of::<LinuxCgroup>(),
        Key::LinuxCgroupMemoryLimit => TypeId::of::<Option<u64>>(),
        Key::LinuxCgroupProcessesLimit => TypeId::of::<Option<u64>>(),
        Key::LinuxCgroupHardFail => TypeId::of::<bool>(),
        Key::GtkOpenglDebug => TypeId::of::<bool>(),
        Key::GtkSingleInstance => TypeId::of::<GtkSingleInstance>(),
        Key::GtkTitlebar => TypeId::of::<bool>(),
        Key::GtkTabsLocation => TypeId::of::<GtkTabsLocation>(),
        Key::GtkTitlebarHideWhenMaximized => TypeId::of::<bool>(),
        Key::GtkToolbarStyle => TypeId::of::<GtkToolbarStyle>(),
        Key::GtkTitlebarStyle => TypeId::of::<GtkTitlebarStyle>(),
        Key::GtkWideTabs => TypeId::of::<bool>(),
        Key::GtkHorizontalTabScroll => TypeId::of::<bool>(),
        Key::GtkCustomCss => TypeId::of::<RepeatablePath>(),
        Key::DesktopNotifications => TypeId::of::<bool>(),
        Key::ProgressStyle => TypeId::of::<bool>(),
        Key::BoldColor => TypeId::of::<Option<BoldColor>>(),
        Key::FaintOpacity => TypeId::of::<f64>(),
        Key::Term => TypeId::of::<Vec<u8>>(),
        Key::EnquiryResponse => TypeId::of::<Vec<u8>>(),
        Key::AsyncBackend => TypeId::of::<AsyncBackend>(),
        Key::AutoUpdate => TypeId::of::<Option<AutoUpdate>>(),
        Key::AutoUpdateChannel => TypeId::of::<Option<rustty_build_config::ReleaseChannel>>(),
    }
}

/// The ordered `(cli-name, Key)` config-field table — the reference's
/// `@typeInfo(Config).@"struct".fields` iteration in declaration order, already
/// `_`-filtered (internals are excluded at the const's construction). This is the
/// public accessor the shell-completion / editor-syntax generators (`rustty-extra`)
/// iterate, exactly as ghostty's `extra/*.zig` iterate the struct fields.
pub fn config_fields() -> &'static [(&'static [u8], Key)] {
    CONFIG_FIELDS
}

// ===========================================================================
// Shell-completion field classification (reify `@typeInfo(field.type)` switch).
//
// The `extra/bash.zig` `_handle_config`, `extra/zsh.zig` `_config`, and
// `extra/fish.zig` top loop each switch on the config field's type to decide what
// value-completion to emit. Rust has no `@typeInfo`, so `config_completion_kind`
// reifies that switch into a per-`Key` classifier — the direct analog of
// `value_type` (which reifies `Config.Value`). The generators share this one
// classifier. The name special-cases (`font-family*` / `theme` /
// `working-directory`) stay generator-local, matching ghostty's name-before-type
// order.
// ===========================================================================

/// The completion-relevant classification of a `Config` field's type, reifying
/// the reference `switch (@typeInfo(field.type))` (with the `field.type ==
/// Config.RepeatablePath` pre-check) that drives value-completion emission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigCompletionKind {
    /// `.bool` — the flag has no value to complete (`return ;;` / no `-a`).
    Bool,
    /// `.@"enum"` — complete each variant name (`info.fields |f| f.name`).
    Enum(&'static [&'static [u8]]),
    /// packed bool struct with no `parseCLI` — complete `name`/`no-name` per flag.
    PackedFlags(&'static [&'static [u8]]),
    /// `field.type == Config.RepeatablePath` — complete file paths.
    RepeatablePath,
    /// Everything else (struct-with-`parseCLI`, non-packed struct, `?bool`, int,
    /// float, color, path, string, optional-enum, …) — no value-completion.
    Other,
}

// The value-name tables for the config enums NOT built via `bare_enum_names!`
// (which now emits its own `VALUE_NAMES`). Each is `@typeInfo(E).@"enum".fields
// |f| f.name` in declaration order; spellings are the format/parse-fidelity-gated
// `@tagName` literals (mirrors the free `to_name` fns next to each type).
const ALPHA_BLENDING_VALUE_NAMES: &[&[u8]] = &[b"native", b"linear", b"linear-corrected"];
const BACKGROUND_IMAGE_POSITION_VALUE_NAMES: &[&[u8]] = &[
    b"top-left",
    b"top-center",
    b"top-right",
    b"center-left",
    b"center-center",
    b"center-right",
    b"bottom-left",
    b"bottom-center",
    b"bottom-right",
    b"center",
];
const BACKGROUND_IMAGE_FIT_VALUE_NAMES: &[&[u8]] = &[b"contain", b"cover", b"stretch", b"none"];
const CURSOR_STYLE_VALUE_NAMES: &[&[u8]] = &[b"bar", b"block", b"underline", b"block_hollow"];
const WINDOW_PADDING_BALANCE_VALUE_NAMES: &[&[u8]] = &[b"false", b"true", b"equal"];
const WINDOW_PADDING_COLOR_VALUE_NAMES: &[&[u8]] = &[b"background", b"extend", b"extend-always"];
const WINDOW_DECORATION_VALUE_NAMES: &[&[u8]] = &[b"auto", b"client", b"server", b"none"];
const WINDOW_COLORSPACE_VALUE_NAMES: &[&[u8]] = &[b"srgb", b"display-p3"];

/// Classify a config field for shell completion — the runtime analog of the
/// reference's `field.type == Config.RepeatablePath` pre-check plus the
/// `switch (@typeInfo(field.type))` (`.bool` / `.@"enum"` / packed-`.@"struct"` /
/// else). `Bool` and `RepeatablePath` are decided by `value_type` equality (the
/// `field.type` identity tests); the enum/packed cases pull their ordered name
/// lists from `E::VALUE_NAMES` / `P::field_names()`; everything else is `Other`
/// (incl. `?bool`, whose `@typeInfo` is `.optional`, not `.bool`).
pub fn config_completion_kind(key: Key) -> ConfigCompletionKind {
    use std::any::TypeId;

    // `field.type == Config.RepeatablePath` (exact type identity, before the
    // `@typeInfo` switch).
    if value_type(key) == TypeId::of::<RepeatablePath>() {
        return ConfigCompletionKind::RepeatablePath;
    }
    // `.bool` — exactly `bool` (`?bool` is `.optional` → falls through to `Other`).
    if value_type(key) == TypeId::of::<bool>() {
        return ConfigCompletionKind::Bool;
    }

    match key {
        // `.@"enum"` fields — complete the variant names.
        Key::AlphaBlending => ConfigCompletionKind::Enum(ALPHA_BLENDING_VALUE_NAMES),
        Key::GraphemeWidthMethod => ConfigCompletionKind::Enum(GraphemeWidthMethod::VALUE_NAMES),
        Key::BackgroundImagePosition => {
            ConfigCompletionKind::Enum(BACKGROUND_IMAGE_POSITION_VALUE_NAMES)
        }
        Key::BackgroundImageFit => ConfigCompletionKind::Enum(BACKGROUND_IMAGE_FIT_VALUE_NAMES),
        Key::CursorStyle => ConfigCompletionKind::Enum(CURSOR_STYLE_VALUE_NAMES),
        Key::MouseShiftCapture => ConfigCompletionKind::Enum(MouseShiftCapture::VALUE_NAMES),
        Key::Scrollbar => ConfigCompletionKind::Enum(Scrollbar::VALUE_NAMES),
        Key::LinkPreviews => ConfigCompletionKind::Enum(LinkPreviews::VALUE_NAMES),
        Key::NotifyOnCommandFinish => {
            ConfigCompletionKind::Enum(NotifyOnCommandFinish::VALUE_NAMES)
        }
        Key::Fullscreen => ConfigCompletionKind::Enum(Fullscreen::VALUE_NAMES),
        Key::WindowPaddingBalance => ConfigCompletionKind::Enum(WINDOW_PADDING_BALANCE_VALUE_NAMES),
        Key::WindowPaddingColor => ConfigCompletionKind::Enum(WINDOW_PADDING_COLOR_VALUE_NAMES),
        Key::WindowDecoration => ConfigCompletionKind::Enum(WINDOW_DECORATION_VALUE_NAMES),
        Key::WindowSubtitle => ConfigCompletionKind::Enum(WindowSubtitle::VALUE_NAMES),
        Key::WindowTheme => ConfigCompletionKind::Enum(WindowTheme::VALUE_NAMES),
        Key::WindowColorspace => ConfigCompletionKind::Enum(WINDOW_COLORSPACE_VALUE_NAMES),
        Key::WindowSaveState => ConfigCompletionKind::Enum(WindowSaveState::VALUE_NAMES),
        Key::WindowNewTabPosition => ConfigCompletionKind::Enum(WindowNewTabPosition::VALUE_NAMES),
        Key::WindowShowTabBar => ConfigCompletionKind::Enum(WindowShowTabBar::VALUE_NAMES),
        Key::DragHandle => ConfigCompletionKind::Enum(DragHandle::VALUE_NAMES),
        Key::ResizeOverlay => ConfigCompletionKind::Enum(ResizeOverlay::VALUE_NAMES),
        Key::ResizeOverlayPosition => {
            ConfigCompletionKind::Enum(ResizeOverlayPosition::VALUE_NAMES)
        }
        Key::ClipboardRead => ConfigCompletionKind::Enum(ClipboardAccess::VALUE_NAMES),
        Key::ClipboardWrite => ConfigCompletionKind::Enum(ClipboardAccess::VALUE_NAMES),
        Key::CopyOnSelect => ConfigCompletionKind::Enum(CopyOnSelect::VALUE_NAMES),
        Key::RightClickAction => ConfigCompletionKind::Enum(RightClickAction::VALUE_NAMES),
        Key::MiddleClickAction => ConfigCompletionKind::Enum(MiddleClickAction::VALUE_NAMES),
        Key::ConfirmCloseSurface => ConfigCompletionKind::Enum(ConfirmCloseSurface::VALUE_NAMES),
        Key::QuickTerminalPosition => {
            ConfigCompletionKind::Enum(QuickTerminalPosition::VALUE_NAMES)
        }
        Key::GtkQuickTerminalLayer => ConfigCompletionKind::Enum(QuickTerminalLayer::VALUE_NAMES),
        Key::QuickTerminalScreen => ConfigCompletionKind::Enum(QuickTerminalScreen::VALUE_NAMES),
        Key::QuickTerminalSpaceBehavior => {
            ConfigCompletionKind::Enum(QuickTerminalSpaceBehavior::VALUE_NAMES)
        }
        Key::QuickTerminalKeyboardInteractivity => {
            ConfigCompletionKind::Enum(QuickTerminalKeyboardInteractivity::VALUE_NAMES)
        }
        Key::ShellIntegration => ConfigCompletionKind::Enum(ShellIntegration::VALUE_NAMES),
        Key::OscColorReportFormat => ConfigCompletionKind::Enum(OSCColorReportFormat::VALUE_NAMES),
        Key::CustomShaderAnimation => {
            ConfigCompletionKind::Enum(CustomShaderAnimation::VALUE_NAMES)
        }
        Key::MacosNonNativeFullscreen => {
            ConfigCompletionKind::Enum(NonNativeFullscreen::VALUE_NAMES)
        }
        Key::MacosWindowButtons => ConfigCompletionKind::Enum(MacWindowButtons::VALUE_NAMES),
        Key::MacosTitlebarStyle => ConfigCompletionKind::Enum(MacTitlebarStyle::VALUE_NAMES),
        Key::MacosTitlebarProxyIcon => {
            ConfigCompletionKind::Enum(MacTitlebarProxyIcon::VALUE_NAMES)
        }
        Key::MacosDockDropBehavior => {
            ConfigCompletionKind::Enum(MacOSDockDropBehavior::VALUE_NAMES)
        }
        Key::MacosHidden => ConfigCompletionKind::Enum(MacHidden::VALUE_NAMES),
        Key::MacosIcon => ConfigCompletionKind::Enum(MacAppIcon::VALUE_NAMES),
        Key::MacosIconFrame => ConfigCompletionKind::Enum(MacAppIconFrame::VALUE_NAMES),
        Key::MacosShortcuts => ConfigCompletionKind::Enum(MacShortcuts::VALUE_NAMES),
        Key::LinuxCgroup => ConfigCompletionKind::Enum(LinuxCgroup::VALUE_NAMES),
        Key::GtkSingleInstance => ConfigCompletionKind::Enum(GtkSingleInstance::VALUE_NAMES),
        Key::GtkTabsLocation => ConfigCompletionKind::Enum(GtkTabsLocation::VALUE_NAMES),
        Key::GtkToolbarStyle => ConfigCompletionKind::Enum(GtkToolbarStyle::VALUE_NAMES),
        Key::GtkTitlebarStyle => ConfigCompletionKind::Enum(GtkTitlebarStyle::VALUE_NAMES),
        Key::AsyncBackend => ConfigCompletionKind::Enum(AsyncBackend::VALUE_NAMES),

        // packed bool structs (no `parseCLI`) — complete `name`/`no-name` flags.
        Key::FontSyntheticStyle => {
            ConfigCompletionKind::PackedFlags(FontSyntheticStyle::field_names())
        }
        Key::FontShapingBreak => {
            ConfigCompletionKind::PackedFlags(FontShapingBreakParse::field_names())
        }
        Key::FreetypeLoadFlags => {
            ConfigCompletionKind::PackedFlags(FreetypeLoadFlags::field_names())
        }
        Key::ScrollToBottom => ConfigCompletionKind::PackedFlags(ScrollToBottom::field_names()),
        Key::SplitPreserveZoom => {
            ConfigCompletionKind::PackedFlags(SplitPreserveZoom::field_names())
        }
        Key::NotifyOnCommandFinishAction => {
            ConfigCompletionKind::PackedFlags(NotifyOnCommandFinishAction::field_names())
        }
        Key::ShellIntegrationFeatures => {
            ConfigCompletionKind::PackedFlags(ShellIntegrationFeatures::field_names())
        }
        Key::BellFeatures => ConfigCompletionKind::PackedFlags(BellFeatures::field_names()),
        Key::AppNotifications => ConfigCompletionKind::PackedFlags(AppNotifications::field_names()),

        // Everything else — no value-completion (the `return ;;` / `( )` / blank arm).
        _ => ConfigCompletionKind::Other,
    }
}

// PORTS: the `test "Value"` block in src/config/key.zig (key.zig:52).
#[cfg(test)]
mod key_tests {
    use super::*;
    use std::any::TypeId;

    // test "Value" {
    #[test]
    fn value() {
        // try testing.expectEqual(Config.RepeatableString, Value(.@"font-family"));
        assert_eq!(
            TypeId::of::<RepeatableString>(),
            value_type(Key::FontFamily)
        );
        // try testing.expectEqual(?bool, Value(.@"cursor-style-blink"));
        assert_eq!(
            TypeId::of::<Option<bool>>(),
            value_type(Key::CursorStyleBlink)
        );
    }
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
    /// Set Rustty's graphical user interface language to a language other than the
    /// system default language. For example:
    ///
    ///     language = de
    ///
    /// will force the strings in Rustty's graphical user interface to be in German
    /// rather than the system default.
    ///
    /// This will not affect the language used by programs run _within_ Rustty.
    /// Those will continue to use the default system language. There are also many
    /// non-GUI elements in Rustty that are not translated - this setting will have
    /// no effect on those.
    ///
    /// Warning: This setting cannot be reloaded at runtime. To change the language
    /// you must fully restart Rustty.
    ///
    /// GTK only.
    /// Available since 1.3.0.
    pub language: Option<Vec<u8>>,

    // @"font-family": RepeatableString = .{},
    /// The font families to use.
    ///
    /// You can generate the list of valid values using the CLI:
    ///
    ///     rustty +list-fonts
    ///
    /// This configuration can be repeated multiple times to specify preferred
    /// fallback fonts when the requested codepoint is not available in the primary
    /// font. This is particularly useful for multiple languages, symbolic fonts,
    /// etc.
    ///
    /// Notes on emoji specifically: On macOS, Rustty by default will always use
    /// Apple Color Emoji and on Linux will always use Noto Emoji. You can
    /// override this behavior by specifying a font family here that contains
    /// emoji glyphs.
    ///
    /// The specific styles (bold, italic, bold italic) do not need to be
    /// explicitly set. If a style is not set, then the regular style (font-family)
    /// will be searched for stylistic variants. If a stylistic variant is not
    /// found, Rustty will use the regular style. This prevents falling back to a
    /// different font family just to get a style such as bold. This also applies
    /// if you explicitly specify a font family for a style. For example, if you
    /// set `font-family-bold = FooBar` and "FooBar" cannot be found, Rustty will
    /// use whatever font is set for `font-family` for the bold style.
    ///
    /// Finally, some styles may be synthesized if they are not supported.
    /// For example, if a font does not have an italic style and no alternative
    /// italic font is specified, Rustty will synthesize an italic style by
    /// applying a slant to the regular style. If you want to disable these
    /// synthesized styles then you can use the `font-style` configurations
    /// as documented below.
    ///
    /// You can disable styles completely by using the `font-style` set of
    /// configurations. See the documentation for `font-style` for more information.
    ///
    /// If you want to overwrite a previous set value rather than append a fallback,
    /// specify the value as `""` (empty string) to reset the list and then set the
    /// new values. For example:
    ///
    ///     font-family = ""
    ///     font-family = "My Favorite Font"
    ///
    /// Setting any of these as CLI arguments will automatically clear the
    /// values set in configuration files so you don't need to specify
    /// `--font-family=""` before setting a new value. You only need to specify
    /// this within config files if you want to clear previously set values in
    /// configuration files or on the CLI if you want to clear values set on the
    /// CLI.
    pub font_family: RepeatableString,
    pub font_family_bold: RepeatableString,
    pub font_family_italic: RepeatableString,
    pub font_family_bold_italic: RepeatableString,

    // @"font-style": FontStyle = .{ .default = {} },
    /// The named font style to use for each of the requested terminal font styles.
    /// This looks up the style based on the font style string advertised by the
    /// font itself. For example, "Iosevka Heavy" has a style of "Heavy".
    ///
    /// You can also use these fields to completely disable a font style. If you set
    /// the value of the configuration below to literal `false` then that font style
    /// will be disabled. If the running program in the terminal requests a disabled
    /// font style, the regular font style will be used instead.
    ///
    /// These are only valid if its corresponding font-family is also specified. If
    /// no font-family is specified, then the font-style is ignored unless you're
    /// disabling the font style.
    pub font_style: FontStyle,
    pub font_style_bold: FontStyle,
    pub font_style_italic: FontStyle,
    pub font_style_bold_italic: FontStyle,

    // @"font-synthetic-style": FontSyntheticStyle = .{},
    /// Control whether Rustty should synthesize a style if the requested style is
    /// not available in the specified font-family.
    ///
    /// Rustty can synthesize bold, italic, and bold italic styles if the font
    /// does not have a specific style. For bold, this is done by drawing an
    /// outline around the glyph of varying thickness. For italic, this is done by
    /// applying a slant to the glyph. For bold italic, both of these are applied.
    ///
    /// Synthetic styles are not perfect and will generally not look as good
    /// as a font that has the style natively. However, they are useful to
    /// provide styled text when the font does not have the style.
    ///
    /// Set this to "false" or "true" to disable or enable synthetic styles
    /// completely. You can disable specific styles using "no-bold", "no-italic",
    /// and "no-bold-italic". You can disable multiple styles by separating them
    /// with a comma. For example, "no-bold,no-italic".
    ///
    /// Available style keys are: `bold`, `italic`, `bold-italic`.
    ///
    /// If synthetic styles are disabled, then the regular style will be used
    /// instead if the requested style is not available. If the font has the
    /// requested style, then the font will be used as-is since the style is
    /// not synthetic.
    ///
    /// Warning: An easy mistake is to disable `bold` or `italic` but not
    /// `bold-italic`. Disabling only `bold` or `italic` will NOT disable either
    /// in the `bold-italic` style. If you want to disable `bold-italic`, you must
    /// explicitly disable it. You cannot partially disable `bold-italic`.
    ///
    /// By default, synthetic styles are enabled.
    pub font_synthetic_style: FontSyntheticStyle,

    // @"font-feature": RepeatableString = .{},
    /// Apply a font feature. To enable multiple font features you can repeat
    /// this multiple times or use a comma-separated list of feature settings.
    ///
    /// The syntax for feature settings is as follows, where `feat` is a feature:
    ///
    ///   * Enable features with e.g. `feat`, `+feat`, `feat on`, `feat=1`.
    ///   * Disabled features with e.g. `-feat`, `feat off`, `feat=0`.
    ///   * Set a feature value with e.g. `feat=2`, `feat = 3`, `feat 4`.
    ///   * Feature names may be wrapped in quotes, meaning this config should be
    ///     syntactically compatible with the `font-feature-settings` CSS property.
    ///
    /// The syntax is fairly loose, but invalid settings will be silently ignored.
    ///
    /// The font feature will apply to all fonts rendered by Rustty. A future
    /// enhancement will allow targeting specific faces.
    ///
    /// To disable programming ligatures, use `-calt` since this is the typical
    /// feature name for programming ligatures. To look into what font features
    /// your font has and what they do, use a font inspection tool such as
    /// [fontdrop.info](https://fontdrop.info).
    ///
    /// To generally disable most ligatures, use `-calt, -liga, -dlig`.
    pub font_feature: RepeatableString,

    // @"font-size": f32 = switch (builtin.os.tag) {
    //     // On macOS we default a little bigger since this tends to look better. This
    //     // is purely subjective but this is easy to modify.
    //     .macos => 13,
    //     else => 12,
    // },
    /// Font size in points. This value can be a non-integer and the nearest integer
    /// pixel size will be selected. If you have a high dpi display where 1pt = 2px
    /// then you can get an odd numbered pixel size by specifying a half point.
    ///
    /// For example, 13.5pt @ 2px/pt = 27px
    ///
    /// Changing this configuration at runtime will only affect existing
    /// terminals that have NOT manually adjusted their font size in some way
    /// (e.g. increased or decreased the font size). Terminals that have manually
    /// adjusted their font size will retain their manually adjusted size.
    /// Otherwise, the font size of existing terminals will be updated on
    /// reload.
    ///
    /// On Linux with GTK, font size is scaled according to both display-wide and
    /// text-specific scaling factors, which are often managed by your desktop
    /// environment (e.g. the GNOME display scale and large text settings).
    pub font_size: f32,

    // @"font-variation": RepeatableFontVariation = .{},
    /// A repeatable configuration to set one or more font variations values for
    /// a variable font. A variable font is a single font, usually with a filename
    /// ending in `-VF.ttf` or `-VF.otf` that contains one or more configurable axes
    /// for things such as weight, slant, etc. Not all fonts support variations;
    /// only fonts that explicitly state they are variable fonts will work.
    ///
    /// The format of this is `id=value` where `id` is the axis identifier. An axis
    /// identifier is always a 4 character string, such as `wght`. To get the list
    /// of supported axes, look at your font documentation or use a font inspection
    /// tool.
    ///
    /// Invalid ids and values are usually ignored. For example, if a font only
    /// supports weights from 100 to 700, setting `wght=800` will do nothing (it
    /// will not be clamped to 700). You must consult your font's documentation to
    /// see what values are supported.
    ///
    /// Common axes are: `wght` (weight), `slnt` (slant), `ital` (italic), `opsz`
    /// (optical size), `wdth` (width), `GRAD` (gradient), etc.
    pub font_variation: RepeatableFontVariation,
    pub font_variation_bold: RepeatableFontVariation,
    pub font_variation_italic: RepeatableFontVariation,
    pub font_variation_bold_italic: RepeatableFontVariation,

    // @"font-codepoint-map": RepeatableCodepointMap = .{},
    /// Force one or a range of Unicode codepoints to map to a specific named font.
    /// This is useful if you want to support special symbols or if you want to use
    /// specific glyphs that render better for your specific font.
    ///
    /// The syntax is `codepoint=fontname` where `codepoint` is either a single
    /// codepoint or a range. Codepoints must be specified as full Unicode
    /// hex values, such as `U+ABCD`. Codepoints ranges are specified as
    /// `U+ABCD-U+DEFG`. You can specify multiple ranges for the same font separated
    /// by commas, such as `U+ABCD-U+DEFG,U+1234-U+5678=fontname`. The font name is
    /// the same value as you would use for `font-family`.
    ///
    /// This configuration can be repeated multiple times to specify multiple
    /// codepoint mappings.
    ///
    /// Changing this configuration at runtime will only affect new terminals,
    /// i.e. new windows, tabs, etc.
    pub font_codepoint_map: RepeatableCodepointMap,

    // @"clipboard-codepoint-map": RepeatableClipboardCodepointMap = .{},
    /// Map specific Unicode codepoints to replacement values when copying text
    /// to clipboard.
    ///
    /// This configuration allows you to replace specific Unicode characters with
    /// other characters or strings when copying terminal content to the clipboard.
    /// This is useful for converting special terminal symbols to more compatible
    /// characters for pasting into other applications.
    ///
    /// The syntax is similar to `font-codepoint-map`:
    /// - Single codepoint: `U+1234=U+ABCD` or `U+1234=replacement_text`
    /// - Codepoint range: `U+1234-U+5678=U+ABCD`
    ///
    /// Examples:
    /// - `clipboard-codepoint-map = U+2500=U+002D` (box drawing horizontal → hyphen)
    /// - `clipboard-codepoint-map = U+2502=U+007C` (box drawing vertical → pipe)
    /// - `clipboard-codepoint-map = U+03A3=SUM` (Greek sigma → "SUM")
    ///
    /// This configuration can be repeated multiple times to specify multiple
    /// mappings. Later entries take priority over earlier ones for overlapping
    /// ranges.
    ///
    /// Note: This only applies to text copying operations, not URL copying.
    pub clipboard_codepoint_map: RepeatableClipboardCodepointMap,

    // @"font-thicken": bool = false,
    /// Draw fonts with a thicker stroke, if supported.
    /// This is currently only supported on macOS.
    pub font_thicken: bool,
    // @"font-thicken-strength": u8 = 255,
    /// Strength of thickening when `font-thicken` is enabled.
    ///
    /// Valid values are integers between `0` and `255`. `0` does not correspond to
    /// *no* thickening, rather it corresponds to the lightest available thickening.
    ///
    /// Has no effect when `font-thicken` is set to `false`.
    ///
    /// This is currently only supported on macOS.
    pub font_thicken_strength: u8,

    // @"font-shaping-break": FontShapingBreak = .{},
    /// Locations to break font shaping into multiple runs.
    ///
    /// A "run" is a contiguous segment of text that is shaped together. "Shaping"
    /// is the process of converting text (codepoints) into glyphs (renderable
    /// characters). This is how ligatures are formed, among other things.
    /// For example, if a coding font turns "!=" into a single glyph, then it
    /// must see "!" and "=" next to each other in a single run. When a run
    /// is broken, the text is shaped separately. To continue our example, if
    /// "!" is at the end of one run and "=" is at the start of the next run,
    /// then the ligature will not be formed.
    ///
    /// Rustty breaks runs at certain points to improve readability or usability.
    /// For example, Rustty by default will break runs under the cursor so that
    /// text editing can see the individual characters rather than a ligature.
    /// This configuration lets you configure this behavior.
    ///
    /// Combine values with a comma to set multiple options. Prefix an
    /// option with "no-" to disable it. Enabling and disabling options
    /// can be done at the same time.
    ///
    /// Available options:
    ///
    ///   * `cursor` - Break runs under the cursor.
    ///
    /// Available since: 1.2.0
    pub font_shaping_break: FontShapingBreak,

    // @"alpha-blending": AlphaBlending =
    //     if (builtin.os.tag == .macos) .native else .@"linear-corrected",
    /// What color space to use when performing alpha blending.
    ///
    /// This affects the appearance of text and of any images with transparency.
    /// Additionally, custom shaders will receive colors in the configured space.
    ///
    /// On macOS the default is `native`, on all other platforms the default is
    /// `linear-corrected`.
    ///
    /// Valid values:
    ///
    /// * `native` - Perform alpha blending in the native color space for the OS.
    ///   On macOS this corresponds to Display P3, and on Linux it's sRGB.
    ///
    /// * `linear` - Perform alpha blending in linear space. This will eliminate
    ///   the darkening artifacts around the edges of text that are very visible
    ///   when certain color combinations are used (e.g. red / green), but makes
    ///   dark text look much thinner than normal and light text much thicker.
    ///   This is also sometimes known as "gamma correction".
    ///
    /// * `linear-corrected` - Same as `linear`, but with a correction step applied
    ///   for text that makes it look nearly or completely identical to `native`,
    ///   but without any of the darkening artifacts.
    ///
    /// Available since: 1.1.0
    pub alpha_blending: AlphaBlending,

    // @"adjust-*": ?MetricModifier = null,
    /// All of the configurations behavior adjust various metrics determined by the
    /// font. The values can be integers (1, -1, etc.) or a percentage (20%, -15%,
    /// etc.). In each case, the values represent the amount to change the original
    /// value.
    ///
    /// For example, a value of `1` increases the value by 1; it does not set it to
    /// literally 1. A value of `20%` increases the value by 20%. And so on.
    ///
    /// There is little to no validation on these values so the wrong values (e.g.
    /// `-100%`) can cause the terminal to be unusable. Use with caution and reason.
    ///
    /// Some values are clamped to minimum or maximum values. This can make it
    /// appear that certain values are ignored. For example, many `*-thickness`
    /// adjustments cannot go below 1px.
    ///
    /// `adjust-cell-height` has some additional behaviors to describe:
    ///
    ///   * The font will be centered vertically in the cell.
    ///
    ///   * The cursor will remain the same size as the font, but may be
    ///     adjusted separately with `adjust-cursor-height`.
    ///
    ///   * Powerline glyphs will be adjusted along with the cell height so
    ///     that things like status lines continue to look aligned.
    pub adjust_cell_width: Option<MetricModifier>,
    pub adjust_cell_height: Option<MetricModifier>,
    /// Distance in pixels or percentage adjustment from the bottom of the cell to the text baseline.
    /// Increase to move baseline UP, decrease to move baseline DOWN.
    /// See the notes about adjustments in `adjust-cell-width`.
    pub adjust_font_baseline: Option<MetricModifier>,
    /// Distance in pixels or percentage adjustment from the top of the cell to the top of the underline.
    /// Increase to move underline DOWN, decrease to move underline UP.
    /// See the notes about adjustments in `adjust-cell-width`.
    pub adjust_underline_position: Option<MetricModifier>,
    /// Thickness in pixels of the underline.
    /// See the notes about adjustments in `adjust-cell-width`.
    pub adjust_underline_thickness: Option<MetricModifier>,
    /// Distance in pixels or percentage adjustment from the top of the cell to the top of the strikethrough.
    /// Increase to move strikethrough DOWN, decrease to move strikethrough UP.
    /// See the notes about adjustments in `adjust-cell-width`.
    pub adjust_strikethrough_position: Option<MetricModifier>,
    /// Thickness in pixels or percentage adjustment of the strikethrough.
    /// See the notes about adjustments in `adjust-cell-width`.
    pub adjust_strikethrough_thickness: Option<MetricModifier>,
    /// Distance in pixels or percentage adjustment from the top of the cell to the top of the overline.
    /// Increase to move overline DOWN, decrease to move overline UP.
    /// See the notes about adjustments in `adjust-cell-width`.
    pub adjust_overline_position: Option<MetricModifier>,
    /// Thickness in pixels or percentage adjustment of the overline.
    /// See the notes about adjustments in `adjust-cell-width`.
    pub adjust_overline_thickness: Option<MetricModifier>,
    /// Thickness in pixels or percentage adjustment of the bar cursor and outlined rect cursor.
    /// See the notes about adjustments in `adjust-cell-width`.
    pub adjust_cursor_thickness: Option<MetricModifier>,
    /// Height in pixels or percentage adjustment of the cursor. Currently applies to all cursor types:
    /// bar, rect, and outlined rect.
    /// See the notes about adjustments in `adjust-cell-width`.
    pub adjust_cursor_height: Option<MetricModifier>,
    /// Thickness in pixels or percentage adjustment of box drawing characters.
    /// See the notes about adjustments in `adjust-cell-width`.
    pub adjust_box_thickness: Option<MetricModifier>,
    /// Height in pixels or percentage adjustment of maximum height for nerd font icons.
    ///
    /// A positive (negative) value will increase (decrease) the maximum icon
    /// height. This may not affect all icons equally: the effect depends on whether
    /// the default size of the icon is height-constrained, which in turn depends on
    /// the aspect ratio of both the icon and your primary font.
    ///
    /// Certain icons designed for box drawing and terminal graphics, such as
    /// Powerline symbols, are not affected by this option.
    ///
    /// See the notes about adjustments in `adjust-cell-width`.
    ///
    /// Available in: 1.2.0
    pub adjust_icon_height: Option<MetricModifier>,

    // @"grapheme-width-method": GraphemeWidthMethod = .unicode,
    /// The method to use for calculating the cell width of a grapheme cluster.
    /// The default value is `unicode` which uses the Unicode standard to determine
    /// grapheme width. This results in correct grapheme width but may result in
    /// cursor-desync issues with some programs (such as shells) that may use a
    /// legacy method such as `wcswidth`.
    ///
    /// Valid values are:
    ///
    /// * `legacy` - Use a legacy method to determine grapheme width, such as
    ///   wcswidth This maximizes compatibility with legacy programs but may result
    ///   in incorrect grapheme width for certain graphemes such as skin-tone
    ///   emoji, non-English characters, etc.
    ///
    ///   This is called "legacy" and not something more specific because the
    ///   behavior is undefined and we want to retain the ability to modify it.
    ///   For example, we may or may not use libc `wcswidth` now or in the future.
    ///
    /// * `unicode` - Use the Unicode standard to determine grapheme width.
    ///
    /// If a running program explicitly enables terminal mode 2027, then `unicode`
    /// width will be forced regardless of this configuration. When mode 2027 is
    /// reset, this configuration will be used again.
    ///
    /// This configuration can be changed at runtime but will not affect existing
    /// terminals. Only new terminals will use the new configuration.
    pub grapheme_width_method: GraphemeWidthMethod,

    // @"freetype-load-flags": FreetypeLoadFlags = .{},
    /// FreeType load flags to enable. The format of this is a list of flags to
    /// enable separated by commas. If you prefix a flag with `no-` then it is
    /// disabled. If you omit a flag, its default value is used, so you must
    /// explicitly disable flags you don't want. You can also use `true` or `false`
    /// to turn all flags on or off.
    ///
    /// This configuration only applies to Rustty builds that use FreeType.
    /// This is usually the case only for Linux builds. macOS uses CoreText
    /// and does not have an equivalent configuration.
    ///
    /// Available flags:
    ///
    ///   * `hinting` - Enable or disable hinting. Enabled by default.
    ///
    ///   * `force-autohint` - Always use the freetype auto-hinter instead of
    ///     the font's native hinter. Disabled by default.
    ///
    ///   * `monochrome` - Instructs renderer to use 1-bit monochrome rendering.
    ///     This will disable anti-aliasing, and probably not look very good unless
    ///     you're using a pixel font. Disabled by default.
    ///
    ///   * `autohint` - Enable the freetype auto-hinter. Enabled by default.
    ///
    ///   * `light` - Use a light hinting style, better preserving glyph shapes.
    ///     This is the most common setting in GTK apps and therefore also Rustty's
    ///     default. This has no effect if `monochrome` is enabled. Enabled by
    ///     default.
    ///
    /// Example: `hinting`, `no-hinting`, `force-autohint`, `no-force-autohint`
    pub freetype_load_flags: FreetypeLoadFlags,

    // theme: ?Theme = null,
    /// A theme to use. This can be a built-in theme name, a custom theme
    /// name, or an absolute path to a custom theme file. Rustty also supports
    /// specifying a different theme to use for light and dark mode. Each
    /// option is documented below.
    ///
    /// If the theme is an absolute pathname, Rustty will attempt to load that
    /// file as a theme. If that file does not exist or is inaccessible, an error
    /// will be logged and no other directories will be searched.
    ///
    /// If the theme is not an absolute pathname, two different directories will be
    /// searched for a file name that matches the theme. This is case sensitive on
    /// systems with case-sensitive filesystems. It is an error for a theme name to
    /// include path separators unless it is an absolute pathname.
    ///
    /// The first directory is the `themes` subdirectory of your Rustty
    /// configuration directory. This is `$XDG_CONFIG_HOME/rustty/themes` or
    /// `~/.config/rustty/themes`.
    ///
    /// The second directory is the `themes` subdirectory of the Rustty resources
    /// directory. Rustty ships with a multitude of themes that will be installed
    /// into this directory. On macOS, this list is in the
    /// `Rustty.app/Contents/Resources/rustty/themes` directory. On Linux, this
    /// list is in the `share/rustty/themes` directory (wherever you installed the
    /// Rustty "share" directory.
    ///
    /// To see a list of available themes, run `rustty +list-themes`.
    ///
    /// A theme file is simply another Rustty configuration file. They share
    /// the same syntax and same configuration options. A theme can set any valid
    /// configuration option so please do not use a theme file from an untrusted
    /// source. The built-in themes are audited to only set safe configuration
    /// options.
    ///
    /// Some options cannot be set within theme files. The reason these are not
    /// supported should be self-evident. A theme file cannot set `theme` or
    /// `config-file`. At the time of writing this, Rustty will not show any
    /// warnings or errors if you set these options in a theme file but they will
    /// be silently ignored.
    ///
    /// Any additional colors specified via background, foreground, palette, etc.
    /// will override the colors specified in the theme.
    ///
    /// To specify a different theme for light and dark mode, use the following
    /// syntax: `light:theme-name,dark:theme-name`. For example:
    /// `light:Rose Pine Dawn,dark:Rose Pine`. Whitespace around all values are
    /// trimmed and order of light and dark does not matter. Both light and dark
    /// must be specified in this form. In this form, the theme used will be
    /// based on the current desktop environment theme.
    ///
    /// There are some known bugs with light/dark mode theming. These will
    /// be fixed in a future update:
    ///
    ///   - macOS: titlebar tabs style is not updated when switching themes.
    pub theme: Option<Theme>,

    // background: Color = .{ .r = 0x28, .g = 0x2C, .b = 0x34 },
    /// Background color for the window.
    /// Specified as either hex (`#RRGGBB` or `RRGGBB`) or a named X11 color.
    pub background: Color,
    // foreground: Color = .{ .r = 0xFF, .g = 0xFF, .b = 0xFF },
    /// Foreground color for the window.
    /// Specified as either hex (`#RRGGBB` or `RRGGBB`) or a named X11 color.
    pub foreground: Color,

    // @"background-image": ?Path = null,
    /// Background image for the terminal.
    ///
    /// This should be a path to a PNG or JPEG file, other image formats are
    /// not yet supported.
    ///
    /// The background image is currently per-terminal, not per-window. If
    /// you are a heavy split user, the background image will be repeated across
    /// splits. A future improvement to Rustty will address this.
    ///
    /// WARNING: Background images are currently duplicated in VRAM per-terminal.
    /// For sufficiently large images, this could lead to a large increase in
    /// memory usage (specifically VRAM usage). A future Rustty improvement
    /// will resolve this by sharing image textures across terminals.
    ///
    /// Available since: 1.2.0
    pub background_image: Option<Path>,
    // @"background-image-opacity": f32 = 1.0,
    /// Background image opacity.
    ///
    /// This is relative to the value of `background-opacity`.
    ///
    /// A value of `1.0` (the default) will result in the background image being
    /// placed on top of the general background color, and then the combined result
    /// will be adjusted to the opacity specified by `background-opacity`.
    ///
    /// A value less than `1.0` will result in the background image being mixed
    /// with the general background color before the combined result is adjusted
    /// to the configured `background-opacity`.
    ///
    /// A value greater than `1.0` will result in the background image having a
    /// higher opacity than the general background color. For instance, if the
    /// configured `background-opacity` is `0.5` and `background-image-opacity`
    /// is set to `1.5`, then the final opacity of the background image will be
    /// `0.5 * 1.5 = 0.75`.
    ///
    /// Available since: 1.2.0
    pub background_image_opacity: f32,
    // @"background-image-position": BackgroundImagePosition = .center,
    /// Background image position.
    ///
    /// Valid values are:
    ///   * `top-left`
    ///   * `top-center`
    ///   * `top-right`
    ///   * `center-left`
    ///   * `center`
    ///   * `center-right`
    ///   * `bottom-left`
    ///   * `bottom-center`
    ///   * `bottom-right`
    ///
    /// The default value is `center`.
    ///
    /// Available since: 1.2.0
    pub background_image_position: BackgroundImagePosition,
    // @"background-image-fit": BackgroundImageFit = .contain,
    /// Background image fit.
    ///
    /// Valid values are:
    ///
    ///  * `contain`
    ///
    ///    Preserving the aspect ratio, scale the background image to the largest
    ///    size that can still be contained within the terminal, so that the whole
    ///    image is visible.
    ///
    ///  * `cover`
    ///
    ///    Preserving the aspect ratio, scale the background image to the smallest
    ///    size that can completely cover the terminal. This may result in one or
    ///    more edges of the image being clipped by the edge of the terminal.
    ///
    ///  * `stretch`
    ///
    ///    Stretch the background image to the full size of the terminal, without
    ///    preserving the aspect ratio.
    ///
    ///  * `none`
    ///
    ///    Don't scale the background image.
    ///
    /// The default value is `contain`.
    ///
    /// Available since: 1.2.0
    pub background_image_fit: BackgroundImageFit,
    // @"background-image-repeat": bool = false,
    /// Whether to repeat the background image or not.
    ///
    /// If this is set to true, the background image will be repeated if there
    /// would otherwise be blank space around it because it doesn't completely
    /// fill the terminal area.
    ///
    /// The default value is `false`.
    ///
    /// Available since: 1.2.0
    pub background_image_repeat: bool,

    // @"selection-foreground": ?TerminalColor = null,
    /// The foreground and background color for selection. If this is not set, then
    /// the selection color is just the inverted window background and foreground
    /// (note: not to be confused with the cell bg/fg).
    /// Specified as either hex (`#RRGGBB` or `RRGGBB`) or a named X11 color.
    /// Since version 1.2.0, this can also be set to `cell-foreground` to match
    /// the cell foreground color, or `cell-background` to match the cell
    /// background color.
    pub selection_foreground: Option<TerminalColor>,
    pub selection_background: Option<TerminalColor>,
    // @"selection-clear-on-typing": bool = true,
    /// Whether to clear selected text when typing. This defaults to `true`.
    /// This is typical behavior for most terminal emulators as well as
    /// text input fields. If you set this to `false`, then the selected text
    /// will not be cleared when typing.
    ///
    /// "Typing" is specifically defined as any non-modifier (shift, control,
    /// alt, etc.) keypress that produces data to be sent to the application
    /// running within the terminal (e.g. the shell). Additionally, selection
    /// is cleared when any preedit or composition state is started (e.g.
    /// when typing languages such as Japanese).
    ///
    /// If this is `false`, then the selection can still be manually
    /// cleared by clicking once or by pressing `escape`.
    ///
    /// Available since: 1.2.0
    pub selection_clear_on_typing: bool,
    // @"selection-clear-on-copy": bool = false,
    /// Whether to clear selected text after copying. This defaults to `false`.
    ///
    /// When set to `true`, the selection will be automatically cleared after
    /// any copy operation that invokes the `copy_to_clipboard` keyboard binding.
    /// Importantly, this will not clear the selection if the copy operation
    /// was invoked via `copy-on-select`.
    ///
    /// When set to `false`, the selection remains visible after copying, allowing
    /// to see what was copied and potentially perform additional operations
    /// on the same selection.
    pub selection_clear_on_copy: bool,
    // @"selection-word-chars": SelectionWordChars = .{},
    /// Characters that mark word boundaries during text selection operations such
    /// as double-clicking. When selecting a word, the selection will stop at any
    /// of these characters.
    ///
    /// This is similar to the `WORDCHARS` environment variable in zsh, except this
    /// specifies the boundary characters rather than the word characters. The
    /// default includes common delimiters and punctuation that typically separate
    /// words in code and prose.
    ///
    /// Each character in this string becomes a word boundary. Multi-byte UTF-8
    /// characters are supported, but only single codepoints can be specified.
    /// Multi-codepoint sequences (e.g. emoji) are not supported.
    ///
    /// The null character (U+0000) is always treated as a boundary and does not
    /// need to be included in this configuration.
    ///
    /// Default: ``\t '"│`|:;,()[]{}<>$``
    ///
    /// To add or remove specific characters, you can set this to a custom value.
    /// For example, to treat semicolons as part of words:
    ///
    ///     selection-word-chars = "\t '\"│`|:,()[]{}<>$"
    ///
    /// Available since: 1.3.0
    pub selection_word_chars: SelectionWordChars,

    // @"minimum-contrast": f64 = 1,
    /// The minimum contrast ratio between the foreground and background colors.
    /// The contrast ratio is a value between 1 and 21. A value of 1 allows for no
    /// contrast (e.g. black on black). This value is the contrast ratio as defined
    /// by the [WCAG 2.0 specification](https://www.w3.org/TR/WCAG20/).
    ///
    /// If you want to avoid invisible text (same color as background), a value of
    /// 1.1 is a good value. If you want to avoid text that is difficult to read, a
    /// value of 3 or higher is a good value. The higher the value, the more likely
    /// that text will become black or white.
    ///
    /// This value does not apply to Emoji or images.
    pub minimum_contrast: f64,

    // palette: Palette = .{},
    /// Color palette for the 256 color form that many terminal applications use.
    /// The syntax of this configuration is `N=COLOR` where `N` is 0 to 255 (for
    /// the 256 colors in the terminal color table) and `COLOR` is a typical RGB
    /// color code such as `#AABBCC` or `AABBCC`, or a named X11 color. For example,
    /// `palette = 5=#BB78D9` will set the 'purple' color.
    ///
    /// The palette index can be in decimal, binary, octal, or hexadecimal.
    /// Decimal is assumed unless a prefix is used: `0b` for binary, `0o` for octal,
    /// and `0x` for hexadecimal.
    ///
    /// For definitions on the color indices and what they canonically map to,
    /// [see this cheat sheet](https://www.ditig.com/256-colors-cheat-sheet).
    ///
    /// For most themes, you only need to set the first 16 colors (0–15) since the
    /// rest of the palette (16–255) will be automatically generated by
    /// default (see `palette-generate` for more details).
    pub palette: Palette,
    // @"palette-generate": bool = false,
    /// Whether to automatically generate the extended 256 color palette
    /// (indices 16–255) from the base 16 ANSI colors.
    ///
    /// This lets theme authors specify only the base 16 colors and have the
    /// rest of the palette be automatically generated in a consistent and
    /// aesthetic way.
    ///
    /// When enabled, the 6×6×6 color cube and 24-step grayscale ramp are
    /// derived from interpolations of the base palette, giving a more cohesive
    /// look. Colors that have been explicitly set via `palette` are never
    /// overwritten.
    ///
    /// The default value is false (disabled), because many legacy programs
    /// using the 256-color palette hardcode assumptions about what these
    /// colors are (mostly assuming the xterm 256 color palette). However, this
    /// is still a very useful tool for theme authors and users who want
    /// to customize their palette without having to specify all 256 colors.
    ///
    /// For more information on how the generation works, see here:
    /// https://gist.github.com/jake-stewart/0a8ea46159a7da2c808e5be2177e1783
    ///
    /// Available since: 1.3.0
    pub palette_generate: bool,
    // @"palette-harmonious": bool = false,
    /// Invert the palette colors generated when `palette-generate` is enabled,
    /// so that the colors go in reverse order. This allows palette-based
    /// applications to work well in both light and dark mode since the
    /// palettes are always relatively good colors.
    ///
    /// This defaults to off because some legacy terminal applications
    /// hardcode the assumption that palette indices 16–231 are ordered from
    /// darkest to lightest, so enabling this would make them unreadable.
    /// This is not a generally good assumption and we encourage modern
    /// terminal applications to use the indices in a more semantic way.
    ///
    /// This has no effect if `palette-generate` is disabled.
    ///
    /// For more information see `palette-generate`.
    ///
    /// Available since: 1.3.0
    pub palette_harmonious: bool,

    // @"cursor-color": ?TerminalColor = null,
    /// The color of the cursor. If this is not set, a default will be chosen.
    ///
    /// Direct colors can be specified as either hex (`#RRGGBB` or `RRGGBB`)
    /// or a named X11 color.
    ///
    /// Additionally, special values can be used to set the color to match
    /// other colors at runtime:
    ///
    ///   * `cell-foreground` - Match the cell foreground color.
    ///     (Available since: 1.2.0)
    ///
    ///   * `cell-background` - Match the cell background color.
    ///     (Available since: 1.2.0)
    pub cursor_color: Option<TerminalColor>,
    // @"cursor-opacity": f64 = 1.0,
    /// The opacity level (opposite of transparency) of the cursor. A value of 1
    /// is fully opaque and a value of 0 is fully transparent. A value less than 0
    /// or greater than 1 will be clamped to the nearest valid value. Note that a
    /// sufficiently small value such as 0.3 may be effectively invisible and may
    /// make it difficult to find the cursor.
    pub cursor_opacity: f64,
    // @"cursor-style": terminal.CursorStyle = .block,
    /// The style of the cursor. This sets the default style. A running program can
    /// still request an explicit cursor style using escape sequences (such as `CSI
    /// q`). Shell configurations will often request specific cursor styles.
    ///
    /// Note that shell integration will automatically set the cursor to a bar at
    /// a prompt, regardless of this configuration. You can disable that behavior
    /// by specifying `shell-integration-features = no-cursor` or disabling shell
    /// integration entirely.
    ///
    /// Valid values are:
    ///
    ///   * `block`
    ///   * `bar`
    ///   * `underline`
    ///   * `block_hollow`
    pub cursor_style: CursorStyle,
    // @"cursor-style-blink": ?bool = null,
    /// Sets the default blinking state of the cursor. This is just the default
    /// state; running programs may override the cursor style using `DECSCUSR` (`CSI
    /// q`).
    ///
    /// If this is not set, the cursor blinks by default. Note that this is not the
    /// same as a "true" value, as noted below.
    ///
    /// If this is not set at all (`null`), then Rustty will respect DEC Mode 12
    /// (AT&T cursor blink) as an alternate approach to turning blinking on/off. If
    /// this is set to any value other than null, DEC mode 12 will be ignored but
    /// `DECSCUSR` will still be respected.
    ///
    /// Valid values are:
    ///
    ///   * ` ` (blank)
    ///   * `true`
    ///   * `false`
    pub cursor_style_blink: Option<bool>,
    // @"cursor-text": ?TerminalColor = null,
    /// The color of the text under the cursor. If this is not set, a default will
    /// be chosen.
    /// Specified as either hex (`#RRGGBB` or `RRGGBB`) or a named X11 color.
    /// Since version 1.2.0, this can also be set to `cell-foreground` to match
    /// the cell foreground color, or `cell-background` to match the cell
    /// background color.
    pub cursor_text: Option<TerminalColor>,
    // @"cursor-click-to-move": bool = true,
    /// Enables the ability to move the cursor at prompts by clicking on a
    /// location in the prompt text.
    ///
    /// This feature requires prompt marking via `OSC 133`. Some shells like Fish
    /// (v4.1+) and Nu (0.111+) natively support this while others may require
    /// additional configuration or Rustty's shell integration features to be
    /// enabled.
    ///
    /// Depending on the shell, this works either by translating your click
    /// position into a series of synthetic arrow key movements or by sending
    /// a click event directly to the shell. In either case, some unexpected
    /// behavior around edge cases is possible.
    pub cursor_click_to_move: bool,

    // @"mouse-hide-while-typing": bool = false,
    /// Hide the mouse immediately when typing. The mouse becomes visible again
    /// when the mouse is used (button, movement, etc.). Platform-specific behavior
    /// may dictate other scenarios where the mouse is shown. For example on macOS,
    /// the mouse is shown again when a new window, tab, or split is created.
    pub mouse_hide_while_typing: bool,
    // @"scroll-to-bottom": ScrollToBottom = .default,
    /// When to scroll the surface to the bottom. The format of this is a list of
    /// options to enable separated by commas. If you prefix an option with `no-`
    /// then it is disabled. If you omit an option, its default value is used.
    ///
    /// Available options:
    ///
    /// - `keystroke` If set, scroll the surface to the bottom when the user
    ///   presses a key that results in data being sent to the PTY (basically
    ///   anything but modifiers or keybinds that are processed by Rustty).
    ///
    /// - `output` If set, scroll the surface to the bottom if there is new data
    ///   to display (e.g., when new lines are printed to the terminal).
    ///
    /// The default is `keystroke, no-output`.
    pub scroll_to_bottom: ScrollToBottom,
    // @"mouse-shift-capture": MouseShiftCapture = .false,
    /// Determines whether running programs can detect the shift key pressed with a
    /// mouse click. Typically, the shift key is used to extend mouse selection.
    ///
    /// The default value of `false` means that the shift key is not sent with
    /// the mouse protocol and will extend the selection. This value can be
    /// conditionally overridden by the running program with the `XTSHIFTESCAPE`
    /// sequence.
    ///
    /// The value `true` means that the shift key is sent with the mouse protocol
    /// but the running program can override this behavior with `XTSHIFTESCAPE`.
    ///
    /// The value `never` is the same as `false` but the running program cannot
    /// override this behavior with `XTSHIFTESCAPE`. The value `always` is the
    /// same as `true` but the running program cannot override this behavior with
    /// `XTSHIFTESCAPE`.
    ///
    /// If you always want shift to extend mouse selection even if the program
    /// requests otherwise, set this to `never`.
    ///
    /// Valid values are:
    ///
    ///   * `true`
    ///   * `false`
    ///   * `always`
    ///   * `never`
    pub mouse_shift_capture: MouseShiftCapture,
    // @"mouse-reporting": bool = true,
    /// Enable or disable mouse reporting. When set to `false`, mouse events will
    /// not be reported to terminal applications even if they request it. This
    /// allows you to always use the mouse for selection and other terminal UI
    /// interactions without applications capturing mouse input.
    ///
    /// When set to `true` (the default), terminal applications can request mouse
    /// reporting and will receive mouse events according to their requested mode.
    ///
    /// This can be toggled at runtime using the `toggle_mouse_reporting` keybind
    /// action.
    pub mouse_reporting: bool,
    // @"mouse-scroll-multiplier": MouseScrollMultiplier = .default,
    /// Multiplier for scrolling distance with the mouse wheel.
    ///
    /// A prefix of `precision:` or `discrete:` can be used to set the multiplier
    /// only for scrolling with the specific type of devices. These can be
    /// comma-separated to set both types of multipliers at the same time, e.g.
    /// `precision:0.1,discrete:3`. If no prefix is used, the multiplier applies
    /// to all scrolling devices. Specifying a prefix was introduced in Rustty
    /// 1.2.1.
    ///
    /// The value will be clamped to [0.01, 10,000]. Both of these are extreme
    /// and you're likely to have a bad experience if you set either extreme.
    ///
    /// The default value is "3" for discrete devices and "1" for precision devices.
    pub mouse_scroll_multiplier: MouseScrollMultiplier,

    // @"background-opacity": f64 = 1.0,
    /// The opacity level (opposite of transparency) of the background. A value of
    /// 1 is fully opaque and a value of 0 is fully transparent. A value less than 0
    /// or greater than 1 will be clamped to the nearest valid value.
    ///
    /// On macOS, background opacity is disabled when the terminal enters native
    /// fullscreen. This is because the background becomes gray and it can cause
    /// widgets to show through which isn't generally desirable.
    ///
    /// On macOS, changing this configuration requires restarting Rustty completely.
    pub background_opacity: f64,
    // @"background-opacity-cells": bool = false,
    /// Applies background opacity to cells with an explicit background color
    /// set.
    ///
    /// Normally, `background-opacity` is only applied to the window background.
    /// If a cell has an explicit background color set, such as red, then that
    /// background color will be fully opaque. An effect of this is that some
    /// terminal applications that repaint the background color of the terminal
    /// such as a Neovim and Tmux may not respect the `background-opacity`
    /// (by design).
    ///
    /// Setting this to `true` will apply the `background-opacity` to all cells
    /// regardless of whether they have an explicit background color set or not.
    ///
    /// Available since: 1.2.0
    pub background_opacity_cells: bool,
    // @"background-blur": BackgroundBlur = .false,
    /// Whether to blur the background when `background-opacity` is less than 1.
    ///
    /// Valid values are:
    ///
    ///   * a nonnegative integer specifying the *blur intensity*
    ///   * `false`, equivalent to a blur intensity of 0
    ///   * `true`, equivalent to the default blur intensity of 20, which is
    ///     reasonable for a good looking blur. Higher blur intensities may
    ///     cause strange rendering and performance issues.
    ///
    /// On macOS 26.0 and later, there are additional special values that
    /// can be set to use the native macOS glass effects:
    ///
    ///   * `macos-glass-regular` - Standard glass effect with some opacity
    ///   * `macos-glass-clear` - Highly transparent glass effect
    ///
    /// If the macOS values are set, then this implies `background-blur = true`
    /// on non-macOS platforms.
    ///
    /// On Linux, the exact blur intensity is ignored and any positive integer
    /// or `true` value will enable background blur. On Wayland, Rustty uses
    /// the [`ext-background-effect-v1`](https://wayland.app/protocols/ext-background-effect-v1)
    /// protocol to enable the blur effect, but this is not always supported
    /// on all compositors. Check the documentation of your compositor or
    /// desktop environment for blur support and how to configure it.
    ///
    /// On X11, blur can only be enabled when using the KWin compositor
    /// as a part of KDE Plasma.
    pub background_blur: BackgroundBlur,

    // @"unfocused-split-opacity": f64 = 0.7,
    /// The opacity level (opposite of transparency) of an unfocused split.
    /// Unfocused splits by default are slightly faded out to make it easier to see
    /// which split is focused. To disable this feature, set this value to 1.
    ///
    /// A value of 1 is fully opaque and a value of 0 is fully transparent. Because
    /// "0" is not useful (it makes the window look very weird), the minimum value
    /// is 0.15. This value still looks weird but you can at least see what's going
    /// on. A value outside of the range 0.15 to 1 will be clamped to the nearest
    /// valid value.
    pub unfocused_split_opacity: f64,
    // @"unfocused-split-fill": ?Color = null,
    /// The color to dim the unfocused split. Unfocused splits are dimmed by
    /// rendering a semi-transparent rectangle over the split. This sets the color of
    /// that rectangle and can be used to carefully control the dimming effect.
    ///
    /// This will default to the background color.
    ///
    /// Specified as either hex (`#RRGGBB` or `RRGGBB`) or a named X11 color.
    pub unfocused_split_fill: Option<Color>,
    // @"split-divider-color": ?Color = null,
    /// The color of the split divider. If this is not set, a default will be chosen.
    /// Specified as either hex (`#RRGGBB` or `RRGGBB`) or a named X11 color.
    ///
    /// Available since: 1.1.0
    pub split_divider_color: Option<Color>,
    // @"split-preserve-zoom": SplitPreserveZoom = .{},
    /// Control when Rustty preserves a zoomed split. Under normal circumstances,
    /// any operation that changes focus or layout of the split tree in a window
    /// will unzoom any zoomed split. This configuration allows you to control
    /// this behavior.
    ///
    /// This can be set to `navigation` to preserve the zoomed split state
    /// when navigating to another split (e.g. via `goto_split`). This will
    /// change the zoomed split to the newly focused split instead of unzooming.
    ///
    /// Any options can also be prefixed with `no-` to disable that option.
    ///
    /// Example: `split-preserve-zoom = navigation`
    ///
    /// Available since: 1.3.0
    pub split_preserve_zoom: SplitPreserveZoom,

    // @"search-foreground": TerminalColor = .{ .color = .{ .r = 0, .g = 0, .b = 0 } },
    /// The foreground and background color for search matches. This only applies
    /// to non-focused search matches, also known as candidate matches.
    ///
    /// Valid values:
    ///
    ///   - Hex (`#RRGGBB` or `RRGGBB`)
    ///   - Named X11 color
    ///   - "cell-foreground" to match the cell foreground color
    ///   - "cell-background" to match the cell background color
    ///
    /// The default value is black text on a golden yellow background.
    pub search_foreground: TerminalColor,
    // @"search-background": TerminalColor = .{ .color = .{ .r = 0xFF, .g = 0xE0, .b = 0x82 } },
    pub search_background: TerminalColor,
    // @"search-selected-foreground": TerminalColor = .{ .color = .{ .r = 0, .g = 0, .b = 0 } },
    /// The foreground and background color for the currently selected search match.
    /// This is the focused match that will be jumped to when using next/previous
    /// search navigation.
    ///
    /// Valid values:
    ///
    ///   - Hex (`#RRGGBB` or `RRGGBB`)
    ///   - Named X11 color
    ///   - "cell-foreground" to match the cell foreground color
    ///   - "cell-background" to match the cell background color
    ///
    /// The default value is black text on a soft peach background.
    pub search_selected_foreground: TerminalColor,
    // @"search-selected-background": TerminalColor = .{ .color = .{ .r = 0xF2, .g = 0xA5, .b = 0x7E } },
    pub search_selected_background: TerminalColor,

    // command: ?Command = null,
    /// The command to run, usually a shell. If this is not an absolute path, it'll
    /// be looked up in the `PATH`. If this is not set, a default will be looked up
    /// from your system. The rules for the default lookup are:
    ///
    ///   * `SHELL` environment variable
    ///
    ///   * `passwd` entry (user information)
    ///
    /// This can contain additional arguments to run the command with. If additional
    /// arguments are provided, the command will be executed using `/bin/sh -c`
    /// to offload shell argument expansion.
    ///
    /// To avoid shell expansion altogether, prefix the command with `direct:`, e.g.
    /// `direct:nvim foo`. This will avoid the roundtrip to `/bin/sh` but will also
    /// not support any shell parsing such as arguments with spaces, filepaths with
    /// `~`, globs, etc. (Available since: 1.2.0)
    ///
    /// You can also explicitly prefix the command with `shell:` to always wrap the
    /// command in a shell. This can be used to ensure our heuristics to choose the
    /// right mode are not used in case they are wrong. (Available since: 1.2.0)
    ///
    /// This command will be used for all new terminal surfaces, i.e. new windows,
    /// tabs, etc. If you want to run a command only for the first terminal surface
    /// created when Rustty starts, use the `initial-command` configuration.
    ///
    /// Rustty supports the common `-e` flag for executing a command with
    /// arguments. For example, `rustty -e fish --with --custom --args`.
    /// This flag sets the `initial-command` configuration, see that for more
    /// information.
    pub command: Option<Command>,
    // @"initial-command": ?Command = null,
    /// This is the same as "command", but only applies to the first terminal
    /// surface created when Rustty starts. Subsequent terminal surfaces will use
    /// the `command` configuration.
    ///
    /// After the first terminal surface is created (or closed), there is no
    /// way to run this initial command again automatically. As such, setting
    /// this at runtime works but will only affect the next terminal surface
    /// if it is the first one ever created.
    ///
    /// If you're using the `rustty` CLI there is also a shortcut to set this
    /// with arguments directly: you can use the `-e` flag. For example: `rustty -e
    /// fish --with --custom --args`. The `-e` flag automatically forces some
    /// other behaviors as well:
    ///
    ///   * Disables shell expansion since the input is expected to already
    ///     be shell-expanded by the upstream (e.g. the shell used to type in
    ///     the `rustty -e` command).
    ///
    ///   * `gtk-single-instance=false` - This ensures that a new instance is
    ///     launched and the CLI args are respected.
    ///
    ///   * `quit-after-last-window-closed=true` - This ensures that the Rustty
    ///     process will exit when the command exits. Additionally, the
    ///     `quit-after-last-window-closed-delay` is unset.
    ///
    ///   * `shell-integration=detect` (if not `none`) - This prevents forcibly
    ///     injecting any configured shell integration into the command's
    ///     environment. With `-e` its highly unlikely that you're executing a
    ///     shell and forced shell integration is likely to cause problems
    ///     (e.g. by wrapping your command in a shell, setting env vars, etc.).
    ///     This is a safety measure to prevent unexpected behavior. If you want
    ///     shell integration with a `-e`-executed command, you must either
    ///     name your binary appropriately or source the shell integration script
    ///     manually.
    pub initial_command: Option<Command>,

    // @"notify-on-command-finish": NotifyOnCommandFinish = .never,
    /// Controls when command finished notifications are sent. There are
    /// three options:
    ///
    /// * `never` - Never send notifications (the default).
    /// * `unfocused` - Only send notifications if the surface that the command is
    ///   running in is not focused.
    /// * `always` - Always send notifications.
    ///
    /// Command finished notifications requires that either shell integration is
    /// enabled, or that your shell sends OSC 133 escape sequences to mark the start
    /// and end of commands.
    ///
    /// On GTK, there is a context menu item that will enable command finished
    /// notifications for a single command, overriding the `never` and `unfocused`
    /// options.
    ///
    /// Available since 1.3.0.
    pub notify_on_command_finish: NotifyOnCommandFinish,
    // @"notify-on-command-finish-action": NotifyOnCommandFinishAction = .{ .bell = true, .notify = false },
    /// If command finished notifications are enabled, this controls how the user is
    /// notified.
    ///
    /// Available options:
    ///
    /// * `bell` - enabled by default
    /// * `notify` - disabled by default
    ///
    /// Options can be combined by listing them as a comma separated list. Options
    /// can be negated by prefixing them with `no-`. For example `no-bell,notify`.
    ///
    /// Available since 1.3.0.
    pub notify_on_command_finish_action: NotifyOnCommandFinishAction,
    // @"notify-on-command-finish-after": Duration = .{ .duration = 5 * std.time.ns_per_s },
    /// If command finished notifications are enabled, this controls how long a
    /// command must have been running before a notification will be sent. The
    /// default is five seconds.
    ///
    /// The duration is specified as a series of numbers followed by time units.
    /// Whitespace is allowed between numbers and units. Each number and unit will
    /// be added together to form the total duration.
    ///
    /// The allowed time units are as follows:
    ///
    ///   * `y` - 365 SI days, or 8760 hours, or 31536000 seconds. No adjustments
    ///     are made for leap years or leap seconds.
    ///   * `d` - one SI day, or 86400 seconds.
    ///   * `h` - one hour, or 3600 seconds.
    ///   * `m` - one minute, or 60 seconds.
    ///   * `s` - one second.
    ///   * `ms` - one millisecond, or 0.001 second.
    ///   * `us` or `µs` - one microsecond, or 0.000001 second.
    ///   * `ns` - one nanosecond, or 0.000000001 second.
    ///
    /// Examples:
    ///   * `1h30m`
    ///   * `45s`
    ///
    /// Units can be repeated and will be added together. This means that
    /// `1h1h` is equivalent to `2h`. This is confusing and should be avoided.
    /// A future update may disallow this.
    ///
    /// The maximum value is `584y 49w 23h 34m 33s 709ms 551µs 615ns`. Any
    /// value larger than this will be clamped to the maximum value.
    ///
    /// Available since 1.3.0
    pub notify_on_command_finish_after: Duration,

    // env: RepeatableStringMap = .{},
    /// Extra environment variables to pass to commands launched in a terminal
    /// surface. The format is `env=KEY=VALUE`.
    ///
    /// `env = foo=bar`
    /// `env = bar=baz`
    ///
    /// Setting `env` to an empty string will reset the entire map to default
    /// (empty).
    ///
    /// `env =`
    ///
    /// Setting a key to an empty string will remove that particular key and
    /// corresponding value from the map.
    ///
    /// `env = foo=bar`
    /// `env = foo=`
    ///
    /// will result in `foo` not being passed to the launched commands.
    ///
    /// Setting a key multiple times will overwrite previous entries.
    ///
    /// `env = foo=bar`
    /// `env = foo=baz`
    ///
    /// will result in `foo=baz` being passed to the launched commands.
    ///
    /// These environment variables will override any existing environment
    /// variables set by Rustty. For example, if you set `RUSTTY_RESOURCES_DIR`
    /// then the value you set here will override the value Rustty typically
    /// automatically injects.
    ///
    /// These environment variables _will not_ be passed to commands run by Rustty
    /// for other purposes, like `open` or `xdg-open` used to open URLs in your
    /// browser.
    ///
    /// Available since: 1.2.0
    pub env: RepeatableStringMap,
    // input: RepeatableReadableIO = .{},
    /// Data to send as input to the command on startup.
    ///
    /// The configured `command` will be launched using the typical rules,
    /// then the data specified as this input will be written to the pty
    /// before any other input can be provided.
    ///
    /// The bytes are sent as-is with no additional encoding. Therefore, be
    /// cautious about input that can contain control characters, because this
    /// can be used to execute programs in a shell.
    ///
    /// The format of this value is:
    ///
    ///   * `raw:<string>` - Send raw text as-is. This uses Zig string literal
    ///     syntax so you can specify control characters and other standard
    ///     escapes.
    ///
    ///   * `path:<path>` - Read a filepath and send the contents. The path
    ///     must be to a file with finite length. e.g. don't use a device
    ///     such as `/dev/stdin` or `/dev/urandom` as these will block
    ///     terminal startup indefinitely. Files are limited to 10MB
    ///     in size to prevent excessive memory usage. If you have files
    ///     larger than this you should write a script to read the file
    ///     and send it to the terminal.
    ///
    /// If no valid prefix is found, it is assumed to be a `raw:` input.
    /// This is an ergonomic choice to allow you to simply write
    /// `input = "Hello, world!"` (a common case) without needing to prefix
    /// every value with `raw:`.
    ///
    /// This can be repeated multiple times to send more data. The data
    /// is concatenated directly with no separator characters in between
    /// (e.g. no newline).
    ///
    /// If any of the input sources do not exist, then none of the input
    /// will be sent. Input sources are not verified until the terminal
    /// is starting, so missing paths will not show up in config validation.
    ///
    /// Changing this configuration at runtime will only affect new
    /// terminals.
    ///
    /// Available since: 1.2.0
    pub input: RepeatableReadableIO,
    // @"wait-after-command": bool = false,
    /// If true, keep the terminal open after the command exits. Normally, the
    /// terminal window closes when the running command (such as a shell) exits.
    /// With this true, the terminal window will stay open until any keypress is
    /// received.
    ///
    /// This is primarily useful for scripts or debugging.
    pub wait_after_command: bool,
    // @"abnormal-command-exit-runtime": u32 = 250,
    /// The number of milliseconds of runtime below which we consider a process exit
    /// to be abnormal. This is used to show an error message when the process exits
    /// too quickly.
    ///
    /// On Linux, this must be paired with a non-zero exit code. On macOS, we allow
    /// any exit code because of the way shell processes are launched via the login
    /// command.
    pub abnormal_command_exit_runtime: u32,
    // @"scrollback-limit-bytes": Limit(usize, 50_000_000) = .default,
    /// The size of the scrollback buffer in bytes. This also includes the active
    /// screen. No matter what this is set to, enough memory will always be
    /// allocated for the visible screen and anything leftover is the limit for
    /// the scrollback.
    ///
    /// When this limit is reached, the oldest lines are removed from the
    /// scrollback.
    ///
    /// Scrollback is stored in memory and allocated lazily up to this limit, so
    /// setting a very large limit does not immediately consume that amount of
    /// memory. On supported systems with scrollback compression enabled, Rustty
    /// attempts to compress fully historical pages which are not currently visible
    /// while the terminal is idle. This can reduce physical memory usage, depending
    /// on the contents of the scrollback.
    ///
    /// This limit always measures the uncompressed logical size of the terminal
    /// pages. Compression does not allow Rustty to retain more history than the
    /// configured limit. Accessing compressed history restores it transparently
    /// and may increase the terminal's physical memory usage again.
    ///
    /// This size is per terminal surface, not for the entire application.
    ///
    /// A separate maximum can be set with `scrollback-limit-lines`; if both limits
    /// are set, then the first one reached will determine when scrollback is
    /// removed.
    ///
    /// The default is 50 MB. Set this to `unlimited` to remove the byte limit.
    ///
    /// This can be changed at runtime but will only affect new terminal surfaces.
    pub scrollback_limit_bytes: ScrollbackLimitBytes,
    // @"scrollback-limit-lines": Limit(usize, std.math.maxInt(usize)) = .default,
    /// The maximum number of lines of scrollback to retain. This excludes
    /// the active screen. Soft-wrapped lines count as multiple lines.
    ///
    /// This limit is an estimate. Internally, Rustty will only trim lines
    /// up to the minimum allocation unit that is used internally (called a
    /// "page"). The size of a page depends on how many styles, graphemes, etc.
    /// take up the screen. In practice, this can be anywhere from a handful to
    /// a couple hundred lines. Importantly, memory is capped either way.
    /// This means that the actual limited lines will likely be slightly
    /// higher in practice.
    ///
    /// The default is `unlimited`. A separate maximum can be set with
    /// `scrollback-limit-bytes`; if both limits are set, then the first one reached
    /// will determine when scrollback is removed.
    ///
    /// This can be changed at runtime but will only affect new terminal surfaces.
    pub scrollback_limit_lines: ScrollbackLimitLines,
    // @"scrollback-compression": bool = true,
    /// Whether to compress scrollback pages while the terminal is idle.
    ///
    /// Rustty does its best to only compress when idle and decompress
    /// as needed. This means that compression doesn't lower IO throughput.
    /// We recommend you keep it on.
    ///
    /// The scrollback limit remains an uncompressed logical limit regardless of
    /// this setting, so disabling compression can increase physical memory usage
    /// but does not change how much history is retained.
    ///
    /// Text-heavy terminal history generally compresses to approximately 10% to
    /// 30% of its uncompressed page memory, corresponding to a 70% to 90% reduction
    /// in physical memory for pages which are compressed. Compression savings are
    /// content-dependent.
    ///
    /// Note that the way Rustty works is that we compress and discard the
    /// physical/resident memory but we retain virtual mappings. You will not
    /// see a decrease in virtual memory usage, but you will see a decrease
    /// in physical/memory usage.
    ///
    /// Changing this at runtime affects future compression work. Pages which are
    /// already compressed remain compressed until their contents are accessed.
    pub scrollback_compression: bool,
    // scrollbar: Scrollbar = .system,
    /// Control when the scrollbar is shown to scroll the scrollback buffer.
    ///
    /// The default value is `system`.
    ///
    /// Valid values:
    ///
    ///   * `system` - Respect the system settings for when to show scrollbars.
    ///     On macOS, we only show scrollbars while actively scrolling or hovering
    ///     the gutter. If the system setting is set to "Always", the scrollbar
    ///     will be shown when the mouse is over the gutter area.
    ///
    ///   * `never` - Never show a scrollbar. You can still scroll using the mouse,
    ///     keybind actions, etc. but you will not have a visual UI widget showing
    ///     a scrollbar.
    pub scrollbar: Scrollbar,

    // link: RepeatableLink = .{},
    /// Match a regular expression against the terminal text and associate clicking
    /// it with an action. This can be used to match URLs, file paths, etc. Actions
    /// can be opening using the system opener (e.g. `open` or `xdg-open`) or
    /// executing any arbitrary binding action.
    ///
    /// Links that are configured earlier take precedence over links that are
    /// configured later.
    ///
    /// A default link that matches a URL and opens it in the system opener always
    /// exists. This can be disabled using `link-url`.
    ///
    /// TODO: This can't currently be set!
    pub link: RepeatableLink,
    // @"link-url": bool = true,
    /// Enable URL matching. URLs are matched on hover with control (Linux) or
    /// command (macOS) pressed and open using the default system application for
    /// the linked URL.
    ///
    /// The URL matcher is always lowest priority of any configured links (see
    /// `link`). If you want to customize URL matching, use `link` and disable this.
    pub link_url: bool,
    // @"link-osc8": bool = true,
    /// Enable hyperlinks created with the OSC 8 escape sequence. When disabled,
    /// OSC 8 hyperlinks are not highlighted, previewed, copied, or opened.
    ///
    /// This does not affect URL matching controlled by `link-url`.
    ///
    /// Available since: 1.4.0
    pub link_osc8: bool,
    // @"link-previews": LinkPreviews = .true,
    /// Show link previews for a matched URL.
    ///
    /// When true, link previews are shown for all matched URLs. When false, link
    /// previews are never shown. When set to "osc8", link previews are only shown
    /// for hyperlinks created with the OSC 8 sequence (in this case, the link text
    /// can differ from the link destination).
    ///
    /// Available since: 1.2.0
    pub link_previews: LinkPreviews,

    // maximize: bool = false,
    /// Whether to start the window in a maximized state. This setting applies
    /// to new windows and does not apply to tabs, splits, etc. However, this setting
    /// will apply to all new windows, not just the first one.
    ///
    /// Available since: 1.1.0
    pub maximize: bool,
    // fullscreen: Fullscreen = .false,
    /// Start new windows in fullscreen. This setting applies to new windows and
    /// does not apply to tabs, splits, etc. However, this setting will apply to all
    /// new windows, not just the first one.
    ///
    /// Allowable values are:
    ///
    ///   * `false` - Don't start in fullscreen (default)
    ///   * `true` - Start in native fullscreen
    ///   * `non-native` - (macOS only) Start in non-native fullscreen, hiding the
    ///     menu bar. This is faster than native fullscreen since it doesn't use
    ///     animations. On non-macOS platforms, this behaves the same as `true`.
    ///   * `non-native-visible-menu` - (macOS only) Start in non-native fullscreen,
    ///     keeping the menu bar visible. On non-macOS platforms, behaves like `true`.
    ///   * `non-native-padded-notch` - (macOS only) Start in non-native fullscreen,
    ///     hiding the menu bar but padding for the notch on applicable devices.
    ///     On non-macOS platforms, behaves like `true`.
    ///
    /// Important: tabs DO NOT WORK with non-native fullscreen modes. Non-native
    /// fullscreen removes the titlebar and macOS native tabs require the titlebar.
    /// If you use tabs, use `true` (native) instead.
    ///
    /// On macOS, `true` (native fullscreen) does not work if `window-decoration`
    /// is set to `false`, because native fullscreen on macOS requires window
    /// decorations.
    pub fullscreen: Fullscreen,
    // title: ?[:0]const u8 = null,
    /// The title Rustty will use for the window. This will force the title of the
    /// window to be this title at all times and Rustty will ignore any set title
    /// escape sequences programs (such as Neovim) may send.
    ///
    /// If you want a blank title, set this to one or more spaces by quoting
    /// the value. For example, `title = " "`. This effectively hides the title.
    /// This is necessary because setting a blank value resets the title to the
    /// default value of the running program.
    ///
    /// This configuration can be reloaded at runtime. If it is set, the title
    /// will update for all windows. If it is unset, the next title change escape
    /// sequence will be honored but previous changes will not retroactively
    /// be set. This latter case may require you to restart programs such as Neovim
    /// to get the new title.
    pub title: Option<Vec<u8>>,
    // class: ?[:0]const u8 = null,
    /// The setting that will change the application class value.
    ///
    /// This controls the class field of the `WM_CLASS` X11 property (when running
    /// under X11), the Wayland application ID (when running under Wayland), and the
    /// bus name that Rustty uses to connect to DBus.
    ///
    /// Note that changing this value between invocations will create new, separate
    /// instances, of Rustty when running with `gtk-single-instance=true`. See that
    /// option for more details.
    ///
    /// Changing this value may break launching Rustty from `.desktop` files, via
    /// DBus activation, or systemd user services as the system is expecting Rustty
    /// to connect to DBus using the default `class` when it is launched.
    ///
    /// The class name must follow the requirements defined [in the GTK
    /// documentation](https://docs.gtk.org/gio/type_func.Application.id_is_valid.html).
    ///
    /// The default is `io.airely.rustty`.
    ///
    /// This only affects GTK builds.
    pub class: Option<Vec<u8>>,
    // @"x11-instance-name": ?[:0]const u8 = null,
    /// This controls the instance name field of the `WM_CLASS` X11 property when
    /// running under X11. It has no effect otherwise.
    ///
    /// The default is `rustty`.
    ///
    /// This only affects GTK builds.
    pub x11_instance_name: Option<Vec<u8>>,

    // @"working-directory": ?WorkingDirectory = null,
    /// The directory to change to after starting the command.
    ///
    /// This setting is secondary to the `window-inherit-working-directory`
    /// setting. If a previous Rustty terminal exists in the same process,
    /// `window-inherit-working-directory` will take precedence. Otherwise, this
    /// setting will be used. Typically, this setting is used only for the first
    /// window.
    ///
    /// The default is `inherit` except in special scenarios listed next. On macOS,
    /// if Rustty can detect it is launched from launchd (double-clicked) or
    /// `open`, then it defaults to `home`. On Linux with GTK, if Rustty can detect
    /// it was launched from a desktop launcher, then it defaults to `home`.
    ///
    /// The value of this must be an absolute path, a path prefixed with `~/`
    /// (the tilde will be expanded to the user's home directory), or
    /// one of the special values below:
    ///
    ///   * `home` - The home directory of the executing user.
    ///
    ///   * `inherit` - The working directory of the launching process.
    pub working_directory: Option<WorkingDirectory>,

    // keybind: Keybinds = .{},
    /// Key bindings. The format is `trigger=action`. Duplicate triggers will
    /// overwrite previously set values. The list of actions is available in
    /// the documentation or using the `rustty +list-actions` command.
    ///
    /// Trigger: `+`-separated list of keys and modifiers. Example: `ctrl+a`,
    /// `ctrl+shift+b`, `up`.
    ///
    /// If the key is a single Unicode codepoint, the trigger will match
    /// any presses that produce that codepoint. These are impacted by
    /// keyboard layouts. For example, `a` will match the `a` key on a
    /// QWERTY keyboard, but will match the `q` key on a AZERTY keyboard
    /// (assuming US physical layout).
    ///
    /// For Unicode codepoints, matching is done by comparing the set of
    /// modifiers with the unmodified codepoint. The unmodified codepoint is
    /// sometimes called an "unshifted character" in other software, but all
    /// modifiers are considered, not only shift. For example, `ctrl+a` will match
    /// `a` but not `ctrl+shift+a` (which is `A` on a US keyboard).
    ///
    /// Further, codepoint matching is case-insensitive and the unmodified
    /// codepoint is always case folded for comparison. As a result,
    /// `ctrl+A` configured will match when `ctrl+a` is pressed. Note that
    /// this means some key combinations are impossible depending on keyboard
    /// layout. For example, `ctrl+_` is impossible on a US keyboard because
    /// `_` is `shift+-` and `ctrl+shift+-` is not equal to `ctrl+_` (because
    /// the modifiers don't match!). More details on impossible key combinations
    /// can be found at this excellent source written by Qt developers:
    /// https://doc.qt.io/qt-6/qkeysequence.html#keyboard-layout-issues
    ///
    /// Physical key codes can be specified by using any of the key codes
    /// as specified by the [W3C specification](https://www.w3.org/TR/uievents-code/).
    /// For example, `KeyA` will match the physical `a` key on a US standard
    /// keyboard regardless of the keyboard layout. These are case-sensitive.
    ///
    /// For aesthetic reasons, the w3c codes also support snake case. For
    /// example, `key_a` is equivalent to `KeyA`. The only exceptions are
    /// function keys, e.g. `F1` is `f1` (no underscore). This is a consequence
    /// of our internal code using snake case but is purposely supported
    /// and tested so it is safe to use. It allows an all-lowercase binding
    /// which I find more aesthetically pleasing.
    ///
    /// Function keys such as `insert`, `up`, `f5`, etc. are also specified
    /// using the keys as specified by the previously linked W3C specification.
    ///
    /// Physical keys always match with a higher priority than Unicode codepoints,
    /// so if you specify both `a` and `KeyA`, the physical key will always be used
    /// regardless of what order they are configured.
    ///
    /// The special key `catch_all` can be used to match any key that is not
    /// otherwise bound. This can be combined with modifiers, for example
    /// `ctrl+catch_all` will match any key pressed with `ctrl` that is not
    /// otherwise bound. When looking up a binding, Rustty first tries to match
    /// `catch_all` with modifiers. If no match is found and the event has
    /// modifiers, it falls back to `catch_all` without modifiers.
    ///
    /// Valid modifiers are `shift`, `ctrl` (alias: `control`), `alt` (alias: `opt`,
    /// `option`), and `super` (alias: `cmd`, `command`). You may use the modifier
    /// or the alias. When debugging keybinds, the non-aliased modifier will always
    /// be used in output.
    ///
    /// Note: The fn or "globe" key on keyboards are not supported as a
    /// modifier. This is a limitation of the operating systems and GUI toolkits
    /// that Rustty uses.
    ///
    /// Some additional notes for triggers:
    ///
    ///   * modifiers cannot repeat, `ctrl+ctrl+a` is invalid.
    ///
    ///   * modifiers and keys can be in any order, `shift+a+ctrl` is *weird*,
    ///     but valid.
    ///
    ///   * only a single key input is allowed, `ctrl+a+b` is invalid.
    ///
    /// You may also specify multiple triggers separated by `>` to require a
    /// sequence of triggers to activate the action. For example,
    /// `ctrl+a>n=new_window` will only trigger the `new_window` action if the
    /// user presses `ctrl+a` followed separately by `n`. In other software, this
    /// is sometimes called a leader key, a key chord, a key table, etc. There
    /// is no hardcoded limit on the number of parts in a sequence.
    ///
    /// Warning: If you define a sequence as a CLI argument to `rustty`,
    /// you probably have to quote the keybind since `>` is a special character
    /// in most shells. Example: rustty --keybind='ctrl+a>n=new_window'
    ///
    /// A trigger sequence has some special handling:
    ///
    ///   * Rustty will wait an indefinite amount of time for the next key in
    ///     the sequence. There is no way to specify a timeout. The only way to
    ///     force the output of a prefix key is to assign another keybind to
    ///     specifically output that key (e.g. `ctrl+a>ctrl+a=text:foo`) or
    ///     press an unbound key which will send both keys to the program.
    ///
    ///   * If an unbound key is pressed during a sequence and a `catch_all`
    ///     binding exists that would `ignore` the input, the entire sequence
    ///     is dropped and nothing happens. Otherwise, the entire sequence is
    ///     encoded and sent to the running program as if no keybind existed.
    ///
    ///   * If a prefix in a sequence is previously bound, the sequence will
    ///     override the previous binding. For example, if `ctrl+a` is bound to
    ///     `new_window` and `ctrl+a>n` is bound to `new_tab`, pressing `ctrl+a`
    ///     will do nothing.
    ///
    ///   * Adding to the above, if a previously bound sequence prefix is
    ///     used in a new, non-sequence binding, the entire previously bound
    ///     sequence will be unbound. For example, if you bind `ctrl+a>n` and
    ///     `ctrl+a>t`, and then bind `ctrl+a` directly, both `ctrl+a>n` and
    ///     `ctrl+a>t` will become unbound.
    ///
    ///   * Trigger sequences are not allowed for `global:` or `all:`-prefixed
    ///     triggers. This is a limitation we could remove in the future.
    ///
    /// Action is the action to take when the trigger is satisfied. It takes the
    /// format `action` or `action:param`. The latter form is only valid if the
    /// action requires a parameter.
    ///
    ///   * `ignore` - Do nothing, ignore the key input. This can be used to
    ///     black hole certain inputs to have no effect.
    ///
    ///   * `unbind` - Remove the binding. This makes it so the previous action
    ///     is removed, and the key will be sent through to the child command
    ///     if it is printable. Unbind will remove any matching trigger,
    ///     including `physical:`-prefixed triggers without specifying the
    ///     prefix.
    ///
    ///   * `csi:text` - Send a CSI sequence. e.g. `csi:A` sends "cursor up".
    ///
    ///   * `esc:text` - Send an escape sequence. e.g. `esc:d` deletes to the
    ///     end of the word to the right.
    ///
    ///   * `text:text` - Send a string. Uses Zig string literal syntax.
    ///     e.g. `text:\x15` sends Ctrl-U.
    ///
    ///   * All other actions can be found in the documentation or by using the
    ///     `rustty +list-actions` command.
    ///
    /// Some notes for the action:
    ///
    ///   * The parameter is taken as-is after the `:`. Double quotes or
    ///     other mechanisms are included and NOT parsed. If you want to
    ///     send a string value that includes spaces, wrap the entire
    ///     trigger/action in double quotes. Example: `--keybind="up=csi:A B"`
    ///
    /// There are some additional special values that can be specified for
    /// keybind:
    ///
    ///   * `keybind=clear` will clear all set keybindings. Warning: this
    ///     removes ALL keybindings up to this point, including the default
    ///     keybindings.
    ///
    /// The keybind trigger can be prefixed with some special values to change
    /// the behavior of the keybind. These are:
    ///
    ///  * `all:`
    ///
    ///    Make the keybind apply to all terminal surfaces. By default,
    ///    keybinds only apply to the focused terminal surface. If this is true,
    ///    then the keybind will be sent to all terminal surfaces. This only
    ///    applies to actions that are surface-specific. For actions that
    ///    are already global (e.g. `quit`), this prefix has no effect.
    ///
    ///    Available since: 1.0.0
    ///
    ///  * `global:`
    ///
    ///    Make the keybind global. By default, keybinds only work within Rustty
    ///    and under the right conditions (application focused, sometimes terminal
    ///    focused, etc.). If you want a keybind to work globally across your system
    ///    (e.g. even when Rustty is not focused), specify this prefix.
    ///    This prefix implies `all:`.
    ///
    ///    Note: this does not work in all environments; see the additional notes
    ///    below for more information.
    ///
    ///    Available since: 1.0.0 on macOS, 1.2.0 on GTK
    ///
    ///  * `unconsumed:`
    ///
    ///    Do not consume the input. By default, a keybind will consume the input,
    ///    meaning that the associated encoding (if any) will not be sent to the
    ///    running program in the terminal. If you wish to send the encoded value
    ///    to the program, specify the `unconsumed:` prefix before the entire
    ///    keybind. For example: `unconsumed:ctrl+a=reload_config`. `global:` and
    ///    `all:`-prefixed keybinds will always consume the input regardless of
    ///    this setting. Since they are not associated with a specific terminal
    ///    surface, they're never encoded.
    ///
    ///    Available since: 1.0.0
    ///
    ///  * `performable:`
    ///
    ///    Only consume the input if the action is able to be performed.
    ///    For example, the `copy_to_clipboard` action will only consume the input
    ///    if there is a selection to copy. If there is no selection, Rustty
    ///    behaves as if the keybind was not set. This has no effect with `global:`
    ///    or `all:`-prefixed keybinds. For key sequences, this will reset the
    ///    sequence if the action is not performable (acting identically to not
    ///    having a keybind set at all).
    ///
    ///    Performable keybinds will not appear as menu shortcuts in the
    ///    application menu. This is because the menu shortcuts force the
    ///    action to be performed regardless of the state of the terminal.
    ///    Performable keybinds will still work, they just won't appear as
    ///    a shortcut label in the menu.
    ///
    ///    Available since: 1.1.0
    ///
    /// Keybind triggers are not unique per prefix combination. For example,
    /// `ctrl+a` and `global:ctrl+a` are not two separate keybinds. The keybind
    /// set later will overwrite the keybind set earlier. In this case, the
    /// `global:` keybind will be used.
    ///
    /// Multiple prefixes can be specified. For example,
    /// `global:unconsumed:ctrl+a=reload_config` will make the keybind global
    /// and not consume the input to reload the config.
    ///
    /// On macOS, this feature requires accessibility permissions to be granted
    /// to Rustty. When a `global:` keybind is specified and Rustty is launched
    /// or reloaded, Rustty will attempt to request these permissions.
    /// If the permissions are not granted, the keybind will not work. On macOS,
    /// you can find these permissions in System Preferences -> Privacy & Security
    /// -> Accessibility.
    ///
    /// Since Rustty 1.4.0, global shortcuts on Linux are primarily supported
    /// through the [`vicinae-hotkey-v1`](https://github.com/vicinaehq/vicinae-wayland-protocols/tree/main/staging/vicinae-hotkey)
    /// protocol, available out-of-the-box with the following Wayland compositors
    /// and desktop environments:
    ///
    ///   - Hyprland since version 0.56.0
    ///
    /// On X11 or when your Wayland compositor/desktop environment does not
    /// support `vicinae-hotkey-v1`, the much more widely-used
    /// [XDG Global Shortcuts](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.GlobalShortcuts.html)
    /// protocol is used instead.
    ///
    /// Note: XDG Global Shortcuts rely on a functional XDG Desktop Portal
    /// implementation **that is designed to be used with your desktop environment
    /// or compositor**. Please make sure that you have the correct one installed.
    ///
    /// Desktop environments that are known to support the XDG Global Shortcuts
    /// protocol include:
    ///
    ///  - KDE Plasma since [5.27](https://kde.org/announcements/plasma/5/5.27.0/#wayland)
    ///    with `xdg-desktop-portal-kde` installed
    ///
    ///  - GNOME since [48](https://release.gnome.org/48/#and-thats-not-all) with
    ///    `xdg-desktop-portal-gnome` installed
    ///
    ///  - Hyprland before version 0.56.0, with `xdg-desktop-portal-hyprland`
    ///    installed. **Some manual configuration is required.** Consult the steps
    ///    outlined on the [Hyprland Wiki](https://wiki.hypr.land/Configuring/Basics/Binds/#dbus-global-shortcuts)
    ///    to set up D-Bus-based global shortcuts correctly.
    ///
    /// Desktop environments and compositors that don't implement either protocol
    /// do not support global shortcuts, like wlroots-based compositors e.g. Sway
    /// (see [upstream issue](https://github.com/emersion/xdg-desktop-portal-wlr/issues/240))
    /// and COSMIC.
    ///
    /// ## Chained Actions
    ///
    /// A keybind can have multiple actions by using the `chain` keyword for
    /// subsequent actions. When a keybind is activated, all chained actions are
    /// executed in order. The syntax is:
    ///
    /// ```ini
    /// keybind = ctrl+a=new_window
    /// keybind = chain=goto_split:left
    /// ```
    ///
    /// This binds `ctrl+a` to first open a new window, then move focus to the
    /// left split. Each `chain` entry appends an action to the most recently
    /// defined keybind. You can chain as many actions as you want:
    ///
    /// ```ini
    /// keybind = ctrl+a=new_window
    /// keybind = chain=goto_split:left
    /// keybind = chain=toggle_fullscreen
    /// ```
    ///
    /// Chained actions cannot have prefixes like `global:` or `unconsumed:`.
    /// The flags from the original keybind apply to the entire chain.
    ///
    /// Chained actions work with key sequences as well. For example:
    ///
    /// ```ini
    /// keybind = ctrl+a>n=new_window
    /// keybind = chain=goto_split:left
    /// ````
    ///
    /// Chains with key sequences apply to the most recent binding in the
    /// sequence.
    ///
    /// Chained keybinds are available since Rustty 1.3.0.
    ///
    /// ## Key Tables
    ///
    /// You may also create a named set of keybindings known as a "key table."
    /// A key table must be explicitly activated for the bindings to become
    /// available. This can be used to implement features such as a
    /// "copy mode", "vim mode", etc. Generically, this can implement modal
    /// keyboard input.
    ///
    /// Key tables are defined using the syntax `<table>/<binding>`. The
    /// `<binding>` value is everything documented above for keybinds. The
    /// `<table>` value is the name of the key table. Table names can contain
    /// anything except `/`, `=`, `+`, and `>`. The characters `+` and `>` are
    /// reserved for keybind syntax (modifier combinations and key sequences).
    /// For example `foo/ctrl+a=new_window` defines a binding within a table
    /// named `foo`.
    ///
    /// Tables are activated and deactivated using the binding actions
    /// `activate_key_table:<name>` and `deactivate_key_table`. Other table
    /// related binding actions also exist; see the documentation for a full list.
    /// These are the primary way to interact with key tables.
    ///
    /// Binding lookup proceeds from the innermost table outward, so keybinds in
    /// the default table remain available unless explicitly unbound in an inner
    /// table.
    ///
    /// A key table has some special syntax and handling:
    ///
    ///   * `<name>/` (with no binding) defines and clears a table, resetting all
    ///     of its keybinds and settings.
    ///
    ///   * You cannot activate a table that is already the innermost table; such
    ///     attempts are ignored. However, the same table can appear multiple times
    ///     in the stack as long as it is not innermost (e.g., `A -> B -> A -> B`
    ///     is valid, but `A -> B -> B` is not).
    ///
    ///   * A table can be activated in one-shot mode using
    ///     `activate_key_table_once:<name>`. A one-shot table is automatically
    ///     deactivated when any non-catch-all binding is invoked.
    ///
    ///   * Key sequences work within tables: `foo/ctrl+a>ctrl+b=new_window`.
    ///     If an invalid key is pressed, the sequence ends but the table remains
    ///     active.
    ///
    ///   * Chain actions work within tables, the `chain` keyword applies to
    ///     the most recently defined binding in the table. e.g. if you set
    ///     `table/ctrl+a=new_window` you can chain by using `chain=text:hello`.
    ///     Important: chain itself doesn't get prefixed with the table name,
    ///     since it applies to the most recent binding in any table.
    ///
    ///   * Prefixes like `global:` work within tables:
    ///     `foo/global:ctrl+a=new_window`.
    ///
    /// Key tables are available since Rustty 1.3.0.
    pub keybind: Keybinds,
    // @"key-remap": KeyRemapSet = .empty,
    /// Remap modifier keys within Rustty. This allows you to swap or reassign
    /// modifier keys at the application level without affecting system-wide
    /// settings.
    ///
    /// The format is `from=to` where both `from` and `to` are modifier key names.
    /// You can use generic names like `ctrl`, `alt`, `shift`, `super` (macOS:
    /// `cmd`/`command`) or sided names like `left_ctrl`, `right_alt`, etc.
    ///
    /// This will NOT change keyboard layout or key encodings outside of Rustty.
    /// For example, on macOS, `option+a` may still produce `å` even if `option` is
    /// remapped to `ctrl`. Desktop environments usually handle key layout long
    /// before Rustty receives the key events.
    ///
    /// Example:
    ///
    ///     key-remap = ctrl=super
    ///     key-remap = left_control=right_alt
    ///
    /// Important notes:
    ///
    /// * This is a one-way remap. If you remap `ctrl=super`, then the physical
    ///   Ctrl key acts as Super, but the Super key remains Super.
    ///
    /// * Remaps are not transitive. If you remap `ctrl=super` and `alt=ctrl`,
    ///   pressing Alt will produce Ctrl, NOT Super.
    ///
    /// * This affects both keybind matching and terminal input encoding.
    ///   This does NOT impact keyboard layout or how keys are interpreted
    ///   prior to Rustty receiving them. For example, `option+a` on macOS
    ///   may still produce `å` even if `option` is remapped to `ctrl`.
    ///
    /// * Generic modifiers (e.g. `ctrl`) match both left and right physical keys.
    ///   Use sided names (e.g. `left_ctrl`) to remap only one side.
    ///
    /// There are other edge case scenarios that may not behave as expected
    /// but are working as intended the way this feature is designed:
    ///
    /// * On macOS, bindings in the main menu will trigger before any remapping
    ///   is done. This is because macOS itself handles menu activation and
    ///   this happens before Rustty receives the key event. To workaround
    ///   this, you should unbind the menu items and rebind them using your
    ///   desired modifier.
    ///
    /// This configuration can be repeated to specify multiple remaps.
    pub key_remap: KeyRemapSet,

    // @"window-padding-x": WindowPadding = .{ .top_left = 2, .bottom_right = 2 },
    /// Horizontal window padding. This applies padding between the terminal cells
    /// and the left and right window borders. The value is in points, meaning that
    /// it will be scaled appropriately for screen DPI.
    ///
    /// If this value is set too large, the screen will render nothing, because the
    /// grid will be completely squished by the padding. It is up to you as the user
    /// to pick a reasonable value. If you pick an unreasonable value, a warning
    /// will appear in the logs.
    ///
    /// Changing this configuration at runtime will only affect new terminals, i.e.
    /// new windows, tabs, etc.
    ///
    /// To set a different left and right padding, specify two numerical values
    /// separated by a comma. For example, `window-padding-x = 2,4` will set the
    /// left padding to 2 and the right padding to 4. If you want to set both
    /// paddings to the same value, you can use a single value. For example,
    /// `window-padding-x = 2` will set both paddings to 2.
    pub window_padding_x: WindowPadding,
    // @"window-padding-y": WindowPadding = .{ .top_left = 2, .bottom_right = 2 },
    /// Vertical window padding. This applies padding between the terminal cells and
    /// the top and bottom window borders. The value is in points, meaning that it
    /// will be scaled appropriately for screen DPI.
    ///
    /// If this value is set too large, the screen will render nothing, because the
    /// grid will be completely squished by the padding. It is up to you as the user
    /// to pick a reasonable value. If you pick an unreasonable value, a warning
    /// will appear in the logs.
    ///
    /// Changing this configuration at runtime will only affect new terminals,
    /// i.e. new windows, tabs, etc.
    ///
    /// To set a different top and bottom padding, specify two numerical values
    /// separated by a comma. For example, `window-padding-y = 2,4` will set the
    /// top padding to 2 and the bottom padding to 4. If you want to set both
    /// paddings to the same value, you can use a single value. For example,
    /// `window-padding-y = 2` will set both paddings to 2.
    pub window_padding_y: WindowPadding,
    // @"window-padding-balance": WindowPaddingBalance = .false,
    /// The viewport dimensions are usually not perfectly divisible by the cell
    /// size. In this case, some extra padding on the end of a column and the bottom
    /// of the final row may exist. If this is `true`, then this extra padding
    /// is automatically balanced between all four edges to minimize imbalance on
    /// one side. If this is `false`, the top left grid cell will always hug the
    /// edge with zero padding other than what may be specified with the other
    /// `window-padding` options.
    ///
    /// If other `window-padding` fields are set and this is `true`, this will still
    /// apply. The other padding is applied first and may affect how many grid cells
    /// actually exist, and this is applied last in order to balance the padding
    /// given a certain viewport size and grid cell size.
    ///
    /// Valid values are:
    ///
    /// * `false` - No balancing is applied.
    /// * `true` - Balance the padding, but cap the top padding to avoid
    ///   excessive space above the first row. Any excess is shifted to the
    ///   bottom.
    /// * `equal` - Balance the padding equally on all sides without any
    ///   top-padding cap. (Available since: 1.4.0)
    pub window_padding_balance: WindowPaddingBalance,
    // @"window-padding-color": WindowPaddingColor = .background,
    /// The color of the padding area of the window. Valid values are:
    ///
    /// * `background` - The background color specified in `background`.
    /// * `extend` - Extend the background color of the nearest grid cell.
    /// * `extend-always` - Same as "extend" but always extends without applying
    ///   any of the heuristics that disable extending noted below.
    ///
    /// The "extend" value will be disabled in certain scenarios. On primary
    /// screen applications (e.g. not something like Neovim), the color will not
    /// be extended vertically if any of the following are true:
    ///
    /// * The nearest row has any cells that have the default background color.
    ///   The thinking is that in this case, the default background color looks
    ///   fine as a padding color.
    /// * The nearest row is a prompt row (requires shell integration). The
    ///   thinking here is that prompts often contain powerline glyphs that
    ///   do not look good extended.
    /// * The nearest row contains a perfect fit powerline character. These
    ///   don't look good extended.
    pub window_padding_color: WindowPaddingColor,
    // @"window-vsync": bool = true,
    /// Synchronize rendering with the screen refresh rate. If true, this will
    /// minimize tearing and align redraws with the screen but may cause input
    /// latency. If false, this will maximize redraw frequency but may cause tearing,
    /// and under heavy load may use more CPU and power.
    ///
    /// This defaults to true because out-of-sync rendering on macOS can
    /// cause kernel panics (macOS 14.4+) and performance issues for external
    /// displays over some hardware such as DisplayLink. If you want to minimize
    /// input latency, set this to false with the known aforementioned risks.
    ///
    /// Changing this value at runtime will only affect new terminals.
    ///
    /// This setting is only supported currently on macOS.
    pub window_vsync: bool,
    // @"window-inherit-working-directory": bool = true,
    /// If true, new windows will inherit the working directory of the
    /// previously focused window. If no window was previously focused, the default
    /// working directory will be used (the `working-directory` option).
    pub window_inherit_working_directory: bool,
    // @"tab-inherit-working-directory": bool = true,
    /// If true, new tabs will inherit the working directory of the
    /// previously focused tab. If no tab was previously focused, the default
    /// working directory will be used (the `working-directory` option).
    pub tab_inherit_working_directory: bool,
    // @"split-inherit-working-directory": bool = true,
    /// If true, new split panes will inherit the working directory of the
    /// previously focused split. If no split was previously focused, the default
    /// working directory will be used (the `working-directory` option).
    pub split_inherit_working_directory: bool,
    // @"window-inherit-font-size": bool = true,
    /// If true, new windows and tabs will inherit the font size of the previously
    /// focused window. If no window was previously focused, the default font size
    /// will be used. If this is false, the default font size specified in the
    /// configuration `font-size` will be used.
    pub window_inherit_font_size: bool,
    // @"window-decoration": WindowDecoration = .auto,
    /// Configure a preference for window decorations. This setting specifies
    /// a _preference_; the actual OS, desktop environment, window manager, etc.
    /// may override this preference. Rustty will do its best to respect this
    /// preference but it may not always be possible.
    ///
    /// Valid values:
    ///
    ///  * `none`
    ///
    ///    All window decorations will be disabled. Titlebar, borders, etc. will
    ///    not be shown. On macOS, this will also disable tabs (enforced by the
    ///    system).
    ///
    ///  * `auto`
    ///
    ///    Automatically decide to use either client-side or server-side
    ///    decorations based on the detected preferences of the current OS and
    ///    desktop environment. This option usually makes Rustty look the most
    ///    "native" for your desktop.
    ///
    ///  * `client`
    ///
    ///    Prefer client-side decorations.
    ///
    ///    Available since: 1.1.0
    ///
    ///  * `server`
    ///
    ///    Prefer server-side decorations. This is only relevant on Linux with GTK,
    ///    either on X11, or Wayland on a compositor that supports the
    ///    `org_kde_kwin_server_decoration` protocol (e.g. KDE Plasma, but almost
    ///    any non-GNOME desktop supports this protocol).
    ///
    ///    If `server` is set but the environment doesn't support server-side
    ///    decorations, client-side decorations will be used instead.
    ///
    ///    Available since: 1.1.0
    ///
    /// The default value is `auto`.
    ///
    /// For the sake of backwards compatibility and convenience, this setting also
    /// accepts boolean true and false values. If set to `true`, this is equivalent
    /// to `auto`. If set to `false`, this is equivalent to `none`.
    /// This is convenient for users who live primarily on systems that don't
    /// differentiate between client and server-side decorations (e.g. macOS and
    /// Windows).
    ///
    /// The "toggle_window_decorations" keybind action can be used to create
    /// a keybinding to toggle this setting at runtime.
    ///
    /// macOS: To hide the titlebar without removing the native window borders
    ///        or rounded corners, use `macos-titlebar-style = hidden` instead.
    pub window_decoration: WindowDecoration,
    // @"window-title-font-family": ?[:0]const u8 = null,
    /// The font that will be used for the application's window and tab titles.
    ///
    /// If this setting is left unset, the system default font will be used.
    ///
    /// Note: any font available on the system may be used, this font is not
    /// required to be a fixed-width font.
    ///
    /// Available since: 1.0.0 on macOS, 1.1.0 on GTK
    pub window_title_font_family: Option<Vec<u8>>,
    // @"window-subtitle": WindowSubtitle = .false,
    /// The text that will be displayed in the subtitle of the window. Valid values:
    ///
    ///   * `false` - Disable the subtitle.
    ///   * `working-directory` - Set the subtitle to the working directory of the
    ///      surface.
    ///
    /// This feature is only supported on GTK.
    ///
    /// Available since: 1.1.0
    // PORT NOTE (R1): the reference doc's list continuation is aligned at its
    // exact indentation; the cosmetic markdown lint is allowed locally rather
    // than reflowing the ported reference text.
    #[allow(clippy::doc_overindented_list_items)]
    pub window_subtitle: WindowSubtitle,
    // @"window-theme": WindowTheme = .auto,
    /// The theme to use for the windows. Valid values:
    ///
    ///   * `auto` - Determine the theme based on the configured terminal
    ///      background color. This has no effect if the "theme" configuration
    ///      has separate light and dark themes. In that case, the behavior
    ///      of "auto" is equivalent to "system".
    ///   * `system` - Use the system theme.
    ///   * `light` - Use the light theme regardless of system theme.
    ///   * `dark` - Use the dark theme regardless of system theme.
    ///   * `rustty` - Use the background and foreground colors specified in the
    ///     Rustty configuration. This is only supported on Linux builds.
    ///
    /// On macOS, if `macos-titlebar-style` is `tabs` or `transparent`, the window theme will be
    /// automatically set based on the luminosity of the terminal background color.
    /// This only applies to terminal windows. This setting will still apply to
    /// non-terminal windows within Rustty.
    ///
    /// This is currently only supported on macOS and Linux.
    // PORT NOTE (R1): reference doc list continuations kept at their exact
    // indentation; the cosmetic markdown lint is allowed locally.
    #[allow(clippy::doc_overindented_list_items)]
    pub window_theme: WindowTheme,
    // @"window-colorspace": WindowColorspace = .srgb,
    /// The color space to use when interpreting terminal colors. "Terminal colors"
    /// refers to colors specified in your configuration and colors produced by
    /// direct-color SGR sequences.
    ///
    /// Valid values:
    ///
    ///   * `srgb` - Interpret colors in the sRGB color space. This is the default.
    ///   * `display-p3` - Interpret colors in the Display P3 color space.
    ///
    /// This setting is currently only supported on macOS.
    pub window_colorspace: WindowColorspace,
    // @"window-height": u32 = 0,
    /// The initial window size. This size is in terminal grid cells by default.
    /// Both values must be set to take effect. If only one value is set, it is
    /// ignored.
    ///
    /// We don't currently support specifying a size in pixels but a future change
    /// can enable that. If this isn't specified, the app runtime will determine
    /// some default size.
    ///
    /// Note that the window manager may put limits on the size or override the
    /// size. For example, a tiling window manager may force the window to be a
    /// certain size to fit within the grid. There is nothing Rustty will do about
    /// this, but it will make an effort.
    ///
    /// Sizes larger than the screen size will be clamped to the screen size.
    /// This can be used to create a maximized-by-default window size.
    ///
    /// This will not affect new tabs, splits, or other nested terminal elements.
    /// This only affects the initial window size of any new window. Changing this
    /// value will not affect the size of the window after it has been created. This
    /// is only used for the initial size.
    ///
    /// BUG: On Linux with GTK, the calculated window size will not properly take
    /// into account window decorations. As a result, the grid dimensions will not
    /// exactly match this configuration. If window decorations are disabled (see
    /// `window-decoration`), then this will work as expected.
    ///
    /// Windows smaller than 10 wide by 4 high are not allowed.
    pub window_height: u32,
    // @"window-width": u32 = 0,
    pub window_width: u32,
    // @"window-position-x": ?i16 = null,
    /// The starting window position. This position is in pixels and is relative
    /// to the top-left corner of the primary monitor. Both values must be set to take
    /// effect. If only one value is set, it is ignored.
    ///
    /// Note that the window manager may put limits on the position or override
    /// the position. For example, a tiling window manager may force the window
    /// to be a certain position to fit within the grid. There is nothing Rustty
    /// will do about this, but it will make an effort.
    ///
    /// Also note that negative values are also up to the operating system and
    /// window manager. Some window managers may not allow windows to be placed
    /// off-screen.
    ///
    /// Invalid positions are runtime-specific, but generally the positions are
    /// clamped to the nearest valid position.
    ///
    /// On macOS, the window position is relative to the top-left corner of
    /// the visible screen area. This means that if the menu bar is visible, the
    /// window will be placed below the menu bar.
    ///
    /// Note: this is only supported on macOS. The GTK runtime does not support
    /// setting the window position, as windows are only allowed position
    /// themselves in X11 and not Wayland.
    pub window_position_x: Option<i16>,
    // @"window-position-y": ?i16 = null,
    pub window_position_y: Option<i16>,
    // @"window-save-state": WindowSaveState = .default,
    /// Whether to enable saving and restoring window state. Window state includes
    /// their position, size, tabs, splits, etc. Some window state requires shell
    /// integration, such as preserving working directories. See `shell-integration`
    /// for more information.
    ///
    /// There are three valid values for this configuration:
    ///
    ///   * `default` will use the default system behavior. On macOS, this
    ///     will only save state if the application is forcibly terminated
    ///     or if it is configured systemwide via Settings.app.
    ///
    ///   * `never` will never save window state.
    ///
    ///   * `always` will always save window state whenever Rustty is exited.
    ///
    /// If you change this value to `never` while Rustty is not running, the next
    /// Rustty launch will NOT restore the window state.
    ///
    /// If you change this value to `default` while Rustty is not running and the
    /// previous exit saved state, the next Rustty launch will still restore the
    /// window state. This is because Rustty cannot know if the previous exit was
    /// due to a forced save or not (macOS doesn't provide this information).
    ///
    /// If you change this value so that window state is saved while Rustty is not
    /// running, the previous window state will not be restored because Rustty only
    /// saves state on exit if this is enabled.
    ///
    /// The default value is `default`.
    ///
    /// This is currently only supported on macOS. This has no effect on Linux.
    pub window_save_state: WindowSaveState,
    // @"window-step-resize": bool = false,
    /// Resize the window in discrete increments of the focused surface's cell size.
    /// If this is disabled, surfaces are resized in pixel increments. Currently
    /// only supported on macOS.
    pub window_step_resize: bool,
    // @"window-new-tab-position": WindowNewTabPosition = .current,
    /// The position where new tabs are created. Valid values:
    ///
    ///   * `current` - Insert the new tab after the currently focused tab,
    ///     or at the end if there are no focused tabs.
    ///
    ///   * `end` - Insert the new tab at the end of the tab list.
    pub window_new_tab_position: WindowNewTabPosition,
    // @"window-show-tab-bar": WindowShowTabBar = .auto,
    /// Whether to show the tab bar.
    ///
    /// Valid values:
    ///
    ///  - `always`
    ///
    ///    Always display the tab bar, even when there's only one tab.
    ///
    ///    Available since: 1.2.0
    ///
    ///  - `auto` *(default)*
    ///
    ///    Automatically show and hide the tab bar. The tab bar is only
    ///    shown when there are two or more tabs present.
    ///
    ///  - `never`
    ///
    ///    Never show the tab bar. Tabs are only accessible via the tab
    ///    overview or by keybind actions.
    ///
    /// Currently only supported on Linux (GTK).
    pub window_show_tab_bar: WindowShowTabBar,
    // @"window-titlebar-background": ?Color = null,
    /// Background color for the window titlebar. This only takes effect if
    /// window-theme is set to rustty. Currently only supported in the GTK app
    /// runtime.
    ///
    /// Specified as either hex (`#RRGGBB` or `RRGGBB`) or a named X11 color.
    pub window_titlebar_background: Option<Color>,
    // @"window-titlebar-foreground": ?Color = null,
    /// Foreground color for the window titlebar. This only takes effect if
    /// window-theme is set to rustty. Currently only supported in the GTK app
    /// runtime.
    ///
    /// Specified as either hex (`#RRGGBB` or `RRGGBB`) or a named X11 color.
    pub window_titlebar_foreground: Option<Color>,

    // @"drag-handle": DragHandle = .auto,
    /// Controls when drag handles are shown over splits,
    /// allowing splits to be rearranged with mouse controls.
    ///
    /// Valid values:
    ///
    ///  - `always`
    ///
    ///    Always display the drag handle, even when there's only one split.
    ///
    ///  - `auto` *(default)*
    ///
    ///    Automatically show and hide the drag handle.
    ///
    ///    On Linux, the handle is only shown when there are two or
    ///    more splits present.
    ///
    ///    On macOS, the handle is only hidden when there's one split
    ///    in **fullscreen** mode, otherwise it's shown when hovered.
    ///
    ///  - `never`
    ///
    ///    Never show the drag handle. Splits then cannot be rearranged with
    ///    mouse controls.
    ///
    /// Available since: 1.4.0.
    pub drag_handle: DragHandle,

    // @"resize-overlay": ResizeOverlay = .@"after-first",
    /// This controls when resize overlays are shown. Resize overlays are a
    /// transient popup that shows the size of the terminal while the surfaces are
    /// being resized. The possible options are:
    ///
    ///   * `always` - Always show resize overlays.
    ///   * `never` - Never show resize overlays.
    ///   * `after-first` - The resize overlay will not appear when the surface
    ///                     is first created, but will show up if the surface is
    ///                     subsequently resized.
    ///
    /// The default is `after-first`.
    // PORT NOTE (R1): reference doc list continuations kept at their exact
    // indentation; the cosmetic markdown lint is allowed locally.
    #[allow(clippy::doc_overindented_list_items)]
    pub resize_overlay: ResizeOverlay,
    // @"resize-overlay-position": ResizeOverlayPosition = .center,
    /// If resize overlays are enabled, this controls the position of the overlay.
    /// The possible options are:
    ///
    ///   * `center`
    ///   * `top-left`
    ///   * `top-center`
    ///   * `top-right`
    ///   * `bottom-left`
    ///   * `bottom-center`
    ///   * `bottom-right`
    ///
    /// The default is `center`.
    pub resize_overlay_position: ResizeOverlayPosition,
    // @"resize-overlay-duration": Duration = .{ .duration = 750 * std.time.ns_per_ms },
    /// If resize overlays are enabled, this controls how long the overlay is
    /// visible on the screen before it is hidden. The default is ¾ of a second or
    /// 750 ms.
    ///
    /// The duration is specified as a series of numbers followed by time units.
    /// Whitespace is allowed between numbers and units. Each number and unit will
    /// be added together to form the total duration.
    ///
    /// The allowed time units are as follows:
    ///
    ///   * `y` - 365 SI days, or 8760 hours, or 31536000 seconds. No adjustments
    ///     are made for leap years or leap seconds.
    ///   * `d` - one SI day, or 86400 seconds.
    ///   * `h` - one hour, or 3600 seconds.
    ///   * `m` - one minute, or 60 seconds.
    ///   * `s` - one second.
    ///   * `ms` - one millisecond, or 0.001 second.
    ///   * `us` or `µs` - one microsecond, or 0.000001 second.
    ///   * `ns` - one nanosecond, or 0.000000001 second.
    ///
    /// Examples:
    ///   * `1h30m`
    ///   * `45s`
    ///
    /// Units can be repeated and will be added together. This means that
    /// `1h1h` is equivalent to `2h`. This is confusing and should be avoided.
    /// A future update may disallow this.
    ///
    /// The maximum value is `584y 49w 23h 34m 33s 709ms 551µs 615ns`. Any
    /// value larger than this will be clamped to the maximum value.
    ///
    /// Available since 1.0.0
    pub resize_overlay_duration: Duration,

    // @"focus-follows-mouse": bool = false,
    /// If true, when there are multiple split panes, the mouse selects the pane
    /// that is focused. This only applies to the currently focused window; e.g.
    /// mousing over a split in an unfocused window will not focus that split
    /// and bring the window to front.
    ///
    /// Default is false.
    pub focus_follows_mouse: bool,

    // @"clipboard-read": ClipboardAccess = .ask,
    /// Whether to allow programs running in the terminal to read/write to the
    /// system clipboard (OSC 52, for googling). The default is to allow clipboard
    /// reading after prompting the user and allow writing unconditionally.
    ///
    /// Valid values are:
    ///
    ///   * `ask`
    ///   * `allow`
    ///   * `deny`
    ///
    pub clipboard_read: ClipboardAccess,
    // @"clipboard-write": ClipboardAccess = .allow,
    pub clipboard_write: ClipboardAccess,
    // @"clipboard-write-limit-bytes": Limit(usize, 64 * 1024 * 1024) = .default,
    /// The maximum size in bytes of a single clipboard write by a program
    /// running in the terminal via the Kitty clipboard protocol (OSC 5522).
    /// This doesn't apply to OSC 52, which is limited by the maximum length
    /// of an escape sequence hardcoded into Rustty for now.
    ///
    /// Data beyond the limit fails the entire write with an `EFBIG` status,
    /// discards the transaction, and leaves the clipboard untouched. Later
    /// write-related packets are ignored until a new write begins.
    ///
    /// The data is buffered in memory while the write is in progress, so
    /// this limit bounds how much memory a program can make Rustty
    /// allocate per write. A future improvement will attempt to spool large
    /// writes to disk.
    ///
    /// The default is 64 MiB, the minimum a conforming implementation must
    /// accept. Set this to `unlimited` to remove the limit, allowing writes
    /// bounded only by available memory. A value of `0` rejects every non-empty
    /// write. To reject clipboard writes entirely, use `clipboard-write = deny`
    /// instead.
    ///
    /// This can be changed at runtime and applies to writes that begin
    /// after the change.
    ///
    /// Available since: 1.4.0
    pub clipboard_write_limit_bytes: ClipboardWriteLimitBytes,
    // @"clipboard-trim-trailing-spaces": bool = true,
    /// Trims trailing whitespace on data that is copied to the clipboard. This does
    /// not affect data sent to the clipboard via `clipboard-write`. This only
    /// applies to trailing whitespace on lines that have other characters.
    /// Completely blank lines always have their whitespace trimmed.
    pub clipboard_trim_trailing_spaces: bool,
    // @"clipboard-paste-protection": bool = true,
    /// Require confirmation before pasting text that appears unsafe. This helps
    /// prevent a "copy/paste attack" where a user may accidentally execute unsafe
    /// commands by pasting text with newlines.
    pub clipboard_paste_protection: bool,
    // @"clipboard-paste-bracketed-safe": bool = true,
    /// If true, bracketed pastes will be considered safe. By default, bracketed
    /// pastes are considered safe. "Bracketed" pastes are pastes while the running
    /// program has bracketed paste mode enabled (a setting set by the running
    /// program, not the terminal emulator).
    pub clipboard_paste_bracketed_safe: bool,

    // @"title-report": bool = false,
    /// Enables or disabled title reporting (CSI 21 t). This escape sequence
    /// allows the running program to query the terminal title. This is a common
    /// security issue and is disabled by default.
    ///
    /// Warning: This can expose sensitive information at best and enable
    /// arbitrary code execution at worst (with a maliciously crafted title
    /// and a minor amount of user interaction).
    ///
    /// Available since: 1.0.1
    pub title_report: bool,
    // @"image-storage-limit": u32 = 320 * 1000 * 1000,
    /// The total amount of bytes that can be used for image data (e.g. the Kitty
    /// image protocol) per terminal screen. The maximum value is 4,294,967,295
    /// (4GiB). The default is 320MB. If this is set to zero, then all image
    /// protocols will be disabled.
    ///
    /// This value is separate for primary and alternate screens so the effective
    /// limit per surface is double.
    pub image_storage_limit: u32,

    // @"copy-on-select": CopyOnSelect = switch (builtin.os.tag) {
    //     .linux => .primary, else => .none,
    // },
    /// Whether to automatically copy selected text to the clipboard.
    ///
    /// Valid values:
    ///
    /// * `none` - Do not copy selected text automatically.
    ///
    /// * `primary` - On Linux, copy to the selection clipboard only. This has no
    ///   effect on macOS. (Available since: 1.4.0)
    ///
    /// * `clipboard` - Copy text to the system clipboard only.
    ///
    /// * `both` - Copy to both clipboards on Linux, and only the system clipboard
    ///   on macOS. (Available since: 1.4.0)
    ///
    /// For backward compatibility and convenience, a value of `true` is the same as
    /// `primary` on Linux and `clipboard` on macOS, and `false` is an alias for
    /// `none`.
    ///
    /// The default value is `primary` on Linux and `none` otherwise.
    pub copy_on_select: CopyOnSelect,

    // @"right-click-action": RightClickAction = .@"context-menu",
    /// The action to take when the user right-clicks on the terminal surface.
    ///
    /// Valid values:
    ///   * `context-menu` - Show the context menu.
    ///   * `paste` - Paste the contents of the clipboard.
    ///   * `copy` - Copy the selected text to the clipboard.
    ///   * `copy-or-paste` - If there is a selection, copy the selected text to
    ///      the clipboard; otherwise, paste the contents of the clipboard.
    ///   * `ignore` - Do nothing, ignore the right-click.
    ///
    /// The default value is `context-menu`.
    // PORT NOTE (R1): reference doc list continuations kept at their exact
    // indentation; the cosmetic markdown lint is allowed locally.
    #[allow(clippy::doc_overindented_list_items)]
    pub right_click_action: RightClickAction,
    // @"middle-click-action": MiddleClickAction = .@"primary-paste",
    /// The action to take when the user middle-clicks on the terminal surface.
    ///
    /// Valid values:
    ///   * `primary-paste` - Paste from the selection clipboard on Linux.
    ///      Does nothing on macOS.
    ///   * `clipboard-paste` - Paste from the system clipboard.
    ///   * `ignore` - Do nothing, ignore the middle click.
    ///
    /// The default value is `primary-paste`.
    // PORT NOTE (R1): reference doc list continuations kept at their exact
    // indentation; the cosmetic markdown lint is allowed locally.
    #[allow(clippy::doc_overindented_list_items)]
    pub middle_click_action: MiddleClickAction,
    // @"click-repeat-interval": u32 = 0,
    /// The time in milliseconds between clicks to consider a click a repeat
    /// (double, triple, etc.) or an entirely new single click. A value of zero will
    /// use a platform-specific default. The default on macOS is determined by the
    /// OS settings. On every other platform it is 500ms.
    pub click_repeat_interval: u32,

    // @"config-file": RepeatablePath = .{},
    /// Additional configuration files to read. This configuration can be repeated
    /// to read multiple configuration files. Configuration files themselves can
    /// load more configuration files. Paths are relative to the file containing the
    /// `config-file` directive. For command-line arguments, paths are relative to
    /// the current working directory.
    ///
    /// Prepend a ? character to the file path to suppress errors if the file does
    /// not exist. If you want to include a file that begins with a literal ?
    /// character, surround the file path in double quotes (").
    ///
    /// Cycles are not allowed. If a cycle is detected, an error will be logged and
    /// the configuration file will be ignored.
    ///
    /// Configuration files are loaded after the configuration they're defined
    /// within in the order they're defined. **THIS IS A VERY SUBTLE BUT IMPORTANT
    /// POINT.** To put it another way: configuration files do not take effect
    /// until after the entire configuration is loaded. For example, in the
    /// configuration below:
    ///
    /// ```
    /// config-file = "foo"
    /// a = 1
    /// ```
    ///
    /// If "foo" contains `a = 2`, the final value of `a` will be 2, because
    /// `foo` is loaded after the configuration file that configures the
    /// nested `config-file` value.
    pub config_file: RepeatablePath,
    // @"config-default-files": bool = true,
    /// When this is true, the default configuration file paths will be loaded.
    /// The default configuration file paths are currently only the XDG
    /// config path ($XDG_CONFIG_HOME/rustty/config.rustty).
    ///
    /// If this is false, the default configuration paths will not be loaded.
    /// This is targeted directly at using Rustty from the CLI in a way
    /// that minimizes external effects.
    ///
    /// This is a CLI-only configuration. Setting this in a configuration file
    /// will have no effect. It is not an error, but it will not do anything.
    /// This configuration can only be set via CLI arguments.
    pub config_default_files: bool,

    // @"confirm-close-surface": ConfirmCloseSurface = .true,
    /// Confirms that a surface should be closed before closing it.
    ///
    /// This defaults to `true`. If set to `false`, surfaces will close without
    /// any confirmation. This can also be set to `always`, which will always
    /// confirm closing a surface, even if shell integration says a process isn't
    /// running.
    pub confirm_close_surface: ConfirmCloseSurface,
    // @"quit-after-last-window-closed": bool = builtin.os.tag == .linux,
    /// Whether or not to quit after the last surface is closed.
    ///
    /// This defaults to `false` on macOS since that is standard behavior for
    /// a macOS application. On Linux, this defaults to `true` since that is
    /// generally expected behavior.
    ///
    /// On Linux, if this is `true`, Rustty can delay quitting fully until a
    /// configurable amount of time has passed after the last window is closed.
    /// See the documentation of `quit-after-last-window-closed-delay`.
    pub quit_after_last_window_closed: bool,
    // @"quit-after-last-window-closed-delay": ?Duration = null,
    /// Controls how long Rustty will stay running after the last open surface has
    /// been closed. This only has an effect if `quit-after-last-window-closed` is
    /// also set to `true`.
    ///
    /// The minimum value for this configuration is `1s`. Any values lower than
    /// this will be clamped to `1s`.
    ///
    /// The duration is specified as a series of numbers followed by time units.
    /// Whitespace is allowed between numbers and units. Each number and unit will
    /// be added together to form the total duration.
    ///
    /// The allowed time units are as follows:
    ///
    ///   * `y` - 365 SI days, or 8760 hours, or 31536000 seconds. No adjustments
    ///     are made for leap years or leap seconds.
    ///   * `d` - one SI day, or 86400 seconds.
    ///   * `h` - one hour, or 3600 seconds.
    ///   * `m` - one minute, or 60 seconds.
    ///   * `s` - one second.
    ///   * `ms` - one millisecond, or 0.001 second.
    ///   * `us` or `µs` - one microsecond, or 0.000001 second.
    ///   * `ns` - one nanosecond, or 0.000000001 second.
    ///
    /// Examples:
    ///   * `1h30m`
    ///   * `45s`
    ///
    /// Units can be repeated and will be added together. This means that
    /// `1h1h` is equivalent to `2h`. This is confusing and should be avoided.
    /// A future update may disallow this.
    ///
    /// The maximum value is `584y 49w 23h 34m 33s 709ms 551µs 615ns`. Any
    /// value larger than this will be clamped to the maximum value.
    ///
    /// By default `quit-after-last-window-closed-delay` is unset and
    /// Rustty will quit immediately after the last window is closed if
    /// `quit-after-last-window-closed` is `true`.
    ///
    /// Only implemented on Linux.
    pub quit_after_last_window_closed_delay: Option<Duration>,
    // @"initial-window": bool = true,
    /// This controls whether an initial window is created when Rustty
    /// is run. Note that if `quit-after-last-window-closed` is `true` and
    /// `quit-after-last-window-closed-delay` is set, setting `initial-window` to
    /// `false` will mean that Rustty will quit after the configured delay if no
    /// window is ever created. Only implemented on Linux and macOS.
    pub initial_window: bool,

    // @"undo-timeout": Duration = .{ .duration = 5 * std.time.ns_per_s },
    /// The duration that undo operations remain available. After this
    /// time, the operation will be removed from the undo stack and
    /// cannot be undone.
    ///
    /// The default value is 5 seconds.
    ///
    /// This timeout applies per operation, meaning that if you perform
    /// multiple operations, each operation will have its own timeout.
    /// New operations do not reset the timeout of previous operations.
    ///
    /// A timeout of zero will effectively disable undo operations. It is
    /// not possible to set an infinite timeout, but you can set a very
    /// large timeout to effectively disable the timeout (on the order of years).
    /// This is highly discouraged, as it will cause the undo stack to grow
    /// indefinitely, memory usage to grow unbounded, and terminal sessions
    /// to never actually quit.
    ///
    /// The duration is specified as a series of numbers followed by time units.
    /// Whitespace is allowed between numbers and units. Each number and unit will
    /// be added together to form the total duration.
    ///
    /// The allowed time units are as follows:
    ///
    ///   * `y` - 365 SI days, or 8760 hours, or 31536000 seconds. No adjustments
    ///     are made for leap years or leap seconds.
    ///   * `d` - one SI day, or 86400 seconds.
    ///   * `h` - one hour, or 3600 seconds.
    ///   * `m` - one minute, or 60 seconds.
    ///   * `s` - one second.
    ///   * `ms` - one millisecond, or 0.001 second.
    ///   * `us` or `µs` - one microsecond, or 0.000001 second.
    ///   * `ns` - one nanosecond, or 0.000000001 second.
    ///
    /// Examples:
    ///   * `1h30m`
    ///   * `45s`
    ///
    /// Units can be repeated and will be added together. This means that
    /// `1h1h` is equivalent to `2h`. This is confusing and should be avoided.
    /// A future update may disallow this.
    ///
    /// This configuration is only supported on macOS. Linux doesn't
    /// support undo operations at all so this configuration has no
    /// effect.
    ///
    /// Available since: 1.2.0
    pub undo_timeout: Duration,

    // @"quick-terminal-position": QuickTerminalPosition = .top,
    /// The position of the "quick" terminal window. To learn more about the
    /// quick terminal, see the documentation for the `toggle_quick_terminal`
    /// binding action.
    ///
    /// Valid values are:
    ///
    ///   * `top` - Terminal appears at the top of the screen.
    ///   * `bottom` - Terminal appears at the bottom of the screen.
    ///   * `left` - Terminal appears at the left of the screen.
    ///   * `right` - Terminal appears at the right of the screen.
    ///   * `center` - Terminal appears at the center of the screen.
    ///
    /// On macOS, changing this configuration requires restarting Rustty
    /// completely.
    ///
    /// Note: There is no default keybind for toggling the quick terminal.
    /// To enable this feature, bind the `toggle_quick_terminal` action to a key.
    pub quick_terminal_position: QuickTerminalPosition,
    // @"quick-terminal-size": QuickTerminalSize = .{},
    /// The size of the quick terminal.
    ///
    /// The size can be specified either as a percentage of the screen dimensions
    /// (height/width), or as an absolute size in pixels. Percentage values are
    /// suffixed with `%` (e.g. `20%`) while pixel values are suffixed with `px`
    /// (e.g. `300px`). A bare value without a suffix is a config error.
    ///
    /// When only one size is specified, the size parameter affects the size of
    /// the quick terminal on its *primary axis*, which depends on its position:
    /// height for quick terminals placed on the top or bottom, and width for left
    /// or right. The primary axis of a centered quick terminal depends on the
    /// monitor's orientation: height when on a landscape monitor, and width when
    /// on a portrait monitor.
    ///
    /// The *secondary axis* would be maximized for non-center positioned
    /// quick terminals unless another size parameter is specified, separated
    /// from the first by a comma (`,`). Percentage and pixel sizes can be mixed
    /// together: for instance, a size of `50%,500px` for a top-positioned quick
    /// terminal would be half a screen tall, and 500 pixels wide.
    ///
    /// Available since: 1.2.0
    pub quick_terminal_size: QuickTerminalSize,
    // @"gtk-quick-terminal-layer": QuickTerminalLayer = .top,
    /// The layer of the quick terminal window. The higher the layer,
    /// the more windows the quick terminal may conceal.
    ///
    /// Valid values are:
    ///
    ///  * `overlay`
    ///
    ///    The quick terminal appears in front of all windows.
    ///
    ///  * `top` (default)
    ///
    ///    The quick terminal appears in front of normal windows but behind
    ///    fullscreen overlays like lock screens.
    ///
    ///  * `bottom`
    ///
    ///    The quick terminal appears behind normal windows but in front of
    ///    wallpapers and other windows in the background layer.
    ///
    ///  * `background`
    ///
    ///    The quick terminal appears behind all windows.
    ///
    /// GTK Wayland only.
    ///
    /// Available since: 1.2.0
    pub gtk_quick_terminal_layer: QuickTerminalLayer,
    // @"gtk-quick-terminal-namespace": [:0]const u8 = "ghostty-quick-terminal",
    /// The namespace for the quick terminal window.
    ///
    /// This is an identifier that is used by the Wayland compositor and/or
    /// scripts to determine the type of layer surfaces and to possibly apply
    /// unique effects.
    ///
    /// GTK Wayland only.
    ///
    /// Available since: 1.2.0
    pub gtk_quick_terminal_namespace: Vec<u8>,
    // @"quick-terminal-screen": QuickTerminalScreen = .main,
    /// The screen where the quick terminal should show up.
    ///
    /// Valid values are:
    ///
    ///  * `main` - The screen that the operating system recommends as the main
    ///    screen. On macOS, this is the screen that is currently receiving
    ///    keyboard input. This screen is defined by the operating system and
    ///    not chosen by Rustty.
    ///
    ///  * `mouse` - The screen that the mouse is currently hovered over.
    ///
    ///  * `macos-menu-bar` - The screen that contains the macOS menu bar as
    ///    set in the display settings on macOS. This is a bit confusing because
    ///    every screen on macOS has a menu bar, but this is the screen that
    ///    contains the primary menu bar.
    ///
    /// The default value is `main` because this is the recommended screen
    /// by the operating system.
    ///
    /// On macOS, `macos-menu-bar` uses the screen containing the menu bar.
    /// On Linux/Wayland, `macos-menu-bar` is treated as equivalent to `main`.
    ///
    /// Note: On Linux, there is no universal concept of a "primary" monitor.
    /// Rustty uses the compositor-reported primary output when available and
    /// falls back to the first monitor reported by GDK if no primary output can
    /// be resolved.
    pub quick_terminal_screen: QuickTerminalScreen,
    // @"quick-terminal-animation-duration": f64 = 0.2,
    /// Duration (in seconds) of the quick terminal enter and exit animation.
    /// Set it to 0 to disable animation completely. This can be changed at
    /// runtime.
    ///
    /// Only implemented on macOS.
    pub quick_terminal_animation_duration: f64,
    // @"quick-terminal-autohide": bool = switch (builtin.os.tag) {
    //     .linux => false, .macos => true, else => false,
    // },
    /// Automatically hide the quick terminal when focus shifts to another window.
    /// Set it to false for the quick terminal to remain open even when it loses focus.
    ///
    /// Defaults to true on macOS and on false on Linux/BSD. This is because global
    /// shortcuts on Linux require system configuration and are considerably less
    /// accessible than on macOS, meaning that it is more preferable to keep the
    /// quick terminal open until the user has completed their task.
    /// This default may change in the future.
    pub quick_terminal_autohide: bool,
    // @"quick-terminal-space-behavior": QuickTerminalSpaceBehavior = .move,
    /// This configuration option determines the behavior of the quick terminal
    /// when switching between macOS spaces. macOS spaces are virtual desktops
    /// that can be manually created or are automatically created when an
    /// application is in full-screen mode.
    ///
    /// Valid values are:
    ///
    ///  * `move` - When switching to another space, the quick terminal will
    ///    also moved to the current space.
    ///
    ///  * `remain` - The quick terminal will stay only in the space where it
    ///    was originally opened and will not follow when switching to another
    ///    space.
    ///
    /// The default value is `move`.
    ///
    /// Only implemented on macOS.
    /// On Linux the behavior is always equivalent to `move`.
    ///
    /// Available since: 1.1.0
    pub quick_terminal_space_behavior: QuickTerminalSpaceBehavior,
    // @"quick-terminal-keyboard-interactivity": QuickTerminalKeyboardInteractivity = .@"on-demand",
    /// Determines under which circumstances that the quick terminal should receive
    /// keyboard input. See the corresponding [Wayland documentation](https://wayland.app/protocols/wlr-layer-shell-unstable-v1#zwlr_layer_surface_v1:enum:keyboard_interactivity)
    /// for a more detailed explanation of the behavior of each option.
    ///
    /// > [!NOTE]
    /// > The exact behavior of each option may differ significantly across
    /// > compositors -- experiment with them on your system to find one that
    /// > suits your liking!
    ///
    /// Valid values are:
    ///
    ///  * `none`
    ///
    ///    The quick terminal will not receive any keyboard input.
    ///
    ///  * `on-demand` (default)
    ///
    ///    The quick terminal would only receive keyboard input when it is focused.
    ///
    ///  * `exclusive`
    ///
    ///    The quick terminal will always receive keyboard input, even when another
    ///    window is currently focused.
    ///
    /// Only has an effect on Linux Wayland.
    /// On macOS the behavior is always equivalent to `on-demand`.
    ///
    /// Available since: 1.2.0
    pub quick_terminal_keyboard_interactivity: QuickTerminalKeyboardInteractivity,

    // @"shell-integration": ShellIntegration = .detect,
    /// Whether to enable shell integration auto-injection or not. Shell integration
    /// greatly enhances the terminal experience by enabling a number of features:
    ///
    ///   * Working directory reporting so new tabs, splits inherit the
    ///     previous terminal's working directory.
    ///
    ///   * Prompt marking that enables the "jump_to_prompt" keybinding.
    ///
    ///   * If you're sitting at a prompt, closing a terminal will not ask
    ///     for confirmation.
    ///
    ///   * Resizing the window with a complex prompt usually paints much
    ///     better.
    ///
    /// Allowable values are:
    ///
    ///   * `none` - Do not do any automatic injection. You can still manually
    ///     configure your shell to enable the integration.
    ///
    ///   * `detect` - Detect the shell based on the filename.
    ///
    ///   * `bash`, `elvish`, `fish`, `nushell`, `zsh` - Use this specific shell injection scheme.
    ///
    /// The default value is `detect`.
    pub shell_integration: ShellIntegration,
    // @"shell-integration-features": ShellIntegrationFeatures = .{},
    /// Shell integration features to enable. These require our shell integration
    /// to be loaded, either automatically via shell-integration or manually.
    ///
    /// The format of this is a list of features to enable separated by commas. If
    /// you prefix a feature with `no-` then it is disabled. If you omit a feature,
    /// its default value is used, so you must explicitly disable features you don't
    /// want. You can also use `true` or `false` to turn all features on or off.
    ///
    /// Example: `cursor`, `no-cursor`, `sudo`, `no-sudo`, `title`, `no-title`
    ///
    /// Available features:
    ///
    ///   * `cursor` - Set the cursor to a bar at the prompt.
    ///
    ///   * `sudo` - Set sudo wrapper to preserve terminfo.
    ///
    ///   * `title` - Set the window title via shell integration.
    ///
    ///   * `ssh-env` - Enable SSH environment variable compatibility. Automatically
    ///     converts TERM from `xterm-rustty` to `xterm-256color` when connecting to
    ///     remote hosts and propagates COLORTERM, TERM_PROGRAM, and TERM_PROGRAM_VERSION.
    ///     Whether or not these variables will be accepted by the remote host(s) will
    ///     depend on whether or not the variables are allowed in their sshd_config.
    ///     (Available since: 1.2.0)
    ///
    ///   * `ssh-terminfo` - Enable automatic terminfo installation on remote hosts.
    ///     Attempts to install Rustty's embedded terminfo entry using `tic` on local
    ///     cache misses. Requires `tic` to be available on remote hosts. Successful
    ///     installations are cached locally to avoid repeat installations.
    ///     If desired, the `+ssh-cache` CLI action can be used to manage the installation
    ///     cache manually using various arguments.
    ///     (Available since: 1.2.0)
    ///
    ///   * `path` - Add Rustty's binary directory to PATH. This ensures the `rustty`
    ///     command is available in the shell even if shell init scripts reset PATH.
    ///     This is particularly useful on macOS where PATH is often overridden by
    ///     system scripts. The directory is only added if not already present.
    ///
    /// SSH features work independently and can be combined for optimal experience:
    /// when both `ssh-env` and `ssh-terminfo` are enabled, Rustty will install its
    /// terminfo on remote hosts and use `xterm-rustty` as TERM, falling back to
    /// `xterm-256color` with environment variables if terminfo installation fails.
    pub shell_integration_features: ShellIntegrationFeatures,

    // @"command-palette-entry": RepeatableCommand = .{},
    /// Custom entries into the command palette.
    ///
    /// Each entry requires the title, the corresponding action, and an optional
    /// description. Each field should be prefixed with the field name, a colon
    /// (`:`), and then the specified value. The syntax for actions is identical
    /// to the one for keybind actions. Whitespace in between fields is ignored.
    ///
    /// If you need to embed commas or any other special characters in the values,
    /// enclose the value in double quotes and it will be interpreted as a Zig
    /// string literal. This is also useful for including whitespace at the
    /// beginning or the end of a value. See the
    /// [Zig documentation](https://ziglang.org/documentation/master/#Escape-Sequences)
    /// for more information on string literals. Note that multiline string literals
    /// are not supported.
    ///
    /// Double quotes can not be used around the field names.
    ///
    /// ```ini
    /// command-palette-entry = title:Reset Font Style, action:csi:0m
    /// command-palette-entry = title:Crash on Main Thread,description:Causes a crash on the main (UI) thread.,action:crash:main
    /// command-palette-entry = title:Focus Split: Right,description:"Focus the split to the right, if it exists.",action:goto_split:right
    /// command-palette-entry = title:"Rustty",description:"Add a little Rustty to your terminal.",action:"text:\xf0\x9f\x91\xbb"
    /// ```
    ///
    /// There are some additional special values that can be specified for
    /// command-palette-entry:
    ///
    ///   * `command-palette-entry=clear` will clear all command entries. Warning: this
    ///     removes ALL entries up to this point, including the default
    ///     entries. Available since: 1.4.0
    ///
    /// By default, the command palette is preloaded with most actions that might
    /// be useful in an interactive setting yet do not have easily accessible or
    /// memorizable shortcuts. The default entries can be restored by setting this
    /// setting to an empty value:
    ///
    /// ```ini
    /// command-palette-entry =
    /// ```
    ///
    /// Available since: 1.2.0
    pub command_palette_entry: RepeatableCommand,

    // @"osc-color-report-format": OSCColorReportFormat = .@"16-bit",
    /// Sets the reporting format for OSC sequences that request color information.
    /// Rustty currently supports OSC 10 (foreground), OSC 11 (background), and
    /// OSC 4 (256 color palette) queries, and by default the reported values
    /// are scaled-up RGB values, where each component are 16 bits. This is how
    /// most terminals report these values. However, some legacy applications may
    /// require 8-bit, unscaled, components. We also support turning off reporting
    /// altogether. The components are lowercase hex values.
    ///
    /// Allowable values are:
    ///
    ///   * `none` - OSC 4/10/11 queries receive no reply
    ///
    ///   * `8-bit` - Color components are return unscaled, e.g. `rr/gg/bb`
    ///
    ///   * `16-bit` - Color components are returned scaled, e.g. `rrrr/gggg/bbbb`
    ///
    /// The default value is `16-bit`.
    pub osc_color_report_format: OSCColorReportFormat,
    // @"vt-kam-allowed": bool = false,
    /// If true, allows the "KAM" mode (ANSI mode 2) to be used within
    /// the terminal. KAM disables keyboard input at the request of the
    /// application. This is not a common feature and is not recommended
    /// to be enabled. This will not be documented further because
    /// if you know you need KAM, you know. If you don't know if you
    /// need KAM, you don't need it.
    pub vt_kam_allowed: bool,

    // @"custom-shader": RepeatablePath = .{},
    /// Custom shaders to run after the default shaders. This is a file path
    /// to a GLSL-syntax shader for all platforms.
    ///
    /// Warning: Invalid shaders can cause Rustty to become unusable such as by
    /// causing the window to be completely black. If this happens, you can
    /// unset this configuration to disable the shader.
    ///
    /// Custom shader support is based on and compatible with the Shadertoy shaders.
    /// Shaders should specify a `mainImage` function and the available uniforms
    /// largely match Shadertoy, with some caveats and Rustty-specific extensions.
    ///
    /// The uniform values available to shaders are as follows:
    ///
    ///  * `sampler2D iChannel0` - Input texture.
    ///
    ///    A texture containing the current terminal screen. If multiple custom
    ///    shaders are specified, the output of previous shaders is written to
    ///    this texture, to allow combining multiple effects.
    ///
    ///  * `vec3 iResolution` - Output texture size, `[width, height, 1]` (in px).
    ///
    ///  * `float iTime` - Time in seconds since first frame was rendered.
    ///
    ///  * `float iTimeDelta` - Time in seconds since previous frame was rendered.
    ///
    ///  * `float iFrameRate` - Average framerate. (NOT CURRENTLY SUPPORTED)
    ///
    ///  * `int iFrame` - Number of frames that have been rendered so far.
    ///
    ///  * `float iChannelTime[4]` - Current time for video or sound input. (N/A)
    ///
    ///  * `vec3 iChannelResolution[4]` - Resolutions of the 4 input samplers.
    ///
    ///    Currently only `iChannel0` exists, and `iChannelResolution[0]` is
    ///    identical to `iResolution`.
    ///
    ///  * `vec4 iMouse` - Mouse input info. (NOT CURRENTLY SUPPORTED)
    ///
    ///  * `vec4 iDate` - Date/time info. (NOT CURRENTLY SUPPORTED)
    ///
    ///  * `float iSampleRate` - Sample rate for audio. (N/A)
    ///
    /// Rustty-specific extensions:
    ///
    ///  * `vec4 iCurrentCursor` - Info about the terminal cursor.
    ///
    ///    - `iCurrentCursor.xy` is the -X, +Y corner of the current cursor.
    ///    - `iCurrentCursor.zw` is the width and height of the current cursor.
    ///
    ///  * `vec4 iPreviousCursor` - Info about the previous terminal cursor.
    ///
    ///  * `vec4 iCurrentCursorColor` - Color of the terminal cursor.
    ///
    ///  * `vec4 iPreviousCursorColor` - Color of the previous terminal cursor.
    ///
    ///  * `vec4 iCurrentCursorStyle` - Style of the terminal cursor
    ///
    ///    Macros simplified use are defined for the various cursor styles:
    ///
    ///    - `CURSORSTYLE_BLOCK` or `0`
    ///    - `CURSORSTYLE_BLOCK_HOLLOW` or `1`
    ///    - `CURSORSTYLE_BAR` or `2`
    ///    - `CURSORSTYLE_UNDERLINE` or `3`
    ///    - `CURSORSTYLE_LOCK` or `4`
    ///
    ///  * `vec4 iPreviousCursorStyle` - Style of the previous terminal cursor
    ///
    ///  * `vec4 iCursorVisible` - Visibility of the terminal cursor.
    ///
    ///  * `float iTimeCursorChange` - Timestamp of terminal cursor change.
    ///
    ///    When the terminal cursor changes position or color, this is set to
    ///    the same time as the `iTime` uniform, allowing you to compute the
    ///    time since the change by subtracting this from `iTime`.
    ///
    ///  * `float iTimeFocus` - Timestamp when the surface last gained iFocus.
    ///
    ///    When the surface gains focus, this is set to the current value of
    ///    `iTime`, similar to how `iTimeCursorChange` works. This allows you
    ///    to compute the time since focus was gained or lost by calculating
    ///    `iTime - iTimeFocus`. Use this to create animations that restart
    ///    when the terminal regains focus.
    ///
    ///  * `int iFocus` - Current focus state of the surface.
    ///
    ///    Set to 1.0 when the surface is focused, 0.0 when unfocused. This
    ///    allows shaders to detect unfocused state and avoid animation artifacts
    ///    from large time deltas caused by infrequent "deceptive frames"
    ///    (e.g., modifier key presses, link hover events in unfocused split panes).
    ///    Check `iFocus > 0` to determine if the surface is currently focused.
    ///
    ///  * `vec3 iPalette[256]` - The 256-color terminal palette.
    ///
    ///    RGB values for all 256 colors in the terminal palette, normalized
    ///    to [0.0, 1.0]. Index 0-15 are the ANSI colors, 16-231 are the 6x6x6
    ///    color cube, and 232-255 are the grayscale colors.
    ///
    ///  * `vec3 iBackgroundColor` - Terminal background color (RGB).
    ///
    ///  * `vec3 iForegroundColor` - Terminal foreground color (RGB).
    ///
    ///  * `vec3 iCursorColor` - Terminal cursor color (RGB).
    ///
    ///  * `vec3 iCursorText` - Terminal cursor text color (RGB).
    ///
    ///  * `vec3 iSelectionBackgroundColor` - Selection background color (RGB).
    ///
    ///  * `vec3 iSelectionForegroundColor` - Selection foreground color (RGB).
    ///
    /// If the shader fails to compile, the shader will be ignored. Any errors
    /// related to shader compilation will not show up as configuration errors
    /// and only show up in the log, since shader compilation happens after
    /// configuration loading on the dedicated render thread.  For interactive
    /// development, use [shadertoy.com](https://shadertoy.com).
    ///
    /// This can be repeated multiple times to load multiple shaders. The shaders
    /// will be run in the order they are specified.
    ///
    /// This can be changed at runtime and will affect all open terminals.
    pub custom_shader: RepeatablePath,
    // @"custom-shader-animation": CustomShaderAnimation = .true,
    /// If `true` (default), the focused terminal surface will run an animation
    /// loop when custom shaders are used. This uses slightly more CPU (generally
    /// less than 10%) but allows the shader to animate. This only runs if there
    /// are custom shaders and the terminal is focused.
    ///
    /// If this is set to `false`, the terminal and custom shader will only render
    /// when the terminal is updated. This is more efficient but the shader will
    /// not animate.
    ///
    /// This can also be set to `always`, which will always run the animation
    /// loop regardless of whether the terminal is focused or not. The animation
    /// loop will still only run when custom shaders are used. Note that this
    /// will use more CPU per terminal surface and can become quite expensive
    /// depending on the shader and your terminal usage.
    ///
    /// This can be changed at runtime and will affect all open terminals.
    pub custom_shader_animation: CustomShaderAnimation,

    // @"bell-features": BellFeatures = .{},
    /// Bell features to enable if bell support is available in your runtime. Not
    /// all features are available on all runtimes. The format of this is a list of
    /// features to enable separated by commas. If you prefix a feature with `no-`
    /// then it is disabled. If you omit a feature, its default value is used.
    ///
    /// Valid values are:
    ///
    ///  * `system`
    ///
    ///    Instruct the system to notify the user using built-in system functions.
    ///    This could result in an audiovisual effect, a notification, or something
    ///    else entirely. Changing these effects require altering system settings:
    ///    for instance under the "Sound > Alert Sound" setting in GNOME,
    ///    or the "Accessibility > System Bell" settings in KDE Plasma.
    ///
    ///    On macOS, this plays the system alert sound.
    ///
    ///  * `audio`
    ///
    ///    Play a custom sound. (Available since 1.3.0 on macOS)
    ///
    ///  * `attention` *(enabled by default)*
    ///
    ///    Request the user's attention when Rustty is unfocused, until it has
    ///    received focus again. On macOS, this will bounce the app icon in the
    ///    dock once. On Linux, the behavior depends on the desktop environment
    ///    and/or the window manager/compositor:
    ///
    ///    - On KDE, the background of the desktop icon in the task bar would be
    ///      highlighted;
    ///
    ///    - On GNOME, you may receive a notification that, when clicked, would
    ///      bring the Rustty window into focus;
    ///
    ///    - On Sway, the window may be decorated with a distinctly colored border;
    ///
    ///    - On other systems this may have no effect at all.
    ///
    ///  * `title` *(enabled by default)*
    ///
    ///    Prepend a bell emoji (🔔) to the title of the alerted surface until the
    ///    terminal is re-focused or interacted with (such as on keyboard input).
    ///
    ///  * `border`
    ///
    ///    Display a border around the alerted surface until the terminal is
    ///    re-focused or interacted with (such as on keyboard input).
    ///
    ///    Available since: 1.2.0 on GTK, 1.2.1 on macOS
    ///
    /// Example: `audio`, `no-audio`, `system`, `no-system`
    ///
    /// Available since: 1.2.0
    pub bell_features: BellFeatures,
    // @"bell-audio-path": ?Path = null,
    /// If `audio` is an enabled bell feature, this is a path to an audio file. If
    /// the path is not absolute, it is considered relative to the directory of the
    /// configuration file that it is referenced from, or from the current working
    /// directory if this is used as a CLI flag. The path may be prefixed with `~/`
    /// to reference the user's home directory.
    ///
    /// Available since: 1.2.0 on GTK, 1.3.0 on macOS.
    pub bell_audio_path: Option<Path>,
    // @"bell-audio-volume": f64 = 0.5,
    /// If `audio` is an enabled bell feature, this is the volume to play the audio
    /// file at (relative to the system volume). This is a floating point number
    /// ranging from 0.0 (silence) to 1.0 (as loud as possible). The default is 0.5.
    ///
    /// Available since: 1.2.0 on GTK, 1.3.0 on macOS.
    pub bell_audio_volume: f64,

    // @"app-notifications": AppNotifications = .{},
    /// Control the in-app notifications that Rustty shows.
    ///
    /// On Linux (GTK), in-app notifications show up as toasts. Toasts appear
    /// overlaid on top of the terminal window. They are used to show information
    /// that is not critical but may be important.
    ///
    /// Possible notifications are:
    ///
    ///   - `clipboard-copy` (default: true) - Show a notification when text is copied
    ///     to the clipboard.
    ///   - `config-reload` (default: true) - Show a notification when
    ///     the configuration is reloaded.
    ///
    /// To specify a notification to enable, specify the name of the notification.
    /// To specify a notification to disable, prefix the name with `no-`. For
    /// example, to disable `clipboard-copy`, set this configuration to
    /// `no-clipboard-copy`. To enable it, set this configuration to `clipboard-copy`.
    ///
    /// Multiple notifications can be enabled or disabled by separating them
    /// with a comma.
    ///
    /// A value of "false" will disable all notifications. A value of "true" will
    /// enable all notifications.
    ///
    /// This configuration only applies to GTK.
    ///
    /// Available since: 1.1.0
    pub app_notifications: AppNotifications,

    // @"macos-non-native-fullscreen": NonNativeFullscreen = .false,
    /// If anything other than false, fullscreen mode on macOS will not use the
    /// native fullscreen, but make the window fullscreen without animations and
    /// using a new space. It's faster than the native fullscreen mode since it
    /// doesn't use animations.
    ///
    /// Important: tabs DO NOT WORK in this mode. Non-native fullscreen removes
    /// the titlebar and macOS native tabs require the titlebar. If you use tabs,
    /// you should not use this mode.
    ///
    /// If you fullscreen a window with tabs, the currently focused tab will
    /// become fullscreen while the others will remain in a separate window in
    /// the background. You can switch to that window using normal window-switching
    /// keybindings such as command+tilde. When you exit fullscreen, the window
    /// will return to the tabbed state it was in before.
    ///
    /// Allowable values are:
    ///
    ///   * `true` - Use non-native macOS fullscreen, hide the menu bar
    ///   * `false` - Use native macOS fullscreen
    ///   * `visible-menu` - Use non-native macOS fullscreen, keep the menu bar
    ///     visible
    ///   * `padded-notch` - Use non-native macOS fullscreen, hide the menu bar,
    ///     but ensure the window is not obscured by the notch on applicable
    ///     devices. The area around the notch will remain transparent currently,
    ///     but in the future we may fill it with the window background color.
    ///
    /// Changing this option at runtime works, but will only apply to the next
    /// time the window is made fullscreen. If a window is already fullscreen,
    /// it will retain the previous setting until fullscreen is exited.
    pub macos_non_native_fullscreen: NonNativeFullscreen,
    // @"macos-window-buttons": MacWindowButtons = .visible,
    /// Whether the window buttons in the macOS titlebar are visible. The window
    /// buttons are the colored buttons in the upper left corner of most macOS apps,
    /// also known as the traffic lights, that allow you to close, miniaturize, and
    /// zoom the window.
    ///
    /// This setting has no effect when `window-decoration = none` or
    /// `macos-titlebar-style = hidden`, as the window buttons are always hidden in
    /// these modes.
    ///
    /// Valid values are:
    ///
    ///   * `visible` - Show the window buttons.
    ///   * `hidden` - Hide the window buttons.
    ///
    /// The default value is `visible`.
    ///
    /// Changing this option at runtime only applies to new windows.
    ///
    /// Available since: 1.2.0
    pub macos_window_buttons: MacWindowButtons,
    // @"macos-titlebar-style": MacTitlebarStyle = .transparent,
    /// The style of the macOS titlebar. Available values are: "native",
    /// "transparent", "tabs", and "hidden".
    ///
    /// The "native" style uses the native macOS titlebar with zero customization.
    /// The titlebar will match your window theme (see `window-theme`).
    ///
    /// The "transparent" style is the same as "native" but the titlebar will
    /// be transparent and allow your window background color to come through.
    /// This makes a more seamless window appearance but looks a little less
    /// typical for a macOS application and may not work well with all themes.
    ///
    /// The "transparent" style will also update in real-time to dynamic
    /// changes to the window background color, e.g. via OSC 11. To make this
    /// more aesthetically pleasing, this only happens if the terminal is
    /// a window, tab, or split that borders the top of the window. This
    /// avoids a disjointed appearance where the titlebar color changes
    /// but all the topmost terminals don't match.
    ///
    /// The "tabs" style is a completely custom titlebar that integrates the
    /// tab bar into the titlebar. This titlebar always matches the background
    /// color of the terminal. There are some limitations to this style:
    /// On macOS 13 and below, saved window state will not restore tabs correctly.
    /// macOS 14 does not have this issue and any other macOS version has not
    /// been tested.
    ///
    /// The "hidden" style hides the titlebar. Unlike `window-decoration = none`,
    /// however, it does not remove the frame from the window or cause it to have
    /// squared corners. Changing to or from this option at run-time may affect
    /// existing windows in buggy ways.
    ///
    /// When "hidden", the top titlebar area can no longer be used for dragging
    /// the window. To drag the window, you can use option+click on the resizable
    /// areas of the frame to drag the window. This is a standard macOS behavior
    /// and not something Rustty enables.
    ///
    /// The default value is "transparent". This is an opinionated choice
    /// but its one I think is the most aesthetically pleasing and works in
    /// most cases.
    ///
    /// Changing this option at runtime only applies to new windows.
    pub macos_titlebar_style: MacTitlebarStyle,
    // @"macos-titlebar-proxy-icon": MacTitlebarProxyIcon = .visible,
    /// Whether the proxy icon in the macOS titlebar is visible. The proxy icon
    /// is the icon that represents the folder of the current working directory.
    /// You can see this very clearly in the macOS built-in Terminal.app
    /// titlebar.
    ///
    /// The proxy icon is only visible with the native macOS titlebar style.
    ///
    /// Valid values are:
    ///
    ///   * `visible` - Show the proxy icon.
    ///   * `hidden` - Hide the proxy icon.
    ///
    /// The default value is `visible`.
    ///
    /// This setting can be changed at runtime and will affect all currently
    /// open windows but only after their working directory changes again.
    /// Therefore, to make this work after changing the setting, you must
    /// usually `cd` to a different directory, open a different file in an
    /// editor, etc.
    pub macos_titlebar_proxy_icon: MacTitlebarProxyIcon,
    // @"macos-dock-drop-behavior": MacOSDockDropBehavior = .@"new-tab",
    /// Controls the windowing behavior when dropping a file or folder
    /// onto the Rustty icon in the macOS dock.
    ///
    /// Valid values are:
    ///
    ///   * `new-tab` - Create a new tab in the current window, or open
    ///     a new window if none exist.
    ///   * `new-window` - Create a new window unconditionally.
    ///
    /// The default value is `new-tab`.
    ///
    /// This setting is only supported on macOS and has no effect on other
    /// platforms.
    pub macos_dock_drop_behavior: MacOSDockDropBehavior,
    // @"macos-option-as-alt": ?inputpkg.OptionAsAlt = null,
    /// macOS doesn't have a distinct "alt" key and instead has the "option"
    /// key which behaves slightly differently. On macOS by default, the
    /// option key plus a character will sometimes produce a Unicode character.
    /// For example, on US standard layouts option-b produces "∫". This may be
    /// undesirable if you want to use "option" as an "alt" key for keybindings
    /// in terminal programs or shells.
    ///
    /// This configuration lets you change the behavior so that option is treated
    /// as alt.
    ///
    /// The default behavior (unset) will depend on your active keyboard
    /// layout. If your keyboard layout is one of the keyboard layouts listed
    /// below, then the default value is "true". Otherwise, the default
    /// value is "false". Keyboard layouts with a default value of "true" are:
    ///
    ///   - U.S. Standard
    ///   - U.S. International
    ///
    /// Note that if an *Option*-sequence doesn't produce a printable character, it
    /// will be treated as *Alt* regardless of this setting. (e.g. `alt+ctrl+a`).
    ///
    /// Explicit values that can be set:
    ///
    /// If `true`, the *Option* key will be treated as *Alt*. This makes terminal
    /// sequences expecting *Alt* to work properly, but will break Unicode input
    /// sequences on macOS if you use them via the *Alt* key.
    ///
    /// You may set this to `false` to restore the macOS *Alt* key unicode
    /// sequences but this will break terminal sequences expecting *Alt* to work.
    ///
    /// The values `left` or `right` enable this for the left or right *Option*
    /// key, respectively.
    pub macos_option_as_alt: Option<rustty_input::config::OptionAsAlt>,
    // @"macos-window-shadow": bool = true,
    /// Whether to enable the macOS window shadow. The default value is true.
    /// With some window managers and window transparency settings, you may
    /// find false more visually appealing.
    pub macos_window_shadow: bool,
    // @"macos-hidden": MacHidden = .never,
    /// If true, the macOS icon in the dock and app switcher will be hidden. This is
    /// mainly intended for those primarily using the quick-terminal mode.
    ///
    /// Note that setting this to true means that keyboard layout changes
    /// will no longer be automatic.
    ///
    /// Control whether macOS app is excluded from the dock and app switcher,
    /// a "hidden" state. This is mainly intended for those primarily using
    /// quick-terminal mode, but is a general configuration for any use
    /// case.
    ///
    /// Available values:
    ///
    ///   * `never` - The macOS app is never hidden.
    ///   * `always` - The macOS app is always hidden.
    ///
    /// Note: When the macOS application is hidden, keyboard layout changes
    /// will no longer be automatic. This is a limitation of macOS.
    ///
    /// Available since: 1.2.0
    pub macos_hidden: MacHidden,
    // @"macos-auto-secure-input": bool = true,
    /// If true, Rustty on macOS will automatically enable the "Secure Input"
    /// feature when it detects that a password prompt is being displayed.
    ///
    /// "Secure Input" is a macOS security feature that prevents applications from
    /// reading keyboard events. This can always be enabled manually using the
    /// `rustty > Secure Keyboard Entry` menu item.
    ///
    /// Note that automatic password prompt detection is based on heuristics
    /// and may not always work as expected. Specifically, it does not work
    /// over SSH connections, but there may be other cases where it also
    /// doesn't work.
    ///
    /// A reason to disable this feature is if you find that it is interfering
    /// with legitimate accessibility software (or software that uses the
    /// accessibility APIs), since secure input prevents any application from
    /// reading keyboard events.
    pub macos_auto_secure_input: bool,
    // @"macos-secure-input-indication": bool = true,
    /// If true, Rustty will show a graphical indication when secure input is
    /// enabled. This indication is generally recommended to know when secure input
    /// is enabled.
    ///
    /// Normally, secure input is only active when a password prompt is displayed
    /// or it is manually (and typically temporarily) enabled. However, if you
    /// always have secure input enabled, the indication can be distracting and
    /// you may want to disable it.
    pub macos_secure_input_indication: bool,
    // @"macos-applescript": bool = true,
    /// If true, Rustty exposes and handles the built-in AppleScript dictionary
    /// on macOS.
    ///
    /// If false, all AppleScript interactions are disabled. This includes
    /// AppleScript commands and AppleScript object lookup for windows, tabs,
    /// and terminals.
    ///
    /// The default is true.
    pub macos_applescript: bool,
    // @"macos-icon": MacAppIcon = .official,
    /// Customize the macOS app icon.
    ///
    /// This only affects the icon that appears in the dock, application
    /// switcher, etc. This does not affect the icon in Finder because
    /// that is controlled by a hardcoded value in the signed application
    /// bundle and can't be changed at runtime. For more details on what
    /// exactly is affected, see the `NSApplication.icon` Apple documentation;
    /// that is the API that is being used to set the icon.
    ///
    /// Valid values:
    ///
    ///  * `official` - Use the official Rustty icon.
    ///  * `blueprint`, `chalkboard`, `microchip`, `glass`, `holographic`,
    ///    `paper`, `retro`, `xray` - Official variants of the Rustty icon
    ///    hand-created by artists (no AI).
    ///  * `custom` - Use a completely custom icon. The location must be specified
    ///    using the additional `macos-custom-icon` configuration
    ///  * `custom-style` - Use the official Rustty icon but with custom
    ///    styles applied to various layers. The custom styles must be
    ///    specified using the additional `macos-icon`-prefixed configurations.
    ///    The `macos-icon-ghost-color` and `macos-icon-screen-color`
    ///    configurations are required for this style.
    ///
    /// WARNING: The `custom-style` option is _experimental_. We may change
    /// the format of the custom styles in the future. We're still finalizing
    /// the exact layers and customization options that will be available.
    ///
    /// Other caveats:
    ///
    ///   * The icon in the update dialog will always be the official icon.
    ///     This is because the update dialog is managed through a
    ///     separate framework and cannot be customized without significant
    ///     effort.
    pub macos_icon: MacAppIcon,
    // @"macos-custom-icon": ?[:0]const u8 = null,
    /// The absolute path to the custom icon file.
    /// Supported formats include PNG, JPEG, and ICNS.
    ///
    /// Defaults to `~/.config/rustty/rustty.icns`
    pub macos_custom_icon: Option<Vec<u8>>,
    // @"macos-icon-frame": MacAppIconFrame = .aluminum,
    /// The material to use for the frame of the macOS app icon.
    ///
    /// Valid values:
    ///
    ///  * `aluminum` - A brushed aluminum frame. This is the default.
    ///  * `beige` - A classic 90's computer beige frame.
    ///  * `plastic` - A glossy, dark plastic frame.
    ///  * `chrome` - A shiny chrome frame.
    ///
    /// Note: This configuration is required when `macos-icon` is set to
    /// `custom-style`.
    pub macos_icon_frame: MacAppIconFrame,
    // @"macos-icon-ghost-color": ?Color = null,
    /// The color of the ghost in the macOS app icon.
    ///
    /// Note: This configuration is required when `macos-icon` is set to
    /// `custom-style`.
    ///
    /// Specified as either hex (`#RRGGBB` or `RRGGBB`) or a named X11 color.
    pub macos_icon_ghost_color: Option<Color>,
    // @"macos-icon-screen-color": ?ColorList = null,
    /// The color of the screen in the macOS app icon.
    ///
    /// The screen is a linear gradient so you can specify multiple colors
    /// that make up the gradient. Up to 64 comma-separated colors may be
    /// specified as either hex (`#RRGGBB` or `RRGGBB`) or as named X11
    /// colors. The first color is the bottom of the gradient and the last
    /// color is the top of the gradient.
    ///
    /// Note: This configuration is required when `macos-icon` is set to
    /// `custom-style`.
    pub macos_icon_screen_color: Option<ColorList>,
    // @"macos-shortcuts": MacShortcuts = .ask,
    /// Whether macOS Shortcuts are allowed to control Rustty.
    ///
    /// Rustty exposes a number of actions that allow Shortcuts to
    /// control and interact with Rustty. This includes creating new
    /// terminals, sending text to terminals, running commands, invoking
    /// any keybind action, etc.
    ///
    /// This is a powerful feature but can be a security risk if a malicious
    /// shortcut is able to be installed and executed. Therefore, this
    /// configuration allows you to disable this feature.
    ///
    /// Valid values are:
    ///
    /// * `ask` - Ask the user whether for permission. Rustty will remember
    ///   this choice and never ask again. This is similar to other macOS
    ///   permissions such as microphone access, camera access, etc.
    ///
    /// * `allow` - Allow Shortcuts to control Rustty without asking.
    ///
    /// * `deny` - Deny Shortcuts from controlling Rustty.
    ///
    /// Available since: 1.2.0
    pub macos_shortcuts: MacShortcuts,

    // @"linux-cgroup": LinuxCgroup = if (builtin.os.tag == .linux)
    //     .@"single-instance" else .never,
    /// Put every surface (tab, split, window) into a transient `systemd` scope.
    ///
    /// This allows per-surface resource management. For example, if a shell program
    /// is using too much memory, only that shell will be killed by the oom monitor
    /// instead of the entire Rustty process. Similarly, if a shell program is
    /// using too much CPU, only that surface will be CPU-throttled.
    ///
    /// This will cause startup times to be slower (a hundred milliseconds or so),
    /// so the default value is "single-instance." In single-instance mode, only
    /// one instance of Rustty is running (see gtk-single-instance) so the startup
    /// time is a one-time cost. Additionally, single instance Rustty is much
    /// more likely to have many windows, tabs, etc. so cgroup isolation is a
    /// big benefit.
    ///
    /// This feature requires `systemd`. If `systemd` is unavailable, cgroup
    /// initialization will fail. By default, this will not prevent Rustty from
    /// working (see `linux-cgroup-hard-fail`).
    ///
    /// Changing this value and reloading the config will not affect existing
    /// surfaces.
    ///
    /// Valid values are:
    ///
    ///   * `never` - Never use cgroups.
    ///   * `always` - Always use cgroups.
    ///   * `single-instance` - Enable cgroups only for Rustty instances launched
    ///     as single-instance applications (see gtk-single-instance).
    pub linux_cgroup: LinuxCgroup,

    // @"linux-cgroup-memory-limit": ?u64 = null,
    /// Memory limit for any individual terminal process (tab, split, window,
    /// etc.) in bytes. If this is unset then no memory limit will be set.
    ///
    /// Note that this sets the `MemoryHigh` setting on the transient `systemd`
    /// scope, which is a soft limit. You should configure something like
    /// `systemd-oom` to handle killing processes that have too much memory
    /// pressure.
    ///
    /// Changing this value and reloading the config will not affect existing
    /// surfaces.
    ///
    /// See the `systemd.resource-control` manual page for more information:
    /// https://www.freedesktop.org/software/systemd/man/latest/systemd.resource-control.html
    pub linux_cgroup_memory_limit: Option<u64>,
    // @"linux-cgroup-processes-limit": ?u64 = null,
    /// Number of processes limit for any individual terminal process (tab, split,
    /// window, etc.). If this is unset then no limit will be set.
    ///
    /// Note that this sets the `TasksMax` setting on the transient `systemd` scope,
    /// which is a hard limit.
    ///
    /// Changing this value and reloading the config will not affect existing
    /// surfaces.
    ///
    /// See the `systemd.resource-control` manual page for more information:
    /// https://www.freedesktop.org/software/systemd/man/latest/systemd.resource-control.html
    pub linux_cgroup_processes_limit: Option<u64>,
    // @"linux-cgroup-hard-fail": bool = false,
    /// If this is false, then creating a transient `systemd` scope (for
    /// `linux-cgroup`) will be allowed to fail and the failure is ignored. This is
    /// useful if you view cgroup isolation as a "nice to have" and not a critical
    /// resource management feature, because surface creation will not fail if
    /// `systemd` APIs fail.
    ///
    /// If this is true, then any transient `systemd` scope creation failure will
    /// cause surface creation to fail.
    ///
    /// Changing this value and reloading the config will not affect existing
    /// surfaces.
    pub linux_cgroup_hard_fail: bool,

    // @"gtk-opengl-debug": bool = builtin.mode == .Debug,
    /// Enable or disable GTK's OpenGL debugging logs. The default is `true` for
    /// debug builds, `false` for all others.
    ///
    /// Available since: 1.1.0
    pub gtk_opengl_debug: bool,
    // @"gtk-single-instance": GtkSingleInstance = .default,
    /// If `true`, the Rustty GTK application will run in single-instance mode:
    /// each new `rustty` process launched will result in a new window if there is
    /// already a running process.
    ///
    /// If `false`, each new Rustty process will launch a separate application.
    ///
    /// If `detect`, Rustty will assume true (single instance) unless one of
    /// the following scenarios is found:
    ///
    /// 1. TERM_PROGRAM environment variable is a non-empty value. In this
    /// case, we assume Rustty is being launched from a graphical terminal
    /// session and you want a dedicated instance.
    ///
    /// 2. Any CLI arguments exist. In this case, we assume you are passing
    /// custom Rustty configuration. Single instance mode inherits the
    /// configuration from when it was launched, so we must disable single
    /// instance to load the new configuration.
    ///
    /// If either of these scenarios is producing a false positive, you can
    /// set this configuration explicitly to the behavior you want.
    ///
    /// The pre-1.2 option `desktop` has been deprecated. Please replace
    /// this with `detect`.
    ///
    /// The default value is `detect`.
    ///
    /// Note that debug builds of Rustty have a separate single-instance ID
    /// so you can test single instance without conflicting with release builds.
    // PORT NOTE (R1): the reference doc's numbered-list continuations are kept
    // at their exact (lazy) indentation; the cosmetic markdown lint is allowed
    // locally rather than reflowing the ported reference text.
    #[allow(clippy::doc_lazy_continuation)]
    pub gtk_single_instance: GtkSingleInstance,
    // @"gtk-titlebar": bool = true,
    /// When enabled, the full GTK titlebar is displayed instead of your window
    /// manager's simple titlebar. The behavior of this option will vary with your
    /// window manager.
    ///
    /// This option does nothing when `window-decoration` is none or when running
    /// under macOS.
    pub gtk_titlebar: bool,
    // @"gtk-tabs-location": GtkTabsLocation = .top,
    /// Determines the side of the screen that the GTK tab bar will stick to.
    /// Top, bottom, and hidden are supported. The default is top.
    ///
    /// When `hidden` is set, a tab button displaying the number of tabs will appear
    /// in the title bar. It has the ability to open a tab overview for displaying
    /// tabs. Alternatively, you can use the `toggle_tab_overview` action in a
    /// keybind if your window doesn't have a title bar, or you can switch tabs
    /// with keybinds.
    pub gtk_tabs_location: GtkTabsLocation,
    // @"gtk-titlebar-hide-when-maximized": bool = false,
    /// If this is `true`, the titlebar will be hidden when the window is maximized,
    /// and shown when the titlebar is unmaximized. GTK only.
    ///
    /// Available since: 1.1.0
    pub gtk_titlebar_hide_when_maximized: bool,
    // @"gtk-toolbar-style": GtkToolbarStyle = .raised,
    /// Determines the appearance of the top and bottom bars tab bar.
    ///
    /// Valid values are:
    ///
    ///  * `flat` - Top and bottom bars are flat with the terminal window.
    ///  * `raised` - Top and bottom bars cast a shadow on the terminal area.
    ///  * `raised-border` - Similar to `raised` but the shadow is replaced with a
    ///    more subtle border.
    pub gtk_toolbar_style: GtkToolbarStyle,
    // @"gtk-titlebar-style": GtkTitlebarStyle = .native,
    /// The style of the GTK titlebar. Available values are `native` and `tabs`.
    ///
    /// The `native` titlebar style is a traditional titlebar with a title, a few
    /// buttons and window controls. A separate tab bar will show up below the
    /// titlebar if you have multiple tabs open in the window.
    ///
    /// The `tabs` titlebar merges the tab bar and the traditional titlebar.
    /// This frees up vertical space on your screen if you use multiple tabs. One
    /// limitation of the `tabs` titlebar is that you cannot drag the titlebar
    /// by the titles any longer (as they are tab titles now). Other areas of the
    /// `tabs` title bar can be used to drag the window around.
    ///
    /// The default style is `native`.
    pub gtk_titlebar_style: GtkTitlebarStyle,
    // @"gtk-wide-tabs": bool = true,
    /// If `true` (default), then the Rustty GTK tabs will be "wide." Wide tabs
    /// are the new typical Gnome style where tabs fill their available space.
    /// If you set this to `false` then tabs will only take up space they need,
    /// which is the old style.
    pub gtk_wide_tabs: bool,
    // @"gtk-horizontal-tab-scroll": bool = true,
    /// If `true` (default), then two-finger horizontal scrolling on a touchpad
    /// will switch between tabs. Scrolling left goes to the next tab and
    /// scrolling right goes to the previous tab. Set this to `false` to
    /// disable this behavior.
    ///
    /// Available since 1.4.0.
    pub gtk_horizontal_tab_scroll: bool,
    // @"gtk-custom-css": RepeatablePath = .{},
    /// Custom CSS files to be loaded.
    ///
    /// GTK CSS documentation can be found at the following links:
    ///
    ///   * https://docs.gtk.org/gtk4/css-overview.html - An overview of GTK CSS.
    ///   * https://docs.gtk.org/gtk4/css-properties.html - A comprehensive list
    ///     of supported CSS properties.
    ///
    /// Launch Rustty with `env GTK_DEBUG=interactive rustty` to tweak Rustty's
    /// CSS in real time using the GTK Inspector. Errors in your CSS files would
    /// also be reported in the terminal you started Rustty from. See
    /// https://developer.gnome.org/documentation/tools/inspector.html for more
    /// information about the GTK Inspector.
    ///
    /// This configuration can be repeated multiple times to load multiple files.
    /// Prepend a ? character to the file path to suppress errors if the file does
    /// not exist. If you want to include a file that begins with a literal ?
    /// character, surround the file path in double quotes (").
    /// The file size limit for a single stylesheet is 5MiB.
    ///
    /// Available since: 1.1.0
    pub gtk_custom_css: RepeatablePath,

    // @"desktop-notifications": bool = true,
    /// If `true` (default), applications running in the terminal can show desktop
    /// notifications using certain escape sequences such as OSC 9 or OSC 777.
    pub desktop_notifications: bool,
    // @"progress-style": bool = true,
    /// If `true` (default), applications running in the terminal can show
    /// graphical progress bars using the ConEmu OSC 9;4 escape sequence.
    /// If `false`, progress bar sequences are silently ignored.
    pub progress_style: bool,

    // @"bold-color": ?BoldColor = null,
    /// Modifies the color used for bold text in the terminal.
    ///
    /// This can be set to a specific color, using the same format as
    /// `background` or `foreground` (e.g. `#RRGGBB` but other formats
    /// are also supported; see the aforementioned documentation). If a
    /// specific color is set, this color will always be used for the default
    /// bold text color. It will set the rest of the bold colors to `bright`.
    ///
    /// This can also be set to `bright`, which uses the bright color palette
    /// for bold text. For example, if the text is red, then the bold will
    /// use the bright red color. The terminal palette is set with `palette`
    /// but can also be overridden by the terminal application itself using
    /// escape sequences such as OSC 4. (Since Rustty 1.2.0, the previous
    /// configuration `bold-is-bright` is deprecated and replaced by this
    /// usage).
    ///
    /// Available since Rustty 1.2.0.
    pub bold_color: Option<BoldColor>,
    // @"faint-opacity": f64 = 0.5,
    /// The opacity level (opposite of transparency) of the faint text. A value of
    /// 1 is fully opaque and a value of 0 is fully transparent. A value less than 0
    /// or greater than 1 will be clamped to the nearest valid value.
    ///
    /// Available since Rustty 1.2.0.
    pub faint_opacity: f64,

    // term: []const u8 = "xterm-ghostty",
    /// This will be used to set the `TERM` environment variable.
    /// HACK: We set this with an `xterm` prefix because vim uses that to enable key
    /// protocols (specifically this will enable `modifyOtherKeys`), among other
    /// features. An option exists in vim to modify this: `:set
    /// keyprotocol=rustty:kitty`, however a bug in the implementation prevents it
    /// from working properly. https://github.com/vim/vim/pull/13211 fixes this.
    pub term: Vec<u8>,
    // @"enquiry-response": []const u8 = "",
    /// String to send when we receive `ENQ` (`0x05`) from the command that we are
    /// running. Defaults to an empty string if not set.
    pub enquiry_response: Vec<u8>,

    // @"async-backend": AsyncBackend = .auto,
    /// Configures the low-level API to use for async IO, eventing, etc.
    ///
    /// Most users should leave this set to `auto`. This will automatically detect
    /// scenarios where APIs may not be available (for example `io_uring` on
    /// certain hardened kernels) and fall back to a different API. However, if
    /// you want to force a specific backend for any reason, you can set this
    /// here.
    ///
    /// Based on various benchmarks, we haven't found a statistically significant
    /// difference between the backends with regards to memory, CPU, or latency.
    /// The choice of backend is more about compatibility and features.
    ///
    /// Available options:
    ///
    ///   * `auto` - Automatically choose the best backend for the platform
    ///     based on available options.
    ///   * `epoll` - Use the `epoll` API
    ///   * `io_uring` - Use the `io_uring` API
    ///
    /// If the selected backend is not available on the platform, Rustty will
    /// fall back to an automatically chosen backend that is available.
    ///
    /// Changing this value requires a full application restart to take effect.
    ///
    /// This is only supported on Linux, since this is the only platform
    /// where we have multiple options. On macOS, we always use `kqueue`.
    ///
    /// Available since: 1.2.0
    pub async_backend: AsyncBackend,
    // @"auto-update": ?AutoUpdate = null,
    /// Control the auto-update functionality of Rustty. This is only supported
    /// on macOS currently, since Linux builds are distributed via package
    /// managers that are not centrally controlled by Rustty.
    ///
    /// Checking or downloading an update does not send any information to
    /// the project beyond standard network information mandated by the
    /// underlying protocols. To put it another way: Rustty doesn't explicitly
    /// add any tracking to the update process. The update process works by
    /// downloading information about the latest version and comparing it
    /// client-side to the current version.
    ///
    /// Valid values are:
    ///
    ///  * `off` - Disable auto-updates.
    ///  * `check` - Check for updates and notify the user if an update is
    ///    available, but do not automatically download or install the update.
    ///  * `download` - Check for updates, automatically download the update,
    ///    notify the user, but do not automatically install the update.
    ///
    /// If unset, we defer to Sparkle's default behavior, which respects the
    /// preference stored in the standard user defaults (`defaults(1)`).
    ///
    /// Changing this value at runtime works after a small delay.
    pub auto_update: Option<AutoUpdate>,
    // @"auto-update-channel": ?build_config.ReleaseChannel = null,
    /// The release channel to use for auto-updates.
    ///
    /// The default value of this matches the release channel of the currently
    /// running Rustty version. If you download a pre-release version of Rustty
    /// then this will be set to `tip` and you will receive pre-release updates.
    /// If you download a stable version of Rustty then this will be set to
    /// `stable` and you will receive stable updates.
    ///
    /// Valid values are:
    ///
    ///  * `stable` - Stable, tagged releases such as "1.0.0".
    ///  * `tip` - Pre-release versions generated from each commit to the
    ///    main branch. This is the version that was in use during private
    ///    beta testing by thousands of people. It is generally stable but
    ///    will likely have more bugs than the stable channel.
    ///
    /// Changing this configuration requires a full restart of
    /// Rustty to take effect.
    ///
    /// This only works on macOS since only macOS has an auto-update feature.
    pub auto_update_channel: Option<rustty_build_config::ReleaseChannel>,

    // ---- Internal (`_`-prefixed) fields. Not copied, not part of `Key`. ----

    // _arena: ?ArenaAllocator = null,
    //
    // PORT NOTE: the reference threads an `ArenaAllocator` for all parsed
    // allocations. The Rust port hands out owned allocations (each parsed value
    // owns its bytes); the arena's observable role is "allocations available".
    // Modeled as a bool flag for the `Target::arena_available` query.
    /// This is set by the CLI parser for deinit.
    pub _arena: bool,
    // _diagnostics: cli.DiagnosticList = .{},
    /// List of diagnostics that were generated during the loading of
    /// the configuration.
    pub _diagnostics: DiagnosticList,
    // _conditional_state: conditional.State = .{},
    /// The conditional truths for the configuration. This is used to
    /// determine if a conditional configuration matches or not.
    pub _conditional_state: conditional::State,
    // _conditional_set: std.EnumSet(conditional.Key) = .{},
    //
    // PORT NOTE: `std.EnumSet(conditional.Key)` — the set of conditional keys
    // present. Modeled as a small set over `conditional::Key`.
    /// The conditional keys that are used at any point during the configuration
    /// loading. This is used to speed up the conditional evaluation process.
    pub _conditional_set: std::collections::HashSet<conditional::Key>,
    // _replay_steps: std.ArrayListUnmanaged(Replay.Step) = .{},
    /// The steps we can use to reload the configuration after it has been loaded
    /// without reopening the files. This is used in very specific cases such
    /// as loadTheme which has more details on why.
    pub _replay_steps: Vec<ReplayStep>,
    // @"_xdg-terminal-exec": bool = false,
    /// Set to true if Rustty was executed as xdg-terminal-exec on Linux.
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
            // 50_000_000 (50MB)
            scrollback_limit_bytes: ScrollbackLimitBytes::default_value(),
            scrollback_limit_lines: ScrollbackLimitLines::default_value(),
            scrollback_compression: true,
            scrollbar: Scrollbar::System,
            link: RepeatableLink::default(),
            link_url: true,
            link_osc8: true,
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
            drag_handle: DragHandle::Auto,
            resize_overlay: ResizeOverlay::AfterFirst,
            resize_overlay_position: ResizeOverlayPosition::Center,
            resize_overlay_duration: Duration {
                duration: 750 * NS_PER_MS,
            },
            focus_follows_mouse: false,
            clipboard_read: ClipboardAccess::Ask,
            clipboard_write: ClipboardAccess::Allow,
            clipboard_write_limit_bytes: ClipboardWriteLimitBytes::default_value(),
            clipboard_trim_trailing_spaces: true,
            clipboard_paste_protection: true,
            clipboard_paste_bracketed_safe: true,
            title_report: false,
            image_storage_limit: 320 * 1000 * 1000,
            // switch (builtin.os.tag) { .linux => .primary, else => .none }
            copy_on_select: if cfg!(target_os = "linux") {
                CopyOnSelect::Primary
            } else {
                CopyOnSelect::None
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
            // R2 IDENTITY NOTE (Task 16): the reference default is
            // "ghostty-quick-terminal" (see the Zig-source comment above);
            // this is a live, user-visible Wayland layer-surface namespace
            // default, so it is rebranded to "rustty-quick-terminal".
            gtk_quick_terminal_namespace: b"rustty-quick-terminal".to_vec(),
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
            gtk_horizontal_tab_scroll: true,
            gtk_custom_css: RepeatablePath::default(),
            desktop_notifications: true,
            progress_style: true,
            bold_color: None,
            faint_opacity: 0.5,
            term: rustty_identity::TERM.as_bytes().to_vec(),
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
    // fn clone(self: Step, alloc: Allocator) Allocator.Error!Step {
    //     return switch (self) {
    //         .@"-e" => self,
    //         .diagnostic => |v| .{ .diagnostic = try v.clone(alloc) },
    //         .arg => |v| .{ .arg = try alloc.dupeZ(u8, v) },
    //         .expand => |v| .{ .expand = try alloc.dupe(u8, v) },
    //         .conditional_arg => |v| conditional: {
    //             var conds = try alloc.alloc(Conditional, v.conditions.len);
    //             for (v.conditions, 0..) |cond, i| conds[i] = try cond.clone(alloc);
    //             break :conditional .{ .conditional_arg = .{
    //                 .conditions = conds,
    //                 .arg = try alloc.dupe(u8, v.arg),
    //             } };
    //         },
    //     };
    // }
    //
    // PARITY: named `clone_in` (the workspace convention for ported `clone`
    // decls) — Config relies on it to clone `_replay_steps`.
    pub fn clone_in(&self) -> ReplayStep {
        match self {
            // .@"-e" => self,
            ReplayStep::DashE => ReplayStep::DashE,
            // .diagnostic => |v| .{ .diagnostic = try v.clone(alloc) },
            ReplayStep::Diagnostic(v) => ReplayStep::Diagnostic(v.clone_in()),
            // .arg => |v| .{ .arg = try alloc.dupeZ(u8, v) },
            ReplayStep::Arg(v) => ReplayStep::Arg(v.clone()),
            // .expand => |v| .{ .expand = try alloc.dupe(u8, v) },
            ReplayStep::Expand(v) => ReplayStep::Expand(v.clone()),
            // .conditional_arg => |v| { var conds = ...; ... }
            ReplayStep::ConditionalArg { conditions, arg } => {
                // var conds = try alloc.alloc(Conditional, v.conditions.len);
                // for (v.conditions, 0..) |cond, i| conds[i] = try cond.clone(alloc);
                let conds: Vec<Conditional> = conditions.iter().map(|cond| cond.clone()).collect();
                ReplayStep::ConditionalArg {
                    conditions: conds,
                    // .arg = try alloc.dupe(u8, v.arg),
                    arg: arg.clone(),
                }
            }
        }
    }
}

// ===========================================================================
// PORT NOTE: support — the std/OS glue + error/iterator types the machinery above
// uses. These add nothing the reference doesn't already do (R1): they are the
// minimal Rust counterparts of the inline std calls in the reference functions.
// ===========================================================================

/// `std.fs.max_path_bytes` — the platform max path length (4096 on Linux/macOS).
const MAX_PATH_BYTES: usize = 4096;

/// `std.time.ns_per_s`.
const NS_PER_S: u64 = 1_000_000_000;

/// The error set of `loadFile`/`loadFsFile`/`loadReader`/`loadRecursiveFiles`.
/// The reference threads `std.fs`/reader errors plus the parse error set; the
/// `loadOptionalFile` arm specifically distinguishes `FileNotFound`.
#[derive(Debug)]
pub enum LoadError {
    /// `error.FileNotFound` — used by `loadOptionalFile` to return `.not_found`.
    FileNotFound,
    /// Any other IO error during load.
    Io(std::io::Error),
    /// A parse error propagated from `loadIter`.
    Parse(ParseError),
}

impl LoadError {
    /// Map a `file_load.open` error (other than `NotAFile`, handled inline) into
    /// a `LoadError`, preserving the `FileNotFound` distinction.
    fn from_open(err: crate::file_load::OpenFileError) -> LoadError {
        match err {
            // error.FileNotFound => loadOptionalFile returns .not_found
            crate::file_load::OpenFileError::FileNotFound => LoadError::FileNotFound,
            // `NotAFile` is handled inline in `loadFile`; the remaining variants
            // (`FileIsEmpty`, `FileOpenFailed`) are surfaced as IO errors.
            other => LoadError::Io(std::io::Error::other(format!("{other:?}"))),
        }
    }
}

impl From<std::io::Error> for LoadError {
    fn from(e: std::io::Error) -> LoadError {
        if e.kind() == std::io::ErrorKind::NotFound {
            LoadError::FileNotFound
        } else {
            LoadError::Io(e)
        }
    }
}

impl From<ParseError> for LoadError {
    fn from(e: ParseError) -> LoadError {
        LoadError::Parse(e)
    }
}

/// `std.log.info` / `std.log.warn` scoped `.config`. The reference routes these
/// through `main_ghostty.zig`'s `logFn`, whose stderr branch is gated on
/// `global.logging().stderr` (`if (!global.logging().stderr) break :stderr;`) —
/// set false by `global.zig` `init` when a CLI action is detected, so action
/// output stays clean. The port honors the same gate via the
/// `rustty_global::logging()` process-global; formatting is split from the
/// write so the gate is unit-testable without capturing stderr.
fn format_log(kind: &str, msg: &str, logging: rustty_global::Logging) -> Option<String> {
    if !logging.stderr {
        return None;
    }
    // `level_txt ++ "(" ++ @tagName(scope) ++ "): "` — logFn's prefix for the
    // `.config` scope; `warning` is `std.log.Level.warn.asText()`.
    Some(format!("{kind}(config): {msg}"))
}

fn log_info(msg: String) {
    if let Some(line) = format_log("info", &msg, rustty_global::logging()) {
        eprintln!("{line}");
    }
}

fn log_warn(msg: String) {
    if let Some(line) = format_log("warning", &msg, rustty_global::logging()) {
        eprintln!("{line}");
    }
}

/// `std.fs.path.dirname` — the directory portion of `path`, or `None` if there
/// is no separator (matching `std.fs.path.dirname` returning null).
fn dirname(path: &[u8]) -> Option<&[u8]> {
    let sep = std::path::MAIN_SEPARATOR as u8;
    let idx = path.iter().rposition(|&b| b == sep)?;
    if idx == 0 {
        // Root: dirname("/foo") == "/".
        Some(&path[..1])
    } else {
        Some(&path[..idx])
    }
}

/// `@tagName(stat.kind)` for a non-file stat used in `config-file` diagnostics.
fn file_kind_name(meta: &std::fs::Metadata) -> &'static str {
    let ft = meta.file_type();
    if ft.is_dir() {
        "directory"
    } else if ft.is_symlink() {
        "sym_link"
    } else {
        "unknown"
    }
}

/// Build an `&OsStr` from raw path bytes without a lossy round-trip.
#[cfg(unix)]
fn os_str_from_bytes(bytes: &[u8]) -> &std::ffi::OsStr {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::OsStr::from_bytes(bytes)
}

#[cfg(not(unix))]
fn os_str_from_bytes(bytes: &[u8]) -> &std::ffi::OsStr {
    std::ffi::OsStr::new(std::str::from_utf8(bytes).unwrap_or(""))
}

/// `std.fs.Dir.realpath` result -> raw path bytes (no lossy round-trip).
#[cfg(unix)]
fn path_to_bytes(p: &std::path::Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    p.as_os_str().as_bytes().to_vec()
}

#[cfg(not(unix))]
fn path_to_bytes(p: &std::path::Path) -> Vec<u8> {
    p.to_string_lossy().into_owned().into_bytes()
}

/// `fn writeConfigTemplate(path: []const u8) !void` — create the template config
/// file at `path`. Reproduces the reference: makes the parent dir, creates the
/// file, and writes the embedded template with the `path` substituted.
// fn writeConfigTemplate(path: []const u8) !void {
fn write_config_template(path: &[u8]) -> std::io::Result<()> {
    // log.info("creating template config file: path={s}", .{path});
    log_info(format!(
        "creating template config file: path={}",
        String::from_utf8_lossy(path)
    ));
    // if (std.fs.path.dirname(path)) |dir_path| {
    //     try std.fs.cwd().makePath(dir_path);
    // }
    if let Some(dir_path) = dirname(path) {
        std::fs::create_dir_all(os_str_from_bytes(dir_path))?;
    }
    // const file = try std.fs.createFileAbsolute(path, .{});
    // defer file.close();
    let mut file = std::fs::File::create(os_str_from_bytes(path))?;
    // var buf: [4096]u8 = undefined;
    // var file_writer = file.writer(&buf);
    // const writer = &file_writer.interface;
    // try writer.print(
    //     @embedFile("./config-template"),
    //     .{ .path = path },
    // );
    use std::io::Write;
    file.write_all(&render_config_template(path))?;
    Ok(())
}

/// PORTS: `@embedFile("./config-template")` rendered with `.{ .path = path }`.
/// The template is a format string with a single `{[path]s}` substitution.
fn render_config_template(path: &[u8]) -> Vec<u8> {
    // PORT NOTE: the reference embeds `src/config/config-template`; its sole
    // substitution is the config-file `path`. The template body is reproduced
    // here verbatim so the generated file is byte-faithful.
    let template = include_str!("config-template");
    template
        .replace("{[path]s}", &String::from_utf8_lossy(path))
        .into_bytes()
}

/// This runs a heuristic to determine if we are likely running
/// Rustty in a CLI environment. We need this to change some behaviors.
/// We should keep the set of behaviors that depend on this as small
/// as possible because magic sucks, but each place is well documented.
// fn probableCliEnvironment() bool {
fn probable_cli_environment() -> bool {
    // switch (builtin.os.tag) {
    //     // Windows has its own problems, just ignore it for now since
    //     // its not a real supported target and GTK via WSL2 assuming
    //     // single instance is probably fine.
    //     .windows => return false,
    //     // On macOS, we don't want to detect `open` calls as CLI envs.
    //     // Our desktop detection on macOS is very accurate due to how
    //     // processes are launched on macOS, so if we detect we're launched
    //     // from the app bundle then we're not in a CLI environment.
    //     .macos => if (internal_os.launchedFromDesktop()) return false,
    //     else => {},
    // }
    #[cfg(target_os = "windows")]
    {
        return false;
    }
    #[cfg(target_os = "macos")]
    {
        if rustty_os::desktop::launchedFromDesktop() {
            return false;
        }
    }

    // If we have TERM_PROGRAM set to a non-empty value, we assume a graphical
    // terminal environment.
    //
    // TODO: This is not available on WASI without libc due to the memory
    // allocation requirement. This restricts this function to said platforms.
    // To be fair, the legacy getenv path had more restrictive issues (no WASI
    // period, or Windows for that matter).
    //
    // if (global.environ().containsUnemptyConstant("TERM_PROGRAM")) return true;
    //
    // PORT NOTE (`e8525c0fd`): upstream replaced `std.posix.getenv("TERM_PROGRAM")`
    // with the captured global environ, and this read MUST follow it there rather
    // than to `std::env`. Two reasons, both real: the `.c` arm of `global.init`
    // installs a caller-supplied block (`rustty_global::environ::set_environ`) and
    // libghostty embedders rely on the heuristic seeing THAT block rather than the
    // host process's; and `std::env::var` is a lossy `String` read, which the
    // byte-slice rule forbids. The WASI/libc caveat above is Zig-specific — Rust's
    // `OsString` path has no such restriction — but it is recorded because it is
    // why upstream's comment exists at all.
    if rustty_global::environ::environ().contains_unempty_constant(b"TERM_PROGRAM") {
        return true;
    }

    // CLI arguments makes things probable
    // if (global.args().vector.len > 1) return true;
    if std::env::args_os().len() > 1 {
        return true;
    }

    // Unlikely CLI environment
    // return false;
    false
}

/// Expand tilde paths to absolute paths to the user's home directory.
/// If expansion fails, an error is logged and the original path is returned.
// fn expandHome(path: []const u8, buf: []u8) []const u8 {
//
// PORT NOTE (`37174c73b`): file-scope helper in the reference (Config.zig, right
// before `loadTheme`); shared by `WorkingDirectory.finalize` and `Theme.finalize`
// (both in `value_types.rs`), hence `pub(crate)`.
pub(crate) fn expand_home<'a>(path: &'a [u8], buf: &'a mut [u8]) -> &'a [u8] {
    // if (!std.mem.startsWith(u8, path, "~/"))
    //     return path;
    if !path.starts_with(b"~/") {
        return path;
    }

    // WS14 T6 (wasm32 arm): a browser has no home directory to expand `~/`
    // against, so wasm returns the path unchanged, exactly as the error arms
    // below do. The reference never analyses this on wasm (Zig analyses
    // `config.zig.Wasm` lazily, so it never forces `internal_os.expandHome`);
    // Rust compiles whole crates, so the arm has to exist.
    #[cfg(target_family = "wasm")]
    {
        path
    }
    #[cfg(not(target_family = "wasm"))]
    {
        // var environ_map = global.environMap() catch |err| {
        //     log.warn("failed to get environment map for path \"{s}\": {}", .{ path, err });
        //     return path;
        // };
        // defer environ_map.deinit();
        //
        // PORT NOTE: `rustty_global::environ_map()` is infallible (a snapshot of
        // the captured environ), so the reference's `catch` warn-arm has no Rust
        // analogue.
        let environ_map = rustty_global::environ_map();

        // return internal_os.expandHome(
        //     global.io(),
        //     &environ_map,
        //     path,
        //     buf,
        // ) catch |err| {
        //     log.warn("failed to expand home directory in path \"{s}\": {}", .{ path, err });
        //     return path;
        // };
        match rustty_os::homedir::expandHome(&environ_map, path, buf) {
            Ok(expanded) => expanded,
            Err(err) => {
                log_warn(format!(
                    "failed to expand home directory in path \"{}\": {:?}",
                    String::from_utf8_lossy(path),
                    err
                ));
                path
            }
        }
    }
}

/// This is used to "replay" the configuration. See loadTheme for details.
///
/// PORTS: `const Replay = struct { ... }` — the `Step` element type lives above
/// as `ReplayStep`; this is the `Iterator` + `iterator` constructor.
// const Replay = struct { const Step = ...; const Iterator = struct {...}; fn iterator(...) {...} };
//
// PORT NOTE (aliasing): the reference `Replay.Iterator` holds `config: *Config`
// and mutates it inside `next` while `cli.args.parse` drives it with the SAME
// `*Config`. In Rust the iterator cannot capture `&mut Config` (it would alias
// the `&mut Config` `parse_self` holds), so `ReplayIterator` is a
// `SelfDrivenIterator<Config>`: its `next_self` receives `&mut Config` as a
// parameter, exactly recreating the reference's interleaving without aliasing.
pub struct ReplayIterator {
    // slice: []const Replay.Step,
    slice: Vec<ReplayStep>,
    // idx: usize = 0,
    idx: usize,
}

impl ReplayIterator {
    /// Construct a Replay iterator from a slice of replay elements.
    /// This can be used with args.parse and handles intermediate
    /// steps such as expanding relative paths.
    // fn iterator(slice: []const Replay.Step, dst: *Config) Iterator {
    //     return .{ .slice = slice, .config = dst };
    // }
    //
    // PORT NOTE: `dst` (the `*Config`) is no longer captured; it is lent to
    // `next_self` per the aliasing note above.
    pub fn new(slice: Vec<ReplayStep>) -> ReplayIterator {
        ReplayIterator { slice, idx: 0 }
    }
}

impl rustty_args::args::SelfDrivenIterator<Config> for ReplayIterator {
    // pub fn next(self: *Self) ?[]const u8 {
    fn next_self(&mut self, config: &mut Config) -> Option<Vec<u8>> {
        // while (true) {
        loop {
            // if (self.idx >= self.slice.len) return null;
            if self.idx >= self.slice.len() {
                return None;
            }
            // defer self.idx += 1;
            let cur = self.idx;
            self.idx += 1;
            // switch (self.slice[self.idx]) {
            match self.slice[cur].clone() {
                // .expand => |base| self.config.expandPaths(base) catch |err| {
                //     // This shouldn't happen because to reach this step
                //     // means that it succeeded before. Its possible since
                //     // expanding paths is a side effect process that the
                //     // world state changed and we can't expand anymore.
                //     // In that really unfortunate case, we log a warning.
                //     log.warn("error expanding paths err={}", .{err});
                // },
                ReplayStep::Expand(base) => {
                    if let Err(err) = config.expand_paths(&base) {
                        log_warn(format!("error expanding paths err={err:?}"));
                    }
                }

                // .diagnostic => |diag| diag: {
                ReplayStep::Diagnostic(diag) => {
                    // Best effort to clone and append the diagnostic.
                    // If it fails we log a warning and continue.
                    // const arena_alloc = self.config._arena.?.allocator();
                    // const cloned = diag.clone(arena_alloc) catch |err| {
                    //     log.warn("error cloning diagnostic err={}", .{err});
                    //     break :diag;
                    // };
                    let cloned = diag.clone_in();
                    // self.config._diagnostics.append(arena_alloc, cloned) catch |err| {
                    //     log.warn("error appending diagnostic err={}", .{err});
                    //     break :diag;
                    // };
                    config._diagnostics.append(cloned);
                }

                // .conditional_arg => |v| conditional: {
                ReplayStep::ConditionalArg { conditions, arg } => {
                    // All conditions must match.
                    // for (v.conditions) |cond| {
                    //     if (!self.config._conditional_state.match(cond)) {
                    //         break :conditional;
                    //     }
                    // }
                    // return v.arg;
                    let mut all_match = true;
                    for cond in &conditions {
                        if !config._conditional_state.is_match(cond) {
                            all_match = false;
                            break;
                        }
                    }
                    if all_match {
                        return Some(arg);
                    }
                }

                // .arg => |arg| return arg,
                ReplayStep::Arg(arg) => return Some(arg),
                // .@"-e" => return "-e",
                ReplayStep::DashE => return Some(b"-e".to_vec()),
            }
        }
    }

    // pub fn location(self: *const Self, _: Allocator) error{}!?diags.Location {
    //     return .{ .cli = self.index };
    // }
    //
    // PORT NOTE: the reference's `Replay.Iterator` has no `location` decl; it
    // relies on `cli.Location.fromIter` returning `.none` for iterators without
    // one. We return `None`, which `parse_self` maps to `Location::None`.
    fn location_self(&self) -> Option<rustty_args::diagnostics::Location> {
        None
    }
}

/// PORTS: `pub const ChangeIterator` (Config.zig:5056) — iterates `Key`s whose
/// value differs between `old` and `new`.
///
/// This yields a key for every changed field between old and new.
// pub const ChangeIterator = struct { old, new, i = 0, pub fn next(...) ?Key };
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
        // const fields = comptime std.meta.fields(Key);
        // while (self.i < fields.len) {
        //     switch (self.i) {
        //         inline 0...(fields.len - 1) => |i| {
        //             const field = fields[i];
        //             const key = @field(Key, field.name);
        //             self.i += 1;
        //             if (self.old.changed(self.new, key)) return key;
        //         },
        //         else => unreachable,
        //     }
        // }
        // return null;
        //
        // PORT NOTE: Zig's `std.meta.fields(Key)` is the comptime `Key`-variant
        // sequence. Rust has no enum reflection, so we iterate the `CONFIG_FIELDS`
        // table's keys, which are the `Key` variants in the reference's struct
        // declaration order (the same order `std.meta.fields(Key)` yields, since
        // `Key` is derived from the Config fields in order). `self.i` indexes that
        // sequence exactly as the reference's `self.i` indexes `fields`.
        while self.i < CONFIG_FIELDS.len() {
            let (_name, key) = CONFIG_FIELDS[self.i];
            self.i += 1;
            if self.old.changed(self.new, key) {
                return Some(key);
            }
        }

        None
    }
}

// ===========================================================================
// Config pass method signatures (the four comptime passes + lifecycle).
// ===========================================================================

impl Config {
    // pub fn load(alloc_gpa: Allocator) !Config {
    pub fn load() -> Result<Config, LoadError> {
        // var result = try default(alloc_gpa);
        let mut result = Config::default_config();
        // errdefer result.deinit();

        // If we have a configuration file in our home directory, parse that first.
        // try result.loadDefaultFiles(alloc_gpa);
        result.load_default_files()?;

        // Parse the config from the CLI args.
        // try result.loadCliArgs(alloc_gpa);
        result.load_cli_args()?;

        // Parse the config files that were added from our file and CLI args.
        // try result.loadRecursiveFiles(alloc_gpa);
        result.load_recursive_files()?;
        // try result.finalize();
        result.finalize()?;

        // return result;
        Ok(result)
    }

    /// PORTS: `pub fn default(alloc_gpa) Allocator.Error!Config`.
    ///
    /// The data half (the `Config` struct defaults) lives in `Default::default`;
    /// `default_config` performs the post-default initialization the reference
    /// does after `var result: Config = .{ ._arena = .init(alloc_gpa) }`.
    // pub fn default(alloc_gpa: Allocator) Allocator.Error!Config {
    // PARITY: the reference is `var result: Config = .{ ._arena = .init(alloc) }`
    // — a deliberate default-then-set-one-field, so the lint does not apply.
    #[allow(clippy::field_reassign_with_default)]
    pub fn default_config() -> Config {
        // Build up our basic config
        // var result: Config = .{
        //     ._arena = .init(alloc_gpa),
        // };
        // errdefer result.deinit();
        // const alloc = result._arena.?.allocator();
        let mut result: Config = Config::default();
        // PARITY: the reference creates the arena here; the Rust port owns data
        // per-field, so we record that the arena "exists" via `_arena = true`
        // (the same flag the args parser observes).
        result._arena = true;

        // Add our default keybindings
        // try result.keybind.init(alloc);
        result.keybind.init();

        // Add our default command palette entries
        // try result.@"command-palette-entry".init(alloc);
        result.command_palette_entry.init();

        // Add our default link for URL detection
        // try result.link.links.append(alloc, .{
        //     .regex = url.regex,
        //     .action = .{ .open = {} },
        //     .highlight = .{ .hover_mods = inputpkg.ctrlOrSuper(.{}) },
        // });
        result.link.links.push(rustty_input::link::Link {
            regex: crate::url::regex().into_bytes(),
            action: rustty_input::link::Action::Open,
            highlight: rustty_input::link::Highlight::HoverMods(rustty_input::ctrl_or_super(
                rustty_vt::input::key_mods::Mods::default(),
            )),
        });

        // return result;
        result
    }

    /// PORTS: `pub fn deinit(self: *Config) void`.
    // pub fn deinit(self: *Config) void {
    //     if (self._arena) |arena| arena.deinit();
    //     self.* = undefined;
    // }
    //
    // PORT NOTE: the reference frees the arena (which owns every value's bytes).
    // In Rust each value type owns its own bytes and is dropped automatically; the
    // arena flag is cleared to match the reference's `_arena` teardown.
    pub fn deinit(&mut self) {
        self._arena = false;
    }

    /// Load configuration from an iterator that yields values that look like
    /// command-line arguments, i.e. `--key=value`.
    // pub fn loadIter(
    //     self: *Config,
    //     alloc: Allocator,
    //     iter: anytype,
    // ) !void {
    //     try cli.args.parse(Config, alloc, self, iter);
    // }
    //
    // PORT NOTE: routed through `parse_self` (the aliasing-safe `parse` variant)
    // so both replay and non-replay iterators share one path. See the
    // `SelfDrivenIterator` note in rustty-args.
    pub fn load_iter<I: rustty_args::args::SelfDrivenIterator<Config>>(
        &mut self,
        iter: &mut I,
    ) -> Result<(), ParseError> {
        rustty_args::args::parse_self(self, iter)
    }

    /// Load configuration from the target config file at `path`.
    ///
    /// `path` must be resolved and absolute.
    // pub fn loadFile(self: *Config, alloc: Allocator, path: []const u8) !void {
    pub fn load_file(&mut self, path: &[u8]) -> Result<(), LoadError> {
        // assert(std.fs.path.isAbsolute(path));
        assert!(std::path::Path::new(os_str_from_bytes(path)).is_absolute());
        // var file = file_load.open(path) catch |err| switch (err) {
        let file = match crate::file_load::open(path) {
            Ok(f) => f,
            // error.NotAFile => {
            //     log.warn(
            //         "config-file {s}: not reading because it is not a file",
            //         .{path},
            //     );
            //     return;
            // },
            Err(crate::file_load::OpenFileError::NotAFile) => {
                log_warn(format!(
                    "config-file {}: not reading because it is not a file",
                    String::from_utf8_lossy(path)
                ));
                return Ok(());
            }
            // else => return err,
            Err(err) => return Err(LoadError::from_open(err)),
        };
        // defer file.close();

        // try self.loadFsFile(alloc, &file, path);
        self.load_fs_file(file, path)
    }

    /// Load config from the given File.
    // fn loadFsFile(self: *Config, alloc: Allocator, file: *std.fs.File, path: []const u8) !void {
    fn load_fs_file(&mut self, mut file: std::fs::File, path: &[u8]) -> Result<(), LoadError> {
        // std.log.info("reading configuration file path={s}", .{path});
        log_info(format!(
            "reading configuration file path={}",
            String::from_utf8_lossy(path)
        ));
        // var buf: [2048]u8 = undefined;
        // var file_reader = file.reader(&buf);
        // const reader = &file_reader.interface;
        //
        // PORT NOTE: `std.Io.Reader` over the file is modeled by reading the file
        // contents into a byte buffer that `LineIterator` (a byte-cursor reader)
        // consumes. This is byte-faithful for the configuration files the loader
        // reads (small, fully-buffered).
        use std::io::Read;
        let mut contents: Vec<u8> = Vec::new();
        file.read_to_end(&mut contents)?;
        // try self.loadReader(alloc, reader, path);
        self.load_reader(&contents, path)
    }

    /// Load config from the given Reader.
    // fn loadReader(self: *Config, alloc: Allocator, reader: *std.Io.Reader, path: []const u8) !void {
    fn load_reader(&mut self, reader: &[u8], path: &[u8]) -> Result<(), LoadError> {
        // bom: {
        //     // If the file starts with a UTF-8 byte order mark, skip it.
        //     // https://en.wikipedia.org/wiki/Byte_order_mark#UTF-8
        //     const bom: []const u8 = &.{ 0xef, 0xbb, 0xbf };
        //     const str = reader.peek(bom.len) catch break :bom;
        //     if (std.mem.eql(u8, str, bom)) {
        //         log.info("skipping UTF-8 byte order mark", .{});
        //         reader.toss(bom.len);
        //     }
        // }
        let bom: &[u8] = &[0xef, 0xbb, 0xbf];
        let reader: &[u8] = 'bom: {
            if reader.len() >= bom.len() && &reader[..bom.len()] == bom {
                log_info("skipping UTF-8 byte order mark".to_string());
                break 'bom &reader[bom.len()..];
            }
            reader
        };
        // var iter: cli.args.LineIterator = .{ .r = reader, .filepath = path };
        let mut iter = rustty_args::args::LineIterator::with_filepath(reader, path);
        // try self.loadIter(alloc, &iter);
        self.load_iter(&mut iter)?;
        // try self.expandPaths(std.fs.path.dirname(path).?);
        self.expand_paths(dirname(path).unwrap())?;
        Ok(())
    }

    /// Load optional configuration file from `path`. All errors are ignored.
    ///
    /// Returns the action that was taken.
    // pub fn loadOptionalFile(
    //     self: *Config,
    //     alloc: Allocator,
    //     path: []const u8,
    // ) OptionalFileAction {
    pub fn load_optional_file(&mut self, path: &[u8]) -> OptionalFileAction {
        // if (self.loadFile(alloc, path)) {
        //     return .loaded;
        // } else |err| switch (err) {
        match self.load_file(path) {
            // return .loaded;
            Ok(()) => OptionalFileAction::Loaded,
            // error.FileNotFound => return .not_found,
            Err(LoadError::FileNotFound) => OptionalFileAction::NotFound,
            // else => {
            //     std.log.warn(
            //         "error reading optional config file, not loading err={} path={s}",
            //         .{ err, path },
            //     );
            //     return .@"error";
            // },
            Err(err) => {
                log_warn(format!(
                    "error reading optional config file, not loading err={err:?} path={}",
                    String::from_utf8_lossy(path)
                ));
                OptionalFileAction::Error
            }
        }
    }

    /// Load configurations from the default configuration files. The default
    /// configuration file is at `$XDG_CONFIG_HOME/rustty/config.rustty`.
    ///
    /// On macOS, `$HOME/Library/Application Support/$CFBundleIdentifier/`
    /// is also loaded.
    ///
    /// The legacy `config` file (without extension) is first loaded,
    /// then `config.rustty`.
    // pub fn loadDefaultFiles(self: *Config, alloc: Allocator) !void {
    pub fn load_default_files(&mut self) -> Result<(), LoadError> {
        // Load XDG first
        // const legacy_xdg_path = try file_load.legacyDefaultXdgPath(alloc);
        // defer alloc.free(legacy_xdg_path);
        // (W94-B02-01: the XDG helpers are byte-typed now — no `into_bytes`.)
        let legacy_xdg_path = crate::file_load::legacy_default_xdg_path().unwrap_or_default();
        // const xdg_path = try file_load.defaultXdgPath(alloc);
        // defer alloc.free(xdg_path);
        let xdg_path = crate::file_load::default_xdg_path().unwrap_or_default();
        // const xdg_loaded: bool = xdg_loaded: {
        let xdg_loaded: bool = {
            // const legacy_xdg_action = self.loadOptionalFile(alloc, legacy_xdg_path);
            let legacy_xdg_action = self.load_optional_file(&legacy_xdg_path);
            // const xdg_action = self.loadOptionalFile(alloc, xdg_path);
            let xdg_action = self.load_optional_file(&xdg_path);
            // if (xdg_action != .not_found and legacy_xdg_action != .not_found) {
            if xdg_action != OptionalFileAction::NotFound
                && legacy_xdg_action != OptionalFileAction::NotFound
            {
                // log.warn("both config files `{s}` and `{s}` exist.", .{ legacy_xdg_path, xdg_path });
                log_warn(format!(
                    "both config files `{}` and `{}` exist.",
                    String::from_utf8_lossy(&legacy_xdg_path),
                    String::from_utf8_lossy(&xdg_path)
                ));
                // log.warn("loading them both in that order", .{});
                log_warn("loading them both in that order".to_string());
                // break :xdg_loaded true;
                true
            } else {
                // break :xdg_loaded xdg_action != .not_found or
                //     legacy_xdg_action != .not_found;
                xdg_action != OptionalFileAction::NotFound
                    || legacy_xdg_action != OptionalFileAction::NotFound
            }
        };

        // On macOS load the app support directory as well
        // if (comptime builtin.os.tag == .macos) {
        #[cfg(target_os = "macos")]
        {
            // const legacy_app_support_path = try file_load.legacyDefaultAppSupportPath(alloc);
            // defer alloc.free(legacy_app_support_path);
            let legacy_app_support_path = crate::file_load::legacy_default_app_support_path()
                .map(String::into_bytes)
                .unwrap_or_default();
            // const app_support_path = try file_load.preferredAppSupportPath(alloc);
            // defer alloc.free(app_support_path);
            let app_support_path = crate::file_load::preferred_app_support_path()
                .map(String::into_bytes)
                .unwrap_or_default();
            // const app_support_loaded: bool = loaded: {
            let app_support_loaded: bool = {
                // const legacy_app_support_action = self.loadOptionalFile(
                //     alloc,
                //     legacy_app_support_path,
                // );
                let legacy_app_support_action = self.load_optional_file(&legacy_app_support_path);

                // The app support path and legacy may be the same, since we
                // use the `preferred` call above. If its the same, avoid
                // a double-load.
                // const app_support_action: OptionalFileAction = if (!std.mem.eql(
                //     u8,
                //     legacy_app_support_path,
                //     app_support_path,
                // )) self.loadOptionalFile(
                //     alloc,
                //     app_support_path,
                // ) else .not_found;
                let app_support_action: OptionalFileAction =
                    if legacy_app_support_path != app_support_path {
                        self.load_optional_file(&app_support_path)
                    } else {
                        OptionalFileAction::NotFound
                    };

                // if (app_support_action != .not_found and legacy_app_support_action != .not_found) {
                if app_support_action != OptionalFileAction::NotFound
                    && legacy_app_support_action != OptionalFileAction::NotFound
                {
                    // log.warn(
                    //     "both config files `{s}` and `{s}` exist.",
                    //     .{ legacy_app_support_path, app_support_path },
                    // );
                    log_warn(format!(
                        "both config files `{}` and `{}` exist.",
                        String::from_utf8_lossy(&legacy_app_support_path),
                        String::from_utf8_lossy(&app_support_path)
                    ));
                    // log.warn("loading them both in that order", .{});
                    log_warn("loading them both in that order".to_string());
                    // break :loaded true;
                    true
                } else {
                    // break :loaded app_support_action != .not_found or
                    //     legacy_app_support_action != .not_found;
                    app_support_action != OptionalFileAction::NotFound
                        || legacy_app_support_action != OptionalFileAction::NotFound
                }
            };

            // If both files are not found, then we create a template file.
            // For macOS, we only create the template file in the app support
            // if (!app_support_loaded and !xdg_loaded) {
            if !app_support_loaded && !xdg_loaded {
                // writeConfigTemplate(app_support_path) catch |err| {
                //     log.warn("error creating template config file err={}", .{err});
                // };
                if let Err(err) = write_config_template(&app_support_path) {
                    log_warn(format!("error creating template config file err={err:?}"));
                }
            }
        }
        // } else {
        #[cfg(not(target_os = "macos"))]
        {
            // if (!xdg_loaded) {
            if !xdg_loaded {
                // writeConfigTemplate(xdg_path) catch |err| {
                //     log.warn("error creating template config file err={}", .{err});
                // };
                if let Err(err) = write_config_template(&xdg_path) {
                    log_warn(format!("error creating template config file err={err:?}"));
                }
            }
        }

        Ok(())
    }

    /// PORTS: `pub fn parseManuallyHook(self, alloc, arg, iter) !bool` — the
    /// `@hasDecl(T, "parseManuallyHook")` entry point used by the args parser.
    ///
    /// Callback for src/cli/args.zig to allow us to handle special cases
    /// like `--help` or `-e`. Returns "false" if the CLI parsing should halt.
    // pub fn parseManuallyHook(
    //     self: *Config,
    //     alloc: Allocator,
    //     arg: []const u8,
    //     iter: anytype,
    // ) !bool {
    //
    // PORT NOTE: the reference consumes the rest of `iter` here (for `-e`).
    // `parse_self` exposes the iterator only via `next_self`, so the hook cannot
    // drain it inline. Instead the hook records the `-e` marker + the parse
    // position and the `-e` command is assembled by the parse driver. To preserve
    // the reference's per-arg behavior WITHOUT threading the iterator, the hook is
    // kept arg-at-a-time: it appends each replay step and, for `-e`, the parse
    // loop's subsequent args become the command. See `parse_manually_hook` Target
    // impl which records args; the `-e` command finalization runs in `finalize`-
    // adjacent assembly. For the scope of this batch the hook ports the non-`-e`
    // replay-recording path faithfully and the `-e` marker path; the full
    // iterator-draining `-e` command assembly is exercised by loadCliArgs/replay.
    pub fn parse_manually_hook(
        &mut self,
        arg: &[u8],
    ) -> Result<rustty_args::args::HookOutcome, ParseError> {
        use rustty_args::args::HookOutcome;
        // if (std.mem.eql(u8, arg, "-e")) {
        if arg == b"-e" {
            // Add the special -e marker. This prevents:
            // (1) config-file from adding args to the end (see #2908)
            // (2) dark/light theme from making this conditional
            // try self._replay_steps.append(alloc, .@"-e");
            self._replay_steps.push(ReplayStep::DashE);

            // Build up the command. We don't clean this up because we take
            // ownership in our allocator.
            // var command: std.ArrayList([:0]const u8) = .empty;
            // errdefer command.deinit(alloc);
            //
            // while (iter.next()) |param| {
            //     const copy = try alloc.dupeZ(u8, param);
            //     try self._replay_steps.append(alloc, .{ .arg = copy });
            //     try command.append(alloc, copy);
            // }
            //
            // PORT NOTE (iterator threading): the reference drains `iter` HERE to
            // build the `-e` command. The hook holds `&mut self`, and the driver's
            // `next_self` also needs `&mut self`, so the drain cannot run inline
            // without aliasing UB. Instead the hook records the `-e` MARKER and
            // returns `HookOutcome::ConsumeRest`; the driver (`parse`/`parse_self`)
            // — which owns `self` and `iter` non-overlappingly — runs the verbatim
            // `while (iter.next())` drain (recording each arg via
            // `record_replay_arg` and collecting the command), then calls
            // `apply_e_command` below, which ports the rest of this branch
            // (lines 4739-4763) line-for-line: the empty-command diagnostic and
            // the `initial-command = .{ .direct = ... }` + implied configs.
            return Ok(HookOutcome::ConsumeRest);
        }

        // Keep track of our input args for replay
        // try self._replay_steps.append(
        //     alloc,
        //     .{ .arg = try alloc.dupeZ(u8, arg) },
        // );
        self._replay_steps.push(ReplayStep::Arg(arg.to_vec()));

        // If we didn't find a special case, continue parsing normally
        // return true;
        Ok(HookOutcome::Continue)
    }

    /// PORTS the tail of `parseManuallyHook`'s `-e` branch (Config.zig:4739-4763),
    /// run by the driver after it has drained `iter` into `command` (each arg
    /// already appended to `_replay_steps` via `record_replay_arg`).
    pub fn apply_e_command(
        &mut self,
        command: Vec<Vec<u8>>,
        location: rustty_args::diagnostics::Location,
    ) -> bool {
        // if (command.items.len == 0) {
        if command.is_empty() {
            // try self._diagnostics.append(alloc, .{
            //     .location = try cli.Location.fromIter(iter, alloc),
            //     .message = try std.fmt.allocPrintSentinel(
            //         alloc,
            //         "missing command after {s}",
            //         .{arg},
            //         0,
            //     ),
            // });
            //
            // `arg` is always "-e" on this path (the `-e` branch).
            self._diagnostics
                .append(rustty_args::diagnostics::Diagnostic {
                    key: Vec::new(),
                    location,
                    message: b"missing command after -e".to_vec(),
                });

            // return false;
            return false;
        }

        // See "command" docs for the implied configurations and why.
        // self.@"initial-command" = .{ .direct = command.items };
        self.initial_command = Some(Command::Direct(command));
        // self.@"gtk-single-instance" = .false;
        self.gtk_single_instance = GtkSingleInstance::False;
        // self.@"quit-after-last-window-closed" = true;
        self.quit_after_last_window_closed = true;
        // self.@"quit-after-last-window-closed-delay" = null;
        self.quit_after_last_window_closed_delay = None;
        // if (self.@"shell-integration" != .none) {
        //     self.@"shell-integration" = .detect;
        // }
        if self.shell_integration != ShellIntegration::None {
            self.shell_integration = ShellIntegration::Detect;
        }

        // Do not continue, we consumed everything.
        // return false;
        false
    }

    // fn compatGtkTabsLocation(
    //     self: *Config,
    //     alloc: Allocator,
    //     key: []const u8,
    //     value: ?[]const u8,
    // ) bool {
    fn compat_gtk_tabs_location(&mut self, key: &[u8], value: Option<&[u8]>) -> bool {
        // _ = alloc;
        // assert(std.mem.eql(u8, key, "gtk-tabs-location"));
        assert!(key == b"gtk-tabs-location");

        // if (std.mem.eql(u8, value orelse "", "hidden")) {
        if value.unwrap_or(b"") == b"hidden" {
            // self.@"window-show-tab-bar" = .never;
            self.window_show_tab_bar = WindowShowTabBar::Never;
            // return true;
            return true;
        }

        // return false;
        false
    }

    // fn compatGtkSingleInstance(
    //     self: *Config,
    //     alloc: Allocator,
    //     key: []const u8,
    //     value: ?[]const u8,
    // ) bool {
    fn compat_gtk_single_instance(&mut self, key: &[u8], value: Option<&[u8]>) -> bool {
        // _ = alloc;
        // assert(std.mem.eql(u8, key, "gtk-single-instance"));
        assert!(key == b"gtk-single-instance");

        // if (std.mem.eql(u8, value orelse "", "desktop")) {
        if value.unwrap_or(b"") == b"desktop" {
            // self.@"gtk-single-instance" = .detect;
            self.gtk_single_instance = GtkSingleInstance::Detect;
            // return true;
            return true;
        }

        // return false;
        false
    }

    // fn compatCursorInvertFgBg(
    //     self: *Config,
    //     alloc: Allocator,
    //     key: []const u8,
    //     value_: ?[]const u8,
    // ) bool {
    fn compat_cursor_invert_fg_bg(&mut self, key: &[u8], value_: Option<&[u8]>) -> bool {
        // _ = alloc;
        // assert(std.mem.eql(u8, key, "cursor-invert-fg-bg"));
        assert!(key == b"cursor-invert-fg-bg");

        // We don't do anything if the value is unset, which is technically
        // not EXACTLY the same as prior behavior since it would fallback
        // to doing whatever cursor-color/cursor-text were set to, but
        // I don't want to store what that is separately so this is close
        // enough.
        //
        // Realistically, these fields were mutually exclusive so anyone
        // relying on that behavior should just upgrade to the new
        // cursor-color/cursor-text fields.
        // const set = cli.args.parseBool(value_ orelse "t") catch return false;
        let set = match rustty_args::args::parse_bool(value_.unwrap_or(b"t")) {
            Ok(v) => v,
            Err(_) => return false,
        };
        // if (set) {
        if set {
            // self.@"cursor-color" = .@"cell-foreground";
            self.cursor_color = Some(TerminalColor::CellForeground);
            // self.@"cursor-text" = .@"cell-background";
            self.cursor_text = Some(TerminalColor::CellBackground);
        }

        // return true;
        true
    }

    // fn compatSelectionInvertFgBg(
    //     self: *Config,
    //     alloc: Allocator,
    //     key: []const u8,
    //     value_: ?[]const u8,
    // ) bool {
    fn compat_selection_invert_fg_bg(&mut self, key: &[u8], value_: Option<&[u8]>) -> bool {
        // _ = alloc;
        // assert(std.mem.eql(u8, key, "selection-invert-fg-bg"));
        assert!(key == b"selection-invert-fg-bg");

        // const set = cli.args.parseBool(value_ orelse "t") catch return false;
        let set = match rustty_args::args::parse_bool(value_.unwrap_or(b"t")) {
            Ok(v) => v,
            Err(_) => return false,
        };
        // if (set) {
        if set {
            // self.@"selection-foreground" = .@"cell-background";
            self.selection_foreground = Some(TerminalColor::CellBackground);
            // self.@"selection-background" = .@"cell-foreground";
            self.selection_background = Some(TerminalColor::CellForeground);
        }

        // return true;
        true
    }

    // fn compatBoldIsBright(
    //     self: *Config,
    //     alloc: Allocator,
    //     key: []const u8,
    //     value_: ?[]const u8,
    // ) bool {
    fn compat_bold_is_bright(&mut self, key: &[u8], value_: Option<&[u8]>) -> bool {
        // _ = alloc;
        // assert(std.mem.eql(u8, key, "bold-is-bright"));
        assert!(key == b"bold-is-bright");

        // const isset = cli.args.parseBool(value_ orelse "t") catch return false;
        let isset = match rustty_args::args::parse_bool(value_.unwrap_or(b"t")) {
            Ok(v) => v,
            Err(_) => return false,
        };
        // if (isset) {
        if isset {
            // self.@"bold-color" = .bright;
            self.bold_color = Some(BoldColor::Bright);
        }

        // return true;
        true
    }

    // fn compatMacOSDockDropBehavior(
    //     self: *Config,
    //     alloc: Allocator,
    //     key: []const u8,
    //     value: ?[]const u8,
    // ) bool {
    fn compat_macos_dock_drop_behavior(&mut self, key: &[u8], value: Option<&[u8]>) -> bool {
        // _ = alloc;
        // assert(std.mem.eql(u8, key, "macos-dock-drop-behavior"));
        assert!(key == b"macos-dock-drop-behavior");

        // if (std.mem.eql(u8, value orelse "", "window")) {
        if value.unwrap_or(b"") == b"window" {
            // self.@"macos-dock-drop-behavior" = .@"new-window";
            self.macos_dock_drop_behavior = MacOSDockDropBehavior::NewWindow;
            // return true;
            return true;
        }

        // return false;
        false
    }

    // fn compatCopyOnSelect(
    //     self: *Config,
    //     alloc: Allocator,
    //     key: []const u8,
    //     value: ?[]const u8,
    // ) bool {
    fn compat_copy_on_select(&mut self, key: &[u8], value: Option<&[u8]>) -> bool {
        // _ = alloc;
        // assert(std.mem.eql(u8, key, "copy-on-select"));
        assert!(key == b"copy-on-select");

        // if (std.mem.eql(u8, value orelse "", "true")) {
        if value.unwrap_or(b"") == b"true" {
            // self.@"copy-on-select" = switch (builtin.os.tag) {
            //     .linux, .freebsd => .primary,
            //     else => .clipboard,
            // };
            self.copy_on_select = if cfg!(any(target_os = "linux", target_os = "freebsd")) {
                CopyOnSelect::Primary
            } else {
                CopyOnSelect::Clipboard
            };
            // return true;
            return true;
        }

        // if (std.mem.eql(u8, value orelse "", "false")) {
        if value.unwrap_or(b"") == b"false" {
            // self.@"copy-on-select" = .none;
            self.copy_on_select = CopyOnSelect::None;
            // return true;
            return true;
        }

        // return false;
        false
    }

    /// Add a diagnostic message to the config with the given string.
    /// This is always added with a location of "none".
    // pub fn addDiagnosticFmt(
    //     self: *Config,
    //     comptime fmt: []const u8,
    //     args: anytype,
    // ) Allocator.Error!void {
    //
    // PORT NOTE: Rust has no comptime format string; callers pass the already-
    // formatted message bytes (the `std.fmt.allocPrintSentinel(.., fmt, args, 0)`
    // result). The location defaults to `.none` exactly as the reference.
    pub fn add_diagnostic_fmt(&mut self, message: Vec<u8>) {
        // const alloc = self._arena.?.allocator();
        // try self._diagnostics.append(alloc, .{
        //     .message = try std.fmt.allocPrintSentinel(alloc, fmt, args, 0),
        // });
        self._diagnostics
            .append(rustty_args::diagnostics::Diagnostic {
                message,
                ..Default::default()
            });
    }

    /// Create a shallow copy of this config. This will share all the memory
    /// allocated with the previous config but will have a new arena for
    /// any changes or new allocations. The config should have `deinit`
    /// called when it is complete.
    ///
    /// Beware: these shallow clones are not meant for a long lifetime,
    /// they are just meant to exist temporarily for the duration of some
    /// modifications. It is very important that the original config not
    /// be deallocated while shallow clones exist.
    // pub fn shallowClone(self: *const Config, alloc_gpa: Allocator) Config {
    //     var result = self.*;
    //     result._arena = .init(alloc_gpa);
    //     return result;
    // }
    //
    // PORT NOTE: the reference's shallow clone shares the original's bytes via a
    // bitwise struct copy and only swaps in a fresh arena. The Rust port owns data
    // per-field (no shared bump arena), so a "shallow" clone is a deep `.clone()`
    // with the `_arena` flag set — the closest faithful equivalent. Long-lived
    // aliasing of the original's bytes (the reference's caveat) does not apply
    // since nothing is shared.
    pub fn shallow_clone(&self) -> Config {
        // var result = self.*;
        let mut result = self.clone_config();
        // result._arena = .init(alloc_gpa);
        result._arena = true;
        // return result;
        result
    }

    /// Load and parse the CLI args.
    // pub fn loadCliArgs(self: *Config, alloc_gpa: Allocator) !void {
    pub fn load_cli_args(&mut self) -> Result<(), LoadError> {
        // switch (builtin.os.tag) {
        //     .windows => {},
        //     // Fast-path if we are Linux/BSD and have no args.
        //     .linux, .freebsd => if (std.os.argv.len <= 1) return,
        //     // Everything else we have to at least try because it may
        //     // not use std.os.argv.
        //     else => {},
        // }
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        {
            if std::env::args_os().len() <= 1 {
                return Ok(());
            }
        }

        // On Linux, we have a special case where if the executing
        // program is "xdg-terminal-exec" then we treat all CLI
        // args as if they are a command to execute.
        //
        // In this mode, we also behave slightly differently:
        //
        //   - The initial window title is set to the full command. This
        //     can be used with window managers to modify positioning,
        //     styling, etc. based on the command.
        //
        // See: https://github.com/Vladimir-csp/xdg-terminal-exec
        // if ((comptime builtin.os.tag == .linux) or (comptime builtin.os.tag == .freebsd)) {
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        {
            // if (internal_os.xdg.parseTerminalExec(std.os.argv)) |args| {
            let argv: Vec<String> = std::env::args().collect();
            let argv_refs: Vec<&str> = argv.iter().map(String::as_str).collect();
            if let Some(args) = rustty_os::xdg::parseTerminalExec(&argv_refs) {
                // const arena_alloc = self._arena.?.allocator();

                // First, we add an artificial "-e" so that if we
                // replay the inputs to rebuild the config (i.e. if
                // a theme is set) then we will get the same behavior.
                // try self._replay_steps.append(arena_alloc, .@"-e");
                self._replay_steps.push(ReplayStep::DashE);

                // Next, take all remaining args and use that to build up
                // a command to execute.
                // var builder: std.ArrayList([:0]const u8) = .empty;
                // errdefer builder.deinit(arena_alloc);
                let mut builder: Vec<Vec<u8>> = Vec::new();
                // for (args) |arg_raw| {
                for arg_raw in args {
                    // const arg = std.mem.sliceTo(arg_raw, 0);
                    // const copy = try arena_alloc.dupeZ(u8, arg);
                    let copy = arg_raw.as_bytes().to_vec();
                    // try self._replay_steps.append(arena_alloc, .{ .arg = copy });
                    self._replay_steps.push(ReplayStep::Arg(copy.clone()));
                    // try builder.append(arena_alloc, copy);
                    builder.push(copy);
                }

                // self.@"_xdg-terminal-exec" = true;
                self._xdg_terminal_exec = true;
                // self.@"initial-command" = .{ .direct = try builder.toOwnedSlice(arena_alloc) };
                self.initial_command = Some(Command::Direct(builder));
                // return;
                return Ok(());
            }
        }

        // We set config-default-files to true here because this
        // should always be reset so we can detect if it is set
        // in the CLI since it is documented as having no affect
        // from files.
        // self.@"config-default-files" = true;
        self.config_default_files = true;

        // Keep track of the replay steps up to this point so we
        // can replay if we are discarding the default files.
        // const replay_len_start = self._replay_steps.items.len;
        let replay_len_start = self._replay_steps.len();

        // font-family settings set via the CLI overwrite any prior values
        // rather than append. This avoids a UX oddity where you have to
        // specify `font-family=""` to clear the font families.
        // const fields = &[_][]const u8{ "font-family", ... };
        // inline for (fields) |field| @field(self, field).overwrite_next = true;
        self.font_family.overwrite_next = true;
        self.font_family_bold.overwrite_next = true;
        self.font_family_italic.overwrite_next = true;
        self.font_family_bold_italic.overwrite_next = true;
        // defer {
        //     inline for (fields) |field| @field(self, field).overwrite_next = false;
        // }
        //
        // PORT NOTE: the `defer` reset is performed at the end of this fn (Rust has
        // no `defer`; the resets are placed before each return path / at the tail).

        // Initialize our CLI iterator.
        // var iter = try cli.args.argsIterator(alloc_gpa);
        // defer iter.deinit();
        let mut iter = match rustty_args::args::args_iterator() {
            Ok(it) => it,
            Err(_) => {
                self.reset_font_family_overwrite();
                return Ok(());
            }
        };
        // try self.loadIter(alloc_gpa, &iter);
        let r = self.load_iter(&mut iter);
        iter.deinit();
        r?;

        // (defer reset, see note above)
        self.reset_font_family_overwrite();

        // If we are not loading the default files, then we need to
        // replay the steps up to this point so that we can rebuild
        // the config without it.
        // if (!self.@"config-default-files") reload: {
        if !self.config_default_files {
            'reload: {
                // const replay_len_end = self._replay_steps.items.len;
                let replay_len_end = self._replay_steps.len();
                // if (replay_len_end == replay_len_start) break :reload;
                if replay_len_end == replay_len_start {
                    break 'reload;
                }
                // log.info("config-default-files unset, discarding configuration from default files", .{});
                log_info(
                    "config-default-files unset, discarding configuration from default files"
                        .to_string(),
                );

                // var new_config = try self.cloneEmpty(alloc_gpa);
                // errdefer new_config.deinit();
                let mut new_config = self.clone_empty();
                // var it = Replay.iterator(
                //     self._replay_steps.items[replay_len_start..replay_len_end],
                //     &new_config,
                // );
                let slice: Vec<ReplayStep> =
                    self._replay_steps[replay_len_start..replay_len_end].to_vec();
                let mut it = ReplayIterator::new(slice);
                // try new_config.loadIter(alloc_gpa, &it);
                new_config.load_iter(&mut it)?;
                // self.deinit();
                self.deinit();
                // self.* = new_config;
                *self = new_config;
            }
        }

        // Any paths referenced from the CLI are relative to the current working
        // directory.
        // var buf: [std.fs.max_path_bytes]u8 = undefined;
        // try self.expandPaths(try std.fs.cwd().realpath(".", &buf));
        let cwd = std::env::current_dir()
            .ok()
            .and_then(|p| p.canonicalize().ok())
            .map(|p| path_to_bytes(&p))
            .unwrap_or_default();
        self.expand_paths(&cwd)?;

        Ok(())
    }

    /// Reset the `overwrite_next` flag on the font-family fields (the reference's
    /// `defer { inline for (fields) ... = false; }` in `loadCliArgs`).
    fn reset_font_family_overwrite(&mut self) {
        self.font_family.overwrite_next = false;
        self.font_family_bold.overwrite_next = false;
        self.font_family_italic.overwrite_next = false;
        self.font_family_bold_italic.overwrite_next = false;
    }

    /// Load and parse the config files that were added in the "config-file" key.
    // pub fn loadRecursiveFiles(self: *Config, alloc_gpa: Allocator) !void {
    pub fn load_recursive_files(&mut self) -> Result<(), LoadError> {
        // if (self.@"config-file".value.items.len == 0) return;
        if self.config_file.value.is_empty() {
            return Ok(());
        }
        // const arena_alloc = self._arena.?.allocator();

        // Keeps track of loaded files to prevent cycles.
        // var loaded = std.StringHashMap(void).init(alloc_gpa);
        // defer loaded.deinit();
        let mut loaded: std::collections::HashSet<Vec<u8>> = std::collections::HashSet::new();

        // We need to insert all of our loaded config-file values
        // PRIOR to the "-e" in our replay steps, since everything
        // after "-e" becomes an "initial-command". To do this, we
        // dupe the values if we find it.
        // var replay_suffix: std.ArrayList(Replay.Step) = .empty;
        // defer replay_suffix.deinit(alloc_gpa);
        let mut replay_suffix: Vec<ReplayStep> = Vec::new();
        // for (self._replay_steps.items, 0..) |step, i| if (step == .@"-e") {
        for i in 0..self._replay_steps.len() {
            if matches!(self._replay_steps[i], ReplayStep::DashE) {
                // We don't need to clone the steps because they should
                // all be allocated in our arena and we're keeping our
                // arena.
                // try replay_suffix.appendSlice(alloc_gpa, self._replay_steps.items[i..]);
                replay_suffix.extend_from_slice(&self._replay_steps[i..]);
                // Remove our old values. Again, don't need to free any
                // memory here because its all part of our arena.
                // self._replay_steps.shrinkRetainingCapacity(i);
                self._replay_steps.truncate(i);
                // break;
                break;
            }
        }

        // We must use a while below and not a for(items) because we
        // may add items to the list while iterating for recursive
        // config-file entries.
        // var i: usize = 0;
        let mut i: usize = 0;
        // while (i < self.@"config-file".value.items.len) : (i += 1) {
        while i < self.config_file.value.len() {
            // const path, const optional = switch (self.@"config-file".value.items[i]) {
            //     .optional => |path| .{ path, true },
            //     .required => |path| .{ path, false },
            // };
            let (path, optional): (Vec<u8>, bool) = match &self.config_file.value[i] {
                Path::Optional(path) => (path.clone(), true),
                Path::Required(path) => (path.clone(), false),
            };

            // Error paths
            // if (path.len == 0) continue;
            if path.is_empty() {
                i += 1;
                continue;
            }

            // All paths should already be absolute at this point because
            // they're fixed up after each load.
            // assert(std.fs.path.isAbsolute(path));
            assert!(std::path::Path::new(os_str_from_bytes(&path)).is_absolute());

            // We must only load a unique file once
            // if (try loaded.fetchPut(path, {}) != null) {
            if !loaded.insert(path.clone()) {
                // const diag: cli.Diagnostic = .{
                //     .message = try std.fmt.allocPrintSentinel(
                //         arena_alloc, "config-file {s}: cycle detected", .{path}, 0),
                // };
                let diag = rustty_args::diagnostics::Diagnostic {
                    message: format!(
                        "config-file {}: cycle detected",
                        String::from_utf8_lossy(&path)
                    )
                    .into_bytes(),
                    ..Default::default()
                };
                // try self._diagnostics.append(arena_alloc, diag);
                self._diagnostics.append(diag.clone());
                // try self._replay_steps.append(arena_alloc, .{ .diagnostic = diag });
                self._replay_steps.push(ReplayStep::Diagnostic(diag));
                // continue;
                i += 1;
                continue;
            }

            // var file = std.fs.openFileAbsolute(path, .{}) catch |err| {
            let file = match std::fs::File::open(os_str_from_bytes(&path)) {
                Ok(f) => f,
                Err(err) => {
                    // if (err != error.FileNotFound or !optional) {
                    if err.kind() != std::io::ErrorKind::NotFound || !optional {
                        // const diag: cli.Diagnostic = .{
                        //     .message = ... "error opening config-file {s}: {}" ...
                        // };
                        let diag = rustty_args::diagnostics::Diagnostic {
                            message: format!(
                                "error opening config-file {}: {}",
                                String::from_utf8_lossy(&path),
                                err
                            )
                            .into_bytes(),
                            ..Default::default()
                        };
                        // try self._diagnostics.append(arena_alloc, diag);
                        self._diagnostics.append(diag.clone());
                        // try self._replay_steps.append(arena_alloc, .{ .diagnostic = diag });
                        self._replay_steps.push(ReplayStep::Diagnostic(diag));
                    }
                    // continue;
                    i += 1;
                    continue;
                }
            };
            // defer file.close();

            // const stat = try file.stat();
            let stat = file.metadata()?;
            // switch (stat.kind) {
            //     .file => {},
            //     else => |kind| { ... "config-file {s}: not reading because file type is {s}" ... continue; },
            // }
            if !stat.is_file() {
                let diag = rustty_args::diagnostics::Diagnostic {
                    message: format!(
                        "config-file {}: not reading because file type is {}",
                        String::from_utf8_lossy(&path),
                        file_kind_name(&stat)
                    )
                    .into_bytes(),
                    ..Default::default()
                };
                self._diagnostics.append(diag.clone());
                self._replay_steps.push(ReplayStep::Diagnostic(diag));
                i += 1;
                continue;
            }

            // try self.loadFsFile(arena_alloc, &file, path);
            self.load_fs_file(file, &path)?;
            i += 1;
        }

        // If we have a suffix, add that back.
        // if (replay_suffix.items.len > 0) {
        if !replay_suffix.is_empty() {
            // try self._replay_steps.appendSlice(arena_alloc, replay_suffix.items);
            self._replay_steps.extend(replay_suffix);
        }

        Ok(())
    }

    // Get the arena allocator associated with the configuration.
    // pub fn arenaAlloc(self: *Config) Allocator {
    //     return self._arena.?.allocator();
    // }
    //
    // PORT NOTE: there is no shared bump arena in the Rust port (each value owns
    // its bytes), so `arenaAlloc` has no observable Rust counterpart. It is
    // retained as a comment for fidelity; call sites that needed the arena now use
    // owned allocation directly.

    /// Returns true if the given key event would trigger a keybinding
    /// if it were to be processed. This is useful for determining if
    /// a key event should be sent to the terminal or not.
    // pub fn keyEventIsBinding(
    //     self: *Config,
    //     event: inputpkg.KeyEvent,
    // ) bool {
    //
    // PORT NOTE: consumed by the libghostty C-ABI export `ghostty_config_key_is_binding`
    // (embedded.zig:1470, rustty-apprt-c chunk E6). Renamed to snake_case per this
    // module's method convention (`clone_config`/`change_conditional_state`/…).
    // `self: *Config` is mutable-capable in the reference but the body only reads, so
    // `&self` is the faithful minimal. `event` is taken by value (the reference
    // `event: inputpkg.KeyEvent`); the borrow for `getEvent` is taken at the call.
    // There is no reference `test{}` for this fn.
    pub fn key_event_is_binding(&self, event: rustty_vt::input::key::KeyEvent) -> bool {
        // switch (event.action) {
        //     .release => return false,
        //     .press, .repeat => {},
        // }
        match event.action {
            rustty_vt::input::key::Action::Release => return false,
            rustty_vt::input::key::Action::Press | rustty_vt::input::key::Action::Repeat => {}
        }

        // If we have a keybinding for this event then we return true.
        // return self.keybind.set.getEvent(event) != null;
        self.keybind.set.get_event(&event).is_some()
    }

    /// Change the state of conditionals and reload the configuration
    /// based on the new state. This returns a new configuration based
    /// on the new state. The caller must free the old configuration if they
    /// wish.
    ///
    /// This returns null if the conditional state would result in no changes
    /// to the configuration. In this case, the caller can continue to use
    /// the existing configuration or clone if they want a copy.
    ///
    /// This doesn't re-read any files, it just re-applies the same
    /// configuration with the new conditional state. Importantly, this means
    /// that if you change the conditional state and the user in the interim
    /// deleted a file that was referenced in the configuration, then the
    /// configuration can still be reloaded.
    // pub fn changeConditionalState(
    //     self: *const Config,
    //     new: conditional.State,
    // ) !?Config {
    pub fn change_conditional_state(
        &self,
        new: conditional::State,
    ) -> Result<Option<Config>, LoadError> {
        // If the conditional state between the old and new is the same,
        // then we don't need to do anything.
        // relevant: {
        let relevant = 'relevant: {
            // inline for (@typeInfo(conditional.Key).@"enum".fields) |field| {
            //     const key: conditional.Key = @field(conditional.Key, field.name);
            //     // Conditional set contains the keys that this config uses. So we
            //     // only continue if we use this key.
            //     if (self._conditional_set.contains(key) and !deepEqual(
            //         @TypeOf(@field(self._conditional_state, field.name)),
            //         @field(self._conditional_state, field.name),
            //         @field(new, field.name),
            //     )) {
            //         break :relevant;
            //     }
            // }
            for key in [conditional::Key::Theme, conditional::Key::Os] {
                let differs = match key {
                    conditional::Key::Theme => self._conditional_state.theme != new.theme,
                    // PORT NOTE: `OsTag` carries only its tag-name bytes (the
                    // reference's `deepEqual` over the enum compares its tag), so
                    // we compare tag names.
                    conditional::Key::Os => {
                        self._conditional_state.os.tag_name() != new.os.tag_name()
                    }
                };
                if self._conditional_set.contains(&key) && differs {
                    break 'relevant true;
                }
            }
            // If we got here, then we didn't find any differences between
            // the old and new conditional state that would affect the
            // configuration.
            // return null;
            false
        };
        if !relevant {
            return Ok(None);
        }

        // Create our new configuration
        // const alloc_gpa = self._arena.?.child_allocator;
        // var new_config = try self.cloneEmpty(alloc_gpa);
        // errdefer new_config.deinit();
        let mut new_config = self.clone_empty();

        // Set our conditional state so the replay below can use it
        // new_config._conditional_state = new;
        new_config._conditional_state = new;

        // Replay all of our steps to rebuild the configuration
        // var it = Replay.iterator(self._replay_steps.items, &new_config);
        let mut it = ReplayIterator::new(self._replay_steps.clone());
        // try new_config.loadIter(alloc_gpa, &it);
        new_config.load_iter(&mut it)?;
        // try new_config.finalize();
        new_config.finalize()?;

        // return new_config;
        Ok(Some(new_config))
    }

    /// Expand the relative paths in config-files to be absolute paths
    /// relative to the base directory.
    // fn expandPaths(self: *Config, base: []const u8) !void {
    fn expand_paths(&mut self, base: &[u8]) -> Result<(), LoadError> {
        // const arena_alloc = self._arena.?.allocator();

        // Keep track of this step for replays
        // try self._replay_steps.append(
        //     arena_alloc,
        //     .{ .expand = try arena_alloc.dupe(u8, base) },
        // );
        self._replay_steps.push(ReplayStep::Expand(base.to_vec()));

        // Expand all of our paths
        // inline for (@typeInfo(Config).@"struct".fields) |field| {
        //     switch (field.type) {
        //         RepeatablePath, Path => {
        //             try @field(self, field.name).expand(arena_alloc, base, &self._diagnostics);
        //         },
        //         ?RepeatablePath, ?Path => {
        //             if (@field(self, field.name)) |*path| {
        //                 try path.expand(arena_alloc, base, &self._diagnostics);
        //             }
        //         },
        //         else => {},
        //     }
        // }
        //
        // PORT NOTE: Rust has no struct reflection; the `RepeatablePath`/`Path`
        // (and optional) fields are enumerated explicitly. The set is:
        //   RepeatablePath: config_file, custom_shader, gtk_custom_css
        //   ?Path:          background_image, bell_audio_path
        self.config_file.expand(base, &mut self._diagnostics)?;
        self.custom_shader.expand(base, &mut self._diagnostics)?;
        self.gtk_custom_css.expand(base, &mut self._diagnostics)?;
        if let Some(path) = &mut self.background_image {
            path.expand(base, &mut self._diagnostics)?;
        }
        if let Some(path) = &mut self.bell_audio_path {
            path.expand(base, &mut self._diagnostics)?;
        }

        Ok(())
    }

    // fn loadTheme(self: *Config, theme: Theme) !void {
    fn load_theme(&mut self, theme: Theme) -> Result<(), LoadError> {
        // Load the correct theme depending on the conditional state.
        // Dark/light themes were programmed prior to conditional configuration
        // so when we introduce that we probably want to replace this.
        // const name: []const u8 = switch (self._conditional_state.theme) {
        //     .light => theme.light,
        //     .dark => theme.dark,
        // };
        let name: Vec<u8> = match self._conditional_state.theme {
            conditional::Theme::Light => theme.light.clone(),
            conditional::Theme::Dark => theme.dark.clone(),
        };

        // Find our theme file and open it. See the open function for details.
        // const themefile = (try themepkg.open(
        //     self._arena.?.allocator(),
        //     name,
        //     &self._diagnostics,
        // )) orelse return;
        let themefile = match crate::theme::open(&name, &mut self._diagnostics) {
            Some(tf) => tf,
            None => return Ok(()),
        };
        // const path = themefile.path;
        let path = themefile.path;
        // const file = themefile.file;
        // defer file.close();
        let mut file = themefile.file;

        // From this point onwards, we load the theme and do a bit of a dance
        // to achieve two separate goals:
        //
        //   (1) We want the theme to be loaded and our existing config to
        //       override the theme. So we need to load the theme and apply
        //       our config on top of it.
        //
        //   (2) We want to free existing memory that we aren't using anymore
        //       as a result of reloading the configuration.
        //
        // Point 2 is strictly a result of aur approach to point 1, but it is
        // a nice property to have to limit memory bloat as much as possible.

        // Load into a new configuration so that we can free the existing memory.
        // const alloc_gpa = self._arena.?.child_allocator;
        // var new_config = try self.cloneEmpty(alloc_gpa);
        // errdefer new_config.deinit();
        let mut new_config = self.clone_empty();

        // Load our theme
        // var buf: [2048]u8 = undefined;
        // var file_reader = file.reader(&buf);
        // const reader = &file_reader.interface;
        // var iter: cli.args.LineIterator = .{ .r = reader, .filepath = path };
        // try new_config.loadIter(alloc_gpa, &iter);
        use std::io::Read;
        let mut contents: Vec<u8> = Vec::new();
        file.read_to_end(&mut contents)?;
        {
            let mut iter = rustty_args::args::LineIterator::with_filepath(&contents, &path);
            new_config.load_iter(&mut iter)?;
        }

        // Setup our replay to be conditional.
        // conditional: for (new_config._replay_steps.items) |*item| {
        'conditional: for item in new_config._replay_steps.iter_mut() {
            // switch (item.*) {
            match item {
                // .expand, .diagnostic => {},
                ReplayStep::Expand(_) | ReplayStep::Diagnostic(_) => {}

                // If we see "-e" then we do NOT make the following arguments
                // conditional since they are supposed to be part of the
                // initial command.
                // .@"-e" => break :conditional,
                ReplayStep::DashE => break 'conditional,

                // Change our arg to be conditional on our theme.
                // .arg => |v| {
                ReplayStep::Arg(v) => {
                    // const alloc_arena = new_config._arena.?.allocator();
                    // const conds = try alloc_arena.alloc(Conditional, 1);
                    // conds[0] = .{
                    //     .key = .theme,
                    //     .op = .eq,
                    //     .value = @tagName(self._conditional_state.theme),
                    // };
                    let conds = vec![Conditional {
                        key: conditional::Key::Theme,
                        op: conditional::Op::Eq,
                        value: self._conditional_state.theme.tag_name().to_vec(),
                    }];
                    // item.* = .{ .conditional_arg = .{ .conditions = conds, .arg = v } };
                    *item = ReplayStep::ConditionalArg {
                        conditions: conds,
                        arg: std::mem::take(v),
                    };
                }

                // .conditional_arg => |v| {
                ReplayStep::ConditionalArg { conditions, arg } => {
                    // const alloc_arena = new_config._arena.?.allocator();
                    // const conds = try alloc_arena.alloc(Conditional, v.conditions.len + 1);
                    // conds[0] = .{ .key = .theme, .op = .eq, .value = @tagName(self._conditional_state.theme) };
                    // @memcpy(conds[1..], v.conditions);
                    let mut conds = Vec::with_capacity(conditions.len() + 1);
                    conds.push(Conditional {
                        key: conditional::Key::Theme,
                        op: conditional::Op::Eq,
                        value: self._conditional_state.theme.tag_name().to_vec(),
                    });
                    conds.extend(std::mem::take(conditions));
                    // item.* = .{ .conditional_arg = .{ .conditions = conds, .arg = v.arg } };
                    *item = ReplayStep::ConditionalArg {
                        conditions: conds,
                        arg: std::mem::take(arg),
                    };
                }
            }
        }

        // Replay our previous inputs so that we can override values
        // from the theme.
        // var slice_it = Replay.iterator(self._replay_steps.items, &new_config);
        // try new_config.loadIter(alloc_gpa, &slice_it);
        let mut slice_it = ReplayIterator::new(self._replay_steps.clone());
        new_config.load_iter(&mut slice_it)?;

        // Success, swap our new config in and free the old.
        // self.deinit();
        self.deinit();
        // self.* = new_config;
        *self = new_config;

        Ok(())
    }

    /// Call this once after you are done setting configuration. This
    /// is idempotent but will waste memory if called multiple times.
    // pub fn finalize(self: *Config) !void {
    pub fn finalize(&mut self) -> Result<(), LoadError> {
        // const alloc = self._arena.?.allocator();

        // We always load the theme first because it may set other fields
        // in our config.
        // if (self.theme) |*theme| {
        if let Some(theme) = &mut self.theme {
            // try theme.finalize(alloc);
            theme.finalize();
            // const different = !std.mem.eql(u8, theme.light, theme.dark);
            let different = theme.light != theme.dark;

            // Warning: loadTheme will deinit our existing config and replace
            // it so all memory from self prior to this point will be freed.
            // try self.loadTheme(theme.*);
            //
            // PORT NOTE: `theme.*` copies the (finalized) payload out of the
            // capture; the clone ends the `&mut self.theme` borrow before
            // `load_theme` takes `&mut self`.
            let theme = theme.clone();
            self.load_theme(theme)?;

            // If we have different light vs dark mode themes, disable
            // window-theme = auto since that breaks it.
            // if (different) {
            if different {
                // This setting doesn't make sense with different light/dark themes
                // because it'll force the theme based on the Rustty theme.
                // if (self.@"window-theme" == .auto) self.@"window-theme" = .system;
                if self.window_theme == WindowTheme::Auto {
                    self.window_theme = WindowTheme::System;
                }

                // Mark that we use a conditional theme
                // self._conditional_set.insert(.theme);
                self._conditional_set.insert(conditional::Key::Theme);
            }
        }

        // Used for a variety of defaults. See the function docs as well the
        // specific variable use sites for more details.
        // const probable_cli = probableCliEnvironment();
        let probable_cli = probable_cli_environment();

        // If we have a font-family set and don't set the others, default
        // the others to the font family. This way, if someone does
        // --font-family=foo, then we try to get the stylized versions of
        // "foo" as well.
        // if (self.@"font-family".count() > 0) {
        if self.font_family.count() > 0 {
            // const fields = &[_][]const u8{
            //     "font-family-bold",
            //     "font-family-italic",
            //     "font-family-bold-italic",
            // };
            // inline for (fields) |field| {
            //     if (@field(self, field).count() == 0) {
            //         @field(self, field) = try self.@"font-family".clone(alloc);
            //     }
            // }
            if self.font_family_bold.count() == 0 {
                self.font_family_bold = self.font_family.clone_in();
            }
            if self.font_family_italic.count() == 0 {
                self.font_family_italic = self.font_family.clone_in();
            }
            if self.font_family_bold_italic.count() == 0 {
                self.font_family_bold_italic = self.font_family.clone_in();
            }
        }

        // Prevent setting TERM to an empty string
        // if (self.term.len == 0) {
        if self.term.is_empty() {
            // HACK: See comment above at definition
            // self.term = "xterm-ghostty";
            self.term = rustty_identity::TERM.as_bytes().to_vec();
        }

        // The default for the working directory depends on the system.
        // var wd: WorkingDirectory = self.@"working-directory" orelse if (probable_cli)
        //     .inherit
        // else
        //     .home;
        let mut wd: WorkingDirectory = self.working_directory.clone().unwrap_or(if probable_cli {
            WorkingDirectory::Inherit
        } else {
            WorkingDirectory::Home
        });

        // If we are missing either a command or home directory, we need
        // to look up defaults which is kind of expensive. We only do this
        // on desktop.
        // if ((comptime !builtin.target.cpu.arch.isWasm()) and
        //     (comptime !builtin.is_test))
        // {
        #[cfg(not(test))]
        {
            // if (self.command == null or wd == .home) command: {
            if self.command.is_none() || wd == WorkingDirectory::Home {
                'command: {
                    // First look up the command using the SHELL env var if needed.
                    // We don't do this in flatpak because SHELL in Flatpak is always
                    // set to /bin/sh.
                    // if (self.command) |cmd|
                    //     log.info("shell src=config value={}", .{cmd})
                    // else shell_env: {
                    if let Some(_cmd) = &self.command {
                        log_info("shell src=config".to_string());
                    } else {
                        'shell_env: {
                            // Flatpak always gets its shell from outside the sandbox
                            // if (internal_os.isFlatpak()) break :shell_env;
                            if rustty_os::flatpak::isFlatpak() {
                                break 'shell_env;
                            }

                            // If we were launched from the desktop, our SHELL env var
                            // will represent our SHELL at login time. We only want to
                            // read from SHELL if we're in a probable CLI environment.
                            // if (!probable_cli) break :shell_env;
                            if !probable_cli {
                                break 'shell_env;
                            }

                            // const value = global.environ().getAlloc(alloc, "SHELL")
                            //     catch |err| switch (err) {
                            //         error.EnvironmentVariableMissing => break :shell_env,
                            //         else => return err,
                            //     };
                            //
                            // PORT NOTE: through the captured global environ (see
                            // `env_is_probably_cli`'s note), and as RAW BYTES —
                            // `SHELL` is a `[]u8` upstream and `std::env::var`
                            // would drop a non-UTF-8 shell path on the floor.
                            let Some(value) = rustty_global::environ::environ().get_alloc(b"SHELL")
                            else {
                                break 'shell_env;
                            };
                            {
                                // log.info("default shell source=env value={s}", .{value});
                                log_info(format!(
                                    "default shell source=env value={}",
                                    String::from_utf8_lossy(&value)
                                ));
                                // const copy = try alloc.dupeZ(u8, value);
                                // self.command = .{ .shell = copy };
                                self.command = Some(Command::Shell(value));
                                // If we don't need the working directory, then we can exit now.
                                // if (wd != .home) break :command;
                                if wd != WorkingDirectory::Home {
                                    break 'command;
                                }
                            } // } else |_| {}
                        }
                    }

                    // switch (builtin.os.tag) {
                    //     .windows => { ... },
                    //     else => { ... passwd ... },
                    // }
                    #[cfg(target_os = "windows")]
                    {
                        // if (self.command == null) {
                        if self.command.is_none() {
                            // log.warn("no default shell found, will default to using cmd", .{});
                            log_warn(
                                "no default shell found, will default to using cmd".to_string(),
                            );
                            // self.command = .{ .shell = "cmd.exe" };
                            self.command = Some(Command::Shell(b"cmd.exe".to_vec()));
                        }
                        // if (wd == .home) {
                        if wd == WorkingDirectory::Home {
                            // var buf: [std.fs.max_path_bytes]u8 = undefined;
                            let mut buf = [0u8; MAX_PATH_BYTES];
                            // if (try internal_os.home(&buf)) |home| {
                            if let Ok(Some(home)) =
                                rustty_os::homedir::home(&rustty_global::environ_map(), &mut buf)
                            {
                                // wd = .{ .path = try alloc.dupe(u8, home) };
                                wd = WorkingDirectory::Path(home.to_vec());
                            } else {
                                // wd = .inherit;
                                wd = WorkingDirectory::Inherit;
                            }
                        }
                    }
                    // WS14 T6 (wasm32 arm): a browser has no passwd database, so neither
                    // arm runs on wasm — `command`/`wd` keep the values set above,
                    // exactly as they would when `pw.shell`/`pw.home` are absent. The
                    // reference never analyses this on wasm (Zig analyses
                    // `config.zig.Wasm` lazily); Rust compiles whole crates.
                    #[cfg(not(any(target_os = "windows", target_family = "wasm")))]
                    {
                        // We need the passwd entry for the remainder
                        // const pw = try internal_os.passwd.get(alloc);
                        let pw = rustty_os::passwd::get()
                            .map_err(|e| LoadError::Io(std::io::Error::other(format!("{e:?}"))))?;
                        // if (self.command == null) {
                        if self.command.is_none() {
                            // if (pw.shell) |sh| {
                            if let Some(sh) = &pw.shell {
                                // log.info("default shell src=passwd value={s}", .{sh});
                                log_info(format!(
                                    "default shell src=passwd value={}",
                                    sh.to_string_lossy()
                                ));
                                // self.command = .{ .shell = sh };
                                self.command = Some(Command::Shell(sh.to_bytes().to_vec()));
                            }
                        }

                        // if (wd == .home) {
                        if wd == WorkingDirectory::Home {
                            // if (pw.home) |home| {
                            if let Some(home) = &pw.home {
                                // log.info("default working directory src=passwd value={s}", .{home});
                                log_info(format!(
                                    "default working directory src=passwd value={}",
                                    home.to_string_lossy()
                                ));
                                // wd = .{ .path = home };
                                wd = WorkingDirectory::Path(home.to_bytes().to_vec());
                            } else {
                                // wd = .inherit;
                                wd = WorkingDirectory::Inherit;
                            }
                        }

                        // if (self.command == null) {
                        if self.command.is_none() {
                            // log.warn("no default shell found, will default to using sh", .{});
                            log_warn(
                                "no default shell found, will default to using sh".to_string(),
                            );
                        }
                    }
                }
            }
        }
        // try wd.finalize(alloc);
        wd.finalize();
        // self.@"working-directory" = wd;
        self.working_directory = Some(wd);

        // Apprt-specific defaults
        // switch (build_config.app_runtime) {
        match rustty_build_config::app_runtime() {
            // .none => {},
            rustty_build_config::Runtime::None => {}
            // .gtk => {
            rustty_build_config::Runtime::Gtk => {
                // switch (self.@"gtk-single-instance") {
                match self.gtk_single_instance {
                    // .true, .false => {},
                    GtkSingleInstance::True | GtkSingleInstance::False => {}
                    // For detection, we assume single instance unless we're
                    // in a CLI environment, then we disable single instance.
                    // .detect => self.@"gtk-single-instance" = if (probable_cli)
                    //     .false
                    // else
                    //     .true,
                    GtkSingleInstance::Detect => {
                        self.gtk_single_instance = if probable_cli {
                            GtkSingleInstance::False
                        } else {
                            GtkSingleInstance::True
                        };
                    }
                }
            }
        }

        // Default our click interval
        // if (self.@"click-repeat-interval" == 0 and
        //     (comptime !builtin.is_test))
        // {
        //     self.@"click-repeat-interval" = internal_os.clickInterval() orelse 500;
        // }
        #[cfg(not(test))]
        {
            if self.click_repeat_interval == 0 {
                self.click_repeat_interval = rustty_os::mouse::clickInterval().unwrap_or(500);
            }
        }

        // Clamp our mouse scroll multiplier
        // self.@"mouse-scroll-multiplier".precision = @min(10_000.0, @max(0.01, self.@"mouse-scroll-multiplier".precision));
        self.mouse_scroll_multiplier.precision =
            10_000.0_f64.min(0.01_f64.max(self.mouse_scroll_multiplier.precision));
        // self.@"mouse-scroll-multiplier".discrete = @min(10_000.0, @max(0.01, self.@"mouse-scroll-multiplier".discrete));
        self.mouse_scroll_multiplier.discrete =
            10_000.0_f64.min(0.01_f64.max(self.mouse_scroll_multiplier.discrete));

        // Clamp our split opacity
        // self.@"unfocused-split-opacity" = @min(1.0, @max(0.15, self.@"unfocused-split-opacity"));
        self.unfocused_split_opacity = 1.0_f64.min(0.15_f64.max(self.unfocused_split_opacity));

        // Clamp our contrast
        // self.@"minimum-contrast" = @min(21, @max(1, self.@"minimum-contrast"));
        self.minimum_contrast = 21.0_f64.min(1.0_f64.max(self.minimum_contrast));

        // Minimum window size
        // if (self.@"window-width" > 0) self.@"window-width" = @max(10, self.@"window-width");
        if self.window_width > 0 {
            self.window_width = 10.max(self.window_width);
        }
        // if (self.@"window-height" > 0) self.@"window-height" = @max(4, self.@"window-height");
        if self.window_height > 0 {
            self.window_height = 4.max(self.window_height);
        }

        // If URLs are disabled, cut off the first link. The first link is
        // always the URL matcher.
        // if (!self.@"link-url") self.link.links.items = self.link.links.items[1..];
        if !self.link_url && !self.link.links.is_empty() {
            self.link.links.remove(0);
        }

        // We warn when the quit-after-last-window-closed-delay is set to a very
        // short value because it can cause Rustty to quit before the first
        // window is even shown.
        // if (self.@"quit-after-last-window-closed-delay") |duration| {
        if let Some(duration) = &self.quit_after_last_window_closed_delay {
            // if (duration.duration < 5 * std.time.ns_per_s) {
            if duration.duration < 5 * NS_PER_S {
                // log.warn(
                //     "quit-after-last-window-closed-delay is set to a very short value ({f}), which might cause problems",
                //     .{duration},
                // );
                log_warn(
                    "quit-after-last-window-closed-delay is set to a very short value, which might cause problems"
                        .to_string(),
                );
            }
        }

        // We can't set this as a struct default because our config is
        // loaded in environments where a build config isn't available.
        // if (self.@"auto-update-channel" == null) {
        if self.auto_update_channel.is_none() {
            // self.@"auto-update-channel" = build_config.release_channel;
            self.auto_update_channel = Some(rustty_build_config::release_channel());
        }

        // self.@"faint-opacity" = std.math.clamp(self.@"faint-opacity", 0.0, 1.0);
        self.faint_opacity = self.faint_opacity.clamp(0.0, 1.0);

        // Finalize key remapping set for efficient lookups
        // self.@"key-remap".finalize();
        self.key_remap.finalize();

        Ok(())
    }

    /// PORTS: `pub fn cloneEmpty(self, alloc_gpa) Allocator.Error!Config`.
    ///
    /// Create a copy of the metadata of this configuration but without
    /// the actual values. Metadata includes conditional state.
    // pub fn cloneEmpty(self, alloc_gpa) Allocator.Error!Config {
    //     var result = try default(alloc_gpa);
    //     result._conditional_state = self._conditional_state;
    //     return result;
    // }
    // PARITY: the reference is verbatim `var result = try default(alloc); result.
    // _conditional_state = self._conditional_state; return result;` — a deliberate
    // default-then-override-one-field, so the lint does not apply.
    #[allow(clippy::field_reassign_with_default)]
    pub fn clone_empty(&self) -> Config {
        // var result = try default(alloc_gpa);
        let mut result = Config::default_config();
        // result._conditional_state = self._conditional_state;
        result._conditional_state = self._conditional_state;
        // return result;
        result
    }

    /// PORTS: `pub fn clone(self, alloc_gpa) Allocator.Error!Config`.
    ///
    /// Create a copy of this configuration.
    ///
    /// This will not re-read referenced configuration files and operates
    /// purely in-memory.
    // pub fn clone(self, alloc_gpa) Allocator.Error!Config {
    pub fn clone_config(&self) -> Config {
        // Start with an empty config
        // var result = try self.cloneEmpty(alloc_gpa);
        // errdefer result.deinit();
        // const alloc_arena = result._arena.?.allocator();
        let mut result = self.clone_empty();

        // Copy our values
        //
        // inline for (@typeInfo(Config).@"struct".fields) |field| {
        //     if (!@hasField(Key, field.name)) continue;
        //     @field(result, field.name) = try cloneValue(
        //         alloc_arena, field.type, @field(self, field.name));
        // }
        //
        // PORT NOTE: the reference iterates the Config struct fields, skipping the
        // `_`-prefixed ones (those NOT in `Key`), and deep-copies each via
        // `cloneValue`. Rust has no struct reflection, so the per-field copy is
        // written explicitly via `self.clone_value_into(&mut result)`, which has
        // one arm per `Key` field — the `clone_value` half of the CLONE pass (see
        // `clone_value_into`). In Rust each value type owns its data (no arena),
        // so `cloneValue` collapses to a deep `.clone()` per field, exactly the
        // reference's per-`field.type` `cloneValue` dispatch (scalars/enums/unions
        // copy, owning types deep-clone, strings dupe). The `_`-prefixed fields
        // are NOT copied here (they keep their `clone_empty` defaults); the ones
        // that matter are handled explicitly below, mirroring the reference.
        self.clone_value_into(&mut result);

        // Copy our diagnostics
        // result._diagnostics = try self._diagnostics.clone(alloc_arena);
        result._diagnostics = self._diagnostics.clone_in();

        // Preserve our replay steps. We copy them exactly to also preserve
        // the exact conditionals required for some steps.
        // try result._replay_steps.ensureTotalCapacity(
        //     alloc_arena, self._replay_steps.items.len);
        result._replay_steps.reserve(self._replay_steps.len());
        // for (self._replay_steps.items) |item| {
        //     result._replay_steps.appendAssumeCapacity(try item.clone(alloc_arena));
        // }
        for item in self._replay_steps.iter() {
            result._replay_steps.push(item.clone_in());
        }
        // assert(result._replay_steps.items.len == self._replay_steps.items.len);
        assert_eq!(result._replay_steps.len(), self._replay_steps.len());

        // Copy the conditional set
        // result._conditional_set = self._conditional_set;
        result._conditional_set = self._conditional_set.clone();

        // return result;
        result
    }

    /// PORTS: `pub fn changed(self, new, comptime key: Key) bool` — true if the
    /// value for `key` differs between `self` and `new`.
    ///
    /// Returns true if the given key has changed from old to new. This
    /// requires the key to be comptime known to make this more efficient.
    // pub fn changed(self: *const Config, new: *const Config, comptime key: Key) bool {
    //     // Get the field at comptime
    //     const field = comptime field: {
    //         const fields = std.meta.fields(Config);
    //         for (fields) |field| {
    //             if (@field(Key, field.name) == key) {
    //                 break :field field;
    //             }
    //         }
    //         unreachable;
    //     };
    //     const old_value = @field(self, field.name);
    //     const new_value = @field(new, field.name);
    //     return !deepEqual(field.type, old_value, new_value);
    // }
    //
    // PORT NOTE: the reference's comptime `key` resolves to the matching Config
    // field, then returns `!deepEqual(field.type, old, new)`. Here `key` is a
    // runtime `Key`; the `match` below is the per-`Key` field resolution, each arm
    // returning `!deep_equal(&self.<field>, &new.<field>)` — the EQUAL/DIFF
    // (CHANGED) dispatch. Every field type implements `DeepEqual` (config-owned
    // types in `deep_equal.rs`; the foreign leaf types — `MetricModifier`,
    // `KeyRemapSet`, `CursorStyle`, `WindowPaddingBalance`, `OptionAsAlt`,
    // `ReleaseChannel` — in their home crates per the orphan rule). `Option<T>`
    // fields resolve through the `Option` blanket impl, exactly as the reference's
    // `.optional` arm does.
    pub fn changed(&self, new: &Config, key: Key) -> bool {
        use rustty_datastruct::comparison::deep_equal;

        // `!deepEqual(field.type, old_value, new_value)` for the field named by
        // `key`. Each arm reads `self.$f`/`new.$f` (the `old_value`/`new_value`).
        macro_rules! c {
            ($f:ident) => {
                !deep_equal(&self.$f, &new.$f)
            };
        }

        match key {
            Key::Language => c!(language),
            Key::FontFamily => c!(font_family),
            Key::FontFamilyBold => c!(font_family_bold),
            Key::FontFamilyItalic => c!(font_family_italic),
            Key::FontFamilyBoldItalic => c!(font_family_bold_italic),
            Key::FontStyle => c!(font_style),
            Key::FontStyleBold => c!(font_style_bold),
            Key::FontStyleItalic => c!(font_style_italic),
            Key::FontStyleBoldItalic => c!(font_style_bold_italic),
            Key::FontSyntheticStyle => c!(font_synthetic_style),
            Key::FontFeature => c!(font_feature),
            Key::FontSize => c!(font_size),
            Key::FontVariation => c!(font_variation),
            Key::FontVariationBold => c!(font_variation_bold),
            Key::FontVariationItalic => c!(font_variation_italic),
            Key::FontVariationBoldItalic => c!(font_variation_bold_italic),
            Key::FontCodepointMap => c!(font_codepoint_map),
            Key::ClipboardCodepointMap => c!(clipboard_codepoint_map),
            Key::FontThicken => c!(font_thicken),
            Key::FontThickenStrength => c!(font_thicken_strength),
            Key::FontShapingBreak => c!(font_shaping_break),
            Key::AlphaBlending => c!(alpha_blending),
            Key::AdjustCellWidth => c!(adjust_cell_width),
            Key::AdjustCellHeight => c!(adjust_cell_height),
            Key::AdjustFontBaseline => c!(adjust_font_baseline),
            Key::AdjustUnderlinePosition => c!(adjust_underline_position),
            Key::AdjustUnderlineThickness => c!(adjust_underline_thickness),
            Key::AdjustStrikethroughPosition => c!(adjust_strikethrough_position),
            Key::AdjustStrikethroughThickness => c!(adjust_strikethrough_thickness),
            Key::AdjustOverlinePosition => c!(adjust_overline_position),
            Key::AdjustOverlineThickness => c!(adjust_overline_thickness),
            Key::AdjustCursorThickness => c!(adjust_cursor_thickness),
            Key::AdjustCursorHeight => c!(adjust_cursor_height),
            Key::AdjustBoxThickness => c!(adjust_box_thickness),
            Key::AdjustIconHeight => c!(adjust_icon_height),
            Key::GraphemeWidthMethod => c!(grapheme_width_method),
            Key::FreetypeLoadFlags => c!(freetype_load_flags),
            Key::Theme => c!(theme),
            Key::Background => c!(background),
            Key::Foreground => c!(foreground),
            Key::BackgroundImage => c!(background_image),
            Key::BackgroundImageOpacity => c!(background_image_opacity),
            Key::BackgroundImagePosition => c!(background_image_position),
            Key::BackgroundImageFit => c!(background_image_fit),
            Key::BackgroundImageRepeat => c!(background_image_repeat),
            Key::SelectionForeground => c!(selection_foreground),
            Key::SelectionBackground => c!(selection_background),
            Key::SelectionClearOnTyping => c!(selection_clear_on_typing),
            Key::SelectionClearOnCopy => c!(selection_clear_on_copy),
            Key::SelectionWordChars => c!(selection_word_chars),
            Key::MinimumContrast => c!(minimum_contrast),
            Key::Palette => c!(palette),
            Key::PaletteGenerate => c!(palette_generate),
            Key::PaletteHarmonious => c!(palette_harmonious),
            Key::CursorColor => c!(cursor_color),
            Key::CursorOpacity => c!(cursor_opacity),
            Key::CursorStyle => c!(cursor_style),
            Key::CursorStyleBlink => c!(cursor_style_blink),
            Key::CursorText => c!(cursor_text),
            Key::CursorClickToMove => c!(cursor_click_to_move),
            Key::MouseHideWhileTyping => c!(mouse_hide_while_typing),
            Key::ScrollToBottom => c!(scroll_to_bottom),
            Key::MouseShiftCapture => c!(mouse_shift_capture),
            Key::MouseReporting => c!(mouse_reporting),
            Key::MouseScrollMultiplier => c!(mouse_scroll_multiplier),
            Key::BackgroundOpacity => c!(background_opacity),
            Key::BackgroundOpacityCells => c!(background_opacity_cells),
            Key::BackgroundBlur => c!(background_blur),
            Key::UnfocusedSplitOpacity => c!(unfocused_split_opacity),
            Key::UnfocusedSplitFill => c!(unfocused_split_fill),
            Key::SplitDividerColor => c!(split_divider_color),
            Key::SplitPreserveZoom => c!(split_preserve_zoom),
            Key::SearchForeground => c!(search_foreground),
            Key::SearchBackground => c!(search_background),
            Key::SearchSelectedForeground => c!(search_selected_foreground),
            Key::SearchSelectedBackground => c!(search_selected_background),
            Key::Command => c!(command),
            Key::InitialCommand => c!(initial_command),
            Key::NotifyOnCommandFinish => c!(notify_on_command_finish),
            Key::NotifyOnCommandFinishAction => c!(notify_on_command_finish_action),
            Key::NotifyOnCommandFinishAfter => c!(notify_on_command_finish_after),
            Key::Env => c!(env),
            Key::Input => c!(input),
            Key::WaitAfterCommand => c!(wait_after_command),
            Key::AbnormalCommandExitRuntime => c!(abnormal_command_exit_runtime),
            Key::ScrollbackLimitBytes => c!(scrollback_limit_bytes),
            Key::ScrollbackLimitLines => c!(scrollback_limit_lines),
            Key::ScrollbackCompression => c!(scrollback_compression),
            Key::Scrollbar => c!(scrollbar),
            Key::Link => c!(link),
            Key::LinkUrl => c!(link_url),
            Key::LinkOsc8 => c!(link_osc8),
            Key::LinkPreviews => c!(link_previews),
            Key::Maximize => c!(maximize),
            Key::Fullscreen => c!(fullscreen),
            Key::Title => c!(title),
            Key::Class => c!(class),
            Key::X11InstanceName => c!(x11_instance_name),
            Key::WorkingDirectory => c!(working_directory),
            Key::Keybind => c!(keybind),
            Key::KeyRemap => c!(key_remap),
            Key::WindowPaddingX => c!(window_padding_x),
            Key::WindowPaddingY => c!(window_padding_y),
            Key::WindowPaddingBalance => c!(window_padding_balance),
            Key::WindowPaddingColor => c!(window_padding_color),
            Key::WindowVsync => c!(window_vsync),
            Key::WindowInheritWorkingDirectory => c!(window_inherit_working_directory),
            Key::TabInheritWorkingDirectory => c!(tab_inherit_working_directory),
            Key::SplitInheritWorkingDirectory => c!(split_inherit_working_directory),
            Key::WindowInheritFontSize => c!(window_inherit_font_size),
            Key::WindowDecoration => c!(window_decoration),
            Key::WindowTitleFontFamily => c!(window_title_font_family),
            Key::WindowSubtitle => c!(window_subtitle),
            Key::WindowTheme => c!(window_theme),
            Key::WindowColorspace => c!(window_colorspace),
            Key::WindowHeight => c!(window_height),
            Key::WindowWidth => c!(window_width),
            Key::WindowPositionX => c!(window_position_x),
            Key::WindowPositionY => c!(window_position_y),
            Key::WindowSaveState => c!(window_save_state),
            Key::WindowStepResize => c!(window_step_resize),
            Key::WindowNewTabPosition => c!(window_new_tab_position),
            Key::WindowShowTabBar => c!(window_show_tab_bar),
            Key::WindowTitlebarBackground => c!(window_titlebar_background),
            Key::WindowTitlebarForeground => c!(window_titlebar_foreground),
            Key::DragHandle => c!(drag_handle),
            Key::ResizeOverlay => c!(resize_overlay),
            Key::ResizeOverlayPosition => c!(resize_overlay_position),
            Key::ResizeOverlayDuration => c!(resize_overlay_duration),
            Key::FocusFollowsMouse => c!(focus_follows_mouse),
            Key::ClipboardRead => c!(clipboard_read),
            Key::ClipboardWrite => c!(clipboard_write),
            Key::ClipboardWriteLimitBytes => c!(clipboard_write_limit_bytes),
            Key::ClipboardTrimTrailingSpaces => c!(clipboard_trim_trailing_spaces),
            Key::ClipboardPasteProtection => c!(clipboard_paste_protection),
            Key::ClipboardPasteBracketedSafe => c!(clipboard_paste_bracketed_safe),
            Key::TitleReport => c!(title_report),
            Key::ImageStorageLimit => c!(image_storage_limit),
            Key::CopyOnSelect => c!(copy_on_select),
            Key::RightClickAction => c!(right_click_action),
            Key::MiddleClickAction => c!(middle_click_action),
            Key::ClickRepeatInterval => c!(click_repeat_interval),
            Key::ConfigFile => c!(config_file),
            Key::ConfigDefaultFiles => c!(config_default_files),
            Key::ConfirmCloseSurface => c!(confirm_close_surface),
            Key::QuitAfterLastWindowClosed => c!(quit_after_last_window_closed),
            Key::QuitAfterLastWindowClosedDelay => c!(quit_after_last_window_closed_delay),
            Key::InitialWindow => c!(initial_window),
            Key::UndoTimeout => c!(undo_timeout),
            Key::QuickTerminalPosition => c!(quick_terminal_position),
            Key::QuickTerminalSize => c!(quick_terminal_size),
            Key::GtkQuickTerminalLayer => c!(gtk_quick_terminal_layer),
            Key::GtkQuickTerminalNamespace => c!(gtk_quick_terminal_namespace),
            Key::QuickTerminalScreen => c!(quick_terminal_screen),
            Key::QuickTerminalAnimationDuration => c!(quick_terminal_animation_duration),
            Key::QuickTerminalAutohide => c!(quick_terminal_autohide),
            Key::QuickTerminalSpaceBehavior => c!(quick_terminal_space_behavior),
            Key::QuickTerminalKeyboardInteractivity => c!(quick_terminal_keyboard_interactivity),
            Key::ShellIntegration => c!(shell_integration),
            Key::ShellIntegrationFeatures => c!(shell_integration_features),
            Key::CommandPaletteEntry => c!(command_palette_entry),
            Key::OscColorReportFormat => c!(osc_color_report_format),
            Key::VtKamAllowed => c!(vt_kam_allowed),
            Key::CustomShader => c!(custom_shader),
            Key::CustomShaderAnimation => c!(custom_shader_animation),
            Key::BellFeatures => c!(bell_features),
            Key::BellAudioPath => c!(bell_audio_path),
            Key::BellAudioVolume => c!(bell_audio_volume),
            Key::AppNotifications => c!(app_notifications),
            Key::MacosNonNativeFullscreen => c!(macos_non_native_fullscreen),
            Key::MacosWindowButtons => c!(macos_window_buttons),
            Key::MacosTitlebarStyle => c!(macos_titlebar_style),
            Key::MacosTitlebarProxyIcon => c!(macos_titlebar_proxy_icon),
            Key::MacosDockDropBehavior => c!(macos_dock_drop_behavior),
            Key::MacosOptionAsAlt => c!(macos_option_as_alt),
            Key::MacosWindowShadow => c!(macos_window_shadow),
            Key::MacosHidden => c!(macos_hidden),
            Key::MacosAutoSecureInput => c!(macos_auto_secure_input),
            Key::MacosSecureInputIndication => c!(macos_secure_input_indication),
            Key::MacosApplescript => c!(macos_applescript),
            Key::MacosIcon => c!(macos_icon),
            Key::MacosCustomIcon => c!(macos_custom_icon),
            Key::MacosIconFrame => c!(macos_icon_frame),
            Key::MacosIconGhostColor => c!(macos_icon_ghost_color),
            Key::MacosIconScreenColor => c!(macos_icon_screen_color),
            Key::MacosShortcuts => c!(macos_shortcuts),
            Key::LinuxCgroup => c!(linux_cgroup),
            Key::LinuxCgroupMemoryLimit => c!(linux_cgroup_memory_limit),
            Key::LinuxCgroupProcessesLimit => c!(linux_cgroup_processes_limit),
            Key::LinuxCgroupHardFail => c!(linux_cgroup_hard_fail),
            Key::GtkOpenglDebug => c!(gtk_opengl_debug),
            Key::GtkSingleInstance => c!(gtk_single_instance),
            Key::GtkTitlebar => c!(gtk_titlebar),
            Key::GtkTabsLocation => c!(gtk_tabs_location),
            Key::GtkTitlebarHideWhenMaximized => c!(gtk_titlebar_hide_when_maximized),
            Key::GtkToolbarStyle => c!(gtk_toolbar_style),
            Key::GtkTitlebarStyle => c!(gtk_titlebar_style),
            Key::GtkWideTabs => c!(gtk_wide_tabs),
            Key::GtkHorizontalTabScroll => c!(gtk_horizontal_tab_scroll),
            Key::GtkCustomCss => c!(gtk_custom_css),
            Key::DesktopNotifications => c!(desktop_notifications),
            Key::ProgressStyle => c!(progress_style),
            Key::BoldColor => c!(bold_color),
            Key::FaintOpacity => c!(faint_opacity),
            Key::Term => c!(term),
            Key::EnquiryResponse => c!(enquiry_response),
            Key::AsyncBackend => c!(async_backend),
            Key::AutoUpdate => c!(auto_update),
            Key::AutoUpdateChannel => c!(auto_update_channel),
        }
    }

    /// Deep-copies every `Key` field from `self` into `dest`. This is the Rust
    /// expansion of the reference `clone`'s `inline for (fields) |field|
    /// @field(result, field.name) = try cloneValue(..., @field(self,
    /// field.name))`. There is exactly one arm per `Key` field, which makes the
    /// clone dispatch exhaustive over the field set — the `clone` half of the
    /// per-`Key` exhaustiveness contract (see `key_exhaustive_parse_format_clone_changed`).
    ///
    /// PORT NOTE: in the reference `cloneValue` dispatches on `field.type`:
    ///
    ///   * `[]const u8` / `[:0]const u8` -> `alloc.dupe`/`dupeZ` (deep string copy)
    ///   * a type with a `clone` decl (`@hasDecl(T, "clone")`) -> `src.clone(alloc)`
    ///   * `.bool`/`.int`/`.float`/`.@"enum"`/`.@"union"` -> `return src` (copy)
    ///   * `.optional` -> recurse into the child (or `null`)
    ///   * packed `.@"struct"` -> `return src` (copy)
    ///
    /// Rust has no comptime type dispatch, so each field is written explicitly
    /// here with the arm that matches its reference `cloneValue` resolution:
    ///
    ///   * `cv!` — the copy/`dupe` arms: owned `Vec`/scalar/enum/union/packed
    ///     struct field whose Rust type derives `Clone`; `.clone()` is the deep
    ///     copy (a `Vec<u8>` clone == `dupe`, a `Copy` scalar/enum == `return src`).
    ///   * `ci!` — the `@hasDecl(T, "clone")` arm for a type that does NOT derive
    ///     `Clone` and whose ported clone decl is named `clone_in` (the workspace
    ///     convention): `Keybinds`, `RepeatableClipboardCodepointMap`,
    ///     `RepeatablePath`.
    ///
    /// (Types whose Rust form derives `Clone` — including the `Option`-wrapped
    /// `clone_in` types `Theme`/`Path`/`WorkingDirectory`/`ColorList`/`Duration` —
    /// resolve through `cv!`'s `.clone()` directly, which is the same deep copy.
    /// `?Command` is the one exception: `Command` has an inherent `clone` but no
    /// `Clone` trait, so its `.optional` arm is mapped explicitly below.)
    //
    // PORTS the reference `fn cloneValue(alloc, comptime T, src) !T`, whose body
    // is the comptime type dispatch reproduced field-by-field below:
    //
    //   fn cloneValue(alloc, comptime T, src) Allocator.Error!T {
    //       // Do known named types first
    //       switch (T) {
    //           []const u8 => return try alloc.dupe(u8, src),
    //           [:0]const u8 => return try alloc.dupeZ(u8, src),
    //           else => {},
    //       }
    //
    //       // If we're a type that can have decls and we have clone, then
    //       // call clone and be done.
    //       const t = @typeInfo(T);
    //       if (t == .@"struct" or t == .@"enum" or t == .@"union") {
    //           if (@hasDecl(T, "clone")) return try src.clone(alloc);
    //       }
    //
    //       // Back into types of types
    //       switch (t) {
    //           inline .bool, .int, .float, .@"enum", .@"union" => return src,
    //           .optional => |info| return try cloneValue(
    //               alloc, info.child, src orelse return null),
    //           .@"struct" => |info| {
    //               // Packed structs we can return directly as copies.
    //               assert(info.layout == .@"packed");
    //               return src;
    //           },
    //           else => { @compileLog(T); @compileError("unsupported field type"); },
    //       }
    //   }
    fn clone_value_into(&self, dest: &mut Config) {
        // copy / `dupe` arm (derive-`Clone` deep copy, or inherent `clone`).
        macro_rules! cv {
            ($f:ident) => {
                dest.$f = self.$f.clone()
            };
        }
        // `@hasDecl(T, "clone")` arm: the ported `clone` decl (named `clone_in`).
        macro_rules! ci {
            ($f:ident) => {
                dest.$f = self.$f.clone_in()
            };
        }

        cv!(language);
        cv!(font_family);
        cv!(font_family_bold);
        cv!(font_family_italic);
        cv!(font_family_bold_italic);
        cv!(font_style);
        cv!(font_style_bold);
        cv!(font_style_italic);
        cv!(font_style_bold_italic);
        cv!(font_synthetic_style);
        cv!(font_feature);
        cv!(font_size);
        cv!(font_variation);
        cv!(font_variation_bold);
        cv!(font_variation_italic);
        cv!(font_variation_bold_italic);
        // RepeatableCodepointMap (no derived `Clone`; `@hasDecl(T, "clone")`)
        ci!(font_codepoint_map);
        // RepeatableClipboardCodepointMap (no derived `Clone`)
        ci!(clipboard_codepoint_map);
        cv!(font_thicken);
        cv!(font_thicken_strength);
        cv!(font_shaping_break);
        cv!(alpha_blending);
        cv!(adjust_cell_width);
        cv!(adjust_cell_height);
        cv!(adjust_font_baseline);
        cv!(adjust_underline_position);
        cv!(adjust_underline_thickness);
        cv!(adjust_strikethrough_position);
        cv!(adjust_strikethrough_thickness);
        cv!(adjust_overline_position);
        cv!(adjust_overline_thickness);
        cv!(adjust_cursor_thickness);
        cv!(adjust_cursor_height);
        cv!(adjust_box_thickness);
        cv!(adjust_icon_height);
        cv!(grapheme_width_method);
        cv!(freetype_load_flags);
        cv!(theme);
        cv!(background);
        cv!(foreground);
        cv!(background_image);
        cv!(background_image_opacity);
        cv!(background_image_position);
        cv!(background_image_fit);
        cv!(background_image_repeat);
        cv!(selection_foreground);
        cv!(selection_background);
        cv!(selection_clear_on_typing);
        cv!(selection_clear_on_copy);
        cv!(selection_word_chars);
        cv!(minimum_contrast);
        cv!(palette);
        cv!(palette_generate);
        cv!(palette_harmonious);
        cv!(cursor_color);
        cv!(cursor_opacity);
        cv!(cursor_style);
        cv!(cursor_style_blink);
        cv!(cursor_text);
        cv!(cursor_click_to_move);
        cv!(mouse_hide_while_typing);
        cv!(scroll_to_bottom);
        cv!(mouse_shift_capture);
        cv!(mouse_reporting);
        cv!(mouse_scroll_multiplier);
        cv!(background_opacity);
        cv!(background_opacity_cells);
        cv!(background_blur);
        cv!(unfocused_split_opacity);
        cv!(unfocused_split_fill);
        cv!(split_divider_color);
        cv!(split_preserve_zoom);
        cv!(search_foreground);
        cv!(search_background);
        cv!(search_selected_foreground);
        cv!(search_selected_background);
        // ?Command — Command's ported `clone` is an inherent `clone` (no Clone
        // trait), so the `.optional` arm maps it explicitly.
        dest.command = self.command.as_ref().map(|v| v.clone());
        dest.initial_command = self.initial_command.as_ref().map(|v| v.clone());
        cv!(notify_on_command_finish);
        cv!(notify_on_command_finish_action);
        cv!(notify_on_command_finish_after);
        cv!(env);
        cv!(input);
        cv!(wait_after_command);
        cv!(abnormal_command_exit_runtime);
        cv!(scrollback_limit_bytes);
        cv!(scrollback_limit_lines);
        cv!(scrollback_compression);
        cv!(scrollbar);
        cv!(link);
        cv!(link_url);
        cv!(link_osc8);
        cv!(link_previews);
        cv!(maximize);
        cv!(fullscreen);
        cv!(title);
        cv!(class);
        cv!(x11_instance_name);
        cv!(working_directory);
        // Keybinds (no derived `Clone`)
        ci!(keybind);
        cv!(key_remap);
        cv!(window_padding_x);
        cv!(window_padding_y);
        cv!(window_padding_balance);
        cv!(window_padding_color);
        cv!(window_vsync);
        cv!(window_inherit_working_directory);
        cv!(tab_inherit_working_directory);
        cv!(split_inherit_working_directory);
        cv!(window_inherit_font_size);
        cv!(window_decoration);
        cv!(window_title_font_family);
        cv!(window_subtitle);
        cv!(window_theme);
        cv!(window_colorspace);
        cv!(window_height);
        cv!(window_width);
        cv!(window_position_x);
        cv!(window_position_y);
        cv!(window_save_state);
        cv!(window_step_resize);
        cv!(window_new_tab_position);
        cv!(window_show_tab_bar);
        cv!(window_titlebar_background);
        cv!(window_titlebar_foreground);
        cv!(drag_handle);
        cv!(resize_overlay);
        cv!(resize_overlay_position);
        cv!(resize_overlay_duration);
        cv!(focus_follows_mouse);
        cv!(clipboard_read);
        cv!(clipboard_write);
        cv!(clipboard_write_limit_bytes);
        cv!(clipboard_trim_trailing_spaces);
        cv!(clipboard_paste_protection);
        cv!(clipboard_paste_bracketed_safe);
        cv!(title_report);
        cv!(image_storage_limit);
        cv!(copy_on_select);
        cv!(right_click_action);
        cv!(middle_click_action);
        cv!(click_repeat_interval);
        // RepeatablePath (no derived `Clone`)
        ci!(config_file);
        cv!(config_default_files);
        cv!(confirm_close_surface);
        cv!(quit_after_last_window_closed);
        cv!(quit_after_last_window_closed_delay);
        cv!(initial_window);
        cv!(undo_timeout);
        cv!(quick_terminal_position);
        cv!(quick_terminal_size);
        cv!(gtk_quick_terminal_layer);
        cv!(gtk_quick_terminal_namespace);
        cv!(quick_terminal_screen);
        cv!(quick_terminal_animation_duration);
        cv!(quick_terminal_autohide);
        cv!(quick_terminal_space_behavior);
        cv!(quick_terminal_keyboard_interactivity);
        cv!(shell_integration);
        cv!(shell_integration_features);
        cv!(command_palette_entry);
        cv!(osc_color_report_format);
        cv!(vt_kam_allowed);
        // RepeatablePath (no derived `Clone`)
        ci!(custom_shader);
        cv!(custom_shader_animation);
        cv!(bell_features);
        cv!(bell_audio_path);
        cv!(bell_audio_volume);
        cv!(app_notifications);
        cv!(macos_non_native_fullscreen);
        cv!(macos_window_buttons);
        cv!(macos_titlebar_style);
        cv!(macos_titlebar_proxy_icon);
        cv!(macos_dock_drop_behavior);
        cv!(macos_option_as_alt);
        cv!(macos_window_shadow);
        cv!(macos_hidden);
        cv!(macos_auto_secure_input);
        cv!(macos_secure_input_indication);
        cv!(macos_applescript);
        cv!(macos_icon);
        cv!(macos_custom_icon);
        cv!(macos_icon_frame);
        cv!(macos_icon_ghost_color);
        cv!(macos_icon_screen_color);
        cv!(macos_shortcuts);
        cv!(linux_cgroup);
        cv!(linux_cgroup_memory_limit);
        cv!(linux_cgroup_processes_limit);
        cv!(linux_cgroup_hard_fail);
        cv!(gtk_opengl_debug);
        cv!(gtk_single_instance);
        cv!(gtk_titlebar);
        cv!(gtk_tabs_location);
        cv!(gtk_titlebar_hide_when_maximized);
        cv!(gtk_toolbar_style);
        cv!(gtk_titlebar_style);
        cv!(gtk_wide_tabs);
        cv!(gtk_horizontal_tab_scroll);
        // RepeatablePath (no derived `Clone`)
        ci!(gtk_custom_css);
        cv!(desktop_notifications);
        cv!(progress_style);
        cv!(bold_color);
        cv!(faint_opacity);
        cv!(term);
        cv!(enquiry_response);
        cv!(async_backend);
        cv!(auto_update);
        cv!(auto_update_channel);
    }

    /// PORTS: `pub fn changeIterator(self, new) ChangeIterator`.
    ///
    /// Returns an iterator that goes through each changed field from
    /// old to new. The order of old or new do not matter.
    // pub fn changeIterator(old: *const Config, new: *const Config) ChangeIterator {
    //     return .{ .old = old, .new = new };
    // }
    pub fn change_iterator<'a>(&'a self, new: &'a Config) -> ChangeIterator<'a> {
        // return .{ .old = old, .new = new };
        ChangeIterator {
            old: self,
            new,
            i: 0,
        }
    }
}

/// The ordered `(field.name, Key)` pairs for every non-`_` Config field, in the
/// reference's struct DECLARATION ORDER.
///
/// PORTS: the iteration target of `FileFormatter.format`'s `inline for
/// (@typeInfo(Config).@"struct".fields)` — for each field the reference reads
/// both `field.name` (the formatter key) and `@field(Key, field.name)` (used by
/// the change-tracking branch). Rust has no struct reflection, so this explicit
/// table provides the same ordered name+key sequence. It is a CONTRACT: the set
/// of names equals `parse_tests::CONFIG_FIELD_NAMES` (204) and the set of keys
/// equals the `Key` variants; declaration ORDER (not the sorted parse list) is
/// what `FileFormatter` emits, matching the reference field order.
pub(crate) const CONFIG_FIELDS: &[(&[u8], Key)] = &[
    (b"language", Key::Language),
    (b"font-family", Key::FontFamily),
    (b"font-family-bold", Key::FontFamilyBold),
    (b"font-family-italic", Key::FontFamilyItalic),
    (b"font-family-bold-italic", Key::FontFamilyBoldItalic),
    (b"font-style", Key::FontStyle),
    (b"font-style-bold", Key::FontStyleBold),
    (b"font-style-italic", Key::FontStyleItalic),
    (b"font-style-bold-italic", Key::FontStyleBoldItalic),
    (b"font-synthetic-style", Key::FontSyntheticStyle),
    (b"font-feature", Key::FontFeature),
    (b"font-size", Key::FontSize),
    (b"font-variation", Key::FontVariation),
    (b"font-variation-bold", Key::FontVariationBold),
    (b"font-variation-italic", Key::FontVariationItalic),
    (b"font-variation-bold-italic", Key::FontVariationBoldItalic),
    (b"font-codepoint-map", Key::FontCodepointMap),
    (b"clipboard-codepoint-map", Key::ClipboardCodepointMap),
    (b"font-thicken", Key::FontThicken),
    (b"font-thicken-strength", Key::FontThickenStrength),
    (b"font-shaping-break", Key::FontShapingBreak),
    (b"alpha-blending", Key::AlphaBlending),
    (b"adjust-cell-width", Key::AdjustCellWidth),
    (b"adjust-cell-height", Key::AdjustCellHeight),
    (b"adjust-font-baseline", Key::AdjustFontBaseline),
    (b"adjust-underline-position", Key::AdjustUnderlinePosition),
    (b"adjust-underline-thickness", Key::AdjustUnderlineThickness),
    (
        b"adjust-strikethrough-position",
        Key::AdjustStrikethroughPosition,
    ),
    (
        b"adjust-strikethrough-thickness",
        Key::AdjustStrikethroughThickness,
    ),
    (b"adjust-overline-position", Key::AdjustOverlinePosition),
    (b"adjust-overline-thickness", Key::AdjustOverlineThickness),
    (b"adjust-cursor-thickness", Key::AdjustCursorThickness),
    (b"adjust-cursor-height", Key::AdjustCursorHeight),
    (b"adjust-box-thickness", Key::AdjustBoxThickness),
    (b"adjust-icon-height", Key::AdjustIconHeight),
    (b"grapheme-width-method", Key::GraphemeWidthMethod),
    (b"freetype-load-flags", Key::FreetypeLoadFlags),
    (b"theme", Key::Theme),
    (b"background", Key::Background),
    (b"foreground", Key::Foreground),
    (b"background-image", Key::BackgroundImage),
    (b"background-image-opacity", Key::BackgroundImageOpacity),
    (b"background-image-position", Key::BackgroundImagePosition),
    (b"background-image-fit", Key::BackgroundImageFit),
    (b"background-image-repeat", Key::BackgroundImageRepeat),
    (b"selection-foreground", Key::SelectionForeground),
    (b"selection-background", Key::SelectionBackground),
    (b"selection-clear-on-typing", Key::SelectionClearOnTyping),
    (b"selection-clear-on-copy", Key::SelectionClearOnCopy),
    (b"selection-word-chars", Key::SelectionWordChars),
    (b"minimum-contrast", Key::MinimumContrast),
    (b"palette", Key::Palette),
    (b"palette-generate", Key::PaletteGenerate),
    (b"palette-harmonious", Key::PaletteHarmonious),
    (b"cursor-color", Key::CursorColor),
    (b"cursor-opacity", Key::CursorOpacity),
    (b"cursor-style", Key::CursorStyle),
    (b"cursor-style-blink", Key::CursorStyleBlink),
    (b"cursor-text", Key::CursorText),
    (b"cursor-click-to-move", Key::CursorClickToMove),
    (b"mouse-hide-while-typing", Key::MouseHideWhileTyping),
    (b"scroll-to-bottom", Key::ScrollToBottom),
    (b"mouse-shift-capture", Key::MouseShiftCapture),
    (b"mouse-reporting", Key::MouseReporting),
    (b"mouse-scroll-multiplier", Key::MouseScrollMultiplier),
    (b"background-opacity", Key::BackgroundOpacity),
    (b"background-opacity-cells", Key::BackgroundOpacityCells),
    (b"background-blur", Key::BackgroundBlur),
    (b"unfocused-split-opacity", Key::UnfocusedSplitOpacity),
    (b"unfocused-split-fill", Key::UnfocusedSplitFill),
    (b"split-divider-color", Key::SplitDividerColor),
    (b"split-preserve-zoom", Key::SplitPreserveZoom),
    (b"search-foreground", Key::SearchForeground),
    (b"search-background", Key::SearchBackground),
    (b"search-selected-foreground", Key::SearchSelectedForeground),
    (b"search-selected-background", Key::SearchSelectedBackground),
    (b"command", Key::Command),
    (b"initial-command", Key::InitialCommand),
    (b"notify-on-command-finish", Key::NotifyOnCommandFinish),
    (
        b"notify-on-command-finish-action",
        Key::NotifyOnCommandFinishAction,
    ),
    (
        b"notify-on-command-finish-after",
        Key::NotifyOnCommandFinishAfter,
    ),
    (b"env", Key::Env),
    (b"input", Key::Input),
    (b"wait-after-command", Key::WaitAfterCommand),
    (
        b"abnormal-command-exit-runtime",
        Key::AbnormalCommandExitRuntime,
    ),
    (b"scrollback-limit-bytes", Key::ScrollbackLimitBytes),
    (b"scrollback-limit-lines", Key::ScrollbackLimitLines),
    (b"scrollback-compression", Key::ScrollbackCompression),
    (b"scrollbar", Key::Scrollbar),
    (b"link", Key::Link),
    (b"link-url", Key::LinkUrl),
    (b"link-osc8", Key::LinkOsc8),
    (b"link-previews", Key::LinkPreviews),
    (b"maximize", Key::Maximize),
    (b"fullscreen", Key::Fullscreen),
    (b"title", Key::Title),
    (b"class", Key::Class),
    (b"x11-instance-name", Key::X11InstanceName),
    (b"working-directory", Key::WorkingDirectory),
    (b"keybind", Key::Keybind),
    (b"key-remap", Key::KeyRemap),
    (b"window-padding-x", Key::WindowPaddingX),
    (b"window-padding-y", Key::WindowPaddingY),
    (b"window-padding-balance", Key::WindowPaddingBalance),
    (b"window-padding-color", Key::WindowPaddingColor),
    (b"window-vsync", Key::WindowVsync),
    (
        b"window-inherit-working-directory",
        Key::WindowInheritWorkingDirectory,
    ),
    (
        b"tab-inherit-working-directory",
        Key::TabInheritWorkingDirectory,
    ),
    (
        b"split-inherit-working-directory",
        Key::SplitInheritWorkingDirectory,
    ),
    (b"window-inherit-font-size", Key::WindowInheritFontSize),
    (b"window-decoration", Key::WindowDecoration),
    (b"window-title-font-family", Key::WindowTitleFontFamily),
    (b"window-subtitle", Key::WindowSubtitle),
    (b"window-theme", Key::WindowTheme),
    (b"window-colorspace", Key::WindowColorspace),
    (b"window-height", Key::WindowHeight),
    (b"window-width", Key::WindowWidth),
    (b"window-position-x", Key::WindowPositionX),
    (b"window-position-y", Key::WindowPositionY),
    (b"window-save-state", Key::WindowSaveState),
    (b"window-step-resize", Key::WindowStepResize),
    (b"window-new-tab-position", Key::WindowNewTabPosition),
    (b"window-show-tab-bar", Key::WindowShowTabBar),
    (b"window-titlebar-background", Key::WindowTitlebarBackground),
    (b"window-titlebar-foreground", Key::WindowTitlebarForeground),
    (b"drag-handle", Key::DragHandle),
    (b"resize-overlay", Key::ResizeOverlay),
    (b"resize-overlay-position", Key::ResizeOverlayPosition),
    (b"resize-overlay-duration", Key::ResizeOverlayDuration),
    (b"focus-follows-mouse", Key::FocusFollowsMouse),
    (b"clipboard-read", Key::ClipboardRead),
    (b"clipboard-write", Key::ClipboardWrite),
    (
        b"clipboard-write-limit-bytes",
        Key::ClipboardWriteLimitBytes,
    ),
    (
        b"clipboard-trim-trailing-spaces",
        Key::ClipboardTrimTrailingSpaces,
    ),
    (b"clipboard-paste-protection", Key::ClipboardPasteProtection),
    (
        b"clipboard-paste-bracketed-safe",
        Key::ClipboardPasteBracketedSafe,
    ),
    (b"title-report", Key::TitleReport),
    (b"image-storage-limit", Key::ImageStorageLimit),
    (b"copy-on-select", Key::CopyOnSelect),
    (b"right-click-action", Key::RightClickAction),
    (b"middle-click-action", Key::MiddleClickAction),
    (b"click-repeat-interval", Key::ClickRepeatInterval),
    (b"config-file", Key::ConfigFile),
    (b"config-default-files", Key::ConfigDefaultFiles),
    (b"confirm-close-surface", Key::ConfirmCloseSurface),
    (
        b"quit-after-last-window-closed",
        Key::QuitAfterLastWindowClosed,
    ),
    (
        b"quit-after-last-window-closed-delay",
        Key::QuitAfterLastWindowClosedDelay,
    ),
    (b"initial-window", Key::InitialWindow),
    (b"undo-timeout", Key::UndoTimeout),
    (b"quick-terminal-position", Key::QuickTerminalPosition),
    (b"quick-terminal-size", Key::QuickTerminalSize),
    (b"gtk-quick-terminal-layer", Key::GtkQuickTerminalLayer),
    (
        b"gtk-quick-terminal-namespace",
        Key::GtkQuickTerminalNamespace,
    ),
    (b"quick-terminal-screen", Key::QuickTerminalScreen),
    (
        b"quick-terminal-animation-duration",
        Key::QuickTerminalAnimationDuration,
    ),
    (b"quick-terminal-autohide", Key::QuickTerminalAutohide),
    (
        b"quick-terminal-space-behavior",
        Key::QuickTerminalSpaceBehavior,
    ),
    (
        b"quick-terminal-keyboard-interactivity",
        Key::QuickTerminalKeyboardInteractivity,
    ),
    (b"shell-integration", Key::ShellIntegration),
    (b"shell-integration-features", Key::ShellIntegrationFeatures),
    (b"command-palette-entry", Key::CommandPaletteEntry),
    (b"osc-color-report-format", Key::OscColorReportFormat),
    (b"vt-kam-allowed", Key::VtKamAllowed),
    (b"custom-shader", Key::CustomShader),
    (b"custom-shader-animation", Key::CustomShaderAnimation),
    (b"bell-features", Key::BellFeatures),
    (b"bell-audio-path", Key::BellAudioPath),
    (b"bell-audio-volume", Key::BellAudioVolume),
    (b"app-notifications", Key::AppNotifications),
    (
        b"macos-non-native-fullscreen",
        Key::MacosNonNativeFullscreen,
    ),
    (b"macos-window-buttons", Key::MacosWindowButtons),
    (b"macos-titlebar-style", Key::MacosTitlebarStyle),
    (b"macos-titlebar-proxy-icon", Key::MacosTitlebarProxyIcon),
    (b"macos-dock-drop-behavior", Key::MacosDockDropBehavior),
    (b"macos-option-as-alt", Key::MacosOptionAsAlt),
    (b"macos-window-shadow", Key::MacosWindowShadow),
    (b"macos-hidden", Key::MacosHidden),
    (b"macos-auto-secure-input", Key::MacosAutoSecureInput),
    (
        b"macos-secure-input-indication",
        Key::MacosSecureInputIndication,
    ),
    (b"macos-applescript", Key::MacosApplescript),
    (b"macos-icon", Key::MacosIcon),
    (b"macos-custom-icon", Key::MacosCustomIcon),
    (b"macos-icon-frame", Key::MacosIconFrame),
    (b"macos-icon-ghost-color", Key::MacosIconGhostColor),
    (b"macos-icon-screen-color", Key::MacosIconScreenColor),
    (b"macos-shortcuts", Key::MacosShortcuts),
    (b"linux-cgroup", Key::LinuxCgroup),
    (b"linux-cgroup-memory-limit", Key::LinuxCgroupMemoryLimit),
    (
        b"linux-cgroup-processes-limit",
        Key::LinuxCgroupProcessesLimit,
    ),
    (b"linux-cgroup-hard-fail", Key::LinuxCgroupHardFail),
    (b"gtk-opengl-debug", Key::GtkOpenglDebug),
    (b"gtk-single-instance", Key::GtkSingleInstance),
    (b"gtk-titlebar", Key::GtkTitlebar),
    (b"gtk-tabs-location", Key::GtkTabsLocation),
    (
        b"gtk-titlebar-hide-when-maximized",
        Key::GtkTitlebarHideWhenMaximized,
    ),
    (b"gtk-toolbar-style", Key::GtkToolbarStyle),
    (b"gtk-titlebar-style", Key::GtkTitlebarStyle),
    (b"gtk-wide-tabs", Key::GtkWideTabs),
    (b"gtk-horizontal-tab-scroll", Key::GtkHorizontalTabScroll),
    (b"gtk-custom-css", Key::GtkCustomCss),
    (b"desktop-notifications", Key::DesktopNotifications),
    (b"progress-style", Key::ProgressStyle),
    (b"bold-color", Key::BoldColor),
    (b"faint-opacity", Key::FaintOpacity),
    (b"term", Key::Term),
    (b"enquiry-response", Key::EnquiryResponse),
    (b"async-backend", Key::AsyncBackend),
    (b"auto-update", Key::AutoUpdate),
    (b"auto-update-channel", Key::AutoUpdateChannel),
];

// ===========================================================================
// FORMAT pass — the per-field `formatEntry` dispatch over the Config fields.
//
// PORTS: the body of `formatter.formatEntry(field.type, field.name, value,
// writer)` (formatter.zig:35) applied to EVERY non-`_` Config field, as driven
// by `FileFormatter.format`'s `inline for (@typeInfo(Config).@"struct".fields)`.
//
// In Zig this is one comptime `inline for` that, for each field, calls the
// generic `formatEntry` and lets `switch (@typeInfo(field.type))` pick the arm.
// Rust has no struct reflection, so — exactly as the PARSE `parse_into_field`
// dispatch does — this is an explicit `match key` with one arm per non-`_`
// field, using the field name (the hyphenated Zig spelling) as the pattern. Each
// arm reproduces the SAME `@typeInfo` arm the reference's generic `formatEntry`
// would select for that field's type:
//
//   - `.bool`/`.int`        -> `format_entry_display`
//   - `.float`              -> `format_entry_float`
//   - `.@"enum"`            -> `format_entry_enum(name, value.to_name())` (`{t}`)
//   - `[]const u8`/`[:0]…`  -> `format_entry_string`
//   - `.optional`           -> recurse into the child for `Some`, bare
//                              `name = \n` (`format_entry_void`) for `None`
//   - struct/union with a
//     `formatEntry` decl     -> `value.format_entry(...)` — the type's own
//                              `FormatEntry` impl (or inherent `EntryFormatter`
//                              method for `Keybinds`/`KeyRemapSet`)
//   - packed struct          -> the type's `FormatEntry` impl
//     (`format_entry_packed_struct`)
// ===========================================================================
impl Config {
    /// Format the single field named `key` (the hyphenated Zig field name) to
    /// `writer`. This is the per-field half of the reference's generic
    /// `formatter.formatEntry(field.type, field.name, value, writer)`.
    ///
    /// PORT NOTE: `key` is the field name bytes (e.g. `b"font-size"`), matching
    /// the PARSE dispatch's `parse_into_field` keys; the 204 non-`_` fields each
    /// have exactly one arm. An unknown key is unreachable in the
    /// `FileFormatter` loop (it iterates the real field set), so it is a bug
    /// rather than a recoverable case — `debug_assert`-guarded and a no-op.
    pub(crate) fn format_entry_field<W: Writer>(
        &self,
        key: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        // `.bool` / `.int` arm: `writer.print("{s} = {}\n", .{ name, value })`.
        macro_rules! f_display {
            ($f:ident) => {
                format_entry_display(key, &self.$f, writer)
            };
        }
        // `.float` arm: `writer.print("{s} = {d}\n", .{ name, value })`.
        macro_rules! f_float {
            ($f:ident) => {
                format_entry_float(key, &self.$f, writer)
            };
        }
        // `[]const u8` / `[:0]const u8` arm: `writer.print("{s} = {s}\n", ...)`.
        macro_rules! f_string {
            ($f:ident) => {
                format_entry_string(key, &self.$f, writer)
            };
        }
        // `.@"enum"` arm: `writer.print("{s} = {t}\n", ...)` via the type's
        // `to_name` (== `@tagName`).
        macro_rules! f_enum {
            ($f:ident) => {
                format_entry_enum(key, self.$f.to_name(), writer)
            };
            ($f:ident, $name_fn:expr) => {
                format_entry_enum(key, $name_fn(self.$f), writer)
            };
        }
        // Struct/union with a `formatEntry` decl (the type's `FormatEntry` impl):
        // `value.formatEntry(entryFormatter(name, writer))`.
        macro_rules! f_entry {
            ($f:ident) => {
                FormatEntry::format_entry(&self.$f, key, writer)
            };
            // 2-arg arm for a Chunk-3a-hoisted type whose `FormatEntry` impl would
            // orphan (foreign cfg-fmt trait on a foreign render-types type): dispatch to
            // the config-local free fn instead.
            ($f:ident, $fmt_fn:expr) => {
                $fmt_fn(self.$f, key, writer)
            };
        }
        // `.optional` arm for a child whose type prints via `{}` (`.bool`/`.int`):
        // `Some` -> `format_entry_display`; `None` -> `name = \n`.
        macro_rules! f_opt_display {
            ($f:ident) => {
                match &self.$f {
                    Some(v) => format_entry_display(key, v, writer),
                    None => format_entry_void(key, writer),
                }
            };
        }
        // `.optional` arm for a child string (`?[:0]const u8` / `?[]const u8`).
        macro_rules! f_opt_string {
            ($f:ident) => {
                match &self.$f {
                    Some(v) => format_entry_string(key, v, writer),
                    None => format_entry_void(key, writer),
                }
            };
        }
        // `.optional` arm for a child bare enum (`?Enum`), via the type's `to_name`.
        macro_rules! f_opt_enum {
            ($f:ident) => {
                match &self.$f {
                    Some(v) => format_entry_enum(key, v.to_name(), writer),
                    None => format_entry_void(key, writer),
                }
            };
            ($f:ident, $name_fn:expr) => {
                match self.$f {
                    Some(v) => format_entry_enum(key, $name_fn(v), writer),
                    None => format_entry_void(key, writer),
                }
            };
        }
        // `.optional` arm for a child whose type has its own `FormatEntry` impl.
        macro_rules! f_opt_entry {
            ($f:ident) => {
                format_entry_optional(key, self.$f.as_ref(), writer)
            };
        }
        // `?Metrics.Modifier`: the child `Modifier.formatEntry` builds the value
        // string into a stack buffer then calls `formatter.formatEntry([]const
        // u8, bytes)` (the `[]const u8` arm). `None` prints the bare line.
        macro_rules! f_opt_metric_modifier {
            ($f:ident) => {
                match self.$f {
                    Some(m) => {
                        // var buf: [1024]u8 = undefined; (the reference's stack buffer)
                        let mut buf = [0u8; 1024];
                        // Modifier.formatEntry maps `error.OutOfMemory` from the
                        // fixed buffer; 1024 bytes is the reference's size and the
                        // values never overflow it.
                        let bytes = m
                            .format_entry(&mut buf)
                            .expect("metric modifier fits in 1024-byte buffer");
                        format_entry_string(key, bytes, writer)
                    }
                    None => format_entry_void(key, writer),
                }
            };
        }
        // Struct/union whose `formatEntry` decl takes the `EntryFormatter`
        // directly (the inherent `format_entry(&mut EntryFormatter)` form used by
        // `Keybinds` and `KeyRemapSet`/`RemapSet`):
        // `value.formatEntry(entryFormatter(name, writer))`.
        macro_rules! f_entry_formatter {
            ($f:ident) => {{
                let mut ef = entry_formatter(key, writer);
                self.$f.format_entry(&mut ef)
            }};
        }

        match key {
            // language: ?[:0]const u8
            b"language" => f_opt_string!(language),

            // @"font-family": RepeatableString
            b"font-family" => f_entry!(font_family),
            b"font-family-bold" => f_entry!(font_family_bold),
            b"font-family-italic" => f_entry!(font_family_italic),
            b"font-family-bold-italic" => f_entry!(font_family_bold_italic),

            // @"font-style": FontStyle (union, FormatEntry)
            b"font-style" => f_entry!(font_style),
            b"font-style-bold" => f_entry!(font_style_bold),
            b"font-style-italic" => f_entry!(font_style_italic),
            b"font-style-bold-italic" => f_entry!(font_style_bold_italic),

            // @"font-synthetic-style": FontSyntheticStyle (packed struct)
            b"font-synthetic-style" => f_entry!(font_synthetic_style),

            // @"font-feature": RepeatableString
            b"font-feature" => f_entry!(font_feature),

            // @"font-size": f32
            b"font-size" => f_float!(font_size),

            // @"font-variation": RepeatableFontVariation
            b"font-variation" => f_entry!(font_variation),
            b"font-variation-bold" => f_entry!(font_variation_bold),
            b"font-variation-italic" => f_entry!(font_variation_italic),
            b"font-variation-bold-italic" => f_entry!(font_variation_bold_italic),

            // @"font-codepoint-map": RepeatableCodepointMap
            //
            // PORT NOTE (DEFERRED SEAM, R4 — see DEFERRALS.md): the value type
            // `RepeatableCodepointMap` wraps `fontpkg.CodepointMap`, which is not
            // yet ported in rustty-font. Its `formatEntry` operates on that font
            // map's `list`/`descriptor` API and is deferred — a clearly surfaced
            // deferral (NOT a stub-to-pass) until `font/CodepointMap.zig` lands.
            b"font-codepoint-map" => self.font_codepoint_map.format_entry(key, writer),

            // @"clipboard-codepoint-map": RepeatableClipboardCodepointMap (FormatEntry)
            b"clipboard-codepoint-map" => f_entry!(clipboard_codepoint_map),

            // @"font-thicken": bool
            b"font-thicken" => f_display!(font_thicken),
            // @"font-thicken-strength": u8
            b"font-thicken-strength" => f_display!(font_thicken_strength),

            // @"font-shaping-break": FontShapingBreak (packed struct)
            b"font-shaping-break" => f_entry!(font_shaping_break, font_shaping_break_format_entry),

            // @"alpha-blending": AlphaBlending (bare enum, external)
            b"alpha-blending" => f_enum!(alpha_blending, alpha_blending_to_name),

            // @"adjust-*": ?MetricModifier
            b"adjust-cell-width" => f_opt_metric_modifier!(adjust_cell_width),
            b"adjust-cell-height" => f_opt_metric_modifier!(adjust_cell_height),
            b"adjust-font-baseline" => f_opt_metric_modifier!(adjust_font_baseline),
            b"adjust-underline-position" => f_opt_metric_modifier!(adjust_underline_position),
            b"adjust-underline-thickness" => f_opt_metric_modifier!(adjust_underline_thickness),
            b"adjust-strikethrough-position" => {
                f_opt_metric_modifier!(adjust_strikethrough_position)
            }
            b"adjust-strikethrough-thickness" => {
                f_opt_metric_modifier!(adjust_strikethrough_thickness)
            }
            b"adjust-overline-position" => f_opt_metric_modifier!(adjust_overline_position),
            b"adjust-overline-thickness" => f_opt_metric_modifier!(adjust_overline_thickness),
            b"adjust-cursor-thickness" => f_opt_metric_modifier!(adjust_cursor_thickness),
            b"adjust-cursor-height" => f_opt_metric_modifier!(adjust_cursor_height),
            b"adjust-box-thickness" => f_opt_metric_modifier!(adjust_box_thickness),
            b"adjust-icon-height" => f_opt_metric_modifier!(adjust_icon_height),

            // @"grapheme-width-method": GraphemeWidthMethod (bare enum)
            b"grapheme-width-method" => f_enum!(grapheme_width_method),

            // @"freetype-load-flags": FreetypeLoadFlags (packed struct)
            b"freetype-load-flags" => f_entry!(freetype_load_flags),

            // theme: ?Theme (FormatEntry, optional)
            b"theme" => f_opt_entry!(theme),

            // background: Color (FormatEntry)
            b"background" => f_entry!(background),
            // foreground: Color (FormatEntry)
            b"foreground" => f_entry!(foreground),

            // @"background-image": ?Path (FormatEntry, optional)
            b"background-image" => f_opt_entry!(background_image),
            // @"background-image-opacity": f32
            b"background-image-opacity" => f_float!(background_image_opacity),
            // @"background-image-position": BackgroundImagePosition (bare enum, external)
            b"background-image-position" => {
                f_enum!(background_image_position, background_image_position_to_name)
            }
            // @"background-image-fit": BackgroundImageFit (bare enum, external)
            b"background-image-fit" => f_enum!(background_image_fit, background_image_fit_to_name),
            // @"background-image-repeat": bool
            b"background-image-repeat" => f_display!(background_image_repeat),

            // @"selection-foreground": ?TerminalColor (FormatEntry, optional)
            b"selection-foreground" => f_opt_entry!(selection_foreground),
            b"selection-background" => f_opt_entry!(selection_background),
            // @"selection-clear-on-typing": bool
            b"selection-clear-on-typing" => f_display!(selection_clear_on_typing),
            // @"selection-clear-on-copy": bool
            b"selection-clear-on-copy" => f_display!(selection_clear_on_copy),
            // @"selection-word-chars": SelectionWordChars (FormatEntry)
            b"selection-word-chars" => f_entry!(selection_word_chars),

            // @"minimum-contrast": f64
            b"minimum-contrast" => f_float!(minimum_contrast),

            // palette: Palette (FormatEntry)
            b"palette" => f_entry!(palette),
            // @"palette-generate": bool
            b"palette-generate" => f_display!(palette_generate),
            // @"palette-harmonious": bool
            b"palette-harmonious" => f_display!(palette_harmonious),

            // @"cursor-color": ?TerminalColor (FormatEntry, optional)
            b"cursor-color" => f_opt_entry!(cursor_color),
            // @"cursor-opacity": f64
            b"cursor-opacity" => f_float!(cursor_opacity),
            // @"cursor-style": terminal.CursorStyle (bare enum, external)
            b"cursor-style" => f_enum!(cursor_style, cursor_style_to_name),
            // @"cursor-style-blink": ?bool
            b"cursor-style-blink" => f_opt_display!(cursor_style_blink),
            // @"cursor-text": ?TerminalColor (FormatEntry, optional)
            b"cursor-text" => f_opt_entry!(cursor_text),
            // @"cursor-click-to-move": bool
            b"cursor-click-to-move" => f_display!(cursor_click_to_move),

            // @"mouse-hide-while-typing": bool
            b"mouse-hide-while-typing" => f_display!(mouse_hide_while_typing),
            // @"scroll-to-bottom": ScrollToBottom (packed struct)
            b"scroll-to-bottom" => f_entry!(scroll_to_bottom),
            // @"mouse-shift-capture": MouseShiftCapture (bare enum)
            b"mouse-shift-capture" => f_enum!(mouse_shift_capture),
            // @"mouse-reporting": bool
            b"mouse-reporting" => f_display!(mouse_reporting),
            // @"mouse-scroll-multiplier": MouseScrollMultiplier (FormatEntry)
            b"mouse-scroll-multiplier" => f_entry!(mouse_scroll_multiplier),

            // @"background-opacity": f64
            b"background-opacity" => f_float!(background_opacity),
            // @"background-opacity-cells": bool
            b"background-opacity-cells" => f_display!(background_opacity_cells),
            // @"background-blur": BackgroundBlur (union, FormatEntry)
            b"background-blur" => f_entry!(background_blur, background_blur_format_entry),

            // @"unfocused-split-opacity": f64
            b"unfocused-split-opacity" => f_float!(unfocused_split_opacity),
            // @"unfocused-split-fill": ?Color (FormatEntry, optional)
            b"unfocused-split-fill" => f_opt_entry!(unfocused_split_fill),
            // @"split-divider-color": ?Color (FormatEntry, optional)
            b"split-divider-color" => f_opt_entry!(split_divider_color),
            // @"split-preserve-zoom": SplitPreserveZoom (packed struct)
            b"split-preserve-zoom" => f_entry!(split_preserve_zoom),

            // @"search-foreground": TerminalColor (FormatEntry)
            b"search-foreground" => f_entry!(search_foreground),
            b"search-background" => f_entry!(search_background),
            b"search-selected-foreground" => f_entry!(search_selected_foreground),
            b"search-selected-background" => f_entry!(search_selected_background),

            // command: ?Command (FormatEntry, optional)
            b"command" => f_opt_entry!(command),
            // @"initial-command": ?Command (FormatEntry, optional)
            b"initial-command" => f_opt_entry!(initial_command),

            // @"notify-on-command-finish": NotifyOnCommandFinish (bare enum)
            b"notify-on-command-finish" => f_enum!(notify_on_command_finish),
            // @"notify-on-command-finish-action": NotifyOnCommandFinishAction (packed struct)
            b"notify-on-command-finish-action" => f_entry!(notify_on_command_finish_action),
            // @"notify-on-command-finish-after": Duration (FormatEntry)
            b"notify-on-command-finish-after" => f_entry!(notify_on_command_finish_after),

            // env: RepeatableStringMap (FormatEntry)
            b"env" => f_entry!(env),
            // input: RepeatableReadableIO (FormatEntry)
            b"input" => f_entry!(input),
            // @"wait-after-command": bool
            b"wait-after-command" => f_display!(wait_after_command),
            // @"abnormal-command-exit-runtime": u32
            b"abnormal-command-exit-runtime" => f_display!(abnormal_command_exit_runtime),
            // @"scrollback-limit-bytes": Limit(usize, 50_000_000)
            b"scrollback-limit-bytes" => f_entry!(scrollback_limit_bytes),
            // @"scrollback-limit-lines": Limit(usize, maxInt(usize))
            b"scrollback-limit-lines" => f_entry!(scrollback_limit_lines),
            // @"scrollback-compression": bool
            b"scrollback-compression" => f_display!(scrollback_compression),
            // scrollbar: Scrollbar (bare enum)
            b"scrollbar" => f_enum!(scrollbar),

            // link: RepeatableLink (FormatEntry)
            b"link" => f_entry!(link),
            // @"link-url": bool
            b"link-url" => f_display!(link_url),
            // @"link-osc8": bool
            b"link-osc8" => f_display!(link_osc8),
            // @"link-previews": LinkPreviews (bare enum)
            b"link-previews" => f_enum!(link_previews),

            // maximize: bool
            b"maximize" => f_display!(maximize),
            // fullscreen: Fullscreen (bare enum)
            b"fullscreen" => f_enum!(fullscreen),
            // title: ?[:0]const u8
            b"title" => f_opt_string!(title),
            // class: ?[:0]const u8
            b"class" => f_opt_string!(class),
            // @"x11-instance-name": ?[:0]const u8
            b"x11-instance-name" => f_opt_string!(x11_instance_name),

            // @"working-directory": ?WorkingDirectory (FormatEntry, optional)
            b"working-directory" => f_opt_entry!(working_directory),

            // keybind: Keybinds (inherent `formatEntry` taking the EntryFormatter)
            b"keybind" => f_entry_formatter!(keybind),
            // @"key-remap": KeyRemapSet (inherent `formatEntry` taking the EntryFormatter)
            b"key-remap" => f_entry_formatter!(key_remap),

            // @"window-padding-x": WindowPadding (FormatEntry)
            b"window-padding-x" => f_entry!(window_padding_x),
            // @"window-padding-y": WindowPadding (FormatEntry)
            b"window-padding-y" => f_entry!(window_padding_y),
            // @"window-padding-balance": WindowPaddingBalance (bare enum, external)
            b"window-padding-balance" => {
                f_enum!(window_padding_balance, window_padding_balance_to_name)
            }
            // @"window-padding-color": WindowPaddingColor (bare enum, external)
            b"window-padding-color" => f_enum!(window_padding_color, window_padding_color_to_name),
            // @"window-vsync": bool
            b"window-vsync" => f_display!(window_vsync),
            // @"window-inherit-working-directory": bool
            b"window-inherit-working-directory" => f_display!(window_inherit_working_directory),
            // @"tab-inherit-working-directory": bool
            b"tab-inherit-working-directory" => f_display!(tab_inherit_working_directory),
            // @"split-inherit-working-directory": bool
            b"split-inherit-working-directory" => f_display!(split_inherit_working_directory),
            // @"window-inherit-font-size": bool
            b"window-inherit-font-size" => f_display!(window_inherit_font_size),
            // @"window-decoration": WindowDecoration (bare enum, custom parseCLI)
            b"window-decoration" => f_enum!(window_decoration),
            // @"window-title-font-family": ?[:0]const u8
            b"window-title-font-family" => f_opt_string!(window_title_font_family),
            // @"window-subtitle": WindowSubtitle (bare enum)
            b"window-subtitle" => f_enum!(window_subtitle),
            // @"window-theme": WindowTheme (bare enum)
            b"window-theme" => f_enum!(window_theme),
            // @"window-colorspace": WindowColorspace (bare enum, external)
            b"window-colorspace" => f_enum!(window_colorspace, window_colorspace_to_name),
            // @"window-height": u32
            b"window-height" => f_display!(window_height),
            // @"window-width": u32
            b"window-width" => f_display!(window_width),
            // @"window-position-x": ?i16
            b"window-position-x" => f_opt_display!(window_position_x),
            // @"window-position-y": ?i16
            b"window-position-y" => f_opt_display!(window_position_y),
            // @"window-save-state": WindowSaveState (bare enum)
            b"window-save-state" => f_enum!(window_save_state),
            // @"window-step-resize": bool
            b"window-step-resize" => f_display!(window_step_resize),
            // @"window-new-tab-position": WindowNewTabPosition (bare enum)
            b"window-new-tab-position" => f_enum!(window_new_tab_position),
            // @"window-show-tab-bar": WindowShowTabBar (bare enum)
            b"window-show-tab-bar" => f_enum!(window_show_tab_bar),
            // @"window-titlebar-background": ?Color (FormatEntry, optional)
            b"window-titlebar-background" => f_opt_entry!(window_titlebar_background),
            // @"window-titlebar-foreground": ?Color (FormatEntry, optional)
            b"window-titlebar-foreground" => f_opt_entry!(window_titlebar_foreground),

            // @"drag-handle": DragHandle (bare enum)
            b"drag-handle" => f_enum!(drag_handle),

            // @"resize-overlay": ResizeOverlay (bare enum)
            b"resize-overlay" => f_enum!(resize_overlay),
            // @"resize-overlay-position": ResizeOverlayPosition (bare enum)
            b"resize-overlay-position" => f_enum!(resize_overlay_position),
            // @"resize-overlay-duration": Duration (FormatEntry)
            b"resize-overlay-duration" => f_entry!(resize_overlay_duration),

            // @"focus-follows-mouse": bool
            b"focus-follows-mouse" => f_display!(focus_follows_mouse),

            // @"clipboard-read": ClipboardAccess (bare enum)
            b"clipboard-read" => f_enum!(clipboard_read),
            // @"clipboard-write": ClipboardAccess (bare enum)
            b"clipboard-write" => f_enum!(clipboard_write),
            // @"clipboard-write-limit-bytes": Limit(usize, 64 * 1024 * 1024)
            b"clipboard-write-limit-bytes" => f_entry!(clipboard_write_limit_bytes),
            // @"clipboard-trim-trailing-spaces": bool
            b"clipboard-trim-trailing-spaces" => f_display!(clipboard_trim_trailing_spaces),
            // @"clipboard-paste-protection": bool
            b"clipboard-paste-protection" => f_display!(clipboard_paste_protection),
            // @"clipboard-paste-bracketed-safe": bool
            b"clipboard-paste-bracketed-safe" => f_display!(clipboard_paste_bracketed_safe),

            // @"title-report": bool
            b"title-report" => f_display!(title_report),
            // @"image-storage-limit": u32
            b"image-storage-limit" => f_display!(image_storage_limit),

            // @"copy-on-select": CopyOnSelect (bare enum)
            b"copy-on-select" => f_enum!(copy_on_select),

            // @"right-click-action": RightClickAction (bare enum)
            b"right-click-action" => f_enum!(right_click_action),
            // @"middle-click-action": MiddleClickAction (bare enum)
            b"middle-click-action" => f_enum!(middle_click_action),
            // @"click-repeat-interval": u32
            b"click-repeat-interval" => f_display!(click_repeat_interval),

            // @"config-file": RepeatablePath (FormatEntry)
            b"config-file" => f_entry!(config_file),
            // @"config-default-files": bool
            b"config-default-files" => f_display!(config_default_files),

            // @"confirm-close-surface": ConfirmCloseSurface (bare enum)
            b"confirm-close-surface" => f_enum!(confirm_close_surface),
            // @"quit-after-last-window-closed": bool
            b"quit-after-last-window-closed" => f_display!(quit_after_last_window_closed),
            // @"quit-after-last-window-closed-delay": ?Duration (FormatEntry, optional)
            b"quit-after-last-window-closed-delay" => {
                f_opt_entry!(quit_after_last_window_closed_delay)
            }
            // @"initial-window": bool
            b"initial-window" => f_display!(initial_window),

            // @"undo-timeout": Duration (FormatEntry)
            b"undo-timeout" => f_entry!(undo_timeout),

            // @"quick-terminal-position": QuickTerminalPosition (bare enum)
            b"quick-terminal-position" => f_enum!(quick_terminal_position),
            // @"quick-terminal-size": QuickTerminalSize (FormatEntry)
            b"quick-terminal-size" => f_entry!(quick_terminal_size),
            // @"gtk-quick-terminal-layer": QuickTerminalLayer (bare enum)
            b"gtk-quick-terminal-layer" => f_enum!(gtk_quick_terminal_layer),
            // @"gtk-quick-terminal-namespace": [:0]const u8
            b"gtk-quick-terminal-namespace" => f_string!(gtk_quick_terminal_namespace),
            // @"quick-terminal-screen": QuickTerminalScreen (bare enum)
            b"quick-terminal-screen" => f_enum!(quick_terminal_screen),
            // @"quick-terminal-animation-duration": f64
            b"quick-terminal-animation-duration" => f_float!(quick_terminal_animation_duration),
            // @"quick-terminal-autohide": bool
            b"quick-terminal-autohide" => f_display!(quick_terminal_autohide),
            // @"quick-terminal-space-behavior": QuickTerminalSpaceBehavior (bare enum)
            b"quick-terminal-space-behavior" => f_enum!(quick_terminal_space_behavior),
            // @"quick-terminal-keyboard-interactivity": QuickTerminalKeyboardInteractivity (bare enum)
            b"quick-terminal-keyboard-interactivity" => {
                f_enum!(quick_terminal_keyboard_interactivity)
            }

            // @"shell-integration": ShellIntegration (bare enum)
            b"shell-integration" => f_enum!(shell_integration),
            // @"shell-integration-features": ShellIntegrationFeatures (packed struct)
            b"shell-integration-features" => f_entry!(shell_integration_features),

            // @"command-palette-entry": RepeatableCommand (FormatEntry)
            b"command-palette-entry" => f_entry!(command_palette_entry),

            // @"osc-color-report-format": OSCColorReportFormat (bare enum)
            b"osc-color-report-format" => f_enum!(osc_color_report_format),
            // @"vt-kam-allowed": bool
            b"vt-kam-allowed" => f_display!(vt_kam_allowed),

            // @"custom-shader": RepeatablePath (FormatEntry)
            b"custom-shader" => f_entry!(custom_shader),
            // @"custom-shader-animation": CustomShaderAnimation (bare enum)
            b"custom-shader-animation" => f_enum!(custom_shader_animation),

            // @"bell-features": BellFeatures (packed struct)
            b"bell-features" => f_entry!(bell_features),
            // @"bell-audio-path": ?Path (FormatEntry, optional)
            b"bell-audio-path" => f_opt_entry!(bell_audio_path),
            // @"bell-audio-volume": f64
            b"bell-audio-volume" => f_float!(bell_audio_volume),

            // @"app-notifications": AppNotifications (packed struct)
            b"app-notifications" => f_entry!(app_notifications),

            // @"macos-non-native-fullscreen": NonNativeFullscreen (bare enum)
            b"macos-non-native-fullscreen" => f_enum!(macos_non_native_fullscreen),
            // @"macos-window-buttons": MacWindowButtons (bare enum)
            b"macos-window-buttons" => f_enum!(macos_window_buttons),
            // @"macos-titlebar-style": MacTitlebarStyle (bare enum)
            b"macos-titlebar-style" => f_enum!(macos_titlebar_style),
            // @"macos-titlebar-proxy-icon": MacTitlebarProxyIcon (bare enum)
            b"macos-titlebar-proxy-icon" => f_enum!(macos_titlebar_proxy_icon),
            // @"macos-dock-drop-behavior": MacOSDockDropBehavior (bare enum)
            b"macos-dock-drop-behavior" => f_enum!(macos_dock_drop_behavior),
            // @"macos-option-as-alt": ?inputpkg.OptionAsAlt (bare enum, external, optional)
            b"macos-option-as-alt" => f_opt_enum!(macos_option_as_alt, option_as_alt_to_name),
            // @"macos-window-shadow": bool
            b"macos-window-shadow" => f_display!(macos_window_shadow),
            // @"macos-hidden": MacHidden (bare enum)
            b"macos-hidden" => f_enum!(macos_hidden),
            // @"macos-auto-secure-input": bool
            b"macos-auto-secure-input" => f_display!(macos_auto_secure_input),
            // @"macos-secure-input-indication": bool
            b"macos-secure-input-indication" => f_display!(macos_secure_input_indication),
            // @"macos-applescript": bool
            b"macos-applescript" => f_display!(macos_applescript),
            // @"macos-icon": MacAppIcon (bare enum)
            b"macos-icon" => f_enum!(macos_icon),
            // @"macos-custom-icon": ?[:0]const u8
            b"macos-custom-icon" => f_opt_string!(macos_custom_icon),
            // @"macos-icon-frame": MacAppIconFrame (bare enum)
            b"macos-icon-frame" => f_enum!(macos_icon_frame),
            // @"macos-icon-ghost-color": ?Color (FormatEntry, optional)
            b"macos-icon-ghost-color" => f_opt_entry!(macos_icon_ghost_color),
            // @"macos-icon-screen-color": ?ColorList (FormatEntry, optional)
            b"macos-icon-screen-color" => f_opt_entry!(macos_icon_screen_color),
            // @"macos-shortcuts": MacShortcuts (bare enum)
            b"macos-shortcuts" => f_enum!(macos_shortcuts),

            // @"linux-cgroup": LinuxCgroup (bare enum)
            b"linux-cgroup" => f_enum!(linux_cgroup),
            // @"linux-cgroup-memory-limit": ?u64
            b"linux-cgroup-memory-limit" => f_opt_display!(linux_cgroup_memory_limit),
            // @"linux-cgroup-processes-limit": ?u64
            b"linux-cgroup-processes-limit" => f_opt_display!(linux_cgroup_processes_limit),
            // @"linux-cgroup-hard-fail": bool
            b"linux-cgroup-hard-fail" => f_display!(linux_cgroup_hard_fail),

            // @"gtk-opengl-debug": bool
            b"gtk-opengl-debug" => f_display!(gtk_opengl_debug),
            // @"gtk-single-instance": GtkSingleInstance (bare enum)
            b"gtk-single-instance" => f_enum!(gtk_single_instance),
            // @"gtk-titlebar": bool
            b"gtk-titlebar" => f_display!(gtk_titlebar),
            // @"gtk-tabs-location": GtkTabsLocation (bare enum)
            b"gtk-tabs-location" => f_enum!(gtk_tabs_location),
            // @"gtk-titlebar-hide-when-maximized": bool
            b"gtk-titlebar-hide-when-maximized" => f_display!(gtk_titlebar_hide_when_maximized),
            // @"gtk-toolbar-style": GtkToolbarStyle (bare enum)
            b"gtk-toolbar-style" => f_enum!(gtk_toolbar_style),
            // @"gtk-titlebar-style": GtkTitlebarStyle (bare enum)
            b"gtk-titlebar-style" => f_enum!(gtk_titlebar_style),
            // @"gtk-wide-tabs": bool
            b"gtk-wide-tabs" => f_display!(gtk_wide_tabs),
            // @"gtk-horizontal-tab-scroll": bool
            b"gtk-horizontal-tab-scroll" => f_display!(gtk_horizontal_tab_scroll),
            // @"gtk-custom-css": RepeatablePath (FormatEntry)
            b"gtk-custom-css" => f_entry!(gtk_custom_css),

            // @"desktop-notifications": bool
            b"desktop-notifications" => f_display!(desktop_notifications),
            // @"progress-style": bool
            b"progress-style" => f_display!(progress_style),

            // @"bold-color": ?BoldColor (FormatEntry, optional)
            b"bold-color" => f_opt_entry!(bold_color),
            // @"faint-opacity": f64
            b"faint-opacity" => f_float!(faint_opacity),

            // term: []const u8
            b"term" => f_string!(term),
            // @"enquiry-response": []const u8
            b"enquiry-response" => f_string!(enquiry_response),

            // @"async-backend": AsyncBackend (bare enum)
            b"async-backend" => f_enum!(async_backend),
            // @"auto-update": ?AutoUpdate (bare enum, optional)
            b"auto-update" => f_opt_enum!(auto_update),
            // @"auto-update-channel": ?build_config.ReleaseChannel (bare enum, external, optional)
            b"auto-update-channel" => f_opt_enum!(auto_update_channel, release_channel_to_name),

            // No non-underscore field matched. Unreachable from `FileFormatter`.
            //
            // RUST CONTRACT (VALIDATION_REPORT §3d): the reference dispatch is a
            // comptime `inline for` over `@typeInfo(Config)` fields — an unknown
            // key CANNOT EXIST at runtime (it is a compile error). This arm is
            // Rust glue for that comptime-impossible case, so it panics
            // unconditionally in EVERY profile; a previous draft's
            // `debug_assert!(false)` + `Ok(())` silently succeeded in release,
            // inventing a formatted-nothing behavior the reference cannot exhibit.
            _ => unreachable!(
                "format_entry_field: unknown key (comptime-impossible in the reference)"
            ),
        }
    }
}

// ===========================================================================
// CHANGED dispatch — `pub fn changed(self, new, comptime key: Key) bool`.
//
// PORT NOTE: `FileFormatter`'s change-tracking branch calls
// `default.changed(self.config, key)`, which resolves `key` to a field and
// returns `!deepEqual(field.type, old, new)`. That per-`Key` `deepEqual` dispatch
// is the EQUAL/DIFF (CHANGED) pass: it requires `DeepEqual` on every config field
// type, including the foreign leaf types (`MetricModifier`/`KeyRemapSet`/
// `CursorStyle`/`WindowPaddingBalance`/`AlphaBlending`/`OptionAsAlt`/`ReleaseChannel`,
// and the Chunk-3a-hoisted `WindowPaddingColor`/`WindowColorspace`/
// `BackgroundImagePosition`/`BackgroundImageFit`/`FontShapingBreak`/`BackgroundBlur`),
// whose impls live in their own crates per the orphan rule. The pass has landed:
// `Config::changed` is fully implemented above (the per-`Key` `c!` match), and
// both the `changed = false` and `changed = true` `FileFormatter` modes (and the
// `format default config changed` test) are exercised.
// ===========================================================================

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

/// `std.meta.stringToEnum(config.AlphaBlending, input)`.
// pub const AlphaBlending = enum { native, linear, "linear-corrected" };
fn alpha_blending_string_to_enum(input: &[u8]) -> Option<AlphaBlending> {
    match input {
        b"native" => Some(AlphaBlending::Native),
        b"linear" => Some(AlphaBlending::Linear),
        b"linear-corrected" => Some(AlphaBlending::LinearCorrected),
        _ => None,
    }
}

// The six Chunk-3a-hoisted config value-types are external (rustty-render-types), so —
// exactly like AlphaBlending above — their `bare_enum_names!` `string_to_enum` becomes a
// config-local free fn (the orphan rule forbids the macro's inherent impl on a foreign
// type). Literals identical to the former `bare_enum_names!` invocations.

/// `std.meta.stringToEnum(config.WindowPaddingColor, input)`.
// pub const WindowPaddingColor = enum { background, extend, "extend-always" };
fn window_padding_color_string_to_enum(input: &[u8]) -> Option<WindowPaddingColor> {
    match input {
        b"background" => Some(WindowPaddingColor::Background),
        b"extend" => Some(WindowPaddingColor::Extend),
        b"extend-always" => Some(WindowPaddingColor::ExtendAlways),
        _ => None,
    }
}

/// `std.meta.stringToEnum(config.WindowColorspace, input)`.
// pub const WindowColorspace = enum { srgb, "display-p3" };
fn window_colorspace_string_to_enum(input: &[u8]) -> Option<WindowColorspace> {
    match input {
        b"srgb" => Some(WindowColorspace::Srgb),
        b"display-p3" => Some(WindowColorspace::DisplayP3),
        _ => None,
    }
}

/// `std.meta.stringToEnum(config.BackgroundImagePosition, input)`.
// pub const BackgroundImagePosition = enum { "top-left", "top-center", "top-right", "center-left", "center-center", "center-right", "bottom-left", "bottom-center", "bottom-right", center };
fn background_image_position_string_to_enum(input: &[u8]) -> Option<BackgroundImagePosition> {
    match input {
        b"top-left" => Some(BackgroundImagePosition::TopLeft),
        b"top-center" => Some(BackgroundImagePosition::TopCenter),
        b"top-right" => Some(BackgroundImagePosition::TopRight),
        b"center-left" => Some(BackgroundImagePosition::CenterLeft),
        b"center-center" => Some(BackgroundImagePosition::CenterCenter),
        b"center-right" => Some(BackgroundImagePosition::CenterRight),
        b"bottom-left" => Some(BackgroundImagePosition::BottomLeft),
        b"bottom-center" => Some(BackgroundImagePosition::BottomCenter),
        b"bottom-right" => Some(BackgroundImagePosition::BottomRight),
        b"center" => Some(BackgroundImagePosition::Center),
        _ => None,
    }
}

/// `std.meta.stringToEnum(config.BackgroundImageFit, input)`.
// pub const BackgroundImageFit = enum { contain, cover, stretch, none };
fn background_image_fit_string_to_enum(input: &[u8]) -> Option<BackgroundImageFit> {
    match input {
        b"contain" => Some(BackgroundImageFit::Contain),
        b"cover" => Some(BackgroundImageFit::Cover),
        b"stretch" => Some(BackgroundImageFit::Stretch),
        b"none" => Some(BackgroundImageFit::None),
        _ => None,
    }
}

// ===========================================================================
// `@tagName` maps for the EXTERNAL enums used by Config fields (FORMAT side).
//
// PORT NOTE: the generic `formatter.formatEntry`'s `.@"enum"` arm prints the
// active tag name (`{t}`). The value-type bare enums carry their own `to_name`
// (== `@tagName`) via the `bare_enum_names!` macro, but the three external enums
// (`terminal.CursorStyle`, `inputpkg.OptionAsAlt`, `build_config.ReleaseChannel`)
// live in other crates without one, so — symmetric with the `*_string_to_enum`
// maps above — their `@tagName` literals are provided here (none are hyphenated).
// ===========================================================================

/// `@tagName(terminal.CursorStyle)`.
// pub const Style = enum { bar, block, underline, block_hollow };
fn cursor_style_to_name(v: CursorStyle) -> &'static [u8] {
    match v {
        CursorStyle::Bar => b"bar",
        CursorStyle::Block => b"block",
        CursorStyle::Underline => b"underline",
        CursorStyle::BlockHollow => b"block_hollow",
    }
}

/// `@tagName(inputpkg.OptionAsAlt)`.
// pub const OptionAsAlt = enum(c_int) { false, true, left, right };
fn option_as_alt_to_name(v: rustty_input::config::OptionAsAlt) -> &'static [u8] {
    use rustty_input::config::OptionAsAlt;
    match v {
        OptionAsAlt::False => b"false",
        OptionAsAlt::True => b"true",
        OptionAsAlt::Left => b"left",
        OptionAsAlt::Right => b"right",
    }
}

/// `@tagName(build_config.ReleaseChannel)`.
// pub const ReleaseChannel = enum { tip, stable };
fn release_channel_to_name(v: rustty_build_config::ReleaseChannel) -> &'static [u8] {
    use rustty_build_config::ReleaseChannel;
    match v {
        ReleaseChannel::Tip => b"tip",
        ReleaseChannel::Stable => b"stable",
    }
}

/// `@tagName(renderer.size.PaddingBalance)`.
// pub const PaddingBalance = enum { false, true, equal };
fn window_padding_balance_to_name(v: WindowPaddingBalance) -> &'static [u8] {
    match v {
        WindowPaddingBalance::False => b"false",
        WindowPaddingBalance::True => b"true",
        WindowPaddingBalance::Equal => b"equal",
    }
}

/// `@tagName(config.AlphaBlending)`.
// pub const AlphaBlending = enum { native, linear, "linear-corrected" };
fn alpha_blending_to_name(v: AlphaBlending) -> &'static [u8] {
    match v {
        AlphaBlending::Native => b"native",
        AlphaBlending::Linear => b"linear",
        AlphaBlending::LinearCorrected => b"linear-corrected",
    }
}

// The six Chunk-3a-hoisted config value-types are external; their `bare_enum_names!`
// `to_name` (== `@tagName`) becomes a config-local free fn here (orphan rule).

/// `@tagName(config.WindowPaddingColor)`.
// pub const WindowPaddingColor = enum { background, extend, "extend-always" };
fn window_padding_color_to_name(v: WindowPaddingColor) -> &'static [u8] {
    match v {
        WindowPaddingColor::Background => b"background",
        WindowPaddingColor::Extend => b"extend",
        WindowPaddingColor::ExtendAlways => b"extend-always",
    }
}

/// `@tagName(config.WindowColorspace)`.
// pub const WindowColorspace = enum { srgb, "display-p3" };
fn window_colorspace_to_name(v: WindowColorspace) -> &'static [u8] {
    match v {
        WindowColorspace::Srgb => b"srgb",
        WindowColorspace::DisplayP3 => b"display-p3",
    }
}

/// `@tagName(config.BackgroundImagePosition)`.
// pub const BackgroundImagePosition = enum { "top-left", "top-center", "top-right", "center-left", "center-center", "center-right", "bottom-left", "bottom-center", "bottom-right", center };
fn background_image_position_to_name(v: BackgroundImagePosition) -> &'static [u8] {
    match v {
        BackgroundImagePosition::TopLeft => b"top-left",
        BackgroundImagePosition::TopCenter => b"top-center",
        BackgroundImagePosition::TopRight => b"top-right",
        BackgroundImagePosition::CenterLeft => b"center-left",
        BackgroundImagePosition::CenterCenter => b"center-center",
        BackgroundImagePosition::CenterRight => b"center-right",
        BackgroundImagePosition::BottomLeft => b"bottom-left",
        BackgroundImagePosition::BottomCenter => b"bottom-center",
        BackgroundImagePosition::BottomRight => b"bottom-right",
        BackgroundImagePosition::Center => b"center",
    }
}

/// `@tagName(config.BackgroundImageFit)`.
// pub const BackgroundImageFit = enum { contain, cover, stretch, none };
fn background_image_fit_to_name(v: BackgroundImageFit) -> &'static [u8] {
    match v {
        BackgroundImageFit::Contain => b"contain",
        BackgroundImageFit::Cover => b"cover",
        BackgroundImageFit::Stretch => b"stretch",
        BackgroundImageFit::None => b"none",
    }
}

// ===========================================================================
// Foreign-enum `@tagName` NUL-terminated name maps (the C-API `c_get` `.@"enum"`
// arm). Symmetric with the `*_to_name` maps above, but each literal is
// sentinel-terminated (`[*:0]const u8`) so the pointer is a valid C string —
// exactly the reference's `@tagName` projection for `ghostty_config_get`.
// ===========================================================================

/// `@tagName(terminal.CursorStyle)`, NUL-terminated for the C API.
pub(crate) fn cursor_style_to_name_z(v: CursorStyle) -> *const std::os::raw::c_char {
    match v {
        CursorStyle::Bar => c"bar".as_ptr(),
        CursorStyle::Block => c"block".as_ptr(),
        CursorStyle::Underline => c"underline".as_ptr(),
        CursorStyle::BlockHollow => c"block_hollow".as_ptr(),
    }
}

/// `@tagName(inputpkg.OptionAsAlt)`, NUL-terminated for the C API.
pub(crate) fn option_as_alt_to_name_z(
    v: rustty_input::config::OptionAsAlt,
) -> *const std::os::raw::c_char {
    use rustty_input::config::OptionAsAlt;
    match v {
        OptionAsAlt::False => c"false".as_ptr(),
        OptionAsAlt::True => c"true".as_ptr(),
        OptionAsAlt::Left => c"left".as_ptr(),
        OptionAsAlt::Right => c"right".as_ptr(),
    }
}

/// `@tagName(build_config.ReleaseChannel)`, NUL-terminated for the C API.
pub(crate) fn release_channel_to_name_z(
    v: rustty_build_config::ReleaseChannel,
) -> *const std::os::raw::c_char {
    use rustty_build_config::ReleaseChannel;
    match v {
        ReleaseChannel::Tip => c"tip".as_ptr(),
        ReleaseChannel::Stable => c"stable".as_ptr(),
    }
}

/// `@tagName(renderer.size.PaddingBalance)`, NUL-terminated for the C API.
pub(crate) fn window_padding_balance_to_name_z(
    v: WindowPaddingBalance,
) -> *const std::os::raw::c_char {
    match v {
        WindowPaddingBalance::False => c"false".as_ptr(),
        WindowPaddingBalance::True => c"true".as_ptr(),
        WindowPaddingBalance::Equal => c"equal".as_ptr(),
    }
}

/// `@tagName(config.AlphaBlending)`, NUL-terminated for the C API.
pub(crate) fn alpha_blending_to_name_z(v: AlphaBlending) -> *const std::os::raw::c_char {
    match v {
        AlphaBlending::Native => c"native".as_ptr(),
        AlphaBlending::Linear => c"linear".as_ptr(),
        AlphaBlending::LinearCorrected => c"linear-corrected".as_ptr(),
    }
}

/// `@tagName(config.WindowPaddingColor)`, NUL-terminated for the C API.
pub(crate) fn window_padding_color_to_name_z(v: WindowPaddingColor) -> *const std::os::raw::c_char {
    match v {
        WindowPaddingColor::Background => c"background".as_ptr(),
        WindowPaddingColor::Extend => c"extend".as_ptr(),
        WindowPaddingColor::ExtendAlways => c"extend-always".as_ptr(),
    }
}

/// `@tagName(config.WindowColorspace)`, NUL-terminated for the C API.
pub(crate) fn window_colorspace_to_name_z(v: WindowColorspace) -> *const std::os::raw::c_char {
    match v {
        WindowColorspace::Srgb => c"srgb".as_ptr(),
        WindowColorspace::DisplayP3 => c"display-p3".as_ptr(),
    }
}

/// `@tagName(config.BackgroundImagePosition)`, NUL-terminated for the C API.
pub(crate) fn background_image_position_to_name_z(
    v: BackgroundImagePosition,
) -> *const std::os::raw::c_char {
    match v {
        BackgroundImagePosition::TopLeft => c"top-left".as_ptr(),
        BackgroundImagePosition::TopCenter => c"top-center".as_ptr(),
        BackgroundImagePosition::TopRight => c"top-right".as_ptr(),
        BackgroundImagePosition::CenterLeft => c"center-left".as_ptr(),
        BackgroundImagePosition::CenterCenter => c"center-center".as_ptr(),
        BackgroundImagePosition::CenterRight => c"center-right".as_ptr(),
        BackgroundImagePosition::BottomLeft => c"bottom-left".as_ptr(),
        BackgroundImagePosition::BottomCenter => c"bottom-center".as_ptr(),
        BackgroundImagePosition::BottomRight => c"bottom-right".as_ptr(),
        BackgroundImagePosition::Center => c"center".as_ptr(),
    }
}

/// `@tagName(config.BackgroundImageFit)`, NUL-terminated for the C API.
pub(crate) fn background_image_fit_to_name_z(v: BackgroundImageFit) -> *const std::os::raw::c_char {
    match v {
        BackgroundImageFit::Contain => c"contain".as_ptr(),
        BackgroundImageFit::Cover => c"cover".as_ptr(),
        BackgroundImageFit::Stretch => c"stretch".as_ptr(),
        BackgroundImageFit::None => c"none".as_ptr(),
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

    /// PORTS: `pub const compatibility = std.StaticStringMap(...).initComptime(&.{...})`
    /// + `@hasDecl(Config, "compatibility")` lookup in `parseIntoField`.
    ///
    /// The reference's `compatibility` map values are either `compatibilityRenamed`
    /// closures (renames) or the `compat*` handler fns. This returns the matching
    /// handler for `key`, boxed (see the `Target::compatibility` note).
    // pub const compatibility = std.StaticStringMap(...).initComptime(&.{ ... });
    fn compatibility(
        &self,
        key: &[u8],
    ) -> Option<rustty_args::args::BoxedCompatibilityHandler<Self>> {
        match key {
            // Ghostty 1.1 introduced background-blur support for Linux which
            // doesn't support a specific radius value. The renaming is to let
            // one field be used for both platforms (macOS retained the ability
            // to set a radius).
            // .{ "background-blur-radius", cli.compatibilityRenamed(Config, "background-blur") },
            b"background-blur-radius" => {
                Some(rustty_args::args::compatibility_renamed(b"background-blur"))
            }

            // Ghostty 1.2 renamed all our adw options to gtk because we now have
            // a hard dependency on libadwaita.
            // .{ "adw-toolbar-style", cli.compatibilityRenamed(Config, "gtk-toolbar-style") },
            b"adw-toolbar-style" => Some(rustty_args::args::compatibility_renamed(
                b"gtk-toolbar-style",
            )),

            // Rustty 1.4 renamed `scrollback-limit` to `scrollback-limit-bytes`
            // when `scrollback-limit-lines` was added so the units are explicit.
            // .{ "scrollback-limit", cli.compatibilityRenamed(Config, "scrollback-limit-bytes") },
            b"scrollback-limit" => Some(rustty_args::args::compatibility_renamed(
                b"scrollback-limit-bytes",
            )),

            // Ghostty 1.2 removed the `hidden` value from `gtk-tabs-location` and
            // moved it to `window-show-tab-bar`.
            // .{ "gtk-tabs-location", compatGtkTabsLocation },
            b"gtk-tabs-location" => {
                Some(Box::new(|dst: &mut Config, k: &[u8], v: Option<&[u8]>| {
                    dst.compat_gtk_tabs_location(k, v)
                }))
            }

            // Ghostty 1.2 lets you set `cell-foreground` and `cell-background`
            // to match the cell foreground and background colors, respectively.
            // This can be used with `cursor-color` and `cursor-text` to recreate
            // this behavior. This applies to selection too.
            // .{ "cursor-invert-fg-bg", compatCursorInvertFgBg },
            b"cursor-invert-fg-bg" => {
                Some(Box::new(|dst: &mut Config, k: &[u8], v: Option<&[u8]>| {
                    dst.compat_cursor_invert_fg_bg(k, v)
                }))
            }
            // .{ "selection-invert-fg-bg", compatSelectionInvertFgBg },
            b"selection-invert-fg-bg" => {
                Some(Box::new(|dst: &mut Config, k: &[u8], v: Option<&[u8]>| {
                    dst.compat_selection_invert_fg_bg(k, v)
                }))
            }

            // Ghostty 1.2 merged `bold-is-bright` into the new `bold-color`
            // by setting the value to "bright".
            // .{ "bold-is-bright", compatBoldIsBright },
            b"bold-is-bright" => Some(Box::new(|dst: &mut Config, k: &[u8], v: Option<&[u8]>| {
                dst.compat_bold_is_bright(k, v)
            })),

            // Ghostty 1.2 removed the "desktop" option and renamed it to "detect".
            // The semantics also changed slightly but this is the correct mapping.
            // .{ "gtk-single-instance", compatGtkSingleInstance },
            b"gtk-single-instance" => {
                Some(Box::new(|dst: &mut Config, k: &[u8], v: Option<&[u8]>| {
                    dst.compat_gtk_single_instance(k, v)
                }))
            }

            // Ghostty 1.3 rename the "window" option to "new-window".
            // See: https://github.com/ghostty-org/ghostty/pull/9764
            // .{ "macos-dock-drop-behavior", compatMacOSDockDropBehavior },
            b"macos-dock-drop-behavior" => {
                Some(Box::new(|dst: &mut Config, k: &[u8], v: Option<&[u8]>| {
                    dst.compat_macos_dock_drop_behavior(k, v)
                }))
            }

            // Ghostty 1.4 updated "copy-on-select", allow copying to the selection
            // clipboard (on supported operating systems), the system clipboard, or
            // both. The semantics also changed but this is the correct mapping.
            // See: https://github.com/ghostty-org/ghostty/pull/12604
            // .{ "copy-on-select", compatCopyOnSelect },
            b"copy-on-select" => Some(Box::new(|dst: &mut Config, k: &[u8], v: Option<&[u8]>| {
                dst.compat_copy_on_select(k, v)
            })),

            _ => None,
        }
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
            // fn-form arm for a Chunk-3a-hoisted type whose `parse_cli` is now a
            // config-local free fn (the inherent method would orphan on the foreign
            // render-types type): `$fn(&mut self.$f, value)`.
            ($f:ident, $parse_fn:expr) => {{
                if is_empty {
                    reset_default!($f);
                }
                $parse_fn(&mut self.$f, value)?;
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
                // FontShapingBreak is hoisted to rustty-render-types (Chunk 3a), so its
                // `PackedStruct` impl (rustty-args trait) is carried by the config-local
                // `FontShapingBreakParse` newtype; `parse_packed_struct` is reused
                // verbatim and `.0` unwraps to the hoisted value.
                self.font_shaping_break = rustty_args::args::parse_packed_struct::<
                    crate::value_types::FontShapingBreakParse,
                >(value.ok_or(Error::ValueRequired)?)?
                .0;
                Ok(())
            }

            // @"alpha-blending": AlphaBlending (bare enum, external)
            b"alpha-blending" => field_enum!(alpha_blending, alpha_blending_string_to_enum),

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
            // @"background-image-position": BackgroundImagePosition (bare enum, external)
            b"background-image-position" => {
                field_enum!(
                    background_image_position,
                    background_image_position_string_to_enum
                )
            }
            // @"background-image-fit": BackgroundImageFit (bare enum, external)
            b"background-image-fit" => {
                field_enum!(background_image_fit, background_image_fit_string_to_enum)
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
            b"background-blur" => field_parsecli_self!(background_blur, background_blur_parse_cli),

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
            // @"scrollback-limit-bytes": Limit(usize, 50_000_000)
            b"scrollback-limit-bytes" => {
                field_parsecli_value!(scrollback_limit_bytes, ScrollbackLimitBytes)
            }
            // @"scrollback-limit-lines": Limit(usize, maxInt(usize))
            b"scrollback-limit-lines" => {
                field_parsecli_value!(scrollback_limit_lines, ScrollbackLimitLines)
            }
            // @"scrollback-compression": bool
            b"scrollback-compression" => field_bool!(scrollback_compression),
            // scrollbar: Scrollbar (bare enum)
            b"scrollbar" => field_enum!(scrollbar, Scrollbar::string_to_enum),

            // link: RepeatableLink (parseCLI self)
            b"link" => field_parsecli_self!(link),
            // @"link-url": bool
            b"link-url" => field_bool!(link_url),
            // @"link-osc8": bool
            b"link-osc8" => field_bool!(link_osc8),
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
            // @"window-padding-color": WindowPaddingColor (bare enum, external)
            b"window-padding-color" => {
                field_enum!(window_padding_color, window_padding_color_string_to_enum)
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
            // @"window-colorspace": WindowColorspace (bare enum, external)
            b"window-colorspace" => {
                field_enum!(window_colorspace, window_colorspace_string_to_enum)
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

            // @"drag-handle": DragHandle (bare enum)
            b"drag-handle" => field_enum!(drag_handle, DragHandle::string_to_enum),

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
            // @"clipboard-write-limit-bytes": Limit(usize, 64 * 1024 * 1024)
            b"clipboard-write-limit-bytes" => {
                field_parsecli_value!(clipboard_write_limit_bytes, ClipboardWriteLimitBytes)
            }
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
            // @"gtk-horizontal-tab-scroll": bool
            b"gtk-horizontal-tab-scroll" => field_bool!(gtk_horizontal_tab_scroll),
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
    fn parse_manually_hook(
        &mut self,
        _arg: &[u8],
    ) -> Option<Result<rustty_args::args::HookOutcome, ParseError>> {
        Some(Config::parse_manually_hook(self, _arg))
    }

    // The `-e` drain finalization — see `Config::apply_e_command`.
    fn apply_e_command(
        &mut self,
        command: Vec<Vec<u8>>,
        location: rustty_args::diagnostics::Location,
    ) -> bool {
        Config::apply_e_command(self, command, location)
    }

    // The per-arg replay recording the driver performs while draining `-e`.
    // PORTS `try self._replay_steps.append(alloc, .{ .arg = copy })`.
    fn record_replay_arg(&mut self, arg: &[u8]) {
        self._replay_steps.push(ReplayStep::Arg(arg.to_vec()));
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
// 204 names that derive the `Key` enum, a COUNT CONTRACT — dispatches to a real
// arm (never `InvalidField`), and that a bogus name does return `InvalidField`.
// ===========================================================================
#[cfg(test)]
mod parse_tests {
    use super::*;

    /// The complete, ordered list of the non-`_` Config field names (the Zig
    /// hyphenated spellings). This is a CONTRACT: it must equal the set of
    /// `Key` variants (204) and the set of `parse_into_field` arms. A drift in
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
        b"clipboard-write-limit-bytes",
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
        b"drag-handle",
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
        b"gtk-horizontal-tab-scroll",
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
        b"link-osc8",
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
        b"scrollback-compression",
        b"scrollback-limit-bytes",
        b"scrollback-limit-lines",
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
    //
    // FIELD-COUNT CONTRACT — LAST RECONCILED AGAINST THE ORACLE AT
    // `6ad1fe7d8` (oracle `origin/main`, 2026-07-29), by `P3-W9 T3` step 3.
    // See `FIELD_COUNT_ORACLE_RECONCILIATION` below for what the number means
    // and for the two rows that are still DIVERGENT by design.
    #[test]
    fn config_field_names_count_is_contract() {
        // 206 -> 208: Phase-4 w8-config (oracle range `6ad1fe7d8..12967b68f`)
        // added `drag-handle` (`c6e7e9e4e`) and `link-osc8` (`77537c806`).
        // 208 -> 209: Phase-5 p5-sys-config (oracle range `12967b68f..683d8db64`)
        // added `clipboard-write-limit-bytes` (`5350d4a5f`, default raised by
        // `70f006575`).
        assert_eq!(CONFIG_FIELD_NAMES.len(), 209);
    }

    /// Area G OQ-6: until `P3-W9 T3`, these three `204` assertions guarded rustty
    /// against ITSELF — nothing asserted the set matched the oracle's, so the
    /// contract's implicit fidelity claim was FALSE and drift was invisible.
    /// This note records the reconciliation so the next drift is visible rather
    /// than inferred. Reproduce with
    /// `python3 .port/phase-3/w9-diff-oracle-vs-rustty-fields.py`:
    ///
    /// ```text
    /// --- oracle 4749c4e93 (the pin): 209 fields | rustty: 206 ---
    ///   in oracle, NOT in rustty: _arena, _conditional_set, _conditional_state,
    ///                             _diagnostics, _replay_steps, _xdg-terminal-exec
    ///   in rustty, NOT in oracle: gtk-horizontal-tab-scroll, scrollback-compression,
    ///                             scrollback-limit-bytes, scrollback-limit-lines
    /// --- oracle origin/main (6ad1fe7d8): 212 fields | rustty: 206 ---
    ///   in oracle, NOT in rustty: <the same 6 `_`-prefixed>
    ///   in rustty, NOT in oracle: <none>
    /// ```
    ///
    /// Reading it:
    ///
    ///  * The six `_`-prefixed entries are the reference's PRIVATE fields, which
    ///    its own `Key`/field derivation skips. They are not config options and
    ///    are correctly absent from this list. Oracle PUBLIC counts are therefore
    ///    203 at the pin and **206** at head.
    ///  * rustty is now at **206**, matching the oracle's public count at head.
    ///    `P3-W3 T4` landed the runtime-limits unit (oracle
    ///    `65c48213b`/`1092204df`/`f4c68d65e`), renaming `scrollback-limit` to
    ///    `scrollback-limit-bytes` and adding `scrollback-limit-lines` TOGETHER
    ///    with the `PageList` mechanism — landing the option alone would have
    ///    shipped an unenforced knob. This block predicted 206 before it landed
    ///    and the prediction held, so the head-side DIVERGENT rows are now zero.
    ///  * `gtk-horizontal-tab-scroll` and `scrollback-compression` show as "not in
    ///    oracle" at the PIN only, because both are beyond-pin ports
    ///    (`5ea45919d`/`b5f5ca078` and `0fb89f4ff`); at head they match.
    ///  * Phase-4 w8-config (oracle range `6ad1fe7d8..12967b68f`, new pin
    ///    `12967b68f`): `drag-handle` (`c6e7e9e4e`) and `link-osc8` (`77537c806`)
    ///    landed on both sides, moving the public count 206 -> **208**.
    ///  * Phase-5 p5-sys-config (oracle range `12967b68f..683d8db64`, new pin
    ///    `683d8db64`): `clipboard-write-limit-bytes` (`5350d4a5f`, default
    ///    raised to 64 MiB by `70f006575`) landed on both sides, moving the
    ///    public count 208 -> **209**.
    const FIELD_COUNT_ORACLE_RECONCILIATION: () = ();

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

    /// PORTS: oracle 0fb89f4ff — a default-on scrollback compression option, and
    /// a 50 MB default logical scrollback limit.
    ///
    /// PORT NOTE: the plan's sketch used `Config::parse_line` /
    /// `Config::format_entry`, neither of which exists here; the real per-field
    /// dispatch pair is `parse_into_field` / `format_entry_field`.
    #[test]
    fn config_scrollback_compression_defaults_and_parses() {
        let c = Config::default();
        assert!(c.scrollback_compression);
        assert_eq!(c.scrollback_limit_bytes.value, 50_000_000);

        let mut c = Config::default();
        c.parse_into_field(b"scrollback-compression", Some(b"false"))
            .unwrap();
        assert!(!c.scrollback_compression);

        // Round-trips through the formatter.
        let mut buf = rustty_cfg_fmt::AllocatingWriter::init();
        c.format_entry_field(b"scrollback-compression", &mut buf)
            .unwrap();
        assert_eq!(buf.written(), b"scrollback-compression = false\n");
    }

    /// `P3-W9 T3` step 4 — `gtk-horizontal-tab-scroll` (oracle `5ea45919d` +
    /// `b5f5ca078`). Registering a Config option touches 12 independent tables;
    /// this covers the ones a missed registration silently breaks, each with its
    /// own assertion so it fails alone rather than as a lump: default, key
    /// lookup, parse, format round-trip. The COUNT half is
    /// `config_field_names_count_is_contract` +
    /// `key_exhaustive_parse_format_clone_changed` (206/206/206).
    ///
    /// EXECUTED NEGATIVE CONTROLS (`P3-W9 T3` step 4, each reverted):
    ///  * delete the `string_to_enum` arm -> assertion (2) FAILS, and it fails
    ///    ALONE. Worth recording: `key_exhaustive_parse_format_clone_changed`
    ///    walks `ALL_KEYS` -> `CONFIG_FIELDS` and never calls `string_to_enum`,
    ///    so a missing NAME->KEY arm is invisible to it. This test is the only
    ///    thing covering that table for this option.
    ///  * delete the `CONFIG_FIELD_NAMES` entry -> the 206 count assertion FAILS
    ///    while this test still passes. The two halves are genuinely independent.
    ///
    /// The BEHAVIOUR half — two-finger horizontal touchpad scroll switching
    /// tabs — belongs to the GTK apprt and is `P3-W8 T5 step 3`'s control, NOT
    /// this one. This test deliberately proves only that the option exists,
    /// parses, formats and round-trips; do not read it as evidence that
    /// anything CONSUMES it.
    #[test]
    fn config_gtk_horizontal_tab_scroll_defaults_parses_and_round_trips() {
        // (1) default is `true`, per the oracle's `= true`.
        assert!(Config::default().gtk_horizontal_tab_scroll);

        // (2) the name resolves to a `Key` (the `string_to_enum` parse table).
        assert_eq!(
            Key::string_to_enum(b"gtk-horizontal-tab-scroll"),
            Some(Key::GtkHorizontalTabScroll)
        );

        // (3) it parses.
        let mut c = Config::default();
        c.parse_into_field(b"gtk-horizontal-tab-scroll", Some(b"false"))
            .unwrap();
        assert!(!c.gtk_horizontal_tab_scroll);

        // (4) it round-trips through the formatter.
        let mut buf = rustty_cfg_fmt::AllocatingWriter::init();
        c.format_entry_field(b"gtk-horizontal-tab-scroll", &mut buf)
            .unwrap();
        assert_eq!(buf.written(), b"gtk-horizontal-tab-scroll = false\n");
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

// ===========================================================================
// CLONE + CHANGED tests + the ONE Key-exhaustiveness test.
//
// PORTS: the `clone default` / `clone preserves conditional state` / `changed`
// `test {}` blocks (Config.zig:10384/10401/10533), plus a Rust-only
// exhaustiveness test that replaces Zig's comptime completeness check: the
// reference's `clone`/`changed`/`formatEntry`/`parseIntoField` all `inline for`
// over the comptime field/`Key` set, so a missing arm is a COMPILE error there.
// Rust's explicit `match`/dispatch tables can drift, so this test asserts every
// `Key` variant has a parse + format + clone + changed arm.
// ===========================================================================
#[cfg(test)]
mod clone_changed_tests {
    use super::*;

    /// Every `Key` variant, in declaration order. Used by the exhaustiveness
    /// test to drive each per-`Key` pass over the full key set.
    const ALL_KEYS: &[Key] = &[
        Key::Language,
        Key::FontFamily,
        Key::FontFamilyBold,
        Key::FontFamilyItalic,
        Key::FontFamilyBoldItalic,
        Key::FontStyle,
        Key::FontStyleBold,
        Key::FontStyleItalic,
        Key::FontStyleBoldItalic,
        Key::FontSyntheticStyle,
        Key::FontFeature,
        Key::FontSize,
        Key::FontVariation,
        Key::FontVariationBold,
        Key::FontVariationItalic,
        Key::FontVariationBoldItalic,
        Key::FontCodepointMap,
        Key::ClipboardCodepointMap,
        Key::FontThicken,
        Key::FontThickenStrength,
        Key::FontShapingBreak,
        Key::AlphaBlending,
        Key::AdjustCellWidth,
        Key::AdjustCellHeight,
        Key::AdjustFontBaseline,
        Key::AdjustUnderlinePosition,
        Key::AdjustUnderlineThickness,
        Key::AdjustStrikethroughPosition,
        Key::AdjustStrikethroughThickness,
        Key::AdjustOverlinePosition,
        Key::AdjustOverlineThickness,
        Key::AdjustCursorThickness,
        Key::AdjustCursorHeight,
        Key::AdjustBoxThickness,
        Key::AdjustIconHeight,
        Key::GraphemeWidthMethod,
        Key::FreetypeLoadFlags,
        Key::Theme,
        Key::Background,
        Key::Foreground,
        Key::BackgroundImage,
        Key::BackgroundImageOpacity,
        Key::BackgroundImagePosition,
        Key::BackgroundImageFit,
        Key::BackgroundImageRepeat,
        Key::SelectionForeground,
        Key::SelectionBackground,
        Key::SelectionClearOnTyping,
        Key::SelectionClearOnCopy,
        Key::SelectionWordChars,
        Key::MinimumContrast,
        Key::Palette,
        Key::PaletteGenerate,
        Key::PaletteHarmonious,
        Key::CursorColor,
        Key::CursorOpacity,
        Key::CursorStyle,
        Key::CursorStyleBlink,
        Key::CursorText,
        Key::CursorClickToMove,
        Key::MouseHideWhileTyping,
        Key::ScrollToBottom,
        Key::MouseShiftCapture,
        Key::MouseReporting,
        Key::MouseScrollMultiplier,
        Key::BackgroundOpacity,
        Key::BackgroundOpacityCells,
        Key::BackgroundBlur,
        Key::UnfocusedSplitOpacity,
        Key::UnfocusedSplitFill,
        Key::SplitDividerColor,
        Key::SplitPreserveZoom,
        Key::SearchForeground,
        Key::SearchBackground,
        Key::SearchSelectedForeground,
        Key::SearchSelectedBackground,
        Key::Command,
        Key::InitialCommand,
        Key::NotifyOnCommandFinish,
        Key::NotifyOnCommandFinishAction,
        Key::NotifyOnCommandFinishAfter,
        Key::Env,
        Key::Input,
        Key::WaitAfterCommand,
        Key::AbnormalCommandExitRuntime,
        Key::ScrollbackLimitBytes,
        Key::ScrollbackLimitLines,
        Key::ScrollbackCompression,
        Key::Scrollbar,
        Key::Link,
        Key::LinkUrl,
        Key::LinkOsc8,
        Key::LinkPreviews,
        Key::Maximize,
        Key::Fullscreen,
        Key::Title,
        Key::Class,
        Key::X11InstanceName,
        Key::WorkingDirectory,
        Key::Keybind,
        Key::KeyRemap,
        Key::WindowPaddingX,
        Key::WindowPaddingY,
        Key::WindowPaddingBalance,
        Key::WindowPaddingColor,
        Key::WindowVsync,
        Key::WindowInheritWorkingDirectory,
        Key::TabInheritWorkingDirectory,
        Key::SplitInheritWorkingDirectory,
        Key::WindowInheritFontSize,
        Key::WindowDecoration,
        Key::WindowTitleFontFamily,
        Key::WindowSubtitle,
        Key::WindowTheme,
        Key::WindowColorspace,
        Key::WindowHeight,
        Key::WindowWidth,
        Key::WindowPositionX,
        Key::WindowPositionY,
        Key::WindowSaveState,
        Key::WindowStepResize,
        Key::WindowNewTabPosition,
        Key::WindowShowTabBar,
        Key::WindowTitlebarBackground,
        Key::WindowTitlebarForeground,
        Key::DragHandle,
        Key::ResizeOverlay,
        Key::ResizeOverlayPosition,
        Key::ResizeOverlayDuration,
        Key::FocusFollowsMouse,
        Key::ClipboardRead,
        Key::ClipboardWrite,
        Key::ClipboardWriteLimitBytes,
        Key::ClipboardTrimTrailingSpaces,
        Key::ClipboardPasteProtection,
        Key::ClipboardPasteBracketedSafe,
        Key::TitleReport,
        Key::ImageStorageLimit,
        Key::CopyOnSelect,
        Key::RightClickAction,
        Key::MiddleClickAction,
        Key::ClickRepeatInterval,
        Key::ConfigFile,
        Key::ConfigDefaultFiles,
        Key::ConfirmCloseSurface,
        Key::QuitAfterLastWindowClosed,
        Key::QuitAfterLastWindowClosedDelay,
        Key::InitialWindow,
        Key::UndoTimeout,
        Key::QuickTerminalPosition,
        Key::QuickTerminalSize,
        Key::GtkQuickTerminalLayer,
        Key::GtkQuickTerminalNamespace,
        Key::QuickTerminalScreen,
        Key::QuickTerminalAnimationDuration,
        Key::QuickTerminalAutohide,
        Key::QuickTerminalSpaceBehavior,
        Key::QuickTerminalKeyboardInteractivity,
        Key::ShellIntegration,
        Key::ShellIntegrationFeatures,
        Key::CommandPaletteEntry,
        Key::OscColorReportFormat,
        Key::VtKamAllowed,
        Key::CustomShader,
        Key::CustomShaderAnimation,
        Key::BellFeatures,
        Key::BellAudioPath,
        Key::BellAudioVolume,
        Key::AppNotifications,
        Key::MacosNonNativeFullscreen,
        Key::MacosWindowButtons,
        Key::MacosTitlebarStyle,
        Key::MacosTitlebarProxyIcon,
        Key::MacosDockDropBehavior,
        Key::MacosOptionAsAlt,
        Key::MacosWindowShadow,
        Key::MacosHidden,
        Key::MacosAutoSecureInput,
        Key::MacosSecureInputIndication,
        Key::MacosApplescript,
        Key::MacosIcon,
        Key::MacosCustomIcon,
        Key::MacosIconFrame,
        Key::MacosIconGhostColor,
        Key::MacosIconScreenColor,
        Key::MacosShortcuts,
        Key::LinuxCgroup,
        Key::LinuxCgroupMemoryLimit,
        Key::LinuxCgroupProcessesLimit,
        Key::LinuxCgroupHardFail,
        Key::GtkOpenglDebug,
        Key::GtkSingleInstance,
        Key::GtkTitlebar,
        Key::GtkTabsLocation,
        Key::GtkTitlebarHideWhenMaximized,
        Key::GtkToolbarStyle,
        Key::GtkTitlebarStyle,
        Key::GtkWideTabs,
        Key::GtkHorizontalTabScroll,
        Key::GtkCustomCss,
        Key::DesktopNotifications,
        Key::ProgressStyle,
        Key::BoldColor,
        Key::FaintOpacity,
        Key::Term,
        Key::EnquiryResponse,
        Key::AsyncBackend,
        Key::AutoUpdate,
        Key::AutoUpdateChannel,
    ];

    // The ONE Key-exhaustiveness test: every `Key` variant has a parse + format +
    // clone + changed arm. This replaces Zig's comptime completeness guarantee
    // (its `inline for` over the field/`Key` set is a compile error if an arm is
    // missing) with a runtime check over the explicit Rust dispatch tables.
    //
    // - `ALL_KEYS` enumerates every variant (its length is checked == 209, the
    //   `Key`-count contract, and == `CONFIG_FIELDS`).
    // - Each `Key` maps to exactly one `(name, key)` in `CONFIG_FIELDS` (the
    //   ordered name+key table shared by parse/format/clone).
    // - PARSE arm: `parse_into_field(name, Some(b""))` dispatches (never
    //   `InvalidField`) — the same proof as `parse_into_field_exhaustive`.
    // - FORMAT arm: `format_entry_field(name, ..)` dispatches (writes a line).
    // - CHANGED arm: `default.changed(&default, key)` resolves the key to a field
    //   (returns `false` for an unchanged clone — every arm is reachable).
    // - CLONE arm: `clone_value_into` (driven by `clone_config`) copies the field;
    //   `changeIterator` over the clone yields nothing, proving every Key field
    //   was deep-copied.
    #[test]
    fn key_exhaustive_parse_format_clone_changed() {
        // `Key`-count contract: 209 variants, matching `CONFIG_FIELDS` and the
        // 209 field names. Last reconciled against the ORACLE at `6ad1fe7d8`
        // (`P3-W9 T3` step 3) — see `FIELD_COUNT_ORACLE_RECONCILIATION` in
        // `parse_tests` for the set-diff and the two rows still divergent by
        // design.
        //
        // 204 -> 206 is TWO concurrent lanes, not one: `P3-W9 T3` added
        // `gtk-horizontal-tab-scroll` and `P3-W3 T4` split `scrollback-limit`
        // into `scrollback-limit-bytes` + `scrollback-limit-lines` (+1 net).
        // Each lane landed 205 independently and they collide here by design;
        // 206 is the merged truth.
        //
        // 206 -> 208: Phase-4 w8-config, from the `6ad1fe7d8..12967b68f` oracle
        // range — `drag-handle` (`c6e7e9e4e`) and `link-osc8` (`77537c806`).
        //
        // 208 -> 209: Phase-5 p5-sys-config, from the `12967b68f..683d8db64`
        // oracle range — `clipboard-write-limit-bytes` (`5350d4a5f`/`70f006575`).
        assert_eq!(ALL_KEYS.len(), 209);
        assert_eq!(CONFIG_FIELDS.len(), 209);

        let default = Config::default();

        for &key in ALL_KEYS {
            // Resolve the key to its `(name, key)` entry — every Key has exactly
            // one CONFIG_FIELDS row.
            let (name, _) = CONFIG_FIELDS
                .iter()
                .copied()
                .find(|(_, k)| *k == key)
                .unwrap_or_else(|| panic!("Key {key:?} missing from CONFIG_FIELDS"));

            // PARSE arm.
            let mut cfg = Config::default();
            let r = cfg.parse_into_field(name, Some(b""));
            assert!(
                r.is_ok(),
                "Key {:?} ({:?}) has no parse arm (got {:?})",
                key,
                std::str::from_utf8(name),
                r
            );

            // FORMAT arm: the call dispatching (not `InvalidField`, not an
            // `unreachable!` deferral panic) is the proof the arm exists. Some
            // empty repeatable types legitimately emit zero bytes when empty
            // (e.g. `link`), so we assert dispatch, not non-empty output.
            let mut buf = rustty_cfg_fmt::AllocatingWriter::init();
            default
                .format_entry_field(name, &mut buf)
                .expect("format_entry_field writes to an AllocatingWriter");

            // CHANGED arm: an unchanged clone never reports a change.
            assert!(
                !default.changed(&default, key),
                "Key {:?} reported a spurious change against itself",
                key
            );
        }

        // CLONE arm (over every Key field at once): a clone has no changes.
        let clone = default.clone_config();
        let mut it = default.change_iterator(&clone);
        assert_eq!(
            it.next(),
            None,
            "clone_config left a Key field uncopied (changeIterator yielded a change)"
        );
    }

    // rustty-only test (VALIDATION_REPORT §3d): the `format_entry_field` `_` arm
    // is Rust glue for a comptime-impossible case (the reference's `inline for`
    // makes an unknown key a COMPILE error). It must panic in every profile —
    // never silently format nothing.
    #[test]
    #[should_panic(expected = "format_entry_field: unknown key")]
    fn format_entry_field_unknown_key_is_unreachable() {
        let default = Config::default();
        let mut buf = rustty_cfg_fmt::AllocatingWriter::init();
        let _ = default.format_entry_field(b"no-such-config-key", &mut buf);
    }

    // test "clone default" {
    //     var source = try Config.default(alloc);
    //     var dest = try source.clone(alloc);
    //     var it = source.changeIterator(&dest);
    //     try testing.expectEqual(@as(?Key, null), it.next());
    // }
    #[test]
    fn clone_default() {
        // var source = try Config.default(alloc);
        let source = Config::default();
        // var dest = try source.clone(alloc);
        let dest = source.clone_config();

        // Should have no changes
        // var it = source.changeIterator(&dest);
        let mut it = source.change_iterator(&dest);
        // try testing.expectEqual(@as(?Key, null), it.next());
        assert_eq!(it.next(), None);

        // I want to do this but this doesn't work (the API doesn't work)
        // try testing.expectEqualDeep(dest, source);
    }

    // test "clone preserves conditional state" {
    //     var a = try Config.default(alloc);
    //     a._conditional_state.theme = .dark;
    //     try testing.expectEqual(.dark, a._conditional_state.theme);
    //     var dest = try a.clone(alloc);
    //     var it = a.changeIterator(&dest);
    //     try testing.expectEqual(@as(?Key, null), it.next());
    //     try testing.expectEqual(.dark, dest._conditional_state.theme);
    // }
    #[test]
    fn clone_preserves_conditional_state() {
        // var a = try Config.default(alloc);
        let mut a = Config::default();
        // a._conditional_state.theme = .dark;
        a._conditional_state.theme = conditional::Theme::Dark;
        // try testing.expectEqual(.dark, a._conditional_state.theme);
        assert_eq!(a._conditional_state.theme, conditional::Theme::Dark);
        // var dest = try a.clone(alloc);
        let dest = a.clone_config();

        // Should have no changes
        // var it = a.changeIterator(&dest);
        let mut it = a.change_iterator(&dest);
        // try testing.expectEqual(@as(?Key, null), it.next());
        assert_eq!(it.next(), None);

        // Should have the same conditional state
        // try testing.expectEqual(.dark, dest._conditional_state.theme);
        assert_eq!(dest._conditional_state.theme, conditional::Theme::Dark);
    }

    // test "changed" {
    //     var source = try Config.default(alloc);
    //     var dest = try source.clone(alloc);
    //     dest.@"font-thicken" = true;
    //     try testing.expect(source.changed(&dest, .@"font-thicken"));
    //     try testing.expect(!source.changed(&dest, .@"font-size"));
    // }
    #[test]
    fn changed() {
        // var source = try Config.default(alloc);
        let source = Config::default();
        // var dest = try source.clone(alloc);
        let mut dest = source.clone_config();
        // dest.@"font-thicken" = true;
        dest.font_thicken = true;

        // try testing.expect(source.changed(&dest, .@"font-thicken"));
        assert!(source.changed(&dest, Key::FontThicken));
        // try testing.expect(!source.changed(&dest, .@"font-size"));
        assert!(!source.changed(&dest, Key::FontSize));
    }
}

// ===========================================================================
// MACHINERY tests (load/loadReader/finalize). PORTS: the `test {}` blocks in the
// `load`..`Replay` range of Config.zig (Phase-0 inventory: ONE test in range —
// "handle bom in config files", assertion-for-assertion below).
// ===========================================================================
#[cfg(test)]
mod machinery_tests {
    use super::*;

    // test "handle bom in config files" {
    #[test]
    fn handle_bom_in_config_files() {
        // const testing = std.testing;
        // const alloc = testing.allocator;

        // {
        {
            // const data = "\xef\xbb\xbfabnormal-command-exit-runtime = 2500\n";
            let data: &[u8] = b"\xef\xbb\xbfabnormal-command-exit-runtime = 2500\n";
            // var reader: std.Io.Reader = .fixed(data);
            // var cfg = try Config.default(alloc);
            // defer cfg.deinit();
            let mut cfg = Config::default_config();
            // try cfg.loadReader(
            //     alloc,
            //     &reader,
            //     "/home/ghostty/.config/ghostty/config.ghostty",
            // );
            cfg.load_reader(data, b"/home/ghostty/.config/ghostty/config.ghostty")
                .unwrap();
            // try cfg.finalize();
            cfg.finalize().unwrap();

            // try testing.expect(cfg._diagnostics.empty());
            assert!(cfg._diagnostics.empty());
            // try testing.expectEqual(
            //     2500,
            //     cfg.@"abnormal-command-exit-runtime",
            // );
            assert_eq!(2500, cfg.abnormal_command_exit_runtime);
        }

        // {
        {
            // const data = "abnormal-command-exit-runtime = 2500\n";
            let data: &[u8] = b"abnormal-command-exit-runtime = 2500\n";
            // var reader: std.Io.Reader = .fixed(data);
            // var cfg = try Config.default(alloc);
            // defer cfg.deinit();
            let mut cfg = Config::default_config();
            // try cfg.loadReader(
            //     alloc,
            //     &reader,
            //     "/home/ghostty/.config/ghostty/config.ghostty",
            // );
            cfg.load_reader(data, b"/home/ghostty/.config/ghostty/config.ghostty")
                .unwrap();
            // try cfg.finalize();
            cfg.finalize().unwrap();

            // try testing.expect(cfg._diagnostics.empty());
            assert!(cfg._diagnostics.empty());
            // try testing.expectEqual(
            //     2500,
            //     cfg.@"abnormal-command-exit-runtime",
            // );
            assert_eq!(2500, cfg.abnormal_command_exit_runtime);
        }
    }

    // No reference `test {}` — PROVING tests for the `.config` scoped logger's
    // stderr gate (reference logFn: `if (!global.logging().stderr) break
    // :stderr;`, main_ghostty.zig; `global.zig` init sets `logging.stderr =
    // false` when a CLI action runs). `format_log` is the SINGLE formatting
    // site `log_info`/`log_warn` write through, so gating is asserted on its
    // return instead of fragile stderr capture.
    #[test]
    fn config_log_gate_honors_logging_stderr() {
        let gated = rustty_global::Logging {
            stderr: false,
            macos: false,
        };
        let ungated = rustty_global::Logging {
            stderr: true,
            macos: false,
        };

        // Action path (stderr=false): NOTHING is emitted for either level.
        assert_eq!(
            format_log("info", "skipping UTF-8 byte order mark", gated),
            None
        );
        assert_eq!(
            format_log("warning", "loading them both in that order", gated),
            None
        );

        // Non-action path (stderr=true): logFn's `.config`-scope prefix bytes.
        assert_eq!(
            format_log("info", "reading configuration file path=/tmp/c", ungated).as_deref(),
            Some("info(config): reading configuration file path=/tmp/c"),
        );
        assert_eq!(
            format_log("warning", "error expanding paths err=X", ungated).as_deref(),
            Some("warning(config): error expanding paths err=X"),
        );
    }

    // End-to-end through the process-global accessor the live `log_info` reads:
    // publish the action-detected value (as `rustty_apprt::init_with_action`
    // does), run the BOM'd-config load (a `log_info` call site), and assert the
    // published gate reaches `format_log`. Publishing is safe crate-wide: no
    // other test in this binary asserts stderr contents or calls `set_logging`.
    #[test]
    fn config_log_gate_reads_published_global() {
        rustty_global::set_logging(rustty_global::Logging {
            stderr: false,
            macos: false,
        });
        assert!(!rustty_global::logging().stderr);

        // The load still succeeds; its `log_info` lines are gated off.
        let data: &[u8] = b"\xef\xbb\xbfabnormal-command-exit-runtime = 2500\n";
        let mut cfg = Config::default_config();
        cfg.load_reader(data, b"/home/ghostty/.config/ghostty/config.ghostty")
            .unwrap();
        assert_eq!(2500, cfg.abnormal_command_exit_runtime);

        // What every `log_info`/`log_warn` in that path consulted:
        assert_eq!(
            format_log(
                "info",
                "skipping UTF-8 byte order mark",
                rustty_global::logging()
            ),
            None,
        );
    }
}

// ===========================================================================
// INTEGRATION tests. PORTS: the top-level `test {}` blocks in the
// finalize/clone/conditional/theme/compatibility range of Config.zig
// (lines 10341-10934), assertion-for-assertion. The Duration tests
// ("parse duration"/"test format"/"test entryFormatter") and the
// "handle bom"/"clone default"/"clone preserves conditional state"/"changed"
// tests in this range are ported in their home modules (value_types.rs and the
// `tests`/`machinery_tests` modules above).
// ===========================================================================
#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::value_types::{BoldColor, Color, MacOSDockDropBehavior, TerminalColor};
    use rustty_os::tempdir::TempDir;

    // const TestIterator = struct {
    //     data: []const []const u8,
    //     i: usize = 0,
    //     pub fn next(self: *TestIterator) ?[]const u8 { ... }
    // };
    //
    // PORTS the reference's test-local `TestIterator`. Like the reference's
    // iterator it yields raw arg strings and has no `location` decl. It
    // implements `SelfDrivenIterator<Config>` directly (ignoring the lent
    // destination, exactly as a non-replay iterator does) so it can drive
    // `Config::load_iter`.
    struct TestIterator {
        data: Vec<Vec<u8>>,
        i: usize,
    }

    impl TestIterator {
        fn new(data: &[&[u8]]) -> TestIterator {
            TestIterator {
                data: data.iter().map(|s| s.to_vec()).collect(),
                i: 0,
            }
        }
    }

    impl rustty_args::args::SelfDrivenIterator<Config> for TestIterator {
        // pub fn next(self: *TestIterator) ?[]const u8 {
        //     if (self.i >= self.data.len) return null;
        //     const result = self.data[self.i];
        //     self.i += 1;
        //     return result;
        // }
        fn next_self(&mut self, _config: &mut Config) -> Option<Vec<u8>> {
            // if (self.i >= self.data.len) return null;
            if self.i >= self.data.len() {
                return None;
            }
            // const result = self.data[self.i];
            let result = self.data[self.i].clone();
            // self.i += 1;
            self.i += 1;
            // return result;
            Some(result)
        }

        // TestIterator has no `location` decl => `Location.fromIter` yields `.none`.
        fn location_self(&self) -> Option<rustty_args::diagnostics::Location> {
            None
        }
    }

    // test "parse hook: invalid command" {
    #[test]
    fn parse_hook_invalid_command() {
        // var cfg = try Config.default(testing.allocator);
        let mut cfg = Config::default_config();
        // const alloc = cfg._arena.?.allocator();

        // var it: TestIterator = .{ .data = &.{"foo"} };
        let mut it = TestIterator::new(&[b"foo"]);
        // try testing.expect(try cfg.parseManuallyHook(alloc, "--command", &it));
        //
        // PORT NOTE: `parseManuallyHook("--command", iter)` returns `Continue`
        // (the reference's `true`) — `--command` is not `-e`, so it records the
        // replay arg and returns "continue". We assert the `HookOutcome` rather
        // than a bare bool.
        assert_eq!(
            cfg.parse_manually_hook(b"--command").unwrap(),
            rustty_args::args::HookOutcome::Continue
        );
        let _ = &mut it;
        // try testing.expect(cfg.command == null);
        assert!(cfg.command.is_none());
    }

    // test "parse e: command only" {
    #[test]
    fn parse_e_command_only() {
        // var cfg = try Config.default(testing.allocator);
        let mut cfg = Config::default_config();

        // var it: TestIterator = .{ .data = &.{"foo"} };
        let mut it = TestIterator::new(&[b"foo"]);
        // try testing.expect(!try cfg.parseManuallyHook(alloc, "-e", &it));
        //
        // PORT NOTE: the reference invokes `parseManuallyHook("-e", &it)` and the
        // hook itself drains `it`. The Rust hook is split: it records the `-e`
        // marker and returns `ConsumeRest` (the reference's `!... => return false`,
        // i.e. "do not continue"); the DRIVER then drains `it` and calls
        // `apply_e_command`. This test reproduces the driver's verbatim drain loop
        // inline so it exercises exactly the path `parseManuallyHook("-e", &it)`
        // runs in the reference.
        assert_eq!(
            cfg.parse_manually_hook(b"-e").unwrap(),
            rustty_args::args::HookOutcome::ConsumeRest
        );
        // The driver's drain (see `parse_self`'s `ConsumeRest` arm).
        let mut command: Vec<Vec<u8>> = Vec::new();
        while let Some(param) = {
            use rustty_args::args::SelfDrivenIterator;
            it.next_self(&mut cfg)
        } {
            // `dst.record_replay_arg(&param)` — the driver's per-arg replay record.
            cfg._replay_steps.push(ReplayStep::Arg(param.clone()));
            command.push(param);
        }
        let consumed_everything =
            !cfg.apply_e_command(command, rustty_args::diagnostics::Location::None);
        // The reference's hook returns `false` ("do not continue") on this path.
        assert!(consumed_everything);

        // const cmd = cfg.@"initial-command".?;
        let cmd = cfg.initial_command.as_ref().unwrap();
        // try testing.expect(cmd == .direct);
        // try testing.expectEqual(cmd.direct.len, 1);
        // try testing.expectEqualStrings(cmd.direct[0], "foo");
        match cmd {
            Command::Direct(direct) => {
                assert_eq!(direct.len(), 1);
                assert_eq!(direct[0].as_slice(), b"foo");
            }
            _ => panic!("expected .direct"),
        }
    }

    // test "parse e: command and args" {
    #[test]
    fn parse_e_command_and_args() {
        // var cfg = try Config.default(testing.allocator);
        let mut cfg = Config::default_config();

        // var it: TestIterator = .{ .data = &.{ "echo", "foo", "bar baz" } };
        let mut it = TestIterator::new(&[b"echo", b"foo", b"bar baz"]);
        // try testing.expect(!try cfg.parseManuallyHook(alloc, "-e", &it));
        //
        // See `parse_e_command_only` for the hook/driver split. We reproduce the
        // driver's drain inline here too.
        assert_eq!(
            cfg.parse_manually_hook(b"-e").unwrap(),
            rustty_args::args::HookOutcome::ConsumeRest
        );
        let mut command: Vec<Vec<u8>> = Vec::new();
        while let Some(param) = {
            use rustty_args::args::SelfDrivenIterator;
            it.next_self(&mut cfg)
        } {
            // `dst.record_replay_arg(&param)` — the driver's per-arg replay record.
            cfg._replay_steps.push(ReplayStep::Arg(param.clone()));
            command.push(param);
        }
        let consumed_everything =
            !cfg.apply_e_command(command, rustty_args::diagnostics::Location::None);
        assert!(consumed_everything);

        // const cmd = cfg.@"initial-command".?;
        let cmd = cfg.initial_command.as_ref().unwrap();
        // try testing.expect(cmd == .direct);
        // try testing.expectEqual(cmd.direct.len, 3);
        // try testing.expectEqualStrings(cmd.direct[0], "echo");
        // try testing.expectEqualStrings(cmd.direct[1], "foo");
        // try testing.expectEqualStrings(cmd.direct[2], "bar baz");
        match cmd {
            Command::Direct(direct) => {
                assert_eq!(direct.len(), 3);
                assert_eq!(direct[0].as_slice(), b"echo");
                assert_eq!(direct[1].as_slice(), b"foo");
                assert_eq!(direct[2].as_slice(), b"bar baz");
            }
            _ => panic!("expected .direct"),
        }
    }

    // test "clone can then change conditional state" {
    #[test]
    fn clone_can_then_change_conditional_state() {
        // This tests a particular bug sequence where:
        //   1. Load light
        //   2. Convert to dark
        //   3. Clone dark
        //   4. Convert to light
        //   5. Config is still dark (bug)

        // Setup our test theme
        // var td = try internal_os.TempDir.init();
        let mut td = TempDir::init().unwrap();
        // defer td.deinit();
        // {
        //     var file = try td.dir.createFile("theme_light", .{});
        //     ... writeAll(@embedFile("testdata/theme_light")); ...
        // }
        td.dir()
            .writeFile("theme_light", include_bytes!("testdata/theme_light"))
            .unwrap();
        // {
        //     var file = try td.dir.createFile("theme_dark", .{});
        //     ... writeAll(@embedFile("testdata/theme_dark")); ...
        // }
        td.dir()
            .writeFile("theme_dark", include_bytes!("testdata/theme_dark"))
            .unwrap();
        // const light = try td.dir.realpath("theme_light", &light_buf);
        let light = td.dir().realpath("theme_light").unwrap();
        let light = light.to_str().unwrap();
        // const dark = try td.dir.realpath("theme_dark", &dark_buf);
        let dark = td.dir().realpath("theme_dark").unwrap();
        let dark = dark.to_str().unwrap();

        // var cfg_light = try Config.default(alloc);
        let mut cfg_light = Config::default_config();
        // var it: TestIterator = .{ .data = &.{
        //     try std.fmt.allocPrint(alloc_arena, "--theme=light:{s},dark:{s}", .{ light, dark }),
        // } };
        let arg = format!("--theme=light:{light},dark:{dark}").into_bytes();
        let mut it = TestIterator::new(&[arg.as_slice()]);
        // try cfg_light.loadIter(alloc, &it);
        cfg_light.load_iter(&mut it).unwrap();
        // try cfg_light.finalize();
        cfg_light.finalize().unwrap();

        // var cfg_dark = (try cfg_light.changeConditionalState(.{ .theme = .dark })).?;
        let mut cfg_dark = cfg_light
            .change_conditional_state(conditional::State {
                theme: conditional::Theme::Dark,
                ..Default::default()
            })
            .unwrap()
            .unwrap();

        // try testing.expectEqual(Color{ .r = 0xEE, .g = 0xEE, .b = 0xEE }, cfg_dark.background);
        assert_eq!(
            Color {
                r: 0xEE,
                g: 0xEE,
                b: 0xEE,
            },
            cfg_dark.background
        );

        // var cfg_clone = try cfg_dark.clone(alloc);
        let mut cfg_clone = cfg_dark.clone_config();
        // try testing.expectEqual(Color{ .r = 0xEE, .g = 0xEE, .b = 0xEE }, cfg_clone.background);
        assert_eq!(
            Color {
                r: 0xEE,
                g: 0xEE,
                b: 0xEE,
            },
            cfg_clone.background
        );

        // var cfg_light2 = (try cfg_clone.changeConditionalState(.{ .theme = .light })).?;
        let mut cfg_light2 = cfg_clone
            .change_conditional_state(conditional::State {
                theme: conditional::Theme::Light,
                ..Default::default()
            })
            .unwrap()
            .unwrap();
        // try testing.expectEqual(Color{ .r = 0xFF, .g = 0xFF, .b = 0xFF }, cfg_light2.background);
        assert_eq!(
            Color {
                r: 0xFF,
                g: 0xFF,
                b: 0xFF,
            },
            cfg_light2.background
        );

        // defers
        cfg_light2.deinit();
        cfg_clone.deinit();
        cfg_dark.deinit();
        cfg_light.deinit();
        td.deinit();
    }

    // test "clone preserves conditional set" {
    #[test]
    fn clone_preserves_conditional_set() {
        // var cfg = try Config.default(alloc);
        let mut cfg = Config::default_config();
        // var it: TestIterator = .{ .data = &.{
        //     "--theme=light:foo,dark:bar",
        //     "--window-theme=auto",
        // } };
        let mut it = TestIterator::new(&[b"--theme=light:foo,dark:bar", b"--window-theme=auto"]);
        // try cfg.loadIter(alloc, &it);
        cfg.load_iter(&mut it).unwrap();
        // try cfg.finalize();
        cfg.finalize().unwrap();

        // var clone1 = try cfg.clone(alloc);
        let clone1 = cfg.clone_config();

        // try testing.expect(clone1._conditional_set.contains(.theme));
        assert!(clone1._conditional_set.contains(&conditional::Key::Theme));
    }

    // test "working-directory expands tilde" {
    #[test]
    fn working_directory_expands_tilde() {
        // var cfg = try Config.default(alloc);
        let mut cfg = Config::default_config();
        // var it: TestIterator = .{ .data = &.{
        //     "--working-directory=~/projects/ghostty",
        // } };
        let mut it = TestIterator::new(&[b"--working-directory=~/projects/ghostty"]);
        // try cfg.loadIter(alloc, &it);
        cfg.load_iter(&mut it).unwrap();
        // try cfg.finalize();
        cfg.finalize().unwrap();

        // var buf: [std.fs.max_path_bytes]u8 = undefined;
        // const expected = internal_os.expandHome("~/projects/ghostty", &buf)
        //     catch "~/projects/ghostty";
        let mut buf = [0u8; MAX_PATH_BYTES];
        let expected: Vec<u8> = rustty_os::expandHome(
            &rustty_global::environ_map(),
            b"~/projects/ghostty",
            &mut buf,
        )
        .map(|s| s.to_vec())
        .unwrap_or_else(|_| b"~/projects/ghostty".to_vec());
        // try testing.expectEqualStrings(expected, cfg.@"working-directory".?.value().?);
        assert_eq!(
            expected.as_slice(),
            cfg.working_directory.as_ref().unwrap().value().unwrap()
        );
    }

    // test "changeConditionalState ignores irrelevant changes" {
    #[test]
    fn change_conditional_state_ignores_irrelevant_changes() {
        // {
        {
            // var cfg = try Config.default(alloc);
            let mut cfg = Config::default_config();
            // var it: TestIterator = .{ .data = &.{ "--theme=foo" } };
            let mut it = TestIterator::new(&[b"--theme=foo"]);
            // try cfg.loadIter(alloc, &it);
            cfg.load_iter(&mut it).unwrap();
            // try cfg.finalize();
            cfg.finalize().unwrap();

            // try testing.expect(try cfg.changeConditionalState(.{ .theme = .dark }) == null);
            assert!(cfg
                .change_conditional_state(conditional::State {
                    theme: conditional::Theme::Dark,
                    ..Default::default()
                })
                .unwrap()
                .is_none());
        }
    }

    // test "changeConditionalState applies relevant changes" {
    #[test]
    fn change_conditional_state_applies_relevant_changes() {
        // {
        {
            // var cfg = try Config.default(alloc);
            let mut cfg = Config::default_config();
            // var it: TestIterator = .{ .data = &.{ "--theme=light:foo,dark:bar" } };
            let mut it = TestIterator::new(&[b"--theme=light:foo,dark:bar"]);
            // try cfg.loadIter(alloc, &it);
            cfg.load_iter(&mut it).unwrap();
            // try cfg.finalize();
            cfg.finalize().unwrap();

            // var cfg2 = (try cfg.changeConditionalState(.{ .theme = .dark })).?;
            let cfg2 = cfg
                .change_conditional_state(conditional::State {
                    theme: conditional::Theme::Dark,
                    ..Default::default()
                })
                .unwrap()
                .unwrap();

            // try testing.expect(cfg2._conditional_set.contains(.theme));
            assert!(cfg2._conditional_set.contains(&conditional::Key::Theme));
        }
    }

    // test "theme loading" {
    #[test]
    fn theme_loading() {
        // Setup our test theme
        // var td = try internal_os.TempDir.init();
        let mut td = TempDir::init().unwrap();
        // { var file = try td.dir.createFile("theme", .{}); ... @embedFile("testdata/theme_simple") ... }
        td.dir()
            .writeFile("theme", include_bytes!("testdata/theme_simple"))
            .unwrap();
        // const path = try td.dir.realpath("theme", &path_buf);
        let path = td.dir().realpath("theme").unwrap();
        let path = path.to_str().unwrap();

        // var cfg = try Config.default(alloc);
        let mut cfg = Config::default_config();
        // var it: TestIterator = .{ .data = &.{
        //     try std.fmt.allocPrint(alloc_arena, "--theme={s}", .{path}),
        // } };
        let arg = format!("--theme={path}").into_bytes();
        let mut it = TestIterator::new(&[arg.as_slice()]);
        // try cfg.loadIter(alloc, &it);
        cfg.load_iter(&mut it).unwrap();
        // try cfg.finalize();
        cfg.finalize().unwrap();

        // try testing.expectEqual(Color{ .r = 0x12, .g = 0x3A, .b = 0xBC }, cfg.background);
        assert_eq!(
            Color {
                r: 0x12,
                g: 0x3A,
                b: 0xBC,
            },
            cfg.background
        );

        // Not a conditional theme
        // try testing.expect(!cfg._conditional_set.contains(.theme));
        assert!(!cfg._conditional_set.contains(&conditional::Key::Theme));

        // defer td.deinit();
        td.deinit();
    }

    // test "theme loading preserves conditional state" {
    #[test]
    fn theme_loading_preserves_conditional_state() {
        // Setup our test theme
        // var td = try internal_os.TempDir.init();
        let mut td = TempDir::init().unwrap();
        td.dir()
            .writeFile("theme", include_bytes!("testdata/theme_simple"))
            .unwrap();
        // const path = try td.dir.realpath("theme", &path_buf);
        let path = td.dir().realpath("theme").unwrap();
        let path = path.to_str().unwrap();

        // var cfg = try Config.default(alloc);
        let mut cfg = Config::default_config();
        // cfg._conditional_state = .{ .theme = .dark };
        cfg._conditional_state = conditional::State {
            theme: conditional::Theme::Dark,
            ..Default::default()
        };
        // var it: TestIterator = .{ .data = &.{
        //     try std.fmt.allocPrint(alloc_arena, "--theme={s}", .{path}),
        // } };
        let arg = format!("--theme={path}").into_bytes();
        let mut it = TestIterator::new(&[arg.as_slice()]);
        // try cfg.loadIter(alloc, &it);
        cfg.load_iter(&mut it).unwrap();
        // try cfg.finalize();
        cfg.finalize().unwrap();

        // try testing.expect(cfg._conditional_state.theme == .dark);
        assert!(cfg._conditional_state.theme == conditional::Theme::Dark);

        // defer td.deinit();
        td.deinit();
    }

    // test "theme priority is lower than config" {
    #[test]
    fn theme_priority_is_lower_than_config() {
        // Setup our test theme
        // var td = try internal_os.TempDir.init();
        let mut td = TempDir::init().unwrap();
        td.dir()
            .writeFile("theme", include_bytes!("testdata/theme_simple"))
            .unwrap();
        // const path = try td.dir.realpath("theme", &path_buf);
        let path = td.dir().realpath("theme").unwrap();
        let path = path.to_str().unwrap();

        // var cfg = try Config.default(alloc);
        let mut cfg = Config::default_config();
        // var it: TestIterator = .{ .data = &.{
        //     "--background=#ABCDEF",
        //     try std.fmt.allocPrint(alloc_arena, "--theme={s}", .{path}),
        // } };
        let theme_arg = format!("--theme={path}").into_bytes();
        let mut it = TestIterator::new(&[b"--background=#ABCDEF", theme_arg.as_slice()]);
        // try cfg.loadIter(alloc, &it);
        cfg.load_iter(&mut it).unwrap();
        // try cfg.finalize();
        cfg.finalize().unwrap();

        // try testing.expectEqual(Color{ .r = 0xAB, .g = 0xCD, .b = 0xEF }, cfg.background);
        assert_eq!(
            Color {
                r: 0xAB,
                g: 0xCD,
                b: 0xEF,
            },
            cfg.background
        );

        // defer td.deinit();
        td.deinit();
    }

    // test "theme loading correct light/dark" {
    #[test]
    fn theme_loading_correct_light_dark() {
        // Setup our test theme
        // var td = try internal_os.TempDir.init();
        let mut td = TempDir::init().unwrap();
        td.dir()
            .writeFile("theme_light", include_bytes!("testdata/theme_light"))
            .unwrap();
        td.dir()
            .writeFile("theme_dark", include_bytes!("testdata/theme_dark"))
            .unwrap();
        // const light = try td.dir.realpath("theme_light", &light_buf);
        let light = td.dir().realpath("theme_light").unwrap();
        let light = light.to_str().unwrap();
        // const dark = try td.dir.realpath("theme_dark", &dark_buf);
        let dark = td.dir().realpath("theme_dark").unwrap();
        let dark = dark.to_str().unwrap();

        // Light
        // {
        {
            // var cfg = try Config.default(alloc);
            let mut cfg = Config::default_config();
            // var it: TestIterator = .{ .data = &.{
            //     try std.fmt.allocPrint(alloc_arena, "--theme=light:{s},dark:{s}", .{ light, dark }),
            // } };
            let arg = format!("--theme=light:{light},dark:{dark}").into_bytes();
            let mut it = TestIterator::new(&[arg.as_slice()]);
            // try cfg.loadIter(alloc, &it);
            cfg.load_iter(&mut it).unwrap();
            // try cfg.finalize();
            cfg.finalize().unwrap();

            // try testing.expectEqual(Color{ .r = 0xFF, .g = 0xFF, .b = 0xFF }, cfg.background);
            assert_eq!(
                Color {
                    r: 0xFF,
                    g: 0xFF,
                    b: 0xFF,
                },
                cfg.background
            );
        }

        // Dark
        // {
        {
            // var cfg = try Config.default(alloc);
            let mut cfg = Config::default_config();
            // cfg._conditional_state = .{ .theme = .dark };
            cfg._conditional_state = conditional::State {
                theme: conditional::Theme::Dark,
                ..Default::default()
            };
            // var it: TestIterator = .{ .data = &.{
            //     try std.fmt.allocPrint(alloc_arena, "--theme=light:{s},dark:{s}", .{ light, dark }),
            // } };
            let arg = format!("--theme=light:{light},dark:{dark}").into_bytes();
            let mut it = TestIterator::new(&[arg.as_slice()]);
            // try cfg.loadIter(alloc, &it);
            cfg.load_iter(&mut it).unwrap();
            // try cfg.finalize();
            cfg.finalize().unwrap();

            // try testing.expectEqual(Color{ .r = 0xEE, .g = 0xEE, .b = 0xEE }, cfg.background);
            assert_eq!(
                Color {
                    r: 0xEE,
                    g: 0xEE,
                    b: 0xEE,
                },
                cfg.background
            );
        }

        // Light to Dark
        // {
        {
            // var cfg = try Config.default(alloc);
            let mut cfg = Config::default_config();
            // var it: TestIterator = .{ .data = &.{
            //     try std.fmt.allocPrint(alloc_arena, "--theme=light:{s},dark:{s}", .{ light, dark }),
            // } };
            let arg = format!("--theme=light:{light},dark:{dark}").into_bytes();
            let mut it = TestIterator::new(&[arg.as_slice()]);
            // try cfg.loadIter(alloc, &it);
            cfg.load_iter(&mut it).unwrap();
            // try cfg.finalize();
            cfg.finalize().unwrap();

            // var new = (try cfg.changeConditionalState(.{ .theme = .dark })).?;
            let new = cfg
                .change_conditional_state(conditional::State {
                    theme: conditional::Theme::Dark,
                    ..Default::default()
                })
                .unwrap()
                .unwrap();
            // try testing.expectEqual(Color{ .r = 0xEE, .g = 0xEE, .b = 0xEE }, new.background);
            assert_eq!(
                Color {
                    r: 0xEE,
                    g: 0xEE,
                    b: 0xEE,
                },
                new.background
            );
        }

        // defer td.deinit();
        td.deinit();
    }

    // test "theme specifying light/dark changes window-theme from auto" {
    #[test]
    fn theme_specifying_light_dark_changes_window_theme_from_auto() {
        // {
        {
            // var cfg = try Config.default(alloc);
            let mut cfg = Config::default_config();
            // var it: TestIterator = .{ .data = &.{
            //     "--theme=light:foo,dark:bar",
            //     "--window-theme=auto",
            // } };
            let mut it =
                TestIterator::new(&[b"--theme=light:foo,dark:bar", b"--window-theme=auto"]);
            // try cfg.loadIter(alloc, &it);
            cfg.load_iter(&mut it).unwrap();
            // try cfg.finalize();
            cfg.finalize().unwrap();

            // try testing.expect(cfg.@"window-theme" == .system);
            assert!(cfg.window_theme == WindowTheme::System);
        }
    }

    // test "theme specifying light/dark sets theme usage in conditional state" {
    #[test]
    fn theme_specifying_light_dark_sets_theme_usage_in_conditional_state() {
        // {
        {
            // var cfg = try Config.default(alloc);
            let mut cfg = Config::default_config();
            // var it: TestIterator = .{ .data = &.{
            //     "--theme=light:foo,dark:bar",
            //     "--window-theme=auto",
            // } };
            let mut it =
                TestIterator::new(&[b"--theme=light:foo,dark:bar", b"--window-theme=auto"]);
            // try cfg.loadIter(alloc, &it);
            cfg.load_iter(&mut it).unwrap();
            // try cfg.finalize();
            cfg.finalize().unwrap();

            // try testing.expect(cfg.@"window-theme" == .system);
            assert!(cfg.window_theme == WindowTheme::System);
            // try testing.expect(cfg._conditional_set.contains(.theme));
            assert!(cfg._conditional_set.contains(&conditional::Key::Theme));
        }
    }

    // test "scrollback limits" {
    #[test]
    fn scrollback_limits() {
        // var cfg = try Config.default(alloc);
        let mut cfg = Config::default_config();
        // try testing.expectEqual(@as(usize, 50_000_000), cfg.@"scrollback-limit-bytes".value);
        assert_eq!(50_000_000usize, cfg.scrollback_limit_bytes.value);
        // try testing.expectEqual(std.math.maxInt(usize), cfg.@"scrollback-limit-lines".value);
        assert_eq!(usize::MAX, cfg.scrollback_limit_lines.value);

        // var it: TestIterator = .{ .data = &.{
        //     "--scrollback-limit-bytes=1234",
        //     "--scrollback-limit-lines=567",
        // } };
        let mut it = TestIterator::new(&[
            b"--scrollback-limit-bytes=1234",
            b"--scrollback-limit-lines=567",
        ]);
        // try cfg.loadIter(alloc, &it);
        cfg.load_iter(&mut it).unwrap();

        // try testing.expectEqual(@as(usize, 1234), cfg.@"scrollback-limit-bytes".value);
        assert_eq!(1234usize, cfg.scrollback_limit_bytes.value);
        // try testing.expectEqual(@as(usize, 567), cfg.@"scrollback-limit-lines".value);
        assert_eq!(567usize, cfg.scrollback_limit_lines.value);

        // var unlimited_it: TestIterator = .{ .data = &.{
        //     "--scrollback-limit-bytes=unlimited",
        //     "--scrollback-limit-lines=unlimited",
        // } };
        let mut unlimited_it = TestIterator::new(&[
            b"--scrollback-limit-bytes=unlimited",
            b"--scrollback-limit-lines=unlimited",
        ]);
        cfg.load_iter(&mut unlimited_it).unwrap();

        assert_eq!(usize::MAX, cfg.scrollback_limit_bytes.value);
        assert_eq!(usize::MAX, cfg.scrollback_limit_lines.value);

        // var reset_it: TestIterator = .{ .data = &.{
        //     "--scrollback-limit-bytes=",
        //     "--scrollback-limit-lines=",
        // } };
        let mut reset_it =
            TestIterator::new(&[b"--scrollback-limit-bytes=", b"--scrollback-limit-lines="]);
        cfg.load_iter(&mut reset_it).unwrap();

        assert_eq!(50_000_000usize, cfg.scrollback_limit_bytes.value);
        assert_eq!(usize::MAX, cfg.scrollback_limit_lines.value);

        // PORT NOTE (rustty-only, `P3-W3` T4 Step 5): the oracle's tests are
        // PARSE-ONLY, which a non-round-tripping formatter would pass. Assert
        // that `unlimited` survives `format_entry` as the literal sentinel and
        // that a finite value still formats as an integer.
        {
            let mut buf = rustty_cfg_fmt::AllocatingWriter::init();
            cfg.format_entry_field(b"scrollback-limit-lines", &mut buf)
                .unwrap();
            assert_eq!(
                b"scrollback-limit-lines = unlimited\n".as_slice(),
                buf.written()
            );
        }
        {
            let mut buf = rustty_cfg_fmt::AllocatingWriter::init();
            cfg.format_entry_field(b"scrollback-limit-bytes", &mut buf)
                .unwrap();
            assert_eq!(
                b"scrollback-limit-bytes = 50000000\n".as_slice(),
                buf.written()
            );
        }
        // And that the formatted `unlimited` parses back to the same value.
        let mut round = Config::default_config();
        round
            .parse_into_field(b"scrollback-limit-bytes", Some(b"unlimited"))
            .unwrap();
        assert_eq!(usize::MAX, round.scrollback_limit_bytes.value);
        assert_eq!(None, round.scrollback_limit_bytes.optional());
        assert_eq!(Some(50_000_000usize), cfg.scrollback_limit_bytes.optional());
    }

    // test "clipboard write limit" {
    #[test]
    fn clipboard_write_limit() {
        // var cfg = try Config.default(alloc);
        // defer cfg.deinit();
        let mut cfg = Config::default_config();
        // try testing.expectEqual(
        //     @as(usize, 64 * 1024 * 1024),
        //     cfg.@"clipboard-write-limit-bytes".value,
        // );
        assert_eq!(64 * 1024 * 1024, cfg.clipboard_write_limit_bytes.value);

        // var it: TestIterator = .{ .data = &.{
        //     "--clipboard-write-limit-bytes=1234",
        // } };
        let mut it = TestIterator::new(&[b"--clipboard-write-limit-bytes=1234"]);
        // try cfg.loadIter(alloc, &it);
        cfg.load_iter(&mut it).unwrap();

        // try testing.expectEqual(
        //     @as(usize, 1234),
        //     cfg.@"clipboard-write-limit-bytes".value,
        // );
        assert_eq!(1234usize, cfg.clipboard_write_limit_bytes.value);

        // var unlimited_it: TestIterator = .{ .data = &.{
        //     "--clipboard-write-limit-bytes=unlimited",
        // } };
        let mut unlimited_it = TestIterator::new(&[b"--clipboard-write-limit-bytes=unlimited"]);
        cfg.load_iter(&mut unlimited_it).unwrap();

        // try testing.expectEqual(
        //     std.math.maxInt(usize),
        //     cfg.@"clipboard-write-limit-bytes".value,
        // );
        assert_eq!(usize::MAX, cfg.clipboard_write_limit_bytes.value);
    }

    // test "compatibility: scrollback-limit renamed to bytes" {
    #[test]
    fn compatibility_scrollback_limit_renamed_to_bytes() {
        // var cfg = try Config.default(alloc);
        let mut cfg = Config::default_config();
        // var it: TestIterator = .{ .data = &.{ "--scrollback-limit=1234" } };
        let mut it = TestIterator::new(&[b"--scrollback-limit=1234"]);
        // try cfg.loadIter(alloc, &it);
        cfg.load_iter(&mut it).unwrap();

        // try testing.expectEqual(@as(usize, 1234), cfg.@"scrollback-limit-bytes".value);
        assert_eq!(1234usize, cfg.scrollback_limit_bytes.value);

        // var unlimited_it: TestIterator = .{ .data = &.{ "--scrollback-limit=unlimited" } };
        let mut unlimited_it = TestIterator::new(&[b"--scrollback-limit=unlimited"]);
        cfg.load_iter(&mut unlimited_it).unwrap();

        // try testing.expectEqual(std.math.maxInt(usize), cfg.@"scrollback-limit-bytes".value);
        assert_eq!(usize::MAX, cfg.scrollback_limit_bytes.value);
    }

    // PORT-SIDE REGRESSION (`dfccdb2d4` + `e80ce2ed4`, #12600, phase-6): the
    // reference added `compatCopyOnSelect` without a `test {}`. `true`/`false`
    // are no longer variants of `CopyOnSelect`, so a bare enum parse of them
    // fails and the compatibility handler is what maps them onto the new
    // values — `true` -> `primary` on Linux/FreeBSD and `clipboard` elsewhere
    // (the "correct mapping" the reference's table comment names), `false` ->
    // `none`. NEGATIVE CONTROL: removing the `copy-on-select` row from
    // `compatibility()` (or the handler's `true` arm) leaves the field at its
    // default and records an `InvalidValue` diagnostic, failing the
    // `cfg._diagnostics.empty()` assertions below.
    #[test]
    fn compatibility_copy_on_select_true_false() {
        // `true` maps to the platform's old "prefer the selection clipboard"
        // behaviour.
        {
            let mut cfg = Config::default_config();
            let mut it = TestIterator::new(&[b"--copy-on-select=true"]);
            cfg.load_iter(&mut it).unwrap();
            assert!(cfg._diagnostics.empty());
            let want = if cfg!(any(target_os = "linux", target_os = "freebsd")) {
                CopyOnSelect::Primary
            } else {
                CopyOnSelect::Clipboard
            };
            assert_eq!(want, cfg.copy_on_select);
        }

        // `false` is an alias for `none`.
        {
            let mut cfg = Config::default_config();
            let mut it = TestIterator::new(&[b"--copy-on-select=false"]);
            cfg.load_iter(&mut it).unwrap();
            assert!(cfg._diagnostics.empty());
            assert_eq!(CopyOnSelect::None, cfg.copy_on_select);
        }

        // The new spellings parse as plain enum values (no handler involved).
        for (arg, want) in [
            (&b"--copy-on-select=none"[..], CopyOnSelect::None),
            (&b"--copy-on-select=primary"[..], CopyOnSelect::Primary),
            (&b"--copy-on-select=clipboard"[..], CopyOnSelect::Clipboard),
            (&b"--copy-on-select=both"[..], CopyOnSelect::Both),
        ] {
            let mut cfg = Config::default_config();
            let mut it = TestIterator::new(&[arg]);
            cfg.load_iter(&mut it).unwrap();
            assert!(cfg._diagnostics.empty());
            assert_eq!(want, cfg.copy_on_select);
        }

        // The handler only rescues `true`/`false`: anything else is still an
        // invalid value (recorded as a diagnostic, field left at its default).
        {
            let mut cfg = Config::default_config();
            let before = cfg.copy_on_select;
            let mut it = TestIterator::new(&[b"--copy-on-select=bogus"]);
            cfg.load_iter(&mut it).unwrap();
            assert!(!cfg._diagnostics.empty());
            assert_eq!(before, cfg.copy_on_select);
        }

        // The direct handler contract (`compatCopyOnSelect`): `true`/`false`
        // handled, anything else `false` (unhandled) and untouched.
        {
            let mut cfg = Config::default_config();
            assert!(cfg.compat_copy_on_select(b"copy-on-select", Some(b"false")));
            assert_eq!(CopyOnSelect::None, cfg.copy_on_select);
            assert!(!cfg.compat_copy_on_select(b"copy-on-select", Some(b"primary")));
            assert_eq!(CopyOnSelect::None, cfg.copy_on_select);
            assert!(!cfg.compat_copy_on_select(b"copy-on-select", None));
        }
    }

    // PORT-SIDE REGRESSION (`dfccdb2d4`, #12600, phase-6): `middle-click-action`
    // gains `clipboard-paste` and keeps `primary-paste` (still the default).
    #[test]
    fn middle_click_action_clipboard_paste() {
        let cfg = Config::default_config();
        assert_eq!(MiddleClickAction::PrimaryPaste, cfg.middle_click_action);

        let mut cfg = Config::default_config();
        let mut it = TestIterator::new(&[b"--middle-click-action=clipboard-paste"]);
        cfg.load_iter(&mut it).unwrap();
        assert!(cfg._diagnostics.empty());
        assert_eq!(MiddleClickAction::ClipboardPaste, cfg.middle_click_action);
    }

    // test "compatibility: gtk-single-instance desktop" {
    #[test]
    fn compatibility_gtk_single_instance_desktop() {
        // {
        {
            // var cfg = try Config.default(alloc);
            let mut cfg = Config::default_config();
            // var it: TestIterator = .{ .data = &.{ "--gtk-single-instance=desktop" } };
            let mut it = TestIterator::new(&[b"--gtk-single-instance=desktop"]);
            // try cfg.loadIter(alloc, &it);
            cfg.load_iter(&mut it).unwrap();

            // We need to test this BEFORE finalize, because finalize will
            // convert our detect to a real value.
            // try testing.expectEqual(GtkSingleInstance.detect, cfg.@"gtk-single-instance");
            assert_eq!(GtkSingleInstance::Detect, cfg.gtk_single_instance);
        }
    }

    // test "compatibility: removed cursor-invert-fg-bg" {
    #[test]
    fn compatibility_removed_cursor_invert_fg_bg() {
        // {
        {
            // var cfg = try Config.default(alloc);
            let mut cfg = Config::default_config();
            // var it: TestIterator = .{ .data = &.{ "--cursor-invert-fg-bg" } };
            let mut it = TestIterator::new(&[b"--cursor-invert-fg-bg"]);
            // try cfg.loadIter(alloc, &it);
            cfg.load_iter(&mut it).unwrap();
            // try cfg.finalize();
            cfg.finalize().unwrap();

            // try testing.expectEqual(TerminalColor.@"cell-foreground", cfg.@"cursor-color");
            assert_eq!(Some(TerminalColor::CellForeground), cfg.cursor_color);
            // try testing.expectEqual(TerminalColor.@"cell-background", cfg.@"cursor-text");
            assert_eq!(Some(TerminalColor::CellBackground), cfg.cursor_text);
        }
    }

    // test "compatibility: removed selection-invert-fg-bg" {
    #[test]
    fn compatibility_removed_selection_invert_fg_bg() {
        // {
        {
            // var cfg = try Config.default(alloc);
            let mut cfg = Config::default_config();
            // var it: TestIterator = .{ .data = &.{ "--selection-invert-fg-bg" } };
            let mut it = TestIterator::new(&[b"--selection-invert-fg-bg"]);
            // try cfg.loadIter(alloc, &it);
            cfg.load_iter(&mut it).unwrap();
            // try cfg.finalize();
            cfg.finalize().unwrap();

            // try testing.expectEqual(TerminalColor.@"cell-background", cfg.@"selection-foreground");
            assert_eq!(
                Some(TerminalColor::CellBackground),
                cfg.selection_foreground
            );
            // try testing.expectEqual(TerminalColor.@"cell-foreground", cfg.@"selection-background");
            assert_eq!(
                Some(TerminalColor::CellForeground),
                cfg.selection_background
            );
        }
    }

    // test "compatibility: removed bold-is-bright" {
    #[test]
    fn compatibility_removed_bold_is_bright() {
        // {
        {
            // var cfg = try Config.default(alloc);
            let mut cfg = Config::default_config();
            // var it: TestIterator = .{ .data = &.{ "--bold-is-bright" } };
            let mut it = TestIterator::new(&[b"--bold-is-bright"]);
            // try cfg.loadIter(alloc, &it);
            cfg.load_iter(&mut it).unwrap();
            // try cfg.finalize();
            cfg.finalize().unwrap();

            // try testing.expectEqual(BoldColor.bright, cfg.@"bold-color");
            assert_eq!(Some(BoldColor::Bright), cfg.bold_color);
        }
    }

    // test "compatibility: window new-window" {
    #[test]
    fn compatibility_window_new_window() {
        // {
        {
            // var cfg = try Config.default(alloc);
            let mut cfg = Config::default_config();
            // var it: TestIterator = .{ .data = &.{ "--macos-dock-drop-behavior=window" } };
            let mut it = TestIterator::new(&[b"--macos-dock-drop-behavior=window"]);
            // try cfg.loadIter(alloc, &it);
            cfg.load_iter(&mut it).unwrap();
            // try cfg.finalize();
            cfg.finalize().unwrap();
            // try testing.expectEqual(MacOSDockDropBehavior.@"new-window", cfg.@"macos-dock-drop-behavior");
            assert_eq!(
                MacOSDockDropBehavior::NewWindow,
                cfg.macos_dock_drop_behavior
            );
        }
    }
}
