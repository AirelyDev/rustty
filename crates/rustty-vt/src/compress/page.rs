//! A compressed terminal page which retains its resident virtual mapping.
//!
//! Terminal pages have two kinds of state: the large `Page.memory` allocation
//! and a comparatively small `Page` value containing offsets, dimensions,
//! dirty state, and allocator metadata. Not all of the latter state lives in
//! the backing memory, so preserving only the memory bytes is insufficient.
//! We preserve the complete `Page` value instead. This follows the same model
//! as `Page::clone_buf`: page internals use offsets, so a shallow page copy
//! remains valid when its memory contents are restored at the same address.
//!
//! The resident memory is deliberately not freed by this type. `PageList`
//! keeps the virtual range allocated while asking the operating system to
//! discard its physical pages. Keeping the range has two useful properties:
//! the embedded page never contains a dangling pointer, and restoring the page
//! does not require a fallible allocation.
//!
//! The intended state transition is:
//!
//! 1. Create this value while the source page is resident.
//! 2. Ask the OS to decommit the source page's memory.
//! 3. Replace the PageList node's resident state with this value.
//! 4. To restore, recommit the retained range and call `restore`.
//! 5. After committing the resident state, drop this value to free the encoded
//!    bytes.
//!
//! If decommit is unavailable or fails, the caller should drop the compressed
//! candidate and leave the source page resident. This type does not perform any
//! virtual-memory operations itself.
//!
//! The native implementation uses `MADV_DONTNEED` on Linux and pairs
//! `MADV_FREE_REUSABLE` with `MADV_FREE_REUSE` on Darwin (see [`crate::mem`]).
//! Targets without a reliable retained-mapping decommit operation should leave
//! page compression disabled rather than free and later reallocate the resident
//! memory.
//!
//! PORTS: ghostty src/terminal/compress/Page.zig (oracle origin/main).
//!
//! PORT NOTE (WS7): the oracle stores an `alloc: Allocator` so a node can free
//! its encoding without a back-reference to the PageList. rustty's `Vec<u8>`
//! carries its `Global` allocator, so that field is unnecessary and the
//! oracle's `deinit` is just `Drop`.
//!
//! PORT NOTE (WS7): `init` takes a RAW `*const Page`, never `&Page`.
//! `PageList::compress_page` holds a `*mut Node` whose `data` union it is about
//! to overwrite; keeping a shared reference to the embedded page live across
//! that write is Stacked-Borrows UB in this port.

use super::lz4;
use crate::page::Page;

/// Complete page metadata together with the retained resident mapping.
pub struct CompressedPage {
    /// The bytes in `page`'s mapping may have been discarded by the OS while
    /// this value is in compressed state. They must not be read until `restore`
    /// has successfully decoded the page.
    pub page: Page,

    /// Exact raw LZ4 block for the page's mapping.
    pub encoded: Vec<u8>,
}

/// Return the largest scratch buffer that can produce a useful compressed
/// representation for `raw_len` bytes.
///
/// This is deliberately smaller than the general LZ4 compression bound. The
/// compressed representation includes an additional allocation compared to a
/// resident terminal page, so an encoded block which fills more than this
/// buffer cannot reduce resident memory. Limiting the output here lets a
/// PageList borrow a standard page-pool item as scratch instead of retaining a
/// larger, compression-bound allocation.
///
/// Callers are expected to reuse the scratch memory when practical. The
/// scratch buffer and hash table are never retained by this type.
///
/// PORTS: oracle@main compress/Page.zig `requiredScratch`.
pub fn required_scratch(raw_len: usize) -> Result<usize, lz4::CompressError> {
    // Validate the codec's input limit before doing arithmetic based on the
    // raw size. The bound itself is intentionally not returned; see above.
    let _ = lz4::compress_bound(raw_len)?;

    let representation_overhead =
        core::mem::size_of::<CompressedPage>() - core::mem::size_of::<Page>();
    if raw_len <= representation_overhead {
        return Ok(0);
    }

    // The savings comparison is strict, so reserve one fewer byte than the
    // break-even encoded size.
    Ok(raw_len - representation_overhead - 1)
}

impl CompressedPage {
    /// Create a compressed representation of `source`.
    ///
    /// `source` is not modified and continues to own its backing allocation.
    /// The scratch buffer must be at least
    /// `required_scratch(source.memory_len())` bytes. It must not overlap the
    /// source memory. On success, the returned value aliases the source's
    /// resident mapping and owns only its exact-sized `encoded` allocation.
    ///
    /// Returns `Ok(None)` if retaining the compressed representation would not
    /// use less memory than the resident representation. An `OutputTooSmall`
    /// result from the codec also means the encoding crossed that break-even
    /// point and is therefore returned as `Ok(None)`. This comparison includes
    /// the in-memory size of both representation structs but does not include
    /// allocator metadata or the retained virtual address range.
    ///
    /// PORTS: oracle@main compress/Page.zig `init`.
    ///
    /// # Safety
    /// `source` must point at a live `Page` whose mapping is resident and does
    /// not overlap `scratch`.
    pub unsafe fn init(
        source: *const Page,
        scratch: &mut [u8],
        table: &mut lz4::HashTable,
    ) -> Result<Option<CompressedPage>, lz4::CompressError> {
        // SAFETY: caller contract — `source` is a live Page. We read only the
        // two mapping fields through the raw pointer, never forming a `&Page`.
        let (raw_ptr, raw_len) = unsafe {
            (
                core::ptr::read(core::ptr::addr_of!((*source).memory_ptr)),
                core::ptr::read(core::ptr::addr_of!((*source).memory_len)),
            )
        };

        let required = required_scratch(raw_len)?;
        if scratch.len() < required {
            return Err(lz4::CompressError::OutputTooSmall);
        }
        if required == 0 {
            return Ok(None);
        }

        // SAFETY: the page's mapping is resident and does not overlap scratch.
        let input = unsafe { core::slice::from_raw_parts(raw_ptr as *const u8, raw_len) };
        let encoded_len = match lz4::compress(input, &mut scratch[..required], table) {
            Ok(n) => n,
            // The scratch limit is the largest representation worth retaining.
            // Running out of room therefore means compression cannot save
            // memory, rather than that the caller failed to provide the
            // documented size.
            Err(lz4::CompressError::OutputTooSmall) => return Ok(None),
            Err(e) => return Err(e),
        };

        // required_scratch already accounts for the representation structs. The
        // retained virtual range contributes no resident bytes after PageList
        // decommits it.
        debug_assert!(
            core::mem::size_of::<CompressedPage>() + encoded_len
                < core::mem::size_of::<Page>() + raw_len
        );

        Ok(Some(CompressedPage {
            // SAFETY: caller contract — `source` is a live Page.
            page: unsafe { (*source).shallow_copy() },
            encoded: scratch[..encoded_len].to_vec(),
        }))
    }

    /// Restore the embedded terminal page into its retained resident mapping.
    ///
    /// The caller must recommit the mapping before calling this on platforms
    /// which require an explicit recommit operation. The compressed value
    /// remains valid whether decoding succeeds or fails, allowing the caller to
    /// retry or discard it. On success the returned page aliases the same
    /// resident mapping; it does not own a new allocation.
    ///
    /// PORTS: oracle@main compress/Page.zig `restore`.
    ///
    /// # Safety
    /// The retained mapping must be recommitted and unaliased.
    pub unsafe fn restore(&self) -> Result<Page, lz4::DecompressError> {
        let result = self.page.shallow_copy();
        // SAFETY: caller contract — the retained mapping is live, unaliased and
        // exactly `memory_len` bytes.
        let out = unsafe {
            core::slice::from_raw_parts_mut(result.memory_ptr as *mut u8, result.memory_len())
        };
        lz4::decompress(&self.encoded, out)?;
        Ok(result)
    }

    /// Clone this page into caller-owned memory without restoring its mapping.
    ///
    /// `memory` must be at least as large as the retained page mapping.
    /// Decoding writes only to that buffer, so both the discarded contents of
    /// the retained mapping and this value's encoded representation remain
    /// unchanged. The returned Page borrows `memory`; the caller must keep it
    /// alive for the Page's lifetime and release it directly rather than
    /// calling `Page::deinit`.
    ///
    /// PORTS: oracle@main compress/Page.zig `cloneBuf`.
    ///
    /// # Safety
    /// `memory` must be a live, page-aligned, unaliased allocation of at least
    /// `memory_len >= self.page.memory_len()` bytes.
    pub unsafe fn clone_buf(
        &self,
        memory: *mut u8,
        memory_len: usize,
    ) -> Result<Page, lz4::DecompressError> {
        debug_assert!(memory_len >= self.page.memory_len());

        // Page internals are offsets into the backing buffer, so all metadata
        // can be copied verbatim when paired with an equally laid-out mapping.
        let mut result = self.page.shallow_copy();
        result.set_memory(memory, self.page.memory_len());
        // SAFETY: caller contract — `memory` is live and unaliased for at least
        // `self.page.memory_len()` bytes.
        let out = unsafe { core::slice::from_raw_parts_mut(memory, self.page.memory_len()) };
        lz4::decompress(&self.encoded, out)?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hyperlink;
    use crate::page::{Capacity, Cell, PageSize, PAGE_SIZE_MIN};
    use crate::style::{self as stylepkg, Style};

    fn aligned_alloc(len: usize) -> (*mut u8, std::alloc::Layout) {
        let layout = std::alloc::Layout::from_size_align(len, PAGE_SIZE_MIN).unwrap();
        // SAFETY: len > 0 and the layout is valid.
        let ptr = unsafe { std::alloc::alloc_zeroed(layout) };
        assert!(!ptr.is_null());
        (ptr, layout)
    }

    fn bytes(ptr: *const u8, len: usize) -> Vec<u8> {
        // SAFETY: the caller passes a live mapping of `len` bytes.
        unsafe { core::slice::from_raw_parts(ptr, len) }.to_vec()
    }

    /// PORTS: oracle@main compress/Page.zig
    /// `test "compressed Page retained mapping round trip"`
    #[test]
    fn compressed_page_retained_mapping_round_trip() {
        let mut resident = Page::init(Capacity {
            cols: 12,
            rows: 9,
            styles: 8,
            grapheme_bytes: 128,
            string_bytes: 128,
            ..crate::page::std_capacity()
        })
        .unwrap();
        resident.size = PageSize { cols: 10, rows: 7 };
        resident.dirty = true;

        // Put state in both the backing memory and the Page value. In
        // particular, the style and hyperlink sets retain live counters
        // outside Page.memory.
        let rac = resident.get_row_and_cell(2, 3);
        unsafe {
            *rac.cell = Cell::init('A' as u32);
            resident.append_grapheme(rac.row, rac.cell, 0x0301).unwrap();

            let base = resident.memory_ptr();
            let style_id = resident
                .styles
                .add(
                    base,
                    Style {
                        flags: stylepkg::Flags {
                            bold: true,
                            ..stylepkg::Flags::default()
                        },
                        ..Style::default()
                    },
                )
                .unwrap();
            (*rac.cell).set_style_id(style_id);
            (*rac.row).set_styled(true);

            let hyperlink_id = resident
                .insert_hyperlink(hyperlink::Hyperlink {
                    id: hyperlink::HyperlinkId::Explicit(b"compressed-page".to_vec()),
                    uri: b"https://rustty.org/docs".to_vec(),
                })
                .unwrap();
            resident
                .set_hyperlink(rac.row, rac.cell, hyperlink_id)
                .unwrap();
        }
        resident.verify_integrity().unwrap();

        let memory_ptr = resident.memory_ptr() as *mut u8;
        let memory_len = resident.memory_len();
        let expected = bytes(memory_ptr, memory_len);

        let mut scratch = vec![0u8; required_scratch(memory_len).unwrap()];
        let mut table: lz4::HashTable = [0; lz4::HASH_TABLE_LEN];
        let compressed =
            unsafe { CompressedPage::init(&resident as *const Page, &mut scratch, &mut table) }
                .unwrap()
                .unwrap();

        assert_eq!(compressed.page.memory_ptr(), memory_ptr as *const u8);
        assert_eq!(compressed.page.memory_len(), memory_len);
        assert!(compressed.encoded.len() < memory_len);
        let expected_encoded = compressed.encoded.clone();

        // Virtual-memory operations belong to PageList, so clearing the
        // contents models a successful decommit: none of the resident bytes
        // remain.
        unsafe { core::ptr::write_bytes(memory_ptr, 0, memory_len) };

        // A clone decodes into independent storage for read-only consumers
        // which must not change the compressed representation. In particular,
        // it does not recommit or overwrite the retained source mapping.
        let (clone_memory, layout) = aligned_alloc(memory_len);
        let cloned = unsafe { compressed.clone_buf(clone_memory, memory_len) }.unwrap();
        assert_ne!(cloned.memory_ptr(), memory_ptr as *const u8);
        assert!(bytes(memory_ptr, memory_len).iter().all(|&b| b == 0));
        assert_eq!(compressed.encoded, expected_encoded);
        assert_eq!(bytes(cloned.memory_ptr(), memory_len), expected);
        assert_eq!(cloned.size, resident.size);
        assert!(cloned.dirty);
        cloned.verify_integrity().unwrap();
        drop(cloned);
        unsafe { std::alloc::dealloc(clone_memory, layout) };

        let restored = unsafe { compressed.restore() }.unwrap();
        assert_eq!(restored.memory_ptr(), memory_ptr as *const u8);
        assert_eq!(restored.memory_len(), memory_len);
        assert_eq!(bytes(restored.memory_ptr(), memory_len), expected);
        assert_eq!(restored.size, resident.size);
        assert!(restored.dirty);
        restored.verify_integrity().unwrap();

        let restored_rac = restored.get_row_and_cell(2, 3);
        assert_eq!(
            restored.lookup_grapheme(restored_rac.cell).unwrap(),
            &[0x0301u32][..]
        );
        assert!(unsafe {
            (*restored
                .styles
                .get(restored.memory_ptr(), (*restored_rac.cell).style_id()))
            .flags
            .bold
        });

        let restored_hyperlink_id = restored.lookup_hyperlink(restored_rac.cell).unwrap();
        let base = restored.memory_ptr();
        let restored_hyperlink = unsafe { &*restored.hyperlink_set.get(base, restored_hyperlink_id) };
        assert_eq!(
            unsafe { restored_hyperlink.uri.slice(base) },
            b"https://rustty.org/docs"
        );

        drop(restored);
        drop(compressed);
        resident.deinit();
    }

    /// PORTS: oracle@main compress/Page.zig
    /// `test "compressed Page requires the maximum useful scratch"`
    #[test]
    fn compressed_page_requires_the_maximum_useful_scratch() {
        let mut resident = Page::init(Capacity::new(4, 4)).unwrap();
        let memory_len = resident.memory_len();
        let expected = bytes(resident.memory_ptr(), memory_len);

        let required = required_scratch(memory_len).unwrap();
        assert_eq!(
            memory_len
                - (core::mem::size_of::<CompressedPage>() - core::mem::size_of::<Page>())
                - 1,
            required,
        );
        assert!(required < lz4::compress_bound(memory_len).unwrap());

        let mut scratch = vec![0u8; required - 1];
        let mut table: lz4::HashTable = [0; lz4::HASH_TABLE_LEN];

        assert_eq!(
            unsafe { CompressedPage::init(&resident as *const Page, &mut scratch, &mut table) }
                .err(),
            Some(lz4::CompressError::OutputTooSmall),
        );
        assert_eq!(bytes(resident.memory_ptr(), memory_len), expected);

        resident.deinit();
    }

    /// PORTS: oracle@main compress/Page.zig
    /// `test "compressed Page rejects a representation without savings"`
    ///
    /// PORT NOTE (WS7 B3): the oracle proves "no allocation happened" with a
    /// `FailingAllocator` whose `fail_index` is 0. rustty's `Vec` uses the
    /// infallible global allocator, so the equivalent assertion is that `init`
    /// returns `Ok(None)` — the early return is taken before any allocation —
    /// and that the source page's bytes are unchanged.
    #[test]
    fn compressed_page_rejects_a_representation_without_savings() {
        let mut resident = Page::init(Capacity {
            cols: 4,
            rows: 4,
            styles: 0,
            grapheme_bytes: 0,
            string_bytes: 0,
            hyperlink_bytes: 0,
        })
        .unwrap();
        let memory_len = resident.memory_len();

        // Incompressible content: a deterministic PRNG stream.
        let ptr = resident.memory_ptr() as *mut u8;
        let mut state: u64 = 0x4C5A_3402;
        for i in 0..memory_len {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            unsafe { *ptr.add(i) = (state >> 33) as u8 };
        }
        let expected = bytes(ptr, memory_len);

        let mut scratch = vec![0u8; required_scratch(memory_len).unwrap()];
        let mut table: lz4::HashTable = [0; lz4::HASH_TABLE_LEN];

        assert!(
            unsafe { CompressedPage::init(&resident as *const Page, &mut scratch, &mut table) }
                .unwrap()
                .is_none()
        );
        assert_eq!(bytes(ptr, memory_len), expected);

        resident.deinit();
    }

    /// PORTS: oracle@main compress/Page.zig
    /// `test "compressed Page can retry after malformed encoded data"`
    #[test]
    fn compressed_page_can_retry_after_malformed_encoded_data() {
        let mut resident = Page::init(Capacity::new(8, 8)).unwrap();
        let memory_ptr = resident.memory_ptr() as *mut u8;
        let memory_len = resident.memory_len();
        let expected = bytes(memory_ptr, memory_len);

        let mut scratch = vec![0u8; required_scratch(memory_len).unwrap()];
        let mut table: lz4::HashTable = [0; lz4::HASH_TABLE_LEN];
        let mut compressed =
            unsafe { CompressedPage::init(&resident as *const Page, &mut scratch, &mut table) }
                .unwrap()
                .unwrap();

        let full_encoded = compressed.encoded.clone();
        compressed.encoded = vec![0xF0];

        let (clone_memory, layout) = aligned_alloc(memory_len);
        assert_eq!(
            unsafe { compressed.clone_buf(clone_memory, memory_len) }.err(),
            Some(lz4::DecompressError::TruncatedInput),
        );
        assert_eq!(
            unsafe { compressed.restore() }.err(),
            Some(lz4::DecompressError::TruncatedInput),
        );
        unsafe { std::alloc::dealloc(clone_memory, layout) };

        compressed.encoded = full_encoded;
        unsafe { core::ptr::write_bytes(memory_ptr, 0, memory_len) };
        let restored = unsafe { compressed.restore() }.unwrap();
        assert_eq!(bytes(restored.memory_ptr(), memory_len), expected);

        drop(restored);
        drop(compressed);
        resident.deinit();
    }
}
