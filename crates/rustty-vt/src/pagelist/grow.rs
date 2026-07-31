//! PageList growth + capacity + scrollbar cluster.
//!
//! PORTS: ghostty src/terminal/PageList.zig — the `grow`, `increaseCapacity`
//! (+ `IncreaseCapacity` enum), `compact`, `scrollbar` (+ `Scrollbar` struct /
//! `cval` / `eql`), `maxSize`, `viewportRowOffset`, `fixupViewport`, and
//! `growRows` cluster.
//!
//! PORT STATUS: methods ported (vt-3c). `maxSize` and `pinIsValid` live in
//! `mod.rs` (CORE callers need them). The 205-block test split lands in the
//! dedicated test phase.

use super::{Node, PageList, Viewport};
use crate::page::{Capacity, Page};
use rustty_render_types::term_size as size;
use rustty_render_types::term_size::OffsetBuf;

impl PageList {
    /// Split the given node in the PageList at the given pin.
    ///
    /// The row at the pin and after will be moved into a new page with
    /// the same capacity as the original page. Alternatively, you can "split
    /// above" by splitting the row following the desired split row.
    ///
    /// Since the split happens below the pin, the pin remains valid.
    pub fn split(&mut self, p: super::Pin) -> Result<(), SplitError> {
        // PORT NOTE: the reference `if (build_options.slow_runtime_safety)`
        // guard maps to `cfg!(debug_assertions)` per the crate convention (see
        // `pin_is_valid` in `mod.rs`); `pin_is_valid` itself asserts it is only
        // reached in such a build.
        if cfg!(debug_assertions) {
            assert!(self.pin_is_valid(p.pin_copy()));
        }

        // Ran into a bug that I can only explain via aliasing. If a tracked
        // pin is passed in, its possible Zig will alias the memory and then
        // when we modify it later it updates our p here. Copying the node
        // fixes this.
        let original_node = p.node;
        // PORT NOTE (WS7 I2): the accessor's result is used RAW. Writing
        // `&mut (*Node::page(..))` would retag it as `Unique`, and the
        // `Node::page` call in the tracked-pin loop below would then invalidate
        // it under Stacked Borrows — while `page` is still dereferenced
        // afterwards to clear rows and shorten the source.
        let page: *mut Page = unsafe { super::Node::page(original_node) };

        // A page that is already 1 row can't be split. In the future we can
        // theoretically maybe split by soft-wrapping multiple pages but that
        // seems crazy and the rest of our PageList can't handle heterogeneously
        // sized pages today.
        if unsafe { (*page).size.rows } <= 1 {
            return Err(SplitError::OutOfSpace);
        }

        // Splitting at row 0 is a no-op since there's nothing before the split point.
        if p.y == 0 {
            return Ok(());
        }

        // At this point we're doing actual modification so make sure
        // on the return that we're good.
        //
        // PORT NOTE: `defer self.assertIntegrity()` — reproduced by calling
        // `assert_integrity` before every exit past this point.

        // Create a new node with the same capacity of managed memory.
        let target = match self.create_page(super::CreatePage::new(unsafe { (*page).capacity })) {
            Ok(v) => v,
            Err(_) => return Err(SplitError::OutOfMemory),
        };
        // errdefer self.destroyNode(target);

        // Determine how many rows we're copying
        let y_start = p.y;
        let y_end = unsafe { (*page).size.rows };
        // Hoisted: `target` was just created resident, so this is one lookup,
        // not three (WS7 I2).
        let target_page: *mut Page = unsafe { super::Node::page(target) };
        unsafe { (*target_page).size.rows = y_end - y_start };
        assert!(unsafe { super::Node::rows(target) <= super::Node::capacity(target).rows });

        // Copy our old data. This should NOT fail because we have the
        // capacity of the old page which already fits the data we requested.
        if let Err(_err) =
            unsafe { (*target_page).clone_from(page, y_start as usize, y_end as usize) }
        {
            // log.err(
            //     "error cloning rows for split err={}",
            //     .{err},
            // );

            // Rather than crash, we return an OutOfSpace to show that
            // we couldn't split and let our callers gracefully handle it.
            // Realistically though... this should not happen.
            self.destroy_node(target);
            return Err(SplitError::OutOfSpace);
        }

        // From this point forward there is no going back. We have no
        // error handling. It is possible but we haven't written it.
        // errdefer comptime unreachable;

        // Failable split work is complete; shortening the source changes its
        // row range.
        self.invalidate_node_layout(original_node);
        self.page_compression.mark_activity();

        // Move any tracked pins from the copied rows
        for tracked in self.tracked_pins.keys() {
            let tracked: *mut super::Pin = *tracked;
            // Raw comparison — forming a `&Page` here would retag and
            // invalidate `page` (WS7 I2).
            if !core::ptr::eq(
                unsafe { super::Node::page((*tracked).node) } as *const Page,
                page as *const Page,
            ) || unsafe { (*tracked).y } < p.y
            {
                continue;
            }

            unsafe {
                (*tracked).node = target;
                (*tracked).y -= p.y;
                // p.x remains the same since we're copying the row as-is
            }
        }

        // Clear our rows
        {
            let base = unsafe { (*page).memory_ptr() };
            let rows_ptr = unsafe { (*page).rows.ptr(base) };
            let cols = unsafe { (*page).size.cols };
            for i in (y_start as usize)..(y_end as usize) {
                let row = unsafe { rows_ptr.add(i) };
                unsafe {
                    (*page).clear_cells(row, 0, cols as usize);
                }
            }
        }
        unsafe { (*page).size.rows -= y_end - y_start };

        unsafe { self.pages.insert_after(original_node, target) };

        self.assert_integrity();
        Ok(())
    }

    /// Return the scrollbar state for this PageList.
    ///
    /// This is amortized O(1): the total is maintained incrementally and
    /// the viewport offset is cached. The first call after the viewport
    /// moves to an arbitrary pin (e.g. scrolling to a selection) may cost
    /// O(pages) to compute the offset, after which it is cached again.
    /// See `viewport_row_offset` for more details.
    pub fn scrollbar(&mut self) -> Scrollbar {
        // If we have no scrollback, special case no scrollbar.
        // We need to do this because the way PageList works is that
        // it always has SOME extra space (due to the way we allocate by page).
        // So even with no scrollback we have some growth. It is architecturally
        // much simpler to just hide that for no-scrollback cases.
        if self.limits.bytes.explicit == 0 {
            return Scrollbar {
                total: self.rows as usize,
                offset: 0,
                len: self.rows as usize,
            };
        }

        Scrollbar {
            total: self.total_rows,
            offset: self.viewport_row_offset(),
            len: self.rows as usize, // Length is always rows
        }
    }

    /// Returns the offset of the current viewport from the top of the
    /// screen.
    ///
    /// This is potentially expensive to calculate because if the viewport
    /// is a pin and the pin is near the beginning of the scrollback, we
    /// will traverse a lot of linked list nodes.
    ///
    /// The result is cached so repeated calls are cheap.
    fn viewport_row_offset(&mut self) -> usize {
        match self.viewport {
            Viewport::Top => 0,
            Viewport::Active => self.total_rows - self.rows as usize,
            Viewport::Pin => {
                // We assert integrity on this code path because it verifies
                // that the cached value is correct.
                //
                // PORT NOTE: `defer self.assertIntegrity()` — reproduced by
                // calling `assert_integrity` before returning the value below.

                // Return cached value if available
                if let Some(cached) = self.viewport_pin_row_offset {
                    self.assert_integrity();
                    return cached;
                }

                // Traverse from the end and count rows until we reach the
                // viewport pin. We count backwards because most of the time
                // a user is scrolling near the active area.
                let top_offset: usize = {
                    let mut offset: usize = 0;
                    let mut node = self.pages.last;
                    let result = loop {
                        if node.is_null() {
                            break None;
                        }
                        let n = node;
                        offset += unsafe { super::Node::rows(n) as usize };
                        if core::ptr::eq(n, unsafe { (*self.viewport_pin).node }) {
                            assert!(
                                unsafe { super::Node::rows(n) } > unsafe { (*self.viewport_pin).y }
                            );
                            offset -= unsafe { (*self.viewport_pin).y as usize };
                            break Some(self.total_rows - offset);
                        }
                        node = unsafe { (*n).prev };
                    };

                    match result {
                        Some(v) => v,
                        // Invalid pins are not possible.
                        None => unreachable!(),
                    }
                };

                // The offset is from the bottom and our cached value and this
                // function returns from the top, so we need to invert it.
                self.viewport_pin_row_offset = Some(top_offset);
                self.assert_integrity();
                top_offset
            }
        }
    }

    /// This fixes up the viewport data when rows are removed from the
    /// PageList. This will update a viewport to `active` if row removal
    /// puts the viewport into the active area, to `top` if the viewport
    /// is now at row 0, and updates any row offset caches as necessary.
    ///
    /// This is unit tested transitively through other tests such as
    /// eraseRows.
    pub(crate) fn fixup_viewport(&mut self, removed: usize) {
        // Page removal can mark every pin on the removed page as garbage. The
        // viewport pin is an internal navigation anchor that is always remapped,
        // so it remains valid after the removal.
        unsafe { (*self.viewport_pin).garbage = false };

        match self.viewport {
            Viewport::Active => {}

            // For pin, we check if our pin is now in the active area and if so
            // we move our viewport back to the active area.
            Viewport::Pin => {
                if self.pin_is_active(unsafe { (*self.viewport_pin).pin_copy() }) {
                    self.viewport = Viewport::Active;
                } else if let Some(v) = self.viewport_pin_row_offset.as_mut() {
                    // If we have a cached row offset, we need to update it
                    // to account for the erased rows.
                    if *v < removed {
                        self.viewport = Viewport::Top;
                    } else {
                        *v -= removed;
                    }
                }
            }

            // For top, we move back to active if our erasing moved our
            // top page into the active area.
            Viewport::Top => {
                if self.pin_is_active(super::Pin {
                    node: self.pages.first,
                    y: 0,
                    x: 0,
                    garbage: false,
                }) {
                    self.viewport = Viewport::Active;
                }
            }
        }
    }

    /// Grow the active area by exactly one row.
    ///
    /// This may allocate, but also may not if our current page has more
    /// capacity we can use. This will prune scrollback if necessary to
    /// adhere to max_size and max_lines.
    ///
    /// This returns the newly allocated page node if there is one.
    pub fn grow(&mut self) -> Result<Option<*mut Node>, super::OutOfMemory> {
        // PORT NOTE: `defer self.assertIntegrity()` — reproduced by calling
        // `assert_integrity` before every exit point below.

        // Growing can move a complete page behind the active boundary.
        self.page_compression.mark_activity();

        let last = self.pages.last;
        if unsafe { super::Node::capacity(last).rows > super::Node::rows(last) } {
            // Fast path: we have capacity in the last page.
            unsafe { (*super::Node::page(last)).size.rows += 1 };
            unsafe { (*super::Node::page(last)).assert_integrity() };

            // Increase our total rows by one
            self.total_rows += 1;

            // Growing inside the last page moves the active boundary without
            // allocating; that alone can make the first page wholly historical.
            self.enforce_limit(super::Key::Lines);
            self.assert_integrity();
            return Ok(None);
        }

        // Slower path: we have no space, we need to allocate a new page.

        // Get the layout first so our failable work is done early.
        // We'll need this for both paths.
        let cap = Self::initial_capacity(self.cols);

        // If allocation would exceed our max size, we prune the first page.
        // We don't need to reallocate because we can simply reuse that first
        // page.
        //
        // We only take this path if we have more than one page since pruning
        // reuses the popped page. It is possible to have a single page and
        // exceed the max size if that page was adjusted to be larger after
        // initial allocation.
        if !self.pages.first.is_null()
            && !core::ptr::eq(self.pages.first, self.pages.last)
            && self.page_size + super::std_size() > self.limits.max(super::Key::Bytes)
        {
            // prune:
            let first = unsafe { self.pages.pop_first() };
            assert!(!core::ptr::eq(first, last));

            // Decrease our total row count from the pruned page
            self.total_rows -= unsafe { super::Node::rows(first) as usize };

            // If our total row count is now less than our required
            // rows then we can't prune. The "+ 1" is because we'll add one
            // more row below.
            if self.total_rows + 1 < self.rows as usize {
                unsafe { self.pages.prepend(first) };
                assert!(core::ptr::eq(self.pages.first, first));
                self.total_rows += unsafe { super::Node::rows(first) as usize };
                // break :prune;
                return self.grow_alloc(cap);
            }

            // If we have a pin viewport cache then we need to update it.
            if self.viewport == Viewport::Pin {
                // viewport:
                if let Some(v) = self.viewport_pin_row_offset.as_mut() {
                    // If our offset is less than the number of rows in the
                    // pruned page, then we are now at the top.
                    if *v < unsafe { super::Node::rows(first) as usize } {
                        self.viewport = Viewport::Top;
                        // break :viewport;
                    } else {
                        // Otherwise, our viewport pin is below what we pruned
                        // so we just decrement our offset.
                        *v -= unsafe { super::Node::rows(first) as usize };
                    }
                }
            }

            // Update any tracked pins that point to this page to point to the
            // new first page to the top-left, and mark them as garbage.
            for p in self.tracked_pins.keys() {
                let p: *mut super::Pin = *p;
                if !core::ptr::eq(unsafe { (*p).node }, first) {
                    continue;
                }
                unsafe {
                    (*p).node = self.pages.first;
                    (*p).y = 0;
                    (*p).x = 0;
                    (*p).garbage = true;
                }
            }
            unsafe { (*self.viewport_pin).garbage = false };

            match unsafe { (*first).owned } {
                // Pool-owned pages are reused below.
                super::Owned::Pool => {}

                // Heap-owned pages can't be reused because they may be any size
                // (larger or smaller than a standard page), so just destroy
                // them.
                super::Owned::Heap => {
                    self.destroy_node(first);
                    // break :prune;
                    return self.grow_alloc(cap);
                }
            }

            // Reset our memory.
            //
            // PORT NOTE (WS7 B6): the oracle reaches the buffer via
            // `first.restore(.discard)` — the contents are about to be zeroed,
            // so a compressed node must NOT be decoded on the way through.
            let (buf_ptr, buf_len) = Self::page_memory(unsafe {
                super::Node::restore(first, super::RestoreMode::Discard)
            });
            unsafe { core::ptr::write_bytes(buf_ptr, 0, buf_len) };
            assert!(buf_len <= super::std_size());

            // Initialize our new page and reinsert it as the last
            unsafe {
                (*first).data = super::NodeData::Resident(Page::init_buf(
                    OffsetBuf::init(buf_ptr),
                    Page::layout(cap),
                ));
                (*super::Node::page_assume_resident(first)).size.rows = 1;
            }
            unsafe { self.pages.insert_after(last, first) };
            self.total_rows += 1;

            // Reusing the node gives it a fresh generation. Do not begin a new
            // page_serial_epoch here: generations are not monotonic in list order,
            // so older live successors may have lower generations. The epoch only
            // advances when reset invalidates the entire list.
            unsafe { (*first).serial = self.page_serial };
            self.page_serial += 1;

            // In this case we do NOT need to update page_size because
            // we're reusing an existing page so nothing has changed.

            unsafe { (*super::Node::page(first)).assert_integrity() };

            // Byte-limit recycling may leave history above the independent line
            // limit, so enforce it after the recycled page becomes the new tail.
            self.enforce_limit(super::Key::Lines);
            self.assert_integrity();
            return Ok(Some(first));
        }

        self.grow_alloc(cap)
    }

    /// PORT NOTE: the tail of `grow` after the `prune:` block. The reference's
    /// `break :prune` falls through to this allocation path; in Rust we factor
    /// it into a helper so every `break :prune` site can `return` to it. It is
    /// NOT a separate reference function — it is the verbatim tail of `grow`
    /// (including its `defer self.assertIntegrity()` exit).
    fn grow_alloc(&mut self, cap: Capacity) -> Result<Option<*mut Node>, super::OutOfMemory> {
        // We need to allocate a new memory buffer.
        let next_node = self.create_page(super::CreatePage::new(cap))?;
        // we don't errdefer this because we've added it to the linked
        // list and its fine to have dangling unused pages.
        unsafe { self.pages.append(next_node) };
        unsafe { (*super::Node::page(next_node)).size.rows = 1 };

        // We should never be more than our max size here because we've
        // verified the case above.
        unsafe { (*super::Node::page(next_node)).assert_integrity() };

        // Record the increased row count
        self.total_rows += 1;

        // Appending a page can cross the line limit and can make the oldest
        // active-boundary page wholly historical.
        self.enforce_limit(super::Key::Lines);
        self.assert_integrity();
        Ok(Some(next_node))
    }

    /// Increase the capacity of the given page node in the given direction.
    /// This will always allocate a new node and remove the old node, so the
    /// existing node pointer will be invalid after this call. The newly created
    /// node on success is returned.
    ///
    /// The increase amount is at the control of the PageList implementation,
    /// but is guaranteed to always increase by at least one unit in the
    /// given dimension. Practically, we'll always increase by much more
    /// (we currently double every time) but callers shouldn't depend on that.
    /// The only guarantee is some amount of growth.
    ///
    /// Adjustment can be null if you want to recreate, reclone the page
    /// with the same capacity. This is a special case used for rehashing since
    /// the logic is otherwise the same. In this case, OutOfMemory is the
    /// only possible error.
    /// Clone the given source row into the row at `dst_y` of the given
    /// node's page, increasing the node's capacity as necessary to fit the
    /// source row's managed memory (styles, hyperlinks, etc.).
    ///
    /// Since increasing capacity replaces the node in the page list, the
    /// (possibly replaced) node is returned and the caller must use it in
    /// place of the old node. The source must NOT be on the given node
    /// since the node's page memory may be freed on capacity increase.
    //
    // UB HAZARD (`P3-W2` T4 Step 3): `src_page` and `node.page()` are two raw
    // `*mut Page`s and `increase_capacity` FREES the latter. If they were the
    // same allocation, `src_row` would dangle on the retry. The reference states
    // the requirement in prose and enforces it with
    // `assert(src_page != node.page())`; the port encodes the disjointness as a
    // `debug_assert!` on the node pointers rather than leaving it a comment.
    //
    // NEGATIVE CONTROLS EXECUTED (`P3-W2` T4 Step 4), both against
    // `pagelist_erase{Row,RowBounded}_hyperlink_dense_row_crosses_page_boundary`:
    //
    // 1. DROP THE RETRY (return `current` on the first clone error) — both tests
    //    FAIL: `assertion failed: unsafe { (*list_cell.cell).hyperlink() }`. The
    //    row crosses the page boundary with its hyperlink flags but no map
    //    entries, i.e. exactly the half-mutated state the commit fixes.
    // 2. DROP THE `for_clone_error` MAPPING (always return `None`, i.e. rehash at
    //    the same capacity) — both tests HANG rather than failing an assertion:
    //    the rehash never makes room, so the retry loop spins forever (killed at
    //    >60s). Recorded as observed: the error->dimension mapping is what makes
    //    this loop TERMINATE, not merely what makes it correct. There is no
    //    iteration bound here because the reference has none.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn clone_row_grow_capacity(
        &mut self,
        node: *mut Node,
        dst_y: usize,
        src_page: *mut Page,
        src_row: *const crate::page::Row,
    ) -> *mut Node {
        debug_assert!(
            !core::ptr::eq(src_page, unsafe { Node::page(node) }),
            "clone_row_grow_capacity: source page must not be the destination node's page; \
             increase_capacity frees the destination page memory"
        );

        let mut current = node;
        loop {
            let cur_page: *mut Page = unsafe { Node::page(current) };
            let cur_rows = unsafe { (*cur_page).rows.ptr((*cur_page).memory_ptr()) };
            let res =
                unsafe { (*cur_page).clone_row_from(&*src_page, cur_rows.add(dst_y), src_row) };
            let Err(err) = res else {
                return current;
            };

            // Adjust our page capacity to make room for what we
            // didn't have space for.
            current = match self.increase_capacity(current, IncreaseCapacity::for_clone_error(err))
            {
                Ok(n) => n,
                // We can't gracefully recover from either of these
                // here: our callers have already rotated rows, so
                // returning an error would leave the page list
                // half-mutated (and corrupt), so a crash is better.
                Err(IncreaseCapacityError::OutOfMemory) => {
                    panic!("increaseCapacity system allocator OOM")
                }
                Err(IncreaseCapacityError::OutOfSpace) => {
                    panic!("increaseCapacity OutOfSpace")
                }
            };

            // Retry the row copy with the increased capacity.
        }
    }

    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn increase_capacity(
        &mut self,
        node: *mut Node,
        adjustment: Option<IncreaseCapacity>,
    ) -> Result<*mut Node, IncreaseCapacityError> {
        // PORT NOTE: `defer self.assertIntegrity()` — reproduced by calling
        // `assert_integrity` before every exit point below.
        let page: *mut Page = unsafe { super::Node::page(node) };

        // Apply our adjustment
        let mut cap = unsafe { (*page).capacity };
        if let Some(v) = adjustment {
            // PORT NOTE: the reference uses `inline else |tag|` to generate the
            // doubling/clamp logic over the four `Capacity` fields generically.
            // Rust has no comptime field reflection, and the four fields have
            // different integer types, so the arms are expanded explicitly. Each
            // arm is the verbatim translation of the reference body:
            //   const old = @field(cap, field_name);
            //   const new = std.math.mul(Int, old, 2) catch ...;
            //   @field(cap, field_name) = new;
            //   if (Page.layout(cap).total_size > size.max_page_size) OutOfSpace;
            macro_rules! adjust_field {
                ($field:ident, $int:ty) => {{
                    let old = cap.$field;
                    // A dimension can be zero for pages with exact
                    // capacities (see compact). Doubling zero stays zero,
                    // which would break our guarantee that we always
                    // increase by at least one unit and turn caller retry
                    // loops into infinite loops. Jump straight to the
                    // standard default instead: it is what all standard
                    // pages start with, so retrying callers are guaranteed
                    // enough room for their pending allocation.
                    //
                    // PORT NOTE (oracle 8307349ec, `P3-W1 T4`): the reference
                    // writes the default as `const default: Capacity = .{ .cols
                    // = 0, .rows = 0 }` and reads `@field(default, field_name)`
                    // — i.e. the struct's own field defaults. `Capacity::default()`
                    // is rustty's spelling of exactly that.
                    let new: $int = if old == 0 {
                        Capacity::default().$field
                    } else {
                        // We use checked math to prevent overflow. If there is an
                        // overflow it means we're out of space in this dimension,
                        // since pages can take up to their maxInt capacity in any
                        // category.
                        match (old as $int).checked_mul(2) {
                            Some(v) => v,
                            None => {
                                // Our final doubling would overflow since maxInt is
                                // 2^N - 1 for an unsignged int of N bits. So, if we overflow
                                // and we haven't used all the bits, use all the bits.
                                if old < <$int>::MAX {
                                    <$int>::MAX
                                } else {
                                    return Err(IncreaseCapacityError::OutOfSpace);
                                }
                            }
                        }
                    };
                    cap.$field = new;

                    // If our capacity exceeds the maximum page size, treat it
                    // as an OutOfSpace because things like page splitting will
                    // help.
                    let layout = Page::layout(cap);
                    if layout.total_size > size::MAX_PAGE_SIZE as usize {
                        return Err(IncreaseCapacityError::OutOfSpace);
                    }
                }};
            }
            match v {
                IncreaseCapacity::Styles => adjust_field!(styles, size::StyleCountInt),
                IncreaseCapacity::GraphemeBytes => {
                    adjust_field!(grapheme_bytes, size::GraphemeBytesInt)
                }
                IncreaseCapacity::HyperlinkBytes => {
                    adjust_field!(hyperlink_bytes, size::HyperlinkCountInt)
                }
                IncreaseCapacity::StringBytes => adjust_field!(string_bytes, size::StringBytesInt),
            }
        }

        // log.info("adjusting page capacity={}", .{cap});

        // Create our new page and clone the old page into it.
        let new_node = self
            .create_page(super::CreatePage::new(cap))
            .map_err(|_| IncreaseCapacityError::OutOfMemory)?;
        // errdefer self.destroyNode(new_node);
        let new_page: *mut Page = unsafe { super::Node::page(new_node) };
        assert!(unsafe { (*new_page).capacity.rows >= (*page).capacity.rows });
        assert!(unsafe { (*new_page).capacity.cols >= (*page).capacity.cols });
        unsafe { (*new_page).size.rows = (*page).size.rows };
        unsafe { (*new_page).size.cols = (*page).size.cols };
        if let Err(_err) = unsafe { (*new_page).clone_from(page, 0, (*page).size.rows as usize) } {
            // cloneFrom only errors if there isn't capacity for the data
            // from the source page but we're only increasing capacity so
            // this should never be possible. If it happens, we should crash
            // because we're in no man's land and can't safely recover.
            // log.err("increaseCapacity clone failed err={}", .{err});
            panic!("unexpected clone failure");
        }

        // Preserve page-level dirty flag (cloneFrom only copies row data)
        unsafe { (*new_page).dirty = (*page).dirty };

        // Must not fail after this because the operations we do after this
        // can't be recovered.
        // errdefer comptime unreachable;

        // Fix up all our tracked pins to point to the new page.
        for p in self.tracked_pins.keys() {
            let p: *mut super::Pin = *p;
            if !core::ptr::eq(unsafe { (*p).node }, node) {
                continue;
            }
            unsafe { (*p).node = new_node };
        }

        // Insert this page and destroy the old page
        unsafe { self.pages.insert_before(node, new_node) };
        unsafe { self.pages.remove(node) };
        self.destroy_node(node);
        self.page_compression.mark_activity();

        // PORT NOTE: the linked-list splice above takes `&mut Node` through
        // `new_node`, which under Stacked Borrows invalidates the earlier
        // `&mut (*super::Node::page(new_node))`-derived `new_page` tag. Re-derive the page
        // pointer from the node via raw projection (no `&mut` retag) before the
        // final integrity check, matching the raw-pointer discipline used by the
        // `page_memory` helper in `mod.rs`.
        let new_page: *mut Page = unsafe { super::Node::page(new_node) };
        unsafe { (*new_page).assert_integrity() };
        self.assert_integrity();
        Ok(new_node)
    }

    /// Compact a page to use the minimum required memory for the contents
    /// it stores. Returns the new node pointer if compaction occurred, or null
    /// if the page was already compact or compaction would not provide any
    /// savings.
    ///
    /// The compacted page is always an exact-size heap allocation, never
    /// a pool item, since a pool item always retains a full std_size
    /// buffer regardless of the page layout. Note that this means that
    /// when compacting a pool-owned node, the freed pool item is returned
    /// to the pool free list, so the memory savings are only fully
    /// realized once the pool itself is reset or freed.
    ///
    /// If this returns OOM, the PageList is left unchanged and no dangling
    /// memory references exist. It is safe to ignore the error and continue using
    /// the uncompacted page.
    ///
    /// PORT NOTE: `compact` is part of the access/introspection refactor in the
    /// reference but is grouped into this growth/capacity cluster per this
    /// file's owner doc header.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn compact(&mut self, node: *mut Node) -> Result<Option<*mut Node>, super::OutOfMemory> {
        // PORT NOTE: `defer self.assertIntegrity()` — reproduced by calling
        // `assert_integrity` before every exit point below.
        let page: *mut Page = unsafe { super::Node::page(node) };

        // We should never have empty rows in our pagelist anyways...
        assert!(unsafe { (*page).size.rows } > 0);

        // Compute the minimum capacity required for this page's content
        let req_cap = unsafe { (*page).exact_row_capacity(0, (*page).size.rows as usize) };
        let new_size = Page::layout(req_cap).total_size;

        // The memory this node currently retains. A pool-owned node always
        // retains a full pool item no matter its layout size.
        let old_size: usize = match unsafe { (*node).owned } {
            super::Owned::Pool => super::std_size(),
            super::Owned::Heap => Self::page_memory_len(node),
        };
        if new_size >= old_size {
            self.assert_integrity();
            return Ok(None);
        }

        // Create the new smaller page
        let new_node = self.create_page(super::CreatePage::exact(req_cap))?;
        // errdefer self.destroyNode(new_node);
        let new_page: *mut Page = unsafe { super::Node::page(new_node) };
        unsafe { (*new_page).size = (*page).size };
        unsafe { (*new_page).dirty = (*page).dirty };
        if let Err(_err) = unsafe { (*new_page).clone_from(page, 0, (*page).size.rows as usize) } {
            // cloneFrom should not fail when compacting since req_cap is
            // computed to exactly fit the source content and our expectation
            // of exactRowCapacity ensures it can fit all the requested
            // data.
            // log.err("compact clone failed err={}", .{err});

            // In this case, let's gracefully degrade by pretending we
            // didn't need to compact.
            self.destroy_node(new_node);
            self.assert_integrity();
            return Ok(None);
        }

        // Fix up all tracked pins to point to the new page
        for p in self.tracked_pins.keys() {
            let p: *mut super::Pin = *p;
            if !core::ptr::eq(unsafe { (*p).node }, node) {
                continue;
            }
            unsafe { (*p).node = new_node };
        }

        // Insert the new page and destroy the old one
        unsafe { self.pages.insert_before(node, new_node) };
        unsafe { self.pages.remove(node) };
        self.destroy_node(node);
        self.page_compression.mark_activity();

        // PORT NOTE: re-derive `new_page` via raw projection after the
        // linked-list splice (see the matching note in `increase_capacity`):
        // the splice retags `new_node`, invalidating the earlier
        // `&mut (*super::Node::page(new_node))`-derived tag under Stacked Borrows.
        let new_page: *mut Page = unsafe { super::Node::page(new_node) };
        unsafe { (*new_page).assert_integrity() };
        self.assert_integrity();
        Ok(Some(new_node))
    }

    /// Grow the number of rows available in the page list by n.
    /// This is only used for testing so it isn't optimized in any way.
    ///
    /// PORT NOTE: `growRows` is the verbatim `for (0..n) |_| _ = try self.grow()`
    /// helper from the reference. It lives in the resize cluster in the
    /// reference; included here as it is a thin wrapper over `grow`.
    #[allow(dead_code)]
    pub(crate) fn grow_rows(&mut self, n: usize) -> Result<(), super::OutOfMemory> {
        for _ in 0..n {
            let _ = self.grow()?;
        }
        Ok(())
    }
}

/// `PageList.SplitError`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitError {
    // Allocator OOM
    OutOfMemory,
    // Page can't be split further because it is already a single row.
    OutOfSpace,
}

/// Possible dimensions to increase capacity for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IncreaseCapacity {
    Styles,
    GraphemeBytes,
    HyperlinkBytes,
    StringBytes,
}

impl IncreaseCapacity {
    /// Returns the capacity dimension that must be increased for the
    /// given row clone error to succeed on retry, or null if the page
    /// only needs to be rehashed at its current capacity.
    pub fn for_clone_error(err: crate::page::CloneFromError) -> Option<IncreaseCapacity> {
        use crate::page::CloneFromError;
        match err {
            // Rehash the sets
            CloneFromError::StyleSetNeedsRehash | CloneFromError::HyperlinkSetNeedsRehash => None,

            // Increase style memory
            CloneFromError::StyleSetOutOfMemory => Some(IncreaseCapacity::Styles),

            // Increase string memory
            CloneFromError::StringAllocOutOfMemory => Some(IncreaseCapacity::StringBytes),

            // Increase hyperlink memory
            CloneFromError::HyperlinkSetOutOfMemory | CloneFromError::HyperlinkMapOutOfMemory => {
                Some(IncreaseCapacity::HyperlinkBytes)
            }

            // Increase grapheme memory
            CloneFromError::GraphemeMapOutOfMemory | CloneFromError::GraphemeAllocOutOfMemory => {
                Some(IncreaseCapacity::GraphemeBytes)
            }
        }
    }
}

/// `PageList.IncreaseCapacityError`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IncreaseCapacityError {
    // An actual system OOM trying to allocate memory.
    OutOfMemory,

    // The existing page is already at max capacity for the given
    // adjustment. The caller must create a new page, remove data from
    // the old page, etc. (up to the caller).
    OutOfSpace,
}

/// This represents the state necessary to render a scrollbar for this
/// PageList. It has the total size, the offset, and the size of the viewport.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scrollbar {
    /// Total size of the scrollable area.
    pub total: usize,

    /// The offset into the total area that the viewport is at. This is
    /// guaranteed to be less than or equal to total. This includes the
    /// visible row.
    pub offset: usize,

    /// The length of the visible area. This is including the offset row.
    pub len: usize,
}

impl Scrollbar {
    /// A zero-sized scrollable region.
    pub const ZERO: Scrollbar = Scrollbar {
        total: 0,
        offset: 0,
        len: 0,
    };

    pub fn cval(self) -> ScrollbarC {
        ScrollbarC {
            total: self.total as u64,
            offset: self.offset as u64,
            len: self.len as u64,
        }
    }

    /// Comparison for scrollbars.
    pub fn eql(self, other: Scrollbar) -> bool {
        self.total == other.total && self.offset == other.offset && self.len == other.len
    }
}

// Sync with: ghostty_action_scrollbar_s
//
// PORT NOTE: the reference nests this as `Scrollbar.C`. Rust can't nest a type
// inside a struct value namespace the same way, so it is hoisted to a sibling
// `ScrollbarC` with the identical `extern` layout.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScrollbarC {
    pub total: u64,
    pub offset: u64,
    pub len: u64,
}
