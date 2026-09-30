//! rustty-apprt::gtk::class::overrides
//!
//! PORTS: ghostty `src/apprt/gtk/class/Overrides.zig` (9fc8d9ebd, "gtk: streamline
//! surface overrides"). The reference factored the caller-supplied
//! command/shell-integration/working-directory/title override quadruple — until
//! then FOUR copies of the same anonymous struct, one each in
//! `application.zig`'s `actionNewWindow`/`actionNewTab`, `split_tree.zig`'s
//! `newSplit`, `surface.zig`'s `Surface.new` and `window.zig`'s
//! `newTab`/`newTabForWindow`/`newTabPage` — into its own file, plus the
//! `application.zig` `parseOverrides` helper that builds one. Commit message:
//! "Moves code related to surface overrides to its own file to help with
//! argument type redefinition in a bunch of places." No behaviour change: the
//! reference's own `parse` body is byte-identical to the pre-refactor
//! `parseOverrides`.
//!
//! Rustty already carried this file split for `TabOverrides` — this task is the
//! **file placement** fix: `TabOverrides` (formerly `class/tab.rs:43`) and
//! `parse_overrides` (formerly `class/application.rs:2148`) move here verbatim,
//! and `tab.rs`/`application.rs`/`window.rs` import them from here instead.
//! Rustty's `SurfaceOverrides` (`class/surface.rs:201`) stays a separate type —
//! Rustty never unified `Surface::new`'s overrides with `TabOverrides` the way
//! the reference's single `Overrides` type now spans all five call sites, and
//! this S-sized task does not do that unification either (out of scope; see
//! the R2 delta review's `9fc8d9ebd` row).

use rustty_config::command::Command;
use rustty_config::value_types::ShellIntegration;

/// `const log = std.log.scoped(.gtk_ghostty_overrides);` (`Overrides.zig:9`) —
/// its own scope, distinct from `gtk_rustty_application`/`gtk_rustty_window`;
/// `parse_overrides`'s diagnostics log under this scope now, not the caller's.
const LOG_SCOPE: &str = "gtk_rustty_overrides";

/// PORTS: `Overrides.zig:14-17` the `Overrides` struct fields (formerly
/// `tab.zig:191-194`'s anonymous `new` overrides struct, and identically
/// `application.zig`/`split_tree.zig`/`surface.zig`/`window.zig`'s four other
/// copies before 9fc8d9ebd). `#[derive(Default)]` is `Overrides.zig:19-24`'s
/// `pub const none: Overrides = .{ .command = null, ... }` sentinel.
#[derive(Default)]
pub struct TabOverrides {
    /// PORTS: `Overrides.zig:14` `command: ?configpkg.Command`.
    pub command: Option<Command>,
    /// PORTS: `Overrides.zig:15` `shell_integration:
    /// ?configpkg.Config.ShellIntegration` (B5, 3650bef4e).
    pub shell_integration: Option<ShellIntegration>,
    /// PORTS: `Overrides.zig:16` `working_directory: ?[:0]const u8`.
    pub working_directory: Option<String>,
    /// PORTS: `Overrides.zig:17` `title: ?[:0]const u8`.
    pub title: Option<String>,
}

/// PORTS: `Overrides.zig:26-124` `pub fn parse(arena_alloc: Allocator,
/// arguments_it: *glib.VariantIter) ParseError!Overrides` (formerly
/// `application.zig`'s file-local `parseOverrides`, B5, 3650bef4e — the
/// factored `new-window`/`new-tab` argv override parser; adds
/// `--shell-integration=`). `ParseError = Allocator.Error ||
/// error{ValueRequired}` (`Overrides.zig:12`); returns `None` where the
/// reference returns an error — the infallible-alloc arms keep only the shape.
pub fn parse_overrides(arguments_it: glib::VariantIter) -> Option<TabOverrides> {
    // `var args: std.ArrayList([:0]const u8) = .empty;` (`:27`).
    let mut args: Vec<Vec<u8>> = Vec::new();

    let mut working_directory: Option<String> = None;
    let mut title: Option<String> = None;
    let mut command: Option<Command> = None;
    // `var parsed_shell_integration: struct { @"shell-integration":
    // ?configpkg.Config.ShellIntegration = null } = .{};` (`:32-34`) — the
    // one-field struct exists only to give `cli.args.parseIntoField` a target;
    // the bare optional is its Rust collapse.
    let mut parsed_shell_integration: Option<ShellIntegration> = None;

    // `var e_seen: bool = false;` (`:39`.)
    let mut e_seen = false;

    // `while (arguments_it.nextValue()) |value|` (`:42`) — iterate the array
    // elements.
    for (i, value) in arguments_it.enumerate() {
        // just to be sure (`:45-46`).
        let Some(str) = value.str() else {
            continue;
        };

        // `log.debug("argument: {d} {s}", .{ i, str });` (`:52`).
        log::debug!(target: LOG_SCOPE, "argument: {i} {str}");

        if e_seen {
            // `args.append(arena_alloc, copy)` (`:54-64`) — everything
            // after `-e` is a direct-exec argument. The two alloc-failure
            // `return err` arms (`:57,61`) are infallible here.
            args.push(str.as_bytes().to_vec());
            continue;
        }

        // `if (std.mem.eql(u8, str, "-e"))` (`:66-69`).
        if str == "-e" {
            e_seen = true;
            continue;
        }

        // `if (std.mem.cutPrefix(u8, str, "--command="))` (`:71-79`) —
        // a parse failure warns and RETURNS the error (the B5 rework; the old
        // handler continued).
        if let Some(v) = str.strip_prefix("--command=") {
            let mut cmd = Command::Shell(Vec::new());
            match cmd.parse_cli(Some(v.as_bytes())) {
                Ok(()) => command = Some(cmd),
                Err(err) => {
                    log::warn!(target: LOG_SCOPE, "unable to parse command: {err:?}");
                    return None;
                }
            }
            continue;
        }
        // `if (std.mem.cutPrefix(u8, str, "--shell-integration="))`
        // (`:81-93`): `cli.args.parseIntoField(..., "shell-integration",
        // trim(v))` — for a bare optional enum field that is
        // `std.meta.stringToEnum` (== `string_to_enum`, the config name-map);
        // a parse failure warns and CONTINUES (`:89-91`), unlike the
        // command arm.
        //
        // PORT NOTE (N477): `parseIntoField` (`cli/args.zig:326-339`) first
        // treats an empty (post-trim) value as "reset to the field default";
        // the anonymous struct's `@"shell-integration"` defaults to `null`,
        // so an empty or all-whitespace `--shell-integration=` resets an
        // earlier value to None instead of failing to parse.
        if let Some(v) = str.strip_prefix("--shell-integration=") {
            let trimmed = trim_ascii_whitespace(v);
            if trimmed.is_empty() {
                parsed_shell_integration = None;
                continue;
            }
            match ShellIntegration::string_to_enum(trimmed.as_bytes()) {
                Some(parsed) => parsed_shell_integration = Some(parsed),
                None => {
                    log::warn!(target: LOG_SCOPE, "unable to parse shell integration {v}: InvalidValue")
                }
            }
            continue;
        }
        // `if (std.mem.cutPrefix(u8, str, "--working-directory="))`
        // (`:95-101`).
        if let Some(v) = str.strip_prefix("--working-directory=") {
            working_directory = Some(trim_ascii_whitespace(v).to_string());
            continue;
        }
        // `if (std.mem.cutPrefix(u8, str, "--title="))` (`:103-109`).
        if let Some(v) = str.strip_prefix("--title=") {
            title = Some(trim_ascii_whitespace(v).to_string());
            continue;
        }
    }

    // `if (args.items.len > 0) command = .{ .direct = args.items };`
    // (`:112-116`).
    if !args.is_empty() {
        command = Some(Command::Direct(args));
    }

    // `return .{ .command, .shell_integration, .working_directory, .title };`
    // (`:118-123`).
    Some(TabOverrides {
        command,
        shell_integration: parsed_shell_integration,
        working_directory,
        title,
    })
}

/// `std.mem.trim(u8, v, &std.ascii.whitespace)` — `Overrides.parse`'s trim of
/// the `--shell-integration=`, `--working-directory=` and `--title=` values
/// (`Overrides.zig:87,96,104`).
///
/// `std.ascii.whitespace` is `{ ' ', '\t', '\n', '\r', VT (0x0B), FF (0x0C) }`
/// (Zig 0.16 `lib/std/ascii.zig:162`). CORRECTION (task T-D, S50-F2): the three
/// sites used `str::trim()`, which also strips Unicode `White_Space` (U+00A0,
/// U+0085, U+2028, U+3000, ...). `str::trim_ascii()` is not a substitute either:
/// its set omits VT, which Zig's includes.
fn trim_ascii_whitespace(v: &str) -> &str {
    v.trim_matches(|c: char| matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0B' | '\x0C'))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// S50-F2: `Overrides.parse` trims its three values with
    /// `std.mem.trim(u8, v, &std.ascii.whitespace)` (`Overrides.zig:81-109`):
    /// ASCII space/TAB/LF/CR/VT/FF only. A value ending in U+00A0 (NO-BREAK SPACE)
    /// or U+3000 keeps it; VT and FF are stripped. Display-free: the argv arrives
    /// as a plain `as` variant.
    #[test]
    fn parse_overrides_trims_ascii_whitespace_only() {
        use glib::prelude::*;
        use rustty_config::value_types::ShellIntegration;

        let parse = |args: &[&str]| {
            let v = args.to_variant();
            parse_overrides(v.iter()).expect("no --command, so parsing cannot fail")
        };

        // Unicode whitespace is NOT trimmed (the reference trims ASCII only).
        let o = parse(&[
            "--title=\u{a0}tab\u{a0}",
            "--working-directory=/tmp/x\u{3000}",
        ]);
        assert_eq!(o.title.as_deref(), Some("\u{a0}tab\u{a0}"));
        assert_eq!(o.working_directory.as_deref(), Some("/tmp/x\u{3000}"));

        // ... so a NO-BREAK SPACE makes the shell-integration value unparseable
        // (the reference warns and continues), where `str::trim` would parse it.
        let o = parse(&["--shell-integration=\u{a0}bash"]);
        assert_eq!(o.shell_integration, None);

        // Every byte of `std.ascii.whitespace`, VT and FF included, IS trimmed.
        let o = parse(&[
            "--title= \t\n\r\x0B\x0Ctab\x0C\x0B\r\n\t ",
            "--working-directory=\x0B/tmp/y\x0C",
            "--shell-integration=\x0Bbash\x0C",
        ]);
        assert_eq!(o.title.as_deref(), Some("tab"));
        assert_eq!(o.working_directory.as_deref(), Some("/tmp/y"));
        assert_eq!(o.shell_integration, Some(ShellIntegration::Bash));

        // N477: an empty or all-whitespace value is `parseIntoField`'s
        // "reset to default" (`cli/args.zig:326-339`), and the default is
        // `null`: a later empty value clears an earlier parsed one.
        let o = parse(&["--shell-integration=bash", "--shell-integration="]);
        assert_eq!(o.shell_integration, None);
        let o = parse(&["--shell-integration=zsh", "--shell-integration= \t\x0B"]);
        assert_eq!(o.shell_integration, None);
        let o = parse(&["--shell-integration=", "--shell-integration=fish"]);
        assert_eq!(o.shell_integration, Some(ShellIntegration::Fish));
    }
}
