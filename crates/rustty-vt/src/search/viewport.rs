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
use crate::highlight::Flattened as FlattenedHighlight;
use crate::pagelist::iter::Direction as IterDirection;
use crate::pagelist::{Node, OutOfMemory, PageList};
use crate::point;
use allocator_api2::alloc::Global as Allocator;

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
    pub fingerprint: Option<Fingerprint>,

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
        //     .fingerprint = null,
        //     .active_dirty = null,
        // };
        Ok(ViewportSearch {
            window,
            fingerprint: None,
            active_dirty: None,
        })
    }

    // pub fn deinit(self: *ViewportSearch) void {
    pub fn deinit(&mut self) {
        // if (self.fingerprint) |*fp| fp.deinit(self.window.alloc);
        if let Some(fp) = self.fingerprint.as_mut() {
            fp.deinit(self.window.alloc);
        }
        // self.window.deinit();
        self.window.deinit();
    }

    /// Reset our fingerprint and results so that the next update will
    /// always re-search.
    // pub fn reset(self: *ViewportSearch) void {
    pub fn reset(&mut self) {
        // if (self.fingerprint) |*fp| fp.deinit(self.window.alloc);
        if let Some(fp) = self.fingerprint.as_mut() {
            fp.deinit(self.window.alloc);
        }
        // self.fingerprint = null;
        self.fingerprint = None;
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
    // pub fn update(
    //     self: *ViewportSearch,
    //     list: *PageList,
    // ) Allocator.Error!bool {
    pub fn update(&mut self, list: &PageList) -> Result<bool, OutOfMemory> {
        // See if our viewport changed
        // var fingerprint: Fingerprint = try .init(self.window.alloc, list);
        let mut fingerprint: Fingerprint = Fingerprint::init(self.window.alloc, list)?;
        // if (self.fingerprint) |*old| {
        if let Some(old) = self.fingerprint.as_mut() {
            // if (old.eql(fingerprint)) match: {
            'match_blk: {
                if old.eql(&fingerprint) {
                    // Determine if we need to check if we overlap the active
                    // area. If we have dirty tracking on we also set it to
                    // false here.
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
                        // for (old.entries) |entry| {
                        //     if (entry.node == active_tl.node) break :match;
                        //     if (entry.node == active_br.node) break :match;
                        // }
                        for node in old.entries.iter().map(|e| e.node) {
                            if core::ptr::eq(node, active_tl.node) {
                                break 'match_blk;
                            }
                            if core::ptr::eq(node, active_br.node) {
                                break 'match_blk;
                            }
                        }
                    }

                    // No change
                    // fingerprint.deinit(self.window.alloc);
                    // return false;
                    fingerprint.deinit(self.window.alloc);
                    return Ok(false);
                }
            }

            // old.deinit(self.window.alloc);
            // self.fingerprint = null;
            old.deinit(self.window.alloc);
            self.fingerprint = None;
        }
        // assert(self.fingerprint == null);
        assert!(self.fingerprint.is_none());
        // self.fingerprint = fingerprint;
        // errdefer {
        //     fingerprint.deinit(self.window.alloc);
        //     self.fingerprint = null;
        // }
        //
        // PORT NOTE: the reference stores `fingerprint` into `self.fingerprint`
        // and keeps a separate `errdefer` that deinits it on any later error.
        // We store it now and route the fallible-step errors through an explicit
        // cleanup closure (`fail` below) so the moved-out value is still freed.
        self.fingerprint = Some(fingerprint);

        // Helper mirroring the reference `errdefer { fingerprint.deinit(...); self.fingerprint = null; }`.
        macro_rules! fail {
            ($e:expr) => {{
                if let Some(fp) = self.fingerprint.as_mut() {
                    fp.deinit(self.window.alloc);
                }
                self.fingerprint = None;
                return Err($e);
            }};
        }

        // If our active area was set as dirty, we always unset it here
        // because we're re-searching now.
        // if (self.active_dirty) |*v| v.* = false;
        if self.active_dirty.is_some() {
            self.active_dirty = Some(false);
        }

        // Clear our previous sliding window
        // self.window.clearAndRetainCapacity();
        self.window.clear_and_retain_capacity();

        // PORT NOTE: re-borrow the stored fingerprint's entries. We snapshot the
        // node pointers into a local so we can mutate `self.window` (also a field
        // of `self`) without aliasing the borrow of `self.fingerprint`.
        let fp_nodes: Vec<*mut Node> = self
            .fingerprint
            .as_ref()
            .unwrap()
            .entries
            .iter()
            .map(|e| e.node)
            .collect();

        // Add enough overlap to cover needle.len - 1 bytes (if it
        // exists) so we can cover the overlap. We only do this for the
        // soft-wrapped prior pages.
        // var node_ = fingerprint.entries[0].node.prev;
        // var added: usize = 0;
        let mut node_: *mut Node = unsafe { (*fp_nodes[0]).prev };
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
            let appended = match self.window.append_if_wrapped(node) {
                Ok(Some(v)) => v,
                Ok(None) => break,
                Err(e) => fail!(e),
            };
            added += appended.content_len;
            if added >= self.window.needle.len() - 1 {
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
        for node in fp_nodes.iter().copied() {
            end_append = match self.window.append(node) {
                Ok(v) => v,
                Err(e) => fail!(e),
            };
        }

        // Add any trailing overlap as well.
        // trailing: {
        'trailing: {
            // const end: *PageList.List.Node =
            //     fingerprint.entries[fingerprint.entries.len - 1].node;
            let end: *mut Node = fp_nodes[fp_nodes.len() - 1];
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
                let appended = match self.window.append(node) {
                    Ok(v) => v,
                    Err(e) => fail!(e),
                };
                added += appended.content_len;
                if added >= self.window.needle.len() - 1 {
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
    pub fn next(&mut self) -> Option<FlattenedHighlight> {
        // return self.window.next();
        self.window.next()
    }
}

/// Viewport fingerprint so we can detect when the viewport moves.
pub struct Fingerprint {
    /// The node addresses and generations that make up the viewport. We
    /// can't safely dereference cached node pointers because the nodes may
    /// be invalid, so equality only compares these captured values.
    // entries: []const Entry,
    pub entries: Vec<Entry>,
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
    // pub fn init(alloc: Allocator, pages: *PageList) Allocator.Error!Fingerprint {
    pub fn init(_alloc: Allocator, pages: &PageList) -> Result<Fingerprint, OutOfMemory> {
        // var list: std.ArrayList(Entry) = .empty;
        // defer list.deinit(alloc);
        let mut list: Vec<Entry> = Vec::new();

        // Get our viewport area. Bottom right of a viewport can never
        // fail.
        // const tl = pages.getTopLeft(.viewport);
        let tl = pages.get_top_left(point::Tag::Viewport);
        // const br = pages.getBottomRight(.viewport).?;
        let br = pages.get_bottom_right(point::Tag::Viewport).unwrap();

        // var it = tl.pageIterator(.right_down, br);
        // while (it.next()) |chunk| try list.append(alloc, .{
        //     .node = chunk.node,
        //     .serial = chunk.node.serial,
        // });
        //
        // INSPECT, NOT RESTORE: only the node's generation stamp is read, which
        // lives in the pagelist Node itself (never in the page mapping), so a
        // compressed page must NOT be restored to fingerprint the viewport.
        let mut it = tl.page_iterator(IterDirection::RightDown, Some(br));
        while let Some(chunk) = it.next() {
            list.push(Entry {
                node: chunk.node,
                serial: unsafe { (*chunk.node).serial },
            });
        }
        // return .{ .entries = try list.toOwnedSlice(alloc) };
        Ok(Fingerprint { entries: list })
    }

    // pub fn deinit(self: *Fingerprint, alloc: Allocator) void {
    //     alloc.free(self.nodes);
    // }
    //
    // PORT NOTE: the reference frees the owned `entries` slice; the Rust `Vec`
    // frees on drop, so this clears it (kept for API parity with the explicit
    // `deinit` callers above).
    pub fn deinit(&mut self, _alloc: Allocator) {
        self.entries = Vec::new();
    }

    // pub fn eql(self: Fingerprint, other: Fingerprint) bool {
    pub fn eql(&self, other: &Fingerprint) -> bool {
        // if (self.entries.len != other.entries.len) return false;
        if self.entries.len() != other.entries.len() {
            return false;
        }
        // for (self.entries, other.entries) |a, b| {
        //     if (a.node != b.node) return false;
        //     if (a.serial != b.serial) return false;
        // }
        for (a, b) in self.entries.iter().zip(other.entries.iter()) {
            if !core::ptr::eq(a.node, b.node) {
                return false;
            }
            if a.serial != b.serial {
                return false;
            }
        }
        // return true;
        true
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
            max_scrollback: 10_000,
            colors: Colors {
                background: DynamicRGB::UNSET,
                foreground: DynamicRGB::UNSET,
                cursor: DynamicRGB::UNSET,
                palette: DynamicPalette::default_palette(),
            },
            default_modes: modespkg::ModePacked::default(),
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

        // const old_entry = search.fingerprint.?.entries[0];
        let old_entry = search.fingerprint.as_ref().unwrap().entries[0];
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
        // const new_entry = search.fingerprint.?.entries[0];
        let new_entry = search.fingerprint.as_ref().unwrap().entries[0];
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
        miri,
        ignore = "MIRI EXCEPTION (vt multi-page-PageList intractable, .port/phase-2/MIRI_EXCEPTIONS.md): builds/reflows multi-page PageLists that exceed Miri's interpreter budget (>20min); covered natively + by ASan (all PageList tests ~18s)"
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
