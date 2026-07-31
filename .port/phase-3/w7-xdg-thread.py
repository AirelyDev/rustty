#!/usr/bin/env python3
"""P3-W7 T3 (area-E `E15`) — thread the explicit environment map through
`rustty-os`'s `xdg` helpers and delete the six process-`setenv` calls from its
tests, mirroring oracle `e8525c0fd` + `f8e13f31e`."""
import re
import sys

P = "crates/rustty-os/src/xdg.rs"
src = open(P, encoding="utf-8").read()

EDITS = [
    (
        """/// Get the XDG user config directory. The returned value is allocated.
pub fn config(opts: Options) -> Result<String, DirError> {
    dir(
        opts,""",
        """/// Get the XDG user config directory. The returned value is allocated.
//
// PORT NOTE (P3-W7 T3 / area-E `E15`): upstream's signature became
// `config(alloc, environ_map, opts)` — the `XDG_*_HOME` lookup reads a
// caller-supplied map instead of the process environment. The allocator half
// has no Rust counterpart (the `String` return owns its allocation).
pub fn config(environ_map: &rustty_global::EnvMap, opts: Options) -> Result<String, DirError> {
    dir(
        environ_map,
        opts,""",
    ),
    (
        """pub fn cache(opts: Options) -> Result<String, DirError> {
    dir(
        opts,""",
        """pub fn cache(environ_map: &rustty_global::EnvMap, opts: Options) -> Result<String, DirError> {
    dir(
        environ_map,
        opts,""",
    ),
    (
        """pub fn state(opts: Options) -> Result<String, DirError> {
    dir(
        opts,""",
        """pub fn state(environ_map: &rustty_global::EnvMap, opts: Options) -> Result<String, DirError> {
    dir(
        environ_map,
        opts,""",
    ),
    (
        """fn dir(opts: Options, internal_opts: InternalOptions) -> Result<String, DirError> {""",
        """fn dir(
    environ_map: &rustty_global::EnvMap,
    opts: Options,
    internal_opts: InternalOptions,
) -> Result<String, DirError> {""",
    ),
    (
        """    // PORT NOTE (P3-W7 T2): `getenvNotEmpty` is byte-typed now (the reference
    // always was). This module's public surface still returns `String` paths,
    // so a non-UTF-8 `XDG_*_HOME` is still treated as unset here — exactly the
    // pre-existing behaviour, not a new one. Byte-typing the XDG path surface
    // ripples into `rustty-cli`/`rustty-config` and is NOT part of this task;
    // it is recorded as residual in the P3-W7 report.
    let env_ = match env_os::getenvNotEmpty(internal_opts.env.as_bytes()).unwrap() {
        Some(v) => Some(v),
        None => {
            if cfg!(windows) {
                env_os::getenvNotEmpty(internal_opts.windows_env.as_bytes()).unwrap()
            } else {
                None
            }
        }
    };
    let env_: Option<String> = env_.and_then(|v| String::from_utf8(v.value).ok());""",
        """    // PORT NOTE (P3-W7 T3): the lookup goes through the caller-supplied map,
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
        let primary = environ_map.get(internal_opts.env.as_bytes()).map(Vec::as_slice);
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
        .and_then(|v| String::from_utf8(v.to_vec()).ok());""",
    ),
    (
        """    if let Some(home) = homedir::home(&mut buf).map_err(|_| DirError::NoHomeDir)? {""",
        """    if let Some(home) = homedir::home(environ_map, &mut buf).map_err(|_| DirError::NoHomeDir)? {""",
    ),
]

for old, new in EDITS:
    if old not in src:
        print("MISS: " + old.splitlines()[0][:80], file=sys.stderr)
        sys.exit(1)
    src = src.replace(old, new, 1)

# ---- replace the whole test module ------------------------------------------
start = src.index("#[cfg(test)]\nmod tests {")
tail_marker = "    fn parse_terminal_exec() {"
head = src[:start]

NEW_TESTS = r'''#[cfg(test)]
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
'''

# keep the parseTerminalExec test verbatim from the old module
old_tests = src[start:]
pt_start = old_tests.index("    #[test]\n    #[allow(non_snake_case)]\n    fn parse_terminal_exec() {")
pt = old_tests[pt_start:]
# drop the trailing helper structs (RestoreEnv / IntoStaticCStr)
helper_start = pt.index("    // PORT NOTE: helper: a Drop guard")
pt = pt[:helper_start].rstrip() + "\n}\n"
# NEW_TESTS already emits a bare `#[test]`; splice in the rest of the fn.
pt = pt[len("    #[test]\n") :]

src = head + NEW_TESTS + pt
open(P, "w", encoding="utf-8").write(src)
print("patched", P)
