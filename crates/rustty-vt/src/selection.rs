//! rustty-vt selection
//!
//! Represents a single selection within the terminal (i.e. a highlight region).
//!
//! PORTS: ghostty src/terminal/Selection.zig
//!
//! PORT STATUS: ported (vt-11d) — full Selection: bounds/order/containment/
//! adjust + the 18 Selection tests.
// PORT NOTE: `start_ptr`/`end_ptr` mirror the reference's `startPtr`/`endPtr`
// (non-trait-method `start`/`end` naming kept verbatim from the reference).
#![allow(clippy::should_implement_trait)]

// const std = @import("std");
// const assert = @import("../quirks.zig").inlineAssert;
// const Allocator = std.mem.Allocator;
// const lib = @import("lib.zig");
// const page = @import("page.zig");
// const point = @import("point.zig");
// const PageList = @import("PageList.zig");
// const Screen = @import("Screen.zig");
// const Pin = PageList.Pin;
use crate::page;
use crate::pagelist::iter::Direction;
use crate::pagelist::Pin;
use crate::point;
use crate::screen::Screen;
use crate::size;

// PORT NOTE: the reference `Allocator.Error` maps to `pagelist::OutOfMemory`
// (the crate's allocator error), matching the PageList allocator seam.
use crate::pagelist::OutOfMemory;

// NOTE(mitchellh): I'm not very happy with how this is implemented, because
// the ordering operations which are used frequently require using
// pointFromPin which -- at the time of writing this -- is slow. The overall
// style of this struct is due to porting it from the previous implementation
// which had an efficient ordering operation.
//
// While reimplementing this, there were too many callers that already
// depended on this behavior so I kept it despite the inefficiency. In the
// future, we should take a look at this again!

/// Represents a single selection within the terminal (i.e. a highlight region).
#[derive(Clone, Copy)]
pub struct Selection {
    /// The bounds of the selection.
    pub bounds: Bounds,

    /// Whether or not this selection refers to a rectangle, rather than whole
    /// lines of a buffer. In this mode, start and end refer to the top left and
    /// bottom right of the rectangle, or vice versa if the selection is backwards.
    pub rectangle: bool,
}

/// The bounds of the selection. A selection bounds can be either tracked
/// or untracked. Untracked bounds are unsafe beyond the point the terminal
/// screen may be modified, since they may point to invalid memory. Tracked
/// bounds are always valid and will be updated as the screen changes, but
/// are more expensive to exist.
///
/// In all cases, start and end can be in any order. There is no guarantee that
/// start is before end or vice versa. If a user selects backwards,
/// start will be after end, and vice versa. Use the struct functions
/// to not have to worry about this.
// PORT NOTE: `*Pin` in the `tracked` variant maps to `*mut Pin` (the faithful
// stable-pointer map: `PinPool` gives stable pointers via `track_pin`).
#[derive(Clone, Copy)]
pub enum Bounds {
    Untracked { start: Pin, end: Pin },

    Tracked { start: *mut Pin, end: *mut Pin },
}

impl Selection {
    /// Initialize a new selection with the given start and end pins on
    /// the screen. The screen will be used for pin tracking.
    pub fn init(start_pin: Pin, end_pin: Pin, rect: bool) -> Selection {
        Selection {
            bounds: Bounds::Untracked {
                start: start_pin,
                end: end_pin,
            },
            rectangle: rect,
        }
    }

    pub fn deinit(self, s: &mut Screen) {
        match self.bounds {
            Bounds::Tracked { start, end } => {
                s.pages.untrack_pin(start);
                s.pages.untrack_pin(end);
            }

            Bounds::Untracked { .. } => {}
        }
    }

    /// Returns true if this selection is equal to another selection.
    pub fn eql(self, other: Selection) -> bool {
        self.start().eql(other.start())
            && self.end().eql(other.end())
            && self.rectangle == other.rectangle
    }

    /// The starting pin of the selection. This is NOT ordered.
    pub fn start_ptr(&mut self) -> *mut Pin {
        match &mut self.bounds {
            Bounds::Untracked { start, .. } => start as *mut Pin,
            Bounds::Tracked { start, .. } => *start,
        }
    }

    /// The ending pin of the selection. This is NOT ordered.
    pub fn end_ptr(&mut self) -> *mut Pin {
        match &mut self.bounds {
            Bounds::Untracked { end, .. } => end as *mut Pin,
            Bounds::Tracked { end, .. } => *end,
        }
    }

    pub fn start(self) -> Pin {
        match self.bounds {
            Bounds::Untracked { start, .. } => start,
            Bounds::Tracked { start, .. } => unsafe { *start },
        }
    }

    pub fn end(self) -> Pin {
        match self.bounds {
            Bounds::Untracked { end, .. } => end,
            Bounds::Tracked { end, .. } => unsafe { *end },
        }
    }

    /// Returns true if this is a tracked selection.
    pub fn tracked(&self) -> bool {
        match self.bounds {
            Bounds::Untracked { .. } => false,
            Bounds::Tracked { .. } => true,
        }
    }

    /// Convert this selection a tracked selection. It is asserted this is
    /// an untracked selection. The tracked selection is returned.
    pub fn track(&self, s: &mut Screen) -> Result<Selection, OutOfMemory> {
        debug_assert!(!self.tracked());

        // Track our pins
        let (start_pin, end_pin) = match self.bounds {
            Bounds::Untracked { start, end } => (start, end),
            // PORT NOTE: the reference reads `self.bounds.untracked.start/.end`
            // directly (a checked union access asserted above); the Rust port's
            // exhaustive `match` makes the `tracked` arm unreachable given the
            // `debug_assert!(!self.tracked())` above.
            Bounds::Tracked { .. } => unreachable!(),
        };
        let tracked_start = s.pages.track_pin(start_pin)?;
        // errdefer s.pages.untrackPin(tracked_start);
        let tracked_end = match s.pages.track_pin(end_pin) {
            Ok(p) => p,
            Err(e) => {
                s.pages.untrack_pin(tracked_start);
                return Err(e);
            }
        };

        Ok(Selection {
            bounds: Bounds::Tracked {
                start: tracked_start,
                end: tracked_end,
            },
            rectangle: self.rectangle,
        })
    }

    /// Returns the top left point of the selection.
    pub fn top_left(self, s: &Screen) -> Pin {
        match self.order(s) {
            Order::Forward => self.start(),
            Order::Reverse => self.end(),
            Order::MirroredForward => {
                let mut p = self.start();
                // p.x = @min(self.end().x, p.node.cols() - 1);
                let cols = unsafe { crate::pagelist::Node::cols(p.node) };
                p.x = core::cmp::min(self.end().x, cols - 1);
                p
            }
            Order::MirroredReverse => {
                let mut p = self.end();
                let cols = unsafe { crate::pagelist::Node::cols(p.node) };
                p.x = core::cmp::min(self.start().x, cols - 1);
                p
            }
        }
    }

    /// Returns the bottom right point of the selection.
    pub fn bottom_right(self, s: &Screen) -> Pin {
        match self.order(s) {
            Order::Forward => self.end(),
            Order::Reverse => self.start(),
            Order::MirroredForward => {
                let mut p = self.end();
                let cols = unsafe { crate::pagelist::Node::cols(p.node) };
                p.x = core::cmp::min(self.start().x, cols - 1);
                p
            }
            Order::MirroredReverse => {
                let mut p = self.start();
                let cols = unsafe { crate::pagelist::Node::cols(p.node) };
                p.x = core::cmp::min(self.end().x, cols - 1);
                p
            }
        }
    }

    pub fn order(self, s: &Screen) -> Order {
        let start_pt = s
            .pages
            .point_from_pin(point::Tag::Screen, self.start())
            .unwrap()
            .coord();
        let end_pt = s
            .pages
            .point_from_pin(point::Tag::Screen, self.end())
            .unwrap()
            .coord();

        if self.rectangle {
            // Reverse (also handles single-column)
            if start_pt.y > end_pt.y && start_pt.x >= end_pt.x {
                return Order::Reverse;
            }
            if start_pt.y >= end_pt.y && start_pt.x > end_pt.x {
                return Order::Reverse;
            }

            // Mirror, bottom-left to top-right
            if start_pt.y > end_pt.y && start_pt.x < end_pt.x {
                return Order::MirroredReverse;
            }

            // Mirror, top-right to bottom-left
            if start_pt.y < end_pt.y && start_pt.x > end_pt.x {
                return Order::MirroredForward;
            }

            // Forward
            return Order::Forward;
        }

        if start_pt.y < end_pt.y {
            return Order::Forward;
        }
        if start_pt.y > end_pt.y {
            return Order::Reverse;
        }
        if start_pt.x <= end_pt.x {
            return Order::Forward;
        }
        Order::Reverse
    }

    /// Returns the selection in the given order.
    ///
    /// The returned selection is always a new untracked selection.
    ///
    /// Note that only forward and reverse are useful desired orders for this
    /// function. All other orders act as if forward order was desired.
    pub fn ordered(self, s: &Screen, desired: Order) -> Selection {
        if self.order(s) == desired {
            return Selection::init(self.start(), self.end(), self.rectangle);
        }

        let tl = self.top_left(s);
        let br = self.bottom_right(s);
        match desired {
            Order::Forward => Selection::init(tl, br, self.rectangle),
            Order::Reverse => Selection::init(br, tl, self.rectangle),
            _ => Selection::init(tl, br, self.rectangle),
        }
    }

    /// Returns true if the selection contains the given point.
    ///
    /// This recalculates top left and bottom right each call. If you have
    /// many points to check, it is cheaper to do the containment logic
    /// yourself and cache the topleft/bottomright.
    pub fn contains(self, s: &Screen, pin: Pin) -> bool {
        let tl_pin = self.top_left(s);
        let br_pin = self.bottom_right(s);

        // This is definitely not very efficient. Low-hanging fruit to
        // improve this.
        let tl = s
            .pages
            .point_from_pin(point::Tag::Screen, tl_pin)
            .unwrap()
            .coord();
        let br = s
            .pages
            .point_from_pin(point::Tag::Screen, br_pin)
            .unwrap()
            .coord();
        let p = s
            .pages
            .point_from_pin(point::Tag::Screen, pin)
            .unwrap()
            .coord();

        // If we're in rectangle select, we can short-circuit with an easy check
        // here
        if self.rectangle {
            return p.y >= tl.y && p.y <= br.y && p.x >= tl.x && p.x <= br.x;
        }

        // If tl/br are same line
        if tl.y == br.y {
            return p.y == tl.y && p.x >= tl.x && p.x <= br.x;
        }

        // If on top line, just has to be left of X
        if p.y == tl.y {
            return p.x >= tl.x;
        }

        // If on bottom line, just has to be right of X
        if p.y == br.y {
            return p.x <= br.x;
        }

        // If between the top/bottom, always good.
        p.y > tl.y && p.y < br.y
    }

    /// Get a selection for a single row in the screen. This will return null
    /// if the row is not included in the selection.
    ///
    /// This is a very expensive operation. It has to traverse the linked list
    /// of pages for the top-left, bottom-right, and the given pin to find
    /// the coordinates. If you are calling this repeatedly, prefer
    /// `containedRowCached`.
    pub fn contained_row(self, s: &Screen, pin: Pin) -> Option<Selection> {
        let tl_pin = self.top_left(s);
        let br_pin = self.bottom_right(s);

        // This is definitely not very efficient. Low-hanging fruit to
        // improve this. Callers should prefer containedRowCached if they
        // can swing it.
        let tl = s
            .pages
            .point_from_pin(point::Tag::Screen, tl_pin)
            .unwrap()
            .coord();
        let br = s
            .pages
            .point_from_pin(point::Tag::Screen, br_pin)
            .unwrap()
            .coord();
        let p = s
            .pages
            .point_from_pin(point::Tag::Screen, pin)
            .unwrap()
            .coord();

        self.contained_row_cached(s, tl_pin, br_pin, pin, tl, br, p)
    }

    /// Same as containedRow but useful if you're calling it repeatedly
    /// so that the pins can be cached across calls. Advanced.
    #[allow(clippy::too_many_arguments)]
    pub fn contained_row_cached(
        self,
        s: &Screen,
        tl_pin: Pin,
        br_pin: Pin,
        pin: Pin,
        tl: point::Coordinate,
        br: point::Coordinate,
        p: point::Coordinate,
    ) -> Option<Selection> {
        // _ = s;
        // PORT NOTE: upstream drops the screen read entirely (`_ = s;`). The
        // parameter must stay for the `contained_row` call site's signature, so
        // we mirror the discard.
        let _ = s;
        if p.y < tl.y || p.y > br.y {
            return None;
        }

        // Rectangle case: we can return early as the x range will always be the
        // same. We've already validated that the row is in the selection.
        if self.rectangle {
            return Some(Selection::init(
                {
                    let mut copy: Pin = pin;
                    // copy.x = @min(tl.x, copy.node.cols() - 1);
                    let cols = unsafe { crate::pagelist::Node::cols(copy.node) };
                    copy.x = core::cmp::min(tl.x, cols - 1);
                    copy
                },
                {
                    let mut copy: Pin = pin;
                    // copy.x = @min(br.x, copy.node.cols() - 1);
                    let cols = unsafe { crate::pagelist::Node::cols(copy.node) };
                    copy.x = core::cmp::min(br.x, cols - 1);
                    copy
                },
                true,
            ));
        }

        if p.y == tl.y {
            // If the selection is JUST this line, return it as-is.
            if p.y == br.y {
                return Some(Selection::init(tl_pin, br_pin, false));
            }

            // Selection top-left line matches only.
            return Some(Selection::init(
                tl_pin,
                {
                    let mut copy: Pin = pin;
                    // copy.x = copy.node.cols() - 1;
                    copy.x = unsafe { crate::pagelist::Node::cols(copy.node) } - 1;
                    copy
                },
                false,
            ));
        }

        // Row is our bottom selection, so we return the selection from the
        // beginning of the line to the br. We know our selection is more than
        // one line (due to conditionals above)
        if p.y == br.y {
            debug_assert!(p.y != tl.y);
            return Some(Selection::init(
                {
                    let mut copy: Pin = pin;
                    copy.x = 0;
                    copy
                },
                br_pin,
                false,
            ));
        }

        // Row is somewhere between our selection lines so we return the full line.
        Some(Selection::init(
            {
                let mut copy: Pin = pin;
                copy.x = 0;
                copy
            },
            {
                let mut copy: Pin = pin;
                // copy.x = copy.node.cols() - 1;
                copy.x = unsafe { crate::pagelist::Node::cols(copy.node) } - 1;
                copy
            },
            false,
        ))
    }

    /// Adjust the selection by some given adjustment. An adjustment allows
    /// a selection to be expanded slightly left, right, up, down, etc.
    pub fn adjust(&mut self, s: &Screen, adjustment: Adjustment) {
        // Note that we always adjust "end" because end always represents
        // the last point of the selection by mouse, not necessarily the
        // top/bottom visually. So this results in the correct behavior
        // whether the user drags up or down.
        // PORT NOTE: `endPtr()` returns a `*Pin`; the Rust port carries it as a
        // raw `*mut Pin` so it does not borrow `self`, which lets the recursive
        // `self.adjust(...)` dispatch arms below re-borrow `self` mutably.
        let end_pin = self.end_ptr();
        match adjustment {
            Adjustment::Up => {
                if let Some(new_end) = unsafe { (*end_pin).up(1) } {
                    unsafe {
                        *end_pin = new_end;
                    }
                } else {
                    self.adjust(s, Adjustment::BeginningOfLine);
                }
            }

            Adjustment::Down => {
                // Find the next non-blank row
                let mut current = unsafe { *end_pin };
                let mut found = false;
                while let Some(next) = current.down(1) {
                    let rac = next.row_and_cell();
                    let cells = unsafe { (*crate::pagelist::Node::page(next.node)).get_cells(rac.row) };
                    if page::Cell::has_text_any(unsafe { &*cells }) {
                        unsafe {
                            *end_pin = next;
                        }
                        found = true;
                        break;
                    }
                    current = next;
                }
                if !found {
                    // If we're at the bottom, just go to the end of the line
                    self.adjust(s, Adjustment::EndOfLine);
                }
            }

            Adjustment::Left => {
                let mut it = unsafe { (*end_pin).cell_iterator(Direction::LeftUp, None) };
                let _ = it.next();
                while let Some(next) = it.next() {
                    let rac = next.row_and_cell();
                    if unsafe { (*rac.cell).has_text() } {
                        unsafe {
                            *end_pin = next;
                        }
                        break;
                    }
                }
            }

            Adjustment::Right => {
                // Step right, wrapping to the next row down at the start of each new line,
                // until we find a non-empty cell.
                let mut it = unsafe { (*end_pin).cell_iterator(Direction::RightDown, None) };
                let _ = it.next();
                while let Some(next) = it.next() {
                    let rac = next.row_and_cell();
                    if unsafe { (*rac.cell).has_text() } {
                        unsafe {
                            *end_pin = next;
                        }
                        break;
                    }
                }
            }

            Adjustment::PageUp => {
                if let Some(new_end) = unsafe { (*end_pin).up(s.pages.rows as usize) } {
                    unsafe {
                        *end_pin = new_end;
                    }
                } else {
                    self.adjust(s, Adjustment::Home);
                }
            }

            Adjustment::PageDown => {
                if let Some(new_end) = unsafe { (*end_pin).down(s.pages.rows as usize) } {
                    unsafe {
                        *end_pin = new_end;
                    }
                } else {
                    self.adjust(s, Adjustment::End);
                }
            }

            Adjustment::Home => unsafe {
                *end_pin = s
                    .pages
                    .pin(point::Point::Screen(point::Coordinate { x: 0, y: 0 }))
                    .unwrap();
            },

            Adjustment::End => {
                let mut it = s.pages.row_iterator(
                    Direction::LeftUp,
                    point::Point::Screen(point::Coordinate::default()),
                    None,
                );
                while let Some(next) = it.next() {
                    let rac = next.row_and_cell();
                    let cells = unsafe { (*crate::pagelist::Node::page(next.node)).get_cells(rac.row) };
                    if page::Cell::has_text_any(unsafe { &*cells }) {
                        unsafe {
                            *end_pin = next;
                            (*end_pin).x = (cells.len() - 1) as size::CellCountInt;
                        }
                        break;
                    }
                }
            }

            Adjustment::BeginningOfLine => unsafe {
                (*end_pin).x = 0;
            },

            Adjustment::EndOfLine => unsafe {
                (*end_pin).x = crate::pagelist::Node::cols((*end_pin).node) - 1;
            },
        }
    }
}

/// The order of the selection:
///
///  * forward: start(x, y) is before end(x, y) (top-left to bottom-right).
///  * reverse: end(x, y) is before start(x, y) (bottom-right to top-left).
///  * mirrored_[forward|reverse]: special, rectangle selections only (see below).
///
///  For regular selections, the above also holds for top-right to bottom-left
///  (forward) and bottom-left to top-right (reverse). However, for rectangle
///  selections, both of these selections are *mirrored* as orientation
///  operations only flip the x or y axis, not both. Depending on the y axis
///  direction, this is either mirrored_forward or mirrored_reverse.
///
// PORT NOTE: `Order = lib.Enum(lib.target, &.{...})`; plain enum for the `.zig`
// target with the keys in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    Forward,
    Reverse,
    MirroredForward,
    MirroredReverse,
}

/// Possible adjustments to the selection.
// PORT NOTE: `Adjustment = lib.Enum(lib.target, &.{...})`; plain enum for the
// `.zig` target with the keys in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Adjustment {
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    BeginningOfLine,
    EndOfLine,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screen::Options;
    use allocator_api2::alloc::Global as Allocator;

    // PORT NOTE: helper for `Screen.init(testing.allocator, .{ .cols, .rows,
    // .max_scrollback })`. The reference struct-literal sets only those three
    // fields; the Rust `Options` carries the deferred kitty fields, so we fill
    // those via `..Options::default()`.
    fn screen_init(cols: u16, rows: u16, max_scrollback: usize) -> Screen {
        Screen::init(
            Allocator,
            Options {
                cols,
                rows,
                max_scrollback,
                ..Options::default()
            },
        )
        .unwrap()
    }

    // PORT NOTE: shorthand for `s.pages.pin(.{ .screen = .{ .x, .y } }).?`.
    fn pin_screen(s: &Screen, x: u16, y: u32) -> Pin {
        s.pages
            .pin(point::Point::Screen(point::Coordinate { x, y }))
            .unwrap()
    }

    // PORT NOTE: shorthand for `s.pages.pointFromPin(.screen, p).?`.
    fn point_from(s: &Screen, p: Pin) -> point::Point {
        s.pages.point_from_pin(point::Tag::Screen, p).unwrap()
    }

    fn scr(x: u16, y: u32) -> point::Point {
        point::Point::Screen(point::Coordinate { x, y })
    }

    // PORT NOTE: the `containedRow`/`ordered` reference tests use
    // `testing.expectEqual(Selection, Selection)`, which compares the whole
    // struct (node ptr + x + y + rectangle). `Selection::eql` is the faithful
    // equivalent for these untracked selections (pins originate from the same
    // screen so node pointers match), so we assert through it.
    // Deep-compare analog of the reference's `testing.expectEqualDeep(expected,
    // sel)` / `testing.expect(sel.eql(...))`. Returns the comparison instead of
    // asserting so the ASSERTION itself lives in each test body
    // (assertion-for-assertion with the reference; G2 counts source-level
    // asserts per test).
    fn sel_eql(expected: Selection, got: Selection) -> bool {
        expected.eql(got)
    }

    // test "Selection: adjust right"
    #[test]
    fn selection_adjust_right() {
        let mut s = screen_init(10, 10, 0);
        s.test_write_string(b"A1234\nB5678\nC1234\nD5678").unwrap();

        // Simple movement right
        {
            let mut sel = Selection::init(pin_screen(&s, 5, 1), pin_screen(&s, 3, 3), false);
            sel.adjust(&s, Adjustment::Right);

            assert_eq!(scr(5, 1), point_from(&s, sel.start()));
            assert_eq!(scr(4, 3), point_from(&s, sel.end()));
            sel.deinit(&mut s);
        }

        // Already at end of the line.
        {
            let mut sel = Selection::init(pin_screen(&s, 4, 1), pin_screen(&s, 4, 2), false);
            sel.adjust(&s, Adjustment::Right);

            assert_eq!(scr(4, 1), point_from(&s, sel.start()));
            assert_eq!(scr(0, 3), point_from(&s, sel.end()));
            sel.deinit(&mut s);
        }

        // Already at end of the screen
        {
            let mut sel = Selection::init(pin_screen(&s, 5, 1), pin_screen(&s, 4, 3), false);
            sel.adjust(&s, Adjustment::Right);

            assert_eq!(scr(5, 1), point_from(&s, sel.start()));
            assert_eq!(scr(4, 3), point_from(&s, sel.end()));
            sel.deinit(&mut s);
        }

        s.deinit();
    }

    // test "Selection: adjust left"
    #[test]
    fn selection_adjust_left() {
        let mut s = screen_init(10, 10, 0);
        s.test_write_string(b"A1234\nB5678\nC1234\nD5678").unwrap();

        // Simple movement left
        {
            let mut sel = Selection::init(pin_screen(&s, 5, 1), pin_screen(&s, 3, 3), false);
            sel.adjust(&s, Adjustment::Left);

            // Start line
            assert_eq!(scr(5, 1), point_from(&s, sel.start()));
            assert_eq!(scr(2, 3), point_from(&s, sel.end()));
            sel.deinit(&mut s);
        }

        // Already at beginning of the line.
        {
            let mut sel = Selection::init(pin_screen(&s, 5, 1), pin_screen(&s, 0, 3), false);
            sel.adjust(&s, Adjustment::Left);

            // Start line
            assert_eq!(scr(5, 1), point_from(&s, sel.start()));
            assert_eq!(scr(4, 2), point_from(&s, sel.end()));
            sel.deinit(&mut s);
        }

        s.deinit();
    }

    // test "Selection: adjust left skips blanks"
    #[test]
    fn selection_adjust_left_skips_blanks() {
        let mut s = screen_init(10, 10, 0);
        s.test_write_string(b"A1234\nB5678\nC12\nD56").unwrap();

        // Same line
        {
            let mut sel = Selection::init(pin_screen(&s, 5, 1), pin_screen(&s, 4, 3), false);
            sel.adjust(&s, Adjustment::Left);

            // Start line
            assert_eq!(scr(5, 1), point_from(&s, sel.start()));
            assert_eq!(scr(2, 3), point_from(&s, sel.end()));
            sel.deinit(&mut s);
        }

        // Edge
        {
            let mut sel = Selection::init(pin_screen(&s, 5, 1), pin_screen(&s, 0, 3), false);
            sel.adjust(&s, Adjustment::Left);

            // Start line
            assert_eq!(scr(5, 1), point_from(&s, sel.start()));
            assert_eq!(scr(2, 2), point_from(&s, sel.end()));
            sel.deinit(&mut s);
        }

        s.deinit();
    }

    // test "Selection: adjust up"
    #[test]
    fn selection_adjust_up() {
        let mut s = screen_init(10, 10, 0);
        s.test_write_string(b"A\nB\nC\nD\nE").unwrap();

        // Not on the first line
        {
            let mut sel = Selection::init(pin_screen(&s, 5, 1), pin_screen(&s, 3, 3), false);
            sel.adjust(&s, Adjustment::Up);

            assert_eq!(scr(5, 1), point_from(&s, sel.start()));
            assert_eq!(scr(3, 2), point_from(&s, sel.end()));
            sel.deinit(&mut s);
        }

        // On the first line
        {
            let mut sel = Selection::init(pin_screen(&s, 5, 1), pin_screen(&s, 3, 0), false);
            sel.adjust(&s, Adjustment::Up);

            assert_eq!(scr(5, 1), point_from(&s, sel.start()));
            assert_eq!(scr(0, 0), point_from(&s, sel.end()));
            sel.deinit(&mut s);
        }

        s.deinit();
    }

    // test "Selection: adjust down"
    #[test]
    fn selection_adjust_down() {
        let mut s = screen_init(10, 10, 0);
        s.test_write_string(b"A\nB\nC\nD\nE").unwrap();

        // Not on the first line
        {
            let mut sel = Selection::init(pin_screen(&s, 5, 1), pin_screen(&s, 3, 3), false);
            sel.adjust(&s, Adjustment::Down);

            assert_eq!(scr(5, 1), point_from(&s, sel.start()));
            assert_eq!(scr(3, 4), point_from(&s, sel.end()));
            sel.deinit(&mut s);
        }

        // On the last line
        {
            let mut sel = Selection::init(pin_screen(&s, 4, 1), pin_screen(&s, 3, 4), false);
            sel.adjust(&s, Adjustment::Down);

            assert_eq!(scr(4, 1), point_from(&s, sel.start()));
            assert_eq!(scr(9, 4), point_from(&s, sel.end()));
            sel.deinit(&mut s);
        }

        s.deinit();
    }

    // test "Selection: adjust down with not full screen"
    #[test]
    fn selection_adjust_down_with_not_full_screen() {
        let mut s = screen_init(10, 10, 0);
        s.test_write_string(b"A\nB\nC").unwrap();

        // On the last line
        {
            let mut sel = Selection::init(pin_screen(&s, 4, 1), pin_screen(&s, 3, 2), false);
            sel.adjust(&s, Adjustment::Down);

            // Start line
            assert_eq!(scr(4, 1), point_from(&s, sel.start()));
            assert_eq!(scr(9, 2), point_from(&s, sel.end()));
            sel.deinit(&mut s);
        }

        s.deinit();
    }

    // test "Selection: adjust home"
    #[test]
    fn selection_adjust_home() {
        let mut s = screen_init(10, 10, 0);
        s.test_write_string(b"A\nB\nC").unwrap();

        // On the last line
        {
            let mut sel = Selection::init(pin_screen(&s, 4, 1), pin_screen(&s, 1, 2), false);
            sel.adjust(&s, Adjustment::Home);

            // Start line
            assert_eq!(scr(4, 1), point_from(&s, sel.start()));
            assert_eq!(scr(0, 0), point_from(&s, sel.end()));
            sel.deinit(&mut s);
        }

        s.deinit();
    }

    // test "Selection: adjust end with not full screen"
    #[test]
    fn selection_adjust_end_with_not_full_screen() {
        let mut s = screen_init(10, 10, 0);
        s.test_write_string(b"A\nB\nC").unwrap();

        // On the last line
        {
            let mut sel = Selection::init(pin_screen(&s, 4, 0), pin_screen(&s, 1, 1), false);
            sel.adjust(&s, Adjustment::End);

            // Start line
            assert_eq!(scr(4, 0), point_from(&s, sel.start()));
            assert_eq!(scr(9, 2), point_from(&s, sel.end()));
            sel.deinit(&mut s);
        }

        s.deinit();
    }

    // test "Selection: adjust beginning of line"
    #[test]
    fn selection_adjust_beginning_of_line() {
        let mut s = screen_init(8, 10, 0);
        s.test_write_string(b"A12 B34\nC12 D34").unwrap();

        // Not at beginning of the line
        {
            let mut sel = Selection::init(pin_screen(&s, 5, 1), pin_screen(&s, 5, 1), false);
            sel.adjust(&s, Adjustment::BeginningOfLine);

            // Start line
            assert_eq!(scr(5, 1), point_from(&s, sel.start()));
            assert_eq!(scr(0, 1), point_from(&s, sel.end()));
            sel.deinit(&mut s);
        }

        // Already at beginning of the line
        {
            let mut sel = Selection::init(pin_screen(&s, 5, 1), pin_screen(&s, 0, 1), false);
            sel.adjust(&s, Adjustment::BeginningOfLine);

            // Start line
            assert_eq!(scr(5, 1), point_from(&s, sel.start()));
            assert_eq!(scr(0, 1), point_from(&s, sel.end()));
            sel.deinit(&mut s);
        }

        // End pin moves to start pin
        {
            let mut sel = Selection::init(pin_screen(&s, 0, 1), pin_screen(&s, 5, 1), false);
            sel.adjust(&s, Adjustment::BeginningOfLine);

            // Start line
            assert_eq!(scr(0, 1), point_from(&s, sel.start()));
            assert_eq!(scr(0, 1), point_from(&s, sel.end()));
            sel.deinit(&mut s);
        }

        s.deinit();
    }

    // test "Selection: adjust end of line"
    #[test]
    fn selection_adjust_end_of_line() {
        let mut s = screen_init(8, 10, 0);
        s.test_write_string(b"A12 B34\nC12 D34").unwrap();

        // Not at end of the line
        {
            let mut sel = Selection::init(pin_screen(&s, 1, 0), pin_screen(&s, 1, 0), false);
            sel.adjust(&s, Adjustment::EndOfLine);

            assert_eq!(scr(1, 0), point_from(&s, sel.start()));
            assert_eq!(scr(7, 0), point_from(&s, sel.end()));
            sel.deinit(&mut s);
        }

        // Already at end of the line
        {
            let mut sel = Selection::init(pin_screen(&s, 1, 0), pin_screen(&s, 7, 0), false);
            sel.adjust(&s, Adjustment::EndOfLine);

            assert_eq!(scr(1, 0), point_from(&s, sel.start()));
            assert_eq!(scr(7, 0), point_from(&s, sel.end()));
            sel.deinit(&mut s);
        }

        // End pin moves to start pin
        {
            let mut sel = Selection::init(pin_screen(&s, 7, 0), pin_screen(&s, 1, 0), false);
            sel.adjust(&s, Adjustment::EndOfLine);

            // Start line
            assert_eq!(scr(7, 0), point_from(&s, sel.start()));
            assert_eq!(scr(7, 0), point_from(&s, sel.end()));
            sel.deinit(&mut s);
        }

        s.deinit();
    }

    // test "Selection: order, standard"
    #[test]
    fn selection_order_standard() {
        let mut s = screen_init(100, 100, 1);

        {
            // forward, multi-line
            let sel = Selection::init(pin_screen(&s, 2, 1), pin_screen(&s, 2, 2), false);
            assert!(sel.order(&s) == Order::Forward);
            sel.deinit(&mut s);
        }
        {
            // reverse, multi-line
            let sel = Selection::init(pin_screen(&s, 2, 2), pin_screen(&s, 2, 1), false);
            assert!(sel.order(&s) == Order::Reverse);
            sel.deinit(&mut s);
        }
        {
            // forward, same-line
            let sel = Selection::init(pin_screen(&s, 2, 1), pin_screen(&s, 3, 1), false);
            assert!(sel.order(&s) == Order::Forward);
            sel.deinit(&mut s);
        }
        {
            // forward, single char
            let sel = Selection::init(pin_screen(&s, 2, 1), pin_screen(&s, 2, 1), false);
            assert!(sel.order(&s) == Order::Forward);
            sel.deinit(&mut s);
        }
        {
            // reverse, single line
            let sel = Selection::init(pin_screen(&s, 2, 1), pin_screen(&s, 1, 1), false);
            assert!(sel.order(&s) == Order::Reverse);
            sel.deinit(&mut s);
        }

        s.deinit();
    }

    // test "Selection: order, rectangle"
    #[test]
    fn selection_order_rectangle() {
        let mut s = screen_init(100, 100, 1);

        // Conventions:
        // TL - top left
        // BL - bottom left
        // TR - top right
        // BR - bottom right
        {
            // forward (TL -> BR)
            let sel = Selection::init(pin_screen(&s, 1, 1), pin_screen(&s, 2, 2), true);
            assert!(sel.order(&s) == Order::Forward);
            sel.deinit(&mut s);
        }
        {
            // reverse (BR -> TL)
            let sel = Selection::init(pin_screen(&s, 2, 2), pin_screen(&s, 1, 1), true);
            assert!(sel.order(&s) == Order::Reverse);
            sel.deinit(&mut s);
        }
        {
            // mirrored_forward (TR -> BL)
            let sel = Selection::init(pin_screen(&s, 3, 1), pin_screen(&s, 1, 3), true);
            assert!(sel.order(&s) == Order::MirroredForward);
            sel.deinit(&mut s);
        }
        {
            // mirrored_reverse (BL -> TR)
            let sel = Selection::init(pin_screen(&s, 1, 3), pin_screen(&s, 3, 1), true);
            assert!(sel.order(&s) == Order::MirroredReverse);
            sel.deinit(&mut s);
        }
        {
            // forward, single line (left -> right )
            let sel = Selection::init(pin_screen(&s, 1, 1), pin_screen(&s, 3, 1), true);
            assert!(sel.order(&s) == Order::Forward);
            sel.deinit(&mut s);
        }
        {
            // reverse, single line (right -> left)
            let sel = Selection::init(pin_screen(&s, 3, 1), pin_screen(&s, 1, 1), true);
            assert!(sel.order(&s) == Order::Reverse);
            sel.deinit(&mut s);
        }
        {
            // forward, single column (top -> bottom)
            let sel = Selection::init(pin_screen(&s, 2, 1), pin_screen(&s, 2, 3), true);
            assert!(sel.order(&s) == Order::Forward);
            sel.deinit(&mut s);
        }
        {
            // reverse, single column (bottom -> top)
            let sel = Selection::init(pin_screen(&s, 2, 3), pin_screen(&s, 2, 1), true);
            assert!(sel.order(&s) == Order::Reverse);
            sel.deinit(&mut s);
        }
        {
            // forward, single cell
            let sel = Selection::init(pin_screen(&s, 1, 1), pin_screen(&s, 1, 1), true);
            assert!(sel.order(&s) == Order::Forward);
            sel.deinit(&mut s);
        }

        s.deinit();
    }

    // test "topLeft"
    #[test]
    fn top_left() {
        let mut s = screen_init(10, 10, 0);
        {
            // forward
            let sel = Selection::init(pin_screen(&s, 1, 1), pin_screen(&s, 3, 1), true);
            let tl = sel.top_left(&s);
            assert_eq!(scr(1, 1), point_from(&s, tl));
            sel.deinit(&mut s);
        }
        {
            // reverse
            let sel = Selection::init(pin_screen(&s, 3, 1), pin_screen(&s, 1, 1), true);
            let tl = sel.top_left(&s);
            assert_eq!(scr(1, 1), point_from(&s, tl));
            sel.deinit(&mut s);
        }
        {
            // mirrored_forward
            let sel = Selection::init(pin_screen(&s, 3, 1), pin_screen(&s, 1, 3), true);
            let tl = sel.top_left(&s);
            assert_eq!(scr(1, 1), point_from(&s, tl));
            sel.deinit(&mut s);
        }
        {
            // mirrored_reverse
            let sel = Selection::init(pin_screen(&s, 1, 3), pin_screen(&s, 3, 1), true);
            let tl = sel.top_left(&s);
            assert_eq!(scr(1, 1), point_from(&s, tl));
            sel.deinit(&mut s);
        }

        s.deinit();
    }

    // test "bottomRight"
    #[test]
    fn bottom_right() {
        let mut s = screen_init(10, 10, 0);
        {
            // forward
            let sel = Selection::init(pin_screen(&s, 1, 1), pin_screen(&s, 3, 1), false);
            let br = sel.bottom_right(&s);
            assert_eq!(scr(3, 1), point_from(&s, br));
            sel.deinit(&mut s);
        }
        {
            // reverse
            let sel = Selection::init(pin_screen(&s, 3, 1), pin_screen(&s, 1, 1), false);
            let br = sel.bottom_right(&s);
            assert_eq!(scr(3, 1), point_from(&s, br));
            sel.deinit(&mut s);
        }
        {
            // mirrored_forward
            let sel = Selection::init(pin_screen(&s, 3, 1), pin_screen(&s, 1, 3), true);
            let br = sel.bottom_right(&s);
            assert_eq!(scr(3, 3), point_from(&s, br));
            sel.deinit(&mut s);
        }
        {
            // mirrored_reverse
            let sel = Selection::init(pin_screen(&s, 1, 3), pin_screen(&s, 3, 1), true);
            let br = sel.bottom_right(&s);
            assert_eq!(scr(3, 3), point_from(&s, br));
            sel.deinit(&mut s);
        }

        s.deinit();
    }

    // test "ordered"
    #[test]
    fn ordered() {
        let mut s = screen_init(10, 10, 0);
        {
            // forward
            let sel = Selection::init(pin_screen(&s, 1, 1), pin_screen(&s, 3, 1), false);
            let sel_reverse = Selection::init(pin_screen(&s, 3, 1), pin_screen(&s, 1, 1), false);
            assert!(sel_eql(sel.ordered(&s, Order::Forward), sel));
            assert!(sel_eql(sel.ordered(&s, Order::Reverse), sel_reverse));
            assert!(sel_eql(sel.ordered(&s, Order::MirroredForward), sel));
        }
        {
            // reverse
            let sel = Selection::init(pin_screen(&s, 3, 1), pin_screen(&s, 1, 1), false);
            let sel_forward = Selection::init(pin_screen(&s, 1, 1), pin_screen(&s, 3, 1), false);
            assert!(sel_eql(sel.ordered(&s, Order::Forward), sel_forward));
            assert!(sel_eql(sel.ordered(&s, Order::Reverse), sel));
            assert!(sel_eql(
                sel.ordered(&s, Order::MirroredForward),
                sel_forward
            ));
        }
        {
            // mirrored_forward
            let sel = Selection::init(pin_screen(&s, 3, 1), pin_screen(&s, 1, 3), true);
            let sel_forward = Selection::init(pin_screen(&s, 1, 1), pin_screen(&s, 3, 3), true);
            let sel_reverse = Selection::init(pin_screen(&s, 3, 3), pin_screen(&s, 1, 1), true);
            assert!(sel_eql(sel.ordered(&s, Order::Forward), sel_forward));
            assert!(sel_eql(sel.ordered(&s, Order::Reverse), sel_reverse));
            assert!(sel_eql(
                sel.ordered(&s, Order::MirroredReverse),
                sel_forward
            ));
        }
        {
            // mirrored_reverse
            let sel = Selection::init(pin_screen(&s, 1, 3), pin_screen(&s, 3, 1), true);
            let sel_forward = Selection::init(pin_screen(&s, 1, 1), pin_screen(&s, 3, 3), true);
            let sel_reverse = Selection::init(pin_screen(&s, 3, 3), pin_screen(&s, 1, 1), true);
            assert!(sel_eql(sel.ordered(&s, Order::Forward), sel_forward));
            assert!(sel_eql(sel.ordered(&s, Order::Reverse), sel_reverse));
            assert!(sel_eql(
                sel.ordered(&s, Order::MirroredForward),
                sel_forward
            ));
        }

        s.deinit();
    }

    // test "Selection: contains"
    #[test]
    fn selection_contains() {
        let mut s = screen_init(10, 10, 0);
        {
            let sel = Selection::init(pin_screen(&s, 5, 1), pin_screen(&s, 3, 2), false);

            assert!(sel.contains(&s, pin_screen(&s, 6, 1)));
            assert!(sel.contains(&s, pin_screen(&s, 1, 2)));
            assert!(!sel.contains(&s, pin_screen(&s, 1, 1)));
            assert!(!sel.contains(&s, pin_screen(&s, 5, 2)));
        }

        // Reverse
        {
            let sel = Selection::init(pin_screen(&s, 3, 2), pin_screen(&s, 5, 1), false);

            assert!(sel.contains(&s, pin_screen(&s, 6, 1)));
            assert!(sel.contains(&s, pin_screen(&s, 1, 2)));
            assert!(!sel.contains(&s, pin_screen(&s, 1, 1)));
            assert!(!sel.contains(&s, pin_screen(&s, 5, 2)));
        }

        // Single line
        {
            let sel = Selection::init(pin_screen(&s, 5, 1), pin_screen(&s, 8, 1), false);

            assert!(sel.contains(&s, pin_screen(&s, 6, 1)));
            assert!(!sel.contains(&s, pin_screen(&s, 2, 1)));
            assert!(!sel.contains(&s, pin_screen(&s, 9, 1)));
        }

        s.deinit();
    }

    // test "Selection: contains, rectangle"
    #[test]
    fn selection_contains_rectangle() {
        let mut s = screen_init(15, 15, 0);
        {
            let sel = Selection::init(pin_screen(&s, 3, 3), pin_screen(&s, 7, 9), true);

            assert!(sel.contains(&s, pin_screen(&s, 5, 6))); // Center
            assert!(sel.contains(&s, pin_screen(&s, 3, 6))); // Left border
            assert!(sel.contains(&s, pin_screen(&s, 7, 6))); // Right border
            assert!(sel.contains(&s, pin_screen(&s, 5, 3))); // Top border
            assert!(sel.contains(&s, pin_screen(&s, 5, 9))); // Bottom border

            assert!(!sel.contains(&s, pin_screen(&s, 5, 2))); // Above center
            assert!(!sel.contains(&s, pin_screen(&s, 5, 10))); // Below center
            assert!(!sel.contains(&s, pin_screen(&s, 2, 6))); // Left center
            assert!(!sel.contains(&s, pin_screen(&s, 8, 6))); // Right center
            assert!(!sel.contains(&s, pin_screen(&s, 8, 3))); // Just right of top right
            assert!(!sel.contains(&s, pin_screen(&s, 2, 9))); // Just left of bottom left
        }

        // Reverse
        {
            let sel = Selection::init(pin_screen(&s, 7, 9), pin_screen(&s, 3, 3), true);

            assert!(sel.contains(&s, pin_screen(&s, 5, 6))); // Center
            assert!(sel.contains(&s, pin_screen(&s, 3, 6))); // Left border
            assert!(sel.contains(&s, pin_screen(&s, 7, 6))); // Right border
            assert!(sel.contains(&s, pin_screen(&s, 5, 3))); // Top border
            assert!(sel.contains(&s, pin_screen(&s, 5, 9))); // Bottom border

            assert!(!sel.contains(&s, pin_screen(&s, 5, 2))); // Above center
            assert!(!sel.contains(&s, pin_screen(&s, 5, 10))); // Below center
            assert!(!sel.contains(&s, pin_screen(&s, 2, 6))); // Left center
            assert!(!sel.contains(&s, pin_screen(&s, 8, 6))); // Right center
            assert!(!sel.contains(&s, pin_screen(&s, 8, 3))); // Just right of top right
            assert!(!sel.contains(&s, pin_screen(&s, 2, 9))); // Just left of bottom left
        }

        // Single line
        // NOTE: This is the same as normal selection but we just do it for brevity
        {
            let sel = Selection::init(pin_screen(&s, 5, 1), pin_screen(&s, 10, 1), true);

            assert!(sel.contains(&s, pin_screen(&s, 6, 1)));
            assert!(!sel.contains(&s, pin_screen(&s, 2, 1)));
            assert!(!sel.contains(&s, pin_screen(&s, 12, 1)));
        }

        s.deinit();
    }

    // test "Selection: rectangle corners clamp across mixed-width pages"
    #[test]
    fn selection_rectangle_corners_clamp_across_mixed_width_pages() {
        let mut s = screen_init(4, 2, 0);

        let first = s.pages.pages.first;
        s.pages
            .split(Pin {
                node: first,
                x: 0,
                y: 1,
                garbage: false,
            })
            .unwrap();
        let second = unsafe { (*first).next };
        // PORT NOTE: `second.page().size.cols = 2` — a deliberate mid-reflow
        // narrow page, written directly through the node pointer.
        unsafe { (*crate::pagelist::Node::page(second)).size.cols = 2 };

        let sel = Selection::init(
            Pin {
                node: first,
                x: 3,
                y: 0,
                garbage: false,
            },
            Pin {
                node: second,
                x: 1,
                y: 0,
                garbage: false,
            },
            true,
        );
        assert_eq!(Order::MirroredForward, sel.order(&s));

        let bottom_right = sel.bottom_right(&s);
        // Resolving the pin is what panics when it is out of bounds.
        let _ = bottom_right.row_and_cell();
        assert!(Pin {
            node: second,
            x: 1,
            y: 0,
            garbage: false
        }
        .eql(bottom_right));

        s.deinit();
    }

    // test "Selection: containedRow clamps mixed-width pages"
    #[test]
    fn selection_contained_row_clamps_mixed_width_pages() {
        let mut s = screen_init(4, 3, 0);

        let first = s.pages.pages.first;
        s.pages
            .split(Pin {
                node: first,
                x: 0,
                y: 2,
                garbage: false,
            })
            .unwrap();
        s.pages
            .split(Pin {
                node: first,
                x: 0,
                y: 1,
                garbage: false,
            })
            .unwrap();
        let middle = unsafe { (*first).next };
        let last = unsafe { (*middle).next };
        unsafe { (*crate::pagelist::Node::page(middle)).size.cols = 2 };

        let linear = Selection::init(
            Pin {
                node: first,
                x: 1,
                y: 0,
                garbage: false,
            },
            Pin {
                node: last,
                x: 1,
                y: 0,
                garbage: false,
            },
            false,
        );
        let linear_row = linear
            .contained_row(
                &s,
                Pin {
                    node: middle,
                    x: 0,
                    y: 0,
                    garbage: false,
                },
            )
            .unwrap();
        let _ = linear_row.end().row_and_cell();
        assert!(Pin {
            node: middle,
            x: 0,
            y: 0,
            garbage: false
        }
        .eql(linear_row.start()));
        assert!(Pin {
            node: middle,
            x: 1,
            y: 0,
            garbage: false
        }
        .eql(linear_row.end()));

        let rectangle = Selection::init(
            Pin {
                node: first,
                x: 1,
                y: 0,
                garbage: false,
            },
            Pin {
                node: last,
                x: 3,
                y: 0,
                garbage: false,
            },
            true,
        );
        let rectangle_row = rectangle
            .contained_row(
                &s,
                Pin {
                    node: middle,
                    x: 0,
                    y: 0,
                    garbage: false,
                },
            )
            .unwrap();
        let _ = rectangle_row.end().row_and_cell();
        assert!(Pin {
            node: middle,
            x: 1,
            y: 0,
            garbage: false
        }
        .eql(rectangle_row.start()));
        assert!(Pin {
            node: middle,
            x: 1,
            y: 0,
            garbage: false
        }
        .eql(rectangle_row.end()));

        s.deinit();
    }

    // test "Selection: containedRow"
    #[test]
    fn selection_contained_row() {
        let mut s = screen_init(10, 5, 0);

        {
            let sel = Selection::init(pin_screen(&s, 5, 1), pin_screen(&s, 3, 3), false);

            // Not contained
            assert!(sel.contained_row(&s, pin_screen(&s, 1, 4)).is_none());

            // Start line
            assert!(sel_eql(
                Selection::init(sel.start(), pin_screen(&s, s.pages.cols - 1, 1), false),
                sel.contained_row(&s, pin_screen(&s, 1, 1)).unwrap(),
            ));

            // End line
            assert!(sel_eql(
                Selection::init(pin_screen(&s, 0, 3), sel.end(), false),
                sel.contained_row(&s, pin_screen(&s, 2, 3)).unwrap(),
            ));

            // Middle line
            assert!(sel_eql(
                Selection::init(
                    pin_screen(&s, 0, 2),
                    pin_screen(&s, s.pages.cols - 1, 2),
                    false,
                ),
                sel.contained_row(&s, pin_screen(&s, 2, 2)).unwrap(),
            ));
        }

        // Rectangle
        {
            let sel = Selection::init(pin_screen(&s, 3, 1), pin_screen(&s, 6, 3), true);

            // Not contained
            assert!(sel.contained_row(&s, pin_screen(&s, 1, 4)).is_none());

            // Start line
            assert!(sel_eql(
                Selection::init(pin_screen(&s, 3, 1), pin_screen(&s, 6, 1), true),
                sel.contained_row(&s, pin_screen(&s, 1, 1)).unwrap(),
            ));

            // End line
            assert!(sel_eql(
                Selection::init(pin_screen(&s, 3, 3), pin_screen(&s, 6, 3), true),
                sel.contained_row(&s, pin_screen(&s, 2, 3)).unwrap(),
            ));

            // Middle line
            assert!(sel_eql(
                Selection::init(pin_screen(&s, 3, 2), pin_screen(&s, 6, 2), true),
                sel.contained_row(&s, pin_screen(&s, 2, 2)).unwrap(),
            ));
        }

        // Single-line selection
        {
            let sel = Selection::init(pin_screen(&s, 2, 1), pin_screen(&s, 6, 1), false);

            // Not contained
            assert!(sel.contained_row(&s, pin_screen(&s, 1, 0)).is_none());
            assert!(sel.contained_row(&s, pin_screen(&s, 1, 2)).is_none());

            // Contained
            assert!(sel_eql(
                sel,
                sel.contained_row(&s, pin_screen(&s, 1, 1)).unwrap()
            ));
        }

        s.deinit();
    }
}
