//! PORTS: ghostty src/config/Config.zig — the `cli.args` `Target` impl: the PARSE
//! pass per-field dispatch (`parseIntoField`), `compatibility`,
//! `parseManuallyHook` and the replay recording.

use super::*;

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

    /// PORTS: `formatValues(Config, key, writer)` (`cli/args.zig`): for the field
    /// named `key`, if its `@typeInfo` is `.@"enum"`, the `, valid values are:
    /// ...` suffix. `config_completion_kind`'s `Enum` arm is exactly that set of
    /// fields (the same `@typeInfo(field.type)` switch drives the reference's
    /// completions), with each list in declaration order (parity S67-F1).
    fn format_values(&self, key: &[u8]) -> Vec<u8> {
        match Key::string_to_enum(key).map(config_completion_kind) {
            Some(ConfigCompletionKind::Enum(names)) => rustty_args::args::format_enum_values(names),
            _ => Vec::new(),
        }
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
        // `if (field.name[0] != '_' and mem.eql(u8, field.name, key))` — the
        // reference's `parseIntoField` SKIPS every underscore-prefixed field, so
        // a `_`-prefixed key matches nothing and falls out as `InvalidField`.
        // This Config is the Target that HAS such fields (`_arena`,
        // `_diagnostics`, `_replay_steps`, `_conditional_state`, …), so the rule
        // is enforced here explicitly rather than left implicit in "the match
        // arms below happen not to list them" (parity S67-F6).
        if is_reserved_key(key) {
            return Err(Error::InvalidField);
        }

        // The empty-string reset: `if (value) |v| { if (v.len == 0) <reset> }`.
        // For a field whose type has no `init` decl, the reset is to the struct's
        // declared default (`field.default_value_ptr`), which `Config::default()`
        // materializes. We compute the empty flag once; each arm consults it.
        let is_empty = is_reset_to_default(value);

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
            // @"vt-window-resize-allowed": bool
            b"vt-window-resize-allowed" => field_bool!(vt_window_resize_allowed),

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
