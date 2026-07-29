//! rustty-vt render
//!
//! PORTS: ghostty src/terminal/render.zig
//!
//! PORT STATUS: complete (vt-13). Every method + all 15 `test{}` blocks ported.
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
// row's cells are stored as parallel columns in `CellList` (`raw:
// Vec<page::Cell>`, `style: Vec<Style>`, and grapheme codepoints in a single
// per-row reusable `grapheme_buf: Vec<u32>` addressed by a `(offset, len)`
// span column). `grapheme_buf.clear()` (retaining capacity) is the direct
// analogue of the reference per-row `arena.reset(.retain_capacity)`
// (render.zig:469-471). An earlier port draft stored cells AoS (`Vec<Cell>`
// with a per-cell owned `Vec<u32>` grapheme); that lost the raw-column memcpy
// rebuild whose value the reference's own comment quantifies at ~300% on the
// `screen-clone` benchmark (render.zig:494-499) and paid per-cell push +
// `Style::default()`/`Vec::new()` stores + drop glue. The reference's
// `.slice().items(.X)` projections become the `CellSlice` column accessors.

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
use allocator_api2::alloc::Global as Allocator;

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
    pub row_data: Vec<Row>,

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
// the module note. Per-cell grapheme dupes and the row highlights are owned
// `Vec`s instead, freed by Rust ownership on clear/drop. The reference field's
// documentation is preserved verbatim here:
//
//     /// Arena used for any heap allocations for cell contents
//     /// in this row. Importantly, this is NOT used for the MultiArrayList
//     /// itself. We do this on purpose so that we can easily clear rows,
//     /// but retain cached MultiArrayList capacities since grid sizes don't
//     /// change often.
//     arena: ArenaAllocator.State,
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
    pub highlights: Vec<Highlight>,
}

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
// PORT NOTE (reshape): see the module note. The columns are kept exactly `cols`
// long; `raw` is filled by a single `extend_from_slice` memcpy on rebuild (the
// reference `fastmem.copy(..., cells_slice.items(.raw), page_cells)`), while
// `style`/`grapheme` are written ONLY for flagged cells in the managed-row loop
// — the reference's "undefined unless flagged" discipline (render.zig:491-505).
// `grapheme_buf` is a reusable per-row arena analogue: cleared (retaining
// capacity) each rebuild.
pub struct CellList {
    raw: Vec<page::Cell>,
    style: Vec<Style>,
    grapheme_spans: Vec<GraphemeSpan>,
    grapheme_buf: Vec<u32>,
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
    // undefined; see `rebuild_raw`) and may dangle past the reset
    // `grapheme_buf`. Flagged cells are always written by `set_grapheme` in
    // the same rebuild, so their spans are always in bounds.
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
        CellList {
            raw: Vec::new(),
            style: Vec::new(),
            grapheme_spans: Vec::new(),
            grapheme_buf: Vec::new(),
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

    // PORT NOTE (reshape): `_ = arena.reset(.retain_capacity)` +
    // `clearRetainingCapacity()` — drops the logical length of every column and
    // the grapheme arena while retaining backing capacity.
    //
    // PERF(port): only the `raw` column and the grapheme arena are logically
    // cleared. The `style`/`grapheme_spans` columns keep BOTH capacity and
    // length: the reference `clearRetainingCapacity` + `resize(alloc, cols)`
    // pair leaves those columns' contents undefined (only `raw` is then
    // memcpy'd, render.zig:469-505), so re-zeroing them here was pure extra
    // write traffic — measured ~38% of `screen-clone.render` (7.1ms of 18.8ms
    // per 1000 updates, non-official harness). Stale slots are exactly the
    // reference's "undefined unless flagged" contract, but remain
    // memory-safe initialized values in Rust.
    fn clear_retaining_capacity(&mut self) {
        self.raw.clear();
        self.grapheme_buf.clear();
    }

    // PORT NOTE (reshape): `try cells.resize(alloc, self.cols)` followed by
    // `fastmem.copy(page.Cell, cells_slice.items(.raw), page_cells)`. The raw
    // column is filled by a single `extend_from_slice` memcpy; the `style` and
    // `grapheme_spans` columns are (re)sized ONLY when the column count
    // changes (the reference `resize` moment) — their contents stay
    // stale-but-unread for unflagged cells exactly like the reference's
    // undefined columns. New slots take `Style::default()` / the empty
    // `(0, 0)` span so first reads are defined.
    fn rebuild_raw(&mut self, page_cells: &[page::Cell]) {
        self.raw.clear();
        self.raw.extend_from_slice(page_cells);
        let n = self.raw.len();
        // PERF(port): unlikely branch — column count changes only on resize.
        if self.style.len() != n {
            self.style.clear();
            self.style.resize(n, Style::default());
            self.grapheme_spans.clear();
            self.grapheme_spans.resize(n, (0, 0));
        }
        self.grapheme_buf.clear();
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
    fn slice_mut(&mut self) -> (&mut [Style], &mut [GraphemeSpan], &mut Vec<u32>) {
        (
            &mut self.style,
            &mut self.grapheme_spans,
            &mut self.grapheme_buf,
        )
    }
}

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
            row_data: Vec::new(),
            dirty: Dirty::False,
            screen: crate::screen_set::Key::Primary,
            viewport_pin: None,
            selection_cache: None,
        }
    }

    // PORT NOTE: the reference `deinit` promotes each row's arena and frees it
    // along with the cells MultiArrayList, then frees `row_data`. The Rust
    // `Vec<Row>` (and the `Vec`s each `Row` owns) free on drop, so `deinit`
    // simply clears the storage for API parity.
    pub fn deinit(&mut self, _alloc: Allocator) {
        self.row_data = Vec::new();
    }

    /// Update the render state to the latest terminal state.
    ///
    /// This will reset the terminal dirty state since it is consumed
    /// by this render state update.
    pub fn update(&mut self, alloc: Allocator, t: &mut Terminal) -> Result<(), OutOfMemory> {
        let s: *mut Screen = t.screens.active;
        let viewport_pin = unsafe { (*s).pages.get_top_left(point::Tag::Viewport) };
        let redraw: bool = 'redraw: {
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
        self.colors.palette = t.colors.palette.current;
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
                // Initialize all our values. Its faster to use slice() + set()
                // because appendAssumeCapacity does this multiple times.
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
                        cells: CellList::empty(),
                        dirty: true,
                        selection: None,
                        highlights: Vec::new(),
                    });
                }
            } else {
                // for the rows beyond self.rows, free arena + cells (Vec drop)
                // and shrink retaining capacity.
                self.row_data.truncate(self.rows as usize);
            }
        }

        // Break down our row data
        // PORT NOTE: the reference splits `row_data` into per-field slices; the
        // Rust port indexes `self.row_data[y]` directly (the `Vec<Row>` layout).

        // Track the last page that we know was dirty. This lets us
        // more quickly do the full-page dirty check.
        let mut last_dirty_page: *mut page::Page = core::ptr::null_mut();

        // Go through and setup our rows.
        let mut row_it = unsafe {
            (*s).pages.row_iterator(
                Direction::RightDown,
                point::Point::Viewport(point::Coordinate { x: 0, y: 0 }),
                None,
            )
        };
        let mut y: size::CellCountInt = 0;
        let mut any_dirty: bool = false;
        'rows: while let Some(row_pin) = row_it.next() {
            // Find our cursor if we haven't found it yet. We do this even
            // if the row is not dirty because the cursor is unrelated.
            if self.cursor.viewport.is_none()
                && core::ptr::eq(row_pin.node, unsafe { (*(*s).cursor.page_pin).node })
                && row_pin.y == unsafe { (*(*s).cursor.page_pin).y }
            {
                self.cursor.viewport = Some(Viewport {
                    y,
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

            // Store our pin and its node generation. We have to store these
            // even if we're not dirty because dirty is only a renderer
            // optimization. It doesn't apply to memory movement. This will let
            // us remap any cell pins back to an exact entry in our RenderState
            // and validate them later without dereferencing a potentially
            // stale node.
            //
            // INSPECT, NOT RESTORE: `node.serial` lives in the pagelist Node,
            // never in the page mapping, so reading it must not restore a
            // compressed page. (The `Node::page` call below is the RESTORE this
            // path legitimately needs: `update` copies the row's cells out.)
            self.row_data[y as usize].pin = row_pin;
            // serial.* = node_serial;
            self.row_data[y as usize].serial = unsafe { (*row_pin.node).serial };

            // Get all our cells in the page.
            let p: *mut page::Page = unsafe { crate::pagelist::Node::page(row_pin.node) };
            let page_rac = row_pin.row_and_cell();

            'dirty: {
                // If we're redrawing then we're definitely dirty.
                if redraw {
                    break 'dirty;
                }

                // If our page is the same as last time then its dirty.
                if core::ptr::eq(p, last_dirty_page) {
                    break 'dirty;
                }
                if unsafe { (*p).dirty } {
                    // If this page is dirty then clear the dirty flag
                    // of the last page and then store this one. This benchmarks
                    // faster than iterating pages again later.
                    if !last_dirty_page.is_null() {
                        unsafe { (*last_dirty_page).dirty = false };
                    }
                    last_dirty_page = p;
                    break 'dirty;
                }

                // If our row is dirty then we're dirty.
                if unsafe { (*page_rac.row).dirty() } {
                    break 'dirty;
                }

                // Not dirty!
                y += 1;
                continue 'rows;
            }

            // Set that at least one row was dirty.
            any_dirty = true;

            // Clear our row dirty, we'll clear our page dirty later.
            // We can't clear it now because we have more rows to go through.
            unsafe { (*page_rac.row).set_dirty(false) };

            // Promote our arena. State is copied by value so we need to
            // restore it on all exit paths so we don't leak memory.
            // (PORT NOTE: no arena in the Rust port — owned `Vec`s instead.)

            // PERF(port): hoist the row projection once per row — the
            // reference hoists the `row_data.slice()` column pointers before
            // the loop (render.zig:383-390); repeated `self.row_data[y]`
            // indexing paid a bounds check per touch (and per cell in the
            // managed loop below).
            let row: &mut Row = &mut self.row_data[y as usize];

            // Reset our cells if we're rebuilding this row.
            if !row.cells.is_empty() {
                // _ = arena.reset(.retain_capacity);
                // clearRetainingCapacity()
                row.cells.clear_retaining_capacity();
                row.selection = None;
                row.highlights = Vec::new();
            }
            row.dirty = true;

            // Get all our cells in the page.
            let page_cells: &[page::Cell] = unsafe { &*(*p).get_cells(page_rac.row) };
            assert!(page_cells.len() == self.cols as usize);

            // Copy our raw row data
            row.raw = unsafe { *page_rac.row };

            // Note: our cells MultiArrayList uses our general allocator.
            // We do this on purpose because as rows become dirty, we do
            // not want to reallocate space for cells (which are large). This
            // was a source of huge slowdown.
            //
            // Our per-row arena is only used for temporary allocations
            // pertaining to cells directly (e.g. graphemes, hyperlinks).
            // try cells.resize(alloc, self.cols);
            // We always copy our raw cell data. In the case we have no
            // managed memory, we can skip setting any other fields.
            //
            // This is an important optimization. For plain-text screens
            // this ends up being something around 300% faster based on
            // the `screen-clone` benchmark.
            // fastmem.copy(page.Cell, cells_slice.items(.raw), page_cells);
            //
            // PERF(port): restore the reference's SoA raw-column memcpy
            // (render.zig:492-505). `rebuild_raw` fills the whole `raw` column
            // with one `extend_from_slice` and leaves `style`/`grapheme`
            // untouched (written below only for flagged cells), instead of the
            // prior per-cell `push(Cell { .. })` with a capacity check +
            // `Style::default()`/`Vec::new()` stores + drop glue per cell.
            row.cells.rebuild_raw(page_cells);
            if !unsafe { (*page_rac.row).managed_memory() } {
                y += 1;
                continue;
            }

            // const arena_alloc = arena.allocator();
            // const cells_grapheme = cells_slice.items(.grapheme);
            // const cells_style = cells_slice.items(.style);
            //
            // PERF(port): hoist the mutable column borrows once per row (the
            // reference's hoisted column pointers above, render.zig:508-510)
            // instead of calling `&mut self` helpers per cell — see
            // `CellList::slice_mut`. The `[..n]` re-slices cannot panic:
            // `rebuild_raw` (called above on the same `page_cells`) enforces
            // `style.len() == grapheme_spans.len() == raw.len() ==
            // page_cells.len()`, letting the compiler elide the per-write
            // bounds checks against the loop bound.
            let n = page_cells.len();
            let (cells_style, cells_spans, grapheme_buf) = row.cells.slice_mut();
            let cells_style = &mut cells_style[..n];
            let cells_spans = &mut cells_spans[..n];
            // PERF(port): hoist the style-set base resolution once per row —
            // `(*p).memory_ptr()` and the set's items offset are loop-invariant
            // (nothing in this loop writes through `p`), but the grapheme arm's
            // `extend_from_slice` call clobbers memory, so LLVM re-loads both
            // per styled cell unless they live in pre-loop locals. The zip
            // iteration replaces indexed column writes (`cells_style[x]`),
            // folding the per-write bounds checks into the shared loop bound.
            // `get(base, id)` == `get_at(items_ptr(base), id)` — see
            // `RefCountedSet::items_ptr` / `get_at`.
            let styles: *const crate::style::Set = unsafe { core::ptr::addr_of!((*p).styles) };
            let style_items = unsafe { (*styles).items_ptr((*p).memory_ptr()) };
            for ((page_cell, style_slot), span_slot) in page_cells
                .iter()
                .zip(cells_style.iter_mut())
                .zip(cells_spans.iter_mut())
            {
                // Append assuming its a single-codepoint, styled cell
                // (most common by far).
                if page_cell.style_id() > 0 {
                    // cells_style[x] = p.styles.get(p.memory, page_cell.style_id).*;
                    let style = unsafe { *(*styles).get_at(style_items, page_cell.style_id()) };
                    *style_slot = style;
                }

                // Switch on our content tag to handle less likely cases.
                match page_cell.content_tag() {
                    page::ContentTag::Codepoint => {
                        // @branchHint(.likely);
                        // Primary codepoint goes into `raw` field.
                    }

                    // If we have a multi-codepoint grapheme, look it up and
                    // set our content type.
                    page::ContentTag::CodepointGrapheme => {
                        // @branchHint(.unlikely);
                        // cells_grapheme[x] = try arena_alloc.dupe(u21, ... orelse &.{});
                        //
                        // PORT NOTE (reshape): the arena `dupe` — append the
                        // grapheme codepoints into the row's reusable
                        // `grapheme_buf` and record their span.
                        let g: &[u32] = unsafe {
                            match (*p).lookup_grapheme(page_cell as *const page::Cell) {
                                Some(s) => s,
                                None => &[],
                            }
                        };
                        let off = grapheme_buf.len() as u32;
                        grapheme_buf.extend_from_slice(g);
                        *span_slot = (off, g.len() as u32);
                    }

                    page::ContentTag::BgColorRgb => {
                        // @branchHint(.unlikely);
                        // cells_style[x] = .{ .bg_color = .{ .rgb = ... } };
                        let rgb = page_cell.content_color_rgb();
                        *style_slot = Style {
                            bg_color: crate::style::Color::Rgb(color::RGB {
                                r: rgb.r,
                                g: rgb.g,
                                b: rgb.b,
                            }),
                            ..Style::default()
                        };
                    }

                    page::ContentTag::BgColorPalette => {
                        // @branchHint(.unlikely);
                        // cells_style[x] = .{ .bg_color = .{ .palette = ... } };
                        *style_slot = Style {
                            bg_color: crate::style::Color::Palette(
                                page_cell.content_color_palette(),
                            ),
                            ..Style::default()
                        };
                    }
                }
            }

            y += 1;
        }
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

            // Note: we don't clear any row_data here because our rebuild
            // above did this.
        } else if any_dirty && self.dirty == Dirty::False {
            self.dirty = Dirty::Partial;
        }

        // Finalize our final dirty page
        if !last_dirty_page.is_null() {
            unsafe { (*last_dirty_page).dirty = false };
        }

        // Clear our dirty flags
        t.flags.dirty = crate::terminal::Dirty::default();
        unsafe { (*s).dirty = crate::screen::Dirty::default() };

        let _ = alloc;
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
                    row.highlights.push(Highlight {
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
                    });

                    row.dirty = true;
                    any_dirty = true;
                }
            }
        }

        // Mark our dirty state.
        if any_dirty && self.dirty == Dirty::False {
            self.dirty = Dirty::Partial;
        }

        let _ = alloc;
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

                let other_page: *mut page::Page =
                    unsafe { crate::pagelist::Node::page(pin.node) };
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

/// `writer.print("{u}", .{cp})`: encode a codepoint as UTF-8. We mirror Zig's
/// `{u}` which encodes the surrogate range losslessly via WTF-8-ish encoding;
/// for the cell codepoints we see (validated terminal content) this is the
/// standard UTF-8 encoding.
fn write_codepoint(writer: &mut Vec<u8>, cp: u32) -> Result<(), ()> {
    match char::from_u32(cp) {
        Some(c) => {
            let mut buf = [0u8; 4];
            writer.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            Ok(())
        }
        None => {
            // Surrogate or out-of-range. Encode the low bits as a 3-byte
            // sequence to mirror Zig's `{u}` permissive behavior for the
            // surrogate range; truly out-of-range was already rejected by
            // `utf8_codepoint_sequence_length`.
            let len = utf8_codepoint_sequence_length(cp).ok_or(())?;
            match len {
                3 => {
                    writer.push(0xE0 | ((cp >> 12) & 0x0F) as u8);
                    writer.push(0x80 | ((cp >> 6) & 0x3F) as u8);
                    writer.push(0x80 | (cp & 0x3F) as u8);
                    Ok(())
                }
                _ => Err(()),
            }
        }
    }
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
    use crate::kitty;
    use crate::modes as modespkg;
    use crate::screen_set::Key;
    use crate::terminal::{Colors, Options, ScrollViewport};
    use allocator_api2::alloc::Global;

    // PORT NOTE: the reference `Terminal.init(alloc, .{ .cols, .rows })` relies on
    // `Options` field defaults (`max_scrollback = 10_000`, `colors = .default`,
    // `default_modes = .{}`, kitty limits). Rust `Options` has no field defaults,
    // so this helper supplies them verbatim, mirroring the reference declaration.
    fn opts(cols: size::CellCountInt, rows: size::CellCountInt) -> Options {
        Options {
            cols,
            rows,
            max_scrollback: 10_000,
            colors: Colors::default(),
            default_modes: modespkg::ModePacked::default(),
            kitty_image_storage_limit: 320 * 1000 * 1000,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        }
    }

    fn opts_sb(
        cols: size::CellCountInt,
        rows: size::CellCountInt,
        max_scrollback: usize,
    ) -> Options {
        Options {
            max_scrollback,
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
        miri,
        ignore = "MIRI EXCEPTION (vt multi-page-PageList intractable, .port/phase-2/MIRI_EXCEPTIONS.md): builds/reflows multi-page PageLists that exceed Miri's interpreter budget (>20min); covered natively + by ASan (all PageList tests ~18s)"
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
            chunks: vec![highlight::Chunk {
                node: pin.node,
                serial: live_serial ^ 1,
                start: pin.y,
                end: pin.y + 1,
            }],
            top_x: 2,
            bot_x: 4,
        };

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

        hl.deinit();
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
}
