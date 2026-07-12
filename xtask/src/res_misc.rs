//! xtask `resources` sub-generator: the remaining build-phase share/ folders
//! `share/{man,locale,bat}` for the macOS Swift-app Copy-Resources phase.
//!
//! PORTS (build-orchestration analog, R4 — independent of parity order):
//!   * `share/man`   — ghostty `src/build/GhosttyDocs.zig` (`installDummy` path).
//!   * `share/locale` — ghostty `src/build/GhosttyI18n.zig`.
//!   * `share/bat`   — ghostty `src/build/GhosttyResources.zig` (sublime syntax
//!                     install to `share/bat/syntaxes`).
//!
//! These three are NOT runtime-critical for a basic terminal window; the macOS
//! `Ghostty.xcodeproj` Copy-Resources build phase references `../zig-out/share/
//! {man,locale,bat}` and only requires the folders to EXIST so the copy step
//! succeeds. Each folder is emitted minimal-but-VALID (R5): never faked content.
//!
//! DEFERRAL SUMMARY (durable, disjoint record — mirrored in
//! `.port/phase-2/DEFERRALS.swiftapp-misc.md`):
//!
//!  1. **man** (P1.3) — we now emit REAL, rendering `roff` man pages:
//!     `share/man/man1/ghostty.1` + `share/man/man5/ghostty.5` (verified via
//!     `man -M zig-out/share/man ghostty`). ghostty's own pipeline
//!     (`GhosttyDocs.zig`) runs `mdgen` (`src/build/mdgen/`) to emit *markdown*
//!     from the `help_strings` corpus, then `pandoc --to man` converts it to
//!     roff. pandoc is not available in this build environment, so we do NOT run
//!     that exact toolchain; instead the man body is a faithful `man(7)`-macro
//!     transcription of ghostty's `mdgen` `ghostty_{1,5}_{header,footer}.md`
//!     prose (real ghostty documentation — R5, not fabricated). The KEPT
//!     `Ghostty`/`ghostty`/`xterm-ghostty`/`com.mitchellh.ghostty` names are the
//!     documentation data-identity the installed `ghostty` command + reused
//!     Swift app carry (R2), same doctrine as the terminfo entry.
//!
//!     PARTIAL — the auto-generated `# CONFIGURATION OPTIONS` /
//!     `# COMMAND LINE ACTIONS` / `# KEYBIND ACTIONS` catalog (mdgen
//!     `genConfig`/`genActions`/`genKeybindActions`, one entry per config field /
//!     CLI action / keybind action from the live help corpus) is DEFERRED. That
//!     corpus lives in `rustty-config`/`rustty-cli`/`rustty-input`
//!     (`help_strings::config_help` + `CONFIG_FIELDS`, `action_help`,
//!     `keybind_help`); reaching it from `res_misc` requires adding those crates
//!     as xtask deps, which (a) is outside this agent's disjoint file set
//!     (`res_misc.rs`+`res_terminfo.rs`) and (b) would race the sibling
//!     resource-generator agents' `xtask/Cargo.toml` edits — the same reason
//!     `gen_misc` is deliberately `std`-only (see `bat` below). In its place the
//!     pages point users at `ghostty +show-config --default --docs` and
//!     `ghostty +<action> --help`, which is exactly how ghostty's own
//!     `ghostty(5)` DESCRIPTION directs readers to the always-current option list.
//!     Un-defer trigger: add a man-page generator to `rustty-extra` (already an
//!     xtask dep, already depends on rustty-config+rustty-cli) porting
//!     mdgen's three `gen*` walks → roff, and call it here.
//!
//!  2. **locale** — rustty i18n is a deferred BREAK. ghostty compiles gettext
//!     `.po` → `.mo` into `share/locale/<lang>/LC_MESSAGES/com.mitchellh.ghostty.mo`
//!     (`GhosttyI18n.zig`); rustty has no `.po` corpus / msgfmt pipeline ported
//!     yet. We emit a minimal-valid `share/locale/.placeholder` so the folder
//!     exists for Copy-Resources. No content is faked (no fake `.mo`).
//!     Un-defer trigger: porting the i18n `.po` corpus + `msgfmt` compile step.
//!
//!  3. **bat** — the sublime-syntax generator IS ported
//!     (`rustty_extra::sublime::syntax() -> Vec<u8>`, golden-tested byte-exact
//!     against ghostty's `ghostty.sublime-syntax`). We write REAL content:
//!     the committed, golden byte-for-byte source of that generator, embedded
//!     via `include_bytes!` from `crates/rustty-extra/testdata/`. This keeps
//!     `gen_misc` self-contained (pure `std`, no xtask→rustty-extra Cargo dep)
//!     so it compiles regardless of the concurrent Cargo.toml edits made by the
//!     sibling resource-generator agents.
//!     Optional follow-up (not a deferral of content — content is real & valid):
//!     the land agent MAY switch this to a live `rustty_extra::sublime::syntax()`
//!     call once `rustty-extra` is a wired xtask dependency; the emitted bytes
//!     are identical either way (guaranteed by `sublime::syntax`'s golden test).

use std::fs;
use std::path::Path;

/// The real ghostty sublime-syntax bytes: byte-identical to what
/// `rustty_extra::sublime::syntax()` emits (that generator's `syntax_matches_golden`
/// test pins it to exactly this file). Embedding the committed source keeps this
/// generator self-contained (no Cargo dep) while shipping authentic content.
const SUBLIME_SYNTAX: &[u8] =
    include_bytes!("../../crates/rustty-extra/testdata/ghostty.sublime-syntax");

/// `man(7)`-macro body of `share/man/man1/ghostty.1` (everything after the
/// `.TH` title line, which [`gen_man`] prepends with the live version). A
/// faithful roff transcription of ghostty `mdgen`'s `ghostty_1_header.md` +
/// `ghostty_1_footer.md` prose (the auto-generated option/action catalog is a
/// documented PARTIAL — see the module DEFERRAL SUMMARY). Kept `Ghostty`/
/// `ghostty`/`xterm-ghostty`/`com.mitchellh.ghostty` names are documentation
/// data-identity (R2).
const MAN1_BODY: &str = r#".SH NAME
ghostty \- Ghostty terminal emulator
.SH SYNOPSIS
.B ghostty
.RI [ +action ]
.RI [ options ]
.SH DESCRIPTION
Ghostty is a cross-platform, GPU-accelerated terminal emulator that aims to push
the boundaries of what is possible with a terminal emulator by exposing modern,
opt-in features that enable CLI tool developers to build more feature rich,
interactive applications.
.PP
There are a number of excellent terminal emulator options that exist today.
The unique goal of Ghostty is to have a platform for experimenting with modern,
optional, non-standards-compliant features to enhance the capabilities of CLI
applications. We aim to be the best in this category, and competitive in the
rest.
.PP
While aiming for this ambitious goal, Ghostty is a fully standards compliant
terminal emulator that aims to remain compatible with all existing shells and
software. You can use this as a drop-in replacement for your existing terminal
emulator.
.SH "COMMAND LINE ACTIONS"
The full, always-current list of command line actions and their documentation is
available at runtime. Run
.B ghostty \-\-help
for the top-level actions, and
.BI "ghostty +" action " \-\-help"
for a specific action's options.
.SH "CONFIGURATION OPTIONS"
The complete, always-current list of configuration options with documentation
can be printed at runtime with
.PP
.RS
.B ghostty +show\-config \-\-default \-\-docs
.RE
.PP
which outputs every option with its default value and doc comment. See
.BR ghostty (5)
for the configuration file format and locations.
.SH FILES
.TP
.I $XDG_CONFIG_HOME/ghostty/config.ghostty
Location of the default configuration file.
.TP
.I $HOME/Library/Application Support/com.mitchellh.ghostty/config.ghostty
.B On macOS,
location of the default configuration file. This location takes precedence over
the XDG environment locations.
.TP
.I $LOCALAPPDATA/ghostty/config.ghostty
.B On Windows,
if
.I $XDG_CONFIG_HOME
is not set,
.I $LOCALAPPDATA
will be searched for configuration files.
.SH ENVIRONMENT
.TP
.B TERM
Defaults to
.BR xterm\-ghostty .
Can be configured with the
.B term
configuration option.
.TP
.B GHOSTTY_RESOURCES_DIR
Where the Ghostty resources can be found.
.TP
.B XDG_CONFIG_HOME
Default location for configuration files.
.TP
.B GHOSTTY_LOG
Controls which destinations receive logs. Ghostty currently defines two
destinations:
.BR stderr " (logging to " stderr ") and " macos
(logging to macOS's unified log; no effect on non-macOS platforms). Combine
values with a comma to enable multiple destinations; prefix a destination with
.B no\-
to disable it. Setting
.B GHOSTTY_LOG
to
.B true
enables all destinations,
.B false
disables all.
.SH BUGS
See GitHub issues: <https://github.com/ghostty-org/ghostty/issues>
.SH AUTHOR
Mitchell Hashimoto <m@mitchellh.com>
.br
Ghostty contributors <https://github.com/ghostty-org/ghostty/graphs/contributors>
.SH "SEE ALSO"
.BR ghostty (5)
"#;

/// `man(7)`-macro body of `share/man/man5/ghostty.5`. Faithful roff
/// transcription of ghostty `mdgen`'s `ghostty_5_header.md` + `ghostty_5_footer.md`
/// prose. See [`MAN1_BODY`] for the identity/deferral notes.
const MAN5_BODY: &str = r#".SH NAME
ghostty \- Ghostty terminal emulator configuration file
.SH DESCRIPTION
To configure Ghostty, you must use a configuration file. GUI-based configuration
is on the roadmap but not yet supported. The configuration file must be placed at
.IR $XDG_CONFIG_HOME/ghostty/config.ghostty ,
which defaults to
.I ~/.config/ghostty/config.ghostty
if the XDG environment is not set.
.PP
.B If you are using macOS,
the configuration file can also be placed at
.IR "$HOME/Library/Application Support/com.mitchellh.ghostty/config.ghostty" .
This is the default configuration location for macOS. It will be searched before
any of the XDG environment locations listed above.
.PP
The file format is documented below as an example:
.PP
.RS
.EX
# The syntax is "key = value". The whitespace around the equals doesn't matter.
background = 282c34
foreground = ffffff

# Blank lines are ignored!

keybind = ctrl+z=close_surface
keybind = ctrl+d=new_split:right

# Colors can be changed by setting the 16 colors of \(gapalette\(ga, with each
# color being defined as regular and bold.
palette = 0=#1d2021
palette = 8=#7c6f64
.EE
.RE
.PP
You can view all available configuration options and their documentation by
executing the command
.B ghostty +show\-config \-\-default \-\-docs
\&. Note that this outputs the full default configuration with docs to stdout, so
you may want to pipe that through a pager, an editor, etc.
.PP
Note: You'll see a lot of seemingly blank configurations like
.BR "font\-family =" .
This is valid syntax to specify the default behavior (no value).
.B +show\-config
outputs it so it's clear that key is defaulting.
.SS "Configuration Errors"
If your configuration file has any errors, Ghostty does its best to ignore them
and move on. Configuration errors will be logged and shown in a dedicated window
on both macOS and Linux (GTK); Ghostty does not treat them as fatal and falls
back to default values for erroneous keys.
.SS "Configuration Options"
The full, always-current catalog of configuration keys — one entry per option,
with its default value and documentation — is printed by
.B ghostty +show\-config \-\-default \-\-docs
\&. You can also read them in the source
.I Config
structure; the available keys are the keys verbatim, and their possible values
are typically documented in the comments.
.SH FILES
.TP
.I $XDG_CONFIG_HOME/ghostty/config.ghostty
Location of the default configuration file.
.TP
.I $HOME/Library/Application Support/com.mitchellh.ghostty/config.ghostty
.B On macOS,
location of the default configuration file. This location takes precedence over
the XDG environment locations.
.TP
.I $LOCALAPPDATA/ghostty/config.ghostty
.B On Windows,
if
.I $XDG_CONFIG_HOME
is not set,
.I $LOCALAPPDATA
will be searched for configuration files.
.SH ENVIRONMENT
.TP
.B XDG_CONFIG_HOME
Default location for configuration files.
.TP
.B $HOME/Library/Application Support/com.mitchellh.ghostty
.B MACOS ONLY
default location for configuration files. This location takes precedence over
the XDG environment locations.
.TP
.B LOCALAPPDATA
.B WINDOWS ONLY:
alternate location to search for configuration files.
.SH BUGS
See GitHub issues: <https://github.com/ghostty-org/ghostty/issues>
.SH AUTHOR
Mitchell Hashimoto <m@mitchellh.com>
.br
Ghostty contributors <https://github.com/ghostty-org/ghostty/graphs/contributors>
.SH "SEE ALSO"
.BR ghostty (1)
"#;

/// Generate the remaining build-phase resource folders under `share_dir`:
/// `man/`, `locale/`, and `bat/`.
///
/// `share_dir` is the `…/zig-out/share` root the `resources` subcommand creates.
/// Returns `Ok(())` after all three subtrees exist and are populated
/// minimal-but-valid (R5). Any I/O failure is surfaced as a `String` error
/// naming the offending path.
pub fn gen_misc(share_dir: &Path) -> Result<(), String> {
    gen_man(share_dir)?;
    gen_locale(share_dir)?;
    gen_bat(share_dir)?;
    Ok(())
}

/// `share/man` — emit real, rendering `roff` man pages `man1/ghostty.1` and
/// `man5/ghostty.5` (P1.3). The `.TH` title line (carrying the live crate
/// version, ghostty's `@@VERSION@@` substitution) is prepended to the
/// [`MAN1_BODY`]/[`MAN5_BODY`] roff bodies. Verify with
/// `man -M <share>/man ghostty` / `man -M <share>/man 5 ghostty`.
fn gen_man(share_dir: &Path) -> Result<(), String> {
    let ver = env!("CARGO_PKG_VERSION");

    let man1_dir = share_dir.join("man").join("man1");
    mkdirp(&man1_dir)?;
    let page1 = format!(
        ".TH GHOSTTY 1 \"\" \"Version {ver}\" \"Ghostty terminal emulator\"\n{MAN1_BODY}"
    );
    write_file(&man1_dir.join("ghostty.1"), page1.as_bytes())?;

    let man5_dir = share_dir.join("man").join("man5");
    mkdirp(&man5_dir)?;
    let page5 = format!(
        ".TH GHOSTTY 5 \"\" \"Version {ver}\" \"Ghostty terminal emulator configuration file\"\n{MAN5_BODY}"
    );
    write_file(&man5_dir.join("ghostty.5"), page5.as_bytes())
}

/// `share/locale` — minimal-valid folder only; rustty i18n is a deferred BREAK,
/// so NO `.mo` content is emitted (never faked). See the module DEFERRAL SUMMARY.
fn gen_locale(share_dir: &Path) -> Result<(), String> {
    let locale_dir = share_dir.join("locale");
    mkdirp(&locale_dir)?;
    write_file(
        &locale_dir.join(".placeholder"),
        b"i18n deferred (BREAK); no compiled .mo catalogs yet",
    )
}

/// `share/bat/syntaxes/ghostty.sublime-syntax` — REAL content: the golden,
/// byte-exact source of the ported `rustty_extra::sublime::syntax()` generator.
/// Mirrors ghostty's install layout (`share/bat/syntaxes`, GhosttyResources.zig).
fn gen_bat(share_dir: &Path) -> Result<(), String> {
    let syntaxes_dir = share_dir.join("bat").join("syntaxes");
    mkdirp(&syntaxes_dir)?;
    write_file(
        &syntaxes_dir.join("ghostty.sublime-syntax"),
        SUBLIME_SYNTAX,
    )
}

fn mkdirp(dir: &Path) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| format!("create_dir_all {}: {e}", dir.display()))
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    fs::write(path, bytes).map_err(|e| format!("write {}: {e}", path.display()))
}
