//! Isolated, non-FFI Miri harness that PROVES the inspector's `Mouse.last_point`
//! Pin handling is Stacked/Tree-Borrows clean across a reflow-like `&mut PageList`
//! mutation — PREPORT_INSPECTOR.md **risk R3** ("`Mouse.last_point: ?terminal.Pin`
//! raw-aliases pagelist memory; INVALIDATED by reflow").
//!
//! # What the real code does (faithful model)
//!
//!   * `src/inspector/widgets/surface.zig:203` — `Mouse.last_point: ?terminal.Pin`.
//!     A `Pin` (`crates/rustty-vt/src/pagelist/mod.rs:630`) is `{ node: *mut Node,
//!     y, x, garbage }` — a **non-owning, `Copy`, raw** back-pointer into a PageList
//!     `Node` (`pagelist/mod.rs:134`), stored as inspector-side mouse state that
//!     PERSISTS across frames.
//!   * Between the mouse-move that records `last_point` and the draw that reads it,
//!     terminal output can trigger a reflow / page-prune that FREES the very `Node`
//!     the stored `Pin` addresses (`&mut PageList`). The stored `Pin` is now a
//!     DANGLING raw handle.
//!   * `surface.zig:377-384` mouseTable re-resolves it, null-guarded:
//!         const p  = mouse.last_point orelse break :pt .{};
//!         const pt = t.screens.active.pages.pointFromPin(.active, p) orelse break :pt .{};
//!     i.e. the stale `Pin` is fed to `PageList.pointFromPin`
//!     (`pagelist/pin.rs:121`), which returns `null` when the pin no longer fits.
//!
//! # The load-bearing SB/TB fact this harness certifies
//!
//! `pointFromPin` (`pagelist/pin.rs:121-152`) walks the LIVE list from
//! `getTopLeft(tag).node`, following `.next`, and locates the pin's node **purely
//! by ADDRESS comparison** — `core::ptr::eq(node, p.node)` (pin.rs:126,141). It
//! dereferences only the nodes reached through the live traversal (`(*node).data.
//! size.rows`, `(*node).next`); it **NEVER dereferences `p.node` itself** and reads
//! only the pin's `Copy` scalar fields (`p.x`, `p.y`). Comparing a dangling raw
//! pointer by address is a pure integer compare — it does not access freed memory,
//! so it is defined under both Stacked and Tree Borrows. A freed pinned node simply
//! never matches a live node ⇒ `None`. **The stale Pin is a weak handle, re-resolved
//! by address-walk, never a live borrow** — exactly R3's required discipline.
//!
//! # Candidates (Miri only interprets executed code; select by argv[1])
//!
//! | candidate | models | expect |
//! |---|---|---|
//! | `pin_stale_resolves_none` | **shipped**: hold a `Pin` into node #2, reflow (free node #2 + relink), then `point_from_pin(stale)` — address-only walk ⇒ `None`, NO deref of the freed node | CLEAN (SB+TB) |
//! | `pin_valid_roundtrip`     | positive: a `Pin` into a node that SURVIVES reflow re-resolves to its live coordinate | CLEAN (SB+TB) |
//! | `pin_deref_stale`         | NEGATIVE: the naive port that DEREFERENCES `p.node` after the reflow freed it (`(*p.node).rows`) — use-after-free | UB (SB+TB) |
//!
//! Pass criterion: the two shipped candidates CLEAN under BOTH Stacked Borrows
//! (default) and Tree Borrows (`-Zmiri-tree-borrows`); the NEGATIVE flagged UB at
//! the stale-`p.node` deref (proving the address-only discipline is load-bearing,
//! not incidentally green).
#![allow(dead_code)]

use std::ptr;

// ---- Node analogue (`rustty_vt::pagelist::Node`, pagelist/mod.rs:134) ----------
// The load-bearing fields: the intrusive `next` link the traversal follows, and a
// `rows` stand-in for `data.size.rows` (the only node datum pointFromPin reads).
struct Node {
    // next: *mut Node  (pagelist/mod.rs:136) — the intrusive forward link.
    next: *mut Node,
    // data.size.rows  (read at pin.rs:133,146 for the coordinate accumulation).
    rows: u32,
    // a serial so we can eyeball which physical node an address belongs to.
    serial: u64,
}

// ---- Coordinate analogue (`rustty_vt::point::Coordinate`) ----------------------
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
struct Coordinate {
    x: u32,
    y: u32,
}

// ---- Pin analogue (`rustty_vt::pagelist::Pin`, pagelist/mod.rs:630) -------------
// NON-owning, Copy, raw back-pointer — the exact shape `Mouse.last_point` stores.
// It owns NOTHING: dropping/copying it never touches `*node`.
#[derive(Clone, Copy)]
struct Pin {
    // node: *mut Node  — raw-aliases pagelist memory (R3).
    node: *mut Node,
    y: u32,
    x: u32,
}

// ---- PageList analogue (`rustty_vt::PageList`, pagelist/mod.rs:508) -------------
// Owns its nodes (each a separate `Box::into_raw` allocation, mirroring the
// MemoryPool-backed `List.Node`s). `first` is the head the top-left walk starts at.
struct PageList {
    first: *mut Node,
}

impl PageList {
    /// Build a `PageList` of `rows_per_node.len()` nodes, each carrying the given
    /// row count, linked front-to-back. Returns the list; nodes are heap-stable
    /// addresses until freed.
    fn new(rows_per_node: &[u32]) -> PageList {
        let mut first: *mut Node = ptr::null_mut();
        let mut prev: *mut Node = ptr::null_mut();
        for (i, &rows) in rows_per_node.iter().enumerate() {
            let node: *mut Node = Box::into_raw(Box::new(Node {
                next: ptr::null_mut(),
                rows,
                serial: i as u64,
            }));
            if first.is_null() {
                first = node;
            } else {
                unsafe { (*prev).next = node };
            }
            prev = node;
        }
        PageList { first }
    }

    /// Free every node the list currently owns (used at teardown).
    unsafe fn free_all(&mut self) {
        let mut n = self.first;
        while !n.is_null() {
            let next = unsafe { (*n).next };
            drop(unsafe { Box::from_raw(n) });
            n = next;
        }
        self.first = ptr::null_mut();
    }

    /// `getTopLeft(.active/.screen)` analogue — here the screen top-left is `first`,
    /// y=0 (pagelist/mod.rs:1645 returns a `Pin{node,y}`; we model the common
    /// full-scrollback case where the top-left is the first node, offset 0).
    fn top_left(&self) -> Pin {
        Pin { node: self.first, y: 0, x: 0 }
    }

    /// FAITHFUL port of `PageList.pointFromPin` (pagelist/pin.rs:121-152), reduced to
    /// the R3-relevant control flow. **It resolves `p.node` by ADDRESS ONLY** — the
    /// stale pin's `node` is only ever the RHS of `ptr::eq`, never dereferenced; only
    /// live-traversal nodes are dereferenced. Returns `None` if `p.node` is not a
    /// live node in the walked region (== the reflow-freed case).
    fn point_from_pin(&self, p: Pin) -> Option<Coordinate> {
        let tl = self.top_left();

        // Count our first page which is special because it may be partial.
        let mut coord = Coordinate { x: p.x, y: 0 };
        if ptr::eq(p.node, tl.node) {
            // address compare only — no deref of p.node
            if tl.y > p.y {
                return None;
            }
            coord.y = p.y - tl.y;
        } else {
            // deref of the LIVE top-left node only:
            coord.y += unsafe { (*tl.node).rows } - tl.y;
            let mut node_ = unsafe { (*tl.node).next };
            loop {
                if node_.is_null() {
                    // We never saw our node, meaning we're outside the range.
                    // (== reflow freed p.node: it is not among the live nodes.)
                    return None;
                }
                let node = node_;
                if ptr::eq(node, p.node) {
                    // address compare of the (possibly dangling) p.node — no deref.
                    coord.y += p.y;
                    break;
                }
                coord.y += unsafe { (*node).rows };
                node_ = unsafe { (*node).next };
            }
        }

        Some(coord)
    }

    /// A reflow-like `&mut PageList` mutation that PRUNES `victim` from the list and
    /// FREES its allocation — the exact hazard that invalidates a persisted `Pin`
    /// (a reflow / page-eviction between mouse-move and draw). After this returns,
    /// any `Pin{node: victim, ..}` is a DANGLING raw handle.
    unsafe fn reflow_prune(&mut self, victim: *mut Node) {
        // unlink victim (fix the predecessor's `next`)…
        if ptr::eq(self.first, victim) {
            self.first = unsafe { (*victim).next };
        } else {
            let mut n = self.first;
            while !n.is_null() {
                let next = unsafe { (*n).next };
                if ptr::eq(next, victim) {
                    unsafe { (*n).next = (*victim).next };
                    break;
                }
                n = next;
            }
        }
        // …then free it (the page's backing memory is reclaimed by the reflow).
        drop(unsafe { Box::from_raw(victim) });
    }
}

// ============ CANDIDATE: pin_stale_resolves_none (expect CLEAN SB+TB) ============
// The SHIPPED discipline. Record a `Pin` into node #2 (as `Mouse.last_point` would
// on a mouse-move), then a reflow FREES node #2, then the draw re-resolves the stale
// pin via `point_from_pin` — an address-only walk that finds no matching live node
// and returns `None`. The freed node is NEVER dereferenced.
fn pin_stale_resolves_none() {
    let mut pages = PageList::new(&[10, 20, 30]); // 3 nodes
    // grab node #2's address (the middle page) and pin into it.
    let node2 = unsafe { (*pages.first).next };
    let stale = Pin { node: node2, y: 5, x: 7 };

    // sanity: while node2 is still live the pin resolves.
    let before = pages.point_from_pin(stale);
    assert_eq!(before, Some(Coordinate { x: 7, y: 15 }), "pre-reflow: 10 (page0 rows) + 5 (pin.y)");

    // REFLOW: prune+free node #2 (&mut PageList). `stale.node` now dangles.
    unsafe { pages.reflow_prune(node2) };

    // DRAW: re-resolve the stale pin. Address-only walk over live nodes {#0,#2(new
    // head-2)} — wait, node2 is gone; the live chain is node0 -> node3. `stale.node`
    // matches neither by address ⇒ None. No deref of the freed node.
    let after = pages.point_from_pin(stale);
    assert!(after.is_none(), "stale pin must re-resolve to None, never deref freed memory");

    unsafe { pages.free_all() };
    println!("pin_stale_resolves_none: OK (stale pin address-compared, resolved None, no UAF)");
}

// ============ CANDIDATE: pin_valid_roundtrip (expect CLEAN SB+TB) ============
// Positive path: a pin into a node that SURVIVES the reflow re-resolves to its live
// coordinate (proves the address-only walk isn't vacuously always-None).
fn pin_valid_roundtrip() {
    let mut pages = PageList::new(&[10, 20, 30]);
    let node2 = unsafe { (*pages.first).next }; // rows=20
    let node3 = unsafe { (*node2).next }; // rows=30, survives
    let keep = Pin { node: node3, y: 4, x: 1 };

    // REFLOW prunes node #2 only; node #3 survives at a stable address.
    unsafe { pages.reflow_prune(node2) };

    // node3 is now the 2nd live node: coord.y = 10 (page0) + 4 (pin.y) = 14.
    let after = pages.point_from_pin(keep);
    assert_eq!(after, Some(Coordinate { x: 1, y: 14 }), "surviving pin re-resolves live");

    unsafe { pages.free_all() };
    println!("pin_valid_roundtrip: OK (surviving pin re-resolved to live coordinate)");
}

// ============ NEGATIVE: pin_deref_stale (expect UB — SB + TB) ============
// The NAIVE port that treats the stored Pin as a live handle and DEREFERENCES
// `p.node` after the reflow freed it (e.g. reading `(*p.node).rows` to compute the
// coordinate directly instead of the address-only walk). This is the use-after-free
// R3 forbids — Miri flags it AT THE DEREF, proving the shipped address-only
// discipline is load-bearing (if deref-ing the stale node were fine, this would be
// clean and the certification false-green).
fn pin_deref_stale() {
    let mut pages = PageList::new(&[10, 20, 30]);
    let node2 = unsafe { (*pages.first).next };
    let stale = Pin { node: node2, y: 5, x: 7 };

    unsafe { pages.reflow_prune(node2) }; // frees node2; stale.node dangles

    // UB: the naive resolver dereferences the freed p.node directly.
    let rows = unsafe { (*stale.node).rows }; // <-- use-after-free flagged here
    println!("pin_deref_stale: (unreachable — Miri should have flagged UB) rows={rows}");

    unsafe { pages.free_all() };
}

fn main() {
    let which = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "pin_stale_resolves_none".to_string());
    match which.as_str() {
        "pin_stale_resolves_none" => pin_stale_resolves_none(),
        "pin_valid_roundtrip" => pin_valid_roundtrip(),
        "pin_deref_stale" => pin_deref_stale(),
        other => {
            eprintln!("unknown candidate: {other}");
            std::process::exit(2);
        }
    }
}
