//! rustty-vt-c point
//!
//! PORTS: the C-ABI projection of ghostty `src/terminal/point.zig`'s
//! `Point.C`/`Point.CValue`/`Point.cval` (the `lib.TaggedUnion` surface).
//!
//! PORT STATUS: FILLED (mutual-atom Stage B v4). The hand-authored tagged
//! union + its const size/align asserts are byte-exact; `from_c`/`cval` bodies
//! are ported (the `lib.TaggedUnion` expansion). `point.zig`'s C surface has no
//! `test {}` blocks of its own (the conversions are exercised by the
//! grid_ref/selection tests).
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

/// C: RusttyPointTag (R2) — checked by the A5 enum-discriminant arm.
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
    /// Narrow an inbound C integer to a `TagC`, mirroring `std.meta.intToEnum`;
    /// `None` outside the declared range.
    ///
    /// D11 (P3-W6 T2): `TagC` is received across the C ABI both as a bare
    /// parameter (`rustty_terminal_point_from_grid_ref`,
    /// `rustty_tracked_grid_ref_point`) and as the `tag` FIELD of a by-value
    /// `PointC` (`rustty_terminal_grid_ref`, `_grid_ref_track`,
    /// `rustty_terminal_selection_contains`, `rustty_tracked_grid_ref_set`).
    /// Both forms are materialized by the callee prologue, so both take the C
    /// integer and narrow here. See `focus.rs`'s `FocusEvent::from_c_int` for
    /// the full rationale. PRE-EXISTING; not in the Phase 3 range.
    pub fn from_c_int(v: c_int) -> Option<TagC> {
        match v {
            0 => Some(TagC::Active),
            1 => Some(TagC::Viewport),
            2 => Some(TagC::Screen),
            3 => Some(TagC::History),
            _ => None,
        }
    }

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
    /// D11 (P3-W6 T2): held as the C integer, NOT as `TagC`. `PointC` is taken
    /// BY VALUE by four exports, so a `TagC`-typed field would be materialized
    /// by the callee prologue — the same UB a bare closed-enum parameter has,
    /// one level down. Narrowed by `PointC::tag()` / `PointC::from_c()`.
    pub tag: c_int,
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
    //
    // PORT NOTE: `lib.TaggedUnion(...).cval` builds the `C` struct from a native
    // tagged-union value: it reads the active tag and copies the matching payload
    // into the union's same-named member. The padding member is left as the
    // active member's bytes (the rest is `undefined` in the reference). We mirror
    // that by initializing the union through the tag's `Coordinate` member; the
    // tail bytes of the `[2]u64` padding are unobserved (the C reader dispatches
    // on `tag` and only reads the active member).
    pub fn cval(point: rustty_vt::point::Point) -> PointC {
        let tag = TagC::from_tag(point.tag());
        let coord = point.coord();
        let value = match tag {
            TagC::Active => PointCValue { active: coord },
            TagC::Viewport => PointCValue { viewport: coord },
            TagC::Screen => PointCValue { screen: coord },
            TagC::History => PointCValue { history: coord },
        };
        PointC {
            tag: tag as c_int,
            value,
        }
    }

    /// The narrowed `tag`, or `None` if the C caller wrote an undeclared value.
    /// D11 (P3-W6 T2): the single narrowing point for the `tag` field.
    #[inline]
    pub fn tag(self) -> Option<TagC> {
        TagC::from_c_int(self.tag)
    }

    /// Convert this C ABI tagged union back into a native `Point`
    /// (`Point.fromC`).
    // PORT NOTE: `point.Point.fromC(pt)` reads `pt.tag` and the matching
    // `pt.value.<tag>` payload, dispatching to `rustty_vt::point::Point::from_c`.
    ///
    /// D11 (P3-W6 T2): now returns `Option`, because `tag` is the raw C integer
    /// and an undeclared value has no `Point` to map to. Every caller returns
    /// `.invalid_value` on `None`, which is the disposition the reference's
    /// `intToEnum` guards use on this boundary.
    pub fn from_c(self) -> Option<rustty_vt::point::Point> {
        let tag = self.tag()?;
        // PORT NOTE: read the union member selected by `tag`. Every member is a
        // `Coordinate` at offset 0, so the read is the same bytes regardless of
        // tag; the dispatch keeps the C-ABI contract explicit. SAFETY: the
        // selected member is the one written by `cval`/the C caller per `tag`.
        let value = match tag {
            TagC::Active => unsafe { self.value.active },
            TagC::Viewport => unsafe { self.value.viewport },
            TagC::Screen => unsafe { self.value.screen },
            TagC::History => unsafe { self.value.history },
        };
        Some(rustty_vt::point::Point::from_c(tag.to_tag(), value))
    }
}
