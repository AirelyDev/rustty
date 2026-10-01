//! PORTS: ghostty src/terminal/PageList.zig — the `resize` reflow style /
//! wide-char / grapheme / kitty-placeholder tests, grapheme-map capacity, and
//! the viewport-pin / backfill remap tests.

use super::cell_codepoint;
use crate::alloc::Global;
use crate::page::ContentTag;
use crate::pagelist::resize::Resize;
use crate::pagelist::scroll::Scroll;
use crate::pagelist::{std_capacity, Options, PageList, Pin};
use crate::point;
use crate::style::{self as stylepkg, Style};
use rustty_render_types::term_size as size;

// =====================================================================
// resize reflow (style / wide / grapheme / kitty placeholder), grapheme
// map capacity, viewport-pin/backfill remap (batch vt-3c)
// =====================================================================

// ref: test "PageList resize reflow less cols copy style"
#[test]
fn pagelist_resize_reflow_less_cols_copy_style() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(4, 2)
        },
    )
    .unwrap();
    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

        // Create a style
        let style = Style {
            flags: stylepkg::Flags {
                bold: true,
                ..stylepkg::Flags::default()
            },
            ..Style::default()
        };
        let base = unsafe { (*page).memory_ptr() };
        let style_id = unsafe { (*page).styles.add(base, style) }.unwrap();

        for x in 0..s.cols as usize - 1 {
            let rac = unsafe { (*page).get_row_and_cell(x, 0) };
            unsafe {
                let mut c = cell_codepoint(x as u32);
                c.set_style_id(style_id);
                *rac.cell = c;
                (*page).styles.use_id(base, style_id);
            }
        }

        // We're over-counted by 1 because `add` implies `use`.
        unsafe { (*page).styles.release(base, style_id) };
    }

    // Resize
    s.resize(Resize {
        cols: Some(2),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(2usize, s.cols as usize);
    assert_eq!(2usize, s.total_rows());

    let mut it = s.row_iterator(
        crate::pagelist::iter::Direction::RightDown,
        point::Point::Active(point::Coordinate::default()),
        None,
    );
    while let Some(offset) = it.next() {
        for x in 0..s.cols as usize - 1 {
            let mut offset_copy = offset.pin_copy();
            offset_copy.x = x as u16;
            let rac = offset_copy.row_and_cell();
            let style_id = unsafe { (*rac.cell).style_id() };
            assert!(style_id != 0);

            let style = unsafe {
                &*(*crate::pagelist::Node::page(offset.node)).styles.get(
                    (*crate::pagelist::Node::page(offset.node)).memory_ptr(),
                    style_id,
                )
            };
            assert!(style.flags.bold);

            assert!(unsafe { (*rac.row).styled() });
        }
    }

    s.deinit();
}

// ref: test "PageList resize reflow less cols to eliminate a wide char"
#[test]
fn pagelist_resize_reflow_less_cols_to_eliminate_a_wide_char() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(2, 1)
        },
    )
    .unwrap();
    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

        {
            let rac = unsafe { (*page).get_row_and_cell(0, 0) };
            unsafe {
                let mut c = cell_codepoint('😀' as u32);
                c.set_wide(crate::page::Wide::Wide);
                *rac.cell = c;
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 0) };
            unsafe {
                let mut c = cell_codepoint(0);
                c.set_wide(crate::page::Wide::SpacerTail);
                *rac.cell = c;
            }
        }
    }

    // Resize
    s.resize(Resize {
        cols: Some(1),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(1usize, s.cols as usize);
    assert_eq!(1usize, s.total_rows());

    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

        {
            let rac = unsafe { (*page).get_row_and_cell(0, 0) };
            assert_eq!(0u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::Narrow, unsafe { (*rac.cell).wide() });
        }
    }

    s.deinit();
}

// ref: test "PageList resize reflow less cols to wrap a wide char"
#[test]
fn pagelist_resize_reflow_less_cols_to_wrap_a_wide_char() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(3, 1)
        },
    )
    .unwrap();
    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

        {
            let rac = unsafe { (*page).get_row_and_cell(0, 0) };
            unsafe {
                *rac.cell = cell_codepoint('x' as u32);
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 0) };
            unsafe {
                let mut c = cell_codepoint('😀' as u32);
                c.set_wide(crate::page::Wide::Wide);
                *rac.cell = c;
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(2, 0) };
            unsafe {
                let mut c = cell_codepoint(0);
                c.set_wide(crate::page::Wide::SpacerTail);
                *rac.cell = c;
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

// ref: test "PageList resize reflow less cols wide char bulk run" (88ed6bebf)
#[test]
fn pagelist_resize_reflow_less_cols_wide_char_bulk_run() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(8, 1)
        },
    )
    .unwrap();
    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

        // A full row of wide character pairs so the reflow takes
        // the bulk run path.
        for i in 0..4usize {
            {
                let rac = unsafe { (*page).get_row_and_cell(i * 2, 0) };
                unsafe {
                    let mut c = cell_codepoint(0x4E00 + i as u32);
                    c.set_wide(crate::page::Wide::Wide);
                    *rac.cell = c;
                }
            }
            {
                let rac = unsafe { (*page).get_row_and_cell(i * 2 + 1, 0) };
                unsafe {
                    let mut c = cell_codepoint(0);
                    c.set_wide(crate::page::Wide::SpacerTail);
                    *rac.cell = c;
                }
            }
        }
    }

    // Resize to exactly two pairs per row: runs end on the row
    // boundary with no spacer heads needed.
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

        for i in 0..4usize {
            let y = i / 2;
            let x = (i % 2) * 2;
            {
                let rac = unsafe { (*page).get_row_and_cell(x, y) };
                assert_eq!(0x4E00 + i as u32, unsafe {
                    (*rac.cell).content_codepoint()
                });
                assert_eq!(crate::page::Wide::Wide, unsafe { (*rac.cell).wide() });
            }
            {
                let rac = unsafe { (*page).get_row_and_cell(x + 1, y) };
                assert_eq!(0u32, unsafe { (*rac.cell).content_codepoint() });
                assert_eq!(crate::page::Wide::SpacerTail, unsafe { (*rac.cell).wide() });
            }
        }

        {
            let rac = unsafe { (*page).get_row_and_cell(0, 0) };
            assert!(unsafe { (*rac.row).wrap() });
            assert!(!unsafe { (*rac.row).wrap_continuation() });
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(0, 1) };
            assert!(!unsafe { (*rac.row).wrap() });
            assert!(unsafe { (*rac.row).wrap_continuation() });
        }
    }

    s.deinit();
}

// ref: test "PageList resize reflow less cols wide char bulk run odd cols spacer head" (88ed6bebf)
#[test]
fn pagelist_resize_reflow_less_cols_wide_char_bulk_run_odd_cols_spacer_head() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(8, 1)
        },
    )
    .unwrap();
    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

        for i in 0..4usize {
            {
                let rac = unsafe { (*page).get_row_and_cell(i * 2, 0) };
                unsafe {
                    let mut c = cell_codepoint(0x4E00 + i as u32);
                    c.set_wide(crate::page::Wide::Wide);
                    *rac.cell = c;
                }
            }
            {
                let rac = unsafe { (*page).get_row_and_cell(i * 2 + 1, 0) };
                unsafe {
                    let mut c = cell_codepoint(0);
                    c.set_wide(crate::page::Wide::SpacerTail);
                    *rac.cell = c;
                }
            }
        }
    }

    // Resize to an odd number of columns: the bulk run must stop a
    // pair short of the row boundary and the slow path inserts a
    // spacer head in the final column.
    s.resize(Resize {
        cols: Some(5),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(5usize, s.cols as usize);
    assert_eq!(2usize, s.total_rows());

    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

        for i in 0..4usize {
            let y = i / 2;
            let x = (i % 2) * 2;
            {
                let rac = unsafe { (*page).get_row_and_cell(x, y) };
                assert_eq!(0x4E00 + i as u32, unsafe {
                    (*rac.cell).content_codepoint()
                });
                assert_eq!(crate::page::Wide::Wide, unsafe { (*rac.cell).wide() });
            }
            {
                let rac = unsafe { (*page).get_row_and_cell(x + 1, y) };
                assert_eq!(0u32, unsafe { (*rac.cell).content_codepoint() });
                assert_eq!(crate::page::Wide::SpacerTail, unsafe { (*rac.cell).wide() });
            }
        }

        {
            let rac = unsafe { (*page).get_row_and_cell(4, 0) };
            assert_eq!(0u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::SpacerHead, unsafe { (*rac.cell).wide() });
            assert!(unsafe { (*rac.row).wrap() });
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(4, 1) };
            assert_eq!(crate::page::Wide::Narrow, unsafe { (*rac.cell).wide() });
            assert!(unsafe { (*rac.row).wrap_continuation() });
        }
    }

    s.deinit();
}

// ref: test "PageList resize reflow less cols wide char bulk run mixed narrow round trip" (88ed6bebf)
#[test]
fn pagelist_resize_reflow_less_cols_wide_char_bulk_run_mixed_narrow_round_trip() {
    // The emoji-in-prose shape: single wide pairs separated by
    // narrow cells, which must all share a single bulk run.
    struct Shape {
        cp: u32,
        wide: crate::page::Wide,
    }
    let shape: &[Shape] = &[
        Shape {
            cp: 0x4E00,
            wide: crate::page::Wide::Wide,
        },
        Shape {
            cp: 0,
            wide: crate::page::Wide::SpacerTail,
        },
        Shape {
            cp: 'x' as u32,
            wide: crate::page::Wide::Narrow,
        },
        Shape {
            cp: 0x4E01,
            wide: crate::page::Wide::Wide,
        },
        Shape {
            cp: 0,
            wide: crate::page::Wide::SpacerTail,
        },
        Shape {
            cp: 'y' as u32,
            wide: crate::page::Wide::Narrow,
        },
        Shape {
            cp: 0x4E02,
            wide: crate::page::Wide::Wide,
        },
        Shape {
            cp: 0,
            wide: crate::page::Wide::SpacerTail,
        },
        Shape {
            cp: 'z' as u32,
            wide: crate::page::Wide::Narrow,
        },
    ];

    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(9, 1)
        },
    )
    .unwrap();
    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
        for (x, c) in shape.iter().enumerate() {
            let rac = unsafe { (*page).get_row_and_cell(x, 0) };
            unsafe {
                let mut cell = cell_codepoint(c.cp);
                cell.set_wide(c.wide);
                *rac.cell = cell;
            }
        }
    }

    // Shrink: the run ends exactly on the row boundary after the
    // second narrow cell.
    s.resize(Resize {
        cols: Some(6),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(6usize, s.cols as usize);
    assert_eq!(2usize, s.total_rows());

    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
        for (x, c) in shape[0..6].iter().enumerate() {
            let rac = unsafe { (*page).get_row_and_cell(x, 0) };
            assert_eq!(c.cp, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(c.wide, unsafe { (*rac.cell).wide() });
        }
        for (x, c) in shape[6..].iter().enumerate() {
            let rac = unsafe { (*page).get_row_and_cell(x, 1) };
            assert_eq!(c.cp, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(c.wide, unsafe { (*rac.cell).wide() });
        }
        assert!(unsafe { (*(*page).get_row_and_cell(0, 0).row).wrap() });
        assert!(unsafe { (*(*page).get_row_and_cell(0, 1).row).wrap_continuation() });
    }

    // Grow back: the wrapped rows must rejoin into the original
    // single-row layout.
    s.resize(Resize {
        cols: Some(9),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(9usize, s.cols as usize);
    assert_eq!(1usize, s.total_rows());

    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
        for (x, c) in shape.iter().enumerate() {
            let rac = unsafe { (*page).get_row_and_cell(x, 0) };
            assert_eq!(c.cp, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(c.wide, unsafe { (*rac.cell).wide() });
        }
        assert!(!unsafe { (*(*page).get_row_and_cell(0, 0).row).wrap() });
    }

    s.deinit();
}

// ref: test "PageList resize reflow less cols wide char bulk run styled" (88ed6bebf)
#[test]
fn pagelist_resize_reflow_less_cols_wide_char_bulk_run_styled() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(8, 1)
        },
    )
    .unwrap();
    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

        // Create a style
        let style = Style {
            flags: stylepkg::Flags {
                bold: true,
                ..stylepkg::Flags::default()
            },
            ..Style::default()
        };
        let base = unsafe { (*page).memory_ptr() };
        let style_id = unsafe { (*page).styles.add(base, style) }.unwrap();

        // Styled pairs: the tail shares the wide cell's style, as
        // the print path writes them.
        for i in 0..4usize {
            {
                let rac = unsafe { (*page).get_row_and_cell(i * 2, 0) };
                unsafe {
                    let mut c = cell_codepoint(0x4E00 + i as u32);
                    c.set_wide(crate::page::Wide::Wide);
                    c.set_style_id(style_id);
                    *rac.cell = c;
                    (*page).styles.use_id(base, style_id);
                }
            }
            {
                let rac = unsafe { (*page).get_row_and_cell(i * 2 + 1, 0) };
                unsafe {
                    let mut c = cell_codepoint(0);
                    c.set_wide(crate::page::Wide::SpacerTail);
                    c.set_style_id(style_id);
                    *rac.cell = c;
                    (*page).styles.use_id(base, style_id);
                }
            }
        }

        // We're over-counted by 1 because `add` implies `use`.
        unsafe { (*page).styles.release(base, style_id) };
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

        for y in 0..2usize {
            for x in 0..4usize {
                let rac = unsafe { (*page).get_row_and_cell(x, y) };
                let style_id = unsafe { (*rac.cell).style_id() };
                assert!(style_id != 0);

                let style = unsafe { &*(*page).styles.get((*page).memory_ptr(), style_id) };
                assert!(style.flags.bold);
                assert!(unsafe { (*rac.row).styled() });
                assert_eq!(
                    if x % 2 == 0 {
                        crate::page::Wide::Wide
                    } else {
                        crate::page::Wide::SpacerTail
                    },
                    unsafe { (*rac.cell).wide() },
                );
            }
        }
    }

    s.deinit();
}

// ref: test "PageList resize reflow less cols wide char bulk run degenerate spacer tail style" (88ed6bebf)
#[test]
fn pagelist_resize_reflow_less_cols_wide_char_bulk_run_degenerate_spacer_tail_style() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(4, 1)
        },
    )
    .unwrap();
    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

        // A styled wide cell whose tail does NOT share its style.
        // This can't come from the print path, but the run scan must
        // reject the pair (slow path) rather than rewrite the tail
        // to the wide cell's style.
        let style = Style {
            flags: stylepkg::Flags {
                bold: true,
                ..stylepkg::Flags::default()
            },
            ..Style::default()
        };
        let base = unsafe { (*page).memory_ptr() };
        let style_id = unsafe { (*page).styles.add(base, style) }.unwrap();

        {
            let rac = unsafe { (*page).get_row_and_cell(0, 0) };
            unsafe {
                let mut c = cell_codepoint(0x4E00);
                c.set_wide(crate::page::Wide::Wide);
                c.set_style_id(style_id);
                *rac.cell = c;
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 0) };
            unsafe {
                let mut c = cell_codepoint(0);
                c.set_wide(crate::page::Wide::SpacerTail);
                *rac.cell = c;
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(2, 0) };
            unsafe {
                *rac.cell = cell_codepoint('x' as u32);
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
    assert_eq!(1usize, s.total_rows());

    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

        {
            let rac = unsafe { (*page).get_row_and_cell(0, 0) };
            assert_eq!(0x4E00u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::Wide, unsafe { (*rac.cell).wide() });
            assert!(unsafe { (*rac.cell).style_id() } != 0);
            let style = unsafe {
                &*(*page)
                    .styles
                    .get((*page).memory_ptr(), (*rac.cell).style_id())
            };
            assert!(style.flags.bold);
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 0) };
            assert_eq!(crate::page::Wide::SpacerTail, unsafe { (*rac.cell).wide() });
            assert_eq!(stylepkg::DEFAULT_ID, unsafe { (*rac.cell).style_id() });
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(2, 0) };
            assert_eq!('x' as u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::Narrow, unsafe { (*rac.cell).wide() });
        }
    }

    s.deinit();
}

// ref: test "PageList resize reflow less cols to wrap a multi-codepoint grapheme with a spacer head"
#[test]
fn pagelist_resize_reflow_less_cols_to_wrap_a_multi_codepoint_grapheme_with_a_spacer_head() {
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(4, 2)
        },
    )
    .unwrap();
    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

        // We want to make the screen look like this:
        //
        // 👨‍👨‍👦‍👦👨‍👨‍👦‍👦

        // First family emoji at (0, 0)
        {
            let rac = unsafe { (*page).get_row_and_cell(0, 0) };
            unsafe {
                let mut c = cell_codepoint(0x1F468); // First codepoint of the grapheme
                c.set_wide(crate::page::Wide::Wide);
                *rac.cell = c;
            }
            unsafe {
                (*page)
                    .set_graphemes(
                        rac.row,
                        rac.cell,
                        &[0x200D, 0x1F468, 0x200D, 0x1F466, 0x200D, 0x1F466],
                    )
                    .unwrap();
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 0) };
            unsafe {
                let mut c = cell_codepoint(0);
                c.set_wide(crate::page::Wide::SpacerTail);
                *rac.cell = c;
            }
        }
        // Second family emoji at (2, 0)
        {
            let rac = unsafe { (*page).get_row_and_cell(2, 0) };
            unsafe {
                let mut c = cell_codepoint(0x1F468); // First codepoint of the grapheme
                c.set_wide(crate::page::Wide::Wide);
                *rac.cell = c;
            }
            unsafe {
                (*page)
                    .set_graphemes(
                        rac.row,
                        rac.cell,
                        &[0x200D, 0x1F468, 0x200D, 0x1F466, 0x200D, 0x1F466],
                    )
                    .unwrap();
            }
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(3, 0) };
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
            assert_eq!(0x1F468u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::Wide, unsafe { (*rac.cell).wide() });

            let cps = unsafe { (*page).lookup_grapheme(rac.cell).unwrap() };
            assert_eq!(6usize, cps.len());
            assert_eq!(0x200D, cps[0]);
            assert_eq!(0x1F468, cps[1]);
            assert_eq!(0x200D, cps[2]);
            assert_eq!(0x1F466, cps[3]);
            assert_eq!(0x200D, cps[4]);
            assert_eq!(0x1F466, cps[5]);

            // Row should be wrapped
            assert!(unsafe { (*rac.row).wrap() });
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 0) };
            assert_eq!(0u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::SpacerTail, unsafe { (*rac.cell).wide() });
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(2, 0) };
            assert_eq!(0u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::SpacerHead, unsafe { (*rac.cell).wide() });
        }

        {
            let rac = unsafe { (*page).get_row_and_cell(0, 0) };
            assert_eq!(0x1F468u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::Wide, unsafe { (*rac.cell).wide() });

            let cps = unsafe { (*page).lookup_grapheme(rac.cell).unwrap() };
            assert_eq!(6usize, cps.len());
            assert_eq!(0x200D, cps[0]);
            assert_eq!(0x1F468, cps[1]);
            assert_eq!(0x200D, cps[2]);
            assert_eq!(0x1F466, cps[3]);
            assert_eq!(0x200D, cps[4]);
            assert_eq!(0x1F466, cps[5]);
        }
        {
            let rac = unsafe { (*page).get_row_and_cell(1, 1) };
            assert_eq!(0u32, unsafe { (*rac.cell).content_codepoint() });
            assert_eq!(crate::page::Wide::SpacerTail, unsafe { (*rac.cell).wide() });
        }
    }

    s.deinit();
}

// ref: test "PageList resize reflow less cols copy kitty placeholder"
// PORT NOTE: reference gates on `comptime !build_options.kitty_graphics`
// (SkipZigTest). Rustty's `KITTY_GRAPHICS` const is always `true`, so the
// test always runs. `kitty.graphics.unicode.placeholder` is U+10EEEE.
#[test]
fn pagelist_resize_reflow_less_cols_copy_kitty_placeholder() {
    const KITTY_PLACEHOLDER: u32 = 0x10EEEE;

    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(4, 2)
        },
    )
    .unwrap();
    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

        // Write unicode placeholders
        for x in 0..s.cols as usize - 1 {
            let rac = unsafe { (*page).get_row_and_cell(x, 0) };
            unsafe {
                (*rac.row).set_kitty_virtual_placeholder(true);
                *rac.cell = cell_codepoint(KITTY_PLACEHOLDER);
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
    assert_eq!(2usize, s.total_rows());

    let mut it = s.row_iterator(
        crate::pagelist::iter::Direction::RightDown,
        point::Point::Active(point::Coordinate::default()),
        None,
    );
    while let Some(offset) = it.next() {
        for x in 0..s.cols as usize - 1 {
            let mut offset_copy = offset.pin_copy();
            offset_copy.x = x as u16;
            let rac = offset_copy.row_and_cell();

            assert!(unsafe { (*rac.row).kitty_virtual_placeholder() });
        }
    }

    s.deinit();
}

// ref: test "PageList resize reflow more cols clears kitty placeholder"
// PORT NOTE: see kitty placeholder note above; `KITTY_GRAPHICS` is always true.
#[test]
fn pagelist_resize_reflow_more_cols_clears_kitty_placeholder() {
    const KITTY_PLACEHOLDER: u32 = 0x10EEEE;

    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(4, 2)
        },
    )
    .unwrap();
    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

        // Write unicode placeholders
        for x in 0..s.cols as usize - 1 {
            let rac = unsafe { (*page).get_row_and_cell(x, 0) };
            unsafe {
                (*rac.row).set_kitty_virtual_placeholder(true);
                *rac.cell = cell_codepoint(KITTY_PLACEHOLDER);
            }
        }
    }

    // Resize smaller then larger
    s.resize(Resize {
        cols: Some(2),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    s.resize(Resize {
        cols: Some(4),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(4usize, s.cols as usize);
    assert_eq!(2usize, s.total_rows());

    let mut it = s.row_iterator(
        crate::pagelist::iter::Direction::RightDown,
        point::Point::Active(point::Coordinate::default()),
        None,
    );
    {
        let row = it.next().unwrap();
        let rac = row.row_and_cell();
        assert!(unsafe { (*rac.row).kitty_virtual_placeholder() });
    }
    {
        let row = it.next().unwrap();
        let rac = row.row_and_cell();
        assert!(!unsafe { (*rac.row).kitty_virtual_placeholder() });
    }
    assert!(it.next().is_none());

    s.deinit();
}

// ref: test "PageList resize reflow wrap moves kitty placeholder"
// PORT NOTE: see kitty placeholder note above; `KITTY_GRAPHICS` is always true.
#[test]
fn pagelist_resize_reflow_wrap_moves_kitty_placeholder() {
    const KITTY_PLACEHOLDER: u32 = 0x10EEEE;

    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(4, 2)
        },
    )
    .unwrap();
    {
        assert!(s.pages.first == s.pages.last);
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };

        // Write unicode placeholders
        for x in 2..s.cols as usize - 1 {
            let rac = unsafe { (*page).get_row_and_cell(x, 0) };
            unsafe {
                (*rac.row).set_kitty_virtual_placeholder(true);
                *rac.cell = cell_codepoint(KITTY_PLACEHOLDER);
            }
        }
    }

    s.resize(Resize {
        cols: Some(2),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(2usize, s.cols as usize);
    assert_eq!(2usize, s.total_rows());

    let mut it = s.row_iterator(
        crate::pagelist::iter::Direction::RightDown,
        point::Point::Active(point::Coordinate::default()),
        None,
    );
    {
        let row = it.next().unwrap();
        let rac = row.row_and_cell();
        assert!(!unsafe { (*rac.row).kitty_virtual_placeholder() });
    }
    {
        let row = it.next().unwrap();
        let rac = row.row_and_cell();
        assert!(unsafe { (*rac.row).kitty_virtual_placeholder() });
    }
    assert!(it.next().is_none());

    s.deinit();
}

// ref: test "PageList resize reflow grapheme map capacity exceeded"
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn pagelist_resize_reflow_grapheme_map_capacity_exceeded() {
    // This test verifies that when reflowing content with many graphemes,
    // the grapheme map capacity is correctly increased when needed.
    let mut s = PageList::init(
        Global,
        Options {
            max_size: Some(0),
            ..Options::new(4, 10)
        },
    )
    .unwrap();
    assert_eq!(1usize, s.total_pages());

    // Get the grapheme capacity from the page. We need more than this many
    // graphemes in a single destination page to trigger capacity increase
    // during reflow. Since each source page can only hold this many graphemes,
    // we create two source pages with graphemes that will merge into one
    // destination page.
    let grapheme_capacity =
        unsafe { (*crate::pagelist::Node::page(s.pages.first)).grapheme_capacity() };
    // Use slightly more than half the capacity per page, so combined they
    // exceed the capacity of a single destination page.
    let graphemes_per_page = grapheme_capacity / 2 + grapheme_capacity / 4;

    // Grow to the capacity of the first page and add more rows
    // so that we have two pages total.
    {
        let cap_rows = unsafe { crate::pagelist::Node::capacity(s.pages.first).rows };
        let size_rows = unsafe { crate::pagelist::Node::rows(s.pages.first) };
        unsafe { (*crate::pagelist::Node::page(s.pages.first)).pause_integrity_checks(true) };
        for _ in size_rows..cap_rows {
            let _ = s.grow().unwrap();
        }
        unsafe { (*crate::pagelist::Node::page(s.pages.first)).pause_integrity_checks(false) };
        assert_eq!(1usize, s.total_pages());
        s.grow_rows(graphemes_per_page).unwrap();
        assert_eq!(2usize, s.total_pages());

        // We now have two pages.
        assert!(s.pages.first != s.pages.last);
        assert!(core::ptr::eq(s.pages.last, unsafe {
            (*s.pages.first).next
        }));
    }

    // Add graphemes to both pages. We add graphemes to rows at the END of the
    // first page, and graphemes to rows at the START of the second page.
    // When reflowing to 2 columns, these rows will wrap and stay together
    // on the same destination page, requiring capacity increase.

    // Add graphemes to the end of the first page (last rows)
    {
        let page = unsafe { crate::pagelist::Node::page(s.pages.first) };
        let start_row = unsafe { (*page).size.rows as usize - graphemes_per_page };
        for i in 0..graphemes_per_page {
            let y = start_row + i;
            let rac = unsafe { (*page).get_row_and_cell(0, y) };
            unsafe {
                *rac.cell = cell_codepoint('A' as u32);
            }
            unsafe {
                (*page).append_grapheme(rac.row, rac.cell, 0x0301).unwrap();
            }
        }
    }

    // Add graphemes to the beginning of the second page
    {
        let page = unsafe { crate::pagelist::Node::page(s.pages.last) };
        let count = unsafe { core::cmp::min(graphemes_per_page, (*page).size.rows as usize) };
        for y in 0..count {
            let rac = unsafe { (*page).get_row_and_cell(0, y) };
            unsafe {
                *rac.cell = cell_codepoint('B' as u32);
            }
            unsafe {
                (*page).append_grapheme(rac.row, rac.cell, 0x0302).unwrap();
            }
        }
    }

    // Resize to fewer columns to trigger reflow.
    // The graphemes from both pages will be copied to destination pages.
    // They will all end up in a contiguous region of the destination.
    // If the bug exists (hyperlink_bytes increased instead of grapheme_bytes),
    // this will fail with GraphemeMapOutOfMemory when we exceed capacity.
    s.resize(Resize {
        cols: Some(2),
        reflow: true,
        ..Default::default()
    })
    .unwrap();

    // Verify the resize succeeded
    assert_eq!(2usize, s.cols as usize);

    s.deinit();
}

// ref: test "PageList resize grow cols with unwrap fixes viewport pin"
#[test]
fn pagelist_resize_grow_cols_with_unwrap_fixes_viewport_pin() {
    // Regression test: after resize/reflow, the viewport pin can end up at a
    // position where pin.y + rows > total_rows, causing getBottomRight to panic.

    // The plan is to pin viewport in history, then grow columns to unwrap rows.
    // The unwrap reduces total_rows, but the tracked pin moves to a position
    // that no longer has enough rows below it for the viewport height.
    let mut s = PageList::init(Global, Options::new(2, 10)).unwrap();

    // Make sure we have some history, in this case we have 30 rows of history
    s.grow_rows(30).unwrap();
    assert_eq!(40usize, s.total_rows());

    // Fill all rows with wrapped content (pairs that unwrap when cols increase)
    let mut it = s.page_iterator(
        crate::pagelist::iter::Direction::RightDown,
        point::Point::Screen(point::Coordinate::default()),
        None,
    );
    while let Some(chunk) = it.next() {
        let page = unsafe { crate::pagelist::Node::page(chunk.node) };
        for y in chunk.start as usize..chunk.end as usize {
            let rac = unsafe { (*page).get_row_and_cell(0, y) };
            if y % 2 == 0 {
                unsafe { (*rac.row).set_wrap(true) };
            } else {
                unsafe { (*rac.row).set_wrap_continuation(true) };
            }
            for x in 0..s.cols as usize {
                let rac = unsafe { (*page).get_row_and_cell(x, y) };
                unsafe {
                    *rac.cell = cell_codepoint('A' as u32);
                }
            }
        }
    }

    // Pin viewport at row 28 (in history, 2 rows before active area at row 30).
    // After unwrap: row 28 -> row 14, total_rows 40 -> 20, active starts at 10.
    // Pin at 14 needs rows 14-23, but only 0-19 exist -> overflow.
    s.scroll(Scroll::Pin(
        s.pin(point::Point::Screen(point::Coordinate { x: 0, y: 28 }))
            .unwrap(),
    ));
    assert!(s.viewport == crate::pagelist::Viewport::Pin);
    assert!(s.get_bottom_right(point::Tag::Viewport).is_some());

    // Resize with reflow: unwraps rows, reducing total_rows
    s.resize(Resize {
        cols: Some(4),
        reflow: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(4usize, s.cols as usize);
    assert!(s.total_rows() < 40);

    // Used to panic here, so test that we can get the bottom right.
    let br_after = s.get_bottom_right(point::Tag::Viewport);
    assert!(br_after.is_some());

    s.deinit();
}

// ref: test "PageList resize (no reflow) more cols remaps pins in backfill path"
#[cfg_attr(
    all(miri, debug_assertions),
    ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
)]
#[test]
fn pagelist_resize_no_reflow_more_cols_remaps_pins_in_backfill_path() {
    // Regression test: when resizeWithoutReflowGrowCols copies rows to a previous
    // page with spare capacity, tracked pins in those rows must be remapped.
    // Without the fix, pins become dangling pointers when the original page is destroyed.
    let cols: size::CellCountInt = 5;
    let cap = std_capacity()
        .adjust(crate::page::Adjustment { cols: Some(cols) })
        .unwrap();
    let mut s = PageList::init(Global, Options::new(cols, cap.rows)).unwrap();

    // Grow until we have two pages.
    while s.pages.first == s.pages.last {
        let _ = s.grow().unwrap();
    }
    let first_page = s.pages.first;
    let second_page = s.pages.last;
    assert!(first_page != second_page);

    // Trim a history row so the first page has spare capacity.
    // This triggers the backfill path in resizeWithoutReflowGrowCols.
    s.erase_history(Some(point::Point::History(point::Coordinate {
        x: 0,
        y: 0,
    })));
    assert!(unsafe {
        crate::pagelist::Node::rows(first_page) < crate::pagelist::Node::capacity(first_page).rows
    });

    // Ensure the resize takes the slow path (new capacity > current capacity).
    let new_cols: size::CellCountInt = cols + 1;
    let adjusted = unsafe { crate::pagelist::Node::capacity(second_page) }
        .adjust(crate::page::Adjustment {
            cols: Some(new_cols),
        })
        .unwrap();
    assert!(unsafe { crate::pagelist::Node::capacity(second_page).cols } < adjusted.cols);

    // Track a pin in row 0 of the second page. This row will be copied
    // to the first page during backfill and the pin must be remapped.
    let tracked = s
        .track_pin(Pin {
            node: second_page,
            x: 0,
            y: 0,
            garbage: false,
        })
        .unwrap();

    // Write a marker character to the tracked cell so we can verify
    // the pin points to the correct cell after resize.
    let marker: u32 = 'X' as u32;
    {
        let rac = unsafe { (*tracked).row_and_cell() };
        unsafe {
            *rac.cell = cell_codepoint(marker);
        }
    }

    s.resize(Resize {
        cols: Some(new_cols),
        reflow: false,
        ..Default::default()
    })
    .unwrap();

    // Verify the pin points to a valid node still in the page list.
    let mut found = false;
    let mut it = s.pages.first;
    while !it.is_null() {
        if it == unsafe { (*tracked).node } {
            found = true;
            break;
        }
        it = unsafe { (*it).next };
    }
    assert!(found);
    assert!(unsafe { (*tracked).y < crate::pagelist::Node::rows((*tracked).node) });

    // Verify the pin still points to the cell with our marker content.
    let cell = unsafe { (*tracked).row_and_cell().cell };
    assert_eq!(ContentTag::Codepoint, unsafe { (*cell).content_tag() });
    assert_eq!(marker, unsafe { (*cell).content_codepoint() });

    s.untrack_pin(tracked);
    s.deinit();
}
