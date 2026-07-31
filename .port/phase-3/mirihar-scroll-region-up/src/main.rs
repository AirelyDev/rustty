//! `P3-W5` Task 3 Steps 3 and 4(b) — Miri proof for
//! `Screen::cursor_scroll_region_up`'s fast in-page path (oracle `77190bd02`).
//!
//! Isolated, **non-FFI** reproduction of the borrow structure of
//! `crates/rustty-vt/src/screen.rs` `cursor_scroll_region_up_inner`: one heap
//! block per page holding `[Row; size_rows][Cell; size_rows * cols]`, with the
//! row base and every cell run materialized as RAW pointers off that base, a
//! `Screen` whose cursor caches a `*mut Row` / `*mut Cell` INTO that block, and
//! tracked pins held as raw pointers. That is the shape area A §5(a) requires
//! be preserved, and the shape the shipped code has.
//!
//! Three candidates, selected by **argv** (never an environment variable: Miri
//! does not forward host env, which is what made an earlier env-switched
//! control vacuous — see `../../phase-2/mirihar-chunk3b/README.md`):
//!
//!   * `raw_ok`      — the SHIPPED shape. Raw pointers throughout; the only
//!                     `&mut` is the `&mut [Row]` handed to `rotate_once`,
//!                     created from the raw base and dropped immediately; the
//!                     cursor's new row/cell pointers are recomputed from the
//!                     RAW base, not from that slice.
//!   * `mutpage_ub`  — the plan's forbidden mutation: `&mut Page` formed from
//!                     the node and held across `clear_cells`, which re-derives
//!                     a `*mut Page` from the SAME node. This is the "reintroduce
//!                     `&mut Page`" negative control (Step 4(b)).
//!   * `rowslice_ub` — the second forbidden mutation: the cursor's new
//!                     `page_row` is taken FROM the `&mut [Row]` that
//!                     `rotate_once` borrowed, then used after the page is
//!                     written again through the raw base. This is why the
//!                     shipped code recomputes from the raw base.
//!
//! `raw_ok` and `rowslice_ub` run the IDENTICAL follow-on sequence (a
//! `clear_dirty`-shaped raw row write, an integrity re-walk, then a write
//! through the cached cursor row). They differ in exactly one thing: where
//! `cursor_row` came from. Without that, the contrast would be confounded.
//!
//! A run that is green for all three would be no evidence at all; the two `_ub`
//! rows are the whole point.
//!
//! ## VACUITY FOUND AND FIXED while building this harness (`P3-W5 T3`)
//!
//! The first `rowslice_ub` popped the slice-derived tag with a *raw read* (the
//! integrity walk) and with a raw *write to a different row*. Both were
//! **CLEAN** under Stacked Borrows — SB stacks are per-location, and a read
//! through the parent raw tag did not remove the `SharedReadWrite` item the
//! `&mut slice[limit]` coercion had pushed. The control only became real once
//! the follow-on wrote *that same row* through the page base. Recorded here
//! because "Miri was green" on the first two shapes proved nothing at all.
//!
//! Run from this directory:
//!
//! ```sh
//! for c in raw_ok mutpage_ub rowslice_ub; do
//!   echo "== $c =="
//!   cargo +nightly miri run --quiet -- "$c"
//!   MIRIFLAGS=-Zmiri-tree-borrows cargo +nightly miri run -q -- "$c"
//! done
//! ```

use std::alloc::{alloc_zeroed, dealloc, Layout};

const COLS: usize = 5;
const SIZE_ROWS: usize = 8;

/// Mirrors `page::Row`: a packed backing integer. Only `dirty` matters here.
#[derive(Clone, Copy)]
#[repr(transparent)]
struct Row(u64);

impl Row {
    fn managed_memory(self) -> bool {
        self.0 & 1 != 0
    }
}

/// Mirrors `page::Cell`: `#[repr(transparent)] struct Cell(u64)`.
#[derive(Clone, Copy)]
#[repr(transparent)]
struct Cell(u64);

/// Mirrors `page::Page`: a single owned memory block plus offsets. `rows` and
/// every cell run are OFFSETS resolved against `memory_ptr()`, exactly as the
/// port's `Offset<T>::ptr(base)` does.
struct Page {
    base: *mut u8,
    layout: Layout,
    size_rows: usize,
    dirty: bool,
}

impl Page {
    fn new() -> Page {
        let rows_bytes = SIZE_ROWS * std::mem::size_of::<Row>();
        let cells_bytes = SIZE_ROWS * COLS * std::mem::size_of::<Cell>();
        let layout = Layout::from_size_align(rows_bytes + cells_bytes, 8).unwrap();
        let base = unsafe { alloc_zeroed(layout) };
        assert!(!base.is_null());
        let p = Page {
            base,
            layout,
            size_rows: SIZE_ROWS,
            dirty: false,
        };
        // Seed distinguishable content so a mis-rotate is observable.
        for y in 0..SIZE_ROWS {
            let cells = unsafe { p.cells_at(y) };
            for x in 0..COLS {
                unsafe { *cells.add(x) = Cell(((y + 1) * 100 + x) as u64) };
            }
        }
        p
    }

    fn memory_ptr(&self) -> *mut u8 {
        self.base
    }

    /// `page.rows.ptr(page.memory.ptr)`
    fn rows_ptr(&self) -> *mut Row {
        self.memory_ptr() as *mut Row
    }

    /// `page.getCells(row)`, by row index.
    ///
    /// # Safety
    /// `y` must be < `size_rows`.
    unsafe fn cells_at(&self, y: usize) -> *mut Cell {
        let cells_base = self.base.add(SIZE_ROWS * std::mem::size_of::<Row>()) as *mut Cell;
        cells_base.add(y * COLS)
    }

    /// `page.getCells(row)` given a row POINTER, as the shipped code calls it.
    ///
    /// # Safety
    /// `row` must point into this page's row array.
    unsafe fn get_cells(&self, row: *const Row) -> *mut Cell {
        let y = (row as usize - self.rows_ptr() as usize) / std::mem::size_of::<Row>();
        self.cells_at(y)
    }
}

impl Drop for Page {
    fn drop(&mut self) {
        unsafe { dealloc(self.base, self.layout) };
    }
}

/// Mirrors `pagelist::Node`; `Node::page(node) -> *mut Page` is raw, as in the
/// port.
struct Node {
    page: Page,
}

impl Node {
    /// # Safety
    /// `node` must be a live `Node`.
    unsafe fn page(node: *mut Node) -> *mut Page {
        std::ptr::addr_of_mut!((*node).page)
    }
}

/// Mirrors `pagelist::Pin`.
struct Pin {
    node: *mut Node,
    x: usize,
    y: usize,
}

/// Mirrors `Screen` + its `cursor`, keeping only the fields the fast path
/// touches.
struct Screen {
    node: *mut Node,
    tracked_pins: Vec<*mut Pin>,
    cursor_pin: *mut Pin,
    cursor_x: usize,
    cursor_page_row: *mut Row,
    cursor_page_cell: *mut Cell,
}

impl Screen {
    fn new() -> Screen {
        let node = Box::into_raw(Box::new(Node { page: Page::new() }));
        // The cursor sits on row 4; a second tracked pin sits inside the region.
        let cursor_pin = Box::into_raw(Box::new(Pin { node, x: 1, y: 4 }));
        let other_pin = Box::into_raw(Box::new(Pin { node, x: 0, y: 3 }));
        let page = unsafe { Node::page(node) };
        let row = unsafe { (*page).rows_ptr().add(4) };
        let cell = unsafe { (*page).get_cells(row).add(1) };
        Screen {
            node,
            tracked_pins: vec![cursor_pin, other_pin],
            cursor_pin,
            cursor_x: 1,
            cursor_page_row: row,
            cursor_page_cell: cell,
        }
    }

    /// Mirrors `Screen::clear_cells(&mut self, page, row, cells)`: it takes
    /// `&mut self` and RE-DERIVES a `*mut Page` from the node it already owns.
    /// That re-derivation is what invalidates any `&mut Page` a caller is
    /// holding.
    fn clear_cells(&mut self, row: *mut Row) {
        let page = unsafe { Node::page(self.node) };
        let cells = unsafe { (*page).get_cells(row) };
        for x in 0..COLS {
            unsafe { *cells.add(x) = Cell(0) };
        }
        unsafe { (*page).dirty = true };
    }

    /// Frees everything the harness allocated, so a clean candidate is clean
    /// under Miri's leak check too (a leak error would drown the signal).
    fn deinit(self) {
        for p in self.tracked_pins.iter().copied() {
            drop(unsafe { Box::from_raw(p) });
        }
        drop(unsafe { Box::from_raw(self.node) });
    }

    /// Mirrors `assert_integrity()`: re-walks the page through the node —
    /// BOTH its rows and its cells, as `Page::assert_integrity` does — and then
    /// through the cursor's cached pointers. Walking the ROWS through the raw
    /// base is load-bearing for `rowslice_ub`: it is the access that pops a
    /// slice-derived tag over the same row memory.
    fn assert_integrity(&self) -> u64 {
        let page = unsafe { Node::page(self.node) };
        let rows = unsafe { (*page).rows_ptr() };
        let mut acc = 0u64;
        for y in 0..unsafe { (*page).size_rows } {
            acc = acc.wrapping_add(unsafe { *rows.add(y) }.0);
            let cells = unsafe { (*page).cells_at(y) };
            for x in 0..COLS {
                acc = acc.wrapping_add(unsafe { *cells.add(x) }.0);
            }
        }
        acc.wrapping_add(unsafe { *self.cursor_page_cell }.0)
    }
}

/// `fastmem::rotateOnce` — the port's real implementation, verbatim.
fn rotate_once<T: Copy>(items: &mut [T]) {
    let tmp = items[0];
    let len = items.len();
    items.copy_within(1..len, 0);
    items[len - 1] = tmp;
}

// ---------------------------------------------------------------------------
// Candidates
// ---------------------------------------------------------------------------

/// The SHIPPED shape of `cursor_scroll_region_up_inner`'s fast path.
fn raw_ok(limit: usize) -> u64 {
    let mut s = Screen::new();
    let pin = s.cursor_pin;
    let pin_y = unsafe { (*pin).y };
    let pin_node = unsafe { (*pin).node };

    let page: *mut Page = unsafe { Node::page(pin_node) };
    let rows: *mut Row = unsafe { (*page).rows_ptr().add(pin_y - limit) };

    // Clear the erased (top) row: raw zero-fill or the &mut-self generic clear.
    {
        let row: *mut Row = rows;
        if !unsafe { *row }.managed_memory() {
            let cells = unsafe { (*page).get_cells(row) };
            unsafe { std::ptr::write_bytes(cells, 0u8, COLS) };
        } else {
            s.clear_cells(row);
        }
    }

    // The ONLY &mut into page memory, narrowly scoped.
    rotate_once(unsafe { std::slice::from_raw_parts_mut(rows, limit + 1) });

    unsafe { (*page).dirty = true };

    // Tracked-pin fixups, through raw pin pointers.
    for p in s.tracked_pins.iter().copied() {
        if !std::ptr::eq(unsafe { (*p).node }, pin_node)
            || std::ptr::eq(p, pin)
            || unsafe { (*p).y } < pin_y - limit
            || unsafe { (*p).y } > pin_y
        {
            continue;
        }
        if unsafe { (*p).y } == 0 {
            unsafe { (*p).x = 0 };
        } else {
            unsafe { (*p).y -= 1 };
        }
    }

    // Cursor cache refresh, recomputed from the RAW base.
    let cursor_row: *mut Row = unsafe { rows.add(limit) };
    s.cursor_page_row = cursor_row;
    s.cursor_page_cell = unsafe { (*page).get_cells(cursor_row).add(s.cursor_x) };

    // The SAME follow-on sequence `rowslice_ub` runs, so the two candidates
    // differ in exactly one thing: where `cursor_row` came from. Models a later
    // `pages.clear_dirty()` walking rows through the page base, then
    // `cursor_mark_dirty()` writing through the cursor's cached row pointer.
    for y in 0..SIZE_ROWS {
        unsafe { *(*page).rows_ptr().add(y) = Row(0) };
    }
    let _ = s.assert_integrity();
    unsafe { *cursor_row = Row(0) };

    let out = s.assert_integrity();
    s.deinit();
    out
}

/// NEGATIVE CONTROL (Step 4(b)): reintroduce `&mut Page`.
fn mutpage_ub(limit: usize) -> u64 {
    let mut s = Screen::new();
    let pin = s.cursor_pin;
    let pin_y = unsafe { (*pin).y };
    let pin_node = unsafe { (*pin).node };

    // THE MUTATION: a unique borrow of the whole Page, held across the calls
    // below. This is what the port's PORT NOTE forbids.
    let page: &mut Page = unsafe { &mut *Node::page(pin_node) };
    let rows: *mut Row = unsafe { page.rows_ptr().add(pin_y - limit) };

    // `clear_cells` takes `&mut self` and re-derives `*mut Page` from the node,
    // which invalidates `page`.
    s.clear_cells(rows);

    // ... and then `page` is used again.
    page.dirty = true;
    rotate_once(unsafe { std::slice::from_raw_parts_mut(rows, limit + 1) });

    let cursor_row: *mut Row = unsafe { rows.add(limit) };
    s.cursor_page_row = cursor_row;
    s.cursor_page_cell = unsafe { page.get_cells(cursor_row).add(s.cursor_x) };

    let out = s.assert_integrity();
    s.deinit();
    out
}

/// NEGATIVE CONTROL: derive the cursor's cached row FROM the `&mut [Row]` the
/// rotate borrowed, then keep using it after the page is touched raw again.
fn rowslice_ub(limit: usize) -> u64 {
    let mut s = Screen::new();
    let pin = s.cursor_pin;
    let pin_y = unsafe { (*pin).y };
    let pin_node = unsafe { (*pin).node };

    let page: *mut Page = unsafe { Node::page(pin_node) };
    let rows: *mut Row = unsafe { (*page).rows_ptr().add(pin_y - limit) };

    {
        let cells = unsafe { (*page).get_cells(rows) };
        unsafe { std::ptr::write_bytes(cells, 0u8, COLS) };
    }

    // THE MUTATION: keep the rotate's slice alive and take the cursor row out
    // of it, rather than recomputing from the raw base.
    let slice: &mut [Row] = unsafe { std::slice::from_raw_parts_mut(rows, limit + 1) };
    rotate_once(slice);
    let cursor_row: *mut Row = &mut slice[limit];

    // Any subsequent access through the raw page pointer pops the slice's tag.
    // The row-clearing write below is what the real code does on the next
    // operation that touches this page (e.g. `clear_dirty`).
    unsafe { (*page).dirty = true };
    for y in 0..SIZE_ROWS {
        unsafe { *(*page).rows_ptr().add(y) = Row(0) };
    }
    let _ = s.assert_integrity();

    // ... and then the slice-derived pointer is used.
    s.cursor_page_row = cursor_row;
    s.cursor_page_cell = unsafe { (*page).get_cells(cursor_row).add(s.cursor_x) };
    unsafe { *cursor_row = Row(0) };

    let out = s.assert_integrity();
    s.deinit();
    out
}

fn main() {
    let candidate = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: mirihar <raw_ok|mutpage_ub|rowslice_ub>");
        std::process::exit(2);
    });

    let out = match candidate.as_str() {
        "raw_ok" => raw_ok(3),
        "mutpage_ub" => mutpage_ub(3),
        "rowslice_ub" => rowslice_ub(3),
        other => {
            eprintln!("unknown candidate: {other}");
            std::process::exit(2);
        }
    };
    println!("{candidate}: {out}");
}
