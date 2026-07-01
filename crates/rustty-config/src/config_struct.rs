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
    parse_field_sentinel_string, parse_field_string, Error, ParseError, Target,
};
use rustty_args::diagnostics::DiagnosticList;
use rustty_cfg_fmt::{
    entry_formatter, format_entry_display, format_entry_enum, format_entry_float,
    format_entry_optional, format_entry_string, format_entry_void, FormatEntry, Writer,
};

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
            b"scrollback-limit" => Some(Key::ScrollbackLimit),
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
        Key::ScrollbackLimit => TypeId::of::<usize>(),
        Key::Scrollbar => TypeId::of::<Scrollbar>(),
        Key::Link => TypeId::of::<RepeatableLink>(),
        Key::LinkUrl => TypeId::of::<bool>(),
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
        Key::ResizeOverlay => TypeId::of::<ResizeOverlay>(),
        Key::ResizeOverlayPosition => TypeId::of::<ResizeOverlayPosition>(),
        Key::ResizeOverlayDuration => TypeId::of::<Duration>(),
        Key::FocusFollowsMouse => TypeId::of::<bool>(),
        Key::ClipboardRead => TypeId::of::<ClipboardAccess>(),
        Key::ClipboardWrite => TypeId::of::<ClipboardAccess>(),
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
// PORT support — the std/OS glue + error/iterator types the machinery above
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

/// `std.log.info` / `std.log.warn` scoped `.config`. The parity harness does not
/// assert log output; these go to stderr like the reference's scoped logger.
fn log_info(msg: String) {
    eprintln!("info(config): {msg}");
}

fn log_warn(msg: String) {
    eprintln!("warning(config): {msg}");
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
/// Ghostty in a CLI environment. We need this to change some behaviors.
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

    // If we have TERM_PROGRAM set to a non-empty value, we assume
    // a graphical terminal environment.
    // if (std.posix.getenv("TERM_PROGRAM")) |v| {
    //     if (v.len > 0) return true;
    // }
    if let Ok(v) = std::env::var("TERM_PROGRAM") {
        if !v.is_empty() {
            return true;
        }
    }

    // CLI arguments makes things probable
    // if (std.os.argv.len > 1) return true;
    if std::env::args_os().len() > 1 {
        return true;
    }

    // Unlikely CLI environment
    // return false;
    false
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
    /// configuration file is at `$XDG_CONFIG_HOME/ghostty/config.ghostty`.
    ///
    /// On macOS, `$HOME/Library/Application Support/$CFBundleIdentifier/`
    /// is also loaded.
    ///
    /// The legacy `config` file (without extension) is first loaded,
    /// then `config.ghostty`.
    // pub fn loadDefaultFiles(self: *Config, alloc: Allocator) !void {
    pub fn load_default_files(&mut self) -> Result<(), LoadError> {
        // Load XDG first
        // const legacy_xdg_path = try file_load.legacyDefaultXdgPath(alloc);
        // defer alloc.free(legacy_xdg_path);
        let legacy_xdg_path = crate::file_load::legacy_default_xdg_path()
            .map(String::into_bytes)
            .unwrap_or_default();
        // const xdg_path = try file_load.defaultXdgPath(alloc);
        // defer alloc.free(xdg_path);
        let xdg_path = crate::file_load::default_xdg_path()
            .map(String::into_bytes)
            .unwrap_or_default();
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
        // We always load the theme first because it may set other fields
        // in our config.
        // if (self.theme) |theme| {
        if let Some(theme) = self.theme.clone() {
            // const different = !std.mem.eql(u8, theme.light, theme.dark);
            let different = theme.light != theme.dark;

            // Warning: loadTheme will deinit our existing config and replace
            // it so all memory from self prior to this point will be freed.
            // try self.loadTheme(theme);
            self.load_theme(theme)?;

            // If we have different light vs dark mode themes, disable
            // window-theme = auto since that breaks it.
            // if (different) {
            if different {
                // This setting doesn't make sense with different light/dark themes
                // because it'll force the theme based on the Ghostty theme.
                // if (self.@"window-theme" == .auto) self.@"window-theme" = .system;
                if self.window_theme == WindowTheme::Auto {
                    self.window_theme = WindowTheme::System;
                }

                // Mark that we use a conditional theme
                // self._conditional_set.insert(.theme);
                self._conditional_set.insert(conditional::Key::Theme);
            }
        }

        // const alloc = self._arena.?.allocator();

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
            self.term = b"xterm-ghostty".to_vec();
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

                            // if (std.process.getEnvVarOwned(alloc, "SHELL")) |value| {
                            if let Ok(value) = std::env::var("SHELL") {
                                // log.info("default shell source=env value={s}", .{value});
                                log_info(format!("default shell source=env value={value}"));
                                // const copy = try alloc.dupeZ(u8, value);
                                // self.command = .{ .shell = copy };
                                self.command = Some(Command::Shell(value.into_bytes()));
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
                            if let Ok(Some(home)) = rustty_os::homedir::home(&mut buf) {
                                // wd = .{ .path = try alloc.dupe(u8, home) };
                                wd = WorkingDirectory::Path(home.to_vec());
                            } else {
                                // wd = .inherit;
                                wd = WorkingDirectory::Inherit;
                            }
                        }
                    }
                    #[cfg(not(target_os = "windows"))]
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
        // short value because it can cause Ghostty to quit before the first
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
            Key::ScrollbackLimit => c!(scrollback_limit),
            Key::Scrollbar => c!(scrollbar),
            Key::Link => c!(link),
            Key::LinkUrl => c!(link_url),
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
            Key::ResizeOverlay => c!(resize_overlay),
            Key::ResizeOverlayPosition => c!(resize_overlay_position),
            Key::ResizeOverlayDuration => c!(resize_overlay_duration),
            Key::FocusFollowsMouse => c!(focus_follows_mouse),
            Key::ClipboardRead => c!(clipboard_read),
            Key::ClipboardWrite => c!(clipboard_write),
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
        cv!(scrollback_limit);
        cv!(scrollbar);
        cv!(link);
        cv!(link_url);
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
        cv!(resize_overlay);
        cv!(resize_overlay_position);
        cv!(resize_overlay_duration);
        cv!(focus_follows_mouse);
        cv!(clipboard_read);
        cv!(clipboard_write);
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
/// of names equals `parse_tests::CONFIG_FIELD_NAMES` (203) and the set of keys
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
    (b"scrollback-limit", Key::ScrollbackLimit),
    (b"scrollbar", Key::Scrollbar),
    (b"link", Key::Link),
    (b"link-url", Key::LinkUrl),
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
    (b"resize-overlay", Key::ResizeOverlay),
    (b"resize-overlay-position", Key::ResizeOverlayPosition),
    (b"resize-overlay-duration", Key::ResizeOverlayDuration),
    (b"focus-follows-mouse", Key::FocusFollowsMouse),
    (b"clipboard-read", Key::ClipboardRead),
    (b"clipboard-write", Key::ClipboardWrite),
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
    /// the PARSE dispatch's `parse_into_field` keys; the 203 non-`_` fields each
    /// have exactly one arm. An unknown key is unreachable in the
    /// `FileFormatter` loop (it iterates the real field set), so it is a bug
    /// rather than a recoverable case — `debug_assert`-guarded and a no-op.
    pub(crate) fn format_entry_field<W: Writer>(
        &self,
        key: &[u8],
        writer: &mut W,
    ) -> Result<(), W::Error> {
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
            b"font-shaping-break" => f_entry!(font_shaping_break),

            // @"alpha-blending": AlphaBlending (bare enum)
            b"alpha-blending" => f_enum!(alpha_blending),

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
            // @"background-image-position": BackgroundImagePosition (bare enum)
            b"background-image-position" => f_enum!(background_image_position),
            // @"background-image-fit": BackgroundImageFit (bare enum)
            b"background-image-fit" => f_enum!(background_image_fit),
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
            b"background-blur" => f_entry!(background_blur),

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
            // @"scrollback-limit": usize
            b"scrollback-limit" => f_display!(scrollback_limit),
            // scrollbar: Scrollbar (bare enum)
            b"scrollbar" => f_enum!(scrollbar),

            // link: RepeatableLink (FormatEntry)
            b"link" => f_entry!(link),
            // @"link-url": bool
            b"link-url" => f_display!(link_url),
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
            // @"window-padding-color": WindowPaddingColor (bare enum)
            b"window-padding-color" => f_enum!(window_padding_color),
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
            // @"window-colorspace": WindowColorspace (bare enum)
            b"window-colorspace" => f_enum!(window_colorspace),
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
            _ => {
                debug_assert!(false, "format_entry_field: unknown key");
                Ok(())
            }
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
// `CursorStyle`/`WindowPaddingBalance`/`OptionAsAlt`/`ReleaseChannel`), whose
// impls live in their own crates per the orphan rule. The pass has landed:
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
        Key::ScrollbackLimit,
        Key::Scrollbar,
        Key::Link,
        Key::LinkUrl,
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
        Key::ResizeOverlay,
        Key::ResizeOverlayPosition,
        Key::ResizeOverlayDuration,
        Key::FocusFollowsMouse,
        Key::ClipboardRead,
        Key::ClipboardWrite,
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
    // - `ALL_KEYS` enumerates every variant (its length is checked == 203, the
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
        // `Key`-count contract: 203 variants, matching `CONFIG_FIELDS` and the
        // 203 field names.
        assert_eq!(ALL_KEYS.len(), 203);
        assert_eq!(CONFIG_FIELDS.len(), 203);

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
        let expected: Vec<u8> = rustty_os::expandHome(b"~/projects/ghostty", &mut buf)
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
