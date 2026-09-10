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

// PORT NOTE: the reference imports `builtin` for the `test {}` os-tag switch below.
// In Rust the os switch is `cfg!(...)`.

// PORT NOTE (§3.4 wasm32 scoped-green): the reference is compiled with Zig's
// LAZY per-declaration analysis, so a wasm build only ever analyzes the decls it
// reaches; the POSIX-only modules are simply never touched. Rust compiles a
// whole crate eagerly, so the 14 POSIX modules below (which reach `libc`, the
// `std::os::unix` extension traits, or emit an `@compileError`-equivalent
// `unsupported platform` on non-POSIX targets) are gated
// `#[cfg(not(target_arch = "wasm32"))]`. This is the parity-preserving analog of
// Zig's laziness: on wasm32 exactly the `os/wasm*` helper surface remains, which
// is the whole point of a wasm build. Every other target compiles all modules
// unchanged. See the workstream report (B2) for the full-workspace follow-up.
// PORT NOTE (P3-W7 T4): `pub mod args;` is GONE — upstream DELETED
// `src/os/args.zig` (-130) in `e8525c0fd`. Zig 0.16 hands `std.process.Args`
// to `main()` inside `std.process.Init.Minimal` and the C API takes them from
// the caller, so the macOS `NSProcessInfo` detour is no longer needed. On the
// Rust side that detour never had a reason to exist (`std::env::args_os()` on
// macOS reads the real process argv via `_NSGetArgv`, valid under
// `NSApplicationMain`) AND it was lossy. The consumer seam moved to
// `rustty_args::args::OsArgIterator`.
pub mod autorelease;

// TEST-ONLY `log::Log` capture backend for the restored `log.*` sites
// (task-14e-2, S111-F4/S111-F5). See the module header.
pub mod cf_release_thread;
pub mod cgroup;
pub mod dbus;
#[cfg(not(target_arch = "wasm32"))]
pub mod desktop;
#[cfg(test)]
pub(crate) mod log_capture;
// PORT NOTE: `os/main.zig:22` `pub const edit = @import("edit.zig");`. Ungated:
// `edit` reaches only `rustty-global`'s `environ()` and the crate-local
// `shell`/`string_encoding` writers, all of which are available on every target.
pub mod edit;
// PORT NOTE (P3-W7 T4): `pub mod env;` is GONE — upstream DELETED
// `src/os/env.zig` (-178) in `e8525c0fd`, and `src/os/main.zig` dropped all of
// its re-exports (`GetEnvResult`, `getEnvMap`, `appendEnv`, `appendEnvAlways`,
// `prependEnv`, `getenv`, `setenv`, `unsetenv`, plus `allocTmpDir`/`freeTmpDir`).
// The replacements:
//   getEnvMap                 -> `rustty_global::environ_map()`
//   getenv / getenvNotEmpty   -> `rustty_global::environ().get_posix/get_alloc`
//                                or a caller-supplied `EnvMap`
//   appendEnv/appendEnvAlways -> `rustty-termio`'s `exec.rs` (file-private)
//   prependEnv                -> `rustty-termio`'s `shell_integration.rs`
//   setenv / unsetenv         -> file-local `extern "C"` decls in the two
//                                modules that legitimately mutate the process
//                                environment (`locale.rs`, GTK `application.rs`),
//                                per upstream's own `locale.zig` precedent
#[cfg(not(target_arch = "wasm32"))]
pub mod file;
pub mod flatpak;
#[cfg(not(target_arch = "wasm32"))]
pub mod homedir;
#[cfg(not(target_arch = "wasm32"))]
pub mod hostname;
pub mod i18n;
pub mod i18n_locales;
pub mod kernel_info;
#[cfg(not(target_arch = "wasm32"))]
pub mod locale;
// PORT NOTE (§3.4 laziness, windows arm): `os/main.zig:24`'s
// `pub const mach = @import("mach.zig");` is a LAZY decl — the reference only
// ever analyzes it from darwin code (`terminal/PageList.zig:518-530`'s
// `pageAllocator()` reaches `@import("../os/mach.zig")` inside the
// `isDarwin()` branch; `os/main.zig:66-69`'s `_ = mach;` sits in the
// `isDarwin()` arm of the test). It is never analyzed on windows. `mach.rs` is
// posix-`mmap`/`sysconf`-bound (raw `libc`), so Rust's eager compilation needs
// the analog of that laziness spelled as a cfg: the unix gate. (wasm32 is not
// `target_family = "unix"`, so this subsumes the previous wasm32 exclusion.)
// See `.port/codex-vet/designs/W2D_WINDOWS_OS.md` WI-1.
#[cfg(target_family = "unix")]
pub mod mach;
pub mod macos;
pub mod mouse;
#[cfg(not(target_arch = "wasm32"))]
pub mod open;
// PORT NOTE: `PageList.pageAllocator()` (src/terminal/PageList.zig). The page
// allocator is OS-specific (darwin tagged mmap vs generic posix page allocator),
// so it is hosted in this `os` crate, mirroring the reference's reach into
// `../os/mach.zig` for the darwin path.
// PORT NOTE (§3.4 wasm32): the page allocator IS needed on wasm32 — it is the VT
// core's page-backing allocator (PageList). On wasm it has no `mmap`, so it
// serves page-aligned, zeroed memory from the Rust global heap (see
// `page_allocator.rs`'s std-heap branch, shared with the Miri path). So it stays
// ungated (unlike the POSIX-only modules).
pub mod page_allocator;
#[cfg(not(target_arch = "wasm32"))]
pub mod passwd;
#[cfg(not(target_arch = "wasm32"))]
pub mod path;
#[cfg(not(target_arch = "wasm32"))]
pub mod pipe;
#[cfg(not(target_arch = "wasm32"))]
pub mod pty;
#[cfg(not(target_arch = "wasm32"))]
pub mod resourcesdir;
pub mod shell;
// PORT NOTE: `os/main.zig:32` `pub const stderr = @import("stderr.zig");`.
// Ungated: the module is self-contained (hand-rolled `extern`s + a no-op arm for
// freestanding/other), so it compiles on every target including wasm32.
pub mod stderr;
pub mod string_encoding;
pub mod systemd;
#[cfg(not(target_arch = "wasm32"))]
pub mod tempdir;
pub mod uri;
pub mod windows;
// PORT NOTE (§3.4 wasm32): `xdg` is not itself POSIX-bound, but it consumes
// `homedir` (`homedir::home` for the `$HOME`-relative default dirs), which IS
// gated above; so `xdg` carries the same `wasm32` gate transitively.
#[cfg(not(target_arch = "wasm32"))]
pub mod xdg;

// `os/wasm.zig` plus the `os/wasm/` subdirectory (log.zig, target.zig).
pub mod wasm;

// PORT NOTE: `pty.zig` lives at `src/pty.zig` in the reference (not under `src/os/`),
// and `os/main.zig` does not re-export it; it is hosted in this crate per the
// porting task and is reached as `rustty_os::pty`. (Module declared above.)

// Namespaces
//
// PORT NOTE: `os/main.zig` declares the `pub const args/cgroup/hostname/i18n/mach/
// path/passwd/xdg/windows/macos/shell/uri` namespaces (and the private `const`
// imports `dbus/desktop/env/file/flatpak/homedir/locale/mouse/openpkg = open/
// pipepkg = pipe/resourcesdir/systemd/kernel_info`). In Rust every file is a
// `pub mod` declared above, which already provides exactly those public
// namespaces (`rustty_os::args`, `rustty_os::xdg`, ...); no separate `pub use`
// alias is needed (and `pub mod windows`/`pub mod macos` carry the reference's
// `#![cfg(...)]` os gates verbatim in their own files).

// Functions and types
//
// PORT NOTE (§3.4 wasm32): the re-exports drawn from the `#[cfg(not(target_arch
// = "wasm32"))]`-gated POSIX modules above carry the SAME gate so the crate's
// public surface stays consistent on wasm32 (nothing names a gated module).
pub use crate::cf_release_thread as CFReleaseThread;
pub use crate::dbus::launchedByDbusActivation;
#[cfg(not(target_arch = "wasm32"))]
pub use crate::desktop::desktopEnvironment;
#[cfg(not(target_arch = "wasm32"))]
pub use crate::desktop::launchedFromDesktop;
// PORT NOTE (P3-W7 T2 / area-E `E13`): the `appendEnv`/`appendEnvAlways`/
// `prependEnv` re-exports are GONE — upstream `e8525c0fd` moved those
// functions into `src/termio/Exec.zig` and
// `src/termio/shell_integration.zig` on the way to deleting `src/os/env.zig`,
// and `src/os/main.zig` dropped the corresponding `pub const` lines.
// PORT NOTE (P3-W7 T4 / area-E `E20`): the `allocTmpDir`/`freeTmpDir`
// re-exports are GONE — upstream `e8525c0fd` dropped them from `os/main.zig`
// because `global.zig` became their only importer. `crate::file::allocTmpDir`
// is still reachable by path for that one caller (`tempdir.rs` and the app
// initializer, which publishes the result via
// `rustty_global::set_tmp_dir_path`).
#[cfg(not(target_arch = "wasm32"))]
pub use crate::file::fixMaxFiles;
#[cfg(not(target_arch = "wasm32"))]
pub use crate::file::randomTmpPath;
#[cfg(not(target_arch = "wasm32"))]
pub use crate::file::restoreMaxFiles;
#[cfg(not(target_arch = "wasm32"))]
pub use crate::file::rlimit;
pub use crate::flatpak::isFlatpak;
#[cfg(feature = "flatpak")]
pub use crate::flatpak::FlatpakHostCommand;
#[cfg(not(target_arch = "wasm32"))]
pub use crate::homedir::expandHome;
#[cfg(not(target_arch = "wasm32"))]
pub use crate::homedir::home;
#[cfg(not(target_arch = "wasm32"))]
pub use crate::locale::ensureLocale;
pub use crate::mouse::clickInterval;
#[cfg(not(target_arch = "wasm32"))]
pub use crate::open::open;
pub use crate::systemd::launchedBySystemd;
#[cfg(not(target_arch = "wasm32"))]
pub use crate::tempdir as TempDir;
// PORT NOTE: the reference's `pub const OpenType = openpkg.Type;`. The port renames
// the type to `Kind` (the R4 parity image of `apprt.action.OpenUrl.Kind`); the
// `OpenType` alias is preserved.
pub use crate::kernel_info::getKernelInfo;
// PORT NOTE: `os/main.zig:59` `pub const getConfigEditCommand =
// edit.getConfigEditCommand;`.
pub use crate::edit::getConfigEditCommand;
#[cfg(not(target_arch = "wasm32"))]
pub use crate::open::Kind as OpenType;
#[cfg(not(target_arch = "wasm32"))]
pub use crate::pipe::pipe;
#[cfg(not(target_arch = "wasm32"))]
pub use crate::resourcesdir::resourcesDir;
#[cfg(not(target_arch = "wasm32"))]
pub use crate::resourcesdir::ResourcesDir;
pub use crate::shell::ShellEscapeWriter;

// PORT NOTE (P3-W7 T3 / area-E `E15`): a test-only `ENV_TEST_LOCK` used to live
// here. It existed only because this crate's `desktop`/`xdg`/`homedir` tests
// mutated the PROCESS environment with `setenv`/`unsetenv` — which Zig's serial
// test runner tolerates and Rust's parallel `libtest` runner does not (`setenv`
// is not thread-safe against a concurrent `getenv` in glibc or Apple libc; see
// the recorded `parallel-cargo-test-exposes-c-init-races` lesson). Upstream
// `e8525c0fd` + `f8e13f31e` rewrote those tests to build LOCAL environment maps
// and deleted every `setenv`/`unsetenv` call, so the hazard is gone at the
// source and the lock is deleted with it rather than kept as a workaround.

#[cfg(test)]
mod tests {
    #[test]
    fn main_test_refs() {
        // PORT NOTE: the reference's `os/main.zig` `test {}` block forces the test
        // analysis of the named submodules:
        //     _ = file; _ = stderr; _ = edit; _ = i18n; _ = path; _ = uri; _ = shell;
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
        let _ = stringify!(stderr);
        let _ = stringify!(edit);
        let _ = stringify!(i18n);
        let _ = stringify!(path);
        let _ = stringify!(uri);
        let _ = stringify!(shell);
    }
}
