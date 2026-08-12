//! rustty-config::repeatable_string_map
//!
//! PORTS: ghostty src/config/RepeatableStringMap.zig — a repeatable key/value
//! string map (`RepeatableStringMap` = `@This()`). C1 leaf value-type (4 tests).
//!
//! PORT STATUS: ported (every line/comment/test{}, R1).

/// RepeatableStringMap is a key/value that can be repeated to accumulate a
/// string map. This isn't called "StringMap" because I find that sometimes
/// leads to confusion that it _accepts_ a map such as JSON dict.
// const RepeatableStringMap = @This();
// const std = @import("std");
// const Allocator = std.mem.Allocator;
//
// const formatterpkg = @import("formatter.zig");
use rustty_cfg_fmt::{format_entry_string, format_entry_void, FormatEntry, FormatError, Writer};
use rustty_zigstd::array_hash_map::AutoArrayHashMapUnmanaged;

// const Map = std.ArrayHashMapUnmanaged(
//     [:0]const u8,
//     [:0]const u8,
//     std.array_hash_map.StringContext,
//     true,
// );
//
// PORT NOTE: `ArrayHashMapUnmanaged([:0]const u8, [:0]const u8, StringContext,
// true)` is an insertion-ordered string map (the `true` stores the hash). Its
// rustty analogue is `AutoArrayHashMapUnmanaged<Vec<u8>, Vec<u8>>` (insertion-
// ordered, `orderedRemove` shifts following entries — preserving order). Keys
// and values are raw bytes (`[:0]const u8` -> `Vec<u8>`, never lossy `String`).
type Map = AutoArrayHashMapUnmanaged<Vec<u8>, Vec<u8>>;

/// PORTS: `RepeatableStringMap` (the `@This()` struct).
// Allocator for the list is the arena for the parent config.
// map: Map = .{},
#[derive(Default)]
pub struct RepeatableStringMap {
    pub map: Map,
}

/// PORTS: the error set of `parseCLI` — `error.ValueRequired || Allocator.Error`.
#[derive(Debug, PartialEq, Eq)]
pub enum ParseError {
    ValueRequired,
}

impl RepeatableStringMap {
    // pub fn parseCLI(
    //     self: *RepeatableStringMap,
    //     alloc: Allocator,
    //     input: ?[]const u8,
    // ) !void {
    pub fn parse_cli(&mut self, input: Option<&[u8]>) -> Result<(), ParseError> {
        // const value = input orelse return error.ValueRequired;
        let value = input.ok_or(ParseError::ValueRequired)?;

        // Empty value resets the list. We don't need to free our values because
        // the allocator used is always an arena.
        // if (value.len == 0) {
        //     self.map.clearRetainingCapacity();
        //     return;
        // }
        if value.is_empty() {
            self.map.clear_retaining_capacity();
            return Ok(());
        }

        // const index = std.mem.indexOfScalar(u8, value, '=') orelse return error.ValueRequired;
        let index = index_of_scalar(value, b'=').ok_or(ParseError::ValueRequired)?;

        // const key = std.mem.trim(u8, value[0..index], &std.ascii.whitespace);
        let key = trim_whitespace(&value[0..index]);
        // const val = std.mem.trim(u8, value[index + 1 ..], &std.ascii.whitespace);
        let val = trim_whitespace(&value[index + 1..]);

        // const key_copy = try alloc.dupeZ(u8, key);
        // errdefer alloc.free(key_copy);
        let key_copy = key.to_vec();

        // Empty value removes the key from the map.
        // if (val.len == 0) {
        //     _ = self.map.orderedRemove(key_copy);
        //     alloc.free(key_copy);
        //     return;
        // }
        if val.is_empty() {
            let _ = self.map.ordered_remove(&key_copy);
            return Ok(());
        }

        // const val_copy = try alloc.dupeZ(u8, val);
        // errdefer alloc.free(val_copy);
        let val_copy = val.to_vec();

        // try self.map.put(alloc, key_copy, val_copy);
        self.map.put(key_copy, val_copy);

        Ok(())
    }

    /// Deep copy of the struct. Required by Config.
    // pub fn clone(
    //     self: *const RepeatableStringMap,
    //     alloc: Allocator,
    // ) Allocator.Error!RepeatableStringMap {
    //     var map: Map = .{};
    //     try map.ensureTotalCapacity(alloc, self.map.count());
    //
    //     errdefer { ... }
    //
    //     var it = self.map.iterator();
    //     while (it.next()) |entry| {
    //         const key = try alloc.dupeZ(u8, entry.key_ptr.*);
    //         const value = try alloc.dupeZ(u8, entry.value_ptr.*);
    //         map.putAssumeCapacity(key, value);
    //     }
    //
    //     return .{ .map = map };
    // }
    //
    // PORT NOTE: the explicit `ensureTotalCapacity` + `putAssumeCapacity` is a
    // perf detail (pre-size, then no-realloc inserts); the `errdefer` frees the
    // partial map on OOM. With infallible allocation, `put` in insertion order
    // is the faithful equivalent and preserves order.
    //
    // PARITY: named `clone` to match the reference method (Config relies on it).
    #[allow(clippy::should_implement_trait)]
    pub fn clone(&self) -> RepeatableStringMap {
        // var map: Map = .{};
        let mut map: Map = Map::new();

        // var it = self.map.iterator();
        // while (it.next()) |entry| { ... }
        for (key_ptr, value_ptr) in self.map.iter() {
            // const key = try alloc.dupeZ(u8, entry.key_ptr.*);
            let key = key_ptr.clone();
            // const value = try alloc.dupeZ(u8, entry.value_ptr.*);
            let value = value_ptr.clone();
            // map.putAssumeCapacity(key, value);
            map.put(key, value);
        }

        // return .{ .map = map };
        RepeatableStringMap { map }
    }

    /// The number of items in the map
    // pub fn count(self: RepeatableStringMap) usize {
    //     return self.map.count();
    // }
    pub fn count(&self) -> usize {
        self.map.count()
    }

    /// Iterator over the entries in the map.
    // pub fn iterator(self: RepeatableStringMap) Map.Iterator {
    //     return self.map.iterator();
    // }
    pub fn iterator(&self) -> impl Iterator<Item = (&Vec<u8>, &Vec<u8>)> {
        self.map.iter()
    }

    /// Compare if two of our value are requal. Required by Config.
    // pub fn equal(self: RepeatableStringMap, other: RepeatableStringMap) bool {
    //     if (self.map.count() != other.map.count()) return false;
    //     var it = self.map.iterator();
    //     while (it.next()) |entry| {
    //         const value = other.map.get(entry.key_ptr.*) orelse return false;
    //         if (!std.mem.eql(u8, entry.value_ptr.*, value)) return false;
    //     } else return true;
    // }
    pub fn equal(&self, other: &RepeatableStringMap) -> bool {
        // if (self.map.count() != other.map.count()) return false;
        if self.map.count() != other.map.count() {
            return false;
        }
        // var it = self.map.iterator();
        // while (it.next()) |entry| { ... } else return true;
        for (key_ptr, value_ptr) in self.map.iter() {
            // const value = other.map.get(entry.key_ptr.*) orelse return false;
            let value = match other.map.get(key_ptr) {
                Some(v) => v,
                None => return false,
            };
            // if (!std.mem.eql(u8, entry.value_ptr.*, value)) return false;
            if value_ptr != value {
                return false;
            }
        }
        true
    }

    // Used by formatter
    // pub fn formatEntry(self: RepeatableStringMap, formatter: formatterpkg.EntryFormatter) !void {
    //
    // PORT NOTE: implemented via the `FormatEntry` trait. See the impl below.
}

impl FormatEntry for RepeatableStringMap {
    // pub fn formatEntry(self: RepeatableStringMap, formatter: formatterpkg.EntryFormatter) !void {
    fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        // If no items, we want to render an empty field.
        // if (self.map.count() == 0) {
        //     try formatter.formatEntry(void, {});
        //     return;
        // }
        if self.map.count() == 0 {
            return format_entry_void(name, writer);
        }

        // var it = self.map.iterator();
        // while (it.next()) |entry| {
        //     var buf: [256]u8 = undefined;
        //     const value = std.fmt.bufPrint(&buf, "{s}={s}", .{ entry.key_ptr.*, entry.value_ptr.* }) catch |err| switch (err) {
        //         error.NoSpaceLeft => return error.OutOfMemory,
        //     };
        //     try formatter.formatEntry([]const u8, value);
        // }
        for (key_ptr, value_ptr) in self.map.iter() {
            // const value = std.fmt.bufPrint(&buf, "{s}={s}", .{ key, value }) catch |err| switch (err) {
            //     error.NoSpaceLeft => return error.OutOfMemory,
            // };
            //
            // PORT NOTE: the reference formats each entry through a FIXED
            // `[256]u8` stack buffer; `key=value` exceeding it is
            // `error.NoSpaceLeft` -> `error.OutOfMemory`. The growable Vec is
            // only the scratch — the fixed capacity is enforced by the bound
            // check (byte-identical output when it fits).
            if key_ptr.len() + 1 + value_ptr.len() > 256 {
                return Err(FormatError::OutOfMemory);
            }
            let mut value: Vec<u8> = Vec::new();
            value.extend_from_slice(key_ptr);
            value.push(b'=');
            value.extend_from_slice(value_ptr);
            // try formatter.formatEntry([]const u8, value);
            format_entry_string(name, &value, writer)?;
        }
        Ok(())
    }
}

/// PORTS: `std.mem.indexOfScalar(u8, slice, value)`.
fn index_of_scalar(slice: &[u8], value: u8) -> Option<usize> {
    slice.iter().position(|&b| b == value)
}

/// PORTS: `std.mem.trim(u8, s, &std.ascii.whitespace)`.
//
// PORT NOTE: `std.ascii.whitespace` is `{ ' ', '\t', '\n', '\r', 0x0B, 0x0C }`
// (space, tab, LF, CR, vertical tab, form feed).
fn trim_whitespace(s: &[u8]) -> &[u8] {
    const WS: &[u8] = &[b' ', b'\t', b'\n', b'\r', 0x0B, 0x0C];
    let is_ws = |b: u8| WS.contains(&b);
    let mut start = 0;
    let mut end = s.len();
    while start < end && is_ws(s[start]) {
        start += 1;
    }
    while end > start && is_ws(s[end - 1]) {
        end -= 1;
    }
    &s[start..end]
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustty_cfg_fmt::{entry_formatter, AllocatingWriter};

    // test "RepeatableStringMap: parseCLI" {
    #[test]
    fn repeatable_string_map_parse_cli() {
        // var map: RepeatableStringMap = .{};
        let mut map = RepeatableStringMap::default();

        // try testing.expectError(error.ValueRequired, map.parseCLI(alloc, "A"));
        assert_eq!(Err(ParseError::ValueRequired), map.parse_cli(Some(b"A")));

        // try map.parseCLI(alloc, "A=B");
        map.parse_cli(Some(b"A=B")).unwrap();
        // try map.parseCLI(alloc, "B=C");
        map.parse_cli(Some(b"B=C")).unwrap();
        // try testing.expectEqual(@as(usize, 2), map.count());
        assert_eq!(2, map.count());

        // try map.parseCLI(alloc, "");
        map.parse_cli(Some(b"")).unwrap();
        // try testing.expectEqual(@as(usize, 0), map.count());
        assert_eq!(0, map.count());

        // try map.parseCLI(alloc, "A=B");
        map.parse_cli(Some(b"A=B")).unwrap();
        // try testing.expectEqual(@as(usize, 1), map.count());
        assert_eq!(1, map.count());
        // try map.parseCLI(alloc, "A=C");
        map.parse_cli(Some(b"A=C")).unwrap();
        // try testing.expectEqual(@as(usize, 1), map.count());
        assert_eq!(1, map.count());
    }

    // test "RepeatableStringMap: formatConfig empty" {
    #[test]
    fn repeatable_string_map_format_config_empty() {
        // var buf: std.Io.Writer.Allocating = .init(testing.allocator);
        let mut buf = AllocatingWriter::init();

        // var list: RepeatableStringMap = .{};
        let list = RepeatableStringMap::default();
        // try list.formatEntry(formatterpkg.entryFormatter("a", &buf.writer));
        let mut f = entry_formatter(b"a", &mut buf);
        f.format_entry(&list).unwrap();
        // try std.testing.expectEqualSlices(u8, "a = \n", buf.written());
        assert_eq!(b"a = \n".as_slice(), buf.written());
    }

    // test "RepeatableStringMap: formatConfig single item" {
    #[test]
    fn repeatable_string_map_format_config_single_item() {
        {
            // var buf: std.Io.Writer.Allocating = .init(testing.allocator);
            let mut buf = AllocatingWriter::init();
            // var map: RepeatableStringMap = .{};
            let mut map = RepeatableStringMap::default();
            // try map.parseCLI(alloc, "A=B");
            map.parse_cli(Some(b"A=B")).unwrap();
            // try map.formatEntry(formatterpkg.entryFormatter("a", &buf.writer));
            let mut f = entry_formatter(b"a", &mut buf);
            f.format_entry(&map).unwrap();
            // try std.testing.expectEqualSlices(u8, "a = A=B\n", buf.written());
            assert_eq!(b"a = A=B\n".as_slice(), buf.written());
        }
        {
            // var buf: std.Io.Writer.Allocating = .init(testing.allocator);
            let mut buf = AllocatingWriter::init();
            // var map: RepeatableStringMap = .{};
            let mut map = RepeatableStringMap::default();
            // try map.parseCLI(alloc, " A = B ");
            map.parse_cli(Some(b" A = B ")).unwrap();
            // try map.formatEntry(formatterpkg.entryFormatter("a", &buf.writer));
            let mut f = entry_formatter(b"a", &mut buf);
            f.format_entry(&map).unwrap();
            // try std.testing.expectEqualSlices(u8, "a = A=B\n", buf.written());
            assert_eq!(b"a = A=B\n".as_slice(), buf.written());
        }
    }

    // rustty-only test (parity fix W95-B04-01): the reference formats each
    // `key=value` entry through a FIXED `[256]u8` buffer
    // (`error.NoSpaceLeft` -> `return error.OutOfMemory`). A payload
    // (`key` + `=` + `value`) of exactly 256 bytes fits; 257 and 300 overflow.
    // Lock both sides of the boundary.
    #[test]
    fn repeatable_string_map_format_config_fixed_buffer_boundary() {
        use rustty_cfg_fmt::FormatError;

        // key "k" + '=' + 254-byte value == 256: fits.
        {
            let mut buf = AllocatingWriter::init();
            let mut map = RepeatableStringMap::default();
            let mut input: Vec<u8> = b"k=".to_vec();
            input.extend_from_slice(&vec![b'v'; 254]);
            map.parse_cli(Some(&input)).unwrap();
            let mut f = entry_formatter(b"a", &mut buf);
            f.format_entry(&map).unwrap();
            let mut expected: Vec<u8> = b"a = k=".to_vec();
            expected.extend_from_slice(&vec![b'v'; 254]);
            expected.push(b'\n');
            assert_eq!(expected.as_slice(), buf.written());
        }

        // key "k" + '=' + 255-byte value == 257: overflows -> OutOfMemory.
        {
            let mut buf = AllocatingWriter::init();
            let mut map = RepeatableStringMap::default();
            let mut input: Vec<u8> = b"k=".to_vec();
            input.extend_from_slice(&vec![b'v'; 255]);
            map.parse_cli(Some(&input)).unwrap();
            let mut f = entry_formatter(b"a", &mut buf);
            assert_eq!(Err(FormatError::OutOfMemory), f.format_entry(&map));
        }

        // key "k" + '=' + 298-byte value == 300: overflows -> OutOfMemory.
        {
            let mut buf = AllocatingWriter::init();
            let mut map = RepeatableStringMap::default();
            let mut input: Vec<u8> = b"k=".to_vec();
            input.extend_from_slice(&vec![b'v'; 298]);
            map.parse_cli(Some(&input)).unwrap();
            let mut f = entry_formatter(b"a", &mut buf);
            assert_eq!(Err(FormatError::OutOfMemory), f.format_entry(&map));
        }
    }

    // test "RepeatableStringMap: formatConfig multiple items" {
    #[test]
    fn repeatable_string_map_format_config_multiple_items() {
        {
            // var buf: std.Io.Writer.Allocating = .init(testing.allocator);
            let mut buf = AllocatingWriter::init();
            // var list: RepeatableStringMap = .{};
            let mut list = RepeatableStringMap::default();
            // try list.parseCLI(alloc, "A=B");
            list.parse_cli(Some(b"A=B")).unwrap();
            // try list.parseCLI(alloc, "B = C");
            list.parse_cli(Some(b"B = C")).unwrap();
            // try list.formatEntry(formatterpkg.entryFormatter("a", &buf.writer));
            let mut f = entry_formatter(b"a", &mut buf);
            f.format_entry(&list).unwrap();
            // try std.testing.expectEqualSlices(u8, "a = A=B\na = B=C\n", buf.written());
            assert_eq!(b"a = A=B\na = B=C\n".as_slice(), buf.written());
        }
    }
}
