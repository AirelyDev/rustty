//! PORTS: ghostty src/input/Binding.zig — the rest of `Action`: the command-palette tagged-union
//! parse (`parseTaggedUnion`), `parse`/`parseParameter`, `Scope`/`scoped`,
//! `format`/`formatValue`, `clone`/`hash`/`equal`. The `Action` union itself,
//! `tag_name` and `FIELD_NAMES` stay in `binding.rs`, which `build.rs` parses.

use super::*;

// PORT NOTE: `cli.args.parseTaggedUnion(Action, alloc, v)` — used by
// `config.RepeatableCommand.parseCLI` (via `parseAutoStruct(inputpkg.Command, ...)`)
// to parse the `action:<tag>:<value>` field of a command-palette entry. This is a
// DISTINCT code path from `Action.parse` (the keybind-trigger parser): here the
// reference synthesizes, per matched union variant, a one-field struct and runs
// `parseIntoField` on it, so the per-variant payload dispatch is the generic
// `parseIntoField` field-type "magic" (string/int/float/enum/struct), NOT the
// keybind `parseParameter` logic. The transcribed reference body:
//
//     const info = @typeInfo(T).@"union";
//     assert(@typeInfo(info.tag_type.?) == .@"enum");
//
//     // Get the union tag that is being set. We support values with no colon
//     // if the value is void so its not an error to have no colon.
//     const colon_idx = mem.indexOf(u8, v, ":") orelse v.len;
//     const tag_str = std.mem.trim(u8, v[0..colon_idx], whitespace);
//     const value = if (colon_idx < v.len) v[colon_idx + 1 ..] else "";
//
//     // Find the field in the union that matches the tag.
//     inline for (info.fields) |field| {
//         if (mem.eql(u8, field.name, tag_str)) {
//             // Special case void types where we don't need a value.
//             if (field.type == void) {
//                 if (value.len > 0) return error.InvalidValue;
//                 return @unionInit(T, field.name, {});
//             }
//             // ... synthesize a one-field struct and parseIntoField it ...
//             try parseIntoField(Target, alloc, &t, field.name, value);
//             return @unionInit(T, field.name, @field(t, field.name));
//         }
//     }
//     return error.InvalidValue;
//
// `split_tagged_union` provides the exact colon/whitespace split. Each arm below
// performs the `parseIntoField` field-type dispatch for that variant's payload
// (the `[]const u8`/`[:0]const u8` arms copy the value; ints/floats parse; enums
// `stringToEnum`; structs `parseStruct` -> `parseAutoStruct`). Void variants
// require an empty value. An unknown tag is `error.InvalidValue`.
impl Action {
    pub(crate) fn parse_auto_tagged_union(
        v: &[u8],
    ) -> Result<Action, rustty_args::args::ParseError> {
        use rustty_args::args::{
            parse_auto_struct, parse_field_enum, parse_field_float, parse_field_int,
            parse_field_string, split_tagged_union, Error,
        };

        // const colon_idx = ...; const tag_str = ...; const value = ...;
        let (tag, value) = split_tagged_union(v);

        // PORT NOTE: helper for the `field.type == void` arm — `if (value.len > 0)
        // return error.InvalidValue;` then `return @unionInit(T, field.name, {})`.
        macro_rules! void_arm {
            ($variant:expr) => {{
                if !value.is_empty() {
                    return Err(Error::InvalidValue);
                }
                return Ok($variant);
            }};
        }

        match tag {
            // void variants
            b"ignore" => void_arm!(Action::Ignore),
            b"unbind" => void_arm!(Action::Unbind),
            b"reset" => void_arm!(Action::Reset),
            b"paste_from_clipboard" => void_arm!(Action::PasteFromClipboard),
            b"paste_from_selection" => void_arm!(Action::PasteFromSelection),
            b"copy_url_to_clipboard" => void_arm!(Action::CopyUrlToClipboard),
            b"copy_title_to_clipboard" => void_arm!(Action::CopyTitleToClipboard),
            b"reset_font_size" => void_arm!(Action::ResetFontSize),
            b"search_selection" => void_arm!(Action::SearchSelection),
            b"start_search" => void_arm!(Action::StartSearch),
            b"end_search" => void_arm!(Action::EndSearch),
            b"clear_screen" => void_arm!(Action::ClearScreen),
            b"select_all" => void_arm!(Action::SelectAll),
            b"scroll_to_top" => void_arm!(Action::ScrollToTop),
            b"scroll_to_bottom" => void_arm!(Action::ScrollToBottom),
            b"scroll_to_selection" => void_arm!(Action::ScrollToSelection),
            b"scroll_page_up" => void_arm!(Action::ScrollPageUp),
            b"scroll_page_down" => void_arm!(Action::ScrollPageDown),
            b"new_window" => void_arm!(Action::NewWindow),
            b"new_tab" => void_arm!(Action::NewTab),
            b"previous_tab" => void_arm!(Action::PreviousTab),
            b"next_tab" => void_arm!(Action::NextTab),
            b"last_tab" => void_arm!(Action::LastTab),
            b"move_tab_to_new_window" => void_arm!(Action::MoveTabToNewWindow),
            b"toggle_tab_overview" => void_arm!(Action::ToggleTabOverview),
            b"prompt_surface_title" => void_arm!(Action::PromptSurfaceTitle),
            b"prompt_tab_title" => void_arm!(Action::PromptTabTitle),
            b"prompt_window_title" => void_arm!(Action::PromptWindowTitle),
            b"toggle_split_zoom" => void_arm!(Action::ToggleSplitZoom),
            b"toggle_readonly" => void_arm!(Action::ToggleReadonly),
            b"equalize_splits" => void_arm!(Action::EqualizeSplits),
            b"reset_window_size" => void_arm!(Action::ResetWindowSize),
            b"show_gtk_inspector" => void_arm!(Action::ShowGtkInspector),
            b"show_on_screen_keyboard" => void_arm!(Action::ShowOnScreenKeyboard),
            b"reload_config" => void_arm!(Action::ReloadConfig),
            b"close_surface" => void_arm!(Action::CloseSurface),
            b"close_window" => void_arm!(Action::CloseWindow),
            b"close_all_windows" => void_arm!(Action::CloseAllWindows),
            b"toggle_maximize" => void_arm!(Action::ToggleMaximize),
            b"toggle_fullscreen" => void_arm!(Action::ToggleFullscreen),
            b"toggle_window_decorations" => void_arm!(Action::ToggleWindowDecorations),
            b"toggle_window_float_on_top" => void_arm!(Action::ToggleWindowFloatOnTop),
            b"toggle_secure_input" => void_arm!(Action::ToggleSecureInput),
            b"toggle_mouse_reporting" => void_arm!(Action::ToggleMouseReporting),
            b"toggle_command_palette" => void_arm!(Action::ToggleCommandPalette),
            b"toggle_quick_terminal" => void_arm!(Action::ToggleQuickTerminal),
            b"toggle_visibility" => void_arm!(Action::ToggleVisibility),
            b"toggle_background_opacity" => void_arm!(Action::ToggleBackgroundOpacity),
            b"check_for_updates" => void_arm!(Action::CheckForUpdates),
            b"undo" => void_arm!(Action::Undo),
            b"redo" => void_arm!(Action::Redo),
            b"end_key_sequence" => void_arm!(Action::EndKeySequence),
            b"deactivate_key_table" => void_arm!(Action::DeactivateKeyTable),
            b"deactivate_all_key_tables" => void_arm!(Action::DeactivateAllKeyTables),
            b"quit" => void_arm!(Action::Quit),

            // []const u8 variants (parseIntoField `[]const u8` arm)
            b"csi" => Ok(Action::Csi(parse_field_string(Some(value))?)),
            b"esc" => Ok(Action::Esc(parse_field_string(Some(value))?)),
            b"text" => Ok(Action::Text(parse_field_string(Some(value))?)),
            b"search" => Ok(Action::Search(parse_field_string(Some(value))?)),
            b"set_surface_title" => Ok(Action::SetSurfaceTitle(parse_field_string(Some(value))?)),
            b"set_tab_title" => Ok(Action::SetTabTitle(parse_field_string(Some(value))?)),
            b"set_window_title" => Ok(Action::SetWindowTitle(parse_field_string(Some(value))?)),
            b"activate_key_table" => Ok(Action::ActivateKeyTable(parse_field_string(Some(value))?)),
            b"activate_key_table_once" => Ok(Action::ActivateKeyTableOnce(parse_field_string(
                Some(value),
            )?)),

            // f32 variants (parseIntoField float arm)
            b"increase_font_size" => Ok(Action::IncreaseFontSize(parse_field_float::<f32>(Some(
                value,
            ))?)),
            b"decrease_font_size" => Ok(Action::DecreaseFontSize(parse_field_float::<f32>(Some(
                value,
            ))?)),
            b"set_font_size" => Ok(Action::SetFontSize(parse_field_float::<f32>(Some(value))?)),
            b"scroll_page_fractional" => Ok(Action::ScrollPageFractional(
                parse_field_float::<f32>(Some(value))?,
            )),

            // integer variants (parseIntoField int arm)
            b"scroll_to_row" => Ok(Action::ScrollToRow(parse_field_int::<usize>(Some(value))?)),
            b"scroll_page_lines" => Ok(Action::ScrollPageLines(parse_field_int::<i16>(Some(
                value,
            ))?)),
            b"jump_to_prompt" => Ok(Action::JumpToPrompt(parse_field_int::<i16>(Some(value))?)),
            // R13 (UD58): bounded by `GOTO_TAB_MAX`, with this parser's
            // out-of-range error.
            b"goto_tab" => {
                let v = parse_field_int::<usize>(Some(value))?;
                if v > GOTO_TAB_MAX {
                    return Err(Error::InvalidValue);
                }
                Ok(Action::GotoTab(v))
            }
            b"move_tab" => Ok(Action::MoveTab(parse_field_int::<isize>(Some(value))?)),

            // enum variants (parseIntoField `.@"enum"` arm -> stringToEnum)
            b"copy_to_clipboard" => Ok(Action::CopyToClipboard(parse_field_enum(
                Some(value),
                CopyToClipboard::string_to_enum,
            )?)),
            b"navigate_search" => Ok(Action::NavigateSearch(parse_field_enum(
                Some(value),
                NavigateSearch::string_to_enum,
            )?)),
            b"adjust_selection" => Ok(Action::AdjustSelection(parse_field_enum(
                Some(value),
                AdjustSelection::string_to_enum,
            )?)),
            b"new_split" => Ok(Action::NewSplit(parse_field_enum(
                Some(value),
                SplitDirection::string_to_enum,
            )?)),
            b"goto_split" => Ok(Action::GotoSplit(parse_field_enum(
                Some(value),
                SplitFocusDirection::string_to_enum,
            )?)),
            b"goto_window" => Ok(Action::GotoWindow(parse_field_enum(
                Some(value),
                GotoWindow::string_to_enum,
            )?)),
            b"inspector" => Ok(Action::Inspector(parse_field_enum(
                Some(value),
                InspectorMode::string_to_enum,
            )?)),
            b"open_config" => Ok(Action::OpenConfig(parse_field_enum(
                Some(value),
                OpenConfig::string_to_enum,
            )?)),
            b"close_tab" => Ok(Action::CloseTab(parse_field_enum(
                Some(value),
                CloseTabMode::string_to_enum,
            )?)),
            b"crash" => Ok(Action::Crash(parse_field_enum(
                Some(value),
                CrashThread::string_to_enum,
            )?)),

            // struct variants (parseIntoField `.@"struct"` arm -> parseStruct ->
            // parseAutoStruct). NOTE: this is the `parseAutoStruct` field-by-field
            // path, NOT each type's custom `parse`/`parseParameter`; e.g.
            // `resize_split:up,10` is parsed as `parseAutoStruct(SplitResizeParameter,
            // "up,10")` (which expects `field:value` entries), exactly as the
            // reference does in the command-palette path.
            b"cursor_key" => {
                let mut t = CursorKey {
                    normal: Vec::new(),
                    application: Vec::new(),
                };
                parse_auto_struct(&mut t, value)?;
                Ok(Action::CursorKey(t))
            }
            b"write_scrollback_file" => {
                let mut t = WriteScreen {
                    action: WriteScreenAction::Copy,
                    emit: WriteScreenFormat::Plain,
                };
                parse_auto_struct(&mut t, value)?;
                Ok(Action::WriteScrollbackFile(t))
            }
            b"write_screen_file" => {
                let mut t = WriteScreen {
                    action: WriteScreenAction::Copy,
                    emit: WriteScreenFormat::Plain,
                };
                parse_auto_struct(&mut t, value)?;
                Ok(Action::WriteScreenFile(t))
            }
            b"write_selection_file" => {
                let mut t = WriteScreen {
                    action: WriteScreenAction::Copy,
                    emit: WriteScreenFormat::Plain,
                };
                parse_auto_struct(&mut t, value)?;
                Ok(Action::WriteSelectionFile(t))
            }
            b"resize_split" => {
                let mut t = SplitResizeParameter(SplitResizeDirection::Up, 0);
                parse_auto_struct(&mut t, value)?;
                Ok(Action::ResizeSplit(t))
            }

            // return error.InvalidValue (unknown tag)
            _ => Err(Error::InvalidValue),
        }
    }
}

// PORT NOTE: `cursor_key`'s payload `CursorKey = struct { normal: []const u8,
// application: []const u8 }`. In the `parseAutoStruct` path each field is a
// `[]const u8` (required, no default), parsed by `parse_field_string`.
impl rustty_args::args::AutoStruct for CursorKey {
    fn field_names() -> &'static [&'static [u8]] {
        &[b"normal", b"application"]
    }
    fn set_field(
        &mut self,
        key: &[u8],
        value: Option<&[u8]>,
    ) -> Result<(), rustty_args::args::ParseError> {
        use rustty_args::args::{parse_field_string, Error};
        match key {
            b"normal" => {
                self.normal = parse_field_string(value)?;
                Ok(())
            }
            b"application" => {
                self.application = parse_field_string(value)?;
                Ok(())
            }
            _ => Err(Error::InvalidField),
        }
    }
    fn apply_default(&mut self, _i: usize) -> Result<(), rustty_args::args::ParseError> {
        // Both fields are required (no default) -> error.InvalidValue.
        Err(rustty_args::args::Error::InvalidValue)
    }
}

// PORT NOTE: `write_*_file`'s payload `WriteScreen = struct { action:
// WriteScreen.Action, emit: WriteScreen.Format }`. Both fields are enums and are
// required (the `copy`/`paste`/`open` decls are named CONSTANTS, not field
// defaults), so the `parseAutoStruct` path needs both `action:` and `emit:`
// entries; a missing one is `error.InvalidValue`.
impl rustty_args::args::AutoStruct for WriteScreen {
    fn field_names() -> &'static [&'static [u8]] {
        &[b"action", b"emit"]
    }
    fn set_field(
        &mut self,
        key: &[u8],
        value: Option<&[u8]>,
    ) -> Result<(), rustty_args::args::ParseError> {
        use rustty_args::args::{parse_field_enum, Error};
        match key {
            b"action" => {
                self.action = parse_field_enum(value, WriteScreenAction::string_to_enum)?;
                Ok(())
            }
            b"emit" => {
                self.emit = parse_field_enum(value, WriteScreenFormat::string_to_enum)?;
                Ok(())
            }
            _ => Err(Error::InvalidField),
        }
    }
    fn apply_default(&mut self, _i: usize) -> Result<(), rustty_args::args::ParseError> {
        Err(rustty_args::args::Error::InvalidValue)
    }
}

// PORT NOTE: `resize_split`'s payload `SplitResizeParameter = struct {
// SplitResizeDirection, u16 }` — a TUPLE struct, so its `parseAutoStruct` field
// names are the tuple positions `"0"`/`"1"` (Zig's reflected field names for a
// tuple). Field 0 is the enum, field 1 the `u16`; both required. (This differs
// from the keybind `parse_resize_split`, which splits on a comma and uses a custom
// per-position parse — the command-palette path uses `parseAutoStruct` instead.)
impl rustty_args::args::AutoStruct for SplitResizeParameter {
    fn field_names() -> &'static [&'static [u8]] {
        &[b"0", b"1"]
    }
    fn set_field(
        &mut self,
        key: &[u8],
        value: Option<&[u8]>,
    ) -> Result<(), rustty_args::args::ParseError> {
        use rustty_args::args::{parse_field_enum, parse_field_int, Error};
        match key {
            b"0" => {
                self.0 = parse_field_enum(value, SplitResizeDirection::string_to_enum)?;
                Ok(())
            }
            b"1" => {
                self.1 = parse_field_int::<u16>(value)?;
                Ok(())
            }
            _ => Err(Error::InvalidField),
        }
    }
    fn apply_default(&mut self, _i: usize) -> Result<(), rustty_args::args::ParseError> {
        Err(rustty_args::args::Error::InvalidValue)
    }
}

// PORT NOTE: the reference's `parseEnum`/`parseInt`/`parseFloat`/`parseParameter`
// are comptime-generic helpers reflecting over `field.type`. Rust has no comptime
// type reflection, so the per-arm dispatch of `parse` (below) calls the concrete
// helper for each variant's payload type, reproducing exactly the reference arm
// `@typeInfo(field.type)` would select. `parseParameter`'s reference body and its
// comments are preserved across the concrete helpers:
//
//   // Fields can provide a custom "parse" function
//   ... (see `parse_resize_split` / `WriteScreen::parse` / `SplitFocusDirection::parse`)
//
//   // Only tuples are supported to avoid ambiguity with field
//   // ordering
//   ... (see `parse_resize_split`)
//
//   // If we have extra parameters it is an error
//   ... (see `parse_resize_split`)

// fn parseInt(comptime T, value) !T { return std.fmt.parseInt(T, value, 10) catch return Error.InvalidFormat; }
//
// S100-F1: this was Rust's `str::parse`, which rejects inputs Zig's
// `std.fmt.parseInt` accepts: `_` digit separators (`goto_tab:1_0`) and `-0`
// for an unsigned type (`goto_tab:-0`). `rustty_zigstd::fmt::parse_int` is
// the port of the Zig parser. The per-type names are the instantiations the
// action arms use.
fn parse_int<T: rustty_zigstd::fmt::ZigInt>(value: &[u8]) -> Result<T, Error> {
    rustty_zigstd::fmt::parse_int::<T>(value, 10).map_err(|_| Error::InvalidFormat)
}
fn parse_int_i16(value: &[u8]) -> Result<i16, Error> {
    parse_int::<i16>(value)
}
fn parse_int_isize(value: &[u8]) -> Result<isize, Error> {
    parse_int::<isize>(value)
}
fn parse_int_usize(value: &[u8]) -> Result<usize, Error> {
    parse_int::<usize>(value)
}

/// The largest `goto_tab` index the action boundary can carry.
///
/// PORT NOTE: reference defect (R13, UPSTREAM-DEFECTS.md UD58). The field is
/// `goto_tab: usize` (`Binding.zig:575`), but `Surface.zig:5328` narrows it with
/// `@enumFromInt` into `apprt.action.GotoTab = enum(c_int)`, whose -1/-2/-3 are the
/// previous/next/last sentinels. Above `c_int` max, ReleaseSafe panics and
/// ReleaseFast truncates, so `goto_tab:4294967295` goes to the previous tab. Both
/// action parsers reject a larger value with the error they already return for an
/// integer argument that does not fit its type (`InvalidFormat` from
/// `Action.parse`, `InvalidValue` from the tagged-union parser).
pub const GOTO_TAB_MAX: usize = std::ffi::c_int::MAX as usize;

/// `parseInt(usize, …)` for `goto_tab`, bounded by [`GOTO_TAB_MAX`] (R13, UD58).
fn parse_int_goto_tab(value: &[u8]) -> Result<usize, Error> {
    let v = parse_int_usize(value)?;
    if v > GOTO_TAB_MAX {
        return Err(Error::InvalidFormat);
    }
    Ok(v)
}
fn parse_int_u16(value: &[u8]) -> Result<u16, Error> {
    parse_int::<u16>(value)
}

// fn parseFloat(comptime T, value) !T { return std.fmt.parseFloat(T, value) catch return Error.InvalidFormat; }
//
// S100-F1: Zig's `std.fmt.parseFloat` also takes `_` separators and hex floats
// (`set_font_size:0x1.8p3` is 12), which `str::parse` rejects.
fn parse_float_f32(value: &[u8]) -> Result<f32, Error> {
    rustty_zigstd::fmt::parse_float::<f32>(value).map_err(|_| Error::InvalidFormat)
}

// PORT NOTE: `parseParameter` for the `resize_split` tuple
// `SplitResizeParameter{ SplitResizeDirection, u16 }`. The reference splits on
// `,`, parses the enum then the int, and errors on extra parameters. This is the
// `.@"struct" => |info| { comptime assert(info.is_tuple); ... }` arm specialized
// to that single tuple type.
//
// Fields can provide a custom "parse" function
//
// Only tuples are supported to avoid ambiguity with field
// ordering
fn parse_resize_split(param: &[u8]) -> Result<SplitResizeParameter, Error> {
    // var it = std.mem.splitAny(u8, param, ",");
    let mut it = param.split(|&c| c == b',');
    // field 0: SplitResizeDirection (enum)
    let next0 = it.next().ok_or(Error::InvalidFormat)?;
    let f0 = SplitResizeDirection::string_to_enum(next0).ok_or(Error::InvalidFormat)?;
    // field 1: u16 (int)
    let next1 = it.next().ok_or(Error::InvalidFormat)?;
    let f1 = parse_int_u16(next1)?;
    // If we have extra parameters it is an error
    if it.next().is_some() {
        return Err(Error::InvalidFormat);
    }
    Ok(SplitResizeParameter(f0, f1))
}

impl Action {
    /// Parse an action in the format of "key=value" where key is the
    /// action name and value is the action parameter. The parameter
    /// is optional depending on the action.
    // PORT NOTE: the per-arm `parse_*`/macro helpers all `return` from this fn
    // (mirroring the reference's `inline for` arms that `return @unionInit(...)`),
    // so the few bare-`return` arms (e.g. `cursor_key`) keep the match arms a
    // uniform diverging type. The `needless_return` lint is allowed for that
    // shape.
    #[allow(clippy::needless_return)]
    pub fn parse(input: &[u8]) -> Result<Action, Error> {
        // Split our action by colon. A colon may not exist for some
        // actions so it is optional. The part preceding the colon is the
        // action name.
        let colon_idx = input.iter().position(|&c| c == b':');
        let action = &input[0..colon_idx.unwrap_or(input.len())];

        // An action name is always required
        if action.is_empty() {
            return Err(Error::InvalidFormat);
        }

        // PORT NOTE: the reference does `inline for (actionInfo.fields)` matching
        // `action == field.name` then dispatching on `field.type`. Here the match
        // is over the action name bytes; each arm reproduces the reference's
        // field-type dispatch:
        //   - void arm: `if (colonIdx != null) return InvalidFormat;`
        //   - []const u8 arm: requires a colon, takes the rest as the param
        //   - CursorKey arm: `return Error.InvalidAction;`
        //   - else arm: optional colon w/ `default` decl, else `parseParameter`.
        // A trailing unmatched name is `Error.InvalidAction`.
        //
        // Helpers for the per-type arms:
        let param_after = |idx: usize| -> &[u8] { &input[idx + 1..] };

        macro_rules! void_arm {
            ($variant:expr) => {{
                // If the field type is void we expect no value
                if colon_idx.is_some() {
                    return Err(Error::InvalidFormat);
                }
                return Ok($variant);
            }};
        }
        macro_rules! string_arm {
            ($variant:path) => {{
                let idx = colon_idx.ok_or(Error::InvalidFormat)?;
                let param = param_after(idx);
                return Ok($variant(param.to_vec()));
            }};
        }
        // else-arm with `default` decl: optional colon.
        macro_rules! enum_arm_default {
            ($ty:ty, $variant:path, $default:expr) => {{
                // Get the parameter after the colon. The parameter
                // can be optional for action types that can have a
                // "default" decl.
                match colon_idx {
                    None => return Ok($variant($default)),
                    Some(idx) => {
                        let param = param_after(idx);
                        let v = <$ty>::string_to_enum(param).ok_or(Error::InvalidFormat)?;
                        return Ok($variant(v));
                    }
                }
            }};
        }
        // else-arm enum WITHOUT default: colon required.
        macro_rules! enum_arm {
            ($ty:ty, $variant:path) => {{
                let idx = colon_idx.ok_or(Error::InvalidFormat)?;
                let param = param_after(idx);
                let v = <$ty>::string_to_enum(param).ok_or(Error::InvalidFormat)?;
                return Ok($variant(v));
            }};
        }
        // else-arm with a custom `parse` decl (no default): colon required.
        macro_rules! parse_arm {
            ($variant:path, $parsefn:expr) => {{
                let idx = colon_idx.ok_or(Error::InvalidFormat)?;
                let param = param_after(idx);
                return Ok($variant($parsefn(param)?));
            }};
        }
        // else-arm int/float (no default): colon required.
        macro_rules! num_arm {
            ($variant:path, $parsefn:expr) => {{
                let idx = colon_idx.ok_or(Error::InvalidFormat)?;
                let param = param_after(idx);
                return Ok($variant($parsefn(param)?));
            }};
        }

        match action {
            b"ignore" => void_arm!(Action::Ignore),
            b"unbind" => void_arm!(Action::Unbind),
            b"csi" => string_arm!(Action::Csi),
            b"esc" => string_arm!(Action::Esc),
            b"text" => string_arm!(Action::Text),
            // Cursor keys can't be set currently
            b"cursor_key" => return Err(Error::InvalidAction),
            b"reset" => void_arm!(Action::Reset),
            b"copy_to_clipboard" => {
                enum_arm_default!(
                    CopyToClipboard,
                    Action::CopyToClipboard,
                    CopyToClipboard::DEFAULT
                )
            }
            b"paste_from_clipboard" => void_arm!(Action::PasteFromClipboard),
            b"paste_from_selection" => void_arm!(Action::PasteFromSelection),
            b"copy_url_to_clipboard" => void_arm!(Action::CopyUrlToClipboard),
            b"copy_title_to_clipboard" => void_arm!(Action::CopyTitleToClipboard),
            b"increase_font_size" => num_arm!(Action::IncreaseFontSize, parse_float_f32),
            b"decrease_font_size" => num_arm!(Action::DecreaseFontSize, parse_float_f32),
            b"reset_font_size" => void_arm!(Action::ResetFontSize),
            b"set_font_size" => num_arm!(Action::SetFontSize, parse_float_f32),
            b"search" => string_arm!(Action::Search),
            b"search_selection" => void_arm!(Action::SearchSelection),
            b"navigate_search" => enum_arm!(NavigateSearch, Action::NavigateSearch),
            b"start_search" => void_arm!(Action::StartSearch),
            b"end_search" => void_arm!(Action::EndSearch),
            b"clear_screen" => void_arm!(Action::ClearScreen),
            b"select_all" => void_arm!(Action::SelectAll),
            b"scroll_to_top" => void_arm!(Action::ScrollToTop),
            b"scroll_to_bottom" => void_arm!(Action::ScrollToBottom),
            b"scroll_to_selection" => void_arm!(Action::ScrollToSelection),
            b"scroll_to_row" => num_arm!(Action::ScrollToRow, parse_int_usize),
            b"scroll_page_up" => void_arm!(Action::ScrollPageUp),
            b"scroll_page_down" => void_arm!(Action::ScrollPageDown),
            b"scroll_page_fractional" => num_arm!(Action::ScrollPageFractional, parse_float_f32),
            b"scroll_page_lines" => num_arm!(Action::ScrollPageLines, parse_int_i16),
            b"adjust_selection" => enum_arm!(AdjustSelection, Action::AdjustSelection),
            b"jump_to_prompt" => num_arm!(Action::JumpToPrompt, parse_int_i16),
            b"write_scrollback_file" => parse_arm!(Action::WriteScrollbackFile, WriteScreen::parse),
            b"write_screen_file" => parse_arm!(Action::WriteScreenFile, WriteScreen::parse),
            b"write_selection_file" => parse_arm!(Action::WriteSelectionFile, WriteScreen::parse),
            b"new_window" => void_arm!(Action::NewWindow),
            b"new_tab" => void_arm!(Action::NewTab),
            b"previous_tab" => void_arm!(Action::PreviousTab),
            b"next_tab" => void_arm!(Action::NextTab),
            b"last_tab" => void_arm!(Action::LastTab),
            // R13 (UD58): bounded by `GOTO_TAB_MAX`.
            b"goto_tab" => num_arm!(Action::GotoTab, parse_int_goto_tab),
            b"move_tab" => num_arm!(Action::MoveTab, parse_int_isize),
            b"move_tab_to_new_window" => void_arm!(Action::MoveTabToNewWindow),
            b"toggle_tab_overview" => void_arm!(Action::ToggleTabOverview),
            b"prompt_surface_title" => void_arm!(Action::PromptSurfaceTitle),
            b"prompt_tab_title" => void_arm!(Action::PromptTabTitle),
            b"prompt_window_title" => void_arm!(Action::PromptWindowTitle),
            b"set_surface_title" => string_arm!(Action::SetSurfaceTitle),
            b"set_tab_title" => string_arm!(Action::SetTabTitle),
            b"set_window_title" => string_arm!(Action::SetWindowTitle),
            b"new_split" => {
                enum_arm_default!(SplitDirection, Action::NewSplit, SplitDirection::DEFAULT)
            }
            b"goto_split" => parse_arm!(Action::GotoSplit, SplitFocusDirection::parse),
            b"goto_window" => enum_arm!(GotoWindow, Action::GotoWindow),
            b"toggle_split_zoom" => void_arm!(Action::ToggleSplitZoom),
            b"toggle_readonly" => void_arm!(Action::ToggleReadonly),
            b"resize_split" => parse_arm!(Action::ResizeSplit, parse_resize_split),
            b"equalize_splits" => void_arm!(Action::EqualizeSplits),
            b"reset_window_size" => void_arm!(Action::ResetWindowSize),
            b"inspector" => enum_arm!(InspectorMode, Action::Inspector),
            b"show_gtk_inspector" => void_arm!(Action::ShowGtkInspector),
            b"show_on_screen_keyboard" => void_arm!(Action::ShowOnScreenKeyboard),
            b"open_config" => {
                enum_arm_default!(OpenConfig, Action::OpenConfig, OpenConfig::DEFAULT)
            }
            b"reload_config" => void_arm!(Action::ReloadConfig),
            b"close_surface" => void_arm!(Action::CloseSurface),
            b"close_tab" => {
                enum_arm_default!(CloseTabMode, Action::CloseTab, CloseTabMode::DEFAULT)
            }
            b"close_window" => void_arm!(Action::CloseWindow),
            b"close_all_windows" => void_arm!(Action::CloseAllWindows),
            b"toggle_maximize" => void_arm!(Action::ToggleMaximize),
            b"toggle_fullscreen" => void_arm!(Action::ToggleFullscreen),
            b"toggle_window_decorations" => void_arm!(Action::ToggleWindowDecorations),
            b"toggle_window_float_on_top" => void_arm!(Action::ToggleWindowFloatOnTop),
            b"toggle_secure_input" => void_arm!(Action::ToggleSecureInput),
            b"toggle_mouse_reporting" => void_arm!(Action::ToggleMouseReporting),
            b"toggle_command_palette" => void_arm!(Action::ToggleCommandPalette),
            b"toggle_quick_terminal" => void_arm!(Action::ToggleQuickTerminal),
            b"toggle_visibility" => void_arm!(Action::ToggleVisibility),
            b"toggle_background_opacity" => void_arm!(Action::ToggleBackgroundOpacity),
            b"check_for_updates" => void_arm!(Action::CheckForUpdates),
            b"undo" => void_arm!(Action::Undo),
            b"redo" => void_arm!(Action::Redo),
            b"end_key_sequence" => void_arm!(Action::EndKeySequence),
            b"activate_key_table" => string_arm!(Action::ActivateKeyTable),
            b"activate_key_table_once" => string_arm!(Action::ActivateKeyTableOnce),
            b"deactivate_key_table" => void_arm!(Action::DeactivateKeyTable),
            b"deactivate_all_key_tables" => void_arm!(Action::DeactivateAllKeyTables),
            b"quit" => void_arm!(Action::Quit),
            b"crash" => enum_arm!(CrashThread, Action::Crash),
            _ => Err(Error::InvalidAction),
        }
    }
}

/// The scope of an action. The scope is the context in which an action
/// must be executed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    App,
    Surface,
}

/// Returns a union type that only contains actions that are scoped to
/// the given scope.
//
// PORT NOTE (core C4): the reference is the comptime type function
// `Action.Scoped(comptime s: Scope) type` (Binding.zig:1436) —
//
//     @setEvalBranchQuota(100_000);
//     const all_fields = @typeInfo(Action).@"union".fields;
//     // Find all fields that are app-scoped
//     ...
//     // Build our union
//     return @Type(.{ .@"union" = ... });
//
// Rust has no comptime type generation, so this is the HAND-WRITTEN
// `Scoped(.app)` instantiation: exactly the fields whose `scope() == .app`, in
// the `Action` union's declaration order (the order the comptime loop
// preserves). Every app-scoped action except `open_config` is a void field
// (`open_config` carries its `OpenConfig` enum payload, `Copy` by value).
// `Scoped(.surface)` is hand-written the same way with
// its consumer (core C14 `performBindingAction`); see the note at
// [`Action::scoped_app`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScopedApp {
    Ignore,
    Unbind,
    NewWindow,
    ShowGtkInspector,
    OpenConfig(OpenConfig),
    ReloadConfig,
    CloseAllWindows,
    ToggleQuickTerminal,
    ToggleVisibility,
    CheckForUpdates,
    Undo,
    Redo,
    Quit,
}

/// Returns a union type that only contains actions that are scoped to
/// the surface.
//
// PORT NOTE (core C14 `performBindingAction`): the `Scoped(.surface)`
// instantiation of the comptime `Action.Scoped(comptime s: Scope) type`
// (Binding.zig:1436) — the hand-written surface counterpart of [`ScopedApp`],
// landed WITH its consumer (`Surface.performBindingAction`, Surface.zig:4809
// `switch (action.scoped(.surface).?)`), exactly as the note at
// [`Action::scoped_app`] promised. The variants are the fields whose
// `scope() == .surface`, in the `Action` union's declaration order (the order
// the comptime field loop preserves). Unlike `ScopedApp`, surface actions
// carry payloads: the reference union shares the Action field TYPES (Zig
// slices are copied headers aliasing the source), so the byte-slice arms
// (`Csi`/`Esc`/`Text`/`Search`/`SetSurfaceTitle`/`SetTabTitle`/
// `ActivateKeyTable`/`ActivateKeyTableOnce`) and the owned-`Vec` `CursorKey`
// BORROW the projected `Action` (`&'a [u8]` / `&'a CursorKey`); the `Copy`
// payloads are carried by value. The type has no `test {}` blocks in the
// reference (it is a comptime-generated dispatch type).
#[derive(Clone, Copy, Debug)]
pub enum ScopedSurface<'a> {
    Csi(&'a [u8]),
    Esc(&'a [u8]),
    Text(&'a [u8]),
    CursorKey(&'a CursorKey),
    Reset,
    CopyToClipboard(CopyToClipboard),
    PasteFromClipboard,
    PasteFromSelection,
    CopyUrlToClipboard,
    CopyTitleToClipboard,
    IncreaseFontSize(f32),
    DecreaseFontSize(f32),
    ResetFontSize,
    SetFontSize(f32),
    Search(&'a [u8]),
    SearchSelection,
    NavigateSearch(NavigateSearch),
    StartSearch,
    EndSearch,
    ClearScreen,
    SelectAll,
    ScrollToTop,
    ScrollToBottom,
    ScrollToSelection,
    ScrollToRow(usize),
    ScrollPageUp,
    ScrollPageDown,
    ScrollPageFractional(f32),
    ScrollPageLines(i16),
    AdjustSelection(AdjustSelection),
    JumpToPrompt(i16),
    WriteScrollbackFile(WriteScreen),
    WriteScreenFile(WriteScreen),
    WriteSelectionFile(WriteScreen),
    NewTab,
    PreviousTab,
    NextTab,
    LastTab,
    GotoTab(usize),
    MoveTab(isize),
    MoveTabToNewWindow,
    ToggleTabOverview,
    PromptSurfaceTitle,
    PromptTabTitle,
    PromptWindowTitle,
    SetSurfaceTitle(&'a [u8]),
    SetTabTitle(&'a [u8]),
    SetWindowTitle(&'a [u8]),
    NewSplit(SplitDirection),
    GotoSplit(SplitFocusDirection),
    GotoWindow(GotoWindow),
    ToggleSplitZoom,
    ToggleReadonly,
    ResizeSplit(SplitResizeParameter),
    EqualizeSplits,
    ResetWindowSize,
    Inspector(InspectorMode),
    ShowOnScreenKeyboard,
    CloseSurface,
    CloseTab(CloseTabMode),
    CloseWindow,
    ToggleMaximize,
    ToggleFullscreen,
    ToggleWindowDecorations,
    ToggleWindowFloatOnTop,
    ToggleSecureInput,
    ToggleMouseReporting,
    ToggleCommandPalette,
    ToggleBackgroundOpacity,
    EndKeySequence,
    ActivateKeyTable(&'a [u8]),
    ActivateKeyTableOnce(&'a [u8]),
    DeactivateKeyTable,
    DeactivateAllKeyTables,
    Crash(CrashThread),
}

impl Action {
    /// Returns the scope of an action.
    pub fn scope(&self) -> Scope {
        match self {
            // Doesn't really matter, so we'll see app.
            Action::Ignore | Action::Unbind => Scope::App,

            // Obviously app actions.
            Action::OpenConfig(_)
            | Action::ReloadConfig
            | Action::CloseAllWindows
            | Action::Quit
            | Action::ToggleQuickTerminal
            | Action::ToggleVisibility
            | Action::CheckForUpdates
            | Action::ShowGtkInspector => Scope::App,

            // These are app but can be special-cased in a surface context.
            Action::NewWindow | Action::Undo | Action::Redo => Scope::App,

            // Obviously surface actions.
            Action::Csi(_)
            | Action::Esc(_)
            | Action::Text(_)
            | Action::CursorKey(_)
            | Action::Search(_)
            | Action::NavigateSearch(_)
            | Action::SearchSelection
            | Action::StartSearch
            | Action::EndSearch
            | Action::Reset
            | Action::CopyToClipboard(_)
            | Action::CopyUrlToClipboard
            | Action::CopyTitleToClipboard
            | Action::PasteFromClipboard
            | Action::PasteFromSelection
            | Action::IncreaseFontSize(_)
            | Action::DecreaseFontSize(_)
            | Action::ResetFontSize
            | Action::SetFontSize(_)
            | Action::PromptSurfaceTitle
            | Action::PromptTabTitle
            | Action::PromptWindowTitle
            | Action::SetSurfaceTitle(_)
            | Action::SetTabTitle(_)
            | Action::SetWindowTitle(_)
            | Action::ClearScreen
            | Action::SelectAll
            | Action::ScrollToTop
            | Action::ScrollToBottom
            | Action::ScrollToSelection
            | Action::ScrollToRow(_)
            | Action::ScrollPageUp
            | Action::ScrollPageDown
            | Action::ScrollPageFractional(_)
            | Action::ScrollPageLines(_)
            | Action::AdjustSelection(_)
            | Action::JumpToPrompt(_)
            | Action::WriteScrollbackFile(_)
            | Action::WriteScreenFile(_)
            | Action::WriteSelectionFile(_)
            | Action::CloseSurface
            | Action::CloseTab(_)
            | Action::CloseWindow
            | Action::ToggleMaximize
            | Action::ToggleFullscreen
            | Action::ToggleWindowDecorations
            | Action::ToggleWindowFloatOnTop
            | Action::ToggleSecureInput
            | Action::ToggleMouseReporting
            | Action::ToggleCommandPalette
            | Action::ToggleBackgroundOpacity
            | Action::ShowOnScreenKeyboard
            | Action::ResetWindowSize
            | Action::ActivateKeyTable(_)
            | Action::ActivateKeyTableOnce(_)
            | Action::DeactivateKeyTable
            | Action::DeactivateAllKeyTables
            | Action::EndKeySequence
            | Action::Crash(_) => Scope::Surface,

            // These are less obvious surface actions. They're surface
            // actions because they are relevant to the surface they
            // come from. For example `new_window` needs to be sourced to
            // a surface so inheritance can be done correctly.
            Action::NewTab
            | Action::PreviousTab
            | Action::NextTab
            | Action::LastTab
            | Action::GotoTab(_)
            | Action::MoveTab(_)
            | Action::MoveTabToNewWindow
            | Action::ToggleTabOverview
            | Action::NewSplit(_)
            | Action::GotoSplit(_)
            | Action::GotoWindow(_)
            | Action::ToggleSplitZoom
            | Action::ToggleReadonly
            | Action::ResizeSplit(_)
            | Action::EqualizeSplits
            | Action::Inspector(_) => Scope::Surface,
        }
    }

    // PORT NOTE (core C4): `Scoped(comptime s: Scope) type` + `scoped(self,
    // comptime s) ?Scoped(s)` build, at comptime, a union containing ONLY the
    // actions whose `scope() == s`, and project the current action into it (or
    // null). These produce comptime-generated TYPES consumed by the apprt/core
    // dispatch layer; there are no `test {}` blocks for them in this file. Rust
    // has no comptime type generation, so each scope's union is written out BY
    // HAND, in the `Action` declaration order the comptime field loop preserves.
    // `Scoped(.app)` is [`ScopedApp`] below (all app-scoped actions are
    // void-payload, so it is a fieldless enum) + [`Action::scoped_app`] — landed
    // WITH its first consumer (core `App.performAction`/`keyEvent`, App.zig:420/
    // 371). `Scoped(.surface)` still lands with ITS consumer (core C14
    // `Surface.performBindingAction`, Surface.zig:4775) — the scoped-union types
    // live with their consumers. The scope partition itself is fully ported
    // above as `scope()`.

    /// Returns the scoped version of this action. If the action is not
    /// scoped to the given scope then this returns null.
    ///
    /// The benefit of this function is that it allows us to use Zig's
    /// exhaustive switch safety to ensure we always properly handle certain
    /// scoped actions.
    //
    // PORT NOTE: `scoped(self, comptime s)` monomorphizes per scope; this is
    // the `s = .app` instantiation (see the note above).
    pub fn scoped_app(&self) -> Option<ScopedApp> {
        // switch (self) { inline else => |v, tag| { ... } }
        match self {
            // Use comptime to prune out non-app actions
            //
            // Initialize our app action
            Action::Ignore => Some(ScopedApp::Ignore),
            Action::Unbind => Some(ScopedApp::Unbind),
            Action::NewWindow => Some(ScopedApp::NewWindow),
            Action::ShowGtkInspector => Some(ScopedApp::ShowGtkInspector),
            Action::OpenConfig(v) => Some(ScopedApp::OpenConfig(*v)),
            Action::ReloadConfig => Some(ScopedApp::ReloadConfig),
            Action::CloseAllWindows => Some(ScopedApp::CloseAllWindows),
            Action::ToggleQuickTerminal => Some(ScopedApp::ToggleQuickTerminal),
            Action::ToggleVisibility => Some(ScopedApp::ToggleVisibility),
            Action::CheckForUpdates => Some(ScopedApp::CheckForUpdates),
            Action::Undo => Some(ScopedApp::Undo),
            Action::Redo => Some(ScopedApp::Redo),
            Action::Quit => Some(ScopedApp::Quit),
            _ => None,
        }
    }

    /// Returns the scoped version of this action for the surface scope. If the
    /// action is not surface-scoped then this returns null.
    //
    // PORT NOTE: `scoped(self, comptime s)` monomorphizes per scope; this is
    // the `s = .surface` instantiation (see the note at [`Action::scoped_app`]
    // and [`ScopedSurface`]). Mirrors the reference's `inline else` + comptime
    // `scope() != s` prune, written out by hand: the surface arms project into
    // [`ScopedSurface`] (byte-slice/`CursorKey` payloads borrowed, `Copy`
    // payloads by value), the app arms return `None`. The partition is proven
    // consistent with [`Action::scope`] above.
    pub fn scoped_surface(&self) -> Option<ScopedSurface<'_>> {
        match self {
            Action::Csi(v) => Some(ScopedSurface::Csi(v.as_slice())),
            Action::Esc(v) => Some(ScopedSurface::Esc(v.as_slice())),
            Action::Text(v) => Some(ScopedSurface::Text(v.as_slice())),
            Action::CursorKey(v) => Some(ScopedSurface::CursorKey(v)),
            Action::Reset => Some(ScopedSurface::Reset),
            Action::CopyToClipboard(v) => Some(ScopedSurface::CopyToClipboard(*v)),
            Action::PasteFromClipboard => Some(ScopedSurface::PasteFromClipboard),
            Action::PasteFromSelection => Some(ScopedSurface::PasteFromSelection),
            Action::CopyUrlToClipboard => Some(ScopedSurface::CopyUrlToClipboard),
            Action::CopyTitleToClipboard => Some(ScopedSurface::CopyTitleToClipboard),
            Action::IncreaseFontSize(v) => Some(ScopedSurface::IncreaseFontSize(*v)),
            Action::DecreaseFontSize(v) => Some(ScopedSurface::DecreaseFontSize(*v)),
            Action::ResetFontSize => Some(ScopedSurface::ResetFontSize),
            Action::SetFontSize(v) => Some(ScopedSurface::SetFontSize(*v)),
            Action::Search(v) => Some(ScopedSurface::Search(v.as_slice())),
            Action::SearchSelection => Some(ScopedSurface::SearchSelection),
            Action::NavigateSearch(v) => Some(ScopedSurface::NavigateSearch(*v)),
            Action::StartSearch => Some(ScopedSurface::StartSearch),
            Action::EndSearch => Some(ScopedSurface::EndSearch),
            Action::ClearScreen => Some(ScopedSurface::ClearScreen),
            Action::SelectAll => Some(ScopedSurface::SelectAll),
            Action::ScrollToTop => Some(ScopedSurface::ScrollToTop),
            Action::ScrollToBottom => Some(ScopedSurface::ScrollToBottom),
            Action::ScrollToSelection => Some(ScopedSurface::ScrollToSelection),
            Action::ScrollToRow(v) => Some(ScopedSurface::ScrollToRow(*v)),
            Action::ScrollPageUp => Some(ScopedSurface::ScrollPageUp),
            Action::ScrollPageDown => Some(ScopedSurface::ScrollPageDown),
            Action::ScrollPageFractional(v) => Some(ScopedSurface::ScrollPageFractional(*v)),
            Action::ScrollPageLines(v) => Some(ScopedSurface::ScrollPageLines(*v)),
            Action::AdjustSelection(v) => Some(ScopedSurface::AdjustSelection(*v)),
            Action::JumpToPrompt(v) => Some(ScopedSurface::JumpToPrompt(*v)),
            Action::WriteScrollbackFile(v) => Some(ScopedSurface::WriteScrollbackFile(*v)),
            Action::WriteScreenFile(v) => Some(ScopedSurface::WriteScreenFile(*v)),
            Action::WriteSelectionFile(v) => Some(ScopedSurface::WriteSelectionFile(*v)),
            Action::NewTab => Some(ScopedSurface::NewTab),
            Action::PreviousTab => Some(ScopedSurface::PreviousTab),
            Action::NextTab => Some(ScopedSurface::NextTab),
            Action::LastTab => Some(ScopedSurface::LastTab),
            Action::GotoTab(v) => Some(ScopedSurface::GotoTab(*v)),
            Action::MoveTab(v) => Some(ScopedSurface::MoveTab(*v)),
            Action::MoveTabToNewWindow => Some(ScopedSurface::MoveTabToNewWindow),
            Action::ToggleTabOverview => Some(ScopedSurface::ToggleTabOverview),
            Action::PromptSurfaceTitle => Some(ScopedSurface::PromptSurfaceTitle),
            Action::PromptTabTitle => Some(ScopedSurface::PromptTabTitle),
            Action::PromptWindowTitle => Some(ScopedSurface::PromptWindowTitle),
            Action::SetSurfaceTitle(v) => Some(ScopedSurface::SetSurfaceTitle(v.as_slice())),
            Action::SetTabTitle(v) => Some(ScopedSurface::SetTabTitle(v.as_slice())),
            Action::SetWindowTitle(v) => Some(ScopedSurface::SetWindowTitle(v.as_slice())),
            Action::NewSplit(v) => Some(ScopedSurface::NewSplit(*v)),
            Action::GotoSplit(v) => Some(ScopedSurface::GotoSplit(*v)),
            Action::GotoWindow(v) => Some(ScopedSurface::GotoWindow(*v)),
            Action::ToggleSplitZoom => Some(ScopedSurface::ToggleSplitZoom),
            Action::ToggleReadonly => Some(ScopedSurface::ToggleReadonly),
            Action::ResizeSplit(v) => Some(ScopedSurface::ResizeSplit(*v)),
            Action::EqualizeSplits => Some(ScopedSurface::EqualizeSplits),
            Action::ResetWindowSize => Some(ScopedSurface::ResetWindowSize),
            Action::Inspector(v) => Some(ScopedSurface::Inspector(*v)),
            Action::ShowOnScreenKeyboard => Some(ScopedSurface::ShowOnScreenKeyboard),
            Action::CloseSurface => Some(ScopedSurface::CloseSurface),
            Action::CloseTab(v) => Some(ScopedSurface::CloseTab(*v)),
            Action::CloseWindow => Some(ScopedSurface::CloseWindow),
            Action::ToggleMaximize => Some(ScopedSurface::ToggleMaximize),
            Action::ToggleFullscreen => Some(ScopedSurface::ToggleFullscreen),
            Action::ToggleWindowDecorations => Some(ScopedSurface::ToggleWindowDecorations),
            Action::ToggleWindowFloatOnTop => Some(ScopedSurface::ToggleWindowFloatOnTop),
            Action::ToggleSecureInput => Some(ScopedSurface::ToggleSecureInput),
            Action::ToggleMouseReporting => Some(ScopedSurface::ToggleMouseReporting),
            Action::ToggleCommandPalette => Some(ScopedSurface::ToggleCommandPalette),
            Action::ToggleBackgroundOpacity => Some(ScopedSurface::ToggleBackgroundOpacity),
            Action::EndKeySequence => Some(ScopedSurface::EndKeySequence),
            Action::ActivateKeyTable(v) => Some(ScopedSurface::ActivateKeyTable(v.as_slice())),
            Action::ActivateKeyTableOnce(v) => {
                Some(ScopedSurface::ActivateKeyTableOnce(v.as_slice()))
            }
            Action::DeactivateKeyTable => Some(ScopedSurface::DeactivateKeyTable),
            Action::DeactivateAllKeyTables => Some(ScopedSurface::DeactivateAllKeyTables),
            Action::Crash(v) => Some(ScopedSurface::Crash(*v)),
            _ => None,
        }
    }

    /// Implements the formatter for the fmt package. This encodes the
    /// action back into the format used by parse.
    pub fn format<W: Writer>(&self, writer: &mut W) -> Result<(), W::Error> {
        // switch (self) { inline else => |value| { ... } }
        //
        // All actions start with the tag.
        // try writer.print("{s}", .{@tagName(self)});
        writer.write_all(self.tag_name().as_bytes())?;

        // Only write the value depending on the type if it's not void
        // PORT NOTE: `if (@TypeOf(value) != void)` — the value-carrying arms
        // write `:` then the formatted value via `formatValue`. The void arms
        // (unit variants) write nothing more.
        match self {
            // void arms: nothing more.
            Action::Ignore
            | Action::Unbind
            | Action::Reset
            | Action::PasteFromClipboard
            | Action::PasteFromSelection
            | Action::CopyUrlToClipboard
            | Action::CopyTitleToClipboard
            | Action::ResetFontSize
            | Action::SearchSelection
            | Action::StartSearch
            | Action::EndSearch
            | Action::ClearScreen
            | Action::SelectAll
            | Action::ScrollToTop
            | Action::ScrollToBottom
            | Action::ScrollToSelection
            | Action::ScrollPageUp
            | Action::ScrollPageDown
            | Action::NewWindow
            | Action::NewTab
            | Action::PreviousTab
            | Action::NextTab
            | Action::LastTab
            | Action::MoveTabToNewWindow
            | Action::ToggleTabOverview
            | Action::PromptSurfaceTitle
            | Action::PromptTabTitle
            | Action::PromptWindowTitle
            | Action::ToggleSplitZoom
            | Action::ToggleReadonly
            | Action::EqualizeSplits
            | Action::ResetWindowSize
            | Action::ShowGtkInspector
            | Action::ShowOnScreenKeyboard
            | Action::ReloadConfig
            | Action::CloseSurface
            | Action::CloseWindow
            | Action::CloseAllWindows
            | Action::ToggleMaximize
            | Action::ToggleFullscreen
            | Action::ToggleWindowDecorations
            | Action::ToggleWindowFloatOnTop
            | Action::ToggleSecureInput
            | Action::ToggleMouseReporting
            | Action::ToggleCommandPalette
            | Action::ToggleQuickTerminal
            | Action::ToggleVisibility
            | Action::ToggleBackgroundOpacity
            | Action::CheckForUpdates
            | Action::Undo
            | Action::Redo
            | Action::EndKeySequence
            | Action::DeactivateKeyTable
            | Action::DeactivateAllKeyTables
            | Action::Quit => Ok(()),

            // value-carrying arms.
            Action::Csi(v)
            | Action::Esc(v)
            | Action::Text(v)
            | Action::Search(v)
            | Action::SetSurfaceTitle(v)
            | Action::SetTabTitle(v)
            | Action::SetWindowTitle(v)
            | Action::ActivateKeyTable(v)
            | Action::ActivateKeyTableOnce(v) => {
                writer.write_all(b":")?;
                format_value_string(writer, v)
            }
            Action::CursorKey(v) => {
                writer.write_all(b":")?;
                format_value_struct(writer, |w| v.format(w))
            }
            Action::CopyToClipboard(v) => {
                writer.write_all(b":")?;
                format_value_enum(writer, v.tag_name())
            }
            Action::IncreaseFontSize(v)
            | Action::DecreaseFontSize(v)
            | Action::SetFontSize(v)
            | Action::ScrollPageFractional(v) => {
                writer.write_all(b":")?;
                format_value_float(writer, *v)
            }
            Action::ScrollToRow(v) | Action::GotoTab(v) => {
                writer.write_all(b":")?;
                format_value_int_usize(writer, *v)
            }
            Action::ScrollPageLines(v) | Action::JumpToPrompt(v) => {
                writer.write_all(b":")?;
                format_value_int_i16(writer, *v)
            }
            Action::MoveTab(v) => {
                writer.write_all(b":")?;
                format_value_int_isize(writer, *v)
            }
            Action::NavigateSearch(v) => {
                writer.write_all(b":")?;
                format_value_enum(writer, v.tag_name())
            }
            Action::AdjustSelection(v) => {
                writer.write_all(b":")?;
                format_value_enum(writer, v.tag_name())
            }
            Action::WriteScrollbackFile(v)
            | Action::WriteScreenFile(v)
            | Action::WriteSelectionFile(v) => {
                writer.write_all(b":")?;
                format_value_struct(writer, |w| v.format(w))
            }
            Action::NewSplit(v) => {
                writer.write_all(b":")?;
                format_value_enum(writer, v.tag_name())
            }
            Action::GotoSplit(v) => {
                writer.write_all(b":")?;
                format_value_enum(writer, v.tag_name())
            }
            Action::GotoWindow(v) => {
                writer.write_all(b":")?;
                format_value_enum(writer, v.tag_name())
            }
            Action::ResizeSplit(v) => {
                writer.write_all(b":")?;
                // .@"struct" tuple arm of formatValue: format each field comma-
                // separated. SplitResizeParameter = (SplitResizeDirection, u16).
                format_value_enum(writer, v.0.tag_name())?;
                writer.write_all(b",")?;
                format_value_int_u16(writer, v.1)
            }
            Action::Inspector(v) => {
                writer.write_all(b":")?;
                format_value_enum(writer, v.tag_name())
            }
            Action::OpenConfig(v) => {
                writer.write_all(b":")?;
                format_value_enum(writer, v.tag_name())
            }
            Action::CloseTab(v) => {
                writer.write_all(b":")?;
                format_value_enum(writer, v.tag_name())
            }
            Action::Crash(v) => {
                writer.write_all(b":")?;
                format_value_enum(writer, v.tag_name())
            }
        }
    }
}

// fn formatValue(writer, value: anytype) — the per-type value formatter used by
// `Action.format`. The reference switches on `@TypeOf(value)` then
// `@typeInfo(Value)`. Each Rust helper below reproduces one arm.

// []const u8 => try writer.print("{s}", .{value}),
fn format_value_string<W: Writer>(writer: &mut W, value: &[u8]) -> Result<(), W::Error> {
    writer.write_all(value)
}

// .@"enum" => try writer.print("{t}", .{value}),
fn format_value_enum<W: Writer>(writer: &mut W, tag_name: &str) -> Result<(), W::Error> {
    writer.write_all(tag_name.as_bytes())
}

// .float => try writer.print("{d}", .{value}),
//
// PORT NOTE: Zig's `{d}` prints the shortest round-trippable decimal; Rust's
// `Display` for `f32` matches (e.g. `0.5`).
fn format_value_float<W: Writer>(writer: &mut W, value: f32) -> Result<(), W::Error> {
    writer.write_all(format!("{value}").as_bytes())
}

// .int => try writer.print("{d}", .{value}),
fn format_value_int_usize<W: Writer>(writer: &mut W, value: usize) -> Result<(), W::Error> {
    writer.write_all(format!("{value}").as_bytes())
}
fn format_value_int_isize<W: Writer>(writer: &mut W, value: isize) -> Result<(), W::Error> {
    writer.write_all(format!("{value}").as_bytes())
}
fn format_value_int_i16<W: Writer>(writer: &mut W, value: i16) -> Result<(), W::Error> {
    writer.write_all(format!("{value}").as_bytes())
}
fn format_value_int_u16<W: Writer>(writer: &mut W, value: u16) -> Result<(), W::Error> {
    writer.write_all(format!("{value}").as_bytes())
}

// .@"struct" => |info| format: { if (@hasDecl(Value, "format")) { try value.format(writer); ... } ... }
//
// PORT NOTE: the struct arm calls the value's own `format`. Both `CursorKey` and
// `WriteScreen` declare a `format`; this thunk just invokes the closure that
// calls it. (The non-tuple/no-format `@compileError` branch can never be reached
// by a constructed `Action`, and the tuple branch is inlined directly for
// `resize_split` in `Action::format`.)
fn format_value_struct<W: Writer, F>(_writer: &mut W, f: F) -> Result<(), W::Error>
where
    F: FnOnce(&mut W) -> Result<(), W::Error>,
{
    f(_writer)
}

impl Action {
    /// Clone this action with the given allocator. The allocator
    /// should be an arena-style allocator since fine-grained
    /// deallocation is not possible.
    //
    // PORT NOTE: `clone(self, alloc) Allocator.Error!Action`. The reference
    // dupes slices and clones structs through the allocator; Rust's `Clone`
    // deep-copies owned `Vec<u8>`s and sub-structs, so `clone_action` is just
    // `self.clone()`. Kept as a named method to mirror the reference call sites
    // (`action.clone(alloc)`). `cloneValue` is folded into the derived `Clone`.
    pub fn clone_action(&self) -> Action {
        self.clone()
    }

    /// Returns a hash code that can be used to uniquely identify this
    /// action.
    pub fn hash(&self) -> u64 {
        // var hasher = std.hash.Wyhash.init(0);
        // self.hashIncremental(&hasher);
        // return hasher.final();
        let mut hasher = DefaultHasher::new();
        self.hash_incremental(&mut hasher);
        hasher.finish()
    }

    /// Hash the action into the given hasher.
    //
    // PORT NOTE: the reference hashes the active tag then the field value
    // (floats by bits; everything else via `autoHashStrat(.DeepRecursive)`).
    // This hash is internal to the `Set` (never oracle-compared), so any
    // self-consistent hash that distinguishes distinct actions is parity-
    // preserving — we hash the tag ordinal then each field's bytes/bits, in the
    // same field structure as the reference.
    pub(super) fn hash_incremental(&self, hasher: &mut DefaultHasher) {
        // Always has the active tag.
        // const Tag = @typeInfo(Action).@"union".tag_type.?;
        // std.hash.autoHash(hasher, @as(Tag, self));
        hasher.write(self.tag_name().as_bytes());

        // Hash the value of the field.
        match self {
            // Do nothing for void
            Action::Ignore
            | Action::Unbind
            | Action::Reset
            | Action::PasteFromClipboard
            | Action::PasteFromSelection
            | Action::CopyUrlToClipboard
            | Action::CopyTitleToClipboard
            | Action::ResetFontSize
            | Action::SearchSelection
            | Action::StartSearch
            | Action::EndSearch
            | Action::ClearScreen
            | Action::SelectAll
            | Action::ScrollToTop
            | Action::ScrollToBottom
            | Action::ScrollToSelection
            | Action::ScrollPageUp
            | Action::ScrollPageDown
            | Action::NewWindow
            | Action::NewTab
            | Action::PreviousTab
            | Action::NextTab
            | Action::LastTab
            | Action::MoveTabToNewWindow
            | Action::ToggleTabOverview
            | Action::PromptSurfaceTitle
            | Action::PromptTabTitle
            | Action::PromptWindowTitle
            | Action::ToggleSplitZoom
            | Action::ToggleReadonly
            | Action::EqualizeSplits
            | Action::ResetWindowSize
            | Action::ShowGtkInspector
            | Action::ShowOnScreenKeyboard
            | Action::ReloadConfig
            | Action::CloseSurface
            | Action::CloseWindow
            | Action::CloseAllWindows
            | Action::ToggleMaximize
            | Action::ToggleFullscreen
            | Action::ToggleWindowDecorations
            | Action::ToggleWindowFloatOnTop
            | Action::ToggleSecureInput
            | Action::ToggleMouseReporting
            | Action::ToggleCommandPalette
            | Action::ToggleQuickTerminal
            | Action::ToggleVisibility
            | Action::ToggleBackgroundOpacity
            | Action::CheckForUpdates
            | Action::Undo
            | Action::Redo
            | Action::EndKeySequence
            | Action::DeactivateKeyTable
            | Action::DeactivateAllKeyTables
            | Action::Quit => {}

            // Floats are hashed by their bits. This is totally not
            // portable and there are edge cases such as NaNs and
            // signed zeros but these are not cases we expect for
            // our bindings.
            Action::IncreaseFontSize(v)
            | Action::DecreaseFontSize(v)
            | Action::SetFontSize(v)
            | Action::ScrollPageFractional(v) => hasher.write_u32(v.to_bits()),

            // Everything else automatically handle.
            Action::Csi(v)
            | Action::Esc(v)
            | Action::Text(v)
            | Action::Search(v)
            | Action::SetSurfaceTitle(v)
            | Action::SetTabTitle(v)
            | Action::SetWindowTitle(v)
            | Action::ActivateKeyTable(v)
            | Action::ActivateKeyTableOnce(v) => hasher.write(v),
            Action::CursorKey(v) => {
                hasher.write(&v.normal);
                hasher.write(&v.application);
            }
            Action::CopyToClipboard(v) => hasher.write(v.tag_name().as_bytes()),
            Action::ScrollToRow(v) | Action::GotoTab(v) => hasher.write_usize(*v),
            Action::ScrollPageLines(v) | Action::JumpToPrompt(v) => hasher.write_i16(*v),
            Action::MoveTab(v) => hasher.write_isize(*v),
            Action::NavigateSearch(v) => hasher.write(v.tag_name().as_bytes()),
            Action::AdjustSelection(v) => hasher.write(v.tag_name().as_bytes()),
            Action::WriteScrollbackFile(v)
            | Action::WriteScreenFile(v)
            | Action::WriteSelectionFile(v) => {
                hasher.write(v.action.tag_name().as_bytes());
                hasher.write(v.emit.tag_name().as_bytes());
            }
            Action::NewSplit(v) => hasher.write(v.tag_name().as_bytes()),
            Action::GotoSplit(v) => hasher.write(v.tag_name().as_bytes()),
            Action::GotoWindow(v) => hasher.write(v.tag_name().as_bytes()),
            Action::ResizeSplit(v) => {
                hasher.write(v.0.tag_name().as_bytes());
                hasher.write_u16(v.1);
            }
            Action::Inspector(v) => hasher.write(v.tag_name().as_bytes()),
            Action::OpenConfig(v) => hasher.write(v.tag_name().as_bytes()),
            Action::CloseTab(v) => hasher.write(v.tag_name().as_bytes()),
            Action::Crash(v) => hasher.write(v.tag_name().as_bytes()),
        }
    }

    /// Compares two actions for equality.
    pub fn equal(&self, other: &Action) -> bool {
        // if (std.meta.activeTag(self) != std.meta.activeTag(other)) return false;
        // return switch (self) { inline else => |field_self, tag| { ...
        //     return deepEqual(@TypeOf(field_self), field_self, field_other); } };
        //
        // PORT NOTE: the reference compares active tags then `deepEqual`s the
        // payloads. Rust's structural match reproduces this: same-variant arms
        // `deep_equal` their payloads (byte slices / floats / ints / enums /
        // structs), and any cross-variant pair is unequal.
        match (self, other) {
            (Action::Csi(a), Action::Csi(b))
            | (Action::Esc(a), Action::Esc(b))
            | (Action::Text(a), Action::Text(b))
            | (Action::Search(a), Action::Search(b))
            | (Action::SetSurfaceTitle(a), Action::SetSurfaceTitle(b))
            | (Action::SetTabTitle(a), Action::SetTabTitle(b))
            | (Action::SetWindowTitle(a), Action::SetWindowTitle(b))
            | (Action::ActivateKeyTable(a), Action::ActivateKeyTable(b))
            | (Action::ActivateKeyTableOnce(a), Action::ActivateKeyTableOnce(b)) => {
                deep_equal::<[u8]>(a, b)
            }
            (Action::CursorKey(a), Action::CursorKey(b)) => a == b,
            (Action::CopyToClipboard(a), Action::CopyToClipboard(b)) => a == b,
            (Action::IncreaseFontSize(a), Action::IncreaseFontSize(b))
            | (Action::DecreaseFontSize(a), Action::DecreaseFontSize(b))
            | (Action::SetFontSize(a), Action::SetFontSize(b))
            | (Action::ScrollPageFractional(a), Action::ScrollPageFractional(b)) => {
                deep_equal(a, b)
            }
            (Action::ScrollToRow(a), Action::ScrollToRow(b))
            | (Action::GotoTab(a), Action::GotoTab(b)) => deep_equal(a, b),
            (Action::ScrollPageLines(a), Action::ScrollPageLines(b))
            | (Action::JumpToPrompt(a), Action::JumpToPrompt(b)) => deep_equal(a, b),
            (Action::MoveTab(a), Action::MoveTab(b)) => deep_equal(a, b),
            (Action::NavigateSearch(a), Action::NavigateSearch(b)) => a == b,
            (Action::AdjustSelection(a), Action::AdjustSelection(b)) => a == b,
            (Action::WriteScrollbackFile(a), Action::WriteScrollbackFile(b))
            | (Action::WriteScreenFile(a), Action::WriteScreenFile(b))
            | (Action::WriteSelectionFile(a), Action::WriteSelectionFile(b)) => a == b,
            (Action::NewSplit(a), Action::NewSplit(b)) => a == b,
            (Action::GotoSplit(a), Action::GotoSplit(b)) => a == b,
            (Action::GotoWindow(a), Action::GotoWindow(b)) => a == b,
            (Action::ResizeSplit(a), Action::ResizeSplit(b)) => a == b,
            (Action::Inspector(a), Action::Inspector(b)) => a == b,
            (Action::OpenConfig(a), Action::OpenConfig(b)) => a == b,
            (Action::CloseTab(a), Action::CloseTab(b)) => a == b,
            (Action::Crash(a), Action::Crash(b)) => a == b,
            // void variants: equal iff same tag.
            _ => {
                // same-tag void variants compare equal; differing tags unequal.
                std::mem::discriminant(self) == std::mem::discriminant(other)
            }
        }
    }
}

// PORT NOTE: `const bindingSetEqual = equal;` — alias used by the `Set` hash-map
// `Context(Action)`. Exposed via `Action::equal`.

// PORT NOTE: `Action.format` writes through the cfg-fmt `Writer`. The `Action`
// also participates in the config formatter as the leaf value of a keybind entry
// (see `Set::Value::format_entries`); it implements `FormatEntry` by delegating
// to its own `format`.
