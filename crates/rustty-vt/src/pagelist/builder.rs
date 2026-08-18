//! PageList builder / detached page allocation (snapshot-enabling APIs).
//!
//! PORTS: oracle@main PageList.zig `allocatePage` / `PageAllocation` /
//! `Builder` — the end state of fc1bd06a1 ("PageList builder to build from
//! raw pages") + 35db32078 ("PageList allocatePage") + 457c5a0a6 ("PageList
//! align Builder/PageAllocation APIs better": `Builder.addPage` →
//! `allocatePage`, `PageAllocation.cancel` → `deinit`, nullable `node` so
//! `deinit` is safe after `finalize` and both APIs can use unconditional
//! deferred cleanup).
//!
//! These exist to serve snapshot decoding (which builds a PageList from raw
//! pages) but are useful and testable standalone; the snapshot subsystem
//! lands as its own area.

use core::mem::ManuallyDrop;

use super::{
    CreatePage, Key, Limits, List, MemoryPool, Node, Options, OutOfMemory, PageList, Pin, PinSet,
    Viewport, PAGE_PREHEAT,
};
use crate::page::{Capacity, Page};
use rustty_render_types::term_size as size;

// Failure injection for `Builder::finish` / `PageList::allocate_page`.
//
// PORT NOTE (port glue, no oracle tripwire): the reference exercises these
// failure paths with `std.testing.FailingAllocator` (`fail_index =
// alloc_index` fails the allocator's next request). The Rust pools are backed
// by the infallible global allocator, so the same injection seam is provided
// as `rustty_util::tripwire` fail points placed exactly where the reference
// allocation would have failed — the same convention as the `init_tw` /
// `initPages_tw` modules in `mod.rs` (which DO exist upstream) and
// `compress.rs`'s `COMPRESS_PAGE_TW`.

/// The `finish` fail point: the tracked viewport pin map insertion (the
/// reference's "next general allocation" after the preheated pin pool).
#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum FinishFp {
    ViewportPinTrack,
}

/// The `allocate_page` fail point: node/page allocation reaching pool growth
/// (the reference's `createPageExt` → `pool.nodes.create()` growth path).
#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum AllocatePageFp {
    PageNode,
}

#[cfg(test)]
thread_local! {
    pub(crate) static FINISH_TW: core::cell::RefCell<
        rustty_util::tripwire::Module<FinishFp, OutOfMemory>,
    > = core::cell::RefCell::new(rustty_util::tripwire::module());
    pub(crate) static ALLOCATE_PAGE_TW: core::cell::RefCell<
        rustty_util::tripwire::Module<AllocatePageFp, OutOfMemory>,
    > = core::cell::RefCell::new(rustty_util::tripwire::module());
}

#[cfg(test)]
#[inline]
fn finish_tw_check(point: FinishFp) -> Result<(), OutOfMemory> {
    FINISH_TW.with(|tw| tw.borrow_mut().check(point))
}

#[cfg(test)]
#[inline]
fn allocate_page_tw_check(point: AllocatePageFp) -> Result<(), OutOfMemory> {
    ALLOCATE_PAGE_TW.with(|tw| tw.borrow_mut().check(point))
}

impl PageList {
    /// Allocate a new page using the PageList's memory pools.
    ///
    /// The page is detached: it doesn't contribute to the memory limits or
    /// row counts or anything in the PageList. The caller must call `finalize`
    /// to add it to the PageList at the appropriate place, or `deinit` to
    /// throw it away.
    //
    // PORT NOTE (raw-self seam): the reference takes `self: *PageList` and the
    // returned `PageAllocation` stores that pointer while the caller keeps
    // using the PageList (allocating further detached pages, reading state)
    // before `finalize`/`deinit`. A `&mut self` receiver would make the stored
    // `destination` a child of a dead unique borrow, invalidated by the next
    // `&mut` use of the list (Stacked Borrows) — so this is an associated fn
    // over a raw `*mut PageList`, per the port's raw-pointer-args discipline
    // for re-read seams.
    ///
    /// # Safety
    /// `this` must point at a live `PageList` that stays at this address (and
    /// is only accessed via `this`-derived pointers or non-invalidating reads)
    /// until the returned allocation is finalized/deinited.
    pub unsafe fn allocate_page(
        this: *mut PageList,
        capacity: Capacity,
    ) -> Result<PageAllocation, OutOfMemory> {
        // Port-glue fail point standing in for the reference FailingAllocator
        // failing `createPageExt`'s pool growth; see the module PORT NOTE.
        #[cfg(test)]
        allocate_page_tw_check(AllocatePageFp::PageNode)?;

        Ok(PageAllocation {
            destination: this,
            // SAFETY: `this` is live per the contract; the pool/serial field
            // projections are disjoint raw-derived borrows for this call only.
            node: PageList::create_page_ext(
                unsafe { &mut (*this).pool },
                CreatePage::new(capacity),
                unsafe { &mut (*this).page_serial },
                None,
            )?,
        })
    }
}

/// One PageList-pooled page which has not yet joined the live page sequence.
//
// LIFETIMES:
//   destination: *mut PageList — BORROW_PARAM, raw non-owning back-reference
//     to the list that allocated the page (owner: the caller of
//     `allocate_page`). Raw, not `&mut`: the caller also touches the list
//     while the allocation is outstanding. Never freed here.
//   node: *mut Node — OWNED (pool storage) until `finalize` transfers it to
//     `destination.pages` or `deinit` returns it to `destination.pool`; null
//     afterwards (the reference `?*List.Node`), which is what makes `deinit`
//     safe to defer unconditionally.
pub struct PageAllocation {
    pub destination: *mut PageList,
    pub node: *mut Node,
}

impl PageAllocation {
    /// Return the fresh page storage for the caller to populate.
    pub fn page(&mut self) -> *mut Page {
        // return self.node.?.pageAssumeResident();
        assert!(!self.node.is_null());
        // SAFETY: `node` is the live, resident node created by
        // `allocate_page`; non-null asserted above (the reference `.?`).
        unsafe { Node::page_assume_resident(self.node) }
    }

    /// Release an uncommitted page back to its PageList's pools.
    ///
    /// This is safe to call after `finalize` succeeds, so callers can defer
    /// it unconditionally.
    pub fn deinit(&mut self) {
        // const node = self.node orelse return;
        let node = self.node;
        if node.is_null() {
            return;
        }
        // SAFETY: `destination` points at the live PageList that allocated
        // `node` (see the struct LIFETIMES note); the pool field projection is
        // a raw-derived borrow for this call only.
        PageList::destroy_node_ext(unsafe { &mut (*self.destination).pool }, node, None);
        self.node = core::ptr::null_mut();
    }

    /// Finalize this complete page and transfer its ownership to the PageList.
    /// The parameter determines where it goes into the PageList.
    ///
    /// Existing pages and tracked pins keep their identity. A pinned viewport
    /// keeps showing the same content while its cached absolute row offset
    /// moves down by the number of newly inserted rows.
    pub fn finalize(&mut self, location: Location) -> Result<(), FinalizeError> {
        match location {
            Location::Prepend => self.prepend(),
        }
    }

    fn prepend(&mut self) -> Result<(), FinalizeError> {
        let destination = self.destination;
        // const node = self.node.?;
        let node = self.node;
        assert!(!node.is_null());

        // SAFETY: `destination` points at the live PageList that allocated
        // `node` (struct LIFETIMES note); all accesses below are raw-derived
        // field projections/reads through it, and `node` is the live detached
        // node, so the metadata reads (`cols`/`rows`/`owned`) are in-bounds.
        unsafe {
            // Validate the populated page and all resulting accounting before
            // publishing the detached node into the live list.
            if Node::cols(node) == 0 || Node::rows(node) == 0 {
                return Err(FinalizeError::InvalidPageDimensions);
            }
            let total_rows = (*destination)
                .total_rows
                .checked_add(Node::rows(node) as usize)
                .ok_or(FinalizeError::RowCountOverflow)?;
            let node_size: usize = match (*node).owned {
                // PagePool.item_size (a full standard pool item).
                super::Owned::Pool => super::std_size(),
                // node.pageAssumeResident().memory.len
                super::Owned::Heap => {
                    PageList::page_memory(Node::page_assume_resident(node)).1
                }
            };
            let page_size = (*destination)
                .page_size
                .checked_add(node_size)
                .ok_or(FinalizeError::PageSizeOverflow)?;

            // Restored history is exact data, so reject a page which cannot
            // coexist with the receiving PageList's configured limits.
            if page_size > (*destination).limits.max(Key::Bytes) {
                return Err(FinalizeError::MaxSizeExceeded);
            }
            // total_rows >= destination.rows: the destination invariantly
            // holds at least its active area, and total_rows only grew.
            debug_assert!(total_rows >= (*destination).rows as usize);
            if total_rows - (*destination).rows as usize > (*destination).limits.max(Key::Lines) {
                return Err(FinalizeError::MaxLinesExceeded);
            }

            // No fallible work remains. Publish the page and update every cached
            // quantity affected by inserting rows above the existing first page.
            // errdefer comptime unreachable;
            (*destination).pages.prepend(node);
            (*destination).page_size = page_size;
            (*destination).total_rows = total_rows;
            if (*destination).viewport == Viewport::Pin {
                if let Some(offset) = (*destination).viewport_pin_row_offset.as_mut() {
                    *offset += Node::rows(node) as usize;
                }
            }
            (*destination).page_compression.mark_activity();

            (*destination).assert_integrity();
        }
        self.node = core::ptr::null_mut();
        Ok(())
    }
}

/// `PageAllocation.Location`
pub enum Location {
    /// Prepend the page to the start of the list (oldest history).
    Prepend,
}

/// `PageAllocation.FinalizeError`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FinalizeError {
    InvalidPageDimensions,
    RowCountOverflow,
    PageSizeOverflow,
    MaxSizeExceeded,
    MaxLinesExceeded,
}

/// Build up a PageList manually from a set of Pages.
///
/// This data structure is transactional: `deinit` releases every page
/// until `finish` is called. This keeps the ownership clear: a complete
/// PageList either owns all its pages or doesn't.
///
/// This was specifically built to help facilitate snapshot decoding
/// which transfers pages directly, but could be generally useful
/// for other purposes as well.
pub struct Builder {
    // PORT NOTE: `ManuallyDrop` because `finish` transfers the pool by value
    // into the produced PageList (the reference `.pool = self.pool` struct
    // copy) while `self` stays borrowed; `deinit` drops it explicitly when the
    // builder never finished. A plain field would double-free on Builder drop.
    // As in Zig, a Builder that is neither finished nor deinit'ed leaks.
    pub pool: ManuallyDrop<MemoryPool>,
    pub pages: List,
    pub page_serial: u64,
    pub page_size: usize,
    pub options: Options,
    pub finished: bool,
}

impl Builder {
    /// Initialize an empty builder. The options are the final state
    /// of the PageList and some validation is done on the finish call
    /// to ensure you built up a proper PageList according to those options.
    //
    // PORT NOTE: the reference takes `alloc: Allocator`; as everywhere in this
    // module, the Rust pools are backed by the global/page allocators selected
    // inside `MemoryPool::init` (R1 additive glue boundary), so no allocator
    // parameter is threaded through.
    pub fn init(options: Options) -> Result<Builder, OutOfMemory> {
        Ok(Builder {
            pool: ManuallyDrop::new(MemoryPool::init(PAGE_PREHEAT)?),
            pages: List::default(),
            page_serial: 0,
            page_size: 0,
            options,
            finished: false,
        })
    }

    /// Release all pages when restoration does not finish.
    ///
    /// This is safe to call after `finish` succeeds, so callers can defer it
    /// unconditionally.
    pub fn deinit(&mut self) {
        if self.finished {
            return;
        }

        // Free all our in-progress pages
        loop {
            // SAFETY: the list and its nodes are live (owned by this builder).
            let node = unsafe { self.pages.pop_first() };
            if node.is_null() {
                break;
            }
            PageList::destroy_node_ext(&mut self.pool, node, Some(&mut self.page_size));
        }
        // Free memory pool
        // SAFETY: `finished` is false, so the pool was never transferred by
        // `finish`; it is dropped exactly once here.
        unsafe { ManuallyDrop::drop(&mut self.pool) };
        // self.* = undefined; — the closest safe equivalent: mark the builder
        // finished so a (contract-violating) reuse cannot double-free.
        self.finished = true;
    }

    /// Allocate a new page into the PageList with the given capacity.
    ///
    /// The caller can then take this page and populate it. When `finish`
    /// is called, ownership is transferred to the resulting PageList.
    /// Until then, this Builder owns the page.
    pub fn allocate_page(&mut self, capacity: Capacity) -> Result<*mut Page, OutOfMemory> {
        let node = PageList::create_page_ext(
            &mut self.pool,
            CreatePage::new(capacity),
            &mut self.page_serial,
            Some(&mut self.page_size),
        )?;
        // SAFETY: `node` is a fresh live node owned by this builder's pools.
        unsafe {
            self.pages.append(node);
        }
        // SAFETY: `create_page_ext` returns a resident node.
        Ok(unsafe { Node::page_assume_resident(node) })
    }

    /// Validate the decoded pages and transfer them into a live PageList.
    /// After this succeeds, `deinit` is a no-op because all resources have
    /// transferred to the PageList.
    pub fn finish(&mut self) -> Result<PageList, FinishError> {
        // These are basic validations but they're cheap to do and
        // we want to be careful we don't let corruption from untrusted
        // sources into our PageList which asserts this.
        if self.options.cols == 0 || self.options.rows == 0 {
            return Err(FinishError::InvalidDimensions);
        }
        if self.pages.first.is_null() {
            return Err(FinishError::NoPages);
        }

        // Manually count our total rows at this point which we'll
        // need for our PageList cache as well as a safety check.
        let total_rows: usize = {
            let mut total_rows: usize = 0;
            let mut node = self.pages.first;
            while !node.is_null() {
                let current = node;
                // SAFETY: `current` is a live node of this builder's list.
                unsafe {
                    if Node::cols(current) == 0 || Node::rows(current) == 0 {
                        return Err(FinishError::InvalidPageDimensions);
                    }
                    total_rows += Node::rows(current) as usize;
                    node = (*current).next;
                }
            }
            if total_rows < self.options.rows as usize {
                return Err(FinishError::InsufficientRows);
            }
            total_rows
        };

        // Get our active pin
        let active_top: Pin = {
            let mut rem: size::CellCountInt = self.options.rows;
            let mut node = self.pages.last;
            loop {
                // else => unreachable — the row count was validated above.
                assert!(!node.is_null());
                let current = node;
                // SAFETY: `current` is a live node of this builder's list.
                let current_rows = unsafe { Node::rows(current) };
                if rem <= current_rows {
                    break Pin {
                        node: current,
                        // In-bounds: rem <= current_rows just checked.
                        y: current_rows - rem,
                        x: 0,
                        garbage: false,
                    };
                }
                rem -= current_rows;
                node = unsafe { (*current).prev };
            }
        };

        // Set our viewport up to the active
        let viewport_pin = self.pool.pins.create().map_err(|_| FinishError::OutOfMemory)?;
        // errdefer self.pool.pins.destroy(viewport_pin);
        // SAFETY: fresh pin storage from the pin pool.
        unsafe {
            viewport_pin.as_ptr().write(active_top);
        }

        // Setup our one viewport tracked pin
        let mut tracked_pins: PinSet = PinSet::new();
        // errdefer tracked_pins.deinit(self.pool.alloc); — the Rust `PinSet`
        // owns its storage; dropping it on an early return frees it.
        //
        // Port-glue fail point standing in for the reference FailingAllocator
        // failing `tracked_pins.putNoClobber`'s map allocation (the Rust
        // `put_no_clobber` is infallible); see the module PORT NOTE.
        #[cfg(test)]
        if let Err(e) = finish_tw_check(FinishFp::ViewportPinTrack) {
            // errdefer self.pool.pins.destroy(viewport_pin);
            // SAFETY: created above, not yet published anywhere.
            unsafe { self.pool.pins.destroy(viewport_pin) };
            return Err(e.into());
        }
        tracked_pins.put_no_clobber(viewport_pin.as_ptr(), ());

        // Initialize limits
        let mut limits: Limits = Limits::init(self.options.cols, self.options.rows);
        limits.set(Key::Bytes, self.options.max_size);
        limits.set(Key::Lines, self.options.max_lines);

        // errdefer comptime unreachable — no fallible work remains.
        let result: PageList = PageList {
            cols: self.options.cols,
            rows: self.options.rows,
            // SAFETY: `finished` is set immediately below, so `deinit` will
            // never touch (and Builder never drops) the moved-out pool.
            pool: unsafe { ManuallyDrop::take(&mut self.pool) },
            pages: List {
                first: self.pages.first,
                last: self.pages.last,
            },
            page_serial: self.page_serial,
            page_serial_epoch: 0,
            page_compression: super::compress::IncrementalCompressionState::default(),
            page_size: self.page_size,
            limits,
            total_rows,
            tracked_pins,
            viewport: Viewport::Active,
            viewport_pin: viewport_pin.as_ptr(),
            viewport_pin_row_offset: None,
            pause_integrity_checks: 0,
        };
        result.assert_integrity();
        self.finished = true;
        Ok(result)
    }
}

/// `Builder.FinishError = Allocator.Error || error{...}`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FinishError {
    OutOfMemory,
    InvalidDimensions,
    InvalidPageDimensions,
    NoPages,
    InsufficientRows,
}

impl From<OutOfMemory> for FinishError {
    fn from(_: OutOfMemory) -> FinishError {
        FinishError::OutOfMemory
    }
}
