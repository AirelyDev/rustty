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
/// parent directories as needed, then marks the `shell-integration` entry
/// RESOLVED in the `share/{APP}/DEFERRALS.md` note (idempotent, additive —
/// it never clobbers other resolutions written to the same note).
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

    mark_resolved(share_dir)?;
    Ok(())
}

/// The marker line appended to `DEFERRALS.md` once shell-integration ships.
const RESOLVED_MARKER: &str = "## RESOLVED: shell-integration (P1.1)";

/// Append a "RESOLVED: shell-integration" section to `share/{APP}/DEFERRALS.md`
/// if not already present (idempotent). The base note (written by
/// `res_terminfo::gen_app_root`) lists shell-integration as DEFERRED; this
/// records that it is now shipped from the vendored tree. Additive so it does
/// not race/clobber other agents' resolutions in the same generated note.
fn mark_resolved(share_dir: &Path) -> Result<(), String> {
    let note_path = share_dir.join(crate::APP).join("DEFERRALS.md");
    let mut contents = std::fs::read_to_string(&note_path).unwrap_or_default();
    if contents.contains(RESOLVED_MARKER) {
        return Ok(());
    }
    if !contents.ends_with('\n') && !contents.is_empty() {
        contents.push('\n');
    }
    contents.push_str(&format!(
        "\n{RESOLVED_MARKER}\n\
\n\
`shell-integration/{{bash,zsh,fish,elvish,nushell}}/...` is now emitted from the\n\
vendored tree (`crates/rustty-extra/shell-integration/`, byte-identical to\n\
ghostty's `src/shell-integration/`, R1/R3) by\n\
`xtask` (`res_shell_integration::gen_shell_integration`), excluding `*.md`\n\
(matching ghostty's `exclude_extensions`). No longer deferred.\n"
    ));
    std::fs::write(&note_path, contents.as_bytes())
        .map_err(|e| format!("write {}: {e}", note_path.display()))?;
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
}
