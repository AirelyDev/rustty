//! rustty-input input/command
//!
//! PORTS: ghostty src/input/command.zig
//!
//! PORT STATUS: done
//
// PORT NOTE: the reference imports
//   const std = @import("std");
//   const assert = @import("../quirks.zig").inlineAssert;
//   const Allocator = std.mem.Allocator;
//   const Action = @import("Binding.zig").Action;
// `assert` -> `rustty_util::quirks::inline_assert` (matching binding.rs).
// `Allocator` -> the Rust system allocator. `Action` -> the ported
// `crate::binding::Action`. The `[:0]const u8` sentinel-terminated strings
// (`title`/`description`) map to owned `Vec<u8>` for the stored fields; the
// `C` struct's `[*:0]const u8` pointers are produced by appending a NUL.

use crate::binding::*;
use rustty_cfg_fmt::AllocatingWriter;
use rustty_os::i18n;
use rustty_util::quirks::inlineAssert as assert;

/// A command is a named binding action that can be executed from
/// something like a command palette.
///
/// A command must be associated with a binding; all commands can be
/// mapped to traditional `keybind` configurations. This restriction
/// makes it so that there is nothing special about commands and likewise
/// it makes it trivial and consistent to define custom commands.
///
/// For apprt implementers: a command palette doesn't have to make use
/// of all the fields here. We try to provide as much information as
/// possible to make it easier to implement a command palette in the way
/// that makes the most sense for the application.
//
// PORT NOTE: `title`/`description` are Zig `[:0]const u8` (NUL-terminated). They
// are stored as `Vec<u8>` WITHOUT the trailing NUL; the NUL is reintroduced when
// converting to the `C` form. `description` defaults to "" (an empty slice).
#[derive(Clone, Debug, PartialEq)]
pub struct Command {
    pub action: Action,
    pub title: Vec<u8>,
    pub description: Vec<u8>,
}

/// `rustty_command_s` — `ghostty_command_s`.
//
// PORT NOTE: `Command.C` is an `extern struct` of four `[*:0]const u8`
// NUL-terminated C strings — i.e. four pointers, 32 bytes on LP64. It was
// PREVIOUSLY modeled as four owned `Vec<u8>` (96 bytes, no `#[repr(C)]`), which
// does not match `include/rustty-embed.h`'s `rustty_command_s` (4x
// `const char*`): the macOS app iterates the header's view of this struct with
// a 32-byte stride and reads each field with `String(cString:)`, which was a
// wild read over the 96-byte Rust layout. `C` is now `#[repr(C)]` with four raw
// `*const c_char` pointers matching the header exactly; see the A4 embed-ABI
// gate (`check_embed_h_abi.sh`). The pointers borrow NUL-terminated storage
// that must outlive them — see `CommandCStorage` below, which is rebuilt
// alongside every `C` value so a `C` can never dangle.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct C {
    pub action_key: *const std::os::raw::c_char,
    pub action: *const std::os::raw::c_char,
    pub title: *const std::os::raw::c_char,
    pub description: *const std::os::raw::c_char,
}

// The byte offsets below are the LP64 layout that `rustty-embed.h` declares and
// that the A4 gate (`check_embed_h_abi.sh`) reads out of these annotations. They
// are deliberately hard literals rather than `n * size_of::<ptr>()` expressions:
// a pointer-arithmetic form is tautological under any `repr`, so it cannot catch
// the layout break it exists to catch.
//
// The const asserts are therefore gated to 64-bit targets, while the `@embed-abi`
// comments the gate greps stay unconditional. `wasm32-unknown-unknown` has 4-byte
// pointers, so the literals are false there — but that target never links the
// embed C ABI at all (`rustty-embed.h` is a native-host header; the wasm
// artifacts export the `config_*`/`atlas_*`/`rustty_wc_*` surfaces instead), so
// there is no ABI to freeze and nothing is lost. Ungated, these asserts fail
// const evaluation and break the `wasm32` build of every crate that reaches
// `rustty-input` — which is how this gate was found.
// @embed-abi rustty_command_s 32
#[cfg(target_pointer_width = "64")]
const _: () = assert!(std::mem::size_of::<C>() == 32);
const _: () =
    assert!(std::mem::align_of::<C>() == std::mem::align_of::<*const std::os::raw::c_char>());
// @embed-abi-field rustty_command_s action_key 0
const _: () = assert!(std::mem::offset_of!(C, action_key) == 0);
// @embed-abi-field rustty_command_s action 8
#[cfg(target_pointer_width = "64")]
const _: () = assert!(std::mem::offset_of!(C, action) == 8);
// @embed-abi-field rustty_command_s title 16
#[cfg(target_pointer_width = "64")]
const _: () = assert!(std::mem::offset_of!(C, title) == 16);
// @embed-abi-field rustty_command_s description 24
#[cfg(target_pointer_width = "64")]
const _: () = assert!(std::mem::offset_of!(C, description) == 24);

/// Owns the NUL-terminated buffers that a `C` value's pointers borrow.
///
/// The oracle (`src/input/command.zig:23-29`) shares memory with the owning
/// `Command` and arena-allocates the formatted action string; a bare
/// `#[repr(C)]` struct of raw pointers has no way to express that lifetime in
/// Rust, so `CommandCStorage` plays the arena's role explicitly: it owns the
/// four `CString`s, and `as_c()` hands out a `C` borrowing them. Every call
/// site that produces a `C` must keep the matching `CommandCStorage` alive at
/// least as long — see `RepeatableCommand` in rustty-config's
/// `value_types.rs`, which stores `storage: Vec<CommandCStorage>` alongside
/// `value_c: Vec<C>` and rebuilds both together at every mutation site
/// (`init`, `parse_cli`, `clone_in`).
#[derive(Clone, Debug, Default)]
pub struct CommandCStorage {
    action_key: std::ffi::CString,
    action: std::ffi::CString,
    title: std::ffi::CString,
    description: std::ffi::CString,
}

impl CommandCStorage {
    /// Build the owned NUL-terminated storage for `cmd`. `action_key` is the
    /// action's tag name (`@tagName`); `action` is the action formatted the
    /// same way `Command.format` renders it (`std.fmt.comptimePrint`/
    /// `std.Io.Writer.Allocating` in the oracle). A `[:0]const u8` in Zig is
    /// not guaranteed free of interior NULs by its type alone; `CString::new`
    /// rejects those, so we fall back to an empty string rather than panic
    /// (matching the byte-slice fidelity policy: never silently truncate to a
    /// lossy `String`, but also never let untrusted config content abort the
    /// process).
    pub fn new(cmd: &Command) -> Self {
        let mut action_buf = AllocatingWriter::init();
        // format is Infallible for our Writer impl.
        let _ = cmd.action.format(&mut action_buf);
        let cs = |v: &[u8]| std::ffi::CString::new(v.to_vec()).unwrap_or_default();
        Self {
            action_key: cs(cmd.action.tag_name().as_bytes()),
            action: cs(action_buf.written()),
            title: cs(&cmd.title),
            description: cs(&cmd.description),
        }
    }

    /// Borrow this storage as the raw-pointer `C` view. The returned `C` is
    /// only valid for as long as `self` (and the `CString`s it owns) is alive.
    pub fn as_c(&self) -> C {
        C {
            action_key: self.action_key.as_ptr(),
            action: self.action.as_ptr(),
            title: self.title.as_ptr(),
            description: self.description.as_ptr(),
        }
    }

    /// Borrow the formatted-action string's bytes (NUL-terminated, matching
    /// the pre-ABI-fix `Vec<u8>` shape).
    //
    // PORT NOTE (regression fix, task-12 checkpoint): `action`/`action_key`/
    // `title`/`description` were made private fields when `C` was tightened
    // to a bare `#[repr(C)]` pointer struct (commit 5073fbfe, S1-F1/S103-F1).
    // That tightening closed a real dangling-pointer hazard: `C`'s raw
    // pointers must never be exposed or cloned independently of the
    // `CommandCStorage` that owns their backing `CString`s (see the
    // `RepeatableCommand` PORT NOTE above `as_c` for the aliasing bug this
    // prevents). The hazard lives in the *pointer* struct, not the *owned
    // string* — borrowing a `&[u8]` out of `CommandCStorage` cannot outlive
    // `self` (ordinary borrow-checked lifetime, no raw pointer involved), so
    // it does not reopen that hole. `crates/rustty-apprt/src/gtk/class/
    // command_palette.rs`'s `prop_get_action_key` is the one consumer that
    // needs the formatted action's bytes (not the pointer view); this is the
    // narrowest accessor that serves it.
    pub fn action(&self) -> &[u8] {
        self.action.as_bytes_with_nul()
    }
}

impl Command {
    pub fn clone_in(&self) -> Command {
        // return .{
        //     .action = try self.action.clone(alloc),
        //     .title = try alloc.dupeZ(u8, self.title),
        //     .description = try alloc.dupeZ(u8, self.description),
        // };
        Command {
            action: self.action.clone_action(),
            title: self.title.clone(),
            description: self.description.clone(),
        }
    }

    pub fn equal(&self, other: &Command) -> bool {
        // if (self.action.hash() != other.action.hash()) return false;
        if self.action.hash() != other.action.hash() {
            return false;
        }
        // if (!std.mem.eql(u8, self.title, other.title)) return false;
        if self.title != other.title {
            return false;
        }
        // if (!std.mem.eql(u8, self.description, other.description)) return false;
        if self.description != other.description {
            return false;
        }
        // return true;
        true
    }

    /// Convert this command to C-compatible storage at comptime.
    //
    // PORT NOTE: the reference `comptimeCval` asserts `@inComptime()` and uses
    // `std.fmt.comptimePrint("{f}", .{self.action})`. Rust has no comptime; this
    // produces the same C form at runtime by formatting the action via the
    // ported `Action::format`. The `@inComptime()` assert has no runtime analog
    // and is dropped (it is a build-context guard, not a behavioral check).
    //
    // PORT NOTE (ABI FIX, S1-F1/S103-F1): the reference returns `C` directly
    // because Zig's comptime data lives in the binary for `'static`-like
    // duration. Rust's `C` is now a bare `#[repr(C)]` pointer struct (see the
    // PORT NOTE on `C`) with nothing to own the pointees, so this returns the
    // owning `CommandCStorage` instead; callers get the `C` view via
    // `CommandCStorage::as_c()` and must keep the storage alive alongside it.
    pub fn comptime_cval(&self) -> CommandCStorage {
        // assert(@inComptime());
        // (see PORT NOTE: no runtime analog)

        // return .{
        //     .action_key = @tagName(self.action),
        //     .action = std.fmt.comptimePrint("{f}", .{self.action}),
        //     .title = self.title,
        //     .description = self.description,
        // };
        CommandCStorage::new(self)
    }

    /// Convert this command to C-compatible storage at runtime.
    ///
    /// This shares memory with the original command.
    ///
    /// The action string is allocated using the provided allocator. You can
    /// free the slice directly if you need to but we recommend an arena
    /// for this.
    //
    // PORT NOTE: the reference returns `Allocator.Error!C` (the `OutOfMemory`
    // path from `buf.toOwnedSliceSentinel`). The unit error preserves that
    // fallible signature; hence the `clippy::result_unit_err` allow.
    //
    // PORT NOTE (ABI FIX, S1-F1/S103-F1): see `comptime_cval` above — this
    // returns the owning `CommandCStorage` rather than a bare `C`, so the
    // pointers `C` exposes always have a live owner. Callers (e.g.
    // `RepeatableCommand::init`/`parse_cli`/`clone_in` in rustty-config) must
    // push the returned storage into their own storage list in the same
    // motion they push the `C` view, so neither ever outlives the other.
    #[allow(clippy::result_unit_err)]
    pub fn cval(&self) -> Result<CommandCStorage, ()> {
        // var buf: std.Io.Writer.Allocating = .init(alloc);
        // defer buf.deinit();
        // self.action.format(&buf.writer) catch return error.OutOfMemory;
        // (format is Infallible here)
        // const action = try buf.toOwnedSliceSentinel(0);
        // return .{
        //     .action_key = @tagName(self.action),
        //     .action = action.ptr,
        //     .title = self.title,
        //     .description = self.description,
        // };
        Ok(CommandCStorage::new(self))
    }

    // pub fn translated(self: Command) Command
    //
    // PORT NOTE: `std.mem.span(i18n._(self.title))` — the stored strings are
    // unsentineled `Vec<u8>` (see the struct PORT NOTE); the NUL the reference's
    // `[:0]const u8` carries is appended for the gettext lookup and the result
    // is copied back sentinel-free.
    pub fn translated(&self) -> Command {
        // return .{
        //     .action = self.action,
        //     .title = std.mem.span(i18n._(self.title)),
        //     .description = std.mem.span(i18n._(self.description)),
        // };
        Command {
            action: self.action.clone_action(),
            title: translate(&self.title),
            description: translate(&self.description),
        }
    }

    /// Implements a comparison function for std.mem.sortUnstable
    /// and similar functions. The sorting is defined by Rustty
    /// to be what we prefer. If a caller wants some other sorting,
    /// they should do it themselves.
    //
    // PORT NOTE: `std.ascii.orderIgnoreCase(lhs.title, rhs.title) == .lt`.
    // The ASCII case-insensitive order maps each byte through ASCII lowercase
    // and compares lexicographically.
    pub fn less_than(lhs: &Command, rhs: &Command) -> bool {
        order_ignore_case(&lhs.title, &rhs.title) == std::cmp::Ordering::Less
    }
}

// PORT NOTE: `cli.args.parseAutoStruct(inputpkg.Command, alloc, input, null)`
// (called by `config.RepeatableCommand.parseCLI`) reflects over `Command`'s
// fields. This impl provides that reflection (the orphan rule requires it to live
// here, with `Command`). The field order matches the reference struct declaration
// (`action`, `title`, `description`); `parseAutoStruct` tracks "set" fields and
// fills defaults / errors on unset-required ones via `apply_default`:
//   - `action: Action`            — required (no default), parsed via the
//                                    `parseTaggedUnion(Action)` path.
//   - `title: [:0]const u8`       — required (no default), `[:0]const u8` arm.
//   - `description: [:0]const u8 = ""` — defaults to the empty slice.
impl rustty_args::args::AutoStruct for Command {
    fn field_names() -> &'static [&'static [u8]] {
        &[b"action", b"title", b"description"]
    }
    fn set_field(
        &mut self,
        key: &[u8],
        value: Option<&[u8]>,
    ) -> Result<(), rustty_args::args::ParseError> {
        use rustty_args::args::{parse_field_sentinel_string, Error};
        match key {
            // .@"union" => parseTaggedUnion(Action, ...)
            b"action" => {
                self.action = Action::parse_auto_tagged_union(value.ok_or(Error::ValueRequired)?)?;
                Ok(())
            }
            // [:0]const u8 arm
            b"title" => {
                self.title = parse_field_sentinel_string(value)?;
                Ok(())
            }
            b"description" => {
                self.description = parse_field_sentinel_string(value)?;
                Ok(())
            }
            _ => Err(Error::InvalidField),
        }
    }
    fn apply_default(&mut self, i: usize) -> Result<(), rustty_args::args::ParseError> {
        match i {
            // action: Action — required (no default).
            0 => Err(rustty_args::args::Error::InvalidValue),
            // title: [:0]const u8 — required (no default).
            1 => Err(rustty_args::args::Error::InvalidValue),
            // description: [:0]const u8 = "" — defaults to the empty slice.
            2 => {
                self.description = Vec::new();
                Ok(())
            }
            _ => unreachable!(),
        }
    }
}

// PORT NOTE: `std.mem.span(i18n._(msgid))` — run the msgid through the gettext
// catalog (`i18n::gettext_`, the reference's `_`). The bytes are NUL-terminated
// for the C lookup and the (possibly translated) result is copied out before
// the temporary sentinel buffer drops.
fn translate(msgid: &[u8]) -> Vec<u8> {
    let c = std::ffi::CString::new(msgid.to_vec()).expect("msgid has no interior NUL");
    i18n::gettext_(&c).to_bytes().to_vec()
}

// PORT NOTE: `std.ascii.orderIgnoreCase` — compares two slices treating ASCII
// letters case-insensitively; on a common prefix, the shorter slice orders
// first.
fn order_ignore_case(a: &[u8], b: &[u8]) -> std::cmp::Ordering {
    let n = a.len().min(b.len());
    let mut i = 0;
    while i < n {
        let la = a[i].to_ascii_lowercase();
        let lb = b[i].to_ascii_lowercase();
        match la.cmp(&lb) {
            std::cmp::Ordering::Equal => {}
            other => return other,
        }
        i += 1;
    }
    a.len().cmp(&b.len())
}

/// PORT NOTE: the reference `defaults` is a comptime `[]const Command`. It is
/// built by iterating `@typeInfo(Action.Key).@"enum".fields`, summing
/// `actionCommands(action).len`, collecting all commands, then sorting with
/// `Command.lessThan`. Rust builds the equivalent `Vec<Command>` on demand; the
/// `@setEvalBranchQuota(100_000)` comptime directive has no runtime analog.
pub fn defaults() -> Vec<Command> {
    // @setEvalBranchQuota(100_000);

    // var count: usize = 0;
    // for (...) |field| { count += actionCommands(action).len; }
    let mut count: usize = 0;
    for field in Action::FIELD_NAMES {
        count += action_commands(field).len();
    }

    // var result: [count]Command = undefined;
    // var i: usize = 0;
    // for (...) |field| { for (commands) |cmd| { result[i] = cmd; i += 1; } }
    let mut result: Vec<Command> = Vec::with_capacity(count);
    let mut i: usize = 0;
    for field in Action::FIELD_NAMES {
        let commands = action_commands(field);
        for cmd in commands {
            result.push(cmd);
            i += 1;
        }
    }

    // std.mem.sortUnstable(Command, &result, {}, Command.lessThan);
    result.sort_unstable_by(|a, b| {
        if Command::less_than(a, b) {
            std::cmp::Ordering::Less
        } else if Command::less_than(b, a) {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Equal
        }
    });

    // assert(i == count);
    assert(i == count);
    // const final = result; break :defaults &final;
    result
}

/// Defaults in C-compatible form.
//
// PORT NOTE: the reference `defaultsC` mirrors `defaults` via `comptimeCval`
// and returns `[]const Command.C` directly (comptime data lives in the
// binary). Rust's `C` is a bare pointer struct with no owner (see the PORT
// NOTE on `C`/`CommandCStorage`), so this returns the owning
// `CommandCStorage` list instead of `Vec<C>`; call `.as_c()` per element for
// the pointer view, keeping the returned storage alive at least as long.
pub fn defaults_c() -> Vec<CommandCStorage> {
    // @setEvalBranchQuota(100_000);
    // var result: [defaults.len]Command.C = undefined;
    // for (defaults, 0..) |cmd, i| result[i] = cmd.comptimeCval();
    let d = defaults();
    let mut result: Vec<CommandCStorage> = Vec::with_capacity(d.len());
    for cmd in d.iter() {
        result.push(cmd.comptime_cval());
    }
    result
}

/// Returns the set of commands associated with this action key by
/// default. Not all actions should have commands. As a general guideline,
/// an action should have a command only if it is useful and reasonable
/// to appear in a command palette.
//
// PORT NOTE: the reference signature is `fn actionCommands(action: Action.Key)
// []const Command`. `Action.Key` is the union's tag enum; here the tag is the
// snake_case field name (`&str`), the single source-of-truth list being
// `Action::FIELD_NAMES`. The `comptime` prefixes on the reference return values
// (which keep the data in the binary, not on the stack) have no runtime analog
// and are dropped; the data is identical. The trailing assert
// `for (result) |cmd| assert(cmd.action == action)` is reproduced after the
// switch, comparing each command's action TAG against `action`.
fn action_commands(action: &str) -> Vec<Command> {
    // This is implemented as a function and switch rather than a
    // flat comptime const because we want to ensure we get a compiler
    // error when a new binding is added so that the contributor has
    // to consider whether that new binding should have commands or not.
    let result: Vec<Command> = match action {
        // Note: the use of `comptime` prefix on the return values
        // ensures that the data returned is all in the binary and
        // and not pointing to the stack.
        "reset" => vec![Command {
            action: Action::Reset,
            title: i18n::N_(b"Reset Terminal").to_vec(),
            description: i18n::N_(b"Reset the terminal to a clean state.").to_vec(),
        }],

        "copy_to_clipboard" => vec![
            Command {
                action: Action::CopyToClipboard(CopyToClipboard::Mixed),
                title: i18n::N_(b"Copy to Clipboard").to_vec(),
                description: i18n::N_(b"Copy the selected text to the clipboard in both plain and styled formats.").to_vec(),
            },
            Command {
                action: Action::CopyToClipboard(CopyToClipboard::Plain),
                title: i18n::N_(b"Copy Selection as Plain Text to Clipboard").to_vec(),
                description: i18n::N_(b"Copy the selected text as plain text to the clipboard.").to_vec(),
            },
            Command {
                action: Action::CopyToClipboard(CopyToClipboard::Vt),
                title: i18n::N_(b"Copy Selection as ANSI Sequences to Clipboard").to_vec(),
                description: i18n::N_(b"Copy the selected text as ANSI escape sequences to the clipboard.").to_vec(),
            },
            Command {
                action: Action::CopyToClipboard(CopyToClipboard::Html),
                title: i18n::N_(b"Copy Selection as HTML to Clipboard").to_vec(),
                description: i18n::N_(b"Copy the selected text as HTML to the clipboard.").to_vec(),
            },
        ],

        "copy_url_to_clipboard" => vec![Command {
            action: Action::CopyUrlToClipboard,
            title: i18n::N_(b"Copy URL to Clipboard").to_vec(),
            description: i18n::N_(b"Copy the URL under the cursor to the clipboard.").to_vec(),
        }],

        "copy_title_to_clipboard" => vec![Command {
            action: Action::CopyTitleToClipboard,
            title: i18n::N_(b"Copy Terminal Title to Clipboard").to_vec(),
            description: i18n::N_(b"Copy the terminal title to the clipboard. If the terminal title is not set this has no effect.").to_vec(),
        }],

        "paste_from_clipboard" => vec![Command {
            action: Action::PasteFromClipboard,
            title: i18n::N_(b"Paste from Clipboard").to_vec(),
            description: i18n::N_(b"Paste the contents of the main clipboard.").to_vec(),
        }],

        "paste_from_selection" => vec![Command {
            action: Action::PasteFromSelection,
            title: i18n::N_(b"Paste from Selection").to_vec(),
            description: i18n::N_(b"Paste the contents of the selection clipboard.").to_vec(),
        }],

        "start_search" => vec![Command {
            action: Action::StartSearch,
            title: i18n::N_(b"Start Search").to_vec(),
            description: i18n::N_(b"Start a search if one isn't already active.").to_vec(),
        }],

        "search_selection" => vec![Command {
            action: Action::SearchSelection,
            title: i18n::N_(b"Search Selection").to_vec(),
            description: i18n::N_(b"Start a search for the current text selection.").to_vec(),
        }],

        "end_search" => vec![Command {
            action: Action::EndSearch,
            title: i18n::N_(b"End Search").to_vec(),
            description: i18n::N_(b"End the current search if any and hide any GUI elements.").to_vec(),
        }],

        "navigate_search" => vec![
            Command {
                action: Action::NavigateSearch(NavigateSearch::Next),
                title: i18n::N_(b"Next Search Result").to_vec(),
                description: i18n::N_(b"Navigate to the next search result, if any.").to_vec(),
            },
            Command {
                action: Action::NavigateSearch(NavigateSearch::Previous),
                title: i18n::N_(b"Previous Search Result").to_vec(),
                description: i18n::N_(b"Navigate to the previous search result, if any.").to_vec(),
            },
        ],

        "increase_font_size" => vec![Command {
            action: Action::IncreaseFontSize(1.0),
            title: i18n::N_(b"Increase Font Size").to_vec(),
            description: i18n::N_(b"Increase the font size by 1 point.").to_vec(),
        }],

        "decrease_font_size" => vec![Command {
            action: Action::DecreaseFontSize(1.0),
            title: i18n::N_(b"Decrease Font Size").to_vec(),
            description: i18n::N_(b"Decrease the font size by 1 point.").to_vec(),
        }],

        "reset_font_size" => vec![Command {
            action: Action::ResetFontSize,
            title: i18n::N_(b"Reset Font Size").to_vec(),
            description: i18n::N_(b"Reset the font size to the default.").to_vec(),
        }],

        "clear_screen" => vec![Command {
            action: Action::ClearScreen,
            title: i18n::N_(b"Clear Screen").to_vec(),
            description: i18n::N_(b"Clear the screen and scrollback.").to_vec(),
        }],

        "select_all" => vec![Command {
            action: Action::SelectAll,
            title: i18n::N_(b"Select All").to_vec(),
            description: i18n::N_(b"Select all text on the screen.").to_vec(),
        }],

        "scroll_to_top" => vec![Command {
            action: Action::ScrollToTop,
            title: i18n::N_(b"Scroll to Top").to_vec(),
            description: i18n::N_(b"Scroll to the top of the screen.").to_vec(),
        }],

        "scroll_to_bottom" => vec![Command {
            action: Action::ScrollToBottom,
            title: i18n::N_(b"Scroll to Bottom").to_vec(),
            description: i18n::N_(b"Scroll to the bottom of the screen.").to_vec(),
        }],

        "scroll_to_selection" => vec![Command {
            action: Action::ScrollToSelection,
            title: i18n::N_(b"Scroll to Selection").to_vec(),
            description: i18n::N_(b"Scroll to the selected text.").to_vec(),
        }],

        "scroll_page_up" => vec![Command {
            action: Action::ScrollPageUp,
            title: i18n::N_(b"Scroll Page Up").to_vec(),
            description: i18n::N_(b"Scroll the screen up by a page.").to_vec(),
        }],

        "scroll_page_down" => vec![Command {
            action: Action::ScrollPageDown,
            title: i18n::N_(b"Scroll Page Down").to_vec(),
            description: i18n::N_(b"Scroll the screen down by a page.").to_vec(),
        }],

        "write_screen_file" => vec![
            Command {
                action: Action::WriteScreenFile(WriteScreen::COPY),
                title: i18n::N_(b"Copy Screen to Temporary File and Copy Path").to_vec(),
                description: i18n::N_(b"Copy the screen contents to a temporary file and copy the path to the clipboard.").to_vec(),
            },
            Command {
                action: Action::WriteScreenFile(WriteScreen::PASTE),
                title: i18n::N_(b"Copy Screen to Temporary File and Paste Path").to_vec(),
                description: i18n::N_(b"Copy the screen contents to a temporary file and paste the path to the file.").to_vec(),
            },
            Command {
                action: Action::WriteScreenFile(WriteScreen::OPEN),
                title: i18n::N_(b"Copy Screen to Temporary File and Open").to_vec(),
                description: i18n::N_(b"Copy the screen contents to a temporary file and open it.").to_vec(),
            },
            Command {
                action: Action::WriteScreenFile(WriteScreen { action: WriteScreenAction::Copy, emit: WriteScreenFormat::Html }),
                title: i18n::N_(b"Copy Screen as HTML to Temporary File and Copy Path").to_vec(),
                description: i18n::N_(b"Copy the screen contents as HTML to a temporary file and copy the path to the clipboard.").to_vec(),
            },
            Command {
                action: Action::WriteScreenFile(WriteScreen { action: WriteScreenAction::Paste, emit: WriteScreenFormat::Html }),
                title: i18n::N_(b"Copy Screen as HTML to Temporary File and Paste Path").to_vec(),
                description: i18n::N_(b"Copy the screen contents as HTML to a temporary file and paste the path to the file.").to_vec(),
            },
            Command {
                action: Action::WriteScreenFile(WriteScreen { action: WriteScreenAction::Open, emit: WriteScreenFormat::Html }),
                title: i18n::N_(b"Copy Screen as HTML to Temporary File and Open").to_vec(),
                description: i18n::N_(b"Copy the screen contents as HTML to a temporary file and open it.").to_vec(),
            },
            Command {
                action: Action::WriteScreenFile(WriteScreen { action: WriteScreenAction::Copy, emit: WriteScreenFormat::Vt }),
                title: i18n::N_(b"Copy Screen as ANSI Sequences to Temporary File and Copy Path").to_vec(),
                description: i18n::N_(b"Copy the screen contents as ANSI escape sequences to a temporary file and copy the path to the clipboard.").to_vec(),
            },
            Command {
                action: Action::WriteScreenFile(WriteScreen { action: WriteScreenAction::Paste, emit: WriteScreenFormat::Vt }),
                title: i18n::N_(b"Copy Screen as ANSI Sequences to Temporary File and Paste Path").to_vec(),
                description: i18n::N_(b"Copy the screen contents as ANSI escape sequences to a temporary file and paste the path to the file.").to_vec(),
            },
            Command {
                action: Action::WriteScreenFile(WriteScreen { action: WriteScreenAction::Open, emit: WriteScreenFormat::Vt }),
                title: i18n::N_(b"Copy Screen as ANSI Sequences to Temporary File and Open").to_vec(),
                description: i18n::N_(b"Copy the screen contents as ANSI escape sequences to a temporary file and open it.").to_vec(),
            },
        ],

        "write_selection_file" => vec![
            Command {
                action: Action::WriteSelectionFile(WriteScreen::COPY),
                title: i18n::N_(b"Copy Selection to Temporary File and Copy Path").to_vec(),
                description: i18n::N_(b"Copy the selection contents to a temporary file and copy the path to the clipboard.").to_vec(),
            },
            Command {
                action: Action::WriteSelectionFile(WriteScreen::PASTE),
                title: i18n::N_(b"Copy Selection to Temporary File and Paste Path").to_vec(),
                description: i18n::N_(b"Copy the selection contents to a temporary file and paste the path to the file.").to_vec(),
            },
            Command {
                action: Action::WriteSelectionFile(WriteScreen::OPEN),
                title: i18n::N_(b"Copy Selection to Temporary File and Open").to_vec(),
                description: i18n::N_(b"Copy the selection contents to a temporary file and open it.").to_vec(),
            },
            Command {
                action: Action::WriteSelectionFile(WriteScreen { action: WriteScreenAction::Copy, emit: WriteScreenFormat::Html }),
                title: i18n::N_(b"Copy Selection as HTML to Temporary File and Copy Path").to_vec(),
                description: i18n::N_(b"Copy the selection contents as HTML to a temporary file and copy the path to the clipboard.").to_vec(),
            },
            Command {
                action: Action::WriteSelectionFile(WriteScreen { action: WriteScreenAction::Paste, emit: WriteScreenFormat::Html }),
                title: i18n::N_(b"Copy Selection as HTML to Temporary File and Paste Path").to_vec(),
                description: i18n::N_(b"Copy the selection contents as HTML to a temporary file and paste the path to the file.").to_vec(),
            },
            Command {
                action: Action::WriteSelectionFile(WriteScreen { action: WriteScreenAction::Open, emit: WriteScreenFormat::Html }),
                title: i18n::N_(b"Copy Selection as HTML to Temporary File and Open").to_vec(),
                description: i18n::N_(b"Copy the selection contents as HTML to a temporary file and open it.").to_vec(),
            },
            Command {
                action: Action::WriteSelectionFile(WriteScreen { action: WriteScreenAction::Copy, emit: WriteScreenFormat::Vt }),
                title: i18n::N_(b"Copy Selection as ANSI Sequences to Temporary File and Copy Path").to_vec(),
                description: i18n::N_(b"Copy the selection contents as ANSI escape sequences to a temporary file and copy the path to the clipboard.").to_vec(),
            },
            Command {
                action: Action::WriteSelectionFile(WriteScreen { action: WriteScreenAction::Paste, emit: WriteScreenFormat::Vt }),
                title: i18n::N_(b"Copy Selection as ANSI Sequences to Temporary File and Paste Path").to_vec(),
                description: i18n::N_(b"Copy the selection contents as ANSI escape sequences to a temporary file and paste the path to the file.").to_vec(),
            },
            Command {
                action: Action::WriteSelectionFile(WriteScreen { action: WriteScreenAction::Open, emit: WriteScreenFormat::Vt }),
                title: i18n::N_(b"Copy Selection as ANSI Sequences to Temporary File and Open").to_vec(),
                description: i18n::N_(b"Copy the selection contents as ANSI escape sequences to a temporary file and open it.").to_vec(),
            },
        ],

        "new_window" => vec![Command {
            action: Action::NewWindow,
            title: i18n::N_(b"New Window").to_vec(),
            description: i18n::N_(b"Open a new window.").to_vec(),
        }],

        "new_tab" => vec![Command {
            action: Action::NewTab,
            title: i18n::N_(b"New Tab").to_vec(),
            description: i18n::N_(b"Open a new tab.").to_vec(),
        }],

        "move_tab" => vec![
            Command {
                action: Action::MoveTab(-1),
                title: i18n::N_(b"Move Tab Left").to_vec(),
                description: i18n::N_(b"Move the current tab to the left.").to_vec(),
            },
            Command {
                action: Action::MoveTab(1),
                title: i18n::N_(b"Move Tab Right").to_vec(),
                description: i18n::N_(b"Move the current tab to the right.").to_vec(),
            },
        ],

        "move_tab_to_new_window" => vec![Command {
            action: Action::MoveTabToNewWindow,
            title: i18n::N_(b"Move Tab to New Window").to_vec(),
            description: i18n::N_(b"Move the current tab to a new window.").to_vec(),
        }],

        "toggle_tab_overview" => vec![Command {
            action: Action::ToggleTabOverview,
            title: i18n::N_(b"Toggle Tab Overview").to_vec(),
            description: i18n::N_(b"Toggle the tab overview.").to_vec(),
        }],

        "prompt_surface_title" => vec![Command {
            action: Action::PromptSurfaceTitle,
            title: i18n::N_("Change Terminal Title…".as_bytes()).to_vec(),
            description: i18n::N_(b"Prompt for a new title for the current terminal.").to_vec(),
        }],

        "prompt_tab_title" => vec![Command {
            action: Action::PromptTabTitle,
            title: i18n::N_("Change Tab Title…".as_bytes()).to_vec(),
            description: i18n::N_(b"Prompt for a new title for the current tab.").to_vec(),
        }],

        "prompt_window_title" => vec![Command {
            action: Action::PromptWindowTitle,
            title: "Change Window Title…".as_bytes().to_vec(),
            description: b"Prompt for a new title for the current window.".to_vec(),
        }],

        "new_split" => vec![
            Command {
                action: Action::NewSplit(SplitDirection::Left),
                title: i18n::N_(b"Split Left").to_vec(),
                description: i18n::N_(b"Split the terminal to the left.").to_vec(),
            },
            Command {
                action: Action::NewSplit(SplitDirection::Right),
                title: i18n::N_(b"Split Right").to_vec(),
                description: i18n::N_(b"Split the terminal to the right.").to_vec(),
            },
            Command {
                action: Action::NewSplit(SplitDirection::Up),
                title: i18n::N_(b"Split Up").to_vec(),
                description: i18n::N_(b"Split the terminal up.").to_vec(),
            },
            Command {
                action: Action::NewSplit(SplitDirection::Down),
                title: i18n::N_(b"Split Down").to_vec(),
                description: i18n::N_(b"Split the terminal down.").to_vec(),
            },
        ],

        "goto_split" => vec![
            Command {
                action: Action::GotoSplit(SplitFocusDirection::Previous),
                title: i18n::N_(b"Focus Split: Previous").to_vec(),
                description: i18n::N_(b"Focus the previous split, if any.").to_vec(),
            },
            Command {
                action: Action::GotoSplit(SplitFocusDirection::Next),
                title: i18n::N_(b"Focus Split: Next").to_vec(),
                description: i18n::N_(b"Focus the next split, if any.").to_vec(),
            },
            Command {
                action: Action::GotoSplit(SplitFocusDirection::Left),
                title: i18n::N_(b"Focus Split: Left").to_vec(),
                description: i18n::N_(b"Focus the split to the left, if it exists.").to_vec(),
            },
            Command {
                action: Action::GotoSplit(SplitFocusDirection::Right),
                title: i18n::N_(b"Focus Split: Right").to_vec(),
                description: i18n::N_(b"Focus the split to the right, if it exists.").to_vec(),
            },
            Command {
                action: Action::GotoSplit(SplitFocusDirection::Up),
                title: i18n::N_(b"Focus Split: Up").to_vec(),
                description: i18n::N_(b"Focus the split above, if it exists.").to_vec(),
            },
            Command {
                action: Action::GotoSplit(SplitFocusDirection::Down),
                title: i18n::N_(b"Focus Split: Down").to_vec(),
                description: i18n::N_(b"Focus the split below, if it exists.").to_vec(),
            },
        ],

        "goto_window" => vec![
            Command {
                action: Action::GotoWindow(GotoWindow::Previous),
                title: i18n::N_(b"Focus Window: Previous").to_vec(),
                description: i18n::N_(b"Focus the previous window, if any.").to_vec(),
            },
            Command {
                action: Action::GotoWindow(GotoWindow::Next),
                title: i18n::N_(b"Focus Window: Next").to_vec(),
                description: i18n::N_(b"Focus the next window, if any.").to_vec(),
            },
        ],

        "toggle_split_zoom" => vec![Command {
            action: Action::ToggleSplitZoom,
            title: i18n::N_(b"Toggle Split Zoom").to_vec(),
            description: i18n::N_(b"Toggle the zoom state of the current split.").to_vec(),
        }],

        "toggle_readonly" => vec![Command {
            action: Action::ToggleReadonly,
            title: i18n::N_(b"Toggle Read-Only Mode").to_vec(),
            description: i18n::N_(b"Toggle read-only mode for the current surface.").to_vec(),
        }],

        "equalize_splits" => vec![Command {
            action: Action::EqualizeSplits,
            title: i18n::N_(b"Equalize Splits").to_vec(),
            description: i18n::N_(b"Equalize the size of all splits.").to_vec(),
        }],

        "reset_window_size" => vec![Command {
            action: Action::ResetWindowSize,
            title: i18n::N_(b"Reset Window Size").to_vec(),
            description: i18n::N_(b"Reset the window size to the default.").to_vec(),
        }],

        "inspector" => vec![Command {
            action: Action::Inspector(InspectorMode::Toggle),
            title: i18n::N_(b"Toggle Inspector").to_vec(),
            description: i18n::N_(b"Toggle the inspector.").to_vec(),
        }],

        "show_gtk_inspector" => vec![Command {
            action: Action::ShowGtkInspector,
            title: i18n::N_(b"Show the GTK Inspector").to_vec(),
            description: i18n::N_(b"Show the GTK inspector.").to_vec(),
        }],

        "show_on_screen_keyboard" => vec![Command {
            action: Action::ShowOnScreenKeyboard,
            title: i18n::N_(b"Show On-Screen Keyboard").to_vec(),
            description: i18n::N_(b"Show the on-screen keyboard if present.").to_vec(),
        }],

        "open_config" => vec![
            Command {
                action: Action::OpenConfig(OpenConfig::OsOpen),
                title: i18n::N_(b"Open Config Using OS editor").to_vec(),
                description: i18n::N_(b"Open the config file with the OS's default editor.").to_vec(),
            },
            Command {
                action: Action::OpenConfig(OpenConfig::NewWindow),
                title: i18n::N_(b"Open Config in New Terminal Window").to_vec(),
                description: i18n::N_(b"Open the config file in a new window using $EDITOR or $VISUAL.").to_vec(),
            },
        ],

        "reload_config" => vec![Command {
            action: Action::ReloadConfig,
            title: i18n::N_(b"Reload Config").to_vec(),
            description: i18n::N_(b"Reload the config file.").to_vec(),
        }],

        "close_surface" => vec![Command {
            action: Action::CloseSurface,
            title: i18n::N_(b"Close Terminal").to_vec(),
            description: i18n::N_(b"Close the current terminal.").to_vec(),
        }],

        "close_tab" => vec![
            Command {
                action: Action::CloseTab(CloseTabMode::This),
                title: i18n::N_(b"Close Tab").to_vec(),
                description: i18n::N_(b"Close the current tab.").to_vec(),
            },
            Command {
                action: Action::CloseTab(CloseTabMode::Other),
                title: i18n::N_(b"Close Other Tabs").to_vec(),
                description: i18n::N_(b"Close all tabs in this window except the current one.").to_vec(),
            },
            Command {
                action: Action::CloseTab(CloseTabMode::Right),
                title: i18n::N_(b"Close Tabs to the Right").to_vec(),
                description: i18n::N_(b"Close all tabs to the right of the current one.").to_vec(),
            },
        ],

        "close_window" => vec![Command {
            action: Action::CloseWindow,
            title: i18n::N_(b"Close Window").to_vec(),
            description: i18n::N_(b"Close the current window.").to_vec(),
        }],

        "close_all_windows" => vec![Command {
            action: Action::CloseAllWindows,
            title: i18n::N_(b"Close All Windows").to_vec(),
            description: i18n::N_(b"Close all windows.").to_vec(),
        }],

        "toggle_maximize" => vec![Command {
            action: Action::ToggleMaximize,
            title: i18n::N_(b"Toggle Maximize").to_vec(),
            description: i18n::N_(b"Toggle the maximized state of the current window.").to_vec(),
        }],

        "toggle_fullscreen" => vec![Command {
            action: Action::ToggleFullscreen,
            title: i18n::N_(b"Toggle Fullscreen").to_vec(),
            description: i18n::N_(b"Toggle the fullscreen state of the current window.").to_vec(),
        }],

        "toggle_window_decorations" => vec![Command {
            action: Action::ToggleWindowDecorations,
            title: i18n::N_(b"Toggle Window Decorations").to_vec(),
            description: i18n::N_(b"Toggle the window decorations.").to_vec(),
        }],

        "toggle_window_float_on_top" => vec![Command {
            action: Action::ToggleWindowFloatOnTop,
            title: i18n::N_(b"Toggle Float on Top").to_vec(),
            description: i18n::N_(b"Toggle the float on top state of the current window.").to_vec(),
        }],

        "toggle_secure_input" => vec![Command {
            action: Action::ToggleSecureInput,
            title: i18n::N_(b"Toggle Secure Input").to_vec(),
            description: i18n::N_(b"Toggle secure input mode.").to_vec(),
        }],

        "toggle_mouse_reporting" => vec![Command {
            action: Action::ToggleMouseReporting,
            title: i18n::N_(b"Toggle Mouse Reporting").to_vec(),
            description: i18n::N_(b"Toggle whether mouse events are reported to terminal applications.").to_vec(),
        }],

        "toggle_background_opacity" => vec![Command {
            action: Action::ToggleBackgroundOpacity,
            title: i18n::N_(b"Toggle Background Opacity").to_vec(),
            description: i18n::N_(b"Toggle the background opacity of a window that started transparent.").to_vec(),
        }],

        "check_for_updates" => vec![Command {
            action: Action::CheckForUpdates,
            title: i18n::N_(b"Check for Updates").to_vec(),
            description: i18n::N_(b"Check for updates to the application.").to_vec(),
        }],

        "undo" => vec![Command {
            action: Action::Undo,
            title: i18n::N_(b"Undo").to_vec(),
            description: i18n::N_(b"Undo the last action.").to_vec(),
        }],

        "redo" => vec![Command {
            action: Action::Redo,
            title: i18n::N_(b"Redo").to_vec(),
            description: i18n::N_(b"Redo the last undone action.").to_vec(),
        }],

        "quit" => vec![Command {
            action: Action::Quit,
            title: i18n::N_(b"Quit").to_vec(),
            description: i18n::N_(b"Quit the application.").to_vec(),
        }],

        "text" => vec![Command {
            action: Action::Text("👻".as_bytes().to_vec()),
            title: i18n::N_(b"Rustty").to_vec(),
            description: i18n::N_(b"Put a little Rustty in your terminal.").to_vec(),
        }],

        // No commands because they're parameterized and there
        // aren't obvious values users would use. It is possible that
        // these may have commands in the future if there are very
        // common values that users tend to use.
        "csi"
        | "esc"
        | "cursor_key"
        | "set_font_size"
        | "set_surface_title"
        | "set_tab_title"
        | "set_window_title"
        | "search"
        | "scroll_to_row"
        | "scroll_page_fractional"
        | "scroll_page_lines"
        | "adjust_selection"
        | "jump_to_prompt"
        | "write_scrollback_file"
        | "goto_tab"
        | "resize_split"
        | "activate_key_table"
        | "activate_key_table_once"
        | "deactivate_key_table"
        | "deactivate_all_key_tables"
        | "end_key_sequence"
        | "crash" => vec![],

        // No commands because I'm not sure they make sense in a command
        // palette context.
        "toggle_command_palette"
        | "toggle_quick_terminal"
        | "toggle_visibility"
        | "previous_tab"
        | "next_tab"
        | "last_tab" => vec![],

        // No commands for obvious reasons
        "ignore" | "unbind" => vec![],

        // PORT NOTE: the reference switch is exhaustive over `Action.Key`; an
        // unknown tag would be a comptime error. Here an unmatched name is
        // unreachable given `Action::FIELD_NAMES` is the single source-of-truth.
        _ => unreachable!("unknown action key: {action}"),
    };

    // All generated commands should have the same action as the
    // action passed in.
    // for (result) |cmd| assert(cmd.action == action);
    for cmd in result.iter() {
        assert(cmd.action.tag_name() == action);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_defaults() {
        // This just ensures that defaults is analyzed and works.
        // try testing.expect(defaults.len > 0);
        assert!(!defaults().is_empty());
        // try testing.expectEqual(defaults.len, defaultsC.len);
        assert_eq!(defaults().len(), defaults_c().len());
    }

    // S1-F1/S103-F1: `include/rustty-embed.h` declares `rustty_command_s` as
    // four NUL-terminated `const char*` (32 bytes). `C` must match that layout
    // exactly, or the macOS app's 32-byte-stride `UnsafeBufferPointer<rustty_command_s>`
    // walk over `rustty_config_get(cfg, &list, "command-palette-entry", ..)`
    // reads Vec headers as if they were char pointers.
    #[test]
    fn command_c_matches_the_header_abi() {
        use std::os::raw::c_char;
        // include/rustty-embed.h declares rustty_command_s as 4x const char*.
        assert_eq!(
            std::mem::size_of::<C>(),
            4 * std::mem::size_of::<*const c_char>()
        );
        assert_eq!(
            std::mem::align_of::<C>(),
            std::mem::align_of::<*const c_char>()
        );
        assert_eq!(std::mem::offset_of!(C, action_key), 0);
        assert_eq!(
            std::mem::offset_of!(C, action),
            std::mem::size_of::<*const c_char>()
        );
        assert_eq!(
            std::mem::offset_of!(C, title),
            2 * std::mem::size_of::<*const c_char>()
        );
        assert_eq!(
            std::mem::offset_of!(C, description),
            3 * std::mem::size_of::<*const c_char>()
        );
    }

    // PORT NOTE (test adaptation): the task brief's sketch constructs
    // `Command { action_key, action, title, description }` as four raw
    // `Vec<u8>` literals ("key"/"act"/"ttl"/"desc"). The actual `Command` type
    // in this file has only `action: Action, title: Vec<u8>, description:
    // Vec<u8>` — there is no `action_key` field; `action_key` is derived from
    // `Action::tag_name()` at `CommandCStorage::new` time, and `action` is the
    // formatted `Action` (see `Action::format`), not a raw byte literal. This
    // test uses a real `Action` variant (`Action::Reset`, a void/unit arm) so
    // `tag_name()` and `format()` both yield `"reset"`, and checks all four
    // fields independently (title/description are distinct literals) so the
    // assertions are not tautological.
    #[test]
    fn command_c_fields_are_nul_terminated_and_readable() {
        let cmd = Command {
            action: Action::Reset,
            title: b"ttl".to_vec(),
            description: b"desc".to_vec(),
        };
        let owned = CommandCStorage::new(&cmd);
        let c = owned.as_c();
        unsafe {
            assert_eq!(std::ffi::CStr::from_ptr(c.action_key).to_bytes(), b"reset");
            assert_eq!(std::ffi::CStr::from_ptr(c.action).to_bytes(), b"reset");
            assert_eq!(std::ffi::CStr::from_ptr(c.title).to_bytes(), b"ttl");
            assert_eq!(std::ffi::CStr::from_ptr(c.description).to_bytes(), b"desc");
        }
    }
}
