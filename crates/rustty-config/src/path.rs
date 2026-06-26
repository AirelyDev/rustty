//! rustty-config::path
//!
//! PORTS: ghostty src/config/path.zig — config path value-types (`Path`,
//! `RepeatablePath`). C1 leaf value-type (8 tests).
//!
//! PORT STATUS: ported (every line/comment/test{}, R1).

// const std = @import("std");
// const builtin = @import("builtin");
// const assert = @import("../quirks.zig").inlineAssert;
// const Allocator = std.mem.Allocator;
// const ArenaAllocator = std.heap.ArenaAllocator;
//
// const cli = @import("../cli.zig");
// const internal_os = @import("../os/main.zig");
// const formatterpkg = @import("formatter.zig");
use rustty_args::diagnostics::{Diagnostic, DiagnosticList};
use rustty_cfg_fmt::{format_entry_void, FormatEntry, Writer};
use rustty_util::quirks::inlineAssert as assert;

// const log = std.log.scoped(.config);

// pub const ParseError = error{ValueRequired} || Allocator.Error;
#[derive(Debug, PartialEq, Eq)]
pub enum ParseError {
    ValueRequired,
}

/// Path is like a string that represents a path value. The difference is that
/// when loading the configuration the value for this will be automatically
/// expanded relative to the path of the config file (or the home directory).
// pub const Path = union(enum) {
#[derive(Clone, PartialEq, Eq)]
pub enum Path {
    /// No error if the file does not exist.
    // optional: [:0]const u8,
    Optional(Vec<u8>),

    /// The file is required to exist.
    // required: [:0]const u8,
    Required(Vec<u8>),
}

impl Path {
    // pub fn len(self: Path) usize {
    //     return switch (self) {
    //         inline else => |path| path.len,
    //     };
    // }
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        match self {
            Path::Optional(path) | Path::Required(path) => path.len(),
        }
    }

    // pub fn equal(self: Path, other: Path) bool {
    //     return std.meta.eql(self, other);
    // }
    pub fn equal(&self, other: &Path) -> bool {
        self == other
    }

    // /// ghostty_config_path_s
    // pub const C = extern struct {
    //     path: [*:0]const u8,
    //     optional: bool,
    // };
    //
    // PORT NOTE: the `C`/`cval` C-ABI surface (`ghostty_config_path_s`) is a pure
    // FFI projection; following the established precedent (vt's C-ABI tagged-union
    // surfaces hoisted to `rustty-vt-c`), the `extern struct C` + `cval` belong to
    // the config C-ABI layer and are deferred until that layer is carved. The
    // plain Rust enum here is the source of truth; no behavior is lost.
    //
    // /// Returns the path as a C-compatible struct.
    // pub fn cval(self: Path) C {
    //     return switch (self) {
    //         .optional => |path| .{ .path = path.ptr, .optional = true },
    //         .required => |path| .{ .path = path.ptr, .optional = false },
    //     };
    // }

    /// Parse the input and return a Path. A leading `?` indicates that the path
    /// is _optional_ and an error should not be logged or displayed to the user
    /// if that path does not exist. Otherwise the path is required and an error
    /// should be logged if the path does not exist.
    // pub fn parse(
    //     /// Allocator to use. This must be an arena allocator because we assume
    //     /// that any allocations will be cleaned up when the arena.
    //     arena_alloc: Allocator,
    //     /// The input.
    //     input: ?[]const u8,
    // ) ParseError!?Path {
    pub fn parse(input: Option<&[u8]>) -> Result<Option<Path>, ParseError> {
        // var value = input orelse return error.ValueRequired;
        let mut value = input.ok_or(ParseError::ValueRequired)?;

        // if (value.len == 0) return null;
        if value.is_empty() {
            return Ok(None);
        }

        // const optional = if (value[0] == '?') opt: {
        //     value = value[1..];
        //     break :opt true;
        // } else false;
        let optional = if value[0] == b'?' {
            value = &value[1..];
            true
        } else {
            false
        };

        // if (value.len >= 2 and value[0] == '"' and value[value.len - 1] == '"') {
        //     value = value[1 .. value.len - 1];
        // }
        if value.len() >= 2 && value[0] == b'"' && value[value.len() - 1] == b'"' {
            value = &value[1..value.len() - 1];
        }

        // if (optional)
        //     return .{ .optional = try arena_alloc.dupeZ(u8, value) }
        // else
        //     return .{ .required = try arena_alloc.dupeZ(u8, value) };
        if optional {
            Ok(Some(Path::Optional(value.to_vec())))
        } else {
            Ok(Some(Path::Required(value.to_vec())))
        }
    }

    /// Parse CLI option.
    // pub fn parseCLI(
    //     /// The path. The value will be overwritten.
    //     self: *Path,
    //     /// Allocator to use. This must be an arena allocator because we assume
    //     /// that any allocations will be cleaned up when the arena.
    //     arena_alloc: Allocator,
    //     // The input.
    //     input: ?[]const u8,
    // ) ParseError!void {
    //     assert(input != null);
    //     const item = try parse(arena_alloc, input) orelse return;
    //     if (item.len() == 0) return;
    //     self.* = item;
    // }
    pub fn parse_cli(&mut self, input: Option<&[u8]>) -> Result<(), ParseError> {
        // assert(input != null);
        assert(input.is_some());
        // const item = try parse(arena_alloc, input) orelse return;
        let item = match Path::parse(input)? {
            Some(item) => item,
            None => return Ok(()),
        };
        // if (item.len() == 0) return;
        if item.len() == 0 {
            return Ok(());
        }
        // self.* = item;
        *self = item;
        Ok(())
    }

    // Used by formatter.
    // pub fn formatEntry(self: *const Path, formatter: formatterpkg.EntryFormatter) !void {
    //
    // PORT NOTE: implemented via the `FormatEntry` trait. See the impl below.

    /// Return a clone of the path.
    // pub fn clone(
    //     /// The path to clone.
    //     self: Path,
    //     /// This must be an arena allocator because we rely on the arena to
    //     /// clean up our allocations.
    //     arena_alloc: Allocator,
    // ) Allocator.Error!Path {
    //     return switch (self) {
    //         .optional => |path| .{ .optional = try arena_alloc.dupeZ(u8, path) },
    //         .required => |path| .{ .required = try arena_alloc.dupeZ(u8, path) },
    //     };
    // }
    pub fn clone_in(&self) -> Path {
        match self {
            Path::Optional(path) => Path::Optional(path.clone()),
            Path::Required(path) => Path::Required(path.clone()),
        }
    }

    /// Expand relative paths or paths prefixed with `~/`. The path will be
    /// overwritten.
    // pub fn expand(
    //     /// The path to expand.
    //     self: *Path,
    //     /// This must be an arena allocator because we rely on the arena to
    //     /// clean up our allocations.
    //     arena_alloc: Allocator,
    //     /// The base directory to expand relative paths. It must be an absolute
    //     /// path.
    //     base: []const u8,
    //     /// Errors will be added to the list of diagnostics if they occur.
    //     diags: *cli.DiagnosticList,
    // ) !void {
    pub fn expand(&mut self, base: &[u8], diags: &mut DiagnosticList) -> std::io::Result<()> {
        // assert(std.fs.path.isAbsolute(base));
        assert(is_absolute(base));

        // const path = switch (self.*) {
        //     .optional, .required => |path| path,
        // };
        let path: Vec<u8> = match self {
            Path::Optional(path) | Path::Required(path) => path.clone(),
        };

        // If it is already absolute we can ignore it.
        // if (path.len == 0 or std.fs.path.isAbsolute(path)) return;
        if path.is_empty() || is_absolute(&path) {
            return Ok(());
        }

        // If it isn't absolute, we need to make it absolute relative
        // to the base.
        // var buf: [std.fs.max_path_bytes]u8 = undefined;

        // Check if the path starts with a tilde and expand it to the
        // home directory on Linux/macOS. We explicitly look for "~/"
        // because we don't support alternate users such as "~alice/"
        // if (std.mem.startsWith(u8, path, "~/")) expand: {
        if path.starts_with(b"~/") {
            // PORT NOTE: the `expand:` labeled block is only `break`-ed by the
            // Windows arm below (`break :expand`); on non-Windows targets that
            // break is cfg'd out, so the label is unused there.
            #[allow(unused_labels)]
            'expand: {
                // Windows isn't supported yet
                // if (comptime builtin.os.tag == .windows) break :expand;
                #[cfg(target_os = "windows")]
                {
                    break 'expand;
                }

                // const expanded: []const u8 = internal_os.expandHome(path, &buf) catch |err| { ... };
                #[cfg(not(target_os = "windows"))]
                {
                    let mut buf = [0u8; libc_max_path_bytes()];
                    let expanded: &[u8] = match rustty_os::homedir::expandHome(&path, &mut buf) {
                        Ok(expanded) => expanded,
                        Err(err) => {
                            // try diags.append(arena_alloc, .{
                            //     .message = try std.fmt.allocPrintSentinel(
                            //         arena_alloc,
                            //         "error expanding home directory for path {s}: {}",
                            //         .{ path, err },
                            //         0,
                            //     ),
                            // });
                            let mut message: Vec<u8> = Vec::new();
                            message.extend_from_slice(b"error expanding home directory for path ");
                            message.extend_from_slice(&path);
                            message.extend_from_slice(b": ");
                            message.extend_from_slice(format!("{err:?}").as_bytes());
                            diags.append(Diagnostic {
                                message,
                                ..Diagnostic::default()
                            });

                            // Blank this path so that we don't attempt to resolve it
                            // again
                            // self.* = .{ .required = "" };
                            *self = Path::Required(Vec::new());

                            // return;
                            return Ok(());
                        }
                    };

                    // log.debug("expanding file path from home directory: path={s}", .{expanded});

                    // switch (self.*) {
                    //     .optional, .required => |*p| p.* = try arena_alloc.dupeZ(u8, expanded),
                    // }
                    let expanded_owned = expanded.to_vec();
                    match self {
                        Path::Optional(p) | Path::Required(p) => *p = expanded_owned,
                    }

                    // return;
                    return Ok(());
                }
            }
        }

        // var dir = try std.fs.openDirAbsolute(base, .{});
        // defer dir.close();
        let base_path = std::path::Path::new(os_str_from_bytes(base));

        // const abs = dir.realpath(path, &buf) catch |err| abs: {
        let joined = base_path.join(os_str_from_bytes(&path));
        let abs: Vec<u8> = match std::fs::canonicalize(&joined) {
            Ok(p) => path_to_bytes(&p),
            Err(err) => 'abs: {
                // if (err == error.FileNotFound) {
                if err.kind() == std::io::ErrorKind::NotFound {
                    // The file doesn't exist. Try to resolve the relative path
                    // another way.
                    // const resolved = try std.fs.path.resolve(arena_alloc, &.{ base, path });
                    // defer arena_alloc.free(resolved);
                    // @memcpy(buf[0..resolved.len], resolved);
                    // break :abs buf[0..resolved.len];
                    let resolved = resolve(&[base, &path]);
                    break 'abs resolved;
                }

                // try diags.append(arena_alloc, .{
                //     .message = try std.fmt.allocPrintSentinel(
                //         arena_alloc,
                //         "error resolving file path {s}: {}",
                //         .{ path, err },
                //         0,
                //     ),
                // });
                let mut message: Vec<u8> = Vec::new();
                message.extend_from_slice(b"error resolving file path ");
                message.extend_from_slice(&path);
                message.extend_from_slice(b": ");
                message.extend_from_slice(format!("{err:?}").as_bytes());
                diags.append(Diagnostic {
                    message,
                    ..Diagnostic::default()
                });

                // Blank this path so that we don't attempt to resolve it again
                // self.* = .{ .required = "" };
                *self = Path::Required(Vec::new());

                // return;
                return Ok(());
            }
        };

        // log.debug("expanding file path relative={s} abs={s}", .{ path, abs });

        // switch (self.*) {
        //     .optional, .required => |*p| p.* = try arena_alloc.dupeZ(u8, abs),
        // }
        match self {
            Path::Optional(p) | Path::Required(p) => *p = abs,
        }

        Ok(())
    }
}

impl FormatEntry for Path {
    /// Used by formatter.
    // pub fn formatEntry(self: *const Path, formatter: formatterpkg.EntryFormatter) !void {
    fn format_entry<W: Writer>(&self, name: &[u8], writer: &mut W) -> Result<(), W::Error> {
        // var buf: [std.fs.max_path_bytes + 1]u8 = undefined;
        // const value = switch (self.*) {
        //     .optional => |path| std.fmt.bufPrint(&buf, "?{s}", .{path}) catch |err| switch (err) {
        //         // Required for builds on Linux where NoSpaceLeft
        //         // isn't an allowed error for fmt.
        //         error.NoSpaceLeft => return error.OutOfMemory,
        //     },
        //     .required => |path| path,
        // };
        //
        // try formatter.formatEntry([]const u8, value);
        match self {
            Path::Optional(path) => {
                // std.fmt.bufPrint(&buf, "?{s}", .{path})
                let mut value: Vec<u8> = Vec::with_capacity(path.len() + 1);
                value.push(b'?');
                value.extend_from_slice(path);
                rustty_cfg_fmt::format_entry_string(name, &value, writer)
            }
            Path::Required(path) => rustty_cfg_fmt::format_entry_string(name, path, writer),
        }
    }
}

/// RepeatablePath is like repeatable string but represents a path value. The
/// difference is that when loading the configuration any values for this will
/// be automatically expanded relative to the path of the config file (or the home
/// directory).
// pub const RepeatablePath = struct {
//     value: std.ArrayListUnmanaged(Path) = .{},
#[derive(Default)]
pub struct RepeatablePath {
    pub value: Vec<Path>,
}

impl RepeatablePath {
    // pub fn parseCLI(self: *RepeatablePath, alloc: Allocator, input: ?[]const u8) ParseError!void {
    pub fn parse_cli(&mut self, input: Option<&[u8]>) -> Result<(), ParseError> {
        // const item = try Path.parse(alloc, input) orelse {
        //     self.value.clearRetainingCapacity();
        //     return;
        // };
        let item = match Path::parse(input)? {
            Some(item) => item,
            None => {
                self.value.clear();
                return Ok(());
            }
        };

        // if (item.len() == 0) {
        //     // This handles the case of zero length paths after removing any ?
        //     // prefixes or surrounding quotes. In this case, we don't reset the
        //     // list.
        //     return;
        // }
        if item.len() == 0 {
            return Ok(());
        }

        // try self.value.append(alloc, item);
        self.value.push(item);

        Ok(())
    }

    /// Deep copy of the struct. Required by Config.
    // pub fn clone(self: *const RepeatablePath, alloc: Allocator) Allocator.Error!RepeatablePath {
    //     const value = try self.value.clone(alloc);
    //     for (value.items) |*item| {
    //         item.* = try item.clone(alloc);
    //     }
    //
    //     return .{ .value = value };
    // }
    pub fn clone_in(&self) -> RepeatablePath {
        let value = self.value.iter().map(|item| item.clone_in()).collect();
        RepeatablePath { value }
    }

    /// Compare if two of our value are equal. Required by Config.
    // pub fn equal(self: RepeatablePath, other: RepeatablePath) bool {
    //     if (self.value.items.len != other.value.items.len) return false;
    //     for (self.value.items, other.value.items) |a, b| {
    //         if (!a.equal(b)) return false;
    //     }
    //
    //     return true;
    // }
    pub fn equal(&self, other: &RepeatablePath) -> bool {
        if self.value.len() != other.value.len() {
            return false;
        }
        for (a, b) in self.value.iter().zip(other.value.iter()) {
            if !a.equal(b) {
                return false;
            }
        }
        true
    }

    // Used by Formatter
    // pub fn formatEntry(self: RepeatablePath, formatter: formatterpkg.EntryFormatter) !void {
    //
    // PORT NOTE: implemented via the `FormatEntry` trait. See the impl below.

    /// Expand all the paths relative to the base directory.
    // pub fn expand(
    //     self: *RepeatablePath,
    //     alloc: Allocator,
    //     base: []const u8,
    //     diags: *cli.DiagnosticList,
    // ) !void {
    //     for (self.value.items) |*path| {
    //         try path.expand(alloc, base, diags);
    //     }
    // }
    pub fn expand(&mut self, base: &[u8], diags: &mut DiagnosticList) -> std::io::Result<()> {
        for path in &mut self.value {
            path.expand(base, diags)?;
        }
        Ok(())
    }
}

impl FormatEntry for RepeatablePath {
    /// Used by Formatter
    // pub fn formatEntry(self: RepeatablePath, formatter: formatterpkg.EntryFormatter) !void {
    fn format_entry<W: Writer>(&self, name: &[u8], writer: &mut W) -> Result<(), W::Error> {
        // if (self.value.items.len == 0) {
        //     try formatter.formatEntry(void, {});
        //     return;
        // }
        if self.value.is_empty() {
            return format_entry_void(name, writer);
        }

        // var buf: [std.fs.max_path_bytes + 1]u8 = undefined;
        // for (self.value.items) |item| {
        //     const value = switch (item) {
        //         .optional => |path| std.fmt.bufPrint(&buf, "?{s}", .{path}) catch |err| switch (err) {
        //             // Required for builds on Linux where NoSpaceLeft
        //             // isn't an allowed error for fmt.
        //             error.NoSpaceLeft => return error.OutOfMemory,
        //         },
        //         .required => |path| path,
        //     };
        //
        //     try formatter.formatEntry([]const u8, value);
        // }
        for item in &self.value {
            match item {
                Path::Optional(path) => {
                    let mut value: Vec<u8> = Vec::with_capacity(path.len() + 1);
                    value.push(b'?');
                    value.extend_from_slice(path);
                    rustty_cfg_fmt::format_entry_string(name, &value, writer)?;
                }
                Path::Required(path) => {
                    rustty_cfg_fmt::format_entry_string(name, path, writer)?;
                }
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// std.fs / std.fs.path helpers used by `expand`.
// ---------------------------------------------------------------------------

/// PORTS: `std.fs.max_path_bytes` (the platform max path length, 4096 on
/// Linux/macOS). Used to size the `expand` stack buffer.
#[allow(dead_code)]
const fn libc_max_path_bytes() -> usize {
    4096
}

/// PORTS: `std.fs.path.isAbsolute(path)` for POSIX — a path is absolute iff it
/// begins with `/`.
//
// PORT NOTE: Windows absolute-path detection (drive letters / UNC) is not
// reproduced; `expand` early-returns on Windows for the `~/` branch and the rest
// of the function is POSIX-shaped in the reference's primary targets.
fn is_absolute(path: &[u8]) -> bool {
    !path.is_empty() && path[0] == b'/'
}

/// PORTS: `std.fs.path.resolve(alloc, &.{ base, path })` for the simple
/// two-component POSIX case used by `expand`'s file-not-found fallback.
//
// PORT NOTE: `resolve` joins the components and normalizes `.`/`..` against the
// (absolute) base, yielding an absolute path WITHOUT touching the filesystem
// (unlike `realpath`/`canonicalize`). The reference passes `{ base, path }` where
// `base` is absolute and `path` is relative; we join and lexically normalize.
fn resolve(components: &[&[u8]]) -> Vec<u8> {
    let mut stack: Vec<Vec<u8>> = Vec::new();
    let mut is_abs = false;
    for comp in components {
        if comp.is_empty() {
            continue;
        }
        if comp[0] == b'/' {
            // An absolute component resets the stack.
            is_abs = true;
            stack.clear();
        }
        for seg in comp.split(|&b| b == b'/') {
            match seg {
                b"" | b"." => {}
                b".." => {
                    stack.pop();
                }
                _ => stack.push(seg.to_vec()),
            }
        }
    }

    let mut out: Vec<u8> = Vec::new();
    if is_abs {
        out.push(b'/');
    }
    for (i, seg) in stack.iter().enumerate() {
        if i > 0 {
            out.push(b'/');
        }
        out.extend_from_slice(seg);
    }
    if out.is_empty() {
        out.push(b'.');
    }
    out
}

/// Convert raw path bytes to an `OsStr` (POSIX bytes are `OsStr` directly).
#[cfg(unix)]
fn os_str_from_bytes(bytes: &[u8]) -> &std::ffi::OsStr {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::OsStr::from_bytes(bytes)
}

#[cfg(not(unix))]
fn os_str_from_bytes(bytes: &[u8]) -> &std::ffi::OsStr {
    // PORT NOTE: on non-unix the bytes are interpreted as UTF-8 (lossy fallback);
    // the `expand` filesystem path is primarily exercised on POSIX.
    std::ffi::OsStr::new(std::str::from_utf8(bytes).unwrap_or(""))
}

/// Convert a resolved `Path` back to raw bytes.
#[cfg(unix)]
fn path_to_bytes(p: &std::path::Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    p.as_os_str().as_bytes().to_vec()
}

#[cfg(not(unix))]
fn path_to_bytes(p: &std::path::Path) -> Vec<u8> {
    p.to_string_lossy().as_bytes().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustty_cfg_fmt::{entry_formatter, AllocatingWriter};

    /// PORTS: `std.meta.Tag(Path)` discriminant comparison helper for the tests.
    fn tag_required(p: &Path) -> bool {
        matches!(p, Path::Required(_))
    }
    fn tag_optional(p: &Path) -> bool {
        matches!(p, Path::Optional(_))
    }

    // test "parse" {
    #[test]
    fn parse() {
        // const Tag = std.meta.Tag(Path);
        {
            // const item = (try Path.parse(alloc, "config.1")).?;
            let item = Path::parse(Some(b"config.1")).unwrap().unwrap();
            // try testing.expectEqual(Tag.required, @as(Tag, item));
            assert!(tag_required(&item));
            // try testing.expectEqualStrings("config.1", item.required);
            assert!(matches!(&item, Path::Required(v) if v.as_slice() == b"config.1"));
        }

        {
            // const item = (try Path.parse(alloc, "?config.2")).?;
            let item = Path::parse(Some(b"?config.2")).unwrap().unwrap();
            // try testing.expectEqual(Tag.optional, @as(Tag, item));
            assert!(tag_optional(&item));
            // try testing.expectEqualStrings("config.2", item.optional);
            assert!(matches!(&item, Path::Optional(v) if v.as_slice() == b"config.2"));
        }

        {
            // const item = (try Path.parse(alloc, "\"?config.3\"")).?;
            let item = Path::parse(Some(b"\"?config.3\"")).unwrap().unwrap();
            // try testing.expectEqual(Tag.required, @as(Tag, item));
            assert!(tag_required(&item));
            // try testing.expectEqualStrings("?config.3", item.required);
            assert!(matches!(&item, Path::Required(v) if v.as_slice() == b"?config.3"));
        }

        {
            // const item = (try Path.parse(alloc, "?\"config.4\"")).?;
            let item = Path::parse(Some(b"?\"config.4\"")).unwrap().unwrap();
            // try testing.expectEqual(Tag.optional, @as(Tag, item));
            assert!(tag_optional(&item));
            // try testing.expectEqualStrings("config.4", item.optional);
            assert!(matches!(&item, Path::Optional(v) if v.as_slice() == b"config.4"));
        }

        {
            // const item = (try Path.parse(alloc, "?")).?;
            let item = Path::parse(Some(b"?")).unwrap().unwrap();
            // try testing.expectEqual(Tag.optional, @as(Tag, item));
            assert!(tag_optional(&item));
            // try testing.expectEqualStrings("", item.optional);
            assert!(matches!(&item, Path::Optional(v) if v.as_slice() == b""));
        }

        {
            // const item = (try Path.parse(alloc, "\"\"")).?;
            let item = Path::parse(Some(b"\"\"")).unwrap().unwrap();
            // try testing.expectEqual(Tag.required, @as(Tag, item));
            assert!(tag_required(&item));
            // try testing.expectEqualStrings("", item.required);
            assert!(matches!(&item, Path::Required(v) if v.as_slice() == b""));
        }

        {
            // const item = (try Path.parse(alloc, "?\"\"")).?;
            let item = Path::parse(Some(b"?\"\"")).unwrap().unwrap();
            // try testing.expectEqual(Tag.optional, @as(Tag, item));
            assert!(tag_optional(&item));
            // try testing.expectEqualStrings("", item.optional);
            assert!(matches!(&item, Path::Optional(v) if v.as_slice() == b""));
        }
    }

    // test "parseCLI" {  (Path)
    #[test]
    fn path_parse_cli() {
        // var item: Path = undefined;
        let mut item = Path::Required(Vec::new());

        // try item.parseCLI(alloc, "config.1");
        item.parse_cli(Some(b"config.1")).unwrap();
        // try testing.expectEqual(Tag.required, @as(Tag, item));
        assert!(tag_required(&item));
        // try testing.expectEqualStrings("config.1", item.required);
        assert!(matches!(&item, Path::Required(v) if v.as_slice() == b"config.1"));

        // try item.parseCLI(alloc, "?config.2");
        item.parse_cli(Some(b"?config.2")).unwrap();
        // try testing.expectEqual(Tag.optional, @as(Tag, item));
        assert!(tag_optional(&item));
        // try testing.expectEqualStrings("config.2", item.optional);
        assert!(matches!(&item, Path::Optional(v) if v.as_slice() == b"config.2"));

        // try item.parseCLI(alloc, "\"?config.3\"");
        item.parse_cli(Some(b"\"?config.3\"")).unwrap();
        // try testing.expectEqual(Tag.required, @as(Tag, item));
        assert!(tag_required(&item));
        // try testing.expectEqualStrings("?config.3", item.required);
        assert!(matches!(&item, Path::Required(v) if v.as_slice() == b"?config.3"));

        // Zero-length values, ignored

        // try item.parseCLI(alloc, "?");
        item.parse_cli(Some(b"?")).unwrap();
        // try testing.expectEqual(Tag.required, @as(Tag, item));
        assert!(tag_required(&item));
        // try testing.expectEqualStrings("?config.3", item.required);
        assert!(matches!(&item, Path::Required(v) if v.as_slice() == b"?config.3"));

        // try item.parseCLI(alloc, "\"\"");
        item.parse_cli(Some(b"\"\"")).unwrap();
        // try testing.expectEqual(Tag.required, @as(Tag, item));
        assert!(tag_required(&item));
        // try testing.expectEqualStrings("?config.3", item.required);
        assert!(matches!(&item, Path::Required(v) if v.as_slice() == b"?config.3"));

        // try item.parseCLI(alloc, "?\"\"");
        item.parse_cli(Some(b"?\"\"")).unwrap();
        // try testing.expectEqual(Tag.required, @as(Tag, item));
        assert!(tag_required(&item));
        // try testing.expectEqualStrings("?config.3", item.required);
        assert!(matches!(&item, Path::Required(v) if v.as_slice() == b"?config.3"));
    }

    // test "formatConfig single item" {  (Path)
    #[test]
    fn path_format_config_single_item() {
        // var buf: std.Io.Writer.Allocating = .init(testing.allocator);
        let mut buf = AllocatingWriter::init();

        // var item: Path = undefined;
        let mut item = Path::Required(Vec::new());
        // try item.parseCLI(alloc, "A");
        item.parse_cli(Some(b"A")).unwrap();
        // try item.formatEntry(formatterpkg.entryFormatter("a", &buf.writer));
        let mut f = entry_formatter(b"a", &mut buf);
        f.format_entry(&item).unwrap();
        // try std.testing.expectEqualSlices(u8, "a = A\n", buf.written());
        assert_eq!(b"a = A\n".as_slice(), buf.written());
    }

    // test "formatConfig multiple items" {  (Path)
    #[test]
    fn path_format_config_multiple_items() {
        // var buf: std.Io.Writer.Allocating = .init(testing.allocator);
        let mut buf = AllocatingWriter::init();

        // var item: Path = undefined;
        let mut item = Path::Required(Vec::new());
        // try item.parseCLI(alloc, "A");
        item.parse_cli(Some(b"A")).unwrap();
        // try item.parseCLI(alloc, "?B");
        item.parse_cli(Some(b"?B")).unwrap();
        // try item.formatEntry(formatterpkg.entryFormatter("a", &buf.writer));
        let mut f = entry_formatter(b"a", &mut buf);
        f.format_entry(&item).unwrap();
        // try std.testing.expectEqualSlices(u8, "a = ?B\n", buf.written());
        assert_eq!(b"a = ?B\n".as_slice(), buf.written());
    }

    // test "parseCLI" {  (RepeatablePath)
    #[test]
    fn repeatable_path_parse_cli() {
        // var list: RepeatablePath = .{};
        let mut list = RepeatablePath::default();
        // try list.parseCLI(alloc, "config.1");
        list.parse_cli(Some(b"config.1")).unwrap();
        // try list.parseCLI(alloc, "?config.2");
        list.parse_cli(Some(b"?config.2")).unwrap();
        // try list.parseCLI(alloc, "\"?config.3\"");
        list.parse_cli(Some(b"\"?config.3\"")).unwrap();

        // try testing.expectEqual(@as(usize, 3), list.value.items.len);
        assert_eq!(3, list.value.len());

        // Zero-length values, ignored
        // try list.parseCLI(alloc, "?");
        list.parse_cli(Some(b"?")).unwrap();
        // try list.parseCLI(alloc, "\"\"");
        list.parse_cli(Some(b"\"\"")).unwrap();

        // try testing.expectEqual(@as(usize, 3), list.value.items.len);
        assert_eq!(3, list.value.len());

        // const Tag = std.meta.Tag(Path);
        // try testing.expectEqual(Tag.required, @as(Tag, list.value.items[0]));
        assert!(tag_required(&list.value[0]));
        // try testing.expectEqualStrings("config.1", list.value.items[0].required);
        assert!(matches!(&list.value[0], Path::Required(v) if v.as_slice() == b"config.1"));

        // try testing.expectEqual(Tag.optional, @as(Tag, list.value.items[1]));
        assert!(tag_optional(&list.value[1]));
        // try testing.expectEqualStrings("config.2", list.value.items[1].optional);
        assert!(matches!(&list.value[1], Path::Optional(v) if v.as_slice() == b"config.2"));

        // try testing.expectEqual(Tag.required, @as(Tag, list.value.items[2]));
        assert!(tag_required(&list.value[2]));
        // try testing.expectEqualStrings("?config.3", list.value.items[2].required);
        assert!(matches!(&list.value[2], Path::Required(v) if v.as_slice() == b"?config.3"));

        // try list.parseCLI(alloc, "");
        list.parse_cli(Some(b"")).unwrap();
        // try testing.expectEqual(@as(usize, 0), list.value.items.len);
        assert_eq!(0, list.value.len());
    }

    // test "formatConfig empty" {  (RepeatablePath)
    #[test]
    fn repeatable_path_format_config_empty() {
        // var buf: std.Io.Writer.Allocating = .init(testing.allocator);
        let mut buf = AllocatingWriter::init();

        // var list: RepeatablePath = .{};
        let list = RepeatablePath::default();
        // try list.formatEntry(formatterpkg.entryFormatter("a", &buf.writer));
        let mut f = entry_formatter(b"a", &mut buf);
        f.format_entry(&list).unwrap();
        // try std.testing.expectEqualSlices(u8, "a = \n", buf.written());
        assert_eq!(b"a = \n".as_slice(), buf.written());
    }

    // test "formatConfig single item" {  (RepeatablePath)
    #[test]
    fn repeatable_path_format_config_single_item() {
        // var buf: std.Io.Writer.Allocating = .init(testing.allocator);
        let mut buf = AllocatingWriter::init();

        // var list: RepeatablePath = .{};
        let mut list = RepeatablePath::default();
        // try list.parseCLI(alloc, "A");
        list.parse_cli(Some(b"A")).unwrap();
        // try list.formatEntry(formatterpkg.entryFormatter("a", &buf.writer));
        let mut f = entry_formatter(b"a", &mut buf);
        f.format_entry(&list).unwrap();
        // try std.testing.expectEqualSlices(u8, "a = A\n", buf.written());
        assert_eq!(b"a = A\n".as_slice(), buf.written());
    }

    // test "formatConfig multiple items" {  (RepeatablePath)
    #[test]
    fn repeatable_path_format_config_multiple_items() {
        // var buf: std.Io.Writer.Allocating = .init(testing.allocator);
        let mut buf = AllocatingWriter::init();

        // var list: RepeatablePath = .{};
        let mut list = RepeatablePath::default();
        // try list.parseCLI(alloc, "A");
        list.parse_cli(Some(b"A")).unwrap();
        // try list.parseCLI(alloc, "?B");
        list.parse_cli(Some(b"?B")).unwrap();
        // try list.formatEntry(formatterpkg.entryFormatter("a", &buf.writer));
        let mut f = entry_formatter(b"a", &mut buf);
        f.format_entry(&list).unwrap();
        // try std.testing.expectEqualSlices(u8, "a = A\na = ?B\n", buf.written());
        assert_eq!(b"a = A\na = ?B\n".as_slice(), buf.written());
    }
}
