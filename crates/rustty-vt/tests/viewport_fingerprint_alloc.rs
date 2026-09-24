//! `P3-W11 T3` (`P3-W6 T6` Step 3) — the negative control for the viewport
//! fingerprint's retained storage (oracle `9659167ec`, `bc8bb6c0f`).
//!
//! Upstream's `fingerprint only allocates on changes` uses Zig's
//! `testing.FailingAllocator`, which rustty has no direct equivalent of because
//! `rustty-vt` allocates through `rustty_vt::alloc::Global`. This file
//! installs a `#[global_allocator]` that can both COUNT and FAIL allocations,
//! which gives the two assertions the plan requires:
//!
//!   1. an unchanged viewport update performs **zero** allocations — an
//!      assertion on the allocation COUNT, not merely on correctness. A test
//!      that only checked `update()` returns `false` passes just as well
//!      against the pre-`9659167ec` code that allocated a whole replacement
//!      fingerprint and threw it away.
//!   2. an allocation failure inside `Fingerprint::update` leaves the OLD
//!      fingerprint intact. That is the contract created by doing
//!      `ensureTotalCapacity` FIRST, before any mutation of the buffer.
//!
//! WHY ITS OWN TEST BINARY: a `#[global_allocator]` is process-wide, and the
//! failure arm has to be armed around one specific call. The arming state is
//! thread-local and this file holds a single `#[test]`, so no other test can
//! observe it.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use rustty_vt::color::{DynamicPalette, DynamicRGB};
use rustty_vt::kitty;
use rustty_vt::modes;
use rustty_vt::screen::Screen;
use rustty_vt::search::viewport::ViewportSearch;
use rustty_vt::terminal::{Colors, Options as TerminalOptions, Terminal};

// ---------------------------------------------------------------------------
// Counting + failing global allocator.
// ---------------------------------------------------------------------------

thread_local! {
    /// Number of allocations (alloc + realloc) made on this thread.
    static COUNT: Cell<u64> = const { Cell::new(0) };
    /// When true, every allocation on this thread fails (returns null).
    static ARMED: Cell<bool> = const { Cell::new(false) };
}

struct Probe;

// SAFETY: every method delegates to `System`, only adding bookkeeping. The
// `ARMED` arm returns a null pointer, which is the documented way for a
// `GlobalAlloc` to report failure; `Vec::try_reserve` handles it.
unsafe impl GlobalAlloc for Probe {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if ARMED.with(|a| a.get()) {
            return core::ptr::null_mut();
        }
        COUNT.with(|c| c.set(c.get() + 1));
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if ARMED.with(|a| a.get()) {
            return core::ptr::null_mut();
        }
        COUNT.with(|c| c.set(c.get() + 1));
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if ARMED.with(|a| a.get()) {
            return core::ptr::null_mut();
        }
        COUNT.with(|c| c.set(c.get() + 1));
        unsafe { System.alloc_zeroed(layout) }
    }
}

// D6-ALLOW(counting/failing probe, not an allocator substitution): this lives in
// an integration test target (`tests/`), so it exists only in this one test
// binary and can never enter a shipped build. It does not blind ASan/LSan the
// way oven-sh/bun#30875's mimalloc did: every success arm forwards to
// `std::alloc::System`, i.e. plain `malloc`/`free`, which the sanitizer
// interceptors still see; it only bumps a thread-local counter first. The ARMED
// arm returns null WITHOUT allocating — a refused allocation is nothing for a
// sanitizer to track, so no allocation is hidden from it. Same shape and same
// rationale as the D6-ALLOW in `rustty-renderer/src/link.rs`.
#[global_allocator]
static PROBE: Probe = Probe;

fn allocs() -> u64 {
    COUNT.with(|c| c.get())
}

fn terminal_opts(cols: u16, rows: u16) -> TerminalOptions {
    TerminalOptions {
        cols,
        rows,
        max_scrollback_bytes: Some(10_000),
        max_scrollback_lines: None,
        colors: Colors {
            background: DynamicRGB::UNSET,
            foreground: DynamicRGB::UNSET,
            cursor: DynamicRGB::UNSET,
            palette: DynamicPalette::default_palette(),
        },
        default_modes: modes::ModePacked::default(),
        default_cursor_style: rustty_vt::screen::CursorStyle::Block,
        default_cursor_blink: Some(false),
        kitty_image_storage_limit: 320 * 1000 * 1000,
        kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
    }
}

// test "fingerprint only allocates on changes"
#[test]
fn fingerprint_only_allocates_on_changes() {
    // REBUILD SENTINEL: verified to fire when tripped (see the lane report).
    assert_eq!(2 + 2, 4, "REBUILD SENTINEL");

    let alloc = rustty_vt::alloc::Global;
    let mut t: Terminal = Terminal::init(alloc, terminal_opts(10, 10)).unwrap();
    let active_ptr: *mut Screen = t.screens.active;

    let mut search: ViewportSearch = ViewportSearch::init(alloc, b"needle").unwrap();
    search.active_dirty = Some(false);

    // First update: the viewport is new, so this must allocate and report a
    // change.
    let before = allocs();
    assert!(search.update(unsafe { &(*active_ptr).pages }).unwrap());
    assert!(
        allocs() > before,
        "the first update must allocate the fingerprint storage; if it does \
         not, this control is VACUOUS"
    );
    let entries_after_first: Vec<(usize, u64)> = search
        .fingerprint
        .entries
        .iter()
        .map(|e| (e.node as usize, e.serial))
        .collect();
    assert!(!entries_after_first.is_empty());

    // Second update over an UNCHANGED viewport with active-dirty tracking
    // disabled: zero allocations. Pre-`9659167ec` this built a whole
    // replacement fingerprint before discovering nothing had changed.
    //
    // Upstream states the same contract through its FailingAllocator:
    //
    //   Fail the next allocation. An unchanged viewport with active dirty
    //   tracking disabled should reuse the existing fingerprint and return
    //   unchanged before rebuilding the search window, so this update should
    //   succeed without reporting a viewport change.
    let before = allocs();
    assert!(!search.update(unsafe { &(*active_ptr).pages }).unwrap());
    assert_eq!(
        before,
        allocs(),
        "an unchanged viewport update must not allocate"
    );

    // A third unchanged update, to show it is steady-state and not a one-off.
    let before = allocs();
    assert!(!search.update(unsafe { &(*active_ptr).pages }).unwrap());
    assert_eq!(before, allocs());

    // ---------------------------------------------------------------------
    // Allocation failure preserves the OLD fingerprint.
    //
    // `Fingerprint::update` reserves capacity for `pages.rows` entries BEFORE
    // touching the buffer. Grow the terminal so the retained capacity is no
    // longer enough, then fail that reservation.
    // ---------------------------------------------------------------------
    t.resize(
        alloc,
        rustty_vt::terminal::Resize {
            cols: 10,
            rows: 4000,
            cell_size_px: None,
        },
    )
    .unwrap();
    let active_ptr: *mut Screen = t.screens.active;

    ARMED.with(|a| a.set(true));
    let res = search.update(unsafe { &(*active_ptr).pages });
    ARMED.with(|a| a.set(false));

    assert!(
        res.is_err(),
        "a failed capacity reservation must surface as OutOfMemory; if this \
         update succeeded the reservation was not needed and the control is \
         VACUOUS"
    );

    let entries_after_failure: Vec<(usize, u64)> = search
        .fingerprint
        .entries
        .iter()
        .map(|e| (e.node as usize, e.serial))
        .collect();
    assert_eq!(
        entries_after_first, entries_after_failure,
        "an allocation failure inside Fingerprint::update must leave the OLD \
         fingerprint byte-for-byte intact — that is what reserving before any \
         mutation buys"
    );

    search.deinit();
    t.deinit(alloc);
}
