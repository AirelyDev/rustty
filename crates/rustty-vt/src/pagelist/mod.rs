//! Maintains a linked list of pages to make up a terminal screen
//! and provides higher level operations on top of those pages to
//! make it slightly easier to work with.
//!
//! PORTS: ghostty src/terminal/PageList.zig
//!
//! PORT STATUS: stub.
//!
//! This module is the verbatim-retargeted decomposition of the reference
//! `terminal/PageList.zig` (a single 14803-line file) into a `pagelist/`
//! module dir. `mod.rs` owns the `PageList` struct definition (ALL fields,
//! same layout/order), the internal `Node`/page-pool/list types it owns, the
//! `Viewport` union, and the CORE lifecycle (`init`, `deinit`, `reset`) plus
//! the lowest-level private page-management helpers that every cluster needs.
//! The remaining method clusters are split across sibling sub-files, each an
//! `impl PageList {}` carrying that cluster's owner doc headers:
//!
//!   - grow.rs   : grow / increaseCapacity / compact / scrollbar / maxSize
//!   - scroll.rs : scroll / scrollPrompt / scrollClear
//!   - resize.rs : resize / resizeCols / resizeWithoutReflow / reflow
//!   - erase.rs  : eraseRow / eraseRowBounded / eraseHistory / eraseActive
//!   - pin.rs    : pins + tracking (pin / trackPin / untrackPin / Pin)
//!   - iter.rs   : prompt / cell / row / page iterators
//!   - access.rs : getCell / getTopLeft / getBottomRight / totalPages /
//!     dirty / clone / split / diagram / highlightSemanticContent / cval / eql
//!
//! PORT NOTE (vt-3c, this batch): the CORE methods are ported on top of the
//! `rustty_zigstd` faithful Zig-stdlib equivalents. The Node pool is
//! `rustty_zigstd::MemoryPool<Node>`, the Pin pool is
//! `rustty_zigstd::MemoryPool<Pin>`, and the tracked-pin set is
//! `rustty_zigstd::AutoArrayHashMapUnmanaged<*mut Pin, ()>`. The PAGE BUFFER
//! pool is hand-rolled below (`PagePool`) as the faithful behavioral
//! equivalent of the reference `MemoryPoolAligned([std_size]u8, page_align)` +
//! `arena.child_allocator` usage (see the `PagePool` PORT NOTE). The 205 ported
//! `test {}` blocks land with their owning clusters (a dedicated tests phase).

// const PageList = @This();

// const std = @import("std");
// const builtin = @import("builtin");
// const build_options = @import("terminal_options");
// const Allocator = std.mem.Allocator;
// const assert = @import("../quirks.zig").inlineAssert;
// const fastmem = @import("../fastmem.zig");
// const tripwire = @import("../tripwire.zig");
// const DoublyLinkedList = @import("../datastruct/main.zig").IntrusiveDoublyLinkedList;
// const color = @import("color.zig");
// const highlight = @import("highlight.zig");
// const kitty = @import("kitty.zig");
// const point = @import("point.zig");
// const pagepkg = @import("page.zig");
// const stylepkg = @import("style.zig");
// const size = @import("size.zig");
// const OffsetBuf = size.OffsetBuf;
// const Capacity = pagepkg.Capacity;
// const Page = pagepkg.Page;
// const Row = pagepkg.Row;
//
// PORT NOTE (R2/R3): retargeted imports. `terminal_options` build options live
// in this crate as `crate::options`; `size` is hoisted into
// `rustty_render_types::term_size` (ambiguity A1); `page`/`style`/`color`/
// `highlight`/`kitty`/`point` are sibling modules of this crate; the intrusive
// doubly-linked list comes from `rustty-datastruct`. The `MemoryPool` /
// `MemoryPoolAligned` / `AutoArrayHashMapUnmanaged` Zig-stdlib equivalents come
// from `rustty-zigstd` (additive infra glue, R1).
// Cluster sub-files (each an `impl PageList {}` carrying that cluster's owner
// doc headers). See the module doc comment above for the cluster map.
pub mod access;
pub mod erase;
pub mod grow;
pub mod iter;
pub mod pin;
pub mod resize;
pub mod scroll;

use rustty_datastruct::IntrusiveDoublyLinkedList as DoublyLinkedList;
use rustty_render_types::term_size as size;
use rustty_render_types::term_size::OffsetBuf;
use rustty_zigstd::array_hash_map::AutoArrayHashMapUnmanaged;
use rustty_zigstd::memory_pool::MemoryPool as ZigMemoryPool;

use core::ptr::NonNull;

use crate::page as pagepkg;
use crate::page::{Capacity, Page, Row};
use crate::point;

// const log = std.log.scoped(.page_list);

/// The number of PageList.Nodes we preheat the pool with. A node is
/// a very small struct so we can afford to preheat many, but the exact
/// number is uncertain. Any number too large is wasting memory, any number
/// too small will cause the pool to have to allocate more memory later.
/// This should be set to some reasonable minimum that we expect a terminal
/// window to scroll into quickly.
const PAGE_PREHEAT: usize = 4;

/// The list of pages in the screen. These are expected to be in order
/// where the first page is the topmost page (scrollback) and the last is
/// the bottommost page (the current active page).
///
/// PORT NOTE: the reference `pub const List = DoublyLinkedList(Node);` makes
/// the node type `Node` itself (intrusive). In Rust the list is
/// `DoublyLinkedList<Node>` where `Node` implements the
/// `rustty_datastruct::Node` trait (the embedded `prev`/`next` raw pointers).
pub type List = DoublyLinkedList<Node>;

/// A single node within the PageList linked list.
///
/// This isn't pub because you can access the type via List.Node.
//
// PORT NOTE: the reference `Node` carries its own intrusive `prev`/`next`
// pointers (a `DoublyLinkedList(Node)` node). We keep them as nullable raw
// pointers and expose them via the `rustty_datastruct::Node` trait so the list
// can splice without owning the node (callers/pools own the storage — the same
// raw self-referential memory model as the reference).
pub struct Node {
    pub(crate) prev: *mut Node,
    pub(crate) next: *mut Node,
    pub data: Page,
    pub serial: u64,
}

impl rustty_datastruct::intrusive_linked_list::Node for Node {
    #[inline]
    fn prev(&self) -> *mut Self {
        self.prev
    }
    #[inline]
    fn next(&self) -> *mut Self {
        self.next
    }
    #[inline]
    fn set_prev(&mut self, p: *mut Self) {
        self.prev = p;
    }
    #[inline]
    fn set_next(&mut self, n: *mut Self) {
        self.next = n;
    }
}

/// The memory pool we get page nodes from.
//
// PORT NOTE: the reference `NodePool = std.heap.MemoryPool(List.Node)`. We use
// the faithful `rustty_zigstd::MemoryPool<Node>` (LIFO free-list reuse).
type NodePool = ZigMemoryPool<Node>;

// /// The standard page capacity that we use as a starting point for
// /// all pages. This is chosen as a sane default that fits most terminal
// /// usage to support using our pool.
// const std_capacity = pagepkg.std_capacity;
//
// PORT NOTE: `pagepkg.std_capacity` is a value in Rust (`pagepkg::std_capacity()`).
#[inline]
fn std_capacity() -> Capacity {
    pagepkg::std_capacity()
}

/// The byte size required for a standard page.
//
// PORT NOTE: the reference `std_size = Page.layout(std_capacity).total_size` is
// a comptime constant. In Rust the layout is computed at runtime (cheap), so we
// expose it as a function.
#[inline]
fn std_size() -> usize {
    Page::layout(std_capacity()).total_size
}

/// `std.heap.page_size_min`. On all targets we care about this is 4096; the
/// page buffer pool aligns standard and non-standard page buffers to this.
//
// PORT NOTE: matches the `page.rs` `PAGE_SIZE_MIN` (private to that module);
// duplicated here so the page-buffer pool can align allocations.
const PAGE_SIZE_MIN: usize = 4096;

/// The memory pool we use for page memory buffers. We use a separate pool
/// so we can allocate these with a page allocator. We have to use a page
/// allocator because we need memory that is zero-initialized and page-aligned.
//
// PORT NOTE (RULES, vt-3c): the reference is
// `PagePool = std.heap.MemoryPoolAligned([std_size]u8, .fromByteUnits(page_size_min))`
// backed by `arena.child_allocator` (the page allocator). zigstd's
// `MemoryPoolAligned` does NOT expose the internals PageList needs
// (`pages.arena.child_allocator`, `pages.reset(.retain_with_limit)` +
// BufNode-zeroing), so per the port RULES we implement the page-buffer
// management DIRECTLY here as the faithful behavioral equivalent:
//
//   * STANDARD pages (`layout.total_size <= std_size`) draw a `std_size`,
//     `page_size_min`-aligned buffer from a LIFO free-list (`std::alloc` with a
//     `Layout` aligned to `page_size_min`); `destroy` zeroes the buffer and
//     returns it to the free-list for reuse.
//   * NON-STANDARD pages (`layout.total_size > std_size`) are allocated
//     directly page-aligned (the reference `page_alloc.alignedAlloc`) and freed
//     individually (the reference `page_alloc.free`). These never enter the
//     free-list and PageList frees them explicitly in `deinit`/`reset`/`clone`
//     before touching the pool — this struct only owns the standard buffers.
//   * `reset(retain_with_limit)` retains free-list buffers up to the limit and
//     FREES the rest, then zeroes every retained buffer (the reference
//     `@memset(0)` over retained mmap arena memory, because retained memory is
//     not re-`mmap`ed and so is not implicitly zeroed).
//   * `item_size` is `std_size` (the reference `PagePool.item_size`).
//
// All buffers are page-aligned, so handing them to `OffsetBuf::init` yields the
// same `[]align(page_size_min) u8` the reference relies on.
struct PagePool {
    /// The LIFO free-list of standard-size, page-aligned buffers available for
    /// reuse. Each pointer addresses a `std_size`-byte, `page_size_min`-aligned
    /// allocation owned by this pool.
    free_list: Vec<NonNull<u8>>,

    /// The total number of standard buffers this pool has ever allocated and
    /// still owns (free-listed + handed-out). Used to honor `retain_with_limit`
    /// during `reset` so we free as much as possible while keeping the retained
    /// capacity, mirroring the reference arena's `retain_with_limit`.
    live: usize,
}

impl PagePool {
    /// `PagePool.item_size`: the byte size of one standard page buffer.
    #[inline]
    fn item_size() -> usize {
        std_size()
    }

    /// `Layout` for one standard, page-aligned buffer.
    #[inline]
    fn std_layout() -> core::alloc::Layout {
        // `std_size` is a multiple of `page_size_min` (Page.layout rounds up),
        // so this is always a valid layout.
        core::alloc::Layout::from_size_align(std_size(), PAGE_SIZE_MIN).unwrap()
    }

    /// `PagePool.initPreheated`: preheat `preheat` standard buffers onto the
    /// free-list. The reference's `mmap`-backed pool hands back zeroed memory;
    /// we zero each preheated buffer to match.
    fn init_preheated(preheat: usize) -> PagePool {
        let mut pool = PagePool {
            free_list: Vec::new(),
            live: 0,
        };
        let mut i: usize = 0;
        while i < preheat {
            let buf = pool.alloc_std();
            pool.free_list.push(buf);
            i += 1;
        }
        pool
    }

    /// Allocate one fresh `std_size`, page-aligned, zeroed buffer and account
    /// for it in `live`.
    fn alloc_std(&mut self) -> NonNull<u8> {
        let layout = Self::std_layout();
        // `alloc_zeroed`: the reference's mmap'd page memory is always zeroed.
        let ptr = unsafe { std::alloc::alloc_zeroed(layout) };
        let nn = NonNull::new(ptr).unwrap_or_else(|| std::alloc::handle_alloc_error(layout));
        self.live += 1;
        nn
    }

    /// `pages.create()`: hand back a standard buffer, reusing a free-listed one
    /// (LIFO) if available, otherwise allocating fresh. Returned memory is
    /// page-aligned and zeroed.
    fn create(&mut self) -> NonNull<u8> {
        if let Some(buf) = self.free_list.pop() {
            return buf;
        }
        self.alloc_std()
    }

    /// `pages.destroy(ptr)`: return a standard buffer to the free-list for
    /// reuse. The buffer is left as-is here; PageList's `destroyNodeExt`
    /// zeroes the page memory before calling this (the reference
    /// `@memset(page.memory, 0)`), matching observable reuse semantics.
    //
    // PORT NOTE: `#[allow(dead_code)]` — called by `destroy_node_ext`, whose
    // only callers (erase/grow/resize clusters) land in the Methods phase.
    #[allow(dead_code)]
    fn destroy(&mut self, ptr: NonNull<u8>) {
        self.free_list.push(ptr);
    }

    /// `pages.reset(.retain_with_limit(limit_bytes))`: retain free-list buffers
    /// totalling at most `limit_bytes`, freeing the rest, then zero every
    /// retained buffer (the reference zeroes the retained arena memory because
    /// it is not re-`mmap`ed). Handed-out buffers are NOT tracked here (the
    /// caller frees non-standard pages and stops referencing standard pages
    /// before reset, exactly as the reference does).
    //
    // PORT NOTE: the reference arena `retain_with_limit` shrinks the arena to
    // `limit_bytes` and zeroes the surviving buffer. We reproduce that on the
    // free-list: keep `floor(limit_bytes / item_size)` buffers (capped at how
    // many we hold), free the remainder, and `@memset(0)` the survivors.
    fn reset_retain_with_limit(&mut self, limit_bytes: usize) {
        let item_size = Self::item_size();
        let keep = if item_size == 0 {
            self.free_list.len()
        } else {
            core::cmp::min(limit_bytes / item_size, self.free_list.len())
        };

        let layout = Self::std_layout();
        // Free the buffers beyond the retained limit.
        while self.free_list.len() > keep {
            let buf = self.free_list.pop().unwrap();
            unsafe { std::alloc::dealloc(buf.as_ptr(), layout) };
            self.live -= 1;
        }

        // Our page pool relies on mmap to zero our page memory. Since we're
        // retaining a certain amount of memory, it won't use mmap and won't
        // be zeroed. Zero out all the retained buffers.
        for buf in &self.free_list {
            unsafe {
                core::ptr::write_bytes(buf.as_ptr(), 0, std_size());
            }
        }
    }
}

impl Drop for PagePool {
    fn drop(&mut self) {
        // `pages.deinit()`: free every standard buffer the pool still owns
        // (only the free-listed ones remain owned by the pool at deinit; any
        // handed-out standard buffers were returned via `destroy` during
        // PageList teardown, and non-standard buffers are freed by the caller).
        let layout = Self::std_layout();
        for buf in self.free_list.drain(..) {
            unsafe { std::alloc::dealloc(buf.as_ptr(), layout) };
        }
    }
}

/// List of pins, known as "tracked" pins. These are pins that are kept
/// up to date automatically through page-modifying operations.
//
// PORT NOTE: the reference `PinSet = std.AutoArrayHashMapUnmanaged(*Pin, void)`
// ports to `rustty_zigstd::AutoArrayHashMapUnmanaged<*mut Pin, ()>` (insertion-
// order iteration, `putNoClobber`/`keys`/`swapRemove`). It is "Unmanaged" in
// name only — the Rust port owns its backing `Vec`, so there is no
// `deinit(alloc)` call; dropping the map frees its storage.
pub type PinSet = AutoArrayHashMapUnmanaged<*mut Pin, ()>;
/// The memory pool we get pins from.
//
// PORT NOTE: the reference `PinPool = std.heap.MemoryPool(Pin)`.
type PinPool = ZigMemoryPool<Pin>;

/// The pool of memory used for a pagelist. This can be shared between
/// multiple pagelists but it is not threadsafe.
pub struct MemoryPool {
    // alloc: Allocator,
    //
    // PORT NOTE: the reference stores the general-purpose allocator (`alloc`)
    // used for the node/pin pools and the tracked-pin hashmap, plus the page
    // allocator is captured inside `pages.arena.child_allocator`. The
    // `rustty_zigstd` pools and the hand-rolled `PagePool` are all backed by the
    // Rust system allocator (`std::alloc`), the faithful map of Zig's
    // `gpa`/`page_allocator`, so no allocator handle is stored.
    nodes: NodePool,
    pages: PagePool,
    pins: PinPool,
}

impl MemoryPool {
    // pub const ResetMode = std.heap.ArenaAllocator.ResetMode;

    // /// This is the page allocator we'll use for all our underlying
    // /// VM page allocations.
    // inline fn pageAllocator() Allocator {
    //     // In tests we use our testing allocator so we can detect leaks.
    //     if (builtin.is_test) return std.testing.allocator;
    //
    //     // On non-macOS we use our standard Zig page allocator.
    //     if (!builtin.target.os.tag.isDarwin()) return std.heap.page_allocator;
    //
    //     // On macOS we want to tag our memory so we can assign it to our
    //     // core terminal usage.
    //     const mach = @import("../os/mach.zig");
    //     return mach.taggedPageAllocator(.application_specific_1);
    // }
    //
    // PORT NOTE: the reference `pageAllocator()` selects the VM page allocator
    // for the page pool: the testing allocator under test (leak detection), the
    // std page allocator on non-macOS, and the mach tagged page allocator on
    // macOS. In the Rust port the page buffers come from the hand-rolled
    // `PagePool` backed by `std::alloc` (page-aligned `alloc_zeroed`), so there
    // is no separate page-allocator selection; the Miri/leak gate (R6) replaces
    // the testing-allocator leak detection.

    pub fn init(preheat: usize) -> Result<MemoryPool, OutOfMemory> {
        let node_pool = NodePool::init_preheated(preheat).map_err(|_| OutOfMemory)?;
        // errdefer node_pool.deinit();
        let page_pool = PagePool::init_preheated(preheat);
        // errdefer page_pool.deinit();
        let pin_pool = PinPool::init_preheated(8).map_err(|_| OutOfMemory)?;
        // errdefer pin_pool.deinit();
        Ok(MemoryPool {
            nodes: node_pool,
            pages: page_pool,
            pins: pin_pool,
        })
    }

    // pub fn deinit(self: *MemoryPool) void {
    //     self.pages.deinit();
    //     self.nodes.deinit();
    //     self.pins.deinit();
    // }
    //
    // PORT NOTE: `MemoryPool::deinit` is the `Drop` of each sub-pool; dropping
    // the `MemoryPool` runs `pages`/`nodes`/`pins` destructors (the reference
    // frees `pages` first, then `nodes`, then `pins` — Rust drops fields in
    // declaration order, matching that ordering).

    /// `MemoryPool.reset`: reset all sub-pools. Only the `retain_with_limit`
    /// mode is used by PageList (`reset`), so we model it directly with the
    /// per-pool retained byte limits the reference passes.
    //
    // PORT NOTE: the reference `reset(mode)` forwards a single
    // `std.heap.ArenaAllocator.ResetMode` to every sub-pool. PageList's only
    // caller (`reset`) uses `.retain_with_limit` with a DIFFERENT byte limit per
    // pool (`page_count * PagePool.item_size` for pages,
    // `page_count * NodePool.item_size` for nodes). We therefore expose the
    // per-pool reset at the call site instead of a single mode, preserving the
    // exact observable behavior.
    fn reset_pages_retain_with_limit(&mut self, limit_bytes: usize) {
        self.pages.reset_retain_with_limit(limit_bytes);
    }

    fn reset_nodes_retain_with_limit(&mut self, limit_bytes: usize) {
        use rustty_zigstd::arena_allocator::ResetMode;
        self.nodes.reset(ResetMode::RetainWithLimit(limit_bytes));
    }
}

/// The viewport location.
//
// PORT NOTE: the reference `Viewport` is a `union(enum)` with no payloads on
// any variant (the `.pin` variant deliberately carries no value), so it ports
// to a plain Rust enum.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Viewport {
    /// The viewport is pinned to the active area. By using a specific marker
    /// for this instead of tracking the row offset, we eliminate a number of
    /// memory writes making scrolling faster.
    Active,

    /// The viewport is pinned to the top of the screen, or the farthest
    /// back in the scrollback history.
    Top,

    /// The viewport is pinned to a tracked pin. The tracked pin is ALWAYS
    /// s.viewport_pin hence this has no value. We force that value to prevent
    /// allocations.
    Pin,
}

/// Maintains a linked list of pages to make up a terminal screen
/// and provides higher level operations on top of those pages to
/// make it slightly easier to work with.
pub struct PageList {
    /// The memory pool we get page nodes, pages from.
    pub pool: MemoryPool,

    /// The list of pages in the screen.
    pub pages: List,

    /// A monotonically increasing serial number that is incremented each
    /// time a page is allocated or reused as new. The serial is assigned to
    /// the Node.
    ///
    /// The serial number can be used to detect whether the page is identical
    /// to the page that was originally referenced by a pointer. Since we reuse
    /// and pool memory, pointer stability is not guaranteed, but the serial
    /// will always be different for different allocations.
    ///
    /// Developer note: we never do overflow checking on this. If we created
    /// a new page every second it'd take 584 billion years to overflow. We're
    /// going to risk it.
    pub page_serial: u64,

    /// The lowest still valid serial number that could exist. This allows
    /// for quick comparisons to find invalid pages in references.
    pub page_serial_min: u64,

    /// Byte size of the total amount of allocated pages. Note this does
    /// not include the total allocated amount in the pool which may be more
    /// than this due to preheating.
    pub page_size: usize,

    /// Maximum size of the page allocation in bytes. This only includes pages
    /// that are used ONLY for scrollback. If the active area is still partially
    /// in a page that also includes scrollback, then that page is not included.
    pub explicit_max_size: usize,

    /// This is the minimum max size that we will respect due to the rows/cols
    /// of the PageList. We must always be able to fit at least the active area
    /// and at least two pages for our algorithms.
    pub min_max_size: usize,

    /// The total number of rows represented by this PageList. This is used
    /// specifically for scrollbar information so we can have the total size.
    pub total_rows: usize,

    /// The list of tracked pins. These are kept up to date automatically.
    pub tracked_pins: PinSet,

    /// The top-left of certain parts of the screen that are frequently
    /// accessed so we don't have to traverse the linked list to find them.
    ///
    /// For other tags, don't need this:
    ///   - screen: pages.first
    ///   - history: active row minus one
    ///
    pub viewport: Viewport,

    /// The pin used for when the viewport scrolls. This is always pre-allocated
    /// so that scrolling doesn't have a failable memory allocation. This should
    /// never be access directly; use `viewport`.
    pub viewport_pin: *mut Pin,

    /// The row offset from the top that the viewport pin is at. We
    /// store the offset from the top because it doesn't change while more
    /// data is printed to the terminal.
    ///
    /// This is null when it isn't calculated. It is calculated on demand
    /// when the viewportRowOffset function is called, because it is only
    /// required for certain operations such as rendering the scrollbar.
    ///
    /// In order to make this more efficient, in many places where the value
    /// would be invalidated, we update it in-place instead. This is key to
    /// keeping our performance decent in normal cases since recalculating
    /// this from scratch, depending on the size of the scrollback and position
    /// of the pin, can be very expensive.
    ///
    /// This is only valid if viewport is `pin`. Every other offset is
    /// self-evident or quick to calculate.
    pub viewport_pin_row_offset: Option<usize>,

    /// The current desired screen dimensions. I say "desired" because individual
    /// pages may still be a different size and not yet reflowed since we lazily
    /// reflow text.
    pub cols: size::CellCountInt,
    pub rows: size::CellCountInt,

    /// If this is true then verifyIntegrity will do nothing. This is
    /// only present with runtime safety enabled.
    //
    // PORT NOTE: the reference gates this field behind
    // `build_options.slow_runtime_safety` (`usize` when on, `void` when off).
    // We always carry a `usize` and gate its use with the slow-runtime-safety
    // build option, matching the page.rs port convention.
    pub pause_integrity_checks: usize,
}

/// A pin into the PageList. See `pin.rs` for the full definition.
//
// PORT NOTE: the full `Pin` body (with `rowAndCell`, `cells`, `isBetween`,
// `before`, `leftWrap`, `rightWrap`, `downOverflow`, `upOverflow`, etc.) lands
// in `pin.rs` (the pin cluster). For the CORE methods we need the field layout
// (`node`/`y`/`x`/`garbage`) and the `pinIsValid` walk; both are defined here.
pub struct Pin {
    pub node: *mut Node,
    pub y: size::CellCountInt,
    pub x: size::CellCountInt,

    /// This is flipped to true for tracked pins that were tracking
    /// a page that got pruned for any reason and where the tracked pin
    /// couldn't be moved to a sensical location. Users of the tracked
    /// pin could use this data and make their own determination of
    /// semantics.
    pub garbage: bool,
}

/// The allocation-failure error, mirroring Zig's `Allocator.Error`/OutOfMemory.
//
// PORT NOTE: the reference functions return `Allocator.Error!T`; in Rust this
// is `Result<T, OutOfMemory>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutOfMemory;

impl PageList {
    /// Returns the minimum valid "max size" for a given number of rows and cols
    /// such that we can fit the active area AND at least two pages. Note we
    /// need the two pages for algorithms to work properly (such as grow) but
    /// we don't need to fit double the active area.
    ///
    /// This min size may not be totally correct in the case that a large
    /// number of other dimensions makes our row size in a page very small.
    /// But this gives us a nice fast heuristic for determining min/max size.
    /// Therefore, if the page size is violated you should always also verify
    /// that we have enough space for the active area.
    fn min_max_size(cols: size::CellCountInt, rows: size::CellCountInt) -> usize {
        // Invariant required to ensure our divCeil below cannot overflow.
        // comptime {
        //     const max_rows = std.math.maxInt(size.CellCountInt);
        //     _ = std.math.divCeil(usize, max_rows, 1) catch unreachable;
        // }
        //
        // PORT NOTE: the reference comptime block proves `divCeil(max_rows, 1)`
        // can't overflow. The Rust `div_ceil` below operates on `usize` and the
        // divisor is `cap.rows` (>= 1, guaranteed by `initial_capacity`), so it
        // cannot overflow.

        // Get our capacity to fit our rows. If the cols are too big, it may
        // force less rows than we want meaning we need more than one page to
        // represent a viewport.
        let cap = Self::initial_capacity(cols);

        // Calculate the number of standard sized pages we need to represent
        // an active area.
        let pages_exact: usize = if (cap.rows as usize) >= (rows as usize) {
            1
        } else {
            // Not possible to overflow/underflow:
            // - initialCapacity guarantees at least 1 row
            // - numerator/denominator can't overflow because of comptime check above
            (rows as usize).div_ceil(cap.rows as usize)
        };

        // We always need at least one page extra so that we
        // can fit partial pages to spread our active area across two pages.
        // Even for caps that can't fit all rows in a single page, we add one
        // because the most extra space we need at any given time is only
        // the partial amount of one page.
        let pages = pages_exact + 1;
        assert!(pages >= 2);

        // log.debug("minMaxSize cols={} rows={} cap={} pages={}", .{
        //     cols,
        //     rows,
        //     cap,
        //     pages,
        // });

        PagePool::item_size() * pages
    }

    /// Calculates the initial capacity for a new page for a given column
    /// count. This will attempt to fit within std_size at all times so we
    /// can use our memory pool, but if cols is too big, this will return a
    /// larger capacity.
    ///
    /// The returned capacity is always guaranteed to layout properly (not
    /// overflow). We are able to support capacities up to the maximum int
    /// value of cols, so this will never overflow.
    fn initial_capacity(cols: size::CellCountInt) -> Capacity {
        // This is an important invariant that ensures that this function
        // can never return an error. We verify here that our standard capacity
        // when increased to maximum possible columns can always support at
        // least one row in memory.
        //
        // IF THIS EVER FAILS: We probably need to modify our logic below
        // to reduce other elements of the capacity (styles, graphemes, etc.).
        // But, instead, I recommend taking a step back and re-evaluating
        // life choices.
        //
        // PORT NOTE: the reference proves this in a `comptime` block; we keep it
        // as a debug-time assertion of the same invariant (the layout of
        // std_capacity at max cols fits within `size::MAX_PAGE_SIZE`).
        if cfg!(debug_assertions) {
            let mut cap = std_capacity();
            cap.cols = size::CellCountInt::MAX;
            let layout = Page::layout(cap);
            assert!(layout.total_size <= size::MAX_PAGE_SIZE as usize);
        }

        match std_capacity().adjust(pagepkg::Adjustment { cols: Some(cols) }) {
            Ok(cap) => {
                // If we can adjust our standard capacity, we fit within the
                // standard size and we're good!
                return cap;
            }
            Err(_err) => {
                // Ensure our error set doesn't change.
                // comptime assert(@TypeOf(err) == error{OutOfMemory});
                //
                // PORT NOTE: `Capacity::adjust` returns `pagepkg::OutOfMemory`,
                // the sole error case — the reference's `error{OutOfMemory}`.
            }
        }

        // This code path means that our standard capacity can't even
        // accommodate our column count! The only solution is to increase
        // our capacity and go non-standard.
        let mut cap: Capacity = std_capacity();
        cap.cols = cols;
        cap
    }

    /// Initialize the page. The top of the first page in the list is always the
    /// top of the active area of the screen (important knowledge for quickly
    /// setting up cursors in Screen).
    ///
    /// max_size is the maximum number of bytes that will be allocated for
    /// pages. If this is smaller than the bytes required to show the viewport
    /// then max_size will be ignored and the viewport will be shown, but no
    /// scrollback will be created. max_size is always rounded down to the nearest
    /// terminal page size (not virtual memory page), otherwise we would always
    /// slightly exceed max_size in the limits.
    ///
    /// If max_size is null then there is no defined limit and the screen will
    /// grow forever. In reality, the limit is set to the byte limit that your
    /// computer can address in memory. If you somehow require more than that
    /// (due to disk paging) then please contribute that yourself and perhaps
    /// search deep within yourself to find out why you need that.
    //
    // PORT NOTE: the reference takes `alloc: Allocator` for the node/pin pools
    // and `pageAllocator()` for the page pool. The `rustty_zigstd` pools and the
    // hand-rolled `PagePool` are all backed by the Rust system allocator, so no
    // allocator parameter is threaded through (R1 additive glue boundary). The
    // `init_tw`/`initPages_tw` tripwire failpoints are exercised only by the
    // deferred test phase and are no-ops in non-test builds; see the tests
    // phase for the failpoint wiring.
    pub fn init(
        cols: size::CellCountInt,
        rows: size::CellCountInt,
        max_size: Option<usize>,
    ) -> Result<PageList, OutOfMemory> {
        // The screen starts with a single page that is the entire viewport,
        // and we'll split it thereafter if it gets too large and add more as
        // necessary.
        // try tw.check(.init_memory_pool);
        let mut pool = MemoryPool::init(PAGE_PREHEAT)?;
        // errdefer pool.deinit();

        // try tw.check(.init_pages);
        let mut page_serial: u64 = 0;
        let (page_list, page_size) = match Self::init_pages(&mut pool, &mut page_serial, cols, rows)
        {
            Ok(v) => v,
            Err(e) => {
                // errdefer pool.deinit() (pool dropped on early return).
                return Err(e);
            }
        };

        // Get our minimum max size, see doc comments for more details.
        let min_max_size = Self::min_max_size(cols, rows);

        // We always track our viewport pin to ensure this is never an allocation
        // try tw.check(.viewport_pin);
        let viewport_pin = pool.pins.create().map_err(|_| OutOfMemory)?;
        // viewport_pin.* = .{ .node = page_list.first.? };
        unsafe {
            viewport_pin.as_ptr().write(Pin {
                node: page_list.first,
                y: 0,
                x: 0,
                garbage: false,
            });
        }
        let mut tracked_pins: PinSet = PinSet::new();
        // errdefer tracked_pins.deinit(pool.alloc);

        // try tw.check(.viewport_pin_track);
        tracked_pins.put_no_clobber(viewport_pin.as_ptr(), ());

        // errdefer comptime unreachable;
        let result: PageList = PageList {
            cols,
            rows,
            pool,
            pages: page_list,
            page_serial,
            page_serial_min: 0,
            page_size,
            explicit_max_size: max_size.unwrap_or(usize::MAX),
            min_max_size,
            total_rows: rows as usize,
            tracked_pins,
            viewport: Viewport::Active,
            viewport_pin: viewport_pin.as_ptr(),
            viewport_pin_row_offset: None,
            pause_integrity_checks: 0,
        };
        result.assert_integrity();
        Ok(result)
    }

    fn init_pages(
        pool: &mut MemoryPool,
        serial: &mut u64,
        cols: size::CellCountInt,
        rows: size::CellCountInt,
    ) -> Result<(List, usize), OutOfMemory> {
        let mut page_list: List = List::default();
        let mut page_size: usize = 0;

        // Add pages as needed to create our initial viewport.
        let cap = Self::initial_capacity(cols);
        let layout = Page::layout(cap);
        let pooled = layout.total_size <= std_size();

        // Guaranteed by comptime checks in initialCapacity but
        // redundant here for safety.
        assert!(layout.total_size <= size::MAX_PAGE_SIZE as usize);

        // If we have an error, we need to clean up our non-standard pages
        // since they're not in the pool.
        //
        // PORT NOTE: the reference `errdefer` frees every non-standard page in
        // `page_list` on failure. We make this explicit at each fallible step
        // by freeing the partial list before returning the error (the only
        // fallible op below is `pool.nodes.create`).
        let mut rem = rows;
        while rem > 0 {
            // try tw.check(.page_node);
            let node = match pool.nodes.create() {
                Ok(n) => n,
                Err(_) => {
                    Self::free_non_standard_pages(&mut page_list);
                    return Err(OutOfMemory);
                }
            };
            // errdefer pool.nodes.destroy(node);

            let page_buf: NonNull<u8> = if pooled {
                // try tw.check(.page_buf_std);
                pool.pages.create()
            } else {
                // try tw.check(.page_buf_non_std);
                // break :buf try page_alloc.alignedAlloc(
                //     u8,
                //     .fromByteUnits(std.heap.page_size_min),
                //     layout.total_size,
                // );
                Self::alloc_non_standard(layout.total_size)
            };
            // errdefer if (pooled)
            //     pool.pages.destroy(page_buf)
            // else
            //     page_alloc.free(page_buf);

            // In runtime safety modes we have to memset because the Zig allocator
            // interface will always memset to 0xAA for undefined. On freestanding
            // (WASM), the WasmAllocator reuses freed slots without zeroing since
            // only fresh memory.grow pages are guaranteed zero by the WASM spec.
            // On native, the OS page allocator (mmap) returns zeroed pages.
            //
            // PORT NOTE: our `PagePool::create`/`alloc_non_standard` always hand
            // back zeroed memory (`alloc_zeroed` / free-list zeroed on reset), so
            // this is already satisfied on every build; the explicit memset is
            // unnecessary in the Rust port.

            // Initialize the first set of pages to contain our viewport so that
            // the top of the first page is always the active area.
            // node.* = .{
            //     .data = .initBuf(.init(page_buf), layout),
            //     .serial = serial.*,
            // };
            let buf = OffsetBuf::init(page_buf.as_ptr() as *const u8);
            let mut data = Page::init_buf(buf, layout);
            data.size.rows = core::cmp::min(rem, data.capacity.rows);
            rem -= data.size.rows;
            unsafe {
                node.as_ptr().write(Node {
                    prev: core::ptr::null_mut(),
                    next: core::ptr::null_mut(),
                    data,
                    serial: *serial,
                });
            }

            // Add the page to the list
            page_size += Self::page_buf_len(layout.total_size, pooled);
            unsafe {
                page_list.append(node.as_ptr());
            }
            // errdefer comptime unreachable;

            // Increment our serial
            *serial += 1;
        }

        assert!(!page_list.first.is_null());

        Ok((page_list, page_size))
    }

    /// Assert that the PageList is in a valid state. This is a no-op in
    /// release builds.
    #[inline]
    pub fn assert_integrity(&self) {
        if !cfg!(debug_assertions) {
            return;
        }

        if let Err(_err) = self.verify_integrity() {
            // log.err("PageList integrity check failed: {}", .{err});
            panic!("PageList integrity check failed");
        }
    }

    /// Pause or resume integrity checks. This is useful when you're doing
    /// a multi-step operation that temporarily leaves the PageList in an
    /// inconsistent state.
    #[inline]
    pub fn pause_integrity_checks(&mut self, pause: bool) {
        if !cfg!(debug_assertions) {
            return;
        }
        if pause {
            self.pause_integrity_checks += 1;
        } else {
            self.pause_integrity_checks -= 1;
        }
    }

    /// Verify the integrity of the PageList. This is expensive and should
    /// only be called in debug/test builds.
    fn verify_integrity(&self) -> Result<(), IntegrityError> {
        if !cfg!(debug_assertions) {
            return Ok(());
        }
        if self.pause_integrity_checks > 0 {
            return Ok(());
        }

        // Our viewport pin should never be garbage
        assert!(unsafe { !(*self.viewport_pin).garbage });

        // Grab our total rows
        let mut actual_total: usize = 0;
        {
            let mut node_ = self.pages.first;
            while !node_.is_null() {
                let node = node_;
                actual_total += unsafe { (*node).data.size.rows as usize };
                node_ = unsafe { (*node).next };

                // While doing this traversal, verify no node has a serial
                // number lower than our min.
                if unsafe { (*node).serial } < self.page_serial_min {
                    // log.warn(
                    //     "PageList integrity violation: page serial too low serial={} min={}",
                    //     .{ node.serial, self.page_serial_min },
                    // );
                    return Err(IntegrityError::PageSerialInvalid);
                }
            }
        }

        // Verify that our cached total_rows matches the actual row count
        if actual_total != self.total_rows {
            // log.warn(
            //     "PageList integrity violation: total_rows mismatch cached={} actual={}",
            //     .{ self.total_rows, actual_total },
            // );
            return Err(IntegrityError::TotalRowsMismatch);
        }

        // Verify that all our tracked pins point to valid pages.
        for p in self.tracked_pins.keys() {
            if !self.pin_is_valid(unsafe { (**p).pin_copy() }) {
                return Err(IntegrityError::TrackedPinInvalid);
            }
        }

        if self.viewport == Viewport::Pin {
            // Verify that our viewport pin row offset is correct.
            let actual_offset: usize = 'offset: {
                let mut offset: usize = 0;
                let mut node = self.pages.last;
                while !node.is_null() {
                    let n = node;
                    offset += unsafe { (*n).data.size.rows as usize };
                    if core::ptr::eq(n, unsafe { (*self.viewport_pin).node }) {
                        offset -= unsafe { (*self.viewport_pin).y as usize };
                        break 'offset self.total_rows - offset;
                    }
                    node = unsafe { (*n).prev };
                }

                // log.warn(
                //     "PageList integrity violation: viewport pin not in list",
                //     .{},
                // );
                return Err(IntegrityError::ViewportPinOffsetMismatch);
            };

            if let Some(cached_offset) = self.viewport_pin_row_offset {
                if cached_offset != actual_offset {
                    // log.warn(
                    //     "PageList integrity violation: viewport pin offset mismatch cached={} actual={}",
                    //     .{ cached_offset, actual_offset },
                    // );
                    return Err(IntegrityError::ViewportPinOffsetMismatch);
                }
            }

            // Ensure our viewport has enough rows.
            let rows = self.total_rows - actual_offset;
            if rows < self.rows as usize {
                // log.warn(
                //     "PageList integrity violation: viewport pin rows too small rows={} needed={}",
                //     .{ rows, self.rows },
                // );
                return Err(IntegrityError::ViewportPinInsufficientRows);
            }
        }

        Ok(())
    }

    /// Deinit the pagelist. If you own the memory pool (used clonePool) then
    /// this will reset the pool and retain capacity.
    pub fn deinit(&mut self) {
        // Verify integrity before cleanup
        self.assert_integrity();

        // Always deallocate our hashmap.
        //
        // PORT NOTE: the reference `tracked_pins.deinit(self.pool.alloc)`; the
        // Rust `PinSet` owns its backing storage, so dropping `self.tracked_pins`
        // frees it. We clear it here to mirror the explicit deinit ordering.
        self.tracked_pins = PinSet::new();

        // Go through our linked list and deallocate all pages that are
        // not standard size.
        let mut it = self.pages.first;
        while !it.is_null() {
            let node = it;
            if Self::page_memory_len(node) > std_size() {
                Self::free_non_standard_page(node);
            }
            it = unsafe { (*node).next };
        }

        // Deallocate all the pages. We don't need to deallocate the list or
        // nodes because they all reside in the pool.
        //
        // PORT NOTE: `self.pool.deinit()` is the pool's `Drop`; dropping
        // `self.pool` when the `PageList` is dropped frees the node/pin/page
        // pools. The standard page buffers are owned by `PagePool` and freed by
        // its `Drop`, so there is nothing to free here beyond the non-standard
        // pages handled above.
    }

    /// Reset the PageList back to an empty state. This is similar to
    /// deinit and reinit but it importantly preserves the pointer
    /// stability of tracked pins (they're moved to the top-left since
    /// all contents are cleared).
    ///
    /// This can't fail because we always retain at least enough allocated
    /// memory to fit the active area.
    pub fn reset(&mut self) {
        // defer self.assertIntegrity();

        // Invalidate all external page refs to the previous list. The reset below
        // rebuilds the page list from the pools, so old untracked refs must be
        // rejected before any validation attempts to inspect their node pointers.
        self.page_serial_min = self.page_serial;

        // We need enough pages/nodes to keep our active area. This should
        // never fail since we by definition have allocated a page already
        // that fits our size but I'm not confident to make that assertion.
        let cap = Self::initial_capacity(self.cols);
        assert!(cap.rows > 0);

        // The number of pages we need is the number of rows in the active
        // area divided by the row capacity of a page.
        let page_count: usize = (self.rows as usize).div_ceil(cap.rows as usize);

        // Before resetting our pools we need to free any pages that
        // are non-standard size since those were allocated outside
        // the pool.
        {
            let mut it = self.pages.first;
            while !it.is_null() {
                let node = it;
                if Self::page_memory_len(node) > std_size() {
                    Self::free_non_standard_page(node);
                }
                it = unsafe { (*node).next };
            }
        }

        // Reset our pools to free as much memory as possible while retaining
        // the capacity for at least the minimum number of pages we need.
        // The return value is whether memory was reclaimed or not, but in
        // either case the pool is left in a valid state.
        //
        // Our page pool relies on mmap to zero our page memory. Since we're
        // retaining a certain amount of memory, it won't use mmap and won't
        // be zeroed. This block zeroes out all the memory in the pool arena.
        //
        // Note: we only have to do this for the page pool because the
        // nodes are always fully overwritten on each allocation.
        //
        // PORT NOTE: the reference reads the private `ArenaAllocator.BufNode`
        // layout to zero the retained arena memory. Our hand-rolled `PagePool`
        // zeroes the retained free-list buffers directly inside
        // `reset_retain_with_limit`, reproducing that `@memset(0)` behavior
        // without depending on a private allocator layout.
        self.pool
            .reset_pages_retain_with_limit(page_count * PagePool::item_size());
        self.pool
            .reset_nodes_retain_with_limit(page_count * NodePool::ITEM_SIZE);

        // Initialize our pages. This should not be able to fail since
        // we retained the capacity for the minimum number of pages we need.
        let (pages, page_size) =
            match Self::init_pages(&mut self.pool, &mut self.page_serial, self.cols, self.rows) {
                Ok(v) => v,
                Err(_) => panic!("initPages failed"),
            };
        self.pages = pages;
        self.page_size = page_size;

        // Our total rows always goes back to the default
        self.total_rows = self.rows as usize;

        // Update all our tracked pins to point to our first page top-left
        // and mark them as garbage, because it got mangled in a way where
        // semantically it really doesn't make sense.
        {
            let first = self.pages.first;
            for p in self.tracked_pins.keys() {
                let p: *mut Pin = *p;
                unsafe {
                    (*p).node = first;
                    (*p).x = 0;
                    (*p).y = 0;
                    (*p).garbage = true;
                }
            }

            // Our viewport pin is never garbage
            unsafe {
                (*self.viewport_pin).garbage = false;
            }
        }

        // Move our viewport back to the active area since everything is gone.
        self.viewport = Viewport::Active;

        self.assert_integrity();
    }

    // -- Lowest-level private page-management helpers ----------------------

    // PORT NOTE: `#[allow(dead_code)]` on these lowest-level private helpers —
    // they are called by the cluster sub-files (grow/erase/resize/clone) whose
    // bodies land in the Methods phase; until then the CORE skeleton has no
    // callers for them.

    /// Create a new page node. This does not add it to the list and this
    /// does not do any memory size accounting with max_size/page_size.
    #[inline]
    #[allow(dead_code)]
    fn create_page(&mut self, cap: Capacity) -> Result<*mut Node, OutOfMemory> {
        // log.debug("create page cap={}", .{cap});
        Self::create_page_ext(
            &mut self.pool,
            cap,
            &mut self.page_serial,
            Some(&mut self.page_size),
        )
    }

    #[inline]
    #[allow(dead_code)]
    fn create_page_ext(
        pool: &mut MemoryPool,
        cap: Capacity,
        serial: &mut u64,
        total_size: Option<&mut usize>,
    ) -> Result<*mut Node, OutOfMemory> {
        let page = pool.nodes.create().map_err(|_| OutOfMemory)?;
        // errdefer pool.nodes.destroy(page);

        let layout = Page::layout(cap);
        let pooled = layout.total_size <= std_size();

        // It would be better to encode this into the Zig error handling
        // system but that is a big undertaking and we only have a few
        // centralized call sites so it is handled on its own currently.
        assert!(layout.total_size <= size::MAX_PAGE_SIZE as usize);

        // Our page buffer comes from our standard memory pool if it
        // is within our standard size since this is what the pool
        // dispenses. Otherwise, we use the heap allocator to allocate.
        let page_buf: NonNull<u8> = if pooled {
            pool.pages.create()
        } else {
            Self::alloc_non_standard(layout.total_size)
        };
        // errdefer if (pooled)
        //     pool.pages.destroy(page_buf)
        // else
        //     page_alloc.free(page_buf);

        // In runtime safety modes, allocators fill with 0xAA. On freestanding
        // (WASM), the WasmAllocator reuses freed slots without zeroing.
        //
        // PORT NOTE: our buffers are always handed back zeroed; the explicit
        // memset is unnecessary in the Rust port (see `init_pages`).

        // page.* = .{
        //     .data = .initBuf(.init(page_buf), layout),
        //     .serial = serial.*,
        // };
        let buf = OffsetBuf::init(page_buf.as_ptr() as *const u8);
        let mut data = Page::init_buf(buf, layout);
        data.size.rows = 0;
        unsafe {
            page.as_ptr().write(Node {
                prev: core::ptr::null_mut(),
                next: core::ptr::null_mut(),
                data,
                serial: *serial,
            });
        }
        *serial += 1;

        if let Some(v) = total_size {
            // Accumulate page size now. We don't assert or check max size
            // because we may exceed it here temporarily as we are allocating
            // pages before destroy.
            *v += Self::page_buf_len(layout.total_size, pooled);
        }

        Ok(page.as_ptr())
    }

    /// Destroy the memory of the given node in the PageList linked list
    /// and return it to the pool. The node is assumed to already be removed
    /// from the linked list.
    ///
    /// IMPORTANT: This function does NOT update `total_rows`. The caller is
    /// responsible for accounting for the removed rows. This function only
    /// updates `page_size` (byte accounting), not row accounting.
    #[allow(dead_code)]
    fn destroy_node(&mut self, node: *mut Node) {
        // PORT NOTE: split the page-size borrow from the pool borrow because the
        // reference passes `&self.pool` and `&self.page_size` separately; in Rust
        // we cannot borrow two fields of `self` mutably through a single helper
        // call, so we pass the page-size out-param via a raw pointer to match the
        // reference `destroyNodeExt(&self.pool, node, &self.page_size)`.
        let page_size: *mut usize = &mut self.page_size;
        Self::destroy_node_ext(&mut self.pool, node, Some(unsafe { &mut *page_size }));
    }

    #[allow(dead_code)]
    fn destroy_node_ext(pool: &mut MemoryPool, node: *mut Node, total_size: Option<&mut usize>) {
        let page: *mut Page = unsafe { &mut (*node).data };

        // Update our accounting for page size
        if let Some(v) = total_size {
            *v -= Self::page_memory_len(node);
        }

        if Self::page_memory_len(node) <= std_size() {
            // Reset the memory to zero so it can be reused
            let (ptr, len) = Self::page_memory(page);
            unsafe {
                core::ptr::write_bytes(ptr, 0, len);
            }
            pool.pages.destroy(NonNull::new(ptr).unwrap());
        } else {
            Self::free_non_standard_page(node);
        }

        unsafe { pool.nodes.destroy(NonNull::new(node).unwrap()) };
    }

    // -- Page-buffer raw-pointer helpers (Rust port glue) ------------------
    //
    // PORT NOTE: the reference accesses `node.data.memory` (a slice) directly.
    // The Rust `Page` keeps `memory_ptr`/`memory_len`, so these small helpers
    // recover the pointer/len without forming a `&Page` (avoids retagging
    // concerns under Miri) and centralize the non-standard alloc/free.

    /// Allocate a page-aligned, zeroed, `total_size`-byte buffer for a
    /// NON-STANDARD page (the reference `page_alloc.alignedAlloc`).
    fn alloc_non_standard(total_size: usize) -> NonNull<u8> {
        let layout = core::alloc::Layout::from_size_align(total_size, PAGE_SIZE_MIN).unwrap();
        let ptr = unsafe { std::alloc::alloc_zeroed(layout) };
        NonNull::new(ptr).unwrap_or_else(|| std::alloc::handle_alloc_error(layout))
    }

    /// Free a NON-STANDARD page's backing buffer (the reference
    /// `page_alloc.free(node.data.memory)`).
    fn free_non_standard_page(node: *mut Node) {
        let page: *const Page = unsafe { &(*node).data };
        let (ptr, len) = Self::page_memory(page as *mut Page);
        let layout = core::alloc::Layout::from_size_align(len, PAGE_SIZE_MIN).unwrap();
        unsafe { std::alloc::dealloc(ptr, layout) };
    }

    /// Free all non-standard pages currently in `list` (the reference
    /// `errdefer` page-cleanup loop in `initPages`/`clone`).
    fn free_non_standard_pages(list: &mut List) {
        let mut it = list.first;
        while !it.is_null() {
            let node = it;
            if Self::page_memory_len(node) > std_size() {
                Self::free_non_standard_page(node);
            }
            it = unsafe { (*node).next };
        }
    }

    /// `node.data.memory.len`.
    #[inline]
    fn page_memory_len(node: *mut Node) -> usize {
        unsafe { (*node).data.memory_len }
    }

    /// `(node.data.memory.ptr, node.data.memory.len)` via field projection on a
    /// `*mut Page` (avoids a `&Page` retag while a field is borrowed).
    #[inline]
    fn page_memory(page: *mut Page) -> (*mut u8, usize) {
        unsafe {
            let ptr = core::ptr::read(core::ptr::addr_of!((*page).memory_ptr));
            let len = core::ptr::read(core::ptr::addr_of!((*page).memory_len));
            (ptr, len)
        }
    }

    /// The byte length accounted for a page buffer in `page_size`. For pooled
    /// (standard) pages the reference uses `page_buf.len` which is exactly
    /// `std_size`; for non-standard pages it is `layout.total_size`.
    #[inline]
    fn page_buf_len(total_size: usize, pooled: bool) -> usize {
        if pooled {
            std_size()
        } else {
            total_size
        }
    }

    /// Returns the actual max size. This may be greater than the explicit
    /// value if the explicit value is less than the min_max_size.
    ///
    /// This value is a HEURISTIC. You cannot assert on this value. We may
    /// exceed this value if required to fit the active area. This may be
    /// required in some cases if the active area has a large number of
    /// graphemes, styles, etc.
    pub fn max_size(&self) -> usize {
        core::cmp::max(self.explicit_max_size, self.min_max_size)
    }

    /// Returns true if the given pin is valid for the page list.
    //
    // PORT NOTE: the canonical home for `pinIsValid` is the pin cluster
    // (`pin.rs`). It is defined here because the CORE `verify_integrity` calls
    // it; when the pin cluster lands it can be re-homed (the body is fully
    // ported, not stubbed).
    pub fn pin_is_valid(&self, p: Pin) -> bool {
        // This is very slow so we want to ensure we only ever
        // call this during slow runtime safety builds.
        //
        // PORT NOTE: the reference `comptime assert(build_options.slow_runtime_safety)`
        // is a compile-time guard valid because `pinIsValid` is only compiled
        // under slow_runtime_safety. In Rust the function is always compiled but
        // only ever *reached* in debug builds (its sole CORE caller
        // `verify_integrity` returns early when `!cfg!(debug_assertions)`); we
        // keep the same invariant as a runtime assertion that would fire if it
        // were ever called outside a slow-runtime-safety build.
        #[allow(clippy::assertions_on_constants)]
        {
            assert!(cfg!(debug_assertions));
        }

        let mut it = self.pages.first;
        while !it.is_null() {
            let node = it;
            if !core::ptr::eq(node, p.node) {
                it = unsafe { (*node).next };
                continue;
            }
            return p.y < unsafe { (*node).data.size.rows }
                && p.x < unsafe { (*node).data.size.cols };
        }

        false
    }

    /// Get the top-left of the screen for the given tag.
    pub fn get_top_left(&self, tag: point::Tag) -> Pin {
        match tag {
            // The full screen or history is always just the first page.
            point::Tag::Screen | point::Tag::History => Pin {
                node: self.pages.first,
                y: 0,
                x: 0,
                garbage: false,
            },

            point::Tag::Viewport => match self.viewport {
                Viewport::Active => self.get_top_left(point::Tag::Active),
                Viewport::Top => self.get_top_left(point::Tag::Screen),
                Viewport::Pin => unsafe { (*self.viewport_pin).pin_copy() },
            },

            // The active area is calculated backwards from the last page.
            // This makes getting the active top left slower but makes scrolling
            // much faster because we don't need to update the top left. Under
            // heavy load this makes a measurable difference.
            point::Tag::Active => {
                let mut rem = self.rows;
                let mut it = self.pages.last;
                while !it.is_null() {
                    let node = it;
                    let node_rows = unsafe { (*node).data.size.rows };
                    if rem <= node_rows {
                        return Pin {
                            node,
                            y: node_rows - rem,
                            x: 0,
                            garbage: false,
                        };
                    }

                    rem -= node_rows;
                    it = unsafe { (*node).prev };
                }

                unreachable!(); // assertion: we always have enough rows for active
            }
        }
    }

    /// The total rows in the screen. This is the actual row count currently
    /// and not a capacity or maximum.
    ///
    /// This is very slow, it traverses the full list of pages to count the
    /// rows, so it is not pub. This is only used for testing/debugging.
    #[allow(dead_code)]
    fn total_rows(&self) -> usize {
        let mut rows: usize = 0;
        let mut node_ = self.pages.first;
        while !node_.is_null() {
            let node = node_;
            rows += unsafe { (*node).data.size.rows as usize };
            node_ = unsafe { (*node).next };
        }

        rows
    }

    /// The total number of pages in this list. This should only be used
    /// for tests since it is O(N) over the list of pages.
    pub fn total_pages(&self) -> usize {
        let mut pages: usize = 0;
        let mut node_ = self.pages.first;
        while !node_.is_null() {
            pages += 1;
            node_ = unsafe { (*node_).next };
        }

        pages
    }
}

impl Pin {
    /// Returns a bitwise copy of this pin. The reference passes `Pin` by value
    /// (`Pin` is a `Copy`-like struct of POD fields); in Rust we read the fields
    /// through a raw pointer to avoid a `&Pin` retag and reconstruct the value.
    #[inline]
    pub(crate) fn pin_copy(&self) -> Pin {
        Pin {
            node: self.node,
            y: self.y,
            x: self.x,
            garbage: self.garbage,
        }
    }
}

/// Integrity errors returned by `verify_integrity`.
//
// PORT NOTE: the reference `IntegrityError = error{...}` is a Zig error set;
// it ports to a Rust enum used as the `Err` variant of `verify_integrity`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntegrityError {
    PageSerialInvalid,
    TotalRowsMismatch,
    TrackedPinInvalid,
    ViewportPinOffsetMismatch,
    ViewportPinInsufficientRows,
}

// PORT NOTE: keep ported-but-not-yet-called names referenced so the crown-jewel
// surface stays stable as the cluster sub-files land in the Methods phase.
const _: () = {
    let _ = PAGE_PREHEAT;
};
#[allow(dead_code)]
type _RowAlias = Row;
