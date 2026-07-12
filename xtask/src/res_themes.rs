//! `res_themes` — emit the ghostty color-theme corpus into
//! `share/ghostty/themes/` for the macOS Xcode Copy-Resources phase.
//!
//! Faithful port of ghostty's `src/build/GhosttyResources.zig` "Themes" block:
//!
//! ```zig
//! if (cfg.emit_themes) {
//!     if (b.lazyDependency("iterm2_themes", .{})) |upstream| {
//!         const install_step = b.addInstallDirectory(.{
//!             .source_dir = upstream.path(""),
//!             .install_dir = .{ .custom = "share" },
//!             .install_subdir = b.pathJoin(&.{ "ghostty", "themes" }),
//!             .exclude_extensions = &.{".md"},
//!         });
//!         ...
//!     }
//! }
//! ```
//!
//! Ghostty pulls the iTerm2-Color-Schemes-derived theme `.conf` files from its
//! lazy `iterm2_themes` build dependency (a pinned upstream tarball) and installs
//! that directory under `share/ghostty/themes`, excluding `.md` files. R3
//! self-contained: rather than fetch that tarball at build time, rustty vendors
//! the corpus **verbatim** into `crates/rustty-extra/themes/` (byte-identical to
//! the upstream set — see that dir's `README.md` for provenance). This module
//! copies that vendored tree into `share/ghostty/themes/`, applying the SAME
//! `.md` exclusion ghostty uses — which skips the vendored `README.md` and keeps
//! the emitted count exactly equal to ghostty's shipped set (534 themes).
//!
//! R2 IDENTITY NOTE: the product is `rustty`, but this corpus keeps ghostty's
//! DATA identity — it installs to `share/ghostty/themes/` (not `share/rustty/…`)
//! because the reused macOS Swift app and the ported `list_themes` fuzzy-picker /
//! `theme =` config setting resolve theme names against that hard-coded
//! `share/ghostty/themes` path. Renaming the directory would break both, so the
//! ghostty name is retained deliberately.
//!
//! R1/R5: the files are real upstream data copied byte-for-byte — no placeholder,
//! no synthesized content, no DEFERRALS entry.

use std::fs;
use std::path::Path;

/// The vendored corpus directory, relative to this crate's manifest
/// (`xtask/`): `../crates/rustty-extra/themes/`.
const VENDORED_THEMES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../crates/rustty-extra/themes"
);

/// Generate the theme resource subtree under `share_dir`:
///
/// ```text
/// share/ghostty/themes/<Theme Name>   (534 iTerm2-derived .conf theme files)
/// ```
///
/// Copies every vendored theme file verbatim, excluding `.md` files (mirroring
/// ghostty's `exclude_extensions = &.{".md"}` — this drops the vendored
/// `README.md` so the emitted set is byte-identical to ghostty's shipped corpus).
pub fn gen_themes(share_dir: &Path) -> Result<(), String> {
    let src = Path::new(VENDORED_THEMES);
    let dest = share_dir.join("ghostty").join("themes");
    fs::create_dir_all(&dest).map_err(|e| format!("mkdir {}: {e}", dest.display()))?;

    let entries =
        fs::read_dir(src).map_err(|e| format!("read_dir {}: {e}", src.display()))?;
    let mut copied = 0usize;
    for entry in entries {
        let entry = entry.map_err(|e| format!("dir entry in {}: {e}", src.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|e| format!("file_type {}: {e}", path.display()))?;
        // Corpus is a flat directory of regular files; skip any non-file.
        if !file_type.is_file() {
            continue;
        }
        // Mirror ghostty's `.md` exclusion (drops the vendor README.md).
        if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("md")) {
            continue;
        }
        let name = entry.file_name();
        let out = dest.join(&name);
        fs::copy(&path, &out)
            .map_err(|e| format!("copy {} -> {}: {e}", path.display(), out.display()))?;
        copied += 1;
    }

    if copied == 0 {
        return Err(format!(
            "no theme files copied from {} (vendored corpus missing?)",
            src.display()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The vendored corpus is exactly ghostty's shipped set: 534 theme files
    /// (upstream tarball minus `.md`), and the emitter reproduces that count.
    #[test]
    fn emits_full_theme_corpus() {
        let tmp = std::env::temp_dir().join(format!("rustty-res-themes-{}", std::process::id()));
        let share = tmp.join("share");
        let _ = fs::remove_dir_all(&tmp);
        gen_themes(&share).expect("gen_themes");

        let themes = share.join("ghostty").join("themes");
        let count = fs::read_dir(&themes)
            .expect("read emitted themes")
            .filter(|e| e.as_ref().unwrap().path().is_file())
            .count();
        assert_eq!(count, 534, "emitted theme count must match ghostty's shipped set");

        // No `.md` leaked through (README.md is excluded, like ghostty does).
        for e in fs::read_dir(&themes).unwrap() {
            let p = e.unwrap().path();
            assert_ne!(
                p.extension().and_then(|x| x.to_str()),
                Some("md"),
                "{} should have been excluded",
                p.display()
            );
        }

        // Spot-check a known theme's content is real (not a placeholder).
        let sample = fs::read_to_string(themes.join("0x96f")).expect("0x96f theme present");
        assert!(sample.contains("palette = 0="), "theme body should be real conf data");

        let _ = fs::remove_dir_all(&tmp);
    }
}
