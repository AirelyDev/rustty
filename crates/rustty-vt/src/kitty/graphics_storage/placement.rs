//! PORTS: ghostty src/terminal/kitty/graphics_storage.zig — the
//! `ScrollOp` / `ScrollMargins` scroll-guard types, the eviction `Candidate`,
//! and `PlacementKey` / `Placement` with its geometry (`sourceRect`,
//! `cellOffset`, `pixelSize`, `gridSize`, `rect`).

use crate::alloc::AVec;
use crate::kitty::graphics_image::{Image, Rect};
use crate::pagelist as PageList;
use crate::point;
use crate::screen::Screen;
use crate::size;
use crate::terminal::Terminal;
use rustty_util::quirks::inlineAssert as assert;

/// How the row operation behind a scrollMarginsBegin/end pair moves
/// tracked pins, which determines how much repositioning work the
/// placements need.
//
// PORT NOTE: nested `pub const ScrollOp` on `ImageStorage` upstream; hoisted
// to module level (the `Candidate` precedent).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollOp {
    /// The operation shifts the entire active window down (it
    /// creates scrollback), so every placement anchored in the
    /// active area changes active position and must be restored.
    WindowShift,

    /// The operation moves rows in place within the scrolling
    /// region: pins anchored outside the region rows keep their
    /// active position without help, so only placements anchored
    /// inside the region rows need handling. This is the common
    /// case (e.g. a status bar image beside a scrolling pane
    /// costs almost nothing per scroll).
    InPlace,
}

/// State returned by scrollMarginsBegin: the final active-area
/// position every surviving placement pin must be restored to once
/// the scrolled rows have settled. Every begin must be paired with
/// exactly one end() call after the scroll's row operations
/// complete.
//
// PORT NOTE (hazard #1, phase-5 plan): the guard holds a raw `*mut Screen`
// AND raw `*mut Pin`s while the caller's `&mut Terminal` row operations run
// between begin and end — both are dereferenced ONLY inside `end`
// (`rust-mut-args-strongly-protected`). The pins are pool-allocated tracked
// pins whose addresses are stable until `untrack_pin` (they are NOT interior
// to the placements map), and the begin/end window performs no storage
// mutation, so no pin recorded here is untracked in between (the upstream
// implicit contract, written down). The exactly-once `end()` contract is
// enforced by ownership: `end(self)` consumes the guard and `Drop` performs
// the restore, so early-error paths (`?`) mirror the Zig `defer ... end()`.
// Proven on `.port/phase-5/mirihar-scroll-margins/` (with a Miri-caught
// foil).
#[must_use = "every scrollMarginsBegin must be paired with exactly one end() after the scroll's row operations complete"]
pub struct ScrollMargins {
    pub(super) screen: *mut Screen,
    // restores: std.ArrayListUnmanaged(Restore) = .empty,
    pub(super) restores: AVec<Restore>,
}

// const Restore = struct { pin: *PageList.Pin, x: size.CellCountInt, y: u32 };
pub(super) struct Restore {
    pub(super) pin: *mut PageList::Pin,
    pub(super) x: size::CellCountInt,
    pub(super) y: u32,
}

impl ScrollMargins {
    /// Reposition every recorded placement pin to its post-scroll
    /// position and release the state. Must be called exactly
    /// once, after the scroll's row operations complete.
    //
    // PORT NOTE: consuming `self`; the body lives in `Drop` so that Zig's
    // `defer state.end()` on error paths is mirrored by the guard simply
    // dropping.
    pub fn end(self) {}
}

impl Drop for ScrollMargins {
    fn drop(&mut self) {
        for restore in &self.restores {
            // restore.pin.* = self.screen.pages.pin(.{ .active = .{
            //     .x = restore.x,
            //     .y = restore.y,
            // } }) orelse continue;
            //
            // SAFETY(raw-page-aliasing/graphics_storage): `screen` is the
            // screen owning every recorded tracked pin (scroll_margins_begin's
            // contract); the shared `pages` projection for the lookup ends
            // before the raw pin write, and the write targets a pool-owned
            // tracked pin with no live reference to it (the caller's row
            // operations have completed by the time the guard drops).
            let new_pin = match unsafe {
                (*self.screen)
                    .pages
                    .pin(point::Point::Active(point::Coordinate {
                        x: restore.x,
                        y: restore.y,
                    }))
            } {
                Some(p) => p,
                None => continue,
            };
            unsafe { *restore.pin = new_pin };
        }
        // self.restores.deinit(self.screen.alloc); — the list drops through
        // the allocator it grew with (the screen's).
    }
}

/// The allocation-failure error, mirroring Zig's `Allocator.Error`/OutOfMemory.
//
// PORT NOTE: the reference's fallible functions return `Allocator.Error!T`; in
// Rust this is `Result<T, OutOfMemory>`. Rust's `HashMap`/`Vec` own their
// storage and never surface OOM through these APIs; the reachable producers
// are `add_image` (eviction cannot free enough) and `add_placement` (the u30
// placement-count overflow, `error.OutOfMemory` upstream).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutOfMemory;

/// The candidate type used during eviction (`evictImageExcept`).
//
// PORT NOTE: the reference declares this as a function-local `const Candidate =
// struct {...}`. Rust has no local-to-method struct visible across closures,
// so it is hoisted to a private module-level struct with identical fields.
#[derive(Clone, Copy)]
pub(super) struct Candidate {
    pub(super) id: u32,
    generation: u64,

    // Lower values are evicted first:
    // 0: transient, unused
    // 1: not transient, unused
    // 2: transient, used
    // 3: not transient, used
    //
    // PORT NOTE: the reference field is `priority: u2`. Rust has no `u2`; `u8`
    // is the narrowest carrier. The value is only ever compared, never
    // bit-packed, so the widening is unobservable — and `init` can only
    // produce 0..=3.
    priority: u8,
}

impl Candidate {
    pub(super) fn init(img: &Image) -> Candidate {
        Candidate {
            id: img.id,
            generation: img.generation,
            priority: (if img.metadata.transient { 0u8 } else { 1u8 })
                + (if img.metadata.placement_count > 0 {
                    2u8
                } else {
                    0u8
                }),
        }
    }

    pub(super) fn less_than(self, rhs: Candidate) -> bool {
        if self.priority != rhs.priority {
            return self.priority < rhs.priority;
        }
        if self.generation != rhs.generation {
            return self.generation < rhs.generation;
        }
        self.id < rhs.id
    }
}

/// Every placement is uniquely identified by the image ID and the
/// placement ID. If an image ID isn't specified it is assumed to be 0.
/// Likewise, if a placement ID isn't specified it is assumed to be 0.
//
// PORT NOTE: the reference `placement_id` is a `packed struct { tag: enum(u1),
// id: u32 }`. The map only ever hashes/compares the key field-wise (it is never
// bit-cast), so a plain struct preserves all observable behavior. `Hash`/`Eq`
// give the `AutoHashMapUnmanaged` auto-context equality used by the lookups.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct PlacementKey {
    pub image_id: u32,
    pub placement_id: PlacementId,
}

impl PlacementKey {
    // pub fn eql(self: PlacementKey, other: PlacementKey) bool {
    //     return std.meta.eql(self, other);
    // }
    pub fn eql(self, other: PlacementKey) -> bool {
        self == other
    }
}

/// Internal placement IDs are assigned by us for placements created
/// without an explicit ID (p=0); external IDs are client-specified.
/// The two are separate namespaces, hence the tag.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct PlacementId {
    pub tag: PlacementIdTag,
    pub id: u32,
}

impl PlacementId {
    /// Deterministic preference order used when an operation must
    /// pick a single placement out of several (kitty uses creation
    /// order, which our map doesn't track): external IDs win over
    /// internal ones, and lower IDs win within a tag.
    pub fn preferred_over(self, other: PlacementId) -> bool {
        if self.tag != other.tag {
            return self.tag == PlacementIdTag::External;
        }
        self.id < other.id
    }
}

// tag: enum(u1) { internal, external }
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum PlacementIdTag {
    Internal,
    External,
}

// PORT NOTE: `Copy` matches the reference, which copies `Placement` values
// out of the map freely (`entry.value_ptr.*`, `resolveChain`'s `root`,
// `placeholderTarget`); the pin variant carries a raw pointer, which copies
// like the Zig `*Pin`.
#[derive(Debug, Clone, Copy)]
pub struct Placement {
    /// The location where this placement should be drawn.
    pub location: Location,

    /// Offset of the x/y from the top-left of the cell.
    pub x_offset: u32,
    pub y_offset: u32,

    /// Source rectangle for the image to pull from
    pub source_x: u32,
    pub source_y: u32,
    pub source_width: u32,
    pub source_height: u32,

    /// The columns/rows this image occupies.
    pub columns: u32,
    pub rows: u32,

    /// The z-index for this placement.
    pub z: i32,
}

/// `pub const Location = union(enum) { pin, virtual, relative }`.
#[derive(Debug, Clone, Copy)]
pub enum Location {
    /// Exactly placed on a screen pin.
    //
    // PORT NOTE: the reference is `pin: *PageList.Pin` — a pointer to a *tracked*
    // pin owned by the pagelist (created via `pages.trackPin`). We hold the same
    // raw `*mut Pin`; `deinit` hands it back via `pages.untrackPin`.
    Pin(*mut PageList::Pin),

    /// Virtual placement (U=1) for unicode placeholders.
    Virtual,

    /// Placed relative to a parent placement (P=/Q=). The
    /// placement has no screen position of its own; it is
    /// positioned at render time by resolving the parent chain
    /// to its root (see resolveChain) and therefore follows the
    /// parent through scrolls automatically. Its lifetime is
    /// tied to the parent: removing any ancestor removes it
    /// too (see removeOrphans).
    Relative(Relative),
}

/// PORT NOTE: nested as `Placement.Relative` upstream; a module-level sibling
/// here (Rust cannot nest types inside structs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Relative {
    /// The parent placement. Resolved to a concrete key when
    /// the placement is created, so the parent is guaranteed
    /// to exist at that point.
    pub parent: PlacementKey,

    /// Cell offsets from the parent's origin (H=/V=).
    pub horizontal_offset: i32,
    pub vertical_offset: i32,
}

impl Placement {
    // PORT NOTE: the reference declares per-field defaults (`x_offset: u32 = 0`,
    // etc.) so `.{ .location = ..., .columns = 10 }` is a valid literal. Rust has
    // no per-field defaults; the tests and `delete` code construct placements
    // with only a subset of fields set, so we provide a constructor mirroring the
    // common `.{ .location = ... }` literal (only `location` set, rest zero) plus
    // direct struct construction with `..Placement::with_location(loc)` where more
    // fields are set.
    pub fn with_location(location: Location) -> Placement {
        Placement {
            location,
            x_offset: 0,
            y_offset: 0,
            source_x: 0,
            source_y: 0,
            source_width: 0,
            source_height: 0,
            columns: 0,
            rows: 0,
            z: 0,
        }
    }

    // The raw `*mut Screen` arg is the established raw-page-aliasing
    // discipline (memory `rust-mut-args-strongly-protected`); the deref is
    // covered by the SAFETY contract below (the rustty-xev precedent for this
    // allow).
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn deinit(&self, s: *mut Screen) {
        match self.location {
            // .pin => |p| s.pages.untrackPin(p),
            // SAFETY(raw-page-aliasing/graphics_storage): `s` is the screen
            // owning this placement's tracked pin (the caller's contract, per
            // the `s: *mut Screen` PORT NOTE on `ImageStorage`); the `&mut`
            // projection of its `pages` field is scoped to this one call and
            // never overlaps a live borrow of `kitty_images`.
            Location::Pin(p) => unsafe { (*s).pages.untrack_pin(p) },
            // .virtual, .relative => {},
            Location::Virtual | Location::Relative(_) => {}
        }
    }

    /// Multiply two protocol-controlled values without allowing them to
    /// wrap. Placement geometry is exposed as u32, so values larger than
    /// that are represented by the largest possible value.
    // fn saturatingMul(lhs: u32, rhs: u32) u32 {
    //     return std.math.mul(u32, lhs, rhs) catch std.math.maxInt(u32);
    // }
    fn saturating_mul(lhs: u32, rhs: u32) -> u32 {
        lhs.saturating_mul(rhs)
    }

    /// Scale a dimension by an aspect ratio and round to the nearest
    /// integer. The u64 intermediate can hold the product of two u32s as
    /// well as the rounding adjustment.
    fn scale_dimension(value: u32, numerator: u32, denominator: u32) -> u32 {
        if denominator == 0 {
            return 0;
        }

        // const rounded = (@as(u64, value) * @as(u64, numerator) +
        //     @as(u64, denominator) / 2) / @as(u64, denominator);
        let rounded =
            (value as u64 * numerator as u64 + denominator as u64 / 2) / denominator as u64;
        // return std.math.cast(u32, rounded) orelse std.math.maxInt(u32);
        u32::try_from(rounded).unwrap_or(u32::MAX)
    }

    /// Returns the requested source rectangle intersected with the image.
    /// A zero width or height requests the full corresponding image
    /// dimension before intersection, as defined by the Kitty protocol.
    pub fn source_rect(&self, image: &Image) -> SourceRect {
        let x = core::cmp::min(self.source_x, image.width);
        let y = core::cmp::min(self.source_y, image.height);
        SourceRect {
            x,
            y,
            width: core::cmp::min(
                if self.source_width > 0 {
                    self.source_width
                } else {
                    image.width
                },
                image.width - x,
            ),
            height: core::cmp::min(
                if self.source_height > 0 {
                    self.source_height
                } else {
                    image.height
                },
                image.height - y,
            ),
        }
    }

    /// Returns the placement offset clamped within the first cell. Pixel
    /// geometry may temporarily be unavailable, in which case there is no
    /// valid offset within the cell.
    pub fn cell_offset(&self, t: &Terminal) -> CellOffset {
        let cell_width: u32 = t.width_px / t.cols as u32;
        let cell_height: u32 = t.height_px / t.rows as u32;

        CellOffset {
            x: if cell_width > 0 {
                core::cmp::min(self.x_offset, cell_width - 1)
            } else {
                0
            },
            y: if cell_height > 0 {
                core::cmp::min(self.y_offset, cell_height - 1)
            } else {
                0
            },
        }
    }

    /// Returns the size of this placement's image in pixels,
    /// taking into account the source rectangle, specified
    /// rows/columns, and aspect ratio.
    pub fn pixel_size(&self, image: &Image, t: &Terminal) -> PixelSize {
        let source = self.source_rect(image);
        let width = source.width;
        let height = source.height;

        // If we don't have any specified cols or rows then the placement
        // should be the native size of the image, and doesn't need to be
        // re-scaled.
        if self.columns == 0 && self.rows == 0 {
            return PixelSize { width, height };
        }

        // We calculate the size of a cell so that we can multiply
        // it by the specified cols/rows to get the correct px size.
        //
        // We assume that the width is divided evenly by the column
        // count and the height by the row count, because it should be.
        let cell_width: u32 = t.width_px / t.cols as u32;
        let cell_height: u32 = t.height_px / t.rows as u32;
        let cell_offset = self.cell_offset(t);

        // If we have a specified cols AND rows then we calculate
        // the width and height from them directly, we don't need
        // to adjust for aspect ratio.
        if self.columns > 0 && self.rows > 0 {
            let calc_width = Self::saturating_mul(cell_width, self.columns);
            let calc_height = Self::saturating_mul(cell_height, self.rows);

            return PixelSize {
                width: calc_width.saturating_sub(cell_offset.x),
                height: calc_height.saturating_sub(cell_offset.y),
            };
        }

        // Either the columns or the rows were specified, but not both,
        // so we need to calculate the other one based on the aspect ratio.

        // If only the columns were specified, we determine
        // the height of the image based on the aspect ratio.
        if self.columns > 0 {
            let calc_width =
                Self::saturating_mul(cell_width, self.columns).saturating_sub(cell_offset.x);
            let calc_height = Self::scale_dimension(calc_width, height, width);

            return PixelSize {
                width: calc_width,
                height: calc_height,
            };
        }

        // Otherwise, only the rows were specified, so we
        // determine the width based on the aspect ratio.
        {
            let calc_height =
                Self::saturating_mul(cell_height, self.rows).saturating_sub(cell_offset.y);
            let calc_width = Self::scale_dimension(calc_height, width, height);

            PixelSize {
                width: calc_width,
                height: calc_height,
            }
        }
    }

    /// Returns the size in grid cells that this placement takes up.
    pub fn grid_size(&self, image: &Image, t: &Terminal) -> GridSize {
        // If we have a specified columns and rows then this is trivial.
        if self.columns > 0 && self.rows > 0 {
            return GridSize {
                cols: self.columns,
                rows: self.rows,
            };
        }

        // Otherwise we calculate the pixel size, divide by
        // cell size, and round up to the nearest integer.
        // PORT NOTE (R7): the numerators are `+|` upstream (`e5840bb9b` — the
        // offsets are protocol-controlled and MAY saturate by design), so this
        // is `saturating_add` without a no-overflow debug_assert; `c5a3c7e2e`
        // clamps the offsets to the cell first (`cellOffset`).
        let calc_size = self.pixel_size(image, t);
        let cell_offset = self.cell_offset(t);
        GridSize {
            // std.math.divCeil(u32, calc_size.width +| cell_offset.x, t.width_px / t.cols) catch 0,
            cols: div_ceil_u32(
                calc_size.width.saturating_add(cell_offset.x),
                t.width_px / t.cols as u32,
            )
            .unwrap_or(0),
            // std.math.divCeil(u32, calc_size.height +| cell_offset.y, t.height_px / t.rows) catch 0,
            rows: div_ceil_u32(
                calc_size.height.saturating_add(cell_offset.y),
                t.height_px / t.rows as u32,
            )
            .unwrap_or(0),
        }
        // NOTE: Above `divCeil`s can only fail if the cell size is 0,
        //       in such a case it seems safe to return 0 for this.
    }

    /// Permanently clip `count` rows off the top of this placement,
    /// which currently spans `span` grid rows, by shrinking the
    /// source rectangle (the requested source rect is materialized
    /// into explicit values in the process). Used when a scroll
    /// within margins pushes the placement past the top margin.
    /// Requires 0 < count < span. Returns false if no visible
    /// source pixels would remain, in which case the placement is
    /// unmodified and should be deleted.
    pub(super) fn clip_top(&mut self, image: &Image, t: &Terminal, count: u32, span: u32) -> bool {
        assert(count > 0 && count < span);
        let src = self.source_rect(image);
        let crop: u32 = if self.rows > 0 {
            // Scaled placement: each grid row shows an equal
            // fraction of the source.
            // @intCast(@as(u64, src.height) * count / span)
            u32::try_from(src.height as u64 * count as u64 / span as u64).unwrap()
        } else {
            // Native size: each grid row shows one cell height of
            // source pixels (kitty does the same).
            Self::saturating_mul(t.height_px / t.rows as u32, count)
        };
        if crop >= src.height {
            return false;
        }
        self.source_x = src.x;
        self.source_y = src.y + crop;
        self.source_width = src.width;
        self.source_height = src.height - crop;
        if self.rows > 0 {
            self.rows -= count;
        }
        true
    }

    /// clipTop, but clipping `count` rows off the bottom.
    pub(super) fn clip_bottom(
        &mut self,
        image: &Image,
        t: &Terminal,
        count: u32,
        span: u32,
    ) -> bool {
        assert(count > 0 && count < span);
        let src = self.source_rect(image);
        // An empty source can never be clipped smaller. This also
        // guarantees we never store an explicit zero source height,
        // which would be reinterpreted as "full image".
        if src.height == 0 {
            return false;
        }
        if self.rows > 0 {
            // const crop: u32 = @intCast(@as(u64, src.height) * count / span);
            let crop: u32 = u32::try_from(src.height as u64 * count as u64 / span as u64).unwrap();
            if crop >= src.height {
                return false;
            }
            self.source_height = src.height - crop;
            self.rows -= count;
        } else {
            // The remaining rows can show this many source pixels,
            // with the first cell losing cellOffset pixels to the
            // placement's y offset.
            let visible = Self::saturating_mul(t.height_px / t.rows as u32, span - count);
            let offset = self.cell_offset(t).y;
            if visible <= offset {
                return false;
            }
            self.source_height = core::cmp::min(src.height, visible - offset);
        }
        self.source_x = src.x;
        self.source_y = src.y;
        self.source_width = src.width;
        true
    }

    /// Returns a selection of the entire rectangle this placement
    /// occupies within the screen. This can return null for a virtual
    /// placement or when unavailable pixel geometry makes it empty.
    ///
    /// Relative placements also return null: they have no screen
    /// position of their own, so geometric queries (delete by
    /// intersection, row, column, etc.) never match them directly.
    /// They are instead removed when their parent chain is removed.
    pub fn rect(&self, image: &Image, t: &Terminal) -> Option<Rect> {
        let grid_size = self.grid_size(image, t);
        let pin = match self.location {
            Location::Pin(p) => p,
            Location::Virtual | Location::Relative(_) => return None,
        };
        // if (pin.garbage) return null;
        // SAFETY(raw-page-aliasing/graphics_storage): `pin` is a tracked pin
        // owned by the screen's pagelist; only shared reads are performed and
        // no `&mut` to the pin is live across them.
        let pin_ref: &PageList::Pin = unsafe { &*pin };
        if pin_ref.garbage {
            return None;
        }

        // A zero pixel-sized placement can produce a zero grid size when
        // pixel geometry is unavailable. It occupies no rectangle.
        if grid_size.cols == 0 || grid_size.rows == 0 {
            return None;
        }

        // var br = switch (pin.downOverflow(grid_size.rows - 1)) {
        //     .offset => |v| v,
        //     .overflow => |v| v.end,
        // };
        let mut br = match pin_ref.down_overflow((grid_size.rows - 1) as usize) {
            PageList::pin::PinOverflow::Offset(v) => v,
            PageList::pin::PinOverflow::Overflow { end, .. } => end,
        };
        // br.x = @intCast(@min(
        //     @as(u32, pin.x) +| (grid_size.cols - 1),
        //     @as(u32, t.cols) - 1,
        // ));
        br.x = size::CellCountInt::try_from(core::cmp::min(
            // We need to sub one here because the x value is
            // one width already. So if the image is width "1"
            // then we add zero to X because X itself is width 1.
            (pin_ref.x as u32).saturating_add(grid_size.cols - 1),
            t.cols as u32 - 1,
        ))
        .unwrap();

        Some(Rect {
            top_left: *pin_ref,
            bottom_right: br,
        })
    }
}

/// `pub const SourceRect = struct { x, y, width, height: u32 }` on `Placement`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Return type of `Placement::cellOffset`: `struct { x: u32, y: u32 }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellOffset {
    pub x: u32,
    pub y: u32,
}

/// Return type of `Placement::pixelSize`: `struct { width: u32, height: u32 }`.
pub struct PixelSize {
    pub width: u32,
    pub height: u32,
}

/// Return type of `Placement::gridSize`: `struct { cols: u32, rows: u32 }`.
pub struct GridSize {
    pub cols: u32,
    pub rows: u32,
}

/// `std.math.divCeil(u32, numerator, denominator)`: ceil division, returning
/// `None` (the reference's `error.DivisionByZero`, caught as `catch 0`) when the
/// denominator is zero.
fn div_ceil_u32(numerator: u32, denominator: u32) -> Option<u32> {
    if denominator == 0 {
        return None;
    }
    Some(numerator.div_ceil(denominator))
}
