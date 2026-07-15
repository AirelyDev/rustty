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
//! R2 IDENTITY NOTE: canonical `TERM` is `xterm-rustty`, but the reused macOS
//! Swift app hard-codes the `ghostty` identity, so these editor files keep the
//! upstream `ghostty` naming — the `ghostty.vim` filenames, the `ghostty`
//! filetype/compiler names, and the `keywordprg=ghostty +explain-config` /
//! `makeprg=ghostty +validate-config` invocations — matching what the app ships.
//! `rustty_extra::vim` is the `identity-oracle` byte producer for this subtree;
//! no `xterm-rustty` variant of the editor files exists (nor is one needed: the
//! app's config filetype is `ghostty`).
//!
//! R5: the generators ARE ported, so this writes real content — no placeholder,
//! no DEFERRALS entry.

use std::fs;
use std::path::Path;

/// The four generated Vim files, as `(subdir, bytes)` — each installs as
/// `<subdir>/ghostty.vim`. Both the Vim and the Neovim install roots receive
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

/// Write one copy of the four `ghostty.vim` files under `root/<subdir>/ghostty.vim`.
fn write_plugin_tree(root: &Path) -> Result<(), String> {
    for (subdir, bytes) in vim_files() {
        let dir = root.join(subdir);
        fs::create_dir_all(&dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;
        let file = dir.join("ghostty.vim");
        fs::write(&file, &bytes).map_err(|e| format!("write {}: {e}", file.display()))?;
    }
    Ok(())
}

/// Generate the editor-syntax resource subtree under `share_dir`
/// (`share_dir` is the `share/` root that Xcode's Copy-Resources phase reads):
///
/// ```text
/// share/vim/vimfiles/{syntax,ftdetect,ftplugin,compiler}/ghostty.vim
/// share/nvim/site/{syntax,ftdetect,ftplugin,compiler}/ghostty.vim
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
                let f = root.join(sub).join("ghostty.vim");
                assert!(f.exists(), "missing {}", f.display());
                assert!(!fs::read(&f).unwrap().is_empty(), "empty {}", f.display());
            }
        }

        // Nvim mirrors Vim byte-for-byte (reference reuses the same write-files tree).
        for sub in ["syntax", "ftdetect", "ftplugin", "compiler"] {
            let v = fs::read(share.join("vim/vimfiles").join(sub).join("ghostty.vim")).unwrap();
            let n = fs::read(share.join("nvim/site").join(sub).join("ghostty.vim")).unwrap();
            assert_eq!(v, n, "vim/nvim differ for {sub}");
        }
        let _ = fs::remove_dir_all(&tmp);
    }
}
