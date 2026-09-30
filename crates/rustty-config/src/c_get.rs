//! PORTS: ghostty `src/config/c_get.zig` — the typed `config_get` over the
//! `Key` registry, the read half of the `ghostty_config_get` C ABI.
//!
//! PORT NOTE: the reference `get` is one comptime `inline else` over every `Key`,
//! resolving each to its Config field (`fieldByKey`) and writing it through
//! `getValue`'s `@TypeOf`/`@typeInfo` type-switch. Rust has no comptime field
//! reflection or value type-switch, so this is rendered as an explicit `match k`
//! with one arm per non-`_` Config field (the `fieldByKey` resolution) calling a
//! per-type-category macro (the `getValue` arms). The category each field falls
//! into is fixed by its Rust type, exactly as the reference's `getValue`
//! `@TypeOf(value)`/`@typeInfo(T)` dispatch fixes it:
//!
//!   - `?[:0]const u8`          -> `g_optstr!`   (null pointer for `None`)
//!   - `bool`                   -> `g_bool!`     (`*bool`)
//!   - `u8`/`u32`               -> `g_uint!`     (`*c_uint`)
//!   - `i16` (via optional)     -> `g_opt_i16!`  (`*c_short`)
//!   - `f32`/`f64`              -> `g_float!`    (`*Float`)
//!   - `.optional`              -> the `g_opt_*` arms (unwrap or `false`)
//!   - `.@"enum"`               -> `g_enum!` / `g_enum_fn!` (`@tagName` -> `*[*:0]const u8`)
//!   - struct/union with `cval` -> `g_cval!` / `g_opt_cval!` (write `value.cval()`)
//!   - packed struct            -> `g_packed!` (backing integer -> `*c_uint`)
//!   - anything else            -> `false` (not yet supported by the C API)

use crate::config_struct::{
    alpha_blending_to_name_z, background_image_fit_to_name_z, background_image_position_to_name_z,
    cursor_style_to_name_z, option_as_alt_to_name_z, release_channel_to_name_z,
    window_colorspace_to_name_z, window_padding_balance_to_name_z, window_padding_color_to_name_z,
    Config, Key,
};
// Chunk-3a-hoisted value-types: their C-API numeric projections (`cval`/`to_c_uint`)
// are config-local free fns in `value_types` (the inherent methods would orphan on the
// foreign render-types type).
use crate::value_types::{background_blur_cval, font_shaping_break_to_c_uint};

/// Get a value from the config by key into the given pointer. This is
/// specifically for C-compatible APIs. If you're using Zig, just access
/// the configuration directly.
///
/// The return value is false if the given key is not supported by the
/// C API yet. This is a fixable problem so if it is important to support
/// some key, please open an issue.
///
/// # Safety
/// `ptr_raw` must point to writable storage whose type matches the C
/// projection of `k`'s value type (the same contract as the reference's
/// `*anyopaque` out-pointer): e.g. `bool` for a bool key, `c_uint` for an
/// integer/packed key, `*const c_char` for a string/enum key, the type's `.C`
/// struct for a `cval` key. A mismatch is undefined behavior, exactly as in the
/// reference.
// pub fn get(config: *const Config, k: Key, ptr_raw: *anyopaque) bool {
pub unsafe fn get(config: &Config, k: Key, ptr_raw: *mut std::ffi::c_void) -> bool {
    // @setEvalBranchQuota(10_000); — comptime-only in the reference.

    // PORT NOTE: the macros below are the per-arm rendering of the reference's
    // `getValue`:
    //
    //   /// Get the value anytype and put it into the pointer. Returns false if
    //   /// the type is not supported by the C API yet or the value is null.
    //   fn getValue(ptr_raw: *anyopaque, value: anytype) bool { switch (...) }
    //
    // Each `match k` arm resolves the field (`fieldByKey`) and routes it to the
    // macro for its type, which writes through `ptr_raw` and returns whether the
    // type/value is supported — exactly `getValue`'s contract.

    // `bool => { const ptr: *bool = @ptrCast(@alignCast(ptr_raw)); ptr.* = value; }`
    macro_rules! g_bool {
        ($f:ident) => {{
            unsafe {
                *(ptr_raw as *mut bool) = config.$f;
            }
            true
        }};
    }
    // `u8, u32 => { const ptr: *c_uint = ...; ptr.* = @intCast(value); }`
    macro_rules! g_uint {
        ($f:ident) => {{
            unsafe {
                *(ptr_raw as *mut std::os::raw::c_uint) = config.$f as std::os::raw::c_uint;
            }
            true
        }};
    }
    // `f32, f64 => |Float| { const ptr: *Float = ...; ptr.* = @floatCast(value); }`
    // (`@floatCast` is identity here — the out type is the field's own float type.)
    macro_rules! g_float {
        ($f:ident, $t:ty) => {{
            unsafe {
                *(ptr_raw as *mut $t) = config.$f;
            }
            true
        }};
    }
    // `?[:0]const u8 => { const ptr: *?[*:0]const u8 = ...; ptr.* = if value
    // |slice| slice.ptr else null; }` — a nullable C string pointer.
    //
    // PORT NOTE: the rustty `?[:0]const u8` representation is `Option<Vec<u8>>`;
    // the `Some` pointer is the `Vec`'s data pointer. (The reference's sentinel
    // guarantee on the slice is the shared property of these string fields; this
    // arm only projects the pointer/null discriminant, exactly as the reference.)
    macro_rules! g_optstr {
        ($f:ident) => {{
            let p = ptr_raw as *mut *const std::os::raw::c_char;
            unsafe {
                *p = match &config.$f {
                    Some(s) => s.as_ptr().cast(),
                    None => std::ptr::null(),
                };
            }
            true
        }};
    }
    // `.@"enum" => { const ptr: *[*:0]const u8 = ...; ptr.* = @tagName(value); }`
    macro_rules! g_enum {
        ($f:ident) => {{
            unsafe {
                *(ptr_raw as *mut *const std::os::raw::c_char) = config.$f.to_name_z();
            }
            true
        }};
    }
    // `.@"enum"` for a foreign enum whose `@tagName` map lives as a free fn.
    macro_rules! g_enum_fn {
        ($f:ident, $namez:expr) => {{
            unsafe {
                *(ptr_raw as *mut *const std::os::raw::c_char) = $namez(config.$f);
            }
            true
        }};
    }
    // `.optional` over a bare enum: unwrap (`@tagName`) or `false`.
    macro_rules! g_opt_enum {
        ($f:ident) => {
            match config.$f {
                Some(v) => {
                    unsafe {
                        *(ptr_raw as *mut *const std::os::raw::c_char) = v.to_name_z();
                    }
                    true
                }
                None => false,
            }
        };
    }
    // `.optional` over a foreign bare enum.
    macro_rules! g_opt_enum_fn {
        ($f:ident, $namez:expr) => {
            match config.$f {
                Some(v) => {
                    unsafe {
                        *(ptr_raw as *mut *const std::os::raw::c_char) = $namez(v);
                    }
                    true
                }
                None => false,
            }
        };
    }
    // `.optional` over `bool`: unwrap (`*bool`) or `false`.
    macro_rules! g_opt_bool {
        ($f:ident) => {
            match config.$f {
                Some(v) => {
                    unsafe {
                        *(ptr_raw as *mut bool) = v;
                    }
                    true
                }
                None => false,
            }
        };
    }
    // `.optional` over `i16`: unwrap (`*c_short`, `@intCast`) or `false`.
    macro_rules! g_opt_i16 {
        ($f:ident) => {
            match config.$f {
                Some(v) => {
                    unsafe {
                        *(ptr_raw as *mut std::os::raw::c_short) = v;
                    }
                    true
                }
                None => false,
            }
        };
    }
    // struct/union with a `cval` decl: `const ptr: *PtrT = ...; ptr.* =
    // value.cval(); return true;` — `PtrT` is `cval`'s return type, inferred here.
    //
    // If the struct implements cval then we call then.
    macro_rules! g_cval {
        ($f:ident) => {{
            let cv = config.$f.cval();
            unsafe {
                std::ptr::write(ptr_raw.cast(), cv);
            }
            true
        }};
    }
    // `cval` for a Chunk-3a-hoisted type whose `cval` is now a config-local free fn
    // (the inherent method would orphan on the foreign render-types type).
    macro_rules! g_cval_fn {
        ($f:ident, $cval_fn:expr) => {{
            let cv = $cval_fn(config.$f);
            unsafe {
                std::ptr::write(ptr_raw.cast(), cv);
            }
            true
        }};
    }
    // `.optional` over a `cval` struct/union: unwrap (write `cval()`) or `false`.
    //
    // If an optional has no value we return false.
    macro_rules! g_opt_cval {
        ($f:ident) => {
            match &config.$f {
                Some(v) => {
                    let cv = v.cval();
                    unsafe {
                        std::ptr::write(ptr_raw.cast(), cv);
                    }
                    true
                }
                None => false,
            }
        };
    }
    // Packed structs that are less than or equal to the
    // size of a C int can be passed directly as their
    // bit representation.
    // packed struct (backing integer <= `c_uint`): `const ptr: *c_uint = ...;
    // ptr.* = @intCast(@as(Backing, @bitCast(value)));`
    macro_rules! g_packed {
        ($f:ident) => {{
            unsafe {
                *(ptr_raw as *mut std::os::raw::c_uint) = config.$f.to_c_uint();
            }
            true
        }};
    }
    // packed struct whose `to_c_uint` is now a config-local free fn (a Chunk-3a-hoisted
    // type: the `packed_to_c_uint!`-generated inherent method would orphan).
    macro_rules! g_packed_fn {
        ($f:ident, $bits_fn:expr) => {{
            unsafe {
                *(ptr_raw as *mut std::os::raw::c_uint) = $bits_fn(config.$f);
            }
            true
        }};
    }

    // switch (k) { inline else => |tag| { ... fieldByKey ... getValue ... } }
    //
    // Each arm is the reference's `fieldByKey(self, tag)` followed by
    // `getValue(ptr_raw, value)`:
    //
    //   /// Get a value from the config by key.
    //   fn fieldByKey(self: *const Config, comptime k: Key) Value(k) { ... }
    match k {
        Key::AbnormalCommandExitRuntime => g_uint!(abnormal_command_exit_runtime),
        Key::AdjustBoxThickness => false,
        Key::AdjustCellHeight => false,
        Key::AdjustCellWidth => false,
        Key::AdjustCursorHeight => false,
        Key::AdjustCursorThickness => false,
        Key::AdjustFontBaseline => false,
        Key::AdjustIconHeight => false,
        Key::AdjustOverlinePosition => false,
        Key::AdjustOverlineThickness => false,
        Key::AdjustStrikethroughPosition => false,
        Key::AdjustStrikethroughThickness => false,
        Key::AdjustUnderlinePosition => false,
        Key::AdjustUnderlineThickness => false,
        Key::AlphaBlending => g_enum_fn!(alpha_blending, alpha_blending_to_name_z),
        Key::AppNotifications => g_packed!(app_notifications),
        Key::AsyncBackend => g_enum!(async_backend),
        Key::AutoUpdate => g_opt_enum!(auto_update),
        Key::AutoUpdateChannel => g_opt_enum_fn!(auto_update_channel, release_channel_to_name_z),
        Key::Background => g_cval!(background),
        Key::BackgroundBlur => g_cval_fn!(background_blur, background_blur_cval),
        Key::BackgroundImage => g_opt_cval!(background_image),
        Key::BackgroundImageFit => g_enum_fn!(background_image_fit, background_image_fit_to_name_z),
        Key::BackgroundImageOpacity => g_float!(background_image_opacity, f32),
        Key::BackgroundImagePosition => {
            g_enum_fn!(
                background_image_position,
                background_image_position_to_name_z
            )
        }
        Key::BackgroundImageRepeat => g_bool!(background_image_repeat),
        Key::BackgroundOpacity => g_float!(background_opacity, f64),
        Key::BackgroundOpacityCells => g_bool!(background_opacity_cells),
        Key::BellAudioPath => g_opt_cval!(bell_audio_path),
        Key::BellAudioVolume => g_float!(bell_audio_volume, f64),
        Key::BellFeatures => g_packed!(bell_features),
        Key::BoldColor => false,
        Key::Class => g_optstr!(class),
        Key::ClickRepeatInterval => g_uint!(click_repeat_interval),
        Key::ClipboardCodepointMap => false,
        Key::ClipboardPasteBracketedSafe => g_bool!(clipboard_paste_bracketed_safe),
        Key::ClipboardPasteProtection => g_bool!(clipboard_paste_protection),
        Key::ClipboardRead => g_enum!(clipboard_read),
        Key::ClipboardTrimTrailingSpaces => g_bool!(clipboard_trim_trailing_spaces),
        Key::ClipboardWrite => g_enum!(clipboard_write),
        Key::ClipboardWriteLimitBytes => false,
        Key::Command => false,
        Key::CommandPaletteEntry => g_cval!(command_palette_entry),
        Key::ConfigDefaultFiles => g_bool!(config_default_files),
        Key::ConfigFile => false,
        Key::ConfirmCloseSurface => g_enum!(confirm_close_surface),
        Key::CopyOnSelect => g_enum!(copy_on_select),
        Key::CursorClickToMove => g_bool!(cursor_click_to_move),
        Key::CursorColor => false,
        Key::CursorOpacity => g_float!(cursor_opacity, f64),
        Key::CursorStyle => g_enum_fn!(cursor_style, cursor_style_to_name_z),
        Key::CursorStyleBlink => g_opt_bool!(cursor_style_blink),
        Key::CursorText => false,
        Key::CustomShader => false,
        Key::CustomShaderAnimation => g_enum!(custom_shader_animation),
        Key::DesktopNotifications => g_bool!(desktop_notifications),
        Key::DragHandle => g_enum!(drag_handle),
        Key::EnquiryResponse => false,
        Key::Env => false,
        Key::FaintOpacity => g_float!(faint_opacity, f64),
        Key::FocusFollowsMouse => g_bool!(focus_follows_mouse),
        Key::FontCodepointMap => false,
        Key::FontFamily => false,
        Key::FontFamilyBold => false,
        Key::FontFamilyBoldItalic => false,
        Key::FontFamilyItalic => false,
        Key::FontFeature => false,
        Key::FontShapingBreak => g_packed_fn!(font_shaping_break, font_shaping_break_to_c_uint),
        Key::FontSize => g_float!(font_size, f32),
        Key::FontStyle => false,
        Key::FontStyleBold => false,
        Key::FontStyleBoldItalic => false,
        Key::FontStyleItalic => false,
        Key::FontSyntheticStyle => g_packed!(font_synthetic_style),
        Key::FontThicken => g_bool!(font_thicken),
        Key::FontThickenStrength => g_uint!(font_thicken_strength),
        Key::FontVariation => false,
        Key::FontVariationBold => false,
        Key::FontVariationBoldItalic => false,
        Key::FontVariationItalic => false,
        Key::Foreground => g_cval!(foreground),
        Key::FreetypeLoadFlags => g_packed!(freetype_load_flags),
        Key::Fullscreen => g_enum!(fullscreen),
        Key::GraphemeWidthMethod => g_enum!(grapheme_width_method),
        Key::GtkCustomCss => false,
        Key::GtkOpenglDebug => g_bool!(gtk_opengl_debug),
        Key::GtkQuickTerminalLayer => g_enum!(gtk_quick_terminal_layer),
        Key::GtkQuickTerminalNamespace => false,
        Key::GtkSingleInstance => g_enum!(gtk_single_instance),
        Key::GtkTabsLocation => g_enum!(gtk_tabs_location),
        Key::GtkTitlebar => g_bool!(gtk_titlebar),
        Key::GtkTitlebarHideWhenMaximized => g_bool!(gtk_titlebar_hide_when_maximized),
        Key::GtkTitlebarStyle => g_enum!(gtk_titlebar_style),
        Key::GtkToolbarStyle => g_enum!(gtk_toolbar_style),
        Key::GtkWideTabs => g_bool!(gtk_wide_tabs),
        Key::GtkHorizontalTabScroll => g_bool!(gtk_horizontal_tab_scroll),
        Key::ImageStorageLimit => g_uint!(image_storage_limit),
        Key::InitialCommand => false,
        Key::InitialWindow => g_bool!(initial_window),
        Key::Input => false,
        Key::KeyRemap => false,
        Key::Keybind => false,
        Key::Language => g_optstr!(language),
        Key::Link => false,
        Key::LinkOsc8 => g_bool!(link_osc8),
        Key::LinkPreviews => g_enum!(link_previews),
        Key::LinkUrl => g_bool!(link_url),
        Key::LinuxCgroup => g_enum!(linux_cgroup),
        Key::LinuxCgroupHardFail => g_bool!(linux_cgroup_hard_fail),
        Key::LinuxCgroupMemoryLimit => false,
        Key::LinuxCgroupProcessesLimit => false,
        Key::MacosApplescript => g_bool!(macos_applescript),
        Key::MacosAutoSecureInput => g_bool!(macos_auto_secure_input),
        Key::MacosCustomIcon => g_optstr!(macos_custom_icon),
        Key::MacosDockDropBehavior => g_enum!(macos_dock_drop_behavior),
        Key::MacosHidden => g_enum!(macos_hidden),
        Key::MacosIcon => g_enum!(macos_icon),
        Key::MacosIconFrame => g_enum!(macos_icon_frame),
        Key::MacosIconGhostColor => g_opt_cval!(macos_icon_ghost_color),
        Key::MacosIconScreenColor => g_opt_cval!(macos_icon_screen_color),
        Key::MacosNonNativeFullscreen => g_enum!(macos_non_native_fullscreen),
        Key::MacosOptionAsAlt => g_opt_enum_fn!(macos_option_as_alt, option_as_alt_to_name_z),
        Key::MacosSecureInputIndication => g_bool!(macos_secure_input_indication),
        Key::MacosShortcuts => g_enum!(macos_shortcuts),
        Key::MacosTitlebarProxyIcon => g_enum!(macos_titlebar_proxy_icon),
        Key::MacosTitlebarStyle => g_enum!(macos_titlebar_style),
        Key::MacosWindowButtons => g_enum!(macos_window_buttons),
        Key::MacosWindowShadow => g_bool!(macos_window_shadow),
        Key::Maximize => g_bool!(maximize),
        Key::MiddleClickAction => g_enum!(middle_click_action),
        Key::MinimumContrast => g_float!(minimum_contrast, f64),
        Key::MouseHideWhileTyping => g_bool!(mouse_hide_while_typing),
        Key::MouseReporting => g_bool!(mouse_reporting),
        Key::MouseScrollMultiplier => false,
        Key::MouseShiftCapture => g_enum!(mouse_shift_capture),
        Key::NotifyOnCommandFinish => g_enum!(notify_on_command_finish),
        Key::NotifyOnCommandFinishAction => g_packed!(notify_on_command_finish_action),
        Key::NotifyOnCommandFinishAfter => g_cval!(notify_on_command_finish_after),
        Key::OscColorReportFormat => g_enum!(osc_color_report_format),
        Key::Palette => g_cval!(palette),
        Key::PaletteGenerate => g_bool!(palette_generate),
        Key::PaletteHarmonious => g_bool!(palette_harmonious),
        Key::ProgressStyle => g_bool!(progress_style),
        Key::QuickTerminalAnimationDuration => g_float!(quick_terminal_animation_duration, f64),
        Key::QuickTerminalAutohide => g_bool!(quick_terminal_autohide),
        Key::QuickTerminalKeyboardInteractivity => g_enum!(quick_terminal_keyboard_interactivity),
        Key::QuickTerminalPosition => g_enum!(quick_terminal_position),
        Key::QuickTerminalScreen => g_enum!(quick_terminal_screen),
        Key::QuickTerminalSize => g_cval!(quick_terminal_size),
        Key::QuickTerminalSpaceBehavior => g_enum!(quick_terminal_space_behavior),
        Key::QuitAfterLastWindowClosed => g_bool!(quit_after_last_window_closed),
        Key::QuitAfterLastWindowClosedDelay => g_opt_cval!(quit_after_last_window_closed_delay),
        Key::ResizeOverlay => g_enum!(resize_overlay),
        Key::ResizeOverlayDuration => g_cval!(resize_overlay_duration),
        Key::ResizeOverlayPosition => g_enum!(resize_overlay_position),
        Key::RightClickAction => g_enum!(right_click_action),
        Key::ScrollToBottom => g_packed!(scroll_to_bottom),
        Key::ScrollbackLimitBytes => false,
        Key::ScrollbackLimitLines => false,
        Key::ScrollbackCompression => g_bool!(scrollback_compression),
        Key::Scrollbar => g_enum!(scrollbar),
        Key::SearchBackground => false,
        Key::SearchForeground => false,
        Key::SearchSelectedBackground => false,
        Key::SearchSelectedForeground => false,
        Key::SelectionBackground => false,
        Key::SelectionClearOnCopy => g_bool!(selection_clear_on_copy),
        Key::SelectionClearOnTyping => g_bool!(selection_clear_on_typing),
        Key::SelectionForeground => false,
        Key::SelectionWordChars => false,
        Key::ShellIntegration => g_enum!(shell_integration),
        Key::ShellIntegrationFeatures => g_packed!(shell_integration_features),
        Key::SplitDividerColor => g_opt_cval!(split_divider_color),
        Key::SplitInheritWorkingDirectory => g_bool!(split_inherit_working_directory),
        Key::SplitPreserveZoom => g_packed!(split_preserve_zoom),
        Key::TabInheritWorkingDirectory => g_bool!(tab_inherit_working_directory),
        Key::Term => false,
        Key::Theme => false,
        Key::Title => g_optstr!(title),
        Key::TitleReport => g_bool!(title_report),
        Key::UndoTimeout => g_cval!(undo_timeout),
        Key::UnfocusedSplitFill => g_opt_cval!(unfocused_split_fill),
        Key::UnfocusedSplitOpacity => g_float!(unfocused_split_opacity, f64),
        Key::VtKamAllowed => g_bool!(vt_kam_allowed),
        Key::VtWindowResizeAllowed => g_bool!(vt_window_resize_allowed),
        Key::WaitAfterCommand => g_bool!(wait_after_command),
        Key::WindowColorspace => g_enum_fn!(window_colorspace, window_colorspace_to_name_z),
        Key::WindowDecoration => g_enum!(window_decoration),
        Key::WindowHeight => g_uint!(window_height),
        Key::WindowInheritFontSize => g_bool!(window_inherit_font_size),
        Key::WindowInheritWorkingDirectory => g_bool!(window_inherit_working_directory),
        Key::WindowNewTabPosition => g_enum!(window_new_tab_position),
        Key::WindowPaddingBalance => {
            g_enum_fn!(window_padding_balance, window_padding_balance_to_name_z)
        }
        Key::WindowPaddingColor => g_enum_fn!(window_padding_color, window_padding_color_to_name_z),
        Key::WindowPaddingX => false,
        Key::WindowPaddingY => false,
        Key::WindowPositionX => g_opt_i16!(window_position_x),
        Key::WindowPositionY => g_opt_i16!(window_position_y),
        Key::WindowSaveState => g_enum!(window_save_state),
        Key::WindowShowTabBar => g_enum!(window_show_tab_bar),
        Key::WindowStepResize => g_bool!(window_step_resize),
        Key::WindowSubtitle => g_enum!(window_subtitle),
        Key::WindowTheme => g_enum!(window_theme),
        Key::WindowTitleFontFamily => g_optstr!(window_title_font_family),
        Key::WindowTitlebarBackground => g_opt_cval!(window_titlebar_background),
        Key::WindowTitlebarForeground => g_opt_cval!(window_titlebar_foreground),
        Key::WindowVsync => g_bool!(window_vsync),
        Key::WindowWidth => g_uint!(window_width),
        Key::WorkingDirectory => false,
        Key::X11InstanceName => g_optstr!(x11_instance_name),
    }
}

#[cfg(test)]
mod tests {
    use super::get;
    use crate::config_struct::{Config, Key};
    use crate::path::{Path, PathC};
    use crate::value_types::{BackgroundBlur, Color, ColorC, WindowTheme};
    use std::ffi::CStr;
    use std::os::raw::{c_char, c_uint};

    // test "c_get: u8"
    #[test]
    fn c_get_u8() {
        // var c = try Config.default(alloc);
        let mut c = Config::default_config();
        // c.@"font-size" = 24;
        c.font_size = 24.0;

        // var cval: f32 = undefined;
        let mut cval: f32 = 0.0;
        // try testing.expect(get(&c, .@"font-size", &cval));
        assert!(unsafe { get(&c, Key::FontSize, std::ptr::from_mut(&mut cval).cast()) });
        // try testing.expectEqual(@as(f32, 24), cval);
        assert_eq!(cval, 24.0_f32);
    }

    // test "c_get: enum"
    #[test]
    fn c_get_enum() {
        let mut c = Config::default_config();
        // c.@"window-theme" = .dark;
        c.window_theme = WindowTheme::Dark;

        // var cval: [*:0]u8 = undefined;
        let mut cval: *const c_char = std::ptr::null();
        // try testing.expect(get(&c, .@"window-theme", @ptrCast(&cval)));
        assert!(unsafe { get(&c, Key::WindowTheme, std::ptr::from_mut(&mut cval).cast()) });

        // const str = std.mem.sliceTo(cval, 0);
        // try testing.expectEqualStrings("dark", str);
        let str_ = unsafe { CStr::from_ptr(cval) };
        assert_eq!(str_.to_bytes(), b"dark");
    }

    // test "c_get: color"
    #[test]
    fn c_get_color() {
        let mut c = Config::default_config();
        // c.background = .{ .r = 255, .g = 0, .b = 0 };
        c.background = Color { r: 255, g: 0, b: 0 };

        // var cval: Color.C = undefined;
        let mut cval = ColorC { r: 0, g: 0, b: 0 };
        // try testing.expect(get(&c, .background, @ptrCast(&cval)));
        assert!(unsafe { get(&c, Key::Background, std::ptr::from_mut(&mut cval).cast()) });
        // try testing.expectEqual(255, cval.r);
        assert_eq!(cval.r, 255);
        assert_eq!(cval.g, 0);
        assert_eq!(cval.b, 0);
    }

    // test "c_get: optional"
    #[test]
    fn c_get_optional() {
        let mut c = Config::default_config();

        {
            // c.@"unfocused-split-fill" = null;
            c.unfocused_split_fill = None;
            // var cval: Color.C = undefined;
            let mut cval = ColorC { r: 0, g: 0, b: 0 };
            // try testing.expect(!get(&c, .@"unfocused-split-fill", @ptrCast(&cval)));
            assert!(!unsafe {
                get(
                    &c,
                    Key::UnfocusedSplitFill,
                    std::ptr::from_mut(&mut cval).cast(),
                )
            });
        }

        {
            // c.@"unfocused-split-fill" = .{ .r = 255, .g = 0, .b = 0 };
            c.unfocused_split_fill = Some(Color { r: 255, g: 0, b: 0 });
            let mut cval = ColorC { r: 0, g: 0, b: 0 };
            assert!(unsafe {
                get(
                    &c,
                    Key::UnfocusedSplitFill,
                    std::ptr::from_mut(&mut cval).cast(),
                )
            });
            assert_eq!(cval.r, 255);
            assert_eq!(cval.g, 0);
            assert_eq!(cval.b, 0);
        }
    }

    // test "c_get: background-blur"
    #[test]
    fn c_get_background_blur() {
        let mut c = Config::default_config();

        {
            // c.@"background-blur" = .false;
            c.background_blur = BackgroundBlur::False;
            let mut cval: i16 = 0;
            assert!(unsafe {
                get(
                    &c,
                    Key::BackgroundBlur,
                    std::ptr::from_mut(&mut cval).cast(),
                )
            });
            assert_eq!(cval, 0);
        }
        {
            // c.@"background-blur" = .true;
            c.background_blur = BackgroundBlur::True;
            let mut cval: i16 = 0;
            assert!(unsafe {
                get(
                    &c,
                    Key::BackgroundBlur,
                    std::ptr::from_mut(&mut cval).cast(),
                )
            });
            assert_eq!(cval, 20);
        }
        {
            // c.@"background-blur" = .{ .radius = 42 };
            c.background_blur = BackgroundBlur::Radius(42);
            let mut cval: i16 = 0;
            assert!(unsafe {
                get(
                    &c,
                    Key::BackgroundBlur,
                    std::ptr::from_mut(&mut cval).cast(),
                )
            });
            assert_eq!(cval, 42);
        }
        {
            // c.@"background-blur" = .@"macos-glass-regular";
            c.background_blur = BackgroundBlur::MacosGlassRegular;
            let mut cval: i16 = 0;
            assert!(unsafe {
                get(
                    &c,
                    Key::BackgroundBlur,
                    std::ptr::from_mut(&mut cval).cast(),
                )
            });
            assert_eq!(cval, -1);
        }
        {
            // c.@"background-blur" = .@"macos-glass-clear";
            c.background_blur = BackgroundBlur::MacosGlassClear;
            let mut cval: i16 = 0;
            assert!(unsafe {
                get(
                    &c,
                    Key::BackgroundBlur,
                    std::ptr::from_mut(&mut cval).cast(),
                )
            });
            assert_eq!(cval, -2);
        }
    }

    // test "c_get: split-preserve-zoom"
    #[test]
    fn c_get_split_preserve_zoom() {
        let mut c = Config::default_config();

        // var bits: c_uint = undefined;
        let mut bits: c_uint = 0;
        // try testing.expect(get(&c, .@"split-preserve-zoom", @ptrCast(&bits)));
        assert!(unsafe {
            get(
                &c,
                Key::SplitPreserveZoom,
                std::ptr::from_mut(&mut bits).cast(),
            )
        });
        // try testing.expectEqual(@as(c_uint, 0), bits);
        assert_eq!(bits, 0 as c_uint);

        // c.@"split-preserve-zoom".navigation = true;
        c.split_preserve_zoom.navigation = true;
        assert!(unsafe {
            get(
                &c,
                Key::SplitPreserveZoom,
                std::ptr::from_mut(&mut bits).cast(),
            )
        });
        // try testing.expectEqual(@as(c_uint, 1), bits);
        assert_eq!(bits, 1 as c_uint);
    }

    // PARITY (rustty-added): the `?Path` keys (`background-image`,
    // `bell-audio-path`) resolve through the reference's generic `.optional` ->
    // `.@"union"` -> `cval()` arms of `getValue`, so a POPULATED key must return
    // true and write a `Path.C` (`rustty_config_path_s`); only a null optional
    // returns false. The reference has no dedicated test for this (its coverage is
    // the comptime-generic `getValue`), so this is the rustty proving test for the
    // projection.
    #[test]
    fn c_get_optional_path() {
        // The out-parameter layout must match `rustty_config_path_s`
        // (`const char* path; bool optional;`).
        assert_eq!(
            std::mem::size_of::<PathC>(),
            std::mem::size_of::<*const c_char>() * 2
        );
        assert_eq!(
            std::mem::align_of::<PathC>(),
            std::mem::align_of::<*const c_char>()
        );

        let null_cval = || PathC {
            path: std::ptr::null(),
            optional: false,
        };

        #[allow(clippy::type_complexity)]
        let fields: [(Key, fn(&mut Config, Option<Path>)); 2] = [
            (Key::BackgroundImage, |c, v| c.background_image = v),
            (Key::BellAudioPath, |c, v| c.bell_audio_path = v),
        ];
        for (key, set) in fields {
            let mut c = Config::default_config();

            // Unset optional -> false (`const unwrapped = value orelse return false`).
            set(&mut c, None);
            let mut cval = null_cval();
            assert!(!unsafe { get(&c, key, std::ptr::from_mut(&mut cval).cast()) });

            // `.required` -> `.{ .path = path.ptr, .optional = false }`.
            set(&mut c, Some(Path::Required(b"/tmp/bg.png".to_vec())));
            let mut cval = null_cval();
            assert!(unsafe { get(&c, key, std::ptr::from_mut(&mut cval).cast()) });
            assert!(!cval.optional);
            assert!(!cval.path.is_null());
            // The `[:0]` sentinel is not stored (crate-wide `Vec<u8>` convention),
            // so read exactly the value bytes rather than through `CStr`.
            let bytes =
                unsafe { std::slice::from_raw_parts(cval.path.cast::<u8>(), "/tmp/bg.png".len()) };
            assert_eq!(bytes, b"/tmp/bg.png");

            // `.optional` -> `.{ .path = path.ptr, .optional = true }`.
            set(&mut c, Some(Path::Optional(b"/tmp/bg.png".to_vec())));
            let mut cval = null_cval();
            assert!(unsafe { get(&c, key, std::ptr::from_mut(&mut cval).cast()) });
            assert!(cval.optional);
            let bytes =
                unsafe { std::slice::from_raw_parts(cval.path.cast::<u8>(), "/tmp/bg.png".len()) };
            assert_eq!(bytes, b"/tmp/bg.png");
        }
    }
}
