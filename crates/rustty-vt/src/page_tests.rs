// PORT NOTE: this file is `include!`d into `page.rs`. It holds the ported
// `test {}` blocks (assertion-for-assertion) plus explicit byte-layout tests for
// the packed `Cell`/`Row` structs (the byte-layout-critical assertions the
// constitution mandates for this sub-batch).

#[cfg(test)]
#[allow(non_snake_case)]
// PORT NOTE: these ported tests reproduce the reference's exact loop and range
// forms verbatim (e.g. `for (0..count) |i| { ... cps[i] }` and
// `if (x >= 2 and x < 8)`); we keep them char-for-char rather than rewriting to
// `.iter().enumerate()` / `Range::contains`, so the corresponding style lints
// are allowed at the module level.
#[allow(clippy::needless_range_loop)]
#[allow(clippy::manual_range_contains)]
mod tests {
    use super::*;

    // PORT NOTE: the reference has two commented-out helper tests just before the
    // live tests. They are reproduced verbatim here (kept commented, as in the
    // reference) for fidelity:
    //
    // Uncomment this when you want to do some math.
    // test "Page size calculator" {
    //     const total_size = alignForward(
    //         usize,
    //         Page.layout(.{
    //             .cols = 250,
    //             .rows = 250,
    //             .styles = 128,
    //             .grapheme_bytes = 1024,
    //         }).total_size,
    //         std.heap.page_size_min,
    //     );
    //
    //     std.log.warn("total_size={} pages={}", .{
    //         total_size,
    //         total_size / std.heap.page_size_min,
    //     });
    // }
    //
    // test "Page std size" {
    //     // We want to ensure that the standard capacity is what we
    //     // expect it to be. Changing this is fine but should be done with care
    //     // so we fail a test if it changes.
    //     const total_size = Page.layout(std_capacity).total_size;
    //     try testing.expectEqual(@as(usize, 524_288), total_size); // 512 KiB
    //     //const pages = total_size / std.heap.page_size_min;
    // }

    // Helper mirroring the reference cell-literal `.{ .content_tag = .codepoint,
    // .content = .{ .codepoint = cp } }`.
    fn cell_codepoint(cp: u21) -> Cell {
        let mut c = Cell::zeroed();
        c.set_content_tag(ContentTag::Codepoint);
        c.set_content_codepoint(cp);
        c
    }

    // =====================================================================
    // Byte-layout-critical tests (constitution mandate for sub-batch 2b).
    // =====================================================================

    #[test]
    fn cell_layout_size_align() {
        // The reference is `Cell = packed struct(u64)`: exactly 8 bytes, 8-align.
        assert_eq!(8, core::mem::size_of::<Cell>());
        assert_eq!(8, core::mem::align_of::<Cell>());
    }

    #[test]
    fn row_layout_size_align() {
        // The reference is `Row = packed struct(u64)`: exactly 8 bytes, 8-align.
        assert_eq!(8, core::mem::size_of::<Row>());
        assert_eq!(8, core::mem::align_of::<Row>());
    }

    #[test]
    fn cell_field_bit_offsets() {
        // content_tag occupies bits 0..1.
        let mut c = Cell::from_bits(0);
        c.set_content_tag(ContentTag::BgColorRgb); // = 3 = 0b11
        assert_eq!(0b11, c.to_bits() & 0b11);
        assert_eq!(c.to_bits(), 0b11);

        // content (codepoint) occupies bits 2..22 (low 21 bits of the 24-bit field).
        let mut c = Cell::from_bits(0);
        c.set_content_codepoint(0x1FFFFF); // 21 bits all set
        assert_eq!(0x1FFFFF << CELL_CONTENT_SHIFT, c.to_bits());

        // color_rgb occupies the full 24-bit content field.
        let mut c = Cell::from_bits(0);
        c.set_content_color_rgb(CellRgb {
            r: 0xAA,
            g: 0xBB,
            b: 0xCC,
        });
        assert_eq!(0xCCBBAAu64 << CELL_CONTENT_SHIFT, c.to_bits());

        // style_id occupies bits 26..41 (16 bits).
        let mut c = Cell::from_bits(0);
        c.set_style_id(0xFFFF);
        assert_eq!(0xFFFFu64 << CELL_STYLE_ID_SHIFT, c.to_bits());
        assert_eq!(26, CELL_STYLE_ID_SHIFT);

        // wide occupies bits 42..43.
        let mut c = Cell::from_bits(0);
        c.set_wide(Wide::SpacerHead); // = 3
        assert_eq!(0b11u64 << CELL_WIDE_SHIFT, c.to_bits());
        assert_eq!(42, CELL_WIDE_SHIFT);

        // protected bit 44.
        let mut c = Cell::from_bits(0);
        c.set_protected(true);
        assert_eq!(1u64 << 44, c.to_bits());

        // hyperlink bit 45.
        let mut c = Cell::from_bits(0);
        c.set_hyperlink(true);
        assert_eq!(1u64 << 45, c.to_bits());

        // semantic_content bits 46..47.
        let mut c = Cell::from_bits(0);
        c.set_semantic_content(SemanticContent::Prompt); // = 2
        assert_eq!(2u64 << 46, c.to_bits());

        // _padding occupies bits 48..63 (16 bits). The accessors never touch it.
        assert_eq!(48, CELL_PADDING_SHIFT);
        assert_eq!(16, CELL_PADDING_BITS);
        // Total accounted bits == 64.
        assert_eq!(
            64,
            CELL_PADDING_SHIFT + CELL_PADDING_BITS
        );
    }

    // PERF(port) gate: `Cell::copy_with_managed_reset` is a masked-store form
    // of the three sequential managed-memory resets in `clone_partial_row_from`
    // (page.zig:905-909). Prove bit-for-bit equivalence with the sequential
    // setter form on a corner lattice plus seeded-random u64 bit patterns
    // (the setters are pure bit-ops, valid on arbitrary bits, so we can probe
    // padding/extreme patterns directly).
    #[test]
    fn cell_copy_with_managed_reset_equivalence() {
        fn sequential_reset(mut c: Cell) -> Cell {
            c.set_hyperlink(false);
            c.set_style_id(stylepkg::DEFAULT_ID);
            if c.content_tag() == ContentTag::CodepointGrapheme {
                c.set_content_tag(ContentTag::Codepoint);
            }
            c
        }
        fn check(bits: u64) {
            assert_eq!(
                Cell::from_bits(bits).copy_with_managed_reset().to_bits(),
                sequential_reset(Cell::from_bits(bits)).to_bits(),
                "bits = {bits:#018x}"
            );
        }

        // Corner lattice: all 4 content tags x hyperlink 0/1 x style_id
        // {0, 1, 0xFFFF} x wide/protected/semantic extremes x padding bits.
        for tag in 0..4u64 {
            for hyperlink in 0..2u64 {
                for style_id in [0u64, 1, 0xFFFF] {
                    for wide in [0u64, 0b11] {
                        for protected in 0..2u64 {
                            for semantic in [0u64, 0b10] {
                                for padding in [0u64, 0xFFFF] {
                                    for content in [0u64, 0x1F_FFFF, 0xFF_FFFF] {
                                        let bits = (tag << CELL_CONTENT_TAG_SHIFT)
                                            | (content << CELL_CONTENT_SHIFT)
                                            | (style_id << CELL_STYLE_ID_SHIFT)
                                            | (wide << CELL_WIDE_SHIFT)
                                            | (protected << CELL_PROTECTED_SHIFT)
                                            | (hyperlink << CELL_HYPERLINK_SHIFT)
                                            | (semantic << CELL_SEMANTIC_CONTENT_SHIFT)
                                            | (padding << CELL_PADDING_SHIFT);
                                        check(bits);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // ~10k seeded-random u64 bit patterns (splitmix64, fixed seed).
        let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
        for _ in 0..10_000 {
            state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = state;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^= z >> 31;
            check(z);
        }
    }

    // PERF(port) gate: `Cell::is_style_run_cell` guards the different-page
    // style-run bulk copy in `clone_partial_row_from_inner`. Prove, on the
    // same corner lattice + seeded-random patterns as the reset-equivalence
    // gate:
    //   (1) the predicate is EXACTLY "neither the grapheme arm nor the
    //       hyperlink arm of the reference managed-copy loop
    //       (page.zig:895-984) fires";
    //   (2) for every such cell, `copy_with_managed_reset` followed by
    //       `set_style_id(style_id)` reproduces the cell verbatim — so a
    //       run-mate cell whose per-cell handling would be
    //       `dst = reset(src); dst.set_style_id(src.style_id())` can be
    //       bulk-copied;
    //   (3) in particular with `style_id == DEFAULT_ID` the reset alone is
    //       the identity (the plain-run special case).
    #[test]
    fn cell_style_run_facts() {
        fn check(bits: u64) {
            let c = Cell::from_bits(bits);
            // (1) predicate == negation of the two managed arm guards.
            assert_eq!(
                c.is_style_run_cell(),
                !(c.has_grapheme() || c.hyperlink()),
                "predicate mismatch, bits = {bits:#018x}"
            );
            if c.is_style_run_cell() {
                // (2) reset-then-restore-style-id is the identity.
                let mut d = c.copy_with_managed_reset();
                d.set_style_id(c.style_id());
                assert_eq!(
                    d.to_bits(),
                    bits,
                    "reset+restore not identity on run cell, bits = {bits:#018x}"
                );
                // (3) with the default style id the reset alone is identity.
                if c.style_id() == stylepkg::DEFAULT_ID {
                    assert_eq!(
                        c.copy_with_managed_reset().to_bits(),
                        bits,
                        "reset not identity on plain run cell, bits = {bits:#018x}"
                    );
                }
            }
        }

        // Corner lattice (same axes as the reset-equivalence gate, plus the
        // kitty placeholder codepoint).
        for tag in 0..4u64 {
            for hyperlink in 0..2u64 {
                for style_id in [0u64, 1, 0xFFFF] {
                    for wide in [0u64, 0b11] {
                        for protected in 0..2u64 {
                            for semantic in [0u64, 0b10] {
                                for padding in [0u64, 0xFFFF] {
                                    for content in [
                                        0u64,
                                        KITTY_PLACEHOLDER as u64,
                                        0x1F_FFFF,
                                        0xFF_FFFF,
                                    ] {
                                        let bits = (tag << CELL_CONTENT_TAG_SHIFT)
                                            | (content << CELL_CONTENT_SHIFT)
                                            | (style_id << CELL_STYLE_ID_SHIFT)
                                            | (wide << CELL_WIDE_SHIFT)
                                            | (protected << CELL_PROTECTED_SHIFT)
                                            | (hyperlink << CELL_HYPERLINK_SHIFT)
                                            | (semantic << CELL_SEMANTIC_CONTENT_SHIFT)
                                            | (padding << CELL_PADDING_SHIFT);
                                        check(bits);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // ~10k seeded-random u64 bit patterns (splitmix64, distinct seed from
        // the reset gate).
        let mut state: u64 = 0x0DDB_1A5E_5BAD_5EED;
        for _ in 0..10_000 {
            state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = state;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^= z >> 31;
            check(z);
        }
    }

    #[test]
    fn row_field_bit_offsets() {
        // cells offset occupies bits 0..31.
        let mut r = Row::from_bits(0);
        r.set_cells(Offset::new(0xDEADBEEF));
        assert_eq!(0xDEADBEEFu64, r.to_bits());

        // wrap bit 32.
        let mut r = Row::from_bits(0);
        r.set_wrap(true);
        assert_eq!(1u64 << 32, r.to_bits());

        // wrap_continuation bit 33.
        let mut r = Row::from_bits(0);
        r.set_wrap_continuation(true);
        assert_eq!(1u64 << 33, r.to_bits());

        // grapheme bit 34.
        let mut r = Row::from_bits(0);
        r.set_grapheme(true);
        assert_eq!(1u64 << 34, r.to_bits());

        // styled bit 35.
        let mut r = Row::from_bits(0);
        r.set_styled(true);
        assert_eq!(1u64 << 35, r.to_bits());

        // hyperlink bit 36.
        let mut r = Row::from_bits(0);
        r.set_hyperlink(true);
        assert_eq!(1u64 << 36, r.to_bits());

        // semantic_prompt bits 37..38.
        let mut r = Row::from_bits(0);
        r.set_semantic_prompt(SemanticPrompt::PromptContinuation); // = 2
        assert_eq!(2u64 << 37, r.to_bits());

        // kitty_virtual_placeholder bit 39.
        let mut r = Row::from_bits(0);
        r.set_kitty_virtual_placeholder(true);
        assert_eq!(1u64 << 39, r.to_bits());

        // dirty bit 40.
        let mut r = Row::from_bits(0);
        r.set_dirty(true);
        assert_eq!(1u64 << 40, r.to_bits());

        // _padding occupies bits 41..63 (23 bits).
        assert_eq!(41, ROW_PADDING_SHIFT);
        assert_eq!(23, ROW_PADDING_BITS);
        assert_eq!(64, ROW_PADDING_SHIFT + ROW_PADDING_BITS);
    }

    #[test]
    fn cell_rgb_layout() {
        // Cell.RGB = packed struct { r,g,b: u8 } = 24 bits.
        assert_eq!(3, core::mem::size_of::<CellRgb>());
    }

    #[test]
    fn enum_discriminants() {
        // ContentTag enum(u2): codepoint=0, codepoint_grapheme=1,
        // bg_color_palette=2, bg_color_rgb=3.
        assert_eq!(0, ContentTag::Codepoint as u8);
        assert_eq!(1, ContentTag::CodepointGrapheme as u8);
        assert_eq!(2, ContentTag::BgColorPalette as u8);
        assert_eq!(3, ContentTag::BgColorRgb as u8);

        // Wide enum(u2): narrow=0, wide=1, spacer_tail=2, spacer_head=3.
        assert_eq!(0, Wide::Narrow as u8);
        assert_eq!(1, Wide::Wide as u8);
        assert_eq!(2, Wide::SpacerTail as u8);
        assert_eq!(3, Wide::SpacerHead as u8);

        // SemanticContent enum(u2): output=0, input=1, prompt=2.
        assert_eq!(0, SemanticContent::Output as u8);
        assert_eq!(1, SemanticContent::Input as u8);
        assert_eq!(2, SemanticContent::Prompt as u8);

        // Row.SemanticPrompt enum(u2): none=0, prompt=1, prompt_continuation=2.
        assert_eq!(0, SemanticPrompt::None as u8);
        assert_eq!(1, SemanticPrompt::Prompt as u8);
        assert_eq!(2, SemanticPrompt::PromptContinuation as u8);
    }

    #[test]
    fn page_alignment_invariant() {
        // The reference `comptime` block on Page:
        //   assert(page_size_min % @max(@alignOf(Row), cells_align,
        //          MetaLayout.alignment) == 0);
        let m = core::mem::align_of::<Row>()
            .max(CELLS_ALIGN)
            .max(MetaLayout::ALIGNMENT);
        assert_eq!(0, PAGE_SIZE_MIN % m);
    }

    #[test]
    fn cells_align_is_the_reference_cache_line() {
        // `cells_align = @max(@alignOf(Cell), @min(std.atomic.cache_line,
        // std.heap.page_size_min))`; the cache line is 128 on aarch64 and
        // x86_64 (Zig 0.16.0 `std.atomic.cacheLineForCpu`).
        if cfg!(any(target_arch = "aarch64", target_arch = "x86_64")) {
            assert_eq!(128, CELLS_ALIGN);
        }
        assert_eq!(
            core::mem::align_of::<Cell>()
                .max(rustty_zigstd::atomic::CACHE_LINE.min(PAGE_SIZE_MIN)),
            CELLS_ALIGN
        );
        // `MetaLayout.alignment`: the largest member base alignment (8 on
        // 64-bit targets, from the style set's and grapheme map's headers).
        assert_eq!(
            StyleSet::base_align()
                .max(GraphemeAlloc::BASE_ALIGN)
                .max(GraphemeMap::base_align())
                .max(StringAlloc::BASE_ALIGN)
                .max(hyperlink::Set::base_align())
                .max(hyperlink::Map::base_align()),
            MetaLayout::ALIGNMENT
        );
        // Every page's cells start on a cache line.
        for cap in [std_capacity(), Capacity::new(1, 1), Capacity::new(80, 591), Capacity::new(3, 7)] {
            assert_eq!(0, Page::layout(cap).cells_start % CELLS_ALIGN);
        }
    }

    #[test]
    fn page_size_min_matches_reference_for_target() {
        // PARITY (audit #4): `PAGE_SIZE_MIN` must equal the reference's comptime
        // `std.heap.page_size_min` for THIS build target, not a hardcoded 4096.
        // The reference selects it from `std.heap.page_size_min_default`
        // (lib/std/heap.zig): aarch64 Apple => 16 KiB, x86_64 Apple => 4 KiB,
        // Linux (x86_64/aarch64) => 4 KiB.
        let expected: usize = if cfg!(all(target_os = "macos", target_arch = "aarch64"))
            || cfg!(all(target_os = "ios", target_arch = "aarch64"))
        {
            16 << 10
        } else {
            // macOS x86_64 and Linux x86_64/aarch64 (the other targets we build).
            4 << 10
        };
        assert_eq!(expected, PAGE_SIZE_MIN);
    }

    #[test]
    fn page_layout_total_size_aligned_to_target_page_size_min() {
        // PARITY (audit #4): `Page.layout` aligns `total_size` up to
        // `page_size_min` via `alignForward(usize, hyperlink_map_end,
        // page_size_min)`. So `total_size` must be an exact multiple of the
        // per-target `PAGE_SIZE_MIN`. On aarch64-macos this is 16384, which a
        // hardcoded-4096 port would get wrong for non-standard page sizes.
        let std_cap = std_capacity();
        let std_size = Page::layout(std_cap).total_size;
        assert_eq!(
            0,
            std_size % PAGE_SIZE_MIN,
            "std page total_size {std_size} not a multiple of PAGE_SIZE_MIN {PAGE_SIZE_MIN}",
        );

        // A reduced (non-standard) capacity: shrink cols hard so the layout is
        // small enough that the page_size_min rounding is observable. Its
        // total_size must also be aligned to PAGE_SIZE_MIN, and must be the
        // SMALLEST such multiple that still contains the unaligned end (i.e.
        // subtracting one PAGE_SIZE_MIN would not fit it).
        let reduced = std_cap.adjust(Adjustment { cols: Some(1) }).unwrap();
        let reduced_size = Page::layout(reduced).total_size;
        assert_eq!(
            0,
            reduced_size % PAGE_SIZE_MIN,
            "reduced page total_size {reduced_size} not a multiple of PAGE_SIZE_MIN {PAGE_SIZE_MIN}",
        );
        assert!(reduced_size >= PAGE_SIZE_MIN);

        // On aarch64 Apple the alignment granularity is genuinely 16 KiB: the
        // total_size must be a multiple of 16384. A hardcoded-4096 port would
        // align the same content end to the nearest 4 KiB instead, generally
        // producing a value that is NOT a multiple of 16384. Assert the stronger
        // 16 KiB alignment directly so this target's parity is pinned.
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            assert_eq!(16 << 10, PAGE_SIZE_MIN);
            assert_eq!(
                0,
                reduced_size % (16 << 10),
                "aarch64-macos reduced page total_size {reduced_size} must be 16 KiB-aligned",
            );
            assert_eq!(
                0,
                std_size % (16 << 10),
                "aarch64-macos std page total_size {std_size} must be 16 KiB-aligned",
            );
        }
    }

    #[test]
    fn available_bytes_for_grid_invariant() {
        // The reference `availableBytesForGrid` asserts at comptime
        //   assert(cells_align % Page.MetaLayout.alignment == 0);
        //   assert(@sizeOf(Cell) % Page.MetaLayout.alignment == 0);
        assert_eq!(0, CELLS_ALIGN % MetaLayout::ALIGNMENT);
        assert_eq!(0, core::mem::size_of::<Cell>() % MetaLayout::ALIGNMENT);
    }

    #[test]
    fn size_int_aliases() {
        // exactRowCapacity relies on:
        //   comptime assert(size.StyleCountInt == size.CellCountInt);
        //   comptime assert(size.HyperlinkCountInt == size.CellCountInt);
        assert_eq!(
            core::any::TypeId::of::<size::StyleCountInt>(),
            core::any::TypeId::of::<size::CellCountInt>()
        );
        assert_eq!(
            core::any::TypeId::of::<size::HyperlinkCountInt>(),
            core::any::TypeId::of::<size::CellCountInt>()
        );
    }

    // Rustty-only (NEW-FINDINGS N88): the page geometry equals the reference's.
    //
    // Every number below was measured from the reference at `12752b2ac` (P7-W1
    // T3, after `d2ff6d77a` cache-line aligned the cells and moved the metadata
    // into one `MetaLayout` block, and `6112935a2` shrank the hash-map header)
    // by the Zig probe `.port/phase-7/probes/p7w1t3_page_geometry_probe.zig`,
    // appended to a scratch copy of `page.zig` (`zig build test-lib-vt`, Zig
    // 0.16.0): natively on macOS arm64 (`page_size_min` 16 KiB) and
    // cross-compiled with `-Dtarget=aarch64-linux-gnu` and run on Linux aarch64
    // (4 KiB). The probe printed `Page.layout(cap).total_size`,
    // `cells_start`, `styles_layout.total_size`, `availableBytesForGrid`,
    // `maxCols`, and `adjust(.{ .cols = c }).rows` of `std_capacity` with the
    // grapheme budget set to 512 (the test build's) and 8,192 (the shipped
    // build's), and the total size of an 80 x 591 page as its style capacity
    // grows. (First measured at `20abdb50a` for N88; there the cells started
    // at 1,720, right after the row headers, and `maxCols` was 47,931 shipped
    // on macOS.)
    //
    // NEGATIVE CONTROL: lay the style set out with the port's own
    // `size_of::<Item>()` / `align_of::<Item>()` again (`Layout::init`,
    // `ref_counted_set.rs`) and this goes red: the styles section is 3,168
    // bytes against 4,000. Align the cells to `align_of::<Cell>()` again (the
    // pre-`d2ff6d77a` layout) and it goes red on `cells_start` (1,720 against
    // 1,792) and on the available bytes.
    #[cfg(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(any(target_os = "linux", target_os = "android"), target_arch = "aarch64")
    ))]
    struct RefGeometry {
        grapheme_bytes: u32,
        total_size: usize,
        available_bytes: usize,
        max_cols: u16,
        /// `(cols, rows)` of `std_capacity.adjust(.{ .cols = cols })`.
        rows: [(u16, u16); 13],
        /// `(styles, total_size)` of an 80 x 591 page.
        non_std: [(u16, usize); 4],
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    const REF_GEOMETRY: [RefGeometry; 2] = [
        RefGeometry {
            grapheme_bytes: 512,
            total_size: 393216,
            available_bytes: 384396,
            max_cols: 48033,
            rows: [
                (1, 24017),
                (40, 1171),
                (80, 593),
                (100, 475),
                (119, 400),
                (120, 397),
                (132, 361),
                (160, 298),
                (200, 239),
                (215, 222),
                (256, 186),
                (1000, 48),
                (10000, 4),
            ],
            non_std: [(256, 409600), (2048, 458752), (8192, 655360), (65535, 2441216)],
        },
        RefGeometry {
            grapheme_bytes: 8192,
            total_size: 409600,
            available_bytes: 383476,
            max_cols: 47918,
            rows: [
                (1, 23966),
                (40, 1168),
                (80, 591),
                (100, 474),
                (119, 399),
                (120, 396),
                (132, 360),
                (160, 297),
                (200, 238),
                (215, 221),
                (256, 186),
                (1000, 47),
                (10000, 4),
            ],
            non_std: [(256, 425984), (2048, 475136), (8192, 671744), (65535, 2457600)],
        },
    ];

    #[cfg(all(any(target_os = "linux", target_os = "android"), target_arch = "aarch64"))]
    const REF_GEOMETRY: [RefGeometry; 2] = [
        RefGeometry {
            grapheme_bytes: 512,
            total_size: 380928,
            available_bytes: 372108,
            max_cols: 46497,
            rows: [
                (1, 23249),
                (40, 1134),
                (80, 574),
                (100, 460),
                (119, 387),
                (120, 384),
                (132, 349),
                (160, 288),
                (200, 231),
                (215, 215),
                (256, 180),
                (1000, 46),
                (10000, 4),
            ],
            non_std: [(256, 397312), (2048, 454656), (8192, 647168), (65535, 2437120)],
        },
        RefGeometry {
            grapheme_bytes: 8192,
            total_size: 401408,
            available_bytes: 375284,
            max_cols: 46894,
            rows: [
                (1, 23454),
                (40, 1143),
                (80, 578),
                (100, 464),
                (119, 390),
                (120, 387),
                (132, 352),
                (160, 291),
                (200, 233),
                (215, 217),
                (256, 182),
                (1000, 46),
                (10000, 4),
            ],
            non_std: [(256, 413696), (2048, 471040), (8192, 663552), (65535, 2453504)],
        },
    ];

    #[cfg(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(any(target_os = "linux", target_os = "android"), target_arch = "aarch64")
    ))]
    #[test]
    fn page_geometry_matches_the_reference() {
        for g in &REF_GEOMETRY {
            let mut cap = std_capacity();
            cap.grapheme_bytes = g.grapheme_bytes;
            let l = Page::layout(cap);
            let gb = g.grapheme_bytes;
            assert_eq!(1792, l.cells_start, "gb={gb}: cells start on a 128-byte cache line");
            assert_eq!(4000, l.styles_layout.total_size, "gb={gb}: styles section");
            assert_eq!(g.total_size, l.total_size, "gb={gb}: std page size");
            assert_eq!(g.available_bytes, cap.available_bytes_for_grid(), "gb={gb}: availableBytesForGrid");
            assert_eq!(Some(g.max_cols), cap.max_cols(), "gb={gb}: maxCols");
            for &(cols, rows) in &g.rows {
                let adjusted = cap.adjust(Adjustment { cols: Some(cols) }).unwrap();
                assert_eq!(rows, adjusted.rows, "gb={gb}: rows per page at {cols} cols");
            }
            for &(styles, total) in &g.non_std {
                let c = Capacity {
                    styles,
                    grapheme_bytes: g.grapheme_bytes,
                    ..Capacity::new(80, 591)
                };
                assert_eq!(total, Page::layout(c).total_size, "gb={gb}: 80x591 page with {styles} styles");
            }
        }
    }

    // Rustty-only (R13, the `Capacity.adjust` row-count defect; see the PORT
    // NOTE in `adjust`): adjusting a page that left the standard capacity to a
    // narrow width must not overflow the `u16` row count. The reference panics
    // ("integer does not fit in destination type", MEASURED at `12752b2ac`);
    // the port clamps to the largest row count, and the page still fits the
    // original size.
    //
    // NEGATIVE CONTROL: without the clamp the row count wraps (`as u16`) and
    // the first assertion fails (1,655 rows against 65,535 on macOS arm64).
    #[test]
    fn capacity_adjust_clamps_rows_to_the_row_count_type() {
        let wide = Capacity {
            cols: 50_000,
            ..std_capacity()
        };
        let total = Page::layout(wide).total_size;
        let adjusted = wide.adjust(Adjustment { cols: Some(80) }).unwrap();
        assert_eq!(size::CellCountInt::MAX, adjusted.rows);
        assert_eq!(80, adjusted.cols);
        assert!(Page::layout(adjusted).total_size <= total);
    }

    // =====================================================================
    // Ported `test {}` blocks (assertion-for-assertion).
    // =====================================================================

    #[test]
    fn page_layout_can_take_a_maxed_capacity() {
        // Our intention is for a maxed-out capacity to always fit
        // within a page layout without triggering runtime safety on any
        // overflow. This simplifies some of our handling downstream of the
        // call (relevant to: https://github.com/ghostty-org/ghostty/issues/10258)
        // inline for (@typeInfo(Capacity).@"struct".fields) |field| {
        //     @field(cap, field.name) = std.math.maxInt(field.type);
        // }
        let cap = Capacity {
            cols: size::CellCountInt::MAX,
            rows: size::CellCountInt::MAX,
            styles: size::StyleCountInt::MAX,
            hyperlink_bytes: size::HyperlinkCountInt::MAX,
            grapheme_bytes: size::GraphemeBytesInt::MAX,
            string_bytes: size::StringBytesInt::MAX,
        };

        // Note that a max capacity will exceed our max_page_size so we
        // can't init a page with it, but it should layout.
        let _ = Page::layout(cap);
    }

    #[test]
    fn cell_is_zero_by_default() {
        let cell = Cell::init(0);
        let cell_int: u64 = cell.to_bits();
        assert_eq!(0u64, cell_int);

        // The zero value should be output type for semantic content.
        // This is very important for our assumptions elsewhere.
        assert_eq!(SemanticContent::Output, cell.semantic_content());
    }

    #[test]
    fn page_capacity_adjust_cols_down() {
        let original = std_capacity();
        let original_size = Page::layout(original).total_size;
        let adjusted = original.adjust(Adjustment {
            cols: Some(original.cols / 2),
        }).unwrap();
        let adjusted_size = Page::layout(adjusted).total_size;
        assert_eq!(original_size, adjusted_size);
        // If we layout a page with 1 more row and it's still the same size
        // then adjust is not producing enough rows.
        let mut bigger = adjusted;
        bigger.rows += 1;
        let bigger_size = Page::layout(bigger).total_size;
        assert!(bigger_size > original_size);
    }

    #[test]
    fn page_capacity_adjust_cols_down_to_1() {
        let original = std_capacity();
        let original_size = Page::layout(original).total_size;
        let adjusted = original.adjust(Adjustment { cols: Some(1) }).unwrap();
        let adjusted_size = Page::layout(adjusted).total_size;
        assert_eq!(original_size, adjusted_size);
        let mut bigger = adjusted;
        bigger.rows += 1;
        let bigger_size = Page::layout(bigger).total_size;
        assert!(bigger_size > original_size);
    }

    #[test]
    fn page_capacity_adjust_cols_up() {
        let original = std_capacity();
        let original_size = Page::layout(original).total_size;
        let adjusted = original.adjust(Adjustment {
            cols: Some(original.cols * 2),
        }).unwrap();
        let adjusted_size = Page::layout(adjusted).total_size;
        assert_eq!(original_size, adjusted_size);
        let mut bigger = adjusted;
        bigger.rows += 1;
        let bigger_size = Page::layout(bigger).total_size;
        assert!(bigger_size > original_size);
    }

    #[test]
    fn page_capacity_adjust_cols_sweep() {
        let mut cap = std_capacity();
        let original_cols = cap.cols;
        let original_size = Page::layout(cap).total_size;
        for c in 1..(original_cols * 2) {
            cap = cap.adjust(Adjustment { cols: Some(c) }).unwrap();
            let adjusted_size = Page::layout(cap).total_size;
            assert_eq!(original_size, adjusted_size);
            let mut bigger = cap;
            bigger.rows += 1;
            let bigger_size = Page::layout(bigger).total_size;
            assert!(bigger_size > original_size);
        }
    }

    #[test]
    fn page_capacity_adjust_cols_too_high() {
        let original = std_capacity();
        assert_eq!(
            Err(OutOfMemory),
            original.adjust(Adjustment {
                cols: Some(size::CellCountInt::MAX),
            }),
        );
    }

    #[test]
    fn capacity_maxCols_basic() {
        let cap = std_capacity();
        let max = cap.max_cols().unwrap();

        // maxCols should be >= current cols (since current capacity is valid)
        assert!(max >= cap.cols);

        // Adjusting to maxCols should succeed with at least 1 row
        let adjusted = cap.adjust(Adjustment { cols: Some(max) }).unwrap();
        assert!(adjusted.rows >= 1);

        // Adjusting to maxCols + 1 should fail
        assert_eq!(
            Err(OutOfMemory),
            cap.adjust(Adjustment { cols: Some(max + 1) }),
        );
    }

    #[test]
    fn capacity_maxCols_preserves_total_size() {
        let cap = std_capacity();
        let original_size = Page::layout(cap).total_size;
        let max = cap.max_cols().unwrap();
        let adjusted = cap.adjust(Adjustment { cols: Some(max) }).unwrap();
        let adjusted_size = Page::layout(adjusted).total_size;
        assert_eq!(original_size, adjusted_size);
    }

    #[test]
    fn capacity_maxCols_with_1_row_exactly() {
        let cap = std_capacity();
        let max = cap.max_cols().unwrap();
        let adjusted = cap.adjust(Adjustment { cols: Some(max) }).unwrap();
        assert_eq!(1 as size::CellCountInt, adjusted.rows);
    }

    #[test]
    fn page_init() {
        let mut page = Page::init(Capacity {
            cols: 120,
            rows: 80,
            styles: 32,
            ..Capacity::new(120, 80)
        })
        .unwrap();
        page.deinit();
    }

    #[test]
    fn page_read_and_write_cells() {
        let mut page = Page::init(Capacity {
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();

        for y in 0..page.capacity.rows as usize {
            let rac = page.get_row_and_cell(1, y);
            unsafe {
                *rac.cell = cell_codepoint(y as u21);
            }
        }

        // Read it again
        for y in 0..page.capacity.rows as usize {
            let rac = page.get_row_and_cell(1, y);
            assert_eq!(y as u21, unsafe { (*rac.cell).content_codepoint() });
        }

        page.deinit();
    }

    #[test]
    fn page_appendGrapheme_small() {
        let mut page = Page::init(Capacity {
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();

        let rac = page.get_row_and_cell(0, 0);
        unsafe {
            *rac.cell = Cell::init(0x09);
        }

        // One
        page.append_grapheme(unsafe { &mut *rac.row }, unsafe { &mut *rac.cell }, 0x0A)
            .unwrap();
        assert!(unsafe { (*rac.row).grapheme() });
        assert!(unsafe { (*rac.cell).has_grapheme() });
        assert_eq!(
            &[0x0Au32][..],
            page.lookup_grapheme(unsafe { &*rac.cell }).unwrap()
        );

        // Two
        page.append_grapheme(unsafe { &mut *rac.row }, unsafe { &mut *rac.cell }, 0x0B)
            .unwrap();
        assert!(unsafe { (*rac.row).grapheme() });
        assert!(unsafe { (*rac.cell).has_grapheme() });
        assert_eq!(
            &[0x0Au32, 0x0B][..],
            page.lookup_grapheme(unsafe { &*rac.cell }).unwrap()
        );

        // Clear it
        page.clear_grapheme(unsafe { &mut *rac.cell });
        page.update_row_grapheme_flag(unsafe { &mut *rac.row });
        assert!(!unsafe { (*rac.row).grapheme() });
        assert!(!unsafe { (*rac.cell).has_grapheme() });

        page.deinit();
    }

    #[test]
    fn page_appendGrapheme_larger_than_chunk() {
        let mut page = Page::init(Capacity {
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();

        let rac = page.get_row_and_cell(0, 0);
        unsafe {
            *rac.cell = Cell::init(0x09);
        }

        let count = GRAPHEME_CHUNK_LEN * 10;
        for i in 0..count {
            page.append_grapheme(
                unsafe { &mut *rac.row },
                unsafe { &mut *rac.cell },
                (0x0A + i) as u32,
            )
            .unwrap();
        }

        let cps = page.lookup_grapheme(unsafe { &*rac.cell }).unwrap();
        assert_eq!(count, cps.len());
        for i in 0..count {
            assert_eq!((0x0A + i) as u32, cps[i]);
        }

        page.deinit();
    }

    // test "Page appendGrapheme caps codepoints per cell"
    #[test]
    fn page_appendGrapheme_caps_codepoints_per_cell() {
        let mut page = Page::init(Capacity {
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();

        let rac = page.get_row_and_cell(0, 0);
        unsafe {
            *rac.cell = Cell::init('A' as u32);
        }

        for i in 0..GRAPHEME_MAX_LEN + 16 {
            page.append_grapheme(
                unsafe { &mut *rac.row },
                unsafe { &mut *rac.cell },
                (0x0300 + i) as u32,
            )
            .unwrap();
        }

        let cps = page.lookup_grapheme(unsafe { &*rac.cell }).unwrap();
        assert_eq!(GRAPHEME_MAX_LEN, cps.len());
        for i in 0..GRAPHEME_MAX_LEN {
            assert_eq!((0x0300 + i) as u32, cps[i]);
        }

        page.deinit();
    }

    // test "Page setGraphemes caps codepoints per cell"
    #[test]
    fn page_setGraphemes_caps_codepoints_per_cell() {
        let mut page = Page::init(Capacity {
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();

        let mut input: [u32; GRAPHEME_MAX_LEN + 16] = [0; GRAPHEME_MAX_LEN + 16];
        for (i, cp) in input.iter_mut().enumerate() {
            *cp = (0x0300 + i) as u32;
        }

        let rac = page.get_row_and_cell(0, 0);
        unsafe {
            *rac.cell = Cell::init('A' as u32);
        }
        page.set_graphemes(rac.row, rac.cell, &input).unwrap();

        assert_eq!(
            GRAPHEME_MAX_LEN,
            page.lookup_grapheme(unsafe { &*rac.cell }).unwrap().len(),
        );

        page.deinit();
    }

    #[test]
    fn page_clearGrapheme_not_all_cells() {
        let mut page = Page::init(Capacity {
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();

        let rac = page.get_row_and_cell(0, 0);
        unsafe {
            *rac.cell = Cell::init(0x09);
        }
        page.append_grapheme(unsafe { &mut *rac.row }, unsafe { &mut *rac.cell }, 0x0A)
            .unwrap();

        let rac2 = page.get_row_and_cell(1, 0);
        unsafe {
            *rac2.cell = Cell::init(0x09);
        }
        page.append_grapheme(unsafe { &mut *rac2.row }, unsafe { &mut *rac2.cell }, 0x0A)
            .unwrap();

        // Clear it
        page.clear_grapheme(unsafe { &mut *rac.cell });
        page.update_row_grapheme_flag(unsafe { &mut *rac.row });
        assert!(unsafe { (*rac.row).grapheme() });
        assert!(!unsafe { (*rac.cell).has_grapheme() });
        assert!(unsafe { (*rac2.cell).has_grapheme() });

        page.deinit();
    }

    #[test]
    fn page_clone() {
        let mut page = Page::init(Capacity {
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();

        // Write
        for y in 0..page.capacity.rows as usize {
            let rac = page.get_row_and_cell(1, y);
            unsafe {
                *rac.cell = cell_codepoint(y as u21);
            }
        }

        // Clone
        let mut page2 = page.clone().unwrap();
        assert_eq!(page2.capacity, page.capacity);

        // Read it again
        for y in 0..page2.capacity.rows as usize {
            let rac = page2.get_row_and_cell(1, y);
            assert_eq!(y as u21, unsafe { (*rac.cell).content_codepoint() });
        }

        // Write again
        for y in 0..page.capacity.rows as usize {
            let rac = page.get_row_and_cell(1, y);
            unsafe {
                *rac.cell = cell_codepoint(0);
            }
        }

        // Read it again, should be unchanged
        for y in 0..page2.capacity.rows as usize {
            let rac = page2.get_row_and_cell(1, y);
            assert_eq!(y as u21, unsafe { (*rac.cell).content_codepoint() });
        }

        // Read the original
        for y in 0..page.capacity.rows as usize {
            let rac = page.get_row_and_cell(1, y);
            assert_eq!(0u32, unsafe { (*rac.cell).content_codepoint() });
        }

        page2.deinit();
        page.deinit();
    }

    #[test]
    fn page_clone_graphemes() {
        let mut page = Page::init(Capacity {
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();

        // Append some graphemes
        {
            let rac = page.get_row_and_cell(0, 0);
            unsafe {
                *rac.cell = Cell::init(0x09);
            }
            page.append_grapheme(unsafe { &mut *rac.row }, unsafe { &mut *rac.cell }, 0x0A)
                .unwrap();
            page.append_grapheme(unsafe { &mut *rac.row }, unsafe { &mut *rac.cell }, 0x0B)
                .unwrap();
        }

        // Clone it
        let mut page2 = page.clone().unwrap();
        {
            let rac = page2.get_row_and_cell(0, 0);
            assert!(unsafe { (*rac.row).grapheme() });
            assert!(unsafe { (*rac.cell).has_grapheme() });
            assert_eq!(
                &[0x0Au32, 0x0B][..],
                page2.lookup_grapheme(unsafe { &*rac.cell }).unwrap()
            );
        }

        page2.deinit();
        page.deinit();
    }

    #[test]
    fn page_clone_styles() {
        let mut page = Page::init(Capacity {
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();

        // Write with some styles
        {
            let base = page.memory_ptr();
            let id = unsafe {
                page.styles.add(
                    base,
                    Style {
                        flags: stylepkg::Flags {
                            bold: true,
                            ..stylepkg::Flags::default()
                        },
                        ..Style::default()
                    },
                )
            }
            .unwrap();

            for x in 0..page.size.cols as usize {
                let rac = page.get_row_and_cell(x, 0);
                unsafe {
                    (*rac.row).set_styled(true);
                    let mut c = cell_codepoint((x + 1) as u21);
                    c.set_style_id(id);
                    *rac.cell = c;
                }
                unsafe { page.styles.use_id(base, id) };
            }
        }

        // Clone it
        let mut page2 = page.clone().unwrap();
        {
            let id: u16 = {
                let rac = page2.get_row_and_cell(0, 0);
                unsafe { (*rac.cell).style_id() }
            };

            for x in 0..page.size.cols as usize {
                let rac = page.get_row_and_cell(x, 0);
                assert!(unsafe { (*rac.row).styled() });
                assert_eq!(id, unsafe { (*rac.cell).style_id() });
            }

            let style = unsafe { &*page.styles.get(page.memory_ptr(), id) };
            assert!((Style {
                flags: stylepkg::Flags {
                    bold: true,
                    ..stylepkg::Flags::default()
                },
                ..Style::default()
            })
            .eql(style));
        }

        page2.deinit();
        page.deinit();
    }

    // =====================================================================
    // PERF(port) (reshape) regression guards for the style-run batch in the
    // different-page clone (`clone_partial_row_from_inner`): assert the EXACT
    // per-style reference counts and id mappings the reference's per-cell
    // `add_with_id` loop produces, across all three routes — the batched
    // `use_multiple` path, the non-batchable fallback (live non-equal item at
    // the source id), and the dead-item resurrection path. Written to pass
    // against the pre-reshape per-cell loop as well (verified both ways).
    // =====================================================================

    fn test_style(bold: bool, italic: bool) -> Style {
        Style {
            flags: stylepkg::Flags {
                bold,
                italic,
                ..stylepkg::Flags::default()
            },
            ..Style::default()
        }
    }

    /// Fill row `y` of `page` with `cols` cells; cell x gets `styles[x %
    /// styles.len()]` (an id of 0 = unstyled).
    fn write_styled_row(page: &mut Page, y: usize, style_ids: &[u16]) {
        let base = page.memory_ptr();
        for x in 0..page.size.cols as usize {
            let id = style_ids[x % style_ids.len()];
            let rac = page.get_row_and_cell(x, y);
            unsafe {
                let mut c = cell_codepoint((x + 1) as u21);
                if id != 0 {
                    c.set_style_id(id);
                    (*rac.row).set_styled(true);
                    page.styles.use_id(base, id);
                }
                *rac.cell = c;
            }
        }
    }

    #[test]
    fn page_cloneFrom_style_run_refcounts_batched() {
        let mut page = Page::init(Capacity {
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();

        let base = page.memory_ptr();
        let style_a = test_style(true, false);
        let style_b = test_style(false, true);
        let id_a = unsafe { page.styles.add(base, style_a) }.unwrap();
        let id_b = unsafe { page.styles.add(base, style_b) }.unwrap();

        // Row 0: one full-width run of style A (10 cells).
        write_styled_row(&mut page, 0, &[id_a]);
        // Row 1: alternating A/B per cell (10 runs of length 1).
        write_styled_row(&mut page, 1, &[id_a, id_b]);
        // Row 2: A for the first half, unstyled for the second
        // (run then plain run).
        write_styled_row(&mut page, 2, &[id_a, id_a, id_a, id_a, id_a, 0, 0, 0, 0, 0]);

        // Clone to a fresh page (all adds route through the clone chain).
        let mut page2 = Page::init(Capacity {
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();
        page2.clone_from(&page, 0, page.size.rows as usize).unwrap();

        let base2 = page2.memory_ptr();
        let id_a2 = unsafe { page2.styles.lookup(base2, style_a) }.expect("style A cloned");
        let id_b2 = unsafe { page2.styles.lookup(base2, style_b) }.expect("style B cloned");

        // Exactly one ref per styled destination cell:
        // A: 10 (row 0) + 5 (row 1) + 5 (row 2) = 20; B: 5 (row 1).
        assert_eq!(20, unsafe { page2.styles.ref_count(base2, id_a2) });
        assert_eq!(5, unsafe { page2.styles.ref_count(base2, id_b2) });
        assert_eq!(2, page2.styles.count());

        // Cell-level mapping + content.
        for y in 0..3usize {
            for x in 0..page2.size.cols as usize {
                let rac = page2.get_row_and_cell(x, y);
                let cell = unsafe { *rac.cell };
                assert_eq!((x + 1) as u21, cell.content_codepoint());
                let src_rac = page.get_row_and_cell(x, y);
                let src_id = unsafe { (*src_rac.cell).style_id() };
                let expect = if src_id == id_a {
                    id_a2
                } else if src_id == id_b {
                    id_b2
                } else {
                    stylepkg::DEFAULT_ID
                };
                assert_eq!(expect, cell.style_id(), "x={x} y={y}");
            }
        }

        page2.deinit();
        page.deinit();
    }

    #[test]
    fn page_cloneFrom_style_run_refcounts_conflicting_dst_id() {
        let mut page = Page::init(Capacity {
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();

        let base = page.memory_ptr();
        let style_a = test_style(true, false);
        let id_a = unsafe { page.styles.add(base, style_a) }.unwrap();
        // Row 0: one full-width run of style A.
        write_styled_row(&mut page, 0, &[id_a]);

        // Destination already has a LIVE, different style at the same id, so
        // `add_with_id` cannot take its fast path for any cell of the run
        // (the non-batchable fallback route).
        let mut page2 = Page::init(Capacity {
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();
        let base2 = page2.memory_ptr();
        let style_c = test_style(false, true);
        let id_c = unsafe { page2.styles.add(base2, style_c) }.unwrap();
        assert_eq!(id_a, id_c); // both are the first id — the conflict

        page2.clone_from(&page, 0, page.size.rows as usize).unwrap();

        // A landed on a fresh id; C is untouched.
        let id_a2 = unsafe { page2.styles.lookup(base2, style_a) }.expect("style A cloned");
        assert_ne!(id_a2, id_c);
        assert_eq!(10, unsafe { page2.styles.ref_count(base2, id_a2) });
        assert_eq!(1, unsafe { page2.styles.ref_count(base2, id_c) });
        assert_eq!(2, page2.styles.count());
        for x in 0..page2.size.cols as usize {
            let rac = page2.get_row_and_cell(x, 0);
            assert_eq!(id_a2, unsafe { (*rac.cell).style_id() }, "x={x}");
        }

        page2.deinit();
        page.deinit();
    }

    #[test]
    fn page_cloneFrom_style_run_refcounts_dead_dst_id() {
        let mut page = Page::init(Capacity {
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();

        let base = page.memory_ptr();
        let style_a = test_style(true, false);
        let id_a = unsafe { page.styles.add(base, style_a) }.unwrap();
        // Row 0: one full-width run of style A.
        write_styled_row(&mut page, 0, &[id_a]);

        // Destination has a DEAD (released, not yet reaped) style at the
        // same id: the first cell resurrects the id via the dead-item branch,
        // after which the remaining cells are pure ref bumps.
        let mut page2 = Page::init(Capacity {
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();
        let base2 = page2.memory_ptr();
        let style_c = test_style(false, true);
        let id_c = unsafe { page2.styles.add(base2, style_c) }.unwrap();
        assert_eq!(id_a, id_c);
        unsafe { page2.styles.release(base2, id_c) }; // ref -> 0, item stays

        page2.clone_from(&page, 0, page.size.rows as usize).unwrap();

        // A reused the dead id; C is gone.
        let id_a2 = unsafe { page2.styles.lookup(base2, style_a) }.expect("style A cloned");
        assert_eq!(id_a, id_a2);
        assert_eq!(10, unsafe { page2.styles.ref_count(base2, id_a2) });
        assert_eq!(None, unsafe { page2.styles.lookup(base2, style_c) });
        assert_eq!(1, page2.styles.count());
        for x in 0..page2.size.cols as usize {
            let rac = page2.get_row_and_cell(x, 0);
            assert_eq!(id_a2, unsafe { (*rac.cell).style_id() }, "x={x}");
        }

        page2.deinit();
        page.deinit();
    }

    /// rustty-only (task S-4, N93): the batched style-run clone
    /// (`clone_partial_row_from_inner`'s different-page arm with
    /// `add_with_id_pure_ref_bump_target`) must leave the destination page
    /// BYTE-IDENTICAL to the reference's per-cell body (page.zig
    /// `clonePartialRowFrom`: reset the cell, `addWithId(style, src_id)`,
    /// store `result orelse src_id`), and with identical set bookkeeping.
    /// Runs of every length 1..=9 over 12 styles, into a fresh destination
    /// (styles REMAP: first-seen numbering), one holding live conflicting
    /// styles at the source ids, and one holding dead ones.
    #[test]
    fn page_cloneFrom_style_runs_match_per_cell_reference() {
        const COLS: usize = 40;
        const ROWS: usize = 12;
        let styles: Vec<Style> = (0..12u8)
            .map(|k| Style {
                fg_color: stylepkg::Color::Rgb(crate::color::RGB {
                    r: k.wrapping_mul(37),
                    g: 200 - k,
                    b: k,
                }),
                ..Style::default()
            })
            .collect();
        let cap = || Capacity {
            styles: 64,
            ..Capacity::new(COLS as u16, ROWS as u16)
        };

        // Source: style ids assigned in a scrambled order, so that the
        // destination's first-seen numbering differs from them.
        let mut src = Page::init(cap()).unwrap();
        let sbase = src.memory_ptr();
        let mut ids = Vec::new();
        for k in [7usize, 2, 11, 0, 5, 9, 1, 4, 10, 3, 8, 6] {
            ids.push((k, unsafe { src.styles.add(sbase, styles[k]) }.unwrap()));
        }
        ids.sort();
        let ids: Vec<u16> = ids.into_iter().map(|(_, id)| id).collect();
        // Every run length 1..=9, cycling styles, with unstyled gaps.
        let mut seq: Vec<u16> = Vec::new();
        let mut z: u64 = 0x93;
        while seq.len() < COLS * ROWS {
            z = z.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let len = 1 + (seq.len() / 7) % 9;
            let id = if (z >> 60) == 0 { 0 } else { ids[((z >> 33) % 12) as usize] };
            seq.extend(std::iter::repeat_n(id, len));
        }
        for y in 0..ROWS {
            for x in 0..COLS {
                let id = seq[y * COLS + x];
                let rac = src.get_row_and_cell(x, y);
                unsafe {
                    let mut c = cell_codepoint((0x41 + (x % 26)) as u21);
                    if id != 0 {
                        c.set_style_id(id);
                        (*rac.row).set_styled(true);
                        src.styles.use_id(sbase, id);
                    }
                    *rac.cell = c;
                }
            }
        }
        // Drop the creation references so only cell references remain.
        for &id in &ids {
            unsafe { src.styles.release(sbase, id) };
        }

        // Destination pre-states: 0 fresh, 1 live conflicting styles at the
        // low ids, 2 the same but released (dead, not yet reaped).
        let prep = |mode: u8| {
            let mut p = Page::init(cap()).unwrap();
            let b = p.memory_ptr();
            if mode > 0 {
                for k in 0..4u8 {
                    let other = Style {
                        bg_color: stylepkg::Color::Rgb(crate::color::RGB { r: 1, g: 2, b: k }),
                        ..Style::default()
                    };
                    let id = unsafe { p.styles.add(b, other) }.unwrap();
                    if mode == 2 {
                        unsafe { p.styles.release(b, id) };
                    }
                }
            }
            p
        };

        let mut remapped_runs = 0usize;
        for mode in 0..3u8 {
            // The port's clone.
            let mut got = prep(mode);
            let batched_before = REMAPPED_STYLE_RUNS_BATCHED.with(|c| c.get());
            got.clone_from(&src, 0, ROWS).unwrap();
            // Positive evidence that the batched remap path ran.
            let batched = REMAPPED_STYLE_RUNS_BATCHED.with(|c| c.get()) - batched_before;
            assert!(batched > 20, "mode {mode}: remapped runs batched: {batched}");

            // The reference's per-cell body.
            let mut want = prep(mode);
            let wbase = want.memory_ptr();
            for y in 0..ROWS {
                let s_row = src.get_row_and_cell(0, y).row;
                let d_row = want.get_row_and_cell(0, y).row;
                unsafe {
                    let mut copy = *s_row;
                    copy.set_cells((*d_row).cells());
                    *d_row = copy;
                }
                for x in 0..COLS {
                    let sc = unsafe { *src.get_row_and_cell(x, y).cell };
                    let dc = want.get_row_and_cell(x, y).cell;
                    unsafe {
                        *dc = sc.copy_with_managed_reset();
                        if sc.style_id() != stylepkg::DEFAULT_ID {
                            (*d_row).set_styled(true);
                            let st = *src.styles.get(sbase, sc.style_id());
                            let added = want
                                .styles
                                .add_with_id(wbase, st, sc.style_id())
                                .unwrap();
                            if added.is_some_and(|a| a != sc.style_id()) {
                                remapped_runs += 1;
                            }
                            (*dc).set_style_id(added.unwrap_or(sc.style_id()));
                        }
                    }
                }
            }

            let g = unsafe { core::slice::from_raw_parts(got.memory_ptr(), got.memory_len()) };
            let w = unsafe { core::slice::from_raw_parts(want.memory_ptr(), want.memory_len()) };
            assert!(g == w, "mode {mode}: page bytes differ from the per-cell reference");
            assert_eq!(want.styles.next_id, got.styles.next_id, "mode {mode}");
            assert_eq!(want.styles.living, got.styles.living, "mode {mode}");
            assert_eq!(want.styles.max_psl, got.styles.max_psl, "mode {mode}");
            assert_eq!(want.styles.psl_stats, got.styles.psl_stats, "mode {mode}");

            got.deinit();
            want.deinit();
        }
        // The case N93 is about was exercised: styles landed on other ids.
        assert!(remapped_runs > 100, "remapped cells: {remapped_runs}");
        src.deinit();
    }

    #[test]
    fn page_cloneFrom() {
        let mut page = Page::init(Capacity {
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();

        // Write
        for y in 0..page.capacity.rows as usize {
            let rac = page.get_row_and_cell(1, y);
            unsafe {
                *rac.cell = cell_codepoint(y as u21);
            }
        }

        // Clone
        let mut page2 = Page::init(Capacity {
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();
        page2
            .clone_from(&page, 0, page.size.rows as usize)
            .unwrap();

        // Read it again
        for y in 0..page2.capacity.rows as usize {
            let rac = page2.get_row_and_cell(1, y);
            assert_eq!(y as u21, unsafe { (*rac.cell).content_codepoint() });
        }

        // Write again
        for y in 0..page.capacity.rows as usize {
            let rac = page.get_row_and_cell(1, y);
            unsafe {
                *rac.cell = cell_codepoint(0);
            }
        }

        // Read it again, should be unchanged
        for y in 0..page2.capacity.rows as usize {
            let rac = page2.get_row_and_cell(1, y);
            assert_eq!(y as u21, unsafe { (*rac.cell).content_codepoint() });
        }

        // Read the original
        for y in 0..page.capacity.rows as usize {
            let rac = page.get_row_and_cell(1, y);
            assert_eq!(0u32, unsafe { (*rac.cell).content_codepoint() });
        }

        page2.deinit();
        page.deinit();
    }

    #[test]
    fn page_cloneFrom_shrink_columns() {
        let mut page = Page::init(Capacity {
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();

        // Write
        for y in 0..page.capacity.rows as usize {
            let rac = page.get_row_and_cell(1, y);
            unsafe {
                *rac.cell = cell_codepoint(y as u21);
            }
        }

        // Clone
        let mut page2 = Page::init(Capacity {
            styles: 8,
            ..Capacity::new(5, 10)
        })
        .unwrap();
        page2
            .clone_from(&page, 0, page.size.rows as usize)
            .unwrap();
        assert_eq!(5 as size::CellCountInt, page2.size.cols);

        // Read it again
        for y in 0..page2.capacity.rows as usize {
            let rac = page2.get_row_and_cell(1, y);
            assert_eq!(y as u21, unsafe { (*rac.cell).content_codepoint() });
        }

        page2.deinit();
        page.deinit();
    }

    #[test]
    fn page_cloneFrom_partial() {
        let mut page = Page::init(Capacity {
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();

        // Write
        for y in 0..page.capacity.rows as usize {
            let rac = page.get_row_and_cell(1, y);
            unsafe {
                *rac.cell = cell_codepoint(y as u21);
            }
        }

        // Clone
        let mut page2 = Page::init(Capacity {
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();
        page2.clone_from(&page, 0, 5).unwrap();

        // Read it again
        for y in 0..5 {
            let rac = page2.get_row_and_cell(1, y);
            assert_eq!(y as u21, unsafe { (*rac.cell).content_codepoint() });
        }
        for y in 5..(page2.size.rows as usize) {
            let rac = page2.get_row_and_cell(1, y);
            assert_eq!(0u32, unsafe { (*rac.cell).content_codepoint() });
        }

        page2.deinit();
        page.deinit();
    }

    #[test]
    fn page_cloneFrom_hyperlinks_exact_capacity() {
        let mut page = Page::init(Capacity::new(50, 50)).unwrap();

        // Ensure our page can accommodate the capacity.
        let hyperlink_cap = page.hyperlink_capacity();
        assert!(hyperlink_cap <= (page.size.cols as usize) * (page.size.rows as usize));

        // Create a hyperlink.
        let hyperlink_id = page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(0),
                uri: b"https://example.com".to_vec(),
            })
            .unwrap();

        // Fill the exact cap with cells.
        'fill: for x in 0..page.size.cols as usize {
            for y in 0..page.size.rows as usize {
                let rac = page.get_row_and_cell(x, y);
                unsafe {
                    *rac.cell = cell_codepoint(42);
                }
                page.set_hyperlink(
                    unsafe { &mut *rac.row },
                    unsafe { &mut *rac.cell },
                    hyperlink_id,
                )
                .unwrap();
                unsafe { page.hyperlink_set.use_id(page.memory_ptr(), hyperlink_id) };

                if page.hyperlink_count() == hyperlink_cap {
                    break 'fill;
                }
            }
        }
        assert_eq!(page.hyperlink_count(), page.hyperlink_capacity());

        // Clone the full page
        let mut page2 = Page::init(page.capacity).unwrap();
        page2
            .clone_from(&page, 0, page.size.rows as usize)
            .unwrap();

        // We should have the same number of hyperlinks
        assert_eq!(page2.hyperlink_count(), page.hyperlink_count());

        page2.deinit();
        page.deinit();
    }

    #[test]
    fn page_cloneFrom_graphemes() {
        let mut page = Page::init(Capacity {
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();

        // Write
        for y in 0..page.capacity.rows as usize {
            let rac = page.get_row_and_cell(1, y);
            unsafe {
                *rac.cell = cell_codepoint((y + 1) as u21);
            }
            page.append_grapheme(unsafe { &mut *rac.row }, unsafe { &mut *rac.cell }, 0x0A)
                .unwrap();
        }

        // Clone
        let mut page2 = Page::init(Capacity {
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();
        page2
            .clone_from(&page, 0, page.size.rows as usize)
            .unwrap();

        // Read it again
        for y in 0..page2.capacity.rows as usize {
            let rac = page2.get_row_and_cell(1, y);
            assert_eq!((y + 1) as u21, unsafe { (*rac.cell).content_codepoint() });
            assert!(unsafe { (*rac.row).grapheme() });
            assert!(unsafe { (*rac.cell).has_grapheme() });
            assert_eq!(
                &[0x0Au32][..],
                page2.lookup_grapheme(unsafe { &*rac.cell }).unwrap()
            );
        }

        // Write again
        for y in 0..page.capacity.rows as usize {
            let rac = page.get_row_and_cell(1, y);
            page.clear_grapheme(unsafe { &mut *rac.cell });
            page.update_row_grapheme_flag(unsafe { &mut *rac.row });
            unsafe {
                *rac.cell = cell_codepoint(0);
            }
        }

        // Read it again, should be unchanged
        for y in 0..page2.capacity.rows as usize {
            let rac = page2.get_row_and_cell(1, y);
            assert_eq!((y + 1) as u21, unsafe { (*rac.cell).content_codepoint() });
            assert!(unsafe { (*rac.row).grapheme() });
            assert!(unsafe { (*rac.cell).has_grapheme() });
            assert_eq!(
                &[0x0Au32][..],
                page2.lookup_grapheme(unsafe { &*rac.cell }).unwrap()
            );
        }

        // Read the original
        for y in 0..page.capacity.rows as usize {
            let rac = page.get_row_and_cell(1, y);
            assert_eq!(0u32, unsafe { (*rac.cell).content_codepoint() });
        }

        page2.deinit();
        page.deinit();
    }

    #[test]
    fn page_cloneFrom_frees_dst_graphemes() {
        let mut page = Page::init(Capacity {
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();
        for y in 0..page.capacity.rows as usize {
            let rac = page.get_row_and_cell(1, y);
            unsafe {
                *rac.cell = cell_codepoint((y + 1) as u21);
            }
        }

        // Clone
        let mut page2 = Page::init(Capacity {
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();
        for y in 0..page2.capacity.rows as usize {
            let rac = page2.get_row_and_cell(1, y);
            unsafe {
                *rac.cell = cell_codepoint((y + 1) as u21);
            }
            page2
                .append_grapheme(unsafe { &mut *rac.row }, unsafe { &mut *rac.cell }, 0x0A)
                .unwrap();
        }

        // Clone from page which has no graphemes.
        page2
            .clone_from(&page, 0, page.size.rows as usize)
            .unwrap();

        // Read it again
        for y in 0..page2.capacity.rows as usize {
            let rac = page2.get_row_and_cell(1, y);
            assert_eq!((y + 1) as u21, unsafe { (*rac.cell).content_codepoint() });
            assert!(!unsafe { (*rac.row).grapheme() });
            assert!(!unsafe { (*rac.cell).has_grapheme() });
        }
        assert_eq!(0usize, page2.grapheme_count());

        page2.deinit();
        page.deinit();
    }

    #[test]
    fn page_cloneRowFrom_partial() {
        let mut page = Page::init(Capacity {
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();

        // Write
        {
            let y = 0;
            for x in 0..page.size.cols as usize {
                let rac = page.get_row_and_cell(x, y);
                unsafe {
                    *rac.cell = cell_codepoint((x + 1) as u21);
                }
            }
        }

        // Clone
        let mut page2 = Page::init(Capacity {
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();
        {
            let dst = page2.get_row(0);
            let src = page.get_row(0);
            page2
                .clone_partial_row_from(&page, dst, src, 2, 8)
                .unwrap();
        }

        // Read it again
        {
            let y = 0;
            for x in 0..page2.size.cols as usize {
                let expected: u21 = if x >= 2 && x < 8 { (x + 1) as u21 } else { 0 };
                let rac = page2.get_row_and_cell(x, y);
                assert_eq!(expected, unsafe { (*rac.cell).content_codepoint() });
            }
        }

        page2.deinit();
        page.deinit();
    }

    #[test]
    fn page_cloneRowFrom_partial_grapheme_in_non_copied_source_region() {
        let mut page = Page::init(Capacity {
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();

        // Write
        {
            let y = 0;
            for x in 0..page.size.cols as usize {
                let rac = page.get_row_and_cell(x, y);
                unsafe {
                    *rac.cell = cell_codepoint((x + 1) as u21);
                }
            }
            {
                let rac = page.get_row_and_cell(0, y);
                page.append_grapheme(unsafe { &mut *rac.row }, unsafe { &mut *rac.cell }, 0x0A)
                    .unwrap();
            }
            {
                let rac = page.get_row_and_cell(9, y);
                page.append_grapheme(unsafe { &mut *rac.row }, unsafe { &mut *rac.cell }, 0x0A)
                    .unwrap();
            }
        }
        assert_eq!(2usize, page.grapheme_count());

        // Clone
        let mut page2 = Page::init(Capacity {
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();
        {
            let dst = page2.get_row(0);
            let src = page.get_row(0);
            page2
                .clone_partial_row_from(&page, dst, src, 2, 8)
                .unwrap();
        }

        // Read it again
        {
            let y = 0;
            for x in 0..page2.size.cols as usize {
                let expected: u21 = if x >= 2 && x < 8 { (x + 1) as u21 } else { 0 };
                let rac = page2.get_row_and_cell(x, y);
                assert_eq!(expected, unsafe { (*rac.cell).content_codepoint() });
                assert!(!unsafe { (*rac.cell).has_grapheme() });
            }
            {
                let rac = page2.get_row_and_cell(9, y);
                assert!(!unsafe { (*rac.row).grapheme() });
            }
        }
        assert_eq!(0usize, page2.grapheme_count());

        page2.deinit();
        page.deinit();
    }

    #[test]
    fn page_cloneRowFrom_partial_grapheme_in_non_copied_dest_region() {
        let mut page = Page::init(Capacity {
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();

        // Write
        {
            let y = 0;
            for x in 0..page.size.cols as usize {
                let rac = page.get_row_and_cell(x, y);
                unsafe {
                    *rac.cell = cell_codepoint((x + 1) as u21);
                }
            }
        }
        assert_eq!(0usize, page.grapheme_count());

        // Clone
        let mut page2 = Page::init(Capacity {
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();
        {
            let y = 0;
            for x in 0..page2.size.cols as usize {
                let rac = page2.get_row_and_cell(x, y);
                unsafe {
                    *rac.cell = cell_codepoint(0xBB);
                }
            }
            {
                let rac = page2.get_row_and_cell(0, y);
                page2
                    .append_grapheme(unsafe { &mut *rac.row }, unsafe { &mut *rac.cell }, 0x0A)
                    .unwrap();
            }
            {
                let rac = page2.get_row_and_cell(9, y);
                page2
                    .append_grapheme(unsafe { &mut *rac.row }, unsafe { &mut *rac.cell }, 0x0A)
                    .unwrap();
            }
        }
        {
            let dst = page2.get_row(0);
            let src = page.get_row(0);
            page2
                .clone_partial_row_from(&page, dst, src, 2, 8)
                .unwrap();
        }

        // Read it again
        {
            let y = 0;
            for x in 0..page2.size.cols as usize {
                let expected: u21 = if x >= 2 && x < 8 { (x + 1) as u21 } else { 0xBB };
                let rac = page2.get_row_and_cell(x, y);
                assert_eq!(expected, unsafe { (*rac.cell).content_codepoint() });
            }
            {
                let rac = page2.get_row_and_cell(9, y);
                assert!(unsafe { (*rac.row).grapheme() });
            }
        }
        assert_eq!(2usize, page2.grapheme_count());

        page2.deinit();
        page.deinit();
    }

    #[test]
    fn page_cloneRowFrom_partial_hyperlink_in_same_page_copy() {
        let mut page = Page::init(Capacity::new(10, 10)).unwrap();

        // We need to create a hyperlink.
        let hyperlink_id = {
            let page_ptr: *mut Page = &mut page;
            page.hyperlink_set.context.set_page(page_ptr);
            page.hyperlink_set.context.set_src_page(None);
            unsafe {
                page.hyperlink_set.add(
                    page.memory_ptr(),
                    hyperlink::PageEntry {
                        id: hyperlink::PageEntryId::Implicit(0),
                        uri: OffsetSlice::default(),
                    },
                )
            }
            .unwrap()
        };

        // Write
        {
            let y = 0;
            for x in 0..page.size.cols as usize {
                let rac = page.get_row_and_cell(x, y);
                unsafe {
                    *rac.cell = cell_codepoint((x + 1) as u21);
                }
            }

            // Hyperlink in a single cell
            {
                let rac = page.get_row_and_cell(7, y);
                page.set_hyperlink(
                    unsafe { &mut *rac.row },
                    unsafe { &mut *rac.cell },
                    hyperlink_id,
                )
                .unwrap();
            }
        }
        assert_eq!(1usize, page.hyperlink_count());

        // Clone into the same page
        {
            let dst: *mut Row = page.get_row(1);
            let src: *const Row = page.get_row(0);
            let page_ptr: *mut Page = &mut page;
            unsafe {
                (*page_ptr)
                    .clone_partial_row_from(page_ptr, &mut *dst, &*src, 2, 8)
                    .unwrap();
            }
        }

        // Read it again
        {
            let y = 1;
            for x in 0..page.size.cols as usize {
                let expected: u21 = if x >= 2 && x < 8 { (x + 1) as u21 } else { 0 };
                let rac = page.get_row_and_cell(x, y);
                assert_eq!(expected, unsafe { (*rac.cell).content_codepoint() });
            }
            {
                let rac = page.get_row_and_cell(7, y);
                assert!(unsafe { (*rac.row).hyperlink() });
                assert!(unsafe { (*rac.cell).hyperlink() });
            }
        }
        assert_eq!(2usize, page.hyperlink_count());

        page.deinit();
    }

    /// Regression test (rustty-only, parity): a same-page cell carrying BOTH
    /// a hyperlink and a style must keep its style through
    /// `clone_partial_row_from`. The reference's `break :hyperlink`
    /// (page.zig:926) exits only the labeled hyperlink block, so the style
    /// arm still runs for that cell; a prior port `continue`d past it on the
    /// same-page fast path, dropping the style copy/refcount.
    #[test]
    fn page_cloneRowFrom_partial_hyperlink_and_style_in_same_page() {
        let mut page = Page::init(Capacity {
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();

        // We need to create a hyperlink.
        let hyperlink_id = {
            let page_ptr: *mut Page = &mut page;
            page.hyperlink_set.context.set_page(page_ptr);
            page.hyperlink_set.context.set_src_page(None);
            unsafe {
                page.hyperlink_set.add(
                    page.memory_ptr(),
                    hyperlink::PageEntry {
                        id: hyperlink::PageEntryId::Implicit(0),
                        uri: OffsetSlice::default(),
                    },
                )
            }
            .unwrap()
        };

        // And a style.
        let style_id = {
            let base = page.memory_ptr();
            unsafe {
                page.styles.add(
                    base,
                    Style {
                        flags: stylepkg::Flags {
                            bold: true,
                            ..stylepkg::Flags::default()
                        },
                        ..Style::default()
                    },
                )
            }
            .unwrap()
        };

        // Write, with the hyperlink AND the style on the same cell.
        {
            let y = 0;
            for x in 0..page.size.cols as usize {
                let rac = page.get_row_and_cell(x, y);
                unsafe {
                    *rac.cell = cell_codepoint((x + 1) as u21);
                }
            }
            {
                let rac = page.get_row_and_cell(7, y);
                page.set_hyperlink(
                    unsafe { &mut *rac.row },
                    unsafe { &mut *rac.cell },
                    hyperlink_id,
                )
                .unwrap();
                let base = page.memory_ptr();
                unsafe {
                    (*rac.row).set_styled(true);
                    (*rac.cell).set_style_id(style_id);
                    page.styles.use_id(base, style_id);
                }
            }
        }
        assert_eq!(1usize, page.hyperlink_count());

        // Clone into the same page
        {
            let dst: *mut Row = page.get_row(1);
            let src: *const Row = page.get_row(0);
            let page_ptr: *mut Page = &mut page;
            unsafe {
                (*page_ptr)
                    .clone_partial_row_from(page_ptr, &mut *dst, &*src, 2, 8)
                    .unwrap();
            }
        }

        // The cloned cell must carry BOTH the hyperlink and the style.
        {
            let rac = page.get_row_and_cell(7, 1);
            assert!(unsafe { (*rac.row).hyperlink() });
            assert!(unsafe { (*rac.cell).hyperlink() });
            assert!(unsafe { (*rac.row).styled() });
            assert_eq!(style_id, unsafe { (*rac.cell).style_id() });
        }
        assert_eq!(2usize, page.hyperlink_count());

        page.deinit();
    }

    #[test]
    fn page_cloneRowFrom_partial_hyperlink_in_same_page_omit() {
        let mut page = Page::init(Capacity::new(10, 10)).unwrap();

        // We need to create a hyperlink.
        let hyperlink_id = {
            let page_ptr: *mut Page = &mut page;
            page.hyperlink_set.context.set_page(page_ptr);
            page.hyperlink_set.context.set_src_page(None);
            unsafe {
                page.hyperlink_set.add(
                    page.memory_ptr(),
                    hyperlink::PageEntry {
                        id: hyperlink::PageEntryId::Implicit(0),
                        uri: OffsetSlice::default(),
                    },
                )
            }
            .unwrap()
        };

        // Write
        {
            let y = 0;
            for x in 0..page.size.cols as usize {
                let rac = page.get_row_and_cell(x, y);
                unsafe {
                    *rac.cell = cell_codepoint((x + 1) as u21);
                }
            }

            // Hyperlink in a single cell
            {
                let rac = page.get_row_and_cell(7, y);
                page.set_hyperlink(
                    unsafe { &mut *rac.row },
                    unsafe { &mut *rac.cell },
                    hyperlink_id,
                )
                .unwrap();
            }
        }
        assert_eq!(1usize, page.hyperlink_count());

        // Clone into the same page
        {
            let dst: *mut Row = page.get_row(1);
            let src: *const Row = page.get_row(0);
            let page_ptr: *mut Page = &mut page;
            unsafe {
                (*page_ptr)
                    .clone_partial_row_from(page_ptr, &mut *dst, &*src, 2, 6)
                    .unwrap();
            }
        }

        // Read it again
        {
            let y = 1;
            for x in 0..page.size.cols as usize {
                let expected: u21 = if x >= 2 && x < 6 { (x + 1) as u21 } else { 0 };
                let rac = page.get_row_and_cell(x, y);
                assert_eq!(expected, unsafe { (*rac.cell).content_codepoint() });
            }
            {
                let rac = page.get_row_and_cell(7, y);
                assert!(!unsafe { (*rac.row).hyperlink() });
                assert!(!unsafe { (*rac.cell).hyperlink() });
            }
        }
        assert_eq!(1usize, page.hyperlink_count());

        page.deinit();
    }

    #[test]
    fn page_moveCells_text_only() {
        let mut page = Page::init(Capacity {
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();

        // Write
        for x in 0..page.capacity.cols as usize {
            let rac = page.get_row_and_cell(x, 0);
            unsafe {
                *rac.cell = cell_codepoint((x + 1) as u21);
            }
        }

        let src = page.get_row(0);
        let dst = page.get_row(1);
        page.move_cells(
            unsafe { &mut *src },
            0,
            unsafe { &mut *dst },
            0,
            page.capacity.cols as usize,
        );

        // New rows should have text
        for x in 0..page.capacity.cols as usize {
            let rac = page.get_row_and_cell(x, 1);
            assert_eq!((x + 1) as u21, unsafe { (*rac.cell).content_codepoint() });
        }

        // Old row should be blank
        for x in 0..page.capacity.cols as usize {
            let rac = page.get_row_and_cell(x, 0);
            assert_eq!(0u32, unsafe { (*rac.cell).content_codepoint() });
        }

        page.deinit();
    }

    #[test]
    fn page_moveCells_graphemes() {
        let mut page = Page::init(Capacity {
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();

        // Write
        for x in 0..page.size.cols as usize {
            let rac = page.get_row_and_cell(x, 0);
            unsafe {
                *rac.cell = cell_codepoint((x + 1) as u21);
            }
            page.append_grapheme(unsafe { &mut *rac.row }, unsafe { &mut *rac.cell }, 0x0A)
                .unwrap();
        }
        let original_count = page.grapheme_count();

        let src = page.get_row(0);
        let dst = page.get_row(1);
        page.move_cells(
            unsafe { &mut *src },
            0,
            unsafe { &mut *dst },
            0,
            page.size.cols as usize,
        );
        assert_eq!(original_count, page.grapheme_count());

        // New rows should have text
        for x in 0..page.size.cols as usize {
            let rac = page.get_row_and_cell(x, 1);
            assert_eq!((x + 1) as u21, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(
                &[0x0Au32][..],
                page.lookup_grapheme(unsafe { &*rac.cell }).unwrap()
            );
        }

        // Old row should be blank
        for x in 0..page.size.cols as usize {
            let rac = page.get_row_and_cell(x, 0);
            assert_eq!(0u32, unsafe { (*rac.cell).content_codepoint() });
        }

        page.deinit();
    }

    #[test]
    fn page_verifyIntegrity_graphemes_good() {
        // Too slow, and not really necessary because the integrity tests are
        // only run in debug builds and unit tests verify they work well enough.
        // PORT NOTE: the reference also has
        //   if (std.valgrind.runningOnValgrind() > 0) return error.SkipZigTest;
        // there is no Valgrind-detection equivalent here; under native test /
        // Miri we always run the check (Miri is our integrity checker).
        let mut page = Page::init(Capacity {
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();

        // Write
        for x in 0..page.size.cols as usize {
            let rac = page.get_row_and_cell(x, 0);
            unsafe {
                *rac.cell = cell_codepoint((x + 1) as u21);
            }
            page.append_grapheme(unsafe { &mut *rac.row }, unsafe { &mut *rac.cell }, 0x0A)
                .unwrap();
        }

        page.verify_integrity().unwrap();

        page.deinit();
    }

    #[test]
    fn page_verifyIntegrity_grapheme_row_not_marked() {
        // Too slow, and not really necessary because the integrity tests are
        // only run in debug builds and unit tests verify they work well enough.
        // PORT NOTE: the reference also has
        //   if (std.valgrind.runningOnValgrind() > 0) return error.SkipZigTest;
        // there is no Valgrind-detection equivalent here; under native test /
        // Miri we always run the check (Miri is our integrity checker).
        let mut page = Page::init(Capacity {
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();

        // Write
        for x in 0..page.size.cols as usize {
            let rac = page.get_row_and_cell(x, 0);
            unsafe {
                *rac.cell = cell_codepoint((x + 1) as u21);
            }
            page.append_grapheme(unsafe { &mut *rac.row }, unsafe { &mut *rac.cell }, 0x0A)
                .unwrap();
        }

        // Make invalid by unmarking the row
        unsafe { (*page.get_row(0)).set_grapheme(false) };

        // S157-F1: `log.warn("page integrity violation y={} grapheme data but
        // row not marked", .{y})` (page.zig:565-568).
        let cap = crate::log_capture::start();
        assert_eq!(
            Err(IntegrityError::UnmarkedGraphemeRow),
            page.verify_integrity(),
        );
        let messages = cap.messages();
        assert!(
            messages.iter().any(|(level, target, msg)| *level == log::Level::Warn
                && target == "page"
                && msg.starts_with("page integrity violation y=0 grapheme data but row not marked")),
            "expected the grapheme-data-row-not-marked diagnostic, got: {messages:?}"
        );
        drop(cap);

        page.deinit();
    }

    #[test]
    fn page_verifyIntegrity_styles_good() {
        // Too slow, and not really necessary because the integrity tests are
        // only run in debug builds and unit tests verify they work well enough.
        // PORT NOTE: the reference also has
        //   if (std.valgrind.runningOnValgrind() > 0) return error.SkipZigTest;
        // there is no Valgrind-detection equivalent here; under native test /
        // Miri we always run the check (Miri is our integrity checker).
        let mut page = Page::init(Capacity {
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();

        let base = page.memory_ptr();
        // Upsert a style we'll use
        let id = unsafe {
            page.styles.add(
                base,
                Style {
                    flags: stylepkg::Flags {
                        bold: true,
                        ..stylepkg::Flags::default()
                    },
                    ..Style::default()
                },
            )
        }
        .unwrap();

        // Write
        for x in 0..page.size.cols as usize {
            let rac = page.get_row_and_cell(x, 0);
            unsafe {
                (*rac.row).set_styled(true);
                let mut c = cell_codepoint((x + 1) as u21);
                c.set_style_id(id);
                *rac.cell = c;
            }
            unsafe { page.styles.use_id(base, id) };
        }

        // The original style add would have incremented the
        // ref count too, so release it to balance that out.
        unsafe { page.styles.release(base, id) };

        page.verify_integrity().unwrap();

        page.deinit();
    }

    #[test]
    fn page_verifyIntegrity_styles_ref_count_mismatch() {
        // Too slow, and not really necessary because the integrity tests are
        // only run in debug builds and unit tests verify they work well enough.
        // PORT NOTE: the reference also has
        //   if (std.valgrind.runningOnValgrind() > 0) return error.SkipZigTest;
        // there is no Valgrind-detection equivalent here; under native test /
        // Miri we always run the check (Miri is our integrity checker).
        let mut page = Page::init(Capacity {
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();

        let base = page.memory_ptr();
        // Upsert a style we'll use
        let id = unsafe {
            page.styles.add(
                base,
                Style {
                    flags: stylepkg::Flags {
                        bold: true,
                        ..stylepkg::Flags::default()
                    },
                    ..Style::default()
                },
            )
        }
        .unwrap();

        // Write
        for x in 0..page.size.cols as usize {
            let rac = page.get_row_and_cell(x, 0);
            unsafe {
                (*rac.row).set_styled(true);
                let mut c = cell_codepoint((x + 1) as u21);
                c.set_style_id(id);
                *rac.cell = c;
            }
            unsafe { page.styles.use_id(base, id) };
        }

        // The original style add would have incremented the
        // ref count too, so release it to balance that out.
        unsafe { page.styles.release(base, id) };

        // Miss a ref
        unsafe { page.styles.release(base, id) };

        // S157-F1: `log.warn("page integrity violation style ref count
        // mismatch id={} expected={} actual={}", .{...})` (page.zig:565-568
        // — the style-ref-count arm).
        let cap = crate::log_capture::start();
        assert_eq!(
            Err(IntegrityError::MismatchedStyleRef),
            page.verify_integrity(),
        );
        let messages = cap.messages();
        assert!(
            messages.iter().any(|(level, target, msg)| *level == log::Level::Warn
                && target == "page"
                && msg.starts_with("page integrity violation style ref count mismatch id=")),
            "expected the style-ref-count-mismatch diagnostic, got: {messages:?}"
        );
        drop(cap);

        page.deinit();
    }

    #[test]
    fn page_verifyIntegrity_zero_rows() {
        // Too slow, and not really necessary because the integrity tests are
        // only run in debug builds and unit tests verify they work well enough.
        // PORT NOTE: the reference also has
        //   if (std.valgrind.runningOnValgrind() > 0) return error.SkipZigTest;
        // there is no Valgrind-detection equivalent here; under native test /
        // Miri we always run the check (Miri is our integrity checker).
        let mut page = Page::init(Capacity {
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();
        page.size.rows = 0;

        // S157-F1: `log.warn("page integrity violation zero row count", .{})`
        // (page.zig:406).
        let cap = crate::log_capture::start();
        assert_eq!(Err(IntegrityError::ZeroRowCount), page.verify_integrity());
        let messages = cap.messages();
        assert!(
            messages.iter().any(|(level, target, msg)| *level == log::Level::Warn
                && target == "page"
                && *msg == "page integrity violation zero row count"),
            "expected the zero-row-count integrity diagnostic, got: {messages:?}"
        );
        drop(cap);

        page.deinit();
    }

    // Task S-1c (no reference test): `assertIntegrity` logs the violation at
    // `err` level (page.zig:372) before it panics.
    #[test]
    #[cfg(debug_assertions)]
    fn page_assertIntegrity_logs_before_it_panics() {
        let mut page = Page::init(Capacity {
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();
        page.size.rows = 0;

        let cap = crate::log_capture::start();
        let panicked =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| page.assert_integrity()))
                .is_err();
        let messages = cap.messages();
        drop(cap);
        assert!(panicked, "an integrity violation must still crash");
        assert!(
            messages.contains(&(
                log::Level::Error,
                "page".to_string(),
                "page integrity violation, crashing. err=ZeroRowCount".to_string(),
            )),
            "expected the crashing diagnostic, got: {messages:?}"
        );

        page.size.rows = 10;
        page.deinit();
    }

    #[test]
    fn page_verifyIntegrity_zero_cols() {
        // Too slow, and not really necessary because the integrity tests are
        // only run in debug builds and unit tests verify they work well enough.
        // PORT NOTE: the reference also has
        //   if (std.valgrind.runningOnValgrind() > 0) return error.SkipZigTest;
        // there is no Valgrind-detection equivalent here; under native test /
        // Miri we always run the check (Miri is our integrity checker).
        let mut page = Page::init(Capacity {
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();
        page.size.cols = 0;

        // S157-F1: `log.warn("page integrity violation zero col count", .{})`
        // (page.zig:410).
        let cap = crate::log_capture::start();
        assert_eq!(Err(IntegrityError::ZeroColCount), page.verify_integrity());
        let messages = cap.messages();
        assert!(
            messages.iter().any(|(level, target, msg)| *level == log::Level::Warn
                && target == "page"
                && *msg == "page integrity violation zero col count"),
            "expected the zero-col-count integrity diagnostic, got: {messages:?}"
        );
        drop(cap);

        page.deinit();
    }

    #[test]
    fn page_exactRowCapacity_empty_rows() {
        let mut page = Page::init(Capacity {
            styles: 8,
            hyperlink_bytes: (32 * hyperlink_set_item_size()) as size::HyperlinkCountInt,
            string_bytes: 512,
            ..Capacity::new(10, 10)
        })
        .unwrap();

        // Empty page: all capacity fields should be 0 (except cols/rows)
        let cap = page.exact_row_capacity(0, 5);
        assert_eq!(10, cap.cols);
        assert_eq!(5, cap.rows);
        assert_eq!(0, cap.styles);
        assert_eq!(0, cap.grapheme_bytes);
        assert_eq!(0, cap.hyperlink_bytes);
        assert_eq!(0, cap.string_bytes);

        page.deinit();
    }

    #[test]
    fn page_exactRowCapacity_styles() {
        let mut page = Page::init(Capacity {
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();
        let base = page.memory_ptr();

        // No styles: capacity should be 0
        {
            let cap = page.exact_row_capacity(0, 5);
            assert_eq!(0, cap.styles);
        }

        // Add one style to a cell
        let style1_id = unsafe {
            page.styles.add(
                base,
                Style {
                    flags: stylepkg::Flags {
                        bold: true,
                        ..stylepkg::Flags::default()
                    },
                    ..Style::default()
                },
            )
        }
        .unwrap();
        {
            let rac = page.get_row_and_cell(0, 0);
            unsafe {
                (*rac.row).set_styled(true);
                (*rac.cell).set_style_id(style1_id);
            }
        }

        // One unique style - capacity accounts for load factor
        let cap_one_style = page.exact_row_capacity(0, 5);
        {
            assert_eq!(
                StyleSet::capacity_for_count(1) as size::StyleCountInt,
                cap_one_style.styles
            );
        }

        // Add same style to another cell (duplicate) - capacity unchanged
        {
            let rac = page.get_row_and_cell(1, 0);
            unsafe {
                (*rac.cell).set_style_id(style1_id);
            }
        }
        {
            let cap = page.exact_row_capacity(0, 5);
            assert_eq!(cap_one_style.styles, cap.styles);
        }

        // Add a different style
        let style2_id = unsafe {
            page.styles.add(
                base,
                Style {
                    flags: stylepkg::Flags {
                        italic: true,
                        ..stylepkg::Flags::default()
                    },
                    ..Style::default()
                },
            )
        }
        .unwrap();
        {
            let rac = page.get_row_and_cell(2, 0);
            unsafe {
                (*rac.cell).set_style_id(style2_id);
            }
        }

        // Two unique styles - capacity accounts for load factor
        let cap_two_styles = page.exact_row_capacity(0, 5);
        {
            assert_eq!(
                StyleSet::capacity_for_count(2) as size::StyleCountInt,
                cap_two_styles.styles
            );
            assert!(cap_two_styles.styles > cap_one_style.styles);
        }

        // Style outside the row range should not be counted
        {
            let rac = page.get_row_and_cell(0, 7);
            unsafe {
                (*rac.row).set_styled(true);
                let id = page
                    .styles
                    .add(
                        base,
                        Style {
                            flags: stylepkg::Flags {
                                underline: crate::sgr::Underline::Single,
                                ..stylepkg::Flags::default()
                            },
                            ..Style::default()
                        },
                    )
                    .unwrap();
                (*rac.cell).set_style_id(id);
            }
        }
        {
            let cap = page.exact_row_capacity(0, 5);
            assert_eq!(cap_two_styles.styles, cap.styles);
        }

        // Full range includes the new style
        {
            let cap = page.exact_row_capacity(0, 10);
            assert_eq!(
                StyleSet::capacity_for_count(3) as size::StyleCountInt,
                cap.styles
            );
        }

        // Verify clone works with exact capacity and produces same result
        {
            let cap = page.exact_row_capacity(0, 5);
            let mut cloned = Page::init(cap).unwrap();
            for y in 0..5 {
                let src_row = page.get_row(y);
                let dst_row = cloned.get_row(y);
                cloned
                    .clone_row_from(&page, dst_row, src_row)
                    .unwrap();
            }
            let cloned_cap = cloned.exact_row_capacity(0, 5);
            assert_eq!(cap, cloned_cap);
            cloned.deinit();
        }

        page.deinit();
    }

    #[test]
    fn page_exactRowCapacity_single_style_clone() {
        // Regression test: verify a single style can be cloned with exact capacity.
        // This tests that capacityForCount properly accounts for ID 0 being reserved.
        let mut page = Page::init(Capacity {
            styles: 8,
            ..Capacity::new(10, 2)
        })
        .unwrap();
        let base = page.memory_ptr();

        // Add exactly one style to row 0
        let style_id = unsafe {
            page.styles.add(
                base,
                Style {
                    flags: stylepkg::Flags {
                        bold: true,
                        ..stylepkg::Flags::default()
                    },
                    ..Style::default()
                },
            )
        }
        .unwrap();
        {
            let rac = page.get_row_and_cell(0, 0);
            unsafe {
                (*rac.row).set_styled(true);
                (*rac.cell).set_style_id(style_id);
            }
        }

        // exactRowCapacity for just row 0 should give capacity for 1 style
        let cap = page.exact_row_capacity(0, 1);
        assert_eq!(
            StyleSet::capacity_for_count(1) as size::StyleCountInt,
            cap.styles
        );

        // Create a new page with exact capacity and clone
        let mut cloned = Page::init(cap).unwrap();

        let src_row = page.get_row(0);
        let dst_row = cloned.get_row(0);

        // This must not fail with StyleSetOutOfMemory
        cloned
            .clone_row_from(&page, dst_row, src_row)
            .unwrap();

        // Verify the style was cloned correctly
        let cloned_cell = cloned.get_row_and_cell(0, 0);
        assert!(unsafe { (*cloned_cell.cell).style_id() } != stylepkg::DEFAULT_ID);

        cloned.deinit();
        page.deinit();
    }

    #[test]
    fn page_exactRowCapacity_styles_max_single_row() {
        let mut page = Page::init(Capacity {
            styles: size::StyleCountInt::MAX,
            ..Capacity::new(size::CellCountInt::MAX, 1)
        })
        .unwrap();
        let base = page.memory_ptr();

        // Style our first row
        unsafe { (*page.get_row(0)).set_styled(true) };

        // Fill cells with styles until we get OOM, but limit to a reasonable count
        // to avoid overflow when computing capacityForCount near maxInt
        let mut count: usize = 0;
        let max_count: usize = 1000; // Limit to avoid overflow in capacity calculation
        for i in 0..(page.size.cols as usize) {
            if count >= max_count {
                break;
            }
            let style_id = match unsafe {
                page.styles.add(
                    base,
                    Style {
                        fg_color: stylepkg::Color::Rgb(crate::color::RGB {
                            r: (i & 0xFF) as u8,
                            g: ((i >> 8) & 0xFF) as u8,
                            b: 0,
                        }),
                        ..Style::default()
                    },
                )
            } {
                Ok(id) => id,
                Err(_) => break,
            };
            let rac = page.get_row_and_cell(i, 0);
            unsafe {
                (*rac.cell).set_style_id(style_id);
            }
            count += 1;
        }

        // Verify we added a meaningful number of styles
        assert!(count > 0);

        // Capacity should be at least count (adjusted for load factor)
        let cap = page.exact_row_capacity(0, 1);
        assert_eq!(
            StyleSet::capacity_for_count(count) as size::StyleCountInt,
            cap.styles
        );

        page.deinit();
    }

    #[test]
    fn page_exactRowCapacity_grapheme_bytes() {
        let mut page = Page::init(Capacity {
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();

        // No graphemes: capacity should be 0
        {
            let cap = page.exact_row_capacity(0, 5);
            assert_eq!(0, cap.grapheme_bytes);
        }

        // Add one grapheme (1 codepoint) to a cell - rounds up to grapheme_chunk
        {
            let rac = page.get_row_and_cell(0, 0);
            unsafe {
                *rac.cell = Cell::init(b'a' as u21);
            }
            page.append_grapheme(unsafe { &mut *rac.row }, unsafe { &mut *rac.cell }, 0x0301)
                .unwrap(); // combining acute accent
        }
        {
            let cap = page.exact_row_capacity(0, 5);
            // 1 codepoint = 4 bytes, rounds up to grapheme_chunk (16)
            assert_eq!(GRAPHEME_CHUNK as size::GraphemeBytesInt, cap.grapheme_bytes);
        }

        // Add another grapheme to a different cell - should sum
        {
            let rac = page.get_row_and_cell(1, 0);
            unsafe {
                *rac.cell = Cell::init(b'e' as u21);
            }
            page.append_grapheme(unsafe { &mut *rac.row }, unsafe { &mut *rac.cell }, 0x0300)
                .unwrap(); // combining grave accent
        }
        {
            let cap = page.exact_row_capacity(0, 5);
            // 2 graphemes, each 1 codepoint = 2 * grapheme_chunk
            assert_eq!(
                (GRAPHEME_CHUNK * 2) as size::GraphemeBytesInt,
                cap.grapheme_bytes
            );
        }

        // Add a larger grapheme (multiple codepoints) that fits in one chunk
        {
            let rac = page.get_row_and_cell(2, 0);
            unsafe {
                *rac.cell = Cell::init(b'o' as u21);
            }
            page.append_grapheme(unsafe { &mut *rac.row }, unsafe { &mut *rac.cell }, 0x0301)
                .unwrap();
            page.append_grapheme(unsafe { &mut *rac.row }, unsafe { &mut *rac.cell }, 0x0302)
                .unwrap();
            page.append_grapheme(unsafe { &mut *rac.row }, unsafe { &mut *rac.cell }, 0x0303)
                .unwrap();
        }
        {
            let cap = page.exact_row_capacity(0, 5);
            // First two cells: 2 * grapheme_chunk
            // Third cell: 3 codepoints = 12 bytes, rounds up to grapheme_chunk
            assert_eq!(
                (GRAPHEME_CHUNK * 3) as size::GraphemeBytesInt,
                cap.grapheme_bytes
            );
        }

        // Grapheme outside the row range should not be counted
        {
            let rac = page.get_row_and_cell(0, 7);
            unsafe {
                *rac.cell = Cell::init(b'x' as u21);
            }
            page.append_grapheme(unsafe { &mut *rac.row }, unsafe { &mut *rac.cell }, 0x0304)
                .unwrap();
        }
        {
            let cap = page.exact_row_capacity(0, 5);
            assert_eq!(
                (GRAPHEME_CHUNK * 3) as size::GraphemeBytesInt,
                cap.grapheme_bytes
            );
        }

        // Full range includes the new grapheme
        {
            let cap = page.exact_row_capacity(0, 10);
            assert_eq!(
                (GRAPHEME_CHUNK * 4) as size::GraphemeBytesInt,
                cap.grapheme_bytes
            );
        }

        // Verify clone works with exact capacity and produces same result
        {
            let cap = page.exact_row_capacity(0, 5);
            let mut cloned = Page::init(cap).unwrap();
            for y in 0..5 {
                let src_row = page.get_row(y);
                let dst_row = cloned.get_row(y);
                cloned
                    .clone_row_from(&page, dst_row, src_row)
                    .unwrap();
            }
            let cloned_cap = cloned.exact_row_capacity(0, 5);
            assert_eq!(cap, cloned_cap);
            cloned.deinit();
        }

        page.deinit();
    }

    #[test]
    fn page_exactRowCapacity_grapheme_bytes_larger_than_chunk() {
        let mut page = Page::init(Capacity {
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();

        // Add a grapheme larger than one chunk (grapheme_chunk_len = 4 codepoints)
        let rac = page.get_row_and_cell(0, 0);
        unsafe {
            *rac.cell = Cell::init(b'a' as u21);
        }

        // Add 6 codepoints - requires 2 chunks (6 * 4 = 24 bytes, rounds up to 32)
        for i in 0..6u32 {
            page.append_grapheme(unsafe { &mut *rac.row }, unsafe { &mut *rac.cell }, 0x0300 + i)
                .unwrap();
        }

        let cap = page.exact_row_capacity(0, 1);
        // 6 codepoints = 24 bytes, alignForward(24, 16) = 32
        assert_eq!(32 as size::GraphemeBytesInt, cap.grapheme_bytes);

        // Verify clone works with exact capacity and produces same result
        let mut cloned = Page::init(cap).unwrap();
        let src_row = page.get_row(0);
        let dst_row = cloned.get_row(0);
        cloned
            .clone_row_from(&page, dst_row, src_row)
            .unwrap();
        let cloned_cap = cloned.exact_row_capacity(0, 1);
        assert_eq!(cap, cloned_cap);

        cloned.deinit();
        page.deinit();
    }

    #[test]
    fn page_exactRowCapacity_hyperlinks() {
        let mut page = Page::init(Capacity {
            styles: 8,
            hyperlink_bytes: (32 * hyperlink_set_item_size()) as size::HyperlinkCountInt,
            string_bytes: 512,
            ..Capacity::new(10, 10)
        })
        .unwrap();

        // No hyperlinks: capacity should be 0
        {
            let cap = page.exact_row_capacity(0, 5);
            assert_eq!(0, cap.hyperlink_bytes);
            assert_eq!(0, cap.string_bytes);
        }

        // Add one hyperlink with implicit ID
        let uri1 = "https://example.com";
        let id1 = {
            let rac = page.get_row_and_cell(0, 0);

            // Create and add hyperlink entry
            let id = page
                .insert_hyperlink(hyperlink::Hyperlink {
                    id: hyperlink::HyperlinkId::Implicit(1),
                    uri: uri1.as_bytes().to_vec(),
                })
                .unwrap();
            page.set_hyperlink(unsafe { &mut *rac.row }, unsafe { &mut *rac.cell }, id)
                .unwrap();
            id
        };
        // 1 hyperlink - capacity accounts for load factor
        let cap_one_link = page.exact_row_capacity(0, 5);
        {
            assert_eq!(
                (hyperlink::Set::capacity_for_count(1) * hyperlink_set_item_size())
                    as size::HyperlinkCountInt,
                cap_one_link.hyperlink_bytes
            );
            // URI "https://example.com" = 19 bytes, rounds up to string_chunk (32)
            assert_eq!(STRING_CHUNK as size::StringBytesInt, cap_one_link.string_bytes);
        }

        // Add same hyperlink to another cell (duplicate ID) - capacity unchanged
        {
            let rac = page.get_row_and_cell(1, 0);

            // Use the same hyperlink ID for another cell
            unsafe { page.hyperlink_set.use_id(page.memory_ptr(), id1) };
            page.set_hyperlink(unsafe { &mut *rac.row }, unsafe { &mut *rac.cell }, id1)
                .unwrap();
        }
        {
            let cap = page.exact_row_capacity(0, 5);
            assert_eq!(cap_one_link.hyperlink_bytes, cap.hyperlink_bytes);
            assert_eq!(cap_one_link.string_bytes, cap.string_bytes);
        }

        // Add a different hyperlink with explicit ID
        let uri2 = "https://other.example.org/path";
        let explicit_id = "my-link-id";
        {
            let rac = page.get_row_and_cell(2, 0);

            let id = page
                .insert_hyperlink(hyperlink::Hyperlink {
                    id: hyperlink::HyperlinkId::Explicit(explicit_id.as_bytes().to_vec()),
                    uri: uri2.as_bytes().to_vec(),
                })
                .unwrap();
            page.set_hyperlink(unsafe { &mut *rac.row }, unsafe { &mut *rac.cell }, id)
                .unwrap();
        }
        // 2 hyperlinks - capacity accounts for load factor
        let cap_two_links = page.exact_row_capacity(0, 5);
        {
            assert_eq!(
                (hyperlink::Set::capacity_for_count(2) * hyperlink_set_item_size())
                    as size::HyperlinkCountInt,
                cap_two_links.hyperlink_bytes
            );
            // First URI: 19 bytes -> 32, Second URI: 30 bytes -> 32, Explicit ID: 10 bytes -> 32
            assert_eq!(
                (STRING_CHUNK * 3) as size::StringBytesInt,
                cap_two_links.string_bytes
            );
        }

        // Hyperlink outside the row range should not be counted
        {
            let rac = page.get_row_and_cell(0, 7); // row 7 is outside range [0, 5)

            let id = page
                .insert_hyperlink(hyperlink::Hyperlink {
                    id: hyperlink::HyperlinkId::Implicit(99),
                    uri: "https://outside.example.com".as_bytes().to_vec(),
                })
                .unwrap();
            page.set_hyperlink(unsafe { &mut *rac.row }, unsafe { &mut *rac.cell }, id)
                .unwrap();
        }
        {
            let cap = page.exact_row_capacity(0, 5);
            assert_eq!(cap_two_links.hyperlink_bytes, cap.hyperlink_bytes);
            assert_eq!(cap_two_links.string_bytes, cap.string_bytes);
        }

        // Full range includes the new hyperlink
        {
            let cap = page.exact_row_capacity(0, 10);
            assert_eq!(
                (hyperlink::Set::capacity_for_count(3) * hyperlink_set_item_size())
                    as size::HyperlinkCountInt,
                cap.hyperlink_bytes
            );
            // Third URI: 27 bytes -> 32
            assert_eq!((STRING_CHUNK * 4) as size::StringBytesInt, cap.string_bytes);
        }

        // Verify clone works with exact capacity and produces same result
        {
            let cap = page.exact_row_capacity(0, 5);
            let mut cloned = Page::init(cap).unwrap();
            for y in 0..5 {
                let src_row = page.get_row(y);
                let dst_row = cloned.get_row(y);
                cloned
                    .clone_row_from(&page, dst_row, src_row)
                    .unwrap();
            }
            let cloned_cap = cloned.exact_row_capacity(0, 5);
            assert_eq!(cap, cloned_cap);
            cloned.deinit();
        }

        page.deinit();
    }

    #[test]
    fn page_exactRowCapacity_single_hyperlink_clone() {
        // Regression test: verify a single hyperlink can be cloned with exact capacity.
        // This tests that capacityForCount properly accounts for ID 0 being reserved.
        let mut page = Page::init(Capacity {
            styles: 8,
            hyperlink_bytes: (32 * hyperlink_set_item_size()) as size::HyperlinkCountInt,
            string_bytes: 512,
            ..Capacity::new(10, 2)
        })
        .unwrap();

        // Add exactly one hyperlink to row 0
        let uri = "https://example.com";
        let _id = {
            let rac = page.get_row_and_cell(0, 0);
            let link_id = page
                .insert_hyperlink(hyperlink::Hyperlink {
                    id: hyperlink::HyperlinkId::Implicit(1),
                    uri: uri.as_bytes().to_vec(),
                })
                .unwrap();
            page.set_hyperlink(unsafe { &mut *rac.row }, unsafe { &mut *rac.cell }, link_id)
                .unwrap();
            link_id
        };

        // exactRowCapacity for just row 0 should give capacity for 1 hyperlink
        let cap = page.exact_row_capacity(0, 1);
        assert_eq!(
            (hyperlink::Set::capacity_for_count(1) * hyperlink_set_item_size())
                as size::HyperlinkCountInt,
            cap.hyperlink_bytes
        );

        // Create a new page with exact capacity and clone
        let mut cloned = Page::init(cap).unwrap();

        let src_row = page.get_row(0);
        let dst_row = cloned.get_row(0);

        // This must not fail with HyperlinkSetOutOfMemory
        cloned
            .clone_row_from(&page, dst_row, src_row)
            .unwrap();

        // Verify the hyperlink was cloned correctly
        let cloned_cell = cloned.get_row_and_cell(0, 0);
        assert!(unsafe { (*cloned_cell.cell).hyperlink() });

        cloned.deinit();
        page.deinit();
    }

    #[test]
    fn page_exactRowCapacity_hyperlink_map_capacity_for_many_cells() {
        // A single hyperlink spanning many cells requires hyperlink_map capacity
        // based on cell count, not unique hyperlink count.
        let cols = 50;
        let mut page = Page::init(Capacity {
            styles: 8,
            hyperlink_bytes: (32 * hyperlink_set_item_size()) as size::HyperlinkCountInt,
            string_bytes: 512,
            ..Capacity::new(cols, 2)
        })
        .unwrap();

        // Add one hyperlink spanning all 50 columns in row 0
        let uri = "https://example.com";
        let id = {
            let rac = page.get_row_and_cell(0, 0);
            let link_id = page
                .insert_hyperlink(hyperlink::Hyperlink {
                    id: hyperlink::HyperlinkId::Implicit(1),
                    uri: uri.as_bytes().to_vec(),
                })
                .unwrap();
            page.set_hyperlink(unsafe { &mut *rac.row }, unsafe { &mut *rac.cell }, link_id)
                .unwrap();
            link_id
        };

        // Apply same hyperlink to remaining cells in row 0
        for x in 1..(cols as usize) {
            let rac = page.get_row_and_cell(x, 0);
            unsafe { page.hyperlink_set.use_id(page.memory_ptr(), id) };
            page.set_hyperlink(unsafe { &mut *rac.row }, unsafe { &mut *rac.cell }, id)
                .unwrap();
        }

        // exactRowCapacity must account for 50 hyperlink cells, not just 1 unique hyperlink
        let cap = page.exact_row_capacity(0, 1);

        // The hyperlink_bytes must be large enough that layout() computes sufficient
        // hyperlink_map capacity. With hyperlink_cell_multiplier=16, we need at least
        // ceil(50/16) = 4 hyperlink entries worth of bytes for the map.
        let min_for_map = (cols as usize).div_ceil(HYPERLINK_CELL_MULTIPLIER);
        let min_hyperlink_bytes = min_for_map * hyperlink_set_item_size();
        assert!(cap.hyperlink_bytes as usize >= min_hyperlink_bytes);

        // Create a new page with exact capacity and clone - must not fail
        let mut cloned = Page::init(cap).unwrap();

        let src_row = page.get_row(0);
        let dst_row = cloned.get_row(0);

        // This must not fail with HyperlinkMapOutOfMemory
        cloned
            .clone_row_from(&page, dst_row, src_row)
            .unwrap();

        // Verify all hyperlinks were cloned correctly
        for x in 0..(cols as usize) {
            let cloned_cell = cloned.get_row_and_cell(x, 0);
            assert!(unsafe { (*cloned_cell.cell).hyperlink() });
        }

        cloned.deinit();
        page.deinit();
    }

    // `test "Page layout avoids double rounding hyperlink map capacity"`
    #[test]
    fn page_layout_avoids_double_rounding_hyperlink_map_capacity() {
        let hyperlink_count = 3usize;
        let layout = Page::layout(Capacity {
            cols: 1,
            rows: 1,
            hyperlink_bytes: (hyperlink_count * hyperlink_set_item_size())
                as size::HyperlinkCountInt,
            ..std_capacity()
        });

        // Three set entries request 48 usable map entries. Scaling that for the
        // 80% load factor needs 60 raw slots, which rounds once to 64. Rounding
        // the request before applying the load factor would allocate 128 slots.
        assert_eq!(64u32, layout.hyperlink_map_layout.capacity);
    }

    /// `hyperlinkCapacity()` must report the map's **max load**, not its raw
    /// slot capacity (`7e14347c1`: `self.hyperlink_map.map(...).maxLoad()`).
    ///
    /// PORT NOTE: upstream ships no test for this hunk. It gets one here because
    /// the two values differ only by the 80% load factor, so a test that merely
    /// asserts "some capacity" — or compares against `hyperlink_map_layout
    /// .capacity` — passes whichever accessor is wired up. This asserts the
    /// exact max-load number AND that it is strictly below the raw capacity, so
    /// reverting `maxLoad()` to `capacity()` fails it.
    #[test]
    fn page_hyperlinkCapacity_reports_max_load_not_raw_capacity() {
        let hyperlink_count = 3usize;
        let cap = Capacity {
            cols: 1,
            rows: 1,
            hyperlink_bytes: (hyperlink_count * hyperlink_set_item_size())
                as size::HyperlinkCountInt,
            ..std_capacity()
        };
        let layout = Page::layout(cap);
        let mut page = Page::init(cap).unwrap();

        // 64 raw slots at an 80% load factor => 51 usable entries.
        let raw_capacity = layout.hyperlink_map_layout.capacity;
        assert_eq!(64u32, raw_capacity);
        assert_eq!(51usize, page.hyperlink_capacity());
        assert!(page.hyperlink_capacity() < raw_capacity as usize);

        page.deinit();
    }

    /// A shallow copy aliases the SAME backing mapping and never owns it.
    ///
    /// PORT NOTE (WS7 B2): the oracle expresses this as a plain Zig struct copy
    /// (`.page = source.*` in compress/Page.zig); rustty's `Page` is not `Copy`
    /// so the copy is an explicit helper and this test pins its contract.
    #[test]
    fn page_shallow_copy_aliases_the_same_mapping_and_is_unowned() {
        let mut page = Page::init(Capacity {
            cols: 10,
            rows: 8,
            ..std_capacity()
        })
        .unwrap();
        page.size = PageSize { cols: 9, rows: 7 };
        page.dirty = true;

        let copy = page.shallow_copy();
        assert_eq!(copy.memory_ptr(), page.memory_ptr());
        assert_eq!(copy.memory_len(), page.memory_len());
        assert_eq!(copy.size, page.size);
        assert_eq!(copy.capacity, page.capacity);
        assert!(copy.dirty);
        // Dropping `copy` must not free the mapping; `page.deinit()` below would
        // then double-free and the test allocator/leak gate would catch it.
        //
        // WS19 (audit row N11): `clippy::drop_non_drop` is right that `Page` is not
        // `Drop` and this is a no-op — but the no-op IS the point being asserted (that
        // the mapping is NOT freed here), and the comment above only makes sense next to
        // an explicit `drop`. Allowed at the statement with the reason.
        #[allow(clippy::drop_non_drop)]
        drop(copy);
        page.deinit();
    }

    // test "Mask" (446f80f4e, WS10 T19)
    #[test]
    fn mask() {
        type M = Mask<Cell, { cell_field_mask(&[CellField::ContentTag, CellField::StyleId]) }, 4>;

        let plain: Cell = Cell::init('A' as u32);
        let mut styled: Cell = Cell::init('B' as u32);
        styled.set_style_id(5);
        let mut styled2: Cell = Cell::init('C' as u32);
        styled2.set_style_id(5);
        let mut other: Cell = Cell::init('D' as u32);
        other.set_style_id(6);

        // match: plain cells only
        {
            let mut cells: [Cell; 4] = [plain, plain, plain, plain];
            assert!(M::match_group(&cells, 0));
            assert!(M::match_scalar(plain));

            cells[2] = styled;
            assert!(!M::match_group(&cells, 0));
            assert!(!M::match_scalar(styled));
        }

        // eql: runs of matching masked fields, other fields may vary
        {
            let expected = M::pattern(styled);
            let mut cells: [Cell; 4] = [styled, styled2, styled, styled2];
            assert!(M::eql(&cells, 0, expected));
            assert!(M::eql_scalar(styled2, expected));

            cells[1] = other;
            assert!(!M::eql(&cells, 0, expected));
            assert!(!M::eql_scalar(other, expected));
        }

        // eqlPrefix: count of leading values matching the pattern
        {
            let expected = M::pattern(styled);
            let mut cells: [Cell; 4] = [styled, styled2, other, styled];
            assert_eq!(2, M::eql_prefix(&cells, 0, expected));

            cells[2] = styled;
            assert_eq!(4, M::eql_prefix(&cells, 0, expected));
        }

        // eqlExact: bit-identical values only
        {
            let expected = M::bits(styled);
            let mut cells: [Cell; 4] = [styled, styled, styled, styled];
            assert!(M::eql_exact(&cells, 0, expected));

            // Same masked fields but different codepoint is not exact.
            cells[3] = styled2;
            assert!(!M::eql_exact(&cells, 0, expected));
        }

        // strip: compare values while ignoring the masked fields
        {
            let mut styled_other: Cell = Cell::init('B' as u32);
            styled_other.set_style_id(6);
            assert_eq!(M::strip(styled), M::strip(styled_other));
            assert!(M::strip(styled) != M::strip(styled2));
        }

        // eqlAny: presence of a matching value anywhere in the group
        {
            let expected = M::pattern(styled);
            let mut cells: [Cell; 4] = [plain, plain, plain, plain];
            assert!(!M::eql_any(&cells, 0, expected));

            cells[2] = styled;
            assert!(M::eql_any(&cells, 0, expected));

            // Masked compare: same masked fields with a different
            // codepoint still matches.
            cells[2] = styled2;
            assert!(M::eql_any(&cells, 0, expected));
        }
    }

    // test "Mask nested field path" (0fb3565c7)
    #[test]
    fn mask_nested_field_path() {
        // Mask only the codepoint data bits of the content field, not
        // the padding next to it or any other field.
        type M = Mask<Cell, { cell_field_mask(&[CellField::ContentCodepointData]) }, 4>;

        let a: Cell = Cell::init('A' as u32);
        let mut b: Cell = Cell::init('A' as u32);
        b.set_style_id(5);
        b.set_wide(Wide::Wide);
        let c: Cell = Cell::init('C' as u32);

        // Same codepoint matches regardless of other fields.
        let expected = M::pattern(a);
        assert!(M::eql_scalar(b, expected));
        assert!(!M::eql_scalar(c, expected));

        // The mask must cover exactly the codepoint data bits.
        let cp_offset = CELL_CONTENT_SHIFT;
        assert_eq!(
            (U21_MAX as u64) << cp_offset,
            cell_field_mask(&[CellField::ContentCodepointData]),
        );

        // Group variants
        {
            let mut cells: [Cell; 4] = [a, b, a, b];
            assert!(M::eql(&cells, 0, expected));
            assert!(M::eql_any(&cells, 0, expected));

            cells[1] = c;
            assert!(!M::eql(&cells, 0, expected));
            assert!(M::eql_any(&cells, 0, expected));

            let none: [Cell; 4] = [c, c, c, c];
            assert!(!M::eql_any(&none, 0, expected));
        }
    }

    /// W62-B06-001. The reference's group ops reduce a
    /// `@Vector(group_len, Backing)` with `@reduce`, which Zig rejects for a
    /// zero-lane vector; the array port must reject `GROUP_LEN == 0` the same
    /// way (see the `compile_fail` doctest on `Mask`) WITHOUT disturbing small
    /// positive group lengths. `GROUP_LEN = 1` is the boundary case: it is the
    /// smallest length the reference accepts, and it is used in the port
    /// (`render.rs` group_len 1), so it must keep computing real answers rather
    /// than tripping the nonzero assert.
    #[test]
    fn mask_group_len_one() {
        type M1 = Mask<Cell, { cell_field_mask(&[CellField::ContentTag, CellField::StyleId]) }, 1>;

        let plain: Cell = Cell::init('A' as u32);
        let mut styled: Cell = Cell::init('B' as u32);
        styled.set_style_id(5);
        let mut styled2: Cell = Cell::init('C' as u32);
        styled2.set_style_id(5);

        // match_group: content_tag is nonzero for a codepoint cell, so a plain
        // cell only matches the style-id half; use the style-id mask alone for
        // the "all masked bits zero" question.
        type MS = Mask<Cell, { cell_field_mask(&[CellField::StyleId]) }, 1>;
        assert!(MS::match_group(&[plain], 0));
        assert!(!MS::match_group(&[styled], 0));

        // eql / eql_prefix: masked fields equal, other fields may vary.
        let expected = M1::pattern(styled);
        assert!(M1::eql(&[styled2], 0, expected));
        assert_eq!(1, M1::eql_prefix(&[styled2], 0, expected));
        assert!(!M1::eql(&[plain], 0, expected));
        assert_eq!(0, M1::eql_prefix(&[plain], 0, expected));

        // eql_exact: bit-identical only.
        assert!(M1::eql_exact(&[styled], 0, M1::bits(styled)));
        assert!(!M1::eql_exact(&[styled2], 0, M1::bits(styled)));
    }

    /// W62-B04-001. The reference's `Cell.init(cp: u21)` cannot be handed a
    /// value above `maxInt(u21)`; the port's `u21 = u32` alias can, and the
    /// 24-bit content union would persist bits 21..23 — building a cell the
    /// reference cannot represent, whose backing bits are nonzero while
    /// `content_codepoint()`/`has_text()` read zero. `set_content_codepoint`
    /// therefore traps in safe builds, mirroring Zig's `@intCast` check.
    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "codepoint out of u21 domain")]
    fn cell_init_rejects_out_of_u21_codepoint() {
        // Exactly bit 21: fits the 24-bit content slot, so without the assert
        // it stores silently and yields `is_zero() == false` with
        // `codepoint() == 0`.
        let _ = Cell::init(0x20_0000);
    }

    /// Companion to `cell_init_rejects_out_of_u21_codepoint`: the largest
    /// in-domain codepoint must still round-trip untouched.
    #[test]
    fn cell_init_max_u21_round_trip() {
        let c = Cell::init(U21_MAX);
        assert_eq!(U21_MAX, c.content_codepoint());
        assert_eq!(U21_MAX, c.codepoint());
        assert!(c.has_text());
        assert!(!c.is_zero());

        // The largest real Unicode scalar, too.
        let c = Cell::init(0x10_FFFF);
        assert_eq!(0x10_FFFF, c.content_codepoint());
        assert_eq!(0x10_FFFF, c.codepoint());
        assert!(c.has_text());
    }

    /// `P3-W1 T4` (oracle c442eb4b3). A zero-row page would leave the pool's
    /// stale free-list node in the first pointer-size bytes of a REUSED buffer,
    /// because `init_buf`'s row loop — the only thing that overwrites those
    /// bytes — would iterate zero times. rustty's `decommit_pool_item` makes
    /// this live, and rustty skips the zeroing memset in release exactly as Zig
    /// does, so without this assert the corruption would be release-only and
    /// dependent on pool reuse order.
    #[test]
    #[should_panic(expected = "cap.rows > 0")]
    fn page_init_buf_rejects_zero_rows() {
        let _ = Page::init(Capacity {
            cols: 10,
            rows: 0,
            ..Capacity::new(10, 1)
        });
    }

    /// A deliberately page-UNaligned (but still 64-byte aligned, so every
    /// `Row`/`Cell`/set alignment inside the page is satisfied) buffer of `len`
    /// bytes carved out of a page-aligned allocation. Every user panics before
    /// returning, so the allocation is freed by `Drop` while the panic unwinds
    /// (it used to be leaked: 12,288 bytes definitely lost per run under
    /// G-VALGRIND, NEW-FINDINGS N418).
    struct PageUnalignedBuf {
        base: *mut u8,
        layout: std::alloc::Layout,
        len: usize,
    }

    impl PageUnalignedBuf {
        fn new(len: usize) -> PageUnalignedBuf {
            let layout =
                std::alloc::Layout::from_size_align(len + PAGE_SIZE_MIN, PAGE_SIZE_MIN).unwrap();
            // SAFETY: non-zero size; the returned block is PAGE_SIZE_MIN-aligned, so
            // `+64` is exactly one page-alignment violation and nothing else.
            let base = unsafe { std::alloc::alloc_zeroed(layout) };
            assert!(!base.is_null());
            assert_eq!(base as usize % PAGE_SIZE_MIN, 0);
            PageUnalignedBuf { base, layout, len }
        }

        fn as_mut_slice(&mut self) -> &mut [u8] {
            // SAFETY: `64 + len` bytes of the live allocation, borrowed from `self`.
            unsafe { core::slice::from_raw_parts_mut(self.base.add(64), self.len) }
        }
    }

    impl Drop for PageUnalignedBuf {
        fn drop(&mut self) {
            // SAFETY: `base` came from `alloc_zeroed(layout)` and is freed once.
            unsafe { std::alloc::dealloc(self.base, self.layout) };
        }
    }

    /// PORT NOTE: `cloneBuf(buf: []align(std.heap.page_size_min) u8)` — the
    /// reference's alignment contract is TYPE-level, so this call would not
    /// compile there. Rust's `&mut [u8]` carries no alignment, so the contract
    /// is an always-on assert; without it the clone silently produced a page
    /// whose backing memory is not page-aligned.
    #[test]
    #[should_panic(expected = "page_size_min-aligned")]
    fn page_cloneBuf_rejects_unaligned_buffer() {
        // var page = try Page.init(...); defer page.deinit();
        // The `defer` runs while the expected panic unwinds, after the buffer's
        // (declared later, dropped first), as Zig runs defers in reverse order
        // (review L-6 F7).
        let page = PageDeinitOnDrop(Page::init(Capacity::new(10, 10)).unwrap());
        let mut buf = PageUnalignedBuf::new(page.0.memory_len);
        let _ = page.0.clone_buf(buf.as_mut_slice());
    }

    /// `defer page.deinit()` for a test whose body is expected to panic.
    struct PageDeinitOnDrop(Page);

    impl Drop for PageDeinitOnDrop {
        fn drop(&mut self) {
            self.0.deinit();
        }
    }

    /// Rustty-only: `init_buf` requires zero-filled backing memory
    /// (`d2ff6d77a`: every metadata member initializes from zeroed memory
    /// without writing), and a debug build samples it. A page-aligned buffer
    /// with one dirty word at the start of the metadata block trips the
    /// sample; the same buffer zeroed builds a page. (`init_buf`'s callers are
    /// each audited to hand it zeroed memory: `init`, `reinit`, the PageList's
    /// `init_pages`, `create_page_ext`, the reflow recycle and the grow-prune
    /// reuse.)
    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "backing memory must be zero-filled (styles")]
    fn page_initBuf_rejects_dirty_metadata_memory() {
        struct PageAlignedBuf(*mut u8, std::alloc::Layout);
        impl Drop for PageAlignedBuf {
            fn drop(&mut self) {
                // SAFETY: allocated below with this layout, freed once.
                unsafe { std::alloc::dealloc(self.0, self.1) };
            }
        }
        let l = Page::layout(Capacity::new(10, 10));
        let layout = std::alloc::Layout::from_size_align(l.total_size, PAGE_SIZE_MIN).unwrap();
        // SAFETY: non-zero size.
        let buf = PageAlignedBuf(unsafe { std::alloc::alloc_zeroed(layout) }, layout);
        assert!(!buf.0.is_null());

        // Zeroed: a page.
        let _ = Page::init_buf(OffsetBuf::init(buf.0), l);

        // One dirty word where the style set's metadata starts.
        // SAFETY: `styles_start + 8 <= total_size`, inside the allocation.
        unsafe { (buf.0.add(l.styles_start) as *mut u64).write_unaligned(0xAAAA_AAAA) };
        let _ = Page::init_buf(OffsetBuf::init(buf.0), l);
    }

    /// PORT NOTE: `initBuf` stores `@alignCast(buf.start()[0..l.total_size])`,
    /// which traps in the reference's Debug/ReleaseSafe builds. A `+64` base is
    /// aligned for `Row`/`Cell`/the sets, so the pre-existing per-type
    /// `Offset::ptr` asserts did NOT catch it.
    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "page_size_min-aligned")]
    fn page_initBuf_rejects_unaligned_buffer() {
        let l = Page::layout(Capacity::new(10, 10));
        let mut buf = PageUnalignedBuf::new(l.total_size);
        let _ = Page::init_buf(OffsetBuf::init(buf.as_mut_slice().as_ptr()), l);
    }

    // PORT NOTE: the reference types the grapheme-tail APIs `u21`
    // (`setGraphemes(cps: []const u21)`, `appendGrapheme(cp: u21)`), so an
    // out-of-domain value cannot cross the API at all: a constant is a compile
    // error and a runtime `@intCast` traps in Debug/ReleaseSafe. Our `u21` alias
    // is `u32` and grapheme tails are stored verbatim in the page's grapheme
    // arena, so the domain is enforced by `debug_assert!` instead — these tests
    // (no reference twin; the type system is the reference's test) pin that.
    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "u21 domain")]
    fn page_appendGrapheme_rejects_out_of_u21_domain() {
        let mut page = Page::init(Capacity {
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();

        let rac = page.get_row_and_cell(0, 0);
        unsafe {
            *rac.cell = Cell::init(0x09);
        }

        // maxInt(u21) + 1
        let _ = page.append_grapheme(unsafe { &mut *rac.row }, unsafe { &mut *rac.cell }, 0x20_0000);
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "u21 domain")]
    fn page_setGraphemes_rejects_out_of_u21_domain() {
        let mut page = Page::init(Capacity {
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();

        let rac = page.get_row_and_cell(0, 0);
        unsafe {
            *rac.cell = Cell::init(0x09);
        }

        let _ = page.set_graphemes(
            unsafe { &mut *rac.row },
            unsafe { &mut *rac.cell },
            &[0x0Au32, 0x20_0000],
        );
    }

    /// The domain BOUNDARY is still accepted and round-trips unchanged: the
    /// assert must not narrow the reference's `u21` range (e.g. to Unicode's
    /// 0x10FFFF), since `u21` holds up to 0x1F_FFFF.
    #[test]
    fn page_grapheme_u21_domain_boundary_roundtrips() {
        let mut page = Page::init(Capacity {
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();

        let rac = page.get_row_and_cell(0, 0);
        unsafe {
            *rac.cell = Cell::init(0x09);
        }

        page.append_grapheme(unsafe { &mut *rac.row }, unsafe { &mut *rac.cell }, 0x10FFFF)
            .unwrap();
        page.append_grapheme(unsafe { &mut *rac.row }, unsafe { &mut *rac.cell }, 0x1F_FFFF)
            .unwrap();
        assert_eq!(
            &[0x10FFFFu32, 0x1F_FFFF][..],
            page.lookup_grapheme(unsafe { &*rac.cell }).unwrap()
        );

        let rac2 = page.get_row_and_cell(1, 0);
        unsafe {
            *rac2.cell = Cell::init(0x09);
        }
        page.set_graphemes(
            unsafe { &mut *rac2.row },
            unsafe { &mut *rac2.cell },
            &[0x1F_FFFFu32],
        )
        .unwrap();
        assert_eq!(
            &[0x1F_FFFFu32][..],
            page.lookup_grapheme(unsafe { &*rac2.cell }).unwrap()
        );

        page.deinit();
    }
}
