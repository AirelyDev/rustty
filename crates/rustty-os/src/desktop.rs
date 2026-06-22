//! rustty-os::desktop
//!
//! PORT STATUS: ported
//!
//! Ports the reference's `src/os/desktop.zig` — every line, comment, and
//! `test {}` is ported here (R1). Retargeted only for product identity (R2).

// PORT: the Zig reference imports `std`, `builtin`, `build_config =
// @import("../build_config.zig")`, `posix = std.posix`, and a `@cImport` of
// `unistd.h`. In Rust we use `rustty-build-config` for the build flags and
// `libc` (R3: a binding to the SAME system library) for `getppid`/`getpid`.

use rustty_build_config as build_config;

// PORT: `const c = @cImport({ @cInclude("unistd.h"); });`.
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
                let env = "RUSTTY_MAC_LAUNCH_SOURCE";
                let source = match posix_getenv(env) {
                    Some(s) => s,
                    None => break 'lib,
                };

                // Source can be "app", "cli", or "zig_run". We assume
                // its the desktop only if its "app". We may want to do
                // "zig_run" but at the moment there's no reason.
                if source == "app" {
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
        let gio_pid_str = match posix_getenv("GIO_LAUNCHED_DESKTOP_FILE_PID") {
            Some(s) => s,
            None => return false,
        };

        let pid = unsafe { c::getpid() };
        let gio_pid = match gio_pid_str.parse::<libc::pid_t>() {
            Ok(v) => v,
            Err(_) => return false,
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
#[allow(non_snake_case)]
pub fn desktopEnvironment() -> DesktopEnvironment {
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
        // PORT: `if (@inComptime()) @compileError("Checking for the desktop
        // environment on Linux/BSD must be done at runtime.");` — Rust has no
        // const-eval reachability here; this function is always runtime.

        // Use $XDG_SESSION_DESKTOP to determine what DE we are using on Linux
        // https://www.freedesktop.org/software/systemd/man/latest/pam_systemd.html#desktop=
        if let Some(sd) = posix_getenv("XDG_SESSION_DESKTOP") {
            if eql_ignore_case("gnome", &sd) {
                return DesktopEnvironment::Gnome;
            }
            if eql_ignore_case("gnome-xorg", &sd) {
                return DesktopEnvironment::Gnome;
            }
        }

        // If $XDG_SESSION_DESKTOP is not set, or doesn't match any known
        // DE, check $XDG_CURRENT_DESKTOP. $XDG_CURRENT_DESKTOP is a
        // colon-separated list of up to three desktop names, although we
        // only look at the first.
        // https://specifications.freedesktop.org/desktop-entry-spec/latest/recognized-keys.html
        if let Some(cd) = posix_getenv("XDG_CURRENT_DESKTOP") {
            let cd_first = cd.split(':').next().unwrap_or("");
            if eql_ignore_case(cd_first, "gnome") {
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

// `posix.getenv`: returns the value as a `String` or `None`.
#[cfg(not(windows))]
fn posix_getenv(key: &str) -> Option<String> {
    std::env::var(key).ok()
}

// `std.ascii.eqlIgnoreCase`.
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn eql_ignore_case(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .all(|(x, y)| x.eq_ignore_ascii_case(&y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg_attr(miri, ignore)] // reads/mutates process env (real OS state)
    #[allow(non_snake_case)]
    fn desktop_environment() {
        let _env = crate::env_test_lock();
        #[cfg(target_os = "macos")]
        {
            assert_eq!(DesktopEnvironment::Macos, desktopEnvironment());
        }
        #[cfg(windows)]
        {
            assert_eq!(DesktopEnvironment::Windows, desktopEnvironment());
        }
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        {
            use crate::env::{setenv, unsetenv};
            let getenv = |k: &str| std::env::var(k).ok();

            let xdg_current_desktop = getenv("XDG_CURRENT_DESKTOP");
            let _restore_current = RestoreEnv {
                key: c"XDG_CURRENT_DESKTOP",
                value: xdg_current_desktop,
            };
            let _ = unsetenv(c"XDG_CURRENT_DESKTOP");

            let xdg_session_desktop = getenv("XDG_SESSION_DESKTOP");
            let _restore_session = RestoreEnv {
                key: c"XDG_SESSION_DESKTOP",
                value: xdg_session_desktop,
            };
            let _ = unsetenv(c"XDG_SESSION_DESKTOP");

            let _ = setenv(c"XDG_SESSION_DESKTOP", c"gnome");
            assert_eq!(DesktopEnvironment::Gnome, desktopEnvironment());
            let _ = setenv(c"XDG_SESSION_DESKTOP", c"gnome-xorg");
            assert_eq!(DesktopEnvironment::Gnome, desktopEnvironment());
            let _ = setenv(c"XDG_SESSION_DESKTOP", c"foobar");
            assert_eq!(DesktopEnvironment::Other, desktopEnvironment());

            let _ = unsetenv(c"XDG_SESSION_DESKTOP");
            assert_eq!(DesktopEnvironment::Other, desktopEnvironment());

            let _ = setenv(c"XDG_CURRENT_DESKTOP", c"GNOME");
            assert_eq!(DesktopEnvironment::Gnome, desktopEnvironment());
            let _ = setenv(c"XDG_CURRENT_DESKTOP", c"FOOBAR");
            assert_eq!(DesktopEnvironment::Other, desktopEnvironment());
            let _ = unsetenv(c"XDG_CURRENT_DESKTOP");
            assert_eq!(DesktopEnvironment::Other, desktopEnvironment());
        }
        #[cfg(not(any(
            target_os = "macos",
            windows,
            target_os = "linux",
            target_os = "freebsd"
        )))]
        {
            // PORT: the reference test's `else` arm calls `DesktopEnvironment()`
            // (the type, capitalized) rather than `desktopEnvironment()`. We
            // reproduce the intended assertion against the function.
            assert_eq!(DesktopEnvironment::Other, desktopEnvironment());
        }
    }

    // PORT helper: a Drop guard reproducing the reference test's `defer if (...)
    // |v| setenv else unsetenv` env save/restore behavior.
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    struct RestoreEnv {
        key: &'static std::ffi::CStr,
        value: Option<String>,
    }
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    impl Drop for RestoreEnv {
        fn drop(&mut self) {
            use crate::env::{setenv, unsetenv};
            match &self.value {
                Some(v) => {
                    let _ = setenv(self.key, &std::ffi::CString::new(v.as_str()).unwrap());
                }
                None => {
                    let _ = unsetenv(self.key);
                }
            }
        }
    }
}
