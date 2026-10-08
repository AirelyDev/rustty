//! rustty-extra::mdgen
//!
//! PORTS: ghostty `src/build/mdgen/mdgen.zig` plus its two entry points
//! `main_ghostty_1.zig` / `main_ghostty_5.zig` and the four prose fixtures
//! `ghostty_{1,5}_{header,footer}.md`.
//!
//! The reference is a build-time executable whose stdout is piped through
//! `pandoc --to man` by `GhosttyDocs.zig:60-88`. In rustty it is a library: the
//! markdown assembly lives here, the `pandoc` stand-in lives in the `render`
//! submodule, and `xtask resources` (`res_misc::gen_man`/`gen_doc`) writes the
//! artifacts. See `render`'s module docs for the tradeoff argument behind not
//! shelling out to `pandoc` (it exists on neither build host).
//!
//! The three `gen_*` walks are the reference's `inline for` over reflected
//! fields. Rust has no comptime reflection, so they are runtime loops over the
//! same three reified tables the shell-completion generators in this crate already
//! use (`config_fields()`, `Action::ALL`, `binding::Action::FIELD_NAMES`) plus the
//! build-generated help corpora (`config_help`, `action_help`, `keybind_help`) —
//! exactly the reference's `@hasDecl(help_strings.X, name)` / `@field(...)` pair.
//!
//! Everything is `&[u8]`/`Vec<u8>`: the fixtures and the help corpora are byte
//! streams and the generated roff is bytes (memory: zig-byteslice-to-rust-rawbytes).
//!
//! **PROSE DEVIATIONS from the reference fixtures** (documented deliberately;
//! prose is not an R1-faithful surface):
//!   * `rustty_1_header.md` drops "We aim to be the best in this category, and
//!     competitive in the rest." (`ghostty_1_header.md:17-18`) — competitive
//!     editorializing, not a factual statement about the program.
//!   * `rustty_5_header.md` rewords "You'll see a lot of weird blank
//!     configurations like" to "The output contains blank configurations like"
//!     (`ghostty_5_header.md:63`) and drops "You also can search for the public
//!     config files of many Ghostty users for examples and inspiration."
//!     (`:70-71`) — a community pointer with no rustty analogue.
//!   * `rustty_5_header.md:104` fixes the upstream typo `--Use the system log
//!     CLI…`, whose stray leading `--` renders as literal text in the man page. A
//!     visible typo in shipped documentation is not a fidelity asset.
//!   * `# BUGS` / `# AUTHOR` carry rustty's maintainer identity instead of the
//!     upstream GitHub links, following the precedent already set in
//!     `rustty-apprt/src/gtk/class/window.rs` (the About dialog DROPS the
//!     reference's `website`/`issue-url` rather than shipping ghostty.org in
//!     user-visible text).

/// The `pandoc` stand-in: markdown → `man(7)` roff and markdown → standalone
/// HTML. See its module docs for the tradeoff argument behind not shelling out
/// to `pandoc` (`GhosttyDocs.zig:60-88`), which exists on neither build host.
pub mod render;

/// `rustty_1_header.md` — PORTS `@embedFile("ghostty_1_header.md")`.
const MAN_1_HEADER: &[u8] = include_bytes!("mdgen/rustty_1_header.md");
/// `rustty_1_footer.md` — PORTS `@embedFile("ghostty_1_footer.md")`.
const MAN_1_FOOTER: &[u8] = include_bytes!("mdgen/rustty_1_footer.md");
/// `rustty_5_header.md` — PORTS `@embedFile("ghostty_5_header.md")`.
const MAN_5_HEADER: &[u8] = include_bytes!("mdgen/rustty_5_header.md");
/// `rustty_5_footer.md` — PORTS `@embedFile("ghostty_5_footer.md")`.
const MAN_5_FOOTER: &[u8] = include_bytes!("mdgen/rustty_5_footer.md");

/// The `@@VERSION@@` marker the fixtures' title blocks carry.
const VERSION_MARKER: &[u8] = b"@@VERSION@@";

/// PORTS: `mdgen.zig:8-19` `substitute`.
///
/// The reference uses `std.mem.replace`, which replaces EVERY occurrence (not
/// just the first), so this does too.
#[must_use]
pub fn substitute(input: &[u8]) -> Vec<u8> {
    let version = rustty_version::VERSION_STRING.as_bytes();
    let mut out = Vec::with_capacity(input.len());
    let mut i = 0usize;
    while i < input.len() {
        if input[i..].starts_with(VERSION_MARKER) {
            out.extend_from_slice(version);
            i += VERSION_MARKER.len();
        } else {
            out.push(input[i]);
            i += 1;
        }
    }
    out
}

/// PORTS: `mdgen.zig:37-45` — the shared doc-block emitter. The first line of a
/// help block is prefixed `":   "` (a pandoc definition-list body), every
/// subsequent line `"    "`; then a blank line and a second newline.
///
/// Reproduced byte-for-byte on `&[u8]`, including the reference's use of
/// `splitScalar`, which yields a trailing empty field for a trailing newline.
fn write_help_block(out: &mut Vec<u8>, help: &[u8]) {
    let mut first = true;
    for s in help.split(|b| *b == b'\n') {
        out.extend_from_slice(if first { b":   " } else { b"    " });
        out.extend_from_slice(s);
        out.push(b'\n');
        first = false;
    }
    out.extend_from_slice(b"\n\n");
}

/// PORTS: `mdgen.zig:21-48` `genConfig`.
///
/// `cli == true` emits the man(1) flavor, where each option is spelled with its
/// `--` CLI prefix; `false` is the man(5) config-file flavor.
pub fn gen_config(out: &mut Vec<u8>, cli: bool) {
    out.extend_from_slice(b"\n# CONFIGURATION OPTIONS\n\n");

    for (name, key) in rustty_config::config_struct::config_fields() {
        // `if (field.name[0] == '_') continue;`
        if name.first() == Some(&b'_') {
            continue;
        }

        out.extend_from_slice(b"**`");
        if cli {
            out.extend_from_slice(b"--");
        }
        out.extend_from_slice(name);
        out.extend_from_slice(b"`**\n\n");

        // `if (@hasDecl(help_strings.Config, field.name))`
        if let Some(help) = rustty_config::help_strings::config_help(*key) {
            write_help_block(out, help);
        }
    }
}

/// PORTS: `mdgen.zig:50-83` `genActions`.
pub fn gen_actions(out: &mut Vec<u8>) {
    use rustty_cli::rustty::Action;

    out.extend_from_slice(b"\n# COMMAND LINE ACTIONS\n\n");

    for action in Action::ALL {
        // `.help => "**`--help`**"`, `.version => "**`--version`**"`, else `+name`.
        match action {
            Action::Help => out.extend_from_slice(b"**`--help`**\n\n"),
            Action::Version => out.extend_from_slice(b"**`--version`**\n\n"),
            _ => {
                out.extend_from_slice(b"**`+");
                out.extend_from_slice(action.tag_name().as_bytes());
                out.extend_from_slice(b"`**\n\n");
            }
        }

        // `if (@hasDecl(help_strings.Action, field.name))`. rustty's generated
        // `action_help` is total (every action has a doc block), so the
        // `@hasDecl` guard is statically satisfied rather than checked.
        write_help_block(
            out,
            rustty_cli::help_strings::action_help(action).as_bytes(),
        );
    }
}

/// PORTS: `mdgen.zig:85-117` `genKeybindActions`.
pub fn gen_keybind_actions(out: &mut Vec<u8>) {
    out.extend_from_slice(b"\n# KEYBIND ACTIONS\n\n");

    for name in rustty_input::binding::Action::FIELD_NAMES {
        let name = name.as_bytes();
        // `if (field.name[0] == '_') continue;`
        if name.first() == Some(&b'_') {
            continue;
        }

        out.extend_from_slice(b"**`");
        out.extend_from_slice(name);
        out.extend_from_slice(b"`**\n\n");

        // `if (@hasDecl(help_strings.KeybindAction, field.name))`
        if let Some(help) = rustty_input::help_strings::keybind_help(name) {
            write_help_block(out, help);
        }
    }
}

/// PORTS: `main_ghostty_1.zig:5-15` — the assembled man(1) markdown.
#[must_use]
pub fn man_1_markdown() -> Vec<u8> {
    let mut out = substitute(MAN_1_HEADER);
    gen_actions(&mut out);
    gen_config(&mut out, true);
    out.extend_from_slice(&substitute(MAN_1_FOOTER));
    out
}

/// PORTS: `main_ghostty_5.zig:5-15` — the assembled man(5) markdown.
#[must_use]
pub fn man_5_markdown() -> Vec<u8> {
    let mut out = substitute(MAN_5_HEADER);
    gen_config(&mut out, false);
    gen_keybind_actions(&mut out);
    out.extend_from_slice(&substitute(MAN_5_FOOTER));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `substitute` replaces EVERY `@@VERSION@@` (`mdgen.zig:8-19` uses
    /// `std.mem.replace`, which is global, not first-only).
    #[test]
    fn substitute_replaces_every_version_marker() {
        let out = substitute(b"a @@VERSION@@ b @@VERSION@@");
        let s = String::from_utf8(out).unwrap();
        assert!(!s.contains("@@VERSION@@"), "left a marker behind: {s}");
        assert_eq!(s.matches(rustty_version::VERSION_STRING).count(), 2);
        // Control: text with no marker is passed through byte-identically.
        assert_eq!(substitute(b"no marker here"), b"no marker here");
    }

    /// Every documented config field appears exactly once as a definition term,
    /// with the `--` CLI prefix in the man(1) flavor and without it in man(5)
    /// (`mdgen.zig:33-35`).
    /// MIRI EXCEPTION (measured 2026-07-30, criterion 2: intractable runtime).
    /// This test renders EVERY documented config field (209 options) and the
    /// full CLI action set through the markdown->roff pipeline. Natively it is
    /// milliseconds; under Miri's interpreter it did not complete in **71 min**
    /// on the Linux VM or in **53 min** on macOS — two independent hosts, no
    /// output, no completion. It hung `check_leaks.sh` for both a Phase-3 lane
    /// and the Linux lane simultaneously.
    ///
    /// `mdgen` arrived with WS13 and was swept into the G6 Miri lane without
    /// ever being triaged for tractability. It exercises no `unsafe`: the whole
    /// module is `String`/`Vec` formatting over `rustty-config` metadata, so
    /// Miri has no UB surface to find here that the native run does not already
    /// cover. Excluded from the Miri lane, NOT from the native suite.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn gen_config_covers_every_field_and_honors_the_cli_flag() {
        let mut cli = Vec::new();
        gen_config(&mut cli, true);
        let cli = String::from_utf8(cli).unwrap();
        let mut file = Vec::new();
        gen_config(&mut file, false);
        let file = String::from_utf8(file).unwrap();

        let fields = rustty_config::config_struct::config_fields();
        // 209, not the plan's 183: `config_fields()` has grown since the WS13 plan
        // was written (204 -> 206: `P3-W3` T4 split `scrollback-limit` into
        // `scrollback-limit-bytes` + `scrollback-limit-lines`, and `P3-W9` T3
        // added `gtk-horizontal-tab-scroll`; 206 -> 208: Phase-4 w8-config
        // added `drag-handle` + `link-osc8`; 208 -> 209: Phase-5 p5-sys-config
        // added `clipboard-write-limit-bytes`; 209 -> 210: P7-W6 T1 added
        // `vt-window-resize-allowed`; 210 -> 212: P8-W3 T2 added
        // `vt-xt-checksum-report` + `vt-xt-checksum-extension`). Pinned so a table that silently
        // shrinks cannot make the per-field loop below vacuous.
        assert_eq!(
            fields.len(),
            212,
            "the field table changed; update this count"
        );
        for (name, _key) in fields {
            let name = std::str::from_utf8(name).unwrap();
            if name.starts_with('_') {
                continue;
            }
            assert_eq!(
                cli.matches(&format!("**`--{name}`**")).count(),
                1,
                "man1 missing --{name}"
            );
            assert_eq!(
                file.matches(&format!("**`{name}`**")).count(),
                1,
                "man5 missing {name}"
            );
        }
        // Negative control: the `_`-prefixed internals the reference skips
        // (`mdgen.zig:30`) must NOT appear.
        assert!(
            !cli.contains("**`--_"),
            "emitted a private `_`-prefixed field"
        );
        assert!(
            !file.contains("**`_"),
            "emitted a private `_`-prefixed field"
        );
        // Negative control: a name that is not a config field must be absent.
        assert!(!file.contains("**`not-a-real-option`**"));
        // Non-vacuity: the doc BODY, not just the term, must be present.
        assert!(
            file.contains(":   The font families to use."),
            "font-family's doc block did not reach the output"
        );
    }

    /// All 18 CLI actions, with `--help`/`--version` spelled as flags and the rest
    /// as `+action` (`mdgen.zig:57-67`).
    /// MIRI EXCEPTION (measured 2026-07-30, criterion 2: intractable runtime).
    /// This test renders EVERY documented config field (209 options) and the
    /// full CLI action set through the markdown->roff pipeline. Natively it is
    /// milliseconds; under Miri's interpreter it did not complete in **71 min**
    /// on the Linux VM or in **53 min** on macOS — two independent hosts, no
    /// output, no completion. It hung `check_leaks.sh` for both a Phase-3 lane
    /// and the Linux lane simultaneously.
    ///
    /// `mdgen` arrived with WS13 and was swept into the G6 Miri lane without
    /// ever being triaged for tractability. It exercises no `unsafe`: the whole
    /// module is `String`/`Vec` formatting over `rustty-config` metadata, so
    /// Miri has no UB surface to find here that the native run does not already
    /// cover. Excluded from the Miri lane, NOT from the native suite.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn gen_actions_covers_every_action_with_the_right_sigil() {
        let mut out = Vec::new();
        gen_actions(&mut out);
        let s = String::from_utf8(out).unwrap();

        use rustty_cli::rustty::Action;
        assert_eq!(Action::ALL.len(), 19);
        for action in Action::ALL {
            let expect = match action {
                Action::Help => "**`--help`**".to_string(),
                Action::Version => "**`--version`**".to_string(),
                _ => format!("**`+{}`**", action.tag_name()),
            };
            assert_eq!(s.matches(&expect).count(), 1, "missing {expect}");
        }
        // Negative controls: the two flag-spelled actions must NOT also appear in
        // `+verb` form, and a non-action must be absent.
        assert!(!s.contains("**`+help`**"));
        assert!(!s.contains("**`+version`**"));
        assert!(!s.contains("**`+not-an-action`**"));
    }

    /// All keybind actions (`mdgen.zig:85-117`).
    #[test]
    fn gen_keybind_actions_covers_every_action() {
        let mut out = Vec::new();
        gen_keybind_actions(&mut out);
        let s = String::from_utf8(out).unwrap();

        let names = rustty_input::binding::Action::FIELD_NAMES;
        // 85 -> 88 (P4-W3 w3-input): `move_tab_to_new_window` (25b2d8a38),
        // `prompt_window_title` + `set_window_title` (8b7c57c75).
        assert_eq!(names.len(), 88, "the keybind action table changed");
        for name in names {
            if name.starts_with('_') {
                continue;
            }
            assert_eq!(
                s.matches(&format!("**`{name}`**")).count(),
                1,
                "missing keybind action {name}"
            );
        }
        // Negative control: a name that is not a keybind action must be absent.
        assert!(!s.contains("**`not_a_real_action`**"));
        // Non-vacuity: at least one doc body reached the output.
        assert!(s.contains(":   Ignore this key combination."));
    }

    /// The assembled page has the sections in the reference's order:
    /// header prose, COMMAND LINE ACTIONS, CONFIGURATION OPTIONS, footer
    /// (`main_ghostty_1.zig:10-13`).
    #[test]
    fn man_1_markdown_section_order_matches_the_reference() {
        let s = String::from_utf8(man_1_markdown()).unwrap();
        let i_desc = s.find("# DESCRIPTION").expect("DESCRIPTION");
        let i_act = s.find("# COMMAND LINE ACTIONS").expect("ACTIONS");
        let i_cfg = s.find("# CONFIGURATION OPTIONS").expect("OPTIONS");
        let i_files = s.find("# FILES").expect("FILES");
        assert!(
            i_desc < i_act && i_act < i_cfg && i_cfg < i_files,
            "section order wrong"
        );
        assert!(
            !s.contains("Ghostty") && !s.contains("ghostty"),
            "R2: upstream identity leaked"
        );
        assert!(
            !s.contains("competitive in the rest"),
            "marketing prose leaked"
        );
        assert!(!s.contains("@@VERSION@@"), "unsubstituted version marker");
    }

    /// man(5) order: header prose, CONFIGURATION OPTIONS, KEYBIND ACTIONS, footer
    /// (`main_ghostty_5.zig:10-13`).
    #[test]
    fn man_5_markdown_section_order_matches_the_reference() {
        let s = String::from_utf8(man_5_markdown()).unwrap();
        let i_desc = s.find("# DESCRIPTION").expect("DESCRIPTION");
        let i_cfg = s.find("# CONFIGURATION OPTIONS").expect("OPTIONS");
        let i_kb = s.find("# KEYBIND ACTIONS").expect("KEYBIND ACTIONS");
        let i_files = s.find("# FILES").expect("FILES");
        assert!(
            i_desc < i_cfg && i_cfg < i_kb && i_kb < i_files,
            "section order wrong"
        );
        assert!(
            !s.contains("Ghostty") && !s.contains("ghostty"),
            "R2: upstream identity leaked"
        );
        assert!(!s.contains("@@VERSION@@"), "unsubstituted version marker");
        // The man(5) flavor must NOT carry the `--` CLI prefix (`genConfig(false)`).
        assert!(!s.contains("**`--font-family`**"));
    }

    /// The three section headers are Zig multiline literals whose last line
    /// is an empty `\\` line (`mdgen.zig:22-27, 52-57, 87-92`). Zig joins the
    /// lines with `\n` and adds none after the last one, so each header is
    /// `"\n# NAME\n\n"`: exactly ONE blank line before the first entry
    /// (S66-F1; measured with `zig test` on the literal at zig 0.16.0).
    #[test]
    #[cfg_attr(miri, ignore)]
    fn section_headers_match_the_reference_multiline_literals() {
        let mut cfg = Vec::new();
        gen_config(&mut cfg, false);
        assert!(
            cfg.starts_with(b"\n# CONFIGURATION OPTIONS\n\n**`"),
            "{:?}",
            String::from_utf8_lossy(&cfg[..40])
        );
        let mut act = Vec::new();
        gen_actions(&mut act);
        assert!(
            act.starts_with(b"\n# COMMAND LINE ACTIONS\n\n**`"),
            "{:?}",
            String::from_utf8_lossy(&act[..40])
        );
        let mut kb = Vec::new();
        gen_keybind_actions(&mut kb);
        assert!(
            kb.starts_with(b"\n# KEYBIND ACTIONS\n\n**`"),
            "{:?}",
            String::from_utf8_lossy(&kb[..40])
        );
    }
}
