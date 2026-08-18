//! rustty-config::edit
//!
//! PORTS: ghostty src/config/edit.zig — resolve/create the config file to open
//! for editing (`openPath` + private path helpers). C1 leaf value-type (0 tests).
//!
//! PORT STATUS: ported (every line/comment, R1).

// const std = @import("std");
// const builtin = @import("builtin");
// const assert = @import("../quirks.zig").inlineAssert;
// const Allocator = std.mem.Allocator;
// const ArenaAllocator = std.heap.ArenaAllocator;
// const file_load = @import("file_load.zig");
use crate::file_load::{self, FileLoadError};
use rustty_util::quirks::inlineAssert as assert;

/// The path to the configuration that should be opened for editing.
///
/// On Linux, this will use the file at the XDG config path. This is the
/// only valid path for Linux so we don't need to check for other paths.
///
/// On macOS, both XDG and AppSupport paths are valid. Because Ghostty
/// prioritizes AppSupport over XDG, we will use AppSupport if it exists,
/// followed by XDG if it exists, and finally AppSupport if neither exist.
/// For the existence check, we also prefer non-empty files over empty
/// files.
///
/// The returned value is allocated using the provided allocator.
// pub fn openPath(alloc_gpa: Allocator) ![:0]const u8 {
pub fn open_path() -> Result<Vec<u8>, FileLoadError> {
    // Use an arena to make memory management easier in here.
    // var arena = ArenaAllocator.init(alloc_gpa);
    // defer arena.deinit();
    // const alloc_arena = arena.allocator();
    //
    // PORT NOTE: the reference uses an arena purely for scratch-allocation
    // ergonomics, dupe-ing the final result into `alloc_gpa`. Rust's owned
    // `String`/`Vec` manage their own memory, so the arena is unnecessary; the
    // final path is returned as an owned `Vec<u8>`.

    // Get the path we should open
    // const config_path = try configPath(alloc_arena);
    let config_path = config_path()?;

    // if (!config_path.exists) {
    if !config_path.exists {
        // if (std.fs.path.dirname(config_path.name)) |config_dir| check_dir: {
        if let Some(config_dir) = dirname(&config_path.name) {
            'check_dir: {
                // Check to see if dir exists.
                // const dir = std.Io.Dir.cwd().openDir(global.io(), config_dir, .{ .follow_symlinks = true }) catch |err| {
                //     switch (err) {
                //         error.FileNotFound => { ... },
                //         else => return err,
                //     }
                // };
                //
                // PORT NOTE: `openDir(..., .{ .follow_symlinks = true })` maps to
                // `std::fs::read_dir` — it follows symlinks, fails with NotFound
                // when the directory is absent, and dropping the handle is the
                // `dir.close(global.io())`.
                let dir = match std::fs::read_dir(os_str_from_bytes(config_dir)) {
                    Ok(dir) => dir,
                    Err(err) => match err.kind() {
                        std::io::ErrorKind::NotFound => {
                            // Create config directory recursively. Note that this does not
                            // allow intermediate symlinks by design, see
                            // std.Io.Threaded.dirCreateDirPath for why. If some sort of
                            // complex symlink structure is needed, it will need to be created
                            // manually.
                            //
                            // PORT NOTE: `create_dir_all` does follow intermediate
                            // symlinks (Rust's stdlib has no non-following recursive
                            // create); the comment above records the reference's
                            // `dirCreateDirPath` constraint.
                            // try std.Io.Dir.cwd().createDirPath(global.io(), config_dir);
                            std::fs::create_dir_all(os_str_from_bytes(config_dir))?;
                            // break :check_dir;
                            break 'check_dir;
                        }
                        _ => return Err(FileLoadError::Io(err)),
                    },
                };
                // dir.close(global.io());
                drop(dir);
            }
        }

        // Try to create file and go on if it already exists
        // _ = std.Io.Dir.createFileAbsolute(
        //     global.io(),
        //     config_path.name,
        //     .{ .exclusive = true },
        // ) catch |err| {
        //     switch (err) {
        //         error.PathAlreadyExists => {},
        //         else => return err,
        //     }
        // };
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(os_str_from_bytes(&config_path.name))
        {
            Ok(_) => {}
            Err(err) => match err.kind() {
                std::io::ErrorKind::AlreadyExists => {}
                _ => return Err(FileLoadError::Io(err)),
            },
        }
    }

    // return try alloc_gpa.dupeZ(u8, config_path.name);
    Ok(config_path.name)
}

// const ConfigPathResult = struct {
//     name: []const u8,
//     exists: bool,
// };
struct ConfigPathResult {
    name: Vec<u8>,
    exists: bool,
}

/// Returns the config path to use for open for the current OS.
///
/// The allocator must be an arena allocator. No memory is freed by this
/// function and the resulting path is not all the memory that is allocated.
// fn configPath(alloc_arena: Allocator) !ConfigPathResult {
fn config_path() -> Result<ConfigPathResult, FileLoadError> {
    // const paths: []const []const u8 = try configPathCandidates(alloc_arena);
    let paths = config_path_candidates()?;
    // assert(paths.len > 0);
    assert(!paths.is_empty());

    // Find the first path that exists and is non-empty. If no paths are
    // non-empty but at least one exists, we will return the first path that
    // exists.
    // var exists: ?[]const u8 = null;
    let mut exists: Option<Vec<u8>> = None;
    // for (paths) |path| {
    for path in &paths {
        // const f = std.fs.openFileAbsolute(path, .{}) catch |err| {
        //     switch (err) {
        //         // File doesn't exist, continue.
        //         error.BadPathName, error.FileNotFound => continue,
        //
        //         // Some other error, assume it exists and return it.
        //         else => return err,
        //     }
        // };
        // defer f.close();
        let f = match std::fs::File::open(os_str_from_bytes(path)) {
            Ok(f) => f,
            Err(err) => match err.kind() {
                // File doesn't exist, continue.
                std::io::ErrorKind::NotFound => continue,
                // Some other error, assume it exists and return it.
                //
                // PORT NOTE: the reference also treats `error.BadPathName` as
                // "doesn't exist, continue"; Rust's `io::ErrorKind` does not
                // distinguish a bad path name from other errors portably, so the
                // non-NotFound branch returns the error as the reference's `else`
                // arm does. (`BadPathName` is a Windows-only consideration here.)
                _ => return Err(FileLoadError::Io(err)),
            },
        };

        // We expect stat to succeed because we just opened the file.
        // const stat = try f.stat();
        let stat = f.metadata()?;

        // If the file is non-empty, return it.
        // if (stat.size > 0) return .{
        //     .name = path,
        //     .exists = true,
        // };
        if stat.len() > 0 {
            return Ok(ConfigPathResult {
                name: path.clone(),
                exists: true,
            });
        }

        // If the file is empty, remember it exists.
        // if (exists == null) exists = path;
        if exists.is_none() {
            exists = Some(path.clone());
        }
    }

    // No paths are non-empty, return the first path that exists.
    // if (exists) |v| return .{
    //     .name = v,
    //     .exists = true,
    // };
    if let Some(v) = exists {
        return Ok(ConfigPathResult {
            name: v,
            exists: true,
        });
    }

    // No paths are non-empty or exist, return the first path.
    // return .{
    //     .name = paths[0],
    //     .exists = false,
    // };
    Ok(ConfigPathResult {
        name: paths[0].clone(),
        exists: false,
    })
}

/// Returns a const list of possible paths the main config file could be
/// in for the current OS.
// fn configPathCandidates(alloc_arena: Allocator) ![]const []const u8 {
fn config_path_candidates() -> Result<Vec<Vec<u8>>, FileLoadError> {
    // var paths: std.ArrayList([]const u8) = try .initCapacity(alloc_arena, 4);
    // errdefer paths.deinit(alloc_arena);
    let mut paths: Vec<Vec<u8>> = Vec::with_capacity(4);

    // if (comptime builtin.os.tag == .macos) {
    //     paths.appendAssumeCapacity(try file_load.defaultAppSupportPath(alloc_arena));
    //     paths.appendAssumeCapacity(try file_load.legacyDefaultAppSupportPath(alloc_arena));
    // }
    #[cfg(target_os = "macos")]
    {
        paths.push(file_load::default_app_support_path()?.into_bytes());
        paths.push(file_load::legacy_default_app_support_path()?.into_bytes());
    }

    // paths.appendAssumeCapacity(try file_load.defaultXdgPath(alloc_arena));
    // (W94-B02-01: the XDG helpers are byte-typed now — no `into_bytes`.)
    paths.push(file_load::default_xdg_path()?);
    // paths.appendAssumeCapacity(try file_load.legacyDefaultXdgPath(alloc_arena));
    paths.push(file_load::legacy_default_xdg_path()?);

    // return paths.items;
    Ok(paths)
}

/// PORTS: `std.fs.path.dirname(path)` for POSIX — the parent directory of
/// `path`, or `None` if there is none.
fn dirname(path: &[u8]) -> Option<&[u8]> {
    // Find the last `/` and return the prefix before it (matching POSIX
    // dirname semantics used by std.fs.path.dirname).
    let idx = path.iter().rposition(|&b| b == b'/')?;
    if idx == 0 {
        // Root-only: dirname of "/foo" is "/".
        return Some(&path[..1]);
    }
    Some(&path[..idx])
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
