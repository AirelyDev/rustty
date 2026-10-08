//! PORTS: ghostty src/config/Config.zig — `finalize`, `cloneEmpty`, `clone`,
//! `changed` (the per-`Key` CHANGED dispatch) and `changeIterator`.

use super::*;

impl Config {
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
        //
        // PORT NOTE (LEDGER S74-F7): this was `#[cfg(not(test))]`, which kept
        // the block on wasm, where an embedder-installed environ could set
        // `command` from `SHELL`. The guard now carries both terms. It is a
        // `const fn` on `cfg!` values rather than a `#[cfg]`, as the Zig is a
        // comptime `if`; the body still compiles everywhere.
        if finalize_looks_up_defaults(cfg!(target_family = "wasm"), cfg!(test)) {
            // if (self.command == null or wd == .home) command: {
            if self.command.is_none() || wd == WorkingDirectory::Home {
                'command: {
                    // First look up the command using the SHELL env var if needed.
                    // We don't do this in flatpak because SHELL in Flatpak is always
                    // set to /bin/sh.
                    // if (self.command) |cmd|
                    //     log.info("shell src=config value={}", .{cmd})
                    // else shell_env: {
                    if let Some(cmd) = &self.command {
                        // PORT NOTE (N11): Zig's `{}` on a value with no
                        // `format` method renders its builtin aggregate
                        // formatter (`config.Command{ .shell = ... }`);
                        // `{cmd:?}` is the port's crate-wide stand-in for that.
                        // Same family as the renderer's `image::Id` /
                        // `renderer::Size` sites; the crate-wide sweep is N11's
                        // own task, not this row's.
                        log_info(format!("shell src=config value={cmd:?}"));
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
                            //     wd = .{ .path = try alloc.dupe(u8, home) };
                            // } else {
                            //     wd = .inherit;
                            // }
                            //
                            // PORT NOTE (LEDGER S74-F6): this was `if let
                            // Ok(Some(home))`, which turned a `home()` error into
                            // `.inherit`; the `try` returns it.
                            wd = windows_home_working_directory(rustty_os::homedir::home(
                                &rustty_global::environ_map(),
                                &mut buf,
                            ))?;
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
                // `{f}` calls `Duration.format` (`Config.zig`'s `Duration`,
                // the `units` walk), NOT the builtin aggregate formatter —
                // `4s`, `1m 30s`. `Duration::format` is the direct port of it
                // and is already used by `format_entry`.
                let mut rendered: Vec<u8> = Vec::new();
                duration.format(&mut rendered);
                log_warn(format!(
                    "quit-after-last-window-closed-delay is set to a very short value ({}), which might cause problems",
                    String::from_utf8_lossy(&rendered)
                ));
            }
        }

        // We can't set this as a struct default because our config is
        // loaded in environments where a build config isn't available.
        // if (self.@"auto-update-channel" == null) {
        if self.auto_update_channel.is_none() {
            // self.@"auto-update-channel" = build_config.release_channel;
            self.auto_update_channel = Some(rustty_version::release_channel());
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
            Key::VtWindowResizeAllowed => c!(vt_window_resize_allowed),
            Key::VtXtChecksumReport => c!(vt_xt_checksum_report),
            Key::VtXtChecksumExtension => c!(vt_xt_checksum_extension),
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
        // RepeatableCommand (no derived `Clone`; see the S1-F1/S103-F1 PORT
        // NOTE on the struct — a derived field-by-field clone would leave
        // `value_c`'s pointers aliasing the SOURCE's `storage`, not the
        // clone's, so this must go through `clone_in()`, which rebuilds
        // `value_c`/`storage` together from the cloned `value`).
        ci!(command_palette_entry);
        cv!(osc_color_report_format);
        cv!(vt_kam_allowed);
        cv!(vt_window_resize_allowed);
        cv!(vt_xt_checksum_report);
        cv!(vt_xt_checksum_extension);
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
