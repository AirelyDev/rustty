//! PORTS: ghostty src/terminal/PageList.zig — the `resize` reflow
//! more-cols/less-cols tests (second half of the resize block).

use super::{cell_codepoint, pin_copy};
use crate::alloc::Global;
use crate::page::{Cell, SemanticPrompt};
use crate::pagelist::resize::Resize;
use crate::pagelist::{std_capacity, Options, PageList, Pin};
use crate::style::{self as stylepkg, Style};
use crate::{hyperlink, point};
use rustty_render_types::term_size as size;

// ref: test "PageList resize reflow less cols wrap across page boundary cursor in second page"
#[test]
fn pagelist_resize_reflow_less_cols_wrap_across_page_boundary_cursor_in_second_page() {
    let mut s = PageList::init(Global, Options::new(5, 10)).unwrap();
    assert_eq!(1usize, s.total_pages());

    // Grow to the capacity of the first page.
    {
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
        unsafe { (*page).pause_integrity_checks(true) };
        for _ in unsafe { (*page).size.rows..(*page).capacity.rows } {
            let _ = s.grow().unwrap();
        }
        unsafe { (*crate::pagelist::Node::page(s.pages.first)).pause_integrity_checks(false) };
        assert_eq!(1usize, s.total_pages());
        s.grow_rows(5).unwrap();
        assert_eq!(2usize, s.total_pages());
    }

    // At this point, we have some rows on the first page, and some on the second.
    // We can now wrap across the boundary condition.
    {
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
        let y = unsafe { (*page).size.rows as usize - 1 };
        {
            let rac = unsafe { (*page).get_row_and_cell(0, y) };
            unsafe { (*rac.row).set_wrap(true) };
        }
        for x in 0..s.cols as usize {
            let rac = unsafe { (*page).get_row_and_cell(x, y) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }
    {
        let page2 = unsafe { crate::pagelist::Node::page(s.pages.last) };
        let y = 0;
        {
            let rac = unsafe { (*page2).get_row_and_cell(0, y) };
            unsafe { (*rac.row).set_wrap_continuation(true) };
        }
        for x in 0..s.cols as usize {
            let rac = unsafe { (*page2).get_row_and_cell(x, y) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }

    // Put a tracked pin in wrapped row on the last page
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { x: 2, y: 5 }))
                .unwrap(),
        )
        .unwrap();
    assert!(core::ptr::eq(unsafe { (*p).node }, s.pages.last));
    assert_eq!(0, unsafe { (*p).y });

    // Resize
    s.resize(Resize {
        cols: Some(4),
        rows: None,
        reflow: true,
        cursor: Some(crate::pagelist::resize::Cursor {
            x: 2,
            y: 5,
            pin: None,
        }),
        pull_scrollback: true,
    })
    .unwrap();
    assert_eq!(4usize, s.cols as usize);

    // Our cursor should remain on the same cell
    assert_eq!(
        point::Point::Active(point::Coordinate { x: 3, y: 5 }),
        s.point_from_pin(point::Tag::Active, pin_copy(p)).unwrap()
    );

    {
        // PAGE 0 ROW 7895, ACTIVE 3
        let p2 = s
            .pin(point::Point::Active(point::Coordinate { x: 0, y: 3 }))
            .unwrap();
        let row = p2.row_and_cell().row;
        assert!(!unsafe { (*row).wrap() });
        assert!(!unsafe { (*row).wrap_continuation() });

        let cells = p2.cells(crate::pagelist::pin::CellSubset::All);
        assert!(!unsafe { (*(cells as *const Cell)).has_text() });
        assert!(!unsafe { (*(cells as *const Cell).add(1)).has_text() });
        assert!(!unsafe { (*(cells as *const Cell).add(2)).has_text() });
        assert!(!unsafe { (*(cells as *const Cell).add(3)).has_text() });
    }
    {
        // PAGE 0 ROW 7896, ACTIVE 4
        let p2 = s
            .pin(point::Point::Active(point::Coordinate { x: 0, y: 4 }))
            .unwrap();
        let row = p2.row_and_cell().row;
        assert!(unsafe { (*row).wrap() });
        assert!(!unsafe { (*row).wrap_continuation() });

        let cells = p2.cells(crate::pagelist::pin::CellSubset::All);
        assert_eq!(0u32, unsafe {
            (*(cells as *const Cell)).content_codepoint()
        });
        assert_eq!(1u32, unsafe {
            (*(cells as *const Cell).add(1)).content_codepoint()
        });
        assert_eq!(2u32, unsafe {
            (*(cells as *const Cell).add(2)).content_codepoint()
        });
        assert_eq!(3u32, unsafe {
            (*(cells as *const Cell).add(3)).content_codepoint()
        });
    }
    {
        // PAGE 0 ROW 7897, ACTIVE 5
        let p2 = s
            .pin(point::Point::Active(point::Coordinate { x: 0, y: 5 }))
            .unwrap();
        let row = p2.row_and_cell().row;
        assert!(unsafe { (*row).wrap() });
        assert!(unsafe { (*row).wrap_continuation() });

        let cells = p2.cells(crate::pagelist::pin::CellSubset::All);
        assert_eq!(4u32, unsafe {
            (*(cells as *const Cell)).content_codepoint()
        });
        assert_eq!(0u32, unsafe {
            (*(cells as *const Cell).add(1)).content_codepoint()
        });
        assert_eq!(1u32, unsafe {
            (*(cells as *const Cell).add(2)).content_codepoint()
        });
        assert_eq!(2u32, unsafe {
            (*(cells as *const Cell).add(3)).content_codepoint()
        });
    }
    {
        // PAGE 0 ROW 7898, ACTIVE 6
        let p2 = s
            .pin(point::Point::Active(point::Coordinate { x: 0, y: 6 }))
            .unwrap();
        let row = p2.row_and_cell().row;
        assert!(!unsafe { (*row).wrap() });
        assert!(unsafe { (*row).wrap_continuation() });

        let cells = p2.cells(crate::pagelist::pin::CellSubset::All);
        assert_eq!(3u32, unsafe {
            (*(cells as *const Cell)).content_codepoint()
        });
        assert_eq!(4u32, unsafe {
            (*(cells as *const Cell).add(1)).content_codepoint()
        });
    }
    {
        // PAGE 0 ROW 7899, ACTIVE 7
        let p2 = s
            .pin(point::Point::Active(point::Coordinate { x: 0, y: 7 }))
            .unwrap();
        let row = p2.row_and_cell().row;
        assert!(!unsafe { (*row).wrap() });
        assert!(!unsafe { (*row).wrap_continuation() });

        let cells = p2.cells(crate::pagelist::pin::CellSubset::All);
        assert!(!unsafe { (*(cells as *const Cell)).has_text() });
        assert!(!unsafe { (*(cells as *const Cell).add(1)).has_text() });
        assert!(!unsafe { (*(cells as *const Cell).add(2)).has_text() });
        assert!(!unsafe { (*(cells as *const Cell).add(3)).has_text() });
    }

    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList resize reflow more cols cursor in wrapped row"
#[test]
fn pagelist_resize_reflow_more_cols_cursor_in_wrapped_row() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(2, 4)
        },
    )
    .unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
    {
        {
            let rac = unsafe { (*page).get_row_and_cell(0, 0) };
            unsafe { (*rac.row).set_wrap(true) };
        }
        for x in 0..s.cols as usize {
            let rac = unsafe { (*page).get_row_and_cell(x, 0) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }
    {
        {
            let rac = unsafe { (*page).get_row_and_cell(0, 1) };
            unsafe { (*rac.row).set_wrap_continuation(true) };
        }
        for x in 0..s.cols as usize {
            let rac = unsafe { (*page).get_row_and_cell(x, 1) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }

    // Put a tracked pin in the history
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { x: 1, y: 1 }))
                .unwrap(),
        )
        .unwrap();

    // Resize
    s.resize(Resize {
        cols: Some(4),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(4usize, s.cols as usize);
    assert_eq!(4usize, s.total_rows());

    // Our cursor should move to the first row
    assert_eq!(
        point::Point::Active(point::Coordinate { x: 3, y: 0 }),
        s.point_from_pin(point::Tag::Active, pin_copy(p)).unwrap()
    );

    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList resize reflow more cols cursor in not wrapped row"
#[test]
fn pagelist_resize_reflow_more_cols_cursor_in_not_wrapped_row() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(2, 4)
        },
    )
    .unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
    {
        {
            let rac = unsafe { (*page).get_row_and_cell(0, 0) };
            unsafe { (*rac.row).set_wrap(true) };
        }
        for x in 0..s.cols as usize {
            let rac = unsafe { (*page).get_row_and_cell(x, 0) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }
    {
        {
            let rac = unsafe { (*page).get_row_and_cell(0, 1) };
            unsafe { (*rac.row).set_wrap_continuation(true) };
        }
        for x in 0..s.cols as usize {
            let rac = unsafe { (*page).get_row_and_cell(x, 1) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }

    // Put a tracked pin in the history
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { x: 1, y: 0 }))
                .unwrap(),
        )
        .unwrap();

    // Resize
    s.resize(Resize {
        cols: Some(4),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(4usize, s.cols as usize);
    assert_eq!(4usize, s.total_rows());

    // Our cursor should move to the first row
    assert_eq!(
        point::Point::Active(point::Coordinate { x: 1, y: 0 }),
        s.point_from_pin(point::Tag::Active, pin_copy(p)).unwrap()
    );

    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList resize reflow more cols cursor in wrapped row that isn't unwrapped"
#[test]
fn pagelist_resize_reflow_more_cols_cursor_in_wrapped_row_that_isnt_unwrapped() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(2, 4)
        },
    )
    .unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
    {
        {
            let rac = unsafe { (*page).get_row_and_cell(0, 0) };
            unsafe { (*rac.row).set_wrap(true) };
        }
        for x in 0..s.cols as usize {
            let rac = unsafe { (*page).get_row_and_cell(x, 0) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }
    {
        {
            let rac = unsafe { (*page).get_row_and_cell(0, 1) };
            unsafe {
                (*rac.row).set_wrap(true);
                (*rac.row).set_wrap_continuation(true);
            }
        }
        for x in 0..s.cols as usize {
            let rac = unsafe { (*page).get_row_and_cell(x, 1) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }
    {
        {
            let rac = unsafe { (*page).get_row_and_cell(0, 2) };
            unsafe { (*rac.row).set_wrap_continuation(true) };
        }
        for x in 0..s.cols as usize {
            let rac = unsafe { (*page).get_row_and_cell(x, 2) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }

    // Put a tracked pin in the history
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { x: 1, y: 2 }))
                .unwrap(),
        )
        .unwrap();

    // Resize
    s.resize(Resize {
        cols: Some(4),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(4usize, s.cols as usize);
    assert_eq!(4usize, s.total_rows());

    // Our cursor should move to the first row
    assert_eq!(
        point::Point::Active(point::Coordinate { x: 1, y: 1 }),
        s.point_from_pin(point::Tag::Active, pin_copy(p)).unwrap()
    );

    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList resize reflow more cols no reflow preserves semantic prompt"
#[test]
fn pagelist_resize_reflow_more_cols_no_reflow_preserves_semantic_prompt() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(2, 4)
        },
    )
    .unwrap();
    {
        assert!(core::ptr::eq(s.pages.first, s.pages.last));
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
        let rac = unsafe { (*page).get_row_and_cell(0, 1) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }

    // Resize
    s.resize(Resize {
        cols: Some(4),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(4usize, s.cols as usize);
    assert_eq!(4usize, s.total_rows());

    {
        assert!(core::ptr::eq(s.pages.first, s.pages.last));
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
        let rac = unsafe { (*page).get_row_and_cell(0, 1) };
        assert!(unsafe { (*rac.row).semantic_prompt() } == SemanticPrompt::Prompt);
    }

    s.deinit();
}

// ref: test "PageList resize reflow exceeds hyperlink memory forcing capacity increase"
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn pagelist_resize_reflow_exceeds_hyperlink_memory_forcing_capacity_increase() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(2, 10)
        },
    )
    .unwrap();
    assert_eq!(1usize, s.total_pages());

    // Grow to the capacity of the first page and add
    // one more row so that we have two pages total.
    {
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
        unsafe { (*page).pause_integrity_checks(true) };
        for _ in unsafe { (*page).size.rows..(*page).capacity.rows } {
            let _ = s.grow().unwrap();
        }
        unsafe { (*crate::pagelist::Node::page(s.pages.first)).pause_integrity_checks(false) };
        assert_eq!(1usize, s.total_pages());
        s.grow_rows(1).unwrap();
        assert_eq!(2usize, s.total_pages());

        // We now have two pages.
        assert!(!core::ptr::eq(s.pages.first, s.pages.last));
        assert!(core::ptr::eq(s.pages.last, unsafe {
            (*s.pages.first).next
        }));
    }

    // We use almost all string alloc capacity with a hyperlink in the final
    // row of the first page, and do the same on the first row of the second
    // page. We also mark the row as wrapped so that when we resize with more
    // cols the row unwraps and we have a single row that requires almost two
    // times the base string alloc capacity.
    //
    // This forces the reflow to increase capacity.
    //
    //  +--+ = PAGE 0
    //  :  :
    //  | X… <- where X is hyperlinked with almost all string cap.
    //  +--+
    //  +--+ = PAGE 1
    //  …X | <- X here also almost hits string cap with a hyperlink.
    //  +--+

    // Almost hit string alloc cap in bottom right of first page.
    // Mark the final row as wrapped.
    {
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
        let id = unsafe {
            (*page).insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(0),
                uri: vec![b'a'; crate::page::STRING_BYTES_DEFAULT - 1],
            })
        }
        .unwrap();
        let rac = unsafe {
            (*page).get_row_and_cell(
                (*page).size.cols as usize - 1,
                (*page).size.rows as usize - 1,
            )
        };
        unsafe {
            (*rac.row).set_wrap(true);
            *rac.cell = cell_codepoint('X' as u32);
        }
        unsafe { (*page).set_hyperlink(rac.row, rac.cell, id) }.unwrap();
        assert!(matches!(
            unsafe {
                (*page).insert_hyperlink(hyperlink::Hyperlink {
                    id: hyperlink::HyperlinkId::Implicit(1),
                    uri: b"AAAAAAAAAAAAAAAAAAAAAAAAAA".to_vec(),
                })
            },
            Err(crate::page::InsertHyperlinkError::StringsOutOfMemory)
        ));
    }

    // Almost hit string alloc cap in top left of second page.
    // Mark the first row as a wrap continuation.
    {
        let page = unsafe { crate::pagelist::Node::page(s.pages.last) };
        let id = unsafe {
            (*page).insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(1),
                uri: vec![b'a'; crate::page::STRING_BYTES_DEFAULT - 1],
            })
        }
        .unwrap();
        let rac = unsafe { (*page).get_row_and_cell(0, 0) };
        unsafe {
            (*rac.row).set_wrap_continuation(true);
            *rac.cell = cell_codepoint('X' as u32);
        }
        unsafe { (*page).set_hyperlink(rac.row, rac.cell, id) }.unwrap();
        assert!(matches!(
            unsafe {
                (*page).insert_hyperlink(hyperlink::Hyperlink {
                    id: hyperlink::HyperlinkId::Implicit(2),
                    uri: b"AAAAAAAAAAAAAAAAAAAAAAAAAA".to_vec(),
                })
            },
            Err(crate::page::InsertHyperlinkError::StringsOutOfMemory)
        ));
    }

    // Resize to 1 column wider, unwrapping the row.
    s.resize(Resize {
        cols: Some(s.cols + 1),
        reflow: true,
        ..Default::default()
    })
    .unwrap();

    s.deinit();
}

// ref: test "PageList resize reflow hyperlink dupe string alloc chunk rounding"
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane (same shape as its sibling `pagelist_resize_reflow_exceeds_hyperlink_memory_forcing_capacity_increase`). It runs in the G6c lane (`check_leaks.sh --debug-assertions-off`). Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn pagelist_resize_reflow_hyperlink_dupe_string_alloc_chunk_rounding() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(2, 10)
        },
    )
    .unwrap();
    assert_eq!(1usize, s.total_pages());

    // Grow to the capacity of the first page and add
    // one more row so that we have two pages total.
    {
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
        unsafe { (*page).pause_integrity_checks(true) };
        for _ in unsafe { (*page).size.rows..(*page).capacity.rows } {
            let _ = s.grow().unwrap();
        }
        unsafe { (*crate::pagelist::Node::page(s.pages.first)).pause_integrity_checks(false) };
        assert_eq!(1usize, s.total_pages());
        s.grow_rows(1).unwrap();
        assert_eq!(2usize, s.total_pages());

        // We now have two pages.
        assert!(!core::ptr::eq(s.pages.first, s.pages.last));
        assert!(core::ptr::eq(s.pages.last, unsafe {
            (*s.pages.first).next
        }));
    }

    // The string allocator hands out 32-byte chunks and every allocation
    // is rounded up to the chunk size independently. Duping a hyperlink
    // during reflow allocates the URI and the explicit ID separately, so
    // two separate allocations can require one more chunk than a single
    // combined allocation of the same total byte length.
    //
    // We arrange for the reflow target page to have exactly two free
    // chunks (64 bytes) remaining when a hyperlink with a 33-byte URI
    // (2 chunks) and a 31-byte explicit ID (1 chunk) is reflowed into
    // it. The combined byte length (64 bytes -> 2 chunks) fits, but the
    // separate allocations (3 chunks) do not, so the reflow must grow
    // the string capacity rather than panic or drop the hyperlink.
    //
    // The two hyperlinked cells are joined as a single wrapped row so
    // that they are always reflowed into the same target page.
    //
    //  +--+ = PAGE 0
    //  :  :
    //  | A… <- A is hyperlinked with all but 64 bytes of string cap.
    //  +--+
    //  +--+ = PAGE 1
    //  …B | <- B is hyperlinked with a 33-byte URI and 31-byte ID.
    //  +--+

    let uri_a: Vec<u8> = vec![b'a'; crate::page::STRING_BYTES_DEFAULT - 64];
    let uri_b: Vec<u8> = vec![b'b'; 33];
    let id_b: Vec<u8> = vec![b'i'; 31];

    // Hyperlink A in the bottom right of the first page. Mark the final
    // row as wrapped.
    {
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
        let id = unsafe {
            (*page).insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(0),
                uri: uri_a.clone(),
            })
        }
        .unwrap();
        let rac = unsafe {
            (*page).get_row_and_cell(
                (*page).size.cols as usize - 1,
                (*page).size.rows as usize - 1,
            )
        };
        unsafe {
            (*rac.row).set_wrap(true);
            *rac.cell = cell_codepoint('A' as u32);
        }
        unsafe { (*page).set_hyperlink(rac.row, rac.cell, id) }.unwrap();

        // Sanity check the chunk math: the remaining 64 bytes fit as a
        // single allocation but not as the two separate allocations that
        // inserting (or duping) hyperlink B performs.
        let base = unsafe { (*page).memory_ptr() };
        let buf = unsafe { (*page).string_alloc.alloc::<u8, _>(base, 64) }.unwrap();
        unsafe { (*page).string_alloc.free(base, buf) };
        assert!(matches!(
            unsafe {
                (*page).insert_hyperlink(hyperlink::Hyperlink {
                    id: hyperlink::HyperlinkId::Explicit(id_b.clone()),
                    uri: uri_b.clone(),
                })
            },
            Err(crate::page::InsertHyperlinkError::StringsOutOfMemory)
        ));
    }

    // Hyperlink B in the top left of the second page. Mark the first
    // row as a wrap continuation.
    {
        let page = unsafe { crate::pagelist::Node::page(s.pages.last) };
        let id = unsafe {
            (*page).insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Explicit(id_b.clone()),
                uri: uri_b.clone(),
            })
        }
        .unwrap();
        let rac = unsafe { (*page).get_row_and_cell(0, 0) };
        unsafe {
            (*rac.row).set_wrap_continuation(true);
            *rac.cell = cell_codepoint('B' as u32);
        }
        unsafe { (*page).set_hyperlink(rac.row, rac.cell, id) }.unwrap();
    }

    // Resize to 1 column wider, unwrapping the row.
    s.resize(Resize {
        cols: Some(s.cols + 1),
        reflow: true,
        ..Default::default()
    })
    .unwrap();

    // Both hyperlinks must have survived the reflow intact.
    let mut found: usize = 0;
    let mut node_it = s.pages.first;
    while !node_it.is_null() {
        let page = unsafe { crate::pagelist::Node::page(node_it) };
        for y in 0..unsafe { (*page).size.rows } as usize {
            for x in 0..unsafe { (*page).size.cols } as usize {
                let rac = unsafe { (*page).get_row_and_cell(x, y) };
                if !unsafe { (*rac.cell).hyperlink() } {
                    continue;
                }
                found += 1;

                let link_id = unsafe { (*page).lookup_hyperlink(rac.cell) }.unwrap();
                let base = unsafe { (*page).memory_ptr() };
                let entry = unsafe { (*page).hyperlink_set.get(base, link_id) };
                let uri = unsafe { (*entry).uri.slice(base) };
                match unsafe { (*entry).id.tag() } {
                    crate::hyperlink::PageEntryIdTag::Implicit => assert_eq!(uri_a, uri),
                    crate::hyperlink::PageEntryIdTag::Explicit => {
                        assert_eq!(uri_b, uri);
                        assert_eq!(id_b, unsafe { (*entry).id.explicit().slice(base) });
                    }
                }
            }
        }
        node_it = unsafe { (*node_it).next };
    }
    assert_eq!(2usize, found);

    s.deinit();
}

// ref: test "PageList resize reflow exceeds grapheme memory forcing capacity increase"
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn pagelist_resize_reflow_exceeds_grapheme_memory_forcing_capacity_increase() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(4, 10)
        },
    )
    .unwrap();
    assert_eq!(1usize, s.total_pages());

    // Grow to the capacity of the first page and add
    // one more row so that we have two pages total.
    {
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
        unsafe { (*page).pause_integrity_checks(true) };
        for _ in unsafe { (*page).size.rows..(*page).capacity.rows } {
            let _ = s.grow().unwrap();
        }
        unsafe { (*crate::pagelist::Node::page(s.pages.first)).pause_integrity_checks(false) };
        assert_eq!(1usize, s.total_pages());
        s.grow_rows(1).unwrap();
        assert_eq!(2usize, s.total_pages());

        // We now have two pages.
        assert!(!core::ptr::eq(s.pages.first, s.pages.last));
        assert!(core::ptr::eq(s.pages.last, unsafe {
            (*s.pages.first).next
        }));
    }

    // We use all grapheme alloc capacity with four maximum-sized graphemes on
    // each page. The two rows form one wrapped logical line across the page
    // boundary, so resizing wider moves all eight graphemes into one page and
    // requires almost two times the base grapheme alloc capacity.
    //
    // This forces the reflow to increase capacity.
    //
    //  +----+ = PAGE 0
    //  :  :
    //  |XXXX| <- four capped graphemes in one wrapped row.
    //  +----+
    //  +----+ = PAGE 1
    //  |XXXX| <- four more capped graphemes continue the logical line.
    //  +----+

    // const suffixes: [pagepkg.grapheme_max_len]u21 = @splat('a');
    let suffixes: Vec<u32> = vec!['a' as u32; crate::page::GRAPHEME_MAX_LEN];

    // Fill the final row of the first page and mark it as wrapped.
    {
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
        let y = unsafe { (*page).size.rows } as usize - 1;
        let row = unsafe { (*page).get_row(y) };
        unsafe { (*row).set_wrap(true) };

        for x in 0..unsafe { (*page).size.cols } as usize {
            let rac = unsafe { (*page).get_row_and_cell(x, y) };
            unsafe {
                *rac.cell = cell_codepoint('X' as u32);
            }
            unsafe { (*page).set_graphemes(rac.row, rac.cell, &suffixes) }.unwrap();
        }
        let base = unsafe { (*page).memory_ptr() };
        assert_eq!(unsafe { (*page).grapheme_alloc.capacity_bytes() }, unsafe {
            (*page).grapheme_alloc.used_bytes(base)
        });
        assert!(unsafe { (*page).grapheme_alloc.alloc::<u32, _>(base, 16) }.is_err());
    }

    // Fill the first row of the second page and mark it as a continuation.
    {
        let page = unsafe { crate::pagelist::Node::page(s.pages.last) };
        let row = unsafe { (*page).get_row(0) };
        unsafe { (*row).set_wrap_continuation(true) };

        for x in 0..unsafe { (*page).size.cols } as usize {
            let rac = unsafe { (*page).get_row_and_cell(x, 0) };
            unsafe {
                *rac.cell = cell_codepoint('X' as u32);
            }
            unsafe { (*page).set_graphemes(rac.row, rac.cell, &suffixes) }.unwrap();
        }
        let base = unsafe { (*page).memory_ptr() };
        assert!(unsafe { (*page).grapheme_alloc.alloc::<u32, _>(base, 16) }.is_err());
    }

    // Resize to 1 column wider, unwrapping the row.
    s.resize(Resize {
        cols: Some(s.cols + 1),
        reflow: true,
        ..Default::default()
    })
    .unwrap();

    s.deinit();
}

// ref: test "PageList resize reflow exceeds style memory forcing capacity increase"
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn pagelist_resize_reflow_exceeds_style_memory_forcing_capacity_increase() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(std_capacity().styles - 1, 10)
        },
    )
    .unwrap();
    assert_eq!(1usize, s.total_pages());

    // Grow to the capacity of the first page and add
    // one more row so that we have two pages total.
    {
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
        unsafe { (*page).pause_integrity_checks(true) };
        for _ in unsafe { (*page).size.rows..(*page).capacity.rows } {
            let _ = s.grow().unwrap();
        }
        unsafe { (*crate::pagelist::Node::page(s.pages.first)).pause_integrity_checks(false) };
        assert_eq!(1usize, s.total_pages());
        s.grow_rows(1).unwrap();
        assert_eq!(2usize, s.total_pages());

        // We now have two pages.
        assert!(!core::ptr::eq(s.pages.first, s.pages.last));
        assert!(core::ptr::eq(s.pages.last, unsafe {
            (*s.pages.first).next
        }));
    }

    // Give each cell in the final row of the first page a unique style.
    // Mark the final row as wrapped.
    {
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
        let base = unsafe { (*page).memory_ptr() };
        for x in 0..s.cols as usize {
            let style = Style {
                bg_color: stylepkg::Color::Rgb(crate::color::RGB {
                    r: x as u8,
                    g: (x >> 8) as u8,
                    b: (x >> 16) as u8,
                }),
                ..Style::default()
            };
            let id = match unsafe { (*page).styles.add(base, style) } {
                Ok(id) => id,
                Err(_) => break,
            };

            let rac = unsafe { (*page).get_row_and_cell(x, (*page).size.rows as usize - 1) };
            unsafe {
                (*rac.row).set_wrap(true);
                (*rac.row).set_styled(true);
                let mut c = cell_codepoint('X' as u32);
                c.set_style_id(id);
                *rac.cell = c;
            }
        }
    }

    // Do the same for the first row of the second page.
    // Mark the first row as a wrap continuation.
    {
        let page = unsafe { crate::pagelist::Node::page(s.pages.last) };
        let base = unsafe { (*page).memory_ptr() };
        for x in 0..s.cols as usize {
            let style = Style {
                fg_color: stylepkg::Color::Rgb(crate::color::RGB {
                    r: x as u8,
                    g: (x >> 8) as u8,
                    b: (x >> 16) as u8,
                }),
                ..Style::default()
            };
            let id = match unsafe { (*page).styles.add(base, style) } {
                Ok(id) => id,
                Err(_) => break,
            };

            let rac = unsafe { (*page).get_row_and_cell(x, 0) };
            unsafe {
                (*rac.row).set_wrap_continuation(true);
                (*rac.row).set_styled(true);
                let mut c = cell_codepoint('X' as u32);
                c.set_style_id(id);
                *rac.cell = c;
            }
        }
    }

    // Resize to twice as wide, fully unwrapping the row.
    s.resize(Resize {
        cols: Some(s.cols * 2),
        reflow: true,
        ..Default::default()
    })
    .unwrap();

    s.deinit();
}

// ref: test "PageList resize reflow more cols unwrap wide spacer head"
#[test]
fn pagelist_resize_reflow_more_cols_unwrap_wide_spacer_head() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(2, 2)
        },
    )
    .unwrap();
    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

        {
            let rac = unsafe { (*page).get_row_and_cell(0, 0) };
            unsafe {
                (*rac.row).set_wrap(true);
                *rac.cell = cell_codepoint('x' as u32);
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 0) };
            unsafe {
                let mut c = cell_codepoint(0);
                c.set_wide(crate::page::Wide::SpacerHead);
                *rac.cell = c;
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(0, 1) };
            unsafe {
                (*rac.row).set_wrap_continuation(true);
                let mut c = cell_codepoint('😀' as u32);
                c.set_wide(crate::page::Wide::Wide);
                *rac.cell = c;
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 1) };
            unsafe {
                let mut c = cell_codepoint(0);
                c.set_wide(crate::page::Wide::SpacerTail);
                *rac.cell = c;
            }
        }
    }

    // Resize
    s.resize(Resize {
        cols: Some(4),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(4usize, s.cols as usize);
    assert_eq!(2usize, s.total_rows());

    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

        {
            let rac = unsafe { (*page).get_row_and_cell(0, 0) };
            assert_eq!('x' as u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::Narrow, unsafe { (*rac.cell).wide() });
            assert!(!unsafe { (*rac.row).wrap() });
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 0) };
            assert_eq!('😀' as u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::Wide, unsafe { (*rac.cell).wide() });
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(2, 0) };
            assert_eq!(0u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::SpacerTail, unsafe { (*rac.cell).wide() });
        }
    }

    s.deinit();
}

// ref: test "PageList resize reflow more cols unwrap wide spacer head across two rows"
#[test]
fn pagelist_resize_reflow_more_cols_unwrap_wide_spacer_head_across_two_rows() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(2, 3)
        },
    )
    .unwrap();
    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

        {
            let rac = unsafe { (*page).get_row_and_cell(0, 0) };
            unsafe {
                (*rac.row).set_wrap(true);
                *rac.cell = cell_codepoint('x' as u32);
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 0) };
            unsafe {
                *rac.cell = cell_codepoint('x' as u32);
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(0, 1) };
            unsafe {
                (*rac.row).set_wrap_continuation(true);
                (*rac.row).set_wrap(true);
                *rac.cell = cell_codepoint('x' as u32);
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 1) };
            unsafe {
                let mut c = cell_codepoint(0);
                c.set_wide(crate::page::Wide::SpacerHead);
                *rac.cell = c;
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(0, 2) };
            unsafe {
                (*rac.row).set_wrap_continuation(true);
                let mut c = cell_codepoint('😀' as u32);
                c.set_wide(crate::page::Wide::Wide);
                *rac.cell = c;
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 2) };
            unsafe {
                let mut c = cell_codepoint(0);
                c.set_wide(crate::page::Wide::SpacerTail);
                *rac.cell = c;
            }
        }
    }

    // Resize
    s.resize(Resize {
        cols: Some(4),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(4usize, s.cols as usize);
    assert_eq!(3usize, s.total_rows());

    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

        {
            let rac = unsafe { (*page).get_row_and_cell(0, 0) };
            assert_eq!('x' as u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::Narrow, unsafe { (*rac.cell).wide() });
            assert!(unsafe { (*rac.row).wrap() });
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 0) };
            assert_eq!('x' as u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::Narrow, unsafe { (*rac.cell).wide() });
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(2, 0) };
            assert_eq!('x' as u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::Narrow, unsafe { (*rac.cell).wide() });
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(3, 0) };
            assert_eq!(0u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::SpacerHead, unsafe { (*rac.cell).wide() });
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(0, 1) };
            assert_eq!('😀' as u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::Wide, unsafe { (*rac.cell).wide() });
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 1) };
            assert_eq!(0u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::SpacerTail, unsafe { (*rac.cell).wide() });
        }
    }

    s.deinit();
}

// ref: test "PageList resize reflow more cols unwrap still requires wide spacer head"
#[test]
fn pagelist_resize_reflow_more_cols_unwrap_still_requires_wide_spacer_head() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(2, 2)
        },
    )
    .unwrap();
    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

        {
            let rac = unsafe { (*page).get_row_and_cell(0, 0) };
            unsafe {
                (*rac.row).set_wrap(true);
                *rac.cell = cell_codepoint('x' as u32);
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 0) };
            unsafe {
                *rac.cell = cell_codepoint('x' as u32);
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(0, 1) };
            unsafe {
                (*rac.row).set_wrap_continuation(true);
                let mut c = cell_codepoint('😀' as u32);
                c.set_wide(crate::page::Wide::Wide);
                *rac.cell = c;
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 1) };
            unsafe {
                let mut c = cell_codepoint(0);
                c.set_wide(crate::page::Wide::SpacerTail);
                *rac.cell = c;
            }
        }
    }

    // Resize
    s.resize(Resize {
        cols: Some(3),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(3usize, s.cols as usize);
    assert_eq!(2usize, s.total_rows());

    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

        {
            let rac = unsafe { (*page).get_row_and_cell(0, 0) };
            assert_eq!('x' as u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::Narrow, unsafe { (*rac.cell).wide() });
            assert!(unsafe { (*rac.row).wrap() });
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 0) };
            assert_eq!('x' as u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::Narrow, unsafe { (*rac.cell).wide() });
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(2, 0) };
            assert_eq!(0u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::SpacerHead, unsafe { (*rac.cell).wide() });
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(0, 1) };
            assert_eq!('😀' as u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::Wide, unsafe { (*rac.cell).wide() });
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 1) };
            assert_eq!(0u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::SpacerTail, unsafe { (*rac.cell).wide() });
        }
    }

    s.deinit();
}

// ref: test "PageList resize reflow less cols no reflow preserves semantic prompt"
#[test]
fn pagelist_resize_reflow_less_cols_no_reflow_preserves_semantic_prompt() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(4, 4)
        },
    )
    .unwrap();
    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
        {
            let rac = unsafe { (*page).get_row_and_cell(0, 1) };
            unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
        }
        for x in 0..s.cols as usize {
            let rac = unsafe { (*page).get_row_and_cell(x, 1) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }

    // Resize
    s.resize(Resize {
        cols: Some(2),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(2usize, s.cols as usize);
    assert_eq!(4usize, s.total_rows());

    {
        assert!(s.pages.first == s.pages.last);
        {
            let p = s
                .pin(point::Point::Active(point::Coordinate { x: 0, y: 1 }))
                .unwrap();
            let rac = p.row_and_cell();
            assert!(unsafe { (*rac.row).wrap() });
            assert!(unsafe { (*rac.row).semantic_prompt() } == SemanticPrompt::Prompt);
        }
        {
            let p = s
                .pin(point::Point::Active(point::Coordinate { x: 0, y: 2 }))
                .unwrap();
            let rac = p.row_and_cell();
            assert!(unsafe { (*rac.row).semantic_prompt() } == SemanticPrompt::Prompt);
        }
    }

    s.deinit();
}

// ref: test "PageList resize reflow less cols no reflow preserves semantic prompt on first line"
#[test]
fn pagelist_resize_reflow_less_cols_no_reflow_preserves_semantic_prompt_on_first_line() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(4, 4)
        },
    )
    .unwrap();
    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
        let rac = unsafe { (*page).get_row_and_cell(0, 0) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }

    // Resize
    s.resize(Resize {
        cols: Some(2),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(2usize, s.cols as usize);
    assert_eq!(4usize, s.total_rows());

    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
        let rac = unsafe { (*page).get_row_and_cell(0, 0) };
        assert!(unsafe { (*rac.row).semantic_prompt() } == SemanticPrompt::Prompt);
    }

    s.deinit();
}

// ref: test "PageList resize reflow less cols wrap preserves semantic prompt"
#[test]
fn pagelist_resize_reflow_less_cols_wrap_preserves_semantic_prompt() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(4, 4)
        },
    )
    .unwrap();
    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
        let rac = unsafe { (*page).get_row_and_cell(0, 0) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }

    // Resize
    s.resize(Resize {
        cols: Some(2),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(2usize, s.cols as usize);
    assert_eq!(4usize, s.total_rows());

    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
        let rac = unsafe { (*page).get_row_and_cell(0, 0) };
        assert!(unsafe { (*rac.row).semantic_prompt() } == SemanticPrompt::Prompt);
    }

    s.deinit();
}

// ref: test "PageList resize reflow less cols no wrapped rows"
#[test]
fn pagelist_resize_reflow_less_cols_no_wrapped_rows() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 3)
        },
    )
    .unwrap();
    assert!(s.pages.first == s.pages.last);
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
    for y in 0..s.rows as usize {
        let end = 4;
        assert!(end < s.cols as usize);
        for x in 0..4usize {
            let rac = unsafe { (*page).get_row_and_cell(x, y) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }

    // Resize
    s.resize(Resize {
        cols: Some(5),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(5usize, s.cols as usize);
    assert_eq!(3usize, s.total_rows());

    let mut it = s.row_iterator(
        crate::pagelist::iter::Direction::RightDown,
        point::Point::Screen(point::Coordinate::default()),
        None,
    );
    while let Some(offset) = it.next() {
        for x in 0..4usize {
            let mut offset_copy = pin_copy(&offset as *const Pin as *mut Pin);
            offset_copy.x = x as size::CellCountInt;
            let rac = offset_copy.row_and_cell();
            let cells = unsafe { (*crate::pagelist::Node::page(offset.node)).get_cells(rac.row) };
            assert_eq!(5usize, cells.len());
            assert_eq!(x as u32, unsafe {
                (*(*cells).as_ptr().add(x)).content_codepoint()
            });
        }
    }

    s.deinit();
}

// ref: test "PageList resize reflow less cols wrapped rows"
#[test]
fn pagelist_resize_reflow_less_cols_wrapped_rows() {
    let mut s = PageList::init(Global, Options::new(4, 2)).unwrap();
    assert!(s.pages.first == s.pages.last);
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
    for y in 0..s.rows as usize {
        for x in 0..s.cols as usize {
            let rac = unsafe { (*page).get_row_and_cell(x, y) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }

    // Resize
    s.resize(Resize {
        cols: Some(2),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(2usize, s.cols as usize);
    assert_eq!(4usize, s.total_rows());

    // Active moves due to scrollback
    {
        let pt = s
            .get_cell(point::Point::Active(point::Coordinate::default()))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 2 }), pt);
    }

    let mut it = s.row_iterator(
        crate::pagelist::iter::Direction::RightDown,
        point::Point::Screen(point::Coordinate::default()),
        None,
    );
    {
        // First row should be wrapped
        let offset = it.next().unwrap();
        let rac = offset.row_and_cell();
        let cells = unsafe { (*crate::pagelist::Node::page(offset.node)).get_cells(rac.row) };
        assert!(unsafe { (*rac.row).wrap() });
        assert_eq!(2usize, cells.len());
        assert_eq!(0u32, unsafe { (*(*cells).as_ptr()).content_codepoint() });
    }
    {
        let offset = it.next().unwrap();
        let rac = offset.row_and_cell();
        let cells = unsafe { (*crate::pagelist::Node::page(offset.node)).get_cells(rac.row) };
        assert!(!unsafe { (*rac.row).wrap() });
        assert_eq!(2usize, cells.len());
        assert_eq!(2u32, unsafe { (*(*cells).as_ptr()).content_codepoint() });
    }
    {
        // First row should be wrapped
        let offset = it.next().unwrap();
        let rac = offset.row_and_cell();
        let cells = unsafe { (*crate::pagelist::Node::page(offset.node)).get_cells(rac.row) };
        assert!(unsafe { (*rac.row).wrap() });
        assert_eq!(2usize, cells.len());
        assert_eq!(0u32, unsafe { (*(*cells).as_ptr()).content_codepoint() });
    }
    {
        let offset = it.next().unwrap();
        let rac = offset.row_and_cell();
        let cells = unsafe { (*crate::pagelist::Node::page(offset.node)).get_cells(rac.row) };
        assert!(!unsafe { (*rac.row).wrap() });
        assert_eq!(2usize, cells.len());
        assert_eq!(2u32, unsafe { (*(*cells).as_ptr()).content_codepoint() });
    }

    s.deinit();
}

// ref: test "PageList resize reflow less cols wrapped rows with graphemes"
#[test]
fn pagelist_resize_reflow_less_cols_wrapped_rows_with_graphemes() {
    let mut s = PageList::init(Global, Options::new(4, 2)).unwrap();
    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
        for y in 0..s.rows as usize {
            for x in 0..s.cols as usize {
                let rac = unsafe { (*page).get_row_and_cell(x, y) };
                unsafe {
                    *rac.cell = cell_codepoint(x as u32);
                }
            }

            let rac = unsafe { (*page).get_row_and_cell(2, y) };
            unsafe { (*page).append_grapheme(rac.row, rac.cell, 'A' as u32) }.unwrap();
        }
    }

    // Resize
    s.resize(Resize {
        cols: Some(2),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(2usize, s.cols as usize);
    assert_eq!(4usize, s.total_rows());

    // Active moves due to scrollback
    {
        let pt = s
            .get_cell(point::Point::Active(point::Coordinate::default()))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 2 }), pt);
    }

    assert!(s.pages.first == s.pages.last);
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
    let mut it = s.row_iterator(
        crate::pagelist::iter::Direction::RightDown,
        point::Point::Screen(point::Coordinate::default()),
        None,
    );
    {
        // First row should be wrapped
        let offset = it.next().unwrap();
        let rac = offset.row_and_cell();
        let cells = unsafe { (*crate::pagelist::Node::page(offset.node)).get_cells(rac.row) };
        assert!(unsafe { (*rac.row).wrap() });
        assert_eq!(2usize, cells.len());
        assert_eq!(0u32, unsafe { (*(*cells).as_ptr()).content_codepoint() });
    }
    {
        let offset = it.next().unwrap();
        let rac = offset.row_and_cell();
        let cells = unsafe { (*crate::pagelist::Node::page(offset.node)).get_cells(rac.row) };
        assert!(!unsafe { (*rac.row).wrap() });
        assert!(unsafe { (*rac.row).grapheme() });
        assert_eq!(2usize, cells.len());
        assert_eq!(2u32, unsafe { (*(*cells).as_ptr()).content_codepoint() });

        let cps = unsafe { (*page).lookup_grapheme(rac.cell) }.unwrap();
        assert_eq!(1usize, cps.len());
        assert_eq!('A' as u32, cps[0]);
    }
    {
        // First row should be wrapped
        let offset = it.next().unwrap();
        let rac = offset.row_and_cell();
        let cells = unsafe { (*crate::pagelist::Node::page(offset.node)).get_cells(rac.row) };
        assert!(unsafe { (*rac.row).wrap() });
        assert_eq!(2usize, cells.len());
        assert_eq!(0u32, unsafe { (*(*cells).as_ptr()).content_codepoint() });
    }
    {
        let offset = it.next().unwrap();
        let rac = offset.row_and_cell();
        let cells = unsafe { (*crate::pagelist::Node::page(offset.node)).get_cells(rac.row) };
        assert!(!unsafe { (*rac.row).wrap() });
        assert!(unsafe { (*rac.row).grapheme() });
        assert_eq!(2usize, cells.len());
        assert_eq!(2u32, unsafe { (*(*cells).as_ptr()).content_codepoint() });

        let cps = unsafe { (*page).lookup_grapheme(rac.cell) }.unwrap();
        assert_eq!(1usize, cps.len());
        assert_eq!('A' as u32, cps[0]);
    }

    s.deinit();
}

// ref: test "PageList resize reflow less cols cursor in wrapped row"
#[test]
fn pagelist_resize_reflow_less_cols_cursor_in_wrapped_row() {
    let mut s = PageList::init(Global, Options::new(4, 2)).unwrap();
    assert!(s.pages.first == s.pages.last);
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
    for y in 0..s.rows as usize {
        for x in 0..s.cols as usize {
            let rac = unsafe { (*page).get_row_and_cell(x, y) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }

    // Put a tracked pin in the history
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { x: 2, y: 1 }))
                .unwrap(),
        )
        .unwrap();

    // Resize
    s.resize(Resize {
        cols: Some(2),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(2usize, s.cols as usize);
    assert_eq!(4usize, s.total_rows());

    // Our cursor should move to the first row
    assert_eq!(
        point::Point::Active(point::Coordinate { x: 0, y: 1 }),
        s.point_from_pin(point::Tag::Active, pin_copy(p)).unwrap()
    );

    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList resize reflow less cols wraps spacer head"
#[test]
fn pagelist_resize_reflow_less_cols_wraps_spacer_head() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(4, 3)
        },
    )
    .unwrap();
    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

        {
            let rac = unsafe { (*page).get_row_and_cell(0, 0) };
            unsafe {
                (*rac.row).set_wrap(true);
                *rac.cell = cell_codepoint('x' as u32);
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 0) };
            unsafe {
                *rac.cell = cell_codepoint('x' as u32);
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(2, 0) };
            unsafe {
                *rac.cell = cell_codepoint('x' as u32);
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(3, 0) };
            unsafe {
                let mut c = cell_codepoint(0);
                c.set_wide(crate::page::Wide::SpacerHead);
                *rac.cell = c;
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(0, 1) };
            unsafe {
                (*rac.row).set_wrap_continuation(true);
                let mut c = cell_codepoint('😀' as u32);
                c.set_wide(crate::page::Wide::Wide);
                *rac.cell = c;
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 1) };
            unsafe {
                let mut c = cell_codepoint(0);
                c.set_wide(crate::page::Wide::SpacerTail);
                *rac.cell = c;
            }
        }
    }

    // Resize
    s.resize(Resize {
        cols: Some(3),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(3usize, s.cols as usize);
    assert_eq!(3usize, s.total_rows());

    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

        {
            let rac = unsafe { (*page).get_row_and_cell(0, 0) };
            assert_eq!('x' as u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::Narrow, unsafe { (*rac.cell).wide() });
            assert!(unsafe { (*rac.row).wrap() });
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 0) };
            assert_eq!('x' as u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::Narrow, unsafe { (*rac.cell).wide() });
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(2, 0) };
            assert_eq!('x' as u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::Narrow, unsafe { (*rac.cell).wide() });
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(0, 1) };
            assert_eq!('😀' as u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::Wide, unsafe { (*rac.cell).wide() });
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 1) };
            assert_eq!(0u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::SpacerTail, unsafe { (*rac.cell).wide() });
        }
    }

    s.deinit();
}

// ref: test "PageList resize reflow less cols cursor goes to scrollback"
#[test]
fn pagelist_resize_reflow_less_cols_cursor_goes_to_scrollback() {
    let mut s = PageList::init(Global, Options::new(4, 2)).unwrap();
    assert!(s.pages.first == s.pages.last);
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
    for y in 0..s.rows {
        for x in 0..s.cols {
            let rac = unsafe { (*page).get_row_and_cell(x as usize, y as usize) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }

    // Put a tracked pin in the history
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { x: 2, y: 0 }))
                .unwrap(),
        )
        .unwrap();

    // Resize
    s.resize(Resize {
        cols: Some(2),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(2usize, s.cols as usize);
    assert_eq!(4usize, s.total_rows());

    // Our cursor should move to the first row
    assert!(s.point_from_pin(point::Tag::Active, pin_copy(p)).is_none());

    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList resize reflow less cols cursor in unchanged row"
#[test]
fn pagelist_resize_reflow_less_cols_cursor_in_unchanged_row() {
    let mut s = PageList::init(Global, Options::new(4, 2)).unwrap();
    assert!(s.pages.first == s.pages.last);
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
    for y in 0..s.rows {
        for x in 0..2 {
            let rac = unsafe { (*page).get_row_and_cell(x as usize, y as usize) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }

    // Put a tracked pin in the history
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { x: 1, y: 0 }))
                .unwrap(),
        )
        .unwrap();

    // Resize
    s.resize(Resize {
        cols: Some(2),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(2usize, s.cols as usize);
    assert_eq!(2usize, s.total_rows());

    // Our cursor should move to the first row
    assert_eq!(
        point::Point::Active(point::Coordinate { x: 1, y: 0 }),
        s.point_from_pin(point::Tag::Active, pin_copy(p)).unwrap()
    );

    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList resize reflow less cols cursor in blank cell"
#[test]
fn pagelist_resize_reflow_less_cols_cursor_in_blank_cell() {
    let mut s = PageList::init(Global, Options::new(6, 2)).unwrap();
    assert!(s.pages.first == s.pages.last);
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
    for y in 0..s.rows {
        for x in 0..2 {
            let rac = unsafe { (*page).get_row_and_cell(x as usize, y as usize) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }

    // Put a tracked pin in the history
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { x: 2, y: 0 }))
                .unwrap(),
        )
        .unwrap();

    // Resize
    s.resize(Resize {
        cols: Some(4),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(4usize, s.cols as usize);
    assert_eq!(2usize, s.total_rows());

    // Our cursor should not move
    assert_eq!(
        point::Point::Active(point::Coordinate { x: 2, y: 0 }),
        s.point_from_pin(point::Tag::Active, pin_copy(p)).unwrap()
    );

    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList resize reflow less cols cursor in final blank cell"
#[test]
fn pagelist_resize_reflow_less_cols_cursor_in_final_blank_cell() {
    let mut s = PageList::init(Global, Options::new(6, 2)).unwrap();
    assert!(s.pages.first == s.pages.last);
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
    for y in 0..s.rows {
        for x in 0..2 {
            let rac = unsafe { (*page).get_row_and_cell(x as usize, y as usize) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }

    // Put a tracked pin in the history
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { x: 3, y: 0 }))
                .unwrap(),
        )
        .unwrap();

    // Resize
    s.resize(Resize {
        cols: Some(4),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(4usize, s.cols as usize);
    assert_eq!(2usize, s.total_rows());

    // Our cursor should move to the first row
    assert_eq!(
        point::Point::Active(point::Coordinate { x: 3, y: 0 }),
        s.point_from_pin(point::Tag::Active, pin_copy(p)).unwrap()
    );

    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList resize reflow less cols cursor in wrapped blank cell"
#[test]
fn pagelist_resize_reflow_less_cols_cursor_in_wrapped_blank_cell() {
    let mut s = PageList::init(Global, Options::new(6, 2)).unwrap();
    assert!(s.pages.first == s.pages.last);
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
    for y in 0..s.rows {
        for x in 0..2 {
            let rac = unsafe { (*page).get_row_and_cell(x as usize, y as usize) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }

    // Put a tracked pin in the history
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { x: 5, y: 0 }))
                .unwrap(),
        )
        .unwrap();

    // Resize
    s.resize(Resize {
        cols: Some(4),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(4usize, s.cols as usize);
    assert_eq!(2usize, s.total_rows());

    // Our cursor should move to the first row
    assert_eq!(
        point::Point::Active(point::Coordinate { x: 3, y: 0 }),
        s.point_from_pin(point::Tag::Active, pin_copy(p)).unwrap()
    );

    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList resize reflow less cols blank lines"
#[test]
fn pagelist_resize_reflow_less_cols_blank_lines() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(4, 3)
        },
    )
    .unwrap();
    assert!(s.pages.first == s.pages.last);
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
    for y in 0..1 {
        for x in 0..4 {
            let rac = unsafe { (*page).get_row_and_cell(x as usize, y as usize) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }

    // Resize
    s.resize(Resize {
        cols: Some(2),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(2usize, s.cols as usize);
    assert_eq!(3usize, s.total_rows());

    let mut it = s.row_iterator(
        crate::pagelist::iter::Direction::RightDown,
        point::Point::Active(point::Coordinate::default()),
        None,
    );
    {
        // First row should be wrapped
        let offset = it.next().unwrap();
        let rac = offset.row_and_cell();
        let cells = unsafe { (*crate::pagelist::Node::page(offset.node)).get_cells(rac.row) };
        assert!(unsafe { (*rac.row).wrap() });
        assert_eq!(2usize, cells.len());
        assert_eq!(0u32, unsafe { (*cells)[0].content_codepoint() });
    }
    {
        let offset = it.next().unwrap();
        let rac = offset.row_and_cell();
        let cells = unsafe { (*crate::pagelist::Node::page(offset.node)).get_cells(rac.row) };
        assert!(!unsafe { (*rac.row).wrap() });
        assert_eq!(2usize, cells.len());
        assert_eq!(2u32, unsafe { (*cells)[0].content_codepoint() });
    }

    s.deinit();
}

// ref: test "PageList resize reflow less cols blank lines between"
#[test]
fn pagelist_resize_reflow_less_cols_blank_lines_between() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(4, 3)
        },
    )
    .unwrap();
    assert!(s.pages.first == s.pages.last);
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
    {
        for x in 0..4 {
            let rac = unsafe { (*page).get_row_and_cell(x as usize, 0) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }
    {
        for x in 0..4 {
            let rac = unsafe { (*page).get_row_and_cell(x as usize, 2) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }

    // Resize
    s.resize(Resize {
        cols: Some(2),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(2usize, s.cols as usize);
    assert_eq!(5usize, s.total_rows());

    let mut it = s.row_iterator(
        crate::pagelist::iter::Direction::RightDown,
        point::Point::Active(point::Coordinate::default()),
        None,
    );
    {
        let offset = it.next().unwrap();
        let rac = offset.row_and_cell();
        assert!(!unsafe { (*rac.row).wrap() });
    }
    {
        let offset = it.next().unwrap();
        let rac = offset.row_and_cell();
        let cells = unsafe { (*crate::pagelist::Node::page(offset.node)).get_cells(rac.row) };
        assert!(unsafe { (*rac.row).wrap() });
        assert_eq!(2usize, cells.len());
        assert_eq!(0u32, unsafe { (*cells)[0].content_codepoint() });
    }
    {
        let offset = it.next().unwrap();
        let rac = offset.row_and_cell();
        let cells = unsafe { (*crate::pagelist::Node::page(offset.node)).get_cells(rac.row) };
        assert!(!unsafe { (*rac.row).wrap() });
        assert_eq!(2usize, cells.len());
        assert_eq!(2u32, unsafe { (*cells)[0].content_codepoint() });
    }

    s.deinit();
}

// ref: test "PageList resize reflow less cols blank lines between no scrollback"
#[test]
fn pagelist_resize_reflow_less_cols_blank_lines_between_no_scrollback() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(5, 3)
        },
    )
    .unwrap();
    assert!(s.pages.first == s.pages.last);
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 0) };
        unsafe {
            *rac.cell = cell_codepoint('A' as u32);
        }
    }
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 2) };
        unsafe {
            *rac.cell = cell_codepoint('C' as u32);
        }
    }

    // Resize
    s.resize(Resize {
        cols: Some(2),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(2usize, s.cols as usize);
    assert_eq!(3usize, s.total_rows());

    let mut it = s.row_iterator(
        crate::pagelist::iter::Direction::RightDown,
        point::Point::Active(point::Coordinate::default()),
        None,
    );
    {
        let offset = it.next().unwrap();
        let rac = offset.row_and_cell();
        let cells = unsafe { (*crate::pagelist::Node::page(offset.node)).get_cells(rac.row) };
        assert!(!unsafe { (*rac.row).wrap() });
        assert_eq!(2usize, cells.len());
        assert_eq!('A' as u32, unsafe { (*cells)[0].content_codepoint() });
    }
    {
        let offset = it.next().unwrap();
        let rac = offset.row_and_cell();
        let cells = unsafe { (*crate::pagelist::Node::page(offset.node)).get_cells(rac.row) };
        assert_eq!(0u32, unsafe { (*cells)[0].content_codepoint() });
    }
    {
        let offset = it.next().unwrap();
        let rac = offset.row_and_cell();
        let cells = unsafe { (*crate::pagelist::Node::page(offset.node)).get_cells(rac.row) };
        assert!(!unsafe { (*rac.row).wrap() });
        assert_eq!(2usize, cells.len());
        assert_eq!('C' as u32, unsafe { (*cells)[0].content_codepoint() });
    }

    s.deinit();
}

// ref: test "PageList resize reflow less cols cursor not on last line preserves location"
#[test]
fn pagelist_resize_reflow_less_cols_cursor_not_on_last_line_preserves_location() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(1),
            ..Options::new(5, 5)
        },
    )
    .unwrap();
    assert!(s.pages.first == s.pages.last);
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
    for y in 0..s.rows {
        for x in 0..2 {
            let rac = unsafe { (*page).get_row_and_cell(x as usize, y as usize) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }

    // Grow blank rows to push our rows back into scrollback
    s.grow_rows(5).unwrap();
    assert_eq!(10usize, s.total_rows());

    // Put a tracked pin in the history
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
                .unwrap(),
        )
        .unwrap();

    // Resize
    s.resize(Resize {
        cols: Some(4),
        reflow: true,

        // Important: not on last row
        cursor: Some(crate::pagelist::resize::Cursor {
            x: 1,
            y: 1,
            pin: None,
        }),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(4usize, s.cols as usize);
    assert_eq!(10usize, s.total_rows());

    // Our cursor should move to the first row
    assert_eq!(
        point::Point::Active(point::Coordinate { x: 0, y: 0 }),
        s.point_from_pin(point::Tag::Active, pin_copy(p)).unwrap()
    );

    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList resize reflow less cols no scrollback pull blank active"
#[test]
fn pagelist_resize_reflow_less_cols_no_scrollback_pull_blank_active() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(1),
            ..Options::new(5, 5)
        },
    )
    .unwrap();
    assert!(s.pages.first == s.pages.last);
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
    for y in 0..s.rows {
        for x in 0..2 {
            let rac = unsafe { (*page).get_row_and_cell(x as usize, y as usize) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }

    // Grow blank rows to push our rows back into scrollback
    s.grow_rows(5).unwrap();
    assert_eq!(10usize, s.total_rows());

    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
                .unwrap(),
        )
        .unwrap();
    // defer s.untrackPin(p);

    // Resize with no cursor. Normally the trailing blank rows would be
    // trimmed and the active area would slide up over our history.
    s.resize(Resize {
        cols: Some(4),
        reflow: true,
        pull_scrollback: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(4usize, s.cols as usize);
    assert_eq!(10usize, s.total_rows());

    // The top of the active area should not move
    assert_eq!(
        point::Point::Active(point::Coordinate { x: 0, y: 0 }),
        s.point_from_pin(point::Tag::Active, pin_copy(p)).unwrap()
    );

    s.untrack_pin(p);
    s.deinit();
}
