//! PORTS: ghostty src/Surface.zig — the `DerivedConfig` struct (`init`, `deinit`,
//! `scaledPadding`), `DerivedConfigLink`, and the shared-grid-set / FreeType load-flag
//! config derivations.

use super::*;

/// The configuration that a surface has, this is copied from the main
/// Config struct usually to prevent sharing a single value.
// const DerivedConfig = struct { ... };
//
// PORT NOTE: Zig-private type widened to `pub` — it types the pub
// `Surface.config` field. Landed under the C5 field-parity audit: 46
// reference fields == 45 Rust fields + the collapsed `arena` (owned fields
// free on drop — the font-shaper shared_grid_set precedent; `deinit` keeps
// the explicit non-memory teardown).
pub struct DerivedConfig {
    // arena: ArenaAllocator,
    //
    // PORT NOTE (collapsed): every arena-cloned field below is an owned value.

    // For docs for these, see the associated config they are derived from.
    // original_font_size: f32,
    pub original_font_size: f32,
    // keybind: configpkg.Keybinds,
    pub keybind: configpkg::value_types::Keybinds,
    // abnormal_command_exit_runtime_ms: u32,
    pub abnormal_command_exit_runtime_ms: u32,
    // clipboard_read: configpkg.ClipboardAccess,
    pub clipboard_read: configpkg::value_types::ClipboardAccess,
    // clipboard_write: configpkg.ClipboardAccess,
    pub clipboard_write: configpkg::value_types::ClipboardAccess,
    // clipboard_trim_trailing_spaces: bool,
    pub clipboard_trim_trailing_spaces: bool,
    // clipboard_paste_protection: bool,
    pub clipboard_paste_protection: bool,
    // clipboard_paste_bracketed_safe: bool,
    pub clipboard_paste_bracketed_safe: bool,
    // clipboard_codepoint_map: configpkg.Config.RepeatableClipboardCodepointMap,
    pub clipboard_codepoint_map: configpkg::value_types::RepeatableClipboardCodepointMap,
    // copy_on_select: configpkg.CopyOnSelect,
    pub copy_on_select: configpkg::value_types::CopyOnSelect,
    // right_click_action: configpkg.RightClickAction,
    pub right_click_action: configpkg::value_types::RightClickAction,
    // middle_click_action: configpkg.MiddleClickAction,
    pub middle_click_action: configpkg::value_types::MiddleClickAction,
    // confirm_close_surface: configpkg.ConfirmCloseSurface,
    pub confirm_close_surface: configpkg::value_types::ConfirmCloseSurface,
    // cursor_click_to_move: bool,
    pub cursor_click_to_move: bool,
    // desktop_notifications: bool,
    pub desktop_notifications: bool,
    // font: font.SharedGridSet.DerivedConfig,
    pub font: shared_grid_set::DerivedConfig,
    // mouse_interval: u64,
    pub mouse_interval: u64,
    // mouse_hide_while_typing: bool,
    pub mouse_hide_while_typing: bool,
    // mouse_reporting: bool,
    pub mouse_reporting: bool,
    // mouse_scroll_multiplier: configpkg.MouseScrollMultiplier,
    pub mouse_scroll_multiplier: configpkg::value_types::MouseScrollMultiplier,
    // mouse_shift_capture: configpkg.MouseShiftCapture,
    pub mouse_shift_capture: configpkg::value_types::MouseShiftCapture,
    // fullscreen: configpkg.Fullscreen,
    pub fullscreen: configpkg::value_types::Fullscreen,
    // macos_non_native_fullscreen: configpkg.NonNativeFullscreen,
    pub macos_non_native_fullscreen: configpkg::value_types::NonNativeFullscreen,
    // macos_option_as_alt: ?input.OptionAsAlt,
    pub macos_option_as_alt: Option<OptionAsAlt>,
    // selection_clear_on_copy: bool,
    pub selection_clear_on_copy: bool,
    // selection_clear_on_typing: bool,
    pub selection_clear_on_typing: bool,
    // selection_word_chars: []const u21,
    //
    // PORT NOTE: owned `Vec<u21>` (the reference slice lives in the collapsed
    // arena; `u21` is the vt key.rs alias = `u32`).
    pub selection_word_chars: Vec<u21>,
    // vt_kam_allowed: bool,
    pub vt_kam_allowed: bool,
    // wait_after_command: bool,
    pub wait_after_command: bool,
    // window_padding_top: u32,
    pub window_padding_top: u32,
    // window_padding_bottom: u32,
    pub window_padding_bottom: u32,
    // window_padding_left: u32,
    pub window_padding_left: u32,
    // window_padding_right: u32,
    pub window_padding_right: u32,
    // window_padding_balance: configpkg.Config.WindowPaddingBalance,
    pub window_padding_balance: configpkg::value_types::WindowPaddingBalance,
    // window_height: u32,
    pub window_height: u32,
    // window_width: u32,
    pub window_width: u32,
    // title: ?[:0]const u8,
    //
    // PORT NOTE: the reference copies the config's slice (pointer into the
    // config's memory); the owned clone here carries the same bytes without
    // the aliased lifetime (rustty-config stores `Option<Vec<u8>>`).
    pub title: Option<Vec<u8>>,
    // title_report: bool,
    pub title_report: bool,
    // vt_window_resize_allowed: bool,
    pub vt_window_resize_allowed: bool,
    // links: []DerivedConfig.Link,
    pub links: Vec<DerivedConfigLink>,
    // link_osc8: bool,
    pub link_osc8: bool,
    // link_previews: configpkg.LinkPreviews,
    pub link_previews: configpkg::value_types::LinkPreviews,
    // scroll_to_bottom: configpkg.Config.ScrollToBottom,
    pub scroll_to_bottom: configpkg::value_types::ScrollToBottom,
    // notify_on_command_finish: configpkg.Config.NotifyOnCommandFinish,
    pub notify_on_command_finish: configpkg::value_types::NotifyOnCommandFinish,
    // notify_on_command_finish_action: configpkg.Config.NotifyOnCommandFinishAction,
    pub notify_on_command_finish_action: configpkg::value_types::NotifyOnCommandFinishAction,
    // notify_on_command_finish_after: Duration,
    pub notify_on_command_finish_after: Duration,
    // key_remaps: input.KeyRemapSet,
    pub key_remaps: RemapSet,
}

// const Link = struct {
//     regex: oni.Regex,
//     action: input.Link.Action,
//     highlight: input.Link.Highlight,
// };
//
// PORT NOTE: the reference nests this as `DerivedConfig.Link` (and refers to
// it by that qualified name — `links: []DerivedConfig.Link`); Rust has no
// nested type declarations, so the sibling carries the qualified name. (The
// module-level name `Link` is reserved for Surface.zig:4271's file-scope
// `const Link`, the C13 mouse-link type.)
pub struct DerivedConfigLink {
    pub regex: oni::Regex,
    pub action: link::Action,
    pub highlight: link::Highlight,
}

impl DerivedConfig {
    // pub fn init(alloc_gpa: Allocator, config: *const configpkg.Config) !DerivedConfig {
    //
    // PORT NOTE: the inferred error set is the `link.oniRegex()` failures ∪
    // `Allocator.Error`; the latter collapses (infallible Rust allocation —
    // the termio/app.rs convention), leaving `oni::Error`.
    pub fn init(
        alloc_gpa: Allocator,
        config: &configpkg::Config,
    ) -> Result<DerivedConfig, oni::Error> {
        // var arena = ArenaAllocator.init(alloc_gpa);
        // errdefer arena.deinit();
        // const alloc = arena.allocator();
        //
        // PORT NOTE (collapsed arena): the clones below are owned values that
        // free on drop; the errdefer chain is covered by ownership drops on
        // the error path. `alloc_gpa` is kept for signature parity.
        let _ = alloc_gpa;

        // Build all of our links
        let links = {
            // var links: std.ArrayList(DerivedConfig.Link) = .empty;
            // defer links.deinit(alloc);
            let mut links: Vec<DerivedConfigLink> = Vec::new();
            // for (config.link.links.items) |link| {
            for link in &config.link.links {
                // var regex = try link.oniRegex();
                // errdefer regex.deinit();
                //
                // PORT NOTE: the errdefer guards the (infallible-in-Rust)
                // `links.append` below. On a FAILED `oniRegex` the reference
                // frees only the list storage (`defer links.deinit`) — the
                // previously-compiled regexes are NOT deinited on that path —
                // and the `Vec` drop on the `?` unwind reproduces that
                // exactly (`DerivedConfigLink` has no `Drop`; onig handles
                // free only via the explicit deinit chain).
                let regex = link.oni_regex()?;
                // try links.append(alloc, .{
                //     .regex = regex,
                //     .action = link.action,
                //     .highlight = link.highlight,
                // });
                links.push(DerivedConfigLink {
                    regex,
                    action: link.action,
                    highlight: link.highlight,
                });
            }

            // break :links try links.toOwnedSlice(alloc);
            links
        };
        // errdefer {
        //     for (links) |*link| link.regex.deinit();
        //     alloc.free(links);
        // }
        //
        // PORT NOTE: nothing after this point can fail in the port (the
        // remaining `try` sites are Allocator.Error-only clones), so this
        // errdefer has no live unwind path.

        // return .{
        Ok(DerivedConfig {
            // .original_font_size = config.@"font-size",
            original_font_size: config.font_size,
            // .keybind = try config.keybind.clone(alloc),
            keybind: config.keybind.clone_in(),
            // .abnormal_command_exit_runtime_ms = config.@"abnormal-command-exit-runtime",
            abnormal_command_exit_runtime_ms: config.abnormal_command_exit_runtime,
            // .clipboard_read = config.@"clipboard-read",
            clipboard_read: config.clipboard_read,
            // .clipboard_write = config.@"clipboard-write",
            clipboard_write: config.clipboard_write,
            // .clipboard_trim_trailing_spaces = config.@"clipboard-trim-trailing-spaces",
            clipboard_trim_trailing_spaces: config.clipboard_trim_trailing_spaces,
            // .clipboard_paste_protection = config.@"clipboard-paste-protection",
            clipboard_paste_protection: config.clipboard_paste_protection,
            // .clipboard_paste_bracketed_safe = config.@"clipboard-paste-bracketed-safe",
            clipboard_paste_bracketed_safe: config.clipboard_paste_bracketed_safe,
            // .clipboard_codepoint_map = try config.@"clipboard-codepoint-map".clone(alloc),
            clipboard_codepoint_map: config.clipboard_codepoint_map.clone_in(),
            // .copy_on_select = config.@"copy-on-select",
            copy_on_select: config.copy_on_select,
            // .right_click_action = config.@"right-click-action",
            right_click_action: config.right_click_action,
            // .middle_click_action = config.@"middle-click-action",
            middle_click_action: config.middle_click_action,
            // .confirm_close_surface = config.@"confirm-close-surface",
            confirm_close_surface: config.confirm_close_surface,
            // .cursor_click_to_move = config.@"cursor-click-to-move",
            cursor_click_to_move: config.cursor_click_to_move,
            // .desktop_notifications = config.@"desktop-notifications",
            desktop_notifications: config.desktop_notifications,
            // .font = try font.SharedGridSet.DerivedConfig.init(alloc, config),
            //
            // PORT NOTE (break #9 DI realization): the reference init is
            // duck-typed (`config: anytype`) over the GLOBAL config, but
            // rustty-font-shaper's `DerivedConfig::init` takes the
            // shaper-local subset type (that tier must not dep rustty-config
            // — G7 cycle). Core is the first crate that sees both sides, so
            // the field-by-field injection is the module-level bridge below
            // ([`font_shared_grid_set_derived_config`]).
            font: font_shared_grid_set_derived_config(config),
            // .mouse_interval = config.@"click-repeat-interval" * 1_000_000, // 500ms
            //
            // PORT NOTE: reference defect (R13), UPSTREAM-DEFECTS.md UD95. The
            // reference multiplies in `u32` (the config field's type) before the
            // result widens to the `u64` field, so `click-repeat-interval` above
            // 4294 (ms) traps in ReleaseSafe and wraps in ReleaseFast (5000 ms
            // becomes 705032704 ns, measured). The port widens first, which is
            // the reference's evident intent (the field is `u64` nanoseconds);
            // any `u32` millisecond count fits.
            mouse_interval: u64::from(config.click_repeat_interval) * 1_000_000, // 500ms
            // .mouse_hide_while_typing = config.@"mouse-hide-while-typing",
            mouse_hide_while_typing: config.mouse_hide_while_typing,
            // .mouse_reporting = config.@"mouse-reporting",
            mouse_reporting: config.mouse_reporting,
            // .mouse_scroll_multiplier = config.@"mouse-scroll-multiplier",
            mouse_scroll_multiplier: config.mouse_scroll_multiplier,
            // .mouse_shift_capture = config.@"mouse-shift-capture",
            mouse_shift_capture: config.mouse_shift_capture,
            // .fullscreen = config.fullscreen,
            fullscreen: config.fullscreen,
            // .macos_non_native_fullscreen = config.@"macos-non-native-fullscreen",
            macos_non_native_fullscreen: config.macos_non_native_fullscreen,
            // .macos_option_as_alt = config.@"macos-option-as-alt",
            macos_option_as_alt: config.macos_option_as_alt,
            // .selection_clear_on_copy = config.@"selection-clear-on-copy",
            selection_clear_on_copy: config.selection_clear_on_copy,
            // .selection_clear_on_typing = config.@"selection-clear-on-typing",
            selection_clear_on_typing: config.selection_clear_on_typing,
            // .selection_word_chars = try alloc.dupe(u21, config.@"selection-word-chars".codepoints),
            selection_word_chars: config.selection_word_chars.codepoints.clone(),
            // .vt_kam_allowed = config.@"vt-kam-allowed",
            vt_kam_allowed: config.vt_kam_allowed,
            // .wait_after_command = config.@"wait-after-command",
            wait_after_command: config.wait_after_command,
            // .window_padding_top = config.@"window-padding-y".top_left,
            window_padding_top: config.window_padding_y.top_left,
            // .window_padding_bottom = config.@"window-padding-y".bottom_right,
            window_padding_bottom: config.window_padding_y.bottom_right,
            // .window_padding_left = config.@"window-padding-x".top_left,
            window_padding_left: config.window_padding_x.top_left,
            // .window_padding_right = config.@"window-padding-x".bottom_right,
            window_padding_right: config.window_padding_x.bottom_right,
            // .window_padding_balance = config.@"window-padding-balance",
            window_padding_balance: config.window_padding_balance,
            // .window_height = config.@"window-height",
            window_height: config.window_height,
            // .window_width = config.@"window-width",
            window_width: config.window_width,
            // .title = config.title,
            //
            // PORT NOTE: owned clone of the reference's shallow slice copy
            // (see the field's PORT NOTE).
            title: config.title.clone(),
            // .title_report = config.@"title-report",
            title_report: config.title_report,
            // .vt_window_resize_allowed = config.@"vt-window-resize-allowed",
            vt_window_resize_allowed: config.vt_window_resize_allowed,
            // .links = links,
            links,
            // .link_osc8 = config.@"link-osc8",
            link_osc8: config.link_osc8,
            // .link_previews = config.@"link-previews",
            link_previews: config.link_previews,
            // .scroll_to_bottom = config.@"scroll-to-bottom",
            scroll_to_bottom: config.scroll_to_bottom,
            // .notify_on_command_finish = config.@"notify-on-command-finish",
            notify_on_command_finish: config.notify_on_command_finish,
            // .notify_on_command_finish_action = config.@"notify-on-command-finish-action",
            notify_on_command_finish_action: config.notify_on_command_finish_action,
            // .notify_on_command_finish_after = config.@"notify-on-command-finish-after",
            notify_on_command_finish_after: config.notify_on_command_finish_after,
            // .key_remaps = try config.@"key-remap".clone(alloc),
            key_remaps: config.key_remap.clone(),
            // Assignments happen sequentially so we have to do this last
            // so that the memory is captured from allocs above.
            // .arena = arena,
            //
            // PORT NOTE: collapsed (see the struct's arena note); the
            // ordering constraint it documents is moot in Rustty.
        })
    }

    // pub fn deinit(self: *DerivedConfig) void {
    pub fn deinit(&mut self) {
        // for (self.links) |*link| link.regex.deinit();
        for link in &mut self.links {
            link.regex.deinit();
        }
        // self.arena.deinit();
        // PORT NOTE: the owned fields free on drop (the collapsed arena).
    }

    // fn scaledPadding(self: *const DerivedConfig, x_dpi: f32, y_dpi: f32) rendererpkg.Padding {
    //
    // (first callsite: `Surface::init`, chunk C6 — Surface.zig:536; more land
    // C8 :2514 / C11 :3630.)
    //
    // PORT NOTE: reference defect (R13), UD71. Each side is
    // `@intFromFloat(@floor(padding * dpi / 72))`, a safety-checked cast of a
    // config value (`window-padding-x`/`-y`, any `u32`) scaled by the
    // embedder's content scale. `window-padding-x = 4294967295` at 72 dpi, or
    // 3000000000 at 144 dpi (Retina), is out of `u32`: ReleaseSafe panics
    // "integer part of floating point value out of bounds" at startup,
    // ReleaseFast yields 4294967295 (measured, Zig 0.16.0). The port's `as u32`
    // saturates to that same 4294967295 — a padding "as large as it goes",
    // which the reference already treats as a padding wider than the window
    // (one-cell grid, the "Is your padding reasonable?" warning). A NaN dpi
    // gives 0 in both trees. What made the saturated value safe downstream is
    // the saturating padding sums in `rustty-render-types` (`sub_padding`,
    // `blank_padding`, `balance_padding`), which overflowed before, and the
    // saturating cursor-position sums in `imePoint` and the custom-shader
    // cursor uniforms (UD76, task R13-C2), which a saturated padding stored by
    // `contentScaleCallback` reached before the IO thread's resize.
    pub(super) fn scaledPadding(&self, x_dpi: f32, y_dpi: f32) -> rendererpkg::size::Padding {
        // const padding_top: u32 = padding_top: {
        //     const padding_top: f32 = @floatFromInt(self.window_padding_top);
        //     break :padding_top @intFromFloat(@floor(padding_top * y_dpi / 72));
        // };
        let padding_top: u32 = {
            let padding_top: f32 = self.window_padding_top as f32;
            (padding_top * y_dpi / 72.0).floor() as u32
        };
        // const padding_bottom: u32 = padding_bottom: {
        //     const padding_bottom: f32 = @floatFromInt(self.window_padding_bottom);
        //     break :padding_bottom @intFromFloat(@floor(padding_bottom * y_dpi / 72));
        // };
        let padding_bottom: u32 = {
            let padding_bottom: f32 = self.window_padding_bottom as f32;
            (padding_bottom * y_dpi / 72.0).floor() as u32
        };
        // const padding_left: u32 = padding_left: {
        //     const padding_left: f32 = @floatFromInt(self.window_padding_left);
        //     break :padding_left @intFromFloat(@floor(padding_left * x_dpi / 72));
        // };
        let padding_left: u32 = {
            let padding_left: f32 = self.window_padding_left as f32;
            (padding_left * x_dpi / 72.0).floor() as u32
        };
        // const padding_right: u32 = padding_right: {
        //     const padding_right: f32 = @floatFromInt(self.window_padding_right);
        //     break :padding_right @intFromFloat(@floor(padding_right * x_dpi / 72));
        // };
        let padding_right: u32 = {
            let padding_right: f32 = self.window_padding_right as f32;
            (padding_right * x_dpi / 72.0).floor() as u32
        };

        // return .{
        //     .top = padding_top,
        //     .bottom = padding_bottom,
        //     .left = padding_left,
        //     .right = padding_right,
        // };
        rendererpkg::size::Padding {
            top: padding_top,
            bottom: padding_bottom,
            left: padding_left,
            right: padding_right,
        }
    }
}

/// PORT NOTE (Rust-only glue, break #9 DI realization — see the
/// `DerivedConfig::init` `.font` assignment): builds the rustty-font-shaper
/// `SharedGridSet.DerivedConfig` from the global config. The field list
/// mirrors the reference `SharedGridSet.zig DerivedConfig.init` assignments
/// one-for-one (== rustty-font-shaper shared_grid_set.rs `DerivedConfig::
/// init`, whose duck-typed `config: anytype` source is the global config at
/// this callsite). The per-type conversions are exact: both sides' font
/// subset types model the SAME reference config types (`RepeatableString`,
/// `FontStyle`, `RepeatableFontVariation`, `RepeatableCodepointMap`,
/// `FontSyntheticStyle`), duplicated across the tier boundary by break #9;
/// `Modifier` and `CodepointMap` are literally the same rustty-font types on
/// both sides.
pub fn font_shared_grid_set_derived_config(
    config: &configpkg::Config,
) -> shared_grid_set::DerivedConfig {
    use rustty_font_shaper::config as font_config;

    // config.RepeatableString -> shaper RepeatableString: same `list` payload
    // (`overwrite_next` is a CLI-parse artifact the shaper subset never had).
    fn repeatable_string(
        v: &configpkg::value_types::RepeatableString,
    ) -> font_config::RepeatableString {
        font_config::RepeatableString {
            list: v.list.clone(),
        }
    }

    // config.FontStyle -> shaper FontStyle: identical variants.
    fn font_style(v: &configpkg::value_types::FontStyle) -> font_config::FontStyle {
        match v {
            configpkg::value_types::FontStyle::Default => font_config::FontStyle::Default,
            configpkg::value_types::FontStyle::False => font_config::FontStyle::False,
            configpkg::value_types::FontStyle::Name(name) => {
                font_config::FontStyle::Name(name.clone())
            }
        }
    }

    // config.RepeatableFontVariation -> shaper RepeatableFontVariation: the
    // config side models `Variation.Id` as the packed `u32`
    // (`a<<24 | b<<16 | c<<8 | d` — value_types.rs `FaceVariation`); the
    // shaper side keeps the reference's field-wise `Id`. Same value, unpacked.
    fn font_variation(
        v: &configpkg::value_types::RepeatableFontVariation,
    ) -> font_config::RepeatableFontVariation {
        font_config::RepeatableFontVariation {
            list: v
                .list
                .iter()
                .map(|variation| rustty_font_shaper::face::Variation {
                    id: rustty_font_shaper::face::Id {
                        a: (variation.id >> 24) as u8,
                        b: (variation.id >> 16) as u8,
                        c: (variation.id >> 8) as u8,
                        d: variation.id as u8,
                    },
                    value: variation.value,
                })
                .collect(),
        }
    }

    shared_grid_set::DerivedConfig {
        // .@"font-family" = try config.@"font-family".clone(alloc), (×4 styles)
        font_family: repeatable_string(&config.font_family),
        font_family_bold: repeatable_string(&config.font_family_bold),
        font_family_italic: repeatable_string(&config.font_family_italic),
        font_family_bold_italic: repeatable_string(&config.font_family_bold_italic),
        // .@"font-style" = try config.@"font-style".clone(alloc), (×4 styles)
        font_style: font_style(&config.font_style),
        font_style_bold: font_style(&config.font_style_bold),
        font_style_italic: font_style(&config.font_style_italic),
        font_style_bold_italic: font_style(&config.font_style_bold_italic),
        // .@"font-variation" = try config.@"font-variation".clone(alloc), (×4 styles)
        font_variation: font_variation(&config.font_variation),
        font_variation_bold: font_variation(&config.font_variation_bold),
        font_variation_italic: font_variation(&config.font_variation_italic),
        font_variation_bold_italic: font_variation(&config.font_variation_bold_italic),
        // .@"font-codepoint-map" = try config.@"font-codepoint-map".clone(alloc),
        //
        // PORT NOTE: `CodepointMap` is the same rustty-font type on both
        // sides; deep-cloned exactly as the config crate's own `clone_in`
        // (Global aborts on OOM — the reference returns the error).
        font_codepoint_map: font_config::RepeatableCodepointMap {
            map: config
                .font_codepoint_map
                .map
                .clone(Allocator)
                .expect("CodepointMap.clone: Global allocator aborts on OOM"),
        },
        // .@"font-synthetic-style" = config.@"font-synthetic-style",
        font_synthetic_style: rustty_font_shaper::collection::FontSyntheticStyle {
            bold: config.font_synthetic_style.bold,
            italic: config.font_synthetic_style.italic,
            bold_italic: config.font_synthetic_style.bold_italic,
        },
        // .@"adjust-cell-width" = config.@"adjust-cell-width", (×13 metrics —
        // `Modifier` is the same rustty-font type on both sides, `Copy`)
        adjust_cell_width: config.adjust_cell_width,
        adjust_cell_height: config.adjust_cell_height,
        adjust_font_baseline: config.adjust_font_baseline,
        adjust_underline_position: config.adjust_underline_position,
        adjust_underline_thickness: config.adjust_underline_thickness,
        adjust_strikethrough_position: config.adjust_strikethrough_position,
        adjust_strikethrough_thickness: config.adjust_strikethrough_thickness,
        adjust_overline_position: config.adjust_overline_position,
        adjust_overline_thickness: config.adjust_overline_thickness,
        adjust_cursor_thickness: config.adjust_cursor_thickness,
        adjust_cursor_height: config.adjust_cursor_height,
        adjust_box_thickness: config.adjust_box_thickness,
        adjust_icon_height: config.adjust_icon_height,
        // .@"freetype-load-flags" = if (FreetypeLoadFlags != void) config.@"freetype-load-flags" else {},
        freetype_load_flags: derived_freetype_load_flags(&config.freetype_load_flags),
    }
}

/// The field-for-field projection of the global config's
/// `config.FreetypeLoadFlags` onto the shaper tier's stand-in for that same
/// reference type (`face::freetype::FreetypeLoadFlags`).
///
/// PORT NOTE (Rust-only glue, break #9 DI realization): in the reference these
/// are ONE type — `font.face.FreetypeLoadFlags` IS `config.FreetypeLoadFlags`
/// on every freetype-active backend, so `SharedGridSet.zig:528` can assign it
/// straight across. rustty's break #9 keeps `rustty-font-shaper` off
/// `rustty-config`, so the shaper declares a structurally identical stand-in
/// (`face/freetype.rs`) and this function is the DI bridge. It is the only
/// place the two spellings meet; every field must be threaded or the user's
/// `freetype-load-flags` setting is silently dropped.
//
// Compiled on every non-wasm target (the shaper's `freetype` module is likewise
// `cfg(not(wasm32))`), so it is unit-testable on macOS even though macOS's
// active backend is `coretext` and takes the `void` arm below.
#[cfg(not(target_arch = "wasm32"))]
pub fn freetype_load_flags_from_config(
    c: &configpkg::value_types::FreetypeLoadFlags,
) -> rustty_font_shaper::face::freetype::FreetypeLoadFlags {
    rustty_font_shaper::face::freetype::FreetypeLoadFlags {
        hinting: c.hinting,
        // PORT NOTE: Zig field `@"force-autohint"`.
        force_autohint: c.force_autohint,
        monochrome: c.monochrome,
        autohint: c.autohint,
        light: c.light,
    }
}

/// `if (font.face.FreetypeLoadFlags != void) config.@"freetype-load-flags" else {}`
/// — the reference's comptime backend selector. The shaper tier owns the
/// `options.backend.hasFreetype()` decision (`face::freetype_load_flags_if_active`):
/// on the macOS default `coretext` backend it is false → `void` (Rust `()`) and
/// the config value is legitimately dropped (the reference `else {}` arm); on
/// the Linux default `fontconfig_freetype` and on `-Dfont-backend=
/// coretext_freetype` it is true and the configured value flows through.
/// (S-14, N266: this was two `cfg(target_os)` arms, which dropped the user's
/// `freetype-load-flags` in a macOS `coretext_freetype` build.)
#[cfg(not(target_arch = "wasm32"))]
fn derived_freetype_load_flags(
    c: &configpkg::value_types::FreetypeLoadFlags,
) -> rustty_font_shaper::face::FreetypeLoadFlags {
    rustty_font_shaper::face::freetype_load_flags_if_active(|| freetype_load_flags_from_config(c))
}

#[cfg(target_arch = "wasm32")]
fn derived_freetype_load_flags(
    _c: &configpkg::value_types::FreetypeLoadFlags,
) -> rustty_font_shaper::face::FreetypeLoadFlags {
    // The `web_canvas` backend's `hasFreetype()` is likewise false; the shaper's
    // wasm placeholder is the `void` arm.
    rustty_font_shaper::face::FREETYPE_LOAD_FLAGS_DEFAULT
}
