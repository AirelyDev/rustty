//! rustty-vt search/sliding_window
//!
//! PORTS: ghostty src/terminal/search/sliding_window.zig
//!
//! PORT STATUS: complete (vt-15). Faithful port of the search sliding-window
//! ring-buffer/window core. The reference `DataBuf = CircBuf(u8, 0)` maps to the
//! ported `rustty_datastruct::CircBuf<u8>`. The reference `MetaBuf =
//! CircBuf(Meta, undefined)` CANNOT use that ported `CircBuf` because `Meta`
//! owns an `ArrayList(point.Coordinate)` (a `Vec`) and is therefore not `Copy`
//! (the ported `CircBuf<T: Copy>` is bound to `Copy` for its `@memset`-on-delete
//! semantics). We therefore port a faithful, byte-for-byte-equivalent non-`Copy`
//! circular buffer `MetaBuf` below, mirroring `circ_buf.zig` exactly for the
//! subset of operations the sliding window uses (`init`/`deinit`/`iterator`/
//! `len`/`clear`/`ensureUnusedCapacity`/`appendAssumeCapacity`/`capacity`/
//! `deleteOldest`), including the `Iterator.idx` field the prune logic reads.

// const std = @import("std");
// const assert = @import("../../quirks.zig").inlineAssert;
// const Allocator = std.mem.Allocator;
// const CircBuf = @import("../../datastruct/main.zig").CircBuf;
// const terminal = @import("../main.zig");
// const point = terminal.point;
// const size = terminal.size;
// const PageList = terminal.PageList;
// const Pin = PageList.Pin;
// const Selection = terminal.Selection;
// const Screen = terminal.Screen;
// const Terminal = terminal.Terminal;
// const PageFormatter = @import("../formatter.zig").PageFormatter;
// const FlattenedHighlight = terminal.highlight.Flattened;
use crate::formatter::{Format, Options, PageFormatter, PointMap};
use crate::highlight::Flattened as FlattenedHighlight;
use crate::pagelist::Node;
use crate::point;
use crate::size;
use allocator_api2::alloc::Global as Allocator;
use rustty_datastruct::circ_buf::{Direction as CircDirection, Error as CircError};
use rustty_datastruct::CircBuf;

// PORT NOTE: the reference `Allocator.Error` is a single-variant error set
// (`OutOfMemory`). We mirror it with the crate-local OOM type used throughout
// the port.
use crate::pagelist::OutOfMemory;

// PORT NOTE: `DataBuf = CircBuf(u8, 0)` ports directly to the ported `CircBuf<u8>`.
type DataBuf = CircBuf<u8>;

/// Searches page nodes via a sliding window. The sliding window maintains
/// the invariant that data isn't pruned until (1) we've searched it and
/// (2) we've accounted for overlaps across pages to fit the needle.
///
/// The sliding window is first initialized empty. Pages are then appended
/// in the order to search them. The sliding window supports both a forward
/// and reverse order specified via `init`. The pages should be appended
/// in the correct order matching the search direction.
///
/// All appends grow the window. The window is only pruned when a search
/// is done (positive or negative match) via `next()`.
///
/// To avoid unnecessary memory growth, the recommended usage is to
/// call `next()` until it returns null and then `append` the next page
/// and repeat the process. This will always maintain the minimum
/// required memory to search for the needle.
///
/// The caller is responsible for providing the pages and ensuring they're
/// in the proper order. The SlidingWindow itself doesn't own the pages, but
/// it will contain pointers to them in order to return selections. If any
/// pages become invalid, the caller should clear the sliding window and
/// start over.
pub struct SlidingWindow {
    /// The allocator to use for all the data within this window. We
    /// store this rather than passing it around because its already
    /// part of multiple elements (eg. Meta's CellMap) and we want to
    /// ensure we always use a consistent allocator. Additionally, only
    /// a small amount of sliding windows are expected to be in use
    /// at any one time so the memory overhead isn't that large.
    // PORT NOTE: `Allocator` is the ZST `allocator_api2::Global`; the ported
    // collections (`Vec`, ported `CircBuf`) carry their own allocator, so this
    // field is kept for API parity but is a unit.
    // PORT NOTE: `pub(crate)` so the sibling search modules (active/pagelist/
    // screen/viewport) can read `window.alloc`, `window.needle`, and
    // `window.direction` as the reference does (`self.active.window.alloc`,
    // `self.window.needle`, `self.window.direction`).
    pub(crate) alloc: Allocator,

    /// The data buffer is a circular buffer of u8 that contains the
    /// encoded page text that we can use to search for the needle.
    data: DataBuf,

    /// The meta buffer is a circular buffer that contains the metadata
    /// about the pages we're searching. This usually isn't that large
    /// so callers must iterate through it to find the offset to map
    /// data to meta.
    meta: MetaBuf,

    /// Buffer that can fit any amount of chunks necessary for next
    /// to never fail allocation.
    // PORT NOTE: the reference `std.MultiArrayList(FlattenedHighlight.Chunk)`
    // ports to a `Vec<Chunk>` (the SoA layout is a Zig storage optimization, not
    // observable). The chunk type is the ported `highlight::Chunk`.
    chunk_buf: Vec<crate::highlight::Chunk>,

    /// Offset into data for our current state. This handles the
    /// situation where our search moved through meta[0] but didn't
    /// do enough to prune it.
    data_offset: usize,

    /// The needle we're searching for. Does own the memory.
    // PORT NOTE: `[]const u8` -> owned `Vec<u8>`. `pub(crate)` so sibling search
    // modules can read `window.needle` / `window.needle.len()`.
    pub(crate) needle: Vec<u8>,

    /// The search direction. If the direction is forward then pages should
    /// be appended in forward linked list order from the PageList. If the
    /// direction is reverse then pages should be appended in reverse order.
    ///
    /// This is important because in most cases, a reverse search is going
    /// to be more desirable to search from the end of the active area
    /// backwards so more recent data is found first. Supporting both is
    /// trivial though and will let us do more complex optimizations in the
    /// future (e.g. starting from the viewport and doing a forward/reverse
    /// concurrently from that point).
    // PORT NOTE: `pub(crate)` so sibling search modules can read
    // `window.direction` (used in their `needle()` assertions).
    pub(crate) direction: Direction,

    /// A buffer to store the overlap search data. This is used to search
    /// overlaps between pages where the match starts on one page and
    /// ends on another. The length is always `needle.len * 2`.
    overlap_buf: Vec<u8>,
}

// const Direction = enum { forward, reverse };
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Forward,
    Reverse,
}

// const Meta = struct { ... };
//
// PORT NOTE: the reference `Meta` owns a `cell_map: std.ArrayList(point.Coordinate)`.
// The ported `cell_map` is a `Vec<point::Coordinate>`. `Meta` is therefore NOT
// `Copy`, which is why `MetaBuf` is a bespoke non-`Copy` circular buffer below.
struct Meta {
    node: *mut Node,
    serial: u64,
    cell_map: Vec<point::Coordinate>,
}

impl Meta {
    // pub fn deinit(self: *Meta, alloc: Allocator) void {
    //     self.cell_map.deinit(alloc);
    // }
    fn deinit(&mut self, _alloc: Allocator) {
        // PORT NOTE: the reference frees `cell_map`; the Rust `Vec` frees on
        // drop, so clearing it (releasing its storage) mirrors the deinit.
        self.cell_map = Vec::new();
    }
}

impl SlidingWindow {
    // pub fn init(
    //     alloc: Allocator,
    //     direction: Direction,
    //     needle_unowned: []const u8,
    // ) Allocator.Error!SlidingWindow {
    pub fn init(
        alloc: Allocator,
        direction: Direction,
        needle_unowned: &[u8],
    ) -> Result<SlidingWindow, OutOfMemory> {
        // var data = try DataBuf.init(alloc, 0);
        // errdefer data.deinit(alloc);
        let data = DataBuf::init(0, 0).map_err(circ_to_oom)?;

        // var meta = try MetaBuf.init(alloc, 0);
        // errdefer meta.deinit(alloc);
        let meta = MetaBuf::init(alloc, 0).map_err(circ_to_oom)?;

        // const needle = try alloc.dupe(u8, needle_unowned);
        // errdefer alloc.free(needle);
        let mut needle: Vec<u8> = needle_unowned.to_vec();
        // switch (direction) {
        //     .forward => {},
        //     .reverse => std.mem.reverse(u8, needle),
        // }
        match direction {
            Direction::Forward => {}
            Direction::Reverse => needle.reverse(),
        }

        // const overlap_buf = try alloc.alloc(u8, needle.len * 2);
        // errdefer alloc.free(overlap_buf);
        let overlap_buf: Vec<u8> = vec![0u8; needle.len() * 2];

        Ok(SlidingWindow {
            alloc,
            data,
            meta,
            // .chunk_buf = .empty,
            chunk_buf: Vec::new(),
            data_offset: 0,
            needle,
            direction,
            overlap_buf,
        })
    }

    // pub fn deinit(self: *SlidingWindow) void {
    pub fn deinit(&mut self) {
        // self.alloc.free(self.overlap_buf);
        self.overlap_buf = Vec::new();
        // self.alloc.free(self.needle);
        self.needle = Vec::new();
        // self.chunk_buf.deinit(self.alloc);
        self.chunk_buf = Vec::new();
        // self.data.deinit(self.alloc);
        // PORT NOTE: the ported `CircBuf` frees on drop; nothing explicit needed.

        // var meta_it = self.meta.iterator(.forward);
        // while (meta_it.next()) |meta| meta.deinit(self.alloc);
        // self.meta.deinit(self.alloc);
        let alloc = self.alloc;
        let mut meta_it = self.meta.iterator(MetaDirection::Forward);
        while let Some(meta) = meta_it.next() {
            meta.deinit(alloc);
        }
        // The ported `MetaBuf` frees on drop.
    }

    /// Clear all data but retain allocated capacity.
    // pub fn clearAndRetainCapacity(self: *SlidingWindow) void {
    pub fn clear_and_retain_capacity(&mut self) {
        // var meta_it = self.meta.iterator(.forward);
        // while (meta_it.next()) |meta| meta.deinit(self.alloc);
        let alloc = self.alloc;
        let mut meta_it = self.meta.iterator(MetaDirection::Forward);
        while let Some(meta) = meta_it.next() {
            meta.deinit(alloc);
        }
        // self.meta.clear();
        self.meta.clear();
        // self.data.clear();
        self.data.clear();
        // self.data_offset = 0;
        self.data_offset = 0;
    }

    /// Search the window for the next occurrence of the needle. As
    /// the window moves, the window will prune itself while maintaining
    /// the invariant that the window is always big enough to contain
    /// the needle.
    ///
    /// This returns a flattened highlight on a match. The
    /// flattened highlight requires allocation and is therefore more expensive
    /// than a normal selection, but it is more efficient to render since it
    /// has all the information without having to dereference pointers into
    /// the terminal state.
    ///
    /// The flattened highlight chunks reference internal memory for this
    /// sliding window and are only valid until the next call to `next()`
    /// or `append()`. If the caller wants to retain the flattened highlight
    /// then they should clone it.
    // pub fn next(self: *SlidingWindow) ?FlattenedHighlight {
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<FlattenedHighlight> {
        // const slices = slices: {
        //     // If we have less data then the needle then we can't possibly match
        //     const data_len = self.data.len();
        //     if (data_len < self.needle.len) return null;
        //
        //     break :slices self.data.getPtrSlice(
        //         self.data_offset,
        //         data_len - self.data_offset,
        //     );
        // };
        let (slice0, slice1): (&[u8], &[u8]) = {
            // If we have less data then the needle then we can't possibly match
            let data_len = self.data.len();
            if data_len < self.needle.len() {
                return None;
            }

            let (s0, s1) = self
                .data
                .get_ptr_slice(self.data_offset, data_len - self.data_offset);
            // SAFETY: `get_ptr_slice` returns two disjoint, valid subslices of
            // the data buffer; we only read them here and the buffer is not
            // mutated for the lifetime of these borrows.
            unsafe { (&*s0, &*s1) }
        };

        // Search the first slice for the needle.
        // if (std.ascii.indexOfIgnoreCase(slices[0], self.needle)) |idx| {
        //     return self.highlight(idx, self.needle.len);
        // }
        if let Some(idx) = ascii_index_of_ignore_case(slice0, &self.needle) {
            return Some(self.highlight(idx, self.needle.len()));
        }

        // Search the overlap buffer for the needle.
        // if (slices[0].len > 0 and slices[1].len > 0) overlap: {
        if !slice0.is_empty() && !slice1.is_empty() {
            'overlap: {
                // Get up to needle.len - 1 bytes from each side (as much as
                // we can) and store it in the overlap buffer.
                // const prefix: []const u8 = prefix: {
                //     const len = @min(slices[0].len, self.needle.len - 1);
                //     const idx = slices[0].len - len;
                //     break :prefix slices[0][idx..];
                // };
                let prefix: &[u8] = {
                    let len = core::cmp::min(slice0.len(), self.needle.len() - 1);
                    let idx = slice0.len() - len;
                    &slice0[idx..]
                };
                // const suffix: []const u8 = suffix: {
                //     const len = @min(slices[1].len, self.needle.len - 1);
                //     break :suffix slices[1][0..len];
                // };
                let suffix: &[u8] = {
                    let len = core::cmp::min(slice1.len(), self.needle.len() - 1);
                    &slice1[0..len]
                };
                // const overlap_len = prefix.len + suffix.len;
                let overlap_len = prefix.len() + suffix.len();
                // assert(overlap_len <= self.overlap_buf.len);
                assert!(overlap_len <= self.overlap_buf.len());
                // @memcpy(self.overlap_buf[0..prefix.len], prefix);
                self.overlap_buf[0..prefix.len()].copy_from_slice(prefix);
                // @memcpy(self.overlap_buf[prefix.len..overlap_len], suffix);
                self.overlap_buf[prefix.len()..overlap_len].copy_from_slice(suffix);

                // Search the overlap
                // const idx = std.ascii.indexOfIgnoreCase(
                //     self.overlap_buf[0..overlap_len],
                //     self.needle,
                // ) orelse break :overlap;
                let idx = match ascii_index_of_ignore_case(
                    &self.overlap_buf[0..overlap_len],
                    &self.needle,
                ) {
                    Some(i) => i,
                    None => break 'overlap,
                };

                // We found a match in the overlap buffer. We need to map the
                // index back to the data buffer in order to get our selection.
                // return self.highlight(
                //     slices[0].len - prefix.len + idx,
                //     self.needle.len,
                // );
                return Some(self.highlight(slice0.len() - prefix.len() + idx, self.needle.len()));
            }
        }

        // Search the last slice for the needle.
        // if (std.ascii.indexOfIgnoreCase(slices[1], self.needle)) |idx| {
        //     return self.highlight(slices[0].len + idx, self.needle.len);
        // }
        if let Some(idx) = ascii_index_of_ignore_case(slice1, &self.needle) {
            return Some(self.highlight(slice0.len() + idx, self.needle.len()));
        }

        // Special case 1-lengthed needles to delete the entire buffer.
        // if (self.needle.len == 1) {
        //     self.clearAndRetainCapacity();
        //     self.assertIntegrity();
        //     return null;
        // }
        if self.needle.len() == 1 {
            self.clear_and_retain_capacity();
            self.assert_integrity();
            return None;
        }

        // No match. We keep `needle.len - 1` bytes available to
        // handle the future overlap case.
        // prune: {
        'prune: {
            // var meta_it = self.meta.iterator(.reverse);
            // var saved: usize = 0;
            //
            // PORT NOTE: this scan only reads `cell_map.len()`, so we use the
            // `&`-borrowing iterator; the reference's `*Meta` mutability is
            // unused here, and a const iterator lets us read `self.meta.len()`
            // (the `meta_it.idx`/`prune_count` math) without a borrow conflict.
            let meta_len_snapshot = self.meta.len();
            let mut meta_it = self.meta.iterator_const(MetaDirection::Reverse);
            let mut saved: usize = 0;
            // while (meta_it.next()) |meta| {
            let prune_count: usize;
            loop {
                let Some(meta) = meta_it.next() else {
                    // If we exited the while loop naturally then we
                    // never got the amount we needed and so there is
                    // nothing to prune.
                    // assert(saved < self.needle.len - 1);
                    // break :prune;
                    assert!(saved < self.needle.len() - 1);
                    break 'prune;
                };
                // const needed = self.needle.len - 1 - saved;
                let needed = self.needle.len() - 1 - saved;
                // if (meta.cell_map.items.len >= needed) {
                if meta.cell_map.len() >= needed {
                    // We save up to this meta. We set our data offset
                    // to exactly where it needs to be to continue
                    // searching.
                    // self.data_offset = meta.cell_map.items.len - needed;
                    // break;
                    self.data_offset = meta.cell_map.len() - needed;
                    // PORT NOTE: the Zig `while`-`else` distinguishes the natural
                    // (no-`break`) exit. On `break`, control continues just after
                    // the loop. We compute `prune_count` from the iterator index
                    // here, then proceed with the post-loop logic below.
                    prune_count = meta_len_snapshot - meta_it.idx;
                    break;
                }

                // saved += meta.cell_map.items.len;
                saved += meta.cell_map.len();
            }

            // const prune_count = self.meta.len() - meta_it.idx;
            // if (prune_count == 0) {
            //     // This can happen if we need to save up to the first
            //     // meta value to retain our window.
            //     break :prune;
            // }
            if prune_count == 0 {
                break 'prune;
            }

            // We can now delete all the metas up to but NOT including
            // the meta we found through meta_it.
            // meta_it = self.meta.iterator(.forward);
            // var prune_data_len: usize = 0;
            let mut prune_it = self.meta.iterator(MetaDirection::Forward);
            let mut prune_data_len: usize = 0;
            // for (0..prune_count) |_| {
            //     const meta = meta_it.next().?;
            //     prune_data_len += meta.cell_map.items.len;
            //     meta.deinit(self.alloc);
            // }
            let alloc = self.alloc;
            for _ in 0..prune_count {
                let meta = prune_it.next().unwrap();
                prune_data_len += meta.cell_map.len();
                meta.deinit(alloc);
            }
            // self.meta.deleteOldest(prune_count);
            self.meta.delete_oldest(prune_count);
            // self.data.deleteOldest(prune_data_len);
            self.data.delete_oldest(prune_data_len);
        }

        // Our data offset now moves to needle.len - 1 from the end so
        // that we can handle the overlap case.
        // self.data_offset = self.data.len() - self.needle.len + 1;
        self.data_offset = self.data.len() - self.needle.len() + 1;

        // self.assertIntegrity();
        self.assert_integrity();
        // return null;
        None
    }

    /// Return a flattened highlight for the given start and length.
    ///
    /// The flattened highlight can be used to render the highlight
    /// in the most efficient way because it doesn't require a terminal
    /// lock to access terminal data to compare whether some viewport
    /// matches the highlight (because it doesn't need to traverse
    /// the page nodes).
    ///
    /// The start index is assumed to be relative to the offset. i.e.
    /// index zero is actually at `self.data[self.data_offset]`. The
    /// selection will account for the offset.
    // fn highlight(
    //     self: *SlidingWindow,
    //     start_offset: usize,
    //     len: usize,
    // ) terminal.highlight.Flattened {
    fn highlight(&mut self, start_offset: usize, len: usize) -> FlattenedHighlight {
        // const start = start_offset + self.data_offset;
        let start = start_offset + self.data_offset;
        // const end = start + len - 1;
        let end = start + len - 1;
        // if (comptime std.debug.runtime_safety) {
        //     assert(start < self.data.len());
        //     assert(start + len <= self.data.len());
        // }
        if cfg!(debug_assertions) {
            assert!(start < self.data.len());
            assert!(start + len <= self.data.len());
        }

        // Clear our previous chunk buffer to store this result
        // self.chunk_buf.clearRetainingCapacity();
        self.chunk_buf.clear();
        // var result: terminal.highlight.Flattened = .empty;
        let mut result: FlattenedHighlight = FlattenedHighlight::empty();

        // Go through the meta nodes to find our start.
        // const tl: struct { ... } = tl: {
        // PORT NOTE: the reference anonymous `tl` struct is ported as the local
        // `TopLeft` struct below. `br` is `Option<BottomRight>`.
        let tl: TopLeft = 'tl: {
            // var meta_it = self.meta.iterator(.forward);
            // var meta_consumed: usize = 0;
            let mut meta_it = self.meta.iterator(MetaDirection::Forward);
            let mut meta_consumed: usize = 0;
            // while (meta_it.next()) |meta| {
            loop {
                let Some(meta) = meta_it.next() else {
                    // Precondition that the start index is within the data buffer.
                    // unreachable;
                    unreachable!();
                };
                // Always increment our consumed count so that our index
                // is right for the end search if we do it.
                // const prior_meta_consumed = meta_consumed;
                // meta_consumed += meta.cell_map.items.len;
                let prior_meta_consumed = meta_consumed;
                meta_consumed += meta.cell_map.len();

                // meta_i is the index we expect to find the match in the
                // cell map within this meta if it contains it.
                // const meta_i = start - prior_meta_consumed;
                let meta_i = start - prior_meta_consumed;

                // This meta doesn't contain the match. This means we
                // can also prune this set of data because we only look
                // forward.
                // if (meta_i >= meta.cell_map.items.len) continue;
                if meta_i >= meta.cell_map.len() {
                    continue;
                }

                // Now we look for the end. In MOST cases it is the same as
                // our starting chunk because highlights are usually small and
                // not on a boundary, so let's optimize for that.
                // const end_i = end - prior_meta_consumed;
                let end_i = end - prior_meta_consumed;
                // if (end_i < meta.cell_map.items.len) {
                if end_i < meta.cell_map.len() {
                    // @branchHint(.likely);

                    // The entire highlight is within this meta.
                    // const start_map = meta.cell_map.items[meta_i];
                    // const end_map = meta.cell_map.items[end_i];
                    let start_map = meta.cell_map[meta_i];
                    let end_map = meta.cell_map[end_i];
                    // result.top_x = start_map.x;
                    // result.bot_x = end_map.x;
                    result.top_x = start_map.x;
                    result.bot_x = end_map.x;
                    // self.chunk_buf.appendAssumeCapacity(.{
                    //     .node = meta.node,
                    //     .serial = meta.serial,
                    //     .start = @intCast(start_map.y),
                    //     .end = @intCast(end_map.y + 1),
                    // });
                    self.chunk_buf.push(crate::highlight::Chunk {
                        node: meta.node,
                        serial: meta.serial,
                        start: start_map.y as size::CellCountInt,
                        end: (end_map.y + 1) as size::CellCountInt,
                    });

                    // break :tl .{
                    //     .br = null,
                    //     .prune = .{
                    //         .meta = meta_it.idx - 1,
                    //         .data = prior_meta_consumed,
                    //     },
                    // };
                    break 'tl TopLeft {
                        br: None,
                        prune: Prune {
                            meta: meta_it.idx - 1,
                            data: prior_meta_consumed,
                        },
                    };
                } else {
                    // We found the meta that contains the start of the match
                    // only. Consume this entire node from our start offset.
                    // const map = meta.cell_map.items[meta_i];
                    let map = meta.cell_map[meta_i];
                    // result.top_x = map.x;
                    result.top_x = map.x;
                    // self.chunk_buf.appendAssumeCapacity(.{
                    //     .node = meta.node,
                    //     .serial = meta.serial,
                    //     .start = @intCast(map.y),
                    //     .end = meta.node.data.size.rows,
                    // });
                    let node_rows = unsafe { crate::pagelist::Node::rows(meta.node) };
                    self.chunk_buf.push(crate::highlight::Chunk {
                        node: meta.node,
                        serial: meta.serial,
                        start: map.y as size::CellCountInt,
                        end: node_rows,
                    });

                    // break :tl .{
                    //     .br = .{
                    //         .it = meta_it,
                    //         .consumed = meta_consumed,
                    //     },
                    //     .prune = .{
                    //         .meta = meta_it.idx - 1,
                    //         .data = prior_meta_consumed,
                    //     },
                    // };
                    break 'tl TopLeft {
                        br: Some(BottomRight {
                            it_idx: meta_it.clone_state(),
                            consumed: meta_consumed,
                        }),
                        prune: Prune {
                            meta: meta_it.idx - 1,
                            data: prior_meta_consumed,
                        },
                    };
                }
            }
        };

        // Search for our end.
        // if (tl.br) |br| {
        if let Some(br) = tl.br {
            // var meta_it = br.it;
            // var meta_consumed: usize = br.consumed;
            let mut meta_it = self.meta.iterator_at(MetaDirection::Forward, br.it_idx);
            let mut meta_consumed: usize = br.consumed;
            // while (meta_it.next()) |meta| {
            loop {
                let Some(meta) = meta_it.next() else {
                    // Precondition that the end index is within the data buffer.
                    // unreachable;
                    unreachable!();
                };
                // meta_i is the index we expect to find the match in the
                // cell map within this meta if it contains it.
                // const meta_i = end - meta_consumed;
                let meta_i = end - meta_consumed;
                // if (meta_i >= meta.cell_map.items.len) {
                if meta_i >= meta.cell_map.len() {
                    // This meta doesn't contain the match. We still add it
                    // to our results because we want the full flattened list.
                    // self.chunk_buf.appendAssumeCapacity(.{
                    //     .node = meta.node,
                    //     .serial = meta.serial,
                    //     .start = 0,
                    //     .end = meta.node.data.size.rows,
                    // });
                    let node_rows = unsafe { crate::pagelist::Node::rows(meta.node) };
                    self.chunk_buf.push(crate::highlight::Chunk {
                        node: meta.node,
                        serial: meta.serial,
                        start: 0,
                        end: node_rows,
                    });

                    // meta_consumed += meta.cell_map.items.len;
                    // continue;
                    meta_consumed += meta.cell_map.len();
                    continue;
                }

                // We found it
                // const map = meta.cell_map.items[meta_i];
                let map = meta.cell_map[meta_i];
                // result.bot_x = map.x;
                result.bot_x = map.x;
                // self.chunk_buf.appendAssumeCapacity(.{
                //     .node = meta.node,
                //     .serial = meta.serial,
                //     .start = 0,
                //     .end = @intCast(map.y + 1),
                // });
                self.chunk_buf.push(crate::highlight::Chunk {
                    node: meta.node,
                    serial: meta.serial,
                    start: 0,
                    end: (map.y + 1) as size::CellCountInt,
                });
                // break;
                break;
            }
        }

        // Our offset into the current meta block is the start index
        // minus the amount of data fully consumed. We then add one
        // to move one past the match so we don't repeat it.
        // self.data_offset = start - tl.prune.data + 1;
        self.data_offset = start - tl.prune.data + 1;

        // If we went beyond our initial meta node we can prune.
        // if (tl.prune.meta > 0) {
        if tl.prune.meta > 0 {
            // Deinit all our memory in the meta blocks prior to our
            // match.
            // var meta_it = self.meta.iterator(.forward);
            // var meta_consumed: usize = 0;
            let mut meta_it = self.meta.iterator(MetaDirection::Forward);
            let mut meta_consumed: usize = 0;
            // for (0..tl.prune.meta) |_| {
            //     const meta: *Meta = meta_it.next().?;
            //     meta_consumed += meta.cell_map.items.len;
            //     meta.deinit(self.alloc);
            // }
            let alloc = self.alloc;
            for _ in 0..tl.prune.meta {
                let meta = meta_it.next().unwrap();
                meta_consumed += meta.cell_map.len();
                meta.deinit(alloc);
            }
            let _ = meta_consumed;
            // if (comptime std.debug.runtime_safety) {
            //     assert(meta_it.idx == tl.prune.meta);
            //     assert(meta_it.next().?.node == self.chunk_buf.items(.node)[0]);
            // }
            if cfg!(debug_assertions) {
                assert!(meta_it.idx == tl.prune.meta);
                let next_node = meta_it.next().unwrap().node;
                assert!(core::ptr::eq(next_node, self.chunk_buf[0].node));
            }
            // self.meta.deleteOldest(tl.prune.meta);
            self.meta.delete_oldest(tl.prune.meta);

            // Delete all the data up to our current index.
            // assert(tl.prune.data > 0);
            assert!(tl.prune.data > 0);
            // self.data.deleteOldest(tl.prune.data);
            self.data.delete_oldest(tl.prune.data);
        }

        // switch (self.direction) {
        match self.direction {
            // .forward => {},
            Direction::Forward => {}
            // .reverse => {
            Direction::Reverse => {
                // const slice = self.chunk_buf.slice();
                // const nodes = slice.items(.node);
                // const starts = slice.items(.start);
                // const ends = slice.items(.end);
                //
                // PORT NOTE: the SoA projections become direct field accesses on
                // the `Vec<Chunk>`. We mutate fields in place.
                let chunks_len = self.chunk_buf.len();

                // if (self.chunk_buf.len > 1) {
                if chunks_len > 1 {
                    // Reverse all our chunks. This should be pretty obvious why.
                    // std.mem.reverse(*PageList.List.Node, nodes);
                    // std.mem.reverse(size.CellCountInt, starts);
                    // std.mem.reverse(size.CellCountInt, ends);
                    self.chunk_buf.reverse();

                    // Now normally with forward traversal with multiple pages,
                    // the suffix of the first page and the prefix of the last
                    // page are used.
                    //
                    // For a reverse traversal, this is inverted (since the
                    // pages are in reverse order we get the suffix of the last
                    // page and the prefix of the first page). So we need to
                    // invert this.
                    //
                    // We DON'T need to do this for any middle pages because
                    // they always use the full page.
                    //
                    // This is a fixup that makes our start/end match the
                    // same logic as the loops above if they were in forward
                    // order.
                    // assert(nodes.len >= 2);
                    assert!(chunks_len >= 2);
                    // starts[0] = ends[0] - 1;
                    self.chunk_buf[0].start = self.chunk_buf[0].end - 1;
                    // ends[0] = nodes[0].data.size.rows;
                    self.chunk_buf[0].end = unsafe { crate::pagelist::Node::rows(self.chunk_buf[0].node) };
                    // ends[nodes.len - 1] = starts[nodes.len - 1] + 1;
                    self.chunk_buf[chunks_len - 1].end = self.chunk_buf[chunks_len - 1].start + 1;
                    // starts[nodes.len - 1] = 0;
                    self.chunk_buf[chunks_len - 1].start = 0;
                } else {
                    // For a single chunk, the y values are in reverse order
                    // (start is the screen-end, end is the screen-start).
                    // Swap them to get proper top-to-bottom order.
                    // const start_y = starts[0];
                    let start_y = self.chunk_buf[0].start;
                    // starts[0] = ends[0] - 1;
                    self.chunk_buf[0].start = self.chunk_buf[0].end - 1;
                    // ends[0] = start_y + 1;
                    self.chunk_buf[0].end = start_y + 1;
                }

                // X values also need to be reversed since the top/bottom
                // are swapped for the nodes.
                // const top_x = result.top_x;
                // result.top_x = result.bot_x;
                // result.bot_x = top_x;
                // PORT NOTE: faithful translation of the reference's explicit
                // three-statement swap (clippy would prefer `mem::swap`, but we
                // keep the line-for-line shape).
                #[allow(clippy::manual_swap)]
                {
                    let top_x = result.top_x;
                    result.top_x = result.bot_x;
                    result.bot_x = top_x;
                }
            }
        }

        // Copy over our MultiArrayList so it points to the proper memory.
        // result.chunks = self.chunk_buf;
        //
        // PORT NOTE: the reference assigns `self.chunk_buf` (a value copy of the
        // MultiArrayList header) into `result.chunks`; the returned `Flattened`
        // then aliases the same backing storage as `self.chunk_buf` and is only
        // valid until the next `next()`/`append()` (which call
        // `clearRetainingCapacity`). We mirror that "valid until next call"
        // contract by cloning the chunk vec into the result; the originals stay
        // in `self.chunk_buf`. (`highlight::Chunk` is not `Copy`.)
        let mut chunks: Vec<crate::highlight::Chunk> = Vec::with_capacity(self.chunk_buf.len());
        for c in &self.chunk_buf {
            chunks.push(crate::highlight::Chunk {
                node: c.node,
                serial: c.serial,
                start: c.start,
                end: c.end,
            });
        }
        result.chunks = chunks;
        // return result;
        result
    }

    /// Add a new node to the sliding window. This will always grow
    /// the sliding window; data isn't pruned until it is consumed
    /// via a search (via next()).
    ///
    /// Returns the number of bytes of content added to the sliding window.
    /// The total bytes will be larger since this omits metadata, but it is
    /// an accurate measure of the text content size added.
    // pub fn append(
    //     self: *SlidingWindow,
    //     node: *PageList.List.Node,
    // ) Allocator.Error!usize {
    #[allow(clippy::not_unsafe_ptr_arg_deref)] // reference takes `node: *PageList.List.Node`
    pub fn append(&mut self, node: *mut Node) -> Result<usize, OutOfMemory> {
        // Initialize our metadata for the node.
        // var meta: Meta = .{
        //     .node = node,
        //     .serial = node.serial,
        //     .cell_map = .empty,
        // };
        // errdefer meta.deinit(self.alloc);
        let mut meta: Meta = Meta {
            node,
            serial: unsafe { (*node).serial },
            cell_map: Vec::new(),
        };

        // This is suboptimal but we need to encode the page once to
        // temporary memory, and then copy it into our circular buffer.
        // In the future, we should benchmark and see if we can encode
        // directly into the circular buffer.
        // var encoded: std.Io.Writer.Allocating = .init(self.alloc);
        // defer encoded.deinit();
        let mut encoded: Vec<u8> = Vec::new();

        // Encode the page into the buffer.
        // const formatter: PageFormatter = formatter: {
        //     var formatter: PageFormatter = .init(&meta.node.data, .{
        //         .emit = .plain,
        //         .unwrap = true,
        //     });
        //     formatter.point_map = .{
        //         .alloc = self.alloc,
        //         .map = &meta.cell_map,
        //     };
        //     break :formatter formatter;
        // };
        // formatter.format(&encoded.writer) catch {
        //     // writer uses anyerror but the only realistic error on
        //     // an ArrayList is out of memory.
        //     return error.OutOfMemory;
        // };
        {
            let page: *const crate::page::Page = unsafe { &(*crate::pagelist::Node::page(meta.node)) };
            let mut formatter: PageFormatter = PageFormatter::init(
                page,
                Options {
                    emit: Format::Plain,
                    unwrap: true,
                    ..Options::plain()
                },
            );
            formatter.point_map = Some(PointMap {
                alloc: self.alloc,
                map: &mut meta.cell_map,
            });
            formatter.format(&mut encoded).map_err(|_| OutOfMemory)?;
        }
        // assert(meta.cell_map.items.len == encoded.written().len);
        assert!(meta.cell_map.len() == encoded.len());

        // If the node we're adding isn't soft-wrapped, we add the
        // trailing newline.
        // const row = node.data.getRow(node.data.size.rows - 1);
        // if (!row.wrap) {
        //     encoded.writer.writeByte('\n') catch return error.OutOfMemory;
        //     try meta.cell_map.append(
        //         self.alloc,
        //         meta.cell_map.getLastOrNull() orelse .{
        //             .x = 0,
        //             .y = 0,
        //         },
        //     );
        // }
        let row = unsafe { (*crate::pagelist::Node::page(node)).get_row(crate::pagelist::Node::rows(node) as usize - 1) };
        if !unsafe { (*row).wrap() } {
            encoded.push(b'\n');
            let last = meta
                .cell_map
                .last()
                .copied()
                .unwrap_or(point::Coordinate { x: 0, y: 0 });
            meta.cell_map.push(last);
        }

        // If our written data is empty, then there is nothing to
        // add to our data set.
        // const written = encoded.written();
        // if (written.len == 0) {
        //     self.assertIntegrity();
        //     return 0;
        // }
        if encoded.is_empty() {
            // PORT NOTE: `meta` (and its `cell_map`) is dropped here, matching the
            // reference `errdefer meta.deinit`/scope-end behavior for the unused
            // metadata.
            self.assert_integrity();
            return Ok(0);
        }

        // Get our written data. If we're doing a reverse search then we
        // need to reverse all our encodings.
        // switch (self.direction) {
        //     .forward => {},
        //     .reverse => {
        //         std.mem.reverse(u8, written);
        //         std.mem.reverse(point.Coordinate, meta.cell_map.items);
        //     },
        // }
        match self.direction {
            Direction::Forward => {}
            Direction::Reverse => {
                encoded.reverse();
                meta.cell_map.reverse();
            }
        }

        // Ensure our buffers are big enough to store what we need.
        // try self.data.ensureUnusedCapacity(self.alloc, written.len);
        self.data
            .ensure_unused_capacity(encoded.len())
            .map_err(circ_to_oom)?;
        // try self.meta.ensureUnusedCapacity(self.alloc, 1);
        self.meta.ensure_unused_capacity(1).map_err(circ_to_oom)?;
        // try self.chunk_buf.ensureTotalCapacity(self.alloc, self.meta.capacity());
        self.chunk_buf.reserve(
            self.meta
                .capacity()
                .saturating_sub(self.chunk_buf.capacity()),
        );

        // Append our new node to the circular buffer.
        // self.data.appendSliceAssumeCapacity(written);
        self.data.append_slice_assume_capacity(&encoded);
        // self.meta.appendAssumeCapacity(meta);
        let written_len = encoded.len();
        self.meta.append_assume_capacity(meta);

        // self.assertIntegrity();
        self.assert_integrity();
        // return written.len;
        Ok(written_len)
    }

    /// Only for tests!
    // fn testChangeNeedle(self: *SlidingWindow, new: []const u8) void {
    #[cfg(test)]
    fn test_change_needle(&mut self, new: &[u8]) {
        // assert(new.len == self.needle.len);
        assert!(new.len() == self.needle.len());
        // self.alloc.free(self.needle);
        // self.needle = self.alloc.dupe(u8, new) catch unreachable;
        self.needle = new.to_vec();
    }

    // fn assertIntegrity(self: *const SlidingWindow) void {
    fn assert_integrity(&self) {
        // if (comptime !std.debug.runtime_safety) return;
        if !cfg!(debug_assertions) {
            return;
        }

        // We don't run integrity checks on Valgrind because its soooooo slow,
        // Valgrind is our integrity checker, and we run these during unit
        // tests (non-Valgrind) anyways so we're verifying anyways.
        // if (std.valgrind.runningOnValgrind() > 0) return;
        // PORT NOTE: no Valgrind detection in the Rust port; the checks always
        // run under test/debug.

        // Integrity check: verify our data matches our metadata exactly.
        // var meta_it = self.meta.iterator(.forward);
        // var data_len: usize = 0;
        // while (meta_it.next()) |m| data_len += m.cell_map.items.len;
        // assert(data_len == self.data.len());
        let mut meta_it = self.meta.iterator_const(MetaDirection::Forward);
        let mut data_len: usize = 0;
        while let Some(m) = meta_it.next() {
            data_len += m.cell_map.len();
        }
        assert!(data_len == self.data.len());

        // Integrity check: verify our data offset is within bounds.
        // assert(self.data.len() == 0 or self.data_offset < self.data.len());
        // PORT NOTE: must use `len() == 0` (NOT the ported `CircBuf::empty()`/
        // `is_empty()`): a zero-capacity `CircBuf` reports `full == true` (and
        // thus `empty() == false`) while still having `len() == 0`, so the two
        // are NOT equivalent here. Faithful to the reference `data.len() == 0`.
        #[allow(clippy::len_zero)]
        let data_len_is_zero = self.data.len() == 0;
        assert!(data_len_is_zero || self.data_offset < self.data.len());
    }
}

// PORT NOTE: the reference anonymous `tl:` block-expression struct, hoisted to a
// named type. `br` carries the iterator state to resume the end search; the
// reference stores the whole `MetaBuf.Iterator` value, we store its `idx`
// (`it_idx`) which is the only state the resume needs (the buffer is `self.meta`).
struct TopLeft {
    /// If non-null, we need to continue searching for the bottom-right.
    br: Option<BottomRight>,

    /// Data to prune, both are lengths.
    prune: Prune,
}

struct BottomRight {
    it_idx: usize,
    consumed: usize,
}

struct Prune {
    meta: usize,
    data: usize,
}

// PORT NOTE: `std.ascii.indexOfIgnoreCase` — find the first index in `haystack`
// where `needle` matches case-insensitively (ASCII). Returns null for an empty
// needle per the std contract used by the reference (the sliding window never
// passes an empty needle, but we mirror the semantics faithfully).
fn ascii_index_of_ignore_case(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    if needle.len() > haystack.len() {
        return None;
    }
    let end = haystack.len() - needle.len();
    let mut i = 0usize;
    while i <= end {
        if ascii_eql_ignore_case(&haystack[i..i + needle.len()], needle) {
            return Some(i);
        }
        i += 1;
    }
    None
}

// PORT NOTE: `std.ascii.eqlIgnoreCase` helper.
fn ascii_eql_ignore_case(a: &[u8], b: &[u8]) -> bool {
    a.eq_ignore_ascii_case(b)
}

// PORT NOTE: bridge the ported `CircBuf` error into the crate-local OOM type.
fn circ_to_oom(_e: CircError) -> OutOfMemory {
    OutOfMemory
}

// =====================================================================
// MetaBuf — faithful non-`Copy` port of `CircBuf(Meta, undefined)`.
//
// The ported `rustty_datastruct::CircBuf<T: Copy>` cannot hold `Meta` (owns a
// `Vec`). This is a 1:1 port of `circ_buf.zig` for the `Meta` element type,
// limited to the operations the sliding window uses. The `undefined` default is
// modeled by leaving deleted slots as their prior value (we never read deleted
// slots), so `deleteOldest` only advances head/tail (it does NOT `@memset` to a
// default the way the `u8` buffer does — the reference `undefined` default makes
// that memset a no-op observationally; for non-`Copy` we must not write a bogus
// `Meta`, so we `take` the slot's contents into a dropped temporary instead,
// preserving the "data is gone" invariant and freeing the `Vec`).
// =====================================================================

#[derive(Clone, Copy, PartialEq, Eq)]
enum MetaDirection {
    Forward,
    Reverse,
}

impl From<MetaDirection> for CircDirection {
    fn from(d: MetaDirection) -> CircDirection {
        match d {
            MetaDirection::Forward => CircDirection::Forward,
            MetaDirection::Reverse => CircDirection::Reverse,
        }
    }
}

struct MetaBuf {
    storage: Vec<Option<Meta>>,
    head: usize,
    tail: usize,
    full: bool,
}

impl MetaBuf {
    // pub fn init(alloc: Allocator, requested_size: usize) Allocator.Error!Self
    fn init(_alloc: Allocator, requested_size: usize) -> Result<MetaBuf, CircError> {
        // var buf = try alloc.alloc(T, requested_size);
        let mut storage: Vec<Option<Meta>> = Vec::with_capacity(requested_size);
        for _ in 0..requested_size {
            storage.push(None);
        }
        Ok(MetaBuf {
            storage,
            head: 0,
            tail: 0,
            full: false,
        })
    }

    // pub fn clear(self: *Self) void
    fn clear(&mut self) {
        // Drop all live entries (the reference resets head/tail/full; our slots
        // own `Meta`, so we release their `Vec`s here to match the data being
        // logically cleared).
        let cap = self.capacity();
        if cap > 0 {
            let len = self.len();
            for i in 0..len {
                let storage_idx = (self.tail + i) % cap;
                self.storage[storage_idx] = None;
            }
        }
        self.head = 0;
        self.tail = 0;
        self.full = false;
    }

    // pub fn iterator(self: *Self, direction: Iterator.Direction) Iterator
    fn iterator(&mut self, direction: MetaDirection) -> MetaIterMut<'_> {
        MetaIterMut {
            buf: self,
            idx: 0,
            direction,
        }
    }

    fn iterator_const(&self, direction: MetaDirection) -> MetaIterRef<'_> {
        MetaIterRef {
            buf: self,
            idx: 0,
            direction,
        }
    }

    // PORT NOTE: resume a forward iterator at a given idx (mirrors the reference
    // copying the `MetaBuf.Iterator` value and continuing `next()` on it).
    fn iterator_at(&mut self, direction: MetaDirection, idx: usize) -> MetaIterMut<'_> {
        MetaIterMut {
            buf: self,
            idx,
            direction,
        }
    }

    // pub fn ensureUnusedCapacity(self: *Self, alloc: Allocator, n: usize) Allocator.Error!void
    fn ensure_unused_capacity(&mut self, amount: usize) -> Result<(), CircError> {
        let new_cap = self.len() + amount;
        if new_cap <= self.capacity() {
            return Ok(());
        }
        self.resize(new_cap)
    }

    // pub fn resize(self: *Self, alloc: Allocator, size: usize) Allocator.Error!void
    fn resize(&mut self, size: usize) -> Result<(), CircError> {
        // Rotate to zero so it is aligned.
        // try self.rotateToZero();
        self.rotate_to_zero();

        // Reallocate, this adds to the end so we're ready to go.
        // const prev_len = self.len();
        // const prev_cap = self.storage.len;
        // self.storage = try alloc.realloc(self.storage, size);
        let prev_len = self.len();
        let prev_cap = self.storage.len();
        // Grow (or shrink) storage to `size`. The sliding window only ever
        // grows the meta buffer, but we mirror the realloc-to-`size` shape.
        if size > prev_cap {
            self.storage.reserve(size - prev_cap);
            while self.storage.len() < size {
                self.storage.push(None);
            }
        } else {
            self.storage.truncate(size);
        }

        // If we grew, we need to set our new defaults. We can add it
        // at the end since we rotated to start.
        // if (size > prev_cap) {
        if size > prev_cap {
            // @memset(self.storage[prev_cap..], default);
            // (the appended `None` slots above are the "default".)

            // Fix up our head/tail
            // if (self.full) {
            //     self.head = prev_len;
            //     self.full = false;
            // }
            if self.full {
                self.head = prev_len;
                self.full = false;
            }
        }

        Ok(())
    }

    // PORT NOTE: rotate the live entries so they begin at storage index 0,
    // mirroring `circ_buf.zig`'s `rotateToZero` used inside `resize`.
    // fn rotateToZero(self: *Self) Allocator.Error!void
    fn rotate_to_zero(&mut self) {
        // If we're already at zero then do nothing.
        // if (self.tail == 0) return;
        if self.tail == 0 {
            return;
        }

        // We use std.mem.rotate to rotate our storage in-place.
        // std.mem.rotate(T, self.storage, self.tail);
        self.storage.rotate_left(self.tail);

        // Then fix up our head and tail.
        // self.head = self.len() % self.storage.len;
        // self.tail = 0;
        self.head = self.len() % self.storage.len();
        self.tail = 0;
    }

    // pub fn capacity(self: Self) usize
    fn capacity(&self) -> usize {
        self.storage.len()
    }

    // pub fn len(self: Self) usize
    fn len(&self) -> usize {
        if self.full {
            return self.storage.len();
        }
        if self.tail <= self.head {
            return self.head - self.tail;
        }
        self.storage.len() - (self.tail - self.head)
    }

    // pub fn appendAssumeCapacity(self: *Self, v: T) void
    fn append_assume_capacity(&mut self, v: Meta) {
        // assert(!self.full);
        assert!(!self.full);
        // self.storage[self.head] = v;
        self.storage[self.head] = Some(v);
        // self.head += 1; if (self.head >= self.storage.len) self.head -= self.storage.len;
        self.head += 1;
        if self.head >= self.storage.len() {
            self.head -= self.storage.len();
        }
        // self.full = self.head == self.tail;
        self.full = self.head == self.tail;
    }

    // pub fn deleteOldest(self: *Self, n: usize) void
    fn delete_oldest(&mut self, n: usize) {
        // assert(n <= self.storage.len);
        assert!(n <= self.storage.len());

        // Special case n == 0 otherwise we will accidentally break
        // our circular buffer.
        // if (n == 0) return;
        if n == 0 {
            return;
        }

        // Clear the values back to default. For non-`Copy` `Meta` we `take`
        // each deleted slot (dropping its `Vec`) instead of `@memset(default)`
        // (the reference `undefined` default makes the memset observationally a
        // no-op; we must not write a bogus `Meta`).
        let cap = self.storage.len();
        let clear_n = core::cmp::min(self.len(), n);
        for i in 0..clear_n {
            let storage_idx = (self.tail + i) % cap;
            self.storage[storage_idx] = None;
        }

        // If we're not full, we can just advance the tail. We know
        // it'll be less than the length because otherwise we'd be full.
        // self.tail += @min(self.len(), n);
        self.tail += core::cmp::min(self.len(), n);
        // if (self.tail >= self.storage.len) self.tail -= self.storage.len;
        if self.tail >= self.storage.len() {
            self.tail -= self.storage.len();
        }
        // self.full = false;
        self.full = false;
    }

    // PORT NOTE: shared index translation used by both iterators (mirrors the
    // `Iterator.next` tail->storage index math).
    fn storage_idx(&self, idx: usize, direction: MetaDirection) -> Option<usize> {
        if idx >= self.len() {
            return None;
        }
        let tail_idx = match direction {
            MetaDirection::Forward => idx,
            MetaDirection::Reverse => self.len() - idx - 1,
        };
        Some((self.tail + tail_idx) % self.capacity())
    }
}

// pub const Iterator = struct { buf: Self, idx: usize, direction: Direction };
//
// PORT NOTE: split into a `&mut`-borrowing and `&`-borrowing iterator because
// the reference `next()` returns `?*T` (mutable) but `assertIntegrity` only
// reads. The `idx` field is `pub` (read in the prune logic via `meta_it.idx`).
struct MetaIterMut<'a> {
    buf: &'a mut MetaBuf,
    idx: usize,
    direction: MetaDirection,
}

impl<'a> MetaIterMut<'a> {
    // pub fn next(self: *Iterator) ?*T
    #[allow(clippy::should_implement_trait)]
    fn next(&mut self) -> Option<&mut Meta> {
        let storage_idx = self.buf.storage_idx(self.idx, self.direction)?;
        // self.idx += 1;
        self.idx += 1;
        // return &self.buf.storage[storage_idx];
        // SAFETY: `storage_idx` is a valid in-bounds index and the slot is
        // `Some` (it lies within `[tail, head)`); we hand out a `&mut Meta` to a
        // live entry, mirroring the reference `*T`.
        self.buf.storage[storage_idx].as_mut()
    }

    // PORT NOTE: the reference copies the `Iterator` value (`break :tl .{ .it =
    // meta_it }`). We snapshot the resumable state (`idx`) instead.
    fn clone_state(&self) -> usize {
        self.idx
    }
}

struct MetaIterRef<'a> {
    buf: &'a MetaBuf,
    idx: usize,
    direction: MetaDirection,
}

impl<'a> MetaIterRef<'a> {
    #[allow(clippy::should_implement_trait)]
    fn next(&mut self) -> Option<&Meta> {
        let storage_idx = self.buf.storage_idx(self.idx, self.direction)?;
        self.idx += 1;
        self.buf.storage[storage_idx].as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::{DynamicPalette, DynamicRGB};
    use crate::modes as modespkg;
    use crate::point::Point;
    use crate::screen::{Options as ScreenOptions, Screen};
    use crate::terminal::{Colors, Options as TerminalOptions, Terminal};

    // PORT NOTE: the reference `Terminal.init(alloc, .{ .cols, .rows })` relies
    // on Zig struct-literal defaults for the remaining `Options` fields. Rust has
    // no such defaults, so this helper supplies them explicitly (mirroring the
    // `terminal.rs` test `opts` helper) for the soft-wrapped Terminal tests.
    fn terminal_opts(cols: u16, rows: u16) -> TerminalOptions {
        TerminalOptions {
            cols,
            rows,
            max_scrollback: 10_000,
            colors: Colors {
                background: DynamicRGB::UNSET,
                foreground: DynamicRGB::UNSET,
                cursor: DynamicRGB::UNSET,
                palette: DynamicPalette::default_palette(),
            },
            default_modes: modespkg::ModePacked::default(),
            kitty_image_storage_limit: 320 * 1000 * 1000,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        }
    }
    use crate::kitty;

    // PORT NOTE: helper to build a `Screen` with the (cols, rows, max_scrollback)
    // the reference tests pass — the remaining `Options` fields take defaults.
    fn screen_init(cols: u16, rows: u16, max_scrollback: usize) -> Screen {
        Screen::init(
            Allocator,
            ScreenOptions {
                cols,
                rows,
                max_scrollback,
                ..ScreenOptions::default()
            },
        )
        .unwrap()
    }

    // test "SlidingWindow empty on init"
    #[test]
    fn sliding_window_empty_on_init() {
        let alloc = Allocator;

        let mut w: SlidingWindow = SlidingWindow::init(alloc, Direction::Forward, b"boo!").unwrap();
        // defer w.deinit();

        // try testing.expectEqual(0, w.data.len());
        assert_eq!(0, w.data.len());
        // try testing.expectEqual(0, w.meta.len());
        assert_eq!(0, w.meta.len());

        w.deinit();
    }

    // test "SlidingWindow single append"
    #[test]
    fn sliding_window_single_append() {
        let alloc = Allocator;

        let mut w: SlidingWindow = SlidingWindow::init(alloc, Direction::Forward, b"boo!").unwrap();

        let mut s = screen_init(80, 24, 0);
        s.test_write_string(b"hello. boo! hello. boo!").unwrap();

        // We want to test single-page cases.
        assert!(core::ptr::eq(s.pages.pages.first, s.pages.pages.last));
        let node: *mut Node = s.pages.pages.first;
        let _ = w.append(node).unwrap();

        // We should be able to find two matches.
        {
            let h = w.next().unwrap();
            let sel = h.untracked();
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 7, y: 0 })),
                s.pages.point_from_pin(point::Tag::Active, sel.start)
            );
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 10, y: 0 })),
                s.pages.point_from_pin(point::Tag::Active, sel.end)
            );
        }
        {
            let h = w.next().unwrap();
            let sel = h.untracked();
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 19, y: 0 })),
                s.pages.point_from_pin(point::Tag::Active, sel.start)
            );
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 22, y: 0 })),
                s.pages.point_from_pin(point::Tag::Active, sel.end)
            );
        }
        assert!(w.next().is_none());
        assert!(w.next().is_none());

        w.deinit();
        s.deinit();
    }

    // test "SlidingWindow single append case insensitive ASCII"
    #[test]
    fn sliding_window_single_append_case_insensitive_ascii() {
        let alloc = Allocator;

        let mut w: SlidingWindow = SlidingWindow::init(alloc, Direction::Forward, b"Boo!").unwrap();

        let mut s = screen_init(80, 24, 0);
        s.test_write_string(b"hello. boo! hello. boo!").unwrap();

        assert!(core::ptr::eq(s.pages.pages.first, s.pages.pages.last));
        let node: *mut Node = s.pages.pages.first;
        let _ = w.append(node).unwrap();

        {
            let h = w.next().unwrap();
            let sel = h.untracked();
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 7, y: 0 })),
                s.pages.point_from_pin(point::Tag::Active, sel.start)
            );
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 10, y: 0 })),
                s.pages.point_from_pin(point::Tag::Active, sel.end)
            );
        }
        {
            let h = w.next().unwrap();
            let sel = h.untracked();
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 19, y: 0 })),
                s.pages.point_from_pin(point::Tag::Active, sel.start)
            );
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 22, y: 0 })),
                s.pages.point_from_pin(point::Tag::Active, sel.end)
            );
        }
        assert!(w.next().is_none());
        assert!(w.next().is_none());

        w.deinit();
        s.deinit();
    }

    // test "SlidingWindow single append single char"
    #[test]
    fn sliding_window_single_append_single_char() {
        let alloc = Allocator;

        let mut w: SlidingWindow = SlidingWindow::init(alloc, Direction::Forward, b"b").unwrap();

        let mut s = screen_init(80, 24, 0);
        s.test_write_string(b"hello. boo! hello. boo!").unwrap();

        assert!(core::ptr::eq(s.pages.pages.first, s.pages.pages.last));
        let node: *mut Node = s.pages.pages.first;
        let _ = w.append(node).unwrap();

        {
            let h = w.next().unwrap();
            let sel = h.untracked();
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 7, y: 0 })),
                s.pages.point_from_pin(point::Tag::Active, sel.start)
            );
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 7, y: 0 })),
                s.pages.point_from_pin(point::Tag::Active, sel.end)
            );
        }
        {
            let h = w.next().unwrap();
            let sel = h.untracked();
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 19, y: 0 })),
                s.pages.point_from_pin(point::Tag::Active, sel.start)
            );
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 19, y: 0 })),
                s.pages.point_from_pin(point::Tag::Active, sel.end)
            );
        }
        assert!(w.next().is_none());
        assert!(w.next().is_none());

        w.deinit();
        s.deinit();
    }

    // test "SlidingWindow single append no match"
    #[test]
    fn sliding_window_single_append_no_match() {
        let alloc = Allocator;

        let mut w: SlidingWindow =
            SlidingWindow::init(alloc, Direction::Forward, b"nope!").unwrap();

        let mut s = screen_init(80, 24, 0);
        s.test_write_string(b"hello. boo! hello. boo!").unwrap();

        assert!(core::ptr::eq(s.pages.pages.first, s.pages.pages.last));
        let node: *mut Node = s.pages.pages.first;
        let _ = w.append(node).unwrap();

        // No matches
        assert!(w.next().is_none());
        assert!(w.next().is_none());

        // Should still keep the page
        assert_eq!(1, w.meta.len());

        w.deinit();
        s.deinit();
    }

    // test "SlidingWindow two pages"
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (vt multi-page-PageList intractable, .port/phase-2/MIRI_EXCEPTIONS.md): builds/reflows multi-page PageLists that exceed Miri's interpreter budget (>20min); covered natively + by ASan (all PageList tests ~18s)"
    )]
    #[test]
    fn sliding_window_two_pages() {
        let alloc = Allocator;

        let mut w: SlidingWindow = SlidingWindow::init(alloc, Direction::Forward, b"boo!").unwrap();

        let mut s = screen_init(80, 24, 1000);

        // Fill up the first page. The final bytes in the first page
        // are "boo!"
        let first_page_rows = unsafe { crate::pagelist::Node::capacity(s.pages.pages.first).rows };
        for _ in 0..first_page_rows - 1 {
            s.test_write_string(b"\n").unwrap();
        }
        for _ in 0..s.pages.cols - 4 {
            s.test_write_string(b"x").unwrap();
        }
        s.test_write_string(b"boo!").unwrap();
        assert!(core::ptr::eq(s.pages.pages.first, s.pages.pages.last));
        s.test_write_string(b"\n").unwrap();
        assert!(!core::ptr::eq(s.pages.pages.first, s.pages.pages.last));
        s.test_write_string(b"hello. boo!").unwrap();

        // Add both pages
        let node: *mut Node = s.pages.pages.first;
        let _ = w.append(node).unwrap();
        let _ = w.append(unsafe { (*node).next }).unwrap();

        // Search should find two matches
        {
            let h = w.next().unwrap();
            let sel = h.untracked();
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 76, y: 22 })),
                s.pages.point_from_pin(point::Tag::Active, sel.start)
            );
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 79, y: 22 })),
                s.pages.point_from_pin(point::Tag::Active, sel.end)
            );
        }
        {
            let h = w.next().unwrap();
            let sel = h.untracked();
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 7, y: 23 })),
                s.pages.point_from_pin(point::Tag::Active, sel.start)
            );
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 10, y: 23 })),
                s.pages.point_from_pin(point::Tag::Active, sel.end)
            );
        }
        assert!(w.next().is_none());
        assert!(w.next().is_none());

        w.deinit();
        s.deinit();
    }

    // test "SlidingWindow two pages single char"
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (vt multi-page-PageList intractable, .port/phase-2/MIRI_EXCEPTIONS.md): builds/reflows multi-page PageLists that exceed Miri's interpreter budget (>20min); covered natively + by ASan (all PageList tests ~18s)"
    )]
    #[test]
    fn sliding_window_two_pages_single_char() {
        let alloc = Allocator;

        let mut w: SlidingWindow = SlidingWindow::init(alloc, Direction::Forward, b"b").unwrap();

        let mut s = screen_init(80, 24, 1000);

        let first_page_rows = unsafe { crate::pagelist::Node::capacity(s.pages.pages.first).rows };
        for _ in 0..first_page_rows - 1 {
            s.test_write_string(b"\n").unwrap();
        }
        for _ in 0..s.pages.cols - 4 {
            s.test_write_string(b"x").unwrap();
        }
        s.test_write_string(b"boo!").unwrap();
        assert!(core::ptr::eq(s.pages.pages.first, s.pages.pages.last));
        s.test_write_string(b"\n").unwrap();
        assert!(!core::ptr::eq(s.pages.pages.first, s.pages.pages.last));
        s.test_write_string(b"hello. boo!").unwrap();

        let node: *mut Node = s.pages.pages.first;
        let _ = w.append(node).unwrap();
        let _ = w.append(unsafe { (*node).next }).unwrap();

        {
            let h = w.next().unwrap();
            let sel = h.untracked();
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 76, y: 22 })),
                s.pages.point_from_pin(point::Tag::Active, sel.start)
            );
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 76, y: 22 })),
                s.pages.point_from_pin(point::Tag::Active, sel.end)
            );
        }
        {
            let h = w.next().unwrap();
            let sel = h.untracked();
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 7, y: 23 })),
                s.pages.point_from_pin(point::Tag::Active, sel.start)
            );
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 7, y: 23 })),
                s.pages.point_from_pin(point::Tag::Active, sel.end)
            );
        }
        assert!(w.next().is_none());
        assert!(w.next().is_none());

        w.deinit();
        s.deinit();
    }

    // test "SlidingWindow two pages match across boundary"
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (vt multi-page-PageList intractable, .port/phase-2/MIRI_EXCEPTIONS.md): builds/reflows multi-page PageLists that exceed Miri's interpreter budget (>20min); covered natively + by ASan (all PageList tests ~18s)"
    )]
    #[test]
    fn sliding_window_two_pages_match_across_boundary() {
        let alloc = Allocator;

        let mut w: SlidingWindow =
            SlidingWindow::init(alloc, Direction::Forward, b"hello, world").unwrap();

        let mut s = screen_init(80, 24, 1000);

        let first_page_rows = unsafe { crate::pagelist::Node::capacity(s.pages.pages.first).rows };
        for _ in 0..first_page_rows - 1 {
            s.test_write_string(b"\n").unwrap();
        }
        for _ in 0..s.pages.cols - 4 {
            s.test_write_string(b"x").unwrap();
        }
        s.test_write_string(b"hell").unwrap();
        assert!(core::ptr::eq(s.pages.pages.first, s.pages.pages.last));
        s.test_write_string(b"o, world!").unwrap();
        assert!(!core::ptr::eq(s.pages.pages.first, s.pages.pages.last));

        let node: *mut Node = s.pages.pages.first;
        let _ = w.append(node).unwrap();
        let _ = w.append(unsafe { (*node).next }).unwrap();

        // Search should find a match
        {
            let h = w.next().unwrap();
            let sel = h.untracked();
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 76, y: 22 })),
                s.pages.point_from_pin(point::Tag::Active, sel.start)
            );
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 7, y: 23 })),
                s.pages.point_from_pin(point::Tag::Active, sel.end)
            );
        }
        assert!(w.next().is_none());
        assert!(w.next().is_none());

        // We shouldn't prune because we don't have enough space
        assert_eq!(2, w.meta.len());

        w.deinit();
        s.deinit();
    }

    // test "SlidingWindow two pages no match across boundary with newline"
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (vt multi-page-PageList intractable, .port/phase-2/MIRI_EXCEPTIONS.md): builds/reflows multi-page PageLists that exceed Miri's interpreter budget (>20min); covered natively + by ASan (all PageList tests ~18s)"
    )]
    #[test]
    fn sliding_window_two_pages_no_match_across_boundary_with_newline() {
        let alloc = Allocator;

        let mut w: SlidingWindow =
            SlidingWindow::init(alloc, Direction::Forward, b"hello, world").unwrap();

        let mut s = screen_init(80, 24, 1000);

        let first_page_rows = unsafe { crate::pagelist::Node::capacity(s.pages.pages.first).rows };
        for _ in 0..first_page_rows - 1 {
            s.test_write_string(b"\n").unwrap();
        }
        for _ in 0..s.pages.cols - 4 {
            s.test_write_string(b"x").unwrap();
        }
        s.test_write_string(b"hell").unwrap();
        assert!(core::ptr::eq(s.pages.pages.first, s.pages.pages.last));
        s.test_write_string(b"\no, world!").unwrap();
        assert!(!core::ptr::eq(s.pages.pages.first, s.pages.pages.last));

        let node: *mut Node = s.pages.pages.first;
        let _ = w.append(node).unwrap();
        let _ = w.append(unsafe { (*node).next }).unwrap();

        // Search should NOT find a match
        assert!(w.next().is_none());
        assert!(w.next().is_none());

        // We shouldn't prune because we don't have enough space
        assert_eq!(2, w.meta.len());

        w.deinit();
        s.deinit();
    }

    // test "SlidingWindow two pages no match across boundary with newline reverse"
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (vt multi-page-PageList intractable, .port/phase-2/MIRI_EXCEPTIONS.md): builds/reflows multi-page PageLists that exceed Miri's interpreter budget (>20min); covered natively + by ASan (all PageList tests ~18s)"
    )]
    #[test]
    fn sliding_window_two_pages_no_match_across_boundary_with_newline_reverse() {
        let alloc = Allocator;

        let mut w: SlidingWindow =
            SlidingWindow::init(alloc, Direction::Reverse, b"hello, world").unwrap();

        let mut s = screen_init(80, 24, 1000);

        let first_page_rows = unsafe { crate::pagelist::Node::capacity(s.pages.pages.first).rows };
        for _ in 0..first_page_rows - 1 {
            s.test_write_string(b"\n").unwrap();
        }
        for _ in 0..s.pages.cols - 4 {
            s.test_write_string(b"x").unwrap();
        }
        s.test_write_string(b"hell").unwrap();
        assert!(core::ptr::eq(s.pages.pages.first, s.pages.pages.last));
        s.test_write_string(b"\no, world!").unwrap();
        assert!(!core::ptr::eq(s.pages.pages.first, s.pages.pages.last));

        // Add both pages in reverse order
        let node: *mut Node = s.pages.pages.first;
        let _ = w.append(unsafe { (*node).next }).unwrap();
        let _ = w.append(node).unwrap();

        // Search should NOT find a match
        assert!(w.next().is_none());
        assert!(w.next().is_none());

        w.deinit();
        s.deinit();
    }

    // test "SlidingWindow two pages no match prunes first page"
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (vt multi-page-PageList intractable, .port/phase-2/MIRI_EXCEPTIONS.md): builds/reflows multi-page PageLists that exceed Miri's interpreter budget (>20min); covered natively + by ASan (all PageList tests ~18s)"
    )]
    #[test]
    fn sliding_window_two_pages_no_match_prunes_first_page() {
        let alloc = Allocator;

        let mut w: SlidingWindow =
            SlidingWindow::init(alloc, Direction::Forward, b"nope!").unwrap();

        let mut s = screen_init(80, 24, 1000);

        let first_page_rows = unsafe { crate::pagelist::Node::capacity(s.pages.pages.first).rows };
        for _ in 0..first_page_rows - 1 {
            s.test_write_string(b"\n").unwrap();
        }
        for _ in 0..s.pages.cols - 4 {
            s.test_write_string(b"x").unwrap();
        }
        s.test_write_string(b"boo!").unwrap();
        assert!(core::ptr::eq(s.pages.pages.first, s.pages.pages.last));
        s.test_write_string(b"\n").unwrap();
        assert!(!core::ptr::eq(s.pages.pages.first, s.pages.pages.last));
        s.test_write_string(b"hello. boo!").unwrap();

        let node: *mut Node = s.pages.pages.first;
        let _ = w.append(node).unwrap();
        let _ = w.append(unsafe { (*node).next }).unwrap();

        // Search should find nothing
        assert!(w.next().is_none());
        assert!(w.next().is_none());

        // We should've pruned our page because the second page
        // has enough text to contain our needle.
        assert_eq!(1, w.meta.len());

        w.deinit();
        s.deinit();
    }

    // test "SlidingWindow two pages no match keeps both pages"
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (vt multi-page-PageList intractable, .port/phase-2/MIRI_EXCEPTIONS.md): builds/reflows multi-page PageLists that exceed Miri's interpreter budget (>20min); covered natively + by ASan (all PageList tests ~18s)"
    )]
    #[test]
    fn sliding_window_two_pages_no_match_keeps_both_pages() {
        let alloc = Allocator;

        let mut s = screen_init(80, 24, 1000);

        let first_page_rows = unsafe { crate::pagelist::Node::capacity(s.pages.pages.first).rows };
        for _ in 0..first_page_rows - 1 {
            s.test_write_string(b"\n").unwrap();
        }
        for _ in 0..s.pages.cols - 4 {
            s.test_write_string(b"x").unwrap();
        }
        s.test_write_string(b"boo!").unwrap();
        assert!(core::ptr::eq(s.pages.pages.first, s.pages.pages.last));
        s.test_write_string(b"\n").unwrap();
        assert!(!core::ptr::eq(s.pages.pages.first, s.pages.pages.last));
        s.test_write_string(b"hello. boo!").unwrap();

        // Imaginary needle for search. Doesn't match!
        let mut needle_list: Vec<u8> = Vec::new();
        needle_list.resize((first_page_rows as usize) * (s.pages.cols as usize), b'x');
        let needle: &[u8] = &needle_list;

        let mut w: SlidingWindow = SlidingWindow::init(alloc, Direction::Forward, needle).unwrap();

        let node: *mut Node = s.pages.pages.first;
        let _ = w.append(node).unwrap();
        let _ = w.append(unsafe { (*node).next }).unwrap();

        // Search should find nothing
        assert!(w.next().is_none());
        assert!(w.next().is_none());

        // No pruning because both pages are needed to fit needle.
        assert_eq!(2, w.meta.len());

        w.deinit();
        s.deinit();
    }

    // test "SlidingWindow single append across circular buffer boundary"
    #[test]
    fn sliding_window_single_append_across_circular_buffer_boundary() {
        let alloc = Allocator;

        let mut w: SlidingWindow = SlidingWindow::init(alloc, Direction::Forward, b"abc").unwrap();

        let mut s = screen_init(80, 24, 0);
        s.test_write_string(b"XXXXXXXXXXXXXXXXXXXboo!XXXXX")
            .unwrap();

        // We are trying to break a circular buffer boundary so the way we
        // do this is to duplicate the data then do a failing search. This
        // will cause the first page to be pruned. The next time we append we'll
        // put it in the middle of the circ buffer. We assert this so that if
        // our implementation changes our test will fail.
        assert!(core::ptr::eq(s.pages.pages.first, s.pages.pages.last));
        let node: *mut Node = s.pages.pages.first;
        let _ = w.append(node).unwrap();
        let _ = w.append(node).unwrap();
        {
            // No wrap around yet
            let (s0, s1) = w.data.get_ptr_slice(0, w.data.len());
            let (s0, s1) = unsafe { (&*s0, &*s1) };
            assert!(!s0.is_empty());
            assert!(s1.is_empty());
        }

        // Search non-match, prunes page
        assert!(w.next().is_none());
        assert_eq!(1, w.meta.len());

        // Change the needle, just needs to be the same length (not a real API)
        w.test_change_needle(b"boo");

        // Add new page, now wraps
        let _ = w.append(node).unwrap();
        {
            let (s0, s1) = w.data.get_ptr_slice(0, w.data.len());
            let (s0, s1) = unsafe { (&*s0, &*s1) };
            assert!(!s0.is_empty());
            assert!(!s1.is_empty());
        }
        {
            let h = w.next().unwrap();
            let sel = h.untracked();
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 19, y: 0 })),
                s.pages.point_from_pin(point::Tag::Active, sel.start)
            );
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 21, y: 0 })),
                s.pages.point_from_pin(point::Tag::Active, sel.end)
            );
        }
        assert!(w.next().is_none());

        w.deinit();
        s.deinit();
    }

    // test "SlidingWindow single append match on boundary"
    #[test]
    fn sliding_window_single_append_match_on_boundary() {
        let alloc = Allocator;

        let mut w: SlidingWindow = SlidingWindow::init(alloc, Direction::Forward, b"abcd").unwrap();

        let mut s = screen_init(80, 24, 0);
        s.test_write_string(b"o!XXXXXXXXXXXXXXXXXXXbo").unwrap();

        // We need to surgically modify the last row to be soft-wrapped
        assert!(core::ptr::eq(s.pages.pages.first, s.pages.pages.last));
        let node: *mut Node = s.pages.pages.first;
        unsafe {
            let row = (*crate::pagelist::Node::page(node)).get_row(crate::pagelist::Node::rows(node) as usize - 1);
            (*row).set_wrap(true);
        }

        let _ = w.append(node).unwrap();
        let _ = w.append(node).unwrap();
        {
            // No wrap around yet
            let (s0, s1) = w.data.get_ptr_slice(0, w.data.len());
            let (s0, s1) = unsafe { (&*s0, &*s1) };
            assert!(!s0.is_empty());
            assert!(s1.is_empty());
        }

        // Search non-match, prunes page
        assert!(w.next().is_none());
        assert_eq!(1, w.meta.len());

        // Change the needle, just needs to be the same length (not a real API)
        w.test_change_needle(b"boo!");

        // Add new page, now wraps
        let _ = w.append(node).unwrap();
        {
            let (s0, s1) = w.data.get_ptr_slice(0, w.data.len());
            let (s0, s1) = unsafe { (&*s0, &*s1) };
            assert!(!s0.is_empty());
            assert!(!s1.is_empty());
        }
        {
            let h = w.next().unwrap();
            let sel = h.untracked();
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 21, y: 0 })),
                s.pages.point_from_pin(point::Tag::Active, sel.start)
            );
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 1, y: 0 })),
                s.pages.point_from_pin(point::Tag::Active, sel.end)
            );
        }
        assert!(w.next().is_none());

        w.deinit();
        s.deinit();
    }

    // test "SlidingWindow single append reversed"
    #[test]
    fn sliding_window_single_append_reversed() {
        let alloc = Allocator;

        let mut w: SlidingWindow = SlidingWindow::init(alloc, Direction::Reverse, b"boo!").unwrap();

        let mut s = screen_init(80, 24, 0);
        s.test_write_string(b"hello. boo! hello. boo!").unwrap();

        assert!(core::ptr::eq(s.pages.pages.first, s.pages.pages.last));
        let node: *mut Node = s.pages.pages.first;
        let _ = w.append(node).unwrap();

        {
            let h = w.next().unwrap();
            let sel = h.untracked();
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 19, y: 0 })),
                s.pages.point_from_pin(point::Tag::Active, sel.start)
            );
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 22, y: 0 })),
                s.pages.point_from_pin(point::Tag::Active, sel.end)
            );
        }
        {
            let h = w.next().unwrap();
            let sel = h.untracked();
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 7, y: 0 })),
                s.pages.point_from_pin(point::Tag::Active, sel.start)
            );
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 10, y: 0 })),
                s.pages.point_from_pin(point::Tag::Active, sel.end)
            );
        }
        assert!(w.next().is_none());
        assert!(w.next().is_none());

        w.deinit();
        s.deinit();
    }

    // test "SlidingWindow single append no match reversed"
    #[test]
    fn sliding_window_single_append_no_match_reversed() {
        let alloc = Allocator;

        let mut w: SlidingWindow =
            SlidingWindow::init(alloc, Direction::Reverse, b"nope!").unwrap();

        let mut s = screen_init(80, 24, 0);
        s.test_write_string(b"hello. boo! hello. boo!").unwrap();

        assert!(core::ptr::eq(s.pages.pages.first, s.pages.pages.last));
        let node: *mut Node = s.pages.pages.first;
        let _ = w.append(node).unwrap();

        // No matches
        assert!(w.next().is_none());
        assert!(w.next().is_none());

        // Should still keep the page
        assert_eq!(1, w.meta.len());

        w.deinit();
        s.deinit();
    }

    // test "SlidingWindow two pages reversed"
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (vt multi-page-PageList intractable, .port/phase-2/MIRI_EXCEPTIONS.md): builds/reflows multi-page PageLists that exceed Miri's interpreter budget (>20min); covered natively + by ASan (all PageList tests ~18s)"
    )]
    #[test]
    fn sliding_window_two_pages_reversed() {
        let alloc = Allocator;

        let mut w: SlidingWindow = SlidingWindow::init(alloc, Direction::Reverse, b"boo!").unwrap();

        let mut s = screen_init(80, 24, 1000);

        let first_page_rows = unsafe { crate::pagelist::Node::capacity(s.pages.pages.first).rows };
        for _ in 0..first_page_rows - 1 {
            s.test_write_string(b"\n").unwrap();
        }
        for _ in 0..s.pages.cols - 4 {
            s.test_write_string(b"x").unwrap();
        }
        s.test_write_string(b"boo!").unwrap();
        assert!(core::ptr::eq(s.pages.pages.first, s.pages.pages.last));
        s.test_write_string(b"\n").unwrap();
        assert!(!core::ptr::eq(s.pages.pages.first, s.pages.pages.last));
        s.test_write_string(b"hello. boo!").unwrap();

        // Add both pages in reverse order
        let node: *mut Node = s.pages.pages.first;
        let _ = w.append(unsafe { (*node).next }).unwrap();
        let _ = w.append(node).unwrap();

        // Search should find two matches (in reverse order)
        {
            let h = w.next().unwrap();
            let sel = h.untracked();
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 7, y: 23 })),
                s.pages.point_from_pin(point::Tag::Active, sel.start)
            );
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 10, y: 23 })),
                s.pages.point_from_pin(point::Tag::Active, sel.end)
            );
        }
        {
            let h = w.next().unwrap();
            let sel = h.untracked();
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 76, y: 22 })),
                s.pages.point_from_pin(point::Tag::Active, sel.start)
            );
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 79, y: 22 })),
                s.pages.point_from_pin(point::Tag::Active, sel.end)
            );
        }
        assert!(w.next().is_none());
        assert!(w.next().is_none());

        w.deinit();
        s.deinit();
    }

    // test "SlidingWindow two pages match across boundary reversed"
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (vt multi-page-PageList intractable, .port/phase-2/MIRI_EXCEPTIONS.md): builds/reflows multi-page PageLists that exceed Miri's interpreter budget (>20min); covered natively + by ASan (all PageList tests ~18s)"
    )]
    #[test]
    fn sliding_window_two_pages_match_across_boundary_reversed() {
        let alloc = Allocator;

        let mut w: SlidingWindow =
            SlidingWindow::init(alloc, Direction::Reverse, b"hello, world").unwrap();

        let mut s = screen_init(80, 24, 1000);

        let first_page_rows = unsafe { crate::pagelist::Node::capacity(s.pages.pages.first).rows };
        for _ in 0..first_page_rows - 1 {
            s.test_write_string(b"\n").unwrap();
        }
        for _ in 0..s.pages.cols - 4 {
            s.test_write_string(b"x").unwrap();
        }
        s.test_write_string(b"hell").unwrap();
        assert!(core::ptr::eq(s.pages.pages.first, s.pages.pages.last));
        s.test_write_string(b"o, world!").unwrap();
        assert!(!core::ptr::eq(s.pages.pages.first, s.pages.pages.last));

        // Add both pages in reverse order
        let node: *mut Node = s.pages.pages.first;
        let _ = w.append(unsafe { (*node).next }).unwrap();
        let _ = w.append(node).unwrap();

        // Search should find a match
        {
            let h = w.next().unwrap();
            let sel = h.untracked();
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 76, y: 22 })),
                s.pages.point_from_pin(point::Tag::Active, sel.start)
            );
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 7, y: 23 })),
                s.pages.point_from_pin(point::Tag::Active, sel.end)
            );
        }
        assert!(w.next().is_none());
        assert!(w.next().is_none());

        // In reverse mode, the last appended meta (first original page) is large
        // enough to contain needle.len - 1 bytes, so pruning occurs
        assert_eq!(1, w.meta.len());

        w.deinit();
        s.deinit();
    }

    // test "SlidingWindow two pages no match prunes first page reversed"
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (vt multi-page-PageList intractable, .port/phase-2/MIRI_EXCEPTIONS.md): builds/reflows multi-page PageLists that exceed Miri's interpreter budget (>20min); covered natively + by ASan (all PageList tests ~18s)"
    )]
    #[test]
    fn sliding_window_two_pages_no_match_prunes_first_page_reversed() {
        let alloc = Allocator;

        let mut w: SlidingWindow =
            SlidingWindow::init(alloc, Direction::Reverse, b"nope!").unwrap();

        let mut s = screen_init(80, 24, 1000);

        let first_page_rows = unsafe { crate::pagelist::Node::capacity(s.pages.pages.first).rows };
        for _ in 0..first_page_rows - 1 {
            s.test_write_string(b"\n").unwrap();
        }
        for _ in 0..s.pages.cols - 4 {
            s.test_write_string(b"x").unwrap();
        }
        s.test_write_string(b"boo!").unwrap();
        assert!(core::ptr::eq(s.pages.pages.first, s.pages.pages.last));
        s.test_write_string(b"\n").unwrap();
        assert!(!core::ptr::eq(s.pages.pages.first, s.pages.pages.last));
        s.test_write_string(b"hello. boo!").unwrap();

        // Add both pages in reverse order
        let node: *mut Node = s.pages.pages.first;
        let _ = w.append(unsafe { (*node).next }).unwrap();
        let _ = w.append(node).unwrap();

        // Search should find nothing
        assert!(w.next().is_none());
        assert!(w.next().is_none());

        // We should've pruned our page because the second page
        // has enough text to contain our needle.
        assert_eq!(1, w.meta.len());

        w.deinit();
        s.deinit();
    }

    // test "SlidingWindow two pages no match keeps both pages reversed"
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (vt multi-page-PageList intractable, .port/phase-2/MIRI_EXCEPTIONS.md): builds/reflows multi-page PageLists that exceed Miri's interpreter budget (>20min); covered natively + by ASan (all PageList tests ~18s)"
    )]
    #[test]
    fn sliding_window_two_pages_no_match_keeps_both_pages_reversed() {
        let alloc = Allocator;

        let mut s = screen_init(80, 24, 1000);

        let first_page_rows = unsafe { crate::pagelist::Node::capacity(s.pages.pages.first).rows };
        for _ in 0..first_page_rows - 1 {
            s.test_write_string(b"\n").unwrap();
        }
        for _ in 0..s.pages.cols - 4 {
            s.test_write_string(b"x").unwrap();
        }
        s.test_write_string(b"boo!").unwrap();
        assert!(core::ptr::eq(s.pages.pages.first, s.pages.pages.last));
        s.test_write_string(b"\n").unwrap();
        assert!(!core::ptr::eq(s.pages.pages.first, s.pages.pages.last));
        s.test_write_string(b"hello. boo!").unwrap();

        // Imaginary needle for search. Doesn't match!
        let mut needle_list: Vec<u8> = Vec::new();
        needle_list.resize((first_page_rows as usize) * (s.pages.cols as usize), b'x');
        let needle: &[u8] = &needle_list;

        let mut w: SlidingWindow = SlidingWindow::init(alloc, Direction::Reverse, needle).unwrap();

        // Add both pages in reverse order
        let node: *mut Node = s.pages.pages.first;
        let _ = w.append(unsafe { (*node).next }).unwrap();
        let _ = w.append(node).unwrap();

        // Search should find nothing
        assert!(w.next().is_none());
        assert!(w.next().is_none());

        // No pruning because both pages are needed to fit needle.
        assert_eq!(2, w.meta.len());

        w.deinit();
        s.deinit();
    }

    // test "SlidingWindow single append across circular buffer boundary reversed"
    #[test]
    fn sliding_window_single_append_across_circular_buffer_boundary_reversed() {
        let alloc = Allocator;

        let mut w: SlidingWindow = SlidingWindow::init(alloc, Direction::Reverse, b"abc").unwrap();

        let mut s = screen_init(80, 24, 0);
        s.test_write_string(b"XXXXXXXXXXXXXXXXXXXboo!XXXXX")
            .unwrap();

        assert!(core::ptr::eq(s.pages.pages.first, s.pages.pages.last));
        let node: *mut Node = s.pages.pages.first;
        let _ = w.append(node).unwrap();
        let _ = w.append(node).unwrap();
        {
            // No wrap around yet
            let (s0, s1) = w.data.get_ptr_slice(0, w.data.len());
            let (s0, s1) = unsafe { (&*s0, &*s1) };
            assert!(!s0.is_empty());
            assert!(s1.is_empty());
        }

        // Search non-match, prunes page
        assert!(w.next().is_none());
        assert_eq!(1, w.meta.len());

        // Change the needle, just needs to be the same length (not a real API)
        // testChangeNeedle doesn't reverse, so pass reversed needle for reverse mode
        w.test_change_needle(b"oob");

        // Add new page, now wraps
        let _ = w.append(node).unwrap();
        {
            let (s0, s1) = w.data.get_ptr_slice(0, w.data.len());
            let (s0, s1) = unsafe { (&*s0, &*s1) };
            assert!(!s0.is_empty());
            assert!(!s1.is_empty());
        }
        {
            let h = w.next().unwrap();
            let sel = h.untracked();
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 19, y: 0 })),
                s.pages.point_from_pin(point::Tag::Active, sel.start)
            );
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 21, y: 0 })),
                s.pages.point_from_pin(point::Tag::Active, sel.end)
            );
        }
        assert!(w.next().is_none());

        w.deinit();
        s.deinit();
    }

    // test "SlidingWindow single append match on boundary reversed"
    #[test]
    fn sliding_window_single_append_match_on_boundary_reversed() {
        let alloc = Allocator;

        let mut w: SlidingWindow = SlidingWindow::init(alloc, Direction::Reverse, b"abcd").unwrap();

        let mut s = screen_init(80, 24, 0);
        s.test_write_string(b"o!XXXXXXXXXXXXXXXXXXXbo").unwrap();

        // We need to surgically modify the last row to be soft-wrapped
        assert!(core::ptr::eq(s.pages.pages.first, s.pages.pages.last));
        let node: *mut Node = s.pages.pages.first;
        unsafe {
            let row = (*crate::pagelist::Node::page(node)).get_row(crate::pagelist::Node::rows(node) as usize - 1);
            (*row).set_wrap(true);
        }

        let _ = w.append(node).unwrap();
        let _ = w.append(node).unwrap();
        {
            // No wrap around yet
            let (s0, s1) = w.data.get_ptr_slice(0, w.data.len());
            let (s0, s1) = unsafe { (&*s0, &*s1) };
            assert!(!s0.is_empty());
            assert!(s1.is_empty());
        }

        // Search non-match, prunes page
        assert!(w.next().is_none());
        assert_eq!(1, w.meta.len());

        // Change the needle, just needs to be the same length (not a real API)
        // testChangeNeedle doesn't reverse, so pass reversed needle for reverse mode
        w.test_change_needle(b"!oob");

        // Add new page, now wraps
        let _ = w.append(node).unwrap();
        {
            let (s0, s1) = w.data.get_ptr_slice(0, w.data.len());
            let (s0, s1) = unsafe { (&*s0, &*s1) };
            assert!(!s0.is_empty());
            assert!(!s1.is_empty());
        }
        {
            let h = w.next().unwrap();
            let sel = h.untracked();
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 21, y: 0 })),
                s.pages.point_from_pin(point::Tag::Active, sel.start)
            );
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 1, y: 0 })),
                s.pages.point_from_pin(point::Tag::Active, sel.end)
            );
        }
        assert!(w.next().is_none());

        w.deinit();
        s.deinit();
    }

    // test "SlidingWindow single append soft wrapped"
    #[test]
    fn sliding_window_single_append_soft_wrapped() {
        let alloc = Allocator;

        let mut w: SlidingWindow = SlidingWindow::init(alloc, Direction::Forward, b"boo!").unwrap();

        let mut t: Terminal = Terminal::init(alloc, terminal_opts(4, 5)).unwrap();

        let mut s = t.vt_stream();
        s.next_slice(b"A\r\nxxboo!\r\nC");
        drop(s);

        // We want to test single-page cases.
        let screen: &mut Screen = unsafe { &mut *t.screens.active };
        assert!(core::ptr::eq(
            screen.pages.pages.first,
            screen.pages.pages.last
        ));
        let node: *mut Node = screen.pages.pages.first;
        let _ = w.append(node).unwrap();

        // We should be able to find two matches.
        {
            let h = w.next().unwrap();
            let sel = h.untracked();
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 2, y: 1 })),
                screen.pages.point_from_pin(point::Tag::Active, sel.start)
            );
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 1, y: 2 })),
                screen.pages.point_from_pin(point::Tag::Active, sel.end)
            );
        }
        assert!(w.next().is_none());
        assert!(w.next().is_none());

        w.deinit();
        t.deinit(alloc);
    }

    // test "SlidingWindow single append reversed soft wrapped"
    #[test]
    fn sliding_window_single_append_reversed_soft_wrapped() {
        let alloc = Allocator;

        let mut w: SlidingWindow = SlidingWindow::init(alloc, Direction::Reverse, b"boo!").unwrap();

        let mut t: Terminal = Terminal::init(alloc, terminal_opts(4, 5)).unwrap();

        let mut s = t.vt_stream();
        s.next_slice(b"A\r\nxxboo!\r\nC");
        drop(s);

        let screen: &mut Screen = unsafe { &mut *t.screens.active };
        assert!(core::ptr::eq(
            screen.pages.pages.first,
            screen.pages.pages.last
        ));
        let node: *mut Node = screen.pages.pages.first;
        let _ = w.append(node).unwrap();

        {
            let h = w.next().unwrap();
            let sel = h.untracked();
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 2, y: 1 })),
                screen.pages.point_from_pin(point::Tag::Active, sel.start)
            );
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 1, y: 2 })),
                screen.pages.point_from_pin(point::Tag::Active, sel.end)
            );
        }
        assert!(w.next().is_none());
        assert!(w.next().is_none());

        w.deinit();
        t.deinit(alloc);
    }

    // This tests a real bug that occurred where a whitespace-only page
    // that encodes to zero bytes would crash.
    // test "SlidingWindow append whitespace only node"
    #[test]
    fn sliding_window_append_whitespace_only_node() {
        let alloc = Allocator;

        let mut w: SlidingWindow = SlidingWindow::init(alloc, Direction::Forward, b"x").unwrap();

        let mut s = screen_init(80, 24, 0);

        // By setting the empty page to wrap we get a zero-byte page.
        // This is invasive but its otherwise hard to reproduce naturally
        // without creating a slow test.
        let node: *mut Node = s.pages.pages.first;
        unsafe {
            let last_row = (*crate::pagelist::Node::page(node)).get_row(crate::pagelist::Node::rows(node) as usize - 1);
            (*last_row).set_wrap(true);
        }

        assert!(core::ptr::eq(s.pages.pages.first, s.pages.pages.last));
        let _ = w.append(node).unwrap();

        // No matches expected
        assert!(w.next().is_none());

        w.deinit();
        s.deinit();
    }
}
