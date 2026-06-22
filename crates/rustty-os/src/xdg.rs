//! Implementation of the XDG Base Directory specification
//! (https://specifications.freedesktop.org/basedir-spec/basedir-spec-latest.html)
//!
//! rustty-os::xdg
//!
//! PORT STATUS: ported
//!
//! Ports the reference's `src/os/xdg.zig` — every line, comment, and `test {}`
//! is ported here (R1). Retargeted only for product identity (R2).

// PORT: the Zig reference imports `std`, `builtin`, `Allocator`, `posix`,
// `homedir = @import("homedir.zig")`, and `env_os = @import("env.zig")`. In Rust
// we use those ported sibling modules plus the standard library. Allocation in
// Rust is infallible for the collections used here, so the reference's
// `Allocator.Error` / `error.NoHomeDir` set collapses to the single
// `error.NoHomeDir` case kept in `DirError`.

use crate::env as env_os;
use crate::homedir;

/// Errors produced by the XDG directory helpers.
#[derive(Debug, PartialEq, Eq)]
pub enum DirError {
    NoHomeDir,
}

#[derive(Default)]
pub struct Options<'a> {
    /// Subdirectories to join to the base. This avoids extra allocations
    /// when building up the directory. This is commonly the application.
    pub subdir: Option<&'a str>,

    /// The home directory for the user. If this is not set, we will attempt
    /// to look it up which is an expensive process. By setting this, you can
    /// avoid lookups.
    pub home: Option<&'a str>,
}

/// Get the XDG user config directory. The returned value is allocated.
pub fn config(opts: Options) -> Result<String, DirError> {
    dir(
        opts,
        InternalOptions {
            env: "XDG_CONFIG_HOME",
            windows_env: "LOCALAPPDATA",
            default_subdir: ".config",
        },
    )
}

/// Get the XDG cache directory. The returned value is allocated.
pub fn cache(opts: Options) -> Result<String, DirError> {
    dir(
        opts,
        InternalOptions {
            env: "XDG_CACHE_HOME",
            windows_env: "LOCALAPPDATA",
            default_subdir: ".cache",
        },
    )
}

/// Get the XDG state directory. The returned value is allocated.
pub fn state(opts: Options) -> Result<String, DirError> {
    dir(
        opts,
        InternalOptions {
            env: "XDG_STATE_HOME",
            windows_env: "LOCALAPPDATA",
            default_subdir: ".local/state",
        },
    )
}

struct InternalOptions {
    env: &'static str,
    windows_env: &'static str,
    default_subdir: &'static str,
}

/// Unified helper to get XDG directories that follow a common pattern.
fn dir(opts: Options, internal_opts: InternalOptions) -> Result<String, DirError> {
    // If we have a cached home dir, use that.
    if let Some(home) = opts.home {
        return Ok(path_join(&[
            home,
            internal_opts.default_subdir,
            opts.subdir.unwrap_or(""),
        ]));
    }

    // First check the env var. On Windows we have to allocate so this tracks
    // both whether we have the env var and whether we own it.
    // on Windows we treat `LOCALAPPDATA` as a fallback for `XDG_CONFIG_HOME`
    let env_ = match env_os::getenvNotEmpty(internal_opts.env).unwrap() {
        Some(v) => Some(v),
        None => {
            if cfg!(windows) {
                env_os::getenvNotEmpty(internal_opts.windows_env).unwrap()
            } else {
                None
            }
        }
    };
    // PORT: `defer if (env_) |env| env.deinit(alloc);` — `GetEnvResult`'s deinit
    // is Drop in Rust, so `env_` is released automatically when this function
    // returns.

    if let Some(env) = &env_ {
        // If we have a subdir, then we use the env as-is to avoid a copy.
        if let Some(subdir) = opts.subdir {
            return Ok(path_join(&[&env.value, subdir]));
        }

        return Ok(env.value.clone());
    }

    // Get our home dir
    let mut buf = [0u8; 1024];
    if let Some(home) = homedir::home(&mut buf).map_err(|_| DirError::NoHomeDir)? {
        // PORT: `homedir.home` returns `[]const u8`; convert to `&str` for the
        // path-join shim (home paths are UTF-8 on the supported platforms).
        let home = std::str::from_utf8(home).map_err(|_| DirError::NoHomeDir)?;
        return Ok(path_join(&[
            home,
            internal_opts.default_subdir,
            opts.subdir.unwrap_or(""),
        ]));
    }

    Err(DirError::NoHomeDir)
}

/// Parses the xdg-terminal-exec specification. This expects argv[0] to
/// be "xdg-terminal-exec".
#[allow(non_snake_case)]
pub fn parseTerminalExec<'a>(argv: &'a [&'a str]) -> Option<&'a [&'a str]> {
    if path_basename(argv[0]) != "xdg-terminal-exec" {
        return None;
    }

    // We expect at least one argument
    if argv.len() < 2 {
        return Some(&[]);
    }

    // If the first argument is "-e" we skip it.
    let start: usize = if argv[1] == "-e" { 2 } else { 1 };
    Some(&argv[start..])
}

// ---------------------------------------------------------------------------
// PORT: support shims for `std.fs.path.{join,basename}` used above.
// ---------------------------------------------------------------------------

// `std.fs.path.sep` / `isSep`.
#[cfg(windows)]
const SEP: u8 = b'\\';
#[cfg(not(windows))]
const SEP: u8 = b'/';

#[cfg(windows)]
fn is_sep(c: u8) -> bool {
    c == b'\\' || c == b'/'
}
#[cfg(not(windows))]
fn is_sep(c: u8) -> bool {
    c == b'/'
}

// `std.fs.path.join`: join path components with the native separator,
// collapsing duplicated separators at the boundaries (faithful reproduction of
// Zig's `joinSepMaybeZ`).
fn path_join(paths: &[&str]) -> String {
    if paths.is_empty() {
        return String::new();
    }

    // Find first non-empty path index.
    let first_path_index = match paths.iter().position(|p| !p.is_empty()) {
        Some(i) => i,
        // All paths provided were empty, so return early.
        None => return String::new(),
    };

    let mut buf = String::new();
    buf.push_str(paths[first_path_index]);
    let mut prev_path = paths[first_path_index].as_bytes();
    let mut i = first_path_index + 1;
    while i < paths.len() {
        let this_path = paths[i].as_bytes();
        if this_path.is_empty() {
            i += 1;
            continue;
        }
        let prev_sep = is_sep(prev_path[prev_path.len() - 1]);
        let this_sep = is_sep(this_path[0]);
        if !prev_sep && !this_sep {
            buf.push(SEP as char);
        }
        let adjusted_path = if prev_sep && this_sep {
            &paths[i][1..]
        } else {
            paths[i]
        };
        buf.push_str(adjusted_path);
        prev_path = this_path;
        i += 1;
    }

    buf
}

// `std.fs.path.basename`: the final path component.
fn path_basename(path: &str) -> &str {
    let bytes = path.as_bytes();
    let mut end_index = bytes.len();
    while end_index > 0 && is_sep(bytes[end_index - 1]) {
        end_index -= 1;
    }
    if end_index == 0 {
        return "";
    }
    let mut start_index = end_index;
    while start_index > 0 && !is_sep(bytes[start_index - 1]) {
        start_index -= 1;
    }
    &path[start_index..end_index]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    #[cfg_attr(miri, ignore)] // exercises homedir lookup (env/syscalls)
    fn config_nonempty() {
        let value = config(Options::default()).unwrap();
        assert!(!value.is_empty());
    }

    #[test]
    fn cache_directory_paths() {
        let mock_home = if cfg!(windows) {
            "C:\\Users\\test"
        } else {
            "/Users/test"
        };

        // Test when XDG_CACHE_HOME is not set
        {
            // Test base path
            {
                let cache_path = cache(Options {
                    home: Some(mock_home),
                    ..Options::default()
                })
                .unwrap();
                let expected = path_join(&[mock_home, ".cache"]);
                assert_eq!(expected, cache_path);
            }

            // Test with subdir
            {
                let cache_path = cache(Options {
                    home: Some(mock_home),
                    subdir: Some("rustty"),
                })
                .unwrap();
                let expected = path_join(&[mock_home, ".cache", "rustty"]);
                assert_eq!(expected, cache_path);
            }
        }
    }

    type DirFn = fn(Options) -> Result<String, DirError>;

    struct DirCase {
        name: &'static str,
        func: DirFn,
        default_subdir: &'static str,
    }

    #[test]
    #[cfg_attr(miri, ignore)] // mutates process env + reads HOME (real OS state)
    fn fallback_when_xdg_env_empty() {
        if cfg!(windows) {
            return; // error.SkipZigTest
        }

        let saved_home: Option<CString> = std::env::var_os("HOME")
            .map(|v| CString::new(v.to_string_lossy().into_owned()).unwrap());
        // defer: restore HOME on scope exit
        let _restore_home = RestoreEnv {
            key: c"HOME",
            value: saved_home.clone(),
        };
        let temp_home = "/tmp/ghostty-test-home";
        let _ = env_os::setenv(c"HOME", &CString::new(temp_home).unwrap());

        let cases = [
            DirCase {
                name: "XDG_CONFIG_HOME",
                func: config,
                default_subdir: ".config",
            },
            DirCase {
                name: "XDG_CACHE_HOME",
                func: cache,
                default_subdir: ".cache",
            },
            DirCase {
                name: "XDG_STATE_HOME",
                func: state,
                default_subdir: ".local/state",
            },
        ];

        for case in cases {
            // Save and restore each environment variable
            let saved_env: Option<CString> = std::env::var_os(case.name)
                .map(|v| CString::new(v.to_string_lossy().into_owned()).unwrap());
            let _restore_env = RestoreEnv {
                key: CString::new(case.name).unwrap().into_static(),
                value: saved_env,
            };

            let expected = path_join(&[temp_home, case.default_subdir]);

            // Test with empty string - should fallback to home
            let _ = env_os::setenv(&CString::new(case.name).unwrap(), c"");
            let actual = (case.func)(Options::default()).unwrap();

            assert_eq!(expected, actual);
        }
    }

    #[test]
    #[cfg_attr(miri, ignore)] // mutates process env + reads HOME (real OS state)
    fn fallback_when_xdg_env_empty_and_subdir() {
        if cfg!(windows) {
            return; // error.SkipZigTest
        }

        let saved_home: Option<CString> = std::env::var_os("HOME")
            .map(|v| CString::new(v.to_string_lossy().into_owned()).unwrap());
        let _restore_home = RestoreEnv {
            key: c"HOME",
            value: saved_home.clone(),
        };

        let temp_home = "/tmp/ghostty-test-home";
        let _ = env_os::setenv(c"HOME", &CString::new(temp_home).unwrap());

        let cases = [
            DirCase {
                name: "XDG_CONFIG_HOME",
                func: config,
                default_subdir: ".config",
            },
            DirCase {
                name: "XDG_CACHE_HOME",
                func: cache,
                default_subdir: ".cache",
            },
            DirCase {
                name: "XDG_STATE_HOME",
                func: state,
                default_subdir: ".local/state",
            },
        ];

        for case in cases {
            // Save and restore each environment variable
            let saved_env: Option<CString> = std::env::var_os(case.name)
                .map(|v| CString::new(v.to_string_lossy().into_owned()).unwrap());
            let _restore_env = RestoreEnv {
                key: CString::new(case.name).unwrap().into_static(),
                value: saved_env,
            };

            let expected = path_join(&[temp_home, case.default_subdir, "rustty"]);

            // Test with empty string - should fallback to home
            let _ = env_os::setenv(&CString::new(case.name).unwrap(), c"");
            let actual = (case.func)(Options {
                subdir: Some("rustty"),
                ..Options::default()
            })
            .unwrap();

            assert_eq!(expected, actual);
        }
    }

    #[test]
    #[allow(non_snake_case)]
    fn parseTerminalExec_() {
        {
            let actual = parseTerminalExec(&["a", "b", "c"]);
            assert!(actual.is_none());
        }
        {
            let actual = parseTerminalExec(&["xdg-terminal-exec"]).unwrap();
            assert_eq!(actual, &[] as &[&str]);
        }
        {
            let actual = parseTerminalExec(&["xdg-terminal-exec", "a", "b", "c"]).unwrap();
            assert_eq!(actual, &["a", "b", "c"]);
        }
        {
            let actual = parseTerminalExec(&["xdg-terminal-exec", "-e", "a", "b", "c"]).unwrap();
            assert_eq!(actual, &["a", "b", "c"]);
        }
        {
            let actual = parseTerminalExec(&["xdg-terminal-exec", "a", "-e", "b", "c"]).unwrap();
            assert_eq!(actual, &["a", "-e", "b", "c"]);
        }
    }

    // PORT helper: a Drop guard reproducing the reference test's `defer env: {...}`
    // blocks that save/restore (or unset) an environment variable.
    struct RestoreEnv {
        key: &'static std::ffi::CStr,
        value: Option<CString>,
    }
    impl Drop for RestoreEnv {
        fn drop(&mut self) {
            match &self.value {
                Some(v) => {
                    let _ = env_os::setenv(self.key, v);
                }
                None => {
                    let _ = env_os::unsetenv(self.key);
                }
            }
        }
    }

    // Helper to leak a CString into a 'static CStr for the per-case restore guard
    // (the case names are 'static literals so this leaks nothing across runs of
    // interest; the test process is short-lived).
    trait IntoStaticCStr {
        fn into_static(self) -> &'static std::ffi::CStr;
    }
    impl IntoStaticCStr for CString {
        fn into_static(self) -> &'static std::ffi::CStr {
            Box::leak(self.into_boxed_c_str())
        }
    }
}
