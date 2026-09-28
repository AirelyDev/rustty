//! The CMake wrapper's generated pkg-config files and install layout
//! (`CMakeLists.txt`, the port of `GhosttyLibVt.zig`'s install and
//! `pkgConfigFiles()`).
//!
//! Each test configures the workspace root's `CMakeLists.txt` into a fresh
//! build directory; configuring does not run cargo, so it is quick. Needs
//! `cmake` on `PATH`; without it a test prints that it was skipped, and under
//! `RUSTTY_REQUIRE_CMAKE=1` (set by CI, whose runners ship cmake for
//! G-CCONSUMER) it fails instead (R14).

use std::path::{Path, PathBuf};
use std::process::Command;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// The R14 strict-mode pivot for `cmake`: a no-op by default, a panic under
/// `RUSTTY_REQUIRE_CMAKE=1`, so a CI leg that lost the tool fails instead of
/// reporting these tests `ok`.
#[track_caller]
fn require_cmake_or_skip(test_name: &str, err: &dyn std::fmt::Display) {
    require_cmake_or_skip_impl(
        std::env::var_os("RUSTTY_REQUIRE_CMAKE").as_deref() == Some("1".as_ref()),
        test_name,
        err,
    );
}

#[track_caller]
fn require_cmake_or_skip_impl(required: bool, test_name: &str, err: &dyn std::fmt::Display) {
    assert!(
        !required,
        "cmake required but unavailable (RUSTTY_REQUIRE_CMAKE=1): \
         {test_name} would have skipped: {err}"
    );
}

/// The pivot itself: a no-op when not required, a panic when required.
#[test]
fn cmake_pivot_panics_only_when_required() {
    require_cmake_or_skip_impl(false, "t", &"missing");
    let r = std::panic::catch_unwind(|| require_cmake_or_skip_impl(true, "t", &"missing"));
    assert!(r.is_err());
}

/// Configure into a fresh directory, returning it, or `None` without cmake
/// (after the strict-mode pivot).
fn configure(tag: &str) -> Option<PathBuf> {
    configure_with(tag, &[])
}

/// Serializes the tests that run `cmake --build` (see its users).
static CMAKE_BUILD: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn configure_with(tag: &str, extra: &[&str]) -> Option<PathBuf> {
    if let Err(e) = Command::new("cmake").arg("--version").output() {
        require_cmake_or_skip(tag, &e);
        eprintln!("skipping the `{tag}` cmake test: cmake is not on PATH ({e})");
        return None;
    }
    let dir = std::env::temp_dir().join(format!("xtask-cmake-lib-vt-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let out = Command::new("cmake")
        .arg("-S")
        .arg(workspace_root())
        .arg("-B")
        .arg(&dir)
        .arg("-DCMAKE_BUILD_TYPE=Release")
        .args(extra)
        .output()
        .expect("spawn cmake");
    assert!(
        out.status.success(),
        "cmake configure failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    Some(dir)
}

/// A `/`-rooted path, or a Windows drive path (`X:`).
fn is_absolute_token(t: &str) -> bool {
    let b = t.as_bytes();
    t.starts_with('/') || (b.len() > 1 && b[1] == b':' && b[0].is_ascii_alphabetic())
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
    // Positive: the field was generated (the toolchain runtime is listed). On
    // Windows the toolchain entries are the system import libraries Rust's std
    // links (`CMakeLists.txt`'s WIN32 arm), not `-lc` (task W-2).
    #[cfg(not(windows))]
    let toolchain = "-lc";
    #[cfg(windows)]
    let toolchain = "-lkernel32";
    assert!(
        private.contains(toolchain),
        "Libs.private lost its toolchain entries: {private:?}"
    );
    assert!(!private.contains("intl"), "Libs.private: {private:?}");
    // No absolute path: POSIX `/...`, and on Windows also a drive path (`X:`).
    assert!(
        private.split_whitespace().all(|t| !is_absolute_token(t)),
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
    // One cmake build at a time: they share the workspace `target/`, and the
    // lib-version test below builds it with a different version (N424).
    let _serial = CMAKE_BUILD.lock().unwrap_or_else(|e| e.into_inner());
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
    // Windows (task W-2): the layout W-1 measured for S64-F5 — the DLL in
    // `bin/` under cargo's name, its import library renamed to the `.pc`'s link
    // name, the static library as `rustty-vt-static.lib` (`GhosttyLibVt.zig`'s
    // `ghostty-vt-static.lib`, `build.zig:172-175`), and no versioned links.
    #[cfg(windows)]
    {
        let lib = prefix.join("lib");
        assert!(
            prefix.join("bin").join("rustty_vt_c.dll").is_file(),
            "the DLL is in bin/"
        );
        assert!(
            lib.join("rustty-vt.lib").is_file(),
            "the import library is rustty-vt.lib"
        );
        assert!(
            lib.join("rustty-vt-static.lib").is_file(),
            "the static library"
        );
        assert!(
            !lib.join("rustty_vt_c.dll.lib").exists(),
            "cargo's import-library name"
        );
        assert!(
            !lib.join("rustty_vt_c.lib").exists(),
            "cargo's static-library name"
        );
        let pc = std::fs::read_to_string(prefix.join("share/pkgconfig/librustty-vt.pc"))
            .or_else(|_| std::fs::read_to_string(lib.join("pkgconfig/librustty-vt.pc")))
            .unwrap();
        assert_eq!(field(&pc, "Libs:"), "-L${libdir} -lrustty-vt");
        let pcs = std::fs::read_to_string(prefix.join("share/pkgconfig/librustty-vt-static.pc"))
            .or_else(|_| std::fs::read_to_string(lib.join("pkgconfig/librustty-vt-static.pc")))
            .unwrap();
        assert_eq!(field(&pcs, "Libs:"), "${libdir}/rustty-vt-static.lib");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(not(windows))]
    {
        // GNUInstallDirs picks the library directory: `lib64` on Fedora-style
        // multilib hosts (measured on Fedora 44 aarch64), `lib` elsewhere.
        let lib = ["lib", "lib64"]
            .iter()
            .map(|d| prefix.join(d))
            .find(|d| d.join("librustty-vt.a").is_file())
            .unwrap_or_else(|| {
                panic!(
                    "no lib/ or lib64/ under {} holds librustty-vt.a",
                    prefix.display()
                )
            });
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
        assert_eq!(std::fs::read_link(lib.join(&so)).unwrap(), Path::new(&real));
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
                id.starts_with(
                    "@rpath/librustty-vt.dylib (compatibility version 1.0.0, current version"
                ) && id.contains(&format!("current version {v}")),
                "{text}"
            );
        }

        if cfg!(any(target_os = "linux", target_os = "freebsd")) {
            // The ELF soname Zig records for a versioned shared library
            // (`lib<name>.so.<major>`), which the soname link above resolves.
            let out = Command::new("readelf")
                .arg("-d")
                .arg(lib.join(&real))
                .output()
                .expect("run readelf");
            let text = String::from_utf8_lossy(&out.stdout);
            assert!(
                text.lines()
                    .any(|l| l.contains("(SONAME)") && l.contains(&format!("[{so}]"))),
                "SONAME must be {so}:\n{text}"
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
}

/// `-Dlib-version-string` (N424, `Config.zig:343-353`): the CMake option
/// versions everything the reference derives from `lib_version` — the
/// `.pc` `Version:` (the full string, `GhosttyLibVt.zig:532`) and the
/// versioned install names and dylib version (MAJOR.MINOR.PATCH,
/// `GhosttyLibVt.zig:396-436`). The library's own C-API version follows the
/// same value through `RUSTTY_LIB_VERSION_STRING` (asserted in rustty-vt's
/// `options` tests). Builds and installs into a temporary prefix.
#[cfg(not(windows))]
#[test]
fn lib_version_string_versions_the_library() {
    const V: &str = "2.3.4-rc.1+b5";
    let _serial = CMAKE_BUILD.lock().unwrap_or_else(|e| e.into_inner());
    let Some(dir) = configure_with("libversion", &[&format!("-DRUSTTY_LIB_VERSION_STRING={V}")])
    else {
        return;
    };
    let pc = std::fs::read_to_string(dir.join("librustty-vt.pc")).unwrap();
    assert_eq!(field(&pc, "Version:"), V);
    let pcs = std::fs::read_to_string(dir.join("librustty-vt-static.pc")).unwrap();
    assert_eq!(field(&pcs, "Version:"), V);

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
    let lib = ["lib", "lib64"]
        .iter()
        .map(|d| prefix.join(d))
        .find(|d| d.join("librustty-vt.a").is_file())
        .expect("an installed lib dir");
    let (real, so) = if cfg!(target_os = "macos") {
        ("librustty-vt.2.3.4.dylib", "librustty-vt.2.dylib")
    } else {
        ("librustty-vt.so.2.3.4", "librustty-vt.so.2")
    };
    assert!(lib.join(real).is_file(), "{real} installed");
    assert_eq!(std::fs::read_link(lib.join(so)).unwrap(), Path::new(real));
    if cfg!(target_os = "macos") {
        let out = Command::new("otool")
            .arg("-L")
            .arg(lib.join(real))
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        let id = text.lines().nth(1).unwrap_or_default().trim().to_string();
        assert!(id.contains("current version 2.3.4"), "{text}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// A non-semver `RUSTTY_LIB_VERSION_STRING` fails the configure, as
/// `std.SemanticVersion.parse` fails `zig build` (N424).
#[test]
fn lib_version_string_must_be_a_semantic_version() {
    if let Err(e) = Command::new("cmake").arg("--version").output() {
        require_cmake_or_skip("libversion-bad", &e);
        eprintln!("skipping the `libversion-bad` cmake test: cmake is not on PATH ({e})");
        return;
    }
    let dir = std::env::temp_dir().join(format!("xtask-cmake-lib-vt-bad-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for bad in ["2.3", "02.3.4", "2.3.4-", "v2.3.4"] {
        let out = Command::new("cmake")
            .arg("-S")
            .arg(workspace_root())
            .arg("-B")
            .arg(&dir)
            .arg(format!("-DRUSTTY_LIB_VERSION_STRING={bad}"))
            .output()
            .expect("spawn cmake");
        assert!(!out.status.success(), "{bad} was accepted");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("is not a semantic version"),
            "{bad}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// S64-F5, Windows: the `.pc` links `-lrustty-vt`, so the installed import
/// library must be the file that flag names, `rustty-vt.lib`, as Zig installs
/// the reference's `ghostty-vt.lib` for its `-lghostty-vt`
/// (`GhosttyLibVt.zig:227-228`, `:529`). The DLL keeps cargo's name (an import
/// library records its DLL's name), but the import library's own file name is
/// free. Read from `CMakeLists.txt`, because the WIN32 branch cannot be
/// configured on this host; the final link is unverified until a Windows run.
#[test]
fn windows_import_library_is_installed_under_the_pc_link_name() {
    let cmake = std::fs::read_to_string(workspace_root().join("CMakeLists.txt")).unwrap();
    let set_value = |var: &str| -> String {
        let key = format!("set({var} \"");
        let at = cmake
            .find(&key)
            .unwrap_or_else(|| panic!("no `{key}` in CMakeLists.txt"));
        let rest = &cmake[at + key.len()..];
        rest[..rest.find('"').unwrap()].to_string()
    };
    let link_name = set_value("RUSTTY_VT_PC_LINK_NAME");
    assert_eq!(link_name, "rustty-vt");
    assert_eq!(
        set_value("RUSTTY_VT_INSTALL_IMPLIB"),
        format!("{link_name}.lib")
    );
    assert!(
        cmake.contains(
            "install(FILES \"${RUSTTY_OUT_DIR}/${RUSTTY_VT_IMPLIB}\" TYPE LIB RENAME \"${RUSTTY_VT_INSTALL_IMPLIB}\")"
        ),
        "the WIN32 install must rename the import library"
    );
    let config =
        std::fs::read_to_string(workspace_root().join("dist/cmake/rustty-vt-config.cmake.in"))
            .unwrap();
    assert!(
        config.contains("IMPORTED_IMPLIB \"${_rustty_vt_libdir}/@RUSTTY_VT_INSTALL_IMPLIB@\""),
        "the installed package config must point at the installed import library"
    );
}
