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
// PORT NOTE: `pub fn defaultDir(alloc: Allocator) !Dir` — the allocator parameter
// collapses (Rust's String owns/frees its allocation; break #1 keeps
// rustty-global injected, not a hard dep). Error set = the xdg lookup's.
pub fn defaultDir() -> Result<Dir, internal_os::xdg::DirError> {
    // const crash_dir = try internal_os.xdg.state(alloc, .{ .subdir = "ghostty/crash" });
    // (R2: the app-name segment of the `"ghostty/crash"` subdir is sourced from
    // rustty-identity, the identity source-of-truth crate, not hardcoded.)
    let subdir = format!("{}/crash", rustty_identity::APP_NAME);
    let crash_dir = internal_os::xdg::state(internal_os::xdg::Options {
        subdir: Some(&subdir),
        ..Default::default()
    })?;
    // errdefer alloc.free(crash_dir); — ownership/Drop frees on every path.
    // return .{ .path = crash_dir };
    Ok(Dir { path: crash_dir })
}

pub struct Dir {
    /// The directory where crash reports are stored. This memory is owned
    /// by the caller.
    pub path: String,
}

impl Dir {
    /// Returns an iterator over the crash reports in this directory. This
    /// iterator must be freed with `ReportIterator.deinit`. The iterator
    /// may have no reports.
    pub fn iterator(&self) -> std::io::Result<ReportIterator> {
        // var dir = std.fs.openDirAbsolute(
        //     self.path,
        //     .{ .iterate = true },
        // ) catch return .{};
        let dir = match std::fs::read_dir(&self.path) {
            Ok(dir) => dir,
            Err(_) => return Ok(ReportIterator::default()),
        };
        // errdefer dir.close(); — Drop closes the handle on every path.

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
        // if (self.dir) |dir| dir.close();
        self.dir = None;
    }

    // PORT NOTE: `pub fn next(self: *ReportIterator) !?Report` — an inherent `next`
    // (NOT Iterator::next; the fallible `!?Report` shape doesn't fit the trait).
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
            // const entry = try self.it.next() orelse return null;
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

        // const stat = try dir.statFile(entry.name);
        let stat = std::fs::metadata(entry.path())?;
        Ok(Some(Report {
            // PORT NOTE: the reference's `entry.name` is a borrow into the iterator
            // (invalidated by the next `next()`); Rust owns the bytes.
            name: entry.file_name().into_encoded_bytes(),
            // PORT NOTE: `stat.mtime` — Zig reports nanoseconds since the epoch as
            // an i128 (negative for pre-epoch times).
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

// PORT NOTE: helper: Zig `std.fs.Dir.statFile(...).mtime` is signed nanoseconds
// since the Unix epoch; `SystemTime` is converted to the same representation.
fn mtime_nanos(mtime: std::time::SystemTime) -> i128 {
    match mtime.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_nanos() as i128,
        Err(e) => -(e.duration().as_nanos() as i128),
    }
}
