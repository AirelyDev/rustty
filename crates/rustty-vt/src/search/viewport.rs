//! rustty-vt search/viewport
//!
//! PORTS: ghostty src/terminal/search/viewport.zig
//!
//! PORT STATUS: complete (vt-15). Faithful port of `ViewportSearch`, which
//! searches only the pages covered by the viewport and re-searches only when
//! the viewport (or, optionally, the dirty active area) changes.

// const std = @import("std");
// const assert = @import("../../quirks.zig").inlineAssert;
// const testing = std.testing;
// const Allocator = std.mem.Allocator;
// const point = @import("../point.zig");
// const size = @import("../size.zig");
// const FlattenedHighlight = @import("../highlight.zig").Flattened;
// const PageList = @import("../PageList.zig");
// const SlidingWindow = @import("sliding_window.zig").SlidingWindow;
// const Terminal = @import("../Terminal.zig");
use super::sliding_window::{Direction, SlidingWindow};
use crate::alloc::Allocator;
use crate::highlight::FlattenedRef;
use crate::pagelist::iter::Direction as IterDirection;
use crate::pagelist::{Node, OutOfMemory, PageList};
use crate::point;

/// Searches for a substring within the viewport of a PageList.
///
/// This contains logic to efficiently detect when the viewport changes
/// and only re-search when necessary.
///
/// The specialization for "viewport" is because the viewport is the
/// only part of the search where the user can actively see the results,
/// usually. In that case, it is more efficient to re-search only the
/// viewport rather than store all the results for the entire screen.
///
/// Note that this searches all the pages that viewport covers, so
/// this can include extra matches outside the viewport if the data
/// lives in the same page.
pub struct ViewportSearch {
    pub window: SlidingWindow,
    // PORT NOTE (oracle `9659167ec`): `fingerprint: Fingerprint = .{}` — no
    // longer optional. The empty fingerprint IS the "never searched" state, and
    // its storage survives `reset()` so the next update reuses it.
    pub fingerprint: Fingerprint,

    /// If this is null, then active dirty tracking is disabled and if the
    /// viewport overlaps the active area we always re-search. If this is
    /// non-null, then we only re-search if the active area is dirty. Dirty
    /// marking is up to the caller.
    pub active_dirty: Option<bool>,
}

impl ViewportSearch {
    // pub fn init(
    //     alloc: Allocator,
    //     needle_unowned: []const u8,
    // ) Allocator.Error!ViewportSearch {
    pub fn init(alloc: Allocator, needle_unowned: &[u8]) -> Result<ViewportSearch, OutOfMemory> {
        // We just do a forward search since the viewport is usually
        // pretty small so search results are instant anyways. This avoids
        // a small amount of work to reverse things.
        // var window: SlidingWindow = try .init(alloc, .forward, needle_unowned);
        // errdefer window.deinit();
        let window: SlidingWindow = SlidingWindow::init(alloc, Direction::Forward, needle_unowned)?;
        // return .{
        //     .window = window,
        //     .active_dirty = null,
        // };
        Ok(ViewportSearch {
            window,
            fingerprint: Fingerprint::default(),
            active_dirty: None,
        })
    }

    // pub fn deinit(self: *ViewportSearch) void {
    pub fn deinit(&mut self) {
        // self.fingerprint.deinit(self.window.alloc);
        self.fingerprint.deinit(self.window.alloc);
        // self.window.deinit();
        self.window.deinit();
    }

    /// Reset our fingerprint and results so that the next update will
    /// always re-search.
    // pub fn reset(self: *ViewportSearch) void {
    pub fn reset(&mut self) {
        // self.fingerprint.clearAndRetainCapacity();
        //
        // PORT NOTE (oracle `9659167ec`): reset no longer FREES the fingerprint;
        // it keeps the buffer so the next update reuses the capacity. That is
        // the entire point of the commit.
        self.fingerprint.clear_and_retain_capacity();
        // self.window.clearAndRetainCapacity();
        self.window.clear_and_retain_capacity();
    }

    /// The needle that this search is using.
    // pub fn needle(self: *const ViewportSearch) []const u8 {
    pub fn needle(&self) -> &[u8] {
        // assert(self.window.direction == .forward);
        assert!(self.window.direction == Direction::Forward);
        // return self.window.needle;
        &self.window.needle
    }

    /// Update the sliding window to reflect the current viewport. This
    /// will do nothing if the viewport hasn't changed since the last
    /// search.
    ///
    /// The PageList must be safe to read throughout the lifetime of this
    /// function.
    ///
    /// Returns true if the viewport changed and a re-search is needed.
    /// Returns false if the viewport is unchanged.
    //
    // pub fn update(
    //     self: *ViewportSearch,
    //     list: *PageList,
    // ) Allocator.Error!bool {
    pub fn update(&mut self, list: &PageList) -> Result<bool, OutOfMemory> {
        // See if our viewport changed. We retain the fingerprint buffer so
        // unchanged viewports don't allocate and changed viewports usually
        // reuse the previous capacity.
        // const fingerprint = &self.fingerprint;
        // if (!try fingerprint.update(self.window.alloc, list)) match: {
        //
        // PORT NOTE (oracle `9659167ec`, area D §4.3 UB HAZARD): upstream binds
        // `const fingerprint = &self.fingerprint` and then, inside the same
        // scope, calls `self.window.append*` — two disjoint fields of `self`.
        // rustty does NOT take that alias: `self.fingerprint` is re-borrowed at
        // each use site so no borrow of it is ever live across a `&mut
        // self.window` call. See `.port/phase-3/mirihar-fingerprint-update` for
        // why the analogous snapshot is UB in Rust.
        let alloc = self.window.alloc;
        let changed = self.fingerprint.update(alloc, list)?;
        if !changed {
            'match_blk: {
                // Determine if we need to check if we overlap the active area.
                // If we have dirty tracking on we also set it to false here.
                // const check_active: bool = active: {
                //     const dirty = self.active_dirty orelse break :active true;
                //     if (!dirty) break :active false;
                //     self.active_dirty = false;
                //     break :active true;
                // };
                let check_active: bool = 'active: {
                    let dirty = match self.active_dirty {
                        Some(d) => d,
                        None => break 'active true,
                    };
                    if !dirty {
                        break 'active false;
                    }
                    self.active_dirty = Some(false);
                    true
                };

                // if (check_active) {
                if check_active {
                    // If our fingerprint contains the active area, then we always
                    // re-search since the active area is mutable.
                    // const active_tl = list.getTopLeft(.active);
                    let active_tl = list.get_top_left(point::Tag::Active);
                    // const active_br = list.getBottomRight(.active).?;
                    let active_br = list.get_bottom_right(point::Tag::Active).unwrap();

                    // If our viewport contains the start or end of the active area,
                    // we are in the active area. We purposely do this first
                    // because our viewport is always larger than the active area.
                    // for (fingerprint.entries.items) |entry| {
                    //     if (entry.node == active_tl.node) break :match;
                    //     if (entry.node == active_br.node) break :match;
                    // }
                    for node in self.fingerprint.entries.iter().map(|e| e.node) {
                        if core::ptr::eq(node, active_tl.node) {
                            break 'match_blk;
                        }
                        if core::ptr::eq(node, active_br.node) {
                            break 'match_blk;
                        }
                    }
                }

                // No change
                // return false;
                return Ok(false);
            }
        }

        // PORT NOTE: `9659167ec` DELETED the `errdefer { fingerprint.deinit();
        // self.fingerprint = null; }`. The fingerprint is now permanent storage
        // owned by `ViewportSearch`, so a later failure inside this function
        // must NOT free it — it is simply left holding the new viewport. The
        // `fail!` macro this replaces is gone with it.

        // If our active area was set as dirty, we always unset it here
        // because we're re-searching now.
        // if (self.active_dirty) |*v| v.* = false;
        if self.active_dirty.is_some() {
            self.active_dirty = Some(false);
        }

        // Clear our previous sliding window
        // self.window.clearAndRetainCapacity();
        self.window.clear_and_retain_capacity();

        // const entries = fingerprint.entries.items;
        //
        // PORT NOTE (oracle `9659167ec` Step 2 / area D §4.3): upstream binds
        // the items slice ONCE and iterates it while `self.window.append*`
        // mutates a sibling field. The previous rustty shape snapshotted the
        // node pointers into an owned `Vec<*mut Node>` — sound, but it
        // allocates on every changed update, which is precisely the allocation
        // this commit exists to remove. We instead re-index
        // `self.fingerprint.entries` on each access: every borrow of the
        // fingerprint ends before the next `&mut self.window` call, so the two
        // disjoint fields are never borrowed simultaneously and no snapshot
        // exists to be invalidated. `.port/phase-3/mirihar-fingerprint-update`
        // shows a held `&[Entry]` snapshot is UB under both Stacked and Tree
        // Borrows.
        let entries_len = self.fingerprint.entries.len();

        // const overlap_len = self.window.needle.len -| 1;
        //
        // PORT NOTE (oracle 5bc6588e4, `P3-W1 T5`): SATURATING, per
        // `.port/phase-3/REVIEW_CLASSES.md` §1 — `-|` is `saturating_sub`, never
        // a plain `-`. `ViewportSearch` has no empty-needle early return (unlike
        // `ActiveSearch`), so an empty needle reaches here and `len - 1` would
        // panic in debug and WRAP to `usize::MAX` in release, turning the
        // overlap break into "never break".
        let overlap_len = self.window.needle.len().saturating_sub(1);
        debug_assert!(
            self.window.needle.is_empty() || overlap_len == self.window.needle.len() - 1,
            "saturation may only fire for the EMPTY needle; if it ever fires for \
             a non-empty one, a real invariant is being absorbed silently"
        );

        // Add enough overlap to cover needle.len - 1 bytes (if it
        // exists) so we can cover the overlap. We only do this for the
        // soft-wrapped prior pages.
        // var node_ = entries[0].node.prev;
        // var added: usize = 0;
        let mut node_: *mut Node = unsafe { (*self.fingerprint.entries[0].node).prev };
        let mut added: usize = 0;
        // while (node_) |node| : (node_ = node.prev) {
        while !node_.is_null() {
            let node = node_;
            // We could be more accurate here and count bytes since the
            // last wrap but its complicated and unlikely multiple pages
            // wrap so this should be fine.
            //
            // INSPECT, NOT RESTORE: one preserved page covers both the wrap
            // check and the formatting (oracle 0d015b2fc).
            // const appended = try self.window.appendIfWrapped(node) orelse break;
            // added += appended.content_len;
            // if (added >= self.window.needle.len - 1) break;
            let appended = match self.window.append_if_wrapped(node)? {
                Some(v) => v,
                None => break,
            };
            added += appended.content_len;
            if added >= overlap_len {
                break;
            }

            node_ = unsafe { (*node).prev };
        }

        // We can use our fingerprint nodes to initialize our sliding
        // window, since we already traversed the viewport once.
        // for (fingerprint.entries) |entry| {
        //     end_append = try self.window.append(entry.node);
        // }
        // var end_append: SlidingWindow.AppendResult = undefined;
        let mut end_append = crate::search::sliding_window::AppendResult {
            content_len: 0,
            last_row_wrapped: false,
        };
        for i in 0..entries_len {
            // The fingerprint borrow ends on this line; the `&mut self.window`
            // borrow below is a disjoint field. Re-indexed each iteration.
            let node = self.fingerprint.entries[i].node;
            end_append = self.window.append(node)?;
        }

        // Add any trailing overlap as well.
        // trailing: {
        'trailing: {
            // const end: *PageList.List.Node = entries[entries.len - 1].node;
            let end: *mut Node = self.fingerprint.entries[entries_len - 1].node;
            // if (!end_append.last_row_wrapped) break :trailing;
            //
            // INSPECT, NOT RESTORE: the wrap flag is the COPY taken during the
            // append above, so the tail page is never re-read (oracle 0d015b2fc).
            if !end_append.last_row_wrapped {
                break 'trailing;
            }

            // node_ = end.next;
            // added = 0;
            node_ = unsafe { (*end).next };
            added = 0;
            // while (node_) |node| : (node_ = node.next) {
            while !node_.is_null() {
                let node = node_;
                // const appended = try self.window.append(node);
                // added += appended.content_len;
                // if (added >= self.window.needle.len - 1) break;
                let appended = self.window.append(node)?;
                added += appended.content_len;
                if added >= overlap_len {
                    break;
                }

                // If this row doesn't wrap, then we can quit
                // if (!appended.last_row_wrapped) break;
                if !appended.last_row_wrapped {
                    break;
                }

                node_ = unsafe { (*node).next };
            }
        }

        // return true;
        Ok(true)
    }

    /// Find the next match for the needle in the active area. This returns
    /// null when there are no more matches.
    // pub fn next(self: *ViewportSearch) ?FlattenedHighlight {
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<FlattenedRef<'_>> {
        // return self.window.next();
        self.window.next()
    }
}

/// Viewport fingerprint so we can detect when the viewport moves.
pub struct Fingerprint {
    /// The node addresses and generations that make up the viewport. We
    /// can't safely dereference cached node pointers because the nodes may
    /// be invalid, so equality only compares these captured values.
    // entries: std.ArrayListUnmanaged(Entry) = .empty,
    //
    // PORT NOTE (parity tail S7-6): an allocator-bound list. `.empty` is on the
    // Rust heap handle and holds nothing; `update` binds it to the allocator it
    // is given (the search's), as the unmanaged list takes it per call.
    pub entries: crate::alloc::AVec<Entry>,
}

/// `ArrayListUnmanaged(Entry)`'s `init_capacity` (`@sizeOf(Entry)` is 16).
const ENTRY_INIT_CAPACITY: usize = crate::zig_list::init_capacity(16);

// fingerprint: Fingerprint = .{},
impl Default for Fingerprint {
    fn default() -> Fingerprint {
        Fingerprint {
            entries: crate::alloc::AVec::new_in(crate::alloc::VtAllocator::GLOBAL),
        }
    }
}

// const Entry = struct {
//     node: *PageList.List.Node,
//     serial: u64,
// };
#[derive(Clone, Copy)]
pub struct Entry {
    pub node: *mut Node,
    pub serial: u64,
}

impl Fingerprint {
    // pub fn deinit(self: *Fingerprint, alloc: Allocator) void {
    //     self.entries.deinit(alloc);
    // }
    //
    // PORT NOTE: the list frees through the allocator it is bound to (checked
    // against `alloc`) and is left empty.
    pub fn deinit(&mut self, alloc: Allocator) {
        if self.entries.capacity() != 0 {
            self.entries.allocator().debug_assert_same(alloc);
        }
        self.entries = crate::alloc::AVec::new_in(alloc);
    }

    // pub fn clearAndRetainCapacity(self: *Fingerprint) void {
    //     self.entries.clearRetainingCapacity();
    // }
    pub fn clear_and_retain_capacity(&mut self) {
        self.entries.clear();
    }

    /// Update this fingerprint to match the current viewport.
    ///
    /// Returns true if the viewport changed. The viewport can have at
    /// most one entry per visible row, so we reserve that upper bound
    /// before mutating the retained buffer. This lets us rebuild in one
    /// pass while still preserving the old fingerprint if allocation
    /// fails.
    // pub fn update(self: *Fingerprint, alloc: Allocator, pages: *PageList) Allocator.Error!bool {
    //
    // PORT NOTE (oracle `9659167ec` then `bc8bb6c0f`): this is `bc8bb6c0f`'s
    // shape, NOT `9659167ec`'s. `9659167ec` snapshotted `const old =
    // self.entries.items`, called `clearRetainingCapacity`, and then read
    // `old[i]` while `appendAssumeCapacity` wrote into the same storage;
    // `bc8bb6c0f` reverted that as UB. The Rust translation of that snapshot is
    // UB too — but only if you translate it faithfully as a REFERENCE. See
    // `.port/phase-3/mirihar-fingerprint-update`: the obvious raw-pointer
    // translation is Miri-CLEAN under both Stacked and Tree Borrows and would
    // have made the control vacuous.
    pub fn update(&mut self, alloc: Allocator, pages: &PageList) -> Result<bool, OutOfMemory> {
        // try self.entries.ensureTotalCapacity(alloc, pages.rows);
        //
        // MUST STAY FIRST. Reserving before any mutation is what makes an
        // allocation failure preserve the OLD fingerprint; moving it after the
        // first write silently breaks that contract.
        crate::alloc::bind(&mut self.entries, alloc);
        crate::zig_list::ensure_total_capacity(
            &mut self.entries,
            pages.rows as usize,
            ENTRY_INIT_CAPACITY,
        )?;

        // const old_len = self.entries.items.len;
        let old_len = self.entries.len();
        // var changed = false;
        let mut changed = false;

        // var i: usize = 0;
        // var it = pages.pageIterator(.right_down, .{ .viewport = .{} }, null);
        // while (it.next()) |chunk| : (i += 1) { ... }
        //
        // INSPECT, NOT RESTORE: only the node's generation stamp is read, which
        // lives in the pagelist Node itself (never in the page mapping), so a
        // compressed page must NOT be restored to fingerprint the viewport.
        let mut i: usize = 0;
        let mut it = pages.page_iterator(
            IterDirection::RightDown,
            point::Point::Viewport(point::Coordinate::default()),
            None,
        );
        while let Some(chunk) = it.next() {
            // const entry: Entry = .{ .node = chunk.node, .serial = chunk.node.serial };
            let entry = Entry {
                node: chunk.node,
                serial: unsafe { (*chunk.node).serial },
            };
            // if (!changed) {
            //     changed = i >= old_len or
            //         self.entries.items[i].node != entry.node or
            //         self.entries.items[i].serial != entry.serial;
            // }
            if !changed {
                changed = i >= old_len
                    || !core::ptr::eq(self.entries[i].node, entry.node)
                    || self.entries[i].serial != entry.serial;
            }

            // if (i < self.entries.items.len) {
            //     self.entries.items[i] = entry;
            // } else {
            //     self.entries.appendAssumeCapacity(entry);
            // }
            if i < self.entries.len() {
                self.entries[i] = entry;
            } else {
                // `appendAssumeCapacity`: `ensureTotalCapacity` above guarantees
                // room for `pages.rows` entries and the viewport can have at
                // most one entry per visible row, so this never reallocates.
                debug_assert!(self.entries.len() < self.entries.capacity());
                self.entries.push(entry);
            }

            i += 1;
        }

        // self.entries.shrinkRetainingCapacity(i);
        // return changed or i != old_len;
        self.entries.truncate(i);
        Ok(changed || i != old_len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::{DynamicPalette, DynamicRGB};
    use crate::kitty;
    use crate::modes as modespkg;
    use crate::point::Point;
    use crate::screen::Screen;
    use crate::terminal::{Colors, Options as TerminalOptions, ScrollViewport, Terminal};

    // PORT NOTE: see active.rs — supplies the Zig `Options` field defaults the
    // reference relies on implicitly.
    fn terminal_opts(cols: u16, rows: u16) -> TerminalOptions {
        TerminalOptions {
            cols,
            rows,
            max_scrollback_bytes: Some(10_000),
            max_scrollback_lines: None,
            colors: Colors {
                background: DynamicRGB::UNSET,
                foreground: DynamicRGB::UNSET,
                cursor: DynamicRGB::UNSET,
                palette: DynamicPalette::default_palette(),
            },
            default_modes: modespkg::ModePacked::default(),
            default_cursor_style: crate::screen::CursorStyle::Block,
            default_cursor_blink: Some(false),
            kitty_image_storage_limit: 320 * 1000 * 1000,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        }
    }

    // test "simple search"
    #[test]
    fn simple_search() {
        let alloc = Allocator;
        let mut t: Terminal = Terminal::init(alloc, terminal_opts(10, 10)).unwrap();
        let active_ptr: *mut Screen = t.screens.active;

        let mut s = t.vt_stream();
        s.next_slice(b"Fizz\r\nBuzz\r\nFizz\r\nBang");

        // var search: ViewportSearch = try .init(alloc, "Fizz");
        let mut search: ViewportSearch = ViewportSearch::init(alloc, b"Fizz").unwrap();
        // try testing.expect(try search.update(&t.screens.active.pages));
        // SB: read the searcher-shared active screen via raw `active_ptr` per-use, never
        // a `&mut Screen` held across `search.update`/`next_slice` mutations (see screen.rs).
        assert!(search.update(unsafe { &(*active_ptr).pages }).unwrap());

        // Viewport contains active so update should always re-search.
        // try testing.expect(try search.update(&t.screens.active.pages));
        assert!(search.update(unsafe { &(*active_ptr).pages }).unwrap());

        {
            let h = search.next().unwrap();
            let sel = h.untracked();
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 0, y: 0 })),
                unsafe {
                    (*active_ptr)
                        .pages
                        .point_from_pin(point::Tag::Active, sel.start)
                }
            );
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 3, y: 0 })),
                unsafe {
                    (*active_ptr)
                        .pages
                        .point_from_pin(point::Tag::Active, sel.end)
                }
            );
        }
        {
            let h = search.next().unwrap();
            let sel = h.untracked();
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 0, y: 2 })),
                unsafe {
                    (*active_ptr)
                        .pages
                        .point_from_pin(point::Tag::Active, sel.start)
                }
            );
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 3, y: 2 })),
                unsafe {
                    (*active_ptr)
                        .pages
                        .point_from_pin(point::Tag::Active, sel.end)
                }
            );
        }
        assert!(search.next().is_none());

        search.deinit();
        s.deinit();
        t.deinit(alloc);
    }

    /// `P3-W1 T5` (oracle 5bc6588e4). `ViewportSearch` has NO empty-needle early
    /// return, so an empty needle reaches the overlap comparison. Pre-fix,
    /// `self.window.needle.len() - 1` panicked in debug and wrapped to
    /// `usize::MAX` in release, which turns "break once we have enough overlap"
    /// into "never break" and walks the whole page list. The value asserted is
    /// that the search completes and yields no matches.
    #[test]
    fn viewport_empty_needle_saturates_the_overlap_length() {
        let alloc = Allocator;
        let mut t: Terminal = Terminal::init(alloc, terminal_opts(10, 10)).unwrap();
        let active_ptr: *mut Screen = t.screens.active;
        unsafe { (*active_ptr).test_write_string(b"hello\nworld\n").unwrap() };

        let mut search: ViewportSearch = ViewportSearch::init(alloc, b"").unwrap();
        let _ = search.update(unsafe { &(*active_ptr).pages }).unwrap();
        assert!(search.next().is_none());

        search.deinit();
        t.deinit(alloc);
    }

    /// PORTS: oracle c243d89bd
    /// `test "fingerprint detects node generation changes"`
    #[test]
    fn fingerprint_detects_node_generation_changes() {
        // const alloc = testing.allocator;
        let alloc = Allocator;
        // var t: Terminal = try .init(alloc, .{ .cols = 10, .rows = 10 });
        // defer t.deinit(alloc);
        let mut t: Terminal = Terminal::init(alloc, terminal_opts(10, 10)).unwrap();
        let active_ptr: *mut Screen = t.screens.active;

        // var search: ViewportSearch = try .init(alloc, "needle");
        // defer search.deinit();
        let mut search: ViewportSearch = ViewportSearch::init(alloc, b"needle").unwrap();
        // search.active_dirty = false;
        search.active_dirty = Some(false);

        // try testing.expect(try search.update(&t.screens.active.pages));
        // try testing.expect(!try search.update(&t.screens.active.pages));
        assert!(search.update(unsafe { &(*active_ptr).pages }).unwrap());
        assert!(!search.update(unsafe { &(*active_ptr).pages }).unwrap());

        // const old_entry = search.fingerprint.entries.items[0];
        let old_entry = search.fingerprint.entries[0];
        // const node = t.screens.active.pages.pages.first.?;
        let node: *mut Node = unsafe { (*active_ptr).pages.pages.first };
        // try testing.expectEqual(node, old_entry.node);
        assert!(core::ptr::eq(node, old_entry.node));
        // const original_serial = node.serial;
        // defer node.serial = original_serial;
        // node.serial += 1;
        let original_serial = unsafe { (*node).serial };
        unsafe { (*node).serial += 1 };

        // The viewport still contains the same pointer, but its generation has
        // changed, so the cached window must be rebuilt.
        // try testing.expect(try search.update(&t.screens.active.pages));
        assert!(search.update(unsafe { &(*active_ptr).pages }).unwrap());
        // const new_entry = search.fingerprint.entries.items[0];
        let new_entry = search.fingerprint.entries[0];
        // try testing.expectEqual(old_entry.node, new_entry.node);
        assert!(core::ptr::eq(old_entry.node, new_entry.node));
        // try testing.expect(old_entry.serial != new_entry.serial);
        assert_ne!(old_entry.serial, new_entry.serial);

        // defer node.serial = original_serial; (restore before the PageList's
        // debug integrity assertions run in `deinit`)
        unsafe { (*node).serial = original_serial };

        search.deinit();
        t.deinit(alloc);
    }

    // test "clear screen and search"
    #[test]
    fn clear_screen_and_search() {
        let alloc = Allocator;
        let mut t: Terminal = Terminal::init(alloc, terminal_opts(10, 10)).unwrap();
        let active_ptr: *mut Screen = t.screens.active;

        let mut s = t.vt_stream();
        s.next_slice(b"Fizz\r\nBuzz\r\nFizz\r\nBang");

        let mut search: ViewportSearch = ViewportSearch::init(alloc, b"Fizz").unwrap();
        {
            // SB: read the searcher-shared active screen via raw `active_ptr` per-use, never
            // a `&mut Screen` held across `search.update`/`next_slice` mutations (see screen.rs).
            assert!(search.update(unsafe { &(*active_ptr).pages }).unwrap());
        }

        // s.nextSlice("\x1b[2J"); // Clear screen
        s.next_slice(b"\x1b[2J"); // Clear screen
                                  // s.nextSlice("\x1b[H"); // Move cursor home
        s.next_slice(b"\x1b[H"); // Move cursor home
                                 // s.nextSlice("Buzz\r\nFizz\r\nBuzz");
        s.next_slice(b"Buzz\r\nFizz\r\nBuzz");
        // SB: read the searcher-shared active screen via raw `active_ptr` per-use, never
        // a `&mut Screen` held across `search.update`/`next_slice` mutations (see screen.rs).
        assert!(search.update(unsafe { &(*active_ptr).pages }).unwrap());

        {
            let h = search.next().unwrap();
            let sel = h.untracked();
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 0, y: 1 })),
                unsafe {
                    (*active_ptr)
                        .pages
                        .point_from_pin(point::Tag::Active, sel.start)
                }
            );
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 3, y: 1 })),
                unsafe {
                    (*active_ptr)
                        .pages
                        .point_from_pin(point::Tag::Active, sel.end)
                }
            );
        }
        assert!(search.next().is_none());

        search.deinit();
        s.deinit();
        t.deinit(alloc);
    }

    // test "clear screen and search dirty tracking"
    #[test]
    fn clear_screen_and_search_dirty_tracking() {
        let alloc = Allocator;
        let mut t: Terminal = Terminal::init(alloc, terminal_opts(10, 10)).unwrap();
        let active_ptr: *mut Screen = t.screens.active;

        let mut s = t.vt_stream();
        s.next_slice(b"Fizz\r\nBuzz\r\nFizz\r\nBang");

        let mut search: ViewportSearch = ViewportSearch::init(alloc, b"Fizz").unwrap();

        // Turn on dirty tracking
        // search.active_dirty = false;
        search.active_dirty = Some(false);

        // SB: read the searcher-shared active screen via raw `active_ptr` per-use, never
        // a `&mut Screen` held across `search.update`/`next_slice` mutations (see screen.rs).

        // Should update since we've never searched before
        assert!(search.update(unsafe { &(*active_ptr).pages }).unwrap());

        // Should not update since nothing changed
        // try testing.expect(!try search.update(&t.screens.active.pages));
        assert!(!search.update(unsafe { &(*active_ptr).pages }).unwrap());

        s.next_slice(b"\x1b[2J"); // Clear screen
        s.next_slice(b"\x1b[H"); // Move cursor home
        s.next_slice(b"Buzz\r\nFizz\r\nBuzz");

        // Should still not update since active area isn't dirty
        assert!(!search.update(unsafe { &(*active_ptr).pages }).unwrap());

        // Mark
        // search.active_dirty = true;
        search.active_dirty = Some(true);
        // try testing.expect(try search.update(&t.screens.active.pages));
        assert!(search.update(unsafe { &(*active_ptr).pages }).unwrap());

        {
            let h = search.next().unwrap();
            let sel = h.untracked();
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 0, y: 1 })),
                unsafe {
                    (*active_ptr)
                        .pages
                        .point_from_pin(point::Tag::Active, sel.start)
                }
            );
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 3, y: 1 })),
                unsafe {
                    (*active_ptr)
                        .pages
                        .point_from_pin(point::Tag::Active, sel.end)
                }
            );
        }
        assert!(search.next().is_none());

        search.deinit();
        s.deinit();
        t.deinit(alloc);
    }

    // test "history search, no active area"
    #[cfg_attr(
        all(miri, debug_assertions),
        ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
    )]
    #[test]
    fn history_search_no_active_area() {
        let alloc = Allocator;
        let mut t: Terminal = Terminal::init(alloc, terminal_opts(10, 2)).unwrap();
        let active_ptr: *mut Screen = t.screens.active;

        let mut s = t.vt_stream();

        // Fill up first page
        // const first_page_rows = t.screens.active.pages.pages.first.?.data.capacity.rows;
        let first_page_rows =
            unsafe { crate::pagelist::Node::capacity((*active_ptr).pages.pages.first).rows };
        // s.nextSlice("Fizz\r\n");
        s.next_slice(b"Fizz\r\n");
        // for (1..first_page_rows - 1) |_| s.nextSlice("\r\n");
        for _ in 1..first_page_rows - 1 {
            s.next_slice(b"\r\n");
        }
        assert!(core::ptr::eq(
            unsafe { (*active_ptr).pages.pages.first },
            unsafe { (*active_ptr).pages.pages.last }
        ));

        // Create second page
        s.next_slice(b"\r\n");
        assert!(!core::ptr::eq(
            unsafe { (*active_ptr).pages.pages.first },
            unsafe { (*active_ptr).pages.pages.last }
        ));
        // s.nextSlice("Buzz\r\nFizz");
        s.next_slice(b"Buzz\r\nFizz");

        // t.scrollViewport(.top);
        s.deinit();
        t.scroll_viewport(ScrollViewport::Top);

        // SB: read the searcher-shared active screen via raw `active_ptr` per-use, never
        // a `&mut Screen` held across `search.update`/`next_slice` mutations (see screen.rs).
        let mut search: ViewportSearch = ViewportSearch::init(alloc, b"Fizz").unwrap();
        assert!(search.update(unsafe { &(*active_ptr).pages }).unwrap());

        {
            let h = search.next().unwrap();
            let sel = h.untracked();
            assert_eq!(
                Some(Point::Screen(point::Coordinate { x: 0, y: 0 })),
                unsafe {
                    (*active_ptr)
                        .pages
                        .point_from_pin(point::Tag::Screen, sel.start)
                }
            );
            assert_eq!(
                Some(Point::Screen(point::Coordinate { x: 3, y: 0 })),
                unsafe {
                    (*active_ptr)
                        .pages
                        .point_from_pin(point::Tag::Screen, sel.end)
                }
            );
        }
        assert!(search.next().is_none());

        // Viewport doesn't contain active
        // try testing.expect(!try search.update(&t.screens.active.pages));
        assert!(!search.update(unsafe { &(*active_ptr).pages }).unwrap());
        assert!(search.next().is_none());

        search.deinit();
        t.deinit(alloc);
    }
}
