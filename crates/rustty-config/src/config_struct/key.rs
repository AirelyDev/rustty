//! PORTS: ghostty src/config/key.zig — the `Key` enum (derived from the `Config`
//! fields), its name lookup, and the `Value` helper (`value_type`), plus
//! `config_fields` and the shell-completion field classification
//! (`ConfigCompletionKind`, `config_completion_kind`) that reify
//! `@typeInfo` over the fields; with key.zig's `test "Value"` (`key_tests`).

use super::*;

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
//     for (field_infos, &names, &raw_values) |field, *name, *raw| {
//         // Ignore fields starting with "_" since they're internal and
//         // not copied ever.
//         if (field.name[0] == '_') continue;
//         ...
//     }
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
    VtWindowResizeAllowed,
    VtXtChecksumReport,
    VtXtChecksumExtension,
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
            b"vt-window-resize-allowed" => Some(Key::VtWindowResizeAllowed),
            b"vt-xt-checksum-extension" => Some(Key::VtXtChecksumExtension),
            b"vt-xt-checksum-report" => Some(Key::VtXtChecksumReport),
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

/// Returns the value type for a key
///
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
        Key::VtWindowResizeAllowed => TypeId::of::<bool>(),
        Key::VtXtChecksumReport => TypeId::of::<bool>(),
        Key::VtXtChecksumExtension => TypeId::of::<XtChecksumExtension>(),
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
        Key::VtXtChecksumExtension => {
            ConfigCompletionKind::PackedFlags(XtChecksumExtension::field_names())
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
