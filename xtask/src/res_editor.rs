//! `res_editor` — emit the Vim/Neovim editor-syntax resource subtree into
//! `share/{vim,nvim}` for the macOS Xcode Copy-Resources phase.
//!
//! Faithful port of ghostty's `src/build/GhosttyResources.zig` "Vim and Neovim
//! plugin" block: it runs the `build_data` generators (`+vim-syntax`,
//! `+vim-ftdetect`, `+vim-ftplugin`, `+vim-compiler`) into a single write-files
//! tree — `syntax/ghostty.vim`, `ftdetect/ghostty.vim`, `ftplugin/ghostty.vim`,
//! `compiler/ghostty.vim` — then installs that *same* tree TWICE:
//!
//!   * `share/vim/vimfiles`  (Vim runtimepath)
//!   * `share/nvim/site`     (Neovim runtimepath)
//!
//! Here the four generators are the pure byte producers already ported in
//! `rustty_extra::vim` (`syntax()` reflects over `Config`'s field names;
//! `FTDETECT`/`FTPLUGIN`/`COMPILER` are the static byte strings). Nvim reuses the
//! byte-identical Vim output, mirrored faithfully (a second copy of the same four
//! files), exactly as the reference's two `addInstallDirectory` steps do.
//!
//! R2 IDENTITY NOTE (Task 18): these editor files now carry the rustty
//! product identity — `rustty.vim` filenames, the `rustty` filetype/compiler
//! names, and the `keywordprg=rustty +explain-config` /
//! `makeprg=rustty +validate-config` invocations — matching `config.rustty`
//! (`rustty_identity::CONFIG_FILE`). This obsoletes the prior "reused macOS
//! Swift app hard-codes the `ghostty` identity" rationale: that app no longer
//! hardcodes ghostty post-rebrand, so shipping `ghostty.vim`/`ghostty`
//! filetype detection here was a real leak (editor syntax-highlighting of
//! `config.rustty` silently failed to activate). `rustty_extra::vim` is the
//! byte producer for this subtree (single rustty-identity profile; no
//! `identity-oracle` dual-profile exists for this content, matching
//! `res_shell_integration.rs`'s precedent for content with no oracle variant).
//!
//! R5: the generators ARE ported, so this writes real content — no placeholder,
//! no DEFERRALS entry.

use std::fs;
use std::path::Path;

/// The four generated Vim files, as `(subdir, bytes)` — each installs as
/// `<subdir>/rustty.vim`. Both the Vim and the Neovim install roots receive
/// this identical set (reference: one `addWriteFiles` tree, two
/// `addInstallDirectory` steps).
fn vim_files() -> [(&'static str, Vec<u8>); 4] {
    [
        ("syntax", rustty_extra::vim::syntax()),
        ("ftdetect", rustty_extra::vim::FTDETECT.to_vec()),
        ("ftplugin", rustty_extra::vim::FTPLUGIN.to_vec()),
        ("compiler", rustty_extra::vim::COMPILER.to_vec()),
    ]
}

/// Write one copy of the four `rustty.vim` files under `root/<subdir>/rustty.vim`.
fn write_plugin_tree(root: &Path) -> Result<(), String> {
    for (subdir, bytes) in vim_files() {
        let dir = root.join(subdir);
        fs::create_dir_all(&dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;
        let file = dir.join("rustty.vim");
        fs::write(&file, &bytes).map_err(|e| format!("write {}: {e}", file.display()))?;
    }
    Ok(())
}

/// Generate the editor-syntax resource subtree under `share_dir`
/// (`share_dir` is the `share/` root that Xcode's Copy-Resources phase reads):
///
/// ```text
/// share/vim/vimfiles/{syntax,ftdetect,ftplugin,compiler}/rustty.vim
/// share/nvim/site/{syntax,ftdetect,ftplugin,compiler}/rustty.vim
/// ```
///
/// Both trees hold the byte-identical generated files (nvim reuses the vim
/// output, matching upstream).
pub fn gen_editor(share_dir: &Path) -> Result<(), String> {
    // Vim runtimepath: share/vim/vimfiles/…
    write_plugin_tree(&share_dir.join("vim").join("vimfiles"))?;
    // Neovim runtimepath: share/nvim/site/… (byte-identical mirror).
    write_plugin_tree(&share_dir.join("nvim").join("site"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_both_vim_and_nvim_trees() {
        let tmp = std::env::temp_dir().join(format!("rustty-res-editor-{}", std::process::id()));
        let share = tmp.join("share");
        let _ = fs::remove_dir_all(&tmp);
        gen_editor(&share).expect("gen_editor");

        for root in [
            share.join("vim").join("vimfiles"),
            share.join("nvim").join("site"),
        ] {
            for sub in ["syntax", "ftdetect", "ftplugin", "compiler"] {
                let f = root.join(sub).join("rustty.vim");
                assert!(f.exists(), "missing {}", f.display());
                assert!(!fs::read(&f).unwrap().is_empty(), "empty {}", f.display());
            }
        }

        // Nvim mirrors Vim byte-for-byte (reference reuses the same write-files tree).
        for sub in ["syntax", "ftdetect", "ftplugin", "compiler"] {
            let v = fs::read(share.join("vim/vimfiles").join(sub).join("rustty.vim")).unwrap();
            let n = fs::read(share.join("nvim/site").join(sub).join("rustty.vim")).unwrap();
            assert_eq!(v, n, "vim/nvim differ for {sub}");
        }
        let _ = fs::remove_dir_all(&tmp);
    }
}
