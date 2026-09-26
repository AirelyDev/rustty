//! rustty-config::file_load
//!
//! PORTS: ghostty src/config/file_load.zig — config-file path resolution
//! (XDG + macOS AppSupport, current + legacy). C1 leaf value-type (0 tests).
//!
//! PORT STATUS: ported (every line/comment, R1).

// const std = @import("std");
// const builtin = @import("builtin");
// const assert = @import("../quirks.zig").inlineAssert;
// const Allocator = std.mem.Allocator;
// const internal_os = @import("../os/main.zig");
use crate::path::is_absolute;
use rustty_util::quirks::inlineAssert as assert;

// const log = std.log.scoped(.config);

/// PORTS: the error set shared by these path helpers — the union of the XDG
/// (`xdg.DirError`) and macOS AppSupport (`macos.AppSupportDirError`) errors plus
/// the `std.fs.File` open errors used by `preferred*`/`open`.
//
// PORT NOTE: the reference functions return `![]const u8` (open error sets). We
// model the cross-cutting error as `FileLoadError`, mapping the underlying
// `xdg.DirError`/`AppSupportDirError`/`std.io::Error` into one enum.
#[derive(Debug)]
pub enum FileLoadError {
    NoHomeDir,
    /// `xdg.DirError`'s `error.BufferTooSmall` (from `homedir.home`): `$HOME`
    /// is longer than the XDG lookup's 1024-byte buffer (LEDGER S112-F11).
    BufferTooSmall,
    /// `xdg.DirError`'s `error.StreamTooLong` (from `homedir.home`'s shell
    /// fallback; `try` passes it on).
    StreamTooLong,
    /// `xdg.DirError`'s spawn/read/wait failure of `homedir.home`'s shell
    /// fallback.
    Spawn(std::io::ErrorKind),
    AppleApiFailed,
    Io(std::io::Error),
}

// WS14 T6 (wasm32 arm): a browser has no filesystem, no XDG base dirs, no user
// database and no home directory, and the reference's own wasm build never
// reaches this path (Zig analyses `config.zig.Wasm` LAZILY, so it never forces
// these decls; Rust compiles whole crates, so the arm must exist). The wasm arm
// takes the SAME branch this code already takes when the lookup fails.
#[cfg(not(target_family = "wasm"))]
impl From<rustty_os::xdg::DirError> for FileLoadError {
    fn from(e: rustty_os::xdg::DirError) -> FileLoadError {
        match e {
            rustty_os::xdg::DirError::NoHomeDir => FileLoadError::NoHomeDir,
            rustty_os::xdg::DirError::BufferTooSmall => FileLoadError::BufferTooSmall,
            rustty_os::xdg::DirError::StreamTooLong => FileLoadError::StreamTooLong,
            rustty_os::xdg::DirError::Spawn(kind) => FileLoadError::Spawn(kind),
        }
    }
}

impl From<std::io::Error> for FileLoadError {
    fn from(e: std::io::Error) -> FileLoadError {
        FileLoadError::Io(e)
    }
}

/// Default path for the XDG home configuration file. Returned value
/// must be freed by the caller.
// pub fn defaultXdgPath(alloc: Allocator) ![]const u8 {
//     return try internal_os.xdg.config(
//         alloc,
//         .{ .subdir = "ghostty/config.ghostty" },
//     );
// }
//
// PORT NOTE (R2): the reference subdir literal `"ghostty/config.ghostty"` is
// the ghostty identity (config dir + config file name). Under rustty identity
// these come from `rustty_identity::{CONFIG_DIR, CONFIG_FILE}` (`rustty` /
// `config.rustty`), flipped back to ghostty's under the `identity-oracle`
// harness feature.
// PORT NOTE (W94-B02-01): the reference returns `![]const u8` — raw path
// bytes; `xdg::config` is byte-typed so a non-UTF-8 `XDG_CONFIG_HOME`
// participates in discovery exactly as in Ghostty.
pub fn default_xdg_path() -> Result<Vec<u8>, FileLoadError> {
    // WS14 T6 (wasm32 arm): no XDG base directory exists in a browser.
    #[cfg(target_family = "wasm")]
    {
        return Err(FileLoadError::NoHomeDir);
    }
    #[cfg(not(target_family = "wasm"))]
    {
        let subdir = format!(
            "{}/{}",
            rustty_identity::CONFIG_DIR,
            rustty_identity::CONFIG_FILE
        );
        Ok(rustty_os::xdg::config(
            &rustty_global::environ_map(),
            rustty_os::xdg::Options {
                subdir: Some(subdir.as_bytes()),
                ..Default::default()
            },
        )?)
    }
}

/// Ghostty <1.3.0 default path for the XDG home configuration file.
/// Returned value must be freed by the caller.
// pub fn legacyDefaultXdgPath(alloc: Allocator) ![]const u8 {
//     return try internal_os.xdg.config(
//         alloc,
//         .{ .subdir = "ghostty/config" },
//     );
// }
//
// PORT NOTE (R2): the legacy subdir literal `"ghostty/config"` -> `<CONFIG_DIR>/config`.
pub fn legacy_default_xdg_path() -> Result<Vec<u8>, FileLoadError> {
    // WS14 T6 (wasm32 arm): no XDG base directory exists in a browser.
    #[cfg(target_family = "wasm")]
    {
        return Err(FileLoadError::NoHomeDir);
    }
    #[cfg(not(target_family = "wasm"))]
    {
        let subdir = format!("{}/config", rustty_identity::CONFIG_DIR);
        Ok(rustty_os::xdg::config(
            &rustty_global::environ_map(),
            rustty_os::xdg::Options {
                subdir: Some(subdir.as_bytes()),
                ..Default::default()
            },
        )?)
    }
}

/// Preferred default path for the XDG home configuration file.
/// Returned value must be freed by the caller.
// pub fn preferredXdgPath(alloc: Allocator) ![]const u8 {
pub fn preferred_xdg_path() -> Result<Vec<u8>, FileLoadError> {
    // If the XDG path exists, use that.
    // const xdg_path = try defaultXdgPath(alloc);
    let xdg_path = default_xdg_path()?;
    // if (open(xdg_path)) |f| {
    //     f.close();
    //     return xdg_path;
    // } else |_| {}
    if let Ok(_f) = open(&xdg_path) {
        return Ok(xdg_path);
    }

    // Try the legacy path
    // errdefer alloc.free(xdg_path);
    // const legacy_xdg_path = try legacyDefaultXdgPath(alloc);
    let legacy_xdg_path = legacy_default_xdg_path()?;
    // if (open(legacy_xdg_path)) |f| {
    //     f.close();
    //     alloc.free(xdg_path);
    //     return legacy_xdg_path;
    // } else |_| {}
    if let Ok(_f) = open(&legacy_xdg_path) {
        return Ok(legacy_xdg_path);
    }

    // Legacy path and XDG path both don't exist. Return the
    // new one.
    // alloc.free(legacy_xdg_path);
    // return xdg_path;
    Ok(xdg_path)
}

/// Default path for the macOS Application Support configuration file.
/// Returned value must be freed by the caller.
// pub fn defaultAppSupportPath(alloc: Allocator) ![]const u8 {
//     return try internal_os.macos.appSupportDir(alloc, "config.ghostty");
// }
//
// PORT NOTE (R2): the literal `"config.ghostty"` -> `rustty_identity::CONFIG_FILE`.
#[cfg(target_os = "macos")]
pub fn default_app_support_path() -> Result<String, FileLoadError> {
    rustty_os::macos::appSupportDir(rustty_identity::CONFIG_FILE)
        .map_err(|_| FileLoadError::AppleApiFailed)
}

/// Ghostty <1.3.0 default path for the macOS Application Support
/// configuration file. Returned value must be freed by the caller.
// pub fn legacyDefaultAppSupportPath(alloc: Allocator) ![]const u8 {
//     return try internal_os.macos.appSupportDir(alloc, "config");
// }
#[cfg(target_os = "macos")]
pub fn legacy_default_app_support_path() -> Result<String, FileLoadError> {
    rustty_os::macos::appSupportDir("config").map_err(|_| FileLoadError::AppleApiFailed)
}

/// Preferred default path for the macOS Application Support configuration file.
/// Returned value must be freed by the caller.
// pub fn preferredAppSupportPath(alloc: Allocator) ![]const u8 {
#[cfg(target_os = "macos")]
pub fn preferred_app_support_path() -> Result<String, FileLoadError> {
    // If the app support path exists, use that.
    // const app_support_path = try defaultAppSupportPath(alloc);
    let app_support_path = default_app_support_path()?;
    // if (open(app_support_path)) |f| {
    //     f.close();
    //     return app_support_path;
    // } else |_| {}
    if let Ok(_f) = open(app_support_path.as_bytes()) {
        return Ok(app_support_path);
    }

    // Try the legacy path
    // errdefer alloc.free(app_support_path);
    // const legacy_app_support_path = try legacyDefaultAppSupportPath(alloc);
    let legacy_app_support_path = legacy_default_app_support_path()?;
    // if (open(legacy_app_support_path)) |f| {
    //     f.close();
    //     alloc.free(app_support_path);
    //     return legacy_app_support_path;
    // } else |_| {}
    if let Ok(_f) = open(legacy_app_support_path.as_bytes()) {
        return Ok(legacy_app_support_path);
    }

    // Legacy path and app support path both don't exist. Return the
    // new one.
    // alloc.free(legacy_app_support_path);
    // return app_support_path;
    Ok(app_support_path)
}

/// Returns the path to the preferred default configuration file.
/// This is the file where users should place their configuration.
///
/// This doesn't create or populate the file with any default
/// contents; downstream callers must handle this.
///
/// The returned value must be freed by the caller.
// pub fn preferredDefaultFilePath(alloc: Allocator) ![]const u8 {
//     switch (builtin.os.tag) {
//         .macos => {
//             // macOS prefers the Application Support directory
//             // if it exists.
//             const app_support_path = try preferredAppSupportPath(alloc);
//             const app_support_file = open(app_support_path) catch {
//                 // Try the XDG path if it exists
//                 const xdg_path = try preferredXdgPath(alloc);
//                 const xdg_file = open(xdg_path) catch {
//                     // If neither file exists, use app support
//                     alloc.free(xdg_path);
//                     return app_support_path;
//                 };
//                 xdg_file.close();
//                 alloc.free(app_support_path);
//                 return xdg_path;
//             };
//             app_support_file.close();
//             return app_support_path;
//         },
//
//         // All other platforms use XDG only
//         else => return try preferredXdgPath(alloc),
//     }
// }
// PORT NOTE (W94-B02-01): returns raw path bytes (`![]const u8` in the
// reference). The macOS Application Support paths are UTF-8 `String`s from the
// Apple APIs and convert losslessly via `into_bytes`.
pub fn preferred_default_file_path() -> Result<Vec<u8>, FileLoadError> {
    #[cfg(target_os = "macos")]
    {
        // macOS prefers the Application Support directory
        // if it exists.
        let app_support_path = preferred_app_support_path()?;
        match open(app_support_path.as_bytes()) {
            Ok(_app_support_file) => {
                // app_support_file.close();
                Ok(app_support_path.into_bytes())
            }
            Err(_) => {
                // Try the XDG path if it exists
                let xdg_path = preferred_xdg_path()?;
                match open(&xdg_path) {
                    Ok(_xdg_file) => {
                        // xdg_file.close();
                        Ok(xdg_path)
                    }
                    Err(_) => {
                        // If neither file exists, use app support
                        Ok(app_support_path.into_bytes())
                    }
                }
            }
        }
    }

    // All other platforms use XDG only
    #[cfg(not(target_os = "macos"))]
    {
        preferred_xdg_path()
    }
}

/// PORTS: `const OpenFileError = error{ ... }`.
// const OpenFileError = error{
//     FileNotFound,
//     FileIsEmpty,
//     FileOpenFailed,
//     NotAFile,
// };
#[derive(Debug, PartialEq, Eq)]
pub enum OpenFileError {
    FileNotFound,
    FileIsEmpty,
    FileOpenFailed,
    NotAFile,
}

/// Opens the file at the given path and returns the file handle
/// if it exists and is non-empty. This also constrains the possible
/// errors to a smaller set that we can explicitly handle.
// pub fn open(path: []const u8) OpenFileError!std.fs.File {
//
// PORT NOTE: `path` is raw bytes; on POSIX they map directly to an `OsStr`.
pub fn open(path: &[u8]) -> Result<std::fs::File, OpenFileError> {
    // assert(std.fs.path.isAbsolute(path));
    assert(is_absolute(path));

    // var file = std.fs.openFileAbsolute(path, .{}) catch |err| switch (err) {
    //     error.FileNotFound => return OpenFileError.FileNotFound,
    //     else => {
    //         log.warn("unexpected file open error path={s} err={}", .{ path, err });
    //         return OpenFileError.FileOpenFailed;
    //     },
    // };
    // errdefer file.close();
    let file = match std::fs::File::open(os_str_from_bytes(path)) {
        Ok(f) => f,
        Err(err) => match err.kind() {
            std::io::ErrorKind::NotFound => return Err(OpenFileError::FileNotFound),
            _ => {
                // log.warn("unexpected file open error path={s} err={}", .{
                //     path,
                //     err,
                // });
                crate::config_struct::log_warn(format!(
                    "unexpected file open error path={} err={err:?}",
                    String::from_utf8_lossy(path)
                ));
                return Err(OpenFileError::FileOpenFailed);
            }
        },
    };

    // const stat = file.stat() catch |err| {
    //     log.warn("error getting file stat path={s} err={}", .{ path, err });
    //     return OpenFileError.FileOpenFailed;
    // };
    let stat = match file.metadata() {
        Ok(m) => m,
        Err(err) => {
            // log.warn("error getting file stat path={s} err={}", .{
            //     path,
            //     err,
            // });
            crate::config_struct::log_warn(format!(
                "error getting file stat path={} err={err:?}",
                String::from_utf8_lossy(path)
            ));
            return Err(OpenFileError::FileOpenFailed);
        }
    };
    // switch (stat.kind) {
    //     .file => {},
    //     else => return OpenFileError.NotAFile,
    // }
    if !stat.is_file() {
        return Err(OpenFileError::NotAFile);
    }

    // if (stat.size == 0) return OpenFileError.FileIsEmpty;
    if stat.len() == 0 {
        return Err(OpenFileError::FileIsEmpty);
    }

    // return file;
    Ok(file)
}

// `std.fs.path.isAbsolute(path)` is the target-dispatched helper ported once in
// `crate::path` (imported at the top of this module).
//
// PORT NOTE: this file previously carried its own leading-`/`-only copy, so the
// `assert(std.fs.path.isAbsolute(path))` above would have fired on a Windows build
// for every drive-qualified or UNC config path (and the assert is ACTIVE in release
// — `inlineAssert`, not `debug_assert!`).

/// Convert raw path bytes to an `OsStr`.
#[cfg(unix)]
fn os_str_from_bytes(bytes: &[u8]) -> &std::ffi::OsStr {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::OsStr::from_bytes(bytes)
}

#[cfg(not(unix))]
fn os_str_from_bytes(bytes: &[u8]) -> &std::ffi::OsStr {
    std::ffi::OsStr::new(std::str::from_utf8(bytes).unwrap_or(""))
}

#[cfg(test)]
mod tests {
    use super::*;

    // No reference `test {}` — PROVING test for S78-F6. `file_load.open`'s two
    // `log.warn` calls (`config/file_load.zig:152-155` unexpected open error,
    // `:162-165` stat error) were carried as inert comments, so a config file
    // that exists but cannot be opened failed silently. The open-error arm is
    // the one reachable portably (a path whose PARENT is a regular file gives
    // `ENOTDIR`, which is not `FileNotFound`); both arms write through the same
    // `config_struct::log_warn` helper.
    #[test]
    fn open_warns_on_an_unexpected_open_error() {
        let mut file = std::env::temp_dir();
        file.push("rustty-s78f6-parent-is-a-file");
        std::fs::write(&file, b"x").unwrap();

        // `<regular file>/child` — ENOTDIR, i.e. an `else =>` open error.
        let canon = std::fs::canonicalize(&file).unwrap();
        let mut path: Vec<u8> = {
            #[cfg(unix)]
            {
                use std::os::unix::ffi::OsStrExt;
                canon.as_os_str().as_bytes().to_vec()
            }
            #[cfg(not(unix))]
            {
                canon.to_string_lossy().as_bytes().to_vec()
            }
        };
        path.extend_from_slice(b"/child");
        assert!(is_absolute(&path));

        // The exact `{err:?}` rendering is the platform's `io::Error` Debug;
        // take it from the same failing call rather than hard-coding an errno
        // spelling, so the assertion pins the LEVEL, PATH and MESSAGE SHAPE on
        // every target.
        let err = std::fs::File::open(os_str_from_bytes(&path)).unwrap_err();
        assert_ne!(err.kind(), std::io::ErrorKind::NotFound);

        crate::config_struct::log_capture::arm();
        let got = open(&path);
        let records = crate::config_struct::log_capture::take();

        assert!(matches!(got, Err(OpenFileError::FileOpenFailed)));
        assert_eq!(
            records,
            vec![format!(
                "warning(config): unexpected file open error path={} err={err:?}",
                String::from_utf8_lossy(&path)
            )],
        );

        std::fs::remove_file(&file).unwrap();
    }
}
