//! `W29-B03` — the negative control for `Terminal::print`'s cross-page
//! wide-grapheme transfer (reference `Terminal.zig:1247-1266`, the
//! `transfer_graphemes:` block).
//!
//! The reference iterates the OLD page's grapheme slice in place:
//!
//! ```zig
//! const cps = old_pin.node.page().lookupGrapheme(old_rac.cell).?;
//! for (cps) |cp| try self.screens.active.appendGrapheme(new_rac.cell, cp);
//! ```
//!
//! The port used to copy that slice into a `Vec<u21>` first, to dodge the
//! borrow across `appendGrapheme`. That copy was a Rust-only allocation on a
//! path the reference does not allocate on, and — unlike `append_grapheme`,
//! whose failure is mapped to `OutOfMemory` — its failure would ABORT the
//! process (`Vec`'s `handle_alloc_error`). The fix keeps the slice as a raw
//! ptr+len instead, which is legal for the same reason the reference's in-place
//! iteration is: this arm only runs when `old_pin.node != new_pin.node`, and
//! `Screen::append_grapheme` mutates/reallocates the CURSOR's node only.
//!
//! A behavioral test cannot see the difference — the success path is identical
//! — so this is an assertion on the allocation COUNT, in the same shape as
//! `viewport_fingerprint_alloc.rs`.
//!
//! WHY ITS OWN TEST BINARY: a `#[global_allocator]` is process-wide. The
//! counting window is thread-local and this file holds a single `#[test]`, so
//! no other test can observe it.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use rustty_vt::color::{DynamicPalette, DynamicRGB};
use rustty_vt::kitty;
use rustty_vt::modes;
use rustty_vt::terminal::{Colors, Options as TerminalOptions, Terminal};

/// Codepoints parked on the OLD page's cell before the transfer: 7 combining
/// marks plus a ZWJ. The `to_vec` that used to sit in the transfer loop
/// allocated exactly `CLUSTER * 4` bytes at `align_of::<u32>()`, which is what
/// the probe below watches for. The count is deliberately odd-sized so no
/// unrelated allocation in `print` can be mistaken for it.
const CLUSTER: usize = 8;
const TARGET_SIZE: usize = CLUSTER * core::mem::size_of::<u32>();
const TARGET_ALIGN: usize = core::mem::align_of::<u32>();

// ---------------------------------------------------------------------------
// Counting global allocator, watching one layout inside one window.
// ---------------------------------------------------------------------------

thread_local! {
    /// Whether allocations on this thread are currently being watched.
    static WATCH: Cell<bool> = const { Cell::new(false) };
    /// Allocations matching `TARGET_SIZE`/`TARGET_ALIGN` seen while watching.
    static HITS: Cell<u64> = const { Cell::new(0) };
}

struct Probe;

fn note(layout: Layout) {
    if layout.size() == TARGET_SIZE && layout.align() == TARGET_ALIGN && WATCH.with(|w| w.get()) {
        HITS.with(|h| h.set(h.get() + 1));
    }
}

// SAFETY: every method delegates to `System`, only adding bookkeeping (a
// thread-local `Cell` bump, which never allocates).
unsafe impl GlobalAlloc for Probe {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        note(layout);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        note(unsafe { Layout::from_size_align_unchecked(new_size, layout.align()) });
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        note(layout);
        unsafe { System.alloc_zeroed(layout) }
    }
}

// D6-ALLOW(counting probe, not an allocator substitution): this lives in an
// integration test target (`tests/`), so it exists only in this one test binary
// and can never enter a shipped build. Every arm forwards to
// `std::alloc::System`, i.e. plain `malloc`/`free`, which sanitizer
// interceptors still see; it only bumps a thread-local counter first. Same
// shape and rationale as the D6-ALLOW in `viewport_fingerprint_alloc.rs`.
#[global_allocator]
static PROBE: Probe = Probe;

fn watch<T>(f: impl FnOnce() -> T) -> (T, u64) {
    HITS.with(|h| h.set(0));
    WATCH.with(|w| w.set(true));
    let out = f();
    WATCH.with(|w| w.set(false));
    (out, HITS.with(|h| h.get()))
}

/// Wide enough that a page holds few enough rows for the padding loop below to
/// fill one quickly.
const COLS: u16 = 120;

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

#[test]
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (rustty-vt, parity tail C-1): intractable under Miri (it fills a whole page line by line, past a 9-minute run before it was stopped); the cross-page grapheme transfer it measures is Miri-verified in the G6c lane by terminal::tests::terminal_print_wide_grapheme_transfers_across_a_page_seam; runs natively in cargo test --workspace (see MIRI_EXCEPTIONS.md)"
)]
fn cross_page_wide_grapheme_transfer_does_not_allocate() {
    // Non-vacuity control for the probe itself: a `Vec<u32>` of exactly the
    // cluster length is the allocation this test claims is gone, so the probe
    // must be able to see one.
    let (v, hits) = watch(|| vec![0u32; CLUSTER]);
    assert_eq!(
        1, hits,
        "the probe cannot see the very allocation it is meant to catch; this \
         control is VACUOUS"
    );
    drop(v);

    let alloc = rustty_vt::alloc::Global;
    let mut t: Terminal = Terminal::init(alloc, terminal_opts(COLS, 5)).unwrap();
    t.modes.set(modes::Mode::GraphemeCluster, true);

    // Pad the pagelist with blank rows until the cursor's page has EXACTLY one
    // free row left. The `linefeed` after the loop takes that row, so the soft
    // wrap below is forced to start a NEW page — the only way to reach the
    // `old_pin.node != new_pin.node` arm of `transfer_graphemes`.
    let mut guard = 0usize;
    loop {
        // SAFETY: stable pointers owned by the active screen. The cursor sits
        // on the bottom row, hence on the last page.
        let node = unsafe { (*(*t.screens.active).cursor.page_pin).node };
        let page = unsafe { rustty_vt::pagelist::Node::page(node) };
        let free = unsafe { (*page).capacity.rows - (*page).size.rows };
        if free == 1 {
            break;
        }
        guard += 1;
        assert!(guard < 100_000, "never reached a page boundary");
        t.carriage_return();
        t.linefeed().unwrap();
    }

    t.carriage_return();
    t.linefeed().unwrap();

    // Park the cursor on the last column with a NARROW base carrying CLUSTER
    // codepoints of grapheme data...
    for _ in 0..(COLS - 1) {
        t.print('a' as u32).unwrap();
    }
    t.print(0x261D).unwrap(); // U+261D BLACK UP POINTING INDEX (ExtPict, width 1)
    for _ in 0..(CLUSTER - 1) {
        t.print(0x0301).unwrap(); // U+0301 COMBINING ACUTE ACCENT
    }
    t.print(0x200D).unwrap(); // U+200D ZERO WIDTH JOINER

    // SAFETY: stable pointers owned by the active screen.
    let before = unsafe { (*(*t.screens.active).cursor.page_pin).node };

    // ...then make the cluster WIDE. That wraps it onto the next row, which is
    // on a different page, so the transfer has to walk the old page's grapheme
    // slice and re-append every codepoint onto the new page's cell.
    let (_, hits) = watch(|| t.print(0x1F600).unwrap()); // U+1F600 GRINNING FACE

    let after = unsafe { (*(*t.screens.active).cursor.page_pin).node };
    assert!(
        !core::ptr::eq(before, after),
        "VACUOUS: the wrap stayed on the same page, so the cross-page arm of \
         `transfer_graphemes` never ran"
    );

    assert_eq!(
        0, hits,
        "the cross-page grapheme transfer copied the old page's {CLUSTER} \
         codepoints into a fresh Vec; the reference iterates them in place and \
         allocates nothing on this path"
    );

    t.deinit(alloc);
}
