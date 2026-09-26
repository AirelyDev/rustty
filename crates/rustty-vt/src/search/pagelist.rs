//! rustty-vt search/pagelist
//!
//! PORTS: ghostty src/terminal/search/pagelist.zig
//!
//! PORT STATUS: complete (vt-15). Faithful port of `PageListSearch`, which
//! searches a `PageList` in reverse order from a given node, feeding pages into
//! the sliding window on demand.

// const std = @import("std");
// const Allocator = std.mem.Allocator;
// const testing = std.testing;
// const terminal = @import("../main.zig");
// const point = terminal.point;
// const FlattenedHighlight = @import("../highlight.zig").Flattened;
// const Page = terminal.Page;
// const PageList = terminal.PageList;
// const Pin = PageList.Pin;
// const Selection = terminal.Selection;
// const Screen = terminal.Screen;
// const Terminal = @import("../Terminal.zig");
// const SlidingWindow = @import("sliding_window.zig").SlidingWindow;
use super::sliding_window::{Direction, SlidingWindow};
use crate::alloc::Allocator;
use crate::highlight::FlattenedRef;
use crate::pagelist::{Node, OutOfMemory, PageList, Pin};

/// Searches for a term in a PageList structure.
///
/// This searches in reverse order starting from the given node.
///
/// This assumes that nodes do not change contents. For nodes that change
/// contents, look at ActiveSearch, which is designed to re-search the active
/// area since it assumed to change. When integrating ActiveSearch with
/// PageListSearch, the caller should start the PageListSearch from the
/// returned node from ActiveSearch.update().
///
/// Concurrent access to a PageList or nodes in a PageList are not allowed,
/// so the caller should ensure that necessary locks are held. Each function
/// documents whether it accesses the PageList or not. For example, you can
/// safely call `next()` without holding a lock, but you must hold a lock
/// while calling `feed()`.
//
// PORT NOTE: `list` is a `*mut PageList` (the reference `*PageList`). The
// reference passes `&pages` and the searcher both reads it and calls
// `trackPin`/`untrackPin` (which mutate the pin list); a raw pointer mirrors
// that shared mutable aliasing without fighting the borrow checker.
pub struct PageListSearch {
    /// The list we're searching.
    pub list: *mut PageList,

    /// The sliding window of page contents and nodes to search.
    pub window: SlidingWindow,

    /// The tracked pin for our current position in the pagelist. This
    /// will always point to the CURRENT node we're searching from so that
    /// we can track if we move.
    pub pin: *mut Pin,
}

impl PageListSearch {
    /// Initialize the page list search. The needle is copied so it can
    /// be freed immediately.
    ///
    /// Accesses the PageList/Node so the caller must ensure it is safe
    /// to do so if there is any concurrent access.
    // pub fn init(
    //     alloc: Allocator,
    //     needle: []const u8,
    //     list: *PageList,
    //     start: *PageList.List.Node,
    // ) Allocator.Error!PageListSearch {
    // PORT NOTE: `list`/`start` are raw pointers (reference takes `*PageList`,
    // `*PageList.List.Node`); the body dereferences them, so the clippy lint is
    // allowed here, matching the established convention in `pagelist/pin.rs` etc.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn init(
        alloc: Allocator,
        needle: &[u8],
        list: *mut PageList,
        start: *mut Node,
    ) -> Result<PageListSearch, OutOfMemory> {
        // We put a tracked pin into the node that we're starting from.
        // By using a tracked pin, we can keep our pagelist references safe
        // because if the pagelist prunes pages, the tracked pin will
        // be moved somewhere safe.
        // const pin = try list.trackPin(.{
        //     .node = start,
        //     .y = start.data.size.rows - 1,
        //     .x = start.data.size.cols - 1,
        // });
        // errdefer list.untrackPin(pin);
        let pin = unsafe {
            (*list).track_pin(Pin {
                node: start,
                y: crate::pagelist::Node::rows(start) - 1,
                x: crate::pagelist::Node::cols(start) - 1,
                garbage: false,
            })?
        };

        // Create our sliding window we'll use for searching.
        // var window: SlidingWindow = try .init(alloc, .reverse, needle);
        // errdefer window.deinit();
        let mut window: SlidingWindow = match SlidingWindow::init(alloc, Direction::Reverse, needle)
        {
            Ok(w) => w,
            Err(e) => {
                unsafe { (*list).untrack_pin(pin) };
                return Err(e);
            }
        };

        // We always feed our initial page data into the window, because
        // we have the lock anyways and this lets our `pin` point to our
        // current node and feed to work properly.
        // _ = try window.append(start);
        match window.append(start) {
            Ok(_) => {}
            Err(e) => {
                window.deinit();
                unsafe { (*list).untrack_pin(pin) };
                return Err(e);
            }
        }

        // return .{
        //     .list = list,
        //     .window = window,
        //     .pin = pin,
        // };
        Ok(PageListSearch { list, window, pin })
    }

    /// Modifies the PageList (to untrack a pin) so the caller must ensure
    /// that it is safe to do so.
    // pub fn deinit(self: *PageListSearch) void {
    pub fn deinit(&mut self) {
        // self.window.deinit();
        self.window.deinit();
        // self.list.untrackPin(self.pin);
        unsafe { (*self.list).untrack_pin(self.pin) };
    }

    /// Release owned search buffers after the PageList itself has already
    /// been destroyed. Its pin belonged to the PageList pool and was freed
    /// with that list, so it must not be untracked here.
    ///
    /// PORTS: oracle@main search/pagelist.zig `deinitListInvalid` (73ac36fa5).
    // pub fn deinitListInvalid(self: *PageListSearch) void {
    pub fn deinit_list_invalid(&mut self) {
        // self.window.deinit();
        self.window.deinit();
    }

    /// Return the next match in the loaded page nodes. If this returns
    /// null then the PageList search needs to be fed the next node(s).
    /// Call, `feed` to do this.
    ///
    /// Beware that the selection returned may point to a node that
    /// is freed if the caller does not hold necessary locks on the
    /// PageList while searching. The pins should be validated prior to
    /// final use.
    ///
    /// This does NOT access the PageList, so it can be called without
    /// a lock held.
    // pub fn next(self: *PageListSearch) ?FlattenedHighlight {
    /// Rust-only test accounting (parity tail S7-6): see
    /// `SlidingWindow::owned_bytes`. The tracked pin is the TERMINAL's (N262)
    /// and is not counted.
    #[cfg(any(test, feature = "test-allocators"))]
    pub fn owned_bytes(&self) -> usize {
        self.window.owned_bytes()
    }

    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<FlattenedRef<'_>> {
        // return self.window.next();
        self.window.next()
    }

    /// Feed more data to the sliding window from the pagelist. This will
    /// feed enough data to cover at least one match (needle length) if it
    /// exists; this doesn't perform the search, it only feeds data.
    ///
    /// This accesses nodes in the PageList, so the caller must ensure
    /// it is safe to do so (i.e. hold necessary locks).
    ///
    /// This returns false if there is no more data to feed. This essentially
    /// means we've searched the entire pagelist.
    // pub fn feed(self: *PageListSearch) Allocator.Error!bool {
    pub fn feed(&mut self) -> Result<bool, OutOfMemory> {
        // If our pin becomes garbage it means wherever we were next
        // was reused and we can't make sense of our progress anymore.
        // It is effectively equivalent to reaching the end of the PageList.
        // if (self.pin.garbage) return false;
        if unsafe { (*self.pin).garbage } {
            return Ok(false);
        }

        // Add at least enough data to find a single match.
        // var rem = self.window.needle.len;
        let mut rem = self.window.needle.len();

        // Start at our previous node and then continue adding until we
        // get our desired amount of data.
        // var node_: ?*PageList.List.Node = self.pin.node.prev;
        // while (node_) |node| : (node_ = node.prev) {
        let mut node_: *mut Node = unsafe { (*(*self.pin).node).prev };
        while !node_.is_null() {
            let node = node_;
            // rem -|= (try self.window.append(node)).content_len;
            rem = rem.saturating_sub(self.window.append(node)?.content_len);

            // Move our tracked pin to the new node.
            // self.pin.node = node;
            // self.pin.y = node.rows() - 1;
            // self.pin.x = node.cols() - 1;
            // self.pin.garbage = false;
            //
            // The preceding page may be SHORTER (e.g. after a split), so the
            // retained bottom-right coordinates must be reset, not carried over
            // (oracle 5d8eb78b7).
            unsafe {
                (*self.pin).node = node;
                (*self.pin).y = crate::pagelist::Node::rows(node) - 1;
                (*self.pin).x = crate::pagelist::Node::cols(node) - 1;
                (*self.pin).garbage = false;
            };

            // if (rem == 0) break;
            if rem == 0 {
                break;
            }

            node_ = unsafe { (*node).prev };
        }

        // True if we fed any data.
        // return rem < self.window.needle.len;
        Ok(rem < self.window.needle.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::{DynamicPalette, DynamicRGB};
    use crate::kitty;
    use crate::modes as modespkg;
    use crate::pagelist::Options;
    use crate::point::{self, Point};
    use crate::screen::Screen;
    use crate::screen::SelectionString;
    use crate::selection::Selection;
    use crate::terminal::{Colors, Options as TerminalOptions, Terminal};

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
        // const alloc = testing.allocator;
        let alloc = Allocator;
        // var t: Terminal = try .init(alloc, .{ .cols = 10, .rows = 10 });
        let mut t: Terminal = Terminal::init(alloc, terminal_opts(10, 10)).unwrap();
        // defer t.deinit(alloc);
        let active_ptr: *mut Screen = t.screens.active;

        // var s = t.vtStream();
        let mut s = t.vt_stream();
        // defer s.deinit();
        // s.nextSlice("Fizz\r\nBuzz\r\nFizz\r\nBang");
        s.next_slice(b"Fizz\r\nBuzz\r\nFizz\r\nBang");

        // var search: PageListSearch = try .init(
        //     alloc,
        //     "Fizz",
        //     &t.screens.active.pages,
        //     t.screens.active.pages.pages.last.?,
        // );
        let active = unsafe { &mut *active_ptr };
        let mut search: PageListSearch =
            PageListSearch::init(alloc, b"Fizz", &mut active.pages, active.pages.pages.last)
                .unwrap();
        // defer search.deinit();

        {
            // const h = search.next().?;
            let h = search.next().unwrap();
            // const sel = h.untracked();
            let sel = h.untracked();
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 0, y: 2 })),
                active.pages.point_from_pin(point::Tag::Active, sel.start)
            );
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 3, y: 2 })),
                active.pages.point_from_pin(point::Tag::Active, sel.end)
            );
        }
        {
            let h = search.next().unwrap();
            let sel = h.untracked();
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 0, y: 0 })),
                active.pages.point_from_pin(point::Tag::Active, sel.start)
            );
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 3, y: 0 })),
                active.pages.point_from_pin(point::Tag::Active, sel.end)
            );
        }
        // try testing.expect(search.next() == null);
        assert!(search.next().is_none());

        // We should not be able to feed since we have one page
        // try testing.expect(!try search.feed());
        assert!(!search.feed().unwrap());

        search.deinit();
        s.deinit();
        t.deinit(alloc);
    }

    // test "feed multiple pages with matches"
    #[cfg_attr(
        all(miri, debug_assertions),
        ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
    )]
    #[test]
    fn feed_multiple_pages_with_matches() {
        let alloc = Allocator;
        let mut t: Terminal = Terminal::init(alloc, terminal_opts(10, 10)).unwrap();
        let active_ptr: *mut Screen = t.screens.active;

        let mut s = t.vt_stream();

        // Fill up first page
        // const first_page_rows = t.screens.active.pages.pages.first.?.data.capacity.rows;
        let first_page_rows =
            unsafe { crate::pagelist::Node::capacity((*active_ptr).pages.pages.first).rows };
        // for (0..first_page_rows - 1) |_| s.nextSlice("\r\n");
        for _ in 0..first_page_rows - 1 {
            s.next_slice(b"\r\n");
        }
        // s.nextSlice("Fizz");
        s.next_slice(b"Fizz");
        // try testing.expect(t.screens.active.pages.pages.first == t.screens.active.pages.pages.last);
        assert!(core::ptr::eq(
            unsafe { (*active_ptr).pages.pages.first },
            unsafe { (*active_ptr).pages.pages.last }
        ));

        // Create second page
        // s.nextSlice("\r\n");
        s.next_slice(b"\r\n");
        // try testing.expect(t.screens.active.pages.pages.first != t.screens.active.pages.pages.last);
        assert!(!core::ptr::eq(
            unsafe { (*active_ptr).pages.pages.first },
            unsafe { (*active_ptr).pages.pages.last }
        ));
        // s.nextSlice("Buzz\r\nFizz");
        s.next_slice(b"Buzz\r\nFizz");

        let active = unsafe { &mut *active_ptr };
        let mut search: PageListSearch =
            PageListSearch::init(alloc, b"Fizz", &mut active.pages, active.pages.pages.last)
                .unwrap();

        // First match on the last page
        // const sel1 = search.next();
        // try testing.expect(sel1 != null);
        let sel1 = search.next();
        assert!(sel1.is_some());
        // try testing.expect(search.next() == null);
        assert!(search.next().is_none());

        // Feed should succeed and load the first page
        // try testing.expect(try search.feed());
        assert!(search.feed().unwrap());

        // Now we should find the match on the first page
        // const sel2 = search.next();
        // try testing.expect(sel2 != null);
        let sel2 = search.next();
        assert!(sel2.is_some());
        // try testing.expect(search.next() == null);
        assert!(search.next().is_none());

        // No more pages to feed
        // try testing.expect(!try search.feed());
        assert!(!search.feed().unwrap());

        search.deinit();
        s.deinit();
        t.deinit(alloc);
    }

    // test "feed multiple pages no matches"
    #[cfg_attr(
        all(miri, debug_assertions),
        ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
    )]
    #[test]
    fn feed_multiple_pages_no_matches() {
        let alloc = Allocator;
        let mut t: Terminal = Terminal::init(alloc, terminal_opts(10, 10)).unwrap();
        let active_ptr: *mut Screen = t.screens.active;

        let mut s = t.vt_stream();

        // Fill up first page
        let first_page_rows =
            unsafe { crate::pagelist::Node::capacity((*active_ptr).pages.pages.first).rows };
        for _ in 0..first_page_rows - 1 {
            s.next_slice(b"\r\n");
        }
        // s.nextSlice("Hello");
        s.next_slice(b"Hello");

        // Create second page
        s.next_slice(b"\r\n");
        assert!(!core::ptr::eq(
            unsafe { (*active_ptr).pages.pages.first },
            unsafe { (*active_ptr).pages.pages.last }
        ));
        // s.nextSlice("World");
        s.next_slice(b"World");

        let active = unsafe { &mut *active_ptr };
        let mut search: PageListSearch =
            PageListSearch::init(alloc, b"Nope", &mut active.pages, active.pages.pages.last)
                .unwrap();

        // No matches on last page
        // try testing.expect(search.next() == null);
        assert!(search.next().is_none());

        // Feed first page
        // try testing.expect(try search.feed());
        assert!(search.feed().unwrap());

        // Still no matches
        assert!(search.next().is_none());

        // No more pages
        assert!(!search.feed().unwrap());

        search.deinit();
        s.deinit();
        t.deinit(alloc);
    }

    // test "feed iteratively through multiple matches"
    #[cfg_attr(
        all(miri, debug_assertions),
        ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
    )]
    #[test]
    fn feed_iteratively_through_multiple_matches() {
        let alloc = Allocator;
        let mut t: Terminal = Terminal::init(alloc, terminal_opts(80, 24)).unwrap();
        let active_ptr: *mut Screen = t.screens.active;

        let mut s = t.vt_stream();

        let first_page_rows =
            unsafe { crate::pagelist::Node::capacity((*active_ptr).pages.pages.first).rows };

        // Fill first page with a match at the end
        for _ in 0..first_page_rows - 1 {
            s.next_slice(b"\r\n");
        }
        // s.nextSlice("Page1Test");
        s.next_slice(b"Page1Test");
        assert!(core::ptr::eq(
            unsafe { (*active_ptr).pages.pages.first },
            unsafe { (*active_ptr).pages.pages.last }
        ));

        // Create second page with a match
        s.next_slice(b"\r\n");
        assert!(!core::ptr::eq(
            unsafe { (*active_ptr).pages.pages.first },
            unsafe { (*active_ptr).pages.pages.last }
        ));
        // s.nextSlice("Page2Test");
        s.next_slice(b"Page2Test");

        let active = unsafe { &mut *active_ptr };
        let mut search: PageListSearch =
            PageListSearch::init(alloc, b"Test", &mut active.pages, active.pages.pages.last)
                .unwrap();

        // Match on page 2
        assert!(search.next().is_some());
        assert!(search.next().is_none());

        // Feed page 1
        assert!(search.feed().unwrap());
        assert!(search.next().is_some());
        assert!(search.next().is_none());

        // No more pages
        assert!(!search.feed().unwrap());

        search.deinit();
        s.deinit();
        t.deinit(alloc);
    }

    // test "feed with match spanning page boundary"
    #[cfg_attr(
        all(miri, debug_assertions),
        ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
    )]
    #[test]
    fn feed_with_match_spanning_page_boundary() {
        let alloc = Allocator;
        let mut t: Terminal = Terminal::init(alloc, terminal_opts(80, 24)).unwrap();
        let active_ptr: *mut Screen = t.screens.active;

        let mut s = t.vt_stream();

        let first_page_rows =
            unsafe { crate::pagelist::Node::capacity((*active_ptr).pages.pages.first).rows };

        // Fill first page ending with "Te"
        for _ in 0..first_page_rows - 1 {
            s.next_slice(b"\r\n");
        }
        // for (0..t.screens.active.pages.cols - 2) |_| s.nextSlice("x");
        for _ in 0..unsafe { (*active_ptr).pages.cols } - 2 {
            s.next_slice(b"x");
        }
        // s.nextSlice("Te");
        s.next_slice(b"Te");
        assert!(core::ptr::eq(
            unsafe { (*active_ptr).pages.pages.first },
            unsafe { (*active_ptr).pages.pages.last }
        ));

        // Second page starts with "st"
        // s.nextSlice("st");
        s.next_slice(b"st");
        assert!(!core::ptr::eq(
            unsafe { (*active_ptr).pages.pages.first },
            unsafe { (*active_ptr).pages.pages.last }
        ));

        let active = unsafe { &mut *active_ptr };
        let mut search: PageListSearch =
            PageListSearch::init(alloc, b"Test", &mut active.pages, active.pages.pages.last)
                .unwrap();

        // No complete match on last page alone (only has "st")
        assert!(search.next().is_none());

        // Feed first page - this should give us enough data to find "Test"
        assert!(search.feed().unwrap());

        // Should find the spanning match
        // const h = search.next().?;
        let h = search.next().unwrap();
        // const sel = h.untracked();
        let sel = h.untracked();
        // try testing.expect(sel.start.node != sel.end.node);
        assert!(!core::ptr::eq(sel.start.node, sel.end.node));
        {
            // const str = try t.screens.active.selectionString(
            //     alloc,
            //     .{ .sel = .init(sel.start, sel.end, false) },
            // );
            // defer alloc.free(str);
            let str = active
                .selection_string(
                    alloc,
                    SelectionString {
                        sel: Selection::init(sel.start, sel.end, false),
                        trim: true,
                    },
                )
                .unwrap();
            // try testing.expectEqualStrings(str, "Test");
            assert_eq!(str.as_slice(), b"Test");
        }

        // No more matches
        assert!(search.next().is_none());

        // No more pages
        assert!(!search.feed().unwrap());

        search.deinit();
        s.deinit();
        t.deinit(alloc);
    }

    /// PORTS: oracle 0d015b2fc
    /// `test "compressed history match spanning page boundary remains compressed"`
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): TIMEOUT in BOTH Miri lanes — re-measured per test in single-worker isolation at a 6-minute bound with `-C debug-assertions=off`, so removing the integrity assertions does not rescue it; the cost is page-mapping/LZ4 volume, not assertion overhead. Covered natively + by ASan."
    )]
    #[test]
    fn compressed_history_match_spanning_page_boundary_remains_compressed() {
        use crate::pagelist::compress::CompressionMode;
        use crate::pagelist::{Node as PlNode, Storage};
        let alloc = Allocator;
        let mut t: Terminal = Terminal::init(
            alloc,
            TerminalOptions {
                max_scrollback_bytes: Some(10 * 1024 * 1024),
                max_scrollback_lines: None,
                ..terminal_opts(80, 24)
            },
        )
        .unwrap();
        let active_ptr: *mut Screen = t.screens.active;
        // PORT NOTE (WS19 T6) — RE-DERIVE, NEVER CACHE. `Terminal::active_mut()`
        // (`&mut *self.screens.active`) is a Unique retag over the whole `Screen`
        // allocation, so it pops any raw pointer derived from that allocation
        // earlier; reading through a cached one afterwards is Stacked-Borrows UB
        // (memory: rust-mut-args-strongly-protected). This test used to cache it
        // across its `vt_stream()`/search writes. The violation was real and
        // deterministic, and had never been observed because a blanket
        // "multi-page PageList intractable" annotation kept the test out of Miri
        // entirely — found the moment the G6c (`-C debug-assertions=off`) lane
        // made it runnable. Test-code only: no production path caches this.
        let pages =
            || -> *mut crate::pagelist::PageList { unsafe { &raw mut (*active_ptr).pages } };

        let mut stream = t.vt_stream();

        let first = unsafe { (*pages()).pages.first };
        let first_page_rows = unsafe { PlNode::capacity(first) }.rows;

        // Put half the needle on either side of a soft-wrapped page boundary.
        for _ in 0..first_page_rows - 1 {
            stream.next_slice(b"\r\n");
        }
        for _ in 0..unsafe { (*pages()).cols } - 2 {
            stream.next_slice(b"x");
        }
        stream.next_slice(b"Test");
        let second = unsafe { (*first).next };
        assert!(core::ptr::eq(second, unsafe { (*pages()).pages.last }));

        // Move both matching pages completely into history. The third page holds
        // the active area, making the first two eligible for compression.
        while core::ptr::eq(unsafe { (*pages()).pages.last }, second) {
            stream.next_slice(b"\r\n");
        }
        for _ in 0..unsafe { (*pages()).rows } {
            stream.next_slice(b"\r\n");
        }
        assert!(!core::ptr::eq(
            unsafe { (*pages()).get_top_left(point::Tag::Active) }.node,
            second
        ));

        let _ = unsafe { (*pages()).compress(CompressionMode::Full) };
        assert_eq!(unsafe { PlNode::storage(first) }, Storage::Compressed);
        assert_eq!(unsafe { PlNode::storage(second) }, Storage::Compressed);
        let compressed = unsafe { (*pages()).memory_stats() };
        assert!(compressed.compressed_pages >= 2);

        let mut search: PageListSearch =
            PageListSearch::init(alloc, b"Test", pages(), unsafe { (*pages()).pages.last })
                .unwrap();

        // Drain and feed incrementally until the boundary match is available. The
        // exact number of feeds depends on how much blank-page text the formatter
        // trims, so bound progress by the number of pages rather than encoding
        // that implementation detail in the test.
        //
        // PORT NOTE (parity tail S7-6): `next()` borrows the window, so the
        // match is kept as an owned clone across the later `feed` calls (the
        // reference keeps its shallow copy, which the window never overwrites
        // after the `break`).
        let mut found: Option<crate::highlight::Flattened> = None;
        for _ in 0..unsafe { (*pages()).total_pages() } + 1 {
            if let Some(m) = search.next() {
                found = Some(m.clone(alloc).unwrap());
                break;
            }
            if !search.feed().unwrap() {
                break;
            }
        }
        let found = found.expect("cross-page match in compressed history");

        let m = found.untracked();
        assert!(core::ptr::eq(m.start.node, first));
        assert!(core::ptr::eq(m.end.node, second));
        assert_eq!(m.start.x, unsafe { (*pages()).cols } - 2);
        assert_eq!(m.end.x, 1);

        // Search owns formatted text and coordinate maps, not page snapshots. The
        // original raw mappings therefore remain discarded after matching.
        assert_eq!(unsafe { (*pages()).memory_stats() }, compressed);
        assert_eq!(unsafe { PlNode::storage(first) }, Storage::Compressed);
        assert_eq!(unsafe { PlNode::storage(second) }, Storage::Compressed);

        search.deinit();
        stream.deinit();
        t.deinit(alloc);
    }

    // test "feed with match spanning page boundary with newline"
    #[cfg_attr(
        all(miri, debug_assertions),
        ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
    )]
    #[test]
    fn feed_with_match_spanning_page_boundary_with_newline() {
        let alloc = Allocator;
        let mut t: Terminal = Terminal::init(alloc, terminal_opts(80, 24)).unwrap();
        let active_ptr: *mut Screen = t.screens.active;

        let mut s = t.vt_stream();

        let first_page_rows =
            unsafe { crate::pagelist::Node::capacity((*active_ptr).pages.pages.first).rows };

        // Fill first page ending with "Te"
        for _ in 0..first_page_rows - 1 {
            s.next_slice(b"\r\n");
        }
        for _ in 0..unsafe { (*active_ptr).pages.cols } - 2 {
            s.next_slice(b"x");
        }
        s.next_slice(b"Te");
        assert!(core::ptr::eq(
            unsafe { (*active_ptr).pages.pages.first },
            unsafe { (*active_ptr).pages.pages.last }
        ));

        // Second page starts with "st"
        // s.nextSlice("\r\n");
        s.next_slice(b"\r\n");
        assert!(!core::ptr::eq(
            unsafe { (*active_ptr).pages.pages.first },
            unsafe { (*active_ptr).pages.pages.last }
        ));
        // s.nextSlice("st");
        s.next_slice(b"st");

        let active = unsafe { &mut *active_ptr };
        let mut search: PageListSearch =
            PageListSearch::init(alloc, b"Test", &mut active.pages, active.pages.pages.last)
                .unwrap();

        // Should not find any matches since we broke with an explicit newline.
        assert!(search.next().is_none());
        assert!(search.feed().unwrap());
        assert!(search.next().is_none());
        assert!(!search.feed().unwrap());

        search.deinit();
        s.deinit();
        t.deinit(alloc);
    }

    // test "feed with pruned page"
    #[cfg_attr(
        all(miri, debug_assertions),
        ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
    )]
    #[test]
    fn feed_with_pruned_page() {
        // const alloc = testing.allocator;
        let alloc = Allocator;

        // Zero here forces minimum max size to effectively two pages.
        // var p: PageList = try .init(alloc, .{
        //     .cols = 80,
        //     .rows = 24,
        //     .max_size = 0,
        // });
        let mut p: PageList = PageList::init(
            crate::alloc::Global,
            Options {
                max_size: Some(0),
                ..Options::new(80, 24)
            },
        )
        .unwrap();
        // defer p.deinit();

        // Grow to capacity
        // const page1_node = p.pages.last.?;
        let page1_node = p.pages.last;
        // const page1 = page1_node.data;
        // for (0..page1.capacity.rows - page1.size.rows) |_| {
        //     try testing.expect(try p.grow() == null);
        // }
        let page1_cap_rows = unsafe { crate::pagelist::Node::capacity(page1_node).rows };
        let page1_size_rows = unsafe { crate::pagelist::Node::rows(page1_node) };
        for _ in 0..page1_cap_rows - page1_size_rows {
            assert!(p.grow().unwrap().is_none());
        }

        // Grow and allocate one more page. Then fill that page up.
        // const page2_node = (try p.grow()).?;
        let page2_node = p.grow().unwrap().unwrap();
        // const page2 = page2_node.data;
        // for (0..page2.capacity.rows - page2.size.rows) |_| {
        //     try testing.expect(try p.grow() == null);
        // }
        let page2_cap_rows = unsafe { crate::pagelist::Node::capacity(page2_node).rows };
        let page2_size_rows = unsafe { crate::pagelist::Node::rows(page2_node) };
        for _ in 0..page2_cap_rows - page2_size_rows {
            assert!(p.grow().unwrap().is_none());
        }

        // Setup search and feed until we can't
        // var search: PageListSearch = try .init(
        //     alloc,
        //     "Test",
        //     &p,
        //     p.pages.last.?,
        // );
        // defer search.deinit();
        let p_ptr: *mut PageList = &mut p;
        let last = p.pages.last;
        let mut search: PageListSearch = PageListSearch::init(alloc, b"Test", p_ptr, last).unwrap();
        // try testing.expect(try search.feed());
        assert!(search.feed().unwrap());
        // try testing.expect(!try search.feed());
        assert!(!search.feed().unwrap());

        // Next should create a new page, but it should reuse our first
        // page since we're at max size.
        // const new = (try p.grow()).?;
        let new = p.grow().unwrap().unwrap();
        // try testing.expect(p.pages.last.? == new);
        assert!(core::ptr::eq(p.pages.last, new));

        // Our first should now be page2 and our last should be page1
        // try testing.expectEqual(page2_node, p.pages.first.?);
        assert!(core::ptr::eq(page2_node, p.pages.first));
        // try testing.expectEqual(page1_node, p.pages.last.?);
        assert!(core::ptr::eq(page1_node, p.pages.last));

        // Feed should still do nothing
        // try testing.expect(!try search.feed());
        assert!(!search.feed().unwrap());

        search.deinit();
        p.deinit();
    }
    /// PORTS: oracle 5d8eb78b7
    /// `test "feed keeps its tracked pin within a shorter page"`
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): out of budget in the debug-assertions-ON G6 lane, and NOT rescuable by the G6c debug-assertions-off lane — this test drives a `debug_assertions`-only mechanism and is already `cfg_attr(not(debug_assertions), ignore)`, so turning the assertions off makes it inert rather than fast. Covered natively + by ASan."
    )]
    // Release lane: this test drives a slow-runtime-safety mechanism (the
    // pagelist integrity gate / `pin_is_valid`), which the port compiles out
    // when `debug_assertions` is off — faithful to the reference's
    // `build_options.slow_runtime_safety`, under which the oracle's test only
    // exists. Debug-only; it still runs and passes there. (Same convention as
    // `terminal_full_reset_tracked_pins`.)
    #[test]
    #[cfg_attr(
        not(debug_assertions),
        ignore = "DEBUG-ONLY BY DESIGN, and this reason was MEASURED (fix round 1 of parity S166-F2): this test drives the slow-runtime-safety mechanism — `PageList::assert_integrity` / `pin_is_valid` — which the port compiles out when `debug_assertions` is off, faithful to the reference's `build_options.slow_runtime_safety`, under which the oracle's counterpart only exists. It is NOT ignored because of the tripwire gate (that claim was false and is gone): un-ignoring it turns the release lane RED, which is why it stays."
    )]
    fn feed_keeps_its_tracked_pin_within_a_shorter_page() {
        use crate::pagelist::{Node as PlNode, PageList, Pin};
        // PORT NOTE: 80 cols instead of the reference's 10. The property under
        // test is only that the split successor is SHORTER than the second page;
        // 10 cols yields ~35k rows per page, which makes the row-by-row fill and
        // the formatter passes take ~1 minute in a debug Rust build.
        let mut pages: PageList =
            PageList::init(crate::alloc::Global, Options::new(80, 2)).unwrap();

        // PORT NOTE: the fill loops are wrapped in `pause_integrity_checks`.
        // rustty's debug `verify_integrity` walks every tracked pin on every
        // `grow`, making the reference's row-by-row fill of a ~35k-row page
        // quadratic (>70s). The checks are re-enabled — and run — before the
        // assertions this test actually makes.
        pages.pause_integrity_checks(true);
        let first = pages.pages.first;
        while unsafe { PlNode::rows(first) } < unsafe { PlNode::capacity(first) }.rows {
            let _ = pages.grow().unwrap();
        }

        let second = pages.grow().unwrap().unwrap();
        while unsafe { PlNode::rows(second) } < unsafe { PlNode::capacity(second) }.rows {
            let _ = pages.grow().unwrap();
        }
        pages.pause_integrity_checks(false);
        pages.assert_integrity();

        pages
            .split(Pin {
                node: first,
                y: 1,
                x: 0,
                garbage: false,
            })
            .unwrap();
        let shorter = unsafe { (*first).next };
        assert!(unsafe { PlNode::rows(shorter) } < unsafe { PlNode::rows(second) });

        let mut search: PageListSearch =
            PageListSearch::init(Allocator, b"x", &mut pages, second).unwrap();

        assert!(search.feed().unwrap());
        assert!(core::ptr::eq(unsafe { (*search.pin).node }, shorter));
        assert!(pages.pin_is_valid(unsafe { (*search.pin).pin_copy() }));

        search.deinit();
        pages.deinit();
    }
}
