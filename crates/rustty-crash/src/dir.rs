//! rustty-crash::dir
//!
//! PORTS: ghostty src/crash/dir.zig — the crash-report directory (`defaultDir`)
//! and its report iterator. 0 reference tests.
//!
//! PORT STATUS: done
#![allow(non_snake_case)]

// const std = @import("std");
// const assert = std.debug.assert;
// const Allocator = std.mem.Allocator;
// const internal_os = @import("../os/main.zig");
use rustty_os as internal_os;

/// Returns a Dir for the default directory. The Dir.path field must be
/// freed with the given allocator.
// PORT NOTE (P4-E33, `15c50c1db` "crash: do not use global state"):
//   pub fn defaultDir(io: std.Io, alloc: Allocator, environ_map: *const std.process.Environ.Map) !Dir
// The `environ_map` parameter is the semantic core: the CALLER supplies the
// environment snapshot; this file no longer imports `global` and performs no
// live environment reads. The `io: std.Io` parameter is N-A (Rust std does I/O
// without a provider handle) and the allocator parameter collapses (Rust's
// String owns/frees its allocation). Error set = the xdg lookup's.
pub fn defaultDir(environ_map: &rustty_global::EnvMap) -> Result<Dir, internal_os::xdg::DirError> {
    // const crash_dir = try internal_os.xdg.state(io, alloc, environ_map, .{ .subdir = "ghostty/crash" });
    // (R2: the app-name segment of the `"ghostty/crash"` subdir is sourced from
    // rustty-identity, the identity source-of-truth crate, not hardcoded.)
    let subdir = format!("{}/crash", rustty_identity::APP_NAME);
    // (W94-B02-01: `xdg::state` is byte-typed — raw path bytes in and out, so
    // a non-UTF-8 `XDG_STATE_HOME` is honored as in the reference.)
    let crash_dir = internal_os::xdg::state(
        environ_map,
        internal_os::xdg::Options {
            subdir: Some(subdir.as_bytes()),
            ..Default::default()
        },
    )?;
    // errdefer alloc.free(crash_dir); — ownership/Drop frees on every path.
    // return .{ .path = crash_dir };
    Ok(Dir { path: crash_dir })
}

pub struct Dir {
    /// The directory where crash reports are stored. This memory is owned
    /// by the caller.
    // PORT NOTE (W94-B02-01): `path: []const u8` — raw path bytes (R1
    // byte-slice rule), not a UTF-8 `String`.
    pub path: Vec<u8>,
}

impl Dir {
    /// Returns an iterator over the crash reports in this directory. This
    /// iterator must be freed with `ReportIterator.deinit`. The iterator
    /// may have no reports.
    // PORT NOTE (P4-E33, `15c50c1db`): `iterator`/`deinit`/`next` grew a
    // `io: std.Io` parameter upstream (previously `global.io()`); N-A here —
    // Rust std does I/O without a provider handle, so no global read to remove.
    pub fn iterator(&self) -> std::io::Result<ReportIterator> {
        // var dir = std.Io.Dir.openDirAbsolute(
        //     io,
        //     self.path,
        //     .{ .iterate = true },
        // ) catch return .{};
        let dir = match std::fs::read_dir(os_str_from_bytes(&self.path)) {
            Ok(dir) => dir,
            Err(_) => return Ok(ReportIterator::default()),
        };
        // errdefer dir.close(io); — Drop closes the handle on every path.

        // return .{
        //     .dir = dir,
        //     .it = dir.iterate(),
        // };
        Ok(ReportIterator { dir: Some(dir) })
    }
}

#[derive(Default)]
pub struct ReportIterator {
    // PORT NOTE: the reference splits the open handle (`dir: ?std.fs.Dir = null`)
    // from its iterator (`it: std.fs.Dir.Iterator = undefined`); Rust's
    // `ReadDir` is both, so one Option field carries the pair.
    pub dir: Option<std::fs::ReadDir>,
}

impl ReportIterator {
    pub fn deinit(&mut self) {
        // if (self.dir) |dir| dir.close(io);
        self.dir = None;
    }

    // PORT NOTE: `pub fn next(self: *ReportIterator, io: std.Io) !?Report` — an
    // inherent `next` (NOT Iterator::next; the fallible `!?Report` shape doesn't
    // fit the trait). The `io` parameter is N-A (see `iterator` above).
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> std::io::Result<Option<Report>> {
        // If we have no dir then we failed to open the directory.
        // const dir = self.dir orelse return null;
        let dir = match self.dir.as_mut() {
            Some(dir) => dir,
            None => return Ok(None),
        };

        // Get the next file entry, if any.
        let entry = loop {
            // const entry = try self.it.next(io) orelse return null;
            let entry = match dir.next() {
                Some(entry) => entry?,
                None => return Ok(None),
            };
            // if (entry.kind != .file) continue;
            if !entry.file_type()?.is_file() {
                continue;
            }
            break entry;
        };

        // const stat = try dir.statFile(io, entry.name, .{});
        let stat = std::fs::metadata(entry.path())?;
        Ok(Some(Report {
            // PORT NOTE: the reference's `entry.name` is a borrow into the iterator
            // (invalidated by the next `next()`); Rust owns the bytes.
            name: entry.file_name().into_encoded_bytes(),
            // PORT NOTE: `stat.mtime.toNanoseconds()` — Zig reports nanoseconds
            // since the epoch as an i128 (negative for pre-epoch times).
            mtime: mtime_nanos(stat.modified()?),
        }))
    }
}

pub struct Report {
    // PORT NOTE: `name: []const u8` -> owned bytes (R1 byte-slice rule; file names
    // are raw bytes on POSIX).
    pub name: Vec<u8>,
    pub mtime: i128,
}

/// Convert raw path bytes to an `OsStr` (W94-B02-01: the byte-typed XDG paths
/// map losslessly to POSIX `OsStr`s).
#[cfg(unix)]
pub(crate) fn os_str_from_bytes(bytes: &[u8]) -> &std::ffi::OsStr {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::OsStr::from_bytes(bytes)
}

#[cfg(not(unix))]
pub(crate) fn os_str_from_bytes(bytes: &[u8]) -> &std::ffi::OsStr {
    std::ffi::OsStr::new(std::str::from_utf8(bytes).unwrap_or(""))
}

// PORT NOTE: helper: Zig `std.Io.Dir.statFile(...).mtime.toNanoseconds()` is
// signed nanoseconds since the Unix epoch; `SystemTime` is converted to the
// same representation.
fn mtime_nanos(mtime: std::time::SystemTime) -> i128 {
    match mtime.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_nanos() as i128,
        Err(e) => -(e.duration().as_nanos() as i128),
    }
}
