//! rustty-input input/Binding
//!
//! PORTS: ghostty src/input/Binding.zig
//!
//! PORT STATUS: done
//!
//! A binding maps some input trigger to an action. When the trigger
//! occurs, the action is performed.
//
// PORT NOTE: the reference imports
//   const std = @import("std");
//   const Allocator = std.mem.Allocator;
//   const assert = @import("../quirks.zig").inlineAssert;
//   const build_config = @import("../build_config.zig");
//   const uucode = @import("uucode");
//   const EntryFormatter = @import("../config/formatter.zig").EntryFormatter;
//   const deepEqual = @import("../datastruct/comparison.zig").deepEqual;
//   const key = @import("key.zig");
//   const key_mods = @import("key_mods.zig");
//   const KeyEvent = key.KeyEvent;
// `std`/`Allocator` map to the Rust system allocator (Set ports the allocator
// seam as glue — see `Set`). `assert` -> `debug_assert!`. `build_config` is only
// named by `Action.getGObjectType` (deferred; see there). `uucode` backs
// `Trigger.foldedCodepoint` (see there). `EntryFormatter`/`deepEqual`/`key`/
// `key_mods`/`KeyEvent` map to the ported crates below.

use rustty_cfg_fmt::{format_entry_string, EntryFormatter, FormatEntry, FormatError, Writer};
use rustty_datastruct::comparison::deep_equal;
use rustty_vt::input::key::{self, Key as KeyKey, KeyEvent};
use rustty_vt::input::key_mods::{self, Mods};
use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;

/// PORT NOTE: Zig `[]const u8` byte-slice -> owned `Vec<u8>` for stored action
/// strings (the action owns its text after `clone`). The `u21` codepoint type
/// mirrors the VT crate's `u21 = u32`.
#[allow(non_camel_case_types)]
pub type u21 = u32;

/// PORT NOTE: the reference declares the binding's fields at file scope (the
/// file IS the `Binding` struct via `const Binding = @This();`). Here they are
/// the fields of an explicit `Binding` struct.
#[derive(Clone, Debug, PartialEq)]
pub struct Binding {
    /// The trigger that needs to be performed to execute the action.
    pub trigger: Trigger,

    /// The action to take if this binding matches
    pub action: Action,

    /// Boolean flags that can be set per binding.
    pub flags: Flags,
}

/// PORT NOTE: the reference error set `error{ InvalidFormat, InvalidAction }`.
pub const ERROR: () = ();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidFormat,
    InvalidAction,
}

impl Binding {
    /// Returns true if lhs should be sorted before rhs
    pub fn less_than(lhs: &Binding, rhs: &Binding) -> bool {
        let lhs_count: usize = {
            let mut count: usize = 0;
            if lhs.trigger.mods.super_ {
                count += 1;
            }
            if lhs.trigger.mods.ctrl {
                count += 1;
            }
            if lhs.trigger.mods.shift {
                count += 1;
            }
            if lhs.trigger.mods.alt {
                count += 1;
            }
            count
        };
        let rhs_count: usize = {
            let mut count: usize = 0;
            if rhs.trigger.mods.super_ {
                count += 1;
            }
            if rhs.trigger.mods.ctrl {
                count += 1;
            }
            if rhs.trigger.mods.shift {
                count += 1;
            }
            if rhs.trigger.mods.alt {
                count += 1;
            }
            count
        };

        if lhs_count != rhs_count {
            return lhs_count > rhs_count;
        }

        if lhs.trigger.mods.int() != rhs.trigger.mods.int() {
            return lhs.trigger.mods.int() > rhs.trigger.mods.int();
        }

        let lhs_key: std::os::raw::c_int = match lhs.trigger.key {
            TriggerKey::Physical(p) => p as std::os::raw::c_int,
            TriggerKey::Unicode(u) => u as std::os::raw::c_int,
            // PORT NOTE: `.catch_all` has no payload; the reference switch is
            // exhaustive only over `physical`/`unicode` here (catch_all is a
            // separate union arm). It never reaches lessThan in practice, but to
            // keep the match total we treat it like its (absent) ordinal.
            TriggerKey::CatchAll => 0,
        };
        let rhs_key: std::os::raw::c_int = match rhs.trigger.key {
            TriggerKey::Physical(p) => p as std::os::raw::c_int,
            TriggerKey::Unicode(u) => u as std::os::raw::c_int,
            TriggerKey::CatchAll => 0,
        };

        lhs_key < rhs_key
    }
}

/// Flags the full binding-scoped flags that can be set per binding.
//
// PORT NOTE: the reference `packed struct(u8)`. The four bool fields occupy bits
// 0..=3 (consumed=0, all=1, global=2, performable=3) with a `u4` padding. `cval`
// reproduces that bit layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Flags {
    /// True if this binding should consume the input when the
    /// action is triggered.
    pub consumed: bool,

    /// True if this binding should be forwarded to all active surfaces
    /// in the application.
    pub all: bool,

    /// True if this binding is global. Global bindings should work system-wide
    /// and not just while Rustty is focused. This may not work on all platforms.
    /// See the keybind config documentation for more information.
    pub global: bool,

    /// True if this binding should only be triggered if the action can be
    /// performed. If the action can't be performed then the binding acts as
    /// if it doesn't exist.
    pub performable: bool,
}

impl Default for Flags {
    fn default() -> Self {
        Flags {
            consumed: true,
            all: false,
            global: false,
            performable: false,
        }
    }
}

/// C type
//
// PORT NOTE: `pub const C = u8;`
#[allow(non_camel_case_types)]
pub type FlagsC = u8;

impl Flags {
    /// Converts this to a C-compatible value.
    ///
    /// Sync with ghostty.h for enums.
    pub fn cval(self) -> FlagsC {
        // const Backing = @typeInfo(Flags).@"struct".backing_integer.?;
        // return @as(Backing, @bitCast(self));
        (self.consumed as FlagsC)
            | ((self.all as FlagsC) << 1)
            | ((self.global as FlagsC) << 2)
            | ((self.performable as FlagsC) << 3)
    }
}

// ===========================================================================
// Action and its associated value types.
// ===========================================================================

/// PORT NOTE: `Action.CrashThread`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrashThread {
    Main,
    Io,
    Render,
}

impl CrashThread {
    fn tag_name(self) -> &'static str {
        match self {
            CrashThread::Main => "main",
            CrashThread::Io => "io",
            CrashThread::Render => "render",
        }
    }
    fn string_to_enum(v: &[u8]) -> Option<CrashThread> {
        match v {
            b"main" => Some(CrashThread::Main),
            b"io" => Some(CrashThread::Io),
            b"render" => Some(CrashThread::Render),
            _ => None,
        }
    }
}

/// PORT NOTE: `Action.CursorKey`. Both fields are owned byte slices.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CursorKey {
    pub normal: Vec<u8>,
    pub application: Vec<u8>,
}

impl CursorKey {
    // pub fn clone(self, alloc) Allocator.Error!CursorKey
    //
    // PORT NOTE: the reference dupes both slices with the allocator. Rust's
    // `Clone` already deep-copies the owned `Vec<u8>`s, so the allocator seam is
    // dropped (no allocator threading in the Rust port).
    pub fn clone_in(&self) -> CursorKey {
        CursorKey {
            normal: self.normal.clone(),
            application: self.application.clone(),
        }
    }

    // pub fn format(self, writer) ... @panic("formatting not supported");
    pub fn format<W: Writer>(&self, _writer: &mut W) -> Result<(), W::Error> {
        panic!("formatting not supported");
    }
}

/// PORT NOTE: `Action.NavigateSearch`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigateSearch {
    Previous,
    Next,
}

impl NavigateSearch {
    fn tag_name(self) -> &'static str {
        match self {
            NavigateSearch::Previous => "previous",
            NavigateSearch::Next => "next",
        }
    }
    fn string_to_enum(v: &[u8]) -> Option<NavigateSearch> {
        match v {
            b"previous" => Some(NavigateSearch::Previous),
            b"next" => Some(NavigateSearch::Next),
            _ => None,
        }
    }
}

/// PORT NOTE: `Action.AdjustSelection`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdjustSelection {
    Left,
    Right,
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
    BeginningOfLine,
    EndOfLine,
}

impl AdjustSelection {
    fn tag_name(self) -> &'static str {
        match self {
            AdjustSelection::Left => "left",
            AdjustSelection::Right => "right",
            AdjustSelection::Up => "up",
            AdjustSelection::Down => "down",
            AdjustSelection::PageUp => "page_up",
            AdjustSelection::PageDown => "page_down",
            AdjustSelection::Home => "home",
            AdjustSelection::End => "end",
            AdjustSelection::BeginningOfLine => "beginning_of_line",
            AdjustSelection::EndOfLine => "end_of_line",
        }
    }
    fn string_to_enum(v: &[u8]) -> Option<AdjustSelection> {
        match v {
            b"left" => Some(AdjustSelection::Left),
            b"right" => Some(AdjustSelection::Right),
            b"up" => Some(AdjustSelection::Up),
            b"down" => Some(AdjustSelection::Down),
            b"page_up" => Some(AdjustSelection::PageUp),
            b"page_down" => Some(AdjustSelection::PageDown),
            b"home" => Some(AdjustSelection::Home),
            b"end" => Some(AdjustSelection::End),
            b"beginning_of_line" => Some(AdjustSelection::BeginningOfLine),
            b"end_of_line" => Some(AdjustSelection::EndOfLine),
            _ => None,
        }
    }
}

/// PORT NOTE: `Action.SplitDirection`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitDirection {
    Right,
    Down,
    Left,
    Up,
    Auto, // splits along the larger direction
}

impl SplitDirection {
    pub const DEFAULT: SplitDirection = SplitDirection::Auto;

    fn tag_name(self) -> &'static str {
        match self {
            SplitDirection::Right => "right",
            SplitDirection::Down => "down",
            SplitDirection::Left => "left",
            SplitDirection::Up => "up",
            SplitDirection::Auto => "auto",
        }
    }
    fn string_to_enum(v: &[u8]) -> Option<SplitDirection> {
        match v {
            b"right" => Some(SplitDirection::Right),
            b"down" => Some(SplitDirection::Down),
            b"left" => Some(SplitDirection::Left),
            b"up" => Some(SplitDirection::Up),
            b"auto" => Some(SplitDirection::Auto),
            _ => None,
        }
    }
}

/// PORT NOTE: `Action.SplitFocusDirection`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitFocusDirection {
    Previous,
    Next,
    Up,
    Left,
    Down,
    Right,
}

impl SplitFocusDirection {
    fn string_to_enum(v: &[u8]) -> Option<SplitFocusDirection> {
        match v {
            b"previous" => Some(SplitFocusDirection::Previous),
            b"next" => Some(SplitFocusDirection::Next),
            b"up" => Some(SplitFocusDirection::Up),
            b"left" => Some(SplitFocusDirection::Left),
            b"down" => Some(SplitFocusDirection::Down),
            b"right" => Some(SplitFocusDirection::Right),
            _ => None,
        }
    }

    pub fn parse(input: &[u8]) -> Result<SplitFocusDirection, Error> {
        match SplitFocusDirection::string_to_enum(input) {
            Some(v) => Ok(v),
            None => {
                // For backwards compatibility we map "top" and "bottom" onto the enum
                // values "up" and "down"
                if input == b"top" {
                    Ok(SplitFocusDirection::Up)
                } else if input == b"bottom" {
                    Ok(SplitFocusDirection::Down)
                } else {
                    Err(Error::InvalidFormat)
                }
            }
        }
    }

    fn tag_name(self) -> &'static str {
        match self {
            SplitFocusDirection::Previous => "previous",
            SplitFocusDirection::Next => "next",
            SplitFocusDirection::Up => "up",
            SplitFocusDirection::Left => "left",
            SplitFocusDirection::Down => "down",
            SplitFocusDirection::Right => "right",
        }
    }
}

/// PORT NOTE: `Action.SplitResizeDirection`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitResizeDirection {
    Up,
    Down,
    Left,
    Right,
}

impl SplitResizeDirection {
    fn tag_name(self) -> &'static str {
        match self {
            SplitResizeDirection::Up => "up",
            SplitResizeDirection::Down => "down",
            SplitResizeDirection::Left => "left",
            SplitResizeDirection::Right => "right",
        }
    }
    fn string_to_enum(v: &[u8]) -> Option<SplitResizeDirection> {
        match v {
            b"up" => Some(SplitResizeDirection::Up),
            b"down" => Some(SplitResizeDirection::Down),
            b"left" => Some(SplitResizeDirection::Left),
            b"right" => Some(SplitResizeDirection::Right),
            _ => None,
        }
    }
}

/// PORT NOTE: `Action.GotoWindow`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GotoWindow {
    Previous,
    Next,
}

impl GotoWindow {
    fn tag_name(self) -> &'static str {
        match self {
            GotoWindow::Previous => "previous",
            GotoWindow::Next => "next",
        }
    }
    fn string_to_enum(v: &[u8]) -> Option<GotoWindow> {
        match v {
            b"previous" => Some(GotoWindow::Previous),
            b"next" => Some(GotoWindow::Next),
            _ => None,
        }
    }
}

/// PORT NOTE: `Action.SplitResizeParameter` — `struct { SplitResizeDirection, u16 }`
/// (a tuple struct). The tuple's first field is parsed as an enum, the second as
/// an int, comma-separated (see `parse_parameter`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SplitResizeParameter(pub SplitResizeDirection, pub u16);

/// PORT NOTE: `Action.CopyToClipboard`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopyToClipboard {
    Plain,
    Vt,
    Html,

    /// This type will mix multiple distinct types with a set content-type
    /// such as text/html for html, so that the OS/application can choose
    /// what is best when pasting.
    Mixed,
}

impl CopyToClipboard {
    pub const DEFAULT: CopyToClipboard = CopyToClipboard::Mixed;

    fn tag_name(self) -> &'static str {
        match self {
            CopyToClipboard::Plain => "plain",
            CopyToClipboard::Vt => "vt",
            CopyToClipboard::Html => "html",
            CopyToClipboard::Mixed => "mixed",
        }
    }
    fn string_to_enum(v: &[u8]) -> Option<CopyToClipboard> {
        match v {
            b"plain" => Some(CopyToClipboard::Plain),
            b"vt" => Some(CopyToClipboard::Vt),
            b"html" => Some(CopyToClipboard::Html),
            b"mixed" => Some(CopyToClipboard::Mixed),
            _ => None,
        }
    }
}

/// PORT NOTE: `Action.WriteScreen`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WriteScreen {
    pub action: WriteScreenAction,
    pub emit: WriteScreenFormat,
}

/// PORT NOTE: `Action.WriteScreen.Action`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteScreenAction {
    Copy,
    Paste,
    Open,
}

impl WriteScreenAction {
    fn tag_name(self) -> &'static str {
        match self {
            WriteScreenAction::Copy => "copy",
            WriteScreenAction::Paste => "paste",
            WriteScreenAction::Open => "open",
        }
    }
    fn string_to_enum(v: &[u8]) -> Option<WriteScreenAction> {
        match v {
            b"copy" => Some(WriteScreenAction::Copy),
            b"paste" => Some(WriteScreenAction::Paste),
            b"open" => Some(WriteScreenAction::Open),
            _ => None,
        }
    }
}

/// PORT NOTE: `Action.WriteScreen.Format`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteScreenFormat {
    Plain,
    Vt,
    Html,
}

impl WriteScreenFormat {
    fn tag_name(self) -> &'static str {
        match self {
            WriteScreenFormat::Plain => "plain",
            WriteScreenFormat::Vt => "vt",
            WriteScreenFormat::Html => "html",
        }
    }
    fn string_to_enum(v: &[u8]) -> Option<WriteScreenFormat> {
        match v {
            b"plain" => Some(WriteScreenFormat::Plain),
            b"vt" => Some(WriteScreenFormat::Vt),
            b"html" => Some(WriteScreenFormat::Html),
            _ => None,
        }
    }
}

impl WriteScreen {
    pub const COPY: WriteScreen = WriteScreen {
        action: WriteScreenAction::Copy,
        emit: WriteScreenFormat::Plain,
    };
    pub const PASTE: WriteScreen = WriteScreen {
        action: WriteScreenAction::Paste,
        emit: WriteScreenFormat::Plain,
    };
    pub const OPEN: WriteScreen = WriteScreen {
        action: WriteScreenAction::Open,
        emit: WriteScreenFormat::Plain,
    };

    pub fn parse(param: &[u8]) -> Result<WriteScreen, Error> {
        // If we don't have a `,`, default to the plain format. This is
        // also very important for backwards compatibility before Ghostty
        // 1.3 which didn't support output formats.
        match param.iter().position(|&c| c == b',') {
            None => Ok(WriteScreen {
                action: parse_enum_write_action(param)?,
                emit: WriteScreenFormat::Plain,
            }),
            Some(idx) => Ok(WriteScreen {
                action: parse_enum_write_action(&param[0..idx])?,
                emit: parse_enum_write_format(&param[idx + 1..])?,
            }),
        }
    }

    // pub fn clone(self, alloc) Allocator.Error!WriteScreen { _ = alloc; return self; }
    pub fn clone_in(&self) -> WriteScreen {
        *self
    }

    pub fn format<W: Writer>(&self, writer: &mut W) -> Result<(), W::Error> {
        // try writer.print("{t},{t}", .{ self.action, self.emit });
        writer.write_all(self.action.tag_name().as_bytes())?;
        writer.write_all(b",")?;
        writer.write_all(self.emit.tag_name().as_bytes())
    }
}

// PORT NOTE: `WriteScreen.parse` calls `Binding.Action.parseEnum(WriteScreen.Action, ...)`
// / `parseEnum(WriteScreen.Format, ...)` — the generic comptime `parseEnum` is
// threaded here as a per-type free fn mirroring `std.meta.stringToEnum`.
fn parse_enum_write_action(value: &[u8]) -> Result<WriteScreenAction, Error> {
    WriteScreenAction::string_to_enum(value).ok_or(Error::InvalidFormat)
}
fn parse_enum_write_format(value: &[u8]) -> Result<WriteScreenFormat, Error> {
    WriteScreenFormat::string_to_enum(value).ok_or(Error::InvalidFormat)
}

// Extern because it is used in the embedded runtime ABI.
/// PORT NOTE: `Action.InspectorMode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InspectorMode {
    Toggle,
    Show,
    Hide,
}

impl InspectorMode {
    fn tag_name(self) -> &'static str {
        match self {
            InspectorMode::Toggle => "toggle",
            InspectorMode::Show => "show",
            InspectorMode::Hide => "hide",
        }
    }
    fn string_to_enum(v: &[u8]) -> Option<InspectorMode> {
        match v {
            b"toggle" => Some(InspectorMode::Toggle),
            b"show" => Some(InspectorMode::Show),
            b"hide" => Some(InspectorMode::Hide),
            _ => None,
        }
    }
}

/// PORT NOTE: `Action.CloseTabMode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseTabMode {
    This,
    Other,
    Right,
}

impl CloseTabMode {
    pub const DEFAULT: CloseTabMode = CloseTabMode::This;

    fn tag_name(self) -> &'static str {
        match self {
            CloseTabMode::This => "this",
            CloseTabMode::Other => "other",
            CloseTabMode::Right => "right",
        }
    }
    fn string_to_enum(v: &[u8]) -> Option<CloseTabMode> {
        match v {
            b"this" => Some(CloseTabMode::This),
            b"other" => Some(CloseTabMode::Other),
            b"right" => Some(CloseTabMode::Right),
            _ => None,
        }
    }
}

/// PORT NOTE: `Action.OpenConfig`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenConfig {
    /// Open the config in the OS default editor.
    OsOpen,

    /// Open the config in a new window using $EDITOR or $VISUAL
    NewWindow,
}

impl OpenConfig {
    // pub const default: OpenConfig = .os_open;
    pub const DEFAULT: OpenConfig = OpenConfig::OsOpen;

    fn tag_name(self) -> &'static str {
        match self {
            OpenConfig::OsOpen => "os_open",
            OpenConfig::NewWindow => "new_window",
        }
    }
    fn string_to_enum(v: &[u8]) -> Option<OpenConfig> {
        match v {
            b"os_open" => Some(OpenConfig::OsOpen),
            b"new_window" => Some(OpenConfig::NewWindow),
            _ => None,
        }
    }
}

/// The set of actions that a keybinding can take.
//
// PORT NOTE: the reference `Action` is a `union(enum)` — a tagged union. Each
// arm maps to a Rust enum variant carrying its payload (or none for `void`
// arms). Variant names are the snake_case tags as PascalCase Rust identifiers;
// the `tag_name`/`string_to_enum` helpers below map back to the reference tag
// strings used by `parse`/`format`.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// Ignore this key combination.
    ///
    /// Rustty will not process this combination nor forward it to the child
    /// process within the terminal, but it may still be processed by the OS or
    /// other applications.
    Ignore,

    /// Unbind a previously bound key binding.
    ///
    /// This cannot unbind bindings that were not bound by Rustty or the user
    /// (e.g. bindings set by the OS or some other application).
    Unbind,

    /// Send a CSI sequence.
    ///
    /// The value should be the CSI sequence without the CSI header (`ESC [` or
    /// `\x1b[`).
    ///
    /// For example, `csi:0m` can be sent to reset all styles of the current text.
    Csi(Vec<u8>),

    /// Send an `ESC` sequence.
    Esc(Vec<u8>),

    /// Send the specified text.
    ///
    /// Uses Zig string literal syntax. This is currently not validated.
    /// If the text is invalid (i.e. contains an invalid escape sequence),
    /// the error will currently only show up in logs.
    Text(Vec<u8>),

    /// Send data to the pty depending on whether cursor key mode is enabled
    /// (`application`) or disabled (`normal`).
    CursorKey(CursorKey),

    /// Reset the terminal.
    ///
    /// This can fix a lot of issues when a running program puts the terminal
    /// into a broken state, equivalent to running the `reset` command.
    ///
    /// If you do this while in a TUI program such as vim, this may break
    /// the program. If you do this while in a shell, you may have to press
    /// enter after to get a new prompt.
    Reset,

    /// Copy the selected text to the clipboard.
    ///
    /// Valid values:
    ///
    ///   - `plain`
    ///
    ///     Copy the selection as plain text only.
    ///
    ///   - `vt`
    ///
    ///     Copy the selection as plain text, preserving terminal escape
    ///     sequences (such as colors and styles).
    ///
    ///   - `html`
    ///
    ///     Copy the selection as HTML, preserving colors and styles as
    ///     HTML markup.
    ///
    ///   - `mixed` (default)
    ///
    ///     Place multiple representations on the clipboard at once
    ///     (e.g. plain text and HTML), each tagged with its content type
    ///     so the receiving OS or application can pick the most appropriate
    ///     representation when pasting.
    CopyToClipboard(CopyToClipboard),

    /// Paste the contents of the default clipboard.
    PasteFromClipboard,

    /// Paste the contents of the selection clipboard.
    PasteFromSelection,

    /// If there is a URL under the cursor, copy it to the default clipboard.
    CopyUrlToClipboard,

    /// Copy the terminal title to the clipboard. If the terminal title is not
    /// set or is empty this has no effect.
    CopyTitleToClipboard,

    /// Increase the font size by the specified amount in points (pt).
    ///
    /// For example, `increase_font_size:1.5` will increase the font size
    /// by 1.5 points.
    IncreaseFontSize(f32),

    /// Decrease the font size by the specified amount in points (pt).
    ///
    /// For example, `decrease_font_size:1.5` will decrease the font size
    /// by 1.5 points.
    DecreaseFontSize(f32),

    /// Reset the font size to the original configured size.
    ResetFontSize,

    /// Set the font size to the specified size in points (pt).
    ///
    /// For example, `set_font_size:14.5` will set the font size
    /// to 14.5 points.
    SetFontSize(f32),

    /// Start a search for the given text. If the text is empty, then
    /// the search is canceled. A canceled search will not disable any GUI
    /// elements showing search. For that, the explicit end_search binding
    /// should be used.
    ///
    /// If a previous search is active, it is replaced.
    Search(Vec<u8>),

    /// Start a search for the current text selection. If there is no
    /// selection, this does nothing. If a search is already active, this
    /// changes the search terms.
    SearchSelection,

    /// Navigate the search results. If there is no active search, this
    /// is not performed.
    ///
    /// Valid values: `previous`, `next`.
    NavigateSearch(NavigateSearch),

    /// Start a search if it isn't started already. This doesn't set any
    /// search terms, but opens the UI for searching.
    StartSearch,

    /// End the current search if any and hide any GUI elements.
    EndSearch,

    /// Clear the screen and all scrollback.
    ClearScreen,

    /// Select all text on the screen.
    SelectAll,

    /// Scroll to the top of the screen.
    ScrollToTop,

    /// Scroll to the bottom of the screen.
    ScrollToBottom,

    /// Scroll to the selected text.
    ScrollToSelection,

    /// Scroll to the given absolute row in the screen with 0 being
    /// the first row.
    ScrollToRow(usize),

    /// Scroll the screen up by one page.
    ScrollPageUp,

    /// Scroll the screen down by one page.
    ScrollPageDown,

    /// Scroll the screen by the specified fraction of a page.
    ///
    /// Positive values scroll downwards, and negative values scroll upwards.
    ///
    /// For example, `scroll_page_fractional:0.5` would scroll the screen
    /// downwards by half a page, while `scroll_page_fractional:-1.5` would
    /// scroll it upwards by one and a half pages.
    ScrollPageFractional(f32),

    /// Scroll the screen by the specified amount of lines.
    ///
    /// Positive values scroll downwards, and negative values scroll upwards.
    ///
    /// For example, `scroll_page_lines:3` would scroll the screen downwards
    /// by 3 lines, while `scroll_page_lines:-10` would scroll it upwards by 10
    /// lines.
    ScrollPageLines(i16),

    /// Adjust the current selection in the given direction or position,
    /// relative to the cursor.
    ///
    /// WARNING: This does not create a new selection, and does nothing when
    /// there currently isn't one.
    ///
    /// Valid arguments are:
    ///
    ///   - `left`, `right`
    ///
    ///     Adjust the selection one cell to the left or right respectively.
    ///
    ///   - `up`, `down`
    ///
    ///     Adjust the selection one line upwards or downwards respectively.
    ///
    ///   - `page_up`, `page_down`
    ///
    ///     Adjust the selection one page upwards or downwards respectively.
    ///
    ///   - `home`, `end`
    ///
    ///     Adjust the selection to the top-left or the bottom-right corner
    ///     of the screen respectively.
    ///
    ///   - `beginning_of_line`, `end_of_line`
    ///
    ///     Adjust the selection to the beginning or the end of the line
    ///     respectively.
    ///
    AdjustSelection(AdjustSelection),

    /// Jump the viewport forward or back by the given number of prompts.
    ///
    /// Requires shell integration.
    ///
    /// Positive values scroll downwards, and negative values scroll upwards.
    JumpToPrompt(i16),

    /// Write the entire scrollback into a temporary file with the specified
    /// action. The action determines what to do with the filepath.
    ///
    /// Valid actions are:
    ///
    ///   - `copy`
    ///
    ///     Copy the file path into the clipboard.
    ///
    ///   - `paste`
    ///
    ///     Paste the file path into the terminal.
    ///
    ///   - `open`
    ///
    ///     Open the file in the default OS editor for text files.
    ///
    ///     The default OS editor is determined by using `open` on macOS
    ///     and `xdg-open` on Linux.
    ///
    WriteScrollbackFile(WriteScreen),

    /// Write the contents of the screen into a temporary file with the
    /// specified action.
    ///
    /// See `write_scrollback_file` for possible actions.
    WriteScreenFile(WriteScreen),

    /// Write the currently selected text into a temporary file with the
    /// specified action.
    ///
    /// See `write_scrollback_file` for possible actions.
    ///
    /// Does nothing when no text is selected.
    WriteSelectionFile(WriteScreen),

    /// Open a new window.
    ///
    /// If the application isn't currently focused,
    /// this will bring it to the front.
    NewWindow,

    /// Open a new tab.
    NewTab,

    /// Go to the previous tab.
    PreviousTab,

    /// Go to the next tab.
    NextTab,

    /// Go to the last tab.
    LastTab,

    /// Go to the tab with the specific index, starting from 1.
    ///
    /// If the tab number is higher than the number of tabs,
    /// this will go to the last tab.
    GotoTab(usize),

    /// Moves a tab by a relative offset.
    ///
    /// Positive values move the tab forwards, and negative values move it
    /// backwards. If the new position is out of bounds, it is wrapped around
    /// cyclically within the tab list.
    ///
    /// For example, `move_tab:1` moves the tab one position forwards, and if
    /// it was already the last tab in the list, it wraps around and becomes
    /// the first tab in the list. Likewise, `move_tab:-1` moves the tab one
    /// position backwards, and if it was the first tab, then it will become
    /// the last tab.
    MoveTab(isize),

    /// Move a tab to a new window.
    MoveTabToNewWindow,

    /// Toggle the tab overview.
    ///
    /// This is only supported on Linux and when the system's libadwaita
    /// version is 1.4 or newer. The current libadwaita version can be
    /// found by running `rustty +version`.
    ToggleTabOverview,

    /// Change the title of the current focused surface via a pop-up prompt.
    PromptSurfaceTitle,

    /// Change the title of the current tab via a pop-up prompt. The
    /// title set via this prompt overrides any title set by the terminal
    /// and persists across focus changes within the tab.
    PromptTabTitle,

    /// Change the title of the current window via a pop-up prompt. The
    /// title set via this prompt overrides any title set by the terminal
    /// and persists across focus changes within the tab.
    ///
    /// Only implemented on Linux.
    PromptWindowTitle,

    /// Set the title for the current focused surface.
    ///
    /// If the title is empty, the surface title is reset to an empty title.
    SetSurfaceTitle(Vec<u8>),

    /// Set the title for the current focused tab.
    ///
    /// If the title is empty, the tab title override is cleared.
    SetTabTitle(Vec<u8>),

    /// Set the title for the current focused window.
    ///
    /// If the title is empty, the tab title override is cleared.
    ///
    /// Only implement on Linux.
    SetWindowTitle(Vec<u8>),

    /// Create a new split in the specified direction.
    ///
    /// Valid arguments:
    ///
    ///   - `right`, `down`, `left`, `up`
    ///
    ///     Creates a new split in the corresponding direction.
    ///
    ///   - `auto`
    ///
    ///     Creates a new split along the larger direction.
    ///     For example, if the parent split is currently wider than it is tall,
    ///     then a left-right split would be created, and vice versa.
    ///
    NewSplit(SplitDirection),

    /// Focus on a split either in the specified direction (`right`, `down`,
    /// `left` and `up`), or in the adjacent split in the order of creation
    /// (`previous` and `next`).
    GotoSplit(SplitFocusDirection),

    /// Focus on either the previous window or the next one ('previous', 'next')
    GotoWindow(GotoWindow),

    /// Zoom in or out of the current split.
    ///
    /// When a split is zoomed into, it will take up the entire space in
    /// the current tab, hiding other splits. The tab or tab bar would also
    /// reflect this by displaying an icon indicating the zoomed state.
    ToggleSplitZoom,

    /// Toggle read-only mode for the current surface.
    ///
    /// When a surface is in read-only mode:
    ///   - No input is sent to the PTY (mouse events, key encoding)
    ///   - Input can still be used at the terminal level to make selections,
    ///     copy/paste (keybinds), scroll, etc.
    ///   - Warn before quit is always enabled in this state even if an active
    ///     process is not running
    ToggleReadonly,

    /// Resize the current split in the specified direction and amount in
    /// pixels. The two arguments should be joined with a comma (`,`),
    /// like in `resize_split:up,10`.
    ResizeSplit(SplitResizeParameter),

    /// Equalize the size of all splits in the current window.
    EqualizeSplits,

    /// Reset the window to the default size. The "default size" is the
    /// size that a new window would be created with. This has no effect
    /// if the window is fullscreen.
    ///
    /// Only implemented on macOS.
    ResetWindowSize,

    /// Control the visibility of the terminal inspector.
    ///
    /// Valid arguments: `toggle`, `show`, `hide`.
    Inspector(InspectorMode),

    /// Show the GTK inspector.
    ///
    /// Has no effect on macOS.
    ShowGtkInspector,

    /// Show the on-screen keyboard if one is present.
    ///
    /// Only implemented on Linux (GTK). On GNOME, the "Screen Keyboard"
    /// accessibility feature must be turned on, which can be found under
    /// Settings > Accessibility > Typing. Other platforms are as of now
    /// untested.
    ShowOnScreenKeyboard,

    /// Open the configuration file in an editor.
    ///
    /// * `os_open`: Use the OS's default editor to edit the configuration file.
    ///   This is the default action. (Available since 1.4.0)
    /// * `new_window`: Launch the editor specified in `$EDITOR` or `$VISUAL` in
    ///   a new Rustty window to edit the configuration file. GTK only. (Available
    ///   since 1.4.0.)
    OpenConfig(OpenConfig),

    /// Reload the configuration.
    ///
    /// The exact meaning depends on the app runtime in use, but this usually
    /// involves re-reading the configuration file and applying any changes
    /// Note that not all changes can be applied at runtime.
    ReloadConfig,

    /// Close the current "surface", whether that is a window, tab, split, etc.
    ///
    /// This might trigger a close confirmation popup, depending on the value
    /// of the `confirm-close-surface` configuration setting.
    CloseSurface,

    /// Close the specified tabs and all splits therein.
    ///
    /// Valid values:
    ///
    ///   - `this` (default)
    ///
    ///     Close the current tab and all splits within it.
    ///
    ///   - `other`
    ///
    ///     Close every tab in the current window except the current tab.
    ///
    ///   - `right`
    ///
    ///     Close every tab to the right of the current tab.
    ///
    /// This might trigger a close confirmation popup, depending on the value
    /// of the `confirm-close-surface` configuration setting.
    CloseTab(CloseTabMode),

    /// Close the current window and all tabs and splits therein.
    ///
    /// This might trigger a close confirmation popup, depending on the value
    /// of the `confirm-close-surface` configuration setting.
    CloseWindow,

    /// Close all windows.
    ///
    /// WARNING: This action has been deprecated and has no effect on either
    /// Linux or macOS. Users are instead encouraged to use `all:close_window`
    /// instead.
    CloseAllWindows,

    /// Maximize or unmaximize the current window.
    ///
    /// On macOS, this zooms the window, which is the closest equivalent
    /// since macOS has no concept of a maximized window.
    ToggleMaximize,

    /// Fullscreen or unfullscreen the current window.
    ToggleFullscreen,

    /// Toggle window decorations (titlebar, buttons, etc.) for the current window.
    ///
    /// Only implemented on Linux.
    ToggleWindowDecorations,

    /// Toggle whether the terminal window should always float on top of other
    /// windows even when unfocused.
    ///
    /// Terminal windows always start as normal (not float-on-top) windows.
    ///
    /// Only implemented on macOS.
    ToggleWindowFloatOnTop,

    /// Toggle secure input mode.
    ///
    /// This is used to prevent apps from monitoring your keyboard input
    /// when entering passwords or other sensitive information.
    ///
    /// This applies to the entire application, not just the focused terminal.
    /// You must manually untoggle it or quit Rustty entirely to disable it.
    ///
    /// Only implemented on macOS, as this uses a built-in system API.
    ToggleSecureInput,

    /// Toggle mouse reporting on or off.
    ///
    /// When mouse reporting is disabled, mouse events will not be reported to
    /// terminal applications even if they request it. This allows you to always
    /// use the mouse for selection and other terminal UI interactions without
    /// applications capturing mouse input.
    ///
    /// This can also be controlled via the `mouse-reporting` configuration
    /// option.
    ToggleMouseReporting,

    /// Toggle the command palette.
    ///
    /// The command palette is a popup that lets you see what actions
    /// you can perform, their associated keybindings (if any), a search bar
    /// to filter the actions, and the ability to then execute the action.
    ///
    /// This requires libadwaita 1.5 or newer on Linux. The current libadwaita
    /// version can be found by running `rustty +version`.
    ToggleCommandPalette,

    /// Toggle the quick terminal.
    ///
    /// The quick terminal, also known as the "Quake-style" or drop-down
    /// terminal, is a terminal window that appears on demand from a keybinding,
    /// often sliding in from a screen edge such as the top. This is useful for
    /// quick access to a terminal without having to open a new window or tab.
    ///
    /// The terminal state is preserved between appearances, so showing the
    /// quick terminal after it was already hidden would display the same
    /// window instead of creating a new one.
    ///
    /// As quick terminals are often useful when other windows are currently
    /// focused, they are best used with *global* keybinds. For example, one
    /// can define the following key bind to toggle the quick terminal from
    /// anywhere within the system by pressing `` Cmd+` ``:
    ///
    /// ```ini
    /// keybind = global:cmd+backquote=toggle_quick_terminal
    /// ```
    ///
    /// The quick terminal has some limitations:
    ///
    ///   - Only one quick terminal instance can exist at a time.
    ///
    ///   - Unlike normal terminal windows, the quick terminal will not be
    ///     restored when the application is restarted on systems that support
    ///     window restoration like macOS.
    ///
    ///   - On Linux, the quick terminal is only supported on Wayland and not
    ///     X11, and only on Wayland compositors that support the `wlr-layer-shell-v1`
    ///     protocol. In practice, this means that only GNOME users would not be
    ///     able to use this feature.
    ///
    ///   - On Linux, slide-in animations are only supported on KDE, and when
    ///     the "Sliding Popups" KWin plugin is enabled.
    ///
    ///     If you do not have this plugin enabled, open System Settings > Apps
    ///     & Windows > Window Management > Desktop Effects, and enable the
    ///     plugin in the plugin list. Rustty would then need to be restarted
    ///     fully for this to take effect.
    ///
    ///   - Quick terminal tabs are only supported on Linux and not on macOS.
    ///     This is because tabs on macOS require a title bar.
    ///
    ///   - On macOS, a fullscreened quick terminal will always be in non-native
    ///     fullscreen mode. This is a requirement due to how the quick terminal
    ///     is rendered.
    ///
    /// See the various configurations for the quick terminal in the
    /// configuration file to customize its behavior.
    ToggleQuickTerminal,

    /// Show or hide all windows. If all windows become shown, we also ensure
    /// Rustty becomes focused. When hiding all windows, focus is yielded
    /// to the next application as determined by the OS.
    ///
    /// Note: When the focused surface is fullscreen, this method does nothing.
    ///
    /// Only implemented on macOS.
    ToggleVisibility,

    /// Toggle the window background opacity between transparent and opaque.
    ///
    /// This does nothing when `background-opacity` is set to 1 or above.
    ///
    /// When `background-opacity` is less than 1, this action will either make
    /// the window transparent or not depending on its current transparency state.
    ///
    /// Only implemented on macOS.
    ToggleBackgroundOpacity,

    /// Check for updates.
    ///
    /// Only implemented on macOS.
    CheckForUpdates,

    /// Undo the last undoable action for the focused surface or terminal,
    /// if possible. This can undo actions such as closing tabs or
    /// windows.
    ///
    /// Not every action in Rustty can be undone or redone. The list
    /// of actions support undo/redo is currently limited to:
    ///
    ///   - New window, close window
    ///   - New tab, close tab
    ///   - New split, close split
    ///
    /// All actions are only undoable/redoable for a limited time.
    /// For example, restoring a closed split can only be done for
    /// some number of seconds since the split was closed. The exact
    /// amount is configured with the `undo-timeout` configuration settings.
    ///
    /// The undo/redo actions being limited ensures that there is
    /// bounded memory usage over time, closed surfaces don't continue running
    /// in the background indefinitely, and the keybinds become available
    /// for terminal applications to use.
    ///
    /// Only implemented on macOS.
    Undo,

    /// Redo the last undoable action for the focused surface or terminal,
    /// if possible. See "undo" for more details on what can and cannot
    /// be undone or redone.
    Redo,

    /// End the currently active key sequence, if any, and flush the
    /// keys up to this point to the terminal, excluding the key that
    /// triggered this action.
    ///
    /// For example: `ctrl+w>escape=end_key_sequence` would encode
    /// `ctrl+w` to the terminal and exit the key sequence.
    ///
    /// Normally, an invalid sequence will reset the key sequence and
    /// flush all data including the invalid key. This action allows
    /// you to flush only the prior keys, which is useful when you want
    /// to bind something like a control key (`ctrl+w`) but not send
    /// additional inputs.
    EndKeySequence,

    /// Activate a named key table (see `keybind` configuration documentation).
    /// The named key table will remain active until `deactivate_key_table`
    /// is called. If you want a one-shot key table activation, use the
    /// `activate_key_table_once` action instead.
    ///
    /// If the named key table does not exist, this action has no effect
    /// and performable will report false.
    ///
    /// If the named key table is already the currently active key table,
    /// this action has no effect and performable will report false.
    ActivateKeyTable(Vec<u8>),

    /// Same as activate_key_table, but the key table will only be active
    /// until the first valid keybinding from that table is used (including
    /// any defined `catch_all` bindings).
    ///
    /// The "once" check is only done if this is the currently active
    /// key table. If another key table is activated later, then this
    /// table will remain active until it pops back out to being the
    /// active key table.
    ActivateKeyTableOnce(Vec<u8>),

    /// Deactivate the currently active key table, if any. The next most
    /// recently activated key table (if any) will become active again.
    /// If no key table is active, this action has no effect.
    DeactivateKeyTable,

    /// Deactivate all active key tables. If no active key table exists,
    /// this will report performable as false.
    DeactivateAllKeyTables,

    /// Quit Rustty.
    Quit,

    /// Crash Rustty in the desired thread for the focused surface.
    ///
    /// WARNING: This is a hard crash (panic) and data can be lost.
    ///
    /// The purpose of this action is to test crash handling. For some
    /// users, it may be useful to test crash reporting functionality in
    /// order to determine if it all works as expected.
    ///
    /// The value determines the crash location:
    ///
    ///   - `main`
    ///
    ///     Crash on the main (GUI) thread.
    ///
    ///   - `io`
    ///
    ///     Crash on the IO thread for the focused surface.
    ///
    ///   - `render`
    ///
    ///     Crash on the render thread for the focused surface.
    ///
    Crash(CrashThread),
}

// PORT NOTE: `Action.Key = @typeInfo(Action).@"union".tag_type.?` — the tag
// enum. In Rust the discriminant is implicit; helpers below expose the tag name
// string used by `parse`/`format`/scope.
//
// Make this a valid gobject if we're in a GTK environment.
//
// PORT NOTE: `Action.getGObjectType` — `switch (build_config.app_runtime) { .gtk
// => @import("gobject").ext.defineBoxed(...), .none => void }`. This makes the
// action a GObject boxed type ONLY in the GTK apprt. `gobject` is an apprt-layer
// binding (deferred — not part of the VT/input parity slice; see DEFERRALS), and
// for `.none` the reference value is `void`. Ported as a deferred apprt seam.

impl Action {
    /// PORT NOTE: `@tagName(self)` — the snake_case tag string of the active
    /// union arm.
    pub fn tag_name(&self) -> &'static str {
        match self {
            Action::Ignore => "ignore",
            Action::Unbind => "unbind",
            Action::Csi(_) => "csi",
            Action::Esc(_) => "esc",
            Action::Text(_) => "text",
            Action::CursorKey(_) => "cursor_key",
            Action::Reset => "reset",
            Action::CopyToClipboard(_) => "copy_to_clipboard",
            Action::PasteFromClipboard => "paste_from_clipboard",
            Action::PasteFromSelection => "paste_from_selection",
            Action::CopyUrlToClipboard => "copy_url_to_clipboard",
            Action::CopyTitleToClipboard => "copy_title_to_clipboard",
            Action::IncreaseFontSize(_) => "increase_font_size",
            Action::DecreaseFontSize(_) => "decrease_font_size",
            Action::ResetFontSize => "reset_font_size",
            Action::SetFontSize(_) => "set_font_size",
            Action::Search(_) => "search",
            Action::SearchSelection => "search_selection",
            Action::NavigateSearch(_) => "navigate_search",
            Action::StartSearch => "start_search",
            Action::EndSearch => "end_search",
            Action::ClearScreen => "clear_screen",
            Action::SelectAll => "select_all",
            Action::ScrollToTop => "scroll_to_top",
            Action::ScrollToBottom => "scroll_to_bottom",
            Action::ScrollToSelection => "scroll_to_selection",
            Action::ScrollToRow(_) => "scroll_to_row",
            Action::ScrollPageUp => "scroll_page_up",
            Action::ScrollPageDown => "scroll_page_down",
            Action::ScrollPageFractional(_) => "scroll_page_fractional",
            Action::ScrollPageLines(_) => "scroll_page_lines",
            Action::AdjustSelection(_) => "adjust_selection",
            Action::JumpToPrompt(_) => "jump_to_prompt",
            Action::WriteScrollbackFile(_) => "write_scrollback_file",
            Action::WriteScreenFile(_) => "write_screen_file",
            Action::WriteSelectionFile(_) => "write_selection_file",
            Action::NewWindow => "new_window",
            Action::NewTab => "new_tab",
            Action::PreviousTab => "previous_tab",
            Action::NextTab => "next_tab",
            Action::LastTab => "last_tab",
            Action::GotoTab(_) => "goto_tab",
            Action::MoveTab(_) => "move_tab",
            Action::MoveTabToNewWindow => "move_tab_to_new_window",
            Action::ToggleTabOverview => "toggle_tab_overview",
            Action::PromptSurfaceTitle => "prompt_surface_title",
            Action::PromptTabTitle => "prompt_tab_title",
            Action::PromptWindowTitle => "prompt_window_title",
            Action::SetSurfaceTitle(_) => "set_surface_title",
            Action::SetTabTitle(_) => "set_tab_title",
            Action::SetWindowTitle(_) => "set_window_title",
            Action::NewSplit(_) => "new_split",
            Action::GotoSplit(_) => "goto_split",
            Action::GotoWindow(_) => "goto_window",
            Action::ToggleSplitZoom => "toggle_split_zoom",
            Action::ToggleReadonly => "toggle_readonly",
            Action::ResizeSplit(_) => "resize_split",
            Action::EqualizeSplits => "equalize_splits",
            Action::ResetWindowSize => "reset_window_size",
            Action::Inspector(_) => "inspector",
            Action::ShowGtkInspector => "show_gtk_inspector",
            Action::ShowOnScreenKeyboard => "show_on_screen_keyboard",
            Action::OpenConfig(_) => "open_config",
            Action::ReloadConfig => "reload_config",
            Action::CloseSurface => "close_surface",
            Action::CloseTab(_) => "close_tab",
            Action::CloseWindow => "close_window",
            Action::CloseAllWindows => "close_all_windows",
            Action::ToggleMaximize => "toggle_maximize",
            Action::ToggleFullscreen => "toggle_fullscreen",
            Action::ToggleWindowDecorations => "toggle_window_decorations",
            Action::ToggleWindowFloatOnTop => "toggle_window_float_on_top",
            Action::ToggleSecureInput => "toggle_secure_input",
            Action::ToggleMouseReporting => "toggle_mouse_reporting",
            Action::ToggleCommandPalette => "toggle_command_palette",
            Action::ToggleQuickTerminal => "toggle_quick_terminal",
            Action::ToggleVisibility => "toggle_visibility",
            Action::ToggleBackgroundOpacity => "toggle_background_opacity",
            Action::CheckForUpdates => "check_for_updates",
            Action::Undo => "undo",
            Action::Redo => "redo",
            Action::EndKeySequence => "end_key_sequence",
            Action::ActivateKeyTable(_) => "activate_key_table",
            Action::ActivateKeyTableOnce(_) => "activate_key_table_once",
            Action::DeactivateKeyTable => "deactivate_key_table",
            Action::DeactivateAllKeyTables => "deactivate_all_key_tables",
            Action::Quit => "quit",
            Action::Crash(_) => "crash",
        }
    }

    /// PORT NOTE: `@typeInfo(Action).@"union".fields` — the union's fields in
    /// declaration order. Rust has no field reflection over an enum, so the
    /// snake_case tag names are listed here in the exact `Action` variant
    /// declaration order. Consumed by `command`/`helpgen_actions` which iterate
    /// over `Action.Key` / the union fields. This list is the single
    /// source-of-truth for that order and MUST match the `Action` variants and
    /// `tag_name` above.
    pub const FIELD_NAMES: &'static [&'static str] = &[
        "ignore",
        "unbind",
        "csi",
        "esc",
        "text",
        "cursor_key",
        "reset",
        "copy_to_clipboard",
        "paste_from_clipboard",
        "paste_from_selection",
        "copy_url_to_clipboard",
        "copy_title_to_clipboard",
        "increase_font_size",
        "decrease_font_size",
        "reset_font_size",
        "set_font_size",
        "search",
        "search_selection",
        "navigate_search",
        "start_search",
        "end_search",
        "clear_screen",
        "select_all",
        "scroll_to_top",
        "scroll_to_bottom",
        "scroll_to_selection",
        "scroll_to_row",
        "scroll_page_up",
        "scroll_page_down",
        "scroll_page_fractional",
        "scroll_page_lines",
        "adjust_selection",
        "jump_to_prompt",
        "write_scrollback_file",
        "write_screen_file",
        "write_selection_file",
        "new_window",
        "new_tab",
        "previous_tab",
        "next_tab",
        "last_tab",
        "goto_tab",
        "move_tab",
        "move_tab_to_new_window",
        "toggle_tab_overview",
        "prompt_surface_title",
        "prompt_tab_title",
        "prompt_window_title",
        "set_surface_title",
        "set_tab_title",
        "set_window_title",
        "new_split",
        "goto_split",
        "goto_window",
        "toggle_split_zoom",
        "toggle_readonly",
        "resize_split",
        "equalize_splits",
        "reset_window_size",
        "inspector",
        "show_gtk_inspector",
        "show_on_screen_keyboard",
        "open_config",
        "reload_config",
        "close_surface",
        "close_tab",
        "close_window",
        "close_all_windows",
        "toggle_maximize",
        "toggle_fullscreen",
        "toggle_window_decorations",
        "toggle_window_float_on_top",
        "toggle_secure_input",
        "toggle_mouse_reporting",
        "toggle_command_palette",
        "toggle_quick_terminal",
        "toggle_visibility",
        "toggle_background_opacity",
        "check_for_updates",
        "undo",
        "redo",
        "end_key_sequence",
        "activate_key_table",
        "activate_key_table_once",
        "deactivate_key_table",
        "deactivate_all_key_tables",
        "quit",
        "crash",
    ];
}

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
    fn hash_incremental(&self, hasher: &mut DefaultHasher) {
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

// ===========================================================================
// Trigger
// ===========================================================================

/// Trigger is the associated key state that can trigger an action.
/// This is an extern struct because this is also used in the C API.
///
/// This must be kept in sync with include/rustty.h ghostty_input_trigger_s
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Trigger {
    /// The key that has to be pressed for a binding to take action.
    pub key: TriggerKey,

    /// The key modifiers that must be active for this to match.
    pub mods: Mods,
}

impl Default for Trigger {
    fn default() -> Self {
        Trigger {
            // key: Trigger.Key = .{ .physical = .unidentified }
            key: TriggerKey::Physical(KeyKey::Unidentified),
            // mods: key.Mods = .{}
            mods: Mods::default(),
        }
    }
}

/// PORT NOTE: `Trigger.Key = union(C.Tag)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TriggerKey {
    /// key is the "physical" version. This is the same as mapped for
    /// standard US keyboard layouts. For non-US keyboard layouts, this
    /// is used to bind to a physical key location rather than a translated
    /// key.
    Physical(KeyKey),

    /// This is used for binding to keys that produce a certain unicode
    /// codepoint. This is useful for binding to keys that don't have a
    /// registered keycode with Rustty.
    Unicode(u21),

    /// A catch-all key that matches any key press that is otherwise
    /// unbound.
    CatchAll,
}

/// The extern struct used for triggers in the C API.
//
// PORT NOTE: `Trigger.C = extern struct { tag, key: C.Key (extern union), mods }`
// where the reference's `mods` is `key.Mods = packed struct(u16)`. The embedding
// C ABI (rustty-embed.h) projects `mods` as `rustty_input_mods_e`, an `int`-wide
// C enum (4 bytes on the Apple LP64 ABI). We therefore carry `mods` as the packed
// integer widened to that 4-byte slot rather than embedding the ergonomic `Mods`
// struct (10 bytes, NOT `#[repr(C)]`): embedding it made `size_of::<TriggerC>()`
// 20, so `rustty_config_trigger` returned a >16-byte struct via the AAPCS64 x8
// sret path while every C/Swift caller (12-byte header struct) expected a
// register return — a hard ABI mismatch that crashed on the first call. Using the
// 4-byte packed slot restores byte-exact parity with the header (and hence the
// reference's proven 12-byte, register-returned ABI). The `int()` bit layout is
// identical to the reference's `key.Mods` backing integer. See the
// `size_of::<TriggerC>() == 12` freeze below and
// `.port/phase-2/MACOS_UI_PARITY_GAPS.md`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct TriggerC {
    pub tag: TriggerCTag,
    pub key: TriggerCKey,
    /// Packed `Mods` (`Mods::int()`) widened to the header's `int`-wide
    /// `rustty_input_mods_e` slot. Reconstruct with `Mods::from_int(mods as u16)`.
    pub mods: u32,
}

// ABI freeze: the header (`rustty_input_trigger_s`) is `{ tag(4), key(4),
// mods(4) } = 12` bytes, register-returned. Any drift back above 16 bytes
// re-introduces the sret crash. (Companion runtime check: rustty-input
// `abi_trigger_c_layout`; embedding-header gate: check_embed_h_abi.sh.)
// @embed-abi rustty_input_trigger_s 12
const _: () = assert!(core::mem::size_of::<TriggerC>() == 12);
const _: () = assert!(core::mem::align_of::<TriggerC>() == 4);
// @embed-abi-field rustty_input_trigger_s tag 0
const _: () = assert!(core::mem::offset_of!(TriggerC, tag) == 0);
// @embed-abi-field rustty_input_trigger_s key 4
const _: () = assert!(core::mem::offset_of!(TriggerC, key) == 4);
// @embed-abi-field rustty_input_trigger_s mods 8
const _: () = assert!(core::mem::offset_of!(TriggerC, mods) == 8);

/// PORT NOTE: `Trigger.C.Tag = enum(c_int) { physical, unicode, catch_all }`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TriggerCTag {
    Physical,
    Unicode,
    CatchAll,
}

/// PORT NOTE: `Trigger.C.Key = extern union { physical: key.Key, unicode: u32 }`.
#[repr(C)]
#[derive(Clone, Copy)]
pub union TriggerCKey {
    pub physical: KeyKey,
    pub unicode: u32,
}

impl Trigger {
    /// Parse a single trigger. The input is expected to be ONLY the trigger
    /// (i.e. in the sequence `a=ignore` input is only `a`). The trigger may
    /// not be part of a sequence (i.e. `a>b`). This parses exactly a single
    /// trigger.
    pub fn parse(input: &[u8]) -> Result<Trigger, Error> {
        if input.is_empty() {
            return Err(Error::InvalidFormat);
        }
        let mut result: Trigger = Trigger::default();
        let mut rem: &[u8] = input;
        'loop_: while !rem.is_empty() {
            let idx = rem.iter().position(|&c| c == b'+').unwrap_or(rem.len());
            let part = &rem[0..idx];
            rem = if idx >= rem.len() {
                b""
            } else {
                &rem[idx + 1..]
            };

            // Check if its a modifier
            // const modsInfo = @typeInfo(key.Mods).@"struct";
            // inline for (modsInfo.fields) |field| { if (field.type == bool) {...} }
            for field in Mods::BOOL_FIELD_NAMES {
                if part == field.as_bytes() {
                    // Repeat not allowed
                    if result.mods.get_bool_field(field).unwrap() {
                        return Err(Error::InvalidFormat);
                    }
                    result.mods.set_bool_field(field, true);
                    continue 'loop_;
                }
            }

            // Alias modifiers
            // inline for (key_mods.alias) |pair| { ... }
            for pair in key_mods::ALIAS {
                if part == pair.0 {
                    // Repeat not allowed
                    let field = pair.1.tag_name();
                    if result.mods.get_bool_field(field).unwrap() {
                        return Err(Error::InvalidFormat);
                    }
                    result.mods.set_bool_field(field, true);
                    continue 'loop_;
                }
            }

            // Anything after this point is a key and we only support
            // single keys.
            if !result.is_key_unset() {
                return Err(Error::InvalidFormat);
            }

            // If the part is empty it means that it is actually
            // a literal `+`, which we treat as a Unicode character.
            if part.is_empty() {
                result.key = TriggerKey::Unicode(b'+' as u21);
                continue 'loop_;
            }

            // Check if its a key
            // const keysInfo = @typeInfo(key.Key).@"enum";
            // inline for (keysInfo.fields) |field| { if name != "unidentified" ... }
            for (k, name) in key::Key::tag_names() {
                if *name != "unidentified" && part == name.as_bytes() {
                    result.key = TriggerKey::Physical(*k);
                    continue 'loop_;
                }
            }

            // If we're still unset and we have exactly one unicode
            // character then we can use that as a key.
            if result.is_key_unset() {
                'unicode: {
                    // Invalid UTF8 drops to invalid format
                    let s = match std::str::from_utf8(part) {
                        Ok(s) => s,
                        Err(_) => break 'unicode,
                    };
                    let mut it = s.chars();
                    // No codepoints or multiple codepoints drops to invalid format
                    let cp = match it.next() {
                        Some(c) => c,
                        None => break 'unicode,
                    };
                    if it.next().is_some() {
                        break 'unicode;
                    }
                    result.key = TriggerKey::Unicode(cp as u21);
                    continue 'loop_;
                }
            }

            // Look for a matching w3c name next.
            if let Some(w3c_key) = key::Key::from_w3c(part) {
                result.key = TriggerKey::Physical(w3c_key);
                continue 'loop_;
            }

            // Check for catch_all. We do this near the end since its unlikely
            // in most cases that we're setting a catch-all key.
            if part == b"catch_all" {
                result.key = TriggerKey::CatchAll;
                continue 'loop_;
            }

            // If we're still unset then we look for backwards compatible
            // keys with Rustty 1.1.x. We do this last so its least likely
            // to impact performance for modern users.
            if let Some(old_key) = backwards_compatible_keys_get(part) {
                result.key = old_key;
                continue 'loop_;
            }

            // We didn't recognize this value
            return Err(Error::InvalidFormat);
        }

        Ok(result)
    }

    /// Returns true if this trigger has no key set.
    pub fn is_key_unset(self) -> bool {
        match self.key {
            TriggerKey::Physical(v) => v == KeyKey::Unidentified,
            TriggerKey::Unicode(_) | TriggerKey::CatchAll => false,
        }
    }

    /// Returns a hash code that can be used to uniquely identify this trigger.
    pub fn hash(self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.hash_incremental(&mut hasher);
        hasher.finish()
    }

    /// Hash the trigger into the given hasher.
    fn hash_incremental(self, hasher: &mut DefaultHasher) {
        // std.hash.autoHash(hasher, std.meta.activeTag(self.key));
        match self.key {
            TriggerKey::Physical(v) => {
                hasher.write_u8(0);
                hasher.write_i32(v as i32);
            }
            TriggerKey::Unicode(cp) => {
                hasher.write_u8(1);
                // std.hash.autoHash(hasher, foldedCodepoint(cp));
                for c in folded_codepoint(cp) {
                    hasher.write_u32(c);
                }
            }
            TriggerKey::CatchAll => {
                hasher.write_u8(2);
            }
        }
        // std.hash.autoHash(hasher, self.mods.binding());
        hasher.write_u16(self.mods.binding().int());
    }

    /// Returns true if two triggers are equal.
    pub fn equal(self, other: Trigger) -> bool {
        if self.mods != other.mods {
            return false;
        }
        // const self_tag = std.meta.activeTag(self.key);
        // const other_tag = std.meta.activeTag(other.key);
        // if (self_tag != other_tag) return false;
        match (self.key, other.key) {
            (TriggerKey::Physical(v), TriggerKey::Physical(o)) => v == o,
            (TriggerKey::Unicode(v), TriggerKey::Unicode(o)) => v == o,
            (TriggerKey::CatchAll, TriggerKey::CatchAll) => true,
            _ => false,
        }
    }

    /// Returns true if two triggers are equal using folded codepoints.
    pub fn folded_equal(self, other: Trigger) -> bool {
        if self.mods != other.mods {
            return false;
        }
        match (self.key, other.key) {
            (TriggerKey::Physical(v), TriggerKey::Physical(o)) => v == o,
            (TriggerKey::Unicode(v), TriggerKey::Unicode(o)) => {
                // deepEqual([3]u21, foldedCodepoint(v), foldedCodepoint(other.key.unicode))
                deep_equal::<[u21; 3]>(&folded_codepoint(v), &folded_codepoint(o))
            }
            (TriggerKey::CatchAll, TriggerKey::CatchAll) => true,
            _ => false,
        }
    }

    // PORT NOTE: `const bindingSetEqual = foldedEqual;` — the `Set` hash-map
    // `Context(Trigger)` uses folded equality. Exposed via `folded_equal`.

    /// Convert the trigger to a C API compatible trigger.
    pub fn cval(self) -> TriggerC {
        TriggerC {
            tag: match self.key {
                TriggerKey::Physical(_) => TriggerCTag::Physical,
                TriggerKey::Unicode(_) => TriggerCTag::Unicode,
                TriggerKey::CatchAll => TriggerCTag::CatchAll,
            },
            key: match self.key {
                TriggerKey::Physical(v) => TriggerCKey { physical: v },
                TriggerKey::Unicode(v) => TriggerCKey { unicode: v },
                // catch_all has no associated value so its an error
                // for a C consumer to look at it.
                //
                // PORT NOTE: the reference leaves this `undefined`; we use the
                // zero `physical` payload as the (never-read) placeholder.
                TriggerKey::CatchAll => TriggerCKey {
                    physical: KeyKey::Unidentified,
                },
            },
            // Project the ergonomic `Mods` struct to its packed integer, widened
            // to the header's `int`-wide `rustty_input_mods_e` slot (see TriggerC).
            mods: u32::from(self.mods.int()),
        }
    }

    /// Format implementation for fmt package.
    pub fn format<W: Writer>(&self, writer: &mut W) -> Result<(), W::Error> {
        // Modifiers first
        if self.mods.super_ {
            writer.write_all(b"super+")?;
        }
        if self.mods.ctrl {
            writer.write_all(b"ctrl+")?;
        }
        if self.mods.alt {
            writer.write_all(b"alt+")?;
        }
        if self.mods.shift {
            writer.write_all(b"shift+")?;
        }

        // Key
        match self.key {
            TriggerKey::Physical(k) => writer.write_all(k.tag_name().as_bytes()),
            TriggerKey::Unicode(c) => {
                // try writer.print("{u}", .{c}); — the codepoint as UTF-8.
                let ch = char::from_u32(c).unwrap_or('\u{fffd}');
                let mut buf = [0u8; 4];
                writer.write_all(ch.encode_utf8(&mut buf).as_bytes())
            }
            TriggerKey::CatchAll => writer.write_all(b"catch_all"),
        }
    }
}

/// The codepoint we use for comparisons. Case folding can result
/// in more codepoints so we need to use a 3 element array.
//
// PORT NOTE: the reference's `foldedCodepoint` ASCII fast path uses
// `uucode.ascii.isAlphabetic`/`toLower`; the Unicode slow path uses
// `uucode.get(.case_folding_full, cp)`, ported as
// `rustty_unicode::case_folding::case_folding_full`. CORRECTION (S65-F1,
// 2026-09-23): the slow path used to return `[cp, 0, 0]` for every non-ASCII
// code point, under a note that the table was "NOT yet ported". As in the
// reference, a folding of up to three code points fills the array; the
// reference's comment about returning the code point as-is does not describe
// its own code, which copies the whole slice.
fn folded_codepoint(cp: u21) -> [u21; 3] {
    // ASCII fast path
    if ascii_is_alphabetic(cp) {
        return [ascii_to_lower(cp), 0, 0];
    }

    // Unicode slow path. Case folding can result in more codepoints.
    // If more codepoints are produced then we return the codepoint
    // as-is which isn't correct but until we have a failing test
    // then I don't want to handle this.
    let (slice, len) = rustty_unicode::case_folding::case_folding_full(cp);
    let mut array: [u21; 3] = [0; 3];
    array[..len].copy_from_slice(&slice[..len]);
    array
}

// PORT NOTE: `uucode.ascii.isAlphabetic` (uucode ascii.zig): `'A'...'Z',
// 'a'...'z' => true`.
fn ascii_is_alphabetic(c: u21) -> bool {
    matches!(c, 0x41..=0x5A | 0x61..=0x7A)
}

// PORT NOTE: `uucode.ascii.toLower` (uucode ascii.zig):
// `c | (@intFromBool(isUpper(c)) << 5)`.
fn ascii_to_lower(c: u21) -> u21 {
    let is_upper = (0x41..=0x5A).contains(&c);
    c | ((is_upper as u21) << 5)
}

/// The values that are backwards compatible with Rustty 1.1.x.
/// Rustty 1.2+ doesn't support these anymore since we moved to
/// W3C key codes.
//
// PORT NOTE: the reference `backwards_compatible_keys` is a
// `std.StaticStringMap(Key)`. Modeled as a static name->TriggerKey table; the
// lookup `.get(part)` is `backwards_compatible_keys_get`. The `keys()`/`values()`
// iteration used by the test is exposed via `BACKWARDS_COMPATIBLE_KEYS`.
pub const BACKWARDS_COMPATIBLE_KEYS: &[(&[u8], TriggerKey)] = &[
    (b"zero", TriggerKey::Unicode(b'0' as u21)),
    (b"one", TriggerKey::Unicode(b'1' as u21)),
    (b"two", TriggerKey::Unicode(b'2' as u21)),
    (b"three", TriggerKey::Unicode(b'3' as u21)),
    (b"four", TriggerKey::Unicode(b'4' as u21)),
    (b"five", TriggerKey::Unicode(b'5' as u21)),
    (b"six", TriggerKey::Unicode(b'6' as u21)),
    (b"seven", TriggerKey::Unicode(b'7' as u21)),
    (b"eight", TriggerKey::Unicode(b'8' as u21)),
    (b"nine", TriggerKey::Unicode(b'9' as u21)),
    (b"plus", TriggerKey::Unicode(b'+' as u21)),
    (b"apostrophe", TriggerKey::Unicode(b'\'' as u21)),
    (b"grave_accent", TriggerKey::Physical(KeyKey::Backquote)),
    (b"left_bracket", TriggerKey::Physical(KeyKey::BracketLeft)),
    (b"right_bracket", TriggerKey::Physical(KeyKey::BracketRight)),
    (b"up", TriggerKey::Physical(KeyKey::ArrowUp)),
    (b"down", TriggerKey::Physical(KeyKey::ArrowDown)),
    (b"left", TriggerKey::Physical(KeyKey::ArrowLeft)),
    (b"right", TriggerKey::Physical(KeyKey::ArrowRight)),
    (b"kp_0", TriggerKey::Physical(KeyKey::Numpad0)),
    (b"kp_1", TriggerKey::Physical(KeyKey::Numpad1)),
    (b"kp_2", TriggerKey::Physical(KeyKey::Numpad2)),
    (b"kp_3", TriggerKey::Physical(KeyKey::Numpad3)),
    (b"kp_4", TriggerKey::Physical(KeyKey::Numpad4)),
    (b"kp_5", TriggerKey::Physical(KeyKey::Numpad5)),
    (b"kp_6", TriggerKey::Physical(KeyKey::Numpad6)),
    (b"kp_7", TriggerKey::Physical(KeyKey::Numpad7)),
    (b"kp_8", TriggerKey::Physical(KeyKey::Numpad8)),
    (b"kp_9", TriggerKey::Physical(KeyKey::Numpad9)),
    (b"kp_add", TriggerKey::Physical(KeyKey::NumpadAdd)),
    (b"kp_subtract", TriggerKey::Physical(KeyKey::NumpadSubtract)),
    (b"kp_multiply", TriggerKey::Physical(KeyKey::NumpadMultiply)),
    (b"kp_divide", TriggerKey::Physical(KeyKey::NumpadDivide)),
    (b"kp_decimal", TriggerKey::Physical(KeyKey::NumpadDecimal)),
    (b"kp_enter", TriggerKey::Physical(KeyKey::NumpadEnter)),
    (b"kp_equal", TriggerKey::Physical(KeyKey::NumpadEqual)),
    (
        b"kp_separator",
        TriggerKey::Physical(KeyKey::NumpadSeparator),
    ),
    (b"kp_left", TriggerKey::Physical(KeyKey::NumpadLeft)),
    (b"kp_right", TriggerKey::Physical(KeyKey::NumpadRight)),
    (b"kp_up", TriggerKey::Physical(KeyKey::NumpadUp)),
    (b"kp_down", TriggerKey::Physical(KeyKey::NumpadDown)),
    (b"kp_page_up", TriggerKey::Physical(KeyKey::NumpadPageUp)),
    (
        b"kp_page_down",
        TriggerKey::Physical(KeyKey::NumpadPageDown),
    ),
    (b"kp_home", TriggerKey::Physical(KeyKey::NumpadHome)),
    (b"kp_end", TriggerKey::Physical(KeyKey::NumpadEnd)),
    (b"kp_insert", TriggerKey::Physical(KeyKey::NumpadInsert)),
    (b"kp_delete", TriggerKey::Physical(KeyKey::NumpadDelete)),
    (b"kp_begin", TriggerKey::Physical(KeyKey::NumpadBegin)),
    (b"left_shift", TriggerKey::Physical(KeyKey::ShiftLeft)),
    (b"right_shift", TriggerKey::Physical(KeyKey::ShiftRight)),
    (b"left_control", TriggerKey::Physical(KeyKey::ControlLeft)),
    (b"right_control", TriggerKey::Physical(KeyKey::ControlRight)),
    (b"left_alt", TriggerKey::Physical(KeyKey::AltLeft)),
    (b"right_alt", TriggerKey::Physical(KeyKey::AltRight)),
    (b"left_super", TriggerKey::Physical(KeyKey::MetaLeft)),
    (b"right_super", TriggerKey::Physical(KeyKey::MetaRight)),
    // Physical variants. This is a blunt approach to this but its
    // glue for backwards compatibility so I'm not too worried about
    // making this super nice.
    (b"physical:zero", TriggerKey::Physical(KeyKey::Digit0)),
    (b"physical:one", TriggerKey::Physical(KeyKey::Digit1)),
    (b"physical:two", TriggerKey::Physical(KeyKey::Digit2)),
    (b"physical:three", TriggerKey::Physical(KeyKey::Digit3)),
    (b"physical:four", TriggerKey::Physical(KeyKey::Digit4)),
    (b"physical:five", TriggerKey::Physical(KeyKey::Digit5)),
    (b"physical:six", TriggerKey::Physical(KeyKey::Digit6)),
    (b"physical:seven", TriggerKey::Physical(KeyKey::Digit7)),
    (b"physical:eight", TriggerKey::Physical(KeyKey::Digit8)),
    (b"physical:nine", TriggerKey::Physical(KeyKey::Digit9)),
    (b"physical:apostrophe", TriggerKey::Physical(KeyKey::Quote)),
    (
        b"physical:grave_accent",
        TriggerKey::Physical(KeyKey::Backquote),
    ),
    (
        b"physical:left_bracket",
        TriggerKey::Physical(KeyKey::BracketLeft),
    ),
    (
        b"physical:right_bracket",
        TriggerKey::Physical(KeyKey::BracketRight),
    ),
    (b"physical:up", TriggerKey::Physical(KeyKey::ArrowUp)),
    (b"physical:down", TriggerKey::Physical(KeyKey::ArrowDown)),
    (b"physical:left", TriggerKey::Physical(KeyKey::ArrowLeft)),
    (b"physical:right", TriggerKey::Physical(KeyKey::ArrowRight)),
    (b"physical:kp_0", TriggerKey::Physical(KeyKey::Numpad0)),
    (b"physical:kp_1", TriggerKey::Physical(KeyKey::Numpad1)),
    (b"physical:kp_2", TriggerKey::Physical(KeyKey::Numpad2)),
    (b"physical:kp_3", TriggerKey::Physical(KeyKey::Numpad3)),
    (b"physical:kp_4", TriggerKey::Physical(KeyKey::Numpad4)),
    (b"physical:kp_5", TriggerKey::Physical(KeyKey::Numpad5)),
    (b"physical:kp_6", TriggerKey::Physical(KeyKey::Numpad6)),
    (b"physical:kp_7", TriggerKey::Physical(KeyKey::Numpad7)),
    (b"physical:kp_8", TriggerKey::Physical(KeyKey::Numpad8)),
    (b"physical:kp_9", TriggerKey::Physical(KeyKey::Numpad9)),
    (b"physical:kp_add", TriggerKey::Physical(KeyKey::NumpadAdd)),
    (
        b"physical:kp_subtract",
        TriggerKey::Physical(KeyKey::NumpadSubtract),
    ),
    (
        b"physical:kp_multiply",
        TriggerKey::Physical(KeyKey::NumpadMultiply),
    ),
    (
        b"physical:kp_divide",
        TriggerKey::Physical(KeyKey::NumpadDivide),
    ),
    (
        b"physical:kp_decimal",
        TriggerKey::Physical(KeyKey::NumpadDecimal),
    ),
    (
        b"physical:kp_enter",
        TriggerKey::Physical(KeyKey::NumpadEnter),
    ),
    (
        b"physical:kp_equal",
        TriggerKey::Physical(KeyKey::NumpadEqual),
    ),
    (
        b"physical:kp_separator",
        TriggerKey::Physical(KeyKey::NumpadSeparator),
    ),
    (
        b"physical:kp_left",
        TriggerKey::Physical(KeyKey::NumpadLeft),
    ),
    (
        b"physical:kp_right",
        TriggerKey::Physical(KeyKey::NumpadRight),
    ),
    (b"physical:kp_up", TriggerKey::Physical(KeyKey::NumpadUp)),
    (
        b"physical:kp_down",
        TriggerKey::Physical(KeyKey::NumpadDown),
    ),
    (
        b"physical:kp_page_up",
        TriggerKey::Physical(KeyKey::NumpadPageUp),
    ),
    (
        b"physical:kp_page_down",
        TriggerKey::Physical(KeyKey::NumpadPageDown),
    ),
    (
        b"physical:kp_home",
        TriggerKey::Physical(KeyKey::NumpadHome),
    ),
    (b"physical:kp_end", TriggerKey::Physical(KeyKey::NumpadEnd)),
    (
        b"physical:kp_insert",
        TriggerKey::Physical(KeyKey::NumpadInsert),
    ),
    (
        b"physical:kp_delete",
        TriggerKey::Physical(KeyKey::NumpadDelete),
    ),
    (
        b"physical:kp_begin",
        TriggerKey::Physical(KeyKey::NumpadBegin),
    ),
    (
        b"physical:left_shift",
        TriggerKey::Physical(KeyKey::ShiftLeft),
    ),
    (
        b"physical:right_shift",
        TriggerKey::Physical(KeyKey::ShiftRight),
    ),
    (
        b"physical:left_control",
        TriggerKey::Physical(KeyKey::ControlLeft),
    ),
    (
        b"physical:right_control",
        TriggerKey::Physical(KeyKey::ControlRight),
    ),
    (b"physical:left_alt", TriggerKey::Physical(KeyKey::AltLeft)),
    (
        b"physical:right_alt",
        TriggerKey::Physical(KeyKey::AltRight),
    ),
    (
        b"physical:left_super",
        TriggerKey::Physical(KeyKey::MetaLeft),
    ),
    (
        b"physical:right_super",
        TriggerKey::Physical(KeyKey::MetaRight),
    ),
];

fn backwards_compatible_keys_get(part: &[u8]) -> Option<TriggerKey> {
    for (k, v) in BACKWARDS_COMPATIBLE_KEYS {
        if *k == part {
            return Some(*v);
        }
    }
    None
}

// ===========================================================================
// Parser
// ===========================================================================

/// PORT NOTE: `Parser.Elem` — the union yielded by the parser iterator.
#[derive(Clone, Debug, PartialEq)]
pub enum Elem {
    /// A leader trigger in a sequence.
    Leader(Trigger),

    /// The final trigger and action in a sequence.
    Binding(Binding),

    /// A chained action `chain=<action>` that should be appended
    /// to the previous binding. Note that any action is parsed, including
    /// invalid actions for chains such as `unbind`. We expect downstream
    /// consumers to validate that the action is valid for chaining.
    Chain(Action),
}

/// Full binding parser. The binding parser is implemented as an iterator
/// which yields elements to support multi-key sequences without allocation.
pub struct Parser {
    pub trigger_it: SequenceIterator,
    pub action: Action,
    pub flags: Flags,
    pub chain: bool,
}

impl Parser {
    pub fn init(raw_input: &[u8]) -> Result<Parser, Error> {
        // const flags, const start_idx = try parseFlags(raw_input);
        let (flags, start_idx) = parse_flags(raw_input)?;
        let input = &raw_input[start_idx..];

        // Find the equal sign. This is more complicated than it seems on
        // the surface because we need to ignore equal signs that are
        // part of the trigger.
        let eql_idx: usize = 'eql: {
            // TODO: We should change this parser into a real state machine
            // based parser that parses the trigger fully, then yields the
            // action after. The loop below is a total mess.
            let mut offset: usize = 0;
            while let Some(offset_idx) = input[offset..].iter().position(|&c| c == b'=') {
                // Find: '=+ctrl' or '==action'
                let idx = offset + offset_idx;
                if idx < input.len() - 1 && (input[idx + 1] == b'+' || input[idx + 1] == b'=') {
                    offset += offset_idx + 1;
                    continue;
                }

                // Looks like the real equal sign.
                break 'eql idx;
            }

            return Err(Error::InvalidFormat);
        };

        // Detect chains. Chains must not have flags.
        let chain = &input[0..eql_idx] == b"chain";
        if chain && start_idx > 0 {
            return Err(Error::InvalidFormat);
        }

        // Sequence iterator goes up to the equal, action is after. We can
        // parse the action now.
        Ok(Parser {
            trigger_it: SequenceIterator {
                // This is kind of hacky but we put a dummy trigger
                // for chained inputs. The `next` will never yield this
                // because we have chain set. When we find a nicer way to
                // do this we can remove it, the e2e is tested.
                input: if chain {
                    b"a".to_vec()
                } else {
                    input[0..eql_idx].to_vec()
                },
                i: 0,
            },
            action: Action::parse(&input[eql_idx + 1..])?,
            flags,
            chain,
        })
    }

    // PORT NOTE: named `next` to mirror the reference `Parser.next`; it is not an
    // `Iterator::next` (different signature/semantics — fallible, yields `Elem`).
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<Option<Elem>, Error> {
        // Get our trigger. If we're out of triggers then we're done.
        let trigger = match self.trigger_it.next()? {
            Some(t) => t,
            None => return Ok(None),
        };

        // If this is our last trigger then it is our final binding.
        if !self.trigger_it.done() {
            // Global/all bindings can't be sequences
            if self.flags.global || self.flags.all {
                return Err(Error::InvalidFormat);
            }
            return Ok(Some(Elem::Leader(trigger)));
        }

        // If we're a chain then return it as-is.
        if self.chain {
            return Ok(Some(Elem::Chain(self.action.clone())));
        }

        // Out of triggers, yield the final action.
        Ok(Some(Elem::Binding(Binding {
            trigger,
            action: self.action.clone(),
            flags: self.flags,
        })))
    }

    pub fn reset(&mut self) {
        self.trigger_it.i = 0;
    }
}

// fn parseFlags(raw_input) Error!struct { Flags, usize }
fn parse_flags(raw_input: &[u8]) -> Result<(Flags, usize), Error> {
    let mut flags: Flags = Flags::default();

    let mut start_idx: usize = 0;
    let mut input: &[u8] = raw_input;
    // PORT NOTE: mirrors the reference `while (true) { ... orelse break; }`.
    #[allow(clippy::while_let_loop)]
    loop {
        // Find the next prefix
        let idx = match input.iter().position(|&c| c == b':') {
            Some(i) => i,
            None => break,
        };
        let prefix = &input[0..idx];

        // If the prefix is one of our flags then set it.
        if prefix == b"all" {
            if flags.all {
                return Err(Error::InvalidFormat);
            }
            flags.all = true;
        } else if prefix == b"global" {
            if flags.global {
                return Err(Error::InvalidFormat);
            }
            flags.global = true;
        } else if prefix == b"unconsumed" {
            if !flags.consumed {
                return Err(Error::InvalidFormat);
            }
            flags.consumed = false;
        } else if prefix == b"performable" {
            if flags.performable {
                return Err(Error::InvalidFormat);
            }
            flags.performable = true;
        } else {
            // If we don't recognize the prefix then we're done. We
            // let any unknown prefix fallthrough to trigger-specific
            // parsing in case there are trigger-specific prefixes
            // (none currently but historically there was `physical:`
            // at one point). Breaking here lets us always implement new
            // prefixes.
            break;
        }

        // Move past the prefix
        start_idx += idx + 1;
        input = &input[idx + 1..];
    }

    Ok((flags, start_idx))
}

/// An iterator that yields each trigger in a sequence of triggers. For
/// example, the sequence "ctrl+a>ctrl+b" would yield "ctrl+a" and then
/// "ctrl+b". The iterator approach allows us to parse a sequence of
/// triggers without allocations.
//
// PORT NOTE: the reference holds `input: []const u8` (a borrow). Here the
// `Parser` owns the trigger substring as a `Vec<u8>` so the iterator is self-
// contained (the reference's "no allocations" is a performance property, not an
// observable one — preserved as a `Goal-2` candidate, not a parity invariant).
pub struct SequenceIterator {
    /// The input of triggers. This is expected to be ONLY triggers. Things
    /// like the "unconsumed:" prefix or action must be stripped before
    /// passing to this iterator.
    pub input: Vec<u8>,
    pub i: usize,
}

impl SequenceIterator {
    /// Returns the next trigger in the sequence if there is no parsing error.
    // PORT NOTE: named `next` to mirror `SequenceIterator.next`; fallible and not
    // an `Iterator::next`.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<Option<Trigger>, Error> {
        if self.done() {
            return Ok(None);
        }
        let rem = &self.input[self.i..];
        let idx = rem.iter().position(|&c| c == b'>').unwrap_or(rem.len());
        // defer self.i += idx + 1;
        let result = Trigger::parse(&rem[0..idx]);
        self.i += idx + 1;
        Ok(Some(result?))
    }

    /// Returns true if there are no more triggers to parse.
    pub fn done(&self) -> bool {
        self.i > self.input.len()
    }
}

/// Parse a single, non-sequenced binding. To support sequences you must
/// use parse. This is a convenience function for single bindings aimed
/// primarily at tests.
///
/// This doesn't support `chain` either, since chaining requires some
/// stateful concept of a prior binding.
//
// PORT NOTE: the reference error set is `Error || error{ UnexpectedChain,
// UnexpectedSequence }`. Modeled as `ParseSingleError`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseSingleError {
    InvalidFormat,
    InvalidAction,
    UnexpectedChain,
    UnexpectedSequence,
}

impl From<Error> for ParseSingleError {
    fn from(e: Error) -> ParseSingleError {
        match e {
            Error::InvalidFormat => ParseSingleError::InvalidFormat,
            Error::InvalidAction => ParseSingleError::InvalidAction,
        }
    }
}

pub fn parse_single(raw_input: &[u8]) -> Result<Binding, ParseSingleError> {
    let mut p = Parser::init(raw_input)?;
    let elem = match p.next()? {
        Some(e) => e,
        None => return Err(ParseSingleError::InvalidFormat),
    };
    match elem {
        Elem::Leader(_) => Err(ParseSingleError::UnexpectedSequence),
        Elem::Binding(b) => Ok(b),
        Elem::Chain(_) => Err(ParseSingleError::UnexpectedChain),
    }
}

// ===========================================================================
// Set
// ===========================================================================

// PORT NOTE: the reference `Set` uses `std.ArrayHashMapUnmanaged(K, V, Context,
// store_hash=true)` for both the forward (`Trigger -> Value`) and reverse
// (`Action -> Trigger`) maps, where `Context` defines `hash` (truncated wyhash)
// and `eql` (`bindingSetEqual`: `Trigger.foldedEqual` / `Action.equal`). It is
// INSERTION-ORDERED with `getOrPut`/`swapRemove`/`swapRemoveAt`/`getEntry`.
//
// rustty-zigstd's `AutoArrayHashMap` requires `K: Eq + Hash` (Rust std hashing),
// which CANNOT express the folded-equality / truncated-wyhash contexts, so the
// `Set`'s two maps are implemented here as `CtxArrayHashMap<K, V>` — an
// insertion-ordered vector of entries plus a parallel stored-hash array, with all
// lookups going through a `MapContext` (exactly Zig's `Context`). This is the
// faithful analog of `ArrayHashMapUnmanaged(..., store_hash=true)`.

/// The hash map context for the set. This defines how the hash map
/// gets the hash key and checks for equality.
//
// PORT NOTE: mirrors Zig's `Set.Context(comptime KeyType)`.
trait MapContext<K> {
    fn ctx_hash(k: &K) -> u32;
    fn ctx_eql(a: &K, b: &K) -> bool;
}

/// Context for `Trigger` keys (forward map): folded equality.
struct TriggerCtx;
impl MapContext<Trigger> for TriggerCtx {
    fn ctx_hash(k: &Trigger) -> u32 {
        // This seems crazy at first glance but this is also how
        // the Zig standard library handles hashing for array
        // hash maps!
        // return @truncate(k.hash());
        k.hash() as u32
    }
    fn ctx_eql(a: &Trigger, b: &Trigger) -> bool {
        // return a.bindingSetEqual(b);  (Trigger.bindingSetEqual = foldedEqual)
        a.folded_equal(*b)
    }
}

/// Context for `Action` keys (reverse map): structural equality.
struct ActionCtx;
impl MapContext<Action> for ActionCtx {
    fn ctx_hash(k: &Action) -> u32 {
        k.hash() as u32
    }
    fn ctx_eql(a: &Action, b: &Action) -> bool {
        // Action.bindingSetEqual = equal
        a.equal(b)
    }
}

/// Insertion-ordered hash map with an explicit context (faithful port of
/// `std.ArrayHashMapUnmanaged(K, V, Context, store_hash=true)`).
//
// PORT NOTE: Clone/Debug/PartialEq are impl'd manually (below) because the
// `PhantomData<C>` marker carries a `MapContext` bound that need not itself be
// Clone/Debug/PartialEq.
struct CtxArrayHashMap<K, V, C: MapContext<K>> {
    /// Entries in insertion order: (key, stored_hash, value).
    entries: Vec<(K, u32, V)>,
    _ctx: std::marker::PhantomData<C>,
}

impl<K: Clone, V: Clone, C: MapContext<K>> Clone for CtxArrayHashMap<K, V, C> {
    fn clone(&self) -> Self {
        CtxArrayHashMap {
            entries: self.entries.clone(),
            _ctx: std::marker::PhantomData,
        }
    }
}

impl<K: std::fmt::Debug, V: std::fmt::Debug, C: MapContext<K>> std::fmt::Debug
    for CtxArrayHashMap<K, V, C>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CtxArrayHashMap")
            .field("entries", &self.entries)
            .finish()
    }
}

impl<K: PartialEq, V: PartialEq, C: MapContext<K>> PartialEq for CtxArrayHashMap<K, V, C> {
    fn eq(&self, other: &Self) -> bool {
        self.entries == other.entries
    }
}

impl<K: Clone, V, C: MapContext<K>> CtxArrayHashMap<K, V, C> {
    fn new() -> Self {
        CtxArrayHashMap {
            entries: Vec::new(),
            _ctx: std::marker::PhantomData,
        }
    }

    fn count(&self) -> usize {
        self.entries.len()
    }

    /// Find the entry index for `key` via context hash+eql.
    fn find(&self, key: &K) -> Option<usize> {
        let h = C::ctx_hash(key);
        self.entries
            .iter()
            .position(|(k, kh, _)| *kh == h && C::ctx_eql(k, key))
    }

    /// `getEntry`: returns the index of the entry, if present.
    fn get_index(&self, key: &K) -> Option<usize> {
        self.find(key)
    }

    /// `get(key)` returning a value reference.
    fn get(&self, key: &K) -> Option<&V> {
        self.find(key).map(|i| &self.entries[i].2)
    }

    /// `getOrPut(key)`: returns (index, found_existing). New entries are
    /// appended with `make` for the value.
    fn get_or_put_with<F: FnOnce() -> V>(&mut self, key: K, make: F) -> (usize, bool) {
        if let Some(i) = self.find(&key) {
            (i, true)
        } else {
            let h = C::ctx_hash(&key);
            let i = self.entries.len();
            self.entries.push((key, h, make()));
            (i, false)
        }
    }

    /// `put(key, value)`: insert or overwrite (position preserved).
    fn put(&mut self, key: K, value: V) {
        if let Some(i) = self.find(&key) {
            self.entries[i].2 = value;
        } else {
            let h = C::ctx_hash(&key);
            self.entries.push((key, h, value));
        }
    }

    /// `swapRemove(key)`: remove by swapping the LAST entry into its slot.
    fn swap_remove(&mut self, key: &K) -> Option<V> {
        let i = self.find(key)?;
        Some(self.entries.swap_remove(i).2)
    }

    /// `swapRemoveAt(index)`.
    fn swap_remove_at(&mut self, index: usize) -> V {
        self.entries.swap_remove(index).2
    }

    fn key_at(&self, index: usize) -> &K {
        &self.entries[index].0
    }
    fn value_at(&self, index: usize) -> &V {
        &self.entries[index].2
    }
    fn value_at_mut(&mut self, index: usize) -> &mut V {
        &mut self.entries[index].2
    }
}

/// Leaf node of a set is an action to trigger. This is a "leaf" compared
/// to the inner nodes which are "leaders" for sequences.
#[derive(Clone, Debug, PartialEq)]
pub struct Leaf {
    pub action: Action,
    pub flags: Flags,
}

impl Leaf {
    pub fn clone_in(&self) -> Leaf {
        Leaf {
            action: self.action.clone_action(),
            flags: self.flags,
        }
    }

    pub fn hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.action.hash_incremental(&mut hasher);
        // std.hash.autoHash(&hasher, self.flags);
        hasher.write_u8(self.flags.cval());
        hasher.finish()
    }

    pub fn generic(&self) -> GenericLeaf {
        GenericLeaf {
            flags: self.flags,
            actions: GenericLeafActions::Single([self.action.clone()]),
        }
    }
}

/// Leaf node of a set that triggers multiple actions in sequence.
#[derive(Clone, Debug, PartialEq)]
pub struct LeafChained {
    pub actions: Vec<Action>,
    pub flags: Flags,
}

impl LeafChained {
    pub fn clone_in(&self) -> LeafChained {
        // var cloned_actions = try self.actions.clone(alloc);
        // for (cloned_actions.items) |*action| action.* = try action.clone(alloc);
        let cloned_actions = self.actions.iter().map(|a| a.clone_action()).collect();
        LeafChained {
            actions: cloned_actions,
            flags: self.flags,
        }
    }

    pub fn generic(&self) -> GenericLeaf {
        GenericLeaf {
            flags: self.flags,
            actions: GenericLeafActions::Many(self.actions.clone()),
        }
    }
}

/// A generic leaf node that can be used to unify the handling of
/// leaf and leaf_chained.
#[derive(Clone, Debug, PartialEq)]
pub struct GenericLeaf {
    pub flags: Flags,
    pub actions: GenericLeafActions,
}

#[derive(Clone, Debug, PartialEq)]
pub enum GenericLeafActions {
    Single([Action; 1]),
    Many(Vec<Action>),
}

impl GenericLeaf {
    pub fn actions_slice(&self) -> &[Action] {
        match &self.actions {
            GenericLeafActions::Single(arr) => arr,
            GenericLeafActions::Many(slice) => slice,
        }
    }
}

/// The entry type for the forward mapping of trigger to action.
//
// PORT NOTE: `Set.Value = union(enum) { leader, leaf, leaf_chained }`. The
// `leader` arm owns its child `Set` (the reference stores `*Set` allocated with
// the arena; Rust owns it with `Box`).
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// This key is a leader key in a sequence. You must follow the given
    /// set to find the next key in the sequence.
    Leader(Box<Set>),

    /// This trigger completes a sequence and the value is the action
    /// to take along with the flags that may define binding behavior.
    Leaf(Leaf),

    /// A set of actions to take in response to a trigger.
    LeafChained(LeafChained),
}

impl Value {
    /// Implements the formatter for the fmt package. This encodes the
    /// action back into the format used by parse.
    //
    // PORT NOTE: the reference `format` only handles `.leader`/`.leaf` (the
    // `.leaf_chained` arm is unhandled in `format`, which would be a comptime/
    // runtime error if reached — it is reachable only via `formatEntries`). We
    // preserve that: `format` panics for `leaf_chained` exactly as the reference
    // switch (which has no `leaf_chained` arm) would fail to compile/match.
    pub fn format<W: Writer>(&self, writer: &mut W) -> Result<(), W::Error> {
        match self {
            Value::Leader(set) => {
                // the leader key was already printed.
                for (k, _, v) in &set.bindings.entries {
                    // try writer.print(">{s}{s}", .{ binding.key_ptr.*, binding.value_ptr.* });
                    writer.write_all(b">")?;
                    k.format(writer)?;
                    v.format(writer)?;
                }
                Ok(())
            }
            Value::Leaf(leaf) => {
                // action implements the format
                writer.write_all(b"=")?;
                leaf.action.format(writer)
            }
            Value::LeafChained(_) => {
                // PORT NOTE: unreachable arm in the reference `format` (no
                // `leaf_chained` case); use `format_entries` for chained values.
                unreachable!("Value.format does not handle leaf_chained")
            }
        }
    }

    /// Writes the configuration entries for the binding
    /// that this value is part of.
    ///
    /// The value may be part of multiple configuration entries
    /// if they're all part of the same prefix sequence (e.g. 'a>b', 'a>c').
    /// These will result in multiple separate entries in the configuration.
    ///
    /// `buffer_stream` is a FixedBufferStream used for temporary storage
    /// that is shared between calls to nested levels of the set.
    /// For example, 'a>b>c=x' and 'a>b>d=y' will reuse the 'a>b' written
    /// to the buffer before flushing it to the formatter with 'c=x' and 'd=y'.
    //
    // PORT NOTE: the reference threads a `*std.Io.Writer` FIXED buffer (`buffer`)
    // that supports rewinding via `buffer.end = pos`. The Rust analog is a
    // `RewindBuffer` (a `Vec<u8>` with an explicit `end` cursor) so the `pos`/
    // `end` rewinding semantics — sharing a prefix between sibling entries — are
    // reproduced exactly. `formatter` is the cfg-fmt `EntryFormatter`.
    pub fn format_entries<W: Writer>(
        &self,
        buffer: &mut RewindBuffer,
        formatter: &mut EntryFormatter<W>,
    ) -> Result<(), FormatEntriesError<W::Error>> {
        match self {
            Value::Leader(set) => {
                // We'll rewind to this position after each sub-entry,
                // sharing the prefix between siblings.
                let pos = buffer.end;

                for (k, _, v) in &set.bindings.entries {
                    // I'm not exactly if this is safe for any arbitrary
                    // writer since the Writer interface does not have any
                    // rewind functions, but for our use case of a
                    // fixed-size buffer writer this should work just fine.
                    buffer.end = pos;
                    // buffer.print(">{f}", .{binding.key_ptr.*}) catch return error.OutOfMemory;
                    buffer
                        .write_all(b">")
                        .map_err(|_| FormatEntriesError::OutOfMemory)?;
                    write_trigger_to_buffer(k, buffer)
                        .map_err(|_| FormatEntriesError::OutOfMemory)?;
                    v.format_entries(buffer, formatter)?;
                }
                Ok(())
            }

            Value::Leaf(leaf) => {
                // When we get to the leaf, the buffer_stream contains
                // the full sequence of keys needed to reach this action.
                // buffer.print("={f}", .{leaf.action}) catch return error.OutOfMemory;
                buffer
                    .write_all(b"=")
                    .map_err(|_| FormatEntriesError::OutOfMemory)?;
                write_action_to_buffer(&leaf.action, buffer)
                    .map_err(|_| FormatEntriesError::OutOfMemory)?;
                // try formatter.formatEntry([]const u8, buffer.buffer[0..buffer.end]);
                formatter
                    .format_entry(&ByteSlice(buffer.slice().to_vec()))
                    .map_err(FormatEntriesError::from)
            }

            Value::LeafChained(leaf) => {
                let pos = buffer.end;
                for (i, action) in leaf.actions.iter().enumerate() {
                    if i == 0 {
                        // buffer.print("={f}", .{action})
                        buffer
                            .write_all(b"=")
                            .map_err(|_| FormatEntriesError::OutOfMemory)?;
                        write_action_to_buffer(action, buffer)
                            .map_err(|_| FormatEntriesError::OutOfMemory)?;
                    } else {
                        buffer.end = 0;
                        // buffer.print("chain={f}", .{action})
                        buffer
                            .write_all(b"chain=")
                            .map_err(|_| FormatEntriesError::OutOfMemory)?;
                        write_action_to_buffer(action, buffer)
                            .map_err(|_| FormatEntriesError::OutOfMemory)?;
                    }
                    formatter
                        .format_entry(&ByteSlice(buffer.slice().to_vec()))
                        .map_err(FormatEntriesError::from)?;
                    buffer.end = pos;
                }
                Ok(())
            }
        }
    }
}

/// PORT NOTE: error union of `formatEntries` — `error{OutOfMemory}` (from the
/// fixed buffer overflowing) plus the formatter writer's error.
pub enum FormatEntriesError<E> {
    OutOfMemory,
    Writer(E),
}

/// The `EntryFormatter::format_entry` error union (`FormatError`) folds into
/// `formatEntries`' inferred error set variant-for-variant — the reference's
/// `try formatter.formatEntry(...)` merges both `error.OutOfMemory` sources
/// into one inferred `!void` set.
impl<E> From<FormatError<E>> for FormatEntriesError<E> {
    fn from(e: FormatError<E>) -> FormatEntriesError<E> {
        match e {
            FormatError::Writer(w) => FormatEntriesError::Writer(w),
            FormatError::OutOfMemory => FormatEntriesError::OutOfMemory,
        }
    }
}

/// PORT NOTE: a `[]const u8` value wrapper implementing `FormatEntry` so it can
/// be passed to `EntryFormatter::format_entry` (the reference calls
/// `formatter.formatEntry([]const u8, bytes)` — the `.pointer` `[]const u8` arm,
/// `format_entry_string`).
struct ByteSlice(Vec<u8>);
impl FormatEntry for ByteSlice {
    fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        format_entry_string(name, &self.0, writer)
    }
}

/// PORT NOTE: the reference's fixed `*std.Io.Writer` over a stack buffer, with an
/// `end` cursor that callers rewind (`buffer.end = pos`). `slice()` is
/// `buffer.buffer[0..buffer.end]`.
pub struct RewindBuffer {
    buf: Vec<u8>,
    end: usize,
}

/// `var buf: [1024]u8 = undefined;` — the capacity of the reference's stack
/// buffer (`Config.zig` `Keybinds.formatEntryDocs`, `RemapSet.formatEntry`).
/// The bound is OBSERVABLE: `std.Io.Writer.fixed(&buf)` fails once a write would
/// pass it, and every call site turns that into `error.OutOfMemory`.
pub const REWIND_BUFFER_CAPACITY: usize = 1024;

impl RewindBuffer {
    pub fn new() -> RewindBuffer {
        RewindBuffer {
            buf: Vec::new(),
            end: 0,
        }
    }
    // PORT NOTE: the `()` error models the reference's fixed-`std.Io.Writer`
    // overflow (`error.OutOfMemory`), which every caller maps to that error. The
    // buffer is heap-backed here (no 1 KiB of stack per format call), but it is
    // CAPPED at the reference's `[1024]u8` so the overflow boundary is identical:
    // a write that would push `end` past 1024 fails instead of growing.
    #[allow(clippy::result_unit_err)]
    pub fn write_all(&mut self, bytes: &[u8]) -> Result<(), ()> {
        // A fixed `std.Io.Writer` fails the write once it no longer fits.
        if self.end + bytes.len() > REWIND_BUFFER_CAPACITY {
            return Err(());
        }
        // Mirrors writing into a fixed buffer at the `end` cursor, overwriting.
        if self.buf.len() < self.end {
            self.buf.resize(self.end, 0);
        }
        self.buf.truncate(self.end);
        self.buf.extend_from_slice(bytes);
        self.end += bytes.len();
        Ok(())
    }
    fn slice(&self) -> &[u8] {
        &self.buf[0..self.end]
    }
}

impl Default for RewindBuffer {
    fn default() -> Self {
        RewindBuffer::new()
    }
}

// Helpers to write a Trigger/Action through the RewindBuffer (the reference uses
// `{f}` format of the trigger/action). They reuse the `Writer` impl below.
//
// PORT NOTE: `pub` so the `Keybinds.formatEntryDocs` value type (rustty-config)
// can write the leading `{f}` trigger (and table-name prefix) into the shared
// `RewindBuffer` before delegating to `Value::format_entries`, exactly as the
// reference does (`writer.print("{f}", .{k})` / `"{s}/{f}"`).
// PORT NOTE: `()` error mirrors the fixed-buffer write error (produced when the
// write passes `REWIND_BUFFER_CAPACITY`); `result_unit_err` allowed for that
// fidelity.
#[allow(clippy::result_unit_err)]
pub fn write_trigger_to_buffer(t: &Trigger, buffer: &mut RewindBuffer) -> Result<(), ()> {
    let mut w = RewindWriter { inner: buffer };
    t.format(&mut w)
}
fn write_action_to_buffer(a: &Action, buffer: &mut RewindBuffer) -> Result<(), ()> {
    let mut w = RewindWriter { inner: buffer };
    a.format(&mut w)
}

struct RewindWriter<'a> {
    inner: &'a mut RewindBuffer,
}
impl Writer for RewindWriter<'_> {
    type Error = ();
    fn write_all(&mut self, bytes: &[u8]) -> Result<(), ()> {
        self.inner.write_all(bytes)
    }
}

/// A structure that contains a set of bindings and focuses on fast lookup.
/// The use case is that this will be called on EVERY key input to look
/// for an associated action so it must be fast.
//
// PORT NOTE: the reference is allocator-managed (`HashMap`/`ReverseMap` are
// `ArrayHashMapUnmanaged` threaded with `alloc`), and uses raw `*Set`/`*Trigger`/
// `*Value` pointers for recursion and `chain_parent`. The Rust port owns its
// child sets via `Box<Set>` (in `Value::Leader`) and the maps via
// `CtxArrayHashMap`. `chain_parent` raw pointers (`*Trigger`/`*Value`/`*Set`) are
// modeled as a NAVIGABLE LOCATOR (`ChainParent`) — the path of leader triggers
// from the root to the containing set, plus the entry's key — so the entry can be
// re-resolved without aliasing UB (memory: raw-ptr discipline). The allocator
// (`alloc`) parameters are kept on the public methods as `()`-free signatures.
#[derive(Clone, Debug, PartialEq)]
pub struct Set {
    /// The set of bindings.
    bindings: CtxArrayHashMap<Trigger, Value, TriggerCtx>,

    /// The reverse mapping of action to binding. Note that multiple
    /// bindings can map to the same action and this map will only have
    /// the most recently added binding for an action.
    ///
    /// Sequenced triggers are never present in the reverse map at this time.
    /// This is a conscious decision since the primary use case of the reverse
    /// map is to support GUI toolkit keyboard accelerators and no mainstream
    /// GUI toolkit supports sequences.
    ///
    /// Performable triggers are also not present in the reverse map. This
    /// is so that GUI toolkits don't register performable triggers as
    /// menu shortcuts (the primary use case of the reverse map). GUI toolkits
    /// such as GTK handle menu shortcuts too early in the event lifecycle
    /// for performable to work so this is a conscious decision to ease the
    /// integration with GUI toolkits.
    reverse: CtxArrayHashMap<Action, Trigger, ActionCtx>,

    /// The chain parent is the information necessary to attach a chained
    /// action to the proper location in our mapping. It tracks both the
    /// entry in the hashmap and the set it belongs to, which is needed
    /// to properly update reverse mappings when converting a leaf to
    /// a chained action.
    chain_parent: Option<ChainParent>,
}

/// Information about a chain parent entry, including which set it
/// belongs to. This is needed because reverse mappings are only
/// maintained in the root set, but the chain parent entry may be
/// in a nested set (for leader key sequences).
//
// PORT NOTE: the reference fields are raw pointers `key_ptr: *Trigger`,
// `value_ptr: *Value`, `set: *Set`. Ported as a locator: `set_path` is the chain
// of leader triggers from the root set to the set containing the entry, and `key`
// is the entry's trigger. The entry is re-resolved on demand (see
// `chain_parent_*` helpers), preserving the observable behavior without holding
// aliasing raw pointers.
#[derive(Clone, Debug, PartialEq)]
pub struct ChainParent {
    pub set_path: Vec<Trigger>,
    pub key: Trigger,
}

impl Default for Set {
    fn default() -> Self {
        Set {
            bindings: CtxArrayHashMap::new(),
            reverse: CtxArrayHashMap::new(),
            chain_parent: None,
        }
    }
}

impl Set {
    // A full key-value entry for the set.
    //
    // PORT NOTE: `pub const Entry = HashMap.Entry;` — a (key, value) entry. The
    // Rust port represents an entry by its insertion index into the bindings map
    // (see `get_index`/`get_value`/`get_key`/`value_at`).

    // PORT NOTE: `deinit(self, alloc)` frees all child leader sets and the maps.
    // In Rust this is handled by `Drop`/ownership (`Box<Set>` children are freed
    // recursively), so no explicit `deinit` is needed; a no-op shim preserves the
    // call site shape (`s.deinit(alloc)`).
    pub fn deinit(&mut self) {
        // Clear any leaders if we have them
        // self.* = undefined; — handled by Rust ownership / Drop.
    }

    /// Get a binding for a given trigger.
    //
    // PORT NOTE: the reference returns `?Entry` (a key/value pointer pair). Here
    // we return the entry index; `get_value`/`get_key` read through it. Tests
    // that did `.?.value_ptr.*` use `get_value`.
    pub fn get_index(&self, t: Trigger) -> Option<usize> {
        self.bindings.get_index(&t)
    }

    /// `self.bindings.count()` — the number of bindings in this set.
    //
    // PORT NOTE: the reference reaches into `self.set.bindings.count()` directly
    // (the field is `pub`). `bindings` is private here, so this accessor mirrors
    // it for the `Keybinds` value type and tests.
    pub fn bindings_count(&self) -> usize {
        self.bindings.count()
    }

    /// `self.bindings.iterator()` — iterate `(trigger, value)` entries in
    /// insertion order.
    //
    // PORT NOTE: the reference iterates `self.set.bindings.iterator()` yielding
    // `entry.key_ptr.* / entry.value_ptr.*`. `bindings` is private here, so this
    // accessor exposes the same insertion-ordered view for `Keybinds.equalSet`
    // and `Keybinds.formatEntryDocs`.
    pub fn iter_bindings(&self) -> impl Iterator<Item = (&Trigger, &Value)> {
        self.bindings.entries.iter().map(|(k, _, v)| (k, v))
    }

    /// `get(t).?.value_ptr.*` — the value for a trigger.
    pub fn get_value(&self, t: Trigger) -> Option<&Value> {
        self.bindings.get(&t)
    }

    /// `get(t).?.key_ptr.*` — the stored key for a trigger.
    pub fn get_key(&self, t: Trigger) -> Option<&Trigger> {
        self.bindings.get_index(&t).map(|i| self.bindings.key_at(i))
    }

    /// Get a trigger for the given action. An action can have multiple
    /// triggers so this will return the first one found.
    pub fn get_trigger(&self, a: &Action) -> Option<Trigger> {
        self.reverse.get(a).copied()
    }

    /// Whether the set currently has a chain parent (for tests).
    pub fn chain_parent(&self) -> Option<&ChainParent> {
        self.chain_parent.as_ref()
    }

    /// Resolve the `Set` containing the chain-parent entry (follows `set_path`).
    fn resolve_set_mut(&mut self, path: &[Trigger]) -> &mut Set {
        let mut cur: &mut Set = self;
        for t in path {
            let idx = cur.bindings.get_index(t).expect("chain path leader exists");
            match cur.bindings.value_at_mut(idx) {
                Value::Leader(s) => cur = s.as_mut(),
                _ => unreachable!("chain path entry must be a leader"),
            }
        }
        cur
    }
}

impl Set {
    /// Add a binding to the set. If the binding already exists then
    /// this will overwrite it.
    pub fn put(&mut self, t: Trigger, action: Action) {
        self.put_flags(t, action, Flags::default());
    }

    /// Add a binding to the set with explicit flags.
    //
    // PORT NOTE: the reference uses `getOrPut` returning raw `key_ptr`/`value_ptr`
    // which it stores in `chain_parent`. We reproduce the same effect: do the
    // get-or-put, set the value to a leaf, update the reverse map, and record the
    // chain_parent as `{ set_path: [], key: t }` (this method always operates on
    // `self`, i.e. the set on which it is called).
    pub fn put_flags(&mut self, t: Trigger, action: Action, flags: Flags) {
        // unbind should never go into the set, it should be handled prior
        debug_assert!(action != Action::Unbind);

        // This is true if we're going to track this entry as
        // a reverse mapping. There are certain scenarios we don't.
        // See the reverse map docs for more information.
        let track_reverse: bool = !flags.performable;

        // No matter what our chained parent becomes invalid because
        // getOrPut invalidates pointers.
        self.chain_parent = None;

        let (idx, found_existing) = self.bindings.get_or_put_with(t, || {
            Value::Leaf(Leaf {
                action: action.clone(),
                flags,
            })
        });
        // errdefer {
        //     // If we have any errors we can't trust our values here. And
        //     // we can't restore the old values because they're also invalidated
        //     // by getOrPut so we just disable chaining.
        //     self.chain_parent = null;
        // }
        //
        // PORT NOTE: the `getOrPut` here is infallible in the Rust port (no
        // allocator error path), so the reference's `errdefer` clearing
        // `chain_parent` cannot fire; its comment is preserved above.

        if found_existing {
            // PORT NOTE: the reference reads the existing value via `gop.value_ptr`
            // and cleans it up before overwriting.
            match self.bindings.value_at(idx) {
                // If we have a leader we need to clean up the memory
                Value::Leader(_) => {
                    // Box<Set> child is dropped when we overwrite below.
                }

                // If we have an existing binding for this trigger, we have to
                // update the reverse mapping to remove the old action.
                Value::Leaf(_) => {
                    if track_reverse {
                        let t_hash = t.hash();
                        let n = self.reverse.count();
                        for i in 0..n {
                            if t_hash == self.reverse.value_at(i).hash() {
                                self.reverse.swap_remove_at(i);
                                break;
                            }
                        }
                    }
                }

                // Chained leaves aren't in the reverse mapping so we just
                // clear it out.
                Value::LeafChained(_) => {
                    // dropped on overwrite below.
                }
            }
        }

        // gop.value_ptr.* = .{ .leaf = .{ .action = action, .flags = flags } };
        *self.bindings.value_at_mut(idx) = Value::Leaf(Leaf {
            action: action.clone(),
            flags,
        });

        // if (track_reverse) try self.reverse.put(alloc, action, t);
        if track_reverse {
            self.reverse.put(action, t);
        }

        // Record chain parent (this set, the entry we just added/updated).
        self.chain_parent = Some(ChainParent {
            set_path: Vec::new(),
            key: t,
        });

        // Invariant: after successful put, chain_parent must be valid and point
        // to the entry we just added/updated.
        debug_assert!(self.chain_parent.is_some());
        debug_assert!(matches!(self.bindings.value_at(idx), Value::Leaf(_)));
    }

    /// Get an entry for the given key event. This will attempt to find
    /// a binding using multiple parts of the event in the following order:
    ///
    ///   1. Physical key (event.physical_key)
    ///   2. Unshifted Unicode codepoint (event.unshifted_codepoint)
    ///
    pub fn get_event(&self, event: &KeyEvent) -> Option<usize> {
        let mut trigger: Trigger = Trigger {
            mods: event.mods.binding(),
            key: TriggerKey::Physical(event.key),
        };
        if let Some(v) = self.get_index(trigger) {
            return Some(v);
        }

        // If our UTF-8 text is exactly one codepoint, we try to match that.
        if !event.utf8.is_empty() {
            'unicode: {
                let s = match std::str::from_utf8(&event.utf8) {
                    Ok(s) => s,
                    Err(_) => break 'unicode,
                };
                let mut it = s.chars();
                // No codepoints or multiple codepoints drops to invalid format
                let cp = match it.next() {
                    Some(c) => c,
                    None => break 'unicode,
                };
                if it.next().is_some() {
                    break 'unicode;
                }

                trigger.key = TriggerKey::Unicode(cp as u21);
                if let Some(v) = self.get_index(trigger) {
                    return Some(v);
                }
            }
        }

        // Finally fallback to the full unshifted codepoint if we have one.
        // Question: should we be doing this if we have UTF-8 text? I
        // suspect "no" but we don't currently have any failing scenarios
        // to verify this.
        if event.unshifted_codepoint > 0 {
            trigger.key = TriggerKey::Unicode(event.unshifted_codepoint);
            if let Some(v) = self.get_index(trigger) {
                return Some(v);
            }
        }

        // Fallback to catch_all with modifiers first, then without modifiers.
        trigger.key = TriggerKey::CatchAll;
        if let Some(v) = self.get_index(trigger) {
            return Some(v);
        }
        if !trigger.mods.empty() {
            trigger.mods = Mods::default();
            if let Some(v) = self.get_index(trigger) {
                return Some(v);
            }
        }

        None
    }

    /// Get the leaf value at an entry index (test helper for `getEvent`).
    pub fn value_at(&self, index: usize) -> &Value {
        self.bindings.value_at(index)
    }

    /// Get the stored trigger key at an entry index.
    //
    // PORT NOTE (additive, core C10 — CORE_BLUEPRINT risk 7): the reference
    // `Set.Entry` is a key/value POINTER pair; the ported `getEvent` returns
    // the entry INDEX (see `get_index`), so `entry.key_ptr.*` reads
    // (Surface.maybeHandleBinding's `.key_sequence` trigger payload) go
    // through this accessor — the `value_at` sibling.
    pub fn key_at(&self, index: usize) -> &Trigger {
        self.bindings.key_at(index)
    }
}

impl Set {
    /// Remove a binding for a given trigger.
    pub fn remove(&mut self, t: Trigger) {
        self.remove_exact(t);
    }

    fn remove_exact(&mut self, t: Trigger) {
        // Removal always resets our chain parent. We could make this
        // finer grained but the way it is documented is that chaining
        // must happen directly after sets so this works.
        self.chain_parent = None;

        // var entry = self.bindings.get(t) orelse return;
        // _ = self.bindings.swapRemove(t);
        let entry = match self.bindings.swap_remove(&t) {
            Some(e) => e,
            None => return,
        };

        match entry {
            // For a leader removal, we need to deallocate our child set.
            // Leaders are never part of reverse maps so no other accounting
            // needs to be done.
            Value::Leader(_) => {
                // Box<Set> child dropped here.
            }

            // For an action we need to fix up the reverse mapping.
            Value::Leaf(leaf) => self.fixup_reverse_for_action(&leaf.action, t),

            // Chained leaves are never in our reverse mapping so no
            // cleanup is required.
            Value::LeafChained(_) => {
                // dropped here.
            }
        }
    }

    /// Fix up the reverse mapping after removing an action.
    ///
    /// When an action is removed from a binding (either by removal or by
    /// converting to a chained action), we need to update the reverse mapping.
    /// If another binding has the same action, we update the reverse mapping
    /// to point to that binding. Otherwise, we remove the action from the
    /// reverse mapping entirely.
    ///
    /// The `old` parameter is the trigger that was previously bound to this
    /// action. It is used to check if the reverse mapping still points to
    /// this trigger; if not, no fixup is needed since the reverse map already
    /// points to a different trigger for this action.
    ///
    /// Note: we'd LIKE to replace this with the most recent binding but
    /// our hash map obviously has no concept of ordering so we have to
    /// choose whatever. Maybe a switch to an array hash map here.
    fn fixup_reverse_for_action(&mut self, action: &Action, old: Trigger) {
        // const entry = self.reverse.getEntry(action) orelse return;
        let entry_idx = match self.reverse.get_index(action) {
            Some(i) => i,
            None => return,
        };

        // If our value is not the same as the old trigger, we can
        // ignore it because our reverse mapping points somewhere else.
        if !self.reverse.value_at(entry_idx).equal(old) {
            return;
        }

        // It is the same trigger, so let's now go through our bindings
        // and try to find another trigger that maps to the same action.
        let action_hash = action.hash();
        let n = self.bindings.count();
        for i in 0..n {
            match self.bindings.value_at(i) {
                Value::Leader(_) | Value::LeafChained(_) => {}
                Value::Leaf(leaf_search) => {
                    if leaf_search.action.hash() == action_hash {
                        let key = *self.bindings.key_at(i);
                        *self.reverse.value_at_mut(entry_idx) = key;
                        return;
                    }
                }
            }
        }

        // No other trigger points to this action so we remove
        // the reverse mapping completely.
        let _ = self.reverse.swap_remove(action);
    }

    /// Append a chained action to the prior set action.
    ///
    /// It is an error if there is no valid prior chain parent.
    pub fn append_chain(&mut self, action: Action) -> Result<(), AppendChainError> {
        // Unbind is not a valid chain action; callers must check this.
        debug_assert!(action != Action::Unbind);

        // const parent = self.chain_parent orelse return error.NoChainParent;
        let parent = match self.chain_parent.clone() {
            Some(p) => p,
            None => return Err(AppendChainError::NoChainParent),
        };

        // Resolve the parent set + entry.
        let path = parent.set_path.clone();
        let pkey = parent.key;

        // We need the parent value to decide leaf vs leaf_chained.
        let parent_set = self.resolve_set_mut(&path);
        let pidx = parent_set
            .bindings
            .get_index(&pkey)
            .expect("chain_parent entry exists");

        match parent_set.bindings.value_at(pidx) {
            // Leader can never be a chain parent. Verified through various
            // assertions and unit tests.
            Value::Leader(_) => unreachable!(),

            // If it is already a chained action, we just append the
            // action. Easy!
            Value::LeafChained(_) => {
                if let Value::LeafChained(leaf) = parent_set.bindings.value_at_mut(pidx) {
                    leaf.actions.push(action);
                }
                Ok(())
            }

            // If it is a leaf, we need to convert it to a leaf_chained.
            // We also need to be careful to remove any prior reverse
            // mappings for this action since chained actions are not
            // part of the reverse mapping.
            Value::Leaf(leaf) => {
                let leaf_action = leaf.action.clone();
                let leaf_flags = leaf.flags;

                // Setup our failable actions list first.
                // var actions: std.ArrayList(Action) = .empty;
                // try actions.ensureTotalCapacity(alloc, 2);
                // actions.appendAssumeCapacity(leaf.action);
                // actions.appendAssumeCapacity(action);
                //
                // PORT NOTE: mirrors the reference's reserve-then-append-assume-
                // capacity sequence (kept literal rather than `vec![..]`).
                #[allow(clippy::vec_init_then_push)]
                let actions: Vec<Action> = {
                    let mut actions: Vec<Action> = Vec::with_capacity(2);
                    actions.push(leaf_action.clone());
                    actions.push(action);
                    actions
                };

                // Convert to leaf_chained first, before fixing up reverse
                // mapping. This is important because fixupReverseForAction
                // searches for other bindings with the same action, and we
                // don't want to find this entry (which is now chained).
                *parent_set.bindings.value_at_mut(pidx) = Value::LeafChained(LeafChained {
                    actions,
                    flags: leaf_flags,
                });

                // Clean up our reverse mapping. Chained actions are not
                // part of the reverse mapping, so we need to fix up the
                // reverse map (possibly restoring another trigger for the
                // same action).
                //
                // PORT NOTE: `parent.set.fixupReverseForAction(...)` operates on
                // the set that OWNS the entry. We re-resolve it and call fixup.
                let parent_set = self.resolve_set_mut(&path);
                parent_set.fixup_reverse_for_action(&leaf_action, pkey);
                Ok(())
            }
        }
    }

    /// Deep clone the set.
    pub fn clone_set(&self) -> Set {
        // var result: Set = .{ .bindings = try self.bindings.clone(alloc),
        //                      .reverse = try self.reverse.clone(alloc) };
        let mut result = Set {
            bindings: CtxArrayHashMap::new(),
            reverse: CtxArrayHashMap::new(),
            // Clone should have null chain_parent.
            chain_parent: None,
        };

        // If we have any leaders we need to clone them.
        for (k, h, v) in &self.bindings.entries {
            let cloned = match v {
                // Leaves could have data to clone (i.e. text actions
                // contain allocated strings).
                Value::Leaf(s) => Value::Leaf(s.clone_in()),
                Value::LeafChained(s) => Value::LeafChained(s.clone_in()),
                // Must be deep cloned.
                Value::Leader(s) => Value::Leader(Box::new(s.clone_set())),
            };
            result.bindings.entries.push((*k, *h, cloned));
        }

        // We need to clone the action keys in the reverse map since
        // they may contain allocated values.
        for (a, h, t) in &self.reverse.entries {
            result.reverse.entries.push((a.clone_action(), *h, *t));
        }

        result
    }
}

/// PORT NOTE: error union of `appendChain` — `Allocator.Error || error{NoChainParent}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppendChainError {
    NoChainParent,
}

/// PORT NOTE: error union of `parseAndPut` — `Allocator.Error || Error`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseAndPutError {
    InvalidFormat,
    InvalidAction,
}

impl From<Error> for ParseAndPutError {
    fn from(e: Error) -> ParseAndPutError {
        match e {
            Error::InvalidFormat => ParseAndPutError::InvalidFormat,
            Error::InvalidAction => ParseAndPutError::InvalidAction,
        }
    }
}

// const ParseAndPutRecurseError = Allocator.Error || error{
//     SequenceUnbind, NoChainParent, UnexpectedEndOfInput, InvalidChainAction };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RecurseError {
    SequenceUnbind,
    NoChainParent,
    UnexpectedEndOfInput,
    InvalidChainAction,
}

impl Set {
    /// Parse a user input binding and add it to the set. This will handle
    /// the "unbind" case, ensure consumed/unconsumed fields are set correctly,
    /// handle sequences, etc.
    ///
    /// If this returns an OutOfMemory error then the set is in a broken
    /// state and should not be used again. Any Error returned is validated
    /// before any set modifications are made.
    pub fn parse_and_put(&mut self, input: &[u8]) -> Result<(), ParseAndPutError> {
        // To make cleanup easier, we ensure that the full sequence is
        // valid before making any set modifications. This is more expensive
        // computationally but it makes cleanup way, way easier.
        let mut it = Parser::init(input)?;
        while it.next()?.is_some() {}
        it.reset();

        // We use recursion so that we can utilize the stack as our state
        // for cleanup.
        let mut path: Vec<Trigger> = Vec::new();
        let updated_set_ = match self.parse_and_put_recurse(&mut path, &mut it) {
            Ok(v) => v,
            Err(err) => {
                match err {
                    // If this gets sent up to the root then we've unbound
                    // all the way up and this put was a success.
                    RecurseError::SequenceUnbind => None,

                    // If our parser input was too short then the format
                    // is invalid because we handle all valid cases.
                    RecurseError::UnexpectedEndOfInput => {
                        return Err(ParseAndPutError::InvalidFormat)
                    }

                    // If we had a chain without a parent then the format is wrong.
                    RecurseError::NoChainParent => return Err(ParseAndPutError::InvalidFormat),

                    // If we had an invalid action for a chain (e.g. unbind).
                    RecurseError::InvalidChainAction => {
                        return Err(ParseAndPutError::InvalidFormat)
                    }
                }

                // Errors must never fall through.
                // unreachable;
                //
                // PORT NOTE: the reference's `OutOfMemory` arm (`return
                // error.OutOfMemory`) has no Rust counterpart — the recursion is
                // infallible w.r.t. allocation — so the match above is exhaustive
                // and there is no fall-through `unreachable` to reach.
            }
        };

        // If we have an updated set (a binding was added) then we store
        // it for our chain parent. If we didn't update a set then we clear
        // our chain parent since chaining is no longer valid until a
        // valid binding is saved.
        match updated_set_ {
            Some(updated_path) => {
                // A successful addition must have recorded a chain parent on the
                // updated set; rebuild the root chain_parent with the FULL path.
                let updated_set = self.resolve_set_mut(&updated_path);
                let cp = updated_set
                    .chain_parent
                    .clone()
                    .expect("updated set recorded a chain parent");
                // The updated set's chain_parent path is relative to that set;
                // prepend the path to the updated set to make it root-relative.
                let mut full_path = updated_path.clone();
                full_path.extend(cp.set_path.iter().copied());
                self.chain_parent = Some(ChainParent {
                    set_path: full_path,
                    key: cp.key,
                });
                debug_assert!(self.chain_parent.is_some());
            }
            None => {
                self.chain_parent = None;
            }
        }

        Ok(())
    }

    /// Returns the set that was ultimately updated if a binding was
    /// added. Unbind does not return a set since nothing was added.
    //
    // PORT NOTE: the reference threads `root: *Set` and `set: *Set`. Here the
    // method runs on `self` (the root) and `path` identifies the CURRENT set
    // (chain of leader triggers from root). The returned `Option<Vec<Trigger>>` is
    // the path to the updated set (the reference's returned `?*Set`).
    fn parse_and_put_recurse(
        &mut self,
        path: &mut Vec<Trigger>,
        it: &mut Parser,
    ) -> Result<Option<Vec<Trigger>>, RecurseError> {
        let elem = match it.next() {
            // `it.next() catch unreachable`
            Ok(v) => v,
            Err(_) => unreachable!(),
        }
        .ok_or(RecurseError::UnexpectedEndOfInput)?;

        match elem {
            Elem::Leader(t) => {
                // If we have a leader, we need to upsert a set for it.
                // Since we remove the value, we need to copy it.
                let cur = self.resolve_set_mut(path);
                let old: Option<Value> = cur.bindings.get(&t).cloned();
                if let Some(entry) = &old {
                    match entry {
                        // We have an existing leader for this key already
                        // so recurse into this set.
                        Value::Leader(_) => {
                            path.push(t);
                            let r = self.parse_and_put_recurse(path, it);
                            path.pop();
                            return match r {
                                // Our child put unbound. If our set is empty we
                                // need to dealloc and continue up. If our set is
                                // not empty then we're done.
                                Err(RecurseError::SequenceUnbind) => {
                                    let child_empty = {
                                        let cur = self.resolve_set_mut(path);
                                        let idx = cur.bindings.get_index(&t).unwrap();
                                        match cur.bindings.value_at(idx) {
                                            Value::Leader(s) => s.bindings.count() == 0,
                                            _ => unreachable!(),
                                        }
                                    };
                                    if child_empty {
                                        let cur = self.resolve_set_mut(path);
                                        cur.remove(t);
                                        Err(RecurseError::SequenceUnbind)
                                    } else {
                                        Ok(None)
                                    }
                                }
                                Err(e) => Err(e),
                                Ok(v) => Ok(v),
                            };
                        }

                        Value::Leaf(_) | Value::LeafChained(_) => {
                            // Remove the existing action. Fallthrough as if
                            // we don't have a leader.
                            let cur = self.resolve_set_mut(path);
                            cur.remove(t);
                        }
                    }
                }

                // Create our new set for this leader
                // Insert the leader entry
                {
                    let cur = self.resolve_set_mut(path);
                    cur.bindings.put(t, Value::Leader(Box::<Set>::default()));
                }

                // Recurse
                path.push(t);
                let r = self.parse_and_put_recurse(path, it);
                path.pop();
                match r {
                    // If our action was to unbind, we restore the old
                    // action if we have it.
                    Err(RecurseError::SequenceUnbind) => {
                        let cur = self.resolve_set_mut(path);
                        cur.remove(t);
                        if let Some(entry) = old {
                            match entry {
                                Value::Leader(_) => unreachable!(), // Handled above
                                Value::Leaf(leaf) => {
                                    let cur = self.resolve_set_mut(path);
                                    // set.putFlags(...) catch {}
                                    cur.put_flags(t, leaf.action, leaf.flags);
                                }
                                Value::LeafChained(leaf) => 'chain: {
                                    // Rebuild our chain
                                    {
                                        let cur = self.resolve_set_mut(path);
                                        cur.put_flags(t, leaf.actions[0].clone(), leaf.flags);
                                    }
                                    for action in &leaf.actions[1..] {
                                        let cur = self.resolve_set_mut(path);
                                        match cur.append_chain(action.clone()) {
                                            Ok(()) => {}
                                            Err(_) => {
                                                let cur = self.resolve_set_mut(path);
                                                cur.remove(t);
                                                break 'chain;
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        Ok(None)
                    }

                    Err(e) => Err(e),
                    Ok(v) => Ok(v),
                }
            }

            Elem::Binding(b) => match b.action {
                Action::Unbind => {
                    let cur = self.resolve_set_mut(path);
                    cur.remove(b.trigger);
                    Err(RecurseError::SequenceUnbind)
                }

                _ => {
                    let cur = self.resolve_set_mut(path);
                    cur.put_flags(b.trigger, b.action, b.flags);
                    Ok(Some(path.clone()))
                }
            },

            Elem::Chain(action) => {
                // Chains can only happen on the root.
                debug_assert!(path.is_empty());
                // Unbind is not valid for chains.
                if action == Action::Unbind {
                    return Err(RecurseError::InvalidChainAction);
                }
                let cur = self.resolve_set_mut(path);
                match cur.append_chain(action) {
                    Ok(()) => Ok(Some(path.clone())),
                    Err(AppendChainError::NoChainParent) => Err(RecurseError::NoChainParent),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustty_cfg_fmt::{entry_formatter, AllocatingWriter};

    // PORT NOTE: convenience constructors mirroring the Zig struct literals used
    // pervasively in the tests (`.{ .key = .{ .unicode = 'a' } }` etc.).
    fn uni(c: u32) -> TriggerKey {
        TriggerKey::Unicode(c)
    }

    // rustty-only (R13, UPSTREAM-DEFECTS.md UD58): a `goto_tab` index that
    // `apprt.action.GotoTab = enum(c_int)` cannot hold is rejected by both action
    // parsers, with the error each returns for an out-of-range integer. The
    // largest index that fits is accepted. 4294967293..=4294967295 are the values
    // the reference's ReleaseFast truncates onto the last/next/previous sentinels.
    #[test]
    fn goto_tab_index_above_c_int_max_is_rejected_by_both_parsers() {
        assert_eq!(GOTO_TAB_MAX, i32::MAX as usize);
        let too_large = [
            "2147483648",
            "3000000000",
            "4294967293",
            "4294967294",
            "4294967295",
            "18446744073709551615",
        ];

        // `Action.parse` (keybind values).
        assert_eq!(
            Ok(Action::GotoTab(GOTO_TAB_MAX)),
            Action::parse(b"goto_tab:2147483647")
        );
        for v in too_large {
            assert_eq!(
                Err(Error::InvalidFormat),
                Action::parse(format!("goto_tab:{v}").as_bytes()),
                "keybind goto_tab:{v}"
            );
        }

        // The tagged-union parser (`command-palette-entry` actions).
        assert_eq!(
            Ok(Action::GotoTab(GOTO_TAB_MAX)),
            Action::parse_auto_tagged_union(b"goto_tab:2147483647")
        );
        for v in too_large {
            assert_eq!(
                Err(rustty_args::args::Error::InvalidValue),
                Action::parse_auto_tagged_union(format!("goto_tab:{v}").as_bytes()),
                "tagged union goto_tab:{v}"
            );
        }
    }

    // test "cval"  (nested in `Flags`; the fn carries the reference name)
    #[test]
    fn cval() {
        assert_eq!(0b0001u8, Flags::default().cval());
        assert_eq!(
            0b0000u8,
            Flags {
                consumed: false,
                ..Flags::default()
            }
            .cval()
        );
        assert_eq!(
            0b0011u8,
            Flags {
                all: true,
                ..Flags::default()
            }
            .cval()
        );
        assert_eq!(
            0b0101u8,
            Flags {
                global: true,
                ..Flags::default()
            }
            .cval()
        );
        assert_eq!(
            0b1001u8,
            Flags {
                performable: true,
                ..Flags::default()
            }
            .cval()
        );
        assert_eq!(
            0b1111u8,
            Flags {
                consumed: true,
                all: true,
                global: true,
                performable: true,
            }
            .cval()
        );
    }

    // test "parse"  (SplitFocusDirection)
    #[test]
    fn split_focus_direction_parse() {
        assert_eq!(
            SplitFocusDirection::Previous,
            SplitFocusDirection::parse(b"previous").unwrap()
        );
        assert_eq!(
            SplitFocusDirection::Next,
            SplitFocusDirection::parse(b"next").unwrap()
        );

        assert_eq!(
            SplitFocusDirection::Up,
            SplitFocusDirection::parse(b"up").unwrap()
        );
        assert_eq!(
            SplitFocusDirection::Left,
            SplitFocusDirection::parse(b"left").unwrap()
        );
        assert_eq!(
            SplitFocusDirection::Down,
            SplitFocusDirection::parse(b"down").unwrap()
        );
        assert_eq!(
            SplitFocusDirection::Right,
            SplitFocusDirection::parse(b"right").unwrap()
        );

        assert_eq!(
            SplitFocusDirection::Up,
            SplitFocusDirection::parse(b"top").unwrap()
        );
        assert_eq!(
            SplitFocusDirection::Down,
            SplitFocusDirection::parse(b"bottom").unwrap()
        );

        assert_eq!(Err(Error::InvalidFormat), SplitFocusDirection::parse(b""));
        assert_eq!(
            Err(Error::InvalidFormat),
            SplitFocusDirection::parse(b"green")
        );
    }

    // test "parse: triggers"
    #[test]
    fn parse_triggers() {
        // single character
        assert_eq!(
            Binding {
                trigger: Trigger {
                    key: uni(b'a' as u32),
                    ..Default::default()
                },
                action: Action::Ignore,
                flags: Flags::default(),
            },
            parse_single(b"a=ignore").unwrap()
        );

        // single modifier
        assert_eq!(
            Binding {
                trigger: Trigger {
                    mods: Mods {
                        shift: true,
                        ..Default::default()
                    },
                    key: uni(b'a' as u32),
                },
                action: Action::Ignore,
                flags: Flags::default(),
            },
            parse_single(b"shift+a=ignore").unwrap()
        );
        assert_eq!(
            Binding {
                trigger: Trigger {
                    mods: Mods {
                        ctrl: true,
                        ..Default::default()
                    },
                    key: uni(b'a' as u32),
                },
                action: Action::Ignore,
                flags: Flags::default(),
            },
            parse_single(b"ctrl+a=ignore").unwrap()
        );

        // multiple modifier
        assert_eq!(
            Binding {
                trigger: Trigger {
                    mods: Mods {
                        shift: true,
                        ctrl: true,
                        ..Default::default()
                    },
                    key: uni(b'a' as u32),
                },
                action: Action::Ignore,
                flags: Flags::default(),
            },
            parse_single(b"shift+ctrl+a=ignore").unwrap()
        );

        // key can come before modifier
        assert_eq!(
            Binding {
                trigger: Trigger {
                    mods: Mods {
                        shift: true,
                        ..Default::default()
                    },
                    key: uni(b'a' as u32),
                },
                action: Action::Ignore,
                flags: Flags::default(),
            },
            parse_single(b"a+shift=ignore").unwrap()
        );

        // physical keys
        assert_eq!(
            Binding {
                trigger: Trigger {
                    mods: Mods {
                        shift: true,
                        ..Default::default()
                    },
                    key: TriggerKey::Physical(KeyKey::KeyA),
                },
                action: Action::Ignore,
                flags: Flags::default(),
            },
            parse_single(b"shift+key_a=ignore").unwrap()
        );

        // unicode keys
        assert_eq!(
            Binding {
                trigger: Trigger {
                    mods: Mods {
                        shift: true,
                        ..Default::default()
                    },
                    key: uni('ö' as u32),
                },
                action: Action::Ignore,
                flags: Flags::default(),
            },
            parse_single("shift+ö=ignore".as_bytes()).unwrap()
        );

        // unconsumed keys
        assert_eq!(
            Binding {
                trigger: Trigger {
                    mods: Mods {
                        shift: true,
                        ..Default::default()
                    },
                    key: uni(b'a' as u32),
                },
                action: Action::Ignore,
                flags: Flags {
                    consumed: false,
                    ..Default::default()
                },
            },
            parse_single(b"unconsumed:shift+a=ignore").unwrap()
        );

        // unconsumed physical keys
        assert_eq!(
            Binding {
                trigger: Trigger {
                    mods: Mods {
                        shift: true,
                        ..Default::default()
                    },
                    key: TriggerKey::Physical(KeyKey::KeyA),
                },
                action: Action::Ignore,
                flags: Flags {
                    consumed: false,
                    ..Default::default()
                },
            },
            parse_single(b"unconsumed:key_a+shift=ignore").unwrap()
        );

        // performable keys
        assert_eq!(
            Binding {
                trigger: Trigger {
                    mods: Mods {
                        shift: true,
                        ..Default::default()
                    },
                    key: uni(b'a' as u32),
                },
                action: Action::Ignore,
                flags: Flags {
                    performable: true,
                    ..Default::default()
                },
            },
            parse_single(b"performable:shift+a=ignore").unwrap()
        );

        // invalid key
        assert_eq!(
            Err(ParseSingleError::InvalidFormat),
            parse_single(b"foo=ignore")
        );

        // repeated control
        assert_eq!(
            Err(ParseSingleError::InvalidFormat),
            parse_single(b"shift+shift+a=ignore")
        );

        // multiple character
        assert_eq!(
            Err(ParseSingleError::InvalidFormat),
            parse_single(b"a+b=ignore")
        );
    }

    // test "parse: w3c key names"
    #[test]
    fn parse_w3c_key_names() {
        // Exact match
        assert_eq!(
            Binding {
                trigger: Trigger {
                    key: TriggerKey::Physical(KeyKey::KeyA),
                    ..Default::default()
                },
                action: Action::Ignore,
                flags: Flags::default(),
            },
            parse_single(b"KeyA=ignore").unwrap()
        );

        // Case-sensitive
        assert_eq!(
            Err(ParseSingleError::InvalidFormat),
            parse_single(b"Keya=ignore")
        );
    }

    // test "parse: catch_all"
    #[test]
    fn parse_catch_all() {
        // Basic catch_all
        assert_eq!(
            Binding {
                trigger: Trigger {
                    key: TriggerKey::CatchAll,
                    ..Default::default()
                },
                action: Action::Ignore,
                flags: Flags::default(),
            },
            parse_single(b"catch_all=ignore").unwrap()
        );

        // catch_all with modifiers
        assert_eq!(
            Binding {
                trigger: Trigger {
                    mods: Mods {
                        ctrl: true,
                        ..Default::default()
                    },
                    key: TriggerKey::CatchAll,
                },
                action: Action::Ignore,
                flags: Flags::default(),
            },
            parse_single(b"ctrl+catch_all=ignore").unwrap()
        );
    }

    // test "parse: plus sign"
    #[test]
    fn parse_plus_sign() {
        assert_eq!(
            Binding {
                trigger: Trigger {
                    key: uni(b'+' as u32),
                    ..Default::default()
                },
                action: Action::Ignore,
                flags: Flags::default(),
            },
            parse_single(b"+=ignore").unwrap()
        );

        // Modifier
        assert_eq!(
            Binding {
                trigger: Trigger {
                    key: uni(b'+' as u32),
                    mods: Mods {
                        ctrl: true,
                        ..Default::default()
                    },
                },
                action: Action::Ignore,
                flags: Flags::default(),
            },
            parse_single(b"ctrl++=ignore").unwrap()
        );

        assert_eq!(
            Err(ParseSingleError::InvalidFormat),
            parse_single(b"++=ignore")
        );
    }

    // test "parse: equals sign"
    #[test]
    fn parse_equals_sign() {
        assert_eq!(
            Binding {
                trigger: Trigger {
                    key: uni(b'=' as u32),
                    ..Default::default()
                },
                action: Action::Ignore,
                flags: Flags::default(),
            },
            parse_single(b"==ignore").unwrap()
        );

        // Modifier
        assert_eq!(
            Binding {
                trigger: Trigger {
                    key: uni(b'=' as u32),
                    mods: Mods {
                        ctrl: true,
                        ..Default::default()
                    },
                },
                action: Action::Ignore,
                flags: Flags::default(),
            },
            parse_single(b"ctrl+==ignore").unwrap()
        );

        assert_eq!(
            Err(ParseSingleError::InvalidFormat),
            parse_single(b"=ignore")
        );
    }

    // test "parse: text action equals sign"
    #[test]
    fn parse_text_action_equals_sign() {
        {
            let binding = parse_single(b"==text:=").unwrap();
            assert_eq!(
                Trigger {
                    key: uni(b'=' as u32),
                    ..Default::default()
                },
                binding.trigger
            );
            assert_eq!(b"=", matches_text(&binding.action));
        }

        {
            let binding = parse_single(b"==text:=hello").unwrap();
            assert_eq!(
                Trigger {
                    key: uni(b'=' as u32),
                    ..Default::default()
                },
                binding.trigger
            );
            assert_eq!(b"=hello", matches_text(&binding.action));
        }

        {
            let binding = parse_single(b"ctrl+==text:=hello").unwrap();
            assert_eq!(
                Trigger {
                    key: uni(b'=' as u32),
                    mods: Mods {
                        ctrl: true,
                        ..Default::default()
                    },
                },
                binding.trigger
            );
            assert_eq!(b"=hello", matches_text(&binding.action));
        }

        {
            let binding = parse_single(b"=+ctrl=text:=hello").unwrap();
            assert_eq!(
                Trigger {
                    key: uni(b'=' as u32),
                    mods: Mods {
                        ctrl: true,
                        ..Default::default()
                    },
                },
                binding.trigger
            );
            assert_eq!(b"=hello", matches_text(&binding.action));
        }
    }

    // PORT NOTE: helper for `binding.action.text` — extracts the `text` payload.
    fn matches_text(a: &Action) -> &[u8] {
        match a {
            Action::Text(v) => v,
            _ => panic!("expected text action"),
        }
    }

    // For Rustty 1.2+ we changed our key names to match the W3C and removed
    // `physical:`. This tests the backwards compatibility with the old format.
    // Note that our backwards compatibility isn't 100% perfect since triggers
    // like `a` now map to unicode instead of "translated" (which was also
    // removed). But we did our best here with what was unambiguous.
    // test "parse: backwards compatibility with <= 1.1.x"
    #[test]
    fn parse_backwards_compatibility_with_1_1_x() {
        // simple, for sanity
        assert_eq!(
            Binding {
                trigger: Trigger {
                    key: uni(b'0' as u32),
                    ..Default::default()
                },
                action: Action::Ignore,
                flags: Flags::default(),
            },
            parse_single(b"zero=ignore").unwrap()
        );
        assert_eq!(
            Binding {
                trigger: Trigger {
                    key: TriggerKey::Physical(KeyKey::Digit0),
                    ..Default::default()
                },
                action: Action::Ignore,
                flags: Flags::default(),
            },
            parse_single(b"physical:zero=ignore").unwrap()
        );

        // duplicates
        assert_eq!(
            Err(ParseSingleError::InvalidFormat),
            parse_single(b"zero+one=ignore")
        );

        // test our full map
        for (k, v) in BACKWARDS_COMPATIBLE_KEYS {
            // try std.fmt.bufPrint(&buf, "{s}=ignore", .{k})
            let mut input = k.to_vec();
            input.extend_from_slice(b"=ignore");
            assert_eq!(
                Binding {
                    trigger: Trigger {
                        key: *v,
                        ..Default::default()
                    },
                    action: Action::Ignore,
                    flags: Flags::default(),
                },
                parse_single(&input).unwrap()
            );
        }
    }

    // test "parse: global triggers"
    #[test]
    fn parse_global_triggers() {
        // global keys
        assert_eq!(
            Binding {
                trigger: Trigger {
                    mods: Mods {
                        shift: true,
                        ..Default::default()
                    },
                    key: uni(b'a' as u32),
                },
                action: Action::Ignore,
                flags: Flags {
                    global: true,
                    ..Default::default()
                },
            },
            parse_single(b"global:shift+a=ignore").unwrap()
        );

        // global physical keys
        assert_eq!(
            Binding {
                trigger: Trigger {
                    mods: Mods {
                        shift: true,
                        ..Default::default()
                    },
                    key: TriggerKey::Physical(KeyKey::KeyA),
                },
                action: Action::Ignore,
                flags: Flags {
                    global: true,
                    ..Default::default()
                },
            },
            parse_single(b"global:key_a+shift=ignore").unwrap()
        );

        // global unconsumed keys
        assert_eq!(
            Binding {
                trigger: Trigger {
                    mods: Mods {
                        shift: true,
                        ..Default::default()
                    },
                    key: uni(b'a' as u32),
                },
                action: Action::Ignore,
                flags: Flags {
                    global: true,
                    consumed: false,
                    ..Default::default()
                },
            },
            parse_single(b"unconsumed:global:a+shift=ignore").unwrap()
        );

        // global sequences not allowed
        {
            let mut p = Parser::init(b"global:a>b=ignore").unwrap();
            assert_eq!(Err(Error::InvalidFormat), p.next());
        }
    }

    // test "parse: all triggers"
    #[test]
    fn parse_all_triggers() {
        // all keys
        assert_eq!(
            Binding {
                trigger: Trigger {
                    mods: Mods {
                        shift: true,
                        ..Default::default()
                    },
                    key: uni(b'a' as u32),
                },
                action: Action::Ignore,
                flags: Flags {
                    all: true,
                    ..Default::default()
                },
            },
            parse_single(b"all:shift+a=ignore").unwrap()
        );

        // all physical keys
        assert_eq!(
            Binding {
                trigger: Trigger {
                    mods: Mods {
                        shift: true,
                        ..Default::default()
                    },
                    key: TriggerKey::Physical(KeyKey::KeyA),
                },
                action: Action::Ignore,
                flags: Flags {
                    all: true,
                    ..Default::default()
                },
            },
            parse_single(b"all:key_a+shift=ignore").unwrap()
        );

        // all unconsumed keys
        assert_eq!(
            Binding {
                trigger: Trigger {
                    mods: Mods {
                        shift: true,
                        ..Default::default()
                    },
                    key: uni(b'a' as u32),
                },
                action: Action::Ignore,
                flags: Flags {
                    all: true,
                    consumed: false,
                    ..Default::default()
                },
            },
            parse_single(b"unconsumed:all:a+shift=ignore").unwrap()
        );

        // all sequences not allowed
        {
            let mut p = Parser::init(b"all:a>b=ignore").unwrap();
            assert_eq!(Err(Error::InvalidFormat), p.next());
        }
    }

    /// S65-F1: `foldedCodepoint`'s Unicode slow path is
    /// `uucode.get(.case_folding_full, cp)` (`Binding.zig:2003-2011`), so two
    /// triggers differing only in the case of a non-ASCII letter hash and
    /// compare equal, and a multi-code-point folding fills more than one slot.
    #[test]
    fn trigger_folds_non_ascii_case() {
        // U+00C9 É folds to U+00E9 é.
        assert_eq!(folded_codepoint(0x00C9), [0x00E9, 0, 0]);
        // U+00DF ß folds to "ss", and so does U+1E9E ẞ.
        assert_eq!(folded_codepoint(0x00DF), [0x73, 0x73, 0]);
        assert_eq!(folded_codepoint(0x1E9E), [0x73, 0x73, 0]);
        // No folding: the code point itself.
        assert_eq!(folded_codepoint(0x00E9), [0x00E9, 0, 0]);

        let upper = Trigger {
            key: TriggerKey::Unicode(0x00C9),
            mods: Mods::default(),
        };
        let lower = Trigger {
            key: TriggerKey::Unicode(0x00E9),
            mods: Mods::default(),
        };
        // `foldedEqual` is the binding set's equality (`bindingSetEqual`);
        // plain `equal` stays exact.
        assert!(upper.folded_equal(lower));
        assert!(!upper.equal(lower));
        assert_eq!(upper.hash(), lower.hash());
        // A different letter still differs.
        let other = Trigger {
            key: TriggerKey::Unicode(0x00E8),
            mods: Mods::default(),
        };
        assert!(!upper.folded_equal(other));
    }

    // test "Trigger: equal"
    #[test]
    fn trigger_equal() {
        // Equal physical keys
        {
            let t1 = Trigger {
                key: TriggerKey::Physical(KeyKey::ArrowUp),
                mods: Mods {
                    ctrl: true,
                    ..Default::default()
                },
            };
            let t2 = Trigger {
                key: TriggerKey::Physical(KeyKey::ArrowUp),
                mods: Mods {
                    ctrl: true,
                    ..Default::default()
                },
            };
            assert!(t1.equal(t2));
        }

        // Different physical keys
        {
            let t1 = Trigger {
                key: TriggerKey::Physical(KeyKey::ArrowUp),
                mods: Mods {
                    ctrl: true,
                    ..Default::default()
                },
            };
            let t2 = Trigger {
                key: TriggerKey::Physical(KeyKey::ArrowDown),
                mods: Mods {
                    ctrl: true,
                    ..Default::default()
                },
            };
            assert!(!t1.equal(t2));
        }

        // Different mods
        {
            let t1 = Trigger {
                key: TriggerKey::Physical(KeyKey::ArrowUp),
                mods: Mods {
                    ctrl: true,
                    ..Default::default()
                },
            };
            let t2 = Trigger {
                key: TriggerKey::Physical(KeyKey::ArrowUp),
                mods: Mods {
                    shift: true,
                    ..Default::default()
                },
            };
            assert!(!t1.equal(t2));
        }

        // Equal unicode keys
        {
            let t1 = Trigger {
                key: uni(b'a' as u32),
                mods: Mods::default(),
            };
            let t2 = Trigger {
                key: uni(b'a' as u32),
                mods: Mods::default(),
            };
            assert!(t1.equal(t2));
        }

        // Different unicode keys
        {
            let t1 = Trigger {
                key: uni(b'a' as u32),
                mods: Mods::default(),
            };
            let t2 = Trigger {
                key: uni(b'b' as u32),
                mods: Mods::default(),
            };
            assert!(!t1.equal(t2));
        }

        // Different key types
        {
            let t1 = Trigger {
                key: uni(b'a' as u32),
                mods: Mods::default(),
            };
            let t2 = Trigger {
                key: TriggerKey::Physical(KeyKey::KeyA),
                mods: Mods::default(),
            };
            assert!(!t1.equal(t2));
        }

        // catch_all
        {
            let t1 = Trigger {
                key: TriggerKey::CatchAll,
                mods: Mods::default(),
            };
            let t2 = Trigger {
                key: TriggerKey::CatchAll,
                mods: Mods::default(),
            };
            assert!(t1.equal(t2));
        }

        // catch_all with different mods
        {
            let t1 = Trigger {
                key: TriggerKey::CatchAll,
                mods: Mods::default(),
            };
            let t2 = Trigger {
                key: TriggerKey::CatchAll,
                mods: Mods {
                    alt: true,
                    ..Default::default()
                },
            };
            assert!(!t1.equal(t2));
        }
    }

    // test "parse: modifier aliases"
    #[test]
    fn parse_modifier_aliases() {
        assert_eq!(
            Binding {
                trigger: Trigger {
                    mods: Mods {
                        super_: true,
                        ..Default::default()
                    },
                    key: uni(b'a' as u32)
                },
                action: Action::Ignore,
                flags: Flags::default(),
            },
            parse_single(b"cmd+a=ignore").unwrap()
        );
        assert_eq!(
            Binding {
                trigger: Trigger {
                    mods: Mods {
                        super_: true,
                        ..Default::default()
                    },
                    key: uni(b'a' as u32)
                },
                action: Action::Ignore,
                flags: Flags::default(),
            },
            parse_single(b"command+a=ignore").unwrap()
        );

        assert_eq!(
            Binding {
                trigger: Trigger {
                    mods: Mods {
                        alt: true,
                        ..Default::default()
                    },
                    key: uni(b'a' as u32)
                },
                action: Action::Ignore,
                flags: Flags::default(),
            },
            parse_single(b"opt+a=ignore").unwrap()
        );
        assert_eq!(
            Binding {
                trigger: Trigger {
                    mods: Mods {
                        alt: true,
                        ..Default::default()
                    },
                    key: uni(b'a' as u32)
                },
                action: Action::Ignore,
                flags: Flags::default(),
            },
            parse_single(b"option+a=ignore").unwrap()
        );

        assert_eq!(
            Binding {
                trigger: Trigger {
                    mods: Mods {
                        ctrl: true,
                        ..Default::default()
                    },
                    key: uni(b'a' as u32)
                },
                action: Action::Ignore,
                flags: Flags::default(),
            },
            parse_single(b"control+a=ignore").unwrap()
        );
    }

    // test "parse: action invalid"
    #[test]
    fn parse_action_invalid() {
        // invalid action
        assert_eq!(
            Err(ParseSingleError::InvalidAction),
            parse_single(b"a=nopenopenope")
        );
    }

    // test "parse: action no parameters"
    #[test]
    fn parse_action_no_parameters() {
        // no parameters
        assert_eq!(
            Binding {
                trigger: Trigger {
                    key: uni(b'a' as u32),
                    ..Default::default()
                },
                action: Action::Ignore,
                flags: Flags::default(),
            },
            parse_single(b"a=ignore").unwrap()
        );
        assert_eq!(
            Err(ParseSingleError::InvalidFormat),
            parse_single(b"a=ignore:A")
        );
    }

    // test "parse: action with string"
    #[test]
    fn parse_action_with_string() {
        // parameter
        {
            let binding = parse_single(b"a=csi:A").unwrap();
            assert!(matches!(binding.action, Action::Csi(_)));
            if let Action::Csi(v) = &binding.action {
                assert_eq!(b"A", v.as_slice());
            }
        }
        // parameter
        {
            let binding = parse_single(b"a=esc:A").unwrap();
            assert!(matches!(binding.action, Action::Esc(_)));
            if let Action::Esc(v) = &binding.action {
                assert_eq!(b"A", v.as_slice());
            }
        }
        {
            let binding = parse_single(b"a=set_surface_title:surface").unwrap();
            assert!(matches!(binding.action, Action::SetSurfaceTitle(_)));
            if let Action::SetSurfaceTitle(v) = &binding.action {
                assert_eq!(b"surface", v.as_slice());
            }
        }
        {
            let binding = parse_single(b"a=set_tab_title:tab").unwrap();
            assert!(matches!(binding.action, Action::SetTabTitle(_)));
            if let Action::SetTabTitle(v) = &binding.action {
                assert_eq!(b"tab", v.as_slice());
            }
        }
    }

    // test "parse: action with enum"
    #[test]
    fn parse_action_with_enum() {
        // parameter
        {
            let binding = parse_single(b"a=new_split:right").unwrap();
            assert!(matches!(binding.action, Action::NewSplit(_)));
            if let Action::NewSplit(v) = binding.action {
                assert_eq!(SplitDirection::Right, v);
            }
        }
    }

    // test "parse: action with enum with default"
    #[test]
    fn parse_action_with_enum_with_default() {
        // parameter
        {
            let binding = parse_single(b"a=new_split").unwrap();
            assert!(matches!(binding.action, Action::NewSplit(_)));
            if let Action::NewSplit(v) = binding.action {
                assert_eq!(SplitDirection::Auto, v);
            }
        }
    }

    // test "parse: action with int"
    #[test]
    fn parse_action_with_int() {
        // parameter
        {
            let binding = parse_single(b"a=jump_to_prompt:-1").unwrap();
            assert!(matches!(binding.action, Action::JumpToPrompt(_)));
            if let Action::JumpToPrompt(v) = binding.action {
                assert_eq!(-1i16, v);
            }
        }
        {
            let binding = parse_single(b"a=jump_to_prompt:10").unwrap();
            assert!(matches!(binding.action, Action::JumpToPrompt(_)));
            if let Action::JumpToPrompt(v) = binding.action {
                assert_eq!(10i16, v);
            }
        }
    }

    // test "parse: action with float"
    #[test]
    fn parse_action_with_float() {
        // parameter
        {
            let binding = parse_single(b"a=scroll_page_fractional:-0.5").unwrap();
            assert!(matches!(binding.action, Action::ScrollPageFractional(_)));
            if let Action::ScrollPageFractional(v) = binding.action {
                assert_eq!(-0.5f32, v);
            }
        }
        {
            let binding = parse_single(b"a=scroll_page_fractional:+0.5").unwrap();
            assert!(matches!(binding.action, Action::ScrollPageFractional(_)));
            if let Action::ScrollPageFractional(v) = binding.action {
                assert_eq!(0.5f32, v);
            }
        }
    }

    /// S100-F1: the numeric action parameters go through Zig's
    /// `std.fmt.parseInt(T, value, 10)` / `std.fmt.parseFloat(T, value)`
    /// (`Binding.zig:1225-1231`), so they take `_` separators, `-0` for an
    /// unsigned type and hex floats, and reject what Zig rejects. Every
    /// integer type the actions use is covered (i16, isize, usize, u16) plus
    /// the f32 one.
    #[test]
    fn parse_action_numbers_take_the_zig_grammar() {
        let action = |s: &[u8]| parse_single(s).map(|b| b.action);

        assert!(matches!(action(b"a=goto_tab:1_0"), Ok(Action::GotoTab(10))));
        assert!(matches!(action(b"a=goto_tab:-0"), Ok(Action::GotoTab(0))));
        assert!(matches!(
            action(b"a=scroll_to_row:+1_000"),
            Ok(Action::ScrollToRow(1000))
        ));
        assert!(matches!(
            action(b"a=scroll_page_lines:-1_0"),
            Ok(Action::ScrollPageLines(-10))
        ));
        assert!(matches!(
            action(b"a=move_tab:-2_0"),
            Ok(Action::MoveTab(-20))
        ));
        match action(b"a=resize_split:up,1_0") {
            Ok(Action::ResizeSplit(v)) => assert_eq!(10u16, v.1),
            other => panic!("resize_split:up,1_0 parsed as {other:?}"),
        }
        assert!(
            matches!(action(b"a=set_font_size:0x1.8p3"), Ok(Action::SetFontSize(v)) if v == 12.0)
        );
        assert!(
            matches!(action(b"a=increase_font_size:1_0.5"), Ok(Action::IncreaseFontSize(v)) if v == 10.5)
        );

        for bad in [
            &b"a=goto_tab:_1"[..],
            b"a=goto_tab:1_",
            b"a=goto_tab:-1",
            b"a=set_font_size:1__0",
            b"a=set_font_size:0x",
        ] {
            assert_eq!(
                Err(ParseSingleError::InvalidFormat),
                parse_single(bad).map(|_| ()),
                "{:?}",
                String::from_utf8_lossy(bad)
            );
        }
    }

    // test "parse: action with a tuple"
    #[test]
    fn parse_action_with_a_tuple() {
        // parameter
        {
            let binding = parse_single(b"a=resize_split:up,10").unwrap();
            assert!(matches!(binding.action, Action::ResizeSplit(_)));
            if let Action::ResizeSplit(v) = binding.action {
                assert_eq!(SplitResizeDirection::Up, v.0);
                assert_eq!(10u16, v.1);
            }
        }

        // missing parameter
        assert_eq!(
            Err(ParseSingleError::InvalidFormat),
            parse_single(b"a=resize_split:up")
        );

        // too many
        assert_eq!(
            Err(ParseSingleError::InvalidFormat),
            parse_single(b"a=resize_split:up,10,12")
        );

        // invalid type
        assert_eq!(
            Err(ParseSingleError::InvalidFormat),
            parse_single(b"a=resize_split:up,four")
        );
    }

    // test "parse: chain"
    #[test]
    fn parse_chain() {
        // Valid
        {
            let mut p = Parser::init(b"chain=new_tab").unwrap();
            assert_eq!(Some(Elem::Chain(Action::NewTab)), p.next().unwrap());
            assert!(p.next().unwrap().is_none());
        }

        // Chain can't have flags
        assert_eq!(
            Error::InvalidFormat,
            Parser::init(b"global:chain=ignore").err().unwrap()
        );

        // Chain can't be part of a sequence
        {
            let mut p = Parser::init(b"a>chain=ignore").unwrap();
            // PORT NOTE: `_ = try p.next();` — the leader element must PARSE.
            // `let _ = p.next()` discarded an `Err`, so the block passed even
            // when the leader itself failed to parse and the `InvalidFormat`
            // below came from the wrong element (S102-F1).
            p.next().unwrap();
            assert_eq!(Err(Error::InvalidFormat), p.next());
        }
    }

    // test "sequence iterator"
    #[test]
    fn sequence_iterator() {
        // single character
        {
            let mut it = SequenceIterator {
                input: b"a".to_vec(),
                i: 0,
            };
            assert_eq!(
                Trigger {
                    key: uni(b'a' as u32),
                    ..Default::default()
                },
                it.next().unwrap().unwrap()
            );
            assert!(it.next().unwrap().is_none());
        }

        // multi character
        {
            let mut it = SequenceIterator {
                input: b"a>b".to_vec(),
                i: 0,
            };
            assert_eq!(
                Trigger {
                    key: uni(b'a' as u32),
                    ..Default::default()
                },
                it.next().unwrap().unwrap()
            );
            assert_eq!(
                Trigger {
                    key: uni(b'b' as u32),
                    ..Default::default()
                },
                it.next().unwrap().unwrap()
            );
            assert!(it.next().unwrap().is_none());
        }

        // empty
        {
            let mut it = SequenceIterator {
                input: b"".to_vec(),
                i: 0,
            };
            assert_eq!(Err(Error::InvalidFormat), it.next());
        }

        // empty starting sequence
        {
            let mut it = SequenceIterator {
                input: b">a".to_vec(),
                i: 0,
            };
            assert_eq!(Err(Error::InvalidFormat), it.next());
        }

        // empty ending sequence
        {
            let mut it = SequenceIterator {
                input: b"a>".to_vec(),
                i: 0,
            };
            assert_eq!(
                Trigger {
                    key: uni(b'a' as u32),
                    ..Default::default()
                },
                it.next().unwrap().unwrap()
            );
            assert_eq!(Err(Error::InvalidFormat), it.next());
        }
    }

    // test "parse: sequences"
    #[test]
    fn parse_sequences() {
        // single character
        {
            let mut p = Parser::init(b"ctrl+a=ignore").unwrap();
            assert_eq!(
                Some(Elem::Binding(Binding {
                    trigger: Trigger {
                        mods: Mods {
                            ctrl: true,
                            ..Default::default()
                        },
                        key: uni(b'a' as u32),
                    },
                    action: Action::Ignore,
                    flags: Flags::default(),
                })),
                p.next().unwrap()
            );
            assert!(p.next().unwrap().is_none());
        }

        // sequence
        {
            let mut p = Parser::init(b"a>b=ignore").unwrap();
            assert_eq!(
                Some(Elem::Leader(Trigger {
                    key: uni(b'a' as u32),
                    ..Default::default()
                })),
                p.next().unwrap()
            );
            assert_eq!(
                Some(Elem::Binding(Binding {
                    trigger: Trigger {
                        key: uni(b'b' as u32),
                        ..Default::default()
                    },
                    action: Action::Ignore,
                    flags: Flags::default(),
                })),
                p.next().unwrap()
            );
            assert!(p.next().unwrap().is_none());
        }
    }

    // PORT NOTE: test helpers for the `Set` value accessors. The reference reads
    // `entry.value_ptr.*.leaf` / `.leaf_chained` directly; these mirror that.
    fn as_leaf(v: &Value) -> &Leaf {
        match v {
            Value::Leaf(l) => l,
            _ => panic!("expected leaf"),
        }
    }
    fn as_leaf_chained(v: &Value) -> &LeafChained {
        match v {
            Value::LeafChained(l) => l,
            _ => panic!("expected leaf_chained"),
        }
    }
    // `try s.chain_parent.?.key_ptr.format(&buf.writer); expectEqualStrings(...)`
    fn chain_parent_key_str(s: &Set) -> Vec<u8> {
        let mut buf = AllocatingWriter::init();
        s.chain_parent().unwrap().key.format(&mut buf).unwrap();
        buf.written().to_vec()
    }
    fn chain_parent_value(s: &Set) -> &Value {
        let cp = s.chain_parent().unwrap();
        let set = resolve_set_ref(s, &cp.set_path);
        set.get_value(cp.key).unwrap()
    }
    fn resolve_set_ref<'a>(s: &'a Set, path: &[Trigger]) -> &'a Set {
        let mut cur = s;
        for t in path {
            match cur.get_value(*t).unwrap() {
                Value::Leader(child) => cur = child.as_ref(),
                _ => unreachable!(),
            }
        }
        cur
    }

    // test "set: parseAndPut typical binding"
    #[test]
    fn set_parse_and_put_typical_binding() {
        let mut s = Set::default();

        s.parse_and_put(b"a=new_window").unwrap();

        // Creates forward mapping
        {
            let action = as_leaf(
                s.get_value(Trigger {
                    key: uni(b'a' as u32),
                    ..Default::default()
                })
                .unwrap(),
            );
            assert!(action.action == Action::NewWindow);
            assert_eq!(Flags::default(), action.flags);
        }

        // Creates reverse mapping
        {
            let trigger = s.get_trigger(&Action::NewWindow).unwrap();
            assert!(trigger.key == uni(b'a' as u32));
        }

        // Sets up the chain parent properly
        assert!(s.chain_parent().is_some());
        assert_eq!(b"a", chain_parent_key_str(&s).as_slice());
    }

    // test "set: parseAndPut unconsumed binding"
    #[test]
    fn set_parse_and_put_unconsumed_binding() {
        let mut s = Set::default();

        s.parse_and_put(b"unconsumed:a=new_window").unwrap();

        // Creates forward mapping
        {
            let trigger = Trigger {
                key: uni(b'a' as u32),
                ..Default::default()
            };
            let action = as_leaf(s.get_value(trigger).unwrap());
            assert!(action.action == Action::NewWindow);
            assert_eq!(
                Flags {
                    consumed: false,
                    ..Default::default()
                },
                action.flags
            );
        }

        // Creates reverse mapping
        {
            let trigger = s.get_trigger(&Action::NewWindow).unwrap();
            assert!(trigger.key == uni(b'a' as u32));
        }

        // Sets up the chain parent properly
        assert!(s.chain_parent().is_some());
        assert_eq!(b"a", chain_parent_key_str(&s).as_slice());
    }

    // test "set: parseAndPut removed binding"
    #[test]
    fn set_parse_and_put_removed_binding() {
        let mut s = Set::default();

        s.parse_and_put(b"a=new_window").unwrap();
        s.parse_and_put(b"a=unbind").unwrap();

        // Creates forward mapping
        {
            let trigger = Trigger {
                key: uni(b'a' as u32),
                ..Default::default()
            };
            assert!(s.get_value(trigger).is_none());
        }
        assert!(s.get_trigger(&Action::NewWindow).is_none());

        // Sets up the chain parent properly
        assert!(s.chain_parent().is_none());
    }

    // test "set: put sets chain_parent"
    #[test]
    fn set_put_sets_chain_parent() {
        let mut s = Set::default();

        s.put(
            Trigger {
                key: uni(b'a' as u32),
                ..Default::default()
            },
            Action::NewWindow,
        );

        // chain_parent should be set
        assert!(s.chain_parent().is_some());
        assert_eq!(b"a", chain_parent_key_str(&s).as_slice());

        // chain_parent value should be a leaf
        assert!(matches!(chain_parent_value(&s), Value::Leaf(_)));
    }

    // test "set: putFlags sets chain_parent"
    #[test]
    fn set_put_flags_sets_chain_parent() {
        let mut s = Set::default();

        s.put_flags(
            Trigger {
                key: uni(b'a' as u32),
                ..Default::default()
            },
            Action::NewWindow,
            Flags {
                consumed: false,
                ..Default::default()
            },
        );

        // chain_parent should be set
        assert!(s.chain_parent().is_some());
        assert_eq!(b"a", chain_parent_key_str(&s).as_slice());

        // chain_parent value should be a leaf with correct flags
        assert!(matches!(chain_parent_value(&s), Value::Leaf(_)));
        assert!(!as_leaf(chain_parent_value(&s)).flags.consumed);
    }

    // test "set: sequence sets chain_parent to final leaf"
    #[test]
    fn set_sequence_sets_chain_parent_to_final_leaf() {
        let mut s = Set::default();

        s.parse_and_put(b"a>b=new_window").unwrap();

        // chain_parent should be set and point to 'b' (the final leaf)
        assert!(s.chain_parent().is_some());
        assert_eq!(b"b", chain_parent_key_str(&s).as_slice());

        // chain_parent value should be a leaf
        assert!(matches!(chain_parent_value(&s), Value::Leaf(_)));
        assert!(as_leaf(chain_parent_value(&s)).action == Action::NewWindow);
    }

    // test "set: multiple leaves under leader updates chain_parent"
    #[test]
    fn set_multiple_leaves_under_leader_updates_chain_parent() {
        let mut s = Set::default();

        s.parse_and_put(b"a>b=new_window").unwrap();

        // After first binding, chain_parent should be 'b'
        assert!(s.chain_parent().is_some());
        assert_eq!(b"b", chain_parent_key_str(&s).as_slice());

        s.parse_and_put(b"a>c=new_tab").unwrap();

        // After second binding, chain_parent should be updated to 'c'
        assert!(s.chain_parent().is_some());
        assert_eq!(b"c", chain_parent_key_str(&s).as_slice());
        assert!(as_leaf(chain_parent_value(&s)).action == Action::NewTab);
    }

    // test "set: sequence unbind clears chain_parent"
    #[test]
    fn set_sequence_unbind_clears_chain_parent() {
        let mut s = Set::default();

        s.parse_and_put(b"a>b=new_window").unwrap();
        assert!(s.chain_parent().is_some());

        s.parse_and_put(b"a>b=unbind").unwrap();

        // After unbind, chain_parent should be cleared
        assert!(s.chain_parent().is_none());
    }

    // test "set: sequence unbind with remaining leaves clears chain_parent"
    #[test]
    fn set_sequence_unbind_with_remaining_leaves_clears_chain_parent() {
        let mut s = Set::default();

        s.parse_and_put(b"a>b=new_window").unwrap();
        s.parse_and_put(b"a>c=new_tab").unwrap();
        s.parse_and_put(b"a>b=unbind").unwrap();

        // After unbind, chain_parent should be cleared even though 'c' remains
        assert!(s.chain_parent().is_none());

        // But 'c' should still exist
        let a_entry = s
            .get_value(Trigger {
                key: uni(b'a' as u32),
                ..Default::default()
            })
            .unwrap();
        assert!(matches!(a_entry, Value::Leader(_)));
        if let Value::Leader(inner_set) = a_entry {
            assert!(inner_set
                .get_value(Trigger {
                    key: uni(b'c' as u32),
                    ..Default::default()
                })
                .is_some());
        }
    }

    // test "set: direct remove clears chain_parent"
    #[test]
    fn set_direct_remove_clears_chain_parent() {
        let mut s = Set::default();

        s.put(
            Trigger {
                key: uni(b'a' as u32),
                ..Default::default()
            },
            Action::NewWindow,
        );
        assert!(s.chain_parent().is_some());

        s.remove(Trigger {
            key: uni(b'a' as u32),
            ..Default::default()
        });

        // After removal, chain_parent should be cleared
        assert!(s.chain_parent().is_none());
    }

    // test "set: invalid format preserves chain_parent"
    #[test]
    fn set_invalid_format_preserves_chain_parent() {
        let mut s = Set::default();

        s.parse_and_put(b"a=new_window").unwrap();
        let before = s.chain_parent().unwrap().clone();
        // const before_key = s.chain_parent.?.key_ptr;
        // const before_value = s.chain_parent.?.value_ptr;
        //
        // PORT NOTE (S-8c): the Rust chain parent records the entry's trigger
        // rather than raw entry pointers, so the reference's two pointer
        // captures are the addresses of the entry's stored key and value,
        // resolved through that trigger.
        let before_key: *const Trigger = s.get_key(before.key).unwrap();
        let before_value: *const Value = s.get_value(before.key).unwrap();

        // Try an invalid parse - should fail
        assert_eq!(
            Err(ParseAndPutError::InvalidAction),
            s.parse_and_put(b"a=invalid_action_xyz")
        );

        // chain_parent should be unchanged
        assert!(s.chain_parent().is_some());
        assert_eq!(&before, s.chain_parent().unwrap());
        // try testing.expect(s.chain_parent.?.key_ptr == before_key);
        let after = s.chain_parent().unwrap().key;
        assert!(std::ptr::eq(s.get_key(after).unwrap(), before_key));
        // try testing.expect(s.chain_parent.?.value_ptr == before_value);
        assert!(std::ptr::eq(s.get_value(after).unwrap(), before_value));
    }

    // test "set: clone produces null chain_parent"
    #[test]
    fn set_clone_produces_null_chain_parent() {
        let mut s = Set::default();

        s.parse_and_put(b"a=new_window").unwrap();
        assert!(s.chain_parent().is_some());

        let cloned = s.clone_set();

        // Clone should have null chain_parent
        assert!(cloned.chain_parent().is_none());

        // But should have the binding
        assert!(cloned
            .get_value(Trigger {
                key: uni(b'a' as u32),
                ..Default::default()
            })
            .is_some());
    }

    // PORT NOTE: reference defect (R13), UPSTREAM-DEFECTS.md UD365. The
    // reference's `GenericLeaf.actions.many` is a slice INTO the set's
    // `leaf_chained` list, which lives in the surface's (or the embedded
    // app's) config arena. A chained `reload_config` replaces that config
    // synchronously on macOS (the action callback calls
    // `ghostty_app_update_config`), so `maybeHandleBinding`'s `for (actions)`
    // loop and its closing/ignore checks, and `App.keyEvent` ->
    // `performAllChainedAction`, then read freed memory. The port's
    // `GenericLeaf` owns a copy of the actions, so they outlive the set. This
    // test fails on the reference's shape: with a borrowed slice, `leaf` could
    // not outlive the set it borrows (`drop(s)` below would not compile), and
    // an unchecked borrow would be the use-after-free Miri reports.
    #[test]
    fn ud365_generic_leaf_actions_outlive_a_replaced_set() {
        let mut s = Set::default();
        s.parse_and_put(b"a=reload_config").unwrap();
        s.parse_and_put(b"chain=text:world").unwrap();
        let leaf = match s
            .get_value(Trigger {
                key: uni(b'a' as u32),
                ..Default::default()
            })
            .unwrap()
        {
            Value::LeafChained(l) => l.generic(),
            other => panic!("expected leaf_chained, got {other:?}"),
        };
        // The config reload: the set (and everything it owns) is replaced
        // while the chain is being performed.
        drop(std::mem::take(&mut s));
        s.parse_and_put(b"a=new_window").unwrap();

        let actions = leaf.actions_slice();
        assert_eq!(2, actions.len());
        assert!(actions[0] == Action::ReloadConfig);
        assert!(actions[1] == Action::Text(b"world".to_vec()));
    }

    // test "set: clone with leaf_chained"
    #[test]
    fn set_clone_with_leaf_chained() {
        let mut s = Set::default();

        // Create a chained binding using parseAndPut with chain=
        s.parse_and_put(b"a=new_window").unwrap();
        s.parse_and_put(b"chain=new_tab").unwrap();

        // Verify we have a leaf_chained
        let entry = s
            .get_value(Trigger {
                key: uni(b'a' as u32),
                ..Default::default()
            })
            .unwrap();
        assert!(matches!(entry, Value::LeafChained(_)));
        assert_eq!(2usize, as_leaf_chained(entry).actions.len());

        // Clone the set
        let cloned = s.clone_set();

        // Verify the cloned set has the leaf_chained with same actions
        let cloned_entry = cloned
            .get_value(Trigger {
                key: uni(b'a' as u32),
                ..Default::default()
            })
            .unwrap();
        assert!(matches!(cloned_entry, Value::LeafChained(_)));
        assert_eq!(2usize, as_leaf_chained(cloned_entry).actions.len());
        assert!(as_leaf_chained(cloned_entry).actions[0] == Action::NewWindow);
        assert!(as_leaf_chained(cloned_entry).actions[1] == Action::NewTab);
    }

    // test "set: clone with leaf_chained containing allocated data"
    #[test]
    fn set_clone_with_leaf_chained_containing_allocated_data() {
        let mut s = Set::default();

        // Create a chained binding with text actions (which have allocated strings)
        s.parse_and_put(b"a=text:hello").unwrap();
        s.parse_and_put(b"chain=text:world").unwrap();

        // Verify we have a leaf_chained
        let entry = s
            .get_value(Trigger {
                key: uni(b'a' as u32),
                ..Default::default()
            })
            .unwrap();
        assert!(matches!(entry, Value::LeafChained(_)));

        // Clone the set
        let cloned = s.clone_set();

        // Verify the cloned set has independent copies of the text
        let cloned_entry = cloned
            .get_value(Trigger {
                key: uni(b'a' as u32),
                ..Default::default()
            })
            .unwrap();
        assert!(matches!(cloned_entry, Value::LeafChained(_)));
        assert_eq!(b"hello", text_of(&as_leaf_chained(cloned_entry).actions[0]));
        assert_eq!(b"world", text_of(&as_leaf_chained(cloned_entry).actions[1]));

        // Verify the pointers are different (truly cloned, not shared)
        //
        // PORT NOTE (S-8c): ported as the reference's pointer comparison. It
        // used to be dropped as "guaranteed by the type" (a cloned `Vec<u8>`
        // always owns a fresh buffer); R1 keeps the assertion anyway.
        assert!(
            text_of(&as_leaf_chained(entry).actions[0]).as_ptr()
                != text_of(&as_leaf_chained(cloned_entry).actions[0]).as_ptr()
        );
    }

    fn text_of(a: &Action) -> &[u8] {
        match a {
            Action::Text(v) => v,
            _ => panic!("expected text"),
        }
    }

    // test "set: parseAndPut sequence"
    #[test]
    fn set_parse_and_put_sequence() {
        let mut s = Set::default();

        s.parse_and_put(b"a>b=new_window").unwrap();
        {
            let t = Trigger {
                key: uni(b'a' as u32),
                ..Default::default()
            };
            let e = s.get_value(t).unwrap();
            assert!(matches!(e, Value::Leader(_)));
            if let Value::Leader(inner) = e {
                let t = Trigger {
                    key: uni(b'b' as u32),
                    ..Default::default()
                };
                let e = inner.get_value(t).unwrap();
                assert!(matches!(e, Value::Leaf(_)));
                assert!(as_leaf(e).action == Action::NewWindow);
                assert_eq!(Flags::default(), as_leaf(e).flags);
            }
        }
    }

    // test "set: parseAndPut sequence with two actions"
    #[test]
    fn set_parse_and_put_sequence_with_two_actions() {
        let mut s = Set::default();

        s.parse_and_put(b"a>b=new_window").unwrap();
        s.parse_and_put(b"a>c=new_tab").unwrap();
        let e = s
            .get_value(Trigger {
                key: uni(b'a' as u32),
                ..Default::default()
            })
            .unwrap();
        assert!(matches!(e, Value::Leader(_)));
        if let Value::Leader(inner) = e {
            {
                let e = inner
                    .get_value(Trigger {
                        key: uni(b'b' as u32),
                        ..Default::default()
                    })
                    .unwrap();
                assert!(matches!(e, Value::Leaf(_)));
                assert!(as_leaf(e).action == Action::NewWindow);
                assert_eq!(Flags::default(), as_leaf(e).flags);
            }
            {
                let e = inner
                    .get_value(Trigger {
                        key: uni(b'c' as u32),
                        ..Default::default()
                    })
                    .unwrap();
                assert!(matches!(e, Value::Leaf(_)));
                assert!(as_leaf(e).action == Action::NewTab);
                assert_eq!(Flags::default(), as_leaf(e).flags);
            }
        }
    }

    // test "set: parseAndPut overwrite sequence"
    #[test]
    fn set_parse_and_put_overwrite_sequence() {
        let mut s = Set::default();

        s.parse_and_put(b"a>b=new_tab").unwrap();
        s.parse_and_put(b"a>b=new_window").unwrap();
        let e = s
            .get_value(Trigger {
                key: uni(b'a' as u32),
                ..Default::default()
            })
            .unwrap();
        assert!(matches!(e, Value::Leader(_)));
        if let Value::Leader(inner) = e {
            let e = inner
                .get_value(Trigger {
                    key: uni(b'b' as u32),
                    ..Default::default()
                })
                .unwrap();
            assert!(matches!(e, Value::Leaf(_)));
            assert!(as_leaf(e).action == Action::NewWindow);
            assert_eq!(Flags::default(), as_leaf(e).flags);
        }
    }

    // test "set: parseAndPut overwrite leader"
    #[test]
    fn set_parse_and_put_overwrite_leader() {
        let mut s = Set::default();

        s.parse_and_put(b"a=new_tab").unwrap();
        s.parse_and_put(b"a>b=new_window").unwrap();
        let e = s
            .get_value(Trigger {
                key: uni(b'a' as u32),
                ..Default::default()
            })
            .unwrap();
        assert!(matches!(e, Value::Leader(_)));
        if let Value::Leader(inner) = e {
            let e = inner
                .get_value(Trigger {
                    key: uni(b'b' as u32),
                    ..Default::default()
                })
                .unwrap();
            assert!(matches!(e, Value::Leaf(_)));
            assert!(as_leaf(e).action == Action::NewWindow);
            assert_eq!(Flags::default(), as_leaf(e).flags);
        }
    }

    // test "set: parseAndPut unbind sequence unbinds leader"
    #[test]
    fn set_parse_and_put_unbind_sequence_unbinds_leader() {
        let mut s = Set::default();

        s.parse_and_put(b"a>b=new_window").unwrap();
        s.parse_and_put(b"a>b=unbind").unwrap();
        {
            let t = Trigger {
                key: uni(b'a' as u32),
                ..Default::default()
            };
            assert!(s.get_value(t).is_none());
        }
    }

    // test "set: parseAndPut unbind sequence unbinds leader if not set"
    #[test]
    fn set_parse_and_put_unbind_sequence_unbinds_leader_if_not_set() {
        let mut s = Set::default();

        s.parse_and_put(b"a>b=unbind").unwrap();
        {
            let t = Trigger {
                key: uni(b'a' as u32),
                ..Default::default()
            };
            assert!(s.get_value(t).is_none());
        }
    }

    // test "set: parseAndPut sequence preserves reverse mapping"
    #[test]
    fn set_parse_and_put_sequence_preserves_reverse_mapping() {
        let mut s = Set::default();

        s.parse_and_put(b"a=new_window").unwrap();
        s.parse_and_put(b"ctrl+a>b=new_window").unwrap();

        // Creates reverse mapping
        {
            let trigger = s.get_trigger(&Action::NewWindow).unwrap();
            assert!(trigger.key == uni(b'a' as u32));
        }
    }

    // test "set: put overwrites sequence"
    #[test]
    fn set_put_overwrites_sequence() {
        let mut s = Set::default();

        s.parse_and_put(b"ctrl+a>b=new_window").unwrap();
        s.put(
            Trigger {
                mods: Mods {
                    ctrl: true,
                    ..Default::default()
                },
                key: uni(b'a' as u32),
            },
            Action::NewWindow,
        );

        // Creates reverse mapping
        {
            let trigger = s.get_trigger(&Action::NewWindow).unwrap();
            assert!(trigger.key == uni(b'a' as u32));
        }
    }

    // test "set: maintains reverse mapping"
    #[test]
    fn set_maintains_reverse_mapping() {
        let mut s = Set::default();

        s.put(
            Trigger {
                key: uni(b'a' as u32),
                ..Default::default()
            },
            Action::NewWindow,
        );
        {
            let trigger = s.get_trigger(&Action::NewWindow).unwrap();
            assert!(trigger.key == uni(b'a' as u32));
        }

        // should be most recent
        s.put(
            Trigger {
                key: uni(b'b' as u32),
                ..Default::default()
            },
            Action::NewWindow,
        );
        {
            let trigger = s.get_trigger(&Action::NewWindow).unwrap();
            assert!(trigger.key == uni(b'b' as u32));
        }

        // removal should replace
        s.remove(Trigger {
            key: uni(b'b' as u32),
            ..Default::default()
        });
        {
            let trigger = s.get_trigger(&Action::NewWindow).unwrap();
            assert!(trigger.key == uni(b'a' as u32));
        }
    }

    // test "set: performable is not part of reverse mappings"
    #[test]
    fn set_performable_is_not_part_of_reverse_mappings() {
        let mut s = Set::default();

        s.put(
            Trigger {
                key: uni(b'a' as u32),
                ..Default::default()
            },
            Action::NewWindow,
        );
        {
            let trigger = s.get_trigger(&Action::NewWindow).unwrap();
            assert!(trigger.key == uni(b'a' as u32));
        }

        // trigger should be non-performable
        s.put_flags(
            Trigger {
                key: uni(b'b' as u32),
                ..Default::default()
            },
            Action::NewWindow,
            Flags {
                performable: true,
                ..Default::default()
            },
        );
        {
            let trigger = s.get_trigger(&Action::NewWindow).unwrap();
            assert!(trigger.key == uni(b'a' as u32));
        }

        // removal of performable should do nothing
        s.remove(Trigger {
            key: uni(b'b' as u32),
            ..Default::default()
        });
        {
            let trigger = s.get_trigger(&Action::NewWindow).unwrap();
            assert!(trigger.key == uni(b'a' as u32));
        }
    }

    // test "set: overriding a mapping updates reverse"
    #[test]
    fn set_overriding_a_mapping_updates_reverse() {
        let mut s = Set::default();

        s.put(
            Trigger {
                key: uni(b'a' as u32),
                ..Default::default()
            },
            Action::NewWindow,
        );
        {
            let trigger = s.get_trigger(&Action::NewWindow).unwrap();
            assert!(trigger.key == uni(b'a' as u32));
        }

        // should be most recent
        s.put(
            Trigger {
                key: uni(b'a' as u32),
                ..Default::default()
            },
            Action::NewTab,
        );
        {
            let trigger = s.get_trigger(&Action::NewWindow);
            assert!(trigger.is_none());
        }
    }

    // test "set: consumed state"
    #[test]
    fn set_consumed_state() {
        let mut s = Set::default();

        s.put(
            Trigger {
                key: uni(b'a' as u32),
                ..Default::default()
            },
            Action::NewWindow,
        );
        assert!(matches!(
            s.get_value(Trigger {
                key: uni(b'a' as u32),
                ..Default::default()
            })
            .unwrap(),
            Value::Leaf(_)
        ));
        assert!(
            as_leaf(
                s.get_value(Trigger {
                    key: uni(b'a' as u32),
                    ..Default::default()
                })
                .unwrap()
            )
            .flags
            .consumed
        );

        s.put_flags(
            Trigger {
                key: uni(b'a' as u32),
                ..Default::default()
            },
            Action::NewWindow,
            Flags {
                consumed: false,
                ..Default::default()
            },
        );
        assert!(matches!(
            s.get_value(Trigger {
                key: uni(b'a' as u32),
                ..Default::default()
            })
            .unwrap(),
            Value::Leaf(_)
        ));
        assert!(
            !as_leaf(
                s.get_value(Trigger {
                    key: uni(b'a' as u32),
                    ..Default::default()
                })
                .unwrap()
            )
            .flags
            .consumed
        );

        s.put(
            Trigger {
                key: uni(b'a' as u32),
                ..Default::default()
            },
            Action::NewWindow,
        );
        assert!(matches!(
            s.get_value(Trigger {
                key: uni(b'a' as u32),
                ..Default::default()
            })
            .unwrap(),
            Value::Leaf(_)
        ));
        assert!(
            as_leaf(
                s.get_value(Trigger {
                    key: uni(b'a' as u32),
                    ..Default::default()
                })
                .unwrap()
            )
            .flags
            .consumed
        );
    }

    // test "set: parseAndPut chain"
    #[test]
    fn set_parse_and_put_chain() {
        let mut s = Set::default();

        s.parse_and_put(b"a=new_window").unwrap();
        s.parse_and_put(b"chain=new_tab").unwrap();

        // Creates forward mapping as leaf_chained
        {
            let entry = s
                .get_value(Trigger {
                    key: uni(b'a' as u32),
                    ..Default::default()
                })
                .unwrap();
            assert!(matches!(entry, Value::LeafChained(_)));
            let chained = as_leaf_chained(entry);
            assert_eq!(2usize, chained.actions.len());
            assert!(chained.actions[0] == Action::NewWindow);
            assert!(chained.actions[1] == Action::NewTab);
        }

        // Does not create reverse mapping, because reverse mappings are only for
        // non-chained actions.
        {
            assert!(s.get_trigger(&Action::NewWindow).is_none());
        }
    }

    // test "set: parseAndPut chain without parent is error"
    #[test]
    fn set_parse_and_put_chain_without_parent_is_error() {
        let mut s = Set::default();

        // Chain without a prior binding should fail
        assert_eq!(
            Err(ParseAndPutError::InvalidFormat),
            s.parse_and_put(b"chain=new_tab")
        );
    }

    // test "set: parseAndPut chain multiple times"
    #[test]
    fn set_parse_and_put_chain_multiple_times() {
        let mut s = Set::default();

        s.parse_and_put(b"a=new_window").unwrap();
        s.parse_and_put(b"chain=new_tab").unwrap();
        s.parse_and_put(b"chain=close_surface").unwrap();

        // Should have 3 actions chained
        {
            let entry = s
                .get_value(Trigger {
                    key: uni(b'a' as u32),
                    ..Default::default()
                })
                .unwrap();
            assert!(matches!(entry, Value::LeafChained(_)));
            let chained = as_leaf_chained(entry);
            assert_eq!(3usize, chained.actions.len());
            assert!(chained.actions[0] == Action::NewWindow);
            assert!(chained.actions[1] == Action::NewTab);
            assert!(chained.actions[2] == Action::CloseSurface);
        }
    }

    // test "set: parseAndPut chain preserves flags"
    #[test]
    fn set_parse_and_put_chain_preserves_flags() {
        let mut s = Set::default();

        s.parse_and_put(b"unconsumed:a=new_window").unwrap();
        s.parse_and_put(b"chain=new_tab").unwrap();

        // Should preserve unconsumed flag
        {
            let entry = s
                .get_value(Trigger {
                    key: uni(b'a' as u32),
                    ..Default::default()
                })
                .unwrap();
            assert!(matches!(entry, Value::LeafChained(_)));
            let chained = as_leaf_chained(entry);
            assert!(!chained.flags.consumed);
            assert_eq!(2usize, chained.actions.len());
        }
    }

    // test "set: parseAndPut chain after unbind is error"
    #[test]
    fn set_parse_and_put_chain_after_unbind_is_error() {
        let mut s = Set::default();

        s.parse_and_put(b"a=new_window").unwrap();
        s.parse_and_put(b"a=unbind").unwrap();

        // Chain after unbind should fail because chain_parent is cleared
        assert_eq!(
            Err(ParseAndPutError::InvalidFormat),
            s.parse_and_put(b"chain=new_tab")
        );
    }

    // test "set: parseAndPut chain on sequence"
    #[test]
    fn set_parse_and_put_chain_on_sequence() {
        let mut s = Set::default();

        s.parse_and_put(b"a>b=new_window").unwrap();
        s.parse_and_put(b"chain=new_tab").unwrap();

        // Navigate to the inner set
        let a_entry = s
            .get_value(Trigger {
                key: uni(b'a' as u32),
                ..Default::default()
            })
            .unwrap();
        assert!(matches!(a_entry, Value::Leader(_)));
        if let Value::Leader(inner_set) = a_entry {
            // Check the chained binding
            let b_entry = inner_set
                .get_value(Trigger {
                    key: uni(b'b' as u32),
                    ..Default::default()
                })
                .unwrap();
            assert!(matches!(b_entry, Value::LeafChained(_)));
            let chained = as_leaf_chained(b_entry);
            assert_eq!(2usize, chained.actions.len());
            assert!(chained.actions[0] == Action::NewWindow);
            assert!(chained.actions[1] == Action::NewTab);
        }
    }

    // test "set: parseAndPut chain with unbind is error"
    #[test]
    fn set_parse_and_put_chain_with_unbind_is_error() {
        let mut s = Set::default();

        s.parse_and_put(b"a=new_window").unwrap();

        // chain=unbind is not valid
        assert_eq!(
            Err(ParseAndPutError::InvalidFormat),
            s.parse_and_put(b"chain=unbind")
        );

        // Original binding should still exist
        let entry = s
            .get_value(Trigger {
                key: uni(b'a' as u32),
                ..Default::default()
            })
            .unwrap();
        assert!(matches!(entry, Value::Leaf(_)));
        assert!(as_leaf(entry).action == Action::NewWindow);
    }

    // PORT NOTE: helper to construct a KeyEvent the way the reference's getEvent
    // tests do (`.{ .key = ..., .mods = ..., .utf8 = ..., .unshifted_codepoint = ... }`).
    fn ev(key: KeyKey, mods: Mods, utf8: &[u8], unshifted_codepoint: u32) -> KeyEvent {
        KeyEvent {
            key,
            mods,
            utf8: utf8.to_vec(),
            unshifted_codepoint,
            ..Default::default()
        }
    }

    // test "set: getEvent physical"
    #[test]
    fn set_get_event_physical() {
        let mut s = Set::default();

        s.parse_and_put(b"ctrl+quote=new_window").unwrap();

        // Physical matches on physical
        {
            let idx = s
                .get_event(&ev(
                    KeyKey::Quote,
                    Mods {
                        ctrl: true,
                        ..Default::default()
                    },
                    b"",
                    0,
                ))
                .unwrap();
            let action = as_leaf(s.value_at(idx));
            assert!(action.action == Action::NewWindow);
        }

        // Physical does not match on UTF8/codepoint
        {
            let action = s.get_event(&ev(
                KeyKey::KeyA,
                Mods {
                    ctrl: true,
                    ..Default::default()
                },
                b"'",
                b'\'' as u32,
            ));
            assert!(action.is_none());
        }
    }

    // test "set: getEvent codepoint"
    #[test]
    fn set_get_event_codepoint() {
        let mut s = Set::default();

        s.parse_and_put(b"ctrl+'=new_window").unwrap();

        // Matches on codepoint
        {
            let idx = s
                .get_event(&ev(
                    KeyKey::KeyA,
                    Mods {
                        ctrl: true,
                        ..Default::default()
                    },
                    b"",
                    b'\'' as u32,
                ))
                .unwrap();
            let action = as_leaf(s.value_at(idx));
            assert!(action.action == Action::NewWindow);
        }

        // Matches on UTF-8
        {
            let idx = s
                .get_event(&ev(
                    KeyKey::KeyA,
                    Mods {
                        ctrl: true,
                        ..Default::default()
                    },
                    b"'",
                    0,
                ))
                .unwrap();
            let action = as_leaf(s.value_at(idx));
            assert!(action.action == Action::NewWindow);
        }

        // Doesn't match on physical
        {
            let action = s.get_event(&ev(
                KeyKey::KeyA,
                Mods {
                    ctrl: true,
                    ..Default::default()
                },
                b"",
                0,
            ));
            assert!(action.is_none());
        }
    }

    // test "set: getEvent codepoint case folding"
    #[test]
    fn set_get_event_codepoint_case_folding() {
        let mut s = Set::default();

        s.parse_and_put(b"ctrl+A=new_window").unwrap();

        // Lowercase codepoint
        {
            let idx = s
                .get_event(&ev(
                    KeyKey::KeyJ,
                    Mods {
                        ctrl: true,
                        ..Default::default()
                    },
                    b"",
                    b'a' as u32,
                ))
                .unwrap();
            let action = as_leaf(s.value_at(idx));
            assert!(action.action == Action::NewWindow);
        }

        // Uppercase codepoint
        {
            let idx = s
                .get_event(&ev(
                    KeyKey::KeyA,
                    Mods {
                        ctrl: true,
                        ..Default::default()
                    },
                    b"",
                    b'A' as u32,
                ))
                .unwrap();
            let action = as_leaf(s.value_at(idx));
            assert!(action.action == Action::NewWindow);
        }

        // Negative case for sanity
        {
            let action = s.get_event(&ev(
                KeyKey::KeyJ,
                Mods {
                    ctrl: true,
                    ..Default::default()
                },
                b"",
                0,
            ));
            assert!(action.is_none());
        }
    }

    // test "set: getEvent catch_all fallback"
    #[test]
    fn set_get_event_catch_all_fallback() {
        let mut s = Set::default();

        s.parse_and_put(b"catch_all=ignore").unwrap();

        // Matches unbound key without modifiers
        {
            let idx = s
                .get_event(&ev(KeyKey::KeyA, Mods::default(), b"", 0))
                .unwrap();
            let action = as_leaf(s.value_at(idx));
            assert!(action.action == Action::Ignore);
        }

        // Matches unbound key with modifiers (falls back to catch_all without mods)
        {
            let idx = s
                .get_event(&ev(
                    KeyKey::KeyA,
                    Mods {
                        ctrl: true,
                        ..Default::default()
                    },
                    b"",
                    0,
                ))
                .unwrap();
            let action = as_leaf(s.value_at(idx));
            assert!(action.action == Action::Ignore);
        }

        // Specific binding takes precedence over catch_all
        s.parse_and_put(b"ctrl+b=new_window").unwrap();
        {
            let idx = s
                .get_event(&ev(
                    KeyKey::KeyB,
                    Mods {
                        ctrl: true,
                        ..Default::default()
                    },
                    b"",
                    b'b' as u32,
                ))
                .unwrap();
            let action = as_leaf(s.value_at(idx));
            assert!(action.action == Action::NewWindow);
        }
    }

    // test "set: getEvent catch_all with modifiers"
    #[test]
    fn set_get_event_catch_all_with_modifiers() {
        let mut s = Set::default();

        s.parse_and_put(b"ctrl+catch_all=close_surface").unwrap();
        s.parse_and_put(b"catch_all=ignore").unwrap();

        // Key with ctrl matches catch_all with ctrl modifier
        {
            let idx = s
                .get_event(&ev(
                    KeyKey::KeyA,
                    Mods {
                        ctrl: true,
                        ..Default::default()
                    },
                    b"",
                    0,
                ))
                .unwrap();
            let action = as_leaf(s.value_at(idx));
            assert!(action.action == Action::CloseSurface);
        }

        // Key without mods matches catch_all without mods
        {
            let idx = s
                .get_event(&ev(KeyKey::KeyA, Mods::default(), b"", 0))
                .unwrap();
            let action = as_leaf(s.value_at(idx));
            assert!(action.action == Action::Ignore);
        }

        // Key with different mods falls back to catch_all without mods
        {
            let idx = s
                .get_event(&ev(
                    KeyKey::KeyA,
                    Mods {
                        alt: true,
                        ..Default::default()
                    },
                    b"",
                    0,
                ))
                .unwrap();
            let action = as_leaf(s.value_at(idx));
            assert!(action.action == Action::Ignore);
        }
    }

    // test "Action: clone"
    #[test]
    fn action_clone() {
        {
            let a: Action = Action::Ignore;
            let b = a.clone_action();
            assert!(b == Action::Ignore);
        }

        {
            let a: Action = Action::Text(b"foo".to_vec());
            let b = a.clone_action();
            assert!(matches!(b, Action::Text(_)));
        }
    }

    // test "parse: increase_font_size"
    #[test]
    fn parse_increase_font_size() {
        {
            let binding = parse_single(b"a=increase_font_size:1.5").unwrap();
            assert!(matches!(binding.action, Action::IncreaseFontSize(_)));
            if let Action::IncreaseFontSize(v) = binding.action {
                assert_eq!(1.5, v);
            }
        }
    }

    // test "parse: decrease_font_size"
    #[test]
    fn parse_decrease_font_size() {
        {
            let binding = parse_single(b"a=decrease_font_size:2.5").unwrap();
            assert!(matches!(binding.action, Action::DecreaseFontSize(_)));
            if let Action::DecreaseFontSize(v) = binding.action {
                assert_eq!(2.5, v);
            }
        }
    }

    // test "parse: reset_font_size"
    #[test]
    fn parse_reset_font_size() {
        {
            let binding = parse_single(b"a=reset_font_size").unwrap();
            assert!(matches!(binding.action, Action::ResetFontSize));
        }
    }

    // test "parse: set_font_size"
    #[test]
    fn parse_set_font_size() {
        {
            let binding = parse_single(b"a=set_font_size:13.5").unwrap();
            assert!(matches!(binding.action, Action::SetFontSize(_)));
            if let Action::SetFontSize(v) = binding.action {
                assert_eq!(13.5, v);
            }
        }
    }

    // test "parse: copy to clipboard default"
    #[test]
    fn parse_copy_to_clipboard_default() {
        {
            let binding = parse_single(b"a=copy_to_clipboard").unwrap();
            assert!(matches!(binding.action, Action::CopyToClipboard(_)));
            if let Action::CopyToClipboard(v) = binding.action {
                assert_eq!(CopyToClipboard::Mixed, v);
            }
        }
    }

    // test "parse: copy to clipboard explicit"
    #[test]
    fn parse_copy_to_clipboard_explicit() {
        {
            let binding = parse_single(b"a=copy_to_clipboard:html").unwrap();
            assert!(matches!(binding.action, Action::CopyToClipboard(_)));
            if let Action::CopyToClipboard(v) = binding.action {
                assert_eq!(CopyToClipboard::Html, v);
            }
        }
    }

    // test "parse: write screen file no format"
    #[test]
    fn parse_write_screen_file_no_format() {
        {
            let binding = parse_single(b"a=write_screen_file:copy").unwrap();
            assert!(matches!(binding.action, Action::WriteScreenFile(_)));
            if let Action::WriteScreenFile(v) = binding.action {
                assert_eq!(WriteScreen::COPY, v);
            }
        }
    }

    // test "parse: write screen file format"
    #[test]
    fn parse_write_screen_file_format() {
        {
            let binding = parse_single(b"a=write_screen_file:copy,html").unwrap();
            assert!(matches!(binding.action, Action::WriteScreenFile(_)));
            if let Action::WriteScreenFile(v) = binding.action {
                assert_eq!(
                    WriteScreen {
                        action: WriteScreenAction::Copy,
                        emit: WriteScreenFormat::Html
                    },
                    v
                );
            }
        }
    }

    // test "parse: write screen file format as string"
    #[test]
    fn parse_write_screen_file_format_as_string() {
        {
            let mut buf = AllocatingWriter::init();
            let binding = parse_single(b"a=write_screen_file:copy,html").unwrap();
            binding.action.format(&mut buf).unwrap();
            assert_eq!(b"write_screen_file:copy,html", buf.written());
        }
    }

    // test "parse: write screen file invalid"
    #[test]
    fn parse_write_screen_file_invalid() {
        assert_eq!(
            Err(ParseSingleError::InvalidFormat),
            parse_single(b"a=write_screen_file:")
        );
        assert_eq!(
            Err(ParseSingleError::InvalidFormat),
            parse_single(b"a=write_screen_file:,")
        );
        assert_eq!(
            Err(ParseSingleError::InvalidFormat),
            parse_single(b"a=write_screen_file:copy,")
        );
        assert_eq!(
            Err(ParseSingleError::InvalidFormat),
            parse_single(b"a=write_screen_file:copy,html,extra")
        );
    }

    // test "action: format"
    #[test]
    fn action_format() {
        let a: Action = Action::Text("👻Rustty'\"".as_bytes().to_vec());

        let mut buf = AllocatingWriter::init();
        a.format(&mut buf).unwrap();

        // const b = try Binding.Action.parse(buf.written());
        let b = Action::parse(buf.written()).unwrap();
        // try testing.expect(a.equal(b));
        assert!(a.equal(&b));
    }

    // test "action: format set title"
    #[test]
    fn action_format_set_title() {
        let a: Action = Action::SetTabTitle(b"foo bar".to_vec());

        let mut buf = AllocatingWriter::init();
        a.format(&mut buf).unwrap();
        assert_eq!(b"set_tab_title:foo bar", buf.written());
    }

    // test "set: appendChain with no parent returns error"
    #[test]
    fn set_append_chain_with_no_parent_returns_error() {
        let mut s = Set::default();

        assert_eq!(
            Err(AppendChainError::NoChainParent),
            s.append_chain(Action::NewTab)
        );
    }

    // test "set: appendChain after put converts to leaf_chained"
    #[test]
    fn set_append_chain_after_put_converts_to_leaf_chained() {
        let mut s = Set::default();

        s.put(
            Trigger {
                key: uni(b'a' as u32),
                ..Default::default()
            },
            Action::NewWindow,
        );

        // First appendChain converts leaf to leaf_chained and appends the new action
        s.append_chain(Action::NewTab).unwrap();

        let entry = s
            .get_value(Trigger {
                key: uni(b'a' as u32),
                ..Default::default()
            })
            .unwrap();
        assert!(matches!(entry, Value::LeafChained(_)));

        let chained = as_leaf_chained(entry);
        assert_eq!(2usize, chained.actions.len());
        assert!(chained.actions[0] == Action::NewWindow);
        assert!(chained.actions[1] == Action::NewTab);
    }

    // test "set: appendChain after putFlags preserves flags"
    #[test]
    fn set_append_chain_after_put_flags_preserves_flags() {
        let mut s = Set::default();

        s.put_flags(
            Trigger {
                key: uni(b'a' as u32),
                ..Default::default()
            },
            Action::NewWindow,
            Flags {
                consumed: false,
                ..Default::default()
            },
        );
        s.append_chain(Action::NewTab).unwrap();

        let entry = s
            .get_value(Trigger {
                key: uni(b'a' as u32),
                ..Default::default()
            })
            .unwrap();
        assert!(matches!(entry, Value::LeafChained(_)));

        let chained = as_leaf_chained(entry);
        assert!(!chained.flags.consumed);
        assert_eq!(2usize, chained.actions.len());
        assert!(chained.actions[0] == Action::NewWindow);
        assert!(chained.actions[1] == Action::NewTab);
    }

    // test "set: appendChain multiple times"
    #[test]
    fn set_append_chain_multiple_times() {
        let mut s = Set::default();

        s.put(
            Trigger {
                key: uni(b'a' as u32),
                ..Default::default()
            },
            Action::NewWindow,
        );
        s.append_chain(Action::NewTab).unwrap();
        s.append_chain(Action::CloseSurface).unwrap();

        let entry = s
            .get_value(Trigger {
                key: uni(b'a' as u32),
                ..Default::default()
            })
            .unwrap();
        assert!(matches!(entry, Value::LeafChained(_)));

        let chained = as_leaf_chained(entry);
        assert_eq!(3usize, chained.actions.len());
        assert!(chained.actions[0] == Action::NewWindow);
        assert!(chained.actions[1] == Action::NewTab);
        assert!(chained.actions[2] == Action::CloseSurface);
    }

    // test "set: appendChain removes reverse mapping"
    #[test]
    fn set_append_chain_removes_reverse_mapping() {
        let mut s = Set::default();

        s.put(
            Trigger {
                key: uni(b'a' as u32),
                ..Default::default()
            },
            Action::NewWindow,
        );

        // Verify reverse mapping exists before chaining
        assert!(s.get_trigger(&Action::NewWindow).is_some());

        // Chaining should remove the reverse mapping
        s.append_chain(Action::NewTab).unwrap();

        // Reverse mapping should be gone since chained actions are not in reverse map
        assert!(s.get_trigger(&Action::NewWindow).is_none());
    }

    // test "set: appendChain with performable does not affect reverse mapping"
    #[test]
    fn set_append_chain_with_performable_does_not_affect_reverse_mapping() {
        let mut s = Set::default();

        // Add a non-performable binding first
        s.put(
            Trigger {
                key: uni(b'b' as u32),
                ..Default::default()
            },
            Action::NewWindow,
        );
        assert!(s.get_trigger(&Action::NewWindow).is_some());

        // Add a performable binding (not in reverse map) and chain it
        s.put_flags(
            Trigger {
                key: uni(b'a' as u32),
                ..Default::default()
            },
            Action::CloseSurface,
            Flags {
                performable: true,
                ..Default::default()
            },
        );

        // close_surface was performable, so not in reverse map
        assert!(s.get_trigger(&Action::CloseSurface).is_none());

        // Chaining the performable binding should not crash or affect anything
        s.append_chain(Action::NewTab).unwrap();

        // The non-performable new_window binding should still be in reverse map
        assert!(s.get_trigger(&Action::NewWindow).is_some());
    }

    // test "set: appendChain restores next valid reverse mapping"
    #[test]
    fn set_append_chain_restores_next_valid_reverse_mapping() {
        let mut s = Set::default();

        // Add two bindings for the same action
        s.put(
            Trigger {
                key: uni(b'a' as u32),
                ..Default::default()
            },
            Action::NewWindow,
        );
        s.put(
            Trigger {
                key: uni(b'b' as u32),
                ..Default::default()
            },
            Action::NewWindow,
        );

        // Reverse mapping should point to 'b' (most recent)
        {
            let trigger = s.get_trigger(&Action::NewWindow).unwrap();
            assert!(trigger.key == uni(b'b' as u32));
        }

        // Chain an action to 'b', which should restore 'a' in the reverse map
        s.append_chain(Action::NewTab).unwrap();

        // Now reverse mapping should point to 'a'
        {
            let trigger = s.get_trigger(&Action::NewWindow).unwrap();
            assert!(trigger.key == uni(b'a' as u32));
        }
    }

    // PORT NOTE: helper reproducing the reference's formatEntries test prologue:
    // `try entry.key_ptr.format(&writer);` writes the trigger into the shared
    // fixed buffer, then `entry.value_ptr.formatEntries(&writer, formatter)`.
    // Returns the formatter's accumulated output bytes.
    fn format_entries_to_string(s: &Set, t: Trigger) -> Vec<u8> {
        let value = s.get_value(t).unwrap();
        let key = s.get_key(t).unwrap();

        let mut output = AllocatingWriter::init();
        let mut buf = RewindBuffer::new();

        // Write the trigger first (as formatEntry in Config.zig does)
        {
            let mut w = RewindWriter { inner: &mut buf };
            key.format(&mut w).unwrap();
        }
        let mut formatter = entry_formatter(b"keybind", &mut output);
        match value.format_entries(&mut buf, &mut formatter) {
            Ok(()) => {}
            Err(FormatEntriesError::OutOfMemory) => panic!("OutOfMemory"),
            Err(FormatEntriesError::Writer(_)) => unreachable!(),
        }
        output.written().to_vec()
    }

    // test "set: formatEntries leaf_chained"
    #[test]
    fn set_format_entries_leaf_chained() {
        let mut s = Set::default();

        // Create a chained binding
        s.parse_and_put(b"a=new_window").unwrap();
        s.parse_and_put(b"chain=new_tab").unwrap();

        // Verify it's a leaf_chained
        let t = Trigger {
            key: uni(b'a' as u32),
            ..Default::default()
        };
        assert!(matches!(s.get_value(t).unwrap(), Value::LeafChained(_)));

        // Format the entries
        let output = format_entries_to_string(&s, t);

        let expected: &[u8] = b"keybind = a=new_window\nkeybind = chain=new_tab\n";
        assert_eq!(expected, output.as_slice());
    }

    // test "set: formatEntries leaf_chained multiple chains"
    #[test]
    fn set_format_entries_leaf_chained_multiple_chains() {
        let mut s = Set::default();

        // Create a chained binding with 3 actions
        s.parse_and_put(b"ctrl+a=new_window").unwrap();
        s.parse_and_put(b"chain=new_tab").unwrap();
        s.parse_and_put(b"chain=close_surface").unwrap();

        // Verify it's a leaf_chained with 3 actions
        let t = Trigger {
            key: uni(b'a' as u32),
            mods: Mods {
                ctrl: true,
                ..Default::default()
            },
        };
        assert!(matches!(s.get_value(t).unwrap(), Value::LeafChained(_)));
        assert_eq!(
            3usize,
            as_leaf_chained(s.get_value(t).unwrap()).actions.len()
        );

        // Format the entries
        let output = format_entries_to_string(&s, t);

        let expected: &[u8] =
            b"keybind = ctrl+a=new_window\nkeybind = chain=new_tab\nkeybind = chain=close_surface\n";
        assert_eq!(expected, output.as_slice());
    }

    // test "set: formatEntries leaf_chained with text action"
    #[test]
    fn set_format_entries_leaf_chained_with_text_action() {
        let mut s = Set::default();

        // Create a chained binding with text actions
        s.parse_and_put(b"a=text:hello").unwrap();
        s.parse_and_put(b"chain=text:world").unwrap();

        // Format the entries
        let t = Trigger {
            key: uni(b'a' as u32),
            ..Default::default()
        };
        let output = format_entries_to_string(&s, t);

        let expected: &[u8] = b"keybind = a=text:hello\nkeybind = chain=text:world\n";
        assert_eq!(expected, output.as_slice());
    }
}
