//! rustty-cli::ssh_cache::disk_cache
//!
//! PORTS: ghostty src/cli/ssh-cache/DiskCache.zig — the on-disk SSH terminfo
//! entry cache. Byte-faithful (R1); paths are byte strings (`Vec<u8>`, R3). The
//! program dir (`ghostty`) and the default terminfo token (`xterm-ghostty`) are
//! routed through `rustty-identity` (R2 / ADV-FIX 5) so the on-disk path and the
//! cached terminfo name track the rustty product identity.
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
        // const state_dir: []const u8 = xdg.state(
        //     alloc,
        //     .{ .subdir = program },
        // ) catch |err| return switch (err) {
        //     error.OutOfMemory => error.OutOfMemory,
        //     else => error.XdgLookupFailed,
        // };
        // defer alloc.free(state_dir);
        let state_dir = xdg::state(xdg::Options {
            subdir: Some(program),
            ..Default::default()
        })
        .map_err(|_| Error::XdgLookupFailed)?;
        // return try std.fs.path.join(alloc, &.{ state_dir, "ssh_cache" });
        Ok(join_path(state_dir.as_bytes(), b"ssh_cache"))
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
    // pub fn add(self: DiskCache, alloc: Allocator, key: []const u8, timestamp: i64) !void {
    pub fn add(&self, key: &[u8], timestamp: i64) -> Result<(), Error> {
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
        // PORT NOTE: the port creates the file EXCLUSIVELY (`create_new`) so the
        // "already exists -> open + fixup" branch is reachable exactly as written.
        // Observable behavior is identical: a fresh file is created 0600, an
        // existing one (already 0600 from a prior add) is opened and left at 0600.
        // `file` is closed on drop (the reference's `defer file.close()`).
        let file = match create_new_secure(as_path(&self.path)) {
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
        // } else {
        //     const key_copy = try alloc.dupe(u8, key);
        //     errdefer alloc.free(key_copy);
        //     const terminfo_copy = try alloc.dupe(u8, "xterm-ghostty");
        //     errdefer alloc.free(terminfo_copy);
        //     try entries.put(key_copy, .{
        //         .hostname = key_copy,
        //         .timestamp = timestamp,
        //         .terminfo_version = terminfo_copy,
        //     });
        // }
        //   R2 / ADV-FIX 5: the cached terminfo token is the rustty identity's
        //   primary terminfo name (`xterm-rustty`). Ownership makes the dupe/
        //   errdefer bookkeeping implicit (the two `Vec<u8>` copies below).
        if let Some(existing) = entries.get_mut(key) {
            existing.timestamp = timestamp;
        } else {
            entries.insert(
                key.to_vec(),
                Entry {
                    hostname: key.to_vec(),
                    timestamp,
                    terminfo_version: rustty_identity::TERMINFO_PRIMARY_NAME.as_bytes().to_vec(),
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

    /// Check if a key exists in the cache.
    /// Returns false if the cache file doesn't exist.
    // pub fn contains(self: DiskCache, alloc: Allocator, key: []const u8) !bool {
    pub fn contains(&self, key: &[u8]) -> Result<bool, Error> {
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

        // return entries.contains(key);
        Ok(entries.contains_key(key))
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
    // if (internal_os.hostname.isValid(host)) { return true; }
    if rustty_os::hostname::isValid(host) {
        return true;
    }

    // We also accept valid IP addresses. In practice, IPv4 addresses are also
    // considered valid hostnames due to their overlapping syntax, so we can
    // simplify this check to be IPv6-specific.
    // if (std.net.Address.parseIp6(host, 0)) |_| { return true; } else |_| { return false; }
    std::str::from_utf8(host)
        .ok()
        .and_then(|s| s.parse::<std::net::Ipv6Addr>().ok())
        .is_some()
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
// PORT: support shims for the std facilities used above.
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

    // PORT (test-only glue): a per-test temporary directory with unique naming and
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
        //   R2 / ADV-FIX 5: the program dir is the rustty app name via identity.
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
        // try cache.add(alloc, "example.com", std.time.timestamp());
        cache.add(b"example.com", timestamp()).unwrap();
        // try cache.add(alloc, "example.com", std.time.timestamp());
        cache.add(b"example.com", timestamp()).unwrap();
        // try testing.expect(try cache.contains(alloc, "example.com"));
        assert!(cache.contains(b"example.com").unwrap());

        // List
        // var entries = try cache.list(alloc);
        // deinitEntries(alloc, &entries);
        let mut entries = cache.list().unwrap();
        DiskCache::deinit_entries(&mut entries);

        // Remove reports that it removed the entry, and a second remove of the
        // same key reports nothing to remove.
        // try testing.expect(try cache.remove(alloc, "example.com"));
        assert!(cache.remove(b"example.com").unwrap());
        // try testing.expect(!try cache.remove(alloc, "example.com"));
        assert!(!cache.remove(b"example.com").unwrap());
        // try testing.expect(!(try cache.contains(alloc, "example.com")));
        assert!(!cache.contains(b"example.com").unwrap());
        // try cache.add(alloc, "example.com", std.time.timestamp());
        cache.add(b"example.com", timestamp()).unwrap();
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
        // try cache.add(alloc, "example.com", std.time.timestamp());
        cache.add(b"example.com", timestamp()).unwrap();
        // try cache.add(alloc, "example.org", std.time.timestamp());
        cache.add(b"example.org", timestamp()).unwrap();

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
        // try cache.add(alloc, "recent.com", now - hour);
        cache.add(b"recent.com", now - hour).unwrap();
        // try cache.add(alloc, "old.com", now - 100 * day);
        cache.add(b"old.com", now - 100 * day).unwrap();

        // Prune entries older than 90 days: only old.com goes.
        // try testing.expectEqual(@as(usize, 1), try cache.prune(alloc, 90 * day));
        assert_eq!(1_usize, cache.prune((90 * day) as u64).unwrap());
        // try testing.expect(try cache.contains(alloc, "recent.com"));
        assert!(cache.contains(b"recent.com").unwrap());
        // try testing.expect(!try cache.contains(alloc, "old.com"));
        assert!(!cache.contains(b"old.com").unwrap());

        // Pruning again removes nothing.
        // try testing.expectEqual(@as(usize, 0), try cache.prune(alloc, 90 * day));
        assert_eq!(0_usize, cache.prune((90 * day) as u64).unwrap());

        // Sub-day granularity: a 30-minute max age prunes the hour-old entry.
        // try testing.expectEqual(@as(usize, 1), try cache.prune(alloc, 30 * std.time.s_per_min));
        assert_eq!(1_usize, cache.prune((30 * S_PER_MIN) as u64).unwrap());
        // try testing.expect(!try cache.contains(alloc, "recent.com"));
        assert!(!cache.contains(b"recent.com").unwrap());
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
        //   R2 / ADV-FIX 5: the older entry's terminfo token is the rustty
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
        // as the UB/leak backstop for the pure logic.
        let mut entries = cache.list().unwrap();
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
        //     if (cache.add(alloc, "user@example.com", 100)) |_| {
        //         if (!failing.has_induced_failure) break;
        //     } else |err| { try testing.expectEqual(error.OutOfMemory, err); }
        // }
        //
        // PORT NOTE: see the `reads survive allocation failure` note — Rust's
        // infallible allocation collapses the injection loop to its single
        // success run; ownership + Miri (R6) provide the leak/UB guarantee.
        cache.add(b"user@example.com", 100).unwrap();
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
