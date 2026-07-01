//! This structure caches the shaped cells for a given text run.
//!
//! At one point, shaping was the most expensive part of rendering text
//! (accounting for 96% of frame time on my machine). To speed it up, this
//! was introduced so that shaping results can be cached depending on the
//! run.
//!
//! The cache key is the text run. The text run builds its own hash value
//! based on the font, style, codepoint, etc. This just utilizes the hash that
//! the text run provides.
//
// PORTS: ghostty `src/font/shaper/Cache.zig` (106 loc, 1 test).
// pub const Cache = @This();

// const std = @import("std");
// const Allocator = std.mem.Allocator;
// const font = @import("../main.zig");
// const CacheTable = @import("../../datastruct/main.zig").CacheTable;
use rustty_datastruct::cache_table::{CacheTable, Context};

use crate::shape::{Cell, TextRun};

// const log = std.log.scoped(.font_shaper_cache);
// PORT NOTE: no logger is wired in this crate (see `feature.rs`); the reference
// declares `log` but never uses it in this file.

/// An owning raw-slice handle for the cached `[]font.shape.Cell`.
///
// PORT NOTE: the reference stores a `[]font.shape.Cell` produced by `alloc.dupe`
// and frees it with `alloc.free`. `CacheTable`'s value type must be
// `Copy + Default`, so the owned slice is carried as a `Copy` fat raw pointer
// (`*mut [Cell]`) obtained from `Box::into_raw`. The heap slice is freed by
// reconstructing the `Box` — see `CellCacheTableContext::evicted`.
#[derive(Clone, Copy)]
struct CellsPtr(*mut [Cell]);

impl Default for CellsPtr {
    fn default() -> Self {
        CellsPtr(core::ptr::slice_from_raw_parts_mut(
            core::ptr::null_mut(),
            0,
        ))
    }
}

/// Context for cache table.
// const CellCacheTableContext = struct {
//     pub fn hash(self, key: u64) u64 { return key; }
//     pub fn eql(self, a: u64, b: u64) bool { return a == b; }
// };
//
// PORT NOTE: the reference context has only `hash`/`eql` and frees cached
// slices manually inside `Cache.clear`, reaching into `map.buckets` /
// `map.lengths`. In rustty those `CacheTable` fields are private, so freeing is
// routed through the `Context::evicted` hook (which the `CacheTable` explicitly
// supports: "the Context is allowed to register an eviction hook"). This is a
// behavior-equivalent structural adaptation — every evicted/cleared slice is
// freed exactly once, so results and leak behavior match the reference.
struct CellCacheTableContext;

impl Context<u64, CellsPtr> for CellCacheTableContext {
    fn hash(&self, key: u64) -> u64 {
        key
    }
    fn eql(&self, a: u64, b: u64) -> bool {
        a == b
    }
    fn evicted(&mut self, _key: u64, value: CellsPtr) {
        // Free the heap slice by reconstructing and dropping the `Box`.
        // (Default `CellsPtr` values are never stored as live entries.)
        unsafe {
            drop(Box::from_raw(value.0));
        }
    }
}

/// Cache table for run hash -> shaped cells.
// const CellCacheTable = CacheTable(
//     u64,
//     []font.shape.Cell,
//     CellCacheTableContext,
//
//     // Capacity is slightly arbitrary. These numbers are guesses.
//     //
//     // I'd expect then an average of 256 frequently cached runs is a
//     // safe guess most terminal screens.
//     256,
//     // 8 items per bucket to give decent resiliency to important runs.
//     8,
// );
type CellCacheTable = CacheTable<u64, CellsPtr, CellCacheTableContext, 256, 8>;

/// The cache table of shaped cells.
// map: CellCacheTable,
pub struct Cache {
    map: CellCacheTable,
}

impl Cache {
    // pub fn init() Cache {
    //     return .{ .map = .{ .context = .{} } };
    // }
    pub fn init() -> Cache {
        Cache {
            map: CacheTable::new(CellCacheTableContext),
        }
    }

    // pub fn deinit(self: *Cache, alloc: Allocator) void {
    //     self.clear(alloc);
    // }
    pub fn deinit(&mut self) {
        self.clear();
    }

    /// Get the shaped cells for the given text run,
    /// or null if they are not in the cache.
    // pub fn get(self: *Cache, run: font.shape.TextRun) ?[]const font.shape.Cell {
    //     return self.map.get(run.hash);
    // }
    pub fn get(&mut self, run: &TextRun) -> Option<&[Cell]> {
        // `CacheTable::get` returns the `Copy` handle by value; deref it to a
        // slice borrowing the cache for the duration.
        let cells = self.map.get(run.hash)?;
        Some(unsafe { &*cells.0 })
    }

    /// Insert the shaped cells for the given text run into the cache.
    ///
    /// The cells will be duplicated.
    // pub fn put(self, alloc, run, cells) Allocator.Error!void {
    //     const copy = try alloc.dupe(font.shape.Cell, cells);
    //     const evicted = self.map.put(run.hash, copy);
    //     if (evicted) |kv| { alloc.free(kv.value); }
    // }
    pub fn put(&mut self, run: &TextRun, cells: &[Cell]) {
        // const copy = try alloc.dupe(font.shape.Cell, cells);
        let copy = CellsPtr(Box::into_raw(cells.to_vec().into_boxed_slice()));
        // const evicted = self.map.put(run.hash, copy);
        // PORT NOTE: `map.put` already frees any evicted slice via the
        // `evicted` hook (see `CellCacheTableContext`), so unlike the reference
        // — which frees `kv.value` here — we drop the returned handle.
        let _evicted = self.map.put(run.hash, copy);
    }

    // fn clear(self: *Cache, alloc: Allocator) void {
    //     for (self.map.buckets, self.map.lengths) |b, l| {
    //         for (b[0..l]) |kv| { alloc.free(kv.value); }
    //     }
    //     self.map.clear();
    // }
    //
    // PORT NOTE: the reference frees each live slice by iterating the (now
    // private) `buckets`/`lengths` before `map.clear()`. Here `map.clear()`
    // itself invokes the `evicted` hook for every live entry, freeing each
    // slice exactly once.
    fn clear(&mut self) {
        self.map.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // test Cache {
    #[test]
    fn cache() {
        // const testing = std.testing;
        // const alloc = testing.allocator;

        // var c = Cache.init();
        // defer c.deinit(alloc);
        let mut c = Cache::init();

        // var run: font.shape.TextRun = undefined;
        // run.hash = 1;
        //
        // PORT NOTE: Rust has no `undefined`; the test only reads `run.hash`, so
        // the remaining fields get placeholder values (the `grid` pointer is
        // never dereferenced by the cache).
        let run = TextRun {
            hash: 1,
            offset: 0,
            cells: 0,
            grid: core::ptr::null_mut(),
            font_index: crate::collection::Index(0),
        };

        // try testing.expect(c.get(run) == null);
        assert!(c.get(&run).is_none());

        // try c.put(alloc, run, &.{
        //     .{ .x = 0, .glyph_index = 0 },
        //     .{ .x = 1, .glyph_index = 1 },
        // });
        c.put(
            &run,
            &[
                Cell {
                    x: 0,
                    x_offset: 0,
                    y_offset: 0,
                    glyph_index: 0,
                },
                Cell {
                    x: 1,
                    x_offset: 0,
                    y_offset: 0,
                    glyph_index: 1,
                },
            ],
        );

        // const actual = c.get(run).?;
        // try testing.expect(actual.len == 2);
        let actual = c.get(&run).unwrap();
        assert!(actual.len() == 2);

        c.deinit();
    }
}
