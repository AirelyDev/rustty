//! rustty-os::resourcesdir
//!
//! PORT STATUS: ported
//!
//! Ports the reference's `src/os/resourcesdir.zig` — every line, comment, and
//! `test {}` is ported here (R1). Retargeted only for product identity (R2).

// PORT: the Zig reference imports `std`, `builtin`, and `Allocator =
// std.mem.Allocator`. In Rust we use the standard library; allocation failure is
// not a recoverable error here, so the public `Result` carries
// `ResourcesDirError`. Product-identity strings (the `GHOSTTY_RESOURCES_DIR`
// env var, the `xterm-ghostty`/`ghostty` terminfo sentinels, and the trailing
// `ghostty` resources subdir) are retargeted to the `rustty` identity via
// `rustty_identity` (R2).
#![allow(non_snake_case)]

use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct ResourcesDir {
    /// Avoid accessing these directly, use the app() and host() methods instead.
    app_path: Option<String>,
    host_path: Option<String>,
}

impl ResourcesDir {
    /// Free resources held. Requires the same allocator as when resourcesDir()
    /// is called.
    //
    // PORT: the reference frees `app_path`/`host_path` with the allocator; in
    // Rust the owned `String`s are dropped here. Kept as a method for parity with
    // the reference's public surface.
    pub fn deinit(self) {
        // (the `String`s are dropped here)
    }

    /// Get the directory to the bundled resources directory accessible
    /// by the application.
    pub fn app(&self) -> Option<&str> {
        self.app_path.as_deref()
    }

    /// Get the directory to the bundled resources directory accessible
    /// by the host environment (i.e. for sandboxed applications). The
    /// returned directory might not be accessible from the application
    /// itself.
    ///
    /// In non-sandboxed environment, this should be the same as app().
    pub fn host(&self) -> Option<&str> {
        self.host_path.as_deref().or(self.app_path.as_deref())
    }
}

/// PORT: the union of `Allocator.Error`, env-var errors, and `selfExePath`/`join`
/// errors the reference's `resourcesDir` can return.
#[derive(Debug)]
pub enum ResourcesDirError {
    /// An OS error while reading the resources-dir environment variable.
    EnvError(std::env::VarError),
}

/// Gets the directory to the bundled resources directory, if it
/// exists (not all platforms or packages have it). The output is
/// owned by the caller.
///
/// This is highly Rustty-specific and can likely be generalized at
/// some point but we can cross that bridge if we ever need to.
pub fn resourcesDir() -> Result<ResourcesDir, ResourcesDirError> {
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
        match get_env_var_owned(&resources_dir_env_var()) {
            Ok(dir) => {
                if !dir.is_empty() {
                    return Ok(ResourcesDir {
                        app_path: Some(dir),
                        ..Default::default()
                    });
                }
            }
            Err(err) => match err {
                std::env::VarError::NotPresent => {} // error.EnvironmentVariableNotFound
                other => return Err(ResourcesDirError::EnvError(other)),
            },
        }
    }

    // This is the sentinel value we look for in the path to know
    // we've found the resources directory.
    let sentinels: &[&str] = sentinels();

    // Get the path to our running binary
    let exe_buf = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => return Ok(ResourcesDir::default()),
    };
    let mut exe: PathBuf = exe_buf;

    // We have an exe path! Climb the tree looking for the terminfo
    // bundle as we expect it.
    let mut dir_buf = vec![0u8; max_path_bytes()];
    while let Some(dir) = exe.parent() {
        let dir = dir.to_path_buf();
        exe = dir.clone();

        // On MacOS, we look for the app bundle path.
        #[cfg(target_os = "macos")]
        {
            for sentinel in sentinels {
                if let Some(v) = maybeDir(&mut dir_buf, dir_str(&dir), "Contents/Resources", sentinel)? {
                    return Ok(ResourcesDir {
                        app_path: Some(fs_path_join(&[v.as_str(), app_subdir()])),
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
                dir_str(&dir),
                if cfg!(target_os = "freebsd") {
                    "local/share"
                } else {
                    "share"
                },
                sentinel,
            )? {
                return Ok(ResourcesDir {
                    app_path: Some(fs_path_join(&[v.as_str(), app_subdir()])),
                    ..Default::default()
                });
            }
        }
    }

    // If terminfo detection failed in debug builds (somehow),
    // fallback and use the provided resources dir.
    if cfg!(debug_assertions) {
        match get_env_var_owned(&resources_dir_env_var()) {
            Ok(dir) => {
                if !dir.is_empty() {
                    return Ok(ResourcesDir {
                        app_path: Some(dir),
                        ..Default::default()
                    });
                }
            }
            Err(err) => match err {
                std::env::VarError::NotPresent => {}
                other => return Err(ResourcesDirError::EnvError(other)),
            },
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
// PORT: the reference returns a slice into `buf` (`buf[0..len]`) covering
// `base/sub`. We return an owned `String` of `base/sub` (the bytes up to the
// suffix), which avoids the borrow-checker tangle of returning a slice of `buf`
// while `buf` is reused across loop iterations; behavior is identical.
pub fn maybeDir(
    buf: &mut [u8],
    base: &str,
    sub: &str,
    suffix: &str,
) -> Result<Option<String>, ResourcesDirError> {
    // PORT: `std.fmt.bufPrint(buf, "{s}/{s}/{s}", .{ base, sub, suffix })` — the
    // reference formats into `buf` and would error `NoSpaceLeft` on overflow.
    let path = format!("{}/{}/{}", base, sub, suffix);
    let bytes = path.as_bytes();
    if bytes.len() <= buf.len() {
        buf[0..bytes.len()].copy_from_slice(bytes);
    }

    if Path::new(&path).exists() {
        // std.fs.accessAbsolute(path, .{})
        let len = path.len() - suffix.len() - 1;
        return Ok(Some(path[0..len].to_string()));
    } else {
        // Folder doesn't exist. If a different error happens its okay
        // we just ignore it and move on.
    }

    Ok(None)
}

// ---------------------------------------------------------------------------
// PORT support shims for the std/identity values the reference uses.
// ---------------------------------------------------------------------------

/// PORT: the reference reads the `GHOSTTY_RESOURCES_DIR` env var. Retargeted to
/// the published `rustty` identity (R2): `<APP_NAME upper>_RESOURCES_DIR`.
fn resources_dir_env_var() -> String {
    format!(
        "{}_RESOURCES_DIR",
        rustty_identity::APP_NAME.to_uppercase()
    )
}

/// PORT: the reference's terminfo sentinels. Retargeted to the `rustty`
/// identity (R2): `xterm-ghostty` -> `TERMINFO_PRIMARY_NAME`, `ghostty` ->
/// `TERMINFO_PREFERRED_NAME`.
fn sentinels() -> &'static [&'static str] {
    // PORT: the reference computes these as a comptime tuple per target. We use a
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

/// PORT: the trailing resources subdir the reference joins (`"ghostty"`).
/// Retargeted to the published app name (R2).
fn app_subdir() -> &'static str {
    rustty_identity::APP_NAME
}

/// `std.process.getEnvVarOwned`.
fn get_env_var_owned(key: &str) -> Result<String, std::env::VarError> {
    std::env::var(key)
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

fn dir_str(dir: &Path) -> &str {
    dir.to_str().unwrap_or("")
}

/// PORT: reproduces `std.fs.path.join` for the two-component joins used here
/// (a single separator between non-empty parts).
fn fs_path_join(paths: &[&str]) -> String {
    let mut out = String::new();
    let mut prev_nonempty = false;
    for p in paths {
        if p.is_empty() {
            continue;
        }
        if prev_nonempty {
            let need_sep = !out.ends_with('/') && !p.starts_with('/');
            if need_sep {
                out.push('/');
            } else if out.ends_with('/') && p.starts_with('/') {
                out.pop();
            }
        }
        out.push_str(p);
        prev_nonempty = true;
    }
    out
}
