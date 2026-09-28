//! rustty-cli::ssh_cache::disk_cache
//!
//! PORTS: ghostty src/cli/ssh-cache/DiskCache.zig — the on-disk SSH terminfo
//! entry cache. Byte-faithful (R1); paths are byte strings (`Vec<u8>`, R3). The
//! program dir (`ghostty`) is routed through `rustty-identity` (R2 / ADV-FIX 5)
//! so the on-disk path tracks the Rustty product identity. Entries record the
//! caller-supplied content-derived terminfo `version` (`rustty_terminfo::version`
//! at the call sites), so a stale cache entry no longer suppresses a required
//! install after the terminfo source changes.
//!
//! PORT NOTE (memory model): the reference `std.StringHashMap(Entry)` stores the
//! hostname exactly ONCE and uses that same allocation as both the map key AND
//! `Entry.hostname` (hence the `key.ptr == value.hostname.ptr` asserts). Rust's
//! `HashMap<Vec<u8>, Entry>` owns the key and `Entry.hostname` as two separate
//! `Vec<u8>` with EQUAL CONTENT, and `Drop` frees each exactly once — so the
//! reference's manual `dupe`/`free`/`errdefer` dance (guarding against a partial
//! allocation leaving a half-built or double-freed slot) collapses to ownership,
//! and the pointer-identity asserts become content-equality asserts. Under Rust,
//! allocation is infallible (aborts), so `error.OutOfMemory` never surfaces; the
//! two `FailingAllocator` OOM-injection tests therefore run their single success
//! path (Miri, R6, is the UB/leak backstop for the pure logic).

// const DiskCache = @This();
//
// const std = @import("std");
// const builtin = @import("builtin");
// const assert = @import("../../quirks.zig").inlineAssert;
//   -> `debug_assert!` (the `inlineAssert` quirk is active in safe builds only).
// const Allocator = std.mem.Allocator;
//   -> dropped tree-wide (Rust global allocator); methods take no allocator arg.
// const internal_os = @import("../../os/main.zig");
// const xdg = internal_os.xdg;
use rustty_os::xdg;
// const Entry = @import("Entry.zig");
use super::entry::Entry;

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

// 512KB - sufficient for approximately 10k entries
// const MAX_CACHE_SIZE = 512 * 1024;
const MAX_CACHE_SIZE: usize = 512 * 1024;

/// The error set produced by the disk cache operations. The reference returns an
/// inferred error union; the port aggregates the observable failures: the
/// explicit `InvalidCacheKey`/`CacheLocked`/`InvalidCachePath`/`XdgLookupFailed`
/// cases plus the underlying filesystem I/O errors.
#[derive(Debug)]
pub enum Error {
    /// `error.InvalidCacheKey` — the key isn't a valid hostname / user@hostname.
    InvalidCacheKey,
    /// `error.CacheLocked` — another process holds the exclusive cache lock.
    CacheLocked,
    /// `error.InvalidCachePath` — the cache path has no directory component.
    InvalidCachePath,
    /// `error.XdgLookupFailed` — the XDG state directory couldn't be resolved.
    XdgLookupFailed,
    /// An underlying filesystem I/O error.
    Io(std::io::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::InvalidCacheKey => write!(f, "InvalidCacheKey"),
            Error::CacheLocked => write!(f, "CacheLocked"),
            Error::InvalidCachePath => write!(f, "InvalidCachePath"),
            Error::XdgLookupFailed => write!(f, "XdgLookupFailed"),
            Error::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

/// Propagate a disk-cache error out of the cli action `run`s (whose faithful
/// error type is I/O-shaped): the underlying I/O error passes through, and the
/// explicit variants wrap into an `other` error carrying their name.
impl From<Error> for std::io::Error {
    fn from(e: Error) -> Self {
        match e {
            Error::Io(io) => io,
            other => std::io::Error::other(other.to_string()),
        }
    }
}

/// An SSH terminfo entry cache that stores its cache data on
/// disk. The cache only stores metadata (hostname, terminfo value,
/// etc.) and does not store any sensitive data.
pub struct DiskCache {
    /// Path to a file where the cache is stored.
    // path: []const u8,
    pub path: Vec<u8>,
}

impl DiskCache {
    /// Returns the default path for the cache for a given program.
    ///
    /// On all platforms, this is `${XDG_STATE_HOME}/ghostty/ssh_cache`.
    ///
    /// The returned value is allocated and must be freed by the caller.
    //
    // PORT NOTE: the reference `program: []const u8` is a UTF-8 product-identity
    // string; the callers pass the app name, routed through rustty-identity (R2).
    // pub fn defaultPath(alloc: Allocator, program: []const u8) ![]const u8 {
    pub fn default_path(program: &str) -> Result<Vec<u8>, Error> {
        // var environ_map = try global.environMap();
        // defer environ_map.deinit();
        // const state_dir: []const u8 = xdg.state(
        //     global.io(),
        //     alloc,
        //     &environ_map,
        //     .{ .subdir = program },
        // ) catch |err| return switch (err) {
        //     error.OutOfMemory => error.OutOfMemory,
        //     else => error.XdgLookupFailed,
        // };
        // defer alloc.free(state_dir);
        // (W94-B02-01: `xdg::state` is byte-typed — raw path bytes in and out.)
        //   (the `global.io()` first param is the reference's `std.Io` provider
        //   threading, 15c50c1db — N-A in Rust, there is no such provider)
        let state_dir = xdg::state(
            &rustty_global::environ_map(),
            xdg::Options {
                subdir: Some(program.as_bytes()),
                ..Default::default()
            },
        )
        .map_err(|_| Error::XdgLookupFailed)?;
        // return try std.fs.path.join(alloc, &.{ state_dir, "ssh_cache" });
        Ok(join_path(&state_dir, b"ssh_cache"))
    }

    /// Clear all cache data stored in the disk cache.
    /// This removes the cache file from disk, effectively clearing all cached
    /// SSH terminfo entries.
    // pub fn clear(self: DiskCache) !void {
    pub fn clear(&self) -> Result<(), Error> {
        // std.fs.cwd().deleteFile(self.path) catch |err| switch (err) {
        //     error.FileNotFound => {},
        //     else => return err,
        // };
        match std::fs::remove_file(as_path(&self.path)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(Error::Io(e)),
        }
    }

    /// Add or update an entry in the cache, recording `timestamp` (Unix seconds).
    /// The cache file is created if it doesn't exist with secure permissions (0600).
    // pub fn add(self: DiskCache, alloc: Allocator, key: []const u8, version: []const u8, timestamp: i64) !void {
    pub fn add(&self, key: &[u8], version: &[u8], timestamp: i64) -> Result<(), Error> {
        // if (!isValidCacheKey(key)) return error.InvalidCacheKey;
        if !Self::is_valid_cache_key(key) {
            return Err(Error::InvalidCacheKey);
        }

        // Create cache directory if needed
        // if (std.fs.path.dirname(self.path)) |dir| {
        //     std.fs.cwd().makePath(dir) catch |err| switch (err) {
        //         error.PathAlreadyExists => {},
        //         else => return err,
        //     };
        // }
        if let Some(dir) = path_dirname(&self.path) {
            // `create_dir_all` already treats an existing directory as success
            // (the reference's `PathAlreadyExists => {}`).
            std::fs::create_dir_all(as_path(dir)).map_err(Error::Io)?;
        }

        // Open or create cache file with secure permissions
        // const file = std.fs.createFileAbsolute(self.path, .{
        //     .read = true, .truncate = false, .mode = 0o600,
        // }) catch |err| switch (err) {
        //     error.PathAlreadyExists => blk: {
        //         const existing_file = try std.fs.openFileAbsolute(self.path, .{ .mode = .read_write });
        //         errdefer existing_file.close();
        //         try fixupPermissions(existing_file);
        //         break :blk existing_file;
        //     },
        //     else => return err,
        // };
        // defer file.close();
        //
        // CORRECTION (parity S71-F4, task T-C). The note here used to read: "the
        // port creates the file EXCLUSIVELY (`create_new`) so the 'already
        // exists -> open + fixup' branch is reachable exactly as written.
        // Observable behavior is identical". It was not: at the pin the call is
        // `std.Io.Dir.createFileAbsolute(io, path, .{ .read = true, .truncate =
        // false, .permissions = 0o600 })`, whose `exclusive` defaults to `false`
        // (Zig 0.16.0 `Io/Dir.zig` `CreateFileOptions`), i.e. a plain O_CREAT
        // open. An existing file is opened as it is, never chmod'ed here, so the
        // `PathAlreadyExists` arm is unreachable in the reference, and in the
        // port. Making it reachable added a chmod of an existing file (visible
        // when `add` then fails) and a create-then-open race. `file` is closed
        // on drop (the reference's `defer file.close()`).
        let file = match create_secure(as_path(&self.path)) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                let existing_file = open_read_write(as_path(&self.path)).map_err(Error::Io)?;
                fixup_permissions(&existing_file)?;
                existing_file
            }
            Err(e) => return Err(Error::Io(e)),
        };

        // Lock
        // Causes a compile failure in the Zig std library on Windows, see:
        // https://github.com/ziglang/zig/issues/18430
        // if (comptime builtin.os.tag != .windows) _ = file.tryLock(.exclusive) catch return error.CacheLocked;
        // defer if (comptime builtin.os.tag != .windows) file.unlock();
        let _lock = FlockGuard::acquire(&file)?;

        // var entries = try readEntries(alloc, file);
        // defer deinitEntries(alloc, &entries);
        let mut entries = read_entries(&file)?;

        // Update the timestamp of an existing entry, or insert a new one. For a
        // new entry, dupe both strings up front so a failed allocation never
        // leaves a half-built slot (borrowed key, undefined value) for the
        // `deinitEntries` defer to walk.
        // if (entries.getPtr(key)) |existing| {
        //     existing.timestamp = timestamp;
        //     const version_copy = try alloc.dupe(u8, version);
        //     alloc.free(existing.terminfo_version);
        //     existing.terminfo_version = version_copy;
        // } else {
        //     const key_copy = try alloc.dupe(u8, key);
        //     errdefer alloc.free(key_copy);
        //     const version_copy = try alloc.dupe(u8, version);
        //     errdefer alloc.free(version_copy);
        //     try entries.put(key_copy, .{
        //         .hostname = key_copy,
        //         .timestamp = timestamp,
        //         .terminfo_version = version_copy,
        //     });
        // }
        //   Ownership makes the dupe/errdefer bookkeeping implicit (the
        //   `Vec<u8>` copies below).
        if let Some(existing) = entries.get_mut(key) {
            existing.timestamp = timestamp;
            existing.terminfo_version = version.to_vec();
        } else {
            entries.insert(
                key.to_vec(),
                Entry {
                    hostname: key.to_vec(),
                    timestamp,
                    terminfo_version: version.to_vec(),
                },
            );
        }

        // try self.writeCacheFile(entries);
        self.write_cache_file(&entries)?;
        Ok(())
    }

    /// Remove an entry from the cache. Returns true if an entry was removed,
    /// false if the key wasn't present (or the cache file is missing).
    // pub fn remove(self: DiskCache, alloc: Allocator, key: []const u8) !bool {
    pub fn remove(&self, key: &[u8]) -> Result<bool, Error> {
        // if (!isValidCacheKey(key)) return error.InvalidCacheKey;
        if !Self::is_valid_cache_key(key) {
            return Err(Error::InvalidCacheKey);
        }

        // Open our file
        // const file = std.fs.openFileAbsolute(self.path, .{ .mode = .read_write }) catch |err| switch (err) {
        //     error.FileNotFound => return false,
        //     else => return err,
        // };
        // defer file.close();
        let file = match open_read_write(as_path(&self.path)) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(Error::Io(e)),
        };
        // try fixupPermissions(file);
        fixup_permissions(&file)?;

        // Lock (see the compile-failure note in `add`).
        // if (comptime builtin.os.tag != .windows) _ = file.tryLock(.exclusive) catch return error.CacheLocked;
        // defer if (comptime builtin.os.tag != .windows) file.unlock();
        let _lock = FlockGuard::acquire(&file)?;

        // Read existing entries
        // var entries = try readEntries(alloc, file);
        // defer deinitEntries(alloc, &entries);
        let mut entries = read_entries(&file)?;

        // Remove the entry if it exists and ensure we free the memory
        // const removed = if (entries.fetchRemove(key)) |kv| removed: {
        //     assert(kv.key.ptr == kv.value.hostname.ptr);
        //     alloc.free(kv.value.hostname);
        //     alloc.free(kv.value.terminfo_version);
        //     break :removed true;
        // } else false;
        let removed = match entries.remove_entry(key) {
            Some((k, v)) => {
                debug_assert_eq!(k.as_slice(), v.hostname.as_slice());
                // (`k`/`v` are freed as they drop here.)
                true
            }
            None => false,
        };

        // try self.writeCacheFile(entries);
        self.write_cache_file(&entries)?;
        // return removed;
        Ok(removed)
    }

    /// Remove all entries older than `max_age_s` seconds and return how many
    /// were pruned. Returns zero (and nothing written) if the cache file is
    /// missing.
    // pub fn prune(self: DiskCache, alloc: Allocator, max_age_s: u64) !usize {
    pub fn prune(&self, max_age_s: u64) -> Result<usize, Error> {
        // const file = std.fs.openFileAbsolute(self.path, .{ .mode = .read_write }) catch |err| switch (err) {
        //     error.FileNotFound => return 0,
        //     else => return err,
        // };
        // defer file.close();
        let file = match open_read_write(as_path(&self.path)) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(e) => return Err(Error::Io(e)),
        };
        // try fixupPermissions(file);
        fixup_permissions(&file)?;

        // Lock (see the compile-failure note in `add`).
        // if (comptime builtin.os.tag != .windows) _ = file.tryLock(.exclusive) catch return error.CacheLocked;
        // defer if (comptime builtin.os.tag != .windows) file.unlock();
        let _lock = FlockGuard::acquire(&file)?;

        // Read existing entries
        // var entries = try readEntries(alloc, file);
        // defer deinitEntries(alloc, &entries);
        let mut entries = read_entries(&file)?;

        // Drop expired entries from the map, then persist what remains.
        // const now = std.time.timestamp();
        let now = timestamp();
        // var expired: std.ArrayList([]const u8) = .empty;
        // defer expired.deinit(alloc);
        let mut expired: Vec<Vec<u8>> = Vec::new();
        // var iter = entries.iterator();
        // while (iter.next()) |kv| {
        //     const age_s = now -| kv.value_ptr.timestamp;
        //     if (age_s > max_age_s) try expired.append(alloc, kv.key_ptr.*);
        // }
        for (key, value) in entries.iter() {
            // `now -| timestamp` is saturating i64 subtraction; the reference
            // compares that (possibly negative) age against `max_age_s: u64`
            // mathematically, so a future timestamp (negative age) is never
            // "expired".
            let age_s = now.saturating_sub(value.timestamp);
            let is_expired = age_s >= 0 && (age_s as u64) > max_age_s;
            if is_expired {
                expired.push(key.clone());
            }
        }
        // for (expired.items) |key| {
        //     const kv = entries.fetchRemove(key).?;
        //     assert(kv.key.ptr == kv.value.hostname.ptr);
        //     alloc.free(kv.value.hostname);
        //     alloc.free(kv.value.terminfo_version);
        // }
        for key in &expired {
            let (k, v) = entries.remove_entry(key.as_slice()).unwrap();
            debug_assert_eq!(k.as_slice(), v.hostname.as_slice());
            // (`k`/`v` are freed as they drop here.)
        }

        // try self.writeCacheFile(entries);
        self.write_cache_file(&entries)?;
        // return expired.items.len;
        Ok(expired.len())
    }

    /// Check if a key with `version` exists in the cache.
    /// Returns false if the cache file doesn't exist or the version doesn't match.
    // pub fn contains(self: DiskCache, alloc: Allocator, key: []const u8, version: []const u8) !bool {
    pub fn contains(&self, key: &[u8], version: &[u8]) -> Result<bool, Error> {
        // if (!isValidCacheKey(key)) return error.InvalidCacheKey;
        if !Self::is_valid_cache_key(key) {
            return Err(Error::InvalidCacheKey);
        }

        // Open our file
        // const file = std.fs.openFileAbsolute(self.path, .{}) catch |err| switch (err) {
        //     error.FileNotFound => return false,
        //     else => return err,
        // };
        // defer file.close();
        let file = match open_read_only(as_path(&self.path)) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(Error::Io(e)),
        };

        // Read existing entries
        // var entries = try readEntries(alloc, file);
        // defer deinitEntries(alloc, &entries);
        let entries = read_entries(&file)?;

        // const entry = entries.get(key) orelse return false;
        let entry = match entries.get(key) {
            Some(e) => e,
            None => return Ok(false),
        };
        // return std.mem.eql(u8, entry.terminfo_version, version);
        Ok(entry.terminfo_version == version)
    }

    // fn writeCacheFile(self: DiskCache, entries: std.StringHashMap(Entry)) !void {
    fn write_cache_file(&self, entries: &HashMap<Vec<u8>, Entry>) -> Result<(), Error> {
        // const cache_dir = std.fs.path.dirname(self.path) orelse return error.InvalidCachePath;
        let cache_dir = path_dirname(&self.path).ok_or(Error::InvalidCachePath)?;
        // const cache_basename = std.fs.path.basename(self.path);
        let cache_basename = path_basename(&self.path);

        // var dir = try std.fs.cwd().openDir(cache_dir, .{});
        // defer dir.close();
        //   (The Rust port operates on paths directly; no persistent Dir handle.)

        // var buf: [1024]u8 = undefined;
        // var atomic_file = try dir.atomicFile(cache_basename, .{ .mode = 0o600, .write_buffer = &buf });
        // defer atomic_file.deinit();
        //
        // var iter = entries.iterator();
        // while (iter.next()) |kv| {
        //     try kv.value_ptr.format(&atomic_file.file_writer.interface);
        // }
        // try atomic_file.finish();
        //
        // PORT NOTE: `dir.atomicFile(name)` writes to a temp sibling then renames
        // it over `name`; `deinit` removes the temp if `finish` wasn't reached.
        // Reproduced here: build the content, write it to a random temp file in
        // `cache_dir` (created 0600), then rename it over `cache_dir/cache_basename`
        // (== self.path). On any failure the temp is removed (atomicFile.deinit).
        let mut content: Vec<u8> = Vec::new();
        for value in entries.values() {
            value.format(&mut content).map_err(Error::Io)?;
        }

        let tmp_path = atomic_temp_path(cache_dir)?;
        let target = join_path(cache_dir, cache_basename);
        let result = (|| -> Result<(), Error> {
            let mut tmp_file = create_new_secure(as_path(&tmp_path)).map_err(Error::Io)?;
            tmp_file.write_all(&content).map_err(Error::Io)?;
            tmp_file.sync_all().map_err(Error::Io)?;
            drop(tmp_file);
            std::fs::rename(as_path(&tmp_path), as_path(&target)).map_err(Error::Io)?;
            Ok(())
        })();
        if result.is_err() {
            // atomic_file.deinit(): remove the temp file if we didn't finish.
            let _ = std::fs::remove_file(as_path(&tmp_path));
        }
        result
    }

    /// List all entries in the cache.
    /// The returned HashMap must be freed using `deinitEntries`.
    /// Returns an empty map if the cache file doesn't exist.
    // pub fn list(self: DiskCache, alloc: Allocator) !std.StringHashMap(Entry) {
    pub fn list(&self) -> Result<HashMap<Vec<u8>, Entry>, Error> {
        // Open our file
        // const file = std.fs.openFileAbsolute(self.path, .{}) catch |err| switch (err) {
        //     error.FileNotFound => return .init(alloc),
        //     else => return err,
        // };
        // defer file.close();
        let file = match open_read_only(as_path(&self.path)) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(HashMap::new()),
            Err(e) => return Err(Error::Io(e)),
        };
        // return readEntries(alloc, file);
        read_entries(&file)
    }

    /// Free memory allocated by the `list` function.
    /// This must be called to properly deallocate all entry data.
    // pub fn deinitEntries(alloc: Allocator, entries: *std.StringHashMap(Entry)) void {
    pub fn deinit_entries(entries: &mut HashMap<Vec<u8>, Entry>) {
        // All our entries we dupe the memory owned by the hostname and the
        // terminfo, and we always match the hostname key and value.
        // var it = entries.iterator();
        // while (it.next()) |entry| {
        //     assert(entry.key_ptr.*.ptr == entry.value_ptr.hostname.ptr);
        //     alloc.free(entry.value_ptr.hostname);
        //     alloc.free(entry.value_ptr.terminfo_version);
        // }
        // entries.deinit();
        //
        // PORT NOTE: ownership frees each entry's bytes on `clear`; the reference's
        // per-entry `alloc.free` becomes the `HashMap`'s drop of its owned values.
        // The pointer-identity assert becomes a content-equality assert.
        for (key, value) in entries.iter() {
            debug_assert_eq!(key.as_slice(), value.hostname.as_slice());
        }
        entries.clear();
    }

    /// Whether an error from a cache operation reflects a problem with the cache
    /// itself. A destination we can't key on and a lock held by another process
    /// both leave the cache healthy, so neither is worth reporting.
    // pub fn isFailure(err: anyerror) bool {
    pub fn is_failure(err: &Error) -> bool {
        // return switch (err) {
        //     error.InvalidCacheKey, error.CacheLocked => false,
        //     else => true,
        // };
        !matches!(err, Error::InvalidCacheKey | Error::CacheLocked)
    }

    // Supports both standalone hostnames and user@hostname format
    // pub fn isValidCacheKey(key: []const u8) bool {
    pub fn is_valid_cache_key(key: &[u8]) -> bool {
        // if (key.len == 0) return false;
        if key.is_empty() {
            return false;
        }

        // Check for user@hostname format
        // if (std.mem.indexOfScalar(u8, key, '@')) |at_pos| {
        //     const user = key[0..at_pos];
        //     const hostname = key[at_pos + 1 ..];
        //     return isValidUser(user) and isValidHost(hostname);
        // }
        if let Some(at_pos) = key.iter().position(|&c| c == b'@') {
            let user = &key[0..at_pos];
            let hostname = &key[at_pos + 1..];
            return is_valid_user(user) && is_valid_host(hostname);
        }

        // return isValidHost(key);
        is_valid_host(key)
    }
}

// fn readEntries(alloc: Allocator, file: std.fs.File) !std.StringHashMap(Entry) {
fn read_entries(file: &File) -> Result<HashMap<Vec<u8>, Entry>, Error> {
    // var reader = file.reader(&.{});
    // const content = try reader.interface.allocRemaining(alloc, .limited(MAX_CACHE_SIZE));
    // defer alloc.free(content);
    //
    // PORT NOTE: read up to MAX_CACHE_SIZE bytes; a larger file is rejected (the
    // reference's `.limited` errors `StreamTooLong` past the cap). `try_clone`
    // reads without disturbing the caller's handle (which stays locked).
    let mut content: Vec<u8> = Vec::new();
    {
        let handle = file.try_clone().map_err(Error::Io)?;
        handle
            .take((MAX_CACHE_SIZE as u64) + 1)
            .read_to_end(&mut content)
            .map_err(Error::Io)?;
    }
    if content.len() > MAX_CACHE_SIZE {
        return Err(Error::Io(std::io::Error::other(
            "cache file exceeds MAX_CACHE_SIZE",
        )));
    }

    // var entries = std.StringHashMap(Entry).init(alloc);
    // errdefer deinitEntries(alloc, &entries);
    let mut entries: HashMap<Vec<u8>, Entry> = HashMap::new();

    // var lines = std.mem.tokenizeScalar(u8, content, '\n');
    // while (lines.next()) |line| {
    let mut lines = TokenizeScalar::new(&content, b'\n');
    while let Some(line) = lines.next() {
        // const trimmed = std.mem.trim(u8, line, " \t\r");
        let trimmed = trim(line, b" \t\r");
        // const entry = Entry.parse(trimmed) orelse continue;
        let parsed = match Entry::parse(trimmed) {
            Some(e) => e,
            None => continue,
        };

        // Dupe both strings up front, before inserting, so the map never
        // holds a half-built entry (a borrowed key or a freed/undefined
        // value) for `deinitEntries` to walk if an allocation fails.
        //
        // PORT NOTE: `Entry::parse` already yields owned copies, so the reference's
        // `var hostname: ?[]u8 = alloc.dupe(...)` / `errdefer free` dance is the
        // ownership of `parsed` below (moved into the map, or dropped on a
        // duplicate/older line).
        // var hostname: ?[]u8 = try alloc.dupe(u8, entry.hostname);
        // errdefer if (hostname) |h| alloc.free(h);
        // var terminfo: ?[]u8 = try alloc.dupe(u8, entry.terminfo_version);
        // errdefer if (terminfo) |t| alloc.free(t);

        // const gop = try entries.getOrPut(hostname.?);
        match entries.entry(parsed.hostname.clone()) {
            // if (!gop.found_existing) {
            std::collections::hash_map::Entry::Vacant(slot) => {
                // New entry: transfer both copies to the map.
                // gop.value_ptr.* = .{ .hostname = hostname.?, .timestamp = entry.timestamp, .terminfo_version = terminfo.? };
                // hostname = null; terminfo = null;
                slot.insert(parsed);
            }
            // } else {
            std::collections::hash_map::Entry::Occupied(mut slot) => {
                // Duplicate key: the map keeps its existing key, so free ours.
                // alloc.free(hostname.?); hostname = null;
                //   (Ours — `parsed.hostname` — is dropped with `parsed` below.)

                // Handle duplicate entries - keep newer timestamp
                // if (entry.timestamp > gop.value_ptr.timestamp) {
                if parsed.timestamp > slot.get().timestamp {
                    // gop.value_ptr.timestamp = entry.timestamp;
                    slot.get_mut().timestamp = parsed.timestamp;
                    // if (!std.mem.eql(u8, gop.value_ptr.terminfo_version, terminfo.?)) {
                    //     alloc.free(gop.value_ptr.terminfo_version);
                    //     gop.value_ptr.terminfo_version = terminfo.?;
                    //     terminfo = null;
                    // }
                    if slot.get().terminfo_version != parsed.terminfo_version {
                        slot.get_mut().terminfo_version = parsed.terminfo_version.clone();
                    }
                }
                // if (terminfo) |t| alloc.free(t); terminfo = null;
                //   (`parsed` — and any not-transferred bytes — drops here.)
            }
        }
    }

    // return entries;
    Ok(entries)
}

// fn fixupPermissions(file: std.fs.File) !void {
fn fixup_permissions(file: &File) -> Result<(), Error> {
    // Windows does not support chmod
    // if (comptime builtin.os.tag == .windows) return;
    #[cfg(windows)]
    {
        let _ = file;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        // Ensure file has correct permissions (readable/writable by owner only)
        // const stat = try file.stat();
        let md = file.metadata().map_err(Error::Io)?;
        // if (stat.mode & 0o777 != 0o600) { try file.chmod(0o600); }
        if md.permissions().mode() & 0o777 != 0o600 {
            file.set_permissions(std::fs::Permissions::from_mode(0o600))
                .map_err(Error::Io)?;
        }
        Ok(())
    }
}

// Checks if a host is a valid hostname or IP address
// fn isValidHost(host: []const u8) bool {
fn is_valid_host(host: &[u8]) -> bool {
    // First check for valid hostnames because this is assumed to be the more
    // likely ssh host format.
    // if (std.Io.net.HostName.validate(host)) |_| {
    //     return true;
    // } else |_| {}
    //
    // PORT NOTE (parity S71-F1, task T-C): this called
    // `rustty_os::hostname::isValid`, a port of the since-deleted
    // `os/hostname.zig` whose length limit is 253. At the pin the reference
    // calls the Zig 0.16.0 standard library's `HostName.validate`
    // (`max_len = 255`), ported below as `host_name_validate`.
    if host_name_validate(host) {
        return true;
    }

    // We also accept valid IP addresses. In practice, IPv4 addresses are also
    // considered valid hostnames due to their overlapping syntax, so we can
    // simplify this check to be IPv6-specific.
    // if (std.Io.net.IpAddress.parseIp6(host, 0)) |_| {
    //     return true;
    // } else |_| {
    //     return false;
    // }
    //
    // PORT NOTE (parity S71-F3, task T-C): this used `std::net::Ipv6Addr`'s
    // parser, whose grammar differs from Zig's `Ip6Address.Unresolved.parse`:
    // Rust accepts an embedded IPv4 in any address (`::1.2.3.4`,
    // `64:ff9b::1.2.3.4`), the reference only after a literal `::ffff:`, and the
    // reference accepts forms Rust rejects (`::1:2:3:4:5:6:7:8`). The reference
    // parser is ported below as `ip6_parse`.
    ip6_parse(host)
}

/// PORTS: Zig 0.16.0 `std.Io.net.HostName.validate` — `true` where the
/// reference returns without error. Validates a hostname according to
/// [RFC 1123](https://www.rfc-editor.org/rfc/rfc1123).
fn host_name_validate(bytes: &[u8]) -> bool {
    // pub const max_len = 255;
    const MAX_LEN: usize = 255;

    // if (bytes.len == 0) return error.InvalidHostName;
    if bytes.is_empty() {
        return false;
    }

    // Ignore trailing dot (FQDN). It doesn't count toward our length.
    let end = if bytes[bytes.len() - 1] == b'.' {
        bytes.len() - 1
    } else {
        bytes.len()
    };

    // The accepted maximum length of a hostname, including labels and dots.
    if end > MAX_LEN {
        return false;
    }

    // Hostnames are divided into dot-separated "labels", which:
    //
    // - Start with a letter or digit
    // - Can contain letters, digits, or hyphens
    // - Must end with a letter or digit
    // - Have a minimum of 1 character and a maximum of 63
    let mut label_len: usize = 0;
    for (i, &c) in bytes[0..end].iter().enumerate() {
        match c {
            b'.' => {
                if label_len == 0 || label_len > 63 {
                    return false;
                }
                if !bytes[i - 1].is_ascii_alphanumeric() {
                    return false;
                }
                label_len = 0;
            }
            b'-' => {
                if label_len == 0 {
                    return false;
                }
                label_len += 1;
            }
            _ => {
                if !c.is_ascii_alphanumeric() {
                    return false;
                }
                label_len += 1;
            }
        }
    }

    // Validate the final label
    if label_len == 0 || label_len > 63 {
        return false;
    }
    bytes[end - 1].is_ascii_alphanumeric()
}

/// PORTS: Zig 0.16.0 `std.Io.net.IpAddress.parseIp6(text, 0)` succeeding, i.e.
/// `Ip6Address.Unresolved.parse(text)` returning `.success` with no interface
/// name (a scope id makes `Ip6Address.parse` return `error.UnresolvedScope`).
/// Only acceptance is observable here, so the parsed bytes are not built.
fn ip6_parse(text: &[u8]) -> bool {
    // if (text.len < 2) return .incomplete;
    if text.len() < 2 {
        return false;
    }
    // const ip4_prefix = "::ffff:";
    // if (std.ascii.startsWithIgnoreCase(text, ip4_prefix)) {
    //     const parsed = Ip4Address.parse(text[ip4_prefix.len..], 0) catch
    //         return .{ .invalid_ip4_mapping = ip4_prefix.len };
    //     ...return .{ .success = ... };
    // }
    const IP4_PREFIX: &[u8] = b"::ffff:";
    if text.len() >= IP4_PREFIX.len() && text[..IP4_PREFIX.len()].eq_ignore_ascii_case(IP4_PREFIX) {
        return ip4_parse(&text[IP4_PREFIX.len()..]);
    }

    // Has to be u16 elements to handle 3-digit hex numbers from compression.
    let mut parts: [u16; 8] = [0; 8];
    let mut parts_i: usize = 0;
    // var text_i: u8 = 0;
    let mut text_i: usize = 0;
    let mut digit_i: u8 = 0;
    let mut compress_start: Option<usize> = None;
    let mut interface_name: bool = false;

    // state: switch (State.digit) { .digit => c: switch (text[text_i]) { ... } }
    let mut c = text[text_i];
    loop {
        match c {
            b'a'..=b'f' | b'A'..=b'F' | b'0'..=b'9' => {
                // 'A'...'F' => |c| continue :c c - 'A' + 'a',
                let digit = match c {
                    b'a'..=b'f' => c - b'a' + 10,
                    b'A'..=b'F' => c - b'A' + 10,
                    _ => c - b'0',
                };
                // parts[parts_i] = (std.math.mul(u16, parts[parts_i], 16) catch
                //     return .{ .overflow = text_i }) + digit;
                match parts[parts_i].checked_mul(16) {
                    Some(v) => parts[parts_i] = v + u16::from(digit),
                    None => return false,
                }
                // if (digit_i == 4) return .{ .invalid_byte = text_i };
                if digit_i == 4 {
                    return false;
                }
                digit_i += 1;
                text_i += 1;
                if text.len() - text_i == 0 {
                    parts_i += 1;
                    break; // continue :state .end;
                }
                c = text[text_i];
            }
            b':' => {
                if digit_i == 0 {
                    // if (compress_start != null) return .{ .invalid_byte = text_i };
                    if compress_start.is_some() {
                        return false;
                    }
                    if text_i == 0 {
                        text_i += 1;
                        if text[text_i] != b':' {
                            return false;
                        }
                        debug_assert!(parts_i == 0);
                    }
                    compress_start = Some(parts_i);
                    text_i += 1;
                    if text.len() - text_i == 0 {
                        break; // continue :state .end;
                    }
                    c = text[text_i];
                } else {
                    parts_i += 1;
                    if parts.len() - parts_i == 0 {
                        break; // continue :state .end;
                    }
                    digit_i = 0;
                    text_i += 1;
                    // if (text.len - text_i == 0) return .incomplete;
                    if text.len() - text_i == 0 {
                        return false;
                    }
                    c = text[text_i];
                }
            }
            b'%' => {
                // if (digit_i == 0) return .{ .invalid_byte = text_i };
                if digit_i == 0 {
                    return false;
                }
                parts_i += 1;
                text_i += 1;
                let name = &text[text_i..];
                // if (name.len == 0) return .incomplete;
                if name.is_empty() {
                    return false;
                }
                interface_name = true;
                // text_i = @intCast(text.len);
                //
                // PORT NOTE: reference defect (R13), UPSTREAM-DEFECTS.md UD160.
                // The reference's `text_i` is a `u8`, so this `@intCast` traps
                // (ReleaseSafe) or is undefined behaviour (ReleaseFast) for any
                // text longer than 255 bytes, which `+ssh-cache --add=` and the
                // shell-integration ssh wrapper pass through unchecked (MEASURED
                // with Zig 0.16.0: `fe80::1%` plus 300 bytes panics "integer does
                // not fit in destination type"). The port keeps a `usize` index,
                // so a long scoped address reaches the same outcome as a short
                // one: a scope id is `error.UnresolvedScope`, i.e. not a valid
                // host.
                text_i = text.len();
                break; // continue :state .end;
            }
            // else => return .{ .invalid_byte = text_i },
            _ => return false,
        }
    }

    // .end => {
    // if (text.len - text_i != 0) return .{ .junk_after_end = text_i };
    if text.len() - text_i != 0 {
        return false;
    }
    let remaining = parts.len() - parts_i;
    // if (compress_start) |s| { ...memmove/memset... } else { if (remaining != 0) return .incomplete; }
    if compress_start.is_none() && remaining != 0 {
        return false;
    }

    // `Ip6Address.parse`: `.interface = if (p.interface_name != null) return
    // error.UnresolvedScope else .none`.
    !interface_name
}

/// PORTS: Zig 0.16.0 `std.Io.net.Ip4Address.parse(buffer, 0)` succeeding.
fn ip4_parse(buffer: &[u8]) -> bool {
    let mut bytes: [u8; 4] = [0; 4];
    let mut index: usize = 0;
    let mut saw_any_digits = false;
    let mut has_zero_prefix = false;
    for &c in buffer {
        match c {
            b'.' => {
                if !saw_any_digits {
                    return false;
                }
                if index == 3 {
                    return false;
                }
                index += 1;
                saw_any_digits = false;
                has_zero_prefix = false;
            }
            b'0'..=b'9' => {
                if c == b'0' && !saw_any_digits {
                    has_zero_prefix = true;
                } else if has_zero_prefix {
                    return false;
                }
                saw_any_digits = true;
                // bytes[index] = try std.math.mul(u8, bytes[index], 10);
                // bytes[index] = try std.math.add(u8, bytes[index], c - '0');
                match bytes[index]
                    .checked_mul(10)
                    .and_then(|v| v.checked_add(c - b'0'))
                {
                    Some(v) => bytes[index] = v,
                    None => return false,
                }
            }
            _ => return false,
        }
    }
    // if (index == 3 and saw_any_digits) return .{ ... };
    // return error.Incomplete;
    index == 3 && saw_any_digits
}

// fn isValidUser(user: []const u8) bool {
fn is_valid_user(user: &[u8]) -> bool {
    // if (user.len == 0 or user.len > 64) return false;
    if user.is_empty() || user.len() > 64 {
        return false;
    }
    // for (user) |c| {
    //     switch (c) {
    //         'a'...'z', 'A'...'Z', '0'...'9', '_', '-', '.' => {},
    //         else => return false,
    //     }
    // }
    for &c in user {
        match c {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'-' | b'.' => {}
            _ => return false,
        }
    }
    // return true;
    true
}

// ---------------------------------------------------------------------------
// PORT NOTE: support shims for the std facilities used above.
// ---------------------------------------------------------------------------

// `std.fs.path.sep`.
#[cfg(windows)]
const SEP: u8 = b'\\';
#[cfg(not(windows))]
const SEP: u8 = b'/';

/// `std.time.timestamp()` — current Unix time in whole seconds.
fn timestamp() -> i64 {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        // Pre-epoch clock: negative seconds (mirrors Zig's signed timestamp).
        Err(e) => -(e.duration().as_secs() as i64),
    }
}

/// Interpret raw path bytes as a filesystem `Path` (R3: paths are byte strings).
#[cfg(not(windows))]
fn as_path(bytes: &[u8]) -> &Path {
    use std::os::unix::ffi::OsStrExt;
    Path::new(std::ffi::OsStr::from_bytes(bytes))
}
#[cfg(windows)]
fn as_path(bytes: &[u8]) -> &Path {
    Path::new(std::str::from_utf8(bytes).unwrap_or(""))
}

/// `std.fs.path.join(&.{ dir, name })`: join with the native separator,
/// collapsing a duplicated boundary separator.
fn join_path(dir: &[u8], name: &[u8]) -> Vec<u8> {
    let mut out = dir.to_vec();
    if !out.is_empty() && out[out.len() - 1] != SEP {
        out.push(SEP);
    }
    out.extend_from_slice(name);
    out
}

/// `std.fs.path.dirname`: the directory component (everything before the last
/// separator), or `None` if the path has no separator.
fn path_dirname(path: &[u8]) -> Option<&[u8]> {
    let idx = path.iter().rposition(|&c| c == SEP)?;
    // A single leading separator (`/name`) keeps the root as the dirname.
    if idx == 0 {
        Some(&path[..1])
    } else {
        Some(&path[..idx])
    }
}

/// `std.fs.path.basename`: the final path component.
fn path_basename(path: &[u8]) -> &[u8] {
    match path.iter().rposition(|&c| c == SEP) {
        Some(idx) => &path[idx + 1..],
        None => path,
    }
}

/// Build a random temp-file path in `cache_dir` for the atomic write.
fn atomic_temp_path(cache_dir: &[u8]) -> Result<Vec<u8>, Error> {
    let mut name_buf = [0u8; 64];
    let rand =
        rustty_os::file::randomBasename(&mut name_buf).map_err(|_| Error::InvalidCachePath)?;
    let mut name = rand.to_vec();
    name.extend_from_slice(b".tmp");
    Ok(join_path(cache_dir, &name))
}

// `std.fs.createFileAbsolute(path, .{ .read = true, .exclusive = true, .mode = 0o600 })`.
fn create_new_secure(path: &Path) -> std::io::Result<File> {
    let mut opts = OpenOptions::new();
    opts.read(true).write(true).create_new(true);
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    opts.open(path)
}

// `std.Io.Dir.createFileAbsolute(io, path, .{ .read = true, .truncate = false,
// .permissions = 0o600 })` with the default `.exclusive = false`: open
// read-write, creating the file 0600 if it does not exist, without truncating
// or re-moding an existing one.
fn create_secure(path: &Path) -> std::io::Result<File> {
    let mut opts = OpenOptions::new();
    opts.read(true).write(true).create(true).truncate(false);
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    opts.open(path)
}

// `std.fs.openFileAbsolute(path, .{ .mode = .read_write })`.
fn open_read_write(path: &Path) -> std::io::Result<File> {
    OpenOptions::new().read(true).write(true).open(path)
}

// `std.fs.openFileAbsolute(path, .{})` (read-only).
fn open_read_only(path: &Path) -> std::io::Result<File> {
    OpenOptions::new().read(true).open(path)
}

/// A RAII guard reproducing `file.tryLock(.exclusive)` + `defer file.unlock()`.
/// The lock is skipped on Windows (the reference gates it behind
/// `builtin.os.tag != .windows` — a Zig std compile-failure workaround).
struct FlockGuard<'a> {
    #[cfg_attr(windows, allow(dead_code))]
    file: &'a File,
}

impl<'a> FlockGuard<'a> {
    fn acquire(file: &'a File) -> Result<FlockGuard<'a>, Error> {
        #[cfg(not(windows))]
        {
            use std::os::unix::io::AsRawFd;
            // _ = file.tryLock(.exclusive) catch return error.CacheLocked;
            let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
            if rc != 0 {
                return Err(Error::CacheLocked);
            }
        }
        Ok(FlockGuard { file })
    }
}

impl Drop for FlockGuard<'_> {
    fn drop(&mut self) {
        // defer if (comptime builtin.os.tag != .windows) file.unlock();
        #[cfg(not(windows))]
        {
            use std::os::unix::io::AsRawFd;
            unsafe {
                libc::flock(self.file.as_raw_fd(), libc::LOCK_UN);
            }
        }
    }
}

/// `std.mem.trim(u8, s, set)`: strip leading/trailing bytes that appear in `set`.
fn trim<'a>(s: &'a [u8], set: &[u8]) -> &'a [u8] {
    let mut start = 0;
    let mut end = s.len();
    while start < end && set.contains(&s[start]) {
        start += 1;
    }
    while end > start && set.contains(&s[end - 1]) {
        end -= 1;
    }
    &s[start..end]
}

/// `std.mem.tokenizeScalar(u8, buf, delim)`: iterate non-empty runs between
/// `delim` bytes.
struct TokenizeScalar<'a> {
    rest: &'a [u8],
    delim: u8,
}

impl<'a> TokenizeScalar<'a> {
    fn new(buf: &'a [u8], delim: u8) -> TokenizeScalar<'a> {
        TokenizeScalar { rest: buf, delim }
    }

    fn next(&mut self) -> Option<&'a [u8]> {
        while !self.rest.is_empty() && self.rest[0] == self.delim {
            self.rest = &self.rest[1..];
        }
        if self.rest.is_empty() {
            return None;
        }
        let mut i = 0;
        while i < self.rest.len() && self.rest[i] != self.delim {
            i += 1;
        }
        let tok = &self.rest[..i];
        self.rest = &self.rest[i..];
        Some(tok)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};

    // PORT NOTE (test-only glue): a per-test temporary directory with unique naming and
    // Drop cleanup, reproducing the reference's `std.testing.tmpDir(.{})` +
    // `defer tmp.cleanup()`. Per-test isolation keeps the parallel `cargo test`
    // runner from racing on a shared cache path (the parallel-init isolation
    // lesson).
    struct TmpDir {
        path: PathBuf,
    }

    static TMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

    impl TmpDir {
        fn new() -> TmpDir {
            let n = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let mut path = std::env::temp_dir();
            path.push(format!(
                "rustty-ssh-cache-test-{}-{}",
                std::process::id(),
                n
            ));
            std::fs::create_dir_all(&path).unwrap();
            TmpDir { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }

        fn join(&self, name: &str) -> PathBuf {
            self.path.join(name)
        }
    }

    impl Drop for TmpDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    // The raw bytes of a `Path` (R3: DiskCache paths are byte strings).
    #[cfg(not(windows))]
    fn path_bytes(p: &Path) -> Vec<u8> {
        use std::os::unix::ffi::OsStrExt;
        p.as_os_str().as_bytes().to_vec()
    }
    #[cfg(windows)]
    fn path_bytes(p: &Path) -> Vec<u8> {
        p.to_string_lossy().into_owned().into_bytes()
    }

    // std.time.s_per_* constants used by the prune tests.
    const S_PER_MIN: i64 = 60;
    const S_PER_HOUR: i64 = 60 * 60;
    const S_PER_DAY: i64 = 24 * 60 * 60;

    // test "disk cache default path"
    #[test]
    #[cfg_attr(miri, ignore)] // reads HOME/XDG_STATE_HOME (real OS state)
    fn disk_cache_default_path() {
        // const path = try DiskCache.defaultPath(alloc, "ghostty");
        // defer alloc.free(path);
        //   R2 / ADV-FIX 5: the program dir is the Rustty app name via identity.
        let path = DiskCache::default_path(rustty_identity::APP_NAME).unwrap();
        // try testing.expect(path.len > 0);
        assert!(!path.is_empty());
    }

    // test "disk cache clear"
    #[test]
    #[cfg_attr(miri, ignore)] // real-OS file create/delete syscalls
    fn disk_cache_clear() {
        // Create our path
        let tmp = TmpDir::new();
        // var file = try tmp.dir.createFile("cache", .{});
        // try file_writer.interface.writeAll("HELLO!");
        std::fs::write(tmp.join("cache"), b"HELLO!").unwrap();
        // const path = try tmp.dir.realpathAlloc(alloc, "cache");
        let path = std::fs::canonicalize(tmp.join("cache")).unwrap();

        // Setup our cache
        // const cache: DiskCache = .{ .path = path };
        let cache = DiskCache {
            path: path_bytes(&path),
        };
        // try cache.clear();
        cache.clear().unwrap();

        // Verify the file is gone
        // try testing.expectError(error.FileNotFound, tmp.dir.openFile("cache", .{}));
        assert_eq!(
            File::open(tmp.join("cache")).unwrap_err().kind(),
            std::io::ErrorKind::NotFound,
        );
    }

    // test "disk cache operations"
    #[test]
    #[cfg_attr(miri, ignore)] // real-OS file/lock/rename syscalls
    fn disk_cache_operations() {
        // Create our path
        let tmp = TmpDir::new();
        // var file = try tmp.dir.createFile("cache", .{}); writer.writeAll("HELLO!"); writer.flush();
        std::fs::write(tmp.join("cache"), b"HELLO!").unwrap();
        // const path = try tmp.dir.realpathAlloc(alloc, "cache");
        let path = std::fs::canonicalize(tmp.join("cache")).unwrap();

        // Setup our cache. Adding the same key twice exercises both the new
        // and existing-entry paths.
        // const cache: DiskCache = .{ .path = path };
        let cache = DiskCache {
            path: path_bytes(&path),
        };
        // try cache.add(alloc, "example.com", "v1", std.Io.Timestamp.now(testing.io, .real).toSeconds());
        cache.add(b"example.com", b"v1", timestamp()).unwrap();
        // try testing.expect(!try cache.contains(alloc, "example.com", "v2"));
        assert!(!cache.contains(b"example.com", b"v2").unwrap());
        // try cache.add(alloc, "example.com", "v2", std.Io.Timestamp.now(testing.io, .real).toSeconds());
        cache.add(b"example.com", b"v2", timestamp()).unwrap();
        // try testing.expect(try cache.contains(alloc, "example.com", "v2"));
        assert!(cache.contains(b"example.com", b"v2").unwrap());

        // List
        // var entries = try cache.list(alloc);
        // defer deinitEntries(alloc, &entries);
        let mut entries = cache.list().unwrap();
        // try testing.expectEqualStrings(
        //     "v2",
        //     entries.get("example.com").?.terminfo_version,
        // );
        assert_eq!(
            b"v2".as_slice(),
            entries
                .get(b"example.com".as_slice())
                .unwrap()
                .terminfo_version
                .as_slice(),
        );

        // Remove reports that it removed the entry, and a second remove of the
        // same key reports nothing to remove.
        // try testing.expect(try cache.remove(alloc, "example.com"));
        assert!(cache.remove(b"example.com").unwrap());
        // try testing.expect(!try cache.remove(alloc, "example.com"));
        assert!(!cache.remove(b"example.com").unwrap());
        // try testing.expect(!(try cache.contains(alloc, "example.com", "v2")));
        assert!(!cache.contains(b"example.com", b"v2").unwrap());
        // try cache.add(alloc, "example.com", "v2", std.Io.Timestamp.now(testing.io, .real).toSeconds());
        cache.add(b"example.com", b"v2", timestamp()).unwrap();

        // (the reference's `defer deinitEntries(alloc, &entries)`)
        DiskCache::deinit_entries(&mut entries);
    }

    // test "disk cache cleans up temp files"
    #[test]
    #[cfg_attr(miri, ignore)] // real-OS dir iteration / rename syscalls
    fn disk_cache_cleans_up_temp_files() {
        let tmp = TmpDir::new();

        // const tmp_path = try tmp.dir.realpathAlloc(alloc, ".");
        let tmp_path = std::fs::canonicalize(tmp.path()).unwrap();
        // const cache_path = try std.fs.path.join(alloc, &.{ tmp_path, "cache" });
        let cache_path = tmp_path.join("cache");

        // const cache: DiskCache = .{ .path = cache_path };
        let cache = DiskCache {
            path: path_bytes(&cache_path),
        };
        // try cache.add(alloc, "example.com", "v1", std.Io.Timestamp.now(testing.io, .real).toSeconds());
        cache.add(b"example.com", b"v1", timestamp()).unwrap();
        // try cache.add(alloc, "example.org", "v1", std.Io.Timestamp.now(testing.io, .real).toSeconds());
        cache.add(b"example.org", b"v1", timestamp()).unwrap();

        // Verify only the cache file exists and no temp files left behind
        // var count: usize = 0;
        // var iter = tmp.dir.iterate();
        // while (try iter.next()) |entry| {
        //     count += 1;
        //     try testing.expectEqualStrings("cache", entry.name);
        // }
        // try testing.expectEqual(1, count);
        let mut count: usize = 0;
        for entry in std::fs::read_dir(&tmp_path).unwrap() {
            let entry = entry.unwrap();
            count += 1;
            assert_eq!("cache", entry.file_name().to_str().unwrap());
        }
        assert_eq!(1, count);
    }

    // test "disk cache prune"
    #[test]
    #[cfg_attr(miri, ignore)] // real-OS file/lock/rename syscalls
    fn disk_cache_prune() {
        let tmp = TmpDir::new();
        let tmp_path = std::fs::canonicalize(tmp.path()).unwrap();
        let cache_path = tmp_path.join("cache");

        let cache = DiskCache {
            path: path_bytes(&cache_path),
        };

        // Back-date one entry an hour old and one 100 days old.
        // const day = std.time.s_per_day; const hour = std.time.s_per_hour;
        let day = S_PER_DAY;
        let hour = S_PER_HOUR;
        // const now = std.time.timestamp();
        let now = timestamp();
        // try cache.add(alloc, "recent.com", "v1", now - hour);
        cache.add(b"recent.com", b"v1", now - hour).unwrap();
        // try cache.add(alloc, "old.com", "v1", now - 100 * day);
        cache.add(b"old.com", b"v1", now - 100 * day).unwrap();

        // Prune entries older than 90 days: only old.com goes.
        // try testing.expectEqual(@as(usize, 1), try cache.prune(alloc, 90 * day));
        assert_eq!(1_usize, cache.prune((90 * day) as u64).unwrap());
        // try testing.expect(try cache.contains(alloc, "recent.com", "v1"));
        assert!(cache.contains(b"recent.com", b"v1").unwrap());
        // try testing.expect(!try cache.contains(alloc, "old.com", "v1"));
        assert!(!cache.contains(b"old.com", b"v1").unwrap());

        // Pruning again removes nothing.
        // try testing.expectEqual(@as(usize, 0), try cache.prune(alloc, 90 * day));
        assert_eq!(0_usize, cache.prune((90 * day) as u64).unwrap());

        // Sub-day granularity: a 30-minute max age prunes the hour-old entry.
        // try testing.expectEqual(@as(usize, 1), try cache.prune(alloc, 30 * std.time.s_per_min));
        assert_eq!(1_usize, cache.prune((30 * S_PER_MIN) as u64).unwrap());
        // try testing.expect(!try cache.contains(alloc, "recent.com", "v1"));
        assert!(!cache.contains(b"recent.com", b"v1").unwrap());
    }

    // test "disk cache prune missing file"
    #[test]
    #[cfg_attr(miri, ignore)] // real-OS file syscalls
    fn disk_cache_prune_missing_file() {
        let tmp = TmpDir::new();

        let tmp_path = std::fs::canonicalize(tmp.path()).unwrap();
        let cache_path = tmp_path.join("cache");

        let cache = DiskCache {
            path: path_bytes(&cache_path),
        };
        // try testing.expectEqual(@as(usize, 0), try cache.prune(alloc, 30));
        assert_eq!(0_usize, cache.prune(30).unwrap());
    }

    // test "disk cache reads duplicate keys"
    #[test]
    #[cfg_attr(miri, ignore)] // real-OS file syscalls
    fn disk_cache_reads_duplicate_keys() {
        let tmp = TmpDir::new();

        // Exercise readEntries' found_existing branch: replace the existing
        // key with the updated entry and ensure (via testing.allocator) that
        // we don't double-free or leak.
        //   R2 / ADV-FIX 5: the older entry's terminfo token is the Rustty
        //   primary terminfo name; the newer one ("xterm-newer") wins.
        let ti = rustty_identity::TERMINFO_PRIMARY_NAME;
        // try file_writer.interface.writeAll("example.com|100|xterm-ghostty\nexample.com|200|xterm-newer\n");
        std::fs::write(
            tmp.join("cache"),
            format!("example.com|100|{ti}\nexample.com|200|xterm-newer\n"),
        )
        .unwrap();
        // const path = try tmp.dir.realpathAlloc(alloc, "cache");
        let path = std::fs::canonicalize(tmp.join("cache")).unwrap();

        // const cache: DiskCache = .{ .path = path };
        let cache = DiskCache {
            path: path_bytes(&path),
        };
        // var entries = try cache.list(alloc);
        // defer deinitEntries(alloc, &entries);
        let mut entries = cache.list().unwrap();

        // try testing.expectEqual(@as(u32, 1), entries.count());
        assert_eq!(1, entries.len());
        // const entry = entries.get("example.com").?;
        let entry = entries.get(b"example.com".as_slice()).unwrap();
        // try testing.expectEqual(@as(i64, 200), entry.timestamp);
        assert_eq!(200_i64, entry.timestamp);
        // try testing.expectEqualStrings("xterm-newer", entry.terminfo_version);
        assert_eq!(b"xterm-newer".as_slice(), entry.terminfo_version.as_slice());

        DiskCache::deinit_entries(&mut entries);
    }

    // test "disk cache reads survive allocation failure"
    #[test]
    #[cfg_attr(miri, ignore)] // real-OS file syscalls
    fn disk_cache_reads_survive_allocation_failure() {
        let tmp = TmpDir::new();

        // Exercise a populated cache containing a duplicate key to ensure
        // that we hit all of the possible allocation behaviors below.
        let ti = rustty_identity::TERMINFO_PRIMARY_NAME;
        std::fs::write(
            tmp.join("cache"),
            format!("a.com|100|{ti}\nb.com|100|{ti}\nc.com|100|{ti}\na.com|200|xterm-newer\n"),
        )
        .unwrap();
        // const path = try tmp.dir.realpathAlloc(testing.allocator, "cache");
        let path = std::fs::canonicalize(tmp.join("cache")).unwrap();

        // const cache: DiskCache = .{ .path = path };
        let cache = DiskCache {
            path: path_bytes(&path),
        };

        // Fail the Nth allocation for every N until the read completes. The
        // FailingAllocator is backed by testing.allocator so we also ensure
        // that we don't double-free or leak; this can only completely succeed
        // or fail with OutOfMemory.
        // var fail_index: usize = 0;
        // while (true) : (fail_index += 1) {
        //     var failing = std.testing.FailingAllocator.init(testing.allocator, .{ .fail_index = fail_index });
        //     const alloc = failing.allocator();
        //     if (cache.list(alloc)) |entries_const| {
        //         var entries = entries_const;
        //         deinitEntries(alloc, &entries);
        //         // Reached a run with no induced failure: every path covered.
        //         if (!failing.has_induced_failure) break;
        //     } else |err| {
        //         try testing.expectEqual(error.OutOfMemory, err);
        //     }
        // }
        //
        // PORT NOTE: Rust allocation is infallible (aborts), so there is no
        // `OutOfMemory` to inject and the fail_index loop collapses to its single
        // success run. The no-double-free / no-leak guarantee the reference proves
        // via FailingAllocator is provided statically by ownership (each `Entry`
        // owns its bytes; the `HashMap` frees each once on drop), with Miri (R6)
        // as the UB/leak backstop for the pure logic. The reference loop's one
        // assertion (its terminal state: a run where `cache.list` completes
        // with no induced failure, i.e. the non-error arm and the `break`) maps
        // to asserting the collapsed run's success arm.
        let list_result = cache.list();
        // Reached a run with no induced failure: every path covered.
        assert!(list_result.is_ok());
        let mut entries = list_result.unwrap();
        DiskCache::deinit_entries(&mut entries);
    }

    // test "disk cache add survives allocation failure"
    #[test]
    #[cfg_attr(miri, ignore)] // real-OS file/lock/rename syscalls
    fn disk_cache_add_survives_allocation_failure() {
        let tmp = TmpDir::new();
        let tmp_path = std::fs::canonicalize(tmp.path()).unwrap();
        let path = tmp_path.join("cache");

        // const cache: DiskCache = .{ .path = path };
        let cache = DiskCache {
            path: path_bytes(&path),
        };

        // Fail the Nth allocation for every N until add completes. A failed add
        // must not leak or leave a half-built map entry. The FailingAllocator
        // is backed by testing.allocator to catch either. Each iteration starts
        // from a clean cache file.
        // var fail_index: usize = 0;
        // while (true) : (fail_index += 1) {
        //     std.fs.cwd().deleteFile(path) catch {};
        //     var failing = std.testing.FailingAllocator.init(testing.allocator, .{ .fail_index = fail_index });
        //     const alloc = failing.allocator();
        //     if (cache.add(alloc, "user@example.com", "v1", 100)) |_| {
        //         if (!failing.has_induced_failure) break;
        //     } else |err| { try testing.expectEqual(error.OutOfMemory, err); }
        // }
        //
        // PORT NOTE: see the `reads survive allocation failure` note — Rust's
        // infallible allocation collapses the injection loop to its single
        // success run; ownership + Miri (R6) provide the leak/UB guarantee. The
        // reference loop's one assertion (its terminal state: an `add` run with
        // no induced failure, i.e. the non-error arm and the `break`) maps to
        // asserting the collapsed run's success arm.
        assert!(cache.add(b"user@example.com", b"v1", 100).is_ok());
    }

    // test isFailure
    #[test]
    fn is_failure_test() {
        // try testing.expect(!isFailure(error.InvalidCacheKey));
        assert!(!DiskCache::is_failure(&Error::InvalidCacheKey));
        // try testing.expect(!isFailure(error.CacheLocked));
        assert!(!DiskCache::is_failure(&Error::CacheLocked));
        // try testing.expect(isFailure(error.AccessDenied));
        //   (`error.AccessDenied` / `error.IsDir` are I/O-shaped in the port)
        assert!(DiskCache::is_failure(&Error::Io(std::io::Error::from(
            std::io::ErrorKind::PermissionDenied
        ))));
        // try testing.expect(isFailure(error.IsDir));
        assert!(DiskCache::is_failure(&Error::Io(std::io::Error::from(
            std::io::ErrorKind::IsADirectory
        ))));
    }

    // test isValidHost
    #[test]
    fn is_valid_host_test() {
        // Valid hostnames
        assert!(is_valid_host(b"localhost"));
        assert!(is_valid_host(b"example.com"));
        assert!(is_valid_host(b"sub.example.com"));

        // IPv4 addresses
        assert!(is_valid_host(b"127.0.0.1"));
        assert!(is_valid_host(b"192.168.1.1"));

        // IPv6 addresses
        assert!(is_valid_host(b"::1"));
        assert!(is_valid_host(b"2001:db8::1"));
        assert!(is_valid_host(b"2001:db8:0:1:1:1:1:1"));
        assert!(!is_valid_host(b"fe80::1%eth0")); // scopes not supported

        // Invalid hosts
        assert!(!is_valid_host(b""));
        assert!(!is_valid_host(b"host\nname"));
        assert!(!is_valid_host(b".example.com"));
        assert!(!is_valid_host(b"host..domain"));
        assert!(!is_valid_host(b"-hostname"));
        assert!(!is_valid_host(b"hostname-"));
        assert!(!is_valid_host(b"host name"));
        assert!(!is_valid_host(b"host_name"));
        assert!(!is_valid_host(b"host@domain"));
        assert!(!is_valid_host(b"host:port"));
    }

    // Zig 0.16.0 `std.Io.net.HostName` `test validate`, the function the
    // reference's `isValidHost` calls at the pin (parity S71-F1).
    #[test]
    fn host_name_validate_std_test() {
        fn rep(s: &str, n: usize) -> String {
            s.repeat(n)
        }
        // Valid hostnames
        assert!(host_name_validate(b"example"));
        assert!(host_name_validate(b"example.com"));
        assert!(host_name_validate(b"www.example.com"));
        assert!(host_name_validate(b"sub.domain.example.com"));
        assert!(host_name_validate(b"example.com."));
        assert!(host_name_validate(b"host-name.example.com."));
        assert!(host_name_validate(b"123.example.com."));
        assert!(host_name_validate(b"a-b.com"));
        assert!(host_name_validate(b"a.b.c.d.e.f.g"));
        assert!(host_name_validate(b"127.0.0.1")); // Also a valid hostname
        assert!(host_name_validate((rep("a", 63) + ".com").as_bytes())); // Label exactly 63 chars (valid)
        assert!(host_name_validate((rep("a.", 127) + "a").as_bytes())); // Total length 255 (valid)
        assert!(host_name_validate((rep("a.", 127) + "a.").as_bytes())); // Total length 255 + trailing dot (valid)

        // Invalid hostnames
        assert!(!host_name_validate(b""));
        assert!(!host_name_validate(b".example.com"));
        assert!(!host_name_validate(b"example.com.."));
        assert!(!host_name_validate(b"host..domain"));
        assert!(!host_name_validate(b"-hostname"));
        assert!(!host_name_validate(b"hostname-"));
        assert!(!host_name_validate(b"hostname-.com"));
        assert!(!host_name_validate(b"a.-.b"));
        assert!(!host_name_validate(b"host_name.com"));
        assert!(!host_name_validate(b"."));
        assert!(!host_name_validate(b".."));
        assert!(!host_name_validate((rep("a", 64) + ".com").as_bytes())); // Label length 64 (too long)
        assert!(!host_name_validate((rep("a.", 127) + "ab").as_bytes())); // Total length 256 (too long)
        assert!(!host_name_validate((rep("a.", 127) + "ab.").as_bytes())); // Total length 256 + trailing dot (too long)
    }

    // parity S71-F1 / S71-F3: `isValidHost` over inputs whose verdicts were
    // MEASURED by running the reference's `isValidHost` body (HostName.validate
    // then IpAddress.parseIp6) under Zig 0.16.0.
    #[test]
    fn is_valid_host_matches_the_reference_std_parsers() {
        // 254- and 255-byte hostnames are valid cache keys (the port's former
        // limit was 253); 256 is not.
        let h254 = "a.".repeat(126) + "aa";
        let h255 = "a.".repeat(127) + "a";
        let h256 = "a.".repeat(127) + "aa";
        assert_eq!((254, 255, 256), (h254.len(), h255.len(), h256.len()));
        assert!(is_valid_host(h254.as_bytes()));
        assert!(is_valid_host(h255.as_bytes()));
        assert!(!is_valid_host(h256.as_bytes()));
        assert!(DiskCache::is_valid_cache_key(
            format!("user@{h255}").as_bytes()
        ));

        let cases: &[(&str, bool)] = &[
            ("localhost", true),
            ("::1", true),
            ("::", true),
            ("1::", true),
            ("::1.2.3.4", false),
            ("64:ff9b::1.2.3.4", false),
            ("::ffff:1.2.3.4", true),
            ("::FFFF:1.2.3.4", true),
            ("::ffff:1.2.3", false),
            ("::ffff:01.2.3.4", false),
            ("1:2:3:4:5:6:7:8", true),
            ("1:2:3:4:5:6:7:8:9", false),
            ("1:2:3:4:5:6:7::", true),
            ("1:2:3:4:5:6:7:8::", false),
            ("::1:2:3:4:5:6:7:8", true),
            ("1::2::3", false),
            ("12345::1", false),
            ("2001:DB8::1", true),
            ("fe80::1%eth0", false),
            (":1", false),
            ("1:", false),
            (":::", false),
            ("1:::2", false),
            ("abcd:ef01:2345:6789:abcd:ef01:2345:6789", true),
            ("g::1", false),
            ("1.2.3.4", true),
            ("::ffff:1.2.3.4.5", false),
            ("::ffff:", false),
            ("ffff::1.2.3.4", false),
            ("0:0:0:0:0:ffff:1.2.3.4", false),
            // Uncompressed and short: `.incomplete` (MEASURED with Zig 0.16.0).
            ("1:2:3", false),
            ("1:2:3:4:5:6:7", false),
        ];
        for &(host, want) in cases {
            assert_eq!(want, is_valid_host(host.as_bytes()), "{host}");
        }
    }

    // parity S71-F4: the reference's `add` opens an existing cache file with a
    // plain O_CREAT open (`.exclusive = false`) and never chmods it; the mode
    // only becomes 0600 when the atomic rewrite replaces the file. Observable
    // when the rewrite then fails: here the directory is read-only, so the
    // file opens but the temp sibling cannot be created, and the existing
    // file must keep its 0644.
    #[cfg(unix)]
    #[test]
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (rustty-cli, parity tail C-1): `mkdir` is forbidden under Miri isolation; runs natively in cargo test --workspace (see MIRI_EXCEPTIONS.md)"
    )]
    fn add_does_not_chmod_an_existing_cache_file() {
        use std::os::unix::fs::PermissionsExt;
        // Root ignores directory permissions, so the rewrite would not fail and
        // the test could not observe anything. No supported test environment
        // runs as root (CI runners, the Linux VM and the nix sandbox all use an
        // ordinary user), so R14: fail loudly there instead of skipping.
        // SAFETY(ffi-boundary/test): `geteuid` has no preconditions.
        assert_ne!(
            0,
            unsafe { libc::geteuid() },
            "add_does_not_chmod_an_existing_cache_file needs a non-root user: \
             root ignores the read-only directory it relies on"
        );
        let tmp = TmpDir::new();
        let dir = tmp.join("ro");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("cache");
        std::fs::write(&file, b"").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        let path = std::fs::canonicalize(&file).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();

        let cache = DiskCache {
            path: path_bytes(&path),
        };
        let res = cache.add(b"example.com", b"v1", timestamp());
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();

        // Positive reach: `add` opened the file and failed later, at the temp
        // file in the read-only directory.
        match res {
            Err(Error::Io(e)) => assert_eq!(std::io::ErrorKind::PermissionDenied, e.kind()),
            other => panic!("expected a permission error from the rewrite, got {other:?}"),
        }
        assert_eq!(0o644, mode);
    }

    // R13 (UPSTREAM-DEFECTS.md UD160): the reference's IPv6 parser indexes the
    // text with a `u8` and `@intCast`s the length into it after a `%`, which
    // traps for any text over 255 bytes (MEASURED with Zig 0.16.0 ReleaseSafe).
    // The port rejects it like any scoped address.
    #[test]
    fn long_scoped_ipv6_is_rejected_without_crashing() {
        let long = "fe80::1%".to_string() + &"a".repeat(300);
        // Positive reach: the text is not a hostname, so the IPv6 parser runs,
        // and its `%` arm is what rejects it (a short scope is rejected too).
        assert!(!host_name_validate(long.as_bytes()));
        assert!(!ip6_parse(long.as_bytes()));
        assert!(!is_valid_host(long.as_bytes()));
        assert!(!DiskCache::is_valid_cache_key(long.as_bytes()));
    }

    // test isValidUser
    #[test]
    fn is_valid_user_test() {
        // Valid
        assert!(is_valid_user(b"user"));
        assert!(is_valid_user(b"user-user"));
        assert!(is_valid_user(b"user_name"));
        assert!(is_valid_user(b"user.name"));
        assert!(is_valid_user(b"user123"));

        // Invalid
        assert!(!is_valid_user(b""));
        assert!(!is_valid_user(b"user name"));
        assert!(!is_valid_user(b"user@example"));
        assert!(!is_valid_user(b"user:group"));
        assert!(!is_valid_user(b"user\nname"));
        assert!(!is_valid_user(&b"a".repeat(65))); // too long
    }

    // test isValidCacheKey
    #[test]
    fn is_valid_cache_key_test() {
        // Valid
        assert!(DiskCache::is_valid_cache_key(b"example.com"));
        assert!(DiskCache::is_valid_cache_key(b"sub.example.com"));
        assert!(DiskCache::is_valid_cache_key(b"192.168.1.1"));
        assert!(DiskCache::is_valid_cache_key(b"::1"));
        assert!(DiskCache::is_valid_cache_key(b"user@example.com"));
        assert!(DiskCache::is_valid_cache_key(b"user@192.168.1.1"));
        assert!(DiskCache::is_valid_cache_key(b"user@::1"));

        // Invalid
        assert!(!DiskCache::is_valid_cache_key(b""));
        assert!(!DiskCache::is_valid_cache_key(b".example.com"));
        assert!(!DiskCache::is_valid_cache_key(b"@example.com"));
        assert!(!DiskCache::is_valid_cache_key(b"user@"));
        assert!(!DiskCache::is_valid_cache_key(b"user@@example"));
        assert!(!DiskCache::is_valid_cache_key(b"user@.example.com"));
    }
}
