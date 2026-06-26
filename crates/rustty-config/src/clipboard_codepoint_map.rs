//! rustty-config::clipboard_codepoint_map
//!
//! PORTS: ghostty src/config/ClipboardCodepointMap.zig — the clipboard
//! codepoint-replacement map (`ClipboardCodepointMap` = `@This()`). C1 leaf
//! value-type (0 tests).
//!
//! PORT STATUS: ported (every line/comment, R1).

/// ClipboardCodepointMap is a map of codepoints to replacement values
/// for clipboard operations. When copying text to clipboard, matching
/// codepoints will be replaced with their mapped values.
// const ClipboardCodepointMap = @This();
//
// const std = @import("std");
// const assert = @import("../quirks.zig").inlineAssert;
// const Allocator = std.mem.Allocator;
use rustty_util::quirks::inlineAssert as assert;

// To ease our usage later, we map it directly to formatter entries.
// pub const Entry = @import("../terminal/formatter.zig").CodepointMap;
// pub const Replacement = Entry.Replacement;
//
// PORT NOTE: `terminal/formatter.zig` lives in `rustty-vt`; `CodepointMap`/
// `Replacement` are re-exported here under the reference names `Entry`/
// `Replacement`.
pub use rustty_vt::formatter::CodepointMap as Entry;
pub use rustty_vt::formatter::Replacement;

/// PORTS: `ClipboardCodepointMap` (the `@This()` struct).
///
/// The list of entries. We use a multiarraylist for cache-friendly lookups.
///
/// Note: we do a linear search because we expect to always have very
/// few entries, so the overhead of a binary search is not worth it.
// list: std.MultiArrayList(Entry) = .{},
//
// PORT NOTE: `std.MultiArrayList(Entry)` (struct-of-arrays) -> `Vec<Entry>`. The
// SoA cache optimization is not part of the parity contract for this small,
// linearly-searched map (it always has very few entries, per the note above).
#[derive(Default)]
pub struct ClipboardCodepointMap {
    pub list: Vec<Entry>,
}

impl ClipboardCodepointMap {
    // pub fn deinit(self: *ClipboardCodepointMap, alloc: Allocator) void {
    //     self.list.deinit(alloc);
    // }
    //
    // PORT NOTE: explicit `deinit` is `Drop` in Rust; the `Vec` frees itself. The
    // method is preserved as a no-op-shaped clear for call-site parity if any
    // downstream relies on the explicit free; here dropping the value suffices.
    pub fn deinit(&mut self) {
        self.list.clear();
    }

    /// Deep copy of the struct. The given allocator is expected to
    /// be an arena allocator of some sort since the struct itself
    /// doesn't support fine-grained deallocation of fields.
    // pub fn clone(self: *const ClipboardCodepointMap, alloc: Allocator) !ClipboardCodepointMap {
    //     var list = try self.list.clone(alloc);
    //     for (list.items(.replacement)) |*r| switch (r.*) {
    //         .string => |s| r.string = try alloc.dupe(u8, s),
    //         .codepoint => {}, // no allocation needed
    //     };
    //
    //     return .{ .list = list };
    // }
    //
    // PARITY: named `clone` to match the reference method (Config relies on it).
    #[allow(clippy::should_implement_trait)]
    pub fn clone(&self) -> ClipboardCodepointMap {
        // var list = try self.list.clone(alloc);
        // for (list.items(.replacement)) |*r| switch (r.*) {
        //     .string => |s| r.string = try alloc.dupe(u8, s),
        //     .codepoint => {}, // no allocation needed
        // };
        let list = self
            .list
            .iter()
            .map(|entry| Entry {
                range: entry.range,
                replacement: match &entry.replacement {
                    // .string => |s| r.string = try alloc.dupe(u8, s),
                    Replacement::String(s) => Replacement::String(s.clone()),
                    // .codepoint => {}, // no allocation needed
                    Replacement::Codepoint(cp) => Replacement::Codepoint(*cp),
                },
            })
            .collect();

        // return .{ .list = list };
        ClipboardCodepointMap { list }
    }

    /// Add an entry to the map.
    ///
    /// For conflicting codepoints, entries added later take priority over
    /// entries added earlier.
    // pub fn add(self: *ClipboardCodepointMap, alloc: Allocator, entry: Entry) !void {
    //     assert(entry.range[0] <= entry.range[1]);
    //     try self.list.append(alloc, entry);
    // }
    pub fn add(&mut self, entry: Entry) {
        // assert(entry.range[0] <= entry.range[1]);
        assert(entry.range[0] <= entry.range[1]);
        // try self.list.append(alloc, entry);
        self.list.push(entry);
    }
}
