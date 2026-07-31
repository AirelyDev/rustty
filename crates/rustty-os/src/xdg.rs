//! Implementation of the XDG Base Directory specification
//! (https://specifications.freedesktop.org/basedir-spec/basedir-spec-latest.html)
//!
//! rustty-os::xdg
//!
//! PORT STATUS: ported
//!
//! Ports the reference's `src/os/xdg.zig` — every line, comment, and `test {}`
//! is ported here (R1). Retargeted only for product identity (R2).

// PORT NOTE: the Zig reference imports `std`, `builtin`, `Allocator`, `posix`,
// `homedir = @import("homedir.zig")`, and `env_os = @import("env.zig")`. In Rust
// we use those ported sibling modules plus the standard library. Allocation in
// Rust is infallible for the collections used here, so the reference's
// `Allocator.Error` / `error.NoHomeDir` set collapses to the single
// `error.NoHomeDir` case kept in `DirError`.

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
//
// PORT NOTE (P3-W7 T3 / area-E `E15`): upstream's signature became
// `config(alloc, environ_map, opts)` — the `XDG_*_HOME` lookup reads a
// caller-supplied map instead of the process environment. The allocator half
// has no Rust counterpart (the `String` return owns its allocation).
pub fn config(environ_map: &rustty_global::EnvMap, opts: Options) -> Result<String, DirError> {
    dir(
        environ_map,
        opts,
        InternalOptions {
            env: "XDG_CONFIG_HOME",
            windows_env: "LOCALAPPDATA",
            default_subdir: ".config",
        },
    )
}

/// Get the XDG cache directory. The returned value is allocated.
pub fn cache(environ_map: &rustty_global::EnvMap, opts: Options) -> Result<String, DirError> {
    dir(
        environ_map,
        opts,
        InternalOptions {
            env: "XDG_CACHE_HOME",
            windows_env: "LOCALAPPDATA",
            default_subdir: ".cache",
        },
    )
}

/// Get the XDG state directory. The returned value is allocated.
pub fn state(environ_map: &rustty_global::EnvMap, opts: Options) -> Result<String, DirError> {
    dir(
        environ_map,
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
    // (read only by the `cfg(windows)` fallback arm in `dir`.)
    #[cfg_attr(not(windows), allow(dead_code))]
    windows_env: &'static str,
    default_subdir: &'static str,
}

/// Unified helper to get XDG directories that follow a common pattern.
fn dir(
    environ_map: &rustty_global::EnvMap,
    opts: Options,
    internal_opts: InternalOptions,
) -> Result<String, DirError> {
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
    // PORT NOTE (P3-W7 T3): the lookup goes through the caller-supplied map,
    // matching upstream's
    //   const env = switch (builtin.os.tag) {
    //       .windows => environ_map.get(internal_opts.env) orelse
    //                   environ_map.get(internal_opts.windows_env) orelse "",
    //       else => environ_map.get(internal_opts.env) orelse "",
    //   };
    // PORT NOTE (P3-W7 T2 residual): this module's public surface still returns
    // `String` paths, so a non-UTF-8 `XDG_*_HOME` is treated as unset here —
    // exactly the pre-existing behaviour, not a new one. Byte-typing the XDG
    // path surface ripples into `rustty-cli`/`rustty-config` and is NOT part of
    // this task; it is recorded as residual in the P3-W7 report.
    let env_: Option<&[u8]> = {
        let primary = environ_map
            .get(internal_opts.env.as_bytes())
            .map(Vec::as_slice);
        #[cfg(windows)]
        {
            primary.or_else(|| {
                environ_map
                    .get(internal_opts.windows_env.as_bytes())
                    .map(Vec::as_slice)
            })
        }
        #[cfg(not(windows))]
        {
            primary
        }
    };
    let env_: Option<String> = env_
        .filter(|v| !v.is_empty())
        .and_then(|v| String::from_utf8(v.to_vec()).ok());
    // PORT NOTE: `defer if (env_) |env| env.deinit(alloc);` — `GetEnvResult`'s deinit
    // is Drop in Rust, so `env_` is released automatically when this function
    // returns.

    if let Some(env) = &env_ {
        // If we have a subdir, then we use the env as-is to avoid a copy.
        if let Some(subdir) = opts.subdir {
            return Ok(path_join(&[env, subdir]));
        }

        return Ok(env.clone());
    }

    // Get our home dir
    let mut buf = [0u8; 1024];
    if let Some(home) = homedir::home(environ_map, &mut buf).map_err(|_| DirError::NoHomeDir)? {
        // PORT NOTE: `homedir.home` returns `[]const u8`; convert to `&str` for the
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
// PORT NOTE: support shims for `std.fs.path.{join,basename}` used above.
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
    use rustty_global::EnvMap;

    // PORT NOTE (P3-W7 T3 / area-E `E15`, oracle `f8e13f31e`): upstream's xdg
    // tests were rewritten to build a LOCAL `Environ.Map` per case, and every
    // `setenv`/`unsetenv` call was deleted. rustty follows: the SIX
    // process-`setenv` calls that used to live in this module are GONE, along
    // with the `RestoreEnv` Drop guard, the `IntoStaticCStr` leak helper, and
    // the `crate::env_test_lock()` serialization those calls forced. `setenv`
    // is not thread-safe against a concurrent `getenv`, and `cargo test` is
    // parallel by default.

    #[test]
    #[cfg_attr(miri, ignore)] // exercises homedir lookup (env/syscalls)
    fn config_nonempty() {
        let environ_map = rustty_global::environ_map();
        let value = config(&environ_map, Options::default()).unwrap();
        assert!(!value.is_empty());
    }

    // test "cache directory paths" {
    #[test]
    fn cache_directory_paths() {
        let mock_home = if cfg!(windows) {
            "C:\\Users\\test"
        } else {
            "/Users/test"
        };
        let environ_map = EnvMap::new();

        // Test when XDG_CACHE_HOME is not set
        {
            // Test base path
            {
                let cache_path = cache(
                    &environ_map,
                    Options {
                        home: Some(mock_home),
                        ..Options::default()
                    },
                )
                .unwrap();
                let expected = path_join(&[mock_home, ".cache"]);
                assert_eq!(expected, cache_path);
            }

            // Test with subdir
            {
                let cache_path = cache(
                    &environ_map,
                    Options {
                        home: Some(mock_home),
                        subdir: Some("rustty"),
                    },
                )
                .unwrap();
                let expected = path_join(&[mock_home, ".cache", "rustty"]);
                assert_eq!(expected, cache_path);
            }
        }
    }

    type DirFn = fn(&EnvMap, Options) -> Result<String, DirError>;

    struct DirCase {
        name: &'static str,
        func: DirFn,
        default_subdir: &'static str,
    }

    fn dir_cases() -> [DirCase; 3] {
        [
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
        ]
    }

    // test "fallback when xdg env empty" {
    #[test]
    fn fallback_when_xdg_env_empty() {
        if cfg!(windows) {
            return; // error.SkipZigTest
        }

        for case in dir_cases() {
            let mut environ_map = EnvMap::new();
            let temp_home = "/tmp/rustty-test-home";
            environ_map.insert(b"HOME".to_vec(), temp_home.as_bytes().to_vec());

            let expected = path_join(&[temp_home, case.default_subdir]);

            // Test with empty string - should fallback to home
            environ_map.insert(case.name.as_bytes().to_vec(), Vec::new());
            let actual = (case.func)(&environ_map, Options::default()).unwrap();

            assert_eq!(expected, actual);
        }
    }

    // test "fallback when xdg env empty and subdir" {
    #[test]
    fn fallback_when_xdg_env_empty_and_subdir() {
        if cfg!(windows) {
            return; // error.SkipZigTest
        }

        for case in dir_cases() {
            let mut environ_map = EnvMap::new();
            let temp_home = "/tmp/rustty-test-home";
            environ_map.insert(b"HOME".to_vec(), temp_home.as_bytes().to_vec());

            let expected = path_join(&[temp_home, case.default_subdir, "rustty"]);

            // Test with empty string - should fallback to home
            environ_map.insert(case.name.as_bytes().to_vec(), Vec::new());
            let actual = (case.func)(
                &environ_map,
                Options {
                    subdir: Some("rustty"),
                    ..Options::default()
                },
            )
            .unwrap();

            assert_eq!(expected, actual);
        }
    }

    #[test]
    #[allow(non_snake_case)]
    fn parse_terminal_exec() {
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
}
