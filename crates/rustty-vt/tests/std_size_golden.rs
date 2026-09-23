//! std_size golden gate — pagelist page-geometry drift detection.
//!
//! This is an INTEGRATION test on purpose: the library is compiled WITHOUT
//! `cfg(test)`, so `std_capacity()` returns the PRODUCTION capacity
//! (`grapheme_bytes = 8192` — the crate's own unit tests see the reference's
//! `builtin.is_test` value 512 instead and could never observe this number).
//!
//! ## Oracle vs port geometry (re-measured 2026-09-22, reference `20abdb50a`)
//!
//! Every section and the total equal the reference's, on aarch64-macos
//! (`page_size_min` 16 KiB) and on aarch64-linux (4 KiB). The reference's
//! numbers were measured by a Zig probe appended to a scratch copy of
//! `page.zig` (`zig build test-lib-vt`, Zig 0.16.0; cross-compiled with
//! `-Dtarget=aarch64-linux-gnu` and run on Linux for the 4 KiB figures):
//!
//! | section        | reference = port | note |
//! |----------------|-----------------:|------|
//! | rows           |   1720 | |
//! | cells          | 369800 | |
//! | styles         |   4000 | 104 items at the reference's 36-byte `Item` (NEW-FINDINGS N88) |
//! | grapheme_alloc |   8256 | |
//! | grapheme_map   |  10768 | |
//! | string_alloc   |   2056 | |
//! | hyperlink_set  |    152 | |
//! | hyperlink_map  |    912 | |
//! | total, 16 KiB  | 409600 | 25 x 16 KiB |
//! | total, 4 KiB   | 401408 | 98 x 4 KiB |
//!
//! CORRECTION (2026-09-22, N88): this doc used to give the oracle as 589,824
//! bytes, with grapheme_alloc 131,136 and string_alloc 65,544. Those were an
//! older pinned oracle's, before it had upstream `b953bb346` (the
//! `BitmapAllocator.layout` chunk sizing); the pin `20abdb50a` contains that
//! commit and gives 409,600, 8,256 and 2,056, the same as the port.
//!
//! CORRECTION (2026-09-22, N88): the styles section used to be 3,168 bytes in
//! the port against the reference's 4,000, because the port's style-set
//! `Item` is 28 bytes / align 2 (its `Style` is 21/1) against the reference's
//! 36 / align 4 (`Style` 28/4). This doc called that "internal repr
//! difference, not observable" and "Geometry is NOT observable behavior".
//! Both were wrong. The styles section feeds `availableBitsForGrid`, so the
//! rows of every page, `maxCols` and the page split points differed (80
//! columns: 593 rows against 591 shipped on macOS; on Linux the port's
//! standard page was 397,312 bytes, 4 KiB smaller, and held 574 rows against
//! 579), and with them the lines kept under `scrollback-limit`, snapshot page
//! boundaries and grid refs. The style set is now laid out with the
//! reference's item geometry (`ref_counted_set::RefItemGeometry`), and the
//! rows per page are pinned against the reference below and in
//! `page_geometry_matches_the_reference` (`page_tests.rs`).
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
//! Geometry is observable (see the correction above) and it is also
//! allocation-comparison-relevant: equal-scope alloc benches vs the Zig
//! oracle must use the same std page. Documented in
//! .port/phase-2/ALLOC_BENCH_V2.md.
//!
//! This test pins the reference's values so any FUTURE drift (cell/row
//! struct changes, RefCountedSet geometry, chunk sizing, ...) is caught
//! structurally. If it fires, diff `Page::layout` section-by-section against
//! page.zig:1700-1760 before touching the golden value.

/// Rows per page of `std_capacity.adjust(.{ .cols = c })` and `maxCols` in a
/// shipped (non-test) build, as the reference's Zig probe measured them.
#[cfg(all(target_arch = "aarch64", any(target_os = "macos", target_os = "linux")))]
fn assert_rows_per_page(rows: &[(u16, u16)], max_cols: u16) {
    use rustty_vt::page::{std_capacity, Adjustment};
    let cap = std_capacity();
    for &(cols, want) in rows {
        let got = cap.adjust(Adjustment { cols: Some(cols) }).unwrap().rows;
        assert_eq!(
            want, got,
            "rows per page at {cols} columns (reference: {want})"
        );
    }
    assert_eq!(
        Some(max_cols),
        cap.max_cols(),
        "maxCols (reference: {max_cols})"
    );
}

/// `Page.layout(std_capacity).total_size` on aarch64-macos, section by
/// section: the reference's geometry (see the module doc).
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
        layout.styles_layout.total_size, 4000,
        "styles section drifted (RefCountedSet item geometry, N88)"
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
    //
    // CLOSED (2026-09-22, N88): every section below was measured in the
    // reference at `20abdb50a` by a Zig probe (`Page.layout(std_capacity)`,
    // grapheme budget 8,192: grapheme_map 10768, string_alloc 2056,
    // hyperlink_set 152, hyperlink_map 912, total 409600). These goldens are
    // now byte-identical to the reference, not only "no further drift".
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
        "std_size drifted from the reference's 409600 (25 x 16 KiB)"
    );

    assert_rows_per_page(
        &[
            (1, 23966),
            (80, 591),
            (119, 399),
            (120, 396),
            (200, 238),
            (1000, 47),
        ],
        47931,
    );
}

/// `Page.layout(std_capacity).total_size` on aarch64-linux (4 KiB
/// `page_size_min`): the reference's 401,408 bytes (98 x 4 KiB). Before N88
/// the port's page was 397,312 bytes here, 4 KiB smaller, holding fewer rows.
#[cfg(all(target_arch = "aarch64", target_os = "linux"))]
#[test]
fn std_size_golden_aarch64_linux() {
    use rustty_vt::page::{std_capacity, Page};

    let cap = std_capacity();
    assert_eq!(
        cap.grapheme_bytes, 8192,
        "production grapheme_bytes drifted (is this a cfg(test) library build?)"
    );
    let layout = Page::layout(cap);
    assert_eq!(layout.rows_size, 1720, "rows section drifted (Row size)");
    assert_eq!(
        layout.cells_size, 369800,
        "cells section drifted (Cell size)"
    );
    assert_eq!(
        layout.styles_layout.total_size, 4000,
        "styles section drifted (RefCountedSet item geometry, N88)"
    );
    assert_eq!(
        layout.grapheme_alloc_layout.total_size, 8256,
        "grapheme_alloc section drifted"
    );
    assert_eq!(
        layout.grapheme_map_layout.total_size, 10768,
        "grapheme_map section drifted"
    );
    assert_eq!(
        layout.string_alloc_layout.total_size, 2056,
        "string_alloc section drifted"
    );
    assert_eq!(
        layout.hyperlink_set_layout.total_size, 152,
        "hyperlink_set section drifted"
    );
    assert_eq!(
        layout.hyperlink_map_layout.total_size, 912,
        "hyperlink_map section drifted"
    );
    assert_eq!(
        layout.total_size, 401408,
        "std_size drifted from the reference's 401408 (98 x 4 KiB)"
    );

    assert_rows_per_page(
        &[
            (1, 23454),
            (80, 579),
            (119, 390),
            (120, 387),
            (200, 233),
            (1000, 46),
        ],
        46907,
    );
}
