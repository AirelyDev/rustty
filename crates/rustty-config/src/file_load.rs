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
pub fn default_xdg_path() -> Result<String, FileLoadError> {
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
        Ok(rustty_os::xdg::config(rustty_os::xdg::Options {
            subdir: Some(&subdir),
            ..Default::default()
        })?)
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
pub fn legacy_default_xdg_path() -> Result<String, FileLoadError> {
    // WS14 T6 (wasm32 arm): no XDG base directory exists in a browser.
    #[cfg(target_family = "wasm")]
    {
        return Err(FileLoadError::NoHomeDir);
    }
    #[cfg(not(target_family = "wasm"))]
    {
        let subdir = format!("{}/config", rustty_identity::CONFIG_DIR);
        Ok(rustty_os::xdg::config(rustty_os::xdg::Options {
            subdir: Some(&subdir),
            ..Default::default()
        })?)
    }
}

/// Preferred default path for the XDG home configuration file.
/// Returned value must be freed by the caller.
// pub fn preferredXdgPath(alloc: Allocator) ![]const u8 {
pub fn preferred_xdg_path() -> Result<String, FileLoadError> {
    // If the XDG path exists, use that.
    // const xdg_path = try defaultXdgPath(alloc);
    let xdg_path = default_xdg_path()?;
    // if (open(xdg_path)) |f| {
    //     f.close();
    //     return xdg_path;
    // } else |_| {}
    if let Ok(_f) = open(xdg_path.as_bytes()) {
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
    if let Ok(_f) = open(legacy_xdg_path.as_bytes()) {
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
pub fn preferred_default_file_path() -> Result<String, FileLoadError> {
    #[cfg(target_os = "macos")]
    {
        // macOS prefers the Application Support directory
        // if it exists.
        let app_support_path = preferred_app_support_path()?;
        match open(app_support_path.as_bytes()) {
            Ok(_app_support_file) => {
                // app_support_file.close();
                Ok(app_support_path)
            }
            Err(_) => {
                // Try the XDG path if it exists
                let xdg_path = preferred_xdg_path()?;
                match open(xdg_path.as_bytes()) {
                    Ok(_xdg_file) => {
                        // xdg_file.close();
                        Ok(xdg_path)
                    }
                    Err(_) => {
                        // If neither file exists, use app support
                        Ok(app_support_path)
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
                // log.warn("unexpected file open error path={s} err={}", .{ path, err });
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
        Err(_err) => {
            // log.warn("error getting file stat path={s} err={}", .{ path, err });
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

/// PORTS: `std.fs.path.isAbsolute(path)` for POSIX.
fn is_absolute(path: &[u8]) -> bool {
    !path.is_empty() && path[0] == b'/'
}

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
