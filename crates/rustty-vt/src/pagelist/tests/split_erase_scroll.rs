//! PORTS: ghostty src/terminal/PageList.zig — the `split`, scroll-to-pin/row,
//! prompt-jump, scrollback-limit (max bytes / max lines, integrity), grow-prune
//! and erase-row viewport-offset-cache tests.

use super::cell_codepoint;
use crate::alloc::Global;
use crate::page::{Capacity, SemanticPrompt};
use crate::pagelist::grow::{IncreaseCapacity, Scrollbar, SplitError};
use crate::pagelist::resize::Resize;
use crate::pagelist::scroll::Scroll;
use crate::pagelist::{std_size, IntegrityError, Key, Limits, Options, PageList, Pin, Viewport};
use crate::style::{self as stylepkg, Style};
use crate::{hyperlink, point};
use rustty_render_types::term_size as size;

// =====================================================================
// split
// =====================================================================

// ref: test "PageList split at middle row"
#[test]
fn pagelist_split_at_middle_row() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 10)
        },
    )
    .unwrap();

    {
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

        // Write content to rows: row 0 gets codepoint 0, row 1 gets 1, etc.
        for y in 0..unsafe { (*page).size.rows } as usize {
            let rac = unsafe { (*page).get_row_and_cell(0, y) };
            unsafe {
                *rac.cell = cell_codepoint(y as u32);
            }
        }
    }

    // Split at row 5 (middle)
    let split_pin = Pin {
        node: s.pages.first,
        y: 5,
        x: 0,
        garbage: false,
    };
    s.split(split_pin).unwrap();

    // Verify two pages exist
    assert!(!s.pages.first.is_null());
    assert!(!unsafe { (*s.pages.first).next }.is_null());

    let first_page = unsafe { &(*crate::pagelist::Node::page(s.pages.first)) };
    let second_page = unsafe { &(*crate::pagelist::Node::page((*s.pages.first).next)) };

    // First page should have rows 0-4 (5 rows)
    assert_eq!(5, first_page.size.rows as usize);
    // Second page should have rows 5-9 (5 rows)
    assert_eq!(5, second_page.size.rows as usize);

    // Verify content in first page is preserved (rows 0-4 have codepoints 0-4)
    for y in 0..5 {
        let rac = first_page.get_row_and_cell(0, y);
        assert_eq!(y as u32, unsafe { (*rac.cell).content_codepoint() });
    }

    // Verify content in second page (original rows 5-9, now at y=0-4)
    for y in 0..5 {
        let rac = second_page.get_row_and_cell(0, y);
        assert_eq!((y + 5) as u32, unsafe { (*rac.cell).content_codepoint() });
    }

    s.deinit();
}

// ref: test "PageList split at row 0 is no-op"
#[test]
fn pagelist_split_at_row_0_is_no_op() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 10)
        },
    )
    .unwrap();

    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

    // Write content to all rows
    for y in 0..unsafe { (*page).size.rows } as usize {
        let rac = unsafe { (*page).get_row_and_cell(0, y) };
        unsafe {
            *rac.cell = cell_codepoint(y as u32);
        }
    }

    // Split at row 0 should be a no-op
    let split_pin = Pin {
        node: s.pages.first,
        y: 0,
        x: 0,
        garbage: false,
    };
    s.split(split_pin).unwrap();

    // Verify only one page exists (no split occurred)
    assert!(!s.pages.first.is_null());
    assert!(unsafe { (*s.pages.first).next }.is_null());

    // Verify all content is still in the original page
    let page = unsafe { &(*crate::pagelist::Node::page(s.pages.first)) };
    assert_eq!(10, page.size.rows as usize);
    for y in 0..10 {
        let rac = page.get_row_and_cell(0, y);
        assert_eq!(y as u32, unsafe { (*rac.cell).content_codepoint() });
    }

    s.deinit();
}

// ref: test "PageList split at last row"
#[test]
fn pagelist_split_at_last_row() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 10)
        },
    )
    .unwrap();

    {
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

        // Write content to all rows
        for y in 0..unsafe { (*page).size.rows } as usize {
            let rac = unsafe { (*page).get_row_and_cell(0, y) };
            unsafe {
                *rac.cell = cell_codepoint(y as u32);
            }
        }
    }

    // Split at last row (row 9)
    let split_pin = Pin {
        node: s.pages.first,
        y: 9,
        x: 0,
        garbage: false,
    };
    s.split(split_pin).unwrap();

    // Verify two pages exist
    assert!(!s.pages.first.is_null());
    assert!(!unsafe { (*s.pages.first).next }.is_null());

    let first_page = unsafe { &(*crate::pagelist::Node::page(s.pages.first)) };
    let second_page = unsafe { &(*crate::pagelist::Node::page((*s.pages.first).next)) };

    // First page should have 9 rows
    assert_eq!(9, first_page.size.rows as usize);
    // Second page should have 1 row
    assert_eq!(1, second_page.size.rows as usize);

    // Verify content in second page (original row 9, now at y=0)
    let rac = second_page.get_row_and_cell(0, 0);
    assert_eq!(9, unsafe { (*rac.cell).content_codepoint() });

    s.deinit();
}

// ref: test "PageList split single row page returns OutOfSpace"
#[test]
fn pagelist_split_single_row_page_returns_OutOfSpace() {
    // Initialize with 1 row
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 1)
        },
    )
    .unwrap();

    let split_pin = Pin {
        node: s.pages.first,
        y: 0,
        x: 0,
        garbage: false,
    };
    let result = s.split(split_pin);

    assert_eq!(Err(SplitError::OutOfSpace), result);

    s.deinit();
}

// ref: test "PageList split moves tracked pins"
#[test]
fn pagelist_split_moves_tracked_pins() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 10)
        },
    )
    .unwrap();

    // Track a pin at row 7
    let tracked = s
        .track_pin(Pin {
            node: s.pages.first,
            y: 7,
            x: 3,
            garbage: false,
        })
        .unwrap();

    // Split at row 5
    let split_pin = Pin {
        node: s.pages.first,
        y: 5,
        x: 0,
        garbage: false,
    };
    s.split(split_pin).unwrap();

    // The tracked pin should now be in the second page
    assert!(core::ptr::eq(unsafe { (*tracked).node }, unsafe {
        (*s.pages.first).next
    }));
    // y should be adjusted: was 7, split at 5, so new y = 7 - 5 = 2
    assert_eq!(2, unsafe { (*tracked).y } as usize);
    // x should remain unchanged
    assert_eq!(3, unsafe { (*tracked).x } as usize);

    s.untrack_pin(tracked);
    s.deinit();
}

// ref: test "PageList split tracked pin before split point unchanged"
#[test]
fn pagelist_split_tracked_pin_before_split_point_unchanged() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 10)
        },
    )
    .unwrap();

    let original_node = s.pages.first;

    // Track a pin at row 2 (before the split point)
    let tracked = s
        .track_pin(Pin {
            node: original_node,
            y: 2,
            x: 5,
            garbage: false,
        })
        .unwrap();

    // Split at row 5
    let split_pin = Pin {
        node: original_node,
        y: 5,
        x: 0,
        garbage: false,
    };
    s.split(split_pin).unwrap();

    // The tracked pin should remain in the original page
    assert!(core::ptr::eq(unsafe { (*tracked).node }, s.pages.first));
    // y and x should be unchanged
    assert_eq!(2, unsafe { (*tracked).y } as usize);
    assert_eq!(5, unsafe { (*tracked).x } as usize);

    s.untrack_pin(tracked);
    s.deinit();
}

// ref: test "PageList split tracked pin at split point moves to new page"
#[test]
fn pagelist_split_tracked_pin_at_split_point_moves_to_new_page() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 10)
        },
    )
    .unwrap();

    let original_node = s.pages.first;

    // Track a pin at the exact split point (row 5)
    let tracked = s
        .track_pin(Pin {
            node: original_node,
            y: 5,
            x: 4,
            garbage: false,
        })
        .unwrap();

    // Split at row 5
    let split_pin = Pin {
        node: original_node,
        y: 5,
        x: 0,
        garbage: false,
    };
    s.split(split_pin).unwrap();

    // The tracked pin should be in the new page
    assert!(core::ptr::eq(unsafe { (*tracked).node }, unsafe {
        (*s.pages.first).next
    }));
    // y should be 0 since it was at the split point: 5 - 5 = 0
    assert_eq!(0, unsafe { (*tracked).y } as usize);
    // x should remain unchanged
    assert_eq!(4, unsafe { (*tracked).x } as usize);

    s.untrack_pin(tracked);
    s.deinit();
}

// ref: test "PageList split multiple tracked pins across regions"
#[test]
fn pagelist_split_multiple_tracked_pins_across_regions() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 10)
        },
    )
    .unwrap();

    let original_node = s.pages.first;

    // Track multiple pins in different regions
    let pin_before = s
        .track_pin(Pin {
            node: original_node,
            y: 1,
            x: 0,
            garbage: false,
        })
        .unwrap();
    let pin_at_split = s
        .track_pin(Pin {
            node: original_node,
            y: 5,
            x: 2,
            garbage: false,
        })
        .unwrap();
    let pin_after1 = s
        .track_pin(Pin {
            node: original_node,
            y: 7,
            x: 3,
            garbage: false,
        })
        .unwrap();
    let pin_after2 = s
        .track_pin(Pin {
            node: original_node,
            y: 9,
            x: 8,
            garbage: false,
        })
        .unwrap();

    // Split at row 5
    let split_pin = Pin {
        node: original_node,
        y: 5,
        x: 0,
        garbage: false,
    };
    s.split(split_pin).unwrap();

    let first_page = s.pages.first;
    let second_page = unsafe { (*first_page).next };

    // Pin before split point stays in original page
    assert!(core::ptr::eq(unsafe { (*pin_before).node }, first_page));
    assert_eq!(1, unsafe { (*pin_before).y } as usize);
    assert_eq!(0, unsafe { (*pin_before).x } as usize);

    // Pin at split point moves to new page with y=0
    assert!(core::ptr::eq(unsafe { (*pin_at_split).node }, second_page));
    assert_eq!(0, unsafe { (*pin_at_split).y } as usize);
    assert_eq!(2, unsafe { (*pin_at_split).x } as usize);

    // Pins after split point move to new page with adjusted y
    assert!(core::ptr::eq(unsafe { (*pin_after1).node }, second_page));
    assert_eq!(2, unsafe { (*pin_after1).y } as usize); // 7 - 5 = 2
    assert_eq!(3, unsafe { (*pin_after1).x } as usize);

    assert!(core::ptr::eq(unsafe { (*pin_after2).node }, second_page));
    assert_eq!(4, unsafe { (*pin_after2).y } as usize); // 9 - 5 = 4
    assert_eq!(8, unsafe { (*pin_after2).x } as usize);

    s.untrack_pin(pin_before);
    s.untrack_pin(pin_at_split);
    s.untrack_pin(pin_after1);
    s.untrack_pin(pin_after2);
    s.deinit();
}

// ref: test "PageList split tracked viewport_pin in split region moves correctly"
#[test]
fn pagelist_split_tracked_viewport_pin_in_split_region_moves_correctly() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 10)
        },
    )
    .unwrap();

    let original_node = s.pages.first;

    // Set viewport_pin to row 7 (after split point)
    unsafe {
        (*s.viewport_pin).node = original_node;
        (*s.viewport_pin).y = 7;
        (*s.viewport_pin).x = 6;
    }

    // Split at row 5
    let split_pin = Pin {
        node: original_node,
        y: 5,
        x: 0,
        garbage: false,
    };
    s.split(split_pin).unwrap();

    // viewport_pin should be in the new page
    assert!(core::ptr::eq(unsafe { (*s.viewport_pin).node }, unsafe {
        (*s.pages.first).next
    }));
    // y should be adjusted: 7 - 5 = 2
    assert_eq!(2, unsafe { (*s.viewport_pin).y } as usize);
    // x should remain unchanged
    assert_eq!(6, unsafe { (*s.viewport_pin).x } as usize);

    s.deinit();
}

// ref: test "PageList split middle page preserves linked list order"
#[test]
fn pagelist_split_middle_page_preserves_linked_list_order() {
    // Create a single page with 12 rows
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 12)
        },
    )
    .unwrap();

    // Split at row 4 to create: page1 (rows 0-3), page2 (rows 4-11)
    let first_node = s.pages.first;
    let split_pin1 = Pin {
        node: first_node,
        y: 4,
        x: 0,
        garbage: false,
    };
    s.split(split_pin1).unwrap();

    // Now we have 2 pages
    let page1 = s.pages.first;
    let page2 = unsafe { (*s.pages.first).next };
    assert_eq!(4, unsafe { crate::pagelist::Node::rows(page1) } as usize);
    assert_eq!(8, unsafe { crate::pagelist::Node::rows(page2) } as usize);

    // Split page2 at row 4 to create: page1 -> page2 (rows 0-3) -> page3 (rows 4-7)
    let split_pin2 = Pin {
        node: page2,
        y: 4,
        x: 0,
        garbage: false,
    };
    s.split(split_pin2).unwrap();

    // Now we have 3 pages
    let first = s.pages.first;
    let middle = unsafe { (*first).next };
    let last = unsafe { (*middle).next };

    // Verify linked list order: first -> middle -> last
    assert!(core::ptr::eq(page1, first));
    assert!(core::ptr::eq(page2, middle));
    assert!(core::ptr::eq(s.pages.last, last));

    // Verify prev pointers
    assert!(unsafe { (*first).prev }.is_null());
    assert!(core::ptr::eq(first, unsafe { (*middle).prev }));
    assert!(core::ptr::eq(middle, unsafe { (*last).prev }));

    // Verify next pointers
    assert!(core::ptr::eq(middle, unsafe { (*first).next }));
    assert!(core::ptr::eq(last, unsafe { (*middle).next }));
    assert!(unsafe { (*last).next }.is_null());

    // Verify row counts
    assert_eq!(4, unsafe { crate::pagelist::Node::rows(first) } as usize);
    assert_eq!(4, unsafe { crate::pagelist::Node::rows(middle) } as usize);
    assert_eq!(4, unsafe { crate::pagelist::Node::rows(last) } as usize);

    s.deinit();
}

// ref: test "PageList split last page makes new page the last"
#[test]
fn pagelist_split_last_page_makes_new_page_the_last() {
    // Create a single page with 10 rows
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 10)
        },
    )
    .unwrap();

    // Split to create 2 pages first
    let first_node = s.pages.first;
    let split_pin1 = Pin {
        node: first_node,
        y: 5,
        x: 0,
        garbage: false,
    };
    s.split(split_pin1).unwrap();

    // Now split the last page
    let last_before_split = s.pages.last;
    assert_eq!(5, unsafe { crate::pagelist::Node::rows(last_before_split) }
        as usize);

    let split_pin2 = Pin {
        node: last_before_split,
        y: 2,
        x: 0,
        garbage: false,
    };
    s.split(split_pin2).unwrap();

    // The new page should be the new last
    let new_last = s.pages.last;
    assert!(!core::ptr::eq(new_last, last_before_split));
    assert!(core::ptr::eq(last_before_split, unsafe {
        (*new_last).prev
    }));
    assert!(unsafe { (*new_last).next }.is_null());

    // Verify row counts: original last has 2 rows, new last has 3 rows
    assert_eq!(2, unsafe { crate::pagelist::Node::rows(last_before_split) }
        as usize);
    assert_eq!(3, unsafe { crate::pagelist::Node::rows(new_last) } as usize);

    s.deinit();
}

// ref: test "PageList split first page keeps original as first"
#[test]
fn pagelist_split_first_page_keeps_original_as_first() {
    // Create 2 pages by splitting
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 10)
        },
    )
    .unwrap();

    let original_first = s.pages.first;
    let split_pin1 = Pin {
        node: original_first,
        y: 5,
        x: 0,
        garbage: false,
    };
    s.split(split_pin1).unwrap();

    // Get second page (created by first split)
    let second_page = unsafe { (*s.pages.first).next };

    // Now split the first page again
    let split_pin2 = Pin {
        node: s.pages.first,
        y: 2,
        x: 0,
        garbage: false,
    };
    s.split(split_pin2).unwrap();

    // Original first should still be first
    assert!(core::ptr::eq(original_first, s.pages.first));
    assert!(unsafe { (*s.pages.first).prev }.is_null());

    // New page should be inserted between first and second
    let inserted = unsafe { (*s.pages.first).next };
    assert!(!core::ptr::eq(inserted, second_page));
    assert!(core::ptr::eq(second_page, unsafe { (*inserted).next }));

    // Verify row counts: first has 2, inserted has 3, second has 5
    assert_eq!(2, unsafe { crate::pagelist::Node::rows(s.pages.first) }
        as usize);
    assert_eq!(3, unsafe { crate::pagelist::Node::rows(inserted) } as usize);
    assert_eq!(5, unsafe { crate::pagelist::Node::rows(second_page) }
        as usize);

    s.deinit();
}

// ref: test "PageList split preserves wrap flags"
#[test]
fn pagelist_split_preserves_wrap_flags() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 10)
        },
    )
    .unwrap();

    {
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

        // Set wrap flags on rows that will be in the second page after split
        // Row 5: wrap = true (this is the start of a wrapped line)
        // Row 6: wrap_continuation = true (this continues the wrap)
        // Row 7: wrap = true, wrap_continuation = true (wrapped and continues)
        {
            let rac5 = unsafe { (*page).get_row_and_cell(0, 5) };
            unsafe { (*rac5.row).set_wrap(true) };

            let rac6 = unsafe { (*page).get_row_and_cell(0, 6) };
            unsafe { (*rac6.row).set_wrap_continuation(true) };

            let rac7 = unsafe { (*page).get_row_and_cell(0, 7) };
            unsafe {
                (*rac7.row).set_wrap(true);
                (*rac7.row).set_wrap_continuation(true);
            }
        }
    }

    // Split at row 5
    let split_pin = Pin {
        node: s.pages.first,
        y: 5,
        x: 0,
        garbage: false,
    };
    s.split(split_pin).unwrap();

    let second_page = unsafe { &(*crate::pagelist::Node::page((*s.pages.first).next)) };

    // Verify wrap flags are preserved in new page
    // Original row 5 is now row 0 in second page
    {
        let rac0 = second_page.get_row_and_cell(0, 0);
        assert!(unsafe { (*rac0.row).wrap() });
        assert!(!unsafe { (*rac0.row).wrap_continuation() });
    }

    // Original row 6 is now row 1 in second page
    {
        let rac1 = second_page.get_row_and_cell(0, 1);
        assert!(!unsafe { (*rac1.row).wrap() });
        assert!(unsafe { (*rac1.row).wrap_continuation() });
    }

    // Original row 7 is now row 2 in second page
    {
        let rac2 = second_page.get_row_and_cell(0, 2);
        assert!(unsafe { (*rac2.row).wrap() });
        assert!(unsafe { (*rac2.row).wrap_continuation() });
    }

    s.deinit();
}

// ref: test "PageList split preserves styled cells"
#[test]
fn pagelist_split_preserves_styled_cells() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 10)
        },
    )
    .unwrap();

    {
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

        // Create a style and apply it to cells in rows 5-7 (which will be in the second page)
        let style = Style {
            flags: stylepkg::Flags {
                bold: true,
                ..stylepkg::Flags::default()
            },
            ..Style::default()
        };
        let base = unsafe { (*page).memory_ptr() };
        let style_id = unsafe { (*page).styles.add(base, style) }.unwrap();

        for y in 5..8 {
            let rac = unsafe { (*page).get_row_and_cell(0, y) };
            unsafe {
                let mut c = cell_codepoint(b'S' as u32);
                c.set_style_id(style_id);
                *rac.cell = c;
                (*rac.row).set_styled(true);
                (*page).styles.use_id(base, style_id);
            }
        }
        // Release the extra ref from add
        unsafe { (*page).styles.release(base, style_id) };
    }

    // Split at row 5
    let split_pin = Pin {
        node: s.pages.first,
        y: 5,
        x: 0,
        garbage: false,
    };
    s.split(split_pin).unwrap();

    let first_page = unsafe { &(*crate::pagelist::Node::page(s.pages.first)) };
    let second_page = unsafe { &(*crate::pagelist::Node::page((*s.pages.first).next)) };

    // First page should have no styles (all styled rows moved to second page)
    assert_eq!(0, first_page.styles.count());

    // Second page should have exactly 1 style (the bold style, used by 3 cells)
    assert_eq!(1, second_page.styles.count());

    // Verify styled cells are preserved in new page
    for y in 0..3 {
        let rac = second_page.get_row_and_cell(0, y);
        assert_eq!(b'S' as u32, unsafe { (*rac.cell).content_codepoint() });
        assert!(unsafe { (*rac.cell).style_id() } != 0);

        let got_style = unsafe {
            &*second_page
                .styles
                .get(second_page.memory_ptr(), (*rac.cell).style_id())
        };
        assert!(got_style.flags.bold);
        assert!(unsafe { (*rac.row).styled() });
    }

    s.deinit();
}

// PORT NOTE (rustty-only, NOT a reference test — VALIDATION_REPORT §3e/§3c.9):
// pins the ERROR-PATH integrity gate of `erase_row`. Zig's
// `defer self.assertIntegrity()` runs on the `try cloneRowFrom` ERROR return;
// the pre-fix Rust body skipped the gate via `?`, so an OOM at exactly the
// point the oracle checks for corruption went ungated. The test drives the
// real error path (cross-page clone into a page whose style set is FULL →
// `CloneFromError::StyleSet*` → `Err(OutOfMemory)`) with a deliberately
// corrupted `total_rows`, and requires the integrity gate to PANIC on that
// error exit (pre-fix: the call returned `Err` silently — fail-first).
#[test]
// WS18 release lane: this test drives a slow-runtime-safety mechanism (the
// pagelist integrity gate / `pin_is_valid`), which the port compiles out when
// `debug_assertions` is off — faithful to the reference's
// `build_options.slow_runtime_safety`, under which the oracle's test only
// exists. Debug-only; it still runs and passes there.
#[cfg_attr(
    not(debug_assertions),
    ignore = "DEBUG-ONLY BY DESIGN, and this reason was MEASURED (fix round 1 of parity S166-F2): this test drives the slow-runtime-safety mechanism — `PageList::assert_integrity` / `pin_is_valid` — which the port compiles out when `debug_assertions` is off, faithful to the reference's `build_options.slow_runtime_safety`, under which the oracle's counterpart only exists. It is NOT ignored because of the tripwire gate (that claim was false and is gone): un-ignoring it turns the release lane RED, which is why it stays."
)]
fn pagelist_erase_row_error_path_runs_integrity_gate() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 10)
        },
    )
    .unwrap();

    // Give the (future) second page's top row a styled cell, then split so
    // pages.first (rows 0-4) has a next page whose row 0 carries the style.
    {
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
        let base = unsafe { (*page).memory_ptr() };
        let style = Style {
            flags: stylepkg::Flags {
                bold: true,
                ..stylepkg::Flags::default()
            },
            ..Style::default()
        };
        let style_id = unsafe { (*page).styles.add(base, style) }.unwrap();
        let rac = unsafe { (*page).get_row_and_cell(0, 5) };
        unsafe {
            let mut c = cell_codepoint(b'S' as u32);
            c.set_style_id(style_id);
            *rac.cell = c;
            (*rac.row).set_styled(true);
        }
    }
    let split_pin = Pin {
        node: s.pages.first,
        y: 5,
        x: 0,
        garbage: false,
    };
    s.split(split_pin).unwrap();
    assert_eq!(2, s.total_pages());

    // Fill the FIRST page's style set to capacity with unique (orphan) styles
    // so the erase's cross-page clone cannot add the styled row's style there.
    {
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
        let base = unsafe { (*page).memory_ptr() };
        let mut n: u32 = 1;
        loop {
            let style = Style {
                fg_color: stylepkg::Color::Rgb(crate::color::RGB {
                    r: (n & 0xFF) as u8,
                    g: ((n >> 8) & 0xFF) as u8,
                    b: ((n >> 16) & 0xFF) as u8,
                }),
                ..Style::default()
            };
            if unsafe { (*page).styles.add(base, style) }.is_err() {
                break;
            }
            n += 1;
        }
    }

    // Corrupt a PageList invariant the integrity gate checks (cached row count),
    // then drive the erase ERROR path: the gate must fire ON THE ERROR EXIT.
    s.total_rows += 1;
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        s.erase_row(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
    }));
    match panicked {
        // post-fix: the defer-image integrity gate panicked on the error exit
        Err(_) => {}
        Ok(result) => panic!(
            "erase_row must run the integrity gate on its error exit \
             (Zig `defer assertIntegrity()`); got silent {result:?}"
        ),
    }

    // Undo the corruption so deinit-time state is consistent again.
    s.total_rows -= 1;
    s.deinit();
}

// ref: test "PageList split preserves grapheme clusters"
#[test]
fn pagelist_split_preserves_grapheme_clusters() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 10)
        },
    )
    .unwrap();

    {
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

        // Add a grapheme cluster to row 6 (will be row 1 in second page after split at 5)
        {
            let rac = unsafe { (*page).get_row_and_cell(0, 6) };
            unsafe {
                *rac.cell = cell_codepoint(0x1F468); // Man emoji
            }
            unsafe {
                (*page).set_graphemes(
                    rac.row,
                    rac.cell,
                    &[
                        0x200D,  // ZWJ
                        0x1F469, // Woman emoji
                    ],
                )
            }
            .unwrap();
        }
    }

    // Split at row 5
    let split_pin = Pin {
        node: s.pages.first,
        y: 5,
        x: 0,
        garbage: false,
    };
    s.split(split_pin).unwrap();

    let first_page = unsafe { &(*crate::pagelist::Node::page(s.pages.first)) };
    let second_page = unsafe { &(*crate::pagelist::Node::page((*s.pages.first).next)) };

    // First page should have no graphemes (the grapheme row moved to second page)
    assert_eq!(0, first_page.grapheme_count());

    // Second page should have exactly 1 grapheme
    assert_eq!(1, second_page.grapheme_count());

    // Verify grapheme is preserved in new page (original row 6 is now row 1)
    {
        let rac = second_page.get_row_and_cell(0, 1);
        assert_eq!(0x1F468, unsafe { (*rac.cell).content_codepoint() });
        assert!(unsafe { (*rac.row).grapheme() });

        let cps = second_page.lookup_grapheme(rac.cell).unwrap();
        assert_eq!(2, cps.len());
        assert_eq!(0x200D, cps[0]);
        assert_eq!(0x1F469, cps[1]);
    }

    s.deinit();
}

// ref: test "PageList split preserves hyperlinks"
#[test]
fn pagelist_split_preserves_hyperlinks() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 10)
        },
    )
    .unwrap();

    let hyperlink_id = {
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

        // Add a hyperlink to row 7 (will be row 2 in second page after split at 5)
        let hyperlink_id = unsafe {
            (*page).insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(0),
                uri: b"https://example.com".to_vec(),
            })
        }
        .unwrap();
        {
            let rac = unsafe { (*page).get_row_and_cell(0, 7) };
            unsafe {
                *rac.cell = cell_codepoint(b'L' as u32);
            }
            unsafe { (*page).set_hyperlink(rac.row, rac.cell, hyperlink_id) }.unwrap();
        }
        hyperlink_id
    };
    let _ = hyperlink_id;

    // Split at row 5
    let split_pin = Pin {
        node: s.pages.first,
        y: 5,
        x: 0,
        garbage: false,
    };
    s.split(split_pin).unwrap();

    let first_page = unsafe { &(*crate::pagelist::Node::page(s.pages.first)) };
    let second_page = unsafe { &(*crate::pagelist::Node::page((*s.pages.first).next)) };

    // First page should have no hyperlinks (the hyperlink row moved to second page)
    assert_eq!(0, first_page.hyperlink_set.count());

    // Second page should have exactly 1 hyperlink
    assert_eq!(1, second_page.hyperlink_set.count());

    // Verify hyperlink is preserved in new page (original row 7 is now row 2)
    {
        let rac = second_page.get_row_and_cell(0, 2);
        assert_eq!(b'L' as u32, unsafe { (*rac.cell).content_codepoint() });
        assert!(unsafe { (*rac.cell).hyperlink() });

        let link_id = second_page.lookup_hyperlink(rac.cell).unwrap();
        let link = unsafe {
            &*second_page
                .hyperlink_set
                .get(second_page.memory_ptr(), link_id)
        };
        assert_eq!(&b"https://example.com"[..], unsafe {
            link.uri.slice(second_page.memory_ptr())
        });
    }

    s.deinit();
}

// ref: test "PageList scroll delta row back without space preserves active"
#[test]
fn pagelist_scroll_delta_row_back_without_space_preserves_active() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
    s.scroll(Scroll::DeltaRow(-1));

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 0 }), pt);
    }

    assert!(s.viewport == crate::pagelist::Viewport::Active);

    assert_eq!(
        Scrollbar {
            total: s.total_rows(),
            offset: s.total_rows - s.rows as usize,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    s.deinit();
}

// ref: test "PageList scroll to pin"
#[test]
fn pagelist_scroll_to_pin() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
    s.grow_rows(10).unwrap();

    s.scroll(Scroll::Pin(
        s.pin(point::Point::Screen(point::Coordinate { x: 2, y: 4 }))
            .unwrap(),
    ));

    assert_eq!(
        Scrollbar {
            total: s.total_rows(),
            offset: 4,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 4 }), pt);
    }

    s.scroll(Scroll::Pin(
        s.pin(point::Point::Screen(point::Coordinate { x: 2, y: 5 }))
            .unwrap(),
    ));

    assert_eq!(
        Scrollbar {
            total: s.total_rows(),
            offset: 5,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 5 }), pt);
    }

    s.deinit();
}

// ref: test "PageList scroll to pin in active"
#[test]
fn pagelist_scroll_to_pin_in_active() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
    s.grow_rows(10).unwrap();

    s.scroll(Scroll::Pin(
        s.pin(point::Point::Screen(point::Coordinate { x: 2, y: 30 }))
            .unwrap(),
    ));

    assert_eq!(
        Scrollbar {
            total: s.total_rows(),
            offset: s.total_rows - s.rows as usize,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 10 }), pt);
    }

    s.deinit();
}

// ref: test "PageList scroll to pin at top"
#[test]
fn pagelist_scroll_to_pin_at_top() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
    s.grow_rows(10).unwrap();

    s.scroll(Scroll::Pin(
        s.pin(point::Point::Screen(point::Coordinate { x: 2, y: 0 }))
            .unwrap(),
    ));

    assert!(s.viewport == crate::pagelist::Viewport::Top);

    assert_eq!(
        Scrollbar {
            total: s.total_rows(),
            offset: 0,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 0 }), pt);
    }

    s.deinit();
}

// ref: test "PageList scroll to row 0"
#[test]
fn pagelist_scroll_to_row_0() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
    s.grow_rows(10).unwrap();

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 10 }), pt);
    }

    s.scroll(Scroll::Row(0));
    assert!(s.viewport == crate::pagelist::Viewport::Top);

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 0 }), pt);
    }

    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: 0,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    s.grow_rows(10).unwrap();
    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 0 }), pt);
    }

    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: 0,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    s.deinit();
}

// ref: test "PageList scroll to row in scrollback"
#[test]
fn pagelist_scroll_to_row_in_scrollback() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
    s.grow_rows(20).unwrap();

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 20 }), pt);
    }

    s.scroll(Scroll::Row(5));
    assert!(s.viewport == crate::pagelist::Viewport::Pin);
    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: 5,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 5 }), pt);
    }

    s.grow_rows(10).unwrap();
    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 5 }), pt);
    }

    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: 5,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    s.deinit();
}

// ref: test "PageList scroll to row in middle"
#[test]
fn pagelist_scroll_to_row_in_middle() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
    s.grow_rows(50).unwrap();

    let total = s.total_rows;
    let midpoint = total / 2;
    s.scroll(Scroll::Row(midpoint));

    assert!(s.viewport == crate::pagelist::Viewport::Pin);
    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: midpoint,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(
            point::Point::Screen(point::Coordinate {
                x: 0,
                y: midpoint as u32,
            }),
            pt
        );
    }

    s.grow_rows(10).unwrap();
    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(
            point::Point::Screen(point::Coordinate {
                x: 0,
                y: midpoint as u32,
            }),
            pt
        );
    }

    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: midpoint,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    s.deinit();
}

// ref: test "PageList scroll to row at active boundary"
#[test]
fn pagelist_scroll_to_row_at_active_boundary() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
    s.grow_rows(20).unwrap();

    let active_start = s.total_rows - s.rows as usize;

    s.scroll(Scroll::Row(active_start));

    assert!(s.viewport == crate::pagelist::Viewport::Active);

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(
            point::Point::Screen(point::Coordinate {
                x: 0,
                y: active_start as u32,
            }),
            pt
        );
    }

    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: s.total_rows - s.rows as usize,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    s.grow_rows(10).unwrap();

    assert!(s.viewport == crate::pagelist::Viewport::Active);

    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: s.total_rows - s.rows as usize,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    s.deinit();
}

// ref: test "PageList scroll to row beyond active"
#[test]
fn pagelist_scroll_to_row_beyond_active() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
    s.grow_rows(10).unwrap();

    s.scroll(Scroll::Row(1000));

    assert!(s.viewport == crate::pagelist::Viewport::Active);

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 10 }), pt);
    }

    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: s.total_rows - s.rows as usize,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    s.deinit();
}

// ref: test "PageList scroll to row without scrollback"
#[test]
fn pagelist_scroll_to_row_without_scrollback() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

    s.scroll(Scroll::Row(5));

    assert!(s.viewport == crate::pagelist::Viewport::Active);

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 0 }), pt);
    }

    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: s.total_rows - s.rows as usize,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    s.deinit();
}

// ref: test "PageList scroll to row then delta"
#[test]
fn pagelist_scroll_to_row_then_delta() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
    s.grow_rows(30).unwrap();

    s.scroll(Scroll::Row(10));

    assert!(s.viewport == crate::pagelist::Viewport::Pin);

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 10 }), pt);
    }

    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: 10,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    s.scroll(Scroll::DeltaRow(5));

    assert!(s.viewport == crate::pagelist::Viewport::Pin);

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 15 }), pt);
    }

    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: 15,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    s.scroll(Scroll::DeltaRow(-3));

    assert!(s.viewport == crate::pagelist::Viewport::Pin);

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 12 }), pt);
    }

    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: 12,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    s.deinit();
}

// ref: test "PageList scroll to row with cache fast path down"
#[test]
fn pagelist_scroll_to_row_with_cache_fast_path_down() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
    s.grow_rows(50).unwrap();

    s.scroll(Scroll::Row(10));

    assert!(s.viewport == crate::pagelist::Viewport::Pin);
    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: 10,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 10 }), pt);
    }

    // Verify cache is populated
    assert!(s.viewport_pin_row_offset.is_some());
    assert_eq!(10_usize, s.viewport_pin_row_offset.unwrap());

    // Now scroll to a different row - this should use the fast path
    s.scroll(Scroll::Row(20));

    assert!(s.viewport == crate::pagelist::Viewport::Pin);
    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: 20,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 20 }), pt);
    }

    s.grow_rows(10).unwrap();
    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 20 }), pt);
    }

    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: 20,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    s.deinit();
}

// ref: test "PageList scroll to row with cache fast path up"
#[test]
fn pagelist_scroll_to_row_with_cache_fast_path_up() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
    s.grow_rows(50).unwrap();

    s.scroll(Scroll::Row(30));

    assert!(s.viewport == crate::pagelist::Viewport::Pin);
    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: 30,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 30 }), pt);
    }

    // Verify cache is populated
    assert!(s.viewport_pin_row_offset.is_some());
    assert_eq!(30_usize, s.viewport_pin_row_offset.unwrap());

    // Now scroll up to a different row - this should use the fast path
    s.scroll(Scroll::Row(15));

    assert!(s.viewport == crate::pagelist::Viewport::Pin);
    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: 15,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 15 }), pt);
    }

    s.grow_rows(10).unwrap();
    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 15 }), pt);
    }

    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: 15,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    s.deinit();
}

// ref: test "PageList scroll clear"
#[test]
fn pagelist_scroll_clear() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

    {
        let cell = s
            .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
            .unwrap();
        unsafe {
            *cell.cell = cell_codepoint('A' as u32);
        }
    }
    {
        let cell = s
            .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 1 }))
            .unwrap();
        unsafe {
            *cell.cell = cell_codepoint('A' as u32);
        }
    }

    s.scroll_clear().unwrap();

    {
        let pt = s
            .get_cell(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
            .unwrap()
            .screen_point();
        assert_eq!(point::Point::Screen(point::Coordinate { x: 0, y: 2 }), pt);
    }

    s.deinit();
}

// ref: test "PageList: jump zero prompts"
#[test]
fn pagelist_jump_zero_prompts() {
    let mut s = PageList::init(Global, Options::new(5, 3)).unwrap();
    s.grow_rows(3).unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 1) };
        unsafe {
            (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt);
        }
    }
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 5) };
        unsafe {
            (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt);
        }
    }

    s.scroll(Scroll::DeltaPrompt(0));
    assert!(s.viewport == crate::pagelist::Viewport::Active);

    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: s.total_rows - s.rows as usize,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    s.deinit();
}

// ref: test "Screen: jump back one prompt"
#[test]
fn screen_jump_back_one_prompt() {
    let mut s = PageList::init(Global, Options::new(5, 3)).unwrap();
    s.grow_rows(3).unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 1) };
        unsafe {
            (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt);
        }
    }
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 5) };
        unsafe {
            (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt);
        }
    }

    // Jump back
    {
        s.scroll(Scroll::DeltaPrompt(-1));
        assert!(s.viewport == crate::pagelist::Viewport::Pin);
        assert_eq!(
            point::Point::Screen(point::Coordinate { x: 0, y: 1 }),
            s.point_from_pin(
                point::Tag::Screen,
                s.pin(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
                    .unwrap()
            )
            .unwrap()
        );

        assert_eq!(
            Scrollbar {
                total: s.total_rows,
                offset: 1,
                len: s.rows as usize,
            },
            s.scrollbar()
        );
    }
    {
        s.scroll(Scroll::DeltaPrompt(-1));
        assert!(s.viewport == crate::pagelist::Viewport::Pin);
        assert_eq!(
            point::Point::Screen(point::Coordinate { x: 0, y: 1 }),
            s.point_from_pin(
                point::Tag::Screen,
                s.pin(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
                    .unwrap()
            )
            .unwrap()
        );

        assert_eq!(
            Scrollbar {
                total: s.total_rows,
                offset: 1,
                len: s.rows as usize,
            },
            s.scrollbar()
        );
    }

    // Jump forward
    {
        s.scroll(Scroll::DeltaPrompt(1));
        assert!(s.viewport == crate::pagelist::Viewport::Active);
        assert_eq!(
            Scrollbar {
                total: s.total_rows,
                offset: s.total_rows - s.rows as usize,
                len: s.rows as usize,
            },
            s.scrollbar()
        );
    }
    {
        s.scroll(Scroll::DeltaPrompt(1));
        assert!(s.viewport == crate::pagelist::Viewport::Active);
        assert_eq!(
            Scrollbar {
                total: s.total_rows,
                offset: s.total_rows - s.rows as usize,
                len: s.rows as usize,
            },
            s.scrollbar()
        );
    }

    s.deinit();
}

// ref: test "Screen: jump forward prompt skips multiline continuation"
#[test]
fn screen_jump_forward_prompt_skips_multiline_continuation() {
    let mut s = PageList::init(Global, Options::new(5, 3)).unwrap();
    s.grow_rows(7).unwrap();

    // Multiline prompt on rows 1-3.
    {
        let p = s
            .pin(point::Point::Screen(point::Coordinate { x: 0, y: 1 }))
            .unwrap();
        unsafe {
            (*p.row_and_cell().row).set_semantic_prompt(SemanticPrompt::Prompt);
        }
    }
    {
        let p = s
            .pin(point::Point::Screen(point::Coordinate { x: 0, y: 2 }))
            .unwrap();
        unsafe {
            (*p.row_and_cell().row).set_semantic_prompt(SemanticPrompt::PromptContinuation);
        }
    }
    {
        let p = s
            .pin(point::Point::Screen(point::Coordinate { x: 0, y: 3 }))
            .unwrap();
        unsafe {
            (*p.row_and_cell().row).set_semantic_prompt(SemanticPrompt::PromptContinuation);
        }
    }

    // Next prompt after command output.
    {
        let p = s
            .pin(point::Point::Screen(point::Coordinate { x: 0, y: 6 }))
            .unwrap();
        unsafe {
            (*p.row_and_cell().row).set_semantic_prompt(SemanticPrompt::Prompt);
        }
    }

    // Starting at the first prompt line should jump to the next prompt,
    // not to continuation lines.
    s.scroll(Scroll::Row(1));
    s.scroll(Scroll::DeltaPrompt(1));
    assert!(s.viewport == crate::pagelist::Viewport::Pin);
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 0, y: 6 }),
        s.point_from_pin(
            point::Tag::Screen,
            s.pin(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
                .unwrap()
        )
        .unwrap()
    );

    // Starting in the middle of continuation lines should also jump to
    // the next prompt.
    s.scroll(Scroll::Row(2));
    s.scroll(Scroll::DeltaPrompt(1));
    assert!(s.viewport == crate::pagelist::Viewport::Pin);
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 0, y: 6 }),
        s.point_from_pin(
            point::Tag::Screen,
            s.pin(point::Point::Viewport(point::Coordinate { x: 0, y: 0 }))
                .unwrap()
        )
        .unwrap()
    );

    s.deinit();
}

// ref: test "PageList set max bytes prunes immediately and can be raised"
#[test]
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound in the debug-assertions-ON G6 lane, where `Page::assert_integrity` runs on nearly every page mutation (measured per test; see the C-1 table there). It RUNS in the G6c lane (`check_leaks.sh --debug-assertions-off`). Do NOT widen this to a plain `miri` ignore: that would drop the test from BOTH lanes."
)]
fn pagelist_set_max_bytes_prunes_immediately_and_can_be_raised() {
    let cols: size::CellCountInt = 80;
    let page_rows: usize = PageList::initial_capacity(cols).rows as usize;

    let mut s = PageList::init(
        Global,
        Options {
            max_size: None,
            ..Options::new(cols, 1)
        },
    )
    .unwrap();

    // Build four complete pages of history followed by the active row.
    s.grow_rows(4 * page_rows).unwrap();
    assert_eq!(5usize, s.total_pages());

    let removed = s.pages.first;
    let retained = unsafe { (*s.pages.last).prev };
    let removed_pin = s
        .track_pin(Pin {
            node: removed,
            y: 0,
            x: 0,
            garbage: false,
        })
        .unwrap();
    let retained_pin = s
        .track_pin(Pin {
            node: retained,
            y: 0,
            x: 0,
            garbage: false,
        })
        .unwrap();

    s.scroll(Scroll::Pin(unsafe { (*retained_pin).pin_copy() }));
    assert_eq!(3 * page_rows, s.scrollbar().offset);

    // The active-area minimum is two pages. Lowering below that immediately
    // removes all older complete historical pages.
    s.set_max_bytes(Some(std_size()));
    assert_eq!(std_size(), s.limits.bytes.explicit);
    assert_eq!(2 * std_size(), s.limits.max(Key::Bytes));
    assert_eq!(s.limits.max(Key::Bytes), s.page_size);
    assert_eq!(2usize, s.total_pages());
    assert_eq!(page_rows, s.total_rows - s.rows as usize);
    assert!(core::ptr::eq(retained, s.pages.first));
    assert!(core::ptr::eq(retained, unsafe { (*removed_pin).node }));
    assert!(unsafe { (*removed_pin).garbage });
    assert!(core::ptr::eq(retained, unsafe { (*retained_pin).node }));
    assert!(!unsafe { (*retained_pin).garbage });
    assert_eq!(0usize, s.scrollbar().offset);

    // Raising the limit doesn't allocate or otherwise change retained data,
    // but subsequent growth can exceed the previous effective limit.
    let limited_size = s.page_size;
    let limited_rows = s.total_rows;
    s.set_max_bytes(Some(8 * std_size()));
    assert_eq!(limited_size, s.page_size);
    assert_eq!(limited_rows, s.total_rows);
    s.grow_rows(2 * page_rows).unwrap();
    assert!(s.page_size > limited_size);

    // Null restores unlimited growth and likewise preserves current data.
    let raised_size = s.page_size;
    let raised_rows = s.total_rows;
    s.set_max_bytes(None);
    assert_eq!(usize::MAX, s.limits.bytes.explicit);
    assert_eq!(raised_size, s.page_size);
    assert_eq!(raised_rows, s.total_rows);
    s.grow_rows(5 * page_rows).unwrap();
    assert!(s.page_size > 8 * std_size());

    s.untrack_pin(retained_pin);
    s.untrack_pin(removed_pin);
    s.deinit();
}

// ref: test "PageList set max bytes zero preserves active boundary"
#[test]
fn pagelist_set_max_bytes_zero_preserves_active_boundary() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: None,
            ..Options::new(80, 1)
        },
    )
    .unwrap();

    // Make the sole page larger than the effective zero-byte limit. Its first
    // row will be history, but the same indivisible page also contains active.
    while s.page_size <= s.limits.bytes.min {
        let _ = s
            .increase_capacity(s.pages.first, Some(IncreaseCapacity::GraphemeBytes))
            .unwrap();
    }
    let _ = s.grow().unwrap();
    assert_eq!(1usize, s.total_pages());
    assert!(core::ptr::eq(
        s.pages.first,
        s.get_top_left(point::Tag::Active).node
    ));
    assert!(s.get_top_left(point::Tag::Active).y > 0);

    s.scroll(Scroll::Top);
    assert!(s.viewport == crate::pagelist::Viewport::Top);

    s.set_max_bytes(Some(0));
    assert_eq!(0usize, s.limits.bytes.explicit);
    assert!(s.page_size > s.limits.max(Key::Bytes));
    assert_eq!(1usize, s.total_pages());
    assert!(s.viewport == crate::pagelist::Viewport::Active);
    assert_eq!(
        Scrollbar {
            total: s.rows as usize,
            offset: 0,
            len: s.rows as usize,
        },
        s.scrollbar()
    );

    // No-scrollback mode cannot be moved back into the retained boundary row.
    s.scroll(Scroll::Top);
    assert!(s.viewport == crate::pagelist::Viewport::Active);

    s.deinit();
}

// ref: test "PageList set max lines prunes immediately and can be raised"
#[test]
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound in the debug-assertions-ON G6 lane, where `Page::assert_integrity` runs on nearly every page mutation (measured per test; see the C-1 table there). It RUNS in the G6c lane (`check_leaks.sh --debug-assertions-off`). Do NOT widen this to a plain `miri` ignore: that would drop the test from BOTH lanes."
)]
fn pagelist_set_max_lines_prunes_immediately_and_can_be_raised() {
    let cols: size::CellCountInt = 80;
    let page_rows: usize = PageList::initial_capacity(cols).rows as usize;
    let lowered_lines = page_rows + page_rows / 2;

    let mut s = PageList::init(
        Global,
        Options {
            max_size: None,
            max_lines: None,
            ..Options::new(cols, 1)
        },
    )
    .unwrap();

    s.grow_rows(4 * page_rows).unwrap();
    assert_eq!(5usize, s.total_pages());

    let removed = s.pages.first;
    let retained = unsafe { (*s.pages.last).prev };
    let removed_pin = s
        .track_pin(Pin {
            node: removed,
            y: 0,
            x: 0,
            garbage: false,
        })
        .unwrap();
    let retained_pin = s
        .track_pin(Pin {
            node: retained,
            y: 0,
            x: 0,
            garbage: false,
        })
        .unwrap();

    s.scroll(Scroll::Pin(unsafe { (*retained_pin).pin_copy() }));
    assert_eq!(3 * page_rows, s.scrollbar().offset);

    // Whole-page enforcement undershoots a non-page-aligned line limit.
    s.set_max_lines(Some(lowered_lines));
    assert_eq!(lowered_lines, s.limits.lines.explicit);
    assert_eq!(lowered_lines, s.limits.max(Key::Lines));
    assert_eq!(page_rows, s.total_rows - s.rows as usize);
    assert_eq!(2usize, s.total_pages());
    assert!(core::ptr::eq(retained, s.pages.first));
    assert!(core::ptr::eq(retained, unsafe { (*removed_pin).node }));
    assert!(unsafe { (*removed_pin).garbage });
    assert!(core::ptr::eq(retained, unsafe { (*retained_pin).node }));
    assert!(!unsafe { (*retained_pin).garbage });
    assert_eq!(0usize, s.scrollbar().offset);

    let limited_size = s.page_size;
    let limited_rows = s.total_rows;
    s.set_max_lines(Some(4 * page_rows));
    assert_eq!(limited_size, s.page_size);
    assert_eq!(limited_rows, s.total_rows);
    s.grow_rows(2 * page_rows).unwrap();
    assert!(s.total_rows - s.rows as usize > lowered_lines);

    let raised_size = s.page_size;
    let raised_rows = s.total_rows;
    s.set_max_lines(None);
    assert_eq!(usize::MAX, s.limits.lines.explicit);
    assert_eq!(raised_size, s.page_size);
    assert_eq!(raised_rows, s.total_rows);
    s.grow_rows(3 * page_rows).unwrap();
    assert!(s.total_rows - s.rows as usize > 4 * page_rows);

    s.untrack_pin(retained_pin);
    s.untrack_pin(removed_pin);
    s.deinit();
}

// ref: test "PageList set max limits remain independent"
#[test]
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound in the debug-assertions-ON G6 lane, where `Page::assert_integrity` runs on nearly every page mutation (measured per test; see the C-1 table there). It RUNS in the G6c lane (`check_leaks.sh --debug-assertions-off`). Do NOT widen this to a plain `miri` ignore: that would drop the test from BOTH lanes."
)]
fn pagelist_set_max_limits_remain_independent() {
    let cols: size::CellCountInt = 80;
    let page_rows: usize = PageList::initial_capacity(cols).rows as usize;
    let byte_limit = 3 * std_size();
    let line_limit = page_rows / 2;

    let mut s = PageList::init(
        Global,
        Options {
            max_size: None,
            max_lines: None,
            ..Options::new(cols, 1)
        },
    )
    .unwrap();

    s.grow_rows(4 * page_rows).unwrap();

    // The byte setter leaves the line limit unlimited.
    s.set_max_bytes(Some(byte_limit));
    assert_eq!(byte_limit, s.limits.bytes.explicit);
    assert_eq!(usize::MAX, s.limits.lines.explicit);
    assert_eq!(3usize, s.total_pages());
    assert_eq!(2 * page_rows, s.total_rows - s.rows as usize);

    // The smaller runtime line limit prunes one more complete page without
    // changing the configured byte limit. Its effective value is raised to
    // the existing one-page minimum.
    s.set_max_lines(Some(line_limit));
    assert_eq!(byte_limit, s.limits.bytes.explicit);
    assert_eq!(line_limit, s.limits.lines.explicit);
    assert_eq!(page_rows, s.limits.max(Key::Lines));
    assert_eq!(2usize, s.total_pages());
    assert_eq!(page_rows, s.total_rows - s.rows as usize);

    // Removing only the line limit leaves byte enforcement in effect.
    s.set_max_lines(None);
    s.grow_rows(3 * page_rows).unwrap();
    assert_eq!(byte_limit, s.page_size);
    assert_eq!(3usize, s.total_pages());
    assert!(s.total_rows - s.rows as usize > page_rows);

    s.deinit();
}

// PORT NOTE (rustty-only guard, `P3-W3` T3 Step 4 / re-vet row `P3-RV1`): the
// sibling of `pagelist_replacements_preserve_compression_continuation_and_mark_activity`
// for the ELEVENTH `mark_activity` site, the one inside `enforce_limit`. Trimming
// scrollback mutates the node set under an in-flight incremental compression
// traversal, so the activity token must be bumped while the traversal
// continuation (`last_serial`/`next_serial`) and its flags are preserved.
//
// The second half is what pins the oracle's exact placement: `f4c68d65e` bumps
// activity on the FIRST ACTUAL REMOVAL (`if (removed == 0)`), not on entry — an
// `enforce` that finds the limit exceeded but immediately hits the
// active-boundary `break` must NOT invalidate the traversal.
#[test]
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound in the debug-assertions-ON G6 lane, where `Page::assert_integrity` runs on nearly every page mutation (measured per test; see the C-1 table there). It RUNS in the G6c lane (`check_leaks.sh --debug-assertions-off`). Do NOT widen this to a plain `miri` ignore: that would drop the test from BOTH lanes."
)]
fn pagelist_enforce_limit_marks_activity_and_preserves_continuation() {
    let cols: size::CellCountInt = 80;
    let page_rows: usize = PageList::initial_capacity(cols).rows as usize;

    let mut s = PageList::init(
        Global,
        Options {
            max_size: None,
            max_lines: None,
            ..Options::new(cols, 1)
        },
    )
    .unwrap();
    s.grow_rows(4 * page_rows).unwrap();

    let state = crate::pagelist::compress::IncrementalCompressionState {
        flags: crate::pagelist::compress::IncrementalCompressionFlags {
            did_compress: false,
            verifying: true,
        },
        activity_serial: 42,
        last_serial: Some(7),
        next_serial: 8,
    };

    // (a) An enforcement that actually removes pages bumps activity_serial and
    //     preserves the continuation.
    s.page_compression = state;
    s.set_max_lines(Some(page_rows));
    assert_eq!(
        crate::pagelist::compress::IncrementalCompressionState {
            activity_serial: 43,
            ..state
        },
        s.page_compression
    );

    // (b) A second enforcement that removes NOTHING (already at the limit)
    //     leaves the whole compression state alone.
    s.page_compression = state;
    s.set_max_lines(Some(page_rows));
    assert_eq!(state, s.page_compression);

    s.deinit();

    // (c) An enforcement that IS exceeded but whose only over-limit page is the
    //     active-boundary page breaks out having removed nothing, and must not
    //     bump activity. This is the case that distinguishes the oracle's
    //     `if (removed == 0)` placement from a bump on entry: the single page
    //     below is larger than the zero-byte effective limit AND contains the
    //     active area, so `exceeded(.bytes)` is true while the prune loop
    //     removes nothing.
    let mut b = PageList::init(
        Global,
        Options {
            max_size: None,
            ..Options::new(cols, 1)
        },
    )
    .unwrap();
    while b.page_size <= b.limits.bytes.min {
        let _ = b
            .increase_capacity(b.pages.first, Some(IncreaseCapacity::GraphemeBytes))
            .unwrap();
    }
    let _ = b.grow().unwrap();
    assert_eq!(1usize, b.total_pages());
    assert!(core::ptr::eq(
        b.pages.first,
        b.get_top_left(point::Tag::Active).node
    ));

    b.page_compression = state;
    b.set_max_bytes(Some(0));
    assert!(b.limits.exceeded(&b, Key::Bytes));
    assert_eq!(1usize, b.total_pages());
    assert_eq!(state, b.page_compression);

    b.deinit();
}

// PORT NOTE (rustty-only guard, `P3-W3` T1 Step 4): `5b2d3b7df` adds
// `IntegrityError.MaxLinesExceeded` but no upstream test reaches it — every
// oracle path that can exceed the line limit calls `enforceMaxLines` first, so
// the variant would be decoration. This test manufactures the exact forbidden
// state (a complete historical page past the limit while the oldest page is NOT
// the active-boundary page) behind `pause_integrity_checks` and asserts the
// check fires. Requires `debug_assertions`: `verify_integrity` is a no-op
// without them.
#[test]
#[cfg_attr(
    not(debug_assertions),
    ignore = "DEBUG-ONLY BY DESIGN, and this reason was MEASURED (fix round 1 of parity S166-F2): this test drives the slow-runtime-safety mechanism — `PageList::assert_integrity` / `pin_is_valid` — which the port compiles out when `debug_assertions` is off, faithful to the reference's `build_options.slow_runtime_safety`, under which the oracle's counterpart only exists. It is NOT ignored because of the tripwire gate (that claim was false and is gone): un-ignoring it turns the release lane RED, which is why it stays."
)]
fn pagelist_integrity_detects_max_lines_exceeded() {
    let cols: size::CellCountInt = 80;
    let page_rows: usize = PageList::initial_capacity(cols).rows as usize;

    let mut s = PageList::init(
        Global,
        Options {
            max_lines: Some(page_rows),
            ..Options::new(cols, 1)
        },
    )
    .unwrap();

    // Right at the limit: one complete historical page plus the active row.
    //
    // The first page's own integrity checks are paused while it fills, the
    // technique the reference's grow-loop tests use (`pointFromPin active from
    // prior page` above): `grow_rows` otherwise runs a page-level
    // `assert_integrity` per row, O(n^2) over the page, which kept this test
    // out of the Miri lane. The page is still filled row by row by
    // `grow_rows`; its checks run in full once it is complete.
    let first = s.pages.first;
    unsafe { (*crate::pagelist::Node::page(first)).pause_integrity_checks(true) };
    s.grow_rows(page_rows).unwrap();
    unsafe { (*crate::pagelist::Node::page(first)).pause_integrity_checks(false) };
    assert_eq!(Ok(()), unsafe {
        (*crate::pagelist::Node::page(first)).verify_integrity()
    });
    assert_eq!(Ok(()), s.verify_integrity());

    // Append a second complete page behind the caller's back with integrity
    // checks paused, so no enforcement path runs. The oldest page is now
    // wholly historical AND the history exceeds the limit.
    s.pause_integrity_checks(true);
    let node = s
        .create_page(crate::pagelist::CreatePage::new(
            PageList::initial_capacity(cols),
        ))
        .unwrap();
    let node_rows = unsafe { crate::pagelist::Node::capacity(node).rows };
    unsafe { (*crate::pagelist::Node::page(node)).size.rows = node_rows };
    unsafe { s.pages.append(node) };
    s.total_rows += node_rows as usize;
    s.pause_integrity_checks(false);

    // S122-F1: the reference logs `log.warn("PageList integrity violation:
    // max lines exceeded history={} max={}", .{...})` (PageList.zig:871-874)
    // on this exact violation.
    let cap = crate::log_capture::start();
    assert_eq!(Err(IntegrityError::MaxLinesExceeded), s.verify_integrity());
    let messages = cap.messages();
    assert!(
        messages
            .iter()
            .any(|(level, target, msg)| *level == log::Level::Warn
                && target == "page_list"
                && msg.starts_with("PageList integrity violation: max lines exceeded history=")),
        "expected the max-lines-exceeded integrity diagnostic, got: {messages:?}"
    );
    drop(cap);

    // Raising the limit back to unlimited restores a valid list so `deinit`'s
    // own integrity assertion holds.
    s.limits.lines.explicit = usize::MAX;
    assert_eq!(Ok(()), s.verify_integrity());

    s.deinit();
}

// S122-F1: `verify_integrity`'s total_rows-mismatch check
// (PageList.zig:831-834). Corrupt the cached `total_rows` field behind
// integrity checks paused (the same technique
// `pagelist_integrity_detects_max_lines_exceeded` uses), then confirm both
// the error variant and the restored `log.warn` diagnostic.
#[test]
#[cfg_attr(
    not(debug_assertions),
    ignore = "DEBUG-ONLY BY DESIGN, and this reason was MEASURED (fix round 1 of parity S166-F2): this test drives the slow-runtime-safety mechanism — `PageList::assert_integrity` / `pin_is_valid` — which the port compiles out when `debug_assertions` is off, faithful to the reference's `build_options.slow_runtime_safety`, under which the oracle's counterpart only exists. It is NOT ignored because of the tripwire gate (that claim was false and is gone): un-ignoring it turns the release lane RED, which is why it stays."
)]
fn pagelist_integrity_detects_total_rows_mismatch() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
    s.grow_rows(10).unwrap();
    assert_eq!(Ok(()), s.verify_integrity());

    s.pause_integrity_checks(true);
    s.total_rows += 5;
    s.pause_integrity_checks(false);

    let cap = crate::log_capture::start();
    assert_eq!(Err(IntegrityError::TotalRowsMismatch), s.verify_integrity());
    let messages = cap.messages();
    assert!(
        messages
            .iter()
            .any(|(level, target, msg)| *level == log::Level::Warn
                && target == "page_list"
                && msg.starts_with("PageList integrity violation: total_rows mismatch cached=")),
        "expected the total_rows-mismatch integrity diagnostic, got: {messages:?}"
    );
    drop(cap);

    s.pause_integrity_checks(true);
    s.total_rows -= 5;
    s.pause_integrity_checks(false);
    assert_eq!(Ok(()), s.verify_integrity());

    s.deinit();
}

// S122-F1: `verify_integrity`'s page-serial-predates-epoch check
// (PageList.zig:820-823). Bumping `page_serial_epoch` past every existing
// node's `serial` (paused, as above) reproduces the violation without
// corrupting anything the reference doesn't itself guard against.
#[test]
#[cfg_attr(
    not(debug_assertions),
    ignore = "DEBUG-ONLY BY DESIGN, and this reason was MEASURED (fix round 1 of parity S166-F2): this test drives the slow-runtime-safety mechanism — `PageList::assert_integrity` / `pin_is_valid` — which the port compiles out when `debug_assertions` is off, faithful to the reference's `build_options.slow_runtime_safety`, under which the oracle's counterpart only exists. It is NOT ignored because of the tripwire gate (that claim was false and is gone): un-ignoring it turns the release lane RED, which is why it stays."
)]
fn pagelist_integrity_detects_page_serial_predates_epoch() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
    s.grow_rows(10).unwrap();
    assert_eq!(Ok(()), s.verify_integrity());

    s.pause_integrity_checks(true);
    s.page_serial_epoch += 1_000_000;
    s.pause_integrity_checks(false);

    let cap = crate::log_capture::start();
    assert_eq!(Err(IntegrityError::PageSerialInvalid), s.verify_integrity());
    let messages = cap.messages();
    assert!(
        messages
            .iter()
            .any(|(level, target, msg)| *level == log::Level::Warn
                && target == "page_list"
                && msg.starts_with(
                    "PageList integrity violation: page serial predates epoch serial="
                )),
        "expected the page-serial-predates-epoch integrity diagnostic, got: {messages:?}"
    );
    drop(cap);

    s.pause_integrity_checks(true);
    s.page_serial_epoch -= 1_000_000;
    s.pause_integrity_checks(false);
    assert_eq!(Ok(()), s.verify_integrity());

    s.deinit();
}

// S122-F1: the three `viewport == .pin` checks (PageList.zig:845-848 "not in
// list", :871-874 "offset mismatch", :880-883 "rows too small").
// `s.scroll(Scroll::Row(_))` is the ordinary, non-corrupting way to put the
// list into `Viewport::Pin` (used throughout this file, e.g.
// `pagelist_scroll_to_row_in_scrollback`); each test then corrupts exactly
// the one cached field its check reads.

// PageList.zig:845-848 — "viewport pin not in list". Point `viewport_pin` at
// a node this list doesn't own so the scan in `verify_integrity` never finds
// it.
#[test]
#[cfg_attr(
    not(debug_assertions),
    ignore = "DEBUG-ONLY BY DESIGN, and this reason was MEASURED (fix round 1 of parity S166-F2): this test drives the slow-runtime-safety mechanism — `PageList::assert_integrity` / `pin_is_valid` — which the port compiles out when `debug_assertions` is off, faithful to the reference's `build_options.slow_runtime_safety`, under which the oracle's counterpart only exists. It is NOT ignored because of the tripwire gate (that claim was false and is gone): un-ignoring it turns the release lane RED, which is why it stays."
)]
fn pagelist_integrity_detects_viewport_pin_not_in_list() {
    // Growing a page one row at a time (`grow_rows`) runs a full page-level
    // `assert_integrity` PER ROW under `debug_assertions` — O(current page
    // size) each time, so forcing a second page this way is O(n^2) in the
    // row count, which is what made earlier drafts of this test time out
    // under Miri regardless of how small a page's byte capacity was made.
    // Sidestepping the loop entirely: create the second (tiny) page
    // directly with ONE `create_page` call and splice it in — the exact
    // technique `pagelist_integrity_detects_max_lines_exceeded` (above)
    // already uses for the same reason.
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
    assert_eq!(Ok(()), s.verify_integrity());

    s.pause_integrity_checks(true);
    let node = s
        .create_page(crate::pagelist::CreatePage::new(Capacity {
            styles: 8,
            ..Capacity::new(10, 10)
        }))
        .unwrap();
    unsafe { (*crate::pagelist::Node::page(node)).size.rows = 5 };
    unsafe { s.pages.append(node) };
    s.total_rows += 5;
    s.pause_integrity_checks(false);
    assert_eq!(2usize, s.total_pages());

    // `total_rows` (29) minus the active height (24) leaves rows 1..=4
    // scrollable before `scroll`'s "at/below the active area" clamp — all
    // of which live on the FIRST (original, `s.pages.first`) page, so the
    // pin lands there with the tiny page appended above as its later
    // sibling.
    s.scroll(Scroll::Row(2));
    assert!(s.viewport == Viewport::Pin);
    assert!(core::ptr::eq(
        unsafe { (*s.viewport_pin).node },
        s.pages.first
    ));
    assert_eq!(Ok(()), s.verify_integrity());

    // `pin_is_valid` (checked, for every tracked pin including this one,
    // BEFORE the viewport-pin block) scans forward from `pages.first` via
    // `.next` and only requires the node to be reachable that way — so we
    // can reproduce "not found by the `.prev` backward scan from
    // `pages.last`" (PageList.zig:845-848) surgically: splice the pin's node
    // out of the BACKWARD chain only (repoint its forward-neighbor's `.prev`
    // around it), leaving the forward `.next` chain — and thus
    // `pin_is_valid` — completely untouched. This reproduces the reference's
    // own distinction (a node can satisfy `pinIsValid` while the
    // list-scan-from-the-tail in `assertIntegrity` still doesn't visit it)
    // without corrupting anything the tracked-pins check itself guards.
    let vp_node = unsafe { (*s.viewport_pin).node };
    let vp_next = unsafe { (*vp_node).next };
    assert!(
        !vp_next.is_null(),
        "the pin's node must have a later sibling"
    );

    s.pause_integrity_checks(true);
    let vp_prev = unsafe { (*vp_node).prev };
    unsafe { (*vp_next).prev = vp_prev };
    s.pause_integrity_checks(false);

    let cap = crate::log_capture::start();
    assert_eq!(
        Err(IntegrityError::ViewportPinOffsetMismatch),
        s.verify_integrity()
    );
    let messages = cap.messages();
    assert!(
        messages
            .iter()
            .any(|(level, target, msg)| *level == log::Level::Warn
                && target == "page_list"
                && *msg == "PageList integrity violation: viewport pin not in list"),
        "expected the viewport-pin-not-in-list integrity diagnostic, got: {messages:?}"
    );
    drop(cap);

    // Repair the backward chain before any further mutation (`deinit`'s own
    // integrity assertion, and the pool teardown it does, both walk the
    // list).
    s.pause_integrity_checks(true);
    unsafe { (*vp_next).prev = vp_node };
    s.pause_integrity_checks(false);
    assert_eq!(Ok(()), s.verify_integrity());

    s.deinit();
}

// PageList.zig:871-874 — "viewport pin offset mismatch". The pin IS in the
// list, but the cached `viewport_pin_row_offset` no longer matches the
// scan's recomputed offset.
#[test]
#[cfg_attr(
    not(debug_assertions),
    ignore = "DEBUG-ONLY BY DESIGN, and this reason was MEASURED (fix round 1 of parity S166-F2): this test drives the slow-runtime-safety mechanism — `PageList::assert_integrity` / `pin_is_valid` — which the port compiles out when `debug_assertions` is off, faithful to the reference's `build_options.slow_runtime_safety`, under which the oracle's counterpart only exists. It is NOT ignored because of the tripwire gate (that claim was false and is gone): un-ignoring it turns the release lane RED, which is why it stays."
)]
fn pagelist_integrity_detects_viewport_pin_offset_mismatch() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
    s.grow_rows(20).unwrap();
    s.scroll(Scroll::Row(5));
    assert!(s.viewport == Viewport::Pin);
    assert_eq!(Ok(()), s.verify_integrity());
    assert!(s.viewport_pin_row_offset.is_some());

    s.pause_integrity_checks(true);
    let saved = s.viewport_pin_row_offset;
    s.viewport_pin_row_offset = Some(saved.unwrap() + 7);
    s.pause_integrity_checks(false);

    let cap = crate::log_capture::start();
    assert_eq!(
        Err(IntegrityError::ViewportPinOffsetMismatch),
        s.verify_integrity()
    );
    let messages = cap.messages();
    assert!(
        messages
            .iter()
            .any(|(level, target, msg)| *level == log::Level::Warn
                && target == "page_list"
                && msg.starts_with(
                    "PageList integrity violation: viewport pin offset mismatch cached="
                )),
        "expected the viewport-pin-offset-mismatch integrity diagnostic, got: {messages:?}"
    );
    drop(cap);

    s.pause_integrity_checks(true);
    s.viewport_pin_row_offset = saved;
    s.pause_integrity_checks(false);
    assert_eq!(Ok(()), s.verify_integrity());

    s.deinit();
}

// PageList.zig:880-883 — "viewport pin rows too small". The recomputed
// offset is correct, but `self.rows` (the active-area height) has been
// inflated past what's left of the scrollback below the pin.
#[test]
#[cfg_attr(
    not(debug_assertions),
    ignore = "DEBUG-ONLY BY DESIGN, and this reason was MEASURED (fix round 1 of parity S166-F2): this test drives the slow-runtime-safety mechanism — `PageList::assert_integrity` / `pin_is_valid` — which the port compiles out when `debug_assertions` is off, faithful to the reference's `build_options.slow_runtime_safety`, under which the oracle's counterpart only exists. It is NOT ignored because of the tripwire gate (that claim was false and is gone): un-ignoring it turns the release lane RED, which is why it stays."
)]
fn pagelist_integrity_detects_viewport_pin_insufficient_rows() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
    s.grow_rows(20).unwrap();
    s.scroll(Scroll::Row(5));
    assert!(s.viewport == Viewport::Pin);
    assert_eq!(Ok(()), s.verify_integrity());

    s.pause_integrity_checks(true);
    let saved = s.rows;
    s.rows = s.total_rows as size::CellCountInt + 100;
    s.pause_integrity_checks(false);

    let cap = crate::log_capture::start();
    assert_eq!(
        Err(IntegrityError::ViewportPinInsufficientRows),
        s.verify_integrity()
    );
    let messages = cap.messages();
    assert!(
        messages
            .iter()
            .any(|(level, target, msg)| *level == log::Level::Warn
                && target == "page_list"
                && msg.starts_with(
                    "PageList integrity violation: viewport pin rows too small rows="
                )),
        "expected the viewport-pin-insufficient-rows integrity diagnostic, got: {messages:?}"
    );
    drop(cap);

    s.pause_integrity_checks(true);
    s.rows = saved;
    s.pause_integrity_checks(false);
    assert_eq!(Ok(()), s.verify_integrity());

    s.deinit();
}

// ref: test "PageList max lines uses one-page minimum"
#[test]
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound in the debug-assertions-ON G6 lane, where `Page::assert_integrity` runs on nearly every page mutation (measured per test; see the C-1 table there). It RUNS in the G6c lane (`check_leaks.sh --debug-assertions-off`). Do NOT widen this to a plain `miri` ignore: that would drop the test from BOTH lanes."
)]
fn pagelist_max_lines_uses_one_page_minimum() {
    let cols: size::CellCountInt = 80;
    let page_rows: usize = PageList::initial_capacity(cols).rows as usize;

    let mut s = PageList::init(
        Global,
        Options {
            max_lines: Some(page_rows / 2),
            ..Options::new(cols, 1)
        },
    )
    .unwrap();

    assert_eq!(page_rows, s.limits.max(Key::Lines));

    // The requested limit is below one page, so a complete page of history
    // remains valid.
    s.grow_rows(page_rows).unwrap();
    assert_eq!(page_rows, s.total_rows - s.rows as usize);
    assert_eq!(2usize, s.total_pages());

    let first = s.pages.first;
    let old_page_size = s.page_size;

    // One more row puts us over the effective limit. The now-complete
    // historical page is removed rather than partially trimmed.
    let _ = s.grow().unwrap();
    assert_eq!(1usize, s.total_rows - s.rows as usize);
    assert_eq!(1usize, s.total_pages());
    assert!(!core::ptr::eq(s.pages.first, first));
    // PORT NOTE: the reference `PagePool.item_size` is `std_size()` here.
    assert_eq!(old_page_size - std_size(), s.page_size);

    s.deinit();
}

// ref: test "PageList max lines does not round larger limits"
#[test]
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound in the debug-assertions-ON G6 lane, where `Page::assert_integrity` runs on nearly every page mutation (measured per test; see the C-1 table there). It RUNS in the G6c lane (`check_leaks.sh --debug-assertions-off`). Do NOT widen this to a plain `miri` ignore: that would drop the test from BOTH lanes."
)]
fn pagelist_max_lines_does_not_round_larger_limits() {
    let cols: size::CellCountInt = 80;
    let page_rows: usize = PageList::initial_capacity(cols).rows as usize;
    let max_lines = page_rows + page_rows / 2;

    let mut s = PageList::init(
        Global,
        Options {
            max_lines: Some(max_lines),
            ..Options::new(cols, 1)
        },
    )
    .unwrap();

    assert_eq!(max_lines, s.limits.max(Key::Lines));
    s.grow_rows(max_lines).unwrap();
    assert_eq!(max_lines, s.total_rows - s.rows as usize);

    let first = s.pages.first;
    let retained = unsafe { (*first).next };
    let removed_pin = s
        .track_pin(Pin {
            node: first,
            y: 0,
            x: 0,
            garbage: false,
        })
        .unwrap();
    let retained_pin = s
        .track_pin(Pin {
            node: retained,
            y: 0,
            x: 0,
            garbage: false,
        })
        .unwrap();

    s.scroll(Scroll::Pin(unsafe { (*retained_pin).pin_copy() }));
    assert_eq!(page_rows, s.scrollbar().offset);

    let old_page_size = s.page_size;
    let _ = s.grow().unwrap();

    // Whole-page pruning undershoots the requested limit without rounding it.
    assert_eq!(max_lines + 1 - page_rows, s.total_rows - s.rows as usize);
    assert!(core::ptr::eq(retained, s.pages.first));
    assert!(core::ptr::eq(retained, unsafe { (*removed_pin).node }));
    assert!(unsafe { (*removed_pin).garbage });
    assert!(core::ptr::eq(retained, unsafe { (*retained_pin).node }));
    assert!(!unsafe { (*retained_pin).garbage });
    assert_eq!(0usize, s.scrollbar().offset);
    assert_eq!(old_page_size - std_size(), s.page_size);

    s.untrack_pin(retained_pin);
    s.untrack_pin(removed_pin);
    s.deinit();
}

// ref: test "PageList max lines and max size enforce the smaller limit"
#[test]
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound in the debug-assertions-ON G6 lane, where `Page::assert_integrity` runs on nearly every page mutation (measured per test; see the C-1 table there). It RUNS in the G6c lane (`check_leaks.sh --debug-assertions-off`). Do NOT widen this to a plain `miri` ignore: that would drop the test from BOTH lanes."
)]
fn pagelist_max_lines_and_max_size_enforce_the_smaller_limit() {
    let cols: size::CellCountInt = 80;
    let page_rows: usize = PageList::initial_capacity(cols).rows as usize;

    // A line limit of one page keeps the logical allocation below a much
    // larger byte limit.
    {
        let mut s = PageList::init(
            Global,
            Options {
                max_size: Some(8 * std_size()),
                max_lines: Some(page_rows),
                ..Options::new(cols, 1)
            },
        )
        .unwrap();

        s.grow_rows(4 * page_rows).unwrap();
        assert!(s.total_rows - s.rows as usize <= s.limits.max(Key::Lines));
        assert!(s.total_pages() <= 2);
        assert!(s.page_size < s.limits.max(Key::Bytes));

        s.deinit();
    }

    // A two-page byte limit prunes before the larger line limit is reached.
    {
        let mut s = PageList::init(
            Global,
            Options {
                max_size: Some(std_size()),
                max_lines: Some(4 * page_rows),
                ..Options::new(cols, 1)
            },
        )
        .unwrap();

        s.grow_rows(2 * page_rows).unwrap();
        assert!(s.total_rows - (s.rows as usize) < s.limits.max(Key::Lines));
        assert_eq!(s.limits.max(Key::Bytes), s.page_size);

        s.deinit();
    }
}

// ref: test "PageList max lines applies to resize and clone"
#[test]
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound in the debug-assertions-ON G6 lane, where `Page::assert_integrity` runs on nearly every page mutation (measured per test; see the C-1 table there). It RUNS in the G6c lane (`check_leaks.sh --debug-assertions-off`). Do NOT widen this to a plain `miri` ignore: that would drop the test from BOTH lanes."
)]
fn pagelist_max_lines_applies_to_resize_and_clone() {
    let cols: size::CellCountInt = 80;
    let page_rows: usize = PageList::initial_capacity(cols).rows as usize;

    let mut s = PageList::init(
        Global,
        Options {
            max_lines: Some(page_rows),
            ..Options::new(cols, 2)
        },
    )
    .unwrap();

    s.grow_rows(page_rows).unwrap();
    assert_eq!(page_rows, s.total_rows - s.rows as usize);

    // Prevent row shrinking from trimming the trailing active row instead of
    // turning it into history.
    let cell = s
        .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 1 }))
        .unwrap();
    unsafe { *cell.cell = cell_codepoint('A' as u32) };

    s.resize(Resize {
        rows: Some(1),
        reflow: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(1usize, s.total_rows - s.rows as usize);

    let new_cols: size::CellCountInt = cols + 1;
    s.resize(Resize {
        cols: Some(new_cols),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(Limits::min_max_lines(new_cols), s.limits.lines.min);

    // Exercise the same active-row shrink through the reflow path. Reflow
    // completes before the newly historical complete page is pruned.
    {
        let mut reflowed = PageList::init(
            Global,
            Options {
                max_lines: Some(page_rows),
                ..Options::new(cols, 2)
            },
        )
        .unwrap();

        reflowed.grow_rows(page_rows).unwrap();
        let active_cell = reflowed
            .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 1 }))
            .unwrap();
        unsafe { *active_cell.cell = cell_codepoint('A' as u32) };

        reflowed
            .resize(Resize {
                cols: Some(new_cols),
                rows: Some(1),
                reflow: true,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(Limits::min_max_lines(new_cols), reflowed.limits.lines.min);
        assert!(
            reflowed.total_rows - reflowed.rows as usize <= reflowed.limits.max(Key::Lines)
                || core::ptr::eq(
                    reflowed.pages.first,
                    reflowed.get_top_left(point::Tag::Active).node
                )
        );

        reflowed.deinit();
    }

    let mut opts = crate::pagelist::access::Clone {
        top: point::Point::Screen(point::Coordinate { x: 0, y: 0 }),
        bot: None,
        tracked_pins: None,
    };
    let mut cloned = s.clone(Global, &mut opts).unwrap();

    // try testing.expectEqual(s.limits, cloned.limits);
    //
    // FIX (S127-F1): three line-only asserts stood here, so `bytes.explicit` and
    // `bytes.min` were never compared and a `clone` that dropped or recomputed
    // the BYTE limit passed. `Limits` derives `PartialEq`, so the reference's
    // whole-struct equality is directly expressible.
    assert_eq!(s.limits, cloned.limits);

    cloned.grow_rows(2 * cloned.limits.max(Key::Lines)).unwrap();
    assert!(
        cloned.total_rows - cloned.rows as usize <= cloned.limits.max(Key::Lines)
            || core::ptr::eq(
                cloned.pages.first,
                cloned.get_top_left(point::Tag::Active).node
            )
    );

    cloned.deinit();
    s.deinit();
}

// ref: test "PageList grow prune scrollback"
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn pagelist_grow_prune_scrollback() {
    // Use std_size to limit scrollback so pruning is triggered.
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(std_size()),
            ..Options::new(80, 24)
        },
    )
    .unwrap();

    // Grow to capacity
    let page1_node = s.pages.last;
    let (page1_cap_rows, page1_size_rows) = {
        let page1 = unsafe { &(*crate::pagelist::Node::page(page1_node)) };
        (page1.capacity.rows, page1.size.rows)
    };
    for _ in 0..(page1_cap_rows - page1_size_rows) {
        assert!(s.grow().unwrap().is_none());
    }

    // Grow and allocate one more page. Then fill that page up.
    let page2_node = s.grow().unwrap().unwrap();
    let (page2_cap_rows, page2_size_rows) = {
        let page2 = unsafe { &(*crate::pagelist::Node::page(page2_node)) };
        (page2.capacity.rows, page2.size.rows)
    };
    for _ in 0..(page2_cap_rows - page2_size_rows) {
        assert!(s.grow().unwrap().is_none());
    }

    // Get our page size
    let old_page_size = s.page_size;

    // Create a tracked pin in the first page
    let p = s
        .track_pin(
            s.pin(point::Point::Screen(point::Coordinate { x: 0, y: 0 }))
                .unwrap(),
        )
        .unwrap();
    assert!(core::ptr::eq(unsafe { (*p).node }, s.pages.first));

    // Scroll back to create a pinned viewport (not active)
    let pin_y = page1_cap_rows as usize / 2;
    s.scroll(Scroll::Pin(
        s.pin(point::Point::Screen(point::Coordinate {
            x: 0,
            y: pin_y as u32,
        }))
        .unwrap(),
    ));
    assert!(s.viewport == crate::pagelist::Viewport::Pin);

    // Get the scrollbar state to populate the cache
    let scrollbar_before = s.scrollbar();
    assert_eq!(pin_y, scrollbar_before.offset);

    // Next should create a new page, but it should reuse our first
    // page since we're at max size.
    let new = s.grow().unwrap().unwrap();
    assert!(core::ptr::eq(s.pages.last, new));
    assert_eq!(s.page_size, old_page_size);

    // Our first should now be page2 and our last should be page1
    assert!(core::ptr::eq(page2_node, s.pages.first));
    assert!(core::ptr::eq(page1_node, s.pages.last));

    // Our tracked pin should point to the top-left of the first page
    assert!(core::ptr::eq(unsafe { (*p).node }, s.pages.first));
    assert!(unsafe { (*p).x } == 0);
    assert!(unsafe { (*p).y } == 0);
    assert!(unsafe { (*p).garbage });

    // Verify the viewport offset cache was invalidated. After pruning,
    // the offset should have changed because we removed rows from
    // the beginning.
    {
        let scrollbar_after = s.scrollbar();
        let rows_pruned = page1_cap_rows as usize;
        // PORT NOTE: the reference computes this with an explicit
        // `if pin_y >= rows_pruned ... else 0` branch; reproduced verbatim
        // (clippy would prefer `saturating_sub`, allowed here for fidelity).
        #[allow(clippy::implicit_saturating_sub)]
        let expected_offset = if pin_y >= rows_pruned {
            pin_y - rows_pruned
        } else {
            0
        };
        assert_eq!(expected_offset, scrollbar_after.offset);
    }

    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList grow prune scrollback with viewport pin not in pruned page"
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn pagelist_grow_prune_scrollback_with_viewport_pin_not_in_pruned_page() {
    // Use std_size to limit scrollback so pruning is triggered.
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(std_size()),
            ..Options::new(80, 24)
        },
    )
    .unwrap();

    // Grow to capacity of first page
    let page1_node = s.pages.last;
    let (page1_cap_rows, page1_size_rows) = {
        let page1 = unsafe { &(*crate::pagelist::Node::page(page1_node)) };
        (page1.capacity.rows, page1.size.rows)
    };
    for _ in 0..(page1_cap_rows - page1_size_rows) {
        assert!(s.grow().unwrap().is_none());
    }

    // Grow and allocate second page, then fill it up
    let page2_node = s.grow().unwrap().unwrap();
    let (page2_cap_rows, page2_size_rows) = {
        let page2 = unsafe { &(*crate::pagelist::Node::page(page2_node)) };
        (page2.capacity.rows, page2.size.rows)
    };
    for _ in 0..(page2_cap_rows - page2_size_rows) {
        assert!(s.grow().unwrap().is_none());
    }

    // Get our page size
    let old_page_size = s.page_size;

    // Scroll back to create a pinned viewport in page2 (NOT page1)
    // This is the key difference from the previous test - the viewport
    // pin is NOT in the page that will be pruned.
    let pin_y = page1_cap_rows as usize + 5;
    s.scroll(Scroll::Pin(
        s.pin(point::Point::Screen(point::Coordinate {
            x: 0,
            y: pin_y as u32,
        }))
        .unwrap(),
    ));
    assert!(s.viewport == crate::pagelist::Viewport::Pin);
    assert!(core::ptr::eq(unsafe { (*s.viewport_pin).node }, page2_node));

    // Get the scrollbar state to populate the cache
    let scrollbar_before = s.scrollbar();
    assert_eq!(pin_y, scrollbar_before.offset);

    // Next grow will trigger pruning of the first page.
    // The viewport_pin.node is page2, not page1, so it won't be moved
    // by the pin update loop, but the cached offset still needs to be
    // invalidated because rows were removed from the beginning.
    let new = s.grow().unwrap().unwrap();
    assert!(core::ptr::eq(s.pages.last, new));
    assert_eq!(s.page_size, old_page_size);

    // Our first should now be page2 (page1 was pruned)
    assert!(core::ptr::eq(page2_node, s.pages.first));

    // The viewport pin should still be on page2, unchanged
    assert!(core::ptr::eq(unsafe { (*s.viewport_pin).node }, page2_node));

    // Verify the viewport offset cache was invalidated/updated.
    // After pruning, the offset should have decreased by the number
    // of rows that were pruned.
    let scrollbar_after = s.scrollbar();
    let rows_pruned = page1_cap_rows as usize;
    let expected_offset = pin_y - rows_pruned;
    assert_eq!(expected_offset, scrollbar_after.offset);

    s.deinit();
}

// ref: test "PageList eraseRows invalidates viewport offset cache"
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn pagelist_eraseRows_invalidates_viewport_offset_cache() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

    // Grow so we take up several pages worth of history
    let page_cap_rows = unsafe { crate::pagelist::Node::capacity(s.pages.last).rows };
    {
        let mut cur_page = s.pages.last;
        for _ in 0..(page_cap_rows as usize * 3) {
            if let Some(new_page) = s.grow().unwrap() {
                cur_page = new_page;
            }
        }
        let _ = cur_page;
    }

    // Scroll back to create a pinned viewport somewhere in the middle
    // of the scrollback
    let pin_y = page_cap_rows;
    s.scroll(Scroll::Pin(
        s.pin(point::Point::Screen(point::Coordinate {
            x: 0,
            y: pin_y as u32,
        }))
        .unwrap(),
    ));
    assert!(s.viewport == crate::pagelist::Viewport::Pin);
    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: pin_y as usize,
            len: s.rows as usize,
        },
        s.scrollbar(),
    );

    // Erase some history rows BEFORE the viewport pin.
    // This removes rows from before our pin, which changes its absolute
    // offset from the top, but the cache is not invalidated.
    let rows_to_erase = page_cap_rows / 2;
    s.erase_history(Some(point::Point::History(point::Coordinate {
        x: 0,
        y: (rows_to_erase - 1) as u32,
    })));

    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: (pin_y - rows_to_erase) as usize,
            len: s.rows as usize,
        },
        s.scrollbar(),
    );

    s.deinit();
}

// ref: test "PageList eraseRow invalidates viewport offset cache"
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn pagelist_eraseRow_invalidates_viewport_offset_cache() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

    // Grow so we take up several pages worth of history
    let page_cap_rows = unsafe { crate::pagelist::Node::capacity(s.pages.last).rows };
    {
        let mut cur_page = s.pages.last;
        for _ in 0..(page_cap_rows as usize * 3) {
            if let Some(new_page) = s.grow().unwrap() {
                cur_page = new_page;
            }
        }
        let _ = cur_page;
    }

    // Scroll back to create a pinned viewport somewhere in the middle
    // of the scrollback
    let pin_y = page_cap_rows;
    s.scroll(Scroll::Pin(
        s.pin(point::Point::Screen(point::Coordinate {
            x: 0,
            y: pin_y as u32,
        }))
        .unwrap(),
    ));
    assert!(s.viewport == crate::pagelist::Viewport::Pin);
    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: pin_y as usize,
            len: s.rows as usize,
        },
        s.scrollbar(),
    );

    // Erase a single row from the history BEFORE the viewport pin.
    // This removes one row from before our pin, which changes its absolute
    // offset from the top by 1, but the cache is not invalidated.
    s.erase_row(point::Point::History(point::Coordinate { x: 0, y: 0 }))
        .unwrap();

    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: (pin_y - 1) as usize,
            len: s.rows as usize,
        },
        s.scrollbar(),
    );

    s.deinit();
}

// ref: test "PageList eraseRowBounded invalidates viewport offset cache"
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn pagelist_eraseRowBounded_invalidates_viewport_offset_cache() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

    // Grow so we take up several pages worth of history
    let page_cap_rows = unsafe { crate::pagelist::Node::capacity(s.pages.last).rows };
    {
        let mut cur_page = s.pages.last;
        for _ in 0..(page_cap_rows as usize * 3) {
            if let Some(new_page) = s.grow().unwrap() {
                cur_page = new_page;
            }
        }
        let _ = cur_page;
    }

    // Scroll back to create a pinned viewport somewhere in the middle
    // of the scrollback
    let pin_y: u16 = 4;
    s.scroll(Scroll::Pin(
        s.pin(point::Point::Screen(point::Coordinate {
            x: 0,
            y: pin_y as u32,
        }))
        .unwrap(),
    ));
    assert!(s.viewport == crate::pagelist::Viewport::Pin);
    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: pin_y as usize,
            len: s.rows as usize,
        },
        s.scrollbar(),
    );

    // Erase a row from the history BEFORE the viewport pin with a bounded
    // shift. This removes one row from before our pin, which changes its
    // absolute offset from the top by 1, but the cache is not invalidated.
    s.erase_row_bounded(point::Point::History(point::Coordinate { x: 0, y: 0 }), 10)
        .unwrap();

    // Verify the scrollbar reflects the change (offset decreased by 1)
    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: (pin_y - 1) as usize,
            len: s.rows as usize,
        },
        s.scrollbar(),
    );

    s.deinit();
}

// ref: test "PageList eraseRowBounded multi-page invalidates viewport offset cache"
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn pagelist_eraseRowBounded_multi_page_invalidates_viewport_offset_cache() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

    // Grow so we take up several pages worth of history
    let page_cap_rows = unsafe { crate::pagelist::Node::capacity(s.pages.last).rows };
    {
        let mut cur_page = s.pages.last;
        for _ in 0..(page_cap_rows as usize * 3) {
            if let Some(new_page) = s.grow().unwrap() {
                cur_page = new_page;
            }
        }
        let _ = cur_page;
    }

    // Scroll back to create a pinned viewport somewhere in the middle
    // of the scrollback, after the first page
    let pin_y = page_cap_rows + 1;
    s.scroll(Scroll::Pin(
        s.pin(point::Point::Screen(point::Coordinate {
            x: 0,
            y: pin_y as u32,
        }))
        .unwrap(),
    ));
    assert!(s.viewport == crate::pagelist::Viewport::Pin);
    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: pin_y as usize,
            len: s.rows as usize,
        },
        s.scrollbar(),
    );

    // Erase a row from the beginning of history with a limit that spans
    // across multiple pages. This ensures we hit the code path where
    // eraseRowBounded finds the limit boundary in a subsequent page.
    let limit = page_cap_rows as usize + 10;
    s.erase_row_bounded(
        point::Point::History(point::Coordinate { x: 0, y: 0 }),
        limit,
    )
    .unwrap();

    // Verify the scrollbar reflects the change (offset decreased by 1)
    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: (pin_y - 1) as usize,
            len: s.rows as usize,
        },
        s.scrollbar(),
    );

    s.deinit();
}

// ref: test "PageList eraseRowBounded full page shift invalidates viewport offset cache"
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn pagelist_eraseRowBounded_full_page_shift_invalidates_viewport_offset_cache() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

    // Grow so we take up several pages worth of history
    let page_cap_rows = unsafe { crate::pagelist::Node::capacity(s.pages.last).rows };
    {
        let mut cur_page = s.pages.last;
        for _ in 0..(page_cap_rows as usize * 4) {
            if let Some(new_page) = s.grow().unwrap() {
                cur_page = new_page;
            }
        }
        let _ = cur_page;
    }

    // Scroll back to create a pinned viewport somewhere well beyond
    // the first two pages
    let pin_y: u16 = 5;
    s.scroll(Scroll::Pin(
        s.pin(point::Point::Screen(point::Coordinate {
            x: 0,
            y: pin_y as u32,
        }))
        .unwrap(),
    ));
    assert!(s.viewport == crate::pagelist::Viewport::Pin);
    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: pin_y as usize,
            len: s.rows as usize,
        },
        s.scrollbar(),
    );

    // Erase a row from the beginning of history with a limit that is
    // larger than multiple full pages. This ensures we hit the code path
    // where eraseRowBounded continues looping through entire pages,
    // rotating all rows in each page until it reaches the limit or
    // runs out of pages.
    let limit = page_cap_rows as usize * 2 + 10;
    s.erase_row_bounded(
        point::Point::History(point::Coordinate { x: 0, y: 0 }),
        limit,
    )
    .unwrap();

    // Verify the scrollbar reflects the change (offset decreased by 1)
    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: (pin_y - 1) as usize,
            len: s.rows as usize,
        },
        s.scrollbar(),
    );

    s.deinit();
}

// ref: test "PageList eraseRowBounded exhausts pages invalidates viewport offset cache"
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn pagelist_eraseRowBounded_exhausts_pages_invalidates_viewport_offset_cache() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

    // Grow so we take up several pages worth of history
    let page_cap_rows = unsafe { crate::pagelist::Node::capacity(s.pages.last).rows };
    {
        let mut cur_page = s.pages.last;
        for _ in 0..(page_cap_rows as usize * 3) {
            if let Some(new_page) = s.grow().unwrap() {
                cur_page = new_page;
            }
        }
        let _ = cur_page;
    }

    // Our total rows should include history
    let total_rows_before = s.total_rows();
    assert!(total_rows_before > s.rows as usize);

    // Scroll back to create a pinned viewport somewhere in the history,
    // well after the erase will complete
    let pin_y = page_cap_rows as usize * 2 + 10;
    s.scroll(Scroll::Pin(
        s.pin(point::Point::Screen(point::Coordinate {
            x: 0,
            y: pin_y as u32,
        }))
        .unwrap(),
    ));
    assert!(s.viewport == crate::pagelist::Viewport::Pin);
    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: pin_y,
            len: s.rows as usize,
        },
        s.scrollbar(),
    );

    // Erase a row from the beginning of history with a limit that is
    // LARGER than all remaining pages combined. This ensures we exhaust
    // all pages in the while loop and reach the cleanup code after the loop.
    let limit = total_rows_before * 2;
    s.erase_row_bounded(
        point::Point::History(point::Coordinate { x: 0, y: 0 }),
        limit,
    )
    .unwrap();

    // Verify the scrollbar reflects the change (offset decreased by 1)
    assert_eq!(
        Scrollbar {
            total: s.total_rows,
            offset: pin_y - 1,
            len: s.rows as usize,
        },
        s.scrollbar(),
    );

    s.deinit();
}

// ref: test "PageList increaseCapacity after col shrink"
#[test]
fn pagelist_increaseCapacity_after_col_shrink() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 2)
        },
    )
    .unwrap();

    // Shrink columns
    s.resize(crate::pagelist::resize::Resize {
        cols: Some(5),
        rows: None,
        reflow: false,
        cursor: None,
        pull_scrollback: true,
    })
    .unwrap();
    assert_eq!(5, s.cols);

    {
        let page = unsafe { &(*crate::pagelist::Node::page(s.pages.first)) };
        assert_eq!(5, page.size.cols);
        assert!(page.capacity.cols >= 10);
    }

    // Increase capacity
    let _ = s
        .increase_capacity(s.pages.first, Some(IncreaseCapacity::Styles))
        .unwrap();

    {
        let page = unsafe { &(*crate::pagelist::Node::page(s.pages.first)) };
        // size.cols should still be 5, not reverted to capacity.cols
        assert_eq!(5, page.size.cols);
        assert_eq!(5, s.cols);
    }

    s.deinit();
}
