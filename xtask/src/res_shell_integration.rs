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
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("create {}: {e}", parent.display()))?;
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
        let base = std::env::temp_dir()
            .join(format!("rustty-xtask-shellint-test-{}", std::process::id()));
        let share = base.join("share");
        std::fs::create_dir_all(&share).unwrap();

        gen_shell_integration(&share).expect("gen_shell_integration");

        // WS1: this generator no longer writes/annotates any DEFERRALS.md note.
        assert!(!share.join(crate::APP).join("DEFERRALS.md").exists());

        let root = share.join(crate::APP).join("shell-integration");
        // Every expected file exists and is non-empty.
        for (rel, _) in FILES {
            let p = root.join(rel);
            let meta = std::fs::metadata(&p)
                .unwrap_or_else(|_| panic!("missing {}", p.display()));
            assert!(meta.len() > 0, "empty {}", p.display());
        }
        // The canonical zsh integration file matches its vendored bytes exactly.
        let zsh = std::fs::read(root.join("zsh/rustty-integration")).unwrap();
        assert_eq!(
            zsh.as_slice(),
            include_bytes!(
                "../../crates/rustty-extra/shell-integration/zsh/rustty-integration"
            )
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

    /// R1/R2 oracle KAT (dev-only): the vendored shell-integration tree must
    /// equal ghostty's `src/shell-integration/` tree mapped through the
    /// per-token rebrand (names + contents), excluding `*.md` on both sides,
    /// with [`VERBATIM_FILES`] compared byte-for-byte instead.
    ///
    /// Run explicitly (never in default `cargo test` — R3 no oracle dep):
    ///   cargo test -p xtask -- --ignored oracle_shell_integration_parity
    #[test]
    #[ignore = "requires the read-only ghostty oracle checkout at ../ghostty (R3: dev-only)"]
    fn oracle_shell_integration_parity() {
        // Candidate oracle locations: `../../ghostty` next to a plain checkout
        // of rustty, OR (when this crate is built from inside a git worktree
        // nested under the main checkout, e.g. `.claude/worktrees/<id>/xtask`)
        // the fixed dev-machine location `/Users/gael/projects/ghostty`.
        let candidates = [
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ghostty/src/shell-integration"),
            Path::new("/Users/gael/projects/ghostty/src/shell-integration").to_path_buf(),
        ];
        let oracle_root = candidates
            .iter()
            .find(|p| p.is_dir())
            .unwrap_or_else(|| {
                panic!(
                    "ghostty oracle not found in any of {:?} — this KAT only runs on the dev machine",
                    candidates
                )
            });
        let vendored_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../crates/rustty-extra/shell-integration");

        let mut failures: Vec<String> = Vec::new();
        let mut mapped_set: Vec<std::path::PathBuf> = Vec::new();

        // Forward direction: every oracle file (non-.md, incl. dotfiles like
        // zsh/.zshenv) must exist vendored under the rebranded name with the
        // expected content.
        for oracle_path in walk(&oracle_root) {
            if oracle_path.extension().and_then(|e| e.to_str()) == Some("md") {
                continue; // README.md — excluded on both sides
            }
            let rel = oracle_path
                .strip_prefix(&oracle_root)
                .unwrap()
                .to_str()
                .expect("oracle path is UTF-8");
            let mapped_rel = rebrand(rel);
            let vendored_path = vendored_root.join(&mapped_rel);
            mapped_set.push(vendored_path.clone());

            let Ok(vendored) = std::fs::read_to_string(&vendored_path) else {
                failures.push(format!("MISSING vendored file: {mapped_rel} (from oracle {rel})"));
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
                    vendored_path.strip_prefix(&vendored_root).unwrap().display()
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
