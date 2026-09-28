//! rustty-vt render
//!
//! PORTS: ghostty src/terminal/render.zig
//!
//! PORT STATUS: complete (vt-13). Every method + all 20 `test{}` blocks ported.
//
// CORRECTION (task-18, an unclaimed instance of the S139-F1 stale-count class —
// see `NEW-FINDINGS.md` N33): this said "all 15 `test{}` blocks". `render.zig`
// has 20 at pin `20abdb50a` and all 20 are live here (23 `#[test]`, the extra
// three being port-specific). "Every method + all N blocks" is a TOTALITY claim,
// so a stale N does not merely under-report — it reads as a specific,
// checkable promise that the file does not keep.
//!
//! Developer note: this is in src/terminal and not src/renderer because
//! the goal is that this remains generic to multiple renderers. This can
//! aid specifically with libghostty-vt with converting terminal state to
//! a renderable form.
//
// PORT NOTE: the reference `RenderState` keeps `row_data` as a
// `std.MultiArrayList(Row)` (a struct-of-arrays for cache locality) and each
// `Row` carries an `ArenaAllocator.State` + `std.MultiArrayList(Cell)`. The
// arena exists purely to bulk-free per-row grapheme dupes cheaply while
// retaining the cells' MultiArrayList capacity across `update` calls.
// `row_data` itself stays `Vec<Row>` (only `update` cares about the row-level
// projections and it iterates them once per frame), but the per-row cells are
// the hot path:
//
// PORT NOTE (reshape): restore MultiArrayList SoA (render.zig:187) — each
// row's cells are stored as parallel columns in `CellList` (`raw`, `style`,
// and grapheme codepoints in a single per-row reusable `grapheme_buf`
// addressed by a `(offset, len)` span column). `grapheme_buf.clear()`
// (retaining capacity) is the direct analogue of the reference per-row
// `arena.reset(.retain_capacity)`
// (render.zig:469-471). An earlier port draft stored cells AoS (`Vec<Cell>`
// with a per-cell owned `Vec<u32>` grapheme); that lost the raw-column memcpy
// rebuild whose value the reference's own comment quantifies at ~300% on the
// `screen-clone` benchmark (render.zig:494-499) and paid per-cell push +
// `Style::default()`/`Vec::new()` stores + drop glue. The reference's
// `.slice().items(.X)` projections become the `CellSlice` column accessors.
//
// PORT NOTE (parity tail S7-5, S175-F1): every container here allocates
// through the allocator `update`/`beginUpdate` is given, as the reference's
// do (`state.alloc` for the C API, `c/render.zig:209-232`); nothing is on the
// Rust heap unless that allocator is the Rust heap (the application's).
// - `row_data` and each row's `cells` are MultiArrayLists, grown by Zig
//   0.16's `resize`: to `growCapacity(n)` elements, by alloc + copy + free,
//   never `remap`. `row_data` is one allocation as in the reference; a row's
//   cells are one allocation per column (a measured choice, see `CellList`).
// - `pending_styles` and each row's `applied_styles` are `std.ArrayList`s:
//   `growCapacity` sizes, grown by `remap` first, then alloc + copy + free
//   (the handle's `grow`).
// - The reference's per-row arena holds the grapheme dupes and the
//   highlights. The port keeps its reshape: a retained per-row grapheme
//   buffer and a retained highlight list, both on the same allocator, cleared
//   (capacity kept) where the reference resets the arena with
//   `.retain_capacity`. A true arena would hand out grapheme slices that
//   dangle once the arena resets, which a safe API cannot expose for the
//   stale "undefined" slots the reference leaves in unflagged cells. The call
//   sequence differs from the arena's (T3 residual, N315); the routing and the
//   steady state (no allocation once capacity is reached) do not.
// - Every growth is fallible: a refused allocation is `error.OutOfMemory`,
//   never an abort (T2).
// - Zig's unmanaged containers take the allocator per call; these bind it on
//   first growth and debug-assert it afterwards (`alloc::bind`).

use crate::alloc::{self as valloc, AVec, Allocator, ArrayListUnmanaged, CopyVecExt};
use crate::color;
use crate::cursor;
use crate::highlight;
use crate::page;
use crate::pagelist::iter::Direction;
use crate::pagelist::{self, OutOfMemory, PageList, Pin};
use crate::point;
use crate::screen::Screen;
use crate::screen_set::ScreenSet;
use crate::selection::Selection;
use crate::size;
use crate::style::Style;
use crate::terminal::Terminal;

/// Contains the state required to render the screen, including optimizing
/// for repeated render calls and only rendering dirty regions.
///
/// Previously, our renderer would use `clone` to clone the screen within
/// the viewport to perform rendering. This worked well enough that we kept
/// it all the way up through the Ghostty 1.2.x series, but the clone time
/// was repeatedly a bottleneck blocking IO.
///
/// Rather than a generic clone that tries to clone all screen state per call
/// (within a region), a stateful approach that optimizes for only what a
/// renderer needs to do makes more sense.
///
/// To use this, initialize the render state to empty, then call `update`
/// on each frame to update the state to the latest terminal state.
///
// PORT NOTE: the reference uses an indented code block; fenced as ```text` so
// rustdoc treats the Zig-style snippet as prose (not an executable doctest).
/// ```text
///     var state: RenderState = .empty;
///     defer state.deinit(alloc);
///     state.update(alloc, &terminal);
/// ```
///
/// ## Two-Phase Updates
///
/// For callers that synchronize terminal access (e.g. a renderer thread
/// sharing a lock with an IO thread), the update can be split into two
/// phases to minimize the time the terminal must be held exclusively:
/// `beginUpdate` requires terminal access, while `endUpdate` completes
/// any deferred work using only memory owned by the render state.
///
/// ```text
///     {
///         mutex.lock();
///         defer mutex.unlock();
///         try state.beginUpdate(alloc, &terminal);
///     }
///
///     // The IO thread is free to modify the terminal while we
///     // complete the update.
///     state.endUpdate();
/// ```
///
/// The render state must be treated as incomplete between the two calls.
/// `update` is a convenience that performs both phases in one call.
///
/// ## Memory
///
/// Note: the render state retains as much memory as possible between updates
/// to prevent future allocations. If a very large frame is rendered once,
/// the render state will retain that much memory until deinit. To avoid
/// waste, it is recommended that the caller `deinit` and start with an
/// empty render state every so often.
pub struct RenderState {
    /// The current screen dimensions. It is possible that these don't match
    /// the renderer's current dimensions in grid cells because resizing
    /// can happen asynchronously. For example, for Metal, our NSView resizes
    /// at a different time than when our internal terminal state resizes.
    /// This can lead to a one or two frame mismatch a renderer needs to
    /// handle.
    ///
    /// The viewport is always exactly equal to the active area size so this
    /// is also the viewport size.
    pub rows: size::CellCountInt,
    pub cols: size::CellCountInt,

    /// The color state for the terminal.
    pub colors: Colors,

    /// Cursor state within the viewport.
    pub cursor: Cursor,

    /// The rows (y=0 is top) of the viewport. Guaranteed to be `rows` length.
    ///
    /// This is a MultiArrayList because only the update cares about
    /// the allocators. Callers care about all the other properties, and
    /// this better optimizes cache locality for read access for those
    /// use cases.
    //
    // PORT NOTE (reshape): a `Vec<Row>` (AoS) on the update allocator, grown
    // as the MultiArrayList is (see `resize_rows`).
    pub row_data: AVec<Row>,

    /// The dirty state of the render state. This is set by the update method.
    /// The renderer/caller should set this to false when it has handled
    /// the dirty state.
    pub dirty: Dirty,

    /// The screen type that this state represents. This is used primarily
    /// to detect changes.
    pub screen: crate::screen_set::Key,

    /// The last viewport pin used to generate this state. This is NOT
    /// a tracked pin and is generally NOT safe to read other than the direct
    /// values for comparison.
    pub viewport_pin: Option<Pin>,

    /// The cached selection so we can avoid expensive selection calculations
    /// if possible.
    pub selection_cache: Option<SelectionCache>,

    /// The pending style runs requiring an endUpdate call, in the
    /// order they were recorded. If multiple begins happen without an
    /// endUpdate call, runs accumulate; rows rebuilt more than once
    /// may then have superseded (stale) runs in this list, which is
    /// harmless: newer runs are appended later so they win, and cells
    /// not covered by newer runs have a default style ID in their raw
    /// data so their style is undefined by contract anyway. See
    /// beginUpdate.
    pub pending_styles: AVec<StyleRun>,

    // PORT NOTE: reference defect (R13, UD315). Set when `beginUpdate` fails
    // part way; the next `beginUpdate` then rebuilds every row. See
    // `begin_update`.
    rebuild_after_failure: bool,
}

/// A run of cells within one row sharing one style, pending
/// denormalization into the per-cell data. This is populated by
/// `beginUpdate` and consumed by `endUpdate`. This exists so that
/// the (potentially large) denormalization of styles into cells
/// can happen outside of any terminal locks. See `beginUpdate`.
#[derive(Clone, Copy, Debug)]
pub struct StyleRun {
    /// The viewport row.
    pub y: size::CellCountInt,

    /// Start (inclusive) x coordinate.
    pub start: size::CellCountInt,
    /// End (exclusive) x coordinate.
    pub end: size::CellCountInt,

    /// The style for this cell range.
    pub style: Style,
}

/// The color state for the terminal.
///
/// The background/foreground will be reversed if the terminal reverse
/// color mode is on! You do not need to handle that manually!
pub struct Colors {
    pub background: color::RGB,
    pub foreground: color::RGB,
    pub cursor: Option<color::RGB>,
    pub palette: color::Palette,
}

pub struct Cursor {
    /// The x/y position of the cursor within the active area.
    pub active: point::Coordinate,

    /// The x/y position of the cursor within the viewport. This
    /// may be null if the cursor is not visible within the viewport.
    pub viewport: Option<Viewport>,

    /// The cell data for the cursor position. Managed memory is not
    /// safe to access from this.
    pub cell: page::Cell,

    /// The style, always valid even if the cell is default style.
    pub style: Style,

    /// The visual style of the cursor itself, such as a block or
    /// bar.
    pub visual_style: cursor::Style,

    /// True if the cursor is detected to be at a password input field.
    pub password_input: bool,

    /// Cursor visibility state determined by the terminal mode.
    pub visible: bool,

    /// Cursor blink state determined by the terminal mode.
    pub blinking: bool,
}

pub struct Viewport {
    /// The x/y position of the cursor within the viewport.
    pub x: size::CellCountInt,
    pub y: size::CellCountInt,

    /// Whether the cursor is part of a wide character and
    /// on the tail of it. If so, some renderers may use this
    /// to move the cursor back one.
    pub wide_tail: bool,
}

/// A row within the viewport.
//
// PORT NOTE: the reference `arena: ArenaAllocator.State` field is dropped — see
// the module note. Per-cell grapheme dupes live in the row's `CellList`
// grapheme buffer and the highlights in `highlights`, both retained lists on
// the update allocator, cleared where the reference resets the arena and freed
// with the row. The reference field's documentation is preserved verbatim here:
//
//     /// Arena used for any heap allocations for cell contents
//     /// in this row. Importantly, this is NOT used for the MultiArrayList
//     /// itself. We do this on purpose so that we can easily clear rows,
//     /// but retain cached MultiArrayList capacities since grid sizes don't
//     /// change often.
//     arena: ArenaAllocator.State,
//
// The rebuild loop's handling of that field is likewise dropped:
//
//     // Promote our arena. State is copied by value so we need to
//     // restore it on all exit paths so we don't leak memory.
//     var arena = b.arenas[vy].promote(b.alloc);
//     defer b.arenas[vy] = arena.state;
pub struct Row {
    /// The page pin. Its copied values may be compared, but its node must
    /// not be dereferenced unless the terminal state is protected from
    /// changes since the last `update` call.
    pub pin: Pin,

    /// The page node generation captured alongside `pin`. This lets
    /// consumers validate the pin without dereferencing its node after
    /// the terminal lock has been released.
    pub serial: u64,

    /// Raw row data.
    pub raw: page::Row,

    /// The cells in this row. Guaranteed to be `cols` length.
    //
    // PORT NOTE (reshape): `std.MultiArrayList(Cell)` -> `CellList` (SoA).
    pub cells: CellList,

    /// A dirty flag that can be used by the renderer to track
    /// its own draw state. `update` will mark this true whenever
    /// this row is changed, too.
    pub dirty: bool,

    /// The x range of the selection within this row.
    pub selection: Option<[size::CellCountInt; 2]>,

    /// The highlights within this row.
    //
    // PORT NOTE: the reference appends these through the row arena
    // (`updateHighlightsFlattened`); here a list on the update allocator,
    // cleared with capacity kept where the reference resets the arena.
    pub highlights: AVec<Highlight>,

    /// The style runs applied to this row's per-cell style data
    /// by the last `endUpdate` that touched it. This is what lets
    /// `endUpdate` skip the (comparatively large) per-cell style
    /// fill when a rebuilt row produced identical runs, which is
    /// the common case: text changes far more often than styling.
    ///
    /// The invariant is that this always describes the current
    /// contents of the cell style data for the covered ranges. It
    /// is cleared whenever the cell storage is reallocated.
    ///
    /// This uses the general allocator (NOT the row arena)
    /// because it must survive row rebuilds. `endUpdate` cannot
    /// allocate, so `beginUpdate` reserves the capacity.
    // applied_styles: std.ArrayList(StyleRun),
    pub applied_styles: AVec<StyleRun>,
}

#[derive(Clone, Copy, Debug)]
pub struct Highlight {
    /// A special tag that can be used by the caller to differentiate
    /// different highlight types. The value is opaque to the RenderState.
    pub tag: u8,

    /// The x ranges of highlights within this row.
    pub range: [size::CellCountInt; 2],
}

// PORT NOTE (reshape): the reference `RenderState.Cell` is the ELEMENT type of
// a `std.MultiArrayList(Cell)`, so its `raw`/`grapheme`/`style` fields live in
// three parallel column arrays, never as a contiguous array of structs. The
// Rust port mirrors that with `CellList` below; `CellRef` is the by-value view
// the reference's `MultiArrayList.get(i)` yields (used by tests and the
// `CellList::get` convenience accessor). The reference field docs are carried
// onto the `CellList` columns.
pub struct CellRef<'a> {
    /// Always set, this is the raw copied cell data from page.Cell.
    /// The managed memory (hyperlinks, graphames, etc.) is NOT safe
    /// to access from here. It is duplicated into the other fields if
    /// it exists.
    pub raw: page::Cell,

    /// Grapheme data for the cell. This is undefined unless the
    /// raw cell's content_tag is `codepoint_grapheme`.
    //
    // PORT NOTE: the reference is `[]const u21` arena-allocated; the Rust port
    // borrows the grapheme cluster as `&[u32]` out of the row's `grapheme_buf`
    // (`lookupGrapheme` yields `&[u32]`). "undefined" is an empty slice.
    pub grapheme: &'a [u32],

    /// The style data for the cell. This is undefined unless
    /// the style_id is non-default on raw.
    pub style: Style,
}

/// `(offset, len)` into a `CellList`'s `grapheme_buf`. `(0, 0)` (an empty
/// slice) is the safe "no grapheme" sentinel for unflagged cells.
type GraphemeSpan = (u32, u32);

/// Struct-of-arrays storage for a row's cells, mirroring the reference
/// `std.MultiArrayList(Cell)`.
//
// PORT NOTE (reshape): see the module note. The three columns (`raw`, the
// grapheme spans, `style`) are separate lists on the update allocator, grown
// together as `MultiArrayList.resize` grows its one allocation: to
// `growCapacity(cols)`, by alloc + copy + free (`setCapacity`, never `remap`).
// One allocation per column instead of one for all three is a measured
// choice (T3 residual, N315): the single SoA block, with the capacity headroom
// between its columns, spread the hot `raw` and `style` columns of a
// viewport's rows over ~1.6x the memory and cost ~3% of `screen-clone.render`
// in cycles at FEWER instructions. The columns are exactly `cols` long; `raw`
// is filled by a single memcpy on rebuild (the reference `fastmem.copy(...,
// cells_slice.items(.raw), page_cells)`), while `style`/`grapheme` are written
// ONLY for flagged cells in the managed-row loop — the reference's "undefined
// unless flagged" discipline (render.zig:491-505). Slots the reference leaves
// undefined after a `resize` are initialized here (a zero cell, the default
// style, the empty span), so every read is defined. `grapheme_buf` is the
// per-row arena analogue: cleared (retaining capacity) each rebuild.
pub struct CellList {
    raw: AVec<page::Cell>,
    style: AVec<Style>,
    grapheme_spans: AVec<GraphemeSpan>,
    grapheme_buf: AVec<u32>,
}

/// A borrowed SoA view over a `CellList`, mirroring
/// `std.MultiArrayList(Cell).Slice`. Consumers project `.raw` / `.style` /
/// `.grapheme(x)` exactly as the reference projects `.items(.raw)` etc.
#[derive(Clone, Copy)]
pub struct CellSlice<'a> {
    /// `.items(.raw)`
    pub raw: &'a [page::Cell],
    /// `.items(.style)`
    pub style: &'a [Style],
    spans: &'a [GraphemeSpan],
    buf: &'a [u32],
}

impl<'a> CellSlice<'a> {
    /// Number of cells (columns) in the row.
    pub fn len(&self) -> usize {
        self.raw.len()
    }

    /// True if there are no cells.
    pub fn is_empty(&self) -> bool {
        self.raw.is_empty()
    }

    /// `.items(.grapheme)[x]`: the grapheme cluster for cell `x`. Undefined
    /// (empty) unless the raw cell's `content_tag` is `codepoint_grapheme`.
    //
    // PERF(port): checked slicing with an empty fallback — spans of UNFLAGGED
    // cells are stale (the reference leaves the grapheme column literally
    // undefined; see `CellList`) and may dangle past the reset
    // `grapheme_buf`. Flagged cells are always written by the grapheme arm of
    // `row_builder` in the same rebuild, so their spans are in bounds.
    pub fn grapheme(&self, x: usize) -> &'a [u32] {
        let (off, len) = self.spans[x];
        self.buf
            .get(off as usize..off as usize + len as usize)
            .unwrap_or(&[])
    }
}

impl Default for CellList {
    fn default() -> CellList {
        CellList::empty()
    }
}

impl CellList {
    /// Empty (the reference `.empty`).
    pub const fn empty() -> CellList {
        CellList::empty_in(valloc::VtAllocator::GLOBAL)
    }

    /// Empty, bound to `alloc` for its first growth.
    pub const fn empty_in(alloc: Allocator) -> CellList {
        CellList {
            raw: AVec::new_in(alloc),
            style: AVec::new_in(alloc),
            grapheme_spans: AVec::new_in(alloc),
            grapheme_buf: AVec::new_in(alloc),
        }
    }

    /// Number of cells (columns) in the row.
    pub fn len(&self) -> usize {
        self.raw.len()
    }

    /// True if there are no cells.
    pub fn is_empty(&self) -> bool {
        self.raw.is_empty()
    }

    /// `MultiArrayList.capacity`.
    pub fn capacity(&self) -> usize {
        self.raw.capacity()
    }

    /// The allocator the cells grow through.
    pub fn allocator(&self) -> Allocator {
        *self.raw.allocator()
    }

    /// `.items(.raw)`: the raw cell column.
    pub fn raw(&self) -> &[page::Cell] {
        &self.raw
    }

    /// Mutable `.items(.raw)` column (used to stamp flags on the render copy,
    /// e.g. the overlay tests / hyperlink marking).
    pub fn raw_mut(&mut self) -> &mut [page::Cell] {
        &mut self.raw
    }

    /// `.items(.style)`: the style column. Only valid for flagged cells.
    pub fn style(&self) -> &[Style] {
        &self.style
    }

    /// `.items(.grapheme)[x]`. Undefined (empty) unless flagged.
    //
    // PERF(port): checked slicing with an empty fallback — see
    // `CellSlice::grapheme` (unflagged spans are stale by design).
    pub fn grapheme(&self, x: usize) -> &[u32] {
        let (off, len) = self.grapheme_spans[x];
        self.grapheme_buf
            .get(off as usize..off as usize + len as usize)
            .unwrap_or(&[])
    }

    /// A borrowed SoA view (the reference `cells.slice()`).
    pub fn slice(&self) -> CellSlice<'_> {
        CellSlice {
            raw: &self.raw,
            style: &self.style,
            spans: &self.grapheme_spans,
            buf: &self.grapheme_buf,
        }
    }

    /// The reference `MultiArrayList.get(x)` — a by-value cell view.
    pub fn get(&self, x: usize) -> CellRef<'_> {
        CellRef {
            raw: self.raw[x],
            grapheme: self.grapheme(x),
            style: self.style[x],
        }
    }

    /// Bind the columns (and the grapheme buffer) to the allocator the
    /// reference passes, on first growth; afterwards debug-assert it (see
    /// `alloc::bind`).
    #[inline]
    fn bind(&mut self, alloc: Allocator) {
        valloc::bind(&mut self.raw, alloc);
        valloc::bind(&mut self.style, alloc);
        valloc::bind(&mut self.grapheme_spans, alloc);
        valloc::bind(&mut self.grapheme_buf, alloc);
    }

    // `try cells.resize(alloc, new_len)`: `ensureTotalCapacity` (`setCapacity(
    // growCapacity(new_len))` when short), then `len = new_len`.
    //
    // PORT NOTE: the reference leaves the new slots undefined; they are
    // initialized here (see the type note). Only a column-count change calls
    // this, so the fill is off the hot path.
    #[cold]
    #[inline(never)]
    fn resize(&mut self, alloc: Allocator, new_len: usize) -> Result<(), OutOfMemory> {
        self.bind(alloc);
        if self.raw.capacity() < new_len {
            self.set_capacity(grow_capacity::<CELL_ZIG_FIELD_MAX>(new_len))?;
        }
        // The capacity is reserved: these never allocate.
        self.raw.resize_copy(new_len, page::Cell::zeroed());
        self.grapheme_spans.resize_copy(new_len, (0, 0));
        self.style.resize_copy(new_len, Style::default());
        Ok(())
    }

    // `setCapacity(gpa, new_capacity)`: a fresh allocation per column, the
    // live prefix copied over, then the old one freed (never `remap`). All
    // three are allocated before any is replaced, so a failure changes
    // nothing, as the reference's single allocation does.
    fn set_capacity(&mut self, new_capacity: usize) -> Result<(), OutOfMemory> {
        fn fresh<T: Copy>(old: &AVec<T>, new_capacity: usize) -> Result<AVec<T>, OutOfMemory> {
            let mut v = AVec::new_in(*old.allocator());
            v.try_reserve_exact(new_capacity).map_err(|_| OutOfMemory)?;
            Ok(v)
        }
        let mut raw = fresh(&self.raw, new_capacity)?;
        let mut spans = fresh(&self.grapheme_spans, new_capacity)?;
        let mut style = fresh(&self.style, new_capacity)?;
        raw.extend_copy(&self.raw);
        spans.extend_copy(&self.grapheme_spans);
        style.extend_copy(&self.style);
        self.raw = raw;
        self.grapheme_spans = spans;
        self.style = style;
        Ok(())
    }

    // `arena.reset(.retain_capacity)` for the grapheme dupes.
    #[inline]
    fn reset_graphemes(&mut self) {
        self.grapheme_buf.clear();
    }

    // `fastmem.copy(page.Cell, cells_slice.items(.raw), page_cells)`.
    #[inline]
    fn copy_raw(&mut self, page_cells: &[page::Cell]) {
        self.raw.copy_from_slice(page_cells);
    }

    // PORT NOTE (reshape): the reference hoists the mutable column pointers
    // once before the managed per-cell loop — `const cells_grapheme =
    // cells_slice.items(.grapheme); const cells_style =
    // cells_slice.items(.style);` (render.zig:508-510). `slice_mut` is the
    // Rust analogue: the three disjoint mutable field borrows (style column,
    // grapheme-span column, grapheme arena) handed to `update()` so it can
    // hoist them the same way.
    //
    // PERF(port): the previous per-write `set_style`/`set_grapheme` helpers
    // went through `&mut self`, so every styled cell re-loaded the column
    // len + base pointer (a bounds check per cell the reference doesn't pay);
    // hoisting the borrows lets those loads live in registers across the loop.
    fn slice_mut(&mut self) -> (&mut [Style], &mut [GraphemeSpan], &mut AVec<u32>) {
        (
            &mut self.style,
            &mut self.grapheme_spans,
            &mut self.grapheme_buf,
        )
    }

    /// `row_cells[run.y].slice().items(.style)` — the style column alone, for
    /// `RenderState::end_update`'s denormalization pass.
    fn style_mut(&mut self) -> &mut [Style] {
        &mut self.style
    }
}

/// Zig 0.16 `growCapacity(minimum)` for a container whose `init_capacity` is
/// `max(1, cache_line / zig_size)`: `@sizeOf(T)` for an `ArrayList(T)`, the
/// largest field's size for a `MultiArrayList(T)`.
//
// PORT NOTE: the sizes are the REFERENCE's (below), so the capacities the
// caller's allocator sees match the reference's element counts; the byte sizes
// differ where the Rust element does (T3 residual, N315).
#[inline]
fn grow_capacity<const ZIG_SIZE: usize>(minimum: usize) -> usize {
    ArrayListUnmanaged::<[u8; ZIG_SIZE]>::grow_capacity(minimum)
}

/// `MultiArrayList(RenderState.Row)`: its largest fields are the three
/// 24-byte lists (`cells`, `highlights`, `applied_styles`).
const ROW_ZIG_FIELD_MAX: usize = 24;
/// `MultiArrayList(RenderState.Cell)`: its largest field is `style` (28 bytes:
/// three 8-byte `Color` unions over a 4-byte packed `RGB`, and the `u16`
/// flags).
const CELL_ZIG_FIELD_MAX: usize = 28;
/// `ArrayList(StyleRun)`: `Style` (28) and three `u16`s, 36 bytes.
const STYLE_RUN_ZIG_SIZE: usize = 36;
/// `ArrayList(Highlight)`: a `u8` tag and two `u16`s, 6 bytes.
const HIGHLIGHT_ZIG_SIZE: usize = 6;

/// `try list.append(gpa, value)`: grows to `growCapacity(len + 1)` when full
/// (`remap` first, then alloc + copy + free: the handle's `grow`).
//
// PERF(port): always inlined with the growth out of line, so an append that
// fits is a compare and a store (the per-run hot path of `row_builder`).
#[inline(always)]
fn try_append<T, const ZIG_SIZE: usize>(list: &mut AVec<T>, value: T) -> Result<(), OutOfMemory> {
    let len = list.len();
    if len == list.capacity() {
        grow_total_capacity::<T, ZIG_SIZE>(list, len + 1)?;
    }
    // SAFETY: `len < capacity` now; the slot is written before `len` covers it.
    unsafe {
        list.as_mut_ptr().add(len).write(value);
        list.set_len(len + 1);
    }
    Ok(())
}

/// `try list.ensureTotalCapacity(gpa, new_capacity)`.
#[inline(always)]
fn ensure_total_capacity<T, const ZIG_SIZE: usize>(
    list: &mut AVec<T>,
    new_capacity: usize,
) -> Result<(), OutOfMemory> {
    if list.capacity() >= new_capacity {
        return Ok(());
    }
    grow_total_capacity::<T, ZIG_SIZE>(list, new_capacity)
}

/// `ensureTotalCapacityPrecise(gpa, growCapacity(new_capacity))`.
#[cold]
#[inline(never)]
fn grow_total_capacity<T, const ZIG_SIZE: usize>(
    list: &mut AVec<T>,
    new_capacity: usize,
) -> Result<(), OutOfMemory> {
    let target = grow_capacity::<ZIG_SIZE>(new_capacity);
    list.try_reserve_exact(target - list.len())
        .map_err(|_| OutOfMemory)
}

/// C: RusttyRenderStateDirty (R2) — checked by the A5 enum-discriminant arm.
// Dirty state.
// PORT NOTE: `Dirty = lib.Enum(lib.target, &.{ "false", "partial", "full" })`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dirty {
    /// Not dirty at all. Can skip rendering if prior state was
    /// already rendered.
    False,

    /// Some rows changed but not all. None of the global state
    /// changed such as colors.
    Partial,

    /// Global state changed or dimensions changed. All rows should
    /// be redrawn.
    Full,
}

pub struct SelectionCache {
    pub selection: Selection,
    pub tl_pin: Pin,
    pub br_pin: Pin,
}

impl RenderState {
    /// Initial state.
    // PORT NOTE: `pub const empty: RenderState = .{ ... }`. The reference inits
    // `cursor.style = undefined`; the Rust port uses `Style::default()` (the
    // value is always overwritten by `update` before observation).
    pub fn empty() -> RenderState {
        RenderState {
            rows: 0,
            cols: 0,
            colors: Colors {
                background: color::RGB { r: 0, g: 0, b: 0 },
                foreground: color::RGB {
                    r: 0xff,
                    g: 0xff,
                    b: 0xff,
                },
                cursor: None,
                palette: color::DEFAULT,
            },
            cursor: Cursor {
                active: point::Coordinate { x: 0, y: 0 },
                viewport: None,
                cell: page::Cell::zeroed(),
                style: Style::default(),
                visual_style: cursor::Style::Block,
                password_input: false,
                visible: true,
                blinking: false,
            },
            // PORT NOTE: `.empty` holds no allocator; these bind to the
            // update allocator on first growth (`alloc::bind`).
            row_data: AVec::new_in(valloc::VtAllocator::GLOBAL),
            dirty: Dirty::False,
            screen: crate::screen_set::Key::Primary,
            viewport_pin: None,
            selection_cache: None,
            pending_styles: AVec::new_in(valloc::VtAllocator::GLOBAL),
            rebuild_after_failure: false,
        }
    }

    // pub fn deinit(self: *RenderState, alloc: Allocator) void {
    //     for (row_data.items(.arena), .items(.cells), .items(.applied_styles))
    //         |state, *cells, *applied| {
    //         var arena: ArenaAllocator = state.promote(alloc);
    //         arena.deinit();
    //         cells.deinit(alloc);
    //         applied.deinit(alloc);
    //     }
    //     self.row_data.deinit(alloc);
    //     self.pending_styles.deinit(alloc);
    // }
    //
    // PORT NOTE: each list frees through the allocator it was bound to when it
    // drops, which is `alloc` (debug-asserted). The state is left `.empty`, so
    // it can be updated again (the reference leaves it undefined).
    pub fn deinit(&mut self, alloc: Allocator) {
        if self.row_data.capacity() != 0 {
            self.row_data.allocator().debug_assert_same(alloc);
        }
        if self.pending_styles.capacity() != 0 {
            self.pending_styles.allocator().debug_assert_same(alloc);
        }
        self.row_data = AVec::new_in(valloc::VtAllocator::GLOBAL);
        self.pending_styles = AVec::new_in(valloc::VtAllocator::GLOBAL);
    }

    /// Update the render state to the latest terminal state.
    ///
    /// This is a convenience function that performs a full update in
    /// one call, equivalent to `beginUpdate` immediately followed by
    /// `endUpdate`. Callers that hold a lock over the terminal state
    /// should prefer calling the two phases directly so that the lock
    /// is only held for `beginUpdate`.
    ///
    /// This will reset the terminal dirty state since it is consumed
    /// by this render state update.
    pub fn update(&mut self, alloc: Allocator, t: &mut Terminal) -> Result<(), OutOfMemory> {
        self.begin_update(alloc, t)?;
        self.end_update();
        Ok(())
    }

    /// Complete a prior `begin_update` call by performing any deferred work. At
    /// the time of writing, this denormalizes the pending style runs into the
    /// per-cell style data.
    ///
    /// This only reads and writes memory owned by the render state, so it is
    /// safe to call while the terminal is being modified (no terminal lock is
    /// required).
    pub fn end_update(&mut self) {
        // Common case: no styled rows were rebuilt.
        if self.pending_styles.is_empty() {
            return;
        }

        let row_len = self.row_data.len();

        // Process the pending runs one row segment at a time. All the
        // runs for a row are appended contiguously by a single
        // beginUpdate, so a segment boundary is simply a change in y.
        let runs: &[StyleRun] = &self.pending_styles;
        let mut i: usize = 0;
        while i < runs.len() {
            let y = runs[i].y;
            let mut j = i + 1;
            while j < runs.len() && runs[j].y == y {
                j += 1;
            }
            let segment = &runs[i..j];
            i = j;

            // Defensive: the row data may have changed shape if the
            // caller violated ordering (e.g. an error path skipped an
            // endUpdate between updates). Any update that changes
            // dimensions clears the pending list (redraw), so this
            // should never actually trigger, but the cost is trivial.
            if y as usize >= row_len {
                continue;
            }

            // If the segment matches the runs already denormalized
            // into this row's cell data then the per-cell styles are
            // already correct and the (comparatively large) fill can
            // be skipped entirely. This is the common case: rebuilt
            // rows usually keep their styling (only the text
            // changed), and full redraws of an unchanged screen keep
            // both.
            let row = &mut self.row_data[y as usize];
            if Self::runs_eql(&row.applied_styles, segment) {
                continue;
            }

            // PERF(port): the reference takes `row_cells[run.y].slice()
            // .items(.style)` per run; every run of the segment has `run.y ==
            // y`, so the column is resolved once.
            let styles = row.cells.style_mut();
            for run in segment {
                let end = core::cmp::min(run.end as usize, styles.len());
                let start = core::cmp::min(run.start as usize, end);

                Self::fill_styles(&mut styles[start..end], run.style);
            }

            // Record what we applied so the next rebuild of this row
            // can skip the fill. beginUpdate reserved the capacity
            // (we cannot allocate here); if it doesn't fit (e.g.
            // segments merged across multiple begins without an end)
            // leave the cache empty, which never matches and simply
            // means the next rebuild applies its runs.
            //
            // PORT NOTE: `appendSliceAssumeCapacity` — the capacity check
            // above guarantees `extend_copy` never allocates.
            row.applied_styles.clear();
            if row.applied_styles.capacity() >= segment.len() {
                row.applied_styles.extend_copy(segment);
            }
        }
        self.pending_styles.clear();
    }

    /// Mark all render-state data as consumed by the renderer.
    ///
    /// This clears both the global dirty state and every per-row dirty flag.
    /// Callers that only consume part of a frame should clear the two layers
    /// individually instead.
    // pub fn clean(self: *RenderState) void { ... }
    pub fn clean(&mut self) {
        self.dirty = Dirty::False;
        // @memset(self.row_data.items(.dirty), false);
        for row in self.row_data.iter_mut() {
            row.dirty = false;
        }
    }

    /// Fill a slice of styles with one value.
    ///
    /// This is equivalent to `@memset(dst, value)` but manually vectorized:
    /// `@memset` with a struct value lowers to a per-element field copy
    /// that reloads the source at every iteration because LLVM cannot
    /// prove the destination doesn't alias it. And, Zig 0.16 disables
    /// auto-vectorization due to an LLVM bug.
    ///
    /// This complexity is justified by accounting for ~10% of the endUpdate
    /// times under heavily styled cases.
    // fn fillStyles(dst: []Style, value: Style) void {
    //     // Each element is written as two overlapping 16-byte vector
    //     // stores held in registers, which requires 16 <= size <= 32.
    //     const elem_size = @sizeOf(Style);
    //     comptime assert(elem_size >= 16 and elem_size <= 32);
    //     ...
    // }
    //
    // PORT NOTE (`74a233b54`, Phase-3 `f2a7652ab` precedent): the reference's
    // manual two-overlapping-16-byte-vector-stores body exists solely because
    // of ZIG 0.16 codegen (per-field `@memset` lowering + the disabled LLVM
    // loop vectorizer). Rust's `<[T]>::fill` with a `Copy` struct lowers to a
    // plain fill loop that LLVM vectorizes normally, so the SEMANTICS are
    // ported and the vector trick is not replicated (its `16 <= @sizeOf(Style)
    // <= 32` comptime assert is kept as the same guard the reference compiles
    // under).
    fn fill_styles(dst: &mut [Style], value: Style) {
        // const elem_size = @sizeOf(Style);
        // comptime assert(elem_size >= 16 and elem_size <= 32);
        let () = Self::_STYLE_SIZE_ASSERT;

        dst.fill(value);
    }

    // The `fill_styles` comptime size assertion (forced by the `let ()` above).
    const _STYLE_SIZE_ASSERT: () = {
        let elem_size = core::mem::size_of::<Style>();
        assert!(elem_size >= 16 && elem_size <= 32);
    };

    /// Returns true if the two style run lists denormalize to
    /// identical per-cell style data. This is a semantic comparison
    /// (styles are compared field-wise, never by bytes, since padding
    /// is undefined).
    // fn runsEql(a: []const StyleRun, b: []const StyleRun) bool { ... }
    fn runs_eql(a: &[StyleRun], b: &[StyleRun]) -> bool {
        if a.len() != b.len() {
            return false;
        }
        for (ar, br) in a.iter().zip(b.iter()) {
            if ar.start != br.start || ar.end != br.end || !ar.style.eql(&br.style) {
                return false;
            }
        }
        true
    }

    /// Begin an update of the render state to the latest terminal
    /// state. Every begin must be completed with an `endUpdate` call
    /// before the render state is read.
    ///
    /// This two-phase structure exists for callers that lock the
    /// terminal state: only this function requires terminal access, so
    /// a caller can hold its lock for this call only and then call
    /// `endUpdate` after releasing it. `endUpdate` exclusively reads
    /// and writes memory owned by the render state.
    ///
    /// Work that doesn't require terminal access may be deferred to
    /// `endUpdate` to keep this call (and therefore lock hold time) as
    /// short as possible. At the time of writing, the deferred work is
    /// the per-cell style denormalization, so between this call and
    /// `endUpdate` the per-cell `style` data of any updated rows is
    /// stale and must not be read. More work may be deferred in the
    /// future; callers should treat the render state as incomplete
    /// until `endUpdate` is called.
    ///
    /// This will reset the terminal dirty state since it is consumed
    /// by this render state update.
    //
    // PORT NOTE: reference defect (R13, UD315). A `beginUpdate` that fails
    // part way has already consumed dirty state it did not act on: `rows`/
    // `cols`/`viewport_pin` are updated first, and each page's and row's dirty
    // flag is cleared before its row is rebuilt. The rows it did not reach keep
    // their old contents (or no cells at all after a resize), and nothing makes
    // the next update rebuild them, so a retry that succeeds can report
    // `.false`/`.partial` over a stale or empty viewport. Reachable once an
    // allocation fails, which the C API's caller-supplied allocator can do.
    // The port remembers the failure and makes the next update a full rebuild
    // (`redraw`), as the reference already does for every other change it
    // cannot patch incrementally. On success nothing changes.
    pub fn begin_update(&mut self, alloc: Allocator, t: &mut Terminal) -> Result<(), OutOfMemory> {
        let result = self.begin_update_(alloc, t);
        if result.is_err() {
            self.rebuild_after_failure = true;
        }
        result
    }

    fn begin_update_(&mut self, alloc: Allocator, t: &mut Terminal) -> Result<(), OutOfMemory> {
        // PORT NOTE: `.empty` lists bind to `alloc` here (their first growth
        // is below); afterwards `alloc` is debug-asserted to be theirs.
        valloc::bind(&mut self.row_data, alloc);
        valloc::bind(&mut self.pending_styles, alloc);

        let s: *mut Screen = t.screens.active;
        let viewport_pin = unsafe { (*s).pages.get_top_left(point::Tag::Viewport) };
        let redraw: bool = 'redraw: {
            // PORT NOTE: reference defect (R13, UD315): see `begin_update`.
            if self.rebuild_after_failure {
                break 'redraw true;
            }

            // If our screen key changed, we need to do a full rebuild
            // because our render state is viewport-specific.
            if t.screens.active_key != self.screen {
                break 'redraw true;
            }

            // If our terminal is dirty at all, we do a full rebuild. These
            // dirty values are full-terminal dirty values.
            {
                // const Int = @typeInfo(Terminal.Dirty).@"struct".backing_integer.?;
                // const v: Int = @bitCast(t.flags.dirty);
                // if (v > 0) break :redraw true;
                let d = t.flags.dirty;
                if d.palette || d.reverse_colors || d.clear || d.preedit || d.glyph_glossary {
                    break 'redraw true;
                }
            }

            // If our screen is dirty at all, we do a full rebuild. This is
            // a full screen dirty tracker.
            {
                // const Int = @typeInfo(Screen.Dirty).@"struct".backing_integer.?;
                // const v: Int = @bitCast(t.screens.active.dirty);
                // if (v > 0) break :redraw true;
                let d = unsafe { (*t.screens.active).dirty };
                if d.selection || d.hyperlink_hover {
                    break 'redraw true;
                }
            }

            // If our dimensions changed, we do a full rebuild.
            if self.rows != unsafe { (*s).pages.rows } || self.cols != unsafe { (*s).pages.cols } {
                break 'redraw true;
            }

            // If our viewport pin changed, we do a full rebuild.
            if let Some(old) = self.viewport_pin {
                if !old.eql(viewport_pin) {
                    break 'redraw true;
                }
            }

            false
        };

        // Always set our cheap fields, its more expensive to compare
        self.rows = unsafe { (*s).pages.rows };
        self.cols = unsafe { (*s).pages.cols };
        self.viewport_pin = Some(viewport_pin);
        self.cursor.active = point::Coordinate {
            x: unsafe { (*s).cursor.x },
            y: unsafe { (*s).cursor.y as u32 },
        };
        self.cursor.cell = unsafe { *(*s).cursor.page_cell };
        self.cursor.style = unsafe { (*s).cursor.style };
        self.cursor.visual_style = unsafe { (*s).cursor.cursor_style };
        self.cursor.password_input = t.flags.password_input;
        self.cursor.visible = t.modes.get(crate::modes::Mode::CursorVisible);
        self.cursor.blinking = t.modes.get(crate::modes::Mode::CursorBlinking);

        // Always reset the cursor viewport position. In the future we can
        // probably cache this by comparing the cursor pin and viewport pin
        // but may not be worth it.
        self.cursor.viewport = None;

        // Colors.
        self.colors.cursor = t.colors.cursor.get();

        // The palette is a relatively large copy (768 bytes at the time
        // of writing) so we only copy it when it could have changed. All
        // palette modifications set a terminal-level dirty flag (see
        // Terminal.Dirty.palette), and any terminal-level dirty flag
        // forces a redraw, so checking redraw is sufficient.
        if redraw {
            self.colors.palette = t.colors.palette.current;
        }
        'bg_fg: {
            // Background/foreground can be unset initially which would
            // depend on "default" background/foreground. The expected use
            // case of Terminal is that the caller set their own configured
            // defaults on load so this doesn't happen.
            let bg = match t.colors.background.get() {
                Some(v) => v,
                None => break 'bg_fg,
            };
            let fg = match t.colors.foreground.get() {
                Some(v) => v,
                None => break 'bg_fg,
            };
            if t.modes.get(crate::modes::Mode::ReverseColors) {
                self.colors.background = fg;
                self.colors.foreground = bg;
            } else {
                self.colors.background = bg;
                self.colors.foreground = fg;
            }
        }

        // Ensure our row length is exactly our height, freeing or allocating
        // data as necessary. In most cases we'll have a perfectly matching
        // size.
        if self.row_data.len() != self.rows as usize {
            // @branchHint(.unlikely);

            if self.row_data.len() < self.rows as usize {
                // Resize our rows to the desired length, marking any added
                // values undefined.
                let old_len = self.row_data.len();
                // try self.row_data.resize(alloc, self.rows);
                resize_rows_capacity(&mut self.row_data, self.rows as usize)?;
                // Initialize all our values. Its faster to use slice() + set()
                // because appendAssumeCapacity does this multiple times.
                //
                // PORT NOTE: the capacity is reserved above, so these pushes
                // never allocate.
                for _y in old_len..self.rows as usize {
                    self.row_data.push(Row {
                        // .arena = .{},     (no arena in the Rust port)
                        pin: Pin {
                            node: core::ptr::null_mut(),
                            y: 0,
                            x: 0,
                            garbage: false,
                        },
                        // .serial = undefined,
                        serial: 0,
                        raw: page::Row::zeroed(),
                        // .cells = .empty,
                        cells: CellList::empty_in(alloc),
                        dirty: true,
                        selection: None,
                        // .highlights = .empty,
                        highlights: AVec::new_in(alloc),
                        // .applied_styles = .empty,
                        applied_styles: AVec::new_in(alloc),
                    });
                }
            } else {
                // For the rows beyond self.rows, free the arena, the cells and
                // the applied styles, then shrinkRetainingCapacity.
                //
                // PORT NOTE: dropping the rows frees each one's lists through
                // the allocator they were bound to (`alloc`).
                self.row_data.truncate(self.rows as usize);
            }
        }

        // Break down our row data
        // PORT NOTE: the reference splits `row_data` into per-field slices; the
        // Rust port indexes `self.row_data[y]` directly (the `Vec<Row>` layout).

        // If we're redrawing then every row will be rebuilt, superseding
        // any pending style runs from prior updates. Clearing also
        // guarantees pending runs always match the current dimensions
        // (dimension changes force a redraw).
        if redraw {
            self.pending_styles.clear();
        }

        // Go through and setup our rows. We iterate page chunks rather
        // than individual rows so that per-page work (dirty flags, cursor
        // detection, memory pointers) is hoisted out of the row loop. This
        // makes the common case of a clean (or mostly clean) frame very
        // cheap: a contiguous scan of row dirty flags.
        //
        // PORT NOTE: the reference builds a `RowBuilder` struct holding the
        // `row_data` column slices plus `&pending_styles`, then calls
        // `builder.row(...)`. rustty's `row_data` is a `Vec<Row>` (AoS with
        // per-row owned `Vec`s), so there are no column slices to hold; the
        // equivalent is the free function `row_builder` below, which takes the
        // two disjoint `&mut` field borrows at each call site. That keeps the
        // borrow short-lived, so `self.cursor` stays writable in the same loop.
        let cols = self.cols;
        let rows = self.rows;
        let mut y: usize = 0;
        let mut any_dirty: bool = false;
        let mut page_it = viewport_pin.page_iterator(Direction::RightDown, None);
        while y < rows as usize {
            let chunk = match page_it.next() {
                Some(c) => c,
                None => break,
            };
            let node = chunk.node;
            let p: *mut page::Page = unsafe { crate::pagelist::Node::page(node) };

            // The number of rows we consume from this chunk. The chunk
            // may extend beyond the viewport (the viewport is always
            // exactly `rows` tall) so we clamp.
            let take: usize = core::cmp::min((chunk.end - chunk.start) as usize, rows as usize - y);

            // Find our cursor if we haven't found it yet. We do this even
            // if rows are not dirty because the cursor is unrelated. We
            // can check the chunk bounds once rather than every row.
            if self.cursor.viewport.is_none()
                && core::ptr::eq(node, unsafe { (*(*s).cursor.page_pin).node })
            {
                'cursor: {
                    let cy = unsafe { (*(*s).cursor.page_pin).y };
                    if cy < chunk.start || (cy as usize) >= chunk.start as usize + take {
                        break 'cursor;
                    }
                    self.cursor.viewport = Some(Viewport {
                        y: (y + (cy - chunk.start) as usize) as size::CellCountInt,
                        x: unsafe { (*s).cursor.x },

                        // Future: we should use our own state here to look this
                        // up rather than calling this.
                        wide_tail: if unsafe { (*s).cursor.x } > 0 {
                            unsafe { (*(*s).cursor_cell_left(1)).wide() == page::Wide::Wide }
                        } else {
                            false
                        },
                    });
                }
            }

            // The page-level dirty flag applies to every row in the chunk.
            // We consume (clear) it now; each node appears at most once in
            // this iteration and we're the only consumer of dirty state.
            let page_dirty = unsafe { (*p).dirty };
            if page_dirty {
                unsafe { (*p).dirty = false };
            }

            // Get our contiguous rows for this chunk.
            //
            // RAW-PTR DISCIPLINE (memory: rust-mut-args-strongly-protected): the
            // rows live in the page's own mapping, which `row_builder` also
            // reads through `p`. We therefore keep the rows as a raw base
            // pointer and only ever form a SHARED `&[page::Row]` for the mask
            // scans (which never write), taking a fresh raw element pointer for
            // the `set_dirty(false)` write and for the builder. The shared
            // slice is formed afresh for EACH scan (`page_rows()` below): one
            // held across the loop is invalidated by the first `set_dirty`
            // write, and the next scan read through a dead tag (Stacked
            // Borrows; task C-1, NEW-FINDINGS N394).
            let page_rows_ptr: *mut page::Row =
                unsafe { (*p).rows.ptr((*p).memory_ptr()).add(chunk.start as usize) };
            let page_rows = || -> &[page::Row] {
                unsafe { core::slice::from_raw_parts(page_rows_ptr, take) }
            };
            debug_assert!(unsafe { (*p).size.cols } == cols);

            // Store our pins and their node generations. We have to store
            // these even for rows that aren't dirty because dirty is only a
            // renderer optimization; it doesn't apply to memory movement.
            // This lets us remap any cell pins back to an exact entry in our
            // RenderState and validate them later without dereferencing a
            // potentially stale node.
            //
            // We can skip the writes when the pins and serials are unchanged:
            // if we're not redrawing, every value was stored by a prior update
            // (row count changes force a redraw). Within a single update a
            // node appears at most once and its stored pins have consecutive
            // y values, so if the first and last entries of this chunk's range
            // already match then every entry in between matches too.
            //
            // PORT NOTE: rustty also stores `node.serial` per row (the
            // compressed-page INSPECT-not-RESTORE discipline; see
            // `update_highlights_flattened`). The serial is a per-NODE value, so
            // it is read once per chunk and folded into the same skip test.
            let node_serial = unsafe { (*node).serial };
            if redraw
                || !core::ptr::eq(self.row_data[y].pin.node, node)
                || self.row_data[y].pin.y != chunk.start
                || self.row_data[y].serial != node_serial
                || !core::ptr::eq(self.row_data[y + take - 1].pin.node, node)
                || self.row_data[y + take - 1].pin.y != chunk.start + take as size::CellCountInt - 1
                || self.row_data[y + take - 1].serial != node_serial
            {
                for (i, py) in (chunk.start..chunk.start + take as size::CellCountInt).enumerate() {
                    let row = &mut self.row_data[y + i];
                    row.pin = Pin {
                        node,
                        y: py,
                        x: 0,
                        garbage: false,
                    };
                    row.serial = node_serial;
                }
            }

            if !redraw && !page_dirty {
                // Only dirty rows (usually none) need a rebuild. Scan the
                // dirty flags a group at a time; the dirty bit is directly
                // testable on the packed row representation.
                let mut i: usize = 0;
                while take - i >= RowDirtyMask::GROUP_LEN {
                    if RowDirtyMask::match_group(page_rows(), i) {
                        // @branchHint(.likely)
                        i += RowDirtyMask::GROUP_LEN;
                        continue;
                    }

                    for j in i..i + RowDirtyMask::GROUP_LEN {
                        let page_row = unsafe { page_rows_ptr.add(j) };
                        if !unsafe { (*page_row).dirty() } {
                            continue;
                        }
                        unsafe { (*page_row).set_dirty(false) };
                        any_dirty = true;
                        row_builder(
                            alloc,
                            &mut self.row_data,
                            &mut self.pending_styles,
                            cols,
                            p,
                            page_row,
                            y + j,
                        )?;
                    }
                    i += RowDirtyMask::GROUP_LEN;
                }
                while i < take {
                    let page_row = unsafe { page_rows_ptr.add(i) };
                    if unsafe { (*page_row).dirty() } {
                        unsafe { (*page_row).set_dirty(false) };
                        any_dirty = true;
                        row_builder(
                            alloc,
                            &mut self.row_data,
                            &mut self.pending_styles,
                            cols,
                            p,
                            page_row,
                            y + i,
                        )?;
                    }
                    i += 1;
                }
            } else {
                // Rebuild every row in the chunk.
                any_dirty = true;
                for i in 0..take {
                    let page_row = unsafe { page_rows_ptr.add(i) };
                    unsafe { (*page_row).set_dirty(false) };
                    row_builder(
                        alloc,
                        &mut self.row_data,
                        &mut self.pending_styles,
                        cols,
                        p,
                        page_row,
                        y + i,
                    )?;
                }
            }

            y += take;
        }
        let y = y as size::CellCountInt;
        assert!(y == self.rows);

        // If our screen has a selection, then mark the rows with the
        // selection. We do this outside of the loop above because its unlikely
        // a selection exists and because the way our selections are structured
        // today is very inefficient.
        //
        // NOTE: To improve the performance of the block below, we'll need
        // to rethink how we model selections in general.
        //
        // There are performance improvements that can be made here, though.
        // For example, `containedRow` recalculates a bunch of information
        // we can cache.
        'selection: {
            let sel: Selection = match unsafe { &(*s).selection } {
                Some(sel) => *sel,
                None => break 'selection,
            };
            // @branchHint(.unlikely);

            // Populate our selection cache to avoid some expensive
            // recalculation.
            // PORT NOTE: the reference returns `*const SelectionCache` via a
            // labeled block; the Rust port resolves to a copy of the two
            // pins + selection it needs (`Pin`/`Selection` are cheap copies).
            let cache: SelectionCache = 'cache: {
                if let Some(c) = &self.selection_cache {
                    'cache_check: {
                        // If we're redrawing, we recalculate the cache just to
                        // be safe.
                        if redraw {
                            break 'cache_check;
                        }

                        // If our selection isn't equal, we aren't cached!
                        if !c.selection.eql(sel) {
                            break 'cache_check;
                        }

                        // If we have no dirty rows, we can not recalculate.
                        if !any_dirty {
                            break 'selection;
                        }

                        // We have dirty rows, we can utilize the cache.
                        break 'cache SelectionCache {
                            selection: c.selection,
                            tl_pin: c.tl_pin,
                            br_pin: c.br_pin,
                        };
                    }
                }

                // Create a new cache
                let tl_pin = sel.top_left(unsafe { &*s });
                let br_pin = sel.bottom_right(unsafe { &*s });
                self.selection_cache = Some(SelectionCache {
                    selection: Selection::init(tl_pin, br_pin, sel.rectangle),
                    tl_pin,
                    br_pin,
                });
                let c = self.selection_cache.as_ref().unwrap();
                break 'cache SelectionCache {
                    selection: c.selection,
                    tl_pin: c.tl_pin,
                    br_pin: c.br_pin,
                };
            };

            // Grab the inefficient data we need from the selection. At
            // least we can cache it.
            let tl = unsafe { (*s).pages.point_from_pin(point::Tag::Screen, cache.tl_pin) }
                .unwrap()
                .coord();
            let br = unsafe { (*s).pages.point_from_pin(point::Tag::Screen, cache.br_pin) }
                .unwrap()
                .coord();

            // We need to determine if our selection is within the viewport.
            // The viewport is generally very small so the efficient way to
            // do this is to traverse the viewport pages and check for the
            // matching selection pages.
            for row in self.row_data.iter_mut() {
                let pin = row.pin;
                let p = unsafe { (*s).pages.point_from_pin(point::Tag::Screen, pin) }
                    .unwrap()
                    .coord();
                let row_sel = match sel.contained_row_cached(
                    unsafe { &*s },
                    cache.tl_pin,
                    cache.br_pin,
                    pin,
                    tl,
                    br,
                    p,
                ) {
                    Some(rs) => rs,
                    None => continue,
                };
                let start = row_sel.start();
                let end = row_sel.end();
                assert!(core::ptr::eq(start.node, end.node));
                assert!(start.x <= end.x);
                assert!(start.y == end.y);
                row.selection = Some([start.x, end.x]);
            }
        }

        // Handle dirty state.
        if redraw {
            // Fully redraw resets some other state.
            self.screen = t.screens.active_key;
            self.dirty = Dirty::Full;
            // PORT NOTE: reference defect (R13, UD315): every row was rebuilt.
            self.rebuild_after_failure = false;

            // Note: we don't clear any row_data here because our rebuild
            // above did this.
        } else if any_dirty && self.dirty == Dirty::False {
            self.dirty = Dirty::Partial;
        }

        // Clear our dirty flags
        t.flags.dirty = crate::terminal::Dirty::default();
        unsafe { (*s).dirty = crate::screen::Dirty::default() };

        Ok(())
    }

    /// Update the highlights in the render state from the given flattened
    /// highlights. Because this uses flattened highlights, it does not require
    /// reading from the terminal state so it should be done outside of
    /// any critical sections.
    ///
    /// This will not clear any previous highlights, so the caller must
    /// manually clear them if desired.
    pub fn update_highlights_flattened(
        &mut self,
        alloc: Allocator,
        tag: u8,
        hls: &[highlight::Flattened],
    ) -> Result<(), OutOfMemory> {
        // Fast path, we have no highlights!
        if hls.is_empty() {
            return Ok(());
        }

        // This is, admittedly, horrendous. This is some low hanging fruit
        // to optimize. In my defense, screens are usually small, the number
        // of highlights is usually small, and this only happens on the
        // viewport outside of a locked area. Still, I'd love to see this
        // improved someday.

        // We need to track whether any row had a match so we can mark
        // the dirty state.
        let mut any_dirty: bool = false;

        let cols = self.cols;
        for row in self.row_data.iter_mut() {
            let row_pin = row.pin;
            let row_serial = row.serial;
            for hl in hls.iter() {
                let nodes_len = hl.chunks.len();
                for (i, chunk) in hl.chunks.iter().enumerate() {
                    let node = chunk.node;
                    // If this node generation doesn't match or we're not
                    // within the row range, skip it. Both serials are copied
                    // values, so this never dereferences a node outside the
                    // terminal lock.
                    if !core::ptr::eq(node, row_pin.node)
                        || chunk.serial != row_serial
                        || row_pin.y < chunk.start
                        || row_pin.y >= chunk.end
                    {
                        continue;
                    }

                    // We're a match!
                    //
                    // PORT NOTE: `try row_highlights.append(arena_alloc, ...)`;
                    // the list is on the update allocator (see `Row`).
                    row.highlights.allocator().debug_assert_same(alloc);
                    let highlight = Highlight {
                        tag,
                        range: [
                            if i == 0 && row_pin.y == hl.chunks[0].start {
                                hl.top_x
                            } else {
                                0
                            },
                            if i == nodes_len - 1 && row_pin.y == hl.chunks[nodes_len - 1].end - 1 {
                                hl.bot_x
                            } else {
                                cols - 1
                            },
                        ],
                    };
                    try_append::<_, HIGHLIGHT_ZIG_SIZE>(&mut row.highlights, highlight)?;

                    row.dirty = true;
                    any_dirty = true;
                }
            }
        }

        // Mark our dirty state.
        if any_dirty && self.dirty == Dirty::False {
            self.dirty = Dirty::Partial;
        }

        Ok(())
    }

    /// Convert the current render state contents to a UTF-8 encoded
    /// string written to the given writer. This will unwrap all the wrapped
    /// rows. This is useful for a minimal viewport search.
    ///
    /// This currently writes empty cell contents as \x00 and writes all
    /// blank lines. This is fine for our current usage (link search) but
    /// we can adjust this later.
    ///
    /// NOTE: There is a limitation in that wrapped lines before/after
    /// the top/bottom line of the viewport are not included, since
    /// the render state cuts them off.
    //
    // PORT NOTE: the reference takes a `*std.Io.Writer` + optional `map`
    // (`StringMap` builder). The Rust port writes into a `Vec<u8>` (UTF-8) and
    // takes an optional `&mut StringMap` (`Vec<point::Coordinate>`); the inner
    // `alloc` is folded into `StringMap`/`Vec` ownership.
    pub fn string(
        &self,
        _alloc: Allocator,
        writer: &mut Vec<u8>,
        mut map: Option<&mut StringMap>,
    ) -> Result<(), StringError> {
        for (y, row) in self.row_data.iter().enumerate() {
            // PORT NOTE (reshape): `cells_slice.items(.raw)` / `.items(.grapheme)`
            // -> the SoA columns, indexed by `x`.
            let cells = &row.cells;
            for x in 0..cells.len() {
                let raw = cells.raw()[x];
                let cp = raw.codepoint();
                let mut len: usize =
                    utf8_codepoint_sequence_length(cp).ok_or(StringError::WriteFailed)?;
                write_codepoint(writer, cp).map_err(|_| StringError::WriteFailed)?;
                if raw.has_grapheme() {
                    for &gcp in cells.grapheme(x).iter() {
                        len +=
                            utf8_codepoint_sequence_length(gcp).ok_or(StringError::WriteFailed)?;
                        write_codepoint(writer, gcp).map_err(|_| StringError::WriteFailed)?;
                    }
                }

                if let Some(m) = map.as_deref_mut() {
                    for _ in 0..len {
                        m.push(point::Coordinate {
                            x: x as size::CellCountInt,
                            y: y as u32,
                        });
                    }
                }
            }

            if !row.raw.wrap() {
                writer.extend_from_slice(b"\n");
                if let Some(m) = map.as_deref_mut() {
                    m.push(point::Coordinate {
                        x: row.cells.len() as size::CellCountInt,
                        y: y as u32,
                    });
                }
            }
        }

        Ok(())
    }

    /// Returns a map of the cells that match to an OSC8 hyperlink over the
    /// given point in the render state.
    ///
    /// IMPORTANT: The terminal must not have updated since the last call to
    /// `update`. If there is any chance the terminal has updated, the caller
    /// must first call `update` again to refresh the render state.
    ///
    /// For example, you may want to hold a lock for the duration of the
    /// update and hyperlink lookup to ensure no updates happen in between.
    pub fn link_cells(
        &self,
        alloc: Allocator,
        viewport_point: point::Coordinate,
    ) -> Result<CellSet, OutOfMemory> {
        let mut result: CellSet = CellSet::new();
        // errdefer result.deinit(alloc);

        // Our viewport point is sent in by the caller and can't be trusted.
        // If it is outside the valid area then just return empty because
        // we can't possibly have a link there.
        if viewport_point.x >= self.cols || viewport_point.y >= self.row_data.len() as u32 {
            return Ok(result);
        }

        // Grab our link ID
        let link_pin: Pin = self.row_data[viewport_point.y as usize].pin;
        let link_page: *mut page::Page = unsafe { crate::pagelist::Node::page(link_pin.node) };
        let link = 'link: {
            let rac = unsafe {
                (*link_page).get_row_and_cell(viewport_point.x as usize, link_pin.y as usize)
            };

            // The likely scenario is that our mouse isn't even over a link.
            if !unsafe { (*rac.cell).hyperlink() } {
                // @branchHint(.likely);
                return Ok(result);
            }

            let link_id = match unsafe { (*link_page).lookup_hyperlink(rac.cell) } {
                Some(id) => id,
                None => return Ok(result),
            };
            break 'link unsafe {
                let base = (*link_page).memory_ptr();
                (*link_page).hyperlink_set.get(base, link_id)
            };
        };

        for (y, row) in self.row_data.iter().enumerate() {
            let pin = row.pin;
            // PORT NOTE (reshape): `cells.items(.raw)` -> the SoA raw column.
            let raw_cells = row.cells.raw();
            'cells: for (x, raw) in raw_cells.iter().enumerate() {
                if !raw.hyperlink() {
                    continue;
                }

                let other_page: *mut page::Page = unsafe { crate::pagelist::Node::page(pin.node) };
                let other = 'link2: {
                    let rac = unsafe { (*other_page).get_row_and_cell(x, pin.y as usize) };
                    let link_id = match unsafe { (*other_page).lookup_hyperlink(rac.cell) } {
                        Some(id) => id,
                        None => continue 'cells,
                    };
                    break 'link2 unsafe {
                        let base = (*other_page).memory_ptr();
                        (*other_page).hyperlink_set.get(base, link_id)
                    };
                };

                if unsafe {
                    (*link).eql(
                        (*link_page).memory_ptr(),
                        &*other,
                        (*other_page).memory_ptr(),
                    )
                } {
                    result.put(
                        point::Coordinate {
                            y: y as u32,
                            x: x as size::CellCountInt,
                        },
                        (),
                    );
                }
            }
        }

        let _ = alloc;
        Ok(result)
    }
}

/// A set of coordinates representing cells.
// PORT NOTE: `CellSet = std.AutoArrayHashMapUnmanaged(point.Coordinate, void)`.
pub type CellSet = rustty_zigstd::array_hash_map::AutoArrayHashMapUnmanaged<point::Coordinate, ()>;

// PORT NOTE: `StringMap = std.ArrayListUnmanaged(point.Coordinate)` (distinct
// from `terminal/StringMap.zig`'s oniguruma-backed `StringMap`).
pub type StringMap = Vec<point::Coordinate>;

/// PORT NOTE: the reference `string` errors are `Allocator.Error ||
/// std.Io.Writer.Error`. Since the Rust writer is an infallible `Vec<u8>`, only
/// the `WriteFailed` (invalid codepoint length) case can arise; modeled here.
#[derive(Debug)]
pub enum StringError {
    WriteFailed,
}

/// `std.unicode.utf8CodepointSequenceLength`: the number of bytes a codepoint
/// encodes to, or `None` (`error.CodepointTooLarge`) if out of range.
fn utf8_codepoint_sequence_length(cp: u32) -> Option<usize> {
    if cp < 0x80 {
        Some(1)
    } else if cp < 0x800 {
        Some(2)
    } else if cp < 0x10000 {
        Some(3)
    } else if cp < 0x110000 {
        Some(4)
    } else {
        None
    }
}

/// `writer.print("{u}", .{cp})`: encode a codepoint as UTF-8. Zig 0.16's `{u}`
/// (`std.Io.Writer.printUnicodeCodepoint`) writes the replacement character
/// U+FFFD (`EF BF BD`) when `utf8Encode` fails with
/// `Utf8CannotEncodeSurrogateHalf` or `CodepointTooLarge`.
// CORRECTION (parity S161-F2): this doc comment used to say Zig's `{u}`
// "encodes the surrogate range losslessly via WTF-8-ish encoding", and the
// surrogate arm wrote the 3-byte generalized-UTF-8 encoding of the surrogate
// (`ED A0 80` for U+D800). The out-of-range arm still returns `Err`: `string`
// rejects that value with `utf8CodepointSequenceLength` before writing it.
fn write_codepoint(writer: &mut Vec<u8>, cp: u32) -> Result<(), ()> {
    match char::from_u32(cp) {
        Some(c) => {
            let mut buf = [0u8; 4];
            writer.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            Ok(())
        }
        None => {
            // Surrogate or out-of-range.
            let len = utf8_codepoint_sequence_length(cp).ok_or(())?;
            match len {
                // buf[0..3].* = std.unicode.replacement_character_utf8;
                3 => {
                    writer.extend_from_slice("\u{FFFD}".as_bytes());
                    Ok(())
                }
                _ => Err(()),
            }
        }
    }
}

// =============================================================================
// Group scans (446f80f4e, WS10 T19)
// =============================================================================

/// The number of rows/cells we scan as a single group when looking for
/// dirty rows or special cells. Rows and cells are small packed structs
/// so a group is scanned with a handful of vector operations.
const SCAN_GROUP_LEN: usize = 8;

/// Group scan helper for the row dirty flag. A row that matches has
/// its dirty flag unset.
type RowDirtyMask =
    page::Mask<page::Row, { page::row_field_mask(&[page::RowField::Dirty]) }, SCAN_GROUP_LEN>;

/// Group scan helper for the cell fields that require managed memory
/// handling. A cell that matches is a plain (possibly zero) codepoint
/// with a default style, requiring no work beyond the raw copy. See
/// `row_builder`.
type CellSpecialMask = page::Mask<
    page::Cell,
    { page::cell_field_mask(&[page::CellField::ContentTag, page::CellField::StyleId]) },
    SCAN_GROUP_LEN,
>;

/// Internal helper for `RenderState::begin_update` that rebuilds a single row of
/// the render state from the current page contents.
///
/// PORT NOTE: reference `const RowBuilder = struct { ... fn row(...) }`. The
/// reference's struct exists to carry the hoisted `row_data` COLUMN slices;
/// rustty's `row_data` is a `Vec<Row>` of owned rows, so the equivalent is a
/// free function over the two disjoint `&mut` field borrows. Every growth is
/// through `alloc` (`b.alloc`) and fallible, as in the reference.
#[allow(clippy::needless_range_loop)]
fn row_builder(
    alloc: Allocator,
    row_data: &mut [Row],
    pending_styles: &mut AVec<StyleRun>,
    cols: size::CellCountInt,
    p: *mut page::Page,
    page_row: *const page::Row,
    vy: usize,
) -> Result<(), OutOfMemory> {
    // Reset our per-row state if we're rebuilding this row. A
    // non-zero cell length means the row was populated by a prior
    // update.
    //
    // PORT NOTE: `_ = arena.reset(.retain_capacity)` resets the grapheme
    // buffer, and `b.highlights[vy] = .empty` clears the highlight list, both
    // keeping their capacity as the arena does (see `Row`).
    {
        let row = &mut row_data[vy];
        if !row.cells.is_empty() {
            row.cells.reset_graphemes();
            row.selection = None;
            row.highlights.clear();
        }
        row.dirty = true;

        // Copy our raw row data
        row.raw = unsafe { *page_row };

        // Note: our cells MultiArrayList uses our general allocator.
        // We do this on purpose because as rows become dirty, we do
        // not want to reallocate space for cells (which are large). This
        // was a source of huge slowdown.
        //
        // Our per-row arena is only used for temporary allocations
        // pertaining to cells directly (e.g. graphemes, hyperlinks).
        if row.cells.len() != cols as usize {
            // The cell storage (including the per-cell style data) is
            // being reallocated, so the applied style cache no longer
            // describes it. Clear it before the resize so an error
            // can't leave it stale.
            row.applied_styles.clear();
            row.cells.resize(alloc, cols as usize)?;
        }
    }

    // Get all our cells in the page.
    let page_cells: &[page::Cell] = unsafe {
        core::slice::from_raw_parts((*page_row).cells().ptr((*p).memory_ptr()), cols as usize)
    };

    // We always copy our raw cell data. In the case we have no
    // managed memory, we can skip setting any other fields.
    //
    // This is an important optimization. For plain-text screens
    // this ends up being something around 300% faster based on
    // the `screen-clone` benchmark.
    //
    // PERF(port) [PRESERVED across 446f80f4e]: `copy_raw` fills the whole `raw`
    // column with one memcpy and leaves `style`/`grapheme` untouched, instead of
    // a per-cell write.
    row_data[vy].cells.copy_raw(page_cells);
    if !unsafe { (*page_row).managed_memory() } {
        return Ok(());
    }

    let n = page_cells.len();
    // const runs_start = b.pending_styles.items.len;
    let runs_start = pending_styles.len();
    // PERF(port) [PRESERVED]: hoist the style-set base resolution once per row —
    // `(*p).memory_ptr()` and the set's items offset are loop-invariant, but the
    // grapheme arm's `try_extend_copy` clobbers memory, so LLVM re-loads both
    // per styled cell unless they live in pre-loop locals.
    let styles: *const crate::style::Set = unsafe { core::ptr::addr_of!((*p).styles) };
    let style_items = unsafe { (*styles).items_ptr((*p).memory_ptr()) };

    let mut x: usize = 0;
    'scan: while x < n {
        // Skip runs of plain cells a group at a time. Cells that
        // need managed handling are often rare even within rows that
        // have managed memory (e.g. a row is "styled" if a single
        // cell has a style) so groups are skipped with a few vector
        // operations.
        while n - x >= CellSpecialMask::GROUP_LEN {
            if !CellSpecialMask::match_group(page_cells, x) {
                break;
            }
            x += CellSpecialMask::GROUP_LEN;
        }

        // Scalar scan to the next special cell.
        loop {
            if x >= n {
                break 'scan;
            }
            if !CellSpecialMask::match_scalar(page_cells[x]) {
                break;
            }
            x += 1;
        }

        let page_cell = &page_cells[x];

        match page_cell.content_tag() {
            // Single-codepoint styled cells are by far the most
            // common special cells, and they usually come in long
            // runs sharing one style ID (e.g. a fully styled row
            // usually uses a single style). Find the run and record
            // it: this does one style lookup per run and defers the
            // (large) per-cell fill to `end_update`, outside of any
            // terminal locks.
            page::ContentTag::Codepoint => {
                // @branchHint(.likely);
                let sid = page_cell.style_id();
                debug_assert!(sid > 0); // special + codepoint implies styled
                let style_val: Style = unsafe { *(*styles).get_at(style_items, sid) };

                // A cell continues the run if its masked special
                // bits are exactly the style ID of the run (in
                // particular the content tag must be a plain
                // codepoint). We can check groups of cells at a
                // time this way.
                let pattern = CellSpecialMask::pattern(*page_cell);
                let start = x;
                x += 1;
                while n - x >= CellSpecialMask::GROUP_LEN {
                    if !CellSpecialMask::eql(page_cells, x, pattern) {
                        break;
                    }
                    x += CellSpecialMask::GROUP_LEN;
                }
                while x < n {
                    if !CellSpecialMask::eql_scalar(page_cells[x], pattern) {
                        break;
                    }
                    x += 1;
                }

                try_append::<_, STYLE_RUN_ZIG_SIZE>(
                    pending_styles,
                    StyleRun {
                        y: vy as size::CellCountInt,
                        start: start as size::CellCountInt,
                        end: x as size::CellCountInt,
                        style: style_val,
                    },
                )?;
            }

            // If we have a multi-codepoint grapheme, look it up and
            // set our content type. Note grapheme cells may also
            // be styled. The style must be recorded as a run (rather
            // than written directly) so that it is ordered correctly
            // relative to possibly-stale runs from prior updates.
            page::ContentTag::CodepointGrapheme => {
                if page_cell.style_id() > 0 {
                    let style_val: Style =
                        unsafe { *(*styles).get_at(style_items, page_cell.style_id()) };
                    try_append::<_, STYLE_RUN_ZIG_SIZE>(
                        pending_styles,
                        StyleRun {
                            y: vy as size::CellCountInt,
                            start: x as size::CellCountInt,
                            end: (x + 1) as size::CellCountInt,
                            style: style_val,
                        },
                    )?;
                }

                // PORT NOTE (reshape): the reference arena-`dupe`s the grapheme;
                // rustty appends the codepoints into the row's reusable
                // `grapheme_buf` and records their span.
                let g: &[u32] = unsafe {
                    match (*p).lookup_grapheme(page_cell as *const page::Cell) {
                        Some(s) => s,
                        None => &[],
                    }
                };
                let (_, cells_spans, grapheme_buf) = row_data[vy].cells.slice_mut();
                let off = grapheme_buf.len() as u32;
                // cells_grapheme[x] = try arena_alloc.dupe(u21, ...);
                grapheme_buf.try_extend_copy(g).map_err(|_| OutOfMemory)?;
                cells_spans[x] = (off, g.len() as u32);
                x += 1;
            }

            // Background-color-only cells. The style is derived
            // entirely from the cell contents. Consecutive cleared
            // cells with the same background are bit-identical, so
            // we run-detect on full equality (e.g. a line cleared
            // with a background color pending is one run).
            page::ContentTag::BgColorRgb | page::ContentTag::BgColorPalette => {
                let style_val: Style = match page_cell.content_tag() {
                    page::ContentTag::BgColorRgb => {
                        let rgb = page_cell.content_color_rgb();
                        Style {
                            bg_color: crate::style::Color::Rgb(color::RGB {
                                r: rgb.r,
                                g: rgb.g,
                                b: rgb.b,
                            }),
                            ..Style::default()
                        }
                    }
                    page::ContentTag::BgColorPalette => Style {
                        bg_color: crate::style::Color::Palette(page_cell.content_color_palette()),
                        ..Style::default()
                    },
                    // else => unreachable
                    _ => unreachable!(),
                };

                let first_bits = CellSpecialMask::bits(*page_cell);
                let start = x;
                x += 1;
                while n - x >= CellSpecialMask::GROUP_LEN {
                    if !CellSpecialMask::eql_exact(page_cells, x, first_bits) {
                        break;
                    }
                    x += CellSpecialMask::GROUP_LEN;
                }
                while x < n {
                    if CellSpecialMask::bits(page_cells[x]) != first_bits {
                        break;
                    }
                    x += 1;
                }

                try_append::<_, STYLE_RUN_ZIG_SIZE>(
                    pending_styles,
                    StyleRun {
                        y: vy as size::CellCountInt,
                        start: start as size::CellCountInt,
                        end: x as size::CellCountInt,
                        style: style_val,
                    },
                )?;
            }
        }
    }

    // Reserve the applied style cache capacity for the runs we
    // appended so that endUpdate (which cannot allocate) is able
    // to record what it applies. See Row.applied_styles.
    //
    let runs_added = pending_styles.len() - runs_start;
    if runs_added > 0 {
        // try b.applied_styles[vy].ensureTotalCapacity(b.alloc, runs_added);
        let applied = &mut row_data[vy].applied_styles;
        applied.allocator().debug_assert_same(alloc);
        ensure_total_capacity::<_, STYLE_RUN_ZIG_SIZE>(applied, runs_added)?;
    }

    Ok(())
}

/// `try self.row_data.resize(alloc, rows)`'s capacity: `ensureTotalCapacity`
/// (`setCapacity(growCapacity(rows))` when short: a fresh allocation, the rows
/// moved over, the old one freed, never `remap`). The caller then appends the
/// new rows into the reserved capacity.
#[cold]
#[inline(never)]
fn resize_rows_capacity(row_data: &mut AVec<Row>, rows: usize) -> Result<(), OutOfMemory> {
    if row_data.capacity() >= rows {
        return Ok(());
    }
    let mut other: AVec<Row> = AVec::new_in(*row_data.allocator());
    other
        .try_reserve_exact(grow_capacity::<ROW_ZIG_FIELD_MAX>(rows))
        .map_err(|_| OutOfMemory)?;
    other.extend(row_data.drain(..));
    *row_data = other;
    Ok(())
}

#[allow(unused_imports)]
use pagelist as _pagelist;
#[allow(unused_imports)]
use PageList as _PageList;
#[allow(unused_imports)]
use ScreenSet as _ScreenSet;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alloc::Global;
    use crate::kitty;
    use crate::modes as modespkg;
    use crate::screen_set::Key;
    use crate::terminal::{Colors, Options, ScrollViewport};

    // PORT NOTE: the reference `Terminal.init(alloc, .{ .cols, .rows })` relies on
    // `Options` field defaults (`max_scrollback_bytes = 10_000`, `colors = .default`,
    // `default_modes = .{}`, kitty limits). Rust `Options` has no field defaults,
    // so this helper supplies them verbatim, mirroring the reference declaration.
    fn opts(cols: size::CellCountInt, rows: size::CellCountInt) -> Options {
        Options {
            cols,
            rows,
            max_scrollback_bytes: Some(10_000),
            max_scrollback_lines: None,
            colors: Colors::default(),
            default_modes: modespkg::ModePacked::default(),
            default_cursor_style: crate::screen::CursorStyle::Block,
            default_cursor_blink: Some(false),
            kitty_image_storage_limit: 320 * 1000 * 1000,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        }
    }

    fn opts_sb(
        cols: size::CellCountInt,
        rows: size::CellCountInt,
        max_scrollback_bytes: usize,
    ) -> Options {
        Options {
            max_scrollback_bytes: Some(max_scrollback_bytes),
            max_scrollback_lines: None,
            ..opts(cols, rows)
        }
    }

    // test "styled"
    #[test]
    fn styled() {
        let alloc = Global;

        let mut t = Terminal::init(alloc, opts(80, 24)).unwrap();

        // This fills the screen up
        t.decaln().unwrap();

        let mut state: RenderState = RenderState::empty();
        state.update(alloc, &mut t).unwrap();
        state.deinit(alloc);
        t.deinit(alloc);
    }

    // test "basic text"
    #[test]
    fn basic_text() {
        let alloc = Global;

        let mut t = Terminal::init(alloc, opts(10, 3)).unwrap();

        {
            let mut s = t.vt_stream();
            s.next_slice(b"ABCD");
            s.deinit();
        }

        let mut state: RenderState = RenderState::empty();
        state.update(alloc, &mut t).unwrap();

        // Verify we have the right number of rows
        assert_eq!(3, state.row_data.len());

        // All rows should have cols cells
        assert_eq!(10, state.row_data[0].cells.len());
        assert_eq!(10, state.row_data[1].cells.len());
        assert_eq!(10, state.row_data[2].cells.len());

        // Row zero should contain our text
        assert_eq!('A' as u32, state.row_data[0].cells.get(0).raw.codepoint());
        assert_eq!('B' as u32, state.row_data[0].cells.get(1).raw.codepoint());
        assert_eq!('C' as u32, state.row_data[0].cells.get(2).raw.codepoint());
        assert_eq!('D' as u32, state.row_data[0].cells.get(3).raw.codepoint());
        assert_eq!(0, state.row_data[0].cells.get(4).raw.codepoint());

        state.deinit(alloc);
        t.deinit(alloc);
    }

    // test "styled text"
    #[test]
    fn styled_text() {
        let alloc = Global;

        let mut t = Terminal::init(alloc, opts(10, 3)).unwrap();

        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b[1mA"); // Bold
            s.next_slice(b"\x1b[0;3mB"); // Italic
            s.next_slice(b"\x1b[0;4mC"); // Underline
            s.deinit();
        }

        let mut state: RenderState = RenderState::empty();
        state.update(alloc, &mut t).unwrap();

        // Verify we have the right number of rows
        assert_eq!(3, state.row_data.len());

        // All rows should have cols cells
        assert_eq!(10, state.row_data[0].cells.len());
        assert_eq!(10, state.row_data[1].cells.len());
        assert_eq!(10, state.row_data[2].cells.len());

        // Row zero should contain our text
        {
            let cell = &state.row_data[0].cells.get(0);
            assert_eq!('A' as u32, cell.raw.codepoint());
            assert!(cell.style.flags.bold);
        }
        {
            let cell = &state.row_data[0].cells.get(1);
            assert_eq!('B' as u32, cell.raw.codepoint());
            assert!(!cell.style.flags.bold);
            assert!(cell.style.flags.italic);
        }
        assert_eq!('C' as u32, state.row_data[0].cells.get(2).raw.codepoint());
        assert_eq!(0, state.row_data[0].cells.get(3).raw.codepoint());

        state.deinit(alloc);
        t.deinit(alloc);
    }

    // PORT NOTE: port-only regression guard (no reference counterpart) for the
    // r4-render-win managed-loop reshape (hoisted style-set base + zip
    // iteration): adjacent cells with DIFFERENT style ids must each resolve to
    // their own style. Written against the pre-reshape loop first; outputs
    // must be identical pre/post.
    #[test]
    fn mixed_styles_adjacent_cells() {
        let alloc = Global;

        let mut t = Terminal::init(alloc, opts(10, 3)).unwrap();

        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b[41mA"); // bg palette 1
            s.next_slice(b"\x1b[42mB"); // bg palette 2
            s.next_slice(b"\x1b[43mC"); // bg palette 3
            s.next_slice(b"\x1b[0mD"); // default
            s.deinit();
        }

        let mut state: RenderState = RenderState::empty();
        state.update(alloc, &mut t).unwrap();

        assert_eq!(3, state.row_data.len());
        assert_eq!(10, state.row_data[0].cells.len());

        {
            let cell = &state.row_data[0].cells.get(0);
            assert_eq!('A' as u32, cell.raw.codepoint());
            assert!(cell.style.bg_color.eql(crate::style::Color::Palette(1)));
        }
        {
            let cell = &state.row_data[0].cells.get(1);
            assert_eq!('B' as u32, cell.raw.codepoint());
            assert!(cell.style.bg_color.eql(crate::style::Color::Palette(2)));
        }
        {
            let cell = &state.row_data[0].cells.get(2);
            assert_eq!('C' as u32, cell.raw.codepoint());
            assert!(cell.style.bg_color.eql(crate::style::Color::Palette(3)));
        }
        {
            let cell = &state.row_data[0].cells.get(3);
            assert_eq!('D' as u32, cell.raw.codepoint());
            assert!(cell.style.default_style());
        }

        state.deinit(alloc);
        t.deinit(alloc);
    }

    // PORT NOTE: port-only regression guard (no reference counterpart), the
    // memoization worst case: two style ids ALTERNATING every cell, with a
    // multi-codepoint grapheme mid-row so the grapheme arm's
    // `extend_from_slice` runs between style resolutions (the call whose
    // memory clobber motivates the hoisted style-set base pointer). Outputs
    // must be identical pre/post reshape.
    #[test]
    fn mixed_styles_alternating_per_cell() {
        let alloc = Global;

        let mut t = Terminal::init(alloc, opts(10, 3)).unwrap();

        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b[41mA"); // x=0: bg palette 1
            s.next_slice(b"\x1b[42mB"); // x=1: bg palette 2
            s.next_slice(b"\x1b[41m"); // x=2..3: bg palette 1, wide grapheme
            s.next_slice("👨\u{200D}".as_bytes()); // ZWJ grapheme
            s.next_slice(b"\x1b[42mC"); // x=4: bg palette 2
            s.next_slice(b"\x1b[41mD"); // x=5: bg palette 1
            s.deinit();
        }

        let mut state: RenderState = RenderState::empty();
        state.update(alloc, &mut t).unwrap();

        assert_eq!(3, state.row_data.len());
        assert_eq!(10, state.row_data[0].cells.len());

        {
            let cell = &state.row_data[0].cells.get(0);
            assert_eq!('A' as u32, cell.raw.codepoint());
            assert!(cell.style.bg_color.eql(crate::style::Color::Palette(1)));
        }
        {
            let cell = &state.row_data[0].cells.get(1);
            assert_eq!('B' as u32, cell.raw.codepoint());
            assert!(cell.style.bg_color.eql(crate::style::Color::Palette(2)));
        }
        {
            let cell = &state.row_data[0].cells.get(2);
            assert_eq!(0x1F468, cell.raw.codepoint());
            assert_eq!(page::Wide::Wide, cell.raw.wide());
            assert_eq!(&[0x200Du32][..], cell.grapheme);
            assert!(cell.style.bg_color.eql(crate::style::Color::Palette(1)));
        }
        {
            let cell = &state.row_data[0].cells.get(3);
            assert_eq!(page::Wide::SpacerTail, cell.raw.wide());
        }
        {
            let cell = &state.row_data[0].cells.get(4);
            assert_eq!('C' as u32, cell.raw.codepoint());
            assert!(cell.style.bg_color.eql(crate::style::Color::Palette(2)));
        }
        {
            let cell = &state.row_data[0].cells.get(5);
            assert_eq!('D' as u32, cell.raw.codepoint());
            assert!(cell.style.bg_color.eql(crate::style::Color::Palette(1)));
        }
        {
            let cell = &state.row_data[0].cells.get(6);
            assert_eq!(0, cell.raw.codepoint());
            assert!(cell.style.default_style());
        }

        state.deinit(alloc);
        t.deinit(alloc);
    }

    // test "grapheme"
    #[test]
    fn grapheme() {
        let alloc = Global;

        let mut t = Terminal::init(alloc, opts(10, 3)).unwrap();

        {
            let mut s = t.vt_stream();
            s.next_slice("A".as_bytes());
            s.next_slice("👨\u{200D}".as_bytes()); // this has a ZWJ
            s.deinit();
        }

        let mut state: RenderState = RenderState::empty();
        state.update(alloc, &mut t).unwrap();

        // Verify we have the right number of rows
        assert_eq!(3, state.row_data.len());

        // All rows should have cols cells
        assert_eq!(10, state.row_data[0].cells.len());
        assert_eq!(10, state.row_data[1].cells.len());
        assert_eq!(10, state.row_data[2].cells.len());

        // Row zero should contain our text
        {
            let cell = &state.row_data[0].cells.get(0);
            assert_eq!('A' as u32, cell.raw.codepoint());
        }
        {
            let cell = &state.row_data[0].cells.get(1);
            assert_eq!(0x1F468, cell.raw.codepoint());
            assert_eq!(page::Wide::Wide, cell.raw.wide());
            assert_eq!(&[0x200Du32][..], cell.grapheme);
        }
        {
            let cell = &state.row_data[0].cells.get(2);
            assert_eq!(0, cell.raw.codepoint());
            assert_eq!(page::Wide::SpacerTail, cell.raw.wide());
        }

        state.deinit(alloc);
        t.deinit(alloc);
    }

    // test "cursor state in viewport"
    #[test]
    fn cursor_state_in_viewport() {
        let alloc = Global;

        let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

        // PORT NOTE: the reference holds a single `Stream` across writes/updates.
        // The Rust `Stream<'_>` borrows the `&mut Terminal`, so we scope each
        // write batch in a block before touching `t`/`state.update`. Each write
        // is a complete escape sequence, so the (re)created stream is behaviorally
        // identical (no split-sequence parser state to preserve).
        {
            let mut s = t.vt_stream();
            s.next_slice(b"A\x1b[H");
            s.deinit();
        }

        let mut state: RenderState = RenderState::empty();

        // Initial update
        state.update(alloc, &mut t).unwrap();
        assert_eq!(0, state.cursor.active.x);
        assert_eq!(0, state.cursor.active.y);
        assert_eq!(0, state.cursor.viewport.as_ref().unwrap().x);
        assert_eq!(0, state.cursor.viewport.as_ref().unwrap().y);
        assert_eq!('A' as u32, state.cursor.cell.codepoint());
        assert!(state.cursor.style.default_style());

        // Set a style on the cursor
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b[1m"); // Bold
            s.deinit();
        }
        state.update(alloc, &mut t).unwrap();
        assert!(!state.cursor.style.default_style());
        assert!(state.cursor.style.flags.bold);
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b[0m"); // Reset style

            // Move cursor to 2,1
            s.next_slice(b"\x1b[2;3H");
            s.deinit();
        }
        state.update(alloc, &mut t).unwrap();
        assert_eq!(2, state.cursor.active.x);
        assert_eq!(1, state.cursor.active.y);
        assert_eq!(2, state.cursor.viewport.as_ref().unwrap().x);
        assert_eq!(1, state.cursor.viewport.as_ref().unwrap().y);

        state.deinit(alloc);
        t.deinit(alloc);
    }

    // test "cursor state out of viewport"
    #[test]
    fn cursor_state_out_of_viewport() {
        let alloc = Global;

        let mut t = Terminal::init(alloc, opts(10, 2)).unwrap();

        {
            let mut s = t.vt_stream();
            s.next_slice(b"A\r\nB\r\nC\r\nD\r\n");
            s.deinit();
        }

        let mut state: RenderState = RenderState::empty();

        // Initial update
        state.update(alloc, &mut t).unwrap();
        assert_eq!(0, state.cursor.active.x);
        assert_eq!(1, state.cursor.active.y);
        assert_eq!(0, state.cursor.viewport.as_ref().unwrap().x);
        assert_eq!(1, state.cursor.viewport.as_ref().unwrap().y);

        // Scroll the viewport
        t.scroll_viewport(ScrollViewport::Top);
        state.update(alloc, &mut t).unwrap();

        // Set a style on the cursor
        assert_eq!(0, state.cursor.active.x);
        assert_eq!(1, state.cursor.active.y);
        assert!(state.cursor.viewport.is_none());

        state.deinit(alloc);
        t.deinit(alloc);
    }

    // test "dirty state"
    #[test]
    fn dirty_state() {
        let alloc = Global;

        let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

        let mut state: RenderState = RenderState::empty();

        // First update should trigger redraw due to resize
        state.update(alloc, &mut t).unwrap();
        assert_eq!(Dirty::Full, state.dirty);

        // Reset dirty flag and dirty rows
        state.dirty = Dirty::False;
        {
            for row in state.row_data.iter_mut() {
                row.dirty = false;
            }
        }

        // Second update with no changes - no dirty rows
        state.update(alloc, &mut t).unwrap();
        assert_eq!(Dirty::False, state.dirty);
        {
            for row in state.row_data.iter() {
                assert!(!row.dirty);
            }
        }

        // Write to first line
        {
            let mut s = t.vt_stream();
            s.next_slice(b"A");
            s.deinit();
        }
        state.update(alloc, &mut t).unwrap();
        assert_eq!(Dirty::Partial, state.dirty);
        {
            assert!(state.row_data[0].dirty); // First row dirty
            assert!(!state.row_data[1].dirty); // Second row clean
        }

        state.deinit(alloc);
        t.deinit(alloc);
    }

    // test "colors"
    #[test]
    fn colors() {
        let alloc = Global;

        let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

        let mut state: RenderState = RenderState::empty();

        // Default colors
        state.update(alloc, &mut t).unwrap();

        // Change cursor color
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b]12;#FF0000\x07");
            s.deinit();
        }
        state.update(alloc, &mut t).unwrap();

        let c = state.colors.cursor.unwrap();
        assert_eq!(0xFF, c.r);
        assert_eq!(0, c.g);
        assert_eq!(0, c.b);

        // Change palette color 0 to White
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b]4;0;#FFFFFF\x07");
            s.deinit();
        }
        state.update(alloc, &mut t).unwrap();
        let p0 = state.colors.palette[0];
        assert_eq!(0xFF, p0.r);
        assert_eq!(0xFF, p0.g);
        assert_eq!(0xFF, p0.b);

        state.deinit(alloc);
        t.deinit(alloc);
    }

    // test "selection single line"
    #[test]
    fn selection_single_line() {
        let alloc = Global;

        let mut t = Terminal::init(alloc, opts(10, 3)).unwrap();

        let screen: *mut Screen = t.screens.active;
        unsafe {
            (*screen)
                .select(Some(Selection::init(
                    (*screen)
                        .pages
                        .pin(point::Point::Active(point::Coordinate { x: 0, y: 1 }))
                        .unwrap(),
                    (*screen)
                        .pages
                        .pin(point::Point::Active(point::Coordinate { x: 2, y: 1 }))
                        .unwrap(),
                    false,
                )))
                .unwrap();
        }

        let mut state: RenderState = RenderState::empty();
        state.update(alloc, &mut t).unwrap();

        assert_eq!(None, state.row_data[0].selection);
        assert_eq!([0, 2], state.row_data[1].selection.unwrap());
        assert_eq!(None, state.row_data[2].selection);

        // Clear the selection
        unsafe { (*screen).select(None).unwrap() };
        state.update(alloc, &mut t).unwrap();
        assert_eq!(None, state.row_data[0].selection);
        assert_eq!(None, state.row_data[1].selection);
        assert_eq!(None, state.row_data[2].selection);

        state.deinit(alloc);
        t.deinit(alloc);
    }

    // test "selection multiple lines"
    #[test]
    fn selection_multiple_lines() {
        let alloc = Global;

        let mut t = Terminal::init(alloc, opts(10, 3)).unwrap();

        let screen: *mut Screen = t.screens.active;
        unsafe {
            (*screen)
                .select(Some(Selection::init(
                    (*screen)
                        .pages
                        .pin(point::Point::Active(point::Coordinate { x: 0, y: 1 }))
                        .unwrap(),
                    (*screen)
                        .pages
                        .pin(point::Point::Active(point::Coordinate { x: 2, y: 2 }))
                        .unwrap(),
                    false,
                )))
                .unwrap();
        }

        let mut state: RenderState = RenderState::empty();
        state.update(alloc, &mut t).unwrap();

        assert_eq!(None, state.row_data[0].selection);
        assert_eq!(
            [0, unsafe { (*screen).pages.cols } - 1],
            state.row_data[1].selection.unwrap(),
        );
        assert_eq!([0, 2], state.row_data[2].selection.unwrap());

        state.deinit(alloc);
        t.deinit(alloc);
    }

    // test "linkCells"
    #[test]
    fn link_cells() {
        let alloc = Global;

        let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

        let mut state: RenderState = RenderState::empty();

        // Create a hyperlink
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b]8;;http://example.com\x1b\\LINK\x1b]8;;\x1b\\");
            s.deinit();
        }
        state.update(alloc, &mut t).unwrap();

        // Query link at 0,0
        let mut cells = state
            .link_cells(alloc, point::Coordinate { x: 0, y: 0 })
            .unwrap();

        assert_eq!(4, cells.count());
        assert!(cells.contains(&point::Coordinate { x: 0, y: 0 }));
        assert!(cells.contains(&point::Coordinate { x: 1, y: 0 }));
        assert!(cells.contains(&point::Coordinate { x: 2, y: 0 }));
        assert!(cells.contains(&point::Coordinate { x: 3, y: 0 }));

        // Query no link
        let mut cells2 = state
            .link_cells(alloc, point::Coordinate { x: 4, y: 0 })
            .unwrap();
        assert_eq!(0, cells2.count());

        // PORT NOTE: `CellSet` (AutoArrayHashMap) frees on drop; the reference
        // `cells.deinit(alloc)` / `cells2.deinit(alloc)` are no-ops here.
        let _ = &mut cells;
        let _ = &mut cells2;

        state.deinit(alloc);
        t.deinit(alloc);
    }

    // Parity S161-F2 (no reference test): `string` writes each codepoint with
    // `writer.print("{u}", ..)`, which in zig 0.16
    // (`std.Io.Writer.printUnicodeCodepoint`) writes the 3-byte U+FFFD for a
    // surrogate half. Its byte count still matches the
    // `utf8CodepointSequenceLength` of 3 that `string` maps.
    #[test]
    fn write_codepoint_writes_replacement_for_a_surrogate() {
        let mut w: Vec<u8> = Vec::new();
        write_codepoint(&mut w, 'A' as u32).unwrap();
        write_codepoint(&mut w, 0xD800).unwrap();
        write_codepoint(&mut w, 0xDFFF).unwrap();
        assert_eq!(b"A\xEF\xBF\xBD\xEF\xBF\xBD".as_slice(), w.as_slice());
        assert_eq!(Some(3), utf8_codepoint_sequence_length(0xD800));
        assert!(write_codepoint(&mut w, 0x11_0000).is_err());
    }

    // test "string"
    #[test]
    fn string() {
        let alloc = Global;

        let mut t = Terminal::init(alloc, opts(5, 2)).unwrap();

        {
            let mut s = t.vt_stream();
            s.next_slice(b"AB");
            s.deinit();
        }

        let mut state: RenderState = RenderState::empty();
        state.update(alloc, &mut t).unwrap();

        let mut w: Vec<u8> = Vec::new();

        state.string(alloc, &mut w, None).unwrap();

        let result = w;

        let expected = b"AB\x00\x00\x00\n\x00\x00\x00\x00\x00\n";
        assert_eq!(&expected[..], result.as_slice());

        state.deinit(alloc);
        t.deinit(alloc);
    }

    // test "linkCells with scrollback spanning pages"
    #[cfg_attr(
        all(miri, debug_assertions),
        ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
    )]
    #[test]
    fn link_cells_with_scrollback_spanning_pages() {
        let alloc = Global;

        let viewport_rows: size::CellCountInt = 10;
        let tail_rows: size::CellCountInt = 5;

        let mut t = Terminal::init(
            alloc,
            opts_sb(page::std_capacity().cols, viewport_rows, 10_000),
        )
        .unwrap();

        let first_page_cap =
            unsafe { crate::pagelist::Node::capacity((*t.screens.active).pages.pages.first).rows };

        {
            let mut s = t.vt_stream();

            // Fill first page
            for _ in 0..(first_page_cap - 1) {
                s.next_slice(b"\r\n");
            }

            // Create second page with hyperlink
            s.next_slice(b"\r\n");
            s.next_slice(b"\x1b]8;;http://example.com\x1b\\LINK\x1b]8;;\x1b\\");
            for _ in 0..(tail_rows - 1) {
                s.next_slice(b"\r\n");
            }
            s.deinit();
        }

        let mut state: RenderState = RenderState::empty();
        state.update(alloc, &mut t).unwrap();

        let expected_viewport_y: u32 = (viewport_rows - tail_rows) as u32;
        // BUG: This crashes without the fix
        let mut cells = state
            .link_cells(
                alloc,
                point::Coordinate {
                    x: 0,
                    y: expected_viewport_y,
                },
            )
            .unwrap();
        assert_eq!(4usize, cells.count());
        let _ = &mut cells;

        state.deinit(alloc);
        t.deinit(alloc);
    }

    // test "linkCells with invalid viewport point"
    #[test]
    fn link_cells_with_invalid_viewport_point() {
        let alloc = Global;

        let mut t = Terminal::init(alloc, opts(10, 5)).unwrap();

        // PORT NOTE: the reference creates `var s = t.vtStream()` but never writes
        // to it; the Rust stream borrows `&mut Terminal`, so we omit the unused
        // stream entirely (behaviorally identical) to allow `t.rows`/`t.cols`
        // reads below.
        {
            let mut s = t.vt_stream();
            s.deinit();
        }

        let mut state: RenderState = RenderState::empty();
        state.update(alloc, &mut t).unwrap();

        // Row out of bound
        {
            let mut cells = state
                .link_cells(
                    alloc,
                    point::Coordinate {
                        x: 0,
                        y: (t.rows + 10) as u32,
                    },
                )
                .unwrap();
            assert_eq!(0, cells.count());
            let _ = &mut cells;
        }

        // Col out of bound
        {
            let mut cells = state
                .link_cells(
                    alloc,
                    point::Coordinate {
                        x: t.cols + 10,
                        y: 0,
                    },
                )
                .unwrap();
            assert_eq!(0, cells.count());
            let _ = &mut cells;
        }

        state.deinit(alloc);
        t.deinit(alloc);
    }

    /// PORTS: oracle c9ad708ef
    /// `test "flattened highlights require matching page serial"`
    #[test]
    fn flattened_highlights_require_matching_page_serial() {
        let alloc = Global;

        // var t = try Terminal.init(alloc, .{ .cols = 10, .rows = 3 });
        // defer t.deinit(alloc);
        let mut t = Terminal::init(alloc, opts(10, 3)).unwrap();

        // Capture the live generation while terminal-owned state is in scope so
        // we can also verify beginUpdate copies it into the render row.
        // const live_pin = t.screens.active.pages.getTopLeft(.viewport);
        // const live_serial = live_pin.node.serial;
        let live_pin = unsafe { (*t.screens.active).pages.get_top_left(point::Tag::Viewport) };
        let live_serial = unsafe { (*live_pin.node).serial };

        // var state: RenderState = .empty;
        // defer state.deinit(alloc);
        // try state.update(alloc, &t);
        let mut state: RenderState = RenderState::empty();
        state.update(alloc, &mut t).unwrap();

        // const pin: PageList.Pin = pin: {
        //     const row_data = state.row_data.slice();
        //     @memset(row_data.items(.dirty), false);
        //     state.dirty = .false;
        //     break :pin row_data.items(.pin)[0];
        // };
        let pin: Pin = {
            for row in state.row_data.iter_mut() {
                row.dirty = false;
            }
            state.dirty = Dirty::False;
            state.row_data[0].pin
        };
        // const row_serial = state.row_data.items(.serial)[0];
        let row_serial = state.row_data[0].serial;
        // try testing.expectEqual(live_pin.node, pin.node);
        // try testing.expectEqual(live_serial, row_serial);
        assert!(core::ptr::eq(live_pin.node, pin.node));
        assert_eq!(live_serial, row_serial);

        // Use the exact node pointer and row captured by the render state, but a
        // different generation. A reused node address must not make this stale
        // flattened highlight match.
        // var hl: highlight.Flattened = .{ .chunks = .empty, .top_x = 2, .bot_x = 4 };
        // defer hl.deinit(alloc);
        // try hl.chunks.append(alloc, .{
        //     .node = pin.node,
        //     .serial = live_serial ^ 1,
        //     .start = pin.y,
        //     .end = pin.y + 1,
        // });
        let mut hl = highlight::Flattened {
            chunks: highlight::ChunkList::new_in(alloc),
            top_x: 2,
            bot_x: 4,
        };
        hl.chunks.push(highlight::Chunk {
            node: pin.node,
            serial: live_serial ^ 1,
            start: pin.y,
            end: pin.y + 1,
        });

        // try state.updateHighlightsFlattened(alloc, 42, &.{hl});
        state
            .update_highlights_flattened(alloc, 42, core::slice::from_ref(&hl))
            .unwrap();
        {
            // try testing.expectEqual(0, row_data.items(.highlights)[0].items.len);
            // try testing.expect(!row_data.items(.dirty)[0]);
            // try testing.expectEqual(.false, state.dirty);
            assert_eq!(0, state.row_data[0].highlights.len());
            assert!(!state.row_data[0].dirty);
            assert_eq!(Dirty::False, state.dirty);
        }

        // The same chunk is accepted once its copied serial also matches.
        // hl.chunks.items(.serial)[0] = live_serial;
        hl.chunks[0].serial = live_serial;
        state
            .update_highlights_flattened(alloc, 42, core::slice::from_ref(&hl))
            .unwrap();
        {
            // const row_highlights = row_data.items(.highlights)[0].items;
            // try testing.expectEqual(1, row_highlights.len);
            // try testing.expectEqual(42, row_highlights[0].tag);
            // try testing.expectEqual([2]size.CellCountInt{ 2, 4 }, row_highlights[0].range);
            // try testing.expect(row_data.items(.dirty)[0]);
            // try testing.expectEqual(.partial, state.dirty);
            assert_eq!(1, state.row_data[0].highlights.len());
            assert_eq!(42, state.row_data[0].highlights[0].tag);
            assert_eq!([2, 4], state.row_data[0].highlights[0].range);
            assert!(state.row_data[0].dirty);
            assert_eq!(Dirty::Partial, state.dirty);
        }

        hl.deinit(alloc);
        state.deinit(alloc);
        t.deinit(alloc);
    }

    /// PORT NOTE: NOT a reference test. The reference's pin/serial skip test
    /// (`render.zig:599-615`) is a seven-disjunct condition whose seventh term is
    /// `row_serials[y + take - 1] != node_serial`; nothing in either tree
    /// exercised that term, so a port dropping it stayed green. This pins it:
    /// with only the LAST row of a chunk holding a stale serial, the chunk's
    /// pins and serials must still be rewritten.
    #[test]
    fn stale_trailing_serial_forces_the_chunk_rewrite() {
        let alloc = Global;
        let mut t = Terminal::init(alloc, opts(10, 3)).unwrap();

        let mut state: RenderState = RenderState::empty();
        state.update(alloc, &mut t).unwrap();

        // One node covers all three viewport rows, so the chunk is y=0, take=3.
        let node = state.row_data[0].pin.node;
        let live_serial = unsafe { (*node).serial };
        assert!(core::ptr::eq(state.row_data[2].pin.node, node));
        assert_eq!(live_serial, state.row_data[0].serial);
        assert_eq!(live_serial, state.row_data[2].serial);

        // Leave the first row's node/y/serial and the last row's node/y intact —
        // i.e. every disjunct but the seventh is satisfied — and stale only the
        // LAST row's serial.
        state.row_data[2].serial = live_serial ^ 1;

        // Nothing changed, so this update does not redraw and takes the skip
        // test. The stale serial must force the rewrite anyway.
        state.update(alloc, &mut t).unwrap();
        assert_eq!(live_serial, state.row_data[2].serial);

        state.deinit(alloc);
        t.deinit(alloc);
    }

    // test "dirty row resets highlights"
    #[test]
    fn dirty_row_resets_highlights() {
        let alloc = Global;

        let mut t = Terminal::init(alloc, opts(10, 3)).unwrap();

        {
            let mut s = t.vt_stream();
            s.next_slice(b"ABC");
            s.deinit();
        }

        let mut state: RenderState = RenderState::empty();
        state.update(alloc, &mut t).unwrap();

        // Reset dirty state
        state.dirty = Dirty::False;
        {
            for row in state.row_data.iter_mut() {
                row.dirty = false;
            }
        }

        // Manually add a highlight to row 0
        {
            state.row_data[0].highlights.push(Highlight {
                tag: 1,
                range: [0, 2],
            });
        }

        // Verify we have a highlight
        {
            assert_eq!(1, state.row_data[0].highlights.len());
        }

        // Write to row 0 to make it dirty
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b[H"); // Move to home
            s.next_slice(b"X");
            s.deinit();
        }
        state.update(alloc, &mut t).unwrap();

        // Verify the highlight was reset on the dirty row
        {
            assert_eq!(0, state.row_data[0].highlights.len());
        }

        state.deinit(alloc);
        t.deinit(alloc);
    }

    #[allow(unused_imports)]
    use Key as _Key;

    /// Verifies that an incrementally updated render state has identical
    /// contents to a from-scratch rebuild. This is the load-bearing check
    /// for our dirty tracking: if any terminal operation changes row
    /// contents without setting a dirty signal that `update` honors
    /// (terminal dirty, screen dirty, page dirty, row dirty, viewport pin,
    /// or dimensions), the incremental state will contain stale rows and
    /// this comparison will fail.
    fn test_compare_states(incremental: &RenderState, fresh: &RenderState) {
        // Row metadata that is allowed to be stale in an incremental
        // update. Dirty tracking only guarantees that VISUAL changes are
        // flagged (see page.Row.dirty); these fields are non-visual
        // metadata that the terminal may change without dirtying the row
        // (e.g. Screen.cursorResetWrap clears wrap flags without a dirty
        // mark). This staleness predates the chunked update
        // implementation; it is present in the row-iterator implementation
        // as well.
        type StaleOkMask = page::Mask<
            page::Row,
            {
                page::row_field_mask(&[
                    page::RowField::Wrap,
                    page::RowField::WrapContinuation,
                    page::RowField::SemanticPrompt,
                    page::RowField::Dirty,
                ])
            },
            1,
        >;

        assert_eq!(fresh.rows, incremental.rows);
        assert_eq!(fresh.cols, incremental.cols);
        assert_eq!(fresh.cursor.active, incremental.cursor.active);
        assert_eq!(
            fresh
                .cursor
                .viewport
                .as_ref()
                .map(|v| (v.y, v.x, v.wide_tail)),
            incremental
                .cursor
                .viewport
                .as_ref()
                .map(|v| (v.y, v.x, v.wide_tail)),
        );
        assert_eq!(
            fresh.cursor.cell.to_bits(),
            incremental.cursor.cell.to_bits()
        );

        assert_eq!(fresh.row_data.len(), incremental.row_data.len());
        for y in 0..fresh.row_data.len() {
            let inc = &incremental.row_data[y];
            let new = &fresh.row_data[y];

            // Pins must match exactly.
            assert!(
                core::ptr::eq(new.pin.node, inc.pin.node),
                "mismatch on row y={y}: pin node"
            );
            assert_eq!(new.pin.y, inc.pin.y, "mismatch on row y={y}: pin y");

            // Raw row data must match, except for non-visual metadata
            // fields which may legitimately be stale (see StaleOkMask).
            assert_eq!(
                StaleOkMask::strip(new.raw),
                StaleOkMask::strip(inc.raw),
                "mismatch on row y={y}: raw"
            );

            let inc_cells = inc.cells.slice();
            let new_cells = new.cells.slice();
            assert_eq!(
                new_cells.len(),
                inc_cells.len(),
                "mismatch on row y={y}: len"
            );
            let managed = new.raw.managed_memory();
            for x in 0..new_cells.len() {
                // Raw cell contents must match.
                let inc_cell = inc_cells.raw[x];
                let new_cell = new_cells.raw[x];
                assert_eq!(
                    new_cell.to_bits(),
                    inc_cell.to_bits(),
                    "mismatch on row y={y} cell x={x}"
                );

                // The style is only defined if the cell is styled or is
                // a bg-color cell within a row that has managed memory.
                if new_cell.style_id() != 0
                    || (managed
                        && matches!(
                            new_cell.content_tag(),
                            page::ContentTag::BgColorRgb | page::ContentTag::BgColorPalette
                        ))
                {
                    assert!(
                        new_cells.style[x].eql(&inc_cells.style[x]),
                        "mismatch on row y={y} cell x={x}: style"
                    );
                }

                // Graphemes are only defined for grapheme cells.
                if new_cell.content_tag() == page::ContentTag::CodepointGrapheme {
                    assert_eq!(
                        new_cells.grapheme(x),
                        inc_cells.grapheme(x),
                        "mismatch on row y={y} cell x={x}: grapheme"
                    );
                }
            }
        }
    }

    // PORT NOTE: reference `std.Random.DefaultPrng` (xoshiro256++) seeded with a
    // fixed constant. The port uses the same algorithm inline so the sequence is
    // reproducible without pulling a `rand` dependency into `rustty-vt`.
    struct Prng {
        s: [u64; 4],
    }

    impl Prng {
        fn init(seed: u64) -> Prng {
            // SplitMix64 seeding, exactly as xoshiro reference implementations do.
            let mut x = seed;
            let mut next = || {
                x = x.wrapping_add(0x9E3779B97F4A7C15);
                let mut z = x;
                z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
                z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
                z ^ (z >> 31)
            };
            Prng {
                s: [next(), next(), next(), next()],
            }
        }

        fn next_u64(&mut self) -> u64 {
            let r = self.s[0]
                .wrapping_add(self.s[3])
                .rotate_left(23)
                .wrapping_add(self.s[0]);
            let t = self.s[1] << 17;
            self.s[2] ^= self.s[0];
            self.s[3] ^= self.s[1];
            self.s[1] ^= self.s[2];
            self.s[0] ^= self.s[3];
            self.s[2] ^= t;
            self.s[3] = self.s[3].rotate_left(45);
            r
        }

        /// `rand.intRangeAtMost(T, lo, hi)` — inclusive on both ends.
        fn range(&mut self, lo: u64, hi: u64) -> u64 {
            lo + self.next_u64() % (hi - lo + 1)
        }

        fn boolean(&mut self) -> bool {
            self.next_u64() & 1 == 1
        }
    }

    // test "incremental updates match full rebuild"
    #[test]
    fn incremental_updates_match_full_rebuild() {
        let alloc = Global;

        // Deterministic so failures are reproducible.
        let mut rand = Prng::init(0xB0BA_CAFE);

        let mut t = Terminal::init(alloc, opts_sb(20, 8, 500)).unwrap();
        let mut inc: RenderState = RenderState::empty();

        for _ in 0..300 {
            // Perform a random batch of operations between updates.
            {
                let mut s = t.vt_stream();
                for _ in 0..rand.range(1, 6) {
                    match rand.range(0, 18) {
                        // Plain text (possibly wrapping and scrolling).
                        0..=2 => {
                            for _ in 0..rand.range(1, 30) {
                                s.next_slice(&[rand.range(b'A' as u64, b'Z' as u64) as u8]);
                            }
                        }

                        // Newlines to build scrollback and trigger pruning.
                        3 | 4 => {
                            for _ in 0..rand.range(1, 10) {
                                s.next_slice(b"x\r\n");
                            }
                        }

                        // Cursor movement.
                        5 => s.next_slice(
                            format!("\x1b[{};{}H", rand.range(1, 8), rand.range(1, 20)).as_bytes(),
                        ),

                        // Styling: bold, truecolor bg, palette fg, reset.
                        6 => s.next_slice(match rand.range(0, 3) {
                            0 => b"\x1b[1m".as_slice(),
                            1 => b"\x1b[48;2;30;60;90m".as_slice(),
                            2 => b"\x1b[38;5;120m".as_slice(),
                            _ => b"\x1b[0m".as_slice(),
                        }),

                        // Erase ops (EL, ED variants including scrollback).
                        7 => s.next_slice(match rand.range(0, 4) {
                            0 => b"\x1b[K".as_slice(),
                            1 => b"\x1b[1K".as_slice(),
                            2 => b"\x1b[J".as_slice(),
                            3 => b"\x1b[2J".as_slice(),
                            _ => b"\x1b[3J".as_slice(),
                        }),

                        // Insert/delete lines (row rotations within regions).
                        8 => s.next_slice(format!("\x1b[{}L", rand.range(1, 4)).as_bytes()),
                        9 => s.next_slice(format!("\x1b[{}M", rand.range(1, 4)).as_bytes()),

                        // Scroll up/down (page-dirty row rotations).
                        10 => s.next_slice(format!("\x1b[{}S", rand.range(1, 4)).as_bytes()),
                        11 => s.next_slice(format!("\x1b[{}T", rand.range(1, 4)).as_bytes()),

                        // Set/reset scroll regions to exercise bounded scrolls.
                        12 => {
                            let top = rand.range(1, 4);
                            let bot = rand.range(top + 1, 8);
                            s.next_slice(format!("\x1b[{top};{bot}r").as_bytes());
                        }

                        // Insert/delete/erase chars within a row.
                        13 => s.next_slice(format!("\x1b[{}@", rand.range(1, 5)).as_bytes()),
                        14 => s.next_slice(format!("\x1b[{}P", rand.range(1, 5)).as_bytes()),

                        // Reverse index (scroll down at top).
                        15 => s.next_slice(b"\x1bM"),

                        // Wide chars and multi-codepoint graphemes.
                        16 => s.next_slice("字👨‍👩‍👧".as_bytes()),

                        // Alternate screen switching (screen key redraw path).
                        17 => s.next_slice(if rand.boolean() {
                            b"\x1b[?1049h".as_slice()
                        } else {
                            b"\x1b[?1049l".as_slice()
                        }),

                        // DECALN full-screen fill.
                        18 => s.next_slice(b"\x1b#8"),

                        _ => unreachable!(),
                    }
                }
                s.deinit();
            }

            // Occasionally scroll the viewport into scrollback and back.
            match rand.range(0, 9) {
                0 => t.scroll_viewport(ScrollViewport::Delta(-3)),
                1 => t.scroll_viewport(ScrollViewport::Delta(2)),
                2 => t.scroll_viewport(ScrollViewport::Bottom),
                3 => t.scroll_viewport(ScrollViewport::Top),
                _ => {}
            }

            // Update our incremental state first: it must consume the dirty
            // state. The fresh state always fully rebuilds (its dimensions
            // start empty so it always redraws) and so does not depend on
            // any dirty flags.
            inc.update(alloc, &mut t).unwrap();

            let mut fresh: RenderState = RenderState::empty();
            fresh.update(alloc, &mut t).unwrap();

            test_compare_states(&inc, &fresh);
            fresh.deinit(alloc);
        }

        inc.deinit(alloc);
        t.deinit(alloc);
    }

    // test "begin and end update"
    #[test]
    fn begin_and_end_update() {
        let alloc = Global;

        let mut t = Terminal::init(alloc, opts(10, 3)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b[1mAB"); // Bold
            s.next_slice(b"\x1b[0;3mC"); // Italic
            s.deinit();
        }

        let mut state: RenderState = RenderState::empty();
        state.begin_update(alloc, &mut t).unwrap();

        // We should have pending style runs on row 0: one for the bold
        // run and one for the italic run.
        {
            let runs = &state.pending_styles;
            assert_eq!(2, runs.len());
            assert_eq!(0, runs[0].y);
            assert_eq!(0, runs[0].start);
            assert_eq!(2, runs[0].end);
            assert!(runs[0].style.flags.bold);
            assert_eq!(0, runs[1].y);
            assert_eq!(2, runs[1].start);
            assert_eq!(3, runs[1].end);
            assert!(runs[1].style.flags.italic);
        }

        // End our update. This should denormalize the runs into cells
        // and clear the pending runs.
        state.end_update();
        {
            assert_eq!(0, state.pending_styles.len());
            assert!(state.row_data[0].cells.get(0).style.flags.bold);
            assert!(state.row_data[0].cells.get(1).style.flags.bold);
            assert!(state.row_data[0].cells.get(2).style.flags.italic);
        }

        state.deinit(alloc);
        t.deinit(alloc);
    }

    // test "endUpdate skips unchanged style runs"
    //
    // PORT NOTE: the reference keeps ONE `Stream` alive across the updates
    // (`var s = t.vtStream(); defer s.deinit();`); rustty's `vt_stream()`
    // mutably borrows the terminal, so each write takes a scoped stream (the
    // `begin_and_end_update` precedent). The byte sequences and assertions are
    // identical.
    #[test]
    fn end_update_skips_unchanged_style_runs() {
        let alloc = Global;

        let mut t = Terminal::init(alloc, opts(10, 3)).unwrap();

        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b[1mAB"); // Bold
            s.deinit();
        }

        let mut state: RenderState = RenderState::empty();
        state.update(alloc, &mut t).unwrap();

        // The applied cache should record the bold run for row 0.
        {
            let applied = &state.row_data[0].applied_styles;
            assert_eq!(1, applied.len());
            assert!(applied[0].style.flags.bold);
            assert!(state.row_data[0].cells.get(0).style.flags.bold);
        }

        // Rewrite the text without changing the styling: the row is
        // rebuilt, the run matches the cache, and the styles must remain
        // correct (the fill is skipped internally).
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b[1;1H\x1b[1mXY");
            s.deinit();
        }
        state.update(alloc, &mut t).unwrap();
        {
            let cells = &state.row_data[0].cells;
            assert_eq!('X' as u32, cells.get(0).raw.codepoint());
            assert!(cells.get(0).style.flags.bold);
            assert!(cells.get(1).style.flags.bold);
        }

        // Change the styling: the cache mismatches and the new styles
        // must be applied and recorded.
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b[1;1H\x1b[0;3mZW"); // Italic
            s.deinit();
        }
        state.update(alloc, &mut t).unwrap();
        {
            let cells = &state.row_data[0].cells;
            assert_eq!('Z' as u32, cells.get(0).raw.codepoint());
            assert!(!cells.get(0).style.flags.bold);
            assert!(cells.get(0).style.flags.italic);

            let applied = &state.row_data[0].applied_styles;
            assert_eq!(1, applied.len());
            assert!(applied[0].style.flags.italic);
        }

        state.deinit(alloc);
        t.deinit(alloc);
    }

    // test "bg color cells"
    #[test]
    fn bg_color_cells() {
        let alloc = Global;

        let mut t = Terminal::init(alloc, opts(10, 3)).unwrap();

        // Write a styled cell (so the row has managed memory) then erase
        // the rest of the line with a palette background pending. The
        // erase produces bg_color content cells rather than styled cells.
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b[1mA\x1b[48;5;1m\x1b[K");
            s.deinit();
        }

        let mut state: RenderState = RenderState::empty();
        state.update(alloc, &mut t).unwrap();

        {
            let cell = state.row_data[0].cells.get(0);
            assert_eq!('A' as u32, cell.raw.codepoint());
            assert!(cell.style.flags.bold);
        }
        for x in 1..10 {
            let cell = state.row_data[0].cells.get(x);
            assert_eq!(page::ContentTag::BgColorPalette, cell.raw.content_tag());
            assert!(crate::style::Color::Palette(1).eql(cell.style.bg_color));
        }

        state.deinit(alloc);
        t.deinit(alloc);
    }

    // =========================================================================
    // Parity tail S7-5 (S175-F1): the render state's containers grow through
    // the allocator `update`/`beginUpdate` is given. Rust-only tests: the
    // reference has no FailingAllocator test for the render state.
    // =========================================================================

    fn vt_write(t: &mut Terminal, bytes: &[u8]) {
        let mut s = t.vt_stream();
        s.next_slice(bytes);
        s.deinit();
    }

    fn resize_terminal(t: &mut Terminal, cols: size::CellCountInt, rows: size::CellCountInt) {
        t.resize(
            Global,
            crate::terminal::Resize {
                cols,
                rows,
                cell_size_px: None,
            },
        )
        .unwrap();
    }

    /// Whether the reference defines `style` for this cell (the run kinds
    /// `row_builder` records).
    fn style_is_defined(raw: page::Cell) -> bool {
        match raw.content_tag() {
            page::ContentTag::Codepoint | page::ContentTag::CodepointGrapheme => raw.style_id() > 0,
            page::ContentTag::BgColorRgb | page::ContentTag::BgColorPalette => true,
        }
    }

    /// Everything a render state defines for a reader, compared field by
    /// field: dimensions, cursor, and every row's raw data, pin, selection
    /// and cells (the raw column, and the style and grapheme columns where
    /// they are defined). Highlights are the caller's (not compared).
    #[track_caller]
    fn assert_same_render(got: &RenderState, want: &RenderState, what: &str) {
        assert_eq!(want.rows, got.rows, "{what}: rows");
        assert_eq!(want.cols, got.cols, "{what}: cols");
        assert_eq!(
            want.cursor
                .viewport
                .as_ref()
                .map(|v| (v.x, v.y, v.wide_tail)),
            got.cursor
                .viewport
                .as_ref()
                .map(|v| (v.x, v.y, v.wide_tail)),
            "{what}: cursor"
        );
        assert_eq!(want.row_data.len(), got.row_data.len(), "{what}: row count");
        for (y, (g, w)) in got.row_data.iter().zip(want.row_data.iter()).enumerate() {
            assert_eq!(w.raw.cval(), g.raw.cval(), "{what}: row {y} raw");
            assert!(g.pin.eql(w.pin), "{what}: row {y} pin");
            assert_eq!(w.serial, g.serial, "{what}: row {y} serial");
            assert_eq!(w.selection, g.selection, "{what}: row {y} selection");
            assert_eq!(w.cells.len(), g.cells.len(), "{what}: row {y} cell count");
            for x in 0..w.cells.len() {
                let (gc, wc) = (g.cells.get(x), w.cells.get(x));
                assert_eq!(wc.raw.cval(), gc.raw.cval(), "{what}: cell {x},{y} raw");
                if style_is_defined(wc.raw) {
                    assert!(gc.style.eql(&wc.style), "{what}: cell {x},{y} style");
                }
                if wc.raw.has_grapheme() {
                    assert_eq!(wc.grapheme, gc.grapheme, "{what}: cell {x},{y} grapheme");
                }
            }
        }
    }

    /// A fresh render state over the Rust heap: the full rebuild `got` must
    /// match.
    fn fresh_render(t: &mut Terminal) -> RenderState {
        let mut fresh = RenderState::empty();
        fresh.update(Global, t).unwrap();
        fresh
    }

    // Every container of the render state allocates, grows and frees through
    // the update allocator, with the reference's call shape and capacities:
    // the MultiArrayLists (`row_data`, and each of a row's three cell
    // columns) take `growCapacity(n)` elements and grow by alloc + copy + free
    // (never `remap`); the `ArrayList`s (`pending_styles`, `applied_styles`,
    // `highlights`) take `growCapacity` and grow by `remap` first; the
    // grapheme buffer and the highlights keep their capacity across rebuilds
    // (the arena's `.retain_capacity`). `deinit` returns every byte.
    #[test]
    fn render_state_internals_route_through_the_allocator_handle() {
        use crate::alloc::testing::{with_handle, CountingAllocator};

        let row_bytes = core::mem::size_of::<Row>();
        let run_bytes = core::mem::size_of::<StyleRun>();
        // The three cell columns (`raw`, grapheme spans, `style`), each of
        // growCapacity(cols) elements.
        let cells_bytes = |cols: usize| {
            grow_capacity::<CELL_ZIG_FIELD_MAX>(cols)
                * (core::mem::size_of::<page::Cell>()
                    + core::mem::size_of::<GraphemeSpan>()
                    + core::mem::size_of::<Style>())
        };
        // Zig's own element counts: `growCapacity` with init 5 (rows), 4
        // (cells), 3 (style runs), 21 (highlights) on a 128-byte cache line.
        if CACHE_LINE_IS_128 {
            assert_eq!(9, grow_capacity::<ROW_ZIG_FIELD_MAX>(3));
            assert_eq!(19, grow_capacity::<CELL_ZIG_FIELD_MAX>(10));
            assert_eq!(4, grow_capacity::<STYLE_RUN_ZIG_SIZE>(1));
            assert_eq!(22, grow_capacity::<HIGHLIGHT_ZIG_SIZE>(1));
        }

        let counting = CountingAllocator::over_heap();
        let c = counting.allocator();
        let mut t = Terminal::init(Global, opts(10, 3)).unwrap();
        with_handle(&c, |alloc| {
            let mut state = RenderState::empty();
            let calls = || {
                (
                    counting.alloc_calls.get(),
                    counting.free_calls.get(),
                    counting.remap_calls.get(),
                )
            };

            // A: plain text. One `row_data` allocation (growCapacity(3) rows)
            // and, per row, one per cell column (growCapacity(10) cells).
            vt_write(&mut t, b"ABCD");
            state.update(alloc, &mut t).unwrap();
            assert_eq!((10, 0, 0), calls(), "row_data + 3 rows x 3 columns");
            assert_eq!(
                grow_capacity::<ROW_ZIG_FIELD_MAX>(3) * row_bytes + 3 * cells_bytes(10),
                counting.live_bytes.get()
            );
            assert_eq!(
                Some((
                    grow_capacity::<CELL_ZIG_FIELD_MAX>(10) * core::mem::size_of::<Style>(),
                    core::mem::align_of::<Style>().trailing_zeros() as u8
                )),
                counting.last_alloc.get(),
                "the last row's style column"
            );
            assert_eq!(
                grow_capacity::<CELL_ZIG_FIELD_MAX>(10),
                state.row_data[0].cells.capacity()
            );

            // B: a styled row: the first pending run and that row's applied
            // cache (growCapacity(1) runs each).
            let live = counting.live_bytes.get();
            vt_write(&mut t, b"\x1b[2;1H\x1b[1mBOLD\x1b[0m");
            state.update(alloc, &mut t).unwrap();
            assert_eq!((12, 0, 0), calls(), "pending_styles + applied_styles");
            assert_eq!(
                live + 2 * grow_capacity::<STYLE_RUN_ZIG_SIZE>(1) * run_bytes,
                counting.live_bytes.get()
            );
            assert!(state.row_data[1].cells.get(0).style.flags.bold);

            // C: a grapheme: the row's grapheme buffer.
            vt_write(&mut t, "\x1b[3;1H\u{1F468}\u{200D}".as_bytes());
            state.update(alloc, &mut t).unwrap();
            assert_eq!((13, 0, 0), calls(), "the grapheme buffer");
            assert_eq!(&[0x200D], state.row_data[2].cells.grapheme(0));

            // D: a highlight on row 0 (growCapacity(1) highlights).
            let live = counting.live_bytes.get();
            let pin = state.row_data[0].pin;
            // The highlight's own chunks are the caller's (the Rust heap
            // here), not the render state's.
            let mut chunks = highlight::ChunkList::new_in(crate::alloc::Global);
            chunks.push(highlight::Chunk {
                node: pin.node,
                serial: state.row_data[0].serial,
                start: pin.y,
                end: pin.y + 1,
            });
            let hl = highlight::Flattened {
                chunks,
                top_x: 0,
                bot_x: 2,
            };
            state
                .update_highlights_flattened(alloc, 1, core::slice::from_ref(&hl))
                .unwrap();
            assert_eq!((14, 0, 0), calls(), "the highlight list");
            assert_eq!(
                live + grow_capacity::<HIGHLIGHT_ZIG_SIZE>(1) * core::mem::size_of::<Highlight>(),
                counting.live_bytes.get()
            );

            // E: rebuilding rows keeps every capacity (the arena's
            // `.retain_capacity`, the lists' `clearRetainingCapacity`).
            vt_write(
                &mut t,
                "\x1b[1;1HZ\x1b[2;1H\x1b[1mX\x1b[0m\x1b[3;1H\u{1F469}\u{200D}".as_bytes(),
            );
            state.update(alloc, &mut t).unwrap();
            assert_eq!((14, 0, 0), calls(), "a steady state allocates nothing");
            assert!(
                state.row_data[0].highlights.is_empty(),
                "the rebuild reset the highlights"
            );

            // F: growing past the rows' and the cells' capacity: row_data and
            // the three old rows' cell columns move (alloc + copy + free, no
            // remap), the nine new rows get their columns.
            resize_terminal(&mut t, 30, 12);
            state.update(alloc, &mut t).unwrap();
            assert_eq!(
                (14 + 1 + 3 * 3 + 9 * 3, 1 + 3 * 3, 0),
                calls(),
                "MultiArrayList growth"
            );
            assert_eq!(
                grow_capacity::<ROW_ZIG_FIELD_MAX>(12),
                state.row_data.capacity()
            );
            assert_eq!(
                grow_capacity::<CELL_ZIG_FIELD_MAX>(30),
                state.row_data[0].cells.capacity()
            );
            assert_eq!(
                grow_capacity::<CELL_ZIG_FIELD_MAX>(30),
                state.row_data[11].cells.capacity()
            );

            // G: five runs in one row grow both run lists past 4: `remap`.
            let before = calls();
            vt_write(
                &mut t,
                b"\x1b[2;1H\x1b[1mA\x1b[0mB\x1b[1mC\x1b[0mD\x1b[1mE\x1b[0mF\x1b[1mG\x1b[0mH\x1b[1mI\x1b[0m",
            );
            state.update(alloc, &mut t).unwrap();
            assert_eq!(
                (before.0, before.1, before.2 + 2),
                calls(),
                "ArrayList growth remaps"
            );
            assert_eq!(
                grow_capacity::<STYLE_RUN_ZIG_SIZE>(5),
                state.pending_styles.capacity()
            );
            assert_eq!(
                grow_capacity::<STYLE_RUN_ZIG_SIZE>(5),
                state.row_data[1].applied_styles.capacity()
            );

            // H: shrinking frees the dropped rows' lists.
            resize_terminal(&mut t, 30, 4);
            state.update(alloc, &mut t).unwrap();
            assert_same_render(&state, &fresh_render(&mut t), "after the shrink");

            state.deinit(alloc);
        });
        assert_eq!(0, counting.live_bytes.get(), "deinit returns every byte");
        assert_eq!(counting.alloc_calls.get(), counting.free_calls.get());
        t.deinit(Global);
    }

    /// `std.atomic.cache_line` is 128 on the targets the counts above assume.
    const CACHE_LINE_IS_128: bool = cfg!(any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "powerpc64"
    ));

    // The update allocator is bound on first growth; another allocator later
    // is a caller bug the port reports (Zig takes the allocator per call and
    // trusts it to be the same).
    #[test]
    #[cfg_attr(
        not(debug_assertions),
        ignore = "debug_assert_same is a debug-build check (R14)"
    )]
    #[should_panic(expected = "allocator mismatch")]
    fn render_state_checks_the_update_allocator() {
        use crate::alloc::testing::{with_handle, CountingAllocator};

        let counting = CountingAllocator::over_heap();
        let c = counting.allocator();
        let mut t = Terminal::init(Global, opts(10, 3)).unwrap();
        with_handle(&c, |alloc| {
            let mut state = RenderState::empty();
            state.update(alloc, &mut t).unwrap();
            // The expected panic is caught so the state and the terminal are
            // still torn down (Miri reports the leak otherwise), then re-raised
            // for `should_panic` (S7-9; the pattern of 16ef8680).
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = state.update(Global, &mut t);
            }));
            state.deinit(alloc);
            t.deinit(Global);
            if let Err(p) = r {
                std::panic::resume_unwind(p);
            }
        });
    }

    /// The sweep scenario: styled text, graphemes, a background-color erase,
    /// both update forms, a highlight, growth past every capacity, and a
    /// shrink. After each call: a success must match a fresh full rebuild;
    /// a failure is lifted (`fail_index` moved past it, so the first failure
    /// is the only one) and the retried update must match it too (R13,
    /// UD315). Returns how many calls failed.
    fn render_sweep_scenario(
        failing: &crate::alloc::testing::FailingAllocator,
        alloc: Allocator,
        what: &str,
    ) -> usize {
        let mut t = Terminal::init(Global, opts(10, 3)).unwrap();
        let mut state = RenderState::empty();
        let failures = core::cell::Cell::new(0usize);
        let check =
            |state: &mut RenderState, t: &mut Terminal, r: Result<(), OutOfMemory>, step: &str| {
                let retried = r.is_err();
                if retried {
                    failures.set(failures.get() + 1);
                    failing.fail_index.set(usize::MAX);
                    state.update(alloc, t).unwrap();
                }
                assert_same_render(state, &fresh_render(t), &format!("{what} {step}"));
                if retried {
                    assert_eq!(
                        Dirty::Full,
                        state.dirty,
                        "{what} {step}: the retry is a full rebuild"
                    );
                }
            };

        vt_write(&mut t, b"\x1b[1mbold\x1b[0m plain\r\n");
        let r = state.update(alloc, &mut t);
        check(&mut state, &mut t, r, "first update");

        let lines = if cfg!(miri) { 2 } else { 8 };
        for i in 0..lines {
            vt_write(
                &mut t,
                format!("\x1b[3{}m{i}\x1b[0m e\u{301} \u{1F468}\u{200D}\u{1F469}\x1b[4{}m\x1b[K\x1b[0m\r\n", i % 8, (i + 1) % 8)
                    .as_bytes(),
            );
        }
        let r = state.begin_update(alloc, &mut t);
        state.end_update();
        check(&mut state, &mut t, r, "two-phase update");

        let pin = state.row_data[0].pin;
        // The highlight's own chunks are the caller's (the Rust heap here).
        let mut chunks = highlight::ChunkList::new_in(crate::alloc::Global);
        chunks.push(highlight::Chunk {
            node: pin.node,
            serial: state.row_data[0].serial,
            start: pin.y,
            end: pin.y + 1,
        });
        let hl = highlight::Flattened {
            chunks,
            top_x: 0,
            bot_x: 3,
        };
        if state
            .update_highlights_flattened(alloc, 7, core::slice::from_ref(&hl))
            .is_err()
        {
            failures.set(failures.get() + 1);
            failing.fail_index.set(usize::MAX);
        }

        resize_terminal(&mut t, 40, 14);
        vt_write(
            &mut t,
            b"\x1b[1mA\x1b[0mB\x1b[1mC\x1b[0mD\x1b[1mE\x1b[0mF\x1b[1mG\x1b[0mH\x1b[1mI\x1b[0m\r\n",
        );
        let r = state.update(alloc, &mut t);
        check(&mut state, &mut t, r, "growth");

        resize_terminal(&mut t, 20, 4);
        let r = state.update(alloc, &mut t);
        check(&mut state, &mut t, r, "shrink");

        state.deinit(alloc);
        t.deinit(Global);
        failures.get()
    }

    // T2 (parity tail S7-5; the Zig `checkAllAllocationFailures` idea): for
    // every n the n-th allocation of the scenario is refused. Nothing aborts,
    // each refused call reports `error.OutOfMemory`, the retry rebuilds a
    // state identical to a fresh one, and `deinit` returns every byte. The
    // sweep ends at the first n that induces no failure, so every allocation
    // is failed once. A second sweep refuses `remap`/`resize` from their n-th
    // call on: growth then takes the alloc + copy + free fallback, and nothing
    // fails.
    #[test]
    fn render_state_allocation_failure_sweep() {
        use crate::alloc::testing::{with_handle, Config, FailingAllocator};

        let mut n = 0usize;
        let mut failed_calls = 0usize;
        loop {
            let failing = FailingAllocator::over_heap(Config {
                fail_index: n,
                ..Config::default()
            });
            let c = failing.allocator();
            let failures = with_handle(&c, |alloc| {
                render_sweep_scenario(&failing, alloc, &format!("alloc n={n}"))
            });
            assert_eq!(
                failing.allocated_bytes.get(),
                failing.freed_bytes.get(),
                "alloc n={n}: every byte is returned"
            );
            assert_eq!(failing.allocations.get(), failing.deallocations.get());
            if !failing.has_induced_failure.get() {
                assert_eq!(0, failures);
                break;
            }
            assert_eq!(1, failures, "alloc n={n}: exactly the refused call fails");
            failed_calls += failures;
            n += 1;
            assert!(n < 10_000, "the sweep must terminate");
        }
        let min_swept = if cfg!(miri) { 10 } else { 25 };
        assert!(n > min_swept, "only {n} allocations were swept");
        assert_eq!(n, failed_calls);

        let clean = FailingAllocator::over_heap(Config::default());
        let c = clean.allocator();
        assert_eq!(
            0,
            with_handle(&c, |alloc| render_sweep_scenario(&clean, alloc, "clean"))
        );
        let remaps = clean.resize_index.get();
        assert!(remaps > 0, "the scenario makes no remap call");
        for n in 0..=remaps {
            let failing = FailingAllocator::over_heap(Config {
                resize_fail_index: n,
                ..Config::default()
            });
            let c = failing.allocator();
            let failures = with_handle(&c, |alloc| {
                render_sweep_scenario(&failing, alloc, &format!("remap n={n}"))
            });
            assert_eq!(0, failures, "remap n={n}: a refused remap falls back");
            assert!(failing.resize_index.get() <= n, "remap n={n}");
            assert_eq!(
                failing.allocated_bytes.get(),
                failing.freed_bytes.get(),
                "remap n={n}"
            );
        }
    }

    // R13 (UD315): a `beginUpdate` that fails part way leaves rows it did not
    // rebuild, and has already cleared their dirty flags. The next update must
    // still produce the full, current viewport.
    #[test]
    fn failed_update_is_followed_by_a_full_rebuild() {
        use crate::alloc::testing::{with_handle, Config, FailingAllocator};

        let mut t = Terminal::init(Global, opts(10, 3)).unwrap();
        vt_write(&mut t, b"one\r\ntwo\r\nthree");
        // Allocation 0 is `row_data`, 1-3 are row 0's cell columns: refuse
        // row 1's first column.
        let failing = FailingAllocator::over_heap(Config {
            fail_index: 4,
            ..Config::default()
        });
        let c = failing.allocator();
        with_handle(&c, |alloc| {
            let mut state = RenderState::empty();
            assert!(state.update(alloc, &mut t).is_err());
            assert!(failing.has_induced_failure.get());
            assert_eq!(0, state.row_data[1].cells.len(), "row 1 was not built");

            failing.fail_index.set(usize::MAX);
            state.update(alloc, &mut t).unwrap();
            assert_eq!(10, state.row_data[1].cells.len(), "row 1 is rebuilt");
            assert_eq!('t' as u32, state.row_data[1].cells.get(0).raw.codepoint());
            assert_eq!('t' as u32, state.row_data[2].cells.get(0).raw.codepoint());
            assert_same_render(&state, &fresh_render(&mut t), "the retry");
            assert_eq!(Dirty::Full, state.dirty);

            // Only the failure forces the rebuild: a clean update after it
            // is incremental again.
            state.dirty = Dirty::False;
            state.update(alloc, &mut t).unwrap();
            assert_eq!(Dirty::False, state.dirty);
            state.deinit(alloc);
        });
        assert_eq!(failing.allocated_bytes.get(), failing.freed_bytes.get());
        t.deinit(Global);
    }
}
