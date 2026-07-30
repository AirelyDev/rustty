//! rustty-vt hyperlink
//!
//! PORTS: ghostty src/terminal/hyperlink.zig
//!
//! PORT STATUS: done
//!
//! PORT NOTE: the reference imports are
//!   const std = @import("std");
//!   const Allocator = std.mem.Allocator;
//!   const hash_map = @import("hash_map.zig");
//!   const AutoOffsetHashMap = hash_map.AutoOffsetHashMap;
//!   const pagepkg = @import("page.zig");
//!   const size = @import("size.zig");
//!   const Offset = size.Offset;
//!   const Cell = pagepkg.Cell;
//!   const Page = pagepkg.Page;
//!   const RefCountedSet = @import("ref_counted_set.zig").RefCountedSet;
//!   const Wyhash = std.hash.Wyhash;
//!   const autoHash = std.hash.autoHash;
//!   const autoHashStrat = std.hash.autoHashStrat;
//! `page` (Cell/Page) is its mutually-recursive sibling, ported alongside this
//! file. The offset types are the hoisted size types from rustty-render-types
//! (A1). `RefCountedSet` and `AutoOffsetHashMap` are the already-ported page
//! data structures.
//!
//! PORT NOTE: the reference `RefCountedSet` takes a per-call `ContextT` value
//! (passed to `addContext`/`lookupContext`/etc.). The ported Rust
//! `RefCountedSet` stores the context as a public field (`set.context`) and the
//! `add`/`lookup`/etc. methods read it. To reproduce the reference's per-call
//! context (which carries the `page`/`src_page` pointers needed by `hash`/
//! `eql`/`deleted`), callers mutate `set.context.page` / `set.context.src_page`
//! immediately before each call. The context here is `SetContext` with the same
//! two nullable page pointers as the reference's anonymous struct.

use std::hash::Hasher;

use rustty_render_types::term_size as size;
use rustty_render_types::term_size::{get_offset, Offset, OffsetSlice};

use crate::hash_map::AutoOffsetHashMap;
use crate::page::{Cell, Page};
use crate::ref_counted_set::{RefCountedSet, RefCountedSetContext};

/// The allocation-failure error, mirroring the reference `error{OutOfMemory}`
/// (Zig's `Allocator.Error`) returned by `PageEntry.dupe`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutOfMemory;

/// The unique identifier for a hyperlink. This is at most the number of cells
/// that can fit in a single terminal page, since each cell can only contain
/// at most one hyperlink.
pub type Id = size::HyperlinkCountInt;

// The mapping of cell to hyperlink. We use an offset hash map to save space
// since its very unlikely a cell is a hyperlink, so its a waste to store
// the hyperlink ID in the cell itself.
pub type Map = AutoOffsetHashMap<Offset<Cell>, Id, 80>;

/// A fully decoded hyperlink that may or may not have its
/// memory within a page. The memory location of this is dependent
/// on the context so users should check with the source of the
/// hyperlink.
#[derive(Clone)]
pub struct Hyperlink {
    pub id: HyperlinkId,
    pub uri: Vec<u8>,
}

/// See PageEntry.Id
// PORT NOTE: the reference is `Hyperlink.Id`, a nested union. Rust nested type
// names can't shadow the field name `id`, so it is hoisted to `HyperlinkId`.
#[derive(Clone)]
pub enum HyperlinkId {
    Explicit(Vec<u8>),
    Implicit(size::OffsetInt),
}

impl Hyperlink {
    /// Deinit and deallocate all the pointers using the given
    /// allocator.
    ///
    /// WARNING: This should only be called if the hyperlink was
    /// heap-allocated. This DOES NOT need to be unconditionally
    /// called.
    // PORT NOTE: the reference `deinit(alloc)` frees `uri` and (if explicit) the
    // id. In Rust the owned `Vec<u8>`s are freed when the `Hyperlink` is
    // dropped, so this is a no-op kept for fidelity. (The reference's allocator
    // is the Rust global allocator here.)
    pub fn deinit(&self) {
        // alloc.free(self.uri);
        // switch (self.id) { .implicit => {}, .explicit => |v| alloc.free(v) }
    }

    /// Duplicate a hyperlink by allocating all values with the
    /// given allocator. The returned hyperlink should have deinit
    /// called.
    pub fn dupe(&self) -> Hyperlink {
        let uri = self.uri.clone();
        let id: HyperlinkId = match &self.id {
            HyperlinkId::Implicit(_) => self.id.clone(),
            HyperlinkId::Explicit(v) => HyperlinkId::Explicit(v.clone()),
        };
        Hyperlink { id, uri }
    }
}

/// A hyperlink that has been committed to page memory. This
/// is a "page entry" because while it represents a hyperlink,
/// some decoding (pointer chasing) is still necessary to get the
/// fully realized ID, URI, etc.
#[derive(Clone, Copy)]
pub struct PageEntry {
    pub id: PageEntryId,
    pub uri: OffsetSlice<u8>,
}

// PORT NOTE: the reference is `PageEntry.Id`, a nested union; hoisted to
// `PageEntryId` (same reason as `HyperlinkId`).
#[derive(Clone, Copy)]
pub enum PageEntryId {
    /// An explicitly provided ID via the OSC8 sequence.
    Explicit(OffsetSlice<u8>),

    /// No ID was provided so we auto-generate the ID based on an
    /// incrementing counter attached to the screen.
    Implicit(size::OffsetInt),
}

impl Default for PageEntry {
    fn default() -> Self {
        PageEntry {
            id: PageEntryId::Implicit(0),
            uri: OffsetSlice::default(),
        }
    }
}

impl PageEntry {
    /// Duplicate this hyperlink from one page to another.
    ///
    /// # Safety
    ///
    /// `self_page` and `dst_page` must be valid pages whose backing memory
    /// contains the offsets referenced by this entry.
    pub unsafe fn dupe(
        &self,
        self_page: *const Page,
        dst_page: *mut Page,
    ) -> Result<PageEntry, OutOfMemory> {
        let mut copy = *self;

        // If the pages are the same then we can return a shallow copy.
        if std::ptr::eq(self_page, dst_page) {
            return Ok(copy);
        }

        // PORT NOTE: access the pages via field-projected helpers (see
        // `Page::string_alloc_alloc`) rather than `&*page` / `&mut *page` — `dupe`
        // is reachable from the hyperlink set context where the page's
        // `hyperlink_set` is mutably borrowed, so a whole-`Page` retag is UB.
        let self_base = unsafe { Page::memory_ptr_of(self_page) };
        let dst_base = unsafe { Page::memory_ptr_of(dst_page) };

        // Copy the URI
        {
            let uri = unsafe { self.uri.slice(self_base) };
            let buf =
                unsafe { Page::string_alloc_alloc(dst_page, uri.len()).map_err(|_| OutOfMemory)? };
            buf.copy_from_slice(uri);
            copy.uri = OffsetSlice {
                offset: unsafe { get_offset::<u8, _>(dst_base, &buf[0]) },
                len: uri.len(),
            };
        }
        // errdefer dst_page.string_alloc.free(...) — handled below on error.

        // Copy the ID
        match copy.id {
            PageEntryId::Implicit(_) => {} // Shallow is fine
            PageEntryId::Explicit(slice) => {
                let id = unsafe { slice.slice(self_base) };
                let id_len = id.len();
                let buf = match unsafe { Page::string_alloc_alloc(dst_page, id_len) } {
                    Ok(b) => b,
                    Err(_) => {
                        // errdefer for the URI allocation above.
                        unsafe {
                            let s = copy.uri.slice(dst_base);
                            Page::string_alloc_free(dst_page, s);
                        }
                        return Err(OutOfMemory);
                    }
                };
                buf.copy_from_slice(id);
                copy.id = PageEntryId::Explicit(OffsetSlice {
                    offset: unsafe { get_offset::<u8, _>(dst_base, &buf[0]) },
                    len: id_len,
                });
            }
        }

        Ok(copy)
    }

    /// # Safety
    ///
    /// `base` must be the true base of the backing memory holding the offsets
    /// referenced by this entry.
    pub unsafe fn hash<B: size::IntFromBase + Copy>(&self, base: B) -> u64 {
        // PORT NOTE: the reference uses Wyhash + autoHash/autoHashStrat. Since
        // this hash is only used internally by the ref-counted set (never
        // oracle-compared / serialized), any self-consistent hash is parity-
        // preserving. We mirror the structure (active tag, then id payload,
        // then uri bytes) using the std hasher.
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        // autoHash(&hasher, std.meta.activeTag(self.id));
        match &self.id {
            PageEntryId::Implicit(v) => {
                hasher.write_u8(1); // tag: implicit
                hasher.write_u32(*v);
            }
            PageEntryId::Explicit(slice) => {
                hasher.write_u8(0); // tag: explicit
                let s = unsafe { slice.slice(base) };
                hasher.write(s);
            }
        }
        let uri = unsafe { self.uri.slice(base) };
        hasher.write(uri);
        hasher.finish()
    }

    /// # Safety
    ///
    /// `self_base`/`other_base` must be the true bases of the backing memory
    /// holding the offsets referenced by `self`/`other`.
    pub unsafe fn eql<B1: size::IntFromBase + Copy, B2: size::IntFromBase + Copy>(
        &self,
        self_base: B1,
        other: &PageEntry,
        other_base: B2,
    ) -> bool {
        // if (std.meta.activeTag(self.id) != std.meta.activeTag(other.id)) return false;
        match (&self.id, &other.id) {
            (PageEntryId::Implicit(a), PageEntryId::Implicit(b)) => {
                if a != b {
                    return false;
                }
            }
            (PageEntryId::Explicit(a), PageEntryId::Explicit(b)) => {
                let self_ptr = unsafe { a.slice(self_base) };
                let other_ptr = unsafe { b.slice(other_base) };
                if self_ptr != other_ptr {
                    return false;
                }
            }
            _ => return false,
        }

        let self_uri = unsafe { self.uri.slice(self_base) };
        let other_uri = unsafe { other.uri.slice(other_base) };
        self_uri == other_uri
    }

    /// Free the memory for this entry from its page.
    ///
    /// # Safety
    ///
    /// `page` must be a valid page whose backing memory holds the offsets
    /// referenced by this entry.
    pub unsafe fn free(&self, page: *mut Page) {
        // PORT NOTE: field-projected access (see `Page::string_alloc_free`) — this
        // runs from the hyperlink set context's `deleted` while the page's
        // `hyperlink_set` is mutably borrowed.
        let base = unsafe { Page::memory_ptr_of(page) };
        match self.id {
            PageEntryId::Implicit(_) => {}
            PageEntryId::Explicit(v) => {
                let s = unsafe { v.slice(base) };
                unsafe { Page::string_alloc_free(page, s) };
            }
        }
        let uri = unsafe { self.uri.slice(base) };
        unsafe { Page::string_alloc_free(page, uri) };
    }
}

/// The context for the hyperlink set. See the module-level PORT NOTE: this
/// carries the per-call page pointers the reference passes as an anonymous
/// struct. Callers set `page`/`src_page` immediately before each set call.
// PORT NOTE: the reference is the anonymous struct passed as `ContextT`:
//   struct {
//       page: ?*Page = null,
//       src_page: ?*const Page = null,
//       pub fn hash(...) ...
//       pub fn eql(...) ...
//       pub fn deleted(...) ...
//   }
// PORT NOTE: the page pointers are stored as exposed addresses (`usize`) rather
// than raw `*mut Page`. The reference holds `*Page` directly, but in Rust storing
// a `*mut Page` derived from `&mut self` and then continuing to use `self` would
// invalidate that pointer's Stacked-Borrows tag (the `set.context.page = self`
// assignment is followed by `&mut self.hyperlink_set` accesses). Round-tripping
// through `usize` gives the stored pointer exposed provenance that survives those
// accesses — the same sound pattern `render-types`' `Offset` uses for its
// int-to-ptr base resolution. The address is reconstructed on each use.
#[derive(Clone, Copy, Default)]
pub struct SetContext {
    /// The page which holds the strings for items in this set (exposed address).
    pub page: Option<usize>,

    /// The page which holds the strings for items
    /// looked up with, e.g., `add` or `lookup`,
    /// if different from the destination page (exposed address).
    pub src_page: Option<usize>,
}

impl SetContext {
    /// Set the destination page from a raw pointer, exposing its provenance.
    pub fn set_page(&mut self, page: *mut Page) {
        self.page = Some((page as *const Page).expose_provenance());
    }

    /// Set the source page from a raw pointer, exposing its provenance.
    pub fn set_src_page(&mut self, page: Option<*const Page>) {
        self.src_page = page.map(|p| p.expose_provenance());
    }

    fn page_ptr(&self) -> *mut Page {
        core::ptr::with_exposed_provenance_mut(self.page.unwrap())
    }

    fn src_or_page(&self) -> *const Page {
        core::ptr::with_exposed_provenance(self.src_page.unwrap_or(self.page.unwrap()))
    }
}

impl RefCountedSetContext<PageEntry> for SetContext {
    fn hash(&self, link: PageEntry) -> u64 {
        // return link.hash((self.src_page orelse self.page.?).memory);
        // PORT NOTE: read `memory_ptr` via field projection (see
        // `Page::memory_ptr_of`) — this callback runs while the page's
        // `hyperlink_set` is mutably borrowed, so forming a `&Page` is UB.
        let base: *const Page = self.src_or_page();
        unsafe { link.hash(Page::memory_ptr_of(base)) }
    }

    fn eql(&self, a: PageEntry, b: PageEntry) -> bool {
        // return a.eql((self.src_page orelse self.page.?).memory, &b, self.page.?.memory);
        let a_base: *const Page = self.src_or_page();
        let b_base: *const Page = self.page_ptr() as *const Page;
        unsafe { a.eql(Page::memory_ptr_of(a_base), &b, Page::memory_ptr_of(b_base)) }
    }

    fn deleted(&self, link: PageEntry) {
        // link.free(self.page.?);
        unsafe { link.free(self.page_ptr()) }
    }
}

/// The set of hyperlinks. This is ref-counted so that a set of cells
/// can share the same hyperlink without duplicating the data.
pub type Set = RefCountedSet<PageEntry, Id, size::CellCountInt, SetContext>;
