//! PORTS: ghostty src/terminal/PageList.zig — the `Builder` / `allocatePage` /
//! `PageAllocation` tests, the recycled/retired-row default-state tests, and
//! the UD80/UD131 `eraseRow` style-degrade tests.

use super::{cell_codepoint, compressed_encoded_for_test};
use crate::alloc::testing::{with_handle, Config as FailingConfig, FailingAllocator};
use crate::alloc::Global;
use crate::page::{Capacity, Cell, ContentTag, Page, SemanticPrompt};
use crate::pagelist::grow::IncreaseCapacity;
use crate::pagelist::resize::Resize;
use crate::pagelist::scroll::Scroll;
use crate::pagelist::{Options, PageList, Pin, Viewport};
use crate::style::{self as stylepkg, Style};
use crate::{hyperlink, point};
use rustty_render_types::term_size as size;

// =====================================================================
// PageList Builder / allocatePage / PageAllocation (phase-4 w2, bundle A-1)
//
// PORTS: the end state of oracle commits fc1bd06a1 ("PageList builder to
// build from raw pages") + 35db32078 ("PageList allocatePage") + 457c5a0a6
// ("PageList align Builder/PageAllocation APIs better"). Each `#[test]` is an
// assertion-for-assertion port of the reference `test {}` block of the same
// name against oracle@main PageList.zig.
// =====================================================================

// ref: test "PageList Builder transfers mixed-width pages"
#[test]
fn pagelist_builder_transfers_mixed_width_pages() {
    use crate::pagelist::Builder;

    // Build two populated pages whose widths differ from each other and from
    // the final active-area width. The first page also includes one row of
    // incidental history above the three-row active area.
    let mut result: PageList = {
        let mut builder = Builder::init(
            Global,
            Options {
                cols: 4,
                rows: 3,
                max_size: None,
                max_lines: None,
            },
        )
        .unwrap();
        // defer builder.deinit(); — runs after `finish` below (no-op then).

        let first = builder
            .allocate_page(Capacity {
                cols: 2,
                rows: 2,
                ..Capacity::default()
            })
            .unwrap();
        unsafe {
            (*first).size.rows = 2;
            *(*first).get_row_and_cell(0, 0).cell = Cell::init('A' as u32);
        }

        let second = builder
            .allocate_page(Capacity {
                cols: 4,
                rows: 2,
                ..Capacity::default()
            })
            .unwrap();
        unsafe {
            (*second).size.rows = 2;
            *(*second).get_row_and_cell(0, 0).cell = Cell::init('B' as u32);
        }

        let r = builder.finish().unwrap();
        builder.deinit();
        r
    };
    // defer result.deinit();

    // Successful finish transfers ownership and initializes the PageList's
    // desired geometry, viewport, and required tracked viewport pin.
    assert_eq!(4, result.cols);
    assert_eq!(3, result.rows);
    assert_eq!(2, result.total_pages());
    assert_eq!(1, result.count_tracked_pins());
    assert!(result.viewport == Viewport::Active);

    // Complete pages and their contents are preserved in insertion order,
    // including widths which have not yet been reflowed.
    let screen_top = result.get_top_left(point::Tag::Screen);
    assert_eq!(2, unsafe { crate::pagelist::Node::cols(screen_top.node) });
    assert_eq!('A' as u32, unsafe {
        (*(*crate::pagelist::Node::page(screen_top.node))
            .get_row_and_cell(0, 0)
            .cell)
            .codepoint()
    });

    // The active area is calculated backward from the newest page, so it
    // begins at row one of the oldest page and leaves row zero as history.
    let active_top = result.get_top_left(point::Tag::Active);
    assert_eq!(screen_top.node, active_top.node);
    assert_eq!(1, active_top.y);
    let next = unsafe { (*active_top.node).next };
    assert_eq!(4, unsafe { crate::pagelist::Node::cols(next) });
    assert_eq!('B' as u32, unsafe {
        (*(*crate::pagelist::Node::page(next))
            .get_row_and_cell(0, 0)
            .cell)
            .codepoint()
    });

    result.assert_integrity();
    result.deinit();
}

// ref: test "PageList Builder validates the finished list"
#[test]
fn pagelist_builder_validates_the_finished_list() {
    use crate::pagelist::builder::FinishError;
    use crate::pagelist::Builder;

    // The desired PageList geometry must describe a non-empty screen.
    {
        let mut builder = Builder::init(Global, Options::new(0, 1)).unwrap();
        // defer builder.deinit();
        assert!(matches!(
            builder.finish(),
            Err(FinishError::InvalidDimensions)
        ));
        builder.deinit();
    }

    // A PageList cannot be finished without any backing pages.
    {
        let mut builder = Builder::init(Global, Options::new(1, 1)).unwrap();
        // defer builder.deinit();
        assert!(matches!(builder.finish(), Err(FinishError::NoPages)));
        builder.deinit();
    }

    // Allocated capacity alone is insufficient: callers must populate a
    // nonzero logical page size before transferring ownership.
    {
        let mut builder = Builder::init(Global, Options::new(1, 1)).unwrap();
        // defer builder.deinit();
        let page = builder
            .allocate_page(Capacity {
                cols: 1,
                rows: 1,
                ..Capacity::default()
            })
            .unwrap();
        unsafe { (*page).size.rows = 0 };
        assert!(matches!(
            builder.finish(),
            Err(FinishError::InvalidPageDimensions)
        ));
        builder.deinit();
    }

    // The populated pages must contain enough rows to cover the active area.
    {
        let mut builder = Builder::init(Global, Options::new(1, 2)).unwrap();
        // defer builder.deinit();
        let page = builder
            .allocate_page(Capacity {
                cols: 1,
                rows: 1,
                ..Capacity::default()
            })
            .unwrap();
        unsafe { (*page).size.rows = 1 };
        assert!(matches!(
            builder.finish(),
            Err(FinishError::InsufficientRows)
        ));
        builder.deinit();
    }
}

// ref: test "PageList Builder finish is transactional on allocation failure"
//
// Ported as written (parity tail S7-3): the builder runs over a
// `FailingAllocator`. (This replaced a port-only tripwire stand-in.)
#[test]
fn pagelist_builder_finish_is_transactional_on_allocation_failure() {
    use crate::pagelist::builder::FinishError;
    use crate::pagelist::Builder;

    // Construct a valid builder so finish reaches its fallible bookkeeping
    // allocations after all page and geometry validation succeeds.
    // var failing = testing.FailingAllocator.init(testing.allocator, .{});
    let failing = FailingAllocator::over_heap(FailingConfig::default());
    let c = failing.allocator();
    with_handle(&c, |alloc| {
        let mut builder = Builder::init(alloc, Options::new(1, 1)).unwrap();
        // defer builder.deinit();
        let page = builder
            .allocate_page(Capacity {
                cols: 1,
                rows: 1,
                ..Capacity::default()
            })
            .unwrap();
        unsafe { (*page).size.rows = 1 };

        // The pools are preheated, so the next general allocation is the tracked
        // viewport pin map created by finish.
        failing.fail_index.set(failing.alloc_index.get());
        assert!(matches!(builder.finish(), Err(FinishError::OutOfMemory)));
        assert!(failing.has_induced_failure.get());

        // Failed finish leaves page ownership with the builder so its normal
        // deinit path can release the still-linked page.
        assert!(!builder.pages.first.is_null());
        assert_eq!(builder.pages.first, builder.pages.last);

        builder.deinit();
    });
}

// ref: test "PageList PageAllocation finalizes pages and preserves live state"
#[test]
fn pagelist_page_allocation_finalizes_pages_and_preserves_live_state() {
    use crate::pagelist::builder::Location;
    use crate::pagelist::Builder;

    // Build an existing list with history, a two-row active area, an external
    // active pin, and a pinned viewport whose absolute offset is cached.
    let mut result: PageList = {
        let mut builder = Builder::init(
            Global,
            Options {
                cols: 4,
                rows: 2,
                max_size: None,
                max_lines: None,
            },
        )
        .unwrap();
        // defer builder.deinit();

        let first = builder
            .allocate_page(Capacity {
                cols: 3,
                rows: 2,
                ..Capacity::default()
            })
            .unwrap();
        unsafe {
            (*first).size.rows = 2;
            *(*first).get_row_and_cell(0, 0).cell = Cell::init('C' as u32);
        }

        let second = builder
            .allocate_page(Capacity {
                cols: 4,
                rows: 2,
                ..Capacity::default()
            })
            .unwrap();
        unsafe {
            (*second).size.rows = 2;
            *(*second).get_row_and_cell(0, 0).cell = Cell::init('D' as u32);
        }

        let r = builder.finish().unwrap();
        builder.deinit();
        r
    };
    // defer result.deinit();

    let old_first = result.pages.first;
    let old_last = result.pages.last;
    let active_top = result.get_top_left(point::Tag::Active);
    let tracked_active = result.track_pin(active_top).unwrap();
    result.scroll(Scroll::Row(1));
    assert!(result.viewport == Viewport::Pin);
    assert_eq!(1, result.scrollbar().offset);

    // PORT NOTE (raw-self seam): `allocate_page` stores its receiver in the
    // returned allocation, so every allocation goes through this single raw
    // root; see the PORT NOTE on `PageList::allocate_page`.
    let this: *mut PageList = &mut result;

    // Prepend differently sized historical pages newest-first. Each page is
    // populated while detached and joins the live list only on success.
    {
        let mut allocation = unsafe {
            PageList::allocate_page(
                this,
                Capacity {
                    cols: 4,
                    rows: 1,
                    ..Capacity::default()
                },
            )
        }
        .unwrap();
        // defer allocation.deinit();
        let page = allocation.page();
        unsafe {
            (*page).size.rows = 1;
            *(*page).get_row_and_cell(0, 0).cell = Cell::init('B' as u32);
        }
        allocation.finalize(Location::Prepend).unwrap();
        allocation.deinit();
    }
    {
        let mut allocation = unsafe {
            PageList::allocate_page(
                this,
                Capacity {
                    cols: 2,
                    rows: 2,
                    ..Capacity::default()
                },
            )
        }
        .unwrap();
        // defer allocation.deinit();
        let page = allocation.page();
        unsafe {
            (*page).size.rows = 2;
            *(*page).get_row_and_cell(0, 0).cell = Cell::init('A' as u32);
        }
        allocation.finalize(Location::Prepend).unwrap();
        allocation.deinit();
    }

    // Repeated prepends reconstruct oldest-to-newest order without replacing
    // any existing nodes or tracked pins.
    assert_eq!(4, result.total_pages());
    assert_eq!(7, result.total_rows);
    assert_eq!(old_last, result.pages.last);
    assert_eq!(old_first, unsafe { (*(*result.pages.first).next).next });
    assert_eq!('A' as u32, unsafe {
        (*(*crate::pagelist::Node::page(result.pages.first))
            .get_row_and_cell(0, 0)
            .cell)
            .codepoint()
    });
    assert_eq!('B' as u32, unsafe {
        (*(*crate::pagelist::Node::page((*result.pages.first).next))
            .get_row_and_cell(0, 0)
            .cell)
            .codepoint()
    });
    assert!(active_top.eql(result.get_top_left(point::Tag::Active)));
    assert!(active_top.eql(unsafe { *tracked_active }));

    // The viewport remains pinned to the same content, while its cached row
    // offset and the scrollbar total include the three new historical rows.
    assert!(result.viewport == Viewport::Pin);
    assert_eq!(old_first, unsafe { (*result.viewport_pin).node });
    assert_eq!(1, unsafe { (*result.viewport_pin).y });
    let scrollbar_state = result.scrollbar();
    assert_eq!(7, scrollbar_state.total);
    assert_eq!(4, scrollbar_state.offset);
    assert_eq!(2, scrollbar_state.len);

    result.assert_integrity();
    result.deinit();
}

// ref: test "PageList PageAllocation stays detached until finalize"
#[test]
fn pagelist_page_allocation_stays_detached_until_finalize() {
    use crate::pagelist::builder::{FinalizeError, Location};

    let mut result = PageList::init(
        Global,
        Options {
            cols: 1,
            rows: 1,
            max_size: None,
            max_lines: None,
        },
    )
    .unwrap();
    // defer result.deinit();

    let initial_first = result.pages.first;
    let initial_last = result.pages.last;
    let initial_total_rows = result.total_rows;
    let initial_page_size = result.page_size;

    // PORT NOTE (raw-self seam): see
    // `pagelist_page_allocation_finalizes_pages_and_preserves_live_state`.
    //
    // TB (N492): `this` is the one raw root (`&raw mut`, no retag), and every
    // later access to `result` goes through it. A `&mut result` reborrow here
    // was frozen by the later read through the owner (`result.assert_integrity()`),
    // so the allocation's `deinit` (which returns its node to the pools through
    // `this`) was a write through a Frozen tag: Tree Borrows UB.
    let this: *mut PageList = &raw mut result;

    // Allocating and populating a detached page does not alter any live list
    // links or accounting. Deinit returns it to the same PageList pools.
    let mut detached = unsafe {
        PageList::allocate_page(
            this,
            Capacity {
                cols: 1,
                rows: 1,
                ..Capacity::default()
            },
        )
    }
    .unwrap();
    unsafe { (*detached.page()).size.rows = 1 };
    assert_eq!(initial_first, unsafe { (*this).pages.first });
    assert_eq!(initial_last, unsafe { (*this).pages.last });
    assert_eq!(initial_total_rows, unsafe { (*this).total_rows });
    assert_eq!(initial_page_size, unsafe { (*this).page_size });
    unsafe { (*this).assert_integrity() };
    detached.deinit();
    unsafe { (*this).assert_integrity() };

    // Invalid populated dimensions leave ownership with the allocation so it
    // can still be released normally.
    let mut invalid = unsafe {
        PageList::allocate_page(
            this,
            Capacity {
                cols: 1,
                rows: 1,
                ..Capacity::default()
            },
        )
    }
    .unwrap();
    // defer invalid.deinit();
    assert!(matches!(
        invalid.finalize(Location::Prepend),
        Err(FinalizeError::InvalidPageDimensions)
    ));

    assert_eq!(initial_first, unsafe { (*this).pages.first });
    assert_eq!(initial_last, unsafe { (*this).pages.last });
    assert_eq!(initial_total_rows, unsafe { (*this).total_rows });
    assert_eq!(initial_page_size, unsafe { (*this).page_size });
    unsafe { (*this).assert_integrity() };

    invalid.deinit();
    unsafe { (*this).deinit() };
}

// ref: test "PageList PageAllocation rejects limits before modifying the destination"
#[test]
fn pagelist_page_allocation_rejects_limits_before_modifying_the_destination() {
    use crate::pagelist::builder::{FinalizeError, Location};

    let mut result = PageList::init(
        Global,
        Options {
            cols: 1,
            rows: 1,
            max_size: Some(0),
            max_lines: None,
        },
    )
    .unwrap();
    // defer result.deinit();

    // PORT NOTE (raw-self seam): see
    // `pagelist_page_allocation_finalizes_pages_and_preserves_live_state`.
    //
    // TB (N492): `this` is the one raw root (`&raw mut`, no retag), and every
    // later access to `result` goes through it. A `&mut result` reborrow here
    // was frozen by the later read through the owner (`result.assert_integrity()`),
    // so the allocation's `deinit` (which returns its node to the pools through
    // `this`) was a write through a Frozen tag: Tree Borrows UB.
    let this: *mut PageList = &raw mut result;

    // The effective minimum permits one complete page beyond the active page.
    // Fill that allowance so the following allocation exceeds the byte limit.
    {
        let mut allocation = unsafe {
            PageList::allocate_page(
                this,
                Capacity {
                    cols: 1,
                    rows: 1,
                    ..Capacity::default()
                },
            )
        }
        .unwrap();
        // defer allocation.deinit();
        unsafe { (*allocation.page()).size.rows = 1 };
        allocation.finalize(Location::Prepend).unwrap();
        allocation.deinit();
    }

    let before_first = unsafe { (*this).pages.first };
    let before_total_rows = unsafe { (*this).total_rows };
    let before_page_size = unsafe { (*this).page_size };

    let mut allocation = unsafe {
        PageList::allocate_page(
            this,
            Capacity {
                cols: 1,
                rows: 1,
                ..Capacity::default()
            },
        )
    }
    .unwrap();
    // defer allocation.deinit();
    unsafe { (*allocation.page()).size.rows = 1 };
    assert!(matches!(
        allocation.finalize(Location::Prepend),
        Err(FinalizeError::MaxSizeExceeded)
    ));

    assert_eq!(before_first, unsafe { (*this).pages.first });
    assert_eq!(before_total_rows, unsafe { (*this).total_rows });
    assert_eq!(before_page_size, unsafe { (*this).page_size });
    unsafe { (*this).assert_integrity() };

    allocation.deinit();
    unsafe { (*this).deinit() };
}

// ref: test "PageList PageAllocation allocation failure leaves list unchanged"
#[test]
//
// Ported as written (parity tail S7-3): the PageList runs over a
// `FailingAllocator` and the allocator's next request, node-pool growth once
// the preheated slots run out, fails. (This replaced a port-only tripwire
// stand-in.)
fn pagelist_page_allocation_allocation_failure_leaves_list_unchanged() {
    use crate::pagelist::{OutOfMemory, PageAllocation};

    // var failing = testing.FailingAllocator.init(testing.allocator, .{});
    let failing = FailingAllocator::over_heap(FailingConfig::default());
    let c = failing.allocator();
    // SAFETY: `c` and `failing` outlive `result`, which is deinited below.
    let alloc = unsafe { crate::alloc::VtAllocator::from_c(core::ptr::NonNull::from(&c)) };
    let mut result = PageList::init(
        alloc,
        Options {
            cols: 1,
            rows: 1,
            max_size: None,
            max_lines: None,
        },
    )
    .unwrap();
    // defer result.deinit();

    let initial_first = result.pages.first;
    let initial_total_rows = result.total_rows;
    let initial_page_size = result.page_size;

    // Existing pool capacity is deliberately an implementation detail. Allow
    // preheated slots to succeed until allocation reaches node-pool growth.
    failing.fail_index.set(failing.alloc_index.get());

    // var allocations: [64]PageAllocation = undefined;
    // var allocation_count: usize = 0;
    // defer for (allocations[0..allocation_count]) |*allocation|
    //     allocation.deinit();
    let mut allocations: Vec<PageAllocation> = Vec::with_capacity(64);

    // PORT NOTE (raw-self seam): see
    // `pagelist_page_allocation_finalizes_pages_and_preserves_live_state`.
    let this: *mut PageList = &mut result;

    let mut failed = false;
    for _ in 0..64 {
        match unsafe {
            PageList::allocate_page(
                this,
                Capacity {
                    cols: 1,
                    rows: 1,
                    ..Capacity::default()
                },
            )
        } {
            Ok(allocation) => allocations.push(allocation),
            Err(err) => {
                assert_eq!(OutOfMemory, err);
                failed = true;
                break;
            }
        }
    }
    assert!(failed);
    assert!(failing.has_induced_failure.get());

    // Detached allocations and failed pool growth never publish into the live
    // list; the deferred cleanup returns every successful allocation.
    assert_eq!(initial_first, result.pages.first);
    assert_eq!(initial_total_rows, result.total_rows);
    assert_eq!(initial_page_size, result.page_size);
    result.assert_integrity();

    // defer for (allocations[0..allocation_count]) |*allocation|
    //     allocation.deinit();
    for allocation in allocations.iter_mut() {
        allocation.deinit();
    }
    result.deinit();
}

// ref: test "PageList eraseRow recycled row has default metadata"
#[test]
fn pagelist_erase_row_recycled_row_has_default_metadata() {
    let mut s = PageList::init(Global, Options::new(5, 3)).unwrap();

    // Simulate the top row being part of a soft-wrapped, prompt-marked
    // line. Erasing it recycles its Row storage as the new blank
    // bottom row, which must not retain any of this metadata.
    {
        let rac = s
            .get_cell(point::Point::Active(point::Coordinate::default()))
            .unwrap();
        unsafe {
            (*rac.row).set_wrap(true);
            (*rac.row).set_wrap_continuation(true);
            (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt);
        }
    }

    s.erase_row(point::Point::Active(point::Coordinate::default()))
        .unwrap();

    {
        let rac = s
            .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 2 }))
            .unwrap();
        unsafe {
            assert!(!(*rac.row).wrap());
            assert!(!(*rac.row).wrap_continuation());
            assert_eq!(SemanticPrompt::None, (*rac.row).semantic_prompt());
        }
    }

    s.deinit();
}

// ref: test "PageList eraseRowBounded recycled row has default metadata"
#[test]
fn pagelist_erase_row_bounded_recycled_row_has_default_metadata() {
    // A limit smaller than the remaining rows in the page exercises
    // the bounded-rotate branch; a larger limit exercises the fallback
    // branch that clears the final row after a full rotation.
    for limit in [1usize, 10] {
        let mut s = PageList::init(Global, Options::new(5, 3)).unwrap();

        {
            let rac = s
                .get_cell(point::Point::Active(point::Coordinate::default()))
                .unwrap();
            unsafe {
                (*rac.row).set_wrap(true);
                (*rac.row).set_wrap_continuation(true);
                (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt);
            }
        }

        s.erase_row_bounded(point::Point::Active(point::Coordinate::default()), limit)
            .unwrap();

        let recycled_y = core::cmp::min(limit, 2);
        let rac = s
            .get_cell(point::Point::Active(point::Coordinate {
                x: 0,
                y: recycled_y as u32,
            }))
            .unwrap();
        unsafe {
            assert!(!(*rac.row).wrap());
            assert!(!(*rac.row).wrap_continuation());
            assert_eq!(SemanticPrompt::None, (*rac.row).semantic_prompt());
        }

        s.deinit();
    }
}

// ref: test "PageList eraseActive regrown rows have default metadata"
#[test]
fn pagelist_erase_active_regrown_rows_have_default_metadata() {
    let mut s = PageList::init(Global, Options::new(5, 3)).unwrap();

    // Mark the rows that will be erased. eraseActive retires their
    // storage into unused page capacity and then regrows the active
    // area, re-exposing the same Row storage via the grow() fast path.
    for y in 0..2u32 {
        let rac = s
            .get_cell(point::Point::Active(point::Coordinate { x: 0, y }))
            .unwrap();
        unsafe {
            (*rac.row).set_wrap(true);
            (*rac.row).set_wrap_continuation(true);
            (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt);
        }
    }

    s.erase_active(1);

    for y in 0..3u32 {
        let rac = s
            .get_cell(point::Point::Active(point::Coordinate { x: 0, y }))
            .unwrap();
        unsafe {
            assert!(!(*rac.row).wrap());
            assert!(!(*rac.row).wrap_continuation());
            assert_eq!(SemanticPrompt::None, (*rac.row).semantic_prompt());
        }
    }

    s.deinit();
}

// ref: test "PageList split retired rows have default state"
#[test]
fn pagelist_split_retired_rows_have_default_state() {
    let mut s = PageList::init(Global, Options::new(5, 10)).unwrap();

    // Put metadata and a background-colored (non-zero, but text-free)
    // cell on a row that the split will move to the new page. The
    // retired Row storage on the source page goes back into unused
    // capacity that grow() re-exposes without clearing.
    {
        let rac = s
            .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 7 }))
            .unwrap();
        unsafe {
            (*rac.row).set_wrap(true);
            (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt);
            let mut c = Cell::zeroed();
            c.set_content_tag(ContentTag::BgColorPalette);
            c.set_content_color_palette(42);
            *rac.cell = c;
        }
    }

    let node = s.pages.first;
    s.split(
        s.pin(point::Point::Active(point::Coordinate { x: 0, y: 5 }))
            .unwrap(),
    )
    .unwrap();

    // The source page was truncated to 5 rows; peek at the retired
    // storage beyond size.rows.
    let page = unsafe { crate::pagelist::Node::page(node) };
    assert_eq!(5, unsafe { (*page).size.rows });
    let rows = unsafe { (*page).rows.ptr((*page).memory_ptr()) };
    for y in 5..10usize {
        let row = unsafe { *rows.add(y) };
        assert!(!row.wrap());
        assert!(!row.wrap_continuation());
        assert_eq!(SemanticPrompt::None, row.semantic_prompt());
        let cells = unsafe { row.cells().ptr((*page).memory_ptr()) };
        for x in 0..(unsafe { (*page).size.cols } as usize) {
            assert!(unsafe { *cells.add(x) }.is_zero());
        }
    }

    s.deinit();
}

// ref: test "PageList resize trimmed rows have default state"
#[test]
fn pagelist_resize_trimmed_rows_have_default_state() {
    let mut s = PageList::init(Global, Options::new(5, 5)).unwrap();

    // A trailing blank row has no text, so shrinking rows trims it,
    // but it can still carry metadata (e.g. a blank prompt
    // continuation line) and background-colored cells. Trimming
    // retires the storage into unused capacity that grow()
    // re-exposes without clearing.
    {
        let rac = s
            .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 4 }))
            .unwrap();
        unsafe {
            (*rac.row).set_wrap_continuation(true);
            (*rac.row).set_semantic_prompt(SemanticPrompt::PromptContinuation);
            let mut c = Cell::zeroed();
            c.set_content_tag(ContentTag::BgColorPalette);
            c.set_content_color_palette(42);
            *rac.cell = c;
        }
    }

    s.resize(Resize {
        rows: Some(4),
        reflow: false,
        ..Resize::default()
    })
    .unwrap();
    s.resize(Resize {
        rows: Some(5),
        reflow: false,
        ..Resize::default()
    })
    .unwrap();

    {
        let rac = s
            .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 4 }))
            .unwrap();
        unsafe {
            assert!(!(*rac.row).wrap());
            assert!(!(*rac.row).wrap_continuation());
            assert_eq!(SemanticPrompt::None, (*rac.row).semantic_prompt());
            assert!((*rac.cell).is_zero());
        }
    }

    s.deinit();
}

// UD80 (R13), the row-copy retry loop of `eraseRow` (`cloneRowGrowCapacity`):
// the destination page's style set is full, at the `u16` capacity ceiling, of
// pinned styles no cell uses (orphans: a count at 65,535 and no cell left,
// what styled output on a very wide terminal leaves behind), and the row
// pulled up across the page boundary carries a style that set does not have.
// The reference's loop panics ("increaseCapacity OutOfSpace"); the port
// re-clones the page once at the same capacity, which drops the orphans, and
// the copy succeeds. 4,000 columns keeps the page at a dozen rows, so the
// clones are cheap.
#[test]
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION — BOTH LANES (rustty-vt, parity tail C-1; .port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound with debug assertions on (G6) and past an 8-minute bound with them off (G6c), both measured one test per process; runs natively in cargo test --workspace and under ASan (see MIRI_EXCEPTIONS.md)"
)]
fn ud80_erase_row_into_a_page_full_of_orphaned_pinned_styles_reclaims_them() {
    let mut s = PageList::init(Global, Options::new(4000, 10)).unwrap();
    {
        let page = unsafe { crate::pagelist::Node::page(s.pages.last) };
        unsafe { (*page).pause_integrity_checks(true) };
        let n = unsafe { (*page).capacity.rows - (*page).size.rows };
        for _ in 0..n {
            let _ = s.grow().unwrap();
        }
        unsafe { (*page).pause_integrity_checks(false) };
        s.grow_rows(5).unwrap();
        assert_eq!(2, s.total_pages());
    }

    // The top row of the second page carries a bold cell.
    let bold = Style {
        flags: stylepkg::Flags {
            bold: true,
            ..Default::default()
        },
        ..Default::default()
    };
    unsafe {
        let page = crate::pagelist::Node::page(s.pages.last);
        let id = (*page).styles.add((*page).memory_ptr(), bold).unwrap();
        let rac = (*page).get_row_and_cell(0, 0);
        *rac.cell = cell_codepoint('S' as u32);
        (*rac.cell).set_style_id(id);
        (*rac.row).set_styled(true);
    }

    // The first page: style capacity at the ceiling, the set full of orphans.
    while unsafe { crate::pagelist::Node::capacity(s.pages.first).styles } < size::CellCountInt::MAX
    {
        let _ = s
            .increase_capacity(s.pages.first, Some(IncreaseCapacity::Styles))
            .unwrap();
    }
    let orphans = unsafe {
        let page = crate::pagelist::Node::page(s.pages.first);
        (*page).pause_integrity_checks(true);
        let mut n = 0u32;
        loop {
            let base = (*page).memory_ptr();
            let st = Style {
                fg_color: stylepkg::Color::Rgb(crate::color::RGB {
                    r: (n & 0xFF) as u8,
                    g: (n >> 8) as u8,
                    b: 0x33,
                }),
                ..Default::default()
            };
            let Ok(id) = (*page).styles.add(base, st) else {
                break;
            };
            (*page).styles.use_multiple(base, id, u16::MAX - 1);
            n += 1;
        }
        (*page).pause_integrity_checks(false);
        (*page).assert_integrity();
        n
    };
    assert!(orphans > 50_000, "the set is full of orphans: {orphans}");
    assert!(crate::pagelist::grow::styles_hold_pinned(unsafe {
        crate::pagelist::Node::page(s.pages.first)
    }));

    s.erase_row(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
        .unwrap();

    // The bold row moved up into the first page (active row 4).
    let lc = s
        .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 4 }))
        .unwrap();
    assert!(std::ptr::eq(lc.node, s.pages.first));
    let page: *mut Page = unsafe { crate::pagelist::Node::page(lc.node) };
    assert_eq!(unsafe { (*lc.cell).codepoint() }, 'S' as u32);
    let st = unsafe {
        *(*page)
            .styles
            .get((*page).memory_ptr(), (*lc.cell).style_id())
    };
    assert!(st.flags.bold, "the copied cell keeps its style");
    assert!(
        unsafe { (*page).styles.count() } < 10,
        "the orphans were dropped"
    );
    // The re-clone is at the SAME capacity: the ceiling is unchanged (a
    // smaller-capacity re-clone would still drop the orphans but is a
    // divergence the reclaim asserts alone do not catch; R13W2 review).
    assert_eq!(
        unsafe { (*page).capacity.styles },
        size::CellCountInt::MAX,
        "the style capacity stays at the ceiling"
    );
    let mut node_ = s.pages.first;
    while !node_.is_null() {
        unsafe { (*crate::pagelist::Node::page(node_)).assert_integrity() };
        node_ = unsafe { (*node_).next };
    }
    s.deinit();
}

const HYPERLINK_URI: &[u8] = b"https://ud131.example/";

// UD131 (R13): the `eraseRow`/`eraseRowBounded` row-copy retry loop
// (`PageList::clone_row_grow_capacity`) when the destination page's style set
// is genuinely FULL of LIVE styles at the `u16` capacity ceiling (no orphaned
// pinned styles for the W2 re-clone to reclaim) and the row pulled across the
// page boundary carries a style the set does not have. The reference
// `@panic("increaseCapacity OutOfSpace")` here (PageList.zig:5108); measured
// reachable from real VT input on a wide terminal (a LF/scroll on the
// alternate screen). The port instead finishes the copy: the unstorable style
// degrades to the default while the codepoint, the storable grapheme and
// hyperlink, and the row flags are kept.
//
// Reaching the loop: the same loop is driven by real VT in the W2 orphan tests
// (`ud80_erase_row_..._reclaims_them`); here the loop is called directly so the
// full-LIVE-set precondition can be fabricated (`test_fill_full_live_pinned`)
// instead of writing the ~8.5 billion cells the review probe needed. A wide
// (50,000-col) page is required because a full set is 53,247 distinct styles,
// each needing a live cell -- more cells than any standard-width page holds. To
// keep it fast, the two pages are made with `split` so each keeps a SMALL
// `size.rows` (the full set fits on two 50,000-wide rows); page clones copy
// `size.rows`, so a full 215-row page would make every capacity growth a
// 10.75M-cell copy.
#[test]
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION — BOTH LANES (rustty-vt, parity tail C-1; .port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound with debug assertions on (G6) and past an 8-minute bound with them off (G6c), both measured one test per process; runs natively in cargo test --workspace and under ASan (see MIRI_EXCEPTIONS.md)"
)]
fn ud131_erase_row_loop_full_live_set_degrades_new_style_to_default() {
    let mut s = PageList::init(Global, Options::new(50000, 8)).unwrap();
    // Two pages of four rows each, both with a small `size.rows`.
    s.split(Pin {
        node: s.pages.first,
        y: 4,
        x: 0,
        garbage: false,
    })
    .unwrap();
    assert_eq!(2, s.total_pages());
    let dst_node = s.pages.first;
    let src_node = s.pages.last;
    assert!(!std::ptr::eq(dst_node, src_node));

    // The source row (top of the second page) carries: a NEW style ('A'), a
    // grapheme ('B' + combining acute), a hyperlink ('C'), a plain cell ('D'),
    // and the row flags (wrap + a semantic prompt).
    let src_page: *mut Page = unsafe { crate::pagelist::Node::page(src_node) };
    let src_row: *const crate::page::Row = unsafe { (*src_page).get_row(0) };
    let link_id = unsafe {
        (*src_page).insert_hyperlink(hyperlink::Hyperlink {
            id: hyperlink::HyperlinkId::Implicit(0),
            uri: HYPERLINK_URI.to_vec(),
        })
    }
    .unwrap();
    let new_style = Style {
        flags: stylepkg::Flags {
            bold: true,
            ..Default::default()
        },
        ..Default::default()
    };
    unsafe {
        let base = (*src_page).memory_ptr();
        let new_id = (*src_page).styles.add(base, new_style).unwrap();
        let row = (*src_page).get_row(0);
        (*row).set_wrap(true);
        (*row).set_semantic_prompt(SemanticPrompt::Prompt);

        let c0 = (*src_page).get_row_and_cell(0, 0).cell;
        *c0 = cell_codepoint('A' as u32);
        (*c0).set_style_id(new_id);
        (*row).set_styled(true);

        let c1 = (*src_page).get_row_and_cell(1, 0).cell;
        *c1 = cell_codepoint('B' as u32);
        (*src_page).set_graphemes(row, c1, &[0x0301u32]).unwrap();

        let c2 = (*src_page).get_row_and_cell(2, 0).cell;
        *c2 = cell_codepoint('C' as u32);
        (*src_page).set_hyperlink(row, c2, link_id).unwrap();

        let c3 = (*src_page).get_row_and_cell(3, 0).cell;
        *c3 = cell_codepoint('D' as u32);
    }

    // The destination page: style capacity grown to the ceiling, then filled
    // with a full LIVE set on rows 5.., with row 0 left blank as the clone
    // target. One style is pinned genuinely so `styles_hold_pinned` stays true
    // across the one re-clone (which is what makes W2's one-shot flag matter:
    // without it, the loop re-clones forever).
    // `increase_capacity` REPLACES the node, so re-fetch `pages.first` each
    // iteration (as the UD80 presize helper does).
    while unsafe { crate::pagelist::Node::capacity(s.pages.first).styles } < size::CellCountInt::MAX
    {
        let _ = s
            .increase_capacity(s.pages.first, Some(IncreaseCapacity::Styles))
            .unwrap();
    }
    let dst_node = s.pages.first;
    // A full LIVE set across rows 1..4 (row 0 stays blank as the clone target),
    // with the first style genuinely pinned (saturated by its cells). The pin
    // keeps `styles_hold_pinned` true across the one re-clone, so the loop
    // re-clones once (reclaiming nothing), then degrades — which is exactly the
    // path W2's one-shot flag bounds. A regression of that flag (task R13-X2's
    // control: resetting it whenever the node changed) now fails this test in
    // under a second on the loop's debug re-clone counter ("UD80 re-clone ran
    // twice: one-shot flag regressed") instead of re-cloning forever.
    let (live, _pinned) =
        unsafe { (*crate::pagelist::Node::page(dst_node)).test_fill_full_live_pinned(1..4, true) };
    assert!(live > 50_000, "the set is full of live styles: {live}");
    assert!(crate::pagelist::grow::styles_hold_pinned(unsafe {
        crate::pagelist::Node::page(dst_node)
    }));

    // Drive the loop directly.
    let out = s.clone_row_grow_capacity(dst_node, 0, src_page, src_row);
    let page: *mut Page = unsafe { crate::pagelist::Node::page(out) };

    unsafe {
        let row = (*page).get_row(0);
        assert!((*row).wrap(), "the row wrap flag is kept");
        assert_eq!(
            (*row).semantic_prompt(),
            SemanticPrompt::Prompt,
            "the row semantic prompt is kept"
        );

        let c0 = (*page).get_row_and_cell(0, 0).cell;
        assert_eq!((*c0).codepoint(), 'A' as u32);
        assert_eq!(
            (*c0).style_id(),
            stylepkg::DEFAULT_ID,
            "the unstorable style degrades to the default"
        );

        let c1 = (*page).get_row_and_cell(1, 0).cell;
        assert_eq!((*c1).codepoint(), 'B' as u32);
        assert!((*c1).has_grapheme(), "the grapheme is kept");
        assert_eq!(
            (*page).lookup_grapheme(c1).unwrap(),
            &[0x0301u32],
            "the grapheme codepoints are kept"
        );

        let c2 = (*page).get_row_and_cell(2, 0).cell;
        assert_eq!((*c2).codepoint(), 'C' as u32);
        assert!((*c2).hyperlink(), "the hyperlink is kept");

        let c3 = (*page).get_row_and_cell(3, 0).cell;
        assert_eq!((*c3).codepoint(), 'D' as u32);
        assert_eq!((*c3).style_id(), stylepkg::DEFAULT_ID);

        // The live set is untouched: still full, still pinned.
        assert!(
            (*page).styles.count() > 50_000,
            "the live styles are kept: {}",
            (*page).styles.count()
        );
        assert!(crate::pagelist::grow::styles_hold_pinned(page));
    }

    let mut node_ = s.pages.first;
    while !node_.is_null() {
        unsafe { (*crate::pagelist::Node::page(node_)).assert_integrity() };
        node_ = unsafe { (*node_).next };
    }
    s.deinit();
}

// UD131 (R13), the per-dimension degrade: a full LIVE style set at the `u16`
// ceiling must degrade ONLY the cells whose style is new to that set. Every
// other dimension of the copy (graphemes, hyperlinks, their strings) must
// still grow, and a style the set already has must still be stored. The
// source row mixes all of these (`ud131_test_support::build_mixed_src`: a wide
// character and its spacer on an existing style and on a new one, existing and
// new styles, a 3,000-cell grapheme run that needs grapheme growth, an
// implicit and an explicit hyperlink, and a 200-cell run over 12 long-URI
// links that needs hyperlink and string growth). The copy into the full page
// is compared cell by cell with a control copy into the same kind of page
// with an ordinary style set.
//
// The degrade is logged once per copy even though the copy retries several
// times after the style dimension degrades (once per grown dimension); two
// copies log twice.
//
// NEGATIVE CONTROLS (task R13-X2, macOS 2026-09-23): "degrade every style"
// (the degrade copy never stores a style) and "degrade graphemes when only
// styles are full" (`degrade_dimension` setting every flag, R13-X's
// behaviour) each turn this test and its Screen twin red.
fn ud131_mixed_fixture(fill: bool) -> PageList {
    let mut s = PageList::init(
        Global,
        Options::new(crate::page::ud131_test_support::mixed_cols(fill), 6),
    )
    .unwrap();
    s.split(Pin {
        node: s.pages.first,
        y: 3,
        x: 0,
        garbage: false,
    })
    .unwrap();
    assert_eq!(2, s.total_pages());
    // Room on the SOURCE page for the mixed row's graphemes and links.
    for (dim, n) in [
        (IncreaseCapacity::GraphemeBytes, 8),
        (IncreaseCapacity::HyperlinkBytes, 8),
        (IncreaseCapacity::StringBytes, 4),
    ] {
        for _ in 0..n {
            let _ = s.increase_capacity(s.pages.last, Some(dim)).unwrap();
        }
    }
    if fill {
        while unsafe { crate::pagelist::Node::capacity(s.pages.first).styles }
            < size::CellCountInt::MAX
        {
            let _ = s
                .increase_capacity(s.pages.first, Some(IncreaseCapacity::Styles))
                .unwrap();
        }
        // No pinned style: the loop degrades on the first `OutOfSpace`.
        let (live, _) = unsafe {
            (*crate::pagelist::Node::page(s.pages.first)).test_fill_full_live_pinned(1..3, false)
        };
        assert!(live > 50_000, "the set is full of live styles: {live}");
    }
    s
}

#[test]
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION — BOTH LANES (rustty-vt, parity tail C-1; .port/phase-2/MIRI_EXCEPTIONS.md, 'Parity tail C-1 — the G6 triage'): past a 15-minute Miri bound with debug assertions on (G6) and past an 8-minute bound with them off (G6c), both measured one test per process; runs natively in cargo test --workspace and under ASan (see MIRI_EXCEPTIONS.md)"
)]
fn ud131_erase_row_loop_degrades_only_new_styles_and_grows_the_other_dimensions() {
    use crate::page::ud131_test_support as t;
    // Control: an ordinary destination stores everything.
    let mut ctl = ud131_mixed_fixture(false);
    let ctl_src = unsafe { crate::pagelist::Node::page(ctl.pages.last) };
    let ctl_row = unsafe { t::build_mixed_src(ctl_src, 0) };
    let cap = crate::log_capture::start();
    let ctl_out = ctl.clone_row_grow_capacity(ctl.pages.first, 0, ctl_src, ctl_row);
    assert!(
        !cap.messages().iter().any(|(_, _, m)| m.contains("UD131")),
        "the control copy does not degrade"
    );
    drop(cap);
    let ctl_page = unsafe { crate::pagelist::Node::page(ctl_out) };

    // Degraded: the destination's style set is full of live styles.
    let mut deg = ud131_mixed_fixture(true);
    let deg_src = unsafe { crate::pagelist::Node::page(deg.pages.last) };
    let deg_row = unsafe { t::build_mixed_src(deg_src, 0) };
    let start_cap = unsafe { crate::pagelist::Node::capacity(deg.pages.first) };
    let styles_before = unsafe {
        (*crate::pagelist::Node::page(deg.pages.first))
            .styles
            .count()
    };

    let cap = crate::log_capture::start();
    let out = deg.clone_row_grow_capacity(deg.pages.first, 0, deg_src, deg_row);
    let first_copy = cap.messages();
    // A second copy of the same row into the same destination row.
    let out = deg.clone_row_grow_capacity(out, 0, deg_src, deg_row);
    let both_copies = cap.messages();
    drop(cap);
    let page = unsafe { crate::pagelist::Node::page(out) };

    unsafe {
        t::assert_only_new_styles_degraded(ctl_page, page, 0);

        // The other dimensions grew instead of degrading.
        let c = (*page).capacity;
        assert!(
            c.grapheme_bytes > start_cap.grapheme_bytes,
            "grapheme bytes grew: {} -> {}",
            start_cap.grapheme_bytes,
            c.grapheme_bytes
        );
        assert!(
            c.hyperlink_bytes > start_cap.hyperlink_bytes,
            "hyperlink bytes grew: {} -> {}",
            start_cap.hyperlink_bytes,
            c.hyperlink_bytes
        );
        assert!(
            c.string_bytes > start_cap.string_bytes,
            "string bytes grew: {} -> {}",
            start_cap.string_bytes,
            c.string_bytes
        );
        // The style set stayed at its ceiling and gained nothing.
        assert_eq!(c.styles, size::CellCountInt::MAX);
        assert_eq!((*page).styles.count(), styles_before);

        (*page).assert_integrity();
        (*ctl_page).assert_integrity();
    }

    // Logged once per copy, with the per-dimension counts: only styles.
    let expected = (
        log::Level::Warn,
        crate::page::LOG_SCOPE.to_string(),
        "row copy at a page capacity ceiling: 5 cell part(s) degraded to default \
         (styles=5, hyperlinks=0, graphemes=0); see UD131"
            .to_string(),
    );
    let ud131 = |v: &[crate::log_capture::CapturedRecord]| {
        v.iter()
            .filter(|(_, _, m)| m.contains("UD131"))
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(
        ud131(&first_copy),
        vec![expected.clone()],
        "one record for one copy"
    );
    assert_eq!(
        ud131(&both_copies),
        vec![expected.clone(), expected],
        "one record per copy"
    );

    ctl.deinit();
    deg.deinit();
}

// UD131 (R13): the degrade dimension mapping is coherent. For EVERY
// `CloneFromError`, the flag that `IncreaseCapacity::for_clone_error` -> the
// row-copy loop's `degrade_dimension` sets is exactly the one
// `RowDegradeMask::degrades` consults, and no other. Without this the non-style
// degrade paths (hyperlinks, strings, graphemes) are untested: mis-mapping
// hyperlink errors to the `strings` flag in `degrades` leaves every VT test
// green, yet at runtime a degraded hyperlink dimension would be grown again
// (`debug_assert!` in debug, an infinite loop in release). NEGATIVE CONTROL
// (task R13-X3): change `HyperlinkSetOutOfMemory | HyperlinkMapOutOfMemory =>
// self.hyperlinks` to `self.strings` in `RowDegradeMask::degrades` and this
// test fails.
#[test]
fn ud131_degrade_dimension_mapping_is_coherent() {
    use crate::page::{CloneFromError as E, RowDegradeMask};
    use crate::pagelist::grow::degrade_dimension;

    // Every variant of `CloneFromError`, exhaustively (a `match` below makes the
    // compiler enforce the list stays complete).
    let all = [
        E::StyleSetOutOfMemory,
        E::StyleSetNeedsRehash,
        E::StringAllocOutOfMemory,
        E::HyperlinkSetOutOfMemory,
        E::HyperlinkSetNeedsRehash,
        E::HyperlinkMapOutOfMemory,
        E::GraphemeMapOutOfMemory,
        E::GraphemeAllocOutOfMemory,
    ];
    for err in all {
        // Exhaustiveness guard: a new variant forces an update here.
        match err {
            E::StyleSetOutOfMemory
            | E::StyleSetNeedsRehash
            | E::StringAllocOutOfMemory
            | E::HyperlinkSetOutOfMemory
            | E::HyperlinkSetNeedsRehash
            | E::HyperlinkMapOutOfMemory
            | E::GraphemeMapOutOfMemory
            | E::GraphemeAllocOutOfMemory => {}
        }

        let adj = IncreaseCapacity::for_clone_error(err);
        match adj {
            None => {
                // Rehash errors grow nothing, so no mask ever degrades them:
                // the loop always returns them and rehashes, as the reference
                // does. True on both the empty and the fully-degraded mask.
                assert!(
                    !RowDegradeMask::default().degrades(err),
                    "rehash error {err:?} must never be degraded"
                );
                assert!(
                    !RowDegradeMask::ALL.degrades(err),
                    "rehash error {err:?} must never be degraded, even under ALL"
                );
            }
            Some(_) => {
                // The mask the loop builds for this error: only the dimension
                // `for_clone_error` grows for it.
                let mut mask = RowDegradeMask::default();
                degrade_dimension(&mut mask, adj);

                // Exactly one flag set: the mapping is 1:1.
                let set = [mask.styles, mask.graphemes, mask.hyperlinks, mask.strings]
                    .iter()
                    .filter(|b| **b)
                    .count();
                assert_eq!(set, 1, "exactly one dimension degrades {err:?}");

                // That flag is the one `degrades` consults for this error.
                assert!(
                    mask.degrades(err),
                    "the flag set for {err:?} is the one that degrades it"
                );

                // And the OTHER three flags do not degrade it: a mask with
                // every flag BUT the mapped one leaves this error to be
                // returned (grown), never degraded. This catches a `degrades`
                // that consults the wrong flag.
                let mut others = RowDegradeMask::ALL;
                // Clear only the mapped flag:
                if mask.styles {
                    others.styles = false;
                }
                if mask.graphemes {
                    others.graphemes = false;
                }
                if mask.hyperlinks {
                    others.hyperlinks = false;
                }
                if mask.strings {
                    others.strings = false;
                }
                assert!(
                    !others.degrades(err),
                    "no dimension other than the mapped one degrades {err:?}"
                );
            }
        }
    }
}

// Parity S124-F2 (no reference test): `diagram` prints codepoints with
// `writer.print("{u}", ..)`, which (zig 0.16
// `std.Io.Writer.printUnicodeCodepoint`) writes U+FFFD for a surrogate half or
// an out-of-range value instead of failing. The port used
// `char::from_u32(..).unwrap()`, which panics.
#[test]
fn pagelist_diagram_writes_replacement_for_unencodable_codepoints() {
    let mut s = PageList::init(Global, Options::new(4, 2)).unwrap();
    unsafe {
        let page = crate::pagelist::Node::page(s.pages.first);
        *(*page).get_row_and_cell(0, 0).cell = cell_codepoint('A' as u32);
        *(*page).get_row_and_cell(1, 0).cell = cell_codepoint(0xD800);
        *(*page).get_row_and_cell(2, 0).cell = cell_codepoint(0x1F_FFFF);
    }
    let mut out = String::new();
    s.diagram(&mut out).unwrap();
    assert!(out.contains("A\u{FFFD}\u{FFFD}"), "{out}");
    s.deinit();
}

/// Make the first page of a fresh single-page list cold in constant time: fill
/// it to capacity by bumping `size.rows` (`init_buf` formats every row up to
/// capacity, so the rows are valid blank rows), `grow()` once for the second
/// page, and bump that page to the active area's height. The first page is
/// then entirely history (S7-9; see `pagelist_grow_prune_miri_tractable`).
fn make_first_page_cold_surgically_for_test(s: &mut PageList) {
    assert!(core::ptr::eq(s.pages.first, s.pages.last), "a fresh list");
    unsafe {
        let first = &mut (*crate::pagelist::Node::page(s.pages.first));
        first.size.rows = first.capacity.rows;
    }
    s.total_rows = s.total_rows();
    let second = s.grow().unwrap().expect("the second page is allocated");
    assert!(core::ptr::eq(second, s.pages.last));
    unsafe {
        let last = &mut (*crate::pagelist::Node::page(s.pages.last));
        assert!(s.rows as usize <= last.capacity.rows as usize);
        last.size.rows = s.rows;
    }
    s.total_rows = s.total_rows();
    s.assert_integrity();
    let active = s.get_top_left(point::Tag::Active).node;
    assert!(
        core::ptr::eq(s.pages.last, active),
        "the active area is on page 2"
    );
    assert_eq!(2, s.total_pages());
}

// Parity tail S7-3 routing evidence (Rust-only; the reference has no unit test
// for this, its routing is by construction): every PageList allocation the
// reference makes through `alloc` (the node and pin pools, every tracked-pin
// set, the compressed-page encoding, `pagePreservingState`'s buffer, the clone's
// pools and pin remap) reaches the caller's allocator, and page memory does
// not (it stays on the page allocator). `deinit` returns every byte.
#[test]
fn pagelist_internals_route_through_the_allocator_handle() {
    use crate::alloc::testing::{with_handle, CountingAllocator};

    let counting = CountingAllocator::over_heap();
    let c = counting.allocator();
    with_handle(&c, |alloc| {
        // init: the pools and the tracked-pin set (entries array + index).
        //
        // The pools alone (P7-W1 T5 re-derivation, `e01e75bbb`, `efa3c66ed`,
        // `35a6fb747`): the node pool's free list (one slot) and its one
        // preheated node (`NODE_PREHEAT`, each node its own allocation), the
        // page pool's free list (`PAGE_PREHEAT` slots; the page items
        // themselves come from the page allocator), and the pin pool arena
        // (one chunk for its `PIN_PREHEAT` pins), all returned when the pools
        // are dropped (`MemoryPool.deinit`).
        {
            let pool = crate::pagelist::MemoryPool::init(
                alloc,
                crate::pagelist::page_backing_alloc(),
                crate::pagelist::PAGE_PREHEAT,
            )
            .unwrap();
            assert_eq!(
                4,
                counting.alloc_calls.get(),
                "node free list + 1 node, page free list, pin pool arena"
            );
            drop(pool);
            assert_eq!(0, counting.live_bytes.get());
        }
        let pools = counting.alloc_calls.get();
        let mut s = PageList::init(alloc, Options::new(80, 24)).unwrap();
        assert_eq!(
            pools + 4 + 2,
            counting.alloc_calls.get(),
            "PageList.init: the pools (4) plus the tracked-pin set (entries + index); \
             the first page's node is the preheated one"
        );
        // Page memory is NOT routed: the caller's allocator holds far less than
        // one standard page.
        assert!(
            counting.live_bytes.get() < crate::pagelist::std_size(),
            "page memory reached the caller's allocator ({} live bytes)",
            counting.live_bytes.get()
        );

        // track/untrack: a tracked pin comes from the (preheated) pin pool and
        // the set already has room, so this makes no new caller allocation;
        // the pin is returned to the pool on untrack.
        let p = s
            .track_pin(
                s.pin(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
                    .unwrap(),
            )
            .unwrap();
        s.untrack_pin(p);

        // Compression: the exact-sized encoding is allocated through `alloc`.
        //
        // One cold page is made surgically (S7-9, Miri budget): the first page
        // is filled to capacity and the second to the active area by bumping
        // `size.rows` (`init_buf` formats rows to full capacity, the
        // `pagelist_grow_prune_miri_tractable` precedent), with one `grow()`
        // between them, instead of `grow_cold_pages_for_test`'s one `grow()`
        // per row. Each of those ~600 grows runs the O(page) integrity walk,
        // which under Miri was quadratic (the test ran 18 h). The routing
        // claims below are about the compression, decode and clone calls, not
        // about how the history was written, and the list is the same shape
        // (two standard pages, the first one cold), so the one configuration
        // serves both the native and the Miri lane.
        make_first_page_cold_surgically_for_test(&mut s);
        let before = counting.alloc_calls.get();
        let first = s.pages.first;
        assert!(s.compress_page(first));
        let encoded_len = compressed_encoded_for_test(first).len();
        assert_eq!(
            before + 1,
            counting.alloc_calls.get(),
            "one exact-sized encoding"
        );
        assert_eq!(Some((encoded_len, 0)), counting.last_alloc.get());

        // pagePreservingState(alloc): one page-aligned decode buffer, freed by
        // `deinit` through the same allocator.
        let before = counting.alloc_calls.get();
        let mut preserved =
            unsafe { crate::pagelist::Node::page_preserving_state(first, alloc) }.unwrap();
        assert_eq!(before + 1, counting.alloc_calls.get());
        let (len, align_log2) = counting.last_alloc.get().unwrap();
        assert_eq!(1usize << align_log2, crate::page::PAGE_SIZE_MIN);
        preserved.deinit();
        assert_eq!(Some((len, align_log2)), counting.last_free.get());

        // clone(alloc): the clone's pools (sized to its page count) and its
        // tracked-pin set. First without a pin remap, as the baseline...
        //
        // The clones start at the last history row, so they span both pages
        // (the pools are sized for two) but copy 1 + 24 rows rather than the
        // whole first page (S7-9: row-by-row cloning of a full page is the
        // bulk of this test's cost natively, and far more under Miri).
        let last_history_row = (s.total_rows - s.rows as usize - 1) as u32;
        let before = counting.alloc_calls.get();
        let mut plain = crate::pagelist::access::Clone {
            top: point::Point::Screen(point::Coordinate {
                x: 0,
                y: last_history_row,
            }),
            bot: None,
            tracked_pins: None,
        };
        let mut baseline = s.clone(alloc, &mut plain).unwrap();
        let clone_calls = counting.alloc_calls.get() - before;
        // The node pool preheats one node whatever the page count
        // (`efa3c66ed`), so the clone's second page allocates its node; its
        // free-list slot grows the existing list in place (a remap, not an
        // alloc call).
        assert_eq!(
            4 + 2 + 1,
            clone_calls,
            "the clone's pools (4), tracked-pin set (entries + index) and second node"
        );
        baseline.deinit();

        // ...then with one: exactly one more allocation, the remap map's
        // table (`TrackedPinsRemap.init(alloc)`, grown by `putNoClobber`). The
        // one remapped pin fits the clone's pin set as built. (The viewport
        // pin is above the cloned range, so a pin is tracked inside it: the
        // top of the active area.)
        let q = s
            .track_pin(
                s.pin(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
                    .unwrap(),
            )
            .unwrap();
        let before = counting.alloc_calls.get();
        let mut opts = crate::pagelist::access::Clone {
            top: point::Point::Screen(point::Coordinate {
                x: 0,
                y: last_history_row,
            }),
            bot: None,
            tracked_pins: Some(crate::pagelist::access::TrackedPinsRemap::with_hasher_in(
                Default::default(),
                alloc,
            )),
        };
        let mut cloned = s.clone(alloc, &mut opts).unwrap();
        assert_eq!(
            clone_calls + 1,
            counting.alloc_calls.get() - before,
            "the pin remap allocates through alloc"
        );
        assert_eq!(
            1,
            opts.tracked_pins.as_ref().unwrap().len(),
            "one pin remapped"
        );
        drop(opts);
        cloned.deinit();
        s.untrack_pin(q);

        s.deinit();
    });
    assert_eq!(0, counting.live_bytes.get(), "deinit returns every byte");
}
