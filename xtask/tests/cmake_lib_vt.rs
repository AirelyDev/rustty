//! The CMake wrapper's generated pkg-config files and install layout
//! (`CMakeLists.txt`, the port of `GhosttyLibVt.zig`'s install and
//! `pkgConfigFiles()`).
//!
//! Each test configures the workspace root's `CMakeLists.txt` into a fresh
//! build directory; configuring does not run cargo, so it is quick. Needs
//! `cmake` on `PATH`; without it a test reports that it did not run rather
//! than passing silently.

use std::path::{Path, PathBuf};
use std::process::Command;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// Configure into a fresh directory, returning it, or `None` without cmake.
fn configure(tag: &str) -> Option<PathBuf> {
    if Command::new("cmake").arg("--version").output().is_err() {
        eprintln!("SKIPPED: cmake is not on PATH");
        return None;
    }
    let dir = std::env::temp_dir().join(format!(
        "xtask-cmake-lib-vt-{tag}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let out = Command::new("cmake")
        .arg("-S")
        .arg(workspace_root())
        .arg("-B")
        .arg(&dir)
        .arg("-DCMAKE_BUILD_TYPE=Release")
        .output()
        .expect("spawn cmake");
    assert!(
        out.status.success(),
        "cmake configure failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    Some(dir)
}

fn field<'a>(pc: &'a str, key: &str) -> &'a str {
    pc.lines()
        .find_map(|l| l.strip_prefix(key))
        .unwrap_or_else(|| panic!("no `{key}` line in:\n{pc}"))
        .trim()
}

/// S64-F6: `GhosttyLibVt.zig:493-499` gives the static `.pc` no
/// `Libs.private` beyond the toolchain's, and libghostty-vt never depends on
/// gettext. The port's static archive bundles its own vendored libintl
/// (`gettext-sys`, measured: `nm` shows `_dgettext`/`_bindtextdomain`/
/// `_textdomain` DEFINED in `librustty_vt_c.a`, and a static link with no
/// `-lintl` and no Homebrew path succeeds and runs), so the `.pc` must not
/// ask for an external libintl, and must not bake a machine-specific
/// absolute path into an installed file.
#[test]
fn static_pc_needs_no_libintl_and_no_absolute_paths() {
    let Some(dir) = configure("pc") else { return };
    let pc = std::fs::read_to_string(dir.join("librustty-vt-static.pc")).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    let private = field(&pc, "Libs.private:");
    // Positive: the field was generated (the toolchain runtime is listed).
    assert!(
        private.contains("-lc"),
        "Libs.private lost its toolchain entries: {private:?}"
    );
    assert!(!private.contains("intl"), "Libs.private: {private:?}");
    assert!(
        private.split_whitespace().all(|t| !t.starts_with('/')),
        "absolute path in Libs.private: {private:?}"
    );
}
