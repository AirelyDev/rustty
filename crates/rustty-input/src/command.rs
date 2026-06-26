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

/// ghostty_command_s
//
// PORT NOTE: `Command.C` is an `extern struct` of four `[*:0]const u8`
// NUL-terminated C strings. Modeled as owned NUL-terminated byte buffers; the
// `action_key`/`action` strings come from `@tagName`/the formatted action.
#[derive(Clone, Debug, PartialEq)]
pub struct C {
    pub action_key: Vec<u8>,
    pub action: Vec<u8>,
    pub title: Vec<u8>,
    pub description: Vec<u8>,
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

    /// Convert this command to a C struct at comptime.
    //
    // PORT NOTE: the reference `comptimeCval` asserts `@inComptime()` and uses
    // `std.fmt.comptimePrint("{f}", .{self.action})`. Rust has no comptime; this
    // produces the same C form at runtime by formatting the action via the
    // ported `Action::format`. The `@inComptime()` assert has no runtime analog
    // and is dropped (it is a build-context guard, not a behavioral check).
    pub fn comptime_cval(&self) -> C {
        // assert(@inComptime());
        // (see PORT NOTE: no runtime analog)

        // return .{
        //     .action_key = @tagName(self.action),
        //     .action = std.fmt.comptimePrint("{f}", .{self.action}),
        //     .title = self.title,
        //     .description = self.description,
        // };
        let mut action = AllocatingWriter::init();
        // format never fails (Infallible).
        let _ = self.action.format(&mut action);
        C {
            action_key: cstr(self.action.tag_name().as_bytes()),
            action: cstr(action.written()),
            title: cstr(&self.title),
            description: cstr(&self.description),
        }
    }

    /// Convert this command to a C struct at runtime.
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
    #[allow(clippy::result_unit_err)]
    pub fn cval(&self) -> Result<C, ()> {
        // var buf: std.Io.Writer.Allocating = .init(alloc);
        // defer buf.deinit();
        let mut buf = AllocatingWriter::init();
        // self.action.format(&buf.writer) catch return error.OutOfMemory;
        // (format is Infallible here)
        let _ = self.action.format(&mut buf);
        // const action = try buf.toOwnedSliceSentinel(0);
        let action = cstr(buf.written());

        // return .{
        //     .action_key = @tagName(self.action),
        //     .action = action.ptr,
        //     .title = self.title,
        //     .description = self.description,
        // };
        Ok(C {
            action_key: cstr(self.action.tag_name().as_bytes()),
            action,
            title: cstr(&self.title),
            description: cstr(&self.description),
        })
    }

    /// Implements a comparison function for std.mem.sortUnstable
    /// and similar functions. The sorting is defined by rustty
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

// PORT NOTE: `[:0]const u8` / `[*:0]const u8` — produce a NUL-terminated owned
// copy of the given bytes.
fn cstr(bytes: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(bytes.len() + 1);
    v.extend_from_slice(bytes);
    v.push(0);
    v
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
// PORT NOTE: the reference `defaultsC` mirrors `defaults` via `comptimeCval`.
pub fn defaults_c() -> Vec<C> {
    // @setEvalBranchQuota(100_000);
    // var result: [defaults.len]Command.C = undefined;
    // for (defaults, 0..) |cmd, i| result[i] = cmd.comptimeCval();
    let d = defaults();
    let mut result: Vec<C> = Vec::with_capacity(d.len());
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
            title: b"Reset Terminal".to_vec(),
            description: b"Reset the terminal to a clean state.".to_vec(),
        }],

        "copy_to_clipboard" => vec![
            Command {
                action: Action::CopyToClipboard(CopyToClipboard::Mixed),
                title: b"Copy to Clipboard".to_vec(),
                description: b"Copy the selected text to the clipboard in both plain and styled formats.".to_vec(),
            },
            Command {
                action: Action::CopyToClipboard(CopyToClipboard::Plain),
                title: b"Copy Selection as Plain Text to Clipboard".to_vec(),
                description: b"Copy the selected text as plain text to the clipboard.".to_vec(),
            },
            Command {
                action: Action::CopyToClipboard(CopyToClipboard::Vt),
                title: b"Copy Selection as ANSI Sequences to Clipboard".to_vec(),
                description: b"Copy the selected text as ANSI escape sequences to the clipboard.".to_vec(),
            },
            Command {
                action: Action::CopyToClipboard(CopyToClipboard::Html),
                title: b"Copy Selection as HTML to Clipboard".to_vec(),
                description: b"Copy the selected text as HTML to the clipboard.".to_vec(),
            },
        ],

        "copy_url_to_clipboard" => vec![Command {
            action: Action::CopyUrlToClipboard,
            title: b"Copy URL to Clipboard".to_vec(),
            description: b"Copy the URL under the cursor to the clipboard.".to_vec(),
        }],

        "copy_title_to_clipboard" => vec![Command {
            action: Action::CopyTitleToClipboard,
            title: b"Copy Terminal Title to Clipboard".to_vec(),
            description: b"Copy the terminal title to the clipboard. If the terminal title is not set this has no effect.".to_vec(),
        }],

        "paste_from_clipboard" => vec![Command {
            action: Action::PasteFromClipboard,
            title: b"Paste from Clipboard".to_vec(),
            description: b"Paste the contents of the main clipboard.".to_vec(),
        }],

        "paste_from_selection" => vec![Command {
            action: Action::PasteFromSelection,
            title: b"Paste from Selection".to_vec(),
            description: b"Paste the contents of the selection clipboard.".to_vec(),
        }],

        "start_search" => vec![Command {
            action: Action::StartSearch,
            title: b"Start Search".to_vec(),
            description: b"Start a search if one isn't already active.".to_vec(),
        }],

        "search_selection" => vec![Command {
            action: Action::SearchSelection,
            title: b"Search Selection".to_vec(),
            description: b"Start a search for the current text selection.".to_vec(),
        }],

        "end_search" => vec![Command {
            action: Action::EndSearch,
            title: b"End Search".to_vec(),
            description: b"End the current search if any and hide any GUI elements.".to_vec(),
        }],

        "navigate_search" => vec![
            Command {
                action: Action::NavigateSearch(NavigateSearch::Next),
                title: b"Next Search Result".to_vec(),
                description: b"Navigate to the next search result, if any.".to_vec(),
            },
            Command {
                action: Action::NavigateSearch(NavigateSearch::Previous),
                title: b"Previous Search Result".to_vec(),
                description: b"Navigate to the previous search result, if any.".to_vec(),
            },
        ],

        "increase_font_size" => vec![Command {
            action: Action::IncreaseFontSize(1.0),
            title: b"Increase Font Size".to_vec(),
            description: b"Increase the font size by 1 point.".to_vec(),
        }],

        "decrease_font_size" => vec![Command {
            action: Action::DecreaseFontSize(1.0),
            title: b"Decrease Font Size".to_vec(),
            description: b"Decrease the font size by 1 point.".to_vec(),
        }],

        "reset_font_size" => vec![Command {
            action: Action::ResetFontSize,
            title: b"Reset Font Size".to_vec(),
            description: b"Reset the font size to the default.".to_vec(),
        }],

        "clear_screen" => vec![Command {
            action: Action::ClearScreen,
            title: b"Clear Screen".to_vec(),
            description: b"Clear the screen and scrollback.".to_vec(),
        }],

        "select_all" => vec![Command {
            action: Action::SelectAll,
            title: b"Select All".to_vec(),
            description: b"Select all text on the screen.".to_vec(),
        }],

        "scroll_to_top" => vec![Command {
            action: Action::ScrollToTop,
            title: b"Scroll to Top".to_vec(),
            description: b"Scroll to the top of the screen.".to_vec(),
        }],

        "scroll_to_bottom" => vec![Command {
            action: Action::ScrollToBottom,
            title: b"Scroll to Bottom".to_vec(),
            description: b"Scroll to the bottom of the screen.".to_vec(),
        }],

        "scroll_to_selection" => vec![Command {
            action: Action::ScrollToSelection,
            title: b"Scroll to Selection".to_vec(),
            description: b"Scroll to the selected text.".to_vec(),
        }],

        "scroll_page_up" => vec![Command {
            action: Action::ScrollPageUp,
            title: b"Scroll Page Up".to_vec(),
            description: b"Scroll the screen up by a page.".to_vec(),
        }],

        "scroll_page_down" => vec![Command {
            action: Action::ScrollPageDown,
            title: b"Scroll Page Down".to_vec(),
            description: b"Scroll the screen down by a page.".to_vec(),
        }],

        "write_screen_file" => vec![
            Command {
                action: Action::WriteScreenFile(WriteScreen::COPY),
                title: b"Copy Screen to Temporary File and Copy Path".to_vec(),
                description: b"Copy the screen contents to a temporary file and copy the path to the clipboard.".to_vec(),
            },
            Command {
                action: Action::WriteScreenFile(WriteScreen::PASTE),
                title: b"Copy Screen to Temporary File and Paste Path".to_vec(),
                description: b"Copy the screen contents to a temporary file and paste the path to the file.".to_vec(),
            },
            Command {
                action: Action::WriteScreenFile(WriteScreen::OPEN),
                title: b"Copy Screen to Temporary File and Open".to_vec(),
                description: b"Copy the screen contents to a temporary file and open it.".to_vec(),
            },
            Command {
                action: Action::WriteScreenFile(WriteScreen { action: WriteScreenAction::Copy, emit: WriteScreenFormat::Html }),
                title: b"Copy Screen as HTML to Temporary File and Copy Path".to_vec(),
                description: b"Copy the screen contents as HTML to a temporary file and copy the path to the clipboard.".to_vec(),
            },
            Command {
                action: Action::WriteScreenFile(WriteScreen { action: WriteScreenAction::Paste, emit: WriteScreenFormat::Html }),
                title: b"Copy Screen as HTML to Temporary File and Paste Path".to_vec(),
                description: b"Copy the screen contents as HTML to a temporary file and paste the path to the file.".to_vec(),
            },
            Command {
                action: Action::WriteScreenFile(WriteScreen { action: WriteScreenAction::Open, emit: WriteScreenFormat::Html }),
                title: b"Copy Screen as HTML to Temporary File and Open".to_vec(),
                description: b"Copy the screen contents as HTML to a temporary file and open it.".to_vec(),
            },
            Command {
                action: Action::WriteScreenFile(WriteScreen { action: WriteScreenAction::Copy, emit: WriteScreenFormat::Vt }),
                title: b"Copy Screen as ANSI Sequences to Temporary File and Copy Path".to_vec(),
                description: b"Copy the screen contents as ANSI escape sequences to a temporary file and copy the path to the clipboard.".to_vec(),
            },
            Command {
                action: Action::WriteScreenFile(WriteScreen { action: WriteScreenAction::Paste, emit: WriteScreenFormat::Vt }),
                title: b"Copy Screen as ANSI Sequences to Temporary File and Paste Path".to_vec(),
                description: b"Copy the screen contents as ANSI escape sequences to a temporary file and paste the path to the file.".to_vec(),
            },
            Command {
                action: Action::WriteScreenFile(WriteScreen { action: WriteScreenAction::Open, emit: WriteScreenFormat::Vt }),
                title: b"Copy Screen as ANSI Sequences to Temporary File and Open".to_vec(),
                description: b"Copy the screen contents as ANSI escape sequences to a temporary file and open it.".to_vec(),
            },
        ],

        "write_selection_file" => vec![
            Command {
                action: Action::WriteSelectionFile(WriteScreen::COPY),
                title: b"Copy Selection to Temporary File and Copy Path".to_vec(),
                description: b"Copy the selection contents to a temporary file and copy the path to the clipboard.".to_vec(),
            },
            Command {
                action: Action::WriteSelectionFile(WriteScreen::PASTE),
                title: b"Copy Selection to Temporary File and Paste Path".to_vec(),
                description: b"Copy the selection contents to a temporary file and paste the path to the file.".to_vec(),
            },
            Command {
                action: Action::WriteSelectionFile(WriteScreen::OPEN),
                title: b"Copy Selection to Temporary File and Open".to_vec(),
                description: b"Copy the selection contents to a temporary file and open it.".to_vec(),
            },
            Command {
                action: Action::WriteSelectionFile(WriteScreen { action: WriteScreenAction::Copy, emit: WriteScreenFormat::Html }),
                title: b"Copy Selection as HTML to Temporary File and Copy Path".to_vec(),
                description: b"Copy the selection contents as HTML to a temporary file and copy the path to the clipboard.".to_vec(),
            },
            Command {
                action: Action::WriteSelectionFile(WriteScreen { action: WriteScreenAction::Paste, emit: WriteScreenFormat::Html }),
                title: b"Copy Selection as HTML to Temporary File and Paste Path".to_vec(),
                description: b"Copy the selection contents as HTML to a temporary file and paste the path to the file.".to_vec(),
            },
            Command {
                action: Action::WriteSelectionFile(WriteScreen { action: WriteScreenAction::Open, emit: WriteScreenFormat::Html }),
                title: b"Copy Selection as HTML to Temporary File and Open".to_vec(),
                description: b"Copy the selection contents as HTML to a temporary file and open it.".to_vec(),
            },
            Command {
                action: Action::WriteSelectionFile(WriteScreen { action: WriteScreenAction::Copy, emit: WriteScreenFormat::Vt }),
                title: b"Copy Selection as ANSI Sequences to Temporary File and Copy Path".to_vec(),
                description: b"Copy the selection contents as ANSI escape sequences to a temporary file and copy the path to the clipboard.".to_vec(),
            },
            Command {
                action: Action::WriteSelectionFile(WriteScreen { action: WriteScreenAction::Paste, emit: WriteScreenFormat::Vt }),
                title: b"Copy Selection as ANSI Sequences to Temporary File and Paste Path".to_vec(),
                description: b"Copy the selection contents as ANSI escape sequences to a temporary file and paste the path to the file.".to_vec(),
            },
            Command {
                action: Action::WriteSelectionFile(WriteScreen { action: WriteScreenAction::Open, emit: WriteScreenFormat::Vt }),
                title: b"Copy Selection as ANSI Sequences to Temporary File and Open".to_vec(),
                description: b"Copy the selection contents as ANSI escape sequences to a temporary file and open it.".to_vec(),
            },
        ],

        "new_window" => vec![Command {
            action: Action::NewWindow,
            title: b"New Window".to_vec(),
            description: b"Open a new window.".to_vec(),
        }],

        "new_tab" => vec![Command {
            action: Action::NewTab,
            title: b"New Tab".to_vec(),
            description: b"Open a new tab.".to_vec(),
        }],

        "move_tab" => vec![
            Command {
                action: Action::MoveTab(-1),
                title: b"Move Tab Left".to_vec(),
                description: b"Move the current tab to the left.".to_vec(),
            },
            Command {
                action: Action::MoveTab(1),
                title: b"Move Tab Right".to_vec(),
                description: b"Move the current tab to the right.".to_vec(),
            },
        ],

        "toggle_tab_overview" => vec![Command {
            action: Action::ToggleTabOverview,
            title: b"Toggle Tab Overview".to_vec(),
            description: b"Toggle the tab overview.".to_vec(),
        }],

        "prompt_surface_title" => vec![Command {
            action: Action::PromptSurfaceTitle,
            title: "Change Terminal Title…".as_bytes().to_vec(),
            description: b"Prompt for a new title for the current terminal.".to_vec(),
        }],

        "prompt_tab_title" => vec![Command {
            action: Action::PromptTabTitle,
            title: "Change Tab Title…".as_bytes().to_vec(),
            description: b"Prompt for a new title for the current tab.".to_vec(),
        }],

        "new_split" => vec![
            Command {
                action: Action::NewSplit(SplitDirection::Left),
                title: b"Split Left".to_vec(),
                description: b"Split the terminal to the left.".to_vec(),
            },
            Command {
                action: Action::NewSplit(SplitDirection::Right),
                title: b"Split Right".to_vec(),
                description: b"Split the terminal to the right.".to_vec(),
            },
            Command {
                action: Action::NewSplit(SplitDirection::Up),
                title: b"Split Up".to_vec(),
                description: b"Split the terminal up.".to_vec(),
            },
            Command {
                action: Action::NewSplit(SplitDirection::Down),
                title: b"Split Down".to_vec(),
                description: b"Split the terminal down.".to_vec(),
            },
        ],

        "goto_split" => vec![
            Command {
                action: Action::GotoSplit(SplitFocusDirection::Previous),
                title: b"Focus Split: Previous".to_vec(),
                description: b"Focus the previous split, if any.".to_vec(),
            },
            Command {
                action: Action::GotoSplit(SplitFocusDirection::Next),
                title: b"Focus Split: Next".to_vec(),
                description: b"Focus the next split, if any.".to_vec(),
            },
            Command {
                action: Action::GotoSplit(SplitFocusDirection::Left),
                title: b"Focus Split: Left".to_vec(),
                description: b"Focus the split to the left, if it exists.".to_vec(),
            },
            Command {
                action: Action::GotoSplit(SplitFocusDirection::Right),
                title: b"Focus Split: Right".to_vec(),
                description: b"Focus the split to the right, if it exists.".to_vec(),
            },
            Command {
                action: Action::GotoSplit(SplitFocusDirection::Up),
                title: b"Focus Split: Up".to_vec(),
                description: b"Focus the split above, if it exists.".to_vec(),
            },
            Command {
                action: Action::GotoSplit(SplitFocusDirection::Down),
                title: b"Focus Split: Down".to_vec(),
                description: b"Focus the split below, if it exists.".to_vec(),
            },
        ],

        "goto_window" => vec![
            Command {
                action: Action::GotoWindow(GotoWindow::Previous),
                title: b"Focus Window: Previous".to_vec(),
                description: b"Focus the previous window, if any.".to_vec(),
            },
            Command {
                action: Action::GotoWindow(GotoWindow::Next),
                title: b"Focus Window: Next".to_vec(),
                description: b"Focus the next window, if any.".to_vec(),
            },
        ],

        "toggle_split_zoom" => vec![Command {
            action: Action::ToggleSplitZoom,
            title: b"Toggle Split Zoom".to_vec(),
            description: b"Toggle the zoom state of the current split.".to_vec(),
        }],

        "toggle_readonly" => vec![Command {
            action: Action::ToggleReadonly,
            title: b"Toggle Read-Only Mode".to_vec(),
            description: b"Toggle read-only mode for the current surface.".to_vec(),
        }],

        "equalize_splits" => vec![Command {
            action: Action::EqualizeSplits,
            title: b"Equalize Splits".to_vec(),
            description: b"Equalize the size of all splits.".to_vec(),
        }],

        "reset_window_size" => vec![Command {
            action: Action::ResetWindowSize,
            title: b"Reset Window Size".to_vec(),
            description: b"Reset the window size to the default.".to_vec(),
        }],

        "inspector" => vec![Command {
            action: Action::Inspector(InspectorMode::Toggle),
            title: b"Toggle Inspector".to_vec(),
            description: b"Toggle the inspector.".to_vec(),
        }],

        "show_gtk_inspector" => vec![Command {
            action: Action::ShowGtkInspector,
            title: b"Show the GTK Inspector".to_vec(),
            description: b"Show the GTK inspector.".to_vec(),
        }],

        "show_on_screen_keyboard" => vec![Command {
            action: Action::ShowOnScreenKeyboard,
            title: b"Show On-Screen Keyboard".to_vec(),
            description: b"Show the on-screen keyboard if present.".to_vec(),
        }],

        "open_config" => vec![Command {
            action: Action::OpenConfig,
            title: b"Open Config".to_vec(),
            description: b"Open the config file.".to_vec(),
        }],

        "reload_config" => vec![Command {
            action: Action::ReloadConfig,
            title: b"Reload Config".to_vec(),
            description: b"Reload the config file.".to_vec(),
        }],

        "close_surface" => vec![Command {
            action: Action::CloseSurface,
            title: b"Close Terminal".to_vec(),
            description: b"Close the current terminal.".to_vec(),
        }],

        "close_tab" => vec![
            Command {
                action: Action::CloseTab(CloseTabMode::This),
                title: b"Close Tab".to_vec(),
                description: b"Close the current tab.".to_vec(),
            },
            Command {
                action: Action::CloseTab(CloseTabMode::Other),
                title: b"Close Other Tabs".to_vec(),
                description: b"Close all tabs in this window except the current one.".to_vec(),
            },
            Command {
                action: Action::CloseTab(CloseTabMode::Right),
                title: b"Close Tabs to the Right".to_vec(),
                description: b"Close all tabs to the right of the current one.".to_vec(),
            },
        ],

        "close_window" => vec![Command {
            action: Action::CloseWindow,
            title: b"Close Window".to_vec(),
            description: b"Close the current window.".to_vec(),
        }],

        "close_all_windows" => vec![Command {
            action: Action::CloseAllWindows,
            title: b"Close All Windows".to_vec(),
            description: b"Close all windows.".to_vec(),
        }],

        "toggle_maximize" => vec![Command {
            action: Action::ToggleMaximize,
            title: b"Toggle Maximize".to_vec(),
            description: b"Toggle the maximized state of the current window.".to_vec(),
        }],

        "toggle_fullscreen" => vec![Command {
            action: Action::ToggleFullscreen,
            title: b"Toggle Fullscreen".to_vec(),
            description: b"Toggle the fullscreen state of the current window.".to_vec(),
        }],

        "toggle_window_decorations" => vec![Command {
            action: Action::ToggleWindowDecorations,
            title: b"Toggle Window Decorations".to_vec(),
            description: b"Toggle the window decorations.".to_vec(),
        }],

        "toggle_window_float_on_top" => vec![Command {
            action: Action::ToggleWindowFloatOnTop,
            title: b"Toggle Float on Top".to_vec(),
            description: b"Toggle the float on top state of the current window.".to_vec(),
        }],

        "toggle_secure_input" => vec![Command {
            action: Action::ToggleSecureInput,
            title: b"Toggle Secure Input".to_vec(),
            description: b"Toggle secure input mode.".to_vec(),
        }],

        "toggle_mouse_reporting" => vec![Command {
            action: Action::ToggleMouseReporting,
            title: b"Toggle Mouse Reporting".to_vec(),
            description: b"Toggle whether mouse events are reported to terminal applications.".to_vec(),
        }],

        "toggle_background_opacity" => vec![Command {
            action: Action::ToggleBackgroundOpacity,
            title: b"Toggle Background Opacity".to_vec(),
            description: b"Toggle the background opacity of a window that started transparent.".to_vec(),
        }],

        "check_for_updates" => vec![Command {
            action: Action::CheckForUpdates,
            title: b"Check for Updates".to_vec(),
            description: b"Check for updates to the application.".to_vec(),
        }],

        "undo" => vec![Command {
            action: Action::Undo,
            title: b"Undo".to_vec(),
            description: b"Undo the last action.".to_vec(),
        }],

        "redo" => vec![Command {
            action: Action::Redo,
            title: b"Redo".to_vec(),
            description: b"Redo the last undone action.".to_vec(),
        }],

        "quit" => vec![Command {
            action: Action::Quit,
            title: b"Quit".to_vec(),
            description: b"Quit the application.".to_vec(),
        }],

        "text" => vec![Command {
            action: Action::Text("👻".as_bytes().to_vec()),
            title: b"rustty".to_vec(),
            description: b"Put a little rustty in your terminal.".to_vec(),
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
}
