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
//! R2 identity note (Task 18): `xterm-rustty` is rustty's CANONICAL primary
//! terminfo name (`rustty-identity::TERMINFO_PRIMARY_NAME`, the value of `TERM`
//! a native rustty build sets), and it is what `rustty_terminfo::entry::rustty()`
//! builds by DEFAULT. This module used to force the `identity-oracle` parity
//! spelling (`xterm-ghostty | ghostty | Ghostty`) on UNCONDITIONALLY via this
//! crate's `rustty-terminfo` dependency, on the obsolete rationale that the
//! reused macOS Swift app hard-coded `TERM=xterm-ghostty`. That app no longer
//! hardcodes ghostty post-rebrand, so the force was a real bug: the shipped
//! default-profile terminfo had ghostty content, and `TERM=xterm-rustty`
//! couldn't resolve. The default profile now compiles the canonical
//! `xterm-rustty | rustty | Rustty` entry; the oracle spelling remains
//! available for parity checks via this xtask crate's OWN `identity-oracle`
//! feature (forwarded to `rustty-terminfo/identity-oracle` — see
//! `xtask/Cargo.toml`).
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
//! R2 identity note (source/termcap filenames, Task 14/18): the compiled
//! entry's primary name now follows the SAME profile as everything else
//! (`xterm-rustty` by default, `xterm-ghostty` under `identity-oracle` — see
//! above), and so do the `{APP}.terminfo`/`{APP}.termcap` DUMP basenames: these
//! are just a human/debug-readable side artifact (`tic`/`infocmp` resolve
//! entries by the compiled DB content, never by this dump file's name), and
//! follow [`crate::APP`] like every other output filename: `rustty.terminfo`/
//! `rustty.termcap` by default, `ghostty.terminfo`/`ghostty.termcap` under
//! `identity-oracle`.
//!
//! The `share/{APP}/` tree's contents (shell-integration scripts, themes) are
//! emitted by the sibling generators `res_shell_integration` / `res_themes`
//! from the vendored trees under `crates/rustty-extra/` (WS1 — the pre-WS1
//! `DEFERRALS.md` placeholder note is gone); [`gen_app_root`] only guarantees
//! the root folder exists and scrubs any stale note from earlier builds.

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
///      `share/terminfo/<dir>/xterm-rustty` by default (`xterm-ghostty` under
///      `identity-oracle`; + the `rustty`/`Rustty` or `ghostty`/`Ghostty` alias
///      entries — same profile as [`crate::APP`], see the R2 note above).
///      macOS's case-insensitive filesystem makes `tic` use hashed
///      two-hex-digit subdirectories (e.g. `78/xterm-rustty`) rather than the
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
    // The expected primary name follows the active profile — `xterm-rustty` by
    // default, `xterm-ghostty` under `identity-oracle` — read directly off the
    // `Source` we just compiled rather than a hardcoded literal (Task 18: this
    // used to hardcode `xterm-ghostty` unconditionally, which broke once the
    // default profile stopped being forced to emit ghostty content).
    let primary_name = String::from_utf8_lossy(&source.names[0]).into_owned();
    if !compiled_entry_exists(&terminfo_dir, &primary_name) {
        return Err(format!(
            "tic reported success but no compiled {primary_name} entry found under {}",
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

/// True if tic produced a compiled entry named `primary_name` anywhere in the
/// terminfo tree (`xterm-rustty` by default, `xterm-ghostty` under
/// `identity-oracle` — Task 18). tic uses either single-letter (`x/xterm-…`) or
/// hashed (`78/xterm-…`) subdirectories depending on the host filesystem, so we
/// scan the immediate subdirectories rather than assuming one layout.
fn compiled_entry_exists(terminfo_dir: &Path, primary_name: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(terminfo_dir) else {
        return false;
    };
    for entry in entries.flatten() {
        let sub = entry.path();
        if sub.is_dir() && sub.join(primary_name).exists() {
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
/// The subtrees under it — `shell-integration/` and `themes/` — are populated
/// by the sibling generators (`res_shell_integration::gen_shell_integration`,
/// `res_themes::gen_themes`) from the REAL vendored trees in
/// `crates/rustty-extra/` (WS1); this fn only guarantees the root exists.
/// It also SCRUBS the pre-WS1 `DEFERRALS.md` placeholder note from stale
/// output dirs so it can never ship in the app bundle again.
fn gen_app_root(share_dir: &Path) -> Result<(), String> {
    let app_dir = share_dir.join(crate::APP);
    std::fs::create_dir_all(&app_dir)
        .map_err(|e| format!("create {}: {e}", app_dir.display()))?;

    let stale_note = app_dir.join("DEFERRALS.md");
    match std::fs::remove_file(&stale_note) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("remove stale {}: {e}", stale_note.display())),
    }
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

        // Compiled terminfo entry exists under the active profile's primary
        // name (`xterm-rustty` by default, `xterm-ghostty` under
        // `identity-oracle` — see the module R2 identity note).
        let primary_name = rustty_terminfo::entry::rustty().names[0].clone();
        let primary_name = String::from_utf8(primary_name).unwrap();
        assert!(
            compiled_entry_exists(&share.join("terminfo"), &primary_name),
            "no compiled {primary_name} entry under {}",
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

        // share/{APP} root exists; the pre-WS1 DEFERRALS.md placeholder is GONE
        // (shell-integration + themes ship real vendored content now).
        assert!(share.join(crate::APP).is_dir());
        assert!(!share.join(crate::APP).join("DEFERRALS.md").exists());

        let _ = std::fs::remove_dir_all(&base);
    }

    /// Regenerating over a stale output dir SCRUBS the pre-WS1 placeholder note
    /// (so it can never ship in the app bundle again).
    #[test]
    fn gen_app_root_scrubs_stale_deferrals_note() {
        let base = std::env::temp_dir()
            .join(format!("rustty-xtask-approot-test-{}", std::process::id()));
        let share = base.join("share");
        let app_dir = share.join(crate::APP);
        std::fs::create_dir_all(&app_dir).unwrap();
        std::fs::write(app_dir.join("DEFERRALS.md"), b"stale placeholder").unwrap();

        gen_app_root(&share).expect("gen_app_root");

        assert!(app_dir.is_dir());
        assert!(!app_dir.join("DEFERRALS.md").exists(), "stale note must be scrubbed");
        let _ = std::fs::remove_dir_all(&base);
    }
}
