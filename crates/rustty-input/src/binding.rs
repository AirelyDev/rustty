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

mod action_impl;
mod parser;
mod set;
mod trigger;
pub use action_impl::*;
pub use parser::*;
pub use set::*;
pub use trigger::*;

#[cfg(test)]
mod tests;
