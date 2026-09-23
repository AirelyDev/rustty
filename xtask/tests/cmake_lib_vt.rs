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

/// S64-F5: the install layout of `GhosttyLibVt.zig:384-490` and Zig's shared
/// install. The shared library is installed under a fully versioned real name,
/// with a major-version link and an unversioned link. On Apple its install
/// name is `@rpath/lib<name>.dylib`, with `-compatibility_version 1.0.0` and
/// `-current_version X.Y.Z`. The static archive is `lib<name>.a`, and the
/// `.pc` links `-l<name>`. Builds and installs into a temporary prefix.
#[test]
fn install_layout_is_versioned_like_the_reference() {
    let Some(dir) = configure("install") else {
        return;
    };
    let prefix = dir.join("prefix");
    for args in [
        vec!["--build".into(), dir.display().to_string()],
        vec![
            "--install".into(),
            dir.display().to_string(),
            "--prefix".into(),
            prefix.display().to_string(),
        ],
    ] {
        let out = Command::new("cmake").args(&args).output().unwrap();
        assert!(
            out.status.success(),
            "cmake {args:?} failed:\n{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let lib = prefix.join("lib");
    let v = env!("CARGO_PKG_VERSION");
    let major = v.split('.').next().unwrap();
    let (real, so, link) = if cfg!(target_os = "macos") {
        (
            format!("librustty-vt.{v}.dylib"),
            format!("librustty-vt.{major}.dylib"),
            "librustty-vt.dylib".to_string(),
        )
    } else {
        (
            format!("librustty-vt.so.{v}"),
            format!("librustty-vt.so.{major}"),
            "librustty-vt.so".to_string(),
        )
    };
    let meta = std::fs::symlink_metadata(lib.join(&real)).expect("real name installed");
    assert!(meta.file_type().is_file());
    assert_eq!(
        std::fs::read_link(lib.join(&so)).unwrap(),
        Path::new(&real)
    );
    assert_eq!(std::fs::read_link(lib.join(&link)).unwrap(), Path::new(&so));
    assert!(lib.join("librustty-vt.a").is_file());
    assert!(!lib.join("librustty_vt_c.a").exists());

    if cfg!(target_os = "macos") {
        let out = Command::new("otool")
            .arg("-L")
            .arg(lib.join(&real))
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        // The first entry after the path line is the library's own id.
        let id = text.lines().nth(1).unwrap_or_default().trim().to_string();
        assert!(
            id.starts_with("@rpath/librustty-vt.dylib (compatibility version 1.0.0, current version")
                && id.contains(&format!("current version {v}")),
            "{text}"
        );
    }

    let pc = std::fs::read_to_string(lib.join("pkgconfig/librustty-vt.pc"))
        .or_else(|_| std::fs::read_to_string(prefix.join("share/pkgconfig/librustty-vt.pc")))
        .unwrap();
    assert_eq!(field(&pc, "Libs:"), "-L${libdir} -lrustty-vt");
    let pcs = std::fs::read_to_string(lib.join("pkgconfig/librustty-vt-static.pc"))
        .or_else(|_| {
            std::fs::read_to_string(prefix.join("share/pkgconfig/librustty-vt-static.pc"))
        })
        .unwrap();
    assert_eq!(field(&pcs, "Libs:"), "${libdir}/librustty-vt.a");
    let _ = std::fs::remove_dir_all(&dir);
}
