//! rustty-vt-c point
//!
//! PORTS: the C-ABI projection of ghostty `src/terminal/point.zig`'s
//! `Point.C`/`Point.CValue`/`Point.cval` (the `lib.TaggedUnion` surface).
//!
//! PORT STATUS: SKELETON (mutual-atom Stage B v3). The hand-authored tagged
//! union + its const size/align asserts are byte-exact; `from_c`/`cval` are
//! SIGNATURES with `todo!()` bodies, filled in a later sub-batch.
//!
//! PORT NOTE (where this lives): `point.zig` itself is a pure-VT module ported in
//! rustty-vt (`rustty_vt::point` — `Tag`, `Coordinate`, `Point`). Only the C-ABI
//! `TaggedUnion` projection (`Point.C` etc.) was DEFERRED to this crate (the
//! `.C`/`.CValue`/`.cval` decls are generated only for the C build). We author
//! it here by hand following the `lib.TaggedUnion(target, Point, [2]u64)`
//! expansion: a `#[repr(C)]` struct `{ tag: TagC, value: PointCValue }` where
//! `PointCValue` is an `extern union` of one `Coordinate` per tag plus a
//! `_padding: [u64; 2]` member. The padding is PINNED: `[2]u64` per the
//! reference comment ("largest variant is Coordinate (u16 + u32 = 6 bytes); use
//! [2]u64 (16 bytes) for future expansion") — never change it (ABI stability).

// const point = @import("../point.zig");  // Tag, Coordinate
use rustty_vt::point::{Coordinate, Tag};
use std::mem::{align_of, size_of};
use std::os::raw::c_int;

/// The C ABI compatible tag type for `Point`.
// PORT NOTE: `Point.Tag = lib.Enum(lib.target, &.{ "active", "viewport",
// "screen", "history" })`. For the C target this is a fieldless `enum(c_int)`
// with the keys in order. `rustty_vt::point::Tag` is the pure-VT plain enum;
// the C ABI needs the explicit `c_int` width, so we mirror it here as a
// `#[repr(C)]` fieldless enum (a faithful re-projection, NOT a fabrication: it
// is the byte-exact `Tag` the `lib.Enum` would emit for `.c`).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TagC {
    Active = 0,
    Viewport = 1,
    Screen = 2,
    History = 3,
}

impl TagC {
    // PORT NOTE: bridge to/from the pure-VT `point::Tag`. The discriminants are
    // identical by construction (same key order).
    pub fn from_tag(tag: Tag) -> TagC {
        match tag {
            Tag::Active => TagC::Active,
            Tag::Viewport => TagC::Viewport,
            Tag::Screen => TagC::Screen,
            Tag::History => TagC::History,
        }
    }

    pub fn to_tag(self) -> Tag {
        match self {
            TagC::Active => Tag::Active,
            TagC::Viewport => Tag::Viewport,
            TagC::Screen => Tag::Screen,
            TagC::History => Tag::History,
        }
    }
}

/// The C ABI compatible union value type for `Point` (`Point.CValue`).
// PORT NOTE: `lib.TaggedUnion` builds an `extern union` with one field per tag
// (each typed as the tag's payload, here `Coordinate`) plus a trailing
// `_padding` field. Ported as a `#[repr(C)]` union.
#[repr(C)]
#[derive(Clone, Copy)]
pub union PointCValue {
    pub active: Coordinate,
    pub viewport: Coordinate,
    pub screen: Coordinate,
    pub history: Coordinate,
    /// Padding to the PINNED ABI size. `[2]u64` (16 bytes) per the reference.
    pub _padding: [u64; 2],
}

/// C: GhosttyPoint -> RusttyPoint (R2) — the C ABI compatible tagged union.
// PORT NOTE: `Point.C = extern struct { tag: Tag, value: CValue }`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PointC {
    pub tag: TagC,
    pub value: PointCValue,
}

// BUILD-failure gate (D2): the hand-authored union MUST match the pinned
// `[2]u64` padding size and the natural alignment, and the C tag MUST be
// `c_int`-width. If any of these drift, the crate fails to compile.
const _: () = assert!(size_of::<PointCValue>() == size_of::<[u64; 2]>());
const _: () = assert!(align_of::<PointCValue>() == 8);
const _: () = assert!(size_of::<TagC>() == size_of::<c_int>());

impl PointC {
    /// Convert a native `Point` into its C ABI tagged union (`Point.cval`).
    // pub const cval = c_union.cval;
    pub fn cval(_point: rustty_vt::point::Point) -> PointC {
        todo!("vt-c point PointC::cval")
    }

    /// Convert this C ABI tagged union back into a native `Point`
    /// (`Point.fromC`).
    // PORT NOTE: `point.Point.fromC(pt)` reads `pt.tag` and the matching
    // `pt.value.<tag>` payload, dispatching to `rustty_vt::point::Point::from_c`.
    pub fn from_c(self) -> rustty_vt::point::Point {
        todo!("vt-c point PointC::from_c")
    }
}
