//! PORTS: ghostty src/config/Config.zig — the CLONE + CHANGED tests and the `Key`
//! exhaustiveness test.

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
    Key::VtWindowResizeAllowed,
    Key::VtXtChecksumReport,
    Key::VtXtChecksumExtension,
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
    //
    // 209 -> 210: Phase-7 W6 T1 — `vt-window-resize-allowed` (`714fe9b90`,
    // `2a987d59d`).
    //
    // 210 -> 212: Phase-8 W3 T2 — `vt-xt-checksum-report` (`9272a2f7c`) and
    // `vt-xt-checksum-extension` (`00a19db51`).
    assert_eq!(ALL_KEYS.len(), 212);
    assert_eq!(CONFIG_FIELDS.len(), 212);

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
    //
    // `default_config()`, NOT `Config::default()` (parity S77-F2): the
    // reference's `Config.default(alloc)` also installs the default
    // keybinds, the command-palette entries and the URL link, so cloning it
    // is what exercises deep-copying those collections. The bare
    // field-default fixture clones three empty containers.
    let source = Config::default_config();
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
    // var a = try Config.default(alloc);  (see `clone_default`, S77-F2)
    let mut a = Config::default_config();
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
    // var source = try Config.default(alloc);  (see `clone_default`, S77-F2)
    let source = Config::default_config();
    // var dest = try source.clone(alloc);
    let mut dest = source.clone_config();
    // dest.@"font-thicken" = true;
    dest.font_thicken = true;

    // try testing.expect(source.changed(&dest, .@"font-thicken"));
    assert!(source.changed(&dest, Key::FontThicken));
    // try testing.expect(!source.changed(&dest, .@"font-size"));
    assert!(!source.changed(&dest, Key::FontSize));
}
