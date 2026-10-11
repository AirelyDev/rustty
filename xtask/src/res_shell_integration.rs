//! Shell-integration resource emission for the macOS Copy-Resources phase.
//!
//! PORTS: the "Shell-integration" `addInstallDirectory` block of ghostty's
//! `src/build/GhosttyResources.zig` — it copies `src/shell-integration/`
//! verbatim into `share/ghostty/shell-integration/` with
//! `exclude_extensions = &.{".md"}`. ghostty installs the tree straight from
//! its source checkout; rustty instead embeds a **vendored** copy of that tree
//! (`crates/rustty-extra/shell-integration/`, see its `VENDORING.md`) via
//! `include_bytes!` so there is NO build/run dependency on the ghostty checkout
//! (R3 self-contained). The vendored bytes were originally byte-identical to
//! ghostty's (R1 fidelity); Task 13 has since rewritten them to the rustty
//! product identity (see below).
//!
//! R2 identity note (STALE PRE-TASK-13 CLAIM CORRECTED): this doc used to claim
//! these scripts "keep ghostty's protocol/OSC identifiers and the
//! `xterm-ghostty` / `GHOSTTY_*` / `ghostty` command names unchanged". That is
//! now FALSE — rebrand Task 13 renamed the five vendored scripts
//! (`rustty.bash`, `rustty-integration.elv`, `rustty-shell-integration.fish`,
//! `rustty.nu`, `rustty-integration`) and rewrote every `GHOSTTY_*` env var and
//! `ghostty` self-reference inside them to `RUSTTY_*`/`rustty`
//! unconditionally (no `identity-oracle` dual-profile for this content). The
//! DESTINATION keys below (`FILES`) were updated in Task 14 to match those
//! renamed sources exactly. The enclosing `share/{APP}/shell-integration/`
//! root DOES still route through [`crate::APP`] (Task 14): `rustty` by
//! default, `ghostty` under `identity-oracle` — matching the runtime lookup
//! (`rustty_os::resourcesdir` / `rustty_identity::APP_NAME`).
//!
//! R5: every emitted file carries REAL vendored content (never faked); `*.md`
//! files (ghostty's `README.md`, rustty's `VENDORING.md`) are excluded from the
//! bundle, exactly matching ghostty's `exclude_extensions`.

use std::path::Path;

/// The vendored shell-integration tree, as `(relative-path, bytes)` pairs.
///
/// Paths are the install-relative layout under
/// `share/{APP}/shell-integration/` (mirroring ghostty's own
/// `share/ghostty/shell-integration/` install layout — see the module R2
/// note). The bytes are embedded at compile time from the vendored copy in
/// `rustty-extra`, so a missing vendored file is a build error (guarantees
/// non-empty, real content).
///
/// The destination basenames below are LITERAL (not `crate::APP`-routed):
/// Task 13 renamed the five scripts unconditionally to their rustty spelling
/// (no `identity-oracle` variant exists for this content), so the
/// destination key simply matches each renamed source file's actual name.
const FILES: &[(&str, &[u8])] = &[
    (
        "bash/bash-preexec.sh",
        include_bytes!("../../crates/rustty-extra/shell-integration/bash/bash-preexec.sh"),
    ),
    (
        // PORT NOTE (Task 13 + 14): source file renamed ghostty.bash ->
        // rustty.bash; this destination key now matches it exactly.
        "bash/rustty.bash",
        include_bytes!("../../crates/rustty-extra/shell-integration/bash/rustty.bash"),
    ),
    (
        "elvish/lib/rustty-integration.elv",
        include_bytes!(
            "../../crates/rustty-extra/shell-integration/elvish/lib/rustty-integration.elv"
        ),
    ),
    (
        "fish/vendor_conf.d/rustty-shell-integration.fish",
        include_bytes!(
            "../../crates/rustty-extra/shell-integration/fish/vendor_conf.d/rustty-shell-integration.fish"
        ),
    ),
    (
        "nushell/vendor/autoload/rustty.nu",
        include_bytes!(
            "../../crates/rustty-extra/shell-integration/nushell/vendor/autoload/rustty.nu"
        ),
    ),
    (
        "zsh/.zshenv",
        include_bytes!("../../crates/rustty-extra/shell-integration/zsh/.zshenv"),
    ),
    (
        "zsh/rustty-integration",
        include_bytes!("../../crates/rustty-extra/shell-integration/zsh/rustty-integration"),
    ),
];

/// Emit `share/{APP}/shell-integration/` from the vendored tree (`{APP}` =
/// [`crate::APP`], Task 14).
///
/// `share_dir` is the `share/` directory (e.g. `zig-out/share`). Writes every
/// file in [`FILES`] under `share_dir/{APP}/shell-integration/`, creating
/// parent directories as needed. (The pre-WS1 `DEFERRALS.md` note-annotation
/// step is gone — the emitted tree IS the real content.)
pub fn gen_shell_integration(share_dir: &Path) -> Result<(), String> {
    let root = share_dir.join(crate::APP).join("shell-integration");

    for (rel, bytes) in FILES {
        if bytes.is_empty() {
            return Err(format!("vendored shell-integration file is empty: {rel}"));
        }
        let path = root.join(rel);
        let parent = path
            .parent()
            .ok_or_else(|| format!("no parent for {}", path.display()))?;
        std::fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
        std::fs::write(&path, bytes).map_err(|e| format!("write {}: {e}", path.display()))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// End-to-end: gen_shell_integration writes the full vendored file set with
    /// real (non-empty) content and no `*.md` files leak into the bundle.
    #[test]
    fn emits_real_vendored_tree_excluding_md() {
        let base =
            std::env::temp_dir().join(format!("rustty-xtask-shellint-test-{}", std::process::id()));
        let share = base.join("share");
        std::fs::create_dir_all(&share).unwrap();

        gen_shell_integration(&share).expect("gen_shell_integration");

        // WS1: this generator no longer writes/annotates any DEFERRALS.md note.
        assert!(!share.join(crate::APP).join("DEFERRALS.md").exists());

        let root = share.join(crate::APP).join("shell-integration");
        // Every expected file exists and is non-empty.
        for (rel, _) in FILES {
            let p = root.join(rel);
            let meta = std::fs::metadata(&p).unwrap_or_else(|_| panic!("missing {}", p.display()));
            assert!(meta.len() > 0, "empty {}", p.display());
        }
        // The canonical zsh integration file matches its vendored bytes exactly.
        let zsh = std::fs::read(root.join("zsh/rustty-integration")).unwrap();
        assert_eq!(
            zsh.as_slice(),
            include_bytes!("../../crates/rustty-extra/shell-integration/zsh/rustty-integration")
                .as_slice()
        );
        // No `*.md` file was emitted (README.md / VENDORING.md are excluded).
        for entry in walk(&root) {
            assert!(
                entry.extension().and_then(|e| e.to_str()) != Some("md"),
                "unexpected .md in bundle: {}",
                entry.display()
            );
        }

        let _ = std::fs::remove_dir_all(&base);
    }

    /// W11-01 regression: the Nushell `ssh` and `sudo` wrappers must each carry
    /// the `@complete external` attribute (oracle ghostty.nu:9/:28) so that a
    /// configured Nushell external completer keeps completing arguments inside
    /// the wrapped commands. Runs in default `cargo test` (no oracle checkout
    /// needed) as drift coverage over the embedded vendored bytes.
    #[test]
    fn nushell_wrappers_delegate_to_external_completer() {
        let nu = FILES
            .iter()
            .find(|(rel, _)| *rel == "nushell/vendor/autoload/rustty.nu")
            .expect("rustty.nu missing from FILES")
            .1;
        let nu = std::str::from_utf8(nu).expect("rustty.nu is UTF-8");
        let count = nu.matches("@complete external").count();
        assert_eq!(
            count, 2,
            "expected exactly 2 `@complete external` attributes (ssh + sudo wrappers), found {count}"
        );
        // Each attribute must sit immediately before its wrapped def.
        for def in ["export def --wrapped ssh", "export def --wrapped sudo"] {
            let idx = nu.find(def).unwrap_or_else(|| panic!("missing `{def}`"));
            let preceding = &nu[..idx];
            let prev_line = preceding.trim_end().lines().next_back().unwrap_or("");
            assert_eq!(
                prev_line.trim(),
                "@complete external",
                "`{def}` is not annotated with `@complete external`"
            );
        }
    }

    fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
        let mut out = Vec::new();
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    out.extend(walk(&p));
                } else {
                    out.push(p);
                }
            }
        }
        out
    }

    /// Per-token product rebrand (R2, rename-casing-fidelity): applied to both
    /// file names and file contents when mapping oracle -> vendored.
    fn rebrand(s: &str) -> String {
        s.replace("GHOSTTY", "RUSTTY")
            .replace("Ghostty", "Rustty")
            .replace("ghostty", "rustty")
    }

    /// Oracle-relative paths (post-rebrand spelling) whose CONTENT is compared
    /// VERBATIM against the oracle instead of through [`rebrand`]. Every entry
    /// requires a written justification:
    ///
    /// * `bash/bash-preexec.sh` — third-party vendored file (rcaloras/bash-preexec,
    ///   MIT). Task 13 deliberately did NOT rebrand it: its `# GHOSTTY:` comment
    ///   lines are ghostty-upstream PATCH ANNOTATIONS inside third-party code
    ///   (provenance markers), not product identity, and the file contains no
    ///   `GHOSTTY_*` env vars or `ghostty` command references.
    const VERBATIM_FILES: &[&str] = &["bash/bash-preexec.sh"];

    /// The read-only ghostty oracle checkout: `ghostty` BESIDE THE MAIN
    /// rustty checkout, the rule `.port/tools/gates/_gatelib.oracle_root` and
    /// its shell twin `_oracle_root.sh` apply for every gate.
    ///
    /// `common_dir` is git's `--git-common-dir` answer for this crate's tree:
    /// the MAIN repository's `.git` even from a linked worktree such as
    /// `.claude/worktrees/<id>`, so its parent is the main checkout. Without it
    /// (git absent or failing) the fallback is this checkout's own sibling,
    /// `<manifest>/../../ghostty`, which from a worktree names a missing
    /// directory and so FAILS loudly rather than finding a stray tree. There is
    /// no environment override, as in the gates (R5). This replaced a literal
    /// path to one developer's machine.
    fn oracle_checkout_from(common_dir: Option<&Path>, manifest_dir: &Path) -> std::path::PathBuf {
        match common_dir.and_then(Path::parent) {
            Some(main_checkout) => main_checkout.join("../ghostty"),
            None => manifest_dir.join("../../ghostty"),
        }
    }

    /// [`oracle_checkout_from`] with git's live answer for this crate. git runs
    /// with every `GIT_*` variable removed, so `GIT_DIR` cannot redirect it to
    /// a decoy repository, and its stdout is trusted only on exit 0.
    fn oracle_checkout() -> std::path::PathBuf {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut git = std::process::Command::new("git");
        git.args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
            .current_dir(manifest_dir);
        for (k, _) in std::env::vars_os() {
            if k.to_string_lossy().starts_with("GIT_") {
                git.env_remove(k);
            }
        }
        let common = git
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| std::path::PathBuf::from(s.trim_end()))
            .filter(|p| !p.as_os_str().is_empty());
        oracle_checkout_from(common.as_deref(), manifest_dir)
    }

    /// The location rule above, without git or an oracle on disk: from a
    /// linked worktree the oracle is the MAIN checkout's sibling (not the
    /// worktree's, `.claude/worktrees/ghostty`), from the main checkout it is
    /// that checkout's sibling, and with no git answer it falls back to the
    /// crate's own `../../ghostty`.
    #[test]
    fn oracle_checkout_is_beside_the_main_checkout() {
        let main = Path::new("/r/rustty");
        let wt_manifest = Path::new("/r/rustty/.claude/worktrees/w/xtask");
        let main_manifest = Path::new("/r/rustty/xtask");
        let common = main.join(".git");
        assert_eq!(
            oracle_checkout_from(Some(&common), wt_manifest),
            Path::new("/r/rustty/../ghostty")
        );
        assert_eq!(
            oracle_checkout_from(Some(&common), main_manifest),
            Path::new("/r/rustty/../ghostty")
        );
        assert_eq!(
            oracle_checkout_from(None, wt_manifest),
            Path::new("/r/rustty/.claude/worktrees/w/xtask/../../ghostty")
        );
    }

    /// R1/R2 oracle KAT (dev-only): the vendored shell-integration tree must
    /// equal ghostty's `src/shell-integration/` tree mapped through the
    /// per-token rebrand (names + contents), excluding `*.md` on both sides,
    /// with [`VERBATIM_FILES`] compared byte-for-byte instead.
    ///
    /// Run explicitly (never in default `cargo test` — R3 no oracle dep):
    ///   cargo test -p xtask -- --ignored oracle_shell_integration_parity
    #[test]
    #[ignore = "requires the read-only ghostty oracle checkout beside the main checkout (R3: dev-only)"]
    fn oracle_shell_integration_parity() {
        // The oracle beside the MAIN checkout, from a plain checkout or any
        // linked worktree ([`oracle_checkout`]).
        let oracle_checkout = oracle_checkout();
        let oracle_dir = oracle_checkout.join("src/shell-integration");
        assert!(
            oracle_dir.is_dir(),
            "ghostty oracle not found: expected {} (`ghostty` beside the main rustty \
             checkout, located through `git rev-parse --git-common-dir`); this KAT needs \
             the read-only oracle checkout there",
            oracle_dir.display()
        );
        let oracle_root = oracle_dir.as_path();
        let vendored_root =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/rustty-extra/shell-integration");

        let mut failures: Vec<String> = Vec::new();
        let mut mapped_set: Vec<std::path::PathBuf> = Vec::new();

        // Forward direction: every oracle file (non-.md, incl. dotfiles like
        // zsh/.zshenv) must exist vendored under the rebranded name with the
        // expected content.
        for oracle_path in walk(oracle_root) {
            if oracle_path.extension().and_then(|e| e.to_str()) == Some("md") {
                continue; // README.md — excluded on both sides
            }
            let rel = oracle_path
                .strip_prefix(oracle_root)
                .unwrap()
                .to_str()
                .expect("oracle path is UTF-8");
            let mapped_rel = rebrand(rel);
            let vendored_path = vendored_root.join(&mapped_rel);
            mapped_set.push(vendored_path.clone());

            let Ok(vendored) = std::fs::read_to_string(&vendored_path) else {
                failures.push(format!(
                    "MISSING vendored file: {mapped_rel} (from oracle {rel})"
                ));
                continue;
            };
            let oracle = std::fs::read_to_string(&oracle_path)
                .unwrap_or_else(|e| panic!("read oracle {rel}: {e}"));
            let expected = if VERBATIM_FILES.contains(&mapped_rel.as_str()) {
                oracle
            } else {
                rebrand(&oracle)
            };
            if vendored != expected {
                failures.push(format!("CONTENT DIVERGES: {mapped_rel}"));
            }
        }

        // Reverse direction: no vendored non-.md file exists that the mapped
        // oracle set does not account for (catches stray additions).
        for vendored_path in walk(&vendored_root) {
            if vendored_path.extension().and_then(|e| e.to_str()) == Some("md") {
                continue; // README.md / VENDORING.md — repo docs, never emitted
            }
            if !mapped_set.contains(&vendored_path) {
                failures.push(format!(
                    "EXTRA vendored file with no oracle counterpart: {}",
                    vendored_path
                        .strip_prefix(&vendored_root)
                        .unwrap()
                        .display()
                ));
            }
        }

        assert!(
            failures.is_empty(),
            "vendored shell-integration tree diverged from the oracle under the \
             rebrand mapping:\n  {}",
            failures.join("\n  ")
        );
    }
}
