//! PORT NOTE: ghostty `src/terminal/c/render.zig` `test {}` blocks, ported
//! assertion-for-assertion. The reference's `&lib.alloc.test_allocator` becomes
//! the C allocator built by `with_test_allocator` (leaks caught by Miri per R6).

use super::*;
use crate::result::Result;
use crate::test_util::with_test_allocator;
use rustty_vt::color::RGB;

// PORT NOTE: the reference accesses `terminal.?.terminal.screens.active` and
// calls `screen.select(.init(...))` directly. In Rust the terminal handle is a
// `*mut TerminalWrapper`; this helper resolves the active `Screen` raw pointer.
unsafe fn active_screen(terminal: terminal_c::Terminal) -> *mut rustty_vt::screen::Screen {
    // const t = terminal.?.terminal; const screen = t.screens.active;
    let t = unsafe { (*terminal).terminal };
    unsafe { (*t).screens.active }
}

// test "render: new/free"
#[test]
fn render_new_free() {
    with_test_allocator(|alloc| {
        // var state: RenderState = null;
        let mut state: RenderState = std::ptr::null_mut();
        // try testing.expectEqual(Result.success, new(&lib.alloc.test_allocator, &state));
        assert_eq!(Result::Success, unsafe { new(Some(alloc), &mut state) });
        // try testing.expect(state != null);
        assert!(!state.is_null());
        // free(state);
        unsafe { free(state) };
    });
}

// test "render: free null"
#[test]
fn render_free_null() {
    // free(null);
    unsafe { free(std::ptr::null_mut()) };
}

// test "render: update invalid value"
#[test]
fn render_update_invalid_value() {
    with_test_allocator(|alloc| {
        let mut state: RenderState = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe { new(Some(alloc), &mut state) });
        // defer free(state);

        // try testing.expectEqual(Result.invalid_value, update(null, null));
        assert_eq!(Result::InvalidValue, unsafe {
            update(std::ptr::null_mut(), std::ptr::null_mut())
        });
        // try testing.expectEqual(Result.invalid_value, update(state, null));
        assert_eq!(Result::InvalidValue, unsafe {
            update(state, std::ptr::null_mut())
        });

        unsafe { free(state) };
    });
}

// test "render: get invalid value"
#[test]
fn render_get_invalid_value() {
    // var cols: size.CellCountInt = 0;
    let mut cols: size::CellCountInt = 0;
    // try testing.expectEqual(Result.invalid_value, get(null, .cols, @ptrCast(&cols)));
    assert_eq!(Result::InvalidValue, unsafe {
        get(
            std::ptr::null_mut(),
            Data::Cols,
            &mut cols as *mut _ as *mut c_void,
        )
    });
}

// test "render: get invalid data"
#[test]
fn render_get_invalid_data() {
    with_test_allocator(|alloc| {
        let mut state: RenderState = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe { new(Some(alloc), &mut state) });
        // defer free(state);

        // try testing.expectEqual(Result.invalid_value, get(state, .invalid, null));
        assert_eq!(Result::InvalidValue, unsafe {
            get(state, Data::Invalid, std::ptr::null_mut())
        });

        unsafe { free(state) };
    });
}

// test "render: colors get invalid value"
#[test]
fn render_colors_get_invalid_value() {
    with_test_allocator(|alloc| {
        let mut state: RenderState = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe { new(Some(alloc), &mut state) });
        // defer free(state);

        // var colors: Colors = std.mem.zeroes(Colors);
        let mut colors: Colors = unsafe { std::mem::zeroed() };
        // colors.size = @sizeOf(Colors);
        colors.size = std::mem::size_of::<Colors>();

        // try testing.expectEqual(Result.invalid_value, colors_get(null, &colors));
        assert_eq!(Result::InvalidValue, unsafe {
            colors_get(std::ptr::null_mut(), &mut colors)
        });
        // try testing.expectEqual(Result.invalid_value, colors_get(state, null));
        assert_eq!(Result::InvalidValue, unsafe {
            colors_get(state, std::ptr::null_mut())
        });

        // colors.size = @sizeOf(usize) - 1;
        colors.size = std::mem::size_of::<usize>() - 1;
        // try testing.expectEqual(Result.invalid_value, colors_get(state, &colors));
        assert_eq!(Result::InvalidValue, unsafe {
            colors_get(state, &mut colors)
        });

        unsafe { free(state) };
    });
}

// test "render: get/set dirty invalid value"
#[test]
fn render_get_set_dirty_invalid_value() {
    // var dirty: Dirty = .false;
    let mut dirty: Dirty = Dirty::False;
    // try testing.expectEqual(Result.invalid_value, get(null, .dirty, @ptrCast(&dirty)));
    assert_eq!(Result::InvalidValue, unsafe {
        get(
            std::ptr::null_mut(),
            Data::Dirty,
            &mut dirty as *mut _ as *mut c_void,
        )
    });
    // const dirty_full: Dirty = .full;
    let dirty_full: Dirty = Dirty::Full;
    // try testing.expectEqual(Result.invalid_value, set(null, .dirty, @ptrCast(&dirty_full)));
    assert_eq!(Result::InvalidValue, unsafe {
        set(
            std::ptr::null_mut(),
            SetOption::Dirty,
            &dirty_full as *const _ as *const c_void,
        )
    });
}

// test "render: get/set dirty"
#[test]
fn render_get_set_dirty() {
    with_test_allocator(|alloc| {
        let mut state: RenderState = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe { new(Some(alloc), &mut state) });
        // defer free(state);

        // var dirty: Dirty = undefined;
        let mut dirty: Dirty = Dirty::False;
        // try testing.expectEqual(Result.success, get(state, .dirty, @ptrCast(&dirty)));
        assert_eq!(Result::Success, unsafe {
            get(state, Data::Dirty, &mut dirty as *mut _ as *mut c_void)
        });
        // try testing.expectEqual(Dirty.false, dirty);
        assert_eq!(Dirty::False, dirty);

        // const dirty_partial: Dirty = .partial;
        let dirty_partial: Dirty = Dirty::Partial;
        assert_eq!(Result::Success, unsafe {
            set(
                state,
                SetOption::Dirty,
                &dirty_partial as *const _ as *const c_void,
            )
        });
        assert_eq!(Result::Success, unsafe {
            get(state, Data::Dirty, &mut dirty as *mut _ as *mut c_void)
        });
        // try testing.expectEqual(Dirty.partial, dirty);
        assert_eq!(Dirty::Partial, dirty);

        // const dirty_full: Dirty = .full;
        let dirty_full: Dirty = Dirty::Full;
        assert_eq!(Result::Success, unsafe {
            set(
                state,
                SetOption::Dirty,
                &dirty_full as *const _ as *const c_void,
            )
        });
        assert_eq!(Result::Success, unsafe {
            get(state, Data::Dirty, &mut dirty as *mut _ as *mut c_void)
        });
        // try testing.expectEqual(Dirty.full, dirty);
        assert_eq!(Dirty::Full, dirty);

        unsafe { free(state) };
    });
}

// test "render: set null value"
#[test]
fn render_set_null_value() {
    with_test_allocator(|alloc| {
        let mut state: RenderState = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe { new(Some(alloc), &mut state) });
        // defer free(state);

        // try testing.expectEqual(Result.invalid_value, set(state, .dirty, null));
        assert_eq!(Result::InvalidValue, unsafe {
            set(state, SetOption::Dirty, std::ptr::null())
        });

        unsafe { free(state) };
    });
}

// PORT NOTE: shared test scaffold for the iterator/update tests below. Builds a
// terminal, render state, and (optionally) an updated state. Returns the handles
// and a teardown closure equivalent to the reference's `defer` chain.
fn with_terminal_state<F: FnOnce(terminal_c::Terminal, RenderState)>(
    alloc: &rustty_lib::allocator::Allocator,
    cols: size::CellCountInt,
    rows: size::CellCountInt,
    f: F,
) {
    let mut terminal: terminal_c::Terminal = std::ptr::null_mut();
    assert_eq!(Result::Success, unsafe {
        terminal_c::new(
            alloc,
            &mut terminal,
            terminal_c::Options {
                cols,
                rows,
                max_scrollback: 10_000,
            },
        )
    });

    let mut state: RenderState = std::ptr::null_mut();
    assert_eq!(Result::Success, unsafe { new(Some(alloc), &mut state) });

    f(terminal, state);

    unsafe { free(state) };
    unsafe { terminal_c::free(terminal) };
}

// test "render: row iterator get invalid value"
#[test]
fn render_row_iterator_get_invalid_value() {
    with_test_allocator(|alloc| {
        // var iterator: RowIterator = null;
        let mut iterator: RowIterator = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            row_iterator_new(Some(alloc), &mut iterator)
        });
        // defer row_iterator_free(iterator);

        // try testing.expectEqual(Result.invalid_value, get(null, .row_iterator, @ptrCast(&iterator)));
        assert_eq!(Result::InvalidValue, unsafe {
            get(
                std::ptr::null_mut(),
                Data::RowIterator,
                &mut iterator as *mut _ as *mut c_void,
            )
        });

        unsafe { row_iterator_free(iterator) };
    });
}

// test "render: row iterator new/free"
#[test]
fn render_row_iterator_new_free() {
    with_test_allocator(|alloc| {
        with_terminal_state(alloc, 80, 24, |terminal, state| {
            // try testing.expectEqual(Result.success, update(state, terminal));
            assert_eq!(Result::Success, unsafe { update(state, terminal) });

            let mut iterator: RowIterator = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                row_iterator_new(Some(alloc), &mut iterator)
            });
            // defer row_iterator_free(iterator);

            // try testing.expect(iterator != null);
            assert!(!iterator.is_null());

            assert_eq!(Result::Success, unsafe {
                get(
                    state,
                    Data::RowIterator,
                    &mut iterator as *mut _ as *mut c_void,
                )
            });

            // const iterator_ptr = iterator.?;
            let iterator_ptr = unsafe { &*iterator };
            // const row_data = state.?.state.row_data.slice();
            let row_data = unsafe { &(*state).state.row_data };

            // try testing.expectEqual(@as(?size.CellCountInt, null), iterator_ptr.y);
            assert_eq!(None, iterator_ptr.y);
            // try testing.expectEqual(row_data.items(.raw).len, iterator_ptr.raws.len);
            // PORT NOTE: the per-field slice lengths all equal `row_data.len()`.
            assert_eq!(row_data.len(), iterator_ptr.rows.len());

            unsafe { row_iterator_free(iterator) };
        });
    });
}

// test "render: row iterator free null"
#[test]
fn render_row_iterator_free_null() {
    // row_iterator_free(null);
    unsafe { row_iterator_free(std::ptr::null_mut()) };
}

// test "render: row iterator next null"
#[test]
fn render_row_iterator_next_null() {
    // try testing.expect(!row_iterator_next(null));
    assert!(!unsafe { row_iterator_next(std::ptr::null_mut()) });
}

// test "render: row get null"
#[test]
fn render_row_get_null() {
    // var dirty: bool = undefined;
    let mut dirty: bool = false;
    // try testing.expectEqual(Result.invalid_value, row_get(null, .dirty, @ptrCast(&dirty)));
    assert_eq!(Result::InvalidValue, unsafe {
        row_get(
            std::ptr::null_mut(),
            RowData::Dirty,
            &mut dirty as *mut _ as *mut c_void,
        )
    });
}

// test "render: row get invalid data"
#[test]
fn render_row_get_invalid_data() {
    with_test_allocator(|alloc| {
        with_terminal_state(alloc, 80, 24, |terminal, state| {
            assert_eq!(Result::Success, unsafe { update(state, terminal) });

            let mut iterator: RowIterator = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                row_iterator_new(Some(alloc), &mut iterator)
            });
            // defer row_iterator_free(iterator);

            assert_eq!(Result::Success, unsafe {
                get(
                    state,
                    Data::RowIterator,
                    &mut iterator as *mut _ as *mut c_void,
                )
            });
            // try testing.expect(row_iterator_next(iterator));
            assert!(unsafe { row_iterator_next(iterator) });
            // try testing.expectEqual(Result.invalid_value, row_get(iterator, .invalid, null));
            assert_eq!(Result::InvalidValue, unsafe {
                row_get(iterator, RowData::Invalid, std::ptr::null_mut())
            });

            unsafe { row_iterator_free(iterator) };
        });
    });
}

// test "render: row set null"
#[test]
fn render_row_set_null() {
    // const dirty = false;
    let dirty = false;
    // try testing.expectEqual(Result.invalid_value, row_set(null, .dirty, @ptrCast(&dirty)));
    assert_eq!(Result::InvalidValue, unsafe {
        row_set(
            std::ptr::null_mut(),
            RowOption::Dirty,
            &dirty as *const _ as *const c_void,
        )
    });
}

// test "render: row set before iteration"
#[test]
fn render_row_set_before_iteration() {
    with_test_allocator(|alloc| {
        with_terminal_state(alloc, 80, 24, |terminal, state| {
            assert_eq!(Result::Success, unsafe { update(state, terminal) });

            let mut iterator: RowIterator = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                row_iterator_new(Some(alloc), &mut iterator)
            });
            // defer row_iterator_free(iterator);

            assert_eq!(Result::Success, unsafe {
                get(
                    state,
                    Data::RowIterator,
                    &mut iterator as *mut _ as *mut c_void,
                )
            });
            // const dirty = false;
            let dirty = false;
            // try testing.expectEqual(Result.invalid_value, row_set(iterator, .dirty, @ptrCast(&dirty)));
            assert_eq!(Result::InvalidValue, unsafe {
                row_set(
                    iterator,
                    RowOption::Dirty,
                    &dirty as *const _ as *const c_void,
                )
            });

            unsafe { row_iterator_free(iterator) };
        });
    });
}

// test "render: row get before iteration"
#[test]
fn render_row_get_before_iteration() {
    with_test_allocator(|alloc| {
        with_terminal_state(alloc, 80, 24, |terminal, state| {
            assert_eq!(Result::Success, unsafe { update(state, terminal) });

            let mut iterator: RowIterator = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                row_iterator_new(Some(alloc), &mut iterator)
            });
            // defer row_iterator_free(iterator);

            assert_eq!(Result::Success, unsafe {
                get(
                    state,
                    Data::RowIterator,
                    &mut iterator as *mut _ as *mut c_void,
                )
            });
            // var dirty: bool = undefined;
            let mut dirty: bool = false;
            // try testing.expectEqual(Result.invalid_value, row_get(iterator, .dirty, @ptrCast(&dirty)));
            assert_eq!(Result::InvalidValue, unsafe {
                row_get(
                    iterator,
                    RowData::Dirty,
                    &mut dirty as *mut _ as *mut c_void,
                )
            });

            unsafe { row_iterator_free(iterator) };
        });
    });
}

// test "render: row get/set dirty"
#[test]
fn render_row_get_set_dirty() {
    with_test_allocator(|alloc| {
        with_terminal_state(alloc, 80, 24, |terminal, state| {
            assert_eq!(Result::Success, unsafe { update(state, terminal) });

            // Dirty the first row so the iterator has at least one dirty row to observe.
            // terminal_c.vt_write(terminal, "hello", 5);
            let input = b"hello";
            unsafe { terminal_c::vt_write(terminal, input.as_ptr(), 5) };
            assert_eq!(Result::Success, unsafe { update(state, terminal) });

            // Create an iterator and verify it is dirty.
            let mut it: RowIterator = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                row_iterator_new(Some(alloc), &mut it)
            });
            // defer row_iterator_free(it);

            assert_eq!(Result::Success, unsafe {
                get(state, Data::RowIterator, &mut it as *mut _ as *mut c_void)
            });
            assert!(unsafe { row_iterator_next(it) });
            let mut dirty: bool = false;
            assert_eq!(Result::Success, unsafe {
                row_get(it, RowData::Dirty, &mut dirty as *mut _ as *mut c_void)
            });
            // try testing.expect(dirty);
            assert!(dirty);

            // Clear dirty on this row.
            // const dirty_false = false;
            let dirty_false = false;
            assert_eq!(Result::Success, unsafe {
                row_set(
                    it,
                    RowOption::Dirty,
                    &dirty_false as *const _ as *const c_void,
                )
            });

            // It should not be dirty anymore.
            let mut it2: RowIterator = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                row_iterator_new(Some(alloc), &mut it2)
            });
            // defer row_iterator_free(it2);

            assert_eq!(Result::Success, unsafe {
                get(state, Data::RowIterator, &mut it2 as *mut _ as *mut c_void)
            });
            assert!(unsafe { row_iterator_next(it2) });
            assert_eq!(Result::Success, unsafe {
                row_get(it2, RowData::Dirty, &mut dirty as *mut _ as *mut c_void)
            });
            // try testing.expect(!dirty);
            assert!(!dirty);

            unsafe { row_iterator_free(it2) };
            unsafe { row_iterator_free(it) };
        });
    });
}

// test "render: row get selection"
#[test]
fn render_row_get_selection() {
    with_test_allocator(|alloc| {
        with_terminal_state(alloc, 10, 3, |terminal, state| {
            // const t = terminal.?.terminal; const screen = t.screens.active;
            let screen = unsafe { active_screen(terminal) };
            // try screen.select(.init(screen.pages.pin(.{ .active = .{ .x = 2, .y = 1 } }).?, screen.pages.pin(.{ .active = .{ .x = 4, .y = 1 } }).?, false));
            let start = unsafe {
                (*screen).pages.pin(rustty_vt::point::Point::Active(
                    rustty_vt::point::Coordinate { x: 2, y: 1 },
                ))
            }
            .unwrap();
            let end = unsafe {
                (*screen).pages.pin(rustty_vt::point::Point::Active(
                    rustty_vt::point::Coordinate { x: 4, y: 1 },
                ))
            }
            .unwrap();
            unsafe {
                (*screen).select(Some(rustty_vt::selection::Selection::init(
                    start, end, false,
                )))
            }
            .unwrap();

            assert_eq!(Result::Success, unsafe { update(state, terminal) });

            let mut it: RowIterator = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                row_iterator_new(Some(alloc), &mut it)
            });
            // defer row_iterator_free(it);

            assert_eq!(Result::Success, unsafe {
                get(state, Data::RowIterator, &mut it as *mut _ as *mut c_void)
            });

            // var sel: RowSelection = .{};
            let mut sel: RowSelection = RowSelection::default();
            // try testing.expect(row_iterator_next(it));
            assert!(unsafe { row_iterator_next(it) });
            // try testing.expectEqual(Result.no_value, row_get(it, .selection, @ptrCast(&sel)));
            assert_eq!(Result::NoValue, unsafe {
                row_get(it, RowData::Selection, &mut sel as *mut _ as *mut c_void)
            });

            assert!(unsafe { row_iterator_next(it) });
            // sel = .{};
            sel = RowSelection::default();
            assert_eq!(Result::Success, unsafe {
                row_get(it, RowData::Selection, &mut sel as *mut _ as *mut c_void)
            });
            // try testing.expectEqual(@as(u16, 2), sel.start_x);
            assert_eq!(2u16, sel.start_x);
            // try testing.expectEqual(@as(u16, 4), sel.end_x);
            assert_eq!(4u16, sel.end_x);

            assert!(unsafe { row_iterator_next(it) });
            sel = RowSelection::default();
            assert_eq!(Result::NoValue, unsafe {
                row_get(it, RowData::Selection, &mut sel as *mut _ as *mut c_void)
            });

            unsafe { row_iterator_free(it) };
        });
    });
}

// test "render: row cells get selected"
#[test]
fn render_row_cells_get_selected() {
    with_test_allocator(|alloc| {
        with_terminal_state(alloc, 10, 3, |terminal, state| {
            let screen = unsafe { active_screen(terminal) };
            let start = unsafe {
                (*screen).pages.pin(rustty_vt::point::Point::Active(
                    rustty_vt::point::Coordinate { x: 2, y: 1 },
                ))
            }
            .unwrap();
            let end = unsafe {
                (*screen).pages.pin(rustty_vt::point::Point::Active(
                    rustty_vt::point::Coordinate { x: 4, y: 1 },
                ))
            }
            .unwrap();
            unsafe {
                (*screen).select(Some(rustty_vt::selection::Selection::init(
                    start, end, false,
                )))
            }
            .unwrap();

            assert_eq!(Result::Success, unsafe { update(state, terminal) });

            let mut it: RowIterator = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                row_iterator_new(Some(alloc), &mut it)
            });
            // defer row_iterator_free(it);

            let mut cells: RowCells = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                row_cells_new(Some(alloc), &mut cells)
            });
            // defer row_cells_free(cells);

            assert_eq!(Result::Success, unsafe {
                get(state, Data::RowIterator, &mut it as *mut _ as *mut c_void)
            });

            assert!(unsafe { row_iterator_next(it) });
            assert_eq!(Result::Success, unsafe {
                row_get(it, RowData::Cells, &mut cells as *mut _ as *mut c_void)
            });

            // var selected: bool = true;
            let mut selected: bool = true;
            assert_eq!(Result::Success, unsafe { row_cells_select(cells, 0) });
            assert_eq!(Result::Success, unsafe {
                row_cells_get(
                    cells,
                    RowCellsData::Selected,
                    &mut selected as *mut _ as *mut c_void,
                )
            });
            assert!(!selected);

            assert!(unsafe { row_iterator_next(it) });
            assert_eq!(Result::Success, unsafe {
                row_get(it, RowData::Cells, &mut cells as *mut _ as *mut c_void)
            });

            assert_eq!(Result::Success, unsafe { row_cells_select(cells, 1) });
            assert_eq!(Result::Success, unsafe {
                row_cells_get(
                    cells,
                    RowCellsData::Selected,
                    &mut selected as *mut _ as *mut c_void,
                )
            });
            assert!(!selected);

            assert_eq!(Result::Success, unsafe { row_cells_select(cells, 2) });
            assert_eq!(Result::Success, unsafe {
                row_cells_get(
                    cells,
                    RowCellsData::Selected,
                    &mut selected as *mut _ as *mut c_void,
                )
            });
            assert!(selected);

            assert_eq!(Result::Success, unsafe { row_cells_select(cells, 4) });
            assert_eq!(Result::Success, unsafe {
                row_cells_get(
                    cells,
                    RowCellsData::Selected,
                    &mut selected as *mut _ as *mut c_void,
                )
            });
            assert!(selected);

            assert_eq!(Result::Success, unsafe { row_cells_select(cells, 5) });
            assert_eq!(Result::Success, unsafe {
                row_cells_get(
                    cells,
                    RowCellsData::Selected,
                    &mut selected as *mut _ as *mut c_void,
                )
            });
            assert!(!selected);

            assert_eq!(Result::Success, unsafe { row_cells_select(cells, 3) });
            // selected = false;
            selected = false;
            // var written: usize = 0;
            let mut written: usize = 0;
            // const keys = [_]RowCellsData{.selected};
            let keys = [RowCellsData::Selected];
            // var values = [_]?*anyopaque{@ptrCast(&selected)};
            let mut values = [&mut selected as *mut _ as *mut c_void];
            assert_eq!(Result::Success, unsafe {
                row_cells_get_multi(
                    cells,
                    keys.len(),
                    keys.as_ptr(),
                    values.as_mut_ptr(),
                    &mut written,
                )
            });
            assert_eq!(keys.len(), written);
            assert!(selected);

            unsafe { row_cells_free(cells) };
            unsafe { row_iterator_free(it) };
        });
    });
}

// test "render: row cells get has_styling"
#[test]
fn render_row_cells_get_has_styling() {
    with_test_allocator(|alloc| {
        with_terminal_state(alloc, 10, 3, |terminal, state| {
            // const input = "A\x1b[31mB";
            let input = b"A\x1b[31mB";
            unsafe { terminal_c::vt_write(terminal, input.as_ptr(), input.len()) };

            assert_eq!(Result::Success, unsafe { update(state, terminal) });

            let mut it: RowIterator = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                row_iterator_new(Some(alloc), &mut it)
            });
            let mut cells: RowCells = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                row_cells_new(Some(alloc), &mut cells)
            });

            assert_eq!(Result::Success, unsafe {
                get(state, Data::RowIterator, &mut it as *mut _ as *mut c_void)
            });
            assert!(unsafe { row_iterator_next(it) });
            assert_eq!(Result::Success, unsafe {
                row_get(it, RowData::Cells, &mut cells as *mut _ as *mut c_void)
            });

            // var has_styling = true;
            let mut has_styling = true;
            assert_eq!(Result::Success, unsafe { row_cells_select(cells, 0) });
            assert_eq!(Result::Success, unsafe {
                row_cells_get(
                    cells,
                    RowCellsData::HasStyling,
                    &mut has_styling as *mut _ as *mut c_void,
                )
            });
            assert!(!has_styling);

            assert_eq!(Result::Success, unsafe { row_cells_select(cells, 1) });
            assert_eq!(Result::Success, unsafe {
                row_cells_get(
                    cells,
                    RowCellsData::HasStyling,
                    &mut has_styling as *mut _ as *mut c_void,
                )
            });
            assert!(has_styling);

            // has_styling = false;
            has_styling = false;
            let mut written: usize = 0;
            let keys = [RowCellsData::HasStyling];
            let mut values = [&mut has_styling as *mut _ as *mut c_void];
            assert_eq!(Result::Success, unsafe {
                row_cells_get_multi(
                    cells,
                    keys.len(),
                    keys.as_ptr(),
                    values.as_mut_ptr(),
                    &mut written,
                )
            });
            assert_eq!(keys.len(), written);
            assert!(has_styling);

            unsafe { row_cells_free(cells) };
            unsafe { row_iterator_free(it) };
        });
    });
}

// test "render: row cells get graphemes utf8"
#[test]
fn render_row_cells_get_graphemes_utf8() {
    with_test_allocator(|alloc| {
        with_terminal_state(alloc, 10, 3, |terminal, state| {
            // const input = "e\u{301}";
            let input = "e\u{301}".as_bytes();
            unsafe { terminal_c::vt_write(terminal, input.as_ptr(), input.len()) };

            assert_eq!(Result::Success, unsafe { update(state, terminal) });

            let mut it: RowIterator = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                row_iterator_new(Some(alloc), &mut it)
            });
            let mut cells: RowCells = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                row_cells_new(Some(alloc), &mut cells)
            });

            assert_eq!(Result::Success, unsafe {
                get(state, Data::RowIterator, &mut it as *mut _ as *mut c_void)
            });
            assert!(unsafe { row_iterator_next(it) });
            assert_eq!(Result::Success, unsafe {
                row_get(it, RowData::Cells, &mut cells as *mut _ as *mut c_void)
            });

            assert_eq!(Result::Success, unsafe { row_cells_select(cells, 0) });

            // var text: lib.Buffer = .{};
            let mut text: Buffer = Buffer::default();
            // try testing.expectEqual(Result.out_of_space, row_cells_get(cells, .graphemes_utf8, @ptrCast(&text)));
            assert_eq!(Result::OutOfSpace, unsafe {
                row_cells_get(
                    cells,
                    RowCellsData::GraphemesUtf8,
                    &mut text as *mut _ as *mut c_void,
                )
            });
            // try testing.expectEqual(@as(usize, input.len), text.len);
            assert_eq!(input.len(), text.len);

            // var small = [_]u8{ 'x', 'x' };
            let mut small = [b'x', b'x'];
            // text = .{ .ptr = &small, .cap = small.len };
            text = Buffer {
                ptr: small.as_mut_ptr(),
                cap: small.len(),
                len: 0,
            };
            assert_eq!(Result::OutOfSpace, unsafe {
                row_cells_get(
                    cells,
                    RowCellsData::GraphemesUtf8,
                    &mut text as *mut _ as *mut c_void,
                )
            });
            assert_eq!(input.len(), text.len);
            // try testing.expectEqualSlices(u8, &.{ 'x', 'x' }, &small);
            assert_eq!([b'x', b'x'], small);

            // var buf: [8]u8 = undefined;
            let mut buf = [0u8; 8];
            // text = .{ .ptr = &buf, .cap = buf.len };
            text = Buffer {
                ptr: buf.as_mut_ptr(),
                cap: buf.len(),
                len: 0,
            };
            assert_eq!(Result::Success, unsafe {
                row_cells_get(
                    cells,
                    RowCellsData::GraphemesUtf8,
                    &mut text as *mut _ as *mut c_void,
                )
            });
            assert_eq!(input.len(), text.len);
            // try testing.expectEqualStrings(input, buf[0..text.len]);
            assert_eq!(input, &buf[0..text.len]);

            assert_eq!(Result::Success, unsafe { row_cells_select(cells, 1) });
            text = Buffer {
                ptr: buf.as_mut_ptr(),
                cap: buf.len(),
                len: 0,
            };
            assert_eq!(Result::Success, unsafe {
                row_cells_get(
                    cells,
                    RowCellsData::GraphemesUtf8,
                    &mut text as *mut _ as *mut c_void,
                )
            });
            // try testing.expectEqual(@as(usize, 0), text.len);
            assert_eq!(0usize, text.len);

            unsafe { row_cells_free(cells) };
            unsafe { row_iterator_free(it) };
        });
    });
}

// test "render: row iterator next"
#[test]
fn render_row_iterator_next() {
    with_test_allocator(|alloc| {
        with_terminal_state(alloc, 80, 24, |terminal, state| {
            assert_eq!(Result::Success, unsafe { update(state, terminal) });

            let mut iterator: RowIterator = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                row_iterator_new(Some(alloc), &mut iterator)
            });
            // defer row_iterator_free(iterator);

            assert_eq!(Result::Success, unsafe {
                get(
                    state,
                    Data::RowIterator,
                    &mut iterator as *mut _ as *mut c_void,
                )
            });

            // const rows = state.?.state.rows;
            let rows = unsafe { (*state).state.rows };
            // if (rows == 0) { try testing.expect(!row_iterator_next(iterator)); return; }
            if rows == 0 {
                assert!(!unsafe { row_iterator_next(iterator) });
                unsafe { row_iterator_free(iterator) };
                return;
            }

            assert!(unsafe { row_iterator_next(iterator) });
            // try testing.expectEqual(@as(?size.CellCountInt, 0), iterator.?.y);
            assert_eq!(Some(0u16), unsafe { (*iterator).y });

            // var i: size.CellCountInt = 1;
            let mut i: size::CellCountInt = 1;
            // while (i < rows) : (i += 1) { ... }
            while i < rows {
                assert!(unsafe { row_iterator_next(iterator) });
                // try testing.expectEqual(@as(?size.CellCountInt, i), iterator.?.y);
                assert_eq!(Some(i), unsafe { (*iterator).y });
                i += 1;
            }

            // try testing.expect(!row_iterator_next(iterator));
            assert!(!unsafe { row_iterator_next(iterator) });
            // try testing.expectEqual(@as(?size.CellCountInt, rows - 1), iterator.?.y);
            assert_eq!(Some(rows - 1), unsafe { (*iterator).y });

            unsafe { row_iterator_free(iterator) };
        });
    });
}

// test "render: update"
#[test]
fn render_update() {
    with_test_allocator(|alloc| {
        with_terminal_state(alloc, 80, 24, |terminal, state| {
            assert_eq!(Result::Success, unsafe { update(state, terminal) });

            let input = b"hello";
            unsafe { terminal_c::vt_write(terminal, input.as_ptr(), 5) };
            assert_eq!(Result::Success, unsafe { update(state, terminal) });

            // var cols: size.CellCountInt = 0; var rows_val: size.CellCountInt = 0;
            let mut cols: size::CellCountInt = 0;
            let mut rows_val: size::CellCountInt = 0;
            assert_eq!(Result::Success, unsafe {
                get(state, Data::Cols, &mut cols as *mut _ as *mut c_void)
            });
            assert_eq!(Result::Success, unsafe {
                get(state, Data::Rows, &mut rows_val as *mut _ as *mut c_void)
            });
            // try testing.expectEqual(@as(size.CellCountInt, 80), cols);
            assert_eq!(80u16, cols);
            // try testing.expectEqual(@as(size.CellCountInt, 24), rows_val);
            assert_eq!(24u16, rows_val);
        });
    });
}

// test "render: colors get"
#[test]
fn render_colors_get() {
    with_test_allocator(|alloc| {
        with_terminal_state(alloc, 80, 24, |terminal, state| {
            assert_eq!(Result::Success, unsafe { update(state, terminal) });

            // var colors: Colors = std.mem.zeroes(Colors);
            let mut colors: Colors = unsafe { std::mem::zeroed() };
            colors.size = std::mem::size_of::<Colors>();
            assert_eq!(Result::Success, unsafe { colors_get(state, &mut colors) });

            // const state_colors = &state.?.state.colors;
            let state_colors = unsafe { &(*state).state.colors };
            // try testing.expectEqual(state_colors.background.cval(), colors.background);
            assert_eq!(state_colors.background, colors.background);
            assert_eq!(state_colors.foreground, colors.foreground);

            // if (state_colors.cursor) |cursor| { ... } else { ... }
            if let Some(cursor) = state_colors.cursor {
                assert!(colors.cursor_has_value);
                assert_eq!(cursor, colors.cursor);
            } else {
                assert!(!colors.cursor_has_value);
            }

            // for (state_colors.palette, colors.palette) |expected, actual| { try testing.expectEqual(expected.cval(), actual); }
            for (expected, actual) in state_colors.palette.iter().zip(colors.palette.iter()) {
                assert_eq!(*expected, *actual);
            }
        });
    });
}

// test "render: row cells bg_color no background"
#[test]
fn render_row_cells_bg_color_no_background() {
    with_test_allocator(|alloc| {
        with_terminal_state(alloc, 80, 24, |terminal, state| {
            // Write plain text (no background color set).
            let input = b"hello";
            unsafe { terminal_c::vt_write(terminal, input.as_ptr(), 5) };

            assert_eq!(Result::Success, unsafe { update(state, terminal) });

            let mut it: RowIterator = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                row_iterator_new(Some(alloc), &mut it)
            });
            assert_eq!(Result::Success, unsafe {
                get(state, Data::RowIterator, &mut it as *mut _ as *mut c_void)
            });
            assert!(unsafe { row_iterator_next(it) });

            let mut cells: RowCells = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                row_cells_new(Some(alloc), &mut cells)
            });

            assert_eq!(Result::Success, unsafe {
                row_get(it, RowData::Cells, &mut cells as *mut _ as *mut c_void)
            });
            assert!(unsafe { row_cells_next(cells) });

            // No background set, should return invalid_value.
            // var bg: colorpkg.RGB.C = undefined;
            let mut bg: RGB = RGB { r: 0, g: 0, b: 0 };
            assert_eq!(Result::InvalidValue, unsafe {
                row_cells_get(
                    cells,
                    RowCellsData::BgColor,
                    &mut bg as *mut _ as *mut c_void,
                )
            });

            unsafe { row_cells_free(cells) };
            unsafe { row_iterator_free(it) };
        });
    });
}

// test "render: row cells bg_color from style"
#[test]
fn render_row_cells_bg_color_from_style() {
    with_test_allocator(|alloc| {
        with_terminal_state(alloc, 80, 24, |terminal, state| {
            // Set an RGB background via SGR 48;2;R;G;B and write text.
            // terminal_c.vt_write(terminal, "\x1b[48;2;10;20;30mA", 18);
            let input = b"\x1b[48;2;10;20;30mA";
            unsafe { terminal_c::vt_write(terminal, input.as_ptr(), 18) };

            assert_eq!(Result::Success, unsafe { update(state, terminal) });

            let mut it: RowIterator = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                row_iterator_new(Some(alloc), &mut it)
            });
            assert_eq!(Result::Success, unsafe {
                get(state, Data::RowIterator, &mut it as *mut _ as *mut c_void)
            });
            assert!(unsafe { row_iterator_next(it) });

            let mut cells: RowCells = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                row_cells_new(Some(alloc), &mut cells)
            });

            assert_eq!(Result::Success, unsafe {
                row_get(it, RowData::Cells, &mut cells as *mut _ as *mut c_void)
            });
            assert!(unsafe { row_cells_next(cells) });

            let mut bg: RGB = RGB { r: 0, g: 0, b: 0 };
            assert_eq!(Result::Success, unsafe {
                row_cells_get(
                    cells,
                    RowCellsData::BgColor,
                    &mut bg as *mut _ as *mut c_void,
                )
            });
            // try testing.expectEqual(@as(u8, 10), bg.r);
            assert_eq!(10u8, bg.r);
            assert_eq!(20u8, bg.g);
            assert_eq!(30u8, bg.b);

            unsafe { row_cells_free(cells) };
            unsafe { row_iterator_free(it) };
        });
    });
}

// test "render: row cells bg_color from content tag"
#[test]
fn render_row_cells_bg_color_from_content_tag() {
    with_test_allocator(|alloc| {
        with_terminal_state(alloc, 80, 24, |terminal, state| {
            // Set an RGB background and then erase the line. The erased cells
            // should carry the background color via the content tag (bg_color_rgb)
            // rather than through the style.
            // terminal_c.vt_write(terminal, "\x1b[48;2;10;20;30m\x1b[2K", 21);
            let input = b"\x1b[48;2;10;20;30m\x1b[2K";
            unsafe { terminal_c::vt_write(terminal, input.as_ptr(), 21) };

            assert_eq!(Result::Success, unsafe { update(state, terminal) });

            let mut it: RowIterator = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                row_iterator_new(Some(alloc), &mut it)
            });
            assert_eq!(Result::Success, unsafe {
                get(state, Data::RowIterator, &mut it as *mut _ as *mut c_void)
            });
            assert!(unsafe { row_iterator_next(it) });

            let mut cells: RowCells = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                row_cells_new(Some(alloc), &mut cells)
            });

            assert_eq!(Result::Success, unsafe {
                row_get(it, RowData::Cells, &mut cells as *mut _ as *mut c_void)
            });
            assert!(unsafe { row_cells_next(cells) });

            let mut bg: RGB = RGB { r: 0, g: 0, b: 0 };
            assert_eq!(Result::Success, unsafe {
                row_cells_get(
                    cells,
                    RowCellsData::BgColor,
                    &mut bg as *mut _ as *mut c_void,
                )
            });
            assert_eq!(10u8, bg.r);
            assert_eq!(20u8, bg.g);
            assert_eq!(30u8, bg.b);

            unsafe { row_cells_free(cells) };
            unsafe { row_iterator_free(it) };
        });
    });
}

// test "render: row cells fg_color no foreground"
#[test]
fn render_row_cells_fg_color_no_foreground() {
    with_test_allocator(|alloc| {
        with_terminal_state(alloc, 80, 24, |terminal, state| {
            // Write plain text (no foreground color set).
            let input = b"hello";
            unsafe { terminal_c::vt_write(terminal, input.as_ptr(), 5) };

            assert_eq!(Result::Success, unsafe { update(state, terminal) });

            let mut it: RowIterator = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                row_iterator_new(Some(alloc), &mut it)
            });
            assert_eq!(Result::Success, unsafe {
                get(state, Data::RowIterator, &mut it as *mut _ as *mut c_void)
            });
            assert!(unsafe { row_iterator_next(it) });

            let mut cells: RowCells = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                row_cells_new(Some(alloc), &mut cells)
            });

            assert_eq!(Result::Success, unsafe {
                row_get(it, RowData::Cells, &mut cells as *mut _ as *mut c_void)
            });
            assert!(unsafe { row_cells_next(cells) });

            // No foreground set, should return invalid_value.
            let mut fg: RGB = RGB { r: 0, g: 0, b: 0 };
            assert_eq!(Result::InvalidValue, unsafe {
                row_cells_get(
                    cells,
                    RowCellsData::FgColor,
                    &mut fg as *mut _ as *mut c_void,
                )
            });

            unsafe { row_cells_free(cells) };
            unsafe { row_iterator_free(it) };
        });
    });
}

// test "render: row cells fg_color from style"
#[test]
fn render_row_cells_fg_color_from_style() {
    with_test_allocator(|alloc| {
        with_terminal_state(alloc, 80, 24, |terminal, state| {
            // Set an RGB foreground via SGR 38;2;R;G;B and write text.
            // terminal_c.vt_write(terminal, "\x1b[38;2;10;20;30mA", 18);
            let input = b"\x1b[38;2;10;20;30mA";
            unsafe { terminal_c::vt_write(terminal, input.as_ptr(), 18) };

            assert_eq!(Result::Success, unsafe { update(state, terminal) });

            let mut it: RowIterator = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                row_iterator_new(Some(alloc), &mut it)
            });
            assert_eq!(Result::Success, unsafe {
                get(state, Data::RowIterator, &mut it as *mut _ as *mut c_void)
            });
            assert!(unsafe { row_iterator_next(it) });

            let mut cells: RowCells = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                row_cells_new(Some(alloc), &mut cells)
            });

            assert_eq!(Result::Success, unsafe {
                row_get(it, RowData::Cells, &mut cells as *mut _ as *mut c_void)
            });
            assert!(unsafe { row_cells_next(cells) });

            let mut fg: RGB = RGB { r: 0, g: 0, b: 0 };
            assert_eq!(Result::Success, unsafe {
                row_cells_get(
                    cells,
                    RowCellsData::FgColor,
                    &mut fg as *mut _ as *mut c_void,
                )
            });
            assert_eq!(10u8, fg.r);
            assert_eq!(20u8, fg.g);
            assert_eq!(30u8, fg.b);

            unsafe { row_cells_free(cells) };
            unsafe { row_iterator_free(it) };
        });
    });
}

// test "render: colors get supports truncated sized struct"
#[test]
fn render_colors_get_supports_truncated_sized_struct() {
    with_test_allocator(|alloc| {
        with_terminal_state(alloc, 80, 24, |terminal, state| {
            assert_eq!(Result::Success, unsafe { update(state, terminal) });

            // var colors: Colors = std.mem.zeroes(Colors);
            let mut colors: Colors = unsafe { std::mem::zeroed() };
            // const sentinel: colorpkg.RGB.C = .{ .r = 0xAA, .g = 0xBB, .b = 0xCC };
            let sentinel: RGB = RGB {
                r: 0xAA,
                g: 0xBB,
                b: 0xCC,
            };
            // for (&colors.palette) |*entry| entry.* = sentinel;
            for entry in colors.palette.iter_mut() {
                *entry = sentinel;
            }

            // colors.size = @offsetOf(Colors, "palette") + @sizeOf(colorpkg.RGB.C) * 2;
            let palette_offset = colors_struct_type().offset_of("palette");
            colors.size = palette_offset + std::mem::size_of::<RGB>() * 2;
            assert_eq!(Result::Success, unsafe { colors_get(state, &mut colors) });

            let state_colors = unsafe { &(*state).state.colors };
            // try testing.expectEqual(state_colors.palette[0].cval(), colors.palette[0]);
            assert_eq!(state_colors.palette[0], colors.palette[0]);
            assert_eq!(state_colors.palette[1], colors.palette[1]);
            // try testing.expectEqual(sentinel, colors.palette[2]);
            assert_eq!(sentinel, colors.palette[2]);
        });
    });
}

// test "render: get_multi success"
#[test]
fn render_get_multi_success() {
    with_test_allocator(|alloc| {
        with_terminal_state(alloc, 80, 24, |terminal, state| {
            assert_eq!(Result::Success, unsafe { update(state, terminal) });

            // var cols: u16 = 0; var rows: u16 = 0; var written: usize = 0;
            let mut cols: u16 = 0;
            let mut rows: u16 = 0;
            let mut written: usize = 0;

            // const keys = [_]Data{ .cols, .rows };
            let keys = [Data::Cols, Data::Rows];
            let mut values = [
                &mut cols as *mut _ as *mut c_void,
                &mut rows as *mut _ as *mut c_void,
            ];
            assert_eq!(Result::Success, unsafe {
                get_multi(
                    state,
                    keys.len(),
                    keys.as_ptr(),
                    values.as_mut_ptr(),
                    &mut written,
                )
            });
            assert_eq!(keys.len(), written);
            assert_eq!(80, cols);
            assert_eq!(24, rows);
        });
    });
}

// test "render: get_multi null returns invalid_value"
#[test]
fn render_get_multi_null_returns_invalid_value() {
    // var cols: u16 = 0;
    let mut cols: u16 = 0;
    // var values = [_]?*anyopaque{@ptrCast(&cols)};
    let mut values = [&mut cols as *mut _ as *mut c_void];
    // try testing.expectEqual(Result.invalid_value, get_multi(null, 1, null, &values, null));
    assert_eq!(Result::InvalidValue, unsafe {
        get_multi(
            std::ptr::null_mut(),
            1,
            std::ptr::null(),
            values.as_mut_ptr(),
            std::ptr::null_mut(),
        )
    });
}

// test "render: row_get_multi success"
#[test]
fn render_row_get_multi_success() {
    with_test_allocator(|alloc| {
        with_terminal_state(alloc, 80, 24, |terminal, state| {
            assert_eq!(Result::Success, unsafe { update(state, terminal) });

            let mut it: RowIterator = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                row_iterator_new(Some(alloc), &mut it)
            });

            assert_eq!(Result::Success, unsafe {
                get(state, Data::RowIterator, &mut it as *mut _ as *mut c_void)
            });
            assert!(unsafe { row_iterator_next(it) });

            // var dirty: bool = true; var written: usize = 0;
            let mut dirty: bool = true;
            let mut written: usize = 0;

            // const keys = [_]RowData{.dirty};
            let keys = [RowData::Dirty];
            let mut values = [&mut dirty as *mut _ as *mut c_void];
            assert_eq!(Result::Success, unsafe {
                row_get_multi(
                    it,
                    keys.len(),
                    keys.as_ptr(),
                    values.as_mut_ptr(),
                    &mut written,
                )
            });
            assert_eq!(keys.len(), written);

            unsafe { row_iterator_free(it) };
        });
    });
}

// test "render: row_get_multi null returns invalid_value"
#[test]
fn render_row_get_multi_null_returns_invalid_value() {
    // var dirty: bool = false;
    let mut dirty: bool = false;
    let mut values = [&mut dirty as *mut _ as *mut c_void];
    assert_eq!(Result::InvalidValue, unsafe {
        row_get_multi(
            std::ptr::null_mut(),
            1,
            std::ptr::null(),
            values.as_mut_ptr(),
            std::ptr::null_mut(),
        )
    });
}

// test "render: row_cells_get_multi success"
#[test]
fn render_row_cells_get_multi_success() {
    with_test_allocator(|alloc| {
        with_terminal_state(alloc, 80, 24, |terminal, state| {
            let input = b"A";
            unsafe { terminal_c::vt_write(terminal, input.as_ptr(), 1) };

            assert_eq!(Result::Success, unsafe { update(state, terminal) });

            let mut it: RowIterator = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                row_iterator_new(Some(alloc), &mut it)
            });

            assert_eq!(Result::Success, unsafe {
                get(state, Data::RowIterator, &mut it as *mut _ as *mut c_void)
            });
            assert!(unsafe { row_iterator_next(it) });

            let mut cells: RowCells = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                row_cells_new(Some(alloc), &mut cells)
            });

            assert_eq!(Result::Success, unsafe {
                row_get(it, RowData::Cells, &mut cells as *mut _ as *mut c_void)
            });
            assert!(unsafe { row_cells_next(cells) });

            // var raw: row.CRow = undefined; var written: usize = 0;
            let mut raw: row::CRow = 0;
            let mut written: usize = 0;

            // const keys = [_]RowCellsData{.raw};
            let keys = [RowCellsData::Raw];
            let mut values = [&mut raw as *mut _ as *mut c_void];
            assert_eq!(Result::Success, unsafe {
                row_cells_get_multi(
                    cells,
                    keys.len(),
                    keys.as_ptr(),
                    values.as_mut_ptr(),
                    &mut written,
                )
            });
            assert_eq!(keys.len(), written);

            unsafe { row_cells_free(cells) };
            unsafe { row_iterator_free(it) };
        });
    });
}

// test "render: row_cells_get_multi null returns invalid_value"
#[test]
fn render_row_cells_get_multi_null_returns_invalid_value() {
    // var raw: row.CRow = undefined;
    let mut raw: row::CRow = 0;
    let mut values = [&mut raw as *mut _ as *mut c_void];
    assert_eq!(Result::InvalidValue, unsafe {
        row_cells_get_multi(
            std::ptr::null_mut(),
            1,
            std::ptr::null(),
            values.as_mut_ptr(),
            std::ptr::null_mut(),
        )
    });
}
