//! rustty-config::io
//!
//! PORTS: ghostty src/config/io.zig — readable IO source value-types
//! (`ReadableIO`, `RepeatableReadableIO`). C1 leaf value-type (4 tests).
//!
//! PORT STATUS: ported (every line/comment/test{}, R1).

// const std = @import("std");
// const assert = @import("../quirks.zig").inlineAssert;
// const Allocator = std.mem.Allocator;
// const ArenaAllocator = std.heap.ArenaAllocator;
// const string = @import("string.zig");
// const formatterpkg = @import("formatter.zig");
// const cli = @import("../cli.zig");
use crate::string;
use rustty_args::args::split_tagged_union;
use rustty_cfg_fmt::{format_entry_void, FormatEntry, Writer};
use rustty_util::quirks::inlineAssert as assert;

/// PORTS: the error set used by `ReadableIO`/`RepeatableReadableIO` `parseCLI` —
/// `error.ValueRequired || error.InvalidString || Allocator.Error` (the
/// `InvalidValue` from `parseTaggedUnion` is caught internally, never surfaced).
#[derive(Debug, PartialEq, Eq)]
pub enum ParseError {
    ValueRequired,
    InvalidString,
}

/// ReadableIO is some kind of IO source that is readable.
///
/// It can be either a direct string or a filepath. The filepath will
/// be deferred and read later, so it won't be checked for existence
/// or readability at configuration time. This allows using a path that
/// might be produced in an intermediate state.
// pub const ReadableIO = union(enum) {
//     const Self = @This();
pub enum ReadableIO {
    // raw: [:0]const u8,
    Raw(Vec<u8>),
    // path: [:0]const u8,
    Path(Vec<u8>),
}

impl ReadableIO {
    // pub fn parseCLI(
    //     self: *Self,
    //     alloc: Allocator,
    //     input_: ?[]const u8,
    // ) !void {
    pub fn parse_cli(&mut self, input_: Option<&[u8]>) -> Result<(), ParseError> {
        // const input = input_ orelse return error.ValueRequired;
        let input = input_.ok_or(ParseError::ValueRequired)?;
        // if (input.len == 0) return error.ValueRequired;
        if input.is_empty() {
            return Err(ParseError::ValueRequired);
        }

        // We create a buffer only to do string parsing and validate
        // it works. We store the value as raw so that our formatting
        // can recreate it.
        // {
        //     const buf = try alloc.alloc(u8, input.len);
        //     defer alloc.free(buf);
        //     _ = try string.parse(buf, input);
        // }
        {
            let mut buf = vec![0u8; input.len()];
            string::parse(&mut buf, input).map_err(|err| match err {
                string::ParseError::OutOfMemory => ParseError::InvalidString,
                string::ParseError::InvalidString => ParseError::InvalidString,
            })?;
        }

        // Next, parse the tagged union using normal rules.
        // self.* = cli.args.parseTaggedUnion(
        //     Self,
        //     alloc,
        //     input,
        // ) catch |err| switch (err) {
        //     // Invalid values in the tagged union are interpreted as
        //     // raw values. This lets users pass in simple string values
        //     // without needing to tag them.
        //     error.InvalidValue => .{ .raw = try alloc.dupeZ(u8, input) },
        //     else => return err,
        // };
        //
        // PORT NOTE: `parseTaggedUnion(Self, alloc, input)` splits `tag:value` and
        // dispatches to the matching union field. The comptime field synthesis is
        // not reproducible (see `rustty-args::args::split_tagged_union`); instead
        // we split, match the tag against `raw`/`path`, and on an unknown tag fall
        // back to `.raw = input` (the reference's `error.InvalidValue` arm). Both
        // `raw`/`path` are plain string fields, so the per-field parse is a dupe.
        let (tag, value) = split_tagged_union(input);
        *self = match tag {
            b"raw" => ReadableIO::Raw(value.to_vec()),
            b"path" => ReadableIO::Path(value.to_vec()),
            // Invalid values in the tagged union are interpreted as
            // raw values. This lets users pass in simple string values
            // without needing to tag them.
            _ => ReadableIO::Raw(input.to_vec()),
        };

        Ok(())
    }

    // pub fn clone(self: Self, alloc: Allocator) Allocator.Error!Self {
    //     return switch (self) {
    //         .raw => |v| .{ .raw = try alloc.dupeZ(u8, v) },
    //         .path => |v| .{ .path = try alloc.dupeZ(u8, v) },
    //     };
    // }
    //
    // PARITY: named `clone` to match the reference method (Config relies on it).
    #[allow(clippy::should_implement_trait)]
    pub fn clone(&self) -> ReadableIO {
        match self {
            ReadableIO::Raw(v) => ReadableIO::Raw(v.clone()),
            ReadableIO::Path(v) => ReadableIO::Path(v.clone()),
        }
    }

    /// Same as clone but also parses the values as Zig strings in
    /// the final resulting value all at once so we can avoid extra
    /// allocations.
    // pub fn cloneParsed(
    //     self: Self,
    //     alloc: Allocator,
    // ) Allocator.Error!Self {
    //     switch (self) {
    //         inline else => |v, tag| {
    //             // Parsing can't fail because we validate it in parseCLI
    //             const copied = try alloc.dupeZ(u8, v);
    //             const parsed = string.parse(copied, v) catch unreachable;
    //             assert(copied.ptr == parsed.ptr);
    //
    //             // If we parsed less than our original length we need
    //             // to keep it null-terminated.
    //             if (parsed.len < copied.len) copied[parsed.len] = 0;
    //
    //             return @unionInit(
    //                 Self,
    //                 @tagName(tag),
    //                 copied[0..parsed.len :0],
    //             );
    //         },
    //     }
    // }
    //
    // PORT NOTE: the reference parses in place (`copied` aliases the parse output;
    // `assert(copied.ptr == parsed.ptr)` confirms the in-place decode), then
    // truncates to the parsed length. Here we copy `v`, parse into the copy
    // (the buffers may alias per `string.parse`'s contract), and truncate to the
    // parsed length. The `[0..parsed.len :0]` re-sentinel is implicit (Vec).
    pub fn clone_parsed(&self) -> ReadableIO {
        // inline else => |v, tag| { ... }
        let rebuild = |v: &[u8]| -> Vec<u8> {
            // const copied = try alloc.dupeZ(u8, v);
            let mut copied = v.to_vec();
            // const parsed = string.parse(copied, v) catch unreachable;
            // Parsing can't fail because we validate it in parseCLI
            //
            // PORT NOTE: `string.parse` writes into `copied` from a separate read
            // of `v`; to honor the "buffers may alias" in-place decode the
            // reference relies on, we parse from a snapshot of the source bytes
            // into `copied`, then truncate. `assert(copied.ptr == parsed.ptr)` is
            // the reference's in-place check; here the equivalent invariant is
            // that we write back into `copied`.
            let src = copied.clone();
            let parsed_len = string::parse(&mut copied, &src).unwrap();
            // assert(copied.ptr == parsed.ptr);
            assert(copied.len() >= parsed_len);
            // If we parsed less than our original length we need
            // to keep it null-terminated.
            // if (parsed.len < copied.len) copied[parsed.len] = 0;
            copied.truncate(parsed_len);
            copied
        };

        match self {
            ReadableIO::Raw(v) => ReadableIO::Raw(rebuild(v)),
            ReadableIO::Path(v) => ReadableIO::Path(rebuild(v)),
        }
    }

    // pub fn equal(self: Self, other: Self) bool {
    //     if (std.meta.activeTag(self) != std.meta.activeTag(other)) {
    //         return false;
    //     }
    //
    //     return switch (self) {
    //         .raw => |v| std.mem.eql(u8, v, other.raw),
    //         .path => |v| std.mem.eql(u8, v, other.path),
    //     };
    // }
    pub fn equal(&self, other: &ReadableIO) -> bool {
        // if (std.meta.activeTag(self) != std.meta.activeTag(other)) return false;
        // return switch (self) {
        //     .raw => |v| std.mem.eql(u8, v, other.raw),
        //     .path => |v| std.mem.eql(u8, v, other.path),
        // };
        match (self, other) {
            (ReadableIO::Raw(v), ReadableIO::Raw(o)) => v == o,
            (ReadableIO::Path(v), ReadableIO::Path(o)) => v == o,
            _ => false,
        }
    }

    // pub fn formatEntry(self: Self, formatter: formatterpkg.EntryFormatter) !void {
    //
    // PORT NOTE: implemented via the `FormatEntry` trait. See the impl below.
}

impl FormatEntry for ReadableIO {
    // pub fn formatEntry(self: Self, formatter: formatterpkg.EntryFormatter) !void {
    fn format_entry<W: Writer>(&self, name: &[u8], writer: &mut W) -> Result<(), W::Error> {
        // var buf: [4096]u8 = undefined;
        // var writer: std.Io.Writer = .fixed(&buf);
        // switch (self) {
        //     inline else => |v, tag| {
        //         writer.writeAll(@tagName(tag)) catch return error.OutOfMemory;
        //         writer.writeByte(':') catch return error.OutOfMemory;
        //         writer.writeAll(v) catch return error.OutOfMemory;
        //     },
        // }
        //
        // try formatter.formatEntry(
        //     []const u8,
        //     writer.buffered(),
        // );
        //
        // PORT NOTE: the reference writes `tag:value` into a 4096-byte fixed
        // buffer then passes the accumulated bytes through the string-slice arm.
        // We build the same bytes and route through `format_entry_string`.
        let (tag, v): (&[u8], &[u8]) = match self {
            ReadableIO::Raw(v) => (b"raw", v),
            ReadableIO::Path(v) => (b"path", v),
        };
        let mut buf: Vec<u8> = Vec::new();
        buf.extend_from_slice(tag);
        buf.push(b':');
        buf.extend_from_slice(v);
        rustty_cfg_fmt::format_entry_string(name, &buf, writer)
    }
}

// pub const RepeatableReadableIO = struct {
//     const Self = @This();
//
//     // Allocator for the list is the arena for the parent config.
//     list: std.ArrayListUnmanaged(ReadableIO) = .{},
#[derive(Default)]
pub struct RepeatableReadableIO {
    pub list: Vec<ReadableIO>,
}

impl RepeatableReadableIO {
    // pub fn parseCLI(
    //     self: *Self,
    //     alloc: Allocator,
    //     input: ?[]const u8,
    // ) !void {
    pub fn parse_cli(&mut self, input: Option<&[u8]>) -> Result<(), ParseError> {
        // const value = input orelse return error.ValueRequired;
        let value = input.ok_or(ParseError::ValueRequired)?;

        // Empty value resets the list
        // if (value.len == 0) {
        //     self.list.clearRetainingCapacity();
        //     return;
        // }
        if value.is_empty() {
            self.list.clear();
            return Ok(());
        }

        // var io: ReadableIO = undefined;
        // try ReadableIO.parseCLI(&io, alloc, value);
        let mut io = ReadableIO::Raw(Vec::new());
        io.parse_cli(Some(value))?;
        // try self.list.append(alloc, io);
        self.list.push(io);

        Ok(())
    }

    /// Deep copy of the struct. Required by Config.
    // pub fn clone(self: *const Self, alloc: Allocator) Allocator.Error!Self {
    //     var list = try std.ArrayListUnmanaged(ReadableIO).initCapacity(alloc, self.list.items.len);
    //     for (self.list.items) |item| {
    //         const copy = try item.clone(alloc);
    //         list.appendAssumeCapacity(copy);
    //     }
    //
    //     return .{ .list = list };
    // }
    //
    // PARITY: named `clone` to match the reference method (Config relies on it).
    #[allow(clippy::should_implement_trait)]
    pub fn clone(&self) -> RepeatableReadableIO {
        let mut list = Vec::with_capacity(self.list.len());
        for item in &self.list {
            let copy = item.clone();
            list.push(copy);
        }
        RepeatableReadableIO { list }
    }

    /// See ReadableIO.cloneParsed
    // pub fn cloneParsed(
    //     self: *const Self,
    //     alloc: Allocator,
    // ) Allocator.Error!Self {
    //     var list = try std.ArrayListUnmanaged(ReadableIO).initCapacity(alloc, self.list.items.len);
    //     for (self.list.items) |item| {
    //         const copy = try item.cloneParsed(alloc);
    //         list.appendAssumeCapacity(copy);
    //     }
    //
    //     return .{ .list = list };
    // }
    pub fn clone_parsed(&self) -> RepeatableReadableIO {
        let mut list = Vec::with_capacity(self.list.len());
        for item in &self.list {
            let copy = item.clone_parsed();
            list.push(copy);
        }
        RepeatableReadableIO { list }
    }

    /// Compare if two of our value are requal. Required by Config.
    // pub fn equal(self: Self, other: Self) bool {
    //     const itemsA = self.list.items;
    //     const itemsB = other.list.items;
    //     if (itemsA.len != itemsB.len) return false;
    //     for (itemsA, itemsB) |a, b| {
    //         if (!a.equal(b)) return false;
    //     } else return true;
    // }
    pub fn equal(&self, other: &RepeatableReadableIO) -> bool {
        // const itemsA = self.list.items;
        let items_a = &self.list;
        // const itemsB = other.list.items;
        let items_b = &other.list;
        // if (itemsA.len != itemsB.len) return false;
        if items_a.len() != items_b.len() {
            return false;
        }
        // for (itemsA, itemsB) |a, b| {
        //     if (!a.equal(b)) return false;
        // } else return true;
        for (a, b) in items_a.iter().zip(items_b.iter()) {
            if !a.equal(b) {
                return false;
            }
        }
        true
    }

    // Used by Formatter
    // pub fn formatEntry(
    //     self: Self,
    //     formatter: formatterpkg.EntryFormatter,
    // ) !void {
    //
    // PORT NOTE: implemented via the `FormatEntry` trait. See the impl below.
}

impl FormatEntry for RepeatableReadableIO {
    // pub fn formatEntry(self: Self, formatter: formatterpkg.EntryFormatter) !void {
    fn format_entry<W: Writer>(&self, name: &[u8], writer: &mut W) -> Result<(), W::Error> {
        // if (self.list.items.len == 0) {
        //     try formatter.formatEntry(void, {});
        //     return;
        // }
        if self.list.is_empty() {
            return format_entry_void(name, writer);
        }

        // for (self.list.items) |value| {
        //     try formatter.formatEntry(ReadableIO, value);
        // }
        for value in &self.list {
            value.format_entry(name, writer)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustty_cfg_fmt::{entry_formatter, AllocatingWriter};

    // test "parseCLI" {  (ReadableIO)
    #[test]
    fn readable_io_parse_cli() {
        {
            // var io: Self = undefined;
            let mut io = ReadableIO::Raw(Vec::new());
            // try Self.parseCLI(&io, alloc, "foo");
            io.parse_cli(Some(b"foo")).unwrap();
            // try testing.expect(io == .raw);
            // try testing.expectEqualStrings("foo", io.raw);
            match &io {
                ReadableIO::Raw(v) => assert_eq!(v.as_slice(), b"foo"),
                _ => panic!("expected raw"),
            }
        }
        {
            // var io: Self = undefined;
            let mut io = ReadableIO::Raw(Vec::new());
            // try Self.parseCLI(&io, alloc, "raw:foo");
            io.parse_cli(Some(b"raw:foo")).unwrap();
            // try testing.expect(io == .raw);
            // try testing.expectEqualStrings("foo", io.raw);
            match &io {
                ReadableIO::Raw(v) => assert_eq!(v.as_slice(), b"foo"),
                _ => panic!("expected raw"),
            }
        }
        {
            // var io: Self = undefined;
            let mut io = ReadableIO::Raw(Vec::new());
            // try Self.parseCLI(&io, alloc, "path:foo");
            io.parse_cli(Some(b"path:foo")).unwrap();
            // try testing.expect(io == .path);
            // try testing.expectEqualStrings("foo", io.path);
            match &io {
                ReadableIO::Path(v) => assert_eq!(v.as_slice(), b"foo"),
                _ => panic!("expected path"),
            }
        }
    }

    // test "formatEntry" {  (ReadableIO)
    #[test]
    fn readable_io_format_entry() {
        // var buf: std.Io.Writer.Allocating = .init(alloc);
        let mut buf = AllocatingWriter::init();

        // var v: Self = undefined;
        let mut v = ReadableIO::Raw(Vec::new());
        // try v.parseCLI(alloc, "raw:foo");
        v.parse_cli(Some(b"raw:foo")).unwrap();
        // try v.formatEntry(formatterpkg.entryFormatter("a", &buf.writer));
        let mut f = entry_formatter(b"a", &mut buf);
        f.format_entry(&v).unwrap();
        // try std.testing.expectEqualSlices(u8, "a = raw:foo\n", buf.written());
        assert_eq!(b"a = raw:foo\n".as_slice(), buf.written());
    }

    // test "parseCLI" {  (RepeatableReadableIO)
    #[test]
    fn repeatable_readable_io_parse_cli() {
        // var list: Self = .{};
        let mut list = RepeatableReadableIO::default();
        // try list.parseCLI(alloc, "raw:A");
        list.parse_cli(Some(b"raw:A")).unwrap();
        // try list.parseCLI(alloc, "path:B");
        list.parse_cli(Some(b"path:B")).unwrap();
        // try testing.expectEqual(@as(usize, 2), list.list.items.len);
        assert_eq!(2, list.list.len());

        // try list.parseCLI(alloc, "");
        list.parse_cli(Some(b"")).unwrap();
        // try testing.expectEqual(@as(usize, 0), list.list.items.len);
        assert_eq!(0, list.list.len());
    }

    // test {
    //     _ = ReadableIO;
    //     _ = RepeatableReadableIO;
    // }
    //
    // PORT NOTE: the reference's bare `test { _ = ReadableIO; _ =
    // RepeatableReadableIO; }` forces analysis of both types' decls. In Rust the
    // `#[test]` functions above compile and run those directly; this
    // reference-analysis test is a no-op here (kept for the test-count contract).
    #[test]
    fn ref_all_decls() {
        let _ = ReadableIO::Raw(Vec::new());
        let _ = RepeatableReadableIO::default();
    }
}
