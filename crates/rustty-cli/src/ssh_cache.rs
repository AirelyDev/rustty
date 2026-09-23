//! rustty-cli::ssh_cache
//!
//! PORTS: ghostty src/cli/ssh_cache.zig — the `+ssh-cache` action (list / add /
//! remove / prune / query the SSH terminfo cache). Byte-faithful (R1); paths and
//! destinations are byte strings (R3). Product identity (app name / terminfo
//! token) routes through `rustty-identity` (R2 / ADV-FIX 5).

// const std = @import("std");
// const fs = std.fs;
// const Allocator = std.mem.Allocator;
// const args = @import("args.zig");
use rustty_args::args;
// const Action = @import("ghostty.zig").Action;
use crate::rustty::{Action, Error};
// const Duration = @import("../config.zig").Config.Duration;
use rustty_config::value_types::Duration;
// const terminfopkg = @import("../terminfo/main.zig");
use rustty_terminfo as terminfopkg;
// pub const Entry = @import("ssh-cache/Entry.zig");
pub mod entry;
pub use entry::Entry;
// pub const DiskCache = @import("ssh-cache/DiskCache.zig");
pub mod disk_cache;
pub use disk_cache::DiskCache;

use std::collections::HashMap;
use std::io::Write;

// pub const Options = struct {
#[derive(Default)]
pub struct Options {
    // clear: bool = false,
    pub clear: bool,
    // add: ?[]const u8 = null,
    pub add: Option<Vec<u8>>,
    // remove: ?[]const u8 = null,
    pub remove: Option<Vec<u8>>,
    // prune: ?Duration = null,
    pub prune: Option<Duration>,
}

impl Options {
    // pub fn deinit(self: *Options) void { _ = self; }
    pub fn deinit(&self) {
        let _ = self;
    }

    // pub fn help(self: Options) !void { _ = self; return Action.help_error; }
    //   -> the `args::Target::help`/`has_help_decl` hooks below.
}

impl args::Target for Options {
    // inline for (info.@"struct".fields) — the four non-`_` fields.
    fn parse_into_field(
        &mut self,
        key: &[u8],
        value: Option<&[u8]>,
    ) -> Result<(), args::ParseError> {
        match key {
            // clear: bool
            b"clear" => {
                // An empty (set-but-empty) value resets to the default (false),
                // as the generic `parseIntoField` rule does (parity S70-F3).
                if args::is_reset_to_default(value) {
                    self.clear = false;
                    return Ok(());
                }
                self.clear = args::parse_field_bool(value)?;
                Ok(())
            }
            // add: ?[]const u8 — an empty (set-but-empty) value resets to null.
            b"add" => {
                if args::is_reset_to_default(value) {
                    self.add = None;
                    return Ok(());
                }
                self.add = Some(args::parse_field_string(value)?);
                Ok(())
            }
            // remove: ?[]const u8
            b"remove" => {
                if args::is_reset_to_default(value) {
                    self.remove = None;
                    return Ok(());
                }
                self.remove = Some(args::parse_field_string(value)?);
                Ok(())
            }
            // prune: ?Duration — the field type has a `parseCLI` (1-arg), and an
            // empty value resets the optional to null.
            b"prune" => {
                if args::is_reset_to_default(value) {
                    self.prune = None;
                    return Ok(());
                }
                self.prune = Some(Duration::parse_cli(value)?);
                Ok(())
            }
            // return error.InvalidField;
            _ => Err(args::Error::InvalidField),
        }
    }

    // @hasDecl(Options, "help") == true.
    fn has_help_decl(&self) -> bool {
        true
    }

    // pub fn help(self: Options) !void { return Action.help_error; }
    fn help(&mut self) -> Option<Result<(), args::ParseError>> {
        let _ = Action::Help;
        Some(Err(args::Error::ActionHelpRequested))
    }
}

/// Manage the SSH terminfo cache for automatic remote host setup.
///
/// The `+ssh` action installs Rustty's terminfo on remote hosts and records
/// each success in this cache so it doesn't re-upload on later connections.
/// (`+ssh` runs automatically from the shell integration when
/// `shell-integration-features` includes `ssh-terminfo`.) This command
/// inspects and maintains that cache.
///
/// The cache stores destinations (a hostname or user@hostname) along with
/// timestamps.
///
/// A positional destination queries the cache: `user@hostname` shows that
/// exact entry, while a bare `hostname` shows every cached entry for that
/// host regardless of user. With no destination and no action, the entire
/// cache is listed. A query that matches nothing exits 1.
///
/// At most one action (`--clear`, `--add`, `--remove`, or `--prune`) may be
/// specified, and not together with a positional destination; combining them
/// is an error.
///
/// `--prune` takes a duration with unit suffixes (`s`, `m`, `h`, `d`, `w`,
/// `y`) and removes every entry older than it, e.g. `--prune=30d`,
/// `--prune=6h`, `--prune=1y`.
///
/// Examples:
///
///     rustty +ssh-cache                           # List all cached destinations
///     rustty +ssh-cache user@example.com          # Show that destination
///     rustty +ssh-cache example.com               # Show all users on that host
///     rustty +ssh-cache --add=user@example.com    # Manually add a destination
///     rustty +ssh-cache --remove=user@example.com # Remove a destination
///     rustty +ssh-cache --prune=30d               # Remove entries older than 30 days
///     rustty +ssh-cache --clear                   # Clear entire cache
// pub fn run(alloc_gpa: Allocator) !u8 {
pub fn run() -> Result<u8, Error> {
    // var arena = std.heap.ArenaAllocator.init(alloc_gpa);
    // defer arena.deinit();
    // const alloc = arena.allocator();
    //   PORT NOTE: the arena allocator is dropped tree-wide; each collected value
    //   owns its bytes.

    // var opts: Options = .{};
    // defer opts.deinit();
    let mut opts = Options::default();

    // var stdout_buffer: [1024]u8 = undefined; ... const stdout = &stdout_writer.interface;
    let mut stdout = std::io::BufWriter::new(std::io::stdout().lock());
    // var stderr_buffer: [1024]u8 = undefined; ... const stderr = &stderr_writer.interface;
    let mut stderr = std::io::BufWriter::new(std::io::stderr().lock());

    // The cache is queried by a positional destination (`user@host` or a
    // bare `host`). `args.parse` rejects non-`--` tokens, so we lift the
    // positional out here and parse only the remaining flags. `--host=X`
    // is accepted as a deprecated spelling of the positional (it was the
    // original shipped flag name).
    // var query: ?[]const u8 = null;
    let mut query: Option<Vec<u8>> = None;
    // var flags: std.ArrayList([]const u8) = .empty;
    let mut flags: Vec<Vec<u8>> = Vec::new();
    {
        // var iter = try args.argsIterator(alloc_gpa);
        // defer iter.deinit();
        let mut iter = args::args_iterator()?;
        // while (iter.next()) |arg| {
        while let Some(arg) = iter.next() {
            let arg: &[u8] = &arg;
            // const is_host_flag = std.mem.startsWith(u8, arg, "--host=");
            let is_host_flag = arg.starts_with(b"--host=");
            if is_host_flag {
                // try stderr.print(
                //     "Warning: --host is deprecated; pass the destination " ++
                //         "directly, e.g. `ghostty +ssh-cache {s}`.\n",
                //     .{arg["--host=".len..]},
                // );
                //   R2: the `ghostty` binary name -> the Rustty product name.
                let _ = writeln!(
                    stderr,
                    "Warning: --host is deprecated; pass the destination directly, e.g. `{} +ssh-cache {}`.",
                    rustty_identity::APP_NAME,
                    String::from_utf8_lossy(&arg["--host=".len()..]),
                );
            }
            // const dest: ?[]const u8 = if (is_host_flag) arg["--host=".len..]
            //     else if (!std.mem.startsWith(u8, arg, "-")) arg else null;
            let dest: Option<&[u8]> = if is_host_flag {
                Some(&arg["--host=".len()..])
            } else if !arg.starts_with(b"-") {
                Some(arg)
            } else {
                None
            };

            // if (dest) |d| {
            if let Some(d) = dest {
                // if (query != null) {
                if query.is_some() {
                    // try stderr.print("Error: only one destination may be specified.\n", .{});
                    // stderr.flush() catch {};
                    // return 2;
                    let _ = writeln!(stderr, "Error: only one destination may be specified.");
                    let _ = stderr.flush();
                    iter.deinit();
                    return Ok(2);
                }
                // query = try alloc.dupe(u8, d);
                query = Some(d.to_vec());
            } else {
                // try flags.append(alloc, try alloc.dupe(u8, arg));
                flags.push(arg.to_vec());
            }
        }
        iter.deinit();
    }

    {
        // var iter = args.sliceIterator(flags.items);
        let flag_refs: Vec<&[u8]> = flags.iter().map(|f| f.as_slice()).collect();
        let mut iter = args::slice_iterator(&flag_refs);
        // args.parse(Options, alloc_gpa, &opts, &iter) catch |err| switch (err) {
        match args::parse(&mut opts, &mut iter) {
            Ok(()) => {}
            // error.InvalidField => { stderr "Error: unknown flag.\n"; flush; return 2; },
            Err(args::Error::InvalidField) => {
                let _ = writeln!(stderr, "Error: unknown flag.");
                let _ = stderr.flush();
                return Ok(2);
            }
            // error.InvalidValue, error.ValueRequired => { stderr "Error: invalid flag value.\n"; flush; return 2; },
            Err(args::Error::InvalidValue) | Err(args::Error::ValueRequired) => {
                let _ = writeln!(stderr, "Error: invalid flag value.");
                let _ = stderr.flush();
                return Ok(2);
            }
            // else => return err,
            Err(other) => return Err(Error::from(other)),
        }
    }

    // Setup our disk cache to the standard location
    // const cache_path = DiskCache.defaultPath(alloc, "ghostty") catch |err| {
    //     try stderr.print(
    //         "Error: unable to determine the cache path: {t}\n",
    //         .{err},
    //     );
    //     stderr.flush() catch {};
    //     return 1;
    // };
    //   R2 / ADV-FIX 5: the program dir is the Rustty app name via identity.
    let cache_path = match DiskCache::default_path(rustty_identity::APP_NAME) {
        Ok(path) => path,
        Err(err) => {
            writeln!(stderr, "Error: unable to determine the cache path: {err}")
                .map_err(Error::from)?;
            let _ = stderr.flush();
            return Ok(1);
        }
    };

    // const result = runInner(alloc, opts, query, .{ .path = cache_path }, stdout, stderr);
    let result = run_inner(
        &opts,
        query.as_deref(),
        DiskCache { path: cache_path },
        &mut stdout,
        &mut stderr,
    );

    // Flushing *shouldn't* fail but...
    // stdout.flush() catch {};
    // stderr.flush() catch {};
    let _ = stdout.flush();
    let _ = stderr.flush();

    opts.deinit();

    // return result;
    result.map_err(Error::from)
}

// pub fn runInner(alloc, opts, query, cache: DiskCache, stdout: *std.Io.Writer, stderr: *std.Io.Writer) !u8 {
//
// PORT NOTE: generic over the output writers (the reference passes `*std.Io.Writer`;
// the unit tests pass in-memory writers). Every disk-cache failure is reported
// inline against `cache.path` and mapped to an exit code, so only writer I/O
// errors propagate.
fn run_inner<Wo: Write, We: Write>(
    opts: &Options,
    query: Option<&[u8]>,
    cache: DiskCache,
    stdout: &mut Wo,
    stderr: &mut We,
) -> Result<u8, std::io::Error> {
    // At most one action may be specified, and a query (positional
    // destination) is itself an action.
    // const action_count =
    //     @as(usize, @intFromBool(opts.clear)) + @intFromBool(opts.add != null) +
    //     @intFromBool(opts.remove != null) + @intFromBool(opts.prune != null) +
    //     @intFromBool(query != null);
    let action_count = (opts.clear as usize)
        + (opts.add.is_some() as usize)
        + (opts.remove.is_some() as usize)
        + (opts.prune.is_some() as usize)
        + (query.is_some() as usize);
    // if (action_count > 1) {
    if action_count > 1 {
        // try stderr.print(
        //     "Error: only one of a destination, --clear, --add, --remove, " ++
        //         "or --prune may be specified.\n", .{});
        // return 2;
        writeln!(
            stderr,
            "Error: only one of a destination, --clear, --add, --remove, or --prune may be specified."
        )?;
        return Ok(2);
    }

    // if (opts.clear) {
    if opts.clear {
        // cache.clear() catch |err| {
        //     try stderr.print(
        //         "Error: unable to clear cache '{s}': {t}\n",
        //         .{ cache.path, err },
        //     );
        //     return 1;
        // };
        if let Err(err) = cache.clear() {
            writeln!(
                stderr,
                "Error: unable to clear cache '{}': {}",
                String::from_utf8_lossy(&cache.path),
                err,
            )?;
            return Ok(1);
        }
        // return 0;
        return Ok(0);
    }

    // if (opts.add) |dest| {
    if let Some(dest) = &opts.add {
        // cache.add(alloc, dest, terminfopkg.version, std.Io.Timestamp.now(global.io(), .real).toSeconds()) catch |err| switch (err) {
        match cache.add(dest, terminfopkg::version().as_bytes(), timestamp()) {
            Ok(()) => {}
            // error.InvalidCacheKey => { stderr "Error: invalid destination '{s}' ..."; return 2; },
            Err(disk_cache::Error::InvalidCacheKey) => {
                writeln!(
                    stderr,
                    "Error: invalid destination '{}' (expected hostname or user@hostname)",
                    String::from_utf8_lossy(dest),
                )?;
                return Ok(2);
            }
            // else => { stderr "Error: unable to add '{s}' to cache '{s}': {t}\n"; return 1; },
            Err(err) => {
                writeln!(
                    stderr,
                    "Error: unable to add '{}' to cache '{}': {}",
                    String::from_utf8_lossy(dest),
                    String::from_utf8_lossy(&cache.path),
                    err,
                )?;
                return Ok(1);
            }
        }
        // return 0;
        return Ok(0);
    }

    // if (opts.remove) |dest| {
    if let Some(dest) = &opts.remove {
        // const removed = cache.remove(alloc, dest) catch |err| switch (err) {
        let removed = match cache.remove(dest) {
            Ok(removed) => removed,
            // error.InvalidCacheKey => { stderr "Error: invalid destination ..."; return 2; },
            Err(disk_cache::Error::InvalidCacheKey) => {
                writeln!(
                    stderr,
                    "Error: invalid destination '{}' (expected hostname or user@hostname)",
                    String::from_utf8_lossy(dest),
                )?;
                return Ok(2);
            }
            // else => { stderr "Error: unable to remove '{s}' from cache '{s}': {t}\n"; return 1; },
            Err(err) => {
                writeln!(
                    stderr,
                    "Error: unable to remove '{}' from cache '{}': {}",
                    String::from_utf8_lossy(dest),
                    String::from_utf8_lossy(&cache.path),
                    err,
                )?;
                return Ok(1);
            }
        };
        // Silence on success; a no-op removal is an error (exit 1).
        // if (!removed) { try stderr.print("Error: '{s}' is not in the cache.\n", .{dest}); return 1; }
        if !removed {
            writeln!(
                stderr,
                "Error: '{}' is not in the cache.",
                String::from_utf8_lossy(dest),
            )?;
            return Ok(1);
        }
        // return 0;
        return Ok(0);
    }

    // if (opts.prune) |max_age| {
    if let Some(max_age) = opts.prune {
        // const max_age_s = max_age.duration / std.time.ns_per_s;
        let max_age_s = max_age.duration / NS_PER_S;
        // if (max_age_s == 0) { stderr "Error: --prune requires a duration of at least one second.\n"; return 2; }
        if max_age_s == 0 {
            writeln!(
                stderr,
                "Error: --prune requires a duration of at least one second."
            )?;
            return Ok(2);
        }
        // const pruned = cache.prune(alloc, max_age_s) catch |err| {
        //     try stderr.print(
        //         "Error: unable to prune cache '{s}': {t}\n",
        //         .{ cache.path, err },
        //     );
        //     return 1;
        // };
        let pruned = match cache.prune(max_age_s) {
            Ok(pruned) => pruned,
            Err(err) => {
                writeln!(
                    stderr,
                    "Error: unable to prune cache '{}': {}",
                    String::from_utf8_lossy(&cache.path),
                    err,
                )?;
                return Ok(1);
            }
        };
        // try stdout.print("Pruned cache entries: {d}\n", .{pruned});
        writeln!(stdout, "Pruned cache entries: {pruned}")?;
        // return 0;
        return Ok(0);
    }

    // var entries = cache.list(alloc) catch |err| {
    //     try stderr.print(
    //         "Error: unable to read cache '{s}': {t}\n",
    //         .{ cache.path, err },
    //     );
    //     return 1;
    // };
    // defer DiskCache.deinitEntries(alloc, &entries);
    let mut entries = match cache.list() {
        Ok(entries) => entries,
        Err(err) => {
            writeln!(
                stderr,
                "Error: unable to read cache '{}': {}",
                String::from_utf8_lossy(&cache.path),
                err,
            )?;
            return Ok(1);
        }
    };

    // A positional query filters the listing: an exact `user@host` match,
    // or every entry on a bare `host`.
    // if (query) |q| {
    if let Some(q) = query {
        // if (!DiskCache.isValidCacheKey(q)) { stderr "Error: invalid destination ..."; return 2; }
        if !DiskCache::is_valid_cache_key(q) {
            writeln!(
                stderr,
                "Error: invalid destination '{}' (expected hostname or user@hostname)",
                String::from_utf8_lossy(q),
            )?;
            DiskCache::deinit_entries(&mut entries);
            return Ok(2);
        }

        // var matches: std.StringHashMap(Entry) = .init(alloc);
        // defer matches.deinit();
        //   PORT NOTE: the reference `matches` holds shallow copies that borrow
        //   `entries`' bytes and `matches.deinit()` frees only the map buckets.
        //   The port clones each matched `Entry` into `matches` (owned); dropping
        //   `matches` frees the clones — observably identical, no double-free.
        let mut matches: HashMap<Vec<u8>, Entry> = HashMap::new();
        // var iter = entries.iterator();
        // while (iter.next()) |kv| {
        //     const key = kv.key_ptr.*;
        //     if (matchesQuery(key, q)) try matches.put(key, kv.value_ptr.*);
        // }
        for (key, value) in entries.iter() {
            if matches_query(key, q) {
                matches.insert(key.clone(), value.clone());
            }
        }

        // if (matches.count() == 0) return 1;
        if matches.is_empty() {
            DiskCache::deinit_entries(&mut entries);
            return Ok(1);
        }
        // try listEntries(alloc, &matches, stdout);
        list_entries(&matches, stdout)?;
        // return 0;
        DiskCache::deinit_entries(&mut entries);
        return Ok(0);
    }

    // List all destinations by default.
    // try listEntries(alloc, &entries, stdout);
    list_entries(&entries, stdout)?;
    // return 0;
    DiskCache::deinit_entries(&mut entries);
    Ok(0)
}

// fn listEntries(alloc, entries: *const std.StringHashMap(Entry), writer: *std.Io.Writer) !void {
fn list_entries<W: Write>(
    entries: &HashMap<Vec<u8>, Entry>,
    writer: &mut W,
) -> std::io::Result<()> {
    // if (entries.count() == 0) return;
    if entries.is_empty() {
        return Ok(());
    }

    // Sort entries by hostname for consistent output
    // var items: std.ArrayList(Entry) = .empty; defer items.deinit(alloc);
    // var iter = entries.iterator();
    // while (iter.next()) |kv| { try items.append(alloc, kv.value_ptr.*); }
    let mut items: Vec<Entry> = entries.values().cloned().collect();

    // std.mem.sort(Entry, items.items, {}, struct { fn lessThan(_, a, b) bool {
    //     return std.mem.lessThan(u8, a.hostname, b.hostname); } }.lessThan);
    items.sort_by(|a, b| a.hostname.cmp(&b.hostname));

    // Align the timestamp column by padding destinations to the widest.
    // var widest: usize = 0;
    // for (items.items) |entry| { widest = @max(widest, entry.hostname.len); }
    let mut widest: usize = 0;
    for entry in &items {
        widest = widest.max(entry.hostname.len());
    }

    // const now = std.time.timestamp();
    let now = timestamp();
    // for (items.items) |entry| {
    for entry in &items {
        // try writer.print("{s}", .{entry.hostname});
        writer.write_all(&entry.hostname)?;
        // try writer.splatByteAll(' ', widest - entry.hostname.len + 2);
        let pad = widest - entry.hostname.len() + 2;
        for _ in 0..pad {
            writer.write_all(b" ")?;
        }

        // var iso_buf: [20]u8 = undefined; var age_buf: [32]u8 = undefined;
        // try writer.print("{s} ({s})\n", .{ formatTimestamp(&iso_buf, entry.timestamp),
        //     relativeAge(&age_buf, now, entry.timestamp) });
        let iso = format_timestamp(entry.timestamp);
        let age = relative_age(now, entry.timestamp);
        writer.write_all(&iso)?;
        writer.write_all(b" (")?;
        writer.write_all(&age)?;
        writer.write_all(b")\n")?;
    }
    Ok(())
}

/// Whether a cache `key` matches a positional `query`. A `user@host` query
/// (containing `@`) matches one exact key; a bare `host` query matches every
/// key on that host regardless of user, comparing against the key's host
/// component (everything after its first `@`, or the whole key if userless).
// fn matchesQuery(key: []const u8, query: []const u8) bool {
fn matches_query(key: &[u8], query: &[u8]) -> bool {
    // if (std.mem.indexOfScalar(u8, query, '@') != null) { return std.mem.eql(u8, key, query); }
    if query.contains(&b'@') {
        return key == query;
    }

    // const at = std.mem.indexOfScalar(u8, key, '@');
    // const host = if (at) |i| key[i + 1 ..] else key;
    let host = match key.iter().position(|&c| c == b'@') {
        Some(i) => &key[i + 1..],
        None => key,
    };
    // return std.mem.eql(u8, host, query);
    host == query
}

/// Format a Unix timestamp as an ISO-8601 UTC string
/// (`YYYY-MM-DDTHH:MM:SSZ`) into `buf`, which must be at least 20 bytes.
/// Out-of-range input is clamped so this can't crash on a garbage cache line.
// fn formatTimestamp(buf: []u8, timestamp: i64) []const u8 {
//
// PORT NOTE: the reference formats into a caller `[20]u8`; the port returns the
// owning byte string. The `std.time.epoch` civil-date math is reproduced below.
fn format_timestamp(ts: i64) -> Vec<u8> {
    // Clamp to [epoch, last second of 9999-12-31Z]: `std.time.epoch`
    // accumulates the year in a `u16` (panics beyond that), and the buffer
    // only fits a 4-digit year.
    // const secs: u64 = @intCast(std.math.clamp(timestamp, 0, 253402300799));
    let secs: u64 = ts.clamp(0, 253402300799) as u64;

    // const epoch = std.time.epoch;
    // const epoch_secs: epoch.EpochSeconds = .{ .secs = secs };
    // const day = epoch_secs.getEpochDay();
    let epoch_day = secs / 86400;
    // const ds = epoch_secs.getDaySeconds();
    let day_seconds = secs % 86400;

    // const year_day = day.calculateYearDay();
    //   EpochDay.calculateYearDay: subtract whole years until the remaining day
    //   index fits inside the current year.
    let mut year_day = epoch_day;
    let mut year: u64 = 1970; // epoch.epoch_year
    loop {
        let year_size = days_in_year(year) as u64;
        if year_day < year_size {
            break;
        }
        year_day -= year_size;
        year += 1;
    }

    // const month_day = year_day.calculateMonthDay();
    //   YearAndDay.calculateMonthDay: subtract whole months until the remaining
    //   0-based day index fits inside the current month.
    let mut month: u64 = 1; // .jan
    let mut day_index = year_day;
    loop {
        let days = days_in_month(year, month) as u64;
        if day_index < days {
            break;
        }
        day_index -= days;
        month += 1;
    }

    // return std.fmt.bufPrint(buf, "{d:0>4}-{d:0>2}-{d:0>2}T{d:0>2}:{d:0>2}:{d:0>2}Z", .{
    //     year_day.year, month_day.month.numeric(), month_day.day_index + 1,
    //     ds.getHoursIntoDay(), ds.getMinutesIntoHour(), ds.getSecondsIntoMinute(),
    // }) catch unreachable;
    let hours = day_seconds / 3600;
    let minutes = (day_seconds % 3600) / 60;
    let seconds = day_seconds % 60;
    format!(
        "{year:0>4}-{month:0>2}-{:0>2}T{hours:0>2}:{minutes:0>2}:{seconds:0>2}Z",
        day_index + 1,
    )
    .into_bytes()
}

/// Format the age of `timestamp` (relative to `now`, both Unix seconds)
/// as a coarse relative time into `buf`, e.g. "2w ago". Uses `Duration`'s
/// unit vocabulary but keeps only the single largest unit for scannability.
/// A non-positive age (timestamp at or after `now`) is "now".
// fn relativeAge(buf: []u8, now: i64, timestamp: i64) []const u8 {
fn relative_age(now: i64, ts: i64) -> Vec<u8> {
    // Saturating so a garbage timestamp can't overflow; clamp at 0 so a
    // future timestamp becomes a zero age rather than going negative.
    // const age: u64 = @intCast(@max(0, now -| timestamp));
    let age: u64 = now.saturating_sub(ts).max(0) as u64;
    // if (age == 0) return "now";
    if age == 0 {
        return b"now".to_vec();
    }

    // Round down to the largest unit that fits, so Duration.format emits
    // only that unit (e.g. 19d -> 2w, 90m -> 1h).
    // const units = [_]u64{ 365 * s_per_day, s_per_week, s_per_day, s_per_hour, s_per_min, 1 };
    let units: [u64; 6] = [
        365 * S_PER_DAY, // y
        S_PER_WEEK,      // w
        S_PER_DAY,       // d
        S_PER_HOUR,      // h
        S_PER_MIN,       // m
        1,               // s
    ];
    // const unit = for (units) |u| { if (age >= u) break u; } else 1;
    let unit = {
        let mut chosen = 1u64;
        for u in units {
            if age >= u {
                chosen = u;
                break;
            }
        }
        chosen
    };

    // Cap the age so `age * ns_per_s` can't overflow u64 (a garbage, e.g.
    // hugely negative, timestamp otherwise yields an age near i64-max).
    // const max_age = std.math.maxInt(u64) / std.time.ns_per_s;
    let max_age = u64::MAX / NS_PER_S;
    // const rounded = @min(age, max_age) / unit * unit;
    let rounded = age.min(max_age) / unit * unit;
    // const d: Duration = .{ .duration = rounded * std.time.ns_per_s };
    let d = Duration {
        duration: rounded * NS_PER_S,
    };
    // return std.fmt.bufPrint(buf, "{f} ago", .{d}) catch unreachable;
    let mut out = Vec::new();
    d.format(&mut out);
    out.extend_from_slice(b" ago");
    out
}

// ---------------------------------------------------------------------------
// PORT NOTE: support shims for the std.time facilities used above.
// ---------------------------------------------------------------------------

// std.time nanosecond/second constants.
const NS_PER_S: u64 = 1_000_000_000;
const S_PER_MIN: u64 = 60;
const S_PER_HOUR: u64 = 60 * 60;
const S_PER_DAY: u64 = 24 * 60 * 60;
const S_PER_WEEK: u64 = 7 * S_PER_DAY;

/// `std.time.timestamp()` — current Unix time in whole seconds.
fn timestamp() -> i64 {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(e) => -(e.duration().as_secs() as i64),
    }
}

// `std.time.epoch.isLeapYear`.
//   The reference's `% == 0` divisibility tests are preserved verbatim (the
//   `is_multiple_of` clippy rewrite would obscure the ported expression).
#[allow(clippy::manual_is_multiple_of)]
fn is_leap_year(year: u64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

// `std.time.epoch.getDaysInYear`.
fn days_in_year(year: u64) -> u32 {
    if is_leap_year(year) {
        366
    } else {
        365
    }
}

// `std.time.epoch.getDaysInMonth` (month is 1-based).
fn days_in_month(year: u64, month: u64) -> u32 {
    match month {
        1 => 31,
        2 => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        }
        3 => 31,
        4 => 30,
        5 => 31,
        6 => 30,
        7 => 31,
        8 => 31,
        9 => 30,
        10 => 31,
        11 => 30,
        12 => 31,
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // `std.mem.indexOf` for the stderr-substring assertions.
    fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        haystack.windows(needle.len()).position(|w| w == needle)
    }

    // test matchesQuery
    #[test]
    fn matches_query_test() {
        // Exact user@host: only the identical key.
        assert!(matches_query(b"user@example.com", b"user@example.com"));
        assert!(!matches_query(b"root@example.com", b"user@example.com"));
        assert!(!matches_query(b"example.com", b"user@example.com"));

        // Bare host: every key on that host, plus a keyless entry for it.
        assert!(matches_query(b"user@example.com", b"example.com"));
        assert!(matches_query(b"root@example.com", b"example.com"));
        assert!(matches_query(b"example.com", b"example.com"));
        assert!(!matches_query(b"user@other.com", b"example.com"));
    }

    // test formatTimestamp
    #[test]
    fn format_timestamp_test() {
        // try testing.expectEqualStrings("2026-05-05T22:49:33Z", formatTimestamp(&buf, 1778021373));
        assert_eq!(
            b"2026-05-05T22:49:33Z".as_slice(),
            format_timestamp(1778021373).as_slice(),
        );

        // Epoch.
        // try testing.expectEqualStrings("1970-01-01T00:00:00Z", formatTimestamp(&buf, 0));
        assert_eq!(
            b"1970-01-01T00:00:00Z".as_slice(),
            format_timestamp(0).as_slice(),
        );

        // Out-of-range inputs clamp instead of overflowing the [20]u8 /
        // panicking inside std: negatives floor at the epoch, huge values cap
        // at the last second of year 9999.
        // try testing.expectEqualStrings("1970-01-01T00:00:00Z", formatTimestamp(&buf, -5));
        assert_eq!(
            b"1970-01-01T00:00:00Z".as_slice(),
            format_timestamp(-5).as_slice(),
        );
        // try testing.expectEqualStrings("9999-12-31T23:59:59Z", formatTimestamp(&buf, std.math.maxInt(i64)));
        assert_eq!(
            b"9999-12-31T23:59:59Z".as_slice(),
            format_timestamp(i64::MAX).as_slice(),
        );
    }

    // test relativeAge
    #[test]
    fn relative_age_test() {
        // const now: i64 = 2_000_000_000; // fixed reference
        let now: i64 = 2_000_000_000;
        // const min = std.time.s_per_min; const hour = std.time.s_per_hour; const day = std.time.s_per_day;
        let min = S_PER_MIN as i64;
        let hour = S_PER_HOUR as i64;
        let day = S_PER_DAY as i64;

        // Out-of-range timestamps don't crash: a huge future one saturates to
        // a non-positive age ("now"); a negative one is a large but real age.
        // try testing.expectEqualStrings("now", relativeAge(&buf, now, std.math.maxInt(i64)));
        assert_eq!(b"now".as_slice(), relative_age(now, i64::MAX).as_slice());
        // try testing.expectEqualStrings("63y ago", relativeAge(&buf, now, -100));
        assert_eq!(b"63y ago".as_slice(), relative_age(now, -100).as_slice());

        // A huge age (garbage timestamp) saturates the ns conversion instead of
        // overflowing; it must not crash and must fit the buffer.
        // try testing.expect(std.mem.endsWith(u8, relativeAge(&buf, std.math.maxInt(i64), 0), " ago"));
        assert!(relative_age(i64::MAX, 0).ends_with(b" ago"));

        // Future timestamp (clock skew) and same-instant read "now".
        // try testing.expectEqualStrings("now", relativeAge(&buf, now, now + 100));
        assert_eq!(b"now".as_slice(), relative_age(now, now + 100).as_slice());
        // try testing.expectEqualStrings("now", relativeAge(&buf, now, now));
        assert_eq!(b"now".as_slice(), relative_age(now, now).as_slice());

        // Only the single largest unit is kept (smaller units rounded away).
        // try testing.expectEqualStrings("30s ago", relativeAge(&buf, now, now - 30));
        assert_eq!(
            b"30s ago".as_slice(),
            relative_age(now, now - 30).as_slice()
        );
        // try testing.expectEqualStrings("1m ago", relativeAge(&buf, now, now - min));
        assert_eq!(
            b"1m ago".as_slice(),
            relative_age(now, now - min).as_slice()
        );
        // try testing.expectEqualStrings("1m ago", relativeAge(&buf, now, now - 90)); // 90s -> 1m
        assert_eq!(b"1m ago".as_slice(), relative_age(now, now - 90).as_slice());
        // try testing.expectEqualStrings("1h ago", relativeAge(&buf, now, now - hour));
        assert_eq!(
            b"1h ago".as_slice(),
            relative_age(now, now - hour).as_slice()
        );
        // try testing.expectEqualStrings("1h ago", relativeAge(&buf, now, now - (hour + 30 * min))); // 1h30m -> 1h
        assert_eq!(
            b"1h ago".as_slice(),
            relative_age(now, now - (hour + 30 * min)).as_slice(),
        );
        // try testing.expectEqualStrings("1d ago", relativeAge(&buf, now, now - day));
        assert_eq!(
            b"1d ago".as_slice(),
            relative_age(now, now - day).as_slice()
        );
        // try testing.expectEqualStrings("2w ago", relativeAge(&buf, now, now - 19 * day)); // 19d -> 2w
        assert_eq!(
            b"2w ago".as_slice(),
            relative_age(now, now - 19 * day).as_slice(),
        );
    }

    // test { _ = DiskCache; _ = Entry; }
    #[test]
    fn ref_all_decls() {
        // Reference every re-exported declaration so it is analyzed by the test
        // build (the reference's anonymous reify `test { _ = DiskCache; _ = Entry; }`).
        let _: fn(&[u8]) -> bool = DiskCache::is_valid_cache_key;
        let _ = Entry::parse(b"a|1|b");
    }

    // parity S70-F3: `--clear=` (set but empty) resets `clear` to its default
    // (`false`), the generic `parseIntoField` rule, instead of failing as a bool.
    #[test]
    fn empty_clear_resets_to_default() {
        use args::Target;
        let mut opts = Options::default();
        opts.parse_into_field(b"clear", Some(b"true")).unwrap();
        // Positive reach: `clear` really left its default first.
        assert!(opts.clear);
        opts.parse_into_field(b"clear", Some(b"")).unwrap();
        assert!(!opts.clear);
    }

    // test "runInner rejects multiple actions"
    #[test]
    fn run_inner_rejects_multiple_actions() {
        // var stdout: std.Io.Writer.Allocating = .init(alloc); var stderr: ...;
        let mut stdout: Vec<u8> = Vec::new();
        let mut stderr: Vec<u8> = Vec::new();

        // The check runs before any cache access, so it never touches disk.
        // const cache: DiskCache = .{ .path = "/nonexistent/ssh_cache" };
        let cache = DiskCache {
            path: b"/nonexistent/ssh_cache".to_vec(),
        };
        // const code = try runInner(alloc, .{ .add = "example.com", .remove = "other.com" }, null, cache, ...);
        let opts = Options {
            add: Some(b"example.com".to_vec()),
            remove: Some(b"other.com".to_vec()),
            ..Default::default()
        };
        let code = run_inner(
            &opts,
            None,
            DiskCache {
                path: cache.path.clone(),
            },
            &mut stdout,
            &mut stderr,
        )
        .unwrap();

        // try testing.expectEqual(@as(u8, 2), code);
        assert_eq!(2u8, code);
        // try testing.expectEqualStrings("", stdout.written());
        assert_eq!(b"".as_slice(), stdout.as_slice());
        // try testing.expect(std.mem.indexOf(u8, stderr.written(), "only one") != null);
        assert!(find_subslice(&stderr, b"only one").is_some());

        // A positional query is itself an action: query + a flag conflicts.
        // stderr.clearRetainingCapacity();
        stderr.clear();
        // const code2 = try runInner(alloc, .{ .clear = true }, "example.com", cache, ...);
        let opts2 = Options {
            clear: true,
            ..Default::default()
        };
        let code2 = run_inner(
            &opts2,
            Some(b"example.com"),
            cache,
            &mut stdout,
            &mut stderr,
        )
        .unwrap();
        // try testing.expectEqual(@as(u8, 2), code2);
        assert_eq!(2u8, code2);
        // try testing.expect(std.mem.indexOf(u8, stderr.written(), "only one") != null);
        assert!(find_subslice(&stderr, b"only one").is_some());
    }

    // test "runInner fails cleanly when the cache is unusable"
    #[test]
    // The test's whole subject is a REAL directory standing where the cache
    // file belongs, so it must create one; `mkdir` is unavailable under Miri's
    // isolation. Same class and same wording as `list_themes.rs`'s file-opening
    // tests. Runs natively in ci.sh's HARD `cargo test --workspace` step.
    #[cfg_attr(miri, ignore = "creates a directory — Miri isolation")]
    fn run_inner_fails_cleanly_when_the_cache_is_unusable() {
        // A directory where the cache file belongs fails every operation.
        // var tmp = testing.tmpDir(.{});
        // defer tmp.cleanup();
        // try tmp.dir.createDirPath(io, "ssh_cache");
        // const tmp_path = try tmp.dir.realPathFileAlloc(io, ".", alloc);
        // const cache_path = try std.fs.path.join(alloc, &.{ tmp_path, "ssh_cache" });
        // const cache: DiskCache = .{ .path = cache_path };
        //
        // PORT NOTE (test-only glue): a per-test unique temp dir with Drop
        // cleanup reproduces `testing.tmpDir` (the parallel-runner isolation
        // lesson); the cache path points at the created DIRECTORY.
        let mut tmp_path = std::env::temp_dir();
        tmp_path.push(format!(
            "rustty-ssh-cache-unusable-{}-{}",
            std::process::id(),
            line!(),
        ));
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(tmp_path.clone());
        let dir_cache_path = tmp_path.join("ssh_cache");
        std::fs::create_dir_all(&dir_cache_path).unwrap();
        let cache_path_bytes: Vec<u8> = {
            #[cfg(unix)]
            {
                use std::os::unix::ffi::OsStrExt;
                dir_cache_path.as_os_str().as_bytes().to_vec()
            }
            #[cfg(not(unix))]
            {
                dir_cache_path.to_string_lossy().into_owned().into_bytes()
            }
        };

        // var stdout: std.Io.Writer.Allocating = .init(alloc); var stderr: ...;
        let mut stdout: Vec<u8> = Vec::new();
        let mut stderr: Vec<u8> = Vec::new();

        // const opts: []const Options = &.{
        //     .{}, // the default listing
        //     .{ .add = "example.com" },
        //     .{ .remove = "example.com" },
        //     .{ .prune = .{ .duration = std.time.ns_per_s } },
        // };
        let opts_list: [Options; 4] = [
            // the default listing
            Options::default(),
            Options {
                add: Some(b"example.com".to_vec()),
                ..Default::default()
            },
            Options {
                remove: Some(b"example.com".to_vec()),
                ..Default::default()
            },
            Options {
                prune: Some(Duration { duration: NS_PER_S }),
                ..Default::default()
            },
        ];
        // for (opts) |o| {
        //     const code = try runInner(alloc, o, null, cache, &stdout.writer, &stderr.writer);
        //     try testing.expectEqual(@as(u8, 1), code);
        // }
        for o in &opts_list {
            let code = run_inner(
                o,
                None,
                DiskCache {
                    path: cache_path_bytes.clone(),
                },
                &mut stdout,
                &mut stderr,
            )
            .unwrap();
            assert_eq!(1u8, code);
        }
    }
}
