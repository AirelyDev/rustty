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

// PORT NOTE (W94-B02-01): the reference fields are `?[]const u8` — raw path
// bytes, NOT UTF-8 text. Byte-typed accordingly (R1 byte-slice rule).
#[derive(Default)]
pub struct Options<'a> {
    /// Subdirectories to join to the base. This avoids extra allocations
    /// when building up the directory. This is commonly the application.
    pub subdir: Option<&'a [u8]>,

    /// The home directory for the user. If this is not set, we will attempt
    /// to look it up which is an expensive process. By setting this, you can
    /// avoid lookups.
    pub home: Option<&'a [u8]>,
}

/// Get the XDG user config directory. The returned value is allocated.
//
// PORT NOTE (P3-W7 T3 / area-E `E15`): upstream's signature became
// `config(alloc, environ_map, opts)` — the `XDG_*_HOME` lookup reads a
// caller-supplied map instead of the process environment. The allocator half
// has no Rust counterpart (the `Vec<u8>` return owns its allocation).
// PORT NOTE (W94-B02-01): the reference returns `![]u8` — raw path bytes. The
// pre-fix `String` surface silently treated a non-UTF-8 `XDG_*_HOME` as unset;
// Ghostty joins the raw env bytes, so this returns `Vec<u8>`.
pub fn config(environ_map: &rustty_global::EnvMap, opts: Options) -> Result<Vec<u8>, DirError> {
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
pub fn cache(environ_map: &rustty_global::EnvMap, opts: Options) -> Result<Vec<u8>, DirError> {
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
pub fn state(environ_map: &rustty_global::EnvMap, opts: Options) -> Result<Vec<u8>, DirError> {
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
) -> Result<Vec<u8>, DirError> {
    // If we have a cached home dir, use that.
    if let Some(home) = opts.home {
        return Ok(path_join(&[
            home,
            internal_opts.default_subdir.as_bytes(),
            opts.subdir.unwrap_or(b""),
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
    // PORT NOTE (W94-B02-01): this surface is byte-typed. The reference keeps
    // the raw env bytes and only checks `env.len > 0`; the former `String`
    // surface (P3-W7 T2 residual) silently treated a non-UTF-8 `XDG_*_HOME`
    // as unset and fell back to `$HOME`, which Ghostty does not do.
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
    // if (env.len > 0) — the ONLY filter the reference applies to the env
    // value; the raw bytes are used as-is (no UTF-8 validation).
    let env_: Option<&[u8]> = env_.filter(|v| !v.is_empty());

    if let Some(env) = env_ {
        // If we have a subdir, then we use the env as-is to avoid a copy.
        if let Some(subdir) = opts.subdir {
            return Ok(path_join(&[env, subdir]));
        }

        // return try alloc.dupe(u8, env);
        return Ok(env.to_vec());
    }

    // Get our home dir
    let mut buf = [0u8; 1024];
    if let Some(home) = homedir::home(environ_map, &mut buf).map_err(|_| DirError::NoHomeDir)? {
        // PORT NOTE (W94-B02-01): `homedir.home` returns `[]const u8`; the raw
        // bytes feed the byte-typed path join directly (the former `&str`
        // round-trip rejected a non-UTF-8 home dir with `NoHomeDir`, which the
        // reference does not do).
        return Ok(path_join(&[
            home,
            internal_opts.default_subdir.as_bytes(),
            opts.subdir.unwrap_or(b""),
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
// PORT NOTE (W94-B02-01): operates on raw path bytes (`[]const u8` in Zig),
// so non-UTF-8 components join losslessly.
fn path_join(paths: &[&[u8]]) -> Vec<u8> {
    if paths.is_empty() {
        return Vec::new();
    }

    // Find first non-empty path index.
    let first_path_index = match paths.iter().position(|p| !p.is_empty()) {
        Some(i) => i,
        // All paths provided were empty, so return early.
        None => return Vec::new(),
    };

    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(paths[first_path_index]);
    let mut prev_path = paths[first_path_index];
    let mut i = first_path_index + 1;
    while i < paths.len() {
        let this_path = paths[i];
        if this_path.is_empty() {
            i += 1;
            continue;
        }
        let prev_sep = is_sep(prev_path[prev_path.len() - 1]);
        let this_sep = is_sep(this_path[0]);
        if !prev_sep && !this_sep {
            buf.push(SEP);
        }
        let adjusted_path = if prev_sep && this_sep {
            &this_path[1..]
        } else {
            this_path
        };
        buf.extend_from_slice(adjusted_path);
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
        let mock_home: &[u8] = if cfg!(windows) {
            b"C:\\Users\\test"
        } else {
            b"/Users/test"
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
                let expected = path_join(&[mock_home, b".cache"]);
                assert_eq!(expected, cache_path);
            }

            // Test with subdir
            {
                let cache_path = cache(
                    &environ_map,
                    Options {
                        home: Some(mock_home),
                        subdir: Some(b"rustty"),
                    },
                )
                .unwrap();
                let expected = path_join(&[mock_home, b".cache", b"rustty"]);
                assert_eq!(expected, cache_path);
            }
        }
    }

    type DirFn = fn(&EnvMap, Options) -> Result<Vec<u8>, DirError>;

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

            let expected = path_join(&[temp_home.as_bytes(), case.default_subdir.as_bytes()]);

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

            let expected = path_join(&[
                temp_home.as_bytes(),
                case.default_subdir.as_bytes(),
                b"rustty",
            ]);

            // Test with empty string - should fallback to home
            environ_map.insert(case.name.as_bytes().to_vec(), Vec::new());
            let actual = (case.func)(
                &environ_map,
                Options {
                    subdir: Some(b"rustty"),
                    ..Options::default()
                },
            )
            .unwrap();

            assert_eq!(expected, actual);
        }
    }

    // PROVING TEST (W94-B02-01, no reference counterpart): Ghostty's `dir`
    // applies exactly one filter to the env value — `env.len > 0` — and then
    // joins the RAW bytes. A non-UTF-8 `XDG_*_HOME` must be used as-is, not
    // silently discarded in favor of the `$HOME` fallback (the pre-fix
    // `String::from_utf8` surface did the latter).
    #[test]
    fn non_utf8_xdg_env_is_used_raw() {
        if cfg!(windows) {
            return; // error.SkipZigTest
        }

        for case in dir_cases() {
            let mut environ_map = EnvMap::new();
            environ_map.insert(b"HOME".to_vec(), b"/tmp/rustty-test-home".to_vec());
            // 0xFF is never valid in UTF-8.
            environ_map.insert(case.name.as_bytes().to_vec(), b"/tmp/w94-\xff-xdg".to_vec());

            // With a subdir: the raw env bytes are joined with the subdir.
            let actual = (case.func)(
                &environ_map,
                Options {
                    subdir: Some(b"rustty"),
                    ..Options::default()
                },
            )
            .unwrap();
            assert_eq!(b"/tmp/w94-\xff-xdg/rustty".to_vec(), actual);

            // Without a subdir: the raw env bytes are duplicated as-is.
            let actual = (case.func)(&environ_map, Options::default()).unwrap();
            assert_eq!(b"/tmp/w94-\xff-xdg".to_vec(), actual);
        }
    }

    // PROVING TEST (W94-B02-01 companion): a non-UTF-8 `$HOME` also flows
    // through the byte-typed join instead of being rejected as `NoHomeDir`.
    #[test]
    fn non_utf8_home_fallback_is_used_raw() {
        if cfg!(windows) {
            return; // error.SkipZigTest
        }

        for case in dir_cases() {
            let mut environ_map = EnvMap::new();
            environ_map.insert(b"HOME".to_vec(), b"/tmp/w94-\xfe-home".to_vec());

            let expected = path_join(&[
                b"/tmp/w94-\xfe-home",
                case.default_subdir.as_bytes(),
                b"rustty",
            ]);
            let actual = (case.func)(
                &environ_map,
                Options {
                    subdir: Some(b"rustty"),
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
