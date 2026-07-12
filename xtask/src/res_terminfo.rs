//! Terminfo + `share/ghostty` resource generation for the macOS Copy-Resources
//! phase (runtime-critical resource).
//!
//! PORTS: the terminfo branch of ghostty's `src/build/GhosttyResources.zig` — the
//! `+terminfo` `build_data_exe` run that emits the terminfo *source* followed by
//! the `tic -x -o share/terminfo` compile step (ghostty runs `tic` in its
//! `install` recipe). rustty already ported the terminfo model (`rustty-terminfo`:
//! `entry::rustty()` builds the `Source`, `Source::encode` writes tic-compilable
//! text), so this module drives those directly and shells out to the system
//! `/usr/bin/tic`.
//!
//! R2 identity note: `xterm-rustty` is rustty's CANONICAL primary terminfo name
//! (`rustty-identity::TERMINFO_PRIMARY_NAME`, the value of `TERM` a native rustty
//! build sets). The macOS Copy-Resources gate, however, reuses the ghostty Swift
//! app unchanged — its `Ghostty.app` hard-codes `TERM=xterm-ghostty`. So the
//! runtime terminfo entry MUST be spelled `xterm-ghostty` for the reused app's
//! `TERM` to resolve at runtime; we therefore build the `Source` through the
//! `identity-ghostty` parity spelling (names `xterm-ghostty | ghostty | Ghostty`,
//! selected by the `rustty-terminfo/identity-ghostty` feature this crate enables).
//! When the app is rebranded to set `TERM=xterm-rustty`, drop that feature and the
//! same pipeline emits the canonical entry with zero other changes.
//!
//! R5: the terminfo generator is fully ported, so terminfo carries no deferral.
//! The `share/ghostty/` tree (shell-integration scripts, themes) is NOT yet ported
//! as rustty data — ghostty ships those as static files under
//! `src/shell-integration/` with no generator, and rustty has not vendored them.
//! Per R5 we therefore create a minimal VALID `share/ghostty/` folder with a
//! self-documenting `DEFERRALS.md` note (never faked script content); see
//! [`gen_ghostty_root`].

use std::path::Path;
use std::process::Command;

/// Compile the terminfo entry into `share/terminfo/…` and create the
/// `share/ghostty/` resource root.
///
/// `share_dir` is the `share/` directory (e.g. `zig-out/share`), matching the
/// `gen_*` convention of the sibling resource modules.
///
/// Pipeline (mirrors ghostty's `GhosttyResources.zig` terminfo install):
///   1. Build the `Source` via `rustty_terminfo::entry::rustty()` (identity-ghostty
///      spelling — see the R2 note above) and `encode` it to tic source text.
///   2. Write that text to a temporary `.terminfo` file.
///   3. Run `/usr/bin/tic -x -o <share>/terminfo <tmp>` to compile it, producing
///      `share/terminfo/<dir>/xterm-ghostty` (+ the `ghostty` / `Ghostty` alias
///      entries). macOS's case-insensitive filesystem makes `tic` use hashed
///      two-hex-digit subdirectories (e.g. `78/xterm-ghostty`) rather than the
///      single-letter `x/…` layout used on Linux; both are valid terminfo trees.
///   4. Create the `share/ghostty/` root (see [`gen_ghostty_root`]).
pub fn gen_terminfo(share_dir: &Path) -> Result<(), String> {
    // 1. Build + encode the terminfo source.
    let source = rustty_terminfo::entry::rustty();
    let mut src_bytes: Vec<u8> = Vec::new();
    source
        .encode(&mut src_bytes)
        .map_err(|e| format!("encode terminfo source: {e}"))?;
    if src_bytes.is_empty() {
        return Err("terminfo source encoded to empty output".to_string());
    }

    // 2. Write the source to a temp file for tic to consume. tic requires a real
    //    path argument; we place it under the system temp dir keyed by pid so
    //    concurrent xtask runs don't collide, and remove it afterwards.
    let tmp = std::env::temp_dir().join(format!("rustty-{}.terminfo", std::process::id()));
    std::fs::write(&tmp, &src_bytes)
        .map_err(|e| format!("write temp terminfo source {}: {e}", tmp.display()))?;

    // 3. Compile with tic into share/terminfo.
    let terminfo_dir = share_dir.join("terminfo");
    std::fs::create_dir_all(&terminfo_dir)
        .map_err(|e| format!("create {}: {e}", terminfo_dir.display()))?;

    let result = run_tic(&terminfo_dir, &tmp);
    // Best-effort cleanup of the temp source regardless of tic's outcome.
    let _ = std::fs::remove_file(&tmp);
    result?;

    // Verify tic actually produced the compiled entry (runtime-critical resource).
    if !compiled_entry_exists(&terminfo_dir) {
        return Err(format!(
            "tic reported success but no compiled xterm-ghostty entry found under {}",
            terminfo_dir.display()
        ));
    }

    // 4. The share/ghostty resource root.
    gen_ghostty_root(share_dir)?;

    Ok(())
}

/// Run `/usr/bin/tic -x -o <out_dir> <src>`. `-x` includes the user-defined
/// (extended) capabilities rustty's entry relies on (`Smulx`, `Setulc`, `Sync`,
/// …). tic emits capability warnings to stderr for entries newer than the host
/// ncurses; those are non-fatal. We only fail on a non-zero exit.
fn run_tic(out_dir: &Path, src: &Path) -> Result<(), String> {
    let output = Command::new("/usr/bin/tic")
        .arg("-x")
        .arg("-o")
        .arg(out_dir)
        .arg(src)
        .output()
        .map_err(|e| format!("spawn /usr/bin/tic: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "tic exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

/// True if tic produced a compiled `xterm-ghostty` entry anywhere in the terminfo
/// tree. tic uses either single-letter (`x/xterm-ghostty`) or hashed
/// (`78/xterm-ghostty`) subdirectories depending on the host filesystem, so we
/// scan the immediate subdirectories rather than assuming one layout.
fn compiled_entry_exists(terminfo_dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(terminfo_dir) else {
        return false;
    };
    for entry in entries.flatten() {
        let sub = entry.path();
        if sub.is_dir() && sub.join("xterm-ghostty").exists() {
            return true;
        }
    }
    false
}

/// Create the `share/ghostty/` resource root the macOS Copy-Resources phase
/// references as `../zig-out/share/ghostty`.
///
/// ghostty populates this with `shell-integration/` (static bash/zsh/fish/elvish/
/// nushell scripts) and `themes/`. Neither is ported as rustty data yet (they are
/// static files under ghostty's `src/shell-integration/` with no generator), so
/// per R5 we create the folder as a minimal VALID structure carrying a
/// `DEFERRALS.md` note instead of fabricating script content. An empty-but-present
/// folder is acceptable for the MVP Copy-Resources gate; the note documents what
/// belongs here and how to un-defer it.
fn gen_ghostty_root(share_dir: &Path) -> Result<(), String> {
    let ghostty_dir = share_dir.join("ghostty");
    std::fs::create_dir_all(&ghostty_dir)
        .map_err(|e| format!("create {}: {e}", ghostty_dir.display()))?;

    let note = b"# share/ghostty (rustty resource root)\n\
\n\
This folder is copied into the macOS app bundle by the Xcode Copy-Resources\n\
phase (referenced as `../zig-out/share/ghostty`). It is generated by\n\
`xtask res-terminfo` (fn `gen_terminfo` -> `gen_ghostty_root`).\n\
\n\
## DEFERRED contents (R5)\n\
\n\
ghostty populates `share/ghostty/` with:\n\
  - `shell-integration/{bash,zsh,fish,elvish,nushell}/...` - static integration\n\
    scripts (ghostty `src/shell-integration/`, NO generator).\n\
  - `themes/...` - the bundled color themes.\n\
\n\
Neither is ported as rustty data yet, so this folder is intentionally minimal\n\
(empty but present) for the MVP Copy-Resources gate. No script content is faked\n\
(R5). To un-defer: vendor ghostty's `src/shell-integration/` tree as rustty data\n\
(carrying the ghostty identity, matching the reused Swift app) and emit it here\n\
alongside the compiled terminfo.\n";

    let note_path = ghostty_dir.join("DEFERRALS.md");
    std::fs::write(&note_path, note.as_slice())
        .map_err(|e| format!("write {}: {e}", note_path.display()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// End-to-end: gen_terminfo compiles a real terminfo tree with tic and creates
    /// the share/ghostty root. Verifies the runtime-critical resource actually
    /// materializes (not just that the pipeline returns Ok).
    #[test]
    fn gen_terminfo_produces_compiled_entry() {
        let base = std::env::temp_dir().join(format!("rustty-xtask-tinfo-test-{}", std::process::id()));
        let share = base.join("share");
        std::fs::create_dir_all(&share).unwrap();

        gen_terminfo(&share).expect("gen_terminfo");

        // Compiled terminfo entry exists.
        assert!(
            compiled_entry_exists(&share.join("terminfo")),
            "no compiled xterm-ghostty entry under {}",
            share.join("terminfo").display()
        );
        // share/ghostty root + note exist.
        assert!(share.join("ghostty").is_dir());
        assert!(share.join("ghostty/DEFERRALS.md").is_file());

        let _ = std::fs::remove_dir_all(&base);
    }
}
