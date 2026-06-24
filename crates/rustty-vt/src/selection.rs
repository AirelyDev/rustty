//! rustty-vt selection
//!
//! Represents a single selection within the terminal (i.e. a highlight region).
//!
//! PORTS: ghostty src/terminal/Selection.zig
//!
//! PORT STATUS: skeleton (vt-10) — types + method signatures; bodies `todo!()`
//! land in vt-11.
// PORT NOTE: this skeleton has `todo!()` method bodies (vt-11 lands the real
// bodies), so the parameters/imports they will consume are not yet read. The
// `unused_*` and `clippy::should_implement_trait` allows are skeleton-only and
// drop out once the bodies land.
#![allow(clippy::todo, dead_code, unused_variables, unused_imports)]
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
use crate::pagelist::Pin;
use crate::point;
use crate::screen::Screen;

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
        todo!()
    }

    /// Returns true if this selection is equal to another selection.
    pub fn eql(self, other: Selection) -> bool {
        todo!()
    }

    /// The starting pin of the selection. This is NOT ordered.
    pub fn start_ptr(&mut self) -> *mut Pin {
        todo!()
    }

    /// The ending pin of the selection. This is NOT ordered.
    pub fn end_ptr(&mut self) -> *mut Pin {
        todo!()
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
        todo!()
    }

    /// Convert this selection a tracked selection. It is asserted this is
    /// an untracked selection. The tracked selection is returned.
    pub fn track(&self, s: &mut Screen) -> Result<Selection, OutOfMemory> {
        todo!()
    }

    /// Returns the top left point of the selection.
    pub fn top_left(self, s: &Screen) -> Pin {
        match self.order(s) {
            Order::Forward => self.start(),
            Order::Reverse => self.end(),
            Order::MirroredForward => {
                let mut p = self.start();
                p.x = self.end().x;
                p
            }
            Order::MirroredReverse => {
                let mut p = self.end();
                p.x = self.start().x;
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
                p.x = self.start().x;
                p
            }
            Order::MirroredReverse => {
                let mut p = self.start();
                p.x = self.end().x;
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
        todo!()
    }

    /// Returns true if the selection contains the given point.
    ///
    /// This recalculates top left and bottom right each call. If you have
    /// many points to check, it is cheaper to do the containment logic
    /// yourself and cache the topleft/bottomright.
    pub fn contains(self, s: &Screen, pin: Pin) -> bool {
        todo!()
    }

    /// Get a selection for a single row in the screen. This will return null
    /// if the row is not included in the selection.
    ///
    /// This is a very expensive operation. It has to traverse the linked list
    /// of pages for the top-left, bottom-right, and the given pin to find
    /// the coordinates. If you are calling this repeatedly, prefer
    /// `containedRowCached`.
    pub fn contained_row(self, s: &Screen, pin: Pin) -> Option<Selection> {
        todo!()
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
        todo!()
    }

    /// Adjust the selection by some given adjustment. An adjustment allows
    /// a selection to be expanded slightly left, right, up, down, etc.
    pub fn adjust(&mut self, s: &Screen, adjustment: Adjustment) {
        todo!()
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
