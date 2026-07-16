// PORT NOTE: this file is `include!`d into `page.rs` (it is a continuation of
// the same module, split only for readability of a very large file). It holds
// the `Page` struct, its `Layout`, and all methods.

/// `Page.Layout`
#[derive(Clone, Copy)]
pub struct Layout {
    pub total_size: usize,
    pub rows_start: usize,
    pub rows_size: usize,
    pub cells_start: usize,
    pub cells_size: usize,
    pub styles_start: usize,
    pub styles_layout: crate::ref_counted_set::Layout<StyleId>,
    pub grapheme_alloc_start: usize,
    pub grapheme_alloc_layout: crate::bitmap_allocator::Layout,
    pub grapheme_map_start: usize,
    pub grapheme_map_layout: crate::hash_map::Layout,
    pub string_alloc_start: usize,
    pub string_alloc_layout: crate::bitmap_allocator::Layout,
    pub hyperlink_map_start: usize,
    pub hyperlink_map_layout: crate::hash_map::Layout,
    pub hyperlink_set_start: usize,
    pub hyperlink_set_layout: crate::ref_counted_set::Layout<hyperlink::Id>,
    pub capacity: Capacity,
}

/// `Page.IntegrityError`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntegrityError {
    ZeroRowCount,
    ZeroColCount,
    UnmarkedGraphemeRow,
    MissingGraphemeData,
    InvalidGraphemeCount,
    UnmarkedGraphemeCell,
    MissingStyle,
    UnmarkedStyleRow,
    MismatchedStyleRef,
    InvalidStyleCount,
    MissingHyperlinkData,
    MismatchedHyperlinkRef,
    UnmarkedHyperlinkCell,
    UnmarkedHyperlinkRow,
    InvalidSpacerTailLocation,
    InvalidSpacerHeadLocation,
    UnwrappedSpacerHead,
}

/// `Page.StyleSetError`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StyleSetError {
    StyleSetOutOfMemory,
    StyleSetNeedsRehash,
}

/// `Page.HyperlinkError`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HyperlinkError {
    StringAllocOutOfMemory,
    HyperlinkSetOutOfMemory,
    HyperlinkSetNeedsRehash,
    HyperlinkMapOutOfMemory,
}

/// `Page.GraphemeError`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphemeError {
    GraphemeMapOutOfMemory,
    GraphemeAllocOutOfMemory,
}

/// `Page.CloneFromError = StyleSetError || HyperlinkError || GraphemeError`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloneFromError {
    // StyleSetError
    StyleSetOutOfMemory,
    StyleSetNeedsRehash,
    // HyperlinkError
    StringAllocOutOfMemory,
    HyperlinkSetOutOfMemory,
    HyperlinkSetNeedsRehash,
    HyperlinkMapOutOfMemory,
    // GraphemeError
    GraphemeMapOutOfMemory,
    GraphemeAllocOutOfMemory,
}

/// `Page.InsertHyperlinkError`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InsertHyperlinkError {
    /// string_alloc errors
    StringsOutOfMemory,

    /// hyperlink_set errors
    SetOutOfMemory,
    SetNeedsRehash,
}

/// A page represents a specific section of terminal screen. The primary
/// idea of a page is that it is a fully self-contained unit that can be
/// serialized, copied, etc. as a convenient way to represent a section
/// of the screen.
///
/// This property is useful for renderers which want to copy just the pages
/// for the visible portion of the screen, or for infinite scrollback where
/// we may want to serialize and store pages that are sufficiently far
/// away from the current viewport.
///
/// Pages are always backed by a single contiguous block of memory that is
/// aligned on a page boundary. This makes it easy and fast to copy pages
/// around. Within the contiguous block of memory, the contents of a page are
/// thoughtfully laid out to optimize primarily for terminal IO (VT streams)
/// and to minimize memory usage.
pub struct Page {
    /// The backing memory for the page. A page is always made up of a
    /// a single contiguous block of memory that is aligned on a page
    /// boundary and is a multiple of the system page size.
    // PORT NOTE: the reference is `memory: []align(page_size_min) u8`. We keep
    // the raw pointer/len; ownership/freeing is governed by whether the page was
    // created via `init` (owns the mmap) or `init_buf` (borrows). The `owned`
    // flag tracks this so `deinit` only frees `init`-created pages, matching the
    // reference's "do NOT call deinit if you used initBuf" contract.
    pub(crate) memory_ptr: *mut u8,
    pub(crate) memory_len: usize,
    owned: bool,

    /// The array of rows in the page. The rows are always in row order
    /// (i.e. index 0 is the top row, index 1 is the row below that, etc.)
    pub rows: Offset<Row>,

    /// The array of cells in the page. The cells are NOT in row order,
    /// but they are in column order. To determine the mapping of cells
    /// to row, you must use the `rows` field. From the pointer to the
    /// first column, all cells in that row are laid out in column order.
    pub cells: Offset<Cell>,

    /// Set to true when an operation is performed that dirties all rows in
    /// the page. See `Row.dirty` for more information on dirty tracking.
    ///
    /// NOTE: A value of false does NOT indicate that
    ///       the page has no dirty rows in it, only
    ///       that no full-page-dirtying operations
    ///       have occurred since it was last cleared.
    pub dirty: bool,

    /// The string allocator for this page used for shared utf-8 encoded
    /// strings. Liveness of strings and memory management is deferred to
    /// the individual use case.
    pub string_alloc: StringAlloc,

    /// The multi-codepoint grapheme data for this page. This is where
    /// any cell that has more than one codepoint will be stored. This is
    /// relatively rare (typically only emoji) so this defaults to a very small
    /// size and we force page realloc when it grows.
    pub grapheme_alloc: GraphemeAlloc,

    /// The mapping of cell to grapheme data. The exact mapping is the
    /// cell offset to the grapheme data offset. Therefore, whenever a
    /// cell is moved (i.e. `erase`) then the grapheme data must be updated.
    /// Grapheme data is relatively rare so this is considered a slow
    /// path.
    pub grapheme_map: GraphemeMap,

    /// The available set of styles in use on this page.
    pub styles: StyleSet,

    /// The structures used for tracking hyperlinks within the page.
    /// The map maps cell offsets to hyperlink IDs and the IDs are in
    /// the ref counted set. The strings within the hyperlink structures
    /// are allocated in the string allocator.
    pub hyperlink_map: hyperlink::Map,
    pub hyperlink_set: hyperlink::Set,

    /// The current dimensions of the page. The capacity may be larger
    /// than this. This allows us to allocate a larger page than necessary
    /// and also to resize a page smaller without reallocating.
    pub size: PageSize,

    /// The capacity of this page. This is the full size of the backing
    /// memory and is fixed at page creation time.
    pub capacity: Capacity,

    /// If this is true then verifyIntegrity will do nothing. This is
    /// only present with runtime safety enabled.
    // PORT NOTE: the reference gates this field behind
    // `build_options.slow_runtime_safety` (`usize` when on, `void` when off).
    // We always carry a `usize` and gate its use with `cfg!(debug_assertions)`.
    pause_integrity_checks: usize,
}

impl Page {
    /// Returns the backing memory base pointer for offset resolution.
    ///
    /// PORT NOTE: the reference field is `memory: []align(page_size_min) u8` (a
    /// slice). Offset-based accessors only need the base pointer; we expose the
    /// pointer directly (the length is tracked in `memory_len`).
    //
    // PORT NOTE (visibility): the reference `Page.memory` field is `pub`, and the
    // C boundary (rustty-vt-c `grid_ref_hyperlink_uri`/`grid_ref_style`) resolves
    // offset-based data (`hyperlink_set.get(memory, id)`, `entry.uri.slice(memory)`,
    // `styles.get(memory, id)`) against it exactly as the reference does. This
    // accessor is therefore `pub` (was `pub(crate)`) to project the reference's
    // public `memory` base across the crate boundary — a non-fabricating seam (the
    // base pointer is the reference's own public datum).
    #[inline]
    pub fn memory_ptr(&self) -> *const u8 {
        self.memory_ptr
    }

    /// Returns the length in bytes of the page's backing memory.
    //
    // PORT NOTE (visibility): the reference field `Page.memory` is a `pub` slice,
    // so both its base pointer AND its length are public data. `memory_ptr`
    // already projects the base; this projects the `.len` half. The inspector's
    // `page` widget reads it for `units.toKibiBytes(page.memory.len)`
    // (inspector/widgets/page.zig:109) — a non-fabricating seam over the
    // reference's own public datum.
    #[inline]
    pub fn memory_len(&self) -> usize {
        self.memory_len
    }

    /// Reads the backing memory base pointer through a raw `*const Page` without
    /// forming a `&Page` over the whole struct.
    ///
    /// This is used by the hyperlink set context, which runs while the set (a
    /// field of the same `Page`) is mutably borrowed: forming a `&Page` would
    /// retag — and conflict with — the strongly-protected `&mut hyperlink_set`.
    /// Projecting to just the `memory_ptr` field via `addr_of!` avoids touching
    /// the protected sub-range.
    ///
    /// # Safety
    ///
    /// `page` must point at a live `Page`.
    #[inline]
    pub(crate) unsafe fn memory_ptr_of(page: *const Page) -> *const u8 {
        unsafe { core::ptr::read(core::ptr::addr_of!((*page).memory_ptr)) as *const u8 }
    }

    /// Reads the `size` field through a raw `*const Page` via field projection
    /// (see `memory_ptr_of`). Used by `clone_partial_row_from` where the source
    /// page may alias `self` (a `&mut self`), making a `&Page` retag UB.
    ///
    /// # Safety
    ///
    /// `page` must point at a live `Page`.
    #[inline]
    pub(crate) unsafe fn size_of(page: *const Page) -> PageSize {
        unsafe { core::ptr::read(core::ptr::addr_of!((*page).size)) }
    }

    /// Reads the `rows` offset through a raw `*const Page` via field projection
    /// (see `size_of`).
    ///
    /// # Safety
    ///
    /// `page` must point at a live `Page`.
    #[inline]
    pub(crate) unsafe fn rows_of(page: *const Page) -> Offset<Row> {
        unsafe { core::ptr::read(core::ptr::addr_of!((*page).rows)) }
    }

    /// Looks up grapheme codepoints for `cell` in `page` via field projection
    /// (reads `page.grapheme_map` without forming a `&Page`). Mirrors
    /// `lookup_grapheme` for a possibly-aliasing source page.
    ///
    /// # Safety
    ///
    /// `page` must point at a live `Page`; `cell` must point into its cells.
    pub(crate) unsafe fn lookup_grapheme_of<'a>(page: *const Page, cell: *const Cell) -> Option<&'a [u32]> {
        let base = unsafe { Page::memory_ptr_of(page) };
        let gm = unsafe { core::ptr::read(core::ptr::addr_of!((*page).grapheme_map)) };
        let cell_offset = unsafe { get_offset::<Cell, _>(base, cell) };
        let map = unsafe { gm.map(base) };
        let actx = AutoContext::default();
        let slice = map.get_context(cell_offset, &actx)?;
        Some(unsafe { slice.slice(base) })
    }

    /// Looks up the hyperlink id for `cell` in `page` via field projection.
    /// Mirrors `lookup_hyperlink` for a possibly-aliasing source page.
    ///
    /// # Safety
    ///
    /// `page` must point at a live `Page`; `cell` must point into its cells.
    pub(crate) unsafe fn lookup_hyperlink_of(
        page: *const Page,
        cell: *const Cell,
    ) -> Option<hyperlink::Id> {
        let base = unsafe { Page::memory_ptr_of(page) };
        let hm = unsafe { core::ptr::read(core::ptr::addr_of!((*page).hyperlink_map)) };
        let cell_offset = unsafe { get_offset::<Cell, _>(base, cell) };
        let map = unsafe { hm.map(base) };
        let actx = AutoContext::default();
        map.get_context(cell_offset, &actx)
    }

    /// Reads a hyperlink set entry from `page` via field projection.
    ///
    /// # Safety
    ///
    /// `page` must point at a live `Page`; `id` must be a valid live entry.
    pub(crate) unsafe fn hyperlink_get_of(
        page: *const Page,
        id: hyperlink::Id,
    ) -> hyperlink::PageEntry {
        let base = unsafe { Page::memory_ptr_of(page) };
        let hs = unsafe { &*core::ptr::addr_of!((*page).hyperlink_set) };
        unsafe { *hs.get(base, id) }
    }

    /// Reads a style set entry from `page` via field projection.
    ///
    /// # Safety
    ///
    /// `page` must point at a live `Page`; `id` must be a valid live entry.
    pub(crate) unsafe fn style_get_of(page: *const Page, id: StyleId) -> Style {
        let base = unsafe { Page::memory_ptr_of(page) };
        let ss = unsafe { &*core::ptr::addr_of!((*page).styles) };
        unsafe { *ss.get(base, id) }
    }

    /// Frees a `string_alloc` byte slice through a raw `*mut Page`, projecting to
    /// the `string_alloc` field only (avoids retagging the whole `Page`, which
    /// would conflict with a strongly-protected `&mut hyperlink_set` when called
    /// from the hyperlink set context's `deleted`/`dupe`).
    ///
    /// # Safety
    ///
    /// `page` must point at a live `Page`; `slice` must have been allocated by
    /// that page's `string_alloc`.
    #[inline]
    pub(crate) unsafe fn string_alloc_free(page: *mut Page, slice: &[u8]) {
        let base = unsafe { Page::memory_ptr_of(page) };
        let sa = unsafe { &mut *core::ptr::addr_of_mut!((*page).string_alloc) };
        unsafe { sa.free(base, slice) };
    }

    /// Allocates from `string_alloc` through a raw `*mut Page`, projecting to the
    /// `string_alloc` field only (see `string_alloc_free`).
    ///
    /// # Safety
    ///
    /// `page` must point at a live `Page`.
    #[inline]
    pub(crate) unsafe fn string_alloc_alloc<'a>(
        page: *mut Page,
        n: usize,
    ) -> Result<&'a mut [u8], crate::bitmap_allocator::OutOfMemory> {
        let base = unsafe { Page::memory_ptr_of(page) };
        let sa = unsafe { &mut *core::ptr::addr_of_mut!((*page).string_alloc) };
        unsafe { sa.alloc::<u8, _>(base, n) }
    }

    // PORT NOTE: the reference has a `comptime` block on `Page`:
    //   comptime {
    //       // The alignment of our members. We want to ensure that the page
    //       // alignment is always divisible by this.
    //       assert(std.heap.page_size_min % @max(
    //           @alignOf(Row),
    //           @alignOf(Cell),
    //           StyleSet.base_align.toByteUnits(),
    //       ) == 0);
    //   }
    // Reproduced as `page_alignment_invariant` test below.

    /// Initialize a new page, allocating the required backing memory.
    /// The size of the initialized page defaults to the full capacity.
    ///
    /// The backing memory is always allocated using mmap directly.
    /// You cannot use custom allocators with this structure because
    /// it is critical to performance that we use mmap.
    pub fn init(cap: Capacity) -> Result<Page, OutOfMemory> {
        let l = Page::layout(cap);

        // We allocate page-aligned zeroed memory directly to avoid Zig
        // allocator overhead (small but meaningful for this path). Both
        // mmap (POSIX) and VirtualAlloc (Windows) guarantee zeroed pages,
        // which is a critical property for us.
        assert!(l.total_size.is_multiple_of(PAGE_SIZE_MIN));
        let backing = page_alloc::alloc(l.total_size).ok_or(OutOfMemory)?;
        // errdefer PageAlloc.free(backing);
        let buf = OffsetBuf::init(backing.ptr as *const u8);
        let page = Page::init_buf(buf, l);
        // Mark as owned so deinit frees the mmap.
        let mut page = page;
        page.owned = true;
        Ok(page)
    }

    /// Initialize a new page using the given backing memory.
    /// It is up to the caller to not call deinit on these pages.
    pub fn init_buf(buf: OffsetBuf, l: Layout) -> Page {
        let cap = l.capacity;
        let rows: Offset<Row> = buf.member(l.rows_start);
        let cells: Offset<Cell> = buf.member(l.cells_start);

        // We need to go through and initialize all the rows so that
        // they point to a valid offset into the cells, since the rows
        // zero-initialized aren't valid.
        let cells_len = (cap.cols as usize) * (cap.rows as usize);
        let cells_ptr: *mut Cell = unsafe { cells.ptr(buf) };
        let cells_slice = unsafe { core::slice::from_raw_parts_mut(cells_ptr, cells_len) };
        let rows_ptr: *mut Row = unsafe { rows.ptr(buf) };
        let rows_slice = unsafe { core::slice::from_raw_parts_mut(rows_ptr, cap.rows as usize) };
        for (y, row) in rows_slice.iter_mut().enumerate() {
            let start = y * (cap.cols as usize);
            let mut r = Row::zeroed();
            r.set_cells(unsafe { get_offset::<Cell, _>(buf, &cells_slice[start]) });
            *row = r;
        }

        let total_size = l.total_size;
        let start = unsafe { buf.start() };

        Page {
            memory_ptr: start,
            memory_len: total_size,
            owned: false,
            rows,
            cells,
            styles: unsafe {
                StyleSet::init(buf.add(l.styles_start), l.styles_layout, stylepkg::SetContext)
            },
            string_alloc: unsafe {
                StringAlloc::init(buf.add(l.string_alloc_start), l.string_alloc_layout)
            },
            grapheme_alloc: unsafe {
                GraphemeAlloc::init(buf.add(l.grapheme_alloc_start), l.grapheme_alloc_layout)
            },
            grapheme_map: unsafe {
                GraphemeMap::init(buf.add(l.grapheme_map_start), l.grapheme_map_layout)
            },
            hyperlink_map: unsafe {
                hyperlink::Map::init(buf.add(l.hyperlink_map_start), l.hyperlink_map_layout)
            },
            hyperlink_set: unsafe {
                hyperlink::Set::init(
                    buf.add(l.hyperlink_set_start),
                    l.hyperlink_set_layout,
                    hyperlink::SetContext::default(),
                )
            },
            size: PageSize {
                cols: cap.cols,
                rows: cap.rows,
            },
            capacity: cap,
            dirty: false,
            pause_integrity_checks: 0,
        }
    }

    /// Deinitialize the page, freeing any backing memory. Do NOT call
    /// this if you allocated the backing memory yourself (i.e. you used
    /// initBuf).
    pub fn deinit(&mut self) {
        if self.owned {
            let backing = page_alloc::Backing {
                ptr: self.memory_ptr,
                len: self.memory_len,
            };
            page_alloc::free(&backing);
        }
        // self.* = undefined;
        self.memory_ptr = core::ptr::null_mut();
        self.memory_len = 0;
        self.owned = false;
    }

    /// Reinitialize the page with the same capacity.
    pub fn reinit(&mut self) {
        // We zero the page memory as u64 instead of u8 because
        // we can and it's empirically quite a bit faster.
        unsafe {
            let p = self.memory_ptr as *mut u64;
            core::slice::from_raw_parts_mut(p, self.memory_len / 8).fill(0);
        }
        let owned = self.owned;
        let buf = OffsetBuf::init(self.memory_ptr as *const u8);
        let new = Page::init_buf(buf, Page::layout(self.capacity));
        // Preserve ownership (init_buf sets owned=false).
        *self = new;
        self.owned = owned;
    }

    /// Temporarily pause integrity checks. This is useful when you are
    /// doing a lot of operations that would trigger integrity check
    /// violations but you know the page will end up in a consistent state.
    pub fn pause_integrity_checks(&mut self, v: bool) {
        if cfg!(debug_assertions) {
            if v {
                self.pause_integrity_checks += 1;
            } else {
                self.pause_integrity_checks -= 1;
            }
        }
    }

    /// A helper that can be used to assert the integrity of the page
    /// when runtime safety is enabled. This is a no-op when runtime
    /// safety is disabled. This uses the libc allocator.
    pub fn assert_integrity(&self) {
        if cfg!(debug_assertions) {
            // PORT NOTE: the reference constructs a DebugAllocator and panics on
            // any integrity violation. Our `verify_integrity` uses Rust's global
            // allocator (no explicit allocator needed); same panic-on-violation
            // behavior.
            if let Err(err) = self.verify_integrity() {
                // log.err("page integrity violation, crashing. err={}", .{err});
                panic!("page integrity violation: {:?}", err);
            }
        }
    }

    /// Verifies the integrity of the page data. This is not fast,
    /// but it is useful for assertions, deserialization, etc. The
    /// allocator is only used for temporary allocations -- all memory
    /// is freed before this function returns.
    ///
    /// Integrity errors are also logged as warnings.
    pub fn verify_integrity(&self) -> Result<(), IntegrityError> {
        // Some things that seem like we should check but do not:
        //
        // - We do not check that the style ref count is exact, only that
        //   it is at least what we see. We do this because some fast paths
        //   trim rows without clearing data.
        // - We do not check that styles seen is exactly the same as the
        //   styles count in the page for the same reason as above.
        // - We only check that we saw less graphemes than the total memory
        //   used for the same reason as styles above.
        //

        // We don't run integrity checks on Valgrind because its soooooo slow,
        // Valgrind is our integrity checker, and we run these during unit
        // tests (non-Valgrind) anyways so we're verifying anyways.
        // PORT NOTE: `std.valgrind.runningOnValgrind()` has no Rust equivalent
        // here; under Miri/native test we always run the checks (Miri is our
        // integrity checker, analogous to Valgrind).

        if cfg!(debug_assertions) && self.pause_integrity_checks > 0 {
            return Ok(());
        }

        if self.size.rows == 0 {
            // log.warn("page integrity violation zero row count", .{});
            return Err(IntegrityError::ZeroRowCount);
        }
        if self.size.cols == 0 {
            // log.warn("page integrity violation zero col count", .{});
            return Err(IntegrityError::ZeroColCount);
        }

        let base = self.memory_ptr();

        let mut graphemes_seen: usize = 0;
        let mut styles_seen: HashMap<StyleId, usize> = HashMap::new();
        let mut hyperlinks_seen: HashMap<hyperlink::Id, usize> = HashMap::new();

        let grapheme_count = self.grapheme_count();

        let rows = unsafe {
            core::slice::from_raw_parts(self.rows.ptr(base), self.size.rows as usize)
        };
        // PORT NOTE: the reference binds `y` for log-warning messages (kept as
        // comments here); the index is otherwise unused so we drop it.
        for row in rows.iter() {
            let graphemes_start = graphemes_seen;
            let cells = unsafe {
                core::slice::from_raw_parts(row.cells().ptr(base), self.size.cols as usize)
            };
            for (x, cell) in cells.iter().enumerate() {
                if cell.has_grapheme() {
                    // If a cell has grapheme data, it must be present in
                    // the grapheme map.
                    if self.lookup_grapheme(cell).is_none() {
                        // log.warn("...grapheme data missing", ...);
                        return Err(IntegrityError::MissingGraphemeData);
                    }

                    graphemes_seen += 1;
                } else if grapheme_count > 0 {
                    // It should not have grapheme data if it isn't marked.
                    // The grapheme_count check above is just an optimization
                    // to speed up integrity checks.
                    if self.lookup_grapheme(cell).is_some() {
                        // log.warn("...cell not marked as grapheme", ...);
                        return Err(IntegrityError::UnmarkedGraphemeCell);
                    }
                }

                if cell.style_id() != stylepkg::DEFAULT_ID {
                    // If a cell has a style, it must be present in the styles
                    // set. Accessing it with `get` asserts that.
                    let _ = unsafe { self.styles.get(base, cell.style_id()) };

                    if !row.styled() {
                        // log.warn("...row not marked as styled", ...);
                        return Err(IntegrityError::UnmarkedStyleRow);
                    }

                    let entry = styles_seen.entry(cell.style_id()).or_insert(0);
                    *entry += 1;
                }

                if cell.hyperlink() {
                    let id = match self.lookup_hyperlink(cell) {
                        Some(id) => id,
                        None => {
                            // log.warn("...hyperlink data missing", ...);
                            return Err(IntegrityError::MissingHyperlinkData);
                        }
                    };

                    if !row.hyperlink() {
                        // log.warn("...row not marked as hyperlink", ...);
                        return Err(IntegrityError::UnmarkedHyperlinkRow);
                    }

                    let entry = hyperlinks_seen.entry(id).or_insert(0);
                    *entry += 1;

                    // Hyperlink ID should be valid. This just straight crashes
                    // if this fails due to assertions.
                    let _ = unsafe { self.hyperlink_set.get(base, id) };
                } else {
                    // It should not have hyperlink data if it isn't marked
                    if self.lookup_hyperlink(cell).is_some() {
                        // log.warn("...cell not marked as hyperlink", ...);
                        return Err(IntegrityError::UnmarkedHyperlinkCell);
                    }
                }

                match cell.wide() {
                    Wide::Narrow => {}
                    Wide::Wide => {}

                    Wide::SpacerTail => {
                        // Spacer tails can't be at the start because they follow
                        // a wide char.
                        if x == 0 {
                            // log.warn("...spacer tail at start", ...);
                            return Err(IntegrityError::InvalidSpacerTailLocation);
                        }

                        // Spacer tails must follow a wide char
                        let prev = cells[x - 1];
                        if prev.wide() != Wide::Wide {
                            // log.warn("...spacer tail not following wide", ...);
                            return Err(IntegrityError::InvalidSpacerTailLocation);
                        }
                    }

                    Wide::SpacerHead => {
                        // Spacer heads must be at the end
                        if x != (self.size.cols as usize) - 1 {
                            // log.warn("...spacer head not at end", ...);
                            return Err(IntegrityError::InvalidSpacerHeadLocation);
                        }

                        // The row must be wrapped
                        if !row.wrap() {
                            // log.warn("...spacer head not wrapped", ...);
                            return Err(IntegrityError::UnwrappedSpacerHead);
                        }
                    }
                }
            }

            // Check row grapheme data
            if graphemes_seen > graphemes_start {
                // If a cell in a row has grapheme data, the row must
                // be marked as having grapheme data.
                if !row.grapheme() {
                    // log.warn("...grapheme data but row not marked", ...);
                    return Err(IntegrityError::UnmarkedGraphemeRow);
                }
            }
        }

        // Our graphemes seen should exactly match the grapheme count
        if graphemes_seen > self.grapheme_count() {
            // log.warn("...grapheme count mismatch...", ...);
            return Err(IntegrityError::InvalidGraphemeCount);
        }

        // Verify all our styles have the correct ref count.
        {
            for (id, expected) in styles_seen.iter() {
                let ref_count = unsafe { self.styles.ref_count(base, *id) } as usize;
                if ref_count < *expected {
                    // log.warn("...style ref count mismatch...", ...);
                    return Err(IntegrityError::MismatchedStyleRef);
                }
            }
        }

        // Verify all our hyperlinks have the correct ref count.
        {
            for (id, expected) in hyperlinks_seen.iter() {
                let ref_count = unsafe { self.hyperlink_set.ref_count(base, *id) } as usize;
                if ref_count < *expected {
                    // log.warn("...hyperlink ref count mismatch...", ...);
                    return Err(IntegrityError::MismatchedHyperlinkRef);
                }
            }
        }

        // Verify there are no zombie styles, that is, styles in the
        // set with ref counts > 0, which are not present in the page.
        {
            let styles_table = unsafe {
                core::slice::from_raw_parts(
                    self.styles.table.ptr(base),
                    self.styles.layout.table_cap,
                )
            };
            let styles_items = unsafe {
                core::slice::from_raw_parts(self.styles.items.ptr(base), self.styles.layout.cap)
            };

            let mut zombies: usize = 0;

            for &id in styles_table {
                if id == 0 {
                    continue;
                }
                let item = &styles_items[id as usize];
                if item.meta.ref_count == 0 {
                    continue;
                }

                let expected = styles_seen.get(&id).copied().unwrap_or(0);
                if expected > 0 {
                    continue;
                }

                if (item.meta.ref_count as usize) > expected {
                    zombies += 1;
                }
            }

            // NOTE: This is currently disabled because @qwerasd says that
            // certain fast paths can cause this but its okay.
            // Just 1 zombie style might be the cursor style, so ignore it.
            // if (zombies > 1) {
            //     log.warn(
            //         "page integrity violation zombie styles count={}",
            //         .{zombies},
            //     );
            //     return IntegrityError.ZombieStyles;
            // }
            let _ = zombies;
        }

        Ok(())
    }

    /// Clone the contents of this page. This will allocate new memory
    /// using the page allocator. If you want to manage memory manually,
    /// use cloneBuf.
    // PORT NOTE: the reference method is named `clone`; we keep the name (it is
    // not the std `Clone::clone` — it allocates fresh page memory and may fail).
    #[allow(clippy::should_implement_trait)]
    pub fn clone(&self) -> Result<Page, OutOfMemory> {
        let backing = page_alloc::alloc(self.memory_len).ok_or(OutOfMemory)?;
        // errdefer PageAlloc.free(backing);
        let buf_slice =
            unsafe { core::slice::from_raw_parts_mut(backing.ptr, backing.len) };
        let mut page = self.clone_buf(buf_slice);
        // Use the original mmap pointer (full-region provenance) for the owned
        // backing so `deinit`'s munmap operates on the exact allocation. The
        // pointer stored by `clone_buf` is derived from the passed slice; for
        // an owned clone we replace it with the allocation base.
        page.memory_ptr = backing.ptr;
        page.owned = true;
        Ok(page)
    }

    /// Clone the entire contents of this page.
    ///
    /// The buffer must be at least the size of self.memory.
    pub fn clone_buf(&self, buf: &mut [u8]) -> Page {
        assert!(buf.len() >= self.memory_len);

        // The entire concept behind a page is that everything is stored
        // as offsets so we can do a simple linear copy of the backing
        // memory and copy all the offsets and everything will work.
        // var result = self.*;
        // result.memory = buf[0..self.memory.len];
        let dst_ptr = buf.as_mut_ptr();

        // This is a memcpy. We may want to investigate if there are
        // faster ways to do this (i.e. copy-on-write tricks) but I suspect
        // they'll be slower. I haven't experimented though.
        // std.log.warn("copy bytes={}", .{self.memory.len});
        // fastmem.copy(u8, result.memory, self.memory);
        unsafe {
            core::ptr::copy_nonoverlapping(self.memory_ptr, dst_ptr, self.memory_len);
        }

        Page {
            memory_ptr: dst_ptr,
            memory_len: self.memory_len,
            owned: false,
            rows: self.rows,
            cells: self.cells,
            dirty: self.dirty,
            string_alloc: self.string_alloc,
            grapheme_alloc: self.grapheme_alloc,
            grapheme_map: self.grapheme_map,
            // PORT NOTE: `var result = self.*` copies the whole struct, including
            // the ref-counted sets. Those sets hold `Offset`s + a context; the
            // shallow `Clone` is exactly what `self.*` does in Zig (offsets are
            // relative to the new backing memory after the bytewise memcpy).
            styles: self.styles.clone(),
            hyperlink_map: self.hyperlink_map,
            hyperlink_set: self.hyperlink_set.clone(),
            size: self.size,
            capacity: self.capacity,
            pause_integrity_checks: self.pause_integrity_checks,
        }
    }

    /// Compute the exact capacity required to store a range of rows from
    /// this page.
    ///
    /// The returned capacity will have the same number of columns as this
    /// page and the number of rows equal to the range given. The returned
    /// capacity is by definition strictly less than or equal to this
    /// page's capacity, so the layout is guaranteed to succeed.
    ///
    /// Preconditions:
    /// - Range must be at least 1 row
    /// - Start and end must be valid for this page
    pub fn exact_row_capacity(&self, y_start: usize, y_end: usize) -> Capacity {
        assert!(y_start < y_end);
        assert!(y_end <= self.size.rows as usize);

        let base = self.memory_ptr();

        // Track unique IDs using a bitset. Both style IDs and hyperlink IDs
        // are CellCountInt (u16), so we reuse this set for both to save
        // stack memory (~8KB instead of ~16KB).
        // PORT NOTE: `std.StaticBitSet(maxInt(CellCountInt)+1)`. We use a boxed
        // bit vector of the same size (65536 bits). The comptime asserts that
        // StyleCountInt == CellCountInt and HyperlinkCountInt == CellCountInt
        // are reproduced in the layout tests.
        let mut id_set = StaticBitSet::new();
        let mut grapheme_bytes: usize = 0;
        let mut string_bytes: usize = 0;

        // First pass: count styles and grapheme bytes
        let rows = unsafe {
            core::slice::from_raw_parts(self.rows.ptr(base).add(y_start), y_end - y_start)
        };
        for row in rows {
            let cells = unsafe {
                core::slice::from_raw_parts(row.cells().ptr(base), self.size.cols as usize)
            };
            for cell in cells {
                if cell.style_id() != stylepkg::DEFAULT_ID {
                    id_set.set(cell.style_id() as usize);
                }

                if cell.has_grapheme() {
                    if let Some(cps) = self.lookup_grapheme(cell) {
                        grapheme_bytes += GraphemeAlloc::bytes_required::<u32>(cps.len());
                    }
                }
            }
        }
        let styles_cap = StyleSet::capacity_for_count(id_set.count());

        // Second pass: count hyperlinks and string bytes
        // We count both unique hyperlinks (for hyperlink_set) and total
        // hyperlink cells (for hyperlink_map capacity).
        id_set = StaticBitSet::new();
        let mut hyperlink_cells: usize = 0;
        for row in rows {
            let cells = unsafe {
                core::slice::from_raw_parts(row.cells().ptr(base), self.size.cols as usize)
            };
            for cell in cells {
                if cell.hyperlink() {
                    hyperlink_cells += 1;
                    if let Some(id) = self.lookup_hyperlink(cell) {
                        // Only count each unique hyperlink once for set sizing
                        if !id_set.is_set(id as usize) {
                            id_set.set(id as usize);

                            // Get the hyperlink entry to compute string bytes
                            let entry = unsafe { &*self.hyperlink_set.get(base, id) };
                            string_bytes +=
                                StringAlloc::bytes_required::<u8>(entry.uri.len);

                            match entry.id {
                                hyperlink::PageEntryId::Implicit(_) => {}
                                hyperlink::PageEntryId::Explicit(slice) => {
                                    string_bytes +=
                                        StringAlloc::bytes_required::<u8>(slice.len);
                                }
                            }
                        }
                    }
                }
            }
        }

        // The hyperlink_map capacity in layout() is computed as:
        //   hyperlink_count * hyperlink_cell_multiplier (rounded to power of 2)
        // We need enough hyperlink_bytes so that when layout() computes
        // the map capacity, it can accommodate all hyperlink cells. This
        // is unit tested.
        let hyperlink_cap = {
            let hyperlink_count = id_set.count();
            let hyperlink_set_cap = hyperlink::Set::capacity_for_count(hyperlink_count);
            let hyperlink_map_min = if hyperlink_cells == 0 {
                0
            } else {
                hyperlink_cells.div_ceil(HYPERLINK_CELL_MULTIPLIER)
            };
            core::cmp::max(hyperlink_set_cap, hyperlink_map_min)
        };

        // All the intCasts below are safe because we should have a
        // capacity strictly less than or equal to this page's capacity.
        Capacity {
            cols: self.size.cols,
            rows: (y_end - y_start) as size::CellCountInt,
            styles: styles_cap as size::StyleCountInt,
            grapheme_bytes: grapheme_bytes as size::GraphemeBytesInt,
            hyperlink_bytes: (hyperlink_cap * hyperlink_set_item_size())
                as size::HyperlinkCountInt,
            string_bytes: string_bytes as size::StringBytesInt,
        }
    }

    /// Clone the contents of another page into this page. The capacities
    /// can be different, but the size of the other page must fit into
    /// this page.
    ///
    /// The y_start and y_end parameters allow you to clone only a portion
    /// of the other page. This is useful for splitting a page into two
    /// or more pages.
    ///
    /// The column count of this page will always be the same as this page.
    /// If the other page has more columns, the extra columns will be
    /// truncated. If the other page has fewer columns, the extra columns
    /// will be zeroed.
    // PORT NOTE: the reference is `cloneFrom(self: *Page, other: *const Page,
    // ...)`. `other` is a raw page pointer (it may alias `self` for same-page
    // clones, which is why it can't be a `&Page`); the deref is sound for the
    // documented preconditions.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn clone_from(
        &mut self,
        other: *const Page,
        y_start: usize,
        y_end: usize,
    ) -> Result<(), CloneFromError> {
        // PORT NOTE: read `other`'s fields via raw projection (it may alias
        // `self` for a same-page clone; see `clone_partial_row_from_inner`).
        let other_size = unsafe { Page::size_of(other) };
        assert!(y_start <= y_end);
        assert!(y_end <= other_size.rows as usize);
        assert!(y_end - y_start <= self.size.rows as usize);

        let other_base = unsafe { Page::memory_ptr_of(other) };
        let self_base = self.memory_ptr();

        // Collect source/dest row pointers up front. The reference iterates
        // `other.rows[y_start..y_end]` and `self.rows[0..y_end-y_start]`.
        let other_rows_ptr = unsafe { Page::rows_of(other).ptr(other_base).add(y_start) };
        let self_rows_ptr = unsafe { self.rows.ptr(self_base) };
        let n = y_end - y_start;
        for i in 0..n {
            let dst_row: *mut Row = unsafe { self_rows_ptr.add(i) };
            let src_row: *const Row = unsafe { other_rows_ptr.add(i) };
            self.clone_row_from(other, dst_row, src_row)?;
        }

        // We should remain consistent
        self.assert_integrity();
        Ok(())
    }

    /// Clone a single row from another page into this page.
    pub fn clone_row_from(
        &mut self,
        other: *const Page,
        dst_row: *mut Row,
        src_row: *const Row,
    ) -> Result<(), CloneFromError> {
        let cols = self.size.cols as usize;
        self.clone_partial_row_from(other, dst_row, src_row, 0, cols)
    }

    /// Clone a single row from another page into this page, supporting
    /// partial copy. cloneRowFrom calls this.
    pub fn clone_partial_row_from(
        &mut self,
        other: *const Page,
        dst_row: *mut Row,
        src_row: *const Row,
        x_start: usize,
        x_end_req: usize,
    ) -> Result<(), CloneFromError> {
        // This whole operation breaks integrity until the end.
        self.pause_integrity_checks(true);
        // defer { self.pauseIntegrityChecks(false); self.assertIntegrity(); }
        let result = self.clone_partial_row_from_inner(
            other, dst_row, src_row, x_start, x_end_req,
        );
        self.pause_integrity_checks(false);
        self.assert_integrity();
        result
    }

    fn clone_partial_row_from_inner(
        &mut self,
        other: *const Page,
        dst_row: *mut Row,
        src_row: *const Row,
        x_start: usize,
        x_end_req: usize,
    ) -> Result<(), CloneFromError> {
        // PORT NOTE: do NOT form `&*other` over the whole `Page` — `other` may
        // alias `self` (same-page clone) while `self` is `&mut self`, which would
        // be a Stacked-Borrows violation. When `other` aliases `self` we read
        // `other`'s data through the `&mut self` receiver (a valid reborrow); for
        // the slow (different-page) path `other` is a distinct allocation read via
        // field projection. We never derive a stale-tag pointer to `self`.
        let same_page = std::ptr::eq(other, self);
        let self_base = self.memory_ptr();
        let other_base = if same_page {
            self_base
        } else {
            unsafe { Page::memory_ptr_of(other) }
        };
        let other_size = if same_page {
            self.size
        } else {
            unsafe { Page::size_of(other) }
        };

        let cell_len = core::cmp::min(self.size.cols as usize, other_size.cols as usize);
        let x_end = core::cmp::min(x_end_req, cell_len);
        assert!(x_start <= x_end);
        let other_cells_ptr = unsafe { (*src_row).cells().ptr(other_base).add(x_start) };
        let dst_cells_off_ptr = unsafe { (*dst_row).cells().ptr(self_base).add(x_start) };
        let span = x_end - x_start;

        // If our destination has styles or graphemes then we need to
        // clear some state. This will free up the managed memory as well.
        if unsafe { (*dst_row).managed_memory() } {
            self.clear_cells(dst_row, x_start, x_end);
        }

        // Copy all the row metadata but keep our cells offset
        {
            let mut copy = unsafe { *src_row };

            // If we're not copying the full row then we want to preserve
            // some original state from our dst row.
            if (x_end - x_start) < self.size.cols as usize {
                let d = unsafe { *dst_row };
                copy.set_wrap(d.wrap());
                copy.set_wrap_continuation(d.wrap_continuation());
                copy.set_grapheme(d.grapheme());
                copy.set_hyperlink(d.hyperlink());
                copy.set_styled(d.styled());
                copy.set_dirty(copy.dirty() | d.dirty());
            }

            // Our cell offset remains the same
            copy.set_cells(unsafe { (*dst_row).cells() });

            unsafe { *dst_row = copy };
        }

        // If we have no managed memory in the source, then we can just
        // copy it directly.
        if !unsafe { (*src_row).managed_memory() } {
            // This is an integrity check: if the row claims it doesn't
            // have managed memory then all cells must also not have
            // managed memory.
            if cfg!(debug_assertions) {
                for i in 0..span {
                    let cell = unsafe { *other_cells_ptr.add(i) };
                    assert!(!cell.has_grapheme());
                    assert!(!cell.hyperlink());
                    assert!(cell.style_id() == stylepkg::DEFAULT_ID);
                }
            }

            let dst_cells =
                unsafe { core::slice::from_raw_parts_mut(dst_cells_off_ptr, span) };
            let other_cells = unsafe { core::slice::from_raw_parts(other_cells_ptr, span) };
            fastmem::copy(dst_cells, other_cells);
        } else {
            // We have managed memory, so we have to do a slower copy to
            // get all of that right.
            for i in 0..span {
                let dst_cell: *mut Cell = unsafe { dst_cells_off_ptr.add(i) };
                let src_cell: *const Cell = unsafe { other_cells_ptr.add(i) };

                // Reset any managed memory markers on the cell so that we don't
                // hit an integrity check if we have to return an error because
                // the page can't fit the new memory.
                // PERF(port): `copy_with_managed_reset` is the single masked
                // store form of `*dst = *src` + the three sequential setter
                // resets (see its PORT NOTE in page.rs).
                unsafe { *dst_cell = (*src_cell).copy_with_managed_reset() };

                // PORT NOTE (reshape): the reference reads `src_cell`
                // field-by-field AFTER the dst resets. If `dst_cell` aliases
                // `src_cell`, the resets have already cleared the hyperlink/
                // style_id/grapheme-tag markers, so every branch test below is
                // false under BOTH read schedules and no branch body (the only
                // writers between the tests) executes — and when they don't
                // alias, nothing below writes `*src_cell`. Hence this
                // post-store snapshot is observably identical to the
                // reference's per-branch reloads, while letting LLVM drop the
                // reloads it must otherwise assume clobbered by the dst
                // stores. (The raw `src_cell` pointer is kept for the
                // lookup_grapheme/lookup_hyperlink calls, which key off the
                // cell's address.)
                let src = unsafe { *src_cell };

                if src.has_grapheme() {
                    self.clone_cell_graphemes_from(
                        other, same_page, dst_row, dst_cell, src_cell,
                    )?;
                }
                if src.hyperlink()
                    && self.clone_cell_hyperlink_from(
                        other, same_page, dst_row, dst_cell, src_cell,
                    )?
                {
                    continue;
                }
                if src.style_id() != stylepkg::DEFAULT_ID {
                    unsafe { (*dst_row).set_styled(true) };

                    if same_page {
                        // If it's the same page we don't have to worry about
                        // copying the style, we can use the style ID directly.
                        unsafe {
                            (*dst_cell).set_style_id(src.style_id());
                            self.styles.use_id(self_base, (*dst_cell).style_id());
                        }
                    } else {
                        // Slow path: Get the style from the other
                        // page and add it to this page's style set.
                        let src_style_id = src.style_id();
                        let other_style =
                            unsafe { Page::style_get_of(other, src_style_id) };
                        let added = unsafe {
                            self.styles.add_with_id(self_base, other_style, src_style_id)
                        }
                        .map_err(|e| match e {
                            // The style set capacity needs to be increased.
                            crate::ref_counted_set::AddError::OutOfMemory => {
                                CloneFromError::StyleSetOutOfMemory
                            }
                            // The style set needs to be rehashed.
                            crate::ref_counted_set::AddError::NeedsRehash => {
                                CloneFromError::StyleSetNeedsRehash
                            }
                        })?;
                        unsafe { (*dst_cell).set_style_id(added.unwrap_or(src_style_id)) };
                    }
                }
                if KITTY_GRAPHICS && src.codepoint() == KITTY_PLACEHOLDER {
                    unsafe { (*dst_row).set_kitty_virtual_placeholder(true) };
                }
            }
        }

        // If we are growing columns, then we need to ensure spacer heads
        // are cleared.
        if self.size.cols > other_size.cols {
            let last: *mut Cell = unsafe {
                (*dst_row)
                    .cells()
                    .ptr(self_base)
                    .add(other_size.cols as usize - 1)
            };
            if unsafe { (*last).wide() } == Wide::SpacerHead {
                unsafe { (*last).set_wide(Wide::Narrow) };
            }
        }

        Ok(())
    }

    // PERF(port): the grapheme/hyperlink arms of the managed-copy loop in
    // `clone_partial_row_from_inner` are outlined into these `#[cold]`
    // per-cell helpers (the reference has them inline in the `for` body,
    // page.zig:911-984). Behavior is byte-identical — same calls in the same
    // order with the same error mapping — but keeping the (rare) arms out of
    // line relieves register pressure in the per-cell loop: inline, their
    // fully-inlined probe loops and `Vec` alloc/dealloc paths force LLVM to
    // spill the loop bound, cell-base pointers, and reset-mask constants to
    // the stack and rematerialize them every iteration (measured 2026-07-16,
    // xctrace: ~40% of the loop's self-time vs the Zig build's hoisted
    // registers; PERF_LOG.md round 3).
    /// The grapheme arm of `clone_partial_row_from_inner`'s managed-copy
    /// loop (one cell).
    #[cold]
    #[inline(never)]
    fn clone_cell_graphemes_from(
        &mut self,
        other: *const Page,
        same_page: bool,
        dst_row: *mut Row,
        dst_cell: *mut Cell,
        src_cell: *const Cell,
    ) -> Result<(), CloneFromError> {
        // Copy the grapheme codepoints
        // When `other` aliases `self`, read through the receiver to
        // avoid a stale-tag access to `self` (see method PORT NOTE).
        // We snapshot into an owned buffer to release the source
        // borrow before the `&mut self` `set_graphemes` call (which
        // copies the codepoints anyway).
        let cps: Vec<u32> = if same_page {
            self.lookup_grapheme(src_cell).unwrap().to_vec()
        } else {
            unsafe { Page::lookup_grapheme_of(other, src_cell) }
                .unwrap()
                .to_vec()
        };

        // Safe to use setGraphemes because we cleared all
        // managed memory for our destination cell range.
        self.set_graphemes(dst_row, dst_cell, &cps).map_err(
            |e| match e {
                GraphemeError::GraphemeMapOutOfMemory => {
                    CloneFromError::GraphemeMapOutOfMemory
                }
                GraphemeError::GraphemeAllocOutOfMemory => {
                    CloneFromError::GraphemeAllocOutOfMemory
                }
            },
        )
    }

    /// The hyperlink arm of `clone_partial_row_from_inner`'s managed-copy
    /// loop (one cell). Returns `true` when the same-page fast path was
    /// taken (the caller `continue`s, preserving the existing control flow).
    #[cold]
    #[inline(never)]
    fn clone_cell_hyperlink_from(
        &mut self,
        other: *const Page,
        same_page: bool,
        dst_row: *mut Row,
        dst_cell: *mut Cell,
        src_cell: *const Cell,
    ) -> Result<bool, CloneFromError> {
        let self_base = self.memory_ptr();
        {
                    let id = if same_page {
                        self.lookup_hyperlink(src_cell).unwrap()
                    } else {
                        unsafe { Page::lookup_hyperlink_of(other, src_cell) }.unwrap()
                    };

                    // Fast-path: same page we can add with the same id.
                    if same_page {
                        unsafe { self.hyperlink_set.use_id(self_base, id) };
                        self.set_hyperlink(dst_row, dst_cell, id).map_err(|_| {
                            CloneFromError::HyperlinkMapOutOfMemory
                        })?;
                        return Ok(true);
                    }

                    // Slow-path: get the hyperlink from the other page,
                    // add it, and migrate.

                    // If our page can't support an additional cell with
                    // a hyperlink then we have to return an error.
                    if self.hyperlink_count() >= self.hyperlink_capacity() {
                        // The hyperlink map capacity needs to be increased.
                        return Err(CloneFromError::HyperlinkMapOutOfMemory);
                    }

                    let other_link = unsafe { Page::hyperlink_get_of(other, id) };
                    let dst_id = {
                        // First check if the link already exists in our page,
                        // and increment its refcount if so, since we're about
                        // to use it.
                        let self_ptr: *mut Page = self;
        self.hyperlink_set.context.set_page(self_ptr);
                        self.hyperlink_set.context.set_src_page(Some(other));
                        let found = unsafe { self.hyperlink_set.lookup(self_base, other_link) };
                        self.hyperlink_set.context.set_src_page(None);
                        if let Some(i) = found {
                            unsafe { self.hyperlink_set.use_id(self_base, i) };
                            i
                        } else {
                            // If we don't have this link in our page yet then
                            // we need to clone it over and add it to our set.

                            // Clone the link.
                            let dst_link = unsafe {
                                other_link.dupe(other, self as *mut Page)
                            }
                            .map_err(|_| {
                                // The string alloc capacity needs to be increased.
                                CloneFromError::StringAllocOutOfMemory
                            })?;

                            // Add it, preferring to use the same ID as the other
                            // page, since this *probably* speeds up full-page
                            // clones.
                            //
                            // TODO(qwerasd): verify the assumption that `addWithId`
                            // is ever actually useful, I think it may not be.
                            let self_ptr: *mut Page = self;
        self.hyperlink_set.context.set_page(self_ptr);
                            let added = unsafe {
                                self.hyperlink_set.add_with_id(self_base, dst_link, id)
                            }
                            .map_err(|e| match e {
                                // The hyperlink set capacity needs to be increased.
                                crate::ref_counted_set::AddError::OutOfMemory => {
                                    CloneFromError::HyperlinkSetOutOfMemory
                                }
                                // The hyperlink set needs to be rehashed.
                                crate::ref_counted_set::AddError::NeedsRehash => {
                                    CloneFromError::HyperlinkSetNeedsRehash
                                }
                            })?;
                            added.unwrap_or(id)
                        }
                    };

                    self.set_hyperlink(dst_row, dst_cell, dst_id)
                        .map_err(|_| CloneFromError::HyperlinkMapOutOfMemory)?;
        }
        Ok(false)
    }

    /// Get a single row. y must be valid.
    // PORT NOTE: the reference is `getRow(self: *const Page) *Row` — a `*const`
    // receiver returning a mutable pointer into the page's offset-addressed
    // backing memory (interior mutability via offsets). We mirror that exact
    // signature with PURE pointer arithmetic (no `&mut` is ever materialized),
    // matching the sibling `get_row_and_cell`/`get_cells`: a Zig `*Row` carries
    // no aliasing protector, and materializing `&mut Row` here would mint a
    // Unique-derived tag that a later sibling `get_row` (or any other reborrow
    // of the same row bytes) invalidates — a Stacked-Borrows hazard proven in
    // the isolated Miri harness (.port/phase-2/mirihar-chunk3b, `getrow_naive`
    // UB vs `getrow_shipped` CLEAN; VALIDATION_REPORT §3c.1).
    pub fn get_row(&self, y: usize) -> *mut Row {
        assert!(y < self.size.rows as usize);
        unsafe { self.rows.ptr(self.memory_ptr()).add(y) }
    }

    /// Get the cells for a row.
    // PORT NOTE: see `get_row` — reference `getCells(self: *const Page) []Cell`.
    #[allow(clippy::mut_from_ref)]
    pub fn get_cells(&self, row: *const Row) -> *mut [Cell] {
        let base = self.memory_ptr();
        if cfg!(debug_assertions) {
            let rows = unsafe { self.rows.ptr(base) };
            let cells = unsafe { self.cells.ptr(base) };
            assert!((row as usize) >= (rows as usize));
            assert!((row as usize) < (cells as usize));
        }

        let cells = unsafe { (*row).cells().ptr(base) };
        core::ptr::slice_from_raw_parts_mut(cells, self.size.cols as usize)
    }

    /// Get the row and cell for the given X/Y within this page.
    pub fn get_row_and_cell(&self, x: usize, y: usize) -> RowAndCell {
        assert!(y < self.size.rows as usize);
        assert!(x < self.size.cols as usize);

        let base = self.memory_ptr();
        let row: *mut Row = unsafe { self.rows.ptr(base).add(y) };
        let cell: *mut Cell = unsafe { (*row).cells().ptr(base).add(x) };

        RowAndCell { row, cell }
    }

    /// Move a cell from one location to another. This will replace the
    /// previous contents with a blank cell. Because this is a move, this
    /// doesn't allocate and can't fail.
    pub fn move_cells(
        &mut self,
        src_row: *mut Row,
        src_left: usize,
        dst_row: *mut Row,
        dst_left: usize,
        len: usize,
    ) {
        // defer self.assertIntegrity();
        self.move_cells_inner(src_row, src_left, dst_row, dst_left, len);
        self.assert_integrity();
    }

    fn move_cells_inner(
        &mut self,
        src_row: *mut Row,
        src_left: usize,
        dst_row: *mut Row,
        dst_left: usize,
        len: usize,
    ) {
        let base = self.memory_ptr();
        let src_cells_ptr = unsafe { (*src_row).cells().ptr(base).add(src_left) };
        let dst_cells_ptr = unsafe { (*dst_row).cells().ptr(base).add(dst_left) };

        // Clear our destination now matter what
        self.clear_cells(dst_row, dst_left, dst_left + len);

        // If src has no managed memory, this is very fast.
        if !unsafe { (*src_row).managed_memory() } {
            let dst_cells = unsafe { core::slice::from_raw_parts_mut(dst_cells_ptr, len) };
            let src_cells = unsafe { core::slice::from_raw_parts(src_cells_ptr, len) };
            fastmem::copy(dst_cells, src_cells);
        } else {
            // Source has graphemes or hyperlinks...
            for i in 0..len {
                let src: *mut Cell = unsafe { src_cells_ptr.add(i) };
                let dst: *mut Cell = unsafe { dst_cells_ptr.add(i) };
                unsafe { *dst = *src };
                if unsafe { (*src).has_grapheme() } {
                    // Required for moveGrapheme assertions
                    unsafe { (*dst).set_content_tag(ContentTag::Codepoint) };
                    self.move_grapheme(src, dst);
                    unsafe {
                        (*src).set_content_tag(ContentTag::Codepoint);
                        (*dst).set_content_tag(ContentTag::CodepointGrapheme);
                        (*dst_row).set_grapheme(true);
                    }
                }
                if unsafe { (*src).hyperlink() } {
                    unsafe { (*dst).set_hyperlink(false) };
                    self.move_hyperlink(src, dst);
                    unsafe {
                        (*dst).set_hyperlink(true);
                        (*dst_row).set_hyperlink(true);
                    }
                }
                if KITTY_GRAPHICS && unsafe { (*src).codepoint() } == KITTY_PLACEHOLDER {
                    unsafe { (*dst_row).set_kitty_virtual_placeholder(true) };
                }
            }
        }

        // The destination row has styles if any of the cells are styled
        if !unsafe { (*dst_row).styled() } {
            let mut any = false;
            for i in 0..len {
                let c = unsafe { *dst_cells_ptr.add(i) };
                if c.style_id() != stylepkg::DEFAULT_ID {
                    any = true;
                    break;
                }
            }
            unsafe { (*dst_row).set_styled(any) };
        }

        // Clear our source row now that the copy is complete. We can NOT
        // use clearCells here because clearCells will garbage collect our
        // styles and graphames but we moved them above.
        //
        // Zero the cells as u64s since empirically this seems
        // to be a bit faster than using @memset(src_cells, .{})
        unsafe {
            core::slice::from_raw_parts_mut(src_cells_ptr as *mut u64, len).fill(0);
        }
        if len == self.size.cols as usize {
            unsafe {
                (*src_row).set_grapheme(false);
                (*src_row).set_hyperlink(false);
                (*src_row).set_styled(false);
                if KITTY_GRAPHICS {
                    (*src_row).set_kitty_virtual_placeholder(false);
                }
            }
        }
    }

    /// Swap two cells within the same row as quickly as possible.
    pub fn swap_cells(&mut self, src: *mut Cell, dst: *mut Cell) {
        // defer self.assertIntegrity();
        self.swap_cells_inner(src, dst);
        self.assert_integrity();
    }

    fn swap_cells_inner(&mut self, src: *mut Cell, dst: *mut Cell) {
        let base = self.memory_ptr();

        // Graphemes are keyed by cell offset so we do have to move them.
        // We do this first so that all our grapheme state is correct.
        if unsafe { (*src).has_grapheme() || (*dst).has_grapheme() } {
            if unsafe { (*src).has_grapheme() && !(*dst).has_grapheme() } {
                self.move_grapheme(src, dst);
            } else if unsafe { !(*src).has_grapheme() && (*dst).has_grapheme() } {
                self.move_grapheme(dst, src);
            } else {
                // Both had graphemes, so we have to manually swap
                let src_offset = unsafe { get_offset::<Cell, _>(base, src) };
                let dst_offset = unsafe { get_offset::<Cell, _>(base, dst) };
                let map = unsafe { self.grapheme_map.map(base) };
                let actx = AutoContext::default();
                let src_entry = map.get_entry_context(src_offset, &actx).unwrap();
                let dst_entry = map.get_entry_context(dst_offset, &actx).unwrap();
                let src_value = unsafe { *src_entry.value_ptr };
                let dst_value = unsafe { *dst_entry.value_ptr };
                unsafe {
                    *src_entry.value_ptr = dst_value;
                    *dst_entry.value_ptr = src_value;
                }
            }
        }

        // Hyperlinks are keyed by cell offset.
        if unsafe { (*src).hyperlink() || (*dst).hyperlink() } {
            if unsafe { (*src).hyperlink() && !(*dst).hyperlink() } {
                self.move_hyperlink(src, dst);
            } else if unsafe { !(*src).hyperlink() && (*dst).hyperlink() } {
                self.move_hyperlink(dst, src);
            } else {
                // Both had hyperlinks, so we have to manually swap
                let src_offset = unsafe { get_offset::<Cell, _>(base, src) };
                let dst_offset = unsafe { get_offset::<Cell, _>(base, dst) };
                let map = unsafe { self.hyperlink_map.map(base) };
                let actx = AutoContext::default();
                let src_entry = map.get_entry_context(src_offset, &actx).unwrap();
                let dst_entry = map.get_entry_context(dst_offset, &actx).unwrap();
                let src_value = unsafe { *src_entry.value_ptr };
                let dst_value = unsafe { *dst_entry.value_ptr };
                unsafe {
                    *src_entry.value_ptr = dst_value;
                    *dst_entry.value_ptr = src_value;
                }
            }
        }

        // Copy the metadata. Note that we do NOT have to worry about
        // styles because styles are keyed by ID and we're preserving the
        // exact ref count and row state here.
        // PORT NOTE: the reference is the three-move swap `old_dst = dst.*;
        // dst.* = src.*; src.* = old_dst;`. We keep it (rather than
        // `mem::swap`, which would need two simultaneous `&mut` from the raw
        // pointers) so no overlapping unique borrows are formed.
        #[allow(clippy::manual_swap)]
        unsafe {
            let old_dst = *dst;
            *dst = *src;
            *src = old_dst;
        }
    }

    /// Clear the cells in the given row. This will reclaim memory used
    /// by graphemes and styles. Note that if the style cleared is still
    /// active, Page cannot know this and it will still be ref counted down.
    /// The best solution for this is to artificially increment the ref count
    /// prior to calling this function.
    pub fn clear_cells(&mut self, row: *mut Row, left: usize, end: usize) {
        // defer self.assertIntegrity();
        self.clear_cells_inner(row, left, end);
        self.assert_integrity();
    }

    fn clear_cells_inner(&mut self, row: *mut Row, left: usize, end: usize) {
        let base = self.memory_ptr();
        let cells_ptr = unsafe { (*row).cells().ptr(base).add(left) };
        let span = end - left;

        // If we have managed memory (styles, graphemes, or hyperlinks)
        // in this row then we go cell by cell and clear them if present.
        if unsafe { (*row).grapheme() } {
            for i in 0..span {
                let cell: *mut Cell = unsafe { cells_ptr.add(i) };
                if unsafe { (*cell).has_grapheme() } {
                    self.clear_grapheme(cell);
                }
            }

            // If we have no left/right scroll region we can be sure
            // that we've cleared all the graphemes, so we clear the
            // flag, otherwise we use the update function to update.
            if span == self.size.cols as usize {
                unsafe { (*row).set_grapheme(false) };
            } else {
                self.update_row_grapheme_flag(row);
            }
        }

        if unsafe { (*row).hyperlink() } {
            for i in 0..span {
                let cell: *mut Cell = unsafe { cells_ptr.add(i) };
                if unsafe { (*cell).hyperlink() } {
                    self.clear_hyperlink(cell);
                }
            }

            // If we have no left/right scroll region we can be sure
            // that we've cleared all the hyperlinks, so we clear the
            // flag, otherwise we use the update function to update.
            if span == self.size.cols as usize {
                unsafe { (*row).set_hyperlink(false) };
            } else {
                self.update_row_hyperlink_flag(row);
            }
        }

        if unsafe { (*row).styled() } {
            for i in 0..span {
                let cell: *const Cell = unsafe { cells_ptr.add(i) };
                if unsafe { (*cell).has_styling() } {
                    unsafe { self.styles.release(base, (*cell).style_id()) };
                }
            }

            // If we have no left/right scroll region we can be sure
            // that we've cleared all the styles, so we clear the
            // flag, otherwise we use the update function to update.
            if span == self.size.cols as usize {
                unsafe { (*row).set_styled(false) };
            } else {
                self.update_row_styled_flag(row);
            }
        }

        if KITTY_GRAPHICS
            && unsafe { (*row).kitty_virtual_placeholder() }
            && span == self.size.cols as usize
        {
            let mut found = false;
            for i in 0..span {
                let c = unsafe { *cells_ptr.add(i) };
                if c.codepoint() == KITTY_PLACEHOLDER {
                    found = true;
                    break;
                }
            }
            if !found {
                unsafe { (*row).set_kitty_virtual_placeholder(false) };
            }
        }

        // Zero the cells as u64s since empirically this seems
        // to be a bit faster than using @memset(cells, .{})
        unsafe {
            core::slice::from_raw_parts_mut(cells_ptr as *mut u64, span).fill(0);
        }
    }

    /// Returns the hyperlink ID for the given cell.
    pub fn lookup_hyperlink(&self, cell: *const Cell) -> Option<hyperlink::Id> {
        let base = self.memory_ptr();
        let cell_offset = unsafe { get_offset::<Cell, _>(base, cell) };
        let map = unsafe { self.hyperlink_map.map(base) };
        let actx = AutoContext::default();
        map.get_context(cell_offset, &actx)
    }

    /// Clear the hyperlink from the given cell.
    ///
    /// In order to update the hyperlink flag on the row, call
    /// `updateRowHyperlinkFlag` after you finish clearing any
    /// hyperlinks in the row.
    pub fn clear_hyperlink(&mut self, cell: *mut Cell) {
        // defer self.assertIntegrity();
        self.clear_hyperlink_inner(cell);
        self.assert_integrity();
    }

    fn clear_hyperlink_inner(&mut self, cell: *mut Cell) {
        let base = self.memory_ptr();
        // Get our ID
        let cell_offset = unsafe { get_offset::<Cell, _>(base, cell) };
        let mut map = unsafe { self.hyperlink_map.map(base) };
        let actx = AutoContext::default();
        let entry = match map.get_entry_context(cell_offset, &actx) {
            Some(e) => e,
            None => return,
        };

        // Release our usage of this, free memory, unset flag
        let value = unsafe { *entry.value_ptr };
        let self_ptr: *mut Page = self;
        self.hyperlink_set.context.set_page(self_ptr);
        unsafe { self.hyperlink_set.release(base, value) };
        unsafe { map.remove_by_ptr(entry.key_ptr) };
        unsafe { (*cell).set_hyperlink(false) };
    }

    /// Checks if the row contains any hyperlinks and sets
    /// the hyperlink flag to false if none are found.
    ///
    /// Call after removing hyperlinks in a row.
    pub fn update_row_hyperlink_flag(&self, row: *mut Row) {
        let base = self.memory_ptr();
        let cells = unsafe {
            core::slice::from_raw_parts((*row).cells().ptr(base), self.size.cols as usize)
        };
        for c in cells {
            if c.hyperlink() {
                return;
            }
        }
        unsafe { (*row).set_hyperlink(false) };
    }

    /// Convert a hyperlink into a page entry, returning the ID.
    ///
    /// This does not de-dupe any strings, so if the URI, explicit ID,
    /// etc. is already in the strings table this will duplicate it.
    ///
    /// To release the memory associated with the given hyperlink,
    /// release the ID from the `hyperlink_set`. If the refcount reaches
    /// zero and the slot is needed then the context will reap the
    /// memory.
    pub fn insert_hyperlink(
        &mut self,
        link: hyperlink::Hyperlink,
    ) -> Result<hyperlink::Id, InsertHyperlinkError> {
        let base = self.memory_ptr();

        // Insert our URI into the page strings table.
        let page_uri: OffsetSlice<u8> = {
            let buf = unsafe {
                self.string_alloc
                    .alloc::<u8, _>(base, link.uri.len())
                    .map_err(|_| InsertHyperlinkError::StringsOutOfMemory)?
            };
            // errdefer self.string_alloc.free(self.memory, buf);
            buf.copy_from_slice(&link.uri);

            OffsetSlice {
                offset: unsafe { get_offset::<u8, _>(base, &buf[0]) },
                len: link.uri.len(),
            }
        };
        // errdefer self.string_alloc.free(self.memory, page_uri.slice(self.memory));

        // Allocate an ID for our page memory if we have to.
        let page_id: hyperlink::PageEntryId = match &link.id {
            hyperlink::HyperlinkId::Explicit(id) => {
                let buf = match unsafe { self.string_alloc.alloc::<u8, _>(base, id.len()) } {
                    Ok(b) => b,
                    Err(_) => {
                        // errdefer for page_uri
                        unsafe {
                            let s = page_uri.slice(base);
                            self.string_alloc.free(base, s);
                        }
                        return Err(InsertHyperlinkError::StringsOutOfMemory);
                    }
                };
                buf.copy_from_slice(id);

                hyperlink::PageEntryId::Explicit(OffsetSlice {
                    offset: unsafe { get_offset::<u8, _>(base, &buf[0]) },
                    len: id.len(),
                })
            }

            hyperlink::HyperlinkId::Implicit(id) => hyperlink::PageEntryId::Implicit(*id),
        };
        // errdefer for page_id explicit string handled on error below.

        // Build our entry
        let entry = hyperlink::PageEntry {
            id: page_id,
            uri: page_uri,
        };

        // Put our hyperlink into the hyperlink set to get an ID
        let self_ptr: *mut Page = self;
        self.hyperlink_set.context.set_page(self_ptr);
        self.hyperlink_set.context.set_src_page(None);
        let id = match unsafe { self.hyperlink_set.add(base, entry) } {
            Ok(id) => id,
            Err(e) => {
                // errdefer: free the explicit id string and the uri.
                match page_id {
                    hyperlink::PageEntryId::Implicit(_) => {}
                    hyperlink::PageEntryId::Explicit(slice) => unsafe {
                        let s = slice.slice(base);
                        self.string_alloc.free(base, s);
                    },
                }
                unsafe {
                    let s = page_uri.slice(base);
                    self.string_alloc.free(base, s);
                }
                return Err(match e {
                    crate::ref_counted_set::AddError::OutOfMemory => {
                        InsertHyperlinkError::SetOutOfMemory
                    }
                    crate::ref_counted_set::AddError::NeedsRehash => {
                        InsertHyperlinkError::SetNeedsRehash
                    }
                });
            }
        };
        // errdefer self.hyperlink_set.release(self.memory, id);

        Ok(id)
    }

    /// Set the hyperlink for the given cell. If the cell already has a
    /// hyperlink, then this will handle memory management and refcount
    /// update for the prior hyperlink.
    ///
    /// DOES NOT increment the reference count for the new hyperlink!
    ///
    /// Caller is responsible for updating the refcount in the hyperlink
    /// set as necessary by calling `use` if the id was not acquired with
    /// `add`.
    pub fn set_hyperlink(
        &mut self,
        row: *mut Row,
        cell: *mut Cell,
        id: hyperlink::Id,
    ) -> Result<(), HyperlinkMapOutOfMemory> {
        // defer self.assertIntegrity();
        let result = self.set_hyperlink_inner(row, cell, id);
        self.assert_integrity();
        result
    }

    fn set_hyperlink_inner(
        &mut self,
        row: *mut Row,
        cell: *mut Cell,
        id: hyperlink::Id,
    ) -> Result<(), HyperlinkMapOutOfMemory> {
        let base = self.memory_ptr();
        let cell_offset = unsafe { get_offset::<Cell, _>(base, cell) };
        let mut map = unsafe { self.hyperlink_map.map(base) };
        let actx = AutoContext::default();
        let gop = match map.get_or_put_context(cell_offset, &actx) {
            Ok(gop) => gop,
            Err(_) => {
                // The hyperlink map capacity needs to be increased.
                return Err(HyperlinkMapOutOfMemory);
            }
        };

        if gop.found_existing {
            // Always release the old hyperlink, because even if it's actually
            // the same as the one we're setting, we'd end up double-counting
            // if we left the reference count be, because the caller does not
            // know whether it's the same and will have increased the count
            // outside of this function.
            let old = unsafe { *gop.value_ptr };
            let self_ptr: *mut Page = self;
        self.hyperlink_set.context.set_page(self_ptr);
            unsafe { self.hyperlink_set.release(base, old) };

            // If the hyperlink matches then we don't need to do anything.
            if old == id {
                // It is possible for cell hyperlink to be false but row
                // must never be false. The cell hyperlink can be false because
                // in Terminal.print we clear the hyperlink for the cursor cell
                // before writing the cell again, so if someone prints over
                // a cell with a matching hyperlink this state can happen.
                // This is tested in Terminal.zig.
                assert!(unsafe { (*row).hyperlink() });
                unsafe { (*cell).set_hyperlink(true) };
                return Ok(());
            }
        }

        // Set the hyperlink on the cell and in the map.
        unsafe { *gop.value_ptr = id };
        unsafe { (*cell).set_hyperlink(true) };
        unsafe { (*row).set_hyperlink(true) };
        Ok(())
    }

    /// Move the hyperlink from one cell to another. This can't fail
    /// because we avoid any allocations since we're just moving data.
    /// Destination must NOT have a hyperlink.
    fn move_hyperlink(&mut self, src: *const Cell, dst: *const Cell) {
        assert!(unsafe { (*src).hyperlink() });
        assert!(unsafe { !(*dst).hyperlink() });

        let base = self.memory_ptr();
        let src_offset = unsafe { get_offset::<Cell, _>(base, src) };
        let dst_offset = unsafe { get_offset::<Cell, _>(base, dst) };
        let mut map = unsafe { self.hyperlink_map.map(base) };
        let actx = AutoContext::default();
        let entry = map.get_entry_context(src_offset, &actx).unwrap();
        let value = unsafe { *entry.value_ptr };
        unsafe { map.remove_by_ptr(entry.key_ptr) };
        map.put_assume_capacity_context(dst_offset, value, &actx);

        // NOTE: We must not set src/dst.hyperlink here because this
        // function is used in various cases where we swap cell contents
        // and its unsafe. The flip side: the caller must be careful
        // to set the proper cell state to represent the move.
    }

    /// Returns the number of hyperlinks in the page. This isn't the byte
    /// size but the total number of unique cells that have hyperlink data.
    pub fn hyperlink_count(&self) -> usize {
        unsafe { self.hyperlink_map.map(self.memory_ptr()) }.count() as usize
    }

    /// Returns the hyperlink capacity for the page. This isn't the byte
    /// size but the number of unique cells that can have hyperlink data.
    pub fn hyperlink_capacity(&self) -> usize {
        unsafe { self.hyperlink_map.map(self.memory_ptr()) }.capacity() as usize
    }

    /// Set the graphemes for the given cell. This asserts that the cell
    /// has no graphemes set, and only contains a single codepoint.
    pub fn set_graphemes(
        &mut self,
        row: *mut Row,
        cell: *mut Cell,
        cps: &[u32],
    ) -> Result<(), GraphemeError> {
        // defer self.assertIntegrity();
        let result = self.set_graphemes_inner(row, cell, cps);
        self.assert_integrity();
        result
    }

    fn set_graphemes_inner(
        &mut self,
        row: *mut Row,
        cell: *mut Cell,
        cps: &[u32],
    ) -> Result<(), GraphemeError> {
        assert!(unsafe { (*cell).codepoint() } > 0);
        assert!(unsafe { (*cell).content_tag() } == ContentTag::Codepoint);

        let base = self.memory_ptr();
        let cell_offset = unsafe { get_offset::<Cell, _>(base, cell) };
        let mut map = unsafe { self.grapheme_map.map(base) };
        let actx = AutoContext::default();

        let slice = unsafe {
            self.grapheme_alloc.alloc::<u32, _>(base, cps.len()).map_err(|_| {
                // The grapheme alloc capacity needs to be increased.
                GraphemeError::GraphemeAllocOutOfMemory
            })?
        };
        // errdefer self.grapheme_alloc.free(self.memory, slice);
        slice.copy_from_slice(cps);

        let value = OffsetSlice {
            offset: unsafe { get_offset::<u32, _>(base, &slice[0]) },
            len: slice.len(),
        };
        if map
            .put_no_clobber_context(cell_offset, value, &actx)
            .is_err()
        {
            // errdefer self.grapheme_alloc.free
            unsafe { self.grapheme_alloc.free(base, slice) };
            // The grapheme map capacity needs to be increased.
            return Err(GraphemeError::GraphemeMapOutOfMemory);
        }
        // errdefer map.remove(cell_offset);

        unsafe { (*cell).set_content_tag(ContentTag::CodepointGrapheme) };
        unsafe { (*row).set_grapheme(true) };

        Ok(())
    }

    /// Append a codepoint to the given cell as a grapheme.
    pub fn append_grapheme(
        &mut self,
        row: *mut Row,
        cell: *mut Cell,
        cp: u32,
    ) -> Result<(), OutOfMemory> {
        // defer self.assertIntegrity();
        let result = self.append_grapheme_inner(row, cell, cp);
        self.assert_integrity();
        result
    }

    fn append_grapheme_inner(
        &mut self,
        row: *mut Row,
        cell: *mut Cell,
        cp: u32,
    ) -> Result<(), OutOfMemory> {
        if cfg!(debug_assertions) {
            assert!(unsafe { (*cell).codepoint() } != 0);
        }

        let base = self.memory_ptr();
        let cell_offset = unsafe { get_offset::<Cell, _>(base, cell) };
        let mut map = unsafe { self.grapheme_map.map(base) };
        let actx = AutoContext::default();

        // If this cell has no graphemes, we can go faster by knowing we
        // need to allocate a new grapheme slice and update the map.
        if unsafe { (*cell).content_tag() } != ContentTag::CodepointGrapheme {
            let cps = unsafe { self.grapheme_alloc.alloc::<u32, _>(base, 1).map_err(|_| OutOfMemory)? };
            // errdefer self.grapheme_alloc.free(self.memory, cps);
            cps[0] = cp;

            let value = OffsetSlice {
                offset: unsafe { get_offset::<u32, _>(base, &cps[0]) },
                len: 1,
            };
            if map.put_no_clobber_context(cell_offset, value, &actx).is_err() {
                unsafe { self.grapheme_alloc.free(base, cps) };
                return Err(OutOfMemory);
            }
            // errdefer map.remove(cell_offset);

            unsafe { (*cell).set_content_tag(ContentTag::CodepointGrapheme) };
            unsafe { (*row).set_grapheme(true) };

            return Ok(());
        }

        // The cell already has graphemes. We need to append to the existing
        // grapheme slice and update the map.
        assert!(unsafe { (*row).grapheme() });

        let slice = map.get_ptr_context(cell_offset, &actx).unwrap();
        let slice_ref = unsafe { &mut *slice };

        // If our slice len doesn't divide evenly by the grapheme chunk
        // length then we can utilize the additional chunk space.
        if slice_ref.len % GRAPHEME_CHUNK_LEN != 0 {
            let cps = unsafe { slice_ref.offset.ptr(base) };
            unsafe {
                *cps.add(slice_ref.len) = cp;
            }
            slice_ref.len += 1;
            return Ok(());
        }

        // We are out of chunk space. There is no fast path here. We need
        // to allocate a larger chunk. This is a very slow path. We expect
        // most graphemes to fit within our chunk size.
        let cps = unsafe {
            self.grapheme_alloc
                .alloc::<u32, _>(base, slice_ref.len + 1)
                .map_err(|_| OutOfMemory)?
        };
        // errdefer self.grapheme_alloc.free(self.memory, cps);
        let old_cps = unsafe { slice_ref.slice(base) };
        fastmem::copy(&mut cps[0..old_cps.len()], old_cps);
        cps[slice_ref.len] = cp;
        let new_len = slice_ref.len + 1;
        *slice_ref = OffsetSlice {
            offset: unsafe { get_offset::<u32, _>(base, &cps[0]) },
            len: new_len,
        };

        // Free our old chunk
        unsafe { self.grapheme_alloc.free(base, old_cps) };
        Ok(())
    }

    /// Returns the codepoints for the given cell. These are the codepoints
    /// in addition to the first codepoint. The first codepoint is NOT
    /// included since it is on the cell itself.
    pub fn lookup_grapheme(&self, cell: *const Cell) -> Option<&[u32]> {
        let base = self.memory_ptr();
        let cell_offset = unsafe { get_offset::<Cell, _>(base, cell) };
        let map = unsafe { self.grapheme_map.map(base) };
        let actx = AutoContext::default();
        let slice = map.get_context(cell_offset, &actx)?;
        Some(unsafe { slice.slice(base) })
    }

    /// Move the graphemes from one cell to another. This can't fail
    /// because we avoid any allocations since we're just moving data.
    ///
    /// WARNING: This will NOT change the content_tag on the cells because
    /// there are scenarios where we want to move graphemes without changing
    /// the content tag. Callers beware but assertIntegrity should catch this.
    pub fn move_grapheme(&mut self, src: *const Cell, dst: *const Cell) {
        if cfg!(debug_assertions) {
            assert!(unsafe { (*src).has_grapheme() });
            assert!(unsafe { !(*dst).has_grapheme() });
        }

        let base = self.memory_ptr();
        let src_offset = unsafe { get_offset::<Cell, _>(base, src) };
        let dst_offset = unsafe { get_offset::<Cell, _>(base, dst) };
        let mut map = unsafe { self.grapheme_map.map(base) };
        let actx = AutoContext::default();
        let entry = map.get_entry_context(src_offset, &actx).unwrap();
        let value = unsafe { *entry.value_ptr };
        unsafe { map.remove_by_ptr(entry.key_ptr) };
        map.put_assume_capacity_context(dst_offset, value, &actx);
    }

    /// Clear the graphemes for a given cell.
    ///
    /// In order to update the grapheme flag on the row, call
    /// `updateRowGraphemeFlag` after you finish clearing any
    /// graphemes in the row.
    pub fn clear_grapheme(&mut self, cell: *mut Cell) {
        // defer self.assertIntegrity();
        self.clear_grapheme_inner(cell);
        self.assert_integrity();
    }

    fn clear_grapheme_inner(&mut self, cell: *mut Cell) {
        if cfg!(debug_assertions) {
            assert!(unsafe { (*cell).has_grapheme() });
        }

        let base = self.memory_ptr();
        // Get our entry in the map, which must exist
        let cell_offset = unsafe { get_offset::<Cell, _>(base, cell) };
        let mut map = unsafe { self.grapheme_map.map(base) };
        let actx = AutoContext::default();
        let entry = map.get_entry_context(cell_offset, &actx).unwrap();

        // Free our grapheme data
        let cps = unsafe { (*entry.value_ptr).slice(base) };
        unsafe { self.grapheme_alloc.free(base, cps) };

        // Remove the entry
        unsafe { map.remove_by_ptr(entry.key_ptr) };

        // Mark that we no longer have graphemes by changing the content tag.
        unsafe { (*cell).set_content_tag(ContentTag::Codepoint) };
    }

    /// Checks if the row contains any graphemes and sets
    /// the grapheme flag to false if none are found.
    ///
    /// Call after removing graphemes in a row.
    pub fn update_row_grapheme_flag(&self, row: *mut Row) {
        let base = self.memory_ptr();
        let cells = unsafe {
            core::slice::from_raw_parts((*row).cells().ptr(base), self.size.cols as usize)
        };
        for c in cells {
            if c.has_grapheme() {
                return;
            }
        }
        unsafe { (*row).set_grapheme(false) };
    }

    /// Returns the number of graphemes in the page. This isn't the byte
    /// size but the total number of unique cells that have grapheme data.
    pub fn grapheme_count(&self) -> usize {
        unsafe { self.grapheme_map.map(self.memory_ptr()) }.count() as usize
    }

    /// Returns the grapheme capacity for the page. This isn't the byte
    /// size but the number of unique cells that can have grapheme data.
    pub fn grapheme_capacity(&self) -> usize {
        unsafe { self.grapheme_map.map(self.memory_ptr()) }.capacity() as usize
    }

    /// Checks if the row contains any styles and sets
    /// the styled flag to false if none are found.
    ///
    /// Call after removing styles in a row.
    pub fn update_row_styled_flag(&self, row: *mut Row) {
        let base = self.memory_ptr();
        let cells = unsafe {
            core::slice::from_raw_parts((*row).cells().ptr(base), self.size.cols as usize)
        };
        for c in cells {
            if c.has_styling() {
                return;
            }
        }
        unsafe { (*row).set_styled(false) };
    }

    /// Returns true if this page is dirty at all.
    pub fn is_dirty(&self) -> bool {
        if self.dirty {
            return true;
        }
        let base = self.memory_ptr();
        let rows =
            unsafe { core::slice::from_raw_parts(self.rows.ptr(base), self.size.rows as usize) };
        for row in rows {
            if row.dirty() {
                return true;
            }
        }
        false
    }

    /// The memory layout for a page given a desired minimum cols
    /// and rows size.
    pub fn layout(cap: Capacity) -> Layout {
        let rows_count: usize = cap.rows as usize;
        let rows_start = 0;
        let rows_end: usize = rows_start + (rows_count * core::mem::size_of::<Row>());

        let cells_count: usize = (cap.cols as usize) * (cap.rows as usize);
        let cells_start = align_forward(rows_end, core::mem::align_of::<Cell>());
        let cells_end = cells_start + (cells_count * core::mem::size_of::<Cell>());

        let styles_layout =
            crate::ref_counted_set::Layout::<StyleId>::init::<Style, size::CellCountInt>(
                cap.styles as usize,
            );
        let styles_start = align_forward(cells_end, style_set_base_align());
        let styles_end = styles_start + styles_layout.total_size;

        let grapheme_alloc_layout = GraphemeAlloc::layout(cap.grapheme_bytes as usize);
        let grapheme_alloc_start = align_forward(styles_end, GraphemeAlloc::BASE_ALIGN);
        let grapheme_alloc_end = grapheme_alloc_start + grapheme_alloc_layout.total_size;

        let grapheme_count: usize = {
            if cap.grapheme_bytes == 0 {
                0
            } else {
                // Use divCeil to match GraphemeAlloc.layout() which uses alignForward,
                // ensuring grapheme_map has capacity when grapheme_alloc has chunks.
                let base = (cap.grapheme_bytes as usize).div_ceil(GRAPHEME_CHUNK);
                base.next_power_of_two()
            }
        };
        let grapheme_map_layout = GraphemeMap::layout(grapheme_count as crate::hash_map::Size);
        let grapheme_map_start = align_forward(grapheme_alloc_end, grapheme_map_base_align());
        let grapheme_map_end = grapheme_map_start + grapheme_map_layout.total_size;

        let string_layout = StringAlloc::layout(cap.string_bytes as usize);
        let string_start = align_forward(grapheme_map_end, StringAlloc::BASE_ALIGN);
        let string_end = string_start + string_layout.total_size;

        let hyperlink_count = (cap.hyperlink_bytes as usize) / hyperlink_set_item_size();
        let hyperlink_set_layout =
            crate::ref_counted_set::Layout::<hyperlink::Id>::init::<
                hyperlink::PageEntry,
                size::CellCountInt,
            >(hyperlink_count);
        let hyperlink_set_start = align_forward(string_end, hyperlink_set_base_align());
        let hyperlink_set_end = hyperlink_set_start + hyperlink_set_layout.total_size;

        let hyperlink_map_count: u32 = {
            if hyperlink_count == 0 {
                0
            } else {
                let mult = hyperlink_count * HYPERLINK_CELL_MULTIPLIER;
                match u32::try_from(mult) {
                    Ok(m) => {
                        // std.math.ceilPowerOfTwoAssert(u32, mult)
                        m.next_power_of_two()
                    }
                    Err(_) => u32::MAX,
                }
            }
        };
        let hyperlink_map_layout = hyperlink::Map::layout(hyperlink_map_count);
        let hyperlink_map_start = align_forward(hyperlink_set_end, hyperlink_map_base_align());
        let hyperlink_map_end = hyperlink_map_start + hyperlink_map_layout.total_size;

        let total_size = align_forward(hyperlink_map_end, PAGE_SIZE_MIN);

        Layout {
            total_size,
            rows_start,
            rows_size: rows_end - rows_start,
            cells_start,
            cells_size: cells_end - cells_start,
            styles_start,
            styles_layout,
            grapheme_alloc_start,
            grapheme_alloc_layout,
            grapheme_map_start,
            grapheme_map_layout,
            string_alloc_start: string_start,
            string_alloc_layout: string_layout,
            hyperlink_map_start,
            hyperlink_map_layout,
            hyperlink_set_start,
            hyperlink_set_layout,
            capacity: cap,
        }
    }
}

/// The error returned by `set_hyperlink` (reference `error{HyperlinkMapOutOfMemory}`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HyperlinkMapOutOfMemory;

/// Return type of `get_row_and_cell` (reference anonymous struct `{ row, cell }`).
pub struct RowAndCell {
    pub row: *mut Row,
    pub cell: *mut Cell,
}

// PORT NOTE: `std.StaticBitSet(maxInt(CellCountInt)+1)` = a fixed 65536-bit set.
// Modeled as a boxed array of u64 words. Used only transiently in
// `exact_row_capacity`.
struct StaticBitSet {
    words: Box<[u64; 1024]>, // 65536 bits / 64
    count: usize,
}

impl StaticBitSet {
    fn new() -> StaticBitSet {
        StaticBitSet {
            words: Box::new([0u64; 1024]),
            count: 0,
        }
    }
    fn set(&mut self, i: usize) {
        let w = i / 64;
        let b = i % 64;
        if (self.words[w] & (1u64 << b)) == 0 {
            self.words[w] |= 1u64 << b;
            self.count += 1;
        }
    }
    fn is_set(&self, i: usize) -> bool {
        let w = i / 64;
        let b = i % 64;
        (self.words[w] & (1u64 << b)) != 0
    }
    fn count(&self) -> usize {
        self.count
    }
}
