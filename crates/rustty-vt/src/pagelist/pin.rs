//! PageList pins + tracking cluster.
//!
//! PORTS: ghostty src/terminal/PageList.zig — the `Pin` struct (with
//! `rowAndCell`, `cells` + `CellSubset`, `isBetween`, `before`, `leftWrap`,
//! `rightWrap`, `downOverflow`, `upOverflow`, and the remaining navigation
//! helpers) plus the tracking API: `pin`, `trackPin`, `untrackPin`,
//! `countTrackedPins`, `trackedPins`, `pinIsValid`, `pinIsActive`, `pinIsTop`,
//! and `pointFromPin`.
//!
//! PORT NOTE: the `Pin` struct *fields* are forward-declared in `mod.rs` so the
//! `PageList` struct fields (`viewport_pin: *mut Pin`) can reference them; this
//! sub-file owns the full `impl Pin` + the `PinSet` hashmap type and the
//! tracking methods.
//!
//! PORT STATUS: methods ported (vt-3c). `pinIsValid` lives in `mod.rs` (the CORE
//! `verify_integrity` caller needs it); this file ports `pin`, `trackPin`,
//! `untrackPin`, `countTrackedPins`, `trackedPins`, `pointFromPin`,
//! `pinIsActive`, `pinIsTop`, and the full `impl Pin` navigation set
//! (`rowAndCell`, `cells`, `isBetween`, `before`, `leftWrap`, `rightWrap`,
//! `down`, `up`, `eql`, `isDirty`, `markDirty`, plus the `downOverflow`/
//! `upOverflow` they delegate to). The 205-block test split lands in the
//! dedicated test phase.

use super::{Node, PageList, Pin};
use crate::page as pagepkg;
use crate::point;
use crate::style as stylepkg;
use rustty_render_types::term_size as size;

impl PageList {
    /// Returns the pin for the given point. The pin is NOT tracked so it
    /// is only valid as long as the pagelist isn't modified.
    ///
    /// This will return null if the point is out of bounds. The caller
    /// should clamp the point to the bounds of the coordinate space if
    /// necessary.
    pub fn pin(&self, pt: point::Point) -> Option<Pin> {
        // getTopLeft is much more expensive than checking the cols bounds
        // so we do this first.
        let x = pt.coord().x;
        if x >= self.cols {
            return None;
        }

        // Grab the top left and move to the point.
        let mut p = self.get_top_left(pt.tag()).down(pt.coord().y as usize)?;
        // Incomplete reflow can leave a page narrower than the desired width.
        // Never manufacture an out-of-bounds pin for that page.
        if x >= unsafe { super::Node::cols(p.node) } {
            return None;
        }
        p.x = x;
        Some(p)
    }

    /// Convert the given pin to a tracked pin. A tracked pin will always be
    /// automatically updated as the pagelist is modified. If the point the
    /// pin points to is removed completely, the tracked pin will be updated
    /// to the top-left of the screen.
    pub fn track_pin(&mut self, p: Pin) -> Result<*mut Pin, super::OutOfMemory> {
        // PORT NOTE: the reference `if (build_options.slow_runtime_safety)`
        // guard maps to `cfg!(debug_assertions)` per the crate convention (see
        // `pin_is_valid` in `mod.rs`).
        if cfg!(debug_assertions) {
            assert!(self.pin_is_valid(p.pin_copy()));
        }

        // Create our tracked pin
        let tracked = self.pool.pins.create().map_err(|_| super::OutOfMemory)?;
        // errdefer self.pool.pins.destroy(tracked);
        unsafe {
            tracked.as_ptr().write(p);
        }
        let tracked = tracked.as_ptr();

        // Add it to the tracked list
        //
        // PORT NOTE: the reference `tracked_pins.putNoClobber(self.pool.alloc,
        // tracked, {})` is `Allocator.Error!void`; the Rust `PinSet` owns its
        // backing `Vec`, so `put_no_clobber` is infallible (it can never signal
        // OOM), and the reference's `errdefer _ = tracked_pins.remove(tracked)`
        // can therefore never fire — there is no fallible step after the insert.
        self.tracked_pins.put_no_clobber(tracked, ());

        Ok(tracked)
    }

    /// Untrack a previously tracked pin. This will deallocate the pin.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn untrack_pin(&mut self, p: *mut Pin) {
        assert!(!core::ptr::eq(p, self.viewport_pin));
        if self.tracked_pins.swap_remove(&p) {
            unsafe {
                self.pool.pins.destroy(core::ptr::NonNull::new_unchecked(p));
            }
        }
    }

    pub fn count_tracked_pins(&self) -> usize {
        self.tracked_pins.count()
    }

    /// Returns the tracked pins for this pagelist. The slice is owned by the
    /// pagelist and is only valid until the pagelist is modified.
    //
    // PORT NOTE: the reference `trackedPins` returns `[]const *Pin` — a borrow of
    // the hashmap's contiguous keys array. The Rust `PinSet`
    // (`AutoArrayHashMapUnmanaged<*mut Pin, ()>`) stores its entries as
    // `Vec<(*mut Pin, ())>`, so the keys are not a contiguous `[*mut Pin]` slice;
    // we therefore collect them into an owned `Vec<*mut Pin>` in insertion order
    // (`keys()`), which preserves the same "valid until the pagelist is modified"
    // contract (the returned `Vec` is a fresh snapshot per call).
    pub fn tracked_pins(&self) -> Vec<*mut Pin> {
        self.tracked_pins.keys().copied().collect()
    }

    /// Convert a pin to a point in the given context. If the pin can't fit
    /// within the given tag (i.e. its in the history but you requested active),
    /// then this will return null.
    ///
    /// Note that this can be a very expensive operation depending on the tag and
    /// the location of the pin. This works by traversing the linked list of pages
    /// in the tagged region.
    ///
    /// Therefore, this is recommended only very rarely.
    pub fn point_from_pin(&self, tag: point::Tag, p: Pin) -> Option<point::Point> {
        let tl = self.get_top_left(tag);

        // Count our first page which is special because it may be partial.
        let mut coord = point::Coordinate { x: p.x, y: 0 };
        if core::ptr::eq(p.node, tl.node) {
            // If our top-left is after our y then we're outside the range.
            if tl.y > p.y {
                return None;
            }
            coord.y = (p.y - tl.y) as u32;
        } else {
            // PORT NOTE (oracle 30b42f42a, `REVIEW_CLASSES.md` §1): every
            // cross-page row contribution is a CHECKED add. The reference used
            // `std.math.add(u32, ...) catch return null`; the plain `+=` this
            // replaces panicked in debug and WRAPPED SILENTLY in release for an
            // unbounded PageList with more than 2^32 rows, converting a valid
            // pin into a plausible-looking wrong point. `None` is the oracle's
            // fallback — it reuses the existing "outside the range" result, so
            // no caller sees a new error shape.
            coord.y = coord
                .y
                .checked_add((unsafe { super::Node::rows(tl.node) } - tl.y) as u32)?;
            let mut node_ = unsafe { (*tl.node).next };
            loop {
                if node_.is_null() {
                    // We never saw our node, meaning we're outside the range.
                    return None;
                }
                let node = node_;
                if core::ptr::eq(node, p.node) {
                    coord.y = coord.y.checked_add(p.y as u32)?;
                    break;
                }

                coord.y = coord
                    .y
                    .checked_add(unsafe { super::Node::rows(node) } as u32)?;
                node_ = unsafe { (*node).next };
            }
        }

        Some(point::Point::from_c(tag, coord))
    }

    /// Returns the viewport for the given pin, preferring to pin to
    /// "active" if the pin is within the active area.
    //
    // PORT NOTE: `#[allow(dead_code)]` — the reference `pinIsActive` is a private
    // helper whose callers (`fixupViewport`, `scroll`, `resize`, `eraseRows`) live
    // in the grow/scroll/resize/erase clusters whose bodies land later; until then
    // the CORE skeleton has no callers for it.
    #[allow(dead_code)]
    pub(crate) fn pin_is_active(&self, p: Pin) -> bool {
        // If the pin is in the active page, then we can quickly determine
        // if we're beyond the end.
        let active = self.get_top_left(point::Tag::Active);
        if core::ptr::eq(p.node, active.node) {
            return p.y >= active.y;
        }

        let mut node_ = unsafe { (*active.node).next };
        while !node_.is_null() {
            let node = node_;
            // This loop is pretty fast because the active area is
            // never that large so this is at most one, two nodes for
            // reasonable terminals (including very large real world
            // ones).

            // A node forward in the active area is our node, so we're
            // definitely in the active area.
            if core::ptr::eq(node, p.node) {
                return true;
            }
            node_ = unsafe { (*node).next };
        }

        false
    }

    /// Returns true if the pin is at the top of the scrollback area.
    pub(crate) fn pin_is_top(&self, p: Pin) -> bool {
        p.y == 0 && core::ptr::eq(p.node, self.pages.first)
    }
}

/// The subset of cells returned by `Pin::cells`.
//
// PORT NOTE: the reference `CellSubset = enum { all, left, right }` is nested in
// the `Pin` struct; in Rust it lives alongside the `impl Pin` block.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CellSubset {
    All,
    Left,
    Right,
}

/// Return type of `Pin::downOverflow`/`Pin::upOverflow`.
//
// PORT NOTE: the reference returns an anonymous `union(enum) { offset: Pin,
// overflow: struct { end: Pin, remaining: usize } }`. This ports to a named Rust
// enum shared by both methods (identical structure in the reference).
pub enum PinOverflow {
    Offset(Pin),
    Overflow { end: Pin, remaining: usize },
}

impl Pin {
    pub fn row_and_cell(&self) -> pagepkg::RowAndCell {
        let rac = unsafe {
            (*super::Node::page(self.node)).get_row_and_cell(self.x as usize, self.y as usize)
        };
        pagepkg::RowAndCell {
            row: rac.row,
            cell: rac.cell,
        }
    }

    /// Returns the cells for the row that this pin is on. The subset determines
    /// what subset of the cells are returned. The "left/right" subsets are
    /// inclusive of the x coordinate of the pin.
    pub fn cells(&self, subset: CellSubset) -> *mut [pagepkg::Cell] {
        let rac = self.row_and_cell();
        let all = unsafe { (*super::Node::page(self.node)).get_cells(rac.row) };
        // PORT NOTE: the reference `getCells` returns a `[]Cell` slice; the Rust
        // port returns the raw `*mut [Cell]` (a fat pointer carrying the length).
        // We read its length via the raw-slice `len()` (no deref of the target)
        // to compute the `.right` subset bound.
        let all_len = all.len();
        let all_base = all as *mut pagepkg::Cell;
        match subset {
            CellSubset::All => all,
            CellSubset::Left => core::ptr::slice_from_raw_parts_mut(all_base, self.x as usize + 1),
            CellSubset::Right => unsafe {
                let base = all_base.add(self.x as usize);
                core::ptr::slice_from_raw_parts_mut(base, all_len - self.x as usize)
            },
        }
    }

    /// Returns true if this pin is between the top and bottom, inclusive.
    //
    // Note: this is primarily unit tested as part of the Kitty
    // graphics deletion code.
    pub fn is_between(&self, top: Pin, bottom: Pin) -> bool {
        if cfg!(debug_assertions) {
            if core::ptr::eq(top.node, bottom.node) {
                // If top is bottom, must be ordered.
                assert!(top.y <= bottom.y);
                if top.y == bottom.y {
                    assert!(top.x <= bottom.x);
                }
            } else {
                // If top is not bottom, top must be before bottom.
                let mut node_ = unsafe { (*top.node).next };
                let mut found = false;
                while !node_.is_null() {
                    let node = node_;
                    if core::ptr::eq(node, bottom.node) {
                        found = true;
                        break;
                    }
                    node_ = unsafe { (*node).next };
                }
                assert!(found);
            }
        }

        if core::ptr::eq(self.node, top.node) {
            // If our pin is the top page and our y is less than the top y
            // then we can't possibly be between the top and bottom.
            if self.y < top.y {
                return false;
            }

            // If our y is after the top y but we're on the same page
            // then we're between the top and bottom if our y is less
            // than or equal to the bottom y if its the same page. If the
            // bottom is another page then it means that the range is
            // at least the full top page and since we're the same page
            // we're in the range.
            if self.y > top.y {
                return if core::ptr::eq(self.node, bottom.node) {
                    self.y <= bottom.y
                } else {
                    true
                };
            }

            // Otherwise our y is the same as the top y, so we need to
            // check the x coordinate.
            assert!(self.y == top.y);
            if self.x < top.x {
                return false;
            }
        }
        if core::ptr::eq(self.node, bottom.node) {
            // Our page is the bottom page so we're between the top and
            // bottom if our y is less than the bottom y.
            if self.y > bottom.y {
                return false;
            }
            if self.y < bottom.y {
                return true;
            }

            // If our y is the same, then we're between if we're before
            // or equal to the bottom x.
            assert!(self.y == bottom.y);
            return self.x <= bottom.x;
        }

        // Our page isn't the top or bottom so we need to check if
        // our page is somewhere between the top and bottom.

        // Since our loop starts at top.page.next we need to check that
        // top != bottom because if they're the same then we can't possibly
        // be between them.
        if core::ptr::eq(top.node, bottom.node) {
            return false;
        }
        let mut node_ = unsafe { (*top.node).next };
        while !node_.is_null() {
            let node = node_;
            if core::ptr::eq(node, bottom.node) {
                break;
            }
            if core::ptr::eq(node, self.node) {
                return true;
            }
            node_ = unsafe { (*node).next };
        }

        false
    }

    /// Returns true if self is before other. This is very expensive since
    /// it requires traversing the linked list of pages. This should not
    /// be called in performance critical paths.
    pub fn before(&self, other: Pin) -> bool {
        if core::ptr::eq(self.node, other.node) {
            if self.y < other.y {
                return true;
            }
            if self.y > other.y {
                return false;
            }
            return self.x < other.x;
        }

        let mut node_ = unsafe { (*self.node).next };
        while !node_.is_null() {
            let node = node_;
            if core::ptr::eq(node, other.node) {
                return true;
            }
            node_ = unsafe { (*node).next };
        }

        false
    }

    #[inline]
    pub fn eql(&self, other: Pin) -> bool {
        core::ptr::eq(self.node, other.node) && self.y == other.y && self.x == other.x
    }

    /// Check if this pin is dirty.
    //
    // PORT NOTE: `#[allow(dead_code)]` — the reference `isDirty` is a `pub inline`
    // helper whose callers land in later clusters/tests; until then the CORE
    // skeleton has no callers for it.
    #[allow(dead_code)]
    #[inline]
    pub fn is_dirty(&self) -> bool {
        unsafe { (*super::Node::page(self.node)).dirty || (*self.row_and_cell().row).dirty() }
    }

    /// Mark this pin location as dirty.
    #[inline]
    pub fn mark_dirty(&self) {
        unsafe { (*self.row_and_cell().row).set_dirty(true) };
    }

    /// Move the pin left n columns. n must fit within the size.
    //
    // PORT NOTE: the reference `left` is `pub inline fn left` — it is a public
    // navigation helper used both by `leftWrap`/`rightWrap` (below, this cluster)
    // and by the iter cluster's `PromptIterator` (`iter.rs`). We keep it `pub` to
    // match the reference visibility and allow the iter cluster to call it.
    // `right` (below) stays private; it has no cross-cluster caller.
    #[inline]
    pub fn left(&self, n: usize) -> Pin {
        assert!(n <= self.x as usize);
        let mut result = self.pin_copy();
        // result.x -= std.math.cast(size.CellCountInt, n) orelse result.x;
        result.x -= match size::CellCountInt::try_from(n) {
            Ok(v) => v,
            Err(_) => result.x,
        };
        result
    }

    /// Move the pin right n columns. n must fit within the size.
    //
    // PORT NOTE: the reference is `pub inline fn right`; it was narrowed to
    // private here only because `right_wrap` was its sole caller. `d6e24d985`
    // rewrites `right_wrap` as a per-row loop that no longer delegates to it,
    // so the visibility is restored to the reference's `pub` rather than
    // deleting a function the oracle still exports.
    #[inline]
    pub fn right(&self, n: usize) -> Pin {
        assert!((self.x as usize) + n < unsafe { super::Node::cols(self.node) } as usize);
        let mut result = self.pin_copy();
        // result.x +|= std.math.cast(size.CellCountInt, n) orelse
        //     std.math.maxInt(size.CellCountInt);
        let add = match size::CellCountInt::try_from(n) {
            Ok(v) => v,
            Err(_) => size::CellCountInt::MAX,
        };
        result.x = result.x.saturating_add(add);
        result
    }

    /// Move the pin left n columns, stopping at the start of the row.
    #[inline]
    pub fn left_clamp(&self, n: size::CellCountInt) -> Pin {
        let mut result = self.pin_copy();
        result.x = result.x.saturating_sub(n); // result.x -|= n
        result
    }

    /// Move the pin right n columns, stopping at the end of the row.
    #[inline]
    pub fn right_clamp(&self, n: size::CellCountInt) -> Pin {
        let mut result = self.pin_copy();
        result.x = self
            .x
            .saturating_add(n)
            .min(unsafe { super::Node::cols(self.node) } - 1);
        result
    }

    /// Move the pin left n cells, wrapping to the previous row as needed.
    ///
    /// If the offset goes beyond the top of the screen, returns null.
    ///
    /// TODO: Unit tests.
    //
    // PORT NOTE (`P3-W1 T1`): this is the end state of TWO oracle commits.
    // `0ff4e41b2` ("terminal: fix pin wrapping at row boundaries") first
    // rebased the remainder arithmetic on `extra_after_remaining - 1`, then
    // `d6e24d985` ("terminal: make pin traversal width-aware") replaced the
    // remainder arithmetic entirely with the per-row loop below, because the
    // closed form assumes every page has the same width — an assumption
    // incomplete reflow violates. Both hashes are recorded so the parity ledger
    // stays honest: `0ff4e41b2`'s four regression tests are ported (they still
    // pass against this end state, which is the point), `d6e24d985`'s three
    // mixed-width tests are ported alongside them, and the verbatim pre-fix
    // comment "This assumes that all pages have the same width" is deleted with
    // the code it described.
    pub fn left_wrap(&self, n: usize) -> Option<Pin> {
        let mut result = *self;
        let mut remaining = n;
        while remaining > result.x as usize {
            remaining -= result.x as usize + 1;
            result = result.up(1)?;
            // Crossing a row boundary lands on that destination row's final
            // cell, whose width may differ from ours during reflow.
            result.x = unsafe { super::Node::cols(result.node) } - 1;
        }

        result.x -= remaining as size::CellCountInt;
        Some(result)
    }

    /// Move the pin right n cells, wrapping to the next row as needed.
    ///
    /// If the offset goes beyond the bottom of the screen, returns null.
    ///
    /// TODO: Unit tests.
    //
    // PORT NOTE (`P3-W1 T1`): see `left_wrap` — oracle `0ff4e41b2` then
    // `d6e24d985`, end state only.
    pub fn right_wrap(&self, n: usize) -> Option<Pin> {
        let mut result = *self;
        let mut remaining = n;
        loop {
            let row_remaining = (unsafe { super::Node::cols(result.node) } - result.x - 1) as usize;
            if remaining <= row_remaining {
                result.x += remaining as size::CellCountInt;
                return Some(result);
            }

            remaining -= row_remaining + 1;
            result = result.down(1)?;
            result.x = 0;
        }
    }

    /// Move the pin down a certain number of rows, or return null if
    /// the pin goes beyond the end of the screen.
    #[inline]
    pub fn down(&self, n: usize) -> Option<Pin> {
        match self.down_overflow(n) {
            PinOverflow::Offset(v) => Some(v),
            PinOverflow::Overflow { .. } => None,
        }
    }

    /// Move the pin up a certain number of rows, or return null if
    /// the pin goes beyond the start of the screen.
    #[inline]
    pub fn up(&self, n: usize) -> Option<Pin> {
        match self.up_overflow(n) {
            PinOverflow::Offset(v) => Some(v),
            PinOverflow::Overflow { .. } => None,
        }
    }

    /// Move the offset down n rows. If the offset goes beyond the
    /// end of the screen, return the overflow amount.
    pub fn down_overflow(&self, n: usize) -> PinOverflow {
        // Index fits within this page
        let rows = unsafe { super::Node::rows(self.node) } as usize - (self.y as usize + 1);
        if n <= rows {
            return PinOverflow::Offset(Pin {
                node: self.node,
                y: match size::CellCountInt::try_from(self.y as usize + n) {
                    Ok(v) => v,
                    Err(_) => size::CellCountInt::MAX,
                },
                x: self.x,
                garbage: false,
            });
        }

        // Need to traverse page links to find the page
        let mut node: *mut Node = self.node;
        let mut n_left: usize = n - rows;
        loop {
            let next = unsafe { (*node).next };
            if next.is_null() {
                return PinOverflow::Overflow {
                    end: Pin {
                        node,
                        y: unsafe { super::Node::rows(node) } - 1,
                        x: self.x.min(unsafe { super::Node::cols(node) } - 1),
                        garbage: false,
                    },
                    remaining: n_left,
                };
            }
            node = next;
            let node_rows = unsafe { super::Node::rows(node) } as usize;
            if n_left <= node_rows {
                return PinOverflow::Offset(Pin {
                    node,
                    y: match size::CellCountInt::try_from(n_left - 1) {
                        Ok(v) => v,
                        Err(_) => size::CellCountInt::MAX,
                    },
                    x: self.x.min(unsafe { super::Node::cols(node) } - 1),
                    garbage: false,
                });
            }
            n_left -= node_rows;
        }
    }

    /// Move the offset up n rows. If the offset goes beyond the
    /// start of the screen, return the overflow amount.
    pub fn up_overflow(&self, n: usize) -> PinOverflow {
        // Index fits within this page
        if n <= self.y as usize {
            return PinOverflow::Offset(Pin {
                node: self.node,
                y: match size::CellCountInt::try_from(self.y as usize - n) {
                    Ok(v) => v,
                    Err(_) => size::CellCountInt::MAX,
                },
                x: self.x,
                garbage: false,
            });
        }

        // Need to traverse page links to find the page
        let mut node: *mut Node = self.node;
        let mut n_left: usize = n - self.y as usize;
        loop {
            let prev = unsafe { (*node).prev };
            if prev.is_null() {
                return PinOverflow::Overflow {
                    end: Pin {
                        node,
                        y: 0,
                        x: self.x.min(unsafe { super::Node::cols(node) } - 1),
                        garbage: false,
                    },
                    remaining: n_left,
                };
            }
            node = prev;
            let node_rows = unsafe { super::Node::rows(node) } as usize;
            if n_left <= node_rows {
                return PinOverflow::Offset(Pin {
                    node,
                    y: match size::CellCountInt::try_from(node_rows - n_left) {
                        Ok(v) => v,
                        Err(_) => size::CellCountInt::MAX,
                    },
                    x: self.x.min(unsafe { super::Node::cols(node) } - 1),
                    garbage: false,
                });
            }
            n_left -= node_rows;
        }
    }

    /// Returns the grapheme codepoints for the given cell. These are only
    /// the EXTRA codepoints and not the first codepoint.
    #[inline]
    #[allow(clippy::not_unsafe_ptr_arg_deref)] // reference takes `cell: *const Cell`
    pub fn grapheme(&self, cell: *const pagepkg::Cell) -> Option<&[u32]> {
        unsafe { (*super::Node::page(self.node)).lookup_grapheme(cell) }
    }

    /// Returns the style for the given cell in this pin.
    #[inline]
    #[allow(clippy::not_unsafe_ptr_arg_deref)] // reference takes `cell: *const Cell`
    pub fn style(&self, cell: *const pagepkg::Cell) -> stylepkg::Style {
        if unsafe { (*cell).style_id() } == stylepkg::DEFAULT_ID {
            return stylepkg::Style::default();
        }
        unsafe {
            *(*super::Node::page(self.node)).styles.get(
                (*super::Node::page(self.node)).memory_ptr(),
                (*cell).style_id(),
            )
        }
    }
}

// =============================================================================
// P3-W0 T6 step 3 — NEGATIVE CONTROLS for the LATENT verdicts in
// `.port/phase-3/REACHABILITY.md`, RE-POLARISED by `P3-W1 T1`.
//
// These two tests were written by `P3-W0` to prove the `left_wrap`/`right_wrap`
// defects were REAL while the production callers did not reach them (verdict
// LATENT). `P3-W1` closed row 1b and found the verdict wrong: `GridSize::update`
// floors the column count at 1 (`rustty-render-types/src/lib.rs`) and
// `grid_size.columns` flows straight into `Terminal::resize`
// (`rustty-termio/src/termio_impl.rs:1084`), so a ONE-COLUMN terminal is
// constructible — and at `cols == 1` the callers' literal `n = 1` DOES reach
// the remainder path. Rows 1/2 are therefore LIVE, and `d6e24d985`'s width-aware
// rewrite is the fix.
//
// The tests are kept, with their arguments unchanged, but their assertions are
// now the CORRECT answers rather than the buggy ones. Keeping them (rather than
// deleting them) means a regression to the pre-fix arithmetic fails here as well
// as in the ported oracle tests.
// =============================================================================
#[cfg(test)]
mod p3_reachability_negative_controls {
    use super::super::{Options, PageList};
    use crate::point;

    /// `pin.rs` `right_wrap`, pre-fix: `result.x = (extra_after_remaining % cols - 1)`.
    /// When `extra_after_remaining` was an exact multiple of `cols` the modulo
    /// was 0 and the `- 1` underflowed `usize`. `d6e24d985`'s per-row loop has
    /// no remainder arithmetic at all, so the same argument now returns the
    /// correct destination.
    #[test]
    fn p3_right_wrap_exact_row_multiple_underflows() {
        let mut s = PageList::init(Options::new(80, 24)).unwrap();
        let pin = s
            .pin(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
            .unwrap();
        // 80 - 0 - 1 = 79 remaining in the row, so n = 79 + 80 = 159 walks to
        // the end of row 0, then across all of row 1, landing on the last cell
        // of row 1 (x = 79, y = 1). Pre-fix this panicked with
        // "attempt to subtract with overflow".
        let got = pin.right_wrap(159).expect("within the pagelist");
        assert_eq!(
            point::Point::Active(point::Coordinate { x: 79, y: 1 }),
            s.point_from_pin(point::Tag::Active, got).unwrap()
        );
        s.deinit();
    }

    /// `pin.rs` `left_wrap`, pre-fix: the symmetric site,
    /// `result.x = (cols - extra_after_remaining % cols)`. That one did not
    /// underflow; on an exact row multiple it yielded `x == cols`, ONE PAST the
    /// last valid column — a silently invalid pin rather than a panic, which is
    /// why it keeps a value assertion instead of `#[should_panic]`. The value is
    /// now the correct in-range answer.
    #[test]
    fn p3_left_wrap_exact_row_multiple_yields_out_of_range_x() {
        let mut s = PageList::init(Options::new(80, 24)).unwrap();
        // Start far enough down that the upward walk stays in range.
        let pin = s
            .pin(point::Point::Active(point::Coordinate { x: 0, y: 10 }))
            .unwrap();
        // x = 0 -> 1 cell consumed to reach the end of row 9, then 79 more
        // walks left across row 9 to x = 0. Pre-fix this yielded x == 80.
        let got = pin.left_wrap(80).expect("within the pagelist");
        assert_eq!(
            got.x, 0,
            "post-fix the exact row multiple lands on a VALID column. If this \
             ever becomes 80 again the pre-fix remainder arithmetic is back."
        );
        assert_eq!(
            point::Point::Active(point::Coordinate { x: 0, y: 9 }),
            s.point_from_pin(point::Tag::Active, got).unwrap()
        );
        s.deinit();
    }
}
