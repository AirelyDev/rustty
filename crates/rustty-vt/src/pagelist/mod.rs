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
pub mod compress;
pub mod erase;
pub mod grow;
pub mod iter;
pub mod pin;
pub mod resize;
pub mod scroll;

// PORT NOTE: the 205 PageList `test {}` blocks are integration tests that
// exercise many methods, so per the port plan they are co-located in a dedicated
// `tests.rs` (gated `#![cfg(test)]`) rather than split per-cluster.
#[cfg(test)]
mod tests;

// TEST-ONLY allocation/latency/RSS bench harness (§4): runs the identical
// scrollback workload under `cargo test --release` and reports MEASURED numbers
// (latency median, peak RSS via getrusage, page allocs+bytes via the
// `page_tracking` TrackingAllocator seam). Not part of the parity contract.
#[cfg(test)]
mod bench;

use rustty_datastruct::IntrusiveDoublyLinkedList as DoublyLinkedList;
use rustty_os::page_allocator::{page_allocator, PageAllocator};
use rustty_render_types::term_size as size;
use rustty_render_types::term_size::OffsetBuf;
use rustty_zigstd::array_hash_map::AutoArrayHashMapUnmanaged;
use rustty_zigstd::memory_pool::DynamicMemoryPoolAligned;
use rustty_zigstd::memory_pool::MemoryPool as ZigMemoryPool;

use allocator_api2::alloc::{Allocator, Global};
use core::alloc::Layout;
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
    pub data: NodeData,
    pub serial: u64,

    /// How the backing memory of the embedded Page was allocated. Pool-owned
    /// memory is always a full standard-size item from the memory pool,
    /// regardless of the page layout size. Heap-owned memory is allocated
    /// directly with the page allocator and is exactly `Page.memory_len()`
    /// bytes.
    ///
    /// This must never be inferred from the memory length: heap-owned pages can
    /// be smaller than the standard size (e.g. compacted pages), and returning
    /// one to the pool would corrupt it.
    ///
    /// This has no `Default` on purpose so that every construction site is
    /// forced to make an explicit decision.
    ///
    /// PORTS: oracle@main PageList.zig `Node.owned` (16e4b5e98).
    pub owned: Owned,
}

/// How a node's backing page memory was allocated.
///
/// PORTS: oracle@main PageList.zig `Node.Owned` (16e4b5e98).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owned {
    /// A full standard-size item dispensed by the page pool.
    Pool,
    /// An exact-size allocation made directly with the page allocator.
    Heap,
}

/// The physical representation of the page contents.
///
/// Both states retain the same page mapping and the same `Page` metadata. A
/// resident page can access the mapping directly since it is, well, resident.
///
/// A compressed page owns an encoded copy while the physical memory behind the
/// raw mapping is discarded. HOWEVER, we retain the virtual allocation so that
/// restoration is infallible.
///
/// PORTS: oracle@main PageList.zig `Node.Data` (421fe8dab).
///
/// PORT NOTE (WS7 I2, Stacked Borrows): this is `#[repr(C, u8)]` so that the
/// `Node` accessors can project the payload with PURE RAW POINTER ARITHMETIC.
/// `match &mut *data { Resident(p) => p as *mut Page }` would return a pointer
/// carrying a fresh `Unique` tag derived from a `&mut NodeData`; a later
/// `Node::page`/`metadata`/`rows` on the SAME node retags `data` again and
/// invalidates the earlier pointer, so dereferencing it afterwards is UB
/// (`grow.rs::split` genuinely holds a page pointer across such a call).
/// Under `repr(C, u8)` — RFC 2195 — the layout is exactly
/// `union { R: #[repr(C)] struct { tag: u8, Page }, C: #[repr(C)] struct { tag: u8, CompressedPage } }`,
/// so the discriminant is a plain byte read and the payload address is a
/// constant `offset_of!` away, both derived from the caller's own raw `node`
/// pointer with no intervening reference.
///
/// The cost is that the Zig-side niche packing is lost (the tag gets its own
/// aligned slot). A `Node` is a handful of words next to a multi-KiB page
/// mapping, so this is not measurable.
#[repr(C, u8)]
pub enum NodeData {
    Resident(Page) = NodeData::TAG_RESIDENT,
    Compressed(crate::compress::CompressedPage) = NodeData::TAG_COMPRESSED,
}

/// The `Resident` variant's guaranteed `repr(C, u8)` layout (RFC 2195).
#[repr(C)]
struct ResidentVariant {
    tag: u8,
    page: Page,
}

/// The `Compressed` variant's guaranteed `repr(C, u8)` layout (RFC 2195).
#[repr(C)]
struct CompressedVariant {
    tag: u8,
    compressed: crate::compress::CompressedPage,
}

impl NodeData {
    const TAG_RESIDENT: u8 = 0;
    const TAG_COMPRESSED: u8 = 1;

    /// Read the discriminant byte through a raw pointer.
    ///
    /// # Safety
    /// `data` must point at a live, initialized `NodeData`.
    #[inline]
    unsafe fn tag(data: *const NodeData) -> u8 {
        // SAFETY: `repr(C, u8)` puts the discriminant in the first byte of
        // every variant, and a `u8` read has no alignment requirement. This is
        // a raw read — it forms no reference and therefore retags nothing.
        unsafe { core::ptr::read(data as *const u8) }
    }

    /// Project the `Resident` payload address. Does NOT check the tag.
    ///
    /// # Safety
    /// `data` must point at a live `NodeData`.
    #[inline]
    unsafe fn resident_payload(data: *const NodeData) -> *const Page {
        // SAFETY: RFC 2195 guarantees the payload sits at
        // `offset_of!(ResidentVariant, page)` within the enum. Raw-derived from
        // `data`, so it inherits `data`'s provenance and tag unchanged.
        unsafe {
            (data as *const u8).add(core::mem::offset_of!(ResidentVariant, page)) as *const Page
        }
    }

    /// Project the `Compressed` payload address. Does NOT check the tag.
    ///
    /// # Safety
    /// `data` must point at a live `NodeData`.
    #[inline]
    unsafe fn compressed_payload(data: *const NodeData) -> *const crate::compress::CompressedPage {
        // SAFETY: as `resident_payload`.
        unsafe {
            (data as *const u8).add(core::mem::offset_of!(CompressedVariant, compressed))
                as *const crate::compress::CompressedPage
        }
    }
}

/// The backing-memory representation currently stored by a node.
///
/// PORTS: oracle@main PageList.zig `Node.Storage`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Storage {
    Resident,
    Compressed,
}

/// Read-only page with full memory access that doesn't change a node's storage
/// state: if a node is compressed it remains compressed.
///
/// Resident pages are borrowed directly, so they're basically free. Compressed
/// pages are decoded into an independently owned Page so callers can inspect
/// their contents. Because resident pages are still borrowed, you have to
/// continue to have exclusive access to the PageList during this operation.
///
/// PORTS: oracle@main PageList.zig `Node.PreservedPage`.
///
/// PORT NOTE (WS7 B6): the oracle's `owned` arm carries the `Allocator` that
/// produced the decode buffer. rustty allocates it from the global allocator,
/// so the arm carries the raw pointer/length needed to release it instead —
/// forming a `&mut [u8]` over a decoded page would alias the `Page` value that
/// names the same bytes.
///
/// PORT NOTE (WS7 M2): `#[must_use]` rather than a `Drop` impl. The oracle's
/// `PreservedPage` is released by an explicit `deinit()` (its `owned` arm holds
/// the allocator that made the buffer), and a `Drop` would diverge from that
/// contract — it would silently free on every early exit, including paths the
/// oracle deliberately leaves to the caller. `#[must_use]` keeps the explicit
/// `deinit()` discipline while making a dropped-on-the-floor value a warning.
/// Today only tests and the inspector construct one; Phase C5 adds real callers.
#[must_use = "a PreservedPage owns a decoded page buffer; call `deinit()`"]
// WS19 (audit row N11): `clippy::large_enum_variant` — `Owned` embeds a whole `Page`
// by value while `Borrowed` is one pointer. That asymmetry is DELIBERATE and mirrors the
// reference's `Node.PreservedPage`: the resident case must not allocate or copy anything
// (it hands back a borrowed pointer into the live node), and the compressed case must own
// the decoded page outright so the node's storage is left untouched. Boxing `Owned` —
// the lint's fix — would add a heap indirection to every compressed-page inspection and
// split ownership of the decoded page from the buffer `deinit` frees, for no benefit:
// this value is a short-lived stack local, never stored in a collection.
#[allow(clippy::large_enum_variant)]
pub enum PreservedPage {
    Borrowed(*const Page),
    Owned {
        page: Page,
        ptr: *mut u8,
        len: usize,
    },
}

impl PreservedPage {
    /// Return the read-only page represented by this value.
    #[inline]
    pub fn page(&self) -> *const Page {
        match self {
            PreservedPage::Borrowed(p) => *p,
            PreservedPage::Owned { page, .. } => page as *const Page,
        }
    }

    /// Release storage owned by this preserved page.
    pub fn deinit(&mut self) {
        if let PreservedPage::Owned { ptr, len, .. } = *self {
            // The clone buffer came from the global allocator rather than the
            // page allocator, so it must not use `Page::deinit`.
            let layout = Layout::from_size_align(len, PAGE_SIZE_MIN).unwrap();
            // SAFETY: `ptr`/`layout` are the pair `page_preserving_state`
            // allocated below, and the decoded `Page` is dropped with `self`.
            unsafe { std::alloc::dealloc(ptr, layout) };
        }
        // self.* = undefined;
        *self = PreservedPage::Borrowed(core::ptr::null());
    }
}

/// How `Node::restore` should treat the compressed representation.
///
/// PORTS: oracle@main PageList.zig `Node.RestoreMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RestoreMode {
    /// Decode the compressed representation back into the raw mapping.
    Preserve,

    /// Discard the compressed representation without decoding it. The caller
    /// must not read the page contents before overwriting them.
    Discard,
}

/// Options for [`PageList::create_page`] / `create_page_ext`.
///
/// PORTS: oracle@main PageList.zig `CreatePage` (16e4b5e98).
#[derive(Debug, Clone, Copy)]
pub(crate) struct CreatePage {
    /// The capacity to allocate the page with.
    pub cap: Capacity,

    /// Force the page backing memory to be an exact-size heap allocation even
    /// if it would fit within a standard-size pool item. This is used when
    /// compacting pages to their minimum size, since a pool item always retains
    /// a full `std_size` buffer regardless of the page layout.
    pub exact_size: bool,
}

impl CreatePage {
    /// `.{ .cap = cap }` — the oracle's default (`exact_size = false`).
    #[inline]
    pub(crate) fn new(cap: Capacity) -> CreatePage {
        CreatePage {
            cap,
            exact_size: false,
        }
    }

    /// `.{ .cap = cap, .exact_size = true }`.
    #[inline]
    pub(crate) fn exact(cap: Capacity) -> CreatePage {
        CreatePage {
            cap,
            exact_size: true,
        }
    }
}

impl Node {
    /// Return the terminal page stored in this node.
    ///
    /// WARNING: this will DECOMPRESS compressed pages! Only use it if you need
    /// access to the underlying memory. If you only need metadata (row/column
    /// counts etc.) use `metadata`/`rows`/`cols`/`capacity`.
    ///
    /// PORTS: oracle@main PageList.zig `Node.page`.
    ///
    /// PORT NOTE (WS7 B5): every accessor below takes a RAW `Node` pointer, per
    /// this port's raw-pointer discipline — a `&mut Node` would be a live
    /// exclusive alias of memory that `*mut Page` also names. The bodies return
    /// the resident page directly while `data` is still a plain `Page`; WS7 B6
    /// changes ONLY these bodies.
    ///
    /// # Safety
    /// `node` must point at a live `Node`.
    #[inline]
    pub unsafe fn page(node: *mut Node) -> *mut Page {
        let data = unsafe { core::ptr::addr_of_mut!((*node).data) };
        match unsafe { NodeData::tag(data) } {
            NodeData::TAG_RESIDENT => unsafe { NodeData::resident_payload(data) as *mut Page },
            _ => unsafe { Node::restore(node, RestoreMode::Preserve) },
        }
    }

    /// Return the terminal page only when its raw memory is resident, else
    /// null.
    ///
    /// Unlike `page`, this never restores a compressed node. This is useful for
    /// diagnostics which can display node metadata without touching the
    /// discarded mapping, but may inspect page contents when they are already
    /// available.
    ///
    /// PORTS: oracle@main PageList.zig `Node.pageIfResident`.
    ///
    /// # Safety
    /// `node` must point at a live `Node`.
    #[inline]
    pub unsafe fn page_if_resident(node: *mut Node) -> *mut Page {
        let data = unsafe { core::ptr::addr_of_mut!((*node).data) };
        match unsafe { NodeData::tag(data) } {
            NodeData::TAG_RESIDENT => unsafe { NodeData::resident_payload(data) as *mut Page },
            _ => core::ptr::null_mut(),
        }
    }

    /// Return a page which the caller KNOWS is already resident.
    ///
    /// This avoids the representation check in hot paths which already hold
    /// live pointers into the page mapping. Such pointers are only valid while
    /// the page is resident. Prefer `page` unless the caller can establish that
    /// invariant independently.
    ///
    /// PORTS: oracle@main PageList.zig `Node.pageAssumeResident`.
    ///
    /// # Safety
    /// `node` must point at a live `Node` whose page is resident.
    #[inline]
    pub unsafe fn page_assume_resident(node: *mut Node) -> *mut Page {
        let data = unsafe { core::ptr::addr_of_mut!((*node).data) };
        match unsafe { NodeData::tag(data) } {
            NodeData::TAG_RESIDENT => unsafe { NodeData::resident_payload(data) as *mut Page },
            // PORT NOTE (WS7 M3): a hard `unreachable!` rather than
            // `unreachable_unchecked`. There are exactly two call sites, both
            // immediately after a `NodeData::Resident` write, so the branch is
            // trivially predicted and the check costs nothing measurable — but
            // getting it wrong would hand callers a garbage `*mut Page`
            // instead of a panic.
            _ => unreachable!("page_assume_resident on a compressed node"),
        }
    }

    /// Return the embedded Page metadata without restoring its memory.
    ///
    /// PORTS: oracle@main PageList.zig `Node.metadata`.
    ///
    /// # Safety
    /// `node` must point at a live `Node`.
    #[inline]
    pub unsafe fn metadata(node: *const Node) -> *const Page {
        let data = unsafe { core::ptr::addr_of!((*node).data) };
        match unsafe { NodeData::tag(data) } {
            NodeData::TAG_RESIDENT => unsafe { NodeData::resident_payload(data) },
            _ => {
                let c = unsafe { NodeData::compressed_payload(data) };
                // Raw place projection — forms no `&CompressedPage`.
                unsafe { core::ptr::addr_of!((*c).page) }
            }
        }
    }

    /// Return the node's backing-memory representation without restoring it.
    ///
    /// PORTS: oracle@main PageList.zig `Node.storage`.
    ///
    /// # Safety
    /// `node` must point at a live `Node`.
    #[inline]
    pub unsafe fn storage(node: *const Node) -> Storage {
        let data = unsafe { core::ptr::addr_of!((*node).data) };
        match unsafe { NodeData::tag(data) } {
            NodeData::TAG_RESIDENT => Storage::Resident,
            _ => Storage::Compressed,
        }
    }

    /// PORTS: oracle@main PageList.zig `Node.isCompressed`.
    ///
    /// # Safety
    /// `node` must point at a live `Node`.
    #[inline]
    pub unsafe fn is_compressed(node: *const Node) -> bool {
        matches!(unsafe { Node::storage(node) }, Storage::Compressed)
    }

    /// Return read-only page contents without changing this node's storage.
    ///
    /// A compressed node decodes into an exact-sized, page-aligned buffer owned
    /// by the returned value. The caller must call `PreservedPage::deinit`.
    ///
    /// PORTS: oracle@main PageList.zig `Node.pagePreservingState`.
    ///
    /// # Safety
    /// `node` must point at a live `Node` and the caller must retain exclusive
    /// access to the owning PageList for the lifetime of the result.
    pub unsafe fn page_preserving_state(node: *const Node) -> PreservedPage {
        let data = unsafe { core::ptr::addr_of!((*node).data) };
        match unsafe { NodeData::tag(data) } {
            NodeData::TAG_RESIDENT => {
                PreservedPage::Borrowed(unsafe { NodeData::resident_payload(data) })
            }
            _ => {
                // SAFETY: the tag says `Compressed`. The `&CompressedPage` is
                // confined to this arm, which forms no other pointer into the
                // node, and the node's storage is unchanged throughout.
                let compressed = unsafe { &*NodeData::compressed_payload(data) };
                let len = compressed.page.memory_len();
                let layout = Layout::from_size_align(len, PAGE_SIZE_MIN).unwrap();
                // SAFETY: `len > 0` and the layout is valid.
                let ptr = unsafe { std::alloc::alloc_zeroed(layout) };
                assert!(!ptr.is_null(), "OOM allocating a preserved page buffer");

                // SAFETY: `ptr` is a live, page-aligned, unaliased buffer of
                // exactly `len` bytes.
                match unsafe { compressed.clone_buf(ptr, len) } {
                    Ok(page) => PreservedPage::Owned { page, ptr, len },
                    Err(_err) => {
                        // The encoded data was produced by our codec and remains
                        // immutable while compressed. Failure is internal
                        // corruption, matching the normal restoration boundary.
                        // SAFETY: `ptr`/`layout` are the pair allocated above.
                        unsafe { std::alloc::dealloc(ptr, layout) };
                        // log.err("failed to clone compressed page err={}", .{err});
                        panic!("failed to clone compressed terminal page");
                    }
                }
            }
        }
    }

    /// Restore this node to the resident representation.
    ///
    /// Preserve mode reconstructs the page contents and is used by `page`.
    /// Discard mode skips decoding for callers which will overwrite or destroy
    /// the page. Both modes recommit the retained mapping, free the encoded
    /// allocation, and leave the node in a valid resident state.
    ///
    /// PORTS: oracle@main PageList.zig `Node.restore` (`noinline`).
    ///
    /// # Safety
    /// `node` must point at a live `Node`; in `Discard` mode the caller must
    /// not read the page contents before overwriting them.
    #[inline(never)]
    pub(crate) unsafe fn restore(node: *mut Node, mode: RestoreMode) -> *mut Page {
        let data = unsafe { core::ptr::addr_of_mut!((*node).data) };

        // Read what we need out of the compressed value and drop the borrow
        // before overwriting `data`: keeping a reference into the union live
        // across the write would be Stacked-Borrows UB.
        if unsafe { NodeData::tag(data) } == NodeData::TAG_RESIDENT {
            return unsafe { Node::page_assume_resident(node) };
        }
        let (mem_ptr, mem_len) = {
            // SAFETY: the tag says `Compressed`. This borrow is confined to the
            // two field reads below and ends before anything else touches the
            // node.
            let c = unsafe { &*NodeData::compressed_payload(data) };
            (c.page.memory_ptr() as *mut u8, c.page.memory_len())
        };

        // Decommit only discarded the physical pages. Recommit prepares the
        // still-reserved mapping for decoding or reuse by the caller.
        // SAFETY: the retained mapping is live, page-aligned and unaliased.
        unsafe { crate::mem::recommit(mem_ptr, mem_len) };

        let restored: Page = match mode {
            RestoreMode::Preserve => {
                // SAFETY: the mapping was just recommitted and is unaliased.
                match unsafe { (*NodeData::compressed_payload(data)).restore() } {
                    Ok(p) => p,
                    Err(_err) => {
                        // Compressed nodes retain the immutable encoding and
                        // exact output mapping used to create them. Any decode
                        // error is an internal codec bug or memory corruption.
                        // log.err("failed to restore compressed page err={}", .{err});
                        panic!("failed to restore compressed terminal page");
                    }
                }
            }

            RestoreMode::Discard => unsafe {
                (*NodeData::compressed_payload(data)).page.shallow_copy()
            },
        };

        // Replacing `data` drops the CompressedPage, freeing its encoding
        // (the oracle's explicit `compressed.deinit()`).
        // SAFETY: `data` points at a live, initialized `NodeData`.
        unsafe { *data = NodeData::Resident(restored) };
        unsafe { Node::page_assume_resident(node) }
    }

    /// Return the number of populated rows without accessing page memory.
    ///
    /// PORTS: oracle@main PageList.zig `Node.rows`.
    ///
    /// # Safety
    /// `node` must point at a live `Node`.
    #[inline]
    pub unsafe fn rows(node: *const Node) -> size::CellCountInt {
        unsafe { (*Node::metadata(node)).size.rows }
    }

    /// Return the current column count without accessing page memory.
    ///
    /// PORTS: oracle@main PageList.zig `Node.cols`.
    ///
    /// # Safety
    /// `node` must point at a live `Node`.
    #[inline]
    pub unsafe fn cols(node: *const Node) -> size::CellCountInt {
        unsafe { (*Node::metadata(node)).size.cols }
    }

    /// Return the page capacity without accessing page memory.
    ///
    /// PORTS: oracle@main PageList.zig `Node.capacity`.
    ///
    /// # Safety
    /// `node` must point at a live `Node`.
    #[inline]
    pub unsafe fn capacity(node: *const Node) -> Capacity {
        unsafe { (*Node::metadata(node)).capacity }
    }
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

/// `std.heap.page_size_min` for the build target (4 KiB on most targets, 16 KiB
/// on aarch64 Apple). The page buffer pool aligns standard and non-standard page
/// buffers to this.
//
// PORT NOTE: re-uses the canonical `page::PAGE_SIZE_MIN` (derived per target
// from `std.heap.page_size_min_default`) so the page-buffer pool alignment and
// `Page.layout` agree by construction instead of being a duplicated literal.
use crate::page::PAGE_SIZE_MIN;

/// The memory pool we use for page memory buffers. We use a separate pool
/// so we can allocate these with a page allocator. We have to use a page
/// allocator because we need memory that is zero-initialized and page-aligned.
//
// PORT NOTE (audit #1/#2/#3): the reference is
// `PagePool = std.heap.MemoryPoolAligned([std_size]u8, .fromByteUnits(page_size_min))`
// backed by `arena.child_allocator` (the page allocator). The faithful Rust
// mapping is `DynamicMemoryPoolAligned<PageAllocator>` constructed over
// `rustty_os::page_allocator::page_allocator()`:
//
//   * It owns a `rustty_zigstd::ArenaAllocator<PageAllocator>` plus a LIFO
//     free-list, exactly like `MemoryPoolAligned`. `item_size` is `std_size`
//     and `item_alignment` is `page_size_min`.
//   * The item SIZE is supplied at construction because the Rust `std_size` is a
//     RUNTIME value (it cannot be a const-generic `[std_size]u8` array length);
//     this is the closest faithful mapping (see `DynamicMemoryPoolAligned`).
//   * STANDARD pages draw a slot from the pool (`pages.create()`); they are NOT
//     eagerly zeroed — a fresh slot is lazily OS-zeroed mmap memory (audit #2,
//     removes the eager per-page RSS commit), a reused free-list slot is zeroed
//     by `destroyNodeExt` before it re-enters the free-list.
//   * NON-STANDARD pages bypass the pool: allocated/freed directly via
//     `pages.child_allocator()` (the reference `pages.arena.child_allocator`).
//   * OOM is FALLIBLE (audit #1): `pages.create()` and the direct page-allocator
//     `allocate` return errors that propagate as `OutOfMemory`.
//   * `reset(.retain_with_limit)` resets the arena and then zeroes the retained
//     arena buffers (audit #3): retained memory is not re-`mmap`ed so it is not
//     implicitly OS-zeroed (the reference `@memset(0)` over the arena BufNodes).
//
// PORT NOTE (leak gate, R6): the reference `pageAllocator()` returns
// `std.testing.allocator` under `builtin.is_test` so the test runner detects any
// leaked page. We reproduce that faithfully UNDER TEST: the page pool's backing
// (child) allocator is `PageBackingAlloc`, which is a plain `PageAllocator` in a
// normal build but a `rustty_zigstd::TrackingAllocator<PageAllocator>` under
// `cfg(test)`. The tracking wrapper records into the process-global
// [`page_tracking::PAGE_TRACKER`] so a test can assert ZERO outstanding page
// allocations after `deinit`/`reset` — native-speed leak coverage on the heavy
// resize/reflow/clone paths where Miri times out. The wrapper changes NOTHING
// observable about the memory; it only counts. The non-`cfg(test)` library never
// references the tracker, exactly as the reference only swaps the allocator under
// `is_test`.
//
// PORT NOTE (alloc-bench seam): the OFF-BY-DEFAULT `alloc-bench` cargo feature
// compiles this same tracking seam into a NON-test build so the standalone
// `examples/alloc_bench.rs` can count page allocations with PRODUCTION page
// geometry (no libtest, `cfg!(test)` false => std_size 589824) — the equal-scope
// twin of the Zig standalone bench (.port/phase-1/bench-zig-pagelist). With the
// feature off (every production build) the `cfg(any(test, feature =
// "alloc-bench"))` sites reduce to the original `cfg(test)` split: the tracker
// is not compiled and the backing allocator is the bare `PageAllocator`.
#[cfg(not(any(test, feature = "alloc-bench")))]
type PageBackingAlloc = PageAllocator;
#[cfg(any(test, feature = "alloc-bench"))]
type PageBackingAlloc =
    rustty_zigstd::tracking_allocator::TrackingAllocator<'static, PageAllocator>;

type PagePool = DynamicMemoryPoolAligned<PageBackingAlloc>;

/// Construct the page pool's backing (child) allocator. In a normal build this is
/// just `page_allocator()`; under test (or the off-by-default `alloc-bench`
/// feature) it is the same `page_allocator()` wrapped in a `TrackingAllocator`
/// that records into the global [`page_tracking`] tracker (the faithful map of
/// `pageAllocator()` returning `std.testing.allocator`).
#[inline]
fn page_backing_alloc() -> PageBackingAlloc {
    #[cfg(not(any(test, feature = "alloc-bench")))]
    {
        page_allocator()
    }
    #[cfg(any(test, feature = "alloc-bench"))]
    {
        rustty_zigstd::tracking_allocator::TrackingAllocator::with_state(
            page_allocator(),
            page_tracking::tracker_static(),
        )
    }
}

/// TEST-ONLY (and `alloc-bench`-feature) page-allocation tracker — the scoped
/// faithful equivalent of Zig's `std.testing.allocator` leak check on the page
/// path (R6). Every page the pool (standard pages) AND the direct non-standard
/// path allocate/free is counted here, so a test can assert ZERO outstanding
/// after `deinit`/`reset` — and the standalone `alloc_bench` example can read
/// deterministic page-alloc counts outside the test harness.
#[cfg(any(test, feature = "alloc-bench"))]
pub mod page_tracking {
    use rustty_zigstd::tracking_allocator::TrackerState;

    thread_local! {
        // THREAD-LOCAL tracker shared by the page pool's `TrackingAllocator` child
        // and the direct non-standard page alloc/free path on THIS thread, so BOTH
        // page sources of a single PageList feed one outstanding-count — exactly
        // like Zig's single `pageAllocator()`/`std.testing.allocator`. It is
        // thread-local (not a process-global static) so concurrently-running tests
        // — `cargo test` runs each test on its own thread — get ISOLATED counts and
        // do not pollute each other's outstanding totals. A PageList and all its
        // page churn live on the thread that created it, so all of its page
        // alloc/free hit the same thread-local tracker.
        static PAGE_TRACKER: TrackerState = const { TrackerState::new() };
    }

    /// Run `f` with a borrow of the current thread's page tracker.
    ///
    /// `pub` (not `pub(crate)`) so the feature-gated `alloc_bench` example —
    /// compiled as an external crate — can read the counters.
    pub fn with_tracker<R>(f: impl FnOnce(&TrackerState) -> R) -> R {
        PAGE_TRACKER.with(|t| f(t))
    }

    /// Obtain a `&'static TrackerState` for the current thread's page tracker.
    ///
    /// The `TrackingAllocator` child stored in the page pool needs a
    /// `&'static TrackerState`. A `thread_local!` value is `'static` for the life
    /// of the thread; we extend the borrow with a pointer read inside the
    /// `with(...)` closure. This is sound for our use: the page pool (and thus the
    /// wrapper holding this reference) never outlives the thread that created it
    /// (a PageList is not `Send` across the wrapper boundary in these tests), so
    /// the thread-local outlives every use of the returned reference.
    pub(crate) fn tracker_static() -> &'static TrackerState {
        PAGE_TRACKER.with(|t| {
            // SAFETY: `t` points at this thread's thread-local `TrackerState`,
            // which lives until the thread exits. The page pool holding the
            // returned reference is dropped before the thread exits (it is owned by
            // a test-local PageList), so the reference never dangles.
            let p: *const TrackerState = t;
            unsafe { &*p }
        })
    }

    /// A scope guard for leak-checking page churn in a single test. Snapshots the
    /// thread-local tracker on construction; on `assert_returned_to_baseline` it
    /// asserts the outstanding counts came back to the snapshot (i.e. every page
    /// allocated during the scope was freed). A baseline delta (rather than
    /// absolute zero) tolerates any page allocations that pre-existed the scope on
    /// this thread; the thread-local isolation means no OTHER test perturbs it.
    //
    // `cfg(test)` within the `any(test, feature = "alloc-bench")` module: the
    // scope guard is a TEST helper only (the standalone bench reads the tracker
    // directly), so a feature-only build does not compile it.
    #[cfg(test)]
    pub(crate) struct PageLeakScope {
        base_allocs: usize,
        base_bytes: usize,
    }

    #[cfg(test)]
    impl PageLeakScope {
        pub(crate) fn new() -> PageLeakScope {
            with_tracker(|t| PageLeakScope {
                base_allocs: t.outstanding_allocs(),
                base_bytes: t.outstanding_bytes(),
            })
        }

        /// Outstanding page allocations created since this scope began.
        pub(crate) fn delta_allocs(&self) -> isize {
            with_tracker(|t| t.outstanding_allocs() as isize - self.base_allocs as isize)
        }

        /// Assert every page allocated since this scope began has been freed.
        #[track_caller]
        pub(crate) fn assert_returned_to_baseline(&self) {
            with_tracker(|t| {
                let da = t.outstanding_allocs() as isize - self.base_allocs as isize;
                let db = t.outstanding_bytes() as isize - self.base_bytes as isize;
                assert!(
                    da == 0 && db == 0,
                    "page leak detected: {da} page allocation(s) / {db} byte(s) not \
                     returned to the page pool after deinit/reset (served {} alloc, \
                     {} free overall on this thread)",
                    t.total_allocs(),
                    t.total_frees(),
                );
            });
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
    // PORT NOTE (audit #1/#2): the reference `pageAllocator()` selects the VM
    // page allocator for the page pool: the testing allocator under test (leak
    // detection), the std page allocator on non-macOS, and the mach tagged page
    // allocator on macOS. The Rust port reproduces this faithfully in
    // `rustty_os::page_allocator::page_allocator()`: a `mmap`-backed, page-aligned,
    // LAZILY OS-zeroed allocator (tagged on macOS), with a `cfg(miri)` std-heap
    // branch standing in for the reference's `is_test` testing-allocator. The
    // `PagePool` is built over it (see `MemoryPool::init`), so fresh standard
    // page buffers are lazily OS-zeroed (no eager memset / RSS commit). The
    // Miri/ASan gates (R6) replace the testing-allocator leak detection.

    // PORT NOTE (audit #1, init signature): the reference
    // `MemoryPool.init(gen_alloc, page_alloc, preheat)` threads the
    // general-purpose allocator into the Node/Pin pools and the page allocator
    // into the PagePool. We mirror that wiring: the Node/Pin pools take `Global`
    // (the faithful map of the Zig gpa — exactly the prior `std::alloc` path),
    // and the PagePool takes `rustty_os::page_allocator::page_allocator()` (the
    // reference `pageAllocator()` — mmap, page-aligned, LAZILY OS-zeroed). The
    // page allocator is selected internally rather than passed by the caller
    // because, unlike Zig, the Rust `pageAllocator()` is a process-wide fact
    // (R1 additive-glue boundary); the threading is otherwise identical.
    pub fn init(preheat: usize) -> Result<MemoryPool, OutOfMemory> {
        let node_pool = NodePool::init_preheated(Global, preheat).map_err(|_| OutOfMemory)?;
        // errdefer node_pool.deinit();
        let page_pool =
            PagePool::init_preheated(page_backing_alloc(), std_size(), PAGE_SIZE_MIN, preheat)
                .map_err(|_| OutOfMemory)?;
        // errdefer page_pool.deinit();
        let pin_pool = PinPool::init_preheated(Global, 8).map_err(|_| OutOfMemory)?;
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
    // the `MemoryPool` runs the `pages`/`nodes`/`pins` destructors. The
    // reference frees them in the order `pages`, `nodes`, `pins`, but that
    // order is not observable: each sub-pool is a self-contained
    // `MemoryPoolAligned` arena over its own child allocator with no
    // cross-pool references, so the destructors are independent and may run in
    // any order. (Rust happens to drop fields in declaration order, which is
    // the same order, but nothing depends on it.)

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
        use rustty_zigstd::arena_allocator::ResetMode;
        // `_ = self.pool.pages.reset(.{ .retain_with_limit = ... });`
        self.pages.reset(ResetMode::RetainWithLimit(limit_bytes));
    }

    /// Zero the retained page-pool arena memory after a reset. Faithful map of
    /// `PageList.reset`'s arena-zeroing block: retained arena memory is not
    /// re-`mmap`ed so it is not implicitly OS-zeroed (the reference `@memset(0)`
    /// walking `pool.pages.arena.state.buffer_list`).
    fn zero_retained_page_arena(&mut self) {
        self.pages.zero_retained_arena();
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

    /// The next globally unique reference generation for this PageList. A
    /// generation is assigned whenever a page is allocated, reused as new, or
    /// changed in place such that existing page coordinates are no longer
    /// stable. The serial is assigned to the Node.
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

    /// The first serial in the current whole-list validity epoch. Only `reset`
    /// advances this value, immediately before rebuilding every page. It is not
    /// the generation of the first page or the exact minimum live generation.
    /// Page generations are not monotonic in list order: replacement and split
    /// operations can put a fresh generation before older live pages.
    ///
    /// A generation below this epoch is definitely invalid, allowing O(1)
    /// rejection in `node_is_valid` and bulk removal of pre-reset search
    /// results. A generation at or above it is only potentially valid and must
    /// still be checked against the live list before its coordinates are used.
    ///
    /// PORTS: oracle@main PageList.zig `page_serial_epoch` (f6d9a582e).
    pub page_serial_epoch: u64,

    /// Continuation state for incremental page compression. This allows
    /// `compress(CompressionMode::Incremental)` to work. More details on all
    /// that there and on `IncrementalCompressionState`.
    pub page_compression: compress::IncrementalCompressionState,

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

/// Represents an exact x/y coordinate within the screen. This is called
/// a "pin" because it is a fixed point within the pagelist direct to
/// a specific page pointer and memory offset. The benefit is that this
/// point remains valid even through scrolling without any additional work.
///
/// A downside is that  the pin is only valid until the pagelist is modified
/// in a way that may invalidate page pointers or shuffle rows, such as resizing,
/// erasing rows, etc.
///
/// A pin can also be "tracked" which means that it will be updated as the
/// PageList is modified.
///
/// The PageList maintains a list of active pin references and keeps them
/// all up to date as the pagelist is modified. This isn't cheap so callers
/// should limit the number of active pins as much as possible.
//
// PORT NOTE: the full `Pin` body (with `rowAndCell`, `cells`, `isBetween`,
// `before`, `leftWrap`, `rightWrap`, `downOverflow`, `upOverflow`, etc.) lands
// in `pin.rs` (the pin cluster). For the CORE methods we need the field layout
// (`node`/`y`/`x`/`garbage`) and the `pinIsValid` walk; both are defined here.
// PORT NOTE: Zig `Pin` is a trivially-copyable value type (passed/stored by
// value throughout Selection/Screen). `Clone`/`Copy` mirror that — the `node`
// raw pointer is a non-owning back-reference, so copying is bit-for-bit faithful.
// PORT NOTE: `PartialEq`/`Eq`/`Debug` mirror Zig's automatic struct equality and
// formatting (used by the formatter tests' `expectEqual(Pin{...}, ...)`). The
// `node` raw pointer compares by address (pointer identity), matching Zig.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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

// const init_tw = tripwire.module(enum {
//     init_memory_pool,
//     init_pages,
//     viewport_pin,
//     viewport_pin_track,
// }, init);
//
// PORT NOTE: the Zig `tripwire.module(P, E)` is a comptime function returning a
// type with type-static state. The Rust `rustty_util::tripwire::Module<P, E>` is
// an INSTANCE holding the same state (see its PORT NOTE). To share that state
// between `init` (which calls `check`) and the `PageList init error` test (which
// calls `error_always`), we hold each module in a `#[cfg(test)] thread_local!`
// and route all access through the small `cfg(test)` helper fns below. In
// non-test builds the helpers are no-ops returning `Ok(())`, mirroring the Zig
// `enabled = builtin.is_test` gate (the `Module::enabled()` short-circuit).
#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum InitFp {
    InitMemoryPool,
    InitPages,
    ViewportPin,
    ViewportPinTrack,
}

// const initPages_tw = tripwire.module(enum {
//     page_node,
//     page_buf_std,
//     page_buf_non_std,
// }, initPages);
#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
// PORT NOTE: variant names mirror the reference enum tags `page_node`,
// `page_buf_std`, `page_buf_non_std` verbatim; the shared `Page` prefix is
// intrinsic to those names, so the `enum_variant_names` lint is suppressed.
#[allow(clippy::enum_variant_names)]
enum InitPagesFp {
    PageNode,
    PageBufStd,
    PageBufNonStd,
}

#[cfg(test)]
thread_local! {
    static INIT_TW: core::cell::RefCell<rustty_util::tripwire::Module<InitFp, OutOfMemory>> =
        core::cell::RefCell::new(rustty_util::tripwire::module());
    static INIT_PAGES_TW: core::cell::RefCell<rustty_util::tripwire::Module<InitPagesFp, OutOfMemory>> =
        core::cell::RefCell::new(rustty_util::tripwire::module());
}

// `try tw.check(point)` for the `init` tripwire module. Only compiled in test
// builds; the call sites are `#[cfg(test)]`-gated so production `init` is
// unchanged, mirroring the Zig `enabled = builtin.is_test` gate where the
// non-test `check` is a no-op.
#[cfg(test)]
#[inline]
fn init_tw_check(point: InitFp) -> Result<(), OutOfMemory> {
    INIT_TW.with(|tw| tw.borrow_mut().check(point))
}

// `try tw.check(point)` for the `initPages` tripwire module.
#[cfg(test)]
#[inline]
fn init_pages_tw_check(point: InitPagesFp) -> Result<(), OutOfMemory> {
    INIT_PAGES_TW.with(|tw| tw.borrow_mut().check(point))
}

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

        // PORT NOTE: the reference `PagePool.item_size` is a comptime const
        // equal to `std_size` (`@sizeOf([std_size]u8)`); we use `std_size()`
        // directly here (static context, no pool instance available).
        std_size() * pages
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
        #[cfg(test)]
        init_tw_check(InitFp::InitMemoryPool)?;
        let mut pool = MemoryPool::init(PAGE_PREHEAT)?;
        // errdefer pool.deinit();

        // try tw.check(.init_pages);
        #[cfg(test)]
        init_tw_check(InitFp::InitPages)?;
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
        #[cfg(test)]
        init_tw_check(InitFp::ViewportPin)?;
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
        #[cfg(test)]
        init_tw_check(InitFp::ViewportPinTrack)?;
        tracked_pins.put_no_clobber(viewport_pin.as_ptr(), ());

        // errdefer comptime unreachable;
        let result: PageList = PageList {
            cols,
            rows,
            pool,
            pages: page_list,
            page_serial,
            page_serial_epoch: 0,
            page_compression: compress::IncrementalCompressionState::default(),
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
            #[cfg(test)]
            if let Err(e) = init_pages_tw_check(InitPagesFp::PageNode) {
                // errdefer: free every non-standard page in the partial list.
                Self::free_non_standard_pages(&mut page_list);
                return Err(e);
            }
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
                #[cfg(test)]
                if let Err(e) = init_pages_tw_check(InitPagesFp::PageBufStd) {
                    // errdefer: node is in the pool (freed on pool Drop); free
                    // every non-standard page in the partial list.
                    Self::free_non_standard_pages(&mut page_list);
                    return Err(e);
                }
                // const buf = try pool.pages.create();
                // recommitPoolItem(buf);
                match pool.pages.create() {
                    Ok(b) => {
                        recommit_pool_item(b.as_ptr(), std_size());
                        b
                    }
                    Err(_) => {
                        Self::free_non_standard_pages(&mut page_list);
                        return Err(OutOfMemory);
                    }
                }
            } else {
                // try tw.check(.page_buf_non_std);
                #[cfg(test)]
                if let Err(e) = init_pages_tw_check(InitPagesFp::PageBufNonStd) {
                    // errdefer: node is in the pool (freed on pool Drop); free
                    // every non-standard page already appended in the partial
                    // list (this iteration's buffer was never allocated).
                    Self::free_non_standard_pages(&mut page_list);
                    return Err(e);
                }
                // break :buf try page_alloc.alignedAlloc(
                //     u8,
                //     .fromByteUnits(std.heap.page_size_min),
                //     layout.total_size,
                // );
                match Self::alloc_non_standard(layout.total_size) {
                    Ok(b) => b,
                    Err(_) => {
                        Self::free_non_standard_pages(&mut page_list);
                        return Err(OutOfMemory);
                    }
                }
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
            // PORT NOTE (audit #2/#4 lazy-zero): a POOLED buffer is NOT eagerly
            // zeroed — a fresh slot is lazily OS-zeroed mmap memory (no eager
            // per-page RSS commit) and a reused free-list slot was zeroed by
            // `destroyNodeExt` before it re-entered the free-list, so handed-out
            // pages are always observably zero. A NON-STANDARD buffer is a fresh
            // `page_allocator().allocate` (lazily OS-zeroed mmap) too. We DON'T
            // run runtime-safety/freestanding eager memsets here (those builds
            // are not targeted by this port), matching the native mmap path.

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
                    data: NodeData::Resident(data),
                    serial: *serial,
                    owned: if pooled { Owned::Pool } else { Owned::Heap },
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
            panic!("PageList integrity check failed: {_err:?}");
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
                actual_total += unsafe { Node::rows(node) as usize };
                node_ = unsafe { (*node).next };

                // Every live node must belong to the current validity epoch.
                if unsafe { (*node).serial } < self.page_serial_epoch {
                    // log.warn(
                    //     "PageList integrity violation: page serial predates epoch serial={} epoch={}",
                    //     .{ node.serial, self.page_serial_epoch },
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
                    offset += unsafe { Node::rows(n) as usize };
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

    /// Deinit the pagelist, freeing all page memory and the memory pool.
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
        // heap-owned (not in the pool).
        //
        // PORT NOTE (WS7 B6): every node is discard-restored first, pool-owned
        // ones included — that is what frees a compressed node's encoded
        // allocation. Node storage lives in a raw memory pool that never runs
        // `Drop`, so skipping this would leak the encoding.
        let mut it = self.pages.first;
        while !it.is_null() {
            let node = it;
            // SAFETY: `node` is a live node of this list.
            let page = unsafe { Node::restore(node, RestoreMode::Discard) };
            if matches!(unsafe { (*node).owned }, Owned::Heap) {
                Self::free_page_memory(page);
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

        // Reset discards all scrollback, so there is nothing left to compress.
        self.page_compression.reset();

        // Begin a new whole-list validity epoch before rebuilding from the pools.
        // Every old reference now has a serial below the epoch and can be rejected
        // in O(1), even if the node pool later reuses its pointer address.
        self.page_serial_epoch = self.page_serial;

        // We need enough pages/nodes to keep our active area. This should
        // never fail since we by definition have allocated a page already
        // that fits our size but I'm not confident to make that assertion.
        let cap = Self::initial_capacity(self.cols);
        assert!(cap.rows > 0);

        // The number of pages we need is the number of rows in the active
        // area divided by the row capacity of a page.
        let page_count: usize = (self.rows as usize).div_ceil(cap.rows as usize);

        // Before resetting our pools we need to free any pages that
        // are heap-owned since those were allocated outside the pool.
        //
        // PORT NOTE (WS7 B6): as in `deinit`, every node is discard-restored so
        // a compressed node's encoded allocation is released.
        {
            let mut it = self.pages.first;
            while !it.is_null() {
                let node = it;
                // SAFETY: `node` is a live node of this list.
                let page = unsafe { Node::restore(node, RestoreMode::Discard) };
                if matches!(unsafe { (*node).owned }, Owned::Heap) {
                    Self::free_page_memory(page);
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
        // WARN: Since HeapAllocator's BufNode is not public API,
        // we have to hardcode its layout here. We do a comptime assert
        // on Zig version to verify we check it on every bump.
        //
        // The buffer minus our header
        //
        // PORT NOTE: the reference reads the private `ArenaAllocator.BufNode`
        // layout to zero the retained arena memory after `pages.reset`. Our
        // `DynamicMemoryPoolAligned` owns a faithful `ArenaAllocator` and exposes
        // `zero_retained_arena()`, which walks the SAME backing-node list and
        // `@memset(0)`s each node's usable region — reproducing that behavior
        // without hardcoding a private std layout.
        self.pool
            .reset_pages_retain_with_limit(page_count * self.pool.pages.item_size());
        self.pool
            .reset_nodes_retain_with_limit(page_count * NodePool::ITEM_SIZE);
        self.pool.zero_retained_page_arena();

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
    fn create_page(&mut self, opts: CreatePage) -> Result<*mut Node, OutOfMemory> {
        // log.debug("create page cap={}", .{opts.cap});
        Self::create_page_ext(
            &mut self.pool,
            opts,
            &mut self.page_serial,
            Some(&mut self.page_size),
        )
    }

    #[inline]
    #[allow(dead_code)]
    fn create_page_ext(
        pool: &mut MemoryPool,
        opts: CreatePage,
        serial: &mut u64,
        total_size: Option<&mut usize>,
    ) -> Result<*mut Node, OutOfMemory> {
        let page = pool.nodes.create().map_err(|_| OutOfMemory)?;
        // errdefer pool.nodes.destroy(page);

        let cap = opts.cap;
        let layout = Page::layout(cap);
        let pooled = !opts.exact_size && layout.total_size <= std_size();

        // It would be better to encode this into the Zig error handling
        // system but that is a big undertaking and we only have a few
        // centralized call sites so it is handled on its own currently.
        assert!(layout.total_size <= size::MAX_PAGE_SIZE as usize);

        // Our page buffer comes from our standard memory pool if it
        // is within our standard size since this is what the pool
        // dispenses. Otherwise, we use the heap allocator to allocate.
        //
        // PORT NOTE (audit #1 fallible OOM): both `pool.pages.create()` and the
        // direct `page_allocator().allocate` (via `alloc_non_standard`) are
        // fallible. On failure we honor the reference `errdefer
        // pool.nodes.destroy(page)` — the node is returned to the node pool —
        // and propagate `OutOfMemory`.
        let page_buf: NonNull<u8> = if pooled {
            // const buf = try pool.pages.create();
            // recommitPoolItem(buf);
            match pool.pages.create() {
                Ok(b) => {
                    recommit_pool_item(b.as_ptr(), std_size());
                    b
                }
                Err(_) => {
                    // errdefer pool.nodes.destroy(page).
                    unsafe { pool.nodes.destroy(page) };
                    return Err(OutOfMemory);
                }
            }
        } else {
            // break :buf try page_alloc.alignedAlloc(...);
            match Self::alloc_non_standard(layout.total_size) {
                Ok(b) => b,
                Err(_) => {
                    // errdefer pool.nodes.destroy(page).
                    unsafe { pool.nodes.destroy(page) };
                    return Err(OutOfMemory);
                }
            }
        };
        // errdefer if (pooled)
        //     pool.pages.destroy(page_buf)
        // else
        //     page_alloc.free(page_buf);

        // In runtime safety modes, allocators fill with 0xAA. On freestanding
        // (WASM), the WasmAllocator reuses freed slots without zeroing.
        //
        // PORT NOTE (audit #2/#4 lazy-zero): handed-out page buffers are always
        // observably zero, but FRESH ones get there lazily: a fresh pool slot is
        // lazily OS-zeroed mmap memory (no eager memset / RSS commit), a reused
        // pool slot was zeroed by `destroyNodeExt` before re-entering the
        // free-list, and a non-standard buffer is a fresh lazily-OS-zeroed mmap.
        // We don't run the runtime-safety/freestanding eager memsets here.

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
                data: NodeData::Resident(data),
                serial: *serial,
                owned: if pooled { Owned::Pool } else { Owned::Heap },
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
        // Restore in discard mode: the contents are about to be destroyed or
        // handed back to the pool, so decoding them would be wasted work. This
        // also frees any encoded allocation the node still owns.
        //
        // SAFETY: `node` is live and already removed from the linked list.
        let page: *mut Page = unsafe { Node::restore(node, RestoreMode::Discard) };
        let owned = unsafe { (*node).owned };
        let (ptr, len) = Self::page_memory(page);

        // Update our accounting for page size. This must mirror what was added
        // at creation time: a pool-owned page always accounts for a full pool
        // item even if its layout is smaller, while a heap-owned page accounts
        // for its exact memory length.
        //
        // PORT NOTE (WS7 B4): this used to subtract `page_memory_len(node)`
        // unconditionally, which under-counted the release of a pool-owned page
        // whose layout is smaller than a full pool item.
        if let Some(v) = total_size {
            *v -= match owned {
                Owned::Pool => std_size(),
                Owned::Heap => len,
            };
        }

        match owned {
            Owned::Pool => {
                debug_assert!(len <= std_size());

                // Reset the memory to zero (and decommit it, where supported) so
                // it can be reused.
                //
                // PORT NOTE (audit #4): this is the REUSED-buffer zeroing. A
                // pooled buffer returning to the free-list MUST read back as
                // zero here (the reference `terminal_mem.decommit(.zero, item,
                // page.memory.len)`) so the next `create()` reusing it hands
                // back observably-zero memory — fresh slots are lazily
                // OS-zeroed, but a reused slot is not re-`mmap`ed.
                decommit_pool_item(ptr, std_size(), len);
                // SAFETY: `ptr` was handed out by `pool.pages.create()` (a
                // standard pooled buffer) and has not been destroyed.
                unsafe { pool.pages.destroy(NonNull::new(ptr).unwrap()) };
            }

            Owned::Heap => Self::free_non_standard_page(node),
        }

        unsafe { pool.nodes.destroy(NonNull::new(node).unwrap()) };
    }

    // -- Page-buffer raw-pointer helpers (Rust port glue) ------------------
    //
    // PORT NOTE: the reference accesses `node.data.memory` (a slice) directly.
    // The Rust `Page` keeps `memory_ptr`/`memory_len`, so these small helpers
    // recover the pointer/len without forming a `&Page` (avoids retagging
    // concerns under Miri) and centralize the non-standard alloc/free.

    /// `Layout` for a NON-STANDARD page buffer of `total_size` bytes, aligned to
    /// `page_size_min`. The reference uses `page_alloc.alignedAlloc(u8,
    /// page_size_min, total_size)` / frees `node.data.memory` (which is exactly
    /// `total_size` bytes at the same alignment).
    #[inline]
    fn non_standard_layout(total_size: usize) -> Layout {
        Layout::from_size_align(total_size, PAGE_SIZE_MIN).unwrap()
    }

    /// Allocate a page-aligned, `total_size`-byte buffer for a NON-STANDARD page
    /// DIRECTLY via the page allocator, bypassing the pool — the reference
    /// `page_alloc = pool.pages.arena.child_allocator;
    /// page_alloc.alignedAlloc(...)`. The mmap'd region is lazily OS-zeroed (no
    /// eager memset). FALLIBLE: returns `OutOfMemory` on map failure.
    //
    // PORT NOTE (audit #1/#2): the page allocator is `page_allocator()`, the
    // SAME value the `PagePool` arena holds as its `child_allocator` (a stateless
    // `Copy` selector — the macOS VM tag only), so going through `page_allocator()`
    // directly here is observably identical to `pool.pages.child_allocator()`.
    // We keep `pool` in the signature to mirror the reference's
    // `pool.pages.arena.child_allocator` access and keep the seam explicit.
    fn alloc_non_standard(total_size: usize) -> Result<NonNull<u8>, OutOfMemory> {
        let layout = Self::non_standard_layout(total_size);
        // Under test, route through the SAME tracking wrapper / global tracker the
        // page pool's child uses, so non-standard pages are leak-counted too
        // (faithful: in Zig the single `pageAllocator()` serves both paths).
        let alloc = page_backing_alloc();
        let slice = alloc.allocate(layout).map_err(|_| OutOfMemory)?;
        Ok(slice.cast::<u8>())
    }

    /// Free a NON-STANDARD page's backing buffer DIRECTLY via the page allocator
    /// (the reference `page_alloc.free(node.data.memory)`).
    fn free_non_standard_page(node: *mut Node) {
        // SAFETY: `node` is live; the metadata carries the mapping to free and
        // never touches the (possibly discarded) page contents.
        let page = unsafe { Node::metadata(node) };
        Self::free_page_memory(page as *mut Page);
    }

    /// Free a heap-owned page's backing mapping, given the `Page` that names
    /// it. Split out of `free_non_standard_page` so `deinit`/`reset` can free
    /// the mapping of a page they have already discard-restored.
    fn free_page_memory(page: *mut Page) {
        let (ptr, len) = Self::page_memory(page);
        let layout = Self::non_standard_layout(len);
        let alloc = page_backing_alloc();
        // SAFETY: `ptr`/`len` describe a buffer previously returned by
        // `alloc_non_standard` (page allocator) with this exact layout.
        unsafe { alloc.deallocate(NonNull::new(ptr).unwrap(), layout) };
    }

    /// Free all non-standard pages currently in `list` (the reference
    /// `errdefer` page-cleanup loop in `initPages`/`clone`).
    fn free_non_standard_pages(list: &mut List) {
        let mut it = list.first;
        while !it.is_null() {
            let node = it;
            if matches!(unsafe { (*node).owned }, Owned::Heap) {
                Self::free_non_standard_page(node);
            }
            it = unsafe { (*node).next };
        }
    }

    /// `node.data.memory.len`.
    #[inline]
    fn page_memory_len(node: *mut Node) -> usize {
        // Metadata only: the mapping length is identical in both storage
        // states, so this must never restore a compressed node.
        unsafe { (*Node::metadata(node)).memory_len }
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

    /// Checks if a pin is valid for this pagelist. This is a very slow and
    /// expensive operation since we traverse the entire linked list in the
    /// worst case. Only for runtime safety/debug.
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
            return p.y < unsafe { Node::rows(node) } && p.x < unsafe { Node::cols(node) };
        }

        false
    }

    /// Give a live node a new generation before changing its coordinate layout
    /// in place.
    ///
    /// This must be called when a node remains at the same address and stays in
    /// the list, but a mutation changes which logical row a `(node, y)`
    /// coordinate identifies or whether that coordinate is still in range.
    /// Examples include rotating rows after an erase, truncating a page's row
    /// range, reinitializing the sole remaining page, or shortening the source
    /// page of a split. Without a new generation, a cached pointer, serial, and
    /// coordinate could still pass `node_is_valid` while referring to a
    /// different row than it originally did. Screen and Terminal fast paths
    /// which manipulate Page rows directly must use this because they bypass
    /// PageList's own row-mutation helpers.
    ///
    /// The caller must pass a node which is currently live in this PageList.
    /// Call this at the point the operation commits to changing the layout and
    /// before the first such change. When an operation is intended to be
    /// atomic, finish its failable preparation first so a failed operation does
    /// not needlessly invalidate references. One call per affected node is
    /// sufficient when an operation performs several layout changes without
    /// exposing intermediate references.
    ///
    /// This helper is only needed for in-place changes. Removing or replacing a
    /// node already invalidates old references through the live-list check in
    /// `node_is_valid`, and newly allocated or reused nodes receive a fresh
    /// serial as part of their initialization. Ordinary cell or style changes
    /// which preserve the meaning of page coordinates do not require a new
    /// generation solely for that reason.
    ///
    /// The only state changed here is `node.serial` and the next-generation
    /// counter `page_serial`. Consequently, every previously captured
    /// pointer-plus-serial pair for this node becomes invalid while new
    /// references can capture the new generation. This does not advance
    /// `page_serial_epoch`: only `reset` starts a new whole-list validity epoch.
    ///
    /// This also does not mutate the page, update tracked pins or viewport
    /// state, adjust row or memory accounting, mark cells dirty, or notify
    /// incremental compression. The surrounding operation remains responsible
    /// for all such bookkeeping required by its layout change.
    ///
    /// PORTS: oracle@main PageList.zig `invalidateNodeLayout`
    /// (a804e3ac7, 0bad91adb).
    //
    // PORT NOTE: raw-pointer argument, never `&mut Node` — callers routinely
    // hold a live alias into the same node across this call (memory:
    // `rust-mut-args-strongly-protected`).
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn invalidate_node_layout(&mut self, node: *mut Node) {
        unsafe { (*node).serial = self.page_serial };
        self.page_serial += 1;
    }

    /// Returns whether a node pointer and serial still identify the same page in
    /// this list. The node pointer is only compared and is safe even if the node
    /// has been destroyed or reused since the serial was captured.
    ///
    /// PORTS: oracle@main PageList.zig `nodeIsValid` (86e1f7b8b, f6d9a582e).
    //
    // PORT NOTE: `target` is a raw `*const Node` and is NEVER dereferenced —
    // only compared against live node addresses. This is the whole point of the
    // helper: callers hold captured pointers that may name destroyed or reused
    // nodes.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn node_is_valid(&self, target: *const Node, serial: u64) -> bool {
        // Reset invalidates the whole prior epoch, so reject those generations
        // without scanning the live list.
        if serial < self.page_serial_epoch {
            return false;
        }

        let mut it = self.pages.first;
        while !it.is_null() {
            let node = it;
            if core::ptr::eq(node as *const Node, target) {
                return unsafe { (*node).serial } == serial;
            }
            it = unsafe { (*node).next };
        }

        false
    }

    /// Mark a point as dirty, used for testing.
    //
    // PORT NOTE: `#[allow(dead_code)]` — the reference `markDirty` is a private
    // testing/debugging helper; its sole callers are test blocks that land in the
    // dedicated test phase.
    #[allow(dead_code)]
    fn mark_dirty(&self, pt: point::Point) {
        self.pin(pt).unwrap().mark_dirty();
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
                    let node_rows = unsafe { Node::rows(node) };
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
            rows += unsafe { Node::rows(node) as usize };
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

/// Zero a pool item buffer that is about to be returned to the pool free list
/// and, where supported, tell the OS it can reclaim the backing memory while
/// the buffer sits unused. Without the decommit, a pool that shrinks
/// (scrollback clears, pruning churn, resets, reflow) retains its high-water
/// RSS forever, since memory-pool free lists never return memory to the OS.
///
/// `dirty_len` is the length of the page that lived in this item, which may be
/// smaller than the item; bytes beyond it are already zero.
///
/// The invariant required for page reuse (see `create_page_ext`): after this
/// call the entire item reads back as zeroes. The pool writes its free-list
/// node into the first pointer-size bytes afterwards, which is safe because
/// page initialization always overwrites them.
///
/// PORT NOTE (WS7 B1): this was a hand-rolled `madvise` block in BL-2, written
/// before the oracle factored the same logic into `src/terminal/mem.zig`.
/// Upstream now calls `terminal_mem.decommit(.zero, item, page.memory.len)`
/// here (`oracle@main:src/terminal/PageList.zig:4379`), so this is a thin
/// wrapper over [`crate::mem::decommit`] in zero mode. Behavior is unchanged:
/// zero mode guarantees the dirty prefix reads back as zero on every target
/// (Linux via MADV_DONTNEED's zero-fill guarantee, Darwin via the memset it
/// performs before MADV_FREE_REUSABLE, everywhere else via the memset
/// fallback).
#[inline]
fn decommit_pool_item(item: *mut u8, item_size: usize, dirty_len: usize) {
    // SAFETY: `item` is a live pool item of `item_size` page-aligned,
    // page-multiple bytes with `dirty_len <= item_size`, and nothing aliases it
    // (it has already been removed from the page list).
    let _ =
        unsafe { crate::mem::decommit(crate::mem::DecommitMode::Zero, item, item_size, dirty_len) };
}

/// The counterpart to [`decommit_pool_item`] for buffers handed back out by the
/// pool. Only Darwin requires this: MADV_FREE_REUSE re-accounts previously
/// reusable memory to the process footprint. Without it the memory still works
/// (touching reusable pages revives them) but footprint reporting can
/// undercount. Harmless on buffers that were never marked reusable (fresh or
/// preheated items).
///
/// PORT NOTE (WS7 B1): as with `decommit_pool_item`, upstream now spells this
/// `terminal_mem.recommit(buf)` (`oracle@main:src/terminal/PageList.zig:690`
/// and `:3897`).
#[inline]
fn recommit_pool_item(item: *mut u8, item_size: usize) {
    // SAFETY: as above; `item` is a live, page-aligned pool item.
    unsafe { crate::mem::recommit(item, item_size) };
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
