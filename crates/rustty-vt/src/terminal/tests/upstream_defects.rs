//! PORTS: ghostty src/terminal/Terminal.zig — the UD80 / UD131 style-capacity tests and the allocator routing tests.

use super::*;

// ---------------------------------------------------------------------
// UD80 (R13): a terminal wide enough to leave the standard page capacity
// gets `std_capacity` with only `cols` changed (`PageList.zig:535-540`),
// 215 rows x cols cells, so more than 65,535 cells can share one style on
// one page and the `u16` style refcount overflows. The port saturates the
// refcount and pins the style (see `ref_counted_set.rs`), so every cell
// keeps its style. These tests drive each native route to the ceiling.
//
// Width: 50,000 leaves the standard capacity in Ghostty and in the port,
// in every build measured. The first width that does is, for Ghostty,
// 47,919 shipped on macOS arm64, 46,895 shipped on Linux aarch64 and
// 48,034 in a macOS test build at `12752b2ac` (it depends on the
// platform's page size; 47,932 / 46,908 / 48,047 before `d2ff6d77a`);
// the port's are the same, since its style set is laid out with the
// reference's item geometry (N88). `ud80_scan`
// asserts the page really is the wide one, so the tests cannot go vacuous
// if those thresholds move. The page is 215 x 50,000, but only the few
// rows a test writes are touched. Count: the cursor holds one reference
// and each styled cell one more, so 65,534 styled cells bring the count
// to 65,535 and the 65,535th overflows it (measured in the reference);
// 70,000 (the reference probe's count) passes the ceiling by 4,466.
// Three screen rows hold 150,000 cells, which DECALN fills in one go.
const UD80_COLS: size::CellCountInt = 50_000;
const UD80_ROWS: size::CellCountInt = 3;
const UD80_PRINTS: usize = 70_000;

/// Walk the active area in reading order and return, for the cells whose
/// codepoint is `cp`, how many there are and whether each one carries
/// `style_id` resolving to a style for which `want` holds. Also returns
/// that style's refcount on the (single) page.
fn ud80_scan(
    t: &mut Terminal,
    cp: u21,
    style_id: style::Id,
    want: impl Fn(&style::Style) -> bool,
) -> (usize, bool, u16) {
    let rows = t.rows as u32;
    let mut n = 0usize;
    let mut all = true;
    let mut page_seen: *mut Page = core::ptr::null_mut();
    for y in 0..rows {
        let lc = t.active_mut().pages.get_cell(active_pt(0, y)).unwrap();
        let page = unsafe { crate::pagelist::Node::page(lc.node) };
        assert!(page_seen.is_null() || page_seen == page, "one page");
        let cap = unsafe { (*page).capacity };
        assert!(
            cap.rows as usize * cap.cols as usize > usize::from(u16::MAX),
            "the page is the wide, non-standard one"
        );
        page_seen = page;
        let cells = unsafe { &*(*page).get_cells(lc.row) };
        for c in cells {
            if c.content_codepoint() != cp {
                continue;
            }
            n += 1;
            if c.style_id() != style_id || style_id == style::DEFAULT_ID {
                all = false;
                continue;
            }
            let st = unsafe { &*(*page).styles.get((*page).memory_ptr(), c.style_id()) };
            all &= want(st);
        }
    }
    let rc = unsafe {
        (*page_seen)
            .styles
            .ref_count((*page_seen).memory_ptr(), style_id)
    };
    (n, all, rc)
}

fn ud80_cursor_style_id(t: &mut Terminal) -> style::Id {
    unsafe { (*t.screens.active).cursor.style_id }
}

// UD80 through `printCell` (`Terminal.zig:1722`, the `Terminal.print`
// API route): one style, 70,000 single prints, checked at the exact
// ceiling on the way.
#[test]
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION — BOTH LANES (rustty-vt, parity tail C-1; .port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound with debug assertions on (G6) and past an 8-minute bound with them off (G6c), both measured one test per process; runs natively in cargo test --workspace and under ASan (see MIRI_EXCEPTIONS.md)"
)]
fn ud80_wide_terminal_one_style_past_the_refcount_ceiling_keeps_every_cell_styled() {
    let mut t = Terminal::init(Global, opts(UD80_COLS, UD80_ROWS)).unwrap();
    t.set_attribute(sgr::Attribute::Bold).unwrap();
    for _ in 0..65_534 {
        t.print('A' as u32).unwrap();
    }
    let id = ud80_cursor_style_id(&mut t);
    let (_, _, rc) = ud80_scan(&mut t, 'A' as u21, id, |s| s.flags.bold);
    assert_eq!(
        rc,
        u16::MAX,
        "65,534 cells plus the cursor: the exact ceiling"
    );
    // The 65,535th styled cell is the one the reference overflows on.
    t.print('A' as u32).unwrap();
    for _ in 65_535..UD80_PRINTS {
        t.print('A' as u32).unwrap();
    }
    let (n, all, rc) = ud80_scan(&mut t, 'A' as u21, id, |s| s.flags.bold);
    assert_eq!(n, UD80_PRINTS, "every print landed");
    assert!(all, "every printed cell keeps the bold style");
    assert_eq!(rc, u16::MAX, "the refcount saturated and pinned");

    // Releasing references on a pinned style (overwrite, then erase) does
    // not underflow the count or free the style while cells use it.
    t.set_cursor_pos(1, 1);
    t.set_attribute(sgr::Attribute::Unset).unwrap();
    for _ in 0..10 {
        t.print('B' as u32).unwrap();
    }
    let (n, all, rc) = ud80_scan(&mut t, 'A' as u21, id, |s| s.flags.bold);
    assert_eq!(n, UD80_PRINTS - 10);
    assert!(all, "the remaining cells keep the bold style");
    assert_eq!(rc, u16::MAX, "a pinned style stays pinned on release");
    t.erase_display(csi::EraseDisplay::Complete, false);
    let (n, _, rc) = ud80_scan(&mut t, 'A' as u21, id, |s| s.flags.bold);
    assert_eq!(n, 0);
    assert_eq!(rc, u16::MAX);

    // The pinned style is reused by the next add of the same value.
    t.set_attribute(sgr::Attribute::Bold).unwrap();
    assert_eq!(ud80_cursor_style_id(&mut t), id);
    t.print('C' as u32).unwrap();
    let (n, all, _) = ud80_scan(&mut t, 'C' as u21, id, |s| s.flags.bold);
    assert_eq!((n, all), (1, true));

    t.deinit(Global);
}

// UD80 through the batched `print_slice` path (`printSliceFill`'s
// `useMultiple`, `Terminal.zig:1114`).
#[test]
fn ud80_wide_terminal_print_slice_past_the_refcount_ceiling_keeps_every_cell_styled() {
    let mut t = Terminal::init(Global, opts(UD80_COLS, UD80_ROWS)).unwrap();
    t.set_attribute(sgr::Attribute::Bold).unwrap();
    let chunk = vec!['A' as u32; 4096];
    let mut left = UD80_PRINTS;
    while left > 0 {
        let k = left.min(chunk.len());
        t.print_slice(&chunk[..k]).unwrap();
        left -= k;
    }
    let id = ud80_cursor_style_id(&mut t);
    let (n, all, rc) = ud80_scan(&mut t, 'A' as u21, id, |s| s.flags.bold);
    assert_eq!(n, UD80_PRINTS);
    assert!(all, "every printed cell keeps the bold style");
    assert_eq!(rc, u16::MAX);
    t.deinit(Global);
}

// UD80 through DECALN (`use_multiple(cols)` per row): three full rows.
#[test]
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound in the debug-assertions-ON G6 lane, where `Page::assert_integrity` runs on nearly every page mutation (measured per test; see the C-1 table there). It RUNS in the G6c lane (`check_leaks.sh --debug-assertions-off`). Do NOT widen this to a plain `miri` ignore: that would drop the test from BOTH lanes."
)]
fn ud80_wide_terminal_decaln_past_the_refcount_ceiling_keeps_every_cell_styled() {
    let mut t = Terminal::init(Global, opts(UD80_COLS, UD80_ROWS)).unwrap();
    let red = color::RGB {
        r: 0xFF,
        g: 0,
        b: 0,
    };
    t.set_attribute(sgr::Attribute::DirectColorBg(red)).unwrap();
    t.decaln().unwrap();
    let id = ud80_cursor_style_id(&mut t);
    let (n, all, rc) = ud80_scan(&mut t, 'E' as u21, id, |s| {
        s.bg_color.tag() == style::ColorTag::Rgb && s.bg_color.rgb() == red
    });
    assert_eq!(n, UD80_COLS as usize * UD80_ROWS as usize);
    assert!(all, "every DECALN cell keeps the background");
    assert_eq!(rc, u16::MAX);
    t.deinit(Global);
}

// UD80 through reflow: a resize re-wraps the saturated rows into new
// pages (`use_multiple` / `add_with_id` in the clone chain).
#[test]
fn ud80_wide_terminal_reflow_past_the_refcount_ceiling_keeps_every_cell_styled() {
    let mut t = Terminal::init(Global, opts(UD80_COLS, UD80_ROWS)).unwrap();
    t.set_attribute(sgr::Attribute::Bold).unwrap();
    for _ in 0..UD80_PRINTS {
        t.print('A' as u32).unwrap();
    }
    t.resize(
        Global,
        Resize {
            cols: UD80_COLS + 49,
            rows: UD80_ROWS,
            cell_size_px: None,
        },
    )
    .unwrap();
    let id = ud80_cursor_style_id(&mut t);
    let (n, all, rc) = ud80_scan(&mut t, 'A' as u21, id, |s| s.flags.bold);
    assert_eq!(n, UD80_PRINTS);
    assert!(all, "every reflowed cell keeps the bold style");
    assert_eq!(rc, u16::MAX);
    t.deinit(Global);
}

// UD80 through a page clone: distinct styles past the page's style
// capacity make `manualStyleUpdate` grow it (`increaseCapacity`), which
// clones every row, the saturated style included, into a new page
// (`add_with_id` / `use_multiple` in the clone chain). 110,000 prints fill
// two rows and start the third, so the clone's per-row `add_with_id` for
// the third row's first cell lands on an already-saturated style.
#[test]
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound in the debug-assertions-ON G6 lane, where `Page::assert_integrity` runs on nearly every page mutation (measured per test; see the C-1 table there). It RUNS in the G6c lane (`check_leaks.sh --debug-assertions-off`). Do NOT widen this to a plain `miri` ignore: that would drop the test from BOTH lanes."
)]
fn ud80_wide_terminal_capacity_clone_past_the_refcount_ceiling_keeps_every_cell_styled() {
    const PRINTS: usize = 110_000;
    let mut t = Terminal::init(Global, opts(UD80_COLS, UD80_ROWS)).unwrap();
    t.set_attribute(sgr::Attribute::Bold).unwrap();
    for _ in 0..PRINTS {
        t.print('A' as u32).unwrap();
    }
    let page_cap = |t: &mut Terminal| unsafe {
        let node = (*(*t.screens.active).cursor.page_pin).node;
        (*crate::pagelist::Node::page(node)).capacity.styles
    };
    let styles_before = page_cap(&mut t);
    for i in 0..300u32 {
        let fg = color::RGB {
            r: (i & 0xFF) as u8,
            g: (i >> 8) as u8,
            b: 1,
        };
        t.set_attribute(sgr::Attribute::DirectColorFg(fg)).unwrap();
        t.print('D' as u32).unwrap();
    }
    assert!(
        page_cap(&mut t) > styles_before,
        "the page was grown (cloned)"
    );
    t.set_attribute(sgr::Attribute::Unset).unwrap();
    t.set_attribute(sgr::Attribute::Bold).unwrap();
    let id = ud80_cursor_style_id(&mut t);
    let (n, all, rc) = ud80_scan(&mut t, 'A' as u21, id, |s| s.flags.bold);
    assert_eq!(n, PRINTS);
    assert!(all, "every cloned cell keeps the bold style");
    assert_eq!(rc, u16::MAX);
    t.deinit(Global);
}

// UD80 through the VT stream, the route an embedder's `vt_write` takes:
// SGR 1, then 70,000 printable bytes, as ordinary output. The stream hands
// runs of printables to `print_slice`, so this is the reference's
// `stream_terminal.zig:362` -> `printSliceFast` -> `printSliceFill` ->
// `useMultiple` route (`Terminal.zig:1114`, `ref_counted_set.zig:397`).
#[test]
fn ud80_wide_terminal_vt_stream_past_the_refcount_ceiling_keeps_every_cell_styled() {
    let mut t = Terminal::init(Global, opts(UD80_COLS, UD80_ROWS)).unwrap();
    {
        let mut stream = t.vt_stream();
        stream.next_slice(b"\x1b[1m");
        let chunk = [b'A'; 1000];
        for _ in 0..UD80_PRINTS / chunk.len() {
            stream.next_slice(&chunk);
        }
        stream.deinit();
    }
    let id = ud80_cursor_style_id(&mut t);
    let (n, all, rc) = ud80_scan(&mut t, 'A' as u21, id, |s| s.flags.bold);
    assert_eq!(n, UD80_PRINTS);
    assert!(all, "every printed cell keeps the bold style");
    assert_eq!(rc, u16::MAX);
    t.deinit(Global);
}

// ---------------------------------------------------------------------
// UD80 (R13), the style-capacity ceiling. A pinned style stays counted
// after its cells are overwritten (an "orphan"). Page clones recount from
// the cells and drop orphans, but the capacity projection counts them, so
// the style capacity can climb to `maxInt(u16)`; there growth returns
// `OutOfSpace`. The port re-clones the page once at the same capacity
// before the reference's split (`manualStyleUpdate`) or crash (the
// row-copy retry loops), when the set holds a pinned style.
//
// The review-R13W probe reached the ceiling organically on a 50,000 x 3
// terminal (a new RGB background plus DECALN per iteration) after 56,675
// iterations, about 8.5 billion cell writes. These tests reach the same
// state in a few iterations: they grow the style capacity straight to the
// ceiling with the reference test's own loop ("restoreCursor uses default
// style on OutOfSpace"), which skips the two projection jumps
// (128 -> 4,096 -> 65,535), and then fill the rest of the set with pinned
// styles no cell uses. Such a style is exactly what the probe accumulates:
// a count at 65,535 and no cell left.

/// Grow the style capacity of the page `node` returns to the `u16`
/// ceiling, the way the reference test does, and return that page.
fn ud80_presize_styles(
    t: &mut Terminal,
    node: impl Fn(&mut Terminal) -> *mut crate::pagelist::Node,
) -> *mut Page {
    loop {
        let n = node(t);
        if unsafe { crate::pagelist::Node::capacity(n).styles } == size::CellCountInt::MAX {
            return unsafe { crate::pagelist::Node::page(n) };
        }
        t.active_mut()
            .increase_capacity(n, Some(pagelist::grow::IncreaseCapacity::Styles))
            .unwrap();
    }
}

/// Fill `page`'s style set with pinned styles that no cell uses until
/// `add` fails. Each gets one reference from `add` and 65,534 more, which
/// pins it at 65,535. `tag` keeps the values distinct from the styles a
/// test sets itself. Once `add` has failed with `OutOfMemory` (the ids are
/// used up, or a probe sequence reached the last `psl_stats` bucket),
/// every later `add` of a new value fails the same way, which is the
/// state the probe reached.
///
/// NOTE (R13W2 review): this fabricated fill stops when it trips the
/// last `psl_stats` bucket, which its structured RGB values reach about
/// 189 ids BEFORE the ids are actually exhausted — not on true id
/// exhaustion as the organic probe does. Both surface the same
/// `AddError::OutOfMemory`, and the item states are the same kind (all
/// pinned orphans, no cells), so the mitigation exercises the same path;
/// only the failure trigger differs by those ~189 ids.
fn ud80_fill_with_orphans(page: *mut Page, tag: u8) -> usize {
    let mut n = 0u32;
    unsafe {
        (*page).pause_integrity_checks(true);
        loop {
            let base = (*page).memory_ptr();
            let st = style::Style {
                fg_color: style::Color::Rgb(color::RGB {
                    r: (n & 0xFF) as u8,
                    g: (n >> 8) as u8,
                    b: tag,
                }),
                ..Default::default()
            };
            let id = match (*page).styles.add(base, st) {
                Ok(id) => id,
                Err(e) => {
                    assert_eq!(e, crate::ref_counted_set::AddError::OutOfMemory);
                    break;
                }
            };
            (*page).styles.use_multiple(base, id, u16::MAX - 1);
            assert!((*page).styles.ref_count_saturated(base, id));
            n += 1;
        }
        (*page).pause_integrity_checks(false);
        (*page).assert_integrity();
    }
    n as usize
}

fn ud80_cursor_page(t: &mut Terminal) -> *mut Page {
    unsafe { crate::pagelist::Node::page((*(*t.screens.active).cursor.page_pin).node) }
}

// The review-R13W probe as a test: a new RGB background plus DECALN per
// iteration, past the iteration at which the unmitigated code starts
// failing every SGR. The first `REAL` iterations run at the ceiling, each
// DECALN pinning its new style and orphaning the previous one; then the
// rest of the set is filled with orphans, so iteration `REAL` finds the
// set full. Without the re-clone the page is split, the cursor (homed by
// DECALN) stays on the full half, and that SGR and every later one fail
// (`set_attribute` keeps the previous style).
#[test]
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION — BOTH LANES (rustty-vt, parity tail C-1; .port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound with debug assertions on (G6) and past an 8-minute bound with them off (G6c), both measured one test per process; runs natively in cargo test --workspace and under ASan (see MIRI_EXCEPTIONS.md)"
)]
fn ud80_wide_terminal_orphaned_pinned_styles_at_the_capacity_ceiling_are_reclaimed_not_split() {
    const REAL: u32 = 4;
    const PAST: u32 = 6;
    let mut t = Terminal::init(Global, opts(UD80_COLS, UD80_ROWS)).unwrap();
    let page = ud80_presize_styles(&mut t, |t| unsafe {
        (*(*t.screens.active).cursor.page_pin).node
    });

    let mut reclaimed_at = None;
    for i in 0..REAL + PAST {
        if i == REAL {
            assert_eq!(ud80_cursor_page(&mut t), page, "not cloned yet");
            let orphans = ud80_fill_with_orphans(page, 0x55);
            assert!(orphans > 50_000, "the set is full of orphans: {orphans}");
        }
        let bg = color::RGB {
            r: i as u8,
            g: 0x11,
            b: 0xAA,
        };
        let before = unsafe { (*ud80_cursor_page(&mut t)).styles.count() };
        t.set_attribute(sgr::Attribute::DirectColorBg(bg))
            .unwrap_or_else(|e| panic!("iteration {i}: SGR failed: {e:?}"));
        let cursor_bg = unsafe { (*t.screens.active).cursor.style.bg_color };
        assert!(
            cursor_bg.tag() == style::ColorTag::Rgb && cursor_bg.rgb() == bg,
            "iteration {i}: the cursor takes the new style"
        );
        let id = ud80_cursor_style_id(&mut t);
        assert_ne!(id, style::DEFAULT_ID, "iteration {i}");
        t.decaln().unwrap();

        let page = ud80_cursor_page(&mut t);
        let after = unsafe { (*page).styles.count() };
        if after < before {
            reclaimed_at.get_or_insert(i);
        }
        assert_eq!(
            t.active_mut().pages.total_pages(),
            1,
            "iteration {i}: no split"
        );
        assert_eq!(
            unsafe { (*page).capacity.styles },
            size::CellCountInt::MAX,
            "iteration {i}: still at the ceiling"
        );
        let (n, all, rc) = ud80_scan(&mut t, 'E' as u21, id, |s| {
            s.bg_color.tag() == style::ColorTag::Rgb && s.bg_color.rgb() == bg
        });
        assert_eq!(n, UD80_COLS as usize * UD80_ROWS as usize);
        assert!(
            all,
            "iteration {i}: every DECALN cell takes the new background"
        );
        assert_eq!(rc, u16::MAX, "iteration {i}");
    }
    // The re-clone ran exactly where the unmitigated code split, and
    // dropped every orphan: only the last DECALN style is left.
    assert_eq!(reclaimed_at, Some(REAL));
    assert!(unsafe { (*ud80_cursor_page(&mut t)).styles.count() } < 10);
    t.deinit(Global);
}

// The no-reclaim case: the set is full at the ceiling of styles that are
// genuinely in use (one pinned by 70,000 bold cells, the rest one cell
// each), so the re-clone reclaims nothing and the reference's split still
// happens, once per SGR, and returns.
#[test]
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION — BOTH LANES (rustty-vt, parity tail C-1; .port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound with debug assertions on (G6) and past an 8-minute bound with them off (G6c), both measured one test per process; runs natively in cargo test --workspace and under ASan (see MIRI_EXCEPTIONS.md)"
)]
fn ud80_wide_terminal_live_styles_at_the_capacity_ceiling_still_split() {
    let mut t = Terminal::init(Global, opts(UD80_COLS, UD80_ROWS)).unwrap();
    t.set_attribute(sgr::Attribute::Bold).unwrap();
    for _ in 0..UD80_PRINTS {
        t.print('A' as u32).unwrap();
    }
    let bold = ud80_cursor_style_id(&mut t);
    let page = ud80_presize_styles(&mut t, |t| unsafe {
        (*(*t.screens.active).cursor.page_pin).node
    });

    // One live style per cell after the cursor, until the set is full.
    let mut live = 0u32;
    unsafe {
        (*page).pause_integrity_checks(true);
        let base = (*page).memory_ptr();
        'fill: for y in 1..UD80_ROWS as usize {
            let row = (*page).get_row(y);
            let cells = &mut *(*page).get_cells(row);
            let start = if y == 1 {
                UD80_PRINTS - UD80_COLS as usize + 1
            } else {
                0
            };
            for c in &mut cells[start..] {
                let st = style::Style {
                    fg_color: style::Color::Rgb(color::RGB {
                        r: (live & 0xFF) as u8,
                        g: (live >> 8) as u8,
                        b: 0x77,
                    }),
                    ..Default::default()
                };
                let Ok(id) = (*page).styles.add(base, st) else {
                    break 'fill;
                };
                c.set_content_codepoint('L' as u21);
                c.set_style_id(id);
                (*row).set_styled(true);
                live += 1;
            }
        }
        (*page).pause_integrity_checks(false);
        (*page).assert_integrity();
        assert!((*page).styles.ref_count_saturated(base, bold));
    }
    assert!(live > 50_000, "the set is full of live styles: {live}");

    let count_l = |t: &mut Terminal| {
        let mut n = 0usize;
        for y in 0..UD80_ROWS as u32 {
            let lc = t.active_mut().pages.get_cell(active_pt(0, y)).unwrap();
            let page = unsafe { crate::pagelist::Node::page(lc.node) };
            for c in unsafe { &*(*page).get_cells(lc.row) } {
                if c.content_codepoint() == 'L' as u21 {
                    let st = unsafe { &*(*page).styles.get((*page).memory_ptr(), c.style_id()) };
                    assert_eq!(st.fg_color.rgb().b, 0x77, "a live style survived");
                    n += 1;
                }
            }
        }
        n
    };

    // The first new style: the re-clone reclaims nothing, so the page is
    // split, as in the reference.
    assert_eq!(t.active_mut().pages.total_pages(), 1);
    let red = color::RGB {
        r: 0xFF,
        g: 0,
        b: 0,
    };
    let r = t.set_attribute(sgr::Attribute::DirectColorBg(red));
    assert_eq!(t.active_mut().pages.total_pages(), 2, "the page was split");
    assert_eq!(count_l(&mut t), live as usize, "no live style was dropped");
    // The split moves the cursor's row, with row 2, to the new page. Both
    // rows' live styles go with it, so that page's set is full too and the
    // add fails: the reference's own "we tried" outcome, which keeps the
    // previous style (bold, no background).
    assert!(r.is_err());
    let cursor = unsafe { &(*t.screens.active).cursor };
    assert!(cursor.style.flags.bold);
    assert_eq!(cursor.style.bg_color.tag(), style::ColorTag::None);

    // And again: the call returns, one attempt per SGR, no loop.
    let blue = color::RGB {
        r: 0,
        g: 0,
        b: 0xFF,
    };
    let r = t.set_attribute(sgr::Attribute::DirectColorBg(blue));
    assert!(r.is_err());
    assert_eq!(count_l(&mut t), live as usize);
    t.deinit(Global);
}

// The row-copy retry loop (`Screen.clonePartialRowGrowCapacity`, reached
// by DL across a page boundary): the destination page's style set is full
// of orphaned pinned styles at the ceiling, and the row pulled into it
// carries a style it does not have. The reference's loop panics
// ("increaseCapacity OutOfSpace") here; the port re-clones once, which
// drops the orphans, and the copy succeeds.
#[test]
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION — BOTH LANES (rustty-vt, parity tail C-1; .port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound with debug assertions on (G6) and past an 8-minute bound with them off (G6c), both measured one test per process; runs natively in cargo test --workspace and under ASan (see MIRI_EXCEPTIONS.md)"
)]
fn ud80_delete_lines_into_a_page_full_of_orphaned_pinned_styles_reclaims_them() {
    let mut t = Terminal::init(Global, opts(4000, 5)).unwrap();
    let first_page_rows =
        unsafe { crate::pagelist::Node::capacity(t.active_mut().pages.pages.first).rows };
    for _ in 0..(first_page_rows + 1) {
        t.linefeed().unwrap();
    }
    assert_eq!(2, unsafe {
        crate::pagelist::Node::rows(t.active_mut().pages.pages.last)
    });
    // The first row of the second page (active y = 3) carries a style.
    t.set_cursor_pos(4, 1);
    t.set_attribute(sgr::Attribute::Bold).unwrap();
    t.print_string(b"S").unwrap();
    t.set_attribute(sgr::Attribute::Unset).unwrap();
    t.set_cursor_pos(1, 1);

    let page = ud80_presize_styles(&mut t, |t| t.active_mut().pages.pages.first);
    assert!(ud80_fill_with_orphans(page, 0x33) > 50_000);

    t.delete_lines(1);
    let lc = t.active_mut().pages.get_cell(active_pt(0, 2)).unwrap();
    let page = unsafe { crate::pagelist::Node::page(lc.node) };
    assert!(std::ptr::eq(lc.node, t.active_mut().pages.pages.first));
    assert_eq!(unsafe { (*lc.cell).content_codepoint() }, 'S' as u21);
    let st = unsafe {
        &*(*page)
            .styles
            .get((*page).memory_ptr(), (*lc.cell).style_id())
    };
    assert!(st.flags.bold, "the copied cell keeps its style");
    assert!(
        unsafe { (*page).styles.count() } < 10,
        "the orphans were dropped"
    );
    // The re-clone is at the SAME capacity: the ceiling is unchanged
    // (a smaller-capacity re-clone would still drop the orphans but is a
    // divergence the reclaim asserts alone do not catch; R13W2 review).
    assert_eq!(
        unsafe { (*page).capacity.styles },
        size::CellCountInt::MAX,
        "the style capacity stays at the ceiling"
    );
    t.deinit(Global);
}

// UD131 (R13) through REAL VT input, one test per route. RELEASE ONLY:
//
//     cargo test -p rustty-vt --release --lib -- --ignored ud131_vt_release
//
// Each case takes well under a second in release; in a debug build the
// per-write page integrity check walks the whole 50,000 x 215 page and a
// case does not finish in ten minutes, which is why the unit tests of the
// two loops fabricate the full set instead. In a debug build these tests
// return at once.
//
// The recipe is the reference probe's (UPSTREAM-DEFECTS.md UD131): a
// 50,000-column terminal, `fills` cells each with a distinct SGR 38;2
// colour (53,247 fill the page's style set with live styles at the `u16`
// ceiling), one cell 'Y' with a new style (background 1;2;3) on a row of
// another page, then the trigger that copies that row onto the full page.
// The reference `@panic`s at `Screen.zig:747` for DL and IL and at
// `PageList.zig:5108` for the alternate-screen LF and SU and the
// primary-screen scroll region (task R13-X's probe, re-measured by the
// R13-X review). The port keeps 'Y', drops its unstorable style to the
// default, and logs the degrade once.
//
// NEGATIVE CONTROLS (task R13-X2, macOS 2026-09-23, release): restoring
// the reference's `@panic("increaseCapacity OutOfSpace")` in the Screen
// loop's `OutOfSpace` arm fails exactly the DL and IL tests; restoring it
// in the PageList loop's arm fails exactly the alternate-screen LF, SU and
// scroll-region tests.
struct Ud131VtCase {
    rows: size::CellCountInt,
    fills: u32,
    pre: &'static [u8],
    fill_row: u32,
    y_row: u32,
    trigger: &'static [u8],
}

/// Runs one case; returns 'Y''s background (`None` when it degraded to
/// the default style) and the UD131 log records.
fn ud131_vt_run(c: &Ud131VtCase) -> (Option<String>, Vec<String>) {
    let mut t = Terminal::init(Global, opts(50_000, c.rows)).unwrap();
    let cap = crate::log_capture::start();
    {
        let mut st = t.vt_stream();
        st.next_slice(c.pre);
        st.next_slice(format!("\x1b[{};1H", c.fill_row).as_bytes());
        let mut buf = Vec::with_capacity(1 << 20);
        for i in 0..c.fills {
            // Distinct 24-bit colours (an odd multiplier is a bijection).
            let v = i.wrapping_add(1).wrapping_mul(2_654_435_761) & 0xFF_FFFF;
            buf.extend_from_slice(
                format!("\x1b[38;2;{};{};{}mX", v >> 16, (v >> 8) & 255, v & 255).as_bytes(),
            );
        }
        buf.extend_from_slice(b"\x1b[0m");
        st.next_slice(&buf);
        st.next_slice(format!("\x1b[{};1H\x1b[48;2;1;2;3mY\x1b[0m", c.y_row).as_bytes());
        st.next_slice(c.trigger);
        st.deinit();
    }
    let logs: Vec<String> = cap
        .messages()
        .into_iter()
        .filter(|(_, _, m)| m.contains("UD131"))
        .map(|(_, _, m)| m)
        .collect();
    drop(cap);

    let mut y_bg = None;
    let mut found = false;
    for y in 0..c.rows as u32 {
        let lc = t.active_mut().pages.get_cell(active_pt(0, y)).unwrap();
        if unsafe { (*lc.cell).content_codepoint() } != 'Y' as u21 {
            continue;
        }
        found = true;
        let page = unsafe { crate::pagelist::Node::page(lc.node) };
        let sid = unsafe { (*lc.cell).style_id() };
        if sid != crate::style::DEFAULT_ID {
            let s = unsafe { *(*page).styles.get((*page).memory_ptr(), sid) };
            y_bg = Some(format!("{:?}", s.bg_color));
        }
        break;
    }
    assert!(found, "the 'Y' cell survives the copy");

    // `assert_integrity` is a no-op without `debug_assertions`, and these
    // tests run in RELEASE, so call `verify_integrity` (whose body runs in
    // every build) directly and assert it on each page (R13-X3, review N1).
    let mut node = t.active_mut().pages.pages.first;
    while !node.is_null() {
        let page = unsafe { &*crate::pagelist::Node::page(node) };
        assert_eq!(page.verify_integrity(), Ok(()), "page integrity holds");
        node = unsafe { (*node).next };
    }
    t.deinit(Global);
    (y_bg, logs)
}

fn ud131_vt_expect_degraded(c: Ud131VtCase) {
    // R14: these tests are #[ignore]d; asked for in a debug build they FAIL
    // with how to run them instead of returning `ok` having run nothing.
    assert!(
        !cfg!(debug_assertions),
        "ud131_vt_release: release only (a debug build does not finish); run \
         cargo test -p rustty-vt --release --lib -- --ignored ud131_vt_release"
    );
    let (bg, logs) = ud131_vt_run(&c);
    assert_eq!(bg, None, "'Y''s new style degrades to the default");
    assert_eq!(
        logs,
        vec![
            "row copy at a page capacity ceiling: 1 cell part(s) degraded to default \
             (styles=1, hyperlinks=0, graphemes=0); see UD131"
                .to_string()
        ],
        "one degraded copy, logged once"
    );
}

const UD131_FULL: u32 = 53_247;

// DL at the bottom row: `deleteLines` -> the Screen loop.
#[test]
#[ignore = "release only: cargo test -p rustty-vt --release --lib -- --ignored ud131_vt_release"]
fn ud131_vt_release_delete_lines_degrades_at_the_screen_site() {
    ud131_vt_expect_degraded(Ud131VtCase {
        rows: 216,
        fills: UD131_FULL,
        pre: b"",
        fill_row: 2,
        y_row: 216,
        trigger: b"\x1b[215;1H\x1b[M",
    });
}

// IL: `insertLines` -> the Screen loop.
#[test]
#[ignore = "release only: cargo test -p rustty-vt --release --lib -- --ignored ud131_vt_release"]
fn ud131_vt_release_insert_lines_degrades_at_the_screen_site() {
    ud131_vt_expect_degraded(Ud131VtCase {
        rows: 430,
        fills: UD131_FULL,
        pre: b"",
        fill_row: 217,
        y_row: 215,
        trigger: b"\x1b[215;1H\x1b[L",
    });
}

// Alternate-screen LF at the bottom: `index` -> `eraseRowBounded` -> the
// PageList loop.
#[test]
#[ignore = "release only: cargo test -p rustty-vt --release --lib -- --ignored ud131_vt_release"]
fn ud131_vt_release_alt_screen_linefeed_degrades_at_the_pagelist_site() {
    ud131_vt_expect_degraded(Ud131VtCase {
        rows: 216,
        fills: UD131_FULL,
        pre: b"\x1b[?1049h",
        fill_row: 2,
        y_row: 216,
        trigger: b"\x1b[216;1H\n",
    });
}

// Alternate-screen SU: `scrollUp` -> `eraseRow` -> the PageList loop.
#[test]
#[ignore = "release only: cargo test -p rustty-vt --release --lib -- --ignored ud131_vt_release"]
fn ud131_vt_release_alt_screen_scroll_up_degrades_at_the_pagelist_site() {
    ud131_vt_expect_degraded(Ud131VtCase {
        rows: 216,
        fills: UD131_FULL,
        pre: b"\x1b[?1049h",
        fill_row: 2,
        y_row: 216,
        trigger: b"\x1b[S",
    });
}

// Primary-screen scroll region with a top margin, LF at its bottom:
// `index` -> `eraseRowBounded` -> the PageList loop.
#[test]
#[ignore = "release only: cargo test -p rustty-vt --release --lib -- --ignored ud131_vt_release"]
fn ud131_vt_release_scroll_region_linefeed_degrades_at_the_pagelist_site() {
    ud131_vt_expect_degraded(Ud131VtCase {
        rows: 216,
        fills: UD131_FULL,
        pre: b"",
        fill_row: 3,
        y_row: 216,
        trigger: b"\x1b[2;216r\x1b[216;1H\n",
    });
}

// Boundary control: one fill fewer (53,246) leaves room for 'Y''s style,
// so the same DL keeps it and logs nothing (the reference survives this
// too).
#[test]
#[ignore = "release only: cargo test -p rustty-vt --release --lib -- --ignored ud131_vt_release"]
fn ud131_vt_release_one_style_below_the_ceiling_keeps_the_style() {
    // R14: these tests are #[ignore]d; asked for in a debug build they FAIL
    // with how to run them instead of returning `ok` having run nothing.
    assert!(
        !cfg!(debug_assertions),
        "ud131_vt_release: release only (a debug build does not finish); run \
         cargo test -p rustty-vt --release --lib -- --ignored ud131_vt_release"
    );
    let (bg, logs) = ud131_vt_run(&Ud131VtCase {
        rows: 216,
        fills: UD131_FULL - 1,
        pre: b"",
        fill_row: 2,
        y_row: 216,
        trigger: b"\x1b[215;1H\x1b[M",
    });
    assert!(
        bg.as_deref()
            .is_some_and(|s| s.contains('1') && s.contains('2') && s.contains('3')),
        "'Y' keeps its 1;2;3 background: {bg:?}"
    );
    assert!(logs.is_empty(), "no degrade: {logs:?}");
}

// UD131 (R13) on an ORDINARY 80x24 terminal, in the DEFAULT debug suite.
//
// Unlike the wide-terminal `ud131_vt_release_*` tests, this route does NOT
// need a page to leave the standard capacity. It drives the ref-counted
// style set into its OTHER out-of-memory arm: the probe-sequence-length
// overflow (`ref_counted_set.zig:268`, "the hash function is not perfect ...
// we claim to be out of memory"), which fires as soon as one item sits 31
// slots from its home bucket. That takes only 32 live styles whose unseeded,
// deterministic style hashes (`style.zig:664`, `std.hash.int(wide[0] ^
// wide[1])`) share their low 16 bits. The 32 colours below all hash to low
// bits 0x2e2a, so they stay clustered at every capacity doubling and
// `increaseCapacity(.styles)` climbs 128 -> ... -> 65535 and then returns
// OutOfSpace, on a page holding only ~32 live styles. About 1.2 KB of
// ordinary output (591 line feeds fill page 1). The style hash is identical in the port, so the same
// colours cluster here (MEASURED, task R13-X3).
//
// Reference (MEASURED, task R13-X3, scratch ReleaseSafe/ReleaseFast builds at
// 20abdb50a): 80x24, these 32 colours, then `Y` with a new style on the first
// row of the next page, then DL across the page boundary -> @panic
// "increaseCapacity OutOfSpace" at Screen.zig:747 <- deleteLines
// (Terminal.zig:3215). ReleaseSafe SIGABRT (44 of 29,923 sweep runs die),
// ReleaseFast SIGTRAP (8 of 253). 31 colours never trigger; no DL never
// triggers.
//
// NEGATIVE CONTROL (task R13-X3): restoring the reference's `@panic` in the
// Screen loop's `OutOfSpace` arm (`clone_partial_row_grow_capacity`, at the
// `IncreaseCapacityError::OutOfSpace` match arm) makes
// `ud131_delete_lines_degrades_on_an_ordinary_80_column_terminal` fail.
#[rustfmt::skip]
const UD131_PSL_COLOURS: [(u8, u8, u8); 32] = [
    (0, 3, 183), (0, 36, 22), (0, 212, 121), (1, 39, 95), (1, 89, 203),
    (1, 98, 209), (1, 196, 183), (3, 10, 143), (3, 27, 63), (3, 136, 88),
    (3, 177, 19), (4, 4, 80), (4, 38, 52), (5, 170, 136), (6, 82, 67),
    (6, 211, 183), (6, 221, 6), (6, 247, 172), (7, 62, 169), (7, 87, 247),
    (7, 177, 15), (8, 49, 218), (8, 179, 161), (9, 140, 163), (9, 205, 72),
    (10, 83, 2), (10, 93, 46), (10, 248, 1), (11, 112, 126), (11, 210, 50),
    (13, 123, 81), (14, 56, 135),
];

struct Ud131PslOutcome {
    y_degraded: bool,
    y_found: bool,
    first_colliding_kept: bool,
    logs: Vec<String>,
}

/// Drives the PSL-overflow recipe on an 80x24 terminal with the first
/// `n_colliding` colours, then DL across a page boundary. Walks every page's
/// `assert_integrity` (real in a debug build).
fn ud131_psl_80col_run(n_colliding: usize) -> Ud131PslOutcome {
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    let cap = crate::log_capture::start();

    // Scroll until the active area straddles a page boundary.
    {
        let mut st = t.vt_stream();
        st.next_slice(b"\x1b[24;1H");
        st.deinit();
    }
    let mut boundary = None;
    for _ in 0..4000 {
        {
            let mut st = t.vt_stream();
            st.next_slice(b"\n");
            st.deinit();
        }
        let n0 = t.active_mut().pages.get_cell(active_pt(0, 0)).unwrap().node;
        for y in 1..24u32 {
            if t.active_mut().pages.get_cell(active_pt(0, y)).unwrap().node != n0 {
                if (4..=20).contains(&y) {
                    boundary = Some(y);
                }
                break;
            }
        }
        if boundary.is_some() {
            break;
        }
    }
    let b = boundary.expect("the active area straddles a page boundary");

    {
        let mut st = t.vt_stream();
        let mut buf = Vec::new();
        // The colliding styles on active row 1 (page 1, the DL destination).
        buf.extend_from_slice(b"\x1b[1;1H");
        for &(r, g, bb) in &UD131_PSL_COLOURS[..n_colliding] {
            buf.extend_from_slice(format!("\x1b[38;2;{r};{g};{bb}mX").as_bytes());
        }
        buf.extend_from_slice(b"\x1b[0m");
        // 'Y' with a NEW style on the first row of page 2.
        buf.extend_from_slice(format!("\x1b[{};1H\x1b[48;2;1;2;3mY\x1b[0m", b + 1).as_bytes());
        // DL at row 2 pulls page 2's first row (Y) into the full page 1.
        buf.extend_from_slice(b"\x1b[2;1H\x1b[M");
        st.next_slice(&buf);
        st.deinit();
    }

    let logs: Vec<String> = cap
        .messages()
        .into_iter()
        .filter(|(_, _, m)| m.contains("UD131"))
        .map(|(_, _, m)| m)
        .collect();
    drop(cap);

    // 'Y' survives the copy; its new style degraded to the default.
    let mut y_found = false;
    let mut y_degraded = false;
    for y in 0..24u32 {
        let lc = t.active_mut().pages.get_cell(active_pt(0, y)).unwrap();
        if unsafe { (*lc.cell).content_codepoint() } != 'Y' as u21 {
            continue;
        }
        y_found = true;
        y_degraded = unsafe { (*lc.cell).style_id() } == crate::style::DEFAULT_ID;
        break;
    }

    // The first colliding cell (active row 1, untouched by DL at row 2) keeps
    // its non-default style: only the UNSTORABLE cell degrades.
    let first = t.active_mut().pages.get_cell(active_pt(0, 0)).unwrap();
    let first_colliding_kept = unsafe { (*first.cell).style_id() } != crate::style::DEFAULT_ID;

    // Integrity holds across every page (real check in a debug build).
    let mut node = t.active_mut().pages.pages.first;
    while !node.is_null() {
        unsafe { (*crate::pagelist::Node::page(node)).assert_integrity() };
        node = unsafe { (*node).next };
    }
    // And the whole page list.
    t.active_mut().pages.assert_integrity();

    t.deinit(Global);
    Ud131PslOutcome {
        y_degraded,
        y_found,
        first_colliding_kept,
        logs,
    }
}

#[test]
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound in the debug-assertions-ON G6 lane, where `Page::assert_integrity` runs on nearly every page mutation (measured per test; see the C-1 table there). It RUNS in the G6c lane (`check_leaks.sh --debug-assertions-off`). Do NOT widen this to a plain `miri` ignore: that would drop the test from BOTH lanes."
)]
fn ud131_delete_lines_degrades_on_an_ordinary_80_column_terminal() {
    let o = ud131_psl_80col_run(32);
    assert!(o.y_found, "the 'Y' cell survives the copy");
    assert!(o.y_degraded, "'Y''s new style degrades to the default");
    assert!(
        o.first_colliding_kept,
        "the storable colliding cell keeps its style; only the unstorable degrades"
    );
    assert_eq!(
        o.logs,
        vec![
            "row copy at a page capacity ceiling: 1 cell part(s) degraded to \
             default (styles=1, hyperlinks=0, graphemes=0); see UD131"
                .to_string()
        ],
        "exactly one degrade record for one copy"
    );
}

// 31 colliding colours stay one below the PSL-overflow threshold, so the set
// rehashes instead of failing: nothing degrades, nothing is logged (the
// reference survives this too).
#[test]
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound in the debug-assertions-ON G6 lane, where `Page::assert_integrity` runs on nearly every page mutation (measured per test; see the C-1 table there). It RUNS in the G6c lane (`check_leaks.sh --debug-assertions-off`). Do NOT widen this to a plain `miri` ignore: that would drop the test from BOTH lanes."
)]
fn ud131_thirty_one_colours_is_the_80_column_control() {
    let o = ud131_psl_80col_run(31);
    assert!(o.y_found, "'Y' is present");
    assert!(!o.y_degraded, "'Y' keeps its new style");
    assert!(o.logs.is_empty(), "no degrade: {:?}", o.logs);
}

// Parity tail S7-3 routing evidence (Rust-only; the reference routes by
// construction): the terminal-owned containers in S7-3's scope reach the
// terminal's allocator — the tabstops' dynamic stops (> 512 columns) and
// the pwd/title lists (grown through `gpa()`) — and `deinit` returns every
// byte. (The screens and PageLists are covered by the Screen and PageList
// routing tests; the glyph glossary map by glossary.rs's.)
#[test]
fn terminal_internals_route_through_the_allocator_handle() {
    use crate::alloc::testing::{with_handle, CountingAllocator};
    use crate::alloc::ArrayListUnmanaged;

    let counting = CountingAllocator::over_heap();
    let c = counting.allocator();
    with_handle(&c, |alloc| {
        // Baseline: 80 columns fit the preallocated tabstops.
        let mut t = Terminal::init(alloc, opts(80, 5)).unwrap();
        let base_calls = counting.alloc_calls.get();
        let base_live = counting.live_bytes.get();
        t.deinit(alloc);
        drop(t);
        assert_eq!(0, counting.live_bytes.get());

        // 600 columns: exactly one more allocation, the dynamic stops
        // (`divCeil(600 - 512, 8)` = 11 bytes).
        let before = counting.alloc_calls.get();
        let mut t = Terminal::init(alloc, opts(600, 5)).unwrap();
        assert_eq!(
            base_calls + 1,
            counting.alloc_calls.get() - before,
            "Tabstops.init(alloc, 600, ...) grows its dynamic stops through alloc"
        );
        assert_eq!(base_live + 11, counting.live_bytes.get());

        // pwd/title: `ensureTotalCapacity(self.gpa(), len + 1)`, rounded up
        // by `growCapacity`.
        let live = counting.live_bytes.get();
        t.set_pwd(b"/tmp").unwrap();
        assert_eq!(
            Some((ArrayListUnmanaged::<u8>::grow_capacity(5), 0)),
            counting.last_alloc.get(),
            "setPwd grows through gpa()"
        );
        t.set_title(b"title").unwrap();
        assert_eq!(
            Some((ArrayListUnmanaged::<u8>::grow_capacity(6), 0)),
            counting.last_alloc.get(),
            "setTitle grows through gpa()"
        );
        assert_eq!(
            live + ArrayListUnmanaged::<u8>::grow_capacity(5)
                + ArrayListUnmanaged::<u8>::grow_capacity(6),
            counting.live_bytes.get()
        );

        t.deinit(alloc);
    });
    assert_eq!(0, counting.live_bytes.get(), "deinit returns every byte");
}

// Parity tail S7-3b (review F5, Rust-only): the glossary is bound to the
// terminal's allocator at `init`; the reference grows it through the
// per-call `alloc` of `glyphProtocol`, so a different allocator there is a
// caller bug the port now reports instead of silently ignoring.
#[test]
#[cfg_attr(
    not(debug_assertions),
    ignore = "debug_assert_same is a debug-build check (R14)"
)]
#[should_panic(expected = "allocator mismatch")]
fn glyph_protocol_checks_the_glossary_allocator() {
    use crate::alloc::testing::{with_handle, CountingAllocator};

    let counting = CountingAllocator::over_heap();
    let c = counting.allocator();
    with_handle(&c, |alloc| {
        let mut t = Terminal::init(alloc, opts(10, 2)).unwrap();
        // The matching allocator is accepted...
        let _ = t.glyph_protocol(alloc, &glyph::Request::Support);
        // ...a different one is not. The expected panic is caught so the
        // terminal is still torn down (Miri reports the leak otherwise),
        // then re-raised for `should_panic`.
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = t.glyph_protocol(Global, &glyph::Request::Support);
        }));
        t.deinit(alloc);
        if let Err(p) = r {
            std::panic::resume_unwind(p);
        }
    });
}
