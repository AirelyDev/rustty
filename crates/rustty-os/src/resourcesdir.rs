//! rustty-os::resourcesdir
//!
//! PORT STATUS: ported
//!
//! Ports the reference's `src/os/resourcesdir.zig` — every line, comment, and
//! `test {}` is ported here (R1). Retargeted only for product identity (R2).

// PORT NOTE: the Zig reference imports `std`, `builtin`, `Allocator =
// std.mem.Allocator`, and `global = @import("../global.zig")`. In Rust we use
// the standard library and `rustty_global`; allocation failure is not a
// recoverable error here, so the public `Result` carries `ResourcesDirError`.
// Product-identity strings (the `GHOSTTY_RESOURCES_DIR` env var, the
// `xterm-ghostty`/`ghostty` terminfo sentinels, and the trailing `ghostty`
// resources subdir) are retargeted to the `rustty` identity via
// `rustty_identity` (R2).
//
// PORT NOTE (S112-F1, S112-F3): the paths are raw bytes (`?[]const u8`), as in
// the reference. They were `String`s: the resources-dir variable was read with
// `std::env::var`, so a value that is not UTF-8 became an error, and the
// executable's directories went through `Path::to_str(..).unwrap_or("")`, so a
// directory that is not UTF-8 was probed at the filesystem root.
#![allow(non_snake_case)]

#[derive(Default, Clone)]
pub struct ResourcesDir {
    /// Avoid accessing these directly, use the app() and host() methods instead.
    app_path: Option<Vec<u8>>,
    host_path: Option<Vec<u8>>,
}

impl ResourcesDir {
    /// Build a `ResourcesDir` from its two paths.
    ///
    /// PORT NOTE (WS20): the Zig reference constructs `internal_os.ResourcesDir`
    /// by struct literal from OUTSIDE this file — `src/apprt/gtk/flatpak.zig:8-10`
    /// does `.{ .app_path = ... }` and then assigns `result.host_path`. Zig's
    /// fields are public, Rust's are deliberately not ("Avoid accessing these
    /// directly"), so that out-of-module construction needs this constructor.
    /// It is the only way to express `flatpak.zig`'s literal without widening
    /// the field visibility.
    #[must_use]
    pub fn from_paths(app_path: Option<Vec<u8>>, host_path: Option<Vec<u8>>) -> Self {
        Self {
            app_path,
            host_path,
        }
    }

    /// Free resources held. Requires the same allocator as when resourcesDir()
    /// is called.
    //
    // PORT NOTE: the reference frees `app_path`/`host_path` with the allocator; in
    // Rust the owned buffers are dropped here. Kept as a method for parity with
    // the reference's public surface.
    pub fn deinit(self) {
        // (the owned paths are dropped here)
    }

    /// Get the directory to the bundled resources directory accessible
    /// by the application.
    pub fn app(&self) -> Option<&[u8]> {
        self.app_path.as_deref()
    }

    /// Get the directory to the bundled resources directory accessible
    /// by the host environment (i.e. for sandboxed applications). The
    /// returned directory might not be accessible from the application
    /// itself.
    ///
    /// In non-sandboxed environment, this should be the same as app().
    pub fn host(&self) -> Option<&[u8]> {
        self.host_path.as_deref().or(self.app_path.as_deref())
    }
}

/// PORT NOTE: the part of `resourcesDir`'s inferred error set that Rust can
/// return: `std.fmt.bufPrint`'s `error.NoSpaceLeft` from `maybeDir`.
/// `Allocator.Error` aborts in Rust, and the environment lookup cannot fail
/// except by the variable being missing (the captured environment is bytes;
/// `getAlloc`'s `InvalidWtf8` is a Windows-only error, and rustty's Windows
/// block is WTF-8 by construction).
//
// CORRECTION (S112-F1, S112-F2): this enum held only `EnvError(VarError)`, for
// a non-UTF-8 variable, which the reference does not treat as an error, and
// had no variant for the reference's `NoSpaceLeft`.
#[derive(Debug, PartialEq, Eq)]
pub enum ResourcesDirError {
    /// `std.fmt.bufPrint` in `maybeDir`: `base/sub/suffix` does not fit in the
    /// `max_path_bytes` buffer.
    NoSpaceLeft,
}

/// Gets the directory to the bundled resources directory, if it
/// exists (not all platforms or packages have it). The output is
/// owned by the caller.
///
/// This is highly Rustty-specific and can likely be generalized at
/// some point but we can cross that bridge if we ever need to.
pub fn resourcesDir() -> Result<ResourcesDir, ResourcesDirError> {
    // std.process.executablePath(global.io(), &exe_buf) catch return .{}
    let exe = std::env::current_exe().ok().map(os_bytes);
    resources_dir_from(&rustty_global::environ(), exe.as_deref())
}

/// The body of [`resourcesDir`], over an explicit environment block and
/// executable path (`None` is `executablePath` failing), so a test can supply
/// both. Production passes `global.environ()` and `std::env::current_exe()`.
fn resources_dir_from(
    environ: &rustty_global::environ::Environ,
    exe: Option<&[u8]>,
) -> Result<ResourcesDir, ResourcesDirError> {
    // Use the RUSTTY_RESOURCES_DIR environment variable in release builds.
    //
    // In debug builds we try using terminfo detection first instead, since
    // if debug Rustty is launched by an older version of Rustty, it
    // would inherit the old, stale resources of older Rustty instead of the
    // freshly built ones under zig-out/share/rustty.
    //
    // Note: we ALWAYS want to allocate here because the result is always
    // freed, do not try to use internal_os.getenv or posix getenv.
    if !cfg!(debug_assertions) {
        // const dir = global.environ().getAlloc(alloc, "GHOSTTY_RESOURCES_DIR") catch |err| switch (err) {
        //     error.EnvironmentVariableMissing => break :env,
        //     else => return err,
        // };
        if let Some(dir) = environ.get_alloc(resources_dir_env_var().as_bytes()) {
            if !dir.is_empty() {
                return Ok(ResourcesDir {
                    app_path: Some(dir),
                    ..Default::default()
                });
            }
        }
    }

    // This is the sentinel value we look for in the path to know
    // we've found the resources directory.
    let sentinels: &[&str] = sentinels();

    // Get the path to our running binary
    let Some(mut exe) = exe else {
        return Ok(ResourcesDir::default());
    };

    // We have an exe path! Climb the tree looking for the terminfo
    // bundle as we expect it.
    let mut dir_buf = vec![0u8; max_path_bytes()];
    while let Some(dir) = fs_path_dirname(exe) {
        exe = dir;

        // On MacOS, we look for the app bundle path.
        #[cfg(target_os = "macos")]
        {
            for sentinel in sentinels {
                if let Some(v) = maybeDir(
                    &mut dir_buf,
                    dir,
                    b"Contents/Resources",
                    sentinel.as_bytes(),
                )? {
                    return Ok(ResourcesDir {
                        app_path: Some(fs_path_join(&[&v, app_subdir().as_bytes()])),
                        ..Default::default()
                    });
                }
            }
        }

        // On all platforms (except BSD), we look for a /usr/share style path. This
        // is valid even on Mac since there is nothing that requires
        // Rustty to be in an app bundle.
        for sentinel in sentinels {
            if let Some(v) = maybeDir(
                &mut dir_buf,
                dir,
                if cfg!(target_os = "freebsd") {
                    b"local/share"
                } else {
                    b"share"
                },
                sentinel.as_bytes(),
            )? {
                return Ok(ResourcesDir {
                    app_path: Some(fs_path_join(&[&v, app_subdir().as_bytes()])),
                    ..Default::default()
                });
            }
        }
    }

    // If terminfo detection failed in debug builds (somehow),
    // fallback and use the provided resources dir.
    if cfg!(debug_assertions) {
        // if (global.environ().getAlloc(alloc, "GHOSTTY_RESOURCES_DIR")) |dir| {
        //     if (dir.len > 0) return .{ .app_path = dir };
        // } else |err| switch (err) {
        //     error.InvalidWtf8, error.EnvironmentVariableMissing => {},
        //     else => return err,
        // }
        if let Some(dir) = environ.get_alloc(resources_dir_env_var().as_bytes()) {
            if !dir.is_empty() {
                return Ok(ResourcesDir {
                    app_path: Some(dir),
                    ..Default::default()
                });
            }
        }
    }

    Ok(ResourcesDir::default())
}

/// Little helper to check if the "base/sub/suffix" directory exists and
/// if so return true. The "suffix" is just used as a way to verify a directory
/// seems roughly right.
///
/// "buf" must be large enough to fit base + sub + suffix. This is generally
/// max_path_bytes so its not a big deal.
//
// PORT NOTE: the reference returns a slice into `buf` (`buf[0..len]`) covering
// `base/sub`. We return an owned copy of `base/sub` (the bytes up to the
// suffix), which avoids the borrow-checker tangle of returning a slice of `buf`
// while `buf` is reused across loop iterations; behavior is identical.
pub fn maybeDir(
    buf: &mut [u8],
    base: &[u8],
    sub: &[u8],
    suffix: &[u8],
) -> Result<Option<Vec<u8>>, ResourcesDirError> {
    // const path = try std.fmt.bufPrint(buf, "{s}/{s}/{s}", .{ base, sub, suffix });
    //
    // PORT NOTE (S112-F2): `bufPrint` fails with `error.NoSpaceLeft` when the
    // path does not fit, and the `try` returns it from `resourcesDir`. This
    // used to format into a `String`, skip the copy when it did not fit, and
    // go on to probe the path anyway.
    let len = base.len() + 1 + sub.len() + 1 + suffix.len();
    if len > buf.len() {
        return Err(ResourcesDirError::NoSpaceLeft);
    }
    let path = &mut buf[..len];
    path[..base.len()].copy_from_slice(base);
    path[base.len()] = b'/';
    path[base.len() + 1..][..sub.len()].copy_from_slice(sub);
    path[base.len() + 1 + sub.len()] = b'/';
    path[base.len() + 1 + sub.len() + 1..].copy_from_slice(suffix);

    // if (std.Io.Dir.accessAbsolute(global.io(), path, .{})) {
    if os_path(path).exists() {
        let len = path.len() - suffix.len() - 1;
        return Ok(Some(buf[0..len].to_vec()));
    } else {
        // Folder doesn't exist. If a different error happens its okay
        // we just ignore it and move on.
    }

    Ok(None)
}

// ---------------------------------------------------------------------------
// PORT NOTE: support shims for the std/identity values the reference uses.
// ---------------------------------------------------------------------------

/// PORT NOTE: the reference reads the `GHOSTTY_RESOURCES_DIR` env var. Retargeted to
/// the published `rustty` identity (R2): `<ENV_PREFIX>_RESOURCES_DIR`.
fn resources_dir_env_var() -> String {
    format!("{}_RESOURCES_DIR", rustty_identity::ENV_PREFIX)
}

/// PORT NOTE: the reference's terminfo sentinels. Retargeted to the `rustty`
/// identity (R2): `xterm-ghostty` -> `TERMINFO_PRIMARY_NAME`, `ghostty` ->
/// `TERMINFO_PREFERRED_NAME`.
fn sentinels() -> &'static [&'static str] {
    // PORT NOTE: the reference computes these as a comptime tuple per target. We use a
    // `OnceLock` because the identity strings are runtime constants here.
    use std::sync::OnceLock;
    static CELL: OnceLock<Vec<&'static str>> = OnceLock::new();
    CELL.get_or_init(|| {
        // Leak the small, process-lifetime sentinel strings so they are `'static`.
        let primary = rustty_identity::TERMINFO_PRIMARY_NAME; // xterm-rustty
        let preferred = rustty_identity::TERMINFO_PREFERRED_NAME; // rustty
                                                                  // (macOS uses only `primary`; other targets use both.)
        let _ = (primary, preferred);
        let leak = |s: String| -> &'static str { Box::leak(s.into_boxed_str()) };
        #[cfg(target_os = "windows")]
        {
            vec![leak(format!("terminfo/{}.terminfo", preferred))]
        }
        #[cfg(target_os = "macos")]
        {
            // macOS hashes the first byte of the entry name as a 2-hex-digit dir.
            vec![leak(format!(
                "terminfo/{:02x}/{}",
                primary.as_bytes()[0],
                primary
            ))]
        }
        #[cfg(target_os = "freebsd")]
        {
            vec![
                leak(format!("site-terminfo/{}/{}", &preferred[0..1], preferred)),
                leak(format!("site-terminfo/{}/{}", &primary[0..1], primary)),
            ]
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "freebsd")))]
        {
            vec![
                leak(format!("terminfo/{}/{}", &preferred[0..1], preferred)),
                leak(format!("terminfo/{}/{}", &primary[0..1], primary)),
            ]
        }
    })
    .as_slice()
}

/// PORT NOTE: the trailing resources subdir the reference joins (`"ghostty"`).
/// Retargeted to the published app name (R2).
fn app_subdir() -> &'static str {
    rustty_identity::APP_NAME
}

/// `std.fs.max_path_bytes` (`PATH_MAX`).
fn max_path_bytes() -> usize {
    #[cfg(not(windows))]
    {
        libc::PATH_MAX as usize
    }
    #[cfg(windows)]
    {
        32767
    }
}

/// The raw bytes of an OS path: the bytes themselves on POSIX, the WTF-8
/// encoding on Windows (`OsStr::as_encoded_bytes`, lossless).
fn os_bytes(p: std::path::PathBuf) -> Vec<u8> {
    p.into_os_string().into_encoded_bytes()
}

/// The inverse of [`os_bytes`], for the probe in `maybeDir`.
fn os_path(bytes: &[u8]) -> std::path::PathBuf {
    std::path::PathBuf::from(rustty_global::environ::os_string_from_bytes(bytes))
}

/// `std.fs.path.dirname`: `dirnamePosix` on every target but Windows.
#[cfg(not(windows))]
fn fs_path_dirname(path: &[u8]) -> Option<&[u8]> {
    crate::path::dirname_posix(path)
}

/// `std.fs.path.dirname` on Windows is `dirnameWindows` (drive letters, UNC
/// roots, both separators); the port uses `Path::parent` over the WTF-8
/// bytes, which agrees for the absolute executable paths this walk sees.
#[cfg(windows)]
fn fs_path_dirname(path: &[u8]) -> Option<&[u8]> {
    let parent = std::path::Path::new(unsafe {
        // SAFETY: `path` is `exe` or a prefix `Path::parent` returned, all
        // split on separator boundaries of `as_encoded_bytes` output.
        std::ffi::OsStr::from_encoded_bytes_unchecked(path)
    })
    .parent()?;
    let bytes = parent.as_os_str().as_encoded_bytes();
    if bytes.is_empty() {
        return None;
    }
    Some(&path[..bytes.len()])
}

/// PORT NOTE: reproduces `std.fs.path.join` for the two-component joins used here
/// (a single separator between non-empty parts).
fn fs_path_join(paths: &[&[u8]]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    let mut prev_nonempty = false;
    for p in paths {
        if p.is_empty() {
            continue;
        }
        if prev_nonempty {
            let need_sep = !out.ends_with(b"/") && !p.starts_with(b"/");
            if need_sep {
                out.push(b'/');
            } else if out.ends_with(b"/") && p.starts_with(b"/") {
                out.pop();
            }
        }
        out.extend_from_slice(p);
        prev_nonempty = true;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustty_global::environ::Environ;

    fn env_with(value: &[u8]) -> Environ {
        let mut entry = format!("{}=", resources_dir_env_var()).into_bytes();
        entry.extend_from_slice(value);
        Environ::from_entries(vec![entry])
    }

    /// A fresh directory under the system temp dir, removed on drop.
    struct TempTree(std::path::PathBuf);
    impl TempTree {
        fn new(tag: &str) -> Self {
            let p = std::env::temp_dir().join(format!(
                "rustty-tg-resourcesdir-{tag}-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).expect("create temp tree");
            TempTree(p)
        }
        fn bytes(&self) -> Vec<u8> {
            os_bytes(self.0.clone())
        }
    }
    impl Drop for TempTree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// S112-F1: the variable's value is used as raw bytes. A value that is not
    /// UTF-8 is returned as the app path (the reference's `getAlloc` does not
    /// validate), not turned into an error. The executable path has no
    /// resources tree above it, so in a debug build the value comes from the
    /// fallback after the walk, and in a release build from the lookup before
    /// it; both are the reference's paths.
    #[test]
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (rustty-os, parity tail C-1): `stat` is forbidden under Miri isolation; runs natively in cargo test --workspace (see MIRI_EXCEPTIONS.md)"
    )]
    fn non_utf8_resources_dir_variable_is_used_raw() {
        let env = env_with(b"/opt/r\xffs/share/rustty");
        let dir = resources_dir_from(&env, Some(b"/nonexistent-tg-f1/bin/rustty")).unwrap();
        assert_eq!(dir.app(), Some(&b"/opt/r\xffs/share/rustty"[..]));
        assert_eq!(dir.host(), dir.app());

        // An empty value is "not set" in both branches (`dir.len > 0`).
        let env = env_with(b"");
        let dir = resources_dir_from(&env, Some(b"/nonexistent-tg-f1/bin/rustty")).unwrap();
        assert_eq!(dir.app(), None);
    }

    /// `executablePath` failing returns `.{}` before the walk and, in a debug
    /// build, before the fallback lookup too.
    #[test]
    fn no_executable_path_returns_empty_in_debug() {
        let env = env_with(b"/opt/rustty");
        let dir = resources_dir_from(&env, None).unwrap();
        if cfg!(debug_assertions) {
            assert_eq!(dir.app(), None);
        } else {
            assert_eq!(dir.app(), Some(&b"/opt/rustty"[..]));
        }
    }

    /// S112-F2: a candidate `base/sub/suffix` longer than the `max_path_bytes`
    /// buffer is `bufPrint`'s `error.NoSpaceLeft`, returned from `resourcesDir`.
    #[test]
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (rustty-os, parity tail C-1): `stat` is forbidden under Miri isolation; runs natively in cargo test --workspace (see MIRI_EXCEPTIONS.md)"
    )]
    fn candidate_longer_than_max_path_is_no_space_left() {
        let mut exe = b"/".to_vec();
        exe.extend(std::iter::repeat_n(b'd', max_path_bytes()));
        exe.extend_from_slice(b"/rustty");
        let env = Environ::from_entries(Vec::new());
        assert_eq!(
            resources_dir_from(&env, Some(&exe)).err(),
            Some(ResourcesDirError::NoSpaceLeft)
        );

        let mut buf = vec![0u8; 8];
        assert_eq!(
            maybeDir(&mut buf, b"/ab", b"cde", b"e").err(),
            Some(ResourcesDirError::NoSpaceLeft),
            "9 bytes do not fit in 8"
        );
        assert_eq!(maybeDir(&mut buf, b"/ab", b"cd", b"e"), Ok(None), "8 fit");
    }

    /// S112-F3: the walk climbs the executable path's bytes with
    /// `std.fs.path.dirname` and finds `<dir>/share/<sentinel>`, returning
    /// `<dir>/share/rustty`.
    #[test]
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (rustty-os, parity tail C-1): `lstat` is forbidden under Miri isolation; runs natively in cargo test --workspace (see MIRI_EXCEPTIONS.md)"
    )]
    fn walk_finds_the_share_tree_above_the_executable() {
        let tree = TempTree::new("walk");
        let sentinel = sentinels()[0];
        std::fs::create_dir_all(tree.0.join("app/share").join(sentinel)).unwrap();
        let mut exe = tree.bytes();
        exe.extend_from_slice(b"/app/bin/deeper/rustty");

        let env = Environ::from_entries(Vec::new());
        let dir = resources_dir_from(&env, Some(&exe)).unwrap();
        let mut want = tree.bytes();
        want.extend_from_slice(b"/app/share/");
        want.extend_from_slice(app_subdir().as_bytes());
        assert_eq!(dir.app(), Some(&want[..]));
    }

    /// S112-F3 on a filesystem that stores non-UTF-8 names (Linux; APFS
    /// refuses them with EILSEQ): the found path keeps the raw byte.
    #[cfg(target_os = "linux")]
    #[test]
    fn walk_keeps_a_non_utf8_directory() {
        use std::os::unix::ffi::OsStrExt;
        let tree = TempTree::new("nonutf8");
        let odd = tree.0.join(std::ffi::OsStr::from_bytes(b"r\xffs"));
        let sentinel = sentinels()[0];
        std::fs::create_dir_all(odd.join("share").join(sentinel)).unwrap();
        let mut exe = os_bytes(odd.clone());
        exe.extend_from_slice(b"/bin/rustty");

        let env = Environ::from_entries(Vec::new());
        let dir = resources_dir_from(&env, Some(&exe)).unwrap();
        let mut want = os_bytes(odd);
        want.extend_from_slice(b"/share/");
        want.extend_from_slice(app_subdir().as_bytes());
        assert_eq!(dir.app(), Some(&want[..]));
    }
}
