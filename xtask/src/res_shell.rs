//! Shell-completion resource generation for the macOS Copy-Resources phase.
//!
//! PORTS: the completion branch of ghostty's `src/build/GhosttyResources.zig`
//! (the `+bash` / `+zsh` / `+fish` `build_data_exe` runs + their `install_subdir`
//! placements). ghostty shells out to its `build-data` exe; rustty calls the
//! already-ported pure byte generators in `rustty-extra` directly.
//!
//! Layout matches ghostty's `zig build` install tree (verified against
//! `GhosttyResources.zig`), with the OUTPUT filenames routed through
//! [`crate::APP`] (Task 14 — `rustty` by default, `ghostty` under
//! `identity-oracle`):
//!   - bash: `share/bash-completion/completions/{APP}.bash`
//!   - zsh:  `share/zsh/site-functions/_{APP}`
//!   - fish: `share/fish/vendor_completions.d/{APP}.fish`
//!
//! R2 identity note (Task 16 correction): an earlier version of this note
//! claimed "the reused macOS Swift app invokes the binary as `ghostty`
//! (its `Ghostty.app` bundle hard-codes `ghostty` / `xterm-ghostty`), so the
//! completion script CONTENT must complete the `ghostty` command" — this is
//! the SAME obsolete "reused macOS app hardcodes ghostty" rationale that
//! Tasks 14/18 found false and corrected for the theme-dir and vim/editor
//! resources; it is false here too (verified: `EXECUTABLE_NAME = rustty` in
//! `Rustty.xcodeproj`, no Swift source hardcodes `"ghostty"`). The shipped
//! binary is `rustty`, so `rustty-extra`'s bash/zsh/fish generators now emit
//! completions that register for the `rustty` command (`complete -c rustty`,
//! `_rustty()`, `#compdef rustty`) — this was a real leak (tab-completion for
//! the actual `rustty` binary silently did not work). Everything else in the
//! generated scripts (flags, field names, enum values, ordering, wording)
//! stays byte-exact against ghostty's own generated output (R1); only the
//! command/function-name brand token changed. Task 14's OUTPUT FILENAME
//! routing (below) is unaffected.
//!
//! R5: all three shell generators are ported and emit non-empty scripts, so no
//! DEFERRALS entry is needed for this resource.

use std::path::Path;

/// Write `share/{bash-completion,zsh,fish}` completion trees under `share_dir`.
///
/// `share_dir` is the `share/` directory (e.g. `zig-out/share`). Each shell's
/// script is produced by the corresponding `rustty-extra` generator (content
/// rebranded to complete the `rustty` command — see the module R2 note) and
/// written to a [`crate::APP`]-named path.
pub fn gen_shell(share_dir: &Path) -> Result<(), String> {
    let app = crate::APP;

    // bash -> share/bash-completion/completions/{APP}.bash
    write_completion(
        share_dir,
        "bash-completion/completions",
        &format!("{app}.bash"),
        rustty_extra::bash::completions(),
    )?;

    // zsh -> share/zsh/site-functions/_{APP}
    write_completion(
        share_dir,
        "zsh/site-functions",
        &format!("_{app}"),
        rustty_extra::zsh::completions(),
    )?;

    // fish -> share/fish/vendor_completions.d/{APP}.fish
    write_completion(
        share_dir,
        "fish/vendor_completions.d",
        &format!("{app}.fish"),
        rustty_extra::fish::completions(),
    )?;

    Ok(())
}

/// Create `share_dir/<subdir>` and write `<file>` with `bytes`, failing if the
/// generator produced an empty script (an empty completion file is never valid
/// output for a ported generator — see R5).
fn write_completion(
    share_dir: &Path,
    subdir: &str,
    file: &str,
    bytes: Vec<u8>,
) -> Result<(), String> {
    if bytes.is_empty() {
        return Err(format!(
            "shell completion generator produced empty output for {subdir}/{file}"
        ));
    }
    let dir = share_dir.join(subdir);
    std::fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let path = dir.join(file);
    std::fs::write(&path, &bytes).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(())
}
