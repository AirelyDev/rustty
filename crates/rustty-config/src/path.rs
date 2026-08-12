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
use rustty_cfg_fmt::{format_entry_void, FormatEntry, FormatError, Writer};
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

/// ghostty_config_path_s -> rustty_config_path_s (R2)
// pub const C = extern struct {
//     path: [*:0]const u8,
//     optional: bool,
// };
//
// PORT NOTE: `path` is the `[*:0]const u8` data pointer of the variant payload.
// The rustty `[:0]const u8` representation is a `Vec<u8>` carrying the value
// bytes WITHOUT the trailing sentinel (the crate-wide convention documented at
// `rustty-args` `parse_field_sentinel_string`), so this projection hands out the
// same pointer the sibling `?[:0]const u8` C getter does (`c_get.rs` `g_optstr!`)
// — the sentinel-storage gap is shared by every `[:0]` config field and is not
// specific to `Path`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct PathC {
    pub path: *const std::os::raw::c_char,
    pub optional: bool,
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
    // PORT NOTE: the `extern struct C` is a top-level item here (`PathC`) because
    // Rust `impl` blocks cannot contain type definitions — the same rendering the
    // crate already uses for `Color.C` -> `ColorC` in `value_types`.

    /// Returns the path as a C-compatible struct.
    // pub fn cval(self: Path) C {
    //     return switch (self) {
    //         .optional => |path| .{ .path = path.ptr, .optional = true },
    //         .required => |path| .{ .path = path.ptr, .optional = false },
    //     };
    // }
    pub fn cval(&self) -> PathC {
        match self {
            Path::Optional(path) => PathC {
                path: path.as_ptr().cast(),
                optional: true,
            },
            Path::Required(path) => PathC {
                path: path.as_ptr().cast(),
                optional: false,
            },
        }
    }

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
                //
                // WS14 T6 (wasm32 arm): a browser has no home directory either, so
                // wasm takes the SAME "unsupported, leave the path alone" branch as
                // Windows. The reference never analyses this on wasm (Zig's lazy
                // analysis of `config.zig.Wasm`); Rust compiles whole crates.
                #[cfg(any(target_os = "windows", target_family = "wasm"))]
                {
                    break 'expand;
                }

                // const expanded: []const u8 = internal_os.expandHome(path, &buf) catch |err| { ... };
                #[cfg(not(any(target_os = "windows", target_family = "wasm")))]
                {
                    let mut buf = [0u8; libc_max_path_bytes()];
                    let expanded: &[u8] = match rustty_os::homedir::expandHome(
                        &rustty_global::environ_map(),
                        &path,
                        &mut buf,
                    ) {
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
    fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
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
                // std.fmt.bufPrint(&buf, "?{s}", .{path}) catch |err| switch (err) {
                //     error.NoSpaceLeft => return error.OutOfMemory,
                // }
                //
                // PORT NOTE: the reference formats through the FIXED
                // `[std.fs.max_path_bytes + 1]u8` stack buffer; `"?" ++ path`
                // exceeding it is `error.NoSpaceLeft` -> `error.OutOfMemory`.
                // The growable Vec is only the scratch — the fixed capacity is
                // enforced by the bound check (byte-identical output when it
                // fits).
                if 1 + path.len() > libc_max_path_bytes() + 1 {
                    return Err(FormatError::OutOfMemory);
                }
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
    fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
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
                    // PORT NOTE: same fixed `[std.fs.max_path_bytes + 1]u8`
                    // bound as `Path::format_entry` — `error.NoSpaceLeft` ->
                    // `error.OutOfMemory` when `"?" ++ path` overflows it.
                    if 1 + path.len() > libc_max_path_bytes() + 1 {
                        return Err(FormatError::OutOfMemory);
                    }
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

/// PORTS: `std.fs.max_path_bytes` (the platform max path length — POSIX
/// `PATH_MAX`: 1024 on Darwin, 4096 on Linux). Sizes the `expand` stack buffer
/// and bounds the `formatEntry` `[std.fs.max_path_bytes + 1]u8` scratch.
const fn libc_max_path_bytes() -> usize {
    // Zig `std.fs.max_path_bytes` is `std.c.PATH_MAX` on POSIX: the Darwin
    // family's `syslimits.h` PATH_MAX is 1024; Linux's `limits.h` is 4096.
    if cfg!(target_vendor = "apple") {
        1024
    } else {
        4096
    }
}

/// PORTS: `std.fs.path.PathType.windows.isSep(u8, c)` — Win32 accepts BOTH
/// separators.
// pub fn isSep(comptime path_type: PathType, comptime T: type, c: T) bool
fn is_sep_windows(c: u8) -> bool {
    c == b'/' || c == b'\\'
}

/// PORTS: `std.fs.path.PathType.posix.isSep(u8, c)`.
fn is_sep_posix(c: u8) -> bool {
    c == b'/'
}

/// PORTS: `std.unicode.utf8ByteSequenceLength(first_byte)` — `null` is the
/// reference's `error.Utf8InvalidStartByte`.
fn utf8_byte_sequence_length(first_byte: u8) -> Option<usize> {
    match first_byte {
        0b0000_0000..=0b0111_1111 => Some(1),
        0b1100_0000..=0b1101_1111 => Some(2),
        0b1110_0000..=0b1110_1111 => Some(3),
        0b1111_0000..=0b1111_0111 => Some(4),
        _ => None,
    }
}

/// PORTS: `std.fs.path.Win32PathType`.
// pub const Win32PathType = enum { unc_absolute, drive_absolute, drive_relative,
//     rooted, relative, local_device, root_local_device };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Win32PathType {
    /// `\\server\share\foo`
    UncAbsolute,
    /// `C:\foo`
    DriveAbsolute,
    /// `C:foo`
    DriveRelative,
    /// `\foo`
    Rooted,
    /// `foo`
    Relative,
    /// `\\.\foo`, `\\?\foo`
    LocalDevice,
    /// `\\.`, `\\?`
    RootLocalDevice,
}

/// Get the path type of a Win32 namespace path.
/// Similar to `RtlDetermineDosPathNameType_U`.
//
// PORTS: `std.fs.path.getWin32PathType(u8, path)` (the WTF-8 instantiation; the
// `u16` one has no caller in the reference config tree).
pub(crate) fn get_win32_path_type(path: &[u8]) -> Win32PathType {
    // if (path.len < 1) return .relative;
    if path.is_empty() {
        return Win32PathType::Relative;
    }

    // if (windows_path.isSep(T, path[0])) {
    if is_sep_windows(path[0]) {
        // \x
        // if (path.len < 2 or !windows_path.isSep(T, path[1])) return .rooted;
        if path.len() < 2 || !is_sep_windows(path[1]) {
            return Win32PathType::Rooted;
        }
        // \\. or \\?
        // if (path.len > 2 and (path[2] == '.' or path[2] == '?')) {
        if path.len() > 2 && (path[2] == b'.' || path[2] == b'?') {
            // exactly \\. or \\? with nothing trailing
            // if (path.len == 3) return .root_local_device;
            if path.len() == 3 {
                return Win32PathType::RootLocalDevice;
            }
            // \\.\x or \\?\x
            // if (windows_path.isSep(T, path[3])) return .local_device;
            if is_sep_windows(path[3]) {
                return Win32PathType::LocalDevice;
            }
        }
        // \\x
        // return .unc_absolute;
        Win32PathType::UncAbsolute
    } else {
        // PORT NOTE: the reference's long comment here explains why the WTF-8
        // instantiation must find the `:` by CODE POINT length rather than at a
        // fixed byte index: `RtlDetermineDosPathNameType_U` accepts any single
        // WTF-16 code unit as a drive letter, so a non-ASCII drive letter (e.g.
        // `€:\`) is drive-absolute while a code point needing two WTF-16 code
        // units (e.g. U+10000) is relative.
        //
        // const code_point_len = std.unicode.utf8ByteSequenceLength(path[0]) catch return .relative;
        // if (code_point_len > 3) return .relative;
        let colon_i = match utf8_byte_sequence_length(path[0]) {
            Some(len) if len <= 3 => len,
            _ => return Win32PathType::Relative,
        };
        // x
        // if (path.len < colon_i + 1 or path[colon_i] != ':') return .relative;
        if path.len() < colon_i + 1 || path[colon_i] != b':' {
            return Win32PathType::Relative;
        }
        // x:\
        // if (path.len > colon_i + 1 and windows_path.isSep(T, path[colon_i + 1])) return .drive_absolute;
        if path.len() > colon_i + 1 && is_sep_windows(path[colon_i + 1]) {
            return Win32PathType::DriveAbsolute;
        }
        // x:
        Win32PathType::DriveRelative
    }
}

/// PORTS: `std.fs.path.getDriveLetter(u8, path).len`.
// fn getDriveLetter(comptime T: type, path: []const T) []const T
fn drive_letter_len(path: &[u8]) -> usize {
    // getWin32PathType will only return .drive_absolute/.drive_relative when there is
    // (1) a valid code point, and (2) a code point < U+10000, so we only need to
    // get the length determined by the first byte.
    utf8_byte_sequence_length(path[0]).expect("valid drive-letter start byte")
}

/// PORTS: `std.fs.path.parsePathWindows(u8, path).root.len` — the length of the
/// path's root (`""`, `C:`, `C:\`, `\`, `\\server\share\`, `\\?\`, ...).
///
/// Only the root LENGTH is needed by the ported callers (`basename`), so the
/// `WindowsPath2` struct itself is not modeled.
fn parse_path_windows_root_len(path: &[u8]) -> usize {
    // const kind = getWin32PathType(T, path);
    match get_win32_path_type(path) {
        // .drive_absolute, .drive_relative => drive_letter_len + (2 or 1)
        kind @ (Win32PathType::DriveAbsolute | Win32PathType::DriveRelative) => {
            drive_letter_len(path) + usize::from(kind == Win32PathType::DriveAbsolute) + 1
        }
        // .relative => path[0..0]
        Win32PathType::Relative => 0,
        // .local_device => path[0..4]
        Win32PathType::LocalDevice => 4,
        // .root_local_device => path
        Win32PathType::RootLocalDevice => path.len(),
        // .rooted => path[0..1]
        Win32PathType::Rooted => 1,
        // .unc_absolute => 2 + (unc.share.ptr - unc.server.ptr) + unc.share.len (+1 if sep_after_share)
        //
        // PORTS: `std.fs.path.parseUNC(u8, path)`, inlined to the byte offsets the
        // root length needs. `server` always starts at index 2, so the reference's
        // pointer difference `share.ptr - server.ptr` is `share_start - 2` and the
        // whole expression collapses to `share_start + share.len`.
        Win32PathType::UncAbsolute => {
            // Asserts that `path` starts with two path separators.
            assert(path.len() >= 2 && is_sep_windows(path[0]) && is_sep_windows(path[1]));
            // For the server, the first path separator after the initial two is always
            // the terminator of the server name, even if that means the server name is
            // zero-length.
            let server_end = match path[2..].iter().position(|&c| is_sep_windows(c)) {
                // orelse return .{ .server = path[2..], .share = path[len..len] }
                None => return path.len(),
                Some(off) => 2 + off,
            };
            // For the share, there can be any number of path separators between the server
            // and the share, so we want to skip over all of them instead of just looking for
            // the first one (`std.mem.tokenizeAny`).
            let rest = &path[server_end + 1..];
            let tok_start = rest
                .iter()
                .position(|&c| !is_sep_windows(c))
                .unwrap_or(rest.len());
            if tok_start == rest.len() {
                // it.next() orelse return .{ .share = path[server_end+1..server_end+1] }
                return server_end + 1;
            }
            let tok_len = rest[tok_start..]
                .iter()
                .position(|&c| is_sep_windows(c))
                .unwrap_or(rest.len() - tok_start);
            let share_start = server_end + 1 + tok_start;
            let mut root_len = share_start + tok_len;
            // if (unc.sep_after_share) root_len += 1;  (`it.index != it.buffer.len`)
            if tok_start + tok_len != rest.len() {
                root_len += 1;
            }
            root_len
        }
    }
}

/// PORTS: `std.fs.path.isAbsoluteWindows(path)` (via `isAbsoluteWindowsImpl`).
// PORT NOTE: on the POSIX hosts the port builds on, the Windows arm is compiled
// (so it stays type-checked and unit-testable) but nothing dispatches to it.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn is_absolute_windows(path: &[u8]) -> bool {
    match get_win32_path_type(path) {
        // Unambiguously absolute
        Win32PathType::DriveAbsolute
        | Win32PathType::UncAbsolute
        | Win32PathType::LocalDevice
        | Win32PathType::RootLocalDevice => true,
        // Unambiguously relative
        Win32PathType::Relative => false,
        // Ambiguous, more absolute than relative
        Win32PathType::Rooted => true,
        // Ambiguous, more relative than absolute
        Win32PathType::DriveRelative => false,
    }
}

/// PORTS: `std.fs.path.isAbsolutePosix(path)`.
pub(crate) fn is_absolute_posix(path: &[u8]) -> bool {
    // return path.len > 0 and path[0] == sep_posix;
    !path.is_empty() && path[0] == b'/'
}

/// PORTS: `std.fs.path.isAbsolute(path)` — the target-dispatched form.
// pub fn isAbsolute(path: []const u8) bool {
//     if (native_os == .windows) return isAbsoluteWindows(path) else return isAbsolutePosix(path);
// }
//
// PORT NOTE: both arms are always COMPILED (only the dispatch is `cfg`-selected)
// so the Windows semantics stay unit-testable from the POSIX hosts the port
// currently builds on — the same way the reference's `isAbsoluteWindows` tests
// run cross-target.
pub(crate) fn is_absolute(path: &[u8]) -> bool {
    #[cfg(windows)]
    {
        is_absolute_windows(path)
    }
    #[cfg(not(windows))]
    {
        is_absolute_posix(path)
    }
}

/// PORTS: `std.fs.path.basenameInner(path_type, path)` — the last component of
/// `path` per `ComponentIterator(path_type, u8).last()`, or `""` when the path
/// has no component past its root.
fn basename_inner(path: &[u8], windows: bool) -> &[u8] {
    let is_sep = if windows {
        is_sep_windows
    } else {
        is_sep_posix
    };

    // ComponentIterator.init: the root length, then every repeated separator
    // directly after it.
    let root_len = if windows {
        parse_path_windows_root_len(path)
    } else {
        usize::from(!path.is_empty() && is_sep(path[0]))
    };
    let mut root_end_index = root_len;
    for &c in &path[root_len..] {
        if !is_sep(c) {
            break;
        }
        root_end_index += 1;
    }

    // ComponentIterator.last()
    let mut end_index = path.len();
    loop {
        if end_index == root_end_index {
            // `orelse return &[_]u8{}` in `basenameInner`.
            return &path[0..0];
        }
        if !is_sep(path[end_index - 1]) {
            break;
        }
        end_index -= 1;
    }
    let mut start_index = end_index;
    loop {
        if start_index == root_end_index {
            break;
        }
        if is_sep(path[start_index - 1]) {
            break;
        }
        start_index -= 1;
    }
    if start_index == end_index {
        return &path[0..0];
    }
    &path[start_index..end_index]
}

/// PORTS: `std.fs.path.basenameWindows(path)`.
// PORT NOTE: on the POSIX hosts the port builds on, the Windows arm is compiled
// (so it stays type-checked and unit-testable) but nothing dispatches to it.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn basename_windows(path: &[u8]) -> &[u8] {
    basename_inner(path, true)
}

/// PORTS: `std.fs.path.basenamePosix(path)`.
pub(crate) fn basename_posix(path: &[u8]) -> &[u8] {
    basename_inner(path, false)
}

/// PORTS: `std.fs.path.basename(path)` — the target-dispatched form (see the
/// `is_absolute` PORT NOTE on why both arms are always compiled).
pub(crate) fn basename(path: &[u8]) -> &[u8] {
    #[cfg(windows)]
    {
        basename_windows(path)
    }
    #[cfg(not(windows))]
    {
        basename_posix(path)
    }
}

/// PORTS: `std.fs.path.WindowsUNC(u8)` — the pieces of a `\\server\share`
/// prefix, as `parseUNC` reports them.
struct WindowsUnc<'a> {
    server: &'a [u8],
    sep_after_server: bool,
    share: &'a [u8],
    sep_after_share: bool,
}

/// PORTS: `std.fs.path.parseUNC(u8, path)`.
/// Asserts that `path` starts with two path separators.
fn parse_unc(path: &[u8]) -> WindowsUnc<'_> {
    assert(path.len() >= 2 && is_sep_windows(path[0]) && is_sep_windows(path[1]));
    // For the server, the first path separator after the initial two is always
    // the terminator of the server name, even if that means the server name is
    // zero-length.
    // const server_end = mem.findAnyPos(T, path, 2, any_sep) orelse return ...
    let server_end = match path[2..].iter().position(|&c| is_sep_windows(c)) {
        None => {
            return WindowsUnc {
                server: &path[2..],
                sep_after_server: false,
                share: &path[path.len()..],
                sep_after_share: false,
            }
        }
        Some(off) => 2 + off,
    };
    // For the share, there can be any number of path separators between the
    // server and the share, so we want to skip over all of them instead of just
    // looking for the first one (`std.mem.tokenizeAny`).
    let rest = &path[server_end + 1..];
    let tok_start = rest
        .iter()
        .position(|&c| !is_sep_windows(c))
        .unwrap_or(rest.len());
    if tok_start == rest.len() {
        // it.next() orelse return ...
        return WindowsUnc {
            server: &path[2..server_end],
            sep_after_server: true,
            share: &path[server_end + 1..server_end + 1],
            sep_after_share: false,
        };
    }
    let tok_len = rest[tok_start..]
        .iter()
        .position(|&c| is_sep_windows(c))
        .unwrap_or(rest.len() - tok_start);
    WindowsUnc {
        server: &path[2..server_end],
        sep_after_server: true,
        share: &rest[tok_start..tok_start + tok_len],
        // .sep_after_share = it.index != it.buffer.len
        sep_after_share: tok_start + tok_len != rest.len(),
    }
}

/// PORTS: `std.fs.path.parsePathWindows(u8, path)` — `WindowsPath2(u8)` as the
/// `(kind, root)` pair. The root length is `parse_path_windows_root_len`, which
/// already inlines the reference's per-kind arms.
fn parse_path_windows(path: &[u8]) -> (Win32PathType, &[u8]) {
    let kind = get_win32_path_type(path);
    (kind, &path[..parse_path_windows_root_len(path)])
}

/// PORTS: `std.os.windows.toUpperWtf16(c)`.
//
// The reference resolves this to `ntdll.RtlUpcaseUnicodeChar` on windows and to
// the vendored `nls.upcaseW` table off windows (`std/os/windows.zig:3716-3720`).
// The windows arm below binds the SAME ntdll entry point (R3). The off-windows
// arm is ASCII-only: `nls.upcaseW` is a 2544-entry copy of the locale-independent
// `l_intl.nls` uppercase table, which this port does not vendor. The ONLY caller
// is `compare_disk_designators_drive` (drive-letter equality), so the divergence
// is confined to NON-ASCII drive letters (e.g. `λ:` vs `Λ:`) evaluated on a
// non-windows host — i.e. to this file's own unit tests, never to a real windows
// run, where the ntdll arm is exact. Recorded in `.port/phase-2/DEFERRALS.md`.
#[cfg(windows)]
fn to_upper_wtf16(c: u16) -> u16 {
    #[link(name = "ntdll")]
    extern "system" {
        fn RtlUpcaseUnicodeChar(SourceCharacter: u16) -> u16;
    }
    unsafe { RtlUpcaseUnicodeChar(c) }
}

#[cfg(not(windows))]
fn to_upper_wtf16(c: u16) -> u16 {
    if c < 0x80 {
        (c as u8).to_ascii_uppercase() as u16
    } else {
        c
    }
}

/// PORTS: `std.os.windows.eqlIgnoreCaseWtf8(a, b)`.
fn eql_ignore_case_wtf8(a: &[u8], b: &[u8]) -> bool {
    // A length equality check is not possible here because there are some
    // codepoints that have a different length uppercase UTF-8 representation
    // than their lowercase counterparts.
    let a_it = decode_wtf8_codepoints(a);
    let mut b_it = decode_wtf8_codepoints(b);
    // `while (true) { const a_cp = a_it.nextCodepoint() orelse break; ... }`
    for a_cp in a_it {
        let Some(b_cp) = b_it.next() else {
            return false;
        };

        if a_cp <= u16::MAX as u32 && b_cp <= u16::MAX as u32 {
            if a_cp != b_cp && to_upper_wtf16(a_cp as u16) != to_upper_wtf16(b_cp as u16) {
                return false;
            }
        } else if a_cp != b_cp {
            return false;
        }
    }
    // Make sure there are no leftover codepoints in b
    if b_it.next().is_some() {
        return false;
    }
    true
}

/// PORTS: `std.unicode.Wtf8View.initUnchecked(s).iterator()`'s `nextCodepoint`
/// stream, as an iterator of scalar values. Invalid sequences are impossible in
/// the only caller's inputs (drive letters classified by `getWin32PathType`);
/// a malformed lead byte terminates the stream, matching the reference's
/// "unchecked" contract of never being handed one.
fn decode_wtf8_codepoints(mut s: &[u8]) -> impl Iterator<Item = u32> + '_ {
    core::iter::from_fn(move || {
        let first = *s.first()?;
        let len = utf8_byte_sequence_length(first)?;
        if s.len() < len {
            return None;
        }
        let cp = match len {
            1 => first as u32,
            2 => ((first as u32 & 0x1F) << 6) | (s[1] as u32 & 0x3F),
            3 => ((first as u32 & 0x0F) << 12) | ((s[1] as u32 & 0x3F) << 6) | (s[2] as u32 & 0x3F),
            _ => {
                ((first as u32 & 0x07) << 18)
                    | ((s[1] as u32 & 0x3F) << 12)
                    | ((s[2] as u32 & 0x3F) << 6)
                    | (s[3] as u32 & 0x3F)
            }
        };
        s = &s[len..];
        Some(cp)
    })
}

/// PORTS: `std.fs.path.compareDiskDesignators(u8, .drive, p1, p2)`.
///
/// `resolveWindows` is the reference's only caller and only ever passes
/// `.drive`; the `.unc` arm (`parseUNC` server+share equality) therefore has no
/// ported caller and is not reproduced.
fn compare_disk_designators_drive(p1: &[u8], p2: &[u8]) -> bool {
    // const drive_letter1 = getDriveLetter(T, p1);
    let d1 = &p1[..drive_letter_len(p1)];
    let d2 = &p2[..drive_letter_len(p2)];
    eql_ignore_case_wtf8(d1, d2)
}

/// PORTS: `std.mem.tokenizeAny(u8, buf, "/\\")` — the non-empty runs between
/// runs of either Win32 separator.
fn tokenize_any_windows_sep(buf: &[u8]) -> impl Iterator<Item = &[u8]> {
    buf.split(|&c| is_sep_windows(c)).filter(|t| !t.is_empty())
}

/// PORTS: `std.fs.path.resolveWindows(alloc, paths)`.
///
/// This function is like a series of `cd` statements executed one after
/// another. It resolves "." and ".." to the best of its ability, but will not
/// convert relative paths to an absolute path. ".." components may persist in
/// the resolved path if the resolved path is relative or drive-relative. Path
/// separators are canonicalized to '\\' and drives are canonicalized to capital
/// letters.
///
/// Each drive has its own current working directory, which is only resolved via
/// the paths provided. In the scenario that the resolved path contains a
/// drive-relative path that can't be resolved using the paths alone, the result
/// will be a drive-relative path. Similarly for a rooted path.
//
// PORT NOTE: on the POSIX hosts the port builds on, the Windows arm is compiled
// (so it stays type-checked and unit-testable) but nothing dispatches to it —
// the same arrangement as `is_absolute_windows`/`basename_windows` above.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn resolve_windows(paths: &[&[u8]]) -> Vec<u8> {
    // `std.bit_set.DynamicBitSetUnmanaged` over `paths.len` (the reference's
    // stack-fallback allocator is an allocation strategy, not semantics).
    let mut relevant_paths = vec![false; paths.len()];

    // Iterate the paths backwards, marking the relevant paths along the way.
    // This also allows us to break from the loop whenever any earlier paths are
    // known to be irrelevant.
    let mut first_path_i = paths.len();
    // The `root:` labeled block: `broke_root` is `break :root <value>`, and
    // falling out of the loop yields `last_effective_root_path`.
    let mut last_effective_root_kind = Win32PathType::Relative;
    let mut last_effective_root: &[u8] = b"";
    let mut last_rooted_path_i: Option<usize> = None;
    let mut last_drive_relative_path_i: usize = 0;
    let mut broke_root: Option<(Win32PathType, &[u8])> = None;

    while first_path_i > 0 {
        first_path_i -= 1;
        let (kind, root) = parse_path_windows(paths[first_path_i]);
        match kind {
            Win32PathType::UncAbsolute
            | Win32PathType::RootLocalDevice
            | Win32PathType::LocalDevice => {
                match last_effective_root_kind {
                    Win32PathType::Rooted => {}
                    Win32PathType::DriveRelative => continue,
                    _ => relevant_paths[first_path_i] = true,
                }
                broke_root = Some((kind, root));
                break;
            }
            Win32PathType::DriveRelative | Win32PathType::DriveAbsolute => {
                match last_effective_root_kind {
                    Win32PathType::DriveRelative => {
                        if !compare_disk_designators_drive(root, last_effective_root) {
                            continue;
                        } else if last_rooted_path_i.is_some() {
                            broke_root = Some((Win32PathType::DriveAbsolute, root));
                            break;
                        }
                    }
                    Win32PathType::Relative => {
                        last_effective_root_kind = kind;
                        last_effective_root = root;
                    }
                    // This is the end of the line, since the rooted path will
                    // always be relative to this drive letter, and even if the
                    // current path is drive-relative, the rooted-ness makes that
                    // irrelevant. Therefore, force the kind of the effective
                    // root to be drive-absolute in order to properly resolve a
                    // rooted path against a drive-relative one, as the result
                    // should always be drive-absolute.
                    Win32PathType::Rooted => {
                        broke_root = Some((Win32PathType::DriveAbsolute, root));
                        break;
                    }
                    Win32PathType::DriveAbsolute
                    | Win32PathType::UncAbsolute
                    | Win32PathType::RootLocalDevice
                    | Win32PathType::LocalDevice => unreachable!(),
                }
                relevant_paths[first_path_i] = true;
                last_drive_relative_path_i = first_path_i;
                if kind == Win32PathType::DriveAbsolute {
                    broke_root = Some((kind, root));
                    break;
                }
            }
            Win32PathType::Relative => {
                match last_effective_root_kind {
                    Win32PathType::Rooted => continue,
                    Win32PathType::Relative => {
                        last_effective_root_kind = kind;
                        last_effective_root = root;
                    }
                    _ => {}
                }
                relevant_paths[first_path_i] = true;
            }
            Win32PathType::Rooted => {
                match last_effective_root_kind {
                    Win32PathType::DriveRelative => {}
                    Win32PathType::Relative => {
                        last_effective_root_kind = kind;
                        last_effective_root = root;
                    }
                    Win32PathType::Rooted => continue,
                    Win32PathType::DriveAbsolute
                    | Win32PathType::UncAbsolute
                    | Win32PathType::RootLocalDevice
                    | Win32PathType::LocalDevice => unreachable!(),
                }
                if last_rooted_path_i.is_none() {
                    last_rooted_path_i = Some(first_path_i);
                    relevant_paths[first_path_i] = true;
                }
            }
        }
    }

    let (root_kind, root_slice) = match broke_root {
        Some(v) => v,
        None => {
            // After iterating, if the pending effective root is drive-relative
            // then that means nothing has led to forcing a drive-absolute root
            // (a path that allows resolving the drive-specific CWD would cause
            // an early break), so we now need to ignore all paths before the
            // most recent drive-relative one. For example, if we're resolving
            // { "\\rooted", "relative", "C:drive-relative" } then the `\rooted`
            // and `relative` needs to be ignored since we can't know what the
            // rooted path is rooted against as that'd require knowing the CWD.
            if last_effective_root_kind == Win32PathType::DriveRelative {
                for slot in relevant_paths.iter_mut().take(last_drive_relative_path_i) {
                    *slot = false;
                }
            }
            (last_effective_root_kind, last_effective_root)
        }
    };

    let mut result: Vec<u8> = Vec::new();
    let mut want_path_sep_between_root_and_component = false;
    match root_kind {
        Win32PathType::RootLocalDevice | Win32PathType::LocalDevice => {
            result.extend_from_slice(b"\\\\");
            result.push(root_slice[2]); // . or ?
            want_path_sep_between_root_and_component = true;
        }
        Win32PathType::DriveAbsolute | Win32PathType::DriveRelative => {
            // PORT NOTE: verbatim `std.ascii.toUpper(root[0])` — the reference
            // emits the FIRST BYTE only, so a multi-byte (non-ASCII) drive
            // letter is truncated to its lead byte here. Inherited per R1.
            result.push(root_slice[0].to_ascii_uppercase());
            result.push(b':');
            if root_kind == Win32PathType::DriveAbsolute {
                result.push(b'\\');
            }
        }
        Win32PathType::UncAbsolute => {
            let unc = parse_unc(root_slice);
            result.extend_from_slice(b"\\\\");
            if !unc.server.is_empty() || unc.sep_after_server {
                result.extend_from_slice(unc.server);
                if unc.sep_after_server {
                    result.push(b'\\');
                } else {
                    want_path_sep_between_root_and_component = true;
                }
            }
            if !unc.share.is_empty() {
                result.extend_from_slice(unc.share);
                if unc.sep_after_share {
                    result.push(b'\\');
                } else {
                    want_path_sep_between_root_and_component = true;
                }
            }
        }
        Win32PathType::Rooted => result.push(b'\\'),
        Win32PathType::Relative => {}
    }

    let root_len = result.len();
    let mut negative_count: usize = 0;
    for (i, path) in paths.iter().enumerate().skip(first_path_i) {
        if !relevant_paths[i] {
            continue;
        }

        let (_, parsed_root) = parse_path_windows(path);
        let skip_len = parsed_root.len();
        for component in tokenize_any_windows_sep(&path[skip_len..]) {
            if component == b"." {
                continue;
            } else if component == b".." {
                if result.is_empty()
                    || (result.len() == root_len && root_kind == Win32PathType::DriveRelative)
                {
                    negative_count += 1;
                    continue;
                }
                loop {
                    if result.len() == root_len {
                        break;
                    }
                    let end_with_sep = is_sep_windows(result[result.len() - 1]);
                    result.truncate(result.len() - 1);
                    if end_with_sep {
                        break;
                    }
                }
            } else if result.len() == root_len && !want_path_sep_between_root_and_component {
                result.extend_from_slice(component);
            } else {
                result.push(b'\\');
                result.extend_from_slice(component);
            }
        }
    }

    if root_len != 0 && result.len() == root_len && negative_count == 0 {
        return result;
    }

    if result.len() == root_len {
        if negative_count == 0 {
            return b".".to_vec();
        }
        for _ in 0..negative_count - 1 {
            result.extend_from_slice(b"..\\");
        }
        result.extend_from_slice(b"..");
    } else {
        // `result.addManyAt(root_len, 3 * negative_count)` filled with "..\".
        let mut prefix: Vec<u8> = Vec::with_capacity(3 * negative_count);
        for _ in 0..negative_count {
            prefix.extend_from_slice(b"..\\");
        }
        result.splice(root_len..root_len, prefix);
    }

    result
}

/// PORTS: `std.fs.path.resolvePosix(alloc, paths)`.
// PORT NOTE: like the Windows arm, always compiled so both stay unit-testable
// from either host; only the `resolve` dispatch below is `cfg`-selected.
#[cfg_attr(windows, allow(dead_code))]
pub(crate) fn resolve_posix(paths: &[&[u8]]) -> Vec<u8> {
    assert(!paths.is_empty());

    let mut result: Vec<u8> = Vec::new();
    let mut negative_count: usize = 0;
    let mut is_abs = false;

    for p in paths {
        if is_absolute_posix(p) {
            is_abs = true;
            negative_count = 0;
            result.clear();
        }
        for component in p.split(|&c| c == b'/').filter(|t| !t.is_empty()) {
            if component == b"." {
                continue;
            } else if component == b".." {
                if result.is_empty() {
                    negative_count += usize::from(!is_abs);
                    continue;
                }
                loop {
                    let ends_with_slash = result[result.len() - 1] == b'/';
                    result.truncate(result.len() - 1);
                    if ends_with_slash || result.is_empty() {
                        break;
                    }
                }
            } else if !result.is_empty() || is_abs {
                result.push(b'/');
                result.extend_from_slice(component);
            } else {
                result.extend_from_slice(component);
            }
        }
    }

    if result.is_empty() {
        if is_abs {
            return b"/".to_vec();
        }
        if negative_count == 0 {
            return b".".to_vec();
        }
        let mut out: Vec<u8> = Vec::with_capacity(3 * negative_count - 1);
        for _ in 0..negative_count - 1 {
            out.extend_from_slice(b"../");
        }
        out.extend_from_slice(b"..");
        return out;
    }

    if negative_count == 0 {
        return result;
    }
    let mut out: Vec<u8> = Vec::with_capacity(3 * negative_count + result.len());
    for _ in 0..negative_count {
        out.extend_from_slice(b"../");
    }
    out.extend_from_slice(&result);
    out
}

/// PORTS: `std.fs.path.resolve(alloc, &.{ base, path })` — the target-dispatched
/// form used by `expand`'s file-not-found fallback.
//
// PORT NOTE: `resolve` joins the components and normalizes `.`/`..` against the
// (absolute) base, yielding an absolute path WITHOUT touching the filesystem
// (unlike `realpath`/`canonicalize`). The reference passes `{ base, path }` where
// `base` is absolute and `path` is relative.
//
// PORT NOTE (Windows): both arms are always COMPILED (only the dispatch is
// `cfg`-selected) so the Windows semantics stay unit-testable from the POSIX
// hosts the port currently builds on — the same arrangement as `is_absolute` and
// `basename` above.
fn resolve(components: &[&[u8]]) -> Vec<u8> {
    #[cfg(windows)]
    {
        resolve_windows(components)
    }
    #[cfg(not(windows))]
    {
        resolve_posix(components)
    }
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
    //
    // PORT NOTE: the reference test name `parse` is duplicated across
    // containers in this crate (`Path` here; `MouseScrollMultiplier` in
    // Config.zig). The port convention disambiguates duplicated test names
    // with the container prefix (`path_parse`, like
    // `mouse_scroll_multiplier_parse`).
    #[test]
    fn path_parse() {
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

    // rustty-only test (parity fix W94-B01-01): the reference formats optional
    // paths through a FIXED `[std.fs.max_path_bytes + 1]u8` buffer, so a path
    // of exactly `max_path_bytes` bytes fits (`?` + path = buffer size) and one
    // byte more returns `error.OutOfMemory`. Required paths bypass the buffer
    // and never error. Lock both sides of the boundary.
    #[test]
    fn path_format_config_optional_fixed_buffer_boundary() {
        use rustty_cfg_fmt::FormatError;

        let max = super::libc_max_path_bytes();

        // Exactly max_path_bytes: fits the [max_path_bytes + 1] buffer.
        {
            let mut buf = AllocatingWriter::init();
            let item = Path::Optional(vec![b'p'; max]);
            let mut f = entry_formatter(b"a", &mut buf);
            f.format_entry(&item).unwrap();
            let mut expected: Vec<u8> = Vec::new();
            expected.extend_from_slice(b"a = ?");
            expected.extend_from_slice(&vec![b'p'; max]);
            expected.push(b'\n');
            assert_eq!(expected.as_slice(), buf.written());
        }

        // One byte over: `?` + path overflows the fixed buffer -> OutOfMemory.
        {
            let mut buf = AllocatingWriter::init();
            let item = Path::Optional(vec![b'p'; max + 1]);
            let mut f = entry_formatter(b"a", &mut buf);
            assert_eq!(Err(FormatError::OutOfMemory), f.format_entry(&item));
        }

        // Required paths bypass the fixed buffer entirely (no bound).
        {
            let mut buf = AllocatingWriter::init();
            let item = Path::Required(vec![b'p'; max + 1]);
            let mut f = entry_formatter(b"a", &mut buf);
            f.format_entry(&item).unwrap();
        }
    }

    // rustty-only test (parity fix W94-B01-01): same fixed-buffer boundary for
    // the `RepeatablePath` formatEntry loop (shared `[max_path_bytes + 1]u8`).
    #[test]
    fn repeatable_path_format_config_optional_fixed_buffer_boundary() {
        use rustty_cfg_fmt::FormatError;

        let max = super::libc_max_path_bytes();

        {
            let mut buf = AllocatingWriter::init();
            let list = RepeatablePath {
                value: vec![Path::Optional(vec![b'p'; max])],
            };
            let mut f = entry_formatter(b"a", &mut buf);
            f.format_entry(&list).unwrap();
        }

        {
            let mut buf = AllocatingWriter::init();
            let list = RepeatablePath {
                value: vec![Path::Optional(vec![b'p'; max + 1])],
            };
            let mut f = entry_formatter(b"a", &mut buf);
            assert_eq!(Err(FormatError::OutOfMemory), f.format_entry(&list));
        }
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
    // PARITY (rustty-added): `std.fs.path`'s target-dispatched
    // `isAbsoluteWindows`/`isAbsolutePosix`/`basenameWindows`/`basenamePosix`.
    //
    // Both target arms are compiled on every host, so the Windows semantics are
    // asserted here from macOS/Linux exactly as the reference asserts them
    // cross-target (`std.fs.path`'s own `test isAbsoluteWindows` / `test
    // basename`). The whole table below was GENERATED by running the four zig
    // 0.16 `std.fs.path` functions over these inputs, so it is the oracle's
    // literal output, not a hand-derived expectation.
    #[test]
    fn std_fs_path_is_absolute_and_basename() {
        #[allow(clippy::type_complexity)]
        let cases: &[(&[u8], bool, bool, &[u8], &[u8])] = &[
            (b"", false, false, b"", b""),
            (b"/", true, true, b"", b""),
            (b"//", true, true, b"", b""),
            (b"//server", true, true, b"", b"server"),
            (b"//server/file", true, true, b"", b"file"),
            (b"\\\\server\\file", true, false, b"", b"\\\\server\\file"),
            (b"\\\\server", true, false, b"", b"\\\\server"),
            (b"\\\\", true, false, b"", b"\\\\"),
            (b"c", false, false, b"c", b"c"),
            (b"c:", false, false, b"", b"c:"),
            (b"c:\\", true, false, b"", b"c:\\"),
            (b"c:/", true, false, b"", b"c:"),
            (
                b"c:\\Users\\t\\theme",
                true,
                false,
                b"theme",
                b"c:\\Users\\t\\theme",
            ),
            (b"c:/Users/t/theme", true, false, b"theme", b"theme"),
            (b"c:foo", false, false, b"foo", b"c:foo"),
            (b"/rooted", true, true, b"rooted", b"rooted"),
            (b"\\rooted", true, false, b"rooted", b"\\rooted"),
            (b"foo", false, false, b"foo", b"foo"),
            (b"foo\\bar", false, false, b"bar", b"foo\\bar"),
            (b"\\\\?\\C:\\x", true, false, b"x", b"\\\\?\\C:\\x"),
            (b"\\\\.", true, false, b"", b"\\\\."),
            (b"\\\\?", true, false, b"", b"\\\\?"),
            (
                b"\\dir\\basename.ext",
                true,
                false,
                b"basename.ext",
                b"\\dir\\basename.ext",
            ),
            (
                b"\\basename.ext",
                true,
                false,
                b"basename.ext",
                b"\\basename.ext",
            ),
            (
                b"basename.ext",
                false,
                false,
                b"basename.ext",
                b"basename.ext",
            ),
            (
                b"basename.ext\\",
                false,
                false,
                b"basename.ext",
                b"basename.ext\\",
            ),
            (
                b"basename.ext\\\\",
                false,
                false,
                b"basename.ext",
                b"basename.ext\\\\",
            ),
            (b"/aaa/bbb", true, true, b"bbb", b"bbb"),
            (b"/aaa/", true, true, b"aaa", b"aaa"),
            (b"/a/b", true, true, b"b", b"b"),
            (b"//a", true, true, b"", b"a"),
            (b"C:", false, false, b"", b"C:"),
            (b"C:\\x\\y", true, false, b"y", b"C:\\x\\y"),
            (b"\\\\a\\b\\c", true, false, b"c", b"\\\\a\\b\\c"),
            (b"\\\\a\\b", true, false, b"", b"\\\\a\\b"),
            (b"\\\\a\\b\\", true, false, b"", b"\\\\a\\b\\"),
            (b"\\/a/b/c", true, false, b"c", b"c"),
            (b"theme/", false, false, b"theme", b"theme"),
            (b"theme//", false, false, b"theme", b"theme"),
            (b"a/b/", false, false, b"b", b"b"),
        ];

        for &(path, abs_windows, abs_posix, base_windows, base_posix) in cases {
            let show = String::from_utf8_lossy(path).into_owned();
            assert_eq!(
                is_absolute_windows(path),
                abs_windows,
                "isAbsoluteWindows({show:?})"
            );
            assert_eq!(
                is_absolute_posix(path),
                abs_posix,
                "isAbsolutePosix({show:?})"
            );
            assert_eq!(
                basename_windows(path),
                base_windows,
                "basenameWindows({show:?})"
            );
            assert_eq!(basename_posix(path), base_posix, "basenamePosix({show:?})");
        }

        // The target-dispatched entry points pick the arm for the build target.
        for &(path, abs_windows, abs_posix, base_windows, base_posix) in cases {
            let (want_abs, want_base) = if cfg!(windows) {
                (abs_windows, base_windows)
            } else {
                (abs_posix, base_posix)
            };
            assert_eq!(is_absolute(path), want_abs);
            assert_eq!(basename(path), want_base);
        }
    }

    // PARITY (rustty-added): the `Win32PathType` classification the two Windows
    // helpers rest on (`std.fs.path`'s own `test getWin32PathType` cases).
    #[test]
    fn std_fs_path_get_win32_path_type() {
        use Win32PathType::*;
        let cases: &[(&[u8], Win32PathType)] = &[
            (b"", Relative),
            (b"x", Relative),
            (b"x\\", Relative),
            (b"//.", RootLocalDevice),
            (b"/\\?", RootLocalDevice),
            (b"\\\\?", RootLocalDevice),
            (b"//./x", LocalDevice),
            (b"/\\?\\x", LocalDevice),
            (b"\\\\?\\x", LocalDevice),
            // local device paths require a path separator after the root,
            // otherwise it is considered a UNC path
            (b"\\\\?x", UncAbsolute),
            (b"//.x", UncAbsolute),
            (b"//", UncAbsolute),
            (b"\\\\x", UncAbsolute),
            (b"//x", UncAbsolute),
            (b"\\x", Rooted),
            (b"/", Rooted),
            (b"x:", DriveRelative),
            (b"x:abc", DriveRelative),
            (b"x:a/b/c", DriveRelative),
            (b"x:\\", DriveAbsolute),
            (b"x:\\abc", DriveAbsolute),
            (b"x:/a/b/c", DriveAbsolute),
            // A non-ASCII code point encoded as ONE WTF-16 code unit is a valid
            // drive letter; one that needs two code units is not.
            ("\u{20ac}:\\".as_bytes(), DriveAbsolute),
            ("\u{20ac}:".as_bytes(), DriveRelative),
            ("\u{10000}:\\".as_bytes(), Relative),
        ];
        for &(path, want) in cases {
            assert_eq!(
                get_win32_path_type(path),
                want,
                "getWin32PathType({:?})",
                String::from_utf8_lossy(path)
            );
        }
    }
    // PARITY (rustty-added): `std.fs.path`'s own `test resolveWindows` + the
    // windows half of its `test resolve`, verbatim. Both `resolve` arms are
    // always compiled (only the dispatch is `cfg`-selected), so the Windows
    // designator/`..` resolution is proven from the POSIX host — the same way
    // the reference cross-target-tests it.
    #[test]
    fn std_fs_path_resolve_windows() {
        let cases: &[(&[&[u8]], &[u8])] = &[
            (&[b"a", b"..\\..\\.."], b"..\\.."),
            (&[b"..", b"", b"..\\..\\foo"], b"..\\..\\..\\foo"),
            (&[b"a\\b\\c\\", b"..\\..\\.."], b"."),
            (&[b"."], b"."),
            (&[b""], b"."),
            (
                &[b"Z:\\", b"/usr/local", b"lib\\zig\\std\\array_list.zig"],
                b"Z:\\usr\\local\\lib\\zig\\std\\array_list.zig",
            ),
            (
                &[b"z:\\", b"usr/local", b"lib\\zig"],
                b"Z:\\usr\\local\\lib\\zig",
            ),
            (&[b"c:\\a\\b\\c", b"/hi", b"ok"], b"C:\\hi\\ok"),
            (&[b"c:\\a\\b\\c\\", b".\\..\\foo"], b"C:\\a\\b\\foo"),
            (&[b"c:/blah\\blah", b"d:/games", b"c:../a"], b"C:\\blah\\a"),
            (&[b"c:/blah\\blah", b"d:/games", b"C:../a"], b"C:\\blah\\a"),
            (&[b"c:/ignore", b"d:\\a/b\\c/d", b"\\e.exe"], b"D:\\e.exe"),
            (&[b"c:/ignore", b"c:/some/file"], b"C:\\some\\file"),
            (
                &[b"d:/foo", b"d:some/dir//", b"D:another"],
                b"D:\\foo\\some\\dir\\another",
            ),
            (
                &[b"//server/share", b"..", b"relative\\"],
                b"\\\\server\\share\\relative",
            ),
            (
                &[b"\\\\server/share", b"..", b"relative\\"],
                b"\\\\server\\share\\relative",
            ),
            (
                &[b"\\\\server/share/ignore", b"//server/share/bar"],
                b"\\\\server\\share\\bar",
            ),
            (
                &[b"\\/server\\share/", b"..", b"relative"],
                b"\\\\server\\share\\relative",
            ),
            (
                &[b"\\\\server\\share", b"C:drive-relative"],
                b"C:drive-relative",
            ),
            (&[b"c:/", b"//"], b"\\\\"),
            (&[b"c:/", b"//server"], b"\\\\server"),
            (&[b"c:/", b"//server/share"], b"\\\\server\\share"),
            (&[b"c:/", b"//server//share////"], b"\\\\server\\share\\"),
            (&[b"c:/", b"///some//dir"], b"\\\\\\some\\dir"),
            (&[b"c:foo", b"bar"], b"C:foo\\bar"),
            (
                &[b"C:\\foo\\tmp.3\\", b"..\\tmp.3\\cycles\\root.js"],
                b"C:\\foo\\tmp.3\\cycles\\root.js",
            ),
            (&[b"relative", b"d:foo"], b"D:foo"),
            (&[b"../..\\..", b"d:foo"], b"D:foo"),
            (&[b"../..\\..", b"\\rooted", b"d:foo"], b"D:foo"),
            (&[b"C:\\foo", b"../..\\..", b"\\rooted", b"d:foo"], b"D:foo"),
            (&[b"D:relevant", b"../..\\..", b"d:foo"], b"D:..\\..\\foo"),
            (
                &[
                    b"D:relevant",
                    b"../..\\..",
                    b"\\\\.\\ignored",
                    b"C:\\ignored",
                    b"C:ignored",
                    b"\\\\ignored",
                    b"d:foo",
                ],
                b"D:..\\..\\foo",
            ),
            (
                &[
                    b"ignored",
                    b"\\\\.\\ignored",
                    b"C:\\ignored",
                    b"C:ignored",
                    b"\\\\ignored",
                    b"d:foo",
                ],
                b"D:foo",
            ),
            (&[b"/foo", b"bar"], b"\\foo\\bar"),
            (
                &[b"//server/share/ignore", b"/foo", b"bar"],
                b"\\\\server\\share\\foo\\bar",
            ),
            (&[b"//server/share/", b"/foo"], b"\\\\server\\share\\foo"),
            (&[b"//server/share", b"/foo"], b"\\\\server\\share\\foo"),
            (&[b"//server/", b"/foo"], b"\\\\server\\foo"),
            (&[b"//server", b"/foo"], b"\\\\server\\foo"),
            (&[b"//", b"/foo"], b"\\\\foo"),
            (&[b"C:", b"/foo", b"bar"], b"C:\\foo\\bar"),
            (&[b"C:\\ignore", b"C:", b"/foo", b"bar"], b"C:\\foo\\bar"),
            (&[b"C:\\ignore", b"\\foo", b"C:bar"], b"C:\\foo\\bar"),
            (&[b"\\ignore", b"\\foo"], b"\\foo"),
            (&[b"c:ignore", b"ignore", b"\\ignore", b"\\foo"], b"C:\\foo"),
            (&[b"\\ignore", b"C:foo"], b"C:foo"),
            (&[b"\\ignore", b"\\ignore2", b"C:foo"], b"C:foo"),
            (
                &[b"c:ignore", b"\\ignore", b"\\rooted", b"C:foo"],
                b"C:\\rooted\\foo",
            ),
            (
                &[b"c:\\ignore", b"\\ignore", b"\\rooted", b"C:foo"],
                b"C:\\rooted\\foo",
            ),
            (
                &[b"d:\\ignore", b"\\ignore", b"\\ignore2", b"C:foo"],
                b"C:foo",
            ),
            (&[b"\\/."], b"\\\\."),
            (&[b"\\/.", b"C:drive-relative"], b"C:drive-relative"),
            (&[b"/\\?"], b"\\\\?"),
            (&[b"ignore", b"c:\\ignore", b"\\\\.", b"foo"], b"\\\\.\\foo"),
            (&[b"ignore", b"c:\\ignore", b"\\\\?", b"foo"], b"\\\\?\\foo"),
            (
                &[b"ignore", b"c:\\ignore", b"//.", b"ignore", b"\\foo"],
                b"\\\\.\\foo",
            ),
            (
                &[b"ignore", b"c:\\ignore", b"\\\\?", b"ignore", b"\\foo"],
                b"\\\\?\\foo",
            ),
            (&[b"a/b"], b"a\\b"),
            (&[b".."], b".."),
            (&[b"../.."], b"..\\.."),
            (&[b"C:foo", b"../.."], b"C:.."),
            (&[b"d:foo", b"../..\\.."], b"D:..\\.."),
            (
                &[b"\\\\?\\C:\\foo", b"../bar", b"baz"],
                b"\\\\?\\C:\\bar\\baz",
            ),
            (
                &[b"\\\\.\\C:/foo", b"../../../../bar", b"baz"],
                b"\\\\.\\bar\\baz",
            ),
            (
                &[b"//./C:/foo", b"../../../../bar", b"baz"],
                b"\\\\.\\bar\\baz",
            ),
            (&[b"\\\\.\\foo", b".."], b"\\\\."),
            (&[b"\\\\.\\foo", b"..\\.."], b"\\\\."),
            (&[b"\\??\\C:\\foo", b"/bar", b"baz"], b"\\bar\\baz"),
            (
                &[b"C:\\", b"\\??\\C:\\foo", b"bar"],
                b"C:\\??\\C:\\foo\\bar",
            ),
        ];
        for &(paths, want) in cases {
            assert_eq!(
                resolve_windows(paths),
                want.to_vec(),
                "resolveWindows({:?}) => {:?}",
                paths
                    .iter()
                    .map(|p| String::from_utf8_lossy(p))
                    .collect::<Vec<_>>(),
                String::from_utf8_lossy(&resolve_windows(paths))
            );
        }
    }

    // PARITY (rustty-added): `std.fs.path`'s own `test resolvePosix` + the posix
    // half of its `test resolve`, verbatim.
    #[test]
    fn std_fs_path_resolve_posix() {
        let cases: &[(&[&[u8]], &[u8])] = &[
            (&[b"a", b"../../.."], b"../.."),
            (&[b"..", b"", b"../../foo"], b"../../../foo"),
            (&[b"a/b/c/", b"../../.."], b"."),
            (&[b"."], b"."),
            (&[b""], b"."),
            (&[b"/a/b", b"c"], b"/a/b/c"),
            (&[b"/a/b", b"c", b"//d", b"e///"], b"/d/e"),
            (&[b"/a/b/c", b"..", b"../"], b"/a"),
            (&[b"/", b"..", b".."], b"/"),
            (&[b"/a/b/c/"], b"/a/b/c"),
            (&[b"/var/lib", b"../", b"file/"], b"/var/file"),
            (&[b"/var/lib", b"/../", b"file/"], b"/file"),
            (&[b"/some/dir", b".", b"/absolute/"], b"/absolute"),
            (
                &[b"/foo/tmp.3/", b"../tmp.3/cycles/root.js"],
                b"/foo/tmp.3/cycles/root.js",
            ),
            (&[b"a/b"], b"a/b"),
            (&[b"."], b"."),
            (
                &[b".", b"src/test.zig", b"..", b"../test/cases.zig"],
                b"test/cases.zig",
            ),
        ];
        for &(paths, want) in cases {
            assert_eq!(
                resolve_posix(paths),
                want.to_vec(),
                "resolvePosix({:?}) => {:?}",
                paths
                    .iter()
                    .map(|p| String::from_utf8_lossy(p))
                    .collect::<Vec<_>>(),
                String::from_utf8_lossy(&resolve_posix(paths))
            );
        }
    }
}
