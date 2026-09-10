//! rustty-cli::rustty
//!
//! PORTS: ghostty src/cli/ghostty.zig — the concrete `Action` enum (the set of
//! `+<action>` CLI verbs), its `detectSpecialCase`, and the `run`/`runMain`
//! dispatch. Byte-faithful (R1); user-facing action verbs are UNCHANGED (R2 —
//! `list-fonts` etc. are user-facing CLI tags), and product-name comments are
//! retargeted `Ghostty`->`rustty`.

// const std = @import("std");
// const Allocator = std.mem.Allocator;
// const help_strings = @import("help_strings");
//   -> the build-generated `crate::help_strings::action_help` table (see `help_text`).
// const actionpkg = @import("action.zig");
// const SpecialCase = actionpkg.SpecialCase;
use crate::action::{self, SpecialCase};

// const list_fonts = @import("list_fonts.zig"); ... etc.
//
// All action impls are real ports (chunks C1–C5, CLI_BLUEPRINT.md §7); `list_fonts`
// + `show_face` port against rustty-font-shaper's discovery + SharedGridSet.
use crate::{
    boo, crash_report, edit_config, explain_config, help, list_actions, list_colors, list_fonts,
    list_keybinds, list_themes, new_tab, new_window, show_config, show_face, ssh, ssh_cache,
    toggle_quick_terminal, validate_config, version,
};

/// The action-dispatch error set.
///
/// The reference's `run`/`runMain` return the inferred error union `!u8`; the Rust
/// port aggregates the observable failures (IO, arg-parse, args-iterator init)
/// plus the [`Error::ActionHelpRequested`] sentinel (the reference's
/// `pub const help_error = error.ActionHelpRequested;`).
#[derive(Debug)]
pub enum Error {
    /// Reference: `pub const help_error = error.ActionHelpRequested;` — returned
    /// by actions that want [`Action::run`] to print their help text.
    ActionHelpRequested,
    /// A write to stdout/stderr failed.
    Io(std::io::Error),
    /// A CLI argument failed to parse.
    Parse(rustty_args::args::ParseError),
    /// The process args iterator failed to initialize.
    ArgsInit(rustty_args::args::InitError),
    /// A `Config.load`/`loadFile`/`finalize` failure propagated from an action
    /// (`show_config`/`validate_config`/`explain_config`). Reference: the
    /// `Config.load(alloc)` errors folded into each action `run`'s inferred `!u8`
    /// error set.
    Config(rustty_config::config_struct::LoadError),
    /// An XDG directory lookup failed. Reference: `crash.defaultDir(alloc)`'s
    /// `xdg.state` error, folded into `crash_report`'s `runInner` `!u8`.
    Xdg(rustty_os::xdg::DirError),
    /// A config file-path resolution failed. Reference:
    /// `configpkg.preferredDefaultFilePath(alloc)`, folded into `edit_config`'s
    /// `runInner` `!u8`.
    FileLoad(rustty_config::file_load::FileLoadError),
    /// A font-discovery failure propagated from `list_fonts`
    /// (`font.Discover.discover`/`DiscoverIterator.next`), folded into the
    /// reference `run`'s inferred `!u8` error set. Present on every target that
    /// has a real discoverer: macOS (CoreText) and Linux/BSD (Fontconfig). The
    /// wasm `web_canvas` backend maps the reference's `font.Discover == void` arm,
    /// so the shaper defines no `discovery::Error` there.
    #[cfg(not(target_arch = "wasm32"))]
    FontDiscover(rustty_font_shaper::discovery::Error),
    /// A FreeType library-init failure propagated from `list_fonts`'
    /// `try font.Library.init(alloc)` (reference `list_fonts.zig:106`), folded
    /// into the inferred `!u8` error set. Linux/BSD only — the
    /// `fontconfig_freetype` backend's `Library` is `FreetypeLibrary`, whose
    /// `init` returns `ft::FtError`; on macOS/wasm `Library` is the `NoopLibrary`
    /// whose `InitError` is uninhabited (that arm is discharged with `match e {}`).
    #[cfg(not(any(target_os = "macos", target_arch = "wasm32")))]
    FontLibrary(rustty_font_shaper::ft::FtError),
    /// A vaxis tty failure propagated from the interactive `prettyPrint` render
    /// paths (`list_colors`/`list_keybinds`). Reference: `vaxis.Tty.init` /
    /// `vaxis.Tty.getWinsize` `try`d into the inferred `!u8` error set.
    Tty(rustty_vaxis::tty::TtyError),
    /// `error.OutOfMemory` propagated from config formatting
    /// (`show_config`'s `configfmt.format` / `list_keybinds`'
    /// `formatEntryDocs`): a value-type `formatEntry` fixed buffer overflowed
    /// (`rustty_cfg_fmt::FormatError::OutOfMemory`), folded into the inferred
    /// `!u8` error sets exactly as the reference's `error.OutOfMemory`.
    OutOfMemory,
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

/// The config-formatting error union (`formatEntry`'s writer error +
/// `error.OutOfMemory`) folds into the action error set: writer failures are
/// IO failures here (every cli formatting writer's `Error` is
/// `std::io::Error`), and `OutOfMemory` maps to [`Error::OutOfMemory`].
impl From<rustty_cfg_fmt::FormatError<std::io::Error>> for Error {
    fn from(e: rustty_cfg_fmt::FormatError<std::io::Error>) -> Self {
        match e {
            rustty_cfg_fmt::FormatError::Writer(w) => Error::Io(w),
            rustty_cfg_fmt::FormatError::OutOfMemory => Error::OutOfMemory,
        }
    }
}

impl From<rustty_args::args::ParseError> for Error {
    fn from(e: rustty_args::args::ParseError) -> Self {
        // The `help` sentinel (`error.ActionHelpRequested`) travels through
        // `args.parse` as `ParseError::ActionHelpRequested` (see the rustty-args
        // `Error` note); recover it as the cli-level `ActionHelpRequested` so
        // `Action::run`'s `help_error` catch matches. All other parse failures
        // wrap into `Parse`.
        match e {
            rustty_args::args::Error::ActionHelpRequested => Error::ActionHelpRequested,
            other => Error::Parse(other),
        }
    }
}

impl From<rustty_args::args::InitError> for Error {
    fn from(e: rustty_args::args::InitError) -> Self {
        Error::ArgsInit(e)
    }
}

impl From<rustty_config::config_struct::LoadError> for Error {
    fn from(e: rustty_config::config_struct::LoadError) -> Self {
        Error::Config(e)
    }
}

impl From<rustty_os::xdg::DirError> for Error {
    fn from(e: rustty_os::xdg::DirError) -> Self {
        Error::Xdg(e)
    }
}

impl From<rustty_config::file_load::FileLoadError> for Error {
    fn from(e: rustty_config::file_load::FileLoadError) -> Self {
        Error::FileLoad(e)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl From<rustty_font_shaper::discovery::Error> for Error {
    fn from(e: rustty_font_shaper::discovery::Error) -> Self {
        Error::FontDiscover(e)
    }
}

#[cfg(not(any(target_os = "macos", target_arch = "wasm32")))]
impl From<rustty_font_shaper::ft::FtError> for Error {
    fn from(e: rustty_font_shaper::ft::FtError) -> Self {
        Error::FontLibrary(e)
    }
}

impl From<rustty_vaxis::tty::TtyError> for Error {
    fn from(e: rustty_vaxis::tty::TtyError) -> Self {
        Error::Tty(e)
    }
}

impl From<std::convert::Infallible> for Error {
    // `rustty_os::getenv` (edit_config) returns `Result<_, Infallible>` (the
    // reference's `Allocator.Error` collapses to `Infallible` in Rust); `?`
    // through it is statically unreachable.
    fn from(e: std::convert::Infallible) -> Self {
        match e {}
    }
}

/// Special commands that can be invoked via CLI flags. These are all
/// invoked by using `+<action>` as a CLI flag. The only exception is
/// "version" which can be invoked additionally with `--version`.
// pub const Action = enum {
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Output the version and exit
    Version,

    /// Output help information for the CLI or configuration
    Help,

    /// List available fonts
    ListFonts,

    /// List available keybinds
    ListKeybinds,

    /// List available themes
    ListThemes,

    /// List named RGB colors
    ListColors,

    /// List keybind actions
    ListActions,

    /// Wrap `ssh` to configure Rustty terminal integration on remote hosts
    Ssh,

    /// Manage SSH terminfo cache for automatic remote host setup
    SshCache,

    /// Edit the config file in the configured terminal editor.
    EditConfig,

    /// Dump the config to stdout
    ShowConfig,

    /// Explain a single config option
    ExplainConfig,

    // Validate passed config file
    ValidateConfig,

    // Show which font face Rustty loads a codepoint from.
    ShowFace,

    // List, (eventually) view, and (eventually) send crash reports.
    CrashReport,

    // Boo!
    Boo,

    // Use IPC to tell the running Rustty to open a new window.
    NewWindow,

    // Use IPC to tell the running Rustty to open a new tab.
    NewTab,

    // Use IPC to tell the running Rustty to toggle the quick terminal.
    ToggleQuickTerminal,
}

// pub fn detectSpecialCase(arg: []const u8) ?SpecialCase(Action) { ... }
impl action::Action for Action {
    // std.meta.stringToEnum(Action, name) — the `+name` -> variant lookup. The
    // R2 rename keeps the same kebab tags: they are user-facing CLI verbs,
    // unchanged by GHOSTTY->Rustty.
    fn from_plus_name(name: &[u8]) -> Option<Self> {
        Some(match name {
            b"version" => Action::Version,
            b"help" => Action::Help,
            b"list-fonts" => Action::ListFonts,
            b"list-keybinds" => Action::ListKeybinds,
            b"list-themes" => Action::ListThemes,
            b"list-colors" => Action::ListColors,
            b"list-actions" => Action::ListActions,
            b"ssh" => Action::Ssh,
            b"ssh-cache" => Action::SshCache,
            b"edit-config" => Action::EditConfig,
            b"show-config" => Action::ShowConfig,
            b"explain-config" => Action::ExplainConfig,
            b"validate-config" => Action::ValidateConfig,
            b"show-face" => Action::ShowFace,
            b"crash-report" => Action::CrashReport,
            b"boo" => Action::Boo,
            b"new-window" => Action::NewWindow,
            b"new-tab" => Action::NewTab,
            b"toggle-quick-terminal" => Action::ToggleQuickTerminal,
            _ => return None,
        })
    }

    fn detect_special_case(arg: &[u8]) -> Option<SpecialCase<Self>> {
        // If we see a "-e" and we haven't seen a command yet, then
        // we are done looking for commands. This special case enables
        // `rustty -e rustty +command`. If we've seen a command we
        // still want to keep looking because
        // `rustty +command -e +command` is invalid.
        if arg == b"-e" {
            return Some(SpecialCase::AbortIfNoAction);
        }

        // Special case, --version always outputs the version no
        // matter what, no matter what other args exist.
        if arg == b"--version" {
            return Some(SpecialCase::Action(Action::Version));
        }

        // --help matches "help" but if a subcommand is specified
        // then we match the subcommand.
        if arg == b"--help" || arg == b"-h" {
            return Some(SpecialCase::Fallback(Action::Help));
        }

        None
    }
}

// The `help_error` arm's writer half, lifted out of [`Action::run`] so its two
// failure diagnostics have a driver.
//
// PORT NOTE (S68-F5): the reference writes these two arms INLINE inside
// `run`'s `inline for`. They are lifted here for one reason only: both arms
// are IO-failure arms on the process's real stdout, and a test cannot make
// `std::io::stdout()` fail. The control flow, the messages, the levels and the
// exit codes are the reference's, unchanged; `run` passes the same
// `BufWriter<StdoutLock>` it built inline before.
fn write_help_text<W: std::io::Write>(stdout: &mut W, text: &str) -> u8 {
    // stdout.writeAll(text) catch |write_err| { ... break :err 1; };
    if let Err(write_err) = writeln!(stdout, "{text}") {
        // std.log.warn("failed to write help text: {}\n", .{write_err});
        //
        // PORT NOTE (S68-F5): `std.log.warn` is the UNSCOPED logger, i.e. the
        // reference's `.default` scope — the facade's carrier for that is the
        // EMPTY target (DECISION-logging.md §6r4). The port used a bare
        // `eprintln!`, which reaches stderr without the level or scope prefix
        // `logFn` renders and ignores `Logging.stderr` entirely. The trailing
        // `\n` is the reference's own and is reproduced verbatim.
        log::warn!(target: "", "failed to write help text: {write_err}\n");
        // break :err 1;
        return 1;
    }
    // stdout.flush() catch |flush_err| { ... break :err 1; };
    if let Err(flush_err) = stdout.flush() {
        // std.log.warn("failed to flush help text: {}\n", .{flush_err});
        log::warn!(target: "", "failed to flush help text: {flush_err}\n");
        // break :err 1;
        return 1;
    }
    // break :err 0;
    0
}

impl Action {
    /// All actions in declaration order — the reference `@typeInfo(Action)
    /// .@"enum".fields` iteration order. Kept EXACT (help/completions/C-enum
    /// parity; CLI_BLUEPRINT.md Risk 7).
    pub const ALL: [Action; 19] = [
        Action::Version,
        Action::Help,
        Action::ListFonts,
        Action::ListKeybinds,
        Action::ListThemes,
        Action::ListColors,
        Action::ListActions,
        Action::Ssh,
        Action::SshCache,
        Action::EditConfig,
        Action::ShowConfig,
        Action::ExplainConfig,
        Action::ValidateConfig,
        Action::ShowFace,
        Action::CrashReport,
        Action::Boo,
        Action::NewWindow,
        Action::NewTab,
        Action::ToggleQuickTerminal,
    ];

    /// The kebab-case tag name (`@tagName`) — the user-facing `+<verb>` (R2:
    /// unchanged; these are user-facing CLI verbs).
    pub fn tag_name(self) -> &'static str {
        match self {
            Action::Version => "version",
            Action::Help => "help",
            Action::ListFonts => "list-fonts",
            Action::ListKeybinds => "list-keybinds",
            Action::ListThemes => "list-themes",
            Action::ListColors => "list-colors",
            Action::ListActions => "list-actions",
            Action::Ssh => "ssh",
            Action::SshCache => "ssh-cache",
            Action::EditConfig => "edit-config",
            Action::ShowConfig => "show-config",
            Action::ExplainConfig => "explain-config",
            Action::ValidateConfig => "validate-config",
            Action::ShowFace => "show-face",
            Action::CrashReport => "crash-report",
            Action::Boo => "boo",
            Action::NewWindow => "new-window",
            Action::NewTab => "new-tab",
            Action::ToggleQuickTerminal => "toggle-quick-terminal",
        }
    }

    /// Run the action. This returns the exit code to exit with.
    // pub fn run(self: Action, alloc: Allocator) !u8 {
    //
    // PORT NOTE: the reference `alloc` allocator param is dropped tree-wide (Rust
    // uses the global allocator); actions take no allocator argument.
    pub fn run(self) -> Result<u8, Error> {
        // return self.runMain(alloc) catch |err| switch (err) {
        match self.run_main() {
            // If help is requested, then we use some comptime trickery
            // to find this action in the help strings and output that.
            //
            // PORT NOTE: the reference `inline for` over `Action` fields locates
            // `self` by name to index `help_strings.Action.<field>`; the Rust
            // runtime equivalent is a direct lookup ([`Action::help_text`]). The
            // per-action help strings come from the build-generated `help_strings`
            // doc table (`crate::help_strings::action_help`, the faithful analog of
            // ghostty's helpgen build step — landed 2026-07-03). The control flow
            // (write text + "\n", flush, exit 0; warn + exit 1 on IO failure) is
            // preserved verbatim.
            // help_error => err: {
            Err(Error::ActionHelpRequested) => {
                // Future note: for now we just output the help text directly
                // to stdout. In the future we can style this much prettier
                // for all commands by just changing this one place.
                // var buffer: [1024]u8; var stdout_writer = File.stdout().writer(&buffer);
                let mut stdout = std::io::BufWriter::new(std::io::stdout().lock());
                // const text = @field(help_strings.Action, field.name) ++ "\n";
                let text = self.help_text();
                Ok(write_help_text(&mut stdout, text))
            }
            // else => err,
            other => other,
        }
    }

    // fn runMain(self: Action, alloc: Allocator) !u8 {
    fn run_main(self) -> Result<u8, Error> {
        // return switch (self) {
        match self {
            Action::Version => version::run(),
            Action::Help => help::run(),
            Action::ListFonts => list_fonts::run(),
            Action::ListKeybinds => list_keybinds::run(),
            Action::ListThemes => list_themes::run(),
            Action::ListColors => list_colors::run(),
            Action::ListActions => list_actions::run(),
            Action::SshCache => ssh_cache::run(),
            Action::Ssh => ssh::run(),
            Action::EditConfig => edit_config::run(),
            Action::ShowConfig => show_config::run(),
            Action::ExplainConfig => explain_config::run(),
            Action::ValidateConfig => validate_config::run(),
            Action::CrashReport => crash_report::run(),
            Action::ShowFace => show_face::run(),
            Action::Boo => boo::run(),
            Action::NewWindow => new_window::run(),
            Action::NewTab => new_tab::run(),
            Action::ToggleQuickTerminal => toggle_quick_terminal::run(),
        }
    }

    /// Returns the help text for this action, printed by [`run`](Action::run)
    /// when an action returns [`Error::ActionHelpRequested`].
    ///
    /// Reference: `@field(help_strings.Action, field.name)` — the build-generated
    /// per-action doc table. This is the ported `crate::help_strings::action_help`
    /// (emitted by build.rs from each action module's `pub fn run` `///` doc), the
    /// runtime analog of the reference's `inline for` field lookup.
    fn help_text(self) -> &'static str {
        // const text = @field(help_strings.Action, field.name) ++ "\n";
        //   (the trailing "\n" is added by the `writeln!` in `run`.)
        crate::help_strings::action_help(self)
    }

    /// Returns the filename associated with an action. This is a relative
    /// path from the root src/ directory.
    // pub fn file(comptime self: Action) []const u8 {
    pub fn file(self) -> String {
        // const tag = @tagName(self);
        let tag = self.tag_name();
        // _ = std.mem.replace(u8, tag, "-", "_", &filename);
        let filename = tag.replace('-', "_");
        // return "cli/" ++ filename ++ ".zig";
        //
        // PORT NOTE: the port's action impls are `.rs`, so the relative path is
        // `cli/<name>.rs` (the reference hardcodes `.zig`).
        format!("cli/{filename}.rs")
    }

    // Returns the options of action. Supports generating shell completions
    // without duplicating the mapping from Action to relevant Option
    // @import(..) declaration.
    //
    // PORT NOTE: the reference `pub fn options(comptime self: Action) type` returns
    // a comptime TYPE (each action's `Options` struct); the shell-completion
    // generator then `@typeInfo`-reflects that struct's fields. Rust has no
    // runtime type-returning fn and no field reflection, so the faithful analog is
    // `option_specs` below: a per-action ordered list of `OptSpec { name, kind }`
    // that reifies each `Options` struct's non-`_` fields + their completion-
    // relevant type category (`@typeInfo(opt.type)`). This lives here in
    // `rustty-cli` — next to the `Options` structs the reference reflected over —
    // exactly as `options()` did. The mapping (`Action::X` -> `<x>::Options`) is
    // preserved in the arm order below.
    //
    //     pub fn options(comptime self: Action) type {
    //         comptime {
    //             return switch (self) {
    //                 .version => version.Options,
    //                 .help => help.Options,
    //                 .@"list-fonts" => list_fonts.Options,
    //                 .@"list-keybinds" => list_keybinds.Options,
    //                 .@"list-themes" => list_themes.Options,
    //                 .@"list-colors" => list_colors.Options,
    //                 .@"list-actions" => list_actions.Options,
    //                 .@"ssh-cache" => ssh_cache.Options,
    //                 .ssh => ssh.Options,
    //                 .@"edit-config" => edit_config.Options,
    //                 .@"show-config" => show_config.Options,
    //                 .@"explain-config" => explain_config.Options,
    //                 .@"validate-config" => validate_config.Options,
    //                 .@"crash-report" => crash_report.Options,
    //                 .@"show-face" => show_face.Options,
    //                 .boo => boo.Options,
    //                 .@"new-window" => new_window.Options,
    //                 .@"new-tab" => new_tab.Options,
    //                 .@"toggle-quick-terminal" => toggle_quick_terminal.Options,
    //             };
    //         }
    //     }

    /// The completion specs for this action's `Options` struct — the reified
    /// `@typeInfo(options()).@"struct".fields` iteration (skipping `_`-prefixed
    /// fields) the shell-completion generators drive. Each [`OptSpec`] carries the
    /// hyphenated CLI name + the `@typeInfo(opt.type)` classification. Actions
    /// whose `Options` has no non-`_` fields return an empty slice (the reference's
    /// `fields.len == 0` skip in the bash/zsh per-action `case` blocks).
    pub fn option_specs(self) -> &'static [OptSpec] {
        use OptCompletionKind::{Bool, Enum, OptionalEnum, OptionalOther, Other};
        match self {
            // version.Options = struct {}; boo/edit-config/crash-report likewise.
            Action::Version => &[],
            // help.Options = struct { help: bool };
            Action::Help => &[OptSpec {
                name: b"help",
                kind: Bool,
            }],
            // list_fonts.Options: family ?str, style ?str, bold bool, italic bool.
            Action::ListFonts => &[
                OptSpec {
                    name: b"family",
                    kind: OptionalOther,
                },
                OptSpec {
                    name: b"style",
                    kind: OptionalOther,
                },
                OptSpec {
                    name: b"bold",
                    kind: Bool,
                },
                OptSpec {
                    name: b"italic",
                    kind: Bool,
                },
            ],
            // list_keybinds.Options: default bool, docs bool, plain bool.
            Action::ListKeybinds => &[
                OptSpec {
                    name: b"default",
                    kind: Bool,
                },
                OptSpec {
                    name: b"docs",
                    kind: Bool,
                },
                OptSpec {
                    name: b"plain",
                    kind: Bool,
                },
            ],
            // list_themes.Options: path bool, plain bool, color ColorScheme (enum).
            Action::ListThemes => &[
                OptSpec {
                    name: b"path",
                    kind: Bool,
                },
                OptSpec {
                    name: b"plain",
                    kind: Bool,
                },
                OptSpec {
                    name: b"color",
                    kind: Enum(crate::list_themes::ColorScheme::VALUE_NAMES),
                },
            ],
            // list_colors.Options: plain bool.
            Action::ListColors => &[OptSpec {
                name: b"plain",
                kind: Bool,
            }],
            // list_actions.Options: docs bool.
            Action::ListActions => &[OptSpec {
                name: b"docs",
                kind: Bool,
            }],
            // ssh.Options: forward-env bool, terminfo bool, cache bool, ssh str,
            // verbose bool (+ `_ssh_args` / `_diagnostics`, skipped).
            Action::Ssh => &[
                OptSpec {
                    name: b"forward-env",
                    kind: Bool,
                },
                OptSpec {
                    name: b"terminfo",
                    kind: Bool,
                },
                OptSpec {
                    name: b"cache",
                    kind: Bool,
                },
                OptSpec {
                    name: b"ssh",
                    kind: Other,
                },
                OptSpec {
                    name: b"verbose",
                    kind: Bool,
                },
            ],
            // ssh_cache.Options: clear bool, add ?str, remove ?str, prune ?Duration.
            Action::SshCache => &[
                OptSpec {
                    name: b"clear",
                    kind: Bool,
                },
                OptSpec {
                    name: b"add",
                    kind: OptionalOther,
                },
                OptSpec {
                    name: b"remove",
                    kind: OptionalOther,
                },
                OptSpec {
                    name: b"prune",
                    kind: OptionalOther,
                },
            ],
            // edit_config.Options = struct {}.
            Action::EditConfig => &[],
            // show_config.Options: default bool, changes-only bool, docs bool,
            // no-pager bool.
            Action::ShowConfig => &[
                OptSpec {
                    name: b"default",
                    kind: Bool,
                },
                OptSpec {
                    name: b"changes-only",
                    kind: Bool,
                },
                OptSpec {
                    name: b"docs",
                    kind: Bool,
                },
                OptSpec {
                    name: b"no-pager",
                    kind: Bool,
                },
            ],
            // explain_config.Options: option ?str, keybind ?str.
            Action::ExplainConfig => &[
                OptSpec {
                    name: b"option",
                    kind: OptionalOther,
                },
                OptSpec {
                    name: b"keybind",
                    kind: OptionalOther,
                },
            ],
            // validate_config.Options: config-file ?str (name special-cased -> files).
            Action::ValidateConfig => &[OptSpec {
                name: b"config-file",
                kind: OptionalOther,
            }],
            // show_face.Options: cp ?u21, string ?str, style font.Style (enum),
            // presentation ?font.Presentation (optional enum) (+ `_diagnostics`).
            Action::ShowFace => &[
                OptSpec {
                    name: b"cp",
                    kind: OptionalOther,
                },
                OptSpec {
                    name: b"string",
                    kind: OptionalOther,
                },
                OptSpec {
                    name: b"style",
                    kind: Enum(FONT_STYLE_VALUE_NAMES),
                },
                OptSpec {
                    name: b"presentation",
                    kind: OptionalEnum(FONT_PRESENTATION_VALUE_NAMES),
                },
            ],
            // crash_report.Options = struct {}.
            Action::CrashReport => &[],
            // boo.Options = struct {}.
            Action::Boo => &[],
            // new_window.Options: class ?str (+ `_working_directory_seen` /
            // `_arguments` / `_diagnostics` / `_raw`, all skipped).
            Action::NewWindow => &[OptSpec {
                name: b"class",
                kind: OptionalOther,
            }],
            // new_tab.Options: class ?str, surface-id ?u64 (+ `_`-prefixed
            // fields, all skipped).
            Action::NewTab => &[
                OptSpec {
                    name: b"class",
                    kind: OptionalOther,
                },
                OptSpec {
                    name: b"surface-id",
                    kind: OptionalOther,
                },
            ],
            // toggle_quick_terminal.Options: class ?str.
            Action::ToggleQuickTerminal => &[OptSpec {
                name: b"class",
                kind: OptionalOther,
            }],
        }
    }
}

/// The completion-relevant classification of an action `Options` field's type,
/// reifying the reference `switch (@typeInfo(opt.type))` (`.bool` / `.@"enum"` /
/// `.optional` with an enum or non-enum child / else) that the bash/zsh/fish
/// per-action completion loops switch on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptCompletionKind {
    /// `.bool` — a flag with no value.
    Bool,
    /// `.@"enum"` — complete the variant names.
    Enum(&'static [&'static [u8]]),
    /// `.optional` whose child is `.@"enum"` — complete the child's variant names.
    OptionalEnum(&'static [&'static [u8]]),
    /// `.optional` whose child is NOT an enum — no value-completion (but the
    /// generators still name-special-case `config-file`; bash's optional arm emits
    /// `return ;;` vs the non-optional arm's `_files ;;`).
    OptionalOther,
    /// Any other non-optional, non-enum, non-bool type — no value-completion.
    Other,
}

/// One `Options` field's completion spec: its hyphenated CLI name (`opt.name`,
/// with `_`-prefixed fields already omitted) and its [`OptCompletionKind`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OptSpec {
    /// The hyphenated CLI spelling of the option (the Zig field name).
    pub name: &'static [u8],
    /// The `@typeInfo(opt.type)` classification driving value-completion.
    pub kind: OptCompletionKind,
}

// The `@tagName` value-name tables for the enum options, in declaration order.
// `font.Style`/`font.Presentation` live in `rustty-font-shaper` (a `#[repr(u8)]`
// enum without a `to_name`); these mirror `show_face`'s own `stringToEnum` arms.
const FONT_STYLE_VALUE_NAMES: &[&[u8]] = &[b"regular", b"bold", b"italic", b"bold_italic"];
const FONT_PRESENTATION_VALUE_NAMES: &[&[u8]] = &[b"text", b"emoji"];

#[cfg(test)]
mod tests {
    use super::Action;
    use crate::action::{detect_iter, DetectError};
    use crate::test_util::ArgIteratorGeneral;

    // test "parse action none"
    #[test]
    fn parse_action_none() {
        let mut iter = ArgIteratorGeneral::init(b"--a=42 --b --b-f=false");
        let action = detect_iter::<Action, _>(&mut iter).unwrap();
        // try testing.expect(action == null);
        assert!(action.is_none());
        iter.deinit();
    }

    // test "parse action version"
    #[test]
    fn parse_action_version() {
        {
            let mut iter = ArgIteratorGeneral::init(b"--a=42 --b --b-f=false --version");
            let action = detect_iter::<Action, _>(&mut iter).unwrap();
            // try testing.expect(action.? == .version);
            assert_eq!(action.unwrap(), Action::Version);
            iter.deinit();
        }

        {
            let mut iter = ArgIteratorGeneral::init(b"--version --a=42 --b --b-f=false");
            let action = detect_iter::<Action, _>(&mut iter).unwrap();
            assert_eq!(action.unwrap(), Action::Version);
            iter.deinit();
        }

        {
            let mut iter = ArgIteratorGeneral::init(b"--c=84 --d --version --a=42 --b --b-f=false");
            let action = detect_iter::<Action, _>(&mut iter).unwrap();
            assert_eq!(action.unwrap(), Action::Version);
            iter.deinit();
        }
    }

    // test "parse action plus"
    #[test]
    fn parse_action_plus() {
        {
            let mut iter = ArgIteratorGeneral::init(b"--a=42 --b --b-f=false +version");
            let action = detect_iter::<Action, _>(&mut iter).unwrap();
            assert_eq!(action.unwrap(), Action::Version);
            iter.deinit();
        }

        {
            let mut iter = ArgIteratorGeneral::init(b"+version --a=42 --b --b-f=false");
            let action = detect_iter::<Action, _>(&mut iter).unwrap();
            assert_eq!(action.unwrap(), Action::Version);
            iter.deinit();
        }

        {
            let mut iter = ArgIteratorGeneral::init(b"--c=84 --d +version --a=42 --b --b-f=false");
            let action = detect_iter::<Action, _>(&mut iter).unwrap();
            assert_eq!(action.unwrap(), Action::Version);
            iter.deinit();
        }
    }

    // test "parse action plus ignores -e"
    #[test]
    fn parse_action_plus_ignores_e() {
        {
            let mut iter = ArgIteratorGeneral::init(b"--a=42 -e +version");
            let action = detect_iter::<Action, _>(&mut iter).unwrap();
            // try testing.expect(action == null);
            assert!(action.is_none());
            iter.deinit();
        }

        {
            let mut iter = ArgIteratorGeneral::init(b"+list-fonts --a=42 -e +version");
            // try testing.expectError(DetectError.MultipleActions, ...);
            assert_eq!(
                detect_iter::<Action, _>(&mut iter),
                Err(DetectError::MultipleActions),
            );
            iter.deinit();
        }
    }
}

#[cfg(test)]
mod help_text_log_tests {
    //! task-14e-2 / S68-F5 — `cli/ghostty.zig:135,139`.
    //!
    //! Both diagnostics sit on IO-failure arms over the process's real stdout,
    //! which a test cannot make fail; [`super::write_help_text`] exists so they
    //! have a driver. Every assertion is `assert_eq!` on the full captured
    //! record vector.
    use super::write_help_text;
    use crate::log_capture::CapturedRecord;
    use std::io::Write;

    /// A writer whose `write` fails immediately.
    struct FailingWrite;
    impl Write for FailingWrite {
        fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::from_raw_os_error(libc::EPIPE))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// A writer that accepts everything and fails only on `flush`.
    struct FailingFlush(Vec<u8>);
    impl Write for FailingFlush {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Err(std::io::Error::from_raw_os_error(libc::ENOSPC))
        }
    }

    #[test]
    fn a_write_failure_warns_under_the_default_scope_and_exits_1() {
        let cap = crate::log_capture::start();
        let code = write_help_text(&mut FailingWrite, "help");
        let messages = cap.messages();
        drop(cap);

        // Positive reach: the reference's arm is `break :err 1`.
        assert_eq!(1, code);
        assert_eq!(
            vec![(
                log::Level::Warn,
                // `std.log.warn` is UNSCOPED — the reference's `.default`
                // scope, carried by the facade as the EMPTY target.
                String::new(),
                format!(
                    "failed to write help text: {}\n",
                    std::io::Error::from_raw_os_error(libc::EPIPE)
                ),
            )],
            messages
        );
    }

    #[test]
    fn a_flush_failure_warns_under_the_default_scope_and_exits_1() {
        let cap = crate::log_capture::start();
        let mut w = FailingFlush(Vec::new());
        let code = write_help_text(&mut w, "help");
        let messages = cap.messages();
        drop(cap);

        assert_eq!(1, code);
        // Positive reach: the write half really succeeded, so the flush arm is
        // what fired.
        assert_eq!(b"help\n".to_vec(), w.0);
        assert_eq!(
            vec![(
                log::Level::Warn,
                String::new(),
                format!(
                    "failed to flush help text: {}\n",
                    std::io::Error::from_raw_os_error(libc::ENOSPC)
                ),
            )],
            messages
        );
    }

    /// The absence half: a writer that succeeds emits nothing and exits 0.
    /// Positive reach is the written bytes — they only exist if the body ran.
    #[test]
    fn a_successful_help_write_is_silent() {
        let cap = crate::log_capture::start();
        let mut out: Vec<u8> = Vec::new();
        let code = write_help_text(&mut out, "help");
        let messages = cap.messages();
        drop(cap);

        assert_eq!(0, code);
        assert_eq!(
            b"help\n".to_vec(),
            out,
            "write_help_text did not write, so this test never reached the \
             code whose silence it asserts"
        );
        assert_eq!(Vec::<CapturedRecord>::new(), messages);
    }
}
