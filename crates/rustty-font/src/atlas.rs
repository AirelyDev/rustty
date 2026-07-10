//! Implements a texture atlas (https://en.wikipedia.org/wiki/Texture_atlas).
//!
//! The implementation is based on "A Thousand Ways to Pack the Bin - A
//! Practical Approach to Two-Dimensional Rectangle Bin Packing" by Jukka
//! Jylänki. This specific implementation is based heavily on
//! Nicolas P. Rougier's freetype-gl project as well as Jukka's C++
//! implementation: https://github.com/juj/RectangleBinPack
//!
//! Limitations that are easy to fix, but I didn't need them:
//!
//!   * Written data must be packed, no support for custom strides.
//!   * Texture is always a square, no ability to set width != height. Note
//!     that regions written INTO the atlas do not have to be square, only
//!     the full atlas texture itself.
//!
// const Atlas = @This();
//
// const std = @import("std");
// const assert = @import("../quirks.zig").inlineAssert;
// const Allocator = std.mem.Allocator;
// const testing = std.testing;
// const fastmem = @import("../fastmem.zig");
// const tripwire = @import("../tripwire.zig");
//
// PORT NOTE: `assert = quirks.inlineAssert`, `fastmem`, and `tripwire` live in
// `rustty-util` (the FIRM leaf crate). `std.mem.Allocator` is faithfully modeled
// by `allocator_api2::alloc::Allocator`; callers pass `allocator_api2::alloc::
// Global` for the prior `std.alloc` behavior. The `Atlas = @This()` file-struct
// becomes the `Atlas<A>` struct below, carrying the allocator type as a generic
// `A: Allocator + Copy` (defaulting to `Global`), so `data`/`nodes` own their
// heap storage and free it on drop (mirroring `deinit`).
#![allow(non_snake_case)]

use allocator_api2::alloc::{Allocator, Global};
use allocator_api2::vec::Vec;
use core::sync::atomic::{AtomicUsize, Ordering};
use rustty_util::fastmem;
use rustty_util::quirks::inlineAssert as assert;

// const log = std.log.scoped(.atlas);
//
// PORT NOTE: the only use of `log` is `log.err` in `dump`'s unsupported-format
// arm; there we route through `eprintln!` with the same `atlas` scope.

/// The format of the texture data being written into the Atlas. This must be
/// uniform for all textures in the Atlas. If you have some textures with
/// different formats, you must use multiple atlases or convert the textures.
// pub const Format = enum(u8) { ... };
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Format {
    /// 1 byte per pixel grayscale.
    Grayscale = 0,
    /// 3 bytes per pixel BGR.
    Bgr = 1,
    /// 4 bytes per pixel BGRA.
    Bgra = 2,
}

impl Format {
    // pub fn depth(self: Format) u8 { ... }
    pub fn depth(self) -> u8 {
        match self {
            Format::Grayscale => 1,
            Format::Bgr => 3,
            Format::Bgra => 4,
        }
    }
}

// const Node = struct { x: u32, y: u32, width: u32 };
#[derive(Clone, Copy)]
struct Node {
    x: u32,
    y: u32,
    width: u32,
}

/// pub const Error = error{ AtlasFull };
// /// Atlas cannot fit the desired region. You must enlarge the atlas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// Atlas cannot fit the desired region. You must enlarge the atlas.
    AtlasFull,
}

/// The allocation-failure error, mirroring Zig's `Allocator.Error` / OutOfMemory.
// PORT NOTE: the reference functions return `Allocator.Error!T`; in Rust this is
// `Result<T, OutOfMemory>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutOfMemory;

/// The combined error set for `reserve`, mirroring the reference's
/// `(Allocator.Error || Error)!Region`.
// PORT NOTE: Zig's `reserve` returns the error union of `Allocator.Error` and
// the Atlas `Error` set; Rust models that union as this two-variant enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReserveError {
    OutOfMemory,
    AtlasFull,
}

impl From<OutOfMemory> for ReserveError {
    fn from(_: OutOfMemory) -> Self {
        ReserveError::OutOfMemory
    }
}

impl From<Error> for ReserveError {
    fn from(e: Error) -> Self {
        match e {
            Error::AtlasFull => ReserveError::AtlasFull,
        }
    }
}

/// A region within the texture atlas. These can be acquired using the
/// "reserve" function. A region reservation is required to write data.
// pub const Region = extern struct { ... };
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Region {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Number of nodes to preallocate in the list on init.
///
/// TODO: figure out optimal prealloc based on real world usage
const NODE_PREALLOC: usize = 64;

/// The texture atlas. (Zig file-struct `Atlas = @This()`.)
pub struct Atlas<A: Allocator + Copy = Global> {
    /// Data is the raw texture data.
    // data: []u8,
    data: Vec<u8, A>,

    /// Width and height of the atlas texture. The current implementation is
    /// always square so this is both the width and the height.
    size: u32,

    /// The nodes (rectangles) of available space.
    // nodes: std.ArrayListUnmanaged(Node) = .{},
    nodes: Vec<Node, A>,

    /// The format of the texture data being written into the Atlas. This must be
    /// uniform for all textures in the Atlas. If you have some textures with
    /// different formats, you must use multiple atlases or convert the textures.
    // PORT NOTE: `pub(crate)` so `sprite::canvas::Canvas::writeAtlas` can assert
    // `atlas.format == .grayscale` exactly as the reference does (Zig struct
    // fields are public across files; this narrows that to the crate).
    pub(crate) format: Format,

    /// This will be incremented every time the atlas is modified. This is useful
    /// for knowing if the texture data has changed since the last time it was
    /// sent to the GPU. It is up the user of the atlas to read this value atomically
    /// to observe it.
    // modified: std.atomic.Value(usize) = .{ .raw = 0 },
    modified: AtomicUsize,

    /// This will be incremented every time the atlas is resized. This is useful
    /// for knowing if a GPU texture can be updated in-place or if it requires
    /// a resize operation.
    // resized: std.atomic.Value(usize) = .{ .raw = 0 },
    resized: AtomicUsize,
}

// pub const init_tw = tripwire.module(enum { alloc_data, alloc_nodes }, init);
//
// PORT NOTE: the Zig `tripwire.module(P, E)` is a comptime function returning a
// type with type-static state; the Rust `rustty_util::tripwire::Module<P, E>` is
// an INSTANCE holding the same state. To share that state between `init` (which
// calls `check`) and the "init error" test (which calls `errorAlways` over every
// `init_tw.FailPoint`), the module lives in a `#[cfg(test)] thread_local!` and is
// reached through the `init_tw_check` helper below. In non-test builds the call
// site is `#[cfg(test)]`-gated, so production `init` never reaches `check`,
// mirroring the Zig `enabled = builtin.is_test` gate. The same scheme is used
// for `reserve_tw` and `grow_tw`.

impl<A: Allocator + Copy> Atlas<A> {
    // pub fn init(alloc: Allocator, size: u32, format: Format) Allocator.Error!Atlas {
    /// Read-only view of the raw texture data.
    // PORT NOTE: the reference `data` is a public field; downstream faces (e.g.
    // the freetype face's render tests) read it directly. Rust keeps the field
    // private and exposes this accessor.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// The width/height of the (square) atlas texture.
    // PORT NOTE: the reference `size` is a public field.
    pub fn size(&self) -> u32 {
        self.size
    }

    /// The pixel format of the atlas.
    // PORT NOTE: the reference `format` is a public field; the freetype face
    // reads it (`atlas.format.depth()`) to pick pitch/depth during render.
    pub fn format(&self) -> Format {
        self.format
    }

    /// The modification counter, incremented on every write to the atlas. The GPU
    /// renderer reads this to know if a frame's cached texture is stale (its
    /// `drawFrame` atlas-sync compares `atlas.modified() > frame.grayscale_modified`).
    // PORT NOTE: the reference `modified` is a public field the renderer reads with
    // a `.monotonic` atomic load (`self.font_grid.atlas_grayscale.modified.load(
    // .monotonic)`); rustty keeps the field private and exposes this accessor (the
    // `data`/`size`/`format` accessor precedent). `.monotonic` → `Ordering::Relaxed`
    // (the tree-wide mapping used at the `fetch_add` call sites in this file).
    pub fn modified(&self) -> usize {
        self.modified.load(Ordering::Relaxed)
    }

    pub fn init(alloc: A, size: u32, format: Format) -> Result<Atlas<A>, OutOfMemory> {
        // const tw = init_tw;

        // try tw.check(.alloc_data);
        #[cfg(test)]
        init_tw_check(InitFp::AllocData)?;
        // var result = Atlas{
        //     .data = try alloc.alloc(u8, size * size * format.depth()),
        //     .size = size,
        //     .nodes = .{},
        //     .format = format,
        // };
        // errdefer result.deinit(alloc);
        //
        // PORT NOTE: `alloc.alloc` returns undefined memory; `Vec::with_capacity_in`
        // aborts on allocation failure rather than returning an error, so we use
        // `try_reserve_exact` over an empty `Vec` to faithfully surface OOM as
        // `Err(OutOfMemory)`. `resize(n, 0)` gives the slice a defined length
        // (the eventual `clear()` `@memset`s it again, harmlessly). The Zig
        // `errdefer result.deinit(alloc)` is the Rust drop of `result` on the
        // early `?`-return below, which frees `data` via its owning allocator.
        let n = (size as usize) * (size as usize) * (format.depth() as usize);
        let mut data: Vec<u8, A> = Vec::new_in(alloc);
        data.try_reserve_exact(n).map_err(|_| OutOfMemory)?;
        data.resize(n, 0);
        let mut result = Atlas {
            data,
            size,
            nodes: Vec::new_in(alloc),
            format,
            modified: AtomicUsize::new(0),
            resized: AtomicUsize::new(0),
        };

        // Prealloc some nodes.
        // try tw.check(.alloc_nodes);
        #[cfg(test)]
        init_tw_check(InitFp::AllocNodes)?;
        // result.nodes = try .initCapacity(alloc, node_prealloc);
        result
            .nodes
            .try_reserve_exact(NODE_PREALLOC)
            .map_err(|_| OutOfMemory)?;

        // This sets up our initial state
        result.clear();

        Ok(result)
    }

    // pub fn deinit(self: *Atlas, alloc: Allocator) void {
    //     self.nodes.deinit(alloc);
    //     alloc.free(self.data);
    //     self.* = undefined;
    // }
    //
    // PORT NOTE: the allocators owned by `nodes`/`data` free their storage on
    // drop; we take `alloc` to mirror the reference signature. `self.* =
    // undefined` has no Rust analogue — the value is consumed by drop.
    pub fn deinit(self, _alloc: A) {
        let _ = self;
    }

    // pub const reserve_tw = tripwire.module(enum { insert_node }, reserve);

    /// Reserve a region within the atlas with the given width and height.
    ///
    /// May allocate to add a new rectangle into the internal list of rectangles.
    /// This will not automatically enlarge the texture if it is full.
    // pub fn reserve(
    //     self: *Atlas,
    //     alloc: Allocator,
    //     width: u32,
    //     height: u32,
    // ) (Allocator.Error || Error)!Region {
    pub fn reserve(&mut self, _alloc: A, width: u32, height: u32) -> Result<Region, ReserveError> {
        // const tw = reserve_tw;

        // x, y are populated within :best_idx below
        // var region: Region = .{ .x = 0, .y = 0, .width = width, .height = height };
        let mut region: Region = Region {
            x: 0,
            y: 0,
            width,
            height,
        };

        // If our width/height are 0, then we return the region as-is. This
        // may seem like an error case but it simplifies downstream callers who
        // might be trying to write empty data.
        if width == 0 && height == 0 {
            return Ok(region);
        }

        // Find the location in our nodes list to insert the new node for this region.
        let best_idx: usize = {
            // var best_height: u32 = std.math.maxInt(u32);
            let mut best_height: u32 = u32::MAX;
            // var best_width: u32 = best_height;
            let mut best_width: u32 = best_height;
            // var chosen: ?usize = null;
            let mut chosen: Option<usize> = None;

            // var i: usize = 0;
            // while (i < self.nodes.items.len) : (i += 1) {
            let mut i: usize = 0;
            while i < self.nodes.len() {
                // Check if our region fits within this node.
                // const y = self.fit(i, width, height) orelse continue;
                let y = match self.fit(i, width, height) {
                    Some(y) => y,
                    None => {
                        i += 1;
                        continue;
                    }
                };

                // const node = self.nodes.items[i];
                let node = self.nodes[i];
                // if ((y + height) < best_height or
                //     ((y + height) == best_height and
                //         (node.width > 0 and node.width < best_width)))
                if (y + height) < best_height
                    || ((y + height) == best_height && (node.width > 0 && node.width < best_width))
                {
                    chosen = Some(i);
                    best_width = node.width;
                    best_height = y + height;
                    region.x = node.x;
                    region.y = y;
                }

                i += 1;
            }

            // If we never found a chosen index, the atlas cannot fit our region.
            // break :best_idx chosen orelse return Error.AtlasFull;
            match chosen {
                Some(c) => c,
                None => return Err(ReserveError::AtlasFull),
            }
        };

        // Insert our new node for this rectangle at the exact best index
        // try tw.check(.insert_node);
        #[cfg(test)]
        reserve_tw_check(ReserveFp::InsertNode)?;
        // try self.nodes.insert(alloc, best_idx, .{
        //     .x = region.x,
        //     .y = region.y + height,
        //     .width = width,
        // });
        // errdefer comptime unreachable;
        //
        // PORT NOTE: `Vec::insert` aborts on OOM, so we `try_reserve(1)` first to
        // surface allocation failure as `Err`; the insert proper is then
        // infallible (mirroring `errdefer comptime unreachable`).
        self.nodes
            .try_reserve(1)
            .map_err(|_| ReserveError::OutOfMemory)?;
        self.nodes.insert(
            best_idx,
            Node {
                x: region.x,
                y: region.y + height,
                width,
            },
        );

        // Optimize our rectangles
        // var i: usize = best_idx + 1;
        // while (i < self.nodes.items.len) : (i += 1) {
        //
        // PORT NOTE: the only loop-back path is the `node.width == 0` removal
        // (`i -= 1` cancels the `: (i += 1)` continuation, re-examining the same
        // index after the shift); every other path `break`s. So `i` is never
        // advanced and stays `best_idx + 1` throughout — hence not `mut`.
        let i: usize = best_idx + 1;
        while i < self.nodes.len() {
            // const node = &self.nodes.items[i];
            // const prev = self.nodes.items[i - 1];
            let prev = self.nodes[i - 1];
            // if (node.x < (prev.x + prev.width)) {
            if self.nodes[i].x < (prev.x + prev.width) {
                // const shrink = prev.x + prev.width - node.x;
                let shrink = prev.x + prev.width - self.nodes[i].x;
                // node.x += shrink;
                self.nodes[i].x += shrink;
                // node.width -|= shrink;
                self.nodes[i].width = self.nodes[i].width.saturating_sub(shrink);
                // if (node.width <= 0) {
                if self.nodes[i].width == 0 {
                    // _ = self.nodes.orderedRemove(i);
                    self.nodes.remove(i);
                    // i -= 1; continue;
                    //
                    // PORT NOTE: `i -= 1` followed by the `: (i += 1)` loop
                    // continuation leaves `i` unchanged, so we re-examine the same
                    // index after the shift.
                    continue;
                }
            }

            break;
        }
        self.merge();

        Ok(region)
    }

    /// Attempts to fit a rectangle of width x height into the node at idx.
    /// The return value is the y within the texture where the rectangle can be
    /// placed. The x is the same as the node.
    // fn fit(self: Atlas, idx: usize, width: u32, height: u32) ?u32 {
    fn fit(&self, idx: usize, width: u32, height: u32) -> Option<u32> {
        // If the added width exceeds our texture size, it doesn't fit.
        // const node = self.nodes.items[idx];
        let node = self.nodes[idx];
        // if ((node.x + width) > (self.size - 1)) return null;
        if (node.x + width) > (self.size - 1) {
            return None;
        }

        // Go node by node looking for space that can fit our width.
        // var y = node.y;
        // var i = idx;
        // var width_left = width;
        let mut y = node.y;
        let mut i = idx;
        let mut width_left = width;
        // while (width_left > 0) : (i += 1) {
        while width_left > 0 {
            // const n = self.nodes.items[i];
            let n = self.nodes[i];
            // if (n.y > y) y = n.y;
            if n.y > y {
                y = n.y;
            }

            // If the added height exceeds our texture size, it doesn't fit.
            // if ((y + height) > (self.size - 1)) return null;
            if (y + height) > (self.size - 1) {
                return None;
            }

            // width_left -|= n.width;
            width_left = width_left.saturating_sub(n.width);
            i += 1;
        }

        Some(y)
    }

    /// Merge adjacent nodes with the same y value.
    // fn merge(self: *Atlas) void {
    fn merge(&mut self) {
        // var i: usize = 0;
        // while (i < self.nodes.items.len - 1) {
        let mut i: usize = 0;
        while i < self.nodes.len() - 1 {
            // const node = &self.nodes.items[i];
            // const next = self.nodes.items[i + 1];
            let next = self.nodes[i + 1];
            // if (node.y == next.y) {
            if self.nodes[i].y == next.y {
                // node.width += next.width;
                self.nodes[i].width += next.width;
                // _ = self.nodes.orderedRemove(i + 1);
                self.nodes.remove(i + 1);
                // continue;
                continue;
            }

            // i += 1;
            i += 1;
        }
    }

    /// Set the data associated with a reserved region. The data is expected
    /// to fit exactly within the region. The data must be formatted with the
    /// proper bpp configured on init.
    // pub fn set(self: *Atlas, reg: Region, data: []const u8) void {
    pub fn set(&mut self, reg: Region, data: &[u8]) {
        assert(reg.x < (self.size - 1));
        assert((reg.x + reg.width) <= (self.size - 1));
        assert(reg.y < (self.size - 1));
        assert((reg.y + reg.height) <= (self.size - 1));

        // const depth = self.format.depth();
        let depth = self.format.depth() as u32;
        // var i: u32 = 0;
        // while (i < reg.height) : (i += 1) {
        let mut i: u32 = 0;
        while i < reg.height {
            // const tex_offset = (((reg.y + i) * self.size) + reg.x) * depth;
            let tex_offset = (((reg.y + i) * self.size) + reg.x) * depth;
            // const data_offset = i * reg.width * depth;
            let data_offset = i * reg.width * depth;
            // fastmem.copy(
            //     u8,
            //     self.data[tex_offset..],
            //     data[data_offset .. data_offset + (reg.width * depth)],
            // );
            fastmem::copy(
                &mut self.data[tex_offset as usize..],
                &data[data_offset as usize..(data_offset + (reg.width * depth)) as usize],
            );
            i += 1;
        }

        // _ = self.modified.fetchAdd(1, .monotonic);
        self.modified.fetch_add(1, Ordering::Relaxed);
    }

    /// Like `set` but allows specifying a width for the source data and an
    /// offset x and y, so that a section of a larger buffer may be copied
    /// in to the atlas.
    // pub fn setFromLarger(
    //     self: *Atlas,
    //     reg: Region,
    //     src: []const u8,
    //     src_width: u32,
    //     src_x: u32,
    //     src_y: u32,
    // ) void {
    pub fn setFromLarger(
        &mut self,
        reg: Region,
        src: &[u8],
        src_width: u32,
        src_x: u32,
        src_y: u32,
    ) {
        assert(reg.x < (self.size - 1));
        assert((reg.x + reg.width) <= (self.size - 1));
        assert(reg.y < (self.size - 1));
        assert((reg.y + reg.height) <= (self.size - 1));

        // const depth = self.format.depth();
        let depth = self.format.depth() as u32;
        // var i: u32 = 0;
        // while (i < reg.height) : (i += 1) {
        let mut i: u32 = 0;
        while i < reg.height {
            // const tex_offset = (((reg.y + i) * self.size) + reg.x) * depth;
            let tex_offset = (((reg.y + i) * self.size) + reg.x) * depth;
            // const src_offset = (((src_y + i) * src_width) + src_x) * depth;
            let src_offset = (((src_y + i) * src_width) + src_x) * depth;
            // fastmem.copy(
            //     u8,
            //     self.data[tex_offset..],
            //     src[src_offset .. src_offset + (reg.width * depth)],
            // );
            fastmem::copy(
                &mut self.data[tex_offset as usize..],
                &src[src_offset as usize..(src_offset + (reg.width * depth)) as usize],
            );
            i += 1;
        }

        // _ = self.modified.fetchAdd(1, .monotonic);
        self.modified.fetch_add(1, Ordering::Relaxed);
    }

    // pub const grow_tw = tripwire.module(enum { ensure_node_capacity, alloc_data }, grow);

    // Grow the texture to the new size, preserving all previously written data.
    // pub fn grow(self: *Atlas, alloc: Allocator, size_new: u32) Allocator.Error!void {
    pub fn grow(&mut self, alloc: A, size_new: u32) -> Result<(), OutOfMemory> {
        // const tw = grow_tw;

        assert(size_new >= self.size);
        // if (size_new == self.size) return;
        if size_new == self.size {
            return Ok(());
        }

        // We reserve space ahead of time for the new node, so that we
        // won't have to handle any errors after allocating our new data.
        // try tw.check(.ensure_node_capacity);
        #[cfg(test)]
        grow_tw_check(GrowFp::EnsureNodeCapacity)?;
        // try self.nodes.ensureUnusedCapacity(alloc, 1);
        self.nodes.try_reserve(1).map_err(|_| OutOfMemory)?;

        // try tw.check(.alloc_data);
        #[cfg(test)]
        grow_tw_check(GrowFp::AllocData)?;
        // const data_new = try alloc.alloc(
        //     u8,
        //     size_new * size_new * self.format.depth(),
        // );
        let n = (size_new as usize) * (size_new as usize) * (self.format.depth() as usize);
        let mut data_new: Vec<u8, A> = Vec::new_in(alloc);
        data_new.try_reserve_exact(n).map_err(|_| OutOfMemory)?;
        data_new.resize(n, 0);

        // Function is infallible from this point.
        // errdefer comptime unreachable;

        // Keep track of our old data so that we can copy it.
        // const data_old = self.data;
        // const size_old = self.size;
        let size_old = self.size;

        // Update our data and size to our new ones.
        // self.data = data_new;
        // self.size = size_new;
        //
        // PORT NOTE: `mem::replace` swaps the new data in and hands us the old
        // buffer (`data_old`); the old `Vec` is dropped at the end of the
        // function (mirroring `defer alloc.free(data_old)`).
        let data_old = core::mem::replace(&mut self.data, data_new);
        self.size = size_new;

        // Free the old data once we're done with it.
        // defer alloc.free(data_old);

        // Zero the new data out and copy the old data over.
        // @memset(self.data, 0);
        self.data.fill(0);
        // self.set(.{
        //     .x = 0, // don't bother skipping border so we can avoid strides
        //     .y = 1, // skip the first border row
        //     .width = size_old,
        //     .height = size_old - 2, // skip the last border row
        // }, data_old[size_old * self.format.depth() ..]);
        self.set(
            Region {
                x: 0, // don't bother skipping border so we can avoid strides
                y: 1, // skip the first border row
                width: size_old,
                height: size_old - 2, // skip the last border row
            },
            &data_old[(size_old as usize * self.format.depth() as usize)..],
        );

        // Add the new rectangle for our added righthand space.
        // self.nodes.appendAssumeCapacity(.{
        //     .x = size_old - 1,
        //     .y = 1,
        //     .width = size_new - size_old,
        // });
        self.nodes.push(Node {
            x: size_old - 1,
            y: 1,
            width: size_new - size_old,
        });

        // We are both modified and resized
        // _ = self.modified.fetchAdd(1, .monotonic);
        // _ = self.resized.fetchAdd(1, .monotonic);
        self.modified.fetch_add(1, Ordering::Relaxed);
        self.resized.fetch_add(1, Ordering::Relaxed);

        Ok(())
    }

    // Empty the atlas. This doesn't reclaim any previously allocated memory.
    // pub fn clear(self: *Atlas) void {
    pub fn clear(&mut self) {
        // _ = self.modified.fetchAdd(1, .monotonic);
        self.modified.fetch_add(1, Ordering::Relaxed);
        // @memset(self.data, 0);
        self.data.fill(0);
        // self.nodes.clearRetainingCapacity();
        self.nodes.clear();

        // Add our initial rectangle. This is the size of the full texture
        // and is the initial rectangle we fit our regions in. We keep a 1px border
        // to avoid artifacting when sampling the texture.
        // self.nodes.appendAssumeCapacity(.{ .x = 1, .y = 1, .width = self.size - 2 });
        self.nodes.push(Node {
            x: 1,
            y: 1,
            width: self.size - 2,
        });
    }

    /// Dump the atlas as a PPM to a writer, for debug purposes.
    /// Only supports grayscale and bgr atlases.
    ///
    /// NOTE: BGR atlases will have the red and blue channels
    ///       swapped because PPM expects RGB. This would be
    ///       easy enough to fix so next time someone needs
    ///       to debug a color atlas they should fix it.
    // pub fn dump(self: Atlas, writer: *std.Io.Writer) std.Io.Writer.Error!void {
    pub fn dump<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        // try writer.print(
        //     \\P{c}
        //     \\{d} {d}
        //     \\255
        //     \\
        // , .{
        //     @as(u8, switch (self.format) {
        //         .grayscale => '5',
        //         .bgr => '6',
        //         else => {
        //             log.err("Unsupported format for dump: {}", .{self.format});
        //             @panic("Cannot dump this atlas format.");
        //         },
        //     }),
        //     self.size,
        //     self.size,
        // });
        let c: char = match self.format {
            Format::Grayscale => '5',
            Format::Bgr => '6',
            _ => {
                eprintln!(
                    "error(atlas): Unsupported format for dump: {:?}",
                    self.format
                );
                panic!("Cannot dump this atlas format.");
            }
        };
        write!(writer, "P{}\n{} {}\n255\n", c, self.size, self.size)?;
        // try writer.writeAll(self.data);
        writer.write_all(&self.data)?;
        Ok(())
    }
}

// PORT NOTE (wasm — feature-gated OFF): the reference defines a `pub const Wasm =
// struct { ... }` block exposing the atlas as a C-ABI/JS surface for the wasm
// build. It depends on `os/wasm.zig` (the wasm bump allocator + host-ownership
// helpers) and `zig-js` (DOM/canvas bindings), neither of which exists in PURE
// rustty-font (R4). Per the agreed plan the wasm surface is feature-gated OFF
// (an off-by-default `wasm` cargo feature) and joins the wasm artifact only when
// the rustty wasm app-runtime lands — exactly as the config/vt wasm exports are
// deferred. The faithful reference is recorded verbatim below so no reference
// line or comment is dropped (R1); none of it pulls a C/JS edge into this crate.
// The block's ONE test (`test "happy path"`, 4 assertions) defers with it and is
// ledgered PER-TEST in `.port/phase-2/DEFERRALS.md` ("rustty-font Atlas wasm
// surface") — porting the Wasm block must port that test with all 4 asserts live.
//
// /// The wasm-compatible API. This lacks documentation unless the API differs
// /// from the standard Zig API. To learn what a function does, just look one
// /// level deeper to what Zig function is called and read the documentation there.
// pub const Wasm = struct {
//     // If you're copying this file (Atlas.zig) out to a separate project,
//     // just replace this with the allocator you want to use.
//     const wasm = @import("../os/wasm.zig");
//     const alloc = wasm.alloc;
//     const js = @import("zig-js");
//
//     export fn atlas_new(size: u32, format: u8) ?*Atlas {
//         const atlas = init(
//             alloc,
//             size,
//             @enumFromInt(format),
//         ) catch return null;
//         const result = alloc.create(Atlas) catch return null;
//         result.* = atlas;
//         return result;
//     }
//
//     export fn atlas_free(ptr: ?*Atlas) void {
//         if (ptr) |v| {
//             v.deinit(alloc);
//             alloc.destroy(v);
//         }
//     }
//
//     /// The return value for this should be freed by the caller with "free".
//     export fn atlas_reserve(self: *Atlas, width: u32, height: u32) ?*Region {
//         return atlas_reserve_(self, width, height) catch return null;
//     }
//
//     fn atlas_reserve_(self: *Atlas, width: u32, height: u32) !*Region {
//         const reg = try self.reserve(alloc, width, height);
//         const result = try alloc.create(Region);
//         errdefer alloc.destroy(result);
//         _ = try wasm.toHostOwned(result);
//         result.* = reg;
//         return result;
//     }
//
//     export fn atlas_set(self: *Atlas, reg: *Region, data: [*]const u8, len: usize) void {
//         self.set(reg.*, data[0..len]);
//     }
//
//     export fn atlas_grow(self: *Atlas, size_new: u32) bool {
//         self.grow(alloc, size_new) catch return false;
//         return true;
//     }
//
//     export fn atlas_clear(self: *Atlas) void {
//         self.clear();
//     }
//
//     /// This creates a Canvas element identified by the id returned that
//     /// the caller can draw into the DOM to visualize the atlas. The returned
//     /// ID must be freed from the JS runtime by calling "zigjs.deleteValue".
//     export fn atlas_debug_canvas(self: *Atlas) u32 {
//         return atlas_debug_canvas_(self) catch |err| {
//             log.warn("error dumping atlas canvas err={}", .{err});
//             return 0;
//         };
//     }
//
//     fn atlas_debug_canvas_(self: *Atlas) !u32 {
//         // Create our canvas
//         const doc = try js.global.get(js.Object, "document");
//         defer doc.deinit();
//         const canvas = try doc.call(js.Object, "createElement", .{js.string("canvas")});
//         errdefer canvas.deinit();
//
//         // Setup our canvas size
//         {
//             try canvas.set("width", self.size);
//             try canvas.set("height", self.size);
//
//             const width_str = try std.fmt.allocPrint(alloc, "{d}px", .{self.size});
//             defer alloc.free(width_str);
//
//             const style = try canvas.get(js.Object, "style");
//             defer style.deinit();
//             try style.set("width", js.string(width_str));
//             try style.set("height", js.string(width_str));
//         }
//
//         // This will return the same context on subsequent calls so it
//         // is important to reset it.
//         const ctx = try canvas.call(js.Object, "getContext", .{js.string("2d")});
//         defer ctx.deinit();
//
//         // We need to draw pixels so this is format dependent.
//         const buf: []u8 = switch (self.format) {
//             .bgra => buf: {
//                 // Convert from BGRA to RGBA by swapping every R and B.
//                 var buf: []u8 = try alloc.dupe(u8, self.data);
//                 errdefer alloc.free(buf);
//                 var i: usize = 0;
//                 while (i < self.data.len) : (i += 4) {
//                     std.mem.swap(u8, &buf[i], &buf[i + 2]);
//                 }
//                 break :buf buf;
//             },
//
//             .grayscale => buf: {
//                 // Convert from A8 to RGBA so every 4th byte is set to a value.
//                 var buf: []u8 = try alloc.alloc(u8, self.data.len * 4);
//                 errdefer alloc.free(buf);
//                 @memset(buf, 0);
//                 for (self.data, 0..) |value, i| {
//                     buf[(i * 4) + 3] = value;
//                 }
//                 break :buf buf;
//             },
//
//             else => return error.UnsupportedAtlasFormat,
//         };
//         defer if (buf.ptr != self.data.ptr) alloc.free(buf);
//
//         // Create an ImageData from our buffer and then write it to the canvas
//         const image_data: js.Object = data: {
//             // Get our runtime memory
//             const mem = try js.runtime.get(js.Object, "memory");
//             defer mem.deinit();
//             const mem_buf = try mem.get(js.Object, "buffer");
//             defer mem_buf.deinit();
//
//             // Create an array that points to our buffer
//             const arr = arr: {
//                 const Uint8ClampedArray = try js.global.get(js.Object, "Uint8ClampedArray");
//                 defer Uint8ClampedArray.deinit();
//                 const arr = try Uint8ClampedArray.new(.{ mem_buf, buf.ptr, buf.len });
//                 if (!wasm.shared_mem) break :arr arr;
//
//                 // If we're sharing memory then we have to copy the data since
//                 // we can't set ImageData directly using a SharedArrayBuffer.
//                 defer arr.deinit();
//                 break :arr try arr.call(js.Object, "slice", .{});
//             };
//             defer arr.deinit();
//
//             // Create the image data from our array
//             const ImageData = try js.global.get(js.Object, "ImageData");
//             defer ImageData.deinit();
//             const data = try ImageData.new(.{ arr, self.size, self.size });
//             errdefer data.deinit();
//
//             break :data data;
//         };
//         defer image_data.deinit();
//
//         // Draw it
//         try ctx.call(void, "putImageData", .{ image_data, 0, 0 });
//
//         const id = @as(js.Ref, @bitCast(@intFromEnum(canvas.value))).id;
//         return id;
//     }
//
//     test "happy path" {
//         const atlas = atlas_new(512, @intFromEnum(Format.grayscale)).?;
//         defer atlas_free(atlas);
//
//         const reg = atlas_reserve(atlas, 2, 2).?;
//         defer alloc.destroy(reg);
//         try testing.expect(wasm.isHostOwned(reg));
//         defer wasm.toModuleOwned(reg);
//         try testing.expect(reg.width > 0);
//
//         const data = &[_]u8{ 1, 2, 3, 4 };
//         try testing.expect(!atlas.modified);
//         atlas_set(atlas, reg, data, data.len);
//         try testing.expect(atlas.modified);
//     }
// };

// PORT NOTE: tripwire fail-point enums + thread-local modules + check helpers for
// `init`/`reserve`/`grow`, mirroring the reference `init_tw`/`reserve_tw`/
// `grow_tw`. Only compiled in test builds; the call sites are `#[cfg(test)]`-
// gated so production code never reaches `check` (Zig `enabled = builtin.is_test`).
#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum InitFp {
    AllocData,
    AllocNodes,
}

// std.meta.tags(init_tw.FailPoint)
#[cfg(test)]
const INIT_FP_TAGS: [InitFp; 2] = [InitFp::AllocData, InitFp::AllocNodes];

#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum ReserveFp {
    InsertNode,
}

// std.meta.tags(reserve_tw.FailPoint)
#[cfg(test)]
const RESERVE_FP_TAGS: [ReserveFp; 1] = [ReserveFp::InsertNode];

#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum GrowFp {
    EnsureNodeCapacity,
    AllocData,
}

// std.meta.tags(grow_tw.FailPoint)
#[cfg(test)]
const GROW_FP_TAGS: [GrowFp; 2] = [GrowFp::EnsureNodeCapacity, GrowFp::AllocData];

#[cfg(test)]
thread_local! {
    static INIT_TW: core::cell::RefCell<rustty_util::tripwire::Module<InitFp, OutOfMemory>> =
        core::cell::RefCell::new(rustty_util::tripwire::module());
    static RESERVE_TW: core::cell::RefCell<rustty_util::tripwire::Module<ReserveFp, OutOfMemory>> =
        core::cell::RefCell::new(rustty_util::tripwire::module());
    static GROW_TW: core::cell::RefCell<rustty_util::tripwire::Module<GrowFp, OutOfMemory>> =
        core::cell::RefCell::new(rustty_util::tripwire::module());
}

#[cfg(test)]
#[inline]
fn init_tw_check(point: InitFp) -> Result<(), OutOfMemory> {
    INIT_TW.with(|tw| tw.borrow_mut().check(point))
}

#[cfg(test)]
#[inline]
fn reserve_tw_check(point: ReserveFp) -> Result<(), OutOfMemory> {
    RESERVE_TW.with(|tw| tw.borrow_mut().check(point))
}

#[cfg(test)]
#[inline]
fn grow_tw_check(point: GrowFp) -> Result<(), OutOfMemory> {
    GROW_TW.with(|tw| tw.borrow_mut().check(point))
}

#[cfg(test)]
mod tests {
    use super::*;
    use allocator_api2::alloc::{AllocError, Allocator};
    use core::alloc::Layout;
    use core::cell::Cell;
    use core::ptr::NonNull;
    use rustty_util::tripwire::ResetMode;

    // test "exact fit"
    #[test]
    fn exact_fit() {
        // const alloc = testing.allocator;
        let alloc = Global;
        // var atlas = try init(alloc, 34, .grayscale); // +2 for 1px border
        let mut atlas = Atlas::init(alloc, 34, Format::Grayscale).unwrap(); // +2 for 1px border

        // const modified = atlas.modified.load(.monotonic);
        let modified = atlas.modified.load(Ordering::Relaxed);
        // _ = try atlas.reserve(alloc, 32, 32);
        atlas.reserve(alloc, 32, 32).unwrap();
        // try testing.expectEqual(modified, atlas.modified.load(.monotonic));
        assert_eq!(modified, atlas.modified.load(Ordering::Relaxed));
        // try testing.expectError(Error.AtlasFull, atlas.reserve(alloc, 1, 1));
        assert_eq!(Err(ReserveError::AtlasFull), atlas.reserve(alloc, 1, 1));

        atlas.deinit(alloc);
    }

    // test "doesn't fit"
    #[test]
    fn doesnt_fit() {
        let alloc = Global;
        // var atlas = try init(alloc, 32, .grayscale);
        let mut atlas = Atlas::init(alloc, 32, Format::Grayscale).unwrap();

        // doesn't fit due to border
        // try testing.expectError(Error.AtlasFull, atlas.reserve(alloc, 32, 32));
        assert_eq!(Err(ReserveError::AtlasFull), atlas.reserve(alloc, 32, 32));

        atlas.deinit(alloc);
    }

    // test "fit multiple"
    #[test]
    fn fit_multiple() {
        let alloc = Global;
        let mut atlas = Atlas::init(alloc, 32, Format::Grayscale).unwrap();

        // _ = try atlas.reserve(alloc, 15, 30);
        atlas.reserve(alloc, 15, 30).unwrap();
        // _ = try atlas.reserve(alloc, 15, 30);
        atlas.reserve(alloc, 15, 30).unwrap();
        // try testing.expectError(Error.AtlasFull, atlas.reserve(alloc, 1, 1));
        assert_eq!(Err(ReserveError::AtlasFull), atlas.reserve(alloc, 1, 1));

        atlas.deinit(alloc);
    }

    // test "writing data"
    #[test]
    fn writing_data() {
        let alloc = Global;
        let mut atlas = Atlas::init(alloc, 32, Format::Grayscale).unwrap();

        // const reg = try atlas.reserve(alloc, 2, 2);
        let reg = atlas.reserve(alloc, 2, 2).unwrap();
        // const old = atlas.modified.load(.monotonic);
        let old = atlas.modified.load(Ordering::Relaxed);
        // atlas.set(reg, &[_]u8{ 1, 2, 3, 4 });
        atlas.set(reg, &[1, 2, 3, 4]);
        // const new = atlas.modified.load(.monotonic);
        let new = atlas.modified.load(Ordering::Relaxed);
        // try testing.expect(new > old);
        assert!(new > old);

        // 33 because of the 1px border and so on
        // try testing.expectEqual(@as(u8, 1), atlas.data[33]);
        assert_eq!(1u8, atlas.data[33]);
        assert_eq!(2u8, atlas.data[34]);
        assert_eq!(3u8, atlas.data[65]);
        assert_eq!(4u8, atlas.data[66]);

        atlas.deinit(alloc);
    }

    // test "writing data from a larger source"
    #[test]
    fn writing_data_from_a_larger_source() {
        let alloc = Global;
        let mut atlas = Atlas::init(alloc, 32, Format::Grayscale).unwrap();

        // const reg = try atlas.reserve(alloc, 2, 2);
        let reg = atlas.reserve(alloc, 2, 2).unwrap();
        // const old = atlas.modified.load(.monotonic);
        let old = atlas.modified.load(Ordering::Relaxed);
        // zig fmt: off
        // atlas.setFromLarger(reg, &[_]u8{ ... }, 5, 2, 1);
        atlas.setFromLarger(
            reg,
            &[8, 8, 8, 8, 8, 8, 8, 1, 2, 8, 8, 8, 3, 4, 8, 8, 8, 8, 8, 8],
            5,
            2,
            1,
        );
        // zig fmt: on
        // const new = atlas.modified.load(.monotonic);
        let new = atlas.modified.load(Ordering::Relaxed);
        // try testing.expect(new > old);
        assert!(new > old);

        // 33 because of the 1px border and so on
        assert_eq!(1u8, atlas.data[33]);
        assert_eq!(2u8, atlas.data[34]);
        assert_eq!(3u8, atlas.data[65]);
        assert_eq!(4u8, atlas.data[66]);

        // None of the `8`s from the source data outside of the
        // specified region should have made it on to the atlas.
        // try testing.expectEqual(null, std.mem.indexOfScalar(u8, atlas.data, 8));
        assert_eq!(None::<usize>, atlas.data.iter().position(|&b| b == 8));

        atlas.deinit(alloc);
    }

    // test "grow"
    #[test]
    fn grow() {
        let alloc = Global;
        // var atlas = try init(alloc, 4, .grayscale); // +2 for 1px border
        let mut atlas = Atlas::init(alloc, 4, Format::Grayscale).unwrap(); // +2 for 1px border

        // const reg = try atlas.reserve(alloc, 2, 2);
        let reg = atlas.reserve(alloc, 2, 2).unwrap();
        // try testing.expectError(Error.AtlasFull, atlas.reserve(alloc, 1, 1));
        assert_eq!(Err(ReserveError::AtlasFull), atlas.reserve(alloc, 1, 1));

        // Write some data so we can verify that growing doesn't mess it up
        // atlas.set(reg, &[_]u8{ 1, 2, 3, 4 });
        atlas.set(reg, &[1, 2, 3, 4]);
        assert_eq!(1u8, atlas.data[5]);
        assert_eq!(2u8, atlas.data[6]);
        assert_eq!(3u8, atlas.data[9]);
        assert_eq!(4u8, atlas.data[10]);

        // Expand by exactly 1 should fit our new 1x1 block.
        // const old_modified = atlas.modified.load(.monotonic);
        let old_modified = atlas.modified.load(Ordering::Relaxed);
        // const old_resized = atlas.resized.load(.monotonic);
        let old_resized = atlas.resized.load(Ordering::Relaxed);
        // try atlas.grow(alloc, atlas.size + 1);
        atlas.grow(alloc, atlas.size + 1).unwrap();
        // const new_modified = atlas.modified.load(.monotonic);
        let new_modified = atlas.modified.load(Ordering::Relaxed);
        // const new_resized = atlas.resized.load(.monotonic);
        let new_resized = atlas.resized.load(Ordering::Relaxed);
        // try testing.expect(new_modified > old_modified);
        assert!(new_modified > old_modified);
        // try testing.expect(new_resized > old_resized);
        assert!(new_resized > old_resized);
        // _ = try atlas.reserve(alloc, 1, 1);
        atlas.reserve(alloc, 1, 1).unwrap();

        // Ensure our data is still set. Not the offsets change due to size.
        assert_eq!(1u8, atlas.data[atlas.size as usize + 1]);
        assert_eq!(2u8, atlas.data[atlas.size as usize + 2]);
        assert_eq!(3u8, atlas.data[atlas.size as usize * 2 + 1]);
        assert_eq!(4u8, atlas.data[atlas.size as usize * 2 + 2]);

        atlas.deinit(alloc);
    }

    // test "writing BGR data"
    #[test]
    fn writing_bgr_data() {
        let alloc = Global;
        // var atlas = try init(alloc, 32, .bgr);
        let mut atlas = Atlas::init(alloc, 32, Format::Bgr).unwrap();

        // This is BGR so its 3 bpp
        // const reg = try atlas.reserve(alloc, 1, 2);
        let reg = atlas.reserve(alloc, 1, 2).unwrap();
        // atlas.set(reg, &[_]u8{ 1, 2, 3, 4, 5, 6 });
        atlas.set(reg, &[1, 2, 3, 4, 5, 6]);

        // 33 because of the 1px border and so on
        // const depth = @as(usize, @intCast(atlas.format.depth()));
        let depth = atlas.format.depth() as usize;
        assert_eq!(1u8, atlas.data[33 * depth]);
        assert_eq!(2u8, atlas.data[33 * depth + 1]);
        assert_eq!(3u8, atlas.data[33 * depth + 2]);
        assert_eq!(4u8, atlas.data[65 * depth]);
        assert_eq!(5u8, atlas.data[65 * depth + 1]);
        assert_eq!(6u8, atlas.data[65 * depth + 2]);

        atlas.deinit(alloc);
    }

    // test "grow BGR"
    #[test]
    fn grow_bgr() {
        let alloc = Global;

        // Atlas is 4x4 so its a 1px border meaning we only have 2x2 available
        // var atlas = try init(alloc, 4, .bgr);
        let mut atlas = Atlas::init(alloc, 4, Format::Bgr).unwrap();

        // Get our 2x2, which should be ALL our usable space
        // const reg = try atlas.reserve(alloc, 2, 2);
        let reg = atlas.reserve(alloc, 2, 2).unwrap();
        // try testing.expectError(Error.AtlasFull, atlas.reserve(alloc, 1, 1));
        assert_eq!(Err(ReserveError::AtlasFull), atlas.reserve(alloc, 1, 1));

        // This is BGR so its 3 bpp
        // atlas.set(reg, &[_]u8{ ... });
        atlas.set(
            reg,
            &[
                10, 11, 12, // (0, 0) (x, y) from top-left
                13, 14, 15, // (1, 0)
                20, 21, 22, // (0, 1)
                23, 24, 25, // (1, 1)
            ],
        );

        // Our top left skips the first row (size * depth) and the first
        // column (depth) for the 1px border.
        // const depth = @as(usize, @intCast(atlas.format.depth()));
        let depth = atlas.format.depth() as usize;
        // var tl = (atlas.size * depth) + depth;
        let mut tl = (atlas.size as usize * depth) + depth;
        assert_eq!(10u8, atlas.data[tl]);
        assert_eq!(11u8, atlas.data[tl + 1]);
        assert_eq!(12u8, atlas.data[tl + 2]);
        assert_eq!(13u8, atlas.data[tl + 3]);
        assert_eq!(14u8, atlas.data[tl + 4]);
        assert_eq!(15u8, atlas.data[tl + 5]);
        assert_eq!(0u8, atlas.data[tl + 6]); // border

        // tl += (atlas.size * depth); // next row
        tl += atlas.size as usize * depth; // next row
        assert_eq!(20u8, atlas.data[tl]);
        assert_eq!(21u8, atlas.data[tl + 1]);
        assert_eq!(22u8, atlas.data[tl + 2]);
        assert_eq!(23u8, atlas.data[tl + 3]);
        assert_eq!(24u8, atlas.data[tl + 4]);
        assert_eq!(25u8, atlas.data[tl + 5]);
        assert_eq!(0u8, atlas.data[tl + 6]); // border

        // Expand by exactly 1 should fit our new 1x1 block.
        // try atlas.grow(alloc, atlas.size + 1);
        atlas.grow(alloc, atlas.size + 1).unwrap();

        // Data should be in same place accounting for the new size
        // tl = (atlas.size * depth) + depth;
        tl = (atlas.size as usize * depth) + depth;
        assert_eq!(10u8, atlas.data[tl]);
        assert_eq!(11u8, atlas.data[tl + 1]);
        assert_eq!(12u8, atlas.data[tl + 2]);
        assert_eq!(13u8, atlas.data[tl + 3]);
        assert_eq!(14u8, atlas.data[tl + 4]);
        assert_eq!(15u8, atlas.data[tl + 5]);
        assert_eq!(0u8, atlas.data[tl + 6]); // border

        // tl += (atlas.size * depth); // next row
        tl += atlas.size as usize * depth; // next row
        assert_eq!(20u8, atlas.data[tl]);
        assert_eq!(21u8, atlas.data[tl + 1]);
        assert_eq!(22u8, atlas.data[tl + 2]);
        assert_eq!(23u8, atlas.data[tl + 3]);
        assert_eq!(24u8, atlas.data[tl + 4]);
        assert_eq!(25u8, atlas.data[tl + 5]);
        assert_eq!(0u8, atlas.data[tl + 6]); // border

        // Should fit the new blocks around the edges
        // _ = try atlas.reserve(alloc, 1, 3);
        atlas.reserve(alloc, 1, 3).unwrap();
        // _ = try atlas.reserve(alloc, 2, 1);
        atlas.reserve(alloc, 2, 1).unwrap();
        // try testing.expectError(Error.AtlasFull, atlas.reserve(alloc, 1, 1));
        assert_eq!(Err(ReserveError::AtlasFull), atlas.reserve(alloc, 1, 1));

        atlas.deinit(alloc);
    }

    // test "grow OOM"
    #[test]
    fn grow_oom() {
        // We use a fixed buffer allocator so that we can consistently hit OOM.
        //
        // We calculate the size to exactly fit the 4x4 pixels and node list.
        // var buf: [
        //     4 * 4 * 1 // 4x4 pixels, each 1 byte.
        //     + node_prealloc * @sizeOf(Node) // preallocated nodes.
        // ]u8 = undefined;
        // (the `* 1` mirrors the reference's `4 * 4 * 1`; identity_op is allowed
        // to keep the faithful byte-size arithmetic.)
        // PORT NOTE: the backing buffer is wrapped in a `#[repr(align(4))]` newtype
        // so its base is `Node`-aligned. The Zig FBA aligns the *absolute* returned
        // pointer; with the base already aligned, the `data` (1-byte) then `nodes`
        // (4-byte, at the 16-aligned offset) allocations need zero alignment
        // padding, so the budget fits the data + node list EXACTLY (matching the
        // reference, whose stack buffer is likewise sufficiently aligned).
        let mut buf = AlignedBuf([0u8; FBA_N]);
        // var fba: std.heap.FixedBufferAllocator = .init(&buf);
        // const alloc = fba.allocator();
        let fba = FixedBufferAllocator::init(&mut buf.0);
        let alloc = &fba;

        // var atlas = try init(alloc, 4, .grayscale); // +2 for 1px border
        let mut atlas = Atlas::init(alloc, 4, Format::Grayscale).unwrap(); // +2 for 1px border

        // const reg = try atlas.reserve(alloc, 2, 2);
        let reg = atlas.reserve(alloc, 2, 2).unwrap();
        // try testing.expectError(Error.AtlasFull, atlas.reserve(alloc, 1, 1));
        assert_eq!(Err(ReserveError::AtlasFull), atlas.reserve(alloc, 1, 1));

        // Write some data so we can verify that attempted growing doesn't mess it up.
        // atlas.set(reg, &[_]u8{ 1, 2, 3, 4 });
        atlas.set(reg, &[1, 2, 3, 4]);
        assert_eq!(1u8, atlas.data[5]);
        assert_eq!(2u8, atlas.data[6]);
        assert_eq!(3u8, atlas.data[9]);
        assert_eq!(4u8, atlas.data[10]);

        // Expand by 1, should give OOM, modified and resized should be unchanged.
        // const old_modified = atlas.modified.load(.monotonic);
        let old_modified = atlas.modified.load(Ordering::Relaxed);
        // const old_resized = atlas.resized.load(.monotonic);
        let old_resized = atlas.resized.load(Ordering::Relaxed);
        // try testing.expectError(
        //     Allocator.Error.OutOfMemory,
        //     atlas.grow(alloc, atlas.size + 1),
        // );
        assert_eq!(Err(OutOfMemory), atlas.grow(alloc, atlas.size + 1));
        // const new_modified = atlas.modified.load(.monotonic);
        let new_modified = atlas.modified.load(Ordering::Relaxed);
        // const new_resized = atlas.resized.load(.monotonic);
        let new_resized = atlas.resized.load(Ordering::Relaxed);
        // try testing.expectEqual(old_modified, new_modified);
        assert_eq!(old_modified, new_modified);
        // try testing.expectEqual(old_resized, new_resized);
        assert_eq!(old_resized, new_resized);

        // Ensure our data is still set.
        assert_eq!(1u8, atlas.data[5]);
        assert_eq!(2u8, atlas.data[6]);
        assert_eq!(3u8, atlas.data[9]);
        assert_eq!(4u8, atlas.data[10]);

        atlas.deinit(alloc);
    }

    // test "init error"
    #[test]
    fn init_error() {
        // Test every failure point in `init` and ensure that we don't
        // leak memory (testing.allocator verifies) since we're exiting early.
        // for (std.meta.tags(init_tw.FailPoint)) |tag| {
        for tag in INIT_FP_TAGS {
            // const tw = init_tw;
            // defer tw.end(.reset) catch unreachable;
            // tw.errorAlways(tag, error.OutOfMemory);
            INIT_TW.with(|tw| tw.borrow_mut().errorAlways(tag, OutOfMemory));
            // try testing.expectError(
            //     error.OutOfMemory,
            //     init(testing.allocator, 32, .grayscale),
            // );
            //
            // PORT NOTE: `Atlas` (atomics + `Vec`s) is not `PartialEq`/`Debug`, so
            // we compare on `.err()` (`Option<OutOfMemory>`) — the Ok atlas, if
            // any, is dropped — mirroring `expectError`'s error-only check.
            assert_eq!(
                Some(OutOfMemory),
                Atlas::init(Global, 32, Format::Grayscale).err()
            );
            // defer tw.end(.reset) catch unreachable;
            INIT_TW.with(|tw| tw.borrow_mut().end(ResetMode::Reset).unwrap());
        }
    }

    // test "reserve error"
    #[test]
    fn reserve_error() {
        // Test every failure point in `reserve` and ensure that we don't
        // leak memory (testing.allocator verifies) since we're exiting early.
        // for (std.meta.tags(reserve_tw.FailPoint)) |tag| {
        for tag in RESERVE_FP_TAGS {
            // const tw = reserve_tw;

            // var atlas = try init(testing.allocator, 32, .grayscale);
            let mut atlas = Atlas::init(Global, 32, Format::Grayscale).unwrap();

            // tw.errorAlways(tag, error.OutOfMemory);
            RESERVE_TW.with(|tw| tw.borrow_mut().errorAlways(tag, OutOfMemory));
            // try testing.expectError(
            //     error.OutOfMemory,
            //     atlas.reserve(testing.allocator, 2, 2),
            // );
            assert_eq!(Err(ReserveError::OutOfMemory), atlas.reserve(Global, 2, 2));
            // defer tw.end(.reset) catch unreachable;
            RESERVE_TW.with(|tw| tw.borrow_mut().end(ResetMode::Reset).unwrap());

            atlas.deinit(Global);
        }
    }

    // test "grow error"
    #[test]
    fn grow_error() {
        // Test every failure point in `grow` and ensure that we don't
        // leak memory (testing.allocator verifies) since we're exiting early.
        // for (std.meta.tags(grow_tw.FailPoint)) |tag| {
        for tag in GROW_FP_TAGS {
            // const tw = grow_tw;

            // var atlas = try init(testing.allocator, 4, .grayscale);
            let mut atlas = Atlas::init(Global, 4, Format::Grayscale).unwrap();

            // Write some data to verify it's preserved after failed grow
            // const reg = try atlas.reserve(testing.allocator, 2, 2);
            let reg = atlas.reserve(Global, 2, 2).unwrap();
            // atlas.set(reg, &[_]u8{ 1, 2, 3, 4 });
            atlas.set(reg, &[1, 2, 3, 4]);

            // const old_modified = atlas.modified.load(.monotonic);
            let old_modified = atlas.modified.load(Ordering::Relaxed);
            // const old_resized = atlas.resized.load(.monotonic);
            let old_resized = atlas.resized.load(Ordering::Relaxed);

            // tw.errorAlways(tag, error.OutOfMemory);
            GROW_TW.with(|tw| tw.borrow_mut().errorAlways(tag, OutOfMemory));
            // try testing.expectError(
            //     error.OutOfMemory,
            //     atlas.grow(testing.allocator, atlas.size + 1),
            // );
            assert_eq!(Err(OutOfMemory), atlas.grow(Global, atlas.size + 1));

            // Verify atlas state is unchanged after failed grow
            // try testing.expectEqual(old_modified, atlas.modified.load(.monotonic));
            assert_eq!(old_modified, atlas.modified.load(Ordering::Relaxed));
            // try testing.expectEqual(old_resized, atlas.resized.load(.monotonic));
            assert_eq!(old_resized, atlas.resized.load(Ordering::Relaxed));
            assert_eq!(1u8, atlas.data[5]);
            assert_eq!(2u8, atlas.data[6]);
            assert_eq!(3u8, atlas.data[9]);
            assert_eq!(4u8, atlas.data[10]);

            // defer tw.end(.reset) catch unreachable;
            GROW_TW.with(|tw| tw.borrow_mut().end(ResetMode::Reset).unwrap());

            atlas.deinit(Global);
        }
    }

    // size to exactly fit the 4x4 pixels and node list (see `grow_oom`).
    #[allow(clippy::identity_op)]
    const FBA_N: usize = 4 * 4 * 1 // 4x4 pixels, each 1 byte.
        + NODE_PREALLOC * core::mem::size_of::<Node>(); // preallocated nodes.

    // `Node`-aligned (4-byte) backing buffer for the FBA (see `grow_oom`).
    #[repr(align(4))]
    struct AlignedBuf([u8; FBA_N]);

    // PORT NOTE: a minimal bump allocator over a fixed buffer, mirroring
    // `std.heap.FixedBufferAllocator`, used by the "grow OOM" test to deterministically
    // hit a REAL allocation failure (distinct from the tripwire-injected failures).
    // It implements `allocator_api2::alloc::Allocator`; the test threads it as a
    // shared `&FixedBufferAllocator` (which is `Copy` and `Allocator`). `deallocate`
    // is a no-op (bump allocator), matching the reference FBA used by this test.
    struct FixedBufferAllocator {
        buf: *mut u8,
        len: usize,
        end_index: Cell<usize>,
    }

    impl FixedBufferAllocator {
        fn init(buf: &mut [u8]) -> FixedBufferAllocator {
            FixedBufferAllocator {
                buf: buf.as_mut_ptr(),
                len: buf.len(),
                end_index: Cell::new(0),
            }
        }
    }

    unsafe impl Allocator for FixedBufferAllocator {
        fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
            // Mirrors `std.heap.FixedBufferAllocator`: align the ABSOLUTE returned
            // pointer (not just the in-buffer offset), so the returned block meets
            // `layout.align()`.
            let off = self.end_index.get();
            let align = layout.align();
            let base = self.buf as usize;
            let cur = base + off;
            let aligned_addr = (cur + align - 1) & !(align - 1);
            let adjust = aligned_addr - cur;
            let new_end = off + adjust + layout.size();
            if new_end > self.len {
                return Err(AllocError);
            }
            self.end_index.set(new_end);
            // SAFETY: `off + adjust + layout.size() = new_end <= self.len`, so
            // `buf + (off + adjust)` and the `layout.size()` bytes following it are
            // all within the backing buffer; the offset derives from `self.buf`,
            // preserving provenance.
            let ptr = unsafe { self.buf.add(off + adjust) };
            let nn = NonNull::new(ptr).ok_or(AllocError)?;
            Ok(NonNull::slice_from_raw_parts(nn, layout.size()))
        }

        unsafe fn deallocate(&self, _ptr: NonNull<u8>, _layout: Layout) {
            // bump allocator: free is a no-op.
        }
    }
}
