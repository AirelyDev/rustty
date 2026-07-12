//! Shell-completion resource generation for the macOS Copy-Resources phase.
//!
//! PORTS: the completion branch of ghostty's `src/build/GhosttyResources.zig`
//! (the `+bash` / `+zsh` / `+fish` `build_data_exe` runs + their `install_subdir`
//! placements). ghostty shells out to its `build-data` exe; rustty calls the
//! already-ported pure byte generators in `rustty-extra` directly.
//!
//! Layout matches ghostty's `zig build` install tree exactly (verified against
//! `GhosttyResources.zig`):
//!   - bash: `share/bash-completion/completions/ghostty.bash`
//!   - zsh:  `share/zsh/site-functions/_ghostty`
//!   - fish: `share/fish/vendor_completions.d/ghostty.fish`
//!
//! R2 identity note: the reused macOS Swift app invokes the binary as `ghostty`
//! (its `Ghostty.app` bundle hard-codes `ghostty` / `xterm-ghostty`), so the
//! completion scripts must complete the `ghostty` command — which is exactly what
//! `rustty-extra`'s generators emit (`complete -c ghostty`, `_ghostty()`,
//! `#compdef ghostty`). No brand rename is applied here.
//!
//! R5: all three shell generators are ported and emit non-empty scripts, so no
//! DEFERRALS entry is needed for this resource.

use std::path::Path;

/// Write `share/{bash-completion,zsh,fish}` completion trees under `share_dir`.
///
/// `share_dir` is the `share/` directory (e.g. `zig-out/share`). Each shell's
/// script is produced by the corresponding `rustty-extra` generator and written
/// to the ghostty-matching path.
pub fn gen_shell(share_dir: &Path) -> Result<(), String> {
    // bash -> share/bash-completion/completions/ghostty.bash
    write_completion(
        share_dir,
        "bash-completion/completions",
        "ghostty.bash",
        rustty_extra::bash::completions(),
    )?;

    // zsh -> share/zsh/site-functions/_ghostty
    write_completion(
        share_dir,
        "zsh/site-functions",
        "_ghostty",
        rustty_extra::zsh::completions(),
    )?;

    // fish -> share/fish/vendor_completions.d/ghostty.fish
    write_completion(
        share_dir,
        "fish/vendor_completions.d",
        "ghostty.fish",
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
        return Err(format!("shell completion generator produced empty output for {subdir}/{file}"));
    }
    let dir = share_dir.join(subdir);
    std::fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let path = dir.join(file);
    std::fs::write(&path, &bytes).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(())
}
