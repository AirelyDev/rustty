//! rustty-vt search/active
//!
//! PORTS: ghostty src/terminal/search/active.zig
//!
//! PORT STATUS: complete (vt-15). Faithful port of `ActiveSearch`, which
//! searches only the mutable active area of a `PageList`, plus its 2 tests.

// const std = @import("std");
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
use crate::pagelist::{Node, OutOfMemory, PageList};

/// Searches for a substring within the active area of a PageList.
///
/// The distinction for "active area" is important because it is the
/// only part of a PageList that is mutable. Therefore, its the only part
/// of the terminal that needs to be repeatedly searched as the contents
/// change.
///
/// This struct specializes in searching only within that active area,
/// and handling the active area moving as new lines are added to the bottom.
pub struct ActiveSearch {
    pub(crate) window: SlidingWindow,
}

impl ActiveSearch {
    // pub fn init(
    //     alloc: Allocator,
    //     needle: []const u8,
    // ) Allocator.Error!ActiveSearch {
    pub fn init(alloc: Allocator, needle: &[u8]) -> Result<ActiveSearch, OutOfMemory> {
        // We just do a forward search since the active area is usually
        // pretty small so search results are instant anyways. This avoids
        // a small amount of work to reverse things.
        // var window: SlidingWindow = try .init(alloc, .forward, needle);
        // errdefer window.deinit();
        let window: SlidingWindow = SlidingWindow::init(alloc, Direction::Forward, needle)?;
        // return .{ .window = window };
        Ok(ActiveSearch { window })
    }

    // pub fn deinit(self: *ActiveSearch) void {
    pub fn deinit(&mut self) {
        // self.window.deinit();
        self.window.deinit();
    }

    /// Update the active area to reflect the current state of the PageList.
    ///
    /// This doesn't do the search, it only copies the necessary data
    /// to perform the search later. This lets the caller hold the lock
    /// on the PageList for a minimal amount of time.
    ///
    /// This returns the first page (in reverse order) covered by this
    /// search. This allows the history search to overlap and search history.
    /// There CAN BE duplicates, and this page CAN BE mutable, so the history
    /// search results should prune anything that's in the active area.
    ///
    /// If the return value is null it means the active area covers the entire
    /// PageList, currently.
    // pub fn update(
    //     self: *ActiveSearch,
    //     list: *const PageList,
    // ) Allocator.Error!?*PageList.List.Node {
    pub fn update(&mut self, list: &PageList) -> Result<Option<*mut Node>, OutOfMemory> {
        // Clear our previous sliding window
        // self.window.clearAndRetainCapacity();
        self.window.clear_and_retain_capacity();

        // An empty needle represents an inactive search and has no overlap
        // or history to load.
        //
        // PORT NOTE (oracle 5bc6588e4, `P3-W1 T5`): this guard is what keeps
        // the `needle.len() - 1` overlap comparison below in range. Upstream
        // does NOT saturate that site (only the two in `viewport.rs`, which has
        // no such guard), so neither do we.
        // if (self.window.needle.len == 0) return null;
        if self.window.needle.is_empty() {
            return Ok(None);
        }

        // First up, add enough pages to cover the active area.
        // var rem: usize = list.rows;
        // var node_ = list.pages.last;
        // var last_node: ?*PageList.List.Node = null;
        let mut rem: usize = list.rows as usize;
        let mut node_: *mut Node = list.pages.last;
        let mut last_node: Option<*mut Node> = None;
        // while (node_) |node| : (node_ = node.prev) {
        while !node_.is_null() {
            let node = node_;
            // _ = try self.window.append(node);
            let _ = self.window.append(node)?;
            // last_node = node;
            last_node = Some(node);

            // If we reached our target amount, then this is the last
            // page that contains the active area. We go to the previous
            // page once more since its the first page of our required
            // overlap.
            // if (rem <= node.data.size.rows) {
            //     node_ = node.prev;
            //     break;
            // }
            if rem <= unsafe { crate::pagelist::Node::rows(node) as usize } {
                node_ = unsafe { (*node).prev };
                break;
            }

            // rem -= node.data.size.rows;
            rem -= unsafe { crate::pagelist::Node::rows(node) as usize };

            node_ = unsafe { (*node).prev };
        }

        // Next, add enough overlap to cover needle.len - 1 bytes (if it
        // exists) so we can cover the overlap.
        // while (node_) |node| : (node_ = node.prev) {
        while !node_.is_null() {
            let node = node_;
            // We could be more accurate here and count bytes since the
            // last wrap but its complicated and unlikely multiple pages
            // wrap so this should be fine.
            //
            // INSPECT, NOT RESTORE: `append_if_wrapped` takes ONE preserved
            // page for both the wrap check and the formatting, so a compressed
            // overlap page is decoded at most once and stays compressed
            // (oracle 0d015b2fc).
            // const appended = try self.window.appendIfWrapped(node) orelse break;
            // if (appended.content_len >= self.window.needle.len - 1) break;
            let appended = match self.window.append_if_wrapped(node)? {
                Some(v) => v,
                None => break,
            };
            if appended.content_len >= self.window.needle.len() - 1 {
                break;
            }

            node_ = unsafe { (*node).prev };
        }

        // Return the last node we added to our window.
        // return last_node;
        Ok(last_node)
    }

    /// Find the next match for the needle in the active area. This returns
    /// null when there are no more matches.
    // pub fn next(self: *ActiveSearch) ?FlattenedHighlight {
    /// Rust-only test accounting (parity tail S7-6): see
    /// `SlidingWindow::owned_bytes`.
    #[cfg(any(test, feature = "test-allocators"))]
    pub fn owned_bytes(&self) -> usize {
        self.window.owned_bytes()
    }

    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<FlattenedRef<'_>> {
        // return self.window.next();
        self.window.next()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::{DynamicPalette, DynamicRGB};
    use crate::kitty;
    use crate::modes as modespkg;
    use crate::point::{self, Point};
    use crate::terminal::{Colors, Options as TerminalOptions, Terminal};

    // PORT NOTE: the reference tests call `Terminal.init(alloc, .{ .cols, .rows })`
    // relying on Zig `Options` field defaults. Rust has no struct-literal
    // defaults, so this helper supplies them explicitly (mirroring the
    // `terminal.rs`/`sliding_window.rs` test `opts` helpers).
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

    /// `P3-W1 T5` (oracle 5bc6588e4). An empty needle is an INACTIVE search:
    /// `update` must report no overlap page and produce no matches, rather than
    /// walking the page list and running the `needle.len() - 1` overlap
    /// comparison that would underflow.
    #[test]
    fn active_empty_needle_is_an_inactive_search() {
        let alloc = Allocator;
        let mut t: Terminal = Terminal::init(alloc, terminal_opts(10, 10)).unwrap();
        let active_ptr: *mut crate::screen::Screen = t.screens.active;
        unsafe { (*active_ptr).test_write_string(b"hello\nworld\n").unwrap() };

        let mut search: ActiveSearch = ActiveSearch::init(alloc, b"").unwrap();
        assert!(search
            .update(unsafe { &(*active_ptr).pages })
            .unwrap()
            .is_none());
        assert!(search.next().is_none());

        search.deinit();
        t.deinit(alloc);
    }

    // test "simple search"
    #[test]
    fn simple_search() {
        // const alloc = testing.allocator;
        let alloc = Allocator;
        // var t: Terminal = try .init(alloc, .{ .cols = 10, .rows = 10 });
        let mut t: Terminal = Terminal::init(alloc, terminal_opts(10, 10)).unwrap();
        // defer t.deinit(alloc);

        // PORT NOTE: `t.screens.active` is a stable heap `*mut Screen`; copy the
        // raw pointer out before borrowing `t` mutably via `vt_stream` so the
        // two accesses don't alias under the borrow checker (the reference reads
        // `t.screens.active` freely as a value).
        let active_ptr: *mut crate::screen::Screen = t.screens.active;

        // var s = t.vtStream();
        let mut s = t.vt_stream();
        // defer s.deinit();
        // s.nextSlice("Fizz\r\nBuzz\r\nFizz\r\nBang");
        s.next_slice(b"Fizz\r\nBuzz\r\nFizz\r\nBang");

        // var search: ActiveSearch = try .init(alloc, "Fizz");
        let mut search: ActiveSearch = ActiveSearch::init(alloc, b"Fizz").unwrap();
        // defer search.deinit();
        // _ = try search.update(&t.screens.active.pages);
        let active = unsafe { &mut *active_ptr };
        let _ = search.update(&active.pages).unwrap();

        {
            // const h = search.next().?;
            let h = search.next().unwrap();
            // const sel = h.untracked();
            let sel = h.untracked();
            // try testing.expectEqual(point.Point{ .active = .{ .x = 0, .y = 0 } }, ...);
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 0, y: 0 })),
                active.pages.point_from_pin(point::Tag::Active, sel.start)
            );
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 3, y: 0 })),
                active.pages.point_from_pin(point::Tag::Active, sel.end)
            );
        }
        {
            let h = search.next().unwrap();
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
        // try testing.expect(search.next() == null);
        assert!(search.next().is_none());

        search.deinit();
        s.deinit();
        t.deinit(alloc);
    }

    // test "clear screen and search"
    #[test]
    fn clear_screen_and_search() {
        // const alloc = testing.allocator;
        let alloc = Allocator;
        // var t: Terminal = try .init(alloc, .{ .cols = 10, .rows = 10 });
        let mut t: Terminal = Terminal::init(alloc, terminal_opts(10, 10)).unwrap();
        // defer t.deinit(alloc);

        // PORT NOTE: see the prior test; copy the stable `*mut Screen` out before
        // borrowing `t` mutably through `vt_stream`.
        let active_ptr: *mut crate::screen::Screen = t.screens.active;

        // var s = t.vtStream();
        let mut s = t.vt_stream();
        // defer s.deinit();
        // s.nextSlice("Fizz\r\nBuzz\r\nFizz\r\nBang");
        s.next_slice(b"Fizz\r\nBuzz\r\nFizz\r\nBang");

        // var search: ActiveSearch = try .init(alloc, "Fizz");
        let mut search: ActiveSearch = ActiveSearch::init(alloc, b"Fizz").unwrap();
        // defer search.deinit();
        // _ = try search.update(&t.screens.active.pages);
        {
            let active = unsafe { &mut *active_ptr };
            let _ = search.update(&active.pages).unwrap();
        }

        // s.nextSlice("\x1b[2J"); // Clear screen
        s.next_slice(b"\x1b[2J"); // Clear screen
                                  // s.nextSlice("\x1b[H"); // Move cursor home
        s.next_slice(b"\x1b[H"); // Move cursor home
                                 // s.nextSlice("Buzz\r\nFizz\r\nBuzz");
        s.next_slice(b"Buzz\r\nFizz\r\nBuzz");
        // _ = try search.update(&t.screens.active.pages);
        let active = unsafe { &mut *active_ptr };
        let _ = search.update(&active.pages).unwrap();

        {
            let h = search.next().unwrap();
            let sel = h.untracked();
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 0, y: 1 })),
                active.pages.point_from_pin(point::Tag::Active, sel.start)
            );
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 3, y: 1 })),
                active.pages.point_from_pin(point::Tag::Active, sel.end)
            );
        }
        // try testing.expect(search.next() == null);
        assert!(search.next().is_none());

        search.deinit();
        s.deinit();
        t.deinit(alloc);
    }
}
