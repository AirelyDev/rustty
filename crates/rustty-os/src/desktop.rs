//! rustty-os::desktop
//!
//! PORT STATUS: ported
//!
//! Ports the reference's `src/os/desktop.zig` — every line, comment, and
//! `test {}` is ported here (R1). Retargeted only for product identity (R2).

// PORT NOTE: the Zig reference imports `std`, `builtin`, `build_config =
// @import("../build_config.zig")`, `posix = std.posix`, and a `@cImport` of
// `unistd.h`. In Rust we use `rustty-build-config` for the build flags and
// `libc` (R3: a binding to the SAME system library) for `getppid`/`getpid`.

// PORT NOTE: the reference's `builtin.os.tag.isDarwin()` guard is a real `cfg`
// here, and `build_config` is only read inside it, so the import carries the
// same cfg (an unconditional `use` is an unused-import warning off Darwin).
#[cfg(target_os = "macos")]
use rustty_build_config as build_config;

// PORT NOTE: `const c = @cImport({ @cInclude("unistd.h"); });`.
#[cfg(not(windows))]
use libc as c;

/// Returns true if the program was launched from a desktop environment.
///
/// On macOS, this returns true if the program was launched from Finder.
///
/// On Linux GTK, this returns true if the program was launched using the
/// desktop file. This also includes when `gtk-launch` is used because I
/// can't find a way to distinguish the two scenarios.
///
/// For other platforms and app runtimes, this returns false.
#[allow(non_snake_case)]
pub fn launchedFromDesktop() -> bool {
    // macOS apps launched from finder or `open` always have the init
    // process as their parent.
    #[cfg(target_os = "macos")]
    {
        // This special case is so that if we launch the app via the
        // app bundle (i.e. via open) then we still treat it as if it
        // was launched from the desktop.
        if build_config::artifact() == build_config::Artifact::Lib {
            'lib: {
                let env = b"RUSTTY_MAC_LAUNCH_SOURCE";
                // PORT NOTE (P3-W7 T3 / area-E `E15`):
                // `global.environ().getPosix(env)` — the captured environment
                // block, not an ad-hoc `posix.getenv`.
                let environ = rustty_global::environ();
                let source = match environ.get_posix(env) {
                    Some(s) => s,
                    None => break 'lib,
                };

                // Source can be "app", "cli", or "zig_run". We assume
                // its the desktop only if its "app". We may want to do
                // "zig_run" but at the moment there's no reason.
                if source == b"app" {
                    return true;
                }
            }
        }

        unsafe { c::getppid() == 1 }
    }

    // On Linux and BSD, GTK sets GIO_LAUNCHED_DESKTOP_FILE and
    // GIO_LAUNCHED_DESKTOP_FILE_PID. We only check the latter to see if
    // we match the PID and assume that if we do, we were launched from
    // the desktop file. Pid comparing catches the scenario where
    // another terminal was launched from a desktop file and then launches
    // rustty and rustty inherits the env.
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    {
        // PORT NOTE (P3-W7 T3 / area-E `E15`):
        // `global.environ().getPosix("GIO_LAUNCHED_DESKTOP_FILE_PID")`.
        let environ = rustty_global::environ();
        let gio_pid_str = match environ.get_posix(b"GIO_LAUNCHED_DESKTOP_FILE_PID") {
            Some(s) => s,
            None => return false,
        };

        let pid = unsafe { c::getpid() };
        // const gio_pid = std.fmt.parseInt(@TypeOf(pid), gio_pid_str, 10) catch break :ul false;
        let gio_pid = match std::str::from_utf8(gio_pid_str)
            .ok()
            .and_then(|s| s.parse::<libc::pid_t>().ok())
        {
            Some(v) => v,
            None => return false,
        };

        gio_pid == pid
    }

    // TODO: This should have some logic to detect this. Perhaps std.builtin.subsystem
    #[cfg(windows)]
    {
        false
    }

    // iPhone/iPad is always launched from the "desktop"
    #[cfg(target_os = "ios")]
    {
        true
    }

    #[cfg(not(any(
        target_os = "macos",
        target_os = "linux",
        target_os = "freebsd",
        windows,
        target_os = "ios"
    )))]
    {
        compile_error!("unsupported platform")
    }
}

/// The list of desktop environments that we detect. New Linux desktop
/// environments should only be added to this list if there's a specific reason
/// to differentiate between `gnome` and `other`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopEnvironment {
    Gnome,
    Macos,
    Other,
    Windows,
}

/// Detect what desktop environment we are running under. This is mainly used
/// on Linux and BSD to enable or disable certain features but there may be more uses in
/// the future.
//
// PORT NOTE (P3-W7 T3 / area-E `E15`, oracle `f8e13f31e`): upstream gained an
// explicit `environ_map: *const std.process.Environ.Map` parameter so the
// detection reads a CALLER-SUPPLIED map instead of the process environment.
// That is what lets its tests build a local map instead of calling `setenv` on
// the process — which in Rust is not merely tidier: `setenv` is not
// thread-safe against a concurrent `getenv` in glibc or Apple libc, and
// `cargo test` runs tests on parallel threads by default (the recorded
// `parallel-cargo-test-exposes-c-init-races` failure class). Rust 2024 makes
// `std::env::set_var` `unsafe` for exactly this reason.
#[allow(non_snake_case)]
#[cfg_attr(
    not(any(target_os = "linux", target_os = "freebsd")),
    allow(unused_variables)
)]
pub fn desktopEnvironment(environ_map: &rustty_global::EnvMap) -> DesktopEnvironment {
    #[cfg(target_os = "macos")]
    {
        DesktopEnvironment::Macos
    }
    #[cfg(windows)]
    {
        DesktopEnvironment::Windows
    }
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    {
        // PORT NOTE: `if (@inComptime()) @compileError("Checking for the desktop
        // environment on Linux/BSD must be done at runtime.");` — Rust has no
        // const-eval reachability here; this function is always runtime.

        // Use $XDG_SESSION_DESKTOP to determine what DE we are using on Linux
        // https://www.freedesktop.org/software/systemd/man/latest/pam_systemd.html#desktop=
        if let Some(sd) = environ_map.get(&b"XDG_SESSION_DESKTOP"[..]) {
            if eql_ignore_case(b"gnome", sd) {
                return DesktopEnvironment::Gnome;
            }
            if eql_ignore_case(b"gnome-xorg", sd) {
                return DesktopEnvironment::Gnome;
            }
        }

        // If $XDG_SESSION_DESKTOP is not set, or doesn't match any known
        // DE, check $XDG_CURRENT_DESKTOP. $XDG_CURRENT_DESKTOP is a
        // colon-separated list of up to three desktop names, although we
        // only look at the first.
        // https://specifications.freedesktop.org/desktop-entry-spec/latest/recognized-keys.html
        if let Some(cd) = environ_map.get(&b"XDG_CURRENT_DESKTOP"[..]) {
            let cd_first = cd.split(|&b| b == b':').next().unwrap_or(b"");
            if eql_ignore_case(cd_first, b"gnome") {
                return DesktopEnvironment::Gnome;
            }
        }

        DesktopEnvironment::Other
    }
    #[cfg(not(any(
        target_os = "macos",
        windows,
        target_os = "linux",
        target_os = "freebsd"
    )))]
    {
        DesktopEnvironment::Other
    }
}

// PORT NOTE (P3-W7 T3): the `posix_getenv` shim is GONE — every reader in this
// module now goes through `global::environ()` (the two `launchedFromDesktop`
// arms) or a caller-supplied `EnvMap` (`desktopEnvironment`), matching upstream
// after `e8525c0fd`/`f8e13f31e`.

// `std.ascii.eqlIgnoreCase`.
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn eql_ignore_case(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b.iter())
            .all(|(x, y)| x.eq_ignore_ascii_case(y))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustty_global::EnvMap;

    // test "desktop environment" {
    //
    // PORT NOTE (P3-W7 T3 / area-E `E15`, oracle `f8e13f31e`): upstream's test
    // was rewritten to build a LOCAL `Environ.Map` per case —
    //
    //   // Always run these tests with a blank evironment map so that we don't
    //   // get any failures from values in the "real" evironment leaking in.
    //
    // — and every `setenv`/`unsetenv` call was deleted. rustty follows: the ten
    // process-`setenv`/`unsetenv` calls that used to live here are GONE, and
    // with them the `RestoreEnv` Drop guard and the `crate::env_test_lock()`
    // serialization those calls forced. `setenv` is not thread-safe against a
    // concurrent `getenv`, and `cargo test` is parallel by default.
    #[test]
    #[allow(non_snake_case)]
    fn desktop_environment() {
        // Always run these tests with a blank evironment map so that we don't get any
        // failures from values in the "real" evironment leaking in.
        #[cfg(target_os = "macos")]
        {
            let environ_map = EnvMap::new();
            assert_eq!(DesktopEnvironment::Macos, desktopEnvironment(&environ_map));
        }
        #[cfg(windows)]
        {
            let environ_map = EnvMap::new();
            assert_eq!(
                DesktopEnvironment::Windows,
                desktopEnvironment(&environ_map)
            );
        }
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        {
            {
                let mut environ_map = EnvMap::new();
                environ_map.insert(b"XDG_SESSION_DESKTOP".to_vec(), b"gnome".to_vec());
                assert_eq!(DesktopEnvironment::Gnome, desktopEnvironment(&environ_map));
            }

            {
                let mut environ_map = EnvMap::new();
                environ_map.insert(b"XDG_SESSION_DESKTOP".to_vec(), b"gnome-xorg".to_vec());
                assert_eq!(DesktopEnvironment::Gnome, desktopEnvironment(&environ_map));
            }

            {
                let mut environ_map = EnvMap::new();
                environ_map.insert(b"XDG_SESSION_DESKTOP".to_vec(), b"foobar".to_vec());
                assert_eq!(DesktopEnvironment::Other, desktopEnvironment(&environ_map));
            }

            {
                let mut environ_map = EnvMap::new();
                environ_map.insert(b"XDG_CURRENT_DESKTOP".to_vec(), b"GNOME".to_vec());
                assert_eq!(DesktopEnvironment::Gnome, desktopEnvironment(&environ_map));
            }

            {
                let mut environ_map = EnvMap::new();
                environ_map.insert(b"XDG_CURRENT_DESKTOP".to_vec(), b"FOOBAR".to_vec());
                assert_eq!(DesktopEnvironment::Other, desktopEnvironment(&environ_map));
            }

            {
                let environ_map = EnvMap::new();
                assert_eq!(DesktopEnvironment::Other, desktopEnvironment(&environ_map));
            }
        }
        #[cfg(not(any(
            target_os = "macos",
            windows,
            target_os = "linux",
            target_os = "freebsd"
        )))]
        {
            // PORT NOTE: the reference test's `else` arm calls `DesktopEnvironment()`
            // (the type, capitalized) rather than `desktopEnvironment()`. We
            // reproduce the intended assertion against the function.
            let environ_map = EnvMap::new();
            assert_eq!(DesktopEnvironment::Other, desktopEnvironment(&environ_map));
        }
    }
}
