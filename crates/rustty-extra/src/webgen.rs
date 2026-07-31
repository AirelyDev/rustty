//! rustty-extra::webgen
//!
//! PORTS: ghostty `src/build/webgen/main_config.zig`, `main_actions.zig` and
//! `main_commands.zig` — the three tiny executables `GhosttyWebdata.zig:44-48,
//! 74-79,113-116` runs to produce `share/ghostty/webdata/{config,actions,
//! commands}.mdx`, the MDX corpus the documentation website consumes.
//!
//! Each reference program is a `main()` that writes one document to stdout. In
//! rustty they are library functions returning the bytes, and `xtask resources`
//! (`res_misc::gen_webdata`) writes them — the same shape `mdgen` takes.
//!
//! The three walks are the reference's `inline for` over reflected fields;
//! Rust has no comptime reflection, so they are runtime loops over the same
//! reified tables `mdgen` uses (`config_fields()`, `Action::ALL`) plus the
//! build-generated help corpora. `actions.mdx` needs no walk at all: the
//! reference's `main_actions.zig` is a four-line wrapper over
//! `helpgen_actions.generate(writer, .markdown, true, …)`, which rustty already
//! ports verbatim at `rustty_input::helpgen_actions::generate`, so this module
//! delegates rather than growing a second, drifting implementation.
//!
//! Bytes throughout (memory: zig-byteslice-to-rust-rawbytes): the help corpora
//! are `&[u8]` and the `.mdx` output is a byte stream.
//!
//! ## R2 identity deviations from the reference frontmatter
//!
//! The reference's frontmatter carries `editOnGithubLink:` pointing at
//! `github.com/ghostty-org/ghostty`. Shipping that in rustty's own generated
//! documentation would put the upstream product's repository in user-visible
//! output, which R2 forbids; and rustty has no public repository to substitute.
//! Following the precedent already set by the GTK About dialog
//! (`rustty-apprt/src/gtk/class/window.rs`, which DROPS the reference's
//! `website`/`issue-url` rather than shipping ghostty.org), the key is retained
//! but resolved to the in-repo SOURCE PATH a contributor would actually edit.
//! That keeps the frontmatter schema intact and the pointer truthful.

use rustty_cfg_fmt::AllocatingWriter;

/// PORTS: `main_config.zig:14-31` — the `config.mdx` frontmatter + preamble.
///
/// The reference's closing sentence "In the future, we'll have a more
/// user-friendly way to view and organize these options." is dropped: it is a
/// roadmap promise about the upstream project's website, not a fact about the
/// document (memory: strip-ghostty-gag-prose).
const CONFIG_HEADER: &[u8] = b"---\ntitle: Reference\ndescription: Reference of all Rustty configuration options.\neditOnGithubLink: crates/rustty-config/src/config_struct.rs\n---\n\nThis is a reference of all Rustty configuration options. These\noptions are ordered roughly by how common they are to be used\nand grouped with related options. I recommend utilizing your\nbrowser's search functionality to find the option you're looking\nfor.\n\n\n";

/// PORTS: `main_commands.zig:16-27` — the `commands.mdx` frontmatter.
const COMMANDS_HEADER: &[u8] = b"---\ntitle: Reference\ndescription: Reference of all Rustty action subcommands.\neditOnGithubLink: crates/rustty-cli/src\n---\nRustty includes a number of utility actions that can be accessed as subcommands.\nActions provide utilities to work with config, list keybinds, list fonts, demo themes,\nand debug.\n";

/// PORTS: `main_config.zig:64-79` — the rough markdown "parsing" the reference
/// does so the website's parser sees triple-backtick code fences and MDX
/// callout components instead of the indented code blocks and `Note:` /
/// `Warning:` paragraph conventions used in the doc comments.
#[derive(Clone, Copy, PartialEq, Eq)]
enum BlockKind {
    /// Plaintext, do nothing.
    Text,
    /// Code block, wrap in triple backticks.
    Code,
    /// `<Note>` callout.
    CalloutNote,
    /// `<Warning>` callout.
    CalloutWarning,
}

/// PORTS: `main_config.zig:127-135` `endBlock`.
fn end_block(out: &mut Vec<u8>, block: Option<BlockKind>) {
    match block {
        Some(BlockKind::Code) => out.extend_from_slice(b"```\n"),
        Some(BlockKind::CalloutNote) => out.extend_from_slice(b"</Note>\n"),
        Some(BlockKind::CalloutWarning) => out.extend_from_slice(b"</Warning>\n"),
        Some(BlockKind::Text) | None => {}
    }
}

/// `std.ascii.startsWithIgnoreCase`.
fn starts_with_ignore_case(hay: &[u8], needle: &[u8]) -> bool {
    hay.len() >= needle.len() && hay[..needle.len()].eq_ignore_ascii_case(needle)
}

/// PORTS: `main_config.zig:12-125` `genConfig`.
#[must_use]
pub fn config_mdx() -> Vec<u8> {
    let mut out = CONFIG_HEADER.to_vec();

    let fields = rustty_config::config_struct::config_fields();
    for (i, (name, key)) in fields.iter().enumerate() {
        // `if (field.name[0] == '_') continue;`
        if name.first() == Some(&b'_') {
            continue;
        }
        // `if (!@hasDecl(help_strings.Config, field.name)) continue;`
        let Some(help) = rustty_config::help_strings::config_help(*key) else {
            continue;
        };

        out.extend_from_slice(b"## `");
        out.extend_from_slice(name);
        out.extend_from_slice(b"`\n");

        // "For all subsequent fields with no docs, they are grouped with the
        // previous field." (`main_config.zig:43-56`)
        for (next_name, next_key) in &fields[i + 1..] {
            if next_name.first() == Some(&b'_') {
                break;
            }
            if rustty_config::help_strings::config_help(*next_key).is_some() {
                break;
            }
            out.extend_from_slice(b"## `");
            out.extend_from_slice(next_name);
            out.extend_from_slice(b"`\n");
        }

        out.push(b'\n');

        let mut block: Option<BlockKind> = None;
        for s in help.split(|b| *b == b'\n') {
            // `if (std.mem.eql(u8, s, ""))` — an empty line resets the block.
            if s.is_empty() {
                end_block(&mut out, block);
                block = None;
                out.push(b'\n');
                continue;
            }

            let first = block.is_none();
            if block.is_none() {
                if s.starts_with(b"    ") {
                    block = Some(BlockKind::Code);
                    out.extend_from_slice(b"```\n");
                } else if starts_with_ignore_case(s, b"note:") {
                    block = Some(BlockKind::CalloutNote);
                    out.extend_from_slice(b"<Note>\n");
                } else if starts_with_ignore_case(s, b"warning:") {
                    block = Some(BlockKind::CalloutWarning);
                    out.extend_from_slice(b"<Warning>\n");
                } else {
                    block = Some(BlockKind::Text);
                }
            }

            match block {
                Some(BlockKind::Text) | None => out.extend_from_slice(s),
                Some(BlockKind::CalloutNote) => {
                    out.extend_from_slice(if first { &s[b"note:".len()..] } else { s });
                }
                Some(BlockKind::CalloutWarning) => {
                    out.extend_from_slice(if first { &s[b"warning:".len()..] } else { s });
                }
                Some(BlockKind::Code) => {
                    out.extend_from_slice(if s.starts_with(b"    ") { &s[4..] } else { s });
                }
            }
            out.push(b'\n');
        }
        end_block(&mut out, block);
        out.push(b'\n');
    }

    out
}

/// PORTS: `main_actions.zig:5-9` — a wrapper over the already-ported
/// `helpgen_actions.generate(writer, .markdown, true, …)`.
#[must_use]
pub fn actions_mdx() -> Vec<u8> {
    let mut w = AllocatingWriter::init();
    let Ok(()) = rustty_input::helpgen_actions::generate(
        &mut w,
        rustty_input::helpgen_actions::Format::Markdown,
        true,
        &rustty_input::helpgen_actions::RealHelpStrings,
    );
    w.written().to_vec()
}

/// PORTS: `main_commands.zig:14-52` `genActions`.
#[must_use]
pub fn commands_mdx() -> Vec<u8> {
    use rustty_cli::rustty::Action;

    let mut out = COMMANDS_HEADER.to_vec();

    for action in Action::ALL {
        // The reference's `switch` has identical arms for `.help`/`.version` and
        // everything else — the heading is always `## <name>`. Only the fenced
        // INVOCATION below distinguishes them.
        out.extend_from_slice(b"## ");
        out.extend_from_slice(action.tag_name().as_bytes());
        out.push(b'\n');

        // `if (@hasDecl(help_strings.Action, field.name))`. rustty's generated
        // `action_help` is total, so the guard is statically satisfied.
        for s in rustty_cli::help_strings::action_help(action)
            .as_bytes()
            .split(|b| *b == b'\n')
        {
            out.extend_from_slice(s);
            out.push(b'\n');
        }
        out.extend_from_slice(b"\n```\n");
        match action {
            Action::Help | Action::Version => {
                out.extend_from_slice(rustty_identity::APP_NAME.as_bytes());
                out.extend_from_slice(b" --");
            }
            _ => {
                out.extend_from_slice(rustty_identity::APP_NAME.as_bytes());
                out.extend_from_slice(b" +");
            }
        }
        out.extend_from_slice(action.tag_name().as_bytes());
        out.extend_from_slice(b"\n```\n\n");
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `config.mdx` carries the frontmatter + one ``## `field` `` per documented
    /// field, and undocumented fields are GROUPED under the preceding documented
    /// one (`main_config.zig:43-56`) rather than dropped.
    /// MIRI EXCEPTION (measured 2026-07-31, criterion 2: intractable runtime).
    /// Natively **0.03 s**; under Miri it did not complete in **1500 s** (25 min)
    /// on an otherwise-idle macOS host — the process printed `running 1 test`
    /// and produced no result before the bound. The cost is structural, not
    /// incidental: the body runs `s.matches(&format!("\n## `{name}`\n")).count()`
    /// once per public config field (206 of them) against the whole rendered
    /// document, plus a `to_lowercase()` over it — O(fields x len) substring
    /// scanning, which is trivial natively and brutal byte-by-byte under an
    /// interpreter.
    ///
    /// It exercises **no `unsafe`**: the module is `String`/`Vec` formatting over
    /// `rustty-config` metadata, so Miri has no UB surface here that the native
    /// run does not already cover. Excluded from the Miri lane, NOT the native
    /// suite.
    ///
    /// THIS IS THE THIRD TEST OF THIS FAMILY, and finding it took three tries
    /// worth recording. The first fix excluded the two `mdgen` coverage tests and
    /// was reported as unblocking the `rustty-extra` Miri lane; it did not — the
    /// lane hung again here. Then two attempts to MEASURE this one were
    /// themselves vacuous: `timeout` does not exist on macOS, so the first
    /// returned success after 0 s having run nothing. A 0-second Miri run is
    /// impossible, which is the only reason it was caught.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn config_mdx_frontmatter_and_grouping() {
        let s = String::from_utf8(config_mdx()).unwrap();
        assert!(s.starts_with("---\ntitle: Reference\n"), "no frontmatter");
        assert!(s.contains("description: Reference of all Rustty configuration options."));

        let fields = rustty_config::config_struct::config_fields();
        let public: Vec<_> = fields
            .iter()
            .filter(|(n, _)| n.first() != Some(&b'_'))
            .collect();
        // EVERY public field — documented or grouped — must reach a heading.
        for (name, _) in &public {
            let name = std::str::from_utf8(name).unwrap();
            assert_eq!(
                s.matches(&format!("\n## `{name}`\n")).count(),
                1,
                "config.mdx is missing a heading for {name}"
            );
        }
        assert!(
            s.matches("\n## `").count() >= 183,
            "expected >=183 headings, got {}",
            s.matches("\n## `").count()
        );
        // Negative controls: no private field, no upstream identity.
        assert!(!s.contains("## `_"), "emitted a private `_`-prefixed field");
        assert!(!s.to_lowercase().contains("ghostty"), "R2: identity leaked");
        assert!(!s.contains("ghostty-org"), "upstream repo link leaked");
        assert!(!s.contains("## `not-a-real-option`"));
    }

    /// The `code` / `<Note>` / `<Warning>` block detection round-trips
    /// (`main_config.zig:60-125`): every opened callout and fence is closed.
    #[test]
    fn config_mdx_blocks_are_balanced() {
        let s = String::from_utf8(config_mdx()).unwrap();
        assert_eq!(s.matches("<Note>").count(), s.matches("</Note>").count());
        assert_eq!(
            s.matches("<Warning>").count(),
            s.matches("</Warning>").count()
        );
        assert_eq!(s.matches("```").count() % 2, 0, "unbalanced code fence");
        // Non-vacuity: the corpus really does exercise all three block kinds, so
        // the equalities above are not comparing 0 with 0.
        assert!(s.contains("<Note>"), "no Note callout was produced");
        assert!(s.contains("<Warning>"), "no Warning callout was produced");
        assert!(s.contains("```"), "no code fence was produced");
        // The `Note:`/`Warning:` prefix is consumed, not re-emitted.
        assert!(!s.contains("<Note>\nNote:"), "callout prefix not stripped");
    }

    /// `actions.mdx` is byte-identical to the already-ported keybind-action
    /// markdown generator's output — the reference's `main_actions.zig` is a
    /// four-line wrapper over it, so any divergence would mean the port grew a
    /// second, drifting implementation.
    #[test]
    fn actions_mdx_delegates_to_helpgen_actions() {
        let mut w = AllocatingWriter::init();
        rustty_input::helpgen_actions::generate(
            &mut w,
            rustty_input::helpgen_actions::Format::Markdown,
            true,
            &rustty_input::helpgen_actions::RealHelpStrings,
        )
        .unwrap();
        assert_eq!(actions_mdx(), w.written());
        // Non-vacuity: the delegate produced real content, not an empty buffer.
        assert!(actions_mdx().len() > 1000, "suspiciously small actions.mdx");
    }

    /// `commands.mdx` documents every CLI action with the right invocation
    /// fence (`--help`/`--version` vs `+action`) (`main_commands.zig:29-52`).
    #[test]
    fn commands_mdx_covers_every_cli_action() {
        use rustty_cli::rustty::Action;
        let s = String::from_utf8(commands_mdx()).unwrap();
        assert!(s.starts_with("---\ntitle: Reference\n"));
        assert_eq!(Action::ALL.len(), 18);
        for action in Action::ALL {
            let name = action.tag_name();
            assert_eq!(
                s.matches(&format!("\n## {name}\n")).count(),
                1,
                "missing heading for {name}"
            );
            let invocation = match action {
                Action::Help | Action::Version => {
                    format!("{} --{name}\n", rustty_identity::APP_NAME)
                }
                _ => format!("{} +{name}\n", rustty_identity::APP_NAME),
            };
            assert!(s.contains(&invocation), "missing invocation {invocation:?}");
        }
        assert_eq!(s.matches("\n## ").count(), 18);
        // Negative controls.
        assert!(
            !s.contains(&format!("{} +help\n", rustty_identity::APP_NAME)),
            "help spelled as a subcommand"
        );
        assert!(!s.to_lowercase().contains("ghostty"), "R2: identity leaked");
        assert_eq!(s.matches("```").count() % 2, 0, "unbalanced code fence");
    }
}
