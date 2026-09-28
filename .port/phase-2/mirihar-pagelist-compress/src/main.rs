//! Isolated Miri harness for WS7 Phase B's compressed scrollback pages.
//!
//! Why this exists: `cargo +nightly miri test -p rustty-vt` must first build the
//! 2136-test `rustty-vt` test binary under Miri, which on this host has never
//! completed (see `../MIRI_EXCEPTIONS.md`, "WS7 Phase A — LZ4 block codec").
//! This harness is a plain binary that depends on the REAL `rustty-vt` crate by
//! path, so Miri interprets the production `pagelist` sources — only the lib is
//! built, not the test binary. It is not a copy: change `pagelist/mod.rs` or
//! `pagelist/grow.rs` and this harness interprets the change.
//!
//! What it proves (WS7 Phase B review items I2 and I3):
//!
//! * **I2 — the `Node` page accessors stay valid across each other.** `page`,
//!   `page_if_resident`, `page_assume_resident`, `metadata`, `storage` and
//!   `page_preserving_state` must return pointers RAW-DERIVED from the caller's
//!   `*mut Node`. A body written as `match &mut *addr_of_mut!((*node).data)`
//!   returns a pointer carrying a fresh `Unique` tag, which a later accessor
//!   call on the SAME node invalidates. `grow.rs::split` holds exactly such a
//!   pointer across a `Node::page` call inside its tracked-pin loop, so
//!   scenario `split_holds_page_across_tracked_loop` below is a direct
//!   reproduction — it FAILS with "attempting a read access ... tag does not
//!   exist in the borrow stack" if the accessors go back to being
//!   reference-derived.
//!
//! * **I3 — `CompressedPage::encoded` is not leaked.** It is a global-allocator
//!   `Vec`, so the repo's page-allocator leak gate cannot see it: deleting the
//!   discard-restore loops from `PageList::deinit`/`reset` leaves the whole
//!   native suite GREEN. Miri's leak checker runs at exit and DOES see it.
//!   `compress_reset_compress_deinit` is the scenario.
//!
//! Miri notes: `rustty-os`'s page allocator already serves page memory from the
//! std heap under `cfg(miri)`, and `mem::{decommit,recommit,can_reclaim}` take
//! their simulated arm under `cfg!(miri)` for the same reason (Miri has no FFI,
//! and the heap is not an OS mapping).
//!
//! Run (from this directory):
//!
//! ```sh
//! cargo +nightly miri run --quiet                                   # Stacked Borrows
//! MIRIFLAGS=-Zmiri-tree-borrows cargo +nightly miri run --quiet     # Tree Borrows
//! ```

use rustty_vt::page::{Capacity, Page};
use rustty_vt::size::OffsetBuf;
use rustty_vt::style::Style;
use rustty_vt::pagelist::compress::{CompressionMode, IncrementalCompressionResult};
use rustty_vt::pagelist::{Node, Storage};
use rustty_vt::point::{Coordinate, Point};
use rustty_vt::PageList;
use rustty_vt::alloc::Global;

// ---------------------------------------------------------------------------
// I3 — a counting global allocator.
//
// `CompressedPage::encoded` is a global-allocator `Vec`, which is exactly why
// the repo's page-allocator leak gate cannot see it. A `#[global_allocator]`
// can, and a binary crate is allowed to install one. This makes the I3 check
// runnable NATIVELY in milliseconds as well as under Miri, so a regression is
// demonstrable without a multi-hour interpretation.
// ---------------------------------------------------------------------------

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicIsize, Ordering};

static LIVE_BYTES: AtomicIsize = AtomicIsize::new(0);

struct Counting;

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            LIVE_BYTES.fetch_add(layout.size() as isize, Ordering::Relaxed);
        }
        p
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE_BYTES.fetch_sub(layout.size() as isize, Ordering::Relaxed);
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(layout) };
        if !p.is_null() {
            LIVE_BYTES.fetch_add(layout.size() as isize, Ordering::Relaxed);
        }
        p
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let p = unsafe { System.realloc(ptr, layout, new_size) };
        if !p.is_null() {
            LIVE_BYTES.fetch_add(new_size as isize - layout.size() as isize, Ordering::Relaxed);
        }
        p
    }
}

#[global_allocator]
static ALLOC: Counting = Counting;

fn live_bytes() -> isize {
    LIVE_BYTES.load(Ordering::Relaxed)
}


// `Node::next` is reached through the intrusive-list trait: the `next` field
// itself is crate-private (`rustty-vt/examples/rss_probe.rs` does the same).
use rustty_datastruct::intrusive_linked_list::Node as ListNode;

/// A small list with enough scrollback for several history pages. Miri is
/// ~100-400x native, so the geometry is the smallest that still produces
/// multiple complete history pages plus an active page.
fn list_with_history(pages_wanted: usize) -> PageList {
    // P3-W10 T4: `PageList::init` took `(cols, rows, max_size)` when this
    // harness was written; `P3-W3`'s runtime scrollback limits replaced that
    // with a single `Options`. The harness was not updated, so from that point
    // G6b failed to COMPILE and reported "RED: PageList leaked compressed-page
    // encodings" — a build break wearing a leak's clothes. Same geometry, same
    // 1 GiB byte budget. Parity tail S7-2 restored the reference's leading
    // `alloc` parameter; `Global` routes through this binary's global
    // allocator, so the counting wrapper still sees every byte.
    let mut list = PageList::init(Global, rustty_vt::pagelist::Options {
        max_size: Some(1 << 30),
        ..rustty_vt::pagelist::Options::new(20, 6)
    })
    .unwrap();
    // Grow until the requested number of NODES exist. A standard page holds
    // `std_capacity().rows` rows, so this is the only geometry-independent way
    // to make real history pages.
    while count_nodes(&list) < pages_wanted {
        let _ = list.grow().unwrap();
    }
    // Then a few more so the last page is partially filled and the active area
    // does not cover the whole final page.
    for _ in 0..8 {
        let _ = list.grow().unwrap();
    }
    list
}

fn count_nodes(list: &PageList) -> usize {
    let mut n = 0;
    let mut node = list.pages.first;
    while !node.is_null() {
        n += 1;
        node = unsafe { ListNode::next(&*node) };
    }
    n
}

fn count_storage(list: &PageList) -> (usize, usize) {
    let (mut resident, mut compressed) = (0, 0);
    let mut node = list.pages.first;
    while !node.is_null() {
        match unsafe { Node::storage(node) } {
            Storage::Resident => resident += 1,
            Storage::Compressed => compressed += 1,
        }
        node = unsafe { ListNode::next(&*node) };
    }
    (resident, compressed)
}

/// I2 scenario A — the exact `grow.rs::split` sequence the reviewer identified.
///
/// `split` derives `page` from `Node::page(original_node)` at the top, then runs
/// `Node::page((*tracked).node)` for every tracked pin — including, by
/// construction, the pin ON the node being split — and afterwards dereferences
/// `page` to clear rows and shorten the source. If the accessor were
/// reference-derived, the second call would invalidate the first pointer and
/// the later writes would be UB.
fn split_holds_page_across_tracked_loop() {
    let mut list = list_with_history(2);

    // A tracked pin on the page we are about to split: this is what forces the
    // interleaved `Node::page` call inside `split`'s tracked-pin loop.
    let pin = list
        .pin(Point::Active(Coordinate { x: 0, y: 1 }))
        .expect("active pin");
    let tracked = list.track_pin(pin).expect("track");

    list.split(pin).expect("split");

    // Touch the moved pin so Miri validates the pointer `split` rewrote.
    assert!(!unsafe { (*tracked).node }.is_null());

    list.deinit();
}

/// I2 scenario B — an accessor pointer held across every OTHER accessor on the
/// same node, which is the general shape of the hazard.
fn page_pointer_survives_sibling_accessors() {
    let mut list = list_with_history(2);
    let node = list.pages.first;

    let page = unsafe { Node::page(node) };

    // Every sibling accessor, each of which touches `(*node).data` again.
    let _ = unsafe { Node::metadata(node) };
    let _ = unsafe { Node::rows(node) };
    let _ = unsafe { Node::cols(node) };
    let _ = unsafe { Node::capacity(node) };
    let _ = unsafe { Node::storage(node) };
    let _ = unsafe { Node::page_if_resident(node) };
    let _ = unsafe { Node::page_assume_resident(node) };
    // `page_preserving_state` is fallible since codex-vet F-015 restored the
    // oracle's recoverable-OOM contract (`Allocator.Error!PreservedPage`,
    // PageList.zig:155-163). The harness exercises the success path. It takes
    // the reference's `alloc` since parity tail S7-3.
    let mut preserved =
        unsafe { Node::page_preserving_state(node, Global) }.expect("preserving read must not OOM here");
    preserved.deinit();
    let _ = unsafe { Node::page(node) };

    // The read AND the write must both still be in-bounds of the original tag.
    let rows = unsafe { (*page).size.rows };
    unsafe { (*page).size.rows = rows };

    list.deinit();
}

/// I2 scenario C — a compress -> restore round trip, with the pointer taken
/// before compression discarded and re-derived afterwards.
fn compress_restore_round_trip() {
    let mut list = list_with_history(3);

    let before = count_storage(&list);
    let result = list.compress(CompressionMode::Full);
    assert_ne!(
        result,
        IncrementalCompressionResult::Unsupported,
        "miri takes mem.rs's simulated reclamation arm, so compression is supported"
    );
    let after = count_storage(&list);
    assert!(
        after.1 > 0,
        "full compression must compress at least one history page \
         (before={before:?} after={after:?})"
    );

    // Find a compressed node and read its metadata WITHOUT restoring it.
    let mut node = list.pages.first;
    while !node.is_null() && unsafe { Node::storage(node) } != Storage::Compressed {
        node = unsafe { ListNode::next(&*node) };
    }
    assert!(!node.is_null(), "expected a compressed node");

    let meta_rows = unsafe { Node::rows(node) };
    assert!(unsafe { Node::page_if_resident(node) }.is_null());
    assert_eq!(unsafe { Node::storage(node) }, Storage::Compressed);

    // Preserving read: decodes into an owned buffer, node stays compressed.
    let mut preserved =
        unsafe { Node::page_preserving_state(node, Global) }.expect("preserving read must not OOM here");
    assert_eq!(unsafe { (*preserved.page()).size.rows }, meta_rows);
    preserved.deinit();
    assert_eq!(
        unsafe { Node::storage(node) },
        Storage::Compressed,
        "page_preserving_state must not restore"
    );

    // Restoring read: now the node becomes resident again, and the returned
    // pointer must be usable for a write.
    let page = unsafe { Node::page(node) };
    assert_eq!(unsafe { Node::storage(node) }, Storage::Resident);
    assert_eq!(unsafe { (*page).size.rows }, meta_rows);
    let cols = unsafe { (*page).size.cols };
    unsafe { (*page).size.cols = cols };

    list.deinit();
}

/// I3 — the leak scenario. `PageList::reset` and `PageList::deinit` each
/// discard-restore every node; that restore is what frees a compressed node's
/// `encoded` Vec. Node storage lives in a raw pool that never runs `Drop`, so
/// dropping either loop leaks the encodings. Miri's exit-time leak checker is
/// the only mechanised check in this repo that can see it.
fn compress_reset_compress_deinit() {
    let mut list = list_with_history(3);

    assert_ne!(
        list.compress(CompressionMode::Full),
        IncrementalCompressionResult::Unsupported
    );
    assert!(count_storage(&list).1 > 0, "first pass must compress");

    // reset() must free the encodings of the pages it throws away.
    list.reset();
    assert_eq!(count_storage(&list).1, 0, "reset leaves nothing compressed");

    // Grow and compress a second time so deinit() has encodings to free too.
    while count_nodes(&list) < 3 {
        let _ = list.grow().unwrap();
    }
    for _ in 0..8 {
        let _ = list.grow().unwrap();
    }
    assert_ne!(
        list.compress(CompressionMode::Full),
        IncrementalCompressionResult::Unsupported
    );
    assert!(count_storage(&list).1 > 0, "second pass must compress");

    // deinit() must free the remaining encodings.
    list.deinit();
}

/// Scenarios can be selected by name on the command line so a single expensive
/// one (notably the leak scenario) can be re-run in isolation, e.g. when
/// demonstrating that removing the discard-restore from `PageList::deinit`
/// makes Miri's leak checker fire. No arguments runs all of them.
///
/// Progress goes to STDERR: `println!` is block-buffered when stdout is a pipe,
/// which makes a long Miri run look hung.
/// I3, mechanised — run `compress_reset_compress_deinit` under the counting
/// allocator and assert the process returns to its pre-scenario live-byte
/// baseline. A first warm-up iteration absorbs one-time lazy initialisation.
///
/// Removing the discard-restore loop from `PageList::deinit` (or `reset`) makes
/// this FAIL with a positive delta equal to the retained encodings, while the
/// entire native `rustty-vt` suite stays green.
fn leak_check() {
    leak_check_of("compress -> reset -> compress -> deinit", || {
        compress_reset_compress_deinit()
    });
}

/// The shared measurement + assertion used by BOTH `leakcheck` and its negative
/// control, so the two arms differ only in the body being measured.
fn leak_check_of(what: &str, body: impl Fn()) {
    // Warm-up: pay any one-time lazy allocation before the measured run.
    body();

    let before = live_bytes();
    body();
    let after = live_bytes();

    let delta = after - before;
    assert_eq!(
        delta, 0,
        "PageList leaked {delta} global-allocator bytes across \
         {what}. The compressed pages' \
         `encoded` Vec is only freed by the discard-restore in \
         `PageList::{{deinit,reset}}`; nothing else in the suite can see this."
    );
}

/// NEGATIVE CONTROL for `leakcheck` — this scenario MUST FAIL.
///
/// `leakcheck` is only worth gating if its assertion can actually fire, and the
/// only way to make the real regression happen is to stop `PageList` from
/// discard-restoring its compressed nodes. Editing production code to prove that
/// is not something a gate can do, but skipping `deinit` entirely has exactly the
/// same observable effect on the global allocator: the compressed nodes'
/// `encoded` Vecs are never freed. `mem::forget` therefore drives the REAL leak
/// through the REAL path rather than injecting a synthetic `Box::leak`.
///
/// `.port/tools/gates/check_pagelist_compress_leaks.sh --self-test` runs this and
/// requires a non-zero exit with "leaked" in the output; that is the proof the
/// gate's FAIL arm is reachable.
fn compress_then_forget() {
    let mut list = list_with_history(3);
    assert_ne!(
        list.compress(CompressionMode::Full),
        IncrementalCompressionResult::Unsupported
    );
    assert!(count_storage(&list).1 > 0, "must compress");
    // The regression, reproduced without touching production code: no
    // discard-restore ever runs for these nodes.
    core::mem::forget(list);
}

fn leak_check_negative() {
    leak_check_of("compress -> forget (NEGATIVE CONTROL, must fail)", || {
        compress_then_forget()
    });
}

/// WS7 Phase E — the CHEAP, DECISIVE probe for the "uninit bytes in the page
/// mapping fed to LZ4" finding.
///
/// `roundtrip`/`leak` take tens of minutes under Miri because they build several
/// std-capacity pages (215x215) and then run LZ4 over every byte of each
/// mapping. But the *defect* needs neither: it is created by `Page::init_buf`,
/// so a SINGLE page at the smallest capacity that still allocates every
/// sub-table reproduces it in seconds.
///
/// This scenario allocates such a page and then reads its mapping byte by byte
/// exactly as `CompressedPage::init` does. Under Miri the read aborts at the
/// FIRST uninitialized byte and names the offset, which is what identifies the
/// write that de-initialized it. When the page mapping is fully initialized the
/// scenario prints the checksum and returns.
///
/// `PAGE_SIZE_MIN`-rounded total size means the mapping is one OS page, so the
/// scan is ~16K interpreted byte reads.
fn mapping_is_fully_initialized() {
    // The smallest capacity that still gives every sub-table a non-zero size:
    // 1 style, 1 hyperlink, and enough grapheme/string bytes to force the
    // bitmap allocators and hash maps to be laid out.
    let cap = Capacity {
        cols: 4,
        rows: 2,
        styles: 128,
        grapheme_bytes: 128,
        hyperlink_bytes: 128,
        string_bytes: 128,
    };
    let mut page = Page::init(cap).expect("page alloc");
    let ptr = page.memory_ptr();
    let len = page.memory_len();

    // The exact read `CompressedPage::init` performs. Reading it a byte at a
    // time (rather than as `[u8; 8]` like the codec) makes Miri report the
    // precise offset of the first uninitialized byte instead of the index
    // within a word.
    let mut sum: u64 = 0;
    for i in 0..len {
        // SAFETY: `ptr[0..len]` is the page's live resident mapping.
        sum = sum.wrapping_add(unsafe { *ptr.add(i) } as u64);
    }
    eprintln!("     mapping of {len} bytes fully initialized (checksum {sum})");

    page.deinit();
}

/// WS7 Phase E — the RESIDUAL probe. Same as `mapinit`, but a non-default
/// `style::Style` is stored in the page's style set first.
///
/// `RefCountedSet::add` must place the caller's `T` in the item slot with a
/// typed write. Historically that re-introduced uninitialized bytes into the
/// mapping — `style::Style` is three `style::Color` values, and `Color` was a
/// Rust enum whose `None`/`Palette` variants did not cover all four bytes of
/// their slot — so this scenario was EXPECTED to abort (at `[0x17c]`).
///
/// **FIXED in WS19 T5**, in two parts: `style::Color` is now a byte-complete
/// `#[repr(C)] { tag: u8, data: [u8; 3] }`, and `RefCountedSet::insert` writes
/// an item's `value`/`meta` fields individually instead of assigning the whole
/// `Item` (which has 1 trailing padding byte a typed write de-initializes —
/// that was the `[0x187]` abort left after the `Color` reshape). It is now the
/// PASSING regression probe for the whole uninit-in-page-mapping class; see
/// `.port/phase-2/MIRI_EXCEPTIONS.md`.
fn styled_mapping_is_fully_initialized() {
    let cap = Capacity {
        cols: 4,
        rows: 2,
        styles: 128,
        grapheme_bytes: 128,
        hyperlink_bytes: 128,
        string_bytes: 128,
    };
    let mut page = Page::init(cap).expect("page alloc");
    let base = OffsetBuf::init(page.memory_ptr());
    let mut style = Style::default();
    style.flags.bold = true;
    // SAFETY: `base` is the page's own mapping base.
    let id = unsafe { page.styles.add(base, style) }.expect("style add");
    assert_ne!(id, 0);

    let ptr = page.memory_ptr();
    let len = page.memory_len();
    let mut sum: u64 = 0;
    for i in 0..len {
        // SAFETY: `ptr[0..len]` is the page's live resident mapping.
        sum = sum.wrapping_add(unsafe { *ptr.add(i) } as u64);
    }
    eprintln!("     styled mapping of {len} bytes fully initialized (checksum {sum})");

    page.deinit();
}

fn main() {
    let selected: Vec<String> = std::env::args().skip(1).collect();
    let want = |name: &str| selected.is_empty() || selected.iter().any(|s| s == name);

    if want("mapinit") {
        mapping_is_fully_initialized();
        eprintln!("ok  mapping_is_fully_initialized");
    }
    if want("mapinit_styled") {
        styled_mapping_is_fully_initialized();
        eprintln!("ok  styled_mapping_is_fully_initialized");
    }

    if want("split") {
        split_holds_page_across_tracked_loop();
        eprintln!("ok  split_holds_page_across_tracked_loop");
    }
    if want("siblings") {
        page_pointer_survives_sibling_accessors();
        eprintln!("ok  page_pointer_survives_sibling_accessors");
    }
    if want("roundtrip") {
        compress_restore_round_trip();
        eprintln!("ok  compress_restore_round_trip");
    }
    if want("leak") {
        compress_reset_compress_deinit();
        eprintln!("ok  compress_reset_compress_deinit");
    }
    if want("leakcheck") {
        leak_check();
        eprintln!("ok  leak_check (counting global allocator, delta == 0)");
    }
    // Selected explicitly only (never by the no-argument "run everything"
    // default): this scenario is EXPECTED to panic.
    if selected.iter().any(|s| s == "leakcheck_negative") {
        leak_check_negative();
        eprintln!("BUG: leak_check_negative did NOT fail");
        std::process::exit(1);
    }
    eprintln!("all selected scenarios passed (leak check runs at exit)");
}

