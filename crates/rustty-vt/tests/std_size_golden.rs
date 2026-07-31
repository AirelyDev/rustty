//! std_size golden gate — pagelist page-geometry drift detection.
//!
//! This is an INTEGRATION test on purpose: the library is compiled WITHOUT
//! `cfg(test)`, so `std_capacity()` returns the PRODUCTION capacity
//! (`grapheme_bytes = 8192` — the crate's own unit tests see the reference's
//! `builtin.is_test` value 512 instead and could never observe this number).
//!
//! ## Oracle vs port geometry (MEASURED 2026-07-16, aarch64-macos)
//!
//! The Zig reference bench (.port/phase-1/bench-zig-pagelist, RESULTS.txt +
//! probe re-run against the pinned oracle) measures the production standard
//! page size as
//!
//!   oracle std_size = 589824 bytes (= 36 x 16 KiB)
//!
//! The port's production std_size is **409600 bytes (= 25 x 16 KiB)**. The
//! difference decomposes EXACTLY into three sections (all other sections are
//! byte-identical — rows 1720, cells 369800, grapheme_map 10768,
//! hyperlink_set 152, hyperlink_map 912):
//!
//! NOTE (2026-07-30, `P3-W2` T2/T3): the two hash-map-backed sections moved
//! twice today, in opposite directions, and both moves are the REFERENCE's own
//! layout — not port drift:
//!
//! 1. WS10 T11 ported `7e14347c1`, which added `available: Size` to the
//!    hash-map `Header` and gave `hyperlink::Map` the 80% max_load_percentage:
//!    grapheme_map 10768 -> 10776 (+8), hyperlink_map 464 -> 916 (+452).
//! 2. `P3-W2` T3 ported `fedd42e8d`, which replaces tombstones with
//!    backward-shift deletion and therefore **deletes `Header.available`
//!    again**: grapheme_map 10776 -> 10768 (-8), hyperlink_map 916 -> 912 (-4).
//!
//! The two deltas differ because the two maps have differently-aligned headers.
//! `hyperlink::Map` is `AutoOffsetHashMap<Offset<Cell>, Id, 80>`: every `Header`
//! field is 4-byte (`Offset` is a `u32`, `Size` is a `u32`), so dropping one
//! `Size` field shrinks the header by exactly 4. `GraphemeMap` is
//! `AutoOffsetHashMap<Offset<Cell>, OffsetSlice<u32>, 100>`, whose value type
//! contains a `usize` and so forces `align_of::<Header<_, _>>() == 8`: the
//! 5-field header was 20 bytes padded to 24, the 4-field header is 16, so it
//! shrinks by 8.
//!
//! The **80% load factor survives** `fedd42e8d` — that is why hyperlink_map
//! settles at 912 rather than returning to its pre-`7e14347c1` 464. If a future
//! change puts it back near 464, the load factor has been lost, not the header.
//! `std_size` itself is unchanged at 409600 across both moves: `Page::layout`
//! rounds the total up to `page_size_min` (16 KiB) and these tens of bytes are
//! absorbed by that rounding. That is also why this golden's `total_size` assert
//! alone would NOT have caught either move — the section-by-section asserts are
//! the ones doing the work.
//!
//! | section        | oracle | port  | why |
//! |----------------|-------:|------:|-----|
//! | styles         |   4000 |  3168 | Zig `Style` is 28 B / align 4; the Rust `Style` is 21 B / align 1, so the RefCountedSet `Item` is 28 B vs Zig's 36 B (same table, same 104-item cap). Internal repr difference, not observable through the style API. |
//! | grapheme_alloc | 131136 |  8256 | `BitmapAllocator.layout` — the port carries upstream ghostty fix `b953bb346` (chunks region sized `aligned_chunk_count * chunk_size`); the PINNED oracle predates it and over-reserves `aligned_cap * chunk_size` (~123 KiB dead space). See bitmap_allocator.rs "PORT NOTE (parity b953bb346)". |
//! | string_alloc   |  65544 |  2056 | Same `b953bb346` fix (~62 KiB dead space in the oracle). |
//!
//! Geometry is NOT observable behavior (R1 covers observable I/O; every page
//! is self-consistent), but it IS allocation-comparison-relevant: equal-scope
//! alloc benches vs the Zig oracle must account for the smaller std page
//! (more std pages per row count, smaller pool items). Documented in
//! .port/phase-2/ALLOC_BENCH_V2.md.
//!
//! This test pins the PORT's current value so any FUTURE drift (cell/row
//! struct changes, RefCountedSet padding, chunk sizing, ...) is caught
//! structurally. If it fires, diff `Page::layout` section-by-section against
//! page.zig:1700-1760 before touching the golden value.

/// `Page.layout(std_capacity).total_size` — pinned port geometry on
/// aarch64-macos (oracle: 589824; see the module doc for the delta).
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn std_size_golden_aarch64_macos() {
    use rustty_vt::page::{std_capacity, Page};

    let cap = std_capacity();
    // The production std_capacity the oracle measured (page.zig:1780-1787).
    assert_eq!(cap.cols, 215, "std_capacity.cols drifted");
    assert_eq!(cap.rows, 215, "std_capacity.rows drifted");
    assert_eq!(cap.styles, 128, "std_capacity.styles drifted");
    assert_eq!(
        cap.grapheme_bytes, 8192,
        "production grapheme_bytes drifted (is this a cfg(test) library build?)"
    );

    let layout = Page::layout(cap);
    // Section-by-section pins so a drift names its culprit directly.
    assert_eq!(layout.rows_size, 1720, "rows section drifted (Row size)");
    assert_eq!(
        layout.cells_size, 369800,
        "cells section drifted (Cell size)"
    );
    assert_eq!(
        layout.styles_layout.total_size, 3168,
        "styles section drifted (Style/Item/RefCountedSet layout)"
    );
    assert_eq!(
        layout.grapheme_alloc_layout.total_size, 8256,
        "grapheme_alloc section drifted (BitmapAllocator layout)"
    );
    // 10776 -> 10768 (-8) when `P3-W2` T3 ported oracle `fedd42e8d`, which
    // deletes `Header.available` along with the tombstone headroom counter it
    // served (verified: `git -C ../ghostty show fedd42e8d:src/terminal/hash_map.zig`
    // — the Header is `values, keys, capacity, size`). This exactly undoes the
    // +8 that `7e14347c1` added one commit earlier; see the module doc for why
    // this map moves by 8 while the hyperlink map moves by 4. The reference's
    // own layout moving — not a rustty-side regression. The golden is re-pinned
    // rather than relaxed: it still fails on any further drift.
    //
    // VERIFICATION LIMIT (be honest about this): the CAUSE is confirmed against
    // the oracle, but the exact byte counts below are re-pinned from rustty's own
    // `Page::layout` rather than independently derived from the oracle's
    // `page.zig:785-790` formula. So these goldens prove "no further drift", not
    // "byte-identical to the reference". Cross-validating the numbers against an
    // oracle-computed layout is tracked as a Phase-3 follow-up.
    assert_eq!(
        layout.grapheme_map_layout.total_size, 10768,
        "grapheme_map section drifted"
    );
    assert_eq!(
        layout.string_alloc_layout.total_size, 2056,
        "string_alloc section drifted (BitmapAllocator layout)"
    );
    assert_eq!(
        layout.hyperlink_set_layout.total_size, 152,
        "hyperlink_set section drifted"
    );
    assert_eq!(
        // 464 -> 916 when WS10 T11 gave `hyperlink::Map` the reference's 80%
        // max_load_percentage (oracle `hyperlink.zig:23`:
        // `AutoOffsetHashMap(Offset(Cell), Id, 80)`), which raises the capacity
        // needed for the same entry count; then 916 -> 912 (-4) when `P3-W2` T3
        // ported `fedd42e8d` and deleted `Header.available`. The load factor is
        // what holds this at 912 instead of 464 — a regression to ~464 means the
        // 80% has been lost, which is separately pinned by
        // `page_hyperlinkCapacity_reports_max_load_not_raw_capacity`. Upstream's
        // own design; see the VERIFICATION LIMIT note above.
        layout.hyperlink_map_layout.total_size,
        912,
        "hyperlink_map section drifted"
    );
    assert_eq!(
        layout.total_size, 409600,
        "std_size drifted from the pinned port geometry 409600 (25 x 16 KiB; \
         Zig oracle: 589824 — see module doc for the accounted difference)"
    );
}
