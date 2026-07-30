//! PORTS: ghostty `src/main_build_data.zig` — the `+gen-*` CLI that emits one
//! build-time data artifact (a shell completion, an editor syntax file, or the
//! terminfo source) to stdout.
//!
//! The reference's own module doc explains why it exists as a separate binary:
//!
//! > This CLI is used to generate data that is used by the build process.
//! > We used to do this directly in our `build.zig` but the problem with that
//! > approach is that any changes to the dependencies of this data would force a
//! > rebuild of our build binary. If we're just doing something like running
//! > tests and not emitting any of the info below, then that is a complete
//! > waste.
//!
//! **Where it lives in rustty, and why here.** `xtask` IS rustty's `build.zig`
//! (R4: it replaces `src/build/`), and `main_build_data` is a build-tool
//! entrypoint that only `build.zig` invokes — it is not on the shipped `rustty`
//! exe's `exe_entrypoint` switch. `xtask` is also the only crate that already
//! depends on BOTH `rustty-extra` (the five completion/syntax generators) and
//! `rustty-terminfo`, so no new crate edge is needed.
//!
//! It is an `xtask` SUBCOMMAND (`cargo run -p xtask -- gen +bash`) rather than a
//! second binary. The reference splits it out purely to keep `build.zig` from
//! rebuilding when the data dependencies change — a Zig build-graph concern with
//! no Rust analogue, since cargo already recompiles per crate. Keeping it in
//! `xtask` additionally lets the anti-drift test below compare the stdout bytes
//! against `res_shell`/`res_editor`'s installed bytes directly.
//!
//! **The other half of the ledger row** (`DEFERRALS.md`, sub-project #3: "the
//! `+gen-*` CLI/build.rs file-emit wiring (writes the scripts to disk at
//! packaging time)") is `xtask resources` — `res_shell::gen_shell`,
//! `res_editor::gen_editor`, `res_terminfo::gen_terminfo` — which calls the
//! same generators and writes the same bytes into `zig-out/share`. This binary
//! is the stdout-facing surface of that identical byte source; the
//! `gen_actions_match_the_installed_bytes` test below pins the two together so
//! they can never drift.
//!
//! Bytes throughout (memory: zig-byteslice-to-rust-rawbytes): every generator
//! returns `Vec<u8>`/`&[u8]` and is written to stdout unmodified.

use std::io::Write;

/// PORTS: `main_build_data.zig:13-27` `Action`.
///
/// The reference spells the vim actions with Zig's `@"…"` quoted-identifier
/// syntax (`@"vim-syntax"`); Rust variant names cannot contain `-`, so the CLI
/// spelling lives in [`Action::CLI_NAMES`], which is the authority for
/// argument parsing and keeps the wire names byte-identical to the reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    // Shell completions
    Bash,
    Fish,
    Zsh,

    // Editor syntax files
    Sublime,
    VimSyntax,
    VimFtdetect,
    VimFtplugin,
    VimCompiler,

    // Other
    Terminfo,
}

impl Action {
    /// Every action, in the reference's declaration order.
    pub const ALL: [Action; 9] = [
        Action::Bash,
        Action::Fish,
        Action::Zsh,
        Action::Sublime,
        Action::VimSyntax,
        Action::VimFtdetect,
        Action::VimFtplugin,
        Action::VimCompiler,
        Action::Terminfo,
    ];

    /// The reference's enum field names, which are also the CLI action names.
    pub const CLI_NAMES: [&'static str; 9] = [
        "bash",
        "fish",
        "zsh",
        "sublime",
        "vim-syntax",
        "vim-ftdetect",
        "vim-ftplugin",
        "vim-compiler",
        "terminfo",
    ];

    /// PORTS: `cli.action.detectArgs(Action, …)` — match a `+<name>` argument
    /// against the action set. The `+` sigil is the reference's own CLI
    /// convention (`ghostty +gen-bash`-style invocation via `+bash`), and a
    /// bare name is accepted too so the binary is usable directly.
    #[must_use]
    pub fn detect(arg: &str) -> Option<Action> {
        let name = arg.strip_prefix('+').unwrap_or(arg);
        Action::CLI_NAMES
            .iter()
            .position(|n| *n == name)
            .map(|i| Action::ALL[i])
    }

    /// The bytes this action emits. PORTS: `main_build_data.zig:36-46`'s
    /// `switch (action)`.
    ///
    /// `terminfo` is the one arm the reference writes through an encoder rather
    /// than a constant (`terminfo/ghostty.zig`'s `ghostty.encode(writer)`); the
    /// port does the same and then hands over the finished bytes, so every arm
    /// has one shape.
    #[must_use]
    pub fn emit(self) -> Vec<u8> {
        match self {
            // try writer.writeAll(@import("extra/bash.zig").completions);
            Action::Bash => rustty_extra::bash::completions(),
            Action::Fish => rustty_extra::fish::completions(),
            Action::Zsh => rustty_extra::zsh::completions(),
            Action::Sublime => rustty_extra::sublime::syntax(),
            Action::VimSyntax => rustty_extra::vim::syntax(),
            Action::VimFtdetect => rustty_extra::vim::FTDETECT.to_vec(),
            Action::VimFtplugin => rustty_extra::vim::FTPLUGIN.to_vec(),
            Action::VimCompiler => rustty_extra::vim::COMPILER.to_vec(),
            // try @import("terminfo/ghostty.zig").ghostty.encode(writer);
            Action::Terminfo => {
                let mut buf = Vec::new();
                rustty_terminfo::entry::rustty()
                    .encode(&mut buf)
                    .expect("encode terminfo into an in-memory buffer");
                buf
            }
        }
    }
}

/// PORTS: `main_build_data.zig:29-49` `main`.
///
/// `const action = action_ orelse return error.NoAction;` — a missing or
/// unrecognised action is an error, not a silent empty stdout.
pub fn run<I: Iterator<Item = String>>(args: I) -> Result<(), String> {
    let action = args.filter_map(|a| Action::detect(&a)).next();
    let Some(action) = action else {
        return Err(format!(
            "NoAction — expected one of: {}",
            Action::CLI_NAMES.join(", ")
        ));
    };

    // Our output always goes to stdout.
    let bytes = action.emit();
    let mut stdout = std::io::stdout().lock();
    stdout
        .write_all(&bytes)
        .and_then(|()| stdout.flush())
        .map_err(|e| format!("writing to stdout: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every reference action name is recognised, with and without the `+`
    /// sigil, and nothing else is.
    #[test]
    fn detect_matches_exactly_the_reference_action_set() {
        assert_eq!(Action::ALL.len(), Action::CLI_NAMES.len());
        for (i, name) in Action::CLI_NAMES.iter().enumerate() {
            assert_eq!(Action::detect(name), Some(Action::ALL[i]), "{name}");
            assert_eq!(
                Action::detect(&format!("+{name}")),
                Some(Action::ALL[i]),
                "+{name}"
            );
        }
        // Negative controls: near-misses and empty input must NOT resolve.
        assert_eq!(Action::detect("vim"), None);
        assert_eq!(Action::detect("vim_syntax"), None);
        assert_eq!(Action::detect("Bash"), None);
        assert_eq!(Action::detect(""), None);
        assert_eq!(Action::detect("+"), None);
    }

    /// Every action emits non-empty bytes, and no two actions emit the same
    /// bytes — which is what catches a `switch`-arm copy/paste, the single most
    /// likely porting error in a nine-arm dispatch.
    #[test]
    fn every_action_emits_distinct_non_empty_bytes() {
        let mut seen: Vec<(Action, Vec<u8>)> = Vec::new();
        for action in Action::ALL {
            let bytes = action.emit();
            assert!(!bytes.is_empty(), "{action:?} emitted nothing");
            for (other, prev) in &seen {
                assert_ne!(
                    *prev, bytes,
                    "{action:?} emits the same bytes as {other:?} — wrong switch arm"
                );
            }
            seen.push((action, bytes));
        }
    }

    /// The stdout surface and the install surface are the SAME bytes.
    ///
    /// This is the anti-drift pin for the two halves of the ledger row: this
    /// binary is the `+gen-*` CLI, `xtask resources` is the file-emit wiring,
    /// and a packager must get identical content either way.
    #[test]
    fn gen_actions_match_the_installed_bytes() {
        let tmp = std::env::temp_dir().join(format!(
            "ws13-build-data-{}-{}",
            std::process::id(),
            line!()
        ));
        let share = tmp.join("share");
        std::fs::create_dir_all(&share).expect("create temp share");
        crate::res_shell::gen_shell(&share).expect("gen_shell");
        crate::res_editor::gen_editor(&share).expect("gen_editor");

        let app = crate::APP;
        let cases: [(Action, std::path::PathBuf); 7] = [
            (
                Action::Bash,
                share.join(format!("bash-completion/completions/{app}.bash")),
            ),
            (
                Action::Fish,
                share.join(format!("fish/vendor_completions.d/{app}.fish")),
            ),
            (
                Action::Zsh,
                share.join(format!("zsh/site-functions/_{app}")),
            ),
            (
                Action::VimSyntax,
                share.join(format!("vim/vimfiles/syntax/{app}.vim")),
            ),
            (
                Action::VimFtdetect,
                share.join(format!("vim/vimfiles/ftdetect/{app}.vim")),
            ),
            (
                Action::VimFtplugin,
                share.join(format!("vim/vimfiles/ftplugin/{app}.vim")),
            ),
            (
                Action::VimCompiler,
                share.join(format!("vim/vimfiles/compiler/{app}.vim")),
            ),
        ];

        for (action, path) in &cases {
            let installed = std::fs::read(path)
                .unwrap_or_else(|e| panic!("read {} ({action:?}): {e}", path.display()));
            assert_eq!(
                action.emit(),
                installed,
                "{action:?} stdout bytes differ from {}",
                path.display()
            );
        }
        // Non-vacuity: the comparison ran against real, non-trivial content.
        assert!(cases.iter().all(|(a, _)| a.emit().len() > 100));

        std::fs::remove_dir_all(&tmp).ok();
    }
}
