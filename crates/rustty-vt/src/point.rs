//! rustty-vt point
//!
//! PORTS: ghostty src/terminal/point.zig
//!
//! PORT STATUS: done

// PARITY: the `Default` impl mirrors the reference's explicit field defaults
// (`x = 0, y = 0`); keep the explicit form.
#![allow(clippy::derivable_impls)]

// PORT NOTE: the reference imports are
//   const std = @import("std");
//   const Allocator = std.mem.Allocator;
//   const lib = @import("lib.zig");
//   const size = @import("size.zig");
// `size` is hoisted into rustty-render-types (A1); `lib.Enum`/`lib.TaggedUnion`
// for the `.zig` target produce plain Zig enums/tagged unions, which become
// idiomatic Rust enums. The C ABI surface (`lib.target == .c`) is only generated
// for the C-ABI build (rustty-vt-c) and carries no behavior of its own; the C
// helpers (`C`, `CValue`, `cval`, `fromC`) are ported below to preserve the
// reference surface for the Zig/Rust target.

use rustty_render_types::term_size as size;

/// The possible reference locations for a point. When someone says "(42, 80)"
/// in the context of a terminal, that could mean multiple things: it is in the
/// current visible viewport? the current active area of the screen where the
/// cursor is? the entire scrollback history? etc.
///
/// This tag is used to differentiate those cases.
// PORT NOTE: `Tag = lib.Enum(lib.target, &.{...})`. For the `.zig` target this
// is a plain enum with the keys in order; ported as a Rust enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tag {
    // Top-left is part of the active area where a running program can
    // jump the cursor and make changes. The active area is the "editable"
    // part of the screen.
    //
    // The bottom-right of the active tag differs from all other tags
    // because it includes the full height (rows) of the screen, including
    // rows that may not be written yet. This is required because the active
    // area is fully "addressable" by the running program (see below) whereas
    // the other tags are used primarily for reading/modifying past-written
    // data so they can't address unwritten rows.
    //
    // Note for those less familiar with terminal functionality: there
    // are escape sequences to move the cursor to any position on
    // the screen, but it is limited to the size of the viewport and
    // the bottommost part of the screen. Terminal programs can't --
    // with sequences at the time of writing this comment -- modify
    // anything in the scrollback, visible viewport (if it differs
    // from the active area), etc.
    Active,

    // Top-left is the visible viewport. This means that if the user
    // has scrolled in any direction, top-left changes. The bottom-right
    // is the last written row from the top-left.
    Viewport,

    // Top-left is the furthest back in the scrollback history
    // supported by the screen and the bottom-right is the bottom-right
    // of the last written row. Note this last point is important: the
    // bottom right is NOT necessarily the same as "active" because
    // "active" always allows referencing the full rows tall of the
    // screen whereas "screen" only contains written rows.
    Screen,

    // The top-left is the same as "screen" but the bottom-right is
    // the line just before the top of "active". This contains only
    // the scrollback history.
    History,
}

/// An x/y point in the terminal for some definition of location (tag).
// PORT NOTE: the reference is `union(Tag)` — a tagged union with one
// `Coordinate` payload per tag. Ported as a Rust enum carrying `Coordinate`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Point {
    Active(Coordinate),
    Viewport(Coordinate),
    Screen(Coordinate),
    History(Coordinate),
}

impl Point {
    #[inline]
    pub fn coord(self) -> Coordinate {
        match self {
            Point::Active(v) | Point::Viewport(v) | Point::Screen(v) | Point::History(v) => v,
        }
    }

    /// Return the tag of this point.
    // PORT NOTE: the reference relies on `@as(Tag, self)` and the
    // `union(Tag)` relationship; in Rust we provide an explicit accessor.
    pub fn tag(self) -> Tag {
        match self {
            Point::Active(_) => Tag::Active,
            Point::Viewport(_) => Tag::Viewport,
            Point::Screen(_) => Tag::Screen,
            Point::History(_) => Tag::History,
        }
    }

    // const c_union = lib.TaggedUnion(
    //     lib.target,
    //     @This(),
    //     // Padding: largest variant is Coordinate (u16 + u32 = 6 bytes).
    //     // Use [2]u64 (16 bytes) for future expansion.
    //     [2]u64,
    // );
    // pub const C = c_union.C;
    // pub const CValue = c_union.CValue;
    // pub const cval = c_union.cval;
    //
    // PORT NOTE: the C ABI tagged-union (`C`, `CValue`, `cval`) is generated only
    // for the C-ABI build and is ported in the rustty-vt-c crate; it carries no
    // Zig/Rust-target behavior. The `fromC` conversion below mirrors the
    // reference for completeness.

    /// Convert a C ABI point into the native tagged union.
    // PORT NOTE: with the C surface deferred to rustty-vt-c, `fromC` takes a
    // `(Tag, Coordinate)` pair representing the reference `C` struct's `tag` and
    // the matching `value.<tag>` payload.
    pub fn from_c(tag: Tag, value: Coordinate) -> Point {
        match tag {
            Tag::Active => Point::Active(value),
            Tag::Viewport => Point::Viewport(value),
            Tag::Screen => Point::Screen(value),
            Tag::History => Point::History(value),
        }
    }
}

// PORT NOTE: the reference is `extern struct`; ported as `#[repr(C)]` to
// preserve the field order/layout.
// PORT NOTE: `Hash` is derived (Rust-only glue) so `point::Coordinate` can key
// the render state's `CellSet` (`AutoArrayHashMap(Coordinate, void)`); it adds
// no observable behavior.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Coordinate {
    /// x can use size.CellCountInt because the number of columns
    /// can't ever be more than a valid number of columns in a Page.
    pub x: size::CellCountInt,

    /// y does not use size.CellCountInt because certain coordinate
    /// usage such as screen/history can have more rows than are possible
    /// in a single page.
    pub y: u32,
}

impl Default for Coordinate {
    // PORT NOTE: the reference field defaults are `x: ... = 0, y: u32 = 0`.
    fn default() -> Self {
        Coordinate { x: 0, y: 0 }
    }
}

impl Coordinate {
    pub fn eql(self, other: Coordinate) -> bool {
        self.x == other.x && self.y == other.y
    }
}
