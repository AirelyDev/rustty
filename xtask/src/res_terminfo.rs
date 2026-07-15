//! Terminfo + `share/{APP}` resource generation for the macOS Copy-Resources
//! phase (runtime-critical resource). `{APP}` is [`crate::APP`]: `rustty` by
//! default, `ghostty` under this xtask's own `identity-oracle` feature.
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
//! `identity-oracle` parity spelling (names `xterm-ghostty | ghostty | Ghostty`,
//! selected by the `rustty-terminfo/identity-oracle` feature this crate enables).
//! When the app is rebranded to set `TERM=xterm-rustty`, drop that feature and the
//! same pipeline emits the canonical entry with zero other changes.
//!
//! R5: the terminfo generator is fully ported, so terminfo carries no deferral.
//! In addition to the compiled DB, this module now also emits the terminfo
//! *source* (`share/terminfo/ghostty.terminfo`) and a `termcap` translation
//! (`share/terminfo/ghostty.termcap`) alongside it — the `emit_terminfo` /
//! `emit_termcap` installs of ghostty's `GhosttyResources.zig` (P1.3,
//! INTEGRATION_PARITY §2.8). ghostty defaults those FALSE for Release and TRUE
//! for Debug; rustty emits them unconditionally so `infocmp`/Debug tooling has
//! byte-identical inputs regardless of profile. The termcap is produced by the
//! system `/usr/bin/infotocap` reading that same source (exactly ghostty's
//! `RunStep "infotocap" <source>`), whose stderr — capability-translation
//! warnings + the 1023-byte-truncation notices termcap inherently requires — is
//! captured and discarded, matching ghostty's `captureStdErr()`.
//!
//! R2 identity note (source/termcap filenames, Task 14): unlike the compiled
//! entry's `xterm-ghostty` primary name (pinned — see above, forced
//! unconditionally by `rustty-terminfo/identity-oracle` for the reused macOS
//! Swift app), the `{APP}.terminfo`/`{APP}.termcap` DUMP basenames are just a
//! human/debug-readable side artifact: `tic`/`infocmp` resolve entries by the
//! compiled DB content, never by this dump file's name. So these basenames
//! follow [`crate::APP`] like every other output filename: `rustty.terminfo`/
//! `rustty.termcap` by default, `ghostty.terminfo`/`ghostty.termcap` under
//! `identity-oracle`.
//!
//! The `share/{APP}/` tree (shell-integration scripts, themes) is NOT yet ported
//! as rustty data — ghostty ships those as static files under
//! `src/shell-integration/` with no generator, and rustty has not vendored them.
//! Per R5 we therefore create a minimal VALID `share/{APP}/` folder with a
//! self-documenting `DEFERRALS.md` note (never faked script content); see
//! [`gen_app_root`].

use std::path::Path;
use std::process::Command;

/// Compile the terminfo entry into `share/terminfo/…` and create the
/// `share/{APP}/` resource root.
///
/// `share_dir` is the `share/` directory (e.g. `zig-out/share`), matching the
/// `gen_*` convention of the sibling resource modules.
///
/// Pipeline (mirrors ghostty's `GhosttyResources.zig` terminfo install):
///   1. Build the `Source` via `rustty_terminfo::entry::rustty()` (identity-oracle
///      spelling — see the R2 note above) and `encode` it to tic source text.
///   2. Write that text to a temporary `.terminfo` file.
///   3. Run `/usr/bin/tic -x -o <share>/terminfo <tmp>` to compile it, producing
///      `share/terminfo/<dir>/xterm-ghostty` (+ the `ghostty` / `Ghostty` alias
///      entries — pinned regardless of [`crate::APP`], see the R2 note above).
///      macOS's case-insensitive filesystem makes `tic` use hashed
///      two-hex-digit subdirectories (e.g. `78/xterm-ghostty`) rather than the
///      single-letter `x/…` layout used on Linux; both are valid terminfo trees.
///   4. Create the `share/{APP}/` root (see [`gen_app_root`]).
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

    // 3b. Emit the terminfo SOURCE + termcap alongside the compiled DB
    //     (P1.3 / INTEGRATION_PARITY §2.8; ghostty GhosttyResources.zig
    //     `emit_terminfo` + `emit_termcap` installs).
    emit_source_and_termcap(&terminfo_dir, &src_bytes)?;

    // 4. The share/{APP} resource root.
    gen_app_root(share_dir)?;

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

/// Emit the terminfo *source* (`{APP}.terminfo`) and its `termcap` translation
/// (`{APP}.termcap`) into `terminfo_dir`, mirroring ghostty's
/// `GhosttyResources.zig` `emit_terminfo`/`emit_termcap` installs (P1.3,
/// INTEGRATION_PARITY §2.8).
///
/// * `{APP}.terminfo` — the exact tic source bytes we compiled above (the
///   captured stdout of ghostty's `+terminfo` build-data run).
/// * `{APP}.termcap` — `/usr/bin/infotocap <source>` (ghostty's
///   `RunStep "infotocap"`). termcap is a strictly lossier legacy format bounded
///   to 1023 bytes/entry, so `infotocap` prints translation + truncation notices
///   to stderr; we capture and DISCARD them (ghostty `captureStdErr()`), failing
///   only on a non-zero exit or empty stdout.
fn emit_source_and_termcap(terminfo_dir: &Path, src_bytes: &[u8]) -> Result<(), String> {
    let src_path = terminfo_dir.join(format!("{}.terminfo", crate::APP));
    std::fs::write(&src_path, src_bytes)
        .map_err(|e| format!("write {}: {e}", src_path.display()))?;

    let output = Command::new("/usr/bin/infotocap")
        .arg(&src_path)
        .output()
        .map_err(|e| format!("spawn /usr/bin/infotocap: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "infotocap exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    if output.stdout.is_empty() {
        return Err("infotocap produced empty termcap output".to_string());
    }

    let cap_path = terminfo_dir.join(format!("{}.termcap", crate::APP));
    std::fs::write(&cap_path, &output.stdout)
        .map_err(|e| format!("write {}: {e}", cap_path.display()))?;
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

/// Create the `share/{APP}/` resource root the macOS Copy-Resources phase
/// references as `../zig-out/share/{APP}` (`{APP}` = [`crate::APP`]: `rustty`
/// by default, `ghostty` under `identity-oracle` — Task 14; this dir must
/// match the runtime lookup, which resolves the same way via
/// `rustty_os::resourcesdir`'s `app_subdir()` -> `rustty_identity::APP_NAME`).
///
/// ghostty populates this with `shell-integration/` (static bash/zsh/fish/elvish/
/// nushell scripts) and `themes/`. Neither is ported as rustty data yet (they are
/// static files under ghostty's `src/shell-integration/` with no generator), so
/// per R5 we create the folder as a minimal VALID structure carrying a
/// `DEFERRALS.md` note instead of fabricating script content. An empty-but-present
/// folder is acceptable for the MVP Copy-Resources gate; the note documents what
/// belongs here and how to un-defer it.
fn gen_app_root(share_dir: &Path) -> Result<(), String> {
    let app_dir = share_dir.join(crate::APP);
    std::fs::create_dir_all(&app_dir)
        .map_err(|e| format!("create {}: {e}", app_dir.display()))?;

    let app = crate::APP;
    let note = format!(
        "# share/{app} ({app} resource root)\n\
\n\
This folder is copied into the macOS app bundle by the Xcode Copy-Resources\n\
phase (referenced as `../zig-out/share/{app}`). It is generated by\n\
`xtask res-terminfo` (fn `gen_terminfo` -> `gen_app_root`).\n\
\n\
## DEFERRED contents (R5)\n\
\n\
ghostty populates the equivalent `share/ghostty/` with:\n\
  - `shell-integration/{{bash,zsh,fish,elvish,nushell}}/...` - static integration\n\
    scripts (ghostty `src/shell-integration/`, NO generator).\n\
  - `themes/...` - the bundled color themes.\n\
\n\
Neither is ported as rustty data yet, so this folder is intentionally minimal\n\
(empty but present) for the MVP Copy-Resources gate. No script content is faked\n\
(R5). To un-defer: vendor ghostty's `src/shell-integration/` tree as rustty data\n\
and emit it here alongside the compiled terminfo.\n"
    );

    let note_path = app_dir.join("DEFERRALS.md");
    std::fs::write(&note_path, note.as_bytes())
        .map_err(|e| format!("write {}: {e}", note_path.display()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// End-to-end: gen_terminfo compiles a real terminfo tree with tic and creates
    /// the share/{APP} root. Verifies the runtime-critical resource actually
    /// materializes (not just that the pipeline returns Ok).
    #[test]
    fn gen_terminfo_produces_compiled_entry() {
        let base = std::env::temp_dir().join(format!("rustty-xtask-tinfo-test-{}", std::process::id()));
        let share = base.join("share");
        std::fs::create_dir_all(&share).unwrap();

        gen_terminfo(&share).expect("gen_terminfo");

        // Compiled terminfo entry exists (pinned `xterm-ghostty`, regardless of
        // `crate::APP` — see the module R2 identity note).
        assert!(
            compiled_entry_exists(&share.join("terminfo")),
            "no compiled xterm-ghostty entry under {}",
            share.join("terminfo").display()
        );
        // Terminfo SOURCE + termcap emitted alongside the compiled DB (P1.3),
        // named after `crate::APP` (Task 14).
        let src = share.join(format!("terminfo/{}.terminfo", crate::APP));
        let cap = share.join(format!("terminfo/{}.termcap", crate::APP));
        assert!(src.is_file(), "no {}.terminfo source at {}", crate::APP, src.display());
        assert!(cap.is_file(), "no {}.termcap at {}", crate::APP, cap.display());
        assert!(
            std::fs::metadata(&src).unwrap().len() > 0,
            "{}.terminfo source is empty", crate::APP
        );
        assert!(
            std::fs::metadata(&cap).unwrap().len() > 0,
            "{}.termcap is empty", crate::APP
        );

        // share/{APP} root + note exist.
        assert!(share.join(crate::APP).is_dir());
        assert!(share.join(crate::APP).join("DEFERRALS.md").is_file());

        let _ = std::fs::remove_dir_all(&base);
    }
}
