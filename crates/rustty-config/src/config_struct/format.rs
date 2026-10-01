//! PORTS: ghostty src/config/Config.zig — the FORMAT pass (`formatEntry` over every
//! non-`_` field), plus the bare-enum name maps (`stringToEnum` / `@tagName` /
//! NUL-terminated) for the external enums the fields use and the parse-error maps
//! for the C1 leaf types.

use super::*;

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
            // @"vt-window-resize-allowed": bool
            b"vt-window-resize-allowed" => f_display!(vt_window_resize_allowed),

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
pub(super) fn cursor_style_string_to_enum(input: &[u8]) -> Option<CursorStyle> {
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
pub(super) fn option_as_alt_string_to_enum(
    input: &[u8],
) -> Option<rustty_input::config::OptionAsAlt> {
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
pub(super) fn release_channel_string_to_enum(
    input: &[u8],
) -> Option<rustty_build_config::ReleaseChannel> {
    use rustty_build_config::ReleaseChannel;
    match input {
        b"tip" => Some(ReleaseChannel::Tip),
        b"stable" => Some(ReleaseChannel::Stable),
        _ => None,
    }
}

/// `std.meta.stringToEnum(renderer.size.PaddingBalance, input)`.
// pub const PaddingBalance = enum { false, true, equal };
pub(super) fn window_padding_balance_string_to_enum(input: &[u8]) -> Option<WindowPaddingBalance> {
    match input {
        b"false" => Some(WindowPaddingBalance::False),
        b"true" => Some(WindowPaddingBalance::True),
        b"equal" => Some(WindowPaddingBalance::Equal),
        _ => None,
    }
}

/// `std.meta.stringToEnum(config.AlphaBlending, input)`.
// pub const AlphaBlending = enum { native, linear, "linear-corrected" };
pub(super) fn alpha_blending_string_to_enum(input: &[u8]) -> Option<AlphaBlending> {
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
pub(super) fn window_padding_color_string_to_enum(input: &[u8]) -> Option<WindowPaddingColor> {
    match input {
        b"background" => Some(WindowPaddingColor::Background),
        b"extend" => Some(WindowPaddingColor::Extend),
        b"extend-always" => Some(WindowPaddingColor::ExtendAlways),
        _ => None,
    }
}

/// `std.meta.stringToEnum(config.WindowColorspace, input)`.
// pub const WindowColorspace = enum { srgb, "display-p3" };
pub(super) fn window_colorspace_string_to_enum(input: &[u8]) -> Option<WindowColorspace> {
    match input {
        b"srgb" => Some(WindowColorspace::Srgb),
        b"display-p3" => Some(WindowColorspace::DisplayP3),
        _ => None,
    }
}

/// `std.meta.stringToEnum(config.BackgroundImagePosition, input)`.
// pub const BackgroundImagePosition = enum { "top-left", "top-center", "top-right", "center-left", "center-center", "center-right", "bottom-left", "bottom-center", "bottom-right", center };
pub(super) fn background_image_position_string_to_enum(
    input: &[u8],
) -> Option<BackgroundImagePosition> {
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
pub(super) fn background_image_fit_string_to_enum(input: &[u8]) -> Option<BackgroundImageFit> {
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
pub(super) fn path_parse_err(e: crate::path::ParseError) -> ParseError {
    match e {
        crate::path::ParseError::ValueRequired => Error::ValueRequired,
    }
}

/// Maps `command::ParseError` (`{ValueRequired}`) into the args error union, as
/// the reference propagates `Command.parseCLI`'s error.
pub(super) fn command_parse_err(e: crate::command::ParseError) -> ParseError {
    match e {
        crate::command::ParseError::ValueRequired => Error::ValueRequired,
    }
}

/// Maps `repeatable_string_map::ParseError` (`{ValueRequired}`) into the args
/// error union, as the reference propagates `RepeatableStringMap.parseCLI`.
pub(super) fn rsm_parse_err(e: crate::repeatable_string_map::ParseError) -> ParseError {
    match e {
        crate::repeatable_string_map::ParseError::ValueRequired => Error::ValueRequired,
    }
}

/// Maps `io::ParseError` (`{ValueRequired, InvalidString}`) into the args error
/// union, as the reference propagates `RepeatableReadableIO.parseCLI`. The
/// reference's `InvalidString` is a value-parse failure (`InvalidValue`).
pub(super) fn io_parse_err(e: crate::io::ParseError) -> ParseError {
    match e {
        crate::io::ParseError::ValueRequired => Error::ValueRequired,
        crate::io::ParseError::InvalidString => Error::InvalidValue,
    }
}
