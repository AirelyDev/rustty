//! PORTS: ghostty src/terminal/PageList.zig — the
//! `pageIterator`/`cellIterator`/`promptIterator`, `highlightSemanticContent`,
//! `erase`/`eraseRowBounded` and `clone` tests.

use super::{cell_codepoint, cell_codepoint_sc};
use crate::alloc::Global;
use crate::page::{SemanticContent, SemanticPrompt};
use crate::pagelist::scroll::Scroll;
use crate::pagelist::{Options, OutOfMemory, PageList};
use crate::point;
use crate::style::{self as stylepkg, Style};
use rustty_render_types::term_size as size;

// ref: test "PageList pageIterator single page"
#[test]
fn pagelist_pageIterator_single_page() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

    // The viewport should be within a single page
    assert!(unsafe { (*s.pages.first).next.is_null() });

    // Iterate the active area
    let mut it = s.page_iterator(
        crate::pagelist::iter::Direction::RightDown,
        point::Point::Active(point::Coordinate::default()),
        None,
    );
    {
        let chunk = it.next().unwrap();
        assert!(core::ptr::eq(chunk.node, s.pages.first));
        assert_eq!(0_usize, chunk.start as usize);
        assert_eq!(s.rows as usize, chunk.end as usize);
    }

    // Should only have one chunk
    assert!(it.next().is_none());

    s.deinit();
}

// ref: test "PageList pageIterator two pages"
#[test]
fn pagelist_pageIterator_two_pages() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

    // Grow to capacity
    let page1_node = s.pages.last;
    let (page1_cap_rows, page1_size_rows) = {
        let page1 = unsafe { &(*crate::pagelist::Node::page(page1_node)) };
        (page1.capacity.rows, page1.size.rows)
    };
    unsafe { (*crate::pagelist::Node::page(page1_node)).pause_integrity_checks(true) };
    for _ in 0..(page1_cap_rows - page1_size_rows) {
        assert!(s.grow().unwrap().is_none());
    }
    unsafe { (*crate::pagelist::Node::page(page1_node)).pause_integrity_checks(false) };
    assert!(s.grow().unwrap().is_some());

    // Iterate the active area
    let mut it = s.page_iterator(
        crate::pagelist::iter::Direction::RightDown,
        point::Point::Active(point::Coordinate::default()),
        None,
    );
    {
        let chunk = it.next().unwrap();
        assert!(core::ptr::eq(chunk.node, s.pages.first));
        let start = unsafe { crate::pagelist::Node::rows(chunk.node) } - s.rows + 1;
        assert_eq!(start, chunk.start);
        assert_eq!(
            unsafe { crate::pagelist::Node::rows(chunk.node) },
            chunk.end
        );
    }
    {
        let chunk = it.next().unwrap();
        assert!(core::ptr::eq(chunk.node, s.pages.last));
        let start: size::CellCountInt = 0;
        assert_eq!(start, chunk.start);
        assert_eq!(start + 1, chunk.end);
    }
    assert!(it.next().is_none());

    s.deinit();
}

// ref: test "PageList pageIterator history two pages"
#[test]
fn pagelist_pageIterator_history_two_pages() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

    // Grow to capacity
    let page1_node = s.pages.last;
    let (page1_cap_rows, page1_size_rows) = {
        let page1 = unsafe { &(*crate::pagelist::Node::page(page1_node)) };
        (page1.capacity.rows, page1.size.rows)
    };
    unsafe { (*crate::pagelist::Node::page(page1_node)).pause_integrity_checks(true) };
    for _ in 0..(page1_cap_rows - page1_size_rows) {
        assert!(s.grow().unwrap().is_none());
    }
    unsafe { (*crate::pagelist::Node::page(page1_node)).pause_integrity_checks(false) };
    assert!(s.grow().unwrap().is_some());

    // Iterate the active area
    let mut it = s.page_iterator(
        crate::pagelist::iter::Direction::RightDown,
        point::Point::History(point::Coordinate::default()),
        None,
    );
    {
        let active_tl = s.get_top_left(point::Tag::Active);
        let chunk = it.next().unwrap();
        assert!(core::ptr::eq(chunk.node, s.pages.first));
        let start: size::CellCountInt = 0;
        assert_eq!(start, chunk.start);
        assert_eq!(active_tl.y, chunk.end);
    }
    assert!(it.next().is_none());

    s.deinit();
}

// ref: test "PageList pageIterator reverse single page"
#[test]
fn pagelist_pageIterator_reverse_single_page() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

    // The viewport should be within a single page
    assert!(unsafe { (*s.pages.first).next.is_null() });

    // Iterate the active area
    let mut it = s.page_iterator(
        crate::pagelist::iter::Direction::LeftUp,
        point::Point::Active(point::Coordinate::default()),
        None,
    );
    {
        let chunk = it.next().unwrap();
        assert!(core::ptr::eq(chunk.node, s.pages.first));
        assert_eq!(0_usize, chunk.start as usize);
        assert_eq!(s.rows as usize, chunk.end as usize);
    }

    // Should only have one chunk
    assert!(it.next().is_none());

    s.deinit();
}

// ref: test "PageList pageIterator reverse two pages"
#[test]
fn pagelist_pageIterator_reverse_two_pages() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

    // Grow to capacity
    let page1_node = s.pages.last;
    let (page1_cap_rows, page1_size_rows) = {
        let page1 = unsafe { &(*crate::pagelist::Node::page(page1_node)) };
        (page1.capacity.rows, page1.size.rows)
    };
    unsafe { (*crate::pagelist::Node::page(page1_node)).pause_integrity_checks(true) };
    for _ in 0..(page1_cap_rows - page1_size_rows) {
        assert!(s.grow().unwrap().is_none());
    }
    unsafe { (*crate::pagelist::Node::page(page1_node)).pause_integrity_checks(false) };
    assert!(s.grow().unwrap().is_some());

    // Iterate the active area
    let mut it = s.page_iterator(
        crate::pagelist::iter::Direction::LeftUp,
        point::Point::Active(point::Coordinate::default()),
        None,
    );
    let mut count: usize = 0;
    {
        let chunk = it.next().unwrap();
        assert!(core::ptr::eq(chunk.node, s.pages.last));
        let start: size::CellCountInt = 0;
        assert_eq!(start, chunk.start);
        assert_eq!(start + 1, chunk.end);
        count += (chunk.end - chunk.start) as usize;
    }
    {
        let chunk = it.next().unwrap();
        assert!(core::ptr::eq(chunk.node, s.pages.first));
        let start = unsafe { crate::pagelist::Node::rows(chunk.node) } - s.rows + 1;
        assert_eq!(start, chunk.start);
        assert_eq!(
            unsafe { crate::pagelist::Node::rows(chunk.node) },
            chunk.end
        );
        count += (chunk.end - chunk.start) as usize;
    }
    assert!(it.next().is_none());
    assert_eq!(s.rows as usize, count);

    s.deinit();
}

// ref: test "PageList pageIterator reverse history two pages"
#[test]
fn pagelist_pageIterator_reverse_history_two_pages() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

    // Grow to capacity
    let page1_node = s.pages.last;
    let (page1_cap_rows, page1_size_rows) = {
        let page1 = unsafe { &(*crate::pagelist::Node::page(page1_node)) };
        (page1.capacity.rows, page1.size.rows)
    };
    unsafe { (*crate::pagelist::Node::page(page1_node)).pause_integrity_checks(true) };
    for _ in 0..(page1_cap_rows - page1_size_rows) {
        assert!(s.grow().unwrap().is_none());
    }
    unsafe { (*crate::pagelist::Node::page(page1_node)).pause_integrity_checks(false) };
    assert!(s.grow().unwrap().is_some());

    // Iterate the active area
    let mut it = s.page_iterator(
        crate::pagelist::iter::Direction::LeftUp,
        point::Point::History(point::Coordinate::default()),
        None,
    );
    {
        let active_tl = s.get_top_left(point::Tag::Active);
        let chunk = it.next().unwrap();
        assert!(core::ptr::eq(chunk.node, s.pages.first));
        let start: size::CellCountInt = 0;
        assert_eq!(start, chunk.start);
        assert_eq!(active_tl.y, chunk.end);
    }
    assert!(it.next().is_none());

    s.deinit();
}

// ref: test "PageList cellIterator"
#[test]
fn pagelist_cellIterator() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(2, 2)
        },
    )
    .unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
    for y in 0..s.rows as usize {
        for x in 0..s.cols as usize {
            let rac = unsafe { (*page).get_row_and_cell(x, y) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }

    let mut it = s.cell_iterator(
        crate::pagelist::iter::Direction::RightDown,
        point::Point::Screen(point::Coordinate::default()),
        None,
    );
    {
        let p = it.next().unwrap();
        assert_eq!(
            point::Point::Screen(point::Coordinate { x: 0, y: 0 }),
            s.point_from_pin(point::Tag::Screen, p).unwrap()
        );
    }
    {
        let p = it.next().unwrap();
        assert_eq!(
            point::Point::Screen(point::Coordinate { x: 1, y: 0 }),
            s.point_from_pin(point::Tag::Screen, p).unwrap()
        );
    }
    {
        let p = it.next().unwrap();
        assert_eq!(
            point::Point::Screen(point::Coordinate { x: 0, y: 1 }),
            s.point_from_pin(point::Tag::Screen, p).unwrap()
        );
    }
    {
        let p = it.next().unwrap();
        assert_eq!(
            point::Point::Screen(point::Coordinate { x: 1, y: 1 }),
            s.point_from_pin(point::Tag::Screen, p).unwrap()
        );
    }
    assert!(it.next().is_none());

    s.deinit();
}

// ref: test "PageList cellIterator reverse"
#[test]
fn pagelist_cellIterator_reverse() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(2, 2)
        },
    )
    .unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
    for y in 0..s.rows as usize {
        for x in 0..s.cols as usize {
            let rac = unsafe { (*page).get_row_and_cell(x, y) };
            unsafe {
                *rac.cell = cell_codepoint(x as u32);
            }
        }
    }

    let mut it = s.cell_iterator(
        crate::pagelist::iter::Direction::LeftUp,
        point::Point::Screen(point::Coordinate::default()),
        None,
    );
    {
        let p = it.next().unwrap();
        assert_eq!(
            point::Point::Screen(point::Coordinate { x: 1, y: 1 }),
            s.point_from_pin(point::Tag::Screen, p).unwrap()
        );
    }
    {
        let p = it.next().unwrap();
        assert_eq!(
            point::Point::Screen(point::Coordinate { x: 0, y: 1 }),
            s.point_from_pin(point::Tag::Screen, p).unwrap()
        );
    }
    {
        let p = it.next().unwrap();
        assert_eq!(
            point::Point::Screen(point::Coordinate { x: 1, y: 0 }),
            s.point_from_pin(point::Tag::Screen, p).unwrap()
        );
    }
    {
        let p = it.next().unwrap();
        assert_eq!(
            point::Point::Screen(point::Coordinate { x: 0, y: 0 }),
            s.point_from_pin(point::Tag::Screen, p).unwrap()
        );
    }
    assert!(it.next().is_none());

    s.deinit();
}

// ref: test "PageList promptIterator left_up"
#[test]
fn pagelist_promptIterator_left_up() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(2, 20)
        },
    )
    .unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
    // Normal prompt
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 3) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }
    // Continuation
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 6) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 7) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::PromptContinuation) };
    }
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 8) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::PromptContinuation) };
    }
    // Broken continuation that has non-prompts in between
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 12) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::PromptContinuation) };
    }

    let mut it = s.prompt_iterator(
        crate::pagelist::iter::Direction::LeftUp,
        point::Point::Screen(point::Coordinate::default()),
        None,
    );
    {
        let p = it.next().unwrap();
        assert_eq!(
            point::Point::Screen(point::Coordinate { x: 0, y: 12 }),
            s.point_from_pin(point::Tag::Screen, p).unwrap()
        );
    }
    {
        let p = it.next().unwrap();
        assert_eq!(
            point::Point::Screen(point::Coordinate { x: 0, y: 6 }),
            s.point_from_pin(point::Tag::Screen, p).unwrap()
        );
    }
    {
        let p = it.next().unwrap();
        assert_eq!(
            point::Point::Screen(point::Coordinate { x: 0, y: 3 }),
            s.point_from_pin(point::Tag::Screen, p).unwrap()
        );
    }
    assert!(it.next().is_none());

    s.deinit();
}

// ref: test "PageList promptIterator right_down"
#[test]
fn pagelist_promptIterator_right_down() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(2, 20)
        },
    )
    .unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
    // Normal prompt
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 3) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }
    // Continuation (prompt on row 6, continuation on rows 7-8)
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 6) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 7) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::PromptContinuation) };
    }
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 8) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::PromptContinuation) };
    }
    // Broken continuation that has non-prompts in between (orphaned continuation at row 12)
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 12) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::PromptContinuation) };
    }

    let mut it = s.prompt_iterator(
        crate::pagelist::iter::Direction::RightDown,
        point::Point::Screen(point::Coordinate::default()),
        None,
    );
    {
        let p = it.next().unwrap();
        assert_eq!(
            point::Point::Screen(point::Coordinate { x: 0, y: 3 }),
            s.point_from_pin(point::Tag::Screen, p).unwrap()
        );
    }
    {
        let p = it.next().unwrap();
        assert_eq!(
            point::Point::Screen(point::Coordinate { x: 0, y: 6 }),
            s.point_from_pin(point::Tag::Screen, p).unwrap()
        );
    }
    {
        let p = it.next().unwrap();
        assert_eq!(
            point::Point::Screen(point::Coordinate { x: 0, y: 12 }),
            s.point_from_pin(point::Tag::Screen, p).unwrap()
        );
    }
    assert!(it.next().is_none());

    s.deinit();
}

// ref: test "PageList promptIterator right_down continuation at start"
#[test]
fn pagelist_promptIterator_right_down_continuation_at_start() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(2, 20)
        },
    )
    .unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

    // Prompt continuation at row 0 (no prior rows - simulates trimmed scrollback)
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 0) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::PromptContinuation) };
    }
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 1) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::PromptContinuation) };
    }
    // Normal prompt later
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 5) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }

    let mut it = s.prompt_iterator(
        crate::pagelist::iter::Direction::RightDown,
        point::Point::Screen(point::Coordinate::default()),
        None,
    );
    {
        // Should return the first continuation line since there's no prior prompt
        let p = it.next().unwrap();
        assert_eq!(
            point::Point::Screen(point::Coordinate { x: 0, y: 0 }),
            s.point_from_pin(point::Tag::Screen, p).unwrap()
        );
    }
    {
        let p = it.next().unwrap();
        assert_eq!(
            point::Point::Screen(point::Coordinate { x: 0, y: 5 }),
            s.point_from_pin(point::Tag::Screen, p).unwrap()
        );
    }
    assert!(it.next().is_none());

    s.deinit();
}

// ref: test "PageList promptIterator right_down with prompt before continuation"
#[test]
fn pagelist_promptIterator_right_down_with_prompt_before_continuation() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(2, 20)
        },
    )
    .unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

    // Prompt on row 2, continuation on rows 3-4
    // Starting iteration from row 3 should still find the prompt at row 2
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 2) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 3) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::PromptContinuation) };
    }
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 4) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::PromptContinuation) };
    }

    // Start iteration from row 3 (middle of the continuation)
    // Since we start on a continuation line, we treat it as the prompt start
    // (handles case where scrollback pruned the actual prompt)
    let mut it = s.prompt_iterator(
        crate::pagelist::iter::Direction::RightDown,
        point::Point::Screen(point::Coordinate { x: 0, y: 3 }),
        None,
    );
    {
        let p = it.next().unwrap();
        // Returns row 3 since that's the first prompt-related line we encounter
        assert_eq!(
            point::Point::Screen(point::Coordinate { x: 0, y: 3 }),
            s.point_from_pin(point::Tag::Screen, p).unwrap()
        );
    }
    assert!(it.next().is_none());

    s.deinit();
}

// ref: test "PageList promptIterator right_down limit inclusive"
#[test]
fn pagelist_promptIterator_right_down_limit_inclusive() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(2, 20)
        },
    )
    .unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

    // Prompt on row 5
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 5) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }
    // Prompt on row 10
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 10) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }

    // Iterate with limit at row 5 (the prompt row) - should include it
    let mut it = s.prompt_iterator(
        crate::pagelist::iter::Direction::RightDown,
        point::Point::Screen(point::Coordinate::default()),
        Some(point::Point::Screen(point::Coordinate { x: 0, y: 5 })),
    );
    {
        let p = it.next().unwrap();
        assert_eq!(
            point::Point::Screen(point::Coordinate { x: 0, y: 5 }),
            s.point_from_pin(point::Tag::Screen, p).unwrap()
        );
    }
    assert!(it.next().is_none());

    s.deinit();
}

// ref: test "PageList promptIterator left_up limit inclusive"
#[test]
fn pagelist_promptIterator_left_up_limit_inclusive() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(2, 20)
        },
    )
    .unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

    // Prompt on row 5
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 5) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }
    // Prompt on row 10
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 10) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }

    // Iterate with limit at row 10 (the prompt row) - should include it
    // tl_pt is the limit (upper bound), bl_pt is the start point for left_up
    let mut it = s.prompt_iterator(
        crate::pagelist::iter::Direction::LeftUp,
        point::Point::Screen(point::Coordinate { x: 0, y: 10 }),
        Some(point::Point::Screen(point::Coordinate { x: 0, y: 15 })),
    );
    {
        let p = it.next().unwrap();
        assert_eq!(
            point::Point::Screen(point::Coordinate { x: 0, y: 10 }),
            s.point_from_pin(point::Tag::Screen, p).unwrap()
        );
    }
    assert!(it.next().is_none());

    s.deinit();
}

// ref: test "PageList highlightSemanticContent prompt"
#[test]
fn pagelist_highlightSemanticContent_prompt() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 20)
        },
    )
    .unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

    // Prompt on row 5
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 5) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };

        // Start the prompt for the first 5 cols
        for x in 0..5 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'A' as u32, SemanticContent::Prompt);
            }
        }

        // Next 3 let's make input
        for x in 5..8 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'B' as u32, SemanticContent::Input);
            }
        }
    }
    // Prompt on row 10
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 10) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }

    let hl = s
        .highlight_semantic_content(
            s.pin(point::Point::Screen(point::Coordinate { x: 2, y: 5 }))
                .unwrap(),
            SemanticContent::Prompt,
        )
        .unwrap();
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 0, y: 5 }),
        s.point_from_pin(point::Tag::Screen, hl.start).unwrap()
    );
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 7, y: 5 }),
        s.point_from_pin(point::Tag::Screen, hl.end).unwrap()
    );

    s.deinit();
}

// ref: test "PageList highlightSemanticContent prompt with output"
#[test]
fn pagelist_highlightSemanticContent_prompt_with_output() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 20)
        },
    )
    .unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

    // Prompt on row 5
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 5) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };

        // First 3 cols are prompt
        for x in 0..3 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'$' as u32, SemanticContent::Prompt);
            }
        }

        // Next 4 are input
        for x in 3..7 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'l' as u32, SemanticContent::Input);
            }
        }

        // Rest is output (shouldn't be included in prompt highlight)
        for x in 7..10 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'o' as u32, SemanticContent::Output);
            }
        }
    }
    // Prompt on row 10
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 10) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }

    // Highlighting from prompt should include prompt and input, but stop at output
    let hl = s
        .highlight_semantic_content(
            s.pin(point::Point::Screen(point::Coordinate { x: 0, y: 5 }))
                .unwrap(),
            SemanticContent::Prompt,
        )
        .unwrap();
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 0, y: 5 }),
        s.point_from_pin(point::Tag::Screen, hl.start).unwrap()
    );
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 6, y: 5 }),
        s.point_from_pin(point::Tag::Screen, hl.end).unwrap()
    );

    s.deinit();
}

// ref: test "PageList highlightSemanticContent prompt multiline"
#[test]
fn pagelist_highlightSemanticContent_prompt_multiline() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 20)
        },
    )
    .unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

    // Prompt starts on row 5
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 5) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };

        // First row is all prompt
        for x in 0..10 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'$' as u32, SemanticContent::Prompt);
            }
        }
    }
    // Row 6 continues with input
    {
        for x in 0..5 {
            let cell = unsafe { (*page).get_row_and_cell(x, 6) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'c' as u32, SemanticContent::Input);
            }
        }
    }
    // Prompt on row 10
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 10) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }

    // Highlighting should span both rows
    let hl = s
        .highlight_semantic_content(
            s.pin(point::Point::Screen(point::Coordinate { x: 2, y: 5 }))
                .unwrap(),
            SemanticContent::Prompt,
        )
        .unwrap();
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 0, y: 5 }),
        s.point_from_pin(point::Tag::Screen, hl.start).unwrap()
    );
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 4, y: 6 }),
        s.point_from_pin(point::Tag::Screen, hl.end).unwrap()
    );

    s.deinit();
}

// ref: test "PageList highlightSemanticContent prompt only"
#[test]
fn pagelist_highlightSemanticContent_prompt_only() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 20)
        },
    )
    .unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

    // Prompt on row 5 with only prompt content (no input)
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 5) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };

        for x in 0..5 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'$' as u32, SemanticContent::Prompt);
            }
        }
    }
    // Prompt on row 10
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 10) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }

    // Highlighting should only include the prompt cells
    let hl = s
        .highlight_semantic_content(
            s.pin(point::Point::Screen(point::Coordinate { x: 0, y: 5 }))
                .unwrap(),
            SemanticContent::Prompt,
        )
        .unwrap();
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 0, y: 5 }),
        s.point_from_pin(point::Tag::Screen, hl.start).unwrap()
    );
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 4, y: 5 }),
        s.point_from_pin(point::Tag::Screen, hl.end).unwrap()
    );

    s.deinit();
}

// ref: test "PageList highlightSemanticContent prompt to end of screen"
#[test]
fn pagelist_highlightSemanticContent_prompt_to_end_of_screen() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 20)
        },
    )
    .unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

    // Single prompt on row 15, no following prompt
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 15) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };

        for x in 0..3 {
            let cell = unsafe { (*page).get_row_and_cell(x, 15) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'$' as u32, SemanticContent::Prompt);
            }
        }

        for x in 3..8 {
            let cell = unsafe { (*page).get_row_and_cell(x, 15) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'c' as u32, SemanticContent::Input);
            }
        }
    }

    // Highlighting should include prompt and input up to column 7
    let hl = s
        .highlight_semantic_content(
            s.pin(point::Point::Screen(point::Coordinate { x: 0, y: 15 }))
                .unwrap(),
            SemanticContent::Prompt,
        )
        .unwrap();
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 0, y: 15 }),
        s.point_from_pin(point::Tag::Screen, hl.start).unwrap()
    );
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 7, y: 15 }),
        s.point_from_pin(point::Tag::Screen, hl.end).unwrap()
    );

    s.deinit();
}

// ref: test "PageList highlightSemanticContent input basic"
#[test]
fn pagelist_highlightSemanticContent_input_basic() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 20)
        },
    )
    .unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

    // Prompt on row 5
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 5) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };

        // First 3 cols are prompt
        for x in 0..3 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'$' as u32, SemanticContent::Prompt);
            }
        }

        // Next 5 are input
        for x in 3..8 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'l' as u32, SemanticContent::Input);
            }
        }
    }
    // Prompt on row 10
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 10) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }

    // Highlighting input should only include input cells
    let hl = s
        .highlight_semantic_content(
            s.pin(point::Point::Screen(point::Coordinate { x: 0, y: 5 }))
                .unwrap(),
            SemanticContent::Input,
        )
        .unwrap();
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 3, y: 5 }),
        s.point_from_pin(point::Tag::Screen, hl.start).unwrap()
    );
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 7, y: 5 }),
        s.point_from_pin(point::Tag::Screen, hl.end).unwrap()
    );

    s.deinit();
}

// ref: test "PageList highlightSemanticContent input with output"
#[test]
fn pagelist_highlightSemanticContent_input_with_output() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 20)
        },
    )
    .unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

    // Prompt on row 5
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 5) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };

        // First 2 cols are prompt
        for x in 0..2 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'$' as u32, SemanticContent::Prompt);
            }
        }

        // Next 3 are input
        for x in 2..5 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'c' as u32, SemanticContent::Input);
            }
        }

        // Rest is output
        for x in 5..10 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'o' as u32, SemanticContent::Output);
            }
        }
    }
    // Prompt on row 10
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 10) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }

    // Highlighting input should stop at output
    let hl = s
        .highlight_semantic_content(
            s.pin(point::Point::Screen(point::Coordinate { x: 0, y: 5 }))
                .unwrap(),
            SemanticContent::Input,
        )
        .unwrap();
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 2, y: 5 }),
        s.point_from_pin(point::Tag::Screen, hl.start).unwrap()
    );
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 4, y: 5 }),
        s.point_from_pin(point::Tag::Screen, hl.end).unwrap()
    );

    s.deinit();
}

// ref: test "PageList highlightSemanticContent input multiline with continuation"
#[test]
fn pagelist_highlightSemanticContent_input_multiline_with_continuation() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 20)
        },
    )
    .unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

    // Prompt on row 5
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 5) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };

        // First 2 cols are prompt
        for x in 0..2 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'$' as u32, SemanticContent::Prompt);
            }
        }

        // Rest is input
        for x in 2..10 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'c' as u32, SemanticContent::Input);
            }
        }
    }
    // Row 6 has continuation prompt then more input
    {
        // Continuation prompt
        for x in 0..2 {
            let cell = unsafe { (*page).get_row_and_cell(x, 6) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'>' as u32, SemanticContent::Prompt);
            }
        }

        // More input
        for x in 2..6 {
            let cell = unsafe { (*page).get_row_and_cell(x, 6) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'd' as u32, SemanticContent::Input);
            }
        }
    }
    // Prompt on row 10
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 10) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }

    // Highlighting input should span both rows, skipping continuation prompts
    let hl = s
        .highlight_semantic_content(
            s.pin(point::Point::Screen(point::Coordinate { x: 0, y: 5 }))
                .unwrap(),
            SemanticContent::Input,
        )
        .unwrap();
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 2, y: 5 }),
        s.point_from_pin(point::Tag::Screen, hl.start).unwrap()
    );
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 5, y: 6 }),
        s.point_from_pin(point::Tag::Screen, hl.end).unwrap()
    );

    s.deinit();
}

// ref: test "PageList highlightSemanticContent input no input returns null"
#[test]
fn pagelist_highlightSemanticContent_input_no_input_returns_null() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 20)
        },
    )
    .unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

    // Prompt on row 5 with only prompt, then immediately output
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 5) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };

        // First 3 cols are prompt
        for x in 0..3 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'$' as u32, SemanticContent::Prompt);
            }
        }

        // Rest is output (no input!)
        for x in 3..10 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'o' as u32, SemanticContent::Output);
            }
        }
    }
    // Prompt on row 10
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 10) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }

    // Highlighting input should return null when there's no input
    let hl = s.highlight_semantic_content(
        s.pin(point::Point::Screen(point::Coordinate { x: 0, y: 5 }))
            .unwrap(),
        SemanticContent::Input,
    );
    assert!(hl.is_none());

    s.deinit();
}

// ref: test "PageList highlightSemanticContent input to end of screen"
#[test]
fn pagelist_highlightSemanticContent_input_to_end_of_screen() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 20)
        },
    )
    .unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

    // Single prompt on row 15, no following prompt
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 15) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };

        for x in 0..2 {
            let cell = unsafe { (*page).get_row_and_cell(x, 15) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'$' as u32, SemanticContent::Prompt);
            }
        }

        for x in 2..7 {
            let cell = unsafe { (*page).get_row_and_cell(x, 15) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'c' as u32, SemanticContent::Input);
            }
        }
    }

    // Highlighting input with no following prompt
    let hl = s
        .highlight_semantic_content(
            s.pin(point::Point::Screen(point::Coordinate { x: 0, y: 15 }))
                .unwrap(),
            SemanticContent::Input,
        )
        .unwrap();
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 2, y: 15 }),
        s.point_from_pin(point::Tag::Screen, hl.start).unwrap()
    );
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 6, y: 15 }),
        s.point_from_pin(point::Tag::Screen, hl.end).unwrap()
    );

    s.deinit();
}

// ref: test "PageList highlightSemanticContent input prompt only returns null"
#[test]
fn pagelist_highlightSemanticContent_input_prompt_only_returns_null() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 20)
        },
    )
    .unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

    // Prompt on row 5 with only prompt content, no input or output
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 5) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };

        // All cells are prompt
        for x in 0..10 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'$' as u32, SemanticContent::Prompt);
            }
        }
    }
    // Mark rows 6-9 as prompt to ensure no input before next prompt
    {
        for y in 6..10 {
            for x in 0..10 {
                let cell = unsafe { (*page).get_row_and_cell(x, y) }.cell;
                unsafe { (*cell).set_semantic_content(SemanticContent::Prompt) };
            }
        }
    }
    // Prompt on row 10
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 10) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }

    // Highlighting input should return null when there's only prompts
    let hl = s.highlight_semantic_content(
        s.pin(point::Point::Screen(point::Coordinate { x: 0, y: 5 }))
            .unwrap(),
        SemanticContent::Input,
    );
    assert!(hl.is_none());

    s.deinit();
}

// ref: test "PageList highlightSemanticContent output basic"
#[test]
fn pagelist_highlightSemanticContent_output_basic() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 20)
        },
    )
    .unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

    // Prompt on row 5
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 5) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };

        // First 2 cols are prompt
        for x in 0..2 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'$' as u32, SemanticContent::Prompt);
            }
        }

        // Next 3 are input
        for x in 2..5 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'l' as u32, SemanticContent::Input);
            }
        }

        // Cols 5-7 are output
        for x in 5..8 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'o' as u32, SemanticContent::Output);
            }
        }

        // Mark remaining cells as prompt to bound the output
        for x in 8..10 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe { (*cell).set_semantic_content(SemanticContent::Prompt) };
        }
    }
    // Prompt on row 10
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 10) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }

    // Highlighting output should only include output cells
    let hl = s
        .highlight_semantic_content(
            s.pin(point::Point::Screen(point::Coordinate { x: 0, y: 5 }))
                .unwrap(),
            SemanticContent::Output,
        )
        .unwrap();
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 5, y: 5 }),
        s.point_from_pin(point::Tag::Screen, hl.start).unwrap()
    );
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 7, y: 5 }),
        s.point_from_pin(point::Tag::Screen, hl.end).unwrap()
    );

    s.deinit();
}

// ref: test "PageList highlightSemanticContent output multiline"
#[test]
fn pagelist_highlightSemanticContent_output_multiline() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 20)
        },
    )
    .unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

    // Prompt on row 5
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 5) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };

        // First 2 cols are prompt
        for x in 0..2 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'$' as u32, SemanticContent::Prompt);
            }
        }

        // Next 2 are input
        for x in 2..4 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'l' as u32, SemanticContent::Input);
            }
        }

        // Rest of row 5 is output
        for x in 4..10 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'o' as u32, SemanticContent::Output);
            }
        }
    }
    // Row 6 is all output
    {
        for x in 0..10 {
            let cell = unsafe { (*page).get_row_and_cell(x, 6) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'o' as u32, SemanticContent::Output);
            }
        }
    }
    // Row 7 has partial output then input to bound it
    {
        for x in 0..5 {
            let cell = unsafe { (*page).get_row_and_cell(x, 7) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'o' as u32, SemanticContent::Output);
            }
        }
        for x in 5..10 {
            let cell = unsafe { (*page).get_row_and_cell(x, 7) }.cell;
            unsafe { (*cell).set_semantic_content(SemanticContent::Input) };
        }
    }
    // Prompt on row 10
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 10) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }

    // Highlighting output should span multiple rows
    let hl = s
        .highlight_semantic_content(
            s.pin(point::Point::Screen(point::Coordinate { x: 0, y: 5 }))
                .unwrap(),
            SemanticContent::Output,
        )
        .unwrap();
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 4, y: 5 }),
        s.point_from_pin(point::Tag::Screen, hl.start).unwrap()
    );
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 4, y: 7 }),
        s.point_from_pin(point::Tag::Screen, hl.end).unwrap()
    );

    s.deinit();
}

// ref: test "PageList highlightSemanticContent output stops at next prompt"
#[test]
fn pagelist_highlightSemanticContent_output_stops_at_next_prompt() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 20)
        },
    )
    .unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

    // Prompt on row 5
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 5) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };

        // First 2 cols are prompt
        for x in 0..2 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'$' as u32, SemanticContent::Prompt);
            }
        }

        // Next 2 are input
        for x in 2..4 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'l' as u32, SemanticContent::Input);
            }
        }

        // Rest is output
        for x in 4..10 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'o' as u32, SemanticContent::Output);
            }
        }
    }
    // Row 6 has output then prompt starts
    {
        for x in 0..3 {
            let cell = unsafe { (*page).get_row_and_cell(x, 6) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'o' as u32, SemanticContent::Output);
            }
        }
        // Next prompt marker on same row
        for x in 3..6 {
            let cell = unsafe { (*page).get_row_and_cell(x, 6) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'$' as u32, SemanticContent::Prompt);
            }
        }
    }
    // Prompt on row 10
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 10) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }

    // Highlighting output should stop before prompt/input
    let hl = s
        .highlight_semantic_content(
            s.pin(point::Point::Screen(point::Coordinate { x: 0, y: 5 }))
                .unwrap(),
            SemanticContent::Output,
        )
        .unwrap();
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 4, y: 5 }),
        s.point_from_pin(point::Tag::Screen, hl.start).unwrap()
    );
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 2, y: 6 }),
        s.point_from_pin(point::Tag::Screen, hl.end).unwrap()
    );

    s.deinit();
}

// ref: test "PageList highlightSemanticContent output to end of screen"
#[test]
fn pagelist_highlightSemanticContent_output_to_end_of_screen() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 20)
        },
    )
    .unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

    // Single prompt on row 15, no following prompt
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 15) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };

        for x in 0..2 {
            let cell = unsafe { (*page).get_row_and_cell(x, 15) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'$' as u32, SemanticContent::Prompt);
            }
        }

        for x in 2..4 {
            let cell = unsafe { (*page).get_row_and_cell(x, 15) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'c' as u32, SemanticContent::Input);
            }
        }

        for x in 4..10 {
            let cell = unsafe { (*page).get_row_and_cell(x, 15) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'o' as u32, SemanticContent::Output);
            }
        }
    }
    // Row 16 has output then prompt to bound it
    {
        for x in 0..8 {
            let cell = unsafe { (*page).get_row_and_cell(x, 16) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'o' as u32, SemanticContent::Output);
            }
        }
        for x in 8..10 {
            let cell = unsafe { (*page).get_row_and_cell(x, 16) }.cell;
            unsafe { (*cell).set_semantic_content(SemanticContent::Prompt) };
        }
    }

    // Highlighting output with no following prompt
    let hl = s
        .highlight_semantic_content(
            s.pin(point::Point::Screen(point::Coordinate { x: 0, y: 15 }))
                .unwrap(),
            SemanticContent::Output,
        )
        .unwrap();
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 4, y: 15 }),
        s.point_from_pin(point::Tag::Screen, hl.start).unwrap()
    );
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 7, y: 16 }),
        s.point_from_pin(point::Tag::Screen, hl.end).unwrap()
    );

    s.deinit();
}

// ref: test "PageList highlightSemanticContent output no output returns null"
#[test]
fn pagelist_highlightSemanticContent_output_no_output_returns_null() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 20)
        },
    )
    .unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

    // Prompt on row 5 with only prompt and input, no output
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 5) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };

        // First 3 cols are prompt
        for x in 0..3 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'$' as u32, SemanticContent::Prompt);
            }
        }

        // Rest is input (must explicitly mark all cells to avoid default .output)
        for x in 3..10 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'c' as u32, SemanticContent::Input);
            }
        }
    }
    // Mark rows 6-9 as input to ensure no output between prompts
    {
        for y in 6..10 {
            for x in 0..10 {
                let cell = unsafe { (*page).get_row_and_cell(x, y) }.cell;
                unsafe { (*cell).set_semantic_content(SemanticContent::Input) };
            }
        }
    }
    // Prompt on row 10 (no output between prompts)
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 10) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }

    // Highlighting output should return null when there's no output
    let hl = s.highlight_semantic_content(
        s.pin(point::Point::Screen(point::Coordinate { x: 0, y: 5 }))
            .unwrap(),
        SemanticContent::Output,
    );
    assert!(hl.is_none());

    s.deinit();
}

// ref: test "PageList highlightSemanticContent output skips empty cells"
#[test]
fn pagelist_highlightSemanticContent_output_skips_empty_cells() {
    // Tests that empty cells with default .output semantic content are
    // not selected as output. This can happen when a prompt/input line
    // doesn't fill the entire row - trailing cells have default .output.
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(10, 20)
        },
    )
    .unwrap();
    assert!(core::ptr::eq(s.pages.first, s.pages.last));
    let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

    // Prompt on row 5 - only fills first 3 cells, rest are empty with default .output
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 5) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };

        // First 3 cols are prompt with text
        for x in 0..3 {
            let cell = unsafe { (*page).get_row_and_cell(x, 5) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'$' as u32, SemanticContent::Prompt);
            }
        }
        // Cells 3-9 are empty (codepoint = 0) with default .output semantic content
        // This simulates what happens when a short prompt is written
    }

    // Row 6 has input (short, doesn't fill line)
    {
        for x in 0..4 {
            let cell = unsafe { (*page).get_row_and_cell(x, 6) }.cell;
            unsafe {
                *cell = cell_codepoint_sc(b'l' as u32, SemanticContent::Input);
            }
        }
        // Cells 4-9 are empty with default .output
    }

    // Row 7-8 have actual output with text
    {
        for y in 7..9 {
            for x in 0..5 {
                let cell = unsafe { (*page).get_row_and_cell(x, y) }.cell;
                unsafe {
                    *cell = cell_codepoint_sc(b'o' as u32, SemanticContent::Output);
                }
            }
        }
    }

    // Prompt on row 10
    {
        let rac = unsafe { (*page).get_row_and_cell(0, 10) };
        unsafe { (*rac.row).set_semantic_prompt(SemanticPrompt::Prompt) };
    }

    // Highlighting output should skip empty cells on rows 5-6 and find
    // the actual output starting at row 7
    let hl = s
        .highlight_semantic_content(
            s.pin(point::Point::Screen(point::Coordinate { x: 0, y: 5 }))
                .unwrap(),
            SemanticContent::Output,
        )
        .unwrap();
    // Output should start at row 7, not row 5 (where empty cells have default .output)
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 0, y: 7 }),
        s.point_from_pin(point::Tag::Screen, hl.start).unwrap()
    );
    assert_eq!(
        point::Point::Screen(point::Coordinate { x: 4, y: 8 }),
        s.point_from_pin(point::Tag::Screen, hl.end).unwrap()
    );

    s.deinit();
}

// ref: test "PageList erase"
#[test]
fn pagelist_erase() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
    assert_eq!(1_usize, s.total_pages());

    // Grow so we take up at least 5 pages.
    let page_cap_rows = unsafe { crate::pagelist::Node::capacity(s.pages.last).rows };
    let mut cur_page = s.pages.last;
    unsafe { (*crate::pagelist::Node::page(cur_page)).pause_integrity_checks(true) };
    for _ in 0..(page_cap_rows as usize * 5) {
        if let Some(new_page) = s.grow().unwrap() {
            unsafe { (*crate::pagelist::Node::page(cur_page)).pause_integrity_checks(false) };
            cur_page = new_page;
            unsafe { (*crate::pagelist::Node::page(cur_page)).pause_integrity_checks(true) };
        }
    }
    unsafe { (*crate::pagelist::Node::page(cur_page)).pause_integrity_checks(false) };
    assert_eq!(6_usize, s.total_pages());

    // Our total rows should be large
    assert!(s.total_rows > s.rows as usize);

    // Erase the entire history, we should be back to just our active set.
    s.erase_history(None);
    assert_eq!(s.rows as usize, s.total_rows);

    // We should be back to just one page
    assert_eq!(1_usize, s.total_pages());
    assert!(core::ptr::eq(s.pages.first, s.pages.last));

    s.deinit();
}

// ref: test "PageList erase reaccounts page size"
#[test]
fn pagelist_erase_reaccounts_page_size() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
    let start_size = s.page_size;

    // Grow so we take up at least 5 pages.
    let page_cap_rows = unsafe { crate::pagelist::Node::capacity(s.pages.last).rows };
    let mut cur_page = s.pages.last;
    unsafe { (*crate::pagelist::Node::page(cur_page)).pause_integrity_checks(true) };
    for _ in 0..(page_cap_rows as usize * 5) {
        if let Some(new_page) = s.grow().unwrap() {
            unsafe { (*crate::pagelist::Node::page(cur_page)).pause_integrity_checks(false) };
            cur_page = new_page;
            unsafe { (*crate::pagelist::Node::page(cur_page)).pause_integrity_checks(true) };
        }
    }
    unsafe { (*crate::pagelist::Node::page(cur_page)).pause_integrity_checks(false) };
    assert!(s.page_size > start_size);

    // Erase the entire history, we should be back to just our active set.
    s.erase_history(None);
    assert_eq!(start_size, s.page_size);

    s.deinit();
}

// ref: test "PageList erase row with tracked pin resets to top-left"
#[test]
fn pagelist_erase_row_with_tracked_pin_resets_to_top_left() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

    // Grow so we take up at least 5 pages.
    let page_cap_rows = unsafe { crate::pagelist::Node::capacity(s.pages.last).rows };
    let mut cur_page = s.pages.last;
    unsafe { (*crate::pagelist::Node::page(cur_page)).pause_integrity_checks(true) };
    for _ in 0..(page_cap_rows as usize * 5) {
        if let Some(new_page) = s.grow().unwrap() {
            unsafe { (*crate::pagelist::Node::page(cur_page)).pause_integrity_checks(false) };
            cur_page = new_page;
            unsafe { (*crate::pagelist::Node::page(cur_page)).pause_integrity_checks(true) };
        }
    }
    unsafe { (*crate::pagelist::Node::page(cur_page)).pause_integrity_checks(false) };

    // Our total rows should be large
    assert!(s.total_rows > s.rows as usize);

    // Put a tracked pin in the history
    let p = s
        .track_pin(
            s.pin(point::Point::History(point::Coordinate { x: 0, y: 0 }))
                .unwrap(),
        )
        .unwrap();

    // Erase the entire history, we should be back to just our active set.
    s.erase_history(None);
    assert_eq!(s.rows as usize, s.total_rows);

    // Our pin should move to the first page
    assert!(core::ptr::eq(s.pages.first, unsafe { (*p).node }));
    assert_eq!(0 as size::CellCountInt, unsafe { (*p).y });
    assert_eq!(0 as size::CellCountInt, unsafe { (*p).x });

    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList erase row with tracked pin shifts"
#[test]
fn pagelist_erase_row_with_tracked_pin_shifts() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

    // Put a tracked pin in the history
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { y: 4, x: 2 }))
                .unwrap(),
        )
        .unwrap();

    // Erase only a few rows in our active
    s.erase_active(3);
    assert_eq!(s.rows as usize, s.total_rows);

    // Our pin should move to the first page
    assert!(core::ptr::eq(s.pages.first, unsafe { (*p).node }));
    assert_eq!(0 as size::CellCountInt, unsafe { (*p).y });
    assert_eq!(2 as size::CellCountInt, unsafe { (*p).x });

    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList erase row with tracked pin is erased"
#[test]
fn pagelist_erase_row_with_tracked_pin_is_erased() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

    // Put a tracked pin in the history
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { y: 2, x: 2 }))
                .unwrap(),
        )
        .unwrap();

    // Erase the entire history, we should be back to just our active set.
    s.erase_active(3);
    assert_eq!(s.rows as usize, s.total_rows);

    // Our pin should move to the first page
    assert!(core::ptr::eq(s.pages.first, unsafe { (*p).node }));
    assert_eq!(0 as size::CellCountInt, unsafe { (*p).y });
    assert_eq!(0 as size::CellCountInt, unsafe { (*p).x });

    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList erase resets viewport to active if moves within active"
#[test]
fn pagelist_erase_resets_viewport_to_active_if_moves_within_active() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

    // Grow so we take up at least 5 pages.
    let page_cap_rows = unsafe { crate::pagelist::Node::capacity(s.pages.last).rows };
    let mut cur_page = s.pages.last;
    unsafe { (*crate::pagelist::Node::page(cur_page)).pause_integrity_checks(true) };
    for _ in 0..(page_cap_rows as usize * 5) {
        if let Some(new_page) = s.grow().unwrap() {
            unsafe { (*crate::pagelist::Node::page(cur_page)).pause_integrity_checks(false) };
            cur_page = new_page;
            unsafe { (*crate::pagelist::Node::page(cur_page)).pause_integrity_checks(true) };
        }
    }
    unsafe { (*crate::pagelist::Node::page(cur_page)).pause_integrity_checks(false) };

    // Move our viewport to the top
    s.scroll(Scroll::DeltaRow(-(s.total_rows as isize)));
    assert!(s.viewport == crate::pagelist::Viewport::Top);

    // Erase the entire history, we should be back to just our active set.
    s.erase_history(None);
    assert!(s.viewport == crate::pagelist::Viewport::Active);

    s.deinit();
}

// ref: test "PageList erase resets viewport if inside erased page but not active"
#[test]
fn pagelist_erase_resets_viewport_if_inside_erased_page_but_not_active() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

    // Grow so we take up at least 5 pages.
    let page_cap_rows = unsafe { crate::pagelist::Node::capacity(s.pages.last).rows };
    let mut cur_page = s.pages.last;
    unsafe { (*crate::pagelist::Node::page(cur_page)).pause_integrity_checks(true) };
    for _ in 0..(page_cap_rows as usize * 5) {
        if let Some(new_page) = s.grow().unwrap() {
            unsafe { (*crate::pagelist::Node::page(cur_page)).pause_integrity_checks(false) };
            cur_page = new_page;
            unsafe { (*crate::pagelist::Node::page(cur_page)).pause_integrity_checks(true) };
        }
    }
    unsafe { (*crate::pagelist::Node::page(cur_page)).pause_integrity_checks(false) };

    // Move our viewport to the top
    s.scroll(Scroll::DeltaRow(-(s.total_rows as isize)));
    assert!(s.viewport == crate::pagelist::Viewport::Top);

    // Erase the entire history, we should be back to just our active set.
    s.erase_history(Some(point::Point::History(point::Coordinate {
        x: 0,
        y: 2,
    })));
    assert!(s.viewport == crate::pagelist::Viewport::Top);

    s.deinit();
}

// ref: test "PageList erase resets viewport to active if top is inside active"
#[test]
fn pagelist_erase_resets_viewport_to_active_if_top_is_inside_active() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();

    // Grow so we take up at least 5 pages.
    let page_cap_rows = unsafe { crate::pagelist::Node::capacity(s.pages.last).rows };
    let mut cur_page = s.pages.last;
    unsafe { (*crate::pagelist::Node::page(cur_page)).pause_integrity_checks(true) };
    for _ in 0..(page_cap_rows as usize * 5) {
        if let Some(new_page) = s.grow().unwrap() {
            unsafe { (*crate::pagelist::Node::page(cur_page)).pause_integrity_checks(false) };
            cur_page = new_page;
            unsafe { (*crate::pagelist::Node::page(cur_page)).pause_integrity_checks(true) };
        }
    }
    unsafe { (*crate::pagelist::Node::page(cur_page)).pause_integrity_checks(false) };

    // Move our viewport to the top
    s.scroll(Scroll::Top);

    // Erase the entire history, we should be back to just our active set.
    s.erase_history(None);
    assert!(s.viewport == crate::pagelist::Viewport::Active);

    s.deinit();
}

// ref: test "PageList erase active regrows automatically"
#[test]
fn pagelist_erase_active_regrows_automatically() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
    assert!(s.total_rows() == s.rows as usize);
    s.erase_active(10);
    assert!(s.total_rows() == s.rows as usize);

    s.deinit();
}

// S124-F1: when regrowing after an active erase fails, the reference logs
// `log.err("failed to regrow active area after erase err={}", .{err})`
// (PageList.zig:5580) and gives up silently rather than propagating the
// error (`erase_rows`/`erase_active` return no `Result`). Forced via the
// SAME `GrowFp::CreatePage` fail point
// `pagelist_resize_no_reflow_grow_rows_oom_restores_limits` uses: fill the
// only page to capacity first so `grow()`'s one fallible allocation is the
// very next call.
#[test]
fn pagelist_erase_active_logs_when_regrow_fails() {
    use crate::pagelist::grow::{GrowFp, GROW_TW};
    use rustty_util::tripwire::ResetMode;

    // Erasing active rows physically shrinks whichever page(s) they lived
    // on, so a regrow ordinarily just reclaims the freed capacity in place
    // — never touching `grow()`'s allocating path at all. To reach
    // `GrowFp::CreatePage`, the erase must remove rows from a DIFFERENT node
    // than the one `grow()` tops back up (always the LAST node): make the
    // active area exactly two full, zero-slack pages (`rows` == exactly
    // `2 * initial_capacity(cols).rows`), then erase exactly the first page.
    // That fully removes the FIRST node (freeing nothing on the last one),
    // so the regrow must extend the last page — which is already at its own
    // full capacity — forcing `grow()` to allocate a brand-new page node.
    let page_cap = PageList::initial_capacity(80).rows;
    let mut s = PageList::init(Global, Options::new(80, page_cap * 2)).unwrap();
    assert!(s.total_rows() == s.rows as usize);
    assert_eq!(2usize, s.total_pages());
    assert_eq!(page_cap, unsafe {
        crate::pagelist::Node::capacity(s.pages.last).rows
    });
    assert_eq!(page_cap, unsafe {
        crate::pagelist::Node::rows(s.pages.last)
    });
    let rows_before = s.total_rows();

    GROW_TW.with(|tw| tw.borrow_mut().errorAlways(GrowFp::CreatePage, OutOfMemory));
    let cap = crate::log_capture::start();
    s.erase_active(page_cap - 1);
    let messages = cap.messages();
    assert!(
        messages
            .iter()
            .any(|(level, target, msg)| *level == log::Level::Error
                && target == "page_list"
                && msg.starts_with("failed to regrow active area after erase err=")),
        "expected the failed-to-regrow-after-erase diagnostic, got: {messages:?}"
    );
    drop(cap);
    GROW_TW.with(|tw| tw.borrow_mut().end(ResetMode::Reset).unwrap());

    // The regrow gave up silently: active area stays short by the erased
    // rows rather than panicking or propagating an error.
    assert_eq!(rows_before - page_cap as usize, s.total_rows()); // erase_active(page_cap-1) erases `page_cap` rows inclusive

    s.deinit();
}

// ref: test "PageList erase a one-row active"
#[test]
fn pagelist_erase_a_one_row_active() {
    let mut s = PageList::init(Global, Options::new(10, 1)).unwrap();
    assert_eq!(1_usize, s.total_pages());

    // Write our letter
    {
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
        for y in 0..s.rows as usize {
            let rac = unsafe { (*page).get_row_and_cell(0, y) };
            unsafe {
                *rac.cell = cell_codepoint('A' as u32);
            }
        }
    }

    s.erase_active(0);
    assert_eq!(s.rows as usize, s.total_rows);

    // The row should be empty
    {
        let get = s
            .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
            .unwrap();
        assert_eq!(0_u32, unsafe { (*get.cell).content_codepoint() });
    }

    s.deinit();
}

// ref: test "PageList eraseRowBounded less than full row"
#[test]
fn pagelist_eraseRowBounded_less_than_full_row() {
    let mut s = PageList::init(Global, Options::new(80, 10)).unwrap();

    // Pins
    let p_top = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { y: 5, x: 0 }))
                .unwrap(),
        )
        .unwrap();
    let p_bot = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { y: 8, x: 0 }))
                .unwrap(),
        )
        .unwrap();
    let p_out = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { y: 9, x: 0 }))
                .unwrap(),
        )
        .unwrap();

    // Erase only a few rows in our active
    s.erase_row_bounded(point::Point::Active(point::Coordinate { x: 0, y: 5 }), 3)
        .unwrap();
    assert_eq!(s.rows as usize, s.total_rows());

    // The erased rows should be dirty
    assert!(s.is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 5 })));
    assert!(s.is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 6 })));
    assert!(s.is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 7 })));

    assert!(core::ptr::eq(s.pages.first, unsafe { (*p_top).node }));
    assert_eq!(4 as size::CellCountInt, unsafe { (*p_top).y });
    assert_eq!(0 as size::CellCountInt, unsafe { (*p_top).x });

    assert!(core::ptr::eq(s.pages.first, unsafe { (*p_bot).node }));
    assert_eq!(7 as size::CellCountInt, unsafe { (*p_bot).y });
    assert_eq!(0 as size::CellCountInt, unsafe { (*p_bot).x });

    assert!(core::ptr::eq(s.pages.first, unsafe { (*p_out).node }));
    assert_eq!(9 as size::CellCountInt, unsafe { (*p_out).y });
    assert_eq!(0 as size::CellCountInt, unsafe { (*p_out).x });

    s.untrack_pin(p_out);
    s.untrack_pin(p_bot);
    s.untrack_pin(p_top);
    s.deinit();
}

// ref: test "PageList eraseRowBounded with pin at top"
#[test]
fn pagelist_eraseRowBounded_with_pin_at_top() {
    let mut s = PageList::init(Global, Options::new(80, 10)).unwrap();

    // Pins
    let p_top = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { y: 0, x: 5 }))
                .unwrap(),
        )
        .unwrap();

    // Erase only a few rows in our active
    s.erase_row_bounded(point::Point::Active(point::Coordinate { x: 0, y: 0 }), 3)
        .unwrap();
    assert_eq!(s.rows as usize, s.total_rows());

    // The erased rows should be dirty
    assert!(s.is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 0 })));
    assert!(s.is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 1 })));
    assert!(s.is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 2 })));

    assert!(core::ptr::eq(s.pages.first, unsafe { (*p_top).node }));
    assert_eq!(0 as size::CellCountInt, unsafe { (*p_top).y });
    assert_eq!(0 as size::CellCountInt, unsafe { (*p_top).x });

    s.untrack_pin(p_top);
    s.deinit();
}

// ref: test "PageList eraseRowBounded full rows single page"
#[test]
fn pagelist_eraseRowBounded_full_rows_single_page() {
    let mut s = PageList::init(Global, Options::new(80, 10)).unwrap();

    // Pins
    let p_in = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { y: 7, x: 0 }))
                .unwrap(),
        )
        .unwrap();
    let p_out = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { y: 9, x: 0 }))
                .unwrap(),
        )
        .unwrap();

    // Erase only a few rows in our active
    s.erase_row_bounded(point::Point::Active(point::Coordinate { x: 0, y: 5 }), 10)
        .unwrap();
    assert_eq!(s.rows as usize, s.total_rows());

    // The erased rows should be dirty
    for y in 5..10 {
        assert!(s.is_dirty(point::Point::Active(point::Coordinate { x: 0, y })));
    }

    // Our pin should move to the first page
    assert!(core::ptr::eq(s.pages.first, unsafe { (*p_in).node }));
    assert_eq!(6 as size::CellCountInt, unsafe { (*p_in).y });
    assert_eq!(0 as size::CellCountInt, unsafe { (*p_in).x });

    assert!(core::ptr::eq(s.pages.first, unsafe { (*p_out).node }));
    assert_eq!(8 as size::CellCountInt, unsafe { (*p_out).y });
    assert_eq!(0 as size::CellCountInt, unsafe { (*p_out).x });

    s.untrack_pin(p_out);
    s.untrack_pin(p_in);
    s.deinit();
}

// ref: test "PageList eraseRowBounded full rows two pages"
#[test]
fn pagelist_eraseRowBounded_full_rows_two_pages() {
    let mut s = PageList::init(Global, Options::new(80, 10)).unwrap();

    // Grow to two pages so our active area straddles
    {
        let page = unsafe { crate::pagelist::Node::page(s.pages.last) };
        unsafe { (*page).pause_integrity_checks(true) };
        let to_grow =
            unsafe { (*page).capacity.rows } as usize - unsafe { (*page).size.rows } as usize;
        for _ in 0..to_grow {
            let _ = s.grow().unwrap();
        }
        unsafe { (*crate::pagelist::Node::page(s.pages.last)).pause_integrity_checks(false) };
        s.grow_rows(5).unwrap();
        assert_eq!(2_usize, s.total_pages());
        assert_eq!(5 as size::CellCountInt, unsafe {
            crate::pagelist::Node::rows(s.pages.last)
        });
    }

    // Pins
    let p_first = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { y: 4, x: 0 }))
                .unwrap(),
        )
        .unwrap();
    let p_first_out = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { y: 3, x: 0 }))
                .unwrap(),
        )
        .unwrap();
    let p_in = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { y: 8, x: 0 }))
                .unwrap(),
        )
        .unwrap();
    let p_out = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { y: 9, x: 0 }))
                .unwrap(),
        )
        .unwrap();

    {
        let prev = unsafe { (*s.pages.last).prev };
        assert!(core::ptr::eq(prev, unsafe { (*p_first).node }));
        let first_rows = unsafe { crate::pagelist::Node::rows((*p_first).node) };
        assert_eq!((first_rows - 1) as usize, unsafe { (*p_first).y } as usize);
        assert_eq!(0 as size::CellCountInt, unsafe { (*p_first).x });

        assert!(core::ptr::eq(prev, unsafe { (*p_first_out).node }));
        let first_out_rows = unsafe { crate::pagelist::Node::rows((*p_first_out).node) };
        assert_eq!((first_out_rows - 2) as usize, unsafe { (*p_first_out).y }
            as usize);
        assert_eq!(0 as size::CellCountInt, unsafe { (*p_first_out).x });

        assert!(core::ptr::eq(s.pages.last, unsafe { (*p_in).node }));
        assert_eq!(3 as size::CellCountInt, unsafe { (*p_in).y });
        assert_eq!(0 as size::CellCountInt, unsafe { (*p_in).x });

        assert!(core::ptr::eq(s.pages.last, unsafe { (*p_out).node }));
        assert_eq!(4 as size::CellCountInt, unsafe { (*p_out).y });
        assert_eq!(0 as size::CellCountInt, unsafe { (*p_out).x });
    }

    // Erase only a few rows in our active
    s.erase_row_bounded(point::Point::Active(point::Coordinate { x: 0, y: 4 }), 4)
        .unwrap();

    // The erased rows should be dirty
    for y in 4..8 {
        assert!(s.is_dirty(point::Point::Active(point::Coordinate { x: 0, y })));
    }

    // In page in first page is shifted
    let prev = unsafe { (*s.pages.last).prev };
    assert!(core::ptr::eq(prev, unsafe { (*p_first).node }));
    let first_rows = unsafe { crate::pagelist::Node::rows((*p_first).node) };
    assert_eq!((first_rows - 2) as usize, unsafe { (*p_first).y } as usize);
    assert_eq!(0 as size::CellCountInt, unsafe { (*p_first).x });

    // Out page in first page should not be shifted
    assert!(core::ptr::eq(prev, unsafe { (*p_first_out).node }));
    let first_out_rows = unsafe { crate::pagelist::Node::rows((*p_first_out).node) };
    assert_eq!((first_out_rows - 2) as usize, unsafe { (*p_first_out).y }
        as usize);
    assert_eq!(0 as size::CellCountInt, unsafe { (*p_first_out).x });

    // In page is shifted
    assert!(core::ptr::eq(s.pages.last, unsafe { (*p_in).node }));
    assert_eq!(2 as size::CellCountInt, unsafe { (*p_in).y });
    assert_eq!(0 as size::CellCountInt, unsafe { (*p_in).x });

    // Out page is not shifted
    assert!(core::ptr::eq(s.pages.last, unsafe { (*p_out).node }));
    assert_eq!(4 as size::CellCountInt, unsafe { (*p_out).y });
    assert_eq!(0 as size::CellCountInt, unsafe { (*p_out).x });

    s.untrack_pin(p_out);
    s.untrack_pin(p_in);
    s.untrack_pin(p_first_out);
    s.untrack_pin(p_first);
    s.deinit();
}

// ref: test "PageList clone"
#[test]
fn pagelist_clone() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
    assert_eq!(s.rows as usize, s.total_rows());

    let mut opts = crate::pagelist::access::Clone {
        top: point::Point::Screen(point::Coordinate { x: 0, y: 0 }),
        bot: None,
        tracked_pins: None,
    };
    let mut s2 = s.clone(Global, &mut opts).unwrap();
    assert_eq!(s.rows as usize, s2.total_rows());

    s2.deinit();
    s.deinit();
}

// ref: test "PageList clone partial trimmed right"
#[test]
fn pagelist_clone_partial_trimmed_right() {
    let mut s = PageList::init(Global, Options::new(80, 20)).unwrap();
    assert_eq!(s.rows as usize, s.total_rows());
    s.grow_rows(30).unwrap();

    let mut opts = crate::pagelist::access::Clone {
        top: point::Point::Screen(point::Coordinate { x: 0, y: 0 }),
        bot: Some(point::Point::Screen(point::Coordinate { x: 0, y: 39 })),
        tracked_pins: None,
    };
    let mut s2 = s.clone(Global, &mut opts).unwrap();
    assert_eq!(40usize, s2.total_rows());

    s2.deinit();
    s.deinit();
}

// ref: test "PageList clone partial trimmed left"
#[test]
fn pagelist_clone_partial_trimmed_left() {
    let mut s = PageList::init(Global, Options::new(80, 20)).unwrap();
    assert_eq!(s.rows as usize, s.total_rows());
    s.grow_rows(30).unwrap();

    let mut opts = crate::pagelist::access::Clone {
        top: point::Point::Screen(point::Coordinate { x: 0, y: 10 }),
        bot: None,
        tracked_pins: None,
    };
    let mut s2 = s.clone(Global, &mut opts).unwrap();
    assert_eq!(40usize, s2.total_rows());

    s2.deinit();
    s.deinit();
}

// ref: test "PageList clone partial trimmed left reclaims styles"
#[test]
fn pagelist_clone_partial_trimmed_left_reclaims_styles() {
    let mut s = PageList::init(Global, Options::new(80, 20)).unwrap();
    assert_eq!(s.rows as usize, s.total_rows());
    s.grow_rows(30).unwrap();

    // Style the rows we're trimming
    {
        assert!(core::ptr::eq(s.pages.first, s.pages.last));
        // PORT NOTE: `page` is a raw `*mut PageData` (the reference's
        // `&...first.?.data` is a Zig pointer). A real `&mut` held across the
        // loop below is invalidated under Stacked Borrows by the shared retag
        // through `Pin::row_and_cell`, so we keep a raw pointer and deref per use.
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

        let style = Style {
            flags: stylepkg::Flags {
                bold: true,
                ..stylepkg::Flags::default()
            },
            ..Style::default()
        };
        let base = unsafe { (*page).memory_ptr() };
        let style_id = unsafe { (*page).styles.add(base, style) }.unwrap();

        let mut it = s.row_iterator(
            crate::pagelist::iter::Direction::LeftUp,
            point::Point::Screen(point::Coordinate { x: 0, y: 0 }),
            Some(point::Point::Screen(point::Coordinate { x: 0, y: 9 })),
        );
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
        let base = unsafe { (*page).memory_ptr() };
        while let Some(p) = it.next() {
            let rac = p.row_and_cell();
            unsafe {
                (*rac.row).set_styled(true);
                let mut c = cell_codepoint(b'A' as u32);
                c.set_style_id(style_id);
                *rac.cell = c;
                (*page).styles.use_id(base, style_id);
            }
        }

        // We're over-counted by 1 because `add` implies `use`.
        unsafe { (*page).styles.release(base, style_id) };

        // Expect to have one style
        assert_eq!(1, unsafe { (*page).styles.count() });
    }

    let mut opts = crate::pagelist::access::Clone {
        top: point::Point::Screen(point::Coordinate { x: 0, y: 10 }),
        bot: None,
        tracked_pins: None,
    };
    let mut s2 = s.clone(Global, &mut opts).unwrap();
    assert_eq!(40usize, s2.total_rows());

    {
        assert!(core::ptr::eq(s2.pages.first, s2.pages.last));
        let page = unsafe { &(*crate::pagelist::Node::page(s2.pages.first)) };
        assert_eq!(0, page.styles.count());
    }

    s2.deinit();
    s.deinit();
}

// ref: test "PageList clone partial trimmed both"
#[test]
fn pagelist_clone_partial_trimmed_both() {
    let mut s = PageList::init(Global, Options::new(80, 20)).unwrap();
    assert_eq!(s.rows as usize, s.total_rows());
    s.grow_rows(30).unwrap();

    let mut opts = crate::pagelist::access::Clone {
        top: point::Point::Screen(point::Coordinate { x: 0, y: 10 }),
        bot: Some(point::Point::Screen(point::Coordinate { x: 0, y: 35 })),
        tracked_pins: None,
    };
    let mut s2 = s.clone(Global, &mut opts).unwrap();
    assert_eq!(26usize, s2.total_rows());

    s2.deinit();
    s.deinit();
}

// ref: test "PageList clone less than active"
#[test]
fn pagelist_clone_less_than_active() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
    assert_eq!(s.rows as usize, s.total_rows());

    let mut opts = crate::pagelist::access::Clone {
        top: point::Point::Active(point::Coordinate { x: 0, y: 5 }),
        bot: None,
        tracked_pins: None,
    };
    let mut s2 = s.clone(Global, &mut opts).unwrap();
    assert_eq!(s.rows as usize, s2.total_rows());

    s2.deinit();
    s.deinit();
}

// ref: test "PageList clone remap tracked pin"
#[test]
fn pagelist_clone_remap_tracked_pin() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
    assert_eq!(s.rows as usize, s.total_rows());

    // Put a tracked pin in the screen
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { x: 0, y: 6 }))
                .unwrap(),
        )
        .unwrap();

    let mut opts = crate::pagelist::access::Clone {
        top: point::Point::Active(point::Coordinate { x: 0, y: 5 }),
        bot: None,
        tracked_pins: Some(crate::pagelist::access::TrackedPinsRemap::default()),
    };
    let mut s2 = s.clone(Global, &mut opts).unwrap();

    // We should be able to find our tracked pin
    let p2 = *opts.tracked_pins.as_ref().unwrap().get(&p).unwrap();
    assert_eq!(
        point::Point::Active(point::Coordinate { x: 0, y: 1 }),
        s2.point_from_pin(point::Tag::Active, unsafe { (*p2).pin_copy() })
            .unwrap(),
    );

    s2.deinit();
    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList clone remap tracked pin not in cloned area"
#[test]
fn pagelist_clone_remap_tracked_pin_not_in_cloned_area() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
    assert_eq!(s.rows as usize, s.total_rows());

    // Put a tracked pin in the screen
    let p = s
        .track_pin(
            s.pin(point::Point::Active(point::Coordinate { x: 0, y: 3 }))
                .unwrap(),
        )
        .unwrap();

    let mut opts = crate::pagelist::access::Clone {
        top: point::Point::Active(point::Coordinate { x: 0, y: 5 }),
        bot: None,
        tracked_pins: Some(crate::pagelist::access::TrackedPinsRemap::default()),
    };
    let mut s2 = s.clone(Global, &mut opts).unwrap();

    // We should be able to find our tracked pin
    assert!(opts.tracked_pins.as_ref().unwrap().get(&p).is_none());

    s2.deinit();
    s.untrack_pin(p);
    s.deinit();
}

// ref: test "PageList clone full dirty"
#[test]
fn pagelist_clone_full_dirty() {
    let mut s = PageList::init(Global, Options::new(80, 24)).unwrap();
    assert_eq!(s.rows as usize, s.total_rows());

    // Mark a row as dirty
    s.mark_dirty(point::Point::Active(point::Coordinate { x: 0, y: 0 }));
    s.mark_dirty(point::Point::Active(point::Coordinate { x: 0, y: 12 }));
    s.mark_dirty(point::Point::Active(point::Coordinate { x: 0, y: 23 }));

    let mut opts = crate::pagelist::access::Clone {
        top: point::Point::Screen(point::Coordinate { x: 0, y: 0 }),
        bot: None,
        tracked_pins: None,
    };
    let mut s2 = s.clone(Global, &mut opts).unwrap();
    assert_eq!(s.rows as usize, s2.total_rows());

    // Should still be dirty
    assert!(s2.is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 0 })));
    assert!(!s2.is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 1 })));
    assert!(s2.is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 12 })));
    assert!(!s2.is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 14 })));
    assert!(s2.is_dirty(point::Point::Active(point::Coordinate { x: 0, y: 23 })));

    s2.deinit();
    s.deinit();
}
