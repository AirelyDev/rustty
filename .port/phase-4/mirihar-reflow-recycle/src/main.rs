//! Isolated, non-FFI Miri harness for the W2 reflow end-state chain
//! (upstream `46276d046` "recycle pages within a column reflow" +
//! `c249b9de3`/`88ed6bebf` `bulkRunLength`/`copyRun`).
//!
//! It faithfully models the exact aliasing structure of
//! `crates/rustty-vt/src/pagelist/resize.rs` (`resize_cols_inner` /
//! `ReflowCursor::{reflow_row, copy_run}`) over
//! `crates/rustty-vt/src/pagelist/mod.rs` (`create_page` recycle path):
//!
//!   * A `Node`'s METADATA (the `Page` struct: memory ptr/len, sizes, the
//!     style-set header) lives in the NODE allocation; the cell rows and the
//!     style ITEMS live in a separate page-aligned PAGE BUFFER allocation.
//!     The two allocations relevant to every compare below are therefore:
//!     src page buffer vs dst page buffer vs the node structs.
//!   * `reflow_row` derives `cells: &[Cell]` (shared slice) from the SOURCE
//!     page buffer and holds it across the whole per-row loop.
//!   * `copy_run` — with that source slice LIVE — mutates the DESTINATION
//!     page: a `&mut` style item in the dst page buffer (the
//!     `add_with_id`/`use_multiple` ref-count bump) and a raw
//!     `copy_nonoverlapping` of the cells into the dst rows. This is sound
//!     because src and dst are always DIFFERENT page-buffer allocations.
//!   * `46276d046` is why that invariant needs proof: after a source node's
//!     LAST row is reflowed, the node is stashed (`recycle_node`) and a later
//!     `create_page` — reached mid-row while a DIFFERENT source node's cell
//!     slice is live — re-initializes the stashed node's page buffer in place
//!     (`memset` via the stored raw memory pointer + metadata rewrite) and
//!     hands it out as the new DESTINATION page. The source slice being held
//!     across that memset belongs to the CURRENT source node, never the
//!     stashed one: a node is stashed only after every one of its rows has
//!     been reflowed, and the row iterator has already advanced past it.
//!   * The stash-then-recycle write requires that NO reference into the
//!     stashed node's page buffer survives the stash — the foil shows Miri
//!     catching exactly that violation.
//!
//! Candidates (argv[1]; Miri only checks executed code):
//!   * `ported`               — the shipped discipline: recycle A's buffer as
//!                              the new dst page WHILE node B's source slice
//!                              is live, then bulk-copy into it. MUST be
//!                              clean under Stacked Borrows and Tree Borrows.
//!   * `ub_recycle_live_slice` — DOCUMENTED-UB foil: the recycle memset runs
//!                              while a `&[Cell]` slice into the SAME (the
//!                              stashed) node's page buffer is still live,
//!                              and the slice is read afterwards. The raw
//!                              write through the buffer's root provenance
//!                              invalidates the shared slice tag; Miri flags
//!                              the read. This is the shape the
//!                              stash-only-after-the-last-row ordering makes
//!                              unreachable in the real code.
//!
//! Run (from this dir):
//! ```sh
//! cargo +nightly miri run -q -- ported                                 # Stacked Borrows
//! MIRIFLAGS=-Zmiri-tree-borrows cargo +nightly miri run -q -- ported   # Tree Borrows
//! cargo +nightly miri run -q -- ub_recycle_live_slice                  # expected: UB report
//! ```
#![allow(dead_code)]

use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::ptr;

// ---------------------------------------------------------------------------
// Cell / page-buffer model.
//
// A page buffer is one heap allocation: `CELL_SLOTS` cells followed by
// `STYLE_SLOTS` u16 style ref-counts (modeling the style-set ITEMS, which
// live inside `Page.memory` in the real layout, reached from the set header
// via the memory base pointer).
// ---------------------------------------------------------------------------

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Cell(u64);

impl Cell {
    fn init(cp: u32, style_id: u16) -> Cell {
        Cell((cp as u64) | ((style_id as u64) << 32))
    }
    fn style_id(self) -> u16 {
        (self.0 >> 32) as u16
    }
}

const CELL_SLOTS: usize = 8;
const STYLE_SLOTS: usize = 4;

fn buf_layout() -> Layout {
    Layout::from_size_align(
        CELL_SLOTS * size_of::<Cell>() + STYLE_SLOTS * size_of::<u16>(),
        align_of::<Cell>(),
    )
    .unwrap()
}

// ---------------------------------------------------------------------------
// Node model: metadata (the embedded `Page`) in the node allocation, page
// bytes in the separate buffer. `mem` is the stored raw base pointer — the
// provenance root every page access derives from (`Page.memory_ptr`).
// ---------------------------------------------------------------------------

struct Node {
    /// `Page.memory_ptr` — root provenance over the whole page buffer.
    mem: *mut u8,
    /// `Page.size.cols` for the single modeled row.
    cols: usize,
    /// Number of live style items (models the style set header).
    styles_len: usize,
}

impl Node {
    fn cells_ptr(node: *mut Node) -> *mut Cell {
        // Raw projection from the stored base pointer; forms no reference.
        unsafe { (*node).mem as *mut Cell }
    }
    fn style_refs_ptr(node: *mut Node) -> *mut u16 {
        unsafe { (*node).mem.add(CELL_SLOTS * size_of::<Cell>()) as *mut u16 }
    }
}

fn create_node(cols: usize) -> *mut Node {
    let mem = unsafe { alloc_zeroed(buf_layout()) };
    assert!(!mem.is_null());
    Box::into_raw(Box::new(Node {
        mem,
        cols,
        styles_len: 0,
    }))
}

fn destroy_node(node: *mut Node) {
    unsafe {
        dealloc((*node).mem, buf_layout());
        drop(Box::from_raw(node));
    }
}

// ---------------------------------------------------------------------------
// PageList model: only the parts the aliasing depends on.
// ---------------------------------------------------------------------------

struct PageListModel {
    /// `recycle_node`: a node available for immediate reuse by
    /// `create_page`. Always null outside an in-progress reflow.
    recycle_node: *mut Node,
}

impl PageListModel {
    /// `create_page` with the `46276d046` recycle path: re-initialize the
    /// stashed node's page buffer in place through its stored raw base
    /// pointer (memset the dirtied prefix + metadata rewrite), exactly like
    /// `PageList::create_page` does before falling back to the pool.
    fn create_page(&mut self, cols: usize) -> *mut Node {
        if !self.recycle_node.is_null() {
            let node = self.recycle_node;
            self.recycle_node = ptr::null_mut();

            // The memset through the ROOT provenance (`mem`) — this is the
            // write that would invalidate any surviving reference into the
            // old page contents (see the foil).
            unsafe {
                let (mem, len) = ((*node).mem, buf_layout().size());
                ptr::write_bytes(mem, 0, len);
                // Metadata rewrite (`node.* = .{ .data = .resident ... }`).
                (*node).cols = cols;
                (*node).styles_len = 0;
            }
            return node;
        }

        create_node(cols)
    }
}

// ---------------------------------------------------------------------------
// ReflowCursor model: dst node + x position + the style memo.
// ---------------------------------------------------------------------------

struct ReflowCursor {
    node: *mut Node,
    x: usize,
    /// StyleCache: (src_page-identity, src_id, dst_id). Modeled by src node
    /// pointer identity like the real `*const Page` key.
    style_cache: (*const Node, u16, u16),
}

impl ReflowCursor {
    fn init(node: *mut Node) -> ReflowCursor {
        ReflowCursor {
            node,
            x: 0,
            style_cache: (ptr::null(), 0, 0),
        }
    }

    /// `copyRun`: with the SOURCE slice live, resolve the style into the
    /// DESTINATION page (a `&mut` ref-count item inside the dst page
    /// buffer), then bulk-copy the raw cells (`copy_nonoverlapping`).
    fn copy_run(&mut self, src_cells: &[Cell], src_node: *const Node) -> bool {
        assert!(!src_cells.is_empty());
        assert!(src_cells.len() <= unsafe { (*self.node).cols } - self.x);

        let style_id = src_cells[0].style_id();
        let n = src_cells.len() as u16;

        if style_id != 0 {
            if ptr::eq(self.style_cache.0, src_node) && self.style_cache.1 == style_id {
                // useMultiple: momentary &mut into the DST page buffer while
                // the SRC slice (different allocation) stays live.
                let refs = Node::style_refs_ptr(self.node);
                unsafe { *(&mut *refs.add(style_id as usize)) += n };
            } else {
                // add_with_id: read the src style through a shared ref into
                // the SRC page buffer (models `styles.get(...).*`)...
                let src_refs = Node::style_refs_ptr(src_node as *mut Node);
                let _src_style_ref: &u16 = unsafe { &*src_refs.add(style_id as usize) };
                // ...then insert/bump in the DST page buffer.
                let refs = Node::style_refs_ptr(self.node);
                unsafe {
                    *(&mut *refs.add(style_id as usize)) += n;
                    (*self.node).styles_len += 1;
                }
                self.style_cache = (src_node, style_id, style_id);
            }
        }

        // Copy the raw cell contents: src slice -> dst raw pointer. The two
        // page buffers are distinct allocations, so no overlap.
        let dst = unsafe { Node::cells_ptr(self.node).add(self.x) };
        unsafe { ptr::copy_nonoverlapping(src_cells.as_ptr(), dst, src_cells.len()) };
        self.x += src_cells.len();
        true
    }
}

// ---------------------------------------------------------------------------
// Candidates.
// ---------------------------------------------------------------------------

/// The shipped discipline (must be Miri-clean under SB and TB).
fn ported() {
    let mut list = PageListModel {
        recycle_node: ptr::null_mut(),
    };

    // Two source nodes with a full row of styled cells each.
    let node_a = create_node(CELL_SLOTS);
    let node_b = create_node(CELL_SLOTS);
    for (node, base_cp) in [(node_a, 0x100u32), (node_b, 0x200u32)] {
        let cells = Node::cells_ptr(node);
        let refs = Node::style_refs_ptr(node);
        for i in 0..CELL_SLOTS {
            unsafe { cells.add(i).write(Cell::init(base_cp + i as u32, 1)) };
        }
        unsafe {
            *refs.add(1) = CELL_SLOTS as u16;
            (*node).styles_len = 1;
        }
    }

    // First rewritten (destination) node: half the columns, so each source
    // row needs two destination pages and reflowing node B reaches
    // create_page mid-row.
    let dst_cols = CELL_SLOTS / 2;
    let first_rewritten = list.create_page(dst_cols);
    let mut cursor = ReflowCursor::init(first_rewritten);
    let mut dst_pages: Vec<*mut Node> = vec![first_rewritten];

    // --- Reflow node A ------------------------------------------------------
    {
        // The source slice, live across every dst mutation for this row.
        let cells_a: &[Cell] =
            unsafe { std::slice::from_raw_parts(Node::cells_ptr(node_a), CELL_SLOTS) };
        let mut x = 0;
        while x < cells_a.len() {
            if cursor.x == unsafe { (*cursor.node).cols } {
                // cursorScrollOrNewPage -> create_page (no stash yet: fresh).
                let node = list.create_page(dst_cols);
                dst_pages.push(node);
                cursor = ReflowCursor::init(node);
            }
            let max_run = (cells_a.len() - x).min(unsafe { (*cursor.node).cols } - cursor.x);
            let window = &cells_a[x..x + max_run];
            assert!(cursor.copy_run(window, node_a));
            x += max_run;
        }
        // Slice ends here — BEFORE the node can be stashed/recycled.
    }
    // Node A fully reflowed: stash it instead of destroying it.
    assert!(list.recycle_node.is_null());
    list.recycle_node = node_a;

    // --- Reflow node B ------------------------------------------------------
    {
        let cells_b: &[Cell] =
            unsafe { std::slice::from_raw_parts(Node::cells_ptr(node_b), CELL_SLOTS) };
        let mut x = 0;
        while x < cells_b.len() {
            if cursor.x == unsafe { (*cursor.node).cols } {
                // Mid-row create_page WHILE cells_b is live: this consumes
                // the stash and memsets node A's buffer — a different
                // allocation from cells_b's, so the slice stays valid.
                let node = list.create_page(dst_cols);
                dst_pages.push(node);
                cursor = ReflowCursor::init(node);
            }
            let max_run = (cells_b.len() - x).min(unsafe { (*cursor.node).cols } - cursor.x);
            let window = &cells_b[x..x + max_run];
            assert!(cursor.copy_run(window, node_b));
            x += max_run;
        }
    }
    // Node B fully reflowed: stash it (replacing nothing; stash was consumed).
    assert!(list.recycle_node.is_null());
    list.recycle_node = node_b;

    // Node A's buffer must have been recycled as a destination page.
    assert!(dst_pages.iter().any(|&n| ptr::eq(n, node_a)));

    // defer: a stashed node must not outlive the reflow. (The null-out
    // mirrors the shipped defer; the model ends here, hence the allow.)
    #[allow(unused_assignments)]
    if !list.recycle_node.is_null() {
        let node = list.recycle_node;
        list.recycle_node = ptr::null_mut();
        destroy_node(node);
    }

    // Verify the reflowed contents through fresh raw-derived reads.
    let mut expect = (0x100u32..0x100 + CELL_SLOTS as u32)
        .chain(0x200..0x200 + CELL_SLOTS as u32);
    for &node in &dst_pages {
        let cells = Node::cells_ptr(node);
        for i in 0..dst_cols {
            let got = unsafe { *cells.add(i) };
            assert_eq!(got, Cell::init(expect.next().unwrap(), 1));
        }
        assert_eq!(unsafe { *Node::style_refs_ptr(node).add(1) }, dst_cols as u16);
    }

    for node in dst_pages {
        destroy_node(node);
    }
    println!("ported: OK");
}

/// DOCUMENTED-UB foil: recycle a node's page buffer while a `&[Cell]` slice
/// into that SAME buffer is still live, then read the slice. The memset goes
/// through the buffer's root provenance and invalidates the shared slice
/// tag — Miri must flag the read. The real code makes this unreachable by
/// stashing a node only AFTER its last row (and thus the last slice into its
/// buffer) is finished.
fn ub_recycle_live_slice() {
    let mut list = PageListModel {
        recycle_node: ptr::null_mut(),
    };

    let node_a = create_node(CELL_SLOTS);
    let cells = Node::cells_ptr(node_a);
    for i in 0..CELL_SLOTS {
        unsafe { cells.add(i).write(Cell::init(0x100 + i as u32, 0)) };
    }

    // The banned interleave: slice still live when the node is stashed...
    let cells_a: &[Cell] = unsafe { std::slice::from_raw_parts(cells as *const Cell, CELL_SLOTS) };
    list.recycle_node = node_a;

    // ...and recycled (memset through the root provenance).
    let recycled = list.create_page(CELL_SLOTS / 2);
    assert!(ptr::eq(recycled, node_a));

    // UB: read through the invalidated slice.
    let first = cells_a[0];
    println!("ub_recycle_live_slice: read {first:?} (Miri should have flagged this)");

    destroy_node(recycled);
}

fn main() {
    let candidate = std::env::args().nth(1).unwrap_or_default();
    match candidate.as_str() {
        "ported" => ported(),
        "ub_recycle_live_slice" => ub_recycle_live_slice(),
        other => {
            eprintln!("unknown candidate {other:?}; use: ported | ub_recycle_live_slice");
            std::process::exit(2);
        }
    }
}
