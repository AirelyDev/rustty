//! PageList page-compression policy.
//!
//! Which pages to compress, when to discard their resident memory, and when to
//! restore them is decided here; the representation and codec live in
//! `crate::compress`.
//!
//! PORTS: the compression sections of ghostty src/terminal/PageList.zig
//! (oracle origin/main; commits 9156ada16, 70e788f06, e7969ed43, f8c217e55,
//! 181254d36, 95685afd2).

use super::{std_size, MemoryPool, Node, NodeData, OutOfMemory, PageList};
use crate::compress;
use crate::mem;
use crate::point;

use allocator_api2::alloc::Allocator;
use core::ptr::NonNull;

/// Temporary output memory used while creating a compressed page.
///
/// Standard-sized output borrows a page-pool item so repeated compression can
/// reuse the same virtual mapping. Oversized pages use a temporary allocation
/// from the page allocator and release it immediately after compression.
///
/// PORTS: oracle@main PageList.zig `CompressionScratch`.
pub(crate) enum CompressionScratch {
    /// A borrowed pool item. Always exactly `std_size()` bytes.
    Pooled(*mut u8),
    /// A temporary page-allocator allocation of `len` bytes.
    Allocated { ptr: *mut u8, len: usize },
}

impl CompressionScratch {
    pub(crate) fn init(
        pool: &mut MemoryPool,
        required: usize,
        raw_len: usize,
    ) -> Result<CompressionScratch, OutOfMemory> {
        debug_assert!(required <= raw_len);

        if required <= std_size() {
            let memory = pool.pages.create().map_err(|_| OutOfMemory)?;
            // SAFETY: a pool item is a live, page-aligned `std_size()` mapping.
            unsafe { mem::recommit(memory.as_ptr(), std_size()) };
            return Ok(CompressionScratch::Pooled(memory.as_ptr()));
        }

        let layout = PageList::non_standard_layout(raw_len);
        let alloc = super::page_backing_alloc();
        let slice = alloc.allocate(layout).map_err(|_| OutOfMemory)?;
        Ok(CompressionScratch::Allocated {
            ptr: slice.cast::<u8>().as_ptr(),
            len: raw_len,
        })
    }

    /// The writable scratch bytes.
    ///
    /// PORT NOTE (WS7 B7): this is the one place a `&mut [u8]` is legitimate —
    /// the scratch buffer is exclusively owned by this value and never aliased
    /// by a `Page`.
    pub(crate) fn bytes(&mut self) -> &mut [u8] {
        match *self {
            // SAFETY: a pool item is live for `std_size()` bytes and this value
            // holds it exclusively until `deinit`.
            CompressionScratch::Pooled(ptr) => unsafe {
                core::slice::from_raw_parts_mut(ptr, std_size())
            },
            // SAFETY: as above, for the temporary allocation.
            CompressionScratch::Allocated { ptr, len } => unsafe {
                core::slice::from_raw_parts_mut(ptr, len)
            },
        }
    }

    pub(crate) fn deinit(self, pool: &mut MemoryPool) {
        match self {
            CompressionScratch::Pooled(ptr) => {
                // Zero (and, where supported, reclaim) the item before it
                // re-enters the free list — `create_page_ext` relies on pooled
                // buffers reading back as zero.
                //
                // SAFETY: `ptr` is the live pool item this value borrowed.
                let _ =
                    unsafe { mem::decommit(mem::DecommitMode::Zero, ptr, std_size(), std_size()) };
                // SAFETY: `ptr` came from `pool.pages.create()`.
                unsafe { pool.pages.destroy(NonNull::new(ptr).unwrap()) };
            }

            CompressionScratch::Allocated { ptr, len } => {
                let layout = PageList::non_standard_layout(len);
                let alloc = super::page_backing_alloc();
                // SAFETY: `ptr`/`layout` are the pair allocated in `init`.
                unsafe { alloc.deallocate(NonNull::new(ptr).unwrap(), layout) };
            }
        }
    }
}

/// PageList-owned state for incremental compression.
///
/// The position is stored as a page serial rather than a node pointer so it
/// remains safe when PageList operations destroy, replace, or reuse nodes
/// between steps. It also records the first serial which was unallocated at the
/// prior step so new nodes before a valid marker restart safely. If the exact
/// serial no longer exists in the cold prefix, the next step also restarts at
/// the first page.
///
/// A completed pass is followed by another pass from the oldest cold page.
/// Compression becomes idle only when that verification pass compresses no
/// pages. This converges after pages are restored between incremental steps
/// without requiring consumers to maintain a separate attempt cursor.
///
/// PORTS: oracle@main PageList.zig `IncrementalCompressionState`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IncrementalCompressionState {
    pub flags: IncrementalCompressionFlags,

    /// Changes whenever PageList activity may affect compression work. Callers
    /// use this value (via `Terminal::compression_activity`) to determine
    /// whether to recompress.
    ///
    /// The directionality of this doesn't matter. We overflow and wrap when
    /// maxxed. The comparison of this value to your saved value is all that
    /// matters. There is a possible edge case where you don't compress, a full
    /// wraparound happens, and you get the same value, but it's so unlikely.
    ///
    /// PORT NOTE (WS7 B8): the oracle's field is a `u48` so that 16 bits are
    /// reserved for `Terminal` to add extra state. Rust has no `u48`, so this
    /// is a `u64` masked to 48 bits by `mark_activity`; `Terminal` composes its
    /// extra 16 bits in the high bits exactly as the oracle does.
    pub activity_serial: u64,

    /// Serial of the last page inspected by the traversal. This is
    /// intentionally an implementation detail and is reset by PageList
    /// operations which restart traversal progress.
    pub last_serial: Option<u64>,

    /// First node serial which had not been allocated at the prior step. A node
    /// at or above this value before the saved marker requires a restart.
    pub next_serial: u64,
}

/// `IncrementalCompressionState.flags`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IncrementalCompressionFlags {
    /// Set after any page is compressed during the current traversal. We always
    /// run incremental compression until we get a fully no-compression pass
    /// (the verification pass). This lets us recompress decompressed pages due
    /// to search, scrolling, etc.
    pub did_compress: bool,

    /// Set after a traversal first reaches the active boundary. When this
    /// verification traversal reaches the boundary, `did_compress` determines
    /// whether to restart once more or finish the pass.
    pub verifying: bool,
}

/// The mask applied to `activity_serial` to model the oracle's `u48`.
const ACTIVITY_SERIAL_MASK: u64 = 0xFFFF_FFFF_FFFF;

impl IncrementalCompressionState {
    /// Record activity without disturbing valid traversal progress.
    ///
    /// PORT NOTE (WS7 I4 — a KNOWN gap, tracked in
    /// `.port/phase-2/DEFERRALS.md`): the oracle has ELEVEN `markActivity` call
    /// sites; rustty has ten. The missing one is
    /// `oracle@main:PageList.zig:6312`, inside `Limits.enforce` — and rustty has
    /// no `PageList::Limits` struct at all (the byte limit is a plain field), so
    /// there is no counterpart function to port it onto. If a `Limits` struct is
    /// ever introduced, `enforce` must `mark_activity` on entry: trimming
    /// scrollback to a limit changes the node set under an in-flight incremental
    /// traversal, which is exactly what the activity token exists to detect.
    #[inline]
    pub fn mark_activity(&mut self) {
        // Zig: `self.activity_serial +%= 1;` on a u48.
        self.activity_serial = self.activity_serial.wrapping_add(1) & ACTIVITY_SERIAL_MASK;
    }

    /// Discard traversal progress without changing the activity token.
    #[inline]
    pub fn reset(&mut self) {
        let activity_serial = self.activity_serial;
        *self = IncrementalCompressionState {
            activity_serial,
            ..Default::default()
        };
    }
}

/// Result of one incremental compression step.
///
/// PORTS: oracle@main PageList.zig `IncrementalCompressionResult`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncrementalCompressionResult {
    /// Strict retained-mapping reclamation is unavailable on this target.
    Unsupported,

    /// More cold pages or a verification pass remain after this invocation's
    /// candidate-bounded work.
    Pending,

    /// A complete verification pass compressed zero pages.
    Complete,
}

/// The mode argument of [`PageList::compress`].
///
/// PORTS: the oracle's inline `enum { incremental, drain, full }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionMode {
    Incremental,
    Drain,
    Full,
}

/// Bound candidate inspection independently from compression work. Skipping an
/// already-compressed page is cheap, but still counts toward this limit.
pub(super) const INCREMENTAL_COMPRESSION_MAX_INSPECTED: usize = 8;

/// Iterate complete historical pages which do not intersect the viewport.
///
/// All boundaries come from PageList pins and Page metadata. Advancing this
/// iterator never restores a compressed page or reads its backing memory.
///
/// PORTS: oracle@main PageList.zig `CompressionIterator`.
pub(crate) struct CompressionIterator {
    current: *mut Node,
    active: *mut Node,
    viewport_first: *mut Node,
    viewport_last: *mut Node,
}

impl CompressionIterator {
    pub(crate) fn init(list: &PageList) -> CompressionIterator {
        CompressionIterator {
            current: list.pages.first,
            active: list.get_top_left(point::Tag::Active).node,
            viewport_first: list.get_top_left(point::Tag::Viewport).node,
            viewport_last: list.get_bottom_right(point::Tag::Viewport).unwrap().node,
        }
    }

    pub(crate) fn next(&mut self) -> Option<*mut Node> {
        while !core::ptr::eq(self.current, self.active) {
            // The viewport is a contiguous node range. Once traversal reaches
            // its first node, advance through the complete visible range and
            // resume at the next offscreen page.
            if core::ptr::eq(self.current, self.viewport_first) {
                while !core::ptr::eq(self.current, self.active)
                    && !core::ptr::eq(self.current, self.viewport_last)
                {
                    self.current = unsafe { (*self.current).next };
                }

                if core::ptr::eq(self.current, self.active) {
                    return None;
                }
                self.current = unsafe { (*self.current).next };
                continue;
            }

            let node = self.current;
            self.current = unsafe { (*node).next };
            return Some(node);
        }

        None
    }

    pub(crate) fn done(&self) -> bool {
        core::ptr::eq(self.current, self.active)
    }
}

/// Failure injection for the final reclamation step. The operating-system
/// failure path cannot otherwise be exercised by tests because `crate::mem`
/// deliberately simulates successful decommit in test builds.
///
/// PORTS: oracle@main PageList.zig `compressPage_tw`.
#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum CompressPageFp {
    Decommit,
}

/// `error{DecommitFailed}`.
#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct DecommitFailed;

#[cfg(test)]
thread_local! {
    pub(crate) static COMPRESS_PAGE_TW: core::cell::RefCell<
        rustty_util::tripwire::Module<CompressPageFp, DecommitFailed>,
    > = core::cell::RefCell::new(rustty_util::tripwire::module());
}

#[cfg(test)]
#[inline]
fn compress_page_tw_check(point: CompressPageFp) -> Result<(), DecommitFailed> {
    COMPRESS_PAGE_TW.with(|tw| tw.borrow_mut().check(point))
}

/// Snapshot of the storage used by page nodes in this list.
///
/// The raw byte counts describe page backing mappings only. They exclude nodes,
/// allocator metadata, unused preheated pool items, and the small
/// representation values stored in each node. A compressed page retains its raw
/// mapping as virtual address space, but its bytes are counted as decommitted
/// because strict reclamation succeeded before the state was published.
///
/// PORTS: oracle@main PageList.zig `MemoryStats`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct MemoryStats {
    /// Pages whose raw backing mappings are resident.
    pub resident_pages: usize,

    /// Pages represented by encoded storage and a decommitted raw mapping.
    pub compressed_pages: usize,

    /// Logical bytes in every raw page mapping.
    pub raw_bytes: usize,

    /// Raw mapping bytes which remain resident.
    pub resident_raw_bytes: usize,

    /// Raw mapping bytes discarded for compressed pages.
    pub decommitted_raw_bytes: usize,

    /// Raw allocation bytes which remain physically resident.
    ///
    /// This can exceed `resident_raw_bytes` because a pool-owned page uses only
    /// part of a standard pool item. Compressing such a page decommits its
    /// initialized range, but the unused tail of the item stays resident.
    pub resident_backing_bytes: usize,

    /// Exact encoded allocations retained for compressed pages.
    pub encoded_bytes: usize,
}

impl MemoryStats {
    /// Estimate resident page backing storage after compression.
    pub fn estimated_resident_bytes(&self) -> usize {
        self.resident_backing_bytes + self.encoded_bytes
    }

    /// Estimate physical bytes avoided by compressed page backing storage.
    ///
    /// PORT NOTE: the oracle's `-|` is a saturating subtraction.
    pub fn estimated_savings(&self) -> usize {
        self.decommitted_raw_bytes
            .saturating_sub(self.encoded_bytes)
    }
}

impl PageList {
    /// Attempt to compress one resident page while retaining its raw mapping.
    ///
    /// Compression is opportunistic: every failure leaves the page resident and
    /// usable. Candidate selection and retry policy belong to `compress`; this
    /// primitive only performs one state transition.
    ///
    /// PORTS: oracle@main PageList.zig `compressPage`.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub(crate) fn compress_page(&mut self, node: *mut Node) -> bool {
        // Recompression requires first restoring the raw page and is a policy
        // decision, so this primitive only accepts resident nodes.
        if unsafe { Node::is_compressed(node) } {
            return false;
        }

        // PORT NOTE (WS7, binding): the page is bound as a RAW pointer. Binding
        // `&*Node::page(node)` would keep a shared reference into the node's
        // storage live across the `(*node).data = ...` write at the end of this
        // function, which Stacked Borrows rejects.
        let page = unsafe { Node::page(node) };
        let raw_len = unsafe { (*page).memory_len() };

        // The scratch size is capped just below the representation's break-even
        // point. Codec limits and pages too small to cover the compressed-state
        // overhead simply make this page ineligible for compression.
        let required = match compress::required_scratch(raw_len) {
            Ok(n) => n,
            // error.InputTooLarge, error.OutputTooSmall => return false
            Err(_) => return false,
        };
        if required == 0 {
            return false;
        }

        // Build the compressed candidate without changing the node. This scope
        // is intentional: it releases the borrowed or temporary scratch before
        // we attempt to discard the source mapping below. Only the candidate's
        // exact-sized encoded allocation survives the scope.
        let candidate = {
            let mut scratch = match CompressionScratch::init(&mut self.pool, required, raw_len) {
                Ok(s) => s,
                Err(OutOfMemory) => return false,
            };

            let mut table: compress::lz4::HashTable = [0; compress::lz4::HASH_TABLE_LEN];
            // SAFETY: `page` is the live resident page of `node`; the scratch
            // buffer is a distinct allocation.
            let result = unsafe {
                compress::CompressedPage::init(
                    page as *const _,
                    &mut scratch.bytes()[..required],
                    &mut table,
                )
            };

            // defer scratch.deinit(&self.pool);
            scratch.deinit(&mut self.pool);

            match result {
                Ok(c) => c,
                // error.OutOfMemory, error.InputTooLarge, error.OutputTooSmall
                Err(_) => return false,
            }
        };

        // None means compression crossed the break-even point. The node and its
        // resident mapping are still untouched in this case.
        let compressed = match candidate {
            Some(c) => c,
            None => return false,
        };

        // Strict decommit is the final fallible step. It either discards the
        // whole raw mapping or leaves it untouched, so failure can safely drop
        // the candidate and preserve the resident node exactly as it was.
        let decommit_allowed: bool = {
            #[cfg(test)]
            {
                compress_page_tw_check(CompressPageFp::Decommit).is_ok()
            }
            #[cfg(not(test))]
            {
                true
            }
        };
        let mem_ptr = compressed.page.memory_ptr() as *mut u8;
        let mem_len = compressed.page.memory_len();
        // SAFETY: the retained mapping is live, page-aligned and unaliased —
        // `page` is not dereferenced past this point.
        if !decommit_allowed
            || !unsafe { mem::decommit(mem::DecommitMode::Strict, mem_ptr, mem_len, mem_len) }
        {
            drop(compressed);
            return false;
        }

        // Publish the new state only after both the encoded allocation and the
        // retained-mapping decommit have succeeded.
        //
        // SAFETY: `node` is live and nothing borrows its storage here.
        unsafe { *core::ptr::addr_of_mut!((*node).data) = NodeData::Compressed(compressed) };
        true
    }

    /// Compress eligible nodes, saving a significant amount of memory.
    ///
    /// Eligible nodes are complete pages before the active boundary which do
    /// not intersect the viewport. The boundary page is excluded because it may
    /// contain both scrollback and active rows; visible pages remain resident
    /// for immediate redraw and scrolling.
    ///
    /// Compression requires a system that supports reclaiming physical memory
    /// for virtual allocations while retaining their address ranges.
    ///
    /// Compression is SLOW (relatively), so incremental compression during idle
    /// periods is recommended. Incremental mode performs one bounded step and
    /// its result specifies whether to continue immediately. Drain mode
    /// performs incremental steps until the pass and its verification pass
    /// finish. Full mode visits every currently eligible node once without
    /// using incremental state.
    ///
    /// PageList tracks mutations which require a later incremental pass in its
    /// compression state. On supported targets, full compression returns
    /// `Complete`, indicating that it has no continuation to schedule rather
    /// than that every page was compressed.
    ///
    /// PORTS: oracle@main PageList.zig `compress`.
    pub fn compress(&mut self, mode: CompressionMode) -> IncrementalCompressionResult {
        match mode {
            CompressionMode::Incremental => self.compress_incremental(),

            CompressionMode::Drain => loop {
                match self.compress_incremental() {
                    IncrementalCompressionResult::Pending => continue,
                    IncrementalCompressionResult::Complete => {
                        break IncrementalCompressionResult::Complete
                    }
                    IncrementalCompressionResult::Unsupported => {
                        break IncrementalCompressionResult::Unsupported
                    }
                }
            },

            CompressionMode::Full => {
                // Match incremental mode's unsupported result. Full compression
                // has no useful work to perform without strict reclamation.
                if !mem::can_reclaim(mem::DecommitMode::Strict) {
                    self.page_compression.reset();
                    return IncrementalCompressionResult::Unsupported;
                }

                self.compress_full();

                // Full compression has no continuation. Discard any partial
                // incremental cursor so later activity starts at the oldest
                // page.
                self.page_compression.reset();

                IncrementalCompressionResult::Complete
            }
        }
    }

    /// Perform one candidate-bounded incremental cold-history compression step.
    ///
    /// PORTS: oracle@main PageList.zig `compressIncremental`.
    fn compress_incremental(&mut self) -> IncrementalCompressionResult {
        // If we can't reclaim virtual memory, compression is unsupported.
        if !mem::can_reclaim(mem::DecommitMode::Strict) {
            self.page_compression.reset();
            return IncrementalCompressionResult::Unsupported;
        }

        // Find the node following the exact continuation marker within the cold
        // prefix. A missing marker means the list changed between steps, so
        // begin again at the current first page. This lookup does not touch
        // page memory.
        let mut it = CompressionIterator::init(self);
        if let Some(last_serial) = self.page_compression.last_serial {
            let mut found = false;
            while let Some(node) = it.next() {
                let serial = unsafe { (*node).serial };

                // A newly allocated or replacement node appeared before the
                // marker. Restart immediately so that node cannot be skipped.
                if serial >= self.page_compression.next_serial {
                    break;
                }

                // Not a match? Keep looking
                if serial != last_serial {
                    continue;
                }

                // Match! The iterator already points to the next offscreen
                // node.
                found = true;
                break;
            }

            if !found {
                // Not found or otherwise invalid. Reset
                self.page_compression.last_serial = None;
                it = CompressionIterator::init(self);
            }
        }

        // Keep track of our next_serial
        self.page_compression.next_serial = self.page_serial;

        // We cap the number of pages we look at to do our best to time-bound
        // the incremental compression.
        let mut inspected_pages: usize = 0;
        while inspected_pages < INCREMENTAL_COMPRESSION_MAX_INSPECTED {
            let Some(node) = it.next() else { break };

            self.page_compression.last_serial = Some(unsafe { (*node).serial });
            inspected_pages += 1;

            // If this page is already compressed, ignore it.
            if unsafe { Node::is_compressed(node) } {
                continue;
            }

            // Compression is substantially more expensive even if it fails. So
            // we just try it.
            if self.compress_page(node) {
                self.page_compression.flags.did_compress = true;
            }
            break;
        }

        // If we didn't reach our active node, then we still have work to do.
        if !it.done() {
            return IncrementalCompressionResult::Pending;
        }

        // We reached our active node. So we're done, except that we always do
        // one pass after the first success so we can recompress nodes that were
        // possibly decompressed (e.g. by search, inspector, whatever).
        if !self.page_compression.flags.verifying || self.page_compression.flags.did_compress {
            let activity_serial = self.page_compression.activity_serial;
            self.page_compression = IncrementalCompressionState {
                flags: IncrementalCompressionFlags {
                    did_compress: false,
                    verifying: true,
                },
                activity_serial,
                ..Default::default()
            };
            return IncrementalCompressionResult::Pending;
        }

        // Leave the state fresh while idle so later activity naturally begins at
        // the oldest cold page, including pages restored after this pass.
        self.page_compression.reset();
        IncrementalCompressionResult::Complete
    }

    /// Return a metadata-only snapshot of page backing storage.
    ///
    /// This never restores compressed pages. It is intended for diagnostics and
    /// other infrequent reporting because it traverses the complete page list.
    ///
    /// PORTS: oracle@main PageList.zig `memoryStats`.
    pub fn memory_stats(&self) -> MemoryStats {
        let mut result = MemoryStats::default();
        let mut current = self.pages.first;
        while !current.is_null() {
            let node = current;
            // SAFETY: `node` is a live node of this list; metadata never
            // restores a compressed page.
            let raw_len = unsafe { (*Node::metadata(node)).memory_len() };
            let backing_len = match unsafe { (*node).owned } {
                super::Owned::Pool => std_size(),
                super::Owned::Heap => raw_len,
            };
            debug_assert!(backing_len >= raw_len);
            result.raw_bytes += raw_len;

            // PORT NOTE (WS7 I2 discipline, Stacked Borrows): raw-derived like
            // every other `NodeData` access in this module — `NodeData::tag`
            // reads the discriminant byte and `NodeData::compressed_payload`
            // projects the payload via a constant `offset_of!`, both taking
            // `data` by raw pointer so neither forms a `&NodeData` that could
            // be invalidated by a later retag on this same node. (The prior
            // `&*addr_of!(...)` match was sound as written here — a read-only
            // walk with no raw alias held across it — but this keeps the
            // whole module on one discipline instead of being the one holdout.)
            let data = unsafe { core::ptr::addr_of!((*node).data) };
            match unsafe { NodeData::tag(data) } {
                NodeData::TAG_RESIDENT => {
                    result.resident_pages += 1;
                    result.resident_raw_bytes += raw_len;
                    result.resident_backing_bytes += backing_len;
                }

                _ => {
                    // TAG_COMPRESSED (the enum has exactly two variants).
                    let compressed = unsafe { &*NodeData::compressed_payload(data) };
                    result.compressed_pages += 1;
                    result.decommitted_raw_bytes += raw_len;
                    // Strict reclamation covers only Page.memory. A standard
                    // pool item can have an unused tail which remains resident.
                    result.resident_backing_bytes += backing_len - raw_len;
                    result.encoded_bytes += compressed.encoded.len();
                }
            }

            current = unsafe { (*node).next };
        }

        result
    }

    /// Compress every fully historical resident page which is currently cold.
    ///
    /// PORTS: oracle@main PageList.zig `compressFull`.
    pub(crate) fn compress_full(&mut self) {
        let mut it = CompressionIterator::init(self);
        while let Some(node) = it.next() {
            // Don't restore an already-compressed page just to recompress it.
            if unsafe { Node::is_compressed(node) } {
                continue;
            }

            // Failure leaves this node resident and unchanged.
            let _ = self.compress_page(node);
        }
    }
}
