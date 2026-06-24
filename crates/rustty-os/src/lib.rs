//! rustty-os
//!
//! PORTS: ghostty src/os/ (34 files incl. wasm/, string_encoding, shell, xdg, path, i18n, ...)
//! + pty.zig. pty.c is built via cc (break #14 relocates os/open->apprt).
//!
//! PORT STATUS: done
//!
//! Every `src/os/*.zig` file (+ `pty.zig`) is ported (R1: every line/comment/
//! test{} ported as the crate is carved, leaves-first, under the acyclicity
//! gate per R4). This crate root is the parity image of the reference's
//! `src/os/main.zig`: it declares the modules and reconstructs the public
//! re-exports (GetEnvResult, getEnvMap, home, open, ...).
//!
//! The module-level doc comment below is the verbatim doc from the reference's
//! `src/os/main.zig`, retargeted only for product identity (R2):
//
// The "os" package contains utilities for interfacing with the operating
// system. These aren't restricted to syscalls or low-level operations, but
// also OS-specific features and conventions.

// Namespaces / files (1:1 with the reference's src/os/*.zig + pty.zig). The
// public re-exports from os/main.zig (GetEnvResult, getEnvMap, home, open, ...)
// are reconstructed below now that every module is ported.

// PORT: the reference imports `builtin` for the `test {}` os-tag switch below.
// In Rust the os switch is `cfg!(...)`.

pub mod args;
pub mod cf_release_thread;
pub mod cgroup;
pub mod dbus;
pub mod desktop;
pub mod env;
pub mod file;
pub mod flatpak;
pub mod homedir;
pub mod hostname;
pub mod i18n;
pub mod i18n_locales;
pub mod kernel_info;
pub mod locale;
pub mod mach;
pub mod macos;
pub mod mouse;
pub mod open;
// PORT: `PageList.pageAllocator()` (src/terminal/PageList.zig). The page
// allocator is OS-specific (darwin tagged mmap vs generic posix page allocator),
// so it is hosted in this `os` crate, mirroring the reference's reach into
// `../os/mach.zig` for the darwin path.
pub mod page_allocator;
pub mod passwd;
pub mod path;
pub mod pipe;
pub mod pty;
pub mod resourcesdir;
pub mod shell;
pub mod string_encoding;
pub mod systemd;
pub mod tempdir;
pub mod uri;
pub mod windows;
pub mod xdg;

// `os/wasm.zig` plus the `os/wasm/` subdirectory (log.zig, target.zig).
pub mod wasm;

// PORT: `pty.zig` lives at `src/pty.zig` in the reference (not under `src/os/`),
// and `os/main.zig` does not re-export it; it is hosted in this crate per the
// porting task and is reached as `rustty_os::pty`. (Module declared above.)

// Namespaces
//
// PORT: `os/main.zig` declares the `pub const args/cgroup/hostname/i18n/mach/
// path/passwd/xdg/windows/macos/shell/uri` namespaces (and the private `const`
// imports `dbus/desktop/env/file/flatpak/homedir/locale/mouse/openpkg = open/
// pipepkg = pipe/resourcesdir/systemd/kernel_info`). In Rust every file is a
// `pub mod` declared above, which already provides exactly those public
// namespaces (`rustty_os::args`, `rustty_os::xdg`, ...); no separate `pub use`
// alias is needed (and `pub mod windows`/`pub mod macos` carry the reference's
// `#![cfg(...)]` os gates verbatim in their own files).

// Functions and types
pub use crate::cf_release_thread as CFReleaseThread;
pub use crate::dbus::launchedByDbusActivation;
pub use crate::desktop::desktopEnvironment;
pub use crate::desktop::launchedFromDesktop;
pub use crate::env::appendEnv;
pub use crate::env::appendEnvAlways;
pub use crate::env::getEnvMap;
pub use crate::env::getenv;
pub use crate::env::prependEnv;
pub use crate::env::setenv;
pub use crate::env::unsetenv;
pub use crate::env::GetEnvResult;
pub use crate::file::allocTmpDir;
pub use crate::file::fixMaxFiles;
pub use crate::file::freeTmpDir;
pub use crate::file::randomTmpPath;
pub use crate::file::restoreMaxFiles;
pub use crate::file::rlimit;
pub use crate::flatpak::isFlatpak;
#[cfg(feature = "flatpak")]
pub use crate::flatpak::FlatpakHostCommand;
pub use crate::homedir::expandHome;
pub use crate::homedir::home;
pub use crate::locale::ensureLocale;
pub use crate::mouse::clickInterval;
pub use crate::open::open;
pub use crate::systemd::launchedBySystemd;
pub use crate::tempdir as TempDir;
// PORT: the reference's `pub const OpenType = openpkg.Type;`. The port renames
// the type to `Kind` (the R4 parity image of `apprt.action.OpenUrl.Kind`); the
// `OpenType` alias is preserved.
pub use crate::kernel_info::getKernelInfo;
pub use crate::open::Kind as OpenType;
pub use crate::pipe::pipe;
pub use crate::resourcesdir::resourcesDir;
pub use crate::resourcesdir::ResourcesDir;
pub use crate::shell::ShellEscapeWriter;

// PORT (test-only glue, R1 "no-loss, not no-addition"): the reference's Zig test
// runner executes a file's `test {}` blocks SERIALLY, so tests that read or
// mutate the process-global environment (`HOME`/`XDG_*`, via `setenv`/`unsetenv`/
// `getenv`) never observe each other's mutations. Rust's `libtest` runner instead
// runs tests in PARALLEL, which would race those process-global reads/writes
// across the `desktop`/`xdg`/`homedir` env tests. This adds nothing-lost: it
// changes no reference logic, comment, or assertion — it only restores the
// reference's serial-execution invariant for the env-touching tests by having
// each acquire one shared lock for the duration of its env access.
#[cfg(test)]
pub(crate) static ENV_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// PORT (test-only glue): acquire the shared env lock. Returns the guard; held
/// for the rest of the calling test so env reads/writes are serialized exactly
/// as the reference's serial test runner guarantees. The lock is poison-tolerant
/// (a panicking test must not deadlock the rest of the suite).
#[cfg(test)]
pub(crate) fn env_test_lock() -> std::sync::MutexGuard<'static, ()> {
    ENV_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    #[test]
    fn main_test_refs() {
        // PORT: the reference's `os/main.zig` `test {}` block forces the test
        // analysis of the named submodules:
        //     _ = file; _ = i18n; _ = path; _ = uri; _ = shell;
        //     if (comptime builtin.os.tag == .linux) { _ = kernel_info; }
        //     else if (comptime builtin.os.tag.isDarwin()) { _ = mach; _ = macos; }
        // In Rust submodules are compiled and tested unconditionally as part of
        // the crate, so this aggregator carries no assertions; it is ported as a
        // no-op `#[test]` to preserve the 1:1 `test {}` -> `#[test]` mapping (R1).
        //
        // The conditional references are reproduced as `cfg!` checks so the
        // os-tag intent is preserved verbatim.
        if cfg!(target_os = "linux") {
            let _ = stringify!(kernel_info);
        } else if cfg!(any(target_os = "macos", target_os = "ios")) {
            let _ = stringify!(mach);
            let _ = stringify!(macos);
        }
        let _ = stringify!(file);
        let _ = stringify!(i18n);
        let _ = stringify!(path);
        let _ = stringify!(uri);
        let _ = stringify!(shell);
    }
}
