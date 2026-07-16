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
//! hyperlink_set 152, hyperlink_map 464):
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
        layout.hyperlink_map_layout.total_size, 464,
        "hyperlink_map section drifted"
    );
    assert_eq!(
        layout.total_size, 409600,
        "std_size drifted from the pinned port geometry 409600 (25 x 16 KiB; \
         Zig oracle: 589824 — see module doc for the accounted difference)"
    );
}
