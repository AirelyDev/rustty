//! PageList page-compression policy.
//!
//! Which pages to compress, when to discard their resident memory, and when to
//! restore them is decided here; the representation and codec live in
//! `crate::compress`.
//!
//! PORTS: the compression sections of ghostty src/terminal/PageList.zig
//! (oracle origin/main; commits 9156ada16, 70e788f06, e7969ed43, f8c217e55,
//! 181254d36, 95685afd2).

use super::{
    std_size, MemoryPool, Node, NodeData, OutOfMemory, PageList,
};
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
// The non-test consumer is `PageList::compress` (WS7 B8).
#[allow(dead_code)]
pub(crate) enum CompressionScratch {
    /// A borrowed pool item. Always exactly `std_size()` bytes.
    Pooled(*mut u8),
    /// A temporary page-allocator allocation of `len` bytes.
    Allocated { ptr: *mut u8, len: usize },
}

#[allow(dead_code)]
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
                let _ = unsafe {
                    mem::decommit(mem::DecommitMode::Zero, ptr, std_size(), std_size())
                };
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

/// Iterate complete historical pages which do not intersect the viewport.
///
/// All boundaries come from PageList pins and Page metadata. Advancing this
/// iterator never restores a compressed page or reads its backing memory.
///
/// PORTS: oracle@main PageList.zig `CompressionIterator`.
// The non-test consumer is `PageList::compress` (WS7 B8).
#[allow(dead_code)]
pub(crate) struct CompressionIterator {
    current: *mut Node,
    active: *mut Node,
    viewport_first: *mut Node,
    viewport_last: *mut Node,
}

#[allow(dead_code)]
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

#[allow(dead_code)]
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
