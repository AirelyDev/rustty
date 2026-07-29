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

use rustty_vt::pagelist::compress::{CompressionMode, IncrementalCompressionResult};
use rustty_vt::pagelist::{Node, Storage};
use rustty_vt::point::{Coordinate, Point};
use rustty_vt::PageList;

// `Node::next` is reached through the intrusive-list trait: the `next` field
// itself is crate-private (`rustty-vt/examples/rss_probe.rs` does the same).
use rustty_datastruct::intrusive_linked_list::Node as ListNode;

/// A small list with enough scrollback for several history pages. Miri is
/// ~100-400x native, so the geometry is the smallest that still produces
/// multiple complete history pages plus an active page.
fn list_with_history(pages_wanted: usize) -> PageList {
    let mut list = PageList::init(20, 6, Some(1 << 30)).unwrap();
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
    let mut preserved = unsafe { Node::page_preserving_state(node) };
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
    let mut preserved = unsafe { Node::page_preserving_state(node) };
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
fn main() {
    let selected: Vec<String> = std::env::args().skip(1).collect();
    let want = |name: &str| selected.is_empty() || selected.iter().any(|s| s == name);

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
    eprintln!("all selected scenarios passed (leak check runs at exit)");
}

