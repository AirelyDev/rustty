//! PORTS: ghostty src/config/Config.zig — the PARSE pass tests.

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
    b"vt-window-resize-allowed",
    b"vt-xt-checksum-extension",
    b"vt-xt-checksum-report",
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
    // 209 -> 210: Phase-7 W6 T1 added `vt-window-resize-allowed`
    // (`714fe9b90`, moved next to `vt-kam-allowed` by `2a987d59d`).
    // 210 -> 212: Phase-8 W3 T2 added `vt-xt-checksum-report` (`9272a2f7c`)
    // and `vt-xt-checksum-extension` (`00a19db51`).
    assert_eq!(CONFIG_FIELD_NAMES.len(), 212);
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

// Underscore-prefixed (internal) fields are never matched. Since parity
// S67-F6 that is an EXPLICIT `is_reserved_key` guard at the top of
// `parse_into_field` (the shared library predicate the rustty-args test for
// this rule also drives) rather than only the implicit "they have no arm,
// so they fall through to `InvalidField` like an unknown name". Removing
// the guard today changes nothing observable — no arm names a `_` key —
// which is exactly why the rule needs somewhere explicit to live.
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
