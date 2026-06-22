//! Maintains a linked list of pages to make up a terminal screen
//! and provides higher level operations on top of those pages to
//! make it slightly easier to work with.
//!
//! PORTS: ghostty src/terminal/PageList.zig
//!
//! PORT STATUS: sub-batch 3 prep (module skeleton).
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
//!                 dirty / clone / split / diagram / highlightSemanticContent /
//!                 cval / eql
//!
//! PORT BLOCKER (sub-batch 3, not this prep batch): the CORE lifecycle bodies
//! (`init`/`deinit`/`reset` and `initPages`/`destroyNode`) depend on a Rust
//! `MemoryPool` (the reference `std.heap.MemoryPool` /
//! `std.heap.MemoryPoolAligned` over a page-aligned arena, plus the
//! `std.heap.ArenaAllocator` `reset(.retain_with_limit)` + BufNode-zeroing
//! behavior in `reset`) and on size-level constants (`size.max_page_size`)
//! that are NOT yet ported. Those are a deliberate sub-batch-3 design decision
//! (the offset-arena + tracked-pin raw pointers the kickoff calls out for the
//! Miri gate). This prep batch lands the faithful struct/type/signature
//! skeleton (every field + every doc comment, verbatim-retargeted) with the
//! blocked bodies as documented `todo!` stubs; it does NOT fabricate logic
//! (R1/R5). The 205 ported `test {}` blocks land with their owning clusters.

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
// doubly-linked list comes from `rustty-datastruct`.
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

use crate::page as pagepkg;
use crate::page::{Capacity, Page, Row};

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

// /// The memory pool we get page nodes from.
// const NodePool = std.heap.MemoryPool(List.Node);

// /// The standard page capacity that we use as a starting point for
// /// all pages. This is chosen as a sane default that fits most terminal
// /// usage to support using our pool.
// const std_capacity = pagepkg.std_capacity;
//
// PORT NOTE: `pagepkg.std_capacity` is a value in Rust (`pagepkg::std_capacity()`).

// /// The byte size required for a standard page.
// const std_size = Page.layout(std_capacity).total_size;

// /// The memory pool we use for page memory buffers. We use a separate pool
// /// so we can allocate these with a page allocator. We have to use a page
// /// allocator because we need memory that is zero-initialized and page-aligned.
// const PagePool = std.heap.MemoryPoolAligned(
//     [std_size]u8,
//     .fromByteUnits(std.heap.page_size_min),
// );

// /// List of pins, known as "tracked" pins. These are pins that are kept
// /// up to date automatically through page-modifying operations.
// const PinSet = std.AutoArrayHashMapUnmanaged(*Pin, void);
// const PinPool = std.heap.MemoryPool(Pin);
//
// PORT NOTE: `Pin` is defined in `pin.rs` (sibling sub-file). `PinSet`/`PinPool`
// land with the `MemoryPool` port (sub-batch 3); see the PORT BLOCKER above.

/// The pool of memory used for a pagelist. This can be shared between
/// multiple pagelists but it is not threadsafe.
//
// PORT BLOCKER (sub-batch 3): the reference `MemoryPool` wraps
// `std.heap.MemoryPool`/`MemoryPoolAligned` (NodePool/PagePool/PinPool) over a
// page-aligned arena. The Rust equivalent (offset-arena + tracked-pin raw
// pointers, the Miri-gated pattern) is a sub-batch-3 design decision and is not
// yet ported. The faithful struct shell + method signatures + doc comments are
// kept here; the bodies are documented `todo!` stubs.
pub struct MemoryPool {
    // alloc: Allocator,
    // nodes: NodePool,
    // pages: PagePool,
    // pins: PinPool,
    _placeholder: (),
}

impl MemoryPool {
    // pub const ResetMode = std.heap.ArenaAllocator.ResetMode;

    // pub fn init(
    //     gen_alloc: Allocator,
    //     page_alloc: Allocator,
    //     preheat: usize,
    // ) Allocator.Error!MemoryPool {
    //     var node_pool = try NodePool.initPreheated(gen_alloc, preheat);
    //     errdefer node_pool.deinit();
    //     var page_pool = try PagePool.initPreheated(page_alloc, preheat);
    //     errdefer page_pool.deinit();
    //     var pin_pool = try PinPool.initPreheated(gen_alloc, 8);
    //     errdefer pin_pool.deinit();
    //     return .{
    //         .alloc = gen_alloc,
    //         .nodes = node_pool,
    //         .pages = page_pool,
    //         .pins = pin_pool,
    //     };
    // }

    // pub fn deinit(self: *MemoryPool) void {
    //     self.pages.deinit();
    //     self.nodes.deinit();
    //     self.pins.deinit();
    // }

    // pub fn reset(self: *MemoryPool, mode: ResetMode) void {
    //     _ = self.pages.reset(mode);
    //     _ = self.nodes.reset(mode);
    //     _ = self.pins.reset(mode);
    // }
}

/// The viewport location.
//
// PORT NOTE: the reference `Viewport` is a `union(enum)` with no payloads on
// any variant (the `.pin` variant deliberately carries no value), so it ports
// to a plain Rust enum.
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
    //
    // PORT NOTE: `PinSet` (`std.AutoArrayHashMapUnmanaged(*Pin, void)`) lands
    // with the `MemoryPool` port (sub-batch 3); placeholder until then.
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

// ---------------------------------------------------------------------------
// PORT NOTE: `Pin` and `PinSet` are owned by `pin.rs` (sibling sub-file). For
// the prep skeleton we forward-declare the names here so the struct fields can
// reference them. The full `Pin` definition (with `rowAndCell`, `cells`,
// `isBetween`, `before`, `leftWrap`, `rightWrap`, `downOverflow`,
// `upOverflow`, etc.) and the `PinSet` hashmap type land in `pin.rs` with the
// MemoryPool port (sub-batch 3).
// ---------------------------------------------------------------------------

/// A pin into the PageList. See `pin.rs` for the full definition.
//
// PORT BLOCKER (sub-batch 3): full body ported in `pin.rs`.
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

/// List of pins, known as "tracked" pins. These are pins that are kept
/// up to date automatically through page-modifying operations.
//
// PORT BLOCKER (sub-batch 3): the reference
// `std.AutoArrayHashMapUnmanaged(*Pin, void)` lands with the MemoryPool port.
pub type PinSet = ();

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
    //
    // PORT BLOCKER (sub-batch 3): depends on `PagePool.item_size` (the
    // page-aligned pool's per-item byte size), not yet ported.
    //
    // PORT NOTE: `#[allow(dead_code)]` on these lowest-level private helpers —
    // they are called by the cluster sub-files (grow/erase/resize/...) whose
    // bodies land in sub-batch 3; until then the prep skeleton has no callers.
    #[allow(dead_code)]
    fn min_max_size(_cols: size::CellCountInt, _rows: size::CellCountInt) -> usize {
        // Invariant required to ensure our divCeil below cannot overflow.
        // Get our capacity to fit our rows. If the cols are too big, it may
        // force less rows than we want meaning we need more than one page to
        // represent a viewport.
        // Calculate the number of standard sized pages we need to represent
        // an active area.
        // We always need at least one page extra so that we
        // can fit partial pages to spread our active area across two pages.
        // Even for caps that can't fit all rows in a single page, we add one
        // because the most extra space we need at any given time is only
        // the partial amount of one page.
        // return PagePool.item_size * pages;
        todo!("PageList::min_max_size — blocked on MemoryPool/PagePool port (sub-batch 3)")
    }

    /// Calculates the initial capacity for a new page for a given column
    /// count. This will attempt to fit within std_size at all times so we
    /// can use our memory pool, but if cols is too big, this will return a
    /// larger capacity.
    ///
    /// The returned capacity is always guaranteed to layout properly (not
    /// overflow). We are able to support capacities up to the maximum int
    /// value of cols, so this will never overflow.
    //
    // PORT BLOCKER (sub-batch 3): depends on `size::max_page_size` (not yet a
    // pub const in `rustty_render_types::term_size`) for the comptime layout
    // invariant. The runtime `std_capacity().adjust(...)` path is ready.
    #[allow(dead_code)]
    fn initial_capacity(_cols: size::CellCountInt) -> Capacity {
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
        // If we can adjust our standard capacity, we fit within the
        // standard size and we're good!
        // Ensure our error set doesn't change.
        //
        // This code path means that our standard capacity can't even
        // accommodate our column count! The only solution is to increase
        // our capacity and go non-standard.
        todo!("PageList::initial_capacity — blocked on size::max_page_size const (sub-batch 3)")
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
    // PORT BLOCKER (sub-batch 3): body depends on `MemoryPool::init`,
    // `init_pages`, `min_max_size`, the page-aligned `pageAllocator`, and the
    // tripwire failpoints — none ported yet.
    pub fn init(
        _cols: size::CellCountInt,
        _rows: size::CellCountInt,
        _max_size: Option<usize>,
    ) -> PageList {
        // The screen starts with a single page that is the entire viewport,
        // and we'll split it thereafter if it gets too large and add more as
        // necessary.
        // Get our minimum max size, see doc comments for more details.
        // We always track our viewport pin to ensure this is never an allocation
        todo!("PageList::init — blocked on MemoryPool/init_pages port (sub-batch 3)")
    }

    /// Initialize the initial set of pages for the viewport. Returns the page
    /// list and the total byte size of the allocated pages.
    //
    // PORT BLOCKER (sub-batch 3): body depends on the page-aligned `PagePool`
    // and `NodePool` allocators plus the tripwire failpoints.
    #[allow(dead_code)]
    fn init_pages(
        _pool: &mut MemoryPool,
        _serial: &mut u64,
        _cols: size::CellCountInt,
        _rows: size::CellCountInt,
    ) -> (List, usize) {
        // Add pages as needed to create our initial viewport.
        // Guaranteed by comptime checks in initialCapacity but
        // redundant here for safety.
        // If we have an error, we need to clean up our non-standard pages
        // since they're not in the pool.
        // In runtime safety modes we have to memset because the Zig allocator
        // interface will always memset to 0xAA for undefined. On freestanding
        // (WASM), the WasmAllocator reuses freed slots without zeroing since
        // only fresh memory.grow pages are guaranteed zero by the WASM spec.
        // On native, the OS page allocator (mmap) returns zeroed pages.
        // Initialize the first set of pages to contain our viewport so that
        // the top of the first page is always the active area.
        // Add the page to the list
        // Increment our serial
        todo!("PageList::init_pages — blocked on MemoryPool/PagePool port (sub-batch 3)")
    }

    /// Assert that the PageList is in a valid state. This is a no-op in
    /// release builds.
    //
    // PORT BLOCKER (sub-batch 3): body depends on `verify_integrity`.
    #[inline]
    pub fn assert_integrity(&self) {
        // if (comptime !build_options.slow_runtime_safety) return;
        // self.verifyIntegrity() catch |err| {
        //     log.err("PageList integrity check failed: {}", .{err});
        //     @panic("PageList integrity check failed");
        // };
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
    //
    // PORT BLOCKER (sub-batch 3): body depends on `Pin`/`pinIsValid` and the
    // viewport pin offset bookkeeping.
    #[allow(dead_code)]
    fn verify_integrity(&self) -> Result<(), IntegrityError> {
        todo!("PageList::verify_integrity — blocked on Pin port (sub-batch 3)")
    }

    /// Deinit the pagelist. If you own the memory pool (used clonePool) then
    /// this will reset the pool and retain capacity.
    //
    // PORT BLOCKER (sub-batch 3): body depends on `MemoryPool::deinit` and the
    // page-aligned child allocator for freeing non-standard pages.
    pub fn deinit(&mut self) {
        // Verify integrity before cleanup
        // Always deallocate our hashmap.
        // Go through our linked list and deallocate all pages that are
        // not standard size.
        // Deallocate all the pages. We don't need to deallocate the list or
        // nodes because they all reside in the pool.
        todo!("PageList::deinit — blocked on MemoryPool port (sub-batch 3)")
    }

    /// Reset the PageList back to an empty state. This is similar to
    /// deinit and reinit but it importantly preserves the pointer
    /// stability of tracked pins (they're moved to the top-left since
    /// all contents are cleared).
    ///
    /// This can't fail because we always retain at least enough allocated
    /// memory to fit the active area.
    //
    // PORT BLOCKER (sub-batch 3): body depends on `MemoryPool` arena
    // `reset(.retain_with_limit)` + the BufNode-zeroing of retained arena
    // memory, plus `init_pages`/`Pin`.
    pub fn reset(&mut self) {
        // Invalidate all external page refs to the previous list. The reset below
        // rebuilds the page list from the pools, so old untracked refs must be
        // rejected before any validation attempts to inspect their node pointers.
        // We need enough pages/nodes to keep our active area. This should
        // never fail since we by definition have allocated a page already
        // that fits our size but I'm not confident to make that assertion.
        // The number of pages we need is the number of rows in the active
        // area divided by the row capacity of a page.
        // Before resetting our pools we need to free any pages that
        // are non-standard size since those were allocated outside
        // the pool.
        // Reset our pools to free as much memory as possible while retaining
        // the capacity for at least the minimum number of pages we need.
        // The return value is whether memory was reclaimed or not, but in
        // either case the pool is left in a valid state.
        // Our page pool relies on mmap to zero our page memory. Since we're
        // retaining a certain amount of memory, it won't use mmap and won't
        // be zeroed. This block zeroes out all the memory in the pool arena.
        // Initialize our pages. This should not be able to fail since
        // we retained the capacity for the minimum number of pages we need.
        // Our total rows always goes back to the default
        // Update all our tracked pins to point to our first page top-left
        // and mark them as garbage, because it got mangled in a way where
        // semantically it really doesn't make sense.
        // Our viewport pin is never garbage
        // Move our viewport back to the active area since everything is gone.
        todo!("PageList::reset — blocked on MemoryPool arena reset port (sub-batch 3)")
    }

    // -- Lowest-level private page-management helpers ----------------------
    //
    // These are needed by virtually every cluster (grow/erase/resize/scroll).
    // PORT BLOCKER (sub-batch 3): bodies depend on the page-aligned `PagePool`
    // and `NodePool` allocators in `MemoryPool`.

    /// Destroy a node, returning its page memory to the pool and decrementing
    /// our page-size accounting.
    #[allow(dead_code)]
    fn destroy_node(&mut self, _node: *mut Node) {
        // destroyNodeExt(&self.pool, node, &self.page_size);
        todo!("PageList::destroy_node — blocked on MemoryPool port (sub-batch 3)")
    }

    /// Destroy a node against an explicit pool, optionally updating a total
    /// size accumulator. Splitting this out lets `clone` reuse it against a
    /// fresh pool.
    #[allow(dead_code)]
    fn destroy_node_ext(_pool: &mut MemoryPool, _node: *mut Node, _total_size: Option<&mut usize>) {
        // Update our accounting for page size
        // Reset the memory to zero so it can be reused
        todo!("PageList::destroy_node_ext — blocked on MemoryPool port (sub-batch 3)")
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

// PORT NOTE: re-export the unused-but-ported names so the crown-jewel surface
// stays stable as the cluster sub-files land. `pagepkg`/`Row`/`size` are used
// by the signatures above; the unused warnings are silenced until the bodies
// land in sub-batch 3.
#[allow(unused_imports)]
use pagepkg as _pagepkg_used;
const _: () = {
    // Keep `Row`/`PAGE_PREHEAT` referenced so the prep skeleton has no dead-code
    // warnings; they go live with `init_pages` in sub-batch 3.
    let _ = PAGE_PREHEAT;
};
#[allow(dead_code)]
type _RowAlias = Row;
